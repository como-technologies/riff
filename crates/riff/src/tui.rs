//! The TUI of riff: `riff` with no command, in a terminal (#994, #1636,
//! 01M6VAZ7WH3MFAYF4PAZW6EDVY).
//!
//! # Design
//!
//! The TUI is a ratatui app with six tabs ([`Tab`],
//! 01M6VAZJYYQZDHD6YDB0NBQ7KW). Each tab has the same five parts
//! (01M6VAZK1R932VQMJBC32MTTJB):
//!
//! ```text
//! [1 Sessions]  2 Chat•2  3 Waves•  4 Machines  5 Devices  6 Reports
//!  riff running · owner mike · build 10df8a4 · link ok 2s
//!  the body of the tab
//!
//!  the note: the fault of the link, or the result of the last action
//!  1-6 tab  r read again  ? help  q quit
//! ```
//!
//! - The strip shows the active tab in `[ ]`, and the news of a tab
//!   ([`News`]) after its name. The news of a tab goes when you open it.
//! - The header ([`view::compute_view`], 01M6VAZK4RQSZ0MG2DHWQC598Q)
//!   shows the state of the whole riff, the owner, the build of this
//!   riff, and the link with the age of the last good look.
//! - The note line shows the fault of the link, else the result of the
//!   last action. The key line shows the keys of the tab, and `?` shows
//!   them all in a popup (01M6VAZKDEQ6951KGPGAMF1SNJ).
//! - The body of a tab comes with its build item: B #1637 Sessions,
//!   C #1638 Waves, D #1639 Machines, E #1640 Chat, F #1641 Reports, G
//!   #1642 Devices. Until then, the body names the commands of the CLI
//!   that show its area ([`Tab::today`], 01M6VAZKVK7530JJ63S7RVF88J).
//!
//! The render is pure (01M6VAZKNXZV1RTG7MT0GMHKT3). [`State`] holds what
//! the TUI knows: the last look of riff-server, the fault of the link,
//! the note and the choices of the person. [`view::compute_view`] makes
//! the [`view::View`] from the state, the size and the time: its rows,
//! their places, and the places that a click hits. [`view::render`]
//! draws the view on a ratatui `Buffer`. So a screen test draws a tab
//! into a `Buffer` and compares the rows, with no terminal.
//!
//! ```mermaid
//! flowchart LR
//!     S["riff-server<br/>riff, who"] -- "a look, or a fault" --> ST["State"]
//!     K["a key or a click"] --> ST
//!     ST -- "&State, the size, the time" --> V["compute_view<br/>pure: the rows and the hits"]
//!     V -- "&View" --> R["render<br/>pure: draws on a Buffer"]
//!     ST -- "an Action" --> E["the loop: quit, or look again"]
//! ```
//!
//! # The looks
//!
//! [`run`] looks at riff-server with the calls `riff` and `who`
//! ([`look`]) each [`LOOK_EVERY`], and each [`IDLE_LOOK_EVERY`] after
//! [`IDLE_AFTER`] with no key and no click ([`every`],
//! 01M6VAZK7N636Q0JFSV553A9ES). `r` looks again at once. A look runs in
//! a task of its own, so a slow look holds up no key.
//!
//! A look that fails does what it does in `riff top`
//! (01M6VAZKAH6B53YKR8W4YKNSH6). The first look runs before the TUI
//! takes the terminal: its error ends the TUI as in the CLI, so a
//! person who is not signed in sees the same line. After a good look,
//! a fault that a new try can repair ([`riff_client::api::passes`])
//! keeps the last screen, puts `link down` in the header and the fault
//! in the note line ([`State::failed`]). The fault goes at the next good
//! look. Another fault ends the TUI with the error.
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Ok: the first look is good
//!     [*] --> [*]: the first look fails, the error
//!     Ok --> Ok: a good look
//!     Ok --> Down: a look fails, a new try can repair it
//!     Down --> Ok: a good look, the fault goes
//!     Ok --> [*]: q, Ctrl-C, or another fault
//!     Down --> [*]: q, Ctrl-C, or another fault
//! ```
//!
//! # The keys and the mouse
//!
//! [`State::on_key`] turns a [`Key`] into an [`Action`]. The mouse does
//! the same as the keys (01M6VAZKG8FS4HEA9VSA4Z4YF1): the view has a
//! [`view::Hit`] for each place that a click hits, with its key, and
//! [`State::on_click`] presses that key. The setting `tui.mouse` turns
//! the mouse off ([`crate::settings::tui_mouse`]).
//!
//! | Key | Does |
//! |---|---|
//! | `1` to `6`, also with Alt | Open a tab. |
//! | `Tab`, `Shift-Tab` | Open the next tab, or the tab before. |
//! | `r` | Look again now. |
//! | `?` | Show or hide the keys of this tab. |
//! | `Esc` | Close the popup. |
//! | `q`, `Ctrl-C` | End the TUI. The sessions go on. |
//!
//! # The marks
//!
//! A session of a tab gets a [`Mark`] (01M6VAZKRTVZTY73PQV5GE2WY6): a
//! small fixed set, from the state that riff-server derives and from
//! the phase of a container session. The worst mark comes first.

pub mod view;

use std::time::Duration;

use anyhow::Result;
use ratatui::style::Color;
use riff_client::api::{self, Api};
use riff_client::sessions::Phase;
use riff_core::build::Build;
use riff_core::name::SessionUri;
use riff_core::wire::{RiffOwner, RiffReply, RiffState, SessionState, WhoReply};

use crate::text;

/// The time between two looks of riff-server.
pub const LOOK_EVERY: Duration = Duration::from_secs(3);

/// The time between two looks after [`IDLE_AFTER`] with no key and no
/// click.
pub const IDLE_LOOK_EVERY: Duration = Duration::from_secs(15);

/// The time with no key and no click after which the TUI looks each
/// [`IDLE_LOOK_EVERY`].
pub const IDLE_AFTER: Duration = Duration::from_secs(120);

/// The time between two looks when the last key or click was `idle`
/// ago (01M6VAZK7N636Q0JFSV553A9ES).
///
/// ```
/// use std::time::Duration;
/// use riff::tui::{IDLE_LOOK_EVERY, LOOK_EVERY, every};
///
/// assert_eq!(every(Duration::from_secs(5)), LOOK_EVERY);
/// assert_eq!(every(Duration::from_secs(119)), LOOK_EVERY);
/// assert_eq!(every(Duration::from_secs(120)), IDLE_LOOK_EVERY);
/// ```
pub fn every(idle: Duration) -> Duration {
    if idle >= IDLE_AFTER {
        IDLE_LOOK_EVERY
    } else {
        LOOK_EVERY
    }
}

/// A tab of the TUI, in the order of the strip
/// (01M6VAZJYYQZDHD6YDB0NBQ7KW).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tab {
    /// Each session and each worker.
    #[default]
    Sessions,
    /// The thread `chat`.
    Chat,
    /// The items, the claims, the pull requests and the verifies.
    Waves,
    /// The numbers of each host, the link and riff-server.
    Machines,
    /// The devices of the person.
    Devices,
    /// The usage, the audit and the log of a session.
    Reports,
}

impl Tab {
    /// Each tab, in the order of the strip.
    pub const ALL: [Tab; 6] = [
        Tab::Sessions,
        Tab::Chat,
        Tab::Waves,
        Tab::Machines,
        Tab::Devices,
        Tab::Reports,
    ];

    /// The name of the tab in the strip.
    pub fn name(self) -> &'static str {
        match self {
            Tab::Sessions => "Sessions",
            Tab::Chat => "Chat",
            Tab::Waves => "Waves",
            Tab::Machines => "Machines",
            Tab::Devices => "Devices",
            Tab::Reports => "Reports",
        }
    }

    /// The number of the tab: its key, from 1 to 6.
    pub fn number(self) -> usize {
        self as usize + 1
    }

    /// The tab of the key `number`, from 1 to 6.
    ///
    /// ```
    /// use riff::tui::Tab;
    ///
    /// assert_eq!(Tab::of_number(1), Some(Tab::Sessions));
    /// assert_eq!(Tab::of_number(6), Some(Tab::Reports));
    /// assert_eq!(Tab::of_number(0), None);
    /// assert_eq!(Tab::of_number(7), None);
    /// for tab in Tab::ALL {
    ///     assert_eq!(Tab::of_number(tab.number()), Some(tab));
    /// }
    /// ```
    pub fn of_number(number: usize) -> Option<Tab> {
        Tab::ALL.get(number.checked_sub(1)?).copied()
    }

    /// The tab after this one. The last tab comes before the first.
    ///
    /// ```
    /// use riff::tui::Tab;
    ///
    /// assert_eq!(Tab::Sessions.next(), Tab::Chat);
    /// assert_eq!(Tab::Reports.next(), Tab::Sessions);
    /// assert_eq!(Tab::Sessions.before(), Tab::Reports);
    /// assert_eq!(Tab::Chat.before(), Tab::Sessions);
    /// ```
    pub fn next(self) -> Tab {
        Tab::ALL[self.number() % Tab::ALL.len()]
    }

    /// The tab before this one. The first tab comes after the last.
    pub fn before(self) -> Tab {
        Tab::ALL[(self as usize + Tab::ALL.len() - 1) % Tab::ALL.len()]
    }

    /// The commands of the CLI that show the area of the tab, until its
    /// build item merges (01M6VAZKVK7530JJ63S7RVF88J). The Devices tab
    /// has none: it needs the device commands.
    pub fn today(self) -> &'static [&'static str] {
        match self {
            Tab::Sessions => &["riff top", "riff who"],
            Tab::Chat => &["riff chat"],
            Tab::Waves => &["riff top", "riff wave list"],
            Tab::Machines => &["riff top", "riff daemon status"],
            Tab::Devices => &[],
            Tab::Reports => &["riff usage", "riff audit", "riff daemon events NAME"],
        }
    }
}

/// The news of a tab since the person looked at it: a dot after the
/// name of the tab in the strip (01M6VAZK1R932VQMJBC32MTTJB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum News {
    /// Something is new: `•`.
    Dot,
    /// N new lines: `•N`.
    Lines(u32),
}

impl News {
    /// The text after the name of the tab.
    ///
    /// ```
    /// use riff::tui::News;
    ///
    /// assert_eq!(News::Dot.text(), "•");
    /// assert_eq!(News::Lines(2).text(), "•2");
    /// ```
    pub fn text(self) -> String {
        match self {
            News::Dot => "•".to_owned(),
            News::Lines(n) => format!("•{n}"),
        }
    }
}

/// The mark of a session (01M6VAZKRTVZTY73PQV5GE2WY6). The order is the
/// order of the rows: the worst mark first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mark {
    /// `!`: the state `blocked`.
    Blocked,
    /// `✓`: the phase `ends`: the turn gave its result.
    Done,
    /// `◐`: the state `waiting`: a verify, a merge, or an item of `Needs:`.
    Waiting,
    /// `●`: the state `busy`, or the phase `works`.
    Busy,
    /// `○`: the state `idle`.
    Idle,
}

impl Mark {
    /// The mark of a session in the state `state` that riff-server
    /// derives, with the phase `phase` of its container. `None` for a
    /// session that shows a word, not a mark: `offline`, `paused` or
    /// `must clear`, and a container with no state and no mark of its
    /// phase. A blocked session gets [`Mark::Blocked`] in each phase.
    ///
    /// ```
    /// use riff::tui::Mark;
    /// use riff_client::sessions::Phase;
    /// use riff_core::wire::SessionState;
    ///
    /// assert_eq!(Mark::of(Some(SessionState::Blocked), Some(Phase::Works)), Some(Mark::Blocked));
    /// assert_eq!(Mark::of(Some(SessionState::Busy), Some(Phase::Ends)), Some(Mark::Done));
    /// assert_eq!(Mark::of(Some(SessionState::Idle), Some(Phase::Works)), Some(Mark::Busy));
    /// assert_eq!(Mark::of(None, Some(Phase::Works)), Some(Mark::Busy));
    /// assert_eq!(Mark::of(Some(SessionState::Busy), None), Some(Mark::Busy));
    /// assert_eq!(Mark::of(Some(SessionState::Waiting), None), Some(Mark::Waiting));
    /// assert_eq!(Mark::of(Some(SessionState::Idle), Some(Phase::Terminal)), Some(Mark::Idle));
    /// assert_eq!(Mark::of(Some(SessionState::Offline), None), None);
    /// assert_eq!(Mark::of(Some(SessionState::Paused), None), None);
    /// assert_eq!(Mark::of(Some(SessionState::MustClear), None), None);
    /// assert_eq!(Mark::of(None, Some(Phase::Holds)), None);
    /// ```
    pub fn of(state: Option<SessionState>, phase: Option<Phase>) -> Option<Mark> {
        match (state, phase) {
            (Some(SessionState::Blocked), _) => Some(Mark::Blocked),
            (_, Some(Phase::Ends)) => Some(Mark::Done),
            (_, Some(Phase::Works)) | (Some(SessionState::Busy), _) => Some(Mark::Busy),
            (Some(SessionState::Waiting), _) => Some(Mark::Waiting),
            (Some(SessionState::Idle), _) => Some(Mark::Idle),
            _ => None,
        }
    }

    /// The symbol of the mark.
    ///
    /// ```
    /// use riff::tui::Mark;
    ///
    /// let marks = [Mark::Blocked, Mark::Done, Mark::Waiting, Mark::Busy, Mark::Idle];
    /// let symbols: Vec<&str> = marks.iter().map(|m| m.symbol()).collect();
    /// assert_eq!(symbols, ["!", "✓", "◐", "●", "○"]);
    /// // The worst mark comes first.
    /// let mut sorted = marks;
    /// sorted.reverse();
    /// sorted.sort();
    /// assert_eq!(sorted, marks);
    /// ```
    pub fn symbol(self) -> &'static str {
        match self {
            Mark::Blocked => "!",
            Mark::Done => "✓",
            Mark::Waiting => "◐",
            Mark::Busy => "●",
            Mark::Idle => "○",
        }
    }

    /// The word of the mark, as a group row counts it.
    pub fn word(self) -> &'static str {
        match self {
            Mark::Blocked => "blocked",
            Mark::Done => "done",
            Mark::Waiting => "waiting",
            Mark::Busy => "busy",
            Mark::Idle => "idle",
        }
    }

    /// The color of the mark. The idle mark is dim: its style, not a
    /// color ([`Mark::style`]).
    pub fn style(self) -> ratatui::style::Style {
        use ratatui::style::{Modifier, Style};
        match self {
            Mark::Blocked => Style::new().fg(Color::Red),
            Mark::Busy => Style::new().fg(Color::Green),
            Mark::Waiting | Mark::Done => Style::new().fg(Color::Cyan),
            Mark::Idle => Style::new().add_modifier(Modifier::DIM),
        }
    }
}

/// A key of the person, as the frame takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Open a tab: `1` to `6`, also with Alt.
    Open(Tab),
    /// Open the next tab: `Tab`.
    Next,
    /// Open the tab before: `Shift-Tab`.
    Before,
    /// Show or hide the keys of the tab: `?`.
    Help,
    /// Close the popup: `Esc`.
    Esc,
    /// Look again now: `r`.
    Again,
    /// End the TUI: `q` or `Ctrl-C`.
    Quit,
}

/// What the loop of the TUI does for a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Look at riff-server again now.
    Look,
    /// End the TUI. The sessions go on.
    Quit,
}

/// The facts of one good look of riff-server ([`look`]).
#[derive(Debug, Clone)]
pub struct Look {
    /// The reply of the call `riff`: the pauses.
    pub pauses: RiffReply,
    /// The reply of the call `who`: the sessions, the owner, the people.
    pub who: WhoReply,
    /// The build of riff-server, from the header of its last reply.
    pub server: Option<Build>,
}

/// The look of a riff that runs, with no sign-in and no session.
impl Default for Look {
    fn default() -> Look {
        Look {
            pauses: RiffState::Running.into(),
            who: WhoReply {
                sessions: Vec::new(),
                owner: RiffOwner::default(),
                people: Vec::new(),
                repos: Vec::new(),
            },
            server: None,
        }
    }
}

/// Looks at riff-server as `me`: the calls `riff` and `who`
/// (01M6VAZK7N636Q0JFSV553A9ES). Each call has the budget of a short
/// command, as in `riff top`.
pub async fn look(api: &Api, me: &SessionUri) -> Result<Look> {
    let pauses = api.pauses(me).await?;
    let who = api.roster(me, false).await?;
    Ok(Look {
        pauses,
        who,
        server: riff_client::link::server_build(),
    })
}

/// The state of the TUI: what it knows, and the choices of the person.
#[derive(Debug, Clone)]
pub struct State {
    /// The tab that the person sees.
    tab: Tab,
    /// The news of each tab, by the number of the tab less one.
    news: [Option<News>; 6],
    /// The last good look, and its time in milliseconds since the epoch.
    look: Option<(Look, i64)>,
    /// The fault of the last look, while the screen is the one of the
    /// last good look.
    fault: Option<String>,
    /// The result of the last action.
    note: Option<String>,
    /// True while the help popup shows.
    help: bool,
    /// The build of this riff.
    build: Build,
}

impl State {
    /// The state at the start: the Sessions tab, no look yet, and
    /// `build`, the build of this riff, for the header.
    pub fn new(build: Build) -> State {
        State {
            tab: Tab::default(),
            news: [None; 6],
            look: None,
            fault: None,
            note: None,
            help: false,
            build,
        }
    }

    /// Takes the good look `look` at the time `at_ms`. The fault of the
    /// link goes (01M6VAZKAH6B53YKR8W4YKNSH6).
    pub fn looked(&mut self, look: Look, at_ms: i64) {
        self.look = Some((look, at_ms));
        self.fault = None;
    }

    /// Takes the fault of a look that failed: the screen keeps the last
    /// good look, the header says `link down`, and the note line shows
    /// `fault` until the next good look (01M6VAZKAH6B53YKR8W4YKNSH6).
    ///
    /// ```
    /// use riff::tui::{Look, State};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this());
    /// state.looked(Look::default(), 0);
    /// state.failed("riff: no good look since 21:35:07: cannot reach riff-server");
    /// assert!(state.link_down());
    /// assert_eq!(state.shown_note(), Some("riff: no good look since 21:35:07: cannot reach riff-server"));
    /// state.looked(Look::default(), 3000);
    /// assert!(!state.link_down());
    /// assert_eq!(state.shown_note(), None);
    /// ```
    pub fn failed(&mut self, fault: impl Into<String>) {
        self.fault = Some(fault.into());
    }

    /// Shows `note` in the note line: the result of an action.
    pub fn note(&mut self, note: impl Into<String>) {
        self.note = Some(note.into());
    }

    /// Gives the tab `tab` the news `news`. The tab that the person sees
    /// gets none: the person sees it.
    ///
    /// ```
    /// use riff::tui::{Key, News, State, Tab};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this());
    /// state.news(Tab::Chat, News::Lines(2));
    /// state.news(Tab::Sessions, News::Dot);
    /// assert_eq!(state.news_of(Tab::Chat), Some(News::Lines(2)));
    /// assert_eq!(state.news_of(Tab::Sessions), None);
    /// // The news of a tab goes when the person opens it.
    /// state.on_key(Key::Open(Tab::Chat));
    /// assert_eq!(state.news_of(Tab::Chat), None);
    /// ```
    pub fn news(&mut self, tab: Tab, news: News) {
        if tab != self.tab {
            self.news[tab as usize] = Some(news);
        }
    }

    /// The news of the tab `tab`.
    pub fn news_of(&self, tab: Tab) -> Option<News> {
        self.news[tab as usize]
    }

    /// The tab that the person sees.
    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// The last good look and its time, if any.
    pub fn last_look(&self) -> Option<(&Look, i64)> {
        self.look.as_ref().map(|(look, at)| (look, *at))
    }

    /// True while the link is down: the last look failed.
    pub fn link_down(&self) -> bool {
        self.fault.is_some() || self.look.is_none()
    }

    /// The build of this riff.
    pub fn build(&self) -> &Build {
        &self.build
    }

    /// The fault of the last look, while the link is down.
    pub fn fault(&self) -> Option<&str> {
        self.fault.as_deref()
    }

    /// The text of the note line: the fault of the link, else the note
    /// of the last action (01M6VAZKDEQ6951KGPGAMF1SNJ).
    pub fn shown_note(&self) -> Option<&str> {
        self.fault.as_deref().or(self.note.as_deref())
    }

    /// True while the help popup shows.
    pub fn help_shown(&self) -> bool {
        self.help
    }

    /// Opens the tab `tab`: its news goes.
    fn open(&mut self, tab: Tab) {
        self.tab = tab;
        self.news[tab as usize] = None;
    }

    /// Takes the key `key` of the person (01M6VAZJYYQZDHD6YDB0NBQ7KW,
    /// 01M6VAZKDEQ6951KGPGAMF1SNJ).
    ///
    /// ```
    /// use riff::tui::{Action, Key, State, Tab};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this());
    /// assert_eq!(state.on_key(Key::Open(Tab::Waves)), None);
    /// assert_eq!(state.tab(), Tab::Waves);
    /// state.on_key(Key::Next);
    /// assert_eq!(state.tab(), Tab::Machines);
    /// state.on_key(Key::Before);
    /// state.on_key(Key::Before);
    /// assert_eq!(state.tab(), Tab::Chat);
    /// // `?` shows the help, and `?` or Esc closes it.
    /// state.on_key(Key::Help);
    /// assert!(state.help_shown());
    /// state.on_key(Key::Esc);
    /// assert!(!state.help_shown());
    /// assert_eq!(state.on_key(Key::Again), Some(Action::Look));
    /// assert_eq!(state.on_key(Key::Quit), Some(Action::Quit));
    /// ```
    pub fn on_key(&mut self, key: Key) -> Option<Action> {
        match key {
            Key::Open(tab) => {
                self.help = false;
                self.open(tab);
                None
            }
            Key::Next => {
                self.help = false;
                self.open(self.tab.next());
                None
            }
            Key::Before => {
                self.help = false;
                self.open(self.tab.before());
                None
            }
            Key::Help => {
                self.help = !self.help;
                None
            }
            Key::Esc => {
                self.help = false;
                None
            }
            Key::Again => Some(Action::Look),
            Key::Quit => Some(Action::Quit),
        }
    }

    /// Takes a click of the person at `column` and `row` on the view
    /// `view`: it presses the key of the place that it hits
    /// (01M6VAZKG8FS4HEA9VSA4Z4YF1). A click on no place does nothing.
    pub fn on_click(&mut self, view: &view::View, column: u16, row: u16) -> Option<Action> {
        self.on_key(view.hit(column, row)?)
    }
}

/// What the person does: a key, or a click of the mouse at a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// A key of the frame.
    Key(Key),
    /// A click of the left button at a column and a row.
    Click { column: u16, row: u16 },
}

/// The input of the person in `event`, or `None` for another event.
///
/// ```
/// use ratatui::crossterm::event::{
///     Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
/// };
/// use riff::tui::{Input, Key, Tab, input_of};
///
/// let press = |code, modifiers| Event::Key(KeyEvent::new(code, modifiers));
/// let none = KeyModifiers::NONE;
/// assert_eq!(input_of(&press(KeyCode::Char('3'), none)), Some(Input::Key(Key::Open(Tab::Waves))));
/// assert_eq!(
///     input_of(&press(KeyCode::Char('6'), KeyModifiers::ALT)),
///     Some(Input::Key(Key::Open(Tab::Reports)))
/// );
/// assert_eq!(input_of(&press(KeyCode::Tab, none)), Some(Input::Key(Key::Next)));
/// assert_eq!(input_of(&press(KeyCode::BackTab, KeyModifiers::SHIFT)), Some(Input::Key(Key::Before)));
/// assert_eq!(input_of(&press(KeyCode::Char('?'), KeyModifiers::SHIFT)), Some(Input::Key(Key::Help)));
/// assert_eq!(input_of(&press(KeyCode::Esc, none)), Some(Input::Key(Key::Esc)));
/// assert_eq!(input_of(&press(KeyCode::Char('r'), none)), Some(Input::Key(Key::Again)));
/// assert_eq!(input_of(&press(KeyCode::Char('q'), none)), Some(Input::Key(Key::Quit)));
/// assert_eq!(
///     input_of(&press(KeyCode::Char('c'), KeyModifiers::CONTROL)),
///     Some(Input::Key(Key::Quit))
/// );
/// assert_eq!(input_of(&press(KeyCode::Char('7'), none)), None);
/// assert_eq!(input_of(&press(KeyCode::Char('x'), none)), None);
/// let mouse = |kind| {
///     Event::Mouse(MouseEvent { kind, column: 4, row: 0, modifiers: KeyModifiers::NONE })
/// };
/// assert_eq!(
///     input_of(&mouse(MouseEventKind::Down(MouseButton::Left))),
///     Some(Input::Click { column: 4, row: 0 })
/// );
/// assert_eq!(input_of(&mouse(MouseEventKind::Moved)), None);
/// ```
pub fn input_of(event: &ratatui::crossterm::event::Event) -> Option<Input> {
    use ratatui::crossterm::event::{
        Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
    };
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            let key = match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Key::Quit,
                KeyCode::Char(digit @ '1'..='6') => {
                    Key::Open(Tab::of_number(digit.to_digit(10)? as usize)?)
                }
                KeyCode::Tab => Key::Next,
                KeyCode::BackTab => Key::Before,
                KeyCode::Char('?') => Key::Help,
                KeyCode::Esc => Key::Esc,
                KeyCode::Char('r') => Key::Again,
                KeyCode::Char('q') => Key::Quit,
                _ => return None,
            };
            Some(Input::Key(key))
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            Some(Input::Click {
                column: mouse.column,
                row: mouse.row,
            })
        }
        _ => None,
    }
}

/// How long [`next_input`] waits for a key or a click. The age of the
/// look in the header draws again after each wait.
const INPUT_WAIT: Duration = Duration::from_millis(200);

/// The next input of the person, or `None` when none comes in
/// [`INPUT_WAIT`]. The blocking reads of crossterm run in a blocking
/// task of tokio, not in the async loop, as in `riff view`.
async fn next_input() -> Result<Option<Input>> {
    use ratatui::crossterm::event;
    tokio::task::spawn_blocking(|| {
        if !event::poll(INPUT_WAIT)? {
            return Ok(None);
        }
        Ok(input_of(&event::read()?))
    })
    .await?
}

/// `riff` with no command, in a terminal: the TUI, as `me` at `api`,
/// until the person ends it (01M6VAZ7WH3MFAYF4PAZW6EDVY). With `mouse`,
/// the TUI takes the mouse (01M6VAZKG8FS4HEA9VSA4Z4YF1).
pub async fn run(api: &Api, me: &SessionUri, mouse: bool) -> Result<()> {
    use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
    use ratatui::crossterm::execute;
    // The first look runs before the TUI takes the terminal: its error
    // prints as in the CLI (01M6VAZKAH6B53YKR8W4YKNSH6).
    let first = look(api, me).await?;
    let mut state = State::new(Build::this());
    state.looked(first, crate::run::now_ms());
    let mut terminal = ratatui::try_init()?;
    if mouse && let Err(e) = execute!(std::io::stdout(), EnableMouseCapture) {
        ratatui::restore();
        return Err(e.into());
    }
    let shown = frames(&mut terminal, &mut state, api, me).await;
    if mouse {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    shown
}

/// The loop of [`run`]: it draws the state, takes the input of the
/// person, and looks at riff-server each [`every`].
async fn frames(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut State,
    api: &Api,
    me: &SessionUri,
) -> Result<()> {
    use tokio::time::Instant;
    let mut input_at = Instant::now();
    let mut next = Instant::now() + LOOK_EVERY;
    let mut looking: Option<tokio::task::JoinHandle<Result<Look>>> = None;
    // `r` while a look runs: look again after it.
    let mut again = false;
    let mut good_at = chrono::Local::now();
    loop {
        if looking
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
            && let Some(done) = looking.take()
        {
            match done.await? {
                Ok(look) => {
                    state.looked(look, crate::run::now_ms());
                    good_at = chrono::Local::now();
                }
                // A fault that a new try can repair keeps the last
                // screen (01M6VAZKAH6B53YKR8W4YKNSH6).
                Err(e) if api::passes(&e) => state.failed(text::top_fault(
                    &e.to_string(),
                    api.base(),
                    &good_at,
                    &chrono::Local::now(),
                )),
                Err(e) => return Err(e),
            }
            next = if std::mem::take(&mut again) {
                Instant::now()
            } else {
                Instant::now() + every(input_at.elapsed())
            };
        }
        if looking.is_none() && Instant::now() >= next {
            let (api, me) = (api.clone(), me.clone());
            looking = Some(tokio::spawn(async move { look(&api, &me).await }));
        }
        let mut drawn = None;
        terminal.draw(|frame| {
            let shown = view::compute_view(state, frame.area(), crate::run::now_ms());
            view::render(&shown, frame.buffer_mut());
            drawn = Some(shown);
        })?;
        let Some(input) = next_input().await? else {
            continue;
        };
        input_at = Instant::now();
        let action = match input {
            Input::Key(key) => state.on_key(key),
            Input::Click { column, row } => drawn
                .as_ref()
                .and_then(|shown| state.on_click(shown, column, row)),
        };
        match action {
            Some(Action::Quit) => {
                if let Some(looking) = looking {
                    looking.abort();
                }
                return Ok(());
            }
            Some(Action::Look) if looking.is_some() => again = true,
            Some(Action::Look) => next = Instant::now(),
            None => {}
        }
    }
}
