//! `riff run ITEM` (#712, #716, #717, 01M4ZKX0NXN7E33VC3PSFGSPDT): one
//! work item in one rootless podman container, through the riff daemon.
//!
//! A real `riff daemon` of this build runs the containers. A fake
//! `podman` logs its arguments, the value of its `GH_TOKEN` and of its
//! `INVOCATION_ID`, and plays the container on the host: it runs `riff
//! relay` of this build, with the socket of the session. The relay
//! starts a fake `claude`: it logs its stdin, prints the events of one
//! turn, and ends when its stdin closes. A fake `gh` gives
//! the token, the issue and the pull request. A real `riff-server` holds
//! the claims and the posts. The clone is real: git clones a bare origin
//! on disk.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use isolated::Isolated;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const GH_TOKEN: &str = "gho_FAKEtoken0123456789";
const ACCESS: &str = "sk-ant-oat01-FAKEaccess";
const REFRESH: &str = "sk-ant-ort01-FAKErefresh";
const IMAGE: &str = "localhost/riff-session:test";
/// The image that `riff.toml` of the repository pins.
const PINNED: &str = "ghcr.io/como-technologies/riff-session@sha256:\
                      0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Writes the program `path` with `body`.
fn script(path: &Path, body: &str) {
    isolated::write_program(path, body);
}

/// A fake `gh` that logs each call to `bin/gh.log`. The branch has no
/// pull request when `bin/no-pr` is there. The issue has the label
/// `no-verify` when `bin/no-verify` is there. `pr view` waits while
/// `bin/pr-hold` is there. For the look of a wave, the open milestone is
/// Wave 27, no pull request is open, and issue 12 is the open issue of
/// Wave 27 while `bin/wave-issue` is there.
fn fake_gh(bin: &Path) {
    let body = r#"#!/bin/sh
dir=$(dirname "$0")
echo "gh $*" >> "$dir/gh.log"
case "$*" in
  'auth token') echo GHTOKEN ;;
  'issue view 12 --json number,state,milestone')
    echo '{"number":12,"state":"OPEN","milestone":{"title":"Wave 27"}}' ;;
  'issue view 12 --json number,labels')
    if [ -f "$dir/no-verify" ]; then echo '{"number":12,"labels":[{"name":"no-verify"}]}'
    else echo '{"number":12,"labels":[]}'; fi ;;
  'pr view worktree-issue-12 --json number,url,headRefOid,state')
    while [ -f "$dir/pr-hold" ]; do sleep 0.05; done
    [ -f "$dir/no-pr" ] && exit 1
    echo '{"number":40,"url":"https://github.com/como-technologies/riff/pull/40","headRefOid":"1a2b3c4d5e6f","state":"OPEN"}' ;;
  'api repos/como-technologies/riff/milestones?state=open&per_page=100')
    echo '[{"title":"Wave 27"}]' ;;
  'pr list --repo como-technologies/riff '*) echo '[]' ;;
  'issue list --repo como-technologies/riff '*)
    if [ -f "$dir/wave-issue" ]; then
      echo '[{"number":12,"body":"","comments":[],"milestone":{"title":"Wave 27"},"labels":[]}]'
    else echo '[]'; fi ;;
  *) exit 1 ;;
esac
"#;
    script(&bin.join("gh"), &body.replace("GHTOKEN", GH_TOKEN));
}

/// A fake `podman`. `image exists` fails when `bin/no-image` is there.
/// `pull` logs, and fails when `bin/no-pull` is there. `run` logs, and
/// plays the container on the host: it runs the riff of the bind at
/// `/opt/riff/riff` as the entrypoint, `riff relay`, with the socket of
/// the bind at `/run/riff`. With `-d` (a terminal), it runs it in the
/// background, and `wait` waits for its end. `stop` logs, and ends the
/// `run`. `container inspect` says `true` while the `run` runs. `attach`
/// and `exec` log, and end at once. With `bin/exit-in-attach`, `attach`
/// first ends the `run`, as `/exit` in the lead does. A zombie counts as
/// ended. The `run`
/// with `-d` exits, so PID 1 becomes the parent of the relay. When PID 1
/// does not reap orphans, the relay stays a zombie after its end (#791).
fn fake_podman(bin: &Path) {
    let body = r#"#!/bin/sh
dir=$(dirname "$0")
# True while the process $1 runs. A zombie has ended.
alive() {
  kill -0 "$1" 2>/dev/null || return 1
  [ "$(sed 's/^.*) \(.\).*/\1/' "/proc/$1/stat" 2>/dev/null)" != Z ]
}
case "$1" in
  image) [ -f "$dir/no-image" ] && exit 1; exit 0 ;;
  pull) echo "podman $*" >> "$dir/podman.log"; [ -f "$dir/no-pull" ] && exit 125
    rm -f "$dir/no-image"; exit 0 ;;
  stop) echo "podman $*" >> "$dir/podman.log"; kill "$(cat "$dir/run.pid")"; exit 0 ;;
  attach|exec) echo "podman $*" >> "$dir/podman.log"
    # The person types /exit in the lead: its container ends.
    if [ "$1" = attach ] && [ -f "$dir/exit-in-attach" ]; then
      kill "$(cat "$dir/run.pid")"
      while alive "$(cat "$dir/run.pid")"; do sleep 0.05; done
    fi
    exit 0 ;;
  wait)
    pid=$(cat "$dir/run.pid")
    while alive "$pid"; do sleep 0.05; done
    echo 0; exit 0 ;;
  container)
    if [ -f "$dir/run.pid" ] && alive "$(cat "$dir/run.pid")"; then
      echo true
    else
      echo false
    fi
    exit 0 ;;
  run) ;;
  *) exit 2 ;;
esac
printf '%s\n' "$@" > "$dir/run.args"
printf '%s' "$GH_TOKEN" > "$dir/run.token"
printf '%s' "$INVOCATION_ID" > "$dir/run.invocation"
echo $$ > "$dir/run.pid"
shift
while [ $# -gt 0 ]; do
  case "$1" in
    -v)
      case "$2" in
        *:/opt/riff/riff:ro) relay="${2%:/opt/riff/riff:ro}" ;;
        *:/run/riff) socket="${2%:/run/riff}/riff.sock" ;;
      esac
      shift 2 ;;
    -d) detach=1; shift ;;
    -w) shift 2; break ;;
    *) shift ;;
  esac
done
# The image, then the entrypoint /opt/riff/riff.
shift 2
if [ -n "$detach" ]; then
  env RIFF_SOCKET="$socket" "$relay" "$@" < /dev/null > "$dir/terminal.out" 2>&1 &
  echo $! > "$dir/run.pid"
  exit 0
fi
exec env RIFF_SOCKET="$socket" "$relay" "$@"
"#;
    script(&bin.join("podman"), body);
}

/// A fake `claude` in the container. It logs its arguments, takes the
/// start text, waits while `bin/hold` is there, and prints
/// `bin/events`. When `bin/tools` is there, it first sends its lines to
/// the riff tools, through the `riff relay mcp` of its MCP config, and
/// logs the answers to `bin/mcp.out` until it has one for each request.
/// Then, for each of `bin/later` and `bin/last` that is there, it waits
/// for the file `.go` beside it, and prints it. It adds the rest of its
/// stdin to `bin/run.rest`, and ends when its stdin closes. With
/// `bin/linger`, it does not end then, as `claude -p` does not end while
/// a background task runs.
fn fake_claude(bin: &Path) {
    let body = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$@" > "$dir/claude.args"
IFS= read -r line
printf '%s\n' "$line" > "$dir/run.stdin"
while [ -f "$dir/hold" ]; do sleep 0.05; done
echo "a line on stderr with GHTOKEN" >&2
if [ -f "$dir/tools" ]; then
  for arg; do config=$arg; done
  riff=$(printf '%s' "$config" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')
  mkfifo "$dir/mcp.in"
  "$riff" relay mcp < "$dir/mcp.in" > "$dir/mcp.out" &
  exec 3> "$dir/mcp.in"
  cat "$dir/tools" >&3
  want=$(grep -c '"id"' "$dir/tools")
  while [ "$(grep -c '"id"' "$dir/mcp.out")" -lt "$want" ]; do sleep 0.05; done
  exec 3>&-
  wait
fi
cat "$dir/events"
for stage in later last; do
  if [ -f "$dir/$stage" ]; then
    while [ ! -f "$dir/$stage.go" ]; do sleep 0.05; done
    cat "$dir/$stage"
  fi
done
cat >> "$dir/run.rest"
touch "$dir/run.closed"
if [ -f "$dir/linger" ]; then
  while :; do sleep 0.05; done
fi
"#;
    script(&bin.join("claude"), &body.replace("GHTOKEN", GH_TOKEN));
}

/// A fake `systemctl --user` that logs each call to `bin/systemctl.log`.
/// `restart riff.service` starts `$FAKE_RIFF daemon` in the background,
/// as systemd does, and writes its pid to `bin/daemon.pid`. `disable
/// --now riff.service` stops it with a SIGTERM.
/// `is-active riff.service` says `active` while that daemon runs.
pub fn fake_systemctl(bin: &Path) {
    let body = r#"#!/bin/sh
dir=$(dirname "$0")
echo "systemctl $*" >> "$dir/systemctl.log"
case "$*" in
  '--user is-active riff.service')
    if [ -f "$dir/daemon.pid" ] && kill -0 "$(cat "$dir/daemon.pid")" 2>/dev/null; then
      echo active; exit 0
    fi
    echo inactive; exit 3 ;;
  '--user disable --now riff.service')
    [ -f "$dir/daemon.pid" ] && kill "$(cat "$dir/daemon.pid")" 2>/dev/null ;;
  '--user restart riff.service')
    "$FAKE_RIFF" daemon >> "$dir/daemon.out" 2>&1 &
    echo $! > "$dir/daemon.pid" ;;
esac
exit 0
"#;
    script(&bin.join("systemctl"), body);
}

/// Waits until the text of `file` holds `part`.
pub async fn wait_for(file: &Path, part: &str) {
    let started = Instant::now();
    while !std::fs::read_to_string(file)
        .unwrap_or_default()
        .contains(part)
    {
        assert!(
            started.elapsed() < isolated::HANG,
            "{} has no {part:?}",
            file.display()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Asks the socket `riff.sock` in the folder `dir` of a session who it
/// is: one line of JSON. It connects through a descriptor of the folder,
/// as riff does, so a long path works.
fn who_is(dir: &Path) -> Value {
    use std::io::{BufRead, Write};
    use std::os::fd::AsRawFd;
    let folder = std::fs::File::open(dir).unwrap();
    let path = format!("/proc/self/fd/{}/riff.sock", folder.as_raw_fd());
    let mut conn = std::os::unix::net::UnixStream::connect(path).unwrap();
    // A relay of the protocol before this one: the daemon takes it.
    let hello = format!(
        "{{\"ask\":\"hello\",\"protocol\":{}}}\n",
        riff::daemon::PROTOCOL - 1
    );
    conn.write_all(hello.as_bytes()).unwrap();
    let mut line = String::new();
    std::io::BufReader::new(conn).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
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
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
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

/// One machine: its environment, the fakes, the origin, the main clone
/// of the person, and its riff daemon.
struct Machine {
    env: Isolated,
    bin: tempfile::TempDir,
    _origin: tempfile::TempDir,
    main: PathBuf,
    server: String,
    daemon: Option<std::process::Child>,
    /// The quiet limit of the daemon, when the test sets one.
    quiet: Option<Duration>,
}

impl Drop for Machine {
    fn drop(&mut self) {
        if let Some(mut daemon) = self.daemon.take() {
            let _ = daemon.kill();
            let _ = daemon.wait();
        }
    }
}

impl Machine {
    async fn new() -> Machine {
        Machine::with_quiet(None).await
    }

    /// A machine whose daemon has the quiet limit `quiet`.
    async fn with_quiet(quiet: Option<Duration>) -> Machine {
        Machine::with_server(quiet, server().await).await
    }

    /// A machine with no riff-server: `RIFF_SERVER` names a port where
    /// nothing listens.
    async fn with_no_server() -> Machine {
        Machine::with_server(None, NO_SERVER.into()).await
    }

    /// A machine whose daemon has the quiet limit `quiet`, with the
    /// riff-server `server`.
    async fn with_server(quiet: Option<Duration>, server: String) -> Machine {
        let env = Isolated::new();
        let bin = tempfile::tempdir().unwrap();
        fake_gh(bin.path());
        fake_podman(bin.path());
        fake_claude(bin.path());
        fake_systemctl(bin.path());
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

        let mut machine = Machine {
            env,
            bin,
            _origin: origin,
            main,
            server,
            daemon: None,
            quiet,
        };
        machine.start_daemon().await;
        machine
    }

    /// Starts `riff daemon` of this machine as systemd does: with an
    /// `INVOCATION_ID`. Waits until it listens.
    async fn start_daemon(&mut self) {
        // The free memory of a test host is no matter: no floor.
        self.ok(None, &["daemon", "limit", "--floor", "0"]).await;
        let log = self.bin.path().join("daemon.out");
        let out = std::fs::File::create(&log).unwrap();
        let mut daemon = self.command(None, &["daemon"]);
        if let Some(quiet) = self.quiet {
            daemon.env(riff::daemon::QUIET_VAR, quiet.as_millis().to_string());
        }
        let daemon = daemon
            .env("INVOCATION_ID", "riff-test-unit")
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .unwrap();
        self.daemon = Some(daemon);
        wait_for(&log, "The riff daemon listens on").await;
    }

    /// Stops the daemon with a SIGTERM, as systemd does, and gives its
    /// exit.
    async fn stop_daemon(&mut self) -> std::process::ExitStatus {
        let mut daemon = self.daemon.take().unwrap();
        let pid = nix::unistd::Pid::from_raw(daemon.id() as i32);
        nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).unwrap();
        tokio::task::spawn_blocking(move || daemon.wait().unwrap())
            .await
            .unwrap()
    }

    /// The folder of the socket of the session in the container `name`.
    fn socket_dir(&self, name: &str) -> PathBuf {
        self.env.riff_home().join("daemon/sessions").join(name)
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
    std::fs::write(home.join(".claude.json"), r#"{"theme":"dark"}"#).unwrap();
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
    assert!(stdout.contains(&format!("The riff daemon pulls the image {PINNED}.")));

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
    let name = format!("riff-issue-12-{short}");
    let socket = machine.socket_dir(&name).display().to_string();
    let riff = machine.env.riff_path().display().to_string();
    let want: Vec<String> = [
        "run",
        "--rm",
        "-i",
        "--sig-proxy=false",
        "--name",
        &format!("riff-issue-12-{short}"),
        "--userns=keep-id",
        "--init",
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
        "-v",
        &format!("{socket}:/run/riff"),
        "-v",
        &format!("{riff}:/opt/riff/riff:ro"),
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
        "-e",
        "RIFF_SOCKET=/run/riff/riff.sock",
        "-w",
        &c,
        PINNED,
        "/opt/riff/riff",
        "relay",
        "--",
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

    // The relay starts claude with the riff tools: the MCP server riff
    // runs riff relay mcp, with the binary of the relay.
    let claude_args: Vec<String> = machine
        .read("claude.args")
        .lines()
        .map(String::from)
        .collect();
    assert_eq!(claude_args[..10], want[want.len() - 10..]);
    assert_eq!(claude_args[10], "--mcp-config");
    let config: Value = serde_json::from_str(&claude_args[11]).unwrap();
    assert_eq!(
        config,
        serde_json::json!({"mcpServers": {"riff": {
            "type": "stdio",
            "command": riff,
            "args": ["relay", "mcp"],
        }}})
    );

    // The token of gh: in the environment of podman, in no argument.
    assert_eq!(machine.read("run.token"), GH_TOKEN);
    assert!(!args.iter().any(|a| a.contains(GH_TOKEN)));
    // The daemon runs podman with no INVOCATION_ID of systemd, so
    // conmon is not in the cgroup of the daemon.
    assert_eq!(machine.read("run.invocation"), "");
    // The socket folder of the session goes at its end.
    assert!(!machine.socket_dir(&name).exists());

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
        "gh pr merge --auto --squash NUMBER",
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
    let daemon = machine.read("daemon.out");
    for secret in [GH_TOKEN, ACCESS] {
        for (name, shown) in [
            ("stdout", &stdout),
            ("stderr", &stderr),
            ("events", &log),
            ("the daemon", &daemon),
        ] {
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
    // The first-run answers of claude, so a takeover asks no question
    // (01M51JACH6TQP318ZFG6NQ9GRM).
    let answers: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("claude/.claude.json")).unwrap())
            .unwrap();
    assert_eq!(answers["hasCompletedOnboarding"], true);
    assert_eq!(answers["theme"], "dark");
    assert_eq!(answers["projects"][&c]["hasTrustDialogAccepted"], true);
    // claude adds no attribution line, so the trailers end the PR body
    // (01M4ZD48DHAKJ1EHX6QHYAQ7ZM).
    let settings: Value = serde_json::from_str(
        &std::fs::read_to_string(folder.join("claude/settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["includeCoAuthoredBy"], false);
    // The rules of the session place, so the auto mode check does not
    // refuse a step of the start text (01M5084K4YQCZF20XA0P2485SG).
    assert_eq!(
        settings["permissions"]["allow"],
        serde_json::json!(riff::permissions::session_rules().allow)
    );
    // No dialog of auto mode, and the places of riff work, with the
    // riff server of the session (01M516HEX3GVTN9K2QS12A4PG3).
    assert_eq!(settings["skillOverrides"]["auto-mode-setup"], "off");
    let environment = settings["autoMode"]["environment"].as_array().unwrap();
    assert_eq!(environment[0], "$defaults");
    assert!(
        environment.iter().any(|e| e
            .as_str()
            .unwrap()
            .starts_with("Key internal services: the riff server at http")),
        "{environment:?}"
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

/// A lead holds the verify of the item while the session works. When
/// `claude` ends, `riff run` sends no verify request, and shows the hold
/// (01M5157TYDEHZHDXSC1AHCRDRN). It still releases the item, and ends
/// with 0.
#[tokio::test]
async fn riff_run_sends_no_verify_request_when_a_lead_holds_the_verify() {
    let machine = Machine::new().await;
    machine.lead().await;
    let hold = machine.bin.path().join("hold");
    std::fs::write(&hold, "").unwrap();
    let child = machine
        .command(None, &["run", "issue-12", "--image", IMAGE])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(&machine.bin.path().join("run.stdin"), "user").await;
    machine
        .ok(
            Some("lead1"),
            &["plan", "hold", "verify-issue-12", "a scratch item"],
        )
        .await;
    std::fs::remove_file(&hold).unwrap();

    let out = output_of(child).await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("The pull request #40"), "{stdout}");
    assert!(
        stdout.contains(
            "riff run sends no verify request for issue-12: verify-issue-12 is held by the lead \
             (the session mike/lead1) since "
        ),
        "{stdout}"
    );
    assert!(stdout.contains(": a scratch item\n"), "{stdout}");
    let thread = machine.ok(Some("lead1"), &["read", "--all"]).await;
    assert!(!thread.contains("verify request: issue-12"), "{thread}");
    machine.ok(Some("lead1"), &["claim", "issue-12"]).await;
}

/// The issue has the label `no-verify`, as the scratch item of the
/// real-session test has. `riff run` sends no verify request
/// (01M5157V0P92EBHF4M7N9K0RGR), also with no hold: a worker that runs
/// the test cannot hold the verify.
#[tokio::test]
async fn riff_run_sends_no_verify_request_for_an_issue_with_the_label_no_verify() {
    let machine = Machine::new().await;
    machine.lead().await;
    std::fs::write(machine.bin.path().join("no-verify"), "").unwrap();

    let out = machine
        .run(None, &["run", "issue-12", "--image", IMAGE])
        .await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("The pull request #40"), "{stdout}");
    assert!(
        stdout.contains(&riff::text::run_no_verify(
            "issue-12",
            &riff::text::no_verify_label(12)
        )),
        "{stdout}"
    );
    let log = machine.read("gh.log");
    assert!(
        log.contains("gh issue view 12 --json number,labels"),
        "{log}"
    );
    let thread = machine.ok(Some("lead1"), &["read", "--all"]).await;
    assert!(!thread.contains("verify request: issue-12"), "{thread}");
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

/// 01M51STPHXSSZMHF9T7Z85097R, 01M51STPMEM62Z8B26YM2CH6QH,
/// 01M51SVXQK037E20R7WAY66532: the main clone pins another image in its
/// working tree, and origin has the merged pin. `riff wave run`, the
/// image step of `riff update --check`, `riff run` and `riff lead start`
/// each use the merged pin, and say in one line that the local pin
/// differs. Only the image of this folder in `riff update --check` reads
/// the working tree. A tag in the working tree stops nothing.
#[tokio::test]
async fn only_the_merged_pin_counts_and_a_local_pin_gives_one_line() {
    let mut machine = Machine::new().await;
    let local = format!(
        "ghcr.io/como-technologies/riff-session@sha256:{}",
        "f".repeat(64)
    );
    let config = machine.main.join("riff.toml");
    std::fs::write(&config, format!("[session]\nimage = \"{local}\"\n")).unwrap();
    let differs = riff::text::image_local_differs(&config, &local, PINNED, "origin/main");
    let says = |out: &str, line: &str| {
        assert!(out.lines().any(|l| l == line), "{line}\nnot in\n{out}");
        assert_eq!(out.matches(line).count(), 1, "{out}");
    };

    // riff wave run: the riff is paused, so the wave starts nothing.
    let out = machine.ok(None, &["wave", "run"]).await;
    says(&out, &differs);

    // The update step of the wave: the merged pin. Only the image of this
    // folder is the pin of the working tree.
    std::fs::write(machine.bin.path().join("no-image"), "").unwrap();
    let out = machine.run(None, &["update", "--check"]).await;
    let stdout = text(&out.stdout);
    says(&stdout, &format!("como-technologies/riff: {differs}"));
    for line in [
        format!(
            "not in step  the session image of como-technologies/riff: podman does not have \
             {PINNED}. Fix: podman pull {PINNED}"
        ),
        format!(
            "not in step  the session image of this folder: podman does not have {local}. Fix: \
             podman pull {local}"
        ),
    ] {
        says(&stdout, &line);
    }
    // The update step at the start of the daemon: the same line in its
    // log.
    machine.stop_daemon().await;
    machine.start_daemon().await;
    wait_for(
        &machine.bin.path().join("daemon.out"),
        &format!("como-technologies/riff: {differs}"),
    )
    .await;
    machine.ok(None, &["wave", "stop"]).await;

    // riff run pulls and runs the merged pin.
    machine.lead().await;
    let out = machine.run(None, &["run", "issue-12"]).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    says(&stdout, &differs);
    assert_eq!(
        machine.read("podman.log"),
        format!("podman pull {PINNED}\n")
    );
    let args = machine.read("run.args");
    assert!(args.lines().any(|a| a == PINNED), "{args}");
    assert!(!args.contains(&local), "{args}");

    // A tag in the working tree: riff says why, and runs the merged pin.
    std::fs::write(&config, format!("[session]\nimage = \"{IMAGE}\"\n")).unwrap();
    std::fs::write(machine.bin.path().join("hold"), "").unwrap();
    std::fs::write(machine.bin.path().join("no-image"), "").unwrap();
    let out = machine.ok(None, &["lead", "start"]).await;
    let line = out
        .lines()
        .find(|l| l.contains("names a tag"))
        .unwrap_or_else(|| panic!("no line of the tag in\n{out}"));
    assert!(
        line.ends_with(&format!(
            "riff uses the merged pin {PINNED} of origin/main."
        )),
        "{line}"
    );
    assert!(out.contains(&riff::text::run_pulling(PINNED)), "{out}");
    let args = machine.read("run.args");
    assert!(args.lines().any(|a| a == PINNED), "{args}");
    let name = "riff-lead-como-technologies-riff";
    std::fs::remove_file(machine.bin.path().join("hold")).unwrap();
    let started = Instant::now();
    while machine.ok(None, &["daemon", "status"]).await.contains(name) {
        assert!(started.elapsed() < isolated::HANG, "the lead did not end");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
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

/// Starts `riff run issue-12` with a `claude` that starts and never ends
/// its turn. Gives the `riff run`, the name of the container and the
/// session ID, when the container has the start text.
async fn hanging_run(machine: &Machine) -> (std::process::Child, String, String) {
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
    wait_for(&machine.bin.path().join("run.stdin"), "user").await;
    let args = machine.read("run.args");
    let args: Vec<&str> = args.lines().collect();
    let session = args[args.iter().position(|a| *a == "--session-id").unwrap() + 1].to_owned();
    (child, format!("riff-issue-12-{}", &session[..8]), session)
}

/// Waits for the end of `child`, and gives its output.
async fn output_of(child: std::process::Child) -> Output {
    tokio::task::spawn_blocking(move || child.wait_with_output().unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_stop_of_riff_run_stops_the_container_and_releases_the_item() {
    let machine = Machine::new().await;
    machine.lead().await;
    let (child, name, session) = hanging_run(&machine).await;

    // While the session runs: the daemon shows it, and its socket tells
    // which session asks.
    let status = machine.ok(None, &["daemon", "status"]).await;
    assert!(
        status.contains(&format!("  {name}  issue-12  session {session}  for ")),
        "{status}"
    );
    let who = who_is(&machine.socket_dir(&name));
    assert_eq!(who["say"], "session", "{who}");
    assert_eq!(who["session"], session.as_str());
    assert_eq!(who["item"], "issue-12");
    assert_eq!(who["name"], name.as_str());

    let pid = nix::unistd::Pid::from_raw(child.id() as i32);
    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).unwrap();
    let out = output_of(child).await;
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert_eq!(
        machine.read("podman.log").trim(),
        format!("podman stop {name}")
    );
    assert!(
        text(&out.stdout).contains(&format!("riff run stops the container {name}")),
        "{}",
        text(&out.stdout)
    );
    assert!(!machine.socket_dir(&name).exists());
    machine.ok(Some("lead1"), &["claim", "issue-12"]).await;
}

/// An event of `claude`: the background task `wait for just check`
/// runs.
const ONE_TASK: &str = r#"{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"b1","task_type":"local_bash","description":"wait for just check"}]}"#;

/// The result of the first turn of a fake `claude`.
const RESULT: &str = r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":5000,"total_cost_usd":0.25}"#;

/// The name of the container of the session of the last `riff run`.
fn container(machine: &Machine) -> String {
    let args = machine.read("run.args");
    let args: Vec<&str> = args.lines().collect();
    let session = args[args.iter().position(|a| *a == "--session-id").unwrap() + 1];
    format!("riff-issue-12-{}", &session[..8])
}

#[tokio::test]
async fn a_session_that_waits_for_a_background_task_stops_at_the_quiet_limit() {
    let quiet = Duration::from_secs(1);
    let machine = Machine::with_quiet(Some(quiet)).await;
    machine.lead().await;
    // claude ends its turn, and a background task runs on. claude does
    // not end when its input closes, and gives no event.
    let init = r#"{"type":"system","subtype":"init","model":"m","permissionMode":"auto"}"#;
    std::fs::write(
        machine.bin.path().join("events"),
        [init, RESULT, ONE_TASK].join("\n") + "\n",
    )
    .unwrap();
    std::fs::write(machine.bin.path().join("linger"), "").unwrap();

    let out = machine
        .run(None, &["run", "issue-12", "--image", IMAGE])
        .await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    // The daemon stopped the session: riff run exits with 1.
    assert_eq!(out.status.code(), Some(1), "{stdout}\n{stderr}");
    let name = container(&machine);
    let quiet_line = riff::text::daemon_quiet(&name, quiet);
    for part in [
        "end: success, 1 turns, 5 s, 0.25 USD",
        "wait: claude waits for 1 background task: wait for just check",
        &quiet_line,
        "The pull request #40",
    ] {
        assert!(stdout.contains(part), "{part}\nnot in\n{stdout}");
    }
    assert!(machine.read("daemon.out").contains(&quiet_line));
    assert_eq!(
        machine.read("podman.log").trim(),
        format!("podman stop {name}")
    );

    // riff run sent the verify request, and released the item.
    let thread = machine.ok(Some("lead1"), &["read", "--all"]).await;
    assert!(
        thread.contains("verify request: issue-12, PR #40, branch worktree-issue-12"),
        "{thread}"
    );
    machine.ok(Some("lead1"), &["claim", "issue-12"]).await;
}

#[tokio::test]
async fn a_new_turn_after_a_background_task_is_no_quiet_and_riff_run_shows_it() {
    let quiet = Duration::from_secs(3);
    let machine = Machine::with_quiet(Some(quiet)).await;
    machine.lead().await;
    // The first turn ends while a background task runs. The end of the
    // task starts a new turn of claude.
    let init = r#"{"type":"system","subtype":"init","model":"m","permissionMode":"auto"}"#;
    std::fs::write(
        machine.bin.path().join("events"),
        [init, ONE_TASK, RESULT].join("\n") + "\n",
    )
    .unwrap();
    let later = [
        r#"{"type":"system","subtype":"task_notification","task_id":"b1","status":"completed","summary":"wait for just check"}"#,
        r#"{"type":"system","subtype":"background_tasks_changed","tasks":[]}"#,
        init,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"The check passed."}]}}"#,
        r#"{"type":"result","subtype":"success","is_error":false,"num_turns":2,"duration_ms":7000,"total_cost_usd":0.3}"#,
    ];
    std::fs::write(machine.bin.path().join("later"), later.join("\n") + "\n").unwrap();

    let child = machine
        .command(None, &["run", "issue-12", "--image", IMAGE])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(&machine.bin.path().join("run.stdin"), "user").await;
    let log = machine.folder("issue-12").join("events.jsonl");
    wait_for(&log, "\"result\"").await;
    // The task ends within the quiet limit.
    tokio::time::sleep(quiet / 3).await;
    std::fs::write(machine.bin.path().join("later.go"), "").unwrap();

    let out = output_of(child).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    for part in [
        "end: success, 1 turns, 5 s, 0.25 USD\nwait: claude waits for 1 background task: wait for just check\n",
        "wait: each background task ended",
        "The check passed.",
        "end: success, 2 turns, 7 s, 0.30 USD",
        "The pull request #40",
    ] {
        assert!(stdout.contains(part), "{part}\nnot in\n{stdout}");
    }
    // No stop.
    assert_eq!(machine.read("podman.log"), "");
    assert!(!machine.read("daemon.out").contains("gave no event"));
    machine.ok(Some("lead1"), &["claim", "issue-12"]).await;
}

#[tokio::test]
async fn a_restart_of_the_daemon_loses_no_event_and_riff_run_goes_on() {
    let mut machine = Machine::new().await;
    machine.lead().await;
    // claude prints its first event, then two events while the daemon is
    // away, then the rest when the test says so.
    let all = events();
    let lines: Vec<&str> = all.lines().collect();
    std::fs::write(
        machine.bin.path().join("later"),
        lines[1..3].join("\n") + "\n",
    )
    .unwrap();
    std::fs::write(
        machine.bin.path().join("last"),
        lines[3..].join("\n") + "\n",
    )
    .unwrap();
    let (child, name, _) = hanging_run(&machine).await;
    let log = machine.folder("issue-12").join("events.jsonl");
    wait_for(&log, "\"init\"").await;

    // systemd stops the unit of the daemon with a SIGTERM.
    let stopped = machine.stop_daemon().await;
    assert!(stopped.success(), "{stopped}");
    assert!(
        machine
            .read("daemon.out")
            .contains("It stopped no session: each container runs on."),
        "{}",
        machine.read("daemon.out")
    );
    // No podman stop. The podman run of the daemon passes no signal to
    // the container, and has no INVOCATION_ID: conmon is not in the
    // cgroup of the unit.
    assert_eq!(machine.read("podman.log"), "");
    assert!(
        machine
            .read("run.args")
            .lines()
            .any(|a| a == "--sig-proxy=false")
    );
    assert_eq!(machine.read("run.invocation"), "");

    // claude goes on while the daemon is away: the relay keeps its
    // events.
    std::fs::write(machine.bin.path().join("later.go"), "").unwrap();

    // A new daemon takes up the session. The relay sends it the kept
    // events, and riff run follows the session again.
    machine.start_daemon().await;
    let taken = format!("The riff daemon takes up the session in the container {name}");
    assert!(
        machine.read("daemon.out").contains(&taken),
        "{}",
        machine.read("daemon.out")
    );
    wait_for(&log, "I read the issue.").await;

    // riff daemon events shows the session from its start.
    let shown = machine.bin.path().join("events.out");
    let view = machine
        .command(None, &["daemon", "events", &name])
        .stdout(std::fs::File::create(&shown).unwrap())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(&shown, "> Bash: gh issue view 12 --comments").await;
    std::fs::write(machine.bin.path().join("last.go"), "").unwrap();

    // riff run ends as with no restart: the verify request, the release.
    let out = output_of(child).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    for part in [
        format!("The riff daemon went away. The container {name} runs on."),
        format!("The riff daemon is back. riff run follows {name} again"),
        "The pull request #40".to_owned(),
    ] {
        assert!(stdout.contains(&part), "{part}\nnot in\n{stdout}");
    }
    // Each event one time, on the terminal and in the log, with no
    // secret.
    // The first event is the one of hanging_run.
    let parts = [
        "start: model m, permission mode auto",
        "I read the issue.",
        "> Bash: gh issue view 12 --comments",
        "! the tool failed: token [secret] and [secret] leaked",
        "vcs_state_changed: action push, branch worktree-issue-12",
        "end: success, 3 turns, 5 s, 0.25 USD",
    ];
    for part in parts {
        assert_eq!(stdout.matches(part).count(), 1, "{part}\nin\n{stdout}");
    }
    let logged = std::fs::read_to_string(&log).unwrap();
    assert_eq!(logged.lines().count(), 5, "{logged}");
    assert!(
        !logged.contains(GH_TOKEN) && !logged.contains(ACCESS),
        "{logged}"
    );

    // The view showed each event from the start, and the end.
    let view = output_of(view).await;
    assert!(view.status.success(), "{}", text(&view.stderr));
    let viewed = std::fs::read_to_string(&shown).unwrap();
    for part in parts {
        assert_eq!(viewed.matches(part).count(), 1, "{part}\nin\n{viewed}");
    }
    assert!(
        viewed.contains(&format!("The container {name} ended with the exit code 0.")),
        "{viewed}"
    );
    machine.ok(Some("lead1"), &["claim", "issue-12"]).await;
}

#[tokio::test]
async fn a_session_uses_the_riff_tools_through_the_relay() {
    let machine = Machine::new().await;
    machine.lead().await;
    // claude calls whoami and post through the MCP server of its relay.
    // claim, release, leave and move act on the host: they refuse
    // (01M4ZV3B6T2KB09WJ59F07HXET).
    let host = machine.bin.path().display().to_string();
    let calls = [
        r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"claude-code","version":"2"}}}"#.to_owned(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_owned(),
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"whoami","arguments":{}}}"#.to_owned(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"post","arguments":{"body":"hello from the container"}}}"#.to_owned(),
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"claim","arguments":{"item":"issue-13"}}}"#.to_owned(),
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"release","arguments":{"item":"issue-12"}}}"#.to_owned(),
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"leave","arguments":{}}}"#.to_owned(),
        format!(r#"{{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{{"name":"move","arguments":{{"path":"{host}"}}}}}}"#),
        r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"whoami","arguments":{}}}"#.to_owned(),
    ];
    std::fs::write(machine.bin.path().join("tools"), calls.join("\n") + "\n").unwrap();

    let out = machine
        .run(None, &["run", "issue-12", "--image", IMAGE])
        .await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let args = machine.read("run.args");
    let args: Vec<&str> = args.lines().collect();
    let session = args[args.iter().position(|a| *a == "--session-id").unwrap() + 1];

    // The tools act as the session of riff run, which holds the item.
    let answers: Vec<Value> = machine
        .read("mcp.out")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let answer = |id: u64| {
        answers
            .iter()
            .find(|a| a["id"] == id)
            .unwrap_or_else(|| panic!("no answer {id} in {answers:?}"))
            .clone()
    };
    let text = |id: u64| {
        answer(id)["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let whoami = text(1);
    assert!(whoami.contains(&format!("session={session}")), "{whoami}");
    assert!(whoami.contains("claim=issue-12"), "{whoami}");
    assert_ne!(answer(2)["result"]["isError"], true, "{answers:?}");
    let thread = machine.ok(Some("lead1"), &["read", "--all"]).await;
    assert!(thread.contains("hello from the container"), "{thread}");

    for (id, tool) in [(3, "claim"), (4, "release"), (5, "leave"), (6, "move")] {
        assert_eq!(answer(id)["result"]["isError"], true, "{answers:?}");
        assert_eq!(text(id), riff::text::not_in_a_container(tool));
    }
    // The claim and the place of the session stay. riff run releases
    // the item at its end.
    assert_eq!(text(7), whoami);
    machine.ok(Some("lead1"), &["claim", "issue-12"]).await;
}

/// A TCP proxy in front of the riff-server of a test. While it is away,
/// it reads each request, gives no reply and keeps the connection open,
/// as a riff-server while its new instance starts. A connection that
/// got a request while the proxy was away stays silent. Each other
/// connection goes to the server.
struct Proxy {
    url: String,
    away: Arc<AtomicBool>,
    /// The request line of each request that got no reply, for example
    /// `POST /v1/post HTTP/1.1`.
    held: Held,
}

/// See [`Proxy::held`].
type Held = Arc<Mutex<Vec<String>>>;

impl Proxy {
    async fn new(server: &str) -> Proxy {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let target = server.trim_start_matches("http://").to_owned();
        let away = Arc::new(AtomicBool::new(false));
        let held = Held::default();
        let (a, h) = (away.clone(), held.clone());
        tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                tokio::spawn(pass(client, target.clone(), a.clone(), h.clone()));
            }
        });
        Proxy { url, away, held }
    }

    fn away(&self, away: bool) {
        self.away.store(away, Ordering::SeqCst);
    }

    fn held(&self) -> usize {
        self.held.lock().unwrap().len()
    }

    /// True when a request to `path` got no reply.
    fn holds(&self, path: &str) -> bool {
        let line = format!(" {path} ");
        self.held.lock().unwrap().iter().any(|l| l.contains(&line))
    }
}

/// Passes the bytes of one connection between `client` and the server
/// at `target`, until a request comes while the proxy is away. Then it
/// drops each byte of the client until the client closes.
async fn pass(client: TcpStream, target: String, away: Arc<AtomicBool>, held: Held) {
    let Ok(server) = TcpStream::connect(&target).await else {
        return;
    };
    let (mut from_client, mut to_client) = client.into_split();
    let (mut from_server, mut to_server) = server.into_split();
    let back = tokio::spawn(async move {
        let _ = tokio::io::copy(&mut from_server, &mut to_client).await;
    });
    let mut chunk = [0; 8192];
    while let Ok(n) = from_client.read(&mut chunk).await
        && n > 0
    {
        if away.load(Ordering::SeqCst) {
            let request = String::from_utf8_lossy(&chunk[..n]);
            let line = request.lines().next().unwrap_or_default().to_owned();
            held.lock().unwrap().push(line);
            while matches!(from_client.read(&mut chunk).await, Ok(n) if n > 0) {}
            break;
        }
        if to_server.write_all(&chunk[..n]).await.is_err() {
            break;
        }
    }
    back.abort();
}

/// riff-server stops answering at the end of a run, as while a new
/// instance starts (#740, 01M4ZQVVGQYXTAVRB69C7YPC3Y). The verify
/// request gets no reply, and its connection stays open. `riff run`
/// tries again past the short budget, and sends the verify request and
/// the release when the server answers again.
#[tokio::test]
async fn riff_run_sends_the_verify_request_and_the_release_when_the_server_answers_again() {
    let machine = Machine::new().await;
    machine.lead().await;
    let proxy = Proxy::new(&machine.server).await;
    let hold = machine.bin.path().join("hold");
    std::fs::write(&hold, "").unwrap();
    // A short budget of 1 s, and a try of 300 ms: a short call fails
    // while the server is away.
    let child = machine
        .command(None, &["run", "issue-12", "--image", IMAGE])
        .env("RIFF_SERVER", &proxy.url)
        .env(riff::link::BUDGET_VAR, "1000")
        .env(riff::link::TRY_VAR, "300")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(&machine.bin.path().join("run.stdin"), "user").await;

    // The server goes away. Then claude ends its turn.
    proxy.away(true);
    let away = Instant::now();
    std::fs::remove_file(&hold).unwrap();
    while proxy.held() < 2 || away.elapsed() < Duration::from_secs(3) {
        assert!(away.elapsed() < isolated::HANG, "{} held", proxy.held());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Three short budgets later: no verify request yet, and the item is
    // still held.
    let thread = machine.ok(Some("lead1"), &["read", "--all"]).await;
    assert!(!thread.contains("verify request: issue-12"), "{thread}");
    still_held(&machine).await;
    proxy.away(false);

    let out = output_of(child).await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("The pull request #40"), "{stdout}");
    let thread = machine.ok(Some("lead1"), &["read", "--all"]).await;
    assert_eq!(
        thread
            .matches("verify request: issue-12, PR #40, branch worktree-issue-12")
            .count(),
        1,
        "{thread}"
    );
    machine.ok(Some("lead1"), &["claim", "issue-12"]).await;
}

/// riff-server stops answering at the end of a run. `riff run` gets
/// `signal` while its first end step, a call to `path`, waits (#756,
/// 01M4ZYFJFTX9ZEX7QT4QH7E9T6). Gives the output of `riff run` and the
/// time from the signal to its end.
async fn a_signal_in_the_end_steps(
    machine: &Machine,
    path: &str,
    signal: nix::sys::signal::Signal,
) -> (Output, Duration) {
    machine.lead().await;
    let proxy = Proxy::new(&machine.server).await;
    let hold = machine.bin.path().join("hold");
    std::fs::write(&hold, "").unwrap();
    let child = machine
        .command(None, &["run", "issue-12", "--image", IMAGE])
        .env("RIFF_SERVER", &proxy.url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(&machine.bin.path().join("run.stdin"), "user").await;

    // The server goes away. Then claude ends its turn, and the first end
    // step waits for the server.
    proxy.away(true);
    let away = Instant::now();
    std::fs::remove_file(&hold).unwrap();
    while !proxy.holds(path) {
        assert!(away.elapsed() < isolated::HANG, "{} held", proxy.held());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let pid = nix::unistd::Pid::from_raw(child.id() as i32);
    nix::sys::signal::kill(pid, signal).unwrap();
    let sent = Instant::now();
    let out = output_of(child).await;
    (out, sent.elapsed())
}

/// The session still holds `issue-12`: the lead cannot claim it.
async fn still_held(machine: &Machine) {
    let taken = machine.run(Some("lead1"), &["claim", "issue-12"]).await;
    assert!(
        !text(&taken.stdout).contains("You hold"),
        "{}",
        text(&taken.stdout)
    );
}

#[tokio::test]
async fn a_sigint_stops_the_wait_of_the_verify_request() {
    use riff::text::{RUN_END, RUN_RELEASE, RUN_VERIFY_REQUEST, run_not_sent};

    let machine = Machine::new().await;
    // The verify request first reads the holds (01M5157TYDEHZHDXSC1AHCRDRN).
    let (out, took) =
        a_signal_in_the_end_steps(&machine, "/v1/plan/show", nix::sys::signal::Signal::SIGINT)
            .await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(
        took < Duration::from_secs(5),
        "{took:?}\n{stdout}\n{stderr}"
    );
    assert_eq!(out.status.code(), Some(1), "{stdout}\n{stderr}");
    assert!(stdout.contains("The pull request #40"), "{stdout}");
    let not_sent = run_not_sent("issue-12", &[RUN_VERIFY_REQUEST, RUN_RELEASE, RUN_END]);
    assert!(stdout.contains(&not_sent), "{stdout}");
    let thread = machine.ok(Some("lead1"), &["read", "--all"]).await;
    assert!(!thread.contains("verify request: issue-12"), "{thread}");
    still_held(&machine).await;
}

#[tokio::test]
async fn a_sigterm_stops_the_wait_of_the_release() {
    use riff::text::{RUN_END, RUN_RELEASE, run_not_sent};

    let machine = Machine::new().await;
    // With no pull request, the release is the first end step.
    std::fs::write(machine.bin.path().join("no-pr"), "").unwrap();
    let (out, took) =
        a_signal_in_the_end_steps(&machine, "/v1/release", nix::sys::signal::Signal::SIGTERM).await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(
        took < Duration::from_secs(5),
        "{took:?}\n{stdout}\n{stderr}"
    );
    assert_eq!(out.status.code(), Some(1), "{stdout}\n{stderr}");
    assert!(stdout.contains("No open pull request"), "{stdout}");
    let not_sent = run_not_sent("issue-12", &[RUN_RELEASE, RUN_END]);
    assert!(stdout.contains(&not_sent), "{stdout}");
    still_held(&machine).await;
}

/// A SIGTERM after the end of the session, while `gh pr view` finds the
/// pull request, stops `riff run` before its first end step (#770,
/// 01M51JG5Y47184401EWT4HVEG0). riff waits for no signal then, so only
/// the streams of the session keep it.
#[tokio::test]
async fn a_sigterm_while_riff_finds_the_pull_request_stops_it_before_the_end_steps() {
    use riff::text::{RUN_END, RUN_RELEASE, RUN_VERIFY_REQUEST, run_not_sent};

    let machine = Machine::new().await;
    machine.lead().await;
    let hold = machine.bin.path().join("pr-hold");
    std::fs::write(&hold, "").unwrap();
    let child = machine
        .command(None, &["run", "issue-12", "--image", IMAGE])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(
        &machine.bin.path().join("gh.log"),
        "gh pr view worktree-issue-12",
    )
    .await;
    let pid = nix::unistd::Pid::from_raw(child.id() as i32);
    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).unwrap();
    std::fs::remove_file(&hold).unwrap();

    let out = output_of(child).await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert_eq!(out.status.code(), Some(1), "{stdout}\n{stderr}");
    assert!(stdout.contains("The pull request #40"), "{stdout}");
    let not_sent = run_not_sent("issue-12", &[RUN_VERIFY_REQUEST, RUN_RELEASE, RUN_END]);
    assert!(stdout.contains(&not_sent), "{stdout}");
    let thread = machine.ok(Some("lead1"), &["read", "--all"]).await;
    assert!(!thread.contains("verify request: issue-12"), "{thread}");
    still_held(&machine).await;
}

#[tokio::test]
async fn riff_run_with_no_daemon_takes_no_claim_and_says_how_to_start_it() {
    let mut machine = Machine::new().await;
    machine.lead().await;
    machine.stop_daemon().await;

    let out = machine
        .run(None, &["run", "issue-12", "--image", IMAGE])
        .await;
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    let socket = machine.env.riff_home().join("daemon/riff.sock");
    assert!(
        stderr.contains(&format!("No riff daemon answers on {}", socket.display())),
        "{stderr}"
    );
    assert!(stderr.contains("Start it: riff daemon install"), "{stderr}");
    assert!(machine.read("run.args").is_empty());
    assert!(!machine.env.riff_home().join("sessions").exists());
    machine.ok(Some("lead1"), &["claim", "issue-12"]).await;
}

/// `riff wave run` asks the daemon to run the wave of the repository of
/// the clone, and `riff wave stop` ends it (01M4ZV3B6J89KJQ0F14YWTZMJQ).
/// `riff daemon status` shows the limit and the wave
/// (01M4ZV3B6HAFTSC2K63FMC5FYA). The riff is paused, so the wave starts
/// nothing.
#[tokio::test]
async fn riff_wave_run_and_stop_ask_the_daemon() {
    let machine = Machine::new().await;
    let out = machine.ok(None, &["wave", "run"]).await;
    assert!(
        out.contains("The riff daemon runs the wave of como-technologies/riff."),
        "{out}"
    );
    let waves = machine.env.riff_home().join("daemon/waves.json");
    let kept = std::fs::read_to_string(&waves).unwrap();
    assert!(
        kept.contains(&format!("{:?}", machine.main.display().to_string())),
        "{kept}"
    );
    let status = machine.ok(None, &["daemon", "status"]).await;
    assert!(
        status.contains("The limit of this host: 3 live sessions, and 0 GB"),
        "{status}"
    );
    assert!(
        status.contains("Waves:\n  como-technologies/riff  "),
        "{status}"
    );
    let out = machine.ok(None, &["wave", "stop"]).await;
    assert!(
        out.contains("starts no new session for the wave of como-technologies/riff"),
        "{out}"
    );
    let status = machine.ok(None, &["daemon", "status"]).await;
    assert!(!status.contains("Waves:"), "{status}");
    let out = machine.run(None, &["wave", "stop"]).await;
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("The riff daemon does not run the wave of como-technologies/riff."),
        "{stderr}"
    );
}

#[tokio::test]
async fn riff_lead_start_runs_the_lead_with_a_terminal_and_attaches_to_it() {
    let machine = Machine::new().await;
    machine.lead().await;
    // The fake claude of the lead waits until the test ends it.
    std::fs::write(machine.bin.path().join("hold"), "").unwrap();
    let name = "riff-lead-como-technologies-riff";
    let out = machine.ok(None, &["lead", "start"]).await;
    assert!(
        out.contains(&format!(
            "The lead of como-technologies/riff starts in the container {name}"
        )),
        "{out}"
    );
    assert!(out.contains(riff::text::LEAD_DETACH), "{out}");
    assert_eq!(
        machine.read("podman.log").trim(),
        format!("podman attach {name}")
    );

    // The daemon runs it with a terminal: the relay runs claude in its
    // own place, with the riff tools.
    let args = machine.read("run.args");
    let args: Vec<&str> = args.lines().collect();
    assert_eq!(args[..5], ["run", "--rm", "-d", "-i", "-t"]);
    let at = args.iter().position(|a| *a == "/opt/riff/riff").unwrap();
    assert_eq!(
        args[at..at + 5],
        ["/opt/riff/riff", "relay", "--terminal", "--", "claude"]
    );
    wait_for(&machine.bin.path().join("claude.args"), "--mcp-config").await;
    let claude = machine.read("claude.args");
    assert!(claude.contains("--permission-mode\nauto\n"), "{claude}");
    assert!(
        claude.contains("You are the lead of como-technologies/riff"),
        "{claude}"
    );
    assert!(!claude.contains("\n-p\n"), "{claude}");

    // Its folder: a clone and a Claude folder with the first-run answers.
    let folder = machine.env.riff_home().join("leads/como-technologies-riff");
    assert!(folder.join("clone/.git").exists());
    let json: Value =
        serde_json::from_slice(&std::fs::read(folder.join("claude/.claude.json")).unwrap())
            .unwrap();
    assert_eq!(json["hasCompletedOnboarding"], true);
    let clone = folder.join("clone").display().to_string();
    assert_eq!(json["projects"][&clone]["hasTrustDialogAccepted"], true);

    // The daemon shows it, and a second start attaches again.
    let status = machine.ok(None, &["daemon", "status"]).await;
    assert!(status.contains(name), "{status}");
    let again = machine.ok(None, &["lead", "start"]).await;
    assert!(
        again.contains(&riff::text::lead_runs("como-technologies/riff", name)),
        "{again}"
    );
    assert_eq!(
        machine.read("podman.log").matches("podman attach").count(),
        2
    );
    // 01M51PD7Q24N252V66NFEHY49N: the lead runs on after the attach.
    for out in [&out, &again] {
        assert!(out.contains(&riff::text::lead_detached(name)), "{out}");
        assert!(!out.contains(&riff::text::lead_ended(name)), "{out}");
    }

    // The person ends the lead: the daemon ends its session.
    std::fs::remove_file(machine.bin.path().join("hold")).unwrap();
    let started = Instant::now();
    while machine.ok(None, &["daemon", "status"]).await.contains(name) {
        assert!(started.elapsed() < isolated::HANG, "the lead did not end");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// 01M51PD7Q24N252V66NFEHY49N: the person types `/exit` in the lead, so
/// its container ends in the attach. `riff lead start` says that the
/// lead ended, not that the person left its terminal.
#[tokio::test]
async fn riff_lead_start_says_that_the_lead_ended_when_it_ends_in_the_attach() {
    let machine = Machine::new().await;
    machine.lead().await;
    std::fs::write(machine.bin.path().join("hold"), "").unwrap();
    std::fs::write(machine.bin.path().join("exit-in-attach"), "").unwrap();
    let name = "riff-lead-como-technologies-riff";
    let out = machine.ok(None, &["lead", "start"]).await;
    assert!(out.contains(&riff::text::lead_ended(name)), "{out}");
    assert!(out.contains(riff::text::LEAD_NO_RESUME), "{out}");
    assert!(!out.contains(&riff::text::lead_detached(name)), "{out}");
    // The daemon ends the session of the lead.
    let started = Instant::now();
    while machine.ok(None, &["daemon", "status"]).await.contains(name) {
        assert!(started.elapsed() < isolated::HANG, "the lead did not end");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// 01M51JYDHAQDJ3GH2J8DT3AZQS: git cannot fast-forward the clone of the
/// lead, so the clone stays as it is, and the lead starts.
#[tokio::test]
async fn riff_lead_start_keeps_a_clone_that_git_cannot_fast_forward_and_starts_the_lead() {
    let machine = Machine::new().await;
    machine.lead().await;
    let name = "riff-lead-como-technologies-riff";
    let until_the_lead_ends = || async {
        let started = Instant::now();
        while machine.ok(None, &["daemon", "status"]).await.contains(name) {
            assert!(started.elapsed() < isolated::HANG, "the lead did not end");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    machine.ok(None, &["lead", "start"]).await;
    until_the_lead_ends().await;
    let clone = machine
        .env
        .riff_home()
        .join("leads/como-technologies-riff/clone");
    let old = git(&clone, &["rev-parse", "HEAD"]);

    // origin gets the new file b, and the clone has an untracked file b.
    std::fs::write(machine.main.join("b"), "origin\n").unwrap();
    git(&machine.main, &["add", "b"]);
    git(&machine.main, &["commit", "-q", "-m", "two"]);
    git(&machine.main, &["push", "-q", "origin", "main"]);
    std::fs::write(clone.join("b"), "mine\n").unwrap();

    let out = machine.ok(None, &["lead", "start"]).await;
    // The note with the reason, and the error of git in it.
    let kept = riff::text::lead_clone_kept(&clone, &riff::text::lead_clone_no_forward(""));
    let (kept, _) = kept.split_once(')').unwrap();
    assert!(out.contains(kept), "{out}");
    assert!(
        out.contains("git merge -q --ff-only @{upstream}: "),
        "{out}"
    );
    assert!(
        out.contains(&format!(
            "The lead of como-technologies/riff starts in the container {name}"
        )),
        "{out}"
    );
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), old);
    assert_eq!(std::fs::read_to_string(clone.join("b")).unwrap(), "mine\n");
    until_the_lead_ends().await;
}

#[tokio::test]
async fn riff_takeover_gives_the_person_the_session_and_the_session_goes_on() {
    take_over_a_session(&[]).await;
}

#[tokio::test]
async fn riff_takeover_at_end_waits_for_the_end_of_the_turn_and_the_session_goes_on() {
    take_over_a_session(&["--at-end"]).await;
}

/// `riff takeover` with the flags `flags` of a session of `riff run` that
/// waits in its turn: the takeover interrupts the turn, or with
/// `--at-end` it sends no interrupt. Then the session goes on with
/// `--resume` and one new input (01M51KJ1R9K7A7RW8T8XH8VW43).
async fn take_over_a_session(flags: &[&str]) {
    let at_end = flags.contains(&"--at-end");
    let machine = Machine::new().await;
    machine.lead().await;
    // claude takes the start text, and waits in its turn.
    std::fs::write(machine.bin.path().join("hold"), "").unwrap();
    let run = machine
        .command(None, &["run", "issue-12", "--image", IMAGE])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(&machine.bin.path().join("run.stdin"), "user").await;
    let name = container(&machine);
    let args = machine.read("run.args");
    let args: Vec<&str> = args.lines().collect();
    let session = args[args.iter().position(|a| *a == "--session-id").unwrap() + 1].to_owned();

    let mut args = vec!["takeover"];
    args.extend(flags);
    args.push(&name);
    let takeover = machine
        .command(None, &args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(
        &machine.bin.path().join("daemon.out"),
        &format!("A person takes over the session in the container {name}."),
    )
    .await;
    // The turn ends: the hold closed the stdin of claude. The fake claude
    // does not stop at the interrupt: the test ends its turn.
    std::fs::remove_file(machine.bin.path().join("hold")).unwrap();

    let taken = output_of(takeover).await;
    let shown = text(&taken.stdout);
    assert!(taken.status.success(), "{shown}{}", text(&taken.stderr));
    let first = if at_end {
        riff::text::takeover_waits(&name)
    } else {
        riff::text::takeover_interrupts(&name)
    };
    assert!(shown.contains(&first), "{shown}");
    let rest = machine.read("run.rest");
    assert_eq!(
        rest.contains(riff::session_relay::INTERRUPT),
        !at_end,
        "{rest}"
    );
    assert!(shown.contains(&riff::text::takeover_held(&name)), "{shown}");
    assert!(shown.contains(&riff::text::takeover_back(&name)), "{shown}");
    assert!(
        machine.read("podman.log").contains(&format!(
            "podman exec -it {name} /opt/riff/riff relay --terminal -- claude --resume \
             {session} --permission-mode auto"
        )),
        "{}",
        machine.read("podman.log")
    );

    // The worker goes on: claude -p again, with --resume and one input.
    let out = output_of(run).await;
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let claude = machine.read("claude.args");
    assert!(
        claude.contains(&format!("--resume\n{session}\n")),
        "{claude}"
    );
    assert!(!claude.contains("--session-id"), "{claude}");
    assert!(
        machine.read("run.stdin").contains("gives it back now"),
        "{}",
        machine.read("run.stdin")
    );
}

/// The riff-server of a machine with no riff-server: a port where
/// nothing listens.
const NO_SERVER: &str = "http://127.0.0.1:9";

/// The claims on the forge of the main clone `main`: each line of
/// `git ls-remote`.
fn claims_of(main: &Path) -> String {
    git(main, &["ls-remote", "origin", "refs/riff/claims/*"])
}

/// The verify requests of issue 12 in the repository thread, in the
/// records of the daemon of `machine`. The riff is the one of the line of
/// its first start in its log. riff reads a copy of the file of the
/// records, so the daemon can write on.
fn verify_requests(machine: &Machine) -> Vec<riff::peer::body::Message> {
    let log = machine.read("daemon.out");
    let line = log
        .lines()
        .find(|l| l.contains("a riff of its own: "))
        .unwrap_or_else(|| panic!("no line of the riff in\n{log}"));
    let riff = line.split("a riff of its own: ").nth(1).unwrap();
    let riff = riff.split(',').next().unwrap();
    let riff = riff::daemon_key::Public::parse(riff).unwrap();
    let copy = tempfile::tempdir().unwrap();
    let records = machine.env.riff_home().join("daemon/state/records");
    std::fs::copy(records, copy.path().join("records")).unwrap();
    let peers = riff::peer::keep::Peers::open(copy.path(), riff, |_| {}).unwrap();
    let thread = "como-technologies/riff".parse().unwrap();
    peers
        .state()
        .messages(&thread)
        .iter()
        .map(|kept| kept.message.clone())
        .filter(|m| m.body.starts_with("verify request: issue-12, PR #40"))
        .collect()
}

/// Done when 5 of #844 (01M519Z4RWHWP3TKNP4T5GKKE9): `riff run --peers`
/// and a wave of the daemon with `--peers` each do one item with the fake
/// runtime and no riff-server. The daemon claims the item on the bare
/// forge, sends the verify request to the lead as a `message` record,
/// and then releases the item.
#[tokio::test]
async fn riff_run_and_a_wave_with_peers_do_one_item_with_no_riff_server() {
    let machine = Machine::with_no_server().await;
    let log = machine.bin.path().join("daemon.out");
    wait_for(&log, "a riff of its own").await;
    git(&machine.main, &["config", "user.name", "Mike"]);
    git(&machine.main, &["config", "user.email", "mike@example.com"]);
    let hold = machine.bin.path().join("hold");

    // riff run --peers: the claim holds while the session runs.
    std::fs::write(&hold, "").unwrap();
    let child = machine
        .command(None, &["run", "issue-12", "--peers", "--image", IMAGE])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(&machine.bin.path().join("run.stdin"), "user").await;
    let claims = claims_of(&machine.main);
    assert!(claims.ends_with("refs/riff/claims/issue-12"), "{claims}");
    let commit = claims.split('\t').next().unwrap();
    let message = git(&machine.main, &["show", "-s", "--format=%B", commit]);
    assert!(message.starts_with("riff claim issue-12"), "{message}");
    assert!(message.contains("Riff-User: mike"), "{message}");
    std::fs::remove_file(&hold).unwrap();
    let out = output_of(child).await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains(&riff::text::run_peers_asks("issue-12")),
        "{stdout}"
    );
    assert!(stdout.contains("The pull request #40"), "{stdout}");
    assert!(
        stdout.contains(&riff::text::run_verify_sent("issue-12", 40)),
        "{stdout}"
    );
    assert!(stdout.contains("released issue-12 on the forge"), "{stdout}");
    assert_eq!(claims_of(&machine.main), "");
    let sent = verify_requests(&machine);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].kind, riff_core::wire::Kind::Message);
    let lead = riff_core::selector::Selector::lead("mike", "como-technologies/riff");
    assert_eq!(sent[0].to, [lead.clone()]);
    // No step reached a riff-server: riff run needs none.
    assert!(!machine.read("daemon.out").contains(NO_SERVER));
    assert!(!stderr.contains(NO_SERVER), "{stderr}");

    // A wave with --peers takes issue 12 from the look of gh, and does it
    // the same way.
    let wave_issue = machine.bin.path().join("wave-issue");
    std::fs::write(&wave_issue, "").unwrap();
    std::fs::write(&hold, "").unwrap();
    machine.ok(None, &["wave", "run", "--peers"]).await;
    let started = Instant::now();
    while !claims_of(&machine.main).ends_with("refs/riff/claims/issue-12") {
        assert!(
            started.elapsed() < isolated::HANG,
            "the wave claimed nothing:\n{}",
            machine.read("daemon.out")
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // The next look finds no free work.
    std::fs::remove_file(&wave_issue).unwrap();
    std::fs::remove_file(&hold).unwrap();
    let started = Instant::now();
    while verify_requests(&machine).len() < 2 || !claims_of(&machine.main).is_empty() {
        assert!(
            started.elapsed() < isolated::HANG,
            "the job of the wave did not end:\n{}",
            machine.read("daemon.out")
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let sent = verify_requests(&machine);
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!(sent[1].to, [lead]);
    machine.ok(None, &["wave", "stop"]).await;
    assert!(!machine.read("daemon.out").contains(NO_SERVER));
}

/// 01M519Z4PESWFV8Y03SKTSGV4Q, 01M51BFYW85YBEWAB20YTCJQFK: `riff release
/// ITEM --forge` in the main clone asks the riff daemon, and the daemon
/// deletes the claim on the forge with the lease of its commit. The
/// daemon is the owner of its own riff, so it frees a claim of another
/// user too. It needs no riff-server.
#[tokio::test]
async fn riff_release_forge_frees_a_claim_on_the_forge_through_the_daemon() {
    let machine = Machine::new().await;
    let log = machine.bin.path().join("daemon.out");
    wait_for(&log, "riff of its own").await;
    git(&machine.main, &["config", "user.name", "Brett"]);
    git(
        &machine.main,
        &["config", "user.email", "brett@example.com"],
    );
    let other = riff::daemon_key::Key::from_bytes(&[9; 32]).public();
    let claim = riff::forge_claim::Claim::now("issue-12", "brett", "s1", other);
    let took = riff::forge_claim::take(&machine.main, &claim).unwrap();
    assert!(matches!(took, riff::forge_claim::Took::Won(_)), "{took:?}");
    let claims = git(
        &machine.main,
        &["ls-remote", "origin", "refs/riff/claims/*"],
    );
    assert!(claims.ends_with("refs/riff/claims/issue-12"), "{claims}");

    let freed = machine.ok(None, &["release", "issue-12", "--forge"]).await;
    assert_eq!(
        freed.trim(),
        riff::text::forge_released("issue-12", Some("brett"))
    );
    let claims = git(
        &machine.main,
        &["ls-remote", "origin", "refs/riff/claims/*"],
    );
    assert_eq!(claims, "");
    assert!(
        machine
            .read("daemon.out")
            .contains(&freed.trim().to_owned())
    );

    // A second release finds no claim.
    let out = machine.run(None, &["release", "issue-12", "--forge"]).await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains(&riff::text::forge_release_none("issue-12")),
        "{}",
        text(&out.stderr)
    );
    // --forge takes no thread and no session of riff-server.
    let out = machine
        .run(None, &["release", "issue-12", "--forge", "--session", "s1"])
        .await;
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
}

#[tokio::test]
async fn riff_takeover_of_a_session_that_the_daemon_does_not_own_fails() {
    let machine = Machine::new().await;
    let out = machine
        .run(None, &["takeover", "riff-issue-9-00000000"])
        .await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("owns no session in the container riff-issue-9-00000000"),
        "{}",
        text(&out.stderr)
    );
}
