//! Get back into the riff (#174): a busy machine keeps its sign-in, a
//! running session goes on after `riff login`, and `riff login` works
//! with a server of another version.
//!
//! Each `riff` runs in one [`Isolated`] environment. Its secrets are
//! files in `RIFF_HOME`, and this test process keeps its own secrets in
//! the same files ([`SecretFiles`]), so that they share the sign-in.

#![cfg(debug_assertions)]

mod common;

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use common::{CLIENT, FakeProvider, browser};
use futures::future::BoxFuture;
use isolated::Isolated;
use keyring_core::api::{CredentialApi, CredentialPersistence, CredentialStoreApi};
use riff::api::Api;
use riff::login::{self, SignIn};
use riff::secrets;
use riff_core::name::SessionUri;
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::{DEFAULT_DOMAIN, Provider};
use riff_server::store::{Loaded, Memory, Store, StoreError, TOKENS, Version};

/// The environment of each `riff` of this test. The secrets of this
/// test process go to its secret files too.
fn env() -> &'static Isolated {
    static STORE: Once = Once::new();
    let env = Isolated::shared();
    STORE.call_once(|| {
        let dir = env.riff_home().join("secrets");
        keyring_core::set_default_store(Arc::new(SecretFiles(dir)));
    });
    env
}

/// A keyring for this test process: the secret files of `riff` in a
/// `RIFF_HOME` (see [`secrets::file_get`]).
#[derive(Debug)]
struct SecretFiles(PathBuf);

impl CredentialStoreApi for SecretFiles {
    fn vendor(&self) -> String {
        "riff secret files".into()
    }

    fn id(&self) -> String {
        self.0.display().to_string()
    }

    fn build(
        &self,
        _service: &str,
        name: &str,
        _modifiers: Option<&HashMap<&str, &str>>,
    ) -> keyring_core::Result<keyring_core::Entry> {
        Ok(keyring_core::Entry::new_with_credential(Arc::new(
            SecretFile {
                dir: self.0.clone(),
                name: name.into(),
            },
        )))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn persistence(&self) -> CredentialPersistence {
        CredentialPersistence::UntilDelete
    }
}

/// One secret of [`SecretFiles`].
#[derive(Debug)]
struct SecretFile {
    dir: PathBuf,
    name: String,
}

fn failure(e: anyhow::Error) -> keyring_core::Error {
    keyring_core::Error::PlatformFailure(e.into())
}

impl CredentialApi for SecretFile {
    fn set_secret(&self, secret: &[u8]) -> keyring_core::Result<()> {
        let value = std::str::from_utf8(secret).map_err(|e| failure(e.into()))?;
        secrets::file_set(&self.dir, &self.name, value).map_err(failure)
    }

    fn get_secret(&self) -> keyring_core::Result<Vec<u8>> {
        match secrets::file_get(&self.dir, &self.name).map_err(failure)? {
            Some(value) => Ok(value.into_bytes()),
            None => Err(keyring_core::Error::NoEntry),
        }
    }

    fn delete_credential(&self) -> keyring_core::Result<()> {
        secrets::file_delete(&self.dir, &self.name).map_err(failure)
    }

    fn get_credential(&self) -> keyring_core::Result<Option<Arc<keyring_core::api::Credential>>> {
        self.get_secret().map(|_| None)
    }

    fn get_specifiers(&self) -> Option<(String, String)> {
        Some((secrets::SERVICE.into(), self.name.clone()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A store whose save of the tokens fails each `every` times, as a
/// store of a busy server can.
struct Busy {
    store: Memory,
    every: usize,
    saves: AtomicUsize,
}

impl Store for Busy {
    fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
        self.store.load(name)
    }

    fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>> {
        self.store.list(prefix)
    }

    fn save<'a>(
        &'a self,
        name: &'a str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> BoxFuture<'a, Result<Version, StoreError>> {
        let n = self.saves.fetch_add(1, Ordering::SeqCst);
        if name == TOKENS && n.is_multiple_of(self.every) {
            return Box::pin(async { Err(StoreError::Failed("the store is busy".into())) });
        }
        self.store.save(name, bytes, known)
    }

    fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), StoreError>> {
        self.store.delete(name)
    }
}

/// A server with sign-in that needs a token for each call, and a fake
/// provider that signs in Ada. With `busy`, its store fails each third
/// save of the tokens.
async fn server(busy: bool) -> (Service, String) {
    server_as(busy, None).await
}

/// A server of another version: each reply names a version that no
/// `riff` of this build talks with.
async fn other_version() -> (Service, String) {
    let other = axum::http::HeaderValue::from_static("0.0.1 0000deadbeef 2000-01-01T00:00:00Z");
    server_as(false, Some(other)).await
}

/// The settings of a server at `url` with sign-in that needs a token for
/// each call, and a fake provider that signs in Ada.
async fn config(url: &str) -> Config {
    let issuer = FakeProvider::start("Ada@comotechnologies.io", Some(DEFAULT_DOMAIN))
        .await
        .issuer;
    let mut config = Config {
        require_sign_in: true,
        provider: Some(Provider {
            issuer,
            client_id: CLIENT.into(),
            client_secret: None,
            allowed_domains: vec![DEFAULT_DOMAIN.into()],
        }),
        ..Config::new(url)
    };
    config.lease.wait = Duration::from_millis(10);
    config
}

/// [`server`], and with `version`, each reply names that version.
async fn server_as(busy: bool, version: Option<axum::http::HeaderValue>) -> (Service, String) {
    env();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = config(&url).await;
    let service = if busy {
        let store = Busy {
            store: Memory::default(),
            every: 3,
            saves: AtomicUsize::new(1),
        };
        Service::load(config, Arc::new(store)).await.unwrap()
    } else {
        Service::new(config)
    };
    let mut router = service.router();
    if let Some(version) = version {
        router = router.layer(axum::middleware::map_response(
            move |mut r: axum::response::Response| {
                r.headers_mut()
                    .insert(riff_core::build::HEADER, version.clone());
                std::future::ready(r)
            },
        ));
    }
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// `riff` with `args`, as the session `session` of Ada, in `dir`.
fn riff(url: &str, dir: &Path, session: &str, args: &[&str]) -> Command {
    let mut cmd = env().riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", url)
        .env("RIFF_USER", "ada")
        .env("RIFF_HOST", "heron")
        .env("RIFF_SESSION", session)
        .env("XDG_RUNTIME_DIR", dir);
    cmd
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// The kept sign-in of `url`, with an expired access token.
fn expire(url: &str) {
    let kept = login::stored(url).unwrap().unwrap();
    login::store(
        url,
        &SignIn {
            expires_at: 0,
            ..kept
        },
    )
    .unwrap();
}

/// 10 `riff` processes of one machine refresh the sign-in at the same
/// time, 50 times, at a server whose store fails each third save. The
/// sign-in stays good (01M3MX4TG7PNNETZ986DQS10JJ, R107).
#[tokio::test(flavor = "multi_thread")]
async fn ten_processes_refresh_at_once_and_the_sign_in_stays() {
    let (_service, url) = server(true).await;
    let dir = tempfile::tempdir().unwrap();
    login::login(&Api::new(&url), browser).await.unwrap();
    for round in 0..50 {
        expire(&url);
        let children: Vec<_> = (0..10)
            .map(|_| {
                riff(&url, dir.path(), "m1", &["members"])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap()
            })
            .collect();
        let outputs: Vec<Output> = tokio::task::spawn_blocking(|| {
            children
                .into_iter()
                .map(|c| c.wait_with_output().unwrap())
                .collect()
        })
        .await
        .unwrap();
        for out in outputs {
            assert!(out.status.success(), "round {round}: {}", text(&out.stderr));
        }
    }
}

/// Waits up to `limit` until `test` is true.
async fn wait_for(limit: Duration, test: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if test() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    test()
}

/// A running `riff watch` says once that the sign-in ended, waits, and
/// goes on after `riff login`, with no restart
/// (01M3MX4VM8CK1GAGJAM2P29NWH).
#[tokio::test(flavor = "multi_thread")]
async fn a_watch_goes_on_after_riff_login() {
    let (service, url) = server(false).await;
    let api = Api::new(&url);
    login::login(&api, browser).await.unwrap();
    service.tokens().revoke_user("ada");

    let dir = tempfile::tempdir().unwrap();
    let (out, err) = (dir.path().join("stdout"), dir.path().join("stderr"));
    let watch = riff(&url, dir.path(), "w1", &["watch", "--once"])
        .stdout(std::fs::File::create(&out).unwrap())
        .stderr(std::fs::File::create(&err).unwrap())
        .spawn()
        .unwrap();
    let watch = std::sync::Mutex::new(watch);
    let read = || std::fs::read_to_string(&err).unwrap_or_default();
    assert!(
        wait_for(Duration::from_secs(10), || read().contains(login::ENDED)).await,
        "{}",
        read()
    );

    login::login(&api, browser).await.unwrap();
    let me: SessionUri = "riff://ada@heron/como-technologies/riff?session=p1"
        .parse()
        .unwrap();
    let person = api.clone().signed_in(Some("p1")).unwrap();
    person.register(&me).await.unwrap();
    // The watch connects again within its retry time; then the tell
    // finds its session and wakes it.
    let ended = || watch.lock().unwrap().try_wait().unwrap();
    let woke = wait_for(Duration::from_secs(20), || ended().is_some());
    let tell = async {
        while person.tell(&me, "w1", "back in").await.is_err() {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    };
    let (woke, ()) = tokio::join!(woke, tell);
    let status = ended();
    let _ = watch.lock().unwrap().kill();
    assert!(woke && status.is_some_and(|s| s.success()), "{}", read());
    assert!(!std::fs::read_to_string(&out).unwrap().is_empty());
    assert_eq!(read().matches(login::ENDED).count(), 1, "{}", read());
}

/// The tools of a running `riff mcp` say that the sign-in ended, and
/// work again after `riff login`, with no restart
/// (01M3MX4VM8CK1GAGJAM2P29NWH, 01M3MX4VCEBTY0DN4JMF624WYE).
#[tokio::test(flavor = "multi_thread")]
async fn the_tools_go_on_after_riff_login() {
    use riff::mcp::Tools;
    use rmcp::ServiceExt;
    use rmcp::model::CallToolRequestParams;

    let (service, url) = server(false).await;
    let api = Api::new(&url);
    login::login(&api, browser).await.unwrap();
    let me: SessionUri = "riff://ada@heron/como-technologies/riff?session=t1"
        .parse()
        .unwrap();
    let session = api.clone().signed_in(Some("t1")).unwrap();
    session.register(&me).await.unwrap();
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let tools = Tools::new(session, me);
    tokio::spawn(async move {
        tools
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = ().serve(client_io).await.unwrap();
    let who = || async {
        let result = client
            .call_tool(CallToolRequestParams::new("who"))
            .await
            .unwrap();
        let text: String = result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect();
        (text, result.is_error == Some(true))
    };
    let (text, failed) = who().await;
    assert!(!failed, "{text}");

    service.tokens().revoke_user("ada");
    let (text, failed) = who().await;
    assert!(failed && text.contains(login::ENDED), "{text}");

    login::login(&api, browser).await.unwrap();
    let (text, failed) = who().await;
    assert!(!failed, "{text}");
    assert!(text.contains("t1"), "{text}");
}

/// `riff login` succeeds against a server of another version. After it,
/// a call says that the versions do not match, and never that the
/// sign-in ended (01M3MX4V43SF2XFCZWANHD19WV, 01M3MX4TSEH18FSNQ28GEH2GFJ).
#[tokio::test(flavor = "multi_thread")]
async fn riff_login_works_with_another_version() {
    let (_service, url) = other_version().await;
    let api = Api::new(&url);
    let sign_in = login::login(&api, browser).await.unwrap();
    assert_eq!(sign_in.user, "ada");

    // A call that needs a refresh first.
    expire(&url);
    let me: SessionUri = "riff://ada@heron".parse().unwrap();
    let error = api
        .signed_in(None)
        .unwrap()
        .who(&me, false)
        .await
        .unwrap_err();
    assert!(
        error.downcast_ref::<riff_core::build::Mismatch>().is_some(),
        "{error:#}"
    );
    assert!(!format!("{error:#}").contains("sign-in ended"), "{error:#}");
}

/// A sign-in made before a restart of `riff-server` on the same store
/// works after it, with no `riff login` (R124, R31). So a deploy of a new
/// release keeps each machine in the riff, also a machine that updates
/// riff by itself (01M3N7JJNPPJNTXFDTEY4MSDVJ).
#[tokio::test(flavor = "multi_thread")]
async fn a_sign_in_stays_over_a_restart_of_riff_server() {
    env();
    let store = Memory::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let config = config(&url).await;
    let old = Service::load(config.clone(), Arc::new(store.clone()))
        .await
        .unwrap();
    let router = old.router();
    let serving = tokio::spawn(async move { axum::serve(listener, router).await });
    login::login(&Api::new(&url), browser).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let members = || {
        let mut cmd = riff(&url, dir.path(), "m1", &["members", "--color", "never"]);
        tokio::task::spawn_blocking(move || cmd.output().unwrap())
    };
    let before = members().await.unwrap();
    assert!(before.status.success(), "{}", text(&before.stderr));
    // The facts are aligned (01M3Q5V313XQN86BA2PBTXHEZC).
    let shown = text(&before.stdout);
    assert!(shown.starts_with("owner            "), "{shown}");
    assert!(shown.contains("\nallowed domains  "), "{shown}");
    assert!(!shown.contains('\x1b'), "{shown:?}");

    // The deploy: the old server stops, a new one loads the same store
    // at the same address.
    serving.abort();
    let _ = serving.await;
    drop(old);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let new = Service::load(config, Arc::new(store)).await.unwrap();
    let router = new.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    // A call with the old access token, then one that refreshes it.
    let kept = members().await.unwrap();
    assert!(kept.status.success(), "{}", text(&kept.stderr));
    expire(&url);
    let refreshed = members().await.unwrap();
    assert!(refreshed.status.success(), "{}", text(&refreshed.stderr));
    assert!(!text(&refreshed.stderr).contains("riff login"));
    assert_eq!(login::stored(&url).unwrap().unwrap().user, "ada");
}

/// "Get back into the riff" in the book names the real error texts and
/// the command that fixes them, and the command is in `riff --help`.
#[test]
fn the_book_says_how_to_get_back_in() {
    let book = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let start = book
        .find("\n### Get back into the riff\n")
        .expect("the how-to");
    let rest = &book[start + 1..];
    let part = &rest[..rest[4..].find("\n#").map_or(rest.len(), |i| i + 4)];
    let ended = format!(
        "{}: {}",
        login::ENDED,
        riff::api::TokenRefused {
            error: "invalid_grant".into(),
            description: None
        }
    );
    let new_riff = riff::text::new_riff("URL");
    let new_riff = new_riff.split(" (").next().unwrap();
    for text in [
        ended.as_str(),
        "no sign-in for URL: run riff login",
        new_riff,
        "do not match",
        "```sh\nriff login\n```",
    ] {
        assert!(part.contains(text), "{text:?} is not in the how-to");
    }
    let help = Isolated::new().riff().arg("--help").output().unwrap();
    assert!(text(&help.stdout).contains("  login "));
}
