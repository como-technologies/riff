//! #2157: `riff host-session` starts `claude` on the host as the person,
//! with the riff tools and the wakes, for the lead of the repository
//! (01M7GTEYVRA3549J3PCA0DG22R, 01M7GTF5EE7KN7PZX0YYP537C8,
//! 01M7GTF5GZYET0F5R0KTGT59C1). A fake `claude` on `PATH` keeps its
//! arguments and its environment, and runs `riff who`. A real
//! `riff-server` with sign-in holds the lead. No test reaches GitHub or
//! the keyring.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use isolated::{Isolated, Span};
use tokio::io::AsyncBufReadExt;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// The fake `claude`: it keeps each argument, the variables of a
/// session and the output of `riff who`, and exits with 7.
const CLAUDE: &str = "#!/bin/sh\ndir=$(dirname \"$0\")\n\
for arg in \"$@\"; do printf '%s\\n--\\n' \"$arg\"; done > \"$dir/args\"\n\
printf 'RIFF_SESSION=%s\\nRIFF_HOST_SESSION=%s\\nRIFF_SOCKET=%s\\nRIFF_SERVER=%s\\n' \
\"$RIFF_SESSION\" \"$RIFF_HOST_SESSION\" \"${RIFF_SOCKET-none}\" \"$RIFF_SERVER\" > \"$dir/env\"\n\
\"$RIFF_TEST_BIN\" who > \"$dir/who\" 2>&1\n\
exit 7\n";

/// One host: a clone of como-technologies/riff with a fake `claude`, at
/// a riff-server where the repository is a project.
struct Host {
    env: Isolated,
    root: tempfile::TempDir,
    server: crate::common::Signed,
}

impl Host {
    async fn new() -> Host {
        let root = tempfile::tempdir().unwrap();
        let clone = root.path().join("riff");
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "-q", "-b", "main"]);
        git(
            &clone,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/como-technologies/riff.git",
            ],
        );
        std::fs::create_dir_all(root.path().join("bin")).unwrap();
        isolated::write_program(root.path().join("bin/claude"), CLAUDE);
        Host {
            env: Isolated::new(),
            root,
            server: crate::common::start_signed_project().await,
        }
    }

    fn clone(&self) -> PathBuf {
        self.root.path().join("riff")
    }

    fn bin(&self) -> PathBuf {
        self.root.path().join("bin")
    }

    /// A file that the fake `claude` wrote, or an empty text.
    fn kept(&self, name: &str) -> String {
        std::fs::read_to_string(self.bin().join(name)).unwrap_or_default()
    }

    /// Each argument of the fake `claude`.
    fn args(&self) -> Vec<String> {
        let kept = self.kept("args");
        let mut args: Vec<String> = kept.split("\n--\n").map(str::to_owned).collect();
        args.pop();
        args
    }

    /// `riff ARGS` as mike on thelio, in the clone, with the fake
    /// `claude` first in `PATH`, and `vars` in its environment.
    fn command(&self, vars: &[(&str, &str)], args: &[&str]) -> Command {
        crate::common::sign_in(&self.server, &self.env.riff_home(), "mike");
        let path = format!(
            "{}:{}",
            self.bin().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(self.clone())
            .env("RIFF_SERVER", &*self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "thelio")
            .env("RIFF_TEST_BIN", self.env.riff_path())
            .env("PATH", path)
            .env_remove("CLAUDECODE")
            .stdin(Stdio::null());
        for (key, value) in vars {
            cmd.env(key, value);
        }
        cmd
    }

    async fn run(&self, vars: &[(&str, &str)], args: &[&str]) -> Output {
        let mut cmd = self.command(vars, args);
        tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap()
    }

    /// The session `id` of mike in the clone becomes the lead.
    async fn lead(&self, id: &str) {
        let out = self.run(&[("RIFF_SESSION", id)], &["whoami"]).await;
        assert!(out.status.success(), "{}", text(&out.stderr));
        crate::common::lead_by_person(&self.server, &self.clone(), "thelio", "mike", id).await;
    }

    /// Each secret in the home of riff: no argument of `claude` holds
    /// one.
    fn secrets(&self) -> Vec<String> {
        let dir = self.env.riff_home().join("secrets");
        let mut secrets = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let file = std::fs::read_to_string(entry.path()).unwrap_or_default();
            match serde_json::from_str::<serde_json::Value>(&file) {
                Ok(value) => strings(&value, &mut secrets),
                Err(_) => secrets.push(file.trim().to_owned()),
            }
        }
        secrets.retain(|s| s.len() >= 16);
        assert!(!secrets.is_empty(), "the home of riff has no secret");
        secrets
    }
}

/// Each string in `value`.
fn strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(list) => list.iter().for_each(|v| strings(v, out)),
        serde_json::Value::Object(map) => map.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

/// 01M7GTEYVRA3549J3PCA0DG22R: `riff host-session` and `riff host
/// session` refuse a session in a container, the lead in its container,
/// an agent session on the host and a worker, before each call:
/// riff-server gets no call, the fake daemon no ask, and `claude` does
/// not start.
#[tokio::test]
async fn host_session_refuses_each_session_and_the_lead() {
    let host = Host::new().await;
    let socket = host.root.path().join("run/riff.sock");
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let socket = socket.to_string_lossy().into_owned();
    host.server.calls().lock().unwrap().clear();
    let cases: [&[(&str, &str)]; 4] = [
        // A worker or a verifier in a container.
        &[("RIFF_SOCKET", &socket), ("RIFF_SESSION", "c1")],
        // The lead in its container: claude with the socket of its session.
        &[
            ("RIFF_SOCKET", &socket),
            ("RIFF_SESSION", "l1"),
            ("CLAUDECODE", "1"),
        ],
        // An agent session on the host.
        &[("CLAUDECODE", "1")],
        &[("RIFF_WORKER", "1"), ("RIFF_SESSION", "w1")],
    ];
    for vars in cases {
        for args in [&["host-session"][..], &["host", "session"]] {
            let out = host.run(vars, args).await;
            assert_eq!(out.status.code(), Some(1), "{vars:?} {args:?}");
            assert_eq!(
                text(&out.stderr),
                format!("{}\n", riff::text::HOST_SESSION_REFUSED),
                "{vars:?} {args:?}"
            );
        }
    }
    assert!(host.kept("args").is_empty(), "claude started");
    assert!(
        listener.accept().is_err(),
        "riff host-session asked the daemon"
    );
    assert!(
        host.server.calls().lock().unwrap().is_empty(),
        "riff host-session called riff-server"
    );
}

/// 01M7GTEYVRA3549J3PCA0DG22R: `riff host-session` starts `claude` as
/// the person, with a new session ID, the first prompt and the riff MCP
/// server, and with no token, no model, no permission mode and no
/// settings of riff. The session has the role `host session` in `riff
/// who` while it runs (01M7GTF5GZYET0F5R0KTGT59C1). The lead gets the
/// session ID at the start and at the end, and riff exits with the exit
/// code of `claude`.
#[tokio::test]
async fn host_session_starts_claude_as_the_person_with_the_riff_tools() {
    let host = Host::new().await;
    host.lead("1ead0001-0000-4000-8000-000000000000").await;
    let out = host.run(&[], &["host-session"]).await;
    assert_eq!(out.status.code(), Some(7), "{}", text(&out.stderr));

    let args = host.args();
    assert_eq!(args[0], "--session-id", "{args:?}");
    let id = args[1].clone();
    assert_eq!(id.len(), 36, "{id}");
    assert!(
        args[2].starts_with(
            "You are the host session of mike on thelio, for the repository \
             como-technologies/riff."
        ),
        "{}",
        args[2]
    );
    assert!(args[2].contains("The lead of como-technologies/riff is mike@thelio:riff (1ead0001)."));
    assert_eq!(args[3], "--mcp-config");
    assert_eq!(args.len(), 5, "{args:?}");
    let config: serde_json::Value = serde_json::from_str(&args[4]).unwrap();
    let riff = &config["mcpServers"]["riff"];
    assert_eq!(
        riff["command"],
        host.env.riff_path().to_string_lossy().as_ref()
    );
    assert_eq!(riff["args"], serde_json::json!(["relay", "mcp"]));
    assert_eq!(riff["env"][riff::host_session::VAR], id.as_str());
    for flag in [
        "--model",
        "--permission-mode",
        "--settings",
        "--dangerously-skip-permissions",
    ] {
        assert!(!args.iter().any(|a| a == flag), "{flag}: {args:?}");
    }
    let all = args.join("\n");
    for secret in host.secrets() {
        assert!(
            !all.contains(&secret),
            "an argument of claude holds a secret"
        );
    }

    let env = host.kept("env");
    assert!(env.contains(&format!("RIFF_SESSION={id}\n")), "{env}");
    assert!(env.contains(&format!("RIFF_HOST_SESSION={id}\n")), "{env}");
    assert!(env.contains("RIFF_SOCKET=none\n"), "{env}");
    assert!(
        env.contains(&format!("RIFF_SERVER={}\n", &*host.server)),
        "{env}"
    );

    let who = host.kept("who");
    let row = who
        .lines()
        .find(|line| line.contains(&format!("({})", &id[..8])))
        .unwrap_or_else(|| panic!("riff who has no host session: {who}"));
    assert!(row.contains("you host session"), "{row}");
    // The keep-alive of riff host-session makes it live.
    assert!(!row.contains("offline"), "{row}");

    let read = host
        .run(
            &[("RIFF_SESSION", "1ead0001-0000-4000-8000-000000000000")],
            &["read"],
        )
        .await;
    let read = text(&read.stdout);
    assert!(
        read.contains(&format!("mike started the host session {id} on thelio.")),
        "{read}"
    );
    assert!(
        read.contains(&format!("the host session {id} of mike on thelio ended.")),
        "{read}"
    );

    // The session is gone at once after the end of claude.
    let after = host.run(&[], &["who"]).await;
    assert!(
        !text(&after.stdout).contains(&id[..8]),
        "{}",
        text(&after.stdout)
    );
    let line = text(&out.stdout);
    assert!(line.contains("riff host-session: the session"), "{line}");
}

/// 01M7GTF5EE7KN7PZX0YYP537C8: in a session of `riff host-session`,
/// `riff relay watch` gets the wake of a message of the lead from
/// riff-server, with no socket and no poll.
#[tokio::test]
async fn the_watch_of_a_host_session_gets_the_wake_of_the_lead() {
    let host = Host::new().await;
    let lead_id = "1ead0002-0000-4000-8000-000000000000";
    host.lead(lead_id).await;
    let id = "5e55a0b2-0000-4000-8000-000000000000";
    let vars = [("RIFF_SESSION", id), (riff::host_session::VAR, id)];
    let mut watch = tokio::process::Command::from(host.command(&vars, &["relay", "watch"]));
    let mut child = watch
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    // The lead tells the session when its watch is open.
    let api = host.server.api("mike", Some(lead_id));
    let lead: riff_core::name::SessionUri =
        format!("riff://mike@thelio/como-technologies/riff?session={lead_id}")
            .parse()
            .unwrap();
    let span = Span::start();
    loop {
        let sessions = api.who(&lead, false).await.unwrap();
        if sessions
            .iter()
            .any(|s| s.uri.who().session() == Some(id) && s.live)
        {
            break;
        }
        assert!(
            span.within(Duration::from_secs(10)),
            "the watch did not open"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    api.tell(&lead, id, "the journal of riff.service, please")
        .await
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .expect("no wake in time")
        .unwrap()
        .unwrap_or_default();
    assert!(
        line.starts_with("riff: mike@thelio:riff (1ead0002) wrote to you in a direct message"),
        "{line}"
    );
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("riff relay watch did not end")
        .unwrap();
    assert!(status.success());
}

/// 01M7GTF5EE7KN7PZX0YYP537C8: in a session of `riff host-session`,
/// `riff relay mcp` serves the riff tools that only talk, as the
/// session, with the words of a host session.
#[tokio::test]
async fn the_mcp_server_of_a_host_session_serves_the_tools_that_talk() {
    use rmcp::ServiceExt;
    let host = Host::new().await;
    let id = "5e55a0b3-0000-4000-8000-000000000000";
    let vars = [("RIFF_SESSION", id), (riff::host_session::VAR, id)];
    let mut cmd = tokio::process::Command::from(host.command(&vars, &["relay", "mcp"]));
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let client = ().serve(io).await.unwrap();
    let info = client.peer_info().unwrap();
    assert_eq!(
        info.instructions.as_deref(),
        Some(riff::text::MCP_HOST_SESSION)
    );
    let mut names: Vec<String> = client
        .list_all_tools()
        .await
        .unwrap()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    names.sort();
    assert_eq!(names, riff::tools::CONTAINER_TOOLS);
    let whoami = client
        .call_tool(rmcp::model::CallToolRequestParams::new("whoami".to_owned()))
        .await
        .unwrap();
    let said: String = whoami
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect();
    assert!(
        said.contains(&format!(
            "riff://mike@thelio/como-technologies/riff?session={id}"
        )),
        "{said}"
    );
    client.cancel().await.unwrap();
}
