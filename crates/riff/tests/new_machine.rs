//! A new machine is asked about the update by itself, once
//! (01M3NT6WV8Q8EFZBK8DHYKW5CC). `riff connect claude` in a terminal
//! asks and sets `update.auto`. The second time, it does not ask. With
//! no terminal, it does not ask. A real `riff-server` with no sign-in
//! stands in for the riff, and a fake `claude` does nothing.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use isolated::Isolated;
use riff::settings::{ASK_UPDATE_AUTO, update_auto};

async fn start_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
}

/// A new machine: its own settings, data and a fake `claude`.
struct Machine {
    dir: tempfile::TempDir,
    url: String,
}

impl Machine {
    async fn new() -> Machine {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join("claude");
        let out = Command::new("sh")
            .args([
                "-c",
                "printf '#!/bin/sh\\nexit 0\\n' > \"$0\" && chmod 755 \"$0\"",
            ])
            .arg(&claude)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        Machine {
            dir,
            url: start_server().await,
        }
    }

    fn settings(&self) -> PathBuf {
        self.dir.path().join("config.toml")
    }

    /// `riff connect claude` on this machine.
    fn connect(&self) -> Command {
        let data = self.dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let mut cmd = Isolated::shared().riff();
        cmd.args(["connect", "claude", "--claude"])
            .arg(self.dir.path().join("claude"))
            .current_dir(self.dir.path())
            .env("RIFF_SERVER", &self.url)
            .env("RIFF_HOME", self.dir.path())
            .env("XDG_DATA_HOME", &data)
            .env("HOME", &data)
            .env_remove("CLAUDE_CONFIG_DIR");
        cmd
    }
}

/// Runs `cmd` in a new pseudo-terminal with `typed` as the keys of the
/// person, and returns what the terminal shows.
fn in_a_terminal(mut cmd: Command, typed: &str) -> String {
    let pty = nix::pty::openpty(None, None).unwrap();
    let slave = std::fs::File::from(pty.slave);
    let mut child = cmd
        .stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave)
        .spawn()
        .unwrap();
    // The spawn made copies: drop the ones of this process, so that the
    // read ends when the child exits.
    drop(cmd);
    let mut master = std::fs::File::from(pty.master);
    std::io::Write::write_all(&mut master, typed.as_bytes()).unwrap();
    let mut shown = Vec::new();
    let mut buf = [0u8; 4096];
    // The read fails with EIO when the child exits.
    while let Ok(n @ 1..) = master.read(&mut buf) {
        shown.extend_from_slice(&buf[..n]);
    }
    assert!(child.wait().unwrap().success(), "{shown:?}");
    String::from_utf8_lossy(&shown).into_owned()
}

fn has_key(settings: &Path) -> bool {
    std::fs::read_to_string(settings).is_ok_and(|s| s.contains("auto"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_machine_is_asked_once_in_a_terminal() {
    let machine = Machine::new().await;
    let question = ASK_UPDATE_AUTO.trim_end();

    // No terminal: no question, and no key.
    let out = tokio::task::spawn_blocking({
        let mut cmd = machine.connect();
        move || cmd.stdin(Stdio::null()).output().unwrap()
    })
    .await
    .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = format!("{out:?}");
    assert!(!text.contains(question), "{text}");
    assert!(!has_key(&machine.settings()), "{text}");

    // The first time in a terminal: the question. The person says no.
    let cmd = machine.connect();
    let shown = tokio::task::spawn_blocking(move || in_a_terminal(cmd, "n\r"))
        .await
        .unwrap();
    assert_eq!(shown.matches(question).count(), 1, "{shown}");
    assert!(shown.contains("update.auto = false"), "{shown}");
    assert!(has_key(&machine.settings()), "{shown}");
    assert!(!update_auto(&machine.settings()).unwrap());

    // The second time: no question, and the answer stays.
    let cmd = machine.connect();
    let shown = tokio::task::spawn_blocking(move || in_a_terminal(cmd, "y\r"))
        .await
        .unwrap();
    assert!(!shown.contains(question), "{shown}");
    assert!(!update_auto(&machine.settings()).unwrap());
}

/// The book has a how-to with the real question.
#[test]
fn the_book_shows_the_question() {
    let book = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let start = book
        .find("\n### A new machine asks about the update by itself\n")
        .expect("the how-to");
    let part = &book[start..];
    let part = &part[..part[5..].find("\n#").map_or(part.len(), |i| i + 5)];
    let question = format!("```text\n{}\n```", ASK_UPDATE_AUTO.trim_end());
    assert!(part.contains(&question), "{part}");
    assert!(part.contains("```sh\nriff connect claude\n```"), "{part}");
}
