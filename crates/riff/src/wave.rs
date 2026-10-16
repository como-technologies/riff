//! Let riff run the wave (#719): the riff daemon starts one session for
//! each free item of the current wave, and one for each pull request
//! that waits for a verify (decisions 8 and 34 of #699).
//!
//! # Design
//!
//! `riff wave run` in a clone asks the daemon to run the wave of that
//! repository ([`Wave`]). The daemon keeps each wave in [`WAVES_FILE`]
//! in its folder, and starts its loop again after its own restart.
//! `riff wave stop` ends the loop: the daemon starts no new session
//! there, and the sessions that run go on.
//!
//! Each item has one session: it starts, does the item and ends
//! (01M4Z60B7NW4GHKGCYZZDG26R6). A verify is an item too: a new session
//! with a clone at the head commit of the pull request. No session
//! waits idle, and no session clears its context.
//!
//! ```mermaid
//! flowchart TD
//!     A["wait for room (crate::room)"] --> S["sweep: the clone of each merged item goes"]
//!     S --> L["look: the pause, the claims, the holds,<br/>the free work of gh, the Gate of each waiting pull request"]
//!     L --> P{"pick"}
//!     P -- "no work" --> W["give the room back; wait for an end, or LOOK_EVERY"] --> A
//!     P -- "an item" --> J["a job: claim, clone, start, wait for the end,<br/>the steps after the end, release"]
//!     J -.-> A
//! ```
//!
//! # The pick
//!
//! [`pick`] takes the first item of the free work of
//! [`crate::rollout::free_work`]: a fix item of a broken `main` first
//! ([`crate::rollout::BROKEN_MAIN`], 01M51QEYS538XA58KDRY844T52), then
//! the pull requests that wait for a verify, then the free items of the
//! current wave. It never starts:
//!
//! - an item while the riff or the repository is paused;
//! - a held item (01M4ZV3B6M7PZRJ39XTXBZP3ZH);
//! - an item that a session claims, or that a job of the wave runs;
//! - the verify of an item while a session holds the item: its author
//!   still works. The verify is a new session: an author never verifies
//!   its own item (01M4ZV3B6NKPEZ5FNE27X95GF0);
//! - a verify before the Gate of the head commit is done;
//! - an item that started [`STARTS`] times in this run of the wave. The
//!   lead gets one message for it.
//!
//! # A job
//!
//! A job does the steps of the host of `riff run` itself
//! ([`crate::run`]), with the sign-in of the person on this machine: a
//! new worker session on riff-server 1.x, the claim, the keep-alive,
//! the clone, the start text, and the start of the container in the
//! room that the loop took. The [`Record`] of the session holds the
//! [`Job`], so a new daemon goes on with the keep-alive and does the
//! steps after the end ([`crate::daemon::Daemon::recover`]).
//!
//! After the end ([`Live::finish`]):
//!
//! - A build sends a verify request for the open pull request of its
//!   branch to the lead, as `riff run` does. When the issue has the
//!   label [`crate::rollout::NO_VERIFY`], or a lead holds `verify-ITEM`,
//!   the lead gets a note with the reason in place of the verify request
//!   ([`crate::run::no_verify`], 01M51KH0H7J35PEKMHP5KQB84T).
//! - A verify reads the result from the last turn of `claude` in
//!   `events.jsonl`: its first line is `PASS` or `FAIL` ([`verdict`]).
//!   The daemon reports it as `riff verify` does: the comment on the
//!   pull request, the status `riff/verify` of the commit, and the
//!   result to the holder of the item and to the lead.
//! - Then the daemon releases the item and ends the worker session.
//!
//! # The clones
//!
//! The clone of a build stays until the merge of its pull request
//! (01M4Z81ARSTAK8HR96S084SDFD). At each look, the sweep removes the
//! folder of each session of the repository that does not run, when the
//! pull request of its branch merged. For a pull request with
//! `Refs #N`, it first adds the comment `Merged in #PR (COMMIT)` to the
//! issue, as the verifier of a worker does, so the item is not free work
//! again. The folder of a verify that did not pass goes at its end.
//!
//! # The world
//!
//! The loop reaches the forge and riff-server through a [`World`]. The
//! daemon uses [`Live`]: `gh` and riff-server 1.x. A test gives a fake
//! world, with the fake runtime ([`crate::runtime::Fake`]). The daemon
//! also keeps the session of the lead alive through it, and follows the
//! wakes of a session for `riff relay watch` ([`World::wake`]).
//!
//! # With no riff-server
//!
//! A wave with an empty riff-server (`riff wave run --peers`) and a job
//! of `riff run --peers` (`run_one`) use the world of the riff of
//! peers of the daemon ([`peers::Peers`], 01M519Z4RWHWP3TKNP4T5GKKE9).
//! The daemon picks the world of each wave and of each session by its
//! riff-server (`Daemon::world_of`). The look reads the
//! pause and the holds from the `plan` records and the claims from
//! `git ls-remote` (01M519Z4F7Z87BXQYT7F8EABCN). A job claims its item on
//! the forge ([`crate::forge_claim`]). After the end, it sends the verify
//! request or the result of a verify to the lead as a `message` record
//! (`EndTalk`), and then releases the claim. The steps after the end
//! of the riff of peers and of riff-server are one code: `EndTalk`
//! picks where they talk.
//!
//! ```mermaid
//! flowchart LR
//!     W["a wave or riff run --peers"] --> S{"its riff-server"}
//!     S -- "a URL" --> L["Live: riff-server 1.x and gh"]
//!     S -- "empty" --> P["peers::Peers: the riff of peers, the forge and gh"]
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::wire::{HoldInfo, Kind, PullState, RiffState, StartReason, WORKER_ALIVE_EVERY};
use serde::{Deserialize, Serialize};
use tokio::io::{BufReader, Lines};
use tokio::net::unix::OwnedReadHalf;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::api::{self, Api};
use crate::daemon::{Daemon, Ended, Out, Record, Say, Start};
use crate::pr::Verdict;
use crate::room::Slot;
use crate::run::{self, Facts};
use crate::runtime::Runtime;
use crate::talk::Talk;
use crate::{identity, pr, rollout, session_image, terminal, text};

pub mod peers;

/// The file of the waves that the daemon runs, in its folder.
pub const WAVES_FILE: &str = "waves.json";

/// How often a wave looks for free work when nothing ends.
pub const LOOK_EVERY: Duration = Duration::from_secs(60);

/// The most starts of one item in one run of a wave.
pub const STARTS: u32 = 3;

/// A wave that the daemon runs: a repository, and where it is on this
/// machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wave {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// The main clone of the repository on this machine.
    pub main: PathBuf,
    /// The riff-server of the riff of the repository. Empty for a wave
    /// with no riff-server: the riff of peers of the daemon
    /// ([`peers::Peers`]).
    pub server: String,
    /// The name of this host in the URI of a session.
    pub host: String,
}

impl Wave {
    /// The place of a session of the wave in the worktree `worktree`.
    ///
    /// ```
    /// use riff::wave::Wave;
    ///
    /// let wave = Wave {
    ///     repo: "o/r".into(),
    ///     main: "/src/r".into(),
    ///     server: "http://127.0.0.1:1".into(),
    ///     host: "thelio".into(),
    /// };
    /// assert_eq!(riff::identity::place_text(&wave.place(Some("issue-12"))?), "thelio/o/r#issue-12");
    /// assert_eq!(wave.thread()?.to_string(), "o/r");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn place(&self, worktree: Option<&str>) -> Result<Place> {
        let worktree = worktree.map(|w| format!("#{w}")).unwrap_or_default();
        identity::place_from_text(&format!("{}/{}{worktree}", self.host, self.repo))
    }

    /// The thread of the repository.
    pub fn thread(&self) -> Result<ThreadName> {
        self.place(None)?
            .default_thread()
            .ok_or_else(|| anyhow!("{}", text::RUN_NO_REPO))
    }
}

/// The waves in [`WAVES_FILE`] of the folder `dir` of the daemon, by
/// repository. With no file, none.
pub fn read_waves(dir: &Path) -> Result<BTreeMap<String, Wave>> {
    let path = dir.join(WAVES_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("cannot read the waves in {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Writes `waves` to [`WAVES_FILE`] in the folder `dir`, in place of
/// the old file.
pub fn write_waves(dir: &Path, waves: &BTreeMap<String, Wave>) -> Result<()> {
    let path = dir.join(WAVES_FILE);
    let tmp = path.with_extension("json.new");
    std::fs::write(&tmp, serde_json::to_vec_pretty(waves)?)
        .with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))
}

/// `riff wave run`: asks the daemon in the folder `dir` to run the
/// current wave of the repository of the place `here`, on the
/// riff-server `server` (01M4ZV3B6J89KJQ0F14YWTZMJQ). An empty `server`
/// is `riff wave run --peers`: the wave has no riff-server.
pub async fn ask_run(dir: &Path, here: &Place, server: &str) -> Result<()> {
    if here.default_thread().is_none() {
        bail!("{}", text::RUN_NO_REPO);
    }
    let (main, _) = run::main_clone(&identity::working_dir()?)?;
    // The repository pins its session image: each session needs it.
    session_image::merged(&main, |line| println!("{line}"))?;
    let repo = here.repo_text();
    let wave = Wave {
        repo: repo.clone(),
        main,
        server: server.to_owned(),
        host: here.host().to_owned(),
    };
    crate::daemon::ask_done(dir, crate::daemon::Ask::Wave { wave }, |line| {
        println!("{line}");
    })
    .await?;
    println!("{}", text::wave_asked(&repo));
    Ok(())
}

/// `riff wave stop`: asks the daemon in the folder `dir` to start no new
/// session for the wave of the repository of the place `here`.
pub async fn ask_stop(dir: &Path, here: &Place) -> Result<()> {
    if here.default_thread().is_none() {
        bail!("{}", text::RUN_NO_REPO);
    }
    let repo = here.repo_text();
    let ask = crate::daemon::Ask::WaveStop { repo: repo.clone() };
    crate::daemon::ask_done(dir, ask, |line| println!("{line}")).await?;
    println!("{}", text::wave_stops(&repo));
    Ok(())
}

/// What a session of a wave does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Work {
    /// The build of the issue on its branch.
    Build {
        /// The issue.
        issue: u64,
    },
    /// The verify of the pull request `pull` of the issue, at the commit
    /// `commit`.
    Verify {
        /// The issue.
        issue: u64,
        /// The pull request.
        pull: u64,
        /// The head commit that the verify tests.
        commit: String,
    },
}

impl Work {
    /// The work item: `issue-N` or `verify-issue-N`.
    ///
    /// ```
    /// use riff::wave::Work;
    ///
    /// assert_eq!(Work::Build { issue: 12 }.item(), "issue-12");
    /// let verify = Work::Verify { issue: 12, pull: 40, commit: "1a2b".into() };
    /// assert_eq!(verify.item(), "verify-issue-12");
    /// ```
    pub fn item(&self) -> String {
        match self {
            Work::Build { issue } => format!("issue-{issue}"),
            Work::Verify { issue, .. } => format!("verify-issue-{issue}"),
        }
    }

    /// The issue.
    pub fn issue(&self) -> u64 {
        match self {
            Work::Build { issue } | Work::Verify { issue, .. } => *issue,
        }
    }
}

/// The job of a session that the daemon started for a wave. It is in the
/// [`Record`] of the session, with no secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    /// The wave.
    pub wave: Wave,
    /// The work.
    pub work: Work,
}

/// A pull request that waits for a verify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// The pull request.
    pub pull: u64,
    /// Its head commit.
    pub commit: String,
    /// True when the Gate of the head is done: passed, or ended.
    pub gate_done: bool,
}

/// The facts of a wave at one look.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Look {
    /// True when the riff or the repository is paused.
    pub paused: bool,
    /// The free work, in order: `verify-issue-N`, then `issue-N`.
    pub free: Vec<String>,
    /// The items of the repository that a session claims.
    pub claims: BTreeSet<String>,
    /// The held items of the repository.
    pub holds: BTreeSet<String>,
    /// The pull requests that wait for a verify, by issue.
    pub pulls: BTreeMap<u64, Waiting>,
}

/// The issue of `verify-issue-N`.
fn verify_issue(item: &str) -> Option<u64> {
    item.strip_prefix("verify-issue-")?.parse().ok()
}

/// The next work of a wave, from the facts of `look`, the items that its
/// jobs run (`running`), and the number of starts of each item
/// (`starts`). See "The pick" in the design.
///
/// ```
/// use std::collections::{BTreeMap, BTreeSet};
/// use riff::wave::{Look, Waiting, Work, pick};
///
/// let look = Look {
///     free: vec!["verify-issue-9".into(), "issue-12".into(), "issue-13".into()],
///     holds: ["issue-12".to_owned()].into(),
///     claims: ["issue-9".to_owned()].into(),
///     pulls: [(9, Waiting { pull: 40, commit: "1a2b".into(), gate_done: true })].into(),
///     ..Look::default()
/// };
/// let none = BTreeSet::new();
/// // The author of 9 still holds it, and 12 is held.
/// assert_eq!(pick(&look, &none, &BTreeMap::new()), Some(Work::Build { issue: 13 }));
/// let look = Look { claims: BTreeSet::new(), ..look };
/// assert_eq!(
///     pick(&look, &none, &BTreeMap::new()),
///     Some(Work::Verify { issue: 9, pull: 40, commit: "1a2b".into() })
/// );
/// assert_eq!(pick(&Look { paused: true, ..look }, &none, &BTreeMap::new()), None);
/// ```
pub fn pick(
    look: &Look,
    running: &BTreeSet<String>,
    starts: &BTreeMap<String, u32>,
) -> Option<Work> {
    if look.paused {
        return None;
    }
    look.free.iter().find_map(|item| {
        let busy = |item: &String| running.contains(item) || look.claims.contains(item);
        if look.holds.contains(item) || busy(item) {
            return None;
        }
        if starts.get(item).copied().unwrap_or(0) >= STARTS {
            return None;
        }
        if let Some(issue) = verify_issue(item) {
            // The author of the item still works on it.
            if busy(&format!("issue-{issue}")) {
                return None;
            }
            let waiting = look.pulls.get(&issue).filter(|w| w.gate_done)?;
            return Some(Work::Verify {
                issue,
                pull: waiting.pull,
                commit: waiting.commit.clone(),
            });
        }
        let issue = item.strip_prefix("issue-")?.parse().ok()?;
        Some(Work::Build { issue })
    })
}

/// The items of the free work of `look` that reached [`STARTS`].
fn spent<'a>(
    look: &'a Look,
    starts: &'a BTreeMap<String, u32>,
) -> impl Iterator<Item = &'a String> {
    look.free
        .iter()
        .filter(|item| starts.get(*item).copied().unwrap_or(0) >= STARTS)
}

/// A future of a [`World`].
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The wait before the next connection of the wakes of a session.
const WAKE_RETRY: Duration = Duration::from_secs(1);

/// The forge and the riff of the waves: [`Live`], or a fake in a test.
pub trait World: Send + Sync {
    /// The facts of the wave now.
    fn look<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Look>>;

    /// The session image of the repository of the wave: the merged pin
    /// of its main clone ([`session_image::merged`]).
    fn image<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<String>>;

    /// The steps before the start: the checks, the claim, the clone and
    /// the start text. `None` when another session holds the item, or
    /// the item is held. After the claim, an error releases the item.
    fn begin<'a>(
        &'a self,
        wave: &'a Wave,
        work: &'a Work,
        image: &'a str,
    ) -> BoxFuture<'a, Result<Option<Start>>>;

    /// Goes on with the claim of the session of `record` after a new
    /// start of the daemon: its keep-alive.
    fn resume(&self, record: &Record);

    /// The steps after the end of the session of `record`. It gives a
    /// line for each step, for `riff run` (`run_one`).
    fn finish<'a>(&'a self, record: &'a Record, ended: Ended)
    -> BoxFuture<'a, Result<Vec<String>>>;

    /// Removes the folder of each session of the wave whose item merged.
    /// `keep` names the folders of the sessions that run.
    fn sweep<'a>(&'a self, wave: &'a Wave, keep: &'a [PathBuf]) -> BoxFuture<'a, Result<()>>;

    /// Tells the lead of the wave `body`, as a message.
    fn tell<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, ()>;

    /// Ends the session of `record` on riff-server, and stops its
    /// keep-alive: the end of the lead.
    fn leave<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, ()>;

    /// Waits for the next wake of the session of `record` on riff-server,
    /// and gives its line, as `riff watch --once` prints it.
    fn wake<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<String>>;
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The items that the daemon runs for the wave `wave`: its jobs before
/// the start (`taken`), and its live sessions.
fn running<R: Runtime + 'static>(
    daemon: &Daemon<R>,
    wave: &Wave,
    taken: &Mutex<BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut items = lock(taken).clone();
    items.extend(
        daemon
            .records()
            .into_iter()
            .filter(|r| r.job.as_ref().is_some_and(|j| j.wave.repo == wave.repo))
            .map(|r| r.live.item),
    );
    items
}

/// The loop of the wave `wave` in `daemon`, until the daemon stops it.
/// `taken` holds the items of its jobs before the start, and `word` what
/// it did last.
pub(crate) async fn run<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    wave: Wave,
    taken: Arc<Mutex<BTreeSet<String>>>,
    word: Arc<Mutex<String>>,
) {
    let say = |text: String| *lock(&word) = text;
    let mut starts: BTreeMap<String, u32> = BTreeMap::new();
    let mut told: BTreeSet<String> = BTreeSet::new();
    loop {
        let slot = daemon
            .room()
            .admit(
                &text::wave_slot(&wave.repo),
                || daemon.live_count(),
                |full| say(text::wave_full(full)),
            )
            .await;
        let world = match daemon.world_of(&wave.server) {
            Ok(world) => world,
            Err(e) => {
                drop(slot);
                say(text::wave_no_look(&format!("{e:#}")));
                tokio::time::sleep(LOOK_EVERY).await;
                continue;
            }
        };
        let keep: Vec<PathBuf> = daemon.records().into_iter().map(|r| r.folder).collect();
        if let Err(e) = world.sweep(&wave, &keep).await {
            eprintln!("riff daemon: {}: {e:#}", wave.repo);
        }
        let next = match world.look(&wave).await {
            Ok(look) => {
                for item in spent(&look, &starts) {
                    if told.insert(item.clone()) {
                        world.tell(&wave, &text::wave_spent(item, STARTS)).await;
                    }
                }
                let next = pick(&look, &running(&daemon, &wave, &taken), &starts);
                say(match &next {
                    _ if look.paused => text::WAVE_PAUSED.into(),
                    Some(work) => text::wave_started(&work.item()),
                    None => text::WAVE_NO_WORK.into(),
                });
                next
            }
            Err(e) => {
                say(text::wave_no_look(&format!("{e:#}")));
                None
            }
        };
        match next {
            Some(work) => {
                let item = work.item();
                *starts.entry(item.clone()).or_default() += 1;
                lock(&taken).insert(item);
                tokio::spawn(job(daemon.clone(), wave.clone(), work, slot, taken.clone()));
            }
            None => {
                drop(slot);
                tokio::select! {
                    () = tokio::time::sleep(LOOK_EVERY) => {}
                    () = daemon.changed() => {}
                }
            }
        }
    }
}

/// One job of the wave `wave`: the work `work` in the room of `slot`.
async fn job<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    wave: Wave,
    work: Work,
    slot: Slot,
    taken: Arc<Mutex<BTreeSet<String>>>,
) {
    let item = work.item();
    match start_job(&daemon, &wave, &work, slot, None, None).await {
        Ok(Some((record, ended))) => {
            finish(daemon.clone(), record, ended).await;
        }
        Ok(None) => println!("{}", text::wave_not_claimed(&wave.repo, &item)),
        Err(e) => {
            let error = format!("{e:#}");
            eprintln!("riff daemon: {}: {item}: {error}", wave.repo);
            if let Ok(world) = daemon.world_of(&wave.server) {
                world
                    .tell(&wave, &text::wave_job_failed(&item, &error))
                    .await;
            }
        }
    }
    lock(&taken).remove(&item);
    daemon.tell_changed();
}

/// `riff run ITEM --peers` ([`crate::daemon::Ask::Run`],
/// 01M519Z4RWHWP3TKNP4T5GKKE9, 01M526Z1SVG3ZK38GH51AHCRHP): one job of
/// `wave` with no riff-server,
/// with the image `image`. It waits for room in the limit of the host,
/// and does the job as a wave does: the claim on the forge, the clone,
/// the session, then the steps after the end. It says each line, each
/// event and the end of the session on `out`, then a line for each
/// step after the end, and [`Say::Done`]. A [`crate::daemon::Then::Stop`]
/// on `from` stops the container: the daemon still does the steps after
/// the end.
pub(crate) async fn run_one<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    wave: Wave,
    work: Work,
    image: String,
    mut from: Lines<BufReader<OwnedReadHalf>>,
    out: &Out,
) {
    let item = work.item();
    if !wave.server.is_empty() {
        out.say(Say::Failed {
            error: text::RUN_ONE_PEERS_ONLY.into(),
        });
        return;
    }
    let Some(slot) = daemon.room_for(&item, &mut from, out).await else {
        return;
    };
    let follower = Some((from, out.clone()));
    match start_job(&daemon, &wave, &work, slot, Some(&image), follower).await {
        Ok(Some((record, ended))) => {
            for text in finish(daemon.clone(), record, ended).await {
                out.say(Say::Line { text });
            }
            out.say(Say::Done);
        }
        Ok(None) => out.say(Say::Failed {
            error: text::run_held_on_forge(&item),
        }),
        Err(e) => out.say(Say::Failed {
            error: format!("{e:#}"),
        }),
    }
    daemon.tell_changed();
}

/// A connection of `riff run` that follows a job: the lines that it
/// sends, and the [`Out`] of its answers.
type Follower = (Lines<BufReader<OwnedReadHalf>>, Out);

/// The steps of a job up to the end of its session: the image, the
/// steps before the start, and the session. `None` when the claim
/// failed. With no `image`, the job takes the image that the wave pins.
/// A `follower` gets each line of the session, and can stop it.
async fn start_job<R: Runtime + 'static>(
    daemon: &Arc<Daemon<R>>,
    wave: &Wave,
    work: &Work,
    slot: Slot,
    image: Option<&str>,
    follower: Option<Follower>,
) -> Result<Option<(Record, Ended)>> {
    let world = daemon.world_of(&wave.server)?;
    let image = match image {
        Some(image) => image.to_owned(),
        None => world.image(wave).await?,
    };
    daemon.have_image(&image, |line| println!("{line}")).await?;
    let Some(start) = world.begin(wave, work, &image).await? else {
        return Ok(None);
    };
    let job = Job {
        wave: wave.clone(),
        work: work.clone(),
    };
    let (from, to) = match follower {
        Some((from, to)) => (Some(from), Some(to)),
        None => (None, None),
    };
    let (out, said) = Out::keep();
    let ended = tokio::spawn(ended_of(said, to));
    let record = daemon
        .launch(start, Some(job), Some(slot), from, &out)
        .await;
    drop(out);
    let ended = ended.await.unwrap_or(Ended {
        code: None,
        failed: true,
    });
    Ok(Some((record, ended)))
}

/// The end of a session from the lines that the daemon says to one of
/// its followers. A failed start, or no end, is a failed session. It
/// gives each line to `to` too, when there is one.
async fn ended_of(mut said: mpsc::UnboundedReceiver<Say>, to: Option<Out>) -> Ended {
    let failed = Ended {
        code: None,
        failed: true,
    };
    while let Some(say) = said.recv().await {
        if let Some(to) = &to {
            to.say(say.clone());
        }
        match say {
            Say::Ended(ended) => return ended,
            Say::Failed { error } => {
                eprintln!("riff daemon: {error}");
                return failed;
            }
            _ => {}
        }
    }
    failed
}

/// Waits for the end of the session of `record` that a new daemon took
/// up, on the lines `said`, and does the steps after the end.
pub(crate) async fn after<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    record: Record,
    said: mpsc::UnboundedReceiver<Say>,
) {
    let ended = ended_of(said, None).await;
    finish(daemon, record, ended).await;
}

/// The steps after the end of the session of `record`, as it `ended`,
/// with the world of its riff. It gives the line of each step. A session
/// with no riff-server waits for the riff of the daemon: a job that
/// ended while no daemon ran (01M526Z1Z80JFAS0GS04PVF1KG).
pub(crate) async fn finish<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    record: Record,
    ended: Ended,
) -> Vec<String> {
    let world = loop {
        match daemon.world_of(&record.server) {
            Err(_) if record.on_peers() => tokio::time::sleep(Duration::from_secs(1)).await,
            world => break world,
        }
    };
    let lines = match world {
        Ok(world) => world.finish(&record, ended).await,
        Err(e) => Err(e),
    };
    daemon.tell_changed();
    lines.unwrap_or_else(|e| {
        let error = format!("{e:#}");
        eprintln!("riff daemon: {}: {error}", record.live.name);
        vec![error]
    })
}

/// The text of the last result of `claude` in the file `events` of a
/// session: the `result` of its last turn.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let events = dir.path().join("events.jsonl");
/// std::fs::write(&events, concat!(
///     r#"{"type":"result","result":"first"}"#, "\n",
///     r#"{"type":"assistant"}"#, "\n",
///     r#"{"type":"result","result":"PASS\n\nAll good."}"#, "\n",
/// ))?;
/// assert_eq!(riff::wave::last_result(&events).as_deref(), Some("PASS\n\nAll good."));
/// assert_eq!(riff::wave::last_result(&dir.path().join("none")), None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn last_result(events: &Path) -> Option<String> {
    let text = std::fs::read_to_string(events).ok()?;
    text.lines().rev().find_map(|line| {
        let event: serde_json::Value = serde_json::from_str(line).ok()?;
        (event["type"] == "result")
            .then(|| event["result"].as_str().map(str::to_owned))
            .flatten()
    })
}

/// The verdict of the result of a verify session, and the result after
/// its verdict line. The verdict is the first line that holds only
/// `PASS` or `FAIL`, also with marks of markdown around it.
///
/// ```
/// use riff::pr::Verdict;
/// use riff::wave::verdict;
///
/// assert_eq!(verdict("PASS\n\n- Done.\n"), Some((Verdict::Pass, "- Done.".into())));
/// assert_eq!(verdict("I checked it.\n\n**FAIL**\n1. fails\n"), Some((Verdict::Fail, "1. fails".into())));
/// assert_eq!(verdict("## PASS"), Some((Verdict::Pass, String::new())));
/// assert_eq!(verdict("It may PASS."), None);
/// ```
pub fn verdict(result: &str) -> Option<(Verdict, String)> {
    let mut lines = result.lines();
    let verdict = lines.by_ref().find_map(|line| {
        match line.trim_matches(|c: char| c.is_whitespace() || "#*_`:".contains(c)) {
            "PASS" => Some(Verdict::Pass),
            "FAIL" => Some(Verdict::Fail),
            _ => None,
        }
    })?;
    let rest: Vec<&str> = lines.collect();
    Some((verdict, rest.join("\n").trim().to_owned()))
}

/// The forge and the riff of the waves on this machine: `gh` with the
/// sign-in of the person, riff-server 1.x with the sign-in of riff, and
/// git.
#[derive(Default)]
pub struct Live {
    /// The keep-alive of the claim of each session, by session ID.
    alive: Mutex<HashMap<String, JoinHandle<()>>>,
    /// The last text that the lead of each wave got: a text goes one
    /// time in a row.
    told: Mutex<HashMap<String, String>>,
}

/// Runs `work` on a thread that may block: `gh` and git.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| anyhow!("{e}"))?
}

/// The branch of the issue `issue`.
fn branch_of(issue: u64) -> String {
    run::branch(&format!("issue-{issue}"))
}

/// The steps after the end of a build, with `gh` for the forge: the
/// verify request for the open pull request of its branch, or a note
/// that it has none. When the issue has the label
/// [`rollout::NO_VERIFY`], or a lead holds `verify-ITEM`, the lead gets
/// a note with the reason in place of the verify request, as `riff run`
/// does ([`run::no_verify`], 01M51KH0H7J35PEKMHP5KQB84T).
async fn after_build(
    gh: pr::Gh,
    riff: &EndTalk<'_>,
    record: &Record,
    wave: &Wave,
    issue: u64,
) -> Result<Vec<String>> {
    let branch = branch_of(issue);
    let (view, at) = (gh.clone(), branch.clone());
    let pull = blocking(move || {
        Ok(view
            .json::<run::Pull>(&["pr", "view", &at, "--json", "number,url,headRefOid,state"])
            .ok()
            .filter(|p| p.state == "OPEN"))
    })
    .await?;
    let me = &record.me;
    let to = [riff_core::selector::Selector::lead(
        me.who().user(),
        &wave.repo,
    )];
    let thread = wave.thread()?;
    let item = &record.live.item;
    let session = run::short(&record.live.session);
    let mut lines = Vec::new();
    let (body, kind) = match &pull {
        Some(pull) => match riff.no_verify(me, &thread, gh, item, issue).await? {
            Some(why) => (
                text::wave_no_verify(
                    item,
                    pull.number,
                    &branch,
                    &pull.head_ref_oid,
                    session,
                    &why,
                ),
                Kind::Note,
            ),
            None => (
                text::wave_verify_request(item, pull.number, &branch, &pull.head_ref_oid, session),
                Kind::Message,
            ),
        },
        None => (
            text::wave_no_pull(item, &branch, &record.folder.join("clone")),
            Kind::Note,
        ),
    };
    match &pull {
        Some(pull) => {
            lines.push(text::run_pull(pull.number, &pull.url));
            lines.push(match kind {
                Kind::Message => text::run_verify_sent(item, pull.number),
                _ => text::run_lead_noted(item),
            });
        }
        None => lines.push(text::run_no_pull(&branch)),
    }
    riff.talk()
        .post(me, Some(&thread), &to, &body, kind)
        .await?;
    Ok(lines)
}

/// Where the steps after the end of a job talk: riff-server 1.x, or the
/// riff of peers of the daemon (01M519Z4RWHWP3TKNP4T5GKKE9).
pub(crate) enum EndTalk<'a> {
    /// riff-server 1.x, with the sign-in of the person.
    Server(&'a Api),
    /// The riff of peers of the daemon: its records.
    Peers(crate::peer::talk::Session, Arc<crate::peer::talk::Hub>),
}

impl EndTalk<'_> {
    /// The riff of peers of `hub`.
    pub(crate) fn peers(hub: &Arc<crate::peer::talk::Hub>) -> EndTalk<'static> {
        EndTalk::Peers(crate::peer::talk::Session::new(hub.clone()), hub.clone())
    }

    /// The calls of the tools that talk.
    fn talk(&self) -> &dyn Talk {
        match self {
            EndTalk::Server(api) => *api,
            EndTalk::Peers(talk, _) => talk,
        }
    }

    /// The holds of the repository `thread`, by item, as `me`.
    async fn holds(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
    ) -> Result<BTreeMap<String, HoldInfo>> {
        match self {
            EndTalk::Server(api) => Ok(api.plan_show(me, thread).await?.holds),
            EndTalk::Peers(_, hub) => Ok(hub.read(|state| peers::holds_in(state, thread))),
        }
    }

    /// [`run::no_verify`] now: the labels of the issue with `gh`, then
    /// the holds of `thread`, as `me`. It asks for the holds only for an
    /// issue with no label. A fault of `gh` shows on stderr, and riff
    /// goes on to the holds.
    async fn no_verify(
        &self,
        me: &SessionUri,
        thread: &ThreadName,
        gh: pr::Gh,
        item: &str,
        issue: u64,
    ) -> Result<Option<String>> {
        let labelled = blocking(move || run::labelled(&gh, issue))
            .await
            .unwrap_or_else(|e| {
                eprintln!("riff: {e:#}");
                false
            });
        if labelled {
            return Ok(run::no_verify(item, issue, true, &BTreeMap::new()));
        }
        let holds = self.holds(me, thread).await?;
        Ok(run::no_verify(item, issue, false, &holds))
    }
}

/// The steps after the end of a verify: the report of its result, as
/// `riff verify` does. The folder of a verify that did not pass goes. It
/// gives a line for each step.
async fn after_verify(
    riff: &EndTalk<'_>,
    record: &Record,
    wave: &Wave,
    pull: u64,
    commit: &str,
) -> Result<Vec<String>> {
    let me = &record.me;
    let thread = wave.thread()?;
    let lead = [riff_core::selector::Selector::lead(
        me.who().user(),
        &wave.repo,
    )];
    let item = &record.live.item;
    let talk = riff.talk();
    let found = last_result(&record.folder.join(crate::daemon::EVENTS));
    let Some((verdict, result)) = found.as_deref().and_then(verdict) else {
        let _ = std::fs::remove_dir_all(&record.folder);
        let body = text::wave_no_result(item, pull);
        talk.post(me, Some(&thread), &lead, &body, Kind::Message)
            .await?;
        return Ok(vec![body]);
    };
    let (repo, tested, shown) = (wave.repo.clone(), commit.to_owned(), result.clone());
    let reported = blocking(move || {
        pr::report(
            &pr::Gh::for_repo(&repo),
            &repo,
            pull,
            &tested,
            verdict,
            &shown,
        )
    })
    .await;
    let reported = match reported {
        Ok(reported) => reported,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&record.folder);
            let body = text::wave_no_report(item, pull, &format!("{e:#}"));
            talk.post(me, Some(&thread), &lead, &body, Kind::Message)
                .await?;
            return Ok(vec![body]);
        }
    };
    let line = text::verify_reported(verdict, pull, &reported);
    println!("{line}");
    if verdict == Verdict::Fail {
        let _ = std::fs::remove_dir_all(&record.folder);
    }
    if let EndTalk::Server(api) = riff {
        let state = match verdict {
            Verdict::Pass => PullState::Passed,
            Verdict::Fail => PullState::Failed,
        };
        crate::look::tell_fact(api, me, reported.issue, pull, state).await;
    }
    let to = pr::result_to(reported.issue, me, &talk.who(me, false).await?);
    let body = text::verify_post(verdict, pull, &reported, &result);
    talk.post(me, Some(&thread), &to, &body, Kind::Message)
        .await?;
    Ok(vec![line])
}

impl Live {
    /// Starts the keep-alive of the claim of the session `me`, a session
    /// in a container: it is live while the keep-alive comes
    /// (01M510MG0CKDXAKF6YJ768K7E8).
    fn keep_alive(&self, api: Api, me: SessionUri, session: &str) {
        let task = tokio::spawn(async move {
            api::keep_alive_in_container(&api, &me, WORKER_ALIVE_EVERY).await;
        });
        if let Some(old) = lock(&self.alive).insert(session.to_owned(), task) {
            old.abort();
        }
    }

    /// Releases `item` of the session `me`, ends the session, and stops
    /// its keep-alive.
    async fn end(&self, api: &Api, me: &SessionUri, thread: &ThreadName, item: &str) {
        if let Err(e) = api.release(me, thread, item).await {
            eprintln!("riff daemon: {e:#}");
        }
        if let Some(session) = me.who().session()
            && let Some(task) = lock(&self.alive).remove(session)
        {
            task.abort();
        }
        let _ = api.end(me).await;
    }

    /// The steps before the start after the claim: the clone, the
    /// Claude folder, the spec and the start text.
    #[allow(clippy::too_many_arguments)]
    async fn prepare(
        wave: &Wave,
        work: &Work,
        image: &str,
        host: &run::Host,
        milestone: Option<String>,
        session: &str,
        me: &SessionUri,
    ) -> Result<Start> {
        let item = work.item();
        let folder = run::sessions_dir()?.join(format!("{item}-{}", run::short(session)));
        let clone = folder.join("clone");
        let claude = folder.join("claude");
        let auto = crate::permissions::Trust::of(me.place().repo(), &wave.server);
        run::claude_folder(&claude, &clone, &host.home.join(".claude.json"), &auto)?;
        let (main, common) = run::main_clone(&wave.main)?;
        let start = match work {
            Work::Build { issue } => {
                let issue = *issue;
                let branch = branch_of(issue);
                let (repo, at) = (wave.repo.clone(), clone.clone());
                let earlier = {
                    let (main, branch) = (main.clone(), branch.clone());
                    blocking(move || run::make_clone(&main, &at, &branch)).await?
                };
                let failed = blocking(move || {
                    let pulls = rollout::pulls(&pr::Gh::for_repo(&repo), &repo)?;
                    Ok(failed_verify(&pulls, issue))
                })
                .await
                .unwrap_or_default();
                let milestone = milestone.context("a build has a milestone")?;
                let frame = pr::body(hygiene::Link::Closes(issue), "SUMMARY", &milestone)?;
                let mut text = text::run_start(
                    &wave.repo, issue, &branch, session, &frame, &milestone, earlier,
                );
                if let Some(failed) = failed {
                    text.push_str("\n\n");
                    text.push_str(&failed);
                }
                text
            }
            Work::Verify {
                issue,
                pull,
                commit,
            } => {
                let (at, commit_at) = (clone.clone(), commit.clone());
                let main_at = main.clone();
                blocking(move || clone_at(&main_at, &at, &commit_at)).await?;
                text::wave_verify_start(&wave.repo, *issue, *pull, commit)
            }
        };
        let existing = |p: PathBuf| p.exists().then_some(p);
        let facts = Facts {
            session: session.to_owned(),
            item: item.clone(),
            image: image.to_owned(),
            home: host.home.clone(),
            user: host.user.clone(),
            clone,
            objects: common.join("objects"),
            claude,
            credentials: host.credentials.clone(),
            gitconfig: existing(host.home.join(".gitconfig")),
            gh_config: existing(host.home.join(".config/gh")),
        };
        let secrets = std::iter::once(host.gh_token.clone())
            .chain(host.sign_in.secrets.iter().cloned())
            .collect();
        Ok(Start {
            session: session.to_owned(),
            item,
            folder,
            spec: run::spec(&facts),
            start,
            values: vec![("GH_TOKEN".to_owned(), host.gh_token.clone())],
            secrets,
            server: wave.server.clone(),
            me: me.clone(),
            terminal: false,
            claim: None,
        })
    }
}

/// The line for the start text of a build when the verify of the pull
/// request of its issue failed: where the result is.
fn failed_verify(pulls: &[rollout::Pull], issue: u64) -> Option<String> {
    let pull = pulls.iter().find(|p| p.issue() == Some(issue))?;
    (pull.verify() == Some(rollout::Verify::Failed)).then(|| {
        let url = pull
            .verify_status()
            .and_then(|c| c.url.as_deref())
            .map(text::safe)
            .unwrap_or_default();
        text::wave_failed_verify(pull.number, &pull.head, &url)
    })
}

/// Makes the clone of a verify at `commit`, detached: the head commit of
/// the pull request.
fn clone_at(main: &Path, clone: &Path, commit: &str) -> Result<()> {
    let remote = run::output("git", main, &["remote", "get-url", "origin"])?;
    let top = main.to_string_lossy();
    let path = clone.to_string_lossy();
    run::output(
        "git",
        main,
        &["clone", "-q", "--reference", &top, "--", &remote, &path],
    )?;
    run::output(
        "git",
        clone,
        &[
            "-c",
            "advice.detachedHead=false",
            "checkout",
            "-q",
            "--detach",
            commit,
        ],
    )?;
    Ok(())
}

/// A pull request as `gh pr view --json number,state,body,mergeCommit`
/// gives it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Merged {
    number: u64,
    state: String,
    body: String,
    #[serde(default)]
    merge_commit: Option<Oid>,
}

/// A commit, as `gh` gives it.
#[derive(Debug, Deserialize)]
struct Oid {
    oid: String,
}

/// The comments of an issue, as `gh issue view --json comments` gives
/// them.
#[derive(Debug, Deserialize)]
struct Comments {
    comments: Vec<rollout::Comment>,
}

/// The work item of the folder `name` of a session: `ITEM-ID8`.
///
/// ```
/// assert_eq!(riff::wave::item_of_folder("issue-12-1a2b3c4d"), Some("issue-12"));
/// assert_eq!(riff::wave::item_of_folder("verify-issue-12-1a2b3c4d"), Some("verify-issue-12"));
/// assert_eq!(riff::wave::item_of_folder("other"), None);
/// ```
pub fn item_of_folder(name: &str) -> Option<&str> {
    let (item, id) = name.rsplit_once('-')?;
    (id.len() == 8 && id.chars().all(|c| c.is_ascii_hexdigit())).then_some(item)
}

/// Sweeps the folder `folder` of a session of the repository `repo` that
/// does not run: see "The clones" in the design.
fn sweep_folder(repo: &str, folder: &Path) -> Result<()> {
    let Some(name) = folder.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    let Some(item) = item_of_folder(name) else {
        return Ok(());
    };
    let (verify, issue) = match verify_issue(item) {
        Some(issue) => (true, issue),
        None => match item.strip_prefix("issue-").and_then(|n| n.parse().ok()) {
            Some(issue) => (false, issue),
            None => return Ok(()),
        },
    };
    let clone = folder.join("clone");
    let origin = run::output("git", &clone, &["remote", "get-url", "origin"]).ok();
    let theirs = origin
        .as_deref()
        .and_then(identity::parse_remote)
        .map(|(owner, name)| format!("{owner}/{name}"));
    if theirs.as_deref() != Some(repo) {
        return Ok(());
    }
    let gh = pr::Gh::for_repo(repo);
    let Ok(pull) = gh.json::<Merged>(&[
        "pr",
        "view",
        &branch_of(issue),
        "--json",
        "number,state,body,mergeCommit",
    ]) else {
        return Ok(());
    };
    match pull.state.as_str() {
        "MERGED" => {
            let refs = format!("Refs #{issue}");
            if pull.body.lines().any(|l| l.trim() == refs) {
                let comments: Comments =
                    gh.json(&["issue", "view", &issue.to_string(), "--json", "comments"])?;
                let said = format!("Merged in #{}", pull.number);
                if !comments
                    .comments
                    .iter()
                    .any(|c| c.body.trim_start().starts_with(&said))
                {
                    let commit = pull.merge_commit.map(|c| c.oid).unwrap_or_default();
                    let body = text::wave_merged(pull.number, &commit);
                    gh.run(
                        &["issue", "comment", &issue.to_string(), "--body-file", "-"],
                        Some(&body),
                    )?;
                }
            }
            std::fs::remove_dir_all(folder)
                .with_context(|| format!("cannot remove {}", folder.display()))?;
            println!("{}", text::wave_swept(item, folder));
        }
        "CLOSED" if verify => {
            std::fs::remove_dir_all(folder)
                .with_context(|| format!("cannot remove {}", folder.display()))?;
        }
        _ => {}
    }
    Ok(())
}

/// The free work of the repository `repo` with `gh`, with the items that
/// a session claims (`claims`) and the held items (`holds`): the items,
/// and the pull request of each verify among them, with the Gate of its
/// head ([`rollout::free_work`]).
async fn free_work(
    repo: &str,
    claims: &BTreeSet<String>,
    holds: &BTreeSet<String>,
) -> Result<(Vec<String>, BTreeMap<u64, Waiting>)> {
    let repo = repo.to_owned();
    let claimed: HashSet<String> = claims.iter().cloned().collect();
    let held: HashSet<String> = holds.iter().cloned().collect();
    blocking(move || {
        let gh = pr::Gh::for_repo(&repo);
        let free = rollout::free_work(&gh, &repo, &claimed, &held)?.items;
        let open = rollout::pulls(&gh, &repo)?;
        let mut pulls = BTreeMap::new();
        for issue in free.iter().filter_map(|item| verify_issue(item)) {
            let Some(pull) = open.iter().find(|p| p.issue() == Some(issue)) else {
                continue;
            };
            let gate = pr::gate_of(&gh, &repo, &pull.head)?;
            pulls.insert(
                issue,
                Waiting {
                    pull: pull.number,
                    commit: pull.head.clone(),
                    gate_done: matches!(gate, pr::Gate::Passed | pr::Gate::Ended(_)),
                },
            );
        }
        Ok((free, pulls))
    })
    .await
}

impl World for Live {
    fn look<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Look>> {
        Box::pin(async move {
            let person = identity::person(&wave.place(None)?, &wave.server)?;
            let thread = wave.thread()?;
            let api = Api::new(&wave.server).signed_in(None)?;
            if api.pauses(&person).await?.state == RiffState::Paused {
                return Ok(Look {
                    paused: true,
                    ..Look::default()
                });
            }
            let claims: BTreeSet<String> = api
                .who(&person, false)
                .await?
                .iter()
                .filter(|s| s.uri.place().repo_text() == wave.repo)
                .flat_map(|s| s.uri.claims().iter().cloned())
                .collect();
            let holds: BTreeSet<String> = api
                .plan_show(&person, &thread)
                .await?
                .holds
                .into_keys()
                .collect();
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
        let (main, repo) = (wave.main.clone(), wave.repo.clone());
        Box::pin(blocking(move || {
            let image = session_image::merged(&main, |line| println!("{repo}: {line}"))?;
            Ok(image.to_string())
        }))
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
            let place = wave.place(Some(&item))?;
            let me = identity::agent(&place, &session, &wave.server)?;
            let thread = wave.thread()?;
            let api = Api::new(&wave.server).signed_in(Some(&session))?;
            api.start(&me, StartReason::Process, true).await?;
            let claimed = api.claim(&me, &thread, &item).await?;
            if !claimed.granted {
                let _ = api.end(&me).await;
                return Ok(None);
            }
            self.keep_alive(api.clone(), me.clone(), &session);
            println!("{}", text::wave_claimed(&wave.repo, &item, &session));
            match Self::prepare(wave, work, image, &host, milestone, &session, &me).await {
                Ok(start) => Ok(Some(start)),
                Err(e) => {
                    self.end(&api, &me, &thread, &item).await;
                    Err(e)
                }
            }
        })
    }

    fn resume(&self, record: &Record) {
        match Api::new(&record.server).signed_in(Some(&record.live.session)) {
            Ok(api) => self.keep_alive(api, record.me.clone(), &record.live.session),
            Err(e) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
        }
    }

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
            // The steps after the end outlast a restart of riff-server,
            // as in `riff run` (01M4ZQVVGQYXTAVRB69C7YPC3Y).
            let api = Api::new(&record.server)
                .signed_in(Some(&record.live.session))?
                .with_budget(run::END_BUDGET);
            let thread = wave.thread()?;
            println!(
                "{}",
                text::wave_ended(&wave.repo, &record.live.item, ended.code, ended.failed)
            );
            let riff = EndTalk::Server(&api);
            let done = match &job.work {
                Work::Build { issue } => {
                    let gh = pr::Gh::for_repo(&wave.repo);
                    after_build(gh, &riff, record, wave, *issue).await
                }
                Work::Verify { pull, commit, .. } => {
                    after_verify(&riff, record, wave, *pull, commit).await
                }
            };
            self.end(&api, &record.me, &thread, &record.live.item).await;
            done
        })
    }

    fn sweep<'a>(&'a self, wave: &'a Wave, keep: &'a [PathBuf]) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let dir = run::sessions_dir()?;
            let keep = keep.to_vec();
            let repo = wave.repo.clone();
            blocking(move || {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    return Ok(());
                };
                for entry in entries.flatten() {
                    let folder = entry.path();
                    if keep.contains(&folder) || !folder.is_dir() {
                        continue;
                    }
                    if let Err(e) = sweep_folder(&repo, &folder) {
                        eprintln!("riff daemon: {e:#}");
                    }
                }
                Ok(())
            })
            .await
        })
    }

    fn leave<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if let Some(task) = lock(&self.alive).remove(&record.live.session) {
                task.abort();
            }
            match Api::new(&record.server).signed_in(Some(&record.live.session)) {
                Ok(api) => {
                    let _ = api.end(&record.me).await;
                }
                Err(e) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
            }
        })
    }

    fn wake<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            use futures::StreamExt;
            let api = Api::new(&record.server).signed_in(Some(&record.live.session))?;
            let me = &record.me;
            let wakes = api::follow(|| api.watch(me), WAKE_RETRY);
            tokio::pin!(wakes);
            loop {
                match wakes.next().await {
                    Some(Ok(wake)) => return Ok(text::wake_line(&wake)),
                    Some(Err(e)) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
                    None => bail!("{}", text::DAEMON_GONE),
                }
            }
        })
    }

    fn tell<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if lock(&self.told).get(&wave.repo).is_some_and(|t| t == body) {
                return;
            }
            let posted = async {
                let person = identity::person(&wave.place(None)?, &wave.server)?;
                let api = Api::new(&wave.server).signed_in(None)?;
                let to = [riff_core::selector::Selector::lead(
                    person.who().user(),
                    &wave.repo,
                )];
                api.post(&person, Some(&wave.thread()?), &to, body, Kind::Message)
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
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    use super::*;
    use crate::daemon::dir_in;
    use crate::room::Limit;
    use crate::runtime::{Bind, Env, Fake, SessionSpec, Source};

    const IMAGE: &str = "localhost/riff-session:test";
    const GB: u64 = 1 << 30;

    /// A pull request of the fake forge.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Pr {
        /// It waits for a verify.
        Asked,
        /// The verify passed.
        Passed,
    }

    /// The forge and the riff of a test.
    #[derive(Debug, Default)]
    struct Forge {
        /// The open items of the wave, in order.
        items: Vec<u64>,
        holds: BTreeSet<String>,
        /// The claims: the item, and the session that holds it.
        claims: BTreeMap<String, String>,
        /// The pull requests, by issue.
        pulls: BTreeMap<u64, Pr>,
        /// Each claim of a session: the item, and the session.
        starts: Vec<(String, String)>,
        /// Each item after its end.
        finished: Vec<String>,
        /// Each message to the lead.
        told: Vec<String>,
        sessions: u64,
        /// A build ends with no pull request.
        no_pull: bool,
    }

    /// A fake [`World`]: a forge in memory, a claim that fails for a
    /// held item as on riff-server, and a session folder in `root`.
    struct Fakes {
        root: PathBuf,
        forge: Mutex<Forge>,
    }

    impl World for Fakes {
        fn look<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<Look>> {
            Box::pin(async move {
                let forge = lock(&self.forge);
                let claimed = |item: &str| forge.claims.contains_key(item);
                let verifies = forge
                    .pulls
                    .iter()
                    .filter(|(n, pr)| **pr == Pr::Asked && !claimed(&format!("verify-issue-{n}")))
                    .map(|(n, _)| format!("verify-issue-{n}"));
                let builds = forge
                    .items
                    .iter()
                    .filter(|n| !forge.pulls.contains_key(n) && !claimed(&format!("issue-{n}")))
                    .map(|n| format!("issue-{n}"));
                Ok(Look {
                    paused: false,
                    free: verifies.chain(builds).collect(),
                    claims: forge.claims.keys().cloned().collect(),
                    holds: forge.holds.clone(),
                    pulls: forge
                        .pulls
                        .keys()
                        .map(|n| {
                            let waiting = Waiting {
                                pull: 40 + n,
                                commit: format!("c{n}"),
                                gate_done: true,
                            };
                            (*n, waiting)
                        })
                        .collect(),
                })
            })
        }

        fn image<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<String>> {
            Box::pin(async { Ok(IMAGE.into()) })
        }

        fn begin<'a>(
            &'a self,
            _: &'a Wave,
            work: &'a Work,
            image: &'a str,
        ) -> BoxFuture<'a, Result<Option<Start>>> {
            Box::pin(async move {
                let item = work.item();
                let mut forge = lock(&self.forge);
                if forge.holds.contains(&item) || forge.claims.contains_key(&item) {
                    return Ok(None);
                }
                forge.sessions += 1;
                let session = format!("{:08x}", forge.sessions);
                forge.claims.insert(item.clone(), session.clone());
                forge.starts.push((item.clone(), session.clone()));
                let name = format!("riff-{item}-{session}");
                let folder = self.root.join("sessions").join(&name);
                let clone = folder.join("clone");
                std::fs::create_dir_all(&clone)?;
                let spec = SessionSpec {
                    name,
                    image: image.into(),
                    home: "/home/mike".into(),
                    workdir: clone.clone(),
                    binds: vec![Bind {
                        source: Source::Path(clone.clone()),
                        target: clone,
                        write: true,
                    }],
                    env: vec![Env::Host("GH_TOKEN".into())],
                    memory: "8g".into(),
                    cpus: 4,
                    command: vec!["claude".into()],
                };
                let me = format!("riff://mike@thelio/o/r?session={session}#{item}").parse()?;
                Ok(Some(Start {
                    session,
                    item,
                    folder,
                    spec,
                    start: "Do it.".into(),
                    values: vec![("GH_TOKEN".into(), "gho_secret1".into())],
                    secrets: vec!["gho_secret1".into()],
                    server: "http://127.0.0.1:1".into(),
                    me,
                    terminal: false,
                    claim: None,
                }))
            })
        }

        fn resume(&self, _: &Record) {}

        fn finish<'a>(
            &'a self,
            record: &'a Record,
            _: Ended,
        ) -> BoxFuture<'a, Result<Vec<String>>> {
            Box::pin(async move {
                let mut forge = lock(&self.forge);
                let job = record.job.as_ref().expect("a job");
                match job.work {
                    Work::Build { .. } if forge.no_pull => None,
                    Work::Build { issue } => forge.pulls.insert(issue, Pr::Asked),
                    Work::Verify { issue, .. } => forge.pulls.insert(issue, Pr::Passed),
                };
                forge.claims.remove(&record.live.item);
                forge.finished.push(record.live.item.clone());
                Ok(Vec::new())
            })
        }

        fn sweep<'a>(&'a self, _: &'a Wave, _: &'a [PathBuf]) -> BoxFuture<'a, Result<()>> {
            Box::pin(async { Ok(()) })
        }

        fn tell<'a>(&'a self, _: &'a Wave, body: &'a str) -> BoxFuture<'a, ()> {
            Box::pin(async move { lock(&self.forge).told.push(body.to_owned()) })
        }

        fn leave<'a>(&'a self, _: &'a Record) -> BoxFuture<'a, ()> {
            Box::pin(async {})
        }

        fn wake<'a>(&'a self, _: &'a Record) -> BoxFuture<'a, Result<String>> {
            Box::pin(std::future::pending())
        }
    }

    /// A daemon on the fake runtime with the fake world, in a temp
    /// folder.
    struct Here {
        _root: tempfile::TempDir,
        fake: Fake,
        world: Arc<Fakes>,
        daemon: Arc<Daemon<Fake>>,
    }

    impl Here {
        /// The wave of the open `items`, with the limit `sessions` and
        /// a floor of 8 GB, and the free memory of `free` in GB.
        async fn new(items: &[u64], sessions: u32, free: Arc<AtomicU64>) -> Here {
            let root = tempfile::tempdir().unwrap();
            let dir = dir_in(root.path());
            let fake = Fake::default().with_image(IMAGE);
            let world = Arc::new(Fakes {
                root: root.path().to_owned(),
                forge: Mutex::new(Forge {
                    items: items.to_vec(),
                    ..Forge::default()
                }),
            });
            let memory: crate::room::Memory =
                Arc::new(move || Some(free.load(Ordering::SeqCst) * GB));
            let relay = root.path().join("static-riff");
            crate::secrets::mock_store();
            let daemon = Daemon::open(fake.clone(), dir.clone())
                .await
                .unwrap()
                .with_memory(memory)
                .with_world(world.clone())
                .with_relay(crate::release::fake_riff(&relay, None));
            Limit {
                sessions,
                floor_gb: 8,
            }
            .write(&dir)
            .unwrap();
            Here {
                _root: root,
                fake,
                world,
                daemon: Arc::new(daemon),
            }
        }

        fn run(&self) {
            self.daemon
                .run_wave(Wave {
                    repo: "o/r".into(),
                    main: "/src/r".into(),
                    server: "http://127.0.0.1:1".into(),
                    host: "thelio".into(),
                })
                .unwrap();
        }

        fn forge(&self) -> MutexGuard<'_, Forge> {
            lock(&self.world.forge)
        }

        /// Waits until `done` holds. It tells the wave to look again
        /// while it waits. At most `limit` sessions live the whole time.
        async fn until(&self, limit: usize, done: impl Fn(&Here) -> bool) {
            let started = Instant::now();
            while !done(self) {
                assert!(self.daemon.live_count() <= limit, "more than {limit} live");
                assert!(started.elapsed() < isolated::HANG, "it did not happen");
                self.daemon.tell_changed();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(self.daemon.live_count() <= limit, "more than {limit} live");
        }

        /// The name of the container of `item`, while it runs.
        fn running(&self, item: &str) -> Option<String> {
            self.daemon
                .records()
                .into_iter()
                .find(|r| r.live.item == item)
                .map(|r| r.live.name)
        }

        /// Ends the live session of `item`.
        async fn end(&self, item: &str) {
            let name = self.running(item).expect("it runs");
            self.fake.stop(&name).await.unwrap();
        }

        /// Lets the wave look for a time. At most `limit` sessions live.
        async fn idle(&self, limit: usize) {
            for _ in 0..20 {
                self.daemon.tell_changed();
                tokio::time::sleep(Duration::from_millis(20)).await;
                assert!(self.daemon.live_count() <= limit, "more than {limit} live");
            }
        }
    }

    #[tokio::test]
    async fn the_limit_of_the_host_holds_while_the_wave_runs() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2, 3], 2, free).await;
        here.run();
        here.until(2, |h| h.daemon.live_count() == 2).await;
        here.idle(2).await;
        assert_eq!(here.forge().starts.len(), 2);
        // Each end makes room for the next work: the verify of an item,
        // or the next item. Two sessions live at most.
        for n in 1..=6 {
            here.until(2, |h| !h.daemon.records().is_empty()).await;
            let item = here.daemon.records()[0].live.item.clone();
            here.end(&item).await;
            here.until(2, |h| h.forge().finished.len() == n).await;
        }
        here.idle(2).await;
        let forge = here.forge();
        assert!(
            forge.pulls.values().all(|pr| *pr == Pr::Passed),
            "{forge:?}"
        );
        assert_eq!(
            forge.starts.len(),
            6,
            "one session for each item and each verify"
        );
    }

    /// 01M51STPHXSSZMHF9T7Z85097R: the wave of the daemon takes the
    /// merged pin of its main clone, not the pin of its working tree.
    #[tokio::test]
    async fn the_wave_takes_the_merged_pin_of_its_main_clone() {
        let pin = |n: &str| format!("ghcr.io/o/i@sha256:{}", n.repeat(64));
        let config = |n: &str| format!("[session]\nimage = \"{}\"\n", pin(n));
        let (_dir, main, _origin) = session_image::test_clone(&config("0"));
        std::fs::write(main.join(session_image::FILE), config("1")).unwrap();
        let wave = Wave {
            repo: "acme/app".into(),
            main,
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        assert_eq!(Live::default().image(&wave).await.unwrap(), pin("0"));
    }

    #[tokio::test]
    async fn the_wave_starts_nothing_while_the_free_memory_is_below_the_floor() {
        let free = Arc::new(AtomicU64::new(2));
        let here = Here::new(&[1], 3, free.clone()).await;
        here.run();
        here.idle(0).await;
        assert!(here.forge().starts.is_empty());
        free.store(9, Ordering::SeqCst);
        here.until(1, |h| h.daemon.live_count() == 1).await;
    }

    #[tokio::test]
    async fn a_held_item_never_starts() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        here.forge().holds.insert("issue-1".into());
        here.forge().holds.insert("verify-issue-2".into());
        here.run();
        here.until(3, |h| h.running("issue-2").is_some()).await;
        here.end("issue-2").await;
        here.until(3, |h| h.forge().finished == ["issue-2"]).await;
        here.idle(3).await;
        let forge = here.forge();
        let started: Vec<&str> = forge.starts.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(started, ["issue-2"], "no held item and no held verify");
        assert_eq!(forge.pulls.get(&2), Some(&Pr::Asked), "the verify waits");
    }

    #[tokio::test]
    async fn an_author_never_verifies_its_own_item() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        // The author opens its pull request, and works on: no verify
        // starts while it holds the item.
        here.forge().pulls.insert(1, Pr::Asked);
        here.idle(3).await;
        assert_eq!(here.forge().starts.len(), 1);
        here.end("issue-1").await;
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let forge = here.forge();
        let [(build, author), (verify, verifier)] = &forge.starts[..] else {
            panic!("{forge:?}");
        };
        assert_eq!(
            (build.as_str(), verify.as_str()),
            ("issue-1", "verify-issue-1")
        );
        assert_ne!(author, verifier, "the verify is a new session");
    }

    #[tokio::test]
    async fn an_item_starts_at_most_three_times_and_the_lead_hears_of_it() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        // Each session of the item ends with no pull request.
        here.forge().no_pull = true;
        here.run();
        for n in 1..=STARTS {
            here.until(3, |h| h.running("issue-1").is_some()).await;
            here.end("issue-1").await;
            here.until(3, |h| h.forge().finished.len() == n as usize)
                .await;
        }
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.starts.len(), 3);
        assert_eq!(forge.told, [text::wave_spent("issue-1", STARTS)]);
    }

    #[tokio::test]
    async fn a_stop_of_the_wave_starts_no_new_session_and_the_sessions_go_on() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 1, free).await;
        here.run();
        here.until(1, |h| h.running("issue-1").is_some()).await;
        assert!(here.daemon.stop_wave("o/r").unwrap());
        assert!(here.running("issue-1").is_some(), "the session goes on");
        here.end("issue-1").await;
        here.until(1, |h| h.forge().finished == ["issue-1"]).await;
        here.idle(1).await;
        assert_eq!(here.forge().starts.len(), 1);
        assert!(!here.daemon.stop_wave("o/r").unwrap());
    }

    #[tokio::test]
    async fn a_new_ask_with_another_main_clone_runs_the_wave_with_it() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 3, free).await;
        here.run();
        let other = Wave {
            repo: "o/r".into(),
            main: "/src/other".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        here.daemon.run_wave(other.clone()).unwrap();
        let kept = read_waves(here.daemon.dir()).unwrap();
        assert_eq!(kept.get("o/r"), Some(&other));
        assert_eq!(here.daemon.waves_shown().len(), 1);
        here.until(0, |h| h.daemon.waves_shown()[0].word == text::WAVE_NO_WORK)
            .await;
    }

    #[test]
    fn a_wave_goes_to_its_file_and_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_waves(dir.path()).unwrap().is_empty());
        let wave = Wave {
            repo: "o/r".into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        let waves: BTreeMap<String, Wave> = [("o/r".to_owned(), wave)].into();
        write_waves(dir.path(), &waves).unwrap();
        assert_eq!(read_waves(dir.path()).unwrap(), waves);
    }

    #[test]
    fn the_pick_skips_a_verify_until_its_gate_is_done_and_counts_the_starts() {
        let waiting = |gate_done| Waiting {
            pull: 40,
            commit: "c".into(),
            gate_done,
        };
        let look = Look {
            free: vec!["verify-issue-9".into(), "issue-12".into()],
            pulls: [(9, waiting(false))].into(),
            ..Look::default()
        };
        let none = BTreeSet::new();
        assert_eq!(
            pick(&look, &none, &BTreeMap::new()),
            Some(Work::Build { issue: 12 })
        );
        let spent: BTreeMap<String, u32> = [("issue-12".to_owned(), STARTS)].into();
        assert_eq!(pick(&look, &none, &spent), None);
        let running: BTreeSet<String> = ["issue-12".to_owned()].into();
        assert_eq!(pick(&look, &running, &BTreeMap::new()), None);
        let look = Look {
            pulls: [(9, waiting(true))].into(),
            ..look
        };
        assert_eq!(
            pick(&look, &none, &BTreeMap::new()),
            Some(Work::Verify {
                issue: 9,
                pull: 40,
                commit: "c".into()
            })
        );
        // The author of 9 runs in this daemon.
        let running: BTreeSet<String> = ["issue-9".to_owned()].into();
        assert_eq!(
            pick(&look, &running, &BTreeMap::new()),
            Some(Work::Build { issue: 12 })
        );
    }

    #[test]
    fn a_job_goes_into_the_record_and_comes_back() {
        let job = Job {
            wave: Wave {
                repo: "o/r".into(),
                main: "/src/r".into(),
                server: "http://127.0.0.1:1".into(),
                host: "thelio".into(),
            },
            work: Work::Verify {
                issue: 12,
                pull: 40,
                commit: "1a2b".into(),
            },
        };
        let text = serde_json::to_string(&job).unwrap();
        assert!(text.contains(r#""kind":"verify""#), "{text}");
        assert_eq!(serde_json::from_str::<Job>(&text).unwrap(), job);
    }

    /// The riff of a test of [`after_build`]: a riff-server in this
    /// process, the lead `mike/lead1` of `o/r`, and the session of a
    /// build of `issue-12` that ended.
    struct Built {
        api: Api,
        wave: Wave,
        lead: SessionUri,
        record: Record,
        dir: tempfile::TempDir,
    }

    impl Built {
        async fn new() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let server = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(async move {
                axum::serve(listener, riff_server::router()).await.unwrap();
            });
            let api = Api::new(&server);
            let wave = Wave {
                repo: "o/r".into(),
                main: "/src/r".into(),
                server: server.clone(),
                host: "thelio".into(),
            };
            let who = |session| riff_core::name::Who::new("mike", Some(session)).unwrap();
            let lead = SessionUri::new(who("lead1"), wave.place(None).unwrap());
            api.start(&lead, StartReason::Process, false).await.unwrap();
            let session = "9f8e7d6c-0000-4000-8000-000000000000";
            let me = SessionUri::new(who(session), wave.place(Some("issue-12")).unwrap());
            api.start(&me, StartReason::Process, true).await.unwrap();
            let dir = tempfile::tempdir().unwrap();
            let record = Record {
                live: crate::daemon::Live {
                    name: "riff-issue-12".into(),
                    session: session.into(),
                    item: "issue-12".into(),
                    image: IMAGE.into(),
                    since_ms: 0,
                    phase: Default::default(),
                    last: String::new(),
                },
                folder: dir.path().to_owned(),
                server,
                me,
                inputs: Vec::new(),
                closed: true,
                failed: false,
                job: Some(Job {
                    wave: wave.clone(),
                    work: Work::Build { issue: 12 },
                }),
                terminal: false,
                takeover: false,
                interrupt: false,
                claim: None,
            };
            Self {
                api,
                wave,
                lead,
                record,
                dir,
            }
        }

        /// A fake `gh`: the branch of issue 12 has the open pull request
        /// #40, and the issue has the label `no-verify` when `labelled`.
        fn gh(&self, labelled: bool) -> pr::Gh {
            let labels = if labelled {
                r#"[{"name":"no-verify"}]"#
            } else {
                "[]"
            };
            let script = format!(
                r#"#!/bin/sh
case "$*" in
"pr view worktree-issue-12 --json number,url,headRefOid,state")
    echo '{{"number":40,"url":"https://github.com/o/r/pull/40","headRefOid":"1a2b3c4d5e","state":"OPEN"}}' ;;
"issue view 12 --json number,labels")
    echo '{{"number":12,"labels":{labels}}}' ;;
*)
    echo "the fake gh has no reply for: $*" >&2
    exit 1 ;;
esac
"#
            );
            pr::Gh::at(isolated::write_program(self.dir.path().join("gh"), script))
        }

        /// Runs [`after_build`] with `gh`, and gives each message that the
        /// lead can read, with its kind.
        async fn after_build(&self, gh: pr::Gh) -> Vec<(Kind, String)> {
            after_build(
                gh,
                &EndTalk::Server(&self.api),
                &self.record,
                &self.wave,
                12,
            )
            .await
            .unwrap();
            let thread = self.wave.thread().unwrap();
            self.api
                .read(&self.lead, &thread, true)
                .await
                .unwrap()
                .into_iter()
                .map(|c| (c.message.kind, c.message.body))
                .collect()
        }
    }

    /// With no hold and no label, the lead gets the verify request
    /// (01M4ZV3B6Q0Y6AX0KSKC55S31R).
    #[tokio::test]
    async fn a_build_sends_the_verify_request_to_the_lead() {
        let riff = Built::new().await;
        let got = riff.after_build(riff.gh(false)).await;
        assert_eq!(
            got,
            [(
                Kind::Message,
                text::wave_verify_request(
                    "issue-12",
                    40,
                    "worktree-issue-12",
                    "1a2b3c4",
                    "9f8e7d6c"
                )
            )]
        );
    }

    /// A lead holds the verify of the item: the lead gets a note with the
    /// hold, and no verify request (01M51KH0H7J35PEKMHP5KQB84T).
    #[tokio::test]
    async fn a_build_whose_verify_is_held_posts_a_note_with_the_hold() {
        let riff = Built::new().await;
        let thread = riff.wave.thread().unwrap();
        riff.api
            .hold(&riff.lead, &thread, "verify-issue-12", "a scratch item")
            .await
            .unwrap();
        let got = riff.after_build(riff.gh(false)).await;
        assert_eq!(got.len(), 1, "{got:?}");
        let (kind, body) = &got[0];
        assert_eq!(*kind, Kind::Note);
        assert!(
            body.starts_with(
                "issue-12: the session of the wave ended, and the item is free. The riff daemon \
                 sends no verify request for PR #40, branch worktree-issue-12, commit 1a2b3c4 \
                 (session 9f8e7d6c): verify-issue-12 is held by the lead (the session \
                 mike/lead1) since "
            ),
            "{body}"
        );
        assert!(body.ends_with(": a scratch item"), "{body}");
        assert!(!body.contains("verify request:"), "{body}");
    }

    /// The issue has the label `no-verify`: the lead gets a note with the
    /// label, and no verify request (01M51KH0H7J35PEKMHP5KQB84T). The book
    /// shows the same note.
    #[tokio::test]
    async fn a_build_of_an_issue_with_the_label_no_verify_posts_a_note() {
        let riff = Built::new().await;
        let got = riff.after_build(riff.gh(true)).await;
        let note = text::wave_no_verify(
            "issue-12",
            40,
            "worktree-issue-12",
            "1a2b3c4",
            "9f8e7d6c",
            &text::no_verify_label(12),
        );
        assert_eq!(got, [(Kind::Note, note.clone())]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/how-it-works.md"
        ))
        .unwrap();
        assert!(book.contains(&format!("\n{note}\n")), "{note}");
    }
}
