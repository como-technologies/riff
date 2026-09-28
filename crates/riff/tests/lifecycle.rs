//! `riff server` (01M3K0Q854K18DGXJKQ427W586), the forms of `--server`
//! (01M3K0Q80BCZQD7DNQQ333ZN09) and `riff update`
//! (01M3K0Q892KWM76R9DJC1P37JA). The update runs a fake `cargo`, a fake
//! `riff` and a fake `riff-server` that log their arguments.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use axum::http::HeaderValue;
use axum::response::Response;
use riff_core::build::{Build, HEADER};

/// A build with another commit.
fn other() -> Build {
    Build {
        commit: "0000deadbeef".into(),
        ..Build::this()
    }
}

/// A fake server that answers each call with 200 and names `build`.
async fn fake(build: Build) -> String {
    let stamp = move |mut r: Response| {
        let value = HeaderValue::from_str(&build.to_string()).unwrap();
        async move {
            r.headers_mut().insert(HEADER, value);
            r
        }
    };
    let router = axum::Router::new()
        .fallback(|| async { "{}" })
        .layer(axum::middleware::map_response(stamp));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

/// A real riff-server of this build, with no sign-in. Its address has
/// no scheme.
async fn real() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    addr.to_string()
}

fn riff(args: &[&str]) -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin("riff"));
    cmd.args(args).env_remove("RIFF_SERVER");
    cmd
}

/// Runs `cmd` away from the runtime of the servers.
async fn run(mut cmd: Command) -> Output {
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

#[tokio::test]
async fn server_shows_the_riff_that_riff_uses_from_riff_server_and_the_local_riff() {
    let addr = real().await;
    let mut cmd = riff(&["server"]);
    cmd.env("RIFF_SERVER", format!("http://{addr}"));
    let out = run(cmd).await;
    assert!(out.status.success());
    let lines: Vec<String> = text(&out.stdout).lines().map(str::to_owned).collect();
    assert_eq!(lines[0], format!("riff {}", riff_core::build::VERSION));
    assert_eq!(
        lines[1],
        format!("riff uses http://{addr}: RIFF_SERVER names it.")
    );
    assert_eq!(
        lines[2],
        format!(
            "http://{addr}: answers, the same build. It has no sign-in: it trusts its network."
        )
    );
    assert!(
        lines[3].starts_with("The riff of this machine, http://127.0.0.1:7878: "),
        "{lines:?}"
    );
}

#[tokio::test]
async fn server_takes_host_and_port_with_no_scheme_and_names_the_flag() {
    let addr = real().await;
    let out = run(riff(&["--server", &addr, "server"])).await;
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains(&format!("riff uses http://{addr}: --server names it.")),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("http://{addr}: answers, the same build.")),
        "{stdout}"
    );
}

#[tokio::test]
async fn server_with_no_server_set_uses_the_riff_of_this_machine() {
    let out = run(riff(&["server"])).await;
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("riff uses http://127.0.0.1:7878: the riff of this machine."),
        "{stdout}"
    );
    assert!(!stdout.contains("The riff of this machine,"), "{stdout}");
}

#[tokio::test]
async fn server_names_another_build_and_a_riff_that_does_not_answer() {
    let url = fake(other()).await;
    let mut cmd = riff(&["server"]);
    cmd.env("RIFF_SERVER", &url);
    let stdout = text(&run(cmd).await.stdout);
    assert!(
        stdout.contains(&format!("{url}: answers, another build, {}.", other())),
        "{stdout}"
    );
    let stdout = text(
        &run(riff(&["--server", "127.0.0.1:9", "server"]))
            .await
            .stdout,
    );
    assert!(
        stdout.contains("http://127.0.0.1:9: no answer."),
        "{stdout}"
    );
}

#[test]
fn a_server_that_is_not_a_url_or_host_and_port_is_refused() {
    let out = riff(&["--server", "first:port", "server"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("port is not a port"));
}

/// A fake command `name` in `bin` that logs its arguments to
/// `bin/NAME.log`, prints `stdout` and exits with `status`.
fn fake_command(bin: &Path, name: &str, stdout: &str, status: u8) -> PathBuf {
    let path = bin.join(name);
    let log = bin.join(format!("{name}.log"));
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\necho \"$*\" >> {}\necho '{stdout}'\nexit {status}\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn log(bin: &Path, name: &str) -> String {
    std::fs::read_to_string(bin.join(format!("{name}.log"))).unwrap_or_default()
}

/// `riff update` with the fake commands of `bin` first in PATH, a fake
/// `cargo` that exits with `cargo_status`, and `server`.
fn update(bin: &Path, cargo_status: u8, server: &str) -> Command {
    let cargo = fake_command(bin, "cargo", "", cargo_status);
    fake_command(bin, "riff", "Installed the riff plugin.", 0);
    fake_command(
        bin,
        "riff-server",
        &format!("riff-server {}", Build::this()),
        0,
    );
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let mut cmd = riff(&["update", "--claude", "/opt/claude", "--server", server]);
    cmd.arg("--cargo").arg(cargo).env("PATH", path);
    cmd
}

#[tokio::test]
async fn update_installs_both_binaries_then_updates_the_plugin() {
    let bin = tempfile::tempdir().unwrap();
    let addr = real().await;
    let out = run(update(bin.path(), 0, &addr)).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        log(bin.path(), "cargo"),
        format!(
            "install --locked --git {} riff riff-server\n",
            env!("CARGO_PKG_REPOSITORY")
        )
    );
    assert_eq!(
        log(bin.path(), "riff"),
        "connect claude --claude /opt/claude\n"
    );
    let stdout = text(&out.stdout);
    assert!(stdout.contains("Installed the riff plugin."), "{stdout}");
    assert!(
        stdout.ends_with("riff is up to date. Start your Claude Code sessions again.\n"),
        "{stdout}"
    );
}

#[tokio::test]
async fn update_tells_to_restart_a_local_riff_of_the_old_build() {
    let bin = tempfile::tempdir().unwrap();
    let url = fake(other()).await;
    let out = run(update(bin.path(), 0, &url)).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains(&format!(
            "The riff at {url} runs the old build. Stop riff-server and start it again."
        )),
        "{stdout}"
    );
    assert_eq!(log(bin.path(), "riff-server"), "--version\n");
}

#[tokio::test]
async fn update_looks_at_the_riff_of_this_machine_when_riff_uses_another() {
    let bin = tempfile::tempdir().unwrap();
    let out = run(update(bin.path(), 0, "http://first:7878")).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(!stdout.contains("first"), "{stdout}");
    assert_eq!(log(bin.path(), "riff-server"), "--version\n");
}

#[tokio::test]
async fn update_stops_when_the_install_fails() {
    let bin = tempfile::tempdir().unwrap();
    let out = run(update(bin.path(), 101, "http://127.0.0.1:9")).await;
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("cargo install failed"));
    assert_eq!(log(bin.path(), "riff"), "");
}
