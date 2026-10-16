//! `riff run ITEM`: one work item in one rootless podman container,
//! through the riff daemon (#712, #716, #717).
//!
//! # Design
//!
//! The host does each riff step with the riff of today: the checks, the
//! claim, the clone, the verify request and the release. The daemon
//! runs the container of the session ([`crate::daemon`]). In the
//! container, `riff relay` starts `claude` ([`crate::session_relay`]),
//! and `claude` does the work (01M4ZKX0NXN7E33VC3PSFGSPDT).
//!
//! `riff run` asks the daemon on its control socket: first for the
//! image, then for the session. It shows each event that the daemon
//! sends. With no daemon, `riff run` stops before the claim. On a host
//! that is not Linux, `riff run` starts nothing
//! (01M4ZBGK7SFEXE1AGH6Z0YQHJP).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant H as riff run (host)
//!     participant S as riff-server
//!     participant D as riff daemon
//!     participant C as riff relay and claude -p (container)
//!     participant G as GitHub
//!     H->>D: the image: pull it when it is not here
//!     H->>S: start a worker session, claim ITEM
//!     H->>H: git clone --reference MAIN, the branch worktree-ITEM
//!     H->>D: start SPEC, with the value of GH_TOKEN beside it
//!     D->>C: start riff relay, with the socket of the session
//!     D->>C: the start text, one stream-json line on the socket
//!     C-->>D: the events, one line each, with their numbers
//!     D-->>H: each event, with no secret
//!     C->>S: the riff tools, through the socket and the daemon
//!     C->>G: push the branch, open the pull request
//!     D->>C: close the input after the result: claude ends
//!     D-->>H: the end of the session
//!     H->>G: find the pull request of the branch
//!     H->>S: a verify request to the lead, release ITEM, end the session
//! ```
//!
//! Before the claim, riff checks each fact that it needs: the image of
//! the session (the daemon pulls it when it must), the sign-in of `gh`,
//! the sign-in of `claude`, `HOME` and `USER`, and the milestone of the
//! issue. A failed check takes no claim (01M4Z81B3A324KKE8M8Z71TM4M).
//!
//! The image is the one that `riff.toml` of the main clone pins by its
//! digest ([`Image::of_repository`]), or the one of `--image`, for
//! example a local build. When podman does not have the image, the
//! daemon pulls it before the claim (01M4Z81B5A456HDKW7G0WFGCY6).
//!
//! When `claude` ends, riff finds the open pull request of the branch,
//! sends a verify request to the lead, releases the item and ends the
//! session. A SIGINT or a SIGTERM asks the daemon to stop the container
//! first (01M4Z81B1913NF6HBZ5G38B3Q9).
//!
//! riff sends no verify request when the issue has the label
//! [`rollout::NO_VERIFY`], or when a lead holds `verify-ITEM`. It shows
//! why ([`no_verify`], 01M5157V0P92EBHF4M7N9K0RGR,
//! 01M5157TYDEHZHDXSC1AHCRDRN). It reads the label and the holds after
//! the session: a hold that the lead makes while the session works
//! counts.
//!
//! ```mermaid
//! flowchart LR
//!     P[an open pull request] --> L{the label no-verify?}
//!     L -->|yes| N[no verify request: riff shows why]
//!     L -->|no| H{a hold of verify-ITEM?}
//!     H -->|yes| N
//!     H -->|no| V[the verify request to the lead]
//! ```
//!
//! `claude` does not end while one of its background tasks runs. After
//! a result, `riff run` shows each task that `claude` waits for
//! ([`View`], 01M4ZSZX9H59T04QSJ01V4F9TE). The start text tells the
//! session to leave no background task (01M4ZSZXBXM6T6MGQ00ACQA66X).
//! When no event comes for the quiet limit after the result, the daemon
//! stops the container ([`daemon::QUIET_LIMIT`]). `riff run` shows why,
//! then does the steps of the end.
//!
//! When the daemon stops before the end of the session, the session
//! runs on: its relay keeps the events. `riff run` waits up to
//! [`DAEMON_WAIT`] for a new daemon, and follows the session again after
//! the last event that it showed ([`Ask::Follow`]). When no daemon comes
//! back, `riff run` says so, and does these steps too
//! (01M4ZKX0F25S284R0N7KZVTHNA).
//!
//! # A restart of riff-server
//!
//! The verify request and the release have the budget [`END_BUDGET`],
//! not the short budget of a command (01M4ZQVVGQYXTAVRB69C7YPC3Y). A
//! call with no reply, or with a fault, tries again with the waits of
//! the link ([`crate::link`]) while the budget lasts. So a new instance
//! of riff-server at the end of a run drops neither. A new instance
//! starts, loads the state, and waits 15 s for the lease before it
//! serves; the old one replies 503 in that time. A cold start or a long
//! load can take this past the short budget. The keep-alive of the
//! session runs until the release, so the claim stays while the calls
//! wait. The steps before the work keep the short budget: when they
//! fail, `riff run` started nothing.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant H as riff run (host)
//!     participant S as riff-server
//!     H->>S: the verify request
//!     Note over S: a new instance starts: no reply
//!     H->>H: no reply in the limit of the try: a new connection, a wait
//!     H->>S: the verify request again, with the same call ID
//!     S-->>H: posted
//!     H->>S: release ITEM
//!     S-->>H: released
//! ```
//!
//! # A signal in the end steps
//!
//! A SIGINT or a SIGTERM stops the wait of the verify request, of the
//! release and of the end of the session (01M4ZYFJFTX9ZEX7QT4QH7E9T6).
//! Tokio keeps its handler of a signal to the end of the process, so a
//! signal after the session does not end riff by itself: riff runs each
//! end step against the signals. After a signal, riff sends no more
//! step, names each step that it did not send, and ends with the code 1.
//! riff-server frees the claim when the session stops, as for each other
//! session.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as the person
//!     participant H as riff run (host)
//!     participant S as riff-server
//!     H->>S: the verify request
//!     Note over S: no reply
//!     P->>H: Ctrl-C (SIGINT) or SIGTERM
//!     H->>H: stop the wait, send no more step
//!     H-->>P: the steps that riff did not send, the exit code 1
//!     Note over S: the session stops: the claim is free
//! ```
//!
//! # The folder of a session
//!
//! Each session has a folder of its own in [`sessions_dir`]: `ITEM-ID8`,
//! where `ID8` is the start of the session ID (01M4Z81ARSTAK8HR96S084SDFD).
//!
//! | Path | Holds |
//! |---|---|
//! | `clone/` | The clone of the session: `git clone --reference MAIN`. It stays until the merge. |
//! | `claude/` | The own Claude folder of the session ([`claude_folder`]): the trust of the clone ([`trust`]), and no attribution line of `claude` ([`settings`]). |
//! | `events.jsonl` | Each event of `claude`, with no secret ([`Scrub`]). The daemon writes it. |
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
//! | The init | `--init`: PID 1 reaps each orphan process | 01M51117K3WXK35CE67KPRFMA8 |
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
//! | The socket of the session | the daemon adds it: its folder at `/run/riff` with write, and `RIFF_SOCKET` | 01M4ZDTVSP0T1YENKAWGKA114D |

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{HoldInfo, Kind, StartReason, WORKER_ALIVE_EVERY};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::signal::unix::{SignalKind, signal};

use crate::api::{self, Api};
use crate::daemon::{self, Ask, Ended, Say};
use crate::runtime::{Bind, Env, SessionSpec, Source};
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

/// The budget of the verify request and of the release of `riff run`
/// (01M4ZQVVGQYXTAVRB69C7YPC3Y): longer than a restart of riff-server.
///
/// ```
/// use std::time::Duration;
/// use riff::link::SHORT_BUDGET;
///
/// assert_eq!(riff::run::END_BUDGET, Duration::from_secs(600));
/// assert!(riff::run::END_BUDGET > SHORT_BUDGET);
/// ```
pub const END_BUDGET: Duration = Duration::from_secs(600);

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

/// The data folder of riff 3.0: `RIFF_HOME`, else `XDG_DATA_HOME/riff`,
/// else `HOME/.local/share/riff`. It holds the session folders and the
/// folder of the daemon ([`crate::daemon::dir`]).
///
/// ```
/// use riff::run::data_dir_from;
/// use std::path::Path;
///
/// let some = |s: &str| Some(s.into());
/// assert_eq!(data_dir_from(some("/r"), some("/d"), some("/h"))?, Path::new("/r"));
/// assert_eq!(data_dir_from(None, some("/d"), some("/h"))?, Path::new("/d/riff"));
/// assert_eq!(
///     data_dir_from(None, some(""), some("/h"))?,
///     Path::new("/h/.local/share/riff")
/// );
/// assert!(data_dir_from(None, None, None).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn data_dir_from(
    riff_home: Option<OsString>,
    data_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf> {
    if let Some(dir) = home::dir_from(riff_home) {
        return Ok(dir);
    }
    let set = |v: Option<OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    match (set(data_home), set(home)) {
        (Some(data), _) => Ok(data.join("riff")),
        (None, Some(home)) => Ok(home.join(".local/share/riff")),
        (None, None) => bail!("set HOME or XDG_DATA_HOME"),
    }
}

/// [`data_dir_from`] with the values of this process.
pub fn data_dir() -> Result<PathBuf> {
    data_dir_from(
        std::env::var_os(home::VAR),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
    )
}

/// The folder of the session folders: `sessions` in [`data_dir`].
///
/// ```
/// use riff::run::sessions_dir_from;
/// use std::path::Path;
///
/// let some = |s: &str| Some(s.into());
/// assert_eq!(sessions_dir_from(some("/r"), some("/d"), some("/h"))?, Path::new("/r/sessions"));
/// assert_eq!(sessions_dir_from(None, some("/d"), some("/h"))?, Path::new("/d/riff/sessions"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn sessions_dir_from(
    riff_home: Option<OsString>,
    data_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf> {
    Ok(data_dir_from(riff_home, data_home, home)?.join("sessions"))
}

/// [`sessions_dir_from`] with the values of this process.
pub fn sessions_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("sessions"))
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

/// The `settings.json` of the own Claude folder: `claude` adds no
/// attribution line to a commit or a pull request, so the trailers end
/// each one (01M4ZD48DHAKJ1EHX6QHYAQ7ZM). It allows each rule of the
/// session place of the permission table, so the auto mode check does
/// not refuse a step of the start text
/// ([`crate::permissions::session_rules`], 01M5084K4YQCZF20XA0P2485SG).
/// It is a setting of riff for its sessions, not a copy of a setting of
/// the person.
///
/// ```
/// let settings = riff::run::settings();
/// assert_eq!(settings["includeCoAuthoredBy"], false);
/// let allow = settings["permissions"]["allow"].as_array().unwrap();
/// assert!(allow.iter().any(|r| r == "Bash(gh issue view *)"));
/// assert_eq!(allow.len(), riff::permissions::session_rules().allow.len());
/// ```
pub fn settings() -> Value {
    json!({
        "includeCoAuthoredBy": false,
        "permissions": { "allow": crate::permissions::session_rules().allow },
    })
}

/// Makes the own Claude folder `dir` of a session, with the
/// `.claude.json` of [`trust`] for `clone` and the `settings.json` of
/// [`settings`].
///
/// ```
/// let dir = std::env::temp_dir().join(format!("riff-doc-claude-{}", std::process::id()));
/// riff::run::claude_folder(&dir, "/data/clone".as_ref()).unwrap();
/// let read = |name| -> serde_json::Value {
///     serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap()
/// };
/// assert_eq!(read(".claude.json"), riff::run::trust("/data/clone".as_ref()));
/// assert_eq!(read("settings.json"), riff::run::settings());
/// std::fs::remove_dir_all(&dir).unwrap();
/// ```
pub fn claude_folder(dir: &Path, clone: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    for (name, value) in [
        (".claude.json", trust(clone)),
        ("settings.json", settings()),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, serde_json::to_string_pretty(&value)?)
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(())
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
pub(crate) fn output(program: &str, dir: &Path, args: &[&str]) -> Result<String> {
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

/// The facts of the host that each session needs: the sign-in of `gh`,
/// the home and the user of the person, and the sign-in of `claude` with
/// [`MIN_MINUTES`] or more left (01M4Z81B3A324KKE8M8Z71TM4M).
pub(crate) struct Host {
    pub(crate) gh_token: String,
    pub(crate) sign_in: SignIn,
    pub(crate) home: PathBuf,
    pub(crate) user: String,
    pub(crate) credentials: PathBuf,
}

/// Reads the [`Host`] facts, with `gh` in the folder `main`. A fact
/// that is missing is an error that says the fix.
pub(crate) fn host_facts(main: &Path) -> Result<Host> {
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
    Ok(Host {
        gh_token,
        sign_in,
        home,
        user,
        credentials,
    })
}

/// The milestone of the issue `issue`, with `gh`. An issue with no
/// milestone, or with one that `text::forge` changes, is refused: the
/// milestone goes into the start text and into a command of the
/// session, as in `riff pr open`.
pub(crate) fn milestone(gh: &pr::Gh, issue: u64) -> Result<String> {
    let found: hygiene::Issue = gh.json(&[
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
    if text::forge(&milestone) != milestone {
        bail!("{}", text::run_no_milestone(issue));
    }
    Ok(milestone)
}

/// True when the issue `issue` has the label [`rollout::NO_VERIFY`], with
/// `gh`.
fn labelled(gh: &pr::Gh, issue: u64) -> Result<bool> {
    let found: rollout::Issue =
        gh.json(&["issue", "view", &issue.to_string(), "--json", "number,labels"])?;
    Ok(found.no_verify())
}

/// Why `riff run` sends no verify request for `item`, the issue `issue`,
/// or `None` to send it: the issue has the label [`rollout::NO_VERIFY`]
/// (`labelled`, 01M5157V0P92EBHF4M7N9K0RGR), or a lead holds
/// `verify-ITEM` (01M5157TYDEHZHDXSC1AHCRDRN). `holds` are the holds of
/// the repository thread.
///
/// ```
/// use std::collections::BTreeMap;
/// use riff::run::no_verify;
/// use riff_core::wire::HoldInfo;
///
/// let hold = |reason: &str| HoldInfo { reason: reason.into(), by: None, at_ms: 1_790_000_000_000 };
/// let mut holds = BTreeMap::new();
/// assert_eq!(no_verify("issue-12", 12, false, &holds), None);
/// // A hold of the item itself does not stop the verify request.
/// holds.insert("issue-12".to_owned(), hold("later"));
/// assert_eq!(no_verify("issue-12", 12, false, &holds), None);
/// holds.insert("verify-issue-12".to_owned(), hold("a scratch item"));
/// assert_eq!(
///     no_verify("issue-12", 12, false, &holds).unwrap(),
///     "riff run sends no verify request for issue-12: verify-issue-12 is held by the lead since \
///      2026-09-21T14:13:20Z: a scratch item"
/// );
/// assert_eq!(
///     no_verify("issue-12", 12, true, &BTreeMap::new()).unwrap(),
///     "riff run sends no verify request for issue-12: issue #12 has the label no-verify."
/// );
/// ```
pub fn no_verify(
    item: &str,
    issue: u64,
    labelled: bool,
    holds: &BTreeMap<String, HoldInfo>,
) -> Option<String> {
    if labelled {
        return Some(text::run_no_verify_label(item, issue));
    }
    let verify = format!("verify-{item}");
    let hold = holds.get(&verify)?;
    let held = riff_state::state::plan::held_text(&verify, hold);
    Some(text::run_verify_held(item, &held))
}

/// [`no_verify`] now: the labels of the issue from `gh`, then the holds
/// of `thread` from riff-server, as `me`. riff asks riff-server only for
/// an issue with no label. A fault of `gh` shows on stderr, and riff
/// goes on to the holds.
async fn no_verify_now(
    api: &Api,
    me: &SessionUri,
    thread: &ThreadName,
    item: &str,
    issue: u64,
) -> Result<Option<String>> {
    let labelled = labelled(&pr::Gh::default(), issue).unwrap_or_else(|e| {
        eprintln!("riff: {e:#}");
        false
    });
    if labelled {
        return Ok(no_verify(item, issue, true, &BTreeMap::new()));
    }
    let holds = api.plan_show(me, thread).await?.holds;
    Ok(no_verify(item, issue, false, &holds))
}

/// Checks each fact that a session needs, before the claim. The daemon
/// in the folder `daemon` pulls the image when this machine does not
/// have it (01M4Z81B5A456HDKW7G0WFGCY6).
async fn check(daemon: &Path, main: &Path, issue: u64, image: &str) -> Result<Checked> {
    daemon::image(daemon, image, |line| println!("{line}")).await?;
    let Host {
        gh_token,
        sign_in,
        home,
        user,
        credentials,
    } = host_facts(main)?;
    Ok(Checked {
        milestone: milestone(&pr::Gh::default(), issue)?,
        gh_token,
        sign_in,
        home,
        user,
        credentials,
    })
}

/// The time now in milliseconds since the epoch.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The main clone of the working folder `dir`: its top, and its git dir.
pub(crate) fn main_clone(dir: &Path) -> Result<(PathBuf, PathBuf)> {
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
pub(crate) fn make_clone(main: &Path, clone: &Path, branch: &str) -> Result<bool> {
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

/// What the terminal shows of the events of one session: the lines of
/// each event ([`event_lines`]), and a line when `claude` waits for its
/// background tasks after the `result` of a turn
/// (01M4ZSZX9H59T04QSJ01V4F9TE).
///
/// ```
/// use riff::run::View;
/// use serde_json::json;
///
/// let mut view = View::default();
/// let tasks = json!({"type":"system","subtype":"background_tasks_changed",
///     "tasks":[{"task_id":"b1","task_type":"local_bash","description":"wait for just check"}]});
/// assert!(view.lines(&tasks).is_empty());
/// let result = json!({"type":"result","subtype":"success","num_turns":2,"duration_ms":1000});
/// assert_eq!(
///     view.lines(&result),
///     ["end: success, 2 turns, 1 s, 0.00 USD", "wait: claude waits for 1 background task: wait for just check"]
/// );
/// let none = json!({"type":"system","subtype":"background_tasks_changed","tasks":[]});
/// assert_eq!(view.lines(&none), ["wait: each background task ended"]);
/// ```
#[derive(Debug, Default)]
pub struct View {
    /// The description of each background task that runs.
    tasks: Vec<String>,
    /// True after a `result`, until the next turn starts.
    after_result: bool,
}

impl View {
    /// The lines that the terminal shows for `event`.
    pub fn lines(&mut self, event: &Value) -> Vec<String> {
        let mut lines = event_lines(event);
        match (event["type"].as_str(), event["subtype"].as_str()) {
            (Some("system"), Some("init")) => self.after_result = false,
            (Some("system"), Some("background_tasks_changed")) => {
                self.tasks = event["tasks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|task| first(task["description"].as_str().unwrap_or("?")))
                    .collect();
                if self.after_result {
                    lines.push(self.waits());
                }
            }
            (Some("result"), _) => {
                self.after_result = true;
                if !self.tasks.is_empty() {
                    lines.push(self.waits());
                }
            }
            _ => {}
        }
        lines
    }

    /// The line of the background tasks that `claude` waits for.
    fn waits(&self) -> String {
        match self.tasks.len() {
            0 => "wait: each background task ended".to_owned(),
            1 => format!(
                "wait: claude waits for 1 background task: {}",
                self.tasks[0]
            ),
            n => format!(
                "wait: claude waits for {n} background tasks: {}",
                self.tasks.join("; ")
            ),
        }
    }

    /// Shows one line of the output of the session: the lines of its
    /// event, or the line itself when it is not JSON.
    pub fn show(&mut self, line: &str) {
        match serde_json::from_str::<Value>(line) {
            Ok(event) => {
                for shown in self.lines(&event) {
                    println!("{shown}");
                }
            }
            Err(_) => println!("{line}"),
        }
    }
}

/// After the daemon went away, the longest wait of `riff run` for a new
/// daemon (01M4ZKX0F25S284R0N7KZVTHNA).
pub const DAEMON_WAIT: Duration = Duration::from_secs(600);

/// The time between two tries of `riff run` to reach a new daemon.
const DAEMON_RETRY: Duration = Duration::from_millis(250);

/// SIGINT and SIGTERM: the signals that stop `riff run`.
struct Signals {
    term: tokio::signal::unix::Signal,
    int: tokio::signal::unix::Signal,
}

impl Signals {
    /// Listens for the signals. From here on, a signal does not end
    /// riff: tokio keeps its handler to the end of the process, so riff
    /// must act on each signal itself.
    fn new() -> std::io::Result<Signals> {
        Ok(Signals {
            term: signal(SignalKind::terminate())?,
            int: signal(SignalKind::interrupt())?,
        })
    }

    /// Waits for the next SIGINT or SIGTERM.
    async fn recv(&mut self) {
        tokio::select! {
            _ = self.term.recv() => {}
            _ = self.int.recv() => {}
        }
    }
}

/// The end steps of `riff run`: the verify request, the release and the
/// end of the session (01M4ZYFJFTX9ZEX7QT4QH7E9T6). A SIGINT or a SIGTERM
/// stops the wait of the step that runs, and riff sends no step after
/// it. riff listens for the signals from the first step on; before it,
/// [`follow`] acts on them.
#[derive(Default)]
struct EndSteps {
    signals: Option<Signals>,
    /// The steps that riff did not send, after a signal.
    not_sent: Vec<&'static str>,
}

impl EndSteps {
    /// Runs `call`, the end step `step`, until it ends or a signal comes.
    /// It gives the result of the call, or `None` when a signal stopped
    /// it or an earlier step.
    async fn send<T>(
        &mut self,
        step: &'static str,
        call: impl Future<Output = Result<T>>,
    ) -> Option<Result<T>> {
        if self.not_sent.is_empty() {
            if self.signals.is_none() {
                self.signals = Signals::new().ok();
            }
            let stop = async {
                match &mut self.signals {
                    Some(signals) => signals.recv().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                done = call => return Some(done),
                () = stop => {}
            }
        }
        self.not_sent.push(step);
        None
    }
}

/// Asks the daemon in the folder `daemon` to run the session of
/// `start`, shows its events, and waits for its end ([`follow`]).
async fn run_session(daemon: &Path, start: daemon::Start) -> Result<Ended> {
    let name = start.spec.name.clone();
    let mut view = View::default();
    follow(
        daemon,
        Ask::Start(Box::new(start)),
        &name,
        DAEMON_WAIT,
        |line| view.show(line),
    )
    .await
}

/// Sends `ask` to the daemon in the folder `daemon`, gives `show` each
/// event of the session `name`, and waits for its end. A SIGINT or a
/// SIGTERM asks the daemon to stop the container. When the daemon goes
/// away, the session runs on ([`crate::session_relay`]): riff tries the
/// daemon again for up to `wait`, and follows the session again after
/// the last event that it showed. So it shows each event one time, and
/// loses none (01M4ZKX0F25S284R0N7KZVTHNA). When no daemon comes back,
/// the session failed for `riff run`.
async fn follow(
    daemon: &Path,
    ask: Ask,
    name: &str,
    wait: Duration,
    mut show: impl FnMut(&str),
) -> Result<Ended> {
    // The handlers come first: a signal before them ends riff with no
    // release.
    let mut signals = Signals::new()?;
    let mut name = name.to_owned();
    let mut client = daemon::Client::connect(daemon).await?;
    client.ask(ask).await?;
    let mut stopped = false;
    let mut seen = 0;
    loop {
        let say = tokio::select! {
            say = client.next() => say,
            () = signals.recv(), if !stopped => {
                stopped = true;
                println!("{}", text::run_stopping(&name));
                client.stop().await?;
                continue;
            }
        };
        match say {
            Ok(Some(Say::Started { name: started, .. })) => name = started,
            Ok(Some(Say::Event { seq, line })) if seq > seen => {
                seen = seq;
                show(&line);
            }
            Ok(Some(Say::Event { .. })) => {}
            Ok(Some(Say::Stderr { line })) => eprintln!("{line}"),
            Ok(Some(Say::Line { text })) => println!("{text}"),
            Ok(Some(Say::Ended(ended))) => return Ok(ended),
            Ok(Some(Say::Failed { error })) => bail!("{error}"),
            Ok(Some(_)) => bail!("{}", text::DAEMON_GONE),
            Ok(None) | Err(_) => {
                println!("{}", text::run_daemon_away(&name));
                let Some(again) = connect_again(daemon, wait, &mut signals).await else {
                    println!("{}", text::run_daemon_gone(&name));
                    return Ok(Ended {
                        code: None,
                        failed: true,
                    });
                };
                client = again;
                client
                    .ask(Ask::Follow {
                        name: name.clone(),
                        after: seen,
                    })
                    .await?;
                println!("{}", text::run_daemon_back(&name, seen));
                if stopped {
                    client.stop().await?;
                }
            }
        }
    }
}

/// Tries the daemon in the folder `daemon` again each
/// [`DAEMON_RETRY`], for up to `wait`. A SIGINT or a SIGTERM ends the
/// wait. It gives `None` when no daemon answered.
async fn connect_again(
    daemon: &Path,
    wait: Duration,
    signals: &mut Signals,
) -> Option<daemon::Client> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        if let Ok(client) = daemon::Client::connect(daemon).await {
            return Some(client);
        }
        tokio::select! {
            () = tokio::time::sleep(DAEMON_RETRY) => {}
            () = tokio::time::sleep_until(deadline) => return None,
            () = signals.recv() => return None,
        }
    }
}

/// A pull request as `gh pr view --json number,url,headRefOid,state`
/// gives it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Pull {
    pub(crate) number: u64,
    pub(crate) url: String,
    pub(crate) head_ref_oid: String,
    pub(crate) state: String,
}

/// `riff run ITEM`: one work item in one container, from the claim to
/// the pull request. With no `image`, it takes the image that the main
/// clone pins. It gives the exit code of the command. On Linux, it runs
/// [`run_with`] with the daemon of this machine ([`daemon::dir`]). On a
/// host that is not Linux, it starts nothing (01M4ZBGK7SFEXE1AGH6Z0YQHJP).
pub async fn run(
    api: Api,
    here: &Place,
    server: &str,
    item: &str,
    image: Option<&str>,
) -> Result<i32> {
    #[cfg(target_os = "linux")]
    {
        run_with(&daemon::dir()?, api, here, server, item, image).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (api, here, server, item, image);
        bail!("{}", text::RUN_LINUX_ONLY)
    }
}

/// [`run`] with the daemon in the folder `daemon`.
pub async fn run_with(
    daemon: &Path,
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
    let checked = check(daemon, &main, issue, image).await?;
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
        tokio::spawn(
            async move { api::keep_alive_in_container(&api, &me, WORKER_ALIVE_EVERY).await },
        )
    };
    if let Some(line) = dropped::at_claim(&main, item).await {
        println!("{line}");
    }
    if let Some(line) = rollout::at_claim(&main, &here.repo_text(), item).await {
        println!("{line}");
    }

    let mut steps = EndSteps::default();
    let worked = work(
        daemon, server, &api, &me, &thread, &main, &common, &checked, &session, item, issue, image,
        &mut steps,
    )
    .await;
    let ending = api.with_budget(END_BUDGET);
    let release = ending.release(&me, &thread, item);
    if let Some(Err(e)) = steps.send(text::RUN_RELEASE, release).await {
        eprintln!("riff: {e:#}");
    }
    alive.abort();
    let _ = steps.send(text::RUN_END, api.end(&me)).await;
    if steps.not_sent.is_empty() {
        return worked;
    }
    println!("{}", text::run_not_sent(item, &steps.not_sent));
    worked.map(|_| 1)
}

/// The part of [`run`] between the claim and the release.
#[allow(clippy::too_many_arguments)]
async fn work(
    daemon: &Path,
    server: &str,
    api: &Api,
    me: &SessionUri,
    thread: &ThreadName,
    main: &Path,
    common: &Path,
    checked: &Checked,
    session: &str,
    item: &str,
    issue: u64,
    image: &str,
    steps: &mut EndSteps,
) -> Result<i32> {
    let folder = sessions_dir()?.join(format!("{item}-{}", short(session)));
    let clone = folder.join("clone");
    let claude = folder.join("claude");
    claude_folder(&claude, &clone)?;
    let branch = branch(item);
    let earlier = make_clone(main, &clone, &branch)?;
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
    let secrets = std::iter::once(checked.gh_token.clone())
        .chain(checked.sign_in.secrets.iter().cloned())
        .collect();
    let values = vec![("GH_TOKEN".to_owned(), checked.gh_token.clone())];
    let ended = run_session(
        daemon,
        daemon::Start {
            session: session.to_owned(),
            item: item.to_owned(),
            folder,
            spec,
            start,
            values,
            secrets,
            server: server.to_owned(),
            me: me.clone(),
            terminal: false,
        },
    )
    .await?;

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
            let ending = api.with_budget(END_BUDGET);
            let ask = async {
                if let Some(why) = no_verify_now(&ending, me, thread, item, issue).await? {
                    println!("{why}");
                    return Ok(());
                }
                ending
                    .post(me, Some(thread), &to, &body, Kind::Message)
                    .await
                    .map(drop)
            };
            if let Some(Err(e)) = steps.send(text::RUN_VERIFY_REQUEST, ask).await {
                eprintln!("riff: {e:#}");
            }
        }
        None => println!("{}", text::run_no_pull(&branch)),
    }
    println!("{}", text::run_kept(&clone));
    Ok(i32::from(
        !(ended.code == Some(0) && !ended.failed && pull.is_some()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// After a signal, riff runs no more end step, and names each step
    /// that it did not send (01M4ZYFJFTX9ZEX7QT4QH7E9T6).
    #[tokio::test]
    async fn after_a_signal_no_end_step_runs() {
        let mut steps = EndSteps {
            signals: None,
            not_sent: vec![text::RUN_VERIFY_REQUEST],
        };
        let release = async { Err::<(), _>(anyhow!("the release ran")) };
        assert!(steps.send(text::RUN_RELEASE, release).await.is_none());
        let end = async { Err::<(), _>(anyhow!("the end ran")) };
        assert!(steps.send(text::RUN_END, end).await.is_none());
        assert!(steps.signals.is_none(), "riff listens after the stop");
        assert_eq!(
            steps.not_sent,
            [text::RUN_VERIFY_REQUEST, text::RUN_RELEASE, text::RUN_END]
        );
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
    fn the_claude_folder_of_the_spec_turns_off_the_attribution_line() {
        let root = tempfile::tempdir().unwrap();
        let facts = Facts {
            claude: root.path().join("claude"),
            clone: root.path().join("clone"),
            ..facts()
        };
        claude_folder(&facts.claude, &facts.clone).unwrap();

        // claude in the container reads this folder as its config folder.
        let spec = spec(&facts);
        let bind = spec
            .binds
            .iter()
            .find(|b| b.source == Source::Path(facts.claude.clone()))
            .unwrap();
        assert_eq!(bind.target, Path::new("/home/mike/.claude"));
        assert!(variables(&spec).contains(&"CLAUDE_CONFIG_DIR=/home/mike/.claude".to_owned()));

        let read = |name: &str| -> Value {
            serde_json::from_str(&std::fs::read_to_string(facts.claude.join(name)).unwrap())
                .unwrap()
        };
        assert_eq!(read("settings.json"), settings());
        assert_eq!(read("settings.json")["includeCoAuthoredBy"], false);
        let clone = facts.clone.display().to_string();
        assert_eq!(
            read(".claude.json")["projects"][&clone]["hasTrustDialogAccepted"],
            true
        );
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
