//! The forge token of a role: a GitHub App gives each session a token
//! with only the rights of its role (#610).
//!
//! # Design
//!
//! The person makes one GitHub App and installs it on the repository.
//! `riff forge app ID KEY` saves its ID in the settings (`forge.app`)
//! and its private key in [`App::key_path`], a file that only the
//! person reads (01M4BV7057YSHEMEHKXK20X0GJ). No profile reads that
//! folder ([`crate::profile::Session::secrets`]), so a session never
//! sees the key.
//!
//! `riff workers run` stays outside the sandbox of `claude`. It makes an
//! installation token for the role of the session: the GitHub API
//! `POST /app/installations/ID/access_tokens` with the repository of
//! the session and the [`permissions`] of the role
//! (01M4BV707FYHJDNC1499YAWR8D). It refuses a token with a right that
//! it did not ask for, or with fewer rights than the role needs.
//!
//! A token ends after one hour. A process cannot change the
//! environment of `claude` after its start. So the token is in files of
//! the temp folder of the session ([`Files`]), and the wrapper writes
//! them again ([`keep`]):
//!
//! - `gh` reads the token from `GH_CONFIG_DIR/hosts.yml`.
//! - git reads it through the credential helper `riff forge
//!   credential`, which [`Files::env`] sets for the github.com URLs.
//! - The wrapper removes `GH_TOKEN` and `GITHUB_TOKEN`, so no token of
//!   the person reaches the session (01M4BV709WGHZ57AM3STC15B69).
//!
//! The role of a worker session follows its claims ([`role_of`]): a
//! `verify-` claim gives the verifier token, each other case the
//! worker token. The wrapper reads the claims from riff-server, not
//! from the session, so a session cannot ask for more rights
//! (01M4BV70C3P5CZFBSFYFWEWRRA). It looks each [`LOOK_EVERY`], and at
//! once when `riff mcp` asks after a claim or a release
//! ([`Files::ask`]). It makes a new token when the role changes, or
//! [`RENEW_BEFORE`] before the token ends ([`due`],
//! 01M4BV70JZNMT3X99E77GC58K9). At a change of role, it revokes the
//! old token first ([`GitHub::revoke`], 01M4BYGV74T1R2H9D1RX6RTC6Z).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant W as riff workers run (outside the sandbox)
//!     participant S as riff-server
//!     participant G as GitHub API
//!     participant C as claude, gh, git (in the sandbox)
//!     W->>S: GET /v1/me: the claims of the session
//!     W->>G: JWT of the App: POST access_tokens (repository, permissions of the role)
//!     G-->>W: token, ends in 1 hour
//!     W->>C: token files in the temp folder
//!     C->>C: gh reads hosts.yml, git runs riff forge credential
//!     C->>W: riff mcp claim: the ask file
//!     W->>S: the claims again
//!     W->>G: a token of the new role
//! ```
//!
//! | Role | GitHub permissions |
//! |---|---|
//! | lead | metadata, actions, checks, statuses: read; contents, issues, pull requests: write |
//! | worker | the same as the lead |
//! | verifier | metadata, actions, checks, contents: read; issues, pull requests, statuses: write |
//! | test run | no token (01M4BV70N7HRW7KQ9ER9D9CDT9) |
//!
//! - No role gets `administration`, `deployments`, `environments` or
//!   `workflows`. So no token approves a deploy, changes a ruleset or
//!   changes a workflow (01M4BV70ED4R56M31119BYJ93S).
//! - Only the verifier sets a commit status, and only the lead and the
//!   worker write the code. So no token can both set `riff/verify` and
//!   merge. The ruleset `releases` stops the push of a `v*` tag, and the
//!   ruleset `main` stops each push to `main` (01M4BV70GQ42MY0YHMRS47EK1E).
//! - GitHub keeps milestones and labels in `issues`, and a worker needs
//!   `issues: write` for its comments. So the lead and the worker get
//!   the same permissions. riff-server keeps the plan to the lead.
//!
//! ```
//! use riff::forge::{Access, permissions};
//! use riff::profile::Role;
//!
//! let worker = permissions(Role::Worker);
//! assert_eq!(worker["contents"], Access::Write);
//! assert!(!worker.contains_key("administration"));
//! assert!(!worker.contains_key("workflows"));
//! assert_eq!(permissions(Role::Verifier)["statuses"], Access::Write);
//! assert_eq!(permissions(Role::Verifier)["contents"], Access::Read);
//! assert!(permissions(Role::TestRun).is_empty());
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::profile::{Right, Role};

/// The variable that names the base URL of the GitHub API. A test sets
/// it to a fake API.
pub const API_VAR: &str = "RIFF_GITHUB_API";

/// The GitHub API when [`API_VAR`] is not set.
pub const GITHUB_API: &str = "https://api.github.com";

/// The variable that names the folder of the token files ([`Files`]).
pub const DIR_VAR: &str = "RIFF_FORGE_DIR";

/// The wrapper makes a new token this long before the old one ends.
pub const RENEW_BEFORE: Duration = Duration::from_secs(10 * 60);

/// The wrapper reads the claims of its session this often.
pub const LOOK_EVERY: Duration = Duration::from_secs(60);

/// The wrapper looks for a new ask file this often.
pub const ASK_EVERY: Duration = Duration::from_secs(1);

/// The user name that git sends with an installation token.
pub const TOKEN_USER: &str = "x-access-token";

/// A level of a GitHub permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    /// Read only.
    Read,
    /// Read and write.
    Write,
}

/// The GitHub permissions of one right of a role.
fn grants(right: Right) -> &'static [(&'static str, Access)] {
    use Access::{Read, Write};
    match right {
        Right::Read => &[
            ("metadata", Read),
            ("contents", Read),
            ("issues", Read),
            ("pull_requests", Read),
            ("checks", Read),
            ("statuses", Read),
            ("actions", Read),
        ],
        // GitHub keeps milestones and labels in `issues`.
        Right::Plan | Right::Comment => &[("issues", Write), ("pull_requests", Write)],
        Right::Push => &[("contents", Write)],
        Right::PullRequest => &[("pull_requests", Write)],
        Right::Verify => &[("statuses", Write)],
    }
}

/// The GitHub permissions of `role`: the highest level that one of its
/// rights gives for each permission. Empty for the test run.
pub fn permissions(role: Role) -> BTreeMap<&'static str, Access> {
    let mut all = BTreeMap::new();
    for right in role.rights() {
        for &(name, access) in grants(*right) {
            let level = all.entry(name).or_insert(access);
            *level = (*level).max(access);
        }
    }
    all
}

/// The role of the token of a worker session with `claims`: a
/// `verify-` claim gives the verifier, each other case the worker.
///
/// ```
/// use riff::forge::role_of;
/// use riff::profile::Role;
///
/// assert_eq!(role_of(&["verify-issue-12".into()]), Role::Verifier);
/// assert_eq!(role_of(&["issue-12".into()]), Role::Worker);
/// assert_eq!(role_of(&[]), Role::Worker);
/// ```
pub fn role_of(claims: &[String]) -> Role {
    if claims.iter().any(|c| c.starts_with("verify-")) {
        Role::Verifier
    } else {
        Role::Worker
    }
}

/// True when the wrapper must make a new token: it has none, its token
/// is of another role, or its token ends in less than [`RENEW_BEFORE`].
///
/// ```
/// use riff::forge::{RENEW_BEFORE, due};
/// use riff::profile::Role;
/// use std::time::{Duration, SystemTime};
///
/// let now = SystemTime::now();
/// let hour = now + Duration::from_secs(3600);
/// assert!(due(None, Role::Worker, now));
/// assert!(!due(Some((Role::Worker, hour)), Role::Worker, now));
/// assert!(due(Some((Role::Worker, hour)), Role::Verifier, now));
/// assert!(due(Some((Role::Worker, now + RENEW_BEFORE)), Role::Worker, now));
/// ```
pub fn due(held: Option<(Role, SystemTime)>, role: Role, now: SystemTime) -> bool {
    match held {
        None => true,
        Some((held, ends)) => held != role || ends <= now + RENEW_BEFORE,
    }
}

/// The GitHub App of riff: its ID and its private key.
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

impl App {
    /// The App with the ID `id` and the private key `pem` (PKCS#1 or
    /// PKCS#8, as GitHub gives it).
    pub fn new(id: u64, pem: &[u8]) -> Result<App> {
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(pem)
            .context("the key is not an RSA private key in PEM form")?;
        Ok(App { id, key })
    }

    /// The ID of the App.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The file of the private key, next to the settings file
    /// `settings`: `forge/app.pem`.
    ///
    /// ```
    /// use std::path::Path;
    /// let key = riff::forge::App::key_path(Path::new("/h/.config/riff/config.toml"));
    /// assert_eq!(key, Path::new("/h/.config/riff/forge/app.pem"));
    /// ```
    pub fn key_path(settings: &Path) -> PathBuf {
        settings
            .parent()
            .unwrap_or(Path::new("."))
            .join("forge")
            .join("app.pem")
    }

    /// The App of the settings file `settings`. `None` when it has no
    /// `forge.app`: then riff gives no forge token.
    pub fn here(settings: &Path) -> Result<Option<App>> {
        let Some(id) = crate::settings::forge_app(settings)? else {
            return Ok(None);
        };
        let path = Self::key_path(settings);
        let pem = std::fs::read(&path)
            .with_context(|| format!("cannot read the key of the App in {}", path.display()))?;
        App::new(id, &pem)
            .with_context(|| format!("{}", path.display()))
            .map(Some)
    }

    /// Saves the App: its key from `pem_file` to [`App::key_path`], with
    /// only the person as reader, and `id` as `forge.app` in `settings`.
    /// It checks the key first.
    pub fn save(settings: &Path, id: u64, pem_file: &Path) -> Result<PathBuf> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let pem = std::fs::read(pem_file)
            .with_context(|| format!("cannot read {}", pem_file.display()))?;
        App::new(id, &pem).with_context(|| format!("{}", pem_file.display()))?;
        let path = Self::key_path(settings);
        let dir = path.parent().unwrap_or(Path::new("."));
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .with_context(|| format!("cannot make {}", dir.display()))?;
        let fail = || format!("cannot write {}", path.display());
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .with_context(fail)?;
        // A file that exists keeps its mode at the open.
        file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))
            .with_context(fail)?;
        std::io::Write::write_all(&mut file, &pem).with_context(fail)?;
        crate::settings::set_forge_app(settings, id)?;
        Ok(path)
    }

    /// The JWT of the App at the Unix time `now`: it is good from one
    /// minute before `now` (for a clock that runs late) for ten minutes,
    /// the longest time that GitHub takes.
    pub fn jwt(&self, now: u64) -> Result<String> {
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
        jsonwebtoken::encode(&header, &claims, &self.key).context("cannot sign the JWT of the App")
    }
}

/// A token of a role for one repository.
#[derive(Clone, PartialEq, Eq)]
pub struct Token {
    /// The role that the token is for.
    pub role: Role,
    /// The token. Never print it.
    pub token: String,
    /// When the token ends.
    pub ends: SystemTime,
    /// The permissions that GitHub gave.
    pub permissions: BTreeMap<String, Access>,
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token")
            .field("role", &self.role)
            .field("ends", &self.ends)
            .field("permissions", &self.permissions)
            .finish_non_exhaustive()
    }
}

/// Checks the permissions that GitHub gave against the permissions of
/// `role`: each one that the role needs, and none more.
///
/// ```
/// use riff::forge::{Access, check_given, permissions};
/// use riff::profile::Role;
///
/// let given = |r| permissions(r).into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
/// assert!(check_given(Role::Worker, &given(Role::Worker)).is_ok());
/// // The lead has more rights than the worker.
/// assert!(check_given(Role::Verifier, &given(Role::Lead)).is_err());
/// // The App lacks a permission of the role.
/// assert!(check_given(Role::Verifier, &given(Role::Worker)).is_err());
/// ```
pub fn check_given(role: Role, given: &BTreeMap<String, Access>) -> Result<()> {
    let asked = permissions(role);
    let more: Vec<String> = given
        .iter()
        .filter(|(name, level)| asked.get(name.as_str()).is_none_or(|a| *level > a))
        .map(|(name, level)| format!("{name}: {level:?}"))
        .collect();
    if !more.is_empty() {
        bail!(
            "GitHub gave the {role} token rights that it did not ask for: {}",
            more.join(", ")
        );
    }
    let less: Vec<String> = asked
        .iter()
        .filter(|(name, level)| given.get(**name).is_none_or(|g| g < level))
        .map(|(name, level)| format!("{name}: {level:?}"))
        .collect();
    if !less.is_empty() {
        bail!(
            "the GitHub App does not have these permissions of the {role}: {}. Add them in the settings of the App",
            less.join(", ")
        );
    }
    Ok(())
}

/// The GitHub API.
#[derive(Debug, Clone)]
pub struct GitHub {
    base: String,
    http: reqwest::Client,
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
    expires_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    permissions: BTreeMap<String, Access>,
}

impl GitHub {
    /// The API at `base`, for example [`GITHUB_API`].
    pub fn new(base: &str) -> GitHub {
        GitHub {
            base: base.trim_end_matches('/').to_owned(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    /// The API of [`API_VAR`], else [`GITHUB_API`].
    pub fn here() -> GitHub {
        let base = std::env::var(API_VAR).ok().filter(|b| !b.is_empty());
        GitHub::new(base.as_deref().unwrap_or(GITHUB_API))
    }

    async fn reply(
        &self,
        request: reqwest::RequestBuilder,
        what: &str,
    ) -> Result<reqwest::Response> {
        request
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "riff")
            .send()
            .await
            .with_context(|| format!("cannot reach the GitHub API for {what}"))
    }

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        what: &str,
    ) -> Result<T> {
        let response = self.reply(request, what).await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("the GitHub API refused {what}: {status}: {}", body.trim());
        }
        response
            .json()
            .await
            .with_context(|| format!("the GitHub API gave no valid reply for {what}"))
    }

    /// The ID of the installation of `app` on `repo` (`OWNER/NAME`).
    pub async fn installation(&self, app: &App, repo: &str, now: u64) -> Result<u64> {
        let url = format!("{}/repos/{repo}/installation", self.base);
        let request = self.http.get(url).bearer_auth(app.jwt(now)?);
        let what = format!("the installation of the App on {repo}");
        Ok(self.send::<Installation>(request, &what).await?.id)
    }

    /// A token of `role` for `repo` (`OWNER/NAME`) only, with only the
    /// [`permissions`] of the role. It refuses the test run, and a
    /// token whose rights do not match the role ([`check_given`]).
    pub async fn token(&self, app: &App, repo: &str, role: Role) -> Result<Token> {
        let permissions = permissions(role);
        if permissions.is_empty() {
            bail!("the {role} gets no forge token");
        }
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let installation = self.installation(app, repo, now).await?;
        let name = repo.rsplit('/').next().unwrap_or(repo);
        let url = format!(
            "{}/app/installations/{installation}/access_tokens",
            self.base
        );
        let request = self.http.post(url).bearer_auth(app.jwt(now)?).json(&Ask {
            repositories: [name],
            permissions,
        });
        let given: Given = self
            .send(request, &format!("a {role} token for {repo}"))
            .await?;
        check_given(role, &given.permissions)?;
        Ok(Token {
            role,
            token: given.token,
            ends: given.expires_at.into(),
            permissions: given.permissions,
        })
    }

    /// Revokes `token`. A token that GitHub does not take any more
    /// (401) counts as revoked.
    pub async fn revoke(&self, token: &str) -> Result<()> {
        let url = format!("{}/installation/token", self.base);
        let what = "the revoke of a token";
        let response = self
            .reply(self.http.delete(url).bearer_auth(token), what)
            .await?;
        let status = response.status();
        if status.is_success() || status == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(());
        }
        let body = response.text().await.unwrap_or_default();
        bail!("the GitHub API refused {what}: {status}: {}", body.trim());
    }
}

/// The token files of one session, in the folder `forge` of its temp
/// folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Files {
    dir: PathBuf,
}

impl Files {
    /// The files in the temp folder `temp`.
    pub fn in_temp(temp: &Path) -> Files {
        Files {
            dir: temp.join("forge"),
        }
    }

    /// The files that [`DIR_VAR`] names, in a session that has them.
    pub fn here() -> Option<Files> {
        let dir = std::env::var_os(DIR_VAR).filter(|d| !d.is_empty())?;
        Some(Files {
            dir: PathBuf::from(dir),
        })
    }

    /// The folder of the files.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn token_path(&self) -> PathBuf {
        self.dir.join("token")
    }

    fn gh_dir(&self) -> PathBuf {
        self.dir.join("gh")
    }

    fn ask_path(&self) -> PathBuf {
        self.dir.join("ask")
    }

    /// Writes `token` for git and for `gh`. Each file is new and renamed
    /// into place, so a reader sees the old token or the new one. Only
    /// the person reads them.
    pub fn write(&self, token: &Token) -> Result<()> {
        use std::os::unix::fs::DirBuilderExt;
        let gh = self.gh_dir();
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&gh)
            .with_context(|| format!("cannot make {}", gh.display()))?;
        put(&self.token_path(), &token.token)?;
        let hosts = format!(
            "github.com:\n    oauth_token: {}\n    user: {TOKEN_USER}\n",
            token.token
        );
        put(&gh.join("hosts.yml"), &hosts)
    }

    /// Removes the token for git and for `gh`. A file that is not there
    /// is no fault.
    pub fn clear(&self) -> Result<()> {
        for path in [self.token_path(), self.gh_dir().join("hosts.yml")] {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(e).with_context(|| format!("cannot remove {}", path.display()));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The token that [`Files::write`] wrote last.
    pub fn token(&self) -> Result<String> {
        let path = self.token_path();
        let token = std::fs::read_to_string(&path)
            .with_context(|| format!("riff has no forge token in {}", path.display()))?;
        Ok(token.trim().to_owned())
    }

    /// Asks the wrapper to read the claims again: after a claim or a
    /// release. It never fails.
    pub fn ask(&self) {
        let _ = std::fs::write(self.ask_path(), b"");
    }

    /// The time of the last ask, if any.
    pub fn asked(&self) -> Option<SystemTime> {
        std::fs::metadata(self.ask_path()).ok()?.modified().ok()
    }

    /// The environment of `claude` for these files, with `riff` the
    /// path of the riff binary. `None` removes the variable. It removes
    /// each token variable of the person: `gh` reads them first.
    ///
    /// ```
    /// use std::path::Path;
    /// use riff::forge::Files;
    ///
    /// let env = Files::in_temp(Path::new("/t")).env(Path::new("/bin/riff"));
    /// let get = |k: &str| env.iter().find(|(v, _)| *v == k).map(|(_, v)| v.clone());
    /// assert_eq!(get("GH_CONFIG_DIR"), Some(Some("/t/forge/gh".into())));
    /// assert_eq!(get("GH_TOKEN"), Some(None));
    /// assert_eq!(get("GIT_CONFIG_VALUE_1"), Some(Some("!'/bin/riff' forge credential".into())));
    /// ```
    pub fn env(&self, riff: &Path) -> Vec<(&'static str, Option<String>)> {
        let helper = "credential.https://github.com.helper";
        let quoted = riff.to_string_lossy().replace('\'', r"'\''");
        vec![
            (DIR_VAR, Some(self.dir.to_string_lossy().into_owned())),
            (
                "GH_CONFIG_DIR",
                Some(self.gh_dir().to_string_lossy().into_owned()),
            ),
            ("GH_TOKEN", None),
            ("GITHUB_TOKEN", None),
            ("GH_ENTERPRISE_TOKEN", None),
            ("GITHUB_ENTERPRISE_TOKEN", None),
            // The empty helper drops each helper of the person for
            // github.com; then riff gives the token.
            ("GIT_CONFIG_COUNT", Some("2".into())),
            ("GIT_CONFIG_KEY_0", Some(helper.into())),
            ("GIT_CONFIG_VALUE_0", Some(String::new())),
            ("GIT_CONFIG_KEY_1", Some(helper.into())),
            (
                "GIT_CONFIG_VALUE_1",
                Some(format!("!'{quoted}' forge credential")),
            ),
        ]
    }
}

/// Writes `text` to `path` with only the person as reader: a new file,
/// renamed into place.
fn put(path: &Path, text: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = path.parent().unwrap_or(Path::new("."));
    let fail = || format!("cannot write {}", path.display());
    let mut new = tempfile::NamedTempFile::new_in(dir).with_context(fail)?;
    new.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))
        .with_context(fail)?;
    std::io::Write::write_all(&mut new, text.as_bytes()).with_context(fail)?;
    new.persist(path).map_err(|e| e.error).with_context(fail)?;
    Ok(())
}

/// The answer of `riff forge credential OPERATION` to git, for the
/// request `input` and the token of `files`. Only `get` for
/// `https://github.com` gets the token; each other request gets
/// nothing, so git asks its next helper.
///
/// ```
/// use riff::forge::credential;
///
/// let token = || Ok("t1".to_owned());
/// let github = "protocol=https\nhost=github.com\n";
/// assert_eq!(credential("get", github, token).unwrap(), "username=x-access-token\npassword=t1\n");
/// assert_eq!(credential("store", github, token).unwrap(), "");
/// assert_eq!(credential("get", "protocol=https\nhost=example.com\n", token).unwrap(), "");
/// ```
pub fn credential(
    operation: &str,
    input: &str,
    token: impl FnOnce() -> Result<String>,
) -> Result<String> {
    let field = |key: &str| {
        input
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
    };
    let github = field("host") == Some("github.com") && field("protocol") == Some("https");
    if operation != "get" || !github {
        return Ok(String::new());
    }
    Ok(format!("username={TOKEN_USER}\npassword={}\n", token()?))
}

/// What the wrapper needs to keep the token of its session.
pub struct Keeper {
    /// The App.
    pub app: App,
    /// The GitHub API.
    pub github: GitHub,
    /// The repository of the session, `OWNER/NAME`.
    pub repo: String,
    /// The token files of the session.
    pub files: Files,
    held: Option<Token>,
}

impl Keeper {
    /// A keeper with no token yet.
    pub fn new(app: App, github: GitHub, repo: String, files: Files) -> Keeper {
        Keeper {
            app,
            github,
            repo,
            files,
            held: None,
        }
    }

    /// Makes a new token when it is [`due`] for the role of `claims`
    /// (`None` when riff-server did not answer: then the role stays).
    /// Returns the new token, if any.
    ///
    /// At a change of role, it removes the token files and revokes the
    /// old token first, so the session never holds the rights of two
    /// roles. When the revoke fails, the session has no token, and the
    /// next step tries again.
    pub async fn step(&mut self, claims: Option<&[String]>) -> Result<Option<Token>> {
        let role = match (claims, &self.held) {
            (Some(claims), _) => role_of(claims),
            (None, Some(held)) => held.role,
            (None, None) => Role::Worker,
        };
        let held = self.held.as_ref().map(|t| (t.role, t.ends));
        if !due(held, role, SystemTime::now()) {
            return Ok(None);
        }
        if let Some(old) = self.held.as_ref().filter(|old| old.role != role) {
            self.files.clear()?;
            self.github.revoke(&old.token).await?;
            self.held = None;
        }
        let token = self.github.token(&self.app, &self.repo, role).await?;
        self.files.write(&token)?;
        self.held = Some(token.clone());
        Ok(Some(token))
    }
}

/// Keeps the token of a worker session: a [`Keeper::step`] each
/// [`LOOK_EVERY`], and at once after an ask ([`Files::ask`]).
/// `claims` reads the claims of the session from riff-server. It runs
/// until the wrapper stops it, and prints each fault.
pub async fn keep<F, Fut>(mut keeper: Keeper, mut claims: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<Vec<String>>>,
{
    let mut asked = keeper.files.asked();
    loop {
        let now = claims().await;
        if let Err(e) = keeper.step(now.as_deref()).await {
            eprintln!("{}", crate::text::forge_no_token(&format!("{e:#}")));
        }
        let mut waited = Duration::ZERO;
        while waited < LOOK_EVERY {
            tokio::time::sleep(ASK_EVERY).await;
            waited += ASK_EVERY;
            let new = keeper.files.asked();
            if new != asked {
                asked = new;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_role_gets_the_rights_of_an_admin() {
        for role in Role::ALL {
            let p = permissions(role);
            for name in [
                "administration",
                "deployments",
                "environments",
                "workflows",
                "secrets",
            ] {
                assert!(!p.contains_key(name), "{role} has {name}");
            }
        }
    }

    #[test]
    fn only_the_verifier_sets_a_status_and_it_writes_no_code() {
        for role in [Role::Lead, Role::Worker] {
            assert_eq!(permissions(role)["statuses"], Access::Read, "{role}");
            assert_eq!(permissions(role)["contents"], Access::Write, "{role}");
        }
        let verifier = permissions(Role::Verifier);
        assert_eq!(verifier["statuses"], Access::Write);
        assert_eq!(verifier["contents"], Access::Read);
    }

    #[test]
    fn the_token_files_give_the_token_to_gh_and_to_git() {
        let dir = tempfile::tempdir().unwrap();
        let files = Files::in_temp(dir.path());
        assert!(files.token().is_err());
        let token = Token {
            role: Role::Worker,
            token: "ghs_one".into(),
            ends: SystemTime::now(),
            permissions: BTreeMap::new(),
        };
        files.write(&token).unwrap();
        assert_eq!(files.token().unwrap(), "ghs_one");
        let hosts = std::fs::read_to_string(dir.path().join("forge/gh/hosts.yml")).unwrap();
        assert!(hosts.contains("oauth_token: ghs_one"), "{hosts}");
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join("forge/token"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "only the person reads the token");
        assert!(
            !format!("{token:?}").contains("ghs_one"),
            "Debug hides the token"
        );
    }

    #[test]
    fn the_saved_key_is_for_the_person_only_also_over_an_old_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.toml");
        let pem = dir.path().join("app.pem");
        std::fs::write(
            &pem,
            include_str!("../../riff-server/testdata/test-only-rsa-key.pem"),
        )
        .unwrap();
        let key = App::key_path(&settings);
        std::fs::create_dir_all(key.parent().unwrap()).unwrap();
        std::fs::write(&key, "old").unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(App::save(&settings, 7, &pem).unwrap(), key);
        let mode = std::fs::metadata(&key).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(App::here(&settings).unwrap().unwrap().id(), 7);
    }

    #[test]
    fn an_ask_changes_the_ask_time() {
        let dir = tempfile::tempdir().unwrap();
        let files = Files::in_temp(dir.path());
        std::fs::create_dir_all(files.dir()).unwrap();
        assert_eq!(files.asked(), None);
        files.ask();
        assert!(files.asked().is_some());
    }
}
