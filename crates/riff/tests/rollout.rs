//! The rollout of the lead runs the free work by itself: it gives free
//! work to the idle workers in tmux, and asks the riff daemon to run the
//! wave of the repository for the rest. It starts no worker in tmux
//! (01M53NGVE839TRH7XB7CDPQNZE, 01M5515CA4CZ6KV4ZD6Y0744Z7). The lead
//! runs `riff mcp` with a fake `tmux` and a fake `gh` on `PATH`, its own
//! riff home, and a fake riff daemon ([`FakeDaemon`]) when the test needs
//! one. The unit tests of `riff::rollout` test the rate and the notes
//! with a fake clock.
//!
//! The lead gets a message for each change of a worker setting
//! (01M3X30KHKB6W11C3NBAW7KCGW to 01M3X30RA3X08JBJ2JBVCCNEH3).
//!
//! An idle worker that joined gets free work with a request of the lead
//! (01M49ZK19GQP79Z8HH14PK85QQ to 01M49ZK1EG52YAKG974XH201RK). The author
//! of an item never gets its verify (01M51533BJSD0RB7BFQR9CAFYW). A held
//! item is no free work (01M51A55Q2MDZT38JDCEJ1QECP). The fix item of a
//! broken `main` comes first (01M51QEYS538XA58KDRY844T52).

use isolated::Isolated;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use riff::identity;
use riff_client::api::{Api, PauseScope};
use riff_core::name::{Place, SessionUri, Who};
use riff_core::wire::{RiffState, StartReason, Status};

/// `list-panes -a` lists the worker panes from the file `workers`.
/// While the file `gate` exists, the call number N of `list-panes -a`
/// waits for the file `go-N` before it reads `workers`. So a test steps
/// the looks one at a time ([`Lead::step`]). `kill-pane -t P` takes the
/// pane `P` out of `workers`.
const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/log"
n=$(grep -c -e '^split-window' -e '^new-window' "$dir/log")
case "$1" in
  list-panes)
    if [ "$2" = "-a" ]; then
      k=$(grep -c '^list-panes -a' "$dir/log")
      while [ -e "$dir/gate" ] && [ ! -e "$dir/go-$k" ]; do sleep 0.05; done
      cat "$dir/workers" 2>/dev/null
    else
      cat "$dir/panes" 2>/dev/null
    fi ;;
  list-windows) cat "$dir/windows" 2>/dev/null ;;
  display-message) echo "@0" ;;
  new-window) echo "@7 %$n" ;;
  split-window) echo "%$n" ;;
  set-option)
    case "$2 $5" in
      "-p @riff-session") echo "$4 $6" >> "$dir/workers" ;;
      "-w @riff") echo "$4 $6" >> "$dir/windows" ;;
    esac ;;
  kill-pane)
    grep -v "^$3 " "$dir/workers" > "$dir/workers.new"
    mv "$dir/workers.new" "$dir/workers" ;;
esac
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
    /// The main clone of the repository.
    main: PathBuf,
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
    fn workers(&self) -> usize {
        std::fs::read_to_string(self.fake.path().join("workers"))
            .unwrap_or_default()
            .lines()
            .count()
    }

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

    /// The pane and the session of the worker `n` (from 1) of the fake
    /// tmux.
    fn worker(&self, n: usize) -> (String, SessionUri) {
        let workers = std::fs::read_to_string(self.fake.path().join("workers")).unwrap();
        let line = workers.lines().nth(n - 1).unwrap();
        let (pane, id) = line.split_once(' ').unwrap();
        let worker = SessionUri::new(Who::new("mike", Some(id)).unwrap(), self.me.place().clone());
        (pane.to_owned(), worker)
    }

    /// A person starts a worker in tmux: the pane `%N` of the session
    /// `wN`, for the next N. The rollout starts none
    /// (01M53NGVE839TRH7XB7CDPQNZE). Gives N.
    fn add_worker(&self) -> usize {
        let n = self.workers() + 1;
        let path = self.fake.path().join("workers");
        let mut workers = std::fs::read_to_string(&path).unwrap_or_default();
        workers.push_str(&format!("%{n} w{n}\n"));
        std::fs::write(path, workers).unwrap();
        n
    }

    /// The worker `n` (from 1) of the fake tmux joins the riff and
    /// claims `item`, as a real worker does. A person starts it when it
    /// does not run yet.
    async fn claim(&self, n: usize, item: &str) {
        while self.workers() < n {
            self.add_worker();
        }
        let (_, worker) = self.worker(n);
        self.api.register_as(&worker, true).await.unwrap();
        let thread = self.me.default_thread().unwrap();
        self.api.claim(&worker, &thread, item).await.unwrap();
    }

    /// `riff workers ARGS` of a person on the machine of the lead.
    fn set(&self, args: &[&str]) {
        let out = Isolated::shared()
            .riff()
            .arg("workers")
            .args(args)
            .env("RIFF_HOME", self.home.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    }

    /// The unread text of the lead, when it contains `needle`.
    async fn reads(&self, needle: &str) -> String {
        reads(&self.api, &self.me, needle).await
    }

    /// The worker `n` (from 1) of the fake tmux joins the riff and
    /// claims nothing. A person starts it when it does not run yet. A
    /// watch of it keeps it live.
    async fn join_idle(&self, n: usize) -> SessionUri {
        while self.workers() < n {
            self.add_worker();
        }
        let (_, worker) = self.worker(n);
        self.api.register_as(&worker, true).await.unwrap();
        worker
    }

    /// The looks of the rollout that got to `gh`: a look of a running
    /// riff with room on a machine asks `gh` for the waves one time.
    fn looks(&self) -> usize {
        std::fs::read_to_string(self.fake.path().join("gh.log"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.starts_with("api ") && line.contains("/milestones"))
            .count()
    }

    /// Waits until one full look ran after this call: the look read
    /// the settings and the sessions, and it got to `gh`. The first new
    /// call of `gh` can be of a look that read them before this call.
    /// The second one is of a look that started after the first. The
    /// riff runs, and a machine has room.
    async fn looked(&self) {
        let (start, before) = (Instant::now(), self.looks());
        while self.looks() < before + 2 {
            assert!(start.elapsed() < WAIT, "timed out: no look of the rollout");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The calls of `list-panes -a` of the fake tmux: the looks at the
    /// worker panes.
    fn pane_looks(&self) -> usize {
        self.tmux_log()
            .lines()
            .filter(|line| line.starts_with("list-panes -a"))
            .count()
    }

    /// Waits until the look at the worker panes after `before` comes.
    async fn until_pane_look(&self, before: usize) {
        let start = Instant::now();
        while self.pane_looks() <= before {
            assert!(start.elapsed() < WAIT, "timed out: no look at the panes");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Closes the gate of the fake tmux: each look at the worker panes
    /// waits for [`Lead::step`]. Only one task may look at the panes:
    /// the reap. So the rollout must have no room on the machine.
    async fn gate(&self) {
        std::fs::write(self.fake.path().join("gate"), "").unwrap();
        self.until_pane_look(self.pane_looks()).await;
    }

    /// Lets the look at the worker panes that waits at the gate go on,
    /// and waits until the next look comes to the gate. The looks run
    /// one after the other, so the look that went on is complete, with
    /// each end call and each note that it gave.
    async fn step(&self) {
        let look = self.pane_looks();
        std::fs::write(self.fake.path().join(format!("go-{look}")), "").unwrap();
        self.until_pane_look(look).await;
    }

    /// Waits until the count of workers stays the same for [`QUIET`],
    /// and gives that count.
    async fn settled(&self) -> usize {
        let start = Instant::now();
        loop {
            let count = self.workers();
            tokio::time::sleep(QUIET).await;
            if self.workers() == count {
                return count;
            }
            assert!(start.elapsed() < WAIT, "the count of workers grows");
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

/// The lead `l1` of mike on host `a`, with a limit of `limit` workers,
/// in a paused riff. `RIFF_ON=1` of the test environment turns riff on,
/// and the lead works in the main clone.
async fn lead(limit: u16) -> Lead {
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
    std::fs::write(
        home.path().join("config.toml"),
        format!("[workers]\nlimit = {limit}\ninterval = 1\n"),
    )
    .unwrap();
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
        .env(
            riff::machine::MACHINE,
            "cpu 8x3000MHz, mem 16GB, 16GB available, load 0.00",
        )
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
        main,
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
/// nothing. No pane of tmux starts.
#[tokio::test(flavor = "multi_thread")]
async fn a_resume_asks_the_daemon_one_time_for_the_wave_and_starts_no_worker() {
    let lead = lead(5).await;
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
    // No worker pane: riff mcp starts only its pane of `riff tail`.
    assert_eq!(lead.workers(), 0);
    let log = lead.tmux_log();
    assert!(
        !log.contains("RIFF_WORKER"),
        "the rollout started a worker: {log}"
    );
}

/// A wave of the repository runs on the daemon: the rollout asks
/// nothing, and with no idle worker it makes no call of `gh`
/// (01M53NGVE839TRH7XB7CDPQNZE).
#[tokio::test(flavor = "multi_thread")]
async fn a_wave_that_runs_gets_no_ask_and_no_call_of_gh() {
    let lead = lead(5).await;
    let daemon = FakeDaemon::start(lead.home.path(), &["o/r"]).await;
    lead.issues(TWO_FREE);
    lead.riff(RiffState::Running).await;
    daemon.looked().await;
    daemon.looked().await;
    assert!(daemon.asks().is_empty());
    assert_eq!(lead.looks(), 0, "a call of gh");
    assert_eq!(lead.workers(), 0);
}

const ONE_ITEM: &str = r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#;

/// 01M558EY88CACNZ55FWMRT8RP7: a daemon that takes the connection and
/// never answers stops no look. After the time limit of the look, it
/// goes on with no daemon facts: the idle worker gets its request, and
/// the lead gets one note with the reason. The next looks ask the daemon
/// again.
#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_that_never_answers_does_not_stop_the_rollout() {
    let lead = lead(5).await;
    let daemon = FakeDaemon::silent(lead.home.path()).await;
    lead.issues(ONE_ITEM);
    lead.riff(RiffState::Running).await;
    let worker = lead.join_idle(1).await;
    let _stream = lead.api.watch(&worker).await.unwrap();
    reads(&lead.api, &worker, "request: claim issue-1").await;
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
    let lead = lead(5).await;
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
    let lead = lead(5).await;
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

/// A pull request that waits for a verify is one unit of work: a
/// verify. An idle worker gets it, and takes the verify claim. Then no
/// work is left: the item is no build (01M3Z9N5HHHS1E17NFGMVBKZ0K), and
/// the rollout asks for no wave.
#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_that_waits_for_a_verify_counts_one_time() {
    let lead = lead(5).await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(ONE_ITEM);
    lead.pull(None);
    lead.riff(RiffState::Running).await;
    let worker = lead.join_idle(1).await;
    let _stream = lead.api.watch(&worker).await.unwrap();
    reads(&lead.api, &worker, "request: claim verify-issue-1").await;
    let thread = lead.me.default_thread().unwrap();
    let reply = lead
        .api
        .claim(&worker, &thread, "verify-issue-1")
        .await
        .unwrap();
    assert!(reply.granted, "{reply:?}");
    daemon.looked().await;
    daemon.looked().await;
    assert!(daemon.asks().is_empty(), "a wave for the build");
}

/// The pull request of an issue with the label `no-verify` is no work
/// for a verify (01M5157V0P92EBHF4M7N9K0RGR): the rollout asks for no
/// wave for it. With no label, the same pull request is work for the
/// wave.
#[tokio::test(flavor = "multi_thread")]
async fn the_pull_request_of_an_issue_with_the_label_no_verify_is_no_work() {
    let lead = lead(5).await;
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

/// An idle worker in tmux that joined still gets free work, as in riff
/// 1.x (01M53NGVE839TRH7XB7CDPQNZE): a request of the lead for a pull
/// request that waits for a verify, with no step of the lead
/// (01M49ZK19GQP79Z8HH14PK85QQ). It gets the request one time
/// (01M49ZK1C1P817KE63EXV3EJDC). It claims, and no worker starts.
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_worker_gets_a_request_for_a_waiting_verify() {
    let lead = lead(5).await;
    lead.issues(ONE_ITEM);
    lead.pull(None);
    lead.riff(RiffState::Running).await;
    let worker = lead.join_idle(1).await;
    let _stream = lead.api.watch(&worker).await.unwrap();
    let read = reads(&lead.api, &worker, "request: claim verify-issue-1").await;
    assert!(read.contains("lead=true"), "{read}");
    // More looks within the wait of 6 intervals: no second request.
    lead.looked().await;
    lead.looked().await;
    let inbox = lead.api.inbox(&worker, None, false).await.unwrap();
    let later = riff::text::inbox(&inbox, &worker);
    assert!(!later.contains("request: claim"), "{later}");
    let thread = lead.me.default_thread().unwrap();
    let reply = lead
        .api
        .claim(&worker, &thread, "verify-issue-1")
        .await
        .unwrap();
    assert!(reply.granted, "{reply:?}");
    assert_eq!(lead.settled().await, 1, "{}", lead.tmux_log());
}

/// An idle worker that joined gets a request for a free item of the
/// current wave (01M49ZK19GQP79Z8HH14PK85QQ).
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_worker_gets_a_request_for_a_free_item() {
    let lead = lead(5).await;
    lead.issues(ONE_ITEM);
    lead.riff(RiffState::Running).await;
    let worker = lead.join_idle(1).await;
    let _stream = lead.api.watch(&worker).await.unwrap();
    reads(&lead.api, &worker, "request: claim issue-1").await;
}

/// The fix item of a broken `main` (the label `broken-main`) is free work
/// before each other item: also before a verify, and also from the
/// backlog (01M51QEYS538XA58KDRY844T52).
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_worker_gets_the_fix_item_of_a_broken_main_first() {
    let lead = lead(5).await;
    lead.issues(
        r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":2,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":9,"body":"Needs: nothing","comments":[],"milestone":{"title":"Backlog"},"labels":[{"name":"broken-main"}]}]"#,
    );
    lead.pull_of(r#""headRefName":"worktree-issue-2""#, None);
    lead.riff(RiffState::Running).await;
    let worker = lead.join_idle(1).await;
    let _stream = lead.api.watch(&worker).await.unwrap();
    let read = reads(&lead.api, &worker, "request: claim").await;
    assert!(read.contains("request: claim issue-9"), "{read}");
    assert!(!read.contains("verify-issue-2"), "{read}");
}

/// A worker on a task of the lead with no work item holds a task claim
/// (01M518P1N8R893MG9Z3VAQZR6Z). It is busy: the rollout sends it no
/// request. A new idle worker gets the request for the free item.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_with_a_task_claim_gets_no_free_work() {
    let lead = lead(5).await;
    lead.issues("[]");
    lead.riff(RiffState::Running).await;
    // A live worker of the user that joined, and claims its task while
    // the wave has no free work.
    let task = SessionUri::new(
        Who::new("mike", Some("t1")).unwrap(),
        lead.me.place().clone(),
    );
    lead.api.register_as(&task, true).await.unwrap();
    let _task = lead.api.watch(&task).await.unwrap();
    let thread = lead.me.default_thread().unwrap();
    let reply = lead
        .api
        .claim(&task, &thread, "task-iroh-listener")
        .await
        .unwrap();
    assert!(reply.granted, "{reply:?}");

    let worker = lead.join_idle(1).await;
    let _stream = lead.api.watch(&worker).await.unwrap();
    lead.issues(ONE_ITEM);
    reads(&lead.api, &worker, "request: claim issue-1").await;
    // The look that gave the request gave none to the worker with the
    // task.
    let inbox = lead.api.inbox(&task, None, false).await.unwrap();
    let read = riff::text::inbox(&inbox, &task);
    assert!(!read.contains("request: claim"), "{read}");
    assert_eq!(lead.settled().await, 1, "{}", lead.tmux_log());
}

/// The branch of a release names no issue: the trailer `Issue: #1` of
/// the body names the item (01M51BF5S2YKB8JG10DF6YVT42).
const RELEASE: &str = r#""headRefName":"release-v1.5.0","body":"Refs #1\n\nLevel: minor.\n\nIssue: #1\nMilestone: Wave 1\n""#;

/// A worker of the user that joined, with no claim. A watch of it keeps
/// it live.
async fn idle_worker(lead: &Lead) -> SessionUri {
    let idle = SessionUri::new(
        Who::new("mike", Some("t1")).unwrap(),
        lead.me.place().clone(),
    );
    lead.api.register_as(&idle, true).await.unwrap();
    idle
}

/// A release pull request counts for the issue of its trailer
/// (01M51BF5S2YKB8JG10DF6YVT42). While it waits for the merge, the item
/// is no build (01M3Z9N5HHHS1E17NFGMVBKZ0K): the idle worker gets no
/// request. After a failed verify, the item is a build again.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_pull_request_that_waits_for_the_merge_gives_no_build() {
    let lead = lead(5).await;
    lead.issues(ONE_ITEM);
    lead.pull_of(RELEASE, Some("SUCCESS"));
    let idle = idle_worker(&lead).await;
    let _watch = lead.api.watch(&idle).await.unwrap();
    lead.riff(RiffState::Running).await;
    lead.looked().await;
    assert_eq!(lead.settled().await, 0, "{}", lead.tmux_log());
    let read = unread(&lead, &idle).await;
    assert!(!read.contains("request: claim"), "{read}");

    lead.pull_of(RELEASE, Some("FAILURE"));
    reads(&lead.api, &idle, "request: claim issue-1").await;
}

/// While a release pull request waits for a verify, the idle worker
/// gets the verify of the issue of its trailer, not the build
/// (01M51BF5S2YKB8JG10DF6YVT42, 01M3Z9N5HHHS1E17NFGMVBKZ0K).
#[tokio::test(flavor = "multi_thread")]
async fn a_release_pull_request_that_waits_for_a_verify_gives_the_verify() {
    let lead = lead(5).await;
    lead.issues(ONE_ITEM);
    lead.pull_of(RELEASE, None);
    let idle = idle_worker(&lead).await;
    let _watch = lead.api.watch(&idle).await.unwrap();
    lead.riff(RiffState::Running).await;
    let read = reads(&lead.api, &idle, "request: claim verify-issue-1").await;
    assert!(!read.contains("request: claim issue-1"), "{read}");
}

/// A held item is no free work (01M51A55Q2MDZT38JDCEJ1QECP): an idle
/// worker gets no request for it. The lead frees it: the next look
/// gives the idle worker the request.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_item_gets_no_worker_until_the_lead_frees_it() {
    let lead = lead(5).await;
    lead.issues(ONE_ITEM);
    let thread = lead.me.default_thread().unwrap();
    lead.api
        .hold(&lead.me, &thread, "issue-1", "waits for the word of Mike")
        .await
        .unwrap();
    let idle = idle_worker(&lead).await;
    let _watch = lead.api.watch(&idle).await.unwrap();
    lead.riff(RiffState::Running).await;
    lead.looked().await;
    assert_eq!(lead.settled().await, 0, "{}", lead.tmux_log());
    let read = unread(&lead, &idle).await;
    assert!(!read.contains("request: claim"), "{read}");

    lead.api.free(&lead.me, &thread, "issue-1").await.unwrap();
    reads(&lead.api, &idle, "request: claim issue-1").await;
}

/// The worker does not claim in 6 intervals: it refused the item, and
/// the rollout asks the daemon for the wave (01M49ZK1C1P817KE63EXV3EJDC,
/// 01M5515CA4CZ6KV4ZD6Y0744Z7). Before that, it asks nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_that_does_not_claim_does_not_block_the_wave() {
    let lead = lead(5).await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(ONE_ITEM);
    lead.riff(RiffState::Running).await;
    let first = lead.join_idle(1).await;
    let _first = lead.api.watch(&first).await.unwrap();
    reads(&lead.api, &first, "request: claim issue-1").await;
    let start = Instant::now();
    daemon.until_asks(1).await;
    // The interval is 1 second: the wait is 6 seconds.
    assert!(
        start.elapsed() >= Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(lead.workers(), 1, "{}", lead.tmux_log());
}

/// The unread text of `who`, with no wait.
async fn unread(lead: &Lead, who: &SessionUri) -> String {
    let inbox = lead.api.inbox(who, None, false).await.unwrap();
    riff::text::inbox(&inbox, who)
}

/// The worker A holds issue-1, asks for a verify and releases the item.
/// riff clears its context: A keeps its session ID, and it is idle. The
/// rollout never gives the verify to A. A person starts the worker B,
/// and B gets the request (01M51533BJSD0RB7BFQR9CAFYW).
#[tokio::test(flavor = "multi_thread")]
async fn the_author_gets_no_request_for_its_verify_also_after_a_clear() {
    let lead = lead(5).await;
    lead.issues(ONE_ITEM);
    lead.riff(RiffState::Running).await;
    let a = lead.join_idle(1).await;
    let _a = lead.api.watch(&a).await.unwrap();
    let thread = lead.me.default_thread().unwrap();
    let reply = lead.api.claim(&a, &thread, "issue-1").await.unwrap();
    assert!(reply.granted, "{reply:?}");
    // A look sees the claim of A.
    lead.looked().await;

    let reply = lead.api.release(&a, &thread, "issue-1").await.unwrap();
    assert!(reply.must_clear, "{reply:?}");
    lead.pull(None);
    lead.api.start(&a, StartReason::Clear, true).await.unwrap();
    // After its clear, A is the only worker that joined and has no
    // claim, and the verify waits.
    lead.looked().await;
    let b = lead.join_idle(2).await;
    let _b = lead.api.watch(&b).await.unwrap();
    reads(&lead.api, &b, "request: claim verify-issue-1").await;
    lead.looked().await;
    let read = unread(&lead, &a).await;
    assert!(!read.contains("request: claim verify-issue-1"), "{read}");
}

/// A usage comment on the issue names the author of issue-1: the first
/// characters of the session ID of A. The rollout never saw the claim of
/// A, as after a new start of the lead. A gets no request for the
/// verify, and the worker B gets it (01M51533BJSD0RB7BFQR9CAFYW).
#[tokio::test(flavor = "multi_thread")]
async fn the_author_of_a_usage_comment_gets_no_request_for_its_verify() {
    let lead = lead(5).await;
    lead.issues(ONE_ITEM);
    lead.pull(None);
    lead.riff(RiffState::Running).await;
    lead.add_worker();
    let (_, a) = lead.worker(1);
    let claim = riff::usage::Claim {
        thread: "o/r".into(),
        item: "issue-1".into(),
        from_ms: 1,
        to_ms: Some(2),
        models: Default::default(),
    };
    let (_, report) = riff::usage::Report::of(&claim, a.who().session().unwrap()).unwrap();
    let issues = serde_json::json!([{
        "number": 1,
        "body": "",
        "comments": [{"body": report.comment()}],
        "milestone": {"title": "Wave 1"},
    }]);
    lead.issues(&issues.to_string());
    lead.looked().await;

    let a = lead.join_idle(1).await;
    let _a = lead.api.watch(&a).await.unwrap();
    let b = lead.join_idle(2).await;
    let _b = lead.api.watch(&b).await.unwrap();
    reads(&lead.api, &b, "request: claim verify-issue-1").await;
    lead.looked().await;
    let read = unread(&lead, &a).await;
    assert!(!read.contains("request: claim verify-issue-1"), "{read}");
}

/// An idle worker of another user in another repository cannot take the
/// free work, so the rollout asks the daemon for the wave
/// (01M3W27BJYFQCHY5MTZ2J4SKW4). An idle worker of another user in the
/// repository of the lead can take it, so the rollout asks nothing.
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_worker_in_another_repository_does_not_stop_the_rollout() {
    let lead = lead(5).await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#);
    // A live idle worker of brett in the repository o/strata.
    let other: SessionUri = "riff://brett@k/o/strata?session=w1".parse().unwrap();
    lead.api.register_as(&other, true).await.unwrap();
    let _other = lead.api.watch(&other).await.unwrap();
    let who = lead.api.who(&lead.me, false).await.unwrap();
    let seen = who.iter().find(|s| s.uri.who() == other.who()).unwrap();
    assert!(seen.live && seen.worker && seen.uri.claims().is_empty());

    lead.riff(RiffState::Running).await;
    daemon.until_asks(1).await;

    // The wave stops. A live idle worker of brett in the repository of
    // the lead.
    let place = Place::new("k", lead.me.place().repo().clone(), None).unwrap();
    let same = SessionUri::new(Who::new("brett", Some("w2")).unwrap(), place);
    lead.api.register_as(&same, true).await.unwrap();
    let _same = lead.api.watch(&same).await.unwrap();
    daemon.stop();
    daemon.looked().await;
    daemon.looked().await;
    assert_eq!(daemon.asks().len(), 1, "brett's worker can take the item");
}

/// The whole riff runs, and the repository of the lead is paused: the
/// rollout asks for no wave for it. A pause of another repository does
/// not stop the rollout (01M3Q5QEBTNM90SPYXNVTT7RJA,
/// 01M3XAHZBGSSJB3YX23K88W01K).
#[tokio::test(flavor = "multi_thread")]
async fn a_pause_of_the_repository_of_the_lead_stops_the_rollout() {
    let lead = lead(5).await;
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

/// A worker with a claim is killed: its pane is gone, with no end call,
/// and its watch stream stays open on the server, as behind a front
/// end. With no call of the lead, `riff mcp` of the lead ends the
/// session, the item is free at once, and the lead gets one note
/// (01M3WG2460P4GF7GEVBY92Q33W). The rollout starts no new worker: it
/// asks the daemon for the wave of the free item
/// (01M53NGVE839TRH7XB7CDPQNZE).
#[tokio::test(flavor = "multi_thread")]
async fn the_item_of_a_killed_worker_goes_to_the_wave() {
    let lead = lead(5).await;
    let daemon = FakeDaemon::start(lead.home.path(), &[]).await;
    lead.issues(r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#);
    lead.riff(RiffState::Running).await;
    lead.claim(1, "issue-1").await;
    let (pane, dead) = lead.worker(1);
    let _stream = lead.api.watch(&dead).await.unwrap();
    lead.looked().await;
    assert!(daemon.asks().is_empty(), "the item has its worker");
    // riff looked at the pane one time or more.
    tokio::time::sleep(riff::reap::EVERY * 2).await;

    // The kill of the whole pane: no process of the worker is left.
    std::fs::write(lead.fake.path().join("workers"), "").unwrap();

    daemon.until_asks(1).await;
    assert_eq!(lead.workers(), 0, "{}", lead.tmux_log());
    let who = lead.api.who(&lead.me, false).await.unwrap();
    assert!(
        !who.iter().any(|s| s.uri.who() == dead.who()),
        "the session of the killed worker is gone at once"
    );

    let id = dead.who().session().unwrap();
    let note = format!(
        "worker stopped: pane {pane}, session {id}, on a. The pane ended with no end call, \
         so riff ended the session. It held issue-1: free now. riff found no cause."
    );
    let start = Instant::now();
    let mut read = String::new();
    while !read.contains(&note) {
        let inbox = lead.api.inbox(&lead.me, None, false).await.unwrap();
        read.push_str(&riff::text::inbox(&inbox, &lead.me));
        assert!(start.elapsed() < WAIT, "no note: {read}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(read.matches("worker stopped").count(), 1, "{read}");
}

/// The lead gets no note for a worker that `riff workers stop` stopped:
/// the stop kills the pane, and sends the end call after it. And riff
/// never ends the worker of another repository on the same machine: it
/// keeps its claim, and the lead gets no note for it
/// (01M3WG2460P4GF7GEVBY92Q33W).
#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_worker_and_a_worker_of_another_repository_give_no_note() {
    // A limit of 0: the rollout does not look at the panes, so each
    // look of the fake tmux is a look of the reap.
    let lead = lead(0).await;
    lead.issues("[]");
    lead.riff(RiffState::Running).await;
    let workers = lead.fake.path().join("workers");
    std::fs::write(&workers, "%7 stop1\n%8 other1\n").unwrap();
    let (_, stopped) = lead.worker(1);
    lead.api.register_as(&stopped, true).await.unwrap();
    let _stopped = lead.api.watch(&stopped).await.unwrap();
    // A worker of the same user on the same machine, in o/strata.
    let other: SessionUri = "riff://mike@a/o/strata?session=other1".parse().unwrap();
    lead.api.register_as(&other, true).await.unwrap();
    let strata = other.default_thread().unwrap();
    let held = lead.api.claim(&other, &strata, "issue-7").await.unwrap();
    assert!(held.granted);
    let _other = lead.api.watch(&other).await.unwrap();
    // riff looks at the two panes.
    lead.gate().await;
    lead.step().await;

    // The two panes end. The next look sees that they are gone. The
    // stop sends its end call after that look, and before the look
    // after it.
    std::fs::write(&workers, "").unwrap();
    lead.step().await;
    lead.api.end(&stopped).await.unwrap();
    // The second look after the end acts on the lost panes.
    lead.step().await;

    let who = lead.api.who(&lead.me, false).await.unwrap();
    let kept = who.iter().find(|s| s.uri.who() == other.who());
    assert_eq!(
        kept.map(|s| s.uri.claims().to_vec()),
        Some(vec!["issue-7".to_owned()]),
        "riff ended the worker of another repository: {who:?}"
    );
    let inbox = lead.api.inbox(&lead.me, None, false).await.unwrap();
    let read = riff::text::inbox(&inbox, &lead.me);
    assert!(!read.contains("worker stopped"), "{read}");
}

/// `riff workers interval` shows and sets the interval
/// (01M3Q5QE9H42FQKEDC5G9GKCWD).
#[test]
fn riff_workers_interval_shows_and_sets_the_interval() {
    let home = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let out = Isolated::shared()
            .riff()
            .arg("workers")
            .arg("interval")
            .args(args)
            .env("RIFF_HOME", home.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    assert!(run(&[]).starts_with("workers.interval  10  ("));
    assert!(run(&[]).contains(
        "\nEach 10 seconds the lead gives free work to the idle workers, and runs the wave of the \
         riff daemon for the rest."
    ));
    assert!(run(&["30"]).starts_with("workers.interval  30  ("));
    assert!(run(&[]).contains("Each 30 seconds"));
    assert!(run(&["0"]).contains("\nThe lead starts no work by itself."));
}

/// A person sets a higher limit on a machine that is at its limit, while
/// an item is free. The lead gets one note with the setting, the old
/// value, the new value and the host. The rollout is on, so the change
/// does nothing more: the rollout starts no worker in tmux
/// (01M3X30KHKB6W11C3NBAW7KCGW, 01M5515CCV7WNKVZVAMQ375J0M).
#[tokio::test(flavor = "multi_thread")]
async fn a_higher_limit_tells_the_lead_and_starts_no_worker() {
    let lead = lead(1).await;
    lead.issues(TWO_FREE);
    lead.riff(RiffState::Running).await;
    lead.claim(1, "issue-1").await;
    // Looks with the old limit.
    assert_eq!(lead.settled().await, 1);

    lead.set(&["limit", "2"]);
    let read = lead.reads("note: workers: limit 1 to 2 on a.").await;
    assert!(!read.contains("the rollout starts"), "{read}");
    assert_eq!(lead.settled().await, 1, "{}", lead.tmux_log());
    let inbox = lead.api.inbox(&lead.me, None, false).await.unwrap();
    let more = riff::text::inbox(&inbox, &lead.me);
    assert!(!more.contains("workers: limit"), "one message: {more}");
}

/// With the rollout off, the lead gets a note that it is off. Then a
/// higher limit, while an item is free, wakes the lead: the message is
/// no note, and it names the command that starts the worker. riff starts
/// none (01M3X30RA3X08JBJ2JBVCCNEH3).
#[tokio::test(flavor = "multi_thread")]
async fn a_higher_limit_wakes_the_lead_when_the_rollout_is_off() {
    let lead = lead(1).await;
    lead.issues(TWO_FREE);
    lead.riff(RiffState::Running).await;
    lead.claim(1, "issue-1").await;
    assert_eq!(lead.settled().await, 1);

    lead.set(&["interval", "0"]);
    let read = lead
        .reads("note: workers: interval 1 to 0 on a: the rollout is off")
        .await;
    assert!(read.contains("riff starts no work by itself."), "{read}");

    lead.set(&["limit", "3"]);
    let needle = "workers: limit 1 to 3 on a: free work waits, and the rollout is off. \
                  Start workers with: riff workers start 1";
    let read = lead.reads(needle).await;
    assert!(!read.contains("note: workers: limit"), "it wakes: {read}");
    assert!(!read.contains("--host"), "the machine of the lead: {read}");
    assert_eq!(lead.settled().await, 1, "riff starts no worker");
}

/// The lead gets a note for a new limit of a workers host, for new idle
/// settings of the server, and for new MCP servers of its machine
/// (01M3X30KHKB6W11C3NBAW7KCGW).
#[tokio::test(flavor = "multi_thread")]
async fn a_change_on_a_host_and_on_the_server_tells_the_lead() {
    let lead = lead(1).await;
    lead.issues("[]");
    lead.riff(RiffState::Running).await;
    // A live workers host of mike on `b`, with a limit of 2.
    let place = Place::new("b", lead.me.place().repo().clone(), None).unwrap();
    let host = SessionUri::new(Who::new("mike", Some("h1")).unwrap(), place);
    let _watch = lead.api.watch(&host).await.unwrap();
    let status = |limit| Status {
        step: riff::host::HostStatus {
            limit,
            floor: 4,
            deaths: 0,
            machine: None,
            disk: None,
            monitor: None,
            more: 0,
            workers: vec![],
        }
        .line(),
    };
    lead.api.status(&host, &status(2)).await.unwrap();
    // The lead looked at the host one time or more, with no wait for a
    // fixed time.
    lead.looked().await;

    lead.api.status(&host, &status(3)).await.unwrap();
    lead.reads("note: workers: limit 2 to 3 on b.").await;

    // Only a person changes the settings of idle workers.
    let mike: SessionUri = "riff://mike@a".parse().unwrap();
    lead.api.idle(&mike, Some(2), Some(300)).await.unwrap();
    lead.reads("note: workers: idle on the server: per host 1 to 2, after 60 to 300 seconds.")
        .await;

    lead.set(&["mcp", "add", "github"]);
    lead.reads(
        "note: workers: mcp [riff] to [riff, github] on a: each new worker there loads them.",
    )
    .await;
}

/// The book and the skill say that the lead gets a message for each
/// change of a worker setting, with the texts that riff makes
/// (01M3X30KHKB6W11C3NBAW7KCGW).
#[test]
fn the_book_and_the_skill_say_what_the_lead_gets_for_a_change() {
    use riff::rollout::{Change, Effect};
    use riff::text::setting_changed;
    use riff_core::wire::Idle;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let book = std::fs::read_to_string(root.join("../../docs/src/how-it-works.md")).unwrap();
    let how = &book[book
        .find("### Change a worker setting while the riff runs")
        .unwrap()..];
    // This how-to and the next one: "Lower the limit while workers run"
    // (01M402VFGAJQM1QW8B42NKMJM4).
    let how = &how[..how.find("\n### Limit the workers of a machine").unwrap()];
    let limit = |old, new| Change::Limit {
        host: "pangolin".into(),
        old,
        new,
    };
    let waits = Effect::Waits {
        count: 2,
        remote: true,
    };
    let texts = [
        "```sh\nriff workers limit 4\n```".to_owned(),
        "```mermaid".to_owned(),
        setting_changed(&limit(3, 4), &Effect::Nothing),
        setting_changed(&limit(1, 3), &waits),
        setting_changed(&limit(4, 2), &Effect::Over(4)),
        setting_changed(
            &Change::Interval {
                host: "thelio".into(),
                old: 10,
                new: 0,
            },
            &Effect::Nothing,
        ),
        setting_changed(
            &Change::Mcp {
                host: "pangolin".into(),
                old: vec!["riff".into()],
                new: vec!["riff".into(), "github".into()],
            },
            &Effect::Nothing,
        ),
        setting_changed(
            &Change::Idle {
                old: Idle::default(),
                new: Idle {
                    per_host: 2,
                    after_secs: 300,
                },
            },
            &Effect::Nothing,
        ),
    ];
    for text in &texts {
        assert!(how.contains(text.as_str()), "the how-to has no {text:?}");
    }
    let skill =
        std::fs::read_to_string(root.join("claude-plugin/riff/skills/riff/SKILL.md")).unwrap();
    assert!(
        skill.contains(&setting_changed(&limit(1, 3), &waits)),
        "skill"
    );
}

/// A workers host of mike on the machine `b`, in the main clone of the
/// lead: its own fake `tmux`, its own riff home, and the limit `limit`.
/// The host stops when the value drops.
struct Host {
    fake: tempfile::TempDir,
    home: tempfile::TempDir,
    process: Child,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

impl Host {
    /// Starts `riff workers host` on `b`. `deaths` are the sessions of
    /// workers that died there in the last minutes.
    async fn start(lead: &Lead, limit: u16, deaths: &[&str]) -> Host {
        let fake = tempfile::tempdir().unwrap();
        script(fake.path(), "tmux", FAKE_TMUX);
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("config.toml"),
            format!("[workers]\nlimit = {limit}\n"),
        )
        .unwrap();
        let state = home.path().join("state");
        let now = riff::monitor::now_secs();
        for (n, id) in (0u64..).zip(deaths) {
            riff::deaths::record(&state, id, now - 60 + n).unwrap();
        }
        let path = format!(
            "{}:{}",
            fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let process = Isolated::shared()
            .riff()
            .args(["workers", "host", "--claude", "true"])
            .current_dir(&lead.main)
            .env("PATH", path)
            .env("RIFF_HOME", home.path())
            .env("RIFF_SERVER", lead.api.base())
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "b")
            .env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", "%0")
            .env(
                riff::machine::MACHINE,
                "cpu 8x3000MHz, mem 16GB, 16GB available, load 0.00",
            )
            .env_remove("RIFF_SESSION")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("RIFF_WORKER")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let host = Host {
            fake,
            home,
            process,
        };
        let start = Instant::now();
        loop {
            let who = lead.api.who(&lead.me, false).await.unwrap_or_default();
            let up = who.iter().any(|s| {
                s.live
                    && s.uri.place().host() == "b"
                    && s.status
                        .as_ref()
                        .is_some_and(|st| st.status.step.starts_with(riff::host::MARK))
            });
            if up {
                return host;
            }
            assert!(start.elapsed() < WAIT, "the host did not start");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The pane and the session of each worker of `b`.
    fn workers(&self) -> Vec<(String, String)> {
        std::fs::read_to_string(self.fake.path().join("workers"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once(' '))
            .map(|(p, s)| (p.to_owned(), s.to_owned()))
            .collect()
    }

    /// A person starts a worker on `b`: the pane `%1` of the session
    /// `b1`. Gives the pane and the session.
    fn add_worker(&self) -> (String, String) {
        std::fs::write(self.fake.path().join("workers"), "%1 b1\n").unwrap();
        ("%1".into(), "b1".into())
    }

    /// The worker pane of `b` ends with no end call.
    fn kill_workers(&self) {
        std::fs::write(self.fake.path().join("workers"), "").unwrap();
    }

    /// The deaths of the workers of `b` in the last hour.
    fn deaths(&self) -> usize {
        riff::deaths::count_in(&self.home.path().join("state"), riff::monitor::now_secs())
    }
}

/// The worker `id` of `b` joins the riff and claims `item`.
async fn claim_on_b(lead: &Lead, id: &str, item: &str) -> SessionUri {
    let worker = SessionUri::new(
        Who::new("mike", Some(id)).unwrap(),
        identity::place_in(&lead.main, "b").unwrap(),
    );
    lead.api.register_as(&worker, true).await.unwrap();
    let thread = lead.me.default_thread().unwrap();
    assert!(
        lead.api
            .claim(&worker, &thread, item)
            .await
            .unwrap()
            .granted
    );
    worker
}

const ONE_FREE: &str = r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#;

/// A worker of the host `b` dies with a claim
/// (01M493YZRPS33RR47Q6T9V6WP9). The host ends its session and records
/// the death, and the item is free. The lead gets one note for the
/// death, and no message that wakes it. The rollout starts no
/// replacement (01M53NGVE839TRH7XB7CDPQNZE): the free item goes to an
/// idle worker or to the wave of the daemon.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_of_a_host_dies_and_the_host_frees_its_item() {
    let lead = lead(0).await;
    lead.issues(ONE_FREE);
    let b = Host::start(&lead, 1, &[]).await;
    lead.riff(RiffState::Running).await;
    let (pane, id) = b.add_worker();
    let worker = claim_on_b(&lead, &id, "issue-1").await;
    // Its watch stream stays open on the server.
    let _stream = lead.api.watch(&worker).await.unwrap();
    // The host looked at the pane one time or more.
    tokio::time::sleep(riff::reap::EVERY * 2).await;

    b.kill_workers();

    let read = lead
        .reads(&format!("worker stopped: pane {pane}, session {id}, on b."))
        .await;
    assert!(read.contains("It held issue-1: free now."), "{read}");
    assert_eq!(b.deaths(), 1);
    assert_eq!(read.matches("worker stopped").count(), 1, "{read}");
    assert!(!read.contains("workers died"), "{read}");
    tokio::time::sleep(QUIET).await;
    assert!(b.workers().is_empty(), "a replacement on b");
    assert_eq!(lead.workers(), 0, "the machine of the lead has the limit 0");
}

/// 3 workers of `b` died in the last hour. The fourth death on `b`
/// starts a loop of deaths: the lead gets one message that wakes it, and
/// `b` starts no worker, though the item is free
/// (01M493YZZEW1FTDBNA090WT2AG, 01M493Z02KS82B3CVZEVFA3D6E).
#[tokio::test(flavor = "multi_thread")]
async fn four_deaths_in_an_hour_stop_the_replacement_on_a_host() {
    let lead = lead(0).await;
    lead.issues(ONE_FREE);
    let b = Host::start(&lead, 1, &["d1", "d2", "d3"]).await;
    lead.riff(RiffState::Running).await;
    let (_, id) = b.add_worker();
    let worker = claim_on_b(&lead, &id, "issue-1").await;
    // Its watch stream stays open on the server.
    let _stream = lead.api.watch(&worker).await.unwrap();
    tokio::time::sleep(riff::reap::EVERY * 2).await;

    b.kill_workers();

    let alarm = riff::text::death_loop("b", 4);
    let read = lead.reads(&alarm).await;
    assert!(
        !read.contains(&format!("note: {alarm}")),
        "a message: {read}"
    );
    assert_eq!(b.deaths(), 4);
    // The host tells its deaths in its status.
    let start = Instant::now();
    loop {
        let who = lead.api.who(&lead.me, false).await.unwrap();
        let deaths = who
            .iter()
            .filter(|s| s.uri.place().host() == "b")
            .find_map(|s| riff::host::HostStatus::parse(&s.status.as_ref()?.status.step))
            .map(|status| status.deaths);
        if deaths == Some(4) {
            break;
        }
        assert!(start.elapsed() < WAIT, "the host tells no deaths");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(QUIET * 2).await;
    assert!(b.workers().is_empty(), "no replacement on b");
    let inbox = lead.api.inbox(&lead.me, None, false).await.unwrap();
    let more = riff::text::inbox(&inbox, &lead.me);
    assert!(!more.contains("workers died"), "one message: {more}");
}

/// The lead lowers the limit: the worker over it ends after its item.
/// The lead raises it again: the lead gets a note, and the rollout starts
/// no worker (01M402VFGAJQM1QW8B42NKMJM4, 01M5515CCV7WNKVZVAMQ375J0M).
#[tokio::test(flavor = "multi_thread")]
async fn a_lower_limit_ends_a_worker_after_its_item_and_a_higher_one_starts_none() {
    let lead = lead(2).await;
    lead.issues(TWO_FREE);
    lead.riff(RiffState::Running).await;
    lead.claim(1, "issue-1").await;
    lead.claim(2, "issue-2").await;
    assert_eq!(lead.settled().await, 2);

    lead.set(&["limit", "1"]);
    lead.reads("workers: limit 2 to 1 on a").await;
    // The worker 1 ends its item: it releases, and its turn ends.
    let (pane, worker) = lead.worker(1);
    let id = worker.who().session().unwrap().to_owned();
    lead.issues(
        r#"[{"number":1,"body":"","comments":[{"body":"Merged in #9 (abc)"}],"milestone":{"title":"Wave 1"}},
{"number":2,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#,
    );
    let thread = lead.me.default_thread().unwrap();
    lead.api.release(&worker, &thread, "issue-1").await.unwrap();
    stop_hook(&lead, &pane, &id);
    lead.reads(&format!("the worker in the pane {pane} ends"))
        .await;
    let start = Instant::now();
    while lead.workers() > 1 {
        assert!(start.elapsed() < WAIT, "the worker over the limit runs");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(lead.settled().await, 1, "no start at the limit");

    lead.issues(
        r#"[{"number":2,"body":"","comments":[],"milestone":{"title":"Wave 1"}},
{"number":3,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#,
    );
    lead.set(&["limit", "2"]);
    lead.reads("note: workers: limit 1 to 2 on a.").await;
    assert_eq!(lead.settled().await, 1, "{}", lead.tmux_log());
}

/// The Stop hook of the worker `id` in `pane` on the machine of the lead.
fn stop_hook(lead: &Lead, pane: &str, id: &str) {
    let path = format!(
        "{}:{}",
        lead.fake.path().display(),
        std::env::var("PATH").unwrap()
    );
    let mut hook = Isolated::shared()
        .riff()
        .args(["hook", "stop"])
        .current_dir(&lead.main)
        .env("PATH", path)
        .env("RIFF_HOME", lead.home.path())
        .env("RIFF_SERVER", lead.api.base())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "a")
        .env("RIFF_SESSION", id)
        .env("RIFF_WORKER", "1")
        .env("TMUX", "/tmp/tmux-1000/default,1,0")
        .env("TMUX_PANE", pane)
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = format!(r#"{{"session_id":"{id}","hook_event_name":"Stop"}}"#);
    std::io::Write::write_all(hook.stdin.as_mut().unwrap(), input.as_bytes()).unwrap();
    let out = hook.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
}
