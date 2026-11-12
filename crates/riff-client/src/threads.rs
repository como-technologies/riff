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
//! - [`Names`]: the names of the chat from riff-server. A chat shows
//!   each sender by the name to type to reach it: `<mike>`, `<riff>`,
//!   and `<riff-bot>` for riff-server itself.
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
//! use riff_client::threads::{Line, Names};
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
//! assert_eq!(line.sender(&Names::default()).name, "r");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use riff_core::name::{NameError, Repo, SessionUri, ThreadName, Who};
use riff_core::selector::Selector;
use riff_core::wire::{ChatName, Kind, NameKind, ThreadInfo};
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

/// The name of riff-server itself in a chat: the sender of each line of
/// [`Line::by_server`].
pub const RIFF_BOT: &str = "riff-bot";

/// The sender of a line, as a chat shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sender {
    /// The name to type to reach the sender: a person is its user
    /// (`mike`), a verified lead the name of its project (`riff`, see
    /// [`Names::project`]), and riff itself [`RIFF_BOT`]. Each other
    /// session has no name: it is `USER` and the first 8 characters of
    /// its session ID. The text is as the sender gave it: the UI makes it
    /// safe.
    pub name: String,
    /// True when `name` is a name of the chat, false for a session with
    /// no name.
    pub named: bool,
}

/// The names of the chat, as riff-server gives them
/// ([`crate::api::Api::names`], 01M6XZ27XV8RAC8B72DCM937ST). A chat
/// shows a lead by the name of its project from them. The default has no
/// name: then each project has the name after the slash.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Names(Vec<ChatName>);

impl Names {
    /// The names `names` of riff-server.
    pub fn new(names: Vec<ChatName>) -> Names {
        Names(names)
    }

    /// Each name, in the order of riff-server.
    pub fn list(&self) -> &[ChatName] {
        &self.0
    }

    /// The name of the project `owner/name` in the chat: `OWNER/REPO`
    /// when riff-server names the project so, on a clash, else `name`,
    /// the name after the slash.
    ///
    /// ```
    /// use riff_client::threads::Names;
    /// use riff_core::wire::{ChatName, NameKind};
    ///
    /// let project = |name: &str| ChatName { name: name.into(), kind: NameKind::Project, online: false };
    /// let names = Names::new(vec![project("acme/strata"), project("como-technologies/strata"), project("riff")]);
    /// assert_eq!(names.project("acme", "strata"), "acme/strata");
    /// assert_eq!(names.project("como-technologies", "riff"), "riff");
    /// // A project that riff-server did not name yet.
    /// assert_eq!(names.project("como-technologies", "kadomony"), "kadomony");
    /// assert_eq!(Names::default().project("acme", "strata"), "strata");
    /// ```
    pub fn project(&self, owner: &str, name: &str) -> String {
        let full = format!("{owner}/{name}");
        let clash = self
            .0
            .iter()
            .any(|n| n.kind == NameKind::Project && n.name.eq_ignore_ascii_case(&full));
        if clash { full } else { name.to_owned() }
    }
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

    /// True for a line of riff-server itself: the kind `riff` from the
    /// sender [`SessionUri::server`] (01M6Y4ZCTSTVM971BDC1F7GJPK).
    /// riff-server signs no line, so the sender proves it, not
    /// [`Line::verified`]: no person signs in as the user of riff-server,
    /// and riff-server refuses the kind `riff` in a post of a client. A
    /// view shows such a line with no mark `verified` or `not verified`
    /// (01M75N2T0E0TV9ZF1ED90Y3PGC).
    ///
    /// ```
    /// use riff_client::api::Checked;
    /// use riff_client::threads::Line;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::{Kind, Message};
    ///
    /// let line = |from: SessionUri, kind, body: &str| {
    ///     let message = Message {
    ///         seq: 1,
    ///         from,
    ///         to: vec![],
    ///         body: body.into(),
    ///         at_ms: 0,
    ///         kind,
    ///         sig: None,
    ///         payload: None,
    ///     };
    ///     Line::of(&Checked { message, verified: false }, &"chat".parse().unwrap())
    /// };
    /// let mike: SessionUri = "riff://mike@thelio".parse()?;
    /// assert!(line(SessionUri::server(), Kind::Riff, "riff release v3.3.0 is cut.").by_server());
    /// assert!(!line(SessionUri::server(), Kind::Message, "hi").by_server());
    /// assert!(!line(mike.clone(), Kind::Riff, "hi").by_server());
    /// assert!(!line(mike, Kind::Message, "riff release v3.3.0 waits for your approval").by_server());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn by_server(&self) -> bool {
        self.kind == Kind::Riff && self.from.is_server()
    }

    /// The sender of the line, by the name to type to reach it, with the
    /// names `names` of riff-server: a person as its user, a verified lead
    /// as the name of its project, and a line of riff-server itself
    /// ([`Line::by_server`]) as [`RIFF_BOT`]. Each other session has no
    /// name.
    ///
    /// ```
    /// use riff_client::api::Checked;
    /// use riff_client::threads::{Line, Names, Sender};
    /// use riff_core::wire::{ChatName, Kind, Message, NameKind};
    ///
    /// let line = |uri: &str, verified, kind| {
    ///     let message = Message {
    ///         seq: 1,
    ///         from: uri.parse().unwrap(),
    ///         to: vec![],
    ///         body: "hi".into(),
    ///         at_ms: 0,
    ///         kind,
    ///         sig: None,
    ///         payload: None,
    ///     };
    ///     Line::of(&Checked { message, verified }, &"chat".parse().unwrap())
    /// };
    /// let names = Names::default();
    /// let sender = |name: &str, named| Sender { name: name.into(), named };
    /// let lead = "riff://mike@thelio/como-technologies/riff?session=4e54d4e5-c891&lead=true";
    /// let said = Kind::Message;
    /// assert_eq!(line("riff://mike@thelio", true, said).sender(&names), sender("mike", true));
    /// assert_eq!(line(lead, true, said).sender(&names), sender("riff", true));
    /// // Only a line of riff-server itself shows as riff-bot: it has no signature.
    /// assert_eq!(line("riff://riff@server", false, Kind::Riff).sender(&names), sender("riff-bot", true));
    /// assert_eq!(line("riff://mike@thelio", true, Kind::Riff).sender(&names), sender("mike", true));
    /// // A lead that the reader did not prove has no name, and a worker neither.
    /// assert_eq!(line(lead, false, said).sender(&names), sender("mike 4e54d4e5", false));
    /// let worker = "riff://mike@thelio/como-technologies/riff?session=0271342b-7454&claim=issue-6";
    /// assert_eq!(line(worker, true, said).sender(&names), sender("mike 0271342b", false));
    /// // A lead outside git has no project.
    /// let nowhere = "riff://mike@thelio?session=4e54d4e5-c891&lead=true";
    /// assert_eq!(line(nowhere, true, said).sender(&names), sender("mike 4e54d4e5", false));
    /// // On a clash, the project has the name OWNER/REPO.
    /// let clash = ChatName { name: "como-technologies/riff".into(), kind: NameKind::Project, online: true };
    /// let names = Names::new(vec![clash]);
    /// assert_eq!(line(lead, true, said).sender(&names), sender("como-technologies/riff", true));
    /// ```
    pub fn sender(&self, names: &Names) -> Sender {
        let named = |name: String| Sender { name, named: true };
        let user = self.from.who().user();
        let id = match self.from.who().session() {
            _ if self.by_server() => return named(RIFF_BOT.to_owned()),
            None => return named(user.to_owned()),
            Some(id) => id,
        };
        if let (true, Repo::Git { owner, name }) = (self.lead, self.from.place().repo()) {
            return named(names.project(owner, name));
        }
        Sender {
            name: format!("{user} {}", id.chars().take(ID_CHARS).collect::<String>()),
            named: false,
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
        assert_eq!(status.sender(&Names::default()).name, "ann b7d0");
    }

    /// Only a line of riff-server itself shows as riff-bot
    /// (01M6Y4ZCTSTVM971BDC1F7GJPK). A line of a person or of a session with
    /// the words of riff-bot shows its real sender, of each kind.
    #[test]
    fn only_a_line_of_riff_server_shows_as_riff_bot() {
        let line = |from: &str, kind, body: &str| {
            let message = Message {
                seq: 1,
                from: from.parse().unwrap(),
                to: vec![],
                body: body.into(),
                at_ms: 0,
                kind,
                sig: None,
                payload: None,
            };
            let checked = Checked {
                message,
                verified: true,
            };
            Line::of(&checked, &ThreadName::chat())
        };
        let names = Names::default();
        let waits = "@mike riff release v3.3.0 waits for your approval";
        for kind in [Kind::Message, Kind::Riff] {
            let typed = line("riff://mike@thelio", kind, waits);
            assert_eq!(typed.sender(&names).name, "mike", "{kind:?}");
            let session = "riff://mike@thelio/o/r?session=0271342b-7454";
            let typed = line(session, kind, waits);
            assert_eq!(typed.sender(&names).name, "mike 0271342b", "{kind:?}");
        }
        let server = SessionUri::server().to_string();
        let passed = "@mike riff release v3.3.0 is cut.";
        let bot = line(&server, Kind::Riff, passed).sender(&names);
        assert_eq!((bot.name.as_str(), bot.named), (RIFF_BOT, true));
    }

    #[test]
    fn a_lead_that_the_reader_did_not_prove_is_no_lead() {
        let me = "riff://brett@heron?session=c1".parse().unwrap();
        let thread = Thread::of(&inbox(false), &me);
        assert!(!thread.lines[0].lead);
        assert_eq!(thread.lines[0].sender(&Names::default()).name, "mike a6cf");
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
