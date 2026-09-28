//! `riff` and a `riff-server` of another build (01M3JEE7P46GWXR1BD4Q1TTSGN
//! to 01M3JEE7WT04BKX377VW5GDSPY, 01M3MNVT7G701SDP1Z1THMRDQ2 to
//! 01M3MNVTC248YYJJQKFD9H1WY9). A fake server names another wire
//! version, or no build. A real server with another build in its reply
//! has the same wire version.

use std::convert::Infallible;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use axum::http::HeaderValue;
use axum::response::Response;
use axum::response::sse::{Event, Sse};
use axum::routing::get;
use futures::Stream;
use riff_core::build::{Build, HEADER, VERSION, WIRE};
use riff_core::wire::{Kind, Message, Tailed};

/// A build with another commit and another wire version, at `time`.
fn other(time: &str) -> Build {
    Build {
        commit: "0000deadbeef".into(),
        time: time.into(),
        wire: WIRE + 1,
        ..Build::this()
    }
}

/// A build with another commit and the same wire version.
fn same_wire() -> Build {
    Build {
        commit: "0000deadbeef".into(),
        time: "2000-01-01T00:00:00Z".into(),
        ..Build::this()
    }
}

/// A layer that names `build` in each reply, or no build.
fn stamp(
    build: Option<Build>,
) -> impl Fn(Response) -> std::future::Ready<Response> + Clone + Send + Sync + 'static {
    move |mut r: Response| {
        if let Some(b) = &build {
            let value = HeaderValue::from_str(&b.to_string()).unwrap();
            r.headers_mut().insert(HEADER, value);
        }
        std::future::ready(r)
    }
}

/// A fake server that answers each call with 200 and names `build`, or
/// no build.
async fn fake(build: Option<Build>) -> String {
    let router = axum::Router::new()
        .fallback(|| async { "{}" })
        .layer(axum::middleware::map_response(stamp(build)));
    serve(router).await
}

async fn serve(router: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

/// A real riff-server that names `build` in its replies.
async fn real(build: Build) -> String {
    let router = riff_server::router().layer(axum::middleware::map_response(stamp(Some(build))));
    serve(router).await
}

/// A stream with one message, then no end.
fn one_message() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let message = Message {
        seq: 1,
        from: "riff://mike@pangolin/como-technologies/riff?session=a1"
            .parse()
            .unwrap(),
        to: vec![],
        body: "back again".into(),
        at_ms: 0,
        kind: Kind::Message,
        sig: None,
    };
    let event = Event::default()
        .json_data(Tailed {
            thread: "como-technologies/riff".parse().unwrap(),
            message,
            keys: Default::default(),
            trusted: true,
        })
        .unwrap();
    Sse::new(futures::StreamExt::chain(
        futures::stream::iter([Ok(event)]),
        futures::stream::pending(),
    ))
}

/// A fake server with a tail stream and a watch stream. While `other`
/// is true, it names another wire version.
async fn streams(other_wire: Arc<AtomicBool>) -> String {
    let router = axum::Router::new()
        .route("/v1/tail", get(|| async { one_message() }))
        .route(
            "/v1/watch",
            get(|| async { Sse::new(futures::stream::pending::<Result<Event, Infallible>>()) }),
        )
        .layer(axum::middleware::map_response(move |r: Response| {
            let build = if other_wire.load(Ordering::SeqCst) {
                other("2999-01-01T00:00:00Z")
            } else {
                Build::this()
            };
            stamp(Some(build))(r)
        }));
    serve(router).await
}

fn riff_at(binary: &Path, server: &str, dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(binary);
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "brett")
        .env("RIFF_HOST", "heron")
        .env("RIFF_SESSION", "b2")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("XDG_RUNTIME_DIR", dir);
    cmd
}

fn riff(server: &str, dir: &Path, args: &[&str]) -> Command {
    riff_at(&assert_cmd::cargo::cargo_bin("riff"), server, dir, args)
}

/// Runs `cmd` away from the runtime of the fake server.
async fn run(mut cmd: Command) -> Output {
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// Starts `cmd` with stdout and stderr in files of `dir`.
fn spawn(mut cmd: Command, dir: &Path) -> (Child, PathBuf, PathBuf) {
    let (out, err) = (dir.join("stdout"), dir.join("stderr"));
    let child = cmd
        .stdout(std::fs::File::create(&out).unwrap())
        .stderr(std::fs::File::create(&err).unwrap())
        .spawn()
        .unwrap();
    (child, out, err)
}

fn read(path: &Path) -> String {
    let mut s = String::new();
    if let Ok(mut f) = std::fs::File::open(path) {
        f.read_to_string(&mut s).unwrap();
    }
    s
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

/// `join`, `post` and `read` each fail, and name both builds and the
/// side to update (01M3JEE7RDTDD3KQMKH41E8D57).
async fn each_call_fails(server: Option<Build>, step: &str) {
    let url = fake(server.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let theirs = server.map_or("a build from before the wire version".into(), |b| {
        b.to_string()
    });
    for args in [
        &["read"][..],
        &["post", "--thread", "t", "hi"],
        &["read", "--thread", "t"],
    ] {
        let out = run(riff(&url, dir.path(), args)).await;
        let err = text(&out.stderr);
        assert!(!out.status.success(), "{args:?}: {err}");
        assert!(err.contains(VERSION), "{args:?}: {err}");
        assert!(err.contains(&theirs), "{args:?}: {err}");
        assert!(err.contains(step), "{args:?}: {err}");
        assert!(err.contains("do not match"), "{args:?}: {err}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_newer_riff_is_refused_and_names_the_server_to_update() {
    each_call_fails(Some(other("2000-01-01T00:00:00Z")), "Update riff-server").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_older_riff_is_refused_and_names_riff_to_update() {
    each_call_fails(
        Some(other("2999-01-01T00:00:00Z")),
        "Update riff on this machine",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_with_no_build_is_refused() {
    each_call_fails(None, "Update riff-server").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_build_works_and_who_and_whoami_show_it() {
    let url = real(Build::this()).await;
    let dir = tempfile::tempdir().unwrap();
    for args in [&["whoami"][..], &["who"]] {
        let out = run(riff(&url, dir.path(), args)).await;
        assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
        let line = format!("riff and riff-server have the build {VERSION}.");
        assert!(text(&out.stdout).contains(&line), "{}", text(&out.stdout));
        assert!(!text(&out.stderr).contains("Run riff update"));
    }
}

/// 01M3MNVT7G701SDP1Z1THMRDQ2, 01M3MNVT9TYNXZ8V845BHKQADV
#[tokio::test(flavor = "multi_thread")]
async fn another_build_with_the_same_wire_works_and_notes_it_once() {
    let theirs = same_wire();
    let url = real(theirs.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = run(riff(&url, dir.path(), &["who"])).await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "{stderr}");
    let note = riff_core::build::other_build(&Build::this(), &theirs);
    assert_eq!(stderr.matches(&note).count(), 1, "{stderr}");
    let line = format!(
        "riff has the build {VERSION}; riff-server has the build {theirs}. The wire matches."
    );
    assert!(stdout.contains(&line), "{stdout}");
}

/// 01M3JEE7TPZMNK7X6JXJ7GWFPP
#[tokio::test(flavor = "multi_thread")]
async fn the_start_hook_tells_the_session_of_the_mismatch() {
    let url = fake(Some(other("2000-01-01T00:00:00Z"))).await;
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = riff(&url, dir.path(), &["hook", "session-start"]);
    cmd.env_remove("RIFF_SESSION")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let out = tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                br#"{"session_id":"b2","source":"startup","hook_event_name":"SessionStart"}"#,
            )
            .unwrap();
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap();
    let out: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let context = out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("do not match"), "{context}");
    assert!(context.contains(VERSION), "{context}");
    assert!(context.contains("Tell your user now"), "{context}");
    assert!(!context.contains("riff watch --once"), "{context}");
}

/// `riff tail` and `riff watch` wait on another wire version, and go on
/// when the wire matches again (01M3MNVTC248YYJJQKFD9H1WY9).
#[tokio::test(flavor = "multi_thread")]
async fn tail_and_watch_wait_on_another_wire_and_go_on() {
    let other_wire = Arc::new(AtomicBool::new(true));
    let url = streams(other_wire.clone()).await;
    let tail_dir = tempfile::tempdir().unwrap();
    let watch_dir = tempfile::tempdir().unwrap();
    let (mut tail, tail_out, tail_err) = spawn(
        riff(&url, tail_dir.path(), &["tail", "como-technologies/riff"]),
        tail_dir.path(),
    );
    let (mut watch, _, watch_err) = spawn(
        riff(&url, watch_dir.path(), &["watch", "--once"]),
        watch_dir.path(),
    );
    let told = |err: &Path| read(err).contains("do not match");
    assert!(
        wait_for(Duration::from_secs(10), || told(&tail_err)
            && told(&watch_err))
        .await
    );
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(tail.try_wait().unwrap().is_none(), "the tail stopped");
    assert!(watch.try_wait().unwrap().is_none(), "the watch stopped");
    assert_eq!(read(&tail_err).matches("do not match").count(), 1);
    assert_eq!(read(&watch_err).matches("do not match").count(), 1);

    other_wire.store(false, Ordering::SeqCst);
    let back = wait_for(Duration::from_secs(15), || {
        read(&tail_out).contains("back again")
    })
    .await;
    let _ = (tail.kill(), watch.kill(), tail.wait(), watch.wait());
    assert!(
        back,
        "stdout: {}\nstderr: {}",
        read(&tail_out),
        read(&tail_err)
    );
}

/// `riff tail` and `riff watch` run the new binary when the file on
/// disk changes, with the same arguments (01M3MNVTC248YYJJQKFD9H1WY9).
#[tokio::test(flavor = "multi_thread")]
async fn tail_and_watch_run_the_new_binary() {
    use std::os::unix::fs::PermissionsExt;

    let url = streams(Arc::new(AtomicBool::new(false))).await;
    for args in [
        &["tail", "como-technologies/riff"][..],
        &["watch", "--once"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("riff");
        std::fs::copy(assert_cmd::cargo::cargo_bin("riff"), &binary).unwrap();
        let (mut child, _, err) = spawn(riff_at(&binary, &url, dir.path(), args), dir.path());
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(
            child.try_wait().unwrap().is_none(),
            "{args:?}: {}",
            read(&err)
        );

        // A new binary in its place, as `cargo install` does.
        let marker = dir.path().join("ran");
        let new = dir.path().join("new");
        let script = format!("#!/bin/sh\necho \"$@\" > '{}'\n", marker.display());
        std::fs::write(&new, script).unwrap();
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::rename(&new, &binary).unwrap();

        let ran = wait_for(Duration::from_secs(10), || marker.exists()).await;
        let _ = (child.kill(), child.wait());
        assert!(ran, "{args:?}: {}", read(&err));
        assert_eq!(read(&marker).trim(), args.join(" "), "{args:?}");
        assert!(
            read(&err).contains("a new riff is on disk"),
            "{}",
            read(&err)
        );
    }
}

/// The error links to the book part that tells how to update, and the
/// book shows how to see the build.
#[test]
fn the_book_has_the_part_that_the_error_names() {
    let book = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let anchor = riff_core::build::UPDATE_URL
        .rsplit_once('#')
        .unwrap()
        .1
        .replace('-', " ");
    let heading = format!("### {}{}\n", anchor[..1].to_uppercase(), &anchor[1..]);
    assert!(book.contains(&heading), "no {heading:?}");
    assert!(book.contains("```sh\nriff --version\nriff-server --version\n```"));
}
