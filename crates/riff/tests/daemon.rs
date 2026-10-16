//! `riff daemon`, `riff daemon install`, `riff daemon uninstall` and
//! `riff daemon status` (#716, #736, 01M4ZDTVK74596M0ZA46N51NKV,
//! 01M501M0T0JBWGMMHCT7HT95T9, 01M4ZDTVNBKDM64043GGWQEV4S).
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
    // The isolated home has no OS keyring: the daemon waits for its key,
    // and says why (01M51JW20CSAECFRZTJ95P6F3E, 01M51JW233MXB72MHHRAX4E4NF).
    let waits = "The riff daemon waits for its key in the OS keyring: riff cannot open the OS \
                 keyring.";
    assert!(status.contains(waits), "{status}");
    assert!(status.contains("Its riff: none yet."), "{status}");
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

/// 01M51M3ZHHBE61CEG9T6ENFG18, 01M51M3ZKWQYBBPD97R0G9QAX0,
/// 01M51M3ZPAEMJ64266QSJBNQ46, 01M51M3ZRKQZ8KDVRGJR3XAY00,
/// 01M51M3ZTV61C9BMFT3J1M06NF: with `update.auto` on, a daemon whose
/// riff is in `SHA256SUMS` of the release of its version adds each
/// missing rule to the user settings of `CLAUDE_CONFIG_DIR` at its start.
/// It keeps each other rule and key, logs each rule, keeps the line of
/// the sync, and marks the add for the workers.
#[tokio::test]
async fn the_daemon_of_a_release_adds_the_missing_rules_at_its_start() {
    let Some(target) = riff::release::TARGET else {
        return;
    };
    let here = Here::new();
    // The release files of this version: the SHA-256 of this riff.
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
    wait_for(&log, "rules to").await;
    let after = std::fs::read_to_string(&settings).unwrap();
    let value: serde_json::Value = serde_json::from_str(&after).unwrap();
    assert_eq!(value["model"], "opus");
    assert_eq!(value["permissions"]["allow"][0], "Read(~/**)");
    let want = riff::permissions::user_rules(Some(&here.env.home()));
    assert!(riff::permissions::missing(&want, &[&after]).is_empty(), "{after}");
    let logged = std::fs::read_to_string(&log).unwrap();
    for rule in &want.allow {
        let added = format!("added the allow rule {rule} to {}", settings.display());
        assert!(logged.contains(&added), "{added}\nnot in\n{logged}");
    }
    let local = here.env.path().join("run/riff");
    assert!(riff::local::rules_added(&local).is_some());
    let last = riff::local::rules_sync(&local).unwrap();
    assert!(
        last.contains(&format!("added {} rules to {}", want.len(), settings.display())),
        "{last}"
    );

    let pid: u32 = here.read("daemon.pid").trim().parse().unwrap();
    term(pid);
    wait_for(&log, "The riff daemon stops").await;
}

/// 01M51M3ZKWQYBBPD97R0G9QAX0: a daemon whose riff is not in
/// `SHA256SUMS`, for example a dev build, changes no rule, also with
/// `update.auto` on.
#[tokio::test]
async fn the_daemon_of_a_dev_build_changes_no_rule() {
    let Some(target) = riff::release::TARGET else {
        return;
    };
    let here = Here::new();
    let tag = format!("v{}", env!("CARGO_PKG_VERSION"));
    let files = here.env.releases().join(&tag);
    std::fs::create_dir_all(&files).unwrap();
    let name = riff::release::asset("riff", target);
    let sum = riff::release::sha256(b"the riff of the release");
    std::fs::write(files.join("SHA256SUMS"), format!("{sum}  {name}\n")).unwrap();
    let claude = here.env.home().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    let settings = claude.join("settings.json");
    let before = r#"{"permissions": {"allow": ["Read(~/**)"]}}"#;
    std::fs::write(&settings, before).unwrap();
    let out = here.run(true, &["update", "--auto", "on"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));

    let out = here.run(true, &["daemon", "install"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let log = here.bin.path().join("daemon.out");
    wait_for(&log, "it is a check build or a dev build").await;
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), before);
    let local = here.env.path().join("run/riff");
    assert_eq!(riff::local::rules_added(&local), None);

    let pid: u32 = here.read("daemon.pid").trim().parse().unwrap();
    term(pid);
    wait_for(&log, "The riff daemon stops").await;
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
    assert!(stderr.contains("RIFF_HOME is set"), "{stderr}");
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

/// `riff daemon limit` shows the limit of the host, and sets it in the
/// folder of the daemon. `riff daemon status` shows it
/// (01M4ZV3B6HAFTSC2K63FMC5FYA).
#[tokio::test]
async fn riff_daemon_limit_shows_and_sets_the_limit_of_the_host() {
    let here = Here::new();
    let out = here.run(false, &["daemon", "limit"]).await;
    assert_eq!(
        text(&out.stdout),
        "The limit of this host: 3 live sessions, and 8 GB of free memory or more for a start.\n"
    );
    assert!(!here.env.riff_home().join("daemon/limit.json").exists());
    let out = here
        .run(
            false,
            &["daemon", "limit", "--sessions", "5", "--floor", "4"],
        )
        .await;
    let set =
        "The limit of this host: 5 live sessions, and 4 GB of free memory or more for a start.";
    assert_eq!(text(&out.stdout), format!("{set}\n"));
    let out = here.run(false, &["daemon", "limit", "--floor", "6"]).await;
    assert!(text(&out.stdout).contains("5 live sessions, and 6 GB"));

    let mut daemon = here.daemon("daemon.out").await;
    let out = here.run(false, &["daemon", "status"]).await;
    let status = text(&out.stdout);
    assert!(status.contains("5 live sessions, and 6 GB"), "{status}");
    term(daemon.id());
    tokio::task::spawn_blocking(move || daemon.wait().unwrap())
        .await
        .unwrap();
}

impl Here {
    /// Starts `riff daemon` of the `RIFF_HOME`, writes its output to
    /// `name`, and waits until it listens and has its riff.
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
        wait_for(&log, RIFF_LINE).await;
        child
    }

    /// The line `Its riff: ...` of `riff daemon status`.
    async fn riff_line(&self) -> String {
        self.status_line("Its riff: ").await
    }

    /// The line of `riff daemon status` that starts with `start`.
    async fn status_line(&self, start: &str) -> String {
        let out = self.run(false, &["daemon", "status"]).await;
        let status = text(&out.stdout);
        assert!(out.status.success(), "{status}");
        status
            .lines()
            .find(|line| line.starts_with(start))
            .unwrap_or_else(|| panic!("no line {start:?} in\n{status}"))
            .to_owned()
    }

    /// The folder of the daemon of the `RIFF_HOME`.
    fn daemon_dir(&self) -> PathBuf {
        self.env.riff_home().join("daemon")
    }

    /// The file of the key of the daemon of the `RIFF_HOME`: the file
    /// store of the secrets of the tests stands for the OS keyring.
    fn key_file(&self) -> PathBuf {
        let name = riff::daemon_key::secret_name(&self.daemon_dir());
        riff::secrets::file_of(&self.env.riff_home().join("secrets"), &name)
    }

    /// The text of the file `name` of the output of a daemon.
    fn output(&self, name: &str) -> String {
        std::fs::read_to_string(self.bin.path().join(name)).unwrap()
    }
}

/// A part of each line of a daemon that has its riff: it made it, made
/// a new one, or read it.
const RIFF_LINE: &str = ", of the owner ";

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

/// 01M519Z2BB0YJY5CRMB54JVYFA, 01M519Z2N2PMZJ0CYAPV0A47BZ: the first
/// start makes the key in the keyring and a riff whose ID is the public
/// key. `riff daemon status` shows the key, the riff and the user of the
/// owner. A second start loads the same key and the same riff.
#[tokio::test]
async fn riff_daemon_status_shows_the_key_the_riff_and_the_owner() {
    let here = Here::new();
    let first = here.daemon("first.out").await;
    let out = here.run(false, &["daemon", "status"]).await;
    let status = text(&out.stdout);
    assert!(out.status.success(), "{status}");
    let key = here.status_line("Its key: ").await["Its key: ".len()..].to_owned();
    assert_eq!(key.len(), 64, "{status}");
    assert!(
        status.contains(&format!("Its riff: {key}, at position ")),
        "{status}"
    );
    let user = std::env::var("USER").unwrap().to_lowercase();
    assert!(
        status.contains(&format!("The owner of its riff: {user}\n")),
        "{status}"
    );
    assert!(here.key_file().exists());
    let said = here.output("first.out");
    assert!(
        said.contains(&riff::text::daemon_made_riff(&key, &user)),
        "{said}"
    );
    stop(first).await;

    let second = here.daemon("second.out").await;
    assert_eq!(
        here.status_line("Its key: ").await,
        format!("Its key: {key}")
    );
    assert!(
        here.riff_line()
            .await
            .starts_with(&format!("Its riff: {key}, "))
    );
    let said = here.output("second.out");
    assert!(
        said.contains(&riff::text::daemon_read_riff(&key, &user)),
        "{said}"
    );
    assert!(!said.contains("made"), "{said}");
    stop(second).await;
}

/// 01M519Z2G77CT94Y6XPW9BCRK9, 01M51JW25SPDW564A3DG31D7XM: a state with
/// no key, and a key with no state, each make a new key and a new riff.
/// The log of the daemon says why. The old state goes to
/// `state.lost-MS`.
#[tokio::test]
async fn a_lost_key_or_a_lost_state_makes_a_new_key_and_a_new_riff() {
    let here = Here::new();
    let first = here.daemon("first.out").await;
    let key1 = here.status_line("Its key: ").await;
    stop(first).await;

    std::fs::remove_file(here.key_file()).unwrap();
    let second = here.daemon("second.out").await;
    let key2 = here.status_line("Its key: ").await;
    assert_ne!(key2, key1);
    let said = here.output("second.out");
    assert!(
        said.contains(
            "Why: its state has a riff, but the OS keyring has no key of the daemon. Its old \
             state is in "
        ),
        "{said}"
    );
    stop(second).await;
    let lost: Vec<_> = std::fs::read_dir(here.daemon_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("state.lost-"))
        .collect();
    assert_eq!(lost.len(), 1, "{lost:?}");
    assert!(said.contains(&lost[0]), "{said}");

    std::fs::remove_dir_all(here.daemon_dir().join("state")).unwrap();
    let third = here.daemon("third.out").await;
    let key3 = here.status_line("Its key: ").await;
    assert_ne!(key3, key2);
    assert_ne!(key3, key1);
    let said = here.output("third.out");
    assert!(
        said.contains("Why: the OS keyring has a key of the daemon, but its state has no riff."),
        "{said}"
    );
    let riff = here.riff_line().await;
    assert!(riff.starts_with(&format!("Its riff: {}, ", &key3["Its key: ".len()..])));
    stop(third).await;
}

/// 01M519Z2BB0YJY5CRMB54JVYFA: no file of the folder of the daemon and
/// no line of its log holds the secret key, as hex or as bytes.
#[tokio::test]
async fn no_file_of_the_daemon_and_no_line_of_its_log_holds_the_secret_key() {
    let here = Here::new();
    let first = here.daemon("first.out").await;
    here.riff_line().await;
    stop(first).await;
    let second = here.daemon("second.out").await;
    here.riff_line().await;
    stop(second).await;

    let hex = std::fs::read_to_string(here.key_file()).unwrap();
    assert_eq!(hex.len(), 64, "{hex}");
    let bytes: Vec<u8> = (0..32)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    let upper = hex.to_uppercase();
    let holds = |data: &[u8]| {
        data.windows(64)
            .any(|w| w == hex.as_bytes() || w == upper.as_bytes())
            || data.windows(32).any(|w| w == bytes.as_slice())
    };
    let mut files = vec![
        here.bin.path().join("first.out"),
        here.bin.path().join("second.out"),
    ];
    let mut dirs = vec![here.daemon_dir()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                dirs.push(entry.path());
            } else if kind.is_file() {
                files.push(entry.path());
            }
        }
    }
    assert!(files.len() > 4, "{files:?}");
    for file in &files {
        let data = std::fs::read(file).unwrap();
        assert!(!holds(&data), "{} holds the secret key", file.display());
    }
}

/// 01M51BFYNMYHSQ62BBJES3CCWT, 01M51JW233MXB72MHHRAX4E4NF: while the
/// keyring refuses (a secret file that riff may not read), the daemon
/// waits: one line, no new key and no riff. `riff daemon status` says
/// why. When the keyring answers, the daemon loads the same key.
#[tokio::test]
async fn a_locked_keyring_makes_the_daemon_wait_and_say_why() {
    use std::os::unix::fs::PermissionsExt;

    let here = Here::new();
    let first = here.daemon("first.out").await;
    let key = here.status_line("Its key: ").await;
    let riff = here.riff_line().await;
    stop(first).await;

    let secrets = here.env.riff_home().join("secrets");
    std::fs::set_permissions(&secrets, std::fs::Permissions::from_mode(0o000)).unwrap();
    let log = here.bin.path().join("locked.out");
    let out = std::fs::File::create(&log).unwrap();
    let child = here
        .command(false, &["daemon"])
        .env(riff::secrets::RETRY_VAR, "1")
        .stdout(out.try_clone().unwrap())
        .stderr(out)
        .spawn()
        .unwrap();
    wait_for(&log, "The riff daemon waits for its key in the OS keyring").await;
    let out = here.run(false, &["daemon", "status"]).await;
    let status = text(&out.stdout);
    assert!(out.status.success(), "{status}");
    assert!(
        status.contains("The riff daemon waits for its key in the OS keyring: the OS keyring of"),
        "{status}"
    );
    assert!(status.contains("Its riff: none yet."), "{status}");
    assert!(!status.contains("Its key"), "{status}");
    // The daemon serves while it waits: its sessions need no key.
    wait_for(&log, "The riff daemon listens on").await;
    // A second look gives no new line.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    std::fs::set_permissions(&secrets, std::fs::Permissions::from_mode(0o700)).unwrap();
    wait_for(&log, "The riff daemon read its key and its riff").await;
    assert_eq!(here.status_line("Its key: ").await, key);
    assert_eq!(here.riff_line().await, riff);
    stop(child).await;
    let said = std::fs::read_to_string(&log).unwrap();
    assert_eq!(said.matches("waits for its key").count(), 1, "{said}");
    assert_eq!(said.matches("answers again").count(), 1, "{said}");
    assert!(!said.contains("made"), "{said}");
    let lost = std::fs::read_dir(here.daemon_dir())
        .unwrap()
        .filter(|entry| {
            let name = entry.as_ref().unwrap().file_name();
            name.to_string_lossy().starts_with("state.lost-")
        })
        .count();
    assert_eq!(lost, 0);
}
