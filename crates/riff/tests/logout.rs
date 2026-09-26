//! `riff logout --all` against a real server (R20).

use std::time::Instant;

use assert_cmd::Command;
use riff_server::Service;

async fn start(admins: &[&str]) -> (Service, String) {
    let service = Service::with_admins(admins.iter().map(|a| a.to_string()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, format!("http://{addr}"))
}

/// Runs `riff` with `token`. Returns stdout, stderr and the exit code.
async fn riff(server: &str, token: &str, args: &[&str]) -> (String, String, i32) {
    let mut cmd = Command::cargo_bin("riff").unwrap();
    cmd.args(args)
        .env("RIFF_SERVER", server)
        .env("RIFF_TOKEN", token);
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
        out.status.code().unwrap(),
    )
}

#[tokio::test]
async fn logout_all_ends_each_sign_in_of_the_caller() {
    let (service, server) = start(&[]).await;
    let now = Instant::now();
    let laptop = service.tokens().sign_in("mike", now).unwrap();
    let desktop = service.tokens().sign_in("mike", now).unwrap();

    let (out, _, code) = riff(&server, &laptop.access_token, &["logout", "--all"]).await;
    assert_eq!(code, 0);
    assert_eq!(
        out,
        "Ended 2 sign-ins of mike. Each device of mike must sign in again.\n"
    );
    assert!(service.tokens().check(&desktop.access_token, now).is_err());
}

#[tokio::test]
async fn an_admin_logs_out_another_person() {
    let (service, server) = start(&["mike"]).await;
    let now = Instant::now();
    let mike = service.tokens().sign_in("mike", now).unwrap();
    let brett = service.tokens().sign_in("brett", now).unwrap();

    let args = ["logout", "--all", "--user", "mike"];
    let (_, err, code) = riff(&server, &brett.access_token, &args).await;
    assert_ne!(code, 0);
    assert!(err.contains("not an admin"), "{err}");

    let args = ["logout", "--all", "--user", "brett"];
    let (out, _, code) = riff(&server, &mike.access_token, &args).await;
    assert_eq!(code, 0);
    assert!(out.starts_with("Ended 1 sign-in of brett."), "{out}");
    assert!(service.tokens().check(&brett.access_token, now).is_err());
    assert_eq!(service.tokens().check(&mike.access_token, now), Ok("mike"));
}
