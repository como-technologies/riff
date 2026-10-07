//! The time limits of the link (01M48RW9E8NS2FPHFHG2S10R7A,
//! 01M48RW9HNKPNZ75H9R01BG6V5). The server of each test is a raw TCP
//! server: it counts its accepts, and it gives a reply that a real
//! server never gives, for example no reply at all. The limits are in
//! milliseconds, so the tests run fast.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures::StreamExt;
use riff::api::{Api, Limits, follow};
use riff_core::name::SessionUri;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The longest time that a test waits for a fact.
const WAIT: Duration = Duration::from_secs(20);

/// What the raw server does after it reads a request.
#[derive(Clone, Copy)]
enum Reply {
    /// It never replies, and keeps the connection open.
    Never,
    /// It opens an event stream with the comment `ready`, then sends no
    /// more byte and keeps the connection open.
    SilentStream,
}

/// Starts a raw server. Returns its URL and the count of its accepts.
async fn raw_server(reply: Reply) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let accepts = Arc::new(AtomicUsize::new(0));
    let count = accepts.clone();
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            count.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(serve(socket, reply));
        }
    });
    (url, accepts)
}

async fn serve(mut socket: TcpStream, reply: Reply) {
    let mut request = Vec::new();
    let mut chunk = [0; 4096];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        match socket.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => request.extend_from_slice(&chunk[..n]),
        }
    }
    if let Reply::SilentStream = reply {
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n{}: {}\r\n\
             transfer-encoding: chunked\r\n\r\n",
            riff_core::build::HEADER,
            riff_core::build::VERSION,
        );
        let ready = b": ready\n\n";
        let body = format!("{:x}\r\n", ready.len());
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(body.as_bytes());
        bytes.extend_from_slice(ready);
        bytes.extend_from_slice(b"\r\n");
        if socket.write_all(&bytes).await.is_err() {
            return;
        }
    }
    // Hold the connection open until the client closes it.
    let _ = socket.read(&mut chunk).await;
    std::future::pending::<()>().await;
}

fn me() -> SessionUri {
    "riff://mike@pangolin/como-technologies/riff?session=a6cf"
        .parse()
        .unwrap()
}

fn fast() -> Limits {
    Limits {
        connect: Duration::from_millis(500),
        try_wait: Duration::from_millis(300),
        stream_idle: Duration::from_millis(300),
    }
}

/// Waits until `accepts` is at least `n`.
async fn accepts_reach(accepts: &AtomicUsize, n: usize) {
    let span = isolated::Span::start();
    while accepts.load(Ordering::SeqCst) < n {
        assert!(
            span.within(WAIT),
            "{} accepts, not {n}",
            accepts.load(Ordering::SeqCst)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A stream that gives no byte after its comment `ready` ends after
/// `stream_idle`, and `follow` connects again. Before, it waited for
/// ever.
#[tokio::test]
async fn a_stream_with_no_byte_ends_and_connects_again() {
    let (url, accepts) = raw_server(Reply::SilentStream).await;
    let api = Api::new(&url).with_limits(fast());
    let me = me();
    let mut wakes = Box::pin(follow(|| api.watch(&me), Duration::from_millis(10)));
    let read = async { while wakes.next().await.is_some() {} };
    let start = Instant::now();
    tokio::select! {
        () = read => panic!("the stream of follow never ends"),
        () = accepts_reach(&accepts, 3) => {}
    }
    // Three connects with an idle limit of 300 ms each: not before 600 ms.
    assert!(
        start.elapsed() >= Duration::from_millis(600),
        "{:?}",
        start.elapsed()
    );
}

/// A server that accepts and never replies gives a call an error after
/// the limit of the try, not a wait for ever.
#[tokio::test]
async fn a_call_to_a_server_that_never_replies_fails_after_the_limit_of_the_try() {
    let (url, accepts) = raw_server(Reply::Never).await;
    let api = Api::new(&url).with_limits(fast());
    let start = Instant::now();
    let call = isolated::in_time(WAIT, api.has_sign_in()).await;
    let error = call.expect("the call waits for ever").unwrap_err();
    assert!(
        start.elapsed() >= Duration::from_millis(300),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(
        error.to_string(),
        riff::text::no_reply(&url, Duration::from_millis(300))
    );
    assert_eq!(accepts.load(Ordering::SeqCst), 1);
}

/// Two streams of one client open two connections: the client of the
/// streams has no pool.
#[tokio::test]
async fn two_streams_of_one_client_open_two_connections() {
    let (url, accepts) = raw_server(Reply::SilentStream).await;
    let api = Api::new(&url);
    let me = me();
    let first = api.watch(&me).await.unwrap();
    let second = api.watch(&me).await.unwrap();
    accepts_reach(&accepts, 2).await;
    assert_eq!(accepts.load(Ordering::SeqCst), 2);
    drop((first, second));
}
