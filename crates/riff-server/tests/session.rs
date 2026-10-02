//! A token for each session (R19, R104, R105,
//! 01M3WFVAB44T8EP4QZD4KS7DRF, 01M3WFVADCDZM8XX590KAEMEYG), over HTTP.

mod common;

use std::time::Instant;

use riff_core::dpop::Key;
use riff_core::wire::TokenReply;
use serde_json::json;

const A: &str = "riff://mike@pangolin/como-technologies/riff?session=a";
const B: &str = "riff://mike@pangolin/como-technologies/riff?session=b";
const MIKE: &str = "riff://mike@pangolin";

/// Swaps a person access token for a session access token.
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
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
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

    // So does the call of the status line (01M3T5GFVS8NMA992KHZN4VE17).
    let me = |uri: &str| {
        let url = format!("{base}/v1/me");
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
    assert_eq!(me(B).await.unwrap().status(), 403);
    assert_eq!(me(A).await.unwrap().status(), 200);
}

#[tokio::test]
async fn only_a_person_token_gives_a_session_token() {
    let (service, base) = common::start(true, &[]).await;
    let key = Key::generate();
    let person = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
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

/// A session token has no refresh token (01M3WFVAB44T8EP4QZD4KS7DRF):
/// the reply has an empty one, and the server refuses it.
#[tokio::test]
async fn a_session_token_has_no_refresh_token() {
    let (service, base) = common::start(true, &[]).await;
    let key = Key::generate();
    let person = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
        .unwrap();
    let reply = for_session(&base, &key, &person.access_token, "a").await;
    assert_eq!(reply.status(), 200);
    let a: serde_json::Value = reply.json().await.unwrap();
    assert_eq!(a["refresh_token"], "");
    assert_eq!(a["expires_in"], 600);
    assert_eq!(a["user"], "mike");
    let token = a["access_token"].as_str().unwrap();
    assert_eq!(register(&base, &key, token, A).await, 200);

    let refused = common::refresh(&base, &key, "grant_type=refresh_token&refresh_token=").await;
    assert_eq!(refused.status(), 400);
    assert_eq!(service.tokens().chains(), 1, "only the person chain");
}

/// One session has many processes: `riff mcp`, `riff watch`, each hook
/// and each `riff` command. A new token for the session ends no other
/// token of it (#381).
#[tokio::test]
async fn two_processes_of_one_session_keep_their_tokens() {
    let (service, base) = common::start(true, &[]).await;
    let key = Key::generate();
    let person = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
        .unwrap();
    let swap = || async {
        let reply = for_session(&base, &key, &person.access_token, "a").await;
        assert_eq!(reply.status(), 200);
        reply.json::<TokenReply>().await.unwrap().access_token
    };
    let long = swap().await;
    for _ in 0..10 {
        assert_eq!(register(&base, &key, &long, A).await, 200);
        let short = swap().await;
        assert_ne!(short, long);
        assert_eq!(register(&base, &key, &short, A).await, 200);
    }
    assert_eq!(register(&base, &key, &long, A).await, 200);
}

/// A removed person and a revoked sign-in lose each session token at
/// once (R20).
#[tokio::test]
async fn the_end_of_a_sign_in_ends_each_session_token_at_once() {
    // mike is an admin of the settings. brett is of an allowed domain.
    let (service, base) = common::start(true, &["mike@comotechnologies.io"]).await;
    let (mike_key, brett_key) = (Key::generate(), Key::generate());
    let mike = service
        .admit("mike@comotechnologies.io", false, &mike_key.thumbprint())
        .await
        .unwrap();
    let brett = service
        .admit("brett@comotechnologies.io", true, &brett_key.thumbprint())
        .await
        .unwrap();
    let brett_b = "riff://brett@kadomony/como-technologies/riff?session=b";

    // Two processes of one session, each with its own token.
    let mut held = Vec::new();
    for (person, key, me, id) in [
        (&mike, &mike_key, A, "a"),
        (&brett, &brett_key, brett_b, "b"),
    ] {
        for _ in 0..2 {
            let reply = for_session(&base, key, &person.access_token, id).await;
            let token = reply.json::<TokenReply>().await.unwrap().access_token;
            assert_eq!(register(&base, key, &token, me).await, 200);
            held.push(token);
        }
    }

    // An admin removes brett.
    let removed = common::post(
        &format!("{base}/v1/remove"),
        &mike_key,
        Some(&mike.access_token),
    )
    .json(&json!({ "email": "brett@comotechnologies.io" }))
    .send()
    .await
    .unwrap();
    assert_eq!(removed.status(), 200);
    for token in &held[2..] {
        assert_eq!(register(&base, &brett_key, token, brett_b).await, 401);
    }
    let again = for_session(&base, &brett_key, &brett.access_token, "b").await;
    assert_eq!(again.status(), 400);
    // The tokens of mike stay.
    for token in &held[..2] {
        assert_eq!(register(&base, &mike_key, token, A).await, 200);
    }

    // Mike revokes his own sign-ins.
    let revoked = common::post(
        &format!("{base}/v1/revoke"),
        &mike_key,
        Some(&mike.access_token),
    )
    .json(&json!({}))
    .send()
    .await
    .unwrap();
    assert_eq!(revoked.status(), 200);
    for token in &held[..2] {
        assert_eq!(register(&base, &mike_key, token, A).await, 401);
    }
    let again = for_session(&base, &mike_key, &mike.access_token, "a").await;
    assert_eq!(again.status(), 400);
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
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
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
