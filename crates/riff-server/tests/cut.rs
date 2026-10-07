//! The sign-ins after a log that went back to an earlier position
//! (01M3XGNZYD1E35DXYTHHJT1CR7): a start drops each sign-in that the
//! log does not hold, so a later removal ends each sign-in of its
//! person.

use crate::common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use riff_core::dpop::Key;
use riff_core::record::Change;
use riff_core::wire::{
    ID_TOKEN_TYPE, Invite, Remove, Removed, Revoke, TOKEN_EXCHANGE, TokenError, TokenReply,
    TokenRequest,
};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::Provider;
use riff_server::store::{Memory, Store};
use riff_server::token::Refused;
use riff_server::tools::{Mode, cut_with};

fn config(url: &str, issuer: &str) -> Config {
    Config {
        provider: Some(Provider {
            issuer: issuer.into(),
            client_id: "riff-client".into(),
            client_secret: None,
            allowed_domains: Vec::new(),
        }),
        lease: common::LEASE,
        save_every: common::SAVE_EVERY,
        ..Config::new(url)
    }
}

/// Serves a riff with sign-in on `store`, on a free port.
async fn serve(issuer: &str, store: Arc<dyn Store>) -> (Service, String) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = Service::load(config(&url, issuer), store).await.unwrap();
    service.save().await.unwrap();
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// A person who signed in: the key of the device and the tokens.
struct Person {
    key: Key,
    pair: TokenReply,
}

async fn sign_in(url: &str, issuer: &str, email: &str) -> Result<Person, TokenError> {
    let key = Key::generate();
    let form = TokenRequest {
        grant_type: TOKEN_EXCHANGE.into(),
        subject_token: Some(common::id_token(issuer, email, None)),
        subject_token_type: Some(ID_TOKEN_TYPE.into()),
        ..TokenRequest::default()
    };
    let reply = common::post(&format!("{url}/v1/token"), &key, None)
        .form(&form)
        .send()
        .await
        .unwrap();
    if reply.status() == 200 {
        Ok(Person {
            key,
            pair: reply.json().await.unwrap(),
        })
    } else {
        Err(reply.json().await.unwrap())
    }
}

async fn call(url: &str, person: &Person, op: &str, body: serde_json::Value) -> (u16, String) {
    let reply = common::post(
        &format!("{url}/v1/{op}"),
        &person.key,
        Some(&person.pair.access_token),
    )
    .json(&body)
    .send()
    .await
    .unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

async fn invite(url: &str, by: &Person, email: &str) {
    let body = serde_json::to_value(Invite {
        email: email.into(),
    })
    .unwrap();
    let (status, text) = call(url, by, "invite", body).await;
    assert_eq!(status, 200, "{text}");
}

/// Given a person with two sign-ins, and a cut of the log between them.
/// When the server starts again, it drops the sign-in from after the
/// new end of the log. Then a removal of the person ends the other
/// one, and no sign-in of the person gives a token.
#[tokio::test]
async fn a_removal_after_a_cut_ends_each_sign_in_of_the_person() {
    let issuer = common::fake_provider().await;
    let store = Memory::default();
    let shared: Arc<dyn Store> = Arc::new(store.clone());
    let (old, url) = serve(&issuer, shared.clone()).await;
    let ada = sign_in(&url, &issuer, "ada@gmail.com").await.unwrap();
    invite(&url, &ada, "bob@gmail.com").await;
    let laptop = sign_in(&url, &issuer, "bob@gmail.com").await.unwrap();
    // The log goes on, and bob signs in on a second device.
    invite(&url, &ada, "carol@gmail.com").await;
    invite(&url, &ada, "dan@gmail.com").await;
    let desktop = sign_in(&url, &issuer, "bob@gmail.com").await.unwrap();
    old.save().await.unwrap();
    old.shutdown().await.unwrap();

    // The position of the record that made bob a person.
    let log = riff_server::log::replay_after(&store, 0).await.unwrap();
    let joined = log
        .records
        .iter()
        .find(|r| matches!(&r.change, Change::PersonJoined(p) if p.user == "bob"))
        .unwrap()
        .envelope
        .position;
    assert!(log.records.last().unwrap().envelope.position > joined + 1);

    // A cut of the log after that record: the two invites go.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let cut = cut_with(&store, joined, Mode::Remove, &common::LEASE)
        .await
        .unwrap();
    assert_eq!(cut.last, joined);

    // A new server on the store. Bob is a member. The sign-in of the
    // desktop started after the new end of the log: the start drops it.
    let (new, url) = serve(&issuer, shared).await;
    assert_eq!(new.members().members, ["bob@gmail.com"]);
    let now = Instant::now();
    assert_eq!(
        new.tokens().keys("bob", now),
        [laptop.key.thumbprint()],
        "only the sign-in from before the new end of the log stays"
    );
    let refresh = |device: &Person| {
        let key = device.key.thumbprint();
        new.tokens().refresh(&device.pair.refresh_token, &key, now)
    };
    assert_eq!(
        refresh(&desktop).map(|pair| pair.user),
        Err(Refused::Unknown)
    );

    // The owner signs in again, and removes bob.
    let ada = sign_in(&url, &issuer, "ada@gmail.com").await.unwrap();
    let body = serde_json::to_value(Remove {
        email: "bob@gmail.com".into(),
    })
    .unwrap();
    let (status, text) = call(&url, &ada, "remove", body).await;
    assert_eq!(status, 200, "{text}");
    let removed: Removed = serde_json::from_str(&text).unwrap();
    assert_eq!(removed.sign_ins, 1);
    let body = serde_json::to_value(Revoke {
        user: Some("bob".into()),
    })
    .unwrap();
    let (status, text) = call(&url, &ada, "revoke", body).await;
    assert_eq!(status, 200, "{text}");

    // No sign-in of bob gives a token.
    assert!(new.tokens().keys("bob", now).is_empty());
    for device in [&laptop, &desktop] {
        assert_eq!(refresh(device).map(|pair| pair.user), Err(Refused::Unknown));
    }
}

/// A sign-in of a person that the log does not know is dropped at a
/// start: the cut removed the record that made the person.
#[tokio::test]
async fn a_start_drops_the_sign_in_of_a_person_that_the_log_does_not_know() {
    let issuer = common::fake_provider().await;
    let store = Memory::default();
    let shared: Arc<dyn Store> = Arc::new(store.clone());
    let (old, url) = serve(&issuer, shared.clone()).await;
    let ada = sign_in(&url, &issuer, "ada@gmail.com").await.unwrap();
    old.save().await.unwrap();
    let end = riff_server::log::replay_after(&store, 0)
        .await
        .unwrap()
        .last;
    invite(&url, &ada, "bob@gmail.com").await;
    let bob = sign_in(&url, &issuer, "bob@gmail.com").await.unwrap();
    old.save().await.unwrap();
    old.shutdown().await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    cut_with(&store, end, Mode::Remove, &common::LEASE)
        .await
        .unwrap();
    let (new, url) = serve(&issuer, shared).await;
    let now = Instant::now();
    assert!(new.tokens().keys("bob", now).is_empty());
    assert_eq!(new.tokens().keys("ada", now).len(), 1);
    assert!(new.members().members.is_empty());
    // The person is no member now, and does not sign in.
    let refused = sign_in(&url, &issuer, "bob@gmail.com").await.err().unwrap();
    assert_eq!(refused.error, "access_denied");
    let key = bob.key.thumbprint();
    let refresh = new.tokens().refresh(&bob.pair.refresh_token, &key, now);
    assert_eq!(refresh.map(|pair| pair.user), Err(Refused::Unknown));
}
