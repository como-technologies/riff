//! `riff github create`, `install`, `check` and `events` against a real
//! riff-server with a fake GitHub (01M6WG52J7WTP079W1AYEQ4W08,
//! 01M6WG5322WRWQA79RCSCY4MSS, 01M6WG534PPE2JW8JQFRJDEK86,
//! 01M6WG537B67VWVENY3821RV33).

use crate::common;
#[path = "../../riff-server/tests/fake_github.rs"]
pub(crate) mod fake_github;

use fake_github::{FakeGitHub, LEAD_CODE, OWNER, REPO, SESSION_CODE, both_apps, webhook_secret};

use std::time::Duration;

use isolated::Isolated;
use riff::github::{Account, Wait};
use riff_server::github::Settings;
use riff_server::keys::{At, FILE};

/// A riff with sign-in whose GitHub is `fake`, with the keys at `keys`.
pub(crate) async fn serve(fake: &FakeGitHub, keys: At) -> common::Signed {
    let (listener, url) = common::listen().await;
    let github = Settings {
        api: fake.url.clone(),
        web: fake.url.clone(),
        keys,
        ..Settings::default()
    };
    common::start_signed_config(
        listener,
        &url,
        None,
        |router| router,
        |config| {
            config.github = github;
        },
    )
    .await
}

/// The token of `gh` of `mike` in [`enroll`]: on the fake GitHub, an
/// admin of each repository that it enrolls.
const PROJECT_ADMIN: &str = "gho_admin_of_the_projects";

/// Enrolls `repo` (`OWNER/REPO`) as a project of the riff `server`, whose
/// GitHub is `fake`, with the hosts `hosts`: none allows each host. The
/// person is `mike`, an admin of the riff and of `repo` on the fake
/// (01M76SG8GBXFYXKWMQHPCNEXBN). A wave and a lead run only for a project
/// (#1863).
pub(crate) async fn enroll(server: &common::Signed, fake: &FakeGitHub, repo: &str, hosts: &[&str]) {
    fake.seen().person(PROJECT_ADMIN, repo, true);
    let me = "riff://mike@thelio".parse().unwrap();
    let hosts: Vec<String> = hosts.iter().map(|h| (*h).to_owned()).collect();
    server
        .api("mike", None)
        .project_init(&me, &repo.parse().unwrap(), &hosts, PROJECT_ADMIN)
        .await
        .unwrap();
}

/// A folder with both Apps in the file of the keys.
pub(crate) fn keys_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let apps = serde_json::to_vec(&both_apps()).unwrap();
    std::fs::write(dir.path().join(FILE), apps).unwrap();
    dir
}

/// A clone of [`REPO`] on GitHub, with no commit.
fn clone() -> tempfile::TempDir {
    crate::common::clone_of(REPO)
}

/// The account of the fake.
fn account() -> Account {
    Account {
        login: OWNER.into(),
        id: 7,
        kind: "Organization".into(),
    }
}

/// The site of GitHub in a test.
const WEB: &str = "https://github.example";

/// A short wait, for a fake that answers at once.
const WAIT: Wait = Wait {
    every: Duration::from_millis(20),
    most: Duration::from_secs(20),
};

/// 01M6WG534PPE2JW8JQFRJDEK86: `riff github check` in a clone prints the
/// bot and the rights of each role, and riff-server revokes each token.
#[tokio::test]
async fn riff_github_check_prints_the_bot_and_the_rights_of_each_role() {
    let fake = FakeGitHub::start().await;
    fake.seen().installed = true;
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let env = Isolated::new();
    common::sign_in(&server, &env.riff_home(), "bob");
    let repo = clone();
    let out = env
        .tokio_riff()
        .args(["github", "check"])
        .current_dir(repo.path())
        .env("RIFF_SERVER", &*server)
        // A plain terminal: the test can run under Claude Code.
        .env_remove("CLAUDECODE")
        .output()
        .await
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        stdout,
        "riff github check acme/app:\n  \
         worker    riff-acme[bot]       actions: write, checks: read, contents: write, issues: write, \
         metadata: read, pull_requests: write, statuses: read, workflows: write\n  \
         verifier  riff-acme[bot]       actions: read, checks: read, contents: read, issues: read, \
         metadata: read, pull_requests: read, statuses: read\n  \
         lead      riff-acme-lead[bot]  actions: write, checks: read, contents: write, issues: write, \
         metadata: read, pull_requests: write, secret_scanning_alerts: write, security_events: write, \
         statuses: write, vulnerability_alerts: read\n\
         riff github check: each role has its rights and no more. riff-server revoked each token.\n"
    );
    let seen = fake.seen();
    assert_eq!(seen.tokens.len(), 3);
    assert_eq!(seen.revoked, seen.tokens);
}

/// 01M6ZHWVPD7MN6GH6GS0GVETHA, 01M74FVDGM0342M8TZQCCK5H4H: the install
/// of the lead App lacks the rights on the alerts, as an install of a
/// lead App that riff made before them. `riff github check` prints the
/// rights of the token of the lead, names each missing right with the
/// fix `riff github install OWNER`, and exits with status 1.
#[tokio::test]
async fn riff_github_check_names_each_right_that_the_lead_misses_with_the_fix() {
    let fake = FakeGitHub::start().await;
    {
        let mut seen = fake.seen();
        seen.installed = true;
        seen.lacks = [
            "security_events",
            "secret_scanning_alerts",
            "vulnerability_alerts",
        ]
        .map(str::to_owned)
        .to_vec();
    }
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let env = Isolated::new();
    common::sign_in(&server, &env.riff_home(), "bob");
    let repo = clone();
    let out = env
        .tokio_riff()
        .args(["github", "check"])
        .current_dir(repo.path())
        .env("RIFF_SERVER", &*server)
        // A plain terminal: the test can run under Claude Code.
        .env_remove("CLAUDECODE")
        .output()
        .await
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let lead = stdout
        .lines()
        .find(|line| line.trim_start().starts_with("lead "))
        .unwrap_or_else(|| panic!("no line of the lead in:\n{stdout}"));
    assert_eq!(
        lead,
        "  lead      riff-acme-lead[bot]  actions: write, checks: read, contents: write, \
         issues: write, metadata: read, pull_requests: write, statuses: write; fail: it misses \
         secret_scanning_alerts: write, security_events: write, vulnerability_alerts: read. \
         The fix: riff github install acme"
    );
    assert!(
        stdout.ends_with("riff github check: 1 role failed. riff-server revoked each token.\n"),
        "{stdout}"
    );
    let seen = fake.seen();
    assert_eq!(seen.tokens.len(), 3);
    assert_eq!(seen.revoked, seen.tokens);
}

/// 01M74A37NMGDEHAD01SJ0DQT9B: `riff github events` in a clone prints the
/// last wakes of the GitHub App for the repository, and the counts of the
/// deliveries.
#[tokio::test]
async fn riff_github_events_prints_the_last_wakes_and_the_counts() {
    let fake = FakeGitHub::start().await;
    let keys = keys_dir();
    let (listener, url) = common::listen().await;
    let github = Settings {
        api: fake.url.clone(),
        web: fake.url.clone(),
        keys: At::Dir(keys.path().to_owned()),
        // The real window: both deliveries go into one wake, also on a
        // slow runner.
        debounce: riff_server::hooks::DEBOUNCE,
        ..Settings::default()
    };
    let server = common::start_signed_config(
        listener,
        &url,
        None,
        |router| router,
        |config| {
            config.github = github;
        },
    )
    .await;
    let env = Isolated::new();
    common::sign_in(&server, &env.riff_home(), "bob");
    let repo = clone();
    let check = serde_json::json!({
        "action": "completed",
        "repository": { "full_name": REPO },
        "check_run": { "pull_requests": [{ "number": 12 }] },
    });
    let body = serde_json::to_vec(&check).unwrap();
    for (id, signed) in [("d-1", true), ("d-2", true), ("d-1", true), ("d-3", false)] {
        let mut delivery = reqwest::Client::new()
            .post(format!("{}/hooks/github", &*server))
            .header("x-github-event", "check_run")
            .header("x-github-delivery", id);
        if signed {
            let signature = riff_server::hooks::signature(webhook_secret(), &body);
            delivery = delivery.header("x-hub-signature-256", signature);
        }
        let reply = delivery.body(body.clone()).send().await.unwrap();
        assert_eq!(reply.status().as_u16(), if signed { 202 } else { 401 });
    }
    // The delivery with no signature got 401, as it must.
    assert_eq!(server.take_unsigned(), ["/hooks/github"]);
    let events = || {
        env.tokio_riff()
            .args(["github", "events"])
            .current_dir(repo.path())
            .env("RIFF_SERVER", &*server)
            // A plain terminal: the test can run under Claude Code.
            .env_remove("CLAUDECODE")
            .output()
    };
    let start = std::time::Instant::now();
    let stdout = loop {
        let out = events().await.unwrap();
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert!(
            out.status.success(),
            "{stdout}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        if stdout.contains("#12") || start.elapsed() > Duration::from_secs(20) {
            break stdout;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{stdout}");
    assert_eq!(
        lines[0],
        "riff github events acme/app: the last wake of the GitHub App:"
    );
    assert!(
        lines[1].ends_with(" UTC  #12  check_run  2 events"),
        "{stdout}"
    );
    assert!(
        lines[2].starts_with("riff-server took 2 deliveries of GitHub since ")
            && lines[2]
                .ends_with("dropped 1 repeated delivery, and refused 1 with no good signature."),
        "{stdout}"
    );
}

/// 01M6WG534PPE2JW8JQFRJDEK86: a role that fails its check fails the
/// command with status 1, and names the fix.
#[tokio::test]
async fn riff_github_check_fails_with_no_installation() {
    let fake = FakeGitHub::start().await;
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let env = Isolated::new();
    common::sign_in(&server, &env.riff_home(), "brett");
    let repo = clone();
    let out = env
        .tokio_riff()
        .args(["github", "check"])
        .current_dir(repo.path())
        .env("RIFF_SERVER", &*server)
        // A plain terminal: the test can run under Claude Code.
        .env_remove("CLAUDECODE")
        .output()
        .await
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("fail: riff-acme is not installed on acme/app: riff github install acme"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("riff github check: 3 roles failed. riff-server revoked each token.\n")
    );
}

/// 01M6Z7DMYAW52X62TN1DEX9NCG: with both Apps installed on acme with only
/// acme/app, `riff github check` in a clone of acme/b names each App
/// whose install does not have acme/b, with the fix.
#[tokio::test]
async fn riff_github_check_names_the_fix_for_a_repository_that_no_install_has() {
    let fake = FakeGitHub::start().await;
    fake.seen().installed = true;
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let env = Isolated::new();
    common::sign_in(&server, &env.riff_home(), "bob");
    let repo = crate::common::clone_of("acme/b");
    let out = env
        .tokio_riff()
        .args(["github", "check"])
        .current_dir(repo.path())
        .env("RIFF_SERVER", &*server)
        // A plain terminal: the test can run under Claude Code.
        .env_remove("CLAUDECODE")
        .output()
        .await
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    for line in [
        "  worker    riff-acme[bot]       fail: riff-acme is not installed on acme/b: \
         riff github install acme",
        "  lead      riff-acme-lead[bot]  fail: riff-acme-lead is not installed on acme/b: \
         riff github install acme",
    ] {
        assert!(
            stdout.lines().any(|l| l == line),
            "{line:?} not in:\n{stdout}"
        );
    }
    assert!(fake.seen().tokens.is_empty());
}

/// 01M6WG537B67VWVENY3821RV33, 01M74A37NMGDEHAD01SJ0DQT9B: each command
/// refuses in an agent session and in a container, before each call.
#[tokio::test]
async fn each_command_refuses_in_an_agent_session_and_in_a_container() {
    let fake = FakeGitHub::start().await;
    let server = serve(&fake, At::Memory).await;
    let env = Isolated::new();
    common::sign_in(&server, &env.riff_home(), "brett");
    let repo = clone();
    let socket = env.path().join("riff.sock");
    let socket_var = riff::daemon::SOCKET_VAR;
    for (command, args) in [
        ("create", &["github", "create", "--owner", OWNER][..]),
        ("install", &["github", "install", OWNER]),
        ("check", &["github", "check"]),
        ("events", &["github", "events"]),
    ] {
        for (var, value, who) in [
            ("CLAUDECODE", "1", "a session"),
            ("RIFF_SESSION", "s1", "a session"),
            (socket_var, socket.to_str().unwrap(), "a container"),
        ] {
            let out = env
                .tokio_riff()
                .args(args)
                .current_dir(repo.path())
                .env("RIFF_SERVER", &*server)
                .env_remove("CLAUDECODE")
                .env(var, value)
                .output()
                .await
                .unwrap();
            let stderr = String::from_utf8(out.stderr).unwrap();
            assert_eq!(out.status.code(), Some(1), "{var}: {stderr}");
            assert_eq!(
                stderr,
                format!(
                    "riff: {who} never runs riff github {command}. riff github {command} changed \
                     nothing. next: a person runs riff github {command} in a plain terminal, \
                     outside Claude Code\n"
                ),
                "{var}"
            );
        }
    }
    let calls = server.calls();
    assert!(calls.lock().unwrap().is_empty(), "{calls:?}");
    assert!(fake.seen().asked.is_empty() && fake.seen().swapped.is_empty());
}

/// The browser of the test: it opens the start page, sends each
/// manifest to the fake page of GitHub, and comes back to riff-server
/// with the code of each App, as GitHub does.
fn browser(server: String, delay: Duration) -> impl Fn(&str) {
    move |start: &str| {
        let (start, server) = (start.to_owned(), server.clone());
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let client = reqwest::Client::new();
            let mut page = client
                .get(&start)
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
            for code in [SESSION_CODE, LEAD_CODE] {
                let action = page.split("action=\"").nth(1).unwrap();
                let action = &action[..action.find('"').unwrap()];
                let state = action.split_once("state=").unwrap().1;
                let back = format!("{server}/github/created?code={code}&state={state}");
                page = client.get(back).send().await.unwrap().text().await.unwrap();
            }
        });
    }
}

/// 01M6WG52T5PAD9NGQ597CP56K7, 01M6WG5322WRWQA79RCSCY4MSS: `riff github
/// create` opens the start page, says when each App is made and
/// installed, then checks the repository of the folder.
#[tokio::test]
async fn create_makes_the_apps_waits_for_the_installs_and_checks() {
    let fake = FakeGitHub::start().await;
    fake.seen().install_on_swap = true;
    let server = serve(&fake, At::Memory).await;
    let api = server.api("brett", None);
    let mut out = Vec::new();
    let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = opened.clone();
    let open = browser(server.to_string(), Duration::ZERO);
    let passed = riff::github::create(
        &api,
        &account(),
        WEB,
        move |url| {
            seen.lock().unwrap().push(url.to_owned());
            open(url);
        },
        WAIT,
        Some(REPO),
        &mut out,
    )
    .await
    .unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(passed, "{out}");
    let opened = opened.lock().unwrap().clone();
    // Each install has acme/app: no page Configure.
    assert_eq!(opened.len(), 1);
    assert!(
        opened[0].starts_with(&format!("{server}/github/start?state=")),
        "{opened:?}"
    );
    for line in [
        "riff github create makes the GitHub Apps riff-acme and riff-acme-lead on acme.",
        &format!("  {}", opened[0]),
        "riff-acme: made. Its bot is riff-acme[bot].",
        "riff-acme-lead: made. Its bot is riff-acme-lead[bot].",
        "riff-acme: installed on acme.",
        "riff-acme-lead: installed on acme.",
        "riff-acme: the install has acme/app.",
        "riff-acme-lead: the install has acme/app.",
        "riff github check acme/app:",
        "riff github check: each role has its rights and no more. riff-server revoked each token.",
    ] {
        assert!(out.lines().any(|l| l == line), "{line:?} not in:\n{out}");
    }
    assert_eq!(fake.seen().swapped, [SESSION_CODE, LEAD_CODE]);
}

/// A second `riff github create` on a riff whose old Apps are installed
/// waits for the new Apps, and checks with their keys.
#[tokio::test]
async fn create_again_waits_for_the_new_apps() {
    let fake = FakeGitHub::start().await;
    fake.seen().installed = true;
    let dir = tempfile::tempdir().unwrap();
    let mut old = both_apps();
    for (keys, id) in [(&mut old.session, 1), (&mut old.lead, 2)] {
        keys.as_mut().unwrap().id = id;
    }
    std::fs::write(dir.path().join(FILE), serde_json::to_vec(&old).unwrap()).unwrap();
    let server = serve(&fake, At::Dir(dir.path().to_owned())).await;
    let api = server.api("brett", None);
    let open = browser(server.to_string(), Duration::from_millis(300));
    let mut out = Vec::new();
    let passed = riff::github::create(&api, &account(), WEB, open, WAIT, Some(REPO), &mut out)
        .await
        .unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(passed, "{out}");
    assert!(out.contains("riff-acme-lead: made."), "{out}");
    let seen = fake.seen();
    assert_eq!(seen.swapped, [SESSION_CODE, LEAD_CODE]);
    let ids: Vec<u64> = seen.asked.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, [101, 101, 202], "the check uses the new Apps");
}

/// 01M6WG5322WRWQA79RCSCY4MSS: `riff github install` opens the install
/// page of each App with the account as the target, waits, and names
/// where to run the check when the folder is no repository of the
/// account.
#[tokio::test]
async fn install_opens_the_install_page_of_each_app() {
    let fake = FakeGitHub::start().await;
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let api = server.api("brett", None);
    let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = opened.clone();
    let fake_seen = fake.seen.clone();
    let mut out = Vec::new();
    let passed = riff::github::install(
        &api,
        &account(),
        "https://github.example",
        move |url| {
            seen.lock().unwrap().push(url.to_owned());
            // The owner of the account clicks "Install".
            fake_seen.lock().unwrap().installed = true;
        },
        WAIT,
        Some("other/repo"),
        &mut out,
    )
    .await
    .unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(passed, "{out}");
    assert_eq!(
        *opened.lock().unwrap(),
        [
            "https://github.example/apps/riff-acme/installations/new/permissions?target_id=7",
            "https://github.example/apps/riff-acme-lead/installations/new/permissions?target_id=7",
        ]
    );
    assert!(out.contains("riff-acme-lead: installed on acme."), "{out}");
    assert!(
        out.ends_with(
            "The Apps are installed. next: run riff github check in a clone of a repository of acme\n"
        ),
        "{out}"
    );
    assert!(
        fake.seen().asked.is_empty(),
        "no check of another repository"
    );

    // A member cannot.
    let member = server.api("bob", None);
    let error = riff::github::install(
        &member,
        &account(),
        "x",
        |_| {},
        WAIT,
        None,
        &mut Vec::new(),
    )
    .await
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("only the owner or an admin"),
        "{error:#}"
    );
}

/// 01M6Z7DMYAW52X62TN1DEX9NCG: both Apps are installed on acme with only
/// acme/app. `riff github install acme --repo acme/b` opens the page
/// Configure of each install, and ends after acme/b is in both: then it
/// checks acme/b.
#[tokio::test]
async fn install_opens_the_page_configure_of_each_install_without_the_repository() {
    let fake = FakeGitHub::start().await;
    fake.seen().installed = true;
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let api = server.api("brett", None);
    let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = opened.clone();
    let fake_seen = fake.seen.clone();
    let mut out = Vec::new();
    let passed = riff::github::install(
        &api,
        &account(),
        WEB,
        move |url| {
            seen.lock().unwrap().push(url.to_owned());
            // The owner of the account adds acme/b to the install of the
            // page: to the install of the lead App 300 ms later.
            let installation: u64 = url.rsplit('/').next().unwrap().parse().unwrap();
            let app = [101, 202]
                .into_iter()
                .find(|id| fake_github::installation_of(*id) == installation)
                .unwrap();
            let delay = Duration::from_millis(if app == 202 { 300 } else { 0 });
            let fake_seen = fake_seen.clone();
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                let mut seen = fake_seen.lock().unwrap();
                seen.added.entry(app).or_default().push("acme/b".into());
            });
        },
        WAIT,
        Some("acme/b"),
        &mut out,
    )
    .await
    .unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(passed, "{out}");
    assert_eq!(
        *opened.lock().unwrap(),
        [
            "https://github.example/organizations/acme/settings/installations/1010",
            "https://github.example/organizations/acme/settings/installations/2020",
        ]
    );
    let lines: Vec<&str> = out.lines().collect();
    let at = |line: &str| {
        lines
            .iter()
            .position(|l| *l == line)
            .unwrap_or_else(|| panic!("{line:?} not in:\n{out}"))
    };
    let order = [
        at("riff-acme: installed on acme."),
        at(
            "riff-acme: the install does not have acme/b. Your browser opens the page Configure \
            of the install. When it does not, open this URL:",
        ),
        at(
            "riff-acme-lead: the install does not have acme/b. Your browser opens the page \
            Configure of the install. When it does not, open this URL:",
        ),
        at("riff-acme: the install has acme/b."),
        at("riff-acme-lead: the install has acme/b."),
        at("riff github check acme/b:"),
        at(
            "riff github check: each role has its rights and no more. riff-server revoked each token.",
        ),
    ];
    assert!(order.is_sorted(), "{order:?}:\n{out}");
    assert!(
        out.contains("Under \"Repository access\", add acme/b, and click \"Save\".\n"),
        "{out}"
    );
    let seen = fake.seen();
    let repos: Vec<&serde_json::Value> = seen
        .asked
        .iter()
        .map(|(_, body)| &body["repositories"])
        .collect();
    assert_eq!(repos, [&serde_json::json!(["b"]); 3]);
}

/// 01M74FVDCJT8P0670P46MWR1MJ, 01M74FVDGM0342M8TZQCCK5H4H: the lead App
/// lacks `vulnerability_alerts` on GitHub, and its install has not
/// accepted `security_events`. `riff github install acme` opens the page
/// of the rights of the App one time, with one line for the right to
/// set, and waits. Then it opens the page Configure of the install one
/// time, waits for the accept, and ends with the check. A second run
/// opens no page, says so in one line, and checks.
#[tokio::test]
async fn install_brings_each_app_and_install_to_the_rights_and_a_second_run_does_nothing() {
    let fake = FakeGitHub::start().await;
    {
        let mut seen = fake.seen();
        seen.installed = true;
        seen.app_lacks = vec!["vulnerability_alerts".into()];
        seen.lacks = vec!["security_events".into()];
    }
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let api = server.api("brett", None);
    let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = opened.clone();
    let fake_seen = fake.seen.clone();
    let mut out = Vec::new();
    let passed = riff::github::install(
        &api,
        &account(),
        WEB,
        move |url| {
            seen.lock().unwrap().push(url.to_owned());
            // The person does each step on GitHub 200 ms later: riff asks
            // riff-server many times in between.
            let rights = url.ends_with("/permissions");
            let fake_seen = fake_seen.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                let mut seen = fake_seen.lock().unwrap();
                match rights {
                    true => seen.save_rights(),
                    false => seen.accept_rights(),
                }
            });
        },
        WAIT,
        Some(REPO),
        &mut out,
    )
    .await
    .unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(passed, "{out}");
    assert_eq!(
        *opened.lock().unwrap(),
        [
            "https://github.example/organizations/acme/settings/apps/riff-acme-lead/permissions",
            "https://github.example/organizations/acme/settings/installations/2020",
        ],
        "each page one time:\n{out}"
    );
    let lines: Vec<&str> = out.lines().collect();
    let at = |line: &str| {
        lines
            .iter()
            .position(|l| *l == line)
            .unwrap_or_else(|| panic!("{line:?} not in:\n{out}"))
    };
    let order = [
        at(
            "riff-acme-lead: the App does not have the rights of riff. Your browser opens the \
             page of its rights. When it does not, open this URL:",
        ),
        at("Set each right, then click \"Save changes\":"),
        at("  Dependabot alerts: Read-only (vulnerability_alerts: read)"),
        at("riff-acme-lead: the App has the rights of riff."),
        at(
            "riff-acme-lead: the install on acme has not accepted the rights of riff: \
             security_events: write, vulnerability_alerts: read. Your browser opens the page \
             Configure of the install. When it does not, open this URL:",
        ),
        at("Click \"Review request\", then \"Accept new permissions\"."),
        at("riff-acme-lead: the install on acme has the rights of riff."),
        at("riff github check acme/app:"),
        at(
            "riff github check: each role has its rights and no more. riff-server revoked each token.",
        ),
    ];
    assert!(order.is_sorted(), "{order:?}:\n{out}");
    assert!(
        !out.contains("riff-acme: the App"),
        "the session App has the rights:\n{out}"
    );
    assert_eq!(fake.seen().tokens.len(), 3);

    // A second run has nothing to do.
    let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = opened.clone();
    let mut out = Vec::new();
    let passed = riff::github::install(
        &api,
        &account(),
        WEB,
        move |url| seen.lock().unwrap().push(url.to_owned()),
        WAIT,
        Some(REPO),
        &mut out,
    )
    .await
    .unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(passed, "{out}");
    assert!(opened.lock().unwrap().is_empty(), "{out}");
    let (first, check) = out.split_once('\n').unwrap();
    assert_eq!(
        first,
        "riff github install changed nothing: both Apps are installed on acme with the rights \
         of riff, and each install has acme/app."
    );
    assert!(check.starts_with("riff github check acme/app:\n"), "{out}");
    assert!(
        check.ends_with(
            "riff github check: each role has its rights and no more. riff-server revoked each \
             token.\n"
        ),
        "{out}"
    );
}

/// 01M6Z7DMYAW52X62TN1DEX9NCG: `riff github install OWNER --repo REPO`
/// refuses a repository of another account, before each call.
#[tokio::test]
async fn install_refuses_a_repository_of_another_account() {
    let fake = FakeGitHub::start().await;
    let server = serve(&fake, At::Memory).await;
    let env = Isolated::new();
    common::sign_in(&server, &env.riff_home(), "brett");
    let out = env
        .tokio_riff()
        .args(["github", "install", OWNER, "--repo", "other/app"])
        .env("RIFF_SERVER", &*server)
        // A plain terminal: the test can run under Claude Code.
        .env_remove("CLAUDECODE")
        .output()
        .await
        .unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains(
            "riff: other/app is no repository of acme. riff github install changed nothing. \
             next: riff github install acme --repo acme/NAME"
        ),
        "{stderr}"
    );
    let calls = server.calls();
    assert!(calls.lock().unwrap().is_empty(), "{calls:?}");
}

/// `riff github install` on a riff with no Apps names `riff github
/// create`.
#[tokio::test]
async fn install_with_no_apps_names_create() {
    let fake = FakeGitHub::start().await;
    let server = serve(&fake, At::Memory).await;
    let api = server.api("brett", None);
    let error = riff::github::install(&api, &account(), "x", |_| {}, WAIT, None, &mut Vec::new())
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("next: riff github create --owner acme"),
        "{error}"
    );
}

/// 01M6WPV8473EBABR1ZMEPZMTQQ: `riff project init` asks riff-server for the bot
/// of the session App. Each person gets it: bob is no admin. A riff with
/// no session App gives none, and a riff-server that does not answer
/// gives why.
#[tokio::test]
async fn each_person_gets_the_bot_of_the_session_app() {
    let fake = FakeGitHub::start().await;
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let bot = riff::guard::session_bot(&server.api("bob", None)).await;
    assert_eq!(bot, Ok(Some("riff-acme[bot]".into())));
    let none = serve(&fake, At::Memory).await;
    assert_eq!(
        riff::guard::session_bot(&none.api("bob", None)).await,
        Ok(None)
    );
    let dead = riff_client::api::Api::new("http://127.0.0.1:9");
    let why = riff::guard::session_bot(&dead).await.unwrap_err();
    assert!(
        why.starts_with(
            "riff cannot ask riff-server at http://127.0.0.1:9 for the bot of the session App: "
        ),
        "{why}"
    );
}

/// 01M6WZ40YED6THKVJ2JQRXFK4N: `riff project init` asks riff-server for the lead
/// App, its App ID and its slug. Each person gets it. A riff with no lead
/// App gives none, and a riff-server that does not answer gives why.
#[tokio::test]
async fn each_person_gets_the_lead_app() {
    let fake = FakeGitHub::start().await;
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let lead = riff::repo_setup::lead_app(&server.api("bob", None)).await;
    assert_eq!(
        lead,
        Ok(Some(riff::repo_setup::LeadApp {
            id: 202,
            slug: "riff-acme-lead".into()
        }))
    );
    let none = serve(&fake, At::Memory).await;
    assert_eq!(
        riff::repo_setup::lead_app(&none.api("bob", None)).await,
        Ok(None)
    );
    let dead = riff_client::api::Api::new("http://127.0.0.1:9");
    let why = riff::repo_setup::lead_app(&dead).await.unwrap_err();
    assert!(
        why.starts_with("riff cannot ask riff-server at http://127.0.0.1:9 for the lead App: "),
        "{why}"
    );
}

/// 01M6WZ40YED6THKVJ2JQRXFK4N: `riff project init --check` in a clone of a
/// repository with the release cycle and no ruleset `releases` names the
/// lead App of the riff in the fix, and changes nothing. With no
/// `[release]` in `riff.toml`, it has no such check.
#[tokio::test]
async fn riff_project_init_check_names_the_ruleset_releases_with_the_lead_app() {
    let fake = FakeGitHub::start().await;
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let env = Isolated::new();
    common::sign_in(&server, &env.riff_home(), "bob");
    let repo = clone();
    let bin = tempfile::tempdir().unwrap();
    isolated::write_program(bin.path().join("gh"), include_str!("fake/forge"));
    for (name, text) in [("files", "riff.toml\n"), ("milestones", "")] {
        std::fs::write(bin.path().join(name), text).unwrap();
    }
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").unwrap()
    );
    let check = || {
        env.tokio_riff()
            .args(["project", "init", "--check"])
            // A plain terminal: the test can run under Claude Code.
            .env_remove("CLAUDECODE")
            .current_dir(repo.path())
            .env("RIFF_SERVER", &*server)
            .env("PATH", &path)
            .output()
    };
    let stdout = String::from_utf8(check().await.unwrap().stdout).unwrap();
    assert!(!stdout.contains("the ruleset releases"), "{stdout}");

    std::fs::write(bin.path().join("config"), "[release]\n").unwrap();
    let out = check().await.unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains(
            "\n  fail  the ruleset releases: acme/app has the release cycle of riff and no \
             ruleset releases, so each session can push a tag v*. Fix: make the ruleset releases \
             on the tags v*, with the admin role and riff-acme-lead as bypass actors. See \"Let \
             the lead push the release tag\" in the book. Then run riff project init again\n"
        ),
        "{stdout}"
    );
    let calls = std::fs::read_to_string(bin.path().join("calls")).unwrap();
    assert!(calls.contains("rulesets?per_page=100"), "{calls}");
    assert!(!calls.contains(" -X "), "{calls}");
}

/// 01M6WPV86SW4AAVA2NCBQNYXK3: `riff project init --check` in a clone of a
/// repository with no guard, in a riff with the session App, names the
/// guard of its bot with the fix, and changes nothing. bob is no admin
/// of the riff.
#[tokio::test]
async fn riff_project_init_check_names_a_repository_with_no_guard() {
    let fake = FakeGitHub::start().await;
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let env = Isolated::new();
    common::sign_in(&server, &env.riff_home(), "bob");
    let repo = clone();
    let bin = tempfile::tempdir().unwrap();
    isolated::write_program(bin.path().join("gh"), include_str!("fake/forge"));
    for (name, text) in [("files", "riff.toml\n"), ("milestones", "")] {
        std::fs::write(bin.path().join(name), text).unwrap();
    }
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").unwrap()
    );
    let out = env
        .tokio_riff()
        .args(["project", "init", "--check"])
        // A plain terminal: the test can run under Claude Code.
        .env_remove("CLAUDECODE")
        .current_dir(repo.path())
        .env("RIFF_SERVER", &*server)
        .env("PATH", path)
        .output()
        .await
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains(
            "\n  fail  the guard: main of acme/app has no .github/workflows/riff-guard.yml, so \
             a change of riff-acme[bot] to an issue stays. Fix: put the guard of \
             riff-acme[bot] on main. See \"Add the guard\" in the book\n"
        ),
        "{stdout}"
    );
    let calls = std::fs::read_to_string(bin.path().join("calls")).unwrap();
    assert!(calls.contains("riff-guard.yml?ref=main"), "{calls}");
    assert!(!calls.contains(" -X "), "{calls}");
}

/// 01M6Z87JTXW0Y99X08VHA2SSSR: `riff project init --check` in a clone of a
/// repository with no installation of the Apps fails a check, and names
/// `riff github install OWNER`. With the Apps on the repository, the
/// check passes.
#[tokio::test]
async fn riff_project_init_check_names_the_install_of_the_apps() {
    let fake = FakeGitHub::start().await;
    let keys = keys_dir();
    let server = serve(&fake, At::Dir(keys.path().to_owned())).await;
    let env = Isolated::new();
    common::sign_in(&server, &env.riff_home(), "bob");
    let repo = clone();
    let init = || {
        env.tokio_riff()
            .args(["project", "init", "--check"])
            // A plain terminal: the test can run under Claude Code.
            .env_remove("CLAUDECODE")
            .current_dir(repo.path())
            .env("RIFF_SERVER", &*server)
            .output()
    };
    let out = init().await.unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let line = "  fail  the GitHub Apps of riff: riff-acme is not installed on acme/app. \
                Fix: riff github install acme\n";
    assert!(stdout.contains(line), "{stdout}");
    // riff-server revoked each token of the check.
    {
        let seen = fake.seen();
        assert_eq!(seen.revoked, seen.tokens);
    }

    fake.seen().installed = true;
    let out = init().await.unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("  pass  the GitHub Apps of riff: on acme/app\n"),
        "{stdout}"
    );
}

/// 01M74A37RFH2HY4YZDWAV07TQF: "How It Works" has "How riff learns of a
/// change on GitHub", with a mermaid diagram, and the how-to "See the
/// last events" with a copyable `sh` block of `riff github events`. "Make
/// the GitHub Apps of riff" links to it.
#[test]
fn the_book_says_how_riff_learns_of_a_change_on_github() {
    let page = crate::book::page("how-it-works.md");
    let part = page
        .split("\n### How riff learns of a change on GitHub\n")
        .nth(1)
        .expect("the part is in How It Works");
    let part = part.split("\n### ").next().unwrap();
    assert!(part.contains("```mermaid\nsequenceDiagram\n"), "{part}");
    let text = part.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "`riff github create` sets it to `URL/hooks/github` of riff-server, with the events \
         `issues`, `issue_comment`, `pull_request`, `check_run`, `status`, `workflow_run` and \
         `merge_group`.",
        "A delivery with no signature or a bad one gets status 401.",
        "A delivery that comes a second time wakes nobody.",
        "An issue that comes into an open wave, and a merge of a pull request, wake the wave of \
         each host at once",
        "riff-server takes the events of one issue or one pull request in 5 seconds into one \
         wake",
        "The wake names the repository, the kinds of the events and the number.",
        "So a host behind a firewall needs no inbound port.",
        "A daemon that was away gets the wakes that it missed when it comes back.",
    ] {
        assert!(text.contains(words), "{words}: {part}");
    }
    let how = part
        .split("\n#### See the last events\n")
        .nth(1)
        .expect("the how-to is in the part");
    let commands = crate::book::commands_in(how);
    assert_eq!(commands, ["riff github events"]);
    crate::book::each_is_real(&commands);
    let team = crate::book::part("start-a-team-riff.md", "Make the GitHub Apps of riff");
    assert!(
        team.contains("(how-it-works.md#how-riff-learns-of-a-change-on-github)"),
        "{team}"
    );
}

/// 01M6WPMWYSW98MQYQCCJ54ZX04, 01M6Z87A04EXTT1FFJSJ1EM6TB: "The auto mode
/// settings of a session" in "How It Works" says who acts on GitHub for
/// each role, and that no session gets the token of the person, with a
/// copyable `sh` block of `riff whoami`.
#[test]
fn the_book_says_who_acts_on_github_for_each_role() {
    let page = crate::book::page("how-it-works.md");
    let settings = page
        .split("\n### The auto mode settings of a session\n")
        .nth(1)
        .expect("the part is in How It Works");
    let settings = settings.split("\n### ").next().unwrap();
    let who = settings
        .split("\n#### Who acts on GitHub for each role\n")
        .nth(1)
        .expect("the how-to is in the part");
    let who = who.split("\n#### ").next().unwrap();
    for row in [
        "| A worker | `riff-OWNER[bot]`",
        "| A verifier | `riff-OWNER[bot]`",
        "| The lead in a container | `riff-OWNER-lead[bot]`",
        // 01M6XZJFE2ZTR5RE3AWM30R704: the daemon writes as the lead App.
        "| The daemon on the host, for its writes | `riff-OWNER-lead[bot]`",
        "| A session of `riff run --no-github` | Nobody: it has no token |",
        "| You in a terminal, a plain `claude` | You |",
    ] {
        assert!(who.contains(row), "{row}: {who}");
    }
    assert!(who.contains("No session gets your token."), "{who}");
    let commands = crate::book::commands_in(who);
    let (gh, riff): (Vec<_>, Vec<_>) = commands.into_iter().partition(|c| c.starts_with("gh "));
    assert_eq!(riff, ["riff whoami", "riff daemon status"]);
    crate::book::each_is_real(&riff);
    // 01M6ZEJ4NEZGQFV3Q4BER5AW3S, 01M6ZHWVPD7MN6GH6GS0GVETHA: the rights
    // of the lead App on the alerts, why, the dismissed alerts, and the
    // token of a lead App with no rights on the alerts.
    assert_eq!(gh.len(), 1, "{gh:?}");
    assert!(
        gh[0].starts_with("gh api 'repos/OWNER/NAME/code-scanning/alerts?state=dismissed' --jq "),
        "{gh:?}"
    );
    let text = who.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "Only the lead App has the rights on the alerts of a repository: \
         `security_events: write` for the code scanning alerts, for example of CodeQL, \
         `secret_scanning_alerts: write` for the secret scanning alerts, and \
         `vulnerability_alerts: read` for the Dependabot alerts.",
        "dismisses a code scanning or a secret scanning alert that is a false positive.",
        "So a failed CodeQL check of a pull request does not wait for a person.",
        "The lead dismisses an alert only after a review, and writes the reason on the alert.",
        "riff-server asks again with the other rights of the lead, and logs one warning with \
         the fix.",
        "`riff github check` names each missing right.",
        "(start-a-team-riff.md#accept-the-new-rights-of-the-lead-app)",
    ] {
        assert!(text.contains(words), "{words}: {who}");
    }
    // The lines of `riff daemon status` are the lines of riff.
    let acts = riff::daemon::gh_token::ActsAs::NoToken {
        why: "riff-acme-lead is not installed on acme/app: riff github install acme".into(),
    };
    let line = riff::text::daemon_writes("acme/app", &acts);
    assert!(who.contains(&format!("\n{line}\n")), "{line}: {who}");
}

/// 01M6YC3SYATQW7GZZV3APFG85B, 01M6YC3T0TXANJH6YMTJZR1HCR: "How It
/// Works" has the how-to "When riff-server cannot read the keys of the
/// Apps", with a copyable `sh` block of the fix, the line of `riff daemon
/// status`, and the prompt for `claude` on the host as riff-server writes
/// it.
#[test]
fn the_book_has_the_how_to_when_riff_server_cannot_read_the_keys() {
    let page = crate::book::page("how-it-works.md");
    let how_to = page
        .split("\n### When riff-server cannot read the keys of the Apps\n")
        .nth(1)
        .expect("the how-to is in How It Works");
    let how_to = how_to.split("\n## ").next().unwrap();
    let how_to = how_to.split("\n### ").next().unwrap();
    let commands = crate::book::commands_in(how_to);
    assert_eq!(
        commands,
        [
            "git pull",
            "riff cloud create shared",
            "riff resume --riff",
            "riff cloud status shared",
            "riff daemon status"
        ]
    );
    let riff: Vec<String> = commands
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    crate::book::each_is_real(&riff);
    let line = riff::text::daemon_gh_none_status(
        "acme/app",
        "riff-server cannot read the keys of the GitHub Apps: ...",
    );
    assert!(how_to.contains(&format!("\n{line}\n")), "{line}: {how_to}");
    // The prompt of the book is the prompt of the line of riff-bot.
    let url = "https://riff.example.com";
    let fault = riff_server::github::keys_fault_text("CAUSE", url);
    let prompt = fault
        .split("paste this prompt: \"")
        .nth(1)
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("the line has the prompt");
    assert!(
        how_to.contains(&format!("\n{prompt}\n")),
        "{prompt}: {how_to}"
    );
}

/// 01M6Z87JTXW0Y99X08VHA2SSSR: "How It Works" has the how-to "Repair riff
/// with claude on the host", with a copyable `sh` block of the fix and
/// one of `claude` with the prompt, as riff writes it.
#[test]
fn the_book_has_the_how_to_repair_riff_with_claude_on_the_host() {
    let page = crate::book::page("how-it-works.md");
    let how_to = page
        .split("\n#### Repair riff with claude on the host\n")
        .nth(1)
        .expect("the how-to is in How It Works");
    let how_to = how_to.split("\n### ").next().unwrap();
    let how_to = how_to.split("\n#### ").next().unwrap();
    let commands = crate::book::commands_in(how_to);
    let why = "riff-acme is not installed on acme/app: riff github install acme";
    let fix = riff::text::gh_repair("acme/app", why);
    let prompt = fix
        .split("paste this prompt: \"")
        .nth(1)
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("the reason has the prompt");
    assert_eq!(
        commands,
        [
            "riff github install acme".to_owned(),
            "riff github check".to_owned(),
            format!("claude \"{prompt}\""),
        ]
    );
    crate::book::each_is_real(&commands[..2]);
    // The lines of `riff daemon status` name the fix and the prompt.
    for line in [
        riff::text::daemon_no_start_status("acme/app", &fix),
        riff::text::daemon_gh_none_status("acme/app", &fix),
    ] {
        assert!(line.contains("riff github install acme"), "{line}");
        assert!(line.ends_with(&format!("\"{prompt}\"")), "{line}");
    }
}

/// 01M6WWG3S60XYGTNBQH7S3F6D1: the token of a verifier reads only, so a
/// verifier adds nothing to an issue. The Claude folder of a verify
/// session allows no `gh issue comment`, its start text names none and
/// sends each finding to the result, and "Add to an issue" in the book
/// says so. A worker keeps the comment.
#[test]
fn a_verifier_puts_each_finding_in_its_result_and_comments_on_no_issue() {
    use riff::permissions::{Role, Trust};

    let dir = tempfile::tempdir().unwrap();
    let auto = Trust {
        owner: Some(OWNER),
        server: Some("http://h:7878"),
        sources: &[],
    };
    let allow_of = |role: Role| -> Vec<String> {
        let claude = dir.path().join(role.as_str());
        let person = dir.path().join("none.json");
        riff::run::claude_folder(&claude, "/data/clone".as_ref(), &person, &auto, role).unwrap();
        let text = std::fs::read_to_string(claude.join("settings.json")).unwrap();
        let settings: serde_json::Value = serde_json::from_str(&text).unwrap();
        serde_json::from_value(settings["permissions"]["allow"].clone()).unwrap()
    };
    let comment = "Bash(gh issue comment *)".to_owned();
    assert!(!allow_of(Role::Verifier).contains(&comment));
    assert!(allow_of(Role::Worker).contains(&comment));

    let checks = ["Gate".to_owned()];
    let text = riff::text::wave_verify_start(REPO, "main", 12, 40, "1a2b3c4", &checks);
    assert!(!text.contains("gh issue comment"), "{text}");
    assert!(text.contains(riff::text::VERIFY_READS_ONLY), "{text}");

    let page = crate::book::page("how-it-works.md");
    let part = page
        .split("\n#### Add to an issue\n")
        .nth(1)
        .expect("the how-to is in How It Works");
    let part = part.split("\n#### ").next().unwrap();
    assert!(
        part.contains("A verifier reads only: it puts each finding in its result"),
        "{part}"
    );
    assert!(
        part.contains("The settings of a session of a worker allow `gh issue comment`."),
        "{part}"
    );
}
