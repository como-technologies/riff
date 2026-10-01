//! The `riff-server` command of "Start a Riff" (R4) is real, and step 2
//! of the path "Just this machine" runs it in a terminal of its own
//! (01M3K0QM5HY852J4E5M2YQDYEM, 01M3MN2R92DA7QPP80G1AENX4M). The other
//! checks of the pages are in `crates/riff/tests/start_a_riff.rs` and
//! `crates/riff/tests/join_a_riff.rs`.

mod common;

use isolated::Isolated;
use std::fs;
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use riff_core::wire::WhoReply;
use serde_json::json;

const PAGE: &str = "start-a-riff.md";

/// The `riff-server` commands in the `sh` blocks of the book page `name`,
/// in order.
fn server_commands(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/src")
        .join(name);
    let page = fs::read_to_string(path).unwrap();
    let mut commands = Vec::new();
    let mut in_sh = false;
    for line in page.lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && (line == "riff-server" || line.starts_with("riff-server ")) {
            commands.push(line.to_owned());
        }
    }
    commands
}

#[test]
fn the_page_runs_the_server_in_a_terminal() {
    assert_eq!(server_commands(PAGE), ["riff-server"]);
}

#[test]
fn each_riff_server_command_of_the_page_is_real() {
    for command in server_commands(PAGE) {
        Isolated::shared()
            .assert_riff_server()
            .args(command.split_whitespace().skip(1))
            .arg("--help")
            .assert()
            .success();
    }
}

/// A `riff-server` process that stops when the test ends.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// How long the test waits for a `riff-server` that listens.
const WAIT: Duration = Duration::from_secs(60);

/// Runs `command` on a free port of the loopback address. Gives the
/// process, its listen address, and its log as a terminal keeps it.
/// Another process can take the free port before the server listens
/// there. Then the server ends, and the test starts it again on a new
/// port. The log line of the server shows that it is the server that
/// listens there.
async fn start(command: &str) -> (Server, String, tempfile::NamedTempFile) {
    let started = Instant::now();
    loop {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let listen = listener.local_addr().unwrap().to_string();
        drop(listener);
        let terminal = tempfile::NamedTempFile::new().unwrap();
        let child = Isolated::shared()
            .riff_server()
            .args(command.split_whitespace().skip(1))
            .env_clear()
            .env("RIFF_LISTEN", &listen)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(terminal.reopen().unwrap())
            .stderr(terminal.reopen().unwrap())
            .spawn()
            .unwrap();
        let mut server = Server(child);
        while server.0.try_wait().unwrap().is_none() {
            let log = fs::read_to_string(terminal.path()).unwrap();
            if log.contains(&format!("riff-server listens on {listen}"))
                && tokio::net::TcpStream::connect(&listen).await.is_ok()
            {
                return (server, listen, terminal);
            }
            assert!(
                started.elapsed() < WAIT,
                "riff-server does not listen on {listen}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(started.elapsed() < WAIT, "riff-server ends at each start");
    }
}

/// Step 2 of "Just this machine": the test runs the command of the page
/// as a process, and keeps its log as a terminal does. The test gives
/// the listen address in the environment, so that it does not take the
/// port of a real riff. The riff has no sign-in and says so in its log.
/// A session of this machine joins it with no token.
#[tokio::test]
async fn step_2_runs_the_riff_of_this_machine_in_a_terminal() {
    let command = &server_commands(PAGE)[0];
    let (_server, listen, terminal) = start(command).await;
    let base = format!("http://{listen}");

    let sign_in = common::client()
        .get(format!("{base}/v1/sign-in"))
        .send()
        .await
        .unwrap();
    assert_eq!(sign_in.status(), 404);

    let me = "riff://ada@pangolin/como-technologies/riff?session=a1";
    let register = common::client()
        .post(format!("{base}/v1/register"))
        .json(&json!({ "me": me }))
        .send()
        .await
        .unwrap();
    assert_eq!(register.status(), 200);
    let who: WhoReply = common::client()
        .post(format!("{base}/v1/who"))
        .json(&json!({ "me": me }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // The first session is the lead, as "Use it" says.
    let uris: Vec<String> = who.sessions.iter().map(|s| s.uri.to_string()).collect();
    assert_eq!(uris, [format!("{me}&lead=true")]);

    let log = fs::read_to_string(terminal.path()).unwrap();
    assert!(
        log.contains(&format!("riff-server listens on {listen}")),
        "{log}"
    );
    assert!(log.contains("nobody can sign in"), "{log}");
}
