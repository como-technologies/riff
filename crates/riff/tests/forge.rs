//! The forge token of a session on the side of `riff` (#610, #628): the
//! wrapper asks riff-server for the token of its session
//! (01M4CNN37FYYB99BS6QV2FFWZ8), keeps it fresh, and gives it to git
//! and `gh` (01M4BV709WGHZ57AM3STC15B69). No outcome gives `claude` a
//! credential of the person (01M4BYVSNQ5SY2GRGT73FV0Z3E). The tests of
//! the server side are in `riff-server/tests/forge.rs`.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::routing::post;
use isolated::Isolated;
use riff::forge::{self, Access, Files, ForgeEnv, Keeper, TokenRole};
use riff_core::wire::{ForgeAccounts, ForgeCheckReply, ForgeTokenReply, RoleCheck};
use serde_json::{Value, json};

/// A reply of the server: a token of `role` that ends in one hour.
fn reply(role: TokenRole, n: usize) -> ForgeTokenReply {
    let ends = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
    ForgeTokenReply {
        role,
        repo: "como-technologies/riff".into(),
        token: format!("ghs_test_{n}"),
        ends_ms: ends
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
        permissions: riff_core::forge::permissions(role)
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    }
}

/// A fake server for the keeper: it gives the token of the role that
/// the test sets, or refuses when the role is `None`. It counts the
/// asks, and sees whether the token files were empty at each ask.
#[derive(Clone, Default)]
struct Server {
    role: Arc<Mutex<Option<TokenRole>>>,
    asks: Arc<Mutex<Vec<bool>>>,
}

impl Server {
    fn ask(&self, files: Files) -> forge::Ask {
        let me = self.clone();
        Box::new(move || {
            let (me, files) = (me.clone(), files.clone());
            Box::pin(async move {
                let mut asks = me.asks.lock().unwrap();
                asks.push(files.token().is_err());
                let n = asks.len();
                match *me.role.lock().unwrap() {
                    Some(role) => Ok(reply(role, n)),
                    None => anyhow::bail!("this riff has no GitHub App"),
                }
            })
        })
    }

    fn set(&self, role: Option<TokenRole>) {
        *self.role.lock().unwrap() = role;
    }

    fn asks(&self) -> usize {
        self.asks.lock().unwrap().len()
    }
}

#[tokio::test]
async fn the_keeper_asks_the_server_at_a_change_of_role_and_not_more() {
    let dir = tempfile::tempdir().unwrap();
    let files = Files::in_temp(dir.path());
    let server = Server::default();
    server.set(Some(TokenRole::Worker));
    let mut keeper = Keeper::new(files.clone(), server.ask(files.clone()));
    let claims = |c: &[&str]| c.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    let first = keeper.step(Some(&[])).await.unwrap().unwrap();
    assert_eq!(first.role, TokenRole::Worker);
    assert_eq!(files.token().unwrap(), first.token);
    // The same role and a fresh token: no ask.
    let same = keeper.step(Some(&claims(&["issue-12"]))).await.unwrap();
    assert!(same.is_none());
    assert_eq!(server.asks(), 1);

    // A verify claim: the keeper removes the old token, then asks.
    server.set(Some(TokenRole::Verifier));
    let verify = keeper
        .step(Some(&claims(&["verify-issue-12"])))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(verify.role, TokenRole::Verifier);
    assert_eq!(verify.permissions["statuses"], Access::Write);
    assert_eq!(files.token().unwrap(), verify.token);
    assert_eq!(
        *server.asks.lock().unwrap(),
        [true, true],
        "no token file at the ask of the new role"
    );
    // riff-server does not answer: the role stays.
    assert!(keeper.step(None).await.unwrap().is_none());
    assert_eq!(server.asks(), 2);
}

#[tokio::test]
async fn a_refusal_of_the_server_leaves_no_token() {
    let dir = tempfile::tempdir().unwrap();
    let files = Files::in_temp(dir.path());
    let server = Server::default();
    server.set(Some(TokenRole::Worker));
    let mut keeper = Keeper::new(files.clone(), server.ask(files.clone()));
    keeper.step(Some(&[])).await.unwrap().unwrap();
    // The verify claim comes, and the server refuses: the session has
    // no token, not the old one.
    server.set(None);
    let err = keeper
        .step(Some(&["verify-issue-1".into()]))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no GitHub App"), "{err}");
    assert!(files.token().is_err(), "no token of the old role");
}

#[tokio::test]
async fn the_keeper_of_the_lead_keeps_its_role() {
    let dir = tempfile::tempdir().unwrap();
    let files = Files::in_temp(dir.path());
    let server = Server::default();
    server.set(Some(TokenRole::Lead));
    let mut keeper =
        Keeper::new(files.clone(), server.ask(files.clone())).with_role(TokenRole::Lead);
    let first = keeper.first(None).await.unwrap();
    assert_eq!(first.role, TokenRole::Lead);
    // Claims do not change the role of the lead.
    let step = keeper.step(Some(&["verify-issue-1".into()])).await.unwrap();
    assert!(step.is_none());
}

/// The token of `files` for each program of a session: git through the
/// credential helper of riff, and `gh` through its config dir. The
/// token of the person in the environment does not reach them.
#[test]
fn git_and_gh_in_a_session_use_the_token_of_the_files() {
    let env = Isolated::new();
    let files = Files::in_temp(&env.path().join("tmp"));
    let token = forge::Token {
        role: TokenRole::Worker,
        token: "ghs_session".into(),
        ends: std::time::SystemTime::now(),
        permissions: BTreeMap::new(),
    };
    files.write(&token).unwrap();
    let forge_env = ForgeEnv::of(&Ok(token), &files, &env.riff_path());
    let mut parent: Vec<_> = env
        .command("git")
        .get_envs()
        .filter_map(|(k, v)| Some((k.to_owned(), v?.to_owned())))
        .collect();
    parent.push(("PATH".into(), std::env::var_os("PATH").unwrap()));
    parent.push(("GH_TOKEN".into(), "ghp_person".into()));
    let mut git = forge_env
        .command(
            "git".as_ref(),
            &["credential".into(), "fill".into()],
            parent,
        )
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

/// A riff-server on a free port whose forge routes the test gives: a
/// riff with no sign-in gives no token, so the test answers in its
/// place. Each other call goes to a real server. The bodies that the
/// routes got are in the list, with each body of a register.
async fn server_with(
    token: Option<ForgeTokenReply>,
    check: Option<ForgeCheckReply>,
) -> (String, Arc<Mutex<Vec<Value>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (s1, s2, s3) = (seen.clone(), seen.clone(), seen.clone());
    let mut router = axum::Router::new().route(
        "/v1/register",
        post(move |Json(body): Json<Value>| async move {
            s3.lock().unwrap().push(body);
            Json(Value::Null)
        }),
    );
    if let Some(token) = token {
        router = router.route(
            "/v1/forge/token",
            post(move |Json(body): Json<Value>| async move {
                s1.lock().unwrap().push(body);
                Json(token)
            }),
        );
    }
    if let Some(check) = check {
        router = router.route(
            "/v1/forge/check",
            post(move |Json(body): Json<Value>| async move {
                s2.lock().unwrap().push(body);
                Json(check)
            }),
        );
    }
    let router = router
        .layer(axum::middleware::map_response(
            |mut r: axum::response::Response| async move {
                let build = riff_core::build::VERSION.parse().unwrap();
                r.headers_mut().insert(riff_core::build::HEADER, build);
                r
            },
        ))
        .fallback_service(riff_server::router());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (url, seen)
}

#[tokio::test]
async fn riff_forge_check_shows_each_role_of_the_server_and_no_token() {
    let roles = TokenRole::ALL
        .into_iter()
        .map(|role| RoleCheck {
            role,
            permissions: reply(role, 0).permissions,
            error: None,
        })
        .collect();
    let check = ForgeCheckReply {
        repo: "como-technologies/riff".into(),
        app: 123,
        roles,
    };
    let (url, seen) = server_with(None, Some(check)).await;
    let env = Isolated::new();
    let repo = clone();
    let mut cmd = env.riff();
    cmd.args(["forge", "check"])
        .current_dir(repo.path())
        .env("RIFF_SERVER", &url)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin");
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{text} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("riff-server checked the GitHub App 123 on como-technologies/riff"),
        "{text}"
    );
    assert!(text.contains("lead: "), "{text}");
    assert!(text.contains("worker: "), "{text}");
    assert!(text.contains("verifier: actions read"), "{text}");
    assert!(text.contains("statuses write"), "{text}");
    assert!(!text.contains("ghs_"), "{text}");
    let body = seen.lock().unwrap().pop().unwrap();
    assert_eq!(
        body["me"], "riff://mike@pangolin/como-technologies/riff",
        "riff asks as the person at the clone"
    );
}

#[test]
fn riff_forge_credential_gives_the_token_only_for_github() {
    let env = Isolated::new();
    let files = Files::in_temp(&env.path().join("tmp"));
    files
        .write(&forge::Token {
            role: TokenRole::Worker,
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

/// One outcome of the token step for the marker test: a clone or no
/// repository, a session ID, the server.
struct Outcome {
    name: &'static str,
    repo: bool,
    session: bool,
    server: String,
    /// The words of the line of the wrapper, or `None` with a token.
    line: Option<&'static str>,
}

/// Runs `riff workers run` with a fake `claude` for `outcome`, with a
/// credential of the person in each place that a program reads: the
/// token variables, an unknown variable, the agent of ssh, the `gh`
/// config and a git helper. Returns what `claude` saw and the stderr
/// of the wrapper.
async fn run_worker(outcome: &Outcome) -> (String, String) {
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
        format!(
            "[credential]\n\thelper = \"!f() {{ echo username=me; echo password={MARKER}; }}; f\"\n"
        ),
    )
    .unwrap();
    let riff_home = root.join("riff-home");
    std::fs::create_dir_all(&riff_home).unwrap();
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
        .env("RIFF_SERVER", &outcome.server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", &riff_home)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("GIT_CONFIG_GLOBAL", &gitconfig)
        .env("TMUX_PANE", "%5")
        .env("GH_TOKEN", MARKER)
        .env("GITHUB_TOKEN", MARKER)
        .env("ANTHROPIC_API_KEY", MARKER)
        .env("ANTHROPIC_AUTH_TOKEN", MARKER)
        .env("RIFF_TEST_MARKER", MARKER)
        .env("SSH_AUTH_SOCK", root.join(MARKER))
        .env_remove("RIFF_WORKER")
        .env_remove("CLAUDE_CODE_SESSION_ID");
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
    assert!(
        !seen.is_empty(),
        "{}: claude did not run: {stderr}",
        outcome.name
    );
    (seen, stderr)
}

/// No outcome of the token step gives `claude` a credential of the
/// person: not with a token, not with a refusal of the server, not with
/// a fault (01M4BYVSNQ5SY2GRGT73FV0Z3E, 01M4BYVSR06B9HNX4SP83SY2SX).
#[tokio::test(flavor = "multi_thread")]
async fn no_outcome_of_the_token_step_gives_claude_a_credential_of_the_person() {
    let (gives, seen) = server_with(Some(reply(TokenRole::Worker, 1)), None).await;
    // A real riff with no sign-in refuses.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let refuses = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, riff_server::router()).await.unwrap() });
    let base = || Outcome {
        name: "",
        repo: true,
        session: true,
        server: gives.clone(),
        line: None,
    };
    let outcomes = [
        Outcome {
            name: "token",
            ..base()
        },
        Outcome {
            name: "no-sign-in",
            server: refuses.clone(),
            line: Some("a riff with no sign-in gives no forge token"),
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
    ];
    for outcome in &outcomes {
        let (seen_by_claude, stderr) = run_worker(outcome).await;
        let name = outcome.name;
        let seen_by_claude = seen_by_claude.as_str();
        assert!(
            !seen_by_claude.contains(MARKER),
            "{name}: claude saw: {seen_by_claude}"
        );
        assert!(
            !seen_by_claude.contains("SSH_AUTH_SOCK"),
            "{name}: {seen_by_claude}"
        );
        assert!(
            seen_by_claude.contains("RIFF_FORGE_DIR="),
            "{name}: {seen_by_claude}"
        );
        match outcome.line {
            None => {
                assert!(
                    seen_by_claude.contains("password=ghs_test_1"),
                    "{name}: {seen_by_claude}"
                );
                assert!(!stderr.contains("no forge token"), "{name}: {stderr}");
            }
            Some(line) => {
                assert!(
                    !seen_by_claude.contains("password="),
                    "{name}: {seen_by_claude}"
                );
                assert!(stderr.contains("no forge token"), "{name}: {stderr}");
                assert!(stderr.contains(line), "{name}: {stderr}");
            }
        }
    }
    // The wrapper registered its session as a worker, so that the
    // server knows it, then asked with the URI of its session
    // (01M4CNN37FYYB99BS6QV2FFWZ8).
    let me = "riff://mike@pangolin/como-technologies/riff?session=w-token";
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen[0], json!({ "me": me, "worker": true }));
    assert_eq!(seen[1], json!({ "me": me }));
}

#[tokio::test]
async fn riff_forge_allow_sends_the_account_and_shows_the_list() {
    let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
    let s = seen.clone();
    let router = axum::Router::new()
        .route(
            "/v1/forge/allow",
            post(move |Json(body): Json<Value>| async move {
                s.lock().unwrap().push(body);
                Json(ForgeAccounts {
                    accounts: vec!["acme".into(), "mike".into()],
                })
            }),
        )
        .layer(axum::middleware::map_response(
            |mut r: axum::response::Response| async move {
                let build = riff_core::build::VERSION.parse().unwrap();
                r.headers_mut().insert(riff_core::build::HEADER, build);
                r
            },
        ))
        .fallback_service(riff_server::router());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let env = Isolated::new();
    for args in [&["acme"][..], &["acme", "--remove"], &[]] {
        let mut cmd = env.riff();
        cmd.args(["forge", "allow"])
            .args(args)
            .env("RIFF_SERVER", &url)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin");
        let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(out.status.success(), "{args:?}: {text}");
        assert!(
            text.contains("riff-server makes forge tokens for the repositories of: acme, mike"),
            "{text}"
        );
    }
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen[0]["owner"], "acme");
    assert_eq!(seen[0]["allowed"], true);
    assert_eq!(seen[1]["allowed"], false);
    assert!(seen[2].get("owner").is_none());
    assert!(
        seen[0]["me"]
            .as_str()
            .unwrap()
            .starts_with("riff://mike@pangolin")
    );
}

/// A riff-server on a free port with the routes of `router`. Each other
/// call goes to a real server.
async fn serve(router: axum::Router) -> String {
    let router = router
        .layer(axum::middleware::map_response(
            |mut r: axum::response::Response| async move {
                let build = riff_core::build::VERSION.parse().unwrap();
                r.headers_mut().insert(riff_core::build::HEADER, build);
                r
            },
        ))
        .fallback_service(riff_server::router());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    url
}

/// A program for `RIFF_BROWSER`: it writes each page that riff opens to
/// the file `opened` in `dir`.
fn browser(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let program = dir.join("browser");
    let opened = dir.join("opened");
    std::fs::write(
        &program,
        format!("#!/bin/sh\necho \"$1\" >> '{}'\n", opened.display()),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    program
}

/// Each page that the browser of [`browser`] opened.
fn opened(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("opened"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// 01M4CTAYZW24PDP2PDK72PT6J1: a person runs `riff forge create` and
/// `riff forge install` in a terminal. A worker and an agent session
/// get a refusal, and riff calls no server.
#[tokio::test]
async fn riff_forge_create_and_install_refuse_in_a_worker_and_an_agent_session() {
    let seen = Arc::new(Mutex::new(0));
    let s = seen.clone();
    let url = serve(axum::Router::new().route(
        "/v1/forge/create",
        post(move || async move {
            *s.lock().unwrap() += 1;
            Json(Value::Null)
        }),
    ))
    .await;
    let env = Isolated::new();
    let runs: [(&[&str], (&str, &str)); 4] = [
        (&["create", "--org", "acme"], ("RIFF_WORKER", "1")),
        (&["create", "--org", "acme"], ("RIFF_SESSION", "s1")),
        (&["create", "--org", "acme"], ("CLAUDE_CODE_SESSION_ID", "s1")),
        (&["install", "acme"], ("RIFF_WORKER", "1")),
    ];
    for (args, (name, value)) in runs {
        let mut cmd = env.riff();
        cmd.arg("forge")
            .args(args)
            .env("RIFF_SERVER", &url)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env(name, value);
        let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(!out.status.success(), "{args:?} {name}: {stderr}");
        assert!(
            stderr.contains("not in a worker or an agent session"),
            "{args:?} {name}: {stderr}"
        );
    }
    assert_eq!(*seen.lock().unwrap(), 0, "riff called no server");
}

/// 01M4CTAZ24PM55TKMP714K3CC4: `riff forge install n8behavior` opens the
/// install page of the App with that account as the target, waits for
/// the install, and says where to run the check.
#[tokio::test]
async fn riff_forge_install_opens_the_install_page_of_the_account() {
    let page = "https://github.com/apps/riff-acme/installations/new/permissions?target_id=10";
    let asks = Arc::new(Mutex::new(Vec::<Value>::new()));
    let a = asks.clone();
    let url = serve(axum::Router::new().route(
        "/v1/forge/install",
        post(move |Json(body): Json<Value>| async move {
            let mut asks = a.lock().unwrap();
            asks.push(body);
            // The person installs the App before the second ask.
            Json(json!({ "url": page, "installed": asks.len() > 1 }))
        }),
    ))
    .await;
    let env = Isolated::new();
    let dir = tempfile::tempdir().unwrap();
    let repo = clone();
    let mut cmd = env.riff();
    cmd.args(["forge", "install", "n8behavior"])
        .current_dir(repo.path())
        .env("RIFF_SERVER", &url)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env(forge::BROWSER_VAR, browser(dir.path()));
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{text} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(opened(dir.path()), [page], "riff opens the install page once");
    let asks = asks.lock().unwrap().clone();
    assert_eq!(asks.len(), 2);
    assert_eq!(asks[0]["owner"], "n8behavior");
    assert!(
        text.contains("The GitHub App of riff is installed on n8behavior."),
        "{text}"
    );
    assert!(
        text.contains("run riff forge check in a clone of a repository of n8behavior"),
        "{text}"
    );
}

/// 01M4CTAYZW24PDP2PDK72PT6J1: `riff forge create --org` opens the start
/// page of the server, waits for the App and its install, allows the
/// org, and runs `riff forge check` in a clone of the org.
#[tokio::test]
async fn riff_forge_create_opens_the_start_page_then_allows_and_checks() {
    let start = "http://riff.test/forge/new?state=s1";
    let seen = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
    let route = |path: &'static str, seen: Arc<Mutex<Vec<(String, Value)>>>| {
        post(move |Json(body): Json<Value>| async move {
            let mut seen = seen.lock().unwrap();
            seen.push((path.to_owned(), body));
            let polls = seen.iter().filter(|(p, _)| p == "created").count();
            Json(match path {
                "create" => json!({ "url": start, "state": "s1" }),
                // The App comes at the first ask, the install at the
                // second.
                "created" => json!({ "app": 7, "slug": "riff-acme", "installed": polls > 1 }),
                "allow" => json!({ "accounts": ["como-technologies"] }),
                _ => serde_json::to_value(ForgeCheckReply {
                    repo: "como-technologies/riff".into(),
                    app: 7,
                    roles: TokenRole::ALL
                        .into_iter()
                        .map(|role| RoleCheck {
                            role,
                            permissions: reply(role, 0).permissions,
                            error: None,
                        })
                        .collect(),
                })
                .unwrap(),
            })
        })
    };
    let url = serve(
        axum::Router::new()
            .route("/v1/forge/create", route("create", seen.clone()))
            .route("/v1/forge/created", route("created", seen.clone()))
            .route("/v1/forge/allow", route("allow", seen.clone()))
            .route("/v1/forge/check", route("check", seen.clone())),
    )
    .await;
    let env = Isolated::new();
    let dir = tempfile::tempdir().unwrap();
    let repo = clone();
    let mut cmd = env.riff();
    cmd.args(["forge", "create", "--org", "como-technologies"])
        .current_dir(repo.path())
        .env("RIFF_SERVER", &url)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env(forge::BROWSER_VAR, browser(dir.path()));
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{text} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(opened(dir.path()), [start]);
    let seen = seen.lock().unwrap().clone();
    let paths: Vec<&str> = seen.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["create", "created", "created", "allow", "check"]);
    assert_eq!(seen[0].1["org"], "como-technologies");
    assert_eq!(seen[1].1["state"], "s1");
    assert_eq!(seen[3].1["owner"], "como-technologies");
    assert_eq!(seen[3].1["allowed"], true);
    for line in [
        "riff-server has the new GitHub App 7 (riff-acme).",
        "The GitHub App of riff is installed on como-technologies.",
        "riff-server makes forge tokens for the repositories of: como-technologies",
        "riff-server checked the GitHub App 7 on como-technologies/riff",
        "verifier: actions read",
    ] {
        assert!(text.contains(line), "{line} is not in: {text}");
    }
}
