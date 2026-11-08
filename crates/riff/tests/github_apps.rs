//! `riff github create`, `install` and `check` against a real
//! riff-server with a fake GitHub (01M6WG52J7WTP079W1AYEQ4W08,
//! 01M6WG5322WRWQA79RCSCY4MSS, 01M6WG534PPE2JW8JQFRJDEK86,
//! 01M6WG537B67VWVENY3821RV33).

use crate::common;
#[path = "../../riff-server/tests/fake_github.rs"]
mod fake_github;

use fake_github::{FakeGitHub, LEAD_CODE, OWNER, REPO, SESSION_CODE, both_apps};

use std::process::Command;
use std::time::Duration;

use isolated::Isolated;
use riff::github::{Account, Wait};
use riff_server::github::Settings;
use riff_server::keys::{At, FILE};

/// A riff with sign-in whose GitHub is `fake`, with the keys at `keys`.
async fn serve(fake: &FakeGitHub, keys: At) -> common::Signed {
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

/// A folder with both Apps in the file of the keys.
fn keys_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let apps = serde_json::to_vec(&both_apps()).unwrap();
    std::fs::write(dir.path().join(FILE), apps).unwrap();
    dir
}

/// A clone of [`REPO`] on GitHub, with no commit.
fn clone() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let origin = format!("https://github.com/{REPO}.git");
    for args in [&["init", "-q"][..], &["remote", "add", "origin", &origin]] {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status();
        assert!(status.unwrap().success());
    }
    dir
}

/// The account of the fake.
fn account() -> Account {
    Account {
        login: OWNER.into(),
        id: 7,
        kind: "Organization".into(),
    }
}

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
         metadata: read, pull_requests: write, statuses: write\n\
         riff github check: each role has its rights and no more. riff-server revoked each token.\n"
    );
    let seen = fake.seen();
    assert_eq!(seen.tokens.len(), 3);
    assert_eq!(seen.revoked, seen.tokens);
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

/// 01M6WG537B67VWVENY3821RV33: each command refuses in an agent session
/// and in a container, before each call.
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
    let passed = riff::github::create(&api, &account(), open, WAIT, Some(REPO), &mut out)
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
