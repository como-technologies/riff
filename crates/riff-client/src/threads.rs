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
//! - [`direct_name`] and [`named`]: the name `dm:PEER` of a direct
//!   thread, that a reader types to read it again
//!   (01M58AJE14M9TPTQE4E8R9PA9M).
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
//! assert_eq!(line.sender().name, "mike's lead, r");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use riff_core::name::{NameError, Repo, SessionUri, ThreadName, Who};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, ThreadInfo};
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
    /// A person is `USER@HOST`. A verified lead is `USER's lead, REPO`,
    /// with the name of its repository (`USER's lead` outside git). Each
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
    /// let lead = "riff://mike@thelio/como-technologies/riff?session=4e54d4e5-c891&lead=true";
    /// assert_eq!(line("riff://mike@thelio", true).sender(), sender("mike@thelio", false));
    /// assert_eq!(line(lead, true).sender(), sender("mike's lead, riff", true));
    /// assert_eq!(line(lead, false).sender(), sender("mike 4e54d4e5", true));
    /// // A lead outside git has no repository to show.
    /// let nowhere = "riff://mike@thelio?session=4e54d4e5-c891&lead=true";
    /// assert_eq!(line(nowhere, true).sender(), sender("mike's lead", true));
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
            Some(_) if self.lead => match self.from.place().repo() {
                Repo::Git { name, .. } => format!("{user}'s lead, {name}"),
                Repo::None => format!("{user}'s lead"),
            },
            Some(id) => format!("{user} {}", id.chars().take(ID_CHARS).collect::<String>()),
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
    /// Each unread line after the page that woke the reader
    /// (01M588J4XNV1TXEPMGTKRVY9VK).
    pub woken: Vec<Line>,
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
    /// let inbox = Inbox { thread, members: vec![], messages: vec![], next: Some(4), woken: vec![] };
    /// let t = Thread::of(&inbox, &me);
    /// assert_eq!((t.other, t.next), (None, Some(4)));
    /// let direct = ThreadName::direct(me.who(), mike.who());
    /// let inbox = Inbox { thread: direct, members: vec![me.clone(), mike.clone()], messages: vec![], next: None, woken: vec![] };
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
            woken: inbox
                .woken
                .iter()
                .map(|c| Line::of(c, &inbox.thread))
                .collect(),
        }
    }
}

/// Puts each direct thread of a read first, and keeps the order of the
/// other threads (01M58AG7Z2PETHBZG7X9QD8PK5). A direct thread holds
/// the request of a lead or a question, so a full page of each other
/// thread does not hide it.
///
/// ```
/// use riff_client::api::Inbox;
/// use riff_client::threads::direct_first;
/// use riff_core::name::{SessionUri, ThreadName};
///
/// let me: SessionUri = "riff://brett@heron?session=b1".parse()?;
/// let mike: SessionUri = "riff://mike@pangolin?session=a1".parse()?;
/// let inbox = |thread: ThreadName| Inbox {
///     thread, members: vec![], messages: vec![], next: None, woken: vec![],
/// };
/// let direct = ThreadName::direct(me.who(), mike.who());
/// let mut list = [inbox("chat".parse()?), inbox("o/r".parse()?), inbox(direct.clone())];
/// direct_first(&mut list);
/// let order: Vec<String> = list.iter().map(|i| i.thread.to_string()).collect();
/// assert_eq!(order, [direct.to_string(), "chat".into(), "o/r".into()]);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn direct_first(list: &mut [Inbox]) {
    list.sort_by_key(|inbox| !inbox.thread.is_direct());
}

/// The characters of a session ID that a sender and a direct thread
/// show.
const ID_CHARS: usize = 8;

/// The start of the name of a direct thread for its reader: `dm:PEER`
/// (01M58AJE14M9TPTQE4E8R9PA9M).
pub const DIRECT: &str = "dm:";

/// The name of the direct thread `thread` for its member `me`: [`DIRECT`]
/// and the first 8 characters of the session ID of the other member, or
/// the user of a person. `None` when `thread` is not a direct thread of
/// `me` (01M58AJE14M9TPTQE4E8R9PA9M).
///
/// ```
/// use riff_client::threads::direct_name;
/// use riff_core::name::{ThreadName, Who};
///
/// let me = Who::new("brett", Some("b7d0"))?;
/// let lead = Who::new("mike", Some("25456ff6-479e"))?;
/// let person = Who::new("mike", None)?;
/// let name = |thread: &ThreadName| direct_name(thread, &me);
/// assert_eq!(name(&ThreadName::direct(&me, &lead)).unwrap(), "dm:25456ff6");
/// assert_eq!(name(&ThreadName::direct(&person, &me)).unwrap(), "dm:mike");
/// assert_eq!(name(&ThreadName::direct(&lead, &person)), None);
/// assert_eq!(name(&"chat".parse()?), None);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn direct_name(thread: &ThreadName, me: &Who) -> Option<String> {
    let peer = thread.peer(me)?;
    Some(match peer.session() {
        Some(id) => format!("{DIRECT}{}", id.chars().take(ID_CHARS).collect::<String>()),
        None => format!("{DIRECT}{}", peer.user()),
    })
}

/// The thread that `given` names for a read of `me`, one of `threads`:
/// the threads of `me` (01M58AJE14M9TPTQE4E8R9PA9M). A name with no
/// [`DIRECT`] is a plain thread name. `dm:PEER` names the direct thread
/// of `me` with the session whose ID is PEER or starts with it, or with
/// the person whose user is PEER. The text after a space does not
/// count, so the label of `threads` works too. A direct thread of two
/// other sessions is never found.
///
/// ```
/// use riff_client::threads::named;
/// use riff_core::name::{ThreadName, Who};
/// use riff_core::wire::ThreadInfo;
///
/// let me = Who::new("brett", Some("b7d0"))?;
/// let lead = Who::new("mike", Some("25456ff6-479e"))?;
/// let direct = ThreadName::direct(&me, &lead);
/// let info = |thread: &ThreadName| ThreadInfo { thread: thread.clone(), members: vec![], unread: 0 };
/// let threads = [info(&"chat".parse()?), info(&direct)];
/// assert_eq!(named("dm:25456ff6", &threads, &me), Ok(direct.clone()));
/// assert_eq!(named("dm:2545", &threads, &me), Ok(direct.clone()));
/// assert_eq!(named("dm:25456ff6 (direct with mike@pangolin:riff)", &threads, &me), Ok(direct));
/// assert_eq!(named("chat", &threads, &me), Ok("chat".parse()?));
/// assert!(named("dm:77e0", &threads, &me).unwrap_err().contains("no direct thread dm:77e0"));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn named(given: &str, threads: &[ThreadInfo], me: &Who) -> Result<ThreadName, String> {
    let Some(rest) = given.strip_prefix(DIRECT) else {
        return given.parse().map_err(|e: NameError| e.to_string());
    };
    let key = rest.split_whitespace().next().unwrap_or_default();
    let mine: Vec<(&ThreadName, Who)> = threads
        .iter()
        .filter_map(|t| Some((&t.thread, t.thread.peer(me)?)))
        .collect();
    let id = |peer: &Who| peer.session().unwrap_or(peer.user()).to_owned();
    let exact: Vec<_> = mine
        .iter()
        .filter(|(thread, peer)| {
            thread.to_string() == given || (!key.is_empty() && id(peer) == key)
        })
        .collect();
    let found: Vec<_> = if exact.is_empty() {
        mine.iter()
            .filter(|(_, peer)| {
                !key.is_empty() && peer.session().is_some_and(|s| s.starts_with(key))
            })
            .collect()
    } else {
        exact
    };
    match found.as_slice() {
        [(thread, _)] => Ok((*thread).clone()),
        [] => Err(format!(
            "you have no direct thread {DIRECT}{key}. The threads tool and riff read --all show \
             the name of each of your direct threads"
        )),
        _ => Err(format!(
            "{DIRECT}{key} names more than one of your direct threads: {}. Give more of the \
             session ID",
            found
                .iter()
                .map(|(_, peer)| format!("{DIRECT}{}", id(peer)))
                .collect::<Vec<_>>()
                .join(", ")
        )),
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
            woken: vec![],
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

    /// A full page of the chat and of the repository thread, then one
    /// direct message, in the order of the list of the server.
    #[test]
    fn a_read_shows_the_direct_thread_before_two_full_pages() {
        let me: SessionUri = "riff://brett@heron?session=b1".parse().unwrap();
        let lead: SessionUri = "riff://mike@pangolin?session=a1&lead=true".parse().unwrap();
        let page = |thread: ThreadName, count: u64| {
            let messages = (1..=count).map(|seq| Checked {
                message: Message {
                    seq,
                    from: lead.clone(),
                    to: vec![],
                    body: format!("{thread} {seq}"),
                    at_ms: 0,
                    kind: Kind::Message,
                    sig: None,
                    payload: None,
                },
                verified: true,
            });
            Inbox {
                members: vec![me.clone(), lead.clone()],
                messages: messages.collect(),
                next: (count == 50).then_some(count),
                woken: vec![],
                thread,
            }
        };
        let direct = ThreadName::direct(me.who(), lead.who());
        let name = |t: &str| t.parse::<ThreadName>().unwrap();
        let mut list = [
            page(name("chat"), 50),
            page(name("o/r"), 50),
            page(direct.clone(), 1),
        ];
        direct_first(&mut list);
        let order: Vec<(String, usize)> = list
            .iter()
            .map(|i| (i.thread.to_string(), i.messages.len()))
            .collect();
        assert_eq!(
            order,
            [
                (direct.to_string(), 1),
                ("chat".into(), 50),
                ("o/r".into(), 50)
            ]
        );
        let first = Thread::of(&list[0], &me);
        assert_eq!(first.other, Some(lead.clone()));
        assert_eq!(first.lines[0].body, format!("{direct} 1"));
    }

    fn who(user: &str, session: Option<&str>) -> Who {
        Who::new(user, session).unwrap()
    }

    fn info(thread: ThreadName) -> ThreadInfo {
        ThreadInfo {
            thread,
            members: vec![],
            unread: 0,
        }
    }

    #[test]
    fn a_session_finds_only_its_own_direct_threads() {
        let (me, ann, bob) = (
            who("brett", Some("b7d0")),
            who("ann", Some("a1b2")),
            who("bob", Some("c3d4")),
        );
        let theirs = ThreadName::direct(&ann, &bob);
        let threads = [info(theirs.clone()), info(ThreadName::direct(&me, &ann))];
        let refused = named("dm:c3d4", &threads, &me).unwrap_err();
        assert!(refused.contains("no direct thread dm:c3d4"), "{refused}");
        assert!(named(&theirs.to_string(), &threads, &me).is_err());
        assert_eq!(
            named("dm:a1", &threads, &me),
            Ok(ThreadName::direct(&me, &ann))
        );
        assert!(named("dm:", &threads, &me).is_err());
    }

    #[test]
    fn a_direct_thread_goes_by_its_name_its_label_or_its_wire_name() {
        let me = who("brett", Some("b7d0"));
        let person = who("mike", None);
        let thread = ThreadName::direct(&me, &person);
        let threads = [info(thread.clone())];
        for given in [
            "dm:mike",
            "dm:mike (direct with mike@pangolin)",
            &thread.to_string(),
        ] {
            assert_eq!(named(given, &threads, &me), Ok(thread.clone()), "{given}");
        }
        assert_eq!(direct_name(&thread, &me).unwrap(), "dm:mike");
    }

    #[test]
    fn a_start_of_two_session_ids_is_refused_and_a_whole_id_wins() {
        let me = who("brett", Some("b7d0"));
        let (a, b) = (who("mike", Some("ab12")), who("mike", Some("ab")));
        let threads = [
            info(ThreadName::direct(&me, &a)),
            info(ThreadName::direct(&me, &b)),
        ];
        assert_eq!(
            named("dm:ab", &threads, &me),
            Ok(ThreadName::direct(&me, &b))
        );
        let refused = named("dm:a", &threads, &me).unwrap_err();
        assert!(refused.contains("dm:ab12, dm:ab"), "{refused}");
        assert!(refused.contains("Give more"), "{refused}");
    }

    #[test]
    fn the_model_is_plain_data_with_serde() {
        let me = "riff://brett@heron?session=c1".parse().unwrap();
        let thread = Thread::of(&inbox(true), &me);
        let json = serde_json::to_string(&thread).unwrap();
        assert_eq!(serde_json::from_str::<Thread>(&json).unwrap(), thread);
    }
}
