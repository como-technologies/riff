//! The claims of the sessions of the daemon on the forge
//! (01M519Z4CT0PRXPRM22037Z5RY, 01M519Z4HK333JWVXAWQHCGK6M,
//! 01M519Z4M0F2RY0VDQNB2QFAPJ, 01M519Z4PESWFV8Y03SKTSGV4Q).
//!
//! # Design
//!
//! A session with no riff-server holds its item with a claim on the
//! forge ([`crate::forge_claim`]). The daemon makes the claim
//! ([`Daemon::claim_forge`]) in the main clone of the repository, and
//! keeps the claim commit in the [`Record`] of the session ([`Forge`]).
//! The daemon knows the main clone of each repository where it made a
//! claim: the file [`FORGES`] in its folder, and the main clone of each
//! of its waves ([`crate::wave`]).
//!
//! | When | The daemon |
//! |---|---|
//! | A claim | writes the main clone to [`FORGES`], makes the claim commit, and pushes its ref with the lease of no ref |
//! | The end of a session | releases the claim of the session with the lease of its commit, then writes the end of the session |
//! | Its start, with its riff and its sessions | releases each claim whose commit names its key and whose session does not run ([`Daemon::sweep_forges`]) |
//! | Each minute, while a session of the daemon is the lead of its user | deletes each claim of that user that [`forge_claim::stale`] gives: its session ended, or 15 minutes with no link to its daemon ([`Daemon::tidy_forges`]) |
//! | `riff release ITEM --forge` ([`Ask::Release`]) | deletes the claim when [`forge_claim::may_free`] lets it ([`Daemon::release_asked`]) |
//!
//! ```mermaid
//! flowchart TD
//!     C["a claim on the forge"] --> E{"its session"}
//!     E -- "ends" --> R["its daemon deletes the claim"]
//!     E -- "does not run after a new start of its daemon" --> R
//!     E -- "ended, or its daemon has no link for 15 minutes" --> L["the daemon of the lead of the same user deletes the claim"]
//!     E -- "of another user, and its daemon is offline" --> H["the claim holds: riff release --forge frees it"]
//! ```
//!
//! Each step of git can block, so it runs on a thread for blocking
//! work. A fault of the forge at one main clone stops no other step: the
//! daemon logs it and goes on.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use riff_core::name::SessionUri;
use serde::{Deserialize, Serialize};

use super::{Daemon, Record, now_ms};
use crate::daemon_key::Public;
use crate::forge_claim::{self, Claim, Freed, Link, Took};
use crate::runtime::Runtime;
use crate::text;

/// The file of the main clones where the daemon made a claim on the
/// forge, by repository, in the folder of the daemon.
pub const FORGES: &str = "forges.json";

/// How often the daemon of a lead looks at the claims of its user.
pub const TIDY_EVERY: Duration = Duration::from_secs(60);

/// The claim of the item of a session on the forge, in the [`Record`]
/// of the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forge {
    /// The main clone of the repository: the daemon claims and releases
    /// there.
    pub main: PathBuf,
    /// The claim commit: the lease of the release.
    pub commit: String,
}

/// Runs `work` on a thread for blocking work: git.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| anyhow!("{e}"))?
}

/// The main clones in the file [`FORGES`] of the folder `dir`, by
/// repository. With no file, none.
pub fn read_forges(dir: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let path = dir.join(FORGES);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("cannot read the main clones in {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Adds the main clone `main` of the repository `repo` to the file
/// [`FORGES`] of the folder `dir`, in place of the old file.
fn add_forge(dir: &Path, repo: &str, main: &Path) -> Result<()> {
    let mut forges = read_forges(dir)?;
    if forges.get(repo).map(PathBuf::as_path) == Some(main) {
        return Ok(());
    }
    forges.insert(repo.to_owned(), main.to_owned());
    let path = dir.join(FORGES);
    let tmp = path.with_extension("json.new");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&forges)?)
        .with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))
}

/// The time of each link of the daemon: the last time that the daemon
/// saw a link to each key ([`Daemon::tidy_forges`]).
#[derive(Debug, Default)]
pub struct Seen {
    /// The last time that a link to each key ran.
    up: BTreeMap<Public, i64>,
}

impl Seen {
    /// Notes that a link to each key of `linked` runs at `now_ms`.
    pub fn note(&mut self, linked: &BTreeSet<Public>, now_ms: i64) {
        for key in linked {
            self.up.insert(*key, now_ms);
        }
    }

    /// The link to the daemon of `key`, with the keys `linked` that have
    /// a link now.
    pub fn link(&self, key: &Public, linked: &BTreeSet<Public>) -> Link {
        Link {
            online: linked.contains(key),
            seen_ms: self.up.get(key).copied(),
        }
    }
}

impl<R: Runtime + 'static> Daemon<R> {
    /// Claims `item` for the session `me` on the forge of the main clone
    /// `main` (01M519Z4CT0PRXPRM22037Z5RY): the claim commit names the
    /// user of the key of the daemon in its riff of peers, the session,
    /// the key and the time. It gives the claim, or `None` when another
    /// session holds the item. It fails while the daemon has no riff.
    pub async fn claim_forge(
        &self,
        main: &Path,
        item: &str,
        me: &SessionUri,
    ) -> Result<Option<Forge>> {
        let riff = self.riff.get().context(text::DAEMON_NO_RIFF_YET)?;
        let peer = riff.hub.peer(me);
        let session = me
            .who()
            .session()
            .context("a claim needs a session ID")?;
        let claim = Claim::now(item, peer.who().user(), session, riff.hub.public());
        // The daemon knows the main clone before the push: a claim after
        // a crash goes at the next start.
        add_forge(&self.dir, &me.place().repo_text(), main)?;
        let at = main.to_owned();
        let took = blocking(move || forge_claim::take(&at, &claim)).await?;
        Ok(match took {
            Took::Won(commit) => {
                println!("{}", text::forge_claimed(item, &commit));
                Some(Forge {
                    main: main.to_owned(),
                    commit,
                })
            }
            Took::Held => {
                println!("{}", text::forge_held(item));
                None
            }
        })
    }

    /// The main clone of each repository that the daemon knows: of
    /// [`FORGES`], and of each of its waves.
    pub fn forges(&self) -> BTreeMap<String, PathBuf> {
        let mut forges = read_forges(&self.dir).unwrap_or_else(|e| {
            eprintln!("riff daemon: {e:#}");
            BTreeMap::new()
        });
        if let Ok(waves) = crate::wave::read_waves(&self.dir) {
            for (repo, wave) in waves {
                forges.entry(repo).or_insert(wave.main);
            }
        }
        forges
    }

    /// At the start of the daemon, with its riff and after it took up its
    /// sessions ([`Daemon::recover`]): releases each claim on each forge
    /// that it knows whose commit names its key and whose session does
    /// not run (01M519Z4HK333JWVXAWQHCGK6M). It gives a line for each
    /// release.
    pub async fn sweep_forges(&self) -> Vec<String> {
        let Some(riff) = self.riff.get() else {
            return Vec::new();
        };
        let me = riff.hub.public();
        let running: BTreeSet<String> = self.records().into_iter().map(|r| r.live.session).collect();
        let mut lines = Vec::new();
        for (repo, main) in self.forges() {
            let running = running.clone();
            let swept = blocking(move || {
                let mut lines = Vec::new();
                for found in forge_claim::found(&main)? {
                    let Some(claim) = found.claim else { continue };
                    if claim.key != me || running.contains(&claim.session) {
                        continue;
                    }
                    if forge_claim::release(&main, &found.item, &found.commit)? == Freed::Deleted {
                        lines.push(text::forge_swept(&found.item, &claim.session));
                    }
                }
                Ok(lines)
            })
            .await;
            match swept {
                Ok(swept) => lines.extend(swept),
                Err(e) => eprintln!("riff daemon: {repo}: {e:#}"),
            }
        }
        lines
    }

    /// The step of the lead (01M519Z4M0F2RY0VDQNB2QFAPJ): for each session
    /// of the daemon that is the lead of its user in its repository, it
    /// deletes each claim of that user on the forge of the repository
    /// that [`forge_claim::stale`] gives, with the lease of its commit.
    /// `seen` holds the time of each link, and `now_ms` is the time now.
    /// It gives a line for each claim that it deleted.
    pub async fn tidy_forges(&self, seen: &mut Seen, now_ms: i64) -> Vec<String> {
        let Some(riff) = self.riff.get() else {
            return Vec::new();
        };
        let me = riff.hub.public();
        let linked = self.links().map(|links| links.linked()).unwrap_or_default();
        seen.note(&linked, now_ms);
        let leads: BTreeSet<(String, String)> = riff.hub.read(|state| {
            state
                .runs()
                .filter(|run| run.author == me && state.is_lead(run))
                .map(|run| {
                    let session = &run.body.session;
                    (session.who().user().to_owned(), session.place().repo_text())
                })
                .collect()
        });
        let forges = self.forges();
        let mut lines = Vec::new();
        for (user, repo) in leads {
            let Some(main) = forges.get(&repo).cloned() else {
                continue;
            };
            let at = main.clone();
            let found = match blocking(move || forge_claim::found(&at)).await {
                Ok(found) => found,
                Err(e) => {
                    eprintln!("riff daemon: {repo}: {e:#}");
                    continue;
                }
            };
            let stale: Vec<(String, String, String, forge_claim::Stale)> = riff.hub.read(|state| {
                found
                    .into_iter()
                    .filter_map(|found| {
                        let claim = found.claim?;
                        let link = seen.link(&claim.key, &linked);
                        let why =
                            forge_claim::stale(&claim, &user, &me, state, link, self.since_ms, now_ms)?;
                        Some((found.item, found.commit, claim.session, why))
                    })
                    .collect()
            });
            for (item, commit, session, why) in stale {
                let at = main.clone();
                let (of, lease) = (item.clone(), commit.clone());
                match blocking(move || forge_claim::release(&at, &of, &lease)).await {
                    Ok(Freed::Deleted) => lines.push(text::forge_lead_freed(&item, &session, why)),
                    Ok(Freed::NotAt) => {}
                    Err(e) => eprintln!("riff daemon: {repo}: {e:#}"),
                }
            }
        }
        lines
    }

    /// Runs the steps of the claims on the forge until the daemon stops:
    /// when the daemon has its riff, [`Daemon::sweep_forges`] one time,
    /// then [`Daemon::tidy_forges`] each [`TIDY_EVERY`]. Call it after
    /// [`Daemon::recover`].
    pub fn watch_forges(self: &std::sync::Arc<Self>) {
        let daemon = self.clone();
        tokio::spawn(async move {
            while daemon.riff.get().is_none() {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            for line in daemon.sweep_forges().await {
                println!("{line}");
            }
            let mut seen = Seen::default();
            loop {
                for line in daemon.tidy_forges(&mut seen, now_ms()).await {
                    println!("{line}");
                }
                tokio::time::sleep(TIDY_EVERY).await;
            }
        });
    }

    /// `riff release ITEM --forge` (01M519Z4PESWFV8Y03SKTSGV4Q): deletes
    /// the claim of `item` on the forge of the main clone `main`, with
    /// the lease of its claim commit, when [`forge_claim::may_free`] lets
    /// the daemon. It gives the line for the person.
    pub async fn release_asked(&self, main: PathBuf, item: String) -> Result<String> {
        let riff = self.riff.get().context(text::DAEMON_NO_RIFF_YET)?;
        let hub = riff.hub.clone();
        blocking(move || {
            forge_claim::reference(&item)?;
            let Some(commit) = forge_claim::list(&main)?.remove(&item) else {
                bail!("{}", text::forge_release_none(&item));
            };
            let claim = forge_claim::facts(&main, &item, &commit)?;
            hub.read(|state| forge_claim::may_free(state, &hub.public(), claim.as_ref()))
                .map_err(|why| anyhow!("{why}"))?;
            match forge_claim::release(&main, &item, &commit)? {
                Freed::Deleted => Ok(text::forge_released(
                    &item,
                    claim.as_ref().map(|c| c.user.as_str()),
                )),
                Freed::NotAt => bail!("{}", text::forge_release_changed(&item)),
            }
        })
        .await
    }
}

/// Releases the claim of the session of `record` on the forge, at the
/// end of the session (01M519Z4HK333JWVXAWQHCGK6M). It blocks: call it on
/// a thread for blocking work.
pub(super) fn release_at_end(record: &Record) {
    let Some(claim) = &record.claim else {
        return;
    };
    let item = &record.live.item;
    match forge_claim::release(&claim.main, item, &claim.commit) {
        Ok(Freed::Deleted) => println!(
            "{}",
            text::forge_released_at_end(item, &record.live.session)
        ),
        Ok(Freed::NotAt) => println!("{}", text::forge_gone_at_end(item)),
        Err(e) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::tests::{Here, IMAGE, me, until};
    use crate::daemon::{Ask, Client, Say, ask_release};
    use crate::daemon_key::Key;
    use crate::forge_claim::tests::clone_of;
    use crate::runtime::Fake;

    async fn here() -> Here {
        Here::new(Fake::default().with_image(IMAGE)).await
    }

    /// The items of the claims on the forge of `main`.
    fn items(main: &Path) -> BTreeSet<String> {
        forge_claim::list(main).unwrap().into_keys().collect()
    }

    /// The set of the items `items`.
    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    /// The user of the key of the daemon of `here`: the owner of its own
    /// riff.
    fn user_of(here: &Here) -> String {
        here.daemon
            .peers(|state| state.owner().map(str::to_owned))
            .flatten()
            .unwrap()
    }

    /// Done when 1 of #844: two daemons claim one item at one time, and
    /// one wins. The other reads it as held. A release with a wrong lease
    /// fails, and the release of the daemon at the end of the session
    /// deletes the ref with the lease (01M4Z60B8EB7A44A41CWWP7G8K,
    /// 01M519Z4HK333JWVXAWQHCGK6M).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn two_daemons_claim_one_item_at_one_time_and_one_wins() {
        let (a, b) = (here().await, here().await);
        let root = tempfile::tempdir().unwrap();
        let mains = [clone_of(root.path(), "a"), clone_of(root.path(), "b")];
        let (me_a, me_b) = (me("wa"), me("wb"));
        let (won_a, won_b) = tokio::join!(
            a.daemon.claim_forge(&mains[0], "issue-12", &me_a),
            b.daemon.claim_forge(&mains[1], "issue-12", &me_b),
        );
        let (winner, name, won, lost_at) = match (won_a.unwrap(), won_b.unwrap()) {
            (Some(won), None) => (&a, "wa", won, &mains[1]),
            (None, Some(won)) => (&b, "wb", won, &mains[0]),
            other => panic!("one daemon wins: {other:?}"),
        };
        // The other daemon reads the claim of the winner.
        let found = forge_claim::found(lost_at).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].commit, won.commit);
        let claim = found[0].claim.clone().unwrap();
        assert_eq!(Some(claim.key), winner.daemon.peers(|s| s.riff()));
        assert_eq!(claim.user, user_of(winner));
        assert_eq!(claim.session, format!("{name}-session"));
        // A release with a wrong lease fails, and changes nothing.
        let wrong = forge_claim::commit(lost_at, &claim).unwrap();
        assert_eq!(
            forge_claim::release(lost_at, "issue-12", &wrong).unwrap(),
            Freed::NotAt
        );
        assert_eq!(items(lost_at), set(&["issue-12"]));

        // The session of the winner holds the claim. At its end, its
        // daemon deletes the ref with the lease.
        let mut start = winner.start(name, "issue-12");
        start.server = String::new();
        start.claim = Some(won);
        let (_client, name) = winner.started(start).await;
        assert_eq!(items(lost_at), set(&["issue-12"]));
        winner.fake.stop(&name).await.unwrap();
        let main = lost_at.clone();
        until(move || items(&main).is_empty()).await;
    }

    /// 01M519Z4HK333JWVXAWQHCGK6M: at its start, a daemon deletes each
    /// claim whose commit names its key and whose session does not run.
    /// The claim of a session that runs, and a claim of another daemon,
    /// stay.
    #[tokio::test]
    async fn at_its_start_a_daemon_deletes_each_claim_of_its_key_whose_session_does_not_run() {
        let (a, b) = (here().await, here().await);
        let root = tempfile::tempdir().unwrap();
        let (main, other) = (clone_of(root.path(), "a"), clone_of(root.path(), "b"));
        let gone = a.daemon.claim_forge(&main, "issue-12", &me("gone")).await;
        assert!(gone.unwrap().is_some());
        let runs = a.daemon.claim_forge(&main, "issue-13", &me("runs")).await;
        let mut start = a.start("runs", "issue-13");
        start.server = String::new();
        start.claim = runs.unwrap();
        let (_client, _) = a.started(start).await;
        let of_b = b.daemon.claim_forge(&other, "issue-14", &me("of-b")).await;
        assert!(of_b.unwrap().is_some());
        assert_eq!(
            read_forges(&a.dir).unwrap(),
            BTreeMap::from([("como-technologies/riff".to_owned(), main.clone())])
        );

        let lines = a.daemon.sweep_forges().await;
        assert_eq!(lines, [text::forge_swept("issue-12", "gone-session")]);
        assert_eq!(items(&main), set(&["issue-13", "issue-14"]));
        assert!(a.daemon.sweep_forges().await.is_empty());
    }

    /// Done when 3 of #844 (01M519Z4M0F2RY0VDQNB2QFAPJ): the daemon of the
    /// lead deletes a claim of a session of its user after the end
    /// record, and after 15 minutes with no link to its daemon, on a test
    /// clock. A claim of another user holds, and so does a claim of a
    /// session of this daemon that runs.
    #[tokio::test]
    async fn the_daemon_of_the_lead_deletes_the_claims_of_its_user_that_ended_or_are_offline() {
        let a = here().await;
        let root = tempfile::tempdir().unwrap();
        let main = clone_of(root.path(), "a");
        let user = user_of(&a);
        let hub = a.daemon.riff.get().unwrap().hub.clone();
        let lead: SessionUri = "riff://mike@thelio/como-technologies/riff?session=lead1"
            .parse()
            .unwrap();
        hub.start(&lead, false, true, Vec::new()).unwrap();
        // A session of this daemon that ended, and one that runs.
        hub.start(&me("e1"), true, false, vec!["issue-1".into()])
            .unwrap();
        let ended = a.daemon.claim_forge(&main, "issue-1", &me("e1")).await;
        assert!(ended.unwrap().is_some());
        hub.end("e1-session").unwrap();
        hub.start(&me("r1"), true, false, vec!["issue-4".into()])
            .unwrap();
        let runs = a.daemon.claim_forge(&main, "issue-4", &me("r1")).await;
        assert!(runs.unwrap().is_some());
        // The claims of the daemon of another key: of the user of the
        // lead, and of another user.
        let other = Key::from_bytes(&[9; 32]).public();
        let take = |item: &str, user: &str| {
            let claim = Claim::now(item, user, "x1", other);
            let took = forge_claim::take(&main, &claim).unwrap();
            assert!(matches!(took, Took::Won(_)), "{took:?}");
        };
        take("issue-2", &user);
        take("issue-3", "brett");

        let since = a.daemon.since_ms;
        let minutes = |m: i64| since + m * 60_000;
        let mut seen = Seen::default();
        let lines = a.daemon.tidy_forges(&mut seen, minutes(14)).await;
        let ended = forge_claim::Stale::Ended;
        assert_eq!(lines, [text::forge_lead_freed("issue-1", "e1-session", ended)]);
        assert_eq!(items(&main), set(&["issue-2", "issue-3", "issue-4"]));
        let lines = a.daemon.tidy_forges(&mut seen, minutes(15)).await;
        let offline = forge_claim::Stale::Offline(15);
        assert_eq!(lines, [text::forge_lead_freed("issue-2", "x1", offline)]);
        assert_eq!(items(&main), set(&["issue-3", "issue-4"]));
        // With no lead, the daemon deletes no claim.
        take("issue-5", &user);
        hub.end("lead1").unwrap();
        assert!(a.daemon.tidy_forges(&mut seen, minutes(99)).await.is_empty());
        assert_eq!(items(&main), set(&["issue-3", "issue-4", "issue-5"]));
    }

    /// Done when 4 of #844 (01M519Z4PESWFV8Y03SKTSGV4Q): `riff release
    /// ITEM --forge` asks the daemon. The owner frees a claim of its own
    /// user and of another user. The rule of a member is in the tests of
    /// [`crate::forge_claim`].
    #[tokio::test]
    async fn riff_release_forge_asks_the_daemon_and_the_owner_frees_each_claim() {
        let a = here().await;
        let root = tempfile::tempdir().unwrap();
        let main = clone_of(root.path(), "a");
        let user = user_of(&a);
        let other = Key::from_bytes(&[9; 32]).public();
        for (item, user) in [("issue-5", user.as_str()), ("issue-6", "brett")] {
            forge_claim::take(&main, &Claim::now(item, user, "x1", other)).unwrap();
        }
        ask_release(&a.dir, &main, "issue-5").await.unwrap();
        assert_eq!(items(&main), set(&["issue-6"]));
        ask_release(&a.dir, &main, "issue-6").await.unwrap();
        assert!(items(&main).is_empty());
        let error = ask_release(&a.dir, &main, "issue-7").await.unwrap_err();
        assert_eq!(error.to_string(), text::forge_release_none("issue-7"));
        let error = ask_release(&a.dir, &main, "../x").await.unwrap_err();
        assert_eq!(error.to_string(), text::forge_claim_bad_item("../x"));
        // The line of the daemon names the item and the user.
        forge_claim::take(&main, &Claim::now("issue-8", "brett", "x2", other)).unwrap();
        let mut client = Client::connect(&a.dir).await.unwrap();
        let ask = Ask::Release {
            main: main.clone(),
            item: "issue-8".into(),
        };
        client.ask(ask).await.unwrap();
        let line = text::forge_released("issue-8", Some("brett"));
        assert_eq!(client.next().await.unwrap(), Some(Say::Line { text: line }));
        assert_eq!(client.next().await.unwrap(), Some(Say::Done));
    }
}
