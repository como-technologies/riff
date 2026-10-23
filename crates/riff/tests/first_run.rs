//! `riff init`, the first run of riff 3.0 on a machine (#721,
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
        "riff checks this machine for riff 3.0:".to_owned(),
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
        "This machine is ready for riff 3.0. Run one item in a container: riff run ITEM".to_owned(),
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
            "6 checks failed. riff installed nothing and pulled nothing. Do each fix, then run \
             riff init again."
        ),
        "{stdout}"
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
        "in step  the plugin of riff 3: it is not here".to_owned(),
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

/// 01M5JFF7TAVA87TMJXK25HDJ19: `riff init --check` names the plugin of
/// riff 3 and keeps it. `riff init` removes the folder and its keys in
/// the user settings, also when a check fails, and keeps each other key.
#[tokio::test]
async fn riff_init_removes_the_plugin_of_riff_3() {
    let machine = Machine::new();
    let plugin = machine.env.home().join(".local/share/riff/claude-plugin");
    std::fs::create_dir_all(plugin.join("riff/hooks")).unwrap();
    machine.write_settings(
        r#"{"enabledPlugins":{"riff@riff":true},"extraKnownMarketplaces":{"riff":{}},"statusLine":{"type":"command","command":"riff statusline"},"theme":"auto"}"#,
    );
    let named = format!(
        "The plugin of riff 3 is still here. riff 4 has no plugin, so each of its hooks fails in \
         Claude Code:\n  {}\n  {}: enabledPlugins.riff@riff, extraKnownMarketplaces.riff, \
         statusLine\n",
        plugin.display(),
        machine.settings().display()
    );

    let out = machine.init(&machine.repo, &["--check"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(stdout.contains(&named), "{named}\nnot in\n{stdout}");
    assert!(
        stdout.contains(
            "riff removed nothing: --check. To remove the plugin of riff 3, run riff init.\n"
        ),
        "{stdout}"
    );
    assert!(plugin.exists());

    // podman fails, so riff stops before the daemon.
    std::fs::write(machine.bin.join("no-podman"), "").unwrap();
    let out = machine.init(&machine.repo, &[]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains(&named), "{stdout}");
    assert!(
        stdout.contains("riff removed the plugin of riff 3.\n"),
        "{stdout}"
    );
    assert!(!plugin.exists());
    assert_eq!(
        std::fs::read_to_string(machine.settings()).unwrap(),
        "{\n  \"theme\": \"auto\"\n}\n"
    );

    let out = machine.init(&machine.repo, &[]).await;
    assert!(
        !text(&out.stdout).contains("plugin of riff 3"),
        "{}",
        text(&out.stdout)
    );
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
            "riff init",
            "riff init --yes",
            "riff init --check"
        ]
    );
    each_is_real(&init);
}
