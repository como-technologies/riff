//! The GitHub Apps of the riff over HTTP, with a fake GitHub
//! (01M6WG52J7WTP079W1AYEQ4W08, 01M6WG52MWZB1SVQMMH6BKJSJ5,
//! 01M6WG52QHC8E24RJ33ZPVHZSJ, 01M6WG52T5PAD9NGQ597CP56K7,
//! 01M6WG52WSWM8A9CN6X448AGAJ, 01M6WG534PPE2JW8JQFRJDEK86): the two
//! manifests, the swap, the store of the keys, the states, the roles of
//! the callers and the check. The webhook is in `hooks.rs`.

use crate::common;
use crate::fake_github::{
    FakeGitHub, KEY, LEAD_CODE, OWNER, REPO, SESSION_CODE, both_apps, installation_of,
    lead_webhook_secret, webhook_secret,
};

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::routing::{get, post};
use riff_core::dpop::Key;
use riff_core::wire::{
    GitHubApp, GitHubAppsReply, GitHubChecked, GitHubOwner, GitHubRole, GitHubStarted, SessionToken,
};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::github::{STATE_LIFE, Settings};
use riff_server::keys::{Apps, At, FILE};
use serde_json::{Value, json};

/// The wait of the tests before the revoke of the older tokens of an
/// install with new rights.
const REVOKE_AFTER: Duration = Duration::from_secs(1);

/// A riff with sign-in whose GitHub is `fake`, with the keys at `keys`
/// and states good for `life`.
async fn serve(fake: &FakeGitHub, keys: At, life: Duration) -> (Service, String) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        require_sign_in: true,
        github: Settings {
            api: fake.url.clone(),
            web: fake.url.clone(),
            keys,
            state_life: life,
            revoke_after: REVOKE_AFTER,
            ..Settings::default()
        },
        ..Config::new(&url)
    };
    let service = Service::new(config);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// A person who signed in.
struct Person {
    key: Key,
    token: String,
}

/// Signs in `email`. The first person of a riff is its owner; each
/// other one is a member.
async fn person(service: &Service, email: &str) -> Person {
    let key = Key::generate();
    let pair = service.admit(email, true, &key.thumbprint()).await.unwrap();
    Person {
        key,
        token: pair.access_token,
    }
}

/// `POST PATH` of `person` with `body`: the status and the text.
async fn call(url: &str, person: &Person, path: &str, body: Value) -> (u16, String) {
    let reply = common::post(&format!("{url}{path}"), &person.key, Some(&person.token))
        .json(&body)
        .send()
        .await
        .unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

/// `GET URL` as a browser: the status and the page.
async fn browse(url: &str) -> (u16, String) {
    let reply = common::client().get(url).send().await.unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

/// `text` with the HTML escapes of riff-server undone.
fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// The value of the attribute `name` in the first tag of `page` that
/// has it.
fn attribute(page: &str, name: &str) -> String {
    let start = format!("{name}=\"");
    let at = page
        .find(&start)
        .unwrap_or_else(|| panic!("no {name} in {page}"))
        + start.len();
    let end = page[at..].find('"').unwrap();
    unescape(&page[at..at + end])
}

/// The form of a manifest page: the action and the manifest.
fn form(page: &str) -> (String, Value) {
    let action = attribute(page, "action");
    let manifest = serde_json::from_str(&attribute(page, "value")).unwrap();
    (action, manifest)
}

/// The state in the action of a manifest page.
fn state_of(action: &str) -> String {
    action.split_once("state=").unwrap().1.to_owned()
}

/// What the browser saw in `riff github create`: each page in order,
/// and the state of each App.
struct Flow {
    start: String,
    pages: Vec<String>,
    states: Vec<String>,
}

/// Runs `riff github create --owner acme` as `owner` and the browser:
/// the start page, then GitHub sends the browser back with the code of
/// each App.
async fn make_apps(url: &str, owner: &Person) -> Flow {
    let create = json!({ "owner": OWNER, "organization": true, "owner_id": 7 });
    let (status, text) = call(url, owner, "/v1/github/create", create).await;
    assert_eq!(status, 200, "{text}");
    let started: GitHubStarted = serde_json::from_str(&text).unwrap();
    let (status, start) = browse(&started.url).await;
    assert_eq!(status, 200, "{start}");
    let mut pages = vec![start.clone()];
    let mut states = Vec::new();
    for code in [SESSION_CODE, LEAD_CODE] {
        let (action, _) = form(pages.last().unwrap());
        let state = state_of(&action);
        let (status, page) =
            browse(&format!("{url}/github/created?code={code}&state={state}")).await;
        assert_eq!(status, 200, "{page}");
        states.push(state);
        pages.push(page);
    }
    Flow {
        start: started.url,
        pages,
        states,
    }
}

/// The rights of a map of `NAME: LEVEL` pairs.
fn rights(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, level)| ((*name).to_owned(), (*level).to_owned()))
        .collect()
}

/// The rights of the token of each role, from the table of the design.
fn table(role: GitHubRole) -> BTreeMap<String, String> {
    match role {
        GitHubRole::Worker => rights(&[
            ("metadata", "read"),
            ("contents", "write"),
            ("pull_requests", "write"),
            ("issues", "write"),
            ("statuses", "read"),
            ("checks", "read"),
            ("actions", "write"),
            ("workflows", "write"),
        ]),
        GitHubRole::Verifier => rights(&[
            ("metadata", "read"),
            ("contents", "read"),
            ("pull_requests", "read"),
            ("issues", "read"),
            ("statuses", "read"),
            ("checks", "read"),
            ("actions", "read"),
        ]),
        GitHubRole::Lead => rights(&[
            ("metadata", "read"),
            ("contents", "write"),
            ("pull_requests", "write"),
            ("issues", "write"),
            ("statuses", "write"),
            ("checks", "read"),
            ("actions", "write"),
            ("security_events", "write"),
            ("secret_scanning_alerts", "write"),
            ("vulnerability_alerts", "read"),
        ]),
    }
}

/// The rights of the lead on the alerts (01M6ZEJ4NEZGQFV3Q4BER5AW3S).
const ALERTS: [&str; 3] = [
    "security_events",
    "secret_scanning_alerts",
    "vulnerability_alerts",
];

/// The rights of the lead with no right on the alerts: the rights of a
/// lead App that riff made before them (01M6ZHWVPD7MN6GH6GS0GVETHA).
fn no_alerts() -> BTreeMap<String, String> {
    let mut lead = table(GitHubRole::Lead);
    lead.retain(|name, _| !ALERTS.contains(&name.as_str()));
    lead
}

/// The rights that no App asks for.
const NEVER: [&str; 6] = [
    "administration",
    "deployments",
    "environments",
    "secrets",
    "variables",
    "packages",
];

/// 01M6WG52MWZB1SVQMMH6BKJSJ5, 01M6WG52QHC8E24RJ33ZPVHZSJ,
/// 01M6ZEJ4NEZGQFV3Q4BER5AW3S, 01M74MB1A1TV4WBZ07AFCEP5WG: the manifest of each App has
/// only the rights of its roles and the webhook, and only the one of the
/// session App has the events of the riff.
#[tokio::test]
async fn each_manifest_has_only_the_rights_of_its_roles() {
    let fake = FakeGitHub::start().await;
    let (service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let flow = make_apps(&url, &owner).await;

    let (action, session) = form(&flow.pages[0]);
    assert!(
        action.starts_with(&format!(
            "{}/organizations/acme/settings/apps/new?state=",
            fake.url
        )),
        "{action}"
    );
    assert_eq!(session["name"], "riff-acme");
    assert_eq!(session["public"], true);
    assert_eq!(session["redirect_url"], format!("{url}/github/created"));
    let mut highest = table(GitHubRole::Worker);
    highest.insert("merge_queues".into(), "read".into());
    assert_eq!(session["default_permissions"], json!(highest));
    assert_eq!(
        session["hook_attributes"]["url"],
        format!("{url}/hooks/github")
    );
    assert_eq!(
        session["default_events"],
        json!([
            "issues",
            "issue_comment",
            "pull_request",
            "check_run",
            "status",
            "workflow_run",
            "merge_group"
        ])
    );

    let (action, lead) = form(&flow.pages[1]);
    assert!(
        action.contains("/organizations/acme/settings/apps/new?state="),
        "{action}"
    );
    assert_eq!(lead["name"], "riff-acme-lead");
    assert_eq!(lead["default_permissions"], json!(table(GitHubRole::Lead)));
    // 01M6ZEJ4NEZGQFV3Q4BER5AW3S: the lead reads the alerts, and
    // dismisses a false positive. No worker and no verifier does.
    for name in ALERTS {
        assert!(lead["default_permissions"].get(name).is_some(), "{lead}");
        let asked = session["default_permissions"].get(name);
        assert!(asked.is_none(), "{name}: {session}");
    }
    // 01M74MB1A1TV4WBZ07AFCEP5WG: the lead App has the same webhook, and asks for no
    // event: GitHub sends it the event `installation`.
    assert_eq!(
        lead["hook_attributes"]["url"],
        format!("{url}/hooks/github")
    );
    assert!(lead.get("default_events").is_none(), "{lead}");

    for manifest in [&session, &lead] {
        let asked = manifest["default_permissions"].as_object().unwrap();
        for right in NEVER {
            assert!(!asked.contains_key(right), "{right} in {manifest}");
        }
    }
    // A personal account gets the page of the account.
    let create = json!({ "owner": "ada", "organization": false });
    let (_, text) = call(&url, &owner, "/v1/github/create", create).await;
    let started: GitHubStarted = serde_json::from_str(&text).unwrap();
    let (_, page) = browse(&started.url).await;
    let (action, _) = form(&page);
    assert!(
        action.starts_with(&format!("{}/settings/apps/new?state=", fake.url)),
        "{action}"
    );
}

/// 01M6WG52WSWM8A9CN6X448AGAJ: the swap puts the ID, the slug, the key,
/// the webhook secret and the bot ID of each App in the file of the
/// server, with mode `0600`. The last page links to the install page of
/// each App on the account.
#[tokio::test]
async fn the_swap_keeps_the_keys_in_the_store_of_the_server() {
    use std::os::unix::fs::PermissionsExt;
    let fake = FakeGitHub::start().await;
    let dir = tempfile::tempdir().unwrap();
    let (service, url) = serve(&fake, At::Dir(dir.path().to_owned()), STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let flow = make_apps(&url, &owner).await;
    assert_eq!(fake.seen().swapped, [SESSION_CODE, LEAD_CODE]);

    let path = dir.path().join(FILE);
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    let kept: Apps = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(kept, both_apps());

    let done = &flow.pages[2];
    for slug in ["riff-acme", "riff-acme-lead"] {
        let link = format!(
            "{}/apps/{slug}/installations/new/permissions?target_id=7",
            fake.url
        );
        assert!(done.contains(&link), "{link} not in {done}");
    }

    // A new server on the same folder has the Apps.
    let (service, url) = serve(&fake, At::Dir(dir.path().to_owned()), STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let (status, text) = call(&url, &owner, "/v1/github/apps", json!({})).await;
    assert_eq!(status, 200, "{text}");
    let apps: GitHubAppsReply = serde_json::from_str(&text).unwrap();
    let bots: Vec<_> = apps
        .apps
        .iter()
        .map(|a| (a.bot.as_str(), a.bot_id))
        .collect();
    assert_eq!(
        bots,
        [
            ("riff-acme[bot]", Some(1101)),
            ("riff-acme-lead[bot]", Some(1202))
        ]
    );
}

/// The lines that the log of the test wrote, as JSON lines.
#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Lines {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// 01M6WG52WSWM8A9CN6X448AGAJ: no reply and no log line has the key, the
/// webhook secret or a token: not the pages, not the JSON replies, not
/// a line of the log, also of a check.
#[tokio::test]
async fn no_reply_and_no_log_line_has_the_key() {
    // The log of this thread misses lines while other tests run.
    if !isolated::rerun_alone(module_path!(), "no_reply_and_no_log_line_has_the_key") {
        return;
    }
    let lines = Lines::default();
    let writer = lines.clone();
    let subscriber = tracing_subscriber::fmt()
        .event_format(riff_server::logline::JsonLines)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _log = tracing::subscriber::set_default(subscriber);

    let fake = FakeGitHub::start().await;
    fake.seen().install_on_swap = true;
    let dir = tempfile::tempdir().unwrap();
    let (service, url) = serve(&fake, At::Dir(dir.path().to_owned()), STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let flow = make_apps(&url, &owner).await;
    let mut replies = flow.pages.clone();
    replies.push(flow.start.clone());
    for (path, body) in [
        ("/v1/github/apps", json!({ "owner": OWNER })),
        ("/v1/github/check", json!({ "repo": REPO })),
        (
            "/v1/github/create",
            json!({ "owner": OWNER, "organization": true }),
        ),
    ] {
        let (status, text) = call(&url, &owner, path, body).await;
        assert_eq!(status, 200, "{path}: {text}");
        replies.push(text);
    }
    // A used state gives a page of its own.
    let used = format!(
        "{url}/github/created?code={SESSION_CODE}&state={}",
        flow.states[0]
    );
    replies.push(browse(&used).await.1);

    let log = String::from_utf8(lines.0.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("riff github create"),
        "the log has the lines: {log}"
    );
    assert!(log.contains("riff github check"), "{log}");
    // A line of the body of the key: each line of it is secret.
    let key_line = KEY.lines().nth(5).unwrap();
    let tokens = fake.seen().tokens.clone();
    assert_eq!(tokens.len(), 3);
    for reply in replies.iter().chain([&log]) {
        assert!(!reply.contains(key_line), "the key in: {reply}");
        assert!(!reply.contains("PRIVATE KEY"), "the key in: {reply}");
        assert!(!reply.contains(webhook_secret()), "the secret in: {reply}");
        assert!(
            !reply.contains(lead_webhook_secret()),
            "the secret in: {reply}"
        );
        for token in &tokens {
            assert!(!reply.contains(token.as_str()), "a token in: {reply}");
        }
    }
}

/// 01M6WG52T5PAD9NGQ597CP56K7: riff-server refuses a state that it did
/// not make, a used one and an old one, on the start page and on the
/// way back from GitHub. A refused state swaps no code.
#[tokio::test]
async fn a_wrong_an_old_or_a_used_state_is_refused() {
    let fake = FakeGitHub::start().await;
    let (service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;

    for page in [
        format!("{url}/github/start?state=made-up"),
        format!("{url}/github/created?code={SESSION_CODE}&state=made-up"),
        format!("{url}/github/created?code={SESSION_CODE}"),
    ] {
        let (status, text) = browse(&page).await;
        assert_eq!(status, 400, "{page}: {text}");
        assert!(
            text.contains("riff-server did not make this state"),
            "{text}"
        );
    }
    assert!(fake.seen().swapped.is_empty());

    let flow = make_apps(&url, &owner).await;
    for (state, code) in flow.states.iter().zip([SESSION_CODE, LEAD_CODE]) {
        let again = format!("{url}/github/created?code={code}&state={state}");
        let (status, text) = browse(&again).await;
        assert_eq!(status, 400, "{text}");
        assert!(text.contains("this state was used"), "{text}");
        let start = format!("{url}/github/start?state={state}");
        assert_eq!(browse(&start).await.0, 400);
    }
    assert_eq!(fake.seen().swapped, [SESSION_CODE, LEAD_CODE]);

    // A state of a short life is old soon.
    let (service, url) = serve(&fake, At::Memory, Duration::from_millis(100)).await;
    let owner = person(&service, "ada@example.com").await;
    let create = json!({ "owner": OWNER, "organization": true });
    let (_, text) = call(&url, &owner, "/v1/github/create", create).await;
    let started: GitHubStarted = serde_json::from_str(&text).unwrap();
    let state = state_of(&started.url);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (status, text) = browse(&started.url).await;
    assert_eq!(status, 400, "{text}");
    assert!(
        text.contains("this state is older than 10 minutes"),
        "{text}"
    );
    let back = format!("{url}/github/created?code={SESSION_CODE}&state={state}");
    let (status, text) = browse(&back).await;
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("older than"), "{text}");
    assert_eq!(fake.seen().swapped, [SESSION_CODE, LEAD_CODE]);
}

/// 01M6WG52T5PAD9NGQ597CP56K7, 01M6WPV8473EBABR1ZMEPZMTQQ: a member who
/// is not an admin cannot make the Apps, or ask whether they are
/// installed on an account. Each person can see the Apps with no
/// account, and check them.
#[tokio::test]
async fn a_member_who_is_not_the_owner_or_an_admin_is_refused() {
    let fake = FakeGitHub::start().await;
    let (service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let member = person(&service, "bob@example.com").await;
    make_apps(&url, &owner).await;
    let (status, text) = call(&url, &member, "/v1/github/apps", json!({})).await;
    assert_eq!(status, 200, "{text}");
    let apps: GitHubAppsReply = serde_json::from_str(&text).unwrap();
    let bots: Vec<(&str, Option<bool>)> = apps
        .apps
        .iter()
        .map(|a| (a.bot.as_str(), a.installed))
        .collect();
    assert_eq!(
        bots,
        [("riff-acme[bot]", None), ("riff-acme-lead[bot]", None)]
    );
    assert!(!text.contains("PRIVATE KEY"), "{text}");
    let create = json!({ "owner": OWNER, "organization": true });
    let (status, text) = call(&url, &member, "/v1/github/create", create).await;
    assert_eq!(status, 403, "{text}");
    assert!(
        text.contains("only an admin of the riff runs riff github create"),
        "{text}"
    );
    let (status, text) = call(&url, &member, "/v1/github/apps", json!({ "owner": OWNER })).await;
    assert_eq!(status, 403, "{text}");
    // 01M6Z7DMYAW52X62TN1DEX9NCG: nor with a repository.
    let (status, text) = call(&url, &member, "/v1/github/apps", json!({ "repo": REPO })).await;
    assert_eq!(status, 403, "{text}");
    let (status, text) = call(&url, &member, "/v1/github/check", json!({ "repo": REPO })).await;
    assert_eq!(status, 200, "{text}");
    // A call with no token gets 401.
    let reply = common::client()
        .post(format!("{url}/v1/github/create"))
        .json(&json!({ "owner": OWNER, "organization": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 401);
}

/// 01M6WG534PPE2JW8JQFRJDEK86: the check asks for a token of each role
/// for the one repository, with the rights of the role, gives the bot
/// and the rights of each, and revokes each token at once.
#[tokio::test]
async fn the_check_gives_the_bot_and_the_rights_of_each_role() {
    let fake = FakeGitHub::start().await;
    fake.seen().install_on_swap = true;
    let (service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    make_apps(&url, &owner).await;
    let (status, text) = call(&url, &owner, "/v1/github/check", json!({ "repo": REPO })).await;
    assert_eq!(status, 200, "{text}");
    let checked: GitHubChecked = serde_json::from_str(&text).unwrap();
    let roles: Vec<_> = checked
        .roles
        .iter()
        .map(|r| (r.role, r.bot.clone().unwrap(), r.rights.clone(), r.passed()))
        .collect();
    let row = |role, bot: &str| (role, bot.to_owned(), table(role), true);
    assert_eq!(
        roles,
        [
            row(GitHubRole::Worker, "riff-acme[bot]"),
            row(GitHubRole::Verifier, "riff-acme[bot]"),
            row(GitHubRole::Lead, "riff-acme-lead[bot]"),
        ]
    );
    let seen = fake.seen();
    let asked: Vec<_> = seen
        .asked
        .iter()
        .map(|(id, body)| (*id, body.clone()))
        .collect();
    let body = |role| json!({ "repositories": ["app"], "permissions": table(role) });
    assert_eq!(
        asked,
        [
            (101, body(GitHubRole::Worker)),
            (101, body(GitHubRole::Verifier)),
            (202, body(GitHubRole::Lead)),
        ]
    );
    assert_eq!(seen.revoked, seen.tokens);
}

/// 01M6WG534PPE2JW8JQFRJDEK86: the check names a right that GitHub did
/// not give, a right more, and a repository with no installation.
#[tokio::test]
async fn the_check_names_each_wrong_right_and_a_missing_installation() {
    let fake = FakeGitHub::start().await;
    let (service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let (_, text) = call(&url, &owner, "/v1/github/check", json!({ "repo": REPO })).await;
    let checked: GitHubChecked = serde_json::from_str(&text).unwrap();
    let problems: Vec<_> = checked
        .roles
        .iter()
        .map(|r| r.problem.clone().unwrap())
        .collect();
    assert_eq!(
        problems,
        [
            "the riff has no session App: riff github create --owner acme",
            "the riff has no session App: riff github create --owner acme",
            "the riff has no lead App: riff github create --owner acme",
        ]
    );

    make_apps(&url, &owner).await;
    let (_, text) = call(&url, &owner, "/v1/github/check", json!({ "repo": REPO })).await;
    let checked: GitHubChecked = serde_json::from_str(&text).unwrap();
    assert_eq!(
        checked.roles[0].problem.as_deref(),
        Some("riff-acme is not installed on acme/app: riff github install acme")
    );

    {
        let mut seen = fake.seen();
        seen.installed = true;
        seen.give = Some(rights(&[
            ("metadata", "read"),
            ("contents", "read"),
            ("secrets", "read"),
        ]));
    }
    let (_, text) = call(&url, &owner, "/v1/github/check", json!({ "repo": REPO })).await;
    let checked: GitHubChecked = serde_json::from_str(&text).unwrap();
    let worker = &checked.roles[0];
    assert!(!worker.passed());
    assert!(
        worker.missing.contains(&"contents: write".to_owned()),
        "{worker:?}"
    );
    assert!(
        worker.extra.contains(&"secrets: read".to_owned()),
        "{worker:?}"
    );
    assert!(
        worker.extra.contains(&"contents: read".to_owned()),
        "{worker:?}"
    );
    {
        let seen = fake.seen();
        assert_eq!(seen.revoked, seen.tokens);
    }
    let (status, _) = call(
        &url,
        &owner,
        "/v1/github/check",
        json!({ "repo": "no repo" }),
    )
    .await;
    assert_eq!(status, 400);
}

/// `POST /v1/github/apps` says whether each App is installed on the
/// account.
#[tokio::test]
async fn the_apps_say_whether_each_is_installed() {
    let fake = FakeGitHub::start().await;
    let (service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let installed = |text: &str| {
        let apps: GitHubAppsReply = serde_json::from_str(text).unwrap();
        apps.apps
            .iter()
            .map(|a| (a.slug.clone(), a.installed))
            .collect::<Vec<_>>()
    };
    let (_, text) = call(&url, &owner, "/v1/github/apps", json!({ "owner": OWNER })).await;
    assert!(installed(&text).is_empty());
    make_apps(&url, &owner).await;
    let (_, text) = call(&url, &owner, "/v1/github/apps", json!({ "owner": OWNER })).await;
    assert_eq!(
        installed(&text),
        [
            ("riff-acme".into(), Some(false)),
            ("riff-acme-lead".into(), Some(false))
        ]
    );
    fake.seen().installed = true;
    let (_, text) = call(&url, &owner, "/v1/github/apps", json!({ "owner": OWNER })).await;
    assert_eq!(
        installed(&text),
        [
            ("riff-acme".into(), Some(true)),
            ("riff-acme-lead".into(), Some(true))
        ]
    );
    let (_, text) = call(&url, &owner, "/v1/github/apps", json!({})).await;
    assert_eq!(
        installed(&text),
        [("riff-acme".into(), None), ("riff-acme-lead".into(), None)]
    );
}

/// 01M6Z7DMYAW52X62TN1DEX9NCG: `POST /v1/github/apps` with a repository
/// gives the ID of the install of each App on the account, and whether
/// the install has the repository. A repository that is no `OWNER/NAME`
/// gets 400.
#[tokio::test]
async fn the_apps_say_whether_each_install_has_the_repository() {
    let fake = FakeGitHub::start().await;
    let (service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    make_apps(&url, &owner).await;
    /// The install and whether it has `repo`, of each App.
    async fn has(url: &str, owner: &Person, repo: &str) -> Vec<(Option<u64>, Option<bool>)> {
        let body = json!({ "owner": OWNER, "repo": repo });
        let (status, text) = call(url, owner, "/v1/github/apps", body).await;
        assert_eq!(status, 200, "{text}");
        let apps: GitHubAppsReply = serde_json::from_str(&text).unwrap();
        apps.apps
            .iter()
            .map(|a| (a.installation, a.has_repo))
            .collect()
    }
    assert_eq!(
        has(&url, &owner, "acme/b").await,
        [(None, Some(false)), (None, Some(false))]
    );
    // Both Apps are installed on acme with only acme/app.
    fake.seen().installed = true;
    assert_eq!(
        has(&url, &owner, "acme/b").await,
        [(Some(1010), Some(false)), (Some(2020), Some(false))]
    );
    fake.seen().added.insert(101, vec!["acme/b".into()]);
    assert_eq!(
        has(&url, &owner, "acme/b").await,
        [(Some(1010), Some(true)), (Some(2020), Some(false))]
    );
    assert_eq!(
        has(&url, &owner, REPO).await,
        [(Some(1010), Some(true)), (Some(2020), Some(true))]
    );
    let (_, text) = call(&url, &owner, "/v1/github/apps", json!({ "owner": OWNER })).await;
    let apps: GitHubAppsReply = serde_json::from_str(&text).unwrap();
    assert!(apps.apps.iter().all(|a| a.has_repo.is_none()), "{text}");
    let body = json!({ "owner": OWNER, "repo": "no repo" });
    let (status, _) = call(&url, &owner, "/v1/github/apps", body).await;
    assert_eq!(status, 400);
}

/// 01M74FVDCJT8P0670P46MWR1MJ: `POST /v1/github/apps` with an account
/// compares the rights of each App and of its install with the one list
/// of riff, the rights of the manifest. It gives each right that the App
/// must set, `none` for a right more, each right that the install has
/// not accepted, and the account of the App. With no account, it reads
/// no right.
#[tokio::test]
async fn the_apps_say_which_rights_each_app_and_each_install_lacks() {
    let fake = FakeGitHub::start().await;
    let (service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let flow = make_apps(&url, &owner).await;
    for (page, app) in flow.pages.iter().zip(GitHubApp::ALL) {
        let (_, manifest) = form(page);
        assert_eq!(
            serde_json::to_value(riff_server::github::app_rights(app)).unwrap(),
            manifest["default_permissions"],
            "the list is the rights of the manifest"
        );
    }
    /// The account, the rights to set and the rights to accept of each
    /// App.
    type Facts = Vec<(
        Option<GitHubOwner>,
        BTreeMap<String, String>,
        BTreeMap<String, String>,
    )>;
    async fn facts(url: &str, owner: &Person, body: Value) -> Facts {
        let (status, text) = call(url, owner, "/v1/github/apps", body).await;
        assert_eq!(status, 200, "{text}");
        let apps: GitHubAppsReply = serde_json::from_str(&text).unwrap();
        apps.apps
            .into_iter()
            .map(|a| (a.owner, a.to_set, a.to_accept))
            .collect()
    }
    let acme = Some(GitHubOwner {
        login: OWNER.into(),
        organization: true,
    });
    let none = BTreeMap::new;
    let account = json!({ "owner": OWNER });
    // Not installed: each App has the list, and no install accepts.
    assert_eq!(
        facts(&url, &owner, account.clone()).await,
        [
            (acme.clone(), none(), none()),
            (acme.clone(), none(), none())
        ]
    );
    {
        let mut seen = fake.seen();
        seen.installed = true;
        seen.app_lacks = vec!["vulnerability_alerts".into()];
        seen.app_more = vec![("administration".into(), "write".into())];
        seen.lacks = vec!["security_events".into()];
    }
    let lead = facts(&url, &owner, account.clone()).await.remove(1);
    assert_eq!(
        lead,
        (
            acme.clone(),
            rights(&[("administration", "none"), ("vulnerability_alerts", "read")]),
            rights(&[
                ("security_events", "write"),
                ("vulnerability_alerts", "read")
            ]),
        )
    );
    fake.seen().save_rights();
    let lead = facts(&url, &owner, account.clone()).await.remove(1);
    assert_eq!(
        lead,
        (
            acme.clone(),
            none(),
            rights(&[
                ("security_events", "write"),
                ("vulnerability_alerts", "read")
            ]),
        )
    );
    fake.seen().accept_rights();
    assert_eq!(
        facts(&url, &owner, account).await,
        [(acme.clone(), none(), none()), (acme, none(), none())]
    );
    fake.seen().app_lacks = vec!["contents".into()];
    assert_eq!(
        facts(&url, &owner, json!({})).await,
        [(None, none(), none()), (None, none(), none())],
        "with no account, riff-server reads no right"
    );
}

/// 01M6WG52WSWM8A9CN6X448AGAJ: a riff-server with a bucket and no secret
/// has no place for the keys: it refuses `riff github create`.
#[tokio::test]
async fn a_server_with_no_place_for_the_keys_refuses_create() {
    let fake = FakeGitHub::start().await;
    let (service, url) = serve(&fake, At::Nowhere, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let create = json!({ "owner": OWNER, "organization": true });
    let (status, text) = call(&url, &owner, "/v1/github/create", create).await;
    assert_eq!(status, 409, "{text}");
    assert!(
        text.contains("riff cloud create NAME makes the secret"),
        "{text}"
    );
}

/// The versions of a fake secret of Secret Manager, and the tokens of
/// each call. With a `refuse` status, each read gets that status and
/// the error text of Google.
#[derive(Clone, Default)]
struct Secret {
    versions: Arc<Mutex<Vec<String>>>,
    bearers: Arc<Mutex<Vec<String>>>,
    refuse: Arc<Mutex<Option<u16>>>,
}

/// A fake metadata server and Secret Manager with the one secret
/// `projects/p/secrets/s`. Gives its URL.
async fn secret_manager(secret: Secret) -> String {
    async fn access(
        State(s): State<Secret>,
        headers: axum::http::HeaderMap,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        let bearer = headers["authorization"].to_str().unwrap().to_owned();
        s.bearers.lock().unwrap().push(bearer);
        if let Some(status) = *s.refuse.lock().unwrap() {
            let status = axum::http::StatusCode::from_u16(status).unwrap();
            let error = json!({ "error": {
                "code": status.as_u16(),
                "message": "Permission 'secretmanager.versions.access' denied on resource \
                            (or it may not exist).",
                "status": "PERMISSION_DENIED",
            }});
            return (status, Json(error)).into_response();
        }
        match s.versions.lock().unwrap().last() {
            Some(data) => Json(json!({ "payload": { "data": data } })).into_response(),
            None => axum::http::StatusCode::NOT_FOUND.into_response(),
        }
    }
    async fn add(
        State(s): State<Secret>,
        headers: axum::http::HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        let bearer = headers["authorization"].to_str().unwrap().to_owned();
        s.bearers.lock().unwrap().push(bearer);
        let data = body["payload"]["data"].as_str().unwrap().to_owned();
        s.versions.lock().unwrap().push(data);
        Json(json!({ "name": "projects/p/secrets/s/versions/1" }))
    }
    let router = axum::Router::new()
        .route(
            "/computeMetadata/v1/instance/service-accounts/default/token",
            get(|| async { Json(json!({ "access_token": "google-token", "expires_in": 3600 })) }),
        )
        .route(
            "/v1/projects/p/secrets/s/versions/latest:access",
            get(access),
        )
        .route("/v1/projects/p/secrets/s:addVersion", post(add))
        .with_state(secret);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    url
}

/// 01M6WG52WSWM8A9CN6X448AGAJ: on Cloud Run, the keys go to a new
/// version of the secret, with the token of the metadata server, and a
/// new server reads them back.
#[tokio::test]
async fn the_keys_go_to_a_version_of_the_secret() {
    use base64::Engine;
    let fake = FakeGitHub::start().await;
    let secret = Secret::default();
    let google = secret_manager(secret.clone()).await;
    let at = At::Secret {
        name: "projects/p/secrets/s".into(),
        api: google.clone(),
        metadata: google,
    };
    let (service, url) = serve(&fake, at.clone(), STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    make_apps(&url, &owner).await;
    let versions = secret.versions.lock().unwrap().clone();
    assert_eq!(versions.len(), 2, "one version for each App");
    let data = base64::engine::general_purpose::STANDARD
        .decode(versions.last().unwrap())
        .unwrap();
    let kept: Apps = serde_json::from_slice(&data).unwrap();
    assert_eq!(kept, both_apps());
    assert!(
        secret
            .bearers
            .lock()
            .unwrap()
            .iter()
            .all(|b| b == "Bearer google-token"),
        "each call has the token of the metadata server"
    );

    let (service, url) = serve(&fake, at, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let (_, text) = call(&url, &owner, "/v1/github/apps", json!({})).await;
    let apps: GitHubAppsReply = serde_json::from_str(&text).unwrap();
    assert_eq!(apps.apps.len(), 2, "{text}");
}

/// A second `riff github create` after a stop between the two Apps
/// starts with the App that the riff lacks.
#[tokio::test]
async fn create_again_starts_with_the_app_that_the_riff_lacks() {
    let fake = FakeGitHub::start().await;
    let (service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let create = json!({ "owner": OWNER, "organization": true });
    let (_, text) = call(&url, &owner, "/v1/github/create", create.clone()).await;
    let started: GitHubStarted = serde_json::from_str(&text).unwrap();
    assert_eq!(started.makes, [GitHubApp::Session, GitHubApp::Lead]);
    let state = state_of(&started.url);
    browse(&format!(
        "{url}/github/created?code={SESSION_CODE}&state={state}"
    ))
    .await;
    // The person closed the browser: no lead App.
    let (_, text) = call(&url, &owner, "/v1/github/create", create).await;
    let started: GitHubStarted = serde_json::from_str(&text).unwrap();
    assert_eq!(started.makes, [GitHubApp::Lead]);
    let (_, page) = browse(&started.url).await;
    let (_, manifest) = form(&page);
    assert_eq!(manifest["name"], "riff-acme-lead");
}

/// The code of GitHub goes into the path of a call: riff-server takes
/// only letters, digits, `-` and `_`, and swaps no other code.
#[tokio::test]
async fn a_code_that_is_no_code_is_refused() {
    let fake = FakeGitHub::start().await;
    let (service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let owner = person(&service, "ada@example.com").await;
    let create = json!({ "owner": OWNER, "organization": true });
    let (_, text) = call(&url, &owner, "/v1/github/create", create).await;
    let started: GitHubStarted = serde_json::from_str(&text).unwrap();
    let state = state_of(&started.url);
    let back = format!("{url}/github/created?code=..%2F..%2Fapp&state={state}");
    let (status, text) = browse(&back).await;
    assert_eq!(status, 400, "{text}");
    assert!(
        text.contains("GitHub gave no code that riff can use"),
        "{text}"
    );
    assert!(fake.seen().swapped.is_empty());
}

/// The URI of the session `id` of ada in the repository `repo`.
fn ada_in(repo: &str, id: &str) -> String {
    format!("riff://ada@heron/{repo}?session={id}")
}

/// The session token of the session `session` of `person`, from the
/// swap at the token endpoint.
async fn session_of(url: &str, person: &Person, session: &str) -> Person {
    let form = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
         &subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token\
         &subject_token={}&session={session}",
        person.token
    );
    let reply = common::refresh(url, &person.key, &form).await;
    assert_eq!(reply.status(), 200);
    let reply: riff_core::wire::TokenReply = reply.json().await.unwrap();
    Person {
        key: person.key.clone(),
        token: reply.access_token,
    }
}

/// A riff of ada with both Apps installed on acme/app, and the sessions
/// of ada: the worker `w1` with the claim `issue-12`, the verifier `v1`
/// with the claim `verify-issue-12`, the lead `l1`, and `o1` in
/// acme/other. Bob, a member, has the worker `b1`.
struct Sessions {
    fake: FakeGitHub,
    url: String,
    ada: Person,
    bob: Person,
    _dir: tempfile::TempDir,
}

async fn sessions() -> Sessions {
    let fake = FakeGitHub::start().await;
    fake.seen().installed = true;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(FILE),
        serde_json::to_vec(&both_apps()).unwrap(),
    )
    .unwrap();
    let (service, url) = serve(&fake, At::Dir(dir.path().to_owned()), STATE_LIFE).await;
    let ada = person(&service, "ada@example.com").await;
    let bob = person(&service, "bob@example.com").await;
    let (status, text) = call(
        &url,
        &ada,
        "/v1/resume",
        json!({ "me": "riff://ada@heron/acme/app", "riff": true }),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    for (who, repo, id, claim) in [
        (&ada, "acme/app", "w1", Some("issue-12")),
        (&ada, "acme/app", "v1", Some("verify-issue-12")),
        (&ada, "acme/app", "l1", None),
        (&ada, "acme/other", "o1", None),
        (&bob, "acme/app", "b1", Some("issue-9")),
    ] {
        let session = session_of(&url, who, id).await;
        let me = if std::ptr::eq(who, &bob) {
            format!("riff://bob@heron/{repo}?session={id}")
        } else {
            ada_in(repo, id)
        };
        let (status, text) = call(&url, &session, "/v1/register", json!({ "me": me })).await;
        assert_eq!(status, 200, "{text}");
        if let Some(item) = claim {
            let body = json!({ "me": me, "thread": repo, "item": item });
            let (status, text) = call(&url, &session, "/v1/claim", body).await;
            assert_eq!(status, 200, "{text}");
        }
    }
    let lead = json!({ "me": "riff://ada@heron/acme/app", "session": "l1" });
    let (status, text) = call(&url, &ada, "/v1/lead", lead).await;
    assert_eq!(status, 200, "{text}");
    Sessions {
        fake,
        url,
        ada,
        bob,
        _dir: dir,
    }
}

/// `POST /v1/github/token` of `person` for the session `id`.
async fn token_of(s: &Sessions, person: &Person, body: Value) -> (u16, String) {
    call(&s.url, person, "/v1/github/token", body).await
}

/// 01M6WPMEEWX09DHJZF18BKBPYW: riff-server takes the role from its own
/// facts. Each role gets a token of its App, for the one repository of
/// the session, with the rights of section 2 of #1543 and no more: a
/// session with a claim `verify-ITEM` gets the rights of a verifier, and
/// a worker that names the lead and another repository gets the rights
/// of a worker for its own repository.
#[tokio::test]
async fn each_role_gets_a_token_of_its_own_repository_with_its_rights_and_no_more() {
    let s = sessions().await;
    let mut got = Vec::new();
    for (id, body) in [
        ("w1", json!({ "session": "w1" })),
        ("v1", json!({ "session": "v1" })),
        ("l1", json!({ "session": "l1" })),
        // A worker cannot get more: the server reads no role and no
        // repository from the request.
        (
            "w1",
            json!({ "session": "w1", "role": "lead", "repo": "acme/web", "permissions": {"administration": "write"} }),
        ),
    ] {
        let (status, text) = token_of(&s, &s.ada, body).await;
        assert_eq!(status, 200, "{id}: {text}");
        let reply: SessionToken = serde_json::from_str(&text).unwrap();
        assert_eq!(reply.repo, REPO, "{id}");
        assert_eq!(reply.ends.as_deref(), Some("2026-11-08T13:00:00Z"));
        assert!(reply.problem.is_none(), "{id}: {text}");
        assert!(reply.token.as_deref().unwrap().starts_with("ghs_"));
        assert_eq!(reply.rights, table(reply.role), "{id}");
        // 01M6ZEJ4NEZGQFV3Q4BER5AW3S: only the token of the lead App has
        // the rights on the alerts.
        let alerts = ALERTS
            .iter()
            .filter(|name| reply.rights.contains_key(**name));
        let want = if reply.role == GitHubRole::Lead { 3 } else { 0 };
        assert_eq!(alerts.count(), want, "{id}: {text}");
        got.push((reply.role, reply.bot.unwrap(), reply.bot_id));
    }
    let worker = (GitHubRole::Worker, "riff-acme[bot]".to_owned(), Some(1101));
    assert_eq!(
        got,
        [
            worker.clone(),
            (
                GitHubRole::Verifier,
                "riff-acme[bot]".to_owned(),
                Some(1101)
            ),
            (
                GitHubRole::Lead,
                "riff-acme-lead[bot]".to_owned(),
                Some(1202)
            ),
            worker,
        ]
    );
    let body = |role| json!({ "repositories": ["app"], "permissions": table(role) });
    let asked: Vec<_> = s.fake.seen().asked.clone();
    assert_eq!(
        asked,
        [
            (101, body(GitHubRole::Worker)),
            (101, body(GitHubRole::Verifier)),
            (202, body(GitHubRole::Lead)),
            (101, body(GitHubRole::Worker)),
        ]
    );
    // The verifier reads only, and only the lead sets a status.
    assert!(table(GitHubRole::Verifier).values().all(|l| l == "read"));
    assert_eq!(table(GitHubRole::Worker)["statuses"], "read");
    // No token goes back to GitHub: the session keeps each one.
    assert!(s.fake.seen().revoked.is_empty());
}

/// 01M6WPMEEWX09DHJZF18BKBPYW: only the person of a session gets its
/// token, with a token of the person: not a session token, not another
/// person, and not a session that riff-server does not know.
#[tokio::test]
async fn a_token_is_only_for_a_session_of_the_person_who_asks() {
    let s = sessions().await;
    let w1 = session_of(&s.url, &s.ada, "w1").await;
    for (who, session, why) in [
        (&w1, "w1", "from the daemon of its person"),
        (&s.ada, "b1", "knows no session b1 of ada"),
        (&s.bob, "w1", "knows no session w1 of bob"),
        (&s.ada, "nobody", "knows no session nobody of ada"),
    ] {
        let (status, text) = token_of(&s, who, json!({ "session": session })).await;
        assert_eq!(status, 403, "{session}: {text}");
        assert!(text.contains(why), "{text}");
    }
    assert!(s.fake.seen().asked.is_empty());
    // With no token, no call.
    let reply = common::client()
        .post(format!("{}/v1/github/token", s.url))
        .json(&json!({ "session": "w1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 401);
}

/// 01M6Z87A04EXTT1FFJSJ1EM6TB: a repository with no installation of the
/// App gives no token, and the reply names the fix: the session gets no
/// token, never the token of the person. So does a riff with no App.
#[tokio::test]
async fn a_repository_with_no_installation_gives_the_fix_and_no_token() {
    let s = sessions().await;
    let (status, text) = token_of(&s, &s.ada, json!({ "session": "o1" })).await;
    assert_eq!(status, 200, "{text}");
    let reply: SessionToken = serde_json::from_str(&text).unwrap();
    assert_eq!(
        (reply.role, reply.repo.as_str()),
        (GitHubRole::Worker, "acme/other")
    );
    assert!(reply.token.is_none());
    assert_eq!(
        reply.problem.as_deref(),
        Some("riff-acme is not installed on acme/other: riff github install acme")
    );
    s.fake.seen().installed = false;
    let (_, text) = token_of(&s, &s.ada, json!({ "session": "l1" })).await;
    let reply: SessionToken = serde_json::from_str(&text).unwrap();
    assert_eq!(
        reply.problem.as_deref(),
        Some("riff-acme-lead is not installed on acme/app: riff github install acme")
    );
    assert!(s.fake.seen().asked.is_empty());

    // A riff with no App.
    let fake = FakeGitHub::start().await;
    let (service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let ada = person(&service, "ada@example.com").await;
    let w1 = session_of(&url, &ada, "w1").await;
    let me = ada_in(REPO, "w1");
    let (status, text) = call(&url, &w1, "/v1/register", json!({ "me": me })).await;
    assert_eq!(status, 200, "{text}");
    let (_, text) = call(&url, &ada, "/v1/github/token", json!({ "session": "w1" })).await;
    let reply: SessionToken = serde_json::from_str(&text).unwrap();
    assert_eq!(
        reply.problem.as_deref(),
        Some("the riff has no session App: riff github create --owner acme")
    );
}

/// 01M6WPMEEWX09DHJZF18BKBPYW: a token that GitHub gave with other
/// rights than the role has goes back to GitHub at once, and the reply
/// is an error with no token.
#[tokio::test]
async fn a_token_with_other_rights_is_revoked_and_refused() {
    let s = sessions().await;
    let mut more = table(GitHubRole::Worker);
    more.insert("secrets".into(), "read".into());
    s.fake.seen().give = Some(more);
    let (status, text) = token_of(&s, &s.ada, json!({ "session": "w1" })).await;
    assert_eq!(status, 502, "{text}");
    assert!(text.contains("more: secrets: read"), "{text}");
    let seen = s.fake.seen();
    assert_eq!(seen.revoked, seen.tokens);
    assert!(!text.contains(seen.tokens[0].as_str()), "{text}");
}

/// 01M6WPMNV3YX526YD54S5R2HR5: the log line of each token of a session
/// names the session, the person, the repository, the role and the end,
/// and never holds the token.
#[tokio::test]
async fn the_log_line_of_a_token_names_the_session_and_holds_no_token() {
    // The log of this thread misses lines while other tests run.
    if !isolated::rerun_alone(
        module_path!(),
        "the_log_line_of_a_token_names_the_session_and_holds_no_token",
    ) {
        return;
    }
    let lines = Lines::default();
    let writer = lines.clone();
    let subscriber = tracing_subscriber::fmt()
        .event_format(riff_server::logline::JsonLines)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _log = tracing::subscriber::set_default(subscriber);

    let s = sessions().await;
    let (status, text) = token_of(&s, &s.ada, json!({ "session": "v1" })).await;
    assert_eq!(status, 200, "{text}");
    let log = String::from_utf8(lines.0.lock().unwrap().clone()).unwrap();
    let line: Value = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| line["message"] == "a token of a session on GitHub")
        .unwrap_or_else(|| panic!("no line of the token in {log}"));
    assert_eq!(line["session"], "v1", "{line}");
    assert_eq!(line["person"], "ada", "{line}");
    assert_eq!(line["repo"], REPO, "{line}");
    assert_eq!(line["role"], "verifier", "{line}");
    assert_eq!(line["ends"], "2026-11-08T13:00:00Z", "{line}");
    for token in s.fake.seen().tokens.iter() {
        assert!(!log.contains(token.as_str()), "a token in: {log}");
    }
}

/// `POST /v1/github/daemon` of `person` for the repository `repo`.
async fn daemon_token_of(s: &Sessions, person: &Person, repo: &str) -> (u16, String) {
    call(&s.url, person, "/v1/github/daemon", json!({ "repo": repo })).await
}

/// 01M6XZJFGNVG6B7DF5VQ3H1DNX: each person gets a token of the lead App
/// for the writes of its daemon on one repository, with the rights of the
/// lead and no more. A repository with no installation gives the fix and
/// no token. A session token and a text that is no repository are refused.
#[tokio::test]
async fn the_daemon_of_each_person_gets_a_token_of_the_lead_app_for_one_repository() {
    let s = sessions().await;
    for person in [&s.ada, &s.bob] {
        let (status, text) = daemon_token_of(&s, person, REPO).await;
        assert_eq!(status, 200, "{text}");
        let reply: SessionToken = serde_json::from_str(&text).unwrap();
        assert_eq!((reply.role, reply.repo.as_str()), (GitHubRole::Lead, REPO));
        assert_eq!(reply.bot.as_deref(), Some("riff-acme-lead[bot]"));
        assert_eq!(reply.bot_id, Some(1202));
        assert_eq!(reply.rights, table(GitHubRole::Lead));
        assert_eq!(reply.ends.as_deref(), Some("2026-11-08T13:00:00Z"));
        assert!(reply.token.as_deref().unwrap().starts_with("ghs_"));
        assert!(reply.problem.is_none(), "{text}");
    }
    let body = json!({ "repositories": ["app"], "permissions": table(GitHubRole::Lead) });
    assert_eq!(s.fake.seen().asked, [(202, body.clone()), (202, body)]);
    assert!(s.fake.seen().revoked.is_empty());

    let (status, text) = daemon_token_of(&s, &s.ada, "acme/other").await;
    assert_eq!(status, 200, "{text}");
    let reply: SessionToken = serde_json::from_str(&text).unwrap();
    assert!(reply.token.is_none());
    assert_eq!(
        reply.problem.as_deref(),
        Some("riff-acme-lead is not installed on acme/other: riff github install acme")
    );

    let w1 = session_of(&s.url, &s.ada, "w1").await;
    let (status, text) = daemon_token_of(&s, &w1, REPO).await;
    assert_eq!(status, 403, "{text}");
    assert!(text.contains("the daemon of its person asks"), "{text}");
    let (status, text) = daemon_token_of(&s, &s.ada, "acme").await;
    assert_eq!(status, 400, "{text}");
    assert_eq!(s.fake.seen().asked.len(), 2);
}

/// 01M74H872NW356DN924N6V81XE: a repository `acme/.` or `acme/..` gets
/// 400 from each call of GitHub, with no request to GitHub, and
/// `acme/.github` is a repository.
#[tokio::test]
async fn a_repository_of_dots_only_is_refused() {
    let s = sessions().await;
    for id in [101, 202] {
        s.fake.seen().added.insert(id, vec!["acme/.github".into()]);
    }
    for repo in ["acme/.", "acme/.."] {
        for (path, body) in [
            ("/v1/github/apps", json!({ "owner": OWNER, "repo": repo })),
            ("/v1/github/check", json!({ "repo": repo })),
            ("/v1/github/daemon", json!({ "repo": repo })),
        ] {
            let (status, text) = call(&s.url, &s.ada, path, body).await;
            assert_eq!(status, 400, "{path} {repo}: {text}");
            assert!(text.contains("is no repository OWNER/NAME"), "{text}");
        }
    }
    assert!(s.fake.seen().asked.is_empty());

    let body = json!({ "owner": OWNER, "repo": "acme/.github" });
    let (status, text) = call(&s.url, &s.ada, "/v1/github/apps", body).await;
    assert_eq!(status, 200, "{text}");
    let apps: GitHubAppsReply = serde_json::from_str(&text).unwrap();
    assert!(apps.apps.iter().all(|a| a.has_repo == Some(true)), "{text}");
    let (status, text) = daemon_token_of(&s, &s.ada, "acme/.github").await;
    assert_eq!(status, 200, "{text}");
    let reply: SessionToken = serde_json::from_str(&text).unwrap();
    assert_eq!(reply.repo, "acme/.github");
    assert!(reply.token.is_some(), "{text}");
}

/// 01M6XZJFGNVG6B7DF5VQ3H1DNX: a token of the daemon with other rights
/// than the lead has goes back to GitHub at once, and the reply is an
/// error with no token. Its log line names the person, the repository
/// and the end, and never holds the token.
#[tokio::test]
async fn a_token_of_the_daemon_has_a_log_line_with_no_token_and_the_rights_of_the_lead() {
    // The log of this thread misses lines while other tests run.
    if !isolated::rerun_alone(
        module_path!(),
        "a_token_of_the_daemon_has_a_log_line_with_no_token_and_the_rights_of_the_lead",
    ) {
        return;
    }
    let lines = Lines::default();
    let writer = lines.clone();
    let subscriber = tracing_subscriber::fmt()
        .event_format(riff_server::logline::JsonLines)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _log = tracing::subscriber::set_default(subscriber);

    let s = sessions().await;
    let (status, text) = daemon_token_of(&s, &s.ada, REPO).await;
    assert_eq!(status, 200, "{text}");
    let log = String::from_utf8(lines.0.lock().unwrap().clone()).unwrap();
    let line: Value = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| line["message"] == "a token of the daemon on GitHub")
        .unwrap_or_else(|| panic!("no line of the token in {log}"));
    assert_eq!(line["person"], "ada", "{line}");
    assert_eq!(line["repo"], REPO, "{line}");
    assert_eq!(line["ends"], "2026-11-08T13:00:00Z", "{line}");

    let mut more = table(GitHubRole::Lead);
    more.insert("workflows".into(), "write".into());
    s.fake.seen().give = Some(more);
    let (status, text) = daemon_token_of(&s, &s.ada, REPO).await;
    assert_eq!(status, 502, "{text}");
    assert!(text.contains("more: workflows: write"), "{text}");
    let seen = s.fake.seen();
    assert_eq!(seen.revoked, seen.tokens[1..]);
    let log = String::from_utf8(lines.0.lock().unwrap().clone()).unwrap();
    for token in seen.tokens.iter() {
        assert!(!log.contains(token.as_str()), "a token in: {log}");
        assert!(!text.contains(token.as_str()), "a token in: {text}");
    }
}

/// The lines of `log` with the severity `WARNING`.
fn warnings(log: &str) -> Vec<Value> {
    log.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line["severity"] == "WARNING")
        .collect()
}

/// 01M6ZHWVPD7MN6GH6GS0GVETHA: the install of the lead App lacks the
/// rights on the alerts, as an install of a lead App that riff made
/// before them. GitHub refuses each token that asks for them. riff-server
/// asks again with the rights of the lead with no right on the alerts:
/// the lead and the daemon still get a token, and each token logs one
/// warning with the fix. A refusal of another right gives no token, as
/// before.
#[tokio::test]
async fn a_lead_app_with_no_rights_on_the_alerts_still_gives_the_lead_a_token() {
    // The log of this thread misses lines while other tests run.
    if !isolated::rerun_alone(
        module_path!(),
        "a_lead_app_with_no_rights_on_the_alerts_still_gives_the_lead_a_token",
    ) {
        return;
    }
    let lines = Lines::default();
    let writer = lines.clone();
    let subscriber = tracing_subscriber::fmt()
        .event_format(riff_server::logline::JsonLines)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _log = tracing::subscriber::set_default(subscriber);
    let log = || String::from_utf8(lines.0.lock().unwrap().clone()).unwrap();

    let s = sessions().await;
    s.fake.seen().lacks = ALERTS.map(str::to_owned).to_vec();
    let (status, text) = token_of(&s, &s.ada, json!({ "session": "l1" })).await;
    assert_eq!(status, 200, "{text}");
    let reply: SessionToken = serde_json::from_str(&text).unwrap();
    assert_eq!(reply.role, GitHubRole::Lead);
    assert!(reply.token.as_deref().unwrap().starts_with("ghs_"));
    assert!(reply.problem.is_none(), "{text}");
    assert_eq!(reply.rights, no_alerts());
    let body = |rights| json!({ "repositories": ["app"], "permissions": rights });
    let first = (202, body(table(GitHubRole::Lead)));
    let again = (202, body(no_alerts()));
    assert_eq!(s.fake.seen().asked, [first.clone(), again.clone()]);
    let warned = warnings(&log());
    assert_eq!(warned.len(), 1, "{}", log());
    assert_eq!(warned[0]["repo"], REPO, "{}", warned[0]);
    assert_eq!(warned[0]["app"], "riff-acme-lead", "{}", warned[0]);
    let message = warned[0]["message"].as_str().unwrap();
    for words in [
        "GitHub refused a token of riff-acme-lead with the rights on the alerts \
         (security_events, secret_scanning_alerts, vulnerability_alerts): ",
        "The permissions requested are not granted to this installation. riff-server",
        "riff-server asked again with no right on the alerts.",
        // 01M74FVDGM0342M8TZQCCK5H4H: the fix is one command.
        "Fix: riff github install acme, in a plain terminal",
    ] {
        assert!(message.contains(words), "{words}: {message}");
    }

    // The daemon writes as the lead App too.
    let (status, text) = daemon_token_of(&s, &s.ada, REPO).await;
    assert_eq!(status, 200, "{text}");
    let reply: SessionToken = serde_json::from_str(&text).unwrap();
    assert!(reply.token.is_some() && reply.problem.is_none(), "{text}");
    assert_eq!(reply.rights, no_alerts());
    assert_eq!(s.fake.seen().asked[2..], [first, again]);
    assert_eq!(warnings(&log()).len(), 2, "{}", log());
    assert!(s.fake.seen().revoked.is_empty());

    // A worker asks for no right on the alerts, so GitHub gives its token.
    let (status, text) = token_of(&s, &s.ada, json!({ "session": "w1" })).await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(warnings(&log()).len(), 2, "{}", log());

    // An install that lacks another right gives no token: riff-server
    // asks again only for the lead, and the second ask fails too.
    s.fake.seen().lacks = vec!["contents".to_owned()];
    let asked = s.fake.seen().asked.len();
    let (status, text) = token_of(&s, &s.ada, json!({ "session": "w1" })).await;
    assert_eq!(status, 502, "{text}");
    assert!(text.contains("GitHub replied 422"), "{text}");
    let (status, text) = token_of(&s, &s.ada, json!({ "session": "l1" })).await;
    assert_eq!(status, 502, "{text}");
    assert_eq!(s.fake.seen().asked.len(), asked + 3);
    let log = log();
    let fallbacks = warnings(&log)
        .into_iter()
        .filter(|line| line["message"].as_str().unwrap().contains("asked again"))
        .count();
    assert_eq!(fallbacks, 2, "{log}");
    for token in s.fake.seen().tokens.iter() {
        assert!(!log.contains(token.as_str()), "a token in: {log}");
    }
}

/// 01M6ZHWVPD7MN6GH6GS0GVETHA: when the install of the lead App lacks
/// the rights on the alerts, the check of the lead gives the rights of
/// the lead with no right on the alerts, and names each missing right.
/// riff-server revokes each token.
#[tokio::test]
async fn the_check_names_each_right_on_the_alerts_that_the_lead_app_lacks() {
    let s = sessions().await;
    s.fake.seen().lacks = ALERTS.map(str::to_owned).to_vec();
    let (status, text) = call(&s.url, &s.ada, "/v1/github/check", json!({ "repo": REPO })).await;
    assert_eq!(status, 200, "{text}");
    let checked: GitHubChecked = serde_json::from_str(&text).unwrap();
    let lead = &checked.roles[2];
    assert_eq!(lead.role, GitHubRole::Lead);
    assert!(!lead.passed());
    assert!(lead.problem.is_none(), "{lead:?}");
    assert_eq!(lead.rights, no_alerts());
    assert_eq!(
        lead.missing,
        [
            "secret_scanning_alerts: write",
            "security_events: write",
            "vulnerability_alerts: read"
        ]
    );
    assert!(lead.extra.is_empty(), "{lead:?}");
    assert!(
        checked.roles[..2].iter().all(|role| role.passed()),
        "{text}"
    );
    let seen = s.fake.seen();
    assert_eq!(seen.asked.len(), 4);
    assert_eq!(seen.revoked, seen.tokens);
}

/// An open `GET /v1/github/renew` stream of a person, and the text that it
/// gave and that no event took yet.
struct RenewStream {
    reply: reqwest::Response,
    buffer: String,
}

impl RenewStream {
    /// The stream of `person` as `uri`.
    async fn open(url: &str, person: &Person, uri: &str) -> RenewStream {
        let path = format!("{url}/v1/github/renew");
        let proof = person
            .key
            .proof("GET", &path, Some(&person.token), common::now());
        let reply = common::client()
            .get(&path)
            .query(&[("uri", uri)])
            .header("dpop", proof)
            .header("authorization", format!("DPoP {}", person.token))
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), 200);
        RenewStream {
            reply,
            buffer: String::new(),
        }
    }

    /// The next renew, or `None` when none comes in `wait`.
    async fn next(&mut self, wait: Duration) -> Option<riff_core::wire::GitHubRenew> {
        loop {
            if let Some(end) = self.buffer.find('\n') {
                let line: String = self.buffer.drain(..=end).collect();
                match line.trim().strip_prefix("data:") {
                    Some(data) => return Some(serde_json::from_str(data.trim()).unwrap()),
                    None => continue,
                }
            }
            let chunk = tokio::time::timeout(wait, self.reply.chunk()).await.ok()?;
            let chunk = chunk.unwrap().expect("the stream stays open");
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
    }
}

/// A delivery of the kind `kind` as GitHub sends it, signed with
/// `secret`: the status and the text.
async fn deliver(url: &str, kind: &str, id: &str, body: &Value, secret: &str) -> (u16, String) {
    let body = serde_json::to_vec(body).unwrap();
    let reply = reqwest::Client::new()
        .post(format!("{url}/hooks/github"))
        .header("x-github-event", kind)
        .header("x-github-delivery", id)
        .header(
            "x-hub-signature-256",
            riff_server::hooks::signature(secret, &body),
        )
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

/// The event `installation` of the install `id` with `action`.
fn installation_event(id: u64, action: &str) -> Value {
    json!({
        "action": action,
        "installation": { "id": id, "account": { "login": OWNER } },
    })
}

/// The token of the reply `text` of a token call, and its install.
fn token_and_install(text: &str) -> (String, Option<u64>, BTreeMap<String, String>) {
    let reply: SessionToken = serde_json::from_str(text).unwrap();
    (reply.token.unwrap(), reply.installation, reply.rights)
}

/// Waits until the fake saw each token of `tokens` revoked.
async fn until_revoked(s: &Sessions, tokens: &[&str]) {
    for _ in 0..500 {
        let revoked = s.fake.seen().revoked.clone();
        if tokens
            .iter()
            .all(|token| revoked.iter().any(|r| r == token))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{tokens:?} not revoked: {:?}", s.fake.seen().revoked);
}

/// 01M74MB94Q4VPK724W7H0PZH9P, 01M74MB92717A21MG97BF7SDP3, 01M74MB1A1TV4WBZ07AFCEP5WG: the install of the lead App lacks
/// the rights on the alerts, and the lead and the daemon got tokens with
/// no right on the alerts. The owner accepts the new rights, and GitHub
/// sends the event `installation` to the webhook of the lead App. The
/// stream of the person gets the renew of the install, and the next token
/// of the lead has the new rights. After the wait, riff-server revokes
/// each older token of the install: the lead's and the daemon's, not the
/// new one, and not the token of a worker of the session App.
#[tokio::test]
async fn an_accept_of_new_rights_revokes_the_tokens_of_the_install_and_the_next_token_has_them() {
    let s = sessions().await;
    let lead_install = installation_of(202);
    let session_install = installation_of(101);
    s.fake.seen().lacks = ALERTS.map(str::to_owned).to_vec();
    let mut stream = RenewStream::open(&s.url, &s.ada, "riff://ada@heron/acme/app").await;
    let (status, text) = token_of(&s, &s.ada, json!({ "session": "l1" })).await;
    assert_eq!(status, 200, "{text}");
    let (old_lead, install, rights) = token_and_install(&text);
    assert_eq!((install, rights), (Some(lead_install), no_alerts()));
    let (status, text) = daemon_token_of(&s, &s.ada, REPO).await;
    assert_eq!(status, 200, "{text}");
    let (old_daemon, install, _) = token_and_install(&text);
    assert_eq!(install, Some(lead_install));
    let (status, text) = token_of(&s, &s.ada, json!({ "session": "w1" })).await;
    assert_eq!(status, 200, "{text}");
    let (worker, install, _) = token_and_install(&text);
    assert_eq!(install, Some(session_install));

    // An event of another action renews nothing.
    let created = installation_event(lead_install, "created");
    let (status, text) = deliver(
        &s.url,
        "installation",
        "d-1",
        &created,
        lead_webhook_secret(),
    )
    .await;
    assert_eq!(status, 202, "{text}");
    assert!(text.contains("does not use"), "{text}");

    // The owner accepts the new rights on GitHub.
    s.fake.seen().lacks.clear();
    let accepted = installation_event(lead_install, "new_permissions_accepted");
    let (status, text) = deliver(
        &s.url,
        "installation",
        "d-2",
        &accepted,
        lead_webhook_secret(),
    )
    .await;
    assert_eq!(status, 202, "{text}");
    assert!(text.contains("renews each token of the install"), "{text}");
    let renew = stream.next(Duration::from_secs(10)).await.expect("a renew");
    assert_eq!(renew.installation, lead_install);
    assert!(
        s.fake.seen().revoked.is_empty(),
        "no revoke before the daemons have the new tokens"
    );

    let (status, text) = token_of(&s, &s.ada, json!({ "session": "l1" })).await;
    assert_eq!(status, 200, "{text}");
    let (new_lead, install, rights) = token_and_install(&text);
    assert_eq!(
        (install, rights),
        (Some(lead_install), table(GitHubRole::Lead))
    );
    until_revoked(&s, &[&old_lead, &old_daemon]).await;
    tokio::time::sleep(REVOKE_AFTER).await;
    let revoked = s.fake.seen().revoked.clone();
    assert_eq!(revoked.len(), 2, "{revoked:?}");
    assert!(
        !revoked.contains(&new_lead) && !revoked.contains(&worker),
        "{revoked:?}"
    );
    assert_eq!(
        stream.next(Duration::from_millis(500)).await,
        None,
        "the new token renews nothing"
    );
}

/// 01M74MB9786AE4YVEG10FNGDT5: a lead App with no webhook sends no event. After the
/// accept, `riff github check` gets a token of the lead with the new
/// rights, and riff-server finds that the install has new rights: the
/// stream gets the renew, and the older token of the lead is revoked.
#[tokio::test]
async fn a_check_that_finds_new_rights_of_an_install_renews_it() {
    let s = sessions().await;
    let lead_install = installation_of(202);
    s.fake.seen().lacks = ALERTS.map(str::to_owned).to_vec();
    let mut stream = RenewStream::open(&s.url, &s.ada, "riff://ada@heron/acme/app").await;
    let (status, text) = token_of(&s, &s.ada, json!({ "session": "l1" })).await;
    assert_eq!(status, 200, "{text}");
    let (old_lead, _, _) = token_and_install(&text);
    // A check before the accept finds the same rights: no renew.
    let (status, _) = call(&s.url, &s.ada, "/v1/github/check", json!({ "repo": REPO })).await;
    assert_eq!(status, 200);
    assert_eq!(stream.next(Duration::from_millis(500)).await, None);

    s.fake.seen().lacks.clear();
    let (status, text) = call(&s.url, &s.ada, "/v1/github/check", json!({ "repo": REPO })).await;
    assert_eq!(status, 200, "{text}");
    let checked: GitHubChecked = serde_json::from_str(&text).unwrap();
    assert!(checked.roles.iter().all(|role| role.passed()), "{text}");
    let renew = stream.next(Duration::from_secs(10)).await.expect("a renew");
    assert_eq!(renew.installation, lead_install);
    until_revoked(&s, &[&old_lead]).await;
}

/// 01M6YC3SYATQW7GZZV3APFG85B: a secret that riff-server cannot read pauses the whole
/// riff. riff-server posts one line as riff-bot in the chat to the owner
/// and each admin, with the fix and the prompt for `claude` on the host,
/// and logs one error. The token call of the lead gets no token, the
/// reason and `no_github`; a worker gets a 503. `riff resume --riff` is
/// refused with the fix while Secret Manager answers 403, and resumes the
/// riff when it answers again. A fault that a new try can end, a 500,
/// pauses nothing.
#[tokio::test]
async fn a_secret_that_answers_403_pauses_the_riff_and_the_lead_gets_no_token() {
    // The log of this thread misses lines while other tests run.
    if !isolated::rerun_alone(
        module_path!(),
        "a_secret_that_answers_403_pauses_the_riff_and_the_lead_gets_no_token",
    ) {
        return;
    }
    use base64::Engine;
    use riff_core::record::By;
    use riff_core::wire::{Kind, ReadReply, RiffReply, RiffState, ServerFacts};
    let lines = Lines::default();
    let writer = lines.clone();
    let subscriber = tracing_subscriber::fmt()
        .event_format(riff_server::logline::JsonLines)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _log = tracing::subscriber::set_default(subscriber);

    let fake = FakeGitHub::start().await;
    fake.seen().installed = true;
    let secret = Secret::default();
    let apps = serde_json::to_vec(&both_apps()).unwrap();
    let data = base64::engine::general_purpose::STANDARD.encode(apps);
    secret.versions.lock().unwrap().push(data);
    // A 500 first: a new try can end it, so it pauses nothing.
    *secret.refuse.lock().unwrap() = Some(500);
    let google = secret_manager(secret.clone()).await;
    let at = At::Secret {
        name: "projects/p/secrets/s".into(),
        api: google.clone(),
        metadata: google,
    };
    let (service, url) = serve(&fake, at, STATE_LIFE).await;
    let ada = person(&service, "ada@example.com").await;
    let bob = person(&service, "bob@example.com").await;
    let place = "riff://ada@heron/acme/app";
    let resume = json!({ "me": place, "riff": true });
    let (status, text) = call(&url, &ada, "/v1/resume", resume.clone()).await;
    assert_eq!(status, 200, "{text}");
    for (id, claim) in [("w1", Some("issue-12")), ("l1", None)] {
        let session = session_of(&url, &ada, id).await;
        let me = ada_in(REPO, id);
        let (status, text) = call(&url, &session, "/v1/register", json!({ "me": me })).await;
        assert_eq!(status, 200, "{text}");
        if let Some(item) = claim {
            let body = json!({ "me": me, "thread": REPO, "item": item });
            let (status, text) = call(&url, &session, "/v1/claim", body).await;
            assert_eq!(status, 200, "{text}");
        }
    }
    let lead = json!({ "me": place, "session": "l1" });
    let (status, text) = call(&url, &ada, "/v1/lead", lead).await;
    assert_eq!(status, 200, "{text}");
    let riff = || async {
        let (_, text) = call(&url, &ada, "/v1/riff", json!({ "me": place })).await;
        serde_json::from_str::<RiffReply>(&text).unwrap()
    };
    assert_eq!(riff().await.state, RiffState::Running);

    // Secret Manager refuses the read.
    *secret.refuse.lock().unwrap() = Some(403);
    let (status, text) = call(&url, &ada, "/v1/github/token", json!({ "session": "w1" })).await;
    assert_eq!(status, 503, "{text}");
    assert!(text.contains("riff cloud create NAME"), "{text}");
    let paused = riff().await;
    assert_eq!(paused.state, RiffState::Paused);
    assert_eq!(paused.riff.unwrap().by, Some(By::Server));

    let (status, text) = call(&url, &ada, "/v1/github/token", json!({ "session": "l1" })).await;
    assert_eq!(status, 200, "{text}");
    let reply: SessionToken = serde_json::from_str(&text).unwrap();
    assert_eq!(reply.role, GitHubRole::Lead);
    assert!(reply.token.is_none() && reply.no_github, "{reply:?}");
    let problem = reply.problem.unwrap();
    assert!(
        problem.starts_with(
            "riff-server cannot read the keys of the GitHub Apps: projects/p/secrets/s: 403 \
             Forbidden: Permission 'secretmanager.versions.access' denied"
        ),
        "{problem}"
    );
    assert!(problem.contains("start claude on the host"), "{problem}");

    let (status, text) = call(&url, &ada, "/v1/resume", resume.clone()).await;
    assert_eq!(status, 409, "{text}");
    assert!(text.contains("the riff stays paused"), "{text}");
    assert!(
        text.contains("riff cloud create NAME, then riff resume --riff"),
        "{text}"
    );
    assert_eq!(riff().await.state, RiffState::Paused);

    let server = format!("{url}/v1/server");
    let proof = ada
        .key
        .proof("GET", &server, Some(&ada.token), common::now());
    let facts: ServerFacts = common::client()
        .get(&server)
        .header("dpop", proof)
        .header("authorization", format!("DPoP {}", ada.token))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let fault = facts.keys_fault.unwrap_or_default();
    assert!(fault.contains("403 Forbidden"), "{fault}");

    // One line in the chat, as riff-bot, to the owner: bob is a member.
    let read = json!({ "me": place, "thread": "chat", "all": true });
    let (_, text) = call(&url, &ada, "/v1/read", read).await;
    let chat: ReadReply = serde_json::from_str(&text).unwrap();
    let [line] = chat.messages.as_slice() else {
        panic!("not one line in the chat: {text}")
    };
    assert_eq!(line.from, riff_core::name::SessionUri::server());
    assert_eq!(line.kind, Kind::Riff);
    assert_eq!(line.body, problem);
    let to: Vec<String> = line.to.iter().map(ToString::to_string).collect();
    assert_eq!(to, ["user=ada,person=true"]);
    let _ = bob;

    // The wake of the repository names the pause and the fix.
    let w1 = session_of(&url, &ada, "w1").await;
    let read = json!({ "me": ada_in(REPO, "w1"), "thread": REPO });
    let (_, text) = call(&url, &w1, "/v1/read", read).await;
    assert!(text.contains("The riff is paused by riff-server"), "{text}");
    assert!(text.contains("riff cloud create NAME"), "{text}");

    let log = String::from_utf8(lines.0.lock().unwrap().clone()).unwrap();
    let errors: Vec<Value> = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line["severity"] == "ERROR")
        .collect();
    let [error] = errors.as_slice() else {
        panic!("not one error line: {log}")
    };
    let message = error["message"].as_str().unwrap();
    assert!(
        message.starts_with("riff-server cannot read the keys of the GitHub Apps"),
        "{message}"
    );

    // Secret Manager answers again: the resume reads the keys, and runs.
    *secret.refuse.lock().unwrap() = None;
    let (status, text) = call(&url, &ada, "/v1/resume", resume).await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(riff().await.state, RiffState::Running);
    let (status, text) = call(&url, &ada, "/v1/github/token", json!({ "session": "l1" })).await;
    assert_eq!(status, 200, "{text}");
    let reply: SessionToken = serde_json::from_str(&text).unwrap();
    assert!(reply.token.is_some() && !reply.no_github, "{reply:?}");
}
