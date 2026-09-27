//! `riff-server` with no sign-in trusts its network (R211). It marks
//! each `read` reply as trusted, also when it listens on the network.

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

/// Starts `riff-server --listen ADDR` with no settings from the
/// environment. Returns the server and the port that it listens on.
fn serve(listen: &str) -> (Server, u16) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_riff-server"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("RIFF_") {
            cmd.env_remove(name);
        }
    }
    let mut child = cmd
        .args(["--listen", listen])
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let server = Server(child);
    let mut lines = BufReader::new(stdout).lines().map_while(Result::ok);
    let line = lines.find(|l| l.contains("listens on")).unwrap();
    // Read the rest of the log, so that the server can write it.
    std::thread::spawn(move || lines.for_each(drop));
    let port = line.rsplit(':').next().unwrap().trim().parse().unwrap();
    (server, port)
}

/// Posts one message to the repository thread and reads it, through
/// the loopback address.
async fn read_reply(port: u16) -> ReadReply {
    let url = format!("http://127.0.0.1:{port}/v1");
    let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a1"
        .parse()
        .unwrap();
    let thread: ThreadName = "como-technologies/riff".parse().unwrap();
    let http = reqwest::Client::new();
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
    for listen in ["127.0.0.1:0", "0.0.0.0:0"] {
        let (_server, port) = serve(listen);
        let reply = read_reply(port).await;
        assert_eq!(reply.messages.len(), 1);
        assert!(reply.trusted, "{listen}");
    }
}
