//! Where riff-server keeps the GitHub App of riff: its ID and its
//! private key (#627).
//!
//! On Cloud Run, the App is one secret in Secret Manager
//! ([`Store::secret`]): the deploy names it in [`SECRET_VAR`], and only
//! the service account of riff-server reads it and adds a version
//! (`riff cloud create`). Each version holds [`Stored`] as JSON. The
//! server reads the latest version at its start ([`Store::read`]), and
//! adds a version when `riff forge create` made a new App
//! ([`Store::write`]). So the private key goes from GitHub to the server
//! and to Secret Manager, and never to a machine
//! (01M4CTAYRSC27Q6AAGTH9CBD7Q).
//!
//! A test uses [`Store::memory`].
//!
//! ```
//! # #[tokio::main(flavor = "current_thread")] async fn main() {
//! use riff_server::forge::store::{Store, Stored};
//!
//! let store = Store::memory();
//! assert_eq!(store.read().await.unwrap(), None);
//! let app = Stored { app: 7, key: "PEM".into() };
//! store.write(&app).await.unwrap();
//! assert_eq!(store.read().await.unwrap(), Some(app.clone()));
//! // The key is in no `Debug` text.
//! assert!(!format!("{app:?}").contains("PEM"));
//! # }
//! ```

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

/// The variable that names the secret of the App:
/// `projects/PROJECT/secrets/NAME`.
pub const SECRET_VAR: &str = "RIFF_FORGE_SECRET";

/// The Secret Manager API.
pub const SECRET_MANAGER: &str = "https://secretmanager.googleapis.com";

/// The longest wait for a call to Secret Manager.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The App in one version of the secret.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    /// The ID of the App.
    pub app: u64,
    /// The private key of the App, in PEM form.
    pub key: String,
}

impl fmt::Debug for Stored {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Stored")
            .field("app", &self.app)
            .finish_non_exhaustive()
    }
}

/// Where the server keeps the App. See the module docs.
#[derive(Clone)]
pub enum Store {
    /// One secret in Secret Manager.
    Secret {
        /// `projects/PROJECT/secrets/NAME`.
        name: String,
        /// The base URL of the Secret Manager API.
        api: String,
        /// The metadata server that gives the access token.
        metadata: String,
    },
    /// The versions in memory, for a test.
    Memory(Arc<Mutex<Vec<Stored>>>),
}

impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Store::Secret { name, .. } => write!(f, "Store::Secret({name})"),
            Store::Memory(_) => f.write_str("Store::Memory"),
        }
    }
}

/// Two stores are the same when they name the same secret, or share
/// the same memory.
impl PartialEq for Store {
    fn eq(&self, other: &Store) -> bool {
        match (self, other) {
            (Store::Secret { name: a, .. }, Store::Secret { name: b, .. }) => a == b,
            (Store::Memory(a), Store::Memory(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for Store {}

#[derive(Deserialize)]
struct Access {
    payload: Payload,
}

#[derive(Serialize, Deserialize)]
struct Payload {
    data: String,
}

#[derive(Serialize)]
struct AddVersion {
    payload: Payload,
}

#[derive(Deserialize)]
struct MetadataToken {
    access_token: String,
}

impl Store {
    /// The secret `name` (`projects/PROJECT/secrets/NAME`) on Cloud Run.
    pub fn secret(name: &str) -> Store {
        Store::Secret {
            name: name.trim().to_owned(),
            api: SECRET_MANAGER.to_owned(),
            metadata: crate::gcs::METADATA.to_owned(),
        }
    }

    /// An empty store in memory, for a test.
    pub fn memory() -> Store {
        Store::Memory(Arc::new(Mutex::new(Vec::new())))
    }

    /// Each version of a store in memory, the oldest first. Empty for a
    /// secret.
    pub fn versions(&self) -> Vec<Stored> {
        match self {
            Store::Memory(versions) => versions.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            Store::Secret { .. } => Vec::new(),
        }
    }

    /// The App of the latest version, or `None` when the store has no
    /// version.
    pub async fn read(&self) -> Result<Option<Stored>, String> {
        let (name, api, metadata) = match self {
            Store::Memory(versions) => {
                let versions = versions.lock().unwrap_or_else(|p| p.into_inner());
                return Ok(versions.last().cloned());
            }
            Store::Secret {
                name,
                api,
                metadata,
            } => (name, api, metadata),
        };
        let http = crate::oidc::client(TIMEOUT);
        let token = access_token(&http, metadata).await?;
        let url = format!("{api}/v1/{name}/versions/latest:access");
        let reply = http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("cannot read the secret {name}: {e}"))?;
        // No secret, or a secret with no version: no App yet.
        if reply.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !reply.status().is_success() {
            return Err(format!("cannot read the secret {name}: {}", reply.status()));
        }
        let access: Access = reply
            .json()
            .await
            .map_err(|e| format!("the secret {name} gave no valid reply: {e}"))?;
        let bytes = STANDARD
            .decode(access.payload.data)
            .map_err(|_| format!("the secret {name} holds no base64"))?;
        serde_json::from_slice(&bytes).map(Some).map_err(|_| {
            format!("the secret {name} holds no App: give it one with riff forge create")
        })
    }

    /// Adds `stored` as the new latest version.
    pub async fn write(&self, stored: &Stored) -> Result<(), String> {
        let (name, api, metadata) = match self {
            Store::Memory(versions) => {
                let mut versions = versions.lock().unwrap_or_else(|p| p.into_inner());
                versions.push(stored.clone());
                return Ok(());
            }
            Store::Secret {
                name,
                api,
                metadata,
            } => (name, api, metadata),
        };
        let http = crate::oidc::client(TIMEOUT);
        let token = access_token(&http, metadata).await?;
        let json = serde_json::to_vec(stored).map_err(|e| e.to_string())?;
        let url = format!("{api}/v1/{name}:addVersion");
        let reply = http
            .post(&url)
            .bearer_auth(token)
            .json(&AddVersion {
                payload: Payload {
                    data: STANDARD.encode(json),
                },
            })
            .send()
            .await
            .map_err(|e| format!("cannot write the secret {name}: {e}"))?;
        if !reply.status().is_success() {
            return Err(format!(
                "cannot write the secret {name}: {}. Run riff cloud create again: it lets \
                 riff-server add a version",
                reply.status()
            ));
        }
        Ok(())
    }
}

/// The access token of the service account, from the metadata server.
async fn access_token(http: &reqwest::Client, metadata: &str) -> Result<String, String> {
    let url = format!("{metadata}{}", crate::gcs::TOKEN_PATH);
    let reply = http
        .get(&url)
        .header("Metadata-Flavor", "Google")
        .send()
        .await
        .map_err(|e| format!("no access token for Secret Manager: {e}"))?;
    if !reply.status().is_success() {
        return Err(format!(
            "no access token for Secret Manager: {}",
            reply.status()
        ));
    }
    let token: MetadataToken = reply
        .json()
        .await
        .map_err(|e| format!("no access token for Secret Manager: {e}"))?;
    Ok(token.access_token)
}
