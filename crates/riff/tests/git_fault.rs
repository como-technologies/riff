//! A fault of `git` does not make a place (#1401,
//! 01M5R9S69W7QEMNKV3JRNV0RNB). `riff whoami` of a session, and then
//! `riff lead --session ID` of its person, are the race of #1401: the
//! second call finds the session only when the first call registered it
//! in the place of its repository. A `git` that fails once made the
//! first call register the session in a place that riff made up.

use isolated::Isolated;
use std::path::{Path, PathBuf};
use std::process::Command as Git;

use crate::common;

/// A git repository with a GitHub origin, so the default thread is
/// `como-technologies/riff`.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ],
    ] {
        assert!(
            Git::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
    }
    dir
}

/// The `git` of the machine, as the first `git` in `PATH`.
fn real_git() -> PathBuf {
    let path = std::env::var("PATH").unwrap();
    std::env::split_paths(&path)
        .map(|dir| dir.join("git"))
        .find(|git| git.is_file())
        .expect("a git in PATH")
}

/// A folder with a `git` that counts its calls, and fails the call
/// that `fail` names. Then it runs the real `git`.
struct Flaky {
    dir: tempfile::TempDir,
}

impl Flaky {
    fn new() -> Flaky {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            r#"#!/bin/sh
dir=$(dirname "$0")
n=$(cat "$dir/calls" 2>/dev/null || echo 0)
n=$((n + 1))
echo "$n" > "$dir/calls"
fail=$(cat "$dir/fail" 2>/dev/null)
if [ "$fail" = all ] || [ "$fail" = "$n" ]; then
  echo "fatal: boom, call $n" >&2
  exit 1
fi
exec {} "$@"
"#,
            real_git().display()
        );
        isolated::write_program(dir.path().join("git"), body);
        Flaky { dir }
    }

    /// Fails the call number `which` (1, 2, ...) or `all` calls, and
    /// counts from 0 again.
    fn fail(&self, which: &str) {
        std::fs::write(self.dir.path().join("fail"), which).unwrap();
        std::fs::write(self.dir.path().join("calls"), "0").unwrap();
    }

    /// The calls of `git` since the last [`Flaky::fail`].
    fn calls(&self) -> u32 {
        let text = std::fs::read_to_string(self.dir.path().join("calls")).unwrap();
        text.trim().parse().unwrap()
    }

    /// The `PATH` that finds this `git` first.
    fn path(&self) -> String {
        format!(
            "{}:{}",
            self.dir.path().display(),
            std::env::var("PATH").unwrap()
        )
    }
}

/// What a `riff` command printed and its exit code.
struct Ran {
    stdout: String,
    stderr: String,
    code: i32,
}

/// Runs `riff` in `dir` as the user mike: as the session `session`, or
/// as the person when it is `None`. With `flaky`, its `git` is first
/// in `PATH`.
async fn riff(
    server: &str,
    dir: &Path,
    session: Option<&str>,
    flaky: Option<&Flaky>,
    args: &[&str],
) -> Ran {
    common::sign_in(server, dir, "mike");
    let mut cmd = Isolated::shared().assert_riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("RIFF_HOME", dir);
    if let Some(id) = session {
        cmd.env("RIFF_SESSION", id);
    }
    if let Some(flaky) = flaky {
        cmd.env("PATH", flaky.path());
    }
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    Ran {
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
        code: out.status.code().unwrap(),
    }
}

/// The race of #1401, on purpose: the call number `n` of `git` fails in
/// `riff whoami`, and then the person names the session the lead.
/// Before the fix, a failed call of `git` for the place made `whoami`
/// register the session in a made-up place, and `lead` found no session.
#[tokio::test]
async fn the_lead_finds_a_session_whose_whoami_lost_any_one_git_call() {
    let server = common::start_signed().await;
    let dir = repo();
    let dir = dir.path();
    let flaky = Flaky::new();

    // How many calls of git does `riff whoami` make?
    flaky.fail("none");
    let probe = riff(&server, dir, Some("probe"), Some(&flaky), &["whoami"]).await;
    assert_eq!(probe.code, 0, "{}{}", probe.stdout, probe.stderr);
    let calls = flaky.calls();
    assert!(
        calls >= 2,
        "the place needs the toplevel and the remote: {calls}"
    );

    for n in 1..=calls {
        let id = format!("lead{n}");
        flaky.fail(&n.to_string());
        let whoami = riff(&server, dir, Some(&id), Some(&flaky), &["whoami"]).await;
        assert_eq!(
            whoami.code, 0,
            "call {n}: {}{}",
            whoami.stdout, whoami.stderr
        );
        let uri = format!("riff://mike@pangolin/como-technologies/riff?session={id}");
        assert!(whoami.stdout.contains(&uri), "call {n}: {}", whoami.stdout);

        let lead = riff(&server, dir, None, None, &["lead", "--session", &id]).await;
        assert_eq!(lead.code, 0, "call {n}: {}{}", lead.stdout, lead.stderr);
    }
}

/// A `git` that always fails gives `whoami` no place. The command
/// fails and says why. It registers no session.
#[tokio::test]
async fn a_git_that_always_fails_stops_whoami_and_registers_no_session() {
    let server = common::start_signed().await;
    let dir = repo();
    let dir = dir.path();
    let flaky = Flaky::new();

    flaky.fail("all");
    let whoami = riff(&server, dir, Some("lead1"), Some(&flaky), &["whoami"]).await;
    assert_eq!(whoami.code, 1, "{}{}", whoami.stdout, whoami.stderr);
    assert!(
        whoami.stderr.contains("riff cannot read git in"),
        "{}",
        whoami.stderr
    );
    assert!(
        whoami.stderr.contains("fatal: boom"),
        "the reason of git: {}",
        whoami.stderr
    );
    assert!(
        whoami.stderr.contains("riff does not guess where you work"),
        "{}",
        whoami.stderr
    );
    // riff tried each call 3 times.
    assert_eq!(
        flaky.calls(),
        riff::identity::GIT_TRIES,
        "{}",
        whoami.stderr
    );

    // No session is in the riff, and the lead has none to name.
    let who = riff(&server, dir, None, None, &["who", "--all", "--long"]).await;
    assert!(!who.stdout.contains("lead1"), "{}", who.stdout);
    let lead = riff(&server, dir, None, None, &["lead", "--session", "lead1"]).await;
    assert_eq!(lead.code, 1, "{}{}", lead.stdout, lead.stderr);
    assert!(
        lead.stderr
            .contains("mike has no session lead1 in como-technologies/riff"),
        "{}",
        lead.stderr
    );
}
