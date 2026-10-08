//! The secrets of a session come in its environment, not from the
//! keyring of the person (#611): `riff workers run` makes a session
//! grant from the sign-in in the keyring, and a session program with no
//! keyring and no D-Bus calls riff and the forge with only its
//! environment (01M4CVXJ7ZDAVRKJ8Y59R3KPDV, 01M4CVXJ3GCEB7B4632J7DD84A).
//! No secret of the session goes into an output, a file or a commit
//! (01M4CVXJA9WAN5M1RKNGETS8AY). The session gets the Claude plan token
//! of its person and no API key (01M4CVXJCGRC59VVZEHBFA9MPG,
//! 01M4C4WW8JS7QVC0ZYHWPSMWKN).

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use isolated::Isolated;
use riff::forge::TokenRole;
use riff::grant::{CLAUDE_TOKEN_SECRET, GRANT_VAR, KEY_VAR};
use riff::login::SignIn;
use riff::secrets::file_set;
use riff_core::dpop::Key;
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::store::Memory;

use crate::forge::{clone, reply};

const MARKER: &str = "person_marker";
const CLAUDE_TOKEN: &str = "sk-ant-oat01-plan-of-mike";
const SESSION: &str = "w-grant";

/// A riff-server that needs sign-in, and its URL. Its forge route gives
/// a worker token: the test answers in place of the GitHub App.
async fn signed_server() -> (Service, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        require_sign_in: true,
        lease: riff_server::lease::Timing {
            wait: Duration::from_millis(50),
            read_every: Duration::from_millis(50),
            valid_for: Duration::from_millis(500),
            exit_after: Duration::from_secs(1),
            ..Default::default()
        },
        ..Config::new(&url)
    };
    let service = Service::load(config, Arc::new(Memory::default()))
        .await
        .unwrap();
    let token = reply(TokenRole::Worker, 1);
    let router = axum::Router::new()
        .route(
            "/v1/forge/token",
            axum::routing::post(move || async move { axum::Json(token) }),
        )
        .layer(axum::middleware::map_response(
            |mut r: axum::response::Response| async move {
                let build = riff_core::build::VERSION.parse().unwrap();
                r.headers_mut().insert(riff_core::build::HEADER, build);
                r
            },
        ))
        .fallback_service(service.router());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// Keeps the sign-in of mike at `url`, its device key and the Claude
/// plan token in the secret files of `riff_home`: the keyring of the
/// person in a test.
async fn sign_in(service: &Service, url: &str, riff_home: &Path) {
    let secrets = riff_home.join("secrets");
    let device = Key::generate();
    file_set(&secrets, &riff::device::secret_name(url), &device.to_secret()).unwrap();
    let pair = service
        .admit("mike@comotechnologies.io", false, &device.thumbprint())
        .await
        .unwrap();
    let sign_in = SignIn {
        user: pair.user,
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_at: 0,
        riff_id: None,
    };
    let json = serde_json::to_string(&sign_in).unwrap();
    file_set(&secrets, &riff::login::secret_name(url), &json).unwrap();
    file_set(&secrets, CLAUDE_TOKEN_SECRET, CLAUDE_TOKEN).unwrap();
}

/// A fake `claude`: it moves the keyring away, then calls riff and the
/// forge with only its environment, and writes what it saw to `out`.
fn fake_claude(root: &Path, out: &Path, riff_home: &Path) -> std::path::PathBuf {
    let claude = root.join("claude");
    let riff = Isolated::shared().riff_path();
    let (o, r, h) = (out.display(), riff.display(), riff_home.display());
    std::fs::write(
        &claude,
        format!(
            "#!/bin/sh\n\
             mv '{h}/secrets' '{h}/secrets.away'\n\
             env > '{o}/env'\n\
             '{r}' whoami > '{o}/whoami' 2>&1\n\
             '{r}' post --kind note --to user=mike hello from the session > '{o}/post' 2>&1\n\
             '{r}' read --all > '{o}/read' 2>&1\n\
             '{r}' logout > '{o}/logout' 2>&1\n\
             printf 'protocol=https\\nhost=github.com\\n\\n' | \
             GIT_TERMINAL_PROMPT=0 git credential fill > '{o}/git' 2>&1\n\
             cat \"$GH_CONFIG_DIR/hosts.yml\" > '{o}/gh' 2>&1\n\
             mv '{h}/secrets.away' '{h}/secrets'\n"
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    claude
}

/// Each file under `dir` that holds `secret`, but the files of `skip`.
fn files_with(dir: &Path, secret: &str, skip: &[&Path]) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if skip.iter().any(|s| path.starts_with(s)) {
                continue;
            }
            // No link and no FIFO: the pool of build jobs is a FIFO.
            let Ok(kind) = e.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file()
                && std::fs::read(&path)
                .is_ok_and(|b| String::from_utf8_lossy(&b).contains(secret))
            {
                found.push(path.display().to_string());
            }
        }
    }
    found
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_works_with_only_its_environment_and_leaks_no_secret() {
    let (service, url) = signed_server().await;
    let dir = clone();
    let root = dir.path();
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let riff_home = root.join("riff-home");
    std::fs::create_dir_all(&riff_home).unwrap();
    sign_in(&service, &url, &riff_home).await;
    let out = root.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let claude = fake_claude(root, &out, &riff_home);

    let run = Isolated::shared()
        .riff()
        .current_dir(root)
        .env("RIFF_SERVER", &url)
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", &riff_home)
        .env("RIFF_SESSION", SESSION)
        .env("HOME", &home)
        .env("TMUX_PANE", "%5")
        .env("GH_TOKEN", MARKER)
        .env("ANTHROPIC_API_KEY", MARKER)
        .env("RIFF_TEST_MARKER", MARKER)
        .env("DBUS_SESSION_BUS_ADDRESS", format!("unix:path=/{MARKER}"))
        .env_remove("RIFF_USER")
        .env_remove("RIFF_WORKER")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .args(["workers", "run"])
        .arg(&claude)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    let seen = |f: &str| std::fs::read_to_string(out.join(f)).unwrap_or_default();
    let env = seen("env");
    assert!(!env.is_empty(), "claude did not run: {stderr}");
    let var = |name: &str| {
        env.lines()
            .find_map(|l| l.strip_prefix(&format!("{name}=")))
            .map(str::to_owned)
    };

    // The session got its secrets, and nothing of the person.
    let grant = var(GRANT_VAR).unwrap_or_else(|| panic!("no grant: {env}\n{stderr}"));
    let session_key = var(KEY_VAR).expect("a session key");
    assert_eq!(var("RIFF_USER").as_deref(), Some("mike"), "{env}");
    assert_eq!(var("CLAUDE_CODE_OAUTH_TOKEN").as_deref(), Some(CLAUDE_TOKEN));
    assert!(!env.contains(MARKER), "{env}");
    assert!(!env.contains("ANTHROPIC_"), "{env}");
    assert!(!env.contains("DBUS_SESSION_BUS_ADDRESS"), "{env}");

    // riff works with no keyring: the session posts, and reads its post
    // as verified, signed with the session key.
    let whoami = seen("whoami");
    assert!(whoami.contains(&format!("session={SESSION}")), "{whoami}");
    let post = seen("post");
    assert!(post.contains("Posted"), "{post}");
    let read = seen("read");
    assert!(read.contains("hello from the session"), "{read}");
    assert!(read.contains("(verified)"), "{read}");
    // A keyring call of the session is an error.
    assert!(seen("logout").contains(riff::grant::NO_KEYRING), "{}", seen("logout"));

    // The forge works with the token files of the session.
    let git = seen("git");
    assert!(git.contains("password=ghs_test_"), "{git}");
    assert!(seen("gh").contains("oauth_token: ghs_test_"), "{}", seen("gh"));

    // No secret of the session in an output, a file or a commit.
    let outputs = [
        stderr.as_str(),
        &whoami,
        &post,
        &read,
        &seen("logout"),
        &String::from_utf8_lossy(&run.stdout),
    ]
    .concat();
    for secret in [grant.as_str(), session_key.as_str(), CLAUDE_TOKEN] {
        assert!(!outputs.contains(secret), "{outputs}");
        let secrets = riff_home.join("secrets");
        let found = files_with(root, secret, &[&out, &secrets]);
        assert!(found.is_empty(), "{found:?}");
    }
    let log = Command::new("git")
        .args(["log", "--all", "-p"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&log.stdout).contains(&grant));
}
