//! A token for each session (R19, R103-R105), over HTTP.

mod common;

use std::time::Instant;

use riff_core::dpop::Key;
use riff_core::wire::TokenReply;
use serde_json::json;

const A: &str = "riff://mike@pangolin/como-technologies/riff?session=a";
const B: &str = "riff://mike@pangolin/como-technologies/riff?session=b";
const MIKE: &str = "riff://mike@pangolin";

/// Swaps a person access token for a session pair.
async fn for_session(base: &str, key: &Key, token: &str, session: &str) -> reqwest::Response {
    let form = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
         &subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token\
         &subject_token={token}&session={session}"
    );
    common::refresh(base, key, &form).await
}

/// Registers `me` with `token`. Returns the status.
async fn register(base: &str, key: &Key, token: &str, me: &str) -> u16 {
    common::post(&format!("{base}/v1/register"), key, Some(token))
        .json(&json!({ "me": me }))
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[tokio::test]
async fn a_session_token_cannot_act_as_another_session() {
    let (service, base) = common::start(true, &[]).await;
    let key = Key::generate();
    let person = service
        .tokens()
        .sign_in("mike", &key.thumbprint(), Instant::now())
        .unwrap();
    let reply = for_session(&base, &key, &person.access_token, "a").await;
    assert_eq!(reply.status(), 200);
    let a: TokenReply = reply.json().await.unwrap();

    assert_eq!(register(&base, &key, &a.access_token, A).await, 200);
    assert_eq!(register(&base, &key, &a.access_token, B).await, 403);
    assert_eq!(register(&base, &key, &a.access_token, MIKE).await, 403);
    let brett = "riff://brett@pangolin/como-technologies/riff?session=a";
    assert_eq!(register(&base, &key, &a.access_token, brett).await, 403);

    // A person token acts only as the person.
    assert_eq!(register(&base, &key, &person.access_token, MIKE).await, 200);
    assert_eq!(register(&base, &key, &person.access_token, A).await, 403);

    // The watch stream checks the session too.
    let watch = |uri: &str| {
        let url = format!("{base}/v1/watch");
        common::client()
            .get(&url)
            .query(&[("uri", uri)])
            .header("authorization", format!("DPoP {}", a.access_token))
            .header(
                "dpop",
                key.proof("GET", &url, Some(&a.access_token), common::now()),
            )
            .send()
    };
    assert_eq!(watch(B).await.unwrap().status(), 403);
    assert_eq!(watch(A).await.unwrap().status(), 200);
}

#[tokio::test]
async fn only_a_person_token_gives_a_session_token() {
    let (service, base) = common::start(true, &[]).await;
    let key = Key::generate();
    let person = service
        .tokens()
        .sign_in("mike", &key.thumbprint(), Instant::now())
        .unwrap();
    let a: TokenReply = for_session(&base, &key, &person.access_token, "a")
        .await
        .json()
        .await
        .unwrap();
    let again = for_session(&base, &key, &a.access_token, "b").await;
    assert_eq!(again.status(), 400);

    // Another device key cannot swap the person token.
    let thief = Key::generate();
    let stolen = for_session(&base, &thief, &person.access_token, "b").await;
    assert_eq!(stolen.status(), 400);
}

#[tokio::test]
async fn a_session_refresh_keeps_the_session() {
    let (service, base) = common::start(true, &[]).await;
    let key = Key::generate();
    let person = service
        .tokens()
        .sign_in("mike", &key.thumbprint(), Instant::now())
        .unwrap();
    let a: TokenReply = for_session(&base, &key, &person.access_token, "a")
        .await
        .json()
        .await
        .unwrap();
    let form = format!("grant_type=refresh_token&refresh_token={}", a.refresh_token);
    let next: TokenReply = common::refresh(&base, &key, &form)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(register(&base, &key, &next.access_token, A).await, 200);
    assert_eq!(register(&base, &key, &next.access_token, B).await, 403);
}

/// Sets a status as `me` with `token`. Returns the status code.
async fn set_status(base: &str, key: &Key, token: &str, me: &str, step: &str) -> u16 {
    common::post(&format!("{base}/v1/status"), key, Some(token))
        .json(&json!({ "me": me, "status": { "step": step } }))
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[tokio::test]
async fn a_session_sets_only_its_own_status() {
    let (service, base) = common::start(true, &[]).await;
    let key = Key::generate();
    let person = service
        .tokens()
        .sign_in("mike", &key.thumbprint(), Instant::now())
        .unwrap();
    let reply = for_session(&base, &key, &person.access_token, "a").await;
    let a: TokenReply = reply.json().await.unwrap();

    assert_eq!(
        set_status(&base, &key, &a.access_token, A, "tests").await,
        200
    );
    assert_eq!(
        set_status(&base, &key, &a.access_token, B, "tests").await,
        403
    );
    // A status that does not fit on one line gets 400.
    assert_eq!(
        set_status(&base, &key, &a.access_token, A, "a\nb").await,
        400
    );

    let who = common::post(&format!("{base}/v1/who"), &key, Some(&a.access_token))
        .json(&json!({ "me": A }))
        .send()
        .await
        .unwrap();
    let who: serde_json::Value = who.json().await.unwrap();
    assert_eq!(who["sessions"][0]["status"]["step"], "tests");
    assert_eq!(who["sessions"][0]["status"]["age_secs"], 0);
}
