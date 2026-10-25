//! `riff init`, the first run of riff on a machine (#721,
//! 01M4ZJN6R68YBD3XMM725ZRJ7W).
//!
//! The facts of the machine are fakes: `RIFF_ETC` gives `subuid` and
//! `subgid`, `RIFF_CGROUP` gives the controllers of cgroup v2, and
//! `RIFF_PODMAN_INIT` gives the folder of the init of podman. A
//! fake `podman` logs each call. The fake `systemctl` of `run.rs` starts
//! the `riff daemon` of this build, as systemd does. The repository is a
//! real git repository with a `riff.toml`.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Output;

use isolated::Isolated;

use crate::book::{commands_of, each_is_real};
use crate::run::{fake_systemctl, wait_for};

/// The image that `riff.toml` of the repository pins.
const IMAGE: &str = "ghcr.io/como-technologies/riff-session@sha256:\
                     0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// The calls of `systemctl` in `log` that change the unit: each call
/// but `is-active`, which riff also makes to wait for a daemon that
/// starts (01M5MS2T8VY8NX4VBMQTCKWDKH).
fn changes(log: &str) -> String {
    log.lines()
        .filter(|l| !l.ends_with("is-active riff.service"))
        .map(|l| format!("{l}\n"))
        .collect()
}

/// The output of `cmd`, away from the runtime of the test.
async fn output(mut cmd: std::process::Command) -> Output {
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

fn script(path: &Path, body: &str) {
    isolated::write_program(path, body);
}

/// A fake `podman` that logs each call to `bin/podman.log`. With the
/// file `bin/no-podman`, `--version` fails. With `bin/no-image`, the
/// image is not here, and a pull removes the file.
fn fake_podman(bin: &Path) {
    let body = r#"#!/bin/sh
dir=$(dirname "$0")
echo "podman $*" >> "$dir/podman.log"
case "$1" in
  --version)
    if [ -f "$dir/no-podman" ]; then echo "podman: no runtime" >&2; exit 125; fi
    echo "podman version 5.7.0" ;;
  image) [ -f "$dir/no-image" ] && exit 1; exit 0 ;;
  pull) rm -f "$dir/no-image" ;;
  *) exit 2 ;;
esac
exit 0
"#;
    script(&bin.join("podman"), body);
}

/// A machine with fakes, and a repository that pins [`IMAGE`].
struct Machine {
    env: Isolated,
    bin: PathBuf,
    etc: PathBuf,
    cgroup: PathBuf,
    /// The folder of the init of podman.
    init: PathBuf,
    repo: PathBuf,
    uid: u32,
}

impl Machine {
    /// A machine with each part.
    fn new() -> Machine {
        let env = Isolated::new();
        let root = env.path().to_owned();
        let (bin, etc, cgroup, init, repo) = (
            root.join("bin"),
            root.join("etc"),
            root.join("cgroup"),
            root.join("podman-init"),
            root.join("repo"),
        );
        for dir in [&bin, &etc, &cgroup, &init, &repo] {
            std::fs::create_dir_all(dir).unwrap();
        }
        fake_podman(&bin);
        script(&init.join("catatonit"), "#!/bin/sh\nexec \"$@\"\n");
        fake_systemctl(&bin);
        let uid = std::fs::metadata(&root).unwrap().uid();
        // By the user ID: the test does not know the login name.
        std::fs::write(etc.join("subuid"), format!("{uid}:100000:65536\n")).unwrap();
        std::fs::write(etc.join("subgid"), format!("{uid}:100000:65536\n")).unwrap();
        std::fs::write(
            cgroup.join("cgroup.controllers"),
            "cpuset cpu io memory pids\n",
        )
        .unwrap();
        let user = cgroup.join(format!("user.slice/user-{uid}.slice/user@{uid}.service"));
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(user.join("cgroup.controllers"), "cpu memory pids\n").unwrap();
        let status = env
            .command("git")
            .args(["init", "-q"])
            .arg(&repo)
            .status()
            .unwrap();
        assert!(status.success());
        std::fs::write(
            repo.join("riff.toml"),
            format!("[session]\nimage = \"{IMAGE}\"\n"),
        )
        .unwrap();
        Machine {
            env,
            bin,
            etc,
            cgroup,
            init,
            repo,
            uid,
        }
    }

    /// `riff init ARGS` in the folder `dir`, as the riff of the machine:
    /// no `RIFF_HOME`.
    async fn init(&self, dir: &Path, args: &[&str]) -> Output {
        let cmd = self.init_command(dir, args);
        output(cmd).await
    }

    /// The command of [`Machine::init`].
    fn init_command(&self, dir: &Path, args: &[&str]) -> std::process::Command {
        let mut cmd = self.command(dir, &["init"]);
        cmd.args(args);
        cmd
    }

    /// `riff ARGS` in the folder `dir`, as the riff of the machine, with
    /// the fakes.
    fn command(&self, dir: &Path, args: &[&str]) -> std::process::Command {
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(dir)
            .env_remove("RIFF_HOME")
            .env(
                "PATH",
                format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("FAKE_RIFF", self.env.riff_path())
            .env("RIFF_ETC", &self.etc)
            .env("RIFF_CGROUP", &self.cgroup)
            .env("RIFF_PODMAN_INIT", &self.init);
        cmd
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.bin.join(name)).unwrap_or_default()
    }

    fn unit(&self) -> PathBuf {
        self.env.home().join(".config/systemd/user/riff.service")
    }

    /// The user settings of Claude Code in the home of the machine.
    fn settings(&self) -> PathBuf {
        self.env.home().join(".claude/settings.json")
    }

    /// Writes `text` to the user settings.
    fn write_settings(&self, text: &str) {
        let path = self.settings();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

#[tokio::test]
async fn riff_init_installs_the_daemon_and_the_daemon_pulls_the_image() {
    let machine = Machine::new();
    std::fs::write(machine.bin.join("no-image"), "").unwrap();
    machine.write_settings(r#"{"model": "opus"}"#);
    let out = machine.init(&machine.repo, &["--yes"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    let uid = machine.uid;
    let settings = machine.settings();
    for line in [
        "riff checks this machine:".to_owned(),
        "  pass  podman: podman version 5.7.0".to_owned(),
        format!(
            "  pass  the init of podman: {}",
            machine.init.join("catatonit").display()
        ),
        format!("  pass  cgroup v2: user@{uid}.service has cpu and memory"),
        format!("  pass  the session image: {IMAGE}"),
        format!(
            "riff wrote the unit {}, and started it again.",
            machine.unit().display()
        ),
        format!("The riff daemon pulls the image {IMAGE}."),
        format!("This machine has the session image {IMAGE}."),
        "This machine is ready. Run one item in a container: riff run ITEM".to_owned(),
    ] {
        assert!(
            stdout.lines().any(|l| l == line),
            "{line}\nnot in\n{stdout}"
        );
    }
    for name in ["/etc/subuid", "/etc/subgid"] {
        assert!(
            stdout.contains(&format!("  pass  {name}: 65536 IDs for ")),
            "{stdout}"
        );
    }
    assert!(stdout.contains("The riff daemon runs: pid "), "{stdout}");
    assert_eq!(
        changes(&machine.read("systemctl.log")),
        "systemctl --user daemon-reload\n\
         systemctl --user enable riff.service\n\
         systemctl --user restart riff.service\n"
    );
    let pulls = |log: &str| log.lines().filter(|l| l.starts_with("podman pull")).count();
    let log = machine.read("podman.log");
    assert!(log.contains(&format!("podman pull {IMAGE}\n")), "{log}");
    assert_eq!(pulls(&log), 1, "{log}");
    // 01M5084K09Z2S7N57315CX2R0F: riff init writes no rule into the user
    // settings.
    assert_eq!(
        std::fs::read_to_string(&settings).unwrap(),
        r#"{"model": "opus"}"#
    );

    // A second run keeps the daemon and the settings, and pulls nothing.
    let before = std::fs::read_to_string(&settings).unwrap();
    let out = machine.init(&machine.repo, &[]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(!stdout.contains("the rules of Claude Code"), "{stdout}");
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), before);
    assert!(
        stdout.contains("The riff daemon of this riff runs already: pid "),
        "{stdout}"
    );
    assert!(stdout.contains("riff init keeps it."), "{stdout}");
    // No second install and no second restart.
    assert_eq!(changes(&machine.read("systemctl.log")).lines().count(), 3);
    assert_eq!(pulls(&machine.read("podman.log")), 1);

    // systemd stops the daemon with a SIGTERM.
    let pid: i32 = machine.read("daemon.pid").trim().parse().unwrap();
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGTERM,
    )
    .unwrap();
    wait_for(&machine.bin.join("daemon.out"), "The riff daemon stops").await;
}

#[tokio::test]
async fn riff_init_names_the_fix_of_each_missing_part_and_installs_nothing() {
    let machine = Machine::new();
    std::fs::write(machine.bin.join("no-podman"), "").unwrap();
    // No init of podman (01M511JYPZ8H73DCJ7867YSFGH).
    std::fs::remove_file(machine.init.join("catatonit")).unwrap();
    std::fs::write(machine.etc.join("subuid"), "someone:100000:65536\n").unwrap();
    std::fs::remove_file(machine.etc.join("subgid")).unwrap();
    std::fs::remove_file(machine.cgroup.join("cgroup.controllers")).unwrap();
    // The isolated folder is in no git repository.
    let out = machine.init(machine.env.path(), &[]).await;
    assert_eq!(out.status.code(), Some(1));
    let stdout = text(&out.stdout);
    let failed: Vec<&str> = stdout
        .lines()
        .filter(|l| l.starts_with("  fail  "))
        .collect();
    let starts = [
        "  fail  podman: podman --version failed: podman: no runtime. Fix: ",
        "  fail  the init of podman: podman has no init: no catatonit in ",
        "  fail  /etc/subuid: 0 IDs for ",
        "  fail  /etc/subgid: riff cannot read /etc/subgid. Fix: ",
        "  fail  cgroup v2: this machine has no cgroup v2. Fix: ",
        "  fail  the session image: ",
    ];
    assert_eq!(failed.len(), starts.len(), "{stdout}");
    for (line, start) in failed.iter().zip(starts) {
        assert!(
            line.starts_with(start),
            "{start}\nis not the start of\n{line}"
        );
    }
    let mut fixes: Vec<&str> = failed
        .iter()
        .map(|l| l.split_once(". Fix: ").expect("a fix").1)
        .collect();
    fixes.sort_unstable();
    fixes.dedup();
    assert_eq!(fixes.len(), starts.len(), "each part has its own fix");
    // The range of someone else ends at 165535: the fix starts after it.
    assert!(
        stdout.contains("sudo usermod --add-subuids 165536-231071 "),
        "{stdout}"
    );
    assert!(
        stdout.contains("sudo usermod --add-subgids 100000-165535 "),
        "{stdout}"
    );
    // riff init writes no user settings (01M5084K09Z2S7N57315CX2R0F).
    assert!(!machine.settings().exists());
    assert!(
        stdout.contains(
            "6 checks failed. riff installed no daemon and pulled no image. Do each fix, then run \
             riff init again:\n"
        ),
        "{stdout}"
    );
    // The end names each failed check with its fix (01M5RGGD6YR36EXZEG61XCJVRY).
    let named = named_at_the_end(&stdout);
    let names: Vec<&str> = named
        .iter()
        .map(|l| l.trim_start().split_once(": ").expect("a name and a fix").0)
        .collect();
    assert_eq!(
        names,
        [
            "podman",
            "the init of podman",
            "/etc/subuid",
            "/etc/subgid",
            "cgroup v2",
            "the session image"
        ],
        "{stdout}"
    );
    assert!(
        named[2].contains("sudo usermod --add-subuids 165536-231071 "),
        "{}",
        named[2]
    );
    assert_eq!(machine.read("systemctl.log"), "");
    assert_eq!(machine.read("podman.log"), "podman --version\n");
    assert!(!machine.unit().exists());
}

/// 01M5MS2T8VY8NX4VBMQTCKWDKH: while systemd starts the unit, `riff daemon status` waits for
/// the daemon and shows it. With no unit that runs, it fails at once.
#[tokio::test]
async fn riff_daemon_status_waits_while_the_unit_starts() {
    let machine = Machine::new();
    let out = machine
        .command(&machine.repo, &["daemon", "status"])
        .output()
        .unwrap();
    let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
    assert!(!out.status.success(), "{all}");
    assert!(all.contains("No riff daemon answers on"), "{all}");

    // The unit is active, and its daemon listens one second later.
    let base = machine.command(&machine.repo, &["daemon"]);
    let mut late = std::process::Command::new("sh");
    late.args(["-c", "sleep 1; exec \"$@\"", "sh"])
        .arg(base.get_program())
        .arg("daemon")
        .current_dir(&machine.repo)
        .stdout(std::fs::File::create(machine.bin.join("daemon.out")).unwrap())
        .stderr(std::process::Stdio::null());
    for (key, value) in base.get_envs() {
        match value {
            Some(value) => late.env(key, value),
            None => late.env_remove(key),
        };
    }
    let mut daemon = late.spawn().unwrap();
    std::fs::write(machine.bin.join("daemon.pid"), daemon.id().to_string()).unwrap();
    let out = output(machine.command(&machine.repo, &["daemon", "status"])).await;
    assert!(
        out.status.success(),
        "{}\n{}",
        text(&out.stdout),
        text(&out.stderr)
    );
    assert!(
        text(&out.stdout).starts_with("The riff daemon runs: "),
        "{}",
        text(&out.stdout)
    );

    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(daemon.id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    )
    .unwrap();
    wait_for(&machine.bin.join("daemon.out"), "The riff daemon stops").await;
    daemon.wait().unwrap();
}

#[tokio::test]
async fn riff_init_check_changes_nothing() {
    let machine = Machine::new();
    let out = machine.init(&machine.repo, &["--check"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert_eq!(stdout.matches("  pass  ").count(), 7, "{stdout}");
    assert!(
        stdout.ends_with("Each check passed. riff changed nothing: --check.\n"),
        "{stdout}"
    );
    assert_eq!(machine.read("systemctl.log"), "");
    assert_eq!(machine.read("podman.log"), "podman --version\n");
    assert!(!machine.unit().exists());
}

impl Machine {
    /// Breaks each part of the machine that `riff init` checks: no
    /// podman, no init of podman, no `subuid`, no `subgid`, no cgroup v2.
    fn break_machine(&self) {
        std::fs::write(self.bin.join("no-podman"), "").unwrap();
        std::fs::remove_file(self.init.join("catatonit")).unwrap();
        std::fs::remove_file(self.etc.join("subuid")).unwrap();
        std::fs::remove_file(self.etc.join("subgid")).unwrap();
        std::fs::remove_file(self.cgroup.join("cgroup.controllers")).unwrap();
    }

    /// `riff init ARGS` in the folder `dir`, in a session container of the
    /// daemon: `RIFF_SOCKET` is set.
    async fn container_init(&self, dir: &Path, args: &[&str]) -> Output {
        let mut cmd = self.init_command(dir, args);
        cmd.env("RIFF_SOCKET", "/run/riff/riff.sock");
        output(cmd).await
    }
}

/// The names of the checks in the lines `lines`: the text between the
/// mark and the first colon.
fn names_of<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    lines
        .iter()
        .map(|l| l.trim_start().split_once("  ").unwrap().1)
        .map(|l| l.split(':').next().unwrap())
        .collect()
}

/// 01M5SMJMR5KS1Z1Z7CEPH9JVBB: `riff init --check` in a session container
/// (`RIFF_SOCKET` is set) prints no line of podman, its init, `subuid`,
/// `subgid` or cgroup v2, prints the line of the skip, and runs the checks
/// of the repository. On a host, the same machine prints the five lines.
#[tokio::test]
async fn riff_init_check_in_a_session_container_skips_the_checks_of_the_machine() {
    let machine = Machine::new();
    machine.add_origin();
    bare(&machine.bin);
    machine.break_machine();

    let host = text(&machine.init(&machine.repo, &["--check"]).await.stdout);
    assert_eq!(
        names_of(&failed_lines(&host))[..5],
        [
            "podman",
            "the init of podman",
            "/etc/subuid",
            "/etc/subgid",
            "cgroup v2"
        ],
        "{host}"
    );
    assert!(!host.contains("the checks of the machine"), "{host}");

    let out = machine.container_init(&machine.repo, &["--check"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.starts_with(
            "riff checks this machine:\n  skip  the checks of the machine: the host runs them\n"
        ),
        "{stdout}"
    );
    for word in ["podman", "subuid", "subgid", "cgroup"] {
        assert!(!stdout.contains(word), "{word}\nin\n{stdout}");
    }
    assert_eq!(
        names_of(&failed_lines(&stdout)),
        [
            "riff.toml on main",
            "auto-merge and squash merge",
            "the ruleset of main",
            "the milestone Backlog",
            "the first wave"
        ],
        "{stdout}"
    );
    assert!(stdout.contains("\n5 checks failed. "), "{stdout}");
    assert!(machine.forge_changes().is_empty());
    assert_eq!(machine.read("systemctl.log"), "");
}

/// 01M5SMJMR5KS1Z1Z7CEPH9JVBB: `riff init` in a session container, where
/// each check of the repository passes, installs no daemon and pulls no
/// image: the host has them.
#[tokio::test]
async fn riff_init_in_a_session_container_installs_no_daemon() {
    let machine = Machine::new();
    machine.add_origin();
    set_up(&machine.bin);
    machine.break_machine();

    let host = machine.init(&machine.repo, &[]).await;
    assert_eq!(host.status.code(), Some(1), "{}", text(&host.stdout));

    for args in [&[][..], &["--yes"]] {
        let out = machine.container_init(&machine.repo, args).await;
        let stdout = text(&out.stdout);
        assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
        assert!(
            stdout.contains("  skip  the checks of the machine: the host runs them\n"),
            "{stdout}"
        );
        assert!(!stdout.contains("fail  "), "{stdout}");
        assert!(
            stdout.ends_with(
                "Each check passed. riff installed no daemon and pulled no image: the host has \
                 the daemon.\n"
            ),
            "{stdout}"
        );
        assert!(!stdout.contains("This machine is ready."), "{stdout}");
    }
    assert_eq!(machine.read("systemctl.log"), "");
    assert!(!machine.unit().exists());
    assert!(machine.forge_changes().is_empty());
}

/// 01M510NW68CTZBHHW7ZYFV7Q3X: the riff of a plain `cargo build` links
/// the libc of the machine. `riff init` gives one failed check for it,
/// with the fix `riff update`, and installs nothing. The test
/// environment takes such a riff, so this test removes `RIFF_DYNAMIC`.
#[cfg(target_env = "gnu")]
#[tokio::test]
async fn riff_init_with_the_riff_of_a_plain_cargo_build_fails_one_check() {
    let machine = Machine::new();
    let riff = std::fs::canonicalize(machine.env.riff_path()).unwrap();
    for args in [&["--check"][..], &[]] {
        let mut cmd = machine.init_command(&machine.repo, args);
        cmd.env_remove("RIFF_DYNAMIC");
        let out = output(cmd).await;
        let stdout = text(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {stdout}");
        let failed: Vec<&str> = stdout
            .lines()
            .filter(|l| l.starts_with("  fail  "))
            .collect();
        assert_eq!(failed.len(), 1, "{args:?}: {stdout}");
        let start = format!(
            "  fail  the riff binary: {}: it needs the shared libraries of /",
            riff.display()
        );
        assert!(
            failed[0].starts_with(&start),
            "{start}
{}",
            failed[0]
        );
        assert!(
            failed[0].ends_with(
                ": it is not static. Fix: run riff update. It installs the static riff and \
                 riff-server of the release, so riff relay starts in each session image"
            ),
            "{}",
            failed[0]
        );
        assert!(stdout.contains("\n1 check failed. "), "{stdout}");
        assert_eq!(machine.read("systemctl.log"), "", "{args:?}");
        assert!(!machine.unit().exists(), "{args:?}");
    }
}

/// 01M51QFPYHK0916C5ZN2B55BR0, 01M51QFPW37EXVH4AXH70MSXBT,
/// 01M51QFPMSG81YRDDGQQ1WJDSQ: `riff update --check` shows each step of
/// an update, in step or not, with its fix, and changes nothing. A dev
/// build with `RIFF_RELEASES` is not a release. The step that only a
/// person can do goes to the local file of the status line, until it is
/// in step.
#[tokio::test]
async fn riff_update_check_shows_each_step_and_changes_nothing() {
    use riff::text;
    let machine = Machine::new();
    std::fs::write(machine.bin.join("no-image"), "").unwrap();
    std::fs::remove_file(machine.init.join("catatonit")).unwrap();
    let before = r#"{"model": "opus"}"#;
    machine.write_settings(before);
    let check = || output(machine.command(&machine.repo, &["update", "--check"]));
    let out = check().await;
    let stdout = self::text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{stdout}\n{}",
        self::text(&out.stderr)
    );
    let releases = machine.env.releases().display().to_string();
    let no_init = text::init_no_init(&riff::first_run::INIT_DIRS);
    for line in [
        text::steps_check_gate(&riff::update_steps::Gate::Off),
        format!(
            "not in step  the riff binary: {}. Fix: riff update",
            text::not_release_releases(&releases)
        ),
        "not in step  the unit of the daemon: no unit of the riff daemon is installed. Fix: riff \
         daemon install"
            .to_owned(),
        format!(
            "not in step  the session image of this folder: podman does not have {IMAGE}. Fix: \
             podman pull {IMAGE}"
        ),
        "in step  podman: podman version 5.7.0".to_owned(),
        format!(
            "needs you  the init of podman: {no_init}. Fix: {}",
            text::INIT_FIX_INIT
        ),
    ] {
        assert!(
            stdout.lines().any(|l| l == line),
            "{line}\nnot in\n{stdout}"
        );
    }
    // It changes nothing.
    assert_eq!(std::fs::read_to_string(machine.settings()).unwrap(), before);
    assert!(!machine.unit().exists());
    assert_eq!(machine.read("systemctl.log"), "");
    let log = machine.read("podman.log");
    assert!(!log.contains("podman pull"), "{log}");
    // The status line shows the step that needs the person.
    let local = machine.env.path().join("run/riff");
    let fix = format!("the init of podman: {}", text::INIT_FIX_INIT);
    assert_eq!(riff::local::needs_you(&local), Some(fix));

    // After the fix of the person, the step is in step, and the file goes.
    script(&machine.init.join("catatonit"), "#!/bin/sh\nexec \"$@\"\n");
    let out = check().await;
    let stdout = self::text(&out.stdout);
    let line = format!(
        "in step  the init of podman: {}",
        machine.init.join("catatonit").display()
    );
    assert!(
        stdout.lines().any(|l| l == line),
        "{line}\nnot in\n{stdout}"
    );
    assert_eq!(riff::local::needs_you(&local), None);
}

/// 01M51JCW04FAHZDQ144KDNCCPX, 01M51JCW2EKF8H8QFQYSVRSPET: `riff init`
/// names each wide allow rule of the user settings and of the local
/// settings of the repository, with its file. A wide rule fails no
/// check. `--check`, `--yes` and a run with no terminal ask nothing and
/// remove nothing.
#[tokio::test]
async fn riff_init_names_each_wide_rule_and_removes_none_with_no_yes() {
    let machine = Machine::new();
    machine.write_settings(
        &serde_json::json!({ "permissions": { "allow": ["Bash(riff *)", "Bash(gh pr merge * --auto --squash)"] } })
            .to_string(),
    );
    let local = machine.repo.join(".claude/settings.local.json");
    std::fs::create_dir_all(local.parent().unwrap()).unwrap();
    std::fs::write(
        &local,
        r#"{"permissions": {"allow": ["Bash(git push *)", "Bash(cargo test:*)"]}}"#,
    )
    .unwrap();
    let before = |path: &Path| std::fs::read_to_string(path).unwrap();
    let (user_text, local_text) = (before(&machine.settings()), before(&local));
    let local = std::fs::canonicalize(&machine.repo)
        .unwrap()
        .join(".claude/settings.local.json");
    let report = format!(
        "These allow rules of Claude Code are wider than the steps of riff. Each one also \
         allows commands that no step of riff needs:\n  {}:\n    allow Bash(gh pr merge * \
         --auto --squash)\n  {}:\n    allow Bash(git push *)\nriff removed no rule. To remove \
         them, run riff init in a terminal, and answer y.\n",
        machine.settings().display(),
        local.display()
    );

    let out = machine.init(&machine.repo, &["--check"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(stdout.contains(&report), "{report}\nnot in\n{stdout}");
    assert!(
        stdout.ends_with("Each check passed. riff changed nothing: --check.\n"),
        "{stdout}"
    );

    // podman fails, so riff stops before the daemon.
    std::fs::write(machine.bin.join("no-podman"), "").unwrap();
    for args in [&["--yes"][..], &[]] {
        let out = machine.init(&machine.repo, args).await;
        let stdout = text(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {stdout}");
        assert!(stdout.contains(&report), "{args:?}: {stdout}");
        assert!(!stdout.contains("[y/N]"), "{args:?}: {stdout}");
        assert!(stdout.contains("\n1 check failed. "), "{args:?}: {stdout}");
    }
    assert_eq!(before(&machine.settings()), user_text);
    assert_eq!(before(&local), local_text);
    assert!(!machine.unit().exists());
}

/// The fake forge `tests/fake/forge` as the `gh` of the machine: it
/// keeps its state in the files of `bin`, and logs each call to
/// `bin/calls`. The `state` files give the repository; the files
/// `bin/not_admin` and `bin/no_login` change the person.
fn fake_forge(bin: &Path, state: &[(&str, &str)]) {
    script(&bin.join("gh"), include_str!("fake/forge"));
    for (name, text) in state {
        std::fs::write(bin.join(name), text).unwrap();
    }
}

/// A pull request that ran the checks of `names`, as the files of the
/// fake forge give it.
fn ci(names: &str) -> [(&'static str, String); 2] {
    [
        (
            "prior_pulls",
            r#"[{"number":3,"html_url":"https://github.com/acme/app/pull/3","head":{"ref":"feature","sha":"abc1234"},"base":{"ref":"main"}}]"#.to_owned(),
        ),
        (
            "checks_abc1234",
            format!(r#"{{"check_runs":[{names}]}}"#),
        ),
    ]
}

/// A repository with no `riff.toml`, no auto-merge, no ruleset and no
/// milestone, whose latest pull request ran two checks.
fn bare(bin: &Path) {
    let [pulls, checks] = ci(r#"{"name":"checks / lint"},{"name":"checks / test"}"#);
    fake_forge(
        bin,
        &[
            ("files", ""),
            ("milestones", ""),
            (pulls.0, &pulls.1),
            (checks.0, &checks.1),
        ],
    );
}

/// A repository that is set up for riff.
fn set_up(bin: &Path) {
    let [pulls, checks] = ci(r#"{"name":"Gate"}"#);
    fake_forge(
        bin,
        &[
            ("files", "riff.toml\n"),
            ("milestones", "Wave 1\nBacklog\n"),
            ("auto_merge", ""),
            (
                "ruleset",
                r#"{"rules":[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"Gate"},{"context":"riff/verify"}]}}]}"#,
            ),
            (pulls.0, &pulls.1),
            (checks.0, &checks.1),
        ],
    );
}

impl Machine {
    /// Gives the repository of the machine a remote `origin` on GitHub.
    fn add_origin(&self) {
        let status = self
            .env
            .command("git")
            .args(["-C"])
            .arg(&self.repo)
            .args(["remote", "add", "origin", "https://github.com/acme/app.git"])
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// The calls of the fake forge that change it.
    fn forge_changes(&self) -> Vec<String> {
        self.read("calls")
            .lines()
            .filter(|l| l.contains(" -X ") || l.starts_with("label "))
            .map(str::to_owned)
            .collect()
    }
}

/// The lines of `stdout` that start with `fail  `.
fn failed_lines(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|l| l.starts_with("  fail  "))
        .collect()
}

/// The lines of `stdout` that start with `step  `: the checks that a step
/// of the setup fixes (01M5RGG9KJXCKH8C94145WS8SM).
fn step_lines(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|l| l.starts_with("  step  "))
        .collect()
}

/// The lines at the end of `stdout` after "N checks failed": `  NAME: FIX`
/// for each failed check (01M5RGGD6YR36EXZEG61XCJVRY).
fn named_at_the_end(stdout: &str) -> Vec<&str> {
    let mut lines = stdout
        .lines()
        .skip_while(|l| !l.contains(" failed. riff installed no daemon"));
    lines.next();
    lines.collect()
}

/// 01M5QVF5535YRN1EF3DZAB4W8G, 01M5R1672NF56P15VD0JEX51K4: `riff init
/// --check` in a clone of a repository with no `riff.toml`, no
/// auto-merge, no ruleset that requires `riff/verify`, no `Backlog` and no
/// wave names the five steps, and changes nothing.
#[tokio::test]
async fn riff_init_check_names_the_github_step_of_each_fact_that_fails() {
    let machine = Machine::new();
    machine.add_origin();
    bare(&machine.bin);
    let out = machine.init(&machine.repo, &["--check"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{stdout}\n{}",
        text(&out.stderr)
    );
    assert_eq!(
        failed_lines(&stdout),
        [
            "  fail  riff.toml on main: main of acme/app has no riff.toml, so riff lead start \
             stops. Fix: put riff.toml with the session image on main. See \"Add another \
             repository to the riff\" in the book",
            "  fail  auto-merge and squash merge: acme/app allows no auto-merge. Fix: allow \
             them: gh api -X PATCH repos/acme/app -F allow_auto_merge=true -F \
             allow_squash_merge=true",
            "  fail  the ruleset of main: no ruleset of main requires the check riff/verify, \
             so a merge does not wait for the verify. Fix: make a ruleset of main that \
             requires a pull request and the checks Gate and riff/verify. See \"Add another \
             repository to the riff\" in the book",
            "  fail  the milestone Backlog: acme/app has no open milestone Backlog. Fix: make \
             it: gh api -X POST repos/acme/app/milestones -f title=Backlog. riff wave make \
             makes it too, with the next wave",
            "  fail  the first wave: acme/app has no open wave. Fix: make it: riff wave make \
             \"Wave 1\"",
        ],
        "{stdout}"
    );
    assert!(
        stdout.contains("5 checks failed. riff installed no daemon and pulled no image."),
        "{stdout}"
    );
    // --check only reads: it plans no step, so the lines say `fail`.
    assert!(step_lines(&stdout).is_empty(), "{stdout}");
    assert!(!stdout.contains("These steps change GitHub"), "{stdout}");
    // riff reads the repository, and changes nothing: no write call.
    assert!(
        machine.forge_changes().is_empty(),
        "{}",
        machine.read("calls")
    );
    assert_eq!(machine.read("systemctl.log"), "");
    assert!(!machine.unit().exists());
}

/// 01M5R1672NF56P15VD0JEX51K4: `riff init --yes` in a clone of a
/// repository with no `riff.toml`, no auto-merge, no ruleset and no
/// milestone makes the pull request with `riff.toml` and merges it,
/// allows auto-merge and squash merge, makes the ruleset with
/// `riff/verify` and the checks of the CI, and makes the two milestones.
/// The repository has no `CLAUDE.md`: each check of the repository passes
/// or has its step, and riff init prints no line for `CLAUDE.md`, also in
/// the second list (01M5RN89Z749VYS1CRYJQTKAWY).
/// A second run makes no call that changes the forge, and asks nothing.
#[tokio::test]
async fn riff_init_yes_sets_up_a_new_repository_and_a_second_run_changes_nothing() {
    let machine = Machine::new();
    machine.add_origin();
    bare(&machine.bin);
    std::fs::write(machine.bin.join("no-image"), "").unwrap();
    let out = machine.init(&machine.repo, &["--yes"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    for line in [
        "riff can set up acme/app for riff. These steps change GitHub:",
        "  allow auto-merge and squash merge",
        "  make riff.toml on main with a pull request",
        "  make the milestone Backlog",
        "  make the milestone Wave 1",
        "  make the ruleset riff on main",
        "  done  allow auto-merge and squash merge: allowed both",
        "  done  make riff.toml on main with a pull request: opened \
         https://github.com/acme/app/pull/7 and merged it",
        "  done  make the milestone Backlog: made Backlog",
        "  done  make the milestone Wave 1: made Wave 1",
        "  done  make the ruleset riff on main: requires checks / lint and checks / test and \
         riff/verify",
    ] {
        assert!(
            stdout.lines().any(|l| l == line),
            "{line}\nnot in\n{stdout}"
        );
    }
    // The clone has its own riff.toml, so riff does not ask for a pull.
    assert!(!stdout.contains("Run git pull"), "{stdout}");
    assert!(stdout.contains("This machine is ready."), "{stdout}");
    assert!(!stdout.contains("CLAUDE.md"), "{stdout}");
    assert!(!stdout.contains("AGENTS.md"), "{stdout}");

    let bin = &machine.bin;
    let json =
        |name: &str| -> serde_json::Value { serde_json::from_str(&machine.read(name)).unwrap() };
    assert_eq!(json("pull.json")["title"], "Set up riff");
    assert_eq!(json("pull.json")["head"], "riff-setup");
    assert_eq!(json("merge.json")["merge_method"], "squash");
    let config = json("config.json");
    assert_eq!(config["branch"], "riff-setup");
    assert!(bin.join("auto_merge").exists());
    assert_eq!(machine.read("milestones"), "Backlog\nWave 1\n");
    let rules = machine.read("ruleset");
    for context in ["checks / lint", "checks / test", "riff/verify"] {
        assert!(
            rules.contains(&format!(r#""context":"{context}""#)),
            "{rules}"
        );
    }
    assert!(machine.read("files").lines().any(|l| l == "riff.toml"));
    let changes = machine.forge_changes();
    assert!(changes.len() >= 8, "{changes:?}");

    // A second run: the checks pass, nothing changes, and nothing is asked.
    let out = machine.init(&machine.repo, &[]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(!stdout.contains("fail  "), "{stdout}");
    assert!(!stdout.contains("CLAUDE.md"), "{stdout}");
    assert!(!stdout.contains("These steps change GitHub"), "{stdout}");
    assert!(!stdout.contains("Do them?"), "{stdout}");
    assert_eq!(machine.forge_changes(), changes);
}

/// 01M5RGG9KJXCKH8C94145WS8SM, 01M5RGGD6YR36EXZEG61XCJVRY: with no `y` (no
/// terminal), the first list has `step` lines for the five facts that a step
/// fixes, and no `fail`. A no does no step, so the end names each check with
/// its fix.
#[tokio::test]
async fn riff_init_without_a_terminal_names_the_steps_and_does_none() {
    let machine = Machine::new();
    machine.add_origin();
    bare(&machine.bin);
    let out = machine.init(&machine.repo, &[]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(failed_lines(&stdout).is_empty(), "{stdout}");
    let steps = step_lines(&stdout);
    assert_eq!(steps.len(), 5, "{stdout}");
    assert_eq!(
        steps[0],
        "  step  riff.toml on main: main of acme/app has no riff.toml, so riff lead start \
         stops. Step: make riff.toml on main with a pull request"
    );
    assert_eq!(
        steps[2],
        "  step  the ruleset of main: no ruleset of main requires the check riff/verify, so a \
         merge does not wait for the verify. Step: make the ruleset riff on main"
    );
    assert!(
        stdout.contains("These steps change GitHub:\n  allow auto-merge and squash merge\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "riff did no step. To do them, run riff init in a terminal and answer y, or run \
             riff init --yes."
        ),
        "{stdout}"
    );
    // No step ran, so no second list. The end names each check, with its fix.
    assert!(
        !stdout.contains("riff checks this machine again:"),
        "{stdout}"
    );
    assert!(stdout.contains("5 checks failed."), "{stdout}");
    let named = named_at_the_end(&stdout);
    assert_eq!(named.len(), 5, "{stdout}");
    assert!(
        named[1].starts_with("  auto-merge and squash merge: allow them: gh api -X PATCH"),
        "{}",
        named[1]
    );
    assert!(machine.forge_changes().is_empty());
    assert_eq!(machine.read("systemctl.log"), "");
}

/// 01M5R1672NF56P15VD0JEX51K4: a person who is not an admin gets the
/// steps named, and riff changes nothing, also with `--yes`.
#[tokio::test]
async fn a_person_who_is_not_an_admin_gets_the_steps_named_and_riff_changes_nothing() {
    let machine = Machine::new();
    machine.add_origin();
    bare(&machine.bin);
    std::fs::write(machine.bin.join("not_admin"), "").unwrap();
    let out = machine.init(&machine.repo, &["--yes"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert_eq!(failed_lines(&stdout).len(), 5, "{stdout}");
    assert!(
        stdout.contains(
            "You are not an admin of acme/app, so riff changes nothing there. Ask an admin to \
             run riff init, or do the fix of each failed check."
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("These steps change GitHub"), "{stdout}");
    assert!(machine.forge_changes().is_empty());
}

/// A clone that is not safe to pull gets the image of the release for the
/// run, and riff says why it ran no `git pull` and to pull (01M5RGGD9AYNC12Z2CG5CDZWNA).
#[tokio::test]
async fn a_clone_that_riff_does_not_pull_gets_the_new_image_and_a_line_to_pull() {
    let machine = Machine::new();
    machine.add_origin();
    bare(&machine.bin);
    std::fs::remove_file(machine.repo.join("riff.toml")).unwrap();
    // On another branch than the default branch of the repository.
    machine.git(&machine.repo, &["symbolic-ref", "HEAD", "refs/heads/dev"]);
    let out = machine.init(&machine.repo, &["--yes"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(
        stdout.contains(
            "riff.toml is on main now. riff ran no git pull in this clone: the clone is on dev, \
             not on main. Run git pull in this clone to get it."
        ),
        "{stdout}"
    );
    let release = riff::session_image::of_release().to_string();
    assert!(
        stdout.contains(&format!("This machine has the session image {release}.")),
        "{stdout}"
    );
}

impl Machine {
    /// `git ARGS` in `dir`, as a person with a name.
    fn git(&self, dir: &Path, args: &[&str]) {
        let status = self
            .env
            .command("git")
            .args([
                "-c",
                "user.name=riff",
                "-c",
                "user.email=riff@example.com",
                "-C",
            ])
            .arg(dir)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    /// A clone of acme/app that lacks only the last commit of `origin`:
    /// the commit with `riff.toml` that the merge of the step makes. The
    /// remote `origin` is a folder of this machine whose path names
    /// `github.com/acme/app`, so riff takes it for the repository on GitHub.
    fn clone_behind_origin(&self) -> PathBuf {
        let root = self.env.path();
        let origin = format!("{}//github.com/acme/app.git", root.join("remote").display());
        let (work, clone) = (root.join("work"), root.join("clone"));
        self.git(root, &["init", "-q", "--bare", "-b", "main", &origin]);
        self.git(root, &["init", "-q", "-b", "main", "work"]);
        self.git(&work, &["remote", "add", "origin", &origin]);
        std::fs::write(work.join("CLAUDE.md"), "rules\n").unwrap();
        self.git(&work, &["add", "."]);
        self.git(&work, &["commit", "-q", "-m", "first"]);
        self.git(&work, &["push", "-q", "origin", "main"]);
        self.git(root, &["clone", "-q", &origin, "clone"]);
        std::fs::write(
            work.join("riff.toml"),
            format!("[session]\nimage = \"{IMAGE}\"\n"),
        )
        .unwrap();
        self.git(&work, &["add", "."]);
        self.git(&work, &["commit", "-q", "-m", "Set up riff"]);
        self.git(&work, &["push", "-q", "origin", "main"]);
        clone
    }
}

/// 01M5RGG9KJXCKH8C94145WS8SM, 01M5RGGD6YR36EXZEG61XCJVRY,
/// 01M5RGGD9AYNC12Z2CG5CDZWNA: `riff init` in a clone of a repository with
/// no setup, that lacks only the commits of `origin`. The first list has
/// `step` lines, and `fail` for no fact that a step fixes. After the steps
/// the clone has `riff.toml` with no `git pull` by hand, the second list
/// shows each check, and the end names the one that still fails.
#[tokio::test]
async fn riff_init_shows_the_steps_pulls_riff_toml_and_names_the_check_that_still_fails() {
    let machine = Machine::new();
    let clone = machine.clone_behind_origin();
    // The machine fails one check that no step fixes: podman.
    std::fs::write(machine.bin.join("no-podman"), "").unwrap();
    bare(&machine.bin);
    assert!(!clone.join("riff.toml").exists());
    let out = machine.init(&clone, &["--yes"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{stdout}\n{}",
        text(&out.stderr)
    );
    let (first, rest) = stdout
        .split_once("riff checks this machine again:\n")
        .expect("a second list");

    // The first list: a `step` for each fact that a step fixes, and a `fail`
    // only for the check that no step fixes.
    let steps: Vec<&str> = step_lines(first)
        .iter()
        .map(|l| l.split_once(": ").unwrap().0)
        .collect();
    assert_eq!(
        steps,
        [
            "  step  the session image",
            "  step  riff.toml on main",
            "  step  auto-merge and squash merge",
            "  step  the ruleset of main",
            "  step  the milestone Backlog",
            "  step  the first wave",
        ],
        "{first}"
    );
    let failed = failed_lines(first);
    assert_eq!(failed.len(), 1, "{first}");
    assert!(failed[0].starts_with("  fail  podman: "), "{}", failed[0]);

    // The steps ran, and riff ran `git pull --ff-only`.
    assert!(
        first.contains(
            "riff.toml is on main now. riff ran git pull --ff-only in this clone, so the clone \
             has it."
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("Run git pull"), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(clone.join("riff.toml")).unwrap(),
        format!("[session]\nimage = \"{IMAGE}\"\n")
    );

    // The second list shows each check; only podman fails; the end names it.
    let (second, end) = rest.split_once("1 check failed.").expect("an end");
    assert!(step_lines(second).is_empty(), "{second}");
    assert_eq!(failed_lines(second).len(), 1, "{second}");
    for line in [
        "  pass  the session image: ghcr.io/como-technologies/riff-session@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "  pass  riff.toml on main: main of acme/app has it",
        "  pass  auto-merge and squash merge: acme/app allows both",
        "  pass  the ruleset of main: it requires the check riff/verify",
    ] {
        assert!(
            second.lines().any(|l| l == line),
            "{line}\nnot in\n{second}"
        );
    }
    assert!(
        end.starts_with(
            " riff installed no daemon and pulled no image. Do each fix, then run riff init \
             again:\n  podman: "
        ),
        "{end}"
    );
    assert_eq!(named_at_the_end(&stdout).len(), 1, "{stdout}");
    assert_eq!(machine.read("systemctl.log"), "");
}

/// A repository that is set up but for its CI: `riff.toml` is on `main`, and
/// no pull request ran a check.
fn no_ci(bin: &Path) {
    fake_forge(
        bin,
        &[
            ("files", "riff.toml\n"),
            ("milestones", "Wave 1\nBacklog\n"),
            ("auto_merge", ""),
        ],
    );
}

/// 01M5RGGD6YR36EXZEG61XCJVRY: the check of the CI is the work of the
/// person. A repository whose pull requests ran no check gets the ruleset
/// with `Gate`, and the check fails again after the steps: the second list
/// shows it, and the end names it with its fix.
#[tokio::test]
async fn the_end_names_the_check_of_the_ci_that_the_steps_cannot_fix() {
    let machine = Machine::new();
    machine.add_origin();
    no_ci(&machine.bin);
    let out = machine.init(&machine.repo, &["--yes"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{stdout}\n{}",
        text(&out.stderr)
    );
    let (first, rest) = stdout
        .split_once("riff checks this machine again:\n")
        .expect("a second list");
    // The ruleset is a step. The check of the CI has none: riff.toml is
    // there, so no pull request is to come.
    assert_eq!(step_lines(first).len(), 1, "{first}");
    assert!(step_lines(first)[0].starts_with("  step  the ruleset of main: "));
    assert_eq!(failed_lines(first).len(), 1, "{first}");
    assert!(
        failed_lines(first)[0].starts_with("  fail  the checks of a pull request: "),
        "{first}"
    );
    assert!(
        first.contains("  done  make the ruleset riff on main: requires Gate and riff/verify"),
        "{first}"
    );
    let (second, end) = rest.split_once("1 check failed.").expect("an end");
    assert_eq!(failed_lines(second).len(), 1, "{second}");
    let named = named_at_the_end(&stdout);
    assert_eq!(named.len(), 1, "{end}");
    assert!(
        named[0].starts_with("  the checks of a pull request: add a workflow that runs on pull_request, with a job named Gate."),
        "{}",
        named[0]
    );
    assert!(
        !stdout.contains("These steps change GitHub:\n  make riff.toml"),
        "{stdout}"
    );
}

#[tokio::test]
async fn riff_init_check_passes_a_repository_that_is_set_up() {
    let machine = Machine::new();
    machine.add_origin();
    set_up(&machine.bin);
    let out = machine.init(&machine.repo, &["--check"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    for line in [
        "  pass  riff.toml on main: main of acme/app has it",
        "  pass  auto-merge and squash merge: acme/app allows both",
        "  pass  the ruleset of main: it requires the check riff/verify",
        "  pass  the milestone Backlog: acme/app has it open",
        "  pass  the first wave: acme/app has the open wave Wave 1",
        "  pass  the checks of a pull request: the full check is Gate",
    ] {
        assert!(
            stdout.lines().any(|l| l == line),
            "{line}\nnot in\n{stdout}"
        );
    }
}

/// The open issues of the fake forge: `(number, title, label, body)`.
fn issues(bin: &Path, open: &[(u64, &str, &str, &str)]) {
    let list: Vec<serde_json::Value> = open
        .iter()
        .map(|(number, title, label, body)| {
            let labels = if label.is_empty() {
                serde_json::json!([])
            } else {
                serde_json::json!([{ "name": label }])
            };
            serde_json::json!({ "number": number, "title": title, "body": body, "labels": labels })
        })
        .collect();
    std::fs::write(bin.join("issues"), serde_json::to_string(&list).unwrap()).unwrap();
}

/// 01M5SMMP7ZW2PZNGK63WWZAC8P: a repository with 3 open issues, one of them
/// with the line `Done when:`, and a tracker with no line. The note names
/// the 2 others, the first run and `--check` print it, and no exit code
/// changes.
#[tokio::test]
async fn riff_init_names_the_open_issues_with_no_done_when_line() {
    let machine = Machine::new();
    machine.add_origin();
    set_up(&machine.bin);
    issues(
        &machine.bin,
        &[
            (11, "Has the line", "", "Text.\n\nDone when: it works."),
            (12, "No line", "", "Text only."),
            (13, "Also no line", "bug", ""),
            (14, "Tracker: the parts", "tracker", "- #11"),
        ],
    );
    let note = "2 open issues of acme/app have no \"Done when:\" line, so the wave starts no \
                session for them: #12, #13. Add the line to each issue. See \"Add a Done when \
                line to an issue\" in the book.";
    for args in [&["--check"][..], &["--yes"]] {
        let out = machine.init(&machine.repo, args).await;
        let stdout = text(&out.stdout);
        assert_eq!(
            stdout.lines().filter(|l| *l == note).count(),
            1,
            "{args:?}\n{note}\nnot once in\n{stdout}"
        );
        assert!(
            !stdout.contains("#11") && !stdout.contains("#14"),
            "{stdout}"
        );
        assert!(
            machine.read("calls").contains("issue list --state open"),
            "{args:?}"
        );
        // The note comes after the list of the checks and changes no exit code.
        assert!(
            stdout.find("  pass  the checks of a pull request").unwrap()
                < stdout.find(note).unwrap(),
            "{stdout}"
        );
    }
    // The book shows the note.
    assert!(
        crate::book::page("development.md").contains(note),
        "the book does not show the note:\n{note}"
    );
    let out = machine.init(&machine.repo, &["--check"]).await;
    assert!(out.status.success(), "{}", text(&out.stdout));
    assert!(machine.forge_changes().is_empty());
}

/// 01M5SMMP7ZW2PZNGK63WWZAC8P: no note when each open issue has the line or
/// is a tracker, and a note about the read when `gh` cannot list the issues.
#[tokio::test]
async fn riff_init_prints_no_note_when_each_open_issue_has_the_line() {
    let machine = Machine::new();
    machine.add_origin();
    set_up(&machine.bin);
    issues(
        &machine.bin,
        &[
            (11, "Has the line", "", "Done when: it works."),
            (14, "Tracker: the parts", "", "- #11"),
        ],
    );
    let out = machine.init(&machine.repo, &["--check"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(!stdout.contains("Done when"), "{stdout}");
    assert!(machine.read("calls").contains("issue list --state open"));

    std::fs::write(machine.bin.join("fail"), "issue list").unwrap();
    let out = machine.init(&machine.repo, &["--check"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    let unread = stdout
        .lines()
        .find(|l| l.starts_with("riff cannot list the open issues of acme/app"))
        .unwrap_or_else(|| panic!("no note in\n{stdout}"));
    assert!(unread.contains("Must have admin rights"), "{unread}");
}

/// The check of the CI has no step: riff names the fix, and does nothing for
/// it. A repository with no `CLAUDE.md` has no line for it (01M5RN89Z749VYS1CRYJQTKAWY).
#[tokio::test]
async fn riff_init_names_the_fix_of_a_missing_ci_check_and_has_no_line_for_claude_md() {
    let machine = Machine::new();
    machine.add_origin();
    set_up(&machine.bin);
    std::fs::write(machine.bin.join("files"), "riff.toml\n").unwrap();
    std::fs::remove_file(machine.bin.join("prior_pulls")).unwrap();
    let out = machine.init(&machine.repo, &["--yes"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let failed = failed_lines(&stdout);
    assert_eq!(failed.len(), 1, "{stdout}");
    assert!(
        failed[0].starts_with("  fail  the checks of a pull request: no pull request of acme/app ran a check named Gate"),
        "{}",
        failed[0]
    );
    assert!(
        failed[0].contains("add a workflow that runs on pull_request"),
        "{}",
        failed[0]
    );
    assert!(!stdout.contains("CLAUDE.md"), "{stdout}");
    assert!(machine.forge_changes().is_empty());
}

#[tokio::test]
async fn riff_init_check_fails_one_check_when_gh_cannot_read_the_repository() {
    let machine = Machine::new();
    machine.add_origin();
    set_up(&machine.bin);
    std::fs::write(machine.bin.join("no_login"), "").unwrap();
    let out = machine.init(&machine.repo, &["--check"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let failed = failed_lines(&stdout);
    assert_eq!(failed.len(), 1, "{stdout}");
    assert!(
        failed[0]
            .starts_with("  fail  the repository on GitHub: riff cannot read the repository: "),
        "{}",
        failed[0]
    );
    assert!(failed[0].contains("gh auth login"), "{}", failed[0]);
    assert!(stdout.contains("1 check failed."), "{stdout}");
}

/// A folder with no remote on GitHub has no check of the repository,
/// and riff calls no `gh`.
#[tokio::test]
async fn riff_init_calls_no_gh_in_a_folder_with_no_remote_on_github() {
    let machine = Machine::new();
    bare(&machine.bin);
    let out = machine.init(&machine.repo, &["--check"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(!stdout.contains("GitHub"), "{stdout}");
    assert_eq!(machine.read("calls"), "");
    // A remote on another host is no remote on GitHub.
    let status = machine
        .env
        .command("git")
        .arg("-C")
        .arg(&machine.repo)
        .args(["remote", "add", "origin", "https://gitlab.com/acme/app.git"])
        .status()
        .unwrap();
    assert!(status.success());
    let out = machine.init(&machine.repo, &["--check"]).await;
    assert!(out.status.success(), "{}", text(&out.stdout));
    assert_eq!(machine.read("calls"), "");
}

#[test]
fn start_a_riff_tells_how_to_get_ready_and_how_to_check_only() {
    let commands = commands_of("start-a-riff.md");
    let init: Vec<String> = commands
        .into_iter()
        .filter(|c| c.starts_with("riff init"))
        .collect();
    assert_eq!(
        init,
        [
            "riff init",
            "riff init",
            "riff init --yes",
            "riff init --check",
            // "Add another repository to the riff": set it up, check in the container of the
            // lead (01M5SMJMR5KS1Z1Z7CEPH9JVBB), check, then pull the image.
            "riff init",
            "riff init --check",
            "riff init --check",
            "riff init"
        ]
    );
    each_is_real(&init);
}

/// The `text` blocks of the part `heading` of the page `name`, each as its lines.
fn text_blocks(name: &str, heading: &str) -> Vec<Vec<String>> {
    let (mut blocks, mut open) = (Vec::new(), None::<Vec<String>>);
    for line in crate::book::part(name, heading).lines() {
        if line.trim() == "```text" {
            open = Some(Vec::new());
        } else if line.trim_start().starts_with("```") {
            blocks.extend(open.take());
        } else if let Some(block) = &mut open {
            block.push(line.to_owned());
        }
    }
    blocks
}

/// Each line of the block `book` is a line of `real`, in the same order. A
/// line `  ...` stands for lines that the book leaves out, and the answer
/// to the question is not in the output of `--yes`.
fn assert_is_output(book: &[String], real: &str) {
    let mut lines = real.lines();
    for line in book {
        if line == "  ..." || line == "Do them? [y/N] y" {
            continue;
        }
        assert!(
            lines.by_ref().any(|l| l == line),
            "{line}\nis not a line of the output, or it comes too early:\n{real}"
        );
    }
}

/// 01M5RGG9KJXCKH8C94145WS8SM, 01M5RGGD6YR36EXZEG61XCJVRY,
/// 01M5RGGD9AYNC12Z2CG5CDZWNA: "Add another repository to the riff" shows
/// the output of a first run, and of a run that ends with a failed check.
/// Each line is a line of the real output.
#[tokio::test]
async fn the_book_shows_the_output_of_a_first_run_and_the_end_of_a_failed_run() {
    let blocks = text_blocks("start-a-riff.md", "Add another repository to the riff");
    // The fourth block is the line of `riff image --check` (start_a_riff.rs).
    assert_eq!(blocks.len(), 4, "{blocks:#?}");

    // The run in the container of the lead (01M5SMJMR5KS1Z1Z7CEPH9JVBB).
    let machine = Machine::new();
    machine.add_origin();
    set_up(&machine.bin);
    let out = machine.container_init(&machine.repo, &["--check"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_is_output(&blocks[0], &text(&out.stdout));

    let machine = Machine::new();
    let clone = machine.clone_behind_origin();
    bare(&machine.bin);
    std::fs::write(machine.bin.join("no-image"), "").unwrap();
    let out = machine.init(&clone, &["--yes"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let real = text(&out.stdout).replace(clone.to_str().unwrap(), "/home/mike/app");
    assert_is_output(&blocks[1], &real);

    let machine = Machine::new();
    machine.add_origin();
    no_ci(&machine.bin);
    let out = machine.init(&machine.repo, &["--yes"]).await;
    assert_eq!(out.status.code(), Some(1));
    assert_is_output(&blocks[2], &text(&out.stdout));
}
