//! The limits of the workers of a machine (01M3WFYZRK5CT22GJW6ZHYT9CC to
//! 01M3WFZ03Z9Y60HPHJJ9ZE6AQZ). A fake `claude`, a fake `tmux`, a fake
//! `systemctl` and a fake `systemd-run` run in place of the real ones.
//! No test changes a setting or a slice of the machine.

mod book;

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use riff::api::Api;
use riff_core::name::SessionUri;

const LEAD: &str = "riff://mike@pangolin/como-technologies/riff?session=lead1";

/// pangolin: 16 cores and 30 GB, with 24 GB available.
const PANGOLIN: &str = "cpu 16x4500MHz, mem 30GB, 24GB available, load 0.50";

/// A fake `tmux`: it writes each call to the file `tmux.log`, and has
/// no worker panes.
const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/tmux.log"
case "$1" in
  display-message) echo "@0" ;;
  new-window) echo "@7 %1" ;;
  split-window) echo "%2" ;;
esac
exit 0
"#;

/// A fake `systemctl` of a machine with systemd: it writes each call to
/// the file `systemctl.log`.
const FAKE_SYSTEMCTL: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$(dirname "$0")/systemctl.log"
"#;

/// A fake `systemctl` of a machine with no systemd user manager.
const NO_SYSTEMD: &str = r#"#!/bin/sh
echo "Failed to connect to user scope bus via local transport: No such file or directory" >&2
exit 1
"#;

/// A fake `systemd-run`: it writes its options to the file
/// `systemd-run.log`, then runs the command after `--`.
const FAKE_SYSTEMD_RUN: &str = r#"#!/bin/sh
dir=$(dirname "$0")
options=""
while [ "$1" != "--" ]; do options="$options $1"; shift; done
shift
echo "$options" >> "$dir/systemd-run.log"
exec "$@"
"#;

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A machine: a main clone of `como-technologies/riff`, a directory
/// `bin` for the fakes, first on `PATH`, and its own settings.
struct Machine {
    root: tempfile::TempDir,
    server: String,
}

impl Machine {
    fn new(server: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        for dir in ["main", "bin", "home"] {
            std::fs::create_dir(root.path().join(dir)).unwrap();
        }
        let m = Machine {
            root,
            server: server.into(),
        };
        let main = m.main();
        git(&main, &["init", "-q"]);
        git(
            &main,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/como-technologies/riff.git",
            ],
        );
        git(&main, &["commit", "-q", "--allow-empty", "-m", "x"]);
        script(&m.bin(), "tmux", FAKE_TMUX);
        m
    }

    fn main(&self) -> PathBuf {
        std::fs::canonicalize(self.root.path().join("main")).unwrap()
    }

    fn bin(&self) -> PathBuf {
        self.root.path().join("bin")
    }

    fn log(&self, name: &str) -> String {
        read(&self.bin().join(name))
    }

    /// `riff ARGS` in the main clone, as a person in a tmux pane.
    fn riff(&self, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.bin().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
        cmd.args(args)
            .current_dir(self.main())
            .env("PATH", path)
            .env("RIFF_HOME", self.root.path().join("home"))
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("RIFF_MACHINE", PANGOLIN)
            .env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", "%0");
        cmd
    }

    fn run(&self, args: &[&str]) -> Output {
        self.riff(args).output().unwrap()
    }

    /// `riff workers ARGS`, which must pass. Returns its stdout.
    fn workers(&self, args: &[&str]) -> String {
        let mut all = vec!["workers"];
        all.extend(args);
        let out = self.run(&all);
        assert!(out.status.success(), "riff workers {args:?}: {out:?}");
        stdout(&out)
    }

    /// A fake `claude` that runs `body`.
    fn claude(&self, body: &str) -> PathBuf {
        script(&self.bin(), "claude", &format!("#!/bin/sh\n{body}\n"))
    }

    /// `riff workers run CLAUDE` as the worker session `w1` in the pane
    /// `%5`, with `vars`.
    fn wrapper(&self, claude: &Path, vars: &[(&str, &str)]) -> Output {
        self.riff(&["workers", "run"])
            .arg(claude)
            .arg("Join the riff.")
            .env("RIFF_SESSION", "w1")
            .env("TMUX_PANE", "%5")
            .envs(vars.iter().copied())
            .output()
            .unwrap()
    }
}

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// The nice value of this test process.
fn nice_here() -> u8 {
    let out = Command::new("nice").output().unwrap();
    stdout(&out).trim().parse().unwrap()
}

/// 01M3WFYZRK5CT22GJW6ZHYT9CC: the wrapper gives `claude` the cores
/// divided by the worker limit in `CARGO_BUILD_JOBS` and
/// `RUST_TEST_THREADS`, and 2 or more. The setting replaces the number.
#[test]
fn the_wrapper_gives_claude_the_jobs_of_a_worker() {
    let m = Machine::new(isolated::DEAD_SERVER);
    let seen = m.bin().join("seen");
    let claude = m.claude(&format!(
        "echo \"$CARGO_BUILD_JOBS $RUST_TEST_THREADS\" > '{}'",
        seen.display()
    ));
    // 16 cores and 4 workers: 4 jobs each.
    m.workers(&["limit", "4"]);
    let out = m.wrapper(&claude, &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(read(&seen).trim(), "4 4");
    // 16 cores and 16 workers: 2 or more.
    m.workers(&["limit", "16"]);
    m.wrapper(&claude, &[]);
    assert_eq!(read(&seen).trim(), "2 2");
    // The setting wins.
    m.workers(&["jobs", "6"]);
    m.wrapper(&claude, &[]);
    assert_eq!(read(&seen).trim(), "6 6");
    // The variables of the person do not win over the limit.
    m.wrapper(&claude, &[("CARGO_BUILD_JOBS", "64")]);
    assert_eq!(read(&seen).trim(), "6 6");
}

/// 01M3ZV0QSFVCHRSEKYK57B88VA: the wrapper starts `claude` with no
/// variable of a context, also when its tmux server has one. So the
/// clear never stops `claude`.
#[test]
fn the_wrapper_starts_claude_with_no_variable_of_a_context() {
    let m = Machine::new(isolated::DEAD_SERVER);
    let seen = m.bin().join("seen");
    let claude = m.claude(&format!(
        "echo \"pid=${{CLAUDE_PID-none}}\" > '{}'",
        seen.display()
    ));
    let out = m.wrapper(&claude, &[("CLAUDE_PID", "4242")]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(read(&seen).trim(), "pid=none");
}

/// 01M3WFYZTX05CGDP2NQF9B356K: the wrapper starts `claude` with nice 10.
/// The setting changes the value, and 0 turns it off.
#[test]
fn the_wrapper_starts_claude_with_nice() {
    let m = Machine::new(isolated::DEAD_SERVER);
    let seen = m.bin().join("seen");
    let claude = m.claude(&format!("nice > '{}'", seen.display()));
    let base = nice_here();
    let nice = |add: u8| (base + add).min(19).to_string();

    let out = m.wrapper(&claude, &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(read(&seen).trim(), nice(10));

    m.workers(&["nice", "3"]);
    m.wrapper(&claude, &[]);
    assert_eq!(read(&seen).trim(), nice(3));

    m.workers(&["nice", "0"]);
    m.wrapper(&claude, &[]);
    assert_eq!(read(&seen).trim(), nice(0));
}

/// 01M3WFYZX6GVFYW6NTTTKF144R: with the slice in `RIFF_WORKER_SLICE`,
/// the wrapper runs `claude` in a scope of that slice of the user
/// manager. With no such variable, it uses no `systemd-run`.
#[test]
fn the_wrapper_runs_claude_in_a_scope_of_the_slice() {
    let m = Machine::new(isolated::DEAD_SERVER);
    script(&m.bin(), "systemd-run", FAKE_SYSTEMD_RUN);
    let seen = m.bin().join("seen");
    let claude = m.claude(&format!(
        "echo \"$RIFF_WORKER $CARGO_BUILD_JOBS $1\" > '{}'",
        seen.display()
    ));
    m.workers(&["limit", "4"]);

    let out = m.wrapper(&claude, &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(m.log("systemd-run.log"), "", "no slice: no scope");
    assert_eq!(read(&seen).trim(), "1 4 Join the riff.");

    std::fs::remove_file(&seen).unwrap();
    let out = m.wrapper(&claude, &[("RIFF_WORKER_SLICE", "riff-workers.slice")]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        m.log("systemd-run.log").trim(),
        "--user --scope --quiet --slice=riff-workers.slice"
    );
    // claude still gets its marks, its jobs and its arguments.
    assert_eq!(read(&seen).trim(), "1 4 Join the riff.");
}

/// 01M3WFZ03Z9Y60HPHJJ9ZE6AQZ: the OS kills `claude`, as it does when
/// the workers take too much memory. The wrapper lives and tells the
/// lead: the signal, and where the work is. The work that is not
/// committed stays in the worktree, so the next worker finds it.
#[tokio::test(flavor = "multi_thread")]
async fn a_killed_worker_tells_the_lead_and_keeps_its_work() {
    let api = start_server().await;
    let lead: SessionUri = LEAD.parse().unwrap();
    api.register(&lead).await.unwrap();
    let m = Machine::new(api.base());
    script(&m.bin(), "systemd-run", FAKE_SYSTEMD_RUN);
    let main = m.main();
    git(
        &main,
        &["worktree", "add", "-q", ".claude/worktrees/issue-12"],
    );
    let worktree = main.join(".claude/worktrees/issue-12");
    let claude = m.claude(&format!(
        "echo 'work in progress' > '{}'\nkill -KILL $$",
        worktree.join("work.rs").display()
    ));
    let wrapper = tokio::task::block_in_place(|| {
        m.wrapper(&claude, &[("RIFF_WORKER_SLICE", "riff-workers.slice")])
    });
    assert_ne!(wrapper.status.code(), Some(0), "{wrapper:?}");

    let read = riff::text::inbox(&api.inbox(&lead, None, false).await.unwrap(), &lead);
    assert!(
        read.contains("worker stopped: pane %5, session w1, signal 9. A kill ended it"),
        "{read}"
    );
    assert!(
        read.contains("Its work that is not committed is in its worktree"),
        "{read}"
    );
    // The next worker finds the worktree with the work.
    let list = Command::new("git")
        .arg("-C")
        .arg(&main)
        .args(["worktree", "list"])
        .output()
        .unwrap();
    assert!(stdout(&list).contains("issue-12"), "{list:?}");
    let status = Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert_eq!(stdout(&status).trim(), "?? work.rs");
    assert_eq!(read_file(&worktree.join("work.rs")), "work in progress\n");
}

fn read_file(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// 01M3WFYZX6GVFYW6NTTTKF144R: on a machine with systemd,
/// `riff workers start` gives the slice of the workers a memory limit
/// and a CPU weight, and names the slice to each worker. The default
/// limit is three quarters of the memory. The setting changes it.
#[test]
fn workers_start_gives_the_slice_a_memory_limit() {
    let m = Machine::new("http://riff.test:7878");
    script(&m.bin(), "systemctl", FAKE_SYSTEMCTL);
    m.workers(&["limit", "4"]);
    let out = m.workers(&["start", "2"]);
    assert!(out.contains("Started 2 workers in"), "{out}");
    assert!(!out.contains("systemd"), "{out}");
    // 30 GB: 23 GB at most, and the OS takes memory back from 9/10 of it.
    assert_eq!(
        m.log("systemctl.log").trim(),
        "--user set-property --runtime riff-workers.slice MemoryHigh=21196M MemoryMax=23552M \
         CPUWeight=50"
    );
    let tmux = m.log("tmux.log");
    assert_eq!(
        tmux.matches("-e RIFF_WORKER_SLICE=riff-workers.slice")
            .count(),
        2,
        "{tmux}"
    );

    let shown = m.workers(&["memory", "10"]);
    assert!(shown.starts_with("workers.memory  10  "), "{shown}");
    m.workers(&["start", "1"]);
    let last = m.log("systemctl.log");
    assert_eq!(
        last.lines().last().unwrap(),
        "--user set-property --runtime riff-workers.slice MemoryHigh=9216M MemoryMax=10240M \
         CPUWeight=50"
    );
}

/// 01M3WFYZZENNHVH8Z2BAFSR6TS: on a machine with no systemd user
/// manager, `riff workers start` says so one time, and starts the
/// workers with no scope.
#[test]
fn a_machine_with_no_systemd_says_so_one_time() {
    let m = Machine::new("http://riff.test:7878");
    script(&m.bin(), "systemctl", NO_SYSTEMD);
    m.workers(&["limit", "4"]);
    let first = m.workers(&["start", "1"]);
    assert!(first.contains("Started 1 worker in"), "{first}");
    assert!(
        first.contains(
            "This machine has no systemd user manager (Failed to connect to user scope bus \
             via local transport: No such file or directory), so the workers run with no \
             memory limit."
        ),
        "{first}"
    );
    let second = m.workers(&["start", "1"]);
    assert!(second.contains("Started 1 worker in"), "{second}");
    assert!(!second.contains("systemd"), "{second}");
    let tmux = m.log("tmux.log");
    assert!(tmux.contains("workers run"), "{tmux}");
    assert!(!tmux.contains("RIFF_WORKER_SLICE"), "{tmux}");
}

/// 01M3WFZ01PTAYYKG3T5CFA2W4D: `riff workers start` starts no worker
/// while the available memory is less than the floor, and says why.
/// `riff workers` shows why too. The setting changes the floor.
#[test]
fn no_worker_starts_under_the_floor_of_available_memory() {
    let m = Machine::new("http://riff.test:7878");
    script(&m.bin(), "systemctl", FAKE_SYSTEMCTL);
    m.workers(&["limit", "4"]);
    let low = "cpu 16x4500MHz, mem 30GB, 3GB available, load 0.50";

    let out = m
        .riff(&["workers", "start", "1"])
        .env("RIFF_MACHINE", low)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(stderr(&out).trim(), riff::text::workers_low(3, 4));
    assert!(!m.log("tmux.log").contains("new-window"), "no pane");
    assert_eq!(m.log("systemctl.log"), "", "no slice");

    let list = m
        .riff(&["workers"])
        .env("RIFF_MACHINE", low)
        .output()
        .unwrap();
    let list = stdout(&list);
    assert!(
        list.contains(
            "Starts no worker: 3 GB of memory is available, and the floor of this machine is \
             4 GB."
        ),
        "{list}"
    );
    // With enough memory, `riff workers` shows no such line.
    assert!(!m.workers(&[]).contains("Starts no worker"));

    // A lower floor lets the worker start.
    let shown = m.workers(&["floor", "2"]);
    assert!(shown.starts_with("workers.floor  2  "), "{shown}");
    let out = m
        .riff(&["workers", "start", "1"])
        .env("RIFF_MACHINE", low)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(m.log("tmux.log").contains("new-window"));
}

/// Each setting of the limits shows its value, what riff makes of it,
/// and how to set it.
#[test]
fn each_setting_of_the_limits_shows_and_sets_its_value() {
    let m = Machine::new(isolated::DEAD_SERVER);
    m.workers(&["limit", "4"]);
    let config = m.root.path().join("home/config.toml");
    let file = format!("({})", config.display());

    assert_eq!(
        m.workers(&["jobs"]),
        format!(
            "workers.jobs  0  {file}\nEach worker builds with 4 jobs and tests with 4 threads: \
             the cores divided by the limit of workers. Set it with: riff workers jobs N (0: \
             riff makes the number)\n"
        )
    );
    assert!(
        m.workers(&["jobs", "6"])
            .contains("builds with 6 jobs and tests with 6 threads.")
    );

    assert_eq!(
        m.workers(&["nice"]),
        format!(
            "workers.nice  10  {file}\nEach worker runs with nice 10. Set it with: riff \
             workers nice N (0 to 19, 0 turns it off)\n"
        )
    );
    assert!(
        m.workers(&["nice", "0"])
            .contains("Each worker runs with no nice.")
    );
    let out = m.run(&["workers", "nice", "20"]);
    assert!(
        !out.status.success(),
        "nice 20 is not a nice value: {out:?}"
    );

    assert_eq!(
        m.workers(&["memory"]),
        format!(
            "workers.memory  0  {file}\nAll workers of this machine get at most 23 GB of \
             memory: three quarters of the memory. Set it with: riff workers memory GB (0: \
             riff makes the number)\n"
        )
    );
    assert!(
        m.workers(&["memory", "12"])
            .contains("at most 12 GB of memory.")
    );

    assert_eq!(
        m.workers(&["floor"]),
        format!(
            "workers.floor  4  {file}\nriff starts no new worker while less than 4 GB of \
             memory is available. Now: 24 GB. Set it with: riff workers floor GB (0 turns it \
             off)\n"
        )
    );
    assert!(m.workers(&["floor", "0"]).starts_with("workers.floor  0  "));

    assert_eq!(
        read(&config),
        "[workers]\nlimit = 4\njobs = 6\nnice = 0\nmemory = 12\nfloor = 0\n"
    );
}

/// The book has a how-to for each new setting, and says how to choose
/// the worker limit from the memory of the machine.
#[test]
fn the_book_has_a_how_to_for_each_limit() {
    let page = book::page("how-it-works.md");
    let start = page
        .find("\n### Limit the workers of a machine\n")
        .expect("the book has \"### Limit the workers of a machine\"");
    let part = &page[start + 1..];
    let part = &part[..part[4..].find("\n### ").map_or(part.len(), |n| n + 4)];
    for (heading, command) in [
        (
            "#### Choose the limit from the memory",
            "riff workers limit 3",
        ),
        ("#### Set the jobs of a worker", "riff workers jobs 4"),
        (
            "#### Set the nice value of the workers",
            "riff workers nice 15",
        ),
        (
            "#### Set the memory of the workers",
            "riff workers memory 20",
        ),
        (
            "#### Set the memory that a new worker needs",
            "riff workers floor 8",
        ),
    ] {
        let how = &part[part
            .find(heading)
            .unwrap_or_else(|| panic!("the book has no {heading:?}"))..];
        let next = how[5..].find("\n#### ").map_or(how.len(), |n| n + 5);
        assert!(
            how[..next].contains("```sh\n") && how[..next].contains(command),
            "{heading} has no {command:?}"
        );
    }
    let commands: Vec<String> = book::commands_in(part)
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    assert_eq!(commands.len(), 7, "{commands:?}");
    book::each_is_real(&commands);
}

/// 01M3WFYZP9Y4N41QGH5SWKFZZC and 01M3WFZ0676C5HJDXCGVZ715K2: the skill
/// says that a session runs one build or test command at a time, and
/// that only the user sets the limits.
#[test]
fn the_skill_says_one_build_at_a_time() {
    let skill = read_file(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("claude-plugin/riff/skills/riff/SKILL.md"),
    );
    let start = skill
        .find("## Build and test")
        .expect("the skill has \"## Build and test\"");
    let part = &skill[start..];
    let part = &part[..part[3..].find("\n## ").map_or(part.len(), |n| n + 3)];
    for text in [
        "Run one build or test command at a time.",
        "run that test by its name in a loop",
        "not the full `just ci`",
        "`CARGO_BUILD_JOBS` and `RUST_TEST_THREADS`",
    ] {
        assert!(part.contains(text), "\"Build and test\" has no {text:?}");
    }
    for setting in ["jobs", "nice", "memory", "floor"] {
        let command = format!("`riff workers {setting}`");
        assert!(skill.contains(&command), "the skill has no {command}");
    }
}
