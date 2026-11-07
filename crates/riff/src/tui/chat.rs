//! The Chat tab of the TUI (#1640): the thread `chat`, as `riff chat`
//! shows it, with the log, the prompt and the wakes
//! (01M3NB5MY93KV9RKZGGSMZW00D).
//!
//! # Design
//!
//! ```text
//!  1 Sessions  [2 Chat]  3 Waves•  4 Machines  5 Devices  6 Reports
//!  riff running · owner mike · build 10df8a4 · link ok 2s
//!  chat · online: mike, brett · away: ann · a line wakes your leads
//! ──── Friday 2026-11-06 ─────────────────────────────────────────────
//!  09:14 [mike's lead, riff] good morning: 2 pull requests merged
//!  09:15 <ann@kadomony> morning
//!  12:20 * mike@thelio goes for lunch
//!
//! > @lead:riff what is next?
//!  Enter wakes your lead in como-technologies/riff.
//!  Enter send  Esc log  PgUp older  Alt-1..6 tab  Ctrl-C quit
//! ```
//!
//! - **The summary row** names the people who are online and away, from
//!   the last look of `who` (01M6VG1P7BH32E5YAP0GJSNJJP). While the stream of the thread is down, it
//!   says `reconnecting…`, or the fault in red.
//! - **The log** ([`Chat`]) keeps the last [`KEEP`] lines of the thread.
//!   A line shows as in `riff chat` ([`crate::chat::said`]): a person as
//!   `<USER@HOST>`, a lead as `[USER's lead, REPO]`, an action as
//!   `* USER@HOST TEXT`. A rule names each new day. A long line wraps,
//!   and each more row starts under the sender. The newest lines show:
//!   `PgUp` shows older lines ([`Scroll`], 01M6VG1P7BH32E5YAP0GJSNJJP).
//! - **The prompt** ([`Prompt`]) has the focus when you open the tab
//!   ([`Focus`]). There each key types, also the digits, `q` and `?`.
//!   `Alt-1` to `Alt-6`, `Tab` and `Ctrl-C` keep their work. `Esc` moves
//!   the focus to the log, where the keys of the frame work, and `i` or
//!   `Enter` gives it back. `↑` and `↓` bring back the lines that you
//!   sent (01M6VG1PA4PM47777ZT4PNGETF).
//! - **The note line** says whom the line will wake before you send it
//!   ([`wake_note`]), with the rules of `riff chat`
//!   (01M6VG1PCXHNVJP7TSP0J5G5K9). A post that fails shows there at
//!   once, before that note, until the next key
//!   (01M6VMACJGF4A0ZXC6YK13KNCH).
//! - **News:** each new line of the thread while another tab shows
//!   adds one to `Chat•N` in the strip (01M6VG1PFPMHRQ4F9XHJTYMSJY).
//!
//! The tab sends and follows as `riff chat` does, and as the person, not
//! as a session ([`Person`]). `Enter` reads the line with
//! [`crate::chat::typed`], checks its lead with
//! [`riff_client::chat::resolve`] and the sessions of the last look, and
//! gives [`Action::Post`] to the loop. [`Link`] posts each line in its
//! order with [`crate::chat::posts`], the code of `riff chat`. It
//! follows the thread from the start of the TUI, with
//! [`crate::catch_up`]: a break of the stream loses no line.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as the person
//!     participant T as the Chat tab
//!     participant L as Link
//!     participant S as riff-server
//!     L->>S: join chat, tail chat, read the history
//!     S-->>L: the lines
//!     L-->>T: Came::Seen, at each frame
//!     P->>T: types "@lead:riff hi"
//!     T-->>P: the note: Enter wakes your lead in como-technologies/riff
//!     P->>T: Enter
//!     T->>L: Action::Post
//!     L->>S: who, then post to user=mike,repo=como-technologies/riff,lead=true
//!     S-->>L: tail: the line
//!     L-->>T: the line shows once
//! ```

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, FixedOffset, Local, NaiveDate, TimeZone};
use futures::StreamExt;
use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line as Row, Span};
use riff_client::api::Api;
use riff_client::chat::{resolve, wakes};
use riff_client::link::STREAM_RETRY;
use riff_client::threads::{ACTION, Line};
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use tokio::sync::mpsc;

use super::view::{Hit, Layout, Placed, View};
use super::{Action, Key as FrameKey, News, State, Tab};
use crate::catch_up::{self, Seen, Shown, Start};
use crate::chat::{INDENT, Typed, typed};
use crate::text::safe;

/// The most lines that the log keeps. An older line goes.
pub const KEEP: usize = 1000;

/// The rows that a turn of the mouse wheel scrolls.
const WHEEL: u16 = 3;

/// The style of the time, the rules and a dim prompt.
const DIM: Style = Style::new().add_modifier(Modifier::DIM);

/// The style of a warning: a line that the chat will not send or could
/// not send, the lost lines.
const WARNING: Style = Style::new().fg(Color::Yellow);

/// The style of a fault of the stream.
const ERROR: Style = Style::new().fg(Color::Red);

/// Where the keys of the Chat tab go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Focus {
    /// The prompt: each key types.
    #[default]
    Prompt,
    /// The log: the keys of the frame work.
    Log,
}

/// A key of the Chat tab ([`key_of`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Type a character in the prompt.
    Type(char),
    /// Delete the character before the cursor.
    Backspace,
    /// Delete the character at the cursor.
    Delete,
    /// Move the cursor one character to the left.
    Left,
    /// Move the cursor one character to the right.
    Right,
    /// Move the cursor to the start of the line.
    Home,
    /// Move the cursor to the end of the line.
    End,
    /// Delete the text before the cursor: `Ctrl-U`.
    Clear,
    /// Show the line that you sent before the one that shows: `↑`.
    Earlier,
    /// Show the line that you sent after the one that shows: `↓`.
    Later,
    /// Send the line: `Enter` in the prompt.
    Send,
    /// Move the focus to the log: `Esc`.
    Log,
    /// Move the focus to the prompt: `i` or `Enter` in the log.
    Prompt,
    /// Scroll the log.
    Scroll(Scroll),
}

/// A scroll of the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    /// Older rows: `↑`, `k` and the wheel.
    Up(u16),
    /// Newer rows: `↓`, `j` and the wheel.
    Down(u16),
    /// A page of older rows: `PgUp`.
    PageUp,
    /// A page of newer rows: `PgDn`.
    PageDown,
    /// The oldest line: `g` or `Home` in the log.
    Oldest,
    /// The newest line: `G` or `End` in the log.
    Newest,
}

/// The key of the Chat tab in `event`, when the focus is `focus`
/// (01M6VG1PA4PM47777ZT4PNGETF). `None`
/// for a key that the frame takes ([`super::input_of`]): `Alt-1` to
/// `Alt-6`, `Tab`, `Shift-Tab` and `Ctrl-C` in the prompt, and each key
/// of the frame in the log. The wheel of the mouse scrolls the log.
///
/// ```
/// use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
/// use riff::tui::chat::{Focus, Key, Scroll, key_of};
///
/// let press = |code, modifiers| Event::Key(KeyEvent::new(code, modifiers));
/// let none = KeyModifiers::NONE;
/// // In the prompt, each key types: also a digit, `q` and `?`.
/// for c in ['q', '2', '?', 'r'] {
///     assert_eq!(key_of(&press(KeyCode::Char(c), none), Focus::Prompt), Some(Key::Type(c)));
/// }
/// assert_eq!(key_of(&press(KeyCode::Enter, none), Focus::Prompt), Some(Key::Send));
/// assert_eq!(key_of(&press(KeyCode::Esc, none), Focus::Prompt), Some(Key::Log));
/// assert_eq!(key_of(&press(KeyCode::Up, none), Focus::Prompt), Some(Key::Earlier));
/// assert_eq!(
///     key_of(&press(KeyCode::PageUp, none), Focus::Prompt),
///     Some(Key::Scroll(Scroll::PageUp))
/// );
/// // Alt-2, Tab and Ctrl-C go to the frame.
/// assert_eq!(key_of(&press(KeyCode::Char('2'), KeyModifiers::ALT), Focus::Prompt), None);
/// assert_eq!(key_of(&press(KeyCode::Tab, none), Focus::Prompt), None);
/// assert_eq!(key_of(&press(KeyCode::Char('c'), KeyModifiers::CONTROL), Focus::Prompt), None);
///
/// // In the log, `i` and Enter go back to the prompt, and the keys of
/// // the frame go to the frame.
/// assert_eq!(key_of(&press(KeyCode::Char('i'), none), Focus::Log), Some(Key::Prompt));
/// assert_eq!(key_of(&press(KeyCode::Enter, none), Focus::Log), Some(Key::Prompt));
/// assert_eq!(key_of(&press(KeyCode::Char('k'), none), Focus::Log), Some(Key::Scroll(Scroll::Up(1))));
/// assert_eq!(key_of(&press(KeyCode::Char('G'), none), Focus::Log), Some(Key::Scroll(Scroll::Newest)));
/// assert_eq!(key_of(&press(KeyCode::Char('q'), none), Focus::Log), None);
/// assert_eq!(key_of(&press(KeyCode::Char('2'), none), Focus::Log), None);
/// ```
pub fn key_of(event: &Event, focus: Focus) -> Option<Key> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            let alt = key.modifiers.contains(KeyModifiers::ALT);
            // AltGr comes as Ctrl and Alt on some terminals.
            let plain = ctrl == alt;
            let key = match (focus, key.code) {
                (Focus::Prompt, KeyCode::Char('a')) if ctrl && !alt => Key::Home,
                (Focus::Prompt, KeyCode::Char('e')) if ctrl && !alt => Key::End,
                (Focus::Prompt, KeyCode::Char('u')) if ctrl && !alt => Key::Clear,
                (Focus::Prompt, KeyCode::Char(c)) if plain && !c.is_control() => Key::Type(c),
                (Focus::Prompt, KeyCode::Backspace) => Key::Backspace,
                (Focus::Prompt, KeyCode::Delete) => Key::Delete,
                (Focus::Prompt, KeyCode::Left) => Key::Left,
                (Focus::Prompt, KeyCode::Right) => Key::Right,
                (Focus::Prompt, KeyCode::Home) => Key::Home,
                (Focus::Prompt, KeyCode::End) => Key::End,
                (Focus::Prompt, KeyCode::Up) => Key::Earlier,
                (Focus::Prompt, KeyCode::Down) => Key::Later,
                (Focus::Prompt, KeyCode::Enter) => Key::Send,
                (Focus::Prompt, KeyCode::Esc) => Key::Log,
                (Focus::Log, KeyCode::Enter) => Key::Prompt,
                (Focus::Log, KeyCode::Char('i')) if !ctrl && !alt => Key::Prompt,
                (Focus::Log, KeyCode::Up) => Key::Scroll(Scroll::Up(1)),
                (Focus::Log, KeyCode::Char('k')) if !ctrl && !alt => Key::Scroll(Scroll::Up(1)),
                (Focus::Log, KeyCode::Down) => Key::Scroll(Scroll::Down(1)),
                (Focus::Log, KeyCode::Char('j')) if !ctrl && !alt => Key::Scroll(Scroll::Down(1)),
                (Focus::Log, KeyCode::Home) => Key::Scroll(Scroll::Oldest),
                (Focus::Log, KeyCode::Char('g')) if !ctrl && !alt => Key::Scroll(Scroll::Oldest),
                (Focus::Log, KeyCode::End) => Key::Scroll(Scroll::Newest),
                (Focus::Log, KeyCode::Char('G')) if !ctrl && !alt => Key::Scroll(Scroll::Newest),
                (_, KeyCode::PageUp) => Key::Scroll(Scroll::PageUp),
                (_, KeyCode::PageDown) => Key::Scroll(Scroll::PageDown),
                _ => return None,
            };
            Some(key)
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => Some(Key::Scroll(Scroll::Up(WHEEL))),
            MouseEventKind::ScrollDown => Some(Key::Scroll(Scroll::Down(WHEEL))),
            _ => None,
        },
        _ => None,
    }
}

/// The line that the person types, and the lines that the person sent.
///
/// ```
/// use riff::tui::chat::{Key, Prompt};
///
/// let mut prompt = Prompt::default();
/// for c in "hi al".chars() {
///     prompt.edit(Key::Type(c));
/// }
/// prompt.edit(Key::Type('l'));
/// prompt.edit(Key::Home);
/// prompt.edit(Key::Delete);
/// prompt.edit(Key::Type('H'));
/// assert_eq!((prompt.text(), prompt.cursor()), ("Hi all", 1));
/// prompt.edit(Key::End);
/// prompt.edit(Key::Backspace);
/// assert_eq!(prompt.text(), "Hi al");
/// assert_eq!(prompt.take(), "Hi al");
/// assert_eq!(prompt.text(), "");
/// // ↑ brings back the line that you sent, and ↓ the text before it.
/// prompt.edit(Key::Type('x'));
/// prompt.edit(Key::Earlier);
/// assert_eq!(prompt.text(), "Hi al");
/// prompt.edit(Key::Later);
/// assert_eq!(prompt.text(), "x");
/// // Ctrl-U deletes the text before the cursor.
/// prompt.edit(Key::Clear);
/// assert_eq!(prompt.text(), "");
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prompt {
    /// The text.
    text: String,
    /// The place of the cursor, in characters.
    cursor: usize,
    /// The lines that the person sent, the oldest first.
    sent: Vec<String>,
    /// The line of `sent` that shows after `↑`.
    back: Option<usize>,
    /// The text before the first `↑`.
    draft: String,
}

impl Prompt {
    /// The text that the person types.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The place of the cursor in the text, in characters.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The byte of the character `at` of the text.
    fn byte(&self, at: usize) -> usize {
        self.text
            .char_indices()
            .nth(at)
            .map_or(self.text.len(), |(i, _)| i)
    }

    /// Takes a key of the prompt. A key that scrolls or moves the focus
    /// does nothing here.
    pub fn edit(&mut self, key: Key) {
        let length = self.text.chars().count();
        match key {
            Key::Type(c) => {
                let at = self.byte(self.cursor);
                self.text.insert(at, c);
                self.cursor += 1;
            }
            Key::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let at = self.byte(self.cursor);
                self.text.remove(at);
            }
            Key::Delete if self.cursor < length => {
                let at = self.byte(self.cursor);
                self.text.remove(at);
            }
            Key::Left => self.cursor = self.cursor.saturating_sub(1),
            Key::Right => self.cursor = (self.cursor + 1).min(length),
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = length,
            Key::Clear => {
                let at = self.byte(self.cursor);
                self.text.replace_range(..at, "");
                self.cursor = 0;
            }
            Key::Earlier if !self.sent.is_empty() => {
                let back = match self.back {
                    None => {
                        self.draft = std::mem::take(&mut self.text);
                        self.sent.len() - 1
                    }
                    Some(i) => i.saturating_sub(1),
                };
                self.back = Some(back);
                self.show(self.sent[back].clone());
            }
            Key::Later => match self.back {
                Some(i) if i + 1 < self.sent.len() => {
                    self.back = Some(i + 1);
                    self.show(self.sent[i + 1].clone());
                }
                Some(_) => {
                    self.back = None;
                    let draft = std::mem::take(&mut self.draft);
                    self.show(draft);
                }
                None => {}
            },
            _ => {}
        }
    }

    /// Shows `text` with the cursor at its end.
    fn show(&mut self, text: String) {
        self.cursor = text.chars().count();
        self.text = text;
    }

    /// The text, for a send: the prompt is empty after it, and `↑` brings
    /// the text back.
    pub fn take(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        if self.sent.last() != Some(&text) {
            self.sent.push(text.clone());
        }
        self.cursor = 0;
        self.back = None;
        self.draft.clear();
        text
    }
}

/// An entry of the log.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// A line of the thread, at its local time.
    Line {
        line: Box<Line>,
        at: DateTime<FixedOffset>,
    },
    /// The number of lines of a break that the server no longer keeps.
    Lost(u64),
}

impl Entry {
    /// The day of a line.
    fn day(&self) -> Option<NaiveDate> {
        match self {
            Entry::Line { at, .. } => Some(at.date_naive()),
            Entry::Lost(_) => None,
        }
    }
}

/// The local time of `at_ms`, in milliseconds since the epoch, as
/// `riff chat` shows it.
fn local(at_ms: u64) -> DateTime<FixedOffset> {
    i64::try_from(at_ms)
        .ok()
        .and_then(|ms| Local.timestamp_millis_opt(ms).single())
        .unwrap_or_else(Local::now)
        .fixed_offset()
}

/// What [`Link`] gives the loop of the TUI.
#[derive(Debug, Clone, PartialEq)]
pub enum Came {
    /// An entry of the log, one time.
    Seen(Entry),
    /// The stream is connected.
    Up,
    /// A connect of the stream failed: it connects again.
    Reconnecting,
    /// A fault of the stream that the person reads, for example a
    /// riff-server that riff cannot talk to.
    Fault(String),
    /// A line that riff could not send, with the error.
    Unsent(String),
}

/// Why the stream of the thread is down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Down {
    /// A connect failed: it connects again.
    Reconnecting,
    /// A fault for the person.
    Fault(String),
}

/// What the person saw of the log at the last draw: the columns of a
/// row and the rows of the log. A scroll needs it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Window {
    /// The columns of a row of text.
    pub columns: u16,
    /// The rows of the log.
    pub rows: u16,
}

/// The state of the Chat tab: the log, the prompt, the focus and the
/// scroll.
#[derive(Debug, Clone, Default)]
pub struct Chat {
    /// The user of the person, who sends the lines.
    user: Option<String>,
    /// The start of the TUI: an older line is no news.
    since_ms: u64,
    /// The last [`KEEP`] entries, the oldest first.
    entries: VecDeque<Entry>,
    prompt: Prompt,
    focus: Focus,
    /// The rows of the log under the window: 0 shows the newest line.
    scroll: usize,
    /// The window of the last draw.
    pub(super) window: Window,
    /// Why the stream of the thread is down, while it is.
    down: Option<Down>,
    /// The note of a post that failed, until the next key
    /// (01M6VMACJGF4A0ZXC6YK13KNCH).
    pub(super) unsent: Option<String>,
}

impl Chat {
    /// The chat of `user`, from the time `since_ms` (milliseconds since
    /// the epoch): a line from before is no news.
    pub fn new(user: &str, since_ms: u64) -> Chat {
        Chat {
            user: Some(user.to_owned()),
            since_ms,
            ..Chat::default()
        }
    }

    /// The entries of the log, the oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// The prompt.
    pub fn prompt(&self) -> &Prompt {
        &self.prompt
    }

    /// Where the keys go.
    pub fn focus(&self) -> Focus {
        self.focus
    }

    /// The rows of the log under the window.
    pub fn scrolled(&self) -> usize {
        self.scroll
    }

    /// Why the stream is down, while it is.
    pub fn down(&self) -> Option<&Down> {
        self.down.as_ref()
    }

    /// The note of a post that failed, until the next key: it comes
    /// before the note of the prompt (01M6VMACJGF4A0ZXC6YK13KNCH).
    pub fn unsent(&self) -> Option<&str> {
        self.unsent.as_deref()
    }

    /// Adds `entry` to the end of the log. While the person looks at
    /// older lines, the window stays on them.
    fn add(&mut self, entry: Entry) {
        self.entries.push_back(entry);
        if self.scroll > 0 {
            let last = self.entries.len() - 1;
            let columns = self.window.columns;
            self.scroll += rows_of(&self.entries, last, columns, columns).len();
        }
        if self.entries.len() > KEEP {
            self.entries.pop_front();
        }
    }

    /// The rows of the whole log in the window of the last draw.
    fn total(&self) -> usize {
        let columns = self.window.columns;
        (0..self.entries.len())
            .map(|i| rows_of(&self.entries, i, columns, columns).len())
            .sum()
    }

    /// Scrolls the log by `scroll`, in the window of the last draw.
    fn scroll_by(&mut self, scroll: Scroll) {
        let rows = usize::from(self.window.rows);
        let page = rows.saturating_sub(1).max(1);
        let most = self.total().saturating_sub(rows);
        self.scroll = match scroll {
            Scroll::Up(n) => self.scroll.saturating_add(usize::from(n)),
            Scroll::Down(n) => self.scroll.saturating_sub(usize::from(n)),
            Scroll::PageUp => self.scroll.saturating_add(page),
            Scroll::PageDown => self.scroll.saturating_sub(page),
            Scroll::Oldest => most,
            Scroll::Newest => 0,
        }
        .min(most);
    }
}

/// Who sends the lines of the Chat tab: the person, at this host, with
/// its sign-in, as `riff chat` does, also when the TUI runs in a
/// session (01M6VG1PCXHNVJP7TSP0J5G5K9).
#[derive(Clone)]
pub struct Person {
    /// The client of riff-server, with the token of the person.
    pub api: Api,
    /// The person: `riff://USER@HOST`.
    pub me: SessionUri,
}

/// The tasks of the Chat tab: one follows the thread `chat` from the
/// start of the TUI (01M6VG1PFPMHRQ4F9XHJTYMSJY), and one posts the
/// lines that the person sends, in order.
pub struct Link {
    came: mpsc::UnboundedReceiver<Came>,
    send: mpsc::UnboundedSender<String>,
    posts: tokio::task::JoinHandle<()>,
    follow: tokio::task::JoinHandle<()>,
}

impl Link {
    /// Starts the tasks of `person`.
    pub fn start(person: &Person) -> Link {
        let (tx, came) = mpsc::unbounded_channel();
        let thread = crate::chat::thread();
        let unsent = tx.clone();
        let (send, posts) = crate::chat::posts(&person.api, &person.me, &thread, move |text| {
            let _ = unsent.send(Came::Unsent(text.to_owned()));
        });
        let (api, me) = (person.api.clone(), person.me.clone());
        let follow = tokio::spawn(async move { follow(&api, &me, &thread, &tx).await });
        Link {
            came,
            send,
            posts,
            follow,
        }
    }

    /// What came since the last call.
    pub fn came(&mut self) -> Vec<Came> {
        let mut came = Vec::new();
        while let Ok(one) = self.came.try_recv() {
            came.push(one);
        }
        came
    }

    /// Posts `body` after the lines before it.
    pub fn send(&self, body: String) {
        let _ = self.send.send(body);
    }

    /// Stops the follow, and waits for the posts of the lines that the
    /// person sent, as `riff chat` does at its end.
    pub async fn end(self) {
        self.follow.abort();
        drop(self.send);
        let _ = self.posts.await;
    }
}

/// Joins `thread` as `me`, and follows it until the receiver of `tx`
/// goes: the history first, then each new line, one time
/// ([`crate::catch_up`]). A join that fails tries again after
/// [`STREAM_RETRY`].
async fn follow(api: &Api, me: &SessionUri, thread: &ThreadName, tx: &mpsc::UnboundedSender<Came>) {
    while let Err(e) = api.join(me, thread).await {
        let fault = safe(&format!("riff: cannot join the chat: {e:#}"));
        if tx.send(Came::Fault(fault)).is_err() {
            return;
        }
        tokio::time::sleep(STREAM_RETRY).await;
    }
    let shown = Arc::new(Mutex::new(Shown::new(Start::History)));
    let connect = || {
        let shown = Arc::clone(&shown);
        async move {
            let stream = catch_up::connect(api, me, thread, &shown).await?;
            let _ = tx.send(Came::Up);
            anyhow::Ok(stream)
        }
    };
    let mut stream = std::pin::pin!(riff_client::api::follow(connect, STREAM_RETRY));
    while let Some(item) = stream.next().await {
        let came = match item {
            Ok(Seen::Message(c)) => Came::Seen(Entry::Line {
                at: local(c.message.at_ms),
                line: Box::new(Line::of(&c, thread)),
            }),
            Ok(Seen::Lost(n)) => Came::Seen(Entry::Lost(n)),
            Err(e) if e.downcast_ref::<riff_core::build::Mismatch>().is_some() => {
                Came::Fault(safe(&format!("riff: {e:#}")))
            }
            Err(_) => Came::Reconnecting,
        };
        if tx.send(came).is_err() {
            return;
        }
    }
}

impl State {
    /// The state of the Chat tab.
    pub fn chat(&self) -> &Chat {
        &self.chat
    }

    /// The Chat tab sends as `user`, and a line from before `since_ms`
    /// is no news.
    pub fn chat_as(&mut self, user: &str, since_ms: u64) {
        self.chat = Chat::new(user, since_ms);
    }

    /// Takes what [`Link`] gave. A new line while another tab shows adds
    /// one to the news of the Chat tab: `Chat•N`
    /// (01M6VAZK1R932VQMJBC32MTTJB). A line that the post of a line
    /// could not send goes in the note line at once: until the next key,
    /// it comes before the note of the prompt
    /// (01M6VMACJGF4A0ZXC6YK13KNCH).
    ///
    /// ```
    /// use riff::tui::chat::{Came, Entry, Key};
    /// use riff::tui::{Key as Frame, News, State, Tab};
    /// use riff_client::threads::Line;
    /// use riff_core::build::Build;
    /// use riff_core::wire::Kind;
    ///
    /// let came = |at_ms: u64| {
    ///     let line = Line {
    ///         seq: at_ms,
    ///         at_ms,
    ///         from: "riff://ann@heron".parse().unwrap(),
    ///         verified: true,
    ///         lead: false,
    ///         to: vec![],
    ///         kind: Kind::Message,
    ///         body: "hi".into(),
    ///     };
    ///     let at = chrono::DateTime::from_timestamp_millis(at_ms as i64).unwrap().fixed_offset();
    ///     Came::Seen(Entry::Line { line: Box::new(line), at })
    /// };
    /// let mut state = State::new(Build::this());
    /// state.chat_as("mike", 5_000);
    /// // The history is no news, and a new line is.
    /// state.chat_came(came(1_000));
    /// assert_eq!(state.news_of(Tab::Chat), None);
    /// state.chat_came(came(6_000));
    /// state.chat_came(came(7_000));
    /// assert_eq!(state.news_of(Tab::Chat), Some(News::Lines(2)));
    /// assert_eq!(state.chat().entries().count(), 3);
    /// // A line that riff could not send goes in the note line. Until
    /// // the next key, it comes before the note of the prompt.
    /// state.chat_came(Came::Unsent("riff: cannot send the line: no".into()));
    /// assert_eq!(state.shown_note(), Some("riff: cannot send the line: no"));
    /// assert_eq!(state.chat().unsent(), Some("riff: cannot send the line: no"));
    /// state.on_key(Frame::Chat(Key::Type('a')));
    /// assert_eq!(state.chat().unsent(), None);
    /// ```
    pub fn chat_came(&mut self, came: Came) {
        match came {
            Came::Seen(entry) => {
                let new =
                    matches!(&entry, Entry::Line { line, .. } if line.at_ms >= self.chat.since_ms);
                self.chat.add(entry);
                if new && self.tab != Tab::Chat {
                    let n = match self.news_of(Tab::Chat) {
                        Some(News::Lines(n)) => n.saturating_add(1),
                        _ => 1,
                    };
                    self.news(Tab::Chat, News::Lines(n));
                }
            }
            Came::Up => self.chat.down = None,
            Came::Reconnecting => self.chat.down = Some(Down::Reconnecting),
            Came::Fault(fault) => self.chat.down = Some(Down::Fault(fault)),
            Came::Unsent(text) => {
                self.chat.unsent = Some(text.clone());
                self.note(text);
            }
        }
    }

    /// The sessions of the last look: `resolve` picks a lead from them.
    fn leads(&self) -> Vec<SessionUri> {
        self.last_look()
            .map(|(look, _)| look.who.sessions.iter().map(|s| s.uri.clone()).collect())
            .unwrap_or_default()
    }

    /// Takes the key `key` of the Chat tab.
    ///
    /// ```
    /// use riff::tui::chat::{Focus, Key};
    /// use riff::tui::{Action, Key as Frame, Look, State, Tab};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this());
    /// state.looked(Look::default(), 0);
    /// state.chat_as("mike", 0);
    /// state.on_key(Frame::Open(Tab::Chat));
    /// assert_eq!(state.chat().focus(), Focus::Prompt);
    /// for c in "hi all".chars() {
    ///     state.on_key(Frame::Chat(Key::Type(c)));
    /// }
    /// // Enter gives the line to the loop, which posts it.
    /// assert_eq!(state.on_key(Frame::Chat(Key::Send)), Some(Action::Post("hi all".into())));
    /// assert_eq!(state.chat().prompt().text(), "");
    /// // `/me TEXT` sends an action, and `/quit` ends the TUI.
    /// for c in "/me waves".chars() {
    ///     state.on_key(Frame::Chat(Key::Type(c)));
    /// }
    /// assert_eq!(state.on_key(Frame::Chat(Key::Send)), Some(Action::Post("/me waves".into())));
    /// for c in "/quit".chars() {
    ///     state.on_key(Frame::Chat(Key::Type(c)));
    /// }
    /// assert_eq!(state.on_key(Frame::Chat(Key::Send)), Some(Action::Quit));
    /// // A line for a lead that the riff does not have is not sent: it
    /// // stays, and the note line names the leads.
    /// state.on_key(Frame::Chat(Key::Clear));
    /// for c in "@lead:strata hi".chars() {
    ///     state.on_key(Frame::Chat(Key::Type(c)));
    /// }
    /// assert_eq!(state.on_key(Frame::Chat(Key::Send)), None);
    /// assert_eq!(state.chat().prompt().text(), "@lead:strata hi");
    /// assert_eq!(state.shown_note(), Some("mike has no lead."));
    /// // Esc moves the focus to the log, and `i` gives it back.
    /// state.on_key(Frame::Chat(Key::Log));
    /// assert_eq!(state.chat().focus(), Focus::Log);
    /// state.on_key(Frame::Chat(Key::Prompt));
    /// assert_eq!(state.chat().focus(), Focus::Prompt);
    /// ```
    pub(super) fn on_chat(&mut self, key: Key) -> Option<Action> {
        match key {
            Key::Send => self.send(),
            Key::Log => {
                self.chat.focus = Focus::Log;
                None
            }
            Key::Prompt => {
                self.chat.focus = Focus::Prompt;
                None
            }
            Key::Scroll(scroll) => {
                self.chat.scroll_by(scroll);
                None
            }
            edit => {
                self.chat.prompt.edit(edit);
                None
            }
        }
    }

    /// `Enter` in the prompt: the line to post, with the rules of
    /// `riff chat` ([`crate::chat::typed`], 01M6VG1PCXHNVJP7TSP0J5G5K9). A line that the chat cannot
    /// send stays in the prompt, and the note line says why.
    fn send(&mut self) -> Option<Action> {
        let text = self.chat.prompt.text().to_owned();
        let body = match typed(&text) {
            Typed::Nothing => return None,
            Typed::Quit => return Some(Action::Quit),
            Typed::Unknown(name) => {
                self.note(unknown(name));
                return None;
            }
            Typed::Say(text) => text.to_owned(),
            Typed::Action(text) => format!("{ACTION}{text}"),
        };
        let user = self.chat.user.clone()?;
        if let Err(e) = resolve(wakes(&body, &user), &self.leads()) {
            self.note(safe(&e));
            return None;
        }
        self.chat.prompt.take();
        self.chat.scroll = 0;
        self.note = None;
        Some(Action::Post(body))
    }
}

/// The note of a command that the chat does not know. It sends nothing.
///
/// ```
/// assert_eq!(
///     riff::tui::chat::unknown("foo"),
///     "Unknown command /foo. Use /me or /quit, or start the line with //."
/// );
/// ```
pub fn unknown(name: &str) -> String {
    format!(
        "Unknown command /{}. Use /me or {}, or start the line with //.",
        safe(name),
        crate::chat::QUIT
    )
}

/// The note for the line `text` that `user` types: whom `Enter` will
/// wake, with the leads of `leads` (01M5QVH7KDK3R2S4NDFB2V1XY9). The
/// bool is true for a line that the chat will not send: a command that
/// it does not know, or a lead that it cannot pick. `None` for an empty
/// line.
///
/// ```
/// use riff::tui::chat::wake_note;
/// use riff_core::name::SessionUri;
///
/// let leads: Vec<SessionUri> = [
///     "riff://mike@thelio/como-technologies/riff?session=m1&lead=true",
///     "riff://brett@heron/como-technologies/strata?session=b1&lead=true",
/// ].into_iter().map(|u| u.parse().unwrap()).collect();
/// let note = |text: &str| wake_note(text, "mike", &leads);
/// let wakes = |text: &str| Some((text.to_owned(), false));
/// assert_eq!(note("hi"), wakes("Enter wakes each lead of yours."));
/// assert_eq!(note("/me waves"), wakes("Enter wakes each lead of yours."));
/// assert_eq!(note("@brett hi"), wakes("Enter wakes each lead of brett."));
/// assert_eq!(
///     note("@lead:riff what is next?"),
///     wakes("Enter wakes your lead in como-technologies/riff.")
/// );
/// assert_eq!(
///     note("@brett:strata @lead ok?"),
///     wakes("Enter wakes the lead of brett in como-technologies/strata and each lead of yours.")
/// );
/// assert_eq!(note("/quit"), wakes("Enter ends the TUI. The sessions go on."));
/// assert_eq!(note("@ann:riff hi"), Some(("ann has no lead.".to_owned(), true)));
/// assert!(note("/foo").unwrap().1);
/// assert_eq!(note("  "), None);
/// ```
pub fn wake_note(text: &str, user: &str, leads: &[SessionUri]) -> Option<(String, bool)> {
    let body = match typed(text) {
        Typed::Nothing => return None,
        Typed::Quit => return Some(("Enter ends the TUI. The sessions go on.".to_owned(), false)),
        Typed::Unknown(name) => return Some((unknown(name), true)),
        Typed::Say(text) | Typed::Action(text) => text,
    };
    Some(match resolve(wakes(body, user), leads) {
        Ok(to) => (format!("Enter wakes {}.", whom(&to, user)), false),
        Err(e) => (safe(&e), true),
    })
}

/// The leads that `to` selects, in words, for the person `user`.
fn whom(to: &[Selector], user: &str) -> String {
    let parts: Vec<String> = to
        .iter()
        .map(|s| match (s.user.as_deref(), s.repo.as_deref()) {
            (Some(u), None) if u == user => "each lead of yours".to_owned(),
            (Some(u), None) => format!("each lead of {}", safe(u)),
            (Some(u), Some(repo)) if u == user => format!("your lead in {}", safe(repo)),
            (Some(u), Some(repo)) => format!("the lead of {} in {}", safe(u), safe(repo)),
            (None, _) => safe(&s.to_string()),
        })
        .collect();
    match parts.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// The note of the Chat tab, with its style: a post that failed after
/// the last key, else whom the line in the prompt will wake, while the
/// prompt holds a line (01M6VMACJGF4A0ZXC6YK13KNCH).
pub(super) fn note(state: &State) -> Option<Row<'static>> {
    let chat = &state.chat;
    if let Some(unsent) = &chat.unsent {
        return Some(Row::styled(safe(unsent), WARNING));
    }
    let user = chat.user.as_deref()?;
    let text = chat.prompt.text();
    let names_repo = wakes(text, user).iter().any(|s| s.repo.is_some());
    let leads = if names_repo {
        state.leads()
    } else {
        Vec::new()
    };
    let (note, warning) = wake_note(text, user, &leads)?;
    Some(if warning {
        Row::styled(note, WARNING)
    } else {
        Row::raw(note)
    })
}

/// The keys of the key line of the Chat tab, with the key that a click
/// on each presses (01M6VAZKG8FS4HEA9VSA4Z4YF1).
pub(super) fn keys(
    focus: Focus,
    layout: Layout,
) -> Vec<(&'static str, &'static str, Option<FrameKey>)> {
    let wide = layout == Layout::Wide;
    let chat = |key| Some(FrameKey::Chat(key));
    let mut keys = Vec::new();
    match focus {
        Focus::Prompt => {
            keys.push(("Enter", "send", chat(Key::Send)));
            keys.push(("Esc", "log", chat(Key::Log)));
            if wide {
                keys.push(("PgUp", "older", chat(Key::Scroll(Scroll::PageUp))));
                keys.push(("Alt-1..6", "tab", None));
            }
            keys.push(("Ctrl-C", "quit", Some(FrameKey::Quit)));
        }
        Focus::Log => {
            keys.push(("i", "type", chat(Key::Prompt)));
            if wide {
                keys.push(("↑↓ PgUp PgDn", "scroll", None));
                keys.push(("1-6", "tab", None));
                keys.push(("r", "read again", Some(FrameKey::Again)));
            } else {
                keys.push(("↑↓", "scroll", None));
            }
            keys.push(("?", "help", Some(FrameKey::Help)));
            keys.push(("q", "quit", Some(FrameKey::Quit)));
        }
    }
    keys
}

/// The keys of the Chat tab in the help popup, before the keys of the
/// frame (01M6VAZKDEQ6951KGPGAMF1SNJ).
pub const HELP: [(&str, &str); 8] = [
    ("Enter", "send the line, or go back to the prompt"),
    ("Esc", "leave the prompt: the keys of the tab work"),
    ("i", "go back to the prompt"),
    ("↑ ↓", "a line that you sent, or scroll the log"),
    ("PgUp, PgDn", "older or newer lines"),
    ("g, G", "the oldest or the newest line, in the log"),
    ("Alt-1 to 6", "open a tab, also from the prompt"),
    ("/me TEXT", "send an action"),
];

/// Places the body of the Chat tab in `area` on `view`: the summary
/// row, the log, the prompt, the places that a click hits, the cursor
/// and the window of the log.
pub(super) fn place(state: &State, area: Rect, layout: Layout, view: &mut View) {
    let chat = &state.chat;
    let margin = u16::from(layout == Layout::Wide);
    let columns = area.width - margin;
    let text_row = |y: u16| Rect::new(area.x + margin, area.y + y, columns, 1);
    view.rows.push(Placed {
        area: text_row(2),
        line: summary(state, layout),
    });

    let top = 3;
    let prompt = area.height - 3;
    let rows = prompt - top;
    for (y, (line, rule)) in (top..).zip(window(chat, columns, area.width, rows)) {
        let place = if rule {
            Rect::new(area.x, area.y + y, area.width, 1)
        } else {
            text_row(y)
        };
        view.rows.push(Placed { area: place, line });
    }
    view.chat = Some(Window { columns, rows });

    let place = Rect::new(area.x, area.y + prompt, area.width, 1);
    let (line, cursor) = prompt_row(chat, area.width);
    view.rows.push(Placed { area: place, line });
    if chat.focus == Focus::Prompt {
        view.cursor = Some(Position::new(area.x + cursor, area.y + prompt));
    }
    view.hits.push(Hit {
        area: place,
        key: FrameKey::Chat(Key::Prompt),
    });
    view.hits.push(Hit {
        area: Rect::new(area.x, area.y + top, area.width, rows),
        key: FrameKey::Chat(Key::Log),
    });
}

/// The summary row: the people who are online and away, and the state
/// of the stream while it is down. The narrow layout counts the people.
fn summary(state: &State, layout: Layout) -> Row<'static> {
    let wide = layout == Layout::Wide;
    let mut parts: Vec<Span<'static>> = vec![Span::raw("chat")];
    match &state.chat.down {
        Some(Down::Reconnecting) => parts.push(Span::styled(crate::view::RECONNECTING, DIM)),
        Some(Down::Fault(fault)) => parts.push(Span::styled(fault.clone(), ERROR)),
        None => {}
    }
    if let Some((look, _)) = state.last_look() {
        let (online, away): (Vec<_>, Vec<_>) = look.who.people.iter().partition(|p| p.live);
        for (word, people) in [("online", online), ("away", away)] {
            if people.is_empty() {
                continue;
            }
            parts.push(Span::raw(if wide {
                let names: Vec<String> = people.iter().map(|p| safe(&p.user)).collect();
                format!("{word}: {}", names.join(", "))
            } else {
                format!("{} {word}", people.len())
            }));
        }
    }
    if wide {
        parts.push(Span::raw("a line wakes your leads"));
    }
    let mut spans = Vec::new();
    for (i, part) in parts.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" · "));
        }
        spans.push(part);
    }
    Row::from(spans)
}

/// The rows of the log that fit `rows`, with `columns` for a row of
/// text and `full` for a rule. The bool is true for a rule. The newest
/// rows show, less the scroll. A log with fewer rows starts at the top.
fn window(chat: &Chat, columns: u16, full: u16, rows: u16) -> Vec<(Row<'static>, bool)> {
    let rows = usize::from(rows);
    let want = chat.scroll.saturating_add(rows);
    let mut got: VecDeque<(Row<'static>, bool)> = VecDeque::new();
    for i in (0..chat.entries.len()).rev() {
        for row in rows_of(&chat.entries, i, columns, full).into_iter().rev() {
            got.push_front(row);
        }
        if got.len() >= want {
            break;
        }
    }
    let scroll = chat.scroll.min(got.len().saturating_sub(rows));
    let end = got.len() - scroll;
    let start = end.saturating_sub(rows);
    got.range(start..end).cloned().collect()
}

/// The rows of the entry `i` of `entries`: a rule of `full` columns
/// first when its day is new, then the line in rows of `columns`.
fn rows_of(
    entries: &VecDeque<Entry>,
    i: usize,
    columns: u16,
    full: u16,
) -> Vec<(Row<'static>, bool)> {
    let width = usize::from(columns.max(1));
    let entry = &entries[i];
    let mut rows = Vec::new();
    if let Some(day) = entry.day() {
        let before = entries.range(..i).rev().find_map(Entry::day);
        if before != Some(day) {
            rows.push((rule(day, full), true));
        }
    }
    match entry {
        Entry::Line { line, at } => {
            rows.extend(
                line_rows(line, at, width)
                    .into_iter()
                    .map(|row| (row, false)),
            );
        }
        Entry::Lost(n) => rows.extend(
            textwrap::wrap(&crate::text::lost_messages(*n), width)
                .into_iter()
                .map(|row| (Row::styled(row.into_owned(), WARNING), false)),
        ),
    }
    rows
}

/// The rule of a new day: `──── Friday 2026-11-06 ────…`, `full`
/// columns wide.
fn rule(day: NaiveDate, full: u16) -> Row<'static> {
    let head = format!("──── {} ", day.format("%A %Y-%m-%d"));
    let rest = usize::from(full).saturating_sub(textwrap::core::display_width(&head));
    Row::styled(format!("{head}{}", "─".repeat(rest)), DIM)
}

/// The rows of a chat line in `width` columns: the time, the sender and
/// the body, as `riff chat` shows them. Each more row starts under the
/// sender.
fn line_rows(line: &Line, at: &DateTime<FixedOffset>, width: usize) -> Vec<Row<'static>> {
    let said = crate::chat::said(line);
    let time = at.format("%H:%M").to_string();
    let who = format!("{}{}{}", said.before, said.name, said.after);
    let mut body: Vec<&str> = said.body.lines().collect();
    if body.is_empty() {
        body.push("");
    }
    let mut rows = Vec::new();
    for (j, text) in body.into_iter().enumerate() {
        // First fit, as a terminal wraps: the rows do not move when the
        // next word comes.
        let options = textwrap::Options::new(width)
            .wrap_algorithm(textwrap::WrapAlgorithm::FirstFit)
            .subsequent_indent(INDENT);
        let first = format!("{time} {who} {text}");
        let wrapped = if j == 0 {
            textwrap::wrap(&first, options)
        } else {
            textwrap::wrap(text, options.initial_indent(INDENT))
        };
        for (k, row) in wrapped.into_iter().enumerate() {
            let row = row.into_owned();
            let head = format!("{time} {who}");
            if j == 0 && k == 0 && row.starts_with(&head) {
                let rest = row[head.len()..].to_owned();
                let name = ratatui_style(crate::style::session(&line.from));
                rows.push(Row::from(vec![
                    Span::styled(time.clone(), DIM),
                    Span::raw(format!(" {}", said.before)),
                    Span::styled(said.name.clone(), name),
                    Span::raw(format!("{}{rest}", said.after)),
                ]));
            } else {
                rows.push(Row::raw(row));
            }
        }
    }
    rows
}

/// The ratatui style of an ANSI style of [`crate::style`]: its color
/// and its bold, dim and underline. So a sender has the color that it
/// has in `riff chat`.
///
/// ```
/// use ratatui::style::{Color, Modifier};
///
/// let style = riff::tui::chat::ratatui_style(riff::style::session(&"riff://mike@thelio".parse()?));
/// assert!(style.add_modifier.contains(Modifier::BOLD | Modifier::UNDERLINED));
/// let lead = "riff://mike@thelio/o/r?session=4e54d4e5&lead=true".parse()?;
/// assert!(riff::tui::chat::ratatui_style(riff::style::session(&lead)).fg.is_some());
/// assert_eq!(riff::tui::chat::ratatui_style(riff::style::WARNING).fg, Some(Color::Yellow));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn ratatui_style(style: anstyle::Style) -> Style {
    use anstyle::{AnsiColor, Effects};
    let mut out = Style::new();
    if let Some(anstyle::Color::Ansi(color)) = style.get_fg_color() {
        out = out.fg(match color {
            AnsiColor::Black => Color::Black,
            AnsiColor::Red => Color::Red,
            AnsiColor::Green => Color::Green,
            AnsiColor::Yellow => Color::Yellow,
            AnsiColor::Blue => Color::Blue,
            AnsiColor::Magenta => Color::Magenta,
            AnsiColor::Cyan => Color::Cyan,
            AnsiColor::White => Color::Gray,
            AnsiColor::BrightBlack => Color::DarkGray,
            AnsiColor::BrightRed => Color::LightRed,
            AnsiColor::BrightGreen => Color::LightGreen,
            AnsiColor::BrightYellow => Color::LightYellow,
            AnsiColor::BrightBlue => Color::LightBlue,
            AnsiColor::BrightMagenta => Color::LightMagenta,
            AnsiColor::BrightCyan => Color::LightCyan,
            AnsiColor::BrightWhite => Color::White,
        });
    }
    let effects = style.get_effects();
    for (effect, modifier) in [
        (Effects::BOLD, Modifier::BOLD),
        (Effects::DIMMED, Modifier::DIM),
        (Effects::UNDERLINE, Modifier::UNDERLINED),
    ] {
        if effects.contains(effect) {
            out = out.add_modifier(modifier);
        }
    }
    out
}

/// The prompt row in `width` columns, and the column of the cursor. A
/// long line shows the part around the cursor. While the log has the
/// focus, the prompt is dim.
fn prompt_row(chat: &Chat, width: u16) -> (Row<'static>, u16) {
    const MARK: &str = "> ";
    let room = usize::from(width).saturating_sub(MARK.len()).max(1);
    let chars: Vec<char> = chat.prompt.text().chars().collect();
    let cursor = chat.prompt.cursor().min(chars.len());
    // `at[i]` is the width of the first `i` characters.
    let mut at = vec![0];
    for c in &chars {
        let wide = textwrap::core::display_width(c.encode_utf8(&mut [0; 4]));
        at.push(at[at.len() - 1] + wide);
    }
    // The cursor needs a column of its own after the text.
    let mut start = 0;
    while start < cursor && at[cursor] - at[start] >= room {
        start += 1;
    }
    let mut end = cursor;
    while end < chars.len() && at[end + 1] - at[start] <= room {
        end += 1;
    }
    let shown: String = chars[start..end].iter().collect();
    let x = MARK.len() + at[cursor] - at[start];
    let style = match chat.focus {
        Focus::Prompt => Style::new(),
        Focus::Log => DIM,
    };
    (
        Row::styled(format!("{MARK}{shown}"), style),
        u16::try_from(x).unwrap_or(width),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::Look;
    use crate::tui::view::{compute_view, render};
    use ratatui::buffer::Buffer;
    use riff_core::build::Build;
    use riff_core::wire::{Kind, RiffOwner};

    /// The time of the screen tests: 13:20 UTC on 2026-11-06, the day of
    /// the mock-up.
    const NOW: i64 = 1_793_971_200_000;

    fn build(id: &str) -> Build {
        format!("0.1.0 {id} 2026-11-06T12:00:00Z").parse().unwrap()
    }

    /// A line of the thread at `time` on 2026-11-06, in UTC.
    fn said(time: &str, from: &str, body: &str) -> Came {
        let at = DateTime::parse_from_rfc3339(&format!("2026-11-06T{time}:00+00:00")).unwrap();
        let from: SessionUri = from.parse().unwrap();
        let line = Line {
            seq: 0,
            at_ms: u64::try_from(at.timestamp_millis()).unwrap(),
            lead: from.lead(),
            from,
            verified: true,
            to: vec![],
            kind: Kind::Message,
            body: body.into(),
        };
        Came::Seen(Entry::Line {
            line: Box::new(line),
            at,
        })
    }

    const MIKE_LEAD: &str = "riff://mike@thelio/como-technologies/riff?session=4e54d4e5&lead=true";
    const ANN_LEAD: &str = "riff://ann@heron/como-technologies/kadomony?session=a1&lead=true";
    const BRETT_LEAD: &str = "riff://brett@pangolin/como-technologies/strata?session=b1&lead=true";

    /// The lines of the mock-up of the Chat tab of #994.
    fn lines() -> Vec<Came> {
        vec![
            said(
                "09:14",
                MIKE_LEAD,
                "good morning: 2 pull requests merged overnight",
            ),
            said("09:15", "riff://ann@kadomony", "morning"),
            said("09:40", ANN_LEAD, "the plan of wave 7 is ready for ann"),
            said(
                "11:58",
                MIKE_LEAD,
                "wave 48 runs: 5 items free, 3 workers started",
            ),
            said(
                "12:01",
                "riff://brett@strata",
                "mike, can you look at #1608?",
            ),
            said(
                "12:02",
                "riff://mike@thelio",
                "@brett:strata yes, after lunch",
            ),
            said(
                "12:02",
                BRETT_LEAD,
                "noted: mike looks at #1608 after lunch",
            ),
            said("12:20", "riff://mike@thelio", "/me goes for lunch"),
            said(
                "12:31",
                MIKE_LEAD,
                "blocked: 3a3f8d5d (#994) asks: which of two designs?",
            ),
            said(
                "12:46",
                MIKE_LEAD,
                "#994 still has no answer; I wait for mike",
            ),
            said(
                "13:05",
                "riff://mike@thelio",
                "back. @lead:riff the first design, with the tab for waves",
            ),
            said("13:05", MIKE_LEAD, "ok: #994 goes on with the first design"),
            said(
                "13:09",
                MIKE_LEAD,
                "#1608: the verify passed; the merge waits for CI",
            ),
            said("13:12", MIKE_LEAD, "#1608 merged"),
        ]
    }

    /// The state of the mock-up: a riff of the owner mike that runs,
    /// with mike and brett online and ann away, the lead of mike in
    /// como-technologies/riff, the lines of the mock-up, and news in
    /// the waves.
    fn mock_up() -> State {
        let mut state = State::new(build("10df8a4c0ffe"));
        let mut look = Look::default();
        look.who.owner = RiffOwner::Owner {
            user: "mike".into(),
            email: "m@x.io".into(),
        };
        look.who.people = serde_json::from_value(serde_json::json!([
            {"user": "mike", "role": "owner", "live": true},
            {"user": "brett", "role": "member", "live": true},
            {"user": "ann", "role": "admin", "live": false, "seen_secs": 3600},
        ]))
        .unwrap();
        look.who.sessions = serde_json::from_value(serde_json::json!([
            {"uri": MIKE_LEAD, "live": true},
            {"uri": BRETT_LEAD, "live": true},
        ]))
        .unwrap();
        look.server = Some(build("10df8a4c0ffe"));
        state.looked(look, NOW - 2_000);
        state.chat_as("mike", 0);
        for came in lines() {
            state.chat_came(came);
        }
        state.news(Tab::Waves, News::Dot);
        state.on_key(FrameKey::Open(Tab::Chat));
        state
    }

    /// Types `text` in the prompt.
    fn type_in(state: &mut State, text: &str) {
        for c in text.chars() {
            state.on_key(FrameKey::Chat(Key::Type(c)));
        }
    }

    /// The rows of `state` drawn into a `Buffer` of `width` x `height`,
    /// with no spaces at their ends, and the view.
    fn drawn(state: &mut State, width: u16, height: u16) -> (Vec<String>, View) {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        let view = compute_view(state, area, NOW);
        render(&view, &mut buffer);
        state.drawn(&view);
        let rows = (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect();
        (rows, view)
    }

    /// The mock-up of the Chat tab of #994 at 80 x 24: the summary
    /// row, the rule of the day, the lines as `riff chat` shows them, the
    /// prompt with the cursor after the text, the note of the wake and
    /// the keys of the prompt.
    #[test]
    fn the_chat_tab_at_80_x_24_is_the_mock_up() {
        let mut state = mock_up();
        type_in(&mut state, "@lead:riff what is next?");
        let (rows, view) = drawn(&mut state, 80, 24);
        let mut want = vec![
            " 1 Sessions  [2 Chat]  3 Waves•  4 Machines  5 Devices  6 Reports".to_owned(),
            " riff running · owner mike · build 10df8a4 · link ok 2s".to_owned(),
            " chat · online: mike, brett · away: ann · a line wakes your leads".to_owned(),
            format!("──── Friday 2026-11-06 {}", "─".repeat(57)),
        ];
        want.extend(
            [
                " 09:14 [mike's lead, riff] good morning: 2 pull requests merged overnight",
                " 09:15 <ann@kadomony> morning",
                " 09:40 [ann's lead, kadomony] the plan of wave 7 is ready for ann",
                " 11:58 [mike's lead, riff] wave 48 runs: 5 items free, 3 workers started",
                " 12:01 <brett@strata> mike, can you look at #1608?",
                " 12:02 <mike@thelio> @brett:strata yes, after lunch",
                " 12:02 [brett's lead, strata] noted: mike looks at #1608 after lunch",
                " 12:20 * mike@thelio goes for lunch",
                " 12:31 [mike's lead, riff] blocked: 3a3f8d5d (#994) asks: which of two designs?",
                " 12:46 [mike's lead, riff] #994 still has no answer; I wait for mike",
                " 13:05 <mike@thelio> back. @lead:riff the first design, with the tab for waves",
                " 13:05 [mike's lead, riff] ok: #994 goes on with the first design",
                " 13:09 [mike's lead, riff] #1608: the verify passed; the merge waits for CI",
                " 13:12 [mike's lead, riff] #1608 merged",
                "",
                "",
                "",
                "> @lead:riff what is next?",
                " Enter wakes your lead in como-technologies/riff.",
                " Enter send  Esc log  PgUp older  Alt-1..6 tab  Ctrl-C quit",
            ]
            .map(str::to_owned),
        );
        assert_eq!(rows, want);
        // The cursor is after the text.
        assert_eq!(
            view.cursor,
            Some(Position::new(
                u16::try_from("> @lead:riff what is next?".len()).unwrap(),
                21
            ))
        );
    }

    /// The narrow mock-up of the Chat tab at 40 x 24
    /// (01M6VAZKK4KHCDCX4RKG9B95D7): no margin, the people as counts,
    /// the long lines wrap under the sender, and the newest rows show.
    #[test]
    fn the_chat_tab_at_40_x_24_is_the_narrow_mock_up() {
        let mut state = mock_up();
        type_in(&mut state, "@brett hi");
        let (rows, _) = drawn(&mut state, 40, 24);
        let want = [
            "‹ 2/6 Chat ›",
            "riff running · link ok 2s",
            "chat · 2 online · 1 away",
            "      after lunch",
            "12:02 [brett's lead, strata] noted: mike",
            "      looks at #1608 after lunch",
            "12:20 * mike@thelio goes for lunch",
            "12:31 [mike's lead, riff] blocked:",
            "      3a3f8d5d (#994) asks: which of two",
            "      designs?",
            "12:46 [mike's lead, riff] #994 still has",
            "      no answer; I wait for mike",
            "13:05 <mike@thelio> back. @lead:riff the",
            "      first design, with the tab for",
            "      waves",
            "13:05 [mike's lead, riff] ok: #994 goes",
            "      on with the first design",
            "13:09 [mike's lead, riff] #1608: the",
            "      verify passed; the merge waits for",
            "      CI",
            "13:12 [mike's lead, riff] #1608 merged",
            "> @brett hi",
            "Enter wakes each lead of brett.",
            "Enter send  Esc log  Ctrl-C quit",
        ];
        assert_eq!(rows, want);
    }

    /// Esc moves the focus to the log: the prompt is dim with no cursor,
    /// and the key line has the keys of the log and of the frame. A click
    /// on the prompt gives the focus back.
    #[test]
    fn esc_moves_the_focus_to_the_log_and_a_click_on_the_prompt_gives_it_back() {
        let mut state = mock_up();
        type_in(&mut state, "half a li");
        state.on_key(FrameKey::Chat(Key::Log));
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(rows[21], "> half a li");
        assert_eq!(view.cursor, None);
        assert_eq!(
            rows[23],
            " i type  ↑↓ PgUp PgDn scroll  1-6 tab  r read again  ? help  q quit"
        );
        // The note says whom the line will wake, also while the log has
        // the focus.
        assert_eq!(rows[22], " Enter wakes each lead of yours.");
        let (narrow, _) = drawn(&mut state, 40, 24);
        assert_eq!(narrow[23], "i type  ↑↓ scroll  ? help  q quit");
        // A click on `i type` or on the prompt gives the focus back.
        assert_eq!(view.hit(2, 23), Some(FrameKey::Chat(Key::Prompt)));
        assert_eq!(view.hit(10, 21), Some(FrameKey::Chat(Key::Prompt)));
        assert_eq!(view.hit(10, 5), Some(FrameKey::Chat(Key::Log)));
        state.on_click(&view, 10, 21);
        assert_eq!(state.chat().focus(), Focus::Prompt);
        let (_, view) = drawn(&mut state, 80, 24);
        assert_eq!(view.hit(2, 23), Some(FrameKey::Chat(Key::Send)));
        assert_eq!(view.hit(14, 23), Some(FrameKey::Chat(Key::Log)));
    }

    /// A post that fails while the prompt holds a line shows at once in
    /// the note line, before the note of the wake. The next key brings
    /// back the note of the wake (01M6VMACJGF4A0ZXC6YK13KNCH).
    #[test]
    fn a_post_that_fails_while_the_prompt_holds_a_line_shows_until_the_next_key() {
        let mut state = mock_up();
        type_in(&mut state, "@lead:riff what is next?");
        let failed = "riff: cannot send the line: cannot reach riff-server";
        state.chat_came(Came::Unsent(failed.into()));
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(rows[21], "> @lead:riff what is next?");
        assert_eq!(rows[22], format!(" {failed}"));
        let note = &view.rows.iter().find(|r| r.area.y == 22).unwrap().line;
        assert_eq!(note.style, WARNING);
        // The next key brings back the note of the wake.
        state.on_key(FrameKey::Chat(Key::End));
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(
            rows[22],
            " Enter wakes your lead in como-technologies/riff."
        );
        // With an empty prompt, the failure is the result of the last
        // action.
        state.on_key(FrameKey::Chat(Key::Clear));
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[22], format!(" {failed}"));
    }

    /// PgUp shows older rows and PgDn newer, in the window of the last
    /// draw. A new line while the person looks at older rows keeps the
    /// window on them.
    #[test]
    fn page_up_shows_older_rows_and_a_new_line_keeps_the_window() {
        let mut state = mock_up();
        for n in 0..20 {
            state.chat_came(said(
                &format!("14:{n:02}"),
                "riff://brett@strata",
                &format!("line {n}"),
            ));
        }
        // 29 rows of the mock-up and 20 more: 49 rows, and 18 show.
        let (rows, _) = drawn(&mut state, 40, 24);
        assert_eq!(rows[20], "14:19 <brett@strata> line 19");
        // PgUp shows a page of 17 older rows: the top row of the last
        // window stays in sight.
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::PageUp)));
        let (page, _) = drawn(&mut state, 40, 24);
        assert_eq!(state.chat().scrolled(), 17);
        assert_eq!(page[20], rows[3]);
        // At the top, PgUp stays there.
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::PageUp)));
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::PageUp)));
        let (top, _) = drawn(&mut state, 40, 24);
        assert_eq!(state.chat().scrolled(), 31);
        assert_eq!(top[3], "──── Friday 2026-11-06 ─────────────────");
        assert_eq!(top[4], "09:14 [mike's lead, riff] good morning:");
        // A new line: the window stays.
        state.chat_came(said("14:30", "riff://brett@strata", "a new line"));
        let (kept, _) = drawn(&mut state, 40, 24);
        assert_eq!(kept, top);
        // PgDn shows newer rows, and End the newest line.
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::PageDown)));
        assert_eq!(state.chat().scrolled(), 15);
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::Newest)));
        let (rows, _) = drawn(&mut state, 40, 24);
        assert_eq!(rows[20], "14:30 <brett@strata> a new line");
        // A send shows the newest line too.
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::Up(3))));
        assert_eq!(state.chat().scrolled(), 3);
        type_in(&mut state, "ok");
        state.on_key(FrameKey::Chat(Key::Send));
        assert_eq!(state.chat().scrolled(), 0);
    }

    /// A line in a new day gets a rule, a body of more lines starts each
    /// line under the sender, and lost lines show a warning.
    #[test]
    fn a_new_day_gets_a_rule_and_the_lost_lines_a_warning() {
        let mut state = mock_up();
        let next = DateTime::parse_from_rfc3339("2026-11-07T08:00:00+00:00").unwrap();
        let mut line = match said("13:12", "riff://ann@kadomony", "") {
            Came::Seen(Entry::Line { line, .. }) => line,
            other => panic!("{other:?}"),
        };
        line.body = "two\nrows".into();
        state.chat_came(Came::Seen(Entry::Lost(3)));
        state.chat_came(Came::Seen(Entry::Line { line, at: next }));
        let (rows, _) = drawn(&mut state, 80, 24);
        let text = rows.join("\n");
        assert!(
            text.contains(
                " riff: 3 messages of the break are lost. The server keeps only the last"
            ),
            "{text}"
        );
        let rule = format!("──── Saturday 2026-11-07 {}", "─".repeat(55));
        assert_eq!(
            &rows[18..21],
            [
                rule,
                " 08:00 <ann@kadomony> two".into(),
                "       rows".into()
            ]
        );
    }

    /// The summary row says when the stream is down, and the fault of a
    /// stream in red. The next connect clears it.
    #[test]
    fn the_summary_row_says_when_the_stream_is_down() {
        let mut state = mock_up();
        state.chat_came(Came::Reconnecting);
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(
            rows[2],
            " chat · (reconnecting…) · online: mike, brett · away: ann · a line wakes your leads"
                .chars()
                .take(80)
                .collect::<String>()
                .trim_end()
        );
        state.chat_came(Came::Up);
        let (rows, _) = drawn(&mut state, 80, 24);
        assert!(rows[2].starts_with(" chat · online:"), "{}", rows[2]);
        state.chat_came(Came::Fault("riff: cannot join the chat: no".into()));
        let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
        let summary = &view.rows.iter().find(|r| r.area.y == 2).unwrap().line;
        let fault = summary
            .spans
            .iter()
            .find(|s| s.content == "riff: cannot join the chat: no")
            .unwrap();
        assert_eq!(fault.style, ERROR);
    }

    /// A long line in the prompt shows the part around the cursor.
    #[test]
    fn a_long_line_in_the_prompt_shows_the_part_at_the_cursor() {
        let mut state = mock_up();
        let long = "a".repeat(30) + &"b".repeat(30);
        type_in(&mut state, &long);
        let (rows, view) = drawn(&mut state, 40, 24);
        assert_eq!(rows[21], format!("> {}{}", "a".repeat(7), "b".repeat(30)));
        assert_eq!(view.cursor, Some(Position::new(39, 21)));
        state.on_key(FrameKey::Chat(Key::Home));
        let (rows, view) = drawn(&mut state, 40, 24);
        assert_eq!(rows[21], format!("> {}{}", "a".repeat(30), "b".repeat(8)));
        assert_eq!(view.cursor, Some(Position::new(2, 21)));
        // A wide character takes two columns: the buffer has a blank cell
        // after it.
        state.on_key(FrameKey::Chat(Key::End));
        state.on_key(FrameKey::Chat(Key::Clear));
        type_in(&mut state, &"日".repeat(25));
        let (rows, view) = drawn(&mut state, 40, 24);
        assert_eq!(rows[21], format!("> {}", "日 ".repeat(18).trim_end()));
        assert_eq!(view.cursor, Some(Position::new(38, 21)));
    }

    /// The text of a line from the thread is safe: no escape reaches the
    /// terminal.
    #[test]
    fn a_line_with_an_escape_shows_with_no_escape() {
        let mut state = mock_up();
        state.chat_came(said(
            "13:15",
            "riff://brett@strata",
            "hi\x1b]0;owned\x07 all\x1b[2J",
        ));
        let (rows, _) = drawn(&mut state, 80, 24);
        assert!(
            rows.contains(&" 13:15 <brett@strata> hi all".to_owned()),
            "{rows:#?}"
        );
    }

    /// The help popup of the Chat tab has its keys, then the keys of the
    /// frame.
    #[test]
    fn the_help_popup_has_the_keys_of_the_chat() {
        let mut state = mock_up();
        state.on_key(FrameKey::Chat(Key::Log));
        state.on_key(FrameKey::Help);
        let (rows, _) = drawn(&mut state, 80, 24);
        let text = rows.join("\n");
        assert!(text.contains("Keys of the Chat tab"), "{text}");
        for (key, does) in HELP {
            assert!(
                rows.iter().any(|r| r.contains(key) && r.contains(does)),
                "{key}: {text}"
            );
        }
        assert!(text.contains("open the next tab"), "{text}");
    }
}
