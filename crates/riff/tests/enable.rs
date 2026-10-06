//! riff is off in a session until a person turns it on for the
//! repository (01M3XY2SHGXQR9NVXF7QJBN09T): `riff enable`, `riff
//! disable`, the scope question of `riff connect claude`, and each entry
//! of the plugin in a directory where riff is off.

use isolated::Isolated;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::ServiceExt;
use serde_json::{Value, json};

mod book;

const ID: &str = "a6cf2205-d54a-4c1e-9b1f-2e3d4c5b6a7f";

/// A riff that does not answer.
const NO_SERVER: &str = "http://127.0.0.1:1";

/// One machine of a test: a home of its own, a fake `claude`, and a
/// `git` that logs each call.
struct Machine {
    env: Isolated,
}

impl Machine {
    fn new() -> Machine {
        Machine::with(Isolated::new())
    }

    fn with(env: Isolated) -> Machine {
        let machine = Machine { env };
        let script = |name: &str, body: &str| {
            let path = machine.bin().join(name);
            std::fs::create_dir_all(machine.bin()).unwrap();
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        let log = machine.env.path().join("claude.log");
        script(
            "claude",
            &format!(
                "echo \"$*\" >> {}\n[ \"$1\" = mcp ] && exit 1\nexit 0",
                log.display()
            ),
        );
        // The real git, with a log of each call.
        let git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|dir| dir.join("git"))
            .find(|git| git.is_file())
            .expect("git on the PATH");
        let log = machine.git_log();
        script(
            "git",
            &format!(
                "echo \"$*\" >> {}\nexec {} \"$@\"",
                log.display(),
                git.display()
            ),
        );
        machine
    }

    fn bin(&self) -> PathBuf {
        self.env.path().join("bin")
    }

    fn git_log(&self) -> PathBuf {
        self.env.path().join("git.log")
    }

    /// The calls of `git` that riff made.
    fn git_calls(&self) -> String {
        std::fs::read_to_string(self.git_log()).unwrap_or_default()
    }

    /// The calls of `claude` that riff made.
    fn claude_calls(&self) -> String {
        std::fs::read_to_string(self.env.path().join("claude.log")).unwrap_or_default()
    }

    /// The user settings of Claude Code.
    fn user(&self) -> PathBuf {
        self.env.home().join(".claude/settings.json")
    }

    /// The settings of riff.
    fn riff_settings(&self) -> String {
        std::fs::read_to_string(self.env.riff_home().join("config.toml")).unwrap_or_default()
    }

    /// A git repository with a GitHub origin. The `git` of the machine
    /// does not make it, so its log stays empty.
    fn repo(&self, name: &str) -> PathBuf {
        let dir = self.plain(name);
        let origin = format!("https://github.com/acme/{name}.git");
        for args in [
            &["init", "-q"][..],
            &["remote", "add", "origin", &origin][..],
        ] {
            let git = self
                .env
                .command("git")
                .args(args)
                .current_dir(&dir)
                .status();
            assert!(git.unwrap().success());
        }
        dir
    }

    /// A directory that is in no repository.
    fn plain(&self, name: &str) -> PathBuf {
        let dir = self.env.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    /// `riff ARGS` in `dir`, with no `RIFF_ON`: the settings decide.
    fn riff(&self, dir: &Path, args: &[&str]) -> Command {
        let mut cmd = self.env.riff();
        let path = std::env::var("PATH").unwrap();
        cmd.args(args)
            .current_dir(dir)
            .env("PATH", format!("{}:{path}", self.bin().display()))
            .env("RIFF_SERVER", NO_SERVER)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env_remove("RIFF_ON")
            .stdin(Stdio::null());
        cmd
    }

    /// The output of `riff ARGS` in `dir`. The command must succeed.
    fn run(&self, dir: &Path, args: &[&str]) -> String {
        let out = self.riff(dir, args).output().unwrap();
        assert!(out.status.success(), "riff {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// `riff connect claude ARGS` in `dir`, with no terminal.
    fn connect(&self, dir: &Path, args: &[&str]) -> String {
        let claude = self.bin().join("claude");
        let mut all = vec!["connect", "claude", "--claude", claude.to_str().unwrap()];
        all.extend(args);
        self.run(dir, &all)
    }
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn local(repo: &Path) -> PathBuf {
    repo.join(".claude/settings.local.json")
}

fn shared(repo: &Path) -> PathBuf {
    repo.join(".claude/settings.json")
}

/// 01M3XY2SNXQJRSH5QX82AFVM2S.
#[test]
fn connect_turns_riff_on_nowhere_and_names_riff_enable() {
    let machine = Machine::new();
    let repo = machine.repo("app");
    let out = machine.connect(&repo, &[]);
    assert!(
        out.ends_with(
            "riff is installed but off. To turn it on in a repository: cd REPO && riff enable\n"
        ),
        "{out}"
    );
    assert_eq!(riff::enable::entry_at(&machine.user()), None);
    assert!(!local(&repo).exists() && !shared(&repo).exists());
    let calls = machine.claude_calls();
    assert!(calls.contains("plugin marketplace add"), "{calls}");
    assert!(!calls.contains("install"), "{calls}");
    assert_eq!(machine.riff_settings(), "", "nobody answered");
}

/// `riff connect claude` in a pseudo-terminal, with `keys` typed. It
/// gives what the terminal showed.
fn connect_in_a_terminal(machine: &Machine, dir: &Path, keys: &str) -> String {
    let claude = machine.bin().join("claude");
    let args = ["connect", "claude", "--claude", claude.to_str().unwrap()];
    in_a_terminal(machine.riff(dir, &args), keys)
}

/// Runs `cmd` in a pseudo-terminal, with `keys` typed. It gives what
/// the terminal showed. The command must succeed.
fn in_a_terminal(mut cmd: Command, keys: &str) -> String {
    let pty = nix::pty::openpty(None, None).unwrap();
    let slave = std::fs::File::from(pty.slave);
    let mut child = cmd
        .stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave)
        .spawn()
        .unwrap();
    // The spawn made copies: drop the ones of this process, so that the
    // read ends when the child exits.
    drop(cmd);
    let mut master = std::fs::File::from(pty.master);
    master.write_all(keys.as_bytes()).unwrap();
    let mut reader = master.try_clone().unwrap();
    let shown = std::thread::spawn(move || {
        let mut all = Vec::new();
        let mut buf = [0u8; 4096];
        // The read fails when the last process of the terminal ends.
        while let Ok(n @ 1..) = reader.read(&mut buf) {
            all.extend_from_slice(&buf[..n]);
        }
        String::from_utf8_lossy(&all).replace("\r\n", "\n")
    });
    assert!(child.wait().unwrap().success());
    drop(child);
    drop(master);
    shown.join().unwrap()
}

/// 01M3XY2SNXQJRSH5QX82AFVM2S: the question, and Enter for "only in
/// this repository". The second Enter answers the question about the
/// update.
#[test]
fn connect_in_a_terminal_asks_the_scope_and_enter_is_this_repository() {
    let machine = Machine::new();
    let repo = machine.repo("app");
    let shown = connect_in_a_terminal(&machine, &repo, "\n\n");
    assert!(shown.contains("Where do you want riff on?"), "{shown}");
    assert!(
        shown.contains("1) Only in this repository (default)"),
        "{shown}"
    );
    assert!(shown.contains("riff is on in this repository"), "{shown}");
    assert_eq!(riff::enable::entry_at(&local(&repo)), Some(true));
    assert_eq!(riff::enable::entry_at(&machine.user()), None);
    assert!(machine.riff_settings().contains("scope = \"repo\""));

    // The answer is kept: no second question.
    let shown = connect_in_a_terminal(&machine, &repo, "");
    assert!(!shown.contains("Where do you want riff on?"), "{shown}");
}

#[test]
fn the_answer_2_in_a_terminal_is_each_repository() {
    let machine = Machine::new();
    let repo = machine.repo("app");
    let shown = connect_in_a_terminal(&machine, &repo, "2\n\n");
    assert!(
        shown.contains("To turn it off: riff disable --global"),
        "{shown}"
    );
    assert_eq!(riff::enable::entry_at(&machine.user()), Some(true));
    assert!(!local(&repo).exists());
}

#[test]
fn connect_with_scope_global_asks_nothing() {
    let machine = Machine::new();
    let dir = machine.plain("downloads");
    let out = machine.connect(&dir, &["--scope", "global"]);
    assert!(!out.contains("Where do you want riff on?"), "{out}");
    assert!(
        out.ends_with("To turn it off: riff disable --global\n"),
        "{out}"
    );
    assert_eq!(riff::enable::entry_at(&machine.user()), Some(true));
    assert!(machine.riff_settings().contains("scope = \"global\""));
}

#[test]
fn connect_with_scope_repo_turns_riff_on_only_here() {
    let machine = Machine::new();
    let (repo, other) = (machine.repo("app"), machine.repo("other"));
    let out = machine.connect(&repo, &["--scope", "repo"]);
    assert!(out.contains("riff is on in this repository"), "{out}");
    assert!(machine.run(&repo, &["server"]).contains("riff on"));
    assert!(machine.run(&other, &["server"]).contains("riff off"));
    // Outside a repository, the answer turns riff on nowhere.
    let out = machine.connect(&machine.plain("downloads"), &["--scope", "repo"]);
    assert!(
        out.contains("This directory is not in a git repository."),
        "{out}"
    );
}

/// 01M3XY2SR3VJZAKEPC6CBCS292: with no terminal, as in `riff update`.
#[test]
fn connect_with_no_terminal_keeps_a_choice_and_never_turns_the_global_scope_on() {
    // A new install stays off, also after a second run.
    let machine = Machine::new();
    let repo = machine.repo("app");
    for _ in 0..2 {
        machine.connect(&repo, &[]);
        assert_eq!(riff::enable::entry_at(&machine.user()), None);
        assert!(!local(&repo).exists());
    }

    // The person chose each repository: an update keeps it.
    machine.run(&repo, &["enable", "--global"]);
    let out = machine.connect(&repo, &[]);
    assert_eq!(riff::enable::entry_at(&machine.user()), Some(true));
    assert!(!out.contains("Now it is on only"), "{out}");

    // The person chose this repository: an update keeps it.
    let machine = Machine::new();
    let repo = machine.repo("app");
    machine.connect(&repo, &["--scope", "repo"]);
    machine.connect(&repo, &[]);
    assert_eq!(riff::enable::entry_at(&local(&repo)), Some(true));
    assert_eq!(riff::enable::entry_at(&machine.user()), None);
}

/// An old install: the user settings turn riff on, riff has no answer,
/// and two projects are in the state file of Claude Code. One of them
/// has the riff permission rules.
fn old_install(machine: &Machine) -> (PathBuf, PathBuf) {
    let (used, other) = (machine.repo("used"), machine.repo("other"));
    riff::enable::set(&machine.user(), Some(true)).unwrap();
    std::fs::create_dir_all(used.join(".claude")).unwrap();
    std::fs::write(
        shared(&used),
        r#"{"permissions": {"allow": ["mcp__plugin_riff_riff"]}}"#,
    )
    .unwrap();
    let projects = json!({"projects": {used.to_str().unwrap(): {}, other.to_str().unwrap(): {}}});
    std::fs::write(
        machine.env.home().join(".claude.json"),
        projects.to_string(),
    )
    .unwrap();
    (used, other)
}

/// 01M3XY2SR3VJZAKEPC6CBCS292: an old install, of a release up to
/// v0.8.0, has the entry in the user settings, and no answer. Its
/// choice is each repository. riff keeps it: riff stays on in each
/// repository, and riff records the answer `global`. `--scope repo`
/// then takes the entry out, and riff names the repositories that used
/// riff.
#[test]
fn an_old_install_stays_on_and_scope_repo_takes_the_entry_out() {
    let machine = Machine::new();
    let (used, other) = old_install(&machine);

    let lines = [
        "riff stays on in each repository on this machine, as before this release. To change \
         it: riff disable --global\n",
        "riff is on in each repository on this machine. Start a new Claude Code session in a \
         repository to use it. To turn it off: riff disable --global\n",
    ];
    for last in lines {
        let out = machine.connect(&other, &[]);
        assert_eq!(riff::enable::entry_at(&machine.user()), Some(true));
        assert!(!out.contains("Now it is on only"), "{out}");
        assert!(out.ends_with(last), "{out}");
        let settings = machine.riff_settings();
        assert!(settings.contains("scope = \"global\""), "{settings}");
        assert!(!local(&other).exists() && !local(&used).exists());
    }

    let out = machine.connect(&other, &["--scope", "repo"]);
    assert_eq!(riff::enable::entry_at(&machine.user()), None);
    assert_eq!(riff::enable::entry_at(&local(&other)), Some(true));
    assert!(
        out.contains("Now it is on only where you turn it on."),
        "{out}"
    );
    assert!(
        out.contains(&format!("\n  cd {} && riff enable\n", used.display())),
        "{out}"
    );
    assert!(!out.contains(&format!("cd {} ", other.display())), "{out}");
    // The next update changes nothing, and names nothing.
    let out = machine.connect(&other, &[]);
    assert_eq!(riff::enable::entry_at(&machine.user()), None);
    assert!(!out.contains("Now it is on only"), "{out}");
}

/// 01M3XY2SR3VJZAKEPC6CBCS292: the first update of a machine from
/// v0.8.0 runs the old `riff update`. It gives the new
/// `riff connect claude` the terminal of the person, in the home
/// directory. On an old install the command asks nothing there: Enter
/// changes nothing, the entry stays, and `riff server` in a repository
/// shows `riff on`.
#[test]
fn connect_in_a_terminal_asks_nothing_on_an_old_install() {
    let machine = Machine::new();
    let (used, _) = old_install(&machine);
    let before: Value = read(&machine.user());
    let home = machine.env.home();

    let shown = connect_in_a_terminal(&machine, &home, "\n\n");
    assert!(!shown.contains("Where do you want riff on?"), "{shown}");
    assert!(!shown.contains("Your choice"), "{shown}");
    assert!(!shown.contains("Now it is on only"), "{shown}");
    assert!(
        shown.contains("riff stays on in each repository on this machine"),
        "{shown}"
    );
    assert!(shown.contains("riff disable --global"), "{shown}");
    let after: Value = read(&machine.user());
    assert_eq!(after["enabledPlugins"], before["enabledPlugins"]);
    assert_eq!(riff::enable::entry_at(&machine.user()), Some(true));
    let settings = machine.riff_settings();
    assert!(settings.contains("scope = \"global\""), "{settings}");
    let server = machine.run(&used, &["server"]);
    assert!(
        server.contains(&format!("riff on ({})", machine.user().display())),
        "{server}"
    );
    // A later run in a terminal asks nothing too.
    let shown = connect_in_a_terminal(&machine, &used, "\n\n");
    assert!(!shown.contains("Where do you want riff on?"), "{shown}");
    assert_eq!(riff::enable::entry_at(&machine.user()), Some(true));
    assert!(!local(&used).exists());
}

/// 01M3XY2SR3VJZAKEPC6CBCS292: `riff update` asks nothing, also when a
/// person runs it in a terminal: the connect of the update has no
/// terminal. A new install stays off. An old install stays on in each
/// repository. The update runs a fake `cargo`, the `riff` of this build
/// and a fake `riff-server`.
#[test]
fn riff_update_in_a_terminal_asks_nothing_and_keeps_the_choice() {
    for old in [false, true] {
        let machine = Machine::new();
        let script = |name: &str| {
            let path = machine.bin().join(name);
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let cargo = script("cargo");
        script("riff-server");
        std::os::unix::fs::symlink(machine.env.riff_path(), machine.bin().join("riff")).unwrap();
        let repo = match old {
            true => old_install(&machine).0,
            false => machine.repo("app"),
        };
        let user_before = std::fs::read_to_string(machine.user()).ok();

        let claude = machine.bin().join("claude");
        let mut update = machine.riff(&repo, &["update", "--tag", "v0.8.0"]);
        update
            .arg("--cargo")
            .arg(&cargo)
            .arg("--claude")
            .arg(&claude);
        // A person who types an answer gets no question for it.
        let shown = in_a_terminal(update, "2\n2\n");

        assert!(!shown.contains("Where do you want riff on?"), "{shown}");
        assert!(!shown.contains("Your choice"), "{shown}");
        // A new install has no answer. An old install keeps its choice.
        let settings = machine.riff_settings();
        match old {
            true => assert!(settings.contains("scope = \"global\""), "{settings}"),
            false => assert_eq!(settings, "", "nobody answered: {shown}"),
        }
        assert!(!local(&repo).exists(), "{shown}");
        assert_eq!(
            riff::enable::entry_at(&machine.user()),
            old.then_some(true),
            "{shown}"
        );
        let last = match old {
            true => "riff stays on in each repository on this machine, as before this release.",
            false => {
                "riff is installed but off. To turn it on in a repository: cd REPO && riff enable"
            }
        };
        assert!(shown.contains(last), "{shown}");
        assert!(!shown.contains("Now it is on only"), "{shown}");
        if let Some(before) = user_before {
            // The update added only the status line to the user settings.
            let after: Value = read(&machine.user());
            let before: Value = serde_json::from_str(&before).unwrap();
            assert_eq!(after["enabledPlugins"], before["enabledPlugins"]);
        }
        assert!(machine.claude_calls().contains("plugin marketplace add"));
    }
}

/// 01M3XY2SKQ27K3TE4NV28FHTVV: `riff enable` and `riff disable` change
/// only the entry of riff, as text. Each other byte of the file stays:
/// the indent, the order and the lines.
#[test]
fn enable_and_disable_keep_each_other_byte_of_the_settings_file() {
    let machine = Machine::new();
    let repo = machine.repo("app");
    let text = "{\n    \"permissions\": {\"allow\": [\"Bash(ls)\", \"Bash(cat:*)\"]},\n    \"model\": \"opus\"\n}\n";
    std::fs::create_dir_all(repo.join(".claude")).unwrap();
    for (flag, file) in [("--local", local(&repo)), ("--shared", shared(&repo))] {
        std::fs::write(&file, text).unwrap();
        machine.run(&repo, &["enable", flag]);
        let on = std::fs::read_to_string(&file).unwrap();
        let entry = ",\n    \"enabledPlugins\": {\n        \"riff@riff\": true\n    }";
        assert_eq!(on, text.replace("\"opus\"", &format!("\"opus\"{entry}")));
        machine.run(&repo, &["disable", flag]);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text, "{flag}");
    }
    // The user settings: `riff enable --global`, then a no for this
    // repository, then both back.
    let user = "{\"model\": \"opus\", \"enabledPlugins\": {\"a@b\": true}}";
    std::fs::create_dir_all(machine.user().parent().unwrap()).unwrap();
    std::fs::write(machine.user(), user).unwrap();
    std::fs::write(local(&repo), text).unwrap();
    machine.run(&repo, &["enable", "--global"]);
    assert_eq!(
        std::fs::read_to_string(machine.user()).unwrap(),
        user.replace("true}", "true, \"riff@riff\": true}")
    );
    machine.run(&repo, &["disable"]);
    let no = std::fs::read_to_string(local(&repo)).unwrap();
    assert!(no.contains("\"riff@riff\": false"), "{no}");
    machine.run(&repo, &["disable", "--global"]);
    assert_eq!(std::fs::read_to_string(machine.user()).unwrap(), user);
    machine.run(&repo, &["disable"]);
    assert_eq!(std::fs::read_to_string(local(&repo)).unwrap(), text);
}

/// 01M3YCGKGP3VC93S8FA1G4K3QK: riff writes through a symbolic link,
/// and the output names the real path of the file that it wrote.
#[test]
fn enable_writes_through_a_symbolic_link_and_names_the_real_path() {
    let machine = Machine::new();
    let repo = machine.repo("app");
    let dotfiles = machine.plain("dotfiles");
    let target = dotfiles.join("riff-settings.json");
    std::fs::write(&target, "{\"keep\": 1}\n").unwrap();
    std::fs::create_dir_all(repo.join(".claude")).unwrap();
    std::os::unix::fs::symlink(&target, local(&repo)).unwrap();

    let out = machine.run(&repo, &["enable"]);
    assert!(
        out.starts_with(&format!("Turned riff on in {}.", target.display())),
        "{out}"
    );
    assert!(!out.contains("settings.local.json"), "{out}");
    assert!(
        local(&repo)
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(riff::enable::entry_at(&target), Some(true));
    let out = machine.run(&repo, &["disable"]);
    assert!(out.contains(&target.display().to_string()), "{out}");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"keep\": 1}\n");
}

/// 01M3YCGKGP3VC93S8FA1G4K3QK: the main clone of a linked worktree
/// comes from git. A tree whose `.git` file names a main clone that git
/// does not confirm gets no write: `riff enable` refuses and says why.
/// In a worktree that git made, `riff enable` writes the local settings
/// of the main clone.
#[test]
fn enable_in_a_worktree_asks_git_for_the_main_clone() {
    let machine = Machine::new();
    // A victim repository, and a tree from an archive that names it.
    let victim = machine.repo("victim");
    std::fs::create_dir_all(victim.join(".git/worktrees/x")).unwrap();
    let tree = machine.plain("archive");
    let gitdir = victim.join(".git/worktrees/x");
    std::fs::write(tree.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
    for command in ["enable", "disable"] {
        let out = machine.riff(&tree, &[command]).output().unwrap();
        assert!(!out.status.success(), "{out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("git does not confirm"), "{stderr}");
        assert!(stderr.contains(&victim.display().to_string()), "{stderr}");
    }
    assert!(!victim.join(".claude").exists(), "riff wrote in the victim");

    // A worktree that git made.
    let main = machine.repo("app");
    let wt = main.join(".claude/worktrees/issue-12");
    for args in [
        &["commit", "-q", "--allow-empty", "-m", "x"][..],
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "issue-12",
            wt.to_str().unwrap(),
        ][..],
    ] {
        let git = machine
            .env
            .command("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(&main)
            .output()
            .unwrap();
        assert!(git.status.success(), "git {args:?}: {git:?}");
    }
    // A second tree whose `.git` file names the entry of that worktree:
    // git gives the common directory of `app`, but `app` names the
    // worktree, not this tree.
    let copy = machine.plain("copy");
    let entry = main.join(".git/worktrees/issue-12");
    assert!(entry.is_dir(), "git made the entry");
    std::fs::write(copy.join(".git"), format!("gitdir: {}\n", entry.display())).unwrap();
    let out = machine.riff(&copy, &["enable"]).output().unwrap();
    assert!(!out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("git does not confirm"), "{stderr}");
    assert!(!local(&main).exists(), "riff wrote in the main clone");

    let out = machine.run(&wt, &["enable"]);
    assert!(
        out.starts_with(&format!("Turned riff on in {}.", local(&main).display())),
        "{out}"
    );
    assert_eq!(riff::enable::entry_at(&local(&main)), Some(true));
}

/// 01M3ZGT8ST7HCK6J7VZJ09XE0M: a tree whose `.git` is a symbolic link
/// to the `.git` file of a worktree of another repository gets no
/// write. `riff enable` and `riff disable` refuse and say why.
#[test]
fn enable_refuses_a_tree_whose_dot_git_is_a_symbolic_link() {
    let machine = Machine::new();
    let victim = machine.repo("victim");
    let wt = victim.join(".claude/worktrees/issue-12");
    for args in [
        &["commit", "-q", "--allow-empty", "-m", "x"][..],
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "issue-12",
            wt.to_str().unwrap(),
        ][..],
    ] {
        let git = machine
            .env
            .command("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(&victim)
            .output()
            .unwrap();
        assert!(git.status.success(), "git {args:?}: {git:?}");
    }
    let tree = machine.plain("tree");
    std::os::unix::fs::symlink(wt.join(".git"), tree.join(".git")).unwrap();
    for command in ["enable", "disable"] {
        let out = machine.riff(&tree, &[command]).output().unwrap();
        assert!(!out.status.success(), "{out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("is a symbolic link"), "{stderr}");
        assert!(stderr.contains(&victim.display().to_string()), "{stderr}");
    }
    assert!(!local(&victim).exists(), "riff wrote in the victim");
    assert!(!wt.join(".claude").exists(), "riff wrote in the worktree");

    // The worktree that git made still passes.
    let out = machine.run(&wt, &["enable"]);
    assert!(
        out.starts_with(&format!("Turned riff on in {}.", local(&victim).display())),
        "{out}"
    );
}

/// 01M3XY2SKQ27K3TE4NV28FHTVV.
#[test]
fn enable_global_and_disable_global_change_only_the_user_entry() {
    let machine = Machine::new();
    let repo = machine.repo("app");
    std::fs::create_dir_all(machine.user().parent().unwrap()).unwrap();
    std::fs::write(
        machine.user(),
        r#"{"model": "opus", "enabledPlugins": {"other@x": true}}"#,
    )
    .unwrap();
    let out = machine.run(&repo, &["enable", "--global"]);
    assert!(
        out.starts_with(&format!("Turned riff on in {}.", machine.user().display())),
        "{out}"
    );
    assert_eq!(
        read(&machine.user()),
        json!({"model": "opus", "enabledPlugins": {"other@x": true, "riff@riff": true}})
    );
    assert!(!repo.join(".claude").exists());
    assert!(machine.run(&repo, &["server"]).contains("riff on"));

    machine.run(&repo, &["disable", "--global"]);
    assert_eq!(
        read(&machine.user()),
        json!({"model": "opus", "enabledPlugins": {"other@x": true}})
    );
    assert!(!repo.join(".claude").exists());
    assert!(machine.riff_settings().contains("scope = \"none\""));
}

/// 01M3XY2SKQ27K3TE4NV28FHTVV.
#[test]
fn enable_writes_the_local_or_the_shared_settings_and_disable_removes_the_entry() {
    let machine = Machine::new();
    let repo = machine.repo("app");
    std::fs::create_dir_all(repo.join(".claude")).unwrap();
    let before = json!({"permissions": {"allow": ["Bash(ls)"]}, "model": "opus"});
    for file in [local(&repo), shared(&repo)] {
        std::fs::write(file, before.to_string()).unwrap();
    }
    let mut with = before.clone();
    with["enabledPlugins"] = json!({"riff@riff": true});

    // In a directory of the repository: riff writes at the top.
    let sub = repo.join("src");
    std::fs::create_dir(&sub).unwrap();
    let out = machine.run(&sub, &["enable"]);
    assert!(out.contains("riff setup"), "{out}");
    assert_eq!(read(&local(&repo)), with);
    assert_eq!(read(&shared(&repo)), before);
    assert_eq!(riff::enable::entry_at(&machine.user()), None);
    machine.run(&sub, &["disable"]);
    assert_eq!(read(&local(&repo)), before);

    machine.run(&repo, &["enable", "--shared"]);
    assert_eq!(read(&shared(&repo)), with);
    assert_eq!(read(&local(&repo)), before);
    // The team turned riff on, and this person says no for this clone.
    let out = machine.run(&repo, &["disable"]);
    assert!(out.starts_with("Wrote a no for this repository"), "{out}");
    assert_eq!(read(&shared(&repo)), with);
    assert!(machine.run(&repo, &["server"]).contains("riff off"));
    machine.run(&repo, &["enable"]);
    machine.run(&repo, &["disable", "--shared"]);
    assert_eq!(read(&shared(&repo)), before);

    // The two flags do not go together, and a place of a repository
    // needs a repository.
    let both = machine
        .riff(&repo, &["enable", "--shared", "--global"])
        .output()
        .unwrap();
    assert!(!both.status.success());
    let outside = machine
        .riff(&machine.plain("downloads"), &["enable"])
        .output()
        .unwrap();
    assert!(!outside.status.success());
    let err = String::from_utf8_lossy(&outside.stderr);
    assert!(err.contains("not in a git repository"), "{err}");
}

/// The `TMPDIR` of a worker is in the home of the person, and the home
/// can be a git repository. The test dirs of a machine are outside it,
/// so each command that writes settings writes nothing in the home
/// (01M49NP2JW8JFWYY56K7AK3H05).
#[test]
fn a_tmpdir_in_a_home_that_is_a_repository_gets_no_settings() {
    let home = isolated::outside_git();
    let git = Command::new("git")
        .args(["init", "-q"])
        .current_dir(home.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status();
    assert!(git.unwrap().success());
    let tmp = home.path().join(".cache/riff/tmp/w1");
    std::fs::create_dir_all(&tmp).unwrap();

    let machine = Machine::with(Isolated::in_roots(&[tmp, std::env::temp_dir()]));
    assert!(!isolated::in_git(machine.env.path()));
    let plain = machine.plain("downloads");
    let out = machine.connect(&plain, &["--scope", "repo"]);
    assert!(
        out.contains("This directory is not in a git repository."),
        "{out}"
    );
    for args in [&["enable"][..], &["enable", "--shared"]] {
        let out = machine.riff(&plain, args).output().unwrap();
        assert!(!out.status.success(), "riff {args:?}: {out:?}");
    }
    // Outside a repository, setup writes in the dir itself.
    machine.run(&plain, &["setup"]);
    assert!(shared(&plain).is_file());
    assert!(!home.path().join(".claude").exists());
}

/// To turn riff off in one repository changes nothing in another one.
#[test]
fn disable_in_one_repository_leaves_the_other_repositories() {
    let machine = Machine::new();
    let (a, b) = (machine.repo("a"), machine.repo("b"));
    machine.run(&a, &["enable", "--global"]);
    let user = std::fs::read_to_string(machine.user()).unwrap();
    machine.run(&a, &["disable"]);
    assert!(machine.run(&a, &["server"]).contains("riff off"));
    assert!(machine.run(&b, &["server"]).contains("riff on"));
    assert_eq!(std::fs::read_to_string(machine.user()).unwrap(), user);
    assert!(!b.join(".claude").exists());
}

/// 01M3XY2SYKG91SAB2FS1QNCZ2H.
#[test]
fn riff_server_shows_whether_riff_is_on_here() {
    let machine = Machine::new();
    let repo = machine.repo("app");
    let out = machine.run(&repo, &["server"]);
    assert!(
        out.contains("\nrepository  riff off. To turn it on: riff enable\n"),
        "{out}"
    );
    machine.run(&repo, &["enable"]);
    let out = machine.run(&repo, &["server"]);
    let on = format!(
        "\nrepository  riff on ({}). To turn it off: riff disable\n",
        local(&repo).display()
    );
    assert!(out.contains(&on), "{out}");
    let out = machine.run(&machine.plain("downloads"), &["server"]);
    assert!(
        out.contains("riff off: this directory is not in a git repository."),
        "{out}"
    );
}

type Calls = Arc<Mutex<Vec<String>>>;

/// A server that answers each call with `{}` and records its path.
async fn counting_server(calls: Calls) -> String {
    let router =
        axum::Router::new()
            .fallback(|| async { "{}" })
            .layer(axum::middleware::map_request(
                move |r: axum::extract::Request| {
                    calls.lock().unwrap().push(r.uri().path().to_owned());
                    std::future::ready(r)
                },
            ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    url
}

/// `riff ARGS` in `dir` as a process of Claude Code, with `stdin`.
async fn entry(machine: &Machine, server: &str, dir: &Path, stdin: &str, args: &[&str]) -> Output {
    let mut cmd = machine.riff(dir, args);
    cmd.env("RIFF_SERVER", server)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let stdin = stdin.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Runs each hook and the status line in `dir`, and gives what they
/// printed.
async fn each_hook(machine: &Machine, server: &str, dir: &Path) -> String {
    let id = format!(r#""session_id":"{ID}""#);
    let mut printed = String::new();
    for source in ["startup", "resume", "clear", "compact"] {
        let input = format!(r#"{{{id},"source":"{source}"}}"#);
        let out = entry(machine, server, dir, &input, &["hook", "session-start"]).await;
        assert!(out.status.success(), "{source}: {out:?}");
        printed.push_str(&stdout(&out));
    }
    let input = format!(r#"{{{id},"reason":"logout"}}"#);
    for args in [
        &["hook", "session-end"][..],
        &["hook", "stop"][..],
        &["hook", "compact", "--session", ID][..],
        &["statusline"][..],
    ] {
        let out = entry(machine, server, dir, &input, args).await;
        assert!(out.status.success(), "{args:?}: {out:?}");
        printed.push_str(&stdout(&out));
    }
    printed
}

/// 01M3XY2ST8R67SKTXJECAYJZRX: no call to the server, no `git`, and no
/// output, in a repository with riff off and outside a repository.
#[tokio::test(flavor = "multi_thread")]
async fn the_hooks_and_the_status_line_do_nothing_where_riff_is_off() {
    let machine = Machine::new();
    let calls = Calls::default();
    let server = counting_server(calls.clone()).await;
    let (repo, plain) = (machine.repo("app"), machine.plain("downloads"));

    for dir in [&repo, &plain] {
        assert_eq!(each_hook(&machine, &server, dir).await, "", "{dir:?}");
    }
    // Each repository is on, and a directory outside git is still off.
    machine.run(&machine.repo("other"), &["enable", "--global"]);
    assert_eq!(each_hook(&machine, &server, &plain).await, "");
    // A no for one repository wins over each repository.
    machine.run(&repo, &["disable"]);
    assert_eq!(each_hook(&machine, &server, &repo).await, "");
    machine.run(&repo, &["disable", "--global"]);
    machine.run(&repo, &["enable"]);
    machine.run(&repo, &["disable"]);

    let made = calls.lock().unwrap().clone();
    assert!(
        made.is_empty(),
        "riff is off, and it sent requests: {made:?}"
    );
    assert_eq!(machine.git_calls(), "", "riff is off, and it ran git");

    // The same entries with riff on: they call the server, and the
    // start context names the file and `riff disable`.
    machine.run(&repo, &["enable"]);
    let printed = each_hook(&machine, &server, &repo).await;
    let line = format!(
        "- riff is on in this repository by {}. To turn it off, your user runs `riff disable` \
         there in a terminal.",
        local(&repo).display()
    );
    assert!(printed.contains(&line), "{printed}");
    assert!(printed.contains("riff a6cf2205"), "{printed}");
    assert!(!calls.lock().unwrap().is_empty());
    assert_ne!(machine.git_calls(), "");
}

/// `RIFF_ON=1` turns riff on for a process, also outside a repository
/// (01M3XY2SWEK0N8MC3MY4TMYTD3).
#[tokio::test(flavor = "multi_thread")]
async fn riff_on_turns_riff_on_for_a_process() {
    let machine = Machine::new();
    let plain = machine.plain("downloads");
    let input = format!(r#"{{"session_id":"{ID}","source":"startup"}}"#);
    let mut cmd = machine.riff(&plain, &["hook", "session-start"]);
    cmd.env("RIFF_ON", "1").stdin(Stdio::piped());
    let mut child = cmd.stdout(Stdio::piped()).spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(stdout(&out).contains("riff watch"), "{out:?}");
    let out = machine
        .riff(&plain, &["server"])
        .env("RIFF_ON", "1")
        .output()
        .unwrap();
    assert!(stdout(&out).contains("riff on (RIFF_ON=1)"), "{out:?}");
}

/// The how-tos of the book name real commands and real flags: each
/// `riff` command of the part runs with `--help`, and the help of the
/// command has each flag of the how-tos.
#[test]
fn the_how_tos_of_the_book_name_real_commands() {
    let connect = book::commands_of_part("how-it-works.md", "Connect");
    for command in [
        "riff enable",
        "riff disable",
        "riff enable --shared",
        "riff enable --global",
        "riff connect claude --scope repo",
        "riff connect claude --scope global",
        "riff connect claude --scope none",
        "riff server",
    ] {
        assert!(
            connect.iter().any(|c| c == command),
            "{command}: {connect:?}"
        );
    }
    let riff: Vec<String> = connect
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    book::each_is_real(&riff);

    for (page, heading, commands) in [
        (
            "start-a-riff.md",
            "Turn riff on in a project",
            ["riff enable"],
        ),
        (
            "join-a-riff.md",
            "Turn riff on in your project",
            ["riff enable"],
        ),
        (
            "start-a-team-riff.md",
            "Turn riff on in the project",
            ["riff enable --shared"],
        ),
    ] {
        let found = book::commands_of_part(page, heading);
        assert_eq!(found, commands, "{page}: {heading}");
        book::each_is_real(&found);
    }

    let help = |args: &[&str]| {
        let out = Isolated::shared().riff().args(args).output().unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let part = book::part("how-it-works.md", "Connect");
    for command in ["enable", "disable"] {
        let text = help(&[command, "--help"]);
        for flag in ["--local", "--shared", "--global"] {
            assert!(text.contains(flag), "riff {command} --help: {flag}");
            assert!(part.contains(flag), "the book: {flag}");
        }
    }
    let text = help(&["connect", "claude", "--help"]);
    assert!(text.contains("--scope"), "{text}");
    for scope in ["repo", "global", "none"] {
        assert!(text.contains(scope), "riff connect claude --help: {scope}");
    }
}

/// 01M3XY2T542DCHBN95H9PX4AGQ: `riff workers start` starts nothing
/// where riff is off, and names `riff enable`. After `riff enable`, the
/// refusal is gone: the next one is that the person is not in tmux.
#[test]
fn riff_workers_start_starts_nothing_where_riff_is_off() {
    let machine = Machine::new();
    let repo = machine.repo("app");
    let start = || {
        let mut cmd = machine.riff(&repo, &["workers", "start", "1"]);
        let out = cmd.env_remove("TMUX").output().unwrap();
        assert!(!out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    assert_eq!(
        start(),
        "riff starts no worker here: riff off. To turn it on: riff enable\n"
    );
    assert_eq!(machine.git_calls(), "", "riff ran git where it is off");
    machine.run(&repo, &["enable"]);
    let on = start();
    assert!(!on.contains("riff enable"), "{on}");
    assert!(on.contains("tmux"), "{on}");
}

/// 01M3XY2ST8R67SKTXJECAYJZRX: `riff mcp` has no tool where riff is
/// off, and makes no call to the server.
#[tokio::test(flavor = "multi_thread")]
async fn riff_mcp_serves_no_tool_where_riff_is_off() {
    let machine = Machine::new();
    let calls = Calls::default();
    let server = counting_server(calls.clone()).await;
    let repo = machine.repo("app");
    let mut cmd = tokio::process::Command::from(machine.riff(&repo, &["mcp"]));
    let mut child = cmd
        .env("RIFF_SERVER", &server)
        .env("RIFF_SESSION", ID)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let client = ().serve(io).await.unwrap();
    let info = client.peer_info().unwrap();
    assert_eq!(info.instructions.as_deref(), Some(riff::text::MCP_OFF));
    assert!(riff::text::MCP_OFF.contains("`riff enable`"));
    assert!(info.capabilities.tools.is_none(), "{info:?}");
    let tools = client.list_all_tools().await.unwrap_or_default();
    assert!(tools.is_empty(), "{tools:?}");
    // It ends when the agent tool closes the stream, with no end call.
    client.cancel().await.unwrap();
    let ended = isolated::in_time(Duration::from_secs(20), child.wait()).await;
    assert!(ended.is_ok(), "riff mcp did not end");
    let made = calls.lock().unwrap().clone();
    assert!(
        made.is_empty(),
        "riff is off, and it sent requests: {made:?}"
    );
    assert_eq!(machine.git_calls(), "");
}

/// 01M3XY2T0R2Q39XYX8AYV7T0RK: a person turned the riff server off for
/// the project in the `/mcp` dialog of Claude Code.
#[tokio::test(flavor = "multi_thread")]
async fn the_start_hook_and_the_status_line_say_when_the_riff_server_is_off_in_mcp() {
    let machine = Machine::new();
    let server = counting_server(Calls::default()).await;
    let (repo, other) = (machine.repo("app"), machine.repo("other"));
    machine.run(&repo, &["enable", "--global"]);
    let state = json!({"projects": {
        repo.to_str().unwrap(): {"disabledMcpServers": ["github", "plugin:riff:riff"]},
        other.to_str().unwrap(): {"disabledMcpServers": ["github"]},
    }});
    std::fs::write(machine.env.home().join(".claude.json"), state.to_string()).unwrap();

    let start = format!(r#"{{"session_id":"{ID}","source":"startup"}}"#);
    let sub = repo.join("src");
    std::fs::create_dir(&sub).unwrap();
    for dir in [&repo, &sub] {
        let hook = entry(&machine, &server, dir, &start, &["hook", "session-start"]).await;
        let context = stdout(&hook);
        assert!(
            context.contains("The riff server is turned off for this project in Claude Code"),
            "{context}"
        );
        assert!(context.contains("`/mcp`"), "{context}");
        assert!(context.contains("riff tell lead"), "{context}");
        let line = entry(&machine, &server, dir, &start, &["statusline"]).await;
        assert_eq!(
            stdout(&line),
            "riff a6cf2205 (no tools: the riff server is off, turn it on in /mcp)\n"
        );
    }

    // Another project of the same machine has the riff server.
    let hook = entry(
        &machine,
        &server,
        &other,
        &start,
        &["hook", "session-start"],
    )
    .await;
    assert!(!stdout(&hook).contains("turned off"), "{hook:?}");
    let line = entry(&machine, &server, &other, &start, &["statusline"]).await;
    assert!(!stdout(&line).contains("no tools"), "{line:?}");
}
