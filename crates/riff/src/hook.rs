//! The Claude Code hooks.
//!
//! # Design
//!
//! A hook cannot call a tool. So the start hook cannot start the watch
//! itself. It adds context instead, and the session starts
//! `riff watch --once` as a background task of the Bash tool (R66).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant C as Claude Code
//!     participant H as riff hook session-start
//!     participant S as session
//!     participant W as riff watch --once
//!     C->>H: stdin: session_id, source
//!     H-->>C: stdout: additionalContext
//!     C->>S: context
//!     S->>W: Bash, run_in_background
//!     W-->>S: one wake, then exit
//!     S->>S: read
//!     S->>W: Bash, run_in_background (R171)
//! ```
//!
//! # Wake sources
//!
//! Tested in Claude Code on 2026-09-27:
//!
//! | Source | Ends | The session gets |
//! |---|---|---|
//! | Monitor tool | after 30 minutes at most | one notice for each line, and a notice at the end |
//! | Bash with `run_in_background` | when the command exits; a test task ran 20 minutes and more, past the 10-minute limit of a foreground call | one notice when the command exits |
//!
//! A notice wakes an idle session. In a turn, it comes with the result
//! of the next tool call. A Monitor task expires each 30 minutes, and a
//! busy session often starts it again only at the end of its turn. So
//! riff uses a background Bash task, which ends only on a wake. The
//! session reads, then starts the watch again at once. No message is
//! lost while no watch runs: a new watch wakes the session once if an
//! addressed message is unread (R49). So the session reads before it
//! starts the watch again.
//!
//! The context depends on the `source` of the start (R68):
//!
//! | Source | The session | The `start` call |
//! |---|---|---|
//! | `startup` | A new start. Follows the start routine (R54, R166). | `process` |
//! | `resume` | A new start. Continues, and claims again what it goes on with. | `resume` |
//! | `clear` | A new start. Keeps its riff session ID and its lead (R168). Follows the start routine. | `clear` |
//! | `compact` | Continues, with its claims. | none |
//!
//! At a new start, the hook sends the start call: the claims of the
//! session are free at once (01M3JEE1QQCFS5TMZW5N2DAD2D). The context names each
//! freed claim, and points to "Pick up dropped work" in the skill
//! (01M3JEE1SWR05DWQA5WQ8AXFTF). The call carries the reason of the
//! start, and the worker mark from `RIFF_WORKER` (01M3X9X9M079WGFPJZHNXH9VEP): see
//! [`Source::reason`]. A start with the reason `process` or `clear` is a
//! fresh context, so it lets a worker claim again after its last
//! release. A compaction is no new session: the hook sends no start, so
//! the session keeps its claims and its lead, and a worker that must
//! clear its context still must.
//!
//! The context also depends on the state of the riff
//! (01M3JCG48QPCNNTKW34FTR0AMR). The hook reads it from the server, and
//! waits at most [`STATE_WAIT`]:
//!
//! | State | A new session (`startup`, `clear`) | A session that continues |
//! |---|---|---|
//! | running | Picks a free item. | Continues. |
//! | paused | Claims nothing, says hello to the lead, sets its status to waiting. The lead tells its user. | Stops at its next step. |
//! | not known | Calls `whoami`, then acts on the state. | Continues. |
//!
//! The watch does not depend on the source. A watch that runs holds a
//! lock (R169, see [`crate::local`]). When the lock is held, the
//! context tells the session to keep that watch. Else it tells the
//! session to start one. So after `/clear`, the session keeps the watch
//! from before `/clear`: it runs for the same ID.
//!
//! A session that left the riff gets no context, and the hook makes no
//! call (01M3MEEFETT9A0DRWBKQTG77Z2, see [`crate::leave`]).
//!
//! The hook never stops a session start (R69). When it cannot find the
//! session, the context has no URI, and the hook still exits with
//! status 0.
//!
//! # A clone that is behind
//!
//! A clone that was not pulled starts its sessions with an old
//! `CLAUDE.md` and old project settings. So the hook runs `git fetch`
//! for at most [`FETCH_WAIT`], at the same time as it reads the state
//! of the riff. When the default branch is behind `origin`, the context
//! says so (01M3JN21T9C5GX6VX8N032JYWE). The hook does not pull. With
//! no remote, a remote that cannot be reached, or a slow fetch, the
//! context has no such line (01M3JN21WDXWTHDKXKQ80ZPYPK). See
//! [`behind`].
//!
//! # Earlier work
//!
//! A session can end with no notice, and its work stays on its pushed
//! branch and in its worktree. After the fetch, at a new start, the
//! context lists the earlier work of the clone that no live session
//! owns (01M3WFYETKXPWWE0R0EAKGCD1E). So the session does not have to
//! look for it. The fetch prunes, so a branch that the forge deleted
//! after its merge is not in the list. See [`crate::dropped`].
//!
//! # A session in a linked worktree
//!
//! tmux opens a new pane in the directory of the current pane. So a
//! session can start in the worktree of another session. At a new
//! start in a linked worktree, the hook looks for the other live
//! sessions in the same place, in the `who` of the server ([`Linked`]):
//!
//! | Other live session there | The context |
//! |---|---|
//! | yes | Names it and the main worktree. Stop, claim nothing, change no file, ask to start again in the main worktree (01M3MYQ299XKJE9X9FHWZ7JFM4). |
//! | no | Names the worktree, and points to "Pick up dropped work" (01M3MYQ2BFKS3KJ8DWNWDJKWB9). |
//! | not known | Names the worktree, and asks for a look at `who`. |
//!
//! In the main worktree, the context has no such line.
//!
//! # The tokens of a claim
//!
//! Each hook gets the path of the transcript of the session. The start
//! hook, the Stop hook and the end hook record it for the session
//! (01M3Y1YP15C7AT2N70BWQP8PE2). A new start and the end of a session
//! free each claim. So the start hook at a new start, and the end hook,
//! start `riff hook usage`, detached: it ends each open claim in the
//! marks of the session, and reports its tokens. See [`crate::usage`].
//!
//! ```
//! use riff::hook::{Source, StartInput};
//!
//! let input: StartInput = serde_json::from_str(r#"{"session_id":"a6cf","source":"clear"}"#)?;
//! assert_eq!(input.source, Source::Clear);
//! let context = riff::hook::start_context(None, input.source, true, None, &[]);
//! assert!(context.contains("claims are free"));
//! assert!(context.contains("Keep it."));
//! assert!(!context.contains("TaskStop"));
//! # Ok::<(), serde_json::Error>(())
//! ```

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use riff_core::build::Mismatch;
use riff_core::name::SessionUri;
use riff_core::wire::{Freed, RiffReply, SessionInfo, StartReason};
use serde::Deserialize;

/// The longest wait of the start hook for the state of the riff.
pub const STATE_WAIT: Duration = Duration::from_secs(3);

/// The longest wait of the start hook for `git fetch`
/// (01M3JN21T9C5GX6VX8N032JYWE).
pub const FETCH_WAIT: Duration = Duration::from_secs(2);

/// The default branch of a clone that is behind `origin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Behind {
    /// The main worktree of the clone, where the user pulls.
    pub main: PathBuf,
    /// The default branch, for example `main`.
    pub branch: String,
    /// How many commits of `origin` the branch does not have.
    pub commits: u64,
}

impl Behind {
    /// The line of the start context.
    ///
    /// ```
    /// use riff::hook::Behind;
    ///
    /// let behind = Behind { main: "/src/riff".into(), branch: "main".into(), commits: 1 };
    /// let line = behind.line();
    /// assert!(line.contains("1 commit behind origin/main"));
    /// assert!(line.contains("git -C /src/riff pull --ff-only"));
    /// assert!(line.contains("Do not pull yourself"));
    /// ```
    pub fn line(&self) -> String {
        let s = if self.commits == 1 { "" } else { "s" };
        format!(
            "- This clone is {} commit{s} behind origin/{}. So your CLAUDE.md and project \
             settings can be old. Ask your user to pull, through the lead when you are not the \
             lead: `git -C {} pull --ff-only`. Do not pull yourself.\n",
            self.commits,
            self.branch,
            self.main.display(),
        )
    }
}

/// Fetches `origin` in the clone of `dir` for at most `wait`
/// ([`crate::dropped::fetch`]), and tells
/// whether its default branch is behind (01M3JN21T9C5GX6VX8N032JYWE).
/// It is `None` when the branch is up to date, and when `dir` is not in
/// git, has no `origin`, or the fetch fails or takes longer than `wait`
/// (01M3JN21WDXWTHDKXKQ80ZPYPK).
///
/// The default branch is the one that `origin/HEAD` names, as `git
/// clone` sets it.
pub async fn behind(dir: &Path, wait: Duration) -> Option<Behind> {
    let head = git(
        dir,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .await?;
    let branch = head.strip_prefix("origin/")?.to_owned();
    if !crate::dropped::fetch(dir, wait).await {
        return None;
    }
    let range = format!("refs/heads/{branch}..refs/remotes/{head}");
    let commits = git(dir, &["rev-list", "--count", &range])
        .await?
        .parse()
        .ok()
        .filter(|&n| n > 0)?;
    let list = git(dir, &["worktree", "list", "--porcelain"]).await?;
    let main = list.lines().next()?.strip_prefix("worktree ")?.into();
    Some(Behind {
        main,
        branch,
        commits,
    })
}

/// A session that starts in a linked worktree, not in the main worktree
/// of its clone (01M3MYQ299XKJE9X9FHWZ7JFM4, 01M3MYQ2BFKS3KJ8DWNWDJKWB9).
/// tmux opens a new pane in the directory of the current pane, so a
/// session can start in the worktree of another session.
///
/// ```
/// use riff::hook::Linked;
///
/// let linked = Linked { path: "/src/riff/.claude/worktrees/issue-12".into(), main: "/src/riff".into() };
/// let other = "riff://mike@pangolin/como-technologies/riff?session=b4b9#issue-12".parse()?;
/// let line = linked.line(Some(&[other]));
/// assert!(line.contains("session=b4b9"));
/// assert!(line.contains("start this session again in the main worktree /src/riff"));
/// let line = linked.line(Some(&[]));
/// assert!(line.contains("Pick up dropped work"));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linked {
    /// The linked worktree where the session starts.
    pub path: PathBuf,
    /// The main worktree of the clone.
    pub main: PathBuf,
}

impl Linked {
    /// The line of the start context. `others` are the other live
    /// sessions in this worktree, or `None` when the hook could not
    /// read them.
    pub fn line(&self, others: Option<&[SessionUri]>) -> String {
        let (path, main) = (self.path.display(), self.main.display());
        let again = format!(
            "Call the riff tell tool with the session `lead`: ask your user to start this \
             session again in the main worktree {main}."
        );
        match others {
            Some([]) => format!(
                "- You started in the linked worktree {path}, not in the main worktree {main}. \
                 No live session works here. It can hold dropped work: see \"Pick up dropped \
                 work\" in the riff skill.\n"
            ),
            Some(others) => {
                let names: Vec<String> = others.iter().map(ToString::to_string).collect();
                format!(
                    "- Stop: you started in the worktree {path} of another live session: {}. \
                     Do not follow the start routine. Claim nothing, change no file here, and \
                     run no git command that writes. {again} Then wait.\n",
                    names.join(", ")
                )
            }
            None => format!(
                "- You started in the linked worktree {path}, not in the main worktree {main}. \
                 Call the riff who tool. When another session works here, claim nothing, \
                 change no file here, and {}\n",
                again.replacen("Call", "call", 1)
            ),
        }
    }
}

/// The linked worktree of `dir`, or `None` when `dir` is in the main
/// worktree or not in git.
pub async fn linked(dir: &Path) -> Option<Linked> {
    let abs = ["rev-parse", "--path-format=absolute"];
    let git_dir = git(dir, &[abs[0], abs[1], "--git-dir"]).await?;
    let common = git(dir, &[abs[0], abs[1], "--git-common-dir"]).await?;
    if git_dir == common {
        return None;
    }
    let path = git(dir, &["rev-parse", "--show-toplevel"]).await?.into();
    let list = git(dir, &["worktree", "list", "--porcelain"]).await?;
    let main = list.lines().next()?.strip_prefix("worktree ")?.into();
    Some(Linked { path, main })
}

/// The other live sessions that work in the place of `me`.
///
/// ```
/// use riff::hook::others_here;
/// use riff_core::wire::SessionInfo;
///
/// let info = |uri: &str, live| SessionInfo { uri: uri.parse().unwrap(), live, idle_secs: 0, status: None, worker: false, stopping: false, claims_secs: 0, must_clear: false, fresh_secs: None, state: Default::default() };
/// let me: riff_core::name::SessionUri = "riff://mike@pangolin/o/r?session=a1#issue-12".parse()?;
/// let who = [
///     info("riff://mike@pangolin/o/r?session=a1#issue-12", true),
///     info("riff://mike@pangolin/o/r?session=b2&claim=issue-12#issue-12", true),
///     info("riff://mike@pangolin/o/r?session=c3#issue-12", false),
///     info("riff://mike@pangolin/o/r?session=d4#issue-13", true),
///     info("riff://mike@thelio/o/r?session=e5#issue-12", true),
/// ];
/// let others = others_here(&me, &who);
/// assert_eq!(others.len(), 1);
/// assert_eq!(others[0].who().session(), Some("b2"));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn others_here(me: &SessionUri, who: &[SessionInfo]) -> Vec<SessionUri> {
    who.iter()
        .filter(|s| s.live && s.uri.who() != me.who() && s.uri.place() == me.place())
        .map(|s| s.uri.clone())
        .collect()
}

async fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// The part of the SessionEnd hook input that riff uses.
///
/// `riff mcp` sends the end call when it stops. The end hook sends it
/// too, so a session also leaves when `riff mcp` cannot. After `/clear`,
/// the session keeps its riff session ID (R168), so the hook does not
/// end it:
///
/// ```
/// use riff::hook::EndInput;
///
/// let input: EndInput = serde_json::from_str(r#"{"session_id":"a6cf","reason":"clear"}"#)?;
/// assert!(!input.ends_the_session());
/// let input: EndInput = serde_json::from_str(r#"{"session_id":"a6cf","reason":"prompt_input_exit"}"#)?;
/// assert!(input.ends_the_session());
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Debug, Default, Deserialize)]
pub struct EndInput {
    /// The session ID.
    pub session_id: Option<String>,
    /// Why the session ended, for example `clear`, `logout` or
    /// `prompt_input_exit`.
    #[serde(default)]
    pub reason: String,
    /// The transcript of the session ([`crate::usage`]).
    #[serde(default)]
    pub transcript_path: Option<PathBuf>,
}

impl EndInput {
    /// False for `/clear`: the riff session goes on (R168).
    pub fn ends_the_session(&self) -> bool {
        self.reason != "clear"
    }
}

use crate::permissions::Rules;
use crate::text::DATA_NOTE;

/// Why the session started, as Claude Code gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// A resumed session. It keeps its session ID.
    Resume,
    /// `/clear`. Claude Code gives the session a new session ID, but the
    /// riff session keeps its ID (R168).
    Clear,
    /// The context was compacted. The background tasks still run.
    Compact,
    /// A new session. An unknown source counts as a new session.
    #[default]
    #[serde(other)]
    Startup,
}

impl Source {
    /// True for a new start: a new agent process, a resume or a
    /// `/clear`. A compaction goes on with the same process and context
    /// (01M3JEE1QQCFS5TMZW5N2DAD2D).
    ///
    /// ```
    /// use riff::hook::Source;
    ///
    /// assert!(Source::Clear.is_new_start());
    /// assert!(!Source::Compact.is_new_start());
    /// ```
    pub fn is_new_start(self) -> bool {
        self.reason().is_some()
    }

    /// The reason of the `start` call that the hook sends for this
    /// source (01M3X9X9M079WGFPJZHNXH9VEP). `None`: the hook sends no start. A
    /// compaction is no new session: it keeps its claims and its lead,
    /// and it is no fresh context.
    ///
    /// ```
    /// use riff::hook::Source;
    /// use riff_core::wire::StartReason;
    ///
    /// assert_eq!(Source::Startup.reason(), Some(StartReason::Process));
    /// assert_eq!(Source::Resume.reason(), Some(StartReason::Resume));
    /// assert_eq!(Source::Clear.reason(), Some(StartReason::Clear));
    /// assert_eq!(Source::Compact.reason(), None);
    /// ```
    pub fn reason(self) -> Option<StartReason> {
        match self {
            Source::Startup => Some(StartReason::Process),
            Source::Resume => Some(StartReason::Resume),
            Source::Clear => Some(StartReason::Clear),
            Source::Compact => None,
        }
    }
}

/// The line of the start context of a worker: a session with
/// `RIFF_WORKER=1` (01M3JQC8ETHRAWSJPHMKA062SQ). A worker with no work
/// waits idle, and a worker that waits for a verify keeps its claim
/// (01M3K0AXMCVRST7HYH4DM8B3AN).
///
/// ```
/// assert!(!riff::hook::WORKER_LINE.contains("set your status"));
/// assert!(!riff::hook::WORKER_LINE.contains("workers done"));
/// ```
pub const WORKER_LINE: &str = "- You are a worker (RIFF_WORKER=1). After you release your last \
claim, do the steps that are left for the item, then end your turn: riff clears your context by \
itself (step 12 of the start routine). When the start routine finds no free item and \
no free verify request, and you hold no claim, keep your watch running, and end your turn. riff \
shows you as idle. Do not end this session: the lead gives you work with a request, and the \
server stops an idle worker when too many wait (01M3Q5A0NKY1FCS0YH6N6YD3GN). While you wait for \
a verify, keep your claim and wait.\n";

/// The line of the start context that names the file that turned riff
/// on, and how to turn it off (01M3XY2SYKG91SAB2FS1QNCZ2H). `None` when
/// no file decides: `RIFF_ON` turned riff on.
///
/// ```
/// use riff::enable::{Place, State};
///
/// let by = Some((Place::Local, "/r/.claude/settings.local.json".into()));
/// let state = State { on: true, by, forced: false, repo: None };
/// assert_eq!(
///     riff::hook::on_line(&state).unwrap(),
///     "- riff is on in this repository by /r/.claude/settings.local.json. To turn it off, \
///      your user runs `riff disable` there in a terminal.\n"
/// );
/// assert_eq!(riff::hook::on_line(&State { by: None, forced: true, ..state }), None);
/// ```
pub fn on_line(state: &crate::enable::State) -> Option<String> {
    let (_, file) = state.by.as_ref()?;
    Some(format!(
        "- riff is on in this repository by {}. To turn it off, your user runs `riff disable` \
         there in a terminal.\n",
        file.display()
    ))
}

/// The line of the start context in a project where a person turned the
/// riff server off in the `/mcp` dialog of Claude Code
/// (01M3XY2T0R2Q39XYX8AYV7T0RK). The session has no riff tools, so it
/// tells its lead with the `riff` command.
pub const MCP_OFF_LINE: &str = "- The riff server is turned off for this project in Claude Code \
(`disabledMcpServers` in its state file has `plugin:riff:riff`), so this session has no riff \
tools. Tell your user to turn it on: `/mcp`, then the server riff. Until then, use the riff \
commands with the Bash tool, for example `riff read`, and tell your lead now: run `riff tell \
lead \"this session has no riff tools: the riff server is turned off for the project in \
/mcp\"`.\n";

/// The part of the SessionStart hook input that riff uses.
#[derive(Debug, Default, Deserialize)]
pub struct StartInput {
    /// The session ID.
    pub session_id: Option<String>,
    /// Why the session started.
    #[serde(default)]
    pub source: Source,
    /// The transcript of the session. `/clear` starts a new one
    /// ([`crate::usage`]).
    #[serde(default)]
    pub transcript_path: Option<PathBuf>,
}

/// The context that the start hook adds. `uri` is the session, when the
/// hook found it. `watching` is true when a watch runs for the session.
/// `riff` holds the pauses of the riff, when the hook could read them.
/// The context names the pause that stops the session, and who set it
/// (01M3XAHZJAF6YVDJ7WX74X8RBX). `freed` holds each claim that this new
/// start freed.
///
/// ```
/// use riff::hook::{Source, start_context};
/// use riff_core::record::By;
/// use riff_core::wire::{PauseInfo, RepositoryPause, RiffReply, RiffState};
///
/// let paused = start_context(None, Source::Startup, false, Some(&RiffState::Paused.into()), &[]);
/// assert!(paused.contains("The riff is paused. Claim nothing."));
/// assert!(!paused.contains("Pick a free item"));
/// let running = start_context(None, Source::Startup, false, Some(&RiffState::Running.into()), &[]);
/// assert!(running.contains("Pick a free item yourself"));
///
/// // The repository of the session is paused, and the riff runs.
/// let me = "riff://brett@kadomony/como-technologies/strata?session=77e0".parse()?;
/// let pause = RepositoryPause {
///     repository: "como-technologies/strata".parse()?,
///     pause: PauseInfo { by: Some(By::Person("brett".into())), at_ms: 7 },
/// };
/// let pauses = RiffReply {
///     state: RiffState::Paused,
///     repositories: vec![pause],
///     ..RiffState::Running.into()
/// };
/// let context = start_context(Some(&me), Source::Startup, false, Some(&pauses), &[]);
/// let line = "- The repository como-technologies/strata is paused by the person brett. Claim nothing.";
/// assert!(context.contains(line), "{context}");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn start_context(
    uri: Option<&SessionUri>,
    source: Source,
    watching: bool,
    riff: Option<&RiffReply>,
    freed: &[Freed],
) -> String {
    let mut out = String::from("riff: ");
    match uri {
        Some(uri) => writeln!(out, "this session is {uri}.").unwrap(),
        None => out.push_str("riff could not find this session. Call the riff whoami tool.\n"),
    }
    let start = "run `riff watch --once` with the Bash tool, with run_in_background true and \
                 the description \"riff wakes\".";
    if source == Source::Clear {
        out.push_str(
            "- /clear did not change your riff session ID or your lead. Your claims are free: \
             a new start is blank. The riff whoami tool shows your URI.\n",
        );
    }
    if !freed.is_empty() {
        let items: Vec<String> = freed
            .iter()
            .map(|f| format!("{} in {}", f.item, f.thread))
            .collect();
        writeln!(
            out,
            "- This new start freed your claims: {}. Another session can take them. To go on \
             with one, claim it again, then see \"Pick up dropped work\" in the riff skill.",
            items.join(", ")
        )
        .unwrap();
    }
    if watching {
        out.push_str("- A task runs `riff watch` for this session. Keep it.\n");
    } else {
        writeln!(out, "- Now {start}").unwrap();
    }
    out.push_str(
        "- When the task ends, call the riff read tool with no thread and start the watch \
         again at once, in the same response: two tool calls in one message, also in the \
         middle of a turn (01M3JPMQJCC3F19QAJ84EKMVKA). When the watch says \"Do not start \
         the watch again now\", do not start it.\n",
    );
    let new = matches!(source, Source::Startup | Source::Clear);
    let find_work = "follow the start routine of the riff skill. Pick a free item yourself. \
                     Do not wait for a plan or for permission. A scope from your user wins.";
    let lead = uri.is_some_and(SessionUri::lead);
    let here = uri.and_then(SessionUri::default_thread);
    let paused = riff.map(|pauses| (pauses, crate::text::paused(pauses, here.as_ref())));
    match paused {
        Some((_, None)) if new => {
            writeln!(out, "- The riff is running. To find work, {find_work}").unwrap();
        }
        Some((_, None)) => {}
        Some((pauses, Some(paused))) if new && lead => {
            let resume = if pauses.riff.is_some() {
                "The owner or an admin resumes it with `riff resume --riff`, or tells you to \
                 call the riff resume tool with `riff` true."
            } else {
                "Your user resumes it with `riff resume`, or tells you to call the riff resume \
                 tool."
            };
            writeln!(
                out,
                "- {paused}. Claim nothing. You are the lead: tell your user. {resume} See \
                 \"Pause\" in the riff skill."
            )
            .unwrap();
        }
        Some((_, Some(paused))) if new => writeln!(
            out,
            "- {paused}. Claim nothing. Say hello to the lead: call the riff tell tool with the \
             session `lead`. Then wait. A resume wakes you. See \"Pause\" in the riff skill."
        )
        .unwrap(),
        Some((_, Some(paused))) => writeln!(
            out,
            "- {paused}. Stop at your next step and wait. See \"Pause\" in the riff skill."
        )
        .unwrap(),
        None if new => writeln!(
            out,
            "- riff could not read the state of the riff. Call the riff whoami tool. When the \
             riff is running, {find_work} When it is paused, see \"Pause\" in the riff skill."
        )
        .unwrap(),
        None => {}
    }
    writeln!(out, "- {DATA_NOTE}").unwrap();
    out
}

/// The start context when the builds of `riff` and its `riff-server` do
/// not match (01M3JEE7TPZMNK7X6JXJ7GWFPP). The session tells its user at
/// once, and does not use the riff.
///
/// ```
/// use riff_core::build::Mismatch;
///
/// let m = Mismatch { riff: Some("0.4.0 bbbb 2026-09-27T11:00:00Z".parse().unwrap()), server: None, seen: None };
/// let context = riff::hook::mismatch_context(None, &m);
/// assert!(context.contains("do not match"));
/// assert!(context.contains("Tell your user now"));
/// assert!(!context.contains("riff watch"));
/// ```
pub fn mismatch_context(uri: Option<&SessionUri>, mismatch: &Mismatch) -> String {
    let mut out = String::from("riff: ");
    if let Some(uri) = uri {
        writeln!(out, "this session is {uri}.").unwrap();
        out.push_str("- ");
    }
    writeln!(out, "riff cannot use its riff-server: {mismatch}").unwrap();
    out.push_str(
        "- Tell your user now, in your first reply. Do not start the watch, and do not call \
         the riff tools, until your user updates riff and starts this session again.\n",
    );
    out
}

/// The start line for the lead when the project lacks riff permission
/// rules (01M3Q53RQGXMYVYGCQQMWA9380), or `None` when it has them all.
///
/// ```
/// use riff::permissions::Rules;
///
/// let left = Rules { allow: vec!["mcp__riff".into()], deny: vec![] };
/// let line = riff::hook::rules_line(&left, "/src/app".as_ref()).unwrap();
/// assert!(line.contains("1 riff permission rule is missing"));
/// assert!(line.contains("run `riff setup` in /src/app"));
/// assert_eq!(riff::hook::rules_line(&Rules::default(), "/src/app".as_ref()), None);
/// ```
pub fn rules_line(missing: &Rules, top: &Path) -> Option<String> {
    if missing.is_empty() {
        return None;
    }
    let count = match missing.len() {
        1 => "1 riff permission rule is".to_owned(),
        n => format!("{n} riff permission rules are"),
    };
    Some(format!(
        "- In the Claude Code settings of this project, {count} missing, so auto mode can \
         block riff work. You cannot add them yourself. Tell your user in your first reply: \
         run `riff setup` in {}, commit .claude/settings.json, and start the sessions \
         again.\n",
        top.display()
    ))
}

/// The hook output that gives `context` to Claude Code.
///
/// ```
/// let out: serde_json::Value = serde_json::from_str(&riff::hook::start_output("hi"))?;
/// assert_eq!(out["hookSpecificOutput"]["hookEventName"], "SessionStart");
/// assert_eq!(out["hookSpecificOutput"]["additionalContext"], "hi");
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn start_output(context: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": context,
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri() -> SessionUri {
        "riff://mike@pangolin/como-technologies/riff?session=a6cf"
            .parse()
            .unwrap()
    }

    const SOURCES: [Source; 4] = [
        Source::Startup,
        Source::Resume,
        Source::Clear,
        Source::Compact,
    ];

    #[test]
    fn each_source_starts_the_watch_when_none_runs() {
        for source in SOURCES {
            let context = start_context(Some(&uri()), source, false, None, &[]);
            assert!(
                context.contains("Now run `riff watch --once` with the Bash tool"),
                "{context}"
            );
            assert!(context.contains("run_in_background true"));
            assert!(context.contains("start the watch again at once, in the same response"));
            assert!(context.contains("middle of a turn"));
            assert!(context.contains(DATA_NOTE));
            assert!(context.contains("session=a6cf"));
        }
    }

    #[test]
    fn each_source_keeps_a_watch_that_runs() {
        for source in SOURCES {
            let context = start_context(Some(&uri()), source, true, None, &[]);
            assert!(context.contains("Keep it."), "{context}");
            assert!(!context.contains("Now run"), "{context}");
            assert!(context.contains("start the watch again at once, in the same response"));
        }
    }

    #[test]
    fn no_source_stops_a_watch() {
        for source in SOURCES {
            for watching in [false, true] {
                assert!(!start_context(None, source, watching, None, &[]).contains("TaskStop"));
            }
        }
    }

    #[test]
    fn clear_keeps_the_id_and_the_lead_and_frees_the_claims() {
        let context = start_context(Some(&uri()), Source::Clear, true, None, &[]);
        assert!(context.contains("did not change your riff session ID or your lead"));
        assert!(context.contains("Your claims are free"));
        assert!(context.contains("whoami"));
        for source in [Source::Startup, Source::Resume, Source::Compact] {
            assert!(!start_context(None, source, true, None, &[]).contains("claims are free"));
        }
    }

    #[test]
    fn a_new_start_names_each_freed_claim() {
        let freed = [Freed {
            thread: "como-technologies/riff".parse().unwrap(),
            item: "issue-12".into(),
        }];
        for source in [Source::Startup, Source::Resume, Source::Clear] {
            let context = start_context(Some(&uri()), source, true, None, &freed);
            assert!(
                context.contains("freed your claims: issue-12 in como-technologies/riff"),
                "{context}"
            );
            assert!(context.contains("Pick up dropped work"), "{context}");
        }
        assert!(!start_context(Some(&uri()), Source::Resume, true, None, &[]).contains("freed"));
    }

    #[test]
    fn only_a_compaction_is_not_a_new_start() {
        for source in SOURCES {
            assert_eq!(source.is_new_start(), source != Source::Compact);
        }
    }

    use riff_core::wire::RiffState;

    /// The pauses of a riff that runs, and of a riff that is paused.
    fn pauses(state: RiffState) -> RiffReply {
        state.into()
    }

    #[test]
    fn new_sessions_in_a_running_riff_get_the_start_routine() {
        let context =
            |source| start_context(None, source, false, Some(&pauses(RiffState::Running)), &[]);
        assert!(context(Source::Startup).contains("start routine"));
        assert!(context(Source::Startup).contains("Pick a free item yourself"));
        assert!(context(Source::Clear).contains("start routine"));
        assert!(!context(Source::Resume).contains("start routine"));
        assert!(!context(Source::Compact).contains("start routine"));
    }

    #[test]
    fn a_new_session_in_a_paused_riff_waits_and_says_hello() {
        for source in [Source::Startup, Source::Clear] {
            let context = start_context(
                Some(&uri()),
                source,
                false,
                Some(&pauses(RiffState::Paused)),
                &[],
            );
            assert!(context.contains("Claim nothing."), "{context}");
            assert!(context.contains("the session `lead`"), "{context}");
            assert!(
                !context.contains("Set your status"),
                "riff shows the pause: {context}"
            );
            assert!(!context.contains("Pick a free item"), "{context}");
        }
    }

    #[test]
    fn the_lead_in_a_paused_riff_tells_its_user() {
        let lead = uri().with_lead(true);
        let context = start_context(
            Some(&lead),
            Source::Startup,
            false,
            Some(&pauses(RiffState::Paused)),
            &[],
        );
        assert!(
            context.contains("You are the lead: tell your user"),
            "{context}"
        );
        assert!(context.contains("riff resume"), "{context}");
        assert!(!context.contains("Say hello to the lead"), "{context}");
    }

    #[test]
    fn a_session_that_continues_in_a_paused_riff_stops() {
        for source in [Source::Resume, Source::Compact] {
            let context = start_context(
                Some(&uri()),
                source,
                true,
                Some(&pauses(RiffState::Paused)),
                &[],
            );
            assert!(context.contains("Stop at your next step"), "{context}");
            assert!(!context.contains("start routine"), "{context}");
        }
    }

    #[test]
    fn an_unknown_state_asks_for_whoami_before_work() {
        let context = start_context(Some(&uri()), Source::Startup, false, None, &[]);
        assert!(context.contains("could not read the state"), "{context}");
        assert!(
            context.contains("When the riff is running, follow"),
            "{context}"
        );
        assert!(
            !start_context(None, Source::Resume, false, None, &[]).contains("state of the riff")
        );
    }

    #[test]
    fn no_uri_asks_for_whoami() {
        assert!(start_context(None, Source::Startup, false, None, &[]).contains("whoami"));
    }

    #[test]
    fn each_end_but_clear_ends_the_session() {
        for reason in ["logout", "prompt_input_exit", "other", ""] {
            let input = EndInput {
                session_id: None,
                reason: reason.into(),
                transcript_path: None,
            };
            assert!(input.ends_the_session(), "{reason}");
        }
        let input: EndInput = serde_json::from_str("{}").unwrap();
        assert!(input.ends_the_session());
    }

    #[test]
    fn the_behind_line_counts_the_commits() {
        let behind = |commits| Behind {
            main: "/src/riff".into(),
            branch: "trunk".into(),
            commits,
        };
        assert!(
            behind(1)
                .line()
                .contains("is 1 commit behind origin/trunk.")
        );
        assert!(
            behind(3)
                .line()
                .contains("is 3 commits behind origin/trunk.")
        );
        assert!(behind(3).line().contains("through the lead"));
    }

    #[tokio::test]
    async fn a_directory_outside_git_is_not_behind() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(behind(dir.path(), FETCH_WAIT).await, None);
    }

    /// 01M3X9X9M079WGFPJZHNXH9VEP: the start call of each source of Claude Code. Only a
    /// new process and a clear are a fresh context.
    #[test]
    fn each_source_sends_its_reason_and_a_compaction_sends_no_start() {
        let source = |json: &str| serde_json::from_str::<StartInput>(json).unwrap().source;
        let sent = [
            ("startup", Some(StartReason::Process)),
            ("resume", Some(StartReason::Resume)),
            ("clear", Some(StartReason::Clear)),
            ("compact", None),
            // A source of a later Claude Code counts as a new session.
            ("later", Some(StartReason::Process)),
        ];
        for (name, reason) in sent {
            let source = source(&format!(r#"{{"source":"{name}"}}"#));
            assert_eq!(source.reason(), reason, "{name}");
            assert_eq!(source.is_new_start(), reason.is_some(), "{name}");
            let fresh = reason.is_some_and(StartReason::is_fresh);
            assert_eq!(
                fresh,
                matches!(name, "startup" | "clear" | "later"),
                "{name}"
            );
        }
        assert_eq!(SOURCES.len(), 4, "each source has a row");
    }

    #[test]
    fn an_unknown_source_is_a_startup() {
        let input: StartInput = serde_json::from_str(r#"{"source":"later"}"#).unwrap();
        assert_eq!(input.source, Source::Startup);
        let input: StartInput = serde_json::from_str("{}").unwrap();
        assert_eq!(input.source, Source::Startup);
        assert_eq!(input.session_id, None);
    }
}
