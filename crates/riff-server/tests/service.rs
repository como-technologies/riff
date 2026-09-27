//! `riff-server install` and `uninstall` write the service files and
//! run `systemctl`. A fake `systemctl` logs its arguments.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// A fake `systemctl` that logs its arguments and exits with `status`.
fn fake_systemctl(dir: &Path, status: u8) -> PathBuf {
    let path = dir.join("systemctl");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\necho \"$*\" >> {}\nexit {status}\n",
            dir.join("log").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn server(config: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = assert_cmd::Command::cargo_bin("riff-server").unwrap();
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("RIFF_") {
            cmd.env_remove(name);
        }
    }
    cmd.args(args).env("XDG_CONFIG_HOME", config).assert()
}

fn log(tmp: &Path) -> String {
    std::fs::read_to_string(tmp.join("log")).unwrap_or_default()
}

#[test]
fn install_writes_the_unit_and_starts_the_service() {
    let tmp = tempfile::tempdir().unwrap();
    let systemctl = fake_systemctl(tmp.path(), 0);
    let user = tmp.path().join("systemd/user");
    let out = server(
        tmp.path(),
        &[
            "install",
            "--systemctl",
            systemctl.to_str().unwrap(),
            "--listen=127.0.0.1:9999",
            "--admin=mike",
        ],
    )
    .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(stdout.contains("It listens on 127.0.0.1:9999."), "{stdout}");
    assert!(stdout.contains("loginctl enable-linger"), "{stdout}");

    let unit = std::fs::read_to_string(user.join("riff-server.service")).unwrap();
    let exe = assert_cmd::cargo::cargo_bin("riff-server");
    assert!(
        unit.contains(&format!("ExecStart={}\n", exe.display())),
        "{unit}"
    );
    let env = std::fs::read_to_string(user.join("riff-server.env")).unwrap();
    assert!(env.contains("RIFF_LISTEN=\"127.0.0.1:9999\"\n"), "{env}");
    assert!(env.contains("RIFF_ADMINS=\"mike\"\n"), "{env}");
    let mode = std::fs::metadata(user.join("riff-server.env"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(
        log(tmp.path()),
        "--user show-environment\n\
         --user daemon-reload\n\
         --user enable riff-server\n\
         --user restart riff-server\n"
    );
}

#[test]
fn install_again_replaces_the_settings() {
    let tmp = tempfile::tempdir().unwrap();
    let systemctl = fake_systemctl(tmp.path(), 0);
    let bin = systemctl.to_str().unwrap();
    server(tmp.path(), &["install", "--systemctl", bin, "--admin=mike"]).success();
    server(tmp.path(), &["install", "--systemctl", bin]).success();
    let env = std::fs::read_to_string(tmp.path().join("systemd/user/riff-server.env")).unwrap();
    assert!(!env.contains("RIFF_ADMINS"), "{env}");
    assert_eq!(log(tmp.path()).matches("restart riff-server").count(), 2);
}

#[test]
fn uninstall_removes_the_service() {
    let tmp = tempfile::tempdir().unwrap();
    let systemctl = fake_systemctl(tmp.path(), 0);
    let bin = systemctl.to_str().unwrap();
    server(tmp.path(), &["install", "--systemctl", bin]).success();
    std::fs::remove_file(tmp.path().join("log")).unwrap();
    server(tmp.path(), &["uninstall", "--systemctl", bin])
        .success()
        .stdout("Removed the riff-server service.\n");
    let user = tmp.path().join("systemd/user");
    assert!(!user.join("riff-server.service").exists());
    assert!(!user.join("riff-server.env").exists());
    assert_eq!(
        log(tmp.path()),
        "--user show-environment\n\
         --user disable --now riff-server\n\
         --user daemon-reload\n"
    );
}

#[test]
fn uninstall_without_a_service_does_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let systemctl = fake_systemctl(tmp.path(), 0);
    server(
        tmp.path(),
        &["uninstall", "--systemctl", systemctl.to_str().unwrap()],
    )
    .success()
    .stdout("The riff-server service is not installed.\n");
    assert_eq!(log(tmp.path()), "--user show-environment\n");
}

#[test]
fn without_systemd_install_and_uninstall_change_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let systemctl = fake_systemctl(tmp.path(), 1);
    let bin = systemctl.to_str().unwrap();
    for command in ["install", "uninstall"] {
        let out = server(tmp.path(), &[command, "--systemctl", bin]).failure();
        let stderr = String::from_utf8_lossy(&out.get_output().stderr);
        assert!(stderr.contains("no systemd user manager"), "{stderr}");
        assert!(stderr.contains("Run riff-server in a terminal"), "{stderr}");
    }
    assert!(!tmp.path().join("systemd").exists());
    assert_eq!(
        log(tmp.path()),
        "--user show-environment\n--user show-environment\n"
    );
}
