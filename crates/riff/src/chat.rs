//! `riff chat`: the people of a riff chat about the riff, in the style
//! of IRC (01M3NB5MY93KV9RKZGGSMZW00D).
//!
//! The chat is the thread [`THREAD`] on the riff server. A person posts
//! as `USER@HOST`, with no session. The server keeps its rules: only a
//! member reads or posts, and the sender comes from the sign-in.
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
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use futures::StreamExt;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::Kind;
use tokio::io::AsyncBufReadExt;

use crate::api::{Api, Checked, follow};
use crate::style::{BOLD, DIM, styled};
use crate::text::safe;

/// The name of the chat thread.
pub const THREAD: &str = "chat";

/// The line that ends `riff chat`.
pub const QUIT: &str = "/quit";

/// The time between two tries to connect the chat again.
const RETRY: Duration = Duration::from_secs(5);

/// The chat thread.
///
/// ```
/// assert_eq!(riff::chat::thread().to_string(), "chat");
/// ```
pub fn thread() -> ThreadName {
    THREAD.parse().expect("chat is a thread name")
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

/// One chat line for people. A date line comes first when the day of
/// `at` is not `last_day`. Then the time, the sender as `USER@HOST`,
/// `lead` for a verified lead, and the body. Each more line of the body
/// is indented. The line has ANSI styles: print it through `anstream`.
/// It removes each escape sequence from the body and the sender.
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
/// };
/// let c = Checked { message, verified: true };
/// let at = chrono::Utc.with_ymd_and_hms(2026, 9, 28, 14, 13, 0).unwrap();
/// let line = riff::chat::line(&c, &at, Some(at.date_naive()));
/// assert_eq!(anstream::adapter::strip_str(&line).to_string(), "14:13 mike@thelio  hi all");
/// let first = riff::chat::line(&c, &at, None);
/// assert!(anstream::adapter::strip_str(&first).to_string().starts_with("2026-09-28\n14:13"));
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
    let from = format!("{}@{}", m.from.who().user(), m.from.place().host());
    let _ = write!(
        out,
        "{} {}",
        styled(DIM, &at.format("%H:%M").to_string()),
        styled(crate::style::session(&m.from), &safe(&from))
    );
    if c.verified && m.from.lead() {
        let _ = write!(out, " {}", styled(BOLD, "lead"));
    }
    let body = safe(&m.body).replace('\t', "    ");
    let _ = write!(
        out,
        "  {}",
        body.lines().collect::<Vec<_>>().join("\n      ")
    );
    out
}

/// Runs the chat of `me` until stdin ends, or the person types
/// [`QUIT`]. It shows the history of the chat, then each new line, and
/// posts each line that the person types. It connects again when the
/// stream ends.
pub async fn run(api: &Api, me: &SessionUri) -> Result<()> {
    let thread = thread();
    api.join(me, &thread).await?;
    let first = api.tail(&thread).await?;
    let mut stream = Box::pin(first.chain(follow(|| api.tail(&thread), RETRY)));
    let mut shown = Shown::default();
    for c in api.read(me, &thread, true).await? {
        shown.print(&c);
    }
    anstream::eprintln!(
        "riff: chat as {}@{}. Type a line and press Enter. @lead wakes your lead. \
         {QUIT} or Ctrl-C exits.",
        me.who().user(),
        me.place().host()
    );
    let mut input = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    let mut lost = false;
    loop {
        tokio::select! {
            typed = input.next_line() => {
                let Some(typed) = typed? else { break };
                let typed = typed.trim();
                if typed == QUIT {
                    break;
                }
                if typed.is_empty() {
                    continue;
                }
                let to = wakes(typed, me.who().user());
                if let Err(e) = api.post(me, Some(&thread), &to, typed, Kind::Message).await {
                    anstream::eprintln!("riff: cannot send the line: {e:#}");
                }
            }
            item = stream.next() => match item {
                Some(Ok(c)) => {
                    if lost {
                        anstream::eprintln!("riff: connected again.");
                        lost = false;
                    }
                    shown.print(&c);
                }
                Some(Err(e)) if !lost => {
                    anstream::eprintln!(
                        "riff: {e:#}. Trying again every {} seconds.",
                        RETRY.as_secs()
                    );
                    lost = true;
                }
                Some(Err(_)) => {}
                None => break,
            },
        }
    }
    Ok(())
}

/// What the chat has shown: the last message and the last day.
#[derive(Default)]
struct Shown {
    seq: u64,
    day: Option<NaiveDate>,
}

impl Shown {
    /// Prints `c` once: a message from the history can come again on the
    /// stream.
    fn print(&mut self, c: &Checked) {
        if c.message.seq <= self.seq {
            return;
        }
        self.seq = c.message.seq;
        let at = i64::try_from(c.message.at_ms)
            .ok()
            .and_then(|ms| Local.timestamp_millis_opt(ms).single())
            .unwrap_or_else(Local::now);
        anstream::println!("{}", line(c, &at, self.day));
        self.day = Some(at.date_naive());
    }
}
