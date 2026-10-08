//! riff tidies the worktrees of a workers machine by itself, and keeps
//! their build folders in the disk (01M41A118QPQKFAAHGQFFX4F3B to
//! 01M41A11GHP78E2VYN14JSE27P). Each test runs the real `riff` binary:
//! `riff workers host` with a tidy each second, a fake `tmux` with no
//! worker, a fake `gh` that gives the pull requests, and a fake disk in
//! `RIFF_DISK`.

use crate::book;

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::disk::Disk;
use riff::identity;
use riff_core::name::{SessionUri, Who};
use riff_core::wire::RiffState;

/// A tmux with no worker pane.
const FAKE_TMUX: &str = r#"#!/bin/sh
# Outside tmux, riff names its own server: -L riff (see start.rs).
[ "$1" = -L ] && shift 2
case "$1" in
  display-message) echo "@0" ;;
esac
exit 0
"#;

/// `gh pr view --json FIELDS -- BRANCH` prints the file `pr-BRANCH.json`,
/// else fails.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
if [ "$1 $2 $5" = "pr view --" ] && [ -f "$dir/pr-$6.json" ]; then cat "$dir/pr-$6.json"; exit 0; fi
echo "no pull requests found for branch \"$6\"" >&2
exit 1
"#;

/// The bound of each wait. It only ends a test that hangs.
const WAIT: Duration = Duration::from_secs(60);

/// A machine with 8 cores and much free memory.
const MACHINE: &str = "cpu 8x3000MHz, mem 64GB, 60GB available, load 0.00";

/// 10% free: under the mark of the build folders, over the low mark.
const TIGHT: &str = "disk 50GB free of 455GB";

/// 3% free: under the low mark.
const LOW: &str = "disk 16GB free of 455GB";

fn script(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A riff that runs, the lead `l1` on the host `a`, and a clone with an
/// `origin` on the machine `pangolin`.
struct Riff {
    api: Api,
    lead: SessionUri,
    fake: tempfile::TempDir,
    home: tempfile::TempDir,
    root: tempfile::TempDir,
}

impl Riff {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, riff_server::router()).await.unwrap();
        });
        let api = Api::new(&format!("http://{addr}"));
        let fake = tempfile::tempdir().unwrap();
        script(fake.path(), "tmux", FAKE_TMUX);
        script(fake.path(), "gh", FAKE_GH);
        let root = tempfile::tempdir().unwrap();
        let origin = root.path().join("origin.git");
        git(root.path(), &["init", "-q", "--bare", "origin.git"]);
        git(
            root.path(),
            &["clone", "-q", &origin.to_string_lossy(), "main"],
        );
        let main = std::fs::canonicalize(root.path().join("main")).unwrap();
        git(&main, &["commit", "-q", "--allow-empty", "-m", "first"]);
        git(&main, &["push", "-q", "origin", "HEAD"]);
        let place = identity::place_in(&main, "a").unwrap();
        let lead = SessionUri::new(Who::new("mike", Some("l1")).unwrap(), place);
        api.register(&lead).await.unwrap();
        api.set_riff(&lead, RiffState::Running).await.unwrap();
        let r = Riff {
            api,
            lead,
            fake,
            home: tempfile::tempdir().unwrap(),
            root,
        };
        let out = r.riff(&["workers", "limit", "2"]).output().unwrap();
        assert!(out.status.success(), "{out:?}");
        r
    }

    fn main(&self) -> PathBuf {
        std::fs::canonicalize(self.root.path().join("main")).unwrap()
    }

    /// `riff ARGS` in the main clone, as the person on `pangolin` in a
    /// tmux pane.
    fn riff(&self, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
        cmd.args(args)
            .current_dir(self.main())
            .env("PATH", path)
            .env("RIFF_HOME", self.home.path())
            .env("RIFF_SERVER", self.api.base())
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("RIFF_MACHINE", MACHINE)
            .env("RIFF_ON", "1")
            .env("RIFF_TIDY_EVERY", "1")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", "%0")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("RIFF_SESSION")
            .env_remove("RIFF_WORKER");
        cmd
    }

    /// Starts `riff workers host` with the disk `disk`, and waits until
    /// it is in `riff who`: its first clean is done.
    async fn host(&self, disk: Option<&str>) -> Host {
        let mut cmd = self.riff(&["workers", "host", "--claude", "true"]);
        if let Some(disk) = disk {
            cmd.env("RIFF_DISK", disk);
        }
        let out = self.fake.path().join("host.out");
        let child = cmd
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&out).unwrap())
            .stderr(std::fs::File::create(self.fake.path().join("host.err")).unwrap())
            .spawn()
            .unwrap();
        let host = Host(child, out);
        self.until("the host in riff who", || async {
            let who = self.api.who(&self.lead, false).await.ok()?;
            who.iter()
                .any(|s| {
                    s.live
                        && s.uri.place().host() == "pangolin"
                        && s.status
                            .as_ref()
                            .is_some_and(|st| st.status.step.starts_with("workers host"))
                })
                .then_some(())
        })
        .await;
        host
    }

    /// Waits until `check` gives `Some`.
    async fn until<T, F, Fut>(&self, what: &str, mut check: F) -> T
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Option<T>>,
    {
        let start = Instant::now();
        loop {
            if let Some(value) = check().await {
                return value;
            }
            assert!(start.elapsed() < WAIT, "timed out: {what}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The unread text of the lead, when it contains `needle`.
    async fn lead_reads(&self, needle: &str) -> String {
        let start = Instant::now();
        let mut all = String::new();
        loop {
            let inbox = self.api.inbox(&self.lead, None, false).await.unwrap();
            all.push_str(&riff::text::inbox(&inbox, &self.lead));
            if all.contains(needle) {
                return all;
            }
            assert!(start.elapsed() < WAIT, "timed out: {needle}\n{all}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// A worktree of the agent tool, on a new branch.
    fn worktree(&self, name: &str) -> PathBuf {
        let main = self.main();
        let path = main.join(".claude/worktrees").join(name);
        let branch = format!("worktree-{name}");
        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &branch,
                &path.to_string_lossy(),
            ],
        );
        path
    }

    /// A worktree with the session `id` in it. An open watch of the
    /// session makes it live.
    async fn owned(&self, name: &str, id: &str) -> (PathBuf, SessionUri) {
        let path = self.worktree(name);
        let place = identity::place_in(&path, "pangolin").unwrap();
        let session = SessionUri::new(Who::new("mike", Some(id)).unwrap(), place);
        self.api.register(&session).await.unwrap();
        (path, session)
    }
}

/// A `riff workers host` that is killed on drop, and the file of its
/// output.
struct Host(Child, PathBuf);

impl Host {
    fn output(&self) -> String {
        std::fs::read_to_string(&self.1).unwrap_or_default()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A `target` with a file in it, in `tree`.
fn build(tree: &Path) -> PathBuf {
    let target = tree.join("target");
    std::fs::create_dir_all(target.join("debug")).unwrap();
    std::fs::write(target.join("debug/riff"), "a build").unwrap();
    target
}

/// 01M41A118QPQKFAAHGQFFX4F3B: a merged, clean worktree with no live
/// owner goes away at the next tidy of the host, with its branch. The
/// worktree of a live session stays.
#[tokio::test(flavor = "multi_thread")]
async fn a_host_removes_a_merged_worktree_at_its_next_tidy() {
    let r = Riff::new().await;
    let main = r.main();
    let host = r.host(None).await;

    // The worktrees come after the first clean of the host.
    let merged = r.worktree("issue-3");
    git(
        &merged,
        &["commit", "-q", "--allow-empty", "-m", "the work"],
    );
    git(&merged, &["push", "-q", "origin", "HEAD"]);
    let head = git(&merged, &["rev-parse", "HEAD"]).trim().to_owned();
    std::fs::write(
        r.fake.path().join("pr-worktree-issue-3.json"),
        format!(r#"{{"number":40,"state":"MERGED","headRefOid":"{head}"}}"#),
    )
    .unwrap();
    let (owned, session) = r.owned("issue-5", "a5a5").await;
    let _live = Box::pin(r.api.watch(&session).await.unwrap());
    let head5 = git(&owned, &["rev-parse", "HEAD"]).trim().to_owned();
    std::fs::write(
        r.fake.path().join("pr-worktree-issue-5.json"),
        format!(r#"{{"number":41,"state":"MERGED","headRefOid":"{head5}"}}"#),
    )
    .unwrap();

    r.until("the merged worktree goes away", || async {
        (!merged.exists()).then_some(())
    })
    .await;
    assert!(
        git(&main, &["branch", "--list", "worktree-issue-3"])
            .trim()
            .is_empty()
    );
    // The host prints the lines of a tidy after the tidy.
    let out = r
        .until("the line of the removal", || async {
            let out = host.output();
            out.contains("removed with its branch worktree-issue-3: its pull request #40 is merged")
                .then_some(out)
        })
        .await;
    assert!(owned.exists(), "the worktree of a live session stays");
    assert!(out.contains("kept: a live session works in it"), "{out}");
}

/// 01M41A11BB4HAD8595DNSBAZ0D: under 15% of free disk, the host removes
/// the `target` of each worktree with no live owner, and the lead gets a
/// note. The source of that worktree stays, and so does the `target` of
/// a worktree of a live session.
#[tokio::test(flavor = "multi_thread")]
async fn a_host_on_a_tight_disk_removes_the_target_of_a_worktree_with_no_owner() {
    let r = Riff::new().await;
    let free = r.worktree("issue-7");
    std::fs::write(free.join("work.txt"), "committed work").unwrap();
    git(&free, &["add", "work.txt"]);
    git(&free, &["commit", "-q", "-m", "the work"]);
    let free_target = build(&free);
    let (owned, session) = r.owned("issue-8", "b8b8").await;
    let _live = Box::pin(r.api.watch(&session).await.unwrap());
    let owned_target = build(&owned);
    // `target` is not ignored in this clone: ignore it, as the riff
    // repository does, so that the worktree is clean.
    std::fs::write(r.main().join(".git/info/exclude"), "target/\n").unwrap();

    let host = r.host(Some(TIGHT)).await;
    let note = r.lead_reads("riff removed the target of").await;
    assert!(
        note.contains(&format!(
            "pangolin: the disk was under 15% free. riff removed the target of 1 worktree(s) \
             with no live owner, now disk 50GB free of 455GB (10%): {}: {}",
            free.display(),
            riff::text::TARGET_REMOVED
        )),
        "{note}"
    );
    assert!(!free_target.exists());
    assert!(free.join("work.txt").exists(), "the source stays");
    assert!(owned_target.exists(), "the target of a live session stays");
    assert!(!note.contains("under 5%"), "{note}");
    drop(host);
}

/// 01M41A11DX1QRP48YPTDNT67W4 and 01M41A11GHP78E2VYN14JSE27P: under 5%
/// of free disk, `riff workers start` starts no worker and says why, the
/// lead gets one note from the host, the host tells its disk in its
/// status, and `riff workers` shows the disk.
#[tokio::test(flavor = "multi_thread")]
async fn under_the_low_mark_no_worker_starts_and_the_lead_gets_one_note() {
    let r = Riff::new().await;
    let low = Disk::parse(LOW).unwrap();

    let out = r
        .riff(&["workers", "start", "1"])
        .env("RIFF_DISK", LOW)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim(),
        riff::text::workers_disk_low(&low)
    );

    let list = r.riff(&["workers"]).env("RIFF_DISK", LOW).output().unwrap();
    let list = anstream::adapter::strip_str(&stdout(&list)).to_string();
    assert!(
        list.contains(
            "disk 16GB free of 455GB (3%)\nStarts no worker: disk 16GB free of 455GB (3%), \
             under 5%."
        ),
        "{list}"
    );
    let list = r
        .riff(&["workers"])
        .env("RIFF_DISK", TIGHT)
        .output()
        .unwrap();
    let list = stdout(&list);
    assert!(list.contains("disk 50GB free of 455GB (10%)"), "{list}");
    assert!(!list.contains("Starts no worker"), "{list}");

    let _host = r.host(Some(LOW)).await;
    let all = r.lead_reads(&riff::text::disk_low("pangolin", &low)).await;
    // Many tidies, one note.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let inbox = r.api.inbox(&r.lead, None, false).await.unwrap();
    let all = all + &riff::text::inbox(&inbox, &r.lead);
    assert_eq!(all.matches("under 5%").count(), 1, "{all}");
    let who = r.api.who(&r.lead, false).await.unwrap();
    let status = who
        .iter()
        .find_map(|s| riff::host::HostStatus::parse(&s.status.as_ref()?.status.step))
        .unwrap();
    assert_eq!(status.disk, Some(low));
}

/// The book has a how-to with an `sh` block for the disk.
#[test]
fn the_book_has_a_how_to_for_the_free_disk() {
    let page = book::page("how-it-works.md");
    let heading = "### See the free disk of a host";
    let start = page
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("the book has no {heading:?}"));
    let how = &page[start + 1..];
    let how = &how[..how[4..].find("\n### ").map_or(how.len(), |n| n + 4)];
    let commands = book::commands_in(how);
    assert!(commands.iter().any(|c| c == "riff workers"), "{how}");
    book::each_is_real(&commands);
    for text in ["15%", "5%", "10 minutes", "target"] {
        assert!(how.contains(text), "the how-to has no {text:?}: {how}");
    }
}
