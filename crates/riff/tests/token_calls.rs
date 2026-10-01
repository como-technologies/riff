//! A refused refresh is not sent again (#348,
//! 01M3W947QF6PFBWR28ZVXCVQHG): a real `riff-server` that counts the
//! replies of `/v1/token`, a fake provider, and the mock keyring of
//! `keyring-core`.

mod common;

use std::sync::{Arc, Mutex, Once};

use common::{CLIENT, FakeProvider, browser};
use riff::api::Api;
use riff::login;
use riff_core::name::SessionUri;
use riff_server::Service;
use riff_server::auth::{Config, TOKEN_PATH};
use riff_server::oidc::{DEFAULT_DOMAIN, Provider};

static MOCK_KEYRING: Once = Once::new();

/// The status of each reply of `/v1/token`, in order.
type Replies = Arc<Mutex<Vec<u16>>>;

/// A server with sign-in that needs a token for each call, a fake
/// provider that signs in Ada, and the replies of its token endpoint.
async fn start() -> (Service, Api, Replies) {
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let issuer = FakeProvider::start("Ada@comotechnologies.io", Some(DEFAULT_DOMAIN))
        .await
        .issuer;
    let service = Service::new(Config {
        require_sign_in: true,
        provider: Some(Provider {
            issuer,
            client_id: CLIENT.into(),
            client_secret: None,
            allowed_domains: vec![DEFAULT_DOMAIN.into()],
        }),
        ..Config::new(&url)
    });
    let replies = Replies::default();
    let seen = replies.clone();
    let router = service.router().layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let seen = seen.clone();
            async move {
                let token = request.uri().path() == TOKEN_PATH;
                let response = next.run(request).await;
                if token {
                    seen.lock().unwrap().push(response.status().as_u16());
                }
                response
            }
        },
    ));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, Api::new(&url), replies)
}

/// The replies of the token endpoint since the last call, and none
/// after it.
fn take(replies: &Replies) -> Vec<u16> {
    std::mem::take(&mut replies.lock().unwrap())
}

fn session(id: &str) -> SessionUri {
    format!("riff://ada@heron/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

/// After the sign-in ends, a running session sends each refused grant
/// one time: its session refresh, the swap of the person token, and
/// the person refresh. Each later call fails with no token call. After
/// `riff login`, the same client goes on.
#[tokio::test]
async fn a_session_does_not_repeat_a_refused_refresh() {
    let (service, api, replies) = start().await;
    login::login(&api, browser).await.unwrap();
    let me = session("t1");
    let client = api.clone().signed_in(Some("t1")).unwrap();
    client.register(&me).await.unwrap();

    service.tokens().revoke_user("ada");
    take(&replies);
    let error = client.who(&me, false).await.unwrap_err();
    assert!(format!("{error:#}").contains(login::ENDED), "{error:#}");
    assert_eq!(take(&replies), [400, 400, 400]);
    assert!(login::stored(api.base()).unwrap().unwrap().ended());

    for _ in 0..10 {
        let error = client.who(&me, false).await.unwrap_err();
        assert!(format!("{error:#}").contains(login::ENDED), "{error:#}");
    }
    assert_eq!(take(&replies), [0u16; 0]);

    // A new process of the machine sends no token call.
    let other = api.clone().signed_in(Some("t2")).unwrap();
    let error = other.register(&session("t2")).await.unwrap_err();
    assert!(format!("{error:#}").contains(login::ENDED), "{error:#}");
    assert_eq!(take(&replies), [0u16; 0]);

    login::login(&api, browser).await.unwrap();
    client.who(&me, false).await.unwrap();
    assert!(take(&replies).iter().all(|status| *status == 200));
}

/// After the sign-in ends, a person client sends the refused refresh
/// one time. Each later call fails with no token call.
#[tokio::test]
async fn a_person_does_not_repeat_a_refused_refresh() {
    let (service, api, replies) = start().await;
    login::login(&api, browser).await.unwrap();
    let person = api.clone().signed_in(None).unwrap();
    person.members().await.unwrap();

    service.tokens().revoke_user("ada");
    take(&replies);
    let error = person.members().await.unwrap_err();
    assert!(format!("{error:#}").contains(login::ENDED), "{error:#}");
    assert_eq!(take(&replies), [400]);

    for _ in 0..10 {
        let error = person.members().await.unwrap_err();
        assert!(format!("{error:#}").contains(login::ENDED), "{error:#}");
    }
    let error = login::access_token(&api).await.unwrap_err();
    assert_eq!(error.to_string(), login::ENDED);
    assert_eq!(take(&replies), [0u16; 0]);

    // The sign-in keeps its user, so `riff` still knows who the person is.
    assert_eq!(login::user(api.base()).unwrap().as_deref(), Some("ada"));

    login::login(&api, browser).await.unwrap();
    person.members().await.unwrap();
}

/// A refresh that fails for another reason keeps the sign-in: the
/// client sends it again.
#[tokio::test]
async fn an_error_that_is_no_refusal_keeps_the_sign_in() {
    let (_service, api, _replies) = start().await;
    let sign_in = login::login(&api, browser).await.unwrap();
    let expired = login::SignIn {
        expires_at: 0,
        ..sign_in
    };
    // No server listens at port 9.
    let down = "http://127.0.0.1:9";
    login::store(down, &expired).unwrap();
    login::access_token(&Api::new(down)).await.unwrap_err();
    assert_eq!(login::stored(down).unwrap(), Some(expired));
}

/// `riff connect claude` signs in again when the sign-in ended.
#[tokio::test]
async fn ensure_signs_in_again_after_the_sign_in_ended() {
    let (service, api, _replies) = start().await;
    login::login(&api, browser).await.unwrap();
    assert_eq!(login::ensure(&api, browser).await.unwrap(), None);

    service.tokens().revoke_user("ada");
    let person = api.clone().signed_in(None).unwrap();
    person.members().await.unwrap_err();
    assert!(login::stored(api.base()).unwrap().unwrap().ended());

    let fresh = login::ensure(&api, browser).await.unwrap().unwrap();
    assert!(!fresh.ended());
    person.members().await.unwrap();
}
