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
//! goes away: the line shows once, as the chat line from the server. A
//! person shows as `<USER@HOST>`, and a session as `[USER's lead]` or
//! `[USER SESSION]` (01M3NJD3BR0XAYNNFTEY0CG761). With a pipe, the chat
//! has no prompt and no line editor.
//!
//! After an update, the chat runs the new `riff` in place
//! (01M3NT6WXGCNKW3EQ7MBJDQTR4), between two lines. It gives the new
//! chat the last line that it showed in the hidden option [`AFTER`], so
//! the new chat shows no line again. In a terminal, the old chat puts
//! the terminal back in its normal mode first, and the new chat draws
//! its prompt again. The text that the person typed but did not send
//! is lost.
//!
//! `/me TEXT` posts an action line: a plain message with the body
//! `/me TEXT` (01M3NJD37CNQX580YC24S7K6ES), so an older riff reads it
//! too. It shows as `* USER@HOST TEXT`.
//!
//! A line wakes no session, unless it names a lead (01M3NB5N0D99JB5CE6RB4VEYPF):
//! `@lead` wakes the lead of the sender, and `@USER` wakes the lead of
//! USER. A lead answers with a post to the chat thread.
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
use tokio::sync::mpsc;

use nix::sys::termios::{SetArg, Termios, tcgetattr, tcsetattr};

use crate::api::{Api, Checked, Reconnect};
use crate::binary::{Follow, with_last, with_place};
use crate::catch_up::{self, Seen, Start};
use crate::link::STREAM_RETRY;
use crate::style::{DIM, WARNING, styled};
use crate::text::{ACTION, action_text, lost_messages, safe};

/// The name of the chat thread.
pub const THREAD: &str = riff_core::name::CHAT;

/// The line that ends `riff chat`.
pub const QUIT: &str = "/quit";

/// The prompt of the input line in a terminal.
pub const PROMPT: &str = "[riff] > ";

/// The hidden option that gives a new chat the last line that the old
/// chat showed, after an update.
pub const AFTER: &str = "--after";

/// The chat thread.
///
/// ```
/// assert_eq!(riff::chat::thread().to_string(), "chat");
/// ```
pub fn thread() -> ThreadName {
    ThreadName::chat()
}

/// The sessions that a chat line of `sender` wakes: the lead of each
/// user that the line names with `@USER`, and the lead of the sender for
/// `@lead`. Punctuation after the name does not count. A line with no
/// `@` wakes no session.
///
/// ```
/// use riff::chat::wakes;
///
/// let lead = |user: &str| format!("user={user},lead=true").parse().unwrap();
/// assert_eq!(wakes("@lead: is #12 done?", "mike"), [lead("mike")]);
/// assert_eq!(wakes("ask @brett, and @lead", "mike"), [lead("brett"), lead("mike")]);
/// assert_eq!(wakes("@mike @lead", "mike"), [lead("mike")]);
/// assert!(wakes("mail mike@thelio about it", "brett").is_empty());
/// assert!(wakes("no names here", "brett").is_empty());
/// ```
pub fn wakes(line: &str, sender: &str) -> Vec<Selector> {
    let mut users: Vec<&str> = Vec::new();
    for word in line.split_whitespace() {
        let Some(name) = word.strip_prefix('@') else {
            continue;
        };
        let name = name.trim_end_matches(|c: char| !c.is_alphanumeric());
        let user = if name == "lead" { sender } else { name };
        if !user.is_empty() && !users.contains(&user) {
            users.push(user);
        }
    }
    users
        .into_iter()
        .map(|user| Selector {
            user: Some(user.to_owned()),
            lead: Some(true),
            ..Selector::default()
        })
        .collect()
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

/// The name of the sender of a chat line, and true for a session. A
/// person is `USER@HOST`. A verified lead is `USER's lead`. Each other
/// session is `USER` and the first 8 characters of its session ID.
///
/// ```
/// use riff::api::Checked;
/// use riff_core::wire::{Kind, Message};
///
/// let from = |uri: &str, verified| Checked {
///     message: Message {
///         seq: 1,
///         from: uri.parse().unwrap(),
///         to: vec![],
///         body: "hi".into(),
///         at_ms: 0,
///         kind: Kind::Message,
///         sig: None,
///         payload: None,
///     },
///     verified,
/// };
/// let lead = "riff://mike@thelio/o/r?session=4e54d4e5-c891&lead=true";
/// let person = riff::chat::sender(&from("riff://mike@thelio", true));
/// assert_eq!(person, ("mike@thelio".to_owned(), false));
/// assert_eq!(riff::chat::sender(&from(lead, true)), ("mike's lead".to_owned(), true));
/// assert_eq!(riff::chat::sender(&from(lead, false)), ("mike 4e54d4e5".to_owned(), true));
/// ```
pub fn sender(c: &Checked) -> (String, bool) {
    let from = &c.message.from;
    let user = from.who().user();
    let name = match from.who().session() {
        None => return (safe(&format!("{user}@{}", from.place().host())), false),
        Some(_) if c.verified && from.lead() => format!("{user}'s lead"),
        Some(id) => format!("{user} {}", id.chars().take(8).collect::<String>()),
    };
    (safe(&name), true)
}

/// One chat line for people. A date line comes first when the day of
/// `at` is not `last_day`. Then the time, the sender and the body: a
/// person as `<USER@HOST>`, a session as `[NAME]` (see [`sender`]), and
/// an action as `* NAME TEXT`. Each more line of the body is indented.
/// The line has ANSI styles: print it through `anstream`. It removes
/// each escape sequence from the body and the sender.
///
/// ```
/// use chrono::TimeZone;
/// use riff::api::Checked;
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
/// let plain = |c: &Checked| {
///     let line = riff::chat::line(c, &at, Some(at.date_naive()));
///     anstream::adapter::strip_str(&line).to_string()
/// };
/// assert_eq!(plain(&c), "14:13 <mike@thelio> hi all");
/// let first = riff::chat::line(&c, &at, None);
/// assert!(anstream::adapter::strip_str(&first).to_string().starts_with("2026-09-28\n14:13"));
///
/// // A lead looks different from its person.
/// let lead = "riff://mike@thelio/o/r?session=4e54d4e5&lead=true".parse()?;
/// let answer = Message { from: lead, ..c.message.clone() };
/// assert_eq!(plain(&Checked { message: answer, verified: true }), "14:13 [mike's lead] hi all");
///
/// let waves = Message { body: "/me waves".into(), ..c.message.clone() };
/// assert_eq!(plain(&Checked { message: waves, verified: true }), "14:13 * mike@thelio waves");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn line<Tz: TimeZone>(c: &Checked, at: &DateTime<Tz>, last_day: Option<NaiveDate>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let m = &c.message;
    let mut out = String::new();
    if last_day != Some(at.date_naive()) {
        let _ = writeln!(out, "{}", styled(DIM, &at.format("%Y-%m-%d").to_string()));
    }
    let (name, session) = sender(c);
    let name = styled(crate::style::session(&m.from), &name);
    let (who, body) = match (action_text(&m.body), session) {
        (Some(text), _) => (format!("* {name}"), text),
        (None, false) => (format!("<{name}>"), m.body.as_str()),
        (None, true) => (format!("[{name}]"), m.body.as_str()),
    };
    let body = safe(body).replace('\t', "    ");
    let _ = write!(
        out,
        "{} {who} {}",
        styled(DIM, &at.format("%H:%M").to_string()),
        body.lines().collect::<Vec<_>>().join("\n      ")
    );
    out
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
/// On a new binary, it runs it in place, between two lines
/// (01M3NT6WXGCNKW3EQ7MBJDQTR4).
///
/// While `riff-server` starts again, it shows the line
/// [`crate::link::WAITING`] above the prompt, and keeps its screen
/// (01M3THEE5V3RFHF9QTA8MA8QDF).
///
/// It sends the typed lines from a task of their own, in their order
/// (01M4A804AWYEHPYZ966A6PXRPF). So the screen takes input while a post
/// waits for its budget. At the end, it waits for the posts of the lines
/// that the person typed.
pub async fn run(api: &Api, me: &SessionUri, after: Option<u64>) -> Result<()> {
    // The wait line goes to the screen, once there is one.
    let lines = Arc::new(OnceLock::<Lines>::new());
    let screen_lines = Arc::clone(&lines);
    let api = &api.clone().waits_to(move |line| match screen_lines.get() {
        Some(lines) => lines.warn(line),
        None => anstream::eprintln!("{line}"),
    });
    let thread = thread();
    api.join(me, &thread).await?;
    if after.is_none() {
        anstream::eprintln!(
            "riff: chat as {}@{}. Type a line and press Enter. /me TEXT sends an action. \
             @lead wakes your lead. {QUIT} or Ctrl-C exits.",
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
            day.line(last);
        }
    }
    // Connected first, then read: no line falls between the two
    // (see [`crate::catch_up`]).
    let start = after.map_or(Start::History, Start::After);
    let shown = Arc::new(Mutex::new(catch_up::Shown::new(start)));
    let first = catch_up::connect(api, me, thread, &shown).await?;
    let again = catch_up::follow_thread(api, me, thread, Arc::clone(&shown), STREAM_RETRY);
    let mut stream = Box::pin(first.chain(again));
    let (screen, mut input) = Screen::start()?;
    let _ = lines.set(screen.lines.clone());
    let (send, posts) = posts(api, me, thread, screen.lines.clone());
    let mut link = Reconnect::default();
    let follow = Follow::this();
    let update = follow.new_one();
    tokio::pin!(update);
    loop {
        tokio::select! {
            () = &mut update => {
                screen.leave();
                let args = with_place(std::env::args_os().skip(1), me.place());
                let last = shown.lock().map_or(None, |s| s.last()).or(after).unwrap_or(0);
                follow.run(with_last(args, AFTER, last.to_string()));
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
                let to = wakes(&body, me.who().user());
                let _ = send.send((to, body));
            }
            item = stream.next() => {
                let Some(item) = item else { break };
                if let Some(line) = link.line(&item) {
                    screen.warn(&line);
                }
                match item {
                    Ok(Seen::Message(c)) => screen.print(&day.line(&c)),
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

/// The task that posts each typed line of the chat, in order
/// (01M4A804AWYEHPYZ966A6PXRPF). A post that fails shows its error on
/// the screen.
fn posts(
    api: &Api,
    me: &SessionUri,
    thread: &ThreadName,
    lines: Lines,
) -> (
    mpsc::UnboundedSender<(Vec<Selector>, String)>,
    tokio::task::JoinHandle<()>,
) {
    let (send, mut typed) = mpsc::unbounded_channel::<(Vec<Selector>, String)>();
    let (api, me, thread) = (api.clone(), me.clone(), thread.clone());
    let task = tokio::spawn(async move {
        while let Some((to, body)) = typed.recv().await {
            if let Err(e) = api
                .post(&me, Some(&thread), &to, &body, Kind::Message)
                .await
            {
                lines.warn(&format!("riff: cannot send the line: {e:#}"));
            }
        }
    });
    (send, task)
}

/// The printer of the line editor: it prints above the prompt.
type Printer = Arc<Mutex<Box<dyn ExternalPrinter + Send>>>;

/// Where the chat prints its lines.
#[derive(Clone)]
enum Lines {
    /// A pipe or a file: no prompt and no line editor.
    Plain,
    /// A terminal: a line editor with the prompt [`PROMPT`].
    Editor { printer: Printer, color: bool },
}

impl Lines {
    /// Prints a chat line.
    fn print(&self, text: &str) {
        match self {
            Lines::Plain => anstream::println!("{text}"),
            Lines::Editor { printer, color } => {
                let text = if *color {
                    text.to_owned()
                } else {
                    anstream::adapter::strip_str(text).to_string()
                };
                if let Ok(mut printer) = printer.lock() {
                    let _ = printer.print(format!("{text}\n"));
                }
            }
        }
    }

    /// Prints a line of riff itself: on stderr with a pipe, above the
    /// prompt in a terminal.
    fn warn(&self, text: &str) {
        match self {
            Lines::Plain => anstream::eprintln!("{text}"),
            Lines::Editor { .. } => self.print(text),
        }
    }
}

/// The screen of the chat.
struct Screen {
    lines: Lines,
    /// The mode of the terminal before the line editor.
    normal: Option<Termios>,
}

impl Screen {
    /// Starts to read the typed lines: with a line editor when stdin and
    /// stdout are a terminal, else line by line. The lines come on the
    /// receiver. It closes when stdin ends, on Ctrl-C or Ctrl-D, and
    /// after [`QUIT`], so the terminal is back in its normal mode.
    fn start() -> Result<(Screen, mpsc::Receiver<String>)> {
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
            };
            return Ok((screen, rx));
        }
        let normal = tcgetattr(std::io::stdin()).ok();
        let mut editor = rustyline::DefaultEditor::new()?;
        let printer: Printer = Arc::new(Mutex::new(Box::new(editor.create_external_printer()?)));
        let held = Arc::clone(&printer);
        std::thread::spawn(move || {
            while let Ok(line) = editor.readline(PROMPT) {
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
        };
        Ok((screen, rx))
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

    /// Prints a chat line.
    fn print(&self, text: &str) {
        self.lines.print(text);
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
    /// The line of `c`, with a date line when its day is new.
    fn line(&mut self, c: &Checked) -> String {
        let at = i64::try_from(c.message.at_ms)
            .ok()
            .and_then(|ms| Local.timestamp_millis_opt(ms).single())
            .unwrap_or_else(Local::now);
        let line = line(c, &at, self.0);
        self.0 = Some(at.date_naive());
        line
    }
}
