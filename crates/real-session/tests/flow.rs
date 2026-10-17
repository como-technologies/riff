//! `real-session` with a fake `gh`, `podman` and `riff`, in a fake home
//! with a real git repository (01M4ZCFGCY301G2ARFMRXV7XDR,
//! 01M4ZCFGF6QN6KZEVMBNW9G4EW, 01M4ZCFGHCS7WJ6CFYBZSX0AT5).
//!
//! Each fake writes its call to the file `log` of its control folder.
//! The fake `riff` also writes its `RIFF_DATA` to the file `data` there,
//! or `none`. The fake `riff daemon` with `RIFF_DATA` makes the file
//! `daemon-runs` there, and runs until SIGTERM. Then it writes
//! `TERM riff daemon` to the log, and removes the file. With no
//! `RIFF_DATA`, it fails: the daemon of the machine is not the daemon of
//! the test. The fake `riff run` fails when no daemon of the test runs,
//! and makes its session folder in `RIFF_DATA`. A file in that folder
//! changes what a fake does:
//!
//! | File | The fake |
//! |---|---|
//! | `rootful` | `podman info` says `false` |
//! | `no-label` | `gh issue edit` fails |
//! | `hold-fails` | `riff plan hold` fails, as for a worker |
//! | `daemon-runs` | `riff daemon status` with `RIFF_DATA` answers: a daemon of the test runs |
//! | `machine-daemon` | `riff daemon status` with no `RIFF_DATA` answers: the daemon of the machine runs |
//! | `daemon-fails` | `riff daemon` ends with 1 at once |
//! | `run-fails` | `riff run` ends with 1 |
//! | `run-waits` | `riff run` waits for a signal |
//! | `run-signaled` | `riff run` ends by SIGTERM, and the test gets no signal |
//! | `stray` | `riff run` writes paths in the home, and changes files that other programs write |
//! | `gitconfig` | `riff run` appends to `~/.gitconfig` |
//! | `others` | `riff run` writes in `~/snap`, in the locks of `claude` and in `~/.claude.json.lock` |
//! | `elsewhere` | `riff run` writes a new folder in `~/.config` |
//! | `workers` | `riff run` builds and commits in a worktree of the other clone ([`Fake::other_clone`]), and makes a new worktree there |
//! | `in-clone` | `riff run` writes a new file in the other clone, outside its worktrees |
//! | `hook` | `riff run` writes a new hook in the git folder of the other clone |
//! | `branch` | `riff run` adds a section `[branch "x"]` to the git config of the other clone and of the clone of the test, as `git push -u` does, and builds in the clone of the test |
//! | `hooks-path` | `riff run` sets `core.hooksPath` in the git config of the other clone and of the clone of the test |
//! | `hook-changed` | `riff run` writes to the hook `pre-push` of the other clone, which is there already |
//! | `no-pull` | `gh pr view` finds no pull request |
//! | `no-gate` | the head commit has no check run |

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use isolated::Isolated;

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
'label create') exit 0 ;;
'issue edit')
  if [ -f "$c/no-label" ]; then echo "could not add the label" >&2; exit 1; fi ;;
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
echo "${RIFF_DATA:-none}" >> "$c/data"
case "$1" in
image) echo "ghcr.io/o/riff-session@sha256:00ff" ;;
plan)
  if [ "$2" = hold ] && [ -f "$c/hold-fails" ]; then
    echo "riff: a worker cannot hold an item. Tell the lead." >&2; exit 1
  fi ;;
daemon)
  if [ "$2" = status ]; then
    if [ -n "$RIFF_DATA" ]; then [ -f "$c/daemon-runs" ]; else [ -f "$c/machine-daemon" ]; fi
    exit
  fi
  if [ -z "$RIFF_DATA" ]; then echo "riff: the fake daemon of the machine runs already" >&2; exit 1; fi
  if [ -f "$c/daemon-fails" ]; then echo "riff: the fake daemon cannot listen" >&2; exit 1; fi
  trap 'echo "TERM riff daemon" >> "$c/log"; rm -f "$c/daemon-runs"; exit 0' TERM
  touch "$c/daemon-runs"
  while :; do sleep 0.05; done ;;
run)
  if [ -z "$RIFF_DATA" ] || [ ! -f "$c/daemon-runs" ]; then
    echo "riff: no riff daemon of the test answers" >&2; exit 1
  fi
  d="$RIFF_DATA/sessions/$2-ID8"
  mkdir -p "$d/clone" "$d/claude"
  echo work > "$d/clone/f"
  if [ -f "$c/stray" ]; then
    touch "$HOME/.cvsignore"
    mkdir -p "$HOME/.claude/projects/-x-$2-ID8-clone" "$HOME/.cache/tool" "$XDG_STATE_HOME/riff"
    touch "$HOME/.claude/projects/other.jsonl" "$HOME/.cache/tool/x" "$XDG_STATE_HOME/riff/x"
    echo '{"numStartups":2}' > "$HOME/.claude.json"
    touch "$HOME/.claude.json.lock"
    echo pwd >> "$HOME/.bash_history"
  fi
  if [ -f "$c/gitconfig" ]; then printf '[alias]\n\tst = status\n' >> "$HOME/.gitconfig"; fi
  if [ -f "$c/others" ]; then
    mkdir -p "$HOME/snap/firefox/common/.mozilla" "$XDG_STATE_HOME/claude/locks"
    touch "$HOME/snap/firefox/common/.mozilla/storage" "$XDG_STATE_HOME/claude/locks/2.1.301.lock"
    touch "$HOME/.claude.json.lock"
  fi
  if [ -f "$c/elsewhere" ]; then mkdir -p "$HOME/.config/tool"; touch "$HOME/.config/tool/x"; fi
  m="$HOME/src/main"
  if [ -f "$c/workers" ]; then
    w="$m/.claude/worktrees/issue-1"
    git -C "$m" worktree add -q -b worktree-issue-2 .claude/worktrees/issue-2
    mkdir -p "$w/target/debug" "$m/.claude/worktrees/issue-2/target"
    touch "$w/target/debug/riff" "$w/new.rs" "$m/.claude/worktrees/issue-2/target/x"
    git -C "$w" add new.rs
    git -C "$w" -c user.name=W -c user.email=w@example.com commit -q -m "WIP: a worker"
  fi
  if [ -f "$c/in-clone" ]; then touch "$m/notes.md"; fi
  if [ -f "$c/hook" ]; then touch "$m/.git/hooks/pre-commit"; fi
  t="$HOME/src/riff"
  if [ -f "$c/branch" ]; then
    for r in "$m" "$t"; do
      git -C "$r" config branch.x.remote origin
      git -C "$r" config branch.x.merge refs/heads/x
    done
    mkdir -p "$t/target/debug"
    touch "$t/target/debug/riff"
  fi
  if [ -f "$c/hooks-path" ]; then
    for r in "$m" "$t"; do git -C "$r" config core.hooksPath /tmp/hooks; done
  fi
  if [ -f "$c/hook-changed" ]; then echo 'curl x' >> "$m/.git/hooks/pre-push"; fi
  if [ -f "$c/run-waits" ]; then touch "$c/running"; sleep 30; fi
  if [ -f "$c/run-signaled" ]; then kill -TERM $$; fi
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
        // A host with claude has its state folder before the test.
        let claude_state = env.home().join(".local/state/claude");
        for dir in [
            &bin,
            &ctl,
            &clone,
            &env.home().join(".claude/projects"),
            &claude_state,
        ] {
            std::fs::create_dir_all(dir).unwrap();
        }
        // The files of the person that are there before the test.
        for (name, text) in [
            (".gitconfig", "[user]\n\tname = Ada\n"),
            (".claude.json", "{\"numStartups\":1}\n"),
            (".bash_history", "ls\n"),
        ] {
            std::fs::write(env.home().join(name), text).unwrap();
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
        for (name, script) in [("gh", GH), ("podman", PODMAN), ("riff", RIFF)] {
            let text = script
                .replace("CTL", &fake.ctl.display().to_string())
                .replace("ID8", ID8);
            isolated::write_program(fake.bin.join(name), text);
        }
        fake
    }

    /// Makes the file `name` in the control folder.
    fn set(&self, name: &str) {
        std::fs::write(self.ctl.join(name), "").unwrap();
    }

    /// Makes another clone of the person, `~/src/main`, with one commit,
    /// the worktree `.claude/worktrees/issue-1` of another session and
    /// the hook `pre-push`, as the main clone of a busy machine when the
    /// test runs from another clone. It gives the clone.
    fn other_clone(&self) -> PathBuf {
        let clone = self.env.home().join("src/main");
        std::fs::create_dir_all(&clone).unwrap();
        std::fs::write(clone.join("README.md"), "main\n").unwrap();
        for args in [
            &["init", "-q"][..],
            &["add", "README.md"],
            &[
                "-c",
                "user.name=W",
                "-c",
                "user.email=w@example.com",
                "commit",
                "-q",
                "-m",
                "first",
            ],
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "worktree-issue-1",
                ".claude/worktrees/issue-1",
            ],
        ] {
            let git = Command::new("git")
                .args(args)
                .current_dir(&clone)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .status()
                .unwrap();
            assert!(git.success(), "git {args:?}");
        }
        // Git makes the folder of the hooks only from its templates.
        std::fs::create_dir_all(clone.join(".git/hooks")).unwrap();
        isolated::write_program(clone.join(".git/hooks/pre-push"), "#!/bin/sh\n");
        clone
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

    /// Starts the test.
    fn spawn(&self, args: &[&str], change: impl FnOnce(&mut Command)) -> Child {
        let mut cmd = self.command(args);
        change(&mut cmd);
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

    /// The `RIFF_DATA` of each call of the fake `riff`, in order, or
    /// `none`.
    fn data(&self) -> Vec<String> {
        std::fs::read_to_string(self.ctl.join("data"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    /// The data folder of the test.
    fn test_data(&self) -> PathBuf {
        self.env.home().join(".local/share/riff/real-session")
    }

    /// The folder of the fake session.
    fn session(&self) -> PathBuf {
        self.test_data()
            .join(format!("sessions/issue-{ISSUE}-{ID8}"))
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The number of commits of the branch `branch` of `clone`.
fn commits(clone: &Path, branch: &str) -> String {
    let out = Command::new("git")
        .args(["rev-list", "--count", branch])
        .current_dir(clone)
        .output()
        .unwrap();
    text(&out.stdout).trim().to_owned()
}

/// The index of the first call that starts with `start`.
fn find(log: &[String], start: &str) -> Option<usize> {
    log.iter().position(|line| line.starts_with(start))
}

/// The clean-up calls of each test that started the daemon.
fn assert_clean(fake: &Fake) {
    let log = &fake.log();
    let stop = find(log, "TERM riff daemon");
    let close = find(log, "gh issue close 801 --reason not planned --comment ");
    let free = find(log, "riff plan free verify-issue-801");
    assert!(stop.is_some() && stop < close && free.is_some(), "{log:#?}");
    assert!(!fake.ctl.join("daemon-runs").exists(), "{log:#?}");
    assert!(find(log, "gh run cancel 5").is_some(), "{log:#?}");
    assert!(find(log, "gh run cancel 6").is_none(), "{log:#?}");
    let session = fake.session();
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
             the Gate is queued, no change in the home"
        ),
        "{stdout}"
    );
    for step in [
        "step 1 of 10: check the host",
        "step 5 of 10: start the daemon",
        "step 6 of 10: run the item",
        "step 10 of 10: wait for the Gate",
    ] {
        assert!(stdout.contains(step), "{step}\n{stdout}");
    }

    let log = fake.log();
    let order: Vec<usize> = [
        "podman info --format {{.Host.Security.Rootless}}",
        "gh auth status",
        "riff image",
        "gh issue create --title Real-session test ",
        "gh label create no-verify --description riff offers no verify of this item --color \
         ededed --force",
        "gh issue edit 801 --add-label no-verify",
        "riff plan hold verify-issue-801 the real-session test makes and closes this item",
        "riff daemon status",
        "riff daemon",
        "riff run issue-801 --image ghcr.io/o/riff-session@sha256:00ff",
        "gh pr view worktree-issue-801 --json number,url,headRefOid,state",
        "gh pr diff 900 --color never",
        "gh api repos/{owner}/{repo}/commits/0123abcd/check-runs?check_name=Gate",
        "gh pr close 900 --delete-branch --comment ",
        "gh run list --branch worktree-issue-801 --json databaseId,status",
        "gh issue close 801",
    ]
    .iter()
    .map(|start| {
        let at =
            |line: &String| line.starts_with(start) && (*start != "riff daemon" || line == start);
        log.iter()
            .position(at)
            .unwrap_or_else(|| panic!("{start}\n{log:#?}"))
    })
    .collect();
    assert!(order.is_sorted(), "{order:?}\n{log:#?}");
    assert!(
        log.iter()
            .any(|l| l.contains("--milestone Backlog --body This is a scratch item")),
        "{log:#?}"
    );
    assert_clean(&fake);
}

/// A worker runs the test: its hold of the verify fails
/// (01M43GSGGY0QMB5D5EH92M6ZFP). The label `no-verify` still keeps the
/// verify of the scratch item from the workers, so step 4 passes with a
/// warning, and the test goes on (01M5157V34H7FWFH6S2DRQXRMP). With no
/// hold, the clean-up frees nothing.
#[test]
fn a_worker_that_cannot_hold_keeps_the_verify_with_the_label() {
    let fake = Fake::new();
    fake.set("hold-fails");
    let out = fake.run(&[]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(
        stdout.contains("step 4 of 10: keep its verify from the workers"),
        "{stdout}"
    );
    assert!(
        stdout.contains("  issue #801 has the label no-verify\n"),
        "{stdout}"
    );
    for part in [
        "  warning: riff did not hold verify-issue-801, and the label no-verify keeps its verify \
         from the workers: ",
        "/riff plan hold verify-issue-801 the real-session test makes and closes this item: no \
         verify ended with exit status: 1: riff: a worker cannot hold an item. Tell the lead.\n",
    ] {
        assert!(stdout.contains(part), "{part}\n{stdout}");
    }
    let log = fake.log();
    let label = find(&log, "gh issue edit 801 --add-label no-verify");
    let run = find(&log, "riff run issue-801");
    assert!(label.is_some() && label < run, "{log:#?}");
    assert!(find(&log, "riff plan free").is_none(), "{log:#?}");
}

/// When `gh` cannot put the label on the issue, step 4 fails: no
/// `riff run` starts, and the test cleans up.
#[test]
fn no_label_fails_step_4_and_runs_no_item() {
    let fake = Fake::new();
    fake.set("no-label");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(
            "real-session: FAIL at step 4 of 10, keep its verify from the workers: gh issue edit \
             801 --add-label no-verify ended with exit status: 1: could not add the label"
        ),
        "{stderr}"
    );
    let log = fake.log();
    assert!(find(&log, "riff plan hold").is_none(), "{log:#?}");
    assert!(find(&log, "riff run").is_none(), "{log:#?}");
    assert!(
        find(&log, "gh issue close 801 --reason not planned").is_some(),
        "{log:#?}"
    );
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
            "real-session: FAIL at step 6 of 10, run the item: riff run issue-801 ended with \
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
    assert_clean(&fake);
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
        "real-session: FAIL at step 7 of 10, check the home: the home has 2 new paths outside \
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
    // Nor do a change of .claude.json that does not name the session, its
    // lock, and a change of a file that the check does not guard.
    assert!(!stderr.contains(".claude.json"), "{stderr}");
    assert!(!stderr.contains(".bash_history"), "{stderr}");
    assert!(!stderr.contains("(changed)"), "{stderr}");
    let log = fake.log();
    assert!(
        find(&log, "gh pr close 900 --delete-branch").is_some(),
        "{log:#?}"
    );
    assert_clean(&fake);
}

/// A write into a file that is there already, and that riff must not
/// write, fails the home check (01M51K051FR0WN71NJXNA86XWY).
#[test]
fn a_change_to_the_gitconfig_fails_the_check_of_the_home() {
    let fake = Fake::new();
    fake.set("gitconfig");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    let shown = format!(
        "real-session: FAIL at step 7 of 10, check the home: the home has 1 changed file \
         outside the folders of riff:\n  {} (changed)\nWhen another program wrote a path, \
         leave out the path or its folder with --skip PATH, and run the test again.\n",
        fake.env.home().join(".gitconfig").display()
    );
    assert!(stderr.contains(&shown), "{shown}\n{stderr}");
    let log = fake.log();
    assert!(find(&log, "gh pr diff").is_none(), "{log:#?}");
    assert_clean(&fake);
}

/// The snap apps, the locks of a `claude` on the host and the lock of
/// `.claude.json` write while the test runs. With no `--skip`, a new
/// file there does not fail the home check, and a new file in another
/// place does (01M51MQB46AYY39YSAY18NMD8T).
#[test]
fn the_snap_apps_and_the_locks_of_claude_do_not_count_and_another_place_does() {
    let fake = Fake::new();
    fake.set("others");
    let out = fake.run(&[]);
    assert!(
        out.status.success(),
        "{}\n{}",
        text(&out.stdout),
        text(&out.stderr)
    );

    let fake = Fake::new();
    fake.set("others");
    fake.set("elsewhere");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    let shown = format!(
        "real-session: FAIL at step 7 of 10, check the home: the home has 1 new path outside \
         the folders of riff:\n  {}\nWhen another program",
        fake.env.home().join(".config/tool").display()
    );
    assert!(stderr.contains(&shown), "{shown}\n{stderr}");
    assert_clean(&fake);
}

/// Workers build and commit in the worktrees of another clone of the
/// person while the test runs, and make a new worktree there. With no
/// `--skip`, this does not fail the home check. A new file in that clone
/// outside its worktrees fails it, and so does a new hook in its git
/// folder (01M51SPS50QQ9Y5C6VE26A18FZ).
#[test]
fn the_worktrees_of_other_sessions_do_not_count_and_the_rest_of_their_clone_does() {
    let fake = Fake::new();
    let clone = fake.other_clone();
    fake.set("workers");
    let out = fake.run(&[]);
    assert!(
        out.status.success(),
        "{}\n{}",
        text(&out.stdout),
        text(&out.stderr)
    );
    // The commit and the new worktree wrote in the git folder of the
    // clone.
    assert_eq!(commits(&clone, "worktree-issue-1"), "2");
    assert!(clone.join(".git/worktrees/issue-2").is_dir());
    assert!(
        clone
            .join(".claude/worktrees/issue-1/target/debug/riff")
            .exists()
    );

    let fake = Fake::new();
    let clone = fake.other_clone();
    fake.set("workers");
    fake.set("in-clone");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    let shown = format!(
        "real-session: FAIL at step 7 of 10, check the home: the home has 1 new path outside \
         the folders of riff:\n  {}\nWhen another program",
        clone.join("notes.md").display()
    );
    assert!(stderr.contains(&shown), "{shown}\n{stderr}");
    assert_clean(&fake);

    let fake = Fake::new();
    let clone = fake.other_clone();
    fake.set("workers");
    fake.set("hook");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    let shown = format!(
        "real-session: FAIL at step 7 of 10, check the home: the home has 1 new path outside \
         the folders of riff:\n  {}\nWhen another program",
        clone.join(".git/hooks/pre-commit").display()
    );
    assert!(stderr.contains(&shown), "{shown}\n{stderr}");
    assert_clean(&fake);
}

/// Each `git push -u` of a session writes a section `[branch "NAME"]` in
/// the git config of its clone. With no `--skip`, this does not fail the
/// home check, and nor does the build of the test in its own clone. A new
/// `core.hooksPath` in the git config of a clone fails it, also in the
/// clone of the test, and the message names the file and the key. A
/// write to a hook that is there already fails it too
/// (01M51X6JA3W2CMV1S5VZEY7FC3).
#[test]
fn a_new_branch_section_does_not_count_and_a_new_hooks_path_does() {
    let fake = Fake::new();
    let clone = fake.other_clone();
    fake.set("branch");
    let out = fake.run(&[]);
    assert!(
        out.status.success(),
        "{}\n{}",
        text(&out.stdout),
        text(&out.stderr)
    );
    for repo in [&clone, &fake.clone] {
        let config = std::fs::read_to_string(repo.join(".git/config")).unwrap();
        assert!(config.contains("[branch \"x\"]"), "{config}");
    }

    let fake = Fake::new();
    let clone = fake.other_clone();
    fake.set("branch");
    fake.set("hooks-path");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    let shown = format!(
        "real-session: FAIL at step 7 of 10, check the home: the home has 2 changed files \
         outside the folders of riff:\n  {} (changed: core.hookspath)\n  {} (changed: \
         core.hookspath)\nWhen another program",
        clone.join(".git/config").display(),
        fake.clone.join(".git/config").display()
    );
    assert!(stderr.contains(&shown), "{shown}\n{stderr}");
    assert_clean(&fake);

    let fake = Fake::new();
    let clone = fake.other_clone();
    fake.set("hook-changed");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    let shown = format!(
        "real-session: FAIL at step 7 of 10, check the home: the home has 1 changed file \
         outside the folders of riff:\n  {} (changed)\nWhen another program",
        clone.join(".git/hooks/pre-push").display()
    );
    assert!(stderr.contains(&shown), "{shown}\n{stderr}");
    assert_clean(&fake);
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
            "FAIL at step 8 of 10, find the pull request: the branch worktree-issue-801 has no \
             open pull request"
        ),
        "{stderr}"
    );
    assert_clean(&fake);
}

#[test]
fn a_gate_that_does_not_start_fails_the_test() {
    let fake = Fake::new();
    fake.set("no-gate");
    let out = fake.run(&["--gate-wait", "0"]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(
            "FAIL at step 10 of 10, wait for the Gate: the Gate did not start on 0123abc in 0 s"
        ),
        "{stderr}"
    );
    assert_clean(&fake);
}

/// The daemon of the machine runs, for example `riff.service`. The test
/// runs its own daemon beside it, and passes. It never stops the daemon
/// of the machine, and never talks to it: each `riff` of the test has the
/// data folder of the test as `RIFF_DATA` (01M53YP9DATAD95EZEZ6R75WEF).
#[test]
fn the_test_passes_beside_the_daemon_of_the_machine_and_leaves_it_running() {
    let fake = Fake::new();
    fake.set("machine-daemon");
    let out = fake.run(&[]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(
        stdout.contains("real-session: PASS: issue #801"),
        "{stdout}"
    );
    let data = fake.test_data().display().to_string();
    let started = stdout
        .lines()
        .find(|line| line.starts_with("  riff daemon, pid "))
        .unwrap_or_default();
    assert!(started.ends_with(&format!(", in {data}")), "{stdout}");
    assert!(fake.ctl.join("machine-daemon").exists());
    let log = fake.log();
    let calls = log.iter().filter(|line| line.starts_with("riff ")).count();
    let values = fake.data();
    assert_eq!(values.len(), calls, "{values:#?}\n{log:#?}");
    assert!(values.iter().all(|value| *value == data), "{values:#?}");
    assert_clean(&fake);
}

/// A daemon of the test answers before the start: another test runs, or
/// a test that a second signal stopped left its daemon. Step 5 fails,
/// and the test does not stop that daemon. The message names no step
/// for the daemon of the machine.
#[test]
fn a_daemon_of_the_test_that_runs_already_fails_the_step_and_the_test_does_not_stop_it() {
    let fake = Fake::new();
    fake.set("daemon-runs");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(
            "FAIL at step 5 of 10, start the daemon: a riff daemon of the test runs already:"
        ),
        "{stderr}"
    );
    assert!(!stderr.contains("systemctl"), "{stderr}");
    let log = fake.log();
    assert!(!log.iter().any(|l| l == "riff daemon"), "{log:#?}");
    assert!(find(&log, "riff run").is_none(), "{log:#?}");
    assert!(find(&log, "TERM riff daemon").is_none(), "{log:#?}");
    assert!(fake.ctl.join("daemon-runs").exists());
    assert!(
        find(&log, "gh issue close 801 --reason not planned").is_some(),
        "{log:#?}"
    );
    assert!(
        find(&log, "riff plan free verify-issue-801").is_some(),
        "{log:#?}"
    );
}

#[test]
fn a_daemon_that_ends_before_it_answers_fails_the_step() {
    let fake = Fake::new();
    fake.set("daemon-fails");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(
            "FAIL at step 5 of 10, start the daemon: riff daemon ended with exit status: 1 \
             before it answered: see its message above"
        ),
        "{stderr}"
    );
    // The message of the daemon is in the output of the test.
    assert!(
        stderr.contains("riff: the fake daemon cannot listen"),
        "{stderr}"
    );
    let log = fake.log();
    assert!(find(&log, "riff run").is_none(), "{log:#?}");
    assert!(
        find(&log, "gh issue close 801 --reason not planned").is_some(),
        "{log:#?}"
    );
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
            "FAIL at step 1 of 10, check the host: podman does not run rootless: podman info \
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
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "{:#?}",
            fake.log()
        );
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
    // The daemon has a process group of its own: Ctrl-C did not reach
    // it, and the clean-up stopped it.
    assert!(
        text(&out.stdout).contains("  stop the riff daemon: done"),
        "{}",
        text(&out.stdout)
    );
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("FAIL at step 6 of 10, run the item: a signal stopped the test"),
        "{stderr}"
    );
    let log = fake.log();
    assert!(find(&log, "gh pr diff").is_none(), "{log:#?}");
    assert_clean(&fake);
}

#[test]
fn a_child_that_ends_by_a_signal_counts_as_a_signal() {
    let fake = Fake::new();
    // The race of Ctrl-C, made certain: riff run ends by the signal, and
    // the test never sees one.
    fake.set("run-signaled");
    let out = fake.run(&[]);
    assert_eq!(out.status.code(), Some(130), "{}", text(&out.stdout));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("FAIL at step 6 of 10, run the item: a signal stopped the test"),
        "{stderr}"
    );
    let log = fake.log();
    assert!(find(&log, "gh pr diff").is_none(), "{log:#?}");
    assert_clean(&fake);
}

#[test]
fn no_riff_binary_is_a_usage_error() {
    let fake = Fake::new();
    let out = fake
        .env
        .command(env!("CARGO_BIN_EXE_real-session"))
        .arg("--gate-wait")
        .arg("1")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("usage: real-session --riff PATH"));
}
