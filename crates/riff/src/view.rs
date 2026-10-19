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

use riff_core::build::{Build, Mismatch};
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{MembersReply, PauseInfo, RiffOwner, RiffReply, SessionInfo};

use crate::style::{BOLD, DIM, ERROR, GOOD, WARNING, styled};
use crate::text::{self, safe};

/// The line of [`Reconnect`] while a connect fails.
pub const RECONNECTING: &str = "(reconnecting…)";

/// The line of [`Reconnect`] when a stream is back after a failed connect.
pub const BACK: &str = "(back)";

/// What a person sees of a stream that
/// [`follow`](riff_client::api::follow) follows (01M3NK7VHXB0PAR8VH8GQQA06K). A server that ends a long poll is
/// normal, so a reconnect shows nothing. Only a failed connect shows one
/// short dim line, [`RECONNECTING`], and the next item [`BACK`]. A server
/// that riff cannot talk to
/// ([`Mismatch`]) shows its error in red.
///
/// ```
/// use riff::view::{BACK, RECONNECTING, Reconnect};
///
/// let plain = |line: Option<String>| line.map(|l| anstream::adapter::strip_str(&l).to_string());
/// let mut link = Reconnect::default();
/// assert_eq!(link.line(&anyhow::Ok(1)), None);
/// let cut: anyhow::Result<u32> = Err(anyhow::anyhow!("error decoding response body"));
/// assert_eq!(plain(link.line(&cut)).as_deref(), Some(RECONNECTING));
/// assert_eq!(link.line(&cut), None);
/// assert_eq!(plain(link.line(&anyhow::Ok(2))).as_deref(), Some(BACK));
/// assert_eq!(link.line(&anyhow::Ok(3)), None);
/// ```
#[derive(Debug, Default)]
pub struct Reconnect {
    lost: bool,
}

impl Reconnect {
    /// The line to show before `item`, with its style, if any.
    pub fn line<T>(&mut self, item: &anyhow::Result<T>) -> Option<String> {
        match (item, self.lost) {
            (Ok(_), true) => {
                self.lost = false;
                Some(styled(DIM, BACK))
            }
            (Ok(_), false) | (Err(_), true) => None,
            (Err(e), false) => {
                self.lost = true;
                Some(if e.downcast_ref::<Mismatch>().is_some() {
                    styled(ERROR, &format!("riff: {e:#}"))
                } else {
                    styled(DIM, RECONNECTING)
                })
            }
        }
    }
}

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

/// `riff watch limit` (01M3Z64J08GW6N1H42AR2FZQZ4).
///
/// ```
/// let on = riff::view::watch_limit(6000, "/h/config.toml".as_ref());
/// let plain = anstream::adapter::strip_str(&on).to_string();
/// assert_eq!(
///     plain,
///     "watch.limit  6000  (/h/config.toml)\n\
///      riff watch --once ends after 6000 seconds with no wake. \
///      Set it with: riff watch limit SECONDS"
/// );
/// let off = riff::view::watch_limit(0, "/h/config.toml".as_ref());
/// assert!(off.contains("riff watch --once waits with no limit."), "{off}");
/// ```
pub fn watch_limit(secs: u64, path: &Path) -> String {
    let hint = if secs == 0 {
        "riff watch --once waits with no limit. Set a limit with: riff watch limit SECONDS"
            .to_owned()
    } else {
        format!(
            "riff watch --once ends after {secs} seconds with no wake. \
             Set it with: riff watch limit SECONDS"
        )
    };
    setting("watch.limit", &secs.to_string(), path, &hint)
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

/// Who set a pause, for example ` by the person mike`. Empty when it is
/// not known.
fn pause_by(pause: &PauseInfo) -> String {
    pause
        .by
        .as_ref()
        .map_or_else(String::new, |by| format!(" by {}", safe(&by.to_string())))
}

/// The facts of the pauses (01M3XAHZJAF6YVDJ7WX74X8RBX): the state of
/// the whole riff, and one fact for each repository that is paused,
/// each with who set the pause. Then the action for the pause that
/// stops a session in the repository `here`.
pub(crate) fn state_facts(
    pauses: &RiffReply,
    here: Option<&ThreadName>,
) -> (Vec<(&'static str, String)>, Option<String>) {
    let mut rows = Vec::new();
    let mut action = None;
    match &pauses.riff {
        None => rows.push(("riff", styled(GOOD, "running"))),
        Some(pause) => {
            let fact = format!("paused{}", pause_by(pause));
            rows.push(("riff", styled(WARNING, &fact)));
            action = Some(
                "The riff is paused. Nobody claims work. The owner or an admin resumes it \
                 with: riff resume --riff"
                    .to_owned(),
            );
        }
    }
    for paused in &pauses.repositories {
        let fact = format!("{}{}", paused.repository, pause_by(&paused.pause));
        rows.push(("paused", styled(WARNING, &safe(&fact))));
        if action.is_none() && Some(&paused.repository) == here {
            action = Some(format!(
                "The repository {} is paused. Nobody claims work there. Your user or the lead \
                 resumes it with: riff resume",
                paused.repository
            ));
        }
    }
    (rows, action.map(|action| styled(WARNING, &action)))
}

/// `riff whoami`: the session, its URI, the pauses of the riff and the
/// build. `state` is the error text when riff cannot read the pauses.
///
/// ```
/// use riff_core::record::By;
/// use riff_core::wire::{PauseInfo, RepositoryPause, RiffReply, RiffState};
///
/// let me = "riff://mike@pangolin/como-technologies/riff?session=a6cf2205-1".parse()?;
/// let out = riff::view::whoami(&me, Ok(RiffState::Running.into()));
/// let plain = anstream::adapter::strip_str(&out).to_string();
/// let lines: Vec<&str> = plain.lines().collect();
/// assert_eq!(lines[0], "session  mike@pangolin:riff (a6cf2205)");
/// assert_eq!(lines[1], "uri      riff://mike@pangolin/como-technologies/riff?session=a6cf2205-1");
/// assert_eq!(lines[2], "riff     running");
/// assert!(lines[3].starts_with("build    v"), "{plain}");
///
/// let paused = riff::view::whoami(&me, Ok(RiffState::Paused.into()));
/// let plain = anstream::adapter::strip_str(&paused).to_string();
/// assert!(plain.ends_with("resumes it with: riff resume --riff\n"), "{plain}");
///
/// // The pause of the repository of the session, and who set it.
/// let pause = RepositoryPause {
///     repository: "como-technologies/riff".parse()?,
///     pause: PauseInfo { by: Some(By::Person("mike".into())), at_ms: 7 },
/// };
/// let pauses = RiffReply {
///     state: RiffState::Paused,
///     repositories: vec![pause],
///     ..RiffState::Running.into()
/// };
/// let plain = anstream::adapter::strip_str(&riff::view::whoami(&me, Ok(pauses))).to_string();
/// let lines: Vec<&str> = plain.lines().collect();
/// assert_eq!(lines[2], "riff     running");
/// assert_eq!(lines[3], "paused   como-technologies/riff by the person mike");
/// assert!(plain.ends_with("lead resumes it with: riff resume\n"), "{plain}");
/// let down = riff::view::whoami(&me, Err("refused".into()));
/// assert!(anstream::adapter::strip_str(&down).to_string().contains("riff     unknown: refused\n"));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn whoami(me: &SessionUri, state: Result<RiffReply, String>) -> String {
    let mut rows = vec![
        (
            "session",
            styled(crate::style::session(me), &safe(&text::name(me))),
        ),
        ("uri", safe(&me.to_string())),
    ];
    let mut action = None;
    match state {
        Ok(pauses) => {
            let (facts, act) = state_facts(&pauses, me.default_thread().as_ref());
            rows.extend(facts);
            action = act;
            rows.extend(build_facts(riff_client::link::server_build().as_ref()));
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
/// use riff_core::wire::{BlockedInfo, RiffOwner, RiffState, SessionInfo, SessionState};
///
/// let me = "riff://mike@pangolin/como-technologies/riff?session=a6cf&lead=true&claim=issue-6#issue-6"
///     .parse()?;
/// let brett = "riff://brett@heron/como-technologies/riff?session=77e0".parse()?;
/// let blocked = BlockedInfo {
///     reason: "which design?".into(),
///     secs: 60,
///     answered: false,
///     woken_again: false,
///     unanswered: false,
/// };
/// let list = [
///     SessionInfo {
///         uri: me,
///         live: true,
///         idle_secs: 0,
///         status: None,
///         worker: false,
///         container: false,
///         binary: None,
///         stopping: false,
///         claims_secs: 0,
///         must_clear: false,
///         fresh_secs: None,
///         state: Some(SessionState::Busy),
///         work: None,
///         waits: None,
///         blocked: None,
///         step: None,
///         machine: None,
///     },
///     SessionInfo {
///         uri: brett,
///         live: true,
///         idle_secs: 0,
///         status: None,
///         worker: false,
///         container: false,
///         binary: None,
///         stopping: false,
///         claims_secs: 0,
///         must_clear: false,
///         fresh_secs: None,
///         state: Some(SessionState::Blocked),
///         work: None,
///         waits: None,
///         blocked: Some(blocked),
///         step: None,
///         machine: None,
///     },
/// ];
/// let owner = RiffOwner::Owner { user: "mike".into(), email: "mike@x.io".into() };
/// let running = RiffState::Running.into();
/// let text = riff::view::who(&running, &owner, &list, &list[0].uri, false);
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
///     "brett@heron:riff (77e0)            blocked            which design? (1m ago)"
/// );
/// let red = riff::style::ERROR;
/// assert!(text.contains(&format!("{red}blocked{red:#}")));
/// assert!(text.contains(&format!("{red}which design? (1m ago){red:#}")));
///
/// // --long shows the URI in place of the name.
/// let long = riff::view::who(&running, &owner, &list, &list[0].uri, true);
/// let plain = anstream::adapter::strip_str(&long).to_string();
/// assert!(plain.contains("\nURI "), "{plain}");
/// assert!(plain.contains("\nriff://brett@heron/como-technologies/riff?session=77e0 "), "{plain}");
///
/// // A riff with no sign-in shows no owner.
/// let text = riff::view::who(&running, &RiffOwner::NoSignIn, &list, &list[0].uri, false);
/// assert!(!anstream::adapter::strip_str(&text).to_string().contains("owner"));
///
/// // A riff with no owner ends with the action.
/// let text = riff::view::who(&running, &RiffOwner::Nobody, &list, &list[0].uri, false);
/// let plain = anstream::adapter::strip_str(&text).to_string();
/// assert!(plain.contains("\nowner  none\n"), "{plain}");
/// assert!(plain.ends_with("riff owner --take\n"), "{plain}");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn who(
    pauses: &RiffReply,
    owner: &RiffOwner,
    sessions: &[SessionInfo],
    me: &SessionUri,
    long: bool,
) -> String {
    let (mut rows, paused) = state_facts(pauses, me.default_thread().as_ref());
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
    rows.extend(build_facts(riff_client::link::server_build().as_ref()));
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
    out.push_str(&hosts(sessions));
    for action in actions {
        let _ = writeln!(out, "{action}");
    }
    out
}

/// The part `Hosts` of `riff who`: the facts of the daemon of each host
/// in `sessions` (01M57D3F7FXTPZB3M72Z1DQ1T7), or nothing with no
/// facts.
///
/// ```
/// use riff_client::daemons::Facts;
/// use riff_core::wire::{MachineInfo, SessionInfo};
///
/// let facts = Facts { build: "3.1.0".into(), limit: 4, floor_gb: 8, ..Facts::default() };
/// let mut s: SessionInfo = serde_json::from_value(serde_json::json!({
///     "uri": "riff://mike@thelio/o/r?session=d1", "live": true,
/// }))?;
/// s.machine = Some(MachineInfo { numbers: facts.line(), age_secs: 12 });
/// let text = riff::view::hosts(&[s]);
/// let plain = anstream::adapter::strip_str(&text).to_string();
/// assert!(plain.starts_with("\nHosts:\nmike@thelio (12s ago)\n  daemon 3.1.0  sessions 0/4  floor 8GB\n"), "{plain}");
/// assert_eq!(riff::view::hosts(&[]), "");
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn hosts(sessions: &[SessionInfo]) -> String {
    let found = riff_client::daemons::hosts(sessions);
    if found.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nHosts:\n");
    for host in found {
        let _ = writeln!(
            out,
            "{}@{} ({} ago)",
            styled(BOLD, &safe(&host.user)),
            styled(BOLD, &safe(&host.host)),
            text::ago(host.age_secs)
        );
        for parts in text::daemon_facts(&host.facts) {
            let line: Vec<String> = parts.iter().map(|(t, style)| styled(*style, t)).collect();
            let _ = writeln!(out, "  {}", line.join("  "));
        }
    }
    out
}

/// The word of the state of `s`, in its color.
fn state_cell(s: &SessionInfo) -> String {
    let state = crate::state::of(s);
    styled(crate::state::style(state), state.word())
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
///         status: Status { step: "tests".into() },
///         age_secs: 7200,
///         stale: false,
///     }),
///     worker: true,
///     container: false,
///     binary: None,
///     stopping: false,
///     claims_secs: 300,
///     must_clear: false,
///     fresh_secs: None,
///     state: Some(SessionState::Busy),
///     work: None,
///     waits: None,
///     blocked: None,
///     step: None,
///     machine: None,
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

/// `riff members` (riff-server 1.x): the owner, the admins, the
/// members and the allowed domains. A riff with no owner ends with the
/// action (01M3Q63NNC6SC03BFCG80M7B4D).
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
