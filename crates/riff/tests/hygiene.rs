//! `riff workers start` and `riff workers next` fast-forward the main
//! clone to `origin` first (01M3MNP34M5PAZW9VWAYVGNSV2). With local
//! changes, they change nothing and say why; `riff workers next` also
//! tells the lead (01M3MNP36TZYN3PE00AZJTJSER).

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use riff::api::Api;
use riff::identity;
use riff_core::name::{SessionUri, Who};
use riff_core::wire::RiffState;

/// A `tmux` that answers the calls of `riff workers start`.
const FAKE_TMUX: &str = r#"#!/bin/sh
case "$1" in
  display-message) echo "@0" ;;
  new-window) echo "@7 %1" ;;
  split-window) echo "%2" ;;
esac
exit 0
"#;

/// `cmd` with no git settings of the user or the machine, for example
/// no commit signing.
fn alone(cmd: &mut Command) -> &mut Command {
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = alone(Command::new("git").arg("-C").arg(dir).args(args))
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A bare `origin.git`, and a main clone `main` that is 2 commits
/// behind it and clean.
struct Clone {
    root: tempfile::TempDir,
}

impl Clone {
    fn behind() -> Self {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        git(r, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
        git(r, &["clone", "-q", "origin.git", "seed"]);
        let seed = r.join("seed");
        std::fs::write(seed.join("file"), "one\n").unwrap();
        git(&seed, &["add", "file"]);
        git(&seed, &["commit", "-q", "-m", "one"]);
        git(&seed, &["push", "-q", "origin", "HEAD:main"]);
        git(r, &["clone", "-q", "origin.git", "main"]);
        for n in ["two", "three"] {
            git(&seed, &["commit", "-q", "--allow-empty", "-m", n]);
        }
        git(&seed, &["push", "-q", "origin", "HEAD:main"]);
        Clone { root }
    }

    fn main(&self) -> PathBuf {
        std::fs::canonicalize(self.root.path().join("main")).unwrap()
    }

    fn head(&self) -> String {
        git(&self.main(), &["rev-parse", "HEAD"])
    }

    fn origin(&self) -> String {
        git(&self.main(), &["rev-parse", "origin/main"])
    }

    fn remote_head(&self) -> String {
        git(&self.root.path().join("seed"), &["rev-parse", "HEAD"])
    }

    /// A local change to a tracked file.
    fn change(&self) {
        std::fs::write(self.main().join("file"), "changed\n").unwrap();
    }
}

/// A machine: the fake `tmux` first on `PATH`, its own settings and
/// runtime directories.
struct Machine {
    fake: tempfile::TempDir,
    run: tempfile::TempDir,
    server: String,
}

impl Machine {
    fn new(server: &str) -> Self {
        let fake = tempfile::tempdir().unwrap();
        let tmux = fake.path().join("tmux");
        std::fs::write(&tmux, FAKE_TMUX).unwrap();
        std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
        Machine {
            fake,
            run: tempfile::tempdir().unwrap(),
            server: server.into(),
        }
    }

    /// `riff workers ARGS` in `dir`, in the tmux pane `%3`.
    fn riff(&self, dir: &Path, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
        alone(&mut cmd)
            .arg("workers")
            .args(args)
            .current_dir(dir)
            .env("PATH", path)
            .env("RIFF_HOME", self.run.path())
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", "%3")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env_remove("RIFF_SESSION")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("RIFF_WORKER");
        cmd
    }

    /// `riff workers start 1` by a person in a plain terminal.
    fn start(&self, dir: &Path) -> Output {
        let out = self.riff(dir, &["limit", "1"]).output().unwrap();
        assert!(out.status.success(), "{out:?}");
        self.riff(dir, &["start", "1"]).output().unwrap()
    }

    /// `riff workers next` in the worker `id`.
    fn next(&self, dir: &Path, id: &str) -> Output {
        self.riff(dir, &["next"])
            .env("RIFF_SESSION", id)
            .env("RIFF_WORKER", "1")
            .output()
            .unwrap()
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

fn session(dir: &Path, id: &str) -> SessionUri {
    let place = identity::place_in(dir, "pangolin").unwrap();
    SessionUri::new(Who::new("mike", Some(id)).unwrap(), place)
}

/// A riff with the lead `l1` and the worker `w1`, both in `dir`.
async fn riff_in(dir: &Path) -> (Api, SessionUri) {
    let api = start_server().await;
    let lead = session(dir, "l1");
    api.register(&lead).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    api.register(&session(dir, "w1")).await.unwrap();
    (api, lead)
}

/// The bodies of the unread messages of `me`.
async fn unread(api: &Api, me: &SessionUri) -> Vec<String> {
    api.inbox(me, None, false)
        .await
        .unwrap()
        .into_iter()
        .flat_map(|i| i.messages)
        .map(|m| m.message.body)
        .collect()
}

#[test]
fn workers_start_fast_forwards_a_clean_main_clone() {
    let clone = Clone::behind();
    let before = clone.head();
    let m = Machine::new("http://riff.test:7878");
    let out = m.start(&clone.main());
    assert!(out.status.success(), "{out:?}");
    assert_eq!(clone.head(), clone.remote_head());
    assert_ne!(clone.head(), before);
    let main = clone.main();
    assert!(
        stdout(&out).contains(&format!(
            "riff: the main clone {} moved 2 commits forward to origin/main.",
            main.display()
        )),
        "{out:?}"
    );
}

#[test]
fn workers_start_keeps_a_main_clone_with_local_changes() {
    let clone = Clone::behind();
    clone.change();
    let before = clone.head();
    let m = Machine::new("http://riff.test:7878");
    let out = m.start(&clone.main());
    assert!(out.status.success(), "{out:?}");
    assert_eq!(clone.head(), before);
    assert_eq!(
        std::fs::read_to_string(clone.main().join("file")).unwrap(),
        "changed\n"
    );
    assert!(
        stdout(&out).contains("stays as it is: it has local changes."),
        "{out:?}"
    );
}

#[test]
fn workers_start_keeps_a_main_clone_on_another_branch() {
    let clone = Clone::behind();
    git(&clone.main(), &["switch", "-q", "-c", "work"]);
    let before = clone.head();
    let m = Machine::new("http://riff.test:7878");
    let out = m.start(&clone.main());
    assert!(out.status.success(), "{out:?}");
    assert_eq!(clone.head(), before);
    assert!(
        stdout(&out).contains("it is on the branch work, not on main."),
        "{out:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn workers_next_fast_forwards_a_clean_main_clone() {
    let clone = Clone::behind();
    let (api, lead) = riff_in(&clone.main()).await;
    let m = Machine::new(api.base());
    let out = m.next(&clone.main(), "w1");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(clone.head(), clone.remote_head());
    assert_eq!(clone.origin(), clone.remote_head());
    let text = stdout(&out);
    assert!(
        text.contains("moved 2 commits forward to origin/main."),
        "{text}"
    );
    assert!(text.contains("End your turn now"), "{text}");
    assert_eq!(unread(&api, &lead).await, Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn workers_next_keeps_a_main_clone_with_local_changes_and_tells_the_lead() {
    let clone = Clone::behind();
    clone.change();
    let before = clone.head();
    let (api, lead) = riff_in(&clone.main()).await;
    let m = Machine::new(api.base());
    let out = m.next(&clone.main(), "w1");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(clone.head(), before);
    assert!(
        stdout(&out).contains("stays as it is: it has local changes."),
        "{out:?}"
    );
    let told = unread(&api, &lead).await;
    assert!(
        told.iter().any(|b| b.contains("it has local changes")),
        "{told:?}"
    );
}
