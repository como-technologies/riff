//! The reads of messages: one page of one thread ([`State::page`]), and
//! the read with no thread ([`State::pages`]). Both use one rule for
//! the messages that a read shows, and the unread counts of
//! [`State::threads_of`] use it too:
//!
//! - A read of all shows each kept message after its start.
//! - An unread read shows each message after the read cursor, but the
//!   own posts of the reader (01M3JPK82PN4F706MCHDH771MW).
//! - An unread read of a lead shows of the thread of another repository
//!   only the messages that pick the lead: a message that woke it, or a
//!   message with a selector in its `to` that matches it now
//!   (01M76SBCXR4A21D49WVKBGWMRY). Of each other thread, it shows each
//!   message: the thread of its repository, the chat, `release`, its
//!   direct threads, and each thread whose name is not `OWNER/REPO`.
//!
//! A read with no thread has a bound: [`READ_CHARS`] characters in all,
//! as [`read_len`] counts them (01M76SB9ACMZWMY9Z6CC4WW3HD). Each thread
//! with a message to show gets its lines first. Then the messages that
//! woke the reader get their room, oldest first in each thread. The
//! pages take the room that is left, thread by thread. A page stops at
//! [`PAGE`] messages, or at the first message with no room. Then it has
//! [`ThreadPage::next`], and the read cursor stays before the first
//! message that it did not show.

use std::collections::BTreeSet;
use std::time::Instant;

use riff_core::name::{ThreadName, Who};
use riff_core::wire::{Message, ThreadPage};

use super::threads::{Stored, Thread};
use super::view::View;
use super::{PAGE, Page, Signal, State, may_read};

/// A read with no thread gives at most this many characters in all, as
/// [`read_len`] counts them (01M76SB9ACMZWMY9Z6CC4WW3HD).
pub const READ_CHARS: usize = 20_000;

/// The characters of a read around its threads, at most: the note at
/// its start and the head of the part of the wakes.
pub const READ_HEAD: usize = 400;

/// The characters of the lines of one thread in a read, at most, with
/// no name and no message: its label in the pages and in the part of
/// the wakes, and the line of its next page.
pub const THREAD_LINES: usize = 300;

/// The characters of one line of a read, at most, with no sender, no
/// selector and no body: the seq, the marks and the words of the kind.
pub const MESSAGE_HEAD: usize = 64;

/// The characters that a read shows of `message`, at most: the head of
/// a line, the URI of the sender, each selector of `to`, and the body.
/// The line of a read shows a shorter sender than the URI.
///
/// ```
/// use riff_core::wire::Message;
/// use riff_state::state::read::{MESSAGE_HEAD, shown_len};
///
/// let message = Message {
///     seq: 1,
///     from: "riff://ann@heron".parse()?,
///     to: vec!["user=bob".parse().unwrap()],
///     body: "héllo".into(),
///     at_ms: 0,
///     kind: Default::default(),
///     sig: None,
///     payload: None,
/// };
/// assert_eq!(shown_len(&message), MESSAGE_HEAD + 16 + (8 + 4) + 5);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn shown_len(message: &Message) -> usize {
    let to: usize = message
        .to
        .iter()
        .map(|selector| selector.to_string().chars().count() + " or ".len())
        .sum();
    MESSAGE_HEAD + chars(&message.from.to_string()) + to + chars(&message.body)
}

/// The characters of the lines of `thread` in a read, at most, with no
/// message.
pub fn thread_lines(thread: &ThreadName) -> usize {
    THREAD_LINES + chars(&thread.to_string())
}

/// The characters of a read with no thread, as the bound counts them:
/// [`READ_HEAD`], the lines of each thread, and each message.
///
/// ```
/// use riff_state::state::read::{READ_HEAD, read_len, thread_lines};
///
/// assert_eq!(read_len(&[]), READ_HEAD);
/// let page = riff_core::wire::ThreadPage {
///     thread: "chat".parse()?,
///     members: vec![],
///     messages: vec![],
///     next: Some(0),
///     woken: vec![],
/// };
/// assert_eq!(read_len(&[page]), READ_HEAD + thread_lines(&"chat".parse()?));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn read_len(pages: &[ThreadPage]) -> usize {
    let lines = |page: &ThreadPage| {
        let messages = page.messages.iter().chain(&page.woken);
        thread_lines(&page.thread) + messages.map(shown_len).sum::<usize>()
    };
    READ_HEAD + pages.iter().map(lines).sum::<usize>()
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

/// True when `stored` picks `who`: it woke `who`, or a selector of its
/// `to` matches `who` now. A name stands for what it names
/// ([`View::named`]). A note wakes nobody, so only its `to` picks.
fn picks(view: &View<'_>, who: &Who, stored: &Stored, now: Instant) -> bool {
    if stored.woken.contains(who) {
        return true;
    }
    let uri = view.uri(who, now);
    stored
        .message
        .to
        .iter()
        .any(|selector| match &selector.name {
            Some(name) => view.named(name).is_some_and(|s| s.matches(&uri)),
            None => selector.matches(&uri),
        })
}

/// True when `who` is a lead, and `thread` is the thread of another
/// repository than its own (01M76SBCXR4A21D49WVKBGWMRY).
fn another_repository(view: &View<'_>, who: &Who, thread: &ThreadName, now: Instant) -> bool {
    thread.is_repository()
        && view.place(who).default_thread().as_ref() != Some(thread)
        && view.is_lead(who, now)
}

/// The rule of a read of `who` in `thread`: true for each message that
/// the read shows, when it comes after the start of the read.
fn shows<'a>(
    view: &'a View<'a>,
    who: &'a Who,
    thread: &ThreadName,
    all: bool,
    now: Instant,
) -> impl Fn(&Stored) -> bool + 'a {
    let picked_only = !all && another_repository(view, who, thread, now);
    move |m: &Stored| {
        all || (m.message.from.who() != who && (!picked_only || picks(view, who, m, now)))
    }
}

/// The room of a read with no thread: the characters that are left, and
/// each message that woke the reader and has its room, by thread and
/// seq.
struct Room {
    left: usize,
    kept: BTreeSet<(ThreadName, u64)>,
}

impl Room {
    /// The characters that `m` of `thread` takes from the room: none when
    /// it has its room.
    fn cost(&self, thread: &ThreadName, m: &Stored) -> usize {
        if self.kept.contains(&(thread.clone(), m.message.seq)) {
            0
        } else {
            shown_len(&m.message)
        }
    }
}

impl State {
    /// The unread count of `who` in `thread` for [`State::threads_of`]:
    /// each message that an unread read shows.
    pub(super) fn unread(&self, who: &Who, thread: &ThreadName, t: &Thread, now: Instant) -> usize {
        let view = self.written_view();
        let shown = shows(&view, who, thread, false, now);
        let read = self.presence.cursor(who, thread);
        t.messages
            .iter()
            .filter(|m| m.message.seq > read && shown(m))
            .count()
    }

    /// The page of [`State::read_page`] for the session `who`, and the
    /// signal that moves its read cursor to the end of the page. It
    /// changes nothing. A direct thread of two other sessions is not
    /// found ([`may_read`]).
    pub fn page(
        &self,
        who: &Who,
        thread: &ThreadName,
        all: bool,
        after: Option<u64>,
        limit: usize,
        now: Instant,
    ) -> Result<(Page, Option<Signal>), String> {
        let not_found = || format!("no thread named {thread}");
        if !may_read(who, thread) {
            return Err(not_found());
        }
        let threads = &self.written.threads().by_name;
        let t = threads.get(thread).ok_or_else(not_found)?;
        Ok(self.page_of(who, thread, t, all, after, limit, None, now))
    }

    /// The read with no thread of the session `who`: one page of each
    /// thread that `who` joined and that has a message to show, each
    /// direct thread first (01M58AG7Z2PETHBZG7X9QD8PK5), with
    /// [`READ_CHARS`] characters in all (01M76SB9ACMZWMY9Z6CC4WW3HD).
    /// Gives the signals that move the read cursors. It changes nothing.
    /// See [`State::read_threads`].
    pub fn pages(&self, who: &Who, all: bool, now: Instant) -> (Vec<ThreadPage>, Vec<Signal>) {
        let view = self.written_view();
        let start = |thread: &ThreadName| {
            if all {
                0
            } else {
                self.presence.cursor(who, thread)
            }
        };
        let mut threads: Vec<(&ThreadName, &Thread)> = self
            .written
            .threads()
            .by_name
            .iter()
            .filter(|(thread, t)| t.members.contains(who) && may_read(who, thread))
            .filter(|(thread, t)| {
                let (from, shown) = (start(thread), shows(&view, who, thread, all, now));
                t.messages.iter().any(|m| m.message.seq > from && shown(m))
            })
            .collect();
        threads.sort_by_key(|(thread, _)| !thread.is_direct());
        let lines: usize = threads.iter().map(|(thread, _)| thread_lines(thread)).sum();
        let mut room = Room {
            left: READ_CHARS.saturating_sub(READ_HEAD + lines),
            kept: BTreeSet::new(),
        };
        // The messages that woke the reader get their room first
        // (01M588J4XNV1TXEPMGTKRVY9VK), oldest first in each thread.
        for (thread, t) in threads.iter().filter(|_| !all) {
            let seen = self.presence.seen_woken(who, thread);
            let woke = t.messages.iter().filter(|m| {
                m.message.seq > seen && m.woken.contains(who) && m.message.from.who() != who
            });
            for m in woke {
                let cost = shown_len(&m.message);
                if cost > room.left {
                    break;
                }
                room.left -= cost;
                room.kept.insert(((*thread).clone(), m.message.seq));
            }
        }
        let mut pages = Vec::new();
        let mut reads = Vec::new();
        for (thread, t) in threads {
            let (page, read) = self.page_of(who, thread, t, all, None, PAGE, Some(&mut room), now);
            reads.extend(read);
            pages.push(ThreadPage {
                thread: thread.clone(),
                members: t.members.iter().map(|m| view.uri(m, now)).collect(),
                messages: page.messages,
                next: page.next,
                woken: page.woken,
            });
        }
        (pages, reads)
    }

    /// One page of `t`, the thread `thread`, for `who`: at most `limit`
    /// messages, and in a read with no thread only those with room. The
    /// signal moves the read cursor to the end of the page.
    #[allow(clippy::too_many_arguments)]
    fn page_of(
        &self,
        who: &Who,
        thread: &ThreadName,
        t: &Thread,
        all: bool,
        after: Option<u64>,
        limit: usize,
        mut room: Option<&mut Room>,
        now: Instant,
    ) -> (Page, Option<Signal>) {
        let view = self.written_view();
        let shown = shows(&view, who, thread, all, now);
        let cursor = self.presence.cursor(who, thread);
        let from = if all { after.unwrap_or(0) } else { cursor };
        let mut messages: Vec<Message> = Vec::new();
        let mut more = false;
        for m in t
            .messages
            .iter()
            .filter(|m| m.message.seq > from && shown(m))
        {
            let cost = room.as_deref().map_or(0, |r| r.cost(thread, m));
            let no_room = room.as_deref().is_some_and(|r| cost > r.left);
            if messages.len() >= limit.max(1) || no_room {
                more = true;
                break;
            }
            if let Some(r) = room.as_deref_mut() {
                r.left -= cost;
            }
            messages.push(m.message.clone());
        }
        let next = more.then(|| messages.last().map_or(from, |m| m.seq));
        // An unread page also gives each message after it that woke the
        // reader and that no read showed (01M588J4XNV1TXEPMGTKRVY9VK). In
        // a read with no thread, only those with room.
        let seen = self.presence.seen_woken(who, thread);
        let has_room = |m: &Stored| {
            room.as_deref()
                .is_none_or(|r| r.kept.contains(&(thread.clone(), m.message.seq)))
        };
        let woken: Vec<Message> = match next {
            Some(end) if !all => t
                .messages
                .iter()
                .filter(|m| m.message.seq > end.max(seen) && m.woken.contains(who))
                .filter(|m| m.message.from.who() != who && has_room(m))
                .map(|m| m.message.clone())
                .collect(),
            _ => Vec::new(),
        };
        // A read of all marks each message up to the last one that it
        // returns, also the ones before `after`. A read of all that
        // returns none marks none (01M6WBQQEGZKVV49QWCQGTE4CY).
        let read = if all {
            messages.last().map(|last| Signal::Read {
                thread: thread.clone(),
                seq: last.seq,
                all,
                woken: 0,
            })
        } else {
            t.messages.back().map(|last| Signal::Read {
                thread: thread.clone(),
                seq: next.unwrap_or(last.message.seq),
                all,
                woken: woken.last().map_or(0, |m| m.seq),
            })
        };
        let page = Page {
            messages,
            next,
            woken,
        };
        (page, read)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Instant;

    use riff_core::name::{SessionUri, ThreadName, Who};
    use riff_core::wire::{Kind, Message, Post, ThreadPage};

    use super::{READ_CHARS, read_len};
    use crate::state::State;

    fn uri(text: &str) -> SessionUri {
        text.parse().unwrap()
    }

    fn thread(text: &str) -> ThreadName {
        text.parse().unwrap()
    }

    /// A post of `kind` of `me` to `thread`, or a direct message with no
    /// thread, to the selectors `to`.
    fn post(
        state: &mut State,
        me: &SessionUri,
        thread: Option<&ThreadName>,
        to: &[&str],
        (kind, body): (Kind, &str),
    ) {
        let to = to.iter().map(|s| s.parse().unwrap()).collect();
        let post = Post {
            kind,
            ..Post::new(me, thread.cloned(), to, body)
        };
        state.post(post, Instant::now(), 0).unwrap();
    }

    fn message(body: &str) -> (Kind, &str) {
        (Kind::Message, body)
    }

    /// The unread count of `who` in each of its threads.
    fn unread(state: &State, who: &Who) -> BTreeMap<ThreadName, usize> {
        let threads = state.threads_of(who, Instant::now());
        threads.into_iter().map(|t| (t.thread, t.unread)).collect()
    }

    /// The bodies of the messages of the page of `thread`.
    fn bodies<'a>(pages: &'a [ThreadPage], thread: &ThreadName) -> Vec<&'a str> {
        let page = pages.iter().find(|p| p.thread == *thread);
        let messages: &[Message] = page.map_or(&[], |p| &p.messages);
        messages.iter().map(|m| m.body.as_str()).collect()
    }

    /// 300 messages of 180 characters in four threads: 150 in the thread
    /// of the repository of the reader, 98 in the chat, 49 in the thread
    /// of another repository, and 3 direct messages that wake the reader.
    #[test]
    fn a_read_with_no_thread_over_300_messages_gives_at_most_20000_characters() {
        let now = Instant::now();
        let mut state = State::default();
        let bob = uri("riff://bob@kite/acme/app?session=b1");
        let ann = uri("riff://ann@heron/acme/app?session=a1");
        let (app, chat, web) = (thread("acme/app"), ThreadName::chat(), thread("acme/web"));
        state.register(&bob, now);
        state.join(&bob, &chat, now);
        state.join(&bob, &web, now);
        let mut n = 0;
        let mut body = || {
            n += 1;
            format!("{n:03} {}", "x".repeat(176))
        };
        for i in 0..297 {
            let thread = match i % 6 {
                0..=2 => &app,
                3 | 4 => &chat,
                _ => &web,
            };
            post(&mut state, &ann, Some(thread), &[], message(&body()));
            if i % 99 == 98 {
                post(&mut state, &ann, None, &["session=b1"], message(&body()));
            }
        }
        let direct = ThreadName::direct(ann.who(), bob.who());
        let before = unread(&state, bob.who());
        assert_eq!(before.values().sum::<usize>(), 300, "{before:?}");

        let pages = state.read_threads(&bob, false, now);
        assert!(read_len(&pages) <= READ_CHARS, "{}", read_len(&pages));
        let shown: usize = pages
            .iter()
            .flat_map(|p| p.messages.iter().chain(&p.woken))
            .map(|m| m.body.chars().count())
            .sum();
        assert!(shown > READ_CHARS / 2, "the read uses its room: {shown}");
        // The direct thread comes first, with each message that woke bob.
        assert_eq!(pages[0].thread, direct);
        assert_eq!(pages[0].messages.len(), 3);
        // The read names each thread with more messages, also a thread
        // with no room for a message.
        assert_eq!(pages.len(), before.len());
        for page in &pages {
            let more = page.messages.len() < before[&page.thread];
            assert_eq!(page.next.is_some(), more, "{}", page.thread);
        }
        let chat_page = pages.iter().find(|p| p.thread == chat).unwrap();
        assert_eq!((chat_page.messages.len(), chat_page.next), (0, Some(0)));

        // The next reads go on where each page stopped: each message
        // comes once, in its order.
        let mut read: BTreeMap<ThreadName, Vec<u64>> = BTreeMap::new();
        let mut pages = pages;
        while !pages.is_empty() {
            assert!(read_len(&pages) <= READ_CHARS, "{}", read_len(&pages));
            for page in &pages {
                let seqs = page.messages.iter().map(|m| m.seq);
                read.entry(page.thread.clone()).or_default().extend(seqs);
            }
            pages = state.read_threads(&bob, false, now);
        }
        for (thread, seqs) in &read {
            let each: Vec<u64> = (1..=before[thread] as u64).collect();
            assert_eq!(seqs, &each, "{thread}");
        }
        assert_eq!(unread(&state, bob.who()).values().sum::<usize>(), 0);
    }

    /// The lead of mike in acme/app joined the thread of acme/web. Of the
    /// posts of brett there, only those that pick the lead are unread for
    /// it: a message that woke it, and a note whose `to` matches it. A
    /// worker of mike there gets each of them.
    #[test]
    fn a_lead_gets_no_unread_message_of_another_repository_unless_its_to_picks_it() {
        let now = Instant::now();
        let mut state = State::default();
        let lead = uri("riff://mike@thelio/acme/app?session=l1");
        let worker = uri("riff://mike@thelio/acme/app?session=w1");
        let brett = uri("riff://brett@heron/acme/web?session=b1");
        let (app, web) = (thread("acme/app"), thread("acme/web"));
        let (chat, release) = (ThreadName::chat(), riff_core::hold::thread());
        for me in [&lead, &worker, &brett] {
            state.register(me, now);
        }
        state.lead(&lead, now).unwrap();
        for me in [&lead, &worker] {
            for thread in [&web, &chat, &release] {
                state.join(me, thread, now);
            }
        }
        let note = |body| (Kind::Note, body);
        let posts = [
            (&web, vec![], message("board of web")),
            (
                &web,
                vec!["user=brett,repo=acme/web,lead=true"],
                message("verify request to the lead of brett"),
            ),
            (&web, vec!["repo=acme/web"], note("news of web")),
            (
                &web,
                vec!["user=mike,lead=true"],
                note("to the lead of mike"),
            ),
            (&web, vec!["session=l1"], message("a question to l1")),
            (&app, vec![], message("news of app")),
            (&chat, vec![], message("hello")),
            (&release, vec![], message("the release waits")),
        ];
        for (thread, to, kind) in posts {
            post(&mut state, &brett, Some(thread), &to, kind);
        }
        post(&mut state, &brett, None, &["session=l1"], message("direct"));
        let direct = ThreadName::direct(brett.who(), lead.who());

        let counts = unread(&state, lead.who());
        let want = [
            (&app, 1),
            (&web, 2),
            (&chat, 1),
            (&release, 1),
            (&direct, 1),
        ];
        let want: BTreeMap<ThreadName, usize> =
            want.into_iter().map(|(t, n)| (t.clone(), n)).collect();
        assert_eq!(counts, want);
        assert_eq!(unread(&state, worker.who())[&web], 5);

        let pages = state.read_threads(&lead, false, now);
        assert_eq!(
            bodies(&pages, &web),
            ["to the lead of mike", "a question to l1"]
        );
        assert_eq!(bodies(&pages, &app), ["news of app"]);
        assert_eq!(bodies(&pages, &chat), ["hello"]);
        assert_eq!(bodies(&pages, &release), ["the release waits"]);
        assert_eq!(bodies(&pages, &direct), ["direct"]);
        assert!(state.read_threads(&lead, false, now).is_empty());
        // The worker is no lead: it reads each message of web.
        let worker_pages = state.read_threads(&worker, false, now);
        assert_eq!(bodies(&worker_pages, &web).len(), 5);
        // A read of the thread with all gives each kept message.
        assert_eq!(state.read(&lead, &web, true, now).unwrap().len(), 5);
    }

    /// An unread read of the thread of another repository by name gives
    /// a lead only the messages that pick it, as a read with no thread.
    #[test]
    fn an_unread_read_by_name_gives_a_lead_only_the_messages_that_pick_it() {
        let now = Instant::now();
        let mut state = State::default();
        let lead = uri("riff://mike@thelio/acme/app?session=l1");
        let brett = uri("riff://brett@heron/acme/web?session=b1");
        let web = thread("acme/web");
        state.register(&lead, now);
        state.lead(&lead, now).unwrap();
        state.join(&lead, &web, now);
        post(&mut state, &brett, Some(&web), &[], message("board of web"));
        post(
            &mut state,
            &brett,
            Some(&web),
            &["lead=true"],
            message("each lead"),
        );
        let read = state.read(&lead, &web, false, now).unwrap();
        let bodies: Vec<&str> = read.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, ["each lead"]);
    }
}
