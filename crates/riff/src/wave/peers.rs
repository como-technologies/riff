//! The waves of the daemon and `riff run --peers` with no riff-server
//! (01M519Z4RWHWP3TKNP4T5GKKE9): [`Peers`], the [`World`] of a wave whose
//! riff-server is empty.
//!
//! # Design
//!
//! A job of the wave does each step of [`Live`] with the forge and git.
//! Each riff step goes to the daemon, and not to riff-server:
//!
//! | Step | [`Live`]: riff-server 1.x | [`Peers`]: the riff of peers |
//! |---|---|---|
//! | The pause and the holds of the look | `GET` of the pauses and of the plan | the `plan` records ([`look_in`]) |
//! | The claims of the look | the claims of the sessions in `who` | `git ls-remote origin 'refs/riff/claims/*'` (01M519Z4F7Z87BXQYT7F8EABCN) |
//! | The session | a worker session, and its keep-alive | the `session` records of the daemon |
//! | The claim | a claim on riff-server | a claim on the forge ([`crate::daemon::forge`]) |
//! | The verify request, the note and the result of a verify | a post | a `message` record to the lead (`EndTalk::peers`) |
//! | The release | a release on riff-server, at the end of the steps | the delete of the claim on the forge, at the end of the steps |
//!
//! The user of a session is the user of the key of the daemon
//! (01M51T470TNWVFF97E474FV7BY). The daemon picks the world of each wave
//! and of each session by its riff-server: empty is the riff of peers
//! (`Daemon::world_of`).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as the daemon: a job of the wave
//!     participant G as the forge
//!     participant P as the riff of peers
//!     participant C as the session
//!     D->>G: git ls-remote: the claims; gh: the free work
//!     D->>P: the pause and the holds
//!     D->>G: the claim: refs/riff/claims/ITEM
//!     D->>P: the session record with the claim
//!     D->>C: the clone and the start text
//!     C->>G: push the branch, open the pull request
//!     C-->>D: the end
//!     D->>P: the end record of the session
//!     D->>P: the verify request to the lead: a message record
//!     D->>G: the release: the delete of the ref with its lease
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use riff_core::name::{Place, SessionUri, ThreadName, Who};
use riff_core::record::By;
use riff_core::selector::Selector;
use riff_core::wire::{HoldInfo, Kind};

use super::{
    BoxFuture, EndTalk, Live, Look, Wave, Work, World, after_build, after_verify, blocking,
    free_work, lock,
};
use crate::daemon::{Ended, Record, Start};
use crate::peer::state::State;
use crate::peer::talk::Hub;
use crate::talk::Talk;
use crate::{forge_claim, pr, run, terminal, text};

/// The holds of the repository `thread` in the riff of peers `state`, by
/// item, as riff-server gives them: each hold that no free ends.
///
/// ```
/// use riff::daemon_key::Key;
/// use riff::peer::state::State;
/// use riff::wave::peers::holds_in;
///
/// let state = State::new(Key::from_bytes(&[1; 32]).public());
/// assert!(holds_in(&state, &"o/r".parse()?).is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn holds_in(state: &State, thread: &ThreadName) -> BTreeMap<String, HoldInfo> {
    state
        .holds()
        .filter(|hold| hold.thread.as_ref() == Some(thread))
        .filter_map(|hold| {
            let item = hold.item.clone()?;
            let info = HoldInfo {
                reason: hold.reason.clone(),
                by: Some(By::Person(hold.user.clone())),
                at_ms: hold.time_ms,
            };
            Some((item, info))
        })
        .collect()
}

/// The pause and the holds of the repository `thread` in the riff of
/// peers `state`, for the look of a wave: true when a pause of the
/// repository or of the whole riff stops it, and the held items.
pub fn look_in(state: &State, thread: &ThreadName) -> (bool, BTreeSet<String>) {
    let paused = state.pauses_of(thread).next().is_some();
    (paused, holds_in(state, thread).into_keys().collect())
}

/// The forge and the riff of the waves with no riff-server: `gh` and git
/// for the forge, and the riff of peers of the daemon. See the design of
/// the module.
pub struct Peers {
    /// The riff of peers of the daemon.
    hub: Arc<Hub>,
    /// The folder of the daemon: the main clones of its claims
    /// ([`crate::daemon::forge::FORGES`]).
    dir: PathBuf,
    /// The steps that do not talk: the image, the clone and the sweep.
    live: Live,
    /// The last text that the lead of each wave got: a text goes one
    /// time in a row.
    told: Mutex<HashMap<String, String>>,
}

impl Peers {
    /// The world of the waves of the daemon in the folder `dir`, with the
    /// riff of peers `hub`.
    pub fn new(hub: Arc<Hub>, dir: PathBuf) -> Peers {
        Peers {
            hub,
            dir,
            live: Live::default(),
            told: Mutex::new(HashMap::new()),
        }
    }

    /// The user of the key of the daemon in its riff.
    fn user(&self) -> Result<String> {
        self.hub
            .read(|state| state.user_of(&self.hub.public()).map(str::to_owned))
            .context(text::DAEMON_NO_RIFF_YET)
    }

    /// The URI of the session `session` of the user of the key of the
    /// daemon at `place`.
    fn uri(&self, place: Place, session: Option<&str>) -> Result<SessionUri> {
        Ok(SessionUri::new(Who::new(&self.user()?, session)?, place))
    }
}

impl World for Peers {
    fn look<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Look>> {
        Box::pin(async move {
            let thread = wave.thread()?;
            let (paused, holds) = self.hub.read(|state| look_in(state, &thread));
            if paused {
                return Ok(Look {
                    paused: true,
                    ..Look::default()
                });
            }
            let main = wave.main.clone();
            let claims: BTreeSet<String> =
                blocking(move || Ok(forge_claim::list(&main)?.into_keys().collect())).await?;
            let (free, pulls) = free_work(&wave.repo, &claims, &holds).await?;
            Ok(Look {
                paused: false,
                free,
                claims,
                holds,
                pulls,
            })
        })
    }

    fn image<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<String>> {
        self.live.image(wave)
    }

    fn begin<'a>(
        &'a self,
        wave: &'a Wave,
        work: &'a Work,
        image: &'a str,
    ) -> BoxFuture<'a, Result<Option<Start>>> {
        Box::pin(async move {
            let item = work.item();
            let main = wave.main.clone();
            let host = blocking(move || run::host_facts(&main)).await?;
            let milestone = match work {
                Work::Build { issue } => {
                    let (repo, issue) = (wave.repo.clone(), *issue);
                    Some(blocking(move || run::milestone(&pr::Gh::for_repo(&repo), issue)).await?)
                }
                Work::Verify { .. } => None,
            };
            let session = terminal::new_session_id();
            let me = self.uri(wave.place(Some(&item))?, Some(&session))?;
            let claimed =
                crate::daemon::forge::claim(&self.dir, &self.hub, &wave.main, &item, &me).await?;
            let Some(claim) = claimed else {
                return Ok(None);
            };
            println!("{}", text::wave_claimed(&wave.repo, &item, &session));
            match Live::prepare(wave, work, image, &host, milestone, &session, &me).await {
                Ok(mut start) => {
                    start.claim = Some(claim);
                    Ok(Some(start))
                }
                Err(e) => {
                    let (main, commit) = (claim.main, claim.commit);
                    let freed = blocking(move || forge_claim::release(&main, &item, &commit)).await;
                    if let Err(e) = freed {
                        eprintln!("riff daemon: {e:#}");
                    }
                    Err(e)
                }
            }
        })
    }

    /// A session of the riff of peers has no keep-alive.
    fn resume(&self, _: &Record) {}

    fn finish<'a>(
        &'a self,
        record: &'a Record,
        ended: Ended,
    ) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            let job = record
                .job
                .as_ref()
                .ok_or_else(|| anyhow!("the session {} has no job", record.live.name))?;
            let wave = &job.wave;
            println!(
                "{}",
                text::wave_ended(&wave.repo, &record.live.item, ended.code, ended.failed)
            );
            let riff = EndTalk::peers(&self.hub);
            let done = match &job.work {
                Work::Build { issue } => {
                    let gh = pr::Gh::for_repo(&wave.repo);
                    after_build(gh, &riff, record, wave, *issue).await
                }
                Work::Verify { pull, commit, .. } => {
                    after_verify(&riff, record, wave, *pull, commit).await
                }
            };
            // The verify request comes before the release
            // (01M519Z4RWHWP3TKNP4T5GKKE9, 01M526Z1WH3865PKGFPYPFWQT7).
            let ending = record.clone();
            let released = blocking(move || Ok(crate::daemon::forge::release_at_end(&ending)))
                .await
                .unwrap_or_default();
            let mut lines = done?;
            lines.extend(released);
            Ok(lines)
        })
    }

    fn sweep<'a>(&'a self, wave: &'a Wave, keep: &'a [PathBuf]) -> BoxFuture<'a, Result<()>> {
        self.live.sweep(wave, keep)
    }

    fn tell<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if lock(&self.told).get(&wave.repo).is_some_and(|t| t == body) {
                return;
            }
            let posted = async {
                let person = self.uri(wave.place(None)?, None)?;
                let to = [Selector::lead(person.who().user(), &wave.repo)];
                let talk = crate::peer::talk::Session::new(self.hub.clone());
                talk.post(&person, Some(&wave.thread()?), &to, body, Kind::Message)
                    .await?;
                Ok::<(), anyhow::Error>(())
            };
            match posted.await {
                Ok(()) => {
                    lock(&self.told).insert(wave.repo.clone(), body.to_owned());
                }
                Err(e) => eprintln!("riff daemon: {}: {e:#}", wave.repo),
            }
        })
    }

    /// A session of the riff of peers has no session on riff-server: the
    /// daemon writes its end record ([`crate::daemon`]).
    fn leave<'a>(&'a self, _: &'a Record) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    /// The daemon wakes a session of the riff of peers itself
    /// ([`Hub::wait`]).
    fn wake<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move { bail!("{}", text::peers_no_server_wake(&record.live.name)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_key::Key;
    use crate::peer::body::{Body, Plan, Riff};
    use crate::peer::record::{Id, Record as Signed, check};

    /// The state of the riff of the owner (mike) with the `plan` records
    /// `plans`.
    fn riff_with(plans: &[Plan]) -> State {
        let owner = Key::from_bytes(&[1; 32]);
        let mut state = State::new(owner.public());
        let mut bodies = vec![Body::Riff(Riff {
            user: "mike".into(),
            host: None,
        })];
        bodies.extend(plans.iter().cloned().map(Body::Plan));
        let mut before = Id::ZERO;
        for (seq, body) in (1..).zip(bodies) {
            let record = Signed::sign(&owner, owner.public(), seq, before, seq, &body).unwrap();
            before = record.id();
            state.keep(&check(record.bytes().to_vec(), &state).unwrap());
        }
        state
    }

    /// 01M519Z4RWHWP3TKNP4T5GKKE9: the look of a wave with no riff-server
    /// reads the pause and the holds of its repository from the `plan`
    /// records. A hold of another repository does not count. A pause of
    /// another repository does not stop it; a pause of the whole riff
    /// does.
    #[test]
    fn the_look_reads_the_pause_and_the_holds_of_the_repository_from_the_plan_records() {
        let (here, there): (ThreadName, ThreadName) =
            ("o/r".parse().unwrap(), "o/other".parse().unwrap());
        let hold = |thread: &ThreadName, item: &str| Plan::Hold {
            thread: thread.clone(),
            item: item.into(),
            reason: "wait for Mike".into(),
        };
        let state = riff_with(&[
            hold(&here, "issue-12"),
            hold(&there, "issue-13"),
            Plan::Pause {
                thread: Some(there.clone()),
            },
        ]);
        let (paused, holds) = look_in(&state, &here);
        assert!(!paused);
        assert_eq!(holds, BTreeSet::from(["issue-12".to_owned()]));
        let info = &holds_in(&state, &here)["issue-12"];
        assert_eq!(info.reason, "wait for Mike");
        assert_eq!(info.by, Some(By::Person("mike".into())));
        assert!(look_in(&state, &there).0);
        let state = riff_with(&[hold(&here, "issue-12"), Plan::Pause { thread: None }]);
        assert!(look_in(&state, &here).0);
    }
}
