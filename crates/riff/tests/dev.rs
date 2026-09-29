//! `just dev` tests a tree without the shared riff
//! (01M3MRDESPG8VGMQ1F6KFJXBC5): it runs the debug `riff-server` of the
//! tree on a free local port and Claude Code with the plugin and the
//! debug `riff` of the tree (01M3JY12HASECNN6SFQ880JT5H). It loads the
//! settings of `.env` (01M3K0QM89E2XM1NWSPT4KXSTC). The test copies the
//! `dev` recipe into a justfile of its own, in a tree with a fake
//! `target/debug`. Fakes of `cargo`, `claude`, `riff` and `riff-server`
//! write each call to a log. The fake `riff-server` writes its listen
//! address to a file, and the test listens there in its place.

use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

mod book;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `dev` recipe of the justfile: its line and its body.
fn recipe() -> String {
    let justfile = std::fs::read_to_string(repo().join("justfile")).unwrap();
    let start = justfile.find("\ndev *ARGS:\n").expect("a dev recipe") + 1;
    let end = justfile[start..]
        .find("\n\n")
        .map_or(justfile.len(), |n| start + n);
    justfile[start..end].to_owned()
}

/// Write an executable script that logs its name and arguments, then
/// runs `tail`.
fn fake(path: &Path, log: &Path, tail: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let name = path.file_name().unwrap().to_str().unwrap();
    let script = format!(
        "#!/bin/sh\necho \"{name} $*\" >> {}\n{tail}\n",
        log.display()
    );
    std::fs::write(path, script).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The installed `riff` of a home: a release.
const INSTALLED: &str = "#!/bin/sh\necho riff 0.1.0 release\n";

/// A fake `riff-server` that listens: it writes its listen address to
/// the file `listen`, and the test listens there. Then it waits.
const LISTENS: &str = "echo \"$RIFF_LISTEN\" > listen\nexec sleep 30";

/// One `just dev` at a time: each looks for a free port from 7900 on.
static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The result of one `just dev`.
struct Dev {
    out: Output,
    log: String,
    home: tempfile::TempDir,
    tree: tempfile::TempDir,
}

/// Run `just dev ARGS` in a fake tree. `server` is the tail of the fake
/// `riff-server`, and `env` the `.env` of the tree, if any. The home
/// has an installed `riff`.
fn dev(args: &[&str], server: &str, env: Option<&str>) -> Dev {
    let _one = ONE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let tree = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let log = tree.path().join("log");
    std::fs::write(tree.path().join("justfile"), recipe()).unwrap();
    if let Some(env) = env {
        std::fs::write(tree.path().join(".env"), env).unwrap();
    }
    let installed = home.path().join(".cargo/bin/riff");
    std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
    std::fs::write(&installed, INSTALLED).unwrap();
    std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o755)).unwrap();
    let bin = tree.path().join("bin");
    fake(&bin.join("cargo"), &log, "");
    fake(
        &bin.join("claude"),
        &log,
        "echo \"claude uses $(command -v riff)\" >> log\nriff server",
    );
    let debug = tree.path().join("target/debug");
    fake(
        &debug.join("riff"),
        &log,
        "echo \"riff at $RIFF_SERVER\" >> log\necho \"$RIFF_HOME\" > riff-home",
    );
    fake(&debug.join("riff-server"), &log, server);
    let path = format!(
        "{}:{}:{}",
        bin.display(),
        installed.parent().unwrap().display(),
        std::env::var("PATH").unwrap()
    );
    let mut child = Command::new("just")
        .arg("dev")
        .args(args)
        .current_dir(tree.path())
        .env("PATH", path)
        .env("HOME", home.path())
        .env_remove("RIFF_OIDC_CLIENT_ID")
        .env_remove("RIFF_SERVER")
        .env_remove("RIFF_LISTEN")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("just runs");
    // Listen in place of the fake server, until `just dev` ends.
    let listen = tree.path().join("listen");
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut listener = None;
    while listener.is_none() && Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if let Ok(addr) = std::fs::read_to_string(&listen)
            && !addr.trim().is_empty()
        {
            listener = listen_in_place(addr.trim());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().unwrap();
    drop(listener);
    let log = std::fs::read_to_string(&log).unwrap_or_default();
    Dev {
        out,
        log,
        home,
        tree,
    }
}

/// Listen at `addr` in place of the fake server. Another process can
/// take the port after `just dev` picks it. Then that process listens
/// there, `just dev` sees it and goes on, and the test does not listen.
fn listen_in_place(addr: &str) -> Option<TcpListener> {
    match TcpListener::bind(addr) {
        Ok(listener) => Some(listener),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => None,
        Err(e) => panic!("listen at {addr}: {e}"),
    }
}

/// The listen address of the server of `just dev`, from the log.
fn listen_of(log: &str) -> String {
    let line = log
        .lines()
        .find_map(|l| l.strip_prefix("riff at http://"))
        .unwrap_or_else(|| panic!("no riff call: {log}"));
    line.to_owned()
}

#[test]
fn dev_runs_claude_with_the_tree_build_against_its_own_server() {
    let dev = dev(&[], LISTENS, None);
    assert!(dev.out.status.success(), "{:?}", dev.out);
    let listen = listen_of(&dev.log);
    let port: u16 = listen.strip_prefix("127.0.0.1:").unwrap().parse().unwrap();
    assert!(port >= 7900, "{listen}");
    let tree = dev.tree.path().canonicalize().unwrap();
    let riff = tree.join("target/debug/riff");
    let plugin = tree.join("crates/riff/claude-plugin/riff");
    assert_eq!(
        dev.log,
        format!(
            "cargo build --workspace\n\
             riff-server \n\
             claude --plugin-dir {} --settings {{\"enabledPlugins\":{{\"riff@riff\":false}}}}\n\
             claude uses {}\n\
             riff server\n\
             riff at http://{listen}\n",
            plugin.display(),
            riff.display(),
        ),
    );
    let stdout = String::from_utf8_lossy(&dev.out.stdout);
    assert!(
        stdout.contains(&format!("The riff of this tree: http://{listen}.")),
        "{stdout}"
    );
}

#[test]
fn dev_leaves_the_installed_riff() {
    let dev = dev(&[], LISTENS, None);
    assert!(dev.out.status.success(), "{:?}", dev.out);
    let installed = dev.home.path().join(".cargo/bin/riff");
    assert!(!installed.is_symlink());
    assert_eq!(std::fs::read_to_string(&installed).unwrap(), INSTALLED);
    let version = Command::new(&installed).arg("--version").output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&version.stdout),
        "riff 0.1.0 release\n"
    );
    assert!(!dev.log.contains("connect"), "{}", dev.log);
    assert!(!dev.log.contains("install"), "{}", dev.log);
}

/// `just dev` gives riff a home of its own in the tree
/// (01M3MY2KWKBJCQ0BCNC6533RBW), so riff keeps its settings, local files
/// and secrets there, not in `~/.config/riff` or the OS keyring
/// (01M3MY2KSV73WS8D902YCH2PRX).
#[test]
fn dev_gives_riff_a_home_in_the_tree() {
    let dev = dev(&[], LISTENS, None);
    assert!(dev.out.status.success(), "{:?}", dev.out);
    let home = std::fs::read_to_string(dev.tree.path().join("riff-home")).unwrap();
    let tree = dev.tree.path().canonicalize().unwrap();
    assert_eq!(
        home.trim(),
        tree.join("target/dev-home").display().to_string()
    );
}

/// Another process takes the port after `just dev` picks it, before
/// the test listens there. `just dev` still runs Claude Code.
#[test]
fn dev_runs_when_another_process_takes_the_picked_port() {
    let sync = tempfile::tempdir().unwrap();
    let (picked, stolen) = (sync.path().join("picked"), sync.path().join("stolen"));
    let server = format!(
        "echo \"$RIFF_LISTEN\" > {}\n\
         while [ ! -f {} ]; do sleep 0.05; done\n{LISTENS}",
        picked.display(),
        stolen.display(),
    );
    let thief = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Ok(addr) = std::fs::read_to_string(&picked)
                && !addr.trim().is_empty()
            {
                let listener = TcpListener::bind(addr.trim()).unwrap();
                std::fs::write(&stolen, "").unwrap();
                return (addr.trim().to_owned(), listener);
            }
            assert!(Instant::now() < deadline, "just dev picks no port");
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let dev = dev(&[], &server, None);
    let (addr, _listener) = thief.join().unwrap();
    assert!(dev.out.status.success(), "{:?}", dev.out);
    assert_eq!(listen_of(&dev.log), addr);
}

#[test]
fn dev_skips_a_port_that_listens() {
    let taken = TcpListener::bind("127.0.0.1:7900");
    let dev = dev(&[], LISTENS, None);
    assert!(dev.out.status.success(), "{:?}", dev.out);
    if taken.is_ok() {
        assert_ne!(listen_of(&dev.log), "127.0.0.1:7900");
    }
}

#[test]
fn dev_gives_the_options_and_the_settings_of_env_to_the_server() {
    let server = format!("echo \"client $RIFF_OIDC_CLIENT_ID\" >> log\n{LISTENS}");
    let env = "RIFF_OIDC_CLIENT_ID=my-app\nRIFF_OIDC_CLIENT_SECRET=my-secret\n";
    let dev = dev(&["--require-sign-in"], &server, Some(env));
    assert!(dev.out.status.success(), "{:?}", dev.out);
    assert!(
        dev.log
            .contains("riff-server --require-sign-in\nclient my-app\n"),
        "{}",
        dev.log
    );
}

#[test]
fn dev_runs_with_no_env() {
    let server = format!("echo \"client ${{RIFF_OIDC_CLIENT_ID:-none}}\" >> log\n{LISTENS}");
    let dev = dev(&[], &server, None);
    assert!(dev.out.status.success(), "{:?}", dev.out);
    assert!(dev.log.contains("client none\n"), "{}", dev.log);
}

#[test]
fn dev_shows_the_server_log_and_starts_no_claude_when_the_server_fails() {
    let dev = dev(&[], "echo 'no port' >&2\nexit 3", None);
    assert!(!dev.out.status.success(), "{:?}", dev.out);
    let stdout = String::from_utf8_lossy(&dev.out.stdout);
    assert!(stdout.contains("no port"), "{stdout}");
    assert!(!dev.log.contains("claude"), "{}", dev.log);
}

#[test]
fn git_ignores_env() {
    let status = Command::new("git")
        .args(["check-ignore", "-q", ".env"])
        .current_dir(repo())
        .status()
        .expect("git runs");
    assert!(status.success(), "git does not ignore .env");
}

/// The commands of the `###` part `heading` of `development.md`.
fn commands_of_how_to(heading: &str) -> Vec<String> {
    let page = book::page("development.md");
    let start = page
        .find(&format!("\n### {heading}\n"))
        .unwrap_or_else(|| panic!("{heading} is in development.md"));
    let rest = &page[start + 1..];
    let part = rest[4..].find("\n##").map_or(rest, |end| &rest[..end + 4]);
    book::commands_in(part)
}

/// The output of `just --list` in the repository.
fn recipes() -> String {
    let out = Command::new("just")
        .arg("--list")
        .current_dir(repo())
        .output()
        .expect("just runs");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn the_book_tests_a_change_with_just_dev() {
    let commands = commands_of_how_to("Test a change without the shared riff");
    assert_eq!(commands, ["just dev"]);
    let recipes = recipes();
    assert!(
        recipes.lines().any(|l| l.trim_start().starts_with("dev ")),
        "{recipes}"
    );
}

#[test]
fn the_book_signs_in_to_the_server_of_the_tree_in_the_dev_session() {
    let commands = commands_of_how_to("Test a debug build with sign-in");
    assert_eq!(commands[1..], ["just dev", "! riff login"]);
    let riff: Vec<String> = commands
        .iter()
        .filter_map(|c| c.strip_prefix("! "))
        .map(str::to_owned)
        .collect();
    book::each_is_real(&riff);
}
