//! `riff run ITEM`: one work item in one rootless podman container,
//! with no daemon (#712, the first slice of riff 3.0).
//!
//! # Design
//!
//! The host does each riff step with the riff of today. The container
//! does the work. No riff runs in the container in this slice
//! (01M4Z81APKC14AVNJWCGRQKNVW).
//!
//! `riff run` drives the container through the trait [`Runtime`]
//! ([`run_with`]). On Linux, the runtime is [`crate::podman::Podman`].
//! The tests take [`crate::runtime::Fake`] (01M4ZBGK5G3EFXWT61DR2NHMDD).
//! On a host that is not Linux, `riff run` starts nothing
//! (01M4ZBGK7SFEXE1AGH6Z0YQHJP).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant H as riff run (host)
//!     participant S as riff-server
//!     participant P as Runtime (podman)
//!     participant C as claude -p (container)
//!     participant G as GitHub
//!     H->>S: start a worker session, claim ITEM
//!     H->>H: git clone --reference MAIN, the branch worktree-ITEM
//!     H->>P: start SPEC, with the value of GH_TOKEN beside it
//!     P->>C: start
//!     H->>C: the start text, one stream-json line on the input
//!     C-->>H: the events, one line each
//!     C->>G: push the branch, open the pull request
//!     C-->>H: the result of the turn
//!     H->>C: close the input: claude ends, the container goes
//!     H->>G: find the pull request of the branch
//!     H->>S: a verify request to the lead, release ITEM, end the session
//! ```
//!
//! Before the claim, riff checks each fact that it needs: the image of
//! the session (podman pulls it when it must), the sign-in of `gh`, the sign-in of `claude`, `HOME` and
//! `USER`, and the milestone of the issue. A failed check takes no
//! claim (01M4Z81B3A324KKE8M8Z71TM4M).
//!
//! The image is the one that `riff.toml` of the main clone pins by its
//! digest ([`Image::of_repository`]), or the one of `--image`, for
//! example a local build. When podman does not have the image, riff
//! pulls it before the claim (01M4Z81B5A456HDKW7G0WFGCY6).
//!
//! When `claude` ends, riff finds the open pull request of the branch,
//! sends a verify request to the lead, releases the item and ends the
//! session. A SIGINT or a SIGTERM stops the container first
//! (01M4Z81B1913NF6HBZ5G38B3Q9).
//!
//! # The folder of a session
//!
//! Each session has a folder of its own in [`sessions_dir`]: `ITEM-ID8`,
//! where `ID8` is the start of the session ID (01M4Z81ARSTAK8HR96S084SDFD).
//!
//! | Path | Holds |
//! |---|---|
//! | `clone/` | The clone of the session: `git clone --reference MAIN`. It stays until the merge. |
//! | `claude/` | The own Claude folder of the session, with the trust of the clone ([`trust`]). |
//! | `events.jsonl` | Each event of `claude`, with no secret ([`Scrub`]). |
//!
//! # The spec of the container
//!
//! [`spec`] makes the [`SessionSpec`] of decision 10 of #699 from the
//! [`Facts`] of the host. [`crate::podman::run_args`] makes the
//! arguments of `podman`. riff builds no shell string: each value is one
//! argument (01M4Z60B8KDXD190RWC7D184Q3).
//!
//! | Part | Value | Requirement |
//! |---|---|---|
//! | The user | `--userns=keep-id` | 01M4Z60B7QDWEFQ96H448RE9N2 |
//! | The home | a tmpfs at the path of `HOME`, `HOME`, `USER`, `LOGNAME` | 01M4Z60B7SHAMXXFK15BE3KZSP, 01M4Z60B7S8V9KKZPNBCQ6M99N |
//! | The clone | with write, at the same path, the working folder | 01M4Z60B7PTJMB37578K8FV22Q |
//! | The objects of the main clone | no write, at the same path | 01M4Z60B7PQ07HRJBXS1TA73PX |
//! | `~/.gitconfig`, `~/.config/gh` | no write, when they exist | 01M4Z60B7QDWEFQ96H448RE9N2 |
//! | The sign-in of `gh` | `-e GH_TOKEN`: the name only | 01M4Z60B89BHXQVJ2MGRRQ2CKJ |
//! | No signature | `commit.gpgsign=false` by `GIT_CONFIG_*` | 01M4Z60B89G8YNVMTEPGX7FQCV |
//! | The Claude folder | with write; the sign-in file of the host with no write | 01M4Z60B86N999JRN7Z1G52PG4, 01M4Z60B869HWDTZMSYPVYACD8 |
//! | The caches | the volumes `riff-cargo` and `riff-sccache` | 01M4Z60B7RPSQ1ABGVMJSZWS5P |
//! | The limits | `--memory 8g`, `--cpus 4` | 01M4Z60B7XT7462XMV49GJTVSJ |
//! | `claude` | `-p` in stream-json, the riff session ID, the auto mode | 01M4Z60B82FHN2YG9YJHW8XZX1, 01M4Z81ATZK3FX9GRFTM236FE6 |

use std::borrow::Cow;
use std::ffi::OsString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, anyhow, bail};
use riff_core::name::{Place, SessionUri};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, StartReason, WORKER_ALIVE_EVERY};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::signal::unix::{SignalKind, signal};

use crate::api::{self, Api};
use crate::runtime::{Bind, Env, Exit, Running, Runtime, SessionSpec, Source};
use crate::session_image::Image;
use crate::{dropped, home, identity, pr, rollout, terminal, text};

/// The memory limit of each container (01M4Z60B7XT7462XMV49GJTVSJ).
pub const MEMORY: &str = "8g";

/// The CPU limit of each container (01M4Z60B7XT7462XMV49GJTVSJ).
pub const CPUS: u32 = 4;

/// The named volume for all of `CARGO_HOME` (01M4Z60B7RPSQ1ABGVMJSZWS5P).
pub const CARGO_VOLUME: &str = "riff-cargo";

/// The named volume for the folder of sccache.
pub const SCCACHE_VOLUME: &str = "riff-sccache";

/// riff starts no session while the access token of `claude` has less
/// than this many minutes left (01M4Z60B869HWDTZMSYPVYACD8).
pub const MIN_MINUTES: i64 = 60;

/// The text that takes the place of a secret.
pub const HIDDEN: &str = "[secret]";

/// The facts of the host and of the session that make a [`SessionSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// The riff session ID. `claude` takes it as its own session ID.
    pub session: String,
    /// The work item, for example `issue-12`.
    pub item: String,
    /// The image.
    pub image: String,
    /// The home of the person.
    pub home: PathBuf,
    /// The user name of the person.
    pub user: String,
    /// The clone of the session.
    pub clone: PathBuf,
    /// The objects of the main clone: `MAIN/.git/objects`.
    pub objects: PathBuf,
    /// The own Claude folder of the session.
    pub claude: PathBuf,
    /// The sign-in file of `claude` on the host.
    pub credentials: PathBuf,
    /// `~/.gitconfig`, when it exists.
    pub gitconfig: Option<PathBuf>,
    /// `~/.config/gh`, when it exists.
    pub gh_config: Option<PathBuf>,
}

/// The [`SessionSpec`] of a session: decision 10 of #699, with the
/// facts of #707.
///
/// ```
/// use riff::run::{Facts, spec};
/// use riff::runtime::{Env, Source};
///
/// let facts = Facts {
///     session: "1a2b3c4d-0000-4000-8000-000000000000".into(),
///     item: "issue-12".into(),
///     image: "localhost/riff-session:latest".into(),
///     home: "/home/mike".into(),
///     user: "mike".into(),
///     clone: "/data/riff/sessions/issue-12-1a2b3c4d/clone".into(),
///     objects: "/src/riff/.git/objects".into(),
///     claude: "/data/riff/sessions/issue-12-1a2b3c4d/claude".into(),
///     credentials: "/home/mike/.claude/.credentials.json".into(),
///     gitconfig: Some("/home/mike/.gitconfig".into()),
///     gh_config: None,
/// };
/// let spec = spec(&facts);
/// assert_eq!(spec.name, "riff-issue-12-1a2b3c4d");
/// assert_eq!(spec.workdir, facts.clone);
/// // The objects of the main clone: at the same path, with no write.
/// let objects = spec.binds.iter().find(|b| b.target == facts.objects).unwrap();
/// assert_eq!((&objects.source, objects.write), (&Source::Path(facts.objects.clone()), false));
/// // The token of gh goes by its name only.
/// assert!(spec.env.contains(&Env::Host("GH_TOKEN".into())));
/// assert!(spec.command.windows(2).any(|w| w == ["--session-id", facts.session.as_str()]));
/// ```
pub fn spec(facts: &Facts) -> SessionSpec {
    let home = &facts.home;
    let path = |p: &Path, write: bool| Bind {
        source: Source::Path(p.to_owned()),
        target: p.to_owned(),
        write,
    };
    let volume = |name: &str, target: PathBuf| Bind {
        source: Source::Volume(name.into()),
        target,
        write: true,
    };
    let claude_dir = home.join(".claude");
    let cargo = home.join(".cargo");
    let sccache = home.join(".cache/sccache");
    let mut binds = vec![path(&facts.clone, true), path(&facts.objects, false)];
    binds.extend(facts.gitconfig.iter().map(|p| Bind {
        source: Source::Path(p.clone()),
        target: home.join(".gitconfig"),
        write: false,
    }));
    binds.extend(facts.gh_config.iter().map(|p| Bind {
        source: Source::Path(p.clone()),
        target: home.join(".config/gh"),
        write: false,
    }));
    binds.push(Bind {
        source: Source::Path(facts.claude.clone()),
        target: claude_dir.clone(),
        write: true,
    });
    binds.push(Bind {
        source: Source::Path(facts.credentials.clone()),
        target: claude_dir.join(".credentials.json"),
        write: false,
    });
    binds.push(volume(CARGO_VOLUME, cargo.clone()));
    binds.push(volume(SCCACHE_VOLUME, sccache.clone()));
    let value = |name: &str, value: &str| Env::Value(name.into(), value.into());
    let shown = |p: &Path| p.to_string_lossy().into_owned();
    let env = vec![
        value("HOME", &shown(home)),
        value("USER", &facts.user),
        value("LOGNAME", &facts.user),
        Env::Host("GH_TOKEN".into()),
        value("GIT_CONFIG_COUNT", "1"),
        value("GIT_CONFIG_KEY_0", "commit.gpgsign"),
        value("GIT_CONFIG_VALUE_0", "false"),
        value("CLAUDE_CONFIG_DIR", &shown(&claude_dir)),
        value("CARGO_HOME", &shown(&cargo)),
        value("SCCACHE_DIR", &shown(&sccache)),
        value("RUSTC_WRAPPER", "sccache"),
        value("SCCACHE_IDLE_TIMEOUT", "0"),
        value("CARGO_BUILD_JOBS", &CPUS.to_string()),
    ];
    let command = [
        "claude",
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--session-id",
        &facts.session,
        "--permission-mode",
        "auto",
    ]
    .map(String::from)
    .to_vec();
    SessionSpec {
        name: format!("riff-{}-{}", facts.item, short(&facts.session)),
        image: facts.image.clone(),
        home: home.clone(),
        workdir: facts.clone.clone(),
        binds,
        env,
        memory: MEMORY.into(),
        cpus: CPUS,
        command,
    }
}

/// The first 8 characters of a session ID.
///
/// ```
/// assert_eq!(riff::run::short("1a2b3c4d-0000"), "1a2b3c4d");
/// assert_eq!(riff::run::short("ab"), "ab");
/// ```
pub fn short(session: &str) -> &str {
    session.get(..8).unwrap_or(session)
}

/// The branch of the session of `item`: the branch name of a worker of
/// riff 1.3, so that its tools find the work (01M4Z81AX3ND5RX7V1GEY4HMF5).
///
/// ```
/// assert_eq!(riff::run::branch("issue-12"), "worktree-issue-12");
/// ```
pub fn branch(item: &str) -> String {
    format!("{}{item}", dropped::BRANCH)
}

/// The folder of the session folders: `RIFF_HOME/sessions`, else
/// `XDG_DATA_HOME/riff/sessions`, else `HOME/.local/share/riff/sessions`.
///
/// ```
/// use riff::run::sessions_dir_from;
/// use std::path::Path;
///
/// let some = |s: &str| Some(s.into());
/// assert_eq!(sessions_dir_from(some("/r"), some("/d"), some("/h"))?, Path::new("/r/sessions"));
/// assert_eq!(sessions_dir_from(None, some("/d"), some("/h"))?, Path::new("/d/riff/sessions"));
/// assert_eq!(
///     sessions_dir_from(None, some(""), some("/h"))?,
///     Path::new("/h/.local/share/riff/sessions")
/// );
/// assert!(sessions_dir_from(None, None, None).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn sessions_dir_from(
    riff_home: Option<OsString>,
    data_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf> {
    if let Some(dir) = home::dir_from(riff_home) {
        return Ok(dir.join("sessions"));
    }
    let set = |v: Option<OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    match (set(data_home), set(home)) {
        (Some(data), _) => Ok(data.join("riff/sessions")),
        (None, Some(home)) => Ok(home.join(".local/share/riff/sessions")),
        (None, None) => bail!("set HOME or XDG_DATA_HOME"),
    }
}

/// [`sessions_dir_from`] with the values of this process.
pub fn sessions_dir() -> Result<PathBuf> {
    sessions_dir_from(
        std::env::var_os(home::VAR),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
    )
}

/// The `.claude.json` of the own Claude folder: it trusts the clone, as
/// the person trusts the main clone (01M4Z60B874MBWZWDP8YDEM3HR).
///
/// ```
/// let trust = riff::run::trust("/data/clone".as_ref());
/// assert_eq!(trust["projects"]["/data/clone"]["hasTrustDialogAccepted"], true);
/// ```
pub fn trust(clone: &Path) -> Value {
    let mut projects = serde_json::Map::new();
    projects.insert(
        clone.to_string_lossy().into_owned(),
        json!({ "hasTrustDialogAccepted": true }),
    );
    json!({ "projects": projects })
}

/// The sign-in of `claude` in its `.credentials.json`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuth {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    claude_ai_oauth: OAuth,
}

/// What riff reads from the sign-in file of `claude`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignIn {
    /// The minutes that the access token has left.
    pub minutes: i64,
    /// The tokens: they must not go into a log or onto the terminal.
    pub secrets: Vec<String>,
}

/// Reads the sign-in file of `claude` at the time `now_ms`.
///
/// ```
/// let file = r#"{"claudeAiOauth":{"accessToken":"sk-a","refreshToken":"sk-r","expiresAt":7200000}}"#;
/// let sign_in = riff::run::sign_in(file, 0)?;
/// assert_eq!(sign_in.minutes, 120);
/// assert_eq!(sign_in.secrets, ["sk-a", "sk-r"]);
/// assert!(riff::run::sign_in("{}", 0).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn sign_in(file: &str, now_ms: i64) -> Result<SignIn> {
    let credentials: Credentials =
        serde_json::from_str(file).context("the sign-in file of claude has no claudeAiOauth")?;
    let oauth = credentials.claude_ai_oauth;
    Ok(SignIn {
        minutes: (oauth.expires_at - now_ms) / 60_000,
        secrets: [oauth.access_token, oauth.refresh_token]
            .into_iter()
            .flatten()
            .collect(),
    })
}

/// Takes each secret out of a text (01M4Z60B8KDXD190RWC7D184Q3).
///
/// ```
/// use riff::run::Scrub;
///
/// let scrub = Scrub::new(["gho_secret1".to_owned(), "".to_owned()]);
/// assert_eq!(scrub.clean("token gho_secret1 here"), "token [secret] here");
/// assert_eq!(scrub.clean("no token"), "no token");
/// ```
#[derive(Debug, Clone, Default)]
pub struct Scrub {
    secrets: Vec<String>,
}

impl Scrub {
    /// A scrub of `secrets`. It ignores a value of fewer than 4
    /// characters: such a value is no token.
    pub fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        Self {
            secrets: secrets.into_iter().filter(|s| s.len() >= 4).collect(),
        }
    }

    /// `text` with each secret replaced by [`HIDDEN`].
    pub fn clean<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let mut out = Cow::Borrowed(text);
        for secret in &self.secrets {
            if out.contains(secret.as_str()) {
                out = Cow::Owned(out.replace(secret.as_str(), HIDDEN));
            }
        }
        out
    }
}

/// The one stream-json line that gives `claude` the text of a person.
///
/// ```
/// let line = riff::run::user_line("Do it.");
/// assert_eq!(
///     line,
///     r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Do it."}]}}"#
/// );
/// ```
pub fn user_line(text: &str) -> String {
    json!({
        "type": "user",
        "message": { "role": "user", "content": [{ "type": "text", "text": text }] },
    })
    .to_string()
}

/// The most characters of one shown part of an event.
const SHOWN: usize = 160;

/// The first line of `text`, at most [`SHOWN`] characters.
fn first(text: &str) -> String {
    let line = text.trim().lines().next().unwrap_or("");
    match line.char_indices().nth(SHOWN) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}

/// A short form of the input of a tool call.
fn tool_input(input: &Value) -> String {
    for key in [
        "command",
        "file_path",
        "pattern",
        "url",
        "description",
        "prompt",
    ] {
        if let Some(value) = input[key].as_str() {
            return first(value);
        }
    }
    String::new()
}

/// The lines that the terminal shows for one event of `claude`, or none.
///
/// ```
/// use riff::run::event_lines;
/// use serde_json::json;
///
/// let init = json!({"type":"system","subtype":"init","model":"claude-opus-5-5","permissionMode":"auto"});
/// assert_eq!(event_lines(&init), ["start: model claude-opus-5-5, permission mode auto"]);
///
/// let call = json!({"type":"assistant","message":{"content":[
///     {"type":"text","text":"I read the issue.\nThen I edit."},
///     {"type":"tool_use","name":"Bash","input":{"command":"gh issue view 12"}}]}});
/// assert_eq!(event_lines(&call), ["I read the issue.", "> Bash: gh issue view 12"]);
///
/// let failed = json!({"type":"user","message":{"content":[
///     {"type":"tool_result","is_error":true,"content":"exit 1"}]}});
/// assert_eq!(event_lines(&failed), ["! the tool failed: exit 1"]);
///
/// let result = json!({"type":"result","subtype":"success","num_turns":8,
///     "duration_ms":22700,"total_cost_usd":0.169});
/// assert_eq!(event_lines(&result), ["end: success, 8 turns, 22 s, 0.17 USD"]);
///
/// assert!(event_lines(&json!({"type":"rate_limit_event"})).is_empty());
/// ```
pub fn event_lines(event: &Value) -> Vec<String> {
    let str_of = |key: &str| event[key].as_str().unwrap_or("?");
    match (str_of("type"), event["subtype"].as_str()) {
        ("system", Some("init")) => vec![format!(
            "start: model {}, permission mode {}",
            str_of("model"),
            str_of("permissionMode")
        )],
        ("system", Some(subtype @ ("permission_denied" | "vcs_state_changed"))) => {
            let mut parts: Vec<String> = event
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(k, _)| !matches!(k.as_str(), "type" | "subtype" | "session_id" | "uuid"))
                .filter_map(|(k, v)| v.as_str().map(|v| format!("{k} {}", first(v))))
                .collect();
            parts.sort();
            vec![format!("{subtype}: {}", parts.join(", "))]
        }
        ("assistant", _) => blocks(event)
            .filter_map(|block| match block["type"].as_str() {
                Some("text") => Some(first(block["text"].as_str().unwrap_or(""))),
                Some("tool_use") => Some(format!(
                    "> {}: {}",
                    block["name"].as_str().unwrap_or("?"),
                    tool_input(&block["input"])
                )),
                _ => None,
            })
            .filter(|line| !line.is_empty())
            .collect(),
        ("user", _) => blocks(event)
            .filter(|block| block["type"] == "tool_result" && block["is_error"] == true)
            .map(|block| {
                format!(
                    "! the tool failed: {}",
                    first(&content_text(&block["content"]))
                )
            })
            .collect(),
        ("result", _) => vec![format!(
            "end: {}, {} turns, {} s, {:.2} USD",
            str_of("subtype"),
            event["num_turns"].as_u64().unwrap_or(0),
            event["duration_ms"].as_u64().unwrap_or(0) / 1000,
            event["total_cost_usd"].as_f64().unwrap_or(0.0)
        )],
        _ => Vec::new(),
    }
}

/// The content blocks of the message of an event.
fn blocks(event: &Value) -> impl Iterator<Item = &Value> {
    event["message"]["content"].as_array().into_iter().flatten()
}

/// The text of the content of a tool result: a text, or a list of
/// text blocks.
fn content_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

/// The issue number of an item `issue-N`. `riff run` takes only an
/// issue (01M4Z81AZ88ZS0HPHV5KGQF3HV).
///
/// ```
/// assert_eq!(riff::run::issue_of("issue-12")?, 12);
/// assert!(riff::run::issue_of("verify-issue-12").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn issue_of(item: &str) -> Result<u64> {
    item.strip_prefix("issue-")
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| anyhow!("{}", text::run_not_an_issue(item)))
}

/// Runs `program ARGS` in `dir`, and gives its stdout. An error names
/// the program and its stderr.
fn output(program: &str, dir: &Path, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .with_context(|| text::no_program(program))?;
    if !out.status.success() {
        bail!(
            "{program} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The facts of the host that `riff run` checks before the claim.
struct Checked {
    milestone: String,
    gh_token: String,
    sign_in: SignIn,
    home: PathBuf,
    user: String,
    credentials: PathBuf,
}

/// Pulls `image` when the runtime does not have it
/// (01M4Z81B5A456HDKW7G0WFGCY6).
async fn have_image(runtime: &impl Runtime, image: &str) -> Result<()> {
    if runtime.has_image(image).await? {
        return Ok(());
    }
    println!("{}", text::run_pulling(image));
    runtime
        .pull(image)
        .await
        .with_context(|| text::run_no_image(image))
}

/// Checks each fact that a session needs, before the claim.
async fn check(runtime: &impl Runtime, main: &Path, issue: u64, image: &str) -> Result<Checked> {
    have_image(runtime, image).await?;
    let gh_token = output("gh", main, &["auth", "token"])
        .map_err(|e| anyhow!("{}: {e:#}", text::RUN_NO_GH))?;
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("set HOME"))?;
    let user = std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .ok_or_else(|| anyhow!("set USER"))?;
    let claude_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|d| !d.is_empty())
        .map_or_else(|| home.join(".claude"), PathBuf::from);
    let credentials = claude_dir.join(".credentials.json");
    let file = std::fs::read_to_string(&credentials)
        .with_context(|| text::run_no_sign_in(&credentials))?;
    let sign_in = sign_in(&file, now_ms())?;
    if sign_in.minutes < MIN_MINUTES {
        bail!("{}", text::run_sign_in_ends(sign_in.minutes));
    }
    let found: hygiene::Issue = pr::Gh::default().json(&[
        "issue",
        "view",
        &issue.to_string(),
        "--json",
        "number,state,milestone",
    ])?;
    let milestone = found
        .milestone
        .map(|m| m.title)
        .ok_or_else(|| anyhow!("{}", text::run_no_milestone(issue)))?;
    // The milestone goes into the start text and into a command of the
    // session: a text of the forge that `text::forge` changes is
    // refused whole, as `riff pr open` does.
    if text::forge(&milestone) != milestone {
        bail!("{}", text::run_no_milestone(issue));
    }
    Ok(Checked {
        milestone,
        gh_token,
        sign_in,
        home,
        user,
        credentials,
    })
}

/// The time now in milliseconds since the epoch.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The main clone of the working folder `dir`: its top, and its git dir.
fn main_clone(dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let common = PathBuf::from(output(
        "git",
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    let top = common
        .parent()
        .ok_or_else(|| anyhow!("the git dir {} has no parent", common.display()))?
        .to_owned();
    Ok((top, common))
}

/// Makes the clone of the session on the branch of the item. It goes
/// on from the branch on `origin` when there is one. True when it did.
fn make_clone(main: &Path, clone: &Path, branch: &str) -> Result<bool> {
    let remote = output("git", main, &["remote", "get-url", "origin"])?;
    let top = main.to_string_lossy();
    let path = clone.to_string_lossy();
    output(
        "git",
        main,
        &["clone", "-q", "--reference", &top, "--", &remote, &path],
    )?;
    let earlier = output(
        "git",
        clone,
        &[
            "rev-parse",
            "-q",
            "--verify",
            &format!("refs/remotes/origin/{branch}"),
        ],
    )
    .is_ok();
    let switch: &[&str] = if earlier {
        &["switch", "-q", branch]
    } else {
        &["switch", "-q", "-c", branch]
    };
    output("git", clone, switch)?;
    Ok(earlier)
}

/// How a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ended {
    /// The exit of the command of the session.
    exit: Exit,
    /// True when the turn of `claude` ended with an error, or a signal
    /// stopped the container.
    failed: bool,
}

/// [`drive`], where a SIGINT or a SIGTERM stops the container.
async fn run_session(
    runtime: &impl Runtime,
    spec: &SessionSpec,
    start: &str,
    events: &Path,
    scrub: &Scrub,
    values: &[(String, String)],
) -> Result<Ended> {
    // The handlers come first: a signal before them ends riff with no
    // release.
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let signalled = async move {
        tokio::select! {
            _ = term.recv() => {}
            _ = int.recv() => {}
        }
    };
    drive(runtime, spec, start, events, scrub, values, signalled).await
}

/// Starts the session of `spec` with the `values` of the host, gives
/// `claude` the start text, shows its events, and waits for its end. When
/// `stop` ends first, it stops the container.
async fn drive(
    runtime: &impl Runtime,
    spec: &SessionSpec,
    start: &str,
    events: &Path,
    scrub: &Scrub,
    values: &[(String, String)],
    stop: impl Future<Output = ()>,
) -> Result<Ended> {
    let Running {
        name,
        input,
        events: output,
        errors,
        end,
    } = runtime.start(spec, values).await?;
    let mut input = Some(input);
    if let Some(input) = input.as_mut() {
        input
            .write_all(format!("{}\n", user_line(start)).as_bytes())
            .await?;
        input.flush().await?;
    }
    let errors = {
        let scrub = scrub.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(errors).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("{}", scrub.clean(&line));
            }
        })
    };
    let mut log = tokio::fs::File::create(events)
        .await
        .with_context(|| format!("cannot write {}", events.display()))?;
    let mut lines = BufReader::new(output).lines();
    let mut stopped = false;
    let mut failed = false;
    tokio::pin!(stop);
    loop {
        let line = tokio::select! {
            line = lines.next_line() => line?,
            () = &mut stop, if !stopped => {
                stopped = true;
                println!("{}", text::run_stopping(&name));
                let _ = runtime.stop(&name).await;
                continue;
            }
        };
        let Some(line) = line else { break };
        let line = scrub.clean(&line);
        log.write_all(format!("{line}\n").as_bytes()).await?;
        match serde_json::from_str::<Value>(&line) {
            Ok(event) => {
                for shown in event_lines(&event) {
                    println!("{shown}");
                }
                if event["type"] == "result" {
                    failed |= event["is_error"] == true;
                    // The end of the one turn: claude ends when its
                    // input closes.
                    input = None;
                }
            }
            Err(_) => println!("{line}"),
        }
    }
    drop(input);
    log.flush().await?;
    let exit = end.wait().await?;
    let _ = errors.await;
    Ok(Ended {
        exit,
        failed: failed || stopped,
    })
}

/// A pull request as `gh pr view --json number,url,headRefOid,state`
/// gives it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pull {
    number: u64,
    url: String,
    head_ref_oid: String,
    state: String,
}

/// `riff run ITEM`: one work item in one container, from the claim to
/// the pull request. With no `image`, it takes the image that the main
/// clone pins. It gives the exit code of the command. On Linux, it runs
/// [`run_with`] with podman. On a host that is not Linux, it starts
/// nothing (01M4ZBGK7SFEXE1AGH6Z0YQHJP).
pub async fn run(
    api: Api,
    here: &Place,
    server: &str,
    item: &str,
    image: Option<&str>,
) -> Result<i32> {
    #[cfg(target_os = "linux")]
    {
        run_with(&crate::podman::Podman, api, here, server, item, image).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (api, here, server, item, image);
        bail!("{}", text::RUN_LINUX_ONLY)
    }
}

/// [`run`] with the runtime `runtime` (01M4ZBGK5G3EFXWT61DR2NHMDD).
pub async fn run_with(
    runtime: &impl Runtime,
    api: Api,
    here: &Place,
    server: &str,
    item: &str,
    image: Option<&str>,
) -> Result<i32> {
    let issue = issue_of(item)?;
    let dir = identity::working_dir()?;
    let (main, common) = main_clone(&dir)?;
    let image = match image {
        Some(image) => image.to_owned(),
        None => Image::of_repository(&main)?.to_string(),
    };
    let image = image.as_str();
    let checked = check(runtime, &main, issue, image).await?;
    let Some(thread) = here.default_thread() else {
        bail!("{}", text::RUN_NO_REPO);
    };

    let session = terminal::new_session_id();
    let place = Place::new(here.host(), here.repo().clone(), Some(item))?;
    let me: SessionUri = identity::agent(&place, &session, server)?;
    let api = api.signed_in(Some(&session))?;
    api.start(&me, StartReason::Process, true).await?;
    let claimed = api.claim(&me, &thread, item).await?;
    println!("{}", text::claimed(&claimed, &thread, item));
    if !claimed.granted {
        let _ = api.end(&me).await;
        return Ok(1);
    }
    let alive = {
        let (api, me) = (api.clone(), me.clone());
        tokio::spawn(async move { api::keep_alive(&api, &me, WORKER_ALIVE_EVERY).await })
    };
    if let Some(line) = dropped::at_claim(&main, item).await {
        println!("{line}");
    }
    if let Some(line) = rollout::at_claim(&main, &here.repo_text(), item).await {
        println!("{line}");
    }

    let worked = work(
        runtime, &api, &me, &thread, &main, &common, &checked, &session, item, issue, image,
    )
    .await;
    if let Err(e) = api.release(&me, &thread, item).await {
        eprintln!("riff: {e:#}");
    }
    alive.abort();
    let _ = api.end(&me).await;
    worked
}

/// The part of [`run`] between the claim and the release.
#[allow(clippy::too_many_arguments)]
async fn work(
    runtime: &impl Runtime,
    api: &Api,
    me: &SessionUri,
    thread: &riff_core::name::ThreadName,
    main: &Path,
    common: &Path,
    checked: &Checked,
    session: &str,
    item: &str,
    issue: u64,
    image: &str,
) -> Result<i32> {
    let folder = sessions_dir()?.join(format!("{item}-{}", short(session)));
    let clone = folder.join("clone");
    let claude = folder.join("claude");
    std::fs::create_dir_all(&claude)
        .with_context(|| format!("cannot make {}", claude.display()))?;
    let branch = branch(item);
    let earlier = make_clone(main, &clone, &branch)?;
    std::fs::write(
        claude.join(".claude.json"),
        serde_json::to_string_pretty(&trust(&clone))?,
    )?;
    let home = &checked.home;
    let existing = |p: PathBuf| p.exists().then_some(p);
    let facts = Facts {
        session: session.to_owned(),
        item: item.to_owned(),
        image: image.to_owned(),
        home: home.clone(),
        user: checked.user.clone(),
        clone: clone.clone(),
        objects: common.join("objects"),
        claude,
        credentials: checked.credentials.clone(),
        gitconfig: existing(home.join(".gitconfig")),
        gh_config: existing(home.join(".config/gh")),
    };
    let spec = spec(&facts);
    let repo = me.place().repo_text();
    println!(
        "{}",
        text::run_started(item, session, &clone, image, &spec.name)
    );
    let frame = pr::body(hygiene::Link::Closes(issue), "SUMMARY", &checked.milestone)?;
    let start = text::run_start(
        &repo,
        issue,
        &branch,
        session,
        &frame,
        &checked.milestone,
        earlier,
    );
    let scrub = Scrub::new(
        std::iter::once(checked.gh_token.clone()).chain(checked.sign_in.secrets.iter().cloned()),
    );
    let events = folder.join("events.jsonl");
    let values = [("GH_TOKEN".to_owned(), checked.gh_token.clone())];
    let ended = run_session(runtime, &spec, &start, &events, &scrub, &values).await?;

    let pull = pr::Gh::default()
        .json::<Pull>(&[
            "pr",
            "view",
            &branch,
            "--json",
            "number,url,headRefOid,state",
        ])
        .ok()
        .filter(|p| p.state == "OPEN");
    match &pull {
        Some(pull) => {
            println!("{}", text::run_pull(pull.number, &pull.url));
            let body = text::run_verify_request(
                item,
                pull.number,
                &branch,
                &pull.head_ref_oid,
                short(session),
            );
            let to = [Selector::lead(me.who().user(), &repo)];
            if let Err(e) = api.post(me, Some(thread), &to, &body, Kind::Message).await {
                eprintln!("riff: {e:#}");
            }
        }
        None => println!("{}", text::run_no_pull(&branch)),
    }
    println!("{}", text::run_kept(&clone));
    Ok(i32::from(
        !(ended.exit.success() && !ended.failed && pull.is_some()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Fake;

    fn facts() -> Facts {
        Facts {
            session: "1a2b3c4d-0000-4000-8000-000000000000".into(),
            item: "issue-12".into(),
            image: "localhost/riff-session:latest".into(),
            home: "/home/mike".into(),
            user: "mike".into(),
            clone: "/r/sessions/issue-12-1a2b3c4d/clone".into(),
            objects: "/src/riff/.git/objects".into(),
            claude: "/r/sessions/issue-12-1a2b3c4d/claude".into(),
            credentials: "/home/mike/.claude/.credentials.json".into(),
            gitconfig: Some("/home/mike/.gitconfig".into()),
            gh_config: Some("/home/mike/.config/gh".into()),
        }
    }

    /// A bind as `SOURCE -> TARGET`, with ` (no write)` when the session
    /// cannot write it.
    fn shown(bind: &Bind) -> String {
        let source = match &bind.source {
            Source::Path(path) => path.display().to_string(),
            Source::Volume(name) => format!("volume {name}"),
        };
        let mode = if bind.write { "" } else { " (no write)" };
        format!("{source} -> {}{mode}", bind.target.display())
    }

    /// The variables of a spec as `NAME=VALUE`, or `NAME` for a value of
    /// the host.
    fn variables(spec: &SessionSpec) -> Vec<String> {
        spec.env
            .iter()
            .map(|env| match env {
                Env::Value(name, value) => format!("{name}={value}"),
                Env::Host(name) => name.clone(),
            })
            .collect()
    }

    #[test]
    fn the_spec_has_each_bind_of_a_session_and_writes_only_its_own_folders() {
        let spec = spec(&facts());
        let binds: Vec<String> = spec.binds.iter().map(shown).collect();
        assert_eq!(
            binds,
            [
                "/r/sessions/issue-12-1a2b3c4d/clone -> /r/sessions/issue-12-1a2b3c4d/clone",
                "/src/riff/.git/objects -> /src/riff/.git/objects (no write)",
                "/home/mike/.gitconfig -> /home/mike/.gitconfig (no write)",
                "/home/mike/.config/gh -> /home/mike/.config/gh (no write)",
                "/r/sessions/issue-12-1a2b3c4d/claude -> /home/mike/.claude",
                "/home/mike/.claude/.credentials.json -> /home/mike/.claude/.credentials.json \
                 (no write)",
                "volume riff-cargo -> /home/mike/.cargo",
                "volume riff-sccache -> /home/mike/.cache/sccache",
            ]
        );
        // Each path of the person on the host has no write.
        for bind in &spec.binds {
            if let Source::Path(path) = &bind.source
                && path.starts_with("/home/mike")
            {
                assert!(!bind.write, "{bind:?}");
            }
        }
    }

    #[test]
    fn the_spec_has_the_home_the_limits_and_the_values_of_a_session() {
        let spec = spec(&facts());
        assert_eq!(spec.name, "riff-issue-12-1a2b3c4d");
        assert_eq!(spec.image, "localhost/riff-session:latest");
        assert_eq!(spec.home, Path::new("/home/mike"));
        assert_eq!(
            spec.workdir,
            Path::new("/r/sessions/issue-12-1a2b3c4d/clone")
        );
        assert_eq!((spec.memory.as_str(), spec.cpus), ("8g", 4));
        assert_eq!(
            variables(&spec),
            [
                "HOME=/home/mike",
                "USER=mike",
                "LOGNAME=mike",
                "GH_TOKEN",
                "GIT_CONFIG_COUNT=1",
                "GIT_CONFIG_KEY_0=commit.gpgsign",
                "GIT_CONFIG_VALUE_0=false",
                "CLAUDE_CONFIG_DIR=/home/mike/.claude",
                "CARGO_HOME=/home/mike/.cargo",
                "SCCACHE_DIR=/home/mike/.cache/sccache",
                "RUSTC_WRAPPER=sccache",
                "SCCACHE_IDLE_TIMEOUT=0",
                "CARGO_BUILD_JOBS=4",
            ]
        );
        assert_eq!(
            spec.command.join(" "),
            "claude -p --input-format stream-json --output-format stream-json --verbose \
             --session-id 1a2b3c4d-0000-4000-8000-000000000000 --permission-mode auto"
        );
        // The token of gh is the one value of the host.
        let token = [("GH_TOKEN".to_owned(), "gho_x".to_owned())];
        assert!(spec.check_values(&token).is_ok());
    }

    #[test]
    fn the_spec_binds_no_git_or_gh_config_that_the_host_does_not_have() {
        let spec = spec(&Facts {
            gitconfig: None,
            gh_config: None,
            ..facts()
        });
        let all: Vec<String> = spec.binds.iter().map(shown).collect();
        let all = all.join("\n");
        assert!(!all.contains(".gitconfig"), "{all}");
        assert!(!all.contains(".config/gh"), "{all}");
    }

    /// The events of one turn of a fake `claude`, with a token in a
    /// tool result.
    const TURN: [&str; 3] = [
        r#"{"type":"system","subtype":"init","model":"m","permissionMode":"auto"}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":"gho_secret1"}]}}"#,
        r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1}"#,
    ];

    /// Drives the session of `facts()` in `fake`, with `stop`. Gives how
    /// it ended and its log of events.
    async fn driven(fake: &Fake, stop: impl Future<Output = ()>) -> (Result<Ended>, String) {
        let folder = tempfile::tempdir().unwrap();
        let events = folder.path().join("events.jsonl");
        let scrub = Scrub::new(["gho_secret1".to_owned()]);
        let values = [("GH_TOKEN".to_owned(), "gho_secret1".to_owned())];
        let ended = drive(
            fake,
            &spec(&facts()),
            "Do it.",
            &events,
            &scrub,
            &values,
            stop,
        )
        .await;
        let log = std::fs::read_to_string(&events).unwrap_or_default();
        (ended, log)
    }

    #[tokio::test]
    async fn a_session_gets_the_start_text_and_ends_after_the_result_of_its_turn() {
        let fake = Fake::default()
            .with_image("localhost/riff-session:latest")
            .with_events(TURN);
        let (ended, log) = driven(&fake, std::future::pending()).await;
        assert_eq!(
            ended.unwrap(),
            Ended {
                exit: Exit::SUCCESS,
                failed: false
            }
        );
        let name = "riff-issue-12-1a2b3c4d";
        assert_eq!(fake.calls(), [format!("start {name}")]);
        // One stream-json line, then the input closed after the result.
        assert_eq!(fake.input(name), format!("{}\n", user_line("Do it.")));
        assert_eq!(fake.exit(name), Some(Exit::SUCCESS));
        // The token goes to the runtime beside the spec, and into no log.
        assert_eq!(
            fake.values(name),
            [("GH_TOKEN".to_owned(), "gho_secret1".to_owned())]
        );
        assert_eq!(log.lines().count(), 3, "{log}");
        assert!(!log.contains("gho_secret1"), "{log}");
        assert!(log.contains(HIDDEN), "{log}");
    }

    #[tokio::test]
    async fn a_turn_that_ends_with_an_error_fails() {
        let error = r#"{"type":"result","subtype":"error_during_execution","is_error":true}"#;
        let fake = Fake::default()
            .with_image("localhost/riff-session:latest")
            .with_events([error]);
        let (ended, _) = driven(&fake, std::future::pending()).await;
        assert_eq!(
            ended.unwrap(),
            Ended {
                exit: Exit::SUCCESS,
                failed: true
            }
        );
    }

    #[tokio::test]
    async fn a_stop_stops_the_container_of_a_turn_that_never_ends() {
        // claude starts, and never ends its turn.
        let fake = Fake::default()
            .with_image("localhost/riff-session:latest")
            .with_events([TURN[0]]);
        let (ended, log) = driven(&fake, async {}).await;
        assert_eq!(
            ended.unwrap(),
            Ended {
                exit: Exit { code: None },
                failed: true
            }
        );
        let name = "riff-issue-12-1a2b3c4d";
        assert_eq!(
            fake.calls(),
            [format!("start {name}"), format!("stop {name}")]
        );
        assert!(log.lines().count() <= 1, "{log}");
    }

    #[tokio::test]
    async fn a_session_with_no_image_on_the_machine_does_not_start() {
        let fake = Fake::default().with_events(TURN);
        let (ended, _) = driven(&fake, std::future::pending()).await;
        let error = ended.unwrap_err().to_string();
        assert!(error.contains("no image"), "{error}");
    }

    #[tokio::test]
    async fn riff_run_pulls_only_an_image_that_the_machine_does_not_have() {
        let (local, remote, missing) = ("img-local", "img-remote", "img-missing");
        let fake = Fake::default().with_image(local).with_registry(remote);
        have_image(&fake, local).await.unwrap();
        assert!(fake.calls().is_empty());
        have_image(&fake, remote).await.unwrap();
        assert!(fake.has_image(remote).await.unwrap());
        let error = format!("{:#}", have_image(&fake, missing).await.unwrap_err());
        assert!(error.contains(&text::run_no_image(missing)), "{error}");
        assert_eq!(
            fake.calls(),
            [format!("pull {remote}"), format!("pull {missing}")]
        );
    }

    #[test]
    fn a_denied_call_and_a_push_show_their_text_fields() {
        let denied = json!({"type":"system","subtype":"permission_denied","session_id":"s",
            "tool_name":"Bash","reason":"the rules refuse it","tool_use_id":"t1"});
        assert_eq!(
            event_lines(&denied),
            ["permission_denied: reason the rules refuse it, tool_name Bash, tool_use_id t1"]
        );
        let push = json!({"type":"system","subtype":"vcs_state_changed","uuid":"u",
            "action":"push","branch":"worktree-issue-12","count":1});
        assert_eq!(
            event_lines(&push),
            ["vcs_state_changed: action push, branch worktree-issue-12"]
        );
    }

    #[test]
    fn a_long_text_shows_its_first_line_cut_at_the_limit() {
        let long = "a".repeat(SHOWN + 5);
        let event = json!({"type":"assistant","message":{"content":[
            {"type":"text","text": format!("{long}\nsecond")}]}});
        assert_eq!(event_lines(&event), [format!("{}…", "a".repeat(SHOWN))]);
    }

    #[test]
    fn a_tool_result_with_text_parts_shows_them() {
        let event = json!({"type":"user","message":{"content":[
            {"type":"tool_result","is_error":true,
             "content":[{"type":"text","text":"no"},{"type":"text","text":"file"}]},
            {"type":"tool_result","is_error":false,"content":"fine"}]}});
        assert_eq!(event_lines(&event), ["! the tool failed: no file"]);
    }

    #[test]
    fn the_scrub_takes_out_each_secret_also_twice_in_one_line() {
        let scrub = Scrub::new(["gho_one".to_owned(), "sk-two".to_owned(), "ab".to_owned()]);
        assert_eq!(
            scrub.clean("gho_one sk-two gho_one ab"),
            "[secret] [secret] [secret] ab"
        );
    }

    #[test]
    fn a_sign_in_that_ended_has_minutes_below_zero() {
        let file = r#"{"claudeAiOauth":{"accessToken":"sk-a","expiresAt":0}}"#;
        let sign_in = sign_in(file, 120 * 60_000).unwrap();
        assert_eq!(sign_in.minutes, -120);
        assert_eq!(sign_in.secrets, ["sk-a"]);
    }
}
