//! `riff device add --key NAME`, `riff device list` and `riff device
//! remove NAME` (01M75NGT9Z0NSZTG97TBAMP7RS, 01M75NH2RSEJ3K618M17PCH3ZZ,
//! 01M75NH30PNATYN33MXZQMVBW8, 01M75NH3625QATGDT6DK652ZK6): the help,
//! the refusal in a session and in a container, and the add against a
//! real server with a fake browser and a fake authenticator. The
//! sign-in is in the mock store of `keyring-core`, so the tests run in
//! process.

use crate::book;
use crate::common;

use std::time::Duration;

use isolated::Isolated;
use riff::device::{Wait, add_key};
use riff::text;
use riff_client::api::Api;
use riff_client::login::{self, SignIn};
use riff_core::device::fake::{Authenticator, Shape};
use riff_core::wire::{KeyOptions, KeyReply};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::Provider;

/// The stdout of `riff ARGS`, in a terminal of 200 columns.
fn riff(args: &[&str]) -> String {
    let out = Isolated::shared()
        .riff()
        .args(args)
        .env("COLUMNS", "200")
        .output()
        .unwrap();
    assert!(out.status.success(), "riff {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// The help of `riff device` lists `add --key`, `list` and `remove`.
/// `riff device add` needs `--key NAME`.
#[test]
fn the_help_of_device_lists_add_with_key_list_and_remove() {
    let help = riff(&["device", "--help"]);
    for line in [
        "  add     Add an authenticator with --key NAME\n",
        "  list    List your devices\n",
        "  remove  Remove one of your devices at once\n",
    ] {
        assert!(help.contains(line), "{line:?}: {help}");
    }
    let add = riff(&["device", "add", "--help"]);
    assert!(add.contains("--key <NAME>"), "{add}");
    let out = Isolated::shared()
        .riff()
        .args(["device", "add"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("--key <NAME>"), "{stderr}");
    let top = riff(&["--help"]);
    assert!(
        top.contains("  device        Add, list and remove your authenticators\n"),
        "{top}"
    );
}

/// Each subcommand refuses in an agent session and in a container,
/// before each call.
#[test]
fn riff_device_refuses_in_an_agent_session_and_in_a_container() {
    let env = Isolated::shared();
    let socket = env.path().join("riff.sock");
    for args in [
        &["device", "add", "--key", "yubikey"][..],
        &["device", "list"],
        &["device", "remove", "yubikey"],
    ] {
        for (var, value, container) in [
            ("CLAUDECODE", "1", false),
            ("RIFF_SESSION", "s1", false),
            (riff::daemon::SOCKET_VAR, socket.to_str().unwrap(), true),
        ] {
            let out = env.riff().args(args).env(var, value).output().unwrap();
            let stderr = String::from_utf8(out.stderr).unwrap();
            assert_eq!(out.status.code(), Some(1), "{args:?} {var}: {stderr}");
            assert_eq!(
                stderr,
                format!("{}\n", text::device_refused(container)),
                "{var}"
            );
        }
    }
}

/// The how-tos of the book run with `--help`.
#[test]
fn each_how_to_of_the_book_is_a_real_command() {
    let commands = book::commands_of_part("how-it-works.md", "A proof of presence");
    assert_eq!(
        commands,
        [
            "riff device add --key yubikey",
            "riff device list",
            "riff device remove yubikey"
        ]
    );
    book::each_is_real(&commands);
}

/// A riff with sign-in, and `email` signed in on this device.
async fn signed_in(email: &str) -> (Service, Api) {
    let (listener, url) = common::listen().await;
    let config = Config {
        provider: Some(Provider {
            issuer: "https://accounts.example.com".into(),
            client_id: "riff".into(),
            client_secret: None,
            allowed_domains: Vec::new(),
        }),
        ..Config::new(&url)
    };
    let service = Service::new(config);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let api = Api::new(&url);
    let jkt = riff_client::device::key(api.base()).unwrap().thumbprint();
    let pair = service.admit(email, false, &jkt).await.unwrap();
    let sign_in = SignIn {
        user: pair.user.clone(),
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_at: u64::MAX,
        riff_id: service.riff_id(),
    };
    login::store(api.base(), &sign_in).unwrap();
    let api = api.signed_in(None).unwrap();
    (service, api)
}

/// The browser of the test: it reads the ticket from the fragment of
/// the page, asks riff-server at `server` for the options, and sends the
/// reply of `key` in the shape `shape`, as the page does.
fn browser(server: String, key: Authenticator, shape: Shape) -> impl FnOnce(&str) {
    move |url: &str| {
        let url = url.to_owned();
        tokio::spawn(async move {
            let (page, ticket) = url.split_once('#').unwrap();
            let origin = page.strip_suffix("/device/key").unwrap();
            let client = reqwest::Client::new();
            let options: KeyOptions = client
                .post(format!("{server}/device/key/options"))
                .json(&serde_json::json!({ "ticket": ticket }))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            let reply: KeyReply = key.reply_as(ticket, &options, origin, shape);
            client
                .post(format!("{server}/device/key/reply"))
                .json(&reply)
                .send()
                .await
                .unwrap();
        });
    }
}

/// A short wait, for a test.
const WAIT: Wait = Wait {
    every: Duration::from_millis(20),
    most: Duration::from_secs(30),
};

/// The add shows the URL of the page, opens it, and waits for the end:
/// the key is in the list, and the remove takes it out.
#[tokio::test]
async fn a_person_adds_lists_and_removes_a_key() {
    let (_service, api) = signed_in("mike@comotechnologies.io").await;
    let open = browser(api.base().to_owned(), Authenticator::new(), Shape::Packed);
    let shown = std::sync::Mutex::new(String::new());
    let show = |lines: &str| shown.lock().unwrap().push_str(lines);
    let name = add_key(&api, "yubikey", show, open, WAIT).await.unwrap();
    assert_eq!(name, "yubikey");
    let shown = shown.into_inner().unwrap();
    assert!(shown.contains("/device/key#"), "{shown}");
    assert!(shown.contains("the key yubikey"), "{shown}");

    let list = text::device_list(&api.devices().await.unwrap());
    let lines: Vec<&str> = list.lines().collect();
    assert_eq!(lines.len(), 2, "{list}");
    assert!(lines[0].starts_with("KIND  NAME     ADDED"), "{list}");
    assert!(lines[1].starts_with("key   yubikey  "), "{list}");
    assert!(lines[1].ends_with(" UTC"), "{list}");

    let removed = api.remove_device("yubikey").await.unwrap();
    assert_eq!(
        text::device_removed(&removed),
        "riff: removed the key yubikey."
    );
    let none = text::device_list(&api.devices().await.unwrap());
    assert!(none.starts_with("You have no devices."), "{none}");
    let error = api.remove_device("yubikey").await.unwrap_err();
    assert!(
        format!("{error:#}").contains("you have no device with the name yubikey"),
        "{error:#}"
    );
}

/// An add that riff-server refuses says why, and adds no key.
#[tokio::test]
async fn riff_device_add_says_why_riff_server_refused_the_key() {
    let (_service, api) = signed_in("mike@comotechnologies.io").await;
    let open = browser(
        api.base().to_owned(),
        Authenticator::new(),
        Shape::BadSignature,
    );
    let error = add_key(&api, "yubikey", |_| {}, open, WAIT)
        .await
        .unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.starts_with("riff did not add the key yubikey: the signature of the reply"),
        "{error}"
    );
    assert!(api.devices().await.unwrap().devices.is_empty());

    // A name that is not allowed fails before the browser opens.
    let opened = std::sync::atomic::AtomicBool::new(false);
    let open = |_: &str| opened.store(true, std::sync::atomic::Ordering::SeqCst);
    let error = add_key(&api, "my key", |_| {}, open, WAIT)
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("not allowed"), "{error:#}");
    assert!(!opened.into_inner());
}

/// An add whose page sends no reply in the wait ends, and says what to
/// run.
#[tokio::test]
async fn riff_device_add_ends_when_the_page_sends_no_reply() {
    let (_service, api) = signed_in("mike@comotechnologies.io").await;
    let wait = Wait {
        every: Duration::from_millis(10),
        most: Duration::from_millis(50),
    };
    let error = add_key(&api, "yubikey", |_| {}, |_| {}, wait)
        .await
        .unwrap_err();
    assert_eq!(format!("{error:#}"), text::device_add_no_reply("yubikey"));
}
