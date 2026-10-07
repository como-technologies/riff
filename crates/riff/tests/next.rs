//! A fresh context for a worker after each item: riff clears the
//! context of a worker by itself, when its turn ends after its last
//! release (01M3XV0562D3H3P22CJDBPAZBH, 01M3JQCCZ5M9VY3RGXWJYJN9Q9,
//! 01M3JQCD16CNWN5FCQBRKHXYMP). A fake `tmux` on `PATH` writes each
//! call to a log. Each test runs the real `riff` binary.

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::identity;
use riff_core::name::{SessionUri, Who};
use riff_core::record::Change;
use riff_core::wire::{RiffState, StartReason};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::store::Memory;

/// A fake `tmux` that writes each call to a log. When the file `clears`
/// next to it names a `riff`, the key `/clear` runs the start hook of
/// the worker `w1` with that `riff`, as `/clear` in a real pane does.
const FAKE_TMUX: &str = r#"#!/bin/sh
# Outside tmux, riff names its own server: -L riff (see start.rs).
[ "$1" = -L ] && shift 2
d="$(dirname "$0")"
printf '%s\n' "$*" >> "$d/log"
if [ "$*" = "send-keys -t %3 -l /clear" ] && [ -e "$d/clears" ]; then
  printf '{"session_id":"w1","source":"clear"}' | "$(cat "$d/clears")" hook session-start >/dev/null 2>&1
fi
"#;

/// Longer than the check of the Stop hook and the wait before its first
/// key ([`riff::next::CLEAR_WAIT`]).
const NO_KEYS: Duration = Duration::from_millis(2500);

/// The bound of each wait for a fact, for example the keys of a clear.
/// It is generous: it only ends a test that hangs. A busy machine can
/// take more than 20 seconds for the check of the Stop hook (#460).
const WAIT: Duration = Duration::from_secs(60);

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

    /// The calls of the fake `tmux`, but the count of the workers
    /// before each clear (01M402VFGAJQM1QW8B42NKMJM4).
    fn log(&self) -> String {
        let log = std::fs::read_to_string(self.fake.path().join("log")).unwrap_or_default();
        log.lines()
            .filter(|l| !l.starts_with("list-panes"))
            .map(|l| format!("{l}\n"))
            .collect()
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
        self.stop_hook_with(id, worker, None);
    }

    /// The Stop hook of the session `id`, with the transcript of its
    /// agent in the input.
    fn stop_hook_with(&self, id: &str, worker: bool, transcript: Option<&Path>) {
        let span = isolated::Span::start();
        self.end_turn(id, worker, transcript);
        assert!(
            span.within(Duration::from_secs(1)),
            "{:?}, CPU {:?}",
            span.wall(),
            span.cpu()
        );
    }

    /// The Stop hook of the session `id`, with no limit on its time: a
    /// test of the clear under load (#497).
    fn end_turn(&self, id: &str, worker: bool, transcript: Option<&Path>) {
        let input = serde_json::json!({
            "session_id": id,
            "hook_event_name": "Stop",
            "transcript_path": transcript,
        })
        .to_string();
        let out = self.hook(id, worker, "stop", &input);
        assert!(out.status.success(), "{out:?}");
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
        let end = Instant::now() + WAIT;
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
        let line = line.unwrap_or_else(|| panic!("no worker in {who}"));
        // The columns have a width: one space between the words.
        line.split_whitespace().collect::<Vec<_>>().join(" ")
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
    let mut watch: Child =
        r.w.riff("w1", true, &["watch", "--once"])
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
    let out = tokio::time::timeout(WAIT, out)
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
    // The watch of the worker: a session with no watch shows as offline.
    let mut watch: Child =
        r.w.riff("w1", true, &["watch"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
    let end = Instant::now() + WAIT;
    while r.who().contains("offline") {
        assert!(Instant::now() < end, "{}", r.who());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let waits = r.who();
    assert!(waits.contains(" must clear worker "), "{waits}");

    r.w.stop_hook("w1", true);
    r.w.keys(0).await;
    // The keys take 4 seconds: the time since the first start is more.
    let old = fresh_secs(&r.who());
    assert!(old >= 4, "{old}");
    r.w.clear("w1");
    let cleared = r.who();
    watch.kill().unwrap();
    watch.wait().unwrap();
    assert!(cleared.contains(" idle worker "), "{cleared}");
    assert!(fresh_secs(&cleared) < old, "{cleared}");
}

/// A proxy in front of `server`. While its gate is closed, it holds each
/// new connection: the server does not answer. It counts the
/// connections that it holds.
struct Gate {
    url: String,
    open: tokio::sync::watch::Sender<bool>,
    held: Arc<AtomicUsize>,
}

impl Gate {
    async fn before(server: &str) -> Self {
        let (open, gate) = tokio::sync::watch::channel(true);
        let held = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let to = server.trim_start_matches("http://").to_owned();
        let count = held.clone();
        tokio::spawn(async move {
            while let Ok((mut from, _)) = listener.accept().await {
                let (mut gate, to, count) = (gate.clone(), to.clone(), count.clone());
                tokio::spawn(async move {
                    if !*gate.borrow() {
                        count.fetch_add(1, Ordering::SeqCst);
                    }
                    if gate.wait_for(|open| *open).await.is_err() {
                        return;
                    }
                    let Ok(mut to) = tokio::net::TcpStream::connect(&to).await else {
                        return;
                    };
                    let _ = tokio::io::copy_bidirectional(&mut from, &mut to).await;
                });
            }
        });
        Gate { url, open, held }
    }
}

/// One prompt in the transcript of an agent: a new turn starts.
const PROMPT: &str = r#"{"type":"user","message":{"content":"Join the riff."}}"#;

/// The reply to the check of a turn comes late, and the worker releases
/// its last claim in its next turn. The check types nothing into that
/// turn. The clear comes when that turn ends
/// (01M3XZCWQED9M9ZB29F730EA58).
#[tokio::test(flavor = "multi_thread")]
async fn a_check_with_a_late_reply_types_nothing_into_a_new_turn() {
    let mut r = Riff::with_a_worker().await;
    let gate = Gate::before(&r.w.server).await;
    r.w.server = gate.url.clone();
    let transcript = r.w.run.path().join("transcript.jsonl");
    std::fs::write(&transcript, format!("{PROMPT}\n")).unwrap();

    // A turn ends while the worker holds its claim. The server does not
    // answer the check.
    gate.open.send(false).unwrap();
    r.w.stop_hook_with("w1", true, Some(&transcript));
    let end = Instant::now() + WAIT;
    while gate.held.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < end, "the check sent no keep-alive");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // The next turn starts and releases the last claim. Then the server
    // answers the check of the first turn: clear.
    std::fs::write(&transcript, format!("{PROMPT}\n{PROMPT}\n")).unwrap();
    let w1 = r.w.uri("w1");
    let thread = w1.default_thread().unwrap();
    r.api.release(&w1, &thread, "issue-12").await.unwrap();
    assert!(r.claim_is_refused("issue-13").await);
    gate.open.send(true).unwrap();
    r.w.no_keys(0).await;

    // The next turn ends: its check clears the worker.
    r.w.stop_hook_with("w1", true, Some(&transcript));
    r.w.keys(0).await;
}

/// A wake waits in the queue of the agent at the end of a turn of a
/// worker that released its last claim. The next turn starts when the
/// Stop hook returns, before its check counts the prompts. The Stop hook
/// counted them, so the check types nothing into that turn. The clear
/// comes when that turn ends (01M3XZCWQED9M9ZB29F730EA58).
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_that_a_waiting_wake_starts_gets_no_keys() {
    let mut r = Riff::with_a_worker().await;
    let gate = Gate::before(&r.w.server).await;
    r.w.server = gate.url.clone();
    let transcript = r.w.run.path().join("transcript.jsonl");
    std::fs::write(&transcript, format!("{PROMPT}\n")).unwrap();
    r.release();
    assert!(r.claim_is_refused("issue-13").await);

    // The turn ends. The server holds the reply, so the check of the
    // Stop hook cannot be first.
    gate.open.send(false).unwrap();
    r.w.stop_hook_with("w1", true, Some(&transcript));
    // The waiting wake starts the next turn at once.
    std::fs::write(&transcript, format!("{PROMPT}\n{PROMPT}\n")).unwrap();
    gate.open.send(true).unwrap();
    r.w.no_keys(0).await;

    // That turn ends: its check clears the worker.
    r.w.stop_hook_with("w1", true, Some(&transcript));
    r.w.keys(0).await;
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

/// Two checks of the old context of a worker both get the ask to clear.
/// The first types `/clear` and the start prompt, and the pane starts
/// the new context. The second types nothing: a new context started
/// after it (01M43STEE72Q9TD8ZNFS3273M3). The new context shows no
/// MustClear. It claims an item, starts a subagent in the background
/// and ends its turn: riff does not clear it, and it keeps its claim.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_of_an_old_context_never_clears_the_new_context() {
    let mut r = Riff::with_a_worker().await;
    let riff = Isolated::shared().riff();
    let clears = r.w.fake.path().join("clears");
    std::fs::write(clears, riff.get_program().as_encoded_bytes()).unwrap();
    let gate = Gate::before(&r.w.server).await;
    r.w.server = gate.url.clone();
    let old = r.w.run.path().join("old.jsonl");
    std::fs::write(&old, format!("{PROMPT}\n")).unwrap();
    r.release();

    // Two turns of the old context end. The server answers both checks
    // only when both asked: each reply asks for the clear.
    gate.open.send(false).unwrap();
    r.w.end_turn("w1", true, Some(&old));
    r.w.end_turn("w1", true, Some(&old));
    let end = Instant::now() + WAIT;
    while gate.held.load(Ordering::SeqCst) < 2 {
        assert!(Instant::now() < end, "the checks sent no keep-alive");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    gate.open.send(true).unwrap();

    // One clear, and the start prompt follows it.
    r.w.keys(0).await;
    r.w.no_keys(4).await;
    let fresh = r.who();
    assert!(!fresh.contains("must clear"), "{fresh}");

    // The new context claims an item, starts a subagent and ends its
    // turn.
    let claim = r.w.riff("w1", true, &["claim", "issue-13"]).output();
    let claim = claim.unwrap();
    assert!(claim.status.success(), "{claim:?}");
    let new = r.w.run.path().join("new.jsonl");
    std::fs::write(&new, format!("{PROMPT}\n{}\n", launch("look"))).unwrap();
    r.w.end_turn("w1", true, Some(&new));
    r.w.no_keys(4).await;
    let holds = r.who();
    assert!(!holds.contains("must clear"), "{holds}");
    let changes = r.changes().await;
    let released = changes
        .iter()
        .any(|c| matches!(c, Change::Released(released) if released.item == "issue-13"));
    assert!(!released, "{changes:?}");
}

/// A worker releases its last claim while a subagent of it runs in the
/// background, and its turn ends. riff asks it to stop the subagent in
/// place of the clear, one time (01M43STEHMTWKJDP48M1DZQPXE). When that
/// turn ends, riff clears the context.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_with_a_running_subagent_stops_it_before_the_clear() {
    let r = Riff::with_a_worker().await;
    let transcript = r.w.run.path().join("transcript.jsonl");
    std::fs::write(&transcript, format!("{PROMPT}\n{}\n", launch("look"))).unwrap();
    r.release();
    r.w.end_turn("w1", true, Some(&transcript));
    let end = Instant::now() + WAIT;
    while r.w.log().lines().count() < 2 {
        assert!(Instant::now() < end, "no prompt: {}", r.w.log());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let ask = "riff: before the clear of your context, stop each subagent that runs in the \
               background with the TaskStop tool: look. Then end your turn.";
    assert_eq!(
        r.w.log().lines().collect::<Vec<_>>(),
        [
            format!("send-keys -t %3 -l {ask}").as_str(),
            "send-keys -t %3 Enter"
        ]
    );
    r.w.no_keys(2).await;

    // The prompt is the next turn. The subagent still runs when it
    // ends: riff asked one time, so it clears the context.
    let line = serde_json::json!({"type": "user", "message": {"content": ask}});
    let text = std::fs::read_to_string(&transcript).unwrap();
    std::fs::write(&transcript, format!("{text}{line}\n")).unwrap();
    r.w.end_turn("w1", true, Some(&transcript));
    r.w.keys(2).await;
}

/// The lines of a transcript in which the agent starts the subagent
/// `about` in the background.
fn launch(about: &str) -> String {
    let call = serde_json::json!({"type": "assistant", "message": {"content": [{
        "type": "tool_use", "id": "t1", "name": "Agent",
        "input": {"description": about, "run_in_background": true},
    }]}});
    let result = serde_json::json!({"type": "user", "message": {"content": [{
        "type": "tool_result", "tool_use_id": "t1",
        "content": [{"type": "text", "text": "Async agent launched successfully.\nagentId: a1"}],
    }]}});
    format!("{call}\n{result}")
}
