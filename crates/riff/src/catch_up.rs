//! The read after a break of `riff tail` and `riff chat`
//! (01M49Z4E8QB1QDXVCPE6MX5JX2, 01M49Z4EB7T972BHEP6T92P574).
//!
//! A stream of a thread gives only the messages that come while it is
//! open. So at each connect, the client:
//!
//! 1. opens the stream,
//! 2. reads the thread after the last `seq` that it showed,
//! 3. shows the live messages of the stream.
//!
//! The open comes before the read, so no message falls between the two.
//! A message can then come two times, from the read and from the
//! stream: [`Shown`] drops each message with a `seq` that it showed.
//! The server keeps no cursor for it, and no `Last-Event-ID`.
//!
//! The server keeps only the last messages of a thread (200). After a
//! longer break, the read starts after a gap. Then the client shows one
//! line with the number of the lost messages ([`Seen::Lost`]).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant T as riff tail
//!     participant S as riff-server
//!     T->>S: GET /v1/tail
//!     S-->>T: message 41, message 42
//!     Note over T,S: the stream breaks
//!     T->>S: GET /v1/tail (a new connection)
//!     T->>S: read the thread after 42
//!     S-->>T: 43, 44 (sent in the break)
//!     S-->>T: live messages, with no 43 or 44 again
//! ```

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::Result;
use futures::{Stream, StreamExt};
use riff_core::name::{SessionUri, ThreadName};

use crate::api::{Api, Checked, Refusal, follow};

/// Where a client starts in a thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// Only the messages that come after the first connect: `riff tail`.
    New,
    /// Each message that the server keeps: `riff chat`.
    History,
    /// Each message after this `seq`: `riff chat` after an update.
    After(u64),
}

/// What a client shows of a thread.
#[derive(Debug)]
pub enum Seen {
    /// A message, one time.
    Message(Checked),
    /// The number of messages of a break that the server no longer
    /// keeps.
    Lost(u64),
}

/// The last `seq` that a client showed. It drops a message that it
/// showed, and counts the messages that a read did not get.
///
/// ```
/// use riff::catch_up::{Shown, Start};
///
/// // `riff tail`: the first read only finds the end of the thread.
/// let mut tail = Shown::new(Start::New);
/// assert_eq!(tail.read_after(), None);
/// tail.ends_at(Some(42));
/// assert_eq!(tail.read_after(), Some(42));
/// assert!(!tail.is_new(42), "the history does not show");
/// assert!(tail.is_new(43));
/// assert!(!tail.is_new(43), "each message shows one time");
///
/// // A break: the read after 43 starts at 300, so 256 messages are lost.
/// assert_eq!(tail.lost(300), 256);
/// assert_eq!(tail.lost(44), 0);
///
/// // `riff chat` reads the history, and loses nothing at the first read.
/// let chat = Shown::new(Start::History);
/// assert_eq!(chat.read_after(), Some(0));
/// assert_eq!(chat.lost(150), 0);
///
/// // After an update, the new chat goes on after the last line.
/// let mut again = Shown::new(Start::After(7));
/// assert_eq!((again.read_after(), again.last()), (Some(7), Some(7)));
/// assert!(!again.is_new(7));
/// assert!(again.is_new(8));
/// ```
#[derive(Debug, Clone)]
pub struct Shown {
    start: Start,
    last: Option<u64>,
}

impl Shown {
    /// Nothing shown yet.
    pub fn new(start: Start) -> Shown {
        let last = match start {
            Start::After(seq) => Some(seq),
            Start::New | Start::History => None,
        };
        Shown { start, last }
    }

    /// The last `seq` that the client showed or skipped.
    pub fn last(&self) -> Option<u64> {
        self.last
    }

    /// The `seq` after which a connect reads the thread. `None`: the
    /// first connect of [`Start::New`], which reads only to find the
    /// end of the thread ([`Shown::ends_at`]).
    pub fn read_after(&self) -> Option<u64> {
        match (self.last, self.start) {
            (Some(seq), _) => Some(seq),
            (None, Start::New) => None,
            (None, Start::History | Start::After(_)) => Some(0),
        }
    }

    /// The thread ends at `seq` (`None`: it has no message). The client
    /// shows only the messages after it.
    pub fn ends_at(&mut self, seq: Option<u64>) {
        self.last = Some(self.last.into_iter().chain(seq).max().unwrap_or(0));
    }

    /// The number of messages between the last shown message and
    /// `first`, the first message of a read. Before the first message,
    /// nothing is lost.
    pub fn lost(&self, first: u64) -> u64 {
        self.last
            .map_or(0, |last| first.saturating_sub(last.saturating_add(1)))
    }

    /// True when the message `seq` is new. It is then the last shown
    /// message.
    pub fn is_new(&mut self, seq: u64) -> bool {
        if self.last.is_some_and(|last| seq <= last) {
            return false;
        }
        self.last = Some(seq);
        true
    }
}

/// One connect to `thread`: it opens the stream, reads the thread after
/// the last message of `shown`, and gives the lost line, the messages
/// of the read and the live messages, each new message one time. The
/// stream ends at its first error. A thread that does not exist yet has
/// no message.
pub async fn connect<'a>(
    api: &'a Api,
    me: &'a SessionUri,
    thread: &'a ThreadName,
    shown: &Arc<Mutex<Shown>>,
) -> Result<impl Stream<Item = Result<Seen>> + use<'a>> {
    let live = api.tail(me, thread).await?;
    let after = lock(shown).read_after();
    let read = match api.read_after(me, thread, after.unwrap_or(0)).await {
        Ok(read) => read,
        Err(e) if is_not_found(&e) => Vec::new(),
        Err(e) => return Err(e),
    };
    let mut first = Vec::new();
    {
        let mut shown = lock(shown);
        if after.is_none() {
            shown.ends_at(read.last().map(|c| c.message.seq));
        } else {
            let lost = read.first().map_or(0, |c| shown.lost(c.message.seq));
            if lost > 0 {
                first.push(Seen::Lost(lost));
            }
            first.extend(
                read.into_iter()
                    .filter(|c| shown.is_new(c.message.seq))
                    .map(Seen::Message),
            );
        }
    }
    let shown = Arc::clone(shown);
    let live = live
        .take_while(|item| std::future::ready(item.is_ok()))
        .filter_map(move |item| {
            let seen = item
                .map(|c| {
                    lock(&shown)
                        .is_new(c.message.seq)
                        .then_some(Seen::Message(c))
                })
                .transpose();
            std::future::ready(seen)
        });
    Ok(futures::stream::iter(first.into_iter().map(Ok)).chain(live))
}

/// Each message of `thread` until stopped: [`connect`] again and again,
/// through [`follow`]. Each connect reads after the last message of
/// `shown`, so a break loses no message.
pub fn follow_thread<'a>(
    api: &'a Api,
    me: &'a SessionUri,
    thread: &'a ThreadName,
    shown: Arc<Mutex<Shown>>,
    retry: Duration,
) -> impl Stream<Item = Result<Seen>> + 'a {
    follow(
        move || {
            let shown = Arc::clone(&shown);
            async move { connect(api, me, thread, &shown).await }
        },
        retry,
    )
}

fn lock(shown: &Mutex<Shown>) -> std::sync::MutexGuard<'_, Shown> {
    shown.lock().unwrap_or_else(PoisonError::into_inner)
}

/// True for the refusal of a thread that the server does not know.
fn is_not_found(e: &anyhow::Error) -> bool {
    e.downcast_ref::<Refusal>()
        .is_some_and(|r| r.status == reqwest::StatusCode::NOT_FOUND)
}
