//! The Chat tab of the TUI (#1640): the thread `chat`, as `riff chat`
//! shows it, with the log and the prompt (01M3NB5MY93KV9RKZGGSMZW00D).
//!
//! # Design
//!
//! ```text
//!  1 Sessions  [2 Chat]  3 Waves•  4 Machines  5 Devices  6 Reports
//!  riff running · owner mike · build 10df8a4 · link ok 2s
//!  chat · online: mike, brett · away: ann · @NAME wakes a person or a project
//! ──── Friday 2026-11-06 ─────────────────────────────────────────────
//!  09:14 <riff> good morning: 2 pull requests merged
//!  09:15 <ann> morning
//!  12:20 * mike goes for lunch
//!  12:40 <riff-bot> members: ann passed the owner role to mike.
//!
//! > @riff what is next?
//!
//!  Enter send  Esc log  PgUp older  Alt-1..6 tab  Ctrl-C quit
//! ```
//!
//! - **The summary row** names the people who are online and away, from
//!   the last look of `who` (01M6VG1P7BH32E5YAP0GJSNJJP), and says how a
//!   line wakes. While the stream of the thread is down, it says
//!   `reconnecting…`, or the fault in red.
//! - **The log** ([`Chat`]) keeps the last [`KEEP`] lines of the thread.
//!   A line shows as in `riff chat` ([`crate::chat::said`]), with the
//!   sender by the name to type to reach it (01M6Y1FB5MZNZ971CJRBWZ2F9X):
//!   a person as `<USER>`, a lead as `<PROJECT>`, riff-server itself as
//!   `<riff-bot>` (01M6Y4ZCTSTVM971BDC1F7GJPK), an action as `* NAME
//!   TEXT`. A line that riff writes as the person shows as the person,
//!   with the mark `[by riff]`. The names come from riff-server
//!   ([`Came::Names`]). A rule names each new day. A long line wraps, and
//!   each more row starts under the sender. The newest lines show: `PgUp`
//!   shows older lines ([`Scroll`],
//!   01M6VG1P7BH32E5YAP0GJSNJJP).
//! - **The row of the new lines** ([`new_below`],
//!   01M6Z8AHEHCPR1A3Z3CEAN59AR): a line that comes while the log is not
//!   at the bottom stays under the window, and the last row of the log
//!   counts those lines, until a scroll shows each of them.
//!   `End` shows the newest line: in the log, and in the prompt at the
//!   end of the line. A click on the row and a send show it too.
//!
//!   ```text
//!    12:40 <riff-bot> members: ann passed the owner role to mike.
//!    2 new lines below: End
//!   > @riff what is next?
//!   ```
//! - **The prompt** ([`Prompt`]) has the focus when you open the tab
//!   ([`Focus`]). There each key types, also the digits, `q` and `?`.
//!   `Alt-1` to `Alt-6`, `Tab` and `Ctrl-C` keep their work. `Esc` moves
//!   the focus to the log, where the keys of the frame work, and `i` or
//!   `Enter` gives it back (01M6VG1PA4PM47777ZT4PNGETF). A switch to
//!   another window and back gives the focus to the prompt too
//!   ([`State::focused`]), and a click on the log keeps it there
//!   (01M6XT6MCZ38XEN7ZHWTRF50Z5).
//! - **The line** wraps in rows of the width of the tab ([`wrap`],
//!   01M6XXDDYQG7M6WYNRXP35DZ1Y). The prompt grows up, one row for each
//!   row of the line, up to [`LINE_ROWS`], and the log has the rows over
//!   it. A longer line scrolls, so that the row of the cursor shows. `↑`
//!   and `↓` move the cursor one row ([`Prompt::edit`]). On the first
//!   row and the last row of a typed text, they go to its start and its
//!   end. Only on an empty line does `↑` bring back the lines that you
//!   sent ([`Sent`], 01M6Y8N8W5RE5H6MW3TNDH5BSY).
//!
//!   ```text
//!   > @riff please look at the plan of wave 52, and tell me which item goes first:
//!     the highlight in riff chat, or /msg. Then start it, and tell me when its pull
//!     request is open.█
//!   ```
//! - **A paste** goes into the prompt at the cursor, also from the log
//!   ([`super::State::on_paste`], 01M6XSK4GHNBJCR0RHMBGZE57G). Its new
//!   lines stay: the prompt shows each one as `↵` at the end of a row,
//!   and `Enter` sends the line as one message. A paste or a typed
//!   character that makes the line longer than [`POST_MAX`] stays out
//!   ([`too_long`], 01M6XSK4K2MDA2SK0A6KHN9HDT, 01M6XVCDEP1ZP9RS7DV795W79E).
//! - **The list of the names** ([`crate::typeahead`],
//!   01M6Y1EKGKNW2MH970PDF6PJK1, 01M6Y1ES82XHFGANKZS7AF3CP2): an `@` at
//!   the start of a word shows the names of the chat that fit the letters
//!   after it. Its rows take the last rows of the log over the prompt,
//!   under the text of the line, and the first name, the pick, is in
//!   reverse video. They take no row of the newest line of the log, and
//!   leave the log one row at least: in a short tab the list shows fewer
//!   names, and the pick at least (01M6Z8AHH9FW3A3132YF8XH7R9). `Tab` or
//!   `Enter` picks it ([`Key::Pick`]), and `Esc` closes the list
//!   ([`Key::Close`]): [`key_of`] gives these keys only while the list
//!   shows ([`Chat::list`]). [`Link`] reads the names at its start and
//!   each [`READ_EVERY`], and gives them as [`Came::Names`].
//!
//!   ```text
//!    13:12 <riff> #1608 merged
//!     riff  project  online
//!     rita  person   offline
//!   > ask @r
//!    Tab or Enter picks riff. Esc closes the list.
//!    Tab pick  Esc close  Ctrl-C quit
//!   ```
//! - **The note line** says what `Enter` does with a command, or why
//!   the chat will not send it ([`prompt_note`]). It never says whom a
//!   line wakes (01M6Y1A5PTCVZ6GRJ0FXMART2T). A post that fails, or a
//!   paste or a key that makes the line too long, shows there at once,
//!   until the next key (01M6VMACJGF4A0ZXC6YK13KNCH). While the list of
//!   the names shows, the note names its keys.
//! - **News:** each new line of the thread while another tab shows
//!   adds one to `Chat•N` in the strip (01M6VG1PFPMHRQ4F9XHJTYMSJY).
//!
//! The tab sends and follows as `riff chat` does, and as the person, not
//! as a session ([`Person`]). `Enter` reads the line with
//! [`crate::chat::typed`], and gives [`Action::Post`] to the loop: it
//! checks no name, and keeps no line for a name. [`Link`] posts each
//! line in its order with [`crate::chat::posts`], the code of `riff
//! chat`, with the names of the line. It follows the thread from the
//! start of the TUI, with [`crate::catch_up`]: a break of the stream
//! loses no line.
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
//!     P->>T: types "@riff hi", and Enter
//!     T->>L: Action::Post
//!     L->>S: post to name=riff
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
use riff_client::link::STREAM_RETRY;
use riff_client::threads::{ACTION, Line, Names};
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::ChatName;
use tokio::sync::mpsc;

use super::view::{Hit, Layout, Placed, View};
use super::{Action, Key as FrameKey, News, State, Tab};
use crate::catch_up::{self, Seen, Shown, Start};
use crate::chat::{INDENT, Sent, Typed, typed};
use crate::text::safe;
use crate::typeahead::{Ahead, List, READ_EVERY};

/// The most lines that the log keeps. An older line goes.
pub const KEEP: usize = 1000;

/// The most bytes of a line of the Chat tab: 64 KiB. A paste or a typed
/// character that makes the line longer stays out of it ([`too_long`],
/// 01M6XSK4K2MDA2SK0A6KHN9HDT, 01M6XVCDEP1ZP9RS7DV795W79E).
pub const POST_MAX: usize = 64 * 1024;

/// The rows that a turn of the mouse wheel scrolls.
const WHEEL: u16 = 3;

/// The style of the time, the rules and a dim prompt.
const DIM: Style = Style::new().add_modifier(Modifier::DIM);

/// The style of a warning: a line that the chat will not send or could
/// not send, the lost lines.
const WARNING: Style = Style::new().fg(Color::Yellow);

/// The style of a fault of the stream.
const ERROR: Style = Style::new().fg(Color::Red);

/// The style of the name that `Tab` and `Enter` pick in the list of the
/// names.
const PICKED: Style = Style::new().add_modifier(Modifier::REVERSED);

/// The style of the row of the new lines under the window of the log.
const NEW: Style = Style::new().add_modifier(Modifier::BOLD);

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
    /// Move the cursor to the end of the line. At the end of the line,
    /// show the newest line of the log, as [`Scroll::Newest`] does.
    End,
    /// Delete the text before the cursor: `Ctrl-U`.
    Clear,
    /// Move the cursor one row up, or on the first row to the start of
    /// the line. On an empty line, show the last line that you sent, and
    /// on a sent line, the one before it: `↑`.
    Up,
    /// Move the cursor one row down, or on the last row to the end of the
    /// line. On a sent line, show the one after it, or after the last one
    /// an empty line: `↓`.
    Down,
    /// Send the line: `Enter` in the prompt.
    Send,
    /// Put the first name of the list of the names in the line: `Tab` or
    /// `Enter` while the list shows.
    Pick,
    /// Close the list of the names: `Esc` while it shows.
    Close,
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
    /// The newest line: `G` or `End` in the log, and a click on the row
    /// of the new lines.
    Newest,
}

/// The key of the Chat tab in `event`, when the focus is `focus`, and
/// `listed` while the list of the names shows (01M6VG1PA4PM47777ZT4PNGETF,
/// 01M6Y1ES82XHFGANKZS7AF3CP2). `None`
/// for a key that the frame takes ([`super::input_of`]): `Alt-1` to
/// `Alt-6`, `Tab`, `Shift-Tab` and `Ctrl-C` in the prompt, and each key
/// of the frame in the log. While the list shows, `Tab` and `Enter` pick
/// a name, and `Esc` closes the list. The wheel of the mouse scrolls the
/// log.
///
/// ```
/// use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
/// use riff::tui::chat::{Focus, Key, Scroll, key_of};
///
/// let none = KeyModifiers::NONE;
/// let key = |code, modifiers, focus| {
///     key_of(&Event::Key(KeyEvent::new(code, modifiers)), focus, false)
/// };
/// let prompt = |code| key(code, none, Focus::Prompt);
/// let log = |code| key(code, none, Focus::Log);
/// // In the prompt, each key types: also a digit, `q` and `?`.
/// for c in ['q', '2', '?', 'r'] {
///     assert_eq!(prompt(KeyCode::Char(c)), Some(Key::Type(c)));
/// }
/// assert_eq!(prompt(KeyCode::Enter), Some(Key::Send));
/// assert_eq!(prompt(KeyCode::Esc), Some(Key::Log));
/// assert_eq!(prompt(KeyCode::Up), Some(Key::Up));
/// assert_eq!(prompt(KeyCode::PageUp), Some(Key::Scroll(Scroll::PageUp)));
/// // Alt-2, Tab and Ctrl-C go to the frame.
/// assert_eq!(key(KeyCode::Char('2'), KeyModifiers::ALT, Focus::Prompt), None);
/// assert_eq!(prompt(KeyCode::Tab), None);
/// assert_eq!(key(KeyCode::Char('c'), KeyModifiers::CONTROL, Focus::Prompt), None);
///
/// // While the list of the names shows, Tab and Enter pick, and Esc
/// // closes it. Alt-Tab goes to the frame.
/// let listed = |code, modifiers| {
///     key_of(&Event::Key(KeyEvent::new(code, modifiers)), Focus::Prompt, true)
/// };
/// assert_eq!(listed(KeyCode::Tab, none), Some(Key::Pick));
/// assert_eq!(listed(KeyCode::Enter, none), Some(Key::Pick));
/// assert_eq!(listed(KeyCode::Esc, none), Some(Key::Close));
/// assert_eq!(listed(KeyCode::Tab, KeyModifiers::ALT), None);
/// assert_eq!(listed(KeyCode::Char('r'), none), Some(Key::Type('r')));
///
/// // In the log, `i` and Enter go back to the prompt, and the keys of
/// // the frame go to the frame.
/// assert_eq!(log(KeyCode::Char('i')), Some(Key::Prompt));
/// assert_eq!(log(KeyCode::Enter), Some(Key::Prompt));
/// assert_eq!(log(KeyCode::Char('k')), Some(Key::Scroll(Scroll::Up(1))));
/// assert_eq!(log(KeyCode::Char('G')), Some(Key::Scroll(Scroll::Newest)));
/// assert_eq!(log(KeyCode::Char('q')), None);
/// assert_eq!(log(KeyCode::Char('2')), None);
/// ```
pub fn key_of(event: &Event, focus: Focus, listed: bool) -> Option<Key> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            let alt = key.modifiers.contains(KeyModifiers::ALT);
            // AltGr comes as Ctrl and Alt on some terminals.
            let plain = ctrl == alt;
            let shift = key.modifiers.contains(KeyModifiers::SHIFT);
            let key = match (focus, key.code) {
                (Focus::Prompt, KeyCode::Tab) if listed && !ctrl && !alt && !shift => Key::Pick,
                (Focus::Prompt, KeyCode::Enter) if listed => Key::Pick,
                (Focus::Prompt, KeyCode::Esc) if listed => Key::Close,
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
                (Focus::Prompt, KeyCode::Up) => Key::Up,
                (Focus::Prompt, KeyCode::Down) => Key::Down,
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
/// [`Prompt::edit`] takes a key in rows of a number of columns of text
/// ([`wrap`]).
///
/// ```
/// use riff::tui::chat::{Key, Prompt};
///
/// let mut prompt = Prompt::default();
/// for c in "hi al".chars() {
///     prompt.edit(Key::Type(c), 40);
/// }
/// prompt.edit(Key::Type('l'), 40);
/// prompt.edit(Key::Home, 40);
/// prompt.edit(Key::Delete, 40);
/// prompt.edit(Key::Type('H'), 40);
/// assert_eq!((prompt.text(), prompt.cursor()), ("Hi all", 1));
/// prompt.edit(Key::End, 40);
/// prompt.edit(Key::Backspace, 40);
/// assert_eq!(prompt.text(), "Hi al");
/// assert_eq!(prompt.take(), "Hi al");
/// assert_eq!(prompt.text(), "");
/// // On an empty line, ↑ brings back the line that you sent, and ↓
/// // empties the line again.
/// prompt.edit(Key::Up, 40);
/// assert_eq!((prompt.text(), prompt.cursor()), ("Hi al", 5));
/// prompt.edit(Key::Down, 40);
/// assert_eq!(prompt.text(), "");
/// // With a typed text, ↑ keeps the text, and goes to its start.
/// prompt.edit(Key::Type('x'), 40);
/// prompt.edit(Key::Up, 40);
/// assert_eq!((prompt.text(), prompt.cursor()), ("x", 0));
/// // Ctrl-U deletes the text before the cursor.
/// prompt.edit(Key::End, 40);
/// prompt.edit(Key::Clear, 40);
/// assert_eq!(prompt.text(), "");
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prompt {
    /// The text.
    text: String,
    /// The place of the cursor, in characters.
    cursor: usize,
    /// The lines that the person sent.
    sent: Sent,
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

    /// Takes a key of the prompt, with the line in rows of `columns`
    /// columns ([`wrap`], 01M6XXDDYQG7M6WYNRXP35DZ1Y). `←` and `→` go
    /// over the end of a row, and `Home` and `End` go to the start and
    /// the end of the line. `↑` and `↓` move the cursor one row, to the
    /// same column or to the end of a shorter row. On the first row, `↑`
    /// goes to the start of the line, and on the last row, `↓` to its
    /// end. Only on an empty line, or on a sent line that came back, they
    /// show the lines that you sent ([`Sent`],
    /// 01M6Y8N8W5RE5H6MW3TNDH5BSY). A key that scrolls or moves the
    /// focus does nothing here.
    ///
    /// ```
    /// use riff::tui::chat::{Key, Prompt};
    ///
    /// // In rows of 10 columns: "hello big " and "world".
    /// let mut prompt = Prompt::default();
    /// prompt.paste("hello big world");
    /// prompt.edit(Key::Up, 10);
    /// assert_eq!(prompt.cursor(), 5);
    /// prompt.edit(Key::Down, 10);
    /// assert_eq!(prompt.cursor(), 15);
    /// // ↑ on the first row goes to the start of the line, and ↓ on the
    /// // last row to its end. The text stays.
    /// prompt.edit(Key::Up, 10);
    /// prompt.edit(Key::Up, 10);
    /// assert_eq!((prompt.text(), prompt.cursor()), ("hello big world", 0));
    /// prompt.edit(Key::Down, 10);
    /// prompt.edit(Key::Down, 10);
    /// assert_eq!((prompt.text(), prompt.cursor()), ("hello big world", 15));
    /// // Home and End go to the start and the end of the line.
    /// prompt.edit(Key::Home, 10);
    /// assert_eq!(prompt.cursor(), 0);
    /// prompt.edit(Key::End, 10);
    /// assert_eq!(prompt.cursor(), 15);
    /// // From the end of the first row, `→` goes to the start of the next.
    /// prompt.edit(Key::Up, 10);
    /// prompt.edit(Key::End, 10);
    /// for _ in 0..6 {
    ///     prompt.edit(Key::Left, 10);
    /// }
    /// assert_eq!(prompt.cursor(), 9);
    /// prompt.edit(Key::Right, 10);
    /// prompt.edit(Key::Up, 10);
    /// assert_eq!(prompt.cursor(), 0);
    /// ```
    pub fn edit(&mut self, key: Key, columns: usize) {
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
            Key::Up => match self.row_to(true, columns) {
                Some(at) => self.cursor = at,
                None => match self.sent.up(&self.text).map(str::to_owned) {
                    Some(line) => self.show(line),
                    None => self.cursor = 0,
                },
            },
            Key::Down => match self.row_to(false, columns) {
                Some(at) => self.cursor = at,
                None => match self.sent.down(&self.text).map(str::to_owned) {
                    Some(line) => self.show(line),
                    None => self.cursor = length,
                },
            },
            _ => {}
        }
    }

    /// The place of the cursor one row up (`up`) or down, in rows of
    /// `columns` columns: at the same column, or at the end of a shorter
    /// row. `None` on the first row up and on the last row down.
    fn row_to(&self, up: bool, columns: usize) -> Option<usize> {
        let rows = Rows::of(&self.text, columns);
        let row = rows.row(self.cursor);
        let to = if up {
            row.checked_sub(1)?
        } else {
            Some(row + 1).filter(|&r| r < rows.starts.len())?
        };
        let column = rows.column(self.cursor);
        let range = rows.range(to);
        // The place after the last character of a row is the start of
        // the next row. Only the last row ends at the end of the line.
        let last = if to + 1 < rows.starts.len() {
            range.end - 1
        } else {
            range.end
        };
        let mut at = range.start;
        while at < last && rows.x[at + 1] - rows.x[range.start] <= column {
            at += 1;
        }
        Some(at)
    }

    /// Puts `text` in the line at the cursor, and the cursor after it.
    ///
    /// ```
    /// use riff::tui::chat::{Key, Prompt};
    ///
    /// let mut prompt = Prompt::default();
    /// for c in "ab".chars() {
    ///     prompt.edit(Key::Type(c), 40);
    /// }
    /// prompt.edit(Key::Left, 40);
    /// prompt.paste("1\n2");
    /// assert_eq!((prompt.text(), prompt.cursor()), ("a1\n2b", 4));
    /// ```
    pub fn paste(&mut self, text: &str) {
        let at = self.byte(self.cursor);
        self.text.insert_str(at, text);
        self.cursor += text.chars().count();
    }

    /// The byte of the cursor in the text.
    fn at(&self) -> usize {
        self.byte(self.cursor)
    }

    /// Puts the first name of `list` in the place of its word, and the
    /// cursor after it ([`List::pick`]).
    fn pick(&mut self, list: &List) {
        let (text, at) = list.pick(&self.text);
        self.cursor = text[..at].chars().count();
        self.text = text;
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
        self.sent.push(&text);
        self.cursor = 0;
        text
    }
}

/// The first character of each row of the line `text`, in rows of
/// `columns` columns (01M6XXDDYQG7M6WYNRXP35DZ1Y). When a character does
/// not fit, its row ends after the last space that fits in it. With no
/// such space, the row ends after the last character that fits. A new
/// line ends its row, and shows as `↵` at its end. When the last row is
/// full, an empty row comes after it: the cursor at the end of the line
/// needs a column of its own.
///
/// ```
/// use riff::tui::chat::wrap;
///
/// assert_eq!(wrap("hi all", 10), [0]);
/// // "hello big " and "world".
/// assert_eq!(wrap("hello big world", 10), [0, 10]);
/// // The space after "big" does not fit: "hello ", then "big world",
/// // a full row, then the empty row of the cursor.
/// assert_eq!(wrap("hello big world", 9), [0, 6, 15]);
/// // A word longer than a row breaks.
/// assert_eq!(wrap(&"x".repeat(25), 10), [0, 10, 20]);
/// // A full last row, and a new line at the end: an empty row comes
/// // after them.
/// assert_eq!(wrap(&"x".repeat(20), 10), [0, 10, 20]);
/// assert_eq!(wrap("one\ntwo\n", 10), [0, 4, 8]);
/// // A wide character takes two columns.
/// assert_eq!(wrap(&"日".repeat(5), 9), [0, 4]);
/// ```
pub fn wrap(text: &str, columns: usize) -> Vec<usize> {
    Rows::of(text, columns).starts
}

/// The line of the prompt in rows ([`wrap`]).
struct Rows {
    /// The characters as the prompt shows them: a new line as `↵`, and a
    /// tab as a space.
    shown: Vec<char>,
    /// The width of the first `i` characters, at `i`.
    x: Vec<usize>,
    /// The first character of each row.
    starts: Vec<usize>,
}

impl Rows {
    /// The rows of `text` in `columns` columns. With 0 columns, before
    /// the first draw, only a new line ends a row.
    fn of(text: &str, columns: usize) -> Rows {
        let columns = if columns == 0 { usize::MAX } else { columns };
        let chars: Vec<char> = text.chars().collect();
        let shown: Vec<char> = chars
            .iter()
            .map(|&c| match c {
                '\n' => '↵',
                '\t' => ' ',
                c => c,
            })
            .collect();
        let mut x = vec![0];
        for c in &shown {
            let wide = textwrap::core::display_width(c.encode_utf8(&mut [0; 4]));
            x.push(x[x.len() - 1] + wide);
        }
        let mut starts = vec![0];
        // The first character after the last space of the row.
        let mut space = None;
        for (i, c) in chars.iter().enumerate() {
            let start = starts[starts.len() - 1];
            if i > start && x[i + 1] - x[start] > columns {
                let soft = space.filter(|&s| x[i + 1] - x[s] <= columns);
                starts.push(soft.unwrap_or(i));
                space = None;
            }
            match c {
                ' ' | '\t' => space = Some(i + 1),
                '\n' => {
                    starts.push(i + 1);
                    space = None;
                }
                _ => {}
            }
        }
        if x[chars.len()] - x[starts[starts.len() - 1]] >= columns {
            starts.push(chars.len());
        }
        Rows { shown, x, starts }
    }

    /// The row of the cursor before the character `at`.
    fn row(&self, at: usize) -> usize {
        self.starts.partition_point(|&s| s <= at) - 1
    }

    /// The characters of the row `row`.
    fn range(&self, row: usize) -> std::ops::Range<usize> {
        let end = self.starts.get(row + 1).copied();
        self.starts[row]..end.unwrap_or(self.shown.len())
    }

    /// The column of the cursor before the character `at`, in its row.
    fn column(&self, at: usize) -> usize {
        self.x[at] - self.x[self.starts[self.row(at)]]
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
    /// The names of the chat, from a read of riff-server: the log shows
    /// each lead by the name of its project from them, and the list of
    /// the names shows them.
    Names(Vec<ChatName>),
}

/// Why the stream of the thread is down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Down {
    /// A connect failed: it connects again.
    Reconnecting,
    /// A fault for the person.
    Fault(String),
}

/// What the person saw of the log and of the line at the last draw. A
/// scroll of the log needs the columns of a row and the rows, and `↑`
/// and `↓` in the line need the columns of a row of the line. The next
/// draw starts the rows of the line at the same row when it can.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Window {
    /// The columns of a row of text of the log.
    pub columns: u16,
    /// The rows of the log.
    pub rows: u16,
    /// The columns of a row of text of the line.
    pub line: u16,
    /// The first row of the line that shows.
    pub line_top: usize,
}

/// The state of the Chat tab: the log, the prompt, the focus and the
/// scroll.
#[derive(Debug, Clone, Default)]
pub struct Chat {
    /// The start of the TUI: an older line is no news.
    since_ms: u64,
    /// The last [`KEEP`] entries, the oldest first.
    entries: VecDeque<Entry>,
    prompt: Prompt,
    focus: Focus,
    /// The rows of the log under the window: 0 shows the newest line.
    scroll: usize,
    /// The newest entries that came while the log was not at the
    /// bottom, and that no scroll showed since: the new lines of
    /// [`below`].
    new: usize,
    /// The window of the last draw.
    pub(super) window: Window,
    /// Why the stream of the thread is down, while it is.
    down: Option<Down>,
    /// The note of a post that failed or of a line that is too long,
    /// until the next key (01M6VMACJGF4A0ZXC6YK13KNCH).
    pub(super) warning: Option<String>,
    /// The names of the chat from the last read ([`Came::Names`]).
    names: Names,
    /// The type-ahead of the prompt: the list that `Esc` closed.
    ahead: Ahead,
}

impl Chat {
    /// The chat from the time `since_ms` (milliseconds since the epoch):
    /// a line from before is no news.
    pub fn new(since_ms: u64) -> Chat {
        Chat {
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

    /// The note of a post that failed or of a line that is too long,
    /// until the next key: it comes before the note of the prompt
    /// (01M6VMACJGF4A0ZXC6YK13KNCH).
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    /// The list of the names for the line in the prompt
    /// ([`crate::typeahead`]), while the prompt has the focus.
    pub fn list(&self) -> Option<List> {
        if self.focus != Focus::Prompt {
            return None;
        }
        let prompt = &self.prompt;
        self.ahead
            .list(self.names.list(), prompt.text(), prompt.at())
    }

    /// The person changed the line or moved the cursor: a closed list
    /// opens again when the cursor left its word.
    fn edited(&mut self) {
        let at = self.prompt.at();
        self.ahead.edited(self.prompt.text(), at);
    }

    /// Adds `entry` to the end of the log. While the person looks at
    /// older lines, the window stays on them, and the entry is a new line
    /// under it ([`below`]).
    fn add(&mut self, entry: Entry) {
        self.entries.push_back(entry);
        if self.scroll > 0 {
            let last = self.entries.len() - 1;
            let columns = self.window.columns;
            self.scroll += rows_of(self, last, columns, columns).len();
            self.new += 1;
        }
        if self.entries.len() > KEEP {
            self.entries.pop_front();
        }
        self.new = self.new.min(self.entries.len());
    }

    /// The rows of the whole log in the window of the last draw.
    fn total(&self) -> usize {
        let columns = self.window.columns;
        (0..self.entries.len())
            .map(|i| rows_of(self, i, columns, columns).len())
            .sum()
    }

    /// Scrolls the log by `scroll`, in the window of the last draw. A new
    /// line that the window shows is no longer new.
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
        self.new = below(self, self.window.columns, self.window.rows);
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
/// start of the TUI (01M6VG1PFPMHRQ4F9XHJTYMSJY), one posts the lines
/// that the person sends, in order, and one reads the names of the chat
/// at the start and each [`READ_EVERY`] (01M6Y1EKKCCZ5215V49Z5T3HKH).
pub struct Link {
    came: mpsc::UnboundedReceiver<Came>,
    send: mpsc::UnboundedSender<String>,
    posts: tokio::task::JoinHandle<()>,
    follow: tokio::task::JoinHandle<()>,
    names: tokio::task::JoinHandle<()>,
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
        let names = tokio::spawn(read_names(api.clone(), me.clone(), tx.clone()));
        let follow = tokio::spawn(async move { follow(&api, &me, &thread, &tx).await });
        Link {
            came,
            send,
            posts,
            follow,
            names,
        }
    }

    /// A link with no call, for a TUI with no sign-in of a person, as in
    /// a container session (#1733): the tab shows `why` as its fault, and
    /// each line that the person sends comes back unsent with `why`.
    pub fn off(why: &str) -> Link {
        let (tx, came) = mpsc::unbounded_channel();
        let _ = tx.send(Came::Fault(safe(why)));
        let (send, mut typed) = mpsc::unbounded_channel::<String>();
        let unsent = safe(&format!("riff: cannot send the line: {why}"));
        let posts = tokio::spawn(async move {
            while typed.recv().await.is_some() {
                let _ = tx.send(Came::Unsent(unsent.clone()));
            }
        });
        Link {
            came,
            send,
            posts,
            follow: tokio::spawn(async {}),
            names: tokio::spawn(async {}),
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
        self.names.abort();
        drop(self.send);
        let _ = self.posts.await;
    }
}

/// Reads the names of the chat as `me`, and gives each read to `tx`,
/// until its receiver goes. A read that fails gives nothing: the tab
/// keeps the names of the last read. With no names, each project has
/// the name after the slash.
async fn read_names(api: Api, me: SessionUri, tx: mpsc::UnboundedSender<Came>) {
    while !tx.is_closed() {
        if let Ok(names) = api.names(&me).await {
            let _ = tx.send(Came::Names(names));
        }
        tokio::time::sleep(READ_EVERY).await;
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

    /// A line of the Chat tab from before `since_ms` is no news.
    pub fn chat_since(&mut self, since_ms: u64) {
        self.chat = Chat::new(since_ms);
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
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// state.chat_since(5_000);
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
    /// assert_eq!(state.chat().warning(), Some("riff: cannot send the line: no"));
    /// state.on_key(Frame::Chat(Key::Type('a')));
    /// assert_eq!(state.chat().warning(), None);
    /// # Ok::<(), riff_core::name::NameError>(())
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
                self.chat.warning = Some(text.clone());
                self.note(text);
            }
            Came::Names(names) => self.chat.names = Names::new(names),
        }
    }

    /// The window of the terminal got the focus back
    /// (01M6XT6MCZ38XEN7ZHWTRF50Z5): the prompt of the Chat tab gets the
    /// focus, as when the person opens the tab. So an `Esc` that the
    /// terminal sent with the switch of windows does not keep the keys
    /// out of the line.
    ///
    /// ```
    /// use riff::tui::chat::{Focus, Key};
    /// use riff::tui::{Key as Frame, State, Tab};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// state.chat_since(0);
    /// state.on_key(Frame::Open(Tab::Chat));
    /// state.on_key(Frame::Chat(Key::Type('h')));
    /// state.on_key(Frame::Chat(Key::Log));
    /// assert_eq!(state.chat().focus(), Focus::Log);
    /// state.focused();
    /// assert_eq!(state.chat().focus(), Focus::Prompt);
    /// state.on_key(Frame::Chat(Key::Type('i')));
    /// assert_eq!(state.chat().prompt().text(), "hi");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn focused(&mut self) {
        self.chat.focus = Focus::Prompt;
    }

    /// Takes the key `key` of the Chat tab. A typed character that makes
    /// the line longer than [`POST_MAX`] stays out, as a paste does: the
    /// note line says [`too_long`] until the next key.
    ///
    /// ```
    /// use riff::tui::chat::{Focus, Key, POST_MAX, too_long};
    /// use riff::tui::{Action, Key as Frame, Look, State, Tab};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// state.looked(Look::default(), 0);
    /// state.chat_since(0);
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
    /// // Enter checks no name: a line with a name that no one has goes
    /// // too, and riff-server resolves it.
    /// state.on_key(Frame::Chat(Key::Clear));
    /// for c in "@lead:strata hi".chars() {
    ///     state.on_key(Frame::Chat(Key::Type(c)));
    /// }
    /// let sent = state.on_key(Frame::Chat(Key::Send));
    /// assert_eq!(sent, Some(Action::Post("@lead:strata hi".into())));
    /// // Also before the first look of the riff.
    /// let mut fresh = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// fresh.on_key(Frame::Open(Tab::Chat));
    /// for c in "@riff hi".chars() {
    ///     fresh.on_key(Frame::Chat(Key::Type(c)));
    /// }
    /// assert_eq!(fresh.on_key(Frame::Chat(Key::Send)), Some(Action::Post("@riff hi".into())));
    /// // A command that the chat does not know stays in the prompt. The
    /// // note of the prompt says why, and Enter adds no note of its own.
    /// for c in "/foo".chars() {
    ///     state.on_key(Frame::Chat(Key::Type(c)));
    /// }
    /// assert_eq!(state.on_key(Frame::Chat(Key::Send)), None);
    /// assert_eq!(state.chat().prompt().text(), "/foo");
    /// assert_eq!(state.shown_note(), None);
    /// // Esc moves the focus to the log, and `i` gives it back.
    /// state.on_key(Frame::Chat(Key::Log));
    /// assert_eq!(state.chat().focus(), Focus::Log);
    /// state.on_key(Frame::Chat(Key::Prompt));
    /// assert_eq!(state.chat().focus(), Focus::Prompt);
    /// // A character that makes the line longer than 64 KiB stays out,
    /// // and the note line says the limit.
    /// state.on_key(Frame::Chat(Key::Clear));
    /// state.on_paste(&"x".repeat(POST_MAX));
    /// state.on_key(Frame::Chat(Key::Type('y')));
    /// assert_eq!(state.chat().prompt().text().len(), POST_MAX);
    /// assert_eq!(state.chat().warning(), Some(too_long().as_str()));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub(super) fn on_chat(&mut self, key: Key) -> Option<Action> {
        let action = match key {
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
            // `End` at the end of the line shows the newest line, as `End`
            // in the log does: the row of the new lines says `End`.
            Key::End if self.chat.prompt.at() == self.chat.prompt.text().len() => {
                self.chat.scroll_by(Scroll::Newest);
                None
            }
            Key::Pick => {
                self.pick();
                None
            }
            Key::Close => {
                let at = self.chat.prompt.at();
                self.chat.ahead.close(self.chat.prompt.text(), at);
                None
            }
            Key::Type(c) if self.chat.prompt.text().len() + c.len_utf8() > POST_MAX => {
                self.over_limit();
                None
            }
            edit => {
                let columns = usize::from(self.chat.window.line);
                self.chat.prompt.edit(edit, columns);
                None
            }
        };
        self.chat.edited();
        action
    }

    /// `Tab` or `Enter` while the list of the names shows: the first
    /// name takes the place of the word ([`List::pick`]). A pick that
    /// makes the line longer than [`POST_MAX`] stays out, as a key does.
    fn pick(&mut self) {
        let Some(list) = self.chat.list() else {
            return;
        };
        if list.pick(self.chat.prompt.text()).0.len() > POST_MAX {
            self.over_limit();
            return;
        }
        self.chat.prompt.pick(&list);
    }

    /// A paste in the Chat tab ([`super::State::on_paste`]): the prompt
    /// gets the focus, and the text goes into the line at the cursor
    /// ([`pasted`]). A paste that makes the line longer than
    /// [`POST_MAX`] stays out: the line keeps its text, and the note line
    /// says [`too_long`] until the next key.
    pub(super) fn chat_paste(&mut self, text: &str) {
        self.chat.focus = Focus::Prompt;
        let text = pasted(text);
        if self.chat.prompt.text().len() + text.len() > POST_MAX {
            self.over_limit();
            return;
        }
        self.chat.prompt.paste(&text);
        self.chat.edited();
    }

    /// A paste or a typed key that makes the line longer than
    /// [`POST_MAX`]: the note line says [`too_long`] until the next key,
    /// and not after it: it is no result of an action.
    fn over_limit(&mut self) {
        self.chat.warning = Some(too_long());
    }

    /// `Enter` in the prompt: the line to post, with the rules of
    /// `riff chat` ([`crate::chat::typed`], 01M6VG1PCXHNVJP7TSP0J5G5K9).
    /// It checks no name (01M6Y1A5PTCVZ6GRJ0FXMART2T). A command that the
    /// chat does not know stays in the prompt, and the note of the prompt
    /// says why ([`prompt_note`]). It gives no note of the last action:
    /// that note would stay after the line goes (01M6XTQHMY4VS04YGWY17BSTM8).
    fn send(&mut self) -> Option<Action> {
        let text = self.chat.prompt.text().to_owned();
        let body = match typed(&text) {
            Typed::Nothing | Typed::Unknown(_) => return None,
            Typed::Quit => return Some(Action::Quit),
            Typed::Say(text) => text.to_owned(),
            Typed::Action(text) => format!("{ACTION}{text}"),
        };
        self.chat.prompt.take();
        self.chat.scroll = 0;
        self.chat.new = 0;
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

/// The text of a paste, safe for the prompt: each line end, `\r\n` or
/// `\r`, is a new line `\n`, and no escape sequence and no control
/// character stays, a new line and a tab apart.
///
/// ```
/// use riff::tui::chat::pasted;
///
/// assert_eq!(pasted("one\r\ntwo\rthree\n"), "one\ntwo\nthree\n");
/// assert_eq!(pasted("{\n\t\"a\": 1\n}"), "{\n\t\"a\": 1\n}");
/// assert_eq!(pasted("hi\x1b]0;owned\x07 all\x1b[2J\x08"), "hi all");
/// ```
pub fn pasted(text: &str) -> String {
    safe(&text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// The note of a paste or a typed character that makes the line longer
/// than [`POST_MAX`]: it names the line, for both
/// (01M6XZX237NRXSR4FN0XBP4MCA).
///
/// ```
/// assert_eq!(
///     riff::tui::chat::too_long(),
///     "The line is too long: a post has at most 64 KiB. The line stays as it was."
/// );
/// ```
pub fn too_long() -> String {
    format!(
        "The line is too long: a post has at most {} KiB. The line stays as it was.",
        POST_MAX / 1024
    )
}

/// The note for the line `text` in the prompt: what `Enter` does with a
/// command, with true for a command that the chat will not send. `None`
/// for each other line: the note never says whom a line wakes
/// (01M6Y1A5PTCVZ6GRJ0FXMART2T).
///
/// ```
/// use riff::tui::chat::prompt_note;
///
/// assert_eq!(
///     prompt_note("/quit"),
///     Some(("Enter ends the TUI. The sessions go on.".to_owned(), false))
/// );
/// assert!(prompt_note("/foo").unwrap().1);
/// for line in ["hi", "@riff is #12 done?", "@lead:strata hi", "/me waves", "  "] {
///     assert_eq!(prompt_note(line), None, "{line}");
/// }
/// ```
pub fn prompt_note(text: &str) -> Option<(String, bool)> {
    match typed(text) {
        Typed::Quit => Some(("Enter ends the TUI. The sessions go on.".to_owned(), false)),
        Typed::Unknown(name) => Some((unknown(name), true)),
        Typed::Nothing | Typed::Say(_) | Typed::Action(_) => None,
    }
}

/// The note of the Chat tab, with its style: a post that failed, or a
/// paste or a key that made the line too long, after the last key, else
/// the keys of the list of the names while it shows, else the note of a
/// command in the prompt ([`prompt_note`], 01M6VMACJGF4A0ZXC6YK13KNCH,
/// 01M6Y1ES82XHFGANKZS7AF3CP2).
pub(super) fn note(state: &State) -> Option<Row<'static>> {
    let chat = &state.chat;
    if let Some(warning) = &chat.warning {
        return Some(Row::styled(safe(warning), WARNING));
    }
    if let Some(list) = chat.list() {
        return Some(Row::raw(list.note()));
    }
    let (note, warning) = prompt_note(chat.prompt.text())?;
    Some(if warning {
        Row::styled(note, WARNING)
    } else {
        Row::raw(note)
    })
}

/// The keys of the key line of the Chat tab, with the key that a click
/// on each presses (01M6VAZKG8FS4HEA9VSA4Z4YF1). While the list of the
/// names shows, the keys of the list come first.
pub(super) fn keys(
    state: &Chat,
    layout: Layout,
) -> Vec<(&'static str, &'static str, Option<FrameKey>)> {
    let wide = layout == Layout::Wide;
    let chat = |key| Some(FrameKey::Chat(key));
    let mut keys = Vec::new();
    match state.focus {
        Focus::Prompt if state.list().is_some() => {
            keys.push(("Tab", "pick", chat(Key::Pick)));
            keys.push(("Esc", "close", chat(Key::Close)));
            keys.push(("Ctrl-C", "quit", Some(FrameKey::Quit)));
        }
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
pub const HELP: [(&str, &str); 11] = [
    ("Enter", "send the line, or go back to the prompt"),
    ("Esc", "leave the prompt: the keys of the tab work"),
    ("i", "go back to the prompt"),
    ("↑ ↓", "a row, a sent line on an empty line, or scroll"),
    (
        "Home, End",
        "the start or the end of the line; End again: the newest line",
    ),
    ("PgUp, PgDn", "older or newer lines"),
    ("g, G", "the oldest or the newest line, in the log"),
    ("Alt-1 to 6", "open a tab, also from the prompt"),
    ("/me TEXT", "send an action"),
    ("@", "a list of the names: Tab or Enter picks, Esc closes"),
    ("a paste", "goes into the line, with its new lines"),
];

/// The mark before the first row of the line.
const MARK: &str = "> ";

/// The start of each more row of the line: under the text of the first
/// row.
const UNDER: &str = "  ";

/// The most rows of the line in the prompt (01M6XXDDYQG7M6WYNRXP35DZ1Y).
/// A third of the rows of the tab is the most in a short terminal.
pub const LINE_ROWS: usize = 5;

/// Places the body of the Chat tab in `area` on `view`: the summary
/// row, the log, the row of the new lines, the prompt, the places that a
/// click hits, the cursor and the window of the log and of the line. The
/// prompt grows up from the row over the note line, one row for each row
/// of the line, and the log has the rows over it. The log is no place
/// that a click hits: a click there keeps the focus in the prompt
/// (01M6XT6MCZ38XEN7ZHWTRF50Z5). A click on the row of the new lines
/// shows the newest line, as `End` does.
pub(super) fn place(state: &State, area: Rect, layout: Layout, view: &mut View) {
    let chat = &state.chat;
    let margin = u16::from(layout == Layout::Wide);
    let columns = area.width - margin;
    let text_row = |y: u16| Rect::new(area.x + margin, area.y + y, columns, 1);
    view.rows.push(Placed {
        area: text_row(2),
        line: summary(state, layout),
    });

    // The tab has the rows between the header and the note line.
    let most = LINE_ROWS.min(usize::from(area.height - 4) / 3).max(1);
    let line = area.width.saturating_sub(super::view::width(MARK)).max(1);
    let shown = prompt_rows(chat, usize::from(line), most);
    let high = u16::try_from(shown.rows.len()).unwrap_or(1);
    let prompt = area.height - 2 - high;

    let top = 3;
    let room = prompt - top;
    // The list of the names takes the last rows of the log. It leaves
    // the log the rows of its newest line, and one row at least: then it
    // shows fewer names, and the pick at least
    // (01M6Z8AHH9FW3A3132YF8XH7R9).
    let list = chat.list();
    let listed = list.as_ref().map(list_rows).unwrap_or_default();
    let newest = chat
        .entries
        .len()
        .checked_sub(1)
        .map_or(0, |last| rows_of(chat, last, columns, area.width).len());
    let keep = u16::try_from(newest)
        .unwrap_or(u16::MAX)
        .clamp(1, room.saturating_sub(1).max(1));
    let over = u16::try_from(listed.len()).map_or(0, |n| n.min(room.saturating_sub(keep)));
    let mut rows = room - over;
    // The row of the new lines takes the last row of the log
    // (01M6Z8AHEHCPR1A3Z3CEAN59AR).
    let mut new = below(chat, columns, rows);
    if new > 0 {
        rows = rows.saturating_sub(1);
        new = below(chat, columns, rows);
    }
    for (y, (line, rule)) in (top..).zip(window(chat, columns, area.width, rows)) {
        let place = if rule {
            Rect::new(area.x, area.y + y, area.width, 1)
        } else {
            text_row(y)
        };
        view.rows.push(Placed { area: place, line });
    }
    if new > 0 {
        let place = text_row(top + rows);
        view.rows.push(Placed {
            area: place,
            line: Row::styled(new_below(new), NEW),
        });
        view.hits.push(Hit {
            area: place,
            key: FrameKey::Chat(Key::Scroll(Scroll::Newest)),
        });
    }
    view.chat = Some(Window {
        columns,
        rows,
        line,
        line_top: shown.top,
    });
    // Under the text of the line, after the mark of the prompt.
    let indent = super::view::width(MARK).min(area.width);
    for (y, line) in (prompt - over..prompt).zip(listed) {
        let place = Rect::new(area.x + indent, area.y + y, area.width - indent, 1);
        view.rows.push(Placed { area: place, line });
    }

    for (y, line) in (prompt..).zip(shown.rows) {
        let place = Rect::new(area.x, area.y + y, area.width, 1);
        view.rows.push(Placed { area: place, line });
    }
    if chat.focus == Focus::Prompt {
        let (x, y) = shown.cursor;
        view.cursor = Some(Position::new(area.x + x, area.y + prompt + y));
    }
    view.hits.push(Hit {
        area: Rect::new(area.x, area.y + prompt, area.width, high),
        key: FrameKey::Chat(Key::Prompt),
    });
}

/// The rows of the list of the names, of one width: the first name, the
/// pick of `Tab` and `Enter`, in reverse video, and the count of the
/// other names dim.
fn list_rows(list: &List) -> Vec<Row<'static>> {
    let rows = list.rows();
    let width = rows
        .iter()
        .map(|row| textwrap::core::display_width(row))
        .max()
        .unwrap_or(0);
    rows.into_iter()
        .enumerate()
        .map(|(i, row)| {
            let pad = " ".repeat(width - textwrap::core::display_width(&row));
            let style = match i {
                0 => PICKED,
                i if i >= list.names.len() => DIM,
                _ => Style::new(),
            };
            Row::from(Span::styled(format!("{row}{pad}"), style))
        })
        .collect()
}

/// The end of the summary row in the wide layout: how a line wakes
/// (01M6Y1A5PTCVZ6GRJ0FXMART2T).
pub const WAKES: &str = "@NAME wakes a person or a project";

/// The summary row: the people who are online and away, the state of
/// the stream while it is down, and [`WAKES`]. The narrow layout counts
/// the people, and has no [`WAKES`].
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
        parts.push(Span::raw(WAKES));
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
        for row in rows_of(chat, i, columns, full).into_iter().rev() {
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

/// The new lines of `chat` under its window of `rows` rows, with
/// `columns` for a row: the lines that came while the log was not at
/// the bottom ([`Chat::add`]), and whose last row does not show. The
/// window is the one of [`window`].
fn below(chat: &Chat, columns: u16, rows: u16) -> usize {
    let rows = usize::from(rows);
    let want = chat.scroll.saturating_add(rows);
    // The rows under the last row of each entry, the newest first.
    let mut under = Vec::new();
    let mut got = 0;
    for i in (0..chat.entries.len()).rev() {
        if got >= want {
            break;
        }
        under.push(got);
        got += rows_of(chat, i, columns, columns).len();
    }
    let scroll = chat.scroll.min(got.saturating_sub(rows));
    under
        .into_iter()
        .take(chat.new)
        .filter(|&under| under < scroll)
        .count()
}

/// The row over the prompt while `n` new lines are under the window of
/// the log (01M6Z8AHEHCPR1A3Z3CEAN59AR).
///
/// ```
/// use riff::tui::chat::new_below;
///
/// assert_eq!(new_below(1), "1 new line below: End");
/// assert_eq!(new_below(3), "3 new lines below: End");
/// ```
pub fn new_below(n: usize) -> String {
    let lines = if n == 1 { "line" } else { "lines" };
    format!("{n} new {lines} below: End")
}

/// The rows of the entry `i` of the log of `chat`: a rule of `full`
/// columns first when its day is new, then the line in rows of
/// `columns`, with the names of the chat.
fn rows_of(chat: &Chat, i: usize, columns: u16, full: u16) -> Vec<(Row<'static>, bool)> {
    let width = usize::from(columns.max(1));
    let entries = &chat.entries;
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
                line_rows(line, &chat.names, at, width)
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

/// The rows of a chat line in `width` columns: the time, the sender by
/// its name in `names`, and the body, as `riff chat` shows them. Each
/// more row starts under the sender.
fn line_rows(
    line: &Line,
    names: &Names,
    at: &DateTime<FixedOffset>,
    width: usize,
) -> Vec<Row<'static>> {
    let said = crate::chat::said(line, names);
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

/// The prompt as it shows ([`prompt_rows`]).
struct PromptRows {
    /// The rows of the line that show.
    rows: Vec<Row<'static>>,
    /// The first row of the line that shows.
    top: usize,
    /// The column of the cursor, and its row in `rows`.
    cursor: (u16, u16),
}

/// The prompt: the line in rows of `columns` columns of text ([`wrap`]),
/// at most `most` of them. A longer line shows the rows around the row
/// of the cursor, from the first row of the last draw when it can. The
/// first row of the line starts with `> `, and each more row starts
/// under its text. A new line shows as `↵` and a tab as a space. While
/// the log has the focus, the prompt is dim.
fn prompt_rows(chat: &Chat, columns: usize, most: usize) -> PromptRows {
    let rows = Rows::of(chat.prompt.text(), columns);
    let cursor = chat.prompt.cursor().min(rows.shown.len());
    let at = rows.row(cursor);
    let len = rows.starts.len();
    let fit = len.min(most);
    let top = super::sessions::window(chat.window.line_top, at, fit, len);
    let style = match chat.focus {
        Focus::Prompt => Style::new(),
        Focus::Log => DIM,
    };
    let shown = (top..top + fit)
        .map(|row| {
            let head = if row == 0 { MARK } else { UNDER };
            let text: String = rows.shown[rows.range(row)].iter().collect();
            Row::styled(format!("{head}{text}"), style)
        })
        .collect();
    let x = MARK.len() + rows.column(cursor);
    PromptRows {
        rows: shown,
        top,
        cursor: (
            u16::try_from(x).unwrap_or(u16::MAX),
            u16::try_from(at - top).unwrap_or(0),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::view::{compute_view, render};
    use crate::tui::{Input, Look};
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::{KeyEvent, MouseButton, MouseEvent};
    use riff_core::build::Build;
    use riff_core::wire::{Kind, NameKind, RiffOwner};

    /// The time of the screen tests: 13:20 UTC on 2026-11-06, the day of
    /// the mock-up.
    const NOW: i64 = 1_793_971_200_000;

    fn build(id: &str) -> Build {
        format!("0.1.0 {id} 2026-11-06T12:00:00Z").parse().unwrap()
    }

    /// A line of the thread at `time` on 2026-11-06, in UTC.
    fn said(time: &str, from: &str, body: &str) -> Came {
        said_as(time, from, Kind::Message, body)
    }

    /// A line of the kind `kind` of the thread at `time` on 2026-11-06,
    /// in UTC.
    fn said_as(time: &str, from: &str, kind: Kind, body: &str) -> Came {
        let at = DateTime::parse_from_rfc3339(&format!("2026-11-06T{time}:00+00:00")).unwrap();
        let from: SessionUri = from.parse().unwrap();
        let line = Line {
            seq: 0,
            at_ms: u64::try_from(at.timestamp_millis()).unwrap(),
            lead: from.lead(),
            from,
            verified: true,
            to: vec![],
            kind,
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
            said_as(
                "12:40",
                "riff://riff@server",
                Kind::Riff,
                "members: ann passed the owner role to mike.",
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
        let mut state = State::new(build("10df8a4c0ffe"), "riff://mike@thelio".parse().unwrap());
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
        state.chat_since(0);
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
    /// prompt with the cursor after the text, no note of a wake, and
    /// the keys of the prompt. Each sender shows by the name to type to
    /// reach it (#1786): a person as `<mike>`, a lead as `<riff>`, and
    /// riff-server itself as `<riff-bot>`.
    #[test]
    fn the_chat_tab_at_80_x_24_is_the_mock_up() {
        let mut state = mock_up();
        type_in(&mut state, "@riff what is next?");
        let (rows, view) = drawn(&mut state, 80, 24);
        let mut want = vec![
            " 1 Sessions  [2 Chat]  3 Waves•  4 Machines  5 Devices  6 Reports".to_owned(),
            " riff running · owner mike · build 10df8a4 · link ok 2s".to_owned(),
            " chat · online: mike, brett · away: ann · @NAME wakes a person or a project"
                .to_owned(),
            format!("──── Friday 2026-11-06 {}", "─".repeat(57)),
        ];
        want.extend(
            [
                " 09:14 <riff> good morning: 2 pull requests merged overnight",
                " 09:15 <ann> morning",
                " 09:40 <kadomony> the plan of wave 7 is ready for ann",
                " 11:58 <riff> wave 48 runs: 5 items free, 3 workers started",
                " 12:01 <brett> mike, can you look at #1608?",
                " 12:02 <mike> @brett:strata yes, after lunch",
                " 12:02 <strata> noted: mike looks at #1608 after lunch",
                " 12:20 * mike goes for lunch",
                " 12:31 <riff> blocked: 3a3f8d5d (#994) asks: which of two designs?",
                " 12:40 <riff-bot> members: ann passed the owner role to mike.",
                " 12:46 <riff> #994 still has no answer; I wait for mike",
                " 13:05 <mike> back. @lead:riff the first design, with the tab for waves",
                " 13:05 <riff> ok: #994 goes on with the first design",
                " 13:09 <riff> #1608: the verify passed; the merge waits for CI",
                " 13:12 <riff> #1608 merged",
                "",
                "",
                "> @riff what is next?",
                "",
                " Enter send  Esc log  PgUp older  Alt-1..6 tab  Ctrl-C quit",
            ]
            .map(str::to_owned),
        );
        assert_eq!(rows, want);
        // The cursor is after the text.
        assert_eq!(
            view.cursor,
            Some(Position::new(
                u16::try_from("> @riff what is next?".len()).unwrap(),
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
            "12:02 <mike> @brett:strata yes, after",
            "      lunch",
            "12:02 <strata> noted: mike looks at",
            "      #1608 after lunch",
            "12:20 * mike goes for lunch",
            "12:31 <riff> blocked: 3a3f8d5d (#994)",
            "      asks: which of two designs?",
            "12:40 <riff-bot> members: ann passed the",
            "      owner role to mike.",
            "12:46 <riff> #994 still has no answer; I",
            "      wait for mike",
            "13:05 <mike> back. @lead:riff the first",
            "      design, with the tab for waves",
            "13:05 <riff> ok: #994 goes on with the",
            "      first design",
            "13:09 <riff> #1608: the verify passed;",
            "      the merge waits for CI",
            "13:12 <riff> #1608 merged",
            "> @brett hi",
            "",
            "Enter send  Esc log  Ctrl-C quit",
        ];
        assert_eq!(rows, want);
    }

    /// Only a line of riff-server itself shows as `<riff-bot>`
    /// (01M6Y4ZCTSTVM971BDC1F7GJPK). A line of mike with the mark
    /// `[by riff]` shows as `<mike>`, with the mark, and so does a line of
    /// mike with the kind `riff` in an old log.
    #[test]
    fn only_a_line_of_riff_server_shows_as_riff_bot() {
        let mut state = mock_up();
        let mike = "riff://mike@thelio";
        let waits = "[by riff] @mike the deploy waits for your approval";
        let passed = "members: ann passed the owner role to brett.";
        for came in [
            said("13:15", mike, waits),
            said_as("13:16", mike, Kind::Riff, "a line of an old log"),
            said_as("13:17", "riff://riff@server", Kind::Riff, passed),
        ] {
            state.chat_came(came);
        }
        let (rows, _) = drawn(&mut state, 80, 24);
        for want in [
            format!(" 13:15 <mike> {waits}"),
            " 13:16 <mike> a line of an old log".to_owned(),
            format!(" 13:17 <riff-bot> {passed}"),
        ] {
            assert!(rows.contains(&want), "{want}: {rows:#?}");
        }
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
        // The note line says nothing of whom the line wakes, also while
        // the log has the focus.
        assert_eq!(rows[22], "");
        let (narrow, _) = drawn(&mut state, 40, 24);
        assert_eq!(narrow[23], "i type  ↑↓ scroll  ? help  q quit");
        // A click on `i type` or on the prompt gives the focus back. The
        // log is no place that a click hits.
        assert_eq!(view.hit(2, 23), Some(FrameKey::Chat(Key::Prompt)));
        assert_eq!(view.hit(10, 21), Some(FrameKey::Chat(Key::Prompt)));
        assert_eq!(view.hit(10, 5), None);
        state.on_click(&view, 10, 21, false);
        assert_eq!(state.chat().focus(), Focus::Prompt);
        let (_, view) = drawn(&mut state, 80, 24);
        assert_eq!(view.hit(2, 23), Some(FrameKey::Chat(Key::Send)));
        assert_eq!(view.hit(14, 23), Some(FrameKey::Chat(Key::Log)));
    }

    /// A press of the key `code` with `modifiers`.
    fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(code, modifiers))
    }

    /// A click of the left button at `column` and `row`.
    fn click(column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    /// Gives the event `event` of the terminal to `state` as the loop of
    /// the TUI does, on the view `view` of the last draw.
    fn send(state: &mut State, view: &View, event: &Event) {
        match state.input(event) {
            Some(Input::Key(key)) => {
                state.on_key(key);
            }
            Some(Input::Click { column, row }) => {
                state.on_click(view, column, row, false);
            }
            Some(Input::Paste(text)) => {
                state.on_paste(&text);
            }
            Some(Input::Focused) => state.focused(),
            None => {}
        }
    }

    /// The events that a switch to another window and back can send to
    /// the TUI (#1762). An `Esc` takes the focus from the prompt, as the
    /// key does. A click on the log took it too: the click that gives
    /// the window the focus often lands there. Now the click keeps the
    /// prompt. `Alt-Tab` opens the next tab, as `Tab` does. `FocusLost`
    /// and `FocusGained` keep the prompt.
    #[test]
    fn of_the_events_of_a_switch_of_windows_esc_takes_the_focus_from_the_prompt() {
        let mut state = mock_up();
        let (_, view) = drawn(&mut state, 80, 24);
        for event in [Event::FocusLost, Event::FocusGained, click(10, 5)] {
            send(&mut state, &view, &event);
            assert_eq!(
                (state.tab(), state.chat().focus()),
                (Tab::Chat, Focus::Prompt),
                "{event:?}"
            );
        }
        send(&mut state, &view, &press(KeyCode::Tab, KeyModifiers::ALT));
        assert_eq!(state.tab(), Tab::Waves);
        state.on_key(FrameKey::Open(Tab::Chat));
        send(&mut state, &view, &press(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(state.chat().focus(), Focus::Log);
    }

    /// The line keeps the focus after a switch to another window and back
    /// (#1762, 01M6XT6MCZ38XEN7ZHWTRF50Z5). After the event of the cause,
    /// `FocusLost` and `FocusGained`, the keys go into the line, and the
    /// cursor shows at the end of the text. The cause is an `Esc` that
    /// the terminal sends, or a click on the log, before the loss of the
    /// focus or after its return.
    #[test]
    fn the_line_keeps_the_focus_after_a_switch_of_windows() {
        let (esc, on_log) = (press(KeyCode::Esc, KeyModifiers::NONE), click(10, 5));
        let (lost, gained) = (Event::FocusLost, Event::FocusGained);
        for events in [
            [esc, lost.clone(), gained.clone()],
            [on_log.clone(), lost.clone(), gained.clone()],
            [lost, gained, on_log],
        ] {
            let mut state = mock_up();
            type_in(&mut state, "half a li");
            let (_, view) = drawn(&mut state, 80, 24);
            for event in &events {
                send(&mut state, &view, event);
            }
            for c in "ne".chars() {
                send(
                    &mut state,
                    &view,
                    &press(KeyCode::Char(c), KeyModifiers::NONE),
                );
            }
            let (rows, view) = drawn(&mut state, 80, 24);
            assert_eq!(rows[21], "> half a line", "{events:?}");
            assert_eq!(view.cursor, Some(Position::new(13, 21)), "{events:?}");
            assert_eq!(
                rows[23], " Enter send  Esc log  PgUp older  Alt-1..6 tab  Ctrl-C quit",
                "{events:?}"
            );
        }
    }

    /// A post that fails while the prompt holds a line shows at once in
    /// the note line, as a warning. After the next key, it stays as the
    /// result of the last action, with no style, also with an empty
    /// prompt (01M6VMACJGF4A0ZXC6YK13KNCH).
    #[test]
    fn a_post_that_fails_while_the_prompt_holds_a_line_shows_until_the_next_key() {
        let mut state = mock_up();
        type_in(&mut state, "@riff what is next?");
        let failed = "riff: cannot send the line: cannot reach riff-server";
        state.chat_came(Came::Unsent(failed.into()));
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(rows[21], "> @riff what is next?");
        assert_eq!(rows[22], format!(" {failed}"));
        let note = &view.rows.iter().find(|r| r.area.y == 22).unwrap().line;
        assert_eq!(note.style, WARNING);
        // The next key ends the warning: the failure is the result of the
        // last action.
        for key in [Key::End, Key::Clear] {
            state.on_key(FrameKey::Chat(key));
            let (rows, view) = drawn(&mut state, 80, 24);
            assert_eq!(rows[22], format!(" {failed}"), "{key:?}");
            let note = &view.rows.iter().find(|r| r.area.y == 22).unwrap().line;
            assert_ne!(note.style, WARNING, "{key:?}");
        }
    }

    /// The cause of #1763: Enter on a line that the chat did not send put
    /// its reason in the note of the last action too, and that note
    /// stayed under each empty prompt and in each tab until the next line
    /// that riff sent. Now Enter on a command that the chat does not know
    /// gives no note of its own: the note of the prompt says why, and it
    /// goes with the line (01M6XTQHMY4VS04YGWY17BSTM8).
    #[test]
    fn the_note_of_an_unknown_command_goes_with_the_line() {
        let mut state = mock_up();
        type_in(&mut state, "/foo");
        assert_eq!(state.on_key(FrameKey::Chat(Key::Send)), None);
        assert_eq!(state.chat().prompt().text(), "/foo");
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(rows[22], format!(" {}", unknown("foo")));
        let note = &view.rows.iter().find(|r| r.area.y == 22).unwrap().line;
        assert_eq!(note.style, WARNING);
        assert_eq!(state.shown_note(), None);

        // With an empty prompt, the row under it is empty.
        state.on_key(FrameKey::Chat(Key::Clear));
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[21], ">");
        assert_eq!(rows[22], "");
        // Another tab has no old note either.
        state.on_key(FrameKey::Open(Tab::Sessions));
        let (rows, _) = drawn(&mut state, 80, 24);
        assert!(
            !rows.iter().any(|row| row.contains("Unknown command")),
            "{rows:#?}"
        );
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
        // 28 rows of the mock-up and 20 more: 48 rows, and 18 show.
        let (rows, _) = drawn(&mut state, 40, 24);
        assert_eq!(rows[20], "14:19 <brett> line 19");
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
        assert_eq!(state.chat().scrolled(), 30);
        assert_eq!(top[3], "──── Friday 2026-11-06 ─────────────────");
        assert_eq!(top[4], "09:14 <riff> good morning: 2 pull");
        // A new line: the window stays on its rows, and the row of the
        // new lines takes its last row.
        state.chat_came(said("14:30", "riff://brett@strata", "a new line"));
        let (kept, _) = drawn(&mut state, 40, 24);
        assert_eq!(kept[3..20], top[4..21]);
        assert_eq!(kept[20], "1 new line below: End");
        // PgDn shows newer rows, a page of 16: the window has 17 rows
        // now. End shows the newest line.
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::PageDown)));
        assert_eq!(state.chat().scrolled(), 15);
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::Newest)));
        let (rows, _) = drawn(&mut state, 40, 24);
        assert_eq!(rows[20], "14:30 <brett> a new line");
        // A send shows the newest line too.
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::Up(3))));
        assert_eq!(state.chat().scrolled(), 3);
        type_in(&mut state, "ok");
        state.on_key(FrameKey::Chat(Key::Send));
        assert_eq!(state.chat().scrolled(), 0);
    }

    /// The mock-up and 20 more lines of brett: 36 rows at 80 x 24, and
    /// 18 show.
    fn long_log() -> State {
        let mut state = mock_up();
        for n in 0..20 {
            state.chat_came(said(
                &format!("14:{n:02}"),
                "riff://brett@strata",
                &format!("line {n}"),
            ));
        }
        state
    }

    /// While the log is scrolled up, a new line stays under the window,
    /// and the row over the prompt says so (01M6Z8AHEHCPR1A3Z3CEAN59AR).
    /// `End` in the prompt, at the end of the line, shows the line, and
    /// the row goes.
    #[test]
    fn scrolled_up_a_new_line_gives_the_row_of_the_new_lines_and_end_shows_it() {
        let mut state = long_log();
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[20], " 14:19 <brett> line 19");
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::PageUp)));
        let (up, _) = drawn(&mut state, 80, 24);
        assert_eq!(up[20], rows[3]);
        // No new line yet: no row of the new lines.
        assert!(!up.iter().any(|row| row.contains("below")), "{up:#?}");

        state.chat_came(said("14:30", MIKE_LEAD, "#1833 merged"));
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(rows[3..20], up[4..21]);
        assert_eq!(rows[20], " 1 new line below: End");
        assert_eq!(rows[21], ">");
        assert!(!rows.contains(&" 14:30 <riff> #1833 merged".to_owned()));
        let row = view.rows.iter().find(|r| r.area.y == 20).unwrap();
        assert_eq!(row.line.style, NEW);

        // End shows the line, and the row goes.
        state.on_key(FrameKey::Chat(Key::End));
        assert_eq!(state.chat().scrolled(), 0);
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(
            rows[19..22],
            [" 14:19 <brett> line 19", " 14:30 <riff> #1833 merged", ">"]
        );
        assert!(!rows.iter().any(|row| row.contains("below")), "{rows:#?}");
    }

    /// The row counts the new lines that no scroll showed yet: a scroll
    /// that shows one counts it no more, also after a scroll up again.
    /// In a typed line, `End` goes to the end of the line first. A click
    /// on the row shows the newest line, and so does a send
    /// (01M6Z8AHEHCPR1A3Z3CEAN59AR).
    #[test]
    fn the_row_of_the_new_lines_counts_the_lines_that_no_scroll_showed() {
        let mut state = long_log();
        drawn(&mut state, 80, 24);
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::PageUp)));
        drawn(&mut state, 80, 24);
        for n in 0..3 {
            state.chat_came(said(&format!("14:3{n}"), MIKE_LEAD, &format!("new {n}")));
        }
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[20], " 3 new lines below: End");
        // Two rows down: the line `new 0` shows.
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::PageDown)));
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::Down(2))));
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(state.chat().scrolled(), 2);
        assert_eq!(
            rows[19..21],
            [" 14:30 <riff> new 0", " 2 new lines below: End"]
        );
        // Up again: `new 0` is no new line now.
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::Up(5))));
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[20], " 2 new lines below: End");

        // In a typed line, the first End goes to the end of the line.
        type_in(&mut state, "ok");
        state.on_key(FrameKey::Chat(Key::Home));
        state.on_key(FrameKey::Chat(Key::End));
        assert_eq!(state.chat().prompt().cursor(), 2);
        assert_eq!(state.chat().scrolled(), 7);

        // A click on the row shows the newest line.
        let (_, view) = drawn(&mut state, 80, 24);
        assert_eq!(
            view.hit(10, 20),
            Some(FrameKey::Chat(Key::Scroll(Scroll::Newest)))
        );
        state.on_click(&view, 10, 20, false);
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(rows[20], " 14:32 <riff> new 2");
        assert_eq!(view.hit(10, 20), None);

        // A send shows the newest line, and no row of the new lines.
        state.on_key(FrameKey::Chat(Key::Scroll(Scroll::PageUp)));
        drawn(&mut state, 80, 24);
        state.chat_came(said("14:40", MIKE_LEAD, "new 3"));
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[20], " 1 new line below: End");
        state.on_key(FrameKey::Chat(Key::Send));
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[20], " 14:40 <riff> new 3");
    }

    /// Seven names of people, `ann0` to `ann6`: the list of `@` has five
    /// of them, and a row of the two more.
    fn seven_names() -> Came {
        Came::Names(
            (0..7)
                .map(|n| ChatName {
                    name: format!("ann{n}"),
                    kind: NameKind::Person,
                    online: true,
                })
                .collect(),
        )
    }

    /// A new line of 4 rows comes while the prompt has 5 rows and the
    /// list of the names shows: the whole line is in sight, and the list
    /// shows fewer names. In a short tab, the list shows only the pick
    /// (01M6Z8AHH9FW3A3132YF8XH7R9).
    #[test]
    fn a_new_line_stays_in_sight_while_the_prompt_has_5_rows_and_the_list_shows() {
        let mut state = mock_up();
        state.chat_came(seven_names());
        type_in(
            &mut state,
            &format!("{}@", "the plan of wave 53 ".repeat(20)),
        );
        // The list has 6 rows over the prompt of 5 rows.
        let (before, _) = drawn(&mut state, 80, 19);
        assert_eq!(before[5], " 13:12 <riff> #1608 merged");
        assert_eq!(before[6], "  ann0  person   online");
        assert_eq!(before[11], "  and 2 more: type more letters");
        assert_eq!(before[16], "  the plan of wave 53 @");
        assert_eq!(before[18], " Tab pick  Esc close  Ctrl-C quit");

        let plan = "the plan of wave 54: ".repeat(12);
        state.chat_came(said("13:20", MIKE_LEAD, &format!("{plan}end of the plan")));
        let (rows, view) = drawn(&mut state, 80, 19);
        assert!(
            rows[3].starts_with(" 13:20 <riff> the plan of wave 54:"),
            "{rows:#?}"
        );
        assert!(rows[6].ends_with("end of the plan"), "{rows:#?}");
        assert_eq!(rows[7], "  ann0  person   online");
        assert_eq!(rows[11], "  ann4  person   online");
        assert_eq!(rows[12..17], before[12..17]);
        let pick = view.rows.iter().find(|r| r.area.y == 7).unwrap();
        assert_eq!(pick.line.spans[0].style, PICKED);

        // In a tab of 12 rows, the prompt has 2 rows, and the list keeps
        // the pick only.
        let (rows, _) = drawn(&mut state, 80, 12);
        assert!(
            rows[3].starts_with(" 13:20 <riff> the plan of wave 54:"),
            "{rows:#?}"
        );
        assert!(rows[6].ends_with("end of the plan"), "{rows:#?}");
        assert_eq!(rows[7], "  ann0  person   online");
        assert_eq!(rows[9], "  the plan of wave 53 @");
        assert_eq!(rows[10], " Tab or Enter picks ann0. Esc closes the list.");
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
            [rule, " 08:00 <ann> two".into(), "       rows".into()]
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
            " chat · (reconnecting…) · online: mike, brett · away: ann · @NAME wakes a person or a project"
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

    /// A bracketed paste of three lines puts the three lines in the
    /// prompt, at the cursor: the prompt shows each new line as `↵` at
    /// the end of a row, and the next line on a new row
    /// (01M6XXDDYQG7M6WYNRXP35DZ1Y). Enter posts them as one message with
    /// the new lines, and the message shows each line under the sender.
    #[test]
    fn a_paste_of_three_lines_is_one_message_with_its_new_lines() {
        let mut state = mock_up();
        let paste = Event::Paste("one\r\ntwo\rthree".into());
        let Some(super::super::Input::Paste(text)) = state.input(&paste) else {
            panic!("a paste is no input");
        };
        assert_eq!(state.on_paste(&text), None);
        assert_eq!(state.chat().prompt().text(), "one\ntwo\nthree");
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(rows[19..22], ["> one↵", "  two↵", "  three"]);
        assert_eq!(rows[22], "");
        assert_eq!(view.cursor, Some(Position::new(7, 21)));
        // A paste goes in at the cursor.
        state.on_key(FrameKey::Chat(Key::Home));
        state.on_paste("zero\n");
        assert_eq!(state.chat().prompt().text(), "zero\none\ntwo\nthree");
        assert_eq!(state.chat().prompt().cursor(), 5);
        // Enter posts the lines as one message.
        assert_eq!(
            state.on_key(FrameKey::Chat(Key::Send)),
            Some(Action::Post("zero\none\ntwo\nthree".into()))
        );
        assert_eq!(state.chat().prompt().text(), "");
        state.chat_came(said("13:15", "riff://mike@thelio", "zero\none\ntwo\nthree"));
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(
            rows[17..21],
            [
                " 13:15 <mike> zero",
                "       one",
                "       two",
                "       three"
            ]
        );
    }

    /// A paste that makes the line longer than [`POST_MAX`] stays out:
    /// the line keeps its text, the note line says the limit at once,
    /// and Enter posts nothing. The next key clears the note line.
    #[test]
    fn a_paste_over_the_limit_gives_the_note_and_posts_nothing() {
        let mut state = mock_up();
        state.on_paste(&"x".repeat(POST_MAX + 1));
        assert_eq!(state.chat().prompt().text(), "");
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(rows[21], ">");
        assert_eq!(rows[22], format!(" {}", too_long()));
        let note = &view.rows.iter().find(|r| r.area.y == 22).unwrap().line;
        assert_eq!(note.style, WARNING);
        assert_eq!(state.on_key(FrameKey::Chat(Key::Send)), None);
        // With a line in the prompt, the note shows too, and the line
        // stays as it was.
        type_in(&mut state, "hi");
        state.on_paste(&"x".repeat(POST_MAX - 1));
        assert_eq!(state.chat().prompt().text(), "hi");
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[22], format!(" {}", too_long()));
        state.on_key(FrameKey::Chat(Key::End));
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[22], "");
        // A paste up to the limit goes in.
        state.on_paste(&"x".repeat(POST_MAX - 2));
        assert_eq!(state.chat().prompt().text().len(), POST_MAX);
        assert_eq!(state.chat().warning(), None);
    }

    /// A typed character that makes the line longer than [`POST_MAX`]
    /// stays out, as a paste does: the line keeps its text, and the note
    /// line says the limit until the next key. A Backspace then works.
    #[test]
    fn a_key_over_the_limit_gives_the_note_and_types_nothing() {
        let mut state = mock_up();
        let full = "x".repeat(POST_MAX);
        state.on_paste(&full);
        state.on_key(FrameKey::Chat(Key::Type('y')));
        assert_eq!(state.chat().prompt().text(), full);
        assert_eq!(state.chat().prompt().cursor(), POST_MAX);
        assert_eq!(state.chat().warning(), Some(too_long().as_str()));
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(rows[22], format!(" {}", too_long()));
        // The note names the line, not a paste, also for a key.
        assert!(rows[22].starts_with(" The line is too long:"), "{rows:#?}");
        let note = &view.rows.iter().find(|r| r.area.y == 22).unwrap().line;
        assert_eq!(note.style, WARNING);
        // Also at the start of the line, the key types nothing.
        state.on_key(FrameKey::Chat(Key::Home));
        state.on_key(FrameKey::Chat(Key::Type('y')));
        assert_eq!(state.chat().prompt().text(), full);
        assert_eq!(state.chat().warning(), Some(too_long().as_str()));
        // A Backspace at the end deletes a character, and the note goes.
        state.on_key(FrameKey::Chat(Key::End));
        state.on_key(FrameKey::Chat(Key::Backspace));
        assert_eq!(state.chat().prompt().text().len(), POST_MAX - 1);
        assert_eq!(state.chat().warning(), None);
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[22], "");
        // The limit counts bytes: a character of two bytes stays out,
        // and a character of one byte goes in.
        state.on_key(FrameKey::Chat(Key::Type('é')));
        assert_eq!(state.chat().prompt().text().len(), POST_MAX - 1);
        assert_eq!(state.chat().warning(), Some(too_long().as_str()));
        state.on_key(FrameKey::Chat(Key::Type('y')));
        assert_eq!(state.chat().prompt().text().len(), POST_MAX);
        assert!(state.chat().prompt().text().ends_with('y'));
        assert_eq!(state.chat().warning(), None);
    }

    /// A paste is no key: in the log, a paste with `q` in it ends no
    /// TUI, and the help popup takes no paste.
    #[test]
    fn a_paste_in_the_log_goes_to_the_prompt_and_the_help_takes_none() {
        let mut state = mock_up();
        state.on_key(FrameKey::Chat(Key::Log));
        assert_eq!(state.on_paste("q"), None);
        assert_eq!(state.chat().focus(), Focus::Prompt);
        assert_eq!(state.chat().prompt().text(), "q");
        state.on_key(FrameKey::Chat(Key::Log));
        state.on_key(FrameKey::Help);
        assert_eq!(state.on_paste("more"), None);
        assert_eq!(state.chat().prompt().text(), "q");
        assert_eq!(state.chat().focus(), Focus::Log);
    }

    /// The line of 200 characters of the tests of the wrap.
    const LONG: &str = "Mike asked for this: typing in the TUI chat does not wrap lines, \
                        they just scroll off the screen to the left. Now the line wraps at \
                        the width of the tab, and the prompt grows up to five rows high now.";

    /// The mock-up with 5 more lines: the log fills the window also at
    /// 80 x 24.
    fn full() -> State {
        let mut state = mock_up();
        for n in 0..5 {
            state.chat_came(said(
                &format!("13:2{n}"),
                "riff://brett@strata",
                &format!("line {n}"),
            ));
        }
        state
    }

    /// A line of 200 characters wraps after a space
    /// (01M6XXDDYQG7M6WYNRXP35DZ1Y): on 3 rows at 80 x 24, and on 5 rows
    /// at 40 x 24, the last 5 of its 6 rows. The cursor is at the end,
    /// the log has as many rows less, and a click on each row of the
    /// prompt hits the prompt. A short terminal has a third of the rows
    /// of the tab.
    #[test]
    fn a_line_of_200_characters_shows_on_3_rows_at_80_and_on_5_rows_at_40() {
        assert_eq!(LONG.chars().count(), 200);
        let mut state = full();
        let (before, _) = drawn(&mut state, 80, 24);
        type_in(&mut state, LONG);
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(
            rows[19..24],
            [
                "> Mike asked for this: typing in the TUI chat does not wrap lines, they just",
                "  scroll off the screen to the left. Now the line wraps at the width of the",
                "  tab, and the prompt grows up to five rows high now.",
                "",
                " Enter send  Esc log  PgUp older  Alt-1..6 tab  Ctrl-C quit",
            ]
        );
        let end = "  tab, and the prompt grows up to five rows high now.".len();
        assert_eq!(
            view.cursor,
            Some(Position::new(u16::try_from(end).unwrap(), 21))
        );
        assert_eq!(view.chat.map(|w| w.rows), Some(16));
        assert_eq!(rows[3..19], before[5..21]);
        for y in 19..22 {
            assert_eq!(view.hit(30, y), Some(FrameKey::Chat(Key::Prompt)), "{y}");
        }
        assert_eq!(view.hit(30, 18), None);

        let mut state = full();
        let (before, _) = drawn(&mut state, 40, 24);
        type_in(&mut state, LONG);
        let (rows, view) = drawn(&mut state, 40, 24);
        assert_eq!(
            rows[17..22],
            [
                "  TUI chat does not wrap lines, they",
                "  just scroll off the screen to the",
                "  left. Now the line wraps at the width",
                "  of the tab, and the prompt grows up",
                "  to five rows high now.",
            ]
        );
        let end = "  to five rows high now.".len();
        assert_eq!(
            view.cursor,
            Some(Position::new(u16::try_from(end).unwrap(), 21))
        );
        assert_eq!(view.chat.map(|w| w.rows), Some(14));
        assert_eq!(rows[3..17], before[7..21]);

        // 12 rows: the tab has 8, and the line a third of them.
        let (rows, view) = drawn(&mut state, 80, 12);
        assert_eq!(
            rows[8..10],
            [
                "  scroll off the screen to the left. Now the line wraps at the width of the",
                "  tab, and the prompt grows up to five rows high now.",
            ]
        );
        assert_eq!(view.chat.map(|w| w.rows), Some(5));
    }

    /// `↑`, `↓`, `Home` and `End` move the cursor through the rows of the
    /// line, and `←` and `→` go over the end of a row
    /// (01M6XXDDYQG7M6WYNRXP35DZ1Y). A line of more than 5 rows scrolls,
    /// so that the row of the cursor shows. `↑` on the first row goes to
    /// the start of the line, and `↓` on the last row to its end. Only on
    /// an empty line does `↑` show the line that you sent
    /// (01M6Y8N8W5RE5H6MW3TNDH5BSY).
    #[test]
    fn the_keys_move_through_the_rows_and_a_long_line_scrolls_to_the_cursor() {
        let mut state = mock_up();
        type_in(&mut state, "ok");
        assert_eq!(
            state.on_key(FrameKey::Chat(Key::Send)),
            Some(Action::Post("ok".into()))
        );
        type_in(&mut state, LONG);
        let (rows, view) = drawn(&mut state, 40, 24);
        assert_eq!(rows[21], "  to five rows high now.");
        assert_eq!(view.cursor, Some(Position::new(24, 21)));
        let mut press = |key| {
            state.on_key(FrameKey::Chat(key));
            let (rows, view) = drawn(&mut state, 40, 24);
            (rows, view.cursor.map(|c| (c.x, c.y)))
        };
        // Home goes to the start of the line, and the first row shows.
        let (rows, cursor) = press(Key::Home);
        assert_eq!(rows[17], "> Mike asked for this: typing in the");
        assert_eq!(cursor, Some((2, 17)));
        // ↓ moves one row, to the same column.
        for (key, x, y) in [(Key::Right, 3, 17), (Key::Down, 3, 18), (Key::Down, 3, 19)] {
            assert_eq!(press(key).1, Some((x, y)), "{key:?}");
        }
        // The row of the cursor shows: the rows scroll by one.
        press(Key::Down);
        press(Key::Down);
        let (rows, cursor) = press(Key::Down);
        assert_eq!(rows[17], "  TUI chat does not wrap lines, they");
        assert_eq!(rows[21], "  to five rows high now.");
        assert_eq!(cursor, Some((3, 21)));
        // ↑ in the window does not scroll.
        let (rows, cursor) = press(Key::Up);
        assert_eq!(rows[17], "  TUI chat does not wrap lines, they");
        assert_eq!(cursor, Some((3, 20)));
        // End goes to the end of the line, and ↑ to the same column, or
        // to the end of a shorter row.
        assert_eq!(press(Key::End).1, Some((24, 21)));
        assert_eq!(press(Key::Up).1, Some((24, 20)));
        // ← at the start of a row goes to the end of the row before it,
        // and → back.
        press(Key::Home);
        assert_eq!(press(Key::Down).1, Some((2, 18)));
        assert_eq!(press(Key::Left).1, Some((36, 17)));
        assert_eq!(press(Key::Right).1, Some((2, 18)));
        // ↑ on the first row goes to the start of the line, and ↓ on the
        // last row to its end: the text stays.
        press(Key::Up);
        press(Key::Right);
        let (rows, cursor) = press(Key::Up);
        assert_eq!(rows[17], "> Mike asked for this: typing in the");
        assert_eq!(cursor, Some((2, 17)));
        press(Key::End);
        let (rows, cursor) = press(Key::Down);
        assert_eq!(rows[21], "  to five rows high now.");
        assert_eq!(cursor, Some((24, 21)));
        // On an empty line, ↑ shows the line that you sent, and ↓ empties
        // the line again.
        press(Key::Clear);
        let (rows, cursor) = press(Key::Up);
        assert_eq!(rows[21], "> ok");
        assert_eq!(cursor, Some((4, 21)));
        let (rows, cursor) = press(Key::Down);
        assert_eq!(rows[21], ">");
        assert_eq!(cursor, Some((2, 21)));
    }

    /// `↑` never puts a sent line in place of a typed text
    /// (01M6Y8N8W5RE5H6MW3TNDH5BSY, #1788). A person moved with `↑` in a
    /// long line, typed, and sent: the line that went out was the line
    /// sent before it, with the new words after it. Now `↑` keeps the
    /// text, and only an empty line brings back a sent line.
    #[test]
    fn up_keeps_a_typed_text_and_brings_back_a_sent_line_only_on_an_empty_line() {
        let mut state = mock_up();
        type_in(&mut state, "the first line");
        assert_eq!(
            state.on_key(FrameKey::Chat(Key::Send)),
            Some(Action::Post("the first line".into()))
        );
        type_in(&mut state, LONG);
        drawn(&mut state, 40, 24);
        for _ in 0..8 {
            state.on_key(FrameKey::Chat(Key::Up));
        }
        assert_eq!(state.chat.prompt.text(), LONG);
        assert_eq!(state.chat.prompt.cursor(), 0);
        type_in(&mut state, "Note: ");
        assert_eq!(
            state.on_key(FrameKey::Chat(Key::Send)),
            Some(Action::Post(format!("Note: {LONG}")))
        );

        // On an empty line, ↑ brings back the last sent line, and more ↑
        // the lines before it. ↓ goes back, and then empties the line.
        let up = |state: &mut State| {
            state.on_key(FrameKey::Chat(Key::Up));
            state.chat.prompt.text().to_owned()
        };
        assert_eq!(up(&mut state), format!("Note: {LONG}"));
        // In a wide tab, the sent line has one row.
        drawn(&mut state, 400, 24);
        assert_eq!(up(&mut state), "the first line");
        state.on_key(FrameKey::Chat(Key::Down));
        assert_eq!(state.chat.prompt.text(), format!("Note: {LONG}"));
        state.on_key(FrameKey::Chat(Key::Down));
        assert_eq!(state.chat.prompt.text(), "");

        // A change to a sent line that came back makes it a typed text:
        // ↑ and ↓ keep it.
        up(&mut state);
        up(&mut state);
        type_in(&mut state, " again");
        assert_eq!(up(&mut state), "the first line again");
        state.on_key(FrameKey::Chat(Key::Down));
        assert_eq!(state.chat.prompt.text(), "the first line again");
        assert_eq!(state.chat.prompt.cursor(), "the first line again".len());
    }

    /// A wide character takes two columns: the buffer has a blank cell
    /// after it, and a row has room for 19 of them at 40 columns.
    #[test]
    fn a_wide_character_takes_two_columns_of_a_row() {
        let mut state = mock_up();
        type_in(&mut state, &"日".repeat(25));
        let (rows, view) = drawn(&mut state, 40, 24);
        assert_eq!(rows[20], format!("> {}", "日 ".repeat(19).trim_end()));
        assert_eq!(rows[21], format!("  {}", "日 ".repeat(6).trim_end()));
        assert_eq!(view.cursor, Some(Position::new(14, 21)));
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
            rows.contains(&" 13:15 <brett> hi all".to_owned()),
            "{rows:#?}"
        );
    }

    /// #1786: on a clash, the names of riff-server give a project the
    /// name `OWNER/REPO`, and the log shows its lead with that name.
    #[test]
    fn a_lead_of_a_project_with_a_clash_shows_as_owner_and_repo() {
        let mut state = mock_up();
        let project = |name: &str| ChatName {
            name: name.into(),
            kind: riff_core::wire::NameKind::Project,
            online: true,
        };
        let names = ["acme/strata", "como-technologies/strata", "riff"];
        state.chat_came(Came::Names(names.map(project).to_vec()));
        let (rows, _) = drawn(&mut state, 80, 24);
        let clash = " 12:02 <como-technologies/strata> noted: mike looks at #1608 after lunch";
        assert!(rows.contains(&clash.to_owned()), "{rows:#?}");
        assert!(
            rows.contains(&" 13:12 <riff> #1608 merged".to_owned()),
            "{rows:#?}"
        );
    }

    /// The names of the chat from riff-server: a project with a lead that
    /// runs, and two people.
    fn names() -> Came {
        let name = |name: &str, kind, online| ChatName {
            name: name.into(),
            kind,
            online,
        };
        Came::Names(vec![
            name("brett", NameKind::Person, true),
            name("riff", NameKind::Project, true),
            name("rita", NameKind::Person, false),
        ])
    }

    /// The keys of the prompt with no list.
    const PROMPT_KEYS: &str = " Enter send  Esc log  PgUp older  Alt-1..6 tab  Ctrl-C quit";

    /// `@r` shows the list of the names over the prompt, under the text
    /// of the line: `riff` first, in reverse video, and the note line
    /// names the keys (01M6Y1EKGKNW2MH970PDF6PJK1,
    /// 01M6Y1ES82XHFGANKZS7AF3CP2). `Tab` completes the name, and opens
    /// no other tab. A name that is not in the list shows no list, and
    /// still posts.
    #[test]
    fn at_r_shows_riff_and_tab_completes_it() {
        let mut state = mock_up();
        state.chat_came(names());
        type_in(&mut state, "ask @r");
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(
            rows[17..24],
            [
                " 13:09 <riff> #1608: the verify passed; the merge waits for CI",
                " 13:12 <riff> #1608 merged",
                "  riff  project  online",
                "  rita  person   offline",
                "> ask @r",
                " Tab or Enter picks riff. Esc closes the list.",
                " Tab pick  Esc close  Ctrl-C quit",
            ]
        );
        let row = |view: &View, y: u16| view.rows.iter().find(|r| r.area.y == y).cloned();
        assert_eq!(row(&view, 19).unwrap().line.spans[0].style, PICKED);
        assert_eq!(row(&view, 20).unwrap().line.spans[0].style, Style::new());
        assert_eq!(view.cursor, Some(Position::new(8, 21)));

        send(&mut state, &view, &press(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.tab(), Tab::Chat);
        assert_eq!(state.chat().prompt().text(), "ask @riff ");
        let (rows, view) = drawn(&mut state, 80, 24);
        assert_eq!(rows[19..22], ["", "", "> ask @riff"]);
        assert_eq!(rows[23], PROMPT_KEYS);
        assert_eq!(view.cursor, Some(Position::new(12, 21)));
        type_in(&mut state, "is #12 done?");
        let enter = press(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            state.input(&enter),
            Some(Input::Key(FrameKey::Chat(Key::Send)))
        );
        assert_eq!(
            state.on_key(FrameKey::Chat(Key::Send)),
            Some(Action::Post("ask @riff is #12 done?".into()))
        );

        // A name that is not in the list: no list, and Enter sends.
        type_in(&mut state, "@zed hi");
        assert_eq!(state.chat().list(), None);
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[19..22], ["", "", "> @zed hi"]);
        assert_eq!(
            state.on_key(FrameKey::Chat(Key::Send)),
            Some(Action::Post("@zed hi".into()))
        );
    }

    /// `Enter` picks a name too, and sends nothing. `Esc` closes the
    /// list, and the focus stays in the prompt. A key in the word keeps
    /// the list closed, and a second `Esc` moves the focus to the log
    /// (01M6Y1ES82XHFGANKZS7AF3CP2).
    #[test]
    fn enter_picks_a_name_and_esc_closes_the_list() {
        let mut state = mock_up();
        state.chat_came(names());
        let (_, view) = drawn(&mut state, 80, 24);
        let enter = press(KeyCode::Enter, KeyModifiers::NONE);
        let esc = press(KeyCode::Esc, KeyModifiers::NONE);
        type_in(&mut state, "@r");
        assert_eq!(
            state.input(&enter),
            Some(Input::Key(FrameKey::Chat(Key::Pick)))
        );
        send(&mut state, &view, &enter);
        assert_eq!(state.chat().prompt().text(), "@riff ");

        type_in(&mut state, "and @b");
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[20..22], ["  brett  person   online", "> @riff and @b"]);
        send(&mut state, &view, &esc);
        assert_eq!(state.chat().focus(), Focus::Prompt);
        assert_eq!(state.chat().list(), None);
        type_in(&mut state, "r");
        let (rows, _) = drawn(&mut state, 80, 24);
        assert_eq!(rows[20..22], ["", "> @riff and @br"]);
        assert_eq!(rows[23], PROMPT_KEYS);
        send(&mut state, &view, &esc);
        assert_eq!(state.chat().focus(), Focus::Log);
        // A new word with `@` opens a list again.
        state.on_key(FrameKey::Chat(Key::Prompt));
        type_in(&mut state, " @b");
        assert_eq!(state.chat().list().unwrap().first().name, "brett");
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
