//! riff starts workers by itself when the wave has free work
//! (01M3Q5QE01DB0FJQJWFKR450KQ to 01M3Q5QEE4MQNCRKVJK3D54G9Z). The lead
//! runs `riff mcp` with a fake `tmux` and a fake `gh` on `PATH`, and its
//! own riff home. The unit tests of `riff::rollout` test the rate, the
//! limit and the placement with a fake clock.
//!
//! The lead gets a message for each change of a worker setting
//! (01M3X30KHKB6W11C3NBAW7KCGW to 01M3X30RA3X08JBJ2JBVCCNEH3).
//!
//! An idle worker that joined gets free work with a request of the lead
//! (01M49ZK19GQP79Z8HH14PK85QQ to 01M49ZK1EG52YAKG974XH201RK).

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use riff::api::{Api, PauseScope};
use riff::identity;
use riff_core::name::{Place, SessionUri, Who};
use riff_core::wire::{RiffState, Status};

/// `list-panes -a` lists the worker panes from the file `workers`.
/// `kill-pane -t P` takes the pane `P` out of it.
const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/log"
n=$(grep -c -e '^split-window' -e '^new-window' "$dir/log")
case "$1" in
  list-panes)
    if [ "$2" = "-a" ]; then cat "$dir/workers" 2>/dev/null; else cat "$dir/panes" 2>/dev/null; fi ;;
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
/// is open.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/gh.log"
case "$1 $2" in
  api*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
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
        let status = state.map_or(String::new(), |state| {
            format!(r#"{{"__typename":"StatusContext","context":"riff/verify","state":"{state}"}}"#)
        });
        let json = format!(
            r#"[{{"number":40,"headRefName":"worktree-issue-1","headRefOid":"1a2b3c4d","isDraft":false,"statusCheckRollup":[{status}]}}]"#
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

    /// The worker `n` (from 1) of the fake tmux joins the riff and
    /// claims `item`, as a real worker does.
    async fn claim(&self, n: usize, item: &str) {
        self.until_workers(n).await;
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
    /// claims nothing. A watch of it keeps it live.
    async fn join_idle(&self, n: usize) -> SessionUri {
        self.until_workers(n).await;
        let (_, worker) = self.worker(n);
        self.api.register_as(&worker, true).await.unwrap();
        worker
    }

    /// Waits until `n` workers run.
    async fn until_workers(&self, n: usize) {
        let start = Instant::now();
        while self.workers() < n {
            assert!(start.elapsed() < WAIT, "timed out: {n} workers");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The looks of the rollout that got to `gh`: a look of a running
    /// riff with room on a machine asks `gh` for the waves one time.
    fn looks(&self) -> usize {
        std::fs::read_to_string(self.fake.path().join("gh.log"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.starts_with("api "))
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
    std::fs::write(&path, text).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A repository with a commit and an origin on GitHub.
fn repository(root: &Path) -> PathBuf {
    let main = root.join("riff");
    std::fs::create_dir(&main).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&main)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", "x"]);
    git(&["remote", "add", "origin", "https://github.com/o/r.git"]);
    std::fs::canonicalize(&main).unwrap()
}

/// What turns riff on for the lead of a test.
enum On {
    /// `RIFF_ON=1` of the test environment. The lead works in the main
    /// clone.
    Env,
    /// The project settings of a linked worktree, where the lead works.
    /// riff is off in the main clone.
    Worktree,
}

/// The lead `l1` of mike on host `a`, with a limit of `limit` workers,
/// in a paused riff.
async fn lead(limit: u16) -> Lead {
    lead_with(limit, On::Env).await
}

/// A linked worktree of `main` whose project settings turn riff on.
fn worktree_with_riff_on(main: &Path) -> PathBuf {
    let tree = main.join(".claude/worktrees/lead");
    let out = Command::new("git")
        .arg("-C")
        .arg(main)
        .args(["worktree", "add", "-q", "-b", "lead"])
        .arg(&tree)
        .output()
        .unwrap();
    assert!(out.status.success(), "git worktree add: {out:?}");
    riff::enable::set(&tree.join(".claude/settings.json"), Some(true)).unwrap();
    tree
}

/// [`lead`], with riff on by `on`.
async fn lead_with(limit: u16, on: On) -> Lead {
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
    let dir = match on {
        On::Env => main.clone(),
        On::Worktree => worktree_with_riff_on(&main),
    };
    let mut mcp = Isolated::shared().riff();
    if let On::Worktree = on {
        mcp.env_remove("RIFF_ON");
    }
    let mcp = mcp
        .arg("mcp")
        .current_dir(&dir)
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
    let place = identity::place_in(&dir, "a").unwrap();
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

/// A resume of a paused riff starts one worker for each free item, with
/// no other step, and a note to the lead for each
/// (01M3Q5QE01DB0FJQJWFKR450KQ, 01M3Q5QEE4MQNCRKVJK3D54G9Z). The next
/// worker starts only when the new one claimed (01M3Q5QEJNP1JGQM7VXXEBJ9J9).
/// A pause stops the rollout within one look, and the resume starts it
/// again (01M3Q5QEBTNM90SPYXNVTT7RJA).
#[tokio::test(flavor = "multi_thread")]
async fn a_resume_starts_one_worker_for_each_free_item() {
    let lead = lead(5).await;
    lead.issues(TWO_FREE);
    assert_eq!(
        lead.settled().await,
        0,
        "no worker starts while the riff is paused"
    );

    // The unit tests of riff::rollout check the rate with a fake clock.
    lead.riff(RiffState::Running).await;
    lead.until_workers(1).await;
    // `RIFF_ON=1` turned riff on for the lead, so its worker gets it
    // (01M3XY2SWEK0N8MC3MY4TMYTD3).
    let log = lead.tmux_log();
    assert!(log.contains("-e RIFF_ON=1"), "{log}");
    assert_eq!(
        lead.settled().await,
        1,
        "the new worker is idle until it claims"
    );
    lead.claim(1, "issue-1").await;
    lead.claim(2, "issue-2").await;
    // Each free item has a worker: no third one.
    assert_eq!(lead.settled().await, 2);

    let start = Instant::now();
    let mut read = String::new();
    while read.matches("a: started 1 worker").count() < 2 {
        let inbox = lead.api.inbox(&lead.me, None, false).await.unwrap();
        read.push_str(&riff::text::inbox(&inbox, &lead.me));
        assert!(start.elapsed() < WAIT, "no notes: {read}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    lead.riff(RiffState::Paused).await;
    lead.issues(
        r#"[{"number":5,"body":"","comments":[],"milestone":{"title":"Wave 1"}},
        {"number":6,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#,
    );
    // A look that started before the pause can start one more worker.
    // Then the count stays: the two free items get no worker each.
    let paused = lead.settled().await;
    assert!(paused <= 3, "a pause stops the rollout: {paused} workers");
    tokio::time::sleep(QUIET).await;
    assert_eq!(lead.workers(), paused, "the count grows in a paused riff");

    // The resume starts the rollout again: a worker for each free item.
    lead.riff(RiffState::Running).await;
    lead.claim(3, "issue-5").await;
    lead.until_workers(4).await;
}

const ONE_ITEM: &str = r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#;

/// The author of a pull request released its item at the verify
/// request. The item is no free work for a build
/// (01M3Z9N5HHHS1E17NFGMVBKZ0K): while the pull request waits for the
/// merge, the rollout starts no worker. After a failed verify the item
/// is free again, and the rollout starts a worker for it.
#[tokio::test(flavor = "multi_thread")]
async fn an_item_with_an_open_pull_request_and_no_claim_is_no_build() {
    let lead = lead(5).await;
    lead.issues(ONE_ITEM);
    lead.pull(Some("SUCCESS"));
    lead.riff(RiffState::Running).await;
    lead.looked().await;
    assert_eq!(
        lead.settled().await,
        0,
        "the pull request waits for the merge: {}",
        lead.tmux_log()
    );

    lead.pull(Some("FAILURE"));
    lead.until_workers(1).await;
    // The next session holds the item: no more work.
    lead.claim(1, "issue-1").await;
    lead.looked().await;
    assert_eq!(lead.settled().await, 1, "{}", lead.tmux_log());
}

/// A pull request that waits for a verify is one unit of work: a
/// verify. The rollout starts one worker for it, and the worker takes
/// the verify claim. Then no work is left: the item is no build
/// (01M3Z9N5HHHS1E17NFGMVBKZ0K).
#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_that_waits_for_a_verify_counts_one_time() {
    let lead = lead(5).await;
    lead.issues(ONE_ITEM);
    lead.pull(None);
    lead.riff(RiffState::Running).await;
    lead.until_workers(1).await;
    lead.claim(1, "verify-issue-1").await;
    lead.looked().await;
    assert_eq!(
        lead.settled().await,
        1,
        "a second worker for the build: {}",
        lead.tmux_log()
    );
}

/// An idle worker that joined gets a request of the lead for a pull
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

/// The worker does not claim in 6 intervals: it refused the item, and
/// the rollout starts a new worker (01M49ZK1C1P817KE63EXV3EJDC). The new
/// worker gets the request and also does not claim. Two workers refused
/// the item: no third worker starts (01M49ZK1EG52YAKG974XH201RK).
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_that_does_not_claim_does_not_block_a_new_worker() {
    let lead = lead(5).await;
    lead.issues(ONE_ITEM);
    lead.riff(RiffState::Running).await;
    let first = lead.join_idle(1).await;
    let _first = lead.api.watch(&first).await.unwrap();
    reads(&lead.api, &first, "request: claim issue-1").await;
    let start = Instant::now();
    lead.until_workers(2).await;
    // The interval is 1 second: the wait is 6 seconds.
    assert!(
        start.elapsed() >= Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    let second = lead.join_idle(2).await;
    let _second = lead.api.watch(&second).await.unwrap();
    reads(&lead.api, &second, "request: claim issue-1").await;
    // The second worker refuses too.
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert_eq!(lead.settled().await, 2, "{}", lead.tmux_log());
}

/// The rollout starts no worker where riff is off in the main clone
/// (01M3XY2T542DCHBN95H9PX4AGQ). A worker there is a plain session: it
/// never joins the riff, so each look would start one more. The lead
/// works in a linked worktree whose project settings turn riff on. The
/// lead gets one note with the reason (01M3YCGKKRDNFC338K1JSK30JK).
/// `riff enable` in the main clone lets the rollout start a worker.
#[tokio::test(flavor = "multi_thread")]
async fn the_rollout_starts_no_worker_where_riff_is_off_in_the_main_clone() {
    let lead = lead_with(5, On::Worktree).await;
    lead.issues(TWO_FREE);
    lead.riff(RiffState::Running).await;
    let note = "a: riff starts no worker here: riff off. To turn it on: riff enable. The rollout \
                starts no worker on a until riff is on in the main clone.";
    let read = lead.reads(note).await;
    assert_eq!(read.matches(note).count(), 1, "{read}");
    assert_eq!(
        lead.settled().await,
        0,
        "riff is off in the main clone: {}",
        lead.tmux_log()
    );
    // More looks ran in that time: the note came one time.
    let inbox = lead.api.inbox(&lead.me, None, false).await.unwrap();
    let later = riff::text::inbox(&inbox, &lead.me);
    assert!(!later.contains("riff starts no worker"), "{later}");

    riff::enable::enable(&lead.main, riff::enable::Place::Local, None).unwrap();
    lead.until_workers(1).await;
    let log = lead.tmux_log();
    assert!(
        !log.contains("RIFF_ON"),
        "no RIFF_ON made the lead on: {log}"
    );
}

/// An idle worker of another user in another repository cannot take the
/// free work, so the rollout starts a worker (01M3W27BJYFQCHY5MTZ2J4SKW4).
/// An idle worker of another user in the repository of the lead can
/// take it, so the rollout starts none.
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_worker_in_another_repository_does_not_stop_the_rollout() {
    let lead = lead(5).await;
    lead.issues(r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#);
    // A live idle worker of brett in the repository o/strata.
    let other: SessionUri = "riff://brett@k/o/strata?session=w1".parse().unwrap();
    lead.api.register_as(&other, true).await.unwrap();
    let _other = lead.api.watch(&other).await.unwrap();
    let who = lead.api.who(&lead.me, false).await.unwrap();
    let seen = who.iter().find(|s| s.uri.who() == other.who()).unwrap();
    assert!(seen.live && seen.worker && seen.uri.claims().is_empty());

    lead.riff(RiffState::Running).await;
    lead.until_workers(1).await;
    lead.claim(1, "issue-1").await;

    // A live idle worker of brett in the repository of the lead.
    let place = Place::new("k", lead.me.place().repo().clone(), None).unwrap();
    let same = SessionUri::new(Who::new("brett", Some("w2")).unwrap(), place);
    lead.api.register_as(&same, true).await.unwrap();
    let _same = lead.api.watch(&same).await.unwrap();
    lead.issues(r#"[{"number":2,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#);
    assert_eq!(lead.settled().await, 1, "brett's worker can take the item");
}

/// The whole riff runs, and the repository of the lead is paused: the
/// rollout starts no worker for it. A pause of another repository does
/// not stop the rollout (01M3Q5QEBTNM90SPYXNVTT7RJA,
/// 01M3XAHZBGSSJB3YX23K88W01K).
#[tokio::test(flavor = "multi_thread")]
async fn a_pause_of_the_repository_of_the_lead_stops_the_rollout() {
    let lead = lead(5).await;
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
    assert_eq!(
        lead.settled().await,
        0,
        "no worker starts for a paused repository"
    );

    // The lead resumes its repository. The other one stays paused, and
    // the rollout starts a worker for the free item.
    lead.api
        .set_pause(&lead.me, &PauseScope::Here, RiffState::Running)
        .await
        .unwrap();
    lead.until_workers(1).await;
    assert_eq!(lead.api.riff(&other).await.unwrap(), RiffState::Paused);
}

/// The rollout never starts more workers than the limit of the machine.
#[tokio::test(flavor = "multi_thread")]
async fn the_limit_caps_the_rollout() {
    let lead = lead(1).await;
    lead.issues(TWO_FREE);
    lead.riff(RiffState::Running).await;
    lead.claim(1, "issue-1").await;
    assert_eq!(lead.settled().await, 1);
}

/// A worker with a claim is killed: its pane is gone, with no end call,
/// and its watch stream stays open on the server, as behind a front
/// end. With no call of the lead, `riff mcp` of the lead ends the
/// session, the item is free at once, the rollout starts a new worker,
/// and the new worker holds the item. The lead gets one note
/// (01M3WG2460P4GF7GEVBY92Q33W).
#[tokio::test(flavor = "multi_thread")]
async fn a_new_worker_takes_the_item_of_a_killed_worker() {
    let lead = lead(5).await;
    lead.issues(r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#);
    lead.riff(RiffState::Running).await;
    lead.claim(1, "issue-1").await;
    let (pane, dead) = lead.worker(1);
    let _stream = lead.api.watch(&dead).await.unwrap();
    assert_eq!(lead.settled().await, 1, "the item has its worker");
    // riff looked at the pane one time or more.
    tokio::time::sleep(riff::reap::EVERY * 2).await;

    // The kill of the whole pane: no process of the worker is left.
    std::fs::write(lead.fake.path().join("workers"), "").unwrap();

    let start = Instant::now();
    while lead.workers() < 1 {
        assert!(start.elapsed() < WAIT, "no new worker for the free item");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (_, new) = lead.worker(1);
    assert_ne!(new.who(), dead.who());
    let who = lead.api.who(&lead.me, false).await.unwrap();
    assert!(
        !who.iter().any(|s| s.uri.who() == dead.who()),
        "the session of the killed worker is gone at once"
    );
    lead.api.register_as(&new, true).await.unwrap();
    let thread = lead.me.default_thread().unwrap();
    let reply = lead.api.claim(&new, &thread, "issue-1").await.unwrap();
    assert!(reply.granted, "{reply:?}");

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
    let lead = lead(5).await;
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
    // riff looked at the panes one time or more.
    tokio::time::sleep(riff::reap::EVERY * 2).await;

    // The two panes end. The stop sends its end call a moment later.
    std::fs::write(&workers, "").unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    lead.api.end(&stopped).await.unwrap();

    tokio::time::sleep(riff::reap::EVERY * 3).await;
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
    assert!(run(&[]).contains("\nThe lead starts at most one worker each 10 seconds."));
    assert!(run(&["30"]).starts_with("workers.interval  30  ("));
    assert!(run(&[]).contains("each 30 seconds"));
    assert!(run(&["0"]).contains("\nThe lead starts no worker by itself."));
}

/// A person sets a higher limit on a machine that is at its limit, while
/// an item is free. The lead gets one note with the setting, the old
/// value, the new value and the host, and it says that the rollout
/// starts a worker. The rollout starts it
/// (01M3X30KHKB6W11C3NBAW7KCGW, 01M3X30R4PSBP3RQWM02BJ6GK3).
///
/// The test does not depend on the moment of the change: one look uses
/// each limit one time, and the note comes before the start
/// (01M3XFHSYJEN9V6QEKWJGJWQ8Q). The first worker shows that a look
/// with the old limit ended, and its claim is on the server before the
/// change.
#[tokio::test(flavor = "multi_thread")]
async fn a_higher_limit_tells_the_lead_and_the_rollout_starts_a_worker() {
    let lead = lead(1).await;
    lead.issues(TWO_FREE);
    lead.riff(RiffState::Running).await;
    lead.claim(1, "issue-1").await;
    assert_eq!(lead.settled().await, 1, "the machine is at its limit");

    lead.set(&["limit", "2"]);
    let read = lead
        .reads("workers: limit 1 to 2 on a: the rollout starts 1 worker.")
        .await;
    assert!(read.contains("note: workers: limit 1 to 2 on a"), "{read}");
    lead.until_workers(2).await;
    lead.claim(2, "issue-2").await;
    assert_eq!(lead.settled().await, 2);
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
    assert!(read.contains("riff starts no worker by itself."), "{read}");

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
        setting_changed(&limit(3, 4), &Effect::Starts),
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
        skill.contains(&setting_changed(&limit(3, 4), &Effect::Starts)),
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

    /// Waits until a worker runs on `b`, and gives the first one.
    async fn first_worker(&self) -> (String, String) {
        let start = Instant::now();
        loop {
            if let Some(worker) = self.workers().into_iter().next() {
                return worker;
            }
            assert!(start.elapsed() < WAIT, "timed out: no worker on b");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
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

/// riff keeps the workers of a host as the person set them
/// (01M493YZRPS33RR47Q6T9V6WP9). The machine of the lead has the limit
/// 0, and the host `b` the limit 1. A worker of `b` dies with a claim.
/// The host ends its session and records the death, the item is free,
/// and the rollout of the lead starts a replacement on `b`. The lead
/// gets one note for the death, and no message that wakes it.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_of_a_host_dies_and_the_host_starts_a_replacement() {
    let lead = lead(0).await;
    lead.issues(ONE_FREE);
    let b = Host::start(&lead, 1, &[]).await;
    lead.riff(RiffState::Running).await;
    let (pane, id) = b.first_worker().await;
    let worker = claim_on_b(&lead, &id, "issue-1").await;
    // Its watch stream stays open on the server.
    let _stream = lead.api.watch(&worker).await.unwrap();
    let mut read = lead.reads("b: started 1 worker").await;
    // The host looked at the pane one time or more.
    tokio::time::sleep(riff::reap::EVERY * 2).await;

    b.kill_workers();

    read += &lead
        .reads(&format!("worker stopped: pane {pane}, session {id}, on b."))
        .await;
    let start = Instant::now();
    let new = loop {
        if let Some((_, new)) = b.workers().into_iter().next() {
            break new;
        }
        assert!(start.elapsed() < WAIT, "no replacement on b");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_ne!(new, id);
    read += &lead.reads(&new).await;
    assert_eq!(b.deaths(), 1);
    assert_eq!(read.matches("worker stopped").count(), 1, "{read}");
    assert!(!read.contains("workers died"), "{read}");
    assert_eq!(lead.workers(), 0, "the machine of the lead has the limit 0");
}

/// 3 workers of `b` died in the last hour. The fourth death on `b`
/// starts a loop of deaths: the lead gets one message that wakes it, and
/// the rollout starts no replacement on `b`, though the item is free
/// (01M493YZZEW1FTDBNA090WT2AG, 01M493Z02KS82B3CVZEVFA3D6E).
#[tokio::test(flavor = "multi_thread")]
async fn four_deaths_in_an_hour_stop_the_replacement_on_a_host() {
    let lead = lead(0).await;
    lead.issues(ONE_FREE);
    let b = Host::start(&lead, 1, &["d1", "d2", "d3"]).await;
    lead.riff(RiffState::Running).await;
    let (_, id) = b.first_worker().await;
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
    // No machine has room, so a look stops before `gh`. Each look in
    // this time starts nothing.
    tokio::time::sleep(QUIET * 2).await;
    assert!(b.workers().is_empty(), "no replacement on b");
    let inbox = lead.api.inbox(&lead.me, None, false).await.unwrap();
    let more = riff::text::inbox(&inbox, &lead.me);
    assert!(!more.contains("workers died"), "one message: {more}");
}

/// The lead lowers the limit: the worker over it ends after its item.
/// The lead raises it again: a new worker starts for the free work
/// (01M493YZRPS33RR47Q6T9V6WP9, 01M402VFGAJQM1QW8B42NKMJM4).
#[tokio::test(flavor = "multi_thread")]
async fn a_lower_limit_ends_a_worker_after_its_item_and_a_higher_one_starts_one() {
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
    lead.reads("workers: limit 1 to 2 on a: the rollout starts 1 worker.")
        .await;
    lead.until_workers(2).await;
    let (_, new) = lead.worker(2);
    assert_ne!(new.who(), worker.who());
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
