//! `riff chat`: the people of a riff chat about the riff, in the style
//! of IRC (01M3NB5MY93KV9RKZGGSMZW00D).
//!
//! The chat is the thread [`THREAD`] on the riff server. A person posts
//! as `USER@HOST`, with no session. The server keeps its rules: only a
//! member reads or posts, and the sender comes from the sign-in.
//!
//! In a terminal, the chat has a prompt line at the bottom
//! (01M3NJD39JVJHY5G71CD79JBY3). A new line prints above the prompt, and
//! the text that the person types stays. After Enter, the typed line
//! goes away: the line shows once, as the chat line from the server.
//! With a pipe, the chat has no prompt and no line editor.
//!
//! A line shows its sender by the name to type to reach it ([`said`],
//! 01M6Y1FB5MZNZ971CJRBWZ2F9X): a person as `<mike>`, a verified lead as
//! the name of its project, `<riff>`, and riff-server itself as
//! `<riff-bot>` (01M6Y4ZCTSTVM971BDC1F7GJPK). The chat reads the names of
//! riff-server ([`Names`]) at its start, and again each [`READ_EVERY`]
//! (01M6Y1EKKCCZ5215V49Z5T3HKH): on a clash, a project has the name
//! `OWNER/REPO`. The log and the type-ahead share them. A session with no
//! name, for example a worker, shows as `[USER SESSION]`. The host shows
//! only in the detail of a line: in `riff tail` and `riff read`. A line
//! that riff writes for the person, for example the ask of a release,
//! comes from riff-server: riff tells it the fact
//! ([`riff_core::wire::BotPost`]), and the line shows as `<riff-bot>`.
//!
//! ```text
//! 14:02 <mike> is the release out?
//! 14:03 <riff> @mike not yet: #12 waits for CI.
//! 14:09 <riff-bot> @mike riff release v1.2.3 waits for your approval: ...
//! 14:12 <riff-bot> members: ann made mike an admin.
//! ```
//!
//! A line of riff itself prints above the prompt too, and the typed text
//! and the cursor stay (01M6QR3JHMC305AV29HPCA0X1Y). Code that runs in
//! the chat calls [`notice`] for it, and never writes to the terminal on
//! its own: the line editor owns the prompt line.
//!
//! After an update, the chat runs the new `riff` in place
//! (01M3NT6WXGCNKW3EQ7MBJDQTR4), between two lines. It gives the new
//! chat the last line that it showed in the hidden option [`AFTER`], so
//! the new chat shows no line again, and the text that the person typed
//! and did not send in the hidden option [`DRAFT`]
//! (01M6QR3JMCZ63ZYSRFBSWJ6WD7). In a terminal, the old chat puts the
//! terminal back in its normal mode first, and the new chat draws its
//! prompt again, with that text after it.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as the person
//!     participant O as the old chat
//!     participant N as the new chat
//!     P->>O: types "half a li"
//!     O->>O: a line of riff itself: prints above the prompt
//!     Note over O: a new riff is on disk
//!     O->>N: run in place: --after 41 --draft "half a li"
//!     N->>P: [riff] > half a li
//! ```
//!
//! `/me TEXT` posts an action line: a plain message with the body
//! `/me TEXT` (01M3NJD37CNQX580YC24S7K6ES), so an older riff reads it
//! too. It shows as `* NAME TEXT`.
//!
//! A line wakes only the names that it holds
//! (01M6Y1A5PTCVZ6GRJ0FXMART2T): `@brett` wakes the person brett, and
//! `@riff` the lead of the sender in the project riff. The chat posts
//! the line with one selector `name=NAME` for each name
//! ([`riff_client::chat::to`]), and checks no name: riff-server resolves
//! each name when the line is posted. A line with no name wakes no one.
//! `riff chat` and the Chat tab of the TUI share the code
//! ([`crate::tui::chat`]). A lead answers with a post to the chat thread.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as riff chat (mike)
//!     participant S as riff-server
//!     participant B as riff chat (brett)
//!     participant L as lead of mike in riff
//!     A->>S: post "@brett @riff is #12 done?" to chat, to name=brett, name=riff
//!     S-->>A: tail: the line
//!     S-->>B: tail: the line, and a wake of the terminals of brett
//!     S-->>L: wake
//!     L->>S: post the answer to chat
//!     S-->>A: tail: the answer
//!     S-->>B: tail: the answer
//! ```
//!
//! In a terminal, `@` at the start of a word shows the names of the chat
//! that fit the letters after it, on the rows under the prompt
//! ([`crate::typeahead`], 01M6Y1EKGKNW2MH970PDF6PJK1). `Tab` or `Enter`
//! picks the first name, and `Esc` closes the list. The list is the hint
//! of the line editor, and the three keys are bindings of it: with no
//! list, each key does its work of the line editor. The hint has no
//! text for the line, so `→` puts none of the list in the line
//! (01M6YDSREH39WWV6QHB0M7Z6P5).
//!
//! ```text
//! [riff] > @r
//!   riff  project  online
//!   rita  person   offline
//!   Tab or Enter picks riff. Esc closes the list.
//! ```
//!
//! In the same way, `#` at the start of a word shows the open issues that
//! fit the letters or digits after it, the newest first
//! (01M74CX2V9YAF0KFPJDRM4DA83, 01M74CX9H7WT1T4EN70AVD2TGZ). `Tab` or
//! `Enter` puts `#N` for an issue of the repository of the working
//! folder, and `OWNER/REPO#N` for an issue of another repository. In a
//! terminal, the chat reads the open issues at its start, and again each
//! [`crate::tui::forge::READ_EVERY`] (`read_issues`,
//! 01M74CX9KVGQG4Y7ATT6268NMY): of the repository of each live session,
//! and of the repository of the working folder, with the conditional
//! read of the Waves tab.
//!
//! ```text
//! [riff] > is #186
//!   #1862  riff  Projects A: riff-server keeps the projects of the riff
//!   #1860  riff  Pick an issue with #: the type-ahead of the open issues
//!   Tab or Enter picks #1862. Esc closes the list.
//! ```
//!
//! A line for the person at the terminal is a highlight, as in IRC
//! ([`crate::highlight`], 01M6XSHCZB0Y08XRJ3J16Q4RTF): a line that names
//! them, a direct line, a word of their list, or an ask of riff. The chat
//! shows it in bold, and puts the marks of `[chat]` after it: the bell,
//! the title and the toast. A key, or a new prompt, clears the number in
//! the title (01M6XSHMNNTRC66CMZ0DF2X5RA). A highlight in the history at
//! the start is only bold (01M6XSHMX4VJKQHAHP8H4XDSBQ).

use std::fmt::Write as _;
use std::io::{IsTerminal, Write as _};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Result;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use futures::StreamExt;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::Kind;
use rustyline::completion::Completer;
use rustyline::config::Config;
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hint, Hinter};
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{
    Cmd, CompletionType, ConditionalEventHandler, Event, EventContext, EventHandler,
    ExternalPrinter, KeyCode, KeyEvent, Modifiers, Movement, RepeatCount,
};
use tokio::sync::mpsc;

use nix::sys::termios::{SetArg, Termios, tcgetattr, tcsetattr};

use crate::binary::{Follow, with_last, with_place};
use crate::catch_up::{self, Seen, Start};
use crate::highlight::{Highlights, Lit, Out, Title};
use crate::style::{DIM, WARNING, styled};
use crate::text::{lost_messages, safe};
use crate::tui::forge;
use crate::typeahead::{Ahead, List, OpenIssues, READ_EVERY};
use riff_client::api::Api;
use riff_client::link::STREAM_RETRY;
use riff_client::sessions::{Sessions, Show};
use riff_client::threads::{ACTION, Line, Names, Sender, action_text};
use riff_client::waves::board_repos;

/// The name of the chat thread.
pub const THREAD: &str = riff_core::name::CHAT;

/// The line that ends `riff chat`.
pub const QUIT: &str = "/quit";

/// The prompt of the input line in a terminal.
pub const PROMPT: &str = "[riff] > ";

/// The hidden option that gives a new chat the last line that the old
/// chat showed, after an update.
pub const AFTER: &str = "--after";

/// The hidden option that gives a new chat the text that the person
/// typed and did not send, after an update (#1665).
pub const DRAFT: &str = "--draft";

/// The chat thread.
///
/// ```
/// assert_eq!(riff::chat::thread().to_string(), "chat");
/// ```
pub fn thread() -> ThreadName {
    ThreadName::chat()
}

/// What a person typed in the chat.
#[derive(Debug, PartialEq, Eq)]
pub enum Typed<'a> {
    /// An empty line, or `/me` with no text: send nothing.
    Nothing,
    /// [`QUIT`]: end the chat.
    Quit,
    /// A chat line to send.
    Say(&'a str),
    /// `/me TEXT`: an action line to send.
    Action(&'a str),
    /// A command that the chat does not know: send nothing.
    Unknown(&'a str),
}

/// What `line` asks for. A line that starts with `/` is a command. A
/// line that starts with `//` is a chat line that starts with `/`, as in
/// IRC.
///
/// ```
/// use riff::chat::{Typed, typed};
///
/// assert_eq!(typed("  hi all "), Typed::Say("hi all"));
/// assert_eq!(typed("/me waves"), Typed::Action("waves"));
/// assert_eq!(typed("/me @lead look"), Typed::Action("@lead look"));
/// assert_eq!(typed("/quit"), Typed::Quit);
/// assert_eq!(typed("/foo bar"), Typed::Unknown("foo"));
/// assert_eq!(typed("//foo"), Typed::Say("/foo"));
/// assert_eq!(typed("/me"), Typed::Nothing);
/// assert_eq!(typed(" "), Typed::Nothing);
/// ```
pub fn typed(line: &str) -> Typed<'_> {
    let line = line.trim();
    if line.is_empty() {
        return Typed::Nothing;
    }
    if line.starts_with("//") {
        return Typed::Say(&line[1..]);
    }
    let Some(command) = line.strip_prefix('/') else {
        return Typed::Say(line);
    };
    let (name, text) = command
        .split_once(char::is_whitespace)
        .unwrap_or((command, ""));
    match (name, text.trim()) {
        ("quit", _) => Typed::Quit,
        ("me", "") => Typed::Nothing,
        ("me", text) => Typed::Action(text),
        (name, _) => Typed::Unknown(name),
    }
}

/// The answer to a command that the chat does not know. It sends
/// nothing.
///
/// ```
/// assert_eq!(
///     riff::chat::unknown("foo"),
///     "riff: unknown command /foo. Commands: /me, /quit. \
///      To send a line that starts with /, start it with //."
/// );
/// ```
pub fn unknown(name: &str) -> String {
    format!(
        "riff: unknown command /{}. Commands: /me, {QUIT}. \
         To send a line that starts with /, start it with //.",
        safe(name)
    )
}

/// The lines that the person sent, for `↑` and `↓` in the line of
/// `riff chat` and of the Chat tab of the TUI
/// (01M6Y8N8W5RE5H6MW3TNDH5BSY). A sent line never takes the place of a
/// typed text: [`Sent::up`] brings one back only on an empty line, or on
/// a sent line that shows as it came back. A change to that line makes
/// it a typed text.
///
/// ```mermaid
/// stateDiagram-v2
///     [*] --> Empty
///     Empty --> Back: ↑, the last sent line
///     Back --> Back: ↑ the line before, ↓ the line after
///     Back --> Empty: ↓ after the last sent line
///     Back --> Typed: a change to the line
///     Empty --> Typed: a key types
///     Typed --> Typed: ↑ the start, ↓ the end of the line
///     Typed --> Empty: Enter, or each character deleted
/// ```
///
/// ```
/// use riff::chat::Sent;
///
/// let mut sent = Sent::default();
/// sent.push("one");
/// sent.push("two");
/// // With a typed text, ↑ and ↓ keep it.
/// assert_eq!(sent.up("my text"), None);
/// assert_eq!(sent.down("my text"), None);
/// // On an empty line, ↑ brings back the last sent line, and then the
/// // line before it. ↓ goes the other way, and after the last sent line
/// // empties the line.
/// assert_eq!(sent.up(""), Some("two"));
/// assert_eq!(sent.up("two"), Some("one"));
/// assert_eq!(sent.up("one"), None);
/// assert_eq!(sent.down("one"), Some("two"));
/// assert_eq!(sent.down("two"), Some(""));
/// // A sent line that came back and changed is a typed text.
/// assert_eq!(sent.up(""), Some("two"));
/// assert_eq!(sent.up("two and more"), None);
/// assert_eq!(sent.down("two and more"), None);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sent {
    /// The lines, the oldest first.
    lines: Vec<String>,
    /// The line that came back last, while it shows.
    back: Option<usize>,
}

impl Sent {
    /// Keeps `line` after a send. A blank line, or the same line as the
    /// last one, adds nothing.
    ///
    /// ```
    /// use riff::chat::Sent;
    ///
    /// let mut sent = Sent::default();
    /// for line in ["ok", "ok", " "] {
    ///     sent.push(line);
    /// }
    /// assert_eq!(sent.up(""), Some("ok"));
    /// assert_eq!(sent.up("ok"), None);
    /// ```
    pub fn push(&mut self, line: &str) {
        self.back = None;
        if !line.trim().is_empty() && self.lines.last().is_none_or(|last| last != line) {
            self.lines.push(line.to_owned());
        }
    }

    /// The line to show after `↑` on the first row of `text`: the last
    /// sent line on an empty line, or the line before a sent line that
    /// shows as it came back. `None` keeps `text`: the cursor goes to
    /// its start. It is also `None` on the oldest line.
    pub fn up(&mut self, text: &str) -> Option<&str> {
        let at = match self.shown(text) {
            Some(i) => i.checked_sub(1)?,
            None if text.is_empty() => self.lines.len().checked_sub(1)?,
            None => return None,
        };
        self.back = Some(at);
        Some(&self.lines[at])
    }

    /// The line to show after `↓` on the last row of `text`: the line
    /// after a sent line that shows as it came back, or an empty line
    /// after the last one. `None` keeps `text`: the cursor goes to its
    /// end.
    pub fn down(&mut self, text: &str) -> Option<&str> {
        let i = self.shown(text)?;
        self.back = (i + 1 < self.lines.len()).then_some(i + 1);
        Some(self.back.map_or("", |at| &self.lines[at]))
    }

    /// The sent line that `text` is, when it came back and did not
    /// change. A change ends the walk through the sent lines.
    fn shown(&mut self, text: &str) -> Option<usize> {
        self.back = self.back.filter(|&i| self.lines[i] == text);
        self.back
    }
}

/// One chat line ([`Line`]) for people, with the names `names` of
/// riff-server. A date line comes first when the day of `at` is not
/// `last_day`. Then the time, the sender and the body, as [`said`] gives
/// them. Each more line of the body is indented. The line has ANSI
/// styles: print it through `anstream`. It removes each escape sequence
/// from the body and the sender.
///
/// ```
/// use chrono::TimeZone;
/// use riff_client::api::Checked;
/// use riff_client::threads::{Line, Names};
/// use riff_core::wire::{Kind, Message};
///
/// let message = Message {
///     seq: 3,
///     from: "riff://mike@thelio".parse()?,
///     to: vec![],
///     body: "hi\x1b]0;owned\x07 all".into(),
///     at_ms: 0,
///     kind: Kind::Message,
///     sig: None,
///     payload: None,
/// };
/// let c = Checked { message, verified: true };
/// let at = chrono::Utc.with_ymd_and_hms(2026, 9, 28, 14, 13, 0).unwrap();
/// let chat = "chat".parse()?;
/// let names = Names::default();
/// let plain = |c: &Checked| {
///     let line = riff::chat::line(&Line::of(c, &chat), &names, &at, Some(at.date_naive()));
///     anstream::adapter::strip_str(&line).to_string()
/// };
/// assert_eq!(plain(&c), "14:13 <mike> hi all");
/// let first = riff::chat::line(&Line::of(&c, &chat), &names, &at, None);
/// assert!(anstream::adapter::strip_str(&first).to_string().starts_with("2026-09-28\n14:13"));
///
/// // A lead has the name of its project.
/// let lead = "riff://mike@thelio/o/r?session=4e54d4e5&lead=true".parse()?;
/// let answer = Message { from: lead, ..c.message.clone() };
/// assert_eq!(plain(&Checked { message: answer, verified: true }), "14:13 <r> hi all");
///
/// let waves = Message { body: "/me waves".into(), ..c.message.clone() };
/// assert_eq!(plain(&Checked { message: waves, verified: true }), "14:13 * mike waves");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn line<Tz: TimeZone>(
    m: &Line,
    names: &Names,
    at: &DateTime<Tz>,
    last_day: Option<NaiveDate>,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut out = String::new();
    if last_day != Some(at.date_naive()) {
        let _ = writeln!(out, "{}", styled(DIM, &at.format("%Y-%m-%d").to_string()));
    }
    let said = said(m, names);
    let name = styled(crate::style::session(&m.from), &said.name);
    let _ = write!(
        out,
        "{} {}{name}{} {}",
        styled(DIM, &at.format("%H:%M").to_string()),
        said.before,
        said.after,
        said.body
            .lines()
            .collect::<Vec<_>>()
            .join(&format!("\n{INDENT}"))
    );
    out
}

/// The indent of each more line of the body of a chat line: the width
/// of the time `HH:MM` and a space.
pub const INDENT: &str = "      ";

/// The sender and the body of a chat line, as `riff chat` and the Chat
/// tab of the TUI show them ([`said`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    /// The text before the name: `<` for a name, `[` for a session with
    /// no name, `* ` for an action.
    pub before: &'static str,
    /// The name of the sender ([`Line::sender`]), safe.
    pub name: String,
    /// The text after the name: `>` for a name, `]` for a session with
    /// no name.
    pub after: &'static str,
    /// The body, or the text of an action, safe, with four spaces for a
    /// tab.
    pub body: String,
}

/// The sender and the body of the chat line `m`, with the names `names`
/// of riff-server. The sender shows by the name to type to reach it
/// (01M6Y1FB5MZNZ971CJRBWZ2F9X): a person as `<USER>`, a verified lead
/// as `<PROJECT>`, and riff-server itself as `<riff-bot>`
/// (01M6Y4ZCTSTVM971BDC1F7GJPK). Each other session has no name, and
/// shows as `[USER ID]`. An action is `* NAME TEXT`. A line of a person
/// with the words of riff-bot shows that person, as each other line does.
/// It removes each escape sequence from the body and the sender.
///
/// ```
/// use riff_client::api::Checked;
/// use riff_client::threads::{Line, Names};
/// use riff_core::wire::{Kind, Message};
///
/// let message = Message {
///     seq: 3,
///     from: "riff://mike@thelio".parse()?,
///     to: vec![],
///     body: "/me waves\x1b[2J\tbye".into(),
///     at_ms: 0,
///     kind: Kind::Message,
///     sig: None,
///     payload: None,
/// };
/// let names = Names::default();
/// let said = |message: &Message| {
///     let checked = Checked { message: message.clone(), verified: true };
///     riff::chat::said(&Line::of(&checked, &"chat".parse().unwrap()), &names)
/// };
/// let waves = said(&message);
/// assert_eq!((waves.before, waves.name.as_str(), waves.after), ("* ", "mike", ""));
/// assert_eq!(waves.body, "waves    bye");
///
/// // A person who types the words of riff-bot shows as the person.
/// let typed = Message { body: "riff release v3.3.0 waits".into(), ..message.clone() };
/// let mike = said(&typed);
/// assert_eq!((mike.before, mike.name.as_str(), mike.after), ("<", "mike", ">"));
/// assert_eq!(mike.body, "riff release v3.3.0 waits");
///
/// // A line of riff-server itself shows as riff-bot.
/// let server = riff_core::name::SessionUri::server();
/// let passed = "members: ann made mike an admin.";
/// let line = Message { from: server, kind: Kind::Riff, body: passed.into(), ..message.clone() };
/// let bot = said(&line);
/// assert_eq!((bot.before, bot.name.as_str(), bot.after), ("<", "riff-bot", ">"));
/// assert_eq!(bot.body, passed);
///
/// // A worker has no name.
/// let worker = "riff://mike@thelio/o/r?session=0271342b-7454&claim=issue-6".parse()?;
/// let done = said(&Message { from: worker, body: "done".into(), ..message });
/// assert_eq!((done.before, done.name.as_str(), done.after), ("[", "mike 0271342b", "]"));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn said(m: &Line, names: &Names) -> Said {
    let Sender { name, named } = m.sender(names);
    let body = m.body.as_str();
    let (before, after, body) = match (action_text(body), named) {
        (Some(text), _) => ("* ", "", text),
        (None, true) => ("<", ">", body),
        (None, false) => ("[", "]", body),
    };
    Said {
        before,
        name: safe(&name),
        after,
        body: safe(body).replace('\t', "    "),
    }
}

/// Runs the chat of `me` until stdin ends, or the person types
/// [`QUIT`]. It shows the start line and the history of the chat, then
/// each new line, and posts each line that the person types.
///
/// It connects again at once when the stream ends, and shows nothing
/// for it (01M3NK7VHXB0PAR8VH8GQQA06K). After each connect, it reads the
/// thread after the last line that it showed, so it shows each line
/// that came while it was not connected, once, or a line with the
/// number of the lost lines (01M49Z4E8QB1QDXVCPE6MX5JX2,
/// 01M49Z4EB7T972BHEP6T92P574). See [`crate::catch_up`].
///
/// With `after`, the chat runs after an update: it shows no start line
/// and no line up to `after`, the last line that the old chat showed.
/// With `draft`, in a terminal, the prompt starts with that text, the
/// text that the person typed and did not send. On a new binary, it
/// runs it in place, between two lines (01M3NT6WXGCNKW3EQ7MBJDQTR4,
/// 01M6QR3JMCZ63ZYSRFBSWJ6WD7).
///
/// While `riff-server` starts again, it shows the line
/// [`riff_client::link::WAITING`] above the prompt, and keeps its screen
/// (01M3THEE5V3RFHF9QTA8MA8QDF).
///
/// It sends the typed lines from a task of their own, in their order
/// (01M4A804AWYEHPYZ966A6PXRPF). So the screen takes input while a post
/// waits for its budget. At the end, it waits for the posts of the lines
/// that the person typed.
///
/// It reads the table `[chat]` of the settings at its start, and marks
/// each highlight for `me` ([`crate::highlight`]). A line of the history
/// of the first connect is not new: it is only bold.
///
/// In a terminal, it reads the open issues for the list of `#`
/// (`read_issues`). `here` is the repository of the working folder,
/// `OWNER/REPO`, if any: the list has its issues, and a pick of one puts
/// `#N`.
pub async fn run(
    api: &Api,
    me: &SessionUri,
    here: Option<String>,
    after: Option<u64>,
    draft: Option<&str>,
) -> Result<()> {
    let config = crate::settings::chat_here()?;
    let thread = thread();
    api.join(me, &thread).await?;
    // The log and the type-ahead share the names of the last read. A
    // riff-server that gives no names: each project has the name after
    // the slash.
    let known = Arc::new(Mutex::new(Known {
        issues: OpenIssues::new(here),
        ..Known::default()
    }));
    read_names(api, me, &known).await;
    if after.is_none() {
        anstream::eprintln!(
            "riff: chat as {}@{}. Type a line and press Enter. /me TEXT sends an action. \
             @NAME wakes a person or a project, for example @brett or @riff; \
             a line with no name wakes no one. {QUIT} or Ctrl-C exits.",
            me.who().user(),
            me.place().host()
        );
    }
    let thread = &thread;
    // After an update, the day of the last line of the old chat: the
    // new chat shows no date line again for it.
    let mut day = Day::default();
    if let Some(seq) = after {
        let (read, _) = api
            .read_page(me, thread, true, Some(seq.saturating_sub(1)))
            .await?;
        if let Some(last) = read.first().filter(|c| c.message.seq == seq) {
            day.line(&Line::of(last, thread), &names_of(&known), false);
        }
    }
    // Connected first, then read: no line falls between the two
    // (see [`crate::catch_up`]).
    let start = after.map_or(Start::History, Start::After);
    let shown = Arc::new(Mutex::new(catch_up::Shown::new(start)));
    let first = catch_up::connect(api, me, thread, &shown).await?;
    // The connect read the history: its last line. After an update, each
    // line that the read gives is new.
    let history = match after {
        None => shown.lock().map_or(None, |s| s.last()),
        Some(_) => None,
    };
    let again = catch_up::follow_thread(api, me, thread, Arc::clone(&shown), STREAM_RETRY);
    let mut stream = Box::pin(first.chain(again));
    let highlights = Highlights::new(me, config, "riff chat", Out::stdout());
    let reads = {
        let (api, me, known) = (api.clone(), me.clone(), Arc::clone(&known));
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(READ_EVERY).await;
                read_names(&api, &me, &known).await;
            }
        })
    };
    let (screen, mut input) = Screen::start(draft, highlights.title(), Arc::clone(&known))?;
    // Only a line editor has a list.
    let issues = matches!(screen.lines, Lines::Editor { .. })
        .then(|| tokio::spawn(read_issues(api.clone(), me.clone(), Arc::clone(&known))));
    let _ = LINES.set(screen.lines.clone());
    let lines = screen.lines.clone();
    let (send, posts) = posts(api, me, thread, move |text| lines.warn(text));
    let mut link = crate::view::Reconnect::default();
    let follow = Follow::this();
    let update = follow.new_one();
    tokio::pin!(update);
    loop {
        tokio::select! {
            () = &mut update => {
                let draft = screen.draft();
                screen.leave();
                let args = with_place(std::env::args_os().skip(1), me.place());
                let last = shown.lock().map_or(None, |s| s.last()).or(after).unwrap_or(0);
                let args = with_last(args, AFTER, last.to_string());
                follow.run(with_last(args, DRAFT, draft));
                break;
            }
            typed_line = input.recv() => {
                let Some(typed_line) = typed_line else { break };
                let body = match typed(&typed_line) {
                    Typed::Nothing => continue,
                    Typed::Quit => break,
                    Typed::Unknown(name) => {
                        screen.warn(&unknown(name));
                        continue;
                    }
                    Typed::Say(text) => text.to_owned(),
                    Typed::Action(text) => format!("{ACTION}{text}"),
                };
                let _ = send.send(body);
            }
            item = stream.next() => {
                let Some(item) = item else { break };
                if let Some(line) = link.line(&item) {
                    screen.warn(&line);
                }
                match item {
                    Ok(Seen::Message(c)) => {
                        let line = Line::of(&c, thread);
                        let new = history.is_none_or(|last| line.seq > last);
                        let names = names_of(&known);
                        let lit = highlights.line(&line, &names, thread, new);
                        screen.print(&day.line(&line, &names, lit.bold), &lit);
                    }
                    Ok(Seen::Lost(n)) => screen.warn(&styled(WARNING, &lost_messages(n))),
                    Err(_) => {}
                }
            }
        }
    }
    reads.abort();
    if let Some(issues) = issues {
        issues.abort();
    }
    drop(send);
    let _ = posts.await;
    Ok(())
}

/// Reads the names of the chat into `known`. A read that fails keeps
/// the names of the last read (01M6Y1EKKCCZ5215V49Z5T3HKH).
async fn read_names(api: &Api, me: &SessionUri, known: &Mutex<Known>) {
    if let Ok(read) = api.names(me).await
        && let Ok(mut known) = known.lock()
    {
        known.names = Names::new(read);
    }
}

/// Reads the open issues for the list of `#` into `known`, at once and
/// each [`forge::READ_EVERY`] (01M74CX9KVGQG4Y7ATT6268NMY): of the
/// repository of each live session that `who` gives `me`, and of the
/// repository of the working folder. It makes the conditional read of
/// the Waves tab ([`forge::open_issues`]), so a read that changed nothing
/// costs no point. Its calls of `gh` count as the caller `chat`
/// (01M74E4D97PSHPVE4GC0EW0RMJ). A read of `gh` runs on a thread of its
/// own: the end of the chat waits for no read. A read that fails keeps
/// the issues of the last read of its repository, and says nothing. A
/// `who` that fails keeps the repositories of the last read.
async fn read_issues(api: Api, me: SessionUri, known: Arc<Mutex<Known>>) {
    let gh = crate::pr::Gh::here("chat").with_one_token();
    let kept = forge::Kept::default();
    let here = known
        .lock()
        .ok()
        .and_then(|k| k.issues.here().map(str::to_owned));
    let mut sessions = Vec::new();
    loop {
        if let Ok(read) = api.who(&me, false).await {
            sessions = read;
        }
        let live = Sessions::new(sessions.clone(), Vec::new());
        let repos: Vec<String> = board_repos(&live, &Show::default(), here.as_deref())
            .into_iter()
            .collect();
        let (gh, kept) = (gh.clone(), kept.clone());
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(forge::open_issues_each(&gh, &repos, &kept));
        });
        if let Ok(read) = rx.await
            && let Ok(mut known) = known.lock()
        {
            known.issues.read(read);
        }
        tokio::time::sleep(forge::READ_EVERY).await;
    }
}

/// The names of the chat of the last read in `known`, for a line of the
/// log.
fn names_of(known: &Mutex<Known>) -> Names {
    known.lock().map(|k| k.names.clone()).unwrap_or_default()
}

/// True when stdin and stdout are a terminal: then the chat has a line
/// editor.
fn terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// The task that posts each line of the chat that `me` sends, in
/// order (01M4A804AWYEHPYZ966A6PXRPF): `riff chat` and the Chat tab of
/// the TUI give it each body. Each post has the names of its line
/// ([`riff_client::chat::to`]), and no selector of its own: the chat
/// checks no name. A post that fails gives `fault` its error, as a safe
/// line for the person.
pub fn posts(
    api: &Api,
    me: &SessionUri,
    thread: &ThreadName,
    fault: impl Fn(&str) + Send + 'static,
) -> (mpsc::UnboundedSender<String>, tokio::task::JoinHandle<()>) {
    let (send, mut typed) = mpsc::unbounded_channel::<String>();
    let (api, me, thread) = (api.clone(), me.clone(), thread.clone());
    let task = tokio::spawn(async move {
        while let Some(body) = typed.recv().await {
            let to = riff_client::chat::to(&body);
            let sent = api.post(&me, Some(&thread), &to, &body, Kind::Message);
            if let Err(e) = sent.await {
                fault(&safe(&format!("riff: cannot send the line: {e:#}")));
            }
        }
    });
    (send, task)
}

/// The printer of the line editor: it prints above the prompt.
type Printer = Arc<Mutex<Box<dyn ExternalPrinter + Send>>>;

/// Where the chat of this process prints, once it has a screen.
static LINES: OnceLock<Lines> = OnceLock::new();

/// Prints a line of riff itself, for example the note of a new release
/// at the server. While `riff chat` has a prompt, the line prints above
/// the prompt and the typed text stays (#1665). Else it goes to stderr.
/// Code that runs in `riff chat` calls this, and not `eprintln!`: a
/// direct write breaks the prompt line.
///
/// ```
/// riff::chat::notice("riff: a new release is at the server.");
/// ```
pub fn notice(text: &str) {
    match LINES.get() {
        Some(lines) => lines.warn(text),
        None => anstream::eprintln!("{text}"),
    }
}

/// Where the chat prints its lines.
#[derive(Clone)]
enum Lines {
    /// A pipe or a file: no prompt and no line editor.
    Plain,
    /// A terminal: a line editor with the prompt [`PROMPT`].
    Editor { printer: Printer, color: bool },
}

impl Lines {
    /// Prints a chat line, and the marks of `lit` after it, as they are:
    /// the bell, the title and the toast of a highlight.
    fn print(&self, text: &str, lit: &Lit) {
        match self {
            Lines::Plain => {
                anstream::println!("{text}");
                lit.put();
            }
            Lines::Editor { printer, color } => {
                let text = if *color {
                    text.to_owned()
                } else {
                    anstream::adapter::strip_str(text).to_string()
                };
                if let Ok(mut printer) = printer.lock() {
                    let _ = printer.print(format!("{text}{}\n", lit.marks));
                }
            }
        }
    }

    /// Prints a line of riff itself: on stderr with a pipe, above the
    /// prompt in a terminal.
    fn warn(&self, text: &str) {
        match self {
            Lines::Plain => anstream::eprintln!("{text}"),
            Lines::Editor { .. } => self.print(text, &Lit::default()),
        }
    }
}

/// The names of the chat and the open issues from the last read, and
/// the type-ahead of the prompt. The log, the hinter and the keys of the
/// line editor share them.
#[derive(Debug, Default)]
struct Known {
    names: Names,
    issues: OpenIssues,
    ahead: Ahead,
}

impl Known {
    /// The list for `line` with the cursor at the byte `pos`.
    fn list(&self, line: &str, pos: usize) -> Option<List> {
        self.ahead.list(self.names.list(), &self.issues, line, pos)
    }
}

/// The start of each row of a list under the prompt.
const LISTED: &str = "  ";

/// The hint of a list: its rows and its note, on the rows under the
/// prompt. It is no text of the line: `→` at the end of the line puts
/// the completion of a hint in the line, and this hint has none.
#[derive(Debug, PartialEq)]
struct Listed(String);

impl Listed {
    /// The hint of `list` in a terminal of `columns` columns: each row
    /// is cut to the width ([`List::rows`]).
    fn of(list: &List, columns: usize) -> Listed {
        let mut rows = list.rows(columns.saturating_sub(LISTED.len()));
        rows.push(list.note());
        Listed(rows.iter().map(|row| format!("\n{LISTED}{row}")).collect())
    }
}

impl Hint for Listed {
    fn display(&self) -> &str {
        &self.0
    }

    fn completion(&self) -> Option<&str> {
        None
    }
}

/// A copy of the text that the person is typing. The line editor lives
/// on a thread of its own and has no call to read its line, but it asks
/// a hinter for a hint after each edit, and at each new prompt. This
/// hinter keeps the line, and gives the list of the word at the cursor
/// as the hint ([`crate::typeahead`]). It also clears the number of the
/// highlights in the title of the terminal: a key or a new prompt clears
/// it (01M6XSHMNNTRC66CMZ0DF2X5RA).
#[derive(Clone, Default)]
struct Typing {
    text: Arc<Mutex<String>>,
    title: Option<Title>,
    names: Arc<Mutex<Known>>,
}

impl Typing {
    /// The text now.
    fn text(&self) -> String {
        self.text
            .lock()
            .map(|text| text.clone())
            .unwrap_or_default()
    }

    /// Keeps `line` as the text now.
    fn set(&self, line: &str) {
        if let Ok(mut text) = self.text.lock() {
            line.clone_into(&mut text);
        }
    }
}

impl Hinter for Typing {
    type Hint = Listed;

    fn hint(&self, line: &str, pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Listed> {
        self.set(line);
        // The line editor calls this on its own thread, the thread that
        // writes the screen: the title goes out between two of its writes.
        if let Some(clear) = self.title.as_ref().and_then(Title::clear) {
            let mut out = std::io::stdout();
            let _ = out.write_all(clear.as_bytes());
            let _ = out.flush();
        }
        let mut names = self.names.lock().ok()?;
        names.ahead.edited(line, pos);
        let columns = textwrap::termwidth();
        names.list(line, pos).map(|list| Listed::of(&list, columns))
    }
}

/// A key of the type-ahead in the line editor, while the list of the
/// names or of the issues shows (01M6Y1EKGKNW2MH970PDF6PJK1,
/// 01M74CX9H7WT1T4EN70AVD2TGZ): `Enter` picks the first entry, as `Tab`
/// does ([`Typing`] is the completer), and `Esc` closes the list. With
/// no list, the key does its work of the line editor: `Enter` sends the
/// line.
struct ListKey {
    names: Arc<Mutex<Known>>,
    /// True for `Enter`, false for `Esc`.
    pick: bool,
}

impl ConditionalEventHandler for ListKey {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        let mut names = self.names.lock().ok()?;
        let (line, pos) = (ctx.line(), ctx.pos());
        names.list(line, pos)?;
        if self.pick {
            Some(Cmd::Complete)
        } else {
            names.ahead.close(line, pos);
            Some(Cmd::Repaint)
        }
    }
}

/// The time that the line editor waits for the next byte after an
/// `Esc`: a lone `Esc` closes the list of the type-ahead, and the bytes
/// of a key with Alt come in one write.
const ESC_WAIT_MS: u16 = 50;

/// `Tab`, and `Enter` while the list shows: the one completion is the
/// first entry of the list, in the place of the `@` or `#` word. With no
/// list, there is no completion.
impl Completer for Typing {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        let list = self.names.lock().ok().and_then(|n| n.list(line, pos));
        Ok(match list {
            Some(list) => (list.at, vec![list.text(line)]),
            None => (pos, Vec::new()),
        })
    }
}

impl Highlighter for Typing {}

impl Validator for Typing {}

impl rustyline::Helper for Typing {}

/// `↑` (`up`) or `↓` of the line editor, with the lines that the person
/// sent ([`arrow`]).
struct Arrow {
    up: bool,
    sent: Arc<Mutex<Sent>>,
}

impl ConditionalEventHandler for Arrow {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        let mut sent = self.sent.lock().ok()?;
        Some(arrow(&mut sent, self.up, ctx.line(), ctx.pos()))
    }
}

/// The command of `↑` (`up`) or `↓` on `line`, with the cursor at the
/// byte `pos` (01M6Y8N8W5RE5H6MW3TNDH5BSY). It moves one line up or down
/// in a text with new lines. On the first or the last line, it shows a
/// line that the person sent ([`Sent`]), or it moves the cursor to the
/// start or the end of a typed text, and the text stays.
fn arrow(sent: &mut Sent, up: bool, line: &str, pos: usize) -> Cmd {
    let (before, after) = line.split_at_checked(pos).unwrap_or((line, ""));
    if up && before.contains('\n') {
        return Cmd::Move(Movement::LineUp(1));
    }
    if !up && after.contains('\n') {
        return Cmd::Move(Movement::LineDown(1));
    }
    let shown = if up { sent.up(line) } else { sent.down(line) };
    match shown {
        Some(text) => Cmd::Replace(Movement::WholeBuffer, Some(text.to_owned())),
        None if up => Cmd::Move(Movement::BeginningOfBuffer),
        None => Cmd::Move(Movement::EndOfBuffer),
    }
}

/// The screen of the chat.
struct Screen {
    lines: Lines,
    /// The mode of the terminal before the line editor.
    normal: Option<Termios>,
    /// The text that the person types now.
    typing: Typing,
}

impl Screen {
    /// Starts to read the typed lines: with a line editor when stdin and
    /// stdout are a terminal, else line by line. The lines come on the
    /// receiver. It closes when stdin ends, on Ctrl-C or Ctrl-D, and
    /// after [`QUIT`], so the terminal is back in its normal mode. In a
    /// terminal, the prompt starts with `draft`, the text that the
    /// person typed in the chat before an update, a key clears the
    /// number of `title`, and `@` or `#` shows a list of `names`: the
    /// names or the open issues.
    fn start(
        draft: Option<&str>,
        title: Title,
        names: Arc<Mutex<Known>>,
    ) -> Result<(Screen, mpsc::Receiver<String>)> {
        let (tx, rx) = mpsc::channel(16);
        if !terminal() {
            // A thread, not a task: a read of stdin that waits does not
            // stop the exit.
            std::thread::spawn(move || {
                for line in std::io::stdin().lines() {
                    let Ok(line) = line else { break };
                    if tx.blocking_send(line).is_err() {
                        break;
                    }
                }
            });
            let screen = Screen {
                lines: Lines::Plain,
                normal: None,
                typing: Typing::default(),
            };
            return Ok((screen, rx));
        }
        let normal = tcgetattr(std::io::stdin()).ok();
        let typing = Typing {
            title: Some(title),
            names,
            ..Typing::default()
        };
        let config = Config::builder()
            .completion_type(CompletionType::List)
            .keyseq_timeout(Some(ESC_WAIT_MS))
            .build();
        let mut editor = rustyline::Editor::<Typing, DefaultHistory>::with_config(config)?;
        editor.set_helper(Some(typing.clone()));
        let sent = Arc::new(Mutex::new(Sent::default()));
        for (code, up) in [(KeyCode::Up, true), (KeyCode::Down, false)] {
            let sent = Arc::clone(&sent);
            let arrow = EventHandler::Conditional(Box::new(Arrow { up, sent }));
            editor.bind_sequence(KeyEvent(code, Modifiers::NONE), arrow);
        }
        for (code, pick) in [(KeyCode::Enter, true), (KeyCode::Esc, false)] {
            let key = ListKey {
                names: Arc::clone(&typing.names),
                pick,
            };
            editor.bind_sequence(
                KeyEvent(code, Modifiers::NONE),
                EventHandler::Conditional(Box::new(key)),
            );
        }
        let printer: Printer = Arc::new(Mutex::new(Box::new(editor.create_external_printer()?)));
        let held = Arc::clone(&printer);
        let mut draft = draft.unwrap_or_default().to_owned();
        let typed_now = typing.clone();
        std::thread::spawn(move || {
            loop {
                let read = if draft.is_empty() {
                    editor.readline(PROMPT)
                } else {
                    editor.readline_with_initial(PROMPT, (&std::mem::take(&mut draft), ""))
                };
                let Ok(line) = read else { break };
                typed_now.set("");
                {
                    // No chat line prints between the typed line and its
                    // clear.
                    let _held = held.lock();
                    clear_typed(&line);
                }
                let _ = editor.add_history_entry(line.as_str());
                if let Ok(mut sent) = sent.lock() {
                    sent.push(&line);
                }
                let quit = typed(&line) == Typed::Quit;
                if tx.blocking_send(line).is_err() || quit {
                    break;
                }
            }
        });
        let color =
            anstream::AutoStream::choice(&std::io::stdout()) != anstream::ColorChoice::Never;
        let screen = Screen {
            lines: Lines::Editor { printer, color },
            normal,
            typing,
        };
        Ok((screen, rx))
    }

    /// The text that the person typed and did not send. A NUL byte
    /// cannot go in an argument, so the text has none.
    fn draft(&self) -> String {
        self.typing.text().replace('\0', "")
    }

    /// Makes the terminal ready for another program: it clears the
    /// prompt line, and puts the terminal back in its normal mode. The
    /// line editor waits on, so this is only for the last moment of the
    /// chat.
    fn leave(&self) {
        let Lines::Editor { printer, .. } = &self.lines else {
            return;
        };
        // No chat line prints after the clear.
        let _held = printer.lock();
        let mut out = std::io::stdout();
        let _ = write!(out, "\r\x1b[K");
        let _ = out.flush();
        if let Some(normal) = &self.normal {
            let _ = tcsetattr(std::io::stdin(), SetArg::TCSANOW, normal);
        }
    }

    /// Prints a chat line, and the marks of its highlight.
    fn print(&self, text: &str, lit: &Lit) {
        self.lines.print(text, lit);
    }

    /// Prints a line of riff itself.
    fn warn(&self, text: &str) {
        self.lines.warn(text);
    }
}

/// Clears the prompt and the typed `line` after Enter: the line comes
/// back from the server as a chat line.
fn clear_typed(line: &str) {
    let columns = textwrap::termwidth().max(1);
    let width = textwrap::core::display_width(PROMPT) + textwrap::core::display_width(line);
    let rows = width.saturating_sub(1) / columns + 1;
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b[{rows}A\r\x1b[J");
    let _ = out.flush();
}

/// The day of the last line that the chat showed.
#[derive(Default)]
struct Day(Option<NaiveDate>);

impl Day {
    /// The chat line of `l` with the names `names`, and a date line when
    /// its day is new. With `bold`, the line is in bold, and the date line
    /// is not.
    fn line(&mut self, l: &Line, names: &Names, bold: bool) -> String {
        let at = i64::try_from(l.at_ms)
            .ok()
            .and_then(|ms| Local.timestamp_millis_opt(ms).single())
            .unwrap_or_else(Local::now);
        let mut shown = line(l, names, &at, self.0);
        if bold {
            let own = line(l, names, &at, Some(at.date_naive()));
            shown = crate::highlight::bold(&shown, &own);
        }
        self.0 = Some(at.date_naive());
        shown
    }
}

#[cfg(test)]
mod tests {
    use rustyline::history::{DefaultHistory, History};

    use super::*;

    #[test]
    fn typing_keeps_the_last_line_that_the_editor_hints_for() {
        let typing = Typing::default();
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history as &dyn History);
        assert_eq!(typing.text(), "");
        assert_eq!(typing.hint("half a li", 9, &ctx), None);
        assert_eq!(typing.text(), "half a li");
        assert_eq!(typing.hint("half", 4, &ctx), None);
        assert_eq!(typing.text(), "half");
        typing.set("");
        assert_eq!(typing.text(), "");
    }

    /// The hint of `@r` shows the list and its note, and has no text for
    /// the line: `→` puts none of it in the line
    /// (01M6YDSREH39WWV6QHB0M7Z6P5).
    #[test]
    fn the_hint_of_the_list_shows_the_names_and_has_no_text_for_the_line() {
        use riff_core::wire::{ChatName, NameKind};

        let typing = Typing::default();
        let riff = ChatName {
            name: "riff".into(),
            kind: NameKind::Project,
            online: true,
        };
        typing.names.lock().unwrap().names = Names::new(vec![riff]);
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history as &dyn History);
        let hint = typing.hint("hi @r", 5, &ctx).unwrap();
        assert_eq!(
            hint.display(),
            "\n  riff  project  online\n  Tab or Enter picks riff. Esc closes the list."
        );
        assert_eq!(hint.completion(), None);
        assert_eq!(typing.hint("hi @zed", 7, &ctx), None);
    }

    /// The hint of `#186` shows the open issues that fit, the newest
    /// first, each row cut to the width of the terminal, and the note
    /// (01M74CX2V9YAF0KFPJDRM4DA83, 01M74CX9H7WT1T4EN70AVD2TGZ). `Tab`
    /// puts `#1862 ` in the line.
    #[test]
    fn the_hint_of_hash_shows_the_open_issues_cut_to_the_width() {
        use crate::typeahead::OpenIssue;

        let issue = |number: u64, title: &str| OpenIssue {
            repo: "o/riff".into(),
            number,
            title: title.into(),
            opened: i64::try_from(number).ok(),
        };
        let mut known = Known {
            issues: OpenIssues::new(Some("o/riff".into())),
            ..Known::default()
        };
        known.issues.read(vec![(
            "o/riff".into(),
            Ok(vec![
                issue(1860, "Pick an issue"),
                issue(
                    1862,
                    "Projects A: riff-server keeps the projects of the riff",
                ),
            ]),
        )]);
        let list = known.list("is #186", 7).unwrap();
        assert_eq!(
            Listed::of(&list, 40).display(),
            "\n  #1862  riff  Projects A: riff-server …\
             \n  #1860  riff  Pick an issue\
             \n  Tab or Enter picks #1862. Esc closes the list."
        );
        let typing = Typing {
            names: Arc::new(Mutex::new(known)),
            ..Typing::default()
        };
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history as &dyn History);
        assert_eq!(
            typing.complete("is #186", 7, &ctx).unwrap(),
            (3, vec!["#1862 ".to_owned()])
        );
    }

    /// 01M6Y8N8W5RE5H6MW3TNDH5BSY: with a typed text, `↑` and `↓` keep
    /// it, and go to its start and its end. On an empty line, `↑` brings
    /// back the last sent line, and `↓` empties the line again.
    #[test]
    fn an_arrow_keeps_a_typed_text_and_brings_back_a_sent_line_on_an_empty_line() {
        let mut sent = Sent::default();
        sent.push("the first line");
        let start = Cmd::Move(Movement::BeginningOfBuffer);
        let end = Cmd::Move(Movement::EndOfBuffer);
        let show = |text: &str| Cmd::Replace(Movement::WholeBuffer, Some(text.into()));
        assert_eq!(arrow(&mut sent, true, "my words", 3), start);
        assert_eq!(arrow(&mut sent, false, "my words", 3), end);
        assert_eq!(arrow(&mut sent, true, "", 0), show("the first line"));
        assert_eq!(arrow(&mut sent, false, "the first line", 14), show(""));
        // No sent line before the first one: the cursor goes to the start.
        assert_eq!(arrow(&mut sent, true, "", 0), show("the first line"));
        assert_eq!(arrow(&mut sent, true, "the first line", 14), start);
        // A change makes it a typed text.
        assert_eq!(arrow(&mut sent, false, "the first line!", 15), end);
        assert_eq!(arrow(&mut sent, true, "the first line!", 15), start);
    }

    /// In a text with new lines, the arrows move one line, and only on
    /// the first or the last line do they show a sent line.
    #[test]
    fn an_arrow_moves_a_line_in_a_text_with_new_lines() {
        let mut sent = Sent::default();
        sent.push("one\ntwo");
        assert_eq!(
            arrow(&mut sent, true, "", 0),
            Cmd::Replace(Movement::WholeBuffer, Some("one\ntwo".into()))
        );
        let up = arrow(&mut sent, true, "one\ntwo", 7);
        assert_eq!(up, Cmd::Move(Movement::LineUp(1)));
        let down = arrow(&mut sent, false, "one\ntwo", 2);
        assert_eq!(down, Cmd::Move(Movement::LineDown(1)));
        assert_eq!(
            arrow(&mut sent, false, "one\ntwo", 7),
            Cmd::Replace(Movement::WholeBuffer, Some(String::new()))
        );
    }

    #[test]
    fn a_clone_of_typing_sees_the_same_line() {
        let typing = Typing::default();
        let other = typing.clone();
        typing.set("a draft");
        assert_eq!(other.text(), "a draft");
    }
}
