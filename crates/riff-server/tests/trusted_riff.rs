//! `riff-server` with no sign-in trusts its network (R211). It marks
//! each `read` reply as trusted, also when it listens on the network.
//! It listens on the network only with `--insecure`
//! (01M3JCE4ZD4DZCQ21FA69RT52D).

mod common;

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Post, Read, ReadReply, Register};

/// Kills the server when the test ends.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `riff-server` with these arguments and no settings from the
/// environment.
fn server(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_riff-server"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("RIFF_") {
            cmd.env_remove(name);
        }
    }
    cmd.args(args).env("NO_COLOR", "1");
    cmd
}

/// Starts `riff-server` with these arguments. Returns the server, the
/// port that it listens on, and the log up to that line.
fn serve(args: &[&str]) -> (Server, u16, String) {
    let mut child = server(args).stdout(Stdio::piped()).spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let server = Server(child);
    let mut lines = BufReader::new(stdout).lines().map_while(Result::ok);
    let mut log = String::new();
    let line = loop {
        let line = lines.next().expect("the server says that it listens");
        log.push_str(&line);
        log.push('\n');
        if line.contains("riff-server listens on") {
            break line;
        }
    };
    // Read the rest of the log, so that the server can write it.
    std::thread::spawn(move || lines.for_each(drop));
    let port = line.rsplit(':').next().unwrap().trim().parse().unwrap();
    (server, port, log)
}

/// Posts one message to the repository thread and reads it, through
/// the loopback address.
async fn read_reply(port: u16) -> ReadReply {
    let url = format!("http://127.0.0.1:{port}/v1");
    let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a1"
        .parse()
        .unwrap();
    let thread: ThreadName = "como-technologies/riff".parse().unwrap();
    let http = common::client();
    let call =
        |op: &str, body: serde_json::Value| http.post(format!("{url}/{op}")).json(&body).send();
    let register = Register { me: me.clone() };
    call("register", serde_json::to_value(register).unwrap())
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let post = Post::new(&me, Some(thread.clone()), vec![], "ready");
    call("post", serde_json::to_value(post).unwrap())
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let read = Read {
        me,
        thread,
        all: true,
    };
    call("read", serde_json::to_value(read).unwrap())
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn a_server_with_no_sign_in_trusts_its_callers() {
    for args in [
        &["--listen", "127.0.0.1:0"][..],
        &["--listen", "0.0.0.0:0", "--insecure"][..],
    ] {
        let (_server, port, _) = serve(args);
        let reply = read_reply(port).await;
        assert_eq!(reply.messages.len(), 1);
        assert!(reply.trusted, "{args:?}");
    }
}

#[test]
fn with_no_sign_in_a_network_address_needs_insecure() {
    let out = server(&["--listen", "0.0.0.0:0"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--insecure"), "{stderr}");
    assert!(stderr.contains("0.0.0.0:0"), "{stderr}");
}

#[test]
fn insecure_warns_at_start() {
    let (_server, _, log) = serve(&["--listen", "0.0.0.0:0", "--insecure"]);
    assert!(log.contains("WARN"), "{log}");
    assert!(log.contains("answer as any person"), "{log}");
}

#[test]
fn loopback_needs_no_flag_and_gives_no_warning() {
    let (_server, _, log) = serve(&["--listen", "127.0.0.1:0"]);
    assert!(!log.contains("any person"), "{log}");
}

#[test]
fn a_server_that_requires_sign_in_listens_anywhere_with_no_flag() {
    let (_server, _, log) = serve(&["--listen", "0.0.0.0:0", "--require-sign-in"]);
    assert!(!log.contains("any person"), "{log}");
}
