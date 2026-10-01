//! `riff` waits through a restart of its server on one machine
//! (01M3TJWJ9914B7Z5EQJF310REK). `riff-server` opens its port only after
//! its load, so each connect in the gap is refused. A process that got a
//! reply from the server before tries again. A process that got no reply
//! fails at once.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use riff::api::{Api, WAITING};
use riff_core::name::SessionUri;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const ME: &str = "riff://mike@pangolin/como-technologies/riff?session=a";

/// A riff-server with no sign-in on `addr`. A send on the first value
/// stops it. The task ends when the port is closed.
async fn serve(addr: SocketAddr) -> (oneshot::Sender<()>, JoinHandle<()>, SocketAddr) {
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel::<()>();
    let served = tokio::spawn(async move {
        axum::serve(listener, riff_server::router())
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    (stop, served, addr)
}

#[tokio::test]
async fn a_process_that_got_a_reply_waits_through_a_restart() {
    let (stop, served, addr) = serve("127.0.0.1:0".parse().unwrap()).await;
    let shown = Arc::new(Mutex::new(Vec::new()));
    let lines = Arc::clone(&shown);
    let api = Api::new(&format!("http://{addr}"))
        .waits_to(move |line| lines.lock().unwrap().push(line.to_owned()));
    let me: SessionUri = ME.parse().unwrap();
    api.register(&me).await.unwrap();
    assert!(riff::api::replied(api.base()));

    // The server stops: its port is closed.
    stop.send(()).unwrap();
    served.await.unwrap();
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());

    // It starts again after 2 seconds, as after a load.
    let gap = Duration::from_secs(2);
    let again = tokio::spawn(async move {
        tokio::time::sleep(gap).await;
        serve(addr).await
    });
    let start = Instant::now();
    api.register(&me).await.expect("the call waits, then works");
    assert!(start.elapsed() >= gap, "{:?}", start.elapsed());
    let shown = shown.lock().unwrap().clone();
    assert_eq!(shown.len(), 1, "one wait line for the gap: {shown:?}");
    assert!(shown[0].contains(WAITING), "{shown:?}");
    drop(again);
}

#[tokio::test]
async fn a_process_that_got_no_reply_fails_at_once() {
    // Nothing listens on port 9.
    let api = Api::new("http://127.0.0.1:9").waits_to(|line| panic!("no wait line: {line}"));
    let me: SessionUri = ME.parse().unwrap();
    let start = Instant::now();
    let error = api.register(&me).await.unwrap_err();
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
    assert!(
        format!("{error:#}").contains("cannot reach riff-server at http://127.0.0.1:9"),
        "{error:#}"
    );
    assert!(!riff::api::replied(api.base()));
}
