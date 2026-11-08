//! `riff-server` saves and exits on SIGTERM (R129).

use crate::common;

use isolated::Isolated;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::time::Duration;

use riff_server::tools::verify;
use riff_state::store::Dir;
use riff_state::{checkpoint, log};
use serde_json::{Value, json};

const MIKE: &str = "riff://mike@pangolin/como-technologies/riff?session=a#api";
const BRETT: &str = "riff://brett@heron/como-technologies/riff?session=b#tests";
const REPO: &str = "como-technologies/riff";

/// Sends SIGTERM to `server`, and waits for its exit.
fn terminate(server: &mut Child) -> ExitStatus {
    let pid = server.id().to_string();
    let kill = Command::new("kill").args(["-TERM", &pid]).status().unwrap();
    assert!(kill.success());
    let span = isolated::Span::start();
    loop {
        if let Some(status) = server.try_wait().unwrap() {
            return status;
        }
        assert!(
            span.within(Duration::from_secs(10)),
            "riff-server did not stop"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn sigterm_stops_the_server_with_success() {
    let mut cmd = Isolated::shared().riff_server();
    let mut server = cmd
        .args(["--listen", "127.0.0.1:0"])
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(server.stdout.take().unwrap()).lines();
    while !lines.next().unwrap().unwrap().contains("listens on") {}

    let status = terminate(&mut server);
    assert!(status.success(), "{status}");
    let rest: Vec<String> = lines.map_while(Result::ok).collect();
    assert!(
        rest.iter().any(|l| l.contains("saving the state")),
        "{rest:?}"
    );
}

/// A call that is still open at SIGTERM does not hold the exit
/// (01M6YP76C4592AA973ZDFSJX77). The client sends the head of a call
/// with `expect: 100-continue`, gets `100 Continue` when the server
/// reads the body, and then sends only a part of the body. The server
/// exits with success, and the call gets no reply.
#[test]
fn sigterm_ends_the_server_with_success_while_a_call_is_open() {
    let log = tempfile::NamedTempFile::new().unwrap();
    let mut server = Isolated::shared()
        .riff_server()
        .env("RIFF_LISTEN", common::LISTEN)
        .stdin(Stdio::null())
        .stdout(log.reopen().unwrap())
        .stderr(log.reopen().unwrap())
        .spawn()
        .unwrap();
    let listen = common::listen_address(&mut server, &log);
    let mut call = TcpStream::connect(&listen).unwrap();
    call.set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let head = format!(
        "POST /v1/register HTTP/1.1\r\nhost: riff\r\n{}: {}\r\n\
         content-type: application/json\r\ncontent-length: 1000\r\n\
         expect: 100-continue\r\n\r\n",
        riff_core::build::HEADER,
        riff_core::build::Build::this(),
    );
    call.write_all(head.as_bytes()).unwrap();
    let mut read = Vec::new();
    while !read.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        assert_eq!(call.read(&mut byte).unwrap(), 1, "{read:?}");
        read.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&read);
    assert!(text.starts_with("HTTP/1.1 100 Continue\r\n"), "{text}");
    call.write_all(br#"{"me":"riff://mike@pangolin"#).unwrap();

    let status = terminate(&mut server);
    let log = fs::read_to_string(log.path()).unwrap();
    assert!(status.success(), "{status}: {log}");
    let mut rest = Vec::new();
    match call.read_to_end(&mut rest) {
        Ok(_) => assert!(rest.is_empty(), "{}", String::from_utf8_lossy(&rest)),
        Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::ConnectionReset, "{e}"),
    }
}

/// Calls `op` and returns the reply. The call must succeed.
async fn call(base: &str, op: &str, body: Value) -> Value {
    let reply = common::client()
        .post(format!("{base}/v1/{op}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200, "{op}");
    reply.json().await.unwrap()
}

/// The real `riff-server` with a directory gets SIGTERM. It writes a
/// last checkpoint at the last position of its log
/// (01M4264028A3KVDK10PPERHM0C). The next start loads it, replays no
/// record, and each read cursor is where it was (#873).
#[tokio::test]
async fn sigterm_writes_a_checkpoint_that_the_next_start_loads_with_each_read_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let output = tempfile::NamedTempFile::new().unwrap();
    let mut server = common::Server(common::server_on_dir(dir.path(), common::LISTEN, &output));
    let listen = common::listen_address(&mut server.0, &output);
    // The server serves after the wait for the lease.
    common::wait_until_it_serves(&listen).await;
    let base = format!("http://{listen}");
    call(&base, "register", json!({ "me": MIKE })).await;
    call(&base, "register", json!({ "me": BRETT })).await;
    let mike = "riff://mike@pangolin/como-technologies/riff";
    call(&base, "resume", json!({ "me": mike, "riff": true })).await;
    for body in ["one", "two"] {
        let post = json!({ "me": MIKE, "thread": REPO, "body": body });
        call(&base, "post", post).await;
    }
    let read = call(&base, "read", json!({ "me": BRETT, "thread": REPO })).await;
    assert_eq!(read["messages"].as_array().unwrap().len(), 2, "{read}");
    let to_brett = json!([{ "user": "brett" }]);
    let post = json!({ "me": MIKE, "thread": "design", "to": to_brett, "body": "look" });
    call(&base, "post", post).await;

    let status = terminate(&mut server.0);
    assert!(status.success(), "{status}");
    let store = Dir::new(dir.path());
    let last = verify(&store).await.unwrap().last.unwrap();
    let names = checkpoint::names(&store).await.unwrap();
    assert_eq!(names.last().map(|n| n.0), Some(last), "{names:?}");
    // The next start loads that checkpoint, and replays no record.
    let after = log::replay_after(&store, last).await.unwrap();
    assert!(after.records.is_empty(), "{:?}", after.records);

    let (_new, base) = common::start_on(Arc::new(Dir::new(dir.path()))).await;
    let read = call(&base, "read", json!({ "me": BRETT, "thread": REPO })).await;
    assert_eq!(read["messages"], json!([]), "{read}");
    let read = call(&base, "read", json!({ "me": BRETT, "thread": "design" })).await;
    assert_eq!(read["messages"][0]["body"], "look", "{read}");
}
