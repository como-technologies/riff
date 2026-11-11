//! The keys of a person over HTTP (01M75NGT9Z0NSZTG97TBAMP7RS,
//! 01M75NH2P3QHCYDKQH60KGSF97, 01M75NH2RSEJ3K618M17PCH3ZZ,
//! 01M75NH2VF6HGZVY04GB358JG0, 01M75NH2Y3H1CR10CGRWNGT8RG,
//! 01M75NH30PNATYN33MXZQMVBW8, 01M75NH33CVGBVASSACBPXRR2G,
//! 01M75NH3625QATGDT6DK652ZK6): the add in the page of the browser with
//! the reply of a fake authenticator, the list, the removal, and each
//! refusal.

use crate::common;

use std::sync::Arc;
use std::time::Duration;

use riff_core::device::fake::{Authenticator, Shape};
use riff_core::dpop::Key;
use riff_core::wire::{
    DeviceInfo, DeviceKind, DevicesReply, ID_TOKEN_TYPE, KeyOptions, KeyReplied, KeyReply,
    KeyStarted, KeyState, TOKEN_EXCHANGE, TokenReply, TokenRequest,
};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::{DEFAULT_DOMAIN, Provider};
use riff_state::store::{Memory, Store};
use serde_json::{Value, json};

/// Serves a riff with sign-in on a free port. With a store, it loads
/// from it.
async fn serve(issuer: &str, store: Option<Arc<dyn Store>>) -> (Service, String) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        provider: Some(Provider {
            issuer: issuer.into(),
            client_id: "riff-client".into(),
            client_secret: None,
            allowed_domains: vec![DEFAULT_DOMAIN.into()],
        }),
        lease: common::LEASE,
        save_every: common::SAVE_EVERY,
        ..Config::new(&url)
    };
    let service = match store {
        Some(store) => Service::load(config, store).await.unwrap(),
        None => Service::new(config),
    };
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// A person who signed in: the key of the device and the tokens.
struct Person {
    key: Key,
    token: String,
}

async fn sign_in(url: &str, issuer: &str, email: &str) -> Person {
    let key = Key::generate();
    let form = TokenRequest {
        grant_type: TOKEN_EXCHANGE.into(),
        subject_token: Some(common::id_token(issuer, email, Some(DEFAULT_DOMAIN))),
        subject_token_type: Some(ID_TOKEN_TYPE.into()),
        ..TokenRequest::default()
    };
    let reply = common::post(&format!("{url}/v1/token"), &key, None)
        .form(&form)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200);
    let pair: TokenReply = reply.json().await.unwrap();
    Person {
        key,
        token: pair.access_token,
    }
}

/// The token of the session `session` of `person`.
async fn session_of(url: &str, person: &Person, session: &str) -> Person {
    let form = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
         &subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token\
         &subject_token={}&session={session}",
        person.token
    );
    let reply = common::refresh(url, &person.key, &form).await;
    assert_eq!(reply.status(), 200);
    let pair: TokenReply = reply.json().await.unwrap();
    Person {
        key: person.key.clone(),
        token: pair.access_token,
    }
}

/// A call of `person` to a route of the devices: the status and the
/// body.
async fn call(url: &str, person: &Person, path: &str, body: Value) -> (u16, String) {
    let reply = common::post(&format!("{url}{path}"), &person.key, Some(&person.token))
        .json(&body)
        .send()
        .await
        .unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

/// A call of the page of the add, with no token: the status and the
/// body.
async fn page_call(url: &str, path: &str, body: &impl serde::Serialize) -> (u16, String) {
    let reply = common::client()
        .post(format!("{url}{path}"))
        .json(body)
        .send()
        .await
        .unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

/// The start of an add of the key `name`: its page and its ticket.
async fn start(url: &str, person: &Person, name: &str) -> KeyStarted {
    let (status, body) = call(url, person, "/v1/device/key", json!({ "name": name })).await;
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

/// The options that the page of `started` reads.
async fn options(url: &str, started: &KeyStarted) -> KeyOptions {
    let (status, body) = page_call(
        url,
        "/device/key/options",
        &json!({ "ticket": started.ticket }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

/// The origin of the page of `started`: its URL up to the path.
fn origin(started: &KeyStarted) -> String {
    started.url.split("/device/key").next().unwrap().to_owned()
}

/// The reply of `key` in the shape `shape` for the add of `started`,
/// as the page sends it: the status and the body.
async fn reply(
    url: &str,
    key: &Authenticator,
    started: &KeyStarted,
    shape: Shape,
) -> (u16, String) {
    let options = options(url, started).await;
    let reply: KeyReply = key.reply_as(&started.ticket, &options, &origin(started), shape);
    page_call(url, "/device/key/reply", &reply).await
}

async fn wait(url: &str, person: &Person, started: &KeyStarted) -> KeyState {
    let (status, body) = call(
        url,
        person,
        "/v1/device/key/wait",
        json!({ "ticket": started.ticket }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

async fn devices(url: &str, person: &Person) -> Vec<DeviceInfo> {
    let (status, body) = call(url, person, "/v1/devices", json!({})).await;
    assert_eq!(status, 200, "{body}");
    serde_json::from_str::<DevicesReply>(&body).unwrap().devices
}

fn names(devices: &[DeviceInfo]) -> Vec<&str> {
    devices.iter().map(|d| d.name.as_str()).collect()
}

/// A person adds a key in the page with the reply of a fake
/// authenticator, lists it and removes it. The page is on `localhost`
/// for a riff on a loopback address, and its ticket is in its fragment.
#[tokio::test]
async fn a_person_adds_lists_and_removes_a_key() {
    let issuer = common::fake_provider().await;
    let (_service, url) = serve(&issuer, None).await;
    let mike = sign_in(&url, &issuer, "mike@comotechnologies.io").await;

    let started = start(&url, &mike, "yubikey").await;
    let port = url.rsplit(':').next().unwrap();
    assert_eq!(
        started.url,
        format!("http://localhost:{port}/device/key#{}", started.ticket)
    );
    assert_eq!(wait(&url, &mike, &started).await, KeyState::Waiting);

    // The page is plain HTML, with no token and no ticket.
    let page = common::client()
        .get(format!("{url}/device/key"))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    assert_eq!(page.headers()["cache-control"], "no-store");
    let page = page.text().await.unwrap();
    assert!(page.contains("navigator.credentials.create"), "{page}");

    let options = options(&url, &started).await;
    assert_eq!(options.rp.id, "localhost");
    assert_eq!(options.user.name, "mike");
    assert_eq!(options.name, "yubikey");
    assert_eq!(options.attestation, "none");
    assert!(options.exclude_credentials.is_empty());

    let key = Authenticator::new();
    let (status, body) = reply(&url, &key, &started, Shape::Packed).await;
    assert_eq!(status, 200, "{body}");
    let replied: KeyReplied = serde_json::from_str(&body).unwrap();
    assert!(
        replied.text.contains("riff added the key yubikey"),
        "{body}"
    );
    assert_eq!(
        wait(&url, &mike, &started).await,
        KeyState::Added {
            name: "yubikey".into()
        }
    );

    let list = devices(&url, &mike).await;
    assert_eq!(names(&list), ["yubikey"]);
    assert_eq!(list[0].kind, DeviceKind::Key);
    assert!(list[0].added_ms > 0);
    assert_eq!(list[0].added_ms, list[0].used_ms);

    // A second key, with no attestation: two keys, so a lost one locks
    // nobody out.
    let second = start(&url, &mike, "phone").await;
    let (status, body) = reply(&url, &Authenticator::new(), &second, Shape::None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(names(&devices(&url, &mike).await), ["phone", "yubikey"]);

    let (status, body) = call(&url, &mike, "/v1/device/remove", json!({"name": "yubikey"})).await;
    assert_eq!(status, 200, "{body}");
    let removed: DeviceInfo = serde_json::from_str(&body).unwrap();
    assert_eq!(removed.name, "yubikey");
    assert_eq!(names(&devices(&url, &mike).await), ["phone"]);
    let (status, body) = call(&url, &mike, "/v1/device/remove", json!({"name": "yubikey"})).await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body.contains("you have no device with the name yubikey"),
        "{body}"
    );
}

/// A reply with a bad signature, or with a challenge that riff-server
/// did not give, is refused: riff adds no key, the wait says why, and
/// the ticket takes no second reply.
#[tokio::test]
async fn a_bad_signature_or_a_challenge_that_riff_did_not_give_is_refused() {
    let issuer = common::fake_provider().await;
    let (_service, url) = serve(&issuer, None).await;
    let mike = sign_in(&url, &issuer, "mike@comotechnologies.io").await;
    let key = Authenticator::new();
    for (shape, part) in [
        (
            Shape::BadSignature,
            "the signature of the reply does not check",
        ),
        (
            Shape::OtherChallenge,
            "a challenge that riff-server did not give",
        ),
        (Shape::OtherOrigin, "not from the page of riff-server"),
        (Shape::Form("tpm"), "riff takes only none and packed"),
    ] {
        let started = start(&url, &mike, "yubikey").await;
        let (status, body) = reply(&url, &key, &started, shape).await;
        assert_eq!(status, 400, "{shape:?}: {body}");
        assert!(body.contains(part), "{shape:?}: {body}");
        match wait(&url, &mike, &started).await {
            KeyState::Failed { reason } => assert!(reason.contains(part), "{reason}"),
            other => panic!("{shape:?}: {other:?}"),
        }
        // One reply for each add.
        let good = key.reply(&started.ticket, &options_now(), &origin(&started));
        let (status, body) = page_call(&url, "/device/key/reply", &good).await;
        assert_eq!(status, 400, "{body}");
        assert!(body.contains("has its reply already"), "{body}");
    }
    assert!(devices(&url, &mike).await.is_empty());

    // A ticket that riff-server did not give.
    let (status, body) =
        page_call(&url, "/device/key/options", &json!({"ticket": "made-up"})).await;
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("knows no such add"), "{body}");
}

/// Any options will do for a reply that riff refuses before its check.
fn options_now() -> KeyOptions {
    riff_core::device::fake::options("localhost", "c2VlZA")
}

/// The options name each key of the person, so that an authenticator
/// makes no second key. A credential that a person of the riff has is
/// refused, and so is a name of another key of the person.
#[tokio::test]
async fn one_authenticator_gives_one_key_and_one_name_one_key() {
    let issuer = common::fake_provider().await;
    let (_service, url) = serve(&issuer, None).await;
    let mike = sign_in(&url, &issuer, "mike@comotechnologies.io").await;
    let bob = sign_in(&url, &issuer, "bob@comotechnologies.io").await;
    let key = Authenticator::new();
    let started = start(&url, &mike, "yubikey").await;
    assert_eq!(reply(&url, &key, &started, Shape::Packed).await.0, 200);

    let again = start(&url, &mike, "again").await;
    let options = options(&url, &again).await;
    assert_eq!(options.exclude_credentials.len(), 1);
    assert_eq!(options.exclude_credentials[0].id, key.credential());
    let (status, body) = reply(&url, &key, &again, Shape::Packed).await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body.contains("riff knows this authenticator already"),
        "{body}"
    );
    // The key of another person too.
    let of_bob = start(&url, &bob, "yubikey").await;
    let (status, body) = reply(&url, &key, &of_bob, Shape::Packed).await;
    assert_eq!(status, 400, "{body}");

    // The start refuses a name that the person has, and a bad name.
    let (status, body) = call(&url, &mike, "/v1/device/key", json!({"name": "yubikey"})).await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body.contains("you have a device with the name yubikey"),
        "{body}"
    );
    let (status, body) = call(&url, &mike, "/v1/device/key", json!({"name": "my key"})).await;
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("not allowed"), "{body}");
}

/// Only a person token adds, lists and removes: a session token of the
/// same person gets 403. The wait of an add tells only its person.
#[tokio::test]
async fn a_session_token_cannot_add_list_or_remove_a_device() {
    let issuer = common::fake_provider().await;
    let (_service, url) = serve(&issuer, None).await;
    let mike = sign_in(&url, &issuer, "mike@comotechnologies.io").await;
    let session = session_of(&url, &mike, "a1").await;
    for (path, body) in [
        ("/v1/device/key", json!({"name": "yubikey"})),
        ("/v1/devices", json!({})),
        ("/v1/device/remove", json!({"name": "yubikey"})),
        ("/v1/device/key/wait", json!({"ticket": "t"})),
    ] {
        let (status, text) = call(&url, &session, path, body).await;
        assert_eq!(status, 403, "{path}: {text}");
        assert!(text.contains("session"), "{path}: {text}");
    }
    // With no token at all: 401.
    let (status, _) = page_call(&url, "/v1/devices", &json!({})).await;
    assert_eq!(status, 401);

    let started = start(&url, &mike, "yubikey").await;
    let bob = sign_in(&url, &issuer, "bob@comotechnologies.io").await;
    let (status, body) = call(
        &url,
        &bob,
        "/v1/device/key/wait",
        json!({"ticket": started.ticket}),
    )
    .await;
    assert_eq!(status, 404, "{body}");
}

/// The keys are in the log: they stay after a restart on the same
/// store, with their times.
#[tokio::test]
async fn the_keys_stay_after_a_restart() {
    let issuer = common::fake_provider().await;
    let store = Memory::default();
    let (old, url) = serve(&issuer, Some(Arc::new(store.clone()))).await;
    let mike = sign_in(&url, &issuer, "mike@comotechnologies.io").await;
    let started = start(&url, &mike, "yubikey").await;
    assert_eq!(
        reply(&url, &Authenticator::new(), &started, Shape::Packed)
            .await
            .0,
        200
    );
    let before = devices(&url, &mike).await;
    old.save().await.unwrap();

    let (new, url) = serve(&issuer, Some(Arc::new(store))).await;
    isolated::in_time(Duration::from_secs(5), old.stopped())
        .await
        .unwrap();
    let mike = sign_in(&url, &issuer, "mike@comotechnologies.io").await;
    assert_eq!(devices(&url, &mike).await, before);
    drop(new);
}
