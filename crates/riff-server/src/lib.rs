//! `riff-server`: the central service that sessions connect to.
//!
//! Slice 1: state in memory, no sign-in.

pub mod state;

use std::convert::Infallible;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Query, State as AxumState};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::{Stream, StreamExt};
use riff_core::name::{SessionName, ThreadName};
use riff_core::wire::{
    Claim, ClaimReply, Membership, Post, Posted, Read, ReadReply, Register, Tailed, Tell, Threads,
    ThreadsReply, Wake, Who, WhoReply,
};
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::state::{Delivery, State};

/// Events that a slow stream may miss before it drops them.
const EVENT_BUFFER: usize = 1024;

type Shared = Arc<Server>;
type Reply<T> = Result<Json<T>, (StatusCode, String)>;

struct Server {
    state: Mutex<State>,
    wakes: broadcast::Sender<(SessionName, Wake)>,
    tail: broadcast::Sender<Tailed>,
}

impl Server {
    fn state(&self) -> MutexGuard<'_, State> {
        // A panic while the lock is held leaves plain data behind; keep going.
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn deliver(&self, delivery: Delivery) {
        // A send fails only when nobody listens. That is not an error.
        for wake in delivery.wakes {
            let _ = self.wakes.send(wake);
        }
        let _ = self.tail.send(delivery.tailed);
    }
}

/// The HTTP routes of `riff-server`.
pub fn router() -> Router {
    let (wakes, _) = broadcast::channel(EVENT_BUFFER);
    let (tail, _) = broadcast::channel(EVENT_BUFFER);
    let server = Arc::new(Server {
        state: Mutex::new(State::default()),
        wakes,
        tail,
    });
    Router::new()
        .route("/v1/register", post(register))
        .route("/v1/who", post(who))
        .route("/v1/threads", post(threads))
        .route("/v1/join", post(join))
        .route("/v1/leave", post(leave))
        .route("/v1/post", post(post_message))
        .route("/v1/tell", post(tell))
        .route("/v1/read", post(read))
        .route("/v1/claim", post(claim))
        .route("/v1/release", post(release))
        .route("/v1/watch", get(watch))
        .route("/v1/tail", get(tail_thread))
        .with_state(server)
}

async fn register(AxumState(s): AxumState<Shared>, Json(r): Json<Register>) -> Reply<()> {
    s.state().register(&r.name, Instant::now());
    Ok(Json(()))
}

async fn who(AxumState(s): AxumState<Shared>, Json(_): Json<Who>) -> Reply<WhoReply> {
    Ok(Json(WhoReply {
        sessions: s.state().who(),
    }))
}

async fn threads(AxumState(s): AxumState<Shared>, Json(r): Json<Threads>) -> Reply<ThreadsReply> {
    Ok(Json(ThreadsReply {
        threads: s.state().threads(&r.name),
    }))
}

async fn join(AxumState(s): AxumState<Shared>, Json(r): Json<Membership>) -> Reply<()> {
    s.state().join(&r.name, &r.thread, Instant::now());
    Ok(Json(()))
}

async fn leave(AxumState(s): AxumState<Shared>, Json(r): Json<Membership>) -> Reply<()> {
    s.state().leave(&r.name, &r.thread, Instant::now());
    Ok(Json(()))
}

async fn post_message(AxumState(s): AxumState<Shared>, Json(r): Json<Post>) -> Reply<Posted> {
    if r.thread.is_direct() {
        return Err(bad_request("use tell for direct messages".into()));
    }
    let delivery = s
        .state()
        .post(&r.from, &r.thread, r.body, Instant::now(), now_ms());
    Ok(Json(posted(&s, delivery)))
}

async fn tell(AxumState(s): AxumState<Shared>, Json(r): Json<Tell>) -> Reply<Posted> {
    let delivery = s
        .state()
        .tell(&r.from, &r.to, r.body, Instant::now(), now_ms());
    Ok(Json(posted(&s, delivery)))
}

async fn read(AxumState(s): AxumState<Shared>, Json(r): Json<Read>) -> Reply<ReadReply> {
    let messages = s
        .state()
        .read(&r.name, &r.thread, r.all, Instant::now())
        .map_err(not_found)?;
    Ok(Json(ReadReply { messages }))
}

async fn claim(AxumState(s): AxumState<Shared>, Json(r): Json<Claim>) -> Reply<ClaimReply> {
    Ok(Json(s.state().claim(
        &r.name,
        &r.thread,
        &r.item,
        Instant::now(),
    )))
}

async fn release(AxumState(s): AxumState<Shared>, Json(r): Json<Claim>) -> Reply<()> {
    s.state()
        .release(&r.name, &r.thread, &r.item, Instant::now())
        .map_err(bad_request)?;
    Ok(Json(()))
}

#[derive(Deserialize)]
struct WatchQuery {
    name: SessionName,
}

/// Streams the wakes for one session. The session is live while the
/// stream is open.
async fn watch(
    AxumState(s): AxumState<Shared>,
    Query(q): Query<WatchQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = s.wakes.subscribe();
    {
        let mut state = s.state();
        let now = Instant::now();
        state.register(&q.name, now);
        state.watch_started(&q.name, now);
    }
    let guard = WatchGuard {
        server: s.clone(),
        name: q.name.clone(),
    };
    let stream = BroadcastStream::new(rx).filter_map(move |event| {
        let _alive = &guard;
        let event = match event {
            Ok((to, wake)) if to == guard.name => Event::default().json_data(wake).ok(),
            _ => None,
        };
        std::future::ready(event.map(Ok))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Marks the session as stopped when its watch stream closes.
struct WatchGuard {
    server: Shared,
    name: SessionName,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        self.server.state().watch_ended(&self.name, Instant::now());
    }
}

#[derive(Deserialize)]
struct TailQuery {
    thread: ThreadName,
}

/// Streams each new message in one thread (R27).
async fn tail_thread(
    AxumState(s): AxumState<Shared>,
    Query(q): Query<TailQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(s.tail.subscribe()).filter_map(move |event| {
        let event = match event {
            Ok(tailed) if tailed.thread == q.thread => Event::default().json_data(tailed).ok(),
            _ => None,
        };
        std::future::ready(event.map(Ok))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn posted(s: &Server, delivery: Delivery) -> Posted {
    let reply = Posted {
        thread: delivery.tailed.thread.clone(),
        seq: delivery.tailed.message.seq,
    };
    s.deliver(delivery);
    reply
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn bad_request(message: String) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message)
}

fn not_found(message: String) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, message)
}
