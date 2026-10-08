//! The forge token of a role: riff-server gives each session a token
//! with only the rights of its role (#610, #628).
//!
//! # Design
//!
//! riff-server holds the GitHub App of riff and makes each token
//! (`riff_server::forge`). The private key of the App is never on a
//! machine (01M4CTAYRSC27Q6AAGTH9CBD7Q).
//!
//! `riff workers run` stays outside the sandbox of `claude`. It asks
//! riff-server for the token of its session ([`Keeper`]): `POST
//! /v1/forge/token` with the URI of the session, signed in as the
//! person. The server picks the role and the repository from its own
//! facts, so a session cannot ask for more rights
//! (01M4CNN37FYYB99BS6QV2FFWZ8). The server gives a token only to a
//! session that it knows, so the wrapper of a worker registers its
//! session before its first ask. The wrapper of the lead asks with the
//! URI of the person, before the session of the lead starts: the server
//! gives it the lead token while the person has a lead in the
//! repository. Until then, the wrapper asks each [`LOOK_EVERY`], and
//! `riff mcp` asks it at once when its session registers
//! ([`Files::ask`]).
//!
//! A token ends after one hour. A process cannot change the
//! environment of `claude` after its start. So the token is in files of
//! the temp folder of the session ([`Files`]), and the wrapper writes
//! them again ([`keep`]):
//!
//! - `gh` reads the token from `GH_CONFIG_DIR/hosts.yml`.
//! - git reads it through the credential helper `riff forge
//!   credential`, which [`ForgeEnv`] sets for the github.com URLs.
//! - [`ForgeEnv`] is the only way to make the command of `claude`. It
//!   starts from an empty environment with only the
//!   [`crate::profile::KEPT_VARS`], so no credential of the person
//!   reaches the session, also with no token
//!   (01M4BV709WGHZ57AM3STC15B69, 01M4BYVSNQ5SY2GRGT73FV0Z3E). Each
//!   cause of no token is an [`Error`].
//!
//! The wrapper reads the claims of its session from riff-server each
//! [`LOOK_EVERY`], and at once when `riff mcp` asks after a claim or a
//! release ([`Files::ask`]). It asks for a new token when the role of
//! the claims changes, or [`RENEW_BEFORE`] before the token ends
//! ([`due`], 01M4BV70JZNMT3X99E77GC58K9). At a change of role, it
//! removes the old token files first, and riff-server revokes the old
//! token (01M4CNN39TTK36GX34RCWKES80). At a renew, riff-server revokes
//! the old token after it makes the new one.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant W as riff workers run (outside the sandbox)
//!     participant S as riff-server
//!     participant C as claude, gh, git (in the sandbox)
//!     W->>S: POST /v1/forge/token: the URI of the session
//!     S-->>W: token of the role, ends in 1 hour
//!     W->>C: token files in the temp folder
//!     C->>C: gh reads hosts.yml, git runs riff forge credential
//!     C->>W: riff mcp claim: the ask file
//!     W->>S: GET /v1/me: the claims, then a token of the new role
//! ```
//!
//! The table of rights is in [`riff_core::forge`]. [`permissions`]
//! gives the rights of a role of a [`Role`] of the profile:
//!
//! ```
//! use riff::forge::{Access, permissions};
//! use riff::profile::Role;
//!
//! let worker = permissions(Role::Worker);
//! assert_eq!(worker["contents"], Access::Write);
//! assert!(!worker.contains_key("administration"));
//! assert_eq!(permissions(Role::Verifier)["statuses"], Access::Write);
//! assert!(permissions(Role::TestRun).is_empty());
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use futures::future::BoxFuture;
pub use riff_core::forge::{Access, TokenRole};
use riff_core::wire::ForgeTokenReply;

use crate::profile::Role;

/// The variable that names the folder of the token files ([`Files`]).
pub const DIR_VAR: &str = "RIFF_FORGE_DIR";

/// The wrapper asks for a new token this long before the old one ends.
pub const RENEW_BEFORE: Duration = Duration::from_secs(10 * 60);

/// The wrapper reads the claims of its session this often.
pub const LOOK_EVERY: Duration = Duration::from_secs(60);

/// The wrapper looks for a new ask file this often.
pub const ASK_EVERY: Duration = Duration::from_secs(1);

/// The user name that git sends with an installation token.
pub const TOKEN_USER: &str = "x-access-token";

/// The role of the token of a [`Role`] of the profile. The test run
/// gets no token (01M4BV70N7HRW7KQ9ER9D9CDT9).
///
/// ```
/// use riff::forge::{TokenRole, token_role};
/// use riff::profile::Role;
///
/// assert_eq!(token_role(Role::Lead), Some(TokenRole::Lead));
/// assert_eq!(token_role(Role::TestRun), None);
/// ```
pub fn token_role(role: Role) -> Option<TokenRole> {
    match role {
        Role::Lead => Some(TokenRole::Lead),
        Role::Worker => Some(TokenRole::Worker),
        Role::Verifier => Some(TokenRole::Verifier),
        Role::TestRun => None,
    }
}

/// The GitHub permissions of `role`. Empty for the test run.
pub fn permissions(role: Role) -> BTreeMap<&'static str, Access> {
    token_role(role).map_or_else(BTreeMap::new, riff_core::forge::permissions)
}

/// The role of the token of a worker session with `claims`, as the
/// server picks it: a `verify-` claim gives the verifier, each other
/// case the worker.
///
/// ```
/// use riff::forge::{TokenRole, role_of};
///
/// assert_eq!(role_of(&["verify-issue-12".into()]), TokenRole::Verifier);
/// assert_eq!(role_of(&["issue-12".into()]), TokenRole::Worker);
/// assert_eq!(role_of(&[]), TokenRole::Worker);
/// ```
pub fn role_of(claims: &[String]) -> TokenRole {
    riff_core::forge::role_of(false, claims)
}

/// True when the wrapper must ask for a new token: it has none, its
/// token is of another role, or its token ends in less than
/// [`RENEW_BEFORE`].
///
/// ```
/// use riff::forge::{RENEW_BEFORE, TokenRole, due};
/// use std::time::{Duration, SystemTime};
///
/// let now = SystemTime::now();
/// let hour = now + Duration::from_secs(3600);
/// assert!(due(None, TokenRole::Worker, now));
/// assert!(!due(Some((TokenRole::Worker, hour)), TokenRole::Worker, now));
/// assert!(due(Some((TokenRole::Worker, hour)), TokenRole::Verifier, now));
/// assert!(due(Some((TokenRole::Worker, now + RENEW_BEFORE)), TokenRole::Worker, now));
/// ```
pub fn due(held: Option<(TokenRole, SystemTime)>, role: TokenRole, now: SystemTime) -> bool {
    match held {
        None => true,
        Some((held, ends)) => held != role || ends <= now + RENEW_BEFORE,
    }
}

/// Why a session has no forge token. Each cause gives the same
/// [`ForgeEnv`] as a token, with no credential of the person
/// (01M4BYVSNQ5SY2GRGT73FV0Z3E).
#[derive(Debug)]
pub enum Error {
    /// The session has no temp folder for the token files.
    NoFolder,
    /// The session has no ID, or its folder is in no repository.
    Place(anyhow::Error),
    /// riff-server gave no token: for example no sign-in, no App, or
    /// no installation of the App on the repository.
    Server(anyhow::Error),
    /// riff cannot write the token files.
    Files(anyhow::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoFolder => f.write_str("the session has no temp folder"),
            Error::Place(e) | Error::Server(e) | Error::Files(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for Error {}

/// A token of a role for one repository.
#[derive(Clone, PartialEq, Eq)]
pub struct Token {
    /// The role that the server picked.
    pub role: TokenRole,
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

impl From<ForgeTokenReply> for Token {
    fn from(reply: ForgeTokenReply) -> Token {
        Token {
            role: reply.role,
            token: reply.token,
            ends: UNIX_EPOCH + Duration::from_millis(reply.ends_ms),
            permissions: reply.permissions,
        }
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

    /// The forge variables of these files, with `riff` the path of the
    /// riff binary ([`ForgeEnv`]).
    fn vars(&self, riff: &Path) -> Vec<(&'static str, String)> {
        let helper = "credential.https://github.com.helper";
        let quoted = riff.to_string_lossy().replace('\'', r"'\''");
        vec![
            (DIR_VAR, self.dir.to_string_lossy().into_owned()),
            (
                "GH_CONFIG_DIR",
                self.gh_dir().to_string_lossy().into_owned(),
            ),
            // An empty helper drops each helper of the person before it;
            // then riff gives the token for github.com.
            ("GIT_CONFIG_COUNT", "3".into()),
            ("GIT_CONFIG_KEY_0", "credential.helper".into()),
            ("GIT_CONFIG_VALUE_0", String::new()),
            ("GIT_CONFIG_KEY_1", helper.into()),
            ("GIT_CONFIG_VALUE_1", String::new()),
            ("GIT_CONFIG_KEY_2", helper.into()),
            (
                "GIT_CONFIG_VALUE_2",
                format!("!'{quoted}' forge credential"),
            ),
        ]
    }
}

/// The environment of `claude` in a session: the only way to make its
/// command ([`ForgeEnv::command`]). The command starts with an empty
/// environment. It gets only the [`crate::profile::KEPT_VARS`] of the
/// parent and the forge variables, so no credential of the person
/// reaches it, with a token and with no token
/// (01M4BYVSNQ5SY2GRGT73FV0Z3E).
///
/// ```
/// use std::path::Path;
/// use riff::forge::{Error, Files, ForgeEnv};
///
/// let files = Files::in_temp(Path::new("/t"));
/// let env = ForgeEnv::of(&Err(Error::NoFolder), &files, Path::new("/bin/riff"));
/// assert_eq!(env.get("GH_CONFIG_DIR"), Some("/t/forge/gh"));
/// assert_eq!(env.get("GIT_CONFIG_VALUE_2"), Some("!'/bin/riff' forge credential"));
/// assert_eq!(env.no_token(), Some("the session has no temp folder"));
/// let parent = [("PATH".into(), "/bin".into()), ("GH_TOKEN".into(), "ghp_person".into())];
/// let cmd = env.command("claude".as_ref(), &[], parent);
/// let set: Vec<String> =
///     cmd.as_std().get_envs().map(|(k, _)| k.to_string_lossy().into_owned()).collect();
/// assert!(set.contains(&"PATH".to_owned()));
/// assert!(!set.contains(&"GH_TOKEN".to_owned()));
/// ```
#[derive(Debug)]
pub struct ForgeEnv {
    vars: Vec<(&'static str, String)>,
    no_token: Option<String>,
}

impl ForgeEnv {
    /// The environment for `given`, the first token step of the
    /// session, and its token `files`. Each outcome gives the same
    /// variables: with no token, the files hold none.
    pub fn of(given: &std::result::Result<Token, Error>, files: &Files, riff: &Path) -> ForgeEnv {
        ForgeEnv {
            vars: files.vars(riff),
            no_token: given.as_ref().err().map(ToString::to_string),
        }
    }

    /// The value of the forge variable `name`.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars
            .iter()
            .find(|(v, _)| *v == name)
            .map(|(_, value)| value.as_str())
    }

    /// Why the session has no token, if it has none.
    pub fn no_token(&self) -> Option<&str> {
        self.no_token.as_deref()
    }

    /// The command `program` with `args`: an empty environment, then
    /// each variable of `parent` that [`crate::profile::kept`] keeps,
    /// then the forge variables.
    pub fn command(
        &self,
        program: &std::ffi::OsStr,
        args: &[std::ffi::OsString],
        parent: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
    ) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args).env_clear();
        for (name, value) in parent {
            if name.to_str().is_some_and(crate::profile::kept) {
                cmd.env(name, value);
            }
        }
        for (name, value) in &self.vars {
            cmd.env(name, value);
        }
        cmd
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

/// The call that asks riff-server for the token of the session: `POST
/// /v1/forge/token` ([`crate::api::Api::forge_token`]). A test gives a
/// fake.
pub type Ask = Box<dyn Fn() -> BoxFuture<'static, Result<ForgeTokenReply>> + Send + Sync>;

/// What the wrapper needs to keep the token of its session.
pub struct Keeper {
    /// The token files of the session.
    pub files: Files,
    ask: Ask,
    held: Option<Token>,
    fixed: Option<TokenRole>,
}

impl Keeper {
    /// A keeper with no token yet. The role follows the claims. `ask`
    /// asks riff-server for a token.
    pub fn new(files: Files, ask: Ask) -> Keeper {
        Keeper {
            files,
            ask,
            held: None,
            fixed: None,
        }
    }

    /// The same keeper, with the one role `role` whatever the claims:
    /// the lead (01M4C4WQVZR49FDGPJMFW22GTM).
    pub fn with_role(self, role: TokenRole) -> Keeper {
        Keeper {
            fixed: Some(role),
            ..self
        }
    }

    /// Asks for a new token when it is [`due`] for the role of `claims`
    /// (`None` when riff-server did not answer: then the role stays).
    /// Returns the new token, if any.
    ///
    /// At a change of role, it removes the old token files first, so
    /// the session never holds the rights of two roles. riff-server
    /// revokes the old token.
    pub async fn step(&mut self, claims: Option<&[String]>) -> Result<Option<Token>, Error> {
        let role = self.role(claims);
        let held = self.held.as_ref().map(|t| (t.role, t.ends));
        if !due(held, role, SystemTime::now()) {
            return Ok(None);
        }
        self.make(role).await.map(Some)
    }

    /// The first token of the session, for the role of `claims`.
    pub async fn first(&mut self, claims: Option<&[String]>) -> Result<Token, Error> {
        let role = self.role(claims);
        self.make(role).await
    }

    fn role(&self, claims: Option<&[String]>) -> TokenRole {
        if let Some(role) = self.fixed {
            return role;
        }
        match (claims, &self.held) {
            (Some(claims), _) => role_of(claims),
            (None, Some(held)) => held.role,
            (None, None) => TokenRole::Worker,
        }
    }

    async fn make(&mut self, role: TokenRole) -> Result<Token, Error> {
        if self.held.as_ref().is_some_and(|old| old.role != role) {
            self.files.clear().map_err(Error::Files)?;
            self.held = None;
        }
        let token = Token::from((self.ask)().await.map_err(Error::Server)?);
        self.files.write(&token).map_err(Error::Files)?;
        self.held = Some(token.clone());
        Ok(token)
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
            eprintln!("{}", crate::text::forge_no_token(&e.to_string()));
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
    use crate::profile::Right;

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

    /// The table of riff-core gives each role the highest level that
    /// one of its rights of the profile gives.
    #[test]
    fn the_table_of_the_server_matches_the_rights_of_each_role() {
        for role in Role::ALL {
            let mut all = BTreeMap::new();
            for right in role.rights() {
                for &(name, access) in grants(*right) {
                    let level = all.entry(name).or_insert(access);
                    *level = (*level).max(access);
                }
            }
            assert_eq!(permissions(role), all, "{role}");
        }
    }

    #[test]
    fn the_token_files_give_the_token_to_gh_and_to_git() {
        let dir = tempfile::tempdir().unwrap();
        let files = Files::in_temp(dir.path());
        assert!(files.token().is_err());
        let token = Token {
            role: TokenRole::Worker,
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
    fn an_ask_changes_the_ask_time() {
        let dir = tempfile::tempdir().unwrap();
        let files = Files::in_temp(dir.path());
        std::fs::create_dir_all(files.dir()).unwrap();
        assert_eq!(files.asked(), None);
        files.ask();
        assert!(files.asked().is_some());
    }
}
