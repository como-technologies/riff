//! A stop of the unit of the daemon leaves the network of the container
//! that runs (01M5RG2AS2CYJPWGKTV7N2HNYP). `all.rs` builds this file on
//! Linux only.
//!
//! The test uses the real podman, the real manager of systemd for the
//! user and the DNS of the host. It skips, and says why, on a host that
//! has no rootless podman, no manager of systemd for the user, no copy of
//! the session image of this repository, or no DNS. The Gate has none of
//! them. To run it on a host:
//!
//! ```sh
//! cargo test -p riff --test all network:: -- --nocapture
//! ```
//!
//! ```mermaid
//! sequenceDiagram
//!     participant T as the test
//!     participant U as a unit of systemd (the daemon)
//!     participant P as podman
//!     T->>U: systemd-run --user --unit=NAME: this test binary
//!     U->>P: Podman.start_terminal: systemd-run --user --scope podman run
//!     T->>P: podman exec NAME getent hosts HOST: it answers
//!     T->>U: systemctl --user stop NAME
//!     T->>P: podman exec NAME getent hosts HOST: it answers still
//! ```
//!
//! This test binary is the daemon of the test: with [`HOLDER`] set, the
//! test starts the container with the [`Podman`] runtime of riff, as the
//! daemon does, and waits for its stop. The unit sets `INVOCATION_ID`
//! in it, so the runtime starts `podman run` in a scope of its own.
//!
//! [`Podman`]: riff::podman::Podman

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use riff::runtime::{Runtime, SessionSpec};

/// The variable that makes the test the daemon of the test. Its value is
/// the name of the container.
const HOLDER: &str = "RIFF_NETWORK_TEST_HOLDER";

/// The name that the container looks up: the host that riff needs.
const HOST: &str = "api.anthropic.com";

/// The name of the test, for the unit that runs it again.
const TEST: &str = "network::a_stop_of_the_unit_of_the_daemon_leaves_the_network_of_the_container";

/// The test, in its two roles: the daemon of the test (with [`HOLDER`]),
/// and the test that stops it.
#[test]
fn a_stop_of_the_unit_of_the_daemon_leaves_the_network_of_the_container() {
    if let Ok(name) = std::env::var(HOLDER) {
        return hold(&name);
    }
    if let Err(why) = ready() {
        eprintln!("skip: {why}");
        return;
    }

    let name = format!("riff-network-test-{}", std::process::id());
    let _clean = Clean(name.clone());
    // The unit runs this test binary as the daemon of the test.
    let started = host("systemd-run")
        .args(["--user", "--quiet", "--collect"])
        .arg(format!("--unit={name}"))
        .arg(format!("--setenv={HOLDER}={name}"))
        .arg("--")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", TEST])
        .output()
        .unwrap();
    assert!(started.status.success(), "systemd-run: {}", text(&started));

    wait("the container runs", Duration::from_secs(120), || {
        running(&name)
    });
    wait(
        "the container reaches the network",
        Duration::from_secs(30),
        || looks_up(&name).is_ok(),
    );

    // systemd stops the unit: it ends each process in the cgroup of the
    // unit.
    let stopped = host("systemctl")
        .args(["--user", "stop"])
        .arg(format!("{name}.service"))
        .output()
        .unwrap();
    assert!(
        stopped.status.success(),
        "systemctl stop: {}",
        text(&stopped)
    );

    // The container runs on, and still reaches the network.
    assert!(running(&name), "the container ended with the unit");
    if let Err(e) = looks_up(&name) {
        panic!("the container lost its network when the unit stopped: {e}");
    }
}

/// The session image of this repository.
fn image() -> String {
    riff::session_image::Image::of_repository(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .unwrap()
        .to_string()
}

/// The daemon of the test: starts the container as the daemon does, and
/// waits for the stop of the unit.
fn hold(name: &str) {
    let spec = SessionSpec {
        name: name.into(),
        image: image(),
        home: "/home/riff".into(),
        workdir: "/".into(),
        binds: vec![],
        env: vec![],
        memory: "512m".into(),
        cpus: 1,
        command: vec!["sleep".into(), "600".into()],
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(riff::podman::Podman.start_terminal(&spec, &[]))
        .unwrap();
    // The stop of the unit ends this process.
    std::thread::sleep(Duration::from_secs(600));
}

/// Why this host cannot run the test, or `Ok`.
fn ready() -> Result<(), String> {
    if podman(&["info", "--format", "{{.Host.Security.Rootless}}"]).as_deref() != Ok("true") {
        return Err("podman does not run rootless here".into());
    }
    let scope = host("systemd-run")
        .args(["--user", "--scope", "--quiet", "true"])
        .output();
    if !scope.is_ok_and(|out| out.status.success()) {
        return Err("systemd-run --user --scope fails here".into());
    }
    let image = image();
    if podman(&["image", "exists", &image]).is_err() {
        return Err(format!("podman has no copy of {image}: run riff init"));
    }
    let lookup = Command::new("getent").args(["hosts", HOST]).output();
    if !lookup.is_ok_and(|out| out.status.success()) {
        return Err(format!("the host cannot look up {HOST}"));
    }
    Ok(())
}

/// A command of the host. The D-Bus address of a test is not for it:
/// `systemd-run --user` and `systemctl --user` reach the manager of the
/// user.
fn host(program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .stdin(Stdio::null());
    command
}

/// The stdout of `podman ARGS` with no end of line, or its stderr.
fn podman(args: &[&str]) -> Result<String, String> {
    let out = host("podman")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// True while the container `name` runs.
fn running(name: &str) -> bool {
    podman(&[
        "container",
        "inspect",
        "--format",
        "{{.State.Running}}",
        name,
    ])
    .is_ok_and(|out| out == "true")
}

/// `getent hosts HOST` in the container `name`.
fn looks_up(name: &str) -> Result<String, String> {
    podman(&["exec", name, "getent", "hosts", HOST])
}

/// The stdout and the stderr of `out`.
fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Waits up to `limit` for `done`, and fails with `what` when it does not
/// come.
fn wait(what: &str, limit: Duration, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < limit, "{what}: not in {limit:?}");
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// At its drop, stops the unit and removes the container `name`.
struct Clean(String);

impl Drop for Clean {
    fn drop(&mut self) {
        let _ = host("systemctl")
            .args(["--user", "stop"])
            .arg(format!("{}.service", self.0))
            .output();
        let _ = podman(&["rm", "--force", "--time", "0", &self.0]);
    }
}
