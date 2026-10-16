//! `riff init`, the first run of riff 3.0 on a machine (#721,
//! 01M4ZJN6R68YBD3XMM725ZRJ7W).
//!
//! The facts of the machine are fakes: `RIFF_ETC` gives `subuid` and
//! `subgid`, and `RIFF_CGROUP` gives the controllers of cgroup v2. A
//! fake `podman` logs each call. The fake `systemctl` of `run.rs` starts
//! the `riff daemon` of this build, as systemd does. The repository is a
//! real git repository with a `riff.toml`.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
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

/// The output of `cmd`, away from the runtime of the test.
async fn output(mut cmd: std::process::Command) -> Output {
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
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
    repo: PathBuf,
    uid: u32,
}

impl Machine {
    /// A machine with each part.
    fn new() -> Machine {
        let env = Isolated::new();
        let root = env.path().to_owned();
        let (bin, etc, cgroup, repo) = (
            root.join("bin"),
            root.join("etc"),
            root.join("cgroup"),
            root.join("repo"),
        );
        for dir in [&bin, &etc, &cgroup, &repo] {
            std::fs::create_dir_all(dir).unwrap();
        }
        fake_podman(&bin);
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
        let mut cmd = self.env.riff();
        cmd.arg("init")
            .args(args)
            .current_dir(dir)
            .env_remove("RIFF_HOME")
            .env(
                "PATH",
                format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("FAKE_RIFF", self.env.riff_path())
            .env("RIFF_ETC", &self.etc)
            .env("RIFF_CGROUP", &self.cgroup);
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

    /// The rules that `riff init` writes into the user settings.
    fn user_rules(&self) -> riff::permissions::Rules {
        riff::permissions::user_rules(Some(&self.env.home()))
    }
}

#[tokio::test]
async fn riff_init_installs_the_daemon_and_the_daemon_pulls_the_image() {
    let machine = Machine::new();
    std::fs::write(machine.bin.join("no-image"), "").unwrap();
    machine.write_settings(r#"{"model": "opus"}"#);
    // --yes adds the permission rules with no question
    // (01M5084K09Z2S7N57315CX2R0F).
    let out = machine.init(&machine.repo, &["--yes"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    let uid = machine.uid;
    let settings = machine.settings();
    let rules = machine.user_rules();
    for line in [
        "riff checks this machine for riff 3.0:".to_owned(),
        "  pass  podman: podman version 5.7.0".to_owned(),
        format!("  pass  cgroup v2: user@{uid}.service has cpu and memory"),
        format!("  pass  the session image: {IMAGE}"),
        format!(
            "  fail  the rules of Claude Code: {} lacks {} permission rules of riff. Fix: \
             answer y to the question of riff init, or run riff init --yes",
            settings.display(),
            rules.len()
        ),
        "  allow Bash(gh issue list *)".to_owned(),
        format!(
            "riff added {} permission rules to {}. Start Claude Code again to use them.",
            rules.len(),
            settings.display()
        ),
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
        machine.read("systemctl.log"),
        "systemctl --user daemon-reload\n\
         systemctl --user enable riff.service\n\
         systemctl --user restart riff.service\n"
    );
    let pulls = |log: &str| log.lines().filter(|l| l.starts_with("podman pull")).count();
    let log = machine.read("podman.log");
    assert!(log.contains(&format!("podman pull {IMAGE}\n")), "{log}");
    assert_eq!(pulls(&log), 1, "{log}");
    // The user settings have each rule, and keep their other key.
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(value["model"], "opus");
    assert_eq!(
        value["permissions"]["allow"],
        serde_json::json!(rules.allow),
        "{value}"
    );

    // A second run keeps the daemon and the settings, and pulls nothing.
    // It asks nothing: the rules are there.
    let before = std::fs::read_to_string(&settings).unwrap();
    let out = machine.init(&machine.repo, &[]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    assert!(
        stdout.contains(&format!(
            "  pass  the rules of Claude Code: {} has each permission rule of riff\n",
            settings.display()
        )),
        "{stdout}"
    );
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), before);
    assert!(
        stdout.contains("The riff daemon of this riff runs already: pid "),
        "{stdout}"
    );
    assert!(stdout.contains("riff init keeps it."), "{stdout}");
    // No second install and no second restart.
    assert_eq!(machine.read("systemctl.log").lines().count(), 3);
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
        "  fail  /etc/subuid: 0 IDs for ",
        "  fail  /etc/subgid: riff cannot read /etc/subgid. Fix: ",
        "  fail  cgroup v2: this machine has no cgroup v2. Fix: ",
        "  fail  the session image: ",
        "  fail  the rules of Claude Code: ",
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
    // No terminal: riff asks nothing about the rules, and writes nothing
    // (01M5084K09Z2S7N57315CX2R0F).
    assert!(
        stdout.contains(
            "riff init asks its question only in a terminal. To add the rules, run: riff init \
             --yes\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "riff changed nothing in {}.\n",
            machine.settings().display()
        )),
        "{stdout}"
    );
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

#[tokio::test]
async fn riff_init_check_changes_nothing() {
    let machine = Machine::new();
    let rules = machine.user_rules();
    machine.write_settings(
        &serde_json::json!({ "permissions": { "allow": rules.allow } }).to_string(),
    );
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
    let rules = machine.user_rules();
    machine.write_settings(
        &serde_json::json!({ "permissions": { "allow": rules.allow } }).to_string(),
    );
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

/// 01M5084K09Z2S7N57315CX2R0F: `riff init --check` names each missing
/// rule, asks nothing and writes nothing.
#[tokio::test]
async fn riff_init_check_names_each_missing_rule() {
    let machine = Machine::new();
    let text_before = r#"{"permissions": {"allow": ["Bash(riff *)"]}}"#;
    machine.write_settings(text_before);
    let out = machine.init(&machine.repo, &["--check"]).await;
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let rules = machine.user_rules();
    for rule in &rules.allow {
        let line = format!("\n  allow {rule}\n");
        assert_eq!(
            stdout.contains(&line),
            rule != "Bash(riff *)",
            "{rule}\n{stdout}"
        );
    }
    assert!(!stdout.contains("[y/N]"), "{stdout}");
    assert!(!stdout.contains("riff changed nothing in"), "{stdout}");
    assert!(stdout.contains("1 check failed."), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(machine.settings()).unwrap(),
        text_before
    );
    assert!(!machine.unit().exists());
}

#[test]
fn start_a_riff_tells_how_to_get_ready_and_how_to_check_only() {
    let commands = commands_of("start-a-riff.md");
    let init: Vec<String> = commands
        .into_iter()
        .filter(|c| c.starts_with("riff init"))
        .collect();
    assert_eq!(init, ["riff init", "riff init --yes", "riff init --check"]);
    each_is_real(&init);
}

/// "Get ready for riff 3.0" names each rule that `riff init` writes
/// (01M5084K09Z2S7N57315CX2R0F).
#[test]
fn start_a_riff_names_each_rule_that_riff_init_writes() {
    let book = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/start-a-riff.md"),
    )
    .unwrap();
    let start = book
        .find("### Let riff set the permissions of Claude Code")
        .unwrap();
    let part = &book[start..];
    let part = &part[..part[4..].find("\n### ").map_or(part.len(), |end| end + 4)];
    let rules = riff::permissions::user_rules(None);
    assert!(!rules.allow.is_empty());
    for rule in &rules.allow {
        assert!(
            part.contains(&format!("`{rule}`")),
            "{rule} is not in\n{part}"
        );
    }
    assert!(part.contains("the same rule with the full path of your home"));
    // 01M509TJNREVNYR9MNN4P9P4RA: the page names no rule of the API of
    // the forge, and the place of the `*` of a rule.
    assert!(!part.contains("gh api"), "{part}");
    assert!(
        part.contains("Its only `*` is at its\nend, after a space."),
        "{part}"
    );
}
