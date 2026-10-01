//! A [`Store`] in a Cloud Storage bucket (R30, R141).
//!
//! # Calls
//!
//! The store calls the Cloud Storage JSON API with `reqwest`. It adds no
//! Google SDK crate (R123).
//!
//! | Store call | Cloud Storage call |
//! |---|---|
//! | [`Store::load`] | `GET /storage/v1/b/BUCKET/o/NAME?alt=media`. The version is the `x-goog-generation` header. |
//! | [`Store::list`] | `GET /storage/v1/b/BUCKET/o?prefix=PREFIX`, page by page. |
//! | [`Store::save`] | `POST /upload/storage/v1/b/BUCKET/o?uploadType=media&name=NAME&ifGenerationMatch=KNOWN`. |
//!
//! The [`Version`] of an object is its generation. A save of a new object
//! sends `ifGenerationMatch=0`. When the bucket holds another generation,
//! Cloud Storage replies 412 and the save fails with
//! [`StoreError::Conflict`].
//!
//! # Access token
//!
//! The store gets its access token from the metadata server of Cloud
//! Run. It keeps the token until one minute before it expires.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant S as riff-server
//!     participant M as Metadata server
//!     participant G as Cloud Storage
//!     S->>M: GET .../service-accounts/default/token
//!     M-->>S: access_token, expires_in
//!     S->>G: POST upload, ifGenerationMatch=KNOWN
//!     alt the bucket holds KNOWN
//!         G-->>S: 200, generation
//!     else another generation
//!         G-->>S: 412, the save fails
//!     end
//! ```
//!
//! # Example
//!
//! ```no_run
//! use riff_server::gcs::Gcs;
//! use riff_server::store::Store;
//!
//! # async fn run() -> Result<(), riff_server::store::StoreError> {
//! let store = Gcs::new("como-riff-state");
//! let version = store.save("tokens", b"{}".to_vec(), None).await?;
//! assert_eq!(store.load("tokens").await?.unwrap().version, version);
//! # Ok(())
//! # }
//! ```

use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures::future::{BoxFuture, FutureExt};
use reqwest::StatusCode;
use serde::Deserialize;

use crate::store::{Loaded, Store, StoreError, Version};

/// The Cloud Storage API.
pub const STORAGE: &str = "https://storage.googleapis.com";

/// The metadata server of Cloud Run.
pub const METADATA: &str = "http://metadata.google.internal";

/// The path of the access token on the metadata server.
pub const TOKEN_PATH: &str = "/computeMetadata/v1/instance/service-accounts/default/token";

/// The longest wait for each call.
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// The store keeps an access token until this time before it expires.
const TOKEN_MARGIN: Duration = Duration::from_secs(60);

/// A store in a Cloud Storage bucket. See the module docs.
pub struct Gcs {
    http: reqwest::Client,
    bucket: String,
    storage: String,
    metadata: String,
    token: Mutex<Option<Token>>,
}

struct Token {
    value: String,
    until: Instant,
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct ObjectReply {
    generation: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListReply {
    #[serde(default)]
    items: Vec<Item>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct Item {
    name: String,
}

impl Gcs {
    /// A store in `bucket`, for a server on Cloud Run.
    pub fn new(bucket: &str) -> Gcs {
        Gcs::with_urls(bucket, STORAGE, METADATA)
    }

    /// A store in `bucket` that calls other servers than [`STORAGE`] and
    /// [`METADATA`]. Tests use it with a fake server.
    pub fn with_urls(bucket: &str, storage: &str, metadata: &str) -> Gcs {
        Gcs {
            http: crate::oidc::client(TIMEOUT),
            bucket: bucket.to_owned(),
            storage: storage.trim_end_matches('/').to_owned(),
            metadata: metadata.trim_end_matches('/').to_owned(),
            token: Mutex::new(None),
        }
    }

    fn cached_token(&self) -> Option<String> {
        let token = self.token.lock().unwrap_or_else(|p| p.into_inner());
        token
            .as_ref()
            .filter(|t| Instant::now() < t.until)
            .map(|t| t.value.clone())
    }

    async fn access_token(&self) -> Result<String, StoreError> {
        if let Some(value) = self.cached_token() {
            return Ok(value);
        }
        let url = format!("{}{TOKEN_PATH}", self.metadata);
        let reply: TokenReply = check(
            self.http
                .get(&url)
                .header("Metadata-Flavor", "Google")
                .send()
                .await,
            &url,
        )
        .await?
        .json()
        .await
        .map_err(|e| failed(&url, e))?;
        let life = Duration::from_secs(reply.expires_in).saturating_sub(TOKEN_MARGIN);
        *self.token.lock().unwrap_or_else(|p| p.into_inner()) = Some(Token {
            value: reply.access_token.clone(),
            until: Instant::now() + life,
        });
        Ok(reply.access_token)
    }

    fn object_url(&self, name: &str) -> String {
        format!(
            "{}/storage/v1/b/{}/o/{}",
            self.storage,
            encode(&self.bucket),
            encode(name)
        )
    }

    async fn get(&self, name: &str) -> Result<Option<Loaded>, StoreError> {
        let url = self.object_url(name);
        let reply = self
            .http
            .get(&url)
            .query(&[("alt", "media")])
            .bearer_auth(self.access_token().await?)
            .send()
            .await;
        if matches!(&reply, Ok(r) if r.status() == StatusCode::NOT_FOUND) {
            return Ok(None);
        }
        let reply = check(reply, &url).await?;
        let version = reply
            .headers()
            .get("x-goog-generation")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| StoreError::Failed(format!("{url}: no x-goog-generation header")))?;
        let bytes = reply.bytes().await.map_err(|e| failed(&url, e))?.to_vec();
        Ok(Some(Loaded { bytes, version }))
    }

    async fn names(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let url = format!("{}/storage/v1/b/{}/o", self.storage, encode(&self.bucket));
        let mut names = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let mut query = vec![("prefix", prefix), ("fields", "items(name),nextPageToken")];
            if let Some(page) = &page {
                query.push(("pageToken", page));
            }
            let request = self
                .http
                .get(&url)
                .query(&query)
                .bearer_auth(self.access_token().await?);
            let reply: ListReply = check(request.send().await, &url)
                .await?
                .json()
                .await
                .map_err(|e| failed(&url, e))?;
            names.extend(reply.items.into_iter().map(|item| item.name));
            match reply.next_page_token {
                Some(next) => page = Some(next),
                None => return Ok(names),
            }
        }
    }

    /// Deletes one object. A missing object is done.
    async fn remove(&self, name: &str) -> Result<(), StoreError> {
        let url = self.object_url(name);
        let reply = self
            .http
            .delete(&url)
            .bearer_auth(self.access_token().await?)
            .send()
            .await;
        if matches!(&reply, Ok(r) if r.status() == StatusCode::NOT_FOUND) {
            return Ok(());
        }
        check(reply, &url).await.map(drop)
    }

    /// One conditional upload. `None` names no object.
    async fn put(
        &self,
        name: &str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> Result<Version, StoreError> {
        let url = format!(
            "{}/upload/storage/v1/b/{}/o",
            self.storage,
            encode(&self.bucket)
        );
        let generation = known.unwrap_or(0).to_string();
        let reply = self
            .http
            .post(&url)
            .query(&[
                ("uploadType", "media"),
                ("name", name),
                ("ifGenerationMatch", &generation),
            ])
            .bearer_auth(self.access_token().await?)
            .header("content-type", "application/json")
            .body(bytes)
            .send()
            .await;
        if matches!(&reply, Ok(r) if r.status() == StatusCode::PRECONDITION_FAILED) {
            return Err(StoreError::Conflict(name.to_owned()));
        }
        let reply: ObjectReply = check(reply, &url)
            .await?
            .json()
            .await
            .map_err(|e| failed(&url, e))?;
        reply
            .generation
            .parse()
            .map_err(|_| StoreError::Failed(format!("{url}: bad generation")))
    }
}

impl Store for Gcs {
    fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
        self.get(name).boxed()
    }

    fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>> {
        self.names(prefix).boxed()
    }

    fn save<'a>(
        &'a self,
        name: &'a str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> BoxFuture<'a, Result<Version, StoreError>> {
        self.put(name, bytes, known).boxed()
    }

    fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), StoreError>> {
        self.remove(name).boxed()
    }

    fn locate(&self, name: &str) -> String {
        format!("gs://{}/{name}", self.bucket)
    }

    fn empty_command(&self) -> Option<String> {
        Some(format!("gcloud storage rm 'gs://{}/**'", self.bucket))
    }
}

/// The reply, or [`StoreError::Failed`] when the call failed or its
/// status is not a success.
async fn check(
    reply: Result<reqwest::Response, reqwest::Error>,
    url: &str,
) -> Result<reqwest::Response, StoreError> {
    let reply = reply.map_err(|e| failed(url, e))?;
    let status = reply.status();
    if status.is_success() {
        return Ok(reply);
    }
    let body = reply.text().await.unwrap_or_default();
    Err(StoreError::Failed(format!("{url}: {status}: {body}")))
}

fn failed(url: &str, error: reqwest::Error) -> StoreError {
    StoreError::Failed(format!("{url}: {error}"))
}

/// One path segment: each byte outside `A-Z a-z 0-9 - _ . ~` becomes
/// `%XX`.
fn encode(segment: &str) -> String {
    let mut encoded = String::new();
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_is_one_path_segment() {
        assert_eq!(
            encode("log/00000000000000000001.jsonl"),
            "log%2F00000000000000000001.jsonl"
        );
    }

    #[test]
    fn unreserved_bytes_stay() {
        assert_eq!(encode("como-riff_state.v1~"), "como-riff_state.v1~");
    }

    #[test]
    fn urls_lose_a_trailing_slash() {
        let store = Gcs::with_urls("b", "http://g/", "http://m/");
        assert_eq!(
            store.object_url("sessions"),
            "http://g/storage/v1/b/b/o/sessions"
        );
    }
}
