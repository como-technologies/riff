//! `riff daemon`, `riff daemon install` and `riff daemon status` (#716,
//! 01M4ZDTVK74596M0ZA46N51NKV, 01M4ZDTVNBKDM64043GGWQEV4S).
//!
//! A fake `systemctl` logs each call. Its `restart riff.service` starts
//! the `riff daemon` of this build in the background, as systemd does.
//! The test of `riff run` through the daemon is in `run.rs`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use isolated::Isolated;

use crate::run::{fake_systemctl, wait_for};

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// An environment with the fake `systemctl` first in `PATH`.
struct Here {
    env: Isolated,
    bin: tempfile::TempDir,
    path: String,
}

impl Here {
    fn new() -> Here {
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
    fn command(&self, machine: bool, args: &[&str]) -> Command {
        let mut cmd = self.env.riff();
        cmd.args(args)
            .env("PATH", &self.path)
            .env("FAKE_RIFF", self.env.riff_path());
        if machine {
            cmd.env_remove("RIFF_HOME");
        }
        cmd
    }

    async fn run(&self, machine: bool, args: &[&str]) -> Output {
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

#[tokio::test]
async fn riff_daemon_install_with_riff_home_installs_nothing() {
    let here = Here::new();
    let out = here.run(false, &["daemon", "install"]).await;
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("RIFF_HOME is set"), "{stderr}");
    assert!(!here.env.home().join(".config/systemd").exists());
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

impl Here {
    /// Starts `riff daemon` of the `RIFF_HOME`, writes its output to
    /// `name`, and waits until it listens.
    async fn daemon(&self, name: &str) -> std::process::Child {
        let log = self.bin.path().join(name);
        let out = std::fs::File::create(&log).unwrap();
        let child = self
            .command(false, &["daemon"])
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .unwrap();
        wait_for(&log, "The riff daemon listens on").await;
        child
    }

    /// The line `Its riff: ...` of `riff daemon status`.
    async fn riff_line(&self) -> String {
        let out = self.run(false, &["daemon", "status"]).await;
        let status = text(&out.stdout);
        assert!(out.status.success(), "{status}");
        status
            .lines()
            .find(|line| line.starts_with("Its riff: "))
            .unwrap_or_else(|| panic!("no riff line in\n{status}"))
            .to_owned()
    }
}

/// Stops `child` with a SIGTERM, and waits for its end.
async fn stop(mut child: std::process::Child) {
    term(child.id());
    let status = tokio::task::spawn_blocking(move || child.wait().unwrap())
        .await
        .unwrap();
    assert!(status.success(), "{status}");
}

/// 01M4ZHW2083VFTQJ772ZJ27XAC, 01M4ZHW24JZZXM3JJXG3PRJS7N: the first
/// start of the daemon makes the riff of the machine. A stop writes a
/// checkpoint, and the next start loads the same riff.
#[tokio::test]
async fn a_restart_of_the_daemon_keeps_its_riff() {
    let here = Here::new();
    let first = here.daemon("first.out").await;
    let line = here.riff_line().await;
    assert!(line.ends_with(" of its log."), "{line}");
    stop(first).await;
    let checkpoints = here.env.riff_home().join("daemon/state/checkpoint");
    assert_eq!(std::fs::read_dir(&checkpoints).unwrap().count(), 1);

    let second = here.daemon("second.out").await;
    assert_eq!(here.riff_line().await, line);
    stop(second).await;
}
