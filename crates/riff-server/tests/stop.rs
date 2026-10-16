//! `riff-server` saves and exits on SIGTERM (R129).

use crate::common;

use isolated::Isolated;
use std::io::{BufRead, BufReader};
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
    let listen = common::free_address();
    let output = tempfile::NamedTempFile::new().unwrap();
    let mut server = common::Server(common::server_on_dir(dir.path(), &listen, &output));
    // The port opens after the wait for the lease.
    let span = isolated::Span::start();
    while tokio::net::TcpStream::connect(&listen).await.is_err() {
        assert!(span.within(Duration::from_secs(60)), "no open port");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
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
