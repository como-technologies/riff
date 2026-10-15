//! `riff server` (01M3Q5VE74608N5H2M73RB6Y2Z), the forms of `--server`
//! (01M3K0Q80BCZQD7DNQQ333ZN09) and `riff update`
//! (01M3K0Q892KWM76R9DJC1P37JA). The update runs a fake `cargo`, a fake
//! `riff` and a fake `riff-server` that log their arguments.

use isolated::{Isolated, Span};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use axum::http::HeaderValue;
use axum::response::Response;
use riff::lifecycle::PROBE_WAIT;
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
    fake_header(Some(build.to_string())).await
}

/// A fake server that answers each call with 200 and `header` as the
/// build header, or no build header.
async fn fake_header(header: Option<String>) -> String {
    let stamp = move |mut r: Response| {
        let value = header.as_deref().map(|h| HeaderValue::from_str(h).unwrap());
        async move {
            if let Some(value) = value {
                r.headers_mut().insert(HEADER, value);
            }
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
    let mut cmd = Isolated::shared().riff();
    cmd.args(args).env_remove("RIFF_SERVER");
    cmd
}

/// Runs `cmd` away from the runtime of the servers.
async fn run(mut cmd: Command) -> Output {
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

/// The output of `riff server` from `cmd` under the real riff, when each
/// riff that it looks at answers and it shows the facts. Under load, a
/// probe can pass [`PROBE_WAIT`] and show no answer: then it runs the
/// command again. With no load, one probe with no answer fails the test
/// (01M41A0M2XWCWTWGF7T9DR03W0).
async fn answered(cmd: impl Fn() -> Command) -> String {
    let span = Span::start();
    loop {
        let out = run(cmd()).await;
        assert!(out.status.success(), "{out:?}");
        let stdout = text(&out.stdout);
        let label = |line: &str| line.split_whitespace().next().map(str::to_owned);
        let labels: Vec<Option<String>> = stdout.lines().map(label).collect();
        let complete = !labels.iter().any(|l| l.as_deref() == Some("answer"))
            && FACTS
                .iter()
                .all(|fact| labels.iter().any(|l| l.as_deref() == Some(*fact)));
        if complete {
            return stdout;
        }
        assert!(
            span.within(PROBE_WAIT),
            "a riff gave no answer in {:.1?}, CPU time {:.1?}:\n{stdout}",
            span.wall(),
            span.cpu()
        );
    }
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// True when a riff-server of this machine listens on 7878. Then
/// `riff server` shows it in a `local` line.
fn local_answers() -> bool {
    std::net::TcpStream::connect_timeout(&"127.0.0.1:7878".parse().unwrap(), PROBE_WAIT).is_ok()
}

/// The first line of `riff server`: the release of this build, with its
/// commit and date short.
fn riff_line() -> String {
    let this = Build::this();
    format!(
        "riff        {}  ({}, {})",
        release(),
        &this.commit[..7],
        &this.time[..10]
    )
}

/// 01M3NTEMQAY1Z10H1GX2K6PEAH: against a riff of the same release,
/// `riff server` is a short table, with no local line when RIFF_SERVER
/// is set and the riff of this machine does not answer.
#[tokio::test]
async fn server_shows_the_riff_that_riff_uses_from_riff_server_as_a_table() {
    let addr = real().await;
    let stdout = answered(|| {
        let mut cmd = riff(&["server"]);
        cmd.env("RIFF_SERVER", format!("http://{addr}"));
        cmd
    })
    .await;
    let table = [
        riff_line(),
        format!("server      http://{addr}  (from RIFF_SERVER)"),
        format!("  release   {}  same build ✓", release()),
        "  sign-in   none: the riff trusts its network".to_owned(),
    ];
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[..4], table, "{stdout}");
    // Then the facts of the riff, one on a line.
    let after = 4 + FACTS.len();
    if local_answers() {
        assert_eq!(
            lines[after], "local       http://127.0.0.1:7878",
            "{stdout}"
        );
        assert!(lines[after + 1].starts_with("  release   "), "{stdout}");
    } else {
        assert_eq!(lines.len(), after + 1, "{stdout}");
    }
    // The last line: whether riff is on here (01M3XY2SYKG91SAB2FS1QNCZ2H).
    assert_eq!(
        lines.last(),
        Some(&"repository  riff on (RIFF_ON=1)"),
        "{stdout}"
    );
}

/// The label of each line of facts in `riff server`, in order.
const FACTS: [&str; 8] = [
    "serves", "error", "log", "faults", "saved", "counts", "memory", "started",
];

/// 01M3TJWJ12WEDCXW3W0529KRP2: `riff server` shows each fact of
/// "Monitoring" of the riff that `riff` uses.
#[tokio::test]
async fn server_shows_each_fact_of_the_riff() {
    let addr = real().await;
    // One call, so that the log has a record and a chunk.
    let me = "riff://mike@pangolin/como-technologies/riff?session=a";
    let registered = reqwest::Client::new()
        .post(format!("http://{addr}/v1/register"))
        .header(HEADER, Build::this().to_string())
        .json(&serde_json::json!({ "me": me }))
        .send()
        .await
        .unwrap();
    assert_eq!(registered.status(), 200);

    let stdout = answered(|| {
        let mut cmd = riff(&["server"]);
        cmd.env("RIFF_SERVER", format!("http://{addr}"));
        cmd
    })
    .await;
    let rows: Vec<(&str, &str)> = stdout
        .lines()
        .skip(4)
        .take(FACTS.len())
        .map(|line| {
            let (label, value) = line.trim_start().split_once(' ').unwrap();
            (label, value.trim_start())
        })
        .collect();
    let labels: Vec<&str> = rows.iter().map(|(label, _)| *label).collect();
    assert_eq!(labels, FACTS, "{stdout}");
    let value = |label: &str| rows.iter().find(|(l, _)| *l == label).unwrap().1;
    // It serves, or 503 and why; the last error.
    assert_eq!(value("serves"), "yes");
    assert_eq!(value("error"), "none since the start");
    // The log position, and the time and the duration of the last
    // chunk write.
    let log = value("log");
    assert!(log.starts_with("position "), "{log}");
    assert!(log.contains("the last chunk write was "), "{log}");
    assert!(
        log.contains(" ago and took ") && log.ends_with(" ms"),
        "{log}"
    );
    // The numbers of write errors and skipped records.
    assert_eq!(
        value("faults"),
        "0 write errors, 0 skipped records since the start"
    );
    // The position, the age and the version of the newest checkpoint.
    assert_eq!(value("saved"), "no checkpoint");
    // The numbers of chunks, sessions, cursors, threads and sign-ins.
    let counts = value("counts");
    for part in [
        "chunks",
        "1 sessions",
        "cursors",
        "1 threads",
        "0 live sign-ins",
    ] {
        assert!(counts.contains(part), "{counts}");
    }
    // The memory in use.
    let memory = value("memory");
    assert!(
        memory.ends_with(" MB in use") || memory == "unknown",
        "{memory}"
    );
    // The start time, and how long the replay took.
    let started = value("started");
    assert!(started.contains(" ago; the replay took "), "{started}");
}

#[tokio::test]
async fn server_takes_host_and_port_with_no_scheme_and_names_the_flag() {
    let addr = real().await;
    let stdout = answered(|| riff(&["--server", &addr, "server"])).await;
    assert!(
        stdout.contains(&format!("\nserver      http://{addr}  (from --server)\n")),
        "{stdout}"
    );
    assert!(stdout.contains("same build ✓"), "{stdout}");
}

#[tokio::test]
async fn server_with_no_server_set_uses_the_riff_of_this_machine() {
    let out = run(riff(&["server"])).await;
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains(
            "\nserver      http://127.0.0.1:7878  (the default: the riff of this machine)\n"
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("\nlocal "), "{stdout}");
}

/// 01M3NTEMQAY1Z10H1GX2K6PEAH: a newer server gives its release and
/// build, and the last line says what to run. The fake server answers
/// the sign-in call, and this machine has no sign-in, so it also asks
/// for `riff login`.
#[tokio::test]
async fn server_names_another_build_and_what_to_run() {
    let url = fake(other()).await;
    let mut cmd = riff(&["server"]);
    cmd.env("RIFF_SERVER", &url);
    let stdout = text(&run(cmd).await.stdout);
    assert!(
        stdout.contains(&format!(
            "\n  release   {}  (0000dea, {})  another build; the versions can talk\n",
            release(),
            &Build::this().time[..10]
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains("\n  sign-in   yes, you are not signed in\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("\nRun riff update, then riff login\n"),
        "{stdout}"
    );
    let this = Build::this().semver().unwrap();
    let other_line = Build {
        version: this.line_after().line_after().to_string(),
        ..other()
    };
    let url = fake(other_line.clone()).await;
    let mut cmd = riff(&["server"]);
    cmd.env("RIFF_SERVER", &url);
    let stdout = text(&run(cmd).await.stdout);
    assert!(
        stdout.contains(&format!(
            "\n  release   v{}  (0000dea, {})  another build; this riff cannot talk to it\n",
            other_line.version,
            &other_line.time[..10]
        )),
        "{stdout}"
    );
    assert!(stdout.contains("\nRun riff update"), "{stdout}");
}

#[tokio::test]
async fn server_names_a_riff_that_does_not_answer() {
    let stdout = text(
        &run(riff(&["--server", "127.0.0.1:9", "server"]))
            .await
            .stdout,
    );
    assert!(
        stdout.contains("\nserver      http://127.0.0.1:9  (from --server)\n  answer    none"),
        "{stdout}"
    );
}

/// 01M3NTEMQAY1Z10H1GX2K6PEAH: `--color never` and a pipe print no
/// escape codes; `--color always` prints them.
#[tokio::test]
async fn server_prints_color_only_when_asked_or_in_a_terminal() {
    let url = fake(other()).await;
    for args in [&["server"][..], &["server", "--color", "never"]] {
        let mut cmd = riff(args);
        cmd.env("RIFF_SERVER", &url).env_remove("CLICOLOR_FORCE");
        let stdout = text(&run(cmd).await.stdout);
        assert!(stdout.contains("Run riff update"), "{stdout}");
        assert!(!stdout.contains('\x1b'), "{args:?}: {stdout}");
    }
    let mut cmd = riff(&["server", "--color", "always"]);
    cmd.env("RIFF_SERVER", &url);
    let stdout = text(&run(cmd).await.stdout);
    assert!(
        stdout.contains("\x1b[31mRun riff update, then riff login"),
        "{stdout}"
    );
}

/// A fake D-Bus in `dir`. It takes each connection and says nothing
/// for longer than [`PROBE_WAIT`], then closes it. So a keyring call
/// through it waits that long, the same as a keyring on a busy machine.
/// Returns the value for `DBUS_SESSION_BUS_ADDRESS`.
fn slow_bus(dir: &Path) -> String {
    let path = dir.join("bus");
    let listener = UnixListener::bind(&path).unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                std::thread::sleep(PROBE_WAIT + Duration::from_secs(1));
                drop(stream);
            });
        }
    });
    format!("unix:path={}", path.display())
}

/// 01M3MX598VTWZ02R7J6AYJB2E5: a slow keyring does not hide a riff that
/// answers. riff reads the sign-in of the riff of this machine only
/// after the probe of the riff that `riff` uses ends.
#[tokio::test]
async fn server_shows_a_riff_that_answers_also_with_a_slow_keyring() {
    let dir = tempfile::tempdir().unwrap();
    let url = fake(Build::this()).await;
    let mut cmd = riff(&["server"]);
    cmd.env("RIFF_SERVER", &url)
        .env_remove("RIFF_HOME")
        .env("DBUS_SESSION_BUS_ADDRESS", slow_bus(dir.path()));
    let stdout = text(&run(cmd).await.stdout);
    assert!(
        stdout.contains(&format!(
            "{url}  (from RIFF_SERVER)\n  release   {}  same build",
            release()
        )),
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

/// The release tag of this build.
fn release() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

/// The line of the fake `cargo` for an install of the release `tag`.
fn install(tag: &str) -> String {
    format!(
        "install --locked --git {} --tag {tag} riff riff-server\n",
        env!("CARGO_PKG_REPOSITORY")
    )
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
    assert_eq!(log(bin.path(), "cargo"), install(&release()));
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

/// 01M4923963S666V9YWTZ46ZZ50: `riff update` on a machine with a limit
/// of workers installs the pinned `sccache` after riff. A failed install
/// is one line, and the update goes on.
#[tokio::test]
async fn update_installs_sccache_on_a_machine_with_workers() {
    let addr = real().await;
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("config.toml"), "[workers]\nlimit = 2\n").unwrap();
    let with = |bin: &Path| {
        let mut cmd = update(bin, 0, &addr);
        // Only a fake sccache of bin counts.
        let real = std::env::var_os("PATH").unwrap();
        let rest = std::env::split_paths(&real).filter(|d| !d.join("sccache").exists());
        let path = std::env::join_paths(std::iter::once(bin.to_owned()).chain(rest)).unwrap();
        cmd.env("PATH", path).env("RIFF_HOME", home.path());
        cmd
    };
    let pinned = "install --locked sccache --version 0.18.0\n";

    // The install puts sccache in place.
    let bin = tempfile::tempdir().unwrap();
    let cmd = with(bin.path());
    let cargo = bin.path().join("cargo");
    std::fs::write(
        &cargo,
        format!(
            "#!/bin/sh\necho \"$*\" >> {}/cargo.log\n\
             case \"$*\" in *sccache*) printf '#!/bin/sh\\necho sccache 0.18.0\\n' > {0}/sccache; \
             chmod +x {0}/sccache ;; esac\n",
            bin.path().display()
        ),
    )
    .unwrap();
    let out = run(cmd).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(log(bin.path(), "cargo"), install(&release()) + pinned);
    assert!(
        text(&out.stdout).contains("riff: installs sccache 0.18.0 for the compile cache"),
        "{}",
        text(&out.stdout)
    );

    // With the pinned sccache: no second install.
    let out = run(with(bin.path())).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        log(bin.path(), "cargo"),
        install(&release()) + pinned + &install(&release())
    );

    // A cargo that installs nothing: one line, and the update passes.
    let bin = tempfile::tempdir().unwrap();
    let out = run(with(bin.path())).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let err = text(&out.stderr);
    assert_eq!(
        err.matches("cannot install sccache 0.18.0").count(),
        1,
        "{err}"
    );
    assert!(
        err.contains("The workers there build with no compile cache."),
        "{err}"
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
    let mut cmd = update(bin.path(), 0, "http://first:7878");
    cmd.args(["--tag", &release()]);
    let out = run(cmd).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(!stdout.contains("first"), "{stdout}");
    assert_eq!(log(bin.path(), "riff-server"), "--version\n");
}

#[tokio::test]
async fn update_stops_when_the_install_fails() {
    let bin = tempfile::tempdir().unwrap();
    let mut cmd = update(bin.path(), 101, "http://127.0.0.1:9");
    cmd.args(["--tag", &release()]);
    let out = run(cmd).await;
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("cargo install failed"));
    assert_eq!(log(bin.path(), "riff"), "");
}

#[tokio::test]
async fn update_tells_to_restart_the_local_riff_when_riff_uses_a_remote_riff() {
    let local = fake(other()).await;
    let old = riff::lifecycle::old_riff(Some(&Build::this()), "http://first:7878", &local).await;
    assert_eq!(old.as_deref(), Some(local.as_str()));
    let words = riff::text::updated(old.as_deref());
    assert!(
        words.contains(&format!(
            "The riff at {local} runs the old build. Stop riff-server and start it again."
        )),
        "{words}"
    );
    let same = fake(Build::this()).await;
    let old = riff::lifecycle::old_riff(Some(&Build::this()), "http://first:7878", &same).await;
    assert_eq!(old, None);
}

#[tokio::test]
async fn server_shows_localhost_and_127_0_0_1_as_one_riff() {
    let mut cmd = riff(&["server"]);
    cmd.env("RIFF_SERVER", "http://localhost:7878");
    let stdout = text(&run(cmd).await.stdout);
    assert!(
        stdout.contains("\nserver      http://localhost:7878  (from RIFF_SERVER)\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("\nlocal "), "{stdout}");
}

#[tokio::test]
async fn server_takes_a_bare_ipv6_address_and_one_with_brackets() {
    for (server, url) in [
        ("::1", "http://[::1]:7878"),
        ("[::1]:7878", "http://[::1]:7878"),
    ] {
        let out = run(riff(&["--server", server, "server"])).await;
        assert!(out.status.success(), "{}", text(&out.stderr));
        let stdout = text(&out.stdout);
        assert!(
            stdout.contains(&format!("\nserver      {url}  (from --server)\n")),
            "{stdout}"
        );
        assert!(!stdout.contains("\nlocal "), "{stdout}");
    }
}

/// 01M3MRMAVVKJ5WS8GWCJHWH0R4: with no `--tag`, `riff update` installs
/// the release that the riff runs, also when the repository has a newer
/// tag. riff asks only the riff: a fake `git` that names a newer tag
/// never runs.
#[tokio::test]
async fn update_installs_the_release_of_the_server_not_a_newer_tag() {
    let bin = tempfile::tempdir().unwrap();
    fake_command(bin.path(), "git", "0000 refs/tags/v0.3.0", 0);
    let url = fake(Build {
        version: "0.2.0".into(),
        ..Build::this()
    })
    .await;
    let out = run(update(bin.path(), 0, &url)).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(log(bin.path(), "cargo"), install("v0.2.0"));
    assert_eq!(log(bin.path(), "git"), "");
    assert!(!text(&out.stdout).contains("newest release"));
}

/// 01M3N73Y9DMVMCV0PJE1R8YCFH: when the riff answers with a build
/// header that riff cannot read, or with none, `riff update` installs
/// the newest release tag and says so in one line.
#[tokio::test]
async fn update_installs_the_newest_release_when_it_cannot_read_the_build_of_the_riff() {
    for header in [Some("v2;0.3.0;e58e345".to_owned()), None] {
        let bin = tempfile::tempdir().unwrap();
        fake_command(
            bin.path(),
            "git",
            "a1\trefs/tags/v0.2.0\nb2\trefs/tags/v0.3.0",
            0,
        );
        let url = fake_header(header.clone()).await;
        let out = run(update(bin.path(), 0, &url)).await;
        assert!(out.status.success(), "{header:?}: {}", text(&out.stderr));
        assert_eq!(log(bin.path(), "cargo"), install("v0.3.0"), "{header:?}");
        let stdout = text(&out.stdout);
        assert!(
            stdout.contains(&format!(
                "riff cannot read the build of the riff at {url}, \
                 so riff installs the newest release, v0.3.0.\n"
            )),
            "{header:?}: {stdout}"
        );
    }
}

/// `riff update --tag` installs that release, and asks no riff.
#[tokio::test]
async fn update_with_a_tag_installs_that_tag() {
    let bin = tempfile::tempdir().unwrap();
    let mut cmd = update(bin.path(), 0, "http://127.0.0.1:9");
    cmd.args(["--tag", "v0.1.1"]);
    let out = run(cmd).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(log(bin.path(), "cargo"), install("v0.1.1"));
}

#[tokio::test]
async fn update_refuses_a_tag_that_is_not_a_release() {
    let bin = tempfile::tempdir().unwrap();
    let mut cmd = update(bin.path(), 0, "http://127.0.0.1:9");
    cmd.args(["--tag", "main"]);
    let out = run(cmd).await;
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("main is not a release tag. Give vX.Y.Z"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(log(bin.path(), "cargo"), "");
}

/// With no `--tag` and a riff that does not answer, `riff update`
/// installs nothing and names the flag.
#[tokio::test]
async fn update_with_no_riff_and_no_tag_names_the_flag() {
    let bin = tempfile::tempdir().unwrap();
    let out = run(update(bin.path(), 0, "http://127.0.0.1:9")).await;
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("cannot find the release of the riff at http://127.0.0.1:9"),
        "{stderr}"
    );
    assert!(stderr.contains("riff update --tag vX.Y.Z"), "{stderr}");
    assert_eq!(log(bin.path(), "cargo"), "");
}

/// 01M3MRMAVVKJ5WS8GWCJHWH0R4: when riff uses the riff of this machine,
/// `riff update` installs the newest release tag of the repository. It
/// compares the numbers, and skips a tag that is not a release.
#[tokio::test]
async fn update_with_the_riff_of_this_machine_installs_the_newest_release() {
    let bin = tempfile::tempdir().unwrap();
    fake_command(
        bin.path(),
        "git",
        "a1\trefs/tags/v0.9.3\nb2\trefs/tags/v0.10.0\nc3\trefs/tags/v1.0.0-rc1",
        0,
    );
    let out = run(update(bin.path(), 0, "http://127.0.0.1:7878")).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        log(bin.path(), "git"),
        format!(
            "ls-remote --tags --refs {} v*\n",
            env!("CARGO_PKG_REPOSITORY")
        )
    );
    assert_eq!(log(bin.path(), "cargo"), install("v0.10.0"));
}

/// With no release tag in the repository, `riff update` installs
/// nothing and names the flag.
#[tokio::test]
async fn update_with_no_release_tag_names_the_flag() {
    let bin = tempfile::tempdir().unwrap();
    fake_command(bin.path(), "git", "", 0);
    let out = run(update(bin.path(), 0, "http://localhost:7878")).await;
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("has no release tag. Name one: riff update --tag vX.Y.Z"),
        "{stderr}"
    );
    assert_eq!(log(bin.path(), "cargo"), "");
}
