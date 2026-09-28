//! `riff invite`, `riff remove` and `riff members` against a real
//! server (01M3JN3AHMK532XMRDASD4XD5D). The sign-in is in the mock
//! store of `keyring-core`, so the tests run in process.

use std::sync::Once;
use std::time::Instant;

use riff::api::Api;
use riff::login::{self, SignIn};
use riff::text;
use riff_core::dpop::Key;
use riff_core::wire::TokenReply;
use riff_server::Service;
use riff_server::auth::Config;

static MOCK_KEYRING: Once = Once::new();

async fn start() -> (Service, Api) {
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = Service::new(Config::new(&url));
    let api = Api::new(&url);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, api)
}

/// Signs in `email` on this device, as the provider sign-in does.
fn sign_in(service: &Service, api: &Api, email: &str) -> TokenReply {
    let jkt = riff::device::key(api.base()).unwrap().thumbprint();
    let pair = service
        .tokens()
        .admit(email, false, &[], &jkt, Instant::now())
        .unwrap();
    let sign_in = SignIn {
        user: pair.user.clone(),
        access_token: pair.access_token.clone(),
        refresh_token: pair.refresh_token.clone(),
        expires_at: u64::MAX,
        riff_id: None,
    };
    login::store(api.base(), &sign_in).unwrap();
    pair
}

#[tokio::test]
async fn the_owner_invites_lists_and_removes() {
    let (service, api) = start().await;
    sign_in(&service, &api, "ada@gmail.com");
    let signed_in = api.clone().signed_in(None).unwrap();

    let invited = signed_in.invite("bob@gmail.com").await.unwrap();
    assert_eq!(
        text::invited(&invited),
        "Invited bob@gmail.com. They can now run riff login."
    );
    let bob_key = Key::generate().thumbprint();
    let bob = service
        .tokens()
        .admit("bob@gmail.com", false, &[], &bob_key, Instant::now())
        .unwrap();

    let list = signed_in.members().await.unwrap();
    assert_eq!(
        text::members(&list),
        "owner: ada@gmail.com\nadmins: none\nmembers: bob@gmail.com\nallowed domains: none"
    );

    let removed = signed_in.remove("bob@gmail.com").await.unwrap();
    assert_eq!(
        text::removed(&removed),
        "Removed bob@gmail.com and ended 1 sign-in."
    );
    let check = service
        .tokens()
        .check(&bob.access_token, &bob_key, Instant::now());
    assert!(check.is_err());
}

#[tokio::test]
async fn a_member_cannot_invite() {
    let (service, api) = start().await;
    let owner_key = Key::generate().thumbprint();
    service
        .tokens()
        .admit("ada@gmail.com", false, &[], &owner_key, Instant::now())
        .unwrap();
    service.tokens().invite("bob@gmail.com").unwrap();
    sign_in(&service, &api, "bob@gmail.com");
    let error = api
        .clone()
        .signed_in(None)
        .unwrap()
        .invite("carol@gmail.com")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not an admin"), "{error}");
}

/// This server has no sign-in provider, so nobody can sign in (R227).
#[tokio::test]
async fn members_needs_a_sign_in() {
    let (_, api) = start().await;
    let error = api.members().await.unwrap_err();
    assert_eq!(error.to_string(), text::nobody_signs_in(api.base()));
}
