//! A fake GitHub API for the forge tokens (#628): the installation of
//! the App on a repository, the access tokens and the revokes. For
//! `riff forge create` (#627): the conversion of a manifest code, the
//! App, the installations of the App and the accounts. The unit tests of
//! `riff_server::forge` use it too.

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
    /// The App that a manifest code gives: the code, the ID and the
    /// private key.
    conversion: Option<(String, u64, String)>,
    /// Each code that a caller swapped.
    converted: Vec<String>,
}

/// The slug of the App of the fake.
pub const SLUG: &str = "riff-acme";

/// The ID of each account of the fake: its length times 1000.
pub fn account_id(login: &str) -> u64 {
    login.len() as u64 * 1000
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
            .route("/app-manifests/{code}/conversions", post(conversion))
            .route("/app", get(app))
            .route("/app/installations", get(installations))
            .route("/users/{login}", get(user))
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

    /// From now on, the manifest code `code` gives the App `id` with the
    /// private key `pem`, owned by the account `acme`.
    pub fn convert_to(&self, code: &str, id: u64, pem: &str) {
        self.inner.lock().unwrap().conversion = Some((code.to_owned(), id, pem.to_owned()));
    }

    /// Each manifest code that a caller swapped, in order.
    pub fn converted(&self) -> Vec<String> {
        self.inner.lock().unwrap().converted.clone()
    }

    /// The App is now installed on `repo` with the installation `id`.
    pub fn install(&self, repo: &str, id: u64) {
        self.inner.lock().unwrap().installs.insert(repo.to_owned(), id);
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
        .map(|r| {
            r.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
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

/// The conversion of a manifest code: no sign-in, as on GitHub.
async fn conversion(
    State(inner): State<Arc<Mutex<Inner>>>,
    Path(code): Path<String>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let mut inner = inner.lock().unwrap();
    inner.converted.push(code.clone());
    match &inner.conversion {
        Some((c, id, pem)) if *c == code => Ok((
            StatusCode::CREATED,
            Json(json!({
                "id": id,
                "slug": SLUG,
                "pem": pem,
                "owner": { "login": "acme", "id": account_id("acme") },
            })),
        )),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

/// The App of the JWT.
async fn app(headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    if bearer(&headers).is_none_or(|jwt| jwt.split('.').count() != 3) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(json!({ "slug": SLUG })))
}

/// The installations of the App: one for each account that has an
/// installed repository.
async fn installations(
    State(inner): State<Arc<Mutex<Inner>>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    if bearer(&headers).is_none_or(|jwt| jwt.split('.').count() != 3) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let inner = inner.lock().unwrap();
    let mut owners: Vec<&str> = inner
        .installs
        .keys()
        .filter_map(|repo| repo.split('/').next())
        .collect();
    owners.sort_unstable();
    owners.dedup();
    let all: Vec<Value> = owners
        .iter()
        .map(|login| json!({ "account": { "login": login } }))
        .collect();
    Ok(Json(Value::Array(all)))
}

/// Each account exists, with the ID of [`account_id`].
async fn user(Path(login): Path<String>) -> Json<Value> {
    Json(json!({ "login": login, "id": account_id(&login) }))
}
