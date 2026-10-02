//! A fresh context for a worker after each item: riff clears the
//! context of a worker by itself, when its turn ends after its last
//! release (01M3XV0562D3H3P22CJDBPAZBH, 01M3JQCCZ5M9VY3RGXWJYJN9Q9,
//! 01M3JQCD16CNWN5FCQBRKHXYMP). A fake `tmux` on `PATH` writes each
//! call to a log. Each test runs the real `riff` binary.

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::identity;
use riff_core::name::{SessionUri, Who};
use riff_core::record::Change;
use riff_core::wire::{RiffState, StartReason};
use riff_server::store::Memory;
use riff_server::{Config, Service};

const FAKE_TMUX: &str = "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$(dirname \"$0\")/log\"\n";

/// Longer than the check of the Stop hook and the wait before its first
/// key ([`riff::next::CLEAR_WAIT`]).
const NO_KEYS: Duration = Duration::from_millis(2500);

struct Worker {
    fake: tempfile::TempDir,
    run: tempfile::TempDir,
    repo: tempfile::TempDir,
    server: String,
}

impl Worker {
    fn new(server: &str) -> Self {
        let fake = tempfile::tempdir().unwrap();
        let tmux = fake.path().join("tmux");
        std::fs::write(&tmux, FAKE_TMUX).unwrap();
        std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
        let repo = tempfile::tempdir().unwrap();
        let out = Command::new("git")
            .args(["init", "-q"])
            .arg(repo.path())
            .output()
            .unwrap();
        assert!(out.status.success());
        Worker {
            fake,
            run: tempfile::tempdir().unwrap(),
            repo,
            server: server.into(),
        }
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.fake.path().join("log")).unwrap_or_default()
    }

    /// A riff command of the session `id` in the pane `%3`. `worker`
    /// sets `RIFF_WORKER=1`.
    fn riff(&self, id: &str, worker: bool, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
        cmd.args(args)
            .current_dir(self.repo.path())
            .env("PATH", path)
            .env("RIFF_HOME", self.run.path())
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("RIFF_SESSION", id)
            .env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", "%3")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env_remove("CLAUDE_CODE_SESSION_ID");
        if worker {
            cmd.env("RIFF_WORKER", "1");
        } else {
            cmd.env_remove("RIFF_WORKER");
        }
        cmd
    }

    /// A hook of the session `id`, with `input` on its stdin.
    fn hook(&self, id: &str, worker: bool, event: &str, input: &str) -> Output {
        let mut hook = self
            .riff(id, worker, &["hook", event])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        std::io::Write::write_all(hook.stdin.as_mut().unwrap(), input.as_bytes()).unwrap();
        hook.wait_with_output().unwrap()
    }

    /// The Stop hook of the session `id`: its turn ends. The hook
    /// returns at once.
    fn stop_hook(&self, id: &str, worker: bool) {
        let start = Instant::now();
        let input = format!(r#"{{"session_id":"{id}","hook_event_name":"Stop"}}"#);
        let out = self.hook(id, worker, "stop", &input);
        assert!(out.status.success(), "{out:?}");
        let took = start.elapsed();
        assert!(took < Duration::from_secs(1), "{took:?}");
    }

    /// What `/clear` does in the pane of the worker `id`: the start hook
    /// with the source `clear`.
    fn clear(&self, id: &str) {
        let input = format!(r#"{{"session_id":"{id}","source":"clear"}}"#);
        let out = self.hook(id, true, "session-start", &input);
        assert!(out.status.success(), "{out:?}");
    }

    /// Waits until the fake `tmux` has the keys of one clear after its
    /// first `before` lines, and checks them.
    async fn keys(&self, before: usize) {
        let end = Instant::now() + Duration::from_secs(20);
        while self.log().lines().count() < before + 4 {
            assert!(Instant::now() < end, "no keys: {}", self.log());
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(
            self.log().lines().skip(before).collect::<Vec<_>>(),
            [
                "send-keys -t %3 -l /clear",
                "send-keys -t %3 Enter",
                "send-keys -t %3 -l Join the riff.",
                "send-keys -t %3 Enter",
            ]
        );
    }

    /// Waits longer than the check and its first key take, and checks
    /// that the fake `tmux` has `lines` lines: riff typed nothing more.
    async fn no_keys(&self, lines: usize) {
        tokio::time::sleep(NO_KEYS).await;
        assert_eq!(self.log().lines().count(), lines, "{}", self.log());
    }

    fn uri(&self, id: &str) -> SessionUri {
        let place = identity::place_in(self.repo.path(), "pangolin").unwrap();
        SessionUri::new(Who::new("mike", Some(id)).unwrap(), place)
    }
}

/// A riff on a memory store, so that a test reads its log.
struct Riff {
    api: Api,
    store: Arc<Memory>,
    w: Worker,
    _service: Service,
}

impl Riff {
    /// A running riff with the lead `l1` and the worker `w1`, which
    /// holds `issue-12`.
    async fn with_a_worker() -> Self {
        let store = Arc::new(Memory::default());
        let mut config = Config::default();
        config.lease.wait = Duration::from_millis(10);
        let service = Service::load(config, store.clone()).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = service.router();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let api = Api::new(&format!("http://{addr}"));
        let w = Worker::new(api.base());
        let lead = w.uri("l1");
        api.register(&lead).await.unwrap();
        api.set_riff(&lead, RiffState::Running).await.unwrap();
        let w1 = w.uri("w1");
        api.start(&w1, StartReason::Process, true).await.unwrap();
        let thread = w1.default_thread().unwrap();
        api.claim(&w1, &thread, "issue-12").await.unwrap();
        Riff {
            api,
            store,
            w,
            _service: service,
        }
    }

    /// The worker releases its last claim with `riff release`. It runs
    /// no other command.
    fn release(&self) -> String {
        let out = self
            .w
            .riff("w1", true, &["release", "issue-12"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        stdout(&out)
    }

    /// Each change of the log, in order.
    async fn changes(&self) -> Vec<Change> {
        let replayed = riff_server::log::replay(&*self.store).await.unwrap();
        replayed.records.into_iter().map(|r| r.change).collect()
    }

    /// True when the server refuses a claim of the worker.
    async fn claim_is_refused(&self, item: &str) -> bool {
        let w1 = self.w.uri("w1");
        let thread = w1.default_thread().unwrap();
        self.api.claim(&w1, &thread, item).await.is_err()
    }

    /// The line of the worker in `riff who`, as the lead sees it.
    fn who(&self) -> String {
        let out = self
            .w
            .riff("l1", false, &["who", "--color", "never"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        let who = stdout(&out);
        let line = who.lines().find(|l| l.contains("(w1)"));
        line.unwrap_or_else(|| panic!("no worker in {who}")).into()
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A worker releases its last claim, and its turn ends. Its pane gets
/// `/clear` and the start prompt, with no other command of the worker
/// (01M3XV0562D3H3P22CJDBPAZBH).
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_that_released_its_last_claim_gets_the_clear_when_its_turn_ends() {
    let r = Riff::with_a_worker().await;
    // A turn that ends while the worker holds its claim: no clear.
    r.w.stop_hook("w1", true);
    r.w.no_keys(0).await;

    let released = r.release();
    assert!(
        released.contains("riff clears your context when your turn ends"),
        "{released}"
    );
    // Nothing types before the turn ends.
    r.w.no_keys(0).await;

    r.w.stop_hook("w1", true);
    r.w.keys(0).await;

    // After the clear, a turn that ends gives no second clear.
    r.w.clear("w1");
    r.w.stop_hook("w1", true);
    r.w.no_keys(4).await;
}

/// The log shows the clear: after the `released` record that asks for
/// it, a `session_started` record of the worker with the reason `clear`
/// (01M3X9X9M079WGFPJZHNXH9VEP).
#[tokio::test(flavor = "multi_thread")]
async fn the_log_has_session_started_with_the_reason_clear_after_the_clear() {
    let r = Riff::with_a_worker().await;
    r.release();
    r.w.stop_hook("w1", true);
    r.w.keys(0).await;
    let w1 = r.w.uri("w1");
    let clear = |changes: &[Change]| {
        changes.iter().position(|c| {
            matches!(c, Change::SessionStarted(s)
                if s.reason == StartReason::Clear && s.worker && s.session.who() == w1.who())
        })
    };
    let before = r.changes().await;
    let asked = before
        .iter()
        .position(|c| matches!(c, Change::Released(released) if released.must_clear))
        .expect("the released record asks for the clear");
    assert_eq!(clear(&before), None);

    r.w.clear("w1");
    let after = r.changes().await;
    let cleared = clear(&after).expect("a session_started record with the reason clear");
    assert!(cleared > asked, "{after:?}");
}

/// A request of the lead that comes while the worker waits for its
/// clear gives no wake, and the claim that it asks for is refused. After
/// the clear, the worker gets the wake and claims the item
/// (01M3X9XBMB3R718Z81BYXTHMZ0).
#[tokio::test(flavor = "multi_thread")]
async fn a_request_that_comes_in_the_wait_is_done_after_the_clear() {
    let r = Riff::with_a_worker().await;
    r.release();
    let mut watch: Child = r
        .w
        .riff("w1", true, &["watch", "--once"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    r.api
        .tell(&r.w.uri("l1"), "w1", "request: claim issue-13")
        .await
        .unwrap();
    assert!(r.claim_is_refused("issue-13").await);

    // The turn ends. The watch still waits: the request gave no wake.
    r.w.stop_hook("w1", true);
    r.w.keys(0).await;
    assert!(
        watch.try_wait().unwrap().is_none(),
        "the watch ended before the clear"
    );

    // The clear: the watch ends with the wake of the request.
    r.w.clear("w1");
    let out = tokio::task::spawn_blocking(move || watch.wait_with_output().unwrap());
    let out = tokio::time::timeout(Duration::from_secs(20), out)
        .await
        .expect("the watch ends after the clear")
        .unwrap();
    let wake = stdout(&out);
    assert!(wake.contains("wrote to you in a direct message"), "{wake}");

    // The worker does the request.
    let claim = r.w.riff("w1", true, &["claim", "issue-13"]).output();
    let claim = claim.unwrap();
    assert!(claim.status.success(), "{claim:?}");
}

/// `riff who` shows the time since the last clear of a worker
/// (01M3X9XC99KY4RQY36A7CYWY11).
#[tokio::test(flavor = "multi_thread")]
async fn riff_who_shows_the_time_since_the_last_clear_of_a_worker() {
    let r = Riff::with_a_worker().await;
    r.release();
    let waits = r.who();
    assert!(waits.contains("  must clear  worker  "), "{waits}");

    r.w.stop_hook("w1", true);
    r.w.keys(0).await;
    // The keys take 4 seconds: the time since the first start is more.
    let old = fresh_secs(&r.who());
    assert!(old >= 4, "{old}");
    r.w.clear("w1");
    let cleared = r.who();
    assert!(cleared.contains("  idle  worker  "), "{cleared}");
    assert!(fresh_secs(&cleared) < old, "{cleared}");
}

/// The seconds of `fresh start Ns ago` in a line of `riff who`.
fn fresh_secs(line: &str) -> u64 {
    let (_, rest) = line.split_once("fresh start ").expect(line);
    let (secs, _) = rest.split_once("s ago").expect(line);
    secs.parse().expect(line)
}

/// riff never clears a session that is no worker, for example the lead:
/// its user works in it (01M3XV0562D3H3P22CJDBPAZBH).
#[tokio::test(flavor = "multi_thread")]
async fn a_session_that_is_no_worker_is_never_cleared() {
    let r = Riff::with_a_worker().await;
    let l1 = r.w.uri("l1");
    let thread = l1.default_thread().unwrap();
    r.api.claim(&l1, &thread, "issue-7").await.unwrap();
    r.api.release(&l1, &thread, "issue-7").await.unwrap();
    // Also with the worker mark in its environment: the server knows
    // that the session is no worker, and does not ask for a clear.
    r.w.stop_hook("l1", true);
    r.w.no_keys(0).await;
}
