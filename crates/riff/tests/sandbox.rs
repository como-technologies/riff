//! The sandbox of a test run, seen from inside (01M4BTG7BXFTWX53R4YAP7YFYD).
//!
//! `just test` runs each test in `riff test-run`. A `bwrap` in a test run
//! fails, so these tests check the sandbox that runs them. Outside a test
//! run (a plain `cargo test`), the inside test prints a skip line.

use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

fn in_a_test_run() -> bool {
    let inside = std::env::var_os(riff::sandbox::TEST_RUN_VAR).is_some();
    if !inside {
        eprintln!("skip: not in a test run; run it with `just test`");
    }
    inside
}

/// The start of PID 1 of this PID namespace, in seconds since 1970.
fn start_of_the_run() -> u64 {
    let stat = std::fs::read_to_string("/proc/1/stat").unwrap();
    let after = &stat[stat.rfind(')').unwrap() + 2..];
    // Field 22 of the line is the start in clock ticks after the boot;
    // `after` starts at field 3.
    let ticks: u64 = after.split(' ').nth(19).unwrap().parse().unwrap();
    let boot: u64 = std::fs::read_to_string("/proc/stat")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("btime "))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    boot + ticks / 100
}

#[test]
fn the_home_of_the_person_is_not_visible() {
    if !in_a_test_run() {
        return;
    }
    let home = std::env::var_os("HOME").unwrap();
    let home = Path::new(&home);
    for secret in [".ssh", ".gnupg", ".local/share/keyrings", ".config/gh"] {
        assert!(!home.join(secret).exists(), "{secret} is visible");
    }
    for entry in std::fs::read_dir(home).unwrap() {
        let entry = entry.unwrap();
        assert!(
            entry.file_type().unwrap().is_dir(),
            "the home has the file {:?}: only the folders of the binds are in a new home",
            entry.file_name()
        );
    }
    assert!(
        !Path::new("/run/user").exists(),
        "the runtime folder is visible"
    );
}

#[test]
fn the_tmp_holds_nothing_from_before_the_run() {
    if !in_a_test_run() {
        return;
    }
    let start = start_of_the_run();
    // riff test-run gives /tmp; .cargo/config.toml gives each test /var/tmp.
    let tmpdir = std::env::var("TMPDIR").unwrap();
    assert!(["/tmp", "/var/tmp"].contains(&tmpdir.as_str()), "{tmpdir}");
    for dir in ["/tmp", "/var/tmp"] {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let made = entry
                .metadata()
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            assert!(
                made + 1 >= start,
                "{dir}/{:?} is older than the run",
                entry.file_name()
            );
        }
    }
}

#[test]
fn a_process_of_the_host_is_not_visible() {
    if !in_a_test_run() {
        return;
    }
    let first = std::fs::read_to_string("/proc/1/comm").unwrap();
    assert_eq!(
        first.trim(),
        "bwrap",
        "PID 1 is the first process of the run"
    );
    for entry in std::fs::read_dir("/proc").unwrap() {
        let name = entry.unwrap().file_name();
        if name.to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        let comm = std::fs::read_to_string(Path::new("/proc").join(&name).join("comm"))
            .unwrap_or_default();
        // The tests of the run start their own servers, but never systemd.
        assert_ne!(comm.trim(), "systemd", "a process of the host is visible");
    }
}

#[test]
fn the_run_has_the_loopback_network_only() {
    if !in_a_test_run() {
        return;
    }
    let dev = std::fs::read_to_string("/proc/net/dev").unwrap();
    let names: Vec<&str> = dev
        .lines()
        .skip(2)
        .filter_map(|l| l.split(':').next())
        .map(str::trim)
        .collect();
    assert_eq!(names, ["lo"]);
    let public: SocketAddr = "1.1.1.1:443".parse().unwrap();
    assert!(TcpStream::connect_timeout(&public, Duration::from_secs(2)).is_err());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    assert!(TcpStream::connect(listener.local_addr().unwrap()).is_ok());
}

/// No variable of the parent reaches a test, but the allow list
/// (01M4CVXJGYCT2HKHJP3BWBV0HC): `just test` gives `riff test-run` a
/// marker `GH_TOKEN`, two cargo registry tokens and an unknown marker
/// variable, and none is here. The runtime folder and the agent of ssh are gone too.
#[test]
fn no_credential_and_no_unknown_variable_of_the_parent_reaches_a_test() {
    if !in_a_test_run() {
        return;
    }
    for name in [
        "GH_TOKEN",
        "CARGO_REGISTRY_TOKEN",
        "CARGO_REGISTRIES_X_TOKEN",
        "RIFF_TEST_PARENT_MARKER",
        "XDG_RUNTIME_DIR",
        "SSH_AUTH_SOCK",
        "RIFF_SESSION_GRANT",
        "CLAUDE_CODE_OAUTH_TOKEN",
    ] {
        assert_eq!(std::env::var_os(name), None, "{name} reached the test");
    }
    let bus = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
    assert_eq!(bus, riff::sandbox::NO_BUS);
}

/// `riff test-run -- true` with only `bin` on the `PATH`.
fn test_run_with_path(bin: &Path) -> std::process::Output {
    let env = isolated::Isolated::new();
    env.riff()
        .args(["test-run", "--", "true"])
        .env("PATH", bin)
        .output()
        .unwrap()
}

#[test]
fn a_test_run_with_no_bwrap_prints_the_sudo_line_and_runs_nothing() {
    let empty = tempfile::tempdir().unwrap();
    let out = test_run_with_path(empty.path());
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("`sudo apt install bubblewrap`"), "{err}");
}

#[test]
fn a_host_that_refuses_the_namespaces_gets_the_apparmor_line() {
    let bin = tempfile::tempdir().unwrap();
    let bwrap = bin.path().join("bwrap");
    std::fs::write(
        &bwrap,
        "#!/bin/sh\necho 'bwrap: No permissions' >&2\nexit 1\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&bwrap, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = test_run_with_path(bin.path());
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("AppArmor"), "{err}");
    assert!(err.contains("apparmor_parser -r /etc/apparmor.d/bwrap-userns-restrict`"));
}
