//! Where riff-server keeps the keys of the GitHub Apps of the riff
//! (01M6WG52WSWM8A9CN6X448AGAJ).
//!
//! # Design
//!
//! The keys are the ID, the slug, the private key and the webhook secret
//! of each App ([`AppKeys`]). GitHub gives them to riff-server only, in
//! the swap of [`crate::github`]. They never go into the log, a record,
//! the bucket, a reply or a log line. Their `Debug` hides the key and the
//! secret.
//!
//! | [`At`] | When | Read | Write |
//! |---|---|---|---|
//! | [`At::Memory`] | a riff-server with no store, and the tests | memory | memory |
//! | [`At::Dir`] | `--dir` | the file [`FILE`] | a new file with mode `0600`, then a rename |
//! | [`At::Secret`] | `RIFF_GITHUB_SECRET` on Cloud Run | `GET …/versions/latest:access` | `POST …:addVersion` |
//! | [`At::Nowhere`] | `--bucket` with no secret | nothing | refused |
//!
//! [`Keys`] reads the place one time, and keeps the Apps in memory: one
//! riff-server serves at a time (the lease), and it is the only writer.
//! Secret Manager takes the access token of the account of the server
//! from the metadata server of Cloud Run, as [`crate::gcs`] does.
//!
//! ```mermaid
//! flowchart LR
//!     S[the swap of a code] --> K[Keys::put]
//!     K --> M[memory]
//!     K --> F["DIR/github-apps.json, 0600"]
//!     K --> V["Secret Manager: a new version"]
//! ```
//!
//! # Example
//!
//! ```
//! # #[tokio::main] async fn main() {
//! use riff_core::wire::GitHubApp;
//! use riff_server::keys::{AppKeys, At, Keys};
//!
//! let dir = tempfile::tempdir().unwrap();
//! let keys = Keys::new(At::Dir(dir.path().to_owned()), reqwest::Client::new());
//! let (key, secret) = (isolated::fake_secret(), isolated::fake_secret());
//! let app = AppKeys::new(7, "riff-acme", "acme", &key, Some(&secret));
//! assert!(!format!("{app:?}").contains(&key));
//! keys.put(GitHubApp::Session, app).await.unwrap();
//!
//! let again = Keys::new(At::Dir(dir.path().to_owned()), reqwest::Client::new());
//! let apps = again.load().await.unwrap();
//! assert_eq!(apps.of(GitHubApp::Session).unwrap().slug, "riff-acme");
//! assert!(apps.of(GitHubApp::Lead).is_none());
//! # }
//! ```

use std::io::Write as _;
use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use riff_core::wire::GitHubApp;
use serde::{Deserialize, Serialize};

/// The file of the keys in `--dir`.
pub const FILE: &str = "github-apps.json";

/// The Secret Manager API.
pub const SECRET_MANAGER: &str = "https://secretmanager.googleapis.com";

/// Where riff-server keeps the keys of the Apps. See the module docs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum At {
    /// In memory only: a restart loses them.
    Memory,
    /// In the file [`FILE`] of this folder.
    Dir(PathBuf),
    /// In a secret of Secret Manager.
    Secret {
        /// The secret, `projects/PROJECT/secrets/NAME`.
        name: String,
        /// The Secret Manager API, [`SECRET_MANAGER`] but in a test.
        api: String,
        /// The metadata server, [`crate::gcs::METADATA`] but in a test.
        metadata: String,
    },
    /// Nowhere: a riff-server with a bucket and no secret.
    Nowhere,
}

impl At {
    /// The secret `name` of Secret Manager, for a riff-server on Cloud
    /// Run.
    ///
    /// ```
    /// use riff_server::keys::At;
    ///
    /// let At::Secret { name, .. } = At::secret("projects/p/secrets/s") else { panic!() };
    /// assert_eq!(name, "projects/p/secrets/s");
    /// ```
    pub fn secret(name: &str) -> At {
        At::Secret {
            name: name.to_owned(),
            api: SECRET_MANAGER.to_owned(),
            metadata: crate::gcs::METADATA.to_owned(),
        }
    }

    /// Where the keys are, for a line of the log.
    pub fn describe(&self) -> String {
        match self {
            At::Memory => "memory".to_owned(),
            At::Dir(dir) => dir.join(FILE).display().to_string(),
            At::Secret { name, .. } => name.clone(),
            At::Nowhere => "nowhere".to_owned(),
        }
    }
}

/// The keys of one App. GitHub gives them in the swap of the code.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppKeys {
    /// The App ID.
    pub id: u64,
    /// The slug: the name of the App in each URL.
    pub slug: String,
    /// The account that owns the App.
    pub owner: String,
    /// The private key, as PEM.
    pub key: String,
    /// The secret of the webhook, for an App with a webhook.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook_secret: Option<String>,
    /// The user ID of the bot of the App, `SLUG[bot]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<u64>,
}

impl AppKeys {
    /// The keys of the App `id` with the slug `slug` on `owner`.
    pub fn new(id: u64, slug: &str, owner: &str, key: &str, webhook_secret: Option<&str>) -> Self {
        AppKeys {
            id,
            slug: slug.to_owned(),
            owner: owner.to_owned(),
            key: key.to_owned(),
            webhook_secret: webhook_secret.map(str::to_owned),
            bot_id: None,
        }
    }

    /// The login of the bot of the App.
    ///
    /// ```
    /// use riff_server::keys::AppKeys;
    ///
    /// let app = AppKeys::new(7, "riff-acme", "acme", &isolated::fake_secret(), None);
    /// assert_eq!(app.bot(), "riff-acme[bot]");
    /// ```
    pub fn bot(&self) -> String {
        format!("{}[bot]", self.slug)
    }
}

/// The `Debug` of the keys hides the key and the secret.
impl std::fmt::Debug for AppKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppKeys")
            .field("id", &self.id)
            .field("slug", &self.slug)
            .field("owner", &self.owner)
            .field("bot_id", &self.bot_id)
            .finish_non_exhaustive()
    }
}

/// The keys of the two Apps of the riff.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Apps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<AppKeys>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<AppKeys>,
}

impl Apps {
    /// The keys of the App `app`, when the riff has it.
    pub fn of(&self, app: GitHubApp) -> Option<&AppKeys> {
        match app {
            GitHubApp::Session => self.session.as_ref(),
            GitHubApp::Lead => self.lead.as_ref(),
        }
    }

    fn set(&mut self, app: GitHubApp, keys: AppKeys) {
        match app {
            GitHubApp::Session => self.session = Some(keys),
            GitHubApp::Lead => self.lead = Some(keys),
        }
    }
}

/// Why the keys did not read or did not write. The text names the
/// place, never a key.
///
/// ```
/// use riff_server::keys::KeysError;
///
/// let fault = KeysError::new("projects/p/secrets/s: 503 Service Unavailable");
/// assert!(!fault.denied);
/// assert!(KeysError::denied("projects/p/secrets/s: 403 Forbidden").denied);
/// assert_eq!(fault.to_string(), "projects/p/secrets/s: 503 Service Unavailable");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeysError {
    /// Why, with the place.
    pub text: String,
    /// True when the place refuses the read: Secret Manager answers 401
    /// or 403, or the file has no read right for the account of the
    /// server. A new try does not help: an admin fixes the secret, and
    /// riff-server pauses the riff until then (01M6YC3SYATQW7GZZV3APFG85B).
    pub denied: bool,
}

impl KeysError {
    /// A fault that a new try can end, for example a 503.
    pub fn new(text: impl Into<String>) -> Self {
        KeysError {
            text: text.into(),
            denied: false,
        }
    }

    /// A read that the place refuses.
    pub fn denied(text: impl Into<String>) -> Self {
        KeysError {
            text: text.into(),
            denied: true,
        }
    }
}

impl std::fmt::Display for KeysError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

impl std::error::Error for KeysError {}

/// The keys of the Apps at one place. See the module docs.
pub struct Keys {
    at: At,
    http: reqwest::Client,
    /// The Apps, after the first read. The lock holds a read and a write
    /// together.
    kept: tokio::sync::Mutex<Option<Apps>>,
}

impl Keys {
    /// The keys at `at`. It reads nothing yet.
    pub fn new(at: At, http: reqwest::Client) -> Self {
        Keys {
            at,
            http,
            kept: tokio::sync::Mutex::new(None),
        }
    }

    /// Where the keys are.
    pub fn at(&self) -> &At {
        &self.at
    }

    /// The Apps of the riff: none before the first swap.
    pub async fn load(&self) -> Result<Apps, KeysError> {
        let mut kept = self.kept.lock().await;
        if kept.is_none() {
            *kept = Some(self.read().await?);
        }
        Ok(kept.clone().unwrap_or_default())
    }

    /// Keeps `keys` as the App `app`, in memory and at the place.
    pub async fn put(&self, app: GitHubApp, keys: AppKeys) -> Result<(), KeysError> {
        let mut kept = self.kept.lock().await;
        let mut apps = match kept.take() {
            Some(apps) => apps,
            None => self.read().await?,
        };
        let old = apps.clone();
        apps.set(app, keys);
        let written = self.write(&apps).await;
        // A write that failed keeps the Apps as they were: the place
        // and the memory agree.
        *kept = Some(if written.is_ok() { apps } else { old });
        written
    }

    async fn read(&self) -> Result<Apps, KeysError> {
        let bytes = match &self.at {
            At::Memory | At::Nowhere => return Ok(Apps::default()),
            At::Dir(dir) => match std::fs::read(dir.join(FILE)) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Apps::default());
                }
                Err(e) => {
                    let text = format!("{}: {e}", self.at.describe());
                    return Err(if e.kind() == std::io::ErrorKind::PermissionDenied {
                        KeysError::denied(text)
                    } else {
                        KeysError::new(text)
                    });
                }
            },
            At::Secret {
                name,
                api,
                metadata,
            } => match self.access(name, api, metadata).await? {
                Some(bytes) => bytes,
                None => return Ok(Apps::default()),
            },
        };
        serde_json::from_slice(&bytes)
            .map_err(|e| KeysError::new(format!("{} does not read: {e}", self.at.describe())))
    }

    async fn write(&self, apps: &Apps) -> Result<(), KeysError> {
        let bytes = serde_json::to_vec(apps).expect("the keys are JSON");
        match &self.at {
            At::Memory => Ok(()),
            At::Nowhere => Err(KeysError::new(NOWHERE)),
            At::Dir(dir) => write_private(dir, &bytes)
                .map_err(|e| KeysError::new(format!("{}: {e}", self.at.describe()))),
            At::Secret {
                name,
                api,
                metadata,
            } => self.add_version(name, api, metadata, &bytes).await,
        }
    }

    /// The data of the newest version of the secret `name`, or `None`
    /// when it has no version.
    async fn access(
        &self,
        name: &str,
        api: &str,
        metadata: &str,
    ) -> Result<Option<Vec<u8>>, KeysError> {
        #[derive(Deserialize)]
        struct Payload {
            data: String,
        }
        #[derive(Deserialize)]
        struct Version {
            payload: Payload,
        }
        let url = format!("{api}/v1/{name}/versions/latest:access");
        let reply = self
            .http
            .get(&url)
            .bearer_auth(self.token(metadata).await?)
            .send()
            .await
            .map_err(|e| KeysError::new(format!("{name}: {e}")))?;
        if reply.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let reply = failed_status(name, reply).await?;
        let version: Version = reply
            .json()
            .await
            .map_err(|e| KeysError::new(format!("{name}: {e}")))?;
        let data = STANDARD
            .decode(version.payload.data)
            .map_err(|e| KeysError::new(format!("{name}: the payload is no base64: {e}")))?;
        Ok(Some(data))
    }

    /// Adds `bytes` as a new version of the secret `name`.
    async fn add_version(
        &self,
        name: &str,
        api: &str,
        metadata: &str,
        bytes: &[u8],
    ) -> Result<(), KeysError> {
        let url = format!("{api}/v1/{name}:addVersion");
        let body = serde_json::json!({ "payload": { "data": STANDARD.encode(bytes) } });
        let reply = self
            .http
            .post(&url)
            .bearer_auth(self.token(metadata).await?)
            .json(&body)
            .send()
            .await
            .map_err(|e| KeysError::new(format!("{name}: {e}")))?;
        failed_status(name, reply).await.map(|_| ())
    }

    /// The access token of the account of the server, from the metadata
    /// server.
    async fn token(&self, metadata: &str) -> Result<String, KeysError> {
        #[derive(Deserialize)]
        struct Token {
            access_token: String,
        }
        let url = format!("{metadata}{}", crate::gcs::TOKEN_PATH);
        let failed =
            |e: reqwest::Error| KeysError::new(format!("no access token for the secret: {e}"));
        let reply = self
            .http
            .get(&url)
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .map_err(failed)?
            .error_for_status()
            .map_err(failed)?;
        Ok(reply.json::<Token>().await.map_err(failed)?.access_token)
    }
}

/// The refusal of a write with no place, and of `riff github create`
/// on a riff-server with a bucket and no secret.
pub const NOWHERE: &str = "this riff-server has no place for the keys of the GitHub Apps: \
                       riff cloud create NAME makes the secret, and riff cloud deploy NAME \
                       names it in RIFF_GITHUB_SECRET";

/// The reply, or an error with its status and its text. The text of
/// Secret Manager names no secret value. A 401 or a 403 is a read that
/// Secret Manager refuses ([`KeysError::denied`]).
async fn failed_status(
    name: &str,
    reply: reqwest::Response,
) -> Result<reqwest::Response, KeysError> {
    let status = reply.status();
    if status.is_success() {
        return Ok(reply);
    }
    let text = format!(
        "{name}: {status}: {}",
        google_message(&reply.text().await.unwrap_or_default())
    );
    Err(if matches!(status.as_u16(), 401 | 403) {
        KeysError::denied(text)
    } else {
        KeysError::new(text)
    })
}

/// The message of an error of a Google API, `{"error": {"message": …}}`,
/// or the whole text when it has none.
///
/// ```
/// use riff_server::keys::google_message;
///
/// let body = r#"{"error": {"code": 403, "message": "Permission 'secretmanager.versions.access' denied on resource (or it may not exist).", "status": "PERMISSION_DENIED"}}"#;
/// assert_eq!(
///     google_message(body),
///     "Permission 'secretmanager.versions.access' denied on resource (or it may not exist)."
/// );
/// assert_eq!(google_message(" no JSON \n"), "no JSON");
/// ```
pub fn google_message(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| text.trim().to_owned())
}

/// Writes `bytes` to [`FILE`] in `dir`: to a new file with mode `0600`
/// first, then a rename. So the file is whole, and only the account of
/// the server reads it.
fn write_private(dir: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    std::fs::create_dir_all(dir)?;
    let new = dir.join(format!("{FILE}.new"));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&new)?;
    // A file from before keeps its mode: set it again.
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&new, dir.join(FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_file_has_mode_600_and_the_keys() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let keys = Keys::new(At::Dir(dir.path().to_owned()), reqwest::Client::new());
        let (key, secret) = (isolated::fake_secret(), isolated::fake_secret());
        let app = AppKeys::new(7, "riff-acme", "acme", &key, Some(&secret));
        keys.put(GitHubApp::Lead, app.clone()).await.unwrap();
        let path = dir.path().join(FILE);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let apps: Apps = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(apps.lead, Some(app));
        assert!(!dir.path().join(format!("{FILE}.new")).exists());
    }

    #[tokio::test]
    async fn nowhere_refuses_a_write_and_reads_no_app() {
        let keys = Keys::new(At::Nowhere, reqwest::Client::new());
        assert_eq!(keys.load().await.unwrap(), Apps::default());
        let app = AppKeys::new(7, "riff-acme", "acme", &isolated::fake_secret(), None);
        let error = keys.put(GitHubApp::Session, app).await.unwrap_err();
        assert!(error.text.contains("riff cloud create NAME"), "{error}");
        // A failed write keeps no App in memory either.
        assert_eq!(keys.load().await.unwrap(), Apps::default());
    }

    #[test]
    fn the_debug_of_the_apps_names_no_key_and_no_secret() {
        let (key, secret) = (isolated::fake_secret(), isolated::fake_secret());
        let app = AppKeys::new(7, "riff-acme", "acme", &key, Some(&secret));
        let apps = Apps {
            session: Some(app.clone()),
            lead: Some(app),
        };
        let text = format!("{apps:?}");
        assert!(text.contains("riff-acme"), "{text}");
        assert!(!text.contains(&key) && !text.contains(&secret), "{text}");
    }
}
