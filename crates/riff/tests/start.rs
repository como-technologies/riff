//! `riff` with no command starts the riff (01M4BSSWWEBVHZGXCVYMJ7D7PQ to
//! 01M4BSSX66A2NNVQK48KQH8BEZ). A fake `tmux` on `PATH` writes each call
//! to a log and keeps its sessions in a file. Its `new-session` runs the
//! command of the pane, so a fake `claude` writes its arguments. riff
//! gives the lead its plugin, MCP config and settings
//! (01M4BYH7Y3P1JMQR51TWFGVZ39), and removes the entries of an older riff
//! after a yes (01M4BYH82P03FTXZBYC72BJ6F3).

use isolated::Isolated;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use riff::api::Api;
use riff::identity;
use riff_core::name::{SessionUri, Who};

/// `has-session` succeeds for a name in the file `sessions`.
/// `new-session` adds its name there and runs its last argument.
const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/log"
[ "$1" = -L ] && shift 2
[ "$1" = -f ] && shift 2
case "$1" in
  has-session) grep -qx "$3" "$dir/sessions" 2>/dev/null ;;
  new-session)
    env > "$dir/server.env"
    echo "=$4" >> "$dir/sessions"
    for last; do :; done
    (cd "$6" && sh -c "$last") ;;
  list-panes) exit 0 ;;
  *) exit 0 ;;
esac
"#;

/// Writes the arguments, and the `RIFF_SERVER` and `RIFF_ON` that it
/// got, and its environment. In the sandbox of the lead, it writes them
/// to its own Claude folder, a write path of the lead that stays after
/// the end (01M4DDWP9XSA14E0YF211XZYKR): the lead writes no file of the
/// folder of riff (01M4DWJ08GVFKV4EC6FNDZA1BD). Outside, it writes them
/// to the folder of riff.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
dir="${CLAUDE_CONFIG_DIR:-$RIFF_HOME/state}"
printf '%s\n' "$*" >> "$dir/claude.log"
printf '%s %s\n' "$RIFF_SERVER" "$RIFF_ON" >> "$dir/claude.log"
env > "$dir/claude.env"
"#;

fn script(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
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

/// A clone of `OWNER/REPO` in `root/NAME`.
fn clone(root: &Path, name: &str, repo: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "x"]);
    let url = format!("https://github.com/{repo}.git");
    git(&dir, &["remote", "add", "origin", &url]);
    std::fs::canonicalize(dir).unwrap()
}

/// A machine with the fake `tmux` and `claude` first on `PATH`, a home
/// of riff of its own, and a home of the person of its own.
struct Machine {
    fake: tempfile::TempDir,
    home: tempfile::TempDir,
    user: tempfile::TempDir,
    server: String,
}

impl Machine {
    fn new(server: &str) -> Self {
        let fake = tempfile::tempdir().unwrap();
        script(fake.path(), "tmux", FAKE_TMUX);
        script(fake.path(), "claude", FAKE_CLAUDE);
        Machine {
            fake,
            home: tempfile::tempdir().unwrap(),
            user: tempfile::tempdir().unwrap(),
            server: server.into(),
        }
    }

    /// The root of the plugin that riff writes.
    fn plugin(&self) -> PathBuf {
        self.user
            .path()
            .join(".local/share/riff/claude-plugin/riff")
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.fake.path().join(name)).unwrap_or_default()
    }

    /// A file of the fake `claude`: in the folder of riff, then in the
    /// Claude folder of the lead.
    fn claude(&self, name: &str) -> String {
        let outside = self.home.path().join("state").join(name);
        let dirs = self.user.path().join(".local/share/riff/claude");
        let lead = std::fs::read_dir(dirs)
            .into_iter()
            .flatten()
            .flatten()
            .map(|d| d.path().join(name));
        std::iter::once(outside)
            .chain(lead)
            .filter_map(|f| std::fs::read_to_string(f).ok())
            .collect()
    }

    /// `riff ARGS` in `dir`, outside tmux, with `answer` on stdin.
    fn riff(&self, dir: &Path, args: &[&str], answer: &str) -> Output {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut child = Isolated::shared()
            .riff()
            .args(args)
            .current_dir(dir)
            .env("PATH", path)
            .env("RIFF_HOME", self.home.path())
            .env("HOME", self.user.path())
            .env_remove("XDG_DATA_HOME")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            // Credentials of the person, as markers.
            .env("GH_TOKEN", MARKER)
            .env("ANTHROPIC_API_KEY", MARKER)
            .env("ANTHROPIC_AUTH_TOKEN", MARKER)
            .env("SSH_AUTH_SOCK", MARKER)
            .env("RIFF_TEST_MARKER", MARKER)
            .env_remove("RIFF_SESSION")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("RIFF_WORKER")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(answer.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

/// The value of each credential of the person in [`Machine::riff`].
const MARKER: &str = "person-secret-7f3a";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// `riff` starts the lead of the picked clone in its own tmux server,
/// and a second `riff` attaches and starts no second lead.
#[tokio::test(flavor = "multi_thread")]
async fn riff_starts_one_lead_in_its_own_tmux_server() {
    let api = start_server().await;
    let m = Machine::new(api.base());
    let root = tempfile::tempdir().unwrap();
    let riff = clone(root.path(), "riff", "como-technologies/riff");
    let other = clone(root.path(), "strata", "como-technologies/strata");
    // A live session in the riff repository.
    let place = identity::place_in(&riff, "pangolin").unwrap();
    let me = SessionUri::new(Who::new("mike", Some("a1")).unwrap(), place);
    api.register(&me).await.unwrap();

    let out = m.riff(&riff, &[], "1\n");
    assert!(out.status.success(), "{out:?}");
    let config = m.home.path().join("state").join("tmux.conf");
    let config = config.display();
    assert!(
        stdout(&out).contains(&format!(
            "  1  como-technologies/riff  paused, 1 live session  {}\n",
            riff.display()
        )),
        "{out:?}"
    );
    assert!(
        stdout(&out).contains(&format!(
            "riff started the lead of como-technologies/riff in {}.",
            riff.display()
        )),
        "{out:?}"
    );
    let log = m.read("log");
    let lines: Vec<&str> = log.lines().collect();
    // The rules file of the lead is outside each write path of the lead
    // (01M4BTB7DY1Y74PP3JWKVX58JQ).
    let settings = m
        .user
        .path()
        .join(".local/share/riff/rules/lead-como-technologies-riff.json");
    let mcp = m
        .user
        .path()
        .join(".local/share/riff/given")
        .join(riff::worker_mcp::FILE);
    let plugin = m.plugin();
    assert_eq!(lines.len(), 3, "{log}");
    assert_eq!(
        lines[0],
        format!("-L riff -f {config} has-session -t =como-technologies/riff")
    );
    // The pane runs the lead through `riff workers lead`
    // (01M4C4WQVZR49FDGPJMFW22GTM), with the plugin, the MCP config and
    // the settings that riff gives (01M4BYH80CFW1TBGKVA2VN9ZBQ).
    let start = format!(
        "-L riff -f {config} new-session -d -s como-technologies/riff -c {} \
         -e RIFF_SERVER={} -e RIFF_ON=1 '",
        riff.display(),
        api.base(),
    );
    let end = format!(
        "' 'workers' 'lead' '--name' 'lead-como-technologies-riff' \
         'claude' '--remote-control' '--plugin-dir' '{}' \
         '--strict-mcp-config' '--mcp-config' '{}' '--settings' '{}'",
        plugin.display(),
        mcp.display(),
        settings.display(),
    );
    assert!(
        lines[1].starts_with(&start) && lines[1].ends_with(&end),
        "{log}"
    );
    assert_eq!(
        lines[2],
        format!("-L riff -f {config} attach-session -t =como-technologies/riff")
    );
    // No credential of the person reaches the tmux server of riff or the
    // lead (01M4C4WW15HGA1VEDFRBEMZAW7, 01M4BYVSNQ5SY2GRGT73FV0Z3E).
    let server_env = m.read("server.env");
    assert!(server_env.contains("RIFF_SERVER="), "{server_env}");
    let claude_env = m.claude("claude.env");
    assert!(claude_env.contains("GH_CONFIG_DIR="), "{claude_env}");
    for env in [&server_env, &claude_env] {
        assert!(!env.contains(MARKER), "{env}");
        assert!(!env.lines().any(|l| l.starts_with("TMUX=")), "{env}");
    }
    // The fake claude ran with a session ID of its own
    // (01M4CVXJEQK32SRQ957X6NAEMH), the flags of the lead, the server and
    // RIFF_ON=1 (01M4BYH80CFW1TBGKVA2VN9ZBQ).
    let log = m.claude("claude.log");
    let session = claude_env
        .lines()
        .find_map(|l| l.strip_prefix("RIFF_SESSION="))
        .unwrap_or_else(|| panic!("the lead has no RIFF_SESSION: {claude_env}"));
    let log = log
        .strip_prefix(&format!("--session-id {session} "))
        .unwrap_or_else(|| panic!("no --session-id {session}: {log}"));
    assert_eq!(
        log,
        format!(
            "--remote-control --plugin-dir {} --strict-mcp-config --mcp-config {} \
             --settings {}\n{} 1\n",
            plugin.display(),
            mcp.display(),
            settings.display(),
            api.base()
        )
    );
    // The plugin has the skill and the hooks, and no MCP server.
    assert!(plugin.join("skills/riff/SKILL.md").is_file());
    assert!(plugin.join("hooks/hooks.json").is_file());
    assert!(!plugin.join(".mcp.json").exists());
    // The MCP config has the riff server, and riff wrote nothing to the
    // Claude config of the person.
    let servers: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&mcp).unwrap()).unwrap();
    assert_eq!(
        servers["mcpServers"]["riff"]["args"],
        serde_json::json!(["mcp"])
    );
    assert!(!m.user.path().join(".claude").exists());
    assert!(!riff.join(".claude/settings.json").exists());
    assert!(!riff.join(".claude/settings.local.json").exists());
    // Its flag settings hold the permission rules of the profile of the
    // lead (01M4BT33R71HXAVQGHFD4ZFGR5): it edits the worktrees of the
    // clone and reads the rest of it (01M4DDWPN8FADA663TTZSVD698), and
    // the rules deny the rest of the home.
    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    let rules = |list: &str| -> Vec<String> {
        settings["permissions"][list]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap().to_owned())
            .collect()
    };
    let edit = format!("Edit(/{}/.claude/worktrees/**)", riff.display());
    assert!(rules("allow").contains(&edit), "{settings}");
    let read = format!("Read(/{}/**)", riff.display());
    assert!(rules("allow").contains(&read), "{settings}");
    let clone = format!("Edit(/{}/**)", riff.display());
    assert!(!rules("allow").contains(&clone), "{settings}");
    assert!(!rules("deny").is_empty(), "{settings}");
    // The status line and the rules of riff work
    // (01M4BYH874WQ16Q0337WQA8AMV).
    assert_eq!(settings["statusLine"]["command"], "riff statusline");
    // The plugin of an older riff is off (01M4CMN13D97R2JKHYGFSAM313).
    assert_eq!(settings["enabledPlugins"]["riff@riff"], false);
    assert!(
        rules("allow").contains(&"Bash(riff *)".to_owned()),
        "{settings}"
    );
    assert!(
        rules("allow").contains(&"mcp__riff".to_owned()),
        "{settings}"
    );
    assert!(
        rules("deny").contains(&"Bash(git push * main)".to_owned()),
        "{settings}"
    );
    let text = std::fs::read_to_string(m.home.path().join("state").join("tmux.conf")).unwrap();
    assert_eq!(text, riff::start::CONFIG);

    // A second riff, from another clone: the picker knows the first
    // clone. Picking it attaches, and starts no second lead.
    let out = m.riff(&other, &[], "1\n");
    assert!(out.status.success(), "{out:?}");
    assert!(
        stdout(&out).contains("  2  como-technologies/strata  paused, 0 live sessions"),
        "{out:?}"
    );
    assert!(
        stdout(&out).contains("The lead of como-technologies/riff runs. riff shows it."),
        "{out:?}"
    );
    let log = m.read("log");
    assert_eq!(log.matches("new-session").count(), 1, "{log}");
    assert_eq!(log.matches("attach-session").count(), 2, "{log}");
    let log = m.claude("claude.log");
    assert_eq!(log.lines().count(), 2);

    // The new clone is known now too.
    let clones = std::fs::read_to_string(m.home.path().join("state").join("clones")).unwrap();
    assert_eq!(clones, format!("{}\n", riff.display()));
}

/// `riff` lists the entries of an older riff in the Claude config, and
/// removes them only after a yes. Each other entry stays
/// (01M4BYH82P03FTXZBYC72BJ6F3). A `.claude/settings.json` that git
/// tracks stays byte for byte: riff lists its entries for a pull
/// request (01M4CMJPGS613K2FHQ6DKSY2WJ).
#[tokio::test(flavor = "multi_thread")]
async fn riff_removes_the_entries_of_an_older_riff_after_a_yes() {
    let api = start_server().await;
    let m = Machine::new(api.base());
    let root = tempfile::tempdir().unwrap();
    let riff = clone(root.path(), "riff", "como-technologies/riff");
    let claude = m.user.path().join(".claude");
    let write = |path: &Path, text: &str| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    let user = claude.join("settings.json");
    write(
        &user,
        r#"{"theme": "dark",
            "enabledPlugins": {"riff@riff": true, "x@y": true},
            "extraKnownMarketplaces": {"riff": {"source": {"source": "directory"}}},
            "statusLine": {"type": "command", "command": "riff statusline"},
            "permissions": {"allow": ["Bash(riff *)", "Bash(ls)"]}}"#,
    );
    let local = riff.join(".claude/settings.local.json");
    write(&local, r#"{"enabledPlugins": {"riff@riff": true}}"#);
    let shared = riff.join(".claude/settings.json");
    write(
        &shared,
        r#"{"permissions": {"allow": ["Bash(gh pr view *)", "Bash(make)"],
            "deny": ["Bash(git push * main)"]}}"#,
    );
    git(&riff, &["add", ".claude/settings.json"]);
    git(&riff, &["commit", "-q", "-m", "settings"]);
    write(
        &claude.join("plugins/installed_plugins.json"),
        r#"{"version": 2, "plugins": {"riff@riff": [{"scope": "user"}]}}"#,
    );
    write(
        &claude.join("plugins/known_marketplaces.json"),
        r#"{"riff": {}}"#,
    );
    // The edit keeps the mode of each file of the person.
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    for path in [&user, &local] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let before = |path: &Path| std::fs::read_to_string(path).unwrap();
    let (user_text, local_text, shared_text) = (before(&user), before(&local), before(&shared));

    // Enter is no, and keeps each entry. The person then picks no
    // repository.
    let out = m.riff(&riff, &[], "\n\n");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let shown = stdout(&out);
    for line in [
        "An older riff wrote these entries to files that git tracks:".to_owned(),
        "An older riff wrote these entries to the Claude config:".to_owned(),
        format!("  {}: enabledPlugins.\"riff@riff\"", user.display()),
        format!("  {}: extraKnownMarketplaces.\"riff\"", user.display()),
        format!("  {}: statusLine", user.display()),
        format!("  {}: permissions.allow: Bash(riff *)", user.display()),
        format!("  {}: enabledPlugins.\"riff@riff\"", local.display()),
        format!(
            "  {}: permissions.allow: Bash(gh pr view *)",
            shared.display()
        ),
        format!(
            "  {}: permissions.deny: Bash(git push * main)",
            shared.display()
        ),
        "  the plugin riff@riff, installed in the scope user".to_owned(),
        "  the plugin marketplace riff".to_owned(),
        riff::text::OLD_CONFIG_KEPT.to_owned(),
    ] {
        assert!(shown.contains(&line), "{line}: {shown}");
    }
    assert!(
        !shown.contains("Bash(ls)") && !shown.contains("Bash(make)"),
        "{shown}"
    );
    assert_eq!(before(&user), user_text);
    assert_eq!(before(&local), local_text);
    assert_eq!(before(&shared), shared_text);
    assert_eq!(m.claude("claude.log"), "");

    // A yes removes them, and riff starts the lead.
    let out = m.riff(&riff, &[], "y\n1\n");
    assert!(out.status.success(), "{out:?}");
    assert!(
        stdout(&out).contains(riff::text::OLD_CONFIG_REMOVED),
        "{out:?}"
    );
    let json = |path: &Path| -> serde_json::Value { serde_json::from_str(&before(path)).unwrap() };
    assert_eq!(
        json(&user),
        serde_json::json!({"theme": "dark", "enabledPlugins": {"x@y": true},
            "permissions": {"allow": ["Bash(ls)"]}})
    );
    assert_eq!(json(&local), serde_json::json!({}));
    assert_eq!(before(&shared), shared_text);
    assert_eq!(mode(&user), 0o600);
    assert_eq!(mode(&local), 0o600);
    let calls = m.claude("claude.log");
    let plugin: Vec<&str> = calls.lines().filter(|l| l.starts_with("plugin ")).collect();
    assert_eq!(
        plugin,
        [
            "plugin uninstall riff@riff --scope user",
            "plugin marketplace remove riff"
        ],
        "{calls}"
    );
    assert!(calls.contains("--remote-control"), "{calls}");
}

/// A path of a new clone starts its lead. An answer that picks nothing
/// starts nothing.
#[test]
fn riff_takes_the_path_of_a_new_clone() {
    let m = Machine::new("http://127.0.0.1:9");
    let root = tempfile::tempdir().unwrap();
    let new = clone(root.path(), "new", "como-technologies/new");
    let out = m.riff(root.path(), &[], "\n");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        stdout(&out).contains("riff knows no clone on this machine."),
        "{out:?}"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("you picked no repository"),
        "{out:?}"
    );
    assert!(!m.read("log").contains("new-session"));

    let out = m.riff(root.path(), &[], "new\n");
    assert!(out.status.success(), "{out:?}");
    assert!(m.read("log").contains(&format!(
        "new-session -d -s como-technologies/new -c {}",
        new.display()
    )));
}

/// A worker never starts the riff.
#[test]
fn a_worker_starts_no_riff() {
    let m = Machine::new("http://127.0.0.1:9");
    let root = tempfile::tempdir().unwrap();
    let out = {
        let path = format!(
            "{}:{}",
            m.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        Isolated::shared()
            .riff()
            .current_dir(root.path())
            .env("PATH", path)
            .env("RIFF_HOME", m.home.path())
            .env("RIFF_WORKER", "1")
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("a worker never starts the riff"));
    assert_eq!(m.read("log"), "");
}

/// Outside tmux, `riff workers` lists the panes of the tmux server of
/// riff (01M4BSSX66A2NNVQK48KQH8BEZ).
#[test]
fn riff_workers_outside_tmux_reads_the_tmux_server_of_riff() {
    let m = Machine::new("http://127.0.0.1:9");
    let root = tempfile::tempdir().unwrap();
    let _ = m.riff(root.path(), &["workers"], "");
    assert!(
        m.read("log")
            .lines()
            .any(|l| l.starts_with("-L riff list-panes -a")),
        "{}",
        m.read("log")
    );
}
