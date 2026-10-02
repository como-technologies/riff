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
        let machine = Machine {
            env: Isolated::new(),
        };
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
    let pty = nix::pty::openpty(None, None).unwrap();
    let slave = std::fs::File::from(pty.slave);
    let claude = machine.bin().join("claude");
    let mut child = machine
        .riff(
            dir,
            &["connect", "claude", "--claude", claude.to_str().unwrap()],
        )
        .stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave)
        .spawn()
        .unwrap();
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

/// 01M3XY2SR3VJZAKEPC6CBCS292: an old install has the entry in the user
/// settings, and no answer.
#[test]
fn connect_moves_an_old_install_to_off_and_names_the_repositories() {
    let machine = Machine::new();
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

    let out = machine.connect(&other, &[]);
    assert_eq!(riff::enable::entry_at(&machine.user()), None);
    assert!(
        out.contains("Now it is on only where you turn it on."),
        "{out}"
    );
    assert!(
        out.contains(&format!("\n  cd {} && riff enable\n", used.display())),
        "{out}"
    );
    assert!(!out.contains(&format!("cd {} ", other.display())), "{out}");
    assert!(out.ends_with("cd REPO && riff enable\n"), "{out}");
    // The next update changes nothing, and names nothing.
    let out = machine.connect(&other, &[]);
    assert_eq!(riff::enable::entry_at(&machine.user()), None);
    assert!(!out.contains("Now it is on only"), "{out}");
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
    let ended = tokio::time::timeout(Duration::from_secs(20), child.wait()).await;
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
