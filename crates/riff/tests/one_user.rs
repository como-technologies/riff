//! Each session keeps one user (R157-R159). A keyring error stops
//! `riff`; it does not fall back to `USER`. The server refuses a known
//! session ID under a new user.

use std::process::Command;
use std::sync::Once;

use keyring_core::{Entry, Error, mock};
use riff::api::Api;
use riff::login::{self, SignIn};
use riff::{identity, secrets};
use riff_core::name::SessionUri;

static MOCK_KEYRING: Once = Once::new();

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// A sign-in of mike at `server` in the mock keyring, and a keyring
/// that fails on the next read of it.
fn broken_sign_in(server: &str) {
    MOCK_KEYRING.call_once(|| keyring_core::set_default_store(mock::Store::new().unwrap()));
    let sign_in = SignIn {
        user: "mike".into(),
        access_token: "a-1".into(),
        refresh_token: "r-1".into(),
        expires_at: 0,
        riff_id: None,
    };
    login::store(server, &sign_in).unwrap();
    let entry = Entry::new(secrets::SERVICE, &login::secret_name(server)).unwrap();
    let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
    cred.set_error(Error::NoStorageAccess(Box::new(std::io::Error::other(
        "locked",
    ))));
}

#[test]
fn a_keyring_error_stops_the_user_lookup() {
    let server = "http://127.0.0.1:9";
    // SAFETY: this test binary changes the environment only here.
    unsafe { std::env::remove_var("RIFF_USER") };
    let place = identity::place(std::path::Path::new(".")).unwrap();

    broken_sign_in(server);
    let error = format!("{:#}", identity::me(&place, server).unwrap_err());
    assert!(
        error.contains("Unlock the keyring, or set RIFF_USER"),
        "{error}"
    );
    assert!(error.contains("locked"), "{error}");

    broken_sign_in(server);
    let error = Api::new(server).signed_in(None).err().unwrap();
    assert!(format!("{error:#}").contains("locked"), "{error:#}");
}

#[cfg(target_os = "linux")]
#[test]
fn a_command_stops_when_it_cannot_open_the_keyring() {
    let out = Command::new(assert_cmd::cargo::cargo_bin("riff"))
        .arg("whoami")
        .env_remove("RIFF_USER")
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env("RIFF_SERVER", "http://127.0.0.1:9")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("set RIFF_USER"), "{stderr}");
}

#[cfg(target_os = "linux")]
#[test]
fn riff_user_stands_in_when_it_cannot_open_the_keyring() {
    let out = Command::new(assert_cmd::cargo::cargo_bin("riff"))
        .arg("whoami")
        .env("RIFF_USER", "brett")
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env("RIFF_SERVER", "http://127.0.0.1:9")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("brett@"));
}

#[tokio::test]
async fn the_server_refuses_a_known_session_under_a_new_user() {
    let api = start_server().await;
    let thread = "como-technologies/riff".parse().unwrap();
    let mike = uri("riff://mike@pangolin/como-technologies/riff?session=a1#issue-7");
    let other = uri("riff://sandman@pangolin/como-technologies/riff?session=a1");
    api.register(&mike).await.unwrap();
    // The first session of mike is its lead, so it can resume the riff.
    api.set_riff(&mike, riff_core::wire::RiffState::Running)
        .await
        .unwrap();
    assert!(api.claim(&mike, &thread, "issue-7").await.unwrap().granted);

    let error = api.register(&other).await.unwrap_err().to_string();
    assert!(error.contains("409"), "{error}");
    assert!(error.contains("known as user mike"), "{error}");
    let error = match api.watch(&other).await {
        Ok(_) => panic!("the watch must fail"),
        Err(e) => e.to_string(),
    };
    assert!(error.contains("known as user mike"), "{error}");
    assert!(api.claim(&other, &thread, "issue-8").await.is_err());

    let who = api.who(&mike, false).await.unwrap();
    assert_eq!(who.len(), 1, "one entry for the session");
    assert_eq!(who[0].uri.who(), mike.who());
    assert_eq!(who[0].uri.claims(), ["issue-7"]);
    assert_eq!(who[0].uri.place().worktree(), Some("issue-7"));
}
