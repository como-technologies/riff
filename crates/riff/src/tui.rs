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
//!   The Machines tab ([`machines`]) and the Chat tab ([`chat`]) are
//!   built.
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
//! a task of its own, so a slow look holds up no key. A look also asks
//! the riff daemon of this host for its status on its socket
//! ([`machines::here`], 01M6VFD09QN71SWMPP8YXCRAEK).
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
//! The frame gives each other key to the tab: the arrows, `Enter`, and
//! a letter as [`Key::Char`]. The tab turns it into an [`Action`], for
//! example [`machines::on_key`].
//!
//! The prompt of the Chat tab takes each key that types: there only
//! Alt opens a tab, and `Ctrl-C` ends the TUI. [`State::input`] gives
//! the keys of the tab ([`chat::key_of`]) before the keys of the frame
//! ([`input_of`]).
//!
//! An action that changes something asks first in the note line
//! ([`Question`], 01M6VAZKDEQ6951KGPGAMF1SNJ, 01M6VFD0S90SB8M8WV952NT4Y1):
//! `Set the limit of thelio to 4 sessions? y/N`. `y` gives the action to
//! the loop, and the loop runs the code of its CLI command in a task
//! of its own. Its result goes to the note line, and the TUI looks
//! again.
//!
//! # The marks
//!
//! A session of a tab gets a [`Mark`] (01M6VAZKRTVZTY73PQV5GE2WY6): a
//! small fixed set, from the state that riff-server derives and from
//! the phase of a container session. The worst mark comes first.

pub mod chat;
pub mod machines;
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
    /// has none: it needs the device commands. A built tab has none: it
    /// has its body ([`Tab::built`]).
    pub fn today(self) -> &'static [&'static str] {
        match self {
            Tab::Sessions => &["riff top", "riff who"],
            Tab::Waves => &["riff top", "riff wave list"],
            Tab::Chat | Tab::Machines | Tab::Devices => &[],
            Tab::Reports => &["riff usage", "riff audit", "riff daemon events NAME"],
        }
    }

    /// True when the build item of the tab merged: the tab draws its own
    /// body.
    ///
    /// ```
    /// use riff::tui::Tab;
    ///
    /// assert!(Tab::Machines.built());
    /// assert!(Tab::Chat.built());
    /// assert!(!Tab::Sessions.built());
    /// ```
    pub fn built(self) -> bool {
        matches!(self, Tab::Machines | Tab::Chat)
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
    /// Select the row above: `↑`, `k`, or the wheel of the mouse.
    Up,
    /// Select the row below: `↓`, `j`, or the wheel of the mouse.
    Down,
    /// Move a page up: `PgUp`.
    PageUp,
    /// Move a page down: `PgDn`.
    PageDown,
    /// Select the first row: `g` or `Home`.
    First,
    /// Select the last row: `G` or `End`.
    Last,
    /// Act on the selected row, or end an answer: `Enter`.
    Enter,
    /// Remove the last character of an answer: `Backspace`.
    Backspace,
    /// Select the row N of the body of the tab: a click on it.
    Row(usize),
    /// Another character with no Ctrl and no Alt: the key of an action
    /// of the tab, the answer to a question, or a character of an
    /// answer.
    Char(char),
    /// A key of the Chat tab: the prompt and the log.
    Chat(chat::Key),
}

/// What the loop of the TUI does for a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Look at riff-server again now.
    Look,
    /// End the TUI. The sessions go on.
    Quit,
    /// Set the most live sessions of `host` with the code of `riff
    /// daemon limit --sessions N` (01M6VFD0PPXDNH3NKD3YE4W9SB).
    Limit { host: String, sessions: u32 },
    /// Post the line to the thread `chat`, as `riff chat` does.
    Post(String),
}

/// A question in the note line (01M6VFD0S90SB8M8WV952NT4Y1): `y` gives
/// `yes` to the loop. Each other key but `q` and `Ctrl-C` answers no:
/// the note line shows `no`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The question, with no `y/N`.
    pub text: String,
    /// The action of `y`.
    pub yes: Action,
    /// The note after no.
    pub no: String,
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
    /// The time of the call `who` (01M6VFD0HGCVMYNNC3H4788W9X).
    pub call: Option<Duration>,
    /// The new connections of the link since the start of the TUI
    /// ([`riff_client::link::Link::swaps`]).
    pub reconnects: u64,
    /// This host and the status of its riff daemon
    /// (01M6VFD09QN71SWMPP8YXCRAEK). `None` when the look did not ask.
    pub here: Option<machines::Here>,
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
            call: None,
            reconnects: 0,
            here: None,
        }
    }
}

/// Looks at riff-server as `me`: the calls `riff` and `who`
/// (01M6VAZK7N636Q0JFSV553A9ES). Each call has the budget of a short
/// command, as in `riff top`. Then it asks the riff daemon of this host
/// for its status ([`machines::here`], 01M6VFD09QN71SWMPP8YXCRAEK).
pub async fn look(api: &Api, me: &SessionUri) -> Result<Look> {
    let pauses = api.pauses(me).await?;
    let start = std::time::Instant::now();
    let who = api.roster(me, false).await?;
    let call = start.elapsed();
    Ok(Look {
        pauses,
        who,
        server: riff_client::link::server_build(),
        call: Some(call),
        reconnects: api.link().swaps(),
        here: Some(machines::here(me).await),
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
    /// The question in the note line, until the person answers it.
    question: Option<Question>,
    /// What the person types in the note line, until `Enter` or `Esc`.
    typing: Option<machines::Typing>,
    /// The host that the person selected in the Machines tab.
    host: Option<machines::Name>,
    /// The filter of the Sessions tab, for example `host:thelio`
    /// (01M6VFD0M3RHT488D0RQ84RVMC).
    filter: Option<String>,
    /// The Chat tab: the log, the prompt and the focus.
    chat: chat::Chat,
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
            question: None,
            typing: None,
            host: None,
            filter: None,
            chat: chat::Chat::default(),
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

    /// Asks `question` in the note line (01M6VFD0S90SB8M8WV952NT4Y1).
    pub fn ask(&mut self, question: Question) {
        self.question = Some(question);
    }

    /// The text of the note line while the TUI asks: the question with
    /// `y/N`, or what the person types with a cursor. It comes before
    /// the fault and the note.
    ///
    /// ```
    /// use riff::tui::{Action, Key, Question, State};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this());
    /// let yes = Action::Limit { host: "thelio".into(), sessions: 4 };
    /// let no = "The limit of thelio stays.".to_owned();
    /// state.ask(Question { text: "Set the limit?".into(), yes: yes.clone(), no: no.clone() });
    /// assert_eq!(state.asked().as_deref(), Some("Set the limit? y/N"));
    /// assert_eq!(state.on_key(Key::Char('y')), Some(yes.clone()));
    /// assert_eq!(state.asked(), None);
    /// // Each other key answers no, also Esc.
    /// state.ask(Question { text: "Set the limit?".into(), yes: yes.clone(), no: no.clone() });
    /// assert_eq!(state.on_key(Key::Esc), None);
    /// assert_eq!(state.shown_note(), Some("The limit of thelio stays."));
    /// // q ends the TUI.
    /// state.ask(Question { text: "Set the limit?".into(), yes, no });
    /// assert_eq!(state.on_key(Key::Quit), Some(Action::Quit));
    /// ```
    pub fn asked(&self) -> Option<String> {
        if let Some(question) = &self.question {
            return Some(format!("{} y/N", question.text));
        }
        self.typing.as_ref().map(machines::Typing::line)
    }

    /// The filter of the Sessions tab, for example `host:thelio`
    /// (01M6VFD0M3RHT488D0RQ84RVMC).
    pub fn filter(&self) -> Option<&str> {
        self.filter.as_deref()
    }

    /// Opens the tab `tab`: its news goes. The prompt of the Chat tab
    /// gets the focus.
    fn open(&mut self, tab: Tab) {
        self.tab = tab;
        self.news[tab as usize] = None;
        if tab == Tab::Chat {
            self.on_chat(chat::Key::Prompt);
        }
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
        // A question takes the next key (01M6VFD0S90SB8M8WV952NT4Y1).
        if let Some(question) = self.question.take() {
            return match key {
                Key::Char('y' | 'Y') => Some(question.yes),
                Key::Quit => Some(Action::Quit),
                _ => {
                    self.note(question.no);
                    None
                }
            };
        }
        if let Some(typing) = self.typing.take() {
            return machines::typed(self, typing, key);
        }
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
            // The popup takes no key of a tab.
            _ if self.help => None,
            Key::Chat(key) => self.on_chat(key),
            key => match self.tab {
                Tab::Machines => machines::on_key(self, key),
                _ => None,
            },
        }
    }

    /// Takes a click of the person at `column` and `row` on the view
    /// `view`: it presses the key of the place that it hits
    /// (01M6VAZKG8FS4HEA9VSA4Z4YF1). A click on no place does nothing.
    pub fn on_click(&mut self, view: &view::View, column: u16, row: u16) -> Option<Action> {
        self.on_key(view.hit(column, row)?)
    }

    /// Keeps what the person saw in `view`, the view of the last draw:
    /// a scroll of the log of the Chat tab needs its rows.
    pub fn drawn(&mut self, view: &view::View) {
        if let Some(window) = view.chat {
            self.chat.window = window;
        }
    }

    /// The input of the person in `event`. In the Chat tab, the keys of
    /// the tab come first ([`chat::key_of`]), then the keys of the frame
    /// ([`input_of`]). The help popup takes only the keys of the frame.
    ///
    /// ```
    /// use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    /// use riff::tui::{Input, Key, State, Tab, chat};
    /// use riff_core::build::Build;
    ///
    /// let q = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    /// let mut state = State::new(Build::this());
    /// assert_eq!(state.input(&q), Some(Input::Key(Key::Quit)));
    /// // In the prompt of the Chat tab, `q` types.
    /// state.on_key(Key::Open(Tab::Chat));
    /// assert_eq!(state.input(&q), Some(Input::Key(Key::Chat(chat::Key::Type('q')))));
    /// // In the log, `q` ends the TUI.
    /// state.on_key(Key::Chat(chat::Key::Log));
    /// assert_eq!(state.input(&q), Some(Input::Key(Key::Quit)));
    /// ```
    pub fn input(&self, event: &ratatui::crossterm::event::Event) -> Option<Input> {
        if self.tab == Tab::Chat
            && !self.help
            && let Some(key) = chat::key_of(event, self.chat.focus())
        {
            return Some(Input::Key(Key::Chat(key)));
        }
        input_of(event)
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
/// // The keys of a tab (01M6VFD0M3RHT488D0RQ84RVMC).
/// for (code, key) in [
///     (KeyCode::Up, Key::Up),
///     (KeyCode::Char('k'), Key::Up),
///     (KeyCode::Down, Key::Down),
///     (KeyCode::Char('j'), Key::Down),
///     (KeyCode::PageUp, Key::PageUp),
///     (KeyCode::PageDown, Key::PageDown),
///     (KeyCode::Char('g'), Key::First),
///     (KeyCode::End, Key::Last),
///     (KeyCode::Enter, Key::Enter),
///     (KeyCode::Backspace, Key::Backspace),
///     (KeyCode::Char('7'), Key::Char('7')),
///     (KeyCode::Char('l'), Key::Char('l')),
/// ] {
///     assert_eq!(input_of(&press(code, none)), Some(Input::Key(key)), "{code:?}");
/// }
/// assert_eq!(input_of(&press(KeyCode::Char('G'), KeyModifiers::SHIFT)), Some(Input::Key(Key::Last)));
/// // A letter with Ctrl is no key of a tab.
/// assert_eq!(input_of(&press(KeyCode::Char('l'), KeyModifiers::CONTROL)), None);
/// assert_eq!(input_of(&press(KeyCode::F(2), none)), None);
/// let mouse = |kind| {
///     Event::Mouse(MouseEvent { kind, column: 4, row: 0, modifiers: KeyModifiers::NONE })
/// };
/// assert_eq!(
///     input_of(&mouse(MouseEventKind::Down(MouseButton::Left))),
///     Some(Input::Click { column: 4, row: 0 })
/// );
/// // The wheel selects the row above or below.
/// assert_eq!(input_of(&mouse(MouseEventKind::ScrollUp)), Some(Input::Key(Key::Up)));
/// assert_eq!(input_of(&mouse(MouseEventKind::ScrollDown)), Some(Input::Key(Key::Down)));
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
                KeyCode::Up | KeyCode::Char('k') => Key::Up,
                KeyCode::Down | KeyCode::Char('j') => Key::Down,
                KeyCode::PageUp => Key::PageUp,
                KeyCode::PageDown => Key::PageDown,
                KeyCode::Home | KeyCode::Char('g') => Key::First,
                KeyCode::End | KeyCode::Char('G') => Key::Last,
                KeyCode::Enter => Key::Enter,
                KeyCode::Backspace => Key::Backspace,
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    Key::Char(c)
                }
                _ => return None,
            };
            Some(Input::Key(key))
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => Some(Input::Click {
                column: mouse.column,
                row: mouse.row,
            }),
            MouseEventKind::ScrollUp => Some(Input::Key(Key::Up)),
            MouseEventKind::ScrollDown => Some(Input::Key(Key::Down)),
            _ => None,
        },
        _ => None,
    }
}

/// How long [`next_input`] waits for a key or a click. The age of the
/// look in the header draws again after each wait.
const INPUT_WAIT: Duration = Duration::from_millis(200);

/// The next event of the terminal, or `None` when none comes in
/// [`INPUT_WAIT`]. The blocking reads of crossterm run in a blocking
/// task of tokio, not in the async loop, as in `riff view`.
async fn next_event() -> Result<Option<ratatui::crossterm::event::Event>> {
    use ratatui::crossterm::event;
    tokio::task::spawn_blocking(|| {
        if !event::poll(INPUT_WAIT)? {
            return Ok(None);
        }
        Ok(Some(event::read()?))
    })
    .await?
}

/// `riff` with no command, in a terminal: the TUI, as `me` at `api`,
/// until the person ends it (01M6VAZ7WH3MFAYF4PAZW6EDVY). The Chat tab
/// posts as `person`, as `riff chat` does. With `mouse`, the TUI takes
/// the mouse (01M6VAZKG8FS4HEA9VSA4Z4YF1).
pub async fn run(api: &Api, me: &SessionUri, person: &chat::Person, mouse: bool) -> Result<()> {
    use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
    use ratatui::crossterm::execute;
    // The first look runs before the TUI takes the terminal: its error
    // prints as in the CLI (01M6VAZKAH6B53YKR8W4YKNSH6).
    let first = look(api, me).await?;
    let mut state = State::new(Build::this());
    let start = crate::run::now_ms();
    state.looked(first, start);
    state.chat_as(person.me.who().user(), u64::try_from(start).unwrap_or(0));
    let mut terminal = ratatui::try_init()?;
    if mouse && let Err(e) = execute!(std::io::stdout(), EnableMouseCapture) {
        ratatui::restore();
        return Err(e.into());
    }
    let shown = frames(&mut terminal, &mut state, api, me, person).await;
    if mouse {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    shown
}

/// The loop of [`run`]: it draws the state, takes the input of the
/// person, looks at riff-server each [`every`], and runs each action in
/// a task of its own. The [`chat::Link`] of `person` follows the thread
/// `chat` and posts its lines.
async fn frames(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut State,
    api: &Api,
    me: &SessionUri,
    person: &chat::Person,
) -> Result<()> {
    use tokio::time::Instant;
    let mut input_at = Instant::now();
    let mut next = Instant::now() + LOOK_EVERY;
    let mut looking: Option<tokio::task::JoinHandle<Result<Look>>> = None;
    // The action that runs: its result is the next note.
    let mut acting: Option<tokio::task::JoinHandle<String>> = None;
    // `r` while a look runs: look again after it.
    let mut again = false;
    let mut good_at = chrono::Local::now();
    let mut link = chat::Link::start(person);
    loop {
        for came in link.came() {
            state.chat_came(came);
        }
        if acting
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
            && let Some(done) = acting.take()
        {
            state.note(done.await?);
            // The action changed something: look again.
            if looking.is_some() {
                again = true;
            } else {
                next = Instant::now();
            }
        }
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
            if let Some(at) = shown.cursor {
                frame.set_cursor_position(at);
            }
            drawn = Some(shown);
        })?;
        if let Some(shown) = &drawn {
            state.drawn(shown);
        }
        let Some(event) = next_event().await? else {
            continue;
        };
        let Some(input) = state.input(&event) else {
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
                for task in [
                    looking.map(|t| t.abort_handle()),
                    acting.map(|t| t.abort_handle()),
                ]
                .into_iter()
                .flatten()
                {
                    task.abort();
                }
                link.end().await;
                return Ok(());
            }
            Some(Action::Look) if looking.is_some() => again = true,
            Some(Action::Look) => next = Instant::now(),
            Some(Action::Limit { .. }) if acting.is_some() => state.note(machines::STILL_ACTING),
            Some(Action::Limit { host, sessions }) => {
                state.note(machines::setting(&host, sessions));
                let (api, me) = (api.clone(), me.clone());
                acting = Some(tokio::spawn(async move {
                    machines::set_limit(&api, &me, &host, sessions).await
                }));
            }
            Some(Action::Post(body)) => link.send(body),
            None => {}
        }
    }
}
