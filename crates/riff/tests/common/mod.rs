//! A fake OpenID Connect provider and a fake browser, for the tests
//! that sign in end to end. Also the mock keyring, and a listener whose
//! URL has no old sign-in in it.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Once};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use riff::login;
use riff_core::wire::Discovery;
use serde_json::{Value, json};

/// The fake GitHub of the tests of riff-server: the tests of riff use it
/// too.
#[path = "../../../riff-server/tests/fake_github.rs"]
pub mod fake_github;

const KEY: &str = include_str!("../../../riff-server/testdata/test-only-rsa-key.pem");
const JWKS: &str = include_str!("../../../riff-server/testdata/test-only-jwks.json");

/// Makes the mock store of `keyring-core` the keyring of this test
/// process. All tests of the test binary share it.
pub fn mock_keyring() {
    static MOCK_KEYRING: Once = Once::new();
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
}

/// The URL of `listener`, with no sign-in for it in the mock keyring.
/// The keyring keeps a sign-in by the URL of its server. When a test
/// ends, its port is free, and the OS can give the port to a later
/// test. So a new server removes the sign-in that an earlier test left
/// at its URL.
pub fn fresh_url(listener: &tokio::net::TcpListener) -> String {
    mock_keyring();
    let url = format!("http://{}", listener.local_addr().unwrap());
    riff_client::login::logout(&url).unwrap();
    url
}

/// A listener on a free port, and its URL from [`fresh_url`].
pub async fn listen() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = fresh_url(&listener);
    (listener, url)
}

/// The client ID of riff at the fake provider.
pub const CLIENT: &str = "riff-client";

/// The refresh token of the account at the fake provider: the token of
/// the test account of a smoke test.
pub const REFRESH: &str = "test-refresh-token";

/// An account at the fake provider: its email, and its `hd` claim.
#[derive(Clone)]
struct Account {
    email: String,
    hd: Option<String>,
}

/// The state of the fake provider: the code challenge of each code
/// that it gave out, and the account that signs in.
#[derive(Clone)]
struct Fake {
    issuer: String,
    codes: Arc<Mutex<HashMap<String, String>>>,
    account: Arc<Mutex<Account>>,
}

/// A fake provider on a free port. It signs in one account at a time.
pub struct FakeProvider {
    /// The issuer URL of the provider.
    pub issuer: String,
    account: Arc<Mutex<Account>>,
}

impl FakeProvider {
    /// Starts a provider that signs in `email`, with the `hd` claim
    /// `hd`.
    pub async fn start(email: &str, hd: Option<&str>) -> FakeProvider {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let discovery = Discovery {
            issuer: issuer.clone(),
            authorization_endpoint: format!("{issuer}/authorize"),
            token_endpoint: format!("{issuer}/token"),
            jwks_uri: format!("{issuer}/jwks"),
        };
        let jwks: Value = serde_json::from_str(JWKS).unwrap();
        let account = Arc::new(Mutex::new(Account {
            email: email.into(),
            hd: hd.map(Into::into),
        }));
        let router = Router::new()
            .route(
                "/.well-known/openid-configuration",
                get(move || async move { Json(discovery) }),
            )
            .route("/jwks", get(move || async move { Json(jwks) }))
            .route("/authorize", get(authorize))
            .route("/token", post(provider_token))
            .with_state(Fake {
                issuer: issuer.clone(),
                codes: Arc::default(),
                account: account.clone(),
            });
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        FakeProvider { issuer, account }
    }

    /// From now on, the provider signs in `email`, with the `hd` claim
    /// `hd`.
    pub fn sign_in_as(&self, email: &str, hd: Option<&str>) {
        *self.account.lock().unwrap() = Account {
            email: email.into(),
            hd: hd.map(Into::into),
        };
    }
}

async fn authorize(State(fake): State<Fake>, Query(q): Query<HashMap<String, String>>) -> Redirect {
    assert_eq!(q["client_id"], CLIENT);
    assert_eq!(q["code_challenge_method"], "S256");
    let code = format!("code-{}", fake.codes.lock().unwrap().len());
    fake.codes
        .lock()
        .unwrap()
        .insert(code.clone(), q["code_challenge"].clone());
    let to = format!("{}?code={code}&state={}", q["redirect_uri"], q["state"]);
    Redirect::to(&to)
}

async fn provider_token(
    State(fake): State<Fake>,
    Form(f): Form<HashMap<String, String>>,
) -> impl IntoResponse {
    let granted = match f.get("grant_type").map(String::as_str) {
        Some("refresh_token") => f.get("refresh_token").map(String::as_str) == Some(REFRESH),
        _ => {
            let challenge = fake.codes.lock().unwrap().remove(&f["code"]);
            challenge == Some(login::challenge(&f["code_verifier"]))
        }
    };
    if !granted || f["client_id"] != CLIENT {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_grant"})),
        );
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test".into());
    let account = fake.account.lock().unwrap().clone();
    let mut claims = json!({
        "iss": fake.issuer,
        "aud": CLIENT,
        "exp": now + 3600,
        "email": account.email,
        "email_verified": true,
    });
    if let Some(hd) = account.hd {
        claims["hd"] = json!(hd);
    }
    let id_token = encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(KEY.as_bytes()).unwrap(),
    )
    .unwrap();
    (StatusCode::OK, Json(json!({ "id_token": id_token })))
}

/// A browser that goes to the URL and follows each redirect.
pub fn browser(url: &str) {
    let url = url.to_owned();
    tokio::spawn(async move { reqwest::get(url).await.unwrap().error_for_status().unwrap() });
}

/// The person of `me` makes the session `me` the lead, as `riff lead
/// --session` does (01M5NJAXTQ0YKN7Q7XN0A7ZMPF). No session becomes the
/// lead by itself (01M5NJAY3FTR005TDJJY1TMGNX). `api` must have no
/// session token.
pub async fn name_lead(api: &riff_client::api::Api, me: &riff_core::name::SessionUri) {
    let person = person_of(me);
    let id = me.who().session().expect("a session");
    api.lead(&person, id).await.unwrap();
}

/// The person `user` names the session `id` the lead of the repository of
/// the clone `dir` at the riff-server `url`, as the riff daemon does for a
/// lead that it starts (01M77Y6ZMFZJGK2DM6RAM94C7Q): riff has no command
/// for it. The place is on the host `host`. At a [`Signed`] server, the
/// call has the sign-in of `user`. It gives the reply of riff-server, and
/// panics with the error.
pub async fn lead_by_person(
    url: &str,
    dir: &std::path::Path,
    host: &str,
    user: &str,
    id: &str,
) -> riff_core::wire::LeadReply {
    let place = riff::identity::place_in(dir, host).unwrap();
    let person =
        riff_core::name::SessionUri::new(riff_core::name::Who::new(user, None).unwrap(), place);
    let signed = sign_ins().lock().unwrap().contains_key(url);
    let api = match signed {
        true => api(url, user, None),
        false => riff_client::api::Api::new(url),
    };
    api.lead(&person, id)
        .await
        .unwrap_or_else(|e| panic!("the lead {id} of {user}: {e:#}"))
}

/// Signs this process in as `email`, the user `user`, at the riff-server
/// `service` that a test made at `url`, as [`start_signed`] does for its
/// people: then [`api`] and [`lead_by_person`] work at that server.
/// `allowed` is the allowed domain of [`riff_server::Service::admit`].
pub async fn admit_process(
    service: &riff_server::Service,
    url: &str,
    user: &str,
    email: &str,
    allowed: bool,
) {
    let key = riff_client::device::key(url).unwrap();
    let pair = service
        .admit(email, allowed, &key.thumbprint())
        .await
        .unwrap();
    let sign_in = riff_client::login::SignIn {
        user: pair.user,
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_at: u64::MAX,
        riff_id: service.riff_id(),
    };
    let mut all = sign_ins().lock().unwrap();
    let (_, _, people) = all
        .entry(url.to_owned())
        .or_insert_with(|| (u64::MAX, key.to_secret(), HashMap::new()));
    people.insert(user.to_owned(), serde_json::to_string(&sign_in).unwrap());
}

/// The person of the session `me`, at the place of `me`.
pub fn person_of(me: &riff_core::name::SessionUri) -> riff_core::name::SessionUri {
    let who = riff_core::name::Who::new(me.who().user(), None).unwrap();
    riff_core::name::SessionUri::new(who, me.place().clone())
}

/// The `server` calls that a fake daemon got on a session socket
/// (`ServerCall`), each as JSON with its `call` name.
pub type Asked = Arc<Mutex<Vec<Value>>>;

/// A socket that answers as the riff daemon does for a session in a
/// container (#1338). For each `server` ask, `reply` gives the JSON
/// answer for the call, or the error text of a refusal. The socket logs
/// each call. The directory holds the socket: keep it until the end of
/// the test.
pub fn session_socket(
    reply: impl Fn(&Value) -> Result<Value, String> + Send + 'static,
) -> (tempfile::TempDir, String, Asked) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("riff.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let asked = Asked::default();
    let log = asked.clone();
    std::thread::spawn(move || {
        use std::io::{BufRead, Write};
        for conn in listener.incoming().flatten() {
            let mut line = String::new();
            let _ = std::io::BufReader::new(&conn).read_line(&mut line);
            let ask: Value = serde_json::from_str(&line).unwrap();
            // The count of a call of `gh` (#1582): the daemon takes it, and
            // the log holds the calls on riff-server only.
            if ask["ask"] == "gh_calls" {
                continue;
            }
            assert_eq!(ask["ask"], "server", "{ask}");
            log.lock().unwrap().push(ask["call"].clone());
            let say = match reply(&ask["call"]) {
                Ok(reply) => json!({"say": "served", "reply": reply}),
                Err(error) => json!({"say": "failed", "error": error}),
            };
            let _ = (&conn).write_all(format!("{say}\n").as_bytes());
        }
    });
    (dir, path.to_string_lossy().into_owned(), asked)
}

/// The names of the calls that `asked` holds, in order.
pub fn call_names(asked: &Asked) -> Vec<String> {
    let asked = asked.lock().unwrap();
    asked
        .iter()
        .map(|c| c["call"].as_str().unwrap().to_owned())
        .collect()
}

/// The ID of a [`Signed`] server, the device key, and the sign-in JSON of
/// each user, of that server. [`sign_in`] writes them into the secret
/// files of a `RIFF_HOME`. The map keeps them by URL, only while the
/// server lives: its [`Drop`] removes them (#1474).
type SignIns = (u64, String, HashMap<String, String>);

fn sign_ins() -> &'static Mutex<HashMap<String, SignIns>> {
    static SIGN_INS: std::sync::OnceLock<Mutex<HashMap<String, SignIns>>> =
        std::sync::OnceLock::new();
    SIGN_INS.get_or_init(Mutex::default)
}

/// The paths of the calls that a [`Signed`] server got.
pub type Calls = Arc<Mutex<Vec<String>>>;

/// A real riff-server that needs a sign-in (`require_sign_in: true`),
/// for the integration tests of each command that calls it
/// (01M5P2CWS1ENC30XNENRX11MY3). A command that forgets
/// [`riff_client::api::Api::signed_in`] sends no token, and the server
/// refuses it with 401. The server notes each such call, and the test
/// fails when it ends, even when the command hides the error.
///
/// The server derefs to its URL. Call [`sign_in`] with the URL and the
/// `RIFF_HOME` of the command, so that the command is signed in as the
/// person brett.
pub struct Signed {
    /// The service of the server.
    pub service: riff_server::Service,
    url: String,
    /// The ID of this server in [`sign_ins`]: a later server at the same
    /// URL has another one.
    id: u64,
    calls: Calls,
    refused: Calls,
}

impl Signed {
    /// Each path that the server got, in order.
    pub fn calls(&self) -> Calls {
        self.calls.clone()
    }

    /// The paths of the calls that came with no token, and got 401.
    pub fn unsigned(&self) -> Vec<String> {
        self.refused.lock().unwrap().clone()
    }

    /// The paths of [`Signed::unsigned`], and forgets them: the test
    /// means to send a call with no token.
    pub fn take_unsigned(&self) -> Vec<String> {
        std::mem::take(&mut self.refused.lock().unwrap())
    }

    /// [`api`] for this server.
    pub fn api(&self, user: &str, session: Option<&str>) -> riff_client::api::Api {
        api(&self.url, user, session)
    }
}

impl std::ops::Deref for Signed {
    type Target = str;
    fn deref(&self) -> &str {
        &self.url
    }
}

impl std::fmt::Display for Signed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.url)
    }
}

impl Drop for Signed {
    /// Removes the sign-ins of this server. The OS gives its port to a
    /// later test, and a machine at an open riff-server on that port must
    /// not get the sign-in of this one (01M5ST2QHBG5S6F2H0FPJ5A24P). A
    /// later server at the same URL keeps its own.
    fn drop(&mut self) {
        let mut sign_ins = sign_ins().lock().unwrap_or_else(|e| e.into_inner());
        if sign_ins
            .get(&self.url)
            .is_some_and(|(id, ..)| *id == self.id)
        {
            sign_ins.remove(&self.url);
        }
        drop(sign_ins);
        if !std::thread::panicking() {
            let refused = self.unsigned();
            assert!(
                refused.is_empty(),
                "a command sent these calls with no token: {refused:?}"
            );
        }
    }
}

/// The people that a [`Signed`] server signs in: the person of a command (the
/// owner), the user of an agent session (an admin), and a member.
pub const PEOPLE: &[&str] = &["brett", "mike", "bob"];

/// Starts a [`Signed`] server on a free port.
pub async fn start_signed() -> Signed {
    start_signed_with(|router| router).await
}

/// As [`start_signed`], with `wrap` applied to the router of the server:
/// a layer that the test needs, for example a header on each reply.
pub async fn start_signed_with(wrap: impl FnOnce(axum::Router) -> axum::Router) -> Signed {
    let (listener, url) = listen().await;
    start_signed_at(listener, &url, None, wrap).await
}

/// The repository that [`start_signed_project`] enrolls.
pub const PROJECT: &str = "como-technologies/riff";

/// The token of `gh` of brett: on the fake GitHub of
/// [`start_signed_project`], brett is an admin of [`PROJECT`].
const BRETT_GH: &str = "gho_brett_admin_of_riff";

/// As [`start_signed`], with [`PROJECT`] a project of the riff, as in a
/// riff of this build: riff-server takes the fact of a release only for a
/// project (01M77F60MR8WZTMFJ5CF2295Q6). Its GitHub is a fake where
/// brett, an admin of the riff, is an admin of the repository, and brett
/// enrolls it. The [`Signed::calls`] start after the enrollment.
pub async fn start_signed_project() -> Signed {
    let github = fake_github::FakeGitHub::start().await;
    github.seen().person(BRETT_GH, PROJECT, true);
    let settings = riff_server::github::Settings {
        api: github.url.clone(),
        web: github.url.clone(),
        ..riff_server::github::Settings::default()
    };
    let (listener, url) = listen().await;
    let server = start_signed_config(
        listener,
        &url,
        None,
        |router| router,
        |config| {
            config.github = settings;
        },
    )
    .await;
    let brett: riff_core::name::SessionUri = "riff://brett@thelio".parse().unwrap();
    let repo = PROJECT.parse().unwrap();
    let api = server.api("brett", None);
    api.project_init(&brett, &repo, &[], BRETT_GH)
        .await
        .unwrap();
    server.calls.lock().unwrap().clear();
    server
}

/// The change that enrolls `repo` as a project of the riff, with no
/// host (01M76SG2E9RPV4B7CYSZPGGZ3P), for [`start_signed_log`].
pub fn project(repo: &str) -> riff_core::record::Change {
    riff_core::record::Change::ProjectEnrolled(riff_core::record::ProjectEnrolled {
        repo: repo.parse().unwrap(),
        hosts: Vec::new(),
    })
}

/// As [`start_signed`], for a server that loads a log of `changes`, the
/// first at position 1: for example the [`project`] of each project of
/// the riff, or the notes of a lead with no session.
pub async fn start_signed_log(changes: Vec<riff_core::record::Change>) -> Signed {
    let loaded = log_of(changes).await;
    let (listener, url) = listen().await;
    start_signed_at(listener, &url, Some(loaded), |router| router).await
}

/// The router of a riff-server with no sign-in at `url` that loads a log
/// of `changes`, as [`start_signed_log`].
pub async fn router_of_log(url: &str, changes: Vec<riff_core::record::Change>) -> axum::Router {
    let (store, lease) = log_of(changes).await;
    let config = riff_server::auth::Config {
        lease,
        ..riff_server::auth::Config::new(url)
    };
    riff_server::Service::load(config, store)
        .await
        .unwrap()
        .router()
}

/// A store with a log of `changes`, the first at position 1, and a short
/// lease, for a server to load.
async fn log_of(changes: Vec<riff_core::record::Change>) -> Loaded {
    let store = riff_state::store::Memory::default();
    let records: Vec<riff_core::record::Record> = changes
        .into_iter()
        .zip(1..)
        .map(|(change, position)| riff_core::record::Record {
            envelope: riff_core::record::Envelope {
                position,
                written_at_ms: 1_790_812_800_000 + position * 1000,
                by: None,
                command: None,
                call: None,
            },
            change,
        })
        .collect();
    riff_state::log::write(&store, &records, &Default::default(), || true)
        .await
        .unwrap();
    let lease = riff_server::lease::Timing {
        wait: std::time::Duration::from_millis(50),
        read_every: std::time::Duration::from_millis(50),
        valid_for: std::time::Duration::from_millis(500),
        exit_after: std::time::Duration::from_secs(1),
        ..Default::default()
    };
    (Arc::new(store) as Arc<dyn riff_state::store::Store>, lease)
}

/// The store that a server loads, and the timing of its lease.
pub type Loaded = (
    Arc<dyn riff_state::store::Store>,
    riff_server::lease::Timing,
);

/// As [`start_signed`], for a server that listens on `listener` and whose
/// public address is `url`, for example a proxy in front of it. With
/// `loaded`, the server loads that store.
pub async fn start_signed_at(
    listener: tokio::net::TcpListener,
    url: &str,
    loaded: Option<Loaded>,
    wrap: impl FnOnce(axum::Router) -> axum::Router,
) -> Signed {
    start_signed_config(listener, url, loaded, wrap, |_| {}).await
}

/// As [`start_signed_at`], with the settings of the server changed by
/// `edit`: for example the GitHub of a test.
pub async fn start_signed_config(
    listener: tokio::net::TcpListener,
    url: &str,
    loaded: Option<Loaded>,
    wrap: impl FnOnce(axum::Router) -> axum::Router,
    edit: impl FnOnce(&mut riff_server::auth::Config),
) -> Signed {
    let url = url.to_owned();
    let mut config = riff_server::auth::Config {
        require_sign_in: true,
        admins: PEOPLE[..2]
            .iter()
            .map(|user| format!("{user}@comotechnologies.io"))
            .collect(),
        ..riff_server::auth::Config::new(&url)
    };
    edit(&mut config);
    let service = match loaded {
        Some((store, lease)) => {
            config.lease = lease;
            riff_server::Service::load(config, store).await.unwrap()
        }
        None => riff_server::Service::new(config),
    };
    let calls = Calls::default();
    let refused = Calls::default();
    let (seen, noted) = (calls.clone(), refused.clone());
    let router = wrap(service.router()).layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let (seen, noted) = (seen.clone(), noted.clone());
            async move {
                let path = request.uri().path().to_owned();
                let bare = !request.headers().contains_key("authorization");
                seen.lock().unwrap().push(path.clone());
                let response = next.run(request).await;
                if bare && response.status() == StatusCode::UNAUTHORIZED {
                    noted.lock().unwrap().push(path);
                }
                response
            }
        },
    ));
    tokio::spawn(async move {
        let service = router.into_make_service_with_connect_info::<std::net::SocketAddr>();
        axum::serve(listener, service).await.unwrap()
    });
    let key = riff_client::device::key(&url).unwrap();
    let mut people = HashMap::new();
    for user in PEOPLE {
        let email = format!("{user}@comotechnologies.io");
        let pair = service
            .admit(&email, true, &key.thumbprint())
            .await
            .unwrap();
        let sign_in = riff_client::login::SignIn {
            user: pair.user,
            access_token: pair.access_token,
            refresh_token: pair.refresh_token,
            expires_at: u64::MAX,
            riff_id: service.riff_id(),
        };
        people.insert((*user).to_owned(), serde_json::to_string(&sign_in).unwrap());
    }
    static IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    sign_ins()
        .lock()
        .unwrap()
        .insert(url.clone(), (id, key.to_secret(), people));
    Signed {
        service,
        url,
        id,
        calls,
        refused,
    }
}

/// Signs in the commands with the `RIFF_HOME` `home` to the [`Signed`]
/// server at `url`, as the person `user` ([`PEOPLE`]). It does nothing for another
/// server.
pub fn sign_in(url: &str, home: &std::path::Path, user: &str) {
    let Some((_, key, people)) = sign_ins().lock().unwrap().get(url).cloned() else {
        return;
    };
    let sign_in = people
        .get(user)
        .unwrap_or_else(|| panic!("no sign-in for {user}"));
    let secrets = home.join("secrets");
    riff_client::secrets::file_set(&secrets, &riff_client::device::secret_name(url), &key).unwrap();
    riff_client::secrets::file_set(&secrets, &riff_client::login::secret_name(url), sign_in)
        .unwrap();
}

/// The `PATH` of the tests with the folder `bin` first: a fake program
/// in `bin` runs in the place of the real one.
pub fn path_with(bin: &std::path::Path) -> String {
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}

/// A fake `gh` for a clone of como-technologies/riff: the open issues
/// #1862, #1860 and #1700 in the REST list, with the time each opened.
/// It logs each call to the file `calls` of its folder.
pub const ISSUES_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
case "$*" in
  *'issues?state=open'*)
    printf 'HTTP/2.0 200 OK\r\nEtag: "v1"\r\n\r\n'
    printf '[{"number":1862,"title":"Projects A: riff-server keeps the projects of the riff","created_at":"2026-11-10T09:00:00Z"},'
    printf '{"number":1860,"title":"Pick an issue with #","created_at":"2026-11-09T09:00:00Z"},'
    printf '{"number":1700,"title":"Show the board","created_at":"2026-10-01T09:00:00Z"}]' ;;
  *) echo '[]' ;;
esac
"#;

/// A clone of `repo`, `OWNER/REPO`, on GitHub, with no commit: a git
/// folder with the remote `origin`.
pub fn clone_of(repo: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let origin = format!("https://github.com/{repo}.git");
    for args in [&["init", "-q"][..], &["remote", "add", "origin", &origin]] {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status();
        assert!(status.unwrap().success());
    }
    dir
}

/// A client of this process for the [`Signed`] server at `url`: it
/// sends the token of `user`, or of the session `session` of `user`. It
/// replaces the sign-in of this process at the server, so build the
/// client of the next user after the use of this one.
pub fn api(url: &str, user: &str, session: Option<&str>) -> riff_client::api::Api {
    let people = sign_ins().lock().unwrap().get(url).cloned();
    let (_, _, people) = people.unwrap_or_else(|| panic!("{url} is not a signed server"));
    let sign_in = people
        .get(user)
        .unwrap_or_else(|| panic!("no sign-in for {user}"));
    riff_client::login::store(url, &serde_json::from_str(sign_in).unwrap()).unwrap();
    riff_client::api::Api::new(url).signed_in(session).unwrap()
}

/// [`api`] for the session `me` of the [`Signed`] server at `url`.
pub fn api_of(url: &str, me: &riff_core::name::SessionUri) -> riff_client::api::Api {
    api(url, me.who().user(), me.who().session())
}

/// A fake registry for a session image `HOST/o/i@sha256:…` (host
/// `127.0.0.1:PORT`): each request with no token gets a 401 that names
/// its own token URL; a request with a token gets the status `manifest`;
/// a token request gets the status `token`. It gives the host, and the
/// number of requests that it got.
pub async fn registry(manifest: u16, token: u16) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    use axum::http::{HeaderMap, header};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let seen = Arc::new(AtomicUsize::new(0));
    let (a, b) = (seen.clone(), seen.clone());
    let app = Router::new()
        .route(
            "/v2/o/i/manifests/{digest}",
            get(move |headers: HeaderMap| async move {
                a.fetch_add(1, Ordering::SeqCst);
                let mut reply = HeaderMap::new();
                if headers.get(header::AUTHORIZATION).is_some() {
                    return (StatusCode::from_u16(manifest).unwrap(), reply);
                }
                let host = headers[header::HOST].to_str().unwrap();
                let challenge = format!(r#"Bearer realm="http://{host}/token",service="reg""#);
                reply.insert(header::WWW_AUTHENTICATE, challenge.parse().unwrap());
                (StatusCode::UNAUTHORIZED, reply)
            }),
        )
        .route(
            "/token",
            get(move || async move {
                b.fetch_add(1, Ordering::SeqCst);
                (StatusCode::from_u16(token).unwrap(), r#"{"token":"anon"}"#)
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (host, seen)
}
