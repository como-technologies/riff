//! A fake GitHub API for the forge tokens (#628): the installation of
//! the App on a repository, the access tokens and the revokes. The unit
//! tests of `riff_server::forge` use it too.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

/// One access token that the fake gave.
#[derive(Clone, Debug)]
pub struct Asked {
    pub installation: u64,
    pub repositories: Vec<String>,
    pub permissions: Value,
    pub token: String,
}

#[derive(Default)]
struct Inner {
    /// The installation of each repository, `OWNER/NAME`.
    installs: HashMap<String, u64>,
    asked: Vec<Asked>,
    revoked: Vec<String>,
    /// A permission that the fake adds to each token: an App with too
    /// many rights.
    extra: Option<(String, String)>,
}

/// A fake GitHub on a free port.
#[derive(Clone)]
pub struct FakeGitHub {
    pub url: String,
    inner: Arc<Mutex<Inner>>,
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get("authorization")?.to_str().ok()?;
    Some(value.strip_prefix("Bearer ")?.to_owned())
}

impl FakeGitHub {
    /// A fake where the App has the installation of each `(repo, id)`.
    pub async fn start(installs: &[(&str, u64)]) -> FakeGitHub {
        let inner = Arc::new(Mutex::new(Inner {
            installs: installs
                .iter()
                .map(|(repo, id)| ((*repo).to_owned(), *id))
                .collect(),
            ..Inner::default()
        }));
        let app = Router::new()
            .route("/repos/{owner}/{name}/installation", get(installation))
            .route("/app/installations/{id}/access_tokens", post(access_token))
            .route("/installation/token", delete(revoke))
            .with_state(inner.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        FakeGitHub { url, inner }
    }

    /// Each token that the fake gave, in order.
    pub fn asked(&self) -> Vec<Asked> {
        self.inner.lock().unwrap().asked.clone()
    }

    /// Each token that a caller revoked, in order.
    pub fn revoked(&self) -> Vec<String> {
        self.inner.lock().unwrap().revoked.clone()
    }

    /// From now on, each token also has the permission `name` at
    /// `level`.
    pub fn add_to_each_token(&self, name: &str, level: &str) {
        self.inner.lock().unwrap().extra = Some((name.to_owned(), level.to_owned()));
    }
}

async fn installation(
    State(inner): State<Arc<Mutex<Inner>>>,
    Path((owner, name)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    // The JWT of the App: three parts.
    if bearer(&headers).is_none_or(|jwt| jwt.split('.').count() != 3) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let id = inner
        .lock()
        .unwrap()
        .installs
        .get(&format!("{owner}/{name}"))
        .copied()
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(json!({ "id": id })))
}

async fn access_token(
    State(inner): State<Arc<Mutex<Inner>>>,
    Path(id): Path<u64>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    if bearer(&headers).is_none_or(|jwt| jwt.split('.').count() != 3) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let mut inner = inner.lock().unwrap();
    let token = format!("ghs_fake_{}", inner.asked.len() + 1);
    let mut permissions = body["permissions"].clone();
    if let Some((name, level)) = &inner.extra {
        permissions[name] = json!(level);
    }
    let repositories = body["repositories"]
        .as_array()
        .map(|r| r.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    inner.asked.push(Asked {
        installation: id,
        repositories,
        permissions: body["permissions"].clone(),
        token: token.clone(),
    });
    let expires_at = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    Ok((
        StatusCode::CREATED,
        Json(json!({ "token": token, "expires_at": expires_at, "permissions": permissions })),
    ))
}

async fn revoke(State(inner): State<Arc<Mutex<Inner>>>, headers: HeaderMap) -> StatusCode {
    match bearer(&headers) {
        Some(token) => {
            inner.lock().unwrap().revoked.push(token);
            StatusCode::NO_CONTENT
        }
        None => StatusCode::UNAUTHORIZED,
    }
}
