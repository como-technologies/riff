//! The lead and its workers in tmux (01M3JD390F49HZSKEJ3VACX0ZA to
//! 01M3JD39BASN1GNJTZXXKBCNZ9), and the workers of a machine
//! (01M3JPQT35BMR7XMAMMFSCDC2B to 01M3JPQTDFW3C7QBSZZ2M831MH). A fake
//! `tmux` on `PATH` writes each call to a log, and keeps the marks of
//! the panes and windows in files.

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::identity;
use riff::terminal::{self, Program, Tmux};
use riff_core::name::SessionUri;
use riff_core::wire::{RiffState, Status};

/// `list-panes -a` lists the worker panes with their session marks, from
/// the file `workers`. `kill-pane` removes a pane from it.
const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/log"
n=$(grep -c -e '^split-window' -e '^new-window' "$dir/log")
case "$1" in
  list-panes)
    if [ "$2" = "-a" ]; then cat "$dir/workers" 2>/dev/null; else cat "$dir/panes" 2>/dev/null; fi ;;
  list-windows) cat "$dir/windows" 2>/dev/null ;;
  display-message) echo "@0" ;;
  new-window) echo "@7 %$n" ;;
  split-window) echo "%$n" ;;
  set-option)
    case "$2 $5" in
      "-p @riff-session") echo "$4 $6" >> "$dir/workers" ;;
      "-p @riff") echo "$6" >> "$dir/panes" ;;
      "-w @riff") echo "$4 $6" >> "$dir/windows" ;;
    esac ;;
  kill-pane)
    grep -v "^$3 " "$dir/workers" > "$dir/workers.new"
    mv "$dir/workers.new" "$dir/workers" ;;
esac
exit 0
"#;

/// A directory with the fake `tmux`.
fn fake_tmux() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let tmux = dir.path().join("tmux");
    std::fs::write(&tmux, FAKE_TMUX).unwrap();
    std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

fn log(fake: &Path) -> String {
    std::fs::read_to_string(fake.join("log")).unwrap_or_default()
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A repository `main` with a linked worktree `wt`. Returns both paths.
fn repository(root: &Path) -> (PathBuf, PathBuf) {
    let main = root.join("main");
    std::fs::create_dir(&main).unwrap();
    git(&main, &["init", "-q"]);
    git(&main, &["commit", "-q", "--allow-empty", "-m", "x"]);
    git(&main, &["worktree", "add", "-q", "../wt"]);
    (
        std::fs::canonicalize(&main).unwrap(),
        std::fs::canonicalize(root.join("wt")).unwrap(),
    )
}

/// A machine for `riff workers`: the fake `tmux` first on `PATH`, its
/// own settings directory, and a riff-server URL. It is a plain
/// terminal: no session ID and no worker mark.
struct Machine {
    fake: tempfile::TempDir,
    run: tempfile::TempDir,
    server: String,
}

impl Machine {
    fn new(server: &str) -> Self {
        Machine {
            fake: fake_tmux(),
            run: tempfile::tempdir().unwrap(),
            server: server.into(),
        }
    }

    fn log(&self) -> String {
        log(self.fake.path())
    }

    /// `riff workers ARGS` in `dir`. `in_tmux` sets the variables of a
    /// tmux pane.
    fn riff(&self, dir: &Path, args: &[&str], in_tmux: bool) -> Command {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
        cmd.arg("workers")
            .args(args)
            .current_dir(dir)
            .env("PATH", path)
            .env("RIFF_HOME", self.run.path())
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env_remove("RIFF_SESSION")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("RIFF_WORKER");
        if in_tmux {
            cmd.env("TMUX", "/tmp/tmux-1000/default,1,0")
                .env("TMUX_PANE", "%0");
        } else {
            cmd.env_remove("TMUX").env_remove("TMUX_PANE");
        }
        cmd
    }

    /// `riff workers start ARGS` in tmux.
    fn start(&self, dir: &Path, args: &[&str]) -> std::process::Output {
        let mut all = vec!["start"];
        all.extend(args);
        self.riff(dir, &all, true).output().unwrap()
    }

    /// The MCP config file of the workers.
    fn mcp_file(&self) -> PathBuf {
        self.run.path().join("state").join(riff::worker_mcp::FILE)
    }

    /// The MCP config of the workers, as JSON.
    fn mcp(&self) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(self.mcp_file()).unwrap()).unwrap()
    }

    /// `riff workers limit N`.
    fn limit(&self, limit: u16) {
        let out = self
            .riff(Path::new("/"), &["limit", &limit.to_string()], false)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    }
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The session ID in each `set-option -p -t PANE @riff-session ID`.
fn marked_sessions(log: &str) -> Vec<String> {
    log.lines()
        .filter_map(|l| l.strip_prefix("set-option -p -t "))
        .filter_map(|l| l.split_once(" @riff-session "))
        .map(|(_, id)| id.to_owned())
        .collect()
}

#[test]
fn workers_start_opens_one_window_with_a_pane_for_each_worker() {
    let m = Machine::new("http://riff.test:7878");
    m.limit(3);
    let root = tempfile::tempdir().unwrap();
    let (main, wt) = repository(root.path());
    let out = m.start(&wt, &["3"]);
    assert!(out.status.success(), "{out:?}");
    assert!(
        stdout(&out).contains("Started 3 workers in")
            && stdout(&out).contains("tmux select-window -t riff-workers"),
        "{out:?}"
    );

    // Each worker has its own riff session ID, in RIFF_SESSION and in
    // the mark of its pane (01M3JPQT9BA7JVMZPV68FY4MQ6).
    let log = m.log();
    let ids = marked_sessions(&log);
    assert_eq!(ids.len(), 3, "{log}");
    assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2]);
    let dir = main.display();
    // Each pane runs claude through the wrapper (01M3JQC8ANFYYEXSHBS2DCZYBX).
    // `RIFF_ON=1` of the test environment turned riff on for the
    // command, so each worker gets it (01M3XY2SWEK0N8MC3MY4TMYTD3).
    let env = |id: &str| {
        format!(
            "-e RIFF_SERVER=http://riff.test:7878 -e RIFF_WORKER=1 -e RIFF_SESSION={id} \
             -e RIFF_ON=1 '{}' workers run 'claude' '--strict-mcp-config' '--mcp-config' '{}' '--settings' \
             '{{\"remoteControlAtStartup\":false,\"awaySummaryEnabled\":false,\"permissions\":{{\"deny\":[\"Bash(riff cloud)\",\"Bash(riff cloud *)\"]}}}}' 'Join the riff.'",
            Isolated::shared().riff_path().display(),
            m.mcp_file().display(),
        )
    };
    let pane = |id: &str| format!("-d -c {dir} -P -F #{{pane_id}} {}", env(id));
    assert_eq!(
        log.lines().collect::<Vec<_>>(),
        [
            "list-panes -a -F #{pane_id} #{@riff-session}".to_owned(),
            "list-windows -t %0 -F #{window_id} #{@riff}".to_owned(),
            "display-message -p -t %0 #{window_id}".to_owned(),
            format!(
                "new-window -a -t @0 -n riff-workers -d -c {dir} -P -F #{{window_id}} #{{pane_id}} {}",
                env(&ids[0])
            ),
            "set-option -w -t @7 @riff workers".to_owned(),
            format!("set-option -p -t %1 @riff-session {}", ids[0]),
            format!("split-window -t @7 {}", pane(&ids[1])),
            format!("set-option -p -t %2 @riff-session {}", ids[1]),
            "select-layout -t @7 tiled".to_owned(),
            format!("split-window -t @7 {}", pane(&ids[2])),
            format!("set-option -p -t %3 @riff-session {}", ids[2]),
            "select-layout -t @7 tiled".to_owned(),
        ]
    );
    // A worker has no Remote Control (01M3JD394YFA3TQRE3E72ZER4Z), also
    // when the user settings turn it on (01M3JV0ZNGKDFMRR9ACT0480V9), and
    // no recap (01M3MN0D429T4Q80DYBE9S9XR7).
    assert!(!log.contains("remote-control"), "{log}");
    assert_eq!(
        log.matches(
            r#"'--settings' '{"remoteControlAtStartup":false,"awaySummaryEnabled":false,"permissions":{"deny":["Bash(riff cloud)","Bash(riff cloud *)"]}}'"#
        )
        .count(),
        3,
        "{log}"
    );
}

/// A worker gets its settings on the command line. The user settings
/// file of the person does not change (01M3MN0D429T4Q80DYBE9S9XR7).
#[test]
fn workers_start_leaves_the_user_settings_file() {
    let m = Machine::new("http://riff.test:7878");
    m.limit(1);
    let root = tempfile::tempdir().unwrap();
    let (_, wt) = repository(root.path());
    let home = tempfile::tempdir().unwrap();
    let settings = home.path().join(".claude/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let text = r#"{"remoteControlAtStartup":true,"awaySummaryEnabled":true}"#;
    std::fs::write(&settings, text).unwrap();
    let out = m
        .riff(&wt, &["start", "1"], true)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), text);
    assert!(
        m.log().contains(r#""awaySummaryEnabled":false"#),
        "{}",
        m.log()
    );
}

/// A worker starts no language server: each installed plugin with one
/// is off in the flag settings of the worker. The user settings file
/// does not change (01M3ZJ1FAF7EJXP9CSET8ZY1K3).
#[test]
fn a_worker_starts_with_each_plugin_with_a_language_server_off() {
    let m = Machine::new("http://riff.test:7878");
    m.limit(1);
    let root = tempfile::tempdir().unwrap();
    let (_, wt) = repository(root.path());
    let home = tempfile::tempdir().unwrap();
    let claude = home.path().join(".claude");
    let market = home.path().join("market");
    std::fs::create_dir_all(market.join(".claude-plugin")).unwrap();
    std::fs::write(
        market.join(".claude-plugin/marketplace.json"),
        r#"{"plugins": [{"name": "rust-analyzer-lsp", "lspServers": {"rust-analyzer": {}}}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(claude.join("plugins")).unwrap();
    std::fs::write(
        claude.join("plugins/known_marketplaces.json"),
        serde_json::json!({"official": {"installLocation": market}}).to_string(),
    )
    .unwrap();
    std::fs::write(
        claude.join("plugins/installed_plugins.json"),
        r#"{"plugins": {"rust-analyzer-lsp@official": [], "riff@riff": []}}"#,
    )
    .unwrap();
    let text = r#"{"enabledPlugins": {"rust-analyzer-lsp@official": true}}"#;
    std::fs::write(claude.join("settings.json"), text).unwrap();
    let out = m
        .riff(&wt, &["start", "1"], true)
        .env("HOME", home.path())
        .env_remove("CLAUDE_CONFIG_DIR")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(
        m.log().contains(
            r#"'--settings' '{"remoteControlAtStartup":false,"awaySummaryEnabled":false,"permissions":{"deny":["Bash(riff cloud)","Bash(riff cloud *)"]},"enabledPlugins":{"rust-analyzer-lsp@official":false}}'"#
        ),
        "{}",
        m.log()
    );
    assert_eq!(
        std::fs::read_to_string(claude.join("settings.json")).unwrap(),
        text
    );
}

/// A worker loads only the MCP servers of `workers.mcp`: riff by
/// default (01M3NB5R92ZC61VW6Y45SJEAY9), and riff plus NAME after
/// `riff workers mcp add NAME` (01M3NB5R6X5AV79DQNKKJBH5J8).
#[test]
fn a_worker_loads_only_the_mcp_servers_of_workers_mcp() {
    let m = Machine::new("http://riff.test:7878");
    m.limit(2);
    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());
    let home = tempfile::tempdir().unwrap();
    let claude_json = home.path().join(".claude.json");
    let person = serde_json::json!({
        "mcpServers": {
            "github": {"command": "gh-mcp", "env": {"TOKEN": "t"}},
            "gmail": {"type": "http", "url": "https://mail.test"},
        },
    });
    std::fs::write(&claude_json, person.to_string()).unwrap();
    let start = || {
        m.riff(&main, &["start", "1"], true)
            .env("HOME", home.path())
            .env_remove("CLAUDE_CONFIG_DIR")
            .output()
            .unwrap()
    };

    let out = start();
    assert!(out.status.success(), "{out:?}");
    let command = format!(
        "'--strict-mcp-config' '--mcp-config' '{}' '--settings'",
        m.mcp_file().display()
    );
    assert!(m.log().contains(&command), "{}", m.log());
    let riff = Isolated::shared().riff_path().display().to_string();
    assert_eq!(
        m.mcp(),
        serde_json::json!({"mcpServers": {"riff": {"command": riff, "args": ["mcp"]}}})
    );
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(m.mcp_file())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    let out = m
        .riff(&main, &["mcp", "add", "github"], false)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(
        stdout(&out).starts_with("workers.mcp  riff, github  ("),
        "{out:?}"
    );
    let out = start();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        m.mcp(),
        serde_json::json!({"mcpServers": {
            "riff": {"command": riff, "args": ["mcp"]},
            "github": {"command": "gh-mcp", "env": {"TOKEN": "t"}},
        }})
    );
    assert_eq!(m.log().matches(&command).count(), 2, "{}", m.log());
    // The token of a server is in the file, never on the command line.
    assert!(!m.log().contains("gh-mcp"), "{}", m.log());
    // The MCP config of the person does not change.
    assert_eq!(
        std::fs::read_to_string(&claude_json).unwrap(),
        person.to_string()
    );
}

/// `riff workers mcp` shows the list, and changes it only as asked.
/// riff stays in the list. A name that the person does not have is
/// left out of a worker, with a warning.
#[test]
fn workers_mcp_keeps_riff_and_warns_about_a_missing_server() {
    let m = Machine::new("http://riff.test:7878");
    m.limit(1);
    let mcp = |args: &[&str]| {
        let mut all = vec!["mcp"];
        all.extend(args);
        m.riff(Path::new("/"), &all, false).output().unwrap()
    };
    let out = mcp(&[]);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).starts_with("workers.mcp  riff  ("), "{out:?}");
    let out = mcp(&["remove", "riff"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(stderr(&out).contains("riff stays"), "{out:?}");
    assert!(mcp(&["add", "unifi"]).status.success());
    assert!(mcp(&["add", "unifi"]).status.success());
    assert!(stdout(&mcp(&[])).starts_with("workers.mcp  riff, unifi  ("));

    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());
    let home = tempfile::tempdir().unwrap();
    let out = m
        .riff(&main, &["start", "1"], true)
        .env("HOME", home.path())
        .env_remove("CLAUDE_CONFIG_DIR")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(stderr(&out).contains("no MCP server unifi"), "{out:?}");
    assert_eq!(m.mcp()["mcpServers"].as_object().unwrap().len(), 1);

    let out = mcp(&["remove", "unifi"]);
    assert!(stdout(&out).starts_with("workers.mcp  riff  ("), "{out:?}");
}

#[test]
fn a_second_start_adds_panes_to_the_same_window() {
    let m = Machine::new("http://riff.test:7878");
    m.limit(3);
    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());
    assert!(m.start(&main, &["1"]).status.success());
    let out = m.start(&main, &["2", "--claude", "/opt/claude"]);
    assert!(out.status.success(), "{out:?}");
    let log = m.log();
    assert_eq!(log.matches("new-window").count(), 1, "{log}");
    assert_eq!(log.matches("split-window -t @7").count(), 2, "{log}");
    assert!(
        log.contains("run '/opt/claude' '--strict-mcp-config'"),
        "{log}"
    );
}

#[test]
fn outside_tmux_workers_start_starts_nothing() {
    let m = Machine::new("http://riff.test:7878");
    m.limit(3);
    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());
    let out = m.riff(&main, &["start", "3"], false).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("needs tmux") && stderr(&out).contains("started nothing"),
        "{out:?}"
    );
    assert_eq!(m.log(), "");
}

#[test]
fn workers_start_needs_a_count_of_one_or_more() {
    let m = Machine::new("http://riff.test:7878");
    let out = m.start(Path::new("/"), &["0"]);
    assert!(!out.status.success());
    assert_eq!(m.log(), "");
}

/// On a new machine, the limit is 0: no worker starts
/// (01M3JPQT35BMR7XMAMMFSCDC2B).
#[test]
fn a_new_machine_starts_no_worker() {
    let m = Machine::new("http://riff.test:7878");
    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());
    let out = m.start(&main, &["2"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let err = stderr(&out);
    assert!(
        err.contains("the limit of workers on this machine is 0"),
        "{err}"
    );
    assert!(err.contains("riff workers limit"), "{err}");
    assert_eq!(m.log(), "");
    let out = m.riff(&main, &["limit"], false).output().unwrap();
    assert!(stdout(&out).starts_with("workers.limit  0  ("), "{out:?}");
}

/// The limit stops the third worker, then each more
/// (01M3JPQT57PJCRBQYJNDVESS04).
#[test]
fn the_limit_stops_the_workers_past_it() {
    let m = Machine::new("http://riff.test:7878");
    m.limit(2);
    let config = std::fs::read_to_string(m.run.path().join("config.toml")).unwrap();
    assert_eq!(config, "[workers]\nlimit = 2\n");
    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());

    let out = m.start(&main, &["3"]);
    assert!(out.status.success(), "{out:?}");
    let out = stdout(&out);
    assert!(out.contains("Started 2 workers in"), "{out}");
    assert!(
        out.contains(
            "The limit of this machine is 2, and 0 workers ran before, so 1 worker did not start."
        ),
        "{out}"
    );
    assert_eq!(marked_sessions(&m.log()).len(), 2);

    let out = m.start(&main, &["1"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(stderr(&out).contains("2 workers run"), "{out:?}");
    assert_eq!(marked_sessions(&m.log()).len(), 2);
}

/// A worker never starts workers (01M3JPQT79FE47518Z8DFFQYYG).
#[test]
fn a_worker_starts_no_worker() {
    let m = Machine::new("http://riff.test:7878");
    m.limit(2);
    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());
    let out = m
        .riff(&main, &["start", "1"], true)
        .env("RIFF_WORKER", "1")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        stderr(&out).contains("a worker never starts workers"),
        "{out:?}"
    );
    assert_eq!(m.log(), "");
}

/// The session `id` of mike on pangolin, at the place of `dir`.
fn session_in(dir: &Path, id: &str) -> SessionUri {
    let place = identity::place_in(dir, "pangolin").unwrap();
    SessionUri::new(riff_core::name::Who::new("mike", Some(id)).unwrap(), place)
}

/// Only the lead starts workers from an agent session
/// (01M3JPQT79FE47518Z8DFFQYYG).
#[tokio::test(flavor = "multi_thread")]
async fn only_the_lead_session_starts_workers() {
    let api = start_server().await;
    let m = Machine::new(api.base());
    m.limit(2);
    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());
    let lead = session_in(&main, "a1");
    let other = session_in(&main, "a2");
    api.register(&lead).await.unwrap();
    api.register(&other).await.unwrap();

    let start_as = |id: &str| {
        m.riff(&main, &["start", "1"], true)
            .env("RIFF_SESSION", id)
            .output()
            .unwrap()
    };
    let out = start_as("a2");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        stderr(&out).contains("only the lead of your user starts workers"),
        "{out:?}"
    );
    assert_eq!(marked_sessions(&m.log()).len(), 0);

    let out = start_as("a1");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(marked_sessions(&m.log()).len(), 1);
}

/// Two workers that run on the server: each is registered, holds a
/// claim and has a status. Their panes are in the fake tmux.
async fn two_workers(api: &Api, m: &Machine, dir: &Path) -> Vec<SessionUri> {
    let thread = "como-technologies/riff".parse().unwrap();
    let mut workers = Vec::new();
    let mut panes = String::new();
    for (pane, id, item) in [("%3", "w1", "issue-12"), ("%4", "w2", "issue-13")] {
        let me = session_in(dir, id);
        api.register(&me).await.unwrap();
        if workers.is_empty() {
            api.set_riff(&me, RiffState::Running).await.unwrap();
        }
        api.claim(&me, &thread, item).await.unwrap();
        let status = Status {
            step: format!("tests of {item}"),
        };
        api.status(&me, &status).await.unwrap();
        panes.push_str(&format!("{pane} {id}\n"));
        workers.push(me);
    }
    std::fs::write(m.fake.path().join("workers"), panes).unwrap();
    workers
}

/// `riff workers` lists each worker with its pane, session ID, claim
/// and status (01M3JPQTBDGT54WN7FZP9CD6B5).
#[tokio::test(flavor = "multi_thread")]
async fn workers_lists_each_worker() {
    let api = start_server().await;
    let m = Machine::new(api.base());
    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());
    let workers = two_workers(&api, &m, &main).await;
    // w1 is live, so the server gives its state (01M3QB6CJ1XCQG5B1BVR8AF3B4).
    let _watch = api.watch(&workers[0]).await.unwrap();

    let out = m.riff(&main, &[], false).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let out = stdout(&out);
    assert!(out.contains("\nPANE  ID  STATE    DETAIL\n"), "{out}");
    let row = |pane: &str| out.lines().find(|l| l.starts_with(pane)).unwrap();
    assert!(
        row("%3  ").starts_with("%3    w1  busy     working on #12  "),
        "{out}"
    );
    assert!(row("%3  ").ends_with(" ago: tests of issue-12"), "{out}");
    assert!(row("%4  ").starts_with("%4    w2  offline  seen "), "{out}");

    std::fs::remove_file(m.fake.path().join("workers")).unwrap();
    let out = m.riff(&main, &[], false).output().unwrap();
    let out = stdout(&out);
    // The heading of this machine has its numbers and its score
    // (01M3Q5QE4SQ8VYN2PSF42KB3QJ), the line of its disk follows
    // (01M41A11GHP78E2VYN14JSE27P), then the line of its compile cache
    // (01M492398HA0AXX0J8BZCKNGTG), then the line of its monitor
    // (01M421QPX01BB15GJXHFYRETTX), and no table.
    assert!(out.contains("  runs 0  cpu "), "{out}");
    let mut lines = out.lines();
    assert!(
        lines
            .next()
            .is_some_and(|l| l.ends_with(|c: char| c.is_ascii_digit())),
        "{out}"
    );
    assert!(
        lines.next().is_some_and(|l| l.starts_with("disk ")),
        "{out}"
    );
    assert!(
        lines.next().is_some_and(|l| l.starts_with("cache ")),
        "{out}"
    );
    assert!(
        lines
            .next()
            .is_some_and(|l| l.starts_with("monitor off  load5 ")),
        "{out}"
    );
    assert!(!out.contains("PANE"), "{out}");
    assert!(out.contains("  score "), "{out}");
    assert_eq!(out.lines().count(), 4, "{out}");
}

/// `riff workers stop` ends each worker: within 10 seconds, no worker is
/// in `riff who`, and their claims are free (01M3JPQTDFW3C7QBSZZ2M831MH).
#[tokio::test(flavor = "multi_thread")]
async fn workers_stop_ends_each_worker() {
    let api = start_server().await;
    let m = Machine::new(api.base());
    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());
    let workers = two_workers(&api, &m, &main).await;
    let person = session_in(&main, "p1");
    api.register(&person).await.unwrap();

    // One pane first.
    let out = m.riff(&main, &["stop", "%3"], false).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("Stopped 1 worker."), "{out:?}");
    let out = m.riff(&main, &["stop", "%3"], false).output().unwrap();
    assert!(!out.status.success(), "{out:?}");
    assert!(
        stderr(&out).contains("no worker runs in the pane %3"),
        "{out:?}"
    );

    let out = m.riff(&main, &["stop"], false).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("Stopped 1 worker."), "{out:?}");
    let log = m.log();
    assert!(
        log.contains("kill-pane -t %3\n") && log.contains("kill-pane -t %4\n"),
        "{log}"
    );

    let start = Instant::now();
    loop {
        let who = api.who(&person, false).await.unwrap();
        let gone = workers
            .iter()
            .all(|w| !who.iter().any(|s| s.uri.who() == w.who()));
        if gone {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "{who:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let thread = "como-technologies/riff".parse().unwrap();
    for item in ["issue-12", "issue-13"] {
        let reply = api.claim(&person, &thread, item).await.unwrap();
        assert!(reply.granted, "{item}: {:?}", reply.held);
    }
}

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_lead_gets_one_tail_pane() {
    let api = start_server().await;
    let lead = uri("riff://mike@pangolin/como-technologies/riff?session=a1");
    let worker = uri("riff://mike@pangolin/como-technologies/riff?session=a2");
    api.register(&lead).await.unwrap();
    api.register(&worker).await.unwrap();
    let fake = fake_tmux();
    let tmux = Tmux::new(fake.path().join("tmux"), "%0");
    let tail = Program::tail("/bin/riff".as_ref(), "/src/riff".as_ref(), api.base());

    // A session that is not the lead gets no pane, and calls no tmux.
    assert!(
        !terminal::tail_beside_lead(&api, &worker, &tmux, &tail)
            .await
            .unwrap()
    );
    assert_eq!(log(fake.path()), "");

    assert!(
        terminal::tail_beside_lead(&api, &lead, &tmux, &tail)
            .await
            .unwrap()
    );
    // A restart, a /clear or a resume finds the marked pane.
    assert!(
        !terminal::tail_beside_lead(&api, &lead, &tmux, &tail)
            .await
            .unwrap()
    );
    let log = log(fake.path());
    assert_eq!(
        log.lines().collect::<Vec<_>>(),
        [
            "list-panes -t %0 -F #{@riff}".to_owned(),
            format!(
                "split-window -h -t %0 -d -c /src/riff -P -F #{{pane_id}} -e RIFF_SERVER={} \
                 '/bin/riff' tail",
                api.base()
            ),
            "set-option -p -t %1 @riff tail".to_owned(),
            "list-panes -t %0 -F #{@riff}".to_owned(),
        ]
    );
}

/// A query makes the server know a session, and makes no lead. So a
/// session in the list is not always a session that did its register:
/// the pane comes only after the register
/// (01M3XM68N5M5DKB86W5079X2G9).
#[tokio::test(flavor = "multi_thread")]
async fn a_session_that_only_asked_gets_its_pane_after_its_register() {
    let api = start_server().await;
    let lead = uri("riff://mike@pangolin/como-technologies/riff?session=a1");
    let fake = fake_tmux();
    let tmux = Tmux::new(fake.path().join("tmux"), "%0");
    let tail = Program::tail("/bin/riff".as_ref(), "/src/riff".as_ref(), api.base());

    let who = api.who(&lead, false).await.unwrap();
    assert!(
        who.iter()
            .any(|s| s.uri.who() == lead.who() && !s.uri.lead()),
        "{who:?}"
    );
    assert!(
        !terminal::tail_beside_lead(&api, &lead, &tmux, &tail)
            .await
            .unwrap()
    );
    assert_eq!(log(fake.path()), "");

    api.register(&lead).await.unwrap();
    assert!(
        terminal::tail_beside_lead(&api, &lead, &tmux, &tail)
            .await
            .unwrap()
    );
}

/// A server whose `register` takes `wait` longer: each other call
/// comes before the end of the register.
async fn start_server_with_a_slow_register(wait: Duration) -> Api {
    use axum::extract::Request;
    use axum::middleware::{Next, from_fn};
    use riff_core::wire::{Call, Register};

    let slow = move |request: Request, next: Next| async move {
        if request.uri().path() == Register::PATH {
            tokio::time::sleep(wait).await;
        }
        next.run(request).await
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router().layer(from_fn(slow)))
            .await
            .unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// `riff mcp` of the session `a1` in the repository `main`, in the pane
/// `%0` of the fake tmux.
fn mcp_in_tmux(api: &Api, fake: &Path, run: &Path, main: &Path) -> tokio::process::Child {
    let path = format!("{}:{}", fake.display(), std::env::var("PATH").unwrap());
    Isolated::shared()
        .tokio_riff()
        .arg("mcp")
        .current_dir(main)
        .env("PATH", path)
        .env("TMUX", "/tmp/tmux-1000/default,1,0")
        .env("TMUX_PANE", "%0")
        .env("RIFF_HOME", run)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", "a1")
        .env("RIFF_SERVER", api.base())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

/// The order that failed: the server knows the session from a query,
/// with no lead mark, before the register of `riff mcp` ends. The pane
/// still comes (01M3XM68N5M5DKB86W5079X2G9).
#[tokio::test(flavor = "multi_thread")]
async fn riff_mcp_adds_the_tail_pane_when_a_query_comes_before_its_register() {
    let api = start_server_with_a_slow_register(Duration::from_secs(1)).await;
    let fake = fake_tmux();
    let run = tempfile::tempdir().unwrap();
    let (main, _) = repository(run.path());

    // The query that came first.
    let me = session_in(&main, "a1");
    let who = api.who(&me, false).await.unwrap();
    assert!(
        who.iter().any(|s| s.uri.who() == me.who() && !s.uri.lead()),
        "{who:?}"
    );

    let mut mcp = mcp_in_tmux(&api, fake.path(), run.path(), &main);
    let start = Instant::now();
    while !log(fake.path()).contains("@riff tail") {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{}",
            log(fake.path())
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let log = log(fake.path());
    assert_eq!(log.matches("split-window -h").count(), 1, "{log}");
    mcp.kill().await.unwrap();
}

/// The lead runs `riff mcp` in tmux: `riff mcp` adds the tail pane.
#[tokio::test(flavor = "multi_thread")]
async fn riff_mcp_of_the_lead_adds_the_tail_pane_in_tmux() {
    let api = start_server().await;
    let fake = fake_tmux();
    let run = tempfile::tempdir().unwrap();
    let (main, _) = repository(run.path());
    let mut mcp = mcp_in_tmux(&api, fake.path(), run.path(), &main);
    let start = Instant::now();
    while !log(fake.path()).contains("@riff tail") {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{}",
            log(fake.path())
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let log = log(fake.path());
    assert_eq!(log.matches("split-window -h").count(), 1, "{log}");
    assert!(log.contains("' tail"), "{log}");
    mcp.kill().await.unwrap();
}

#[test]
fn the_book_has_a_how_to_for_each_step() {
    let book = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md");
    let book = std::fs::read_to_string(book).unwrap();
    let part = &book[book
        .find("## Run the lead and its workers in tmux")
        .unwrap()..];
    for (heading, command) in [
        ("### Start the lead in tmux", "claude --remote-control"),
        ("### Start workers", "riff workers start 3"),
        ("### Start workers", "riff workers start 1 --claude "),
        (
            "#### The language server of a worker",
            "pgrep -a rust-analyzer",
        ),
        (
            "#### Change the rate of the rollout",
            "riff workers interval 30",
        ),
        ("#### Turn the rollout off", "riff workers interval 0"),
        (
            "### Take over a worker",
            "tmux select-window -t riff-workers",
        ),
        ("### Set the limit of workers", "riff workers limit 3"),
        ("### List the workers", "riff workers\n"),
        ("### Stop the workers", "riff workers stop\n"),
        ("### Stop the workers", "riff workers stop %3"),
        ("### A worker goes to its next item", "riff who\n"),
        ("### Clear a worker by hand", "riff workers stop %3"),
        ("### A worker with no work waits idle", "riff workers\n"),
    ] {
        let how = &part[part.find(heading).unwrap()..];
        let next = how[4..].find("\n### ").map_or(how.len(), |n| n + 4);
        assert!(
            how[..next].contains("```sh\n") && how[..next].contains(command),
            "{heading} has no {command:?}"
        );
    }
    let help = Isolated::shared()
        .riff()
        .args(["workers", "start", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(
        help.contains("--claude <CLAUDE>") && help.contains("<COUNT>"),
        "{help}"
    );
    let help = Isolated::shared()
        .riff()
        .args(["workers", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    for command in ["start", "limit", "stop", "idle"] {
        assert!(help.contains(&format!("  {command} ")), "{help}");
    }
    // riff clears a worker by itself: no command asks for it
    // (01M3XV0562D3H3P22CJDBPAZBH).
    assert!(!help.contains("  next "), "{help}");
    // A worker does not end itself; the server stops idle workers
    // (01M3Q5A0NKY1FCS0YH6N6YD3GN).
    assert!(!help.contains("  done "), "{help}");
}
