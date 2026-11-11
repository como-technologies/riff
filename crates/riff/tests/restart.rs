//! `riff` waits through a restart of its server on one machine
//! (01M3TJWJ9914B7Z5EQJF310REK). `riff-server` opens its port only after
//! its load, so each connect in the gap is refused. A process that got a
//! reply from the server before tries again. A process that got no reply
//! fails at once.
//!
//! This file is a test binary of its own, and no test of it starts a
//! child process (01M752WCFNXA7PEWM6X4FVDVYH). macOS has no
//! `SOCK_CLOEXEC`: tokio opens a socket, and then marks it close-on-exec.
//! A child that another test starts between the two steps gets a copy of
//! the listening socket. The socket of the stopped server then still
//! listens in that child, and a connect to its port does not fail.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use riff_client::api::Api;
use riff_client::link::WAITING;
use riff_core::name::SessionUri;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const ME: &str = "riff://mike@pangolin/como-technologies/riff?session=a";

/// A riff-server with no sign-in on `addr` (not common::start_signed:
/// the test stops it and starts it again on the same port). A send on
/// the first value stops it. The task ends when the port is closed.
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
    let (line, line_shown) = oneshot::channel::<()>();
    let (lines, line) = (Arc::clone(&shown), Mutex::new(Some(line)));
    let api = Api::new(&format!("http://{addr}")).waits_to(move |text| {
        lines.lock().unwrap().push(text.to_owned());
        if let Some(line) = line.lock().unwrap().take() {
            let _ = line.send(());
        }
    });
    let me: SessionUri = ME.parse().unwrap();
    api.register(&me).await.unwrap();
    assert!(api.link().replied());

    // The server stops: its port is closed.
    stop.send(()).unwrap();
    served.await.unwrap();
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());

    // It starts again 2 seconds after the wait line, as after a load. The
    // waits of the call bring the line, and not the time of its tries. So
    // the test waits for the line, not for a time
    // (01M41A0M2XWCWTWGF7T9DR03W0).
    let after = Duration::from_secs(2);
    let again = tokio::spawn(async move {
        line_shown.await.unwrap();
        tokio::time::sleep(after).await;
        serve(addr).await
    });
    let start = Instant::now();
    api.register(&me).await.expect("the call waits, then works");
    assert!(start.elapsed() >= after, "{:?}", start.elapsed());
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
    let span = isolated::Span::start();
    let error = api.register(&me).await.unwrap_err();
    assert!(span.within(Duration::from_secs(2)), "{:?}", span.wall());
    assert!(
        format!("{error:#}").contains("cannot reach riff-server at http://127.0.0.1:9"),
        "{error:#}"
    );
    assert!(!api.link().replied());
}

/// No test of this binary starts a child process
/// (01M752WCFNXA7PEWM6X4FVDVYH). This test is the last item of the
/// file, and the code above it names no way to start one.
#[test]
fn no_test_of_this_binary_starts_a_child() {
    let source = include_str!("restart.rs");
    let (above, _) = source.split_once("/// No test of this binary").unwrap();
    for word in ["Command", "Isolated", "process::", "rerun"] {
        assert!(!above.contains(word), "restart.rs names {word}");
    }
}
