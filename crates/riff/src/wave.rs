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
//! [`crate::rollout::free_work`]: the pull requests that wait for a
//! verify first, then the free items of the current wave. It never
//! starts:
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
//!   branch to the lead, as `riff run` does.
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
//! world, with the fake runtime ([`crate::runtime::Fake`]).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::wire::{Kind, PullState, RiffState, StartReason, WORKER_ALIVE_EVERY};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::api::{self, Api};
use crate::daemon::{Daemon, Ended, Out, Record, Say, Start};
use crate::room::Slot;
use crate::run::{self, Facts};
use crate::runtime::Runtime;
use crate::session_image::Image;
use crate::pr::Verdict;
use crate::{identity, pr, rollout, terminal, text};

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
    /// The riff-server of the riff of the repository.
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
pub fn pick(look: &Look, running: &BTreeSet<String>, starts: &BTreeMap<String, u32>) -> Option<Work> {
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
fn spent<'a>(look: &'a Look, starts: &'a BTreeMap<String, u32>) -> impl Iterator<Item = &'a String> {
    look.free
        .iter()
        .filter(|item| starts.get(*item).copied().unwrap_or(0) >= STARTS)
}

/// A future of a [`World`].
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The forge and the riff of the waves: [`Live`], or a fake in a test.
pub trait World: Send + Sync {
    /// The facts of the wave now.
    fn look<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Look>>;

    /// The session image of the repository of the wave.
    fn image(&self, wave: &Wave) -> Result<String>;

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

    /// The steps after the end of the session of `record`.
    fn finish<'a>(&'a self, record: &'a Record, ended: Ended) -> BoxFuture<'a, Result<()>>;

    /// Removes the folder of each session of the wave whose item merged.
    /// `keep` names the folders of the sessions that run.
    fn sweep<'a>(&'a self, wave: &'a Wave, keep: &'a [PathBuf]) -> BoxFuture<'a, Result<()>>;

    /// Tells the lead of the wave `body`, as a message.
    fn tell<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, ()>;
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
        let world = daemon.world().clone();
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
    match start_job(&daemon, &wave, &work, slot).await {
        Ok(Some((record, ended))) => finish(daemon.clone(), record, ended).await,
        Ok(None) => println!("{}", text::wave_not_claimed(&wave.repo, &item)),
        Err(e) => {
            let error = format!("{e:#}");
            eprintln!("riff daemon: {}: {item}: {error}", wave.repo);
            daemon
                .world()
                .tell(&wave, &text::wave_job_failed(&item, &error))
                .await;
        }
    }
    lock(&taken).remove(&item);
    daemon.tell_changed();
}

/// The steps of a job up to the end of its session: the image, the
/// steps before the start, and the session. `None` when the claim
/// failed.
async fn start_job<R: Runtime + 'static>(
    daemon: &Arc<Daemon<R>>,
    wave: &Wave,
    work: &Work,
    slot: Slot,
) -> Result<Option<(Record, Ended)>> {
    let world = daemon.world().clone();
    let image = world.image(wave)?;
    daemon
        .have_image(&image, |line| println!("{line}"))
        .await?;
    let Some(start) = world.begin(wave, work, &image).await? else {
        return Ok(None);
    };
    let job = Job {
        wave: wave.clone(),
        work: work.clone(),
    };
    let (out, said) = Out::keep();
    let ended = tokio::spawn(ended_of(said));
    let record = daemon.launch(start, Some(job), slot, None, &out).await;
    drop(out);
    let ended = ended.await.unwrap_or(Ended {
        code: None,
        failed: true,
    });
    Ok(Some((record, ended)))
}

/// The end of a session from the lines that the daemon says to one of
/// its followers. A failed start, or no end, is a failed session.
async fn ended_of(mut said: mpsc::UnboundedReceiver<Say>) -> Ended {
    let failed = Ended {
        code: None,
        failed: true,
    };
    while let Some(say) = said.recv().await {
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
    let ended = ended_of(said).await;
    finish(daemon, record, ended).await;
}

/// The steps after the end of the session of `record`, as it `ended`.
pub(crate) async fn finish<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    record: Record,
    ended: Ended,
) {
    if let Err(e) = daemon.world().finish(&record, ended).await {
        eprintln!("riff daemon: {}: {e:#}", record.live.name);
    }
    daemon.tell_changed();
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
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| anyhow!("{e}"))?
}

/// The branch of the issue `issue`.
fn branch_of(issue: u64) -> String {
    run::branch(&format!("issue-{issue}"))
}

impl Live {
    /// Starts the keep-alive of the claim of the session `me`.
    fn keep_alive(&self, api: Api, me: SessionUri, session: &str) {
        let task = tokio::spawn(async move { api::keep_alive(&api, &me, WORKER_ALIVE_EVERY).await });
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
        run::claude_folder(&claude, &clone)?;
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
                let mut text =
                    text::run_start(&wave.repo, issue, &branch, session, &frame, &milestone, earlier);
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
        })
    }

    /// The steps after the end of a build: the verify request for the
    /// open pull request of its branch, or a note that it has none.
    async fn after_build(&self, api: &Api, record: &Record, wave: &Wave, issue: u64) -> Result<()> {
        let branch = branch_of(issue);
        let repo = wave.repo.clone();
        let at = branch.clone();
        let pull = blocking(move || {
            Ok(pr::Gh::for_repo(&repo)
                .json::<run::Pull>(&[
                    "pr",
                    "view",
                    &at,
                    "--json",
                    "number,url,headRefOid,state",
                ])
                .ok()
                .filter(|p| p.state == "OPEN"))
        })
        .await?;
        let me = &record.me;
        let to = [riff_core::selector::Selector::lead(me.who().user(), &wave.repo)];
        let thread = wave.thread()?;
        let session = run::short(&record.live.session);
        let (body, kind) = match &pull {
            Some(pull) => (
                text::wave_verify_request(&record.live.item, pull.number, &branch, &pull.head_ref_oid, session),
                Kind::Message,
            ),
            None => (
                text::wave_no_pull(&record.live.item, &branch, &record.folder.join("clone")),
                Kind::Note,
            ),
        };
        api.post(me, Some(&thread), &to, &body, kind).await?;
        Ok(())
    }

    /// The steps after the end of a verify: the report of its result, as
    /// `riff verify` does. The folder of a verify that did not pass
    /// goes.
    async fn after_verify(
        &self,
        api: &Api,
        record: &Record,
        wave: &Wave,
        pull: u64,
        commit: &str,
    ) -> Result<()> {
        let me = &record.me;
        let thread = wave.thread()?;
        let lead = [riff_core::selector::Selector::lead(me.who().user(), &wave.repo)];
        let item = &record.live.item;
        let found = last_result(&record.folder.join(crate::daemon::EVENTS));
        let Some((verdict, result)) = found.as_deref().and_then(verdict) else {
            let _ = std::fs::remove_dir_all(&record.folder);
            api.post(me, Some(&thread), &lead, &text::wave_no_result(item, pull), Kind::Message)
                .await?;
            return Ok(());
        };
        let (repo, tested, shown) = (wave.repo.clone(), commit.to_owned(), result.clone());
        let reported = blocking(move || {
            pr::report(&pr::Gh::for_repo(&repo), &repo, pull, &tested, verdict, &shown)
        })
        .await;
        let reported = match reported {
            Ok(reported) => reported,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&record.folder);
                let body = text::wave_no_report(item, pull, &format!("{e:#}"));
                api.post(me, Some(&thread), &lead, &body, Kind::Message).await?;
                return Ok(());
            }
        };
        println!("{}", text::verify_reported(verdict, pull, &reported));
        if verdict == Verdict::Fail {
            let _ = std::fs::remove_dir_all(&record.folder);
        }
        let state = match verdict {
            Verdict::Pass => PullState::Passed,
            Verdict::Fail => PullState::Failed,
        };
        crate::look::tell_fact(api, me, reported.issue, pull, state).await;
        let to = pr::result_to(reported.issue, me, &api.who(me, false).await?);
        let body = text::verify_post(verdict, pull, &reported, &result);
        api.post(me, Some(&thread), &to, &body, Kind::Message).await?;
        Ok(())
    }
}

/// The line for the start text of a build when the verify of the pull
/// request of its branch failed: where the result is.
fn failed_verify(pulls: &[rollout::Pull], issue: u64) -> Option<String> {
    let pull = pulls
        .iter()
        .find(|p| rollout::branch_issue(&p.branch) == Some(issue))?;
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
        &["-c", "advice.detachedHead=false", "checkout", "-q", "--detach", commit],
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
            let holds = api.plan_show(&person, &thread).await?.holds.into_keys().collect();
            let repo = wave.repo.clone();
            let claimed: HashSet<String> = claims.iter().cloned().collect();
            let (free, pulls) = blocking(move || {
                let gh = pr::Gh::for_repo(&repo);
                let free = rollout::free_work(&gh, &repo, &claimed)?;
                let open = rollout::pulls(&gh, &repo)?;
                let mut pulls = BTreeMap::new();
                for issue in free.iter().filter_map(|item| verify_issue(item)) {
                    let Some(pull) = open
                        .iter()
                        .find(|p| rollout::branch_issue(&p.branch) == Some(issue))
                    else {
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
            .await?;
            Ok(Look {
                paused: false,
                free,
                claims,
                holds,
                pulls,
            })
        })
    }

    fn image(&self, wave: &Wave) -> Result<String> {
        Ok(Image::of_repository(&wave.main)?.to_string())
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

    fn finish<'a>(&'a self, record: &'a Record, ended: Ended) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let job = record
                .job
                .as_ref()
                .ok_or_else(|| anyhow!("the session {} has no job", record.live.name))?;
            let wave = &job.wave;
            let api = Api::new(&record.server).signed_in(Some(&record.live.session))?;
            let thread = wave.thread()?;
            println!(
                "{}",
                text::wave_ended(&wave.repo, &record.live.item, ended.code, ended.failed)
            );
            let done = match &job.work {
                Work::Build { issue } => self.after_build(&api, record, wave, *issue).await,
                Work::Verify { pull, commit, .. } => {
                    self.after_verify(&api, record, wave, *pull, commit).await
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

    fn tell<'a>(&'a self, wave: &'a Wave, body: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if lock(&self.told).get(&wave.repo).is_some_and(|t| t == body) {
                return;
            }
            let posted = async {
                let person = identity::person(&wave.place(None)?, &wave.server)?;
                let api = Api::new(&wave.server).signed_in(None)?;
                let to = [riff_core::selector::Selector::lead(person.who().user(), &wave.repo)];
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

