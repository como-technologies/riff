//! A machine that updates riff by itself (#190): `riff update --auto`
//! (01M3N7JJC5WQBJ7SJZSZNBAVVR), the start of one update for a newer
//! release (01M3N7JJEKZMN1E5NJQRK2QYVB, 01M3N7JJH0SXXQYYBAHWPCNQGX), and
//! the message to the lead (01M3N7JJKBME6VSNTHD8VPN3K9).
//!
//! A real `riff-server` with no sign-in stands in for the riff. Each of
//! its replies names a newer release. The update runs a fake `cargo`, a
//! fake `riff` for `riff connect` and a fake `riff-server` that log their
//! arguments. Each test is one machine: one [`Isolated`] environment.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use axum::http::HeaderValue;
use axum::response::Response;
use isolated::Isolated;
use riff_core::build::{Build, HEADER};

/// A build of the next minor version: the release of the next wave.
fn newer() -> Build {
    let v = Build::this().semver().unwrap();
    Build {
        version: format!("{}.{}.0", v.major, v.minor + 1),
        ..Build::this()
    }
}

/// The release tag of [`newer`].
fn newer_tag() -> String {
    format!("v{}", newer().version)
}

/// The release tag of this build.
fn this_tag() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

/// A real riff-server of this build, with no sign-in. Each reply names
/// the build [`newer`].
async fn newer_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let value = HeaderValue::from_str(&newer().to_string()).unwrap();
    let router =
        riff_server::router().layer(axum::middleware::map_response(move |mut r: Response| {
            r.headers_mut().insert(HEADER, value.clone());
            std::future::ready(r)
        }));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

/// A git repository with a GitHub origin, so the place of each session
/// is `como-technologies/riff`.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ],
    ] {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status();
        assert!(status.unwrap().success());
    }
    dir
}

/// A fake command `name` in `bin` that logs its arguments to
/// `bin/NAME.log`, prints `stdout` and exits with `status`.
fn fake_command(bin: &Path, name: &str, stdout: &str, status: u8) {
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
}

fn log(bin: &Path, name: &str) -> String {
    std::fs::read_to_string(bin.join(format!("{name}.log"))).unwrap_or_default()
}

/// The line of the fake `cargo` for an install of the release `tag`.
fn install(tag: &str) -> String {
    format!(
        "install --locked --git {} --tag {tag} riff riff-server\n",
        env!("CARGO_PKG_REPOSITORY")
    )
}

/// One machine: its environment, its fake commands and a clone.
struct Machine {
    env: Isolated,
    bin: tempfile::TempDir,
    repo: tempfile::TempDir,
    server: String,
}

impl Machine {
    /// A machine with a fake `cargo` that exits with `cargo_status`.
    async fn new(cargo_status: u8) -> Machine {
        let bin = tempfile::tempdir().unwrap();
        fake_command(bin.path(), "cargo", "", cargo_status);
        fake_command(bin.path(), "riff", "Installed the riff plugin.", 0);
        fake_command(
            bin.path(),
            "riff-server",
            &format!("riff-server {}", newer()),
            0,
        );
        Machine {
            env: Isolated::new(),
            bin,
            repo: repo(),
            server: newer_server().await,
        }
    }

    /// `riff ARGS` as the session `session` of mike on pangolin, with the
    /// fake commands first in `PATH`.
    fn riff(&self, session: &str, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.bin.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(self.repo.path())
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("RIFF_SESSION", session)
            .env("PATH", path);
        cmd
    }

    /// Runs `riff ARGS` as `session`, away from the runtime of the server.
    async fn run(&self, session: &str, args: &[&str]) -> Output {
        let mut cmd = self.riff(session, args);
        tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap()
    }

    /// The local files of riff on this machine.
    fn state(&self) -> PathBuf {
        self.env.riff_home().join("state")
    }

    /// Each message of the lead session `lead`, in full.
    async fn lead_messages(&self, lead: &str) -> String {
        let out = self.run(lead, &["read", "--all"]).await;
        assert!(out.status.success(), "{}", text(&out.stderr));
        text(&out.stdout)
    }
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// Waits until `test` is true, for at most `limit`.
async fn wait_for(limit: Duration, mut test: impl AsyncFnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if test().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    test().await
}

#[tokio::test]
async fn update_auto_sets_and_shows_the_setting() {
    let machine = Machine::new(0).await;
    let show = machine.run("s1", &["update", "--auto"]).await;
    assert!(
        text(&show.stdout).starts_with("update.auto = false: "),
        "{}",
        text(&show.stdout)
    );
    let on = machine.run("s1", &["update", "--auto", "on"]).await;
    assert!(on.status.success(), "{}", text(&on.stderr));
    assert!(text(&on.stdout).starts_with("update.auto = true: "));
    let config = std::fs::read_to_string(machine.env.riff_home().join("config.toml")).unwrap();
    assert_eq!(config, "[update]\nauto = true\n");
    let off = machine.run("s1", &["update", "--auto", "off"]).await;
    assert!(text(&off.stdout).starts_with("update.auto = false: "));
    // The setting installs nothing.
    assert_eq!(log(machine.bin.path(), "cargo"), "");
    let both = machine
        .run("s1", &["update", "--auto", "on", "--tag", "v0.1.0"])
        .await;
    assert!(!both.status.success());
}

/// With `update.auto = true`, many processes see a newer release at the
/// same time. One `riff update --tag` of that release runs for the
/// machine, and the lead gets one message.
#[tokio::test(flavor = "multi_thread")]
async fn one_update_runs_for_the_machine_with_the_release_of_the_server() {
    let machine = Machine::new(0).await;
    // The first session of mike in the repository is the lead.
    assert!(machine.run("lead", &["whoami"]).await.status.success());
    machine.run("w", &["update", "--auto", "on"]).await;

    let mut children: Vec<_> = (0..6)
        .map(|n| {
            let mut cmd = machine.riff(&format!("w{n}"), &["who"]);
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
            cmd.spawn().unwrap()
        })
        .collect();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    let done = wait_for(Duration::from_secs(20), async || {
        machine
            .lead_messages("lead")
            .await
            .contains("updated itself")
    })
    .await;
    let update_log = std::fs::read_to_string(riff::local::update_log(&machine.state()));
    assert!(done, "no message to the lead. update.log: {update_log:?}");
    // Late processes of the first round, and a new round, start nothing.
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(machine.run("w9", &["who"]).await.status.success());
    tokio::time::sleep(Duration::from_secs(1)).await;

    assert_eq!(log(machine.bin.path(), "cargo"), install(&newer_tag()));
    assert_eq!(
        log(machine.bin.path(), "riff"),
        "connect claude --claude claude\n"
    );
    let messages = machine.lead_messages("lead").await;
    let words = format!(
        "riff on pangolin updated itself from {} to {}.",
        this_tag(),
        newer_tag()
    );
    assert_eq!(messages.matches(&words).count(), 1, "{messages}");
    assert_eq!(
        riff::local::tried(&machine.state()).as_deref(),
        Some(newer_tag().as_str())
    );
}

/// With `update.auto = false`, nothing runs, and the note to update
/// stays.
#[tokio::test]
async fn nothing_runs_with_update_auto_off() {
    let machine = Machine::new(0).await;
    let out = machine.run("w1", &["who"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stderr = text(&out.stderr);
    assert!(stderr.contains("Run riff update"), "{stderr}");
    assert!(!stderr.contains("in the background"), "{stderr}");
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(log(machine.bin.path(), "cargo"), "");
    assert_eq!(riff::local::tried(&machine.state()), None);
    assert!(!riff::local::update_log(&machine.state()).exists());
}

/// A failed update keeps the old riff, tells the lead once, and does not
/// try the same release again.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_update_keeps_the_old_riff_and_tells_the_lead_once() {
    let machine = Machine::new(101).await;
    assert!(machine.run("lead", &["whoami"]).await.status.success());
    machine.run("w", &["update", "--auto", "on"]).await;

    let out = machine.run("w1", &["who"]).await;
    assert!(
        text(&out.stderr).contains("riff installs it now, in the background"),
        "{}",
        text(&out.stderr)
    );
    let told = wait_for(Duration::from_secs(20), async || {
        machine
            .lead_messages("lead")
            .await
            .contains("cannot update itself")
    })
    .await;
    assert!(told, "no message to the lead");
    for n in 2..5 {
        assert!(
            machine
                .run(&format!("w{n}"), &["who"])
                .await
                .status
                .success()
        );
    }
    tokio::time::sleep(Duration::from_secs(1)).await;

    // cargo ran once. The plugin and the binaries stay as they were.
    assert_eq!(log(machine.bin.path(), "cargo"), install(&newer_tag()));
    assert_eq!(log(machine.bin.path(), "riff"), "");
    let version = machine.run("w", &["--version"]).await;
    assert!(text(&version.stdout).contains(&Build::this().to_string()));
    let messages = machine.lead_messages("lead").await;
    assert_eq!(
        messages.matches("cannot update itself").count(),
        1,
        "{messages}"
    );
    let words = format!(
        "riff on pangolin cannot update itself from {} to {}: cargo install failed",
        this_tag(),
        newer_tag()
    );
    assert!(messages.contains(&words), "{messages}");
}

/// "Update riff by itself" in the book shows the real commands and
/// texts, and `riff update --help` has the flag.
#[test]
fn the_book_shows_how_to_update_riff_by_itself() {
    let book = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let start = book
        .find("\n### Update riff by itself\n")
        .expect("the how-to");
    let rest = &book[start + 1..];
    let part = &rest[..rest[4..].find("\n#").map_or(rest.len(), |i| i + 4)];
    let note = format!("riff: {}", riff::text::auto_update_started("v0.4.0"));
    let told = riff::text::auto_updated("pangolin", "v0.3.0", "v0.4.0");
    for text in [
        "```sh\nriff update --auto on\n```",
        "```sh\nriff update --auto off\n```",
        "```sh\nriff update --auto\n```",
        "`update.auto`",
        note.as_str(),
        told.as_str(),
        "update.log",
    ] {
        assert!(part.contains(text), "{text:?} is not in the how-to");
    }
    let help = Isolated::new()
        .riff()
        .args(["update", "--help"])
        .output()
        .unwrap();
    assert!(text(&help.stdout).contains("--auto [<AUTO>]"));
    assert!(!text(&help.stdout).contains("--background"));
}

/// A fake `cargo` in `bin` that logs its arguments like
/// [`fake_command`], and its working directory to `bin/cargo.pwd`. The
/// log shows `gone` when it runs in a removed directory.
fn fake_cargo_with_dir(bin: &Path) {
    let path = bin.join("cargo");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\necho \"$*\" >> {log}\n(/bin/pwd -P 2>/dev/null || echo gone) > {pwd}\n",
            log = bin.join("cargo.log").display(),
            pwd = bin.join("cargo.pwd").display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The directory where the fake `cargo` of [`fake_cargo_with_dir`] ran.
fn cargo_dir(bin: &Path) -> String {
    let pwd = std::fs::read_to_string(bin.join("cargo.pwd")).unwrap_or_default();
    pwd.trim_end().to_owned()
}

/// `command` in `dir`, a directory that a shell removes just before it
/// runs `command`.
fn in_removed_dir(command: &Command, dir: &Path) -> Command {
    std::fs::create_dir_all(dir).unwrap();
    let mut sh = Command::new("sh");
    sh.arg("-c")
        .arg(r#"cd "$1" && rmdir "$1" && shift && exec "$@""#)
        .arg("sh")
        .arg(dir)
        .arg(command.get_program())
        .args(command.get_args())
        .current_dir(dir);
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => sh.env(key, value),
            None => sh.env_remove(key),
        };
    }
    sh
}

/// Runs `command` away from the runtime of the server.
async fn output(mut command: Command) -> Output {
    tokio::task::spawn_blocking(move || command.output().unwrap())
        .await
        .unwrap()
}

/// A `riff` process in a removed worktree sees a newer release. The
/// update in the background runs in the local files of riff, `cargo`
/// runs in the home directory, and the lead gets its message
/// (01M3NT2Q0RNM9PVHT42V459624, 01M3NT2Q30P9GCQGENWMP1NKN2).
#[tokio::test(flavor = "multi_thread")]
async fn the_update_of_a_process_in_a_removed_directory_runs_in_a_directory_that_exists() {
    let machine = Machine::new(0).await;
    fake_cargo_with_dir(machine.bin.path());
    assert!(machine.run("lead", &["whoami"]).await.status.success());
    machine.run("w", &["update", "--auto", "on"]).await;

    let gone = machine
        .repo
        .path()
        .join(".claude/worktrees/verify-issue-12");
    let place = "pangolin/como-technologies/riff#verify-issue-12";
    let who = machine.riff("w1", &["--place", place, "who"]);
    let out = output(in_removed_dir(&who, &gone)).await;
    assert!(!gone.exists());
    assert!(
        text(&out.stderr).contains("riff installs it now, in the background"),
        "{}",
        text(&out.stderr)
    );
    let done = wait_for(Duration::from_secs(20), async || {
        machine
            .lead_messages("lead")
            .await
            .contains("updated itself")
    })
    .await;
    let update_log = std::fs::read_to_string(riff::local::update_log(&machine.state()));
    assert!(done, "no message to the lead. update.log: {update_log:?}");
    assert_eq!(log(machine.bin.path(), "cargo"), install(&newer_tag()));
    let home = machine.env.home().canonicalize().unwrap();
    assert_eq!(cargo_dir(machine.bin.path()), home.display().to_string());
}

/// `riff update` by hand in a removed directory runs `cargo` in the home
/// directory (01M3NT2Q30P9GCQGENWMP1NKN2).
#[tokio::test]
async fn riff_update_by_hand_in_a_removed_directory_runs_in_the_home_directory() {
    let machine = Machine::new(0).await;
    fake_cargo_with_dir(machine.bin.path());
    let gone = machine.repo.path().join("gone");
    let tag = newer_tag();
    let update = machine.riff("w1", &["update", "--tag", &tag]);
    let out = output(in_removed_dir(&update, &gone)).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(log(machine.bin.path(), "cargo"), install(&tag));
    let home = machine.env.home().canonicalize().unwrap();
    assert_eq!(cargo_dir(machine.bin.path()), home.display().to_string());
}

/// An update in the background that fails for a missing directory does
/// not mark the release as tried, so the next `riff` process tries it
/// again (01M3NT2PYFHPB0C19Q2QB2AE6W). The lead still gets its message.
#[tokio::test(flavor = "multi_thread")]
async fn a_failure_for_a_missing_directory_leaves_the_release_untried() {
    let machine = Machine::new(0).await;
    assert!(machine.run("lead", &["whoami"]).await.status.success());
    let tag = newer_tag();
    let args = [
        "update",
        "--background",
        "--tag",
        &tag,
        "--cargo",
        "/no/such/dir/cargo",
    ];
    let out = machine.run("w1", &args).await;
    assert!(!out.status.success());
    assert_eq!(riff::local::tried(&machine.state()), None);
    assert!(!riff::local::updating(&machine.state()));
    let messages = machine.lead_messages("lead").await;
    assert!(
        messages.contains("cannot update itself") && messages.contains("the next riff command"),
        "{messages}"
    );
}
