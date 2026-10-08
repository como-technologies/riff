//! The forge token of each session: riff-server makes it with the GitHub
//! App of riff (#628).
//!
//! # Design
//!
//! One GitHub App of riff serves many accounts: organizations and
//! personal accounts install it on their repositories. Its ID and its
//! private key are one secret in Secret Manager ([`store`]). Only the
//! service account of riff-server reads it and adds a version. The
//! server reads it at its start. `riff forge create` makes the App and
//! puts it there ([`manifest`]), so the key never goes to the machine of
//! a person (01M4CTAYRSC27Q6AAGTH9CBD7Q). A server for a test or a
//! person can take the App from [`APP_VAR`] and [`KEY_VAR`] in place of
//! the store.
//!
//! The wrapper of a session (`riff workers run`, outside the sandbox)
//! asks `POST /v1/forge/token` with the URI of its session. The server
//! picks the role and the repository from its own facts
//! (`State::forge_fact`, [`riff_core::forge::role_of`]): the lead, a
//! `verify-` claim, else the worker. A session that the server does not
//! know, or whose claims ended, gets no token ([`Refusal::NoSession`]):
//! so the wrapper of a worker registers its session first. A URI with
//! no session is the wrapper of the lead, before its session starts: it
//! gets the lead token only while its person has a lead in the
//! repository of the URI ([`Refusal::NotLead`]). So a session cannot
//! ask for more rights (01M4CNN37FYYB99BS6QV2FFWZ8). The server finds the
//! installation of the App on that repository, and makes a token there
//! for that one repository and the
//! [`permissions`] of the role. It
//! refuses a token with other rights
//! ([`check_given`]).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant W as riff workers run (outside the sandbox)
//!     participant S as riff-server
//!     participant G as GitHub
//!     W->>S: POST /v1/forge/token (signed-in session)
//!     S->>S: role from the facts of the server: lead, a verify- claim, else worker
//!     S->>G: JWT of the App: GET /repos/OWNER/NAME/installation
//!     S->>G: POST access_tokens: the one repository, the rights of the role
//!     G-->>S: a token for one hour
//!     S->>G: revoke the old token of this session, when the role changed
//!     S-->>W: the token and its end time
//!     W->>W: token files for gh and git
//! ```
//!
//! - A riff with no sign-in gives no token: the server cannot know the
//!   person (01M4CHQR5ZFQCQ7CC1CGHYFY8S). A call with no token is
//!   refused.
//! - The App is public: each account can install it. The server makes
//!   tokens and checks only for the repositories of the accounts that
//!   the owner or an admin allowed (`riff forge allow OWNER`, the record
//!   `forge_allowed`, [`Refusal::NotAllowed`],
//!   01M4CNN3C41DBHVYX87Q17GW2C).
//! - The installation comes from the repository of each session, so one
//!   server gives tokens for each repository where the App is
//!   installed. A repository with no installation gives an error that
//!   names `riff forge install OWNER` ([`Refusal::NoInstallation`],
//!   01M4CHQRCP5DFNRDHKMBNHTD5T).
//! - `riff forge check` asks the server for a token of each role, and
//!   the server revokes each one at once ([`Forge::check`],
//!   01M4CHQREYBFR465528EYTVDYE).
//! - The server keeps the last token of each session in memory
//!   ([`Forge::held_role`]). At a claim, a release, the end of a
//!   session, the end of the allow of an account, and each
//!   [`SETTLE_EVERY`], it compares the role of each held token with the
//!   facts, and revokes each token whose role changed or whose claims
//!   ended ([`Forge::settle`], 01M4CNN39TTK36GX34RCWKES80). So a session
//!   that died with no `end` call loses its token soon after its claims
//!   end. The wrapper then asks for a new one.
//! - At a renew of a session, the server revokes the old token after it
//!   made the new one ([`Forge::give`]): a session never holds two good
//!   tokens.
//! - Each token gives one log line ([`TARGET`], `result` `token`): the
//!   session, the repository, the role and the end time. Each revoke
//!   gives one line with `result` `revoked`. A line never holds the
//!   token (01M4CHQRAFPQDVB67XGDD4N9GB).

pub mod manifest;
pub mod store;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::http::StatusCode;
use riff_core::forge::{Access, TokenRole, check_given, permissions};
use riff_core::name::Who;
use riff_core::wire::{ForgeCheckReply, ForgeTokenReply, RoleCheck};
use serde::{Deserialize, Serialize};

/// The variable that holds the ID of the GitHub App.
pub const APP_VAR: &str = "RIFF_FORGE_APP";

/// The variable that holds the private key of the App, in PEM form. The
/// deploy fills it from Secret Manager.
pub const KEY_VAR: &str = "RIFF_FORGE_KEY";

/// The variable that names the base URL of the GitHub API. A test sets
/// it to a fake API.
pub const API_VAR: &str = "RIFF_GITHUB_API";

/// The GitHub API when [`API_VAR`] is not set.
pub const GITHUB_API: &str = "https://api.github.com";

/// The target of the log lines of the forge tokens.
pub const TARGET: &str = "riff_server::forge";

/// The server compares the held tokens with the facts this often, so a
/// session that died with no `end` call loses its token soon after its
/// claims end.
pub const SETTLE_EVERY: Duration = Duration::from_secs(60);

/// The longest wait for a call to GitHub.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The GitHub App of riff: its ID and its private key.
#[derive(Clone)]
pub struct App {
    id: u64,
    key: jsonwebtoken::EncodingKey,
}

impl fmt::Debug for App {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("App")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Two Apps are the same when they have the same ID: GitHub gives one
/// App each key.
impl PartialEq for App {
    fn eq(&self, other: &App) -> bool {
        self.id == other.id
    }
}

impl Eq for App {}

impl App {
    /// The App with the ID `id` and the private key `pem` (PKCS#1 or
    /// PKCS#8, as GitHub gives it).
    pub fn new(id: u64, pem: &[u8]) -> Result<App, String> {
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(pem)
            .map_err(|_| "the key of the GitHub App is not an RSA private key in PEM form")?;
        Ok(App { id, key })
    }

    /// The ID of the App.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The JWT of the App at the Unix time `now`: it is good from one
    /// minute before `now` (for a clock that runs late) for ten minutes,
    /// the longest time that GitHub takes.
    pub fn jwt(&self, now: u64) -> Result<String, String> {
        #[derive(Serialize)]
        struct Claims {
            iat: u64,
            exp: u64,
            iss: String,
        }
        let claims = Claims {
            iat: now.saturating_sub(60),
            exp: now + 9 * 60,
            iss: self.id.to_string(),
        };
        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        jsonwebtoken::encode(&header, &claims, &self.key)
            .map_err(|e| format!("cannot sign the JWT of the GitHub App: {e}"))
    }
}

/// The App and the GitHub API of a server that gives forge tokens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub app: App,
    /// The base URL of the GitHub API, for example [`GITHUB_API`].
    pub api: String,
}

impl Settings {
    /// The settings of the variables `app`, `key` and `api` of
    /// [`APP_VAR`], [`KEY_VAR`] and [`API_VAR`]. `None` when the App is
    /// not set: then the server gives no forge token. An error when only
    /// one of the two is set, or when they are bad.
    ///
    /// ```
    /// use riff_server::forge::Settings;
    ///
    /// assert!(Settings::of(None, None, None).unwrap().is_none());
    /// assert!(Settings::of(Some("12"), None, None).is_err());
    /// assert!(Settings::of(Some("x"), Some("pem"), None).is_err());
    /// ```
    pub fn of(
        app: Option<&str>,
        key: Option<&str>,
        api: Option<&str>,
    ) -> Result<Option<Settings>, String> {
        fn set(v: Option<&str>) -> Option<&str> {
            v.map(str::trim).filter(|v| !v.is_empty())
        }
        let (app, key) = match (set(app), set(key)) {
            (None, None) => return Ok(None),
            (Some(app), Some(key)) => (app, key),
            _ => return Err(format!("set both {APP_VAR} and {KEY_VAR}, or neither")),
        };
        let id = app
            .parse()
            .map_err(|_| format!("{APP_VAR} is no App ID: {app}"))?;
        Ok(Some(Settings {
            app: App::new(id, key.as_bytes())?,
            api: set(api)
                .unwrap_or(GITHUB_API)
                .trim_end_matches('/')
                .to_owned(),
        }))
    }
}

/// Why the server gives no token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The call has no sign-in, or the riff has none.
    NoSignIn,
    /// The server has no GitHub App.
    NoApp,
    /// The session is in no repository.
    NoRepository,
    /// The server does not know the session, or it ended, or it had no
    /// sign of life for the claim grace.
    NoSession,
    /// The person is not the lead of the repository.
    NotLead { repo: String },
    /// No owner or admin allowed the GitHub account of the repository.
    NotAllowed { owner: String },
    /// The App is not installed on the repository.
    NoInstallation { repo: String },
    /// GitHub gave no token of the role.
    GitHub(String),
}

impl Refusal {
    /// The HTTP status of the refusal.
    pub fn status(&self) -> StatusCode {
        match self {
            Refusal::NoSignIn
            | Refusal::NoSession
            | Refusal::NotLead { .. }
            | Refusal::NotAllowed { .. } => StatusCode::FORBIDDEN,
            Refusal::NoRepository => StatusCode::BAD_REQUEST,
            Refusal::NoApp | Refusal::NoInstallation { .. } => StatusCode::CONFLICT,
            Refusal::GitHub(_) => StatusCode::BAD_GATEWAY,
        }
    }
}

impl fmt::Display for Refusal {
    /// The text for a person.
    ///
    /// ```
    /// use riff_server::forge::Refusal;
    ///
    /// let no = Refusal::NoInstallation { repo: "acme/app".into() };
    /// assert_eq!(
    ///     no.to_string(),
    ///     "the GitHub App of riff is not installed on acme/app. An admin of acme runs: riff forge install acme"
    /// );
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::NoSignIn => f.write_str(
                "a riff with no sign-in gives no forge token: the server cannot know the person",
            ),
            Refusal::NoApp => f.write_str(
                "this riff has no GitHub App. An admin of the riff gives it one: riff cloud forge",
            ),
            Refusal::NoRepository => f.write_str("the session is in no repository"),
            Refusal::NoSession => f.write_str(
                "the server does not know this session, or it ended: a session gets a forge \
                 token only while it is in the riff",
            ),
            Refusal::NotAllowed { owner } => write!(
                f,
                "this riff makes no forge token for the repositories of {owner}. The owner or \
                 an admin of the riff runs: riff forge allow {owner}"
            ),
            Refusal::NotLead { repo } => write!(
                f,
                "you are not the lead of {repo}: only the lead of a repository gets a lead \
                 token with no session"
            ),
            Refusal::NoInstallation { repo } => {
                let owner = repo.split('/').next().unwrap_or(repo);
                write!(
                    f,
                    "the GitHub App of riff is not installed on {repo}. An admin of {owner} \
                     runs: riff forge install {owner}"
                )
            }
            Refusal::GitHub(why) => f.write_str(why),
        }
    }
}

impl From<Refusal> for (StatusCode, String) {
    fn from(refusal: Refusal) -> Self {
        (refusal.status(), refusal.to_string())
    }
}

/// The last token that the server gave to one session at one
/// repository.
#[derive(Clone, PartialEq, Eq)]
struct Held {
    role: TokenRole,
    token: String,
}

#[derive(Deserialize)]
struct Installation {
    id: u64,
}

#[derive(Serialize)]
struct Ask<'a> {
    repositories: [&'a str; 1],
    permissions: BTreeMap<&'static str, Access>,
}

#[derive(Deserialize)]
struct Given {
    token: String,
    expires_at: String,
    #[serde(default)]
    permissions: BTreeMap<String, Access>,
}

/// The forge tokens of a server. See the module docs.
pub struct Forge {
    /// The App. `riff forge create` sets a new one at run time.
    settings: RwLock<Option<Settings>>,
    /// The base URL of the GitHub API.
    api: String,
    /// Where the server keeps the App ([`store`]), if anywhere.
    store: Option<store::Store>,
    /// The starts of `riff forge create` ([`manifest`]).
    starts: manifest::Starts,
    http: reqwest::Client,
    held: Mutex<HashMap<(Who, String), Held>>,
}

impl fmt::Debug for Forge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Forge")
            .field("settings", &self.settings)
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Forge {
    /// The forge tokens of `settings`. With no settings, each ask is
    /// refused with [`Refusal::NoApp`].
    pub fn new(settings: Option<Settings>) -> Forge {
        Forge::with_store(settings, None, GITHUB_API)
    }

    /// The forge tokens of `settings`, with the App kept in `store`, and
    /// the GitHub API at `api` when `settings` has none.
    pub fn with_store(
        settings: Option<Settings>,
        store: Option<store::Store>,
        api: &str,
    ) -> Forge {
        let api = settings
            .as_ref()
            .map_or(api, |s| s.api.as_str())
            .trim_end_matches('/')
            .to_owned();
        Forge {
            settings: RwLock::new(settings),
            api,
            store,
            starts: manifest::Starts::default(),
            http: crate::oidc::client(TIMEOUT),
            held: Mutex::new(HashMap::new()),
        }
    }

    /// The ID of the App, if the server has one.
    pub fn app(&self) -> Option<u64> {
        self.current().map(|s| s.app.id)
    }

    /// The App now, if the server has one.
    fn current(&self) -> Option<Settings> {
        self.settings
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// From now on, the server makes each token with `app`.
    pub fn set_app(&self, app: App) {
        let settings = Settings {
            app,
            api: self.api.clone(),
        };
        *self.settings.write().unwrap_or_else(|p| p.into_inner()) = Some(settings);
    }

    fn settings(&self) -> Result<Settings, Refusal> {
        self.current().ok_or(Refusal::NoApp)
    }

    fn held(&self) -> std::sync::MutexGuard<'_, HashMap<(Who, String), Held>> {
        self.held.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The role of the token that the server gave last to `who` at
    /// `repo`, if it holds one.
    pub fn held_role(&self, who: &Who, repo: &str) -> Option<TokenRole> {
        self.held()
            .get(&(who.clone(), repo.to_owned()))
            .map(|h| h.role)
    }

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        what: &str,
    ) -> Result<(StatusCode, Option<T>), Refusal> {
        let response = request
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "riff-server")
            .send()
            .await
            .map_err(|e| Refusal::GitHub(format!("cannot reach the GitHub API for {what}: {e}")))?;
        let status = response.status();
        if !status.is_success() {
            return Ok((status, None));
        }
        let body = response.json().await.map_err(|e| {
            Refusal::GitHub(format!(
                "the GitHub API gave no valid reply for {what}: {e}"
            ))
        })?;
        Ok((status, Some(body)))
    }

    /// The ID of the installation of the App on `repo`.
    async fn installation(&self, s: &Settings, repo: &str) -> Result<u64, Refusal> {
        let url = format!("{}/repos/{repo}/installation", s.api);
        let jwt = s.app.jwt(now_secs()).map_err(Refusal::GitHub)?;
        let what = format!("the installation of the App on {repo}");
        match self
            .send::<Installation>(self.http.get(url).bearer_auth(jwt), &what)
            .await?
        {
            (_, Some(installation)) => Ok(installation.id),
            (StatusCode::NOT_FOUND, None) => Err(Refusal::NoInstallation {
                repo: repo.to_owned(),
            }),
            (status, None) => Err(Refusal::GitHub(format!(
                "the GitHub API refused {what}: {status}"
            ))),
        }
    }

    /// A new token of `role` for `repo` only, with only the permissions
    /// of the role.
    async fn make(&self, repo: &str, role: TokenRole) -> Result<ForgeTokenReply, Refusal> {
        let s = self.settings()?;
        let installation = self.installation(&s, repo).await?;
        let name = repo.rsplit('/').next().unwrap_or(repo);
        let url = format!("{}/app/installations/{installation}/access_tokens", s.api);
        let jwt = s.app.jwt(now_secs()).map_err(Refusal::GitHub)?;
        let request = self.http.post(url).bearer_auth(jwt).json(&Ask {
            repositories: [name],
            permissions: permissions(role),
        });
        let what = format!("a {role} token for {repo}");
        let given = match self.send::<Given>(request, &what).await? {
            (_, Some(given)) => given,
            (status, None) => {
                return Err(Refusal::GitHub(format!(
                    "the GitHub API refused {what}: {status}"
                )));
            }
        };
        if let Err(why) = check_given(role, &given.permissions) {
            // A token with the wrong rights is no use: it ends now.
            self.revoke(&given.token).await;
            return Err(Refusal::GitHub(why));
        }
        Ok(ForgeTokenReply {
            role,
            repo: repo.to_owned(),
            token: given.token,
            ends_ms: chrono::DateTime::parse_from_rfc3339(&given.expires_at)
                .ok()
                .and_then(|t| u64::try_from(t.timestamp_millis()).ok())
                .unwrap_or(0),
            permissions: given.permissions,
        })
    }

    /// Revokes `token` at GitHub. A token that GitHub does not take any
    /// more (401) counts as revoked. A fault gives a warning: the token
    /// ends by itself within one hour.
    async fn revoke(&self, token: &str) {
        let Ok(s) = self.settings() else {
            return;
        };
        let url = format!("{}/installation/token", s.api);
        let request = self
            .http
            .delete(url)
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "riff-server");
        match request.send().await {
            Ok(r) if r.status().is_success() || r.status() == StatusCode::UNAUTHORIZED => {}
            Ok(r) => tracing::warn!(target: TARGET, "the revoke of a token failed: {}", r.status()),
            Err(e) => tracing::warn!(target: TARGET, "the revoke of a token failed: {e}"),
        }
    }

    /// Revokes the held token of `who` at `repo`, and writes its line.
    async fn revoke_held(&self, who: &Who, repo: &str, held: Held, why: &str) {
        self.revoke(&held.token).await;
        tracing::info!(
            target: TARGET,
            result = "revoked",
            session = %who,
            repo,
            role = %held.role,
            why,
            "revoked"
        );
    }

    /// A token of `role` for the session `who` at `repo`. The caller
    /// picked `role` from the facts of the server. For a session (a
    /// `who` with a session ID), it first revokes each other token of
    /// that session: of another role, or of another repository. So a
    /// session never holds the rights of two roles. At a renew (a token
    /// of the same role), it revokes the old token after it made the new
    /// one, so a session never holds two good tokens. A token of the
    /// lead of a person (a `who` with no session) stays good until it
    /// ends: two wrappers of the same person can share the role.
    pub async fn give(
        &self,
        who: &Who,
        repo: &str,
        role: TokenRole,
    ) -> Result<ForgeTokenReply, Refusal> {
        self.settings()?;
        let session = who.session().is_some();
        let old: Vec<((Who, String), Held)> = {
            let mut held = self.held();
            let other = |(w, r): &(Who, String), h: &Held| {
                w == who && (r == repo || session) && (r != repo || h.role != role)
            };
            let keys: Vec<(Who, String)> = held
                .iter()
                .filter(|(k, h)| other(k, h))
                .map(|(k, _)| k.clone())
                .collect();
            keys.into_iter()
                .filter_map(|k| held.remove(&k).map(|h| (k, h)))
                .collect()
        };
        for ((w, r), h) in old {
            self.revoke_held(&w, &r, h, "the role changed").await;
        }
        let token = self.make(repo, role).await?;
        let renewed = self.held().insert(
            (who.clone(), repo.to_owned()),
            Held {
                role,
                token: token.token.clone(),
            },
        );
        tracing::info!(
            target: TARGET,
            result = "token",
            session = %who,
            repo,
            role = %role,
            ends_ms = token.ends_ms,
            "token"
        );
        if let Some(old) = renewed.filter(|old| session && old.token != token.token) {
            self.revoke_held(who, repo, old, "a new token of the role")
                .await;
        }
        Ok(token)
    }

    /// Compares each held token of a session with the facts of the
    /// server: `now` gives the role and the repository of a session, or
    /// `None` when the session is gone. It revokes each token whose role
    /// or repository changed. A token of the lead of a person (a `who`
    /// with no session) stays: it has no claims.
    pub async fn settle(&self, now: impl Fn(&Who) -> Option<(TokenRole, String)>) {
        let changed: Vec<((Who, String), Held)> = {
            let mut held = self.held();
            let keys: Vec<(Who, String)> = held
                .iter()
                .filter(|((who, repo), h)| {
                    who.session().is_some()
                        && now(who).is_none_or(|(role, r)| role != h.role || &r != repo)
                })
                .map(|(k, _)| k.clone())
                .collect();
            keys.into_iter()
                .filter_map(|k| held.remove(&k).map(|h| (k, h)))
                .collect()
        };
        for ((who, repo), held) in changed {
            self.revoke_held(&who, &repo, held, "the claims changed")
                .await;
        }
    }

    /// The sessions that hold a token now.
    pub fn holders(&self) -> Vec<Who> {
        self.held().keys().map(|(who, _)| who.clone()).collect()
    }

    /// `riff forge check`: a token of each role for `repo`, checked and
    /// revoked at once. The reply holds no token.
    pub async fn check(&self, repo: &str) -> Result<ForgeCheckReply, Refusal> {
        let app = self.settings()?.app.id;
        let mut roles = Vec::new();
        for role in TokenRole::ALL {
            match self.make(repo, role).await {
                Ok(token) => {
                    self.revoke(&token.token).await;
                    roles.push(RoleCheck {
                        role,
                        permissions: token.permissions,
                        error: None,
                    });
                }
                Err(refusal @ Refusal::NoInstallation { .. }) => return Err(refusal),
                Err(refusal) => roles.push(RoleCheck {
                    role,
                    permissions: BTreeMap::new(),
                    error: Some(refusal.to_string()),
                }),
            }
        }
        Ok(ForgeCheckReply {
            repo: repo.to_owned(),
            app,
            roles,
        })
    }
}

#[cfg(test)]
#[path = "../tests/common/github.rs"]
mod fake_github;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logline::testing::Capture;
    use fake_github::FakeGitHub;

    const KEY: &str = include_str!("../testdata/test-only-rsa-key.pem");

    fn who(text: &str) -> Who {
        let uri: riff_core::name::SessionUri = text.parse().unwrap();
        uri.who().clone()
    }

    async fn forge() -> (Forge, FakeGitHub) {
        let github = FakeGitHub::start(&[("acme/app", 11)]).await;
        let settings = Settings::of(Some("7"), Some(KEY), Some(&github.url)).unwrap();
        (Forge::new(settings), github)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn each_token_and_each_revoke_has_a_log_line_with_no_token() {
        let capture = Capture::start();
        let (forge, github) = forge().await;
        let w = who("riff://mike@pangolin/acme/app?session=w1");
        let worker = forge.give(&w, "acme/app", TokenRole::Worker).await.unwrap();
        let verifier = forge
            .give(&w, "acme/app", TokenRole::Verifier)
            .await
            .unwrap();
        assert_eq!(github.revoked(), std::slice::from_ref(&worker.token));
        let lines = capture.lines();
        let tokens = capture.results("token");
        assert_eq!(tokens.len(), 2, "{lines:?}");
        assert_eq!(tokens[0]["session"], "mike/w1");
        assert_eq!(tokens[0]["repo"], "acme/app");
        assert_eq!(tokens[0]["role"], "worker");
        assert_eq!(tokens[1]["role"], "verifier");
        assert!(tokens[1]["ends_ms"].as_u64().unwrap() > 0);
        let revoked = capture.results("revoked");
        assert_eq!(revoked.len(), 1, "{lines:?}");
        assert_eq!(revoked[0]["role"], "worker");
        let text = capture.text();
        for token in [&worker.token, &verifier.token] {
            assert!(
                !text.contains(token.as_str()),
                "a line holds a token: {text}"
            );
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn settle_revokes_a_token_whose_role_changed_and_keeps_the_lead() {
        let (forge, github) = forge().await;
        let w = who("riff://mike@pangolin/acme/app?session=w1");
        let v = who("riff://mike@pangolin/acme/app?session=v1");
        let lead = who("riff://mike@pangolin/acme/app");
        let worker = forge.give(&w, "acme/app", TokenRole::Worker).await.unwrap();
        let verifier = forge
            .give(&v, "acme/app", TokenRole::Verifier)
            .await
            .unwrap();
        let led = forge
            .give(&lead, "acme/app", TokenRole::Lead)
            .await
            .unwrap();
        // w1 claimed a verify; v1 still verifies; the lead has no facts.
        forge
            .settle(|who| match who.session() {
                Some("w1" | "v1") => Some((TokenRole::Verifier, "acme/app".into())),
                _ => None,
            })
            .await;
        assert_eq!(github.revoked(), [worker.token]);
        assert_eq!(forge.held_role(&w, "acme/app"), None);
        assert_eq!(forge.held_role(&v, "acme/app"), Some(TokenRole::Verifier));
        assert_eq!(forge.held_role(&lead, "acme/app"), Some(TokenRole::Lead));
        // A session that ended loses its token.
        forge.settle(|_| None).await;
        assert_eq!(github.revoked().last(), Some(&verifier.token));
        assert!(!github.revoked().contains(&led.token));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_renew_revokes_the_old_token_after_it_makes_the_new_one() {
        let (forge, github) = forge().await;
        let w = who("riff://mike@pangolin/acme/app?session=w1");
        let old = forge.give(&w, "acme/app", TokenRole::Worker).await.unwrap();
        let new = forge.give(&w, "acme/app", TokenRole::Worker).await.unwrap();
        assert_eq!(github.revoked(), [old.token]);
        assert!(!github.revoked().contains(&new.token));
        assert_eq!(forge.held_role(&w, "acme/app"), Some(TokenRole::Worker));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_lead_token_of_a_person_stays_good_until_it_ends() {
        let (forge, github) = forge().await;
        let lead = who("riff://mike@pangolin/acme/app");
        forge
            .give(&lead, "acme/app", TokenRole::Lead)
            .await
            .unwrap();
        forge
            .give(&lead, "acme/app", TokenRole::Lead)
            .await
            .unwrap();
        assert!(github.revoked().is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_server_with_no_app_refuses() {
        let forge = Forge::new(None);
        let w = who("riff://mike@pangolin/acme/app?session=w1");
        let refused = forge.give(&w, "acme/app", TokenRole::Worker).await;
        assert_eq!(refused.unwrap_err(), Refusal::NoApp);
        assert_eq!(forge.check("acme/app").await.unwrap_err(), Refusal::NoApp);
    }
}
