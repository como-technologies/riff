//! riff controls the processes of its workers itself.
//!
//! # Design
//!
//! A worker starts background commands, for example `just ci`. When
//! riff clears its context, Claude Code keeps them, and nothing waits
//! for them. They slow the machine. The auto mode check of the agent
//! tool refuses a raw `kill PID` of an agent, and it is right to. So
//! riff stops them in code, with its own checks, and the agent never
//! names a process ID (01M3ZV0QSFVCHRSEKYK57B88VA).
//!
//! | Process | How riff knows it |
//! |---|---|
//! | of the worker `ID` | its cgroup is a systemd scope of the worker: `riff-worker-ID.PID.scope` ([`scope_unit`], 01M49SV9W4S1HJ4BYANA388VD2, 01M49SV9Z2A7TXWFTMVNYXSQNM). A process cannot leave its cgroup by a change of its own environment. |
//! | of the worker `ID`, with no scope | no process is in a scope of the worker, for example on a machine with no systemd. Then its environment has `RIFF_WORKER=1`, `RIFF_SESSION=ID` and the `RIFF_HOME` of the caller (none when the caller has none). Each child gets the variables of its parent, also after its parent ends. riff says one time that it uses the environment ([`say`], 01M49SVFW0FZ3DK57PACS7W5EY). |
//! | of a context | it has also the variable of the agent tool ([`crate::next::Agent::context_var`]): Claude Code gives it to each command of its Bash tool and to each hook, not to `claude` and not to its MCP servers. |
//! | the watch | `riff watch`. riff keeps it and its parents: a worker keeps its watch over a clear (01M3JQCD16CNWN5FCQBRKHXYMP). |
//! | the caller | this process and its parents. riff never stops them. |
//! | the start of the context | the start hook writes the start of its own process to the file `context-ID` of the local dir ([`mark`]). The file names the boot. |
//!
//! ```mermaid
//! flowchart TD
//!     A["each process of the user<br/>/proc/PID/cgroup"] --> G{"a process in the scope<br/>riff-worker-ID.N.scope?"}
//!     G -- yes --> P{"in that scope?"}
//!     G -- "no: say so one time" --> W{"/proc/PID/environ:<br/>RIFF_WORKER=1 and RIFF_SESSION=ID?"}
//!     P -- no --> K["not of this worker"]
//!     W -- no --> K
//!     P -- yes --> C{"variable of the agent tool?"}
//!     W -- yes --> C
//!     C -- "no: claude, MCP server" --> S["keep"]
//!     C -- yes --> R{"riff watch, or the caller,<br/>or a parent of one?"}
//!     R -- yes --> S
//!     R -- no --> T{"started before the start<br/>of the context?"}
//!     T -- yes --> X["stop: SIGTERM, then SIGKILL"]
//!     T -- no --> S
//! ```
//!
//! - At the clear (01M3ZV0TJDQ6JCM7XG0036MSV1), each process of the
//!   context is old: the turn ended, and the new context starts after
//!   the keys. riff stops them before it types `/clear`
//!   ([`old_context`] with no start).
//! - `riff workers reap` (01M3ZV0TKBP201FKY32ZD81G4E) stops only the
//!   processes that started before the start of the current context
//!   ([`context_start`]). With no start in the file, it stops nothing.
//! - `riff workers stop PANE` (01M3ZV0TMNQDK9WC3BR1NPGAC2) stops each
//!   process of the worker ([`of_worker`]), after it closes the pane.
//! - riff reads the start of a process again just before the signal.
//!   A process ID that a new process took is safe.
//!
//! ```
//! use riff::workload::{Proc, old_context};
//!
//! let p = |pid, ppid, start, argv: &[&str], context| Proc {
//!     pid,
//!     ppid,
//!     start,
//!     argv: argv.iter().map(|a| a.to_string()).collect(),
//!     worker: Some("w1".into()),
//!     scope: None,
//!     context,
//! };
//! let all = [
//!     p(10, 1, 100, &["claude"], false),
//!     p(11, 10, 101, &["riff", "mcp"], false),
//!     p(20, 10, 200, &["bash", "-c", "riff watch --once"], true),
//!     p(21, 20, 200, &["riff", "watch", "--once"], true),
//!     p(30, 10, 300, &["bash", "-c", "just ci"], true),
//!     p(31, 30, 300, &["just", "ci"], true),
//!     p(40, 1, 400, &["riff", "hook", "clear"], true),
//! ];
//! // The clear: riff hook clear (40) stops `just ci` and its shell.
//! let old = old_context(&all, "w1", 40, None);
//! assert_eq!(old.procs.iter().map(|p| p.pid).collect::<Vec<_>>(), [30, 31]);
//! // A reap after a context that started at 250 keeps a new `just ci`.
//! assert!(old_context(&all, "w1", 40, Some(250)).procs.is_empty());
//! ```

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

/// How long riff waits after SIGTERM before it sends SIGKILL.
pub const STOP_WAIT: Duration = Duration::from_secs(3);

/// A process of this user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    /// The parent process.
    pub ppid: u32,
    /// The start, in clock ticks after the boot.
    pub start: u64,
    /// The command line.
    pub argv: Vec<String>,
    /// The session ID of the worker, when the process is of a worker:
    /// `RIFF_WORKER=1` and `RIFF_SESSION`.
    pub worker: Option<String>,
    /// The worker of the systemd scope that holds the process, in the
    /// form of [`unit_part`], from its cgroup ([`scope_worker`]).
    pub scope: Option<String>,
    /// True when the process has the variable of a context of the agent
    /// tool.
    pub context: bool,
}

impl Proc {
    /// True for `riff watch`.
    ///
    /// ```
    /// use riff::workload::Proc;
    /// let p = |argv: &[&str]| Proc { pid: 1, ppid: 0, start: 0, argv: argv.iter().map(|a| a.to_string()).collect(), worker: None, scope: None, context: true };
    /// assert!(p(&["/home/m/.cargo/bin/riff", "watch", "--once"]).is_watch());
    /// assert!(!p(&["riff", "workers"]).is_watch());
    /// assert!(!p(&["bash", "-c", "riff watch --once"]).is_watch());
    /// ```
    pub fn is_watch(&self) -> bool {
        let riff = self
            .argv
            .first()
            .is_some_and(|a| Path::new(a).file_name().is_some_and(|n| n == "riff"));
        riff && self.argv.get(1).is_some_and(|a| a == "watch")
    }

    /// The command line in one line, at most 80 characters.
    ///
    /// ```
    /// use riff::workload::Proc;
    /// let p = Proc { pid: 1, ppid: 0, start: 0, argv: vec!["just".into(), "ci".into()], worker: None, scope: None, context: true };
    /// assert_eq!(p.line(), "just ci");
    /// ```
    pub fn line(&self) -> String {
        let line = self.argv.join(" ");
        let line: String = line
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        match line.char_indices().nth(80) {
            Some((at, _)) => format!("{}…", &line[..at]),
            None => line,
        }
    }
}

/// The parts of `/proc/PID/stat` that riff uses: the parent and the
/// start.
///
/// ```
/// let stat = "4242 (just ci) S 4200 4242 4200 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 1 0 98765 0 0";
/// assert_eq!(riff::workload::parse_stat(stat), Some((4200, 98765)));
/// assert_eq!(riff::workload::parse_stat("4242 (x"), None);
/// ```
pub fn parse_stat(stat: &str) -> Option<(u32, u64)> {
    // The name of the program can hold spaces and parentheses.
    let (_, rest) = stat.rsplit_once(')')?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let ppid = fields.get(1)?.parse().ok()?;
    let start = fields.get(19)?.parse().ok()?;
    Some((ppid, start))
}

/// The worker and the context mark of an environment of
/// `/proc/PID/environ`. `context_var` is the variable of the agent
/// tool. A process is of a worker only in the riff home `home`, the
/// value of `RIFF_HOME` of the caller (01M438620PJHSVSPAENBKKJ6C2): a
/// riff of another home has its own workers, also with the same
/// session ID.
///
/// ```
/// use riff::workload::parse_environ;
/// let env = b"HOME=/h\0RIFF_WORKER=1\0RIFF_SESSION=w1\0CLAUDE_PID=10\0";
/// assert_eq!(parse_environ(env, "CLAUDE_PID", None), (Some("w1".into()), true));
/// assert_eq!(parse_environ(b"RIFF_SESSION=w1\0", "CLAUDE_PID", None), (None, false));
/// // Another riff home: not a worker of this riff.
/// assert_eq!(parse_environ(env, "CLAUDE_PID", Some("/t/riff")), (None, true));
/// let env = b"RIFF_WORKER=1\0RIFF_SESSION=w1\0RIFF_HOME=/t/riff\0";
/// assert_eq!(parse_environ(env, "CLAUDE_PID", Some("/t/riff")), (Some("w1".into()), false));
/// assert_eq!(parse_environ(env, "CLAUDE_PID", None), (None, false));
/// ```
pub fn parse_environ(env: &[u8], context_var: &str, home: Option<&str>) -> (Option<String>, bool) {
    let (mut worker, mut session, mut context) = (false, None, false);
    let mut own_home = None;
    for var in env.split(|b| *b == 0) {
        let var = String::from_utf8_lossy(var);
        let Some((name, value)) = var.split_once('=') else {
            continue;
        };
        match name {
            crate::worker::WORKER => worker = crate::worker::is_worker_value(Some(value)),
            "RIFF_SESSION" => session = Some(value.to_owned()).filter(|s| !s.is_empty()),
            crate::home::VAR => own_home = Some(value.to_owned()).filter(|h| !h.is_empty()),
            _ if name == context_var => context = true,
            _ => {}
        }
    }
    let here = own_home.as_deref() == home;
    (session.filter(|_| worker && here), context)
}

/// The start of the name of the systemd scope of a worker.
pub const SCOPE_PREFIX: &str = "riff-worker-";

/// The file in the local dir that says that riff said one time that it
/// finds the processes of a worker by their environment
/// (01M49SVFW0FZ3DK57PACS7W5EY).
pub const SAID_BY_ENVIRONMENT: &str = "no-scope-select";

/// The session `session` as a part of a systemd unit name: each
/// character but a letter, a digit, `_` and `-` becomes `_`.
///
/// ```
/// use riff::workload::unit_part;
/// assert_eq!(unit_part("2a880834-5ae9"), "2a880834-5ae9");
/// assert_eq!(unit_part("w.1~x"), "w_1_x");
/// ```
pub fn unit_part(session: &str) -> String {
    session
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The systemd scope of the worker `session` that the wrapper `wrapper`
/// starts: `riff-worker-ID.PID.scope` (01M49SV9W4S1HJ4BYANA388VD2). The
/// PID of the wrapper makes the name new for each start.
///
/// ```
/// use riff::workload::{scope_unit, scope_worker};
/// let unit = scope_unit("2a880834", 4242);
/// assert_eq!(unit, "riff-worker-2a880834.4242.scope");
/// assert_eq!(scope_worker(&format!("/user.slice/riff-workers.slice/{unit}")).as_deref(), Some("2a880834"));
/// ```
pub fn scope_unit(session: &str, wrapper: u32) -> String {
    format!("{SCOPE_PREFIX}{}.{wrapper}.scope", unit_part(session))
}

/// The worker of the cgroup path `cgroup`, in the form of
/// [`unit_part`], when its last part is a scope of [`scope_unit`].
///
/// ```
/// use riff::workload::scope_worker;
/// assert_eq!(scope_worker("/a.slice/riff-worker-w1.10.scope").as_deref(), Some("w1"));
/// assert_eq!(scope_worker("/a.slice/riff-worker-w1-2.10.scope").as_deref(), Some("w1-2"));
/// assert_eq!(scope_worker("/a.slice/riff-worker-w1.scope"), None);
/// assert_eq!(scope_worker("/a.slice/riff-worker-.10.scope"), None);
/// assert_eq!(scope_worker("/a.slice/tmux-spawn-f089.scope"), None);
/// assert_eq!(scope_worker("/a.slice/riff-worker-w1.10.scope/sub"), None);
/// ```
pub fn scope_worker(cgroup: &str) -> Option<String> {
    let unit = cgroup.rsplit('/').next()?;
    let stem = unit.strip_suffix(".scope")?.strip_prefix(SCOPE_PREFIX)?;
    let (session, wrapper) = stem.rsplit_once('.')?;
    let digits = !wrapper.is_empty() && wrapper.bytes().all(|b| b.is_ascii_digit());
    (digits && !session.is_empty()).then(|| session.to_owned())
}

/// How riff found the processes of a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    /// By the systemd scope of the worker (01M49SV9Z2A7TXWFTMVNYXSQNM).
    Scope,
    /// By the environment: no process is in a scope of the worker
    /// (01M49SVFW0FZ3DK57PACS7W5EY).
    Environment,
}

/// The processes that riff selected, and how it found them.
#[derive(Debug)]
pub struct Selection<'a> {
    pub procs: Vec<&'a Proc>,
    pub by: By,
}

/// Each process of the worker `session` in `all`: each process in a
/// scope of the worker when one process is in one, else each process
/// with the environment of the worker (01M49SV9Z2A7TXWFTMVNYXSQNM,
/// 01M49SVFW0FZ3DK57PACS7W5EY).
///
/// ```
/// use riff::workload::{By, Proc, of_session};
/// let p = |pid, worker: Option<&str>, scope: Option<&str>| Proc {
///     pid, ppid: 1, start: 0, argv: vec!["sleep".into()],
///     worker: worker.map(Into::into), scope: scope.map(Into::into), context: true,
/// };
/// // The environment only: by the environment.
/// let all = [p(10, Some("w1"), None), p(11, None, None)];
/// let found = of_session(&all, "w1");
/// assert_eq!((found.procs.len(), found.by), (1, By::Environment));
/// // A scope: a process with no variable of the worker is in it, and a
/// // process with the variables outside of it is not.
/// let all = [p(10, Some("w1"), Some("w1")), p(11, None, Some("w1")), p(12, Some("w1"), None)];
/// let found = of_session(&all, "w1");
/// assert_eq!(found.by, By::Scope);
/// assert_eq!(found.procs.iter().map(|p| p.pid).collect::<Vec<_>>(), [10, 11]);
/// ```
pub fn of_session<'a>(all: &'a [Proc], session: &str) -> Selection<'a> {
    let unit = unit_part(session);
    let scoped = |p: &&Proc| p.scope.as_deref() == Some(unit.as_str());
    if all.iter().any(|p| scoped(&p)) {
        return Selection {
            procs: all.iter().filter(scoped).collect(),
            by: By::Scope,
        };
    }
    Selection {
        procs: all
            .iter()
            .filter(|p| p.worker.as_deref() == Some(session))
            .collect(),
        by: By::Environment,
    }
}

/// The line that says that riff found the processes of the worker
/// `session` by their environment, one time on this machine: the file
/// [`SAID_BY_ENVIRONMENT`] in the local dir `local` holds that
/// (01M49SVFW0FZ3DK57PACS7W5EY). A selection by the scope removes the
/// file, so the line comes again after a scope stops to work.
///
/// ```
/// use riff::workload::{By, say};
/// let local = tempfile::tempdir()?;
/// let line = say(By::Environment, "w1", Some(local.path()));
/// assert!(line.is_some_and(|l| l.contains("no systemd scope")));
/// assert_eq!(say(By::Environment, "w1", Some(local.path())), None, "one time");
/// assert_eq!(say(By::Scope, "w1", Some(local.path())), None);
/// assert!(say(By::Environment, "w1", Some(local.path())).is_some(), "again after a scope");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn say(by: By, session: &str, local: Option<&Path>) -> Option<String> {
    let said = local.map(|dir| dir.join(SAID_BY_ENVIRONMENT));
    let line = (by == By::Environment).then(|| crate::text::workers_by_environment(session));
    crate::limits::once(said.as_deref(), line)
}

/// [`say`] in the local dir of this machine, on stderr.
pub fn say_here(by: By, session: &str) {
    if let Some(line) = say(by, session, crate::local::dir().as_deref()) {
        eprintln!("{line}");
    }
}

/// The process `pid`, or `None` when it is gone or is not of this user.
pub fn read(pid: u32, context_var: &str) -> Option<Proc> {
    let dir = PathBuf::from(format!("/proc/{pid}"));
    let (ppid, start) = parse_stat(&std::fs::read_to_string(dir.join("stat")).ok()?)?;
    let env = std::fs::read(dir.join("environ")).ok()?;
    let home = std::env::var(crate::home::VAR).ok();
    let home = home.as_deref().filter(|h| !h.is_empty());
    let argv = std::fs::read(dir.join("cmdline")).unwrap_or_default();
    let argv: Vec<String> = argv
        .split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    let (worker, context) = parse_environ(&env, context_var, home);
    let cgroup = std::fs::read_to_string(dir.join("cgroup")).ok();
    let scope = cgroup
        .as_deref()
        .and_then(crate::reap::cgroup_path)
        .and_then(|path| scope_worker(&path));
    Some(Proc {
        pid,
        ppid,
        start,
        argv,
        worker,
        scope,
        context,
    })
}

/// Each process of this user that riff can read.
pub fn all(context_var: &str) -> Vec<Proc> {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    dir.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
        .filter_map(|pid| read(pid, context_var))
        .collect()
}

/// `pid` and each parent of it in `all`.
fn line_of(all: &[Proc], pid: u32) -> BTreeSet<u32> {
    let mut line = BTreeSet::new();
    let mut at = Some(pid);
    while let Some(pid) = at.filter(|pid| *pid > 1 && line.insert(*pid)) {
        at = all.iter().find(|p| p.pid == pid).map(|p| p.ppid);
    }
    line
}

/// Each process of the worker `session` in `all` ([`of_session`]), but
/// `me` and its parents (01M3ZV0TMNQDK9WC3BR1NPGAC2).
pub fn of_worker<'a>(all: &'a [Proc], session: &str, me: u32) -> Selection<'a> {
    let kept = line_of(all, me);
    let mut found = of_session(all, session);
    found.procs.retain(|p| !kept.contains(&p.pid));
    found
}

/// Each process of an old context of the worker `session` in `all`: a
/// process of a context that started before `since`, or each one with
/// no `since`. It keeps `me`, each `riff watch`, and their parents
/// (01M3ZV0QSFVCHRSEKYK57B88VA).
pub fn old_context<'a>(
    all: &'a [Proc],
    session: &str,
    me: u32,
    since: Option<u64>,
) -> Selection<'a> {
    let mut found = of_session(all, session);
    let mut kept = line_of(all, me);
    for watch in found.procs.iter().filter(|p| p.is_watch()) {
        kept.extend(line_of(all, watch.pid));
    }
    found.procs.retain(|p| {
        p.context && !kept.contains(&p.pid) && since.is_none_or(|since| p.start < since)
    });
    found
}

/// Stops each process of `procs`: SIGTERM, then SIGKILL after
/// [`STOP_WAIT`]. It sends no signal to a process whose start changed:
/// its process ID is of a new process. Returns each process that it
/// stopped.
pub fn stop(procs: &[&Proc], context_var: &str) -> Vec<Proc> {
    let same = |p: &Proc| read(p.pid, context_var).is_some_and(|now| now.start == p.start);
    let signal = |p: &Proc, signal| {
        let pid = i32::try_from(p.pid).ok().filter(|pid| *pid > 1)?;
        kill(Pid::from_raw(pid), signal).ok()
    };
    let stopped: Vec<Proc> = procs
        .iter()
        .filter(|p| same(p) && signal(p, Signal::SIGTERM).is_some())
        .map(|p| (*p).clone())
        .collect();
    let end = Instant::now() + STOP_WAIT;
    while stopped.iter().any(same) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
    for p in stopped.iter().filter(|p| same(p)) {
        signal(p, Signal::SIGKILL);
    }
    stopped
}

/// The start of this process, in clock ticks after the boot.
pub fn own_start() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    parse_stat(&stat).map(|(_, start)| start)
}

/// The ID of this boot.
fn boot() -> Option<String> {
    let id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    Some(id.trim().to_owned())
}

fn context_file(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("context-{}", riff_core::name::sanitize(session)))
}

/// Writes the start of the context of the worker `session` to the
/// local dir `dir`: the boot and the start of this process
/// (01M3ZV0TJX2H77RW6ZA3ERZT9H).
pub fn mark(dir: &Path, session: &str) -> std::io::Result<()> {
    let (Some(boot), Some(start)) = (boot(), own_start()) else {
        return Ok(());
    };
    std::fs::create_dir_all(dir)?;
    std::fs::write(context_file(dir, session), format!("{boot} {start}\n"))
}

/// The start of the context of the worker `session` from [`mark`], or
/// `None` when the file is missing or is of an earlier boot.
///
/// ```
/// let run = tempfile::tempdir()?;
/// assert_eq!(riff::workload::context_start(run.path(), "w1"), None);
/// riff::workload::mark(run.path(), "w1")?;
/// assert_eq!(riff::workload::context_start(run.path(), "w1"), riff::workload::own_start());
/// std::fs::write(run.path().join("context-w1"), "an-old-boot 5\n")?;
/// assert_eq!(riff::workload::context_start(run.path(), "w1"), None);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn context_start(dir: &Path, session: &str) -> Option<u64> {
    let text = std::fs::read_to_string(context_file(dir, session)).ok()?;
    let (at, start) = text.trim().split_once(' ')?;
    (Some(at) == boot().as_deref()).then(|| start.parse().ok())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, ppid: u32, worker: Option<&str>, context: bool) -> Proc {
        Proc {
            pid,
            ppid,
            start: 0,
            argv: vec!["x".into()],
            worker: worker.map(str::to_owned),
            scope: None,
            context,
        }
    }

    #[test]
    fn the_whole_worker_is_each_process_of_its_session_but_the_caller() {
        let all = [
            p(10, 1, Some("w1"), false),
            p(11, 10, Some("w1"), true),
            p(12, 11, Some("w1"), true),
            p(20, 1, Some("w2"), true),
            p(30, 1, None, true),
        ];
        let pids: Vec<u32> = of_worker(&all, "w1", 99)
            .procs
            .iter()
            .map(|p| p.pid)
            .collect();
        assert_eq!(pids, [10, 11, 12]);
        let pids: Vec<u32> = of_worker(&all, "w1", 12)
            .procs
            .iter()
            .map(|p| p.pid)
            .collect();
        assert!(pids.is_empty(), "12 and its parents: {pids:?}");
    }

    #[test]
    fn a_process_that_drops_the_variables_stays_in_the_scope_of_its_worker() {
        let scoped = |pid, ppid, worker: Option<&str>, context| Proc {
            scope: Some("w1".into()),
            ..p(pid, ppid, worker, context)
        };
        let all = [
            scoped(10, 1, Some("w1"), false),
            scoped(11, 10, Some("w1"), true),
            // `env -u RIFF_WORKER -u RIFF_SESSION sleep 30 &`
            scoped(12, 11, None, true),
            // The variables of the worker, outside its scope.
            p(20, 1, Some("w1"), true),
        ];
        let stop = of_worker(&all, "w1", 99);
        assert_eq!(stop.by, By::Scope);
        let pids: Vec<u32> = stop.procs.iter().map(|p| p.pid).collect();
        assert_eq!(pids, [10, 11, 12]);
        let clear = old_context(&all, "w1", 99, None);
        let pids: Vec<u32> = clear.procs.iter().map(|p| p.pid).collect();
        assert_eq!(pids, [11, 12]);
    }

    #[test]
    fn a_loop_of_parents_ends() {
        let all = [p(5, 6, None, true), p(6, 5, None, true)];
        assert_eq!(line_of(&all, 5), BTreeSet::from([5, 6]));
    }

    #[test]
    fn this_process_reads_itself() {
        let me = read(std::process::id(), "CLAUDE_PID").expect("own process");
        assert_eq!(Some(me.start), own_start());
        assert!(all("CLAUDE_PID").iter().any(|p| p.pid == me.pid));
    }
}
