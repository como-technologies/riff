//! `riff forge create` and `riff forge install` over HTTP, with a fake
//! GitHub and a fake secret store (#627).

use crate::common;
use crate::common::github::{FakeGitHub, SLUG, account_id};

use riff_core::dpop::Key;
use riff_core::forge::app_permissions;
use riff_core::wire::{ForgeCreateReply, ForgeCreatedReply, ForgeInstallReply};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::forge::store::{Store, Stored};
use serde_json::{Value, json};

const KEY: &str = include_str!("../testdata/test-only-rsa-key.pem");

/// The person URI of mike in a clone of `acme/app`.
const PERSON: &str = "riff://mike@pangolin/acme/app";

/// The ID of the App that the fake GitHub makes.
const APP: u64 = 99;

/// A riff with sign-in, no App yet, a store in memory and the fake
/// GitHub.
async fn start(github: &FakeGitHub) -> (Service, String, Store) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let store = Store::memory();
    let service = Service::new(Config {
        require_sign_in: true,
        forge_store: Some(store.clone()),
        github_api: github.url.clone(),
        ..Config::new(&url)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url, store)
}

/// A person, signed in on one device.
struct Person {
    key: Key,
    token: String,
}

impl Person {
    /// `email` signs in. The first person is the owner of the riff. A
    /// person in an allowed domain is a member.
    async fn sign_in(service: &Service, email: &str) -> Person {
        let key = Key::generate();
        let token = service
            .admit(email, true, &key.thumbprint())
            .await
            .unwrap()
            .access_token;
        Person { key, token }
    }

    /// `POST /v1/OP` with `body`.
    async fn call(&self, base: &str, op: &str, body: Value) -> (u16, String) {
        let reply = common::post(&format!("{base}/v1/{op}"), &self.key, Some(&self.token))
            .json(&body)
            .send()
            .await
            .unwrap();
        (reply.status().as_u16(), reply.text().await.unwrap())
    }

    async fn create(&self, base: &str, org: &str) -> ForgeCreateReply {
        let body = json!({ "me": PERSON, "org": org });
        let (status, text) = self.call(base, "forge/create", body).await;
        assert_eq!(status, 200, "{text}");
        serde_json::from_str(&text).unwrap()
    }

    async fn created(&self, base: &str, state: &str) -> (u16, String) {
        let body = json!({ "me": PERSON, "state": state });
        self.call(base, "forge/created", body).await
    }
}

/// A browser: it follows no redirect, so the test sees each one.
fn browser() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

/// `GET` of a page: the status, the `Location` and the body.
async fn page(url: &str) -> (u16, String, String) {
    let reply = browser().get(url).send().await.unwrap();
    let status = reply.status().as_u16();
    let location = reply
        .headers()
        .get("location")
        .map(|l| l.to_str().unwrap().to_owned())
        .unwrap_or_default();
    (status, location, reply.text().await.unwrap())
}

/// The manifest in the form of the start page.
fn manifest_of(html: &str) -> Value {
    let start = html.find("name=\"manifest\" value=\"").unwrap() + 23;
    let end = start + html[start..].find('"').unwrap();
    let text = html[start..end]
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn riff_forge_create_makes_the_app_and_only_the_store_gets_the_key() {
    let github = FakeGitHub::start(&[]).await;
    github.convert_to("code1", APP, KEY);
    let (service, base, store) = start(&github).await;
    let mike = Person::sign_in(&service, "mike@comotechnologies.io").await;
    let (status, text) = mike
        .call(
            &base,
            "forge/install",
            json!({ "me": PERSON, "owner": "acme" }),
        )
        .await;
    assert_eq!(status, 409, "no App yet: {text}");
    let start = mike.create(&base, "acme").await;
    assert_eq!(
        start.url,
        format!("{base}/forge/new?state={}", start.state),
        "the start page of the server"
    );
    let mut replies = vec![serde_json::to_string(&start).unwrap()];

    // The start page posts the manifest to the org on GitHub.
    let (status, _, html) = page(&start.url).await;
    assert_eq!(status, 200, "{html}");
    assert!(
        html.contains(&format!(
            "action=\"https://github.com/organizations/acme/settings/apps/new?state={}\"",
            start.state
        )),
        "{html}"
    );
    let manifest = manifest_of(&html);
    assert_eq!(
        manifest["default_permissions"],
        serde_json::to_value(app_permissions()).unwrap(),
        "only the permissions of the roles"
    );
    assert_eq!(manifest["hook_attributes"]["active"], false, "no webhook");
    assert_eq!(manifest["default_events"], json!([]));
    assert_eq!(manifest["public"], true, "each account can install it");
    assert_eq!(manifest["redirect_url"], format!("{base}/forge/created"));
    replies.push(html);

    // GitHub did not send the browser back yet.
    let (status, text) = mike.created(&base, &start.state).await;
    assert_eq!(status, 200, "{text}");
    let waiting: ForgeCreatedReply = serde_json::from_str(&text).unwrap();
    assert_eq!(waiting, ForgeCreatedReply::default());

    // GitHub sends the browser back with the code. The server swaps it,
    // and sends the browser to the install page on the org.
    let back = format!("{base}/forge/created?code=code1&state={}", start.state);
    let (status, location, body) = page(&back).await;
    assert_eq!(status, 303, "{body}");
    assert_eq!(
        location,
        format!(
            "https://github.com/apps/{SLUG}/installations/new/permissions?target_id={}",
            account_id("acme")
        )
    );
    replies.push(body);
    assert_eq!(github.converted(), ["code1"]);
    assert_eq!(
        store.versions(),
        [Stored {
            app: APP,
            key: KEY.to_owned()
        }],
        "the store keeps the ID and the key"
    );

    let (_, text) = mike.created(&base, &start.state).await;
    let made: ForgeCreatedReply = serde_json::from_str(&text).unwrap();
    assert_eq!(made.app, Some(APP));
    assert_eq!(made.slug.as_deref(), Some(SLUG));
    assert!(!made.installed);
    replies.push(text);

    // The admin installs the App on the org.
    github.install("acme/app", 11);
    let (_, text) = mike.created(&base, &start.state).await;
    let installed: ForgeCreatedReply = serde_json::from_str(&text).unwrap();
    assert!(installed.installed, "{text}");
    replies.push(text);

    // The server uses the new App at once: its JWT reads the App.
    let (status, text) = mike
        .call(
            &base,
            "forge/install",
            json!({ "me": PERSON, "owner": "acme" }),
        )
        .await;
    assert_eq!(status, 200, "{text}");
    replies.push(text);

    for reply in &replies {
        assert!(
            !reply.contains("PRIVATE KEY"),
            "a reply holds the key: {reply}"
        );
    }
}

#[tokio::test]
async fn a_wrong_or_used_state_is_refused() {
    let github = FakeGitHub::start(&[]).await;
    github.convert_to("code1", APP, KEY);
    let (service, base, store) = start(&github).await;
    let mike = Person::sign_in(&service, "mike@comotechnologies.io").await;

    // A state that the server did not make.
    let (status, _, html) = page(&format!("{base}/forge/new?state=abc")).await;
    assert_eq!(status, 400);
    assert!(html.contains("Run riff forge create again"), "{html}");
    let back = format!("{base}/forge/created?code=code1&state=abc");
    let (status, location, _) = page(&back).await;
    assert_eq!((status, location.as_str()), (400, ""));
    assert!(github.converted().is_empty(), "no code is swapped");

    // A state goes back from GitHub one time only.
    let start = mike.create(&base, "acme").await;
    let back = format!("{base}/forge/created?code=code1&state={}", start.state);
    assert_eq!(page(&back).await.0, 303);
    let (status, _, html) = page(&back).await;
    assert_eq!(status, 400, "{html}");
    let (status, _, _) = page(&start.url).await;
    assert_eq!(status, 400, "the start page of a used state");
    assert_eq!(github.converted(), ["code1"]);
    assert_eq!(store.versions().len(), 1);

    // A code that GitHub does not know ends the start with its error.
    let start = mike.create(&base, "acme").await;
    let back = format!("{base}/forge/created?code=other&state={}", start.state);
    let (status, _, html) = page(&back).await;
    assert_eq!(status, 400, "{html}");
    let (_, text) = mike.created(&base, &start.state).await;
    let failed: ForgeCreatedReply = serde_json::from_str(&text).unwrap();
    assert!(
        failed.error.as_deref().is_some_and(|e| e.contains("404")),
        "{text}"
    );
    assert_eq!(store.versions().len(), 1);
}

#[tokio::test]
async fn a_member_that_is_not_an_admin_is_refused() {
    let github = FakeGitHub::start(&[]).await;
    let (service, base, store) = start(&github).await;
    let mike = Person::sign_in(&service, "mike@comotechnologies.io").await;
    let bob = Person::sign_in(&service, "bob@comotechnologies.io").await;
    let body = json!({ "me": "riff://bob@kadomony/acme/app", "org": "acme" });
    let (status, text) = bob.call(&base, "forge/create", body).await;
    assert_eq!(status, 403, "{text}");
    assert!(text.contains("only the owner and the admins"), "{text}");

    // The state of the owner is not for bob.
    let start = mike.create(&base, "acme").await;
    let body = json!({ "me": "riff://bob@kadomony/acme/app", "state": start.state });
    let (status, _) = bob.call(&base, "forge/created", body).await;
    assert_eq!(status, 403);
    assert!(store.versions().is_empty());
}

#[tokio::test]
async fn a_server_with_no_store_refuses_riff_forge_create() {
    let github = FakeGitHub::start(&[]).await;
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let service = Service::new(Config {
        require_sign_in: true,
        github_api: github.url.clone(),
        ..Config::new(&base)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mike = Person::sign_in(&service, "mike@comotechnologies.io").await;
    let body = json!({ "me": PERSON, "org": "acme" });
    let (status, text) = mike.call(&base, "forge/create", body).await;
    assert_eq!(status, 409, "{text}");
    assert!(text.contains("RIFF_FORGE_SECRET"), "{text}");
}

#[tokio::test]
async fn riff_forge_install_gives_the_install_page_of_the_account() {
    let github = FakeGitHub::start(&[("acme/app", 11)]).await;
    github.convert_to("code1", APP, KEY);
    let (service, base, _) = start(&github).await;
    let mike = Person::sign_in(&service, "mike@comotechnologies.io").await;
    let start = mike.create(&base, "acme").await;
    let back = format!("{base}/forge/created?code=code1&state={}", start.state);
    assert_eq!(page(&back).await.0, 303);

    let install = |owner: &'static str| {
        let mike = &mike;
        let base = &base;
        async move {
            let body = json!({ "me": PERSON, "owner": owner });
            let (status, text) = mike.call(base, "forge/install", body).await;
            assert_eq!(status, 200, "{text}");
            serde_json::from_str::<ForgeInstallReply>(&text).unwrap()
        }
    };
    let reply = install("n8behavior").await;
    assert_eq!(
        reply.url,
        format!(
            "https://github.com/apps/{SLUG}/installations/new/permissions?target_id={}",
            account_id("n8behavior")
        )
    );
    assert!(!reply.installed);
    github.install("n8behavior/dotfiles", 33);
    assert!(install("n8behavior").await.installed);
    assert!(install("acme").await.installed);

    let body = json!({ "me": PERSON, "owner": "acme/app" });
    let (status, _) = mike.call(&base, "forge/install", body).await;
    assert_eq!(status, 400, "no account name");
}
