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
//! - The note line shows the first note that it has: a question or the
//!   filter that the person types in the Sessions tab, the fault of the
//!   link, in the Chat tab a post that failed and the note of the
//!   prompt, else the result of the last action
//!   (01M6VMACJGF4A0ZXC6YK13KNCH). With none of them, the Waves tab
//!   shows the age of its last read of `gh` (01M6VFGYW0XVJ5JSX6R1NCR6W7).
//!   The key line shows the keys of the tab, and `?` shows them all in a
//!   popup (01M6VAZKDEQ6951KGPGAMF1SNJ).
//! - The Sessions tab ([`sessions`], #1637) is the one view of the
//!   sessions. The Chat tab ([`chat`], #1640) shows the thread `chat`.
//!   The Waves tab ([`waves`], #1638) shows a board for each repository.
//!   The Machines tab ([`machines`], #1639) shows the hosts. The Reports
//!   tab ([`reports`], #1641) shows the usage, the audit and the log of
//!   a session. The body of the Devices tab comes with its build item G
//!   #1642. Until then, it says that it is not built yet
//!   (01M6VAZKVK7530JJ63S7RVF88J).
//!
//! The render is pure (01M6VAZKNXZV1RTG7MT0GMHKT3). [`State`] holds what
//! the TUI knows: the last look of riff-server, the last status of the
//! riff daemon of this machine, the titles of the issues, the boards of
//! the Waves tab, the fault of the link, the note and the choices of the
//! person. [`view::compute_view`] makes the [`view::View`] from the
//! state, the size and the time: its rows, their places, and the places
//! that a click hits. [`view::render`] draws the view on a ratatui
//! `Buffer`. So a screen test draws a tab into a `Buffer` and compares
//! the rows, with no terminal.
//!
//! ```mermaid
//! flowchart LR
//!     S["riff-server<br/>riff, who, plan show, tail"] -- "a look, or a fault" --> ST["State"]
//!     D["the socket of the daemon<br/>status"] -- "the containers here" --> ST
//!     G["gh<br/>api, pr list"] -- "the titles, the boards" --> ST
//!     K["a key or a click"] --> ST
//!     ST -- "&State, the size, the time" --> V["compute_view<br/>pure: the rows and the hits"]
//!     V -- "&View" --> R["render<br/>pure: draws on a Buffer"]
//!     ST -- "an Action" --> E["the loop: the code of the CLI command"]
//!     E -- "a note, a new look" --> ST
//! ```
//!
//! # The looks
//!
//! [`run`] looks at riff-server with the calls `riff` and `who`
//! ([`look`]) each [`LOOK_EVERY`], and each [`IDLE_LOOK_EVERY`] after
//! [`IDLE_AFTER`] with no key and no click ([`every`],
//! 01M6VAZK7N636Q0JFSV553A9ES). `r` looks again at once. A message in
//! the thread of the repository of the folder starts a look at once, at
//! most one each [`THREAD_LOOK_EVERY`], as in `riff top`. A look runs in
//! a task of its own, so a slow look holds up no key. A look also asks
//! the riff daemon of this host for its status on its socket
//! ([`machines::here`], 01M6VFD09QN71SWMPP8YXCRAEK).
//!
//! While the Sessions tab is open (01M6VGHCDTDJ69B7C02B5K61PW), the TUI
//! asks the riff daemon of this machine for its status each
//! [`DAEMON_EVERY`]: its container sessions
//! join the sessions model. It reads the titles of the open issues of
//! each repository with a live session from `gh` each
//! [`crate::top::ISSUES_TTL`], with the conditional read of the Waves
//! tab and no read of the pull requests ([`forge::titles`]). A read of
//! `gh` that fails names the error in the note line once, and keeps the
//! last titles.
//!
//! While the Waves tab shows, each look also reads the holds of its
//! boards (01M6VFGK79JGRF919BM8SJ6E08), and the TUI reads the forge of
//! its boards each [`forge::READ_EVERY`] ([`forge::read_each`],
//! 01M6VFGK4P2FWMFQHBZKR3JXSZ). Each call of `gh` of the TUI counts as
//! the caller `tui`. The two tabs share the `ETag` of each read
//! ([`forge::Kept`]).
//!
//! | What | Source | When |
//! |---|---|---|
//! | The sessions, the state of the riff | riff-server: `riff`, `who` | each 3 s, each 15 s when idle, and after a message of the thread |
//! | The holds of the boards | riff-server: `plan show` | at each look while the Waves tab shows |
//! | The container sessions of this machine | the socket of the daemon: `status` | each second while the Sessions tab is open |
//! | The titles of the issues | `gh api`: the conditional read of the open issues | each 60 s while the Sessions tab is open |
//! | The items of the boards | `gh api`, and `gh pr list` with an open pull request | each 60 s while the Waves tab shows, and at once on `r` |
//! | The usage of a wave | `gh`: the milestones, the issues and their comments | when the Usage report shows with no read yet, and on `r` |
//! | The audit of a wave | riff-server: the log; `gh`: the forge | on `Enter` and `r` in the Audit report |
//! | The events of a session | the socket of the daemon, or riff-server for another host | while the Session log shows |
//!
//! A read of `gh` runs on a thread of its own, not in a blocking task of
//! tokio: the end of the process waits for each blocking task of tokio.
//! So `q` ends the TUI at once and gives back the screen, also while a
//! slow read runs. The read goes on, and its result goes nowhere (#1707,
//! 01M6VT7TTV8J90C5E3QAZ454GY).
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
//! [`input_of`] turns an event of the terminal into a [`Key`], and
//! [`State::on_key`] turns a key into an [`Action`]. The mouse does the
//! same as the keys (01M6VAZKG8FS4HEA9VSA4Z4YF1): the view has a
//! [`view::Hit`] for each place that a click hits, with its key, and
//! [`State::on_click`] presses that key. A double click on a row presses
//! Enter, and the wheel moves the selection. The setting `tui.mouse`
//! turns the mouse off ([`crate::settings::tui_mouse`]). The TUI also
//! asks the terminal for focus events: when its window gets the focus
//! back, [`input_of`] gives [`Input::Focused`], and the prompt of the
//! Chat tab gets the focus ([`State::focused`],
//! 01M6XT6MCZ38XEN7ZHWTRF50Z5).
//!
//! A plain character is a [`Key::Char`]. The state gives it its meaning:
//! while the note line asks for a text, it types into it
//! (01M6VFGKCJ02G33TXR01GY6M0G). While the person types a filter of the
//! Sessions tab or of the Machines tab ([`State::takes_text`]), each
//! character is a letter of the filter, also `1` to `6` and `q`
//! (01M6VJQPNJ4AX4RT877F1QAPPX).
//! Else the frame takes its keys, and gives each other key to the tab:
//! the arrows, `Enter`, and a letter. The tab turns it into an
//! [`Action`], for example [`machines::on_key`] and
//! [`waves::Waves::on_key`].
//!
//! | Key | Does |
//! |---|---|
//! | `1` to `6`, also with Alt | Open a tab. |
//! | `Tab`, `Shift-Tab` | Open the next tab, or the tab before. |
//! | `r` | Look again now. |
//! | `?` | Show or hide the keys of this tab. |
//! | `Esc` | Close the popup. With no popup, the tab takes it: the Sessions tab and the Machines tab clear their filter. |
//! | `y`, `n` | Answer the question in the note line. |
//! | `q`, `Ctrl-C` | End the TUI. The sessions go on. |
//! | `↑` `↓` `j` `k`, `PgUp` `PgDn`, `g` `G` | Select a row, in a tab with rows. |
//!
//! The help popup takes no key of a tab.
//!
//! The prompt of the Chat tab takes each key that types: there only
//! Alt opens a tab, and `Ctrl-C` ends the TUI. [`State::input`] gives
//! the keys of the tab ([`chat::key_of`]) before the keys of the frame
//! ([`input_of`]).
//!
//! # The actions
//!
//! Each action calls the code of its CLI command
//! (01M54216GT8SX6NVMKRGNR2H0T). An action that changes something asks
//! first in the note line ([`Question`], 01M6VFD0S90SB8M8WV952NT4Y1,
//! 01M6VFGK9XXWCA8X7CE8EZNSJV, 01M6VMACJGF4A0ZXC6YK13KNCH):
//! `Set the limit of thelio to 4 sessions? y/N`. `y` gives the action to
//! the loop. The attach asks nothing. The attach and the takeover leave
//! the screen of the TUI, as `riff lead start` and `riff takeover` do in
//! a terminal, and the TUI draws again at their end
//! (01M6VGHC59FJS10FJEYH6K4CAQ). The limit, a request to the daemon of a
//! host and each [`Effect`] run in a task of their own. The result of an
//! action goes to the note line, and the TUI looks again at once.
//!
//! | Action | The code of |
//! |---|---|
//! | [`Action::Attach`] | `riff lead start`: [`crate::lead::attach`] |
//! | [`Action::Takeover`] | `riff takeover NAME`: [`crate::takeover::takeover`] |
//! | [`Action::End`] | `riff daemon end NAME`: [`crate::daemon::ask_end`] |
//! | [`Action::Host`] | `riff daemon end NAME --host HOST`, `riff leave --host HOST`: [`sessions::ask_host`] |
//! | [`Action::Limit`] | `riff daemon limit --sessions N`: [`machines::set_limit`] |
//! | [`Action::Do`] | `riff plan hold`, `riff plan free`, `riff pause`, `riff resume`, `riff questions --answered`: [`Effect::run`] |
//! | [`Action::Post`] | `riff chat`: [`crate::chat::posts`], through [`chat::Link`] |
//!
//! # The marks
//!
//! A session of a tab gets a [`Mark`] (01M6VAZKRTVZTY73PQV5GE2WY6): a
//! small fixed set, from the state that riff-server derives and from
//! the phase of a container session. The worst mark comes first. A
//! session that turns `!` or `✓` while the person is in another tab
//! gives the Sessions tab the news `•`.

pub mod chat;
pub mod forge;
pub mod machines;
pub mod reports;
pub mod sessions;
pub mod view;
pub mod waves;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use anyhow::Result;
use ratatui::style::Color;
use riff_client::api::{self, Api, PauseScope};
use riff_client::sessions::{Phase, Sessions};
use riff_client::waves::Issues;
use riff_core::build::Build;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{HoldInfo, RiffOwner, RiffReply, RiffState, SessionState, WhoReply};

use crate::daemon::Status;
use crate::reach::Reach;
use crate::text;

/// The time between two looks of riff-server.
pub const LOOK_EVERY: Duration = Duration::from_secs(3);

/// The time between two looks after [`IDLE_AFTER`] with no key and no
/// click.
pub const IDLE_LOOK_EVERY: Duration = Duration::from_secs(15);

/// The time with no key and no click after which the TUI looks each
/// [`IDLE_LOOK_EVERY`].
pub const IDLE_AFTER: Duration = Duration::from_secs(120);

/// The least time between two looks that messages of the thread start.
pub const THREAD_LOOK_EVERY: Duration = Duration::from_secs(1);

/// The time between two asks of the status of the riff daemon of this
/// machine, while the Sessions tab is open.
pub const DAEMON_EVERY: Duration = Duration::from_secs(1);

/// The longest time between the two clicks of a double click.
pub const DOUBLE_CLICK: Duration = Duration::from_millis(500);

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

    /// True when the build item of the tab merged: the tab draws its own
    /// body. The Sessions tab (#1637), the Chat tab (#1640), the Waves tab
    /// (#1638), the Machines tab (#1639) and the Reports tab (#1641) are
    /// built. The body of the Devices tab says that it is not built yet:
    /// no command of the CLI shows its area yet
    /// (01M6VAZKVK7530JJ63S7RVF88J).
    ///
    /// ```
    /// use riff::tui::Tab;
    ///
    /// assert!(Tab::Sessions.built());
    /// assert!(Tab::Waves.built());
    /// assert!(Tab::Machines.built());
    /// assert!(Tab::Chat.built());
    /// assert!(Tab::Reports.built());
    /// assert!(!Tab::Devices.built());
    /// ```
    pub fn built(self) -> bool {
        self != Tab::Devices
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

/// A key of the person. [`State::on_key`] gives each key its meaning in
/// the tab and the mode: a character is a key of the tab, a letter of a
/// text, or the answer to a question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Open a tab: `Alt-1` to `Alt-6`, and a click on a tab. The digits
    /// with no Alt are a [`Key::Char`].
    Open(Tab),
    /// Open the next tab: `Tab`.
    Next,
    /// Open the tab before: `Shift-Tab`.
    Before,
    /// Show or hide the keys of the tab: a click on `? help`.
    Help,
    /// Close the popup, cancel a question, leave a mode, or clear the
    /// filter: `Esc`.
    Esc,
    /// Look again now: a click on `r read again`.
    Again,
    /// End the TUI: `Ctrl-C`, and a click on `q quit`.
    Quit,
    /// Select the row above: `↑`, or the wheel of the mouse.
    Up,
    /// Select the row below: `↓`, or the wheel of the mouse.
    Down,
    /// Move a page up: `PgUp`.
    PageUp,
    /// Move a page down: `PgDn`.
    PageDown,
    /// Select the first row: `Home`.
    First,
    /// Select the last row: `End`.
    Last,
    /// Fold: `←`.
    Left,
    /// Unfold: `→`.
    Right,
    /// Act on the selected row, or take a text: `Enter`.
    Enter,
    /// Delete the last character of a text: `Backspace`.
    Backspace,
    /// A click on the row of this index in the rows of the tab: the tree
    /// of the Sessions tab, the rows of the Waves tab, or the hosts of
    /// the Machines tab.
    Row(usize),
    /// A click on the fold mark of the row of this index in the tree of
    /// the Sessions tab.
    Fold(usize),
    /// A click on the name of a report in the Reports tab.
    Report(reports::Report),
    /// A plain character, for example `q`, `3`, `j`, `h` or `y`. The
    /// state gives it its meaning ([`State::on_key`]).
    Char(char),
    /// A key of the Chat tab: the prompt and the log.
    Chat(chat::Key),
}

/// What the loop of the TUI does for a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Look at riff-server again now, and read the forge of the tab.
    Look,
    /// End the TUI. The sessions go on.
    Quit,
    /// Attach to the lead in this container of this machine
    /// ([`crate::lead::attach`]).
    Attach(String),
    /// Take over the session in this container of this machine
    /// ([`crate::takeover::takeover`]).
    Takeover(String),
    /// End the session of the daemon of this machine in this container
    /// ([`crate::daemon::ask_end`]).
    End(String),
    /// Ask the riff daemon of `host` for `request` in the repository
    /// `repo`, as `--host HOST` does: `daemon end NAME` or `leave REPO`
    /// ([`sessions::ask_host`], 01M6WMAXEVZB2XJN4XPR879J6W).
    Host {
        host: String,
        repo: String,
        request: crate::daemon::remote::Request,
    },
    /// Set the most live sessions of `host` with the code of `riff
    /// daemon limit --sessions N` (01M6VFD0PPXDNH3NKD3YE4W9SB).
    Limit { host: String, sessions: u32 },
    /// Post the line to the thread `chat`, as `riff chat` does.
    Post(String),
    /// Run the effect: the person said `y`.
    Do(Effect),
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

/// An action of the Waves tab that changes something, with the code of
/// its CLI command (01M6VFGK9XXWCA8X7CE8EZNSJV). `p` and `P` of the
/// Sessions tab are its pause too (01M6VGH4Y0G25VYBBD5ZV4GG2H).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// `riff plan hold issue-ITEM REASON` in the repository `repo`.
    Hold {
        repo: String,
        item: u64,
        reason: String,
    },
    /// `riff plan free issue-ITEM` in the repository `repo`.
    Free { repo: String, item: u64 },
    /// `riff pause` or `riff resume` of the repository `repo`, or of the
    /// whole riff with no repository.
    Pause {
        repo: Option<String>,
        state: RiffState,
    },
    /// `riff questions --answered ITEM --repo REPO`
    /// ([`crate::questions::answered`]).
    Answered { repo: String, item: u64 },
}

impl Effect {
    /// The question of the note line before the effect: `y` runs it,
    /// and no changes nothing.
    ///
    /// ```
    /// use riff::tui::{Action, Effect, NOTHING_CHANGED};
    /// use riff_core::wire::RiffState;
    ///
    /// let hold = Effect::Hold { repo: "o/r".into(), item: 12, reason: "a design".into() };
    /// let question = hold.clone().question();
    /// assert_eq!(question.text, "Hold #12 of o/r: a design?");
    /// assert_eq!(question.yes, Action::Do(hold));
    /// assert_eq!(question.no, NOTHING_CHANGED);
    /// let free = Effect::Free { repo: "o/r".into(), item: 12 };
    /// assert_eq!(free.question().text, "Free #12 of o/r?");
    /// let pause = Effect::Pause { repo: Some("o/r".into()), state: RiffState::Paused };
    /// assert_eq!(pause.question().text, "Pause o/r?");
    /// let resume = Effect::Pause { repo: None, state: RiffState::Running };
    /// assert_eq!(resume.question().text, "Resume the whole riff?");
    /// let answered = Effect::Answered { repo: "o/r".into(), item: 12 };
    /// assert_eq!(answered.question().text, "Mark the questions of #12 of o/r as answered?");
    /// ```
    pub fn question(self) -> Question {
        let text = match &self {
            Effect::Hold { repo, item, reason } => format!(
                "Hold #{item} of {}: {}?",
                text::safe(repo),
                text::safe(reason)
            ),
            Effect::Free { repo, item } => format!("Free #{item} of {}?", text::safe(repo)),
            Effect::Answered { repo, item } => format!(
                "Mark the questions of #{item} of {} as answered?",
                text::safe(repo)
            ),
            Effect::Pause { repo, state } => {
                let verb = match state {
                    RiffState::Paused => "Pause",
                    RiffState::Running => "Resume",
                };
                match repo {
                    Some(repo) => format!("{verb} {}?", text::safe(repo)),
                    None => format!("{verb} the whole riff?"),
                }
            }
        };
        Question::new(text, Action::Do(self))
    }

    /// The note line while the effect runs.
    ///
    /// ```
    /// use riff::tui::Effect;
    /// use riff_core::wire::RiffState;
    ///
    /// let free = Effect::Free { repo: "o/r".into(), item: 12 };
    /// assert_eq!(free.doing(), "riff frees #12 of o/r.");
    /// let pause = Effect::Pause { repo: None, state: RiffState::Paused };
    /// assert_eq!(pause.doing(), "riff pauses the whole riff.");
    /// let answered = Effect::Answered { repo: "o/r".into(), item: 12 };
    /// assert_eq!(answered.doing(), "riff marks #12 of o/r as answered.");
    /// ```
    pub fn doing(&self) -> String {
        let (verb, what) = match self {
            Effect::Hold { repo, item, .. } => {
                ("holds", format!("#{item} of {}", text::safe(repo)))
            }
            Effect::Free { repo, item } => ("frees", format!("#{item} of {}", text::safe(repo))),
            Effect::Answered { repo, item } => (
                "marks",
                format!("#{item} of {} as answered", text::safe(repo)),
            ),
            Effect::Pause { repo, state } => (
                match state {
                    RiffState::Paused => "pauses",
                    RiffState::Running => "resumes",
                },
                repo.as_deref()
                    .map_or_else(|| "the whole riff".to_owned(), text::safe),
            ),
        };
        format!("riff {verb} {what}.")
    }

    /// Runs the effect as `me` at `api`, or through `reach`, with the
    /// code of its CLI command, and gives the line of that command. The
    /// calls of `gh` go through `gh`.
    pub async fn run(
        &self,
        api: &Api,
        me: &SessionUri,
        reach: &Reach,
        gh: &crate::pr::Gh,
    ) -> Result<String> {
        let thread = |repo: &str| repo.parse::<ThreadName>();
        match self {
            Effect::Hold { repo, item, reason } => {
                reach
                    .plan_hold(&thread(repo)?, &format!("issue-{item}"), reason)
                    .await
            }
            Effect::Free { repo, item } => {
                reach
                    .plan_free(&thread(repo)?, &format!("issue-{item}"))
                    .await
            }
            Effect::Answered { repo, item } => {
                crate::questions::answered(gh, api, me, &thread(repo)?, *item).await
            }
            Effect::Pause { repo, state } => {
                let scope = match repo {
                    Some(repo) => PauseScope::Repository(thread(repo)?),
                    None => PauseScope::Riff,
                };
                crate::reach::pause(api, me, me.place(), scope, *state).await
            }
        }
    }
}

/// The line of a question that the person cancelled.
pub const NOTHING_CHANGED: &str = "Nothing changed.";

/// The line of an action while the action before it still runs.
pub const STILL_ACTING: &str = "riff still waits for the reply of the last action.";

impl Question {
    /// The question `text` with the action `yes`. After no, the note
    /// line says that nothing changed ([`NOTHING_CHANGED`]).
    ///
    /// ```
    /// use riff::tui::{Action, NOTHING_CHANGED, Question};
    ///
    /// let question = Question::new("Pause the riff?", Action::Look);
    /// assert_eq!(question.no, NOTHING_CHANGED);
    /// ```
    pub fn new(text: impl Into<String>, yes: Action) -> Question {
        Question {
            text: text.into(),
            yes,
            no: NOTHING_CHANGED.to_owned(),
        }
    }
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
    /// The holds of each board, by `OWNER/REPO` and item, while the Waves
    /// tab shows (01M6VFGK79JGRF919BM8SJ6E08). A read that fails keeps
    /// the last holds of its repository ([`State::looked`]).
    pub holds: BTreeMap<String, BTreeMap<String, HoldInfo>>,
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
            holds: BTreeMap::new(),
        }
    }
}

/// Looks at riff-server as `me`: the calls `riff` and `who` of
/// `riff top` ([`crate::top::look`], 01M6VAZK7N636Q0JFSV553A9ES), and
/// `plan show` for each repository of `boards`
/// (01M6VFGK79JGRF919BM8SJ6E08). Each call has the budget of a short
/// command, as in `riff top`. Then it asks the riff daemon of this host
/// for its status ([`machines::here`], 01M6VFD09QN71SWMPP8YXCRAEK).
pub async fn look(api: &Api, me: &SessionUri, boards: &[String]) -> Result<Look> {
    let (pauses, who, call) = crate::top::look(api, me).await?;
    let mut holds = BTreeMap::new();
    for repo in boards {
        let Ok(thread) = repo.parse::<ThreadName>() else {
            continue;
        };
        if let Ok(plan) = api.plan_show(me, &thread).await {
            holds.insert(repo.clone(), plan.holds);
        }
    }
    Ok(Look {
        pauses,
        who,
        server: riff_client::link::server_build(),
        call: Some(call),
        reconnects: api.link().swaps(),
        here: Some(machines::here(me).await),
        holds,
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
    /// The question in the note line, until the person answers it.
    question: Option<Question>,
    /// True while the help popup shows.
    help: bool,
    /// The build of this riff.
    build: Build,
    /// The limit that the person types in the note line, until `Enter`
    /// or `Esc`.
    typing: Option<machines::Typing>,
    /// The host that the person selected in the Machines tab.
    host: Option<machines::Name>,
    /// The session of the TUI: the person, its host, and the repository
    /// of its folder.
    me: SessionUri,
    /// The last status of the riff daemon of this machine, or why no
    /// daemon answers. `None` before the first ask.
    daemon: Option<Result<Status, String>>,
    /// The sessions model of the last look and of the last status of
    /// the daemon.
    model: Sessions,
    /// The issues of each repository from `gh`: the titles.
    titles: BTreeMap<String, Issues>,
    /// The last fault of `gh`, so the note line names it once.
    gh_fault: Option<String>,
    /// The sessions with a `!` or a `✓` at the last look.
    alerts: BTreeSet<String>,
    /// The choices of the person in the Sessions tab.
    sessions: sessions::Choices,
    /// The Chat tab: the log, the prompt and the focus.
    chat: chat::Chat,
    /// The Waves tab.
    waves: waves::Waves,
    /// The reason of a hold that the person types in the note line,
    /// until `Enter` or `Esc`.
    reason: Option<waves::Reason>,
    /// The Reports tab.
    reports: reports::Reports,
    /// The rows of a page of the body at the last draw.
    page: usize,
    /// The time of the last draw, in milliseconds since the epoch: the
    /// age in a note of a key.
    drawn_ms: i64,
    /// The filter of the Machines tab (01M6VJQPJXKHKJ8WT1CAQMN1YR).
    host_filter: machines::Filter,
}

impl State {
    /// The state at the start: the Sessions tab, no look yet, `build`,
    /// the build of this riff, for the header, and `me`, the session of
    /// the TUI.
    pub fn new(build: Build, me: SessionUri) -> State {
        State {
            tab: Tab::default(),
            news: [None; 6],
            look: None,
            fault: None,
            note: None,
            question: None,
            help: false,
            build,
            typing: None,
            host: None,
            me,
            daemon: None,
            model: Sessions::default(),
            titles: BTreeMap::new(),
            gh_fault: None,
            alerts: BTreeSet::new(),
            sessions: sessions::Choices::default(),
            chat: chat::Chat::default(),
            waves: waves::Waves::default(),
            reason: None,
            reports: reports::Reports::default(),
            page: 20,
            drawn_ms: 0,
            host_filter: machines::Filter::default(),
        }
    }

    /// The state of a TUI in the clone of `here`, `OWNER/REPO`: its
    /// repository gets a board in the Waves tab
    /// (01M6VFG882T794B1CTHK5AKPDG).
    #[must_use]
    pub fn in_clone(mut self, here: Option<String>) -> State {
        self.waves = waves::Waves::new(here);
        self
    }

    /// Takes the good look `look` at the time `at_ms`. The fault of the
    /// link goes (01M6VAZKAH6B53YKR8W4YKNSH6). A repository whose holds
    /// the look could not read keeps its last holds.
    pub fn looked(&mut self, mut look: Look, at_ms: i64) {
        if let Some((last, _)) = self.look.take() {
            for (repo, holds) in last.holds {
                look.holds.entry(repo).or_insert(holds);
            }
        }
        self.look = Some((look, at_ms));
        self.fault = None;
        self.model_again();
    }

    /// Takes the status of the riff daemon of this machine: `Err` says why
    /// no daemon answers. A `✓` that the person saw comes back after a new
    /// turn of its session.
    pub fn daemon_looked(&mut self, status: Result<Status, String>) {
        let ends: BTreeSet<&str> = status
            .iter()
            .flat_map(|s| &s.sessions)
            .filter(|c| c.phase == Phase::Ends)
            .map(|c| c.session.as_str())
            .collect();
        self.sessions.seen.retain(|id| ends.contains(id.as_str()));
        self.daemon = Some(status);
        self.model_again();
    }

    /// Takes a read of the issues of each repository from `gh`: a read
    /// that failed keeps the last issues of its repository, and the note
    /// line names a new fault once (01M3ZC09FA9DZPTHK31XECZ566).
    pub fn titled(&mut self, read: Titles) {
        let fault = read
            .iter()
            .find_map(|(_, read)| read.as_ref().err())
            .map(|e| text::top_gh_fault(e));
        if fault.is_some() && fault != self.gh_fault {
            self.note = fault.clone();
        }
        self.gh_fault = fault;
        let read = read
            .into_iter()
            .map(|(repo, read)| (repo, read.ok()))
            .collect();
        self.titles = Issues::newest_each(std::mem::take(&mut self.titles), read);
    }

    /// Makes the sessions model again from the last look and the last
    /// status of the daemon, and gives the Sessions tab the news `•` for
    /// a session that turned `!` or `✓`.
    fn model_again(&mut self) {
        let server = self
            .look
            .as_ref()
            .map(|(look, _)| look.who.sessions.clone())
            .unwrap_or_default();
        let containers = match &self.daemon {
            Some(Ok(status)) => status.sessions.clone(),
            _ => Vec::new(),
        };
        self.model = Sessions::new(server, containers);
        let alerts: BTreeSet<String> = self
            .model
            .entries
            .iter()
            .filter(|entry| {
                let state = entry.server.as_ref().map(crate::state::of);
                matches!(
                    Mark::of(state, entry.phase()),
                    Some(Mark::Blocked | Mark::Done)
                )
            })
            .filter_map(|entry| entry.id().map(str::to_owned))
            .collect();
        if alerts.iter().any(|id| !self.alerts.contains(id)) {
            self.news(Tab::Sessions, News::Dot);
        }
        self.alerts = alerts;
    }

    /// Takes the read `read` of the forge for the Waves tab at the time
    /// `at_ms` ([`waves::Waves::read`]): a new fault of `gh` goes to the
    /// note line, and an item that turned stuck gives the tab news.
    pub fn read_forge(&mut self, read: Vec<(String, Result<forge::Forge, String>)>, at_ms: i64) {
        let (fault, stuck) = self.waves.read(read, at_ms);
        if let Some(fault) = fault {
            self.note(fault);
        }
        if stuck {
            self.news(Tab::Waves, News::Dot);
        }
    }

    /// The repositories of the boards of the Waves tab, while it shows:
    /// the look reads their holds, and the forge reads their items.
    pub fn boards(&self) -> Vec<String> {
        match (self.tab, self.last_look()) {
            (Tab::Waves, Some((look, _))) => self.waves.repos(look),
            _ => Vec::new(),
        }
    }

    /// The Waves tab.
    pub fn waves(&self) -> &waves::Waves {
        &self.waves
    }

    /// The rows of the Waves tab now.
    pub fn wave_rows(&self) -> Vec<waves::Row> {
        match self.last_look() {
            Some((look, at_ms)) => waves::rows(&self.waves, look, at_ms),
            None => Vec::new(),
        }
    }

    /// Takes the size of the body of the last draw, `rows` rows, and its
    /// time `at_ms`.
    pub fn drew(&mut self, rows: u16, at_ms: i64) {
        self.drawn_ms = at_ms;
        self.page = usize::from(rows).max(1);
    }

    /// Takes the fault of a look that failed: the screen keeps the last
    /// good look, the header says `link down`, and the note line shows
    /// `fault` until the next good look (01M6VAZKAH6B53YKR8W4YKNSH6).
    ///
    /// ```
    /// use riff::tui::{Look, State};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// state.looked(Look::default(), 0);
    /// state.failed("riff: no good look since 21:35:07: cannot reach riff-server");
    /// assert!(state.link_down());
    /// assert_eq!(state.shown_note(), Some("riff: no good look since 21:35:07: cannot reach riff-server"));
    /// state.looked(Look::default(), 3000);
    /// assert!(!state.link_down());
    /// assert_eq!(state.shown_note(), None);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn failed(&mut self, fault: impl Into<String>) {
        self.fault = Some(fault.into());
    }

    /// Shows `note` in the note line: the result of an action.
    pub fn note(&mut self, note: impl Into<String>) {
        self.note = Some(note.into());
    }

    /// Shows the end of the attach to the lead `name`
    /// (01M51PD7Q24N252V66NFEHY49N): a detach, the end of the lead, or
    /// the error of the attach.
    ///
    /// ```
    /// use riff::lead::Left;
    /// use riff::tui::State;
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// state.attached("n", &Ok(Left::Ended));
    /// assert_eq!(state.shown_note(), Some(riff::text::lead_ended("n").as_str()));
    /// state.attached("n", &Ok(Left::Detached));
    /// assert_eq!(state.shown_note(), Some(riff::text::lead_detached("n").as_str()));
    /// state.attached("n", &Err(anyhow::anyhow!("no podman")));
    /// assert_eq!(state.shown_note(), Some("no podman"));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn attached(&mut self, name: &str, left: &Result<crate::lead::Left>) {
        self.note(match left {
            Ok(left) => left.line(name),
            Err(e) => format!("{e:#}"),
        });
    }

    /// Shows the end of the takeover of `name` with the exit code
    /// `taken` of [`crate::takeover::takeover`]: 0 when the session goes
    /// on, 1 when it ended before the takeover.
    ///
    /// ```
    /// use riff::tui::State;
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// state.took_over("n", &Ok(1));
    /// assert_eq!(state.shown_note(), Some(riff::text::takeover_session_ended("n").as_str()));
    /// state.took_over("n", &Ok(0));
    /// assert_eq!(state.shown_note(), Some(riff::text::takeover_back("n").as_str()));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn took_over(&mut self, name: &str, taken: &Result<i32>) {
        self.note(match taken {
            Ok(0) => text::takeover_back(name),
            Ok(_) => text::takeover_session_ended(name),
            Err(e) => format!("{e:#}"),
        });
    }

    /// Shows the result of an action that gives a line, or its error.
    pub fn done(&mut self, result: Result<String>) {
        self.note(match result {
            Ok(line) => line,
            Err(e) => format!("{e:#}"),
        });
    }

    /// Gives the tab `tab` the news `news`. The tab that the person sees
    /// gets none: the person sees it.
    ///
    /// ```
    /// use riff::tui::{Key, News, State, Tab};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// state.news(Tab::Chat, News::Lines(2));
    /// state.news(Tab::Sessions, News::Dot);
    /// assert_eq!(state.news_of(Tab::Chat), Some(News::Lines(2)));
    /// assert_eq!(state.news_of(Tab::Sessions), None);
    /// // The news of a tab goes when the person opens it.
    /// state.on_key(Key::Open(Tab::Chat));
    /// assert_eq!(state.news_of(Tab::Chat), None);
    /// # Ok::<(), riff_core::name::NameError>(())
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

    /// The session of the TUI.
    pub fn me(&self) -> &SessionUri {
        &self.me
    }

    /// The sessions model of the last look and status.
    pub fn model(&self) -> &Sessions {
        &self.model
    }

    /// The choices of the person in the Sessions tab.
    pub fn choices(&self) -> &sessions::Choices {
        &self.sessions
    }

    /// The fault of the last look, while the link is down.
    pub fn fault(&self) -> Option<&str> {
        self.fault.as_deref()
    }

    /// The fault of the link, else the note of the last action, else in
    /// the Sessions tab with no riff daemon here: that no daemon answers.
    /// A question of the note line comes before them, and in the Chat
    /// tab the notes of the chat come between them
    /// (01M6VMACJGF4A0ZXC6YK13KNCH, [`State::asked`]).
    pub fn shown_note(&self) -> Option<&str> {
        self.fault.as_deref().or(self.note.as_deref()).or_else(|| {
            (self.tab == Tab::Sessions && matches!(self.daemon, Some(Err(_))))
                .then_some(text::NO_DAEMON_HERE)
        })
    }

    /// True while the help popup shows.
    pub fn help_shown(&self) -> bool {
        self.help
    }

    /// Asks `question` in the note line (01M6VFD0S90SB8M8WV952NT4Y1).
    pub fn ask(&mut self, question: Question) {
        self.question = Some(question);
    }

    /// Asks for the reason of a hold in the note line: the person types
    /// it ([`waves::typed`], 01M6VFGK9XXWCA8X7CE8EZNSJV).
    pub fn ask_reason(&mut self, reason: waves::Reason) {
        self.reason = Some(reason);
    }

    /// The text of the note line while the TUI asks: the question with
    /// `y/N`, or what the person types with a cursor: the limit of a host
    /// or the reason of a hold. It comes before the fault and the note
    /// (01M6VMACJGF4A0ZXC6YK13KNCH).
    ///
    /// ```
    /// use riff::tui::{Action, Key, Question, State};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
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
    /// // A question with no note of its own says that nothing changed.
    /// state.ask(Question::new("Pause the riff?", Action::Look));
    /// assert_eq!(state.on_key(Key::Char('n')), None);
    /// assert_eq!(state.shown_note(), Some(riff::tui::NOTHING_CHANGED));
    /// // q and Ctrl-C end the TUI.
    /// state.ask(Question { text: "Set the limit?".into(), yes: yes.clone(), no: no.clone() });
    /// assert_eq!(state.on_key(Key::Char('q')), Some(Action::Quit));
    /// state.ask(Question { text: "Set the limit?".into(), yes, no });
    /// assert_eq!(state.on_key(Key::Quit), Some(Action::Quit));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn asked(&self) -> Option<String> {
        if let Some(question) = &self.question {
            return Some(format!("{} y/N", question.text));
        }
        if let Some(reason) = &self.reason {
            return Some(reason.line());
        }
        if self.host_filter.typing {
            return Some(self.host_filter.line());
        }
        self.typing.as_ref().map(machines::Typing::line)
    }

    /// The filter of the Machines tab (01M6VJQPJXKHKJ8WT1CAQMN1YR).
    pub fn host_filter(&self) -> &machines::Filter {
        &self.host_filter
    }

    /// True while the person types a filter of the Sessions tab or of
    /// the Machines tab: each character is a letter of the filter
    /// ([`State::on_key`], 01M6VJQPNJ4AX4RT877F1QAPPX).
    ///
    /// ```
    /// use riff::tui::{Key, State, Tab};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// state.on_key(Key::Open(Tab::Machines));
    /// assert!(!state.takes_text());
    /// state.on_key(Key::Char('/'));
    /// assert!(state.takes_text());
    /// state.on_key(Key::Enter);
    /// assert!(!state.takes_text());
    /// state.on_key(Key::Open(Tab::Sessions));
    /// state.on_key(Key::Char('/'));
    /// assert!(state.takes_text());
    /// state.on_key(Key::Esc);
    /// assert!(!state.takes_text());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn takes_text(&self) -> bool {
        self.question.is_none()
            && self.typing.is_none()
            && self.reason.is_none()
            && (self.host_filter.typing || self.tab == Tab::Sessions && self.sessions.typing())
    }

    /// Opens the tab `tab`: its news goes. The prompt of the Chat tab
    /// gets the focus.
    fn open(&mut self, tab: Tab) {
        self.help = false;
        self.tab = tab;
        self.news[tab as usize] = None;
        if tab == Tab::Chat {
            self.on_chat(chat::Key::Prompt);
        }
    }

    /// Takes the key `key` of the person (01M6VAZJYYQZDHD6YDB0NBQ7KW,
    /// 01M6VAZKDEQ6951KGPGAMF1SNJ). A question of the note line takes the
    /// key first, then a text that the person types: the limit of a host
    /// ([`machines::typed`]), the reason of a hold ([`waves::typed`]), or
    /// the filter of the Sessions tab. Then the frame takes its keys, and
    /// the tab the others: `k` `j` `g` `G` are `↑` `↓` `Home` `End`
    /// (01M6VFGKCJ02G33TXR01GY6M0G). Opening the Waves tab looks again at
    /// once, for the holds of its boards.
    ///
    /// ```
    /// use riff::tui::{Action, Key, State, Tab};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// assert_eq!(state.on_key(Key::Open(Tab::Machines)), None);
    /// assert_eq!(state.tab(), Tab::Machines);
    /// assert_eq!(state.on_key(Key::Char('3')), Some(Action::Look));
    /// assert_eq!(state.tab(), Tab::Waves);
    /// state.on_key(Key::Next);
    /// assert_eq!(state.tab(), Tab::Machines);
    /// state.on_key(Key::Before);
    /// state.on_key(Key::Open(Tab::Chat));
    /// assert_eq!(state.tab(), Tab::Chat);
    /// // `?` shows the help, and `?` or Esc closes it.
    /// state.on_key(Key::Char('?'));
    /// assert!(state.help_shown());
    /// state.on_key(Key::Esc);
    /// assert!(!state.help_shown());
    /// state.on_key(Key::Help);
    /// state.on_key(Key::Help);
    /// assert!(!state.help_shown());
    /// assert_eq!(state.on_key(Key::Char('r')), Some(Action::Look));
    /// assert_eq!(state.on_key(Key::Again), Some(Action::Look));
    /// assert_eq!(state.on_key(Key::Char('q')), Some(Action::Quit));
    /// assert_eq!(state.on_key(Key::Quit), Some(Action::Quit));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn on_key(&mut self, key: Key) -> Option<Action> {
        // A post that failed comes before the note of the prompt until
        // the next key (01M6VMACJGF4A0ZXC6YK13KNCH).
        self.chat.warning = None;
        // A question takes the next key (01M6VFD0S90SB8M8WV952NT4Y1).
        if let Some(question) = self.question.take() {
            return match key {
                Key::Char('y' | 'Y') => {
                    self.note = None;
                    Some(question.yes)
                }
                Key::Quit | Key::Char('q') => Some(Action::Quit),
                _ => {
                    self.note(question.no);
                    None
                }
            };
        }
        // A text takes each character (01M6VFGKCJ02G33TXR01GY6M0G).
        if let Some(typing) = self.typing.take() {
            return machines::typed(self, typing, key);
        }
        if let Some(reason) = self.reason.take() {
            return waves::typed(self, reason, key);
        }
        // A filter takes `1` to `6` and `q` too
        // (01M6VJQPNJ4AX4RT877F1QAPPX).
        if self.tab == Tab::Sessions && self.sessions.typing() && key != Key::Quit {
            return self.on_sessions_key(key);
        }
        if self.host_filter.typing {
            return machines::filter_typed(self, key);
        }
        let key = match key {
            Key::Char(digit @ '1'..='6') => digit
                .to_digit(10)
                .and_then(|n| Tab::of_number(n as usize))
                .map_or(key, Key::Open),
            Key::Char('?') => Key::Help,
            Key::Char('r') => Key::Again,
            Key::Char('q') => Key::Quit,
            Key::Char('k') => Key::Up,
            Key::Char('j') => Key::Down,
            Key::Char('g') => Key::First,
            Key::Char('G') => Key::Last,
            key => key,
        };
        match key {
            Key::Open(tab) => self.go(tab),
            Key::Next => self.go(self.tab.next()),
            Key::Before => self.go(self.tab.before()),
            Key::Help => {
                self.help = !self.help;
                None
            }
            // Esc closes the popup first; else the tab takes it
            // (01M6VJQPJXKHKJ8WT1CAQMN1YR).
            Key::Esc if self.help => {
                self.help = false;
                None
            }
            Key::Again => {
                if self.tab == Tab::Reports {
                    self.reports_again();
                }
                Some(Action::Look)
            }
            Key::Quit => Some(Action::Quit),
            // The popup takes no key of a tab.
            _ if self.help => None,
            Key::Chat(key) => self.on_chat(key),
            key => match self.tab {
                Tab::Sessions => self.on_sessions_key(key),
                Tab::Waves => self.waves_key(key),
                Tab::Machines => machines::on_key(self, key),
                Tab::Reports => self.on_reports_key(key),
                _ => None,
            },
        }
    }

    /// Opens the tab `tab` ([`State::open`]). The Waves tab looks again
    /// at once.
    fn go(&mut self, tab: Tab) -> Option<Action> {
        let was = self.tab;
        self.open(tab);
        (tab == Tab::Waves && was != Tab::Waves).then_some(Action::Look)
    }

    /// Takes a key of the Waves tab that the frame does not take.
    fn waves_key(&mut self, key: Key) -> Option<Action> {
        let rows = self.wave_rows();
        let (look, at_ms) = self.last_look()?;
        let look = look.clone();
        let at = (&look, self.drawn_ms.max(at_ms));
        match self.waves.on_key(key, &rows, at, self.page)? {
            waves::Answer::Note(note) => self.note(note),
            waves::Answer::Ask(question) => self.ask(question),
            waves::Answer::Reason(reason) => self.ask_reason(reason),
        }
        None
    }

    /// Takes a click of the person at `column` and `row` on the view
    /// `view`: it presses the key of the place that it hits
    /// (01M6VAZKG8FS4HEA9VSA4Z4YF1). A click on no place does nothing.
    /// With `double`, the second click of a double click on a row also
    /// presses Enter.
    pub fn on_click(
        &mut self,
        view: &view::View,
        column: u16,
        row: u16,
        double: bool,
    ) -> Option<Action> {
        let key = view.hit(column, row)?;
        // In the Waves tab, the second click is on the selected row: it
        // acts as Enter already (01M6VFGKCJ02G33TXR01GY6M0G).
        let enter = double && matches!(key, Key::Row(_)) && self.tab != Tab::Waves;
        let action = self.on_key(key);
        if enter {
            return self.on_key(Key::Enter);
        }
        action
    }

    /// Keeps what the person saw in `view`, the view of the last draw:
    /// the window of the tree of the Sessions tab, its first row and the
    /// rows that fit for `PgUp` and `PgDn`, and the rows of the log of
    /// the Chat tab for a scroll.
    pub fn drawn(&mut self, view: &view::View) {
        (self.sessions.top, self.sessions.page) = (view.top, view.page);
        self.sessions.pane = view.pane;
        if let Some(window) = view.chat {
            self.chat.window = window;
        }
        if let Some(window) = view.reports {
            self.reports.window = window;
            self.reports.top = window.top;
        }
    }

    /// The input of the person in `event`. In the Chat tab, the keys of
    /// the tab come first ([`chat::key_of`]), then the keys of the frame
    /// ([`input_of`]). The help popup takes only the keys of the frame.
    ///
    /// ```
    /// use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    /// use riff::tui::{Action, Input, Key, State, Tab, chat};
    /// use riff_core::build::Build;
    ///
    /// let q = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// assert_eq!(state.input(&q), Some(Input::Key(Key::Char('q'))));
    /// // In the prompt of the Chat tab, `q` types.
    /// state.on_key(Key::Open(Tab::Chat));
    /// assert_eq!(state.input(&q), Some(Input::Key(Key::Chat(chat::Key::Type('q')))));
    /// // In the log, `q` goes to the frame, and ends the TUI.
    /// state.on_key(Key::Chat(chat::Key::Log));
    /// assert_eq!(state.input(&q), Some(Input::Key(Key::Char('q'))));
    /// assert_eq!(state.on_key(Key::Char('q')), Some(Action::Quit));
    /// // In a filter of the Machines tab, `q` is a letter.
    /// state.on_key(Key::Open(Tab::Machines));
    /// state.on_key(Key::Char('/'));
    /// assert_eq!(state.input(&q), Some(Input::Key(Key::Char('q'))));
    /// assert_eq!(state.on_key(Key::Char('q')), None);
    /// assert_eq!(state.asked().as_deref(), Some("/q█"));
    /// # Ok::<(), riff_core::name::NameError>(())
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

    /// Takes the text `text` of a paste of the person: a bracketed paste
    /// of the terminal (01M6XSK4GHNBJCR0RHMBGZE57G). In the Chat tab, the
    /// text goes into the prompt, with its new lines ([`chat`]). A text
    /// of one line takes it with each new line and tab as a space: a
    /// filter, the reason of a hold, and the digits of the limit of a
    /// host. A paste answers no question and is no key of a tab: else it
    /// does nothing.
    ///
    /// ```
    /// use riff::tui::waves::Reason;
    /// use riff::tui::{Effect, Key, State, Tab, chat};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// // In the Sessions tab, a paste is no key: `x` ends no session.
    /// state.on_paste("x");
    /// assert_eq!(state.choices().filter(), "");
    /// // The filter takes a paste on one line.
    /// state.on_key(Key::Char('/'));
    /// state.on_paste("host:thelio\nbusy");
    /// assert_eq!(state.choices().filter(), "host:thelio busy");
    /// state.on_key(Key::Enter);
    /// // So does the reason of a hold, and a paste of `y` answers no
    /// // question.
    /// state.ask_reason(Reason { repo: "o/r".into(), item: 12, text: String::new() });
    /// state.on_paste("waits\tfor\r\nmike");
    /// assert_eq!(state.asked().as_deref(), Some("Hold #12 of o/r. The reason: waits for mike█"));
    /// state.on_key(Key::Esc);
    /// let hold = Effect::Hold { repo: "o/r".into(), item: 12, reason: "r".into() };
    /// state.ask(hold.question());
    /// assert_eq!(state.on_paste("y"), None);
    /// assert_eq!(state.asked().as_deref(), Some("Hold #12 of o/r: r? y/N"));
    /// state.on_key(Key::Esc);
    /// // In the Chat tab, also from the log, the paste goes into the prompt.
    /// state.on_key(Key::Open(Tab::Chat));
    /// state.on_key(Key::Chat(chat::Key::Log));
    /// state.on_paste("one\r\ntwo");
    /// assert_eq!(state.chat().focus(), chat::Focus::Prompt);
    /// assert_eq!(state.chat().prompt().text(), "one\ntwo");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn on_paste(&mut self, text: &str) -> Option<Action> {
        self.chat.warning = None;
        if self.question.is_some() {
            return None;
        }
        let one_line = self.typing.is_some()
            || self.reason.is_some()
            || self.host_filter.typing
            || self.tab == Tab::Sessions && self.sessions.typing();
        if one_line {
            let digits = self.typing.is_some();
            let chars = chat::pasted(text)
                .chars()
                .map(|c| if matches!(c, '\n' | '\t') { ' ' } else { c })
                .filter(|c| !digits || c.is_ascii_digit())
                .collect::<Vec<_>>();
            for c in chars {
                self.on_key(Key::Char(c));
            }
        } else if self.tab == Tab::Chat && !self.help {
            self.chat_paste(text);
        }
        None
    }
}

/// What the person does: a key, a click of the mouse at a place, a
/// paste, or a switch back to the window of the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A key.
    Key(Key),
    /// A click of the left button at a column and a row.
    Click { column: u16, row: u16 },
    /// The text of a bracketed paste ([`State::on_paste`]).
    Paste(String),
    /// The window of the terminal got the focus back ([`State::focused`],
    /// 01M6XT6MCZ38XEN7ZHWTRF50Z5).
    Focused,
}

/// The input of the person in `event`: a key, a click, a paste or the
/// return of the focus, or `None` for another event. The loss of the
/// focus is no input.
///
/// ```
/// use ratatui::crossterm::event::{
///     Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
/// };
/// use riff::tui::{Input, Key, Tab, input_of};
///
/// let press = |code, modifiers| Event::Key(KeyEvent::new(code, modifiers));
/// let none = KeyModifiers::NONE;
/// let key = |code, modifiers| input_of(&press(code, modifiers));
/// assert_eq!(key(KeyCode::Char('3'), none), Some(Input::Key(Key::Char('3'))));
/// assert_eq!(key(KeyCode::Char('6'), KeyModifiers::ALT), Some(Input::Key(Key::Open(Tab::Reports))));
/// assert_eq!(key(KeyCode::Tab, none), Some(Input::Key(Key::Next)));
/// assert_eq!(key(KeyCode::BackTab, KeyModifiers::SHIFT), Some(Input::Key(Key::Before)));
/// assert_eq!(key(KeyCode::Char('?'), KeyModifiers::SHIFT), Some(Input::Key(Key::Char('?'))));
/// assert_eq!(key(KeyCode::Char('P'), KeyModifiers::SHIFT), Some(Input::Key(Key::Char('P'))));
/// assert_eq!(key(KeyCode::Esc, none), Some(Input::Key(Key::Esc)));
/// assert_eq!(key(KeyCode::Char('c'), KeyModifiers::CONTROL), Some(Input::Key(Key::Quit)));
/// // The keys of a tab (01M6VFD0M3RHT488D0RQ84RVMC, 01M6VFGKCJ02G33TXR01GY6M0G).
/// for (code, want) in [
///     (KeyCode::Up, Key::Up),
///     (KeyCode::Down, Key::Down),
///     (KeyCode::PageUp, Key::PageUp),
///     (KeyCode::PageDown, Key::PageDown),
///     (KeyCode::Home, Key::First),
///     (KeyCode::End, Key::Last),
///     (KeyCode::Left, Key::Left),
///     (KeyCode::Right, Key::Right),
///     (KeyCode::Enter, Key::Enter),
///     (KeyCode::Backspace, Key::Backspace),
///     (KeyCode::Char('j'), Key::Char('j')),
///     (KeyCode::Char('l'), Key::Char('l')),
/// ] {
///     assert_eq!(key(code, none), Some(Input::Key(want)), "{code:?}");
/// }
/// // A letter with Ctrl or Alt is no key of a tab.
/// assert_eq!(key(KeyCode::Char('7'), KeyModifiers::ALT), None);
/// assert_eq!(key(KeyCode::Char('l'), KeyModifiers::CONTROL), None);
/// assert_eq!(key(KeyCode::F(1), none), None);
/// let mouse = |kind| {
///     Event::Mouse(MouseEvent { kind, column: 4, row: 0, modifiers: KeyModifiers::NONE })
/// };
/// assert_eq!(
///     input_of(&mouse(MouseEventKind::Down(MouseButton::Left))),
///     Some(Input::Click { column: 4, row: 0 })
/// );
/// // The wheel moves the selection.
/// assert_eq!(input_of(&mouse(MouseEventKind::ScrollDown)), Some(Input::Key(Key::Down)));
/// assert_eq!(input_of(&mouse(MouseEventKind::ScrollUp)), Some(Input::Key(Key::Up)));
/// assert_eq!(input_of(&mouse(MouseEventKind::Moved)), None);
/// // A bracketed paste is one input, with its new lines.
/// assert_eq!(
///     input_of(&Event::Paste("one\ntwo".into())),
///     Some(Input::Paste("one\ntwo".into()))
/// );
/// // The terminal sends `CSI I` when its window gets the focus back.
/// assert_eq!(input_of(&Event::FocusGained), Some(Input::Focused));
/// assert_eq!(input_of(&Event::FocusLost), None);
/// ```
pub fn input_of(event: &ratatui::crossterm::event::Event) -> Option<Input> {
    use ratatui::crossterm::event::{
        Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
    };
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            let control = key.modifiers.contains(KeyModifiers::CONTROL);
            let alt = key.modifiers.contains(KeyModifiers::ALT);
            let key = match key.code {
                KeyCode::Char('c') if control => Key::Quit,
                KeyCode::Char(digit @ '1'..='6') if alt => {
                    Key::Open(Tab::of_number(digit.to_digit(10)? as usize)?)
                }
                KeyCode::Char(c) if !control && !alt => Key::Char(c),
                KeyCode::Tab => Key::Next,
                KeyCode::BackTab => Key::Before,
                KeyCode::Esc => Key::Esc,
                KeyCode::Up => Key::Up,
                KeyCode::Down => Key::Down,
                KeyCode::PageUp => Key::PageUp,
                KeyCode::PageDown => Key::PageDown,
                KeyCode::Home => Key::First,
                KeyCode::End => Key::Last,
                KeyCode::Left => Key::Left,
                KeyCode::Right => Key::Right,
                KeyCode::Enter => Key::Enter,
                KeyCode::Backspace => Key::Backspace,
                _ => return None,
            };
            Some(Input::Key(key))
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => Some(Input::Click {
                column: mouse.column,
                row: mouse.row,
            }),
            MouseEventKind::ScrollDown => Some(Input::Key(Key::Down)),
            MouseEventKind::ScrollUp => Some(Input::Key(Key::Up)),
            _ => None,
        },
        Event::Paste(text) => Some(Input::Paste(text.clone())),
        Event::FocusGained => Some(Input::Focused),
        _ => None,
    }
}

/// How long [`next_event`] waits for a key or a click. The age of the
/// look in the header draws again after each wait.
const INPUT_WAIT: Duration = Duration::from_millis(200);

/// The next event of the terminal, or `None` when none comes in
/// [`INPUT_WAIT`]. The blocking reads of crossterm run in a blocking
/// task of tokio, not in the async loop. The task ends after
/// [`INPUT_WAIT`], so no read of the TUI takes a key from an attach or a
/// takeover (01M50RRQBZ8ASPT25DYPX78HP9).
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

/// Takes the terminal for the TUI, with its bracketed paste
/// (01M6XSK4GHNBJCR0RHMBGZE57G), its focus events
/// (01M6XT6MCZ38XEN7ZHWTRF50Z5), and with `mouse`, the mouse
/// (01M6VAZKG8FS4HEA9VSA4Z4YF1). A terminal with no bracketed paste
/// gives a paste as keys.
fn enter(mouse: bool) -> Result<ratatui::DefaultTerminal> {
    use ratatui::crossterm::event::{EnableBracketedPaste, EnableFocusChange, EnableMouseCapture};
    use ratatui::crossterm::execute;
    let terminal = ratatui::try_init()?;
    let mut out = std::io::stdout();
    let _ = execute!(out, EnableBracketedPaste);
    let taken = execute!(out, EnableFocusChange).and_then(|()| {
        if mouse {
            execute!(out, EnableMouseCapture)
        } else {
            Ok(())
        }
    });
    if let Err(e) = taken {
        leave(mouse);
        return Err(e.into());
    }
    Ok(terminal)
}

/// Gives the terminal back: for an attach, a takeover, or the end.
fn leave(mouse: bool) {
    use ratatui::crossterm::event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture,
    };
    use ratatui::crossterm::execute;
    if mouse {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
    let _ = execute!(std::io::stdout(), DisableBracketedPaste);
    let _ = execute!(std::io::stdout(), DisableFocusChange);
    ratatui::restore();
}

/// `riff` with no command, in a terminal: the TUI, as `me` at `api`,
/// until the person ends it (01M6VAZ7WH3MFAYF4PAZW6EDVY). The repository
/// of the folder `here` gets a board in the Waves tab. The Chat tab posts
/// as `person`, as `riff chat` does. An action of the Waves tab reaches
/// riff-server through `reach`, as its CLI command does. With `mouse`,
/// the TUI takes the mouse (01M6VAZKG8FS4HEA9VSA4Z4YF1).
pub async fn run(
    api: &Api,
    me: &SessionUri,
    here: &riff_core::name::Place,
    person: &chat::Person,
    reach: &Reach,
    mouse: bool,
) -> Result<()> {
    // The first look runs before the TUI takes the terminal: its error
    // prints as in the CLI (01M6VAZKAH6B53YKR8W4YKNSH6).
    let first = look(api, me, &[]).await?;
    let here = here.default_thread().map(|t| t.to_string());
    let mut state = State::new(Build::this(), me.clone()).in_clone(here);
    let start = crate::run::now_ms();
    state.looked(first, start);
    state.chat_since(u64::try_from(start).unwrap_or(0));
    let mut terminal = enter(mouse)?;
    let shown = frames(&mut terminal, &mut state, api, me, person, reach, mouse).await;
    leave(mouse);
    shown
}

/// A read of the titles of the issues of each repository from `gh`: the
/// issues, or the error of `gh`.
type Titles = Vec<(String, Result<Issues, String>)>;

/// A read of the forge for the Waves tab: the read of each repository.
type Boards = Vec<(String, Result<forge::Forge, String>)>;

/// A read of `gh` on a thread of its own: the end of the TUI waits for
/// no read (#1707). A thread that nobody joins holds up no exit.
struct Reading<T>(std::thread::JoinHandle<T>);

impl<T: Send + 'static> Reading<T> {
    /// Starts the read `read` on a thread of its own.
    fn start(read: impl FnOnce() -> T + Send + 'static) -> Reading<T> {
        Reading(std::thread::spawn(read))
    }

    /// True when the read ended.
    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    /// The result of the read, once it ended. A read that panicked is an
    /// error.
    fn result(self) -> Result<T> {
        self.0
            .join()
            .map_err(|_| anyhow::anyhow!("a read of gh panicked"))
    }
}

/// The tasks of the loop of the TUI: each look that runs, and when the
/// next one starts.
struct Tasks {
    /// The look of riff-server that runs.
    looking: Option<tokio::task::JoinHandle<Result<Look>>>,
    /// The time of the next look, and of the start of the last one.
    next: tokio::time::Instant,
    started: tokio::time::Instant,
    /// `r` while a look runs: look again after it.
    again: bool,
    /// The ask of the riff daemon of this machine that runs, and the
    /// time of the next one.
    asking: Option<tokio::task::JoinHandle<Result<Status, String>>>,
    next_ask: tokio::time::Instant,
    /// The read of the titles from `gh` that runs, and the time of the
    /// last one.
    reading: Option<Reading<Titles>>,
    read_at: Option<tokio::time::Instant>,
    /// The read of the forge for the Waves tab that runs.
    forge: Option<Reading<Boards>>,
    /// The kept replies of the conditional reads of `gh`: the Sessions
    /// tab and the Waves tab share them.
    kept: forge::Kept,
    /// `r` while the Waves tab shows: read the forge again.
    forge_again: bool,
    /// The action that runs in a task of its own: its result is the
    /// next note.
    acting: Option<tokio::task::JoinHandle<String>>,
    /// The read of the usage of the Reports tab that runs, with the wave
    /// that it reads.
    usage: Option<Asked<reports::Usage>>,
    /// The audit of the Reports tab that runs, with the wave that it
    /// audits.
    audit: Option<Asked<reports::Audited>>,
    /// The follow of the Session log that runs, its session, and its
    /// lines.
    follow: Option<Following>,
}

/// A read of a report of the Reports tab that runs, and the wave that it
/// reads.
type Asked<T> = (tokio::task::JoinHandle<Result<T, String>>, Option<u64>);

/// The follow of a Session log that runs: its task, its session, and its
/// lines.
type Following = (
    tokio::task::JoinHandle<Result<bool>>,
    reports::Source,
    tokio::sync::mpsc::UnboundedReceiver<String>,
);

impl Tasks {
    /// Stops each task that runs: the TUI ends. A read of `gh` goes on
    /// ([`Reading`]).
    fn abort(&self) {
        if let Some((task, _)) = &self.usage {
            task.abort();
        }
        if let Some((task, _)) = &self.audit {
            task.abort();
        }
        if let Some((task, ..)) = &self.follow {
            task.abort();
        }
        if let Some(task) = &self.looking {
            task.abort();
        }
        if let Some(task) = &self.asking {
            task.abort();
        }
        if let Some(task) = &self.acting {
            task.abort();
        }
    }
}

/// The loop of [`run`]: it draws the state, takes the input of the
/// person, does the actions, and looks at riff-server each [`every`].
/// While the Sessions tab is open, it asks the daemon of this machine and
/// reads the titles from `gh`. While the Waves tab shows, it reads the
/// forge each [`forge::READ_EVERY`]. The [`chat::Link`] of `person`
/// follows the thread `chat` and posts its lines.
async fn frames(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut State,
    api: &Api,
    me: &SessionUri,
    person: &chat::Person,
    reach: &Reach,
    mouse: bool,
) -> Result<()> {
    use futures::StreamExt;
    use tokio::time::Instant;
    let dir = crate::daemon::dir().ok();
    // The calls of `gh` share one token, and count as `tui`
    // (01M6VFGK4P2FWMFQHBZKR3JXSZ).
    let gh = crate::pr::Gh::here("tui")
        .with_one_token()
        .with_shared_reads();
    let mut input_at = Instant::now();
    let mut tasks = Tasks {
        looking: None,
        next: Instant::now() + LOOK_EVERY,
        started: Instant::now(),
        again: false,
        asking: None,
        next_ask: Instant::now(),
        reading: None,
        read_at: None,
        forge: None,
        kept: forge::Kept::default(),
        forge_again: false,
        acting: None,
        usage: None,
        audit: None,
        follow: None,
    };
    let mut good_at = chrono::Local::now();
    let mut link = chat::Link::start(person);
    // A message of the thread of the repository of the folder starts a
    // look, as in `riff top`.
    let (told, mut messages) = tokio::sync::mpsc::channel::<()>(1);
    let follower = me.default_thread().map(|thread| {
        let (api, me) = (api.clone(), me.clone());
        tokio::spawn(async move {
            let stream = riff_client::api::follow(
                || api.tail(&me, &thread),
                riff_client::link::STREAM_RETRY,
            );
            let mut stream = Box::pin(stream);
            while let Some(item) = stream.next().await {
                if item.is_ok() && told.send(()).await.is_err() {
                    return;
                }
            }
        })
    });
    let mut last_click: Option<(Key, Instant)> = None;
    let shown = loop {
        for came in link.came() {
            state.chat_came(came);
        }
        if tasks.forge.as_ref().is_some_and(Reading::is_finished)
            && let Some(done) = tasks.forge.take()
        {
            state.read_forge(done.result()?, crate::run::now_ms());
        }
        if tasks
            .acting
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
            && let Some(done) = tasks.acting.take()
        {
            state.note(done.await?);
            // The action changed something: look again, and read the
            // forge again while the Waves tab shows.
            tasks.forge_again = true;
            if tasks.looking.is_some() {
                tasks.again = true;
            } else {
                tasks.next = Instant::now();
            }
        }
        reports_tasks(&mut tasks, state, api, me, reach, &gh).await?;
        if tasks.forge.is_none()
            && state.tab() == Tab::Waves
            && (tasks.forge_again || state.waves().due(crate::run::now_ms()))
        {
            tasks.forge_again = false;
            let repos = state.boards();
            let (gh, kept) = (gh.clone(), tasks.kept.clone());
            tasks.forge = Some(Reading::start(move || forge::read_each(&gh, &repos, &kept)));
        }
        if tasks
            .looking
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
            && let Some(done) = tasks.looking.take()
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
                Err(e) => break Err(e),
            }
            tasks.next = if std::mem::take(&mut tasks.again) {
                Instant::now()
            } else {
                Instant::now() + every(input_at.elapsed())
            };
        }
        if messages.try_recv().is_ok() {
            tasks.next = tasks.next.min(tasks.started + THREAD_LOOK_EVERY);
        }
        if tasks.looking.is_none() && Instant::now() >= tasks.next {
            let (api, me, boards) = (api.clone(), me.clone(), state.boards());
            tasks.started = Instant::now();
            tasks.looking = Some(tokio::spawn(async move { look(&api, &me, &boards).await }));
        }
        if tasks
            .asking
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
            && let Some(done) = tasks.asking.take()
        {
            state.daemon_looked(done.await?);
            tasks.next_ask = Instant::now() + DAEMON_EVERY;
        }
        let sessions = state.tab() == Tab::Sessions;
        if sessions && tasks.asking.is_none() && Instant::now() >= tasks.next_ask {
            let dir = dir.clone();
            tasks.asking = Some(tokio::spawn(async move {
                let Some(dir) = dir else {
                    return Err(text::NO_DAEMON_HERE.to_owned());
                };
                crate::daemon::status(&dir)
                    .await
                    .map_err(|e| format!("{e:#}"))
            }));
        }
        if tasks.reading.as_ref().is_some_and(Reading::is_finished)
            && let Some(done) = tasks.reading.take()
        {
            state.titled(done.result()?);
        }
        let due = tasks
            .read_at
            .is_none_or(|at| at.elapsed() >= crate::top::ISSUES_TTL);
        if sessions && tasks.reading.is_none() && due {
            let repo = me.default_thread().map(|t| t.to_string());
            let repos = riff_client::waves::board_repos(
                state.model(),
                &riff_client::sessions::Show::default(),
                repo.as_deref(),
            );
            let (gh, kept) = (gh.clone(), tasks.kept.clone());
            tasks.read_at = Some(Instant::now());
            tasks.reading = Some(Reading::start(move || {
                repos
                    .into_iter()
                    .map(|repo| {
                        let read = forge::titles(&gh, &repo, &kept);
                        (repo, read.map_err(|e| format!("{e:#}")))
                    })
                    .collect()
            }));
        }
        let mut drawn = None;
        terminal.draw(|frame| {
            let shown = view::compute_view(state, frame.area(), crate::run::now_ms());
            view::render(&shown, frame.buffer_mut());
            if let Some(at) = shown.cursor {
                frame.set_cursor_position(at);
            }
            drawn = Some((shown, frame.area()));
        })?;
        if let Some((shown, area)) = &drawn {
            state.drawn(shown);
            state.drew(area.height.saturating_sub(4), crate::run::now_ms());
        }
        let event = match next_event().await {
            Ok(Some(event)) => event,
            Ok(None) => continue,
            Err(e) => break Err(e),
        };
        let Some(input) = state.input(&event) else {
            continue;
        };
        input_at = Instant::now();
        let action = match input {
            Input::Key(key) => state.on_key(key),
            Input::Click { column, row } => {
                let hit = drawn.as_ref().and_then(|(shown, _)| shown.hit(column, row));
                let double = hit.is_some()
                    && last_click
                        .is_some_and(|(key, at)| Some(key) == hit && at.elapsed() < DOUBLE_CLICK);
                last_click = hit.map(|key| (key, Instant::now()));
                drawn
                    .as_ref()
                    .and_then(|(shown, _)| state.on_click(shown, column, row, double))
            }
            Input::Paste(text) => state.on_paste(&text),
            Input::Focused => {
                state.focused();
                None
            }
        };
        let Some(action) = action else {
            continue;
        };
        match action {
            Action::Quit => {
                // An action that runs ends first: the person said `y`.
                if let Some(acting) = tasks.acting.take() {
                    let _ = acting.await;
                }
                link.end().await;
                break Ok(());
            }
            Action::Look => {
                tasks.forge_again = state.tab() == Tab::Waves;
                if tasks.looking.is_some() {
                    tasks.again = true;
                } else {
                    tasks.next = Instant::now();
                }
            }
            Action::Attach(name) => {
                leave(mouse);
                let left = attach(&name).await;
                *terminal = enter(mouse)?;
                state.attached(&name, &left);
                (tasks.next, tasks.next_ask) = (Instant::now(), Instant::now());
            }
            Action::Takeover(name) => {
                leave(mouse);
                let taken = take_over(dir.as_deref(), &name).await;
                *terminal = enter(mouse)?;
                state.took_over(&name, &taken);
                (tasks.next, tasks.next_ask) = (Instant::now(), Instant::now());
            }
            Action::End(name) => {
                state.done(end(dir.as_deref(), &name).await);
                (tasks.next, tasks.next_ask) = (Instant::now(), Instant::now());
            }
            Action::Limit { .. } | Action::Host { .. } | Action::Do(_)
                if tasks.acting.is_some() =>
            {
                state.note(STILL_ACTING);
            }
            Action::Host {
                host,
                repo,
                request,
            } => {
                state.note(text::asking_host(&host, &request.to_string()));
                let (api, me, reach) = (api.clone(), me.clone(), reach.clone());
                tasks.acting = Some(tokio::spawn(async move {
                    sessions::ask_host(&api, &me, &reach, &host, &repo, &request).await
                }));
            }
            Action::Limit { host, sessions } => {
                state.note(machines::setting(&host, sessions));
                let (api, me) = (api.clone(), me.clone());
                tasks.acting = Some(tokio::spawn(async move {
                    machines::set_limit(&api, &me, &host, sessions).await
                }));
            }
            Action::Post(body) => link.send(body),
            Action::Do(effect) => {
                state.note(effect.doing());
                let (api, me, reach, gh) = (api.clone(), me.clone(), reach.clone(), gh.clone());
                tasks.acting = Some(tokio::spawn(async move {
                    match effect.run(&api, &me, &reach, &gh).await {
                        Ok(line) => line,
                        Err(e) => format!("riff: {e:#}"),
                    }
                }));
            }
        }
    };
    if let Some(follower) = follower {
        follower.abort();
    }
    tasks.abort();
    shown
}

/// The tasks of the Reports tab (01M6VT3JFEKD2FEQGNGGKHC8X5): it takes
/// the result of a read of the usage, of an audit and of a follow of a
/// Session log that ended, and the lines of the follow. It starts the
/// read that the tab asks for, each in a task of its own. The calls of
/// `gh` of the usage and of the audit run on threads of their own
/// ([`crate::pr::on_thread`]): a slow `gh` holds up no `q`
/// (01M6VT7TTV8J90C5E3QAZ454GY). It stops the follow of a log that does
/// not show.
async fn reports_tasks(
    tasks: &mut Tasks,
    state: &mut State,
    api: &Api,
    me: &SessionUri,
    reach: &Reach,
    gh: &crate::pr::Gh,
) -> Result<()> {
    use tokio::task::JoinHandle;
    fn done<T>(task: &Option<(JoinHandle<T>, Option<u64>)>) -> bool {
        task.as_ref().is_some_and(|(task, _)| task.is_finished())
    }
    if done(&tasks.usage)
        && let Some((task, asked)) = tasks.usage.take()
    {
        state.usage_read(asked, task.await?, crate::run::now_ms());
    }
    if tasks.usage.is_none()
        && let Some((repo, wave)) = state.usage_due()
    {
        state.usage_started();
        let gh = gh.clone();
        let task = tokio::spawn(async move {
            crate::pr::on_thread(move || reports::read_usage(&gh, &repo, wave))
                .await
                .and_then(|read| read)
                .map_err(|e| format!("{e:#}"))
        });
        tasks.usage = Some((task, wave));
    }
    if done(&tasks.audit)
        && let Some((task, asked)) = tasks.audit.take()
    {
        state.audited(asked, task.await?, crate::run::now_ms());
    }
    if tasks.audit.is_none()
        && let Some((repo, wave)) = state.audit_due()
    {
        state.audit_started();
        let (gh, reach) = (gh.clone(), reach.clone());
        let task = tokio::spawn(async move {
            reports::run_audit(&gh, &reach, &repo, wave)
                .await
                .map_err(|e| format!("{e:#}"))
        });
        tasks.audit = Some((task, wave));
    }
    if let Some((task, _, lines)) = &mut tasks.follow {
        // A follow that ended sent each of its lines before its end.
        let ended = task.is_finished();
        while let Ok(line) = lines.try_recv() {
            state.log_came(line);
        }
        if ended && let Some((task, ..)) = tasks.follow.take() {
            state.log_ended(match task.await {
                Ok(Ok(failed)) => Ok(failed),
                Ok(Err(e)) => Err(format!("{e:#}")),
                Err(e) => Err(e.to_string()),
            });
        }
    }
    let wanted = state.log_wanted().cloned();
    if let Some((task, source, _)) = &tasks.follow
        && wanted.as_ref() != Some(source)
    {
        task.abort();
        tasks.follow = None;
        state.log_stopped();
    }
    if tasks.follow.is_none()
        && let Some(source) = wanted
    {
        state.log_started();
        let (sender, lines) = tokio::sync::mpsc::unbounded_channel();
        let (api, me, reach, followed) = (api.clone(), me.clone(), reach.clone(), source.clone());
        let task =
            tokio::spawn(
                async move { reports::follow(&api, &me, &reach, &followed, sender).await },
            );
        tasks.follow = Some((task, source, lines));
    }
    Ok(())
}

/// The attach of `l` and Enter to the lead `name`: the code of
/// `riff lead start` (01M51PD7M04HZR2PVVJA4WCBK5).
async fn attach(name: &str) -> Result<crate::lead::Left> {
    #[cfg(target_os = "linux")]
    {
        let folder = crate::lead::folder_of_name(&crate::run::data_dir()?, name);
        crate::lead::attach(&crate::podman::Podman, name, &folder).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        anyhow::bail!(text::RUN_LINUX_ONLY)
    }
}

/// The takeover of `t` of the session `name`: the code of
/// `riff takeover NAME`, with the daemon in the folder `dir`. It stops
/// the turn at once (01M51KJ1R9K7A7RW8T8XH8VW43).
async fn take_over(dir: Option<&std::path::Path>, name: &str) -> Result<i32> {
    let dir = dir.ok_or_else(|| anyhow::anyhow!(text::NO_DAEMON_HERE))?;
    #[cfg(target_os = "linux")]
    {
        crate::takeover::takeover(dir, &crate::podman::Podman, name, false, |line| {
            println!("{line}");
        })
        .await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (dir, name);
        anyhow::bail!(text::RUN_LINUX_ONLY)
    }
}

/// The end of `x` of the session `name`: the code of `riff daemon end
/// NAME`, with the daemon in the folder `dir`. It gives the last line of
/// the daemon.
async fn end(dir: Option<&std::path::Path>, name: &str) -> Result<String> {
    let dir = dir.ok_or_else(|| anyhow::anyhow!(text::NO_DAEMON_HERE))?;
    let mut lines = Vec::new();
    crate::daemon::ask_end(dir, name, |line| lines.push(line.to_owned())).await?;
    Ok(lines.pop().unwrap_or_else(|| text::daemon_ended(name)))
}

/// The lines of the first `text` block of `doc` after the line `after`,
/// with `strip` taken from the start of each line, and no spaces at
/// their ends: the mock-up of a tab in the rustdoc or in the book, for a
/// screen test.
#[cfg(test)]
pub(crate) fn text_block(doc: &str, after: &str, strip: &str) -> Vec<String> {
    let start = doc.find(after).unwrap_or_else(|| panic!("{after}"));
    doc[start..]
        .lines()
        .map(|line| {
            line.strip_prefix(strip)
                .unwrap_or(line.trim_start_matches(strip.trim_end()))
        })
        .skip_while(|line| *line != "```text")
        .skip(1)
        .take_while(|line| *line != "```")
        .map(|line| line.trim_end().to_owned())
        .collect()
}
