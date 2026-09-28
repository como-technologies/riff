//! `riff login` end to end: a fake provider, a real `riff-server`, and
//! a fake browser that follows the redirects. The keyring is the mock
//! store of `keyring-core`.

mod common;

use std::sync::{Arc, Mutex, Once};
use std::time::Instant;

use axum::Router;
use common::{CLIENT, FakeProvider, browser};
use riff::api::Api;
use riff::login::{self, SignIn};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::{DEFAULT_DOMAIN, Provider};

static MOCK_KEYRING: Once = Once::new();

fn mock_keyring() {
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
}

/// A fake provider that signs in Ada, and its issuer.
async fn fake_provider() -> String {
    FakeProvider::start("Ada@comotechnologies.io", Some(DEFAULT_DOMAIN))
        .await
        .issuer
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

/// A kept sign-in with an expired access token, so that the next
/// [`login::access_token`] refreshes it.
fn expired(sign_in: SignIn) -> SignIn {
    SignIn {
        expires_at: 0,
        ..sign_in
    }
}

/// Refreshes the kept sign-in twice, and returns the first sign-in: its
/// refresh token was used before the last refresh.
async fn refresh_twice(api: &Api) -> SignIn {
    let first = login::login(api, browser).await.unwrap();
    login::store(api.base(), &expired(first.clone())).unwrap();
    login::access_token(api).await.unwrap();
    let second = login::stored(api.base()).unwrap().unwrap();
    login::store(api.base(), &expired(second)).unwrap();
    login::access_token(api).await.unwrap();
    first
}

#[tokio::test]
async fn a_used_refresh_token_ends_the_sign_in() {
    let (_, api) = start().await;
    let first = refresh_twice(&api).await;

    // Someone replays the old refresh token after the next refresh.
    login::store(api.base(), &expired(first)).unwrap();
    let error = login::access_token(&api).await.unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        format!(
            "{}: riff-server refused the token request: invalid_grant",
            login::ENDED
        )
    );
}

/// The same refresh token again before the next refresh is a lost reply:
/// the sign-in stays (01M3MX4TG7PNNETZ986DQS10JJ).
#[tokio::test]
async fn a_refresh_token_again_before_the_next_refresh_keeps_the_sign_in() {
    let (service, api) = start().await;
    let first = login::login(&api, browser).await.unwrap();
    login::store(api.base(), &expired(first.clone())).unwrap();
    login::access_token(&api).await.unwrap();
    // The reply of that refresh got lost: the keyring holds the old pair.
    login::store(api.base(), &expired(first)).unwrap();
    let fresh = login::access_token(&api).await.unwrap();
    assert_eq!(
        service.tokens().check(&fresh, &jkt(&api), Instant::now()),
        Ok("ada".to_owned())
    );
}

/// At a riff with sign-in, an ended sign-in still says `riff login`
/// (R226).
#[tokio::test]
async fn a_call_with_an_ended_sign_in_says_to_run_riff_login() {
    let (_, api) = start().await;
    let first = refresh_twice(&api).await;
    login::store(api.base(), &expired(first)).unwrap();

    assert!(api.has_sign_in().await.unwrap());
    let me = "riff://ada@pangolin/como-technologies/riff"
        .parse()
        .unwrap();
    let person = api.clone().signed_in(None).unwrap();
    let error = person.who(&me, false).await.unwrap_err();
    assert!(format!("{error:#}").contains("run riff login"), "{error:#}");
}

/// `riff connect claude` signs in only at a riff with sign-in, and only
/// when this machine has no sign-in there (01M3JZN1ZZED3FXQEFNJ4KVCN5).
#[tokio::test]
async fn ensure_signs_in_only_when_needed() {
    let (_, api) = start().await;
    let first = login::ensure(&api, browser).await.unwrap();
    assert_eq!(first.map(|s| s.user).as_deref(), Some("ada"));
    let again = login::ensure(&api, |_| panic!("no browser")).await.unwrap();
    assert_eq!(again, None);

    // A riff with no sign-in needs none.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let open = Api::new(&format!("http://{}", listener.local_addr().unwrap()));
    let router = Service::default().router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let none = login::ensure(&open, |_| panic!("no browser"))
        .await
        .unwrap();
    assert_eq!(none, None);
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

/// After a new riff at the same URL, `riff connect claude` signs in
/// again: the old sign-in does not count.
#[tokio::test]
async fn ensure_signs_in_again_at_a_new_riff() {
    let (front, api) = Front::start().await;
    let old = Service::new(front.config());
    front.serve(&old);
    login::ensure(&api, browser).await.unwrap().unwrap();
    let new = Service::new(front.config());
    front.serve(&new);
    let again = login::ensure(&api, browser).await.unwrap().unwrap();
    assert_eq!(again.riff_id.as_deref(), Some(new.tokens().riff_id()));
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
