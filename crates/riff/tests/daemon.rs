//! `riff daemon`, `riff daemon install`, `riff daemon uninstall`,
//! `riff daemon status` and `riff daemon sweep` (#716, #736, #938,
//! 01M4ZDTVK74596M0ZA46N51NKV, 01M501M0T0JBWGMMHCT7HT95T9,
//! 01M4ZDTVNBKDM64043GGWQEV4S).
//!
//! A fake `systemctl` logs each call. Its `restart riff.service` starts
//! the `riff daemon` of this build in the background, as systemd does.
//! The test of `riff run` through the daemon is in `run.rs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use isolated::Isolated;

use crate::run::{fake_systemctl, git, wait_for};

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// An environment with the fake `systemctl` first in `PATH`.
pub(crate) struct Here {
    env: Isolated,
    bin: tempfile::TempDir,
    path: String,
}

impl Here {
    pub(crate) fn new() -> Here {
        let env = Isolated::new();
        let bin = tempfile::tempdir().unwrap();
        fake_systemctl(bin.path());
        let path = format!(
            "{}:{}",
            bin.path().display(),
            std::env::var("PATH").unwrap()
        );
        Here { env, bin, path }
    }

    /// `riff ARGS`, with no `RIFF_HOME` when `machine` is true: the riff
    /// of the machine, in the isolated home.
    pub(crate) fn command(&self, machine: bool, args: &[&str]) -> Command {
        let mut cmd = self.env.riff();
        cmd.args(args)
            .env("PATH", &self.path)
            .env("FAKE_RIFF", self.env.riff_path());
        if machine {
            cmd.env_remove("RIFF_HOME");
        }
        cmd
    }

    pub(crate) async fn run(&self, machine: bool, args: &[&str]) -> Output {
        let mut cmd = self.command(machine, args);
        tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap()
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.bin.path().join(name)).unwrap_or_default()
    }

    /// The data folder of riff of the machine: `XDG_DATA_HOME/riff`.
    fn data(&self) -> PathBuf {
        self.env.home().join(".local/share/riff")
    }
}

/// Stops the process `pid` with a SIGTERM, as systemd does.
fn term(pid: u32) {
    let pid = nix::unistd::Pid::from_raw(pid as i32);
    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).unwrap();
}

#[tokio::test]
async fn riff_daemon_install_writes_the_unit_and_starts_the_daemon() {
    let here = Here::new();
    let out = here.run(true, &["daemon", "install"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));

    let unit_path = here.env.home().join(".config/systemd/user/riff.service");
    assert!(
        stdout.contains(&format!(
            "riff wrote the unit {}, and started it again.",
            unit_path.display()
        )),
        "{stdout}"
    );
    assert!(stdout.contains("The riff daemon runs: pid "), "{stdout}");
    let unit = std::fs::read_to_string(&unit_path).unwrap();
    let exe = std::fs::canonicalize(here.env.riff_path()).unwrap();
    for line in [
        format!("ExecStart=\"{}\" daemon", exe.display()),
        format!("Environment=\"PATH={}\"", here.path),
        format!(
            "Environment=\"XDG_DATA_HOME={}\"",
            here.env.home().join(".local/share").display()
        ),
        "WantedBy=default.target".to_owned(),
    ] {
        assert!(unit.lines().any(|l| l == line), "{line}\nnot in\n{unit}");
    }
    assert_eq!(
        here.read("systemctl.log"),
        "systemctl --user daemon-reload\n\
         systemctl --user enable riff.service\n\
         systemctl --user restart riff.service\n"
    );

    // The daemon answers in the data folder of riff of the machine.
    let out = here.run(true, &["daemon", "status"]).await;
    let status = text(&out.stdout);
    assert!(out.status.success(), "{status}");
    assert!(status.contains("Sessions: none."), "{status}");
    // The daemon has no key and no riff of peers (01M53NGW6F5HP1DZ9QRGP50MGT).
    assert!(
        !status.contains("key") && !status.contains("Its riff"),
        "{status}"
    );
    assert!(
        status.contains(&format!(
            "Its folder: {}",
            here.data().join("daemon").display()
        )),
        "{status}"
    );

    // systemd stops the daemon with a SIGTERM.
    let old: u32 = here.read("daemon.pid").trim().parse().unwrap();
    term(old);
    wait_for(&here.bin.path().join("daemon.out"), "The riff daemon stops").await;
}

/// 01M51QARV45MGEKC6KGWX1N5NS, 01M51M3ZKWQYBBPD97R0G9QAX0,
/// 01M51M3ZPAEMJ64266QSJBNQ46, 01M51M3ZTV61C9BMFT3J1M06NF: a daemon with
/// `RIFF_RELEASES` and its own SHA-256 in `SHA256SUMS` there changes no
/// rule, also with `update.auto` on. Its line names the variable, and it
/// keeps that line. The unit keeps `CLAUDE_CONFIG_DIR`. The add of the
/// rules has its tests in `rule_sync::tests`.
#[tokio::test]
async fn a_daemon_with_riff_releases_and_its_own_sha256_there_changes_no_rule() {
    let Some(target) = riff::release::TARGET else {
        return;
    };
    let here = Here::new();
    // RIFF_RELEASES of the test environment names this folder. Its
    // SHA256SUMS has the SHA-256 of this riff, a dev build.
    let tag = format!("v{}", env!("CARGO_PKG_VERSION"));
    let files = here.env.releases().join(&tag);
    std::fs::create_dir_all(&files).unwrap();
    let bytes = std::fs::read(here.env.riff_path()).unwrap();
    let sum = riff::release::sha256(&bytes);
    let name = riff::release::asset("riff", target);
    std::fs::write(files.join("SHA256SUMS"), format!("{sum}  {name}\n")).unwrap();
    // The user settings of the person: a rule and a key of their own.
    let claude = here.env.home().join("claude");
    std::fs::create_dir_all(&claude).unwrap();
    let settings = claude.join("settings.json");
    let before = r#"{"model": "opus", "permissions": {"allow": ["Read(~/**)"]}}"#;
    std::fs::write(&settings, before).unwrap();
    let out = here.run(true, &["update", "--auto", "on"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));

    let mut cmd = here.command(true, &["daemon", "install"]);
    cmd.env("CLAUDE_CONFIG_DIR", &claude);
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let unit_path = here.env.home().join(".config/systemd/user/riff.service");
    let unit = std::fs::read_to_string(&unit_path).unwrap();
    let line = format!("Environment=\"CLAUDE_CONFIG_DIR={}\"", claude.display());
    assert!(unit.lines().any(|l| l == line), "{line}\nnot in\n{unit}");

    let log = here.bin.path().join("daemon.out");
    let why = riff::text::not_release_releases(&here.env.releases().display().to_string());
    wait_for(&log, &why).await;
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), before);
    let logged = std::fs::read_to_string(&log).unwrap();
    assert!(!logged.contains("the sync of the rules"), "{logged}");
    let local = here.env.path().join("run/riff");
    assert_eq!(riff::local::rules_added(&local), None);
    let last = riff::local::rules_sync(&local).unwrap();
    assert!(last.contains("changed no rule"), "{last}");
    assert!(last.ends_with(&why), "{last}");

    let pid: u32 = here.read("daemon.pid").trim().parse().unwrap();
    term(pid);
    wait_for(&log, "The riff daemon stops").await;
}

/// 01M51QFPJAABDP1V7WFY8NQR60, 01M51QFPMSG81YRDDGQQ1WJDSQ,
/// 01M51QFPQ68JHS08TWXNNWZ0D7: the daemon of a dev build does no step of
/// an update, also with `update.auto` on and its own SHA-256 in the
/// `SHA256SUMS` of `RIFF_RELEASES`. The unit file of an older release
/// stays, and the daemon logs the gate and the step of the unit. The
/// steps of a release have their tests in `update_steps::tests`.
#[tokio::test]
async fn a_daemon_of_a_dev_build_does_no_step_of_an_update() {
    let Some(target) = riff::release::TARGET else {
        return;
    };
    let here = Here::new();
    let tag = format!("v{}", env!("CARGO_PKG_VERSION"));
    let files = here.env.releases().join(&tag);
    std::fs::create_dir_all(&files).unwrap();
    let bytes = std::fs::read(here.env.riff_path()).unwrap();
    let sum = riff::release::sha256(&bytes);
    let name = riff::release::asset("riff", target);
    std::fs::write(files.join("SHA256SUMS"), format!("{sum}  {name}\n")).unwrap();
    let out = here.run(true, &["update", "--auto", "on"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    // The unit file of an older release: another RestartSec.
    let unit = riff::service::Unit {
        exe: std::fs::canonicalize(here.env.riff_path()).unwrap(),
        env: vec![("PATH".into(), here.path.clone())],
    };
    let old = unit
        .text()
        .unwrap()
        .replace("RestartSec=2\n", "RestartSec=9\n");
    let unit_path = here.env.home().join(".config/systemd/user/riff.service");
    std::fs::create_dir_all(unit_path.parent().unwrap()).unwrap();
    std::fs::write(&unit_path, &old).unwrap();

    // systemd starts the daemon of the unit.
    let mut cmd = here.env.command(here.bin.path().join("systemctl"));
    cmd.args(["--user", "restart", "riff.service"])
        .env("PATH", &here.path)
        .env("FAKE_RIFF", here.env.riff_path())
        .env_remove("RIFF_HOME");
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let log = here.bin.path().join("daemon.out");
    let releases = here.env.releases().display().to_string();
    let gate = format!(
        "does no step of an update: it is not a release: {}",
        riff::text::not_release_releases(&releases)
    );
    wait_for(&log, &gate).await;
    let unit_line = format!(
        "not in step  the unit of the daemon: {}. Fix: riff daemon install",
        riff::text::STEP_UNIT_OLD
    );
    wait_for(&log, &unit_line).await;
    assert_eq!(std::fs::read_to_string(&unit_path).unwrap(), old);
    assert!(!here.read("systemctl.log").contains("daemon-reload"));

    let pid: u32 = here.read("daemon.pid").trim().parse().unwrap();
    term(pid);
    wait_for(&log, "The riff daemon stops").await;
}

/// Starts the daemon of a unit that names the riff of an older release,
/// with `mode` in the file `try-restart` of the fake `systemctl`. The
/// daemon runs an old riff, so it asks for its new start at once. Waits
/// for that ask, and for `line` in the log of the daemon. Then it stops
/// the daemon, and gives its log.
async fn the_daemon_of_an_old_unit_starts_again(mode: &str, line: &str) -> String {
    let here = Here::new();
    std::fs::write(here.bin.path().join("try-restart"), mode).unwrap();
    let unit = riff::service::Unit {
        exe: "/opt/riff-of-an-older-release/riff".into(),
        env: vec![("PATH".into(), here.path.clone())],
    };
    let unit_path = here.env.home().join(".config/systemd/user/riff.service");
    std::fs::create_dir_all(unit_path.parent().unwrap()).unwrap();
    std::fs::write(&unit_path, unit.text().unwrap()).unwrap();

    // systemd starts the daemon of the unit.
    let mut cmd = here.env.command(here.bin.path().join("systemctl"));
    cmd.args(["--user", "restart", "riff.service"])
        .env("PATH", &here.path)
        .env("FAKE_RIFF", here.env.riff_path())
        .env_remove("RIFF_HOME");
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let log = here.bin.path().join("daemon.out");
    wait_for(&log, riff::text::DAEMON_NEW_RIFF).await;
    let ask = "systemctl --user --no-block try-restart riff.service";
    wait_for(&here.bin.path().join("systemctl.log"), ask).await;
    wait_for(&log, line).await;

    let pid: u32 = here.read("daemon.pid").trim().parse().unwrap();
    term(pid);
    wait_for(&log, "The riff daemon stops").await;
    std::fs::read_to_string(&log).unwrap()
}

/// 01M53NEE2AVNHA8RD8N40FPH03, 01M517BTP6BAGTRBYWPX9GPVE0: the stop of
/// the unit at the new start ends the `systemctl` child of the daemon
/// with a SIGTERM. The daemon logs no failure, and no `riff daemon
/// install` step. A real failure of `systemctl`, with an exit code that
/// is not 0 and its error on stderr, still logs the error.
#[tokio::test]
async fn a_new_start_that_stops_the_systemctl_of_the_daemon_logs_no_failure() {
    let no_restart = riff::text::daemon_no_restart("");
    let no_restart = no_restart.split(':').next().unwrap();

    let stopped = the_daemon_of_an_old_unit_starts_again("stop", riff::text::DAEMON_NEW_RIFF).await;
    assert!(!stopped.contains(no_restart), "{stopped}");
    assert!(!stopped.contains("Run: riff daemon install"), "{stopped}");

    let error = "Failed to try-restart riff.service: Access denied";
    let failed = format!(
        "riff daemon: {}",
        riff::text::daemon_no_restart(&format!(
            "systemctl --user --no-block try-restart riff.service: {error}"
        ))
    );
    let log = the_daemon_of_an_old_unit_starts_again(error, &failed).await;
    assert_eq!(log.matches(no_restart).count(), 1, "{log}");
}

#[tokio::test]
async fn riff_daemon_install_with_riff_home_installs_nothing() {
    let here = Here::new();
    let out = here.run(false, &["daemon", "install"]).await;
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("RIFF_HOME or RIFF_DATA is set"), "{stderr}");
    assert!(!here.env.home().join(".config/systemd").exists());
    assert_eq!(here.read("systemctl.log"), "");
}

#[tokio::test]
async fn riff_daemon_uninstall_stops_the_daemon_and_deletes_its_unit() {
    let here = Here::new();
    let out = here.run(true, &["daemon", "install"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let unit_path = here.env.home().join(".config/systemd/user/riff.service");
    assert!(unit_path.exists());
    std::fs::remove_file(here.bin.path().join("systemctl.log")).unwrap();

    let out = here.run(true, &["daemon", "uninstall"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(
        stdout.contains(&format!(
            "riff stopped the daemon, and deleted the unit {}.",
            unit_path.display()
        )),
        "{stdout}"
    );
    assert!(!unit_path.exists());
    assert_eq!(
        here.read("systemctl.log"),
        "systemctl --user disable --now riff.service\n\
         systemctl --user daemon-reload\n"
    );
    wait_for(&here.bin.path().join("daemon.out"), "The riff daemon stops").await;
    // The folder of the daemon stays.
    assert!(here.data().join("daemon").is_dir());

    // A second run finds nothing, and runs no systemctl.
    let out = here.run(true, &["daemon", "uninstall"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("No riff daemon is installed: there is no unit "),
        "{stdout}"
    );
    assert_eq!(
        here.read("systemctl.log"),
        "systemctl --user disable --now riff.service\n\
         systemctl --user daemon-reload\n"
    );
}

#[tokio::test]
async fn riff_daemon_uninstall_with_riff_home_removes_nothing() {
    let here = Here::new();
    let out = here.run(false, &["daemon", "uninstall"]).await;
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("RIFF_HOME or RIFF_DATA is set"), "{stderr}");
    assert_eq!(here.read("systemctl.log"), "");
}

#[tokio::test]
async fn riff_daemon_status_with_no_daemon_shows_the_unit_and_exits_with_1() {
    let here = Here::new();
    let out = here.run(false, &["daemon", "status"]).await;
    assert_eq!(out.status.code(), Some(1));
    let stdout = text(&out.stdout);
    let socket = here.env.riff_home().join("daemon/riff.sock");
    assert!(
        stdout.contains(&format!("No riff daemon answers on {}", socket.display())),
        "{stdout}"
    );
    assert!(
        stdout.contains("The unit riff.service is inactive."),
        "{stdout}"
    );
    assert_eq!(
        here.read("systemctl.log"),
        "systemctl --user is-active riff.service\n"
    );
}

#[tokio::test]
async fn a_second_daemon_does_not_start_while_one_answers() {
    let here = Here::new();
    let log = here.bin.path().join("first.out");
    let out = std::fs::File::create(&log).unwrap();
    let mut first = here
        .command(false, &["daemon"])
        .stdout(out.try_clone().unwrap())
        .stderr(out)
        .spawn()
        .unwrap();
    wait_for(&log, "The riff daemon listens on").await;
    let socket: &Path = &here.env.riff_home().join("daemon/riff.sock");

    let out = here.run(false, &["daemon"]).await;
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(&format!(
            "A riff daemon runs already: pid {}, on {}",
            first.id(),
            socket.display()
        )),
        "{stderr}"
    );

    term(first.id());
    let status = tokio::task::spawn_blocking(move || first.wait().unwrap())
        .await
        .unwrap();
    assert!(status.success(), "{status}");
    assert!(!socket.exists());
    // A stop of the daemon says that it stopped no session.
    let said = std::fs::read_to_string(&log).unwrap();
    assert!(said.contains("It stopped no session"), "{said}");
}

/// 01M53YP97S9BDMFBN0X5G444MY: a daemon of a `RIFF_DATA` runs beside the
/// daemon that runs already, with its own folder, socket and key. Each
/// `riff daemon` command with that `RIFF_DATA` talks only to it. Its stop
/// leaves the other daemon as it was.
#[tokio::test]
async fn a_daemon_of_riff_data_runs_beside_the_daemon_that_runs_already() {
    let here = Here::new();
    let first = here.daemon("first.out").await;
    let ours = format!("pid {}, ", first.id());
    let data = here.env.path().join("test-data");
    let vars = [(riff::run::DATA_VAR, data.to_str().unwrap())];
    let second = here.daemon_with("second.out", &vars).await;
    let mut cmd = here.command(false, &["daemon", "status"]);
    cmd.envs(vars);
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    let theirs = text(&out.stdout);
    assert!(out.status.success(), "{theirs}");
    let pid = format!("pid {}, ", second.id());
    assert!(theirs.contains(&pid), "{theirs}");
    let folder = format!("Its folder: {}", data.join("daemon").display());
    assert!(theirs.lines().any(|l| l == folder), "{theirs}");
    assert!(!theirs.contains(&ours), "{theirs}");
    assert!(data.join("daemon/riff.sock").exists());

    stop(second).await;
    assert!(!data.join("daemon/riff.sock").exists());
    // The first daemon answers on, in its own folder.
    let out = here.run(false, &["daemon", "status"]).await;
    let status = text(&out.stdout);
    assert!(out.status.success(), "{status}");
    assert!(status.contains(&ours), "{status}");
    let folder = format!("Its folder: {}", here.daemon_dir().display());
    assert!(status.lines().any(|l| l == folder), "{status}");
    stop(first).await;
}

/// 01M53YP9AHPZ9YPNQFJ1SYZGWF: the daemon of a `RIFF_DATA` is never the
/// daemon of the unit. `riff daemon install` and `riff daemon uninstall`
/// with `RIFF_DATA` change nothing, and the start steps of its daemon
/// have no step of the unit. With no `RIFF_DATA`, they have it.
#[tokio::test]
async fn the_daemon_of_riff_data_is_never_the_daemon_of_the_unit() {
    let here = Here::new();
    let data = here.env.path().join("test-data");
    for args in [["daemon", "install"], ["daemon", "uninstall"]] {
        let mut cmd = here.command(true, &args);
        cmd.env(riff::run::DATA_VAR, &data);
        let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap();
        assert!(!out.status.success());
        let stderr = text(&out.stderr);
        assert!(stderr.contains("RIFF_HOME or RIFF_DATA is set"), "{stderr}");
    }
    assert!(!here.env.home().join(".config/systemd").exists());
    assert_eq!(here.read("systemctl.log"), "");

    // The daemons of the machine, with no RIFF_HOME.
    let start = |name: &str, data: Option<&Path>| {
        let log = here.bin.path().join(name);
        let out = std::fs::File::create(&log).unwrap();
        let mut cmd = here.command(true, &["daemon"]);
        if let Some(data) = data {
            cmd.env(riff::run::DATA_VAR, data);
        }
        let child = cmd
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .unwrap();
        (child, log)
    };
    let (child, log) = start("machine.out", None);
    wait_for(&log, riff::text::STEP_UNIT).await;
    stop(child).await;

    let (child, log) = start("data.out", Some(&data));
    let socket = data.join("daemon/riff.sock");
    wait_for(&log, &format!("listens on {}", socket.display())).await;
    wait_for(&log, riff::text::INIT_RULES).await;
    stop(child).await;
    let said = std::fs::read_to_string(&log).unwrap();
    assert!(!said.contains(riff::text::STEP_UNIT), "{said}");
}

/// `riff daemon limit` shows the limit of the host, and sets it in the
/// folder of the daemon. `riff daemon status` shows it
/// (01M4ZV3B6HAFTSC2K63FMC5FYA). `--cpus` and `--memory` set the caps of
/// each new container, and riff refuses 0 (01M56Q4ASHTDWVA2MS2CBQ2PAM).
#[tokio::test]
async fn riff_daemon_limit_shows_and_sets_the_limit_of_the_host() {
    let here = Here::new();
    let out = here.run(false, &["daemon", "limit"]).await;
    assert_eq!(
        text(&out.stdout),
        "The limit of this host: 3 live sessions, and 8 GB of free memory or more for a start.\n\
         Each new container: 8 CPUs and 8 build jobs, and 8 GB of memory.\n"
    );
    let file = here.env.riff_home().join("daemon/limit.json");
    assert!(!file.exists());
    let out = here
        .run(
            false,
            &["daemon", "limit", "--sessions", "5", "--floor", "4"],
        )
        .await;
    let set =
        "The limit of this host: 5 live sessions, and 4 GB of free memory or more for a start.";
    assert_eq!(
        text(&out.stdout),
        format!("{set}\nEach new container: 8 CPUs and 8 build jobs, and 8 GB of memory.\n")
    );
    let out = here.run(false, &["daemon", "limit", "--floor", "6"]).await;
    assert!(text(&out.stdout).contains("5 live sessions, and 6 GB"));
    let out = here
        .run(false, &["daemon", "limit", "--cpus", "6", "--memory", "12"])
        .await;
    let caps = "Each new container: 6 CPUs and 6 build jobs, and 12 GB of memory.";
    assert!(text(&out.stdout).contains(caps), "{}", text(&out.stdout));
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(saved.contains(r#""cpus":6,"memory_gb":12"#), "{saved}");
    for zero in [["--cpus", "0"], ["--memory", "0"]] {
        let out = here
            .run(false, &["daemon", "limit", zero[0], zero[1]])
            .await;
        assert!(!out.status.success());
        assert!(text(&out.stderr).contains(&format!("riff refuses {} 0", zero[0])));
    }
    assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);

    let mut daemon = here.daemon("daemon.out").await;
    let out = here.run(false, &["daemon", "status"]).await;
    let status = text(&out.stdout);
    assert!(status.contains("5 live sessions, and 6 GB"), "{status}");
    assert!(status.contains(caps), "{status}");
    term(daemon.id());
    tokio::task::spawn_blocking(move || daemon.wait().unwrap())
        .await
        .unwrap();
}

/// A fake `gh` for the sweep of the folders of the sessions. It logs
/// `GH_REPO` and each call to `bin/gh.log`. On the forge:
///
/// | Issue | Its branch and the issue |
/// |---|---|
/// | 1 | the pull request #41 merged |
/// | 2 | the pull request #42 closed with no merge |
/// | 3 | no pull request, and the issue is closed |
/// | 4 | the pull request #44 is open, after the closed #43 |
/// | 5 | no pull request, and the issue is open |
/// | 6 | the pull request #46 closed with no merge |
fn fake_sweep_gh(bin: &Path) {
    let body = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$GH_REPO gh $*" >> "$dir/gh.log"
fields='--state all --json number,state,body,mergeCommit --limit 100'
case "$*" in
  "pr list --head worktree-issue-1 $fields")
    echo '[{"number":41,"state":"MERGED","body":"Closes #1","mergeCommit":{"oid":"c41"}}]' ;;
  "pr list --head worktree-issue-2 $fields") echo '[{"number":42,"state":"CLOSED","body":""}]' ;;
  "pr list --head worktree-issue-3 $fields") echo '[]' ;;
  'issue view 3 --json state') echo '{"state":"CLOSED"}' ;;
  "pr list --head worktree-issue-4 $fields")
    echo '[{"number":44,"state":"OPEN","body":""},{"number":43,"state":"CLOSED","body":""}]' ;;
  "pr list --head worktree-issue-5 $fields") echo '[]' ;;
  'issue view 5 --json state') echo '{"state":"OPEN"}' ;;
  "pr list --head worktree-issue-6 $fields") echo '[{"number":46,"state":"CLOSED","body":""}]' ;;
  *) exit 1 ;;
esac
"#;
    isolated::write_program(bin.join("gh"), body);
}

/// Makes the folder `name` of a session in `sessions`, with a clone whose
/// origin is `o/r` on GitHub. The clone is as a session leaves it after
/// its push: one commit, which `main` of `origin` has. It gives the
/// folder.
fn session_folder(sessions: &Path, name: &str) -> PathBuf {
    let clone = sessions.join(name).join("clone");
    std::fs::create_dir_all(&clone).unwrap();
    git(&clone, &["init", "-q", "-b", "main"]);
    git(&clone, &["commit", "-q", "--allow-empty", "-m", "start"]);
    git(
        &clone,
        &["remote", "add", "origin", "https://github.com/o/r.git"],
    );
    git(&clone, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    sessions.join(name)
}

/// 01M53NJ5Y2CMY5MWF9328KW7DE: `riff daemon sweep` removes the folder of
/// each session whose pull request merged or closed, or whose issue is
/// closed with no pull request, and prints each one with its reason. It
/// keeps the folder of an open pull request, of an open issue, and of a
/// `riff run` that holds its lock.
#[tokio::test]
async fn riff_daemon_sweep_removes_the_folders_whose_work_is_over_and_says_why() {
    let here = Here::new();
    fake_sweep_gh(here.bin.path());
    let sessions = here.env.riff_home().join("sessions");
    let merged = session_folder(&sessions, "issue-1-00000001");
    let closed = session_folder(&sessions, "verify-issue-2-00000002");
    let issue = session_folder(&sessions, "issue-3-00000003");
    let open = session_folder(&sessions, "issue-4-00000004");
    let open_issue = session_folder(&sessions, "issue-5-00000005");
    let runs = session_folder(&sessions, "issue-6-00000006");
    let lock = riff::daemon::sweep::hold(&runs).unwrap();
    // No sweep at the start: this test sweeps by hand.
    let daemon = here
        .daemon_with("daemon.out", &[(riff::daemon::sweep::EVERY_VAR, "0")])
        .await;

    let out = here.run(false, &["daemon", "sweep"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let removed =
        |folder: &Path, why: &str| format!("riff removed the folder {}: {why}.", folder.display());
    assert_eq!(
        text(&out.stdout),
        [
            removed(&merged, "the pull request #41 merged"),
            removed(
                &issue,
                "the issue #3 is closed, and its branch has no pull request"
            ),
            removed(&closed, "the pull request #42 closed with no merge"),
        ]
        .map(|line| format!("{line}\n"))
        .concat()
    );
    for gone in [&merged, &closed, &issue] {
        assert!(!gone.exists(), "{}", gone.display());
    }
    for kept in [&open, &open_issue, &runs] {
        assert!(kept.exists(), "{}", kept.display());
    }
    let log = here.read("gh.log");
    assert!(log.lines().all(|l| l.starts_with("o/r gh ")), "{log}");

    // The riff run ends: its folder goes, with its lock.
    drop(lock);
    let out = here.run(false, &["daemon", "sweep"]).await;
    assert_eq!(
        text(&out.stdout),
        format!(
            "{}\n",
            removed(&runs, "the pull request #46 closed with no merge")
        )
    );
    assert!(!runs.exists() && !riff::daemon::sweep::lock_of(&runs).exists());
    let out = here.run(false, &["daemon", "sweep"]).await;
    assert_eq!(
        text(&out.stdout),
        "riff removed no folder: the session of each folder runs, or its work is not over.\n"
    );
    stop(daemon).await;
}

/// 01M53S7WAXD46YPN0DR96XSTMG: `riff daemon sweep` keeps the folder of a
/// closed pull request while its clone has a commit that no branch of
/// `origin` has, and says why. After the push, it removes the folder.
#[tokio::test]
async fn riff_daemon_sweep_keeps_a_folder_with_a_commit_that_is_not_pushed() {
    let here = Here::new();
    fake_sweep_gh(here.bin.path());
    let sessions = here.env.riff_home().join("sessions");
    let ahead = session_folder(&sessions, "issue-6-00000006");
    let clone = ahead.join("clone");
    git(&clone, &["switch", "-q", "-c", "worktree-issue-6"]);
    git(
        &clone,
        &["commit", "-q", "--allow-empty", "-m", "not pushed"],
    );
    let daemon = here
        .daemon_with("daemon.out", &[(riff::daemon::sweep::EVERY_VAR, "0")])
        .await;

    let out = here.run(false, &["daemon", "sweep"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        format!(
            "riff kept the folder {}: the pull request #46 closed with no merge, but its clone \
             has 1 commit that is not pushed. Push the work, or remove the folder.\n",
            ahead.display()
        )
    );
    assert!(ahead.exists());

    git(
        &clone,
        &["update-ref", "refs/remotes/origin/worktree-issue-6", "HEAD"],
    );
    let out = here.run(false, &["daemon", "sweep"]).await;
    assert_eq!(
        text(&out.stdout),
        format!(
            "riff removed the folder {}: the pull request #46 closed with no merge.\n",
            ahead.display()
        )
    );
    assert!(!ahead.exists());
    stop(daemon).await;
}

/// 01M53NJ5V27BCHDNHK2THQQZZQ: the daemon sweeps by itself at its start,
/// with no wave and no step of a person.
#[tokio::test]
async fn the_daemon_removes_the_folder_of_a_closed_pull_request_at_its_start() {
    let here = Here::new();
    fake_sweep_gh(here.bin.path());
    let sessions = here.env.riff_home().join("sessions");
    let closed = session_folder(&sessions, "issue-2-00000002");
    let open = session_folder(&sessions, "issue-4-00000004");
    let daemon = here.daemon("daemon.out").await;
    wait_for(
        &here.bin.path().join("daemon.out"),
        &format!("riff removed the folder {}", closed.display()),
    )
    .await;
    assert!(!closed.exists());
    assert!(open.exists());
    stop(daemon).await;
}

impl Here {
    /// Starts `riff daemon` of the `RIFF_HOME`, writes its output to
    /// `name`, and waits until it listens.
    pub(crate) async fn daemon(&self, name: &str) -> std::process::Child {
        self.daemon_with(name, &[]).await
    }

    /// [`Here::daemon`] with the variables `vars`.
    async fn daemon_with(&self, name: &str, vars: &[(&str, &str)]) -> std::process::Child {
        let log = self.bin.path().join(name);
        let out = std::fs::File::create(&log).unwrap();
        let child = self
            .command(false, &["daemon"])
            .envs(vars.iter().copied())
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .unwrap();
        wait_for(&log, "The riff daemon listens on").await;
        child
    }

    /// The folder of the daemon of the `RIFF_HOME`.
    pub(crate) fn daemon_dir(&self) -> PathBuf {
        self.env.riff_home().join("daemon")
    }

    /// The folder of the secrets of the `RIFF_HOME`: the file store of
    /// the secrets of the tests stands for the OS keyring.
    fn secrets(&self) -> PathBuf {
        self.env.riff_home().join("secrets")
    }

    /// The text of the file `name` of the output of a daemon.
    fn output(&self, name: &str) -> String {
        std::fs::read_to_string(self.bin.path().join(name)).unwrap()
    }

    /// A fake `git` first in `PATH`: it logs each call to `git.log`, and
    /// runs the `git` of the machine. It gives the log.
    fn logged_git(&self) -> PathBuf {
        let real = std::env::var("PATH")
            .unwrap()
            .split(':')
            .map(|dir| Path::new(dir).join("git"))
            .find(|git| git.is_file())
            .unwrap();
        let log = self.bin.path().join("git.log");
        let script = format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nexec '{}' \"$@\"\n",
            log.display(),
            real.display()
        );
        isolated::write_program(self.bin.path().join("git"), &script);
        log
    }
}

/// Each file under `dir`, with its bytes. Empty when `dir` is not there.
fn files_of(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(at) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                todo.push(path);
            } else {
                files.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    files
}

/// Stops `child` with a SIGTERM, and waits for its end.
pub(crate) async fn stop(mut child: std::process::Child) {
    term(child.id());
    let status = tokio::task::spawn_blocking(move || child.wait().unwrap())
        .await
        .unwrap();
    assert!(status.success(), "{status}");
}

/// 01M53NGW3FR555TWP8Z8HHRVDJ, 01M53NGW6F5HP1DZ9QRGP50MGT: a start with
/// an empty fake keyring makes no key, opens no riff of peers, writes no
/// record and reads no claim on the forge (`git ls-remote`). `riff
/// daemon status` shows no key, no riff and no peer link.
///
/// 01M53NGWFBWR1WMJWC2DHVHJ3T: the state of the riff of peers that riff
/// 1.6 to 1.8 made on a machine is the same after a start, a status and
/// a stop: the folders `state` and `state.lost-MS`, the files
/// `invites.json` and `forges.json`, and the secret `daemon-key DIR` in
/// the fake keyring.
#[tokio::test]
async fn the_daemon_makes_no_key_and_no_riff_and_leaves_the_state_of_the_riff_of_peers() {
    let here = Here::new();
    let git = here.logged_git();
    let dir = here.daemon_dir();
    let shown = |status: &str| {
        for part in ["key", "Its riff", "owner", "peer", "link"] {
            assert!(!status.contains(part), "{part}: {status}");
        }
    };

    // An empty fake keyring, and no state.
    let daemon = here.daemon("empty.out").await;
    let out = here.run(false, &["daemon", "status"]).await;
    let status = text(&out.stdout);
    assert!(out.status.success(), "{status}");
    shown(&status);
    stop(daemon).await;
    assert_eq!(files_of(&here.secrets()), BTreeMap::new());
    for name in ["state", "invites.json", "forges.json"] {
        assert!(!dir.join(name).exists(), "{name}");
    }
    let said = here.output("empty.out");
    for part in ["its key", "its riff", "peer link", "iroh"] {
        assert!(!said.contains(part), "{part}: {said}");
    }

    // The state of riff 1.8, and its key in the fake keyring.
    let main = here.env.path().join("main");
    let old = [
        (
            "state/riff.json",
            r#"{"riff":"0a1b","owner":"mike"}"#.to_owned(),
        ),
        ("state/records", "a record of riff 1.8".to_owned()),
        ("state.lost-7/records", "an old record".to_owned()),
        ("invites.json", "[]".to_owned()),
        (
            "forges.json",
            serde_json::json!({ "o/r": main }).to_string(),
        ),
    ];
    for (name, bytes) in &old {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let key = riff::secrets::file_of(&here.secrets(), &format!("daemon-key {}", dir.display()));
    std::fs::create_dir_all(key.parent().unwrap()).unwrap();
    std::fs::write(&key, "a key of riff 1.8").unwrap();
    let peer_state = || {
        let mut files = files_of(&dir.join("state"));
        files.extend(files_of(&dir.join("state.lost-7")));
        for name in ["invites.json", "forges.json"] {
            files.insert(dir.join(name), std::fs::read(dir.join(name)).unwrap());
        }
        files.extend(files_of(&here.secrets()));
        files
    };
    let before = peer_state();
    assert_eq!(before.len(), 6);

    let daemon = here.daemon("old.out").await;
    let out = here.run(false, &["daemon", "status"]).await;
    let status = text(&out.stdout);
    assert!(out.status.success(), "{status}");
    shown(&status);
    stop(daemon).await;
    assert_eq!(peer_state(), before);
    let calls = std::fs::read_to_string(&git).unwrap_or_default();
    assert!(!calls.contains("ls-remote"), "{calls}");
}
