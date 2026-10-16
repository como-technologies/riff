//! `riff view`: the ratatui view of the riff daemon of this machine
//! (#720, 01M4Z60B81RTZT71PN5HV0GZHX, 01M501PA4MG5NYNK7FBBRRAT20).
//!
//! # Design
//!
//! The view asks the daemon for its status each [`LOOK_EVERY`]
//! ([`crate::daemon::status`]), and draws it with ratatui:
//!
//! ```text
//! riff daemon 1.3.1 06a0ae03be98, pid 4242, limit 3 sessions and a floor of 8 GB, 0 waiting
//!   SESSION                                 ITEM             STATE                AGE  LAST
//! > riff-lead-o-r                           lead             lead                 2h
//!   riff-issue-12-1a2b3c4d                  issue-12         works                5m   > Bash: just check
//!   riff-verify-issue-9-2b3c4d5e            verify-issue-9   ends                 1m   end: success, 8 turns
//!   brett@kadomony:riff#issue-14 (77e0aaaa) issue-14         busy                      tests
//!   mike@pangolin:riff#issue-15 (5b1e2a90)  issue-15         offline, seen 5m ago      docs
//! wave o/r: started issue-12
//! Up/Down select   Enter attach to the lead   t take over now   q quit
//! ```
//!
//! The rows of this machine come first: each session that the daemon
//! owns ([`Row::Here`]). Then come the sessions of each other daemon of
//! the riff of peers, from their `session` records ([`Row::Peer`],
//! [`crate::daemon::Status::riff_sessions`],
//! 01M519Z554SC3EAT69Q3M6JDAP): the name with the user, the host and the
//! short session ID, the claims, the state and the status. A session of
//! a daemon that is offline shows `offline`, with the time when its
//! daemon was last seen. A takeover works only for a session of this
//! machine: `t` on a row of another machine says so, and names its host.
//!
//! [`Screen`] holds the state of the view, and [`Screen::on_key`] turns
//! a key into an [`Action`]: both have no terminal, so a test checks
//! them, and draws the view on the `TestBackend` of ratatui. [`view`]
//! runs the loop with the terminal of the person.
//!
//! - The SESSION column is as wide as the longest name, so each name
//!   shows in full (01M51PD7T7451SR11W7JA7QW3X).
//! - Enter attaches to the lead ([`crate::lead::attach`]): the selected
//!   row when it is a lead, else the first lead. The view gives the
//!   terminal to `podman attach`, and takes it back after Ctrl-P Ctrl-Q
//!   or the end of the lead. The note under the rows says that the lead
//!   runs on, or that it ended ([`Screen::attached`]).
//! - `t` takes over the selected worker ([`crate::takeover`]). The view
//!   gives the terminal to `claude --resume`, and takes it back after
//!   `/exit`. The note under the rows says that the session goes on,
//!   or that it ended before the takeover ([`Screen::took_over`]).
//! - The keys come from a blocking task of tokio that ends after
//!   200 ms, so the async loop never blocks, and no read of the view
//!   takes a key from a takeover.
//! - `q`, Esc or Ctrl-C ends the view. The sessions go on.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Row as TableRow, Table, TableState};

use riff_core::wire::{SessionInfo, SessionState};

use crate::daemon::{self, Live, Phase, Status};
use crate::runtime::Runtime;
use crate::text;

/// How often the view asks the daemon for its status.
pub const LOOK_EVERY: Duration = Duration::from_secs(1);

/// A key of the person, as the view takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Select the row above.
    Up,
    /// Select the row below.
    Down,
    /// Attach to the lead.
    Enter,
    /// Take over the selected session.
    Takeover,
    /// End the view.
    Quit,
}

/// What the view does for a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Attach to the session with a terminal in this container.
    Attach(String),
    /// Take over the session in this container.
    Takeover(String),
    /// End the view.
    Quit,
}

/// A row of the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row<'a> {
    /// A session that the daemon of this machine owns.
    Here(&'a Live),
    /// A session of another daemon of the riff of peers, from its
    /// `session` records (01M519Z554SC3EAT69Q3M6JDAP).
    Peer(&'a SessionInfo),
}

impl Row<'_> {
    /// The name of the lead of this machine: a session with a terminal.
    fn lead_here(self) -> Option<String> {
        match self {
            Row::Here(live) if live.phase == Phase::Terminal => Some(live.name.clone()),
            Row::Here(_) | Row::Peer(_) => None,
        }
    }

    /// The cells of the row, with the ages at the time `now_ms`: the
    /// name, the item, the state, the age and the last step. A row of
    /// another machine has its claims as the item, and its status as the
    /// last step.
    fn cells(self, now_ms: i64) -> [String; 5] {
        match self {
            Row::Here(s) => {
                let age = u64::try_from((now_ms - s.since_ms) / 1000).unwrap_or(0);
                [
                    text::safe(&s.name),
                    text::safe(&s.item),
                    text::phase_word(s.phase).to_owned(),
                    text::ago(age),
                    text::safe(&s.last),
                ]
            }
            Row::Peer(s) => {
                let claims = s.uri.claims();
                let item = if claims.is_empty() && s.uri.lead() {
                    "lead".to_owned()
                } else {
                    claims.join(", ")
                };
                let state = match s.state.unwrap_or_default() {
                    SessionState::Offline => text::view_offline(s.idle_secs),
                    state => state.word().to_owned(),
                };
                let last = s.status.as_ref().map_or("", |status| &status.status.step);
                [
                    text::safe(&text::name(&s.uri)),
                    text::safe(&item),
                    state,
                    String::new(),
                    text::safe(last),
                ]
            }
        }
    }
}

/// The state of the view.
#[derive(Debug, Clone, Default)]
pub struct Screen {
    /// The last status of the daemon, or why no daemon answers.
    status: Option<Result<Status, String>>,
    /// The selected row of the sessions.
    selected: usize,
    /// A line for the person, after an action.
    note: Option<String>,
}

impl Screen {
    /// Takes the status of the daemon: `Err` says why no daemon answers.
    /// The selection stays in the rows.
    pub fn set(&mut self, status: Result<Status, String>) {
        self.status = Some(status);
        let rows = self.rows().len();
        self.selected = self.selected.min(rows.saturating_sub(1));
    }

    /// Shows `note` for the person, under the rows.
    pub fn note(&mut self, note: impl Into<String>) {
        self.note = Some(note.into());
    }

    /// Shows the end of the attach to the lead `name`
    /// (01M51PD7Q24N252V66NFEHY49N): a detach, the end of the lead, or
    /// the error of the attach.
    ///
    /// ```
    /// use riff::lead::Left;
    /// use riff::screen::Screen;
    ///
    /// let mut screen = Screen::default();
    /// screen.attached("n", &Ok(Left::Ended));
    /// assert_eq!(screen.shown_note(), Some(riff::text::lead_ended("n").as_str()));
    /// screen.attached("n", &Ok(Left::Detached));
    /// assert_eq!(screen.shown_note(), Some(riff::text::lead_detached("n").as_str()));
    /// ```
    pub fn attached(&mut self, name: &str, left: &Result<crate::lead::Left>) {
        self.note(match left {
            Ok(left) => left.line(name),
            Err(e) => format!("{e:#}"),
        });
    }

    /// Shows the end of the takeover of `name` with the exit code
    /// `taken` of [`crate::takeover::takeover`]
    /// (01M50RRQBZ8ASPT25DYPX78HP9): 0 when the session goes on, 1 when
    /// it ended before the takeover.
    ///
    /// ```
    /// use riff::screen::Screen;
    ///
    /// let mut screen = Screen::default();
    /// screen.took_over("n", &Ok(1));
    /// assert_eq!(screen.shown_note(), Some(riff::text::takeover_session_ended("n").as_str()));
    /// screen.took_over("n", &Ok(0));
    /// assert_eq!(screen.shown_note(), Some(riff::text::takeover_back("n").as_str()));
    /// ```
    pub fn took_over(&mut self, name: &str, taken: &Result<i32>) {
        self.note(match taken {
            Ok(0) => text::takeover_back(name),
            Ok(_) => text::takeover_session_ended(name),
            Err(e) => format!("{e:#}"),
        });
    }

    /// The note under the rows, if any.
    pub fn shown_note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// The rows of the view, in order. First the sessions of the daemon
    /// of this machine: each lead first, then the others, each by its
    /// name. Then each session of another daemon of the riff, by its
    /// user, its host and its name. A session of the riff that this
    /// daemon owns has only its row of this machine.
    pub fn rows(&self) -> Vec<Row<'_>> {
        let Some(Ok(status)) = &self.status else {
            return Vec::new();
        };
        let mut here: Vec<&Live> = status.sessions.iter().collect();
        here.sort_by_key(|s| (s.phase != Phase::Terminal, s.name.clone()));
        let owned = |s: &SessionInfo| {
            let id = s.uri.who().session();
            status
                .sessions
                .iter()
                .any(|live| Some(live.session.as_str()) == id)
        };
        let mut peers: Vec<&SessionInfo> =
            status.riff_sessions.iter().filter(|s| !owned(s)).collect();
        peers.sort_by_key(|s| {
            (
                s.uri.who().user().to_owned(),
                s.uri.place().host().to_owned(),
                text::name(&s.uri),
            )
        });
        let mut rows: Vec<Row> = here.into_iter().map(Row::Here).collect();
        rows.extend(peers.into_iter().map(Row::Peer));
        rows
    }

    /// The selected row.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Takes the key `key` of the person.
    ///
    /// ```
    /// use riff::screen::{Action, Key, Screen};
    ///
    /// let mut screen = Screen::default();
    /// assert_eq!(screen.on_key(Key::Quit), Some(Action::Quit));
    /// // No daemon: no session to attach to.
    /// assert_eq!(screen.on_key(Key::Enter), None);
    /// ```
    pub fn on_key(&mut self, key: Key) -> Option<Action> {
        let all = self.rows();
        let selected = all.get(self.selected).copied();
        let lead = selected
            .and_then(Row::lead_here)
            .or_else(|| all.iter().find_map(|row| row.lead_here()));
        let rows = all.len();
        let selected = selected.map(|row| match row {
            Row::Here(s) => Ok((s.name.clone(), s.phase)),
            Row::Peer(s) => Err(text::takeover_other_host(
                &text::name(&s.uri),
                s.uri.place().host(),
            )),
        });
        match key {
            Key::Quit => Some(Action::Quit),
            Key::Up => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            Key::Down => {
                if self.selected + 1 < rows {
                    self.selected += 1;
                }
                None
            }
            Key::Enter => match lead {
                Some(name) => Some(Action::Attach(name)),
                None => {
                    self.note(text::VIEW_NO_LEAD);
                    None
                }
            },
            Key::Takeover => match selected {
                Some(Ok((name, Phase::Terminal))) => {
                    self.note(text::view_no_takeover(&name));
                    None
                }
                Some(Ok((name, _))) => Some(Action::Takeover(name)),
                // Only a session of this machine
                // (01M519Z554SC3EAT69Q3M6JDAP).
                Some(Err(refused)) => {
                    self.note(refused);
                    None
                }
                None => None,
            },
        }
    }

    /// Draws the view on `frame`, with the ages at the time `now_ms`.
    pub fn draw(&self, frame: &mut Frame, now_ms: i64) {
        let waves = match &self.status {
            Some(Ok(status)) => status.waves.len() + usize::from(!status.waiting.is_empty()),
            _ => 0,
        };
        let [head, rows, tail, note, keys] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(2),
            Constraint::Length(u16::try_from(waves).unwrap_or(u16::MAX)),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        let head_line = match &self.status {
            None => String::new(),
            Some(Ok(status)) => text::view_daemon(status),
            Some(Err(error)) => text::view_no_daemon(error),
        };
        frame.render_widget(Paragraph::new(head_line), head);

        let all = self.rows();
        let cells: Vec<[String; 5]> = all.iter().map(|row| row.cells(now_ms)).collect();
        // The SESSION and the STATE columns are as wide as their longest
        // cell, so each name and each state shows in full.
        let wide = |col: usize, head: &str, least: usize| {
            let wide = cells
                .iter()
                .map(|row| row[col].chars().count())
                .chain([head.len(), least])
                .max()
                .unwrap_or_default();
            Constraint::Length(u16::try_from(wide).unwrap_or(u16::MAX))
        };
        let widths = [
            wide(0, "SESSION", 0),
            Constraint::Length(16),
            wide(2, "STATE", 20),
            Constraint::Length(4),
            Constraint::Fill(1),
        ];
        let table = Table::new(cells.into_iter().map(TableRow::new), widths)
            .header(
                TableRow::new(["SESSION", "ITEM", "STATE", "AGE", "LAST"])
                    .style(Style::new().add_modifier(Modifier::BOLD)),
            )
            .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");
        let mut state =
            TableState::default().with_selected((!all.is_empty()).then_some(self.selected));
        frame.render_stateful_widget(table, rows, &mut state);

        if let Some(Ok(status)) = &self.status {
            let mut lines: Vec<Line> = status
                .waves
                .iter()
                .map(|w| Line::from(text::view_wave(w)))
                .collect();
            if !status.waiting.is_empty() {
                lines.push(Line::from(text::view_waiting(&status.waiting)));
            }
            frame.render_widget(Paragraph::new(lines), tail);
        }
        if let Some(line) = &self.note {
            frame.render_widget(Paragraph::new(line.as_str()), note);
        }
        frame.render_widget(
            Paragraph::new(text::VIEW_KEYS).style(Style::new().add_modifier(Modifier::DIM)),
            keys,
        );
    }
}

/// The key of the person in `event`, or `None` for another event.
fn key_of(event: &ratatui::crossterm::event::Event) -> Option<Key> {
    use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
    let Event::Key(key) = event else {
        return None;
    };
    if key.kind != KeyEventKind::Press {
        return None;
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => Some(Key::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(Key::Down),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Char('t') => Some(Key::Takeover),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Key::Quit),
        KeyCode::Char('q') | KeyCode::Esc => Some(Key::Quit),
        _ => None,
    }
}

/// The next key of the person, or `None` when no key comes in
/// [`KEY_WAIT`]. The blocking reads of crossterm run in a blocking task
/// of tokio, not in the async loop of the view
/// (01M50RRQBZ8ASPT25DYPX78HP9). The task ends after [`KEY_WAIT`], so
/// no read of the view takes a key from a takeover or an attach.
async fn next_key() -> Result<Option<Key>> {
    use ratatui::crossterm::event;
    tokio::task::spawn_blocking(|| {
        if !event::poll(KEY_WAIT)? {
            return Ok(None);
        }
        Ok(key_of(&event::read()?))
    })
    .await?
}

/// How long [`next_key`] waits for a key.
const KEY_WAIT: Duration = Duration::from_millis(200);

/// `riff view`: shows the daemon in the folder `dir` until the person
/// ends the view. It attaches and takes over with `runtime`.
pub async fn view(dir: &Path, runtime: &impl Runtime) -> Result<()> {
    let mut screen = Screen::default();
    let mut terminal = ratatui::try_init()?;
    let mut next = tokio::time::Instant::now();
    let shown = loop {
        if tokio::time::Instant::now() >= next {
            screen.set(daemon::status(dir).await.map_err(|e| format!("{e:#}")));
            next = tokio::time::Instant::now() + LOOK_EVERY;
        }
        if let Err(e) = terminal.draw(|frame| screen.draw(frame, crate::run::now_ms())) {
            break Err(e.into());
        }
        let key = match next_key().await {
            Ok(key) => key,
            Err(e) => break Err(e),
        };
        let Some(action) = key.and_then(|key| screen.on_key(key)) else {
            continue;
        };
        // The terminal goes to the action, and comes back after it.
        ratatui::restore();
        match &action {
            Action::Quit => break Ok(()),
            Action::Attach(name) => {
                let left = crate::lead::attach(runtime, name).await;
                screen.attached(name, &left);
            }
            Action::Takeover(name) => {
                let taken =
                    crate::takeover::takeover(dir, runtime, name, false, |l| println!("{l}")).await;
                screen.took_over(name, &taken);
            }
        }
        terminal = match ratatui::try_init() {
            Ok(terminal) => terminal,
            Err(e) => return Err(e.into()),
        };
        next = tokio::time::Instant::now();
    };
    ratatui::restore();
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    const NOW: i64 = 10_000_000;

    fn live(name: &str, item: &str, phase: Phase, last: &str) -> Live {
        Live {
            name: name.into(),
            session: format!("{name}-session"),
            item: item.into(),
            image: "img".into(),
            since_ms: NOW - 300_000,
            phase,
            last: last.into(),
        }
    }

    fn status(sessions: Vec<Live>) -> Status {
        Status {
            pid: 4242,
            build: "1.3.1 06a0ae03be98".into(),
            since_ms: 0,
            dir: "/d".into(),
            waits: None,
            key: None,
            riff: None,
            owner: None,
            links: None,
            position: 0,
            sessions,
            riff_sessions: vec![],
            limit: Some(crate::room::Limit::default()),
            waiting: vec![],
            waves: vec![],
            jobs: 0,
        }
    }

    /// The text of the screen after a draw of `screen`, one line for
    /// each row of the terminal.
    fn drawn(screen: &Screen, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| screen.draw(frame, NOW)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    fn two() -> Screen {
        let mut screen = Screen::default();
        screen.set(Ok(status(vec![
            live(
                "riff-issue-12-1a2b3c4d",
                "issue-12",
                Phase::Works,
                "> Bash: just check",
            ),
            live("riff-lead-o-r", "lead", Phase::Terminal, ""),
        ])));
        screen
    }

    #[test]
    fn the_view_shows_each_session_with_its_item_its_state_its_age_and_its_last_step() {
        let lines = drawn(&two(), 110, 8);
        assert_eq!(
            lines[0],
            "riff daemon 1.3.1 06a0ae03be98, pid 4242, limit 3 sessions and a floor of 8 GB, 0 waiting"
        );
        assert!(
            lines[1].contains("SESSION") && lines[1].contains("LAST"),
            "{lines:#?}"
        );
        // The lead is first, and selected.
        assert!(lines[2].starts_with("> riff-lead-o-r"), "{lines:#?}");
        assert!(
            lines[2].contains("lead") && lines[2].contains("5m"),
            "{lines:#?}"
        );
        assert!(lines[3].contains("riff-issue-12-1a2b3c4d"), "{lines:#?}");
        assert!(lines[3].contains("issue-12"), "{lines:#?}");
        assert!(lines[3].contains("works"), "{lines:#?}");
        assert!(lines[3].contains("> Bash: just check"), "{lines:#?}");
        assert_eq!(lines[7], text::VIEW_KEYS);
    }

    /// 01M51PD7T7451SR11W7JA7QW3X
    #[test]
    fn the_view_shows_the_full_name_of_each_session() {
        let lead = "riff-lead-como-technologies-riff";
        let mut screen = Screen::default();
        screen.set(Ok(status(vec![
            live(
                "riff-verify-issue-123-1a2b3c4d",
                "verify-issue-123",
                Phase::Works,
                "",
            ),
            live(lead, "lead", Phase::Terminal, ""),
        ])));
        let lines = drawn(&screen, 110, 8);
        assert!(
            lines[2].starts_with(&format!("> {lead} lead")),
            "{lines:#?}"
        );
        assert!(
            lines[3].starts_with("  riff-verify-issue-123-1a2b3c4d   verify-issue-123"),
            "{lines:#?}"
        );
        // The next column starts after the longest name.
        assert_eq!(lines[1].find("ITEM"), Some(2 + lead.len() + 1));
    }

    /// The view of thelio in the riff of three daemons
    /// ([`crate::peer::talk::three_daemons`]): the lead and a worker of
    /// this machine, and the sessions of brett on kadomony (online) and
    /// of mike on pangolin (offline). The lead of this machine is in the
    /// riff of peers too.
    fn riff() -> Screen {
        let mut shown = status(vec![
            live("riff-lead-o-r", "lead", Phase::Terminal, ""),
            live(
                "riff-issue-12-1a2b3c4d",
                "issue-12",
                Phase::Works,
                "> Bash: just check",
            ),
        ]);
        shown.sessions[0].session = "1a2b3c4d-1".into();
        shown.riff_sessions = crate::peer::talk::three_daemons(NOW);
        let mut screen = Screen::default();
        screen.set(Ok(shown));
        screen
    }

    /// 01M519Z554SC3EAT69Q3M6JDAP
    #[test]
    fn the_view_shows_the_sessions_of_each_daemon_of_the_riff() {
        let lines = drawn(&riff(), 150, 10);
        assert!(lines[2].starts_with("> riff-lead-o-r "), "{lines:#?}");
        assert!(
            lines[3].starts_with("  riff-issue-12-1a2b3c4d "),
            "{lines:#?}"
        );
        // Each session of another daemon: the user and the host in its
        // name, its item, its state and its status.
        let brett = &lines[4];
        assert!(
            brett.starts_with("  brett@kadomony:riff#issue-14 (77e0aaaa) "),
            "{lines:#?}"
        );
        assert!(
            brett.contains(" issue-14 ") && brett.contains(" busy "),
            "{brett}"
        );
        assert!(brett.ends_with(" tests"), "{brett}");
        // The daemon of pangolin is offline: its link ended 5 minutes ago.
        let pangolin = &lines[5];
        assert!(
            pangolin.starts_with("  mike@pangolin:riff#issue-15 (5b1e2a90) "),
            "{lines:#?}"
        );
        assert!(pangolin.contains(" issue-15 "), "{pangolin}");
        assert!(pangolin.contains(" offline, seen 5m ago "), "{pangolin}");
        assert!(pangolin.ends_with(" docs"), "{pangolin}");
        // The lead of thelio in the riff of peers has no second row.
        assert!(
            !lines.iter().any(|l| l.contains("mike@thelio")),
            "{lines:#?}"
        );
        assert_eq!(lines[6], "");
        // Each name and each state shows in full: the columns start at
        // the same place on each row.
        let at = |line: &str, part: &str| line.find(part).unwrap();
        let item = at(&lines[1], "ITEM");
        assert_eq!(at(&lines[3], " issue-12 ") + 1, item);
        assert_eq!(at(brett, " issue-14 ") + 1, item);
        assert_eq!(at(pangolin, " issue-15 ") + 1, item);
        let state = at(&lines[1], "STATE");
        assert_eq!(at(brett, " busy ") + 1, state);
        assert_eq!(at(pangolin, " offline") + 1, state);
        assert!(
            at(&lines[1], "AGE") > at(pangolin, " seen 5m ago "),
            "{lines:#?}"
        );
    }

    /// 01M519Z554SC3EAT69Q3M6JDAP: a takeover works only for a session of
    /// this machine.
    #[test]
    fn a_takeover_of_a_session_of_another_machine_refuses_and_names_its_host() {
        let mut screen = riff();
        screen.on_key(Key::Down);
        screen.on_key(Key::Down);
        assert_eq!(screen.selected(), 2);
        assert_eq!(screen.on_key(Key::Takeover), None);
        let refused =
            text::takeover_other_host("brett@kadomony:riff#issue-14 (77e0aaaa)", "kadomony");
        assert!(refused.contains("runs on the host kadomony"), "{refused}");
        assert_eq!(screen.shown_note(), Some(refused.as_str()));
        assert_eq!(drawn(&screen, 150, 10)[8], refused);
        // The offline session of pangolin: the same, with its host.
        screen.on_key(Key::Down);
        assert_eq!(screen.on_key(Key::Takeover), None);
        assert!(
            screen
                .shown_note()
                .is_some_and(|note| note.contains("runs on the host pangolin")),
            "{:?}",
            screen.shown_note()
        );
        // Enter on a row of another machine attaches to the lead of this
        // machine. Down stops at the last row.
        assert_eq!(
            screen.on_key(Key::Enter),
            Some(Action::Attach("riff-lead-o-r".into()))
        );
        screen.on_key(Key::Down);
        assert_eq!(screen.selected(), 3);
        // A worker of this machine still takes a takeover.
        screen.on_key(Key::Up);
        screen.on_key(Key::Up);
        assert_eq!(
            screen.on_key(Key::Takeover),
            Some(Action::Takeover("riff-issue-12-1a2b3c4d".into()))
        );
    }

    #[test]
    fn the_view_shows_the_waves_the_starts_that_wait_and_a_note() {
        let mut screen = Screen::default();
        let mut shown = status(vec![live(
            "riff-issue-12-1a2b3c4d",
            "issue-12",
            Phase::Holds,
            "",
        )]);
        shown.waves = vec![daemon::WaveShown {
            repo: "o/r".into(),
            word: "started issue-12".into(),
        }];
        shown.waiting = vec!["riff-issue-13-2b3c4d5e".into()];
        screen.set(Ok(shown));
        screen.note("a note");
        let lines = drawn(&screen, 110, 9);
        assert!(lines[2].contains("holds for a takeover"), "{lines:#?}");
        assert_eq!(lines[5], "wave o/r: started issue-12");
        assert_eq!(
            lines[6],
            text::view_waiting(&["riff-issue-13-2b3c4d5e".into()])
        );
        assert_eq!(lines[7], "a note");
    }

    #[test]
    fn the_view_with_no_daemon_says_how_to_start_it() {
        let mut screen = Screen::default();
        screen.set(Err("no socket".into()));
        let lines = drawn(&screen, 110, 6);
        assert_eq!(lines[0], text::view_no_daemon("no socket"));
        assert_eq!(screen.on_key(Key::Takeover), None);
        assert_eq!(screen.on_key(Key::Enter), None);
        let lines = drawn(&screen, 110, 6);
        assert_eq!(lines[4], text::VIEW_NO_LEAD);
    }

    #[test]
    fn the_keys_select_a_row_attach_to_the_lead_and_take_over_a_worker() {
        let mut screen = two();
        // The lead is selected: Enter attaches to it, t does not take it
        // over.
        assert_eq!(
            screen.on_key(Key::Enter),
            Some(Action::Attach("riff-lead-o-r".into()))
        );
        assert_eq!(screen.on_key(Key::Takeover), None);
        assert_eq!(
            drawn(&screen, 110, 8)[6],
            text::view_no_takeover("riff-lead-o-r")
        );
        // Down selects the worker, and stops at the last row.
        assert_eq!(screen.on_key(Key::Down), None);
        assert_eq!(screen.on_key(Key::Down), None);
        assert_eq!(screen.selected(), 1);
        assert!(drawn(&screen, 110, 8)[3].starts_with("> riff-issue-12"));
        assert_eq!(
            screen.on_key(Key::Takeover),
            Some(Action::Takeover("riff-issue-12-1a2b3c4d".into()))
        );
        // Enter on a worker attaches to the lead.
        assert_eq!(
            screen.on_key(Key::Enter),
            Some(Action::Attach("riff-lead-o-r".into()))
        );
        assert_eq!(screen.on_key(Key::Up), None);
        assert_eq!(screen.on_key(Key::Up), None);
        assert_eq!(screen.selected(), 0);
        // A new status with fewer rows keeps the selection in them.
        screen.on_key(Key::Down);
        screen.set(Ok(status(vec![])));
        assert_eq!(screen.selected(), 0);
        assert_eq!(screen.on_key(Key::Quit), Some(Action::Quit));
    }

    /// 01M51PD7Q24N252V66NFEHY49N
    #[test]
    fn an_attach_says_that_the_lead_runs_on_or_that_it_ended() {
        use crate::lead::Left;
        let mut screen = two();
        screen.attached("riff-lead-o-r", &Ok(Left::Ended));
        assert_eq!(drawn(&screen, 110, 8)[6], text::lead_ended("riff-lead-o-r"));
        screen.attached("riff-lead-o-r", &Ok(Left::Detached));
        assert_eq!(
            drawn(&screen, 110, 8)[6],
            text::lead_detached("riff-lead-o-r")
        );
        screen.attached("riff-lead-o-r", &Err(anyhow::anyhow!("no podman")));
        assert_eq!(drawn(&screen, 110, 8)[6], "no podman");
    }

    #[test]
    fn a_takeover_of_a_session_that_ended_says_that_it_ended() {
        let mut screen = two();
        screen.took_over("riff-issue-12-1a2b3c4d", &Ok(1));
        assert_eq!(
            drawn(&screen, 110, 8)[6],
            text::takeover_session_ended("riff-issue-12-1a2b3c4d")
        );
        screen.took_over("riff-issue-12-1a2b3c4d", &Ok(0));
        assert_eq!(
            drawn(&screen, 110, 8)[6],
            text::takeover_back("riff-issue-12-1a2b3c4d")
        );
        screen.took_over("riff-issue-12-1a2b3c4d", &Err(anyhow::anyhow!("no daemon")));
        assert_eq!(drawn(&screen, 110, 8)[6], "no daemon");
    }

    #[test]
    fn a_key_of_the_terminal_is_a_key_of_the_view() {
        use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
        let press = |code, modifiers| Event::Key(KeyEvent::new(code, modifiers));
        let none = KeyModifiers::NONE;
        assert_eq!(key_of(&press(KeyCode::Up, none)), Some(Key::Up));
        assert_eq!(key_of(&press(KeyCode::Char('j'), none)), Some(Key::Down));
        assert_eq!(key_of(&press(KeyCode::Enter, none)), Some(Key::Enter));
        assert_eq!(
            key_of(&press(KeyCode::Char('t'), none)),
            Some(Key::Takeover)
        );
        assert_eq!(key_of(&press(KeyCode::Esc, none)), Some(Key::Quit));
        assert_eq!(
            key_of(&press(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Key::Quit)
        );
        assert_eq!(key_of(&press(KeyCode::Char('x'), none)), None);
        let mut release = KeyEvent::new(KeyCode::Char('q'), none);
        release.kind = KeyEventKind::Release;
        assert_eq!(key_of(&Event::Key(release)), None);
    }
}
