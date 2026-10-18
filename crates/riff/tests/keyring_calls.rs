//! riff makes few calls to the OS keyring (#1003). Each process that
//! calls it opens a session of the Secret Service, and a GNOME Keyring
//! that gets many sessions grows until it aborts. Each test runs the
//! real `riff` binary with its secrets in `RIFF_HOME`, and counts the
//! calls to the store of each process in the file of
//! `RIFF_KEYRING_LOG` (01M54510W507A3T5JRTZ55H0ZQ):
//!
//! - `riff statusline` with a kept answer, and each `riff hook` event
//!   that does not talk to riff-server, make no call
//!   (01M54510M36SZ1C3AE2C8DS585).
//! - A process that talks to riff-server opens the store one time, and
//!   reads each secret at most one time. Only the refresh of the sign-in
//!   reads it again, under its lock (01M54510PS9FDKQKJB0RAH3XG3).

use crate::book;
use crate::common;

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use common::{CLIENT, FakeProvider};
use isolated::Isolated;
use riff_core::dpop::Key;
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::{DEFAULT_DOMAIN, Provider};

const ID: &str = "a6cf2205-d54a-4c1e-9b1f-2e3d4c5b6a7f";

/// A machine: an environment with its secrets in `RIFF_HOME`, a clone
/// with a GitHub origin, and the log of the calls to the keyring store.
struct Machine {
    env: Isolated,
    clone: tempfile::TempDir,
}

impl Machine {
    fn new() -> Machine {
        let clone = tempfile::tempdir().unwrap();
        for args in [
            &["init", "-q"][..],
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/como-technologies/riff.git",
            ][..],
        ] {
            let status = Command::new("git")
                .args(args)
                .current_dir(clone.path())
                .status()
                .unwrap();
            assert!(status.success());
        }
        Machine {
            env: Isolated::new(),
            clone,
        }
    }

    fn log(&self) -> PathBuf {
        self.env.path().join("keyring-calls")
    }

    fn secrets(&self) -> PathBuf {
        self.env.riff_home().join("secrets")
    }

    /// Runs `riff ARGS` at `server` in the clone with `stdin`, and
    /// returns its process ID and its output. `RIFF_USER` is not set,
    /// so `riff` finds the user in the sign-in, as on a machine of a
    /// person.
    async fn riff(
        &self,
        server: &str,
        session: Option<&str>,
        stdin: &str,
        args: &[&str],
    ) -> (u32, std::process::Output) {
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(self.clone.path())
            .env("RIFF_SERVER", server)
            .env("RIFF_HOST", "thelio")
            .env("USER", "mike")
            .env("RIFF_KEYRING_LOG", self.log())
            .env_remove("RIFF_USER")
            .env_remove("RIFF_SESSION")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(id) = session {
            cmd.env("RIFF_SESSION", id);
        }
        let stdin = stdin.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut child = cmd.spawn().unwrap();
            let pid = child.id();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(stdin.as_bytes())
                .unwrap();
            (pid, child.wait_with_output().unwrap())
        })
        .await
        .unwrap()
    }

    /// The calls of the process `pid` to the keyring store, in order.
    fn calls(&self, pid: u32) -> Vec<String> {
        let text = std::fs::read_to_string(self.log()).unwrap_or_default();
        let prefix = format!("{pid} ");
        text.lines()
            .filter_map(|line| line.strip_prefix(&prefix))
            .map(str::to_owned)
            .collect()
    }
}

/// A riff-server with no sign-in.
async fn open_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, riff_server::router()).await.unwrap() });
    format!("http://{addr}")
}

/// A riff-server with sign-in and a provider, and the sign-in of mike
/// with a new device key in the secret files of `machine`. With
/// `expired`, the access token of the sign-in is old, so the first call
/// refreshes it.
async fn signed_in_server(machine: &Machine, expired: bool) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let issuer = FakeProvider::start("mike@comotechnologies.io", Some(DEFAULT_DOMAIN))
        .await
        .issuer;
    let service = Service::new(Config {
        require_sign_in: true,
        provider: Some(Provider {
            issuer,
            client_id: CLIENT.into(),
            client_secret: None,
            allowed_domains: vec![DEFAULT_DOMAIN.into()],
        }),
        ..Config::new(&url)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let key = Key::generate();
    let files = machine.secrets();
    riff_client::secrets::file_set(
        &files,
        &riff_client::device::secret_name(&url),
        &key.to_secret(),
    )
    .unwrap();
    let pair = service
        .admit("mike@comotechnologies.io", true, &key.thumbprint())
        .await
        .unwrap();
    let sign_in = riff_client::login::SignIn {
        user: pair.user,
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_at: if expired { 0 } else { u64::MAX },
        riff_id: service.riff_id(),
    };
    let json = serde_json::to_string(&sign_in).unwrap();
    riff_client::secrets::file_set(&files, &riff_client::login::secret_name(&url), &json).unwrap();
    url
}

/// The input of a hook or of the status line for the session [`ID`].
fn input(extra: &str) -> String {
    format!(r#"{{"session_id":"{ID}","cwd":"/x"{extra}}}"#)
}

fn names(url: &str) -> (String, String) {
    (
        riff_client::login::secret_name(url),
        riff_client::device::secret_name(url),
    )
}

#[tokio::test]
async fn the_status_line_with_a_kept_answer_calls_no_keyring() {
    let machine = Machine::new();
    let server = open_server().await;
    let (sign_in, _) = names(&server);

    let (pid, first) = machine
        .riff(&server, None, &input(""), &["statusline"])
        .await;
    assert!(first.status.success(), "{first:?}");
    // It asks riff-server: one look for the user in the sign-in, and no
    // second read of it for the client.
    assert_eq!(
        machine.calls(pid),
        ["open".to_owned(), format!("read {sign_in}")]
    );

    let (pid, kept) = machine
        .riff(&server, None, &input(""), &["statusline"])
        .await;
    assert_eq!(kept.stdout, first.stdout);
    assert_eq!(machine.calls(pid), [""; 0], "the kept answer");
}

#[tokio::test]
async fn each_hook_event_that_does_not_talk_to_riff_server_calls_no_keyring() {
    let machine = Machine::new();
    let server = open_server().await;
    let tool = r#","tool_name":"Bash","tool_input":{"command":"ls","description":"list"}"#;
    let events: [(&[&str], String); 5] = [
        (&["hook", "tool"], input(tool)),
        (&["hook", "tool", "--done"], input(tool)),
        (&["hook", "prompt"], input(r#","prompt":"hi""#)),
        (&["hook", "stop"], input("")),
        (
            &["hook", "usage", "--session", ID, "--before", "0"],
            String::new(),
        ),
    ];
    for (args, stdin) in events {
        let (pid, out) = machine.riff(&server, None, &stdin, args).await;
        assert!(out.status.success(), "{args:?}: {out:?}");
        assert_eq!(machine.calls(pid), [""; 0], "riff {args:?}");
    }

    // A control: the start hook talks to riff-server, and the log has
    // its calls.
    let (pid, out) = machine
        .riff(&server, None, &input(""), &["hook", "session-start"])
        .await;
    assert!(out.status.success(), "{out:?}");
    assert_eq!(machine.calls(pid)[0], "open");
}

#[tokio::test]
async fn a_process_that_talks_to_riff_server_reads_each_secret_one_time() {
    let machine = Machine::new();
    let server = signed_in_server(&machine, false).await;
    let (sign_in, device_key) = names(&server);

    let (pid, out) = machine.riff(&server, Some(ID), "", &["whoami"]).await;
    assert!(out.status.success(), "{out:?}");
    let shown = String::from_utf8_lossy(&out.stdout);
    assert!(shown.contains("riff://mike@thelio"), "{shown}");
    // The user, the client, the check of the riff, the person token and
    // the swap for a session token all use the one read of each secret.
    assert_eq!(
        machine.calls(pid),
        [
            "open".to_owned(),
            format!("read {sign_in}"),
            format!("read {device_key}"),
        ]
    );
}

#[tokio::test]
async fn only_the_refresh_reads_the_sign_in_again_under_its_lock() {
    let machine = Machine::new();
    let server = signed_in_server(&machine, true).await;
    let (sign_in, device_key) = names(&server);

    let (pid, out) = machine.riff(&server, Some(ID), "", &["whoami"]).await;
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        machine.calls(pid),
        [
            "open".to_owned(),
            format!("read {sign_in}"),
            format!("read {device_key}"),
            // Another process can have refreshed while this one waited
            // for the lock.
            format!("read {sign_in}"),
            format!("write {sign_in}"),
        ]
    );

    // The next process finds a live token: one read of each secret.
    let (pid, out) = machine.riff(&server, Some(ID), "", &["whoami"]).await;
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        machine.calls(pid),
        [
            "open".to_owned(),
            format!("read {sign_in}"),
            format!("read {device_key}"),
        ]
    );
}

/// The book has a how-to with `sh` blocks to count the calls.
#[test]
fn the_book_has_a_how_to_to_count_the_calls_to_the_keyring() {
    let page = book::page("development.md");
    let heading = "### Count the calls to the keyring";
    let start = page
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("the book has no {heading:?}"));
    let how = &page[start + 1..];
    let how = &how[..how[4..].find("\n### ").map_or(how.len(), |n| n + 4)];
    let commands = book::commands_in(how);
    let run = "RIFF_KEYRING_LOG=/tmp/riff-keyring-calls riff whoami";
    assert!(commands.iter().any(|c| c == run), "{how}");
    book::each_is_real(&["riff whoami".to_owned()]);
    assert!(commands.iter().any(|c| c.contains("OpenSession")), "{how}");
    assert!(commands.iter().any(|c| c.contains("VmLck")), "{how}");
    assert!(how.contains("```mermaid"), "{how}");
}

/// The log has no secret: only the names of the secrets.
#[tokio::test]
async fn the_log_of_the_calls_holds_no_secret() {
    let machine = Machine::new();
    let server = signed_in_server(&machine, true).await;
    let (pid, out) = machine.riff(&server, Some(ID), "", &["whoami"]).await;
    assert!(out.status.success(), "{out:?}");
    let log = std::fs::read_to_string(machine.log()).unwrap();
    assert!(!machine.calls(pid).is_empty(), "{log}");
    let files = machine.secrets();
    for entry in std::fs::read_dir(&files).unwrap() {
        let value = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(!log.contains(&value), "a value is in the log: {log}");
        let json: serde_json::Value = serde_json::from_str(&value).unwrap_or_default();
        for token in ["access_token", "refresh_token"] {
            if let Some(token) = json.get(token).and_then(|t| t.as_str()) {
                assert!(!log.contains(token), "{token} is in the log: {log}");
            }
        }
    }
}
