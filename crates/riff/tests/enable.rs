//! riff is on only in a session that riff started
//! (01M4BYH80CFW1TBGKVA2VN9ZBQ): each entry of the plugin does nothing
//! in a session with no `RIFF_ON=1`, also with the entries of an older
//! riff in the Claude config.

use isolated::Isolated;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::ServiceExt;

const ID: &str = "a6cf2205-d54a-4c1e-9b1f-2e3d4c5b6a7f";

/// One machine of a test: a home of its own, and a `git` that logs
/// each call.
struct Machine {
    env: Isolated,
}

impl Machine {
    fn new() -> Machine {
        let machine = Machine {
            env: Isolated::new(),
        };
        std::fs::create_dir_all(machine.bin()).unwrap();
        // The real git, with a log of each call.
        let git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|dir| dir.join("git"))
            .find(|git| git.is_file())
            .expect("git on the PATH");
        let path = machine.bin().join("git");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\necho \"$*\" >> {}\nexec {} \"$@\"\n",
                machine.git_log().display(),
                git.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        machine
    }

    fn bin(&self) -> PathBuf {
        self.env.path().join("bin")
    }

    fn git_log(&self) -> PathBuf {
        self.env.path().join("git.log")
    }

    /// The calls of `git` that riff made.
    fn git_calls(&self) -> String {
        std::fs::read_to_string(self.git_log()).unwrap_or_default()
    }

    /// A git repository with a GitHub origin, and the entries of an
    /// older riff that turned the plugin on: in the user settings and
    /// in the local settings of the repository.
    fn repo(&self, name: &str) -> PathBuf {
        let dir = self.env.path().join(name);
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let origin = format!("https://github.com/acme/{name}.git");
        for args in [
            &["init", "-q"][..],
            &["remote", "add", "origin", &origin][..],
        ] {
            let git = self
                .env
                .command("git")
                .args(args)
                .current_dir(&dir)
                .status();
            assert!(git.unwrap().success());
        }
        let on = r#"{"enabledPlugins": {"riff@riff": true}}"#;
        std::fs::write(dir.join(".claude/settings.local.json"), on).unwrap();
        let user = self.env.home().join(".claude/settings.json");
        std::fs::create_dir_all(user.parent().unwrap()).unwrap();
        std::fs::write(user, on).unwrap();
        dir
    }

    /// `riff ARGS` in `dir`, with no `RIFF_ON`: a session that riff
    /// did not start.
    fn riff(&self, dir: &Path, args: &[&str]) -> Command {
        let mut cmd = self.env.riff();
        let path = std::env::var("PATH").unwrap();
        cmd.args(args)
            .current_dir(dir)
            .env("PATH", format!("{}:{path}", self.bin().display()))
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env_remove("RIFF_ON")
            .stdin(Stdio::null());
        cmd
    }
}

type Calls = Arc<Mutex<Vec<String>>>;

/// A server that answers each call with `{}` and records its path.
async fn counting_server(calls: Calls) -> String {
    let router =
        axum::Router::new()
            .fallback(|| async { "{}" })
            .layer(axum::middleware::map_request(
                move |r: axum::extract::Request| {
                    calls.lock().unwrap().push(r.uri().path().to_owned());
                    std::future::ready(r)
                },
            ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    url
}

/// `riff ARGS` in `dir` as a process of Claude Code, with `stdin`.
async fn entry(
    machine: &Machine,
    server: &str,
    dir: &Path,
    on: bool,
    stdin: &str,
    args: &[&str],
) -> Output {
    let mut cmd = machine.riff(dir, args);
    if on {
        cmd.env("RIFF_ON", "1");
    }
    cmd.env("RIFF_SERVER", server)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let stdin = stdin.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Runs each hook and the status line in `dir`, with `RIFF_ON=1` when
/// `on`, and gives what they printed.
async fn each_hook(machine: &Machine, server: &str, dir: &Path, on: bool) -> String {
    let id = format!(r#""session_id":"{ID}""#);
    let mut printed = String::new();
    for source in ["startup", "resume", "clear", "compact"] {
        let input = format!(r#"{{{id},"source":"{source}"}}"#);
        let out = entry(machine, server, dir, on, &input, &["hook", "session-start"]).await;
        assert!(out.status.success(), "{source}: {out:?}");
        printed.push_str(&stdout(&out));
    }
    let input = format!(r#"{{{id},"reason":"logout"}}"#);
    for args in [
        &["hook", "session-end"][..],
        &["hook", "stop"][..],
        &["hook", "compact", "--session", ID][..],
        &["statusline"][..],
    ] {
        let out = entry(machine, server, dir, on, &input, args).await;
        assert!(out.status.success(), "{args:?}: {out:?}");
        printed.push_str(&stdout(&out));
    }
    printed
}

/// 01M3XY2ST8R67SKTXJECAYJZRX, 01M4BYH80CFW1TBGKVA2VN9ZBQ: in a session
/// that riff did not start, no call to the server, no `git`, and no
/// output, also where an older riff turned the plugin on. With
/// `RIFF_ON=1`, the same entries act.
#[tokio::test(flavor = "multi_thread")]
async fn the_hooks_and_the_status_line_act_only_in_a_session_that_riff_started() {
    let machine = Machine::new();
    let calls = Calls::default();
    let server = counting_server(calls.clone()).await;
    let repo = machine.repo("app");

    assert_eq!(each_hook(&machine, &server, &repo, false).await, "");
    let made = calls.lock().unwrap().clone();
    assert!(
        made.is_empty(),
        "riff is off, and it sent requests: {made:?}"
    );
    assert_eq!(machine.git_calls(), "", "riff is off, and it ran git");

    let printed = each_hook(&machine, &server, &repo, true).await;
    assert!(printed.contains("riff a6cf2205"), "{printed}");
    assert!(!printed.contains("riff disable"), "{printed}");
    assert!(!calls.lock().unwrap().is_empty());
    assert_ne!(machine.git_calls(), "");
}

/// 01M3XY2ST8R67SKTXJECAYJZRX, 01M4BYH80CFW1TBGKVA2VN9ZBQ: `riff mcp`
/// has no tool in a session that riff did not start, and makes no call
/// to the server.
#[tokio::test(flavor = "multi_thread")]
async fn riff_mcp_serves_no_tool_in_a_session_that_riff_did_not_start() {
    let machine = Machine::new();
    let calls = Calls::default();
    let server = counting_server(calls.clone()).await;
    let repo = machine.repo("app");
    let mut cmd = tokio::process::Command::from(machine.riff(&repo, &["mcp"]));
    let mut child = cmd
        .env("RIFF_SERVER", &server)
        .env("RIFF_SESSION", ID)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let client = ().serve(io).await.unwrap();
    let info = client.peer_info().unwrap();
    assert_eq!(info.instructions.as_deref(), Some(riff::text::MCP_OFF));
    assert!(riff::text::MCP_OFF.contains("`riff`"));
    assert!(info.capabilities.tools.is_none(), "{info:?}");
    let tools = client.list_all_tools().await.unwrap_or_default();
    assert!(tools.is_empty(), "{tools:?}");
    // It ends when the agent tool closes the stream, with no end call.
    client.cancel().await.unwrap();
    let ended = isolated::in_time(Duration::from_secs(20), child.wait()).await;
    assert!(ended.is_ok(), "riff mcp did not end");
    let made = calls.lock().unwrap().clone();
    assert!(
        made.is_empty(),
        "riff is off, and it sent requests: {made:?}"
    );
    assert_eq!(machine.git_calls(), "");
}

