//! `just dev` runs the debug builds of a tree (01M3JY12HASECNN6SFQ880JT5H)
//! with the settings of `.env` (01M3K0QM89E2XM1NWSPT4KXSTC). The test
//! copies the `dev` recipe into a justfile of its own, in a tree with a
//! fake `target/debug`. Fakes of `cargo`, `riff` and `riff-server` write
//! each call to a log.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `dev` recipe of the justfile: its doc comment, its line and its
/// body.
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

/// Run `just dev ARGS` in a fake tree. `server` is the tail of the fake
/// `riff-server`, and `env` the `.env` of the tree, if any. Returns the
/// output, the log, and the fake home.
fn dev(args: &[&str], server: &str, env: Option<&str>) -> (Output, String, tempfile::TempDir) {
    let tree = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let log = tree.path().join("log");
    std::fs::write(tree.path().join("justfile"), recipe()).unwrap();
    if let Some(env) = env {
        std::fs::write(tree.path().join(".env"), env).unwrap();
    }
    let bin = tree.path().join("bin");
    fake(&bin.join("cargo"), &log, "");
    let debug = tree.path().join("target/debug");
    fake(&debug.join("riff"), &log, "");
    fake(&debug.join("riff-server"), &log, server);
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let out = Command::new("just")
        .arg("dev")
        .args(args)
        .current_dir(tree.path())
        .env("PATH", path)
        .env("HOME", home.path())
        .env_remove("RIFF_OIDC_CLIENT_ID")
        .output()
        .expect("just runs");
    let log = std::fs::read_to_string(&log).unwrap_or_default();
    (out, log, home)
}

const RESTORE: &str = "Restore the release setup:\n  just install\n";

#[test]
fn dev_builds_links_connects_and_runs_the_server() {
    let (out, log, home) = dev(&["--listen", "127.0.0.1:7979"], "", None);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        log,
        "cargo build --workspace\n\
         riff connect claude\n\
         riff-server --listen 127.0.0.1:7979\n",
    );
    let link = std::fs::read_link(home.path().join(".cargo/bin/riff")).unwrap();
    assert!(link.ends_with("target/debug/riff"), "{}", link.display());
    assert!(link.is_absolute(), "{}", link.display());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.ends_with(RESTORE), "{stdout}");
}

#[test]
fn dev_gives_the_settings_of_env_to_the_server() {
    let server = r#"echo "client $RIFF_OIDC_CLIENT_ID" >> log"#;
    let env = "RIFF_OIDC_CLIENT_ID=my-app\nRIFF_OIDC_CLIENT_SECRET=my-secret\n";
    let (out, log, _home) = dev(&[], server, Some(env));
    assert!(out.status.success(), "{out:?}");
    assert!(log.ends_with("riff-server \nclient my-app\n"), "{log}");
}

#[test]
fn dev_runs_with_no_env() {
    let server = r#"echo "client ${RIFF_OIDC_CLIENT_ID:-none}" >> log"#;
    let (out, log, _home) = dev(&[], server, None);
    assert!(out.status.success(), "{out:?}");
    assert!(log.ends_with("client none\n"), "{log}");
}

#[test]
fn dev_prints_the_restore_steps_when_the_server_fails() {
    let (out, _log, _home) = dev(&[], "exit 3", None);
    assert!(!out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.ends_with(RESTORE), "{stdout}");
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
