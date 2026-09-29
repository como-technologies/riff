//! The output of the riff commands for people: one look for each
//! command (01M3Q5V313XQN86BA2PBTXHEZC).
//!
//! - Facts are `key  value` lines, aligned: see [`facts`].
//! - A list is a table with a header row: see [`table`]. A row names a
//!   session by its short ID. `--long` shows the full URI or ID in its
//!   place, never both on a row.
//! - A status is on the row of its session, in its own column.
//! - A setting is `key  value  (file)`. The hint how to change it comes
//!   once, dim, at the end: see [`setting`].
//! - An action that the person must take comes last, in yellow or red.
//!
//! The views have ANSI styles. Print them through `anstream`, which
//! removes the styles when the output has no color. `--color` works as
//! in `riff tail` (01M3JDCA9070MY30AYHK3Y67EF), so a pipe gets plain
//! text that is easy to grep.
//!
//! ```
//! use riff::view::{facts, table};
//!
//! assert_eq!(facts(&[("riff", "running".into()), ("owner", "ada".into())]),
//!            "riff   running\nowner  ada\n");
//! let rows = [vec!["%3".into(), "live".into()], vec!["%10".into(), "idle 2m".into()]];
//! let plain = anstream::adapter::strip_str(&table(&["PANE", "STATE"], &rows)).to_string();
//! assert_eq!(plain, "PANE  STATE\n%3    live\n%10   idle 2m\n");
//! ```

use std::fmt::Write;
use std::path::Path;

use riff_core::build::Build;
use riff_core::name::SessionUri;
use riff_core::wire::{MembersReply, RiffOwner, RiffState, SessionInfo};

use crate::style::{BOLD, DIM, ERROR, GOOD, WARNING, styled};
use crate::text::{self, safe};

/// `text` in `style`, or nothing when `text` is empty: so a line has no
/// spaces at its end.
fn paint(style: anstyle::Style, text: &str) -> String {
    if text.is_empty() {
        String::new()
    } else {
        styled(style, text)
    }
}

/// The width of `text` on a terminal, with no ANSI styles.
fn width(text: &str) -> usize {
    anstream::adapter::strip_str(text)
        .to_string()
        .chars()
        .count()
}

/// `text` padded with spaces to `cols` columns.
fn pad(text: &str, cols: usize) -> String {
    let mut out = text.to_owned();
    out.extend(std::iter::repeat_n(' ', cols.saturating_sub(width(text))));
    out
}

/// One line for each fact: the key, padded to the widest key, two
/// spaces, and the value.
pub fn facts(rows: &[(&str, String)]) -> String {
    let cols = rows.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
    let mut out = String::new();
    for (key, value) in rows {
        let _ = writeln!(out, "{}", format!("{key:<cols$}  {value}").trim_end());
    }
    out
}

/// A table: the `head` row in bold, then each row. Each column is as
/// wide as its widest cell, with two spaces between columns. A line
/// has no spaces at its end.
pub fn table(head: &[&str], rows: &[Vec<String>]) -> String {
    let mut cols: Vec<usize> = head.iter().map(|h| h.len()).collect();
    for row in rows {
        for (col, cell) in cols.iter_mut().zip(row) {
            *col = (*col).max(width(cell));
        }
    }
    let line = |cells: Vec<String>| {
        let mut line = String::new();
        let last = cells.len().saturating_sub(1);
        for (i, (cell, col)) in cells.iter().zip(&cols).enumerate() {
            if i == last {
                line.push_str(cell);
            } else {
                let _ = write!(line, "{}  ", pad(cell, *col));
            }
        }
        format!("{}\n", line.trim_end())
    };
    let mut out = line(head.iter().map(|h| styled(BOLD, h)).collect());
    for row in rows {
        out.push_str(&line(row.clone()));
    }
    out
}

/// A setting: `key  value  (file)`, and the dim `hint` how to change it
/// on the last line.
///
/// ```
/// let out = riff::view::setting("workers.limit", "2", "/h/config.toml".as_ref(),
///     "Set it with: riff workers limit N");
/// let plain = anstream::adapter::strip_str(&out).to_string();
/// assert_eq!(plain, "workers.limit  2  (/h/config.toml)\nSet it with: riff workers limit N");
/// ```
pub fn setting(key: &str, value: &str, path: &Path, hint: &str) -> String {
    let file = format!("({})", path.display());
    format!(
        "{key}  {value}  {}\n{}",
        styled(DIM, &file),
        styled(DIM, hint)
    )
}

/// `riff update --auto` (01M3N7JJC5WQBJ7SJZSZNBAVVR).
///
/// ```
/// let on = riff::view::auto_update(true, "/h/config.toml".as_ref());
/// let plain = anstream::adapter::strip_str(&on).to_string();
/// assert_eq!(plain, "update.auto  true  (/h/config.toml)\nTurn it off with: riff update --auto off");
/// let off = riff::view::auto_update(false, "/h/config.toml".as_ref());
/// assert!(off.contains("riff update --auto on"));
/// ```
pub fn auto_update(on: bool, path: &Path) -> String {
    let hint = if on {
        "Turn it off with: riff update --auto off"
    } else {
        "Turn it on with: riff update --auto on"
    };
    setting("update.auto", &on.to_string(), path, hint)
}

/// `riff lead compact` (01M3Q88GBSRJRP4VGVDV3EJZ4R).
///
/// ```
/// let on = riff::view::lead_compact(true, 60, "/h/config.toml".as_ref());
/// let plain = anstream::adapter::strip_str(&on).to_string();
/// assert_eq!(
///     plain,
///     "lead.compact  true  (/h/config.toml)\n\
///      lead.quiet  60  (/h/config.toml)\n\
///      At the end of a wave, riff compacts the lead after 60 seconds with no input. \
///      Turn it off with: riff lead compact off"
/// );
/// let off = riff::view::lead_compact(false, 60, "/h/config.toml".as_ref());
/// assert!(off.contains("Turn it on with: riff lead compact on"), "{off}");
/// ```
pub fn lead_compact(on: bool, quiet: u64, path: &Path) -> String {
    let hint = if on {
        format!(
            "At the end of a wave, riff compacts the lead after {quiet} seconds with no input. \
             Turn it off with: riff lead compact off"
        )
    } else {
        "riff does not compact the lead. Turn it on with: riff lead compact on".to_owned()
    };
    let file = format!("({})", path.display());
    format!(
        "lead.compact  {on}  {}\n{}",
        styled(DIM, &file),
        setting("lead.quiet", &quiet.to_string(), path, &hint)
    )
}

/// `riff workers limit` (01M3JPQT35BMR7XMAMMFSCDC2B).
pub fn workers_limit(limit: u16, path: &Path) -> String {
    setting(
        "workers.limit",
        &limit.to_string(),
        path,
        "Set it with: riff workers limit N",
    )
}

/// `riff workers interval` (01M3Q5QE9H42FQKEDC5G9GKCWD): the most
/// seconds between two workers that the lead starts by itself. 0 turns
/// it off.
///
/// ```
/// let out = riff::view::workers_interval(10, "/h/c.toml".as_ref());
/// assert_eq!(
///     anstream::adapter::strip_str(&out).to_string(),
///     "workers.interval  10  (/h/c.toml)\n\
///      The lead starts at most one worker each 10 seconds. \
///      Set it with: riff workers interval SECONDS (0 turns it off)"
/// );
/// assert!(riff::view::workers_interval(0, "/h/c.toml".as_ref())
///     .contains("The lead starts no worker by itself."));
/// ```
pub fn workers_interval(seconds: u16, path: &Path) -> String {
    let what = match seconds {
        0 => "starts no worker by itself".to_owned(),
        1 => "starts at most one worker each second".to_owned(),
        n => format!("starts at most one worker each {n} seconds"),
    };
    setting(
        "workers.interval",
        &seconds.to_string(),
        path,
        &format!("The lead {what}. Set it with: riff workers interval SECONDS (0 turns it off)"),
    )
}

/// `riff workers mcp` (01M3NB5R6X5AV79DQNKKJBH5J8).
///
/// ```
/// let out = riff::view::workers_mcp(&["riff".into(), "github".into()], "/h/c.toml".as_ref());
/// assert!(anstream::adapter::strip_str(&out).to_string()
///     .starts_with("workers.mcp  riff, github  (/h/c.toml)\n"));
/// ```
pub fn workers_mcp(names: &[String], path: &Path) -> String {
    setting(
        "workers.mcp",
        &names.join(", "),
        path,
        "Change it with: riff workers mcp add NAME, or riff workers mcp remove NAME",
    )
}

/// The facts of the build: `riff` and, when the server of the last call
/// runs another build, `riff-server` in yellow
/// (01M3JEE7WT04BKX377VW5GDSPY).
pub(crate) fn build_facts(server: Option<&Build>) -> Vec<(&'static str, String)> {
    let this = Build::this();
    let mut rows = vec![("build", text::build_facts(&this))];
    if let Some(server) = server.filter(|s| !s.matches(&this)) {
        let facts = format!(
            "{}  another build; the versions can talk",
            text::build_facts(server)
        );
        rows.push(("riff-server", styled(WARNING, &facts)));
    }
    rows
}

/// The fact of the state of the riff, and the action for a paused
/// riff.
pub(crate) fn state_fact(state: RiffState) -> ((&'static str, String), Option<String>) {
    match state {
        RiffState::Running => (("riff", styled(GOOD, "running")), None),
        RiffState::Paused => (
            ("riff", styled(WARNING, "paused")),
            Some(styled(
                WARNING,
                "The riff is paused. Nobody claims work. Your user or the lead resumes it with: \
                 riff resume",
            )),
        ),
    }
}

/// `riff whoami`: the session, its URI, the state of the riff and the
/// build. `state` is the error text when riff cannot read the state.
///
/// ```
/// use riff_core::wire::RiffState;
///
/// let me = "riff://mike@pangolin/como-technologies/riff?session=a6cf2205-1".parse()?;
/// let out = riff::view::whoami(&me, Ok(RiffState::Running));
/// let plain = anstream::adapter::strip_str(&out).to_string();
/// let lines: Vec<&str> = plain.lines().collect();
/// assert_eq!(lines[0], "session  mike@pangolin:riff (a6cf2205)");
/// assert_eq!(lines[1], "uri      riff://mike@pangolin/como-technologies/riff?session=a6cf2205-1");
/// assert_eq!(lines[2], "riff     running");
/// assert!(lines[3].starts_with("build    v"), "{plain}");
///
/// let paused = riff::view::whoami(&me, Ok(RiffState::Paused));
/// let plain = anstream::adapter::strip_str(&paused).to_string();
/// assert!(plain.ends_with("resumes it with: riff resume\n"), "{plain}");
/// let down = riff::view::whoami(&me, Err("refused".into()));
/// assert!(anstream::adapter::strip_str(&down).to_string().contains("riff     unknown: refused\n"));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn whoami(me: &SessionUri, state: Result<RiffState, String>) -> String {
    let mut rows = vec![
        (
            "session",
            styled(crate::style::session(me), &safe(&text::name(me))),
        ),
        ("uri", safe(&me.to_string())),
    ];
    let mut action = None;
    match state {
        Ok(state) => {
            let (fact, act) = state_fact(state);
            rows.push(fact);
            action = act;
            rows.extend(build_facts(crate::api::server_build().as_ref()));
        }
        Err(e) => rows.push(("riff", styled(ERROR, &format!("unknown: {}", safe(&e))))),
    }
    let mut out = facts(&rows);
    if let Some(action) = action {
        let _ = writeln!(out, "{action}");
    }
    out
}

/// `riff who` for people (01M3Q63MVZ74WPNBA3QJYQGHFG): the facts of the
/// riff, then a table with a row for each session.
///
/// - The facts: the state of the riff (`running` in green, `paused` in
///   yellow), the owner ([`text::owner_line`]; none with no sign-in),
///   and the build.
/// - The columns: SESSION (the [`text::name`] in the color of the
///   session, as in `riff tail`), STATE (the word of the state that the
///   server derives, in its color: see [`crate::state`]), ROLE (`you`
///   in bold and the [`text::tags`]) and DETAIL (the
///   [`crate::state::detail`]). With `long`, the column URI takes the
///   place of SESSION.
/// - The actions come last: resume a paused riff, and take the owner
///   role of a riff with no owner.
///
/// Each text from the server is [`safe`].
///
/// ```
/// use riff_core::wire::{RiffOwner, RiffState, SessionInfo, SessionState, Status, StatusInfo};
///
/// let me = "riff://mike@pangolin/como-technologies/riff?session=a6cf&lead=true&claim=issue-6#issue-6"
///     .parse()?;
/// let brett = "riff://brett@heron/como-technologies/riff?session=77e0".parse()?;
/// let blocked = Status { step: "merge".into(), blocked: Some("waits for a review".into()) };
/// let list = [
///     SessionInfo {
///         uri: me,
///         live: true,
///         idle_secs: 0,
///         status: None,
///         worker: false,
///         stopping: false,
///         claims_secs: 0,
///         state: SessionState::Busy,
///     },
///     SessionInfo {
///         uri: brett,
///         live: true,
///         idle_secs: 0,
///         status: Some(StatusInfo { status: blocked, age_secs: 60, stale: false }),
///         worker: false,
///         stopping: false,
///         claims_secs: 0,
///         state: SessionState::Blocked,
///     },
/// ];
/// let owner = RiffOwner::Owner { user: "mike".into(), email: "mike@x.io".into() };
/// let text = riff::view::who(RiffState::Running, &owner, &list, &list[0].uri, false);
/// let plain = anstream::adapter::strip_str(&text).to_string();
/// let lines: Vec<&str> = plain.lines().collect();
/// assert_eq!(lines[0], "riff   running");
/// assert_eq!(lines[1], "owner  mike (mike@x.io)");
/// assert!(lines[2].starts_with("build  v"));
/// assert_eq!(lines[3], "");
/// assert_eq!(lines[4], "SESSION                            STATE    ROLE      DETAIL");
/// assert_eq!(lines[5], "mike@pangolin:riff#issue-6 (a6cf)  busy     you lead  working on #6");
/// assert_eq!(
///     lines[6],
///     "brett@heron:riff (77e0)            blocked            waits for a review (step: merge, 1m ago)"
/// );
/// let red = riff::style::ERROR;
/// assert!(text.contains(&format!("{red}blocked{red:#}")));
/// assert!(text.contains(&format!("{red}waits for a review (step: merge, 1m ago){red:#}")));
///
/// // --long shows the URI in place of the name.
/// let long = riff::view::who(RiffState::Running, &owner, &list, &list[0].uri, true);
/// let plain = anstream::adapter::strip_str(&long).to_string();
/// assert!(plain.contains("\nURI "), "{plain}");
/// assert!(plain.contains("\nriff://brett@heron/como-technologies/riff?session=77e0 "), "{plain}");
///
/// // A riff with no sign-in shows no owner.
/// let text = riff::view::who(RiffState::Running, &RiffOwner::NoSignIn, &list, &list[0].uri, false);
/// assert!(!anstream::adapter::strip_str(&text).to_string().contains("owner"));
///
/// // A riff with no owner ends with the action.
/// let text = riff::view::who(RiffState::Running, &RiffOwner::Nobody, &list, &list[0].uri, false);
/// let plain = anstream::adapter::strip_str(&text).to_string();
/// assert!(plain.contains("\nowner  none\n"), "{plain}");
/// assert!(plain.ends_with("riff owner --take\n"), "{plain}");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn who(
    state: RiffState,
    owner: &RiffOwner,
    sessions: &[SessionInfo],
    me: &SessionUri,
    long: bool,
) -> String {
    let (fact, paused) = state_fact(state);
    let mut rows = vec![fact];
    let mut actions: Vec<String> = paused.into_iter().collect();
    match owner {
        RiffOwner::NoSignIn => {}
        RiffOwner::Nobody => {
            rows.push(("owner", "none".into()));
            actions.push(styled(WARNING, text::NO_OWNER));
        }
        RiffOwner::Owner { user, email } => {
            rows.push(("owner", format!("{} ({})", safe(user), safe(email))));
        }
    }
    rows.extend(build_facts(crate::api::server_build().as_ref()));
    let mut out = facts(&rows);
    out.push('\n');
    if sessions.is_empty() {
        out.push_str("Nobody is in the riff.\n");
    } else {
        let head = [
            if long { "URI" } else { "SESSION" },
            "STATE",
            "ROLE",
            "DETAIL",
        ];
        let rows: Vec<Vec<String>> = sessions
            .iter()
            .map(|s| {
                let mut role: Vec<String> = Vec::new();
                if s.uri.who() == me.who() {
                    role.push(styled(BOLD, "you"));
                }
                role.extend(text::tags(s, owner).into_iter().map(String::from));
                let name = if long {
                    styled(DIM, &safe(&s.uri.to_string()))
                } else {
                    styled(crate::style::session(&s.uri), &safe(&text::name(&s.uri)))
                };
                vec![name, state_cell(s), role.join(" "), detail_cell(s)]
            })
            .collect();
        out.push_str(&table(&head, &rows));
    }
    for action in actions {
        let _ = writeln!(out, "{action}");
    }
    out
}

/// The word of the state of `s`, in its color.
fn state_cell(s: &SessionInfo) -> String {
    styled(crate::state::style(s.state), s.state.word())
}

/// The DETAIL cell of `s`: each line of the [`crate::state::detail`]
/// with its style, on one line.
///
/// ```
/// use riff_core::wire::{SessionInfo, SessionState, Status, StatusInfo};
///
/// let s = SessionInfo {
///     uri: "riff://mike@thelio/o/r?session=w1&claim=issue-12".parse()?,
///     live: true,
///     idle_secs: 0,
///     status: Some(StatusInfo {
///         status: Status { step: "tests".into(), blocked: None },
///         age_secs: 7200,
///         stale: false,
///     }),
///     worker: true,
///     stopping: false,
///     claims_secs: 300,
///     state: SessionState::Busy,
/// };
/// let plain = anstream::adapter::strip_str(&riff::view::detail_cell(&s)).to_string();
/// assert_eq!(plain, "working on #12  2h ago: tests");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn detail_cell(s: &SessionInfo) -> String {
    crate::state::detail(s, &|_| None)
        .iter()
        .map(|(line, style)| paint(*style, line))
        .collect::<Vec<_>>()
        .join("  ")
}

/// The heading of the workers of one machine in `riff workers`
/// (01M3N7AKFPX3ZGQARSG2V64GBD): the host in bold, its limit, the
/// workers that run, and the numbers and the score of the machine
/// when riff knows them (01M3Q5QE4SQ8VYN2PSF42KB3QJ).
///
/// ```
/// use riff::machine::Machine;
///
/// let plain = |s: String| anstream::adapter::strip_str(&s).to_string();
/// assert_eq!(plain(riff::view::host_heading("pangolin", 3, 1, None)), "pangolin  limit 3  runs 1");
/// let m = Machine { cores: 16, mhz: 4500, mem_gb: 32, load: 1.5 };
/// assert_eq!(
///     plain(riff::view::host_heading("pangolin", 3, 1, Some(&m))),
///     "pangolin  limit 3  runs 1  cpu 16x4500MHz, mem 32GB, load 1.50  score 24.0"
/// );
/// ```
pub fn host_heading(
    host: &str,
    limit: u16,
    runs: usize,
    machine: Option<&crate::machine::Machine>,
) -> String {
    let mut out = format!("{}  limit {limit}  runs {runs}", styled(BOLD, &safe(host)));
    if let Some(m) = machine {
        let _ = write!(
            out,
            "  {}",
            styled(DIM, &format!("{m}  score {:.1}", m.score()))
        );
    }
    out
}

/// The table of `riff workers` for the worker panes of one machine
/// (01M3JPQTBDGT54WN7FZP9CD6B5): PANE, ID (8 characters; the full
/// session ID with `long`), STATE and DETAIL from `sessions`. A
/// worker that is not in `sessions` has the state `not in riff who`.
/// No pane gives no table.
///
/// ```
/// use riff::terminal::WorkerPane;
/// use riff_core::wire::{SessionInfo, SessionState};
///
/// let panes = [
///     WorkerPane { pane: "%3".into(), session: "a6cf2205-1".into() },
///     WorkerPane { pane: "%4".into(), session: "77e0aaaa-2".into() },
/// ];
/// let info = SessionInfo {
///     uri: "riff://mike@pangolin/como-technologies/riff?session=a6cf2205-1&claim=issue-12#issue-12".parse()?,
///     live: true,
///     idle_secs: 0,
///     status: None,
///     worker: true,
///     stopping: false,
///     claims_secs: 0,
///     state: SessionState::Busy,
/// };
/// let out = riff::view::workers(&panes, &[info.clone()], false);
/// let plain = anstream::adapter::strip_str(&out).to_string();
/// assert_eq!(
///     plain,
///     "PANE  ID        STATE            DETAIL\n\
///      %3    a6cf2205  busy             working on #12\n\
///      %4    77e0aaaa  not in riff who\n"
/// );
/// let long = riff::view::workers(&panes, &[info], true);
/// assert!(anstream::adapter::strip_str(&long).to_string().contains("\n%3    a6cf2205-1  busy"));
/// assert_eq!(riff::view::workers(&[], &[], false), "");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn workers(
    panes: &[crate::terminal::WorkerPane],
    sessions: &[SessionInfo],
    long: bool,
) -> String {
    if panes.is_empty() {
        return String::new();
    }
    let rows: Vec<Vec<String>> = panes
        .iter()
        .map(|w| {
            let id = if long {
                safe(&w.session)
            } else {
                safe(&w.session.chars().take(8).collect::<String>())
            };
            let info = sessions
                .iter()
                .find(|s| s.uri.who().session() == Some(w.session.as_str()));
            let mut row = vec![safe(&w.pane), id];
            match info {
                None => row.push(styled(WARNING, "not in riff who")),
                Some(info) => {
                    row.push(state_cell(info));
                    row.push(detail_cell(info));
                }
            }
            row
        })
        .collect();
    table(&["PANE", "ID", "STATE", "DETAIL"], &rows)
}

/// `riff members`: the owner, the admins, the members and the allowed
/// domains. A riff with no owner ends with the action
/// (01M3Q63NNC6SC03BFCG80M7B4D).
///
/// ```
/// use riff_core::wire::MembersReply;
///
/// let reply = MembersReply {
///     owner: Some("ada@gmail.com".into()),
///     admins: vec![],
///     members: vec!["bob@gmail.com".into()],
///     allowed_domains: vec!["x.io".into()],
/// };
/// assert_eq!(
///     anstream::adapter::strip_str(&riff::view::members(&reply)).to_string(),
///     "owner            ada@gmail.com\nadmins           none\nmembers          bob@gmail.com\n\
///      allowed domains  x.io\n"
/// );
/// let none = MembersReply { owner: None, ..reply };
/// let plain = anstream::adapter::strip_str(&riff::view::members(&none)).to_string();
/// assert!(plain.starts_with("owner            none\n"), "{plain}");
/// assert!(plain.ends_with("\nThe riff has no owner. An admin takes the owner role with: riff owner --take\n"));
/// ```
pub fn members(reply: &MembersReply) -> String {
    let list = |items: &[String]| {
        if items.is_empty() {
            "none".to_owned()
        } else {
            items.iter().map(|i| safe(i)).collect::<Vec<_>>().join(", ")
        }
    };
    let mut out = facts(&[
        ("owner", safe(reply.owner.as_deref().unwrap_or("none"))),
        ("admins", list(&reply.admins)),
        ("members", list(&reply.members)),
        ("allowed domains", list(&reply.allowed_domains)),
    ]);
    if reply.owner.is_none() {
        let _ = writeln!(out, "{}", styled(WARNING, text::NO_OWNER));
    }
    out
}
