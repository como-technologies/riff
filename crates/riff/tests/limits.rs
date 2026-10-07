//! The limits of the workers of a machine (01M3WFYZRK5CT22GJW6ZHYT9CC to
//! 01M3WFZ03Z9Y60HPHJJ9ZE6AQZ). A fake `claude`, a fake `tmux`, a fake
//! `systemctl` and a fake `systemd-run` run in place of the real ones.
//! No test changes a setting or a slice of the machine.

use crate::book;

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
# Outside tmux, riff names its own server: -L riff (see start.rs).
[ "$1" = -L ] && shift 2
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
        self.in_pane(Isolated::shared().riff(), args)
    }

    /// `cmd ARGS` in the main clone, as a person in a tmux pane.
    fn in_pane(&self, mut cmd: Command, args: &[&str]) -> Command {
        let real = std::env::var_os("PATH").unwrap();
        let rest = std::env::split_paths(&real);
        let path = std::env::join_paths(std::iter::once(self.bin()).chain(rest)).unwrap();
        cmd.args(args)
            .current_dir(self.main())
            .env("PATH", path)
            .env("RIFF_HOME", self.root.path().join("home"))
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("RIFF_MACHINE", PANGOLIN)
            .env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", "%0")
            // The memory pressure of the machine of the test does not
            // count: a test sets its own.
            .env(riff::jobserver::PSI_VAR, self.root.path().join("no-psi"))
            // A test that runs in a worker has the pool and the test
            // runner of that worker: the machine of the test has none.
            .env_remove("MAKEFLAGS");
        for (name, _) in std::env::vars() {
            if name.starts_with("CARGO_TARGET_") && name.ends_with("_RUNNER") {
                cmd.env_remove(name);
            }
        }
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
        self.wrapper_in(self.riff(&["workers", "run"]), claude, vars)
    }

    /// [`Machine::wrapper`] with a nice value of its own: `nice -n ADD`
    /// more than this test process.
    fn wrapper_at(&self, add: u8, claude: &Path) -> Output {
        let riff = Isolated::shared().riff_path();
        let add = add.to_string();
        let args = ["-n", add.as_str(), riff.to_str().unwrap(), "workers", "run"];
        let nice = self.in_pane(Isolated::shared().command("nice"), &args);
        self.wrapper_in(nice, claude, &[])
    }

    fn wrapper_in(&self, cmd: Command, claude: &Path, vars: &[(&str, &str)]) -> Output {
        self.wrapper_command(cmd, claude, vars).output().unwrap()
    }

    fn wrapper_command(&self, mut cmd: Command, claude: &Path, vars: &[(&str, &str)]) -> Command {
        cmd.arg(claude)
            .arg("Join the riff.")
            .env("RIFF_SESSION", "w1")
            .env("TMUX_PANE", "%5")
            .envs(vars.iter().copied());
        cmd
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

/// The variables of the pool of an outer worker, and a person's
/// `CARGO_BUILD_JOBS`.
fn outer_pool(runner: &str) -> [(&str, &str); 4] {
    let flags = "-j --jobserver-auth=fifo:/outer/fifo";
    [
        ("CARGO_BUILD_JOBS", "64"),
        ("MAKEFLAGS", flags),
        ("CARGO_MAKEFLAGS", flags),
        (runner, "/outer/riff workers test-run"),
    ]
}

/// The nice value of this test process.
fn nice_here() -> u8 {
    let out = Command::new("nice").output().unwrap();
    stdout(&out).trim().parse().unwrap()
}

/// 01M3WFYZRK5CT22GJW6ZHYT9CC and 01M3ZGZMJ9RF1C4AHG78GQ2NM4: the wrapper
/// holds the pool and gives `claude` its `MAKEFLAGS` and the test runner,
/// and no `CARGO_BUILD_JOBS` and no `RUST_TEST_THREADS`.
/// The setting turns the pool off and replaces the number.
#[test]
fn the_wrapper_gives_claude_the_jobs_of_a_worker() {
    let m = Machine::new(isolated::DEAD_SERVER);
    let seen = m.bin().join("seen");
    let claude = m.claude(&format!(
        "echo \"[$CARGO_BUILD_JOBS] $RUST_TEST_THREADS [$MAKEFLAGS] [${}]\" > '{}'\n\
         cat '{}/state/jobs/size' >> '{}'",
        riff::limits::runner_var(),
        seen.display(),
        m.root.path().join("home").display(),
        seen.display()
    ));
    // 16 threads and 4 workers: a pool of 16 - 2 - 4 tokens. The test
    // threads come from the pool too: no RUST_TEST_THREADS.
    m.workers(&["limit", "4"]);
    let out = m.wrapper(
        &claude,
        &[("CARGO_BUILD_JOBS", "64"), ("RUST_TEST_THREADS", "9")],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let fifo = m.root.path().join("home/state/jobs/fifo");
    let seen_now = read(&seen);
    let prefix = format!("[]  [-j --jobserver-auth=fifo:{}] [/", fifo.display());
    assert!(
        seen_now.starts_with(&prefix) && seen_now.ends_with(" workers test-run]\n10"),
        "the variables of the person do not win over the pool: {seen_now}"
    );
    assert_eq!(
        riff::jobserver::state(&m.root.path().join("home/state/jobs")),
        None,
        "the pool ends with the wrapper"
    );
    // 16 cores and 16 workers: 1 or more.
    m.workers(&["limit", "16"]);
    m.wrapper(&claude, &[]);
    assert!(read(&seen).ends_with("\n1"), "{}", read(&seen));
    // The setting wins, with no pool, and no variable of an outer pool
    // stays (01M41CR2HJRFW6R7YMJTPVEMJ1).
    m.workers(&["jobs", "6"]);
    m.wrapper(&claude, &outer_pool(&riff::limits::runner_var()));
    assert!(read(&seen).starts_with("[6] 6 [] []"), "{}", read(&seen));
}

/// Waits until the pool in `dir` has `free` free tokens.
fn wait_for_free(dir: &Path, free: u16) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let now = riff::jobserver::state(dir).map(|s| s.free);
        if now == Some(free) {
            return;
        }
        assert!(
            std::time::Instant::now() < end,
            "free tokens: {now:?}, not {free}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// 01M49XNPMXD3SF6JHBYV9DN59M: while the memory pressure is above the
/// limit, the pool gives out no new token. Under the limit, it gives
/// them out again. `riff workers jobs` shows the pressure.
#[test]
fn memory_pressure_above_the_limit_stops_new_tokens() {
    let m = Machine::new(isolated::DEAD_SERVER);
    let psi = m.root.path().join("pressure");
    let high = "some avg10=40.00 avg60=20.00 avg300=5.00 total=1\n\
                full avg10=10.00 avg60=5.00 avg300=1.00 total=1\n";
    let low = "some avg10=1.00 avg60=0.50 avg300=0.10 total=2\n\
               full avg10=0.00 avg60=0.00 avg300=0.00 total=2\n";
    std::fs::write(&psi, high).unwrap();
    let stop = m.root.path().join("stop");
    let claude = m.claude(&format!(
        "while [ ! -f '{}' ]; do sleep 0.1; done",
        stop.display()
    ));
    m.workers(&["limit", "4"]);
    let psi_var = (riff::jobserver::PSI_VAR, psi.to_str().unwrap());
    let mut wrapper = m
        .wrapper_command(m.riff(&["workers", "run"]), &claude, &[psi_var])
        .spawn()
        .unwrap();
    let dir = m.root.path().join("home/state/jobs");
    wait_for_free(&dir, 0);
    let shown = stdout(
        &m.riff(&["workers", "jobs"])
            .env(psi_var.0, psi_var.1)
            .output()
            .unwrap(),
    );
    assert!(
        shown.contains("Now it is 40.0%: the pool gives out no new token."),
        "{shown}"
    );

    std::fs::write(&psi, low).unwrap();
    wait_for_free(&dir, 10);
    std::fs::write(&stop, "").unwrap();
    assert!(wrapper.wait().unwrap().success());
}

/// 01M3ZGZMRHXRBP762QPVCV0YX8: when riff cannot make the pool, the
/// wrapper gives the fixed share and says why. `riff workers start`
/// says so one time.
#[test]
fn with_no_pool_the_worker_gets_the_fixed_share() {
    let m = Machine::new(isolated::DEAD_SERVER);
    let state = m.root.path().join("home/state");
    std::fs::create_dir_all(&state).unwrap();
    // A file where the pool must go.
    std::fs::write(state.join("jobs"), "").unwrap();
    let seen = m.bin().join("seen");
    let runner = riff::limits::runner_var();
    let claude = m.claude(&format!(
        "echo \"$CARGO_BUILD_JOBS $RUST_TEST_THREADS [$MAKEFLAGS] [$CARGO_MAKEFLAGS] [${runner}]\" > '{}'",
        seen.display()
    ));
    m.workers(&["limit", "4"]);
    // The wrapper starts in a worker with a pool (01M41CR2HJRFW6R7YMJTPVEMJ1).
    let out = m.wrapper(&claude, &outer_pool(&runner));
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(read(&seen).trim(), "3 3 [] [] []");
    assert!(
        stderr(&out).contains("riff cannot make the pool of build jobs ("),
        "{out:?}"
    );

    script(&m.bin(), "systemctl", FAKE_SYSTEMCTL);
    let first = m.workers(&["start", "1"]);
    assert!(first.contains("Started 1 worker in"), "{first}");
    assert!(
        first.contains("so each worker builds with a fixed share of the cores."),
        "{first}"
    );
    let second = m.workers(&["start", "1"]);
    assert!(!second.contains("pool"), "one time: {second}");
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

/// 01M3ZZGRB5NDAA419ZNEWN0811: on a machine where riff cannot read the
/// physical cores, riff counts half of the logical CPUs. `riff workers
/// jobs` says so each time, `riff workers start` one time.
#[test]
fn with_no_physical_cores_riff_counts_half_of_the_logical_cpus() {
    let m = Machine::new(isolated::DEAD_SERVER);
    let cpuinfo = m.bin().join("cpuinfo");
    std::fs::write(&cpuinfo, "processor\t: 0\nprocessor\t: 1\n").unwrap();
    let cpuinfo = cpuinfo.to_str().unwrap();
    let said = "riff cannot read the physical cores of this machine, so it counts half of the \
                16 logical CPUs: 8.";
    m.workers(&["limit", "2"]);
    let out = m
        .riff(&["workers", "jobs"])
        .env(riff::limits::CPUINFO, cpuinfo)
        .output()
        .unwrap();
    let jobs = stdout(&out);
    assert!(jobs.contains(said), "{jobs}");
    assert!(jobs.contains("one pool of 12 tokens"), "16 - 2 - 2: {jobs}");

    let seen = m.bin().join("seen");
    let claude = m.claude(&format!(
        "echo \"$RUST_TEST_THREADS\" > '{}'\ncat '{}/state/jobs/size' >> '{}'",
        seen.display(),
        m.root.path().join("home").display(),
        seen.display()
    ));
    let out = m.wrapper(&claude, &[(riff::limits::CPUINFO, cpuinfo)]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(read(&seen), "\n12");

    script(&m.bin(), "systemctl", FAKE_SYSTEMCTL);
    let start = |n: &str| {
        let out = m
            .riff(&["workers", "start", n])
            .env(riff::limits::CPUINFO, cpuinfo)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        stdout(&out)
    };
    assert!(start("1").contains(said), "the first start says so");
    assert!(!start("1").contains("physical"), "one time");
}

/// 01M3WFYZRK5CT22GJW6ZHYT9CC and 01M3ZZGRFYH0KSYMM71EK3TT6T: when more
/// workers run than the limit, the numbers count the workers that run,
/// and `riff workers jobs` shows them.
#[test]
fn the_numbers_count_the_workers_over_the_limit() {
    let m = Machine::new(isolated::DEAD_SERVER);
    let dir = m.root.path().join("home/state/jobs");
    m.workers(&["limit", "2"]);
    // 3 workers run; the wrapper is the 4th.
    let running: Vec<_> = (0..3)
        .map(|_| riff::jobserver::Member::join(&dir).unwrap())
        .collect();
    let seen = m.bin().join("seen");
    let claude = m.claude(&format!(
        "echo \"$RUST_TEST_THREADS $(cat '{}/size') $(cat '{}/counted')\" > '{}'",
        dir.display(),
        dir.display(),
        seen.display()
    ));
    let out = m.wrapper(&claude, &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    // 16 threads and 4 workers, not 2: 16 - 2 - 4 tokens.
    assert_eq!(read(&seen), " 10 4\n");
    assert_eq!(riff::jobserver::workers(&dir), 3, "the wrapper left");

    // A pool for 2 workers, and 3 that run.
    let pool = riff::jobserver::Pool::hold(&dir, 13, 2).unwrap();
    let jobs = m.workers(&["jobs"]);
    assert!(
        jobs.contains(
            "3 workers run, more than 2: each worker after the first 2 keeps one \
                       token out of the pool."
        ),
        "{jobs}"
    );
    drop((pool, running));
}

/// 01M3WFYZTX05CGDP2NQF9B356K: the wrapper starts `claude` with nice 10.
/// The setting changes the value, and 0 turns it off.
/// 01M407J8R79WVYVABVCSHFAMJ9: the value is absolute, also when the
/// wrapper runs at a nice value of its own. A wrapper at a higher value
/// keeps its own value, and says so.
#[test]
fn the_wrapper_starts_claude_with_nice() {
    let m = Machine::new(isolated::DEAD_SERVER);
    let seen = m.bin().join("seen");
    let claude = m.claude(&format!("nice > '{}'", seen.display()));
    let base = nice_here();
    let at = |nice: u8, wrapper: u8| nice.max(wrapper).to_string();

    let out = m.wrapper(&claude, &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(read(&seen).trim(), at(10, base));

    // The wrapper runs at a nice value of its own: claude still gets 10.
    let own = (base + 5).min(19);
    let out = m.wrapper_at(5, &claude);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(read(&seen).trim(), at(10, own));
    let said = format!("runs at nice {own}, more than workers.nice 10");
    assert_eq!(stderr(&out).contains(&said), own > 10, "{out:?}");

    let out = m.wrapper_at(15, &claude);
    let own = (base + 15).min(19);
    assert_eq!(read(&seen).trim(), at(10, own), "{out:?}");
    assert!(
        stderr(&out).contains(&format!("runs at nice {own}")),
        "{out:?}"
    );

    m.workers(&["nice", "3"]);
    m.wrapper(&claude, &[]);
    assert_eq!(read(&seen).trim(), at(3, base));

    m.workers(&["nice", "0"]);
    m.wrapper(&claude, &[]);
    assert_eq!(read(&seen).trim(), at(0, base));
}

/// 01M3WFYZX6GVFYW6NTTTKF144R: with the slice in `RIFF_WORKER_SLICE`,
/// the wrapper runs `claude` in a scope of that slice of the user
/// manager. With no such variable, it uses no `systemd-run`.
#[test]
fn the_wrapper_runs_claude_in_a_scope_of_the_slice() {
    let m = Machine::new(isolated::DEAD_SERVER);
    script(&m.bin(), "systemctl", FAKE_SYSTEMCTL);
    script(&m.bin(), "systemd-run", FAKE_SYSTEMD_RUN);
    let seen = m.bin().join("seen");
    let claude = m.claude(&format!(
        "echo \"$RIFF_WORKER $RUST_TEST_THREADS $1\" > '{}'",
        seen.display()
    ));
    m.workers(&["limit", "4"]);

    let out = m.wrapper(&claude, &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(m.log("systemd-run.log"), "", "no slice: no scope");
    assert_eq!(read(&seen).trim(), "1  Join the riff.");

    std::fs::remove_file(&seen).unwrap();
    let out = m.wrapper(&claude, &[("RIFF_WORKER_SLICE", "riff-workers.slice")]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    // The first scope is the check that a scope works in the pane. The
    // scope of claude has the name of the worker and of the wrapper
    // (01M49SV9W4S1HJ4BYANA388VD2).
    let log = m.log("systemd-run.log");
    let (check, worker) = log.trim().split_once('\n').unwrap();
    assert_eq!(check, "--user --scope --quiet --slice=riff-workers.slice");
    let unit = worker
        .strip_prefix(" --user --scope --quiet --slice=riff-workers.slice --unit=")
        .unwrap_or_else(|| panic!("no scope name: {log:?}"));
    assert_eq!(
        riff::workload::scope_worker(unit).as_deref(),
        Some("w1"),
        "{log:?}"
    );
    // claude still gets its marks, its jobs and its arguments.
    assert_eq!(read(&seen).trim(), "1  Join the riff.");
}

/// A fake `systemd-run` with no user bus in the pane.
const NO_BUS: &str = r#"#!/bin/sh
echo "Failed to connect to bus: No medium found" >&2
exit 1
"#;

/// 01M407J8X25H9AT8M789EG5RQZ: when `systemd-run --user --scope` fails
/// in the pane, the wrapper starts `claude` with no scope, and says so
/// one time.
#[test]
fn with_no_scope_in_the_pane_the_worker_starts_with_no_scope() {
    let m = Machine::new(isolated::DEAD_SERVER);
    script(&m.bin(), "systemctl", FAKE_SYSTEMCTL);
    script(&m.bin(), "systemd-run", NO_BUS);
    let seen = m.bin().join("seen");
    let claude = m.claude(&format!("echo \"$RIFF_WORKER $1\" > '{}'", seen.display()));
    let slice = [("RIFF_WORKER_SLICE", "riff-workers.slice")];

    let first = m.wrapper(&claude, &slice);
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    assert_eq!(read(&seen).trim(), "1 Join the riff.");
    assert!(
        stderr(&first).contains(
            "systemd-run cannot make a scope in this pane (Failed to connect to bus: No medium \
             found), so this worker runs with no memory limit."
        ),
        "{first:?}"
    );

    std::fs::remove_file(&seen).unwrap();
    let second = m.wrapper(&claude, &slice);
    assert_eq!(second.status.code(), Some(0), "{second:?}");
    assert_eq!(read(&seen).trim(), "1 Join the riff.");
    assert!(!stderr(&second).contains("systemd-run"), "{second:?}");
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
    script(&m.bin(), "systemctl", FAKE_SYSTEMCTL);
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

/// 01M3WFYZX6GVFYW6NTTTKF144R, 01M4C2PXZ5WNE4C2CJW2HABPY0: on a machine
/// with systemd, `riff workers start` calls no `systemctl`: it can run
/// in the sandbox of the lead. It names the slice to each worker. The
/// wrapper, outside each sandbox, gives the slice a memory limit and a
/// CPU weight. The default limit is three quarters of the memory. The
/// setting changes it.
#[test]
fn the_wrapper_gives_the_slice_a_memory_limit() {
    let m = Machine::new("http://riff.test:7878");
    script(&m.bin(), "systemctl", FAKE_SYSTEMCTL);
    script(&m.bin(), "systemd-run", FAKE_SYSTEMD_RUN);
    m.workers(&["limit", "4"]);
    let out = m.workers(&["start", "2"]);
    assert!(out.contains("Started 2 workers in"), "{out}");
    assert!(!out.contains("systemd"), "{out}");
    assert_eq!(m.log("systemctl.log"), "", "the start calls no systemctl");
    let tmux = m.log("tmux.log");
    assert_eq!(
        tmux.matches("-e RIFF_WORKER_SLICE=riff-workers.slice")
            .count(),
        2,
        "{tmux}"
    );

    let claude = m.claude("exit 0");
    let slice = [("RIFF_WORKER_SLICE", "riff-workers.slice")];
    let out = m.wrapper(&claude, &slice);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    // 30 GB: 23 GB at most, and the OS takes memory back from 9/10 of it.
    assert_eq!(
        m.log("systemctl.log").trim(),
        "--user set-property --runtime riff-workers.slice MemoryHigh=21196M MemoryMax=23552M \
         CPUWeight=50"
    );
    assert!(m.log("systemd-run.log").contains("--slice=riff-workers.slice"));

    let shown = m.workers(&["memory", "10"]);
    assert!(shown.starts_with("workers.memory  10  "), "{shown}");
    let out = m.wrapper(&claude, &slice);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let last = m.log("systemctl.log");
    assert_eq!(
        last.lines().last().unwrap(),
        "--user set-property --runtime riff-workers.slice MemoryHigh=9216M MemoryMax=10240M \
         CPUWeight=50"
    );
}

/// 01M3WFYZZENNHVH8Z2BAFSR6TS: on a machine with no systemd user
/// manager, the wrapper says so one time in its pane, and starts
/// `claude` with no scope.
#[test]
fn a_machine_with_no_systemd_says_so_one_time() {
    let m = Machine::new(isolated::DEAD_SERVER);
    script(&m.bin(), "systemctl", NO_SYSTEMD);
    script(&m.bin(), "systemd-run", FAKE_SYSTEMD_RUN);
    let seen = m.bin().join("seen");
    let claude = m.claude(&format!("echo \"$RIFF_WORKER $1\" > '{}'", seen.display()));
    let slice = [("RIFF_WORKER_SLICE", "riff-workers.slice")];

    let first = m.wrapper(&claude, &slice);
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    assert_eq!(read(&seen).trim(), "1 Join the riff.");
    assert!(
        stderr(&first).contains(
            "riff: this machine has no systemd user manager (Failed to connect to user scope \
             bus via local transport: No such file or directory), so the workers run with no \
             memory limit."
        ),
        "{first:?}"
    );

    std::fs::remove_file(&seen).unwrap();
    let second = m.wrapper(&claude, &slice);
    assert_eq!(second.status.code(), Some(0), "{second:?}");
    assert_eq!(read(&seen).trim(), "1 Join the riff.");
    assert!(!stderr(&second).contains("systemd"), "{second:?}");
    assert_eq!(m.log("systemd-run.log"), "", "no scope");
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
            "workers.jobs  0  {file}\nThe machine has 16 physical cores and 16 hardware \
             threads. All workers take their compile jobs and test threads from one pool of 10 \
             tokens: the hardware threads less 2, less 4 workers (the limit, or the workers that \
             run when they are more). Each build also has one job of its own. A test program \
             takes each free token, and runs one test thread for each. No worker runs now. \
             While the memory pressure is above 10%, the pool gives out no new token. riff \
             cannot read the memory pressure of this machine. Set it with: riff workers jobs N \
             (N turns the pool off; 0: the pool)\n"
        )
    );
    assert!(
        m.workers(&["jobs", "6"])
            .contains("No pool: each worker builds with 6 jobs and tests with 6 threads.")
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

/// 01M419XAX31FF9Z1647E881CSH: `riff workers` shows the cap of the
/// clock and the clock now from the cpufreq files, not the limit of the
/// hardware, and the score counts the cap.
#[test]
fn riff_workers_shows_the_cap_of_the_clock_and_the_clock_now() {
    let m = Machine::new(isolated::DEAD_SERVER);
    let sys = m.root.path().join("sys");
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    for core in 0..cores {
        let dir = sys.join(format!("cpu{core}/cpufreq"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cpuinfo_max_freq"), "4001000\n").unwrap();
        std::fs::write(dir.join("scaling_max_freq"), "3000000\n").unwrap();
        let now = if core % 2 == 0 {
            "2980000\n"
        } else {
            "3000000\n"
        };
        std::fs::write(dir.join("scaling_cur_freq"), now).unwrap();
    }
    let cpuinfo = m.root.path().join("cpuinfo");
    std::fs::write(&cpuinfo, "cpu MHz\t\t: 4001.000\n").unwrap();

    let out = m
        .riff(&["workers"])
        .env_remove("RIFF_MACHINE")
        .env(riff::machine::CPU_SYS, &sys)
        .env(riff::limits::CPUINFO, &cpuinfo)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let list = stdout(&out);
    // The mean of the cores: half of them run at 2980 MHz.
    let now = 2980 + 20 * (cores / 2) / cores;
    assert!(
        list.contains(&format!("cpu {cores}x3000MHz (now {now}MHz), mem ")),
        "{list}"
    );
    assert!(!list.contains("4001MHz"), "{list}");
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
            "riff workers limit 4",
        ),
        ("#### See the pool of build jobs", "riff workers jobs"),
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
        (
            "#### Set the memory of the workers",
            "systemd-cgls --user-unit riff.slice",
        ),
        (
            "#### Limit a session that you start by hand",
            "--slice=riff-workers.slice",
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
    assert_eq!(commands.len(), 8, "{commands:?}");
    book::each_is_real(&commands);
}

/// 01M3WFYZP9Y4N41QGH5SWKFZZC and 01M3WFZ0676C5HJDXCGVZ715K2: the skill
/// says that the pool shares the cores, that a worker keeps the
/// variables of riff, and that only the user sets the limits. It no
/// longer says "one build at a time".
#[test]
fn the_skill_says_the_pool_shares_the_cores() {
    let skill = read_file(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("claude-plugin/riff/skills/riff/SKILL.md"),
    );
    let start = skill
        .find("## Build and test")
        .expect("the skill has \"## Build and test\"");
    let part = &skill[start..];
    let part = &part[..part[3..].find("\n## ").map_or(part.len(), |n| n + 3)];
    for text in [
        "one pool of build jobs",
        "`MAKEFLAGS` and the cargo test runner",
        "the free jobs of the pool as its test\n  threads",
        "do not replace the test runner",
        "run that test by its name in a loop",
        "not the full `just ci`",
        "`CARGO_BUILD_JOBS` and\n  `RUST_TEST_THREADS`",
    ] {
        assert!(part.contains(text), "\"Build and test\" has no {text:?}");
    }
    assert!(!skill.contains("one build or test command at a time"));
    for setting in ["jobs", "nice", "memory", "floor"] {
        let command = format!("`riff workers {setting}`");
        assert!(skill.contains(&command), "the skill has no {command}");
    }
}

/// 01M4BQA5K7DQHQ4DSJGQJH8ZQE: riff gives a worker no compile cache.
/// With an `sccache` on the `PATH`, the wrapper runs no `sccache` and
/// sets no variable of a cache: `claude` gets the variables of the
/// person as they are. `riff workers` shows no cache, and `riff workers
/// cache` is no command.
#[test]
fn the_wrapper_gives_claude_no_compile_cache() {
    let m = Machine::new(isolated::DEAD_SERVER);
    let seen = m.bin().join("seen");
    let claude = m.claude(&format!(
        "echo \"[$RUSTC_WRAPPER] [$SCCACHE_DIR] [$SCCACHE_SERVER_PORT]\" > '{}'",
        seen.display()
    ));
    script(
        &m.bin(),
        "sccache",
        "#!/bin/sh\necho \"$*\" >> \"$(dirname \"$0\")/sccache.log\"\n",
    );
    let person = [("RUSTC_WRAPPER", "/person/sccache")];
    let out = m.wrapper(&claude, &person);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    // The other two variables are as this test got them.
    let own = |var| std::env::var(var).unwrap_or_default();
    let want = format!(
        "[/person/sccache] [{}] [{}]\n",
        own("SCCACHE_DIR"),
        own("SCCACHE_SERVER_PORT")
    );
    assert_eq!(read(&seen), want);
    assert!(!stderr(&out).contains("sccache"), "{out:?}");
    assert!(!m.bin().join("sccache.log").exists(), "riff ran sccache");

    let listed = m.workers(&[]);
    assert!(!listed.contains("cache"), "{listed}");
    let cache = m.run(&["workers", "cache"]);
    assert!(!cache.status.success(), "{cache:?}");
}
