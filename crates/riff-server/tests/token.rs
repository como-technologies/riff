use std::time::Instant;

use riff_core::wire::{TokenError, TokenReply};
use riff_server::Service;

async fn start() -> (Service, String) {
    let service = Service::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/token", listener.local_addr().unwrap());
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

async fn refresh(url: &str, form: &str) -> (u16, String) {
    let reply = reqwest::Client::new()
        .post(url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form.to_owned())
        .send()
        .await
        .unwrap();
    assert_eq!(reply.headers()["cache-control"], "no-store");
    (reply.status().as_u16(), reply.text().await.unwrap())
}

fn error(body: &str) -> String {
    serde_json::from_str::<TokenError>(body).unwrap().error
}

#[tokio::test]
async fn refresh_rotates_and_reuse_revokes() {
    let (service, url) = start().await;
    let first = service.tokens().sign_in("mike", Instant::now()).unwrap();
    let form = format!(
        "grant_type=refresh_token&refresh_token={}",
        first.refresh_token
    );

    let (status, body) = refresh(&url, &form).await;
    assert_eq!(status, 200);
    let second: TokenReply = serde_json::from_str(&body).unwrap();
    assert_eq!(second.token_type, "Bearer");
    assert!(second.expires_in <= 600);
    let now = Instant::now();
    assert_eq!(
        service.tokens().check(&second.access_token, now),
        Ok("mike")
    );

    // The same refresh token again: refused, and the sign-in ends.
    let (status, body) = refresh(&url, &form).await;
    assert_eq!((status, error(&body).as_str()), (400, "invalid_grant"));
    assert!(service.tokens().check(&second.access_token, now).is_err());
}

#[tokio::test]
async fn unknown_tokens_and_grants_are_refused() {
    let (_, url) = start().await;
    let (status, body) = refresh(&url, "grant_type=refresh_token&refresh_token=nope").await;
    assert_eq!((status, error(&body).as_str()), (400, "invalid_grant"));
    let (status, body) = refresh(&url, "grant_type=password&refresh_token=nope").await;
    assert_eq!(
        (status, error(&body).as_str()),
        (400, "unsupported_grant_type")
    );
}
