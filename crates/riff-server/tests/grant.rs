//! The session grant over HTTP (01M4CVXJ3GCEB7B4632J7DD84A,
//! 01M4CVXJ5RHHMPE4AYH7KV6E2R): the token endpoint makes a grant for a
//! person token with two proofs, the grant lives through a restart, and
//! only the session key swaps it for a token of its session.

use crate::common;

use std::sync::Arc;
use std::time::Instant;

use riff_core::dpop::Key;
use riff_core::wire::{
    ACCESS_TOKEN_TYPE, GRANT_TOKEN_TYPE, TOKEN_EXCHANGE, TokenError, TokenReply,
};
use riff_server::store::{Memory, SIGN_INS, Store};

/// The form that asks for a grant of `session` on `session_key`, with
/// the person token `person`, at the server `url`.
fn ask(url: &str, person: &str, session: &str, session_key: &Key) -> String {
    let proof = session_key.proof("POST", &format!("{url}/v1/token"), None, now());
    encode(&[
        ("grant_type", TOKEN_EXCHANGE),
        ("subject_token", person),
        ("subject_token_type", ACCESS_TOKEN_TYPE),
        ("session", session),
        ("requested_token_type", GRANT_TOKEN_TYPE),
        ("session_proof", proof.as_str()),
    ])
}

/// The form that swaps `grant` for a session access token.
fn swap(grant: &str) -> String {
    encode(&[
        ("grant_type", TOKEN_EXCHANGE),
        ("subject_token", grant),
        ("subject_token_type", GRANT_TOKEN_TYPE),
    ])
}

/// A form body. Each value here is URL-safe, but for the colons of the
/// token types.
fn encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", v.replace(':', "%3A")))
        .collect::<Vec<_>>()
        .join("&")
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn reply(reply: reqwest::Response) -> Result<TokenReply, String> {
    let status = reply.status();
    let text = reply.text().await.unwrap();
    if status == 200 {
        return Ok(serde_json::from_str(&text).unwrap());
    }
    Err(serde_json::from_str::<TokenError>(&text).unwrap().error)
}

#[tokio::test]
async fn a_grant_acts_only_as_its_session_and_lives_through_a_restart() {
    let store = Memory::default();
    let (service, url) = common::start_on(Arc::new(store.clone())).await;
    let (device, session_key) = (Key::generate(), Key::generate());
    let person = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &device.thumbprint(),
            Instant::now(),
        )
        .unwrap();
    let form = ask(&url, &person.access_token, "a6cf", &session_key);
    let grant = reply(common::refresh(&url, &device, &form).await)
        .await
        .unwrap();
    assert_eq!(grant.token_type, GRANT_TOKEN_TYPE);
    assert_eq!(grant.user, "mike");
    assert!(grant.refresh_token.is_empty());

    // The device key does not swap the grant: only the session key.
    let refused = reply(common::refresh(&url, &device, &swap(&grant.access_token)).await).await;
    assert_eq!(refused.unwrap_err(), "invalid_grant");
    let access = reply(common::refresh(&url, &session_key, &swap(&grant.access_token)).await)
        .await
        .unwrap();
    let who = service
        .tokens()
        .caller(&access.access_token, &session_key.thumbprint(), Instant::now())
        .unwrap();
    assert_eq!(who.to_string(), "mike/a6cf");

    // The server saved the grant before its reply, as a hash.
    let saved = store.load(SIGN_INS).await.unwrap().unwrap();
    let text = String::from_utf8_lossy(&saved.bytes);
    assert!(text.contains("a6cf"), "{text}");
    assert!(!text.contains(&grant.access_token), "the grant is a hash");

    service.shutdown().await.unwrap();
    drop(service);
    let (restarted, url) = common::start_on(Arc::new(store)).await;
    let again = reply(common::refresh(&url, &session_key, &swap(&grant.access_token)).await)
        .await
        .unwrap();
    let who = restarted
        .tokens()
        .caller(&again.access_token, &session_key.thumbprint(), Instant::now())
        .unwrap();
    assert_eq!(who.to_string(), "mike/a6cf");
}

#[tokio::test]
async fn a_grant_needs_a_person_token_and_a_new_key() {
    let (service, url) = common::start(false, &[]).await;
    let (device, session_key) = (Key::generate(), Key::generate());
    let person = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &device.thumbprint(),
            Instant::now(),
        )
        .unwrap();

    // The session key must not be the device key.
    let same = ask(&url, &person.access_token, "a6cf", &device);
    let refused = reply(common::refresh(&url, &device, &same).await).await;
    assert_eq!(refused.unwrap_err(), "invalid_dpop_proof");

    // No proof of the session key, no grant.
    let no_proof = ask(&url, &person.access_token, "a6cf", &session_key)
        .split('&')
        .filter(|p| !p.starts_with("session_proof="))
        .collect::<Vec<_>>()
        .join("&");
    let refused = reply(common::refresh(&url, &device, &no_proof).await).await;
    assert_eq!(refused.unwrap_err(), "invalid_request");

    // A token of a grant gives no grant.
    let form = ask(&url, &person.access_token, "a6cf", &session_key);
    let grant = reply(common::refresh(&url, &device, &form).await)
        .await
        .unwrap();
    let access = reply(common::refresh(&url, &session_key, &swap(&grant.access_token)).await)
        .await
        .unwrap();
    let other = Key::generate();
    let form = ask(&url, &access.access_token, "b7d0", &other);
    let refused = reply(common::refresh(&url, &session_key, &form).await).await;
    assert_eq!(refused.unwrap_err(), "invalid_grant");

    // A grant that the server does not know.
    let refused = reply(common::refresh(&url, &session_key, &swap("g9.nope")).await).await;
    assert_eq!(refused.unwrap_err(), "invalid_grant");
}
