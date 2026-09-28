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

/// The person reads why the server refuses the sign-in: another email
/// holds their USER (R209).
#[tokio::test]
async fn login_says_that_another_account_holds_the_user() {
    let (service, api) = start().await;
    // Another email signed in as `ada` first.
    service
        .tokens()
        .sign_in("ada@other.test", "k", Instant::now())
        .unwrap();
    let error = login::login(&api, browser).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("access_denied: the user ada belongs to another account"),
        "{error}"
    );
    assert_eq!(login::stored(api.base()).unwrap(), None);
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

/// A server with sign-in at a fixed URL, whose riff a test can replace:
/// a new riff at the same URL, or a restart.
struct Front {
    url: String,
    issuer: String,
    current: Arc<Mutex<Router>>,
}

impl Front {
    async fn start() -> (Front, Api) {
        mock_keyring();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let current = Arc::new(Mutex::new(Router::new()));
        let serve = current.clone();
        let router = Router::new().fallback(move |request: axum::extract::Request| {
            let router = serve.lock().unwrap().clone();
            async move { tower::ServiceExt::oneshot(router, request).await }
        });
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let front = Front {
            url: url.clone(),
            issuer: fake_provider().await,
            current,
        };
        (front, Api::new(&url))
    }

    fn config(&self) -> Config {
        let mut config = Config {
            provider: Some(Provider {
                issuer: self.issuer.clone(),
                client_id: CLIENT.into(),
                client_secret: None,
                allowed_domains: vec![DEFAULT_DOMAIN.into()],
            }),
            ..Config::new(&self.url)
        };
        config.lease.wait = std::time::Duration::from_millis(10);
        config
    }

    /// Serves `service` from now on.
    fn serve(&self, service: &Service) {
        *self.current.lock().unwrap() = service.router();
    }
}

fn ada() -> riff_core::name::SessionUri {
    "riff://ada@pangolin/como-technologies/riff"
        .parse()
        .unwrap()
}

/// After a new riff at the same URL, the next command removes the old
/// sign-in and says to run `riff login`, with no other error. The
/// command after it runs with no sign-in (01M3JNVBRS35B3CD67367JF7SJ).
#[tokio::test]
async fn a_new_riff_at_the_same_url_asks_for_riff_login() {
    let (front, api) = Front::start().await;
    let old = Service::new(front.config());
    front.serve(&old);
    let sign_in = login::login(&api, browser).await.unwrap();
    assert_eq!(sign_in.riff_id.as_deref(), Some(old.tokens().riff_id()));
    let person = api.clone().signed_in(None).unwrap();
    person.who(&ada(), false).await.unwrap();

    let new = Service::new(front.config());
    assert_ne!(new.tokens().riff_id(), old.tokens().riff_id());
    front.serve(&new);
    let person = api.clone().signed_in(None).unwrap();
    let error = person.who(&ada(), false).await.unwrap_err();
    assert_eq!(format!("{error:#}"), riff::text::new_riff(api.base()));
    assert_eq!(login::stored(api.base()).unwrap(), None);

    // The next command has no sign-in, and no error about it.
    let next = api.clone().signed_in(None).unwrap();
    next.who(&ada(), false).await.unwrap();
}

/// A restart on the same store keeps the riff ID, and the sign-in stays.
#[tokio::test]
async fn a_restart_on_the_same_store_keeps_the_sign_in() {
    let (front, api) = Front::start().await;
    let store = riff_server::store::Memory::default();
    let old = Service::load(front.config(), Arc::new(store.clone()))
        .await
        .unwrap();
    front.serve(&old);
    let sign_in = login::login(&api, browser).await.unwrap();
    old.save().await.unwrap();

    let new = Service::load(front.config(), Arc::new(store))
        .await
        .unwrap();
    assert_eq!(new.tokens().riff_id(), old.tokens().riff_id());
    front.serve(&new);
    let person = api.clone().signed_in(None).unwrap();
    person.who(&ada(), false).await.unwrap();
    let kept = login::stored(api.base()).unwrap().unwrap();
    assert_eq!(kept.riff_id, sign_in.riff_id);
}

/// A sign-in from before the riff ID counts as old: the next command
/// asks for `riff login` once.
#[tokio::test]
async fn a_sign_in_from_before_the_riff_id_is_old() {
    let (front, api) = Front::start().await;
    let service = Service::new(front.config());
    front.serve(&service);
    let sign_in = login::login(&api, browser).await.unwrap();
    let old = SignIn {
        riff_id: None,
        ..sign_in
    };
    login::store(api.base(), &old).unwrap();

    let person = api.clone().signed_in(None).unwrap();
    let error = person.who(&ada(), false).await.unwrap_err();
    assert_eq!(format!("{error:#}"), riff::text::new_riff(api.base()));
    let next = api.clone().signed_in(None).unwrap();
    next.who(&ada(), false).await.unwrap();
}

/// In "A restart" of the book, the text about a bucket has its own
/// heading, after the heading for a restart with no bucket.
#[test]
fn the_book_keeps_the_bucket_out_of_the_restart_with_no_bucket() {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let no_bucket = page
        .find("\n### After a restart with no bucket, run riff login\n")
        .expect("a heading for a restart with no bucket");
    let rest = &page[no_bucket + 1..];
    let end = rest[4..].find("\n#").map_or(rest.len(), |i| i + 4);
    let part = &rest[..end];
    for text in ["with a bucket", "With a bucket"] {
        assert!(
            !part.contains(text),
            "{text:?} is under the no-bucket heading"
        );
    }
    assert!(
        rest[end..].starts_with("\n### A restart with a bucket\n"),
        "the bucket heading follows the no-bucket heading"
    );
}
