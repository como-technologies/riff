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
//!   The Waves tab ([`waves`]), the Machines tab ([`machines`]) and the
//!   Chat tab ([`chat`]) are built.
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
//! A plain character is a [`Key::Char`]. The state gives it its meaning:
//! while the note line asks for a text, it types into it
//! (01M6VFGKCJ02G33TXR01GY6M0G). Else the frame takes its keys, and
//! gives each other key to the tab: the arrows, `Enter`, and a letter.
//! The tab turns it into an [`Action`], for example
//! [`machines::on_key`] and [`waves::Waves::on_key`].
//!
//! | Key | Does |
//! |---|---|
//! | `1` to `6`, also with Alt | Open a tab. |
//! | `Tab`, `Shift-Tab` | Open the next tab, or the tab before. |
//! | `r` | Look again now. |
//! | `?` | Show or hide the keys of this tab. |
//! | `Esc` | Close the popup. |
//! | `q`, `Ctrl-C` | End the TUI. The sessions go on. |
//! | `↑` `↓` `j` `k`, `PgUp` `PgDn`, `g` `G` | Select a row, in a tab with rows. |
//!
//! The prompt of the Chat tab takes each key that types: there only
//! Alt opens a tab, and `Ctrl-C` ends the TUI. [`State::input`] gives
//! the keys of the tab ([`chat::key_of`]) before the keys of the frame
//! ([`input_of`]).
//!
//! # The actions
//!
//! An action that changes something asks first in the note line
//! ([`Question`], 01M6VFD0S90SB8M8WV952NT4Y1, 01M6VFGK9XXWCA8X7CE8EZNSJV,
//! 01M6VFGYW0XVJ5JSX6R1NCR6W7):
//! `Set the limit of thelio to 4 sessions? y/N`. `y` gives the action to
//! the loop: [`Action::Limit`], or [`Action::Do`] with an [`Effect`].
//! The loop runs the code of its CLI command in a task of its own
//! ([`machines::set_limit`], [`Effect::run`]). Its result goes to the
//! note line, and the TUI looks again at once.
//!
//! # The marks
//!
//! A session of a tab gets a [`Mark`] (01M6VAZKRTVZTY73PQV5GE2WY6): a
//! small fixed set, from the state that riff-server derives and from
//! the phase of a container session. The worst mark comes first.

pub mod chat;
pub mod forge;
pub mod machines;
pub mod view;
pub mod waves;

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Result;
use ratatui::style::Color;
use riff_client::api::{self, Api, PauseScope};
use riff_client::sessions::Phase;
use riff_core::build::Build;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{HoldInfo, RiffOwner, RiffReply, RiffState, SessionState, WhoReply};

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
            Tab::Chat | Tab::Waves | Tab::Machines | Tab::Devices => &[],
            Tab::Reports => &["riff usage", "riff audit", "riff daemon events NAME"],
        }
    }

    /// True when the build item of the tab merged: the tab draws its own
    /// body.
    ///
    /// ```
    /// use riff::tui::Tab;
    ///
    /// assert!(Tab::Waves.built());
    /// assert!(Tab::Machines.built());
    /// assert!(Tab::Chat.built());
    /// assert!(!Tab::Sessions.built());
    /// ```
    pub fn built(self) -> bool {
        matches!(self, Tab::Chat | Tab::Waves | Tab::Machines)
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
    /// Open a tab: `Alt-1` to `Alt-6`, and a click on a tab. The digits
    /// with no Alt are a [`Key::Char`].
    Open(Tab),
    /// Open the next tab: `Tab`.
    Next,
    /// Open the tab before: `Shift-Tab`.
    Before,
    /// Show or hide the keys of the tab: a click on `? help`.
    Help,
    /// Close the popup, or cancel a question: `Esc`.
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
    /// A click on the row of this index in the rows of the tab.
    Row(usize),
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
/// its CLI command (01M6VFGK9XXWCA8X7CE8EZNSJV).
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
    /// ```
    pub fn question(self) -> Question {
        let text = match &self {
            Effect::Hold { repo, item, reason } => format!(
                "Hold #{item} of {}: {}?",
                text::safe(repo),
                text::safe(reason)
            ),
            Effect::Free { repo, item } => format!("Free #{item} of {}?", text::safe(repo)),
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
        Question {
            text,
            yes: Action::Do(self),
            no: NOTHING_CHANGED.to_owned(),
        }
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
    /// ```
    pub fn doing(&self) -> String {
        let (verb, what) = match self {
            Effect::Hold { repo, item, .. } => {
                ("holds", format!("#{item} of {}", text::safe(repo)))
            }
            Effect::Free { repo, item } => ("frees", format!("#{item} of {}", text::safe(repo))),
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
    /// code of its CLI command, and gives the line of that command.
    pub async fn run(&self, api: &Api, me: &SessionUri, reach: &Reach) -> Result<String> {
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

/// Looks at riff-server as `me`: the calls `riff` and `who`
/// (01M6VAZK7N636Q0JFSV553A9ES), and `plan show` for each repository of
/// `boards` (01M6VFGK79JGRF919BM8SJ6E08). Each call has the budget of a
/// short command, as in `riff top`. Then it asks the riff daemon of this
/// host for its status ([`machines::here`], 01M6VFD09QN71SWMPP8YXCRAEK).
pub async fn look(api: &Api, me: &SessionUri, boards: &[String]) -> Result<Look> {
    let pauses = api.pauses(me).await?;
    let start = std::time::Instant::now();
    let who = api.roster(me, false).await?;
    let call = start.elapsed();
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
    /// The Waves tab.
    waves: waves::Waves,
    /// The reason of a hold that the person types in the note line,
    /// until `Enter` or `Esc`.
    reason: Option<waves::Reason>,
    /// The rows of a page of the body at the last draw.
    page: usize,
    /// The time of the last draw, in milliseconds since the epoch: the
    /// age in a note of a key.
    drawn_ms: i64,
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
            waves: waves::Waves::default(),
            reason: None,
            page: 20,
            drawn_ms: 0,
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

    /// Asks for the reason of a hold in the note line: the person types
    /// it ([`waves::typed`], 01M6VFGK9XXWCA8X7CE8EZNSJV).
    pub fn ask_reason(&mut self, reason: waves::Reason) {
        self.reason = Some(reason);
    }

    /// The text of the note line while the TUI asks: the question with
    /// `y/N`, or what the person types with a cursor. It comes before
    /// the fault and the note (01M6VFGYW0XVJ5JSX6R1NCR6W7).
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
    /// // q and Ctrl-C end the TUI.
    /// state.ask(Question { text: "Set the limit?".into(), yes: yes.clone(), no: no.clone() });
    /// assert_eq!(state.on_key(Key::Char('q')), Some(Action::Quit));
    /// state.ask(Question { text: "Set the limit?".into(), yes, no });
    /// assert_eq!(state.on_key(Key::Quit), Some(Action::Quit));
    /// ```
    pub fn asked(&self) -> Option<String> {
        if let Some(question) = &self.question {
            return Some(format!("{} y/N", question.text));
        }
        if let Some(reason) = &self.reason {
            return Some(reason.line());
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
    /// 01M6VAZKDEQ6951KGPGAMF1SNJ). A question of the note line takes the
    /// key first, then a text that the person types ([`machines::typed`],
    /// [`waves::typed`]). Then the frame takes its keys, and the tab the
    /// others: `k` `j` `g` `G` are `↑` `↓` `Home` `End`
    /// (01M6VFGKCJ02G33TXR01GY6M0G). Opening the Waves tab looks again at
    /// once, for the holds of its boards.
    ///
    /// ```
    /// use riff::tui::{Action, Key, State, Tab};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this());
    /// assert_eq!(state.on_key(Key::Open(Tab::Machines)), None);
    /// assert_eq!(state.tab(), Tab::Machines);
    /// assert_eq!(state.on_key(Key::Char('3')), Some(Action::Look));
    /// assert_eq!(state.tab(), Tab::Waves);
    /// state.on_key(Key::Next);
    /// assert_eq!(state.tab(), Tab::Machines);
    /// state.on_key(Key::Before);
    /// state.on_key(Key::Before);
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
    /// ```
    pub fn on_key(&mut self, key: Key) -> Option<Action> {
        // A question takes the next key (01M6VFD0S90SB8M8WV952NT4Y1).
        if let Some(question) = self.question.take() {
            return match key {
                Key::Char('y' | 'Y') => Some(question.yes),
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
                Tab::Waves => self.waves_key(key),
                Tab::Machines => machines::on_key(self, key),
                _ => None,
            },
        }
    }

    /// Opens the tab `tab` and closes the help. The Waves tab looks again
    /// at once.
    fn go(&mut self, tab: Tab) -> Option<Action> {
        self.help = false;
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
    /// use riff::tui::{Action, Input, Key, State, Tab, chat};
    /// use riff_core::build::Build;
    ///
    /// let q = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    /// let mut state = State::new(Build::this());
    /// assert_eq!(state.input(&q), Some(Input::Key(Key::Char('q'))));
    /// // In the prompt of the Chat tab, `q` types.
    /// state.on_key(Key::Open(Tab::Chat));
    /// assert_eq!(state.input(&q), Some(Input::Key(Key::Chat(chat::Key::Type('q')))));
    /// // In the log, `q` goes to the frame, and ends the TUI.
    /// state.on_key(Key::Chat(chat::Key::Log));
    /// assert_eq!(state.input(&q), Some(Input::Key(Key::Char('q'))));
    /// assert_eq!(state.on_key(Key::Char('q')), Some(Action::Quit));
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
    use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
    use ratatui::crossterm::execute;
    // The first look runs before the TUI takes the terminal: its error
    // prints as in the CLI (01M6VAZKAH6B53YKR8W4YKNSH6).
    let first = look(api, me, &[]).await?;
    let here = here.default_thread().map(|t| t.to_string());
    let mut state = State::new(Build::this()).in_clone(here);
    let start = crate::run::now_ms();
    state.looked(first, start);
    state.chat_as(person.me.who().user(), u64::try_from(start).unwrap_or(0));
    let mut terminal = ratatui::try_init()?;
    if mouse && let Err(e) = execute!(std::io::stdout(), EnableMouseCapture) {
        ratatui::restore();
        return Err(e.into());
    }
    let shown = frames(&mut terminal, &mut state, api, me, person, reach).await;
    if mouse {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    shown
}

/// A read of the forge for the Waves tab that runs: it gives back the
/// kept replies, and the read of each repository.
type Reading = tokio::task::JoinHandle<(forge::Kept, Vec<(String, Result<forge::Forge, String>)>)>;

/// The loop of [`run`]: it draws the state, takes the input of the
/// person, looks at riff-server each [`every`], reads the forge of the
/// Waves tab each [`forge::READ_EVERY`] while it shows, and runs each
/// action in a task of its own. The [`chat::Link`] of `person` follows
/// the thread `chat` and posts its lines.
async fn frames(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut State,
    api: &Api,
    me: &SessionUri,
    person: &chat::Person,
    reach: &Reach,
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
    // The reads of the forge share one token, and count as `tui`
    // (01M6VFGK4P2FWMFQHBZKR3JXSZ).
    let gh = crate::pr::Gh::here("tui")
        .with_one_token()
        .with_shared_reads();
    let mut kept = Some(forge::Kept::default());
    let mut reading: Option<Reading> = None;
    // `r` while the Waves tab shows: read the forge again.
    let mut read_again = false;
    loop {
        for came in link.came() {
            state.chat_came(came);
        }
        if reading
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
            && let Some(done) = reading.take()
        {
            let (back, read) = done.await?;
            kept = Some(back);
            state.read_forge(read, crate::run::now_ms());
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
        if reading.is_none()
            && state.tab() == Tab::Waves
            && (read_again || state.waves().due(crate::run::now_ms()))
        {
            read_again = false;
            let repos = state.boards();
            let gh = gh.clone();
            let mut back = kept.take().unwrap_or_default();
            reading = Some(tokio::task::spawn_blocking(move || {
                let read = forge::read_each(&gh, &repos, &mut back);
                (back, read)
            }));
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
            let (api, me, boards) = (api.clone(), me.clone(), state.boards());
            looking = Some(tokio::spawn(async move { look(&api, &me, &boards).await }));
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
                .and_then(|(shown, _)| state.on_click(shown, column, row)),
        };
        match action {
            Some(Action::Quit) => {
                if let Some(looking) = looking {
                    looking.abort();
                }
                // An action that runs ends first: the person said `y`.
                if let Some(acting) = acting {
                    let _ = acting.await;
                }
                link.end().await;
                return Ok(());
            }
            Some(Action::Look) => {
                read_again = state.tab() == Tab::Waves;
                if looking.is_some() {
                    again = true;
                } else {
                    next = Instant::now();
                }
            }
            Some(Action::Limit { .. } | Action::Do(_)) if acting.is_some() => {
                state.note(STILL_ACTING);
            }
            Some(Action::Limit { host, sessions }) => {
                state.note(machines::setting(&host, sessions));
                let (api, me) = (api.clone(), me.clone());
                acting = Some(tokio::spawn(async move {
                    machines::set_limit(&api, &me, &host, sessions).await
                }));
            }
            Some(Action::Post(body)) => link.send(body),
            Some(Action::Do(effect)) => {
                state.note(effect.doing());
                let (api, me, reach) = (api.clone(), me.clone(), reach.clone());
                acting = Some(tokio::spawn(async move {
                    match effect.run(&api, &me, &reach).await {
                        Ok(line) => line,
                        Err(e) => format!("riff: {e:#}"),
                    }
                }));
            }
            None => {}
        }
    }
}
