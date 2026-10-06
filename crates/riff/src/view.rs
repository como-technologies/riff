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
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{MembersReply, PauseInfo, RiffOwner, RiffReply, SessionInfo};

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

/// `riff lead blocked` (01M41FZQ545HQ9Q75CSKX8HF8H,
/// 01M41FZQKZKW131Z8822G31T5G).
///
/// ```
/// let text = riff::view::lead_blocked(15, true, std::path::Path::new("/c.toml"));
/// let plain = anstream::adapter::strip_str(&text).to_string();
/// assert!(plain.starts_with("lead.wake  15  (/c.toml)\nlead.notify  true  (/c.toml)\n"), "{plain}");
/// assert!(plain.contains("riff lead blocked --notify off"), "{plain}");
/// ```
pub fn lead_blocked(wake: u64, notify: bool, path: &Path) -> String {
    let file = styled(DIM, &format!("({})", path.display()));
    let what = if notify {
        "and a desktop notification tells you. Turn it off with: riff lead blocked --notify off"
    } else {
        "and riff sends no desktop notification. Turn it on with: riff lead blocked --notify on"
    };
    format!(
        "lead.wake  {wake}  {file}\nlead.notify  {notify}  {file}\nA block with no answer for \
         {wake} minutes wakes the lead again. After {wake} minutes more, riff top shows \"the lead \
         gave no answer\", {what}"
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

/// `riff workers jobs` (01M3WFYZRK5CT22GJW6ZHYT9CC,
/// 01M3ZGZMV78G3BNVFGHAZWQQDX): the setting, the physical `cores`, the
/// pool of the machine with the tokens in use when it runs, and the
/// test threads of each worker.
///
/// ```
/// use riff::jobserver::State;
/// use riff::limits::{Cores, Limits};
///
/// let strip = |s: String| anstream::adapter::strip_str(&s).to_string();
/// let cores = Cores { physical: 8, logical: 16, read: true };
/// let limits = Limits { jobs: 2, tokens: 4, counted: 3, nice: 10 };
/// let path = "/h/c.toml".as_ref();
/// let pool = State { size: 4, free: 1, counted: 3, workers: 3 };
/// assert_eq!(
///     strip(riff::view::workers_jobs(0, &limits, &cores, Some(pool), path)),
///     "workers.jobs  0  (/h/c.toml)\n\
///      The machine has 8 physical cores. All workers take their compile jobs from one pool \
///      of 4 tokens: the physical cores less 1, less 3 workers (the limit, or the workers \
///      that run when they are more). Each build also has one job of its own. Now 3 tokens \
///      are in use. Each worker tests with 2 threads, from the same pool. Set it with: riff \
///      workers jobs N (N turns the pool off; 0: the pool)"
/// );
/// let over = State { workers: 5, ..pool };
/// assert!(strip(riff::view::workers_jobs(0, &limits, &cores, Some(over), path))
///     .contains("5 workers run, more than 3: each worker after the first 3 keeps one token \
///                out of the pool."));
/// assert!(strip(riff::view::workers_jobs(0, &limits, &cores, None, path))
///     .contains("Each build also has one job of its own. No worker runs now."));
/// let guess = Cores { read: false, ..cores };
/// assert!(strip(riff::view::workers_jobs(0, &limits, &guess, None, path))
///     .contains("riff cannot read the physical cores of this machine, so it counts half of \
///                the 16 logical CPUs: 8. All workers"));
/// let fixed = Limits { jobs: 6, tokens: 0, counted: 3, nice: 10 };
/// assert!(strip(riff::view::workers_jobs(6, &fixed, &cores, None, path))
///     .contains("No pool: each worker builds with 6 jobs and tests with 6 threads. Set it"));
/// ```
pub fn workers_jobs(
    value: u16,
    limits: &crate::limits::Limits,
    cores: &crate::limits::Cores,
    pool: Option<crate::jobserver::State>,
    path: &Path,
) -> String {
    let jobs = limits.jobs;
    let how = if limits.tokens == 0 {
        format!("No pool: each worker builds with {jobs} jobs and tests with {jobs} threads.")
    } else {
        let now = match pool {
            Some(pool) => {
                let mut now = format!(
                    "Now {} tokens are in use.",
                    pool.size.saturating_sub(pool.free)
                );
                if pool.workers > pool.counted {
                    let counted = pool.counted;
                    now.push_str(&format!(
                        " {} workers run, more than {counted}: each worker after the first \
                         {counted} keeps one token out of the pool.",
                        pool.workers
                    ));
                }
                now
            }
            None => "No worker runs now.".into(),
        };
        let cores = cores
            .said()
            .unwrap_or_else(|| format!("The machine has {} physical cores.", cores.physical));
        format!(
            "{cores} All workers take their compile jobs from one pool of {} tokens: the \
             physical cores less 1, less {} workers (the limit, or the workers that run when \
             they are more). Each build also has one job of its own. {now} Each worker tests \
             with {jobs} threads, from the same pool.",
            pool.map_or(limits.tokens, |pool| pool.size),
            pool.map_or(limits.counted, |pool| pool.counted),
        )
    };
    setting(
        "workers.jobs",
        &value.to_string(),
        path,
        &format!("{how} Set it with: riff workers jobs N (N turns the pool off; 0: the pool)"),
    )
}

/// `riff workers nice` (01M3WFYZTX05CGDP2NQF9B356K).
///
/// ```
/// let out = riff::view::workers_nice(10, "/h/c.toml".as_ref());
/// assert_eq!(
///     anstream::adapter::strip_str(&out).to_string(),
///     "workers.nice  10  (/h/c.toml)\n\
///      Each worker runs with nice 10. Set it with: riff workers nice N (0 to 19, 0 turns it off)"
/// );
/// ```
pub fn workers_nice(nice: u8, path: &Path) -> String {
    let what = match nice {
        0 => "with no nice".to_owned(),
        n => format!("with nice {n}"),
    };
    setting(
        "workers.nice",
        &nice.to_string(),
        path,
        &format!(
            "Each worker runs {what}. Set it with: riff workers nice N (0 to 19, 0 turns it off)"
        ),
    )
}

/// `riff workers memory` (01M3WFYZX6GVFYW6NTTTKF144R): the setting, and
/// the `gb` that all workers get.
///
/// ```
/// let out = riff::view::workers_memory(0, 23, "/h/c.toml".as_ref());
/// assert_eq!(
///     anstream::adapter::strip_str(&out).to_string(),
///     "workers.memory  0  (/h/c.toml)\n\
///      All workers of this machine get at most 23 GB of memory: three quarters of the \
///      memory. Set it with: riff workers memory GB (0: riff makes the number)"
/// );
/// ```
pub fn workers_memory(value: u32, gb: u32, path: &Path) -> String {
    let from = if value == 0 {
        ": three quarters of the memory"
    } else {
        ""
    };
    setting(
        "workers.memory",
        &value.to_string(),
        path,
        &format!(
            "All workers of this machine get at most {gb} GB of memory{from}. \
             Set it with: riff workers memory GB (0: riff makes the number)"
        ),
    )
}

/// `riff workers floor` (01M3WFZ01PTAYYKG3T5CFA2W4D): the setting, and
/// the memory that is available now.
///
/// ```
/// let out = riff::view::workers_floor(4, 24, "/h/c.toml".as_ref());
/// assert_eq!(
///     anstream::adapter::strip_str(&out).to_string(),
///     "workers.floor  4  (/h/c.toml)\n\
///      riff starts no new worker while less than 4 GB of memory is available. Now: 24 GB. \
///      Set it with: riff workers floor GB (0 turns it off)"
/// );
/// ```
pub fn workers_floor(floor: u32, avail_gb: u32, path: &Path) -> String {
    setting(
        "workers.floor",
        &floor.to_string(),
        path,
        &format!(
            "riff starts no new worker while less than {floor} GB of memory is available. \
             Now: {avail_gb} GB. Set it with: riff workers floor GB (0 turns it off)"
        ),
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
///         stopping: false,
///         claims_secs: 0,
///         must_clear: false,
///         fresh_secs: None,
///         state: Some(SessionState::Busy),
///         work: None,
///         waits: None,
///         blocked: None,
///         step: None,
///     },
///     SessionInfo {
///         uri: brett,
///         live: true,
///         idle_secs: 0,
///         status: None,
///         worker: false,
///         stopping: false,
///         claims_secs: 0,
///         must_clear: false,
///         fresh_secs: None,
///         state: Some(SessionState::Blocked),
///         work: None,
///         waits: None,
///         blocked: Some(blocked),
///         step: None,
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
///     stopping: false,
///     claims_secs: 300,
///     must_clear: false,
///     fresh_secs: None,
///     state: Some(SessionState::Busy),
///     work: None,
///     waits: None,
///     blocked: None,
///     step: None,
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
/// when riff knows them (01M3Q5QE4SQ8VYN2PSF42KB3QJ). The limit is the
/// wanted state, and the workers that run are the running state. The
/// heading says each difference, and why the machine starts no worker
/// (01M493Z063SSTAJS2KNDFBTEBJ).
///
/// ```
/// use riff::machine::Machine;
///
/// let plain = |s: String| anstream::adapter::strip_str(&s).to_string();
/// assert_eq!(plain(riff::view::host_heading("pangolin", 1, 1, None, 4, 0)), "pangolin  limit 1  runs 1");
/// let m = Machine { cores: 16, mhz: 4500, now_mhz: 4400, mem_gb: 32, avail_gb: 24, load: 1.5 };
/// assert_eq!(
///     plain(riff::view::host_heading("pangolin", 1, 1, Some(&m), 4, 0)),
///     "pangolin  limit 1  runs 1  cpu 16x4500MHz (now 4400MHz), mem 32GB, 24GB available, load 1.50  score 24.0"
/// );
/// // Fewer workers than the limit: room for more.
/// assert_eq!(
///     plain(riff::view::host_heading("pangolin", 3, 1, None, 4, 0)),
///     "pangolin  limit 3  runs 1: room for 2 workers, the rollout starts them for free work"
/// );
/// // More workers than the limit (01M402VFQHC5PH39DTFV6AH60F).
/// assert_eq!(
///     plain(riff::view::host_heading("pangolin", 2, 4, None, 4, 0)),
///     "pangolin  limit 2  runs 4: 2 workers end after their item"
/// );
/// // Why the machine starts no worker (01M3WFZ01PTAYYKG3T5CFA2W4D).
/// let low = Machine { avail_gb: 3, ..m };
/// assert_eq!(
///     plain(riff::view::host_heading("pangolin", 3, 3, Some(&low), 4, 0)),
///     "pangolin  limit 3  runs 3  cpu 16x4500MHz (now 4400MHz), mem 32GB, 3GB available, load 1.50  score 24.0\n\
///      Starts no worker: 3 GB of memory is available, and the floor of this machine is 4 GB."
/// );
/// // A loop of deaths (01M493YZZEW1FTDBNA090WT2AG).
/// assert_eq!(
///     plain(riff::view::host_heading("pangolin", 3, 1, None, 4, 4)),
///     "pangolin  limit 3  runs 1: room for 2 workers, the rollout starts them for free work\n\
///      Starts no worker: 4 workers died in the last hour. riff starts workers again when 3 or \
///      fewer died in the last hour."
/// );
/// ```
pub fn host_heading(
    host: &str,
    limit: u16,
    runs: usize,
    machine: Option<&crate::machine::Machine>,
    floor: u32,
    deaths: usize,
) -> String {
    let mut out = format!("{}  limit {limit}  runs {runs}", styled(BOLD, &safe(host)));
    if let Some(end) = text::end_after_item(limit, runs) {
        let _ = write!(out, "{}", styled(WARNING, &format!(": {end}")));
    }
    if let Some(room) = text::room_for(limit, runs) {
        let _ = write!(out, ": {room}");
    }
    let mut why = Vec::new();
    if let Some(m) = machine {
        let _ = write!(
            out,
            "  {}",
            styled(DIM, &format!("{m}  score {:.1}", m.score()))
        );
        if m.low(floor) {
            why.push(text::low_memory(m.avail_gb, floor));
        }
    }
    if crate::deaths::halted(deaths) {
        why.push(text::deaths_halt(deaths));
    }
    for why in why {
        let _ = write!(
            out,
            "\n{}",
            styled(WARNING, &format!("Starts no worker: {why}"))
        );
    }
    out
}

/// The line of the temp folders of the workers of this machine in
/// `riff workers`, under the line of its disk
/// (01M41VAGY396K07BTPSW9TNBX5).
///
/// ```
/// let plain = |s: String| anstream::adapter::strip_str(&s).to_string();
/// assert_eq!(
///     plain(riff::view::temp_line("/h/.cache/riff/tmp".as_ref(), 3 << 30)),
///     "temp 3.0GB in /h/.cache/riff/tmp\n"
/// );
/// ```
pub fn temp_line(root: &Path, bytes: u64) -> String {
    let line = format!("temp {} in {}", text::size_words(bytes), root.display());
    format!("{}\n", styled(DIM, &line))
}

/// `riff workers tmp` (01M41VAGJC69S9R2TD1B1EQ4W4): the folder of the
/// temp folders of the workers, and its disk use.
///
/// ```
/// let out = riff::view::workers_tmp("/h/.cache/riff/tmp".as_ref(), 0, "/h/c.toml".as_ref());
/// assert_eq!(
///     anstream::adapter::strip_str(&out).to_string(),
///     "workers.tmp  /h/.cache/riff/tmp  (/h/c.toml)\n\
///      Each worker gets a temp folder of its own in it, and riff deletes it when the \
///      worker ends. Now: 0MB. Set it with: riff workers tmp DIR"
/// );
/// ```
pub fn workers_tmp(root: &Path, bytes: u64, path: &Path) -> String {
    setting(
        "workers.tmp",
        &root.display().to_string(),
        path,
        &format!(
            "Each worker gets a temp folder of its own in it, and riff deletes it when the \
             worker ends. Now: {}. Set it with: riff workers tmp DIR",
            text::size_words(bytes)
        ),
    )
}

/// The line of the disk of one machine in `riff workers`, under its
/// heading (01M41A11GHP78E2VYN14JSE27P). Under the low mark it says that
/// the machine starts no worker (01M41A11DX1QRP48YPTDNT67W4). No disk
/// gives no line.
///
/// ```
/// use riff::disk::Disk;
///
/// let plain = |s: String| anstream::adapter::strip_str(&s).to_string();
/// assert_eq!(plain(riff::view::disk_line(Some(&Disk { free_gb: 200, total_gb: 455 }))), "disk 200GB free of 455GB (43%)\n");
/// assert_eq!(
///     plain(riff::view::disk_line(Some(&Disk { free_gb: 16, total_gb: 455 }))),
///     "disk 16GB free of 455GB (3%)\nStarts no worker: disk 16GB free of 455GB (3%), under 5%.\n"
/// );
/// assert_eq!(riff::view::disk_line(None), "");
/// ```
pub fn disk_line(disk: Option<&crate::disk::Disk>) -> String {
    let Some(disk) = disk else {
        return String::new();
    };
    let mut out = format!("{}\n", styled(DIM, &disk.to_string()));
    if disk.low() {
        let why = text::low_disk(disk);
        let _ = writeln!(
            out,
            "{}",
            styled(WARNING, &format!("Starts no worker: {why}"))
        );
    }
    out
}

/// The line of the monitor of one machine in `riff workers`, under its
/// heading (01M421QPX01BB15GJXHFYRETTX): on or off, and its last
/// numbers. `ago` is the age in seconds of the last look, when riff
/// knows it. A host that tells no numbers gives no line.
///
/// ```
/// use riff::monitor::{Kill, Numbers};
///
/// let plain = |s: String| anstream::adapter::strip_str(&s).to_string();
/// let n = Numbers { on: true, load5: 9.8, limit: 12.0, physical: 8, jobs: 2, kill: None };
/// assert_eq!(
///     plain(riff::view::monitor_line(Some(&n), Some(12))),
///     "monitor on  load5 9.80 of 12.00 (8 cores)  jobs 2  last look 12s ago\n"
/// );
/// let over = Numbers { load5: 13.0, ..n.clone() };
/// assert!(plain(riff::view::monitor_line(Some(&over), None)).starts_with("monitor on  load5 13.00 of 12.00"));
/// let kill = Some(Kill { at: 1727980000, by: "kernel".into(), what: String::new() });
/// let killed = Numbers { kill, ..n.clone() };
/// assert_eq!(
///     plain(riff::view::monitor_line(Some(&killed), None)),
///     format!("monitor on  load5 9.80 of 12.00 (8 cores)  jobs 2  last kill {} kernel\n", riff::text::clock(1727980000)),
/// );
/// let off = Numbers { on: false, ..n };
/// assert_eq!(
///     plain(riff::view::monitor_line(Some(&off), None)),
///     "monitor off  load5 9.80 of 12.00 (8 cores)  jobs 2\n"
/// );
/// assert_eq!(riff::view::monitor_line(None, None), "");
/// ```
pub fn monitor_line(numbers: Option<&crate::monitor::Numbers>, ago: Option<u64>) -> String {
    let Some(n) = numbers else {
        return String::new();
    };
    let state = if n.on { "monitor on" } else { "monitor off" };
    let load = format!(
        "load5 {:.2} of {:.2} ({} cores)",
        n.load5, n.limit, n.physical
    );
    let load = if n.load5 > n.limit {
        styled(WARNING, &load)
    } else {
        styled(DIM, &load)
    };
    let mut out = format!(
        "{}  {load}  {}",
        styled(DIM, state),
        styled(DIM, &format!("jobs {}", n.jobs))
    );
    if let Some(kill) = &n.kill {
        let kill = format!("last kill {} {}", text::clock(kill.at), safe(&kill.by));
        let _ = write!(out, "  {}", styled(WARNING, &kill));
    }
    if n.on
        && let Some(ago) = ago
    {
        let _ = write!(out, "  {}", styled(DIM, &format!("last look {ago}s ago")));
    }
    out.push('\n');
    out
}

/// `riff workers monitor` (01M421QPKWPX00X24F8V6DT8Z3): the settings,
/// the limits that they give on this machine, and the last look.
///
/// ```
/// use riff::monitor::{Saved, SavedLimits};
/// use riff::settings::Monitor;
///
/// let plain = |s: String| anstream::adapter::strip_str(&s).to_string();
/// let on = Monitor { on: true, every: 15, load: 1.5 };
/// let out = plain(riff::view::workers_monitor(&on, 8, 4, None, "/h/c.toml".as_ref()));
/// assert_eq!(
///     out,
///     "monitor.on  true  (/h/c.toml)\n\
///      monitor.every  15\n\
///      monitor.load  1.5\n\
///      The monitor looks at this machine each 15 seconds. It tells the lead when the 5-minute \
///      load goes over 12.00 (1.5 times 8 physical cores), when less than 4 GB of memory is \
///      available (workers.floor), and at each kill. Set it with: riff workers monitor on|off \
///      --every SECONDS --load N\n\
///      No look yet. A workers host, or the riff mcp of the lead, runs the monitor.\n"
/// );
/// let saved = Saved {
///     at: 1727980000,
///     load1: 3.1,
///     load5: 2.8,
///     avail_gb: 20,
///     limits: SavedLimits { load: 12.0, floor: 4 },
///     kill: None,
/// };
/// let out = plain(riff::view::workers_monitor(&on, 8, 4, Some(&saved), "/h/c.toml".as_ref()));
/// assert!(out.ends_with(&format!(
///     "Last look {}: load 3.10 2.80, 20 GB available. No kill.\n",
///     riff::text::clock(1727980000)
/// )));
/// let off = Monitor { on: false, ..on };
/// let out = plain(riff::view::workers_monitor(&off, 8, 4, None, "/h/c.toml".as_ref()));
/// assert!(out.ends_with("The monitor is off. Turn it on with: riff workers monitor on\n"), "{out}");
/// ```
pub fn workers_monitor(
    monitor: &crate::settings::Monitor,
    physical: u16,
    floor: u32,
    saved: Option<&crate::monitor::Saved>,
    path: &Path,
) -> String {
    let mut out = format!(
        "monitor.on  {}  {}\nmonitor.every  {}\nmonitor.load  {}\n",
        monitor.on,
        styled(DIM, &format!("({})", path.display())),
        monitor.every,
        monitor.load
    );
    if !monitor.on {
        let _ = writeln!(
            out,
            "{}",
            styled(
                DIM,
                "The monitor is off. Turn it on with: riff workers monitor on"
            )
        );
        return out;
    }
    let limit = monitor.load * f64::from(physical);
    let memory = if floor == 0 {
        String::new()
    } else {
        format!(", when less than {floor} GB of memory is available (workers.floor)")
    };
    let _ = writeln!(
        out,
        "{}",
        styled(
            DIM,
            &format!(
                "The monitor looks at this machine each {} seconds. It tells the lead when the \
                 5-minute load goes over {limit:.2} ({} times {physical} physical cores){memory}, \
                 and at each kill. Set it with: riff workers monitor on|off --every SECONDS \
                 --load N",
                monitor.every, monitor.load
            )
        )
    );
    let last = match saved {
        None => {
            "No look yet. A workers host, or the riff mcp of the lead, runs the monitor.".to_owned()
        }
        Some(s) => {
            let kill = match &s.kill {
                None => "No kill.".to_owned(),
                Some(k) => format!(
                    "Last kill {}: {} killed {}.",
                    text::clock(k.at),
                    safe(&k.by),
                    safe(&k.what)
                ),
            };
            format!(
                "Last look {}: load {:.2} {:.2}, {} GB available. {kill}",
                text::clock(s.at),
                s.load1,
                s.load5,
                s.avail_gb
            )
        }
    };
    let _ = writeln!(out, "{last}");
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
///     must_clear: false,
///     fresh_secs: None,
///     state: Some(SessionState::Busy),
///     work: None,
///     waits: None,
///     blocked: None,
///     step: None,
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
