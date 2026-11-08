//! The GitHub Apps of the riff: the server side of `riff github create`,
//! `install` and `check` (01M6WG52J7WTP079W1AYEQ4W08).
//!
//! # Design
//!
//! A riff has two Apps on one account: the session App `riff-OWNER` for
//! each worker and each verifier, and the lead App `riff-OWNER-lead`
//! ([`GitHubApp`]). Each token of an App acts as the bot of the App, so
//! GitHub tells the lead from a worker by the App, and a worker from a
//! verifier by the rights of the token ([`rights`]).
//!
//! `riff github create` makes the Apps with the manifest flow of GitHub
//! (01M6WG52T5PAD9NGQ597CP56K7):
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff github create
//!     participant B as browser
//!     participant S as riff-server
//!     participant G as GitHub
//!     R->>S: POST /v1/github/create (the owner or an admin)
//!     S-->>R: the start page, with a new state (10 minutes, one use)
//!     R->>B: open the start page
//!     B->>G: POST the manifest of riff-OWNER
//!     G-->>B: go to /github/created with a code and the state
//!     B->>S: the code and the state
//!     S->>G: POST /app-manifests/CODE/conversions
//!     G-->>S: the App ID, the slug, the private key, the webhook secret
//!     S->>G: GET /users/SLUG[bot]: the user ID of the bot
//!     S-->>B: the page with the manifest of riff-OWNER-lead, and a new state
//!     Note over B,S: the same swap for the lead App
//!     S-->>B: the install page of each App on OWNER
//!     R->>S: POST /v1/github/apps until both are installed
//!     R->>S: POST /v1/github/check
//! ```
//!
//! - [`manifest`] asks for the highest right of the roles of the App and
//!   no more ([`app_rights`]). The manifest of the session App also sets
//!   the webhook [`HOOK_PATH`] with the [`EVENTS`]
//!   (01M6WG52QHC8E24RJ33ZPVHZSJ). riff-server takes each delivery with
//!   status 202, and drops it.
//! - A state is good for [`STATE_LIFE`] and for one use ([`States`]).
//!   The start page reads its state, and the swap uses it. Each state
//!   names one App, so the state of the session App never makes the
//!   lead App.
//! - The swap keeps the keys in [`crate::keys`]. The key and the
//!   webhook secret are in no reply and in no log line
//!   (01M6WG52WSWM8A9CN6X448AGAJ).
//! - `riff github check` asks for a token of each role for one
//!   repository (01M6WG534PPE2JW8JQFRJDEK86). riff-server
//!   signs a JWT of the App with its key, finds the installation of the
//!   repository, asks for a token with the rights of the role, compares
//!   the rights that GitHub gave, and revokes the token at once. The
//!   reply holds no token.
//!
//! The pages of the browser and the webhook need no riff token: the
//! state, and the signature of GitHub, are their proof. The three calls
//! of `riff` need a token, as the calls of the members do.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Query, State as AxumState};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::{Extension, Json};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use riff_core::name::{Repo, SessionUri, Who};
use riff_core::wire::{
    GitHubApp, GitHubAppFacts, GitHubApps, GitHubAppsReply, GitHubCheck, GitHubChecked,
    GitHubCreate, GitHubDaemonToken, GitHubRole, GitHubStarted, GitHubToken, Rights, RoleChecked,
    SessionToken,
};
use riff_state::state::{Code, Refused, Role};
use serde::{Deserialize, Serialize};

use crate::auth::SignedIn;
use crate::engine::Failed;
use crate::keys::{AppKeys, Apps, At, Keys};

/// The API of GitHub.
pub const API: &str = "https://api.github.com";

/// The site of GitHub: the pages of the Apps.
pub const WEB: &str = "https://github.com";

/// The path of the webhook of the session App.
pub const HOOK_PATH: &str = "/hooks/github";

/// The path of the start page of `riff github create`.
pub const START_PATH: &str = "/github/start";

/// The path that GitHub sends the browser to after it made an App.
pub const CREATED_PATH: &str = "/github/created";

/// How long a state of `riff github create` is good.
pub const STATE_LIFE: Duration = Duration::from_secs(10 * 60);

/// The events of the webhook of the session App.
pub const EVENTS: [&str; 7] = [
    "issues",
    "issue_comment",
    "pull_request",
    "check_run",
    "status",
    "workflow_run",
    "merge_group",
];

/// The reads that the [`EVENTS`] need beyond the rights of the roles:
/// `merge_group` needs `merge_queues`.
const EVENT_READS: [&str; 1] = ["merge_queues"];

/// A JWT of an App is good for this long. GitHub takes at most 10
/// minutes.
const JWT_LIFE: Duration = Duration::from_secs(9 * 60);

/// The version of the REST API of GitHub.
const API_VERSION: &str = "2022-11-28";

/// The GitHub settings of a riff-server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The API of GitHub: [`API`] but in a test.
    pub api: String,
    /// The site of GitHub: [`WEB`] but in a test.
    pub web: String,
    /// Where riff-server keeps the keys of the Apps.
    pub keys: At,
    /// How long a state is good: [`STATE_LIFE`] but in a test.
    pub state_life: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            api: API.to_owned(),
            web: WEB.to_owned(),
            keys: At::Memory,
            state_life: STATE_LIFE,
        }
    }
}

/// The rights of the token of `role` (01M6WG52MWZB1SVQMMH6BKJSJ5).
///
/// ```
/// use riff_core::wire::GitHubRole;
/// use riff_server::github::rights;
///
/// let worker = rights(GitHubRole::Worker);
/// assert_eq!(worker["contents"], "write");
/// assert_eq!(worker["statuses"], "read");
/// assert_eq!(worker["workflows"], "write");
/// let verifier = rights(GitHubRole::Verifier);
/// assert!(verifier.values().all(|level| level == "read"));
/// assert!(!verifier.contains_key("workflows"));
/// let lead = rights(GitHubRole::Lead);
/// assert_eq!(lead["statuses"], "write");
/// assert!(!lead.contains_key("workflows"));
/// ```
pub fn rights(role: GitHubRole) -> Rights {
    let (read, write): (&[&str], &[&str]) = match role {
        GitHubRole::Worker => (
            &["metadata", "statuses", "checks"],
            &[
                "contents",
                "pull_requests",
                "issues",
                "actions",
                "workflows",
            ],
        ),
        GitHubRole::Verifier => (
            &[
                "metadata",
                "contents",
                "pull_requests",
                "issues",
                "statuses",
                "checks",
                "actions",
            ],
            &[],
        ),
        GitHubRole::Lead => (
            &["metadata", "checks"],
            &["contents", "pull_requests", "issues", "statuses", "actions"],
        ),
    };
    let level = |names: &[&str], level: &str| {
        names
            .iter()
            .map(|name| ((*name).to_owned(), level.to_owned()))
            .collect::<Vec<_>>()
    };
    level(read, "read")
        .into_iter()
        .chain(level(write, "write"))
        .collect()
}

/// The rights that the App `app` asks for: the highest level of each
/// right of its roles, and for the session App the reads of its
/// [`EVENTS`] (01M6WG52QHC8E24RJ33ZPVHZSJ).
///
/// ```
/// use riff_core::wire::GitHubApp;
/// use riff_server::github::app_rights;
///
/// let session = app_rights(GitHubApp::Session);
/// assert_eq!(session["contents"], "write");
/// assert_eq!(session["merge_queues"], "read");
/// assert!(!app_rights(GitHubApp::Lead).contains_key("merge_queues"));
/// ```
pub fn app_rights(app: GitHubApp) -> Rights {
    let mut highest = Rights::new();
    let roles = GitHubRole::ALL.into_iter().filter(|role| role.app() == app);
    for (name, level) in roles.flat_map(rights) {
        let kept = highest.entry(name).or_insert_with(|| level.clone());
        if level == "write" {
            *kept = level;
        }
    }
    if app == GitHubApp::Session {
        for name in EVENT_READS {
            highest
                .entry(name.to_owned())
                .or_insert_with(|| "read".to_owned());
        }
    }
    highest
}

/// The manifest of the App `app` on the account `owner`, for the riff at
/// `public_url` (01M6WG52QHC8E24RJ33ZPVHZSJ).
///
/// ```
/// use riff_core::wire::GitHubApp;
/// use riff_server::github::manifest;
///
/// let session = manifest(GitHubApp::Session, "acme", "https://riff.example.com");
/// assert_eq!(session["name"], "riff-acme");
/// assert_eq!(session["public"], true);
/// assert_eq!(session["redirect_url"], "https://riff.example.com/github/created");
/// assert_eq!(session["hook_attributes"]["url"], "https://riff.example.com/hooks/github");
/// assert_eq!(session["default_events"][0], "issues");
/// let lead = manifest(GitHubApp::Lead, "acme", "https://riff.example.com");
/// assert_eq!(lead["name"], "riff-acme-lead");
/// assert!(lead.get("hook_attributes").is_none());
/// assert!(lead.get("default_events").is_none());
/// ```
pub fn manifest(app: GitHubApp, owner: &str, public_url: &str) -> serde_json::Value {
    let mut manifest = serde_json::json!({
        "name": app.name(owner),
        "url": public_url,
        "description": match app {
            GitHubApp::Session => "The workers and the verifiers of the riff.",
            GitHubApp::Lead => "The lead of the riff.",
        },
        "redirect_url": format!("{public_url}{CREATED_PATH}"),
        "public": true,
        "default_permissions": app_rights(app),
    });
    if app == GitHubApp::Session {
        manifest["hook_attributes"] = serde_json::json!({
            "url": format!("{public_url}{HOOK_PATH}"),
            "active": true,
        });
        manifest["default_events"] = serde_json::json!(EVENTS);
    }
    manifest
}

/// The page of GitHub that takes a manifest: of an organization, or of
/// a personal account.
///
/// ```
/// use riff_server::github::{WEB, new_app_url};
///
/// assert_eq!(
///     new_app_url(WEB, "acme", true, "S"),
///     "https://github.com/organizations/acme/settings/apps/new?state=S"
/// );
/// assert_eq!(new_app_url(WEB, "ada", false, "S"), "https://github.com/settings/apps/new?state=S");
/// ```
pub fn new_app_url(web: &str, owner: &str, organization: bool, state: &str) -> String {
    if organization {
        format!("{web}/organizations/{owner}/settings/apps/new?state={state}")
    } else {
        format!("{web}/settings/apps/new?state={state}")
    }
}

/// The install page of the App `slug`, with the account `owner_id` as
/// the target when riff knows it.
///
/// ```
/// use riff_server::github::{WEB, install_url};
///
/// assert_eq!(
///     install_url(WEB, "riff-acme", Some(7)),
///     "https://github.com/apps/riff-acme/installations/new/permissions?target_id=7"
/// );
/// assert_eq!(install_url(WEB, "riff-acme", None), "https://github.com/apps/riff-acme/installations/new");
/// ```
pub fn install_url(web: &str, slug: &str, owner_id: Option<u64>) -> String {
    match owner_id {
        Some(id) => format!("{web}/apps/{slug}/installations/new/permissions?target_id={id}"),
        None => format!("{web}/apps/{slug}/installations/new"),
    }
}

/// What differs between the rights that a role needs and the rights
/// that GitHub gave: each missing right, and each right more, as `NAME:
/// LEVEL`.
///
/// ```
/// use riff_server::github::differ;
///
/// let need = [("contents".to_owned(), "write".to_owned())].into();
/// let given = [
///     ("contents".to_owned(), "read".to_owned()),
///     ("metadata".to_owned(), "read".to_owned()),
/// ].into();
/// let (missing, extra) = differ(&need, &given);
/// assert_eq!(missing, ["contents: write"]);
/// assert_eq!(extra, ["contents: read", "metadata: read"]);
/// ```
pub fn differ(need: &Rights, given: &Rights) -> (Vec<String>, Vec<String>) {
    let line = |(name, level): (&String, &String)| format!("{name}: {level}");
    let missing = need
        .iter()
        .filter(|(name, level)| given.get(*name) != Some(level))
        .map(line)
        .collect();
    let extra = given
        .iter()
        .filter(|(name, level)| need.get(*name) != Some(level))
        .map(line)
        .collect();
    (missing, extra)
}

/// True when `owner` can be the login of an account on GitHub: 1 to 39
/// letters, digits and dashes, with no dash first.
///
/// ```
/// use riff_server::github::valid_owner;
///
/// assert!(valid_owner("como-technologies") && valid_owner("n8behavior"));
/// assert!(!valid_owner("") && !valid_owner("-x") && !valid_owner("a/b") && !valid_owner("a\"b"));
/// ```
pub fn valid_owner(owner: &str) -> bool {
    (1..=39).contains(&owner.len())
        && !owner.starts_with('-')
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// One state of `riff github create`: the App that its swap makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub app: GitHubApp,
    /// The account that gets the App.
    pub owner: String,
    pub organization: bool,
    pub owner_id: Option<u64>,
    /// The user who ran `riff github create`.
    pub by: String,
}

/// Why riff-server refuses a state (01M6WG52T5PAD9NGQ597CP56K7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateError {
    /// riff-server did not make it, or forgot it.
    Unknown,
    /// It is older than [`STATE_LIFE`].
    Old,
    /// A swap used it.
    Used,
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let why = match self {
            StateError::Unknown => "riff-server did not make this state",
            StateError::Old => "this state is older than 10 minutes",
            StateError::Used => "this state was used",
        };
        write!(f, "{why}. Run riff github create again")
    }
}

/// The states of `riff github create`: each one is good for
/// [`STATE_LIFE`] and for one use.
///
/// ```
/// use std::time::{Duration, Instant};
/// use riff_core::wire::GitHubApp;
/// use riff_server::github::{Pending, StateError, States, STATE_LIFE};
///
/// let pending = Pending {
///     app: GitHubApp::Session,
///     owner: "acme".into(),
///     organization: true,
///     owner_id: None,
///     by: "ada".into(),
/// };
/// let now = Instant::now();
/// let mut states = States::default();
/// let state = states.make(pending.clone(), now);
/// assert_eq!(states.peek(&state, now), Ok(&pending));
/// assert_eq!(states.take(&state, now), Ok(pending.clone()));
/// assert_eq!(states.take(&state, now), Err(StateError::Used));
/// assert_eq!(states.take("made-up", now), Err(StateError::Unknown));
/// let old = states.make(pending, now);
/// let later = now + STATE_LIFE + Duration::from_secs(1);
/// assert_eq!(states.take(&old, later), Err(StateError::Old));
/// ```
#[derive(Debug)]
pub struct States {
    /// How long a state is good.
    life: Duration,
    live: HashMap<String, (Pending, Instant)>,
    /// Each used state, with the time when riff-server made it.
    used: HashMap<String, Instant>,
}

impl Default for States {
    fn default() -> Self {
        States::new(STATE_LIFE)
    }
}

impl States {
    /// The states, each good for `life`.
    pub fn new(life: Duration) -> Self {
        States {
            life,
            live: HashMap::new(),
            used: HashMap::new(),
        }
    }

    /// A new state for `pending`.
    pub fn make(&mut self, pending: Pending, now: Instant) -> String {
        self.forget(now);
        let state = crate::token::random_token();
        self.live.insert(state.clone(), (pending, now));
        state
    }

    /// The App of `state`, which stays good.
    pub fn peek(&self, state: &str, now: Instant) -> Result<&Pending, StateError> {
        match self.live.get(state) {
            Some((_, made)) if now.duration_since(*made) > self.life => Err(StateError::Old),
            Some((pending, _)) => Ok(pending),
            None if self.used.contains_key(state) => Err(StateError::Used),
            None => Err(StateError::Unknown),
        }
    }

    /// The App of `state`, which is then used.
    pub fn take(&mut self, state: &str, now: Instant) -> Result<Pending, StateError> {
        self.peek(state, now)?;
        let (pending, made) = self.live.remove(state).ok_or(StateError::Unknown)?;
        self.used.insert(state.to_owned(), made);
        Ok(pending)
    }

    /// Forgets each state older than two lives: riff-server then calls
    /// it unknown.
    fn forget(&mut self, now: Instant) {
        let life = self.life;
        let kept = |made: &Instant| now.duration_since(*made) <= 2 * life;
        self.live.retain(|_, (_, made)| kept(made));
        self.used.retain(|_, made| kept(made));
    }
}

/// A call to GitHub that failed. The text names the call and the reply
/// of GitHub, never a key or a token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubError(pub String);

impl std::fmt::Display for GitHubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The reply of GitHub to the swap of a code. Its `Debug` hides the key
/// and the secret.
#[derive(Deserialize)]
struct Converted {
    id: u64,
    slug: String,
    pem: String,
    #[serde(default)]
    webhook_secret: Option<String>,
    owner: Account,
}

#[derive(Deserialize)]
struct Account {
    login: String,
}

/// A token that GitHub gave, with its rights and its end.
#[derive(Deserialize)]
struct Granted {
    token: String,
    #[serde(default)]
    permissions: Rights,
    #[serde(default)]
    expires_at: Option<String>,
}

/// Why riff-server has no token of a role for a repository.
#[derive(Clone, Debug, PartialEq, Eq)]
enum NoGrant {
    /// The riff has no App of the role, or the App is not on the
    /// repository: the text names the fix. A session then keeps the
    /// token of the person (01M6WPMWSMQABE97YCMS5Z3M1P).
    Missing(String),
    /// A call to GitHub failed.
    Failed(String),
}

impl NoGrant {
    fn text(self) -> String {
        match self {
            NoGrant::Missing(why) | NoGrant::Failed(why) => why,
        }
    }
}

/// The claims of the JWT of an App.
#[derive(Serialize)]
struct Claims {
    iat: u64,
    exp: u64,
    iss: u64,
}

/// The calls of riff-server to GitHub.
struct Client {
    http: reqwest::Client,
    api: String,
}

impl Client {
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.api))
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header(header::USER_AGENT, "riff-server")
    }

    /// Sends `request`, and gives the reply when its status is a
    /// success. `what` names the call in the error.
    async fn send(
        &self,
        what: &str,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, GitHubError> {
        let reply = request
            .send()
            .await
            .map_err(|e| GitHubError(format!("{what}: {e}")))?;
        checked(what, reply).await
    }

    /// Swaps the code of a new App for its keys.
    async fn convert(&self, code: &str) -> Result<AppKeys, GitHubError> {
        let what = "the swap of the code of the new App";
        let path = format!("/app-manifests/{code}/conversions");
        let reply = self
            .send(what, self.request(reqwest::Method::POST, &path))
            .await?;
        let converted: Converted = reply
            .json()
            .await
            .map_err(|e| GitHubError(format!("{what}: {e}")))?;
        Ok(AppKeys::new(
            converted.id,
            &converted.slug,
            &converted.owner.login,
            &converted.pem,
            converted.webhook_secret.as_deref(),
        ))
    }

    /// The user ID of the bot of the App `slug`.
    async fn bot_id(&self, slug: &str) -> Result<u64, GitHubError> {
        #[derive(Deserialize)]
        struct User {
            id: u64,
        }
        let what = format!("the user {slug}[bot]");
        let path = format!("/users/{slug}%5Bbot%5D");
        let reply = self
            .send(&what, self.request(reqwest::Method::GET, &path))
            .await?;
        let user: User = reply
            .json()
            .await
            .map_err(|e| GitHubError(format!("{what}: {e}")))?;
        Ok(user.id)
    }

    /// A request as the App `app`, with a new JWT of the App.
    fn as_app(
        &self,
        app: &AppKeys,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, GitHubError> {
        Ok(self.request(method, path).bearer_auth(jwt(app)?))
    }

    /// The installation of `app` that holds the repository `repo`, or
    /// `None`.
    async fn installation(&self, app: &AppKeys, repo: &str) -> Result<Option<u64>, GitHubError> {
        #[derive(Deserialize)]
        struct Installation {
            id: u64,
        }
        let what = format!("the installation of {} for {repo}", app.slug);
        let path = format!("/repos/{repo}/installation");
        let reply = self
            .as_app(app, reqwest::Method::GET, &path)?
            .send()
            .await
            .map_err(|e| GitHubError(format!("{what}: {e}")))?;
        if reply.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let installation: Installation = checked(&what, reply)
            .await?
            .json()
            .await
            .map_err(|e| GitHubError(format!("{what}: {e}")))?;
        Ok(Some(installation.id))
    }

    /// True when `app` is installed on the account `owner`.
    async fn installed_on(&self, app: &AppKeys, owner: &str) -> Result<bool, GitHubError> {
        #[derive(Deserialize)]
        struct Installation {
            account: Account,
        }
        let what = format!("the installations of {}", app.slug);
        for page in 1.. {
            let path = format!("/app/installations?per_page=100&page={page}");
            let reply = self
                .send(&what, self.as_app(app, reqwest::Method::GET, &path)?)
                .await?;
            let installations: Vec<Installation> = reply
                .json()
                .await
                .map_err(|e| GitHubError(format!("{what}: {e}")))?;
            if installations
                .iter()
                .any(|i| i.account.login.eq_ignore_ascii_case(owner))
            {
                return Ok(true);
            }
            if installations.len() < 100 {
                break;
            }
        }
        Ok(false)
    }

    /// A token of the installation `installation` for the one repository
    /// `name`, with the rights `rights`.
    async fn token(
        &self,
        app: &AppKeys,
        installation: u64,
        name: &str,
        rights: &Rights,
    ) -> Result<Granted, GitHubError> {
        let what = format!("a token of {}", app.slug);
        let path = format!("/app/installations/{installation}/access_tokens");
        let body = serde_json::json!({ "repositories": [name], "permissions": rights });
        let request = self.as_app(app, reqwest::Method::POST, &path)?.json(&body);
        self.send(&what, request)
            .await?
            .json()
            .await
            .map_err(|e| GitHubError(format!("{what}: {e}")))
    }

    /// Revokes `token` at once.
    async fn revoke(&self, token: &str) -> Result<(), GitHubError> {
        let request = self
            .request(reqwest::Method::DELETE, "/installation/token")
            .bearer_auth(token);
        self.send("the revoke of a token", request)
            .await
            .map(|_| ())
    }
}

/// The reply, when its status is a success. Else the error names `what`,
/// the status and the message of GitHub: a refusal of GitHub holds no
/// secret.
async fn checked(what: &str, reply: reqwest::Response) -> Result<reqwest::Response, GitHubError> {
    let status = reply.status();
    if status.is_success() {
        return Ok(reply);
    }
    let text = reply.text().await.unwrap_or_default();
    let message = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_owned))
        .unwrap_or(text);
    Err(GitHubError(format!(
        "{what}: GitHub replied {status}: {}",
        message.trim()
    )))
}

/// A new JWT of the App `app`, signed with its key: good for
/// [`JWT_LIFE`], from one minute before now, as GitHub asks.
fn jwt(app: &AppKeys) -> Result<String, GitHubError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let claims = Claims {
        iat: now.saturating_sub(60),
        exp: now + JWT_LIFE.as_secs(),
        iss: app.id,
    };
    // The error of a key names no part of the key.
    let key = EncodingKey::from_rsa_pem(app.key.as_bytes())
        .map_err(|e| GitHubError(format!("the key of {} does not read: {e}", app.slug)))?;
    jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|e| GitHubError(format!("the JWT of {}: {e}", app.slug)))
}

/// The GitHub part of a riff-server: the keys of the Apps, the states of
/// `riff github create`, and the calls to GitHub.
pub(crate) struct GitHub {
    settings: Settings,
    client: Client,
    keys: Keys,
    states: Mutex<States>,
}

impl GitHub {
    pub(crate) fn new(settings: Settings, http: reqwest::Client) -> Self {
        GitHub {
            client: Client {
                http: http.clone(),
                api: settings.api.trim_end_matches('/').to_owned(),
            },
            keys: Keys::new(settings.keys.clone(), http),
            states: Mutex::new(States::new(settings.state_life)),
            settings,
        }
    }

    fn states(&self) -> std::sync::MutexGuard<'_, States> {
        self.states.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The App that a new `riff github create` makes first: the first
    /// App that the riff lacks, or the session App again.
    fn first_app(apps: &Apps) -> GitHubApp {
        GitHubApp::ALL
            .into_iter()
            .find(|app| apps.of(*app).is_none())
            .unwrap_or(GitHubApp::Session)
    }

    /// The page that posts the manifest of `pending.app` to GitHub with
    /// `state`.
    fn manifest_page(&self, pending: &Pending, state: &str, public_url: &str) -> String {
        let name = pending.app.name(&pending.owner);
        let action = new_app_url(
            self.settings.web.trim_end_matches('/'),
            &pending.owner,
            pending.organization,
            state,
        );
        let manifest = manifest(pending.app, &pending.owner, public_url).to_string();
        page(
            &format!("Make the GitHub App {name}"),
            &format!(
                "<p>GitHub shows the App {name}. Click <b>Create GitHub App</b> there.</p>\n\
                 <form id=\"make\" method=\"post\" action=\"{action}\">\n\
                 <input type=\"hidden\" name=\"manifest\" value=\"{manifest}\">\n\
                 <button type=\"submit\">Go to GitHub</button>\n\
                 </form>\n\
                 <script>document.getElementById(\"make\").submit();</script>",
                action = escape(&action),
                manifest = escape(&manifest),
            ),
        )
    }

    /// The last page: the install page of each App on the account.
    fn install_page(&self, apps: &Apps, pending: &Pending) -> String {
        let web = self.settings.web.trim_end_matches('/');
        let links: String = GitHubApp::ALL
            .into_iter()
            .filter_map(|app| apps.of(app))
            .map(|keys| {
                let url = install_url(web, &keys.slug, pending.owner_id);
                format!(
                    "<li><a href=\"{}\">Install {}</a></li>\n",
                    escape(&url),
                    escape(&keys.slug)
                )
            })
            .collect();
        page(
            "The GitHub Apps of the riff are ready",
            &format!(
                "<p>Install each App on {owner}, on the repositories of the riff:</p>\n\
                 <ul>\n{links}</ul>\n\
                 <p>riff github create waits in your terminal for the two installs.</p>",
                owner = escape(&pending.owner),
            ),
        )
    }

    /// Swaps `code` for the keys of `pending.app`, reads the user ID of
    /// its bot, and keeps them.
    async fn swap(&self, code: &str, pending: &Pending) -> Result<AppKeys, String> {
        let mut keys = self.client.convert(code).await.map_err(|e| e.0)?;
        match self.client.bot_id(&keys.slug).await {
            Ok(id) => keys.bot_id = Some(id),
            Err(e) => tracing::warn!("no user ID of the bot of {}: {e}", keys.slug),
        }
        self.keys
            .put(pending.app, keys.clone())
            .await
            .map_err(|e| {
                format!(
                    "GitHub made the App {}, but riff-server did not keep its key: {e}. \
                     Delete the App on GitHub, then run riff github create again",
                    keys.slug
                )
            })?;
        Ok(keys)
    }

    /// `granted` when GitHub gave it the rights of `role` and no more
    /// (01M6WPMEEWX09DHJZF18BKBPYW). Else the token goes back to GitHub at
    /// once, and the error says why.
    async fn of_role(&self, role: GitHubRole, granted: Granted) -> Result<Granted, String> {
        let (missing, extra) = differ(&rights(role), &granted.permissions);
        if missing.is_empty() && extra.is_empty() {
            return Ok(granted);
        }
        if let Err(e) = self.client.revoke(&granted.token).await {
            tracing::warn!("the revoke of a token with wrong rights: {e}");
        }
        Err(format!(
            "GitHub gave the token of the {role} other rights than the role has \
             (missing: {}; more: {}). Run riff github check",
            missing.join(", "),
            extra.join(", ")
        ))
    }

    /// A token of the App of `role` for the one repository `repo`, with
    /// the rights of the role: the App, its installation on the
    /// repository, and the token.
    async fn grant<'a>(
        &self,
        apps: &'a Apps,
        role: GitHubRole,
        repo: &str,
    ) -> Result<(&'a AppKeys, Granted), NoGrant> {
        let app = role.app();
        let (owner, name) = repo.split_once('/').unwrap_or((repo, repo));
        let Some(keys) = apps.of(app) else {
            return Err(NoGrant::Missing(format!(
                "the riff has no {} App: riff github create --owner {owner}",
                app_word(app)
            )));
        };
        let installation = match self.client.installation(keys, repo).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                return Err(NoGrant::Missing(format!(
                    "{} is not installed on {repo}: riff github install {owner}",
                    keys.slug
                )));
            }
            Err(e) => return Err(NoGrant::Failed(e.0)),
        };
        let granted = self
            .client
            .token(keys, installation, name, &rights(role))
            .await
            .map_err(|e| NoGrant::Failed(e.0))?;
        Ok((keys, granted))
    }

    /// The check of one role for the repository `repo`.
    async fn check_role(&self, apps: &Apps, role: GitHubRole, repo: &str) -> RoleChecked {
        let mut checked = RoleChecked {
            role,
            bot: apps.of(role.app()).map(AppKeys::bot),
            rights: Rights::new(),
            missing: Vec::new(),
            extra: Vec::new(),
            problem: None,
        };
        let need = rights(role);
        let (keys, granted) = match self.grant(apps, role, repo).await {
            Ok(grant) => grant,
            Err(why) => {
                checked.problem = Some(why.text());
                return checked;
            }
        };
        let revoked = self.client.revoke(&granted.token).await;
        let (missing, extra) = differ(&need, &granted.permissions);
        tracing::info!(
            role = %role,
            repo,
            bot = %keys.bot(),
            passed = missing.is_empty() && extra.is_empty(),
            revoked = revoked.is_ok(),
            "riff github check: a token of the role, revoked at once"
        );
        if let Err(e) = revoked {
            tracing::warn!("riff github check: {e}");
        }
        checked.rights = granted.permissions;
        checked.missing = missing;
        checked.extra = extra;
        checked
    }
}

/// The word of an App in a text: `session` or `lead`.
fn app_word(app: GitHubApp) -> &'static str {
    match app {
        GitHubApp::Session => "session",
        GitHubApp::Lead => "lead",
    }
}

/// `text` for an HTML attribute or text.
///
/// ```
/// assert_eq!(riff_server::github::escape(r#"<a href="x">&'"#), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
/// ```
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// A whole page with the title `title` and the HTML `body`.
fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>riff: {title}</title></head>\n\
         <body>\n<h1>{title}</h1>\n{body}\n</body></html>\n",
        title = escape(title),
    )
}

/// A page for the browser, which no cache keeps.
fn html(status: StatusCode, page: String) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Html(page),
    )
        .into_response()
}

/// The page of a refusal.
fn refused_page(status: StatusCode, why: &str) -> Response {
    html(
        status,
        page(
            "riff did not make the GitHub App",
            &format!("<p>{}</p>", escape(why)),
        ),
    )
}

/// Refuses a caller who is not the owner or an admin of the riff.
fn need_admin(
    s: &crate::Server,
    signed_in: &SignedIn,
    what: &str,
) -> Result<(), (StatusCode, String)> {
    let user = signed_in.who.user();
    if s.engine.read(|state| state.role(user)) < Role::Admin {
        let reason = format!("only the owner or an admin of the riff runs {what}");
        return Err(Failed::Refused(Refused::new(Code::NotAllowed, &reason)).into());
    }
    Ok(())
}

/// A failed read of the keys: 503, with the place.
fn keys_failed(e: crate::keys::KeysError) -> (StatusCode, String) {
    tracing::error!("the keys of the GitHub Apps did not read: {e}");
    (
        StatusCode::SERVICE_UNAVAILABLE,
        format!("riff-server cannot read the keys of the GitHub Apps: {e}"),
    )
}

/// `POST /v1/github/create`: a new state, and the URL of the start page
/// (01M6WG52T5PAD9NGQ597CP56K7).
pub(crate) async fn create(
    AxumState(s): AxumState<crate::Shared>,
    Extension(signed_in): Extension<SignedIn>,
    Json(r): Json<GitHubCreate>,
) -> crate::Reply<GitHubStarted> {
    need_admin(&s, &signed_in, "riff github create")?;
    if !valid_owner(&r.owner) {
        let why = format!("{:?} is no account of GitHub", r.owner);
        return Err((StatusCode::BAD_REQUEST, why));
    }
    let github = &s.github;
    if *github.keys.at() == At::Nowhere {
        return Err((StatusCode::CONFLICT, crate::keys::NOWHERE.to_owned()));
    }
    let apps = github.keys.load().await.map_err(keys_failed)?;
    let app = GitHub::first_app(&apps);
    if let Some(old) = apps.of(app) {
        tracing::warn!(
            "riff github create makes the {} App again: its swap replaces the keys of {}",
            app_word(app),
            old.slug
        );
    }
    let pending = Pending {
        app,
        owner: r.owner.clone(),
        organization: r.organization,
        owner_id: r.owner_id,
        by: signed_in.who.user().to_owned(),
    };
    let state = github.states().make(pending, Instant::now());
    tracing::info!(
        owner = %r.owner,
        by = %signed_in.who.user(),
        "riff github create: a start page for the {} App",
        app_word(app)
    );
    Ok(Json(GitHubStarted {
        url: format!("{}{START_PATH}?state={state}", s.config.public_url),
        makes: GitHubApp::ALL
            .into_iter()
            .skip_while(|made| *made != app)
            .collect(),
    }))
}

/// The query of the start page.
#[derive(Deserialize)]
pub(crate) struct StartQuery {
    #[serde(default)]
    state: String,
}

/// `GET /github/start`: the page that posts the manifest of the first
/// App to GitHub. It does not use the state.
pub(crate) async fn start(
    AxumState(s): AxumState<crate::Shared>,
    Query(q): Query<StartQuery>,
) -> Response {
    let pending = s.github.states().peek(&q.state, Instant::now()).cloned();
    match pending {
        Ok(pending) => html(
            StatusCode::OK,
            s.github
                .manifest_page(&pending, &q.state, &s.config.public_url),
        ),
        Err(e) => {
            tracing::warn!("riff github create: a start page with a bad state: {e}");
            refused_page(StatusCode::BAD_REQUEST, &e.to_string())
        }
    }
}

/// The query that GitHub gives after it made an App.
#[derive(Deserialize)]
pub(crate) struct CreatedQuery {
    #[serde(default)]
    code: String,
    #[serde(default)]
    state: String,
}

/// `GET /github/created`: uses the state, swaps the code for the keys of
/// the App, and gives the page of the next App or the install page
/// (01M6WG52WSWM8A9CN6X448AGAJ).
pub(crate) async fn created(
    AxumState(s): AxumState<crate::Shared>,
    Query(q): Query<CreatedQuery>,
) -> Response {
    let taken = s.github.states().take(&q.state, Instant::now());
    let pending = match taken {
        Ok(pending) => pending,
        Err(e) => {
            tracing::warn!("riff github create: GitHub came back with a bad state: {e}");
            return refused_page(StatusCode::BAD_REQUEST, &e.to_string());
        }
    };
    // The code goes into the path of a call to GitHub: letters, digits,
    // `-` and `_` only.
    let code_chars = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    if q.code.is_empty() || !q.code.chars().all(code_chars) {
        return refused_page(
            StatusCode::BAD_REQUEST,
            "GitHub gave no code that riff can use",
        );
    }
    let keys = match s.github.swap(&q.code, &pending).await {
        Ok(keys) => keys,
        Err(why) => {
            tracing::error!("riff github create: {why}");
            return refused_page(StatusCode::BAD_GATEWAY, &why);
        }
    };
    tracing::info!(
        app = %keys.slug,
        id = keys.id,
        owner = %keys.owner,
        by = %pending.by,
        "riff github create: riff-server keeps the keys of the {} App in {}",
        app_word(pending.app),
        s.github.keys.at().describe()
    );
    let apps = match s.github.keys.load().await {
        Ok(apps) => apps,
        Err(e) => return refused_page(StatusCode::SERVICE_UNAVAILABLE, &e.0),
    };
    if pending.app == GitHubApp::Session {
        let next = Pending {
            app: GitHubApp::Lead,
            ..pending
        };
        let state = s.github.states().make(next.clone(), Instant::now());
        let page = s.github.manifest_page(&next, &state, &s.config.public_url);
        return html(StatusCode::OK, page);
    }
    html(StatusCode::OK, s.github.install_page(&apps, &pending))
}

/// `POST /v1/github/apps`: each App of the riff, and whether it is
/// installed on the account of the query. Each person asks with no
/// account, for example `riff init` for the bot of the guard. Only the
/// owner or an admin asks with an account (01M6WPV8473EBABR1ZMEPZMTQQ).
pub(crate) async fn apps(
    AxumState(s): AxumState<crate::Shared>,
    Extension(signed_in): Extension<SignedIn>,
    Json(r): Json<GitHubApps>,
) -> crate::Reply<GitHubAppsReply> {
    if r.owner.is_some() {
        need_admin(&s, &signed_in, "riff github install")?;
    }
    let apps = s.github.keys.load().await.map_err(keys_failed)?;
    let mut reply = GitHubAppsReply::default();
    for app in GitHubApp::ALL {
        let Some(keys) = apps.of(app) else {
            continue;
        };
        let installed = match &r.owner {
            Some(owner) => Some(
                s.github
                    .client
                    .installed_on(keys, owner)
                    .await
                    .map_err(|e| (StatusCode::BAD_GATEWAY, e.0))?,
            ),
            None => None,
        };
        reply.apps.push(GitHubAppFacts {
            app,
            id: keys.id,
            slug: keys.slug.clone(),
            bot: keys.bot(),
            bot_id: keys.bot_id,
            installed,
        });
    }
    Ok(Json(reply))
}

/// True when `repo` can be a repository `OWNER/NAME` of GitHub.
///
/// ```
/// use riff_server::github::valid_repo;
///
/// assert!(valid_repo("como-technologies/riff") && valid_repo("acme/app.rs"));
/// assert!(!valid_repo("acme") && !valid_repo("acme/") && !valid_repo("acme/a/b"));
/// ```
pub fn valid_repo(repo: &str) -> bool {
    repo.split_once('/').is_some_and(|(owner, name)| {
        valid_owner(owner)
            && !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    })
}

/// The role on GitHub of the session `uri`, from the facts of
/// riff-server (01M6WPMEEWX09DHJZF18BKBPYW): the lead of its repository,
/// else a verifier when it holds a claim `verify-ITEM`, else a worker.
///
/// ```
/// use riff_core::name::SessionUri;
/// use riff_core::wire::GitHubRole;
/// use riff_server::github::role_of;
///
/// let uri = |text: &str| text.parse::<SessionUri>().unwrap();
/// let worker = uri("riff://ada@h/acme/app?session=w1&claim=issue-12");
/// assert_eq!(role_of(&worker), GitHubRole::Worker);
/// assert_eq!(role_of(&uri("riff://ada@h/acme/app?session=w2")), GitHubRole::Worker);
/// let verifier = uri("riff://ada@h/acme/app?session=v1&claim=verify-issue-12");
/// assert_eq!(role_of(&verifier), GitHubRole::Verifier);
/// let lead = uri("riff://ada@h/acme/app?session=l1&lead=true");
/// assert_eq!(role_of(&lead), GitHubRole::Lead);
/// ```
pub fn role_of(uri: &SessionUri) -> GitHubRole {
    if uri.lead() {
        GitHubRole::Lead
    } else if uri
        .claims()
        .iter()
        .any(|claim| claim.starts_with("verify-"))
    {
        GitHubRole::Verifier
    } else {
        GitHubRole::Worker
    }
}

/// `POST /v1/github/token`: a token of the session for its one
/// repository, with the rights of its role and no more
/// (01M6WPMEEWX09DHJZF18BKBPYW). Only the person of the session asks,
/// signed in as the person: the daemon. riff-server takes the role and
/// the repository from its own facts. A riff with no App of the role, or
/// an App that is not on the repository, gives no token and says why:
/// the session keeps the token of the person. The log line names the
/// session, the person, the repository, the role and the end, never the
/// token.
pub(crate) async fn token(
    AxumState(s): AxumState<crate::Shared>,
    Extension(signed_in): Extension<SignedIn>,
    Json(r): Json<GitHubToken>,
) -> crate::Reply<SessionToken> {
    let person = signed_in.who.user().to_owned();
    let refused = |reason: &str| -> (StatusCode, String) {
        Failed::Refused(Refused::new(Code::NotAllowed, reason)).into()
    };
    if signed_in.who.session().is_some() {
        return Err(refused(
            "a session gets its token on GitHub from the daemon of its person",
        ));
    }
    let who = Who::new(&person, Some(&r.session))
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let info = s
        .engine
        .read(|state| state.me(&who, Instant::now(), crate::now_ms()));
    let Some(info) = info else {
        let reason = format!("riff-server knows no session {} of {person}", r.session);
        return Err(refused(&reason));
    };
    let role = role_of(&info.uri);
    let repo = match info.uri.place().repo() {
        Repo::Git { owner, name } => format!("{owner}/{name}"),
        Repo::None => String::new(),
    };
    if !valid_repo(&repo) {
        let why = format!("the session {} works in no repository of GitHub", r.session);
        return Err((StatusCode::BAD_REQUEST, why));
    }
    let apps = s.github.keys.load().await.map_err(keys_failed)?;
    let keys = apps.of(role.app());
    let mut reply = SessionToken {
        role,
        repo: repo.clone(),
        bot: keys.map(AppKeys::bot),
        bot_id: keys.and_then(|keys| keys.bot_id),
        token: None,
        rights: Rights::new(),
        ends: None,
        problem: None,
    };
    let granted = match s.github.grant(&apps, role, &repo).await {
        Ok((_, granted)) => granted,
        Err(NoGrant::Missing(why)) => {
            tracing::info!(
                session = %r.session,
                person,
                repo,
                role = %role,
                "no token of an App for the session, which keeps the token of its person: {why}"
            );
            reply.problem = Some(why);
            return Ok(Json(reply));
        }
        Err(NoGrant::Failed(why)) => {
            tracing::warn!(session = %r.session, person, repo, role = %role, "no token of the session: {why}");
            return Err((StatusCode::BAD_GATEWAY, why));
        }
    };
    let granted = match s.github.of_role(role, granted).await {
        Ok(granted) => granted,
        Err(why) => {
            tracing::warn!(session = %r.session, person, repo, role = %role, "{why}");
            return Err((StatusCode::BAD_GATEWAY, why));
        }
    };
    tracing::info!(
        session = %r.session,
        person,
        repo,
        role = %role,
        ends = granted.expires_at.as_deref().unwrap_or("unknown"),
        "a token of a session on GitHub"
    );
    reply.rights = granted.permissions;
    reply.ends = granted.expires_at;
    reply.token = Some(granted.token);
    Ok(Json(reply))
}

/// `POST /v1/github/daemon`: a token of the lead App for the writes of the
/// daemon of the person on one repository, with the rights of the lead and
/// no more (01M6XZJFGNVG6B7DF5VQ3H1DNX). Each person asks, signed in as
/// the person, with no session token: a person can start a lead in each
/// repository of the riff, and the lead gets the same token. A riff with
/// no lead App, or a lead App that is not on the repository, gives no
/// token and says why: the daemon writes with the token of the person.
/// The log line names the person, the repository and the end, never the
/// token.
pub(crate) async fn daemon_token(
    AxumState(s): AxumState<crate::Shared>,
    Extension(signed_in): Extension<SignedIn>,
    Json(r): Json<GitHubDaemonToken>,
) -> crate::Reply<SessionToken> {
    let person = signed_in.who.user().to_owned();
    if signed_in.who.session().is_some() {
        let reason = "a session gets no token of the daemon: the daemon of its person asks";
        return Err(Failed::Refused(Refused::new(Code::NotAllowed, reason)).into());
    }
    let repo = r.repo;
    if !valid_repo(&repo) {
        let why = format!("{repo:?} is no repository OWNER/NAME of GitHub");
        return Err((StatusCode::BAD_REQUEST, why));
    }
    let role = GitHubRole::Lead;
    let apps = s.github.keys.load().await.map_err(keys_failed)?;
    let keys = apps.of(role.app());
    let mut reply = SessionToken {
        role,
        repo: repo.clone(),
        bot: keys.map(AppKeys::bot),
        bot_id: keys.and_then(|keys| keys.bot_id),
        token: None,
        rights: Rights::new(),
        ends: None,
        problem: None,
    };
    let granted = match s.github.grant(&apps, role, &repo).await {
        Ok((_, granted)) => granted,
        Err(NoGrant::Missing(why)) => {
            tracing::info!(
                person,
                repo,
                "no token of the lead App for the daemon, which writes with the token of its person: {why}"
            );
            reply.problem = Some(why);
            return Ok(Json(reply));
        }
        Err(NoGrant::Failed(why)) => {
            tracing::warn!(person, repo, "no token of the daemon: {why}");
            return Err((StatusCode::BAD_GATEWAY, why));
        }
    };
    let granted = match s.github.of_role(role, granted).await {
        Ok(granted) => granted,
        Err(why) => {
            tracing::warn!(person, repo, "{why}");
            return Err((StatusCode::BAD_GATEWAY, why));
        }
    };
    tracing::info!(
        person,
        repo,
        ends = granted.expires_at.as_deref().unwrap_or("unknown"),
        "a token of the daemon on GitHub"
    );
    reply.rights = granted.permissions;
    reply.ends = granted.expires_at;
    reply.token = Some(granted.token);
    Ok(Json(reply))
}

/// `POST /v1/github/check`: a token of each role for the repository,
/// checked and revoked at once (01M6WG534PPE2JW8JQFRJDEK86).
pub(crate) async fn check(
    AxumState(s): AxumState<crate::Shared>,
    Extension(_): Extension<SignedIn>,
    Json(r): Json<GitHubCheck>,
) -> crate::Reply<GitHubChecked> {
    if !valid_repo(&r.repo) {
        let why = format!("{:?} is no repository OWNER/NAME of GitHub", r.repo);
        return Err((StatusCode::BAD_REQUEST, why));
    }
    let apps = s.github.keys.load().await.map_err(keys_failed)?;
    let mut checked = GitHubChecked::default();
    for role in GitHubRole::ALL {
        checked
            .roles
            .push(s.github.check_role(&apps, role, &r.repo).await);
    }
    Ok(Json(checked))
}

/// `POST /hooks/github`: each delivery of the webhook of the session
/// App. riff-server takes it with status 202, and drops it
/// (01M6WG52QHC8E24RJ33ZPVHZSJ).
pub(crate) async fn hook() -> StatusCode {
    StatusCode::ACCEPTED
}
