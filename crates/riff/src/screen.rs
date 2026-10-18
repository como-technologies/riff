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
//! wave o/r: started issue-12
//! Up/Down select   Enter attach to the lead   t take over now   q quit
//! ```
//!
//! Each row is a session that the daemon of this machine owns
//! ([`Row`]): a container session of the sessions model of
//! `riff-client` ([`Sessions`], 01M56Y8KD05JH2J7AMK636WGT7). The view
//! shows no session of another daemon (01M53NGW6F5HP1DZ9QRGP50MGT).
//!
//! `riff view --host HOST` ([`view_host`]) draws the same view from the
//! status of the daemon of HOST, through riff-server
//! (01M57D4FZGCVE62AP4MVG59HM4). Enter and `t` give a note there
//! ([`on_key_of_host`]).
//!
//! [`Screen`] holds the state of the view, and [`Screen::on_key`] turns
//! a key into an [`Action`]: both have no terminal, so a test checks
//! them, and draws the view on the `TestBackend` of ratatui. [`view`]
//! runs the loop with the terminal of the person.
//!
//! - The SESSION column is as wide as the longest name, so each name
//!   shows in full (01M51PD7T7451SR11W7JA7QW3X). The STATE column is at
//!   least 20 wide, and as wide as its longest state.
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

use crate::daemon::{self, Status, remote};
use crate::runtime::Runtime;
use crate::text;
use riff_client::api::Api;
use riff_client::sessions::{Live, Phase, Sessions};
use riff_core::name::SessionUri;
use riff_core::wire::SessionInfo;

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

/// A row of the view: a session that the daemon of this machine owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row<'a>(pub &'a Live);

impl Row<'_> {
    /// The name of the lead: a session with a terminal.
    fn lead_here(self) -> Option<String> {
        (self.0.phase == Phase::Terminal).then(|| self.0.name.clone())
    }

    /// The cells of the row, with the ages at the time `now_ms`: the
    /// name, the item, the state, the age and the last step.
    fn cells(self, now_ms: i64) -> [String; 5] {
        let s = self.0;
        let age = u64::try_from((now_ms - s.since_ms) / 1000).unwrap_or(0);
        [
            text::safe(&s.name),
            text::safe(&s.item),
            text::phase_word(s.phase).to_owned(),
            text::ago(age),
            text::safe(&s.last),
        ]
    }
}

/// The state of the view.
#[derive(Debug, Clone, Default)]
pub struct Screen {
    /// The last status of the daemon, or why no daemon answers.
    status: Option<Result<Status, String>>,
    /// The sessions model of the last status ([`Sessions`]): the view
    /// draws its container sessions.
    sessions: Sessions,
    /// The selected row of the sessions.
    selected: usize,
    /// A line for the person, after an action.
    note: Option<String>,
}

impl Screen {
    /// Takes the status of the daemon: `Err` says why no daemon answers.
    /// The selection stays in the rows. `riff view` asks no riff-server:
    /// the model has no session of the server (01M53NGW6F5HP1DZ9QRGP50MGT).
    pub fn set(&mut self, status: Result<Status, String>) {
        self.show(status, Vec::new());
    }

    /// Takes the status of the daemon and `server`, the sessions of
    /// riff-server, in one sessions model (01M56Y8KD05JH2J7AMK636WGT7).
    /// The view draws the container sessions of the model: a container
    /// session that is also on the server is one row.
    pub fn show(&mut self, status: Result<Status, String>, server: Vec<SessionInfo>) {
        let containers = status
            .as_ref()
            .map(|s| s.sessions.clone())
            .unwrap_or_default();
        self.sessions = Sessions::new(server, containers);
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

    /// The rows of the view, in order: the sessions of the daemon of
    /// this machine, each lead first, then the others, each by its name.
    pub fn rows(&self) -> Vec<Row<'_>> {
        let mut here: Vec<&Live> = self.sessions.containers().collect();
        here.sort_by_key(|s| (s.phase != Phase::Terminal, s.name.clone()));
        here.into_iter().map(Row).collect()
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
        let selected = selected.map(|Row(s)| (s.name.clone(), s.phase));
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
                Some((name, Phase::Terminal)) => {
                    self.note(text::view_no_takeover(&name));
                    None
                }
                Some((name, _)) => Some(Action::Takeover(name)),
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

/// The key of the person in the view of another host: Enter and `t`
/// work only on that host, so they give a note (01M57D4FZGCVE62AP4MVG59HM4).
///
/// ```
/// use riff::screen::{Action, Key, Screen, on_key_of_host};
///
/// let mut screen = Screen::default();
/// assert_eq!(on_key_of_host(&mut screen, Key::Quit, "thelio"), Some(Action::Quit));
/// assert_eq!(on_key_of_host(&mut screen, Key::Takeover, "thelio"), None);
/// assert_eq!(screen.shown_note(), Some(riff::text::view_on_host("thelio").as_str()));
/// ```
pub fn on_key_of_host(screen: &mut Screen, key: Key, host: &str) -> Option<Action> {
    match key {
        Key::Enter | Key::Takeover => {
            screen.note(text::view_on_host(host));
            None
        }
        key => screen.on_key(key),
    }
}

/// `riff view --host HOST`: shows the daemon of `host` through
/// riff-server until the person ends the view
/// (01M57D4FZGCVE62AP4MVG59HM4). A task asks the daemon for its status
/// each [`remote::VIEW_EVERY`], so a slow reply holds up no key.
pub async fn view_host(api: &Api, me: &SessionUri, host: &str) -> Result<()> {
    let (send, mut statuses) = tokio::sync::mpsc::channel(1);
    let (api, me, of) = (api.clone(), me.clone(), host.to_owned());
    let asks = tokio::spawn(async move {
        loop {
            let status = remote::state(&api, &me, &of).await;
            if send
                .send(status.map_err(|e| format!("{e:#}")))
                .await
                .is_err()
            {
                return;
            }
            tokio::time::sleep(remote::VIEW_EVERY).await;
        }
    });
    let mut screen = Screen::default();
    screen.note(text::view_of_host(host));
    let mut terminal = ratatui::try_init()?;
    let shown = loop {
        while let Ok(status) = statuses.try_recv() {
            screen.set(status);
        }
        if let Err(e) = terminal.draw(|frame| screen.draw(frame, crate::run::now_ms())) {
            break Err(e.into());
        }
        let key = match next_key().await {
            Ok(key) => key,
            Err(e) => break Err(e),
        };
        if let Some(Action::Quit) = key.and_then(|key| on_key_of_host(&mut screen, key, host)) {
            break Ok(());
        }
    };
    asks.abort();
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
            sessions,
            limit: Some(crate::room::Limit::default()),
            waiting: vec![],
            waves: vec![],
            jobs: 0,
            mcp: vec![],
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

    /// 01M56Y8KD05JH2J7AMK636WGT7: one sessions model holds a session of
    /// riff-server, a container session of the daemon, and one entry for
    /// a container session that is on riff-server too. `riff top --once`
    /// draws the sessions on the server and `riff view` the containers:
    /// each draws the shared session once.
    #[test]
    fn riff_top_and_riff_view_draw_one_sessions_model() {
        let on_server = |uri: &str, container: bool| -> SessionInfo {
            serde_json::from_value(serde_json::json!({
                "uri": uri, "live": true, "container": container, "state": "busy",
            }))
            .unwrap()
        };
        let in_container = |name: &str, item: &str, id: &str, phase: Phase| Live {
            session: id.into(),
            ..live(name, item, phase, "")
        };
        let server = vec![
            on_server("riff://mike@thelio/o/r?session=aaaa1111&lead=true", false),
            on_server(
                "riff://mike@thelio/o/r?session=bbbb2222&claim=issue-12",
                true,
            ),
        ];
        let containers = vec![
            in_container(
                "riff-issue-12-bbbb2222",
                "issue-12",
                "bbbb2222",
                Phase::Holds,
            ),
            in_container(
                "riff-issue-13-cccc3333",
                "issue-13",
                "cccc3333",
                Phase::Works,
            ),
        ];
        let mut screen = Screen::default();
        screen.show(Ok(status(containers)), server.clone());
        let model = &screen.sessions;
        let ids: Vec<Option<&str>> = model.entries.iter().map(|e| e.id()).collect();
        assert_eq!(ids, [Some("aaaa1111"), Some("bbbb2222"), Some("cccc3333")]);
        assert_eq!(model.entries[1].phase(), Some(Phase::Holds));
        assert!(model.entries[1].server.is_some());

        // `riff view`: the two containers, one row each.
        let lines = drawn(&screen, 110, 8);
        let count = |text: &str| lines.iter().filter(|l| l.contains(text)).count();
        assert_eq!(count("riff-issue-12-bbbb2222"), 1, "{lines:#?}");
        assert_eq!(count("riff-issue-13-cccc3333"), 1, "{lines:#?}");
        assert_eq!(count("aaaa1111"), 0, "{lines:#?}");

        // `riff top --once`: the two sessions on the server, one row each,
        // as with no container session.
        let top_of = |sessions: &Sessions| {
            let running = riff_core::wire::RiffState::Running.into();
            let top = crate::top::Top {
                pauses: &running,
                owner: &riff_core::wire::RiffOwner::NoSignIn,
                server: None,
                sessions,
                people: &[],
                issues: &Default::default(),
                repo: None,
                show: &Default::default(),
                width: 100,
                fault: None,
                machines: &[],
            };
            anstream::adapter::strip_str(&top.view()).to_string()
        };
        let text = top_of(model);
        assert_eq!(text.matches("aaaa1111").count(), 1, "{text}");
        assert_eq!(text.matches("bbbb2222").count(), 1, "{text}");
        assert!(!text.contains("cccc3333"), "{text}");
        assert_eq!(text, top_of(&Sessions::new(server, Vec::new())));
    }

    /// 01M53NGW6F5HP1DZ9QRGP50MGT: the view shows no session of another
    /// daemon, also when the status of a daemon of riff 1.6 to 1.8 has
    /// the sessions of its riff of peers.
    #[test]
    fn the_view_shows_no_session_of_another_daemon() {
        let mut old = serde_json::to_value(status(vec![live(
            "riff-issue-12-1a2b3c4d",
            "issue-12",
            Phase::Works,
            "",
        )]))
        .unwrap();
        old["key"] = "0a1b".into();
        old["riff_sessions"] = serde_json::json!([
            {"uri": "riff://brett@kadomony/o/riff?session=77e0aaaa-1&claim=issue-14#issue-14", "live": true},
            {"uri": "riff://mike@pangolin/o/riff?session=5b1e2a90-1&claim=issue-15#issue-15", "live": false},
        ]);
        let mut screen = Screen::default();
        screen.set(Ok(serde_json::from_value(old).unwrap()));
        assert_eq!(screen.rows().len(), 1);
        let lines = drawn(&screen, 150, 8);
        assert!(
            lines[2].starts_with("> riff-issue-12-1a2b3c4d "),
            "{lines:#?}"
        );
        for other in [
            "brett", "kadomony", "issue-14", "pangolin", "issue-15", "offline",
        ] {
            assert!(
                !lines.iter().any(|l| l.contains(other)),
                "{other}: {lines:#?}"
            );
        }
        screen.on_key(Key::Down);
        assert_eq!(screen.selected(), 0);
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
