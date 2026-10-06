//! `riff-server` runs each command with a call ID one time only
//! (01M48VFX22S4811DYBBD7QDW24). A second try of the same call gets the
//! reply of the first, with the header `riff-repeat`.

mod common;

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use riff_core::name::SessionUri;
use riff_core::record::Record;
use riff_core::wire::{
    CALL_HEADER, Claim, Post, REPEAT_HEADER, Read, ReadReply, Register, Release, ReleaseReply,
    Resume,
};
use riff_server::checkpoint;
use riff_server::engine::{Authenticated, Engine, Failed, Open, Replied, Routed};
use riff_server::log::{Timing, write};
use riff_server::state::{CALL_KEEP, Code, Command, State};
use riff_server::store::Memory;

fn now_ms() -> u64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    u64::try_from(since.as_millis()).unwrap()
}

/// An engine and its writer, which the test drives: each chunk is
/// written only at [`Writer::write`]. The writer keeps each record that
/// it wrote: the log.
struct Writer {
    engine: Engine,
    store: Memory,
    log: Vec<Record>,
}

impl Writer {
    fn new() -> Writer {
        Writer::of(State::with_writer(Instant::now(), now_ms()))
    }

    fn of(state: State) -> Writer {
        Writer {
            engine: Engine::new(state, Open),
            store: Memory::default(),
            log: Vec::new(),
        }
    }

    /// Writes each chunk of the queue, and finishes it.
    async fn write(&mut self) {
        while let Some(chunk) = self.engine.take() {
            let records = chunk.records();
            let written = write(&self.store, &records, &Timing::default(), || true)
                .await
                .unwrap();
            self.log.extend(records);
            self.engine.finish(chunk, written);
        }
    }

    /// The call of `command`, with the call ID `call`.
    fn call<C: Routed>(&self, command: C, call: &str) -> Authenticated<C> {
        let call_of = self.engine.authenticate(None, command).unwrap();
        call_of.with_call(Some(call.to_owned()))
    }

    /// Sends a call, and lets it go into the queue: the try waits for
    /// the writer.
    async fn send<C>(
        &self,
        call: Authenticated<C>,
    ) -> tokio::task::JoinHandle<Result<Replied<C::Reply>, Failed>>
    where
        C: Command,
        C::Reply: 'static,
    {
        let engine = self.engine.clone();
        let task = tokio::spawn(async move { engine.run(call).await });
        tokio::task::yield_now().await;
        task
    }

    /// Sends a call, writes its records, and gives its reply.
    async fn run<C>(&mut self, call: Authenticated<C>) -> Result<Replied<C::Reply>, Failed>
    where
        C: Command,
        C::Reply: 'static,
    {
        let task = self.send(call).await;
        self.write().await;
        task.await.unwrap()
    }

    /// Each record of the log of this kind.
    fn count(&self, kind: &str) -> usize {
        self.log
            .iter()
            .filter(|record| record.change.kind() == kind)
            .count()
    }
}

fn lead() -> SessionUri {
    "riff://mike@pangolin/como-technologies/riff?session=lead"
        .parse()
        .unwrap()
}

fn worker() -> SessionUri {
    "riff://mike@pangolin/como-technologies/riff?session=w1"
        .parse()
        .unwrap()
}

fn claim(me: &SessionUri) -> Claim {
    let thread = me.default_thread().unwrap();
    Claim {
        me: me.clone(),
        thread,
        item: "issue-12".into(),
    }
}

fn release(me: &SessionUri) -> Release {
    let thread = me.default_thread().unwrap();
    Release {
        me: me.clone(),
        thread,
        item: "issue-12".into(),
    }
}

/// A running riff with a lead, and a worker that holds `issue-12`.
async fn worker_holds() -> Writer {
    let mut w = Writer::new();
    let (lead, worker) = (lead(), worker());
    let register = Register {
        me: lead.clone(),
        worker: false,
    };
    w.run(w.call(register, "r1")).await.unwrap();
    w.run(w.call(Resume::whole(lead), "r2")).await.unwrap();
    let register = Register {
        me: worker.clone(),
        worker: true,
    };
    w.run(w.call(register, "r3")).await.unwrap();
    w.run(w.call(claim(&worker), "r4")).await.unwrap();
    w
}

/// A `release` whose reply is cut after the write, then a second try
/// with the same call ID: one `released` record, and the reply of the
/// first try with `must_clear`.
#[tokio::test]
async fn a_second_try_after_a_cut_gets_the_reply_of_the_first_and_writes_nothing() {
    let mut w = worker_holds().await;
    let worker = worker();

    let first = w.send(w.call(release(&worker), "c1")).await;
    w.write().await;
    // The cut: the try is gone after the write, with its reply.
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert_eq!(w.count("released"), 1);

    let second = w.run(w.call(release(&worker), "c1")).await.unwrap();
    let must_clear = ReleaseReply { must_clear: true };
    assert_eq!(
        second,
        Replied {
            reply: must_clear,
            repeat: true
        }
    );
    assert_eq!(w.count("released"), 1);
    let released = w
        .log
        .iter()
        .find(|r| r.change.kind() == "released")
        .unwrap();
    assert_eq!(released.call.as_deref(), Some("c1"));

    // The same release with no call ID runs `handle`: the item is free.
    let no_id = w.engine.authenticate(None, release(&worker)).unwrap();
    let task = w.send(no_id).await;
    w.write().await;
    let Err(Failed::Refused(refused)) = task.await.unwrap() else {
        panic!("a second release with no call ID is refused");
    };
    assert_eq!(refused.code, Code::NotHolder);
}

/// A second try that comes while the first waits for the writer gets
/// the same reply, and `handle` runs one time
/// (01M48VFX8K93F8XRWDB2BDP240).
#[tokio::test]
async fn a_second_try_while_the_first_waits_gets_the_same_reply() {
    let mut w = worker_holds().await;
    let worker = worker();

    let first = w.send(w.call(release(&worker), "c1")).await;
    let second = w.send(w.call(release(&worker), "c1")).await;
    // A second `handle` would see the item free in the pending copy,
    // and refuse it with `not_holder`.
    w.write().await;
    let first = first.await.unwrap().unwrap();
    let second = second.await.unwrap().unwrap();
    assert_eq!(first.reply, ReleaseReply { must_clear: true });
    assert!(!first.repeat);
    assert_eq!(second.reply, first.reply);
    assert!(second.repeat);
    assert_eq!(w.count("released"), 1);
    assert!(w.engine.take().is_none());
}

/// A post with no signature, sent two times with one call ID, makes
/// one message.
#[tokio::test]
async fn an_unsigned_post_sent_two_times_with_one_call_id_makes_one_message() {
    let mut w = worker_holds().await;
    let lead = lead();
    let thread = lead.default_thread();
    let post = || Post::new(&lead, thread.clone(), Vec::new(), "hello");

    let first = w.run(w.call(post(), "p1")).await.unwrap();
    let second = w.run(w.call(post(), "p1")).await.unwrap();
    let json = |posted| serde_json::to_value(posted).unwrap();
    assert_eq!(json(&second.reply), json(&first.reply));
    assert!(!first.repeat && second.repeat);
    assert_eq!(w.count("posted"), 1);

    // Another call ID is another message.
    w.run(w.call(post(), "p2")).await.unwrap();
    assert_eq!(w.count("posted"), 2);
}

/// A refused command keeps no key: a second try that the state now
/// accepts runs `handle` (01M48VFXBGBW3PTC2JNHYNSE0W).
#[tokio::test]
async fn a_refused_call_keeps_no_key_and_a_second_try_runs_handle() {
    let mut w = Writer::new();
    let lead = lead();
    let register = Register {
        me: lead.clone(),
        worker: false,
    };
    w.run(w.call(register, "r1")).await.unwrap();

    // A new riff is paused: the claim is refused.
    let Err(Failed::Refused(refused)) = w.run(w.call(claim(&lead), "c1")).await else {
        panic!("a claim in a paused riff is refused");
    };
    assert_eq!(refused.code, Code::Paused);

    w.run(w.call(Resume::whole(lead.clone()), "r2"))
        .await
        .unwrap();
    let second = w.run(w.call(claim(&lead), "c1")).await.unwrap();
    assert!(!second.repeat);
    assert_eq!(w.count("claimed"), 1);
}

/// The reply to a repeated `release` of a state that the log gives.
async fn repeat_release(state: State) -> Replied<ReleaseReply> {
    let mut w = Writer::of(state);
    w.run(w.call(release(&worker()), "c1")).await.unwrap()
}

/// A start from a checkpoint and a start from the full log give the
/// same reply to a repeated call (01M48VFXHHND8SX4DBXZTFMJGQ).
#[tokio::test]
async fn a_start_from_a_checkpoint_and_from_the_full_log_give_the_same_repeat() {
    let mut w = worker_holds().await;
    let worker = worker();
    let before = w.log.len();
    w.run(w.call(release(&worker), "c1")).await.unwrap();
    let post = Post::new(&worker, worker.default_thread(), Vec::new(), "bye");
    w.run(w.call(post, "p1")).await.unwrap();
    let log = w.log.clone();
    let (now, ms) = (Instant::now(), now_ms());

    // The full log.
    let full = State::replay(log.clone(), now, ms);
    // A checkpoint after the release, through its JSON.
    let snapshot = full.snapshot(now, ms);
    let encoded = checkpoint::encode(&checkpoint::Checkpoint::new("1.1.0", ms, snapshot));
    let decoded = checkpoint::decode(&encoded).unwrap();
    let from_checkpoint = State::load(Some(decoded.state), [], now, ms);
    // A checkpoint before the release, and the records after it.
    let early = State::replay(log[..before].to_vec(), now, ms).snapshot(now, ms);
    let from_early = State::load(Some(early), log[before..].to_vec(), now, ms);

    let must_clear = ReleaseReply { must_clear: true };
    let expected = Replied {
        reply: must_clear,
        repeat: true,
    };
    for state in [full, from_checkpoint, from_early] {
        assert_eq!(repeat_release(state).await, expected);
    }

    // After CALL_KEEP, a load keeps no call: the try runs `handle`.
    let later = ms + u64::try_from(CALL_KEEP.as_millis()).unwrap() + 1;
    let old = State::replay(log, now, later);
    let mut w = Writer::of(old);
    let Err(Failed::Refused(refused)) = w.run(w.call(release(&worker), "c1")).await else {
        panic!("a release of a free item is refused");
    };
    assert_eq!(refused.code, Code::NotHolder);
}

/// Over HTTP: the header `riff-call` gives the call ID, and the reply to
/// a repeated post has the header `riff-repeat: 1`.
#[tokio::test]
async fn over_http_a_repeated_post_has_the_header_riff_repeat() {
    let (_service, url) = common::start(false, &[]).await;
    let http = common::client();
    let me = lead();
    let thread = me.default_thread().unwrap();
    let register = Register {
        me: me.clone(),
        worker: false,
    };
    let reply = http
        .post(format!("{url}/v1/register"))
        .json(&register)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200);

    let post = Post::new(&me, Some(thread.clone()), Vec::new(), "hello");
    let send = || {
        http.post(format!("{url}/v1/post"))
            .header(CALL_HEADER, "Zm9vYmFy")
            .json(&post)
            .send()
    };
    let first = send().await.unwrap();
    assert_eq!(first.status(), 200);
    assert!(first.headers().get(REPEAT_HEADER).is_none());
    let first: serde_json::Value = first.json().await.unwrap();
    let second = send().await.unwrap();
    assert_eq!(second.status(), 200);
    assert_eq!(second.headers()[REPEAT_HEADER], "1");
    let second: serde_json::Value = second.json().await.unwrap();
    assert_eq!(second, first);

    let read = Read {
        me: me.clone(),
        thread,
        all: true,
        after: None,
    };
    let reply: ReadReply = http
        .post(format!("{url}/v1/read"))
        .json(&read)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let hellos = reply
        .messages
        .iter()
        .filter(|message| message.body == "hello")
        .count();
    assert_eq!(hellos, 1);

    // A call ID that is too long is refused.
    let reply = http
        .post(format!("{url}/v1/post"))
        .header(CALL_HEADER, "x".repeat(129))
        .json(&post)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 400);
}
