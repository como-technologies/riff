//! Each session keeps one user (R157-R159). A keyring error stops
//! `riff`; it does not fall back to `USER`. The server refuses a known
//! session ID under a new user.

use crate::common;
use isolated::Isolated;

use keyring_core::{Entry, Error, mock};
use riff::identity;
use riff_client::api::Api;
use riff_client::login::{self, SignIn};
use riff_client::secrets;
use riff_core::name::SessionUri;

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

/// A sign-in of mike at `server` in the mock keyring, and a keyring
/// that fails on the next read of it. Another process wrote the
/// sign-in, so this process keeps no value of it
/// ([`secrets::kept`]): only this test uses `server`.
fn broken_sign_in(server: &str) {
    common::mock_keyring();
    let sign_in = SignIn {
        user: "mike".into(),
        access_token: "a-1".into(),
        refresh_token: "r-1".into(),
        expires_at: 0,
        riff_id: None,
    };
    let entry = Entry::new(secrets::SERVICE, &login::secret_name(server)).unwrap();
    entry
        .set_password(&serde_json::to_string(&sign_in).unwrap())
        .unwrap();
    let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
    cred.set_error(Error::NoStorageAccess(Box::new(std::io::Error::other(
        "locked",
    ))));
}

#[test]
fn a_keyring_error_stops_the_user_lookup() {
    let server = "http://one-user.invalid:9";
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
    let out = Isolated::shared()
        .riff()
        .arg("whoami")
        .env_remove("RIFF_USER")
        .env_remove("RIFF_HOME")
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
    let out = Isolated::shared()
        .riff()
        .arg("whoami")
        .env("RIFF_USER", "brett")
        .env_remove("RIFF_HOME")
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env("RIFF_SERVER", "http://127.0.0.1:9")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("session  brett@"));
}

#[tokio::test]
async fn the_server_refuses_a_known_session_under_a_new_user() {
    let server = common::start_signed().await;
    let api = server.api("mike", Some("a1"));
    let thread = "como-technologies/riff".parse().unwrap();
    let mike = uri("riff://mike@pangolin/como-technologies/riff?session=a1#issue-7");
    let other = uri("riff://bob@pangolin/como-technologies/riff?session=a1");
    api.register(&mike).await.unwrap();
    // The person of mike names it the lead, so it can resume the riff.
    common::name_lead(&server.api("mike", None), &mike).await;
    api.set_riff(&mike, riff_core::wire::RiffState::Running)
        .await
        .unwrap();
    assert!(api.claim(&mike, &thread, "issue-7").await.unwrap().granted);

    let bob = server.api("bob", Some("a1"));
    let error = bob.register(&other).await.unwrap_err().to_string();
    assert!(error.contains("409"), "{error}");
    assert!(error.contains("known as user mike"), "{error}");
    let error = match bob.watch(&other).await {
        Ok(_) => panic!("the watch must fail"),
        Err(e) => e.to_string(),
    };
    assert!(error.contains("known as user mike"), "{error}");
    assert!(bob.claim(&other, &thread, "issue-8").await.is_err());

    let api = server.api("mike", Some("a1"));
    let who = api.who(&mike, false).await.unwrap();
    assert_eq!(who.len(), 1, "one entry for the session");
    assert_eq!(who[0].uri.who(), mike.who());
    assert_eq!(who[0].uri.claims(), ["issue-7"]);
    assert_eq!(who[0].uri.place().worktree(), Some("issue-7"));
}
