//! A fake OpenID Connect provider and a fake browser, for the tests
//! that sign in end to end. Also the mock keyring, and a listener whose
//! URL has no old sign-in in it.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Once};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use riff::login;
use riff_core::wire::Discovery;
use serde_json::{Value, json};

const KEY: &str = include_str!("../../../riff-server/testdata/test-only-rsa-key.pem");
const JWKS: &str = include_str!("../../../riff-server/testdata/test-only-jwks.json");

/// Makes the mock store of `keyring-core` the keyring of this test
/// process. All tests of the test binary share it.
pub fn mock_keyring() {
    static MOCK_KEYRING: Once = Once::new();
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
}

/// The URL of `listener`, with no sign-in for it in the mock keyring.
/// The keyring keeps a sign-in by the URL of its server. When a test
/// ends, its port is free, and the OS can give the port to a later
/// test. So a new server removes the sign-in that an earlier test left
/// at its URL.
pub fn fresh_url(listener: &tokio::net::TcpListener) -> String {
    mock_keyring();
    let url = format!("http://{}", listener.local_addr().unwrap());
    login::logout(&url).unwrap();
    url
}

/// A listener on a free port, and its URL from [`fresh_url`].
pub async fn listen() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = fresh_url(&listener);
    (listener, url)
}

/// The client ID of riff at the fake provider.
pub const CLIENT: &str = "riff-client";

/// The refresh token of the account at the fake provider: the token of
/// the test account of a smoke test.
pub const REFRESH: &str = "test-refresh-token";

/// An account at the fake provider: its email, and its `hd` claim.
#[derive(Clone)]
struct Account {
    email: String,
    hd: Option<String>,
}

/// The state of the fake provider: the code challenge of each code
/// that it gave out, and the account that signs in.
#[derive(Clone)]
struct Fake {
    issuer: String,
    codes: Arc<Mutex<HashMap<String, String>>>,
    account: Arc<Mutex<Account>>,
}

/// A fake provider on a free port. It signs in one account at a time.
pub struct FakeProvider {
    /// The issuer URL of the provider.
    pub issuer: String,
    account: Arc<Mutex<Account>>,
}

impl FakeProvider {
    /// Starts a provider that signs in `email`, with the `hd` claim
    /// `hd`.
    pub async fn start(email: &str, hd: Option<&str>) -> FakeProvider {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let discovery = Discovery {
            issuer: issuer.clone(),
            authorization_endpoint: format!("{issuer}/authorize"),
            token_endpoint: format!("{issuer}/token"),
            jwks_uri: format!("{issuer}/jwks"),
        };
        let jwks: Value = serde_json::from_str(JWKS).unwrap();
        let account = Arc::new(Mutex::new(Account {
            email: email.into(),
            hd: hd.map(Into::into),
        }));
        let router = Router::new()
            .route(
                "/.well-known/openid-configuration",
                get(move || async move { Json(discovery) }),
            )
            .route("/jwks", get(move || async move { Json(jwks) }))
            .route("/authorize", get(authorize))
            .route("/token", post(provider_token))
            .with_state(Fake {
                issuer: issuer.clone(),
                codes: Arc::default(),
                account: account.clone(),
            });
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        FakeProvider { issuer, account }
    }

    /// From now on, the provider signs in `email`, with the `hd` claim
    /// `hd`.
    pub fn sign_in_as(&self, email: &str, hd: Option<&str>) {
        *self.account.lock().unwrap() = Account {
            email: email.into(),
            hd: hd.map(Into::into),
        };
    }
}

async fn authorize(State(fake): State<Fake>, Query(q): Query<HashMap<String, String>>) -> Redirect {
    assert_eq!(q["client_id"], CLIENT);
    assert_eq!(q["code_challenge_method"], "S256");
    let code = format!("code-{}", fake.codes.lock().unwrap().len());
    fake.codes
        .lock()
        .unwrap()
        .insert(code.clone(), q["code_challenge"].clone());
    let to = format!("{}?code={code}&state={}", q["redirect_uri"], q["state"]);
    Redirect::to(&to)
}

async fn provider_token(
    State(fake): State<Fake>,
    Form(f): Form<HashMap<String, String>>,
) -> impl IntoResponse {
    let granted = match f.get("grant_type").map(String::as_str) {
        Some("refresh_token") => f.get("refresh_token").map(String::as_str) == Some(REFRESH),
        _ => {
            let challenge = fake.codes.lock().unwrap().remove(&f["code"]);
            challenge == Some(login::challenge(&f["code_verifier"]))
        }
    };
    if !granted || f["client_id"] != CLIENT {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_grant"})),
        );
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test".into());
    let account = fake.account.lock().unwrap().clone();
    let mut claims = json!({
        "iss": fake.issuer,
        "aud": CLIENT,
        "exp": now + 3600,
        "email": account.email,
        "email_verified": true,
    });
    if let Some(hd) = account.hd {
        claims["hd"] = json!(hd);
    }
    let id_token = encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(KEY.as_bytes()).unwrap(),
    )
    .unwrap();
    (StatusCode::OK, Json(json!({ "id_token": id_token })))
}

/// A browser that goes to the URL and follows each redirect.
pub fn browser(url: &str) {
    let url = url.to_owned();
    tokio::spawn(async move { reqwest::get(url).await.unwrap().error_for_status().unwrap() });
}
