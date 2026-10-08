//! `POST /v1/forge/token` and `POST /v1/forge/check` over HTTP, with a
//! fake GitHub of two accounts (#628).

use crate::common;
use crate::common::github::FakeGitHub;

use std::time::Duration;

use riff_core::dpop::Key;
use riff_core::forge::{TokenRole, permissions};
use riff_core::wire::{ForgeCheckReply, ForgeTokenReply, TokenReply};
use riff_server::Service;
use riff_server::auth::Config;
use serde_json::{Value, json};

const KEY: &str = include_str!("../testdata/test-only-rsa-key.pem");

/// The lead, a worker and a verifier of mike in `acme/app`, a session
/// in `acme/lib` (the same organization), and a session in
/// `mike/tools` (a personal account).
const LEAD: &str = "riff://mike@pangolin/acme/app?session=l";
const WORKER: &str = "riff://mike@pangolin/acme/app?session=w#issue-1";
const VERIFIER: &str = "riff://mike@pangolin/acme/app?session=v#verify-issue-1";
const LIB: &str = "riff://mike@pangolin/acme/lib?session=x";
const TOOLS: &str = "riff://mike@pangolin/mike/tools?session=t";
const OTHER: &str = "riff://mike@pangolin/stranger/app?session=o";
/// The wrapper of the lead, before its session starts.
const PERSON: &str = "riff://mike@pangolin/acme/app";

/// The installations of the App: one on the organization `acme`, one on
/// the personal account `mike`.
const INSTALLS: &[(&str, u64)] = &[("acme/app", 11), ("acme/lib", 11), ("mike/tools", 22)];

/// A riff with sign-in, and the App of the fake GitHub.
async fn start(github: &FakeGitHub) -> (Service, String) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let forge = riff_server::forge::Settings::of(Some("7"), Some(KEY), Some(&github.url))
        .unwrap()
        .unwrap();
    let service = Service::new(Config {
        require_sign_in: true,
        forge: Some(forge),
        ..Config::new(&url)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// mike, signed in on one device.
struct Mike {
    key: Key,
    person: String,
}

impl Mike {
    /// mike signs in. The first person is the owner of the riff.
    async fn sign_in(service: &Service) -> Mike {
        let key = Key::generate();
        let person = service
            .admit("mike@comotechnologies.io", false, &key.thumbprint())
            .await
            .unwrap()
            .access_token;
        Mike { key, person }
    }

    /// The token of the session of `uri`, or of the person for a URI
    /// with no session.
    async fn token(&self, base: &str, uri: &str) -> String {
        let Some(id) = uri.split("session=").nth(1) else {
            return self.person.clone();
        };
        let id = id.split('#').next().unwrap();
        let form = format!(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
             &subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token\
             &subject_token={}&session={id}",
            self.person
        );
        let reply = common::refresh(base, &self.key, &form).await;
        assert_eq!(reply.status(), 200);
        reply.json::<TokenReply>().await.unwrap().access_token
    }

    /// `POST /v1/OP` with `body` as the session of `uri`.
    async fn call(&self, base: &str, uri: &str, op: &str, body: Value) -> (u16, String) {
        let token = self.token(base, uri).await;
        let reply = common::post(&format!("{base}/v1/{op}"), &self.key, Some(&token))
            .json(&body)
            .send()
            .await
            .unwrap();
        (reply.status().as_u16(), reply.text().await.unwrap())
    }

    /// A new riff starts paused: mike, the owner, resumes it.
    async fn resume(&self, base: &str) {
        let body = json!({ "me": PERSON, "riff": true });
        let (status, text) = self.call(base, PERSON, "resume", body).await;
        assert_eq!(status, 200, "{text}");
    }

    async fn register(&self, base: &str, uri: &str, worker: bool) {
        let me = uri.split('#').next().unwrap();
        let body = json!({ "me": me, "worker": worker });
        let (status, text) = self.call(base, uri, "register", body).await;
        assert_eq!(status, 200, "{text}");
        if let Some(item) = uri.split('#').nth(1) {
            let thread = me.split('/').skip(3).collect::<Vec<_>>().join("/");
            let thread = thread.split('?').next().unwrap();
            let body = json!({ "me": me, "thread": thread, "item": item });
            let (status, text) = self.call(base, uri, "claim", body).await;
            assert_eq!(status, 200, "{text}");
        }
    }

    /// mike, the owner, allows the GitHub accounts `owners`.
    async fn allow(&self, base: &str, owners: &[&str]) {
        for owner in owners {
            let body = json!({ "me": "riff://mike@pangolin", "owner": owner, "allowed": true });
            let (status, text) = self.call(base, PERSON, "forge/allow", body).await;
            assert_eq!(status, 200, "{text}");
        }
    }

    async fn forge(&self, base: &str, uri: &str) -> (u16, String) {
        let me = uri.split('#').next().unwrap();
        self.call(base, uri, "forge/token", json!({ "me": me }))
            .await
    }

    async fn forge_token(&self, base: &str, uri: &str) -> ForgeTokenReply {
        let (status, text) = self.forge(base, uri).await;
        assert_eq!(status, 200, "{text}");
        serde_json::from_str(&text).unwrap()
    }
}

fn asked_rights(role: TokenRole) -> Value {
    serde_json::to_value(permissions(role)).unwrap()
}

#[tokio::test]
async fn each_role_gets_its_rights_on_the_repository_of_its_session_only() {
    let github = FakeGitHub::start(INSTALLS).await;
    let (service, base) = start(&github).await;
    let mike = Mike::sign_in(&service).await;
    mike.allow(&base, &["acme"]).await;
    mike.resume(&base).await;
    mike.register(&base, LEAD, false).await;
    mike.register(&base, WORKER, true).await;
    mike.register(&base, VERIFIER, true).await;

    for (uri, role) in [
        (PERSON, TokenRole::Lead),
        (LEAD, TokenRole::Lead),
        (WORKER, TokenRole::Worker),
        (VERIFIER, TokenRole::Verifier),
    ] {
        let token = mike.forge_token(&base, uri).await;
        assert_eq!(token.role, role, "{uri}");
        assert_eq!(token.repo, "acme/app");
        let asked = github.asked().pop().unwrap();
        assert_eq!(asked.token, token.token);
        assert_eq!(asked.installation, 11);
        assert_eq!(asked.repositories, ["app"], "one repository only");
        assert_eq!(asked.permissions, asked_rights(role), "{uri}");
        assert!(token.ends_ms > 0);
    }
}

#[tokio::test]
async fn a_session_of_each_account_gets_a_token_of_its_own_installation_and_repository() {
    let github = FakeGitHub::start(INSTALLS).await;
    let (service, base) = start(&github).await;
    let mike = Mike::sign_in(&service).await;
    mike.allow(&base, &["acme", "mike"]).await;
    for (uri, repo, installation, name) in [
        (LIB, "acme/lib", 11, "lib"),
        (TOOLS, "mike/tools", 22, "tools"),
    ] {
        mike.register(&base, uri, true).await;
        let token = mike.forge_token(&base, uri).await;
        assert_eq!((token.role, token.repo.as_str()), (TokenRole::Worker, repo));
        let asked = github.asked().pop().unwrap();
        assert_eq!(asked.installation, installation, "{uri}");
        assert_eq!(asked.repositories, [name], "{uri}");
    }
}

#[tokio::test]
async fn the_server_takes_the_repository_from_its_facts_not_from_the_call() {
    let github = FakeGitHub::start(INSTALLS).await;
    let (service, base) = start(&github).await;
    let mike = Mike::sign_in(&service).await;
    mike.allow(&base, &["acme", "mike"]).await;
    mike.register(&base, TOOLS, true).await;
    // The same session names another repository in the call.
    let body = json!({ "me": "riff://mike@pangolin/acme/app?session=t" });
    let (status, text) = mike.call(&base, TOOLS, "forge/token", body).await;
    assert_eq!(status, 200, "{text}");
    let token: ForgeTokenReply = serde_json::from_str(&text).unwrap();
    assert_eq!(token.repo, "mike/tools");
    assert_eq!(github.asked().pop().unwrap().repositories, ["tools"]);
}

#[tokio::test]
async fn a_repository_with_no_installation_names_riff_forge_install() {
    let github = FakeGitHub::start(INSTALLS).await;
    let (service, base) = start(&github).await;
    let mike = Mike::sign_in(&service).await;
    mike.allow(&base, &["stranger"]).await;
    mike.register(&base, OTHER, true).await;
    let (status, text) = mike.forge(&base, OTHER).await;
    assert_eq!(status, 409);
    assert!(text.contains("riff forge install stranger"), "{text}");
    assert!(github.asked().is_empty());
}

#[tokio::test]
async fn a_riff_with_no_sign_in_gives_no_forge_token() {
    let github = FakeGitHub::start(INSTALLS).await;
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let forge = riff_server::forge::Settings::of(Some("7"), Some(KEY), Some(&github.url)).unwrap();
    let service = Service::new(Config {
        forge,
        ..Config::new(&url)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let reply = common::client()
        .post(format!("{url}/v1/forge/token"))
        .json(&json!({ "me": WORKER.split('#').next().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 403);
    assert!(reply.text().await.unwrap().contains("no sign-in"));
    assert!(github.asked().is_empty());
}

#[tokio::test]
async fn a_server_with_no_app_gives_no_forge_token() {
    let (service, base) = common::start(true, &[]).await;
    let mike = Mike::sign_in(&service).await;
    mike.register(&base, LIB, true).await;
    let (status, text) = mike.forge(&base, LIB).await;
    assert_eq!(status, 409);
    assert!(text.contains("riff forge create"), "{text}");
}

#[tokio::test]
async fn a_change_of_claim_revokes_the_old_token() {
    let github = FakeGitHub::start(INSTALLS).await;
    let (service, base) = start(&github).await;
    let mike = Mike::sign_in(&service).await;
    mike.allow(&base, &["acme"]).await;
    mike.resume(&base).await;
    mike.register(&base, LEAD, false).await;
    mike.register(&base, WORKER, true).await;
    let worker = mike.forge_token(&base, WORKER).await;
    let lead = mike.forge_token(&base, PERSON).await;
    assert_eq!(worker.role, TokenRole::Worker);

    // The worker session claims a verify: its role is now the verifier.
    // The server revokes its worker token by itself.
    let me = WORKER.split('#').next().unwrap();
    let claim = json!({ "me": me, "thread": "acme/app", "item": "verify-issue-9" });
    let (status, text) = mike.call(&base, WORKER, "claim", claim).await;
    assert_eq!(status, 200, "{text}");
    let span = isolated::Span::start();
    while !github.revoked().contains(&worker.token) {
        assert!(span.within(Duration::from_secs(20)), "no revoke");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // The next token of the session has the rights of the verifier.
    let verifier = mike.forge_token(&base, WORKER).await;
    assert_eq!(verifier.role, TokenRole::Verifier);

    // At the release, the verifier token goes too.
    let release = json!({ "me": me, "thread": "acme/app", "item": "verify-issue-9" });
    let (status, text) = mike.call(&base, WORKER, "release", release).await;
    assert_eq!(status, 200, "{text}");
    let span = isolated::Span::start();
    while !github.revoked().contains(&verifier.token) {
        assert!(span.within(Duration::from_secs(20)), "no revoke");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // The token of the lead stays: it has no claims.
    assert!(!github.revoked().contains(&lead.token));
}

#[tokio::test]
async fn the_end_of_a_session_revokes_its_token() {
    let github = FakeGitHub::start(INSTALLS).await;
    let (service, base) = start(&github).await;
    let mike = Mike::sign_in(&service).await;
    mike.allow(&base, &["acme"]).await;
    mike.register(&base, LIB, true).await;
    let token = mike.forge_token(&base, LIB).await;
    let (status, text) = mike.call(&base, LIB, "end", json!({ "me": LIB })).await;
    assert_eq!(status, 200, "{text}");
    let span = isolated::Span::start();
    while !github.revoked().contains(&token.token) {
        assert!(span.within(Duration::from_secs(20)), "no revoke");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn riff_forge_check_makes_a_token_of_each_role_and_revokes_it() {
    let github = FakeGitHub::start(INSTALLS).await;
    let (service, base) = start(&github).await;
    let mike = Mike::sign_in(&service).await;
    mike.allow(&base, &["acme"]).await;
    mike.register(&base, LEAD, false).await;
    let (status, text) = mike
        .call(&base, PERSON, "forge/check", json!({ "me": PERSON }))
        .await;
    assert_eq!(status, 200, "{text}");
    assert!(!text.contains("ghs_"), "the reply holds no token: {text}");
    let check: ForgeCheckReply = serde_json::from_str(&text).unwrap();
    assert_eq!((check.repo.as_str(), check.app), ("acme/app", 7));
    let roles: Vec<TokenRole> = check.roles.iter().map(|r| r.role).collect();
    assert_eq!(roles, TokenRole::ALL);
    assert!(check.roles.iter().all(|r| r.error.is_none()));
    let made: Vec<String> = github.asked().into_iter().map(|a| a.token).collect();
    assert_eq!(made.len(), 3);
    assert_eq!(github.revoked(), made, "each token of the check is revoked");

    // An App with more rights than a role fails the check of each role.
    github.add_to_each_token("administration", "write");
    let (status, text) = mike
        .call(&base, PERSON, "forge/check", json!({ "me": PERSON }))
        .await;
    assert_eq!(status, 200, "{text}");
    let check: ForgeCheckReply = serde_json::from_str(&text).unwrap();
    for role in &check.roles {
        let error = role.error.as_deref().unwrap_or_default();
        assert!(error.contains("administration"), "{error}");
    }
    assert_eq!(
        github.revoked().len(),
        6,
        "a token with too many rights ends at once"
    );
}

#[tokio::test]
async fn a_session_that_the_server_does_not_know_gets_no_token() {
    let github = FakeGitHub::start(INSTALLS).await;
    let (service, base) = start(&github).await;
    let mike = Mike::sign_in(&service).await;
    mike.allow(&base, &["acme", "mike"]).await;
    // The session never registered, or it ended.
    let (status, text) = mike.forge(&base, LIB).await;
    assert_eq!(status, 403, "{text}");
    assert!(text.contains("does not know this session"), "{text}");
    mike.register(&base, TOOLS, true).await;
    let (status, text) = mike.call(&base, TOOLS, "end", json!({ "me": TOOLS })).await;
    assert_eq!(status, 200, "{text}");
    let (status, text) = mike.forge(&base, TOOLS).await;
    assert_eq!(status, 403, "{text}");
    assert!(github.asked().is_empty());
}

#[tokio::test]
async fn a_person_gets_a_lead_token_only_for_a_repository_where_it_has_a_lead() {
    let github = FakeGitHub::start(&[("acme/app", 11), ("stranger/app", 33)]).await;
    let (service, base) = start(&github).await;
    let mike = Mike::sign_in(&service).await;
    mike.allow(&base, &["acme", "stranger"]).await;
    // No lead yet: the wrapper of the lead gets no token.
    let (status, text) = mike.forge(&base, PERSON).await;
    assert_eq!(status, 403, "{text}");
    assert!(text.contains("not the lead of acme/app"), "{text}");
    mike.register(&base, LEAD, false).await;
    assert_eq!(mike.forge_token(&base, PERSON).await.role, TokenRole::Lead);
    // mike leads acme/app, not stranger/app.
    let stranger = "riff://mike@pangolin/stranger/app";
    let (status, text) = mike.forge(&base, stranger).await;
    assert_eq!(status, 403, "{text}");
    assert!(text.contains("not the lead of stranger/app"), "{text}");
    let (status, _) = mike
        .call(&base, stranger, "forge/check", json!({ "me": stranger }))
        .await;
    assert_eq!(status, 403);
    assert_eq!(
        github.asked().len(),
        1,
        "no call to GitHub for stranger/app"
    );
}

#[tokio::test]
async fn only_an_allowed_account_gets_tokens() {
    let github = FakeGitHub::start(&[("acme/app", 11), ("stranger/app", 33)]).await;
    let (service, base) = start(&github).await;
    let mike = Mike::sign_in(&service).await;
    // A stranger installed the public App on stranger/app.
    mike.register(&base, OTHER, true).await;
    let (status, text) = mike.forge(&base, OTHER).await;
    assert_eq!(status, 403, "{text}");
    assert!(text.contains("riff forge allow stranger"), "{text}");
    let (status, text) = mike
        .call(&base, OTHER, "forge/check", json!({ "me": OTHER }))
        .await;
    assert_eq!(status, 403, "{text}");
    assert!(github.asked().is_empty());

    // The owner allows the account: the session gets its token.
    mike.allow(&base, &["Stranger"]).await;
    let token = mike.forge_token(&base, OTHER).await;
    assert_eq!(token.repo, "stranger/app");

    // The list, and the end of the allow.
    let list = json!({ "me": "riff://mike@pangolin" });
    let (status, text) = mike.call(&base, PERSON, "forge/allow", list).await;
    assert_eq!(
        (status, text.as_str()),
        (200, r#"{"accounts":["stranger"]}"#)
    );
    let off = json!({ "me": "riff://mike@pangolin", "owner": "stranger", "allowed": false });
    let (status, text) = mike.call(&base, PERSON, "forge/allow", off).await;
    assert_eq!((status, text.as_str()), (200, r#"{"accounts":[]}"#));
    let span = isolated::Span::start();
    while !github.revoked().contains(&token.token) {
        assert!(span.within(Duration::from_secs(20)), "no revoke");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (status, _) = mike.forge(&base, OTHER).await;
    assert_eq!(status, 403);

    // A session cannot allow an account: only a person, the owner or
    // an admin.
    let body = json!({ "me": OTHER, "owner": "stranger", "allowed": true });
    let (status, text) = mike.call(&base, OTHER, "forge/allow", body).await;
    assert_eq!(status, 403, "{text}");
}
