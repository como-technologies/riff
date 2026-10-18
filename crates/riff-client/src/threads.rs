//! The threads model: the lines of `riff read`, `riff tail` and
//! `riff chat` (01M56Y8KAPHJMSK89ZW1PA8B0E, 01M56Y8KHD61NXC57V7H546KWW).
//!
//! # Design
//!
//! riff-server gives the messages of a thread: a page of a read
//! ([`Inbox`]) or one message of the stream ([`Checked`]). This module
//! makes the model of each one, as plain data with serde:
//!
//! - [`Line`]: one message. It holds the facts that each view shows:
//!   the sender ([`Line::sender`]), the receivers ([`To`]) with the mark of
//!   the whole thread, the mark `verified`, the lead mark of a verified
//!   lead, the kind and the body. An action line (`/me TEXT`,
//!   01M3NJD37CNQX580YC24S7K6ES) has [`Line::action`].
//! - [`Thread`]: the lines of one thread of a read, and the other member
//!   of a direct thread, for its label.
//!
//! `riff read` draws a [`Thread`] for each thread, `riff tail` a block
//! for each [`Line`], and `riff chat` a chat line for each [`Line`]. The
//! words, the colors and the wrap stay in the UI.
//!
//! ```mermaid
//! flowchart LR
//!     S["riff-server"] -- "a read: Inbox" --> T["Thread"]
//!     S -- "the stream: Checked" --> L["Line"]
//!     T --> R["riff read"]
//!     L --> TA["riff tail"]
//!     L --> C["riff chat"]
//! ```
//!
//! ```
//! use riff_client::api::Checked;
//! use riff_client::threads::Line;
//! use riff_core::wire::{Kind, Message};
//!
//! let thread = "o/r".parse()?;
//! let message = Message {
//!     seq: 2,
//!     from: "riff://mike@pangolin/o/r?session=a6cf&lead=true".parse()?,
//!     to: vec!["repo=o/r".parse()?, "claim=issue-6".parse()?],
//!     body: "/me waves".into(),
//!     at_ms: 0,
//!     kind: Kind::Message,
//!     sig: None,
//!     payload: None,
//! };
//! let line = Line::of(&Checked { message, verified: true }, &thread);
//! assert!(line.lead);
//! let to: Vec<(String, bool)> = line.to.iter().map(|t| (t.selector.to_string(), t.all)).collect();
//! assert_eq!(to, [("repo=o/r".to_owned(), true), ("claim=issue-6".to_owned(), false)]);
//! assert_eq!(line.action(), Some("waves"));
//! assert_eq!(line.sender().name, "mike's lead");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::Kind;
use serde::{Deserialize, Serialize};

use crate::api::{Checked, Inbox};

/// The start of the body of an action line of the chat, as `/me` in
/// IRC (01M3NJD37CNQX580YC24S7K6ES). An action is a plain message, so
/// each riff shows it, also an older one.
pub const ACTION: &str = "/me ";

/// The text of an action line: the body after [`ACTION`]. `None` for
/// each other body.
///
/// ```
/// use riff_client::threads::action_text;
///
/// assert_eq!(action_text("/me waves"), Some("waves"));
/// assert_eq!(action_text("hi /me"), None);
/// ```
pub fn action_text(body: &str) -> Option<&str> {
    body.strip_prefix(ACTION)
}

/// A receiver of a message: the sessions that its selector matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct To {
    pub selector: Selector,
    /// True for the selector of the repository of the thread: each
    /// session of the thread.
    pub all: bool,
}

/// The sender of a line, as a chat shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sender {
    /// A person is `USER@HOST`. A verified lead is `USER's lead`. Each
    /// other session is `USER` and the first 8 characters of its session
    /// ID. The text is as the sender gave it: the UI makes it safe.
    pub name: String,
    /// True for a session, false for a person.
    pub session: bool,
}

/// One message of a thread (01M56Y8KHD61NXC57V7H546KWW).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub seq: u64,
    /// The time of the message, in milliseconds since 1970.
    pub at_ms: u64,
    pub from: SessionUri,
    /// True when the reader proved the sender (R199).
    pub verified: bool,
    /// True for a verified lead: the lead mark of a sender that the
    /// reader did not prove does not count.
    pub lead: bool,
    /// The receivers, in the order of the message.
    pub to: Vec<To>,
    pub kind: Kind,
    pub body: String,
}

impl Line {
    /// The line of the message `c` of `thread`.
    pub fn of(c: &Checked, thread: &ThreadName) -> Line {
        let m = &c.message;
        let all = Selector {
            repo: Some(thread.to_string()),
            ..Selector::default()
        };
        Line {
            seq: m.seq,
            at_ms: m.at_ms,
            from: m.from.clone(),
            verified: c.verified,
            lead: c.verified && m.from.lead(),
            to: m
                .to
                .iter()
                .map(|s| To {
                    selector: s.clone(),
                    all: *s == all,
                })
                .collect(),
            kind: m.kind,
            body: m.body.clone(),
        }
    }

    /// The text of an action line, or `None` for another body.
    pub fn action(&self) -> Option<&str> {
        action_text(&self.body)
    }

    /// The sender of the line.
    ///
    /// ```
    /// use riff_client::api::Checked;
    /// use riff_client::threads::{Line, Sender};
    /// use riff_core::wire::{Kind, Message};
    ///
    /// let line = |uri: &str, verified| {
    ///     let message = Message {
    ///         seq: 1,
    ///         from: uri.parse().unwrap(),
    ///         to: vec![],
    ///         body: "hi".into(),
    ///         at_ms: 0,
    ///         kind: Kind::Message,
    ///         sig: None,
    ///         payload: None,
    ///     };
    ///     Line::of(&Checked { message, verified }, &"chat".parse().unwrap())
    /// };
    /// let sender = |name: &str, session| Sender { name: name.into(), session };
    /// let lead = "riff://mike@thelio/o/r?session=4e54d4e5-c891&lead=true";
    /// assert_eq!(line("riff://mike@thelio", true).sender(), sender("mike@thelio", false));
    /// assert_eq!(line(lead, true).sender(), sender("mike's lead", true));
    /// assert_eq!(line(lead, false).sender(), sender("mike 4e54d4e5", true));
    /// ```
    pub fn sender(&self) -> Sender {
        let user = self.from.who().user();
        let name = match self.from.who().session() {
            None => {
                return Sender {
                    name: format!("{user}@{}", self.from.place().host()),
                    session: false,
                };
            }
            Some(_) if self.lead => format!("{user}'s lead"),
            Some(id) => format!("{user} {}", id.chars().take(8).collect::<String>()),
        };
        Sender {
            name,
            session: true,
        }
    }
}

/// The lines of one thread of a read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub name: ThreadName,
    /// The other member of a direct thread, or the reader when it has
    /// none. `None` for a thread that is not direct.
    pub other: Option<SessionUri>,
    pub lines: Vec<Line>,
    /// The seq of the last line, when more messages follow.
    pub next: Option<u64>,
}

impl Thread {
    /// The thread of `inbox`, a page of a read of `me`.
    ///
    /// ```
    /// use riff_client::api::Inbox;
    /// use riff_client::threads::Thread;
    /// use riff_core::name::{SessionUri, ThreadName};
    ///
    /// let me: SessionUri = "riff://brett@heron?session=b1".parse()?;
    /// let mike: SessionUri = "riff://mike@pangolin?session=a1".parse()?;
    /// let thread = "o/r".parse()?;
    /// let inbox = Inbox { thread, members: vec![], messages: vec![], next: Some(4) };
    /// let t = Thread::of(&inbox, &me);
    /// assert_eq!((t.other, t.next), (None, Some(4)));
    /// let direct = ThreadName::direct(me.who(), mike.who());
    /// let inbox = Inbox { thread: direct, members: vec![me.clone(), mike.clone()], messages: vec![], next: None };
    /// assert_eq!(Thread::of(&inbox, &me).other, Some(mike));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn of(inbox: &Inbox, me: &SessionUri) -> Thread {
        let other = inbox.thread.is_direct().then(|| {
            inbox
                .members
                .iter()
                .find(|m| m.who() != me.who())
                .unwrap_or(me)
                .clone()
        });
        Thread {
            name: inbox.thread.clone(),
            other,
            lines: inbox
                .messages
                .iter()
                .map(|c| Line::of(c, &inbox.thread))
                .collect(),
            next: inbox.next,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riff_core::wire::Message;

    /// A sample reply of a read of riff-server: one message to the
    /// whole thread, and a status request to a claim.
    const MESSAGES: &str = r#"[
        {"seq": 1, "from": "riff://mike@pangolin/o/r?session=a6cf&lead=true", "to": [{"repo": "o/r"}],
         "body": "hello", "at_ms": 7},
        {"seq": 2, "from": "riff://ann@heron/o/r?session=b7d0", "to": [{"claim": "issue-6"}],
         "body": "", "at_ms": 9, "kind": "status"}
    ]"#;

    fn inbox(verified: bool) -> Inbox {
        let messages: Vec<Message> = serde_json::from_str(MESSAGES).unwrap();
        Inbox {
            thread: "o/r".parse().unwrap(),
            members: vec![],
            messages: messages
                .into_iter()
                .map(|message| Checked { message, verified })
                .collect(),
            next: Some(2),
        }
    }

    #[test]
    fn the_model_comes_from_a_reply_of_a_read() {
        let me = "riff://brett@heron?session=c1".parse().unwrap();
        let thread = Thread::of(&inbox(true), &me);
        assert_eq!(thread.other, None);
        assert_eq!(thread.next, Some(2));
        let [hello, status] = thread.lines.as_slice() else {
            panic!("{thread:?}");
        };
        assert_eq!((hello.seq, hello.at_ms), (1, 7));
        assert!(hello.to.iter().all(|t| t.all), "{hello:?}");
        assert!(hello.lead && hello.verified);
        assert_eq!(status.kind, Kind::Status);
        let claim = To {
            selector: "claim=issue-6".parse().unwrap(),
            all: false,
        };
        assert_eq!(status.to, [claim]);
        assert_eq!(status.sender().name, "ann b7d0");
    }

    #[test]
    fn a_lead_that_the_reader_did_not_prove_is_no_lead() {
        let me = "riff://brett@heron?session=c1".parse().unwrap();
        let thread = Thread::of(&inbox(false), &me);
        assert!(!thread.lines[0].lead);
        assert_eq!(thread.lines[0].sender().name, "mike a6cf");
    }

    #[test]
    fn the_model_is_plain_data_with_serde() {
        let me = "riff://brett@heron?session=c1".parse().unwrap();
        let thread = Thread::of(&inbox(true), &me);
        let json = serde_json::to_string(&thread).unwrap();
        assert_eq!(serde_json::from_str::<Thread>(&json).unwrap(), thread);
    }
}
