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
//! assert_eq!(facts(&[("riff", "running".into()), ("admins", "ada".into())]),
//!            "riff    running\nadmins  ada\n");
//! let rows = [vec!["%3".into(), "live".into()], vec!["%10".into(), "idle 2m".into()]];
//! let plain = anstream::adapter::strip_str(&table(&["PANE", "STATE"], &rows)).to_string();
//! assert_eq!(plain, "PANE  STATE\n%3    live\n%10   idle 2m\n");
//! ```

use std::fmt::Write;
use std::path::Path;

use riff_client::sessions::NO_PROJECT;
use riff_core::build::{Build, Mismatch};
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{MembersReply, PauseInfo, RiffReply, SessionInfo};

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

/// True when the wave of the repository `repo` (`OWNER/REPO`) waits for a
/// lead (01M776RSYKJK9NETFAD8G0CCVQ): riff-server names it with no live
/// lead in `pauses`, and the daemon of a host in the facts of `sessions`
/// runs its loop ([`riff_client::daemons::hosts`]). `riff top` and the TUI
/// show it.
///
/// ```
/// use riff_client::daemons::{Facts, Wave};
/// use riff_core::wire::{MachineInfo, RiffReply, RiffState, SessionInfo};
///
/// let facts = Facts {
///     waves: vec![Wave { repo: "o/r".into(), word: String::new(), image: None, waits: Default::default() }],
///     ..Facts::default()
/// };
/// let mut daemon: SessionInfo = serde_json::from_value(serde_json::json!({
///     "uri": "riff://mike@thelio/o/r?session=d1", "live": true,
/// })).unwrap();
/// daemon.machine = Some(MachineInfo { numbers: facts.line(), age_secs: 5 });
/// let pauses = RiffReply { no_lead: vec!["o/r".parse()?, "o/s".parse()?], ..RiffState::Running.into() };
/// assert!(riff::view::waits_for_lead(&pauses, [&daemon], "o/r"));
/// // No host runs the wave of o/s.
/// assert!(!riff::view::waits_for_lead(&pauses, [&daemon], "o/s"));
/// assert!(!riff::view::waits_for_lead(&RiffState::Running.into(), [&daemon], "o/r"));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn waits_for_lead<'a>(
    pauses: &RiffReply,
    sessions: impl IntoIterator<Item = &'a SessionInfo>,
    repo: &str,
) -> bool {
    repo.parse::<ThreadName>()
        .is_ok_and(|thread| pauses.waits_for_lead(&thread))
        && riff_client::daemons::hosts(sessions)
            .iter()
            .any(|host| host.facts.waves.iter().any(|wave| wave.repo == repo))
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
                "The riff is paused. Nobody claims work. An admin resumes it with: riff \
                 resume --riff"
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

/// `riff whoami`: the session, its URI, the pauses of the riff, the
/// build, the policy of the proof of presence (01M75WKSYCMMJFGAM3W4ASGN85), and in a
/// container how the session acts on GitHub
/// (01M6WPMWYSW98MQYQCCJ54ZX04). `state` is the error text when riff
/// cannot read the pauses.
///
/// ```
/// use riff_core::record::By;
/// use riff_core::wire::{GitHubRole, PauseInfo, RepositoryPause, RiffReply, RiffState};
/// use riff::daemon::gh_token::ActsAs;
///
/// let me = "riff://mike@pangolin/como-technologies/riff?session=a6cf2205-1".parse()?;
/// let out = riff::view::whoami(&me, Ok(RiffState::Running.into()), None);
/// let plain = anstream::adapter::strip_str(&out).to_string();
/// let lines: Vec<&str> = plain.lines().collect();
/// assert_eq!(lines[0], "session  mike@pangolin:riff (a6cf2205)");
/// assert_eq!(lines[1], "uri      riff://mike@pangolin/como-technologies/riff?session=a6cf2205-1");
/// assert_eq!(lines[2], "riff     running");
/// assert!(lines[3].starts_with("build    v"), "{plain}");
/// assert_eq!(lines[4], "proof    off");
/// assert!(!plain.contains("github"), "{plain}");
///
/// // In a container: the bot, the role, the rights and the end of the token.
/// let bot = ActsAs::Bot {
///     bot: "riff-acme[bot]".into(),
///     role: GitHubRole::Worker,
///     rights: [("contents".to_owned(), "write".to_owned()), ("metadata".to_owned(), "read".to_owned())].into(),
///     ends: "2026-11-08T13:00:00Z".into(),
///     installation: Some(1010),
/// };
/// let out = riff::view::whoami(&me, Ok(RiffState::Running.into()), Some(&bot));
/// let plain = anstream::adapter::strip_str(&out).to_string();
/// assert!(plain.contains("github   riff-acme[bot], a worker, until 2026-11-08T13:00:00Z\n"), "{plain}");
/// assert!(plain.contains("rights   contents: write, metadata: read\n"), "{plain}");
/// // With no token of an App, never the token of the person
/// // (01M6Z87A04EXTT1FFJSJ1EM6TB).
/// let none = ActsAs::NoToken { why: "riff-acme-lead is not installed on acme/app: riff github install acme".into() };
/// let out = riff::view::whoami(&me, Ok(RiffState::Running.into()), Some(&none));
/// let plain = anstream::adapter::strip_str(&out).to_string();
/// assert!(plain.contains(
///     "github   no token: riff-acme-lead is not installed on acme/app: riff github install acme\n"
/// ), "{plain}");
/// // A session of riff run --no-github (01M6Z87JY83V3KR72E2AG8V8KG).
/// let out = riff::view::whoami(&me, Ok(RiffState::Running.into()), Some(&ActsAs::NoGitHub));
/// let plain = anstream::adapter::strip_str(&out).to_string();
/// assert!(plain.contains(&format!("github   none: {}\n", riff::text::GH_RUN_NO_GITHUB)), "{plain}");
/// let none = ActsAs::NoToken { why: "riff-server cannot read the keys of the GitHub Apps".into() };
/// let out = riff::view::whoami(&me, Ok(RiffState::Paused.into()), Some(&none));
/// let plain = anstream::adapter::strip_str(&out).to_string();
/// assert!(plain.contains(
///     "github   no token: riff-server cannot read the keys of the GitHub Apps\n"
/// ), "{plain}");
///
/// let paused = riff::view::whoami(&me, Ok(RiffState::Paused.into()), None);
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
/// let plain = anstream::adapter::strip_str(&riff::view::whoami(&me, Ok(pauses), None)).to_string();
/// let lines: Vec<&str> = plain.lines().collect();
/// assert_eq!(lines[2], "riff     running");
/// assert_eq!(lines[3], "paused   como-technologies/riff by the person mike");
/// assert!(plain.ends_with("lead resumes it with: riff resume\n"), "{plain}");
/// let down = riff::view::whoami(&me, Err("refused".into()), None);
/// assert!(anstream::adapter::strip_str(&down).to_string().contains("riff     unknown: refused\n"));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn whoami(
    me: &SessionUri,
    state: Result<RiffReply, String>,
    github: Option<&crate::daemon::gh_token::ActsAs>,
) -> String {
    use crate::daemon::gh_token::ActsAs;
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
            // The policy of the proof of presence (01M75WKSYCMMJFGAM3W4ASGN85).
            rows.push(("proof", pauses.proof.word().to_owned()));
        }
        Err(e) => rows.push(("riff", styled(ERROR, &format!("unknown: {}", safe(&e))))),
    }
    match github {
        Some(ActsAs::Bot {
            bot,
            role,
            rights,
            ends,
            ..
        }) => {
            let role = match role {
                riff_core::wire::GitHubRole::Lead => "the lead".to_owned(),
                role => format!("a {role}"),
            };
            rows.push(("github", safe(&format!("{bot}, {role}, until {ends}"))));
            let rights: Vec<String> = rights
                .iter()
                .map(|(name, level)| format!("{name}: {level}"))
                .collect();
            rows.push(("rights", safe(&rights.join(", "))));
        }
        Some(ActsAs::NoGitHub) => {
            rows.push(("github", safe(&format!("none: {}", text::GH_RUN_NO_GITHUB))))
        }
        Some(ActsAs::NoToken { why }) => {
            rows.push(("github", styled(ERROR, &safe(&format!("no token: {why}")))))
        }
        None => {}
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
///   yellow), and the build. No fact names an owner
///   (01M7767T8EFVR79NSHKTK53G7B).
/// - The columns: SESSION (the [`text::name`] in the color of the
///   session, as in `riff tail`), STATE (the word of the state that the
///   server derives, in its color: see [`crate::state`]), ROLE (`you`
///   in bold and the [`text::tags`]) and DETAIL (the
///   [`crate::state::detail`]). With `long`, the column URI takes the
///   place of SESSION.
/// - The rows group by project (01M77F8JN1SE2H4DZBZH1YX7WD): a bold line
///   with the repository of each project of `projects` with a session
///   comes before the rows of its sessions, in the order of
///   `OWNER/REPO`. The rows of the sessions in no project come last,
///   after the dim line [`NO_PROJECT`]. In a group, the rows keep the
///   order of `sessions`.
/// - The actions come last: resume a paused riff.
///
/// Each text from the server is [`safe`].
///
/// ```
/// use riff_core::wire::{BlockedInfo, RiffState, SessionInfo, SessionState};
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
/// let running = RiffState::Running.into();
/// let projects = ["como-technologies/riff".parse()?];
/// let text = riff::view::who(&running, &list, &projects, &list[0].uri, false);
/// let plain = anstream::adapter::strip_str(&text).to_string();
/// let lines: Vec<&str> = plain.lines().collect();
/// assert_eq!(lines[0], "riff   running");
/// assert!(lines[1].starts_with("build  v"));
/// assert_eq!(lines[2], "");
/// assert_eq!(lines[3], "SESSION                            STATE    ROLE      DETAIL");
/// assert_eq!(lines[4], "como-technologies/riff");
/// assert_eq!(lines[5], "mike@pangolin:riff#issue-6 (a6cf)  busy     you lead  working on #6");
/// assert_eq!(
///     lines[6],
///     "brett@heron:riff (77e0)            blocked            which design? (1m ago)"
/// );
/// assert!(!plain.contains("owner"), "{plain}");
/// let red = riff::style::ERROR;
/// assert!(text.contains(&format!("{red}blocked{red:#}")));
/// assert!(text.contains(&format!("{red}which design? (1m ago){red:#}")));
///
/// // --long shows the URI in place of the name.
/// let long = riff::view::who(&running, &list, &projects, &list[0].uri, true);
/// let plain = anstream::adapter::strip_str(&long).to_string();
/// assert!(plain.contains("\nURI "), "{plain}");
/// assert!(plain.contains("\nriff://brett@heron/como-technologies/riff?session=77e0 "), "{plain}");
///
/// // With no project, each row is in the group `no project`.
/// let text = riff::view::who(&running, &list, &[], &list[0].uri, false);
/// let plain = anstream::adapter::strip_str(&text).to_string();
/// assert!(plain.contains("DETAIL\nno project\nmike@pangolin:riff#issue-6 (a6cf) "), "{plain}");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn who(
    pauses: &RiffReply,
    sessions: &[SessionInfo],
    projects: &[ThreadName],
    me: &SessionUri,
    long: bool,
) -> String {
    let (mut rows, paused) = state_facts(pauses, me.default_thread().as_ref());
    if let Some(line) = release(pauses, sessions) {
        rows.insert(1, ("release", styled(WARNING, &line)));
    }
    let actions: Vec<String> = paused.into_iter().collect();
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
        let groups = by_project(sessions, projects);
        let rows: Vec<Vec<String>> = groups
            .iter()
            .flat_map(|(_, list)| list.iter())
            .map(|s| {
                let mut role: Vec<String> = Vec::new();
                if s.uri.who() == me.who() {
                    role.push(styled(BOLD, "you"));
                }
                role.extend(text::tags(s).into_iter().map(String::from));
                let name = if long {
                    styled(DIM, &safe(&s.uri.to_string()))
                } else {
                    styled(crate::style::session(&s.uri), &safe(&text::name(&s.uri)))
                };
                vec![name, state_cell(s), role.join(" "), detail_cell(s)]
            })
            .collect();
        // One table for all the rows, so each group has the same columns.
        let table = table(&head, &rows);
        let mut lines = table.lines();
        if let Some(line) = lines.next() {
            let _ = writeln!(out, "{line}");
        }
        for (project, list) in &groups {
            let heading = match project {
                Some(repo) => styled(BOLD, &safe(repo)),
                None => styled(DIM, NO_PROJECT),
            };
            let _ = writeln!(out, "{heading}");
            for line in lines.by_ref().take(list.len()) {
                let _ = writeln!(out, "{line}");
            }
        }
    }
    out.push_str(&hosts(sessions, long));
    for action in actions {
        let _ = writeln!(out, "{action}");
    }
    out
}

/// The sessions of `sessions` by project (01M77F8JN1SE2H4DZBZH1YX7WD):
/// each project of `projects` with a session, in the order of
/// `OWNER/REPO`, then `None` with the sessions in no project. In a group,
/// the sessions keep their order.
fn by_project<'a>(
    sessions: &'a [SessionInfo],
    projects: &[ThreadName],
) -> Vec<(Option<String>, Vec<&'a SessionInfo>)> {
    let mut groups: std::collections::BTreeMap<String, Vec<&SessionInfo>> = Default::default();
    let mut none = Vec::new();
    for s in sessions {
        match riff_client::sessions::project(s, projects) {
            Some(repo) => groups.entry(repo).or_default().push(s),
            None => none.push(s),
        }
    }
    let mut out: Vec<(Option<String>, Vec<&SessionInfo>)> = groups
        .into_iter()
        .map(|(repo, list)| (Some(repo), list))
        .collect();
    if !none.is_empty() {
        out.push((None, none));
    }
    out
}

/// The line of the open release of the riff (01M6XTS1GCW7PTWSMM1HKQTHSN), the
/// same in the row `release` of `riff who` and in the header of the TUI: the
/// release that the watches of the hosts in `sessions` follow, from the cut
/// to the check ([`riff_client::daemons::release`],
/// 01M75R6FPVZA3JA5C48PQ26A85), with the pause of the whole riff in
/// `pauses` ([`text::release_line`]). With no line of an open release, the
/// line of a wave with no open item whose release waits for its cut
/// ([`riff_client::daemons::complete`], [`text::complete_line`],
/// 01M75N6YNHQQHGS0VGKTKSNJTR). `None` with neither.
///
/// ```
/// use riff_client::daemons::{Facts, Release, ReleaseStep};
/// use riff_core::wire::{MachineInfo, RiffState, SessionInfo};
///
/// let daemon = |step| {
///     let facts = Facts {
///         releases: vec![Release { repo: "o/r".into(), tag: "v3.3.0".into(), step }],
///         ..Facts::default()
///     };
///     let mut s: SessionInfo = serde_json::from_value(serde_json::json!({
///         "uri": "riff://mike@thelio/o/r?session=d1", "live": true,
///     }))
///     .unwrap();
///     s.machine = Some(MachineInfo { numbers: facts.line(), age_secs: 12 });
///     s
/// };
/// let (running, paused) = (RiffState::Running.into(), RiffState::Paused.into());
/// assert_eq!(riff::view::release(&running, &[]), None);
/// assert_eq!(
///     riff::view::release(&paused, &[daemon(ReleaseStep::Runs)]).unwrap(),
///     "v3.3.0: the deploy runs; the riff is paused"
/// );
/// assert_eq!(riff::view::release(&running, &[daemon(ReleaseStep::Passed)]), None);
/// // From the cut (01M75R6FPVZA3JA5C48PQ26A85).
/// assert_eq!(
///     riff::view::release(&running, &[daemon(ReleaseStep::TestRuns)]).unwrap(),
///     "v3.3.0: the real-session test runs"
/// );
///
/// // A wave with no open item, whose release waits for its cut: its line,
/// // while no open release has a line.
/// use riff_client::daemons::{Complete, CutWaits};
/// let mut done: SessionInfo = serde_json::from_value(serde_json::json!({
///     "uri": "riff://mike@pangolin/o/r?session=d2", "live": true,
/// }))
/// .unwrap();
/// let facts = Facts {
///     complete: vec![Complete { repo: "o/r".into(), wave: 41, waits: Some(CutWaits::MainFailed) }],
///     ..Facts::default()
/// };
/// done.machine = Some(MachineInfo { numbers: facts.line(), age_secs: 12 });
/// assert_eq!(
///     riff::view::release(&running, &[done.clone()]).unwrap(),
///     "Wave 41 is complete: the release waits for its cut; the full check of main failed"
/// );
/// assert_eq!(
///     riff::view::release(&running, &[done, daemon(ReleaseStep::Waits)]).unwrap(),
///     "v3.3.0: the deploy waits for a person (riff release approve v3.3.0)"
/// );
///
/// // `riff who` shows the line in the row `release`, under the state of the riff.
/// let me = "riff://mike@thelio/o/r".parse()?;
/// let text = riff::view::who(&paused, &[daemon(ReleaseStep::Passed)], &[], &me, false);
/// let plain = anstream::adapter::strip_str(&text).to_string();
/// assert!(
///     plain.starts_with(
///         "riff     paused\nrelease  v3.3.0: the deploy passed; it waits for the check of the lead of o/r\n"
///     ),
///     "{plain}"
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn release(pauses: &RiffReply, sessions: &[SessionInfo]) -> Option<String> {
    let hosts = riff_client::daemons::hosts(sessions);
    riff_client::daemons::release(&hosts)
        .and_then(|release| text::release_line(&release, pauses.riff.is_some()))
        .or_else(|| riff_client::daemons::complete(&hosts).map(|c| text::complete_line(&c)))
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
/// let text = riff::view::hosts(&[s], false);
/// let plain = anstream::adapter::strip_str(&text).to_string();
/// assert!(plain.starts_with("\nHosts:\nmike@thelio (12s ago)\n  daemon 3.1.0  sessions 0/4  floor 8GB\n"), "{plain}");
/// assert_eq!(riff::view::hosts(&[], false), "");
///
/// // --long adds the digest of the pin of each wave and of each session.
/// use riff_client::daemons::{Image, Session, Wave};
/// use riff_client::sessions::Phase;
/// let facts = Facts {
///     build: "3.1.0".into(),
///     limit: 4,
///     floor_gb: 8,
///     sessions: vec![Session {
///         name: "riff-issue-12-1a2b3c4d".into(),
///         item: "issue-12".into(),
///         phase: Phase::Works,
///         last: String::new(),
///         digest: "1a2b3c4d5e6f".into(),
///     }],
///     waves: vec![Wave {
///         repo: "o/r".into(),
///         word: "started issue-12".into(),
///         image: Some(Image { digest: "1a2b3c4d5e6f".into(), here: true }),
///         waits: Default::default(),
///     }],
///     ..Facts::default()
/// };
/// let mut s: SessionInfo = serde_json::from_value(serde_json::json!({
///     "uri": "riff://mike@thelio/o/r?session=d1", "live": true,
/// }))?;
/// s.machine = Some(MachineInfo { numbers: facts.line(), age_secs: 12 });
/// let long = anstream::adapter::strip_str(&riff::view::hosts(&[s.clone()], true)).to_string();
/// assert!(long.contains("\n  wave o/r: started issue-12  image 1a2b3c4d5e6f here\n"), "{long}");
/// assert!(long.contains("\n  riff-issue-12-1a2b3c4d  issue-12  works  image 1a2b3c4d5e6f\n"), "{long}");
/// let short = anstream::adapter::strip_str(&riff::view::hosts(&[s], false)).to_string();
/// assert!(short.contains("\n  wave o/r: started issue-12  image here\n"), "{short}");
/// assert!(!short.contains("1a2b3c4d5e6f"), "{short}");
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn hosts(sessions: &[SessionInfo], long: bool) -> String {
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
        for parts in text::daemon_facts(&host.facts, long) {
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

/// `riff members`: the admins, the members, the allowed domains and the
/// policy of the proof of presence (01M75WKSYCMMJFGAM3W4ASGN85). It
/// names no owner (01M7767T8EFVR79NSHKTK53G7B).
///
/// ```
/// use riff_core::proof::Policy;
/// use riff_core::wire::MembersReply;
///
/// let reply = MembersReply {
///     admins: vec!["ada@gmail.com".into()],
///     members: vec!["bob@gmail.com".into()],
///     allowed_domains: vec!["x.io".into()],
///     proof: Policy::Required,
/// };
/// assert_eq!(
///     anstream::adapter::strip_str(&riff::view::members(&reply)).to_string(),
///     "admins           ada@gmail.com\nmembers          bob@gmail.com\n\
///      allowed domains  x.io\nproof            required\n"
/// );
/// let none = MembersReply { admins: vec![], ..reply };
/// let plain = anstream::adapter::strip_str(&riff::view::members(&none)).to_string();
/// assert!(plain.starts_with("admins           none\n"), "{plain}");
/// ```
pub fn members(reply: &MembersReply) -> String {
    let list = |items: &[String]| {
        if items.is_empty() {
            "none".to_owned()
        } else {
            items.iter().map(|i| safe(i)).collect::<Vec<_>>().join(", ")
        }
    };
    facts(&[
        ("admins", list(&reply.admins)),
        ("members", list(&reply.members)),
        ("allowed domains", list(&reply.allowed_domains)),
        ("proof", reply.proof.word().to_owned()),
    ])
}
