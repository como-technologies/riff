//! `riff ps`: the riff processes of this host as a tree (#1610,
//! 01M6QBXNMEBANX439DF7J1PQ3S, 01M6QBXNQC6X7NJVR03M92V0Q8,
//! 01M6QBXNTMH7B2SNXBHAFKCN81, 01M6QBXNY8GM1DRN4RR9TPSEZH).
//!
//! # Design
//!
//! No riff command could say which process made the calls of `gh` that
//! `riff daemon status` counts. A lead in a container cannot see the
//! processes of the host at all. So the daemon reads the processes and
//! answers `riff ps` on its sockets, and `riff ps` draws the answer
//! ([`Ps`], [`draw`]).
//!
//! ```mermaid
//! flowchart LR
//!     P["riff ps on the host"] -- "Ask::Ps, control socket" --> D["riff daemon"]
//!     L["riff ps in the lead container"] -- "ServerCall::Ps, socket of the session" --> D
//!     D -- "platform::processes" --> H["riff processes outside a container,<br/>and their parents up to the first process that is not riff"]
//!     D -- "Runtime::processes: podman top" --> C["the processes of each session container"]
//!     D -- "the tally of gh calls, by PID" --> G["the GitHub calls of each process"]
//! ```
//!
//! | Part | Source | Shows |
//! |---|---|---|
//! | the daemon | [`crate::platform::processes`] | its PID, run time and command line |
//! | a session container | [`Runtime::processes`](crate::runtime::Runtime::processes) | its name, its [`Kind`], its session ID and each process in it |
//! | a riff command outside a container | [`crate::platform::processes`] | the process and its parents up to the first process that is not riff |
//!
//! A process outside a container is a riff process when the program of its
//! command, or the file of its binary, is `riff` or `riff-server`. On
//! Linux, a process of a container shows on the host too, with the PID
//! of the host ([`crate::platform::Process::in_container`]). The daemon
//! leaves it to the container part, which shows the PID that the process
//! has in its container: the PID that its `riff` sends with its calls of
//! `gh` ([`crate::ghcalls::Call::pid`]). The daemon reads only the
//! processes of its own user, and only a lead in a container may ask.
//!
//! Each process that made calls of `gh` in the last hour shows the count
//! and the GraphQL points ([`count_calls`]). A call counts for a process
//! when the PID is the same and the call ended after the process started,
//! so a new process with the PID of an old one shows none of its calls.
//!
//! ```mermaid
//! flowchart TD
//!     W["watch -n 5 riff wave list"] --> R["riff wave list: GitHub, 12 calls, 24 points"]
//!     S["bash"] --> T["riff top"]
//!     S --> V["riff, the TUI"]
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::ghcalls::Call;
use crate::platform::Process;

/// The slack, in milliseconds, between the run time of a process and the
/// time of its first call: a run time has a resolution of one second.
const SLACK_MS: i64 = 2_000;

/// The most processes that `riff ps` shows for one container. A build in
/// a container can run hundreds of processes.
pub const MAX_IN_CONTAINER: usize = 60;

/// The names of the programs that make a process a riff process.
const PROGRAMS: &[&str] = &["riff", "riff-server"];

/// One process.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proc {
    /// The process ID: on the host, or in the container of the process.
    pub pid: u32,
    /// The ID of the parent, in the same space as [`Proc::pid`].
    pub ppid: u32,
    /// How long the process has run, in seconds.
    pub secs: u64,
    /// The command line, with no control character.
    pub args: String,
    /// The calls of `gh` that the process made in the last hour.
    #[serde(default)]
    pub calls: u32,
    /// The GraphQL points of those calls.
    #[serde(default)]
    pub points: u32,
}

/// What a session container is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The session with a terminal.
    Lead,
    /// The build session of a work item.
    Work,
    /// The session that verifies the pull request of an item.
    Verify,
}

impl Kind {
    /// The kind of the session of `item`: the lead has a terminal, and the
    /// item of a verify starts with `verify-`.
    ///
    /// ```
    /// use riff::ps::Kind;
    ///
    /// assert_eq!(Kind::of("issue-12", false), Kind::Work);
    /// assert_eq!(Kind::of("verify-issue-12", false), Kind::Verify);
    /// assert_eq!(Kind::of("lead", true), Kind::Lead);
    /// assert_eq!(Kind::Work.word(), "work item");
    /// ```
    pub fn of(item: &str, lead: bool) -> Self {
        if lead {
            Self::Lead
        } else if item.starts_with("verify-") {
            Self::Verify
        } else {
            Self::Work
        }
    }

    /// The words for a person.
    pub fn word(self) -> &'static str {
        match self {
            Self::Lead => "lead",
            Self::Work => "work item",
            Self::Verify => "verify",
        }
    }
}

/// A session container and its processes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Container {
    /// The name of the container.
    pub name: String,
    /// What the session is for.
    pub kind: Kind,
    /// The work item, for example `issue-12`.
    pub item: String,
    /// The riff session ID.
    pub session: String,
    /// The processes in the container, with their PIDs in it.
    pub procs: Vec<Proc>,
    /// Why the daemon has no list of the processes, when it has none.
    #[serde(default)]
    pub error: Option<String>,
}

/// The answer of the daemon to `riff ps`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ps {
    /// The process ID of the daemon.
    pub daemon: u32,
    /// The riff processes outside a container, and their parents up to the
    /// first process that is not riff. The daemon is one of them.
    pub host: Vec<Proc>,
    /// The session containers, by name.
    pub containers: Vec<Container>,
}

/// True when the program of `process` is riff: the first word of its
/// command, the file of its binary, or its name.
fn is_riff(process: &Process) -> bool {
    let base = |path: &str| path.rsplit('/').next().unwrap_or_default().to_owned();
    let first = process.args.first().map(|arg| base(arg));
    let exe = process
        .exe
        .as_ref()
        .and_then(|exe| exe.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    [first, exe, Some(process.name.clone())]
        .into_iter()
        .flatten()
        .any(|name| PROGRAMS.contains(&name.as_str()))
}

/// The line of `process` for `riff ps`.
fn proc_of(process: &Process) -> Proc {
    let args = if process.args.is_empty() {
        format!("[{}]", process.name)
    } else {
        process.args.join(" ")
    };
    Proc {
        pid: process.pid,
        ppid: process.ppid,
        secs: process.secs,
        args: crate::text::safe(&args).replace(['\n', '\t'], " "),
        calls: 0,
        points: 0,
    }
}

/// The riff processes of `all`, the processes of the host
/// ([`crate::platform::processes`]), with the parents of each up to the
/// first process that is not riff, and the daemon `daemon`. A process
/// in a container is left to
/// [`Runtime::processes`](crate::runtime::Runtime::processes).
///
/// ```
/// use riff::platform::Process;
/// use riff::ps::host;
///
/// let process = |pid, ppid, secs, name: &str, args: &str| Process {
///     pid,
///     ppid,
///     secs,
///     name: name.into(),
///     args: args.split(' ').map(String::from).collect(),
///     ..Process::default()
/// };
/// let all = [
///     process(10, 1, 900, "riff", "/usr/bin/riff daemon"),
///     process(20, 1, 500, "bash", "bash"),
///     process(21, 20, 100, "riff", "riff top"),
///     process(30, 1, 500, "vim", "vim notes"),
///     Process { in_container: true, ..process(40, 1, 50, "riff", "/opt/riff/riff relay") },
/// ];
/// let procs = host(&all, 10);
/// let ids: Vec<(u32, u64, &str)> = procs.iter().map(|p| (p.pid, p.secs, p.args.as_str())).collect();
/// // vim is no riff process and has no riff child.
/// assert_eq!(ids, [(10, 900, "/usr/bin/riff daemon"), (20, 500, "bash"), (21, 100, "riff top")]);
/// ```
pub fn host(all: &[Process], daemon: u32) -> Vec<Proc> {
    let all: BTreeMap<u32, &Process> = all
        .iter()
        .filter(|process| !process.in_container)
        .map(|process| (process.pid, process))
        .collect();
    keep(&all, daemon)
}

/// The riff processes of `all` and the parents of each, up to and with the
/// first process that is not riff. The daemon `daemon` is the end of a
/// chain too: it is riff.
fn keep(all: &BTreeMap<u32, &Process>, daemon: u32) -> Vec<Proc> {
    let mut kept: BTreeSet<u32> = BTreeSet::new();
    for process in all.values().filter(|process| is_riff(process)) {
        kept.insert(process.pid);
        if process.pid == daemon {
            continue;
        }
        let mut parent = process.ppid;
        // A process that is kept already ends the chain: also a loop of
        // parents, which a changing list of processes can show.
        while let Some(up) = all.get(&parent) {
            if !kept.insert(up.pid) || up.pid == daemon || !is_riff(up) {
                break;
            }
            parent = up.ppid;
        }
    }
    kept.iter()
        .filter_map(|pid| all.get(pid))
        .map(|process| proc_of(process))
        .collect()
}

/// The processes in the output of `podman top NAME pid ppid etime args`
/// ([`crate::podman::top_args`]). The first line holds the names of the columns.
///
/// ```
/// use riff::ps::parse_top;
///
/// let out = "PID   PPID  ELAPSED     COMMAND\n\
///            1     0     3-04:05:06  /opt/riff/riff relay -- claude -p\n\
///            14    1     12:30       claude -p --verbose\n\
///            90    14    5           sleep 30\n";
/// let procs = parse_top(out);
/// assert_eq!(procs.len(), 3);
/// assert_eq!((procs[0].pid, procs[0].ppid, procs[0].secs), (1, 0, 3 * 86_400 + 4 * 3_600 + 5 * 60 + 6));
/// assert_eq!(procs[0].args, "/opt/riff/riff relay -- claude -p");
/// assert_eq!(procs[1].secs, 12 * 60 + 30);
/// assert_eq!(procs[2].secs, 5);
/// ```
pub fn parse_top(output: &str) -> Vec<Proc> {
    output.lines().skip(1).filter_map(top_line).collect()
}

fn top_line(line: &str) -> Option<Proc> {
    let (pid, rest) = word(line)?;
    let (ppid, rest) = word(rest)?;
    let (etime, args) = word(rest)?;
    Some(Proc {
        pid: pid.parse().ok()?,
        ppid: ppid.parse().ok()?,
        secs: etime_secs(etime)?,
        args: crate::text::safe(args.trim()).replace(['\n', '\t'], " "),
        calls: 0,
        points: 0,
    })
}

/// The first word of `text`, and what follows it.
fn word(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    Some(text.split_once(char::is_whitespace).unwrap_or((text, "")))
}

/// The seconds of a time of `ps`: `[[DD-]hh:]mm:ss`.
fn etime_secs(etime: &str) -> Option<u64> {
    let (days, clock) = match etime.split_once('-') {
        Some((days, clock)) => (days.parse::<u64>().ok()?, clock),
        None => (0, etime),
    };
    let mut secs = 0;
    for part in clock.split(':') {
        secs = secs * 60 + part.parse::<u64>().ok()?;
    }
    Some(days * 86_400 + secs)
}

/// Sets the calls and the points of each process of `procs` from `calls`,
/// the calls of the last hour at `now_ms`. A call counts for the process
/// with its PID in the container `session` (empty for the host) when it
/// ended after the process started.
///
/// ```
/// use riff::ghcalls::Call;
/// use riff::ps::{Proc, count_calls};
///
/// let call = |at_ms, pid, session: &str, points| Call {
///     at_ms, pid, points, session: session.into(), ..Call::default()
/// };
/// let mut procs = vec![
///     Proc { pid: 7, secs: 100, args: "riff top".into(), ..Proc::default() },
///     Proc { pid: 8, secs: 2, args: "riff pr wait".into(), ..Proc::default() },
/// ];
/// let calls = [
///     call(950_000, 7, "", 2),
///     call(960_000, 7, "", 2),
///     // The same PID in a container is another process.
///     call(960_000, 7, "riff-issue-1-aa", 5),
///     // A call before the process started: an old process with this PID.
///     call(500_000, 8, "", 9),
/// ];
/// count_calls(&mut procs, "", &calls, 1_000_000);
/// assert_eq!((procs[0].calls, procs[0].points), (2, 4));
/// assert_eq!((procs[1].calls, procs[1].points), (0, 0));
/// ```
pub fn count_calls(procs: &mut [Proc], session: &str, calls: &[Call], now_ms: i64) {
    for proc in procs {
        let started = now_ms - i64::try_from(proc.secs).unwrap_or(i64::MAX) * 1_000;
        let mine = calls.iter().filter(|call| {
            call.pid == proc.pid && call.session == session && call.at_ms + SLACK_MS >= started
        });
        (proc.calls, proc.points) = mine.fold((0, 0), |(calls, points), call| {
            (calls + 1, points + call.points)
        });
    }
}

/// The text of `riff ps` for a terminal of `width` columns. Each line holds
/// the PID, the run time and the command line, cut to the width. A process
/// that made calls of `gh` ends with the count, which the cut keeps.
///
/// ```
/// use riff::ps::{Container, Kind, Proc, Ps, draw};
///
/// let proc = |pid, ppid, secs, args: &str| Proc { pid, ppid, secs, args: args.into(), ..Proc::default() };
/// let mut top = proc(3002, 3001, 5, "riff wave list");
/// (top.calls, top.points) = (12, 24);
/// let ps = Ps {
///     daemon: 812,
///     host: vec![
///         proc(812, 1, 3 * 86_400, "riff daemon"),
///         proc(3001, 900, 7_200, "watch -n 5 riff wave list"),
///         top,
///     ],
///     containers: vec![Container {
///         name: "riff-issue-12-1a2b3c4d".into(),
///         kind: Kind::Work,
///         item: "issue-12".into(),
///         session: "1a2b3c4d-0000-4000-8000-000000000000".into(),
///         procs: vec![proc(1, 0, 600, "/opt/riff/riff relay -- claude"), proc(14, 1, 590, "claude -p")],
///         error: None,
///     }],
/// };
/// let text = draw(&ps, 100);
/// assert_eq!(text, "\
/// Daemon:
///   812  3d  riff daemon
/// Containers:
///   riff-issue-12-1a2b3c4d  work item  issue-12  session 1a2b3c4d-0000-4000-8000-000000000000
///     1  10m  /opt/riff/riff relay -- claude
///     └─ 14  9m  claude -p
/// Outside a container:
///   3001  2h  watch -n 5 riff wave list
///   └─ 3002  5s  riff wave list  GitHub, last hour: 12 calls, 24 points
/// ");
/// // A narrow terminal cuts each line, and keeps the calls whole.
/// let narrow = draw(&ps, 60);
/// assert!(narrow.lines().all(|line| line.chars().count() <= 60), "{narrow}");
/// assert!(narrow.contains("…  GitHub, last hour: 12 calls, 24 points\n"), "{narrow}");
/// ```
pub fn draw(ps: &Ps, width: usize) -> String {
    let mut out = String::new();
    let (daemon, others) = split_daemon(&ps.host, ps.daemon);
    out.push_str("Daemon:\n");
    if daemon.is_empty() {
        let _ = writeln!(out, "  {}", crate::text::PS_DAEMON_NOT_SEEN);
    }
    tree(&mut out, &daemon, "  ", width);
    if ps.containers.is_empty() {
        out.push_str("Containers: none.\n");
    } else {
        out.push_str("Containers:\n");
    }
    for container in &ps.containers {
        // The item of the lead is its name, `lead`: it says nothing more.
        let item = match container.kind {
            Kind::Lead => String::new(),
            _ => format!("  {}", container.item),
        };
        let line = format!(
            "  {}  {}{item}  session {}",
            container.name,
            container.kind.word(),
            container.session
        );
        let _ = writeln!(out, "{}", cut(&line, width, ""));
        if let Some(error) = &container.error {
            let line = format!("    {}", crate::text::ps_no_list(error));
            let _ = writeln!(out, "{}", cut(&line, width, ""));
            continue;
        }
        let shown = container.procs.len().min(MAX_IN_CONTAINER);
        tree(&mut out, &container.procs[..shown], "    ", width);
        if container.procs.len() > shown {
            let more = crate::text::ps_more(container.procs.len() - shown);
            let _ = writeln!(out, "    {more}");
        }
    }
    if others.is_empty() {
        out.push_str("Outside a container: no other riff process.\n");
    } else {
        out.push_str("Outside a container:\n");
        tree(&mut out, &others, "  ", width);
    }
    out
}

/// The processes of the tree of the daemon, and the others.
fn split_daemon(host: &[Proc], daemon: u32) -> (Vec<Proc>, Vec<Proc>) {
    let children = children_of(host);
    let mut mine = BTreeSet::new();
    let mut stack = vec![daemon];
    while let Some(pid) = stack.pop() {
        if host.iter().any(|proc| proc.pid == pid) && mine.insert(pid) {
            stack.extend(children.get(&pid).into_iter().flatten().copied());
        }
    }
    host.iter()
        .cloned()
        .partition(|proc| mine.contains(&proc.pid))
}

/// The PIDs of the children of each process of `procs`, in order.
fn children_of(procs: &[Proc]) -> BTreeMap<u32, Vec<u32>> {
    let known: BTreeSet<u32> = procs.iter().map(|proc| proc.pid).collect();
    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for proc in procs {
        if known.contains(&proc.ppid) && proc.ppid != proc.pid {
            children.entry(proc.ppid).or_default().push(proc.pid);
        }
    }
    for list in children.values_mut() {
        list.sort_unstable();
    }
    children
}

/// The place in a tree that a line draws.
struct Walk<'a> {
    by_pid: BTreeMap<u32, &'a Proc>,
    children: BTreeMap<u32, Vec<u32>>,
    indent: &'a str,
    width: usize,
    seen: BTreeSet<u32>,
}

/// Draws `procs` as trees, each line after `indent`. A process whose
/// parent is not in `procs` is the root of a tree.
fn tree(out: &mut String, procs: &[Proc], indent: &str, width: usize) {
    let mut walk = Walk {
        by_pid: procs.iter().map(|proc| (proc.pid, proc)).collect(),
        children: children_of(procs),
        indent,
        width,
        seen: BTreeSet::new(),
    };
    let mut roots: Vec<u32> = procs
        .iter()
        .filter(|proc| !walk.by_pid.contains_key(&proc.ppid) || proc.ppid == proc.pid)
        .map(|proc| proc.pid)
        .collect();
    roots.sort_unstable();
    for root in roots {
        walk.branch(out, root, "", "");
    }
}

impl Walk<'_> {
    /// Draws the process `pid` after `lead`, and its children after
    /// `rest`.
    fn branch(&mut self, out: &mut String, pid: u32, lead: &str, rest: &str) {
        let Some(proc) = self.by_pid.get(&pid).copied() else {
            return;
        };
        if !self.seen.insert(pid) {
            return;
        }
        let calls = crate::text::ps_calls(proc.calls, proc.points);
        let head = format!(
            "{}{lead}{}  {}  {}",
            self.indent,
            proc.pid,
            crate::text::ago(proc.secs),
            proc.args
        );
        let _ = writeln!(out, "{}", cut(&head, self.width, &calls));
        let kids = self.children.get(&pid).cloned().unwrap_or_default();
        for (i, kid) in kids.iter().enumerate() {
            let (down, next) = if i + 1 == kids.len() {
                ("└─ ", "   ")
            } else {
                ("├─ ", "│  ")
            };
            self.branch(
                out,
                *kid,
                &format!("{rest}{down}"),
                &format!("{rest}{next}"),
            );
        }
    }
}

/// `line` cut to `width` columns, then `suffix`. A cut line ends with `…`.
/// The suffix stays whole: it says the calls.
fn cut(line: &str, width: usize, suffix: &str) -> String {
    let room = width.saturating_sub(suffix.chars().count());
    if line.chars().count() <= room {
        return format!("{line}{suffix}");
    }
    let head: String = line.chars().take(room.saturating_sub(1)).collect();
    format!("{head}…{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(pid: u32, ppid: u32, args: &str) -> Process {
        Process {
            pid,
            ppid,
            secs: 1,
            args: args.split(' ').map(str::to_owned).collect(),
            name: args
                .split(' ')
                .next()
                .unwrap()
                .rsplit('/')
                .next()
                .unwrap()
                .into(),
            ..Process::default()
        }
    }

    /// 01M6QBXNMEBANX439DF7J1PQ3S (#1610): a riff command shows with its
    /// parents up to the first process that is not riff, and nothing above
    /// it.
    #[test]
    fn a_riff_command_shows_with_its_parents_up_to_the_first_that_is_not_riff() {
        let procs = [
            raw(1, 0, "systemd"),
            raw(100, 1, "tmux"),
            raw(200, 100, "bash"),
            raw(300, 200, "watch -n 5 riff wave list"),
            raw(400, 300, "riff wave list"),
            raw(500, 400, "riff pr wait"),
            raw(600, 200, "vim x"),
        ];
        let kept: Vec<u32> = host(&procs, 9).iter().map(|p| p.pid).collect();
        // `riff pr wait` has a riff parent, so the chain goes on to `watch`.
        assert_eq!(kept, [300, 400, 500]);
    }

    /// The program is riff by its name, by the file of its binary, also one
    /// that a new `riff update` replaced, and by its name.
    #[test]
    fn a_process_is_riff_by_its_command_its_binary_or_its_name() {
        let mut one = raw(1, 0, "/home/m/.local/bin/riff top");
        assert!(is_riff(&one));
        one.args = vec!["tmux".into()];
        one.name = "tmux".into();
        assert!(!is_riff(&one));
        one.exe = Some("/home/m/.local/bin/riff".into());
        one.replaced = true;
        assert!(is_riff(&one));
        one.exe = Some("/usr/bin/riffraff".into());
        assert!(!is_riff(&one));
        let server = raw(2, 0, "riff-server --listen x");
        assert!(is_riff(&server));
        // A program that only names riff in its arguments is not riff.
        assert!(!is_riff(&raw(3, 0, "grep riff x")));
    }

    /// The daemon ends a chain: its parent is not shown.
    #[test]
    fn the_chain_ends_at_the_daemon() {
        let procs = [
            raw(1, 0, "systemd"),
            raw(10, 1, "riff daemon"),
            raw(11, 10, "riff pr wait"),
        ];
        let kept: Vec<u32> = host(&procs, 10).iter().map(|p| p.pid).collect();
        assert_eq!(kept, [10, 11]);
    }

    /// A loop in the parents, which a changing list of processes can show,
    /// ends.
    #[test]
    fn a_loop_of_parents_ends() {
        let procs = [raw(1, 2, "riff a"), raw(2, 1, "riff b")];
        assert_eq!(host(&procs, 9).len(), 2);
    }

    #[test]
    fn the_time_of_ps_is_seconds() {
        assert_eq!(etime_secs("05"), Some(5));
        assert_eq!(etime_secs("01:05"), Some(65));
        assert_eq!(etime_secs("1:01:05"), Some(3_665));
        assert_eq!(etime_secs("2-01:01:05"), Some(2 * 86_400 + 3_665));
        assert_eq!(etime_secs("soon"), None);
    }

    /// The command line of a process holds anything: the text has no
    /// control character that a terminal would act on.
    #[test]
    fn a_command_line_has_no_control_character() {
        let procs = parse_top("PID PPID ELAPSED COMMAND\n5 1 00:03 echo \x1b]0;x\x07 a\tb\n");
        assert_eq!(procs[0].args, "echo  a b");
        // A line with no number is no process.
        assert!(parse_top("PID PPID ELAPSED COMMAND\nUSER x y z\n\n").is_empty());
    }

    /// 01M6QBXNTMH7B2SNXBHAFKCN81 (#1610): a call counts for the process
    /// with its ID in its container.
    #[test]
    fn the_calls_of_a_process_in_a_container_need_the_same_container() {
        let mut procs = vec![Proc {
            pid: 5,
            secs: 10,
            ..Proc::default()
        }];
        let calls = [Call {
            at_ms: 99_000,
            pid: 5,
            points: 3,
            session: "riff-a".into(),
            ..Call::default()
        }];
        count_calls(&mut procs, "riff-b", &calls, 100_000);
        assert_eq!(procs[0].calls, 0);
        count_calls(&mut procs, "riff-a", &calls, 100_000);
        assert_eq!((procs[0].calls, procs[0].points), (1, 3));
    }

    /// A container whose list the runtime cannot give shows why. A
    /// process that no tree holds is a root.
    #[test]
    fn a_container_with_no_list_shows_why() {
        let ps = Ps {
            daemon: 1,
            host: Vec::new(),
            containers: vec![Container {
                name: "riff-lead-o-r".into(),
                kind: Kind::Lead,
                item: "lead".into(),
                session: "s1".into(),
                procs: Vec::new(),
                error: Some("podman does not answer".into()),
            }],
        };
        let text = draw(&ps, 200);
        assert_eq!(
            text,
            "Daemon:\n  The riff daemon answers, but riff cannot read its process.\nContainers:\n  \
             riff-lead-o-r  lead  session s1\n    No list of processes: podman does not \
             answer\nOutside a container: no other riff process.\n"
        );
    }

    /// A line cut keeps the calls, and a tree of a container with many
    /// processes ends with a count.
    #[test]
    fn a_cut_line_keeps_the_calls_and_a_long_list_ends_with_a_count() {
        assert_eq!(cut("abcdef", 6, ""), "abcdef");
        assert_eq!(cut("abcdefg", 6, ""), "abcde…");
        assert_eq!(cut("abcdefg", 8, "  x"), "abcd…  x");
        let procs: Vec<Proc> = (1..=70)
            .map(|pid| Proc {
                pid,
                ppid: u32::from(pid > 1),
                args: "sleep".into(),
                ..Proc::default()
            })
            .collect();
        let ps = Ps {
            daemon: 1,
            host: Vec::new(),
            containers: vec![Container {
                name: "c".into(),
                kind: Kind::Verify,
                item: "verify-issue-1".into(),
                session: "s".into(),
                procs,
                error: None,
            }],
        };
        let text = draw(&ps, 80);
        assert!(text.contains("verify  verify-issue-1  session s"), "{text}");
        assert!(text.contains(&crate::text::ps_more(10)), "{text}");
    }
}
