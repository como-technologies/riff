//! `real-session` with a fake `gh`, `podman` and `riff`, in a fake home
//! with a real git repository (01M4ZCFGCY301G2ARFMRXV7XDR,
//! 01M4ZCFGF6QN6KZEVMBNW9G4EW, 01M4ZCFGHCS7WJ6CFYBZSX0AT5).
//!
//! Each fake writes its call to the file `log` of its control folder.
//! A file in that folder changes what a fake does:
//!
//! | File | The fake |
//! |---|---|
//! | `rootful` | `podman info` says `false` |
//! | `run-fails` | `riff run` ends with 1 |
//! | `run-waits` | `riff run` waits for a signal |
//! | `stray` | `riff run` writes paths in the home |
//! | `no-pull` | `gh pr view` finds no pull request |
//! | `no-gate` | the head commit has no check run |

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use isolated::Isolated;

use crate::SPAWN;

/// The issue that the fake `gh issue create` makes.
const ISSUE: u64 = 801;

/// The `ID8` of the fake session.
const ID8: &str = "1a2b3c4d";

/// One test: a fake home with a clone, the fakes, and their control
/// folder.
struct Fake {
    env: Isolated,
    bin: PathBuf,
    ctl: PathBuf,
    clone: PathBuf,
}

const GH: &str = r#"#!/bin/sh
c='CTL'
echo "gh $*" >> "$c/log"
case "$1 $2" in
'auth status') exit 0 ;;
'issue create')
  while [ $# -gt 0 ]; do
    if [ "$1" = --body ]; then printf '%s' "$2" > "$c/body"; fi
    shift
  done
  echo "https://github.com/o/r/issues/801" ;;
'issue close') exit 0 ;;
'pr view')
  if [ -f "$c/no-pull" ]; then echo "no pull requests found for branch \"$3\"" >&2; exit 1; fi
  if [ -f "$c/closed" ]; then state=CLOSED; else state=OPEN; fi
  echo "{\"number\":900,\"url\":\"https://github.com/o/r/pull/900\",\"headRefOid\":\"0123abcd\",\"state\":\"$state\"}" ;;
'pr diff')
  echo "+++ b/docs/src/riff-3.md"
  grep '^Real-session test ' "$c/body" | sed 's/^/+/' ;;
'pr close') touch "$c/closed" ;;
'run list') echo '[{"databaseId":5,"status":"in_progress"},{"databaseId":6,"status":"completed"}]' ;;
'run cancel') exit 0 ;;
'api '*check-runs*)
  if [ -f "$c/no-gate" ]; then echo '{"total_count":0,"check_runs":[]}'
  else echo '{"total_count":1,"check_runs":[{"name":"Gate","status":"queued"}]}'; fi ;;
'api -X') exit 0 ;;
*) echo "unknown: $*" >&2; exit 1 ;;
esac
"#;

const PODMAN: &str = r#"#!/bin/sh
c='CTL'
echo "podman $*" >> "$c/log"
if [ -f "$c/rootful" ]; then echo false; else echo true; fi
"#;

const RIFF: &str = r#"#!/bin/sh
c='CTL'
echo "riff $*" >> "$c/log"
case "$1" in
image) echo "ghcr.io/o/riff-session@sha256:00ff" ;;
plan) exit 0 ;;
run)
  d="$XDG_DATA_HOME/riff/sessions/$2-ID8"
  mkdir -p "$d/clone" "$d/claude"
  echo work > "$d/clone/f"
  if [ -f "$c/stray" ]; then
    touch "$HOME/.cvsignore"
    mkdir -p "$HOME/.claude/projects/-x-$2-ID8-clone" "$HOME/.cache/tool" "$XDG_STATE_HOME/riff"
    touch "$HOME/.claude/projects/other.jsonl" "$HOME/.cache/tool/x" "$XDG_STATE_HOME/riff/x"
  fi
  if [ -f "$c/run-waits" ]; then touch "$c/running"; sleep 30; fi
  if [ -f "$c/run-fails" ]; then exit 1; fi ;;
*) echo "unknown: $*" >&2; exit 1 ;;
esac
"#;

impl Fake {
    fn new() -> Fake {
        let env = Isolated::new();
        let bin = env.path().join("bin");
        let ctl = env.path().join("ctl");
        let clone = env.home().join("src/riff");
        for dir in [&bin, &ctl, &clone, &env.home().join(".claude/projects")] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let git = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&clone)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(git.success());
        let fake = Fake {
            env,
            bin,
            ctl,
            clone,
        };
        let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        for (name, script) in [("gh", GH), ("podman", PODMAN), ("riff", RIFF)] {
            let path = fake.bin.join(name);
            let text = script
                .replace("CTL", &fake.ctl.display().to_string())
                .replace("ID8", ID8);
            std::fs::write(&path, text).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        fake
    }

    /// Makes the file `name` in the control folder.
    fn set(&self, name: &str) {
        std::fs::write(self.ctl.join(name), "").unwrap();
    }

    /// The command of the test in the clone, with the fakes first on the
    /// `PATH`.
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = self.env.command(env!("CARGO_BIN_EXE_real-session"));
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(&path));
        cmd.env("PATH", std::env::join_paths(paths).unwrap())
            .env_remove("RIFF_HOME")
            .env("XDG_CACHE_HOME", self.env.home().join(".cache"))
            .arg("--riff")
            .arg(self.bin.join("riff"))
            .args(args)
            .current_dir(&self.clone)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    /// Starts the test under [`SPAWN`].
    fn spawn(&self, args: &[&str], change: impl FnOnce(&mut Command)) -> Child {
        let mut cmd = self.command(args);
        change(&mut cmd);
        let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        cmd.spawn().unwrap()
    }

    /// Runs the test to its end.
    fn run(&self, args: &[&str]) -> Output {
        self.spawn(args, |_| {}).wait_with_output().unwrap()
    }

    /// Each call of the fakes, in order.
    fn log(&self) -> Vec<String> {
        std::fs::read_to_string(self.ctl.join("log"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    /// The folder of the fake session.
    fn session(&self) -> PathBuf {
        self.env
            .home()
            .join(format!(".local/share/riff/sessions/issue-{ISSUE}-{ID8}"))
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The index of the first call that starts with `start`.
fn find(log: &[String], start: &str) -> Option<usize> {
    log.iter().position(|line| line.starts_with(start))
}

/// The clean-up calls of each test that made the scratch issue.
fn assert_clean(log: &[String], session: &Path) {
    let close = find(log, "gh issue close 801 --reason not planned --comment ");
    let free = find(log, "riff plan free verify-issue-801");
    assert!(close.is_some() && free.is_some(), "{log:#?}");
    assert!(find(log, "gh run cancel 5").is_some(), "{log:#?}");
    assert!(find(log, "gh run cancel 6").is_none(), "{log:#?}");
    assert!(!session.exists(), "{}", session.display());
}

#[test]
fn a_pass_runs_each_step_in_order_and_cleans_up() {
    let fake = Fake::new();
    let out = fake.run(&[]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(
        stdout.contains(
            "real-session: PASS: issue #801, pull request #900 (https://github.com/o/r/pull/900), \
             the Gate is queued, no new path in the home"
        ),
        "{stdout}"
    );
    for step in [
        "step 1 of 9: check the host",
        "step 5 of 9: run the item",
        "step 9 of 9: wait for the Gate",
    ] {
        assert!(stdout.contains(step), "{step}\n{stdout}");
    }

    let log = fake.log();
    let order: Vec<usize> = [
        "podman info --format {{.Host.Security.Rootless}}",
        "gh auth status",
        "riff image",
        "gh issue create --title Real-session test ",
        "riff plan hold verify-issue-801 the real-session test makes and closes this item",
        "riff run issue-801 --image ghcr.io/o/riff-session@sha256:00ff",
        "gh pr view worktree-issue-801 --json number,url,headRefOid,state",
        "gh pr diff 900 --color never",
        "gh api repos/{owner}/{repo}/commits/0123abcd/check-runs?check_name=Gate",
        "gh pr close 900 --delete-branch --comment ",
        "gh run list --branch worktree-issue-801 --json databaseId,status",
        "gh issue close 801",
    ]
    .iter()
    .map(|start| find(&log, start).unwrap_or_else(|| panic!("{start}\n{log:#?}")))
    .collect();
    assert!(order.is_sorted(), "{order:?}\n{log:#?}");
    assert!(
        log.iter()
            .any(|l| l.contains("--milestone Backlog --body This is a scratch item")),
        "{log:#?}"
    );
    assert_clean(&log, &fake.session());
}

#[test]
fn a_failed_step_names_the_step_stops_the_test_and_cleans_up() {
    let fake = Fake::new();
    fake.set("run-fails");
    fake.set("no-pull");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(
            "real-session: FAIL at step 5 of 9, run the item: riff run issue-801 ended with \
             exit status: 1"
        ),
        "{stderr}"
    );
    let log = fake.log();
    // No step after the failed one runs.
    assert!(find(&log, "gh pr diff").is_none(), "{log:#?}");
    // With no pull request, the clean-up deletes the branch.
    assert!(
        find(
            &log,
            "gh api -X DELETE repos/{owner}/{repo}/git/refs/heads/worktree-issue-801"
        )
        .is_some(),
        "{log:#?}"
    );
    assert!(find(&log, "gh pr close").is_none(), "{log:#?}");
    assert_clean(&log, &fake.session());
}

#[test]
fn a_new_path_in_the_home_fails_the_check_of_the_home() {
    let fake = Fake::new();
    fake.set("stray");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    let home = fake.env.home();
    let shown = format!(
        "real-session: FAIL at step 6 of 9, check the home: the home has 2 new paths outside \
         the folders of riff:\n  {}\n  {}\n",
        home.join(".claude/projects/-x-issue-801-1a2b3c4d-clone")
            .display(),
        home.join(".cvsignore").display()
    );
    assert!(stderr.contains(&shown), "{shown}\n{stderr}");
    // The other session, the cache and the folder of riff do not count.
    assert!(!stderr.contains("other.jsonl"), "{stderr}");
    assert!(!stderr.contains(".cache"), "{stderr}");
    assert!(!stderr.contains(".local/state"), "{stderr}");
    let log = fake.log();
    assert!(find(&log, "gh pr close 900 --delete-branch").is_some(), "{log:#?}");
    assert_clean(&log, &fake.session());
}

#[test]
fn a_folder_of_skip_does_not_count() {
    let fake = Fake::new();
    fake.set("stray");
    let home = fake.env.home();
    std::fs::write(home.join(".cvsignore"), "").unwrap();
    let skip = home.join(".claude/projects");
    let out = fake.run(&["--skip", &skip.display().to_string()]);
    assert!(
        out.status.success(),
        "{}\n{}",
        text(&out.stdout),
        text(&out.stderr)
    );
}

#[test]
fn no_pull_request_fails_the_test() {
    let fake = Fake::new();
    fake.set("no-pull");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(
            "FAIL at step 7 of 9, find the pull request: the branch worktree-issue-801 has no \
             open pull request"
        ),
        "{stderr}"
    );
    assert_clean(&fake.log(), &fake.session());
}

#[test]
fn a_gate_that_does_not_start_fails_the_test() {
    let fake = Fake::new();
    fake.set("no-gate");
    let out = fake.run(&["--gate-wait", "0"]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("FAIL at step 9 of 9, wait for the Gate: the Gate did not start on 0123abc in 0 s"),
        "{stderr}"
    );
    assert_clean(&fake.log(), &fake.session());
}

#[test]
fn a_failed_check_of_the_host_makes_no_item() {
    let fake = Fake::new();
    fake.set("rootful");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(
            "FAIL at step 1 of 9, check the host: podman does not run rootless: podman info \
             says \"false\""
        ),
        "{stderr}"
    );
    let log = fake.log();
    assert!(find(&log, "gh issue").is_none(), "{log:#?}");
    assert!(!text(&out.stdout).contains("clean up"));
}

#[test]
fn a_signal_stops_the_test_and_it_cleans_up() {
    let fake = Fake::new();
    fake.set("run-waits");
    let child = fake.spawn(&[], |cmd| {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    });
    let start = Instant::now();
    while !fake.ctl.join("running").exists() {
        assert!(start.elapsed() < Duration::from_secs(20), "{:#?}", fake.log());
        std::thread::sleep(Duration::from_millis(20));
    }
    // Ctrl-C: the signal goes to each process of the group.
    let kill = Command::new("kill")
        .args(["-INT", "--", &format!("-{}", child.id())])
        .status()
        .unwrap();
    assert!(kill.success());
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(130), "{}", text(&out.stdout));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("FAIL at step 5 of 9, run the item: a signal stopped the test"),
        "{stderr}"
    );
    let log = fake.log();
    assert!(find(&log, "gh pr diff").is_none(), "{log:#?}");
    assert_clean(&log, &fake.session());
}

#[test]
fn no_riff_binary_is_a_usage_error() {
    let fake = Fake::new();
    let out = {
        let mut cmd = fake.env.command(env!("CARGO_BIN_EXE_real-session"));
        cmd.arg("--gate-wait").arg("1");
        let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        cmd.output().unwrap()
    };
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("usage: real-session --riff PATH"));
}
