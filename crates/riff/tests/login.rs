//! `riff login` end to end: a fake provider, a real `riff-server`, and
//! a fake browser that follows the redirects. The keyring is the mock
//! store of `keyring-core`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Once};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use riff::api::Api;
use riff::login::{self, SignIn};
use riff_core::wire::Discovery;
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::{DEFAULT_DOMAIN, Provider};
use serde_json::{Value, json};

const KEY: &str = include_str!("../../riff-server/testdata/test-only-rsa-key.pem");
const JWKS: &str = include_str!("../../riff-server/testdata/test-only-jwks.json");
const CLIENT: &str = "riff-client";

static MOCK_KEYRING: Once = Once::new();

fn mock_keyring() {
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
}

/// The code challenge of each code that the fake provider gave out.
#[derive(Clone, Default)]
struct Fake {
    issuer: String,
    codes: Arc<Mutex<HashMap<String, String>>>,
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
    let challenge = fake.codes.lock().unwrap().remove(&f["code"]);
    if challenge != Some(login::challenge(&f["code_verifier"])) || f["client_id"] != CLIENT {
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
    let claims = json!({
        "iss": fake.issuer,
        "aud": CLIENT,
        "exp": now + 3600,
        "email": "Ada@comotechnologies.io",
        "email_verified": true,
        "hd": "comotechnologies.io",
    });
    let id_token = encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(KEY.as_bytes()).unwrap(),
    )
    .unwrap();
    (StatusCode::OK, Json(json!({ "id_token": id_token })))
}

async fn fake_provider() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let discovery = Discovery {
        issuer: issuer.clone(),
        authorization_endpoint: format!("{issuer}/authorize"),
        token_endpoint: format!("{issuer}/token"),
        jwks_uri: format!("{issuer}/jwks"),
    };
    let jwks: Value = serde_json::from_str(JWKS).unwrap();
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
            ..Fake::default()
        });
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    issuer
}

async fn start() -> (Service, Api) {
    mock_keyring();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = Service::new(Config {
        provider: Some(Provider {
            issuer: fake_provider().await,
            client_id: CLIENT.into(),
            client_secret: None,
            allowed_domains: vec![DEFAULT_DOMAIN.into()],
        }),
        ..Config::new(&url)
    });
    let api = Api::new(&url);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, api)
}

/// The thumbprint of the device key of this machine for the server.
fn jkt(api: &Api) -> String {
    riff::device::key(api.base()).unwrap().thumbprint()
}

/// A browser that goes to the URL and follows each redirect.
fn browser(url: &str) {
    let url = url.to_owned();
    tokio::spawn(async move { reqwest::get(url).await.unwrap().error_for_status().unwrap() });
}

#[tokio::test]
async fn login_signs_in_and_keeps_the_sign_in() {
    let (service, api) = start().await;
    let sign_in = login::login(&api, browser).await.unwrap();
    assert_eq!(sign_in.user, "ada");
    assert_eq!(
        service
            .tokens()
            .check(&sign_in.access_token, &jkt(&api), Instant::now()),
        Ok("ada".to_owned())
    );
    assert_eq!(login::stored(api.base()).unwrap(), Some(sign_in.clone()));
    assert_eq!(login::user(api.base()).unwrap().as_deref(), Some("ada"));

    // A live access token comes from the keyring as it is.
    assert_eq!(
        login::access_token(&api).await.unwrap(),
        sign_in.access_token
    );

    assert!(login::logout(api.base()).unwrap());
    assert_eq!(login::stored(api.base()).unwrap(), None);
    let error = login::access_token(&api).await.unwrap_err();
    assert!(error.to_string().contains("run riff login"), "{error}");
}

#[tokio::test]
async fn an_old_access_token_is_refreshed() {
    let (service, api) = start().await;
    let first = login::login(&api, browser).await.unwrap();
    let old = SignIn {
        expires_at: 0,
        ..first.clone()
    };
    login::store(api.base(), &old).unwrap();

    let fresh = login::access_token(&api).await.unwrap();
    assert_ne!(fresh, first.access_token);
    assert_eq!(
        service.tokens().check(&fresh, &jkt(&api), Instant::now()),
        Ok("ada".to_owned())
    );
    let kept = login::stored(api.base()).unwrap().unwrap();
    assert_eq!(kept.access_token, fresh);
    assert_ne!(kept.refresh_token, first.refresh_token);
}

#[tokio::test]
async fn a_used_refresh_token_ends_the_sign_in() {
    let (_, api) = start().await;
    let first = login::login(&api, browser).await.unwrap();
    let old = SignIn {
        expires_at: 0,
        ..first
    };
    login::store(api.base(), &old).unwrap();
    login::access_token(&api).await.unwrap();

    // Someone replays the old refresh token.
    login::store(api.base(), &old).unwrap();
    let error = login::access_token(&api).await.unwrap_err();
    assert!(format!("{error:#}").contains("run riff login"), "{error:#}");
}

/// At a riff with sign-in, an ended sign-in still says `riff login`
/// (R226).
#[tokio::test]
async fn a_call_with_an_ended_sign_in_says_to_run_riff_login() {
    let (_, api) = start().await;
    let first = login::login(&api, browser).await.unwrap();
    let old = SignIn {
        expires_at: 0,
        ..first
    };
    login::store(api.base(), &old).unwrap();
    login::access_token(&api).await.unwrap();
    login::store(api.base(), &old).unwrap();

    assert!(api.has_sign_in().await.unwrap());
    let me = "riff://ada@pangolin/como-technologies/riff"
        .parse()
        .unwrap();
    let person = api.clone().signed_in(None).unwrap();
    let error = person.who(&me, false).await.unwrap_err();
    assert!(format!("{error:#}").contains("run riff login"), "{error:#}");
}

#[tokio::test]
async fn logout_all_with_no_sign_in_at_a_riff_with_sign_in_says_riff_login() {
    let (_, api) = start().await;
    let error = login::logout_all(&api, None).await.unwrap_err();
    assert!(error.to_string().contains("run riff login"), "{error}");
}

#[tokio::test]
async fn a_server_without_a_provider_says_so() {
    mock_keyring();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = Api::new(&format!("http://{}", listener.local_addr().unwrap()));
    let router = Service::default().router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let error = login::login(&api, |_| panic!("no browser"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("no sign-in provider"), "{error}");
}
