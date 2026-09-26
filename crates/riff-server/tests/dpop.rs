//! Tokens bound to a device key (R18, RFC 9449), over HTTP.

mod common;

use std::time::Instant;

use riff_core::dpop::Key;

async fn who(request: reqwest::RequestBuilder) -> reqwest::Response {
    request
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap()
}

fn challenge(reply: &reqwest::Response) -> String {
    reply.headers()["www-authenticate"]
        .to_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn a_copied_token_without_the_key_is_refused() {
    let (service, base) = common::start(true, &[]).await;
    let url = format!("{base}/v1/who");
    let key = Key::generate();
    let pair = service
        .tokens()
        .sign_in("mike", &key.thumbprint(), Instant::now())
        .unwrap();
    let token = pair.access_token.as_str();

    // The owner passes.
    let ok = who(common::post(&url, &key, Some(token))).await;
    assert_eq!(ok.status(), 200);

    // A thief with the token and their own key.
    let thief = who(common::post(&url, &Key::generate(), Some(token))).await;
    assert_eq!(thief.status(), 401);
    assert!(
        challenge(&thief).contains("another device key"),
        "{}",
        challenge(&thief)
    );

    // A thief with the token and no proof.
    let bare = reqwest::Client::new()
        .post(&url)
        .header("authorization", format!("DPoP {token}"));
    let bare = who(bare).await;
    assert_eq!(bare.status(), 401);
    assert!(challenge(&bare).starts_with(r#"DPoP error="invalid_dpop_proof""#));

    // The token as a bearer token.
    let bearer = reqwest::Client::new()
        .post(&url)
        .bearer_auth(token)
        .header("dpop", key.proof("POST", &url, Some(token), common::now()));
    let bearer = who(bearer).await;
    assert_eq!(bearer.status(), 401);
    assert!(challenge(&bearer).starts_with(r#"DPoP algs="ES256""#));
}

#[tokio::test]
async fn a_proof_works_once_and_only_for_its_request() {
    let (service, base) = common::start(true, &[]).await;
    let url = format!("{base}/v1/who");
    let key = Key::generate();
    let pair = service
        .tokens()
        .sign_in("mike", &key.thumbprint(), Instant::now())
        .unwrap();
    let token = pair.access_token.as_str();
    let proof = key.proof("POST", &url, Some(token), common::now());
    let send = |proof: String| {
        reqwest::Client::new()
            .post(&url)
            .header("authorization", format!("DPoP {token}"))
            .header("dpop", proof)
    };

    assert_eq!(who(send(proof.clone())).await.status(), 200);
    let replayed = who(send(proof)).await;
    assert_eq!(replayed.status(), 401);
    assert!(challenge(&replayed).contains("used before"));

    // A proof for another path.
    let other = key.proof(
        "POST",
        &format!("{base}/v1/post"),
        Some(token),
        common::now(),
    );
    assert_eq!(who(send(other)).await.status(), 401);
}

#[tokio::test]
async fn refresh_needs_the_key_of_the_sign_in() {
    let (service, base) = common::start(true, &[]).await;
    let key = Key::generate();
    let pair = service
        .tokens()
        .sign_in("mike", &key.thumbprint(), Instant::now())
        .unwrap();
    let form = format!(
        "grant_type=refresh_token&refresh_token={}",
        pair.refresh_token
    );

    let thief = common::refresh(&base, &Key::generate(), &form).await;
    assert_eq!(thief.status(), 400);

    let no_proof = reqwest::Client::new()
        .post(format!("{base}/v1/token"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(no_proof.status(), 400);
    assert!(
        no_proof
            .text()
            .await
            .unwrap()
            .contains("invalid_dpop_proof")
    );

    // The refused tries did not use the refresh token.
    assert_eq!(common::refresh(&base, &key, &form).await.status(), 200);
}
