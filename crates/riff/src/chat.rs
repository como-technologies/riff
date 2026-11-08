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
//! the name of its project, `<riff>`, and riff itself as `<riff-bot>`.
//! The chat reads the names of riff-server ([`Names`]) at its start: on a
//! clash, a project has the name `OWNER/REPO`. A session with no name,
//! for example a worker, shows as `[USER SESSION]`. The host shows only in
//! the detail of a line: in `riff tail` and `riff read`.
//!
//! ```text
//! 14:02 <mike> is the release out?
//! 14:03 <riff> @mike not yet: #12 waits for CI.
//! 14:09 <riff-bot> the deploy waits for your approval
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
//! A line wakes each lead of the sender, unless it names a lead
//! (01M3NB5N0D99JB5CE6RB4VEYPF): `@lead` wakes each lead of the sender,
//! and `@USER` wakes each lead of USER. A user has one lead in each
//! repository, so a person with two repositories has two leads. A lead
//! answers with a post to the chat thread.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as riff chat (mike)
//!     participant S as riff-server
//!     participant B as riff chat (brett)
//!     participant L as lead of brett
//!     A->>S: post "hi @brett" to chat, to user=brett,lead=true
//!     S-->>A: tail: the line
//!     S-->>B: tail: the line
//!     S-->>L: wake
//!     L->>S: post the answer to chat
//!     S-->>A: tail: the answer
//!     S-->>B: tail: the answer
//! ```
//!
//! `@lead:REPO` and `@USER:REPO` wake one lead: the lead in the
//! repository REPO (01M5QVH7KDK3R2S4NDFB2V1XY9). REPO is `OWNER/REPO`, or
//! the name after the slash. [`riff_client::chat::wakes`] reads the line,
//! and [`riff_client::chat::resolve`] picks the lead from the leads that
//! riff-server lists. `riff chat` and the Chat tab of the TUI share them
//! ([`crate::tui::chat`]). When no lead or more than one lead fits, the
//! chat sends nothing and names the leads that it found.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as riff chat (mike)
//!     participant S as riff-server
//!     participant L as lead of brett in strata
//!     A->>S: who
//!     S-->>A: the sessions, with each lead
//!     A->>S: post "@brett:strata hi" to chat, to user=brett,repo=como-technologies/strata,lead=true
//!     S-->>L: wake
//!     Note over A,S: no lead or two leads fit: the chat sends nothing
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
use riff_core::selector::Selector;
use riff_core::wire::Kind;
use rustyline::ExternalPrinter;
use rustyline::completion::Completer;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use tokio::sync::mpsc;

use nix::sys::termios::{SetArg, Termios, tcgetattr, tcsetattr};

use crate::binary::{Follow, with_last, with_place};
use crate::catch_up::{self, Seen, Start};
use crate::highlight::{Highlights, Lit, Out, Title};
use crate::style::{DIM, WARNING, styled};
use crate::text::{lost_messages, safe};
use riff_client::api::{Api, Refusal};
use riff_client::link::STREAM_RETRY;
use riff_client::threads::{ACTION, BY_RIFF, Line, Names, Sender, action_text};

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

/// Posts `body` in riff chat as `me`, as a line that riff writes: the
/// kind of the post is `riff` (01M5TKMVB7CF0QNAWXEVGEQETJ). The watch of
/// a deploy, the daemon and `riff release` post so, as the person of the
/// riff. `to` is what [`riff_client::chat::wakes`] gave for `body`. A
/// line of the kind `riff` is no word of the person.
///
/// A riff-server of the release before refuses the kind `riff`. After a
/// rollback of the server to that release, the line goes again with the
/// kind `message` and the mark [`BY_RIFF`] at the start of its body
/// (01M5TQ0DX4M2EJJGE7QA8DZVYS). Any other refusal is an error.
///
/// ```mermaid
/// sequenceDiagram
///     participant R as riff
///     participant S as riff-server (the release before)
///     R->>S: post, kind riff
///     S-->>R: refused: no such kind
///     R->>S: post, kind message, body "[by riff] ..."
///     S-->>R: posted
/// ```
pub async fn post_by_riff(api: &Api, me: &SessionUri, to: &[Selector], body: &str) -> Result<()> {
    post_by_riff_keyed(api, me, to, body, None).await
}

/// [`post_by_riff`] with a key (01M5TKG2K1AYK1KE7C2AA2W9DS): riff-server
/// drops the post when a kept message of the same user in the thread has
/// the key. The line that goes again with the kind `message` has the key
/// too.
pub async fn post_by_riff_keyed(
    api: &Api,
    me: &SessionUri,
    to: &[Selector],
    body: &str,
    key: Option<&str>,
) -> Result<()> {
    let thread = thread();
    let sent = api
        .post_keyed(me, Some(&thread), to, body, Kind::Riff, key)
        .await;
    match sent {
        Err(e) if refuses_kind_riff(&e) => {
            let marked = format!("{BY_RIFF}{body}");
            api.post_keyed(me, Some(&thread), to, &marked, Kind::Message, key)
                .await?;
            Ok(())
        }
        sent => sent.map(drop),
    }
}

/// True when `error` is the refusal of a riff-server that does not know
/// the kind `riff`: a bad request with "no such kind". No other error is.
///
/// ```
/// use riff::chat::refuses_kind_riff;
/// use riff_client::api::Refusal;
///
/// let refusal = |status, text: &str| {
///     anyhow::Error::new(Refusal {
///         op: "post".into(),
///         status,
///         code: Some("bad_request".into()),
///         text: text.into(),
///     })
/// };
/// let bad = reqwest::StatusCode::BAD_REQUEST;
/// let old = "no such kind: use message, status or note";
/// assert!(refuses_kind_riff(&refusal(bad, old)));
/// // Another bad request, another status and another error do not match.
/// assert!(!refuses_kind_riff(&refusal(bad, "the thread is too long")));
/// assert!(!refuses_kind_riff(&refusal(reqwest::StatusCode::FORBIDDEN, old)));
/// assert!(!refuses_kind_riff(&anyhow::anyhow!(old)));
/// ```
pub fn refuses_kind_riff(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Refusal>().is_some_and(|refusal| {
        refusal.status == reqwest::StatusCode::BAD_REQUEST && refusal.text.contains("no such kind")
    })
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
    /// tab. A line of riff itself has no mark [`BY_RIFF`] here.
    pub body: String,
}

/// The sender and the body of the chat line `m`, with the names `names`
/// of riff-server. The sender shows by the name to type to reach it
/// (01M6Y1FB5MZNZ971CJRBWZ2F9X): a person as `<USER>`, a verified lead
/// as `<PROJECT>`, and riff itself as `<riff-bot>`. Each other session
/// has no name, and shows as `[USER ID]`. An action is `* NAME TEXT`. It
/// removes each escape sequence from the body and the sender.
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
/// // riff itself, also with the mark of an old riff-server.
/// let riff = Message { body: "[by riff] a release waits".into(), ..message.clone() };
/// let bot = said(&riff);
/// assert_eq!((bot.before, bot.name.as_str(), bot.after), ("<", "riff-bot", ">"));
/// assert_eq!(bot.body, "a release waits");
///
/// // A worker has no name.
/// let worker = "riff://mike@thelio/o/r?session=0271342b-7454&claim=issue-6".parse()?;
/// let done = said(&Message { from: worker, body: "done".into(), ..message });
/// assert_eq!((done.before, done.name.as_str(), done.after), ("[", "mike 0271342b", "]"));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn said(m: &Line, names: &Names) -> Said {
    let Sender { name, named } = m.sender(names);
    // A body with the mark is a line of riff itself (Line::by_riff).
    let body = m.body.strip_prefix(BY_RIFF).unwrap_or(&m.body);
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
pub async fn run(
    api: &Api,
    me: &SessionUri,
    after: Option<u64>,
    draft: Option<&str>,
) -> Result<()> {
    let config = crate::settings::chat_here()?;
    let thread = thread();
    api.join(me, &thread).await?;
    // A riff-server that gives no names: each project has the name
    // after the slash.
    let names = Names::new(api.names(me).await.unwrap_or_default());
    if after.is_none() {
        anstream::eprintln!(
            "riff: chat as {}@{}. Type a line and press Enter. /me TEXT sends an action. \
             A line wakes each lead of yours; @USER wakes each lead of USER; \
             @lead:REPO or @USER:REPO wakes one lead. {QUIT} or Ctrl-C exits.",
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
            day.line(&Line::of(last, thread), &names, false);
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
    let (screen, mut input) = Screen::start(draft, highlights.title())?;
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
                        let lit = highlights.line(&line, &names, thread, new);
                        screen.print(&day.line(&line, &names, lit.bold), &lit);
                    }
                    Ok(Seen::Lost(n)) => screen.warn(&styled(WARNING, &lost_messages(n))),
                    Err(_) => {}
                }
            }
        }
    }
    drop(send);
    let _ = posts.await;
    Ok(())
}

/// The task that posts each line of the chat that `me` sends, in
/// order (01M4A804AWYEHPYZ966A6PXRPF): `riff chat` and the Chat tab of
/// the TUI give it each body. The receivers come from
/// [`riff_client::chat::receivers`]. A post that fails gives `fault` its
/// error, as a safe line for the person. A line with a lead that the
/// chat cannot pick ([`riff_client::chat::resolve`]) is not sent, and
/// the line names the leads that the chat found.
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
            let sent = async {
                let to = riff_client::chat::receivers(&api, &me, &body).await?;
                api.post(&me, Some(&thread), &to, &body, Kind::Message)
                    .await
            };
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

/// A copy of the text that the person is typing. The line editor lives
/// on a thread of its own and has no call to read its line, but it asks
/// a hinter for a hint after each edit, and at each new prompt. This
/// hinter keeps the line and gives no hint. It also clears the number of
/// the highlights in the title of the terminal: a key or a new prompt
/// clears it (01M6XSHMNNTRC66CMZ0DF2X5RA).
#[derive(Clone, Default)]
struct Typing {
    text: Arc<Mutex<String>>,
    title: Option<Title>,
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
    type Hint = String;

    fn hint(&self, line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<String> {
        self.set(line);
        // The line editor calls this on its own thread, the thread that
        // writes the screen: the title goes out between two of its writes.
        if let Some(clear) = self.title.as_ref().and_then(Title::clear) {
            let mut out = std::io::stdout();
            let _ = out.write_all(clear.as_bytes());
            let _ = out.flush();
        }
        None
    }
}

impl Completer for Typing {
    type Candidate = String;
}

impl Highlighter for Typing {}

impl Validator for Typing {}

impl rustyline::Helper for Typing {}

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
    /// person typed in the chat before an update, and a key clears the
    /// number of `title`.
    fn start(draft: Option<&str>, title: Title) -> Result<(Screen, mpsc::Receiver<String>)> {
        let (tx, rx) = mpsc::channel(16);
        if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
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
            ..Typing::default()
        };
        let mut editor = rustyline::Editor::<Typing, DefaultHistory>::new()?;
        editor.set_helper(Some(typing.clone()));
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

    #[test]
    fn a_clone_of_typing_sees_the_same_line() {
        let typing = Typing::default();
        let other = typing.clone();
        typing.set("a draft");
        assert_eq!(other.text(), "a draft");
    }
}
