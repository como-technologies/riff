//! The rollout of the lead runs the free work by itself: it asks the
//! riff daemon to run the wave of the repository when free work waits.
//! It starts no worker in tmux (01M53NGVE839TRH7XB7CDPQNZE,
//! 01M5515CA4CZ6KV4ZD6Y0744Z7, 01M599XR3NMJD9RAJ6ZAMW6KG5). The lead
//! runs `riff mcp` with a fake `tmux` and a fake `gh` on `PATH`, its own
//! riff home, and a fake riff daemon ([`FakeDaemon`]) when the test needs
//! one. The unit tests of `riff::rollout` test the rate and the notes
//! with a fake clock.
//!
//! A held item is no free work (01M51A55Q2MDZT38JDCEJ1QECP).

use isolated::Isolated;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use riff::identity;
use riff_client::api::{Api, PauseScope};
use riff_core::name::{SessionUri, Who};
use riff_core::wire::RiffState;

/// A `tmux` that logs each call. The rollout must make none.
const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/log"
exit 0
"#;

/// The wave `Wave 1` holds the issues of the file `issues`. The open
/// pull requests are in the file `pulls`. With no file, no pull request
/// is open. The head of `main` has no Gate yet.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/gh.log"
case "$1 $2" in
  api*)
    case "$2" in
      */milestones*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
      *) echo '[]' ;;
    esac ;;
  "issue list") cat "$dir/issues" ;;
  "pr list") cat "$dir/pulls" 2>/dev/null || echo '[]' ;;
  *) exit 1 ;;
esac
"#;

/// The bound of each wait. It is generous: it only ends a test that
/// hangs.
const WAIT: Duration = Duration::from_secs(60);

/// The interval of the rollout in these tests is 1 second. A count of
/// workers that does not change for this time is the result of each
/// look that ran.
const QUIET: Duration = Duration::from_secs(4);

/// A fake riff daemon in the riff home of a lead. It answers `status`
/// with the waves that it runs, and takes `wave`: it logs the ask, and
/// the wave of its repository runs. It takes `image`: it logs the image,
/// and has it, unless [`FakeDaemon::no_container`] says why it cannot.
/// Each other ask fails.
struct FakeDaemon {
    /// The repository of each ask of `wave`, in order.
    asks: Arc<Mutex<Vec<String>>>,
    /// The repositories of the waves that run.
    waves: Arc<Mutex<Vec<String>>>,
    /// The asks of `status`: one at each look of a running riff.
    looks: Arc<Mutex<usize>>,
    /// The image of each ask of `image`, in order, and why the daemon
    /// cannot start a container, when it cannot.
    images: Arc<Mutex<(Vec<String>, Option<String>)>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for FakeDaemon {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl FakeDaemon {
    /// The fake daemon of the riff home `home`, with the waves `waves`.
    async fn start(home: &Path, waves: &[&str]) -> FakeDaemon {
        let dir = riff::daemon::dir_in(home);
        std::fs::create_dir_all(&dir).unwrap();
        let listener = tokio::net::UnixListener::bind(riff::daemon::socket(&dir)).unwrap();
        let asks = Arc::new(Mutex::new(Vec::new()));
        let shown = Arc::new(Mutex::new(waves.iter().map(|w| w.to_string()).collect()));
        let looks = Arc::new(Mutex::new(0));
        let images = Arc::new(Mutex::new((Vec::new(), None)));
        let (a, w, l, i) = (asks.clone(), shown.clone(), looks.clone(), images.clone());
        let task = tokio::spawn(async move {
            loop {
                let Ok((conn, _)) = listener.accept().await else {
                    return;
                };
                let (a, w, l, i) = (a.clone(), w.clone(), l.clone(), i.clone());
                tokio::spawn(async move { FakeDaemon::answer(conn, &a, &w, &l, &i).await });
            }
        });
        FakeDaemon {
            asks,
            waves: shown,
            looks,
            images,
            task,
        }
    }

    /// A fake daemon of the riff home `home` that takes each connection,
    /// keeps it open, and never answers: as a daemon whose status waits
    /// on a lock (01M558EY88CACNZ55FWMRT8RP7). It counts the connections
    /// as looks.
    async fn silent(home: &Path) -> FakeDaemon {
        let dir = riff::daemon::dir_in(home);
        std::fs::create_dir_all(&dir).unwrap();
        let listener = tokio::net::UnixListener::bind(riff::daemon::socket(&dir)).unwrap();
        let looks = Arc::new(Mutex::new(0));
        let l = looks.clone();
        let task = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((conn, _)) = listener.accept().await {
                *l.lock().unwrap() += 1;
                held.push(conn);
            }
        });
        FakeDaemon {
            asks: Arc::default(),
            waves: Arc::default(),
            looks,
            images: Arc::default(),
            task,
        }
    }

    /// Answers the one ask of `conn`.
    async fn answer(
        conn: tokio::net::UnixStream,
        asks: &Mutex<Vec<String>>,
        waves: &Mutex<Vec<String>>,
        looks: &Mutex<usize>,
        images: &Mutex<(Vec<String>, Option<String>)>,
    ) {
        use riff::daemon::{Ask, Hello, Say};
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let (read, mut write) = conn.into_split();
        let mut lines = BufReader::new(read).lines();
        let Ok(Some(line)) = lines.next_line().await else {
            return;
        };
        let hello: Hello = serde_json::from_str(&line).unwrap();
        let say = match hello.ask {
            Ask::Status => {
                *looks.lock().unwrap() += 1;
                let shown: Vec<serde_json::Value> = waves
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|repo| serde_json::json!({"repo": repo, "word": "no work"}))
                    .collect();
                let status = serde_json::json!({
                    "pid": 1, "build": "test", "since_ms": 0, "dir": "/d",
                    "sessions": [], "waves": shown,
                });
                Say::Status(Box::new(serde_json::from_value(status).unwrap()))
            }
            Ask::Wave { wave } => {
                asks.lock().unwrap().push(wave.repo.clone());
                waves.lock().unwrap().push(wave.repo);
                Say::Done
            }
            Ask::Image { image } => {
                let mut images = images.lock().unwrap();
                images.0.push(image);
                match images.1.clone() {
                    Some(error) => Say::Failed { error },
                    None => Say::Done,
                }
            }
            _ => Say::Failed {
                error: "the fake daemon takes no other ask".into(),
            },
        };
        let mut line = serde_json::to_string(&say).unwrap();
        line.push('\n');
        let _ = write.write_all(line.as_bytes()).await;
    }

    /// The repository of each ask of the wave.
    fn asks(&self) -> Vec<String> {
        self.asks.lock().unwrap().clone()
    }

    /// The image of each ask of `image`.
    fn images(&self) -> Vec<String> {
        self.images.lock().unwrap().0.clone()
    }

    /// From now on, each ask of `image` fails with `why`, as on a machine
    /// that cannot start a container.
    fn no_container(&self, why: &str) {
        self.images.lock().unwrap().1 = Some(why.to_owned());
    }

    /// The wave of each repository stops, as with `riff wave stop`.
    fn stop(&self) {
        self.waves.lock().unwrap().clear();
    }

    /// Waits until the rollout made `n` asks of the wave.
    async fn until_asks(&self, n: usize) {
        let start = Instant::now();
        while self.asks().len() < n {
            assert!(start.elapsed() < WAIT, "timed out: {n} asks of the wave");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Waits until two more looks of a running riff asked the daemon
    /// for its status after this call: the second one is a full look
    /// that started after the call.
    async fn looked(&self) {
        let start = Instant::now();
        let before = *self.looks.lock().unwrap();
        while *self.looks.lock().unwrap() < before + 2 {
            assert!(start.elapsed() < WAIT, "timed out: no look of the rollout");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

struct Lead {
    api: Api,
    fake: tempfile::TempDir,
    home: tempfile::TempDir,
    _root: tempfile::TempDir,
    me: SessionUri,
    mcp: Child,
}

impl Drop for Lead {
    fn drop(&mut self) {
        let _ = self.mcp.kill();
        let _ = self.mcp.wait();
    }
}

impl Lead {
    /// Each call of the fake tmux.
    fn tmux_log(&self) -> String {
        std::fs::read_to_string(self.fake.path().join("log")).unwrap_or_default()
    }

    fn issues(&self, json: &str) {
        std::fs::write(self.fake.path().join("issues"), json).unwrap();
    }

    /// One open pull request for the branch of issue 1, with the
    /// status `riff/verify` of `state` on its head, or with no status.
    fn pull(&self, state: Option<&str>) {
        self.pull_of(r#""headRefName":"worktree-issue-1""#, state);
    }

    /// [`Lead::pull`] with the fields `head` of the JSON of `gh`: the
    /// branch, and the body.
    fn pull_of(&self, head: &str, state: Option<&str>) {
        let status = state.map_or(String::new(), |state| {
            format!(r#"{{"__typename":"StatusContext","context":"riff/verify","state":"{state}"}}"#)
        });
        let json = format!(
            r#"[{{"number":40,{head},"headRefOid":"1a2b3c4d","isDraft":false,"statusCheckRollup":[{status}]}}]"#
        );
        // A rename: a look never reads half of the file.
        let new = self.fake.path().join("pulls.new");
        std::fs::write(&new, json).unwrap();
        std::fs::rename(new, self.fake.path().join("pulls")).unwrap();
    }

    async fn riff(&self, state: RiffState) {
        self.api.set_riff(&self.me, state).await.unwrap();
    }

    /// The unread text of the lead, when it contains `needle`.
    async fn reads(&self, needle: &str) -> String {
        reads(&self.api, &self.me, needle).await
    }

    /// The looks of the rollout that got to `gh`: a look of a running
    /// riff asks `gh` for the waves one time.
    fn looks(&self) -> usize {
        std::fs::read_to_string(self.fake.path().join("gh.log"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.starts_with("api ") && line.contains("/milestones"))
            .count()
    }

    /// Waits until one full look ran after this call, and it got to
    /// `gh`. The first new call of `gh` can be of a look that started
    /// before this call. The second one is of a look that started after
    /// the first. The riff runs.
    async fn looked(&self) {
        let (start, before) = (Instant::now(), self.looks());
        while self.looks() < before + 2 {
            assert!(start.elapsed() < WAIT, "timed out: no look of the rollout");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

/// The unread text of `me`, when it contains `needle`.
async fn reads(api: &Api, me: &SessionUri, needle: &str) -> String {
    let start = Instant::now();
    let mut read = String::new();
    while !read.contains(needle) {
        let inbox = api.inbox(me, None, false).await.unwrap();
        read.push_str(&riff::text::inbox(&inbox, me));
        assert!(start.elapsed() < WAIT, "timed out: {needle}\n{read}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    read
}

fn script(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    isolated::write_program(&path, text);
}

/// The image that `riff.toml` of the repository pins.
const PINNED: &str = "ghcr.io/o/riff-session@sha256:\
                      0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A repository `o/r` with a commit that pins the session image, and an
/// origin on disk: OWNER/REPO from its path, as on GitHub. So the ask of
/// the wave reads the merged pin with no network.
fn repository(root: &Path) -> PathBuf {
    let bare = root.join("o/r.git");
    std::fs::create_dir_all(&bare).unwrap();
    let main = root.join("riff");
    std::fs::create_dir(&main).unwrap();
    let git = |dir: &Path, args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    git(&main, &["init", "-q", "-b", "main"]);
    std::fs::write(
        main.join("riff.toml"),
        format!("[session]\nimage = \"{PINNED}\"\n"),
    )
    .unwrap();
    git(&main, &["add", "riff.toml"]);
    git(&main, &["commit", "-q", "-m", "x"]);
    git(&main, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&main, &["push", "-q", "origin", "main"]);
    git(&main, &["fetch", "-q", "origin"]);
    git(&main, &["remote", "set-head", "origin", "main"]);
    std::fs::canonicalize(&main).unwrap()
}

/// The lead `l1` of mike on host `a`, in a paused riff. `RIFF_ON=1` of
/// the test environment turns riff on, and the lead works in the main
/// clone.
async fn lead() -> Lead {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    let api = Api::new(&format!("http://{addr}"));
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let fake = tempfile::tempdir().unwrap();
    script(fake.path(), "tmux", FAKE_TMUX);
    script(fake.path(), "gh", FAKE_GH);
    let home = tempfile::tempdir().unwrap();
    let path = format!(
        "{}:{}",
        fake.path().display(),
        std::env::var("PATH").unwrap()
    );
    let mcp = Isolated::shared()
        .riff()
        .arg("mcp")
        .current_dir(&main)
        .env("PATH", path)
        .env("RIFF_HOME", home.path())
        .env("RIFF_SERVER", api.base())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "a")
        .env("RIFF_SESSION", "l1")
        .env("TMUX", "/tmp/tmux-1000/default,1,0")
        .env("TMUX_PANE", "%0")
        .env(riff::rollout::INTERVAL_VAR, "1")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("RIFF_WORKER")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let place = identity::place_in(&main, "a").unwrap();
    let me = SessionUri::new(Who::new("mike", Some("l1")).unwrap(), place);
    let start = Instant::now();
    loop {
        let who = api.who(&me, false).await.unwrap_or_default();
        if who.iter().any(|s| s.uri.who() == me.who() && s.uri.lead()) {
            break;
        }
        assert!(start.elapsed() < WAIT, "the lead did not register");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Lead {
        api,
        fake,
        home,
        _root: root,
        me,
        mcp,
    }
}

const TWO_FREE: &str = r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"}},
{"number":2,"body":"Needs: nothing","comments":[],"milestone":{"title":"Wave 1"}},
{"number":3,"body":"Needs: #1","comments":[],"milestone":{"title":"Wave 1"}},
{"number":4,"body":"","comments":[{"body":"Merged in #9 (abc)"}],"milestone":{"title":"Wave 1"}},
{"number":8,"body":"","comments":[],"milestone":{"title":"Backlog"}}]"#;

/// Free work in the current wave and no wave of the repository on the
/// daemon (01M53NGVE839TRH7XB7CDPQNZE, 01M5515CA4CZ6KV4ZD6Y0744Z7). While
/// the riff is paused, the rollout asks nothing. The resume makes the
/// rollout ask the daemon one time to run the wave of the repository,
/// and the lead gets a note. Then the wave runs: the later looks ask
/// nothing. The rollout offers no tmux worker: it makes no call of
/// `tmux` (01M599XR3NMJD9RAJ6ZAMW6KG5).
#[tokio::test(flavor = "multi_thread")]
async fn a_resume_asks_the_daemon_one_time_for_the_wave_and_offers_no_tmux_worker() {
    let lead = lead().await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(TWO_FREE);
    tokio::time::sleep(QUIET).await;
    assert!(daemon.asks().is_empty(), "no ask while the riff is paused");

    lead.riff(RiffState::Running).await;
    daemon.until_asks(1).await;
    assert_eq!(daemon.asks(), ["o/r"]);
    // Before the ask, the daemon got the image that the repository pins.
    assert_eq!(daemon.images(), [PINNED]);
    lead.reads(&riff::text::rollout_wave("a", "o/r")).await;
    // More looks while the wave runs: no second ask.
    daemon.looked().await;
    daemon.looked().await;
    assert_eq!(daemon.asks().len(), 1);
    // A session of a worker in tmux gets no request, and tmux gets no call.
    let log = lead.tmux_log();
    assert!(log.is_empty(), "the rollout called tmux: {log}");
}

/// A wave of the repository runs on the daemon: the rollout asks
/// nothing, and makes no call of `gh` (01M53NGVE839TRH7XB7CDPQNZE).
#[tokio::test(flavor = "multi_thread")]
async fn a_wave_that_runs_gets_no_ask_and_no_call_of_gh() {
    let lead = lead().await;
    let daemon = FakeDaemon::start(lead.home.path(), &["o/r"]).await;
    lead.issues(TWO_FREE);
    lead.riff(RiffState::Running).await;
    daemon.looked().await;
    daemon.looked().await;
    assert!(daemon.asks().is_empty());
    assert_eq!(lead.looks(), 0, "a call of gh");
}

const ONE_ITEM: &str = r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#;

/// 01M558EY88CACNZ55FWMRT8RP7: a daemon that takes the connection and
/// never answers stops no look. After the time limit of the look, it
/// goes on with no daemon facts: the lead gets one note with the reason.
/// The next looks ask the daemon again.
#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_that_never_answers_does_not_stop_the_rollout() {
    let lead = lead().await;
    let daemon = FakeDaemon::silent(lead.home.path()).await;
    lead.issues(ONE_ITEM);
    lead.riff(RiffState::Running).await;
    let why = riff::text::daemon_silent(riff::daemon::STATUS_WAIT);
    lead.reads(&riff::text::rollout_no_wave("a", "o/r", &why))
        .await;
    daemon.looked().await;
}

/// 01M558EYAJFZK7Y822NVWGBS9H: the daemon cannot start a container of
/// the session image, as on a machine with no podman. The rollout asks
/// the daemon for the image that the repository pins, makes no ask of
/// the wave, and the lead gets one note with the reason. The next looks
/// ask for the image again, and give no second note.
#[tokio::test(flavor = "multi_thread")]
async fn a_machine_that_cannot_run_a_container_gets_no_ask_of_the_wave() {
    let lead = lead().await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    let why = riff::text::no_program("podman");
    daemon.no_container(&why);
    lead.issues(ONE_ITEM);
    lead.riff(RiffState::Running).await;
    let reason = riff::text::wave_no_container(&why);
    lead.reads(&riff::text::rollout_no_wave("a", "o/r", &reason))
        .await;
    daemon.looked().await;
    daemon.looked().await;
    assert!(daemon.asks().is_empty(), "an ask of the wave");
    let images = daemon.images();
    assert!(images.len() >= 2, "{images:?}");
    assert!(images.iter().all(|image| image == PINNED), "{images:?}");
    let inbox = lead.api.inbox(&lead.me, None, false).await.unwrap();
    let read = riff::text::inbox(&inbox, &lead.me);
    assert!(
        !read.contains("the rollout cannot run the wave"),
        "a second note: {read}"
    );
}

/// The author of a pull request released its item at the verify
/// request. The item is no free work for a build
/// (01M3Z9N5HHHS1E17NFGMVBKZ0K): while the pull request waits for the
/// merge, the rollout asks the daemon for no wave. After a failed verify
/// the item is free again, and the rollout asks for the wave.
#[tokio::test(flavor = "multi_thread")]
async fn an_item_with_an_open_pull_request_and_no_claim_is_no_build() {
    let lead = lead().await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(ONE_ITEM);
    lead.pull(Some("SUCCESS"));
    lead.riff(RiffState::Running).await;
    lead.looked().await;
    assert!(
        daemon.asks().is_empty(),
        "the pull request waits for the merge"
    );

    lead.pull(Some("FAILURE"));
    daemon.until_asks(1).await;
}

/// A pull request that waits for a verify is one unit of work: the
/// rollout asks the daemon for the wave, and the wave takes the verify
/// (01M3Z9N5HHHS1E17NFGMVBKZ0K).
#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_that_waits_for_a_verify_is_work_for_the_wave() {
    let lead = lead().await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(ONE_ITEM);
    lead.pull(None);
    lead.riff(RiffState::Running).await;
    daemon.until_asks(1).await;
    assert_eq!(daemon.asks(), ["o/r"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_pull_request_of_an_issue_with_the_label_no_verify_is_no_work() {
    let lead = lead().await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(
        r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Backlog"},
        "labels":[{"name":"no-verify"}]}]"#,
    );
    lead.pull(None);
    lead.riff(RiffState::Running).await;
    lead.looked().await;
    assert!(daemon.asks().is_empty());
    let log = std::fs::read_to_string(lead.fake.path().join("gh.log")).unwrap();
    assert!(
        log.contains("--json number,body,comments,milestone,labels"),
        "{log}"
    );

    lead.issues(r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Backlog"}}]"#);
    daemon.until_asks(1).await;
}

/// The branch of a release names no issue: the trailer `Issue: #1` of
/// the body names the item (01M51BF5S2YKB8JG10DF6YVT42).
const RELEASE: &str = r#""headRefName":"release-v1.5.0","body":"Refs #1\n\nLevel: minor.\n\nIssue: #1\nMilestone: Wave 1\n""#;

/// A release pull request counts for the issue of its trailer
/// (01M51BF5S2YKB8JG10DF6YVT42). While it waits for the merge, the item
/// is no build (01M3Z9N5HHHS1E17NFGMVBKZ0K): the rollout asks for no
/// wave. After a failed verify, the item is a build again.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_pull_request_that_waits_for_the_merge_gives_no_build() {
    let lead = lead().await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(ONE_ITEM);
    lead.pull_of(RELEASE, Some("SUCCESS"));
    lead.riff(RiffState::Running).await;
    lead.looked().await;
    assert!(daemon.asks().is_empty(), "{:?}", daemon.asks());

    lead.pull_of(RELEASE, Some("FAILURE"));
    daemon.until_asks(1).await;
}

/// While a release pull request waits for a verify, it is work for the
/// wave (01M51BF5S2YKB8JG10DF6YVT42, 01M3Z9N5HHHS1E17NFGMVBKZ0K).
#[tokio::test(flavor = "multi_thread")]
async fn a_release_pull_request_that_waits_for_a_verify_is_work() {
    let lead = lead().await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(ONE_ITEM);
    lead.pull_of(RELEASE, None);
    lead.riff(RiffState::Running).await;
    daemon.until_asks(1).await;
}

/// A held item is no free work (01M51A55Q2MDZT38JDCEJ1QECP): the
/// rollout asks for no wave for it. The lead frees it: the next look
/// asks for the wave.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_item_gets_no_wave_until_the_lead_frees_it() {
    let lead = lead().await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(ONE_ITEM);
    let thread = lead.me.default_thread().unwrap();
    lead.api
        .hold(&lead.me, &thread, "issue-1", "waits for the word of Mike")
        .await
        .unwrap();
    lead.riff(RiffState::Running).await;
    lead.looked().await;
    assert!(daemon.asks().is_empty(), "{:?}", daemon.asks());

    lead.api.free(&lead.me, &thread, "issue-1").await.unwrap();
    daemon.until_asks(1).await;
}

/// The whole riff runs, and the repository of the lead is paused: the
/// rollout asks for no wave for it. A pause of another repository does
/// not stop the rollout (01M3Q5QEBTNM90SPYXNVTT7RJA,
/// 01M3XAHZBGSSJB3YX23K88W01K).
#[tokio::test(flavor = "multi_thread")]
async fn a_pause_of_the_repository_of_the_lead_stops_the_rollout() {
    let lead = lead().await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#);
    // The lead of brett pauses the repository o/strata, and the lead
    // pauses its own repository.
    let other: SessionUri = "riff://brett@k/o/strata?session=b1".parse().unwrap();
    lead.api.register(&other).await.unwrap();
    for me in [&other, &lead.me] {
        let (reply, _) = lead
            .api
            .set_pause(me, &PauseScope::Here, RiffState::Paused)
            .await
            .unwrap();
        assert!(reply.changed);
    }

    lead.riff(RiffState::Running).await;
    let pauses = lead.api.pauses(&lead.me).await.unwrap();
    assert!(pauses.riff.is_none() && pauses.repositories.len() == 2);
    tokio::time::sleep(QUIET).await;
    assert!(daemon.asks().is_empty(), "an ask for a paused repository");

    // The lead resumes its repository. The other one stays paused, and
    // the rollout asks for the wave.
    lead.api
        .set_pause(&lead.me, &PauseScope::Here, RiffState::Running)
        .await
        .unwrap();
    daemon.until_asks(1).await;
    assert_eq!(lead.api.riff(&other).await.unwrap(), RiffState::Paused);
}

/// `riff wave stop` stops the wave of the daemon. The rollout asks for
/// the wave again at its next look with free work
/// (01M5515CFHRA6AJG4GKX0ZV77W).
#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_wave_gets_a_new_ask_while_free_work_waits() {
    let lead = lead().await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(ONE_ITEM);
    lead.riff(RiffState::Running).await;
    daemon.until_asks(1).await;
    daemon.stop();
    daemon.until_asks(2).await;
}
