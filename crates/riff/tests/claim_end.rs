//! A session hears when the server ends one of its claims
//! (01M51JBEQRWWZKKP3GWW8QX96C), over real HTTP. The holder `w1` runs
//! `riff watch --once` as a real process. A new start of `w1` (its
//! start hook, as a new Claude Code process and as `/clear`) and a
//! release of the lead `l1` end its claim: each ends the watch with a
//! wake of the server, and `riff read` shows the claim, the reason and
//! the step. The release of the holder itself wakes no session: the
//! first wake of each watch after it is a later message of the other
//! session.

use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

use isolated::Isolated;
use riff::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Freed, RiffState, StartReason, Wake};
use riff_state::owner::server_uri;
use riff_state::state::{Dropped, dropped};

/// How long a watch may take to end after its wake.
const WAIT: Duration = Duration::from_secs(20);

const ITEM: &str = "issue-12";

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

fn uri(id: &str) -> SessionUri {
    format!("riff://mike@pangolin/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// A riff of the lead `l1` and the session `w1`, each in a clone of
/// the repository, and the claim of `w1` on [`ITEM`].
struct Riff {
    api: Api,
    clone: tempfile::TempDir,
}

impl Riff {
    async fn new() -> Riff {
        let api = start_server().await;
        // The first session of mike is the lead (R176).
        api.register(&uri("l1")).await.unwrap();
        api.register(&uri("w1")).await.unwrap();
        api.set_riff(&uri("l1"), RiffState::Running).await.unwrap();
        let clone = tempfile::tempdir().unwrap();
        git(clone.path(), &["init", "-q"]);
        let origin = "https://github.com/como-technologies/riff.git";
        git(clone.path(), &["remote", "add", "origin", origin]);
        let riff = Riff { api, clone };
        riff.claim().await;
        // Each session reads the resume of the lead, so that no watch
        // starts with its wake (R49).
        for id in ["l1", "w1"] {
            riff.api.inbox(&uri(id), None, false).await.unwrap();
        }
        riff
    }

    async fn claim(&self) {
        let claimed = self.api.claim(&uri("w1"), &repo(), ITEM).await.unwrap();
        assert!(claimed.granted, "{claimed:?}");
    }

    /// `riff ARGS` as the session `id`, in the clone. Each session has
    /// its own riff home, so each has its own watch.
    fn riff(&self, id: &str, args: &[&str]) -> Command {
        let home = self.clone.path().join(".riff").join(id);
        let mut cmd = Isolated::shared().riff();
        cmd.args(args)
            .current_dir(self.clone.path())
            .env("RIFF_HOME", home)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("RIFF_SERVER", self.api.base())
            .env("RIFF_SESSION", id)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("RIFF_WORKER")
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        cmd
    }

    /// Starts `riff watch --once` of `id`, and waits until the server
    /// sees it.
    async fn watch(&self, id: &str) -> Child {
        let mut child = self
            .riff(id, &["watch", "--once"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let span = isolated::Span::start();
        while !self.watches(id).await {
            if child.try_wait().unwrap().is_some() {
                let mut out = String::new();
                let stdout = child.stdout.as_mut().unwrap();
                std::io::Read::read_to_string(stdout, &mut out).unwrap();
                panic!("the watch of {id} ended before it was live: {out}");
            }
            assert!(span.within(WAIT), "the watch of {id} does not start");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        child
    }

    /// True when `who` shows `id` as live: for a session that is not
    /// the lead, it has an open watch. A wake that comes before the
    /// watch is open is not lost: the watch gets it when it starts
    /// (R49).
    async fn watches(&self, id: &str) -> bool {
        self.api
            .who(&uri("x0"), false)
            .await
            .unwrap()
            .into_iter()
            .any(|s| s.uri.who().session() == Some(id) && s.live)
    }

    /// Runs `riff ARGS` as `id`, and checks that it succeeds.
    async fn run(&self, id: &str, args: &[&str]) -> String {
        let mut cmd = self.riff(id, args);
        cmd.stdout(Stdio::piped());
        let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap();
        assert!(out.status.success(), "riff {args:?}: {out:?}");
        stdout(&out)
    }

    /// Runs the start hook of `w1` for `source`, as Claude Code does
    /// for a new process (`startup`) or a `/clear` (`clear`).
    async fn start_hook(&self, source: &str) -> String {
        let mut cmd = self.riff("w1", &["hook", "session-start"]);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
        let input = format!(r#"{{"session_id":"n1","source":"{source}"}}"#);
        let out = tokio::task::spawn_blocking(move || {
            let mut hook = cmd.spawn().unwrap();
            std::io::Write::write_all(hook.stdin.as_mut().unwrap(), input.as_bytes()).unwrap();
            hook.wait_with_output().unwrap()
        })
        .await
        .unwrap();
        assert!(out.status.success(), "{out:?}");
        stdout(&out)
    }

    /// The unread messages of `w1`, in the form of `riff read`.
    async fn read(&self) -> String {
        self.run("w1", &["read"]).await
    }
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The output of the watch `child`, when it ends in time.
async fn woke(child: Child) -> String {
    let out = tokio::task::spawn_blocking(move || child.wait_with_output().unwrap());
    let out = tokio::time::timeout(WAIT, out)
        .await
        .expect("the watch ends with a wake")
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    stdout(&out)
}

/// The wake line of a direct message of `from`.
fn wake_from(from: &SessionUri, out: &str) -> bool {
    let wake = Wake {
        thread: ThreadName::direct(from.who(), uri("x0").who()),
        seq: 1,
        from: from.clone(),
        kind: riff_core::wire::Kind::Message,
    };
    let line = riff::text::wake_line(&wake);
    let start = line.split(" (message").next().unwrap();
    out.starts_with(start)
}

/// The text of the message of the server for the claim of `w1`.
fn told(why: Dropped) -> String {
    let freed = Freed {
        thread: repo(),
        item: ITEM.into(),
    };
    dropped(&why, &[freed])
}

/// A new start of the holder ends its claim and wakes its watch, also
/// when the watch runs in another process: the reason is the kind of
/// the start. The hook context names the claim too.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_start_wakes_the_holder_with_the_reason() {
    let riff = Riff::new().await;
    for (source, reason) in [
        ("startup", StartReason::Process),
        ("clear", StartReason::Clear),
    ] {
        let watch = riff.watch("w1").await;
        let context = riff.start_hook(source).await;
        assert!(context.contains("freed your claims: issue-12"), "{context}");
        let out = woke(watch).await;
        assert!(wake_from(&server_uri(), &out), "{source}: {out}");
        let read = riff.read().await;
        let text = told(Dropped::Start(reason));
        assert!(read.contains(&text), "{source}: {read}");
        assert!(read.contains("claim it again"), "{read}");
        // The session takes the item back for the next start.
        riff.claim().await;
    }
}

/// A release of the lead for the holder ends the claim, and wakes the
/// holder with the name of the lead and the step: stop the work.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_of_the_lead_wakes_the_holder_with_the_reason() {
    let riff = Riff::new().await;
    let watch = riff.watch("w1").await;
    riff.run("l1", &["release", ITEM, "--session", "w1"]).await;
    let out = woke(watch).await;
    assert!(wake_from(&server_uri(), &out), "{out}");
    let read = riff.read().await;
    let lead = uri("l1").with_lead(true);
    assert!(read.contains(&told(Dropped::Lead(lead))), "{read}");
    assert!(read.contains("Stop the work on it"), "{read}");
}

/// The holder releases its own claim: no session wakes. The first wake
/// of each watch after the release is a later message of the other
/// session.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_of_the_holder_wakes_no_one() {
    let riff = Riff::new().await;
    let holder = riff.watch("w1").await;
    let lead = riff.watch("l1").await;
    riff.run("w1", &["release", ITEM]).await;

    riff.api
        .tell(&uri("l1"), "w1", "after the release")
        .await
        .unwrap();
    let out = woke(holder).await;
    assert!(wake_from(&uri("l1"), &out), "{out}");

    riff.api
        .tell(&uri("w1"), "l1", "after the release")
        .await
        .unwrap();
    let out = woke(lead).await;
    assert!(wake_from(&uri("w1"), &out), "{out}");
    let read = riff.read().await;
    assert!(!read.contains("claims:"), "{read}");
}
