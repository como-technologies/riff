//! A fake GitHub for the tests of the GitHub Apps of the riff: the swap
//! of the code of a manifest, the bot users, the installations, the
//! tokens and their revoke. It checks the JWT of each call of an App
//! with the public key of the test key. The tests of `riff` include this
//! file too.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde_json::{Value, json};

/// The account of the Apps.
pub const OWNER: &str = "acme";

/// The repository of the tests, on [`OWNER`].
pub const REPO: &str = "acme/app";

/// The private key of each App: the test key.
pub const KEY: &str = include_str!("../testdata/test-only-rsa-key.pem");

/// The public key of the test key.
const JWKS: &str = include_str!("../testdata/test-only-jwks.json");

/// The webhook secret of the session App.
pub const WEBHOOK_SECRET: &str = "whsec-of-the-session-app-of-the-test";

/// The code that GitHub gives for the session App, and the one for the
/// lead App.
pub const SESSION_CODE: &str = "code-of-the-session-app";
pub const LEAD_CODE: &str = "code-of-the-lead-app";

/// The ID, the slug and the bot ID of the App of `code`.
pub fn app_of(code: &str) -> Option<(u64, &'static str, u64)> {
    match code {
        SESSION_CODE => Some((101, "riff-acme", 1101)),
        LEAD_CODE => Some((202, "riff-acme-lead", 1202)),
        _ => None,
    }
}

/// What the fake saw, and how it answers.
#[derive(Default)]
pub struct Seen {
    /// True: both Apps are installed on [`OWNER`], with [`REPO`].
    pub installed: bool,
    /// The repositories that the person added to the install of each
    /// App, by the App ID, beyond [`REPO`].
    pub added: BTreeMap<u64, Vec<String>>,
    /// True: the swap of the lead App installs both Apps, as the clicks
    /// of the person do.
    pub install_on_swap: bool,
    /// The codes that riff-server swapped.
    pub swapped: Vec<String>,
    /// The App ID and the body of each token request.
    pub asked: Vec<(u64, Value)>,
    /// Each token that the fake gave.
    pub tokens: Vec<String>,
    /// Each token that riff-server revoked.
    pub revoked: Vec<String>,
    /// The rights that the fake gives in place of the asked ones.
    pub give: Option<BTreeMap<String, String>>,
    /// True: each token ends one hour after now, as on GitHub. Else it
    /// ends at a time of the tests.
    pub hour: bool,
}

/// A fake GitHub on a free port.
#[derive(Clone)]
pub struct FakeGitHub {
    pub url: String,
    pub seen: Arc<Mutex<Seen>>,
}

impl FakeGitHub {
    pub async fn start() -> FakeGitHub {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let router = Router::new()
            .route("/app-manifests/{code}/conversions", post(convert))
            .route("/users/{login}", get(user))
            .route("/repos/{owner}/{repo}/installation", get(installation))
            .route("/app/installations", get(installations))
            .route("/app/installations/{id}/access_tokens", post(token))
            .route("/installation/token", delete(revoke))
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        FakeGitHub { url, seen }
    }

    pub fn seen(&self) -> std::sync::MutexGuard<'_, Seen> {
        self.seen.lock().unwrap()
    }
}

type Shared = Arc<Mutex<Seen>>;

async fn convert(State(seen): State<Shared>, Path(code): Path<String>) -> Response {
    let Some((id, slug, _)) = app_of(&code) else {
        return (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"}))).into_response();
    };
    {
        let mut seen = seen.lock().unwrap();
        seen.swapped.push(code.clone());
        if code == LEAD_CODE && seen.install_on_swap {
            seen.installed = true;
        }
    }
    let webhook_secret = (code == SESSION_CODE).then_some(WEBHOOK_SECRET);
    Json(json!({
        "id": id,
        "slug": slug,
        "name": slug,
        "node_id": "A_1",
        "owner": { "login": OWNER, "id": 7 },
        "client_id": "Iv1.client",
        "client_secret": "client-secret-of-the-test",
        "webhook_secret": webhook_secret,
        "pem": KEY,
    }))
    .into_response()
}

async fn user(Path(login): Path<String>) -> Response {
    let bot = [SESSION_CODE, LEAD_CODE]
        .into_iter()
        .filter_map(app_of)
        .find(|(_, slug, _)| format!("{slug}[bot]") == login);
    match bot {
        Some((_, _, bot_id)) => Json(json!({ "login": login, "id": bot_id })).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"}))).into_response(),
    }
}

/// The refusal of a call with no good JWT.
fn bad_credentials() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"message": "Bad credentials"})),
    )
        .into_response()
}

/// The App ID of the JWT of `headers`, or `None`.
fn app_id(headers: &HeaderMap) -> Option<u64> {
    let jwt = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))?;
    let jwks: Value = serde_json::from_str(JWKS).unwrap();
    let jwk = &jwks["keys"][0];
    let key =
        DecodingKey::from_rsa_components(jwk["n"].as_str().unwrap(), jwk["e"].as_str().unwrap())
            .unwrap();
    let mut validation = Validation::new(Algorithm::RS256);
    // GitHub takes the App ID as a number in `iss`: the test reads it.
    validation.set_required_spec_claims(&["exp", "iat"]);
    let claims = jsonwebtoken::decode::<Value>(jwt, &key, &validation).ok()?;
    claims.claims["iss"].as_u64()
}

async fn installation(
    State(seen): State<Shared>,
    headers: HeaderMap,
    Path((owner, repo)): Path<(String, String)>,
) -> Response {
    let Some(id) = app_id(&headers) else {
        return bad_credentials();
    };
    let repo = format!("{owner}/{repo}");
    let has = {
        let seen = seen.lock().unwrap();
        let added = seen
            .added
            .get(&id)
            .is_some_and(|added| added.contains(&repo));
        seen.installed && (repo == REPO || added)
    };
    if !has {
        return (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"}))).into_response();
    }
    Json(json!({ "id": installation_of(id), "account": { "login": OWNER } })).into_response()
}

/// The ID of the install of the App `id` on [`OWNER`].
pub fn installation_of(id: u64) -> u64 {
    id * 10
}

async fn installations(State(seen): State<Shared>, headers: HeaderMap) -> Response {
    let Some(id) = app_id(&headers) else {
        return bad_credentials();
    };
    if !seen.lock().unwrap().installed {
        return Json(json!([])).into_response();
    }
    Json(json!([{ "id": installation_of(id), "account": { "login": OWNER } }])).into_response()
}

async fn token(
    State(seen): State<Shared>,
    headers: HeaderMap,
    Path(installation): Path<u64>,
    Json(body): Json<Value>,
) -> Response {
    let Some(id) = app_id(&headers) else {
        return bad_credentials();
    };
    if installation != installation_of(id) {
        return (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"}))).into_response();
    }
    let mut seen = seen.lock().unwrap();
    let ends = match seen.hour {
        true => (chrono::Utc::now() + chrono::Duration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string(),
        false => "2026-11-08T13:00:00Z".to_owned(),
    };
    let token = format!("ghs_token_of_the_test_{}", seen.tokens.len());
    let permissions = match &seen.give {
        Some(give) => json!(give),
        None => body["permissions"].clone(),
    };
    seen.asked.push((id, body));
    seen.tokens.push(token.clone());
    (
        StatusCode::CREATED,
        Json(json!({
            "token": token,
            "expires_at": ends,
            "permissions": permissions,
            "repository_selection": "selected",
        })),
    )
        .into_response()
}

async fn revoke(State(seen): State<Shared>, headers: HeaderMap) -> StatusCode {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_owned();
    let mut seen = seen.lock().unwrap();
    if !seen.tokens.contains(&token) {
        return StatusCode::UNAUTHORIZED;
    }
    seen.revoked.push(token);
    StatusCode::NO_CONTENT
}

/// The keys of both Apps of the fake, as riff-server keeps them after
/// the swap: for a test that starts after `riff github create`.
pub fn both_apps() -> riff_server::keys::Apps {
    let keys = |code: &str, secret: Option<&str>| {
        let (id, slug, bot_id) = app_of(code).unwrap();
        let mut keys = riff_server::keys::AppKeys::new(id, slug, OWNER, KEY, secret);
        keys.bot_id = Some(bot_id);
        keys
    };
    riff_server::keys::Apps {
        session: Some(keys(SESSION_CODE, Some(WEBHOOK_SECRET))),
        lead: Some(keys(LEAD_CODE, None)),
    }
}
