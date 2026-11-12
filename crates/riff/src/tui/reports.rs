//! The Reports tab of the TUI (#1641, 01M54216GT8SX6NVMKRGNR2H0T): the
//! usage, the audit and the log of a session.
//!
//! # Design
//!
//! ```text
//!  1 Sessions  2 Chat•2  3 Waves•  4 Machines  5 Devices  [6 Reports]
//!  riff running · build 10df8a4 · link ok 2s
//!  [Usage]  Audit  Session log
//!  USAGE OF WAVE 48 · como-technologies/riff · 14.3M tokens in 6 issues
//!    ISSUE  TITLE                            TOKENS  MODELS
//!  > #994   Design: the TUI of riff          1.9M    fable 5.1 · sonnet 5.5
//!    #1590  An idle look asks GitHub first   3.1M    sonnet 5.5
//!    #1594  riff counts the gh calls         2.4M    sonnet 5.5
//!    #1599  The wave takes an issue          700k    sonnet 5.5
//!    #1600  The book tells how to detach     400k    sonnet 5.5
//!    #1601  Fix the help                     5.7M    sonnet 5.5 · opus 5.5
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!  Read from the comments of the issues with gh 3m ago.
//!  ↑↓ select  ←→ report  [ ] wave  Enter claims  r read again  ? help  q quit
//! ```
//!
//! The tab has three reports ([`Report`]). The row under the header
//! names them, with the report that shows in `[ ]`. `←` and `→` show
//! the report before or after, and a click on a name shows it. Each
//! report calls the code of its CLI command:
//!
//! | Report | The code of | When it reads |
//! |---|---|---|
//! | Usage | `riff usage --wave N`: [`read_usage`], [`crate::usage::Forge::wave_usage`] | When it shows with no read yet, after `[` or `]`, and on `r` |
//! | Audit | `riff audit --wave N`: [`run_audit`], [`crate::audit::of_wave`] | On `Enter`, and on `r` after an audit |
//! | Session log | `riff daemon events NAME`, with `--host HOST` for another host: [`follow`] | While it shows, up to the end of the session |
//!
//! A report reads no source on a timer (01M6VT3JFEKD2FEQGNGGKHC8X5).
//! The usage and the audit are of one repository: the repository of the
//! working folder, else the first repository with a live session
//! ([`State::reports_repo`]). They start with the current wave: the
//! open wave with the lowest number ([`crate::waves::current`]). `[` and
//! `]` show the wave before or after.
//!
//! ```mermaid
//! flowchart LR
//!     G["gh: the milestones,<br/>the issues, their comments"] -- "read_usage" --> U["Usage"]
//!     S["riff-server: the log"] -- "run_audit" --> A["Audit"]
//!     G -- "run_audit" --> A
//!     D["the riff daemon: the events<br/>of the session"] -- "follow" --> L["Session log"]
//!     T["the Sessions tab: Enter on a worker"] --> L
//! ```
//!
//! - **Usage** (01M6VT3JHVFQ27VJE8987WET34): the total of the wave, and
//!   a row for each issue with its tokens and its models, the model with
//!   the most tokens first. `Enter` on an issue shows its claims, as
//!   `riff usage ISSUE` prints them, from the same read
//!   ([`crate::usage::issue_text`]). `Esc` goes back to the issues.
//! - **Audit** (01M6VT3JMABMEXSBPE414MBJZP): before `Enter`, the report
//!   says that the audit reads the log of riff-server and the forge.
//!   `Enter` runs it in a task of its own, and the report shows each
//!   rule with `pass`, `fail` or `not checked`, as `riff audit` prints
//!   it ([`crate::audit::render`]).
//! - **Session log** (01M6VT3JPQXRWMX9PWYADT4MJX): `Enter` on a worker in
//!   the Sessions tab opens it with that session ([`State::open_log`]).
//!   It shows each line of `riff daemon events NAME`, and follows the
//!   session while the report shows. It keeps the last [`LOG_LINES`]
//!   lines. The window follows the end, until `↑` scrolls up. `End` or
//!   `G` follows the end again.
//!
//! The narrow layout has fewer words: the title has no repository, a
//! row of the usage has the issue, the tokens and the title, and the
//! note line is short.

use std::collections::VecDeque;

use anyhow::{Context, Result, anyhow};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use riff_client::api::Api;
use riff_core::name::{SessionUri, ThreadName};

use super::view::{ACTIVE, DIM, ERROR, GOOD, Hit, Layout, Placed, WARNING, width};
use super::{Action, Key, State, Tab};
use crate::pr::Gh;
use crate::reach::Reach;
use crate::text::{self, safe};
use crate::usage::{self, IssueUsage};

/// The most lines that the Session log keeps: the oldest go first.
pub const LOG_LINES: usize = 2000;

/// The width of the column of the title of an issue in the wide layout.
const TITLE: u16 = 32;

/// The style of the selected row.
const SELECTED: Style = Style::new().add_modifier(Modifier::REVERSED);

/// The Usage report with no repository.
pub const NO_REPOSITORY: &str = "The usage and the audit are of a repository. Run riff in a \
     clone, or start a session in a repository.";

/// The Audit report before `Enter` (01M6VT3JMABMEXSBPE414MBJZP).
pub const AUDIT_FIRST: &str = "The audit reads the log of riff-server, and each issue and pull \
     request of the wave with gh: the heaviest read of the TUI. Only the owner and the admins \
     can read the log. Press Enter to start it.";

/// The Audit report while the audit runs.
pub const AUDITING: &str = "riff audits the wave: it reads the log of riff-server and the forge.";

/// The Session log with no session (01M6VT3JPQXRWMX9PWYADT4MJX).
pub const NO_LOG: &str =
    "No session yet. Enter on a worker in the Sessions tab shows its events here.";

/// The Session log before the first line of the session.
pub const LOG_STARTS: &str = "riff reads the events of the session.";

/// The Sessions tab: Enter on a session that runs in no container that
/// riff knows.
pub const NO_LOG_OF_SESSION: &str = "This session runs in no container that riff knows: riff \
     daemon events shows only a session of a riff daemon.";

/// `[` or `]` before the first read of the waves.
pub const NO_WAVES_YET: &str = "riff knows the waves of the repository after the first read.";

/// A report of the tab, in the order of the row under the header.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Report {
    /// The tokens of the issues of a wave: `riff usage --wave N`.
    #[default]
    Usage,
    /// The rules of a wave: `riff audit --wave N`.
    Audit,
    /// The events of a session: `riff daemon events NAME`.
    Log,
}

impl Report {
    /// Each report, in the order of the row.
    pub const ALL: [Report; 3] = [Report::Usage, Report::Audit, Report::Log];

    /// The name of the report in the row.
    pub fn name(self) -> &'static str {
        match self {
            Report::Usage => "Usage",
            Report::Audit => "Audit",
            Report::Log => "Session log",
        }
    }

    /// The report after this one: `→`. The last comes before the first.
    ///
    /// ```
    /// use riff::tui::reports::Report;
    ///
    /// assert_eq!(Report::Usage.next(), Report::Audit);
    /// assert_eq!(Report::Log.next(), Report::Usage);
    /// assert_eq!(Report::Usage.before(), Report::Log);
    /// ```
    pub fn next(self) -> Report {
        Report::ALL[(self as usize + 1) % Report::ALL.len()]
    }

    /// The report before this one: `←`.
    pub fn before(self) -> Report {
        Report::ALL[(self as usize + Report::ALL.len() - 1) % Report::ALL.len()]
    }
}

/// A wave of the repository of the reports: its number, its full title,
/// and the number of each wave of the repository, open and closed, in
/// order, for `[` and `]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wave {
    pub number: u64,
    pub title: String,
    pub waves: Vec<u64>,
}

/// The wave `wave` of `repo` (`OWNER/REPO`), else its current wave
/// ([`crate::waves::current`]), with [`crate::waves::find`] over each
/// milestone, open and closed, as `riff usage --wave` and `riff audit
/// --wave` find it ([`crate::waves::title`]). One call of `gh`.
pub fn wave_of(gh: &Gh, repo: &str, wave: Option<u64>) -> Result<Wave> {
    let all = crate::waves::all(gh, repo)?;
    let milestones = crate::waves::pairs(&all);
    let number = match wave {
        Some(number) => number,
        None => crate::waves::current(&milestones)
            .ok_or_else(|| anyhow!("{} has no open wave.", safe(repo)))?,
    };
    let title = crate::waves::find(&milestones, &number.to_string())?;
    let mut waves: Vec<u64> = milestones
        .iter()
        .filter_map(|(title, _)| crate::waves::wave_number(title))
        .collect();
    waves.sort_unstable();
    waves.dedup();
    Ok(Wave {
        number,
        title,
        waves,
    })
}

/// The usage of one wave of one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    /// `OWNER/REPO`.
    pub repo: String,
    pub wave: Wave,
    /// Each issue of the wave, in number order.
    pub issues: Vec<IssueUsage>,
}

impl Usage {
    /// The tokens of the whole wave.
    pub fn tokens(&self) -> usage::Tokens {
        self.issues.iter().map(IssueUsage::tokens).sum()
    }
}

/// Reads the usage of the wave `wave` of `repo`, else of its current
/// wave, with the code of `riff usage --wave N` ([`wave_of`],
/// [`crate::usage::Forge::wave_usage`]). The calls of `gh` block: the
/// TUI runs it in a blocking task.
pub fn read_usage(gh: &Gh, repo: &str, wave: Option<u64>) -> Result<Usage> {
    let forge = usage::Forge::of(gh, repo).with_context(|| format!("{repo} is no repository"))?;
    let wave = wave_of(gh, repo, wave)?;
    let issues = forge.wave_usage(&wave.title)?;
    Ok(Usage {
        repo: repo.to_owned(),
        wave,
        issues,
    })
}

/// An audit of one wave of one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audited {
    /// `OWNER/REPO`.
    pub repo: String,
    pub wave: Wave,
    pub report: crate::audit::Report,
}

/// Audits the wave `wave` of `repo`, else its current wave, with the
/// code of `riff audit --wave N` ([`wave_of`],
/// [`crate::audit::of_wave`]). The calls of `gh` run on threads of their
/// own ([`crate::pr::on_thread`]).
pub async fn run_audit(gh: &Gh, reach: &Reach, repo: &str, wave: Option<u64>) -> Result<Audited> {
    let thread: ThreadName = repo.parse()?;
    let (asker, name) = (gh.clone(), repo.to_owned());
    let wave = crate::pr::on_thread(move || wave_of(&asker, &name, wave)).await??;
    let report = crate::audit::of_wave(gh, reach, &thread, &wave.title).await?;
    Ok(Audited {
        repo: repo.to_owned(),
        wave,
        report,
    })
}

/// The session of the Session log: the name of its container, and its
/// host when it is not this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub host: Option<String>,
}

/// Follows the session `source` up to its end, with the code of `riff
/// daemon events NAME` (01M6VT3JPQXRWMX9PWYADT4MJX): through the socket
/// of the riff daemon of this machine ([`crate::daemon::follow_events`]),
/// through the socket of the session in a container
/// ([`Reach::follow_events`]), or for another host through riff-server
/// as `me`, as with `--host HOST`
/// ([`crate::daemon::remote::follow_events`]). Each line goes to
/// `lines`. It gives true when the session failed.
pub async fn follow(
    api: &Api,
    me: &SessionUri,
    reach: &Reach,
    source: &Source,
    lines: tokio::sync::mpsc::UnboundedSender<String>,
) -> Result<bool> {
    let show = |line: &str| {
        let _ = lines.send(line.to_owned());
    };
    let name = source.name.as_str();
    match (source.host.as_deref(), reach) {
        (Some(host), Reach::Direct(_)) => {
            crate::daemon::remote::follow_events(api, me, host, name, show).await
        }
        (host, Reach::Daemon(_)) => reach.follow_events(host, name, show).await,
        (None, Reach::Direct(_)) => {
            crate::daemon::follow_events(&crate::daemon::dir()?, name, show, show).await
        }
    }
}

/// A read of a report: the last good read and its time, the fault of
/// the last read, and whether a read is asked or runs.
#[derive(Debug, Clone)]
pub struct Got<T> {
    last: Option<(T, i64)>,
    fault: Option<String>,
    run: Run,
}

impl<T> Default for Got<T> {
    fn default() -> Got<T> {
        Got {
            last: None,
            fault: None,
            run: Run::Idle,
        }
    }
}

/// Whether a read of a report is asked or runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Run {
    Idle,
    /// Asked: the loop starts it.
    Due,
    Running,
}

impl<T> Got<T> {
    /// Takes the read `read` at `at_ms`: a fault keeps the last read.
    /// An `r` while the read ran asks for one more.
    fn took(&mut self, read: Result<T, String>, at_ms: i64) {
        if self.run == Run::Running {
            self.run = Run::Idle;
        }
        match read {
            Ok(got) => {
                self.last = Some((got, at_ms));
                self.fault = None;
            }
            Err(e) => self.fault = Some(e),
        }
    }

    /// The last good read.
    fn last(&self) -> Option<&T> {
        self.last.as_ref().map(|(got, _)| got)
    }
}

/// The Session log: the session, its lines, and the end of its follow.
#[derive(Debug, Clone)]
pub struct Log {
    pub source: Source,
    lines: VecDeque<String>,
    /// The end of the follow: `Ok(true)` when the session failed, or the
    /// fault of the follow. `None` while the session runs.
    end: Option<Result<bool, String>>,
    /// True while a follow runs.
    running: bool,
}

impl Log {
    fn new(source: Source) -> Log {
        Log {
            source,
            lines: VecDeque::new(),
            end: None,
            running: false,
        }
    }

    /// The lines of the log, oldest first.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }
}

/// The window of a long report at the last draw: its first row, the rows
/// that fit, and the rows of the report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Window {
    pub top: usize,
    pub fit: usize,
    pub rows: usize,
}

/// The Reports tab: the report that shows, the wave, the reads, and the
/// choices of the person. They live while the TUI runs.
#[derive(Debug, Clone)]
pub struct Reports {
    report: Report,
    /// The wave that the person picked with `[` and `]`. `None` is the
    /// current wave.
    pick: Option<u64>,
    /// The wave of the last read: its number and each wave.
    known: Option<Wave>,
    usage: Got<Usage>,
    audit: Got<Audited>,
    /// The selected issue of the usage, and true while its claims show.
    at: usize,
    claims: bool,
    log: Option<Log>,
    /// The first row of the window that the person asked for, and true
    /// while the window of the log follows its end.
    pub(super) top: usize,
    tail: bool,
    /// The window at the last draw.
    pub(super) window: Window,
}

impl Default for Reports {
    fn default() -> Reports {
        Reports {
            report: Report::Usage,
            pick: None,
            known: None,
            usage: Got::default(),
            audit: Got::default(),
            at: 0,
            claims: false,
            log: None,
            top: 0,
            tail: true,
            window: Window::default(),
        }
    }
}

impl Reports {
    /// The report that shows.
    pub fn report(&self) -> Report {
        self.report
    }

    /// The Session log, when a session has one.
    pub fn log(&self) -> Option<&Log> {
        self.log.as_ref()
    }

    /// The number of the wave that the person picked, else of the wave
    /// of the last read.
    fn wave_number(&self) -> Option<u64> {
        self.pick.or(self.known.as_ref().map(|w| w.number))
    }

    /// Shows the report `report`.
    fn show(&mut self, report: Report) {
        self.report = report;
        self.top = 0;
        self.tail = true;
    }

    /// `[` and `]`: picks the wave before or after. A new wave has no
    /// read yet. It gives a note when there is no such wave.
    fn step(&mut self, after: bool) -> Option<String> {
        let Some(known) = &self.known else {
            return Some(NO_WAVES_YET.to_owned());
        };
        let now = self.pick.unwrap_or(known.number);
        let to = if after {
            known.waves.iter().find(|n| **n > now)
        } else {
            known.waves.iter().rev().find(|n| **n < now)
        };
        let Some(&to) = to else {
            return Some(if after {
                format!("Wave {now} is the last wave of the repository.")
            } else {
                format!("Wave {now} is the first wave of the repository.")
            });
        };
        self.pick = Some(to);
        self.usage = Got::default();
        self.audit = Got::default();
        (self.at, self.claims, self.top) = (0, false, 0);
        None
    }

    /// `r`: reads the report that shows again: the usage, the audit after
    /// one ran, or the log from its start.
    fn again(&mut self) {
        match self.report {
            Report::Usage => self.usage.run = Run::Due,
            Report::Audit if self.audit.last.is_some() || self.audit.fault.is_some() => {
                self.audit.run = Run::Due;
            }
            Report::Audit => {}
            Report::Log => {
                if let Some(log) = &mut self.log {
                    log.end = None;
                }
            }
        }
    }

    /// The rows of the window of a long report move by `by`, or to the
    /// start or the end. A move to the end of the log follows it again.
    fn scroll(&mut self, to: Scroll) {
        let Window { top, fit, rows } = self.window;
        let last = rows.saturating_sub(fit);
        let top = match to {
            Scroll::Up(n) => top.saturating_sub(n),
            Scroll::Down(n) => top + n,
            Scroll::Start => 0,
            Scroll::End => last,
        };
        self.tail = top >= last;
        self.top = top.min(last);
    }
}

/// A move of the window of a long report.
#[derive(Debug, Clone, Copy)]
enum Scroll {
    Up(usize),
    Down(usize),
    Start,
    End,
}

impl State {
    /// The Reports tab.
    pub fn reports(&self) -> &Reports {
        &self.reports
    }

    /// The repository of the usage and the audit: the repository of the
    /// working folder, else the first repository with a live session.
    pub fn reports_repo(&self) -> Option<String> {
        self.waves.here().map(str::to_owned).or_else(|| {
            let (look, _) = self.last_look()?;
            self.waves.repos(look).into_iter().next()
        })
    }

    /// The repository and the wave of a read of the usage that the loop
    /// starts now: the Usage report shows with no read yet, or the person
    /// asked for one.
    pub fn usage_due(&self) -> Option<(String, Option<u64>)> {
        let r = &self.reports;
        let shown = self.tab == Tab::Reports && r.report == Report::Usage;
        let unread = r.usage.last.is_none() && r.usage.fault.is_none();
        let due = match r.usage.run {
            Run::Due => true,
            Run::Idle => shown && unread,
            Run::Running => false,
        };
        due.then(|| self.reports_repo().map(|repo| (repo, r.pick)))
            .flatten()
    }

    /// A read of the usage starts.
    pub fn usage_started(&mut self) {
        self.reports.usage.run = Run::Running;
    }

    /// Takes the read `read` of the usage of the wave `asked`, at `at_ms`.
    /// A read of another wave than the one that the person picked since
    /// counts for nothing.
    pub fn usage_read(&mut self, asked: Option<u64>, read: Result<Usage, String>, at_ms: i64) {
        let r = &mut self.reports;
        if asked != r.pick {
            return;
        }
        if let Ok(usage) = &read {
            r.known = Some(usage.wave.clone());
            r.at = r.at.min(usage.issues.len().saturating_sub(1));
        }
        r.usage.took(read, at_ms);
    }

    /// The repository and the wave of an audit that the loop starts now:
    /// the person pressed `Enter` or `r`.
    pub fn audit_due(&self) -> Option<(String, Option<u64>)> {
        let r = &self.reports;
        (r.audit.run == Run::Due)
            .then(|| self.reports_repo().map(|repo| (repo, r.pick)))
            .flatten()
    }

    /// An audit starts.
    pub fn audit_started(&mut self) {
        self.reports.audit.run = Run::Running;
    }

    /// Takes the audit `read` of the wave `asked`, at `at_ms`.
    pub fn audited(&mut self, asked: Option<u64>, read: Result<Audited, String>, at_ms: i64) {
        let r = &mut self.reports;
        if asked != r.pick {
            return;
        }
        if let Ok(audited) = &read {
            r.known = Some(audited.wave.clone());
        }
        r.audit.took(read, at_ms);
    }

    /// The session whose events the loop follows now: while the Session
    /// log shows, up to the end of the session.
    pub fn log_wanted(&self) -> Option<&Source> {
        let r = &self.reports;
        let shown = self.tab == Tab::Reports && r.report == Report::Log;
        r.log
            .as_ref()
            .filter(|log| shown && log.end.is_none())
            .map(|log| &log.source)
    }

    /// A follow of the log starts: from the start of the session.
    pub fn log_started(&mut self) {
        if let Some(log) = &mut self.reports.log {
            log.lines.clear();
            log.running = true;
        }
    }

    /// Takes a line of the log. The oldest line goes after
    /// [`LOG_LINES`] lines.
    pub fn log_came(&mut self, line: String) {
        let r = &mut self.reports;
        let Some(log) = &mut r.log else {
            return;
        };
        log.lines.push_back(safe(&line));
        if log.lines.len() > LOG_LINES {
            log.lines.pop_front();
            r.top = r.top.saturating_sub(1);
        }
    }

    /// The follow of the log ended: `Ok(true)` when the session failed,
    /// or its fault.
    pub fn log_ended(&mut self, end: Result<bool, String>) {
        if let Some(log) = &mut self.reports.log {
            log.running = false;
            log.end = Some(end);
        }
    }

    /// The loop stopped the follow of the log: the report does not show.
    pub fn log_stopped(&mut self) {
        if let Some(log) = &mut self.reports.log {
            log.running = false;
        }
    }

    /// Opens the Session log of `source` (01M6VT3JPQXRWMX9PWYADT4MJX): the
    /// Reports tab shows it, and the loop follows it.
    pub fn open_log(&mut self, source: Source) {
        let r = &mut self.reports;
        if r.log.as_ref().is_none_or(|log| log.source != source) {
            r.log = Some(Log::new(source));
        }
        r.show(Report::Log);
        self.open(Tab::Reports);
    }

    /// `r` in the Reports tab: the report that shows reads again.
    pub(super) fn reports_again(&mut self) {
        self.reports.again();
    }

    /// The key `key` of the person in the Reports tab.
    pub(super) fn on_reports_key(&mut self, key: Key) -> Option<Action> {
        let r = &mut self.reports;
        let page = r.window.fit.max(1);
        let note = match key {
            Key::Left => {
                r.show(r.report.before());
                None
            }
            Key::Right => {
                r.show(r.report.next());
                None
            }
            Key::Report(report) => {
                r.show(report);
                None
            }
            Key::Char('[') => r.step(false),
            Key::Char(']') => r.step(true),
            key if r.report == Report::Usage && !r.claims => {
                let len = r.usage.last().map_or(0, |u| u.issues.len());
                let last = len.saturating_sub(1);
                match key {
                    Key::Up => r.at = r.at.saturating_sub(1),
                    Key::Down => r.at = (r.at + 1).min(last),
                    Key::PageUp => r.at = r.at.saturating_sub(page),
                    Key::PageDown => r.at = (r.at + page).min(last),
                    Key::First => r.at = 0,
                    Key::Last => r.at = last,
                    Key::Row(at) => r.at = at.min(last),
                    Key::Enter if len > 0 => (r.claims, r.top) = (true, 0),
                    _ => {}
                }
                None
            }
            key => {
                match key {
                    Key::Up => r.scroll(Scroll::Up(1)),
                    Key::Down => r.scroll(Scroll::Down(1)),
                    Key::PageUp => r.scroll(Scroll::Up(page)),
                    Key::PageDown => r.scroll(Scroll::Down(page)),
                    Key::First => r.scroll(Scroll::Start),
                    Key::Last => r.scroll(Scroll::End),
                    Key::Esc if r.claims => (r.claims, r.top) = (false, 0),
                    Key::Enter if r.report == Report::Audit && r.audit.run == Run::Idle => {
                        r.audit.run = Run::Due;
                        r.top = 0;
                    }
                    _ => {}
                }
                None
            }
        };
        if let Some(note) = note {
            self.note(note);
        }
        None
    }
}

/// `text` cut to `cols` columns, with `…` at the end of a cut.
fn cut(text: &str, cols: u16) -> String {
    if width(text) <= cols {
        return text.to_owned();
    }
    let mut out = String::new();
    for c in text.chars() {
        if width(&out) + width(&c.to_string()) + 1 > cols {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// `text` cut or filled with spaces to `cols` columns.
fn pad(text: &str, cols: u16) -> String {
    let text = cut(text, cols);
    let fill = usize::from(cols.saturating_sub(width(&text)));
    format!("{text}{}", " ".repeat(fill))
}

/// The rows of `text` in `cols` columns, each with its style: a long line
/// wraps, and each more row starts two columns after the start of its
/// line.
fn wrapped(text: &str, cols: u16, style: impl Fn(&str) -> Style) -> Vec<Line<'static>> {
    let cols = usize::from(cols.max(1));
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = safe(line);
        let style = style(&line);
        if line.is_empty() {
            rows.push(Line::raw(""));
            continue;
        }
        let indent: String = line.chars().take_while(|c| *c == ' ').collect();
        let more = format!("{indent}  ");
        let options = if more.len() + 8 < cols {
            textwrap::Options::new(cols).subsequent_indent(&more)
        } else {
            textwrap::Options::new(cols)
        };
        rows.extend(
            textwrap::wrap(&line, options)
                .into_iter()
                .map(|row| Line::styled(row.into_owned(), style)),
        );
    }
    rows
}

/// The style of a line of `riff audit`: the verdict of a rule and of a
/// check, and the result.
///
/// ```
/// use ratatui::style::{Color, Style};
/// use riff::tui::reports::audit_style;
///
/// let (good, warning, error) = (
///     Style::new().fg(Color::Green),
///     Style::new().fg(Color::Yellow),
///     Style::new().fg(Color::Red),
/// );
/// assert_eq!(audit_style("Rule 1: pass"), good);
/// assert_eq!(audit_style("Rule 2: fail"), error);
/// assert_eq!(audit_style("Rule 7: not checked"), warning);
/// assert_eq!(audit_style("  fail: #12 has no pull request"), error);
/// assert_eq!(audit_style("Result: each rule passes."), good);
/// assert_eq!(audit_style("Result: no rule fails. 2 rules are not checked."), warning);
/// assert_eq!(audit_style("Result: rule 2 fails."), error);
/// assert_eq!(audit_style("  Each verifier held no other claim."), Style::new());
/// ```
pub fn audit_style(line: &str) -> Style {
    if let Some((_, verdict)) = line
        .strip_prefix("Rule ")
        .and_then(|rest| rest.split_once(": "))
    {
        return match verdict {
            "pass" => GOOD,
            "fail" => ERROR,
            _ => WARNING,
        };
    }
    if let Some(result) = line.strip_prefix("Result: ") {
        return if result.starts_with("each rule passes") {
            GOOD
        } else if result.starts_with("no rule fails") {
            WARNING
        } else {
            ERROR
        };
    }
    if line.starts_with("  fail: ") {
        ERROR
    } else {
        Style::new()
    }
}

/// The row under the header: the name of each report, the one that
/// shows in `[ ]`, and the place of each name that a click hits.
fn strip(state: &State, area: Rect) -> (Line<'static>, Vec<Hit>) {
    let mut spans = Vec::new();
    let mut hits = Vec::new();
    let mut x = area.x;
    for (i, report) in Report::ALL.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
            x += 2;
        }
        let shown = report == state.reports.report;
        let label = if shown {
            format!("[{}]", report.name())
        } else {
            report.name().to_owned()
        };
        let wide = width(&label);
        hits.push(Hit {
            area: Rect::new(x, area.y, wide, 1).intersection(area),
            key: Key::Report(report),
        });
        spans.push(if shown {
            Span::styled(label, ACTIVE)
        } else {
            Span::raw(label)
        });
        x += wide;
    }
    (Line::from(spans), hits)
}

/// `N issues`, or `1 issue`.
fn issues(n: usize) -> String {
    if n == 1 {
        "1 issue".to_owned()
    } else {
        format!("{n} issues")
    }
}

/// The name of the wave of the reports in a title: `WAVE 48`, or `THE
/// CURRENT WAVE` before the first read.
fn wave_name(r: &Reports) -> String {
    r.wave_number()
        .map_or_else(|| "THE CURRENT WAVE".to_owned(), |n| format!("WAVE {n}"))
}

/// The rows of the body of the Reports tab in `area`, the places that a
/// click hits, and the window of a long report. `margin` is the column
/// before each row of the wide layout.
pub fn body(
    state: &State,
    area: Rect,
    layout: Layout,
    margin: u16,
) -> (Vec<Placed>, Vec<Hit>, Window) {
    let mut placed = Vec::new();
    let mut hits = Vec::new();
    if area.height < 3 {
        return (placed, hits, Window::default());
    }
    let row = |y: u16| Rect::new(area.x + margin, area.y + y, area.width - margin, 1);
    let cols = area.width - margin;
    let (line, strip_hits) = strip(state, row(0));
    placed.push(Placed { area: row(0), line });
    hits.extend(strip_hits);
    let r = &state.reports;
    let wide = layout == Layout::Wide;
    let repo = state.reports_repo();
    let (title, lines, select) = match r.report {
        Report::Usage => usage_lines(state, repo.as_deref(), cols, wide),
        Report::Audit => audit_lines(state, repo.as_deref(), cols),
        Report::Log => log_lines(state, cols),
    };
    placed.push(Placed {
        area: row(1),
        line: Line::styled(title, ACTIVE),
    });
    let fit = usize::from(area.height - 2);
    let (head, fit) = match &select {
        Some(_) if !lines.is_empty() => (1, fit - 1),
        _ => (0, fit),
    };
    let rows = lines.len() - head;
    let top = match select {
        Some(at) => super::sessions::window(r.top, at, fit, rows),
        None if r.report == Report::Log && r.tail => rows.saturating_sub(fit),
        None => r.top.min(rows.saturating_sub(fit)),
    };
    let mut lines = lines.into_iter();
    if head == 1
        && let Some(line) = lines.next()
    {
        placed.push(Placed { area: row(2), line });
    }
    let first = 2 + u16::try_from(head).unwrap_or(0);
    for ((i, line), y) in lines
        .enumerate()
        .skip(top)
        .zip(first..area.height)
        .take(fit)
    {
        if select.is_some() {
            hits.push(Hit {
                area: row(y),
                key: Key::Row(i),
            });
        }
        placed.push(Placed { area: row(y), line });
    }
    (placed, hits, Window { top, fit, rows })
}

/// The title and the lines of the Usage report in `cols` columns, and
/// the selected issue when the lines are the issues: then the first line
/// is the line of the columns.
fn usage_lines(
    state: &State,
    repo: Option<&str>,
    cols: u16,
    wide: bool,
) -> (String, Vec<Line<'static>>, Option<usize>) {
    let r = &state.reports;
    let wave = wave_name(r);
    let Some(usage) = r.usage.last() else {
        let title = match (repo, wide) {
            (Some(repo), true) => format!("USAGE OF {wave} · {}", safe(repo)),
            _ => format!("USAGE OF {wave}"),
        };
        let text = match (repo, &r.usage.fault, r.usage.run) {
            (None, _, _) => NO_REPOSITORY.to_owned(),
            (_, Some(fault), Run::Idle) => {
                format!("riff cannot read the usage: {fault}\nPress r to read it again.")
            }
            _ => "riff reads the comments of the issues of the wave with gh.".to_owned(),
        };
        return (title, wrapped(&text, cols, |_| Style::new()), None);
    };
    let total = usage::short_count(usage.tokens().total());
    let count = issues(usage.issues.len());
    if r.claims
        && let Some(issue) = usage.issues.get(r.at)
    {
        let title = format!("CLAIMS OF #{} · {}", issue.number, safe(&issue.title));
        let mut text = usage::issue_text(issue.number, &issue.counted);
        if let Some(line) = &issue.uncounted {
            text.push_str(line);
        }
        return (title, wrapped(&text, cols, |_| Style::new()), None);
    }
    let number = usage.wave.number;
    let title = if wide {
        format!(
            "USAGE OF WAVE {number} · {} · {total} tokens in {count}",
            safe(&usage.repo)
        )
    } else {
        format!("WAVE {number} · {total} tokens in {count}")
    };
    if usage.issues.is_empty() {
        let text = format!("Wave {number} has no issue.");
        return (title, vec![Line::raw(text)], None);
    }
    let head = if wide {
        format!("  ISSUE  {}TOKENS  MODELS", pad("TITLE", TITLE + 1))
    } else {
        "  ISSUE  TOKENS TITLE".to_owned()
    };
    let mut lines = vec![Line::styled(head, DIM)];
    for (i, issue) in usage.issues.iter().enumerate() {
        let selected = i == r.at;
        let number = pad(&format!("#{}", issue.number), 7);
        let tokens = usage::short_count(issue.tokens().total());
        let line = if wide {
            let models: Vec<String> = issue
                .models()
                .iter()
                .map(|m| usage::short_model(m))
                .collect();
            let models = if models.is_empty() {
                "-".to_owned()
            } else {
                safe(&models.join(" · "))
            };
            format!(
                "{}{number}{}{}{models}",
                if selected { "> " } else { "  " },
                pad(&cut(&safe(&issue.title), TITLE), TITLE + 1),
                pad(&tokens, 8),
            )
        } else {
            let title = cut(&safe(&issue.title), cols.saturating_sub(16));
            format!(
                "{}{number}{}{title}",
                if selected { "> " } else { "  " },
                pad(&tokens, 7),
            )
        };
        let line = Line::raw(cut(&line, cols));
        lines.push(if selected {
            line.patch_style(SELECTED)
        } else {
            line
        });
    }
    (title, lines, Some(r.at))
}

/// The title and the lines of the Audit report in `cols` columns.
fn audit_lines(
    state: &State,
    repo: Option<&str>,
    cols: u16,
) -> (String, Vec<Line<'static>>, Option<usize>) {
    let r = &state.reports;
    let title = match r.audit.last() {
        Some(audited) => format!(
            "AUDIT OF WAVE {} · {}",
            audited.wave.number,
            safe(&audited.repo)
        ),
        None => match repo {
            Some(repo) => format!("AUDIT OF {} · {}", wave_name(r), safe(repo)),
            None => format!("AUDIT OF {}", wave_name(r)),
        },
    };
    let text = match (repo, r.audit.last(), &r.audit.fault, r.audit.run) {
        (None, ..) => NO_REPOSITORY.to_owned(),
        (_, _, Some(fault), Run::Idle) => format!("riff cannot audit the wave: {fault}"),
        (_, Some(audited), ..) => crate::audit::render(&audited.report, &audited.repo),
        (_, None, _, Run::Idle) => AUDIT_FIRST.to_owned(),
        (_, None, ..) => AUDITING.to_owned(),
    };
    (title, wrapped(&text, cols, audit_style), None)
}

/// The title and the lines of the Session log in `cols` columns.
fn log_lines(state: &State, cols: u16) -> (String, Vec<Line<'static>>, Option<usize>) {
    let Some(log) = &state.reports.log else {
        return (
            "SESSION LOG".to_owned(),
            wrapped(NO_LOG, cols, |_| Style::new()),
            None,
        );
    };
    let host = log
        .source
        .host
        .as_deref()
        .unwrap_or_else(|| state.me.place().host());
    let now = match &log.end {
        None if log.running => "follows",
        None => "starts",
        Some(Ok(false)) => "ended",
        Some(Ok(true)) => "failed",
        Some(Err(_)) => "stopped",
    };
    let title = format!("LOG OF {} · {} · {now}", safe(&log.source.name), safe(host));
    let mut lines: Vec<Line<'static>> = log
        .lines
        .iter()
        .flat_map(|line| wrapped(line, cols, |_| Style::new()))
        .collect();
    if lines.is_empty() && log.end.is_none() {
        lines.push(Line::styled(LOG_STARTS, DIM));
    }
    if let Some(Err(fault)) = &log.end {
        let fault = format!("riff cannot follow the session: {fault}");
        lines.extend(wrapped(&fault, cols, |_| ERROR));
    }
    (title, lines, None)
}

/// The keys of the key line of the Reports tab: the text of the key, its
/// word, and the key that a click on it presses.
pub fn keys(state: &State, layout: Layout) -> Vec<(&'static str, &'static str, Option<Key>)> {
    let r = &state.reports;
    let list = r.report == Report::Usage && !r.claims;
    let mut keys = vec![
        ("↑↓", if list { "select" } else { "scroll" }, None),
        ("←→", "report", None),
    ];
    if layout == Layout::Wide {
        match r.report {
            Report::Usage if r.claims => keys.insert(1, ("Esc", "back", Some(Key::Esc))),
            Report::Usage => keys.extend([
                ("[ ]", "wave", None),
                ("Enter", "claims", Some(Key::Enter)),
                ("r", "read again", Some(Key::Again)),
            ]),
            Report::Audit => {
                keys.extend([("[ ]", "wave", None), ("Enter", "audit", Some(Key::Enter))])
            }
            Report::Log => keys.push(("End", "follow", Some(Key::Last))),
        }
    }
    keys.extend([
        ("?", "help", Some(Key::Help)),
        ("q", "quit", Some(Key::Quit)),
    ]);
    keys
}

/// The keys of the Reports tab in the help popup.
pub fn help_keys() -> Vec<(&'static str, &'static str)> {
    vec![
        ("← →", "show the report before or after"),
        ("[ ]", "show the wave before or after"),
        ("↑ ↓ j k", "select an issue, or scroll the report"),
        ("PgUp PgDn g G", "move a page, or to the start or the end"),
        ("Enter", "show the claims of the issue, or start the audit"),
        ("Esc", "go back from the claims to the issues"),
        ("End", "follow the end of the session log"),
    ]
}

/// The note line of the Reports tab with no other note: the age of the
/// read of the usage or of the audit, or that a read runs.
pub fn note(state: &State, layout: Layout, now_ms: i64) -> Option<String> {
    let r = &state.reports;
    let ago = |at_ms: i64| text::ago(u64::try_from((now_ms - at_ms) / 1000).unwrap_or(0));
    match r.report {
        Report::Usage if r.usage.run != Run::Idle && r.usage.last.is_some() => {
            Some("riff reads the usage again.".to_owned())
        }
        Report::Usage => {
            let (_, at_ms) = r.usage.last.as_ref()?;
            Some(match layout {
                Layout::Wide => format!(
                    "Read from the comments of the issues with gh {} ago.",
                    ago(*at_ms)
                ),
                _ => format!("Read with gh {} ago.", ago(*at_ms)),
            })
        }
        Report::Audit if r.audit.run != Run::Idle && r.audit.last.is_some() => {
            Some("riff runs the audit again.".to_owned())
        }
        Report::Audit => {
            let (_, at_ms) = r.audit.last.as_ref()?;
            Some(format!("The audit ran {} ago.", ago(*at_ms)))
        }
        Report::Log => None,
    }
}

#[cfg(test)]
mod tests;
