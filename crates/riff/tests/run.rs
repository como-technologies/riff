//! `riff run ITEM` (#712, 01M4Z81APKC14AVNJWCGRQKNVW): one work item in
//! one rootless podman container, with no daemon.
//!
//! A fake `podman` logs its arguments, the value of its `GH_TOKEN` and
//! its stdin, and plays `claude`: it prints the events of one turn, and
//! ends when its stdin closes. A fake `gh` gives the token, the issue
//! and the pull request. A real `riff-server` holds the claims and the
//! posts. The clone is real: git clones a bare origin on disk.

use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use isolated::Isolated;
use serde_json::Value;

const GH_TOKEN: &str = "gho_FAKEtoken0123456789";
const ACCESS: &str = "sk-ant-oat01-FAKEaccess";
const REFRESH: &str = "sk-ant-ort01-FAKErefresh";
const IMAGE: &str = "localhost/riff-session:test";
/// The image that `riff.toml` of the repository pins.
const PINNED: &str = "ghcr.io/como-technologies/riff-session@sha256:\
                      0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Writes the program `path` with `body`.
fn script(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A fake `gh` that logs each call to `bin/gh.log`.
fn fake_gh(bin: &Path) {
    let body = r#"#!/bin/sh
dir=$(dirname "$0")
echo "gh $*" >> "$dir/gh.log"
case "$*" in
  'auth token') echo GHTOKEN ;;
  'issue view 12 --json number,state,milestone')
    echo '{"number":12,"state":"OPEN","milestone":{"title":"Wave 27"}}' ;;
  'pr view worktree-issue-12 --json number,url,headRefOid,state')
    echo '{"number":40,"url":"https://github.com/como-technologies/riff/pull/40","headRefOid":"1a2b3c4d5e6f","state":"OPEN"}' ;;
  *) exit 1 ;;
esac
"#;
    script(&bin.join("gh"), &body.replace("GHTOKEN", GH_TOKEN));
}

/// A fake `podman`. `image exists` fails when `bin/no-image` is there.
/// `pull` logs, and fails when `bin/no-pull` is there. `run` logs,
/// prints `bin/events`, and waits for the end of stdin. `stop` logs,
/// and ends the `run`.
fn fake_podman(bin: &Path) {
    let body = r#"#!/bin/sh
dir=$(dirname "$0")
case "$1" in
  image) [ -f "$dir/no-image" ] && exit 1; exit 0 ;;
  pull) echo "podman $*" >> "$dir/podman.log"; [ -f "$dir/no-pull" ] && exit 125
    rm -f "$dir/no-image"; exit 0 ;;
  stop) echo "podman $*" >> "$dir/podman.log"; kill "$(cat "$dir/run.pid")"; exit 0 ;;
  run) ;;
  *) exit 2 ;;
esac
printf '%s\n' "$@" > "$dir/run.args"
printf '%s' "$GH_TOKEN" > "$dir/run.token"
echo $$ > "$dir/run.pid"
IFS= read -r line
printf '%s\n' "$line" > "$dir/run.stdin"
echo "a line on stderr with GHTOKEN" >&2
cat "$dir/events"
cat > /dev/null
touch "$dir/run.closed"
"#;
    script(&bin.join("podman"), &body.replace("GHTOKEN", GH_TOKEN));
}

/// The events of one turn of a fake `claude`. A tool result holds the
/// tokens: riff must not show them.
fn events() -> String {
    [
        r#"{"type":"system","subtype":"init","session_id":"s","model":"claude-opus-5-5","permissionMode":"auto"}"#.to_owned(),
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"I read the issue."},{"type":"tool_use","name":"Bash","input":{"command":"gh issue view 12 --comments"}}]}}"#.to_owned(),
        format!(r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","is_error":true,"content":"token {GH_TOKEN} and {ACCESS} leaked"}}]}}}}"#),
        r#"{"type":"system","subtype":"vcs_state_changed","action":"push","branch":"worktree-issue-12"}"#.to_owned(),
        r#"{"type":"result","subtype":"success","is_error":false,"num_turns":3,"duration_ms":5000,"total_cost_usd":0.25,"result":"https://github.com/como-technologies/riff/pull/40"}"#.to_owned(),
    ]
    .join("\n")
        + "\n"
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// Runs `git ARGS` in `dir`, with no config of the person, and gives
/// its stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", text(&out.stderr));
    text(&out.stdout).trim().to_owned()
}

/// A real riff-server of this build, with no sign-in.
async fn server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, riff_server::router()).await.unwrap() });
    format!("http://{addr}")
}

/// Each path in `dir`, below it, with the size of each file.
fn listing(dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut todo = vec![dir.to_owned()];
    while let Some(next) = todo.pop() {
        for entry in std::fs::read_dir(&next).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            let name = path.strip_prefix(dir).unwrap().display().to_string();
            if meta.is_dir() {
                out.insert(format!("{name}/"));
                todo.push(path);
            } else {
                out.insert(format!("{name} {}", meta.len()));
            }
        }
    }
    out
}

/// One machine: its environment, the fakes, the origin and the main
/// clone of the person.
struct Machine {
    env: Isolated,
    bin: tempfile::TempDir,
    _origin: tempfile::TempDir,
    main: PathBuf,
    server: String,
}

impl Machine {
    async fn new() -> Machine {
        let env = Isolated::new();
        let bin = tempfile::tempdir().unwrap();
        fake_gh(bin.path());
        fake_podman(bin.path());
        std::fs::write(bin.path().join("events"), events()).unwrap();

        // The origin: OWNER/REPO from its path, as on GitHub.
        let origin = tempfile::tempdir().unwrap();
        let bare = origin.path().join("como-technologies/riff.git");
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare", "-b", "main"]);
        let seed = origin.path().join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        git(&seed, &["init", "-q", "-b", "main"]);
        std::fs::write(seed.join("README.md"), "riff\n").unwrap();
        std::fs::write(
            seed.join("riff.toml"),
            format!("[session]\nimage = \"{PINNED}\"\n"),
        )
        .unwrap();
        git(&seed, &["add", "README.md", "riff.toml"]);
        git(&seed, &["commit", "-q", "-m", "one"]);
        git(&seed, &["push", "-q", bare.to_str().unwrap(), "main"]);
        let main = env.path().join("src/riff");
        std::fs::create_dir_all(main.parent().unwrap()).unwrap();
        git(
            env.path(),
            &[
                "clone",
                "-q",
                bare.to_str().unwrap(),
                main.to_str().unwrap(),
            ],
        );

        // The home of the person: a git config, the sign-in of claude
        // for 3 hours, and a gh config.
        let home = env.home();
        std::fs::write(home.join(".gitconfig"), "[user]\n\tname = Mike\n").unwrap();
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        Machine::sign_in(&home, 180);
        std::fs::create_dir_all(home.join(".config/gh")).unwrap();
        std::fs::write(home.join(".config/gh/hosts.yml"), "github.com:\n").unwrap();

        Machine {
            env,
            bin,
            _origin: origin,
            main,
            server: server().await,
        }
    }

    /// The sign-in of claude, for `minutes` from now.
    fn sign_in(home: &Path, minutes: i64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let file = serde_json::json!({"claudeAiOauth": {
            "accessToken": ACCESS,
            "refreshToken": REFRESH,
            "expiresAt": now + minutes * 60_000,
        }});
        std::fs::write(home.join(".claude/.credentials.json"), file.to_string()).unwrap();
    }

    /// `riff ARGS` in the main clone, with the fakes first in `PATH`. A
    /// `session` makes it a command of that session.
    fn command(&self, session: Option<&str>, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.bin.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(&self.main)
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("USER", "mike")
            .env("RIFF_HOST", "thelio")
            .env("PATH", path);
        if let Some(session) = session {
            cmd.env("RIFF_SESSION", session);
        }
        cmd
    }

    async fn run(&self, session: Option<&str>, args: &[&str]) -> Output {
        let mut cmd = self.command(session, args);
        tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap()
    }

    async fn ok(&self, session: Option<&str>, args: &[&str]) -> String {
        let out = self.run(session, args).await;
        assert!(out.status.success(), "riff {args:?}: {}", text(&out.stderr));
        text(&out.stdout)
    }

    /// A lead runs the riff, so a claim works.
    async fn lead(&self) {
        self.ok(Some("lead1"), &["lead"]).await;
        self.ok(Some("lead1"), &["resume", "--riff"]).await;
    }

    /// The one session folder of the item.
    fn folder(&self, item: &str) -> PathBuf {
        let sessions = self.env.riff_home().join("sessions");
        let found: Vec<PathBuf> = std::fs::read_dir(&sessions)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&format!("{item}-"))
            })
            .collect();
        assert_eq!(found.len(), 1, "{found:?}");
        found[0].clone()
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.bin.path().join(name)).unwrap_or_default()
    }
}

#[tokio::test]
async fn riff_run_gives_the_container_each_bind_and_value_and_writes_nothing_in_the_home() {
    let machine = Machine::new().await;
    machine.lead().await;
    let home = machine.env.home();
    let before = listing(&home);

    // With no --image: the image that riff.toml pins. podman does not
    // have it, so riff pulls it.
    std::fs::write(machine.bin.path().join("no-image"), "").unwrap();
    let out = machine.run(None, &["run", "issue-12"]).await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert_eq!(
        machine.read("podman.log"),
        format!("podman pull {PINNED}\n")
    );
    assert!(stdout.contains(&format!("riff run pulls the image {PINNED}.")));

    // The home of the person has no new file, and no file changed size.
    assert_eq!(listing(&home), before);

    // The arguments of podman: each bind and value, in order.
    let folder = machine.folder("issue-12");
    let clone = folder.join("clone");
    let objects = PathBuf::from(git(
        &machine.main,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    ))
    .join("objects");
    let args: Vec<String> = machine.read("run.args").lines().map(String::from).collect();
    let session = args[args.iter().position(|a| a == "--session-id").unwrap() + 1].clone();
    assert_eq!(session.len(), 36, "{session}");
    let short = &session[..8];
    assert!(folder.ends_with(format!("issue-12-{short}")), "{folder:?}");
    let (h, c, o) = (
        home.display().to_string(),
        clone.display().to_string(),
        objects.display().to_string(),
    );
    let claude = folder.join("claude").display().to_string();
    let want: Vec<String> = [
        "run",
        "--rm",
        "-i",
        "--name",
        &format!("riff-issue-12-{short}"),
        "--userns=keep-id",
        "--memory",
        "8g",
        "--cpus",
        "4",
        "--mount",
        &format!("type=tmpfs,destination={h},tmpfs-mode=0700,U=true"),
        "-v",
        &format!("{c}:{c}"),
        "-v",
        &format!("{o}:{o}:ro"),
        "-v",
        &format!("{h}/.gitconfig:{h}/.gitconfig:ro"),
        "-v",
        &format!("{h}/.config/gh:{h}/.config/gh:ro"),
        "-v",
        &format!("{claude}:{h}/.claude"),
        "-v",
        &format!("{h}/.claude/.credentials.json:{h}/.claude/.credentials.json:ro"),
        "-v",
        &format!("riff-cargo:{h}/.cargo"),
        "-v",
        &format!("riff-sccache:{h}/.cache/sccache"),
        "-e",
        &format!("HOME={h}"),
        "-e",
        "USER=mike",
        "-e",
        "LOGNAME=mike",
        "-e",
        "GH_TOKEN",
        "-e",
        "GIT_CONFIG_COUNT=1",
        "-e",
        "GIT_CONFIG_KEY_0=commit.gpgsign",
        "-e",
        "GIT_CONFIG_VALUE_0=false",
        "-e",
        &format!("CLAUDE_CONFIG_DIR={h}/.claude"),
        "-e",
        &format!("CARGO_HOME={h}/.cargo"),
        "-e",
        &format!("SCCACHE_DIR={h}/.cache/sccache"),
        "-e",
        "RUSTC_WRAPPER=sccache",
        "-e",
        "SCCACHE_IDLE_TIMEOUT=0",
        "-e",
        "CARGO_BUILD_JOBS=4",
        "-w",
        &c,
        PINNED,
        "claude",
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--session-id",
        &session,
        "--permission-mode",
        "auto",
    ]
    .map(String::from)
    .to_vec();
    assert_eq!(args, want);

    // The token of gh: in the environment of podman, in no argument.
    assert_eq!(machine.read("run.token"), GH_TOKEN);
    assert!(!args.iter().any(|a| a.contains(GH_TOKEN)));

    // The start text: one stream-json line, then stdin closes after the
    // result.
    let start: Value = serde_json::from_str(machine.read("run.stdin").trim()).unwrap();
    assert_eq!(start["type"], "user");
    let start = start["message"]["content"][0]["text"].as_str().unwrap();
    for part in [
        "Do the work item issue-12",
        "git push -u origin worktree-issue-12",
        &format!("Riff-Session: {session}"),
        "Closes #12\n\nSUMMARY\n\nIssue: #12\nMilestone: Wave 27\n",
        "gh pr merge NUMBER --auto --squash",
    ] {
        assert!(start.contains(part), "{part}\nnot in\n{start}");
    }
    assert!(machine.bin.path().join("run.closed").exists());

    // The terminal shows the events, with no secret.
    for part in [
        "You hold issue-12 in como-technologies/riff.",
        "start: model claude-opus-5-5, permission mode auto",
        "I read the issue.",
        "> Bash: gh issue view 12 --comments",
        "! the tool failed: token [secret] and [secret] leaked",
        "vcs_state_changed: action push, branch worktree-issue-12",
        "end: success, 3 turns, 5 s, 0.25 USD",
        "The pull request #40: https://github.com/como-technologies/riff/pull/40",
        &format!("The clone stays until the merge: {c}"),
    ] {
        assert!(stdout.contains(part), "{part}\nnot in\n{stdout}");
    }
    assert!(
        stderr.contains("a line on stderr with [secret]"),
        "{stderr}"
    );
    let log = std::fs::read_to_string(folder.join("events.jsonl")).unwrap();
    assert_eq!(log.lines().count(), 5, "{log}");
    for secret in [GH_TOKEN, ACCESS] {
        for (name, shown) in [("stdout", &stdout), ("stderr", &stderr), ("events", &log)] {
            assert!(!shown.contains(secret), "{name} has a secret:\n{shown}");
        }
    }

    // The clone: in the folder of riff, on the branch of the item, with
    // the objects of the main clone as its alternates.
    assert_eq!(
        git(&clone, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "worktree-issue-12"
    );
    let alternates = std::fs::read_to_string(clone.join(".git/objects/info/alternates")).unwrap();
    assert_eq!(alternates.trim(), o);
    let trust: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("claude/.claude.json")).unwrap())
            .unwrap();
    assert_eq!(trust["projects"][&c]["hasTrustDialogAccepted"], true);
    // claude adds no attribution line, so the trailers end the PR body
    // (01M4ZD48DHAKJ1EHX6QHYAQ7ZM).
    let settings: Value = serde_json::from_str(
        &std::fs::read_to_string(folder.join("claude/settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        settings,
        serde_json::json!({ "includeCoAuthoredBy": false })
    );

    // The lead got the verify request, and the item is free again.
    let thread = machine.ok(Some("lead1"), &["read", "--all"]).await;
    assert!(
        thread.contains(&format!(
            "verify request: issue-12, PR #40, branch worktree-issue-12, commit 1a2b3c4, from \
             riff run {short}."
        )),
        "{thread}"
    );
    machine.ok(Some("lead1"), &["claim", "issue-12"]).await;
}

#[tokio::test]
async fn riff_run_goes_on_from_the_branch_of_an_earlier_session() {
    let machine = Machine::new().await;
    machine.lead().await;
    // An earlier session pushed the branch of the item.
    git(&machine.main, &["switch", "-q", "-c", "worktree-issue-12"]);
    std::fs::write(machine.main.join("work.md"), "earlier\n").unwrap();
    git(&machine.main, &["add", "work.md"]);
    git(&machine.main, &["commit", "-q", "-m", "WIP: earlier"]);
    git(
        &machine.main,
        &["push", "-q", "origin", "worktree-issue-12"],
    );
    git(&machine.main, &["switch", "-q", "main"]);

    let out = machine
        .run(None, &["run", "issue-12", "--image", IMAGE])
        .await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let clone = machine.folder("issue-12").join("clone");
    assert_eq!(git(&clone, &["log", "-1", "--format=%s"]), "WIP: earlier");
    assert!(
        machine
            .read("run.stdin")
            .contains("work of an earlier session")
    );
}

#[tokio::test]
async fn a_failed_check_takes_no_claim_and_starts_no_container() {
    let machine = Machine::new().await;
    machine.lead().await;

    // podman does not have the image, and cannot pull it.
    let (no_image, no_pull) = (
        machine.bin.path().join("no-image"),
        machine.bin.path().join("no-pull"),
    );
    std::fs::write(&no_image, "").unwrap();
    std::fs::write(&no_pull, "").unwrap();
    let out = machine
        .run(None, &["run", "issue-12", "--image", IMAGE])
        .await;
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains(&format!(
            "The image {IMAGE} is not on this machine, and podman cannot pull it."
        )),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(machine.read("podman.log"), format!("podman pull {IMAGE}\n"));
    std::fs::remove_file(&no_image).unwrap();
    std::fs::remove_file(&no_pull).unwrap();

    // riff.toml names a tag: riff takes no image from it.
    let config = machine.main.join("riff.toml");
    let pinned = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("[session]\nimage = \"{IMAGE}\"\n")).unwrap();
    let out = machine.run(None, &["run", "issue-12"]).await;
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("names a tag"),
        "{}",
        text(&out.stderr)
    );
    std::fs::write(&config, pinned).unwrap();

    Machine::sign_in(&machine.env.home(), 30);
    let out = machine
        .run(None, &["run", "issue-12", "--image", IMAGE])
        .await;
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("riff run needs 60 minutes or more"),
        "{}",
        text(&out.stderr)
    );

    let out = machine.run(None, &["run", "verify-issue-12"]).await;
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("riff run takes an issue"));

    assert!(machine.read("run.args").is_empty());
    assert!(!machine.env.riff_home().join("sessions").exists());
    machine.ok(Some("lead1"), &["claim", "issue-12"]).await;
}

#[tokio::test]
async fn riff_run_starts_no_container_for_an_item_that_another_session_holds() {
    let machine = Machine::new().await;
    machine.lead().await;
    machine.ok(Some("other"), &["claim", "issue-12"]).await;

    let out = machine
        .run(None, &["run", "issue-12", "--image", IMAGE])
        .await;
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert!(machine.read("run.args").is_empty());
}

#[tokio::test]
async fn a_stop_of_riff_run_stops_the_container_and_releases_the_item() {
    let machine = Machine::new().await;
    machine.lead().await;
    // claude starts, and never ends its turn.
    std::fs::write(
        machine.bin.path().join("events"),
        r#"{"type":"system","subtype":"init","model":"m","permissionMode":"auto"}"#.to_owned()
            + "\n",
    )
    .unwrap();

    let mut cmd = machine.command(None, &["run", "issue-12", "--image", IMAGE]);
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !machine.bin.path().join("run.stdin").exists() {
        assert!(
            started.elapsed() < isolated::HANG,
            "podman run did not start"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let pid = nix::unistd::Pid::from_raw(child.id() as i32);
    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).unwrap();
    let out = tokio::task::spawn_blocking(move || child.wait_with_output().unwrap())
        .await
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));

    let folder = machine.folder("issue-12");
    let short = folder
        .file_name()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches("issue-12-")
        .to_owned();
    assert_eq!(
        machine.read("podman.log").trim(),
        format!("podman stop riff-issue-12-{short}")
    );
    assert!(
        text(&out.stdout).contains(&format!(
            "riff run stops the container riff-issue-12-{short}"
        )),
        "{}",
        text(&out.stdout)
    );
    machine.ok(Some("lead1"), &["claim", "issue-12"]).await;
}
