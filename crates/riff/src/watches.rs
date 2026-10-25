//! The watches of the async operations (01M5RGCW5NCKQYA61RM63914TX,
//! 01M5RGD0RTXYA7FF5K1TH72MGX).
//!
//! # Design
//!
//! No session has to remember to wait. A riff command that starts an
//! async operation registers a watch in the same call. The riff daemon
//! of the host looks at each watch each [`every`]. When
//! the operation ends, with a success or a failure, the daemon posts one
//! message to the lead of the repository, with the result, the failed
//! jobs and the next step. Then the watch goes.
//!
//! ```mermaid
//! flowchart LR
//!     C["riff release cut, verify, test,<br/>publish, approve, retry, rollback"] -- "register" --> F["watches.json<br/>in the folder of the daemon"]
//!     W["riff wave run"] -- "one watch for the wave" --> F
//!     D["riff daemon:<br/>a look each minute"] -- "read" --> F
//!     D -- "gh, git" --> G["the forge"]
//!     D -- "one message at the end" --> L["the lead"]
//!     D -- "a line while a deploy waits" --> O["the owner in riff chat"]
//! ```
//!
//! | Command | [`Op`] | Ends when |
//! |---|---|---|
//! | `riff release cut`, `riff release verify` | [`Op::Pull`] | the Gate of the release pull request ends, or the pull request merges or closes |
//! | `riff release test --host HOST` | [`Op::Test`] | the real-session test passes or fails |
//! | `riff release publish`, `approve`, `retry`, `rollback` | [`Op::Deploy`] | the job `Deploy` of the tag passes or fails |
//! | `riff wave run` | [`Op::Wave`] | never: it posts each time the last open item of the current wave closes |
//!
//! The same watch is one watch: a second `riff release verify` of the
//! same pull request registers nothing new ([`Registered::Kept`]).
//!
//! # The file
//!
//! The daemon keeps the watches in [`FILE`] of its folder
//! ([`crate::daemon::dir`]), so a restart keeps them. A command on the
//! host writes the file itself, with a lock ([`Watches::register`]). A
//! lead in a container asks the daemon on its socket
//! ([`crate::daemon::ServerCall::Watch`]), because the container has no
//! access to the folder. The file holds the repository, the [`Op`], the
//! start, and what the watch saw ([`Seen`]). The daemon finds the clone
//! and the riff-server of a repository in `waves.json`, or in
//! `served.json` ([`wave_of`]).
//!
//! # A look
//!
//! One look ([`Watches::look_all`]) reads the file and groups the watches
//! by repository. For each repository, one forge ([`Lookout::forge`])
//! serves all of its watches: `gh` with the sign-in of the person on the
//! host, and `git` in the main clone. [`look`] asks the forge for the
//! state of one watch:
//!
//! ```mermaid
//! flowchart TD
//!     R["read the forge"] --> E{"fault?"}
//!     E -- "yes, a new one" --> F["message to the lead: the fault, one time"]
//!     E -- "yes, the same" --> K["nothing: try again at the next look"]
//!     E -- no --> S{"the operation ended?"}
//!     S -- no --> K
//!     S -- yes --> P["message to the lead:<br/>the result, the failed jobs, next: ..."]
//!     P --> G["the watch goes"]
//! ```
//!
//! A message that does not go out stays: the next look posts it. A look
//! that ends with a fault posts it once, until the fault changes.
//!
//! # A deploy
//!
//! The watch of a deploy follows the CI run of the tag ([`Op::Deploy`]).
//! The job `Deploy` waits for `production` only after the jobs before it
//! passed, so a run that still builds posts nothing
//! (01M5RGD0XTR4X1MEV726HMCEGQ):
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Builds: riff release publish
//!     Builds --> Waits: the job Deploy waits for production
//!     Builds --> Failed: a job failed
//!     Waits --> Waits: 4 hours: a reminder to the owner
//!     Waits --> Runs: a person runs riff release approve
//!     Runs --> Passed: the job Deploy passed
//!     Runs --> Failed: the job Deploy failed
//!     Waits --> Failed: a job failed
//!     Passed --> [*]: message to the lead: next: riff release check
//!     Failed --> [*]: message to the lead: the jobs, next: riff release retry
//! ```
//!
//! At "Waits" the lead gets one message, and the owner gets the line
//! "waits for your approval" in `riff chat`, with each hold of the
//! version (01M5SNV7BC5J58WRZRJ5HS4NSJ). After `riff release retry`
//! or `rollback`, the CI run still has its old state for a moment. So
//! the command gives the state that it saw ([`Stale`]), and the watch
//! ignores that state until it moves.
//!
//! At "Waits", "Passed" and "Failed" the watch also posts one note to
//! each other lead of the riff ([`Post::Leads`], [`crate::hold`],
//! 01M5SNV75DRFHR26BT2M8243VV): the lead of the release has the message
//! above, and the other leads have the note, with what comes next for
//! them. The note comes after the message and the line to the owner, so
//! a post that fails leaves the message to the lead as the first fact.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use nix::fcntl::{Flock, FlockArg};
use riff_core::wire::Kind;
use serde::{Deserialize, Serialize};

use crate::cycle::{self, DeployState, Forge, TestState};
use crate::daemon::remote;
use crate::hold::{self, Hold, Step};
use crate::pr::Gate;
use crate::wave::{BoxFuture, Wave};

/// The file of the watches, in the folder of the daemon.
pub const FILE: &str = "watches.json";

/// The file that locks [`FILE`] between the daemon and a command.
const LOCK: &str = "watches.lock";

/// The variable that gives the time between two looks at the watches in
/// milliseconds. Only a test sets it.
///
/// ```
/// assert_eq!(riff::watches::EVERY_VAR, "RIFF_WATCHES_EVERY_MS");
/// ```
pub const EVERY_VAR: &str = "RIFF_WATCHES_EVERY_MS";

/// The time between two looks at the watches: [`crate::look::LOOK_EVERY`],
/// or the milliseconds of [`EVERY_VAR`].
pub fn every() -> Duration {
    std::env::var(EVERY_VAR)
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(crate::look::LOOK_EVERY, Duration::from_millis)
}

/// The time between two lines to the owner while a deploy waits for the
/// approval (01M58KMM3NFGJRW4CCV50KJQB2, 01M5RGD0XTR4X1MEV726HMCEGQ).
pub const REMIND_EVERY: Duration = Duration::from_secs(4 * 60 * 60);

/// The state of a CI run that a command saw before it started an
/// operation: the watch ignores it until the state moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stale {
    /// The ID of the CI run.
    pub run: u64,
    /// Where its job `Deploy` was.
    pub state: DeployState,
}

/// An async operation that a command starts.
///
/// ```
/// use riff::watches::Op;
///
/// let op = Op::Pull { tag: "v4.3.0".into(), number: 1140 };
/// assert_eq!(serde_json::to_string(&op)?, r#"{"kind":"pull","tag":"v4.3.0","number":1140}"#);
/// assert_eq!(op.key(), "pull-1140");
/// assert_eq!(Op::Wave.key(), "wave");
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Op {
    /// The release pull request `number` of `tag`: its Gate, then its
    /// merge (`riff release cut`, `riff release verify`).
    Pull {
        /// The tag of the release, `vX.Y.Z`.
        tag: String,
        /// The number of the pull request.
        number: u64,
    },
    /// The real-session test of `tag` on `commit` (`riff release test`).
    Test {
        /// The tag of the release, `vX.Y.Z`.
        tag: String,
        /// The commit that the test runs on.
        commit: String,
    },
    /// The CI run of `tag` up to the end of the job `Deploy`
    /// (`riff release publish`, `approve`, `retry`, `rollback`).
    Deploy {
        /// The tag of the release, `vX.Y.Z`.
        tag: String,
        /// The state that the command saw before it started the
        /// operation, when the tag had a CI run then.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stale: Option<Stale>,
    },
    /// The current wave of the repository: the last open item closes.
    Wave,
}

impl Op {
    /// The name of the watch in its repository. Two watches with the
    /// same key are one watch.
    pub fn key(&self) -> String {
        match self {
            Op::Pull { number, .. } => format!("pull-{number}"),
            Op::Test { commit, .. } => format!("test-{commit}"),
            Op::Deploy { tag, .. } => format!("deploy-{tag}"),
            Op::Wave => "wave".to_owned(),
        }
    }

    /// What the watch waits for, in words for a line to the lead.
    ///
    /// ```
    /// use riff::watches::Op;
    ///
    /// let test = Op::Test { tag: "v4.3.0".into(), commit: "abc1234".into() };
    /// assert_eq!(test.what(), "the real-session test of v4.3.0 on abc1234");
    /// assert_eq!(Op::Wave.what(), "the current wave");
    /// ```
    pub fn what(&self) -> String {
        match self {
            Op::Pull { number, .. } => format!("the release pull request #{number}"),
            Op::Test { tag, commit } => format!("the real-session test of {tag} on {commit}"),
            Op::Deploy { tag, .. } => format!("the deploy of {tag}"),
            Op::Wave => "the current wave".to_owned(),
        }
    }
}

/// What a watch saw at the looks before: it tells each fact one time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Seen {
    /// [`Op::Deploy`]: the state moved from [`Stale`].
    pub moved: bool,
    /// [`Op::Pull`]: the lead got the news that the Gate passed.
    pub gate: bool,
    /// [`Op::Deploy`]: the job `Deploy` waits, and the lead got the news.
    pub waits: bool,
    /// [`Op::Deploy`]: the last line to the owner, in milliseconds since
    /// the epoch.
    pub reminded_ms: Option<i64>,
    /// [`Op::Wave`]: the title of the wave that had an open item.
    pub wave: Option<String>,
    /// The last fault that the lead got, until a look reads the forge.
    pub fault: Option<String>,
}

/// A watch in [`FILE`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watch {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// The operation.
    pub op: Op,
    /// The start of the watch, in milliseconds since the epoch.
    pub since_ms: i64,
    /// What the watch saw.
    #[serde(default)]
    pub seen: Seen,
}

/// What [`look`] decided about one watch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// The posts, in order.
    pub posts: Vec<Post>,
    /// True when the operation ended: the watch goes.
    pub end: bool,
}

/// A post of a watch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Post {
    /// A message that wakes the lead of the repository.
    Lead(String),
    /// A note to each other lead of the riff: the step of the release
    /// that the watch saw ([`crate::hold::tell_leads`],
    /// 01M5SNV75DRFHR26BT2M8243VV).
    Leads(String),
    /// A line to the owner in `riff chat`: the deploy of `tag` waits for
    /// the approval ([`cycle::waits_line`]).
    Owner {
        /// The tag of the release.
        tag: String,
        /// The page of the CI run.
        run: String,
        /// True for a reminder.
        reminder: bool,
    },
}

/// What a watch needs of the host: the forge of a repository, and the
/// posts. The daemon uses [`Here`]; a test gives a fake.
pub trait Lookout: Send + Sync {
    /// The forge of the main clone of `wave`. It runs `gh` and `git`, so
    /// the caller calls it on a thread that may block. With `fetch`, it
    /// fetches `origin` and its tags first: the watch of a wave needs
    /// only `gh`, and the main clone of a person stays as it is.
    fn forge(&self, wave: &Wave, fetch: bool) -> Result<Box<dyn Forge>>;

    /// Posts `body` as a message to the lead of the repository of
    /// `wave`, as the person.
    fn tell<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, Result<()>>;

    /// Writes the line that `line` makes in `riff chat`, as the person.
    /// `line` gets the USER of the owner of the riff, or an empty text
    /// when it has none. The line wakes the owner.
    fn chat<'a>(
        &'a self,
        wave: &'a Wave,
        line: &'a (dyn Fn(&str) -> String + Sync),
    ) -> BoxFuture<'a, Result<()>>;

    /// Posts `body` as a message to each lead of the riff, but the lead of
    /// the person in the repository of `wave`, as the person
    /// (01M5SNV75DRFHR26BT2M8243VV). That lead gets the message of the
    /// watch.
    fn leads<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, Result<()>>;

    /// The holds on `tag` that stand now, as the person
    /// (01M5SNV7BC5J58WRZRJ5HS4NSJ).
    fn holds<'a>(&'a self, wave: &'a Wave, tag: &'a str) -> BoxFuture<'a, Result<Vec<Hold>>>;
}

/// The [`Lookout`] of the daemon: `gh` with the sign-in of the person,
/// and riff-server.
#[derive(Debug, Default, Clone, Copy)]
pub struct Here;

impl Lookout for Here {
    fn forge(&self, wave: &Wave, fetch: bool) -> Result<Box<dyn Forge>> {
        let gh = crate::daemon::sweep::gh(&wave.repo);
        Ok(match fetch {
            true => Box::new(cycle::GitHub::new(gh, &wave.main)?),
            false => Box::new(cycle::GitHub::at(gh, &wave.main)),
        })
    }

    fn tell<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(crate::wave::post_as_person(
            wave,
            None,
            body,
            Kind::Message,
            None,
        ))
    }

    fn chat<'a>(
        &'a self,
        wave: &'a Wave,
        line: &'a (dyn Fn(&str) -> String + Sync),
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let person = crate::identity::person(&wave.place(None)?, &wave.server)?;
            let api = riff_client::api::Api::new(&wave.server).signed_in(None)?;
            let owner = match api.roster(&person, false).await?.owner {
                riff_core::wire::RiffOwner::Owner { user, .. } => user,
                _ => String::new(),
            };
            let body = line(&owner);
            let to = crate::chat::wakes(&body, person.who().user());
            api.post(
                &person,
                Some(&crate::chat::thread()),
                &to,
                &body,
                Kind::Message,
            )
            .await?;
            Ok(())
        })
    }

    fn leads<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let person = crate::identity::person(&wave.place(None)?, &wave.server)?;
            let api = riff_client::api::Api::new(&wave.server).signed_in(None)?;
            crate::hold::tell_leads(&api, &person, true, body).await?;
            Ok(())
        })
    }

    fn holds<'a>(&'a self, wave: &'a Wave, tag: &'a str) -> BoxFuture<'a, Result<Vec<Hold>>> {
        Box::pin(async move {
            let lines = crate::hold::read_as_person(&wave.server, &wave.place(None)?).await?;
            Ok(crate::hold::holds(&lines, tag))
        })
    }
}

/// The wave of the repository `repo` that the daemon in `dir` knows: the
/// wave that it runs, else the repository that it serves.
pub fn wave_of(dir: &Path, repo: &str) -> Result<Wave> {
    if let Some(wave) = crate::wave::read_waves(dir)?.remove(repo) {
        return Ok(wave);
    }
    remote::read_served(dir)?
        .remove(repo)
        .map(|served| served.wave)
        .ok_or_else(|| anyhow!(crate::text::watch_no_wave(repo)))
}

/// What [`Watches::register`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Registered {
    /// A new watch.
    New,
    /// The repository had this watch already: nothing changed.
    Kept,
}

/// The watches in [`FILE`] of the folder of the daemon.
#[derive(Debug, Clone)]
pub struct Watches {
    dir: PathBuf,
}

impl Watches {
    /// The watches in the folder `dir` of the daemon.
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_owned(),
        }
    }

    fn path(&self) -> PathBuf {
        self.dir.join(FILE)
    }

    /// Takes the lock of the file: the daemon and a command change it one
    /// at a time.
    fn lock(&self) -> Result<Flock<std::fs::File>> {
        let path = self.dir.join(LOCK);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        Flock::lock(file, FlockArg::LockExclusive)
            .map_err(|(_, e)| anyhow!("cannot lock {}: {e}", path.display()))
    }

    /// Each watch, in the order of their start. With no file, none.
    pub fn read(&self) -> Result<Vec<Watch>> {
        let path = self.path();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("cannot read the watches in {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    fn write(&self, watches: &[Watch]) -> Result<()> {
        let path = self.path();
        let tmp = path.with_extension("json.new");
        let mut file = std::fs::File::create(&tmp)
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        file.write_all(&serde_json::to_vec_pretty(watches)?)?;
        file.sync_all()?;
        std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))
    }

    /// Registers the watch of `op` in `repo`, started at `now_ms`. It
    /// fails when the daemon knows no clone of `repo`
    /// ([`wave_of`]): the watch could never look at the forge.
    ///
    /// ```
    /// use riff::watches::{Op, Registered, Watches};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let watches = Watches::new(dir.path());
    /// // The daemon runs no wave of o/r yet.
    /// assert!(watches.register("o/r", Op::Wave, 0).is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn register(&self, repo: &str, op: Op, now_ms: i64) -> Result<Registered> {
        wave_of(&self.dir, repo)?;
        let _lock = self.lock()?;
        let mut watches = self.read()?;
        let key = op.key();
        if watches.iter().any(|w| w.repo == repo && w.op.key() == key) {
            return Ok(Registered::Kept);
        }
        watches.push(Watch {
            repo: repo.to_owned(),
            op,
            since_ms: now_ms,
            seen: Seen::default(),
        });
        self.write(&watches)?;
        Ok(Registered::New)
    }

    /// Keeps `watch` in the file in place of the watch with its key, or
    /// removes it when `end` is true. A watch that the file lost stays
    /// lost.
    fn settle(&self, watch: &Watch, end: bool) -> Result<()> {
        let _lock = self.lock()?;
        let mut watches = self.read()?;
        let key = watch.op.key();
        let Some(at) = watches
            .iter()
            .position(|w| w.repo == watch.repo && w.op.key() == key)
        else {
            return Ok(());
        };
        if end {
            watches.remove(at);
        } else {
            watches[at] = watch.clone();
        }
        self.write(&watches)
    }

    /// One watch of the current wave for each wave that the daemon runs
    /// (`waves.json`), and none for a repository that it does not run.
    fn keep_waves(&self, now_ms: i64) -> Result<()> {
        let waves = crate::wave::read_waves(&self.dir)?;
        let _lock = self.lock()?;
        let mut watches = self.read()?;
        let before = watches.len();
        watches.retain(|w| w.op != Op::Wave || waves.contains_key(&w.repo));
        let mut changed = watches.len() != before;
        for repo in waves.keys() {
            if !watches.iter().any(|w| w.repo == *repo && w.op == Op::Wave) {
                watches.push(Watch {
                    repo: repo.clone(),
                    op: Op::Wave,
                    since_ms: now_ms,
                    seen: Seen::default(),
                });
                changed = true;
            }
        }
        if changed {
            self.write(&watches)?;
        }
        Ok(())
    }

    /// One look at each watch (see "A look" above). It posts through
    /// `lookout`. The error is a file that riff cannot read or write.
    pub async fn look_all(&self, lookout: &Arc<dyn Lookout>, now_ms: i64) -> Result<()> {
        self.keep_waves(now_ms)?;
        let mut by_repo: BTreeMap<String, Vec<Watch>> = BTreeMap::new();
        for watch in self.read()? {
            by_repo.entry(watch.repo.clone()).or_default().push(watch);
        }
        for (repo, watches) in by_repo {
            let wave = match wave_of(&self.dir, &repo) {
                Ok(wave) => wave,
                Err(e) => {
                    self.no_wave(watches, &format!("{e:#}"));
                    continue;
                }
            };
            let seen = {
                let (lookout, wave) = (lookout.clone(), wave.clone());
                tokio::task::spawn_blocking(move || read_all(&*lookout, &wave, watches, now_ms))
                    .await?
            };
            for (watch, result) in seen {
                self.settle_one(lookout, &wave, watch, result).await;
            }
        }
        Ok(())
    }

    /// The watches of a repository whose clone the daemon does not know
    /// any more: the fault goes to stderr, one time.
    fn no_wave(&self, watches: Vec<Watch>, fault: &str) {
        for mut watch in watches {
            if watch.seen.fault.as_deref() == Some(fault) {
                continue;
            }
            eprintln!("riff daemon: {}: {fault}", watch.repo);
            watch.seen.fault = Some(fault.to_owned());
            if let Err(e) = self.settle(&watch, false) {
                eprintln!("riff daemon: {e:#}");
            }
        }
    }

    /// Posts what one look found, then keeps or removes the watch.
    async fn settle_one(
        &self,
        lookout: &Arc<dyn Lookout>,
        wave: &Wave,
        mut watch: Watch,
        result: Result<Outcome>,
    ) {
        let saved = match result {
            Ok(outcome) => {
                watch.seen.fault = None;
                for post in &outcome.posts {
                    if let Err(e) = post_one(&**lookout, wave, post).await {
                        eprintln!("riff daemon: {}: {e:#}", watch.repo);
                        // The lead has no message yet: the next look tries again.
                        if matches!(post, Post::Lead(_)) {
                            return;
                        }
                    }
                }
                self.settle(&watch, outcome.end)
            }
            Err(e) => {
                let fault = format!("{e:#}");
                if watch.seen.fault.as_deref() == Some(fault.as_str()) {
                    return;
                }
                let body = crate::text::watch_fault(&watch.op.what(), &fault);
                if let Err(e) = lookout.tell(wave, &body).await {
                    eprintln!("riff daemon: {}: {e:#}", watch.repo);
                    return;
                }
                watch.seen.fault = Some(fault);
                self.settle(&watch, false)
            }
        };
        if let Err(e) = saved {
            eprintln!("riff daemon: {e:#}");
        }
    }
}

/// The result of [`look`] for each of `watches`, with one forge.
fn read_all(
    lookout: &dyn Lookout,
    wave: &Wave,
    watches: Vec<Watch>,
    now_ms: i64,
) -> Vec<(Watch, Result<Outcome>)> {
    let fetch = watches.iter().any(|w| w.op != Op::Wave);
    let forge = match lookout.forge(wave, fetch) {
        Ok(forge) => forge,
        Err(e) => {
            let fault = format!("{e:#}");
            return watches
                .into_iter()
                .map(|w| (w, Err(anyhow!(fault.clone()))))
                .collect();
        }
    };
    watches
        .into_iter()
        .map(|mut watch| {
            let result = look(&*forge, &mut watch, now_ms);
            (watch, result)
        })
        .collect()
}

async fn post_one(lookout: &dyn Lookout, wave: &Wave, post: &Post) -> Result<()> {
    match post {
        Post::Lead(body) => lookout.tell(wave, body).await,
        Post::Leads(body) => lookout.leads(wave, body).await,
        Post::Owner { tag, run, reminder } => {
            // A hold that riff cannot read leaves the line as it was.
            let holds = lookout.holds(wave, tag).await.unwrap_or_default();
            let line = |owner: &str| cycle::waits_line(owner, tag, run, *reminder, &holds);
            lookout.chat(wave, &line).await
        }
    }
}

/// The next step after an operation on `tag` ended: the next step of the
/// release at its state now ([`cycle::next`]), or `fallback`.
fn next_of(forge: &dyn Forge, tag: &str, fallback: &str) -> String {
    cycle::tag_version(tag)
        .and_then(|version| cycle::stage(forge, version))
        .ok()
        .flatten()
        .map_or_else(|| fallback.to_owned(), |stage| cycle::next(&stage))
}

/// The failed jobs for a line: `: A, B`, or a text for no name.
fn jobs_text(names: &[String], ended: &str) -> String {
    match names.is_empty() {
        true => format!(" (it ended with {ended})"),
        false => format!(": {}", names.join(", ")),
    }
}

/// Looks at the forge for `watch` at `now_ms`, and says what to post.
/// It changes `watch.seen` for what it decided. It fails when the forge
/// does not answer.
pub fn look(forge: &dyn Forge, watch: &mut Watch, now_ms: i64) -> Result<Outcome> {
    match watch.op.clone() {
        Op::Pull { tag, number } => look_pull(forge, &mut watch.seen, &tag, number),
        Op::Test { tag, commit } => look_test(forge, &tag, &commit),
        Op::Deploy { tag, stale } => {
            look_deploy(forge, &mut watch.seen, &watch.repo, &tag, stale, now_ms)
        }
        Op::Wave => look_wave(forge, &mut watch.seen),
    }
}

fn say(body: String, end: bool) -> Outcome {
    Outcome {
        posts: vec![Post::Lead(body)],
        end,
    }
}

/// [`say`], and a note to each other lead of the riff
/// (01M5SNV75DRFHR26BT2M8243VV).
fn say_and_tell_leads(body: String, note: String, end: bool) -> Outcome {
    Outcome {
        posts: vec![Post::Lead(body), Post::Leads(note)],
        end,
    }
}

fn look_pull(forge: &dyn Forge, seen: &mut Seen, tag: &str, number: u64) -> Result<Outcome> {
    let pull = forge.candidate(number)?;
    match pull.state.as_str() {
        "MERGED" => {
            let next = next_of(forge, tag, "riff release status");
            return Ok(say(
                format!("The release pull request #{number} of {tag} merged. next: {next}"),
                true,
            ));
        }
        "CLOSED" => {
            let cut = cycle::tag_version(tag).map_or_else(
                |_| "riff release cut X.Y.Z".to_owned(),
                |v| format!("riff release cut {v}"),
            );
            return Ok(say(
                format!(
                    "The release pull request #{number} of {tag} closed with no merge. next: {cut}"
                ),
                true,
            ));
        }
        _ => {}
    }
    let checks = forge.full_check()?;
    match forge.gate(&checks, &pull.head)? {
        Gate::Ended(conclusion) => {
            let jobs = forge.failed_jobs(&checks, &pull.head)?;
            Ok(say(
                format!(
                    "The Gate of the release pull request #{number} failed{}. next: run the \
                     failed jobs again, then riff release verify {number}",
                    jobs_text(&jobs, &conclusion)
                ),
                true,
            ))
        }
        Gate::Passed if !seen.gate => {
            seen.gate = true;
            if forge.verify_passed(&pull.head)? {
                return Ok(Outcome::default());
            }
            Ok(say(
                format!(
                    "The Gate of the release pull request #{number} passed. next: riff release \
                     verify {number}"
                ),
                false,
            ))
        }
        _ => Ok(Outcome::default()),
    }
}

fn look_test(forge: &dyn Forge, tag: &str, commit: &str) -> Result<Outcome> {
    let Some(state @ (TestState::Passed | TestState::Failed)) = forge.test_state(commit)? else {
        return Ok(Outcome::default());
    };
    let next = next_of(forge, tag, "riff release status");
    Ok(say(
        match state {
            TestState::Passed => {
                format!("The real-session test of {tag} passed on {commit}. next: {next}")
            }
            _ => format!(
                "The real-session test of {tag} failed on {commit}. The failed step is on the \
                 release issue. next: {next}"
            ),
        },
        true,
    ))
}

fn look_deploy(
    forge: &dyn Forge,
    seen: &mut Seen,
    repo: &str,
    tag: &str,
    stale: Option<Stale>,
    now_ms: i64,
) -> Result<Outcome> {
    let Some(deploy) = forge.deploy(tag)? else {
        return Ok(Outcome::default());
    };
    if !seen.moved {
        let now = Stale {
            run: deploy.run,
            state: deploy.state,
        };
        if stale == Some(now) {
            return Ok(Outcome::default());
        }
        seen.moved = true;
    }
    let run = deploy.url.as_str();
    match deploy.state {
        DeployState::Before | DeployState::Running => {
            seen.waits = false;
            seen.reminded_ms = None;
            Ok(Outcome::default())
        }
        DeployState::Waiting if !seen.waits => {
            seen.waits = true;
            seen.reminded_ms = Some(now_ms);
            Ok(Outcome {
                posts: vec![
                    Post::Lead(format!(
                        "The deploy of {tag} waits for a person: {run}. riff told the owner in \
                         riff chat. next: a person runs riff release approve {tag} in a terminal"
                    )),
                    Post::Owner {
                        tag: tag.to_owned(),
                        run: run.to_owned(),
                        reminder: false,
                    },
                    Post::Leads(crate::text::release_note(Step::Waits, tag, &hold::by(repo))),
                ],
                end: false,
            })
        }
        DeployState::Waiting => {
            let due = seen
                .reminded_ms
                .is_none_or(|at| now_ms.saturating_sub(at) >= REMIND_EVERY.as_millis() as i64);
            if !due {
                return Ok(Outcome::default());
            }
            seen.reminded_ms = Some(now_ms);
            Ok(Outcome {
                posts: vec![Post::Owner {
                    tag: tag.to_owned(),
                    run: run.to_owned(),
                    reminder: true,
                }],
                end: false,
            })
        }
        DeployState::Passed => {
            let next = next_of(forge, tag, &format!("riff release check {tag}"));
            Ok(say_and_tell_leads(
                format!("The deploy of {tag} passed: {run}. next: {next}"),
                crate::text::release_note(Step::Deployed, tag, &hold::by(repo)),
                true,
            ))
        }
        DeployState::Failed => {
            let next = next_of(
                forge,
                tag,
                &format!("riff release retry {tag}, or riff release rollback vPREVIOUS"),
            );
            Ok(say_and_tell_leads(
                format!(
                    "The deploy of {tag} failed{}: {run}. The old revision still serves. next: \
                     {next}",
                    jobs_text(&deploy.failed, "a failure")
                ),
                crate::text::release_note(Step::DeployFailed, tag, &hold::by(repo)),
                true,
            ))
        }
    }
}

fn look_wave(forge: &dyn Forge, seen: &mut Seen) -> Result<Outcome> {
    let Some((title, open)) = forge.wave_open()? else {
        return Ok(Outcome::default());
    };
    if open > 0 {
        seen.wave = Some(title);
        return Ok(Outcome::default());
    }
    if seen.wave.as_deref() != Some(title.as_str()) {
        return Ok(Outcome::default());
    }
    seen.wave = None;
    let number = hygiene::wave_number(&title).unwrap_or_default();
    let next = match forge.has_release() {
        true => format!("riff release cut X.Y.Z, or riff wave end {number} --no-release TEXT"),
        false => format!("riff wave end {number}"),
    };
    Ok(say(
        format!("Wave {number} is complete: next {next}"),
        false,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use tokio::task::JoinHandle;

    use super::*;
    use crate::cycle::tests::Fake;
    use crate::cycle::{Candidate, Deploy, ReleasePr};

    const REPO: &str = "o/r";

    /// The host of a test: a fake forge that a test changes between two
    /// looks, and the posts that the watches made.
    struct Ground {
        forge: Mutex<Fake>,
        /// The fault of the forge, when it fails.
        fault: Mutex<Option<String>>,
        /// True when riff-server does not take a message.
        down: AtomicBool,
        told: Mutex<Vec<String>>,
        chats: Mutex<Vec<String>>,
        /// The notes to each other lead.
        leads: Mutex<Vec<String>>,
        /// The holds that stand.
        holds: Mutex<Vec<Hold>>,
        /// The `fetch` of each forge that the watches asked for.
        fetches: Mutex<Vec<bool>>,
    }

    impl Ground {
        fn new(forge: Fake) -> Arc<Ground> {
            Arc::new(Ground {
                forge: Mutex::new(forge),
                fault: Mutex::new(None),
                down: AtomicBool::new(false),
                told: Mutex::default(),
                chats: Mutex::default(),
                leads: Mutex::default(),
                holds: Mutex::default(),
                fetches: Mutex::default(),
            })
        }

        fn forge(&self, change: impl FnOnce(&mut Fake)) {
            change(&mut self.forge.lock().unwrap());
        }

        /// The messages to the lead since the last call.
        fn told(&self) -> Vec<String> {
            std::mem::take(&mut self.told.lock().unwrap())
        }

        /// The lines to the owner since the last call.
        fn chats(&self) -> Vec<String> {
            std::mem::take(&mut self.chats.lock().unwrap())
        }

        /// The notes to each other lead since the last call.
        fn leads(&self) -> Vec<String> {
            std::mem::take(&mut self.leads.lock().unwrap())
        }
    }

    impl Lookout for Ground {
        fn forge(&self, _: &Wave, fetch: bool) -> Result<Box<dyn Forge>> {
            self.fetches.lock().unwrap().push(fetch);
            if let Some(fault) = self.fault.lock().unwrap().clone() {
                return Err(anyhow!(fault));
            }
            Ok(Box::new(self.forge.lock().unwrap().clone()))
        }

        fn tell<'a>(&'a self, _: &'a Wave, body: &'a str) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                if self.down.load(Ordering::SeqCst) {
                    return Err(anyhow!("riff-server does not answer"));
                }
                self.told.lock().unwrap().push(body.to_owned());
                Ok(())
            })
        }

        fn chat<'a>(
            &'a self,
            _: &'a Wave,
            line: &'a (dyn Fn(&str) -> String + Sync),
        ) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                self.chats.lock().unwrap().push(line("mike"));
                Ok(())
            })
        }

        fn leads<'a>(&'a self, _: &'a Wave, body: &'a str) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                if self.down.load(Ordering::SeqCst) {
                    return Err(anyhow!("riff-server does not answer"));
                }
                self.leads.lock().unwrap().push(body.to_owned());
                Ok(())
            })
        }

        fn holds<'a>(&'a self, _: &'a Wave, _: &'a str) -> BoxFuture<'a, Result<Vec<Hold>>> {
            Box::pin(async move { Ok(self.holds.lock().unwrap().clone()) })
        }
    }

    /// The folder of a daemon that runs the wave of `o/r`.
    fn daemon_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let wave = Wave {
            repo: REPO.into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        crate::wave::write_waves(dir.path(), &BTreeMap::from([(REPO.to_owned(), wave)])).unwrap();
        dir
    }

    const MINUTE: i64 = 60_000;

    /// One look of a new `Watches` on the folder: a daemon that started
    /// again reads the file.
    async fn look(dir: &Path, ground: &Arc<Ground>, now_ms: i64) {
        let ground: Arc<dyn Lookout> = ground.clone();
        Watches::new(dir).look_all(&ground, now_ms).await.unwrap();
    }

    fn watch(dir: &Path, op: Op) {
        let registered = Watches::new(dir).register(REPO, op, 0).unwrap();
        assert_eq!(registered, Registered::New);
    }

    /// The watches in the file, but the watch of the wave.
    fn kept(dir: &Path) -> Vec<Op> {
        let watches = Watches::new(dir).read().unwrap();
        watches
            .into_iter()
            .map(|w| w.op)
            .filter(|op| *op != Op::Wave)
            .collect()
    }

    fn pull() -> Op {
        Op::Pull {
            tag: "v3.3.0".into(),
            number: 1140,
        }
    }

    /// A forge with an open release pull request whose Gate runs.
    fn open_pull() -> Fake {
        Fake {
            candidate: Candidate {
                state: "OPEN".into(),
                head: "h1".into(),
                ..Candidate::default()
            },
            checks: vec!["Gate".into()],
            gate: Gate::Running,
            verify: false,
            ..Fake::new()
        }
    }

    fn merged(fake: &mut Fake) {
        fake.candidate.state = "MERGED".into();
        fake.release_pr = Some(ReleasePr {
            number: 1140,
            url: "https://github.com/o/r/pull/1140".into(),
            merge: "m1".into(),
            head: "h1".into(),
            body: String::new(),
        });
        fake.since = vec!["m1"];
    }

    fn deploy(state: DeployState, failed: &[&str]) -> Option<Deploy> {
        Some(Deploy {
            run: 9,
            url: "https://github.com/o/r/actions/runs/9".into(),
            state,
            at: String::new(),
            failed: failed.iter().map(ToString::to_string).collect(),
        })
    }

    #[tokio::test]
    async fn the_gate_passes_and_the_pull_request_merges_with_one_message_each() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        watch(dir.path(), pull());

        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty(), "the Gate runs");

        ground.forge(|f| f.gate = Gate::Passed);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(
            ground.told(),
            ["The Gate of the release pull request #1140 passed. next: riff release verify 1140"]
        );
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert!(ground.told().is_empty(), "no second message for the Gate");
        assert_eq!(kept(dir.path()), [pull()]);

        // The lead verified, and the pull request merged.
        ground.forge(merged);
        look(dir.path(), &ground, 4 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The release pull request #1140 of v3.3.0 merged. next: riff release test \
                 v3.3.0 --host HOST, or just real-session on origin/main"
            ]
        );
        look(dir.path(), &ground, 5 * MINUTE).await;
        assert!(ground.told().is_empty(), "no second message for the merge");
        assert!(kept(dir.path()).is_empty(), "the watch goes");
    }

    #[tokio::test]
    async fn a_gate_that_fails_tells_the_failed_jobs_once() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        ground.forge(|f| {
            f.gate = Gate::Ended("failure".into());
            f.failed = vec!["Clippy".into(), "Tests 1/4".into()];
        });
        watch(dir.path(), pull());

        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The Gate of the release pull request #1140 failed: Clippy, Tests 1/4. next: \
                 run the failed jobs again, then riff release verify 1140"
            ]
        );
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty());
        assert!(kept(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn a_pull_request_that_closes_with_no_merge_tells_the_lead() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        ground.forge(|f| f.candidate.state = "CLOSED".into());
        watch(dir.path(), pull());
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The release pull request #1140 of v3.3.0 closed with no merge. next: riff \
                 release cut 3.3.0"
            ]
        );
        assert!(kept(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn a_gate_that_passed_after_the_verify_tells_nothing_new() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        // `riff release verify` registers after it set the status.
        ground.forge(|f| {
            f.gate = Gate::Passed;
            f.verify = true;
        });
        watch(dir.path(), pull());
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty());
        ground.forge(merged);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
    }

    fn test_op() -> Op {
        Op::Test {
            tag: "v3.3.0".into(),
            commit: "m1".into(),
        }
    }

    /// A forge with a merged release and a test that runs.
    fn testing() -> Fake {
        let mut fake = Fake::ready("3.2.0");
        fake.running = vec!["m1"];
        fake.real = vec![];
        fake
    }

    #[tokio::test]
    async fn a_test_that_passes_tells_the_lead_the_next_step_once() {
        let dir = daemon_dir();
        let ground = Ground::new(testing());
        watch(dir.path(), test_op());
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty(), "the test runs");

        ground.forge(|f| {
            f.running.clear();
            f.real = vec![("m1", true)];
        });
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(
            ground.told(),
            ["The real-session test of v3.3.0 passed on m1. next: riff release publish v3.3.0"]
        );
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert!(ground.told().is_empty());
        assert!(kept(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn a_test_that_fails_tells_the_lead_the_next_step_once() {
        let dir = daemon_dir();
        let ground = Ground::new(testing());
        watch(dir.path(), test_op());
        ground.forge(|f| {
            f.running.clear();
            f.real = vec![("m1", false)];
        });
        look(dir.path(), &ground, MINUTE).await;
        let told = ground.told();
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(
            told[0].starts_with(
                "The real-session test of v3.3.0 failed on m1. The failed step is on the \
                 release issue. next: file the bug in the wave"
            ),
            "{told:?}"
        );
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty());
        assert!(kept(dir.path()).is_empty());
    }

    fn deploy_op(stale: Option<Stale>) -> Op {
        Op::Deploy {
            tag: "v3.3.0".into(),
            stale,
        }
    }

    #[tokio::test]
    async fn a_run_that_still_builds_says_nothing_of_the_approval() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| f.deploy = deploy(DeployState::Before, &[]));
        watch(dir.path(), deploy_op(None));

        for at in 1..4 {
            look(dir.path(), &ground, at * MINUTE).await;
        }
        assert!(ground.told().is_empty(), "the jobs before Deploy run");
        assert!(ground.chats().is_empty(), "no line of an approval");
        assert_eq!(kept(dir.path()), [deploy_op(None)]);

        // The job Deploy waits: the lead and the owner get it, one time.
        ground.forge(|f| f.deploy = deploy(DeployState::Waiting, &[]));
        look(dir.path(), &ground, 5 * MINUTE).await;
        assert_eq!(
            ground.told(),
            ["The deploy of v3.3.0 waits for a person: \
                 https://github.com/o/r/actions/runs/9. riff told the owner in riff chat. \
                 next: a person runs riff release approve v3.3.0 in a terminal"]
        );
        assert_eq!(
            ground.chats(),
            ["@mike riff release v3.3.0 waits for your approval: \
                 https://github.com/o/r/actions/runs/9. In a terminal, run: riff release \
                 approve v3.3.0"]
        );
        // 01M5SNV75DRFHR26BT2M8243VV: each other lead gets the wait too.
        assert_eq!(
            ground.leads(),
            [crate::text::release_note(
                Step::Waits,
                "v3.3.0",
                "the lead of o/r"
            )]
        );
        look(dir.path(), &ground, 6 * MINUTE).await;
        assert!(ground.told().is_empty());
        assert!(ground.chats().is_empty());
        assert!(ground.leads().is_empty(), "one note for the wait");
    }

    /// 01M5SNV7BC5J58WRZRJ5HS4NSJ: the line to the owner shows each hold
    /// that stands, also in a reminder.
    #[tokio::test]
    async fn the_line_to_the_owner_shows_each_hold() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| f.deploy = deploy(DeployState::Waiting, &[]));
        ground.holds.lock().unwrap().push(Hold {
            user: "nate".into(),
            repo: "n8behavior/sens-o-matic".into(),
            reason: "I migrate a database.".into(),
        });
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, 0).await;
        let hold = " A hold stands: the lead of n8behavior/sens-o-matic asks to wait: I migrate a \
                    database.";
        let chats = ground.chats();
        assert!(chats[0].ends_with(hold), "{chats:?}");
        look(dir.path(), &ground, 4 * 60 * MINUTE).await;
        let chats = ground.chats();
        assert!(
            chats[0].starts_with("Reminder: ") && chats[0].ends_with(hold),
            "{chats:?}"
        );
    }

    #[tokio::test]
    async fn a_deploy_that_waits_reminds_the_owner_each_four_hours() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| f.deploy = deploy(DeployState::Waiting, &[]));
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, 0).await;
        assert_eq!((ground.told().len(), ground.chats().len()), (1, 1));

        let hours = |n: i64| n * 60 * MINUTE;
        look(dir.path(), &ground, hours(3)).await;
        assert!(ground.chats().is_empty(), "3 hours is too soon");
        look(dir.path(), &ground, hours(4)).await;
        let chats = ground.chats();
        assert_eq!(chats.len(), 1);
        assert!(
            chats[0].starts_with("Reminder: @mike riff release v3.3.0 still waits"),
            "{chats:?}"
        );
        assert!(
            ground.told().is_empty(),
            "a reminder is no message to the lead"
        );
        look(dir.path(), &ground, hours(5)).await;
        assert!(ground.chats().is_empty());
        look(dir.path(), &ground, hours(8)).await;
        assert_eq!(ground.chats().len(), 1);
    }

    #[tokio::test]
    async fn a_deploy_that_passes_tells_the_lead_once() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.deploy = deploy(DeployState::Waiting, &[]);
            f.tags = vec!["v3.3.0".into()];
        });
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, MINUTE).await;
        ground.told();

        // A person approved: the job runs, then passes.
        ground.forge(|f| f.deploy = deploy(DeployState::Running, &[]));
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.chats().len() == 1);
        ground.forge(|f| f.deploy = deploy(DeployState::Passed, &[]));
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "The deploy of v3.3.0 passed: https://github.com/o/r/actions/runs/9. next: \
                 riff release check v3.3.0"
            ]
        );
        assert_eq!(
            ground.leads(),
            [
                crate::text::release_note(Step::Waits, "v3.3.0", "the lead of o/r"),
                crate::text::release_note(Step::Deployed, "v3.3.0", "the lead of o/r"),
            ]
        );
        look(dir.path(), &ground, 4 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.leads().is_empty());
        assert!(kept(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn a_deploy_that_fails_tells_the_failed_jobs_once() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.deploy = deploy(DeployState::Failed, &["Crate audit", "Deploy"]);
            f.tags = vec!["v3.3.0".into()];
        });
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(
            ground.told(),
            ["The deploy of v3.3.0 failed: Crate audit, Deploy: \
                 https://github.com/o/r/actions/runs/9. The old revision still serves. next: \
                 riff release retry v3.3.0, or riff release rollback v3.2.0"]
        );
        assert!(ground.chats().is_empty());
        assert_eq!(
            ground.leads(),
            [crate::text::release_note(
                Step::DeployFailed,
                "v3.3.0",
                "the lead of o/r"
            )]
        );
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty() && ground.leads().is_empty());
        assert!(kept(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn a_retry_ignores_the_failed_state_that_it_saw_until_the_run_moves() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.deploy = deploy(DeployState::Failed, &["Deploy"]);
            f.tags = vec!["v3.3.0".into()];
        });
        let stale = Stale {
            run: 9,
            state: DeployState::Failed,
        };
        watch(dir.path(), deploy_op(Some(stale)));

        // GitHub still shows the old failure.
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty());

        ground.forge(|f| f.deploy = deploy(DeployState::Running, &[]));
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty());

        // The run fails again: this one is news.
        ground.forge(|f| f.deploy = deploy(DeployState::Failed, &["Deploy"]));
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
        assert!(kept(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn a_rollback_ignores_the_old_run_of_the_older_release() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        ground.forge(|f| {
            f.deploy = deploy(DeployState::Passed, &[]);
            f.tags = vec!["v3.3.0".into()];
        });
        let stale = Stale {
            run: 9,
            state: DeployState::Passed,
        };
        watch(dir.path(), deploy_op(Some(stale)));
        look(dir.path(), &ground, MINUTE).await;
        assert!(
            ground.told().is_empty(),
            "the run of the old release passed long ago"
        );

        // The run by hand has a new ID and waits.
        ground.forge(|f| {
            f.deploy = Some(Deploy {
                run: 10,
                state: DeployState::Waiting,
                ..deploy(DeployState::Waiting, &[]).unwrap()
            });
        });
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
    }

    #[tokio::test]
    async fn the_wave_tells_the_lead_when_its_last_open_item_closes() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::new());
        ground.forge(|f| {
            f.wave_open = Some(("Wave 41: only the current release".into(), 2));
            f.has_release = true;
        });
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty());
        assert_eq!(Watches::new(dir.path()).read().unwrap()[0].op, Op::Wave);

        ground.forge(|f| f.wave_open.as_mut().unwrap().1 = 1);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty());

        // A new start of the daemon between the two looks keeps the watch.
        ground.forge(|f| f.wave_open.as_mut().unwrap().1 = 0);
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(
            ground.told(),
            [
                "Wave 41 is complete: next riff release cut X.Y.Z, or riff wave end 41 \
                 --no-release TEXT"
            ]
        );
        look(dir.path(), &ground, 4 * MINUTE).await;
        assert!(ground.told().is_empty(), "once");

        // A new item, then its close: a new completion.
        ground.forge(|f| f.wave_open.as_mut().unwrap().1 = 1);
        look(dir.path(), &ground, 5 * MINUTE).await;
        ground.forge(|f| f.wave_open.as_mut().unwrap().1 = 0);
        look(dir.path(), &ground, 6 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
    }

    #[tokio::test]
    async fn the_wave_of_a_repository_with_no_release_table_ends_the_wave() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::new());
        ground.forge(|f| f.wave_open = Some(("Wave 3".into(), 1)));
        look(dir.path(), &ground, MINUTE).await;
        ground.forge(|f| f.wave_open.as_mut().unwrap().1 = 0);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(ground.told(), ["Wave 3 is complete: next riff wave end 3"]);
    }

    /// The watch of a wave needs only `gh`: it does not fetch the main
    /// clone of the person each minute.
    #[tokio::test]
    async fn only_a_watch_of_an_operation_fetches_the_main_clone() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::ready("3.3.0"));
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(*ground.fetches.lock().unwrap(), [false]);
        watch(dir.path(), deploy_op(None));
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(*ground.fetches.lock().unwrap(), [false, true]);
    }

    #[tokio::test]
    async fn a_wave_that_the_daemon_stopped_loses_its_watch() {
        let dir = daemon_dir();
        let ground = Ground::new(Fake::new());
        look(dir.path(), &ground, MINUTE).await;
        assert_eq!(Watches::new(dir.path()).read().unwrap().len(), 1);
        crate::wave::write_waves(dir.path(), &BTreeMap::new()).unwrap();
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(Watches::new(dir.path()).read().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_forge_that_fails_tells_the_lead_the_fault_once() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        watch(dir.path(), pull());
        *ground.fault.lock().unwrap() = Some("gh: HTTP 502".into());

        look(dir.path(), &ground, MINUTE).await;
        // Each watch of the repository tells the fault one time.
        assert_eq!(
            ground.told(),
            [
                "riff cannot read the state of the release pull request #1140: gh: HTTP 502. \
                 riff tries again each minute, and tells you this fault one time.",
                "riff cannot read the state of the current wave: gh: HTTP 502. riff tries \
                 again each minute, and tells you this fault one time."
            ]
        );
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert!(ground.told().is_empty(), "the same fault: one message");
        assert_eq!(kept(dir.path()), [pull()], "the watch goes on");

        // The forge answers again: the watch ends as it does with no fault.
        *ground.fault.lock().unwrap() = None;
        ground.forge(|f| f.candidate.state = "CLOSED".into());
        look(dir.path(), &ground, 3 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
        assert!(kept(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn a_message_that_does_not_go_out_stays_for_the_next_look() {
        let dir = daemon_dir();
        let ground = Ground::new(open_pull());
        ground.forge(|f| f.candidate.state = "CLOSED".into());
        watch(dir.path(), pull());

        ground.down.store(true, Ordering::SeqCst);
        look(dir.path(), &ground, MINUTE).await;
        assert!(ground.told().is_empty());
        assert_eq!(kept(dir.path()), [pull()], "the end is not lost");

        ground.down.store(false, Ordering::SeqCst);
        look(dir.path(), &ground, 2 * MINUTE).await;
        assert_eq!(ground.told().len(), 1);
        assert!(kept(dir.path()).is_empty());
    }

    #[test]
    fn a_second_registration_of_the_same_operation_changes_nothing() {
        let dir = daemon_dir();
        let watches = Watches::new(dir.path());
        assert_eq!(watches.register(REPO, pull(), 5).unwrap(), Registered::New);
        assert_eq!(watches.register(REPO, pull(), 9).unwrap(), Registered::Kept);
        let all = watches.read().unwrap();
        assert_eq!((all.len(), all[0].since_ms), (1, 5));
        let other = Op::Pull {
            tag: "v3.3.0".into(),
            number: 1141,
        };
        assert_eq!(watches.register(REPO, other, 9).unwrap(), Registered::New);
    }

    #[test]
    fn a_repository_that_the_daemon_does_not_know_gets_no_watch() {
        let dir = daemon_dir();
        let error = Watches::new(dir.path())
            .register("o/other", pull(), 0)
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("does not serve o/other"),
            "{error:#}"
        );
        assert!(Watches::new(dir.path()).read().unwrap().is_empty());
    }

    #[test]
    fn a_repository_that_the_daemon_serves_only_is_known() {
        let dir = tempfile::tempdir().unwrap();
        let wave = Wave {
            repo: REPO.into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        remote::serve_in_file(dir.path(), &wave).unwrap();
        assert_eq!(wave_of(dir.path(), REPO).unwrap(), wave);
        assert!(
            Watches::new(dir.path())
                .register(REPO, test_op(), 0)
                .is_ok()
        );
    }

    #[test]
    fn the_file_keeps_a_watch_that_an_old_riff_wrote_with_less() {
        let dir = daemon_dir();
        let text = r#"[{"repo":"o/r","op":{"kind":"deploy","tag":"v3.3.0"},"since_ms":7}]"#;
        std::fs::write(dir.path().join(FILE), text).unwrap();
        let all = Watches::new(dir.path()).read().unwrap();
        assert_eq!(all[0].op, deploy_op(None));
        assert_eq!(all[0].seen, Seen::default());
    }
    /// Waits until `done` holds: the daemon looks each 20 ms.
    async fn until(mut done: impl FnMut() -> bool) {
        for _ in 0..250 {
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the daemon did not do it in 5 seconds");
    }

    /// A daemon on `dir` that looks each 20 ms through `ground`.
    fn daemon(
        dir: &Path,
        ground: &Arc<Ground>,
    ) -> (
        Arc<crate::daemon::Daemon<crate::runtime::Fake>>,
        JoinHandle<()>,
    ) {
        let lookout: Arc<dyn Lookout> = ground.clone();
        let daemon = crate::daemon::Daemon::new(crate::runtime::Fake::default(), dir.to_owned())
            .unwrap()
            .with_lookout(lookout);
        let daemon = Arc::new(daemon);
        let task = daemon.watch_every(Duration::from_millis(20));
        (daemon, task)
    }

    /// A change of the fake forge, and the words of the message of the lead.
    type Ends = (Box<dyn Fn(&mut Fake)>, &'static str);

    /// What a command starts, and how its operation ends.
    struct Case {
        command: &'static str,
        op: Op,
        /// The forge when the operation succeeds, and the words of the
        /// message of the lead.
        ok: Ends,
        /// The same for a failure.
        bad: Ends,
        /// True when the watch stays after the news: a deploy that waits.
        stays_on_ok: bool,
    }

    fn cases() -> Vec<Case> {
        let tag = || "v3.3.0".to_owned();
        let wait = |f: &mut Fake| f.deploy = deploy(DeployState::Waiting, &[]);
        let stale = |state| Some(Stale { run: 9, state });
        let pass = |f: &mut Fake| f.deploy = deploy(DeployState::Passed, &[]);
        let fail = |f: &mut Fake| f.deploy = deploy(DeployState::Failed, &["Deploy"]);
        vec![
            Case {
                command: "riff release cut",
                op: pull(),
                ok: (Box::new(merged), "of v3.3.0 merged"),
                bad: (
                    Box::new(|f| f.gate = Gate::Ended("failure".into())),
                    "Gate of the release pull request #1140 failed",
                ),
                stays_on_ok: false,
            },
            Case {
                command: "riff release verify",
                op: pull(),
                ok: (Box::new(merged), "of v3.3.0 merged"),
                bad: (
                    Box::new(|f| f.candidate.state = "CLOSED".into()),
                    "closed with no merge",
                ),
                stays_on_ok: false,
            },
            Case {
                command: "riff release test",
                op: test_op(),
                ok: (
                    Box::new(|f| {
                        f.running.clear();
                        f.real = vec![("m1", true)];
                    }),
                    "passed on m1",
                ),
                bad: (
                    Box::new(|f| {
                        f.running.clear();
                        f.real = vec![("m1", false)];
                    }),
                    "failed on m1",
                ),
                stays_on_ok: false,
            },
            Case {
                command: "riff release publish",
                op: Op::Deploy {
                    tag: tag(),
                    stale: None,
                },
                ok: (Box::new(wait), "waits for a person"),
                bad: (Box::new(fail), "failed: Deploy"),
                stays_on_ok: true,
            },
            Case {
                command: "riff release approve",
                op: Op::Deploy {
                    tag: tag(),
                    stale: stale(DeployState::Waiting),
                },
                ok: (Box::new(pass), "passed"),
                bad: (Box::new(fail), "failed: Deploy"),
                stays_on_ok: false,
            },
            Case {
                command: "riff release retry",
                op: Op::Deploy {
                    tag: tag(),
                    stale: stale(DeployState::Failed),
                },
                ok: (Box::new(pass), "passed"),
                bad: (
                    // The run moved and failed again.
                    Box::new(|f| {
                        f.deploy = Some(Deploy {
                            run: 10,
                            ..deploy(DeployState::Failed, &["Deploy"]).unwrap()
                        });
                    }),
                    "failed: Deploy",
                ),
                stays_on_ok: false,
            },
            Case {
                command: "riff release rollback",
                op: Op::Deploy {
                    tag: "v3.2.0".into(),
                    stale: None,
                },
                ok: (Box::new(pass), "passed"),
                bad: (Box::new(fail), "failed: Deploy"),
                stays_on_ok: false,
            },
        ]
    }

    /// 01M5RGCW5NCKQYA61RM63914TX: for each command that starts an async
    /// operation, the daemon follows the watch that the command
    /// registered. A new daemon between the start and the end keeps the
    /// watch. The lead gets one message at the end, with a success and
    /// with a failure, and no second message.
    #[tokio::test]
    async fn the_daemon_wakes_the_lead_once_at_the_end_of_each_operation() {
        for ok in [true, false] {
            for case in cases() {
                let name = format!("{} {}", case.command, if ok { "ok" } else { "bad" });
                let dir = daemon_dir();
                let mut forge = open_pull();
                forge.running = vec!["m1"];
                forge.release_pr = Some(ReleasePr {
                    number: 1140,
                    url: String::new(),
                    merge: "m1".into(),
                    head: "h1".into(),
                    body: String::new(),
                });
                forge.since = vec!["m1"];
                forge.tags = vec!["v3.3.0".into(), "v3.2.0".into()];
                forge.deploy = match &case.op {
                    // The state that the command saw before.
                    Op::Deploy { stale: Some(s), .. } => deploy(s.state, &["Deploy"]),
                    _ => deploy(DeployState::Before, &[]),
                };
                let ground = Ground::new(forge);
                Watches::new(dir.path())
                    .register(REPO, case.op.clone(), 0)
                    .unwrap();

                // The operation runs, and the daemon starts again.
                let (first, task) = daemon(dir.path(), &ground);
                tokio::time::sleep(Duration::from_millis(100)).await;
                assert!(ground.told().is_empty(), "{name}: it runs");
                task.abort();
                drop(first);
                let (_second, _task) = daemon(dir.path(), &ground);

                let (change, words) = if ok { &case.ok } else { &case.bad };
                ground.forge(|f| change(f));
                until(|| !ground.told.lock().unwrap().is_empty()).await;
                // Some looks more: no second message.
                tokio::time::sleep(Duration::from_millis(150)).await;
                let told = ground.told();
                assert_eq!(told.len(), 1, "{name}: {told:?}");
                assert!(told[0].contains(words), "{name}: {told:?}");
                let stays = ok && case.stays_on_ok;
                assert_eq!(kept(dir.path()).len(), usize::from(stays), "{name}");
            }
        }
    }

    /// A daemon that starts again learns what a watch told, from the file.
    #[tokio::test]
    async fn a_new_daemon_does_not_tell_again_what_the_watch_told() {
        let dir = daemon_dir();
        let mut forge = Fake::ready("3.3.0");
        forge.deploy = deploy(DeployState::Waiting, &[]);
        let ground = Ground::new(forge);
        watch(dir.path(), deploy_op(None));
        let (first, task) = daemon(dir.path(), &ground);
        until(|| !ground.told.lock().unwrap().is_empty()).await;
        task.abort();
        drop(first);
        let (_second, _task) = daemon(dir.path(), &ground);
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            ground.told().len(),
            1,
            "the waiting deploy is told one time"
        );
        assert_eq!(ground.chats().len(), 1);
    }
}
