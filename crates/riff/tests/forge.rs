//! The forge token of a role (#610): a fake GitHub API checks the JWT
//! of the App, the repository and the permissions that riff asks for
//! each role (01M4BV707FYHJDNC1499YAWR8D), that a worker session never
//! gets the rights of the lead (01M4BV70C3P5CZFBSFYFWEWRRA), and that
//! git and `gh` in a session use the token (01M4BV709WGHZ57AM3STC15B69).

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use isolated::Isolated;
use riff::forge::{self, Access, App, Files, ForgeEnv, GitHub, Keeper};
use riff::profile::Role;
use serde_json::{Value, json};

const KEY: &str = include_str!("../../riff-server/testdata/test-only-rsa-key.pem");
const JWKS: &str = include_str!("../../riff-server/testdata/test-only-jwks.json");
const APP: u64 = 123;
const INSTALLATION: u64 = 42;

/// One call of the fake API: the path, the claims of its JWT, its body.
#[derive(Debug, Clone)]
struct Call {
    path: String,
    iss: String,
    body: Value,
}

#[derive(Clone, Default)]
struct Fake {
    calls: Arc<Mutex<Vec<Call>>>,
    /// Permissions that the fake gives on top of the asked ones.
    extra: Arc<Mutex<BTreeMap<String, String>>>,
    /// The fake refuses each revoke while this is true.
    refuse_revoke: Arc<Mutex<bool>>,
}

/// The claims of a JWT of the App, checked with the public key of the
/// test key: RS256, ten minutes at most.
fn app_claims(headers: &HeaderMap) -> Result<Value, StatusCode> {
    let jwt = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let set: jsonwebtoken::jwk::JwkSet = serde_json::from_str(JWKS).unwrap();
    let key = jsonwebtoken::DecodingKey::from_jwk(&set.keys[0]).unwrap();
    let mut check = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    check.set_required_spec_claims(&["exp", "iat", "iss"]);
    let data =
        jsonwebtoken::decode::<Value>(jwt, &key, &check).map_err(|_| StatusCode::UNAUTHORIZED)?;
    let c = data.claims;
    let span = c["exp"].as_u64().unwrap() - c["iat"].as_u64().unwrap();
    assert!(span <= 600, "GitHub takes a JWT of ten minutes at most");
    Ok(c)
}

async fn installation(
    State(fake): State<Fake>,
    Path((owner, repo)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    let claims = app_claims(&headers)?;
    fake.calls.lock().unwrap().push(Call {
        path: format!("/repos/{owner}/{repo}/installation"),
        iss: claims["iss"].as_str().unwrap().into(),
        body: Value::Null,
    });
    Ok(Json(json!({ "id": INSTALLATION })))
}

async fn access_tokens(
    State(fake): State<Fake>,
    Path(id): Path<u64>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let claims = app_claims(&headers)?;
    assert_eq!(id, INSTALLATION);
    fake.calls.lock().unwrap().push(Call {
        path: format!("/app/installations/{id}/access_tokens"),
        iss: claims["iss"].as_str().unwrap().into(),
        body: body.clone(),
    });
    let mut given = body["permissions"].as_object().cloned().unwrap_or_default();
    for (k, v) in fake.extra.lock().unwrap().iter() {
        given.insert(k.clone(), json!(v));
    }
    let n = fake.calls.lock().unwrap().len();
    Ok(Json(json!({
        "token": format!("ghs_test_{n}"),
        "expires_at": (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
        "permissions": given,
        "repository_selection": "selected",
    })))
}

/// A revoke: its call has the path, and the revoked token as its body.
async fn revoke(State(fake): State<Fake>, headers: HeaderMap) -> StatusCode {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    fake.calls.lock().unwrap().push(Call {
        path: "/installation/token".into(),
        iss: String::new(),
        body: json!(token),
    });
    if *fake.refuse_revoke.lock().unwrap() {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::NO_CONTENT
    }
}

/// A fake GitHub API on a free port, and its URL.
async fn fake_github() -> (Fake, String) {
    let fake = Fake::default();
    let app = Router::new()
        .route("/repos/{owner}/{repo}/installation", get(installation))
        .route("/app/installations/{id}/access_tokens", post(access_tokens))
        .route("/installation/token", delete(revoke))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (fake, url)
}

fn app() -> App {
    App::new(APP, KEY.as_bytes()).unwrap()
}

/// The permissions of `role` as the fake gets them in a body.
fn asked(role: Role) -> Value {
    serde_json::to_value(forge::permissions(role)).unwrap()
}

#[tokio::test]
async fn each_role_asks_for_its_permissions_on_one_repository() {
    let (fake, url) = fake_github().await;
    let github = GitHub::new(&url);
    for role in [Role::Lead, Role::Worker, Role::Verifier] {
        let token = github
            .token(&app(), "como-technologies/riff", role)
            .await
            .unwrap();
        assert_eq!(token.role, role);
        assert!(token.token.starts_with("ghs_test_"));
        let last = fake.calls.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last.path, "/app/installations/42/access_tokens");
        assert_eq!(last.iss, APP.to_string(), "the JWT names the App");
        assert_eq!(last.body["repositories"], json!(["riff"]), "one repository");
        assert_eq!(last.body["permissions"], asked(role), "{role}");
    }
    let calls = fake.calls.lock().unwrap();
    assert!(
        calls
            .iter()
            .any(|c| c.path == "/repos/como-technologies/riff/installation")
    );
}

#[tokio::test]
async fn the_test_run_gets_no_token() {
    let (fake, url) = fake_github().await;
    let err = GitHub::new(&url)
        .token(&app(), "como-technologies/riff", Role::TestRun)
        .await
        .unwrap_err();
    assert!(format!("{err:#}").contains("no forge token"), "{err:#}");
    assert!(fake.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_token_with_more_rights_than_its_role_is_refused() {
    let (fake, url) = fake_github().await;
    fake.extra
        .lock()
        .unwrap()
        .insert("administration".into(), "write".into());
    let dir = tempfile::tempdir().unwrap();
    let files = Files::in_temp(dir.path());
    let mut keeper = Keeper::new(
        app(),
        GitHub::new(&url),
        "como-technologies/riff".into(),
        files.clone(),
    );
    let err = keeper.step(Some(&[])).await.unwrap_err();
    assert!(format!("{err:#}").contains("administration"), "{err:#}");
    assert!(files.token().is_err(), "no token file for a refused token");
}

#[tokio::test]
async fn a_worker_session_gets_the_token_of_its_claims_and_never_of_the_lead() {
    let (fake, url) = fake_github().await;
    let dir = tempfile::tempdir().unwrap();
    let files = Files::in_temp(dir.path());
    let mut keeper = Keeper::new(
        app(),
        GitHub::new(&url),
        "como-technologies/riff".into(),
        files.clone(),
    );
    let claims = |c: &[&str]| c.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    let first = keeper.step(Some(&[])).await.unwrap().unwrap();
    assert_eq!(first.role, Role::Worker);
    assert_eq!(files.token().unwrap(), first.token);
    // The same role and a fresh token: no new token.
    assert!(
        keeper
            .step(Some(&claims(&["issue-12"])))
            .await
            .unwrap()
            .is_none()
    );
    // A verify claim gives the verifier token.
    let verify = keeper
        .step(Some(&claims(&["verify-issue-12"])))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(verify.role, Role::Verifier);
    assert_eq!(verify.permissions["statuses"], Access::Write);
    assert_eq!(files.token().unwrap(), verify.token);
    // riff-server does not answer: the role stays.
    assert!(keeper.step(None).await.unwrap().is_none());
    // The release gives the worker token again.
    let back = keeper.step(Some(&[])).await.unwrap().unwrap();
    assert_eq!(back.role, Role::Worker);

    // Each token of the session is of the worker or of the verifier,
    // never of the lead.
    let roles = [first.role, verify.role, back.role];
    assert!(!roles.contains(&Role::Lead), "{roles:?}");
    let (worker, verifier) = (asked(Role::Worker), asked(Role::Verifier));
    for call in fake.calls.lock().unwrap().iter() {
        if call.path.ends_with("access_tokens") {
            let p = &call.body["permissions"];
            assert!(*p == worker || *p == verifier, "{p}");
        }
    }
}

#[tokio::test]
async fn a_change_of_role_revokes_the_old_token_before_it_asks_for_the_new_one() {
    let (fake, url) = fake_github().await;
    let dir = tempfile::tempdir().unwrap();
    let files = Files::in_temp(dir.path());
    let mut keeper = Keeper::new(
        app(),
        GitHub::new(&url),
        "como-technologies/riff".into(),
        files.clone(),
    );
    let verify = ["verify-issue-12".to_owned()];
    let worker = keeper.step(Some(&[])).await.unwrap().unwrap();

    // GitHub refuses the revoke: the session has no token, and no
    // verifier token is made.
    *fake.refuse_revoke.lock().unwrap() = true;
    let asks = |fake: &Fake| {
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.path.ends_with("access_tokens"))
            .count()
    };
    let before = asks(&fake);
    let err = keeper.step(Some(&verify)).await.unwrap_err();
    assert!(format!("{err:#}").contains("revoke"), "{err:#}");
    assert!(files.token().is_err(), "no token while the old one lives");
    assert_eq!(asks(&fake), before, "no new token before the revoke");

    // The next step revokes the old token, then asks for the new one.
    *fake.refuse_revoke.lock().unwrap() = false;
    let verifier = keeper.step(Some(&verify)).await.unwrap().unwrap();
    assert_eq!(verifier.role, Role::Verifier);
    assert_eq!(files.token().unwrap(), verifier.token);
    {
        let calls = fake.calls.lock().unwrap();
        let revoked = calls
            .iter()
            .rposition(|c| c.path == "/installation/token" && c.body == json!(worker.token))
            .unwrap();
        let asked_new = calls
            .iter()
            .rposition(|c| c.path.ends_with("access_tokens"))
            .unwrap();
        assert!(revoked < asked_new, "{calls:?}");
    }

    // The same role and a fresh token: no call.
    let n = fake.calls.lock().unwrap().len();
    assert!(keeper.step(Some(&verify)).await.unwrap().is_none());
    assert_eq!(fake.calls.lock().unwrap().len(), n);
}

/// The token of `files` for each program of a session: git through the
/// credential helper of riff, and `gh` through its config dir. The
/// token of the person in the environment does not reach them.
#[test]
fn git_and_gh_in_a_session_use_the_token_of_the_files() {
    let env = Isolated::new();
    let files = Files::in_temp(&env.path().join("tmp"));
    files
        .write(&forge::Token {
            role: Role::Worker,
            token: "ghs_session".into(),
            ends: std::time::SystemTime::now(),
            permissions: BTreeMap::new(),
        })
        .unwrap();
    let token = files.token().map(|token| forge::Token {
        role: Role::Worker,
        token,
        ends: std::time::SystemTime::now(),
        permissions: BTreeMap::new(),
    });
    let given = token.map_err(forge::Error::Files);
    let forge_env = ForgeEnv::of(&given, &files, &env.riff_path());
    let mut parent: Vec<_> = env
        .command("git")
        .get_envs()
        .filter_map(|(k, v)| Some((k.to_owned(), v?.to_owned())))
        .collect();
    parent.push(("PATH".into(), std::env::var_os("PATH").unwrap()));
    parent.push(("GH_TOKEN".into(), "ghp_person".into()));
    let mut git = forge_env
        .command("git".as_ref(), &["credential".into(), "fill".into()], parent)
        .into_std();
    git.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let mut child = git.spawn().unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"protocol=https\nhost=github.com\npath=como-technologies/riff.git\n\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("username=x-access-token"), "{text}");
    assert!(text.contains("password=ghs_session"), "{text}");

    let gh_dir = forge_env.get("GH_CONFIG_DIR").unwrap();
    let hosts = std::fs::read_to_string(std::path::Path::new(gh_dir).join("hosts.yml")).unwrap();
    assert!(hosts.contains("oauth_token: ghs_session"), "{hosts}");
}

/// A clone of `como-technologies/riff` in a temp dir.
fn clone() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ],
    ] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(ok.success());
    }
    dir
}

#[tokio::test]
async fn riff_forge_app_saves_the_app_and_riff_forge_check_shows_each_role() {
    let (_fake, url) = fake_github().await;
    let env = Isolated::new();
    let key = env.path().join("downloaded.pem");
    std::fs::write(&key, KEY).unwrap();
    let out = env
        .riff()
        .args(["forge", "app", &APP.to_string()])
        .arg(&key)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("riff saved the GitHub App 123"), "{text}");
    let saved = env.riff_home().join("forge/app.pem");
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&saved).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0, "only the person reads the key");
    let settings = std::fs::read_to_string(env.riff_home().join("config.toml")).unwrap();
    assert!(settings.contains("app = 123"), "{settings}");

    // A file that is not a key is refused, and nothing changes.
    let bad = env.path().join("bad.pem");
    std::fs::write(&bad, "not a key").unwrap();
    let out = env
        .riff()
        .args(["forge", "app", "9"])
        .arg(&bad)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not an RSA private key"));

    let repo = clone();
    let mut check = env.riff();
    check
        .args(["forge", "check"])
        .current_dir(repo.path())
        .env(forge::API_VAR, &url);
    let out = tokio::task::spawn_blocking(move || check.output().unwrap())
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{text} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("lead: "), "{text}");
    assert!(text.contains("worker: "), "{text}");
    assert!(text.contains("verifier: actions read"), "{text}");
    assert!(text.contains("statuses write"), "{text}");
    assert!(
        !text.contains("ghs_"),
        "riff forge check shows no token: {text}"
    );
}

#[test]
fn riff_forge_credential_gives_the_token_only_for_github() {
    let env = Isolated::new();
    let files = Files::in_temp(&env.path().join("tmp"));
    files
        .write(&forge::Token {
            role: Role::Worker,
            token: "ghs_cred".into(),
            ends: std::time::SystemTime::now(),
            permissions: BTreeMap::new(),
        })
        .unwrap();
    let run = |input: &str| {
        let mut cmd = env.riff();
        cmd.args(["forge", "credential", "get"])
            .env(forge::DIR_VAR, files.dir())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(
        run("protocol=https\nhost=github.com\n\n"),
        "username=x-access-token\npassword=ghs_cred\n"
    );
    assert_eq!(run("protocol=https\nhost=gitlab.com\n\n"), "");
}

/// The marker of each credential of the person in the marker test.
const MARKER: &str = "person_marker";

/// One outcome of the token step for the marker test: its settings, a
/// key file, a clone or no repository, a session ID, the GitHub API.
struct Outcome {
    name: &'static str,
    settings: Option<&'static str>,
    key: bool,
    repo: bool,
    session: bool,
    api: Option<String>,
    /// The words of the line of the wrapper, or `None` with a token.
    line: Option<&'static str>,
}

/// Runs `riff workers run` with a fake `claude` for `outcome`, with a
/// credential of the person in each place that a program reads: the
/// token variables, an unknown variable, the agent of ssh, the `gh`
/// config and a git helper. Returns what `claude` saw and the stderr
/// of the wrapper.
async fn run_worker(server: &str, outcome: &Outcome) -> (String, String) {
    let dir = if outcome.repo {
        clone()
    } else {
        tempfile::tempdir().unwrap()
    };
    let root = dir.path();
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".config/gh")).unwrap();
    std::fs::write(
        home.join(".config/gh/hosts.yml"),
        format!("github.com:\n    oauth_token: {MARKER}\n"),
    )
    .unwrap();
    let gitconfig = home.join(".gitconfig");
    std::fs::write(
        &gitconfig,
        format!("[credential]\n\thelper = \"!f() {{ echo username=me; echo password={MARKER}; }}; f\"\n"),
    )
    .unwrap();
    let riff_home = root.join("riff-home");
    std::fs::create_dir_all(&riff_home).unwrap();
    if let Some(settings) = outcome.settings {
        std::fs::write(riff_home.join("config.toml"), settings).unwrap();
    }
    if outcome.key {
        let key = App::key_path(&riff_home.join("config.toml"));
        std::fs::create_dir_all(key.parent().unwrap()).unwrap();
        std::fs::write(key, KEY).unwrap();
    }
    let out = root.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let claude = root.join("claude");
    let o = out.display();
    std::fs::write(
        &claude,
        format!(
            "#!/bin/sh\nenv > '{o}/env'\n\
             printf 'protocol=https\\nhost=github.com\\n\\n' | \
             GIT_TERMINAL_PROMPT=0 git credential fill > '{o}/git' 2>&1\n\
             cat \"$GH_CONFIG_DIR/hosts.yml\" > '{o}/gh' 2>&1\n"
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut cmd = Isolated::shared().riff();
    cmd.current_dir(root)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", &riff_home)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("GIT_CONFIG_GLOBAL", &gitconfig)
        .env("TMUX_PANE", "%5")
        .env("GH_TOKEN", MARKER)
        .env("GITHUB_TOKEN", MARKER)
        .env("RIFF_TEST_MARKER", MARKER)
        .env("SSH_AUTH_SOCK", root.join(MARKER))
        .env_remove("RIFF_WORKER")
        .env_remove("CLAUDE_CODE_SESSION_ID");
    match &outcome.api {
        Some(api) => cmd.env(forge::API_VAR, api),
        None => cmd.env_remove(forge::API_VAR),
    };
    if outcome.session {
        cmd.env("RIFF_SESSION", format!("w-{}", outcome.name));
    } else {
        cmd.env_remove("RIFF_SESSION");
    }
    let run = cmd.args(["workers", "run"]).arg(&claude).output().unwrap();
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    let seen = ["env", "git", "gh"]
        .map(|f| std::fs::read_to_string(out.join(f)).unwrap_or_default())
        .join("\n");
    assert!(!seen.is_empty(), "{}: claude did not run: {stderr}", outcome.name);
    (seen, stderr)
}

/// No outcome of the token step gives `claude` a credential of the
/// person: not with a token, not with no App, not with a fault
/// (01M4BYVSNQ5SY2GRGT73FV0Z3E, 01M4BYVSR06B9HNX4SP83SY2SX).
#[tokio::test(flavor = "multi_thread")]
async fn no_outcome_of_the_token_step_gives_claude_a_credential_of_the_person() {
    let (_fake, github) = fake_github().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, riff_server::router()).await.unwrap() });
    let app = "[forge]\napp = 123\n";
    let base = || Outcome {
        name: "",
        settings: Some(app),
        key: true,
        repo: true,
        session: true,
        api: Some(github.clone()),
        line: None,
    };
    let outcomes = [
        Outcome {
            name: "token",
            ..base()
        },
        Outcome {
            name: "no-settings",
            settings: None,
            line: Some("the settings name no GitHub App"),
            ..base()
        },
        Outcome {
            name: "no-app",
            settings: Some("[forge]\n"),
            line: Some("the settings name no GitHub App"),
            ..base()
        },
        Outcome {
            name: "bad-setting",
            settings: Some("[forge]\napp = \"123\"\n"),
            line: Some("is not a number"),
            ..base()
        },
        Outcome {
            name: "no-key",
            key: false,
            line: Some("cannot read the key of the App"),
            ..base()
        },
        Outcome {
            name: "no-repository",
            repo: false,
            line: Some("in no repository"),
            ..base()
        },
        Outcome {
            name: "no-session",
            session: false,
            line: Some("the session has no temp folder"),
            ..base()
        },
        Outcome {
            name: "api",
            api: Some("http://127.0.0.1:9".into()),
            line: Some("cannot reach the GitHub API"),
            ..base()
        },
    ];
    for outcome in &outcomes {
        let (seen, stderr) = run_worker(&server, outcome).await;
        let name = outcome.name;
        assert!(!seen.contains(MARKER), "{name}: claude saw: {seen}");
        assert!(!seen.contains("SSH_AUTH_SOCK"), "{name}: {seen}");
        assert!(seen.contains("RIFF_FORGE_DIR="), "{name}: {seen}");
        match outcome.line {
            None => {
                assert!(seen.contains("password=ghs_test_"), "{name}: {seen}");
                assert!(!stderr.contains("no forge token"), "{name}: {stderr}");
            }
            Some(line) => {
                assert!(!seen.contains("password="), "{name}: {seen}");
                assert!(stderr.contains("no forge token"), "{name}: {stderr}");
                assert!(stderr.contains(line), "{name}: {stderr}");
            }
        }
    }
}
