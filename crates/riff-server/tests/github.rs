//! The GitHub Apps of the riff over HTTP, with a fake GitHub
//! (01M6WG52J7WTP079W1AYEQ4W08, 01M6WG52MWZB1SVQMMH6BKJSJ5,
//! 01M6WG52QHC8E24RJ33ZPVHZSJ, 01M6WG52T5PAD9NGQ597CP56K7,
//! 01M6WG52WSWM8A9CN6X448AGAJ, 01M6WG534PPE2JW8JQFRJDEK86): the two
//! manifests, the swap, the store of the keys, the states, the roles of
//! the callers, the check and the webhook.

use crate::common;
use crate::fake_github::{
    FakeGitHub, KEY, LEAD_CODE, OWNER, REPO, SESSION_CODE, WEBHOOK_SECRET, both_apps,
};

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::routing::{get, post};
use riff_core::dpop::Key;
use riff_core::wire::{GitHubApp, GitHubAppsReply, GitHubChecked, GitHubRole, GitHubStarted};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::github::{STATE_LIFE, Settings};
use riff_server::keys::{Apps, At, FILE};
use serde_json::{Value, json};

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
        ]),
    }
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

/// 01M6WG52MWZB1SVQMMH6BKJSJ5, 01M6WG52QHC8E24RJ33ZPVHZSJ: the manifest
/// of each App has only the rights of its roles, and the one of the
/// session App has the webhook and the events of the riff.
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
    assert!(lead.get("hook_attributes").is_none(), "{lead}");
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
        assert!(!reply.contains(WEBHOOK_SECRET), "the secret in: {reply}");
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
/// is not the owner or an admin cannot make the Apps, or ask whether they
/// are installed on an account. Each person can see the Apps with no
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
        text.contains("only the owner or an admin of the riff runs riff github create"),
        "{text}"
    );
    let (status, text) = call(&url, &member, "/v1/github/apps", json!({ "owner": OWNER })).await;
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

/// 01M6WG52QHC8E24RJ33ZPVHZSJ: riff-server takes each delivery of the
/// webhook with status 202, with no token and no build.
#[tokio::test]
async fn the_webhook_takes_each_delivery_with_202() {
    let fake = FakeGitHub::start().await;
    let (_service, url) = serve(&fake, At::Memory, STATE_LIFE).await;
    let reply = reqwest::Client::new()
        .post(format!("{url}/hooks/github"))
        .header("x-github-event", "issues")
        .json(&json!({ "action": "edited" }))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 202);
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
/// each call.
#[derive(Clone, Default)]
struct Secret {
    versions: Arc<Mutex<Vec<String>>>,
    bearers: Arc<Mutex<Vec<String>>>,
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
