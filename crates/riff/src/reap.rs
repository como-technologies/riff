//! A worker can die at each moment. riff sees it, and the work goes on.
//!
//! # Design
//!
//! A worker pane can end with no end call of its session: a memory
//! kill, a crash, a closed pane. Then no process of the worker is left
//! to tell the server. The server sees it only after 3 minutes with no
//! keep-alive, and the claims are free after 5 minutes (R206, R9).
//!
//! So the process that looks after the workers of a machine watches
//! their panes (01M3WG2460P4GF7GEVBY92Q33W): `riff workers host`, and
//! `riff mcp` of the lead on the machine of the lead. Each [`EVERY`] it
//! lists the worker panes ([`Reaper::look`]). For a pane that was there
//! at the last look and is gone now, for each cause, it does
//! [`reap`]:
//!
//! ```mermaid
//! sequenceDiagram
//!     participant O as systemd-oomd
//!     participant T as tmux pane of the worker
//!     participant H as riff workers host, or riff mcp of the lead
//!     participant S as riff-server
//!     participant L as lead
//!     participant R as rollout
//!     O->>T: kill each process of the pane
//!     H->>H: look: the pane is gone
//!     H->>S: who: the session is live, it holds issue-12
//!     H->>S: end, as the session: issue-12 is free at once
//!     H->>H: the journal: who killed the scope of the pane?
//!     H->>S: a note to the lead: pane, session, item, cause
//!     R->>S: who, gh: issue-12 is free, no worker is idle
//!     R->>T: a new worker
//!     T->>S: claim issue-12, and go on from the pushed branch
//! ```
//!
//! - A session that is not live at the look has its end: `riff workers
//!   stop`, the stop of an idle worker by the server, or the wrapper
//!   after an exit of `claude`. riff does nothing for it, so the lead
//!   gets one message for one worker.
//! - The note wakes nobody. The death of a worker is not an event for
//!   the lead: the rollout starts a worker for the free item, as for
//!   each free item ([`crate::rollout`]).
//! - The cause is best effort ([`oom_cause`]). tmux puts each pane in a
//!   systemd scope of its own. riff keeps the scope of each worker pane
//!   while the pane lives ([`scope_of`]), and looks for the line of
//!   `systemd-oomd` that names the scope ([`journal`]). With no such
//!   line, the note says that riff found no cause.

use std::time::Duration;

use riff_core::name::SessionUri;
use riff_core::wire::SessionInfo;

use crate::api::Api;
use crate::terminal::{Terminal, Tmux, WorkerPane};
use crate::text;

/// The time between two looks at the worker panes.
pub const EVERY: Duration = Duration::from_secs(5);

/// How far back riff reads the journal for the cause.
const JOURNAL_SINCE: &str = "-10min";

/// A worker pane that riff watches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watched {
    pub pane: WorkerPane,
    /// The systemd scope of the pane, if riff found it.
    pub scope: Option<String>,
}

/// Remembers the worker panes of the last look.
///
/// ```
/// use riff::reap::Reaper;
/// use riff::terminal::WorkerPane;
///
/// let pane = |p: &str, s: &str| WorkerPane { pane: p.into(), session: s.into() };
/// let scope = |w: &WorkerPane| Some(format!("/scope-of-{}", w.pane));
/// let mut reaper = Reaper::default();
/// // The first look only remembers.
/// assert!(reaper.look(&[pane("%1", "s1"), pane("%2", "s2")], scope).is_empty());
/// assert!(reaper.look(&[pane("%1", "s1"), pane("%2", "s2")], scope).is_empty());
/// // The pane %2 is gone. riff kept its scope from the first look.
/// let lost = reaper.look(&[pane("%1", "s1"), pane("%3", "s3")], scope);
/// assert_eq!(lost.len(), 1);
/// assert_eq!(lost[0].pane, pane("%2", "s2"));
/// assert_eq!(lost[0].scope.as_deref(), Some("/scope-of-%2"));
/// // It tells of a lost pane one time.
/// assert!(reaper.look(&[pane("%1", "s1"), pane("%3", "s3")], scope).is_empty());
/// ```
#[derive(Debug, Default)]
pub struct Reaper {
    seen: Vec<Watched>,
}

impl Reaper {
    /// One look: `now` are the worker panes of the machine. It gives
    /// each worker whose pane was there at the last look and is not
    /// there now. `scope` finds the scope of a new pane; it runs one
    /// time for each pane.
    pub fn look(
        &mut self,
        now: &[WorkerPane],
        scope: impl Fn(&WorkerPane) -> Option<String>,
    ) -> Vec<Watched> {
        let (stay, lost): (Vec<Watched>, Vec<Watched>) = std::mem::take(&mut self.seen)
            .into_iter()
            .partition(|w| now.contains(&w.pane));
        self.seen = stay;
        for pane in now {
            if !self.seen.iter().any(|w| &w.pane == pane) {
                self.seen.push(Watched {
                    pane: pane.clone(),
                    scope: scope(pane),
                });
            }
        }
        lost
    }
}

/// The systemd scope of the process `pid`: the path of its cgroup, from
/// `/proc/PID/cgroup`. `None` on a system with no such file.
pub fn scope_of(pid: u32) -> Option<String> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    cgroup_path(&text)
}

/// The path of the cgroup in the text of `/proc/PID/cgroup` (cgroup
/// version 2).
///
/// ```
/// let text = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/tmux-spawn-f089.scope\n";
/// assert_eq!(
///     riff::reap::cgroup_path(text).as_deref(),
///     Some("/user.slice/user-1000.slice/user@1000.service/app.slice/tmux-spawn-f089.scope"),
/// );
/// assert_eq!(riff::reap::cgroup_path("1:name=systemd:/x\n"), None);
/// assert_eq!(riff::reap::cgroup_path("0::/\n"), None);
/// ```
pub fn cgroup_path(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("0::"))
        .map(str::trim)
        .filter(|path| path.len() > 1)
        .map(str::to_owned)
}

/// The cause of the end of the scope `scope`, when `journal` has the
/// line of `systemd-oomd` that names it.
///
/// ```
/// let journal = "Considered 74 cgroups for killing, top candidates were:\n\
///     Killed /user.slice/app.slice/tmux-spawn-f089.scope due to memory pressure for \
///     /user.slice/user-1000.slice/user@1000.service being 66.21% > 50.00% for > 20s \
///     with reclaim activity\n";
/// assert_eq!(
///     riff::reap::oom_cause(journal, "/user.slice/app.slice/tmux-spawn-f089.scope").as_deref(),
///     Some(
///         "systemd-oomd killed the pane: memory pressure for \
///          /user.slice/user-1000.slice/user@1000.service being 66.21% > 50.00% for > 20s \
///          with reclaim activity"
///     ),
/// );
/// assert_eq!(riff::reap::oom_cause(journal, "/user.slice/app.slice/tmux-spawn-0000.scope"), None);
/// ```
pub fn oom_cause(journal: &str, scope: &str) -> Option<String> {
    let killed = format!("Killed {scope} ");
    let line = journal
        .lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix(&killed))?;
    let why = line.strip_prefix("due to ").unwrap_or(line).trim();
    Some(format!("systemd-oomd killed the pane: {why}"))
}

/// The last lines of `systemd-oomd` in the journal, or `None` when
/// `journalctl` does not run or gives nothing.
pub fn journal() -> Option<String> {
    let out = std::process::Command::new("journalctl")
        .args(["-q", "--no-pager", "-o", "cat", "-u", "systemd-oomd"])
        .args(["--since", JOURNAL_SINCE])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    (out.status.success() && !text.trim().is_empty()).then_some(text)
}

/// One look at the worker panes of `tmux` (see [`Reaper::look`]). A
/// look that cannot list the panes gives nothing, and forgets nothing.
pub fn lost(reaper: &mut Reaper, tmux: &Tmux) -> Vec<Watched> {
    match tmux.worker_panes() {
        Ok(panes) => reaper.look(&panes, |w| tmux.pane_pid(&w.pane).and_then(scope_of)),
        Err(e) => {
            eprintln!("riff: cannot list the worker panes: {e:#}");
            Vec::new()
        }
    }
}

/// Ends the session of each worker in `lost` that is still live in
/// `sessions` (`riff who`), and gives one note for the lead for each
/// (01M3WG2460P4GF7GEVBY92Q33W). `me` is the caller: the host, or the
/// lead. An end frees the claims of the session at once (R206).
/// `journal` reads the journal one time, only when a lost pane has a
/// scope.
pub async fn reap(
    api: &Api,
    me: &SessionUri,
    sessions: &[SessionInfo],
    lost: &[Watched],
    journal: impl Fn() -> Option<String>,
) -> Vec<String> {
    let mut notes = Vec::new();
    let mut lines: Option<Option<String>> = None;
    for worker in lost {
        let id = worker.pane.session.as_str();
        let Some(info) = sessions
            .iter()
            .find(|s| s.uri.who().user() == me.who().user() && s.uri.who().session() == Some(id))
        else {
            // The session has its end, or it never joined.
            continue;
        };
        let claims = info.uri.claims().to_vec();
        let session = SessionUri::new(info.uri.who().clone(), info.uri.place().clone());
        let ended = match api.clone().signed_in(Some(id)) {
            Ok(api) => api.end(&session).await,
            Err(e) => Err(e),
        };
        let cause = worker.scope.as_deref().and_then(|scope| {
            let lines = lines.get_or_insert_with(&journal);
            oom_cause(lines.as_deref()?, scope)
        });
        let host = info.uri.place().host();
        let mut note = text::worker_gone(host, &worker.pane, &claims, cause.as_deref());
        if let Err(e) = ended {
            note.push_str(&text::worker_gone_not_ended(&format!("{e:#}")));
        }
        notes.push(note);
    }
    notes
}
