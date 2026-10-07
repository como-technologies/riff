//! `riff` and a `riff-server` of another build (01M3JEE7P46GWXR1BD4Q1TTSGN
//! to 01M3JEE7WT04BKX377VW5GDSPY, 01M3MX1DYY6AVDW946NR0B9T2C to
//! 01M3MX1E8M9TKBN90P4DYKH3H8, 01M3MNVTC248YYJJQKFD9H1WY9,
//! 01M3NT6WXGCNKW3EQ7MBJDQTR4). A fake server
//! names a version that this riff cannot talk to, or no build. A real
//! server with another build in its reply names a version that it can
//! talk to.

use std::convert::Infallible;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::http::HeaderValue;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures::Stream;
use isolated::Isolated;
use riff_core::build::{Build, HEADER, Semver, VERSION};
use riff_core::wire::{Kind, Message, Tailed};

/// The version of this riff.
fn this() -> Semver {
    Build::this().semver().unwrap()
}

/// A build of `version` with another commit.
fn at(version: Semver) -> Build {
    Build {
        version: version.to_string(),
        commit: "0000deadbeef".into(),
        time: "2000-01-01T00:00:00Z".into(),
    }
}

/// A server of the line before this riff, or of 0.8 for the line 1:
/// this riff is newer, and cannot talk to it.
fn older() -> Build {
    at(this()
        .line_before()
        .unwrap_or_else(|| "0.8.0".parse().unwrap()))
}

/// A server two lines after this riff: it refuses this riff.
fn much_newer() -> Build {
    at(this().line_after().line_after())
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
    serve(real_router(build)).await
}

/// The router of [`real`].
fn real_router(build: Build) -> axum::Router {
    riff_server::router().layer(axum::middleware::map_response(stamp(Some(build))))
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
        payload: None,
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

/// A fake server with a tail stream and a watch stream. While
/// `other_line` is true, it names a version that this riff cannot talk
/// to.
async fn streams(other_line: Arc<AtomicBool>) -> String {
    serve(streams_router(other_line)).await
}

/// The router of [`streams`].
fn streams_router(other_line: Arc<AtomicBool>) -> axum::Router {
    axum::Router::new()
        .route("/v1/tail", get(|| async { one_message() }))
        .route(
            "/v1/watch",
            get(|| async { Sse::new(futures::stream::pending::<Result<Event, Infallible>>()) }),
        )
        .layer(axum::middleware::map_response(move |r: Response| {
            let build = if other_line.load(Ordering::SeqCst) {
                much_newer()
            } else {
                Build::this()
            };
            stamp(Some(build))(r)
        }))
}

fn riff_at(binary: &Path, server: &str, dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Isolated::shared().command(binary);
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "brett")
        .env("RIFF_HOST", "heron")
        .env("RIFF_SESSION", "b2")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("RIFF_HOME", dir);
    cmd
}

fn riff(server: &str, dir: &Path, args: &[&str]) -> Command {
    riff_at(&Isolated::shared().riff_path(), server, dir, args)
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

/// The bound of each wait for a fact. It is generous: it only ends a
/// test that hangs, and a busy machine passes (#460).
const WAIT: Duration = Duration::from_secs(60);

/// Waits up to `limit` until `test` is true.
async fn wait_for(limit: Duration, test: impl Fn() -> bool) -> bool {
    let span = isolated::Span::start();
    while span.within(limit) {
        if test() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    test()
}

/// `join`, `post` and `read` each fail, and name both builds and the
/// side to update (01M3MX1E65XGWDZ062PQ9YXQ5T).
async fn each_call_fails(server: Option<Build>, step: &str) {
    let url = fake(server.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let theirs = server.map_or(
        format!("no riff build in the reply: status 200 OK from {url}/v1/"),
        |b| b.to_string(),
    );
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
    each_call_fails(Some(older()), "Update riff-server").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_older_riff_is_refused_and_names_riff_to_update() {
    each_call_fails(Some(much_newer()), "Update riff on this machine").await;
    each_call_fails(Some(much_newer()), riff_core::build::UPDATE_URL).await;
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
        let line = format!("  v{}  (", env!("CARGO_PKG_VERSION"));
        assert!(text(&out.stdout).contains(&line), "{}", text(&out.stdout));
        assert!(
            !text(&out.stdout).contains("riff-server"),
            "{}",
            text(&out.stdout)
        );
        assert!(!text(&out.stderr).contains("Run riff update"));
    }
}

/// Runs `riff who` against a real server that names `theirs`. It
/// works, and prints `note` once.
async fn works_and_notes_once(theirs: Build, note: &str) {
    let url = real(theirs.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = run(riff(&url, dir.path(), &["who"])).await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "{stderr}");
    assert_eq!(stderr.matches(note).count(), 1, "{stderr}");
    assert_eq!(
        stderr.matches("riff-server runs build").count(),
        1,
        "{stderr}"
    );
    let commit: String = theirs.commit.chars().take(7).collect();
    let line = format!("\nriff-server  v{}  ({commit}, ", theirs.version);
    assert!(stdout.contains(&line), "{stdout}");
    assert!(
        stdout.contains("  another build; the versions can talk\n"),
        "{stdout}"
    );
}

/// 01M3MX1DYY6AVDW946NR0B9T2C, 01M3MX1E8M9TKBN90P4DYKH3H8: the same
/// line and another patch.
#[tokio::test(flavor = "multi_thread")]
async fn another_patch_of_the_line_works_and_notes_it_once() {
    let theirs = at(Semver {
        patch: this().patch + 3,
        ..this()
    });
    let note = format!(
        "riff-server runs build {theirs}; this riff runs build {VERSION}. Run riff update \
         when you can."
    );
    works_and_notes_once(theirs, &note).await;
}

/// 01M3MX1E1EY1M7JGNCN6FCEVQK: a server of the line after this riff
/// talks with it, and the note says to update soon.
#[tokio::test(flavor = "multi_thread")]
async fn a_server_of_the_next_line_works_with_the_update_note() {
    let next = this().line_after();
    let theirs = at(next);
    let note = format!(
        "riff-server runs build {theirs}; this riff runs build {VERSION}. riff-server {} will \
         refuse riff {}. Run riff update soon.",
        next.line_after().line(),
        this().line()
    );
    works_and_notes_once(theirs, &note).await;
}

/// 01M3JEE7TPZMNK7X6JXJ7GWFPP
#[tokio::test(flavor = "multi_thread")]
async fn the_start_hook_tells_the_session_of_the_mismatch() {
    let url = fake(Some(older())).await;
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

/// `riff tail` and `riff watch` wait on a version that they cannot talk
/// to, and go on when the versions can talk again
/// (01M3MNVTC248YYJJQKFD9H1WY9).
#[tokio::test(flavor = "multi_thread")]
async fn tail_and_watch_wait_on_another_line_and_go_on() {
    let other_line = Arc::new(AtomicBool::new(true));
    let url = streams(other_line.clone()).await;
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
    assert!(wait_for(WAIT, || told(&tail_err) && told(&watch_err)).await);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(tail.try_wait().unwrap().is_none(), "the tail stopped");
    assert!(watch.try_wait().unwrap().is_none(), "the watch stopped");
    assert_eq!(read(&tail_err).matches("do not match").count(), 1);
    assert_eq!(read(&watch_err).matches("do not match").count(), 1);

    other_line.store(false, Ordering::SeqCst);
    let back = wait_for(WAIT, || read(&tail_out).contains("back again")).await;
    let _ = (tail.kill(), watch.kill(), tail.wait(), watch.wait());
    assert!(
        back,
        "stdout: {}\nstderr: {}",
        read(&tail_out),
        read(&tail_err)
    );
}

/// Runs `cmd`, which writes an executable file. This process never
/// holds a write fd of the file. So a fork of a parallel test cannot
/// inherit one, and an exec of the file cannot fail with ETXTBSY.
fn by_child(cmd: &mut Command) {
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{out:?}");
}

/// `riff tail` and `riff watch` run the new binary when the file on
/// disk changes, with the same arguments (01M3MNVTC248YYJJQKFD9H1WY9).
#[tokio::test(flavor = "multi_thread")]
async fn tail_and_watch_run_the_new_binary() {
    let asked = Arc::new(AtomicBool::new(false));
    let router = streams_router(Arc::new(AtomicBool::new(false)));
    let url = serve(marked(router, asked.clone())).await;
    runs_the_new_binary(&url, &asked, &["tail", "como-technologies/riff"]).await;
    runs_the_new_binary(&url, &asked, &["watch", "--once"]).await;
}

/// `riff top` runs the new binary the same way
/// (01M3NT6WXGCNKW3EQ7MBJDQTR4).
#[tokio::test(flavor = "multi_thread")]
async fn top_runs_the_new_binary() {
    let asked = Arc::new(AtomicBool::new(false));
    let url = serve(marked(real_router(Build::this()), asked.clone())).await;
    runs_the_new_binary(&url, &asked, &["top"]).await;
}

/// Puts a new binary in place of `binary`, as `cargo install` does: a
/// script that writes its arguments to the file that it returns.
fn new_binary(binary: &Path, dir: &Path) -> PathBuf {
    let marker = dir.join("ran");
    let new = dir.join("new");
    let script = format!("#!/bin/sh\necho \"$@\" > '{}'\n", marker.display());
    by_child(
        Command::new("sh")
            .args(["-c", "printf '%s' \"$1\" > \"$0\" && chmod 755 \"$0\""])
            .arg(&new)
            .arg(script),
    );
    std::fs::rename(&new, binary).unwrap();
    marker
}

/// `router` with a mark: `asked` is true after its first request.
fn marked(router: axum::Router, asked: Arc<AtomicBool>) -> axum::Router {
    let mark = move |request: axum::extract::Request, next: axum::middleware::Next| {
        asked.store(true, Ordering::SeqCst);
        next.run(request)
    };
    router.layer(axum::middleware::from_fn(mark))
}

/// A riff that runs `args` at `url` runs the new binary on disk, with
/// the same arguments after the place of the old process.
/// `asked` is the mark of the server ([`marked`]).
async fn runs_the_new_binary(url: &str, asked: &AtomicBool, args: &[&str]) {
    asked.store(false, Ordering::SeqCst);
    let dir = isolated::outside_git();
    let binary = dir.path().join("riff");
    by_child(
        Command::new("cp")
            .arg(Isolated::shared().riff_path())
            .arg(&binary),
    );
    let (mut child, _, err) = spawn(riff_at(&binary, url, dir.path(), args), dir.path());
    // The process runs: it asked the server, so it looks at its binary
    // on disk (#460).
    let started = wait_for(WAIT, || asked.load(Ordering::SeqCst)).await;
    assert!(started, "{args:?}: {}", read(&err));
    assert!(
        child.try_wait().unwrap().is_none(),
        "{args:?}: {}",
        read(&err)
    );

    let marker = new_binary(&binary, dir.path());

    // The line ends with a newline once the script wrote all of it.
    let ran = wait_for(WAIT, || read(&marker).ends_with('\n')).await;
    let _ = (child.kill(), child.wait());
    assert!(ran, "{args:?}: {}", read(&err));
    // The same arguments, after the place of the old process: the
    // host, no repository, and the directory as the worktree.
    let line = read(&marker);
    let (place, rest) = line.trim().split_once(' ').unwrap();
    assert_eq!(place, "--place", "{line}");
    assert!(rest.starts_with("heron/-#"), "{line}");
    let ran = rest.split_once(' ').unwrap().1;
    // A watch with `--once` also gives the end of its wait, at most
    // the default limit from now (01M3Z64J08GW6N1H42AR2FZQZ4).
    let ran = match ran.split_once(" --until ") {
        Some((ran, until)) if args.contains(&"--once") => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let until: u64 = until.parse().unwrap();
            let limit = riff::settings::WATCH_LIMIT;
            assert!((now + limit - 60..=now + limit).contains(&until), "{line}");
            ran
        }
        _ => {
            assert!(!args.contains(&"--once"), "no --until: {line}");
            ran
        }
    };
    assert_eq!(ran, args.join(" "), "{line}");
    assert!(
        read(&err).contains("a new riff is on disk"),
        "{}",
        read(&err)
    );
}

/// Puts a copy of `from` at `to` as `cargo install` does: a new file
/// beside it, then a rename.
fn install(from: &Path, to: &Path) {
    let stage = to.with_extension("stage");
    by_child(Command::new("cp").arg(from).arg(&stage));
    std::fs::rename(&stage, to).unwrap();
}

/// A watch whose working directory is gone at an update runs the new
/// riff in the nearest parent that exists, and keeps watching: the next
/// post wakes it (01M3NJGD45GF7Y4CZWQ7GRDHZN). The update goes as
/// `riff update` does it: riff, then riff-server.
#[tokio::test(flavor = "multi_thread")]
async fn a_watch_in_a_removed_worktree_runs_the_new_riff_and_keeps_watching() {
    let url = real(Build::this()).await;
    let root = tempfile::tempdir().unwrap();
    let (bin, data) = (root.path().join("bin"), root.path().join("data"));
    let worktrees = root.path().join("repo/.claude/worktrees");
    let worktree = worktrees.join("issue-12");
    for dir in [&bin, &data, &worktree] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let isolated = Isolated::shared();
    install(&isolated.riff_path(), &bin.join("riff"));
    install(&isolated.riff_server_path(), &bin.join("riff-server"));
    let mut watch = riff_at(&bin.join("riff"), &url, &worktree, &["watch", "--once"]);
    watch.env("RIFF_HOME", root.path());
    let (mut child, out, err) = spawn(watch, root.path());
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(child.try_wait().unwrap().is_none(), "{}", read(&err));

    std::fs::remove_dir(&worktree).unwrap();
    install(&isolated.riff_path(), &bin.join("riff"));
    install(&isolated.riff_server_path(), &bin.join("riff-server"));
    let moved = format!(
        "riff: the working directory {} is gone. The new riff runs in {}.",
        worktree.display(),
        worktrees.display()
    );
    let ran = wait_for(WAIT, || read(&err).contains(&moved)).await;
    assert!(ran, "{}", read(&err));
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        child.try_wait().unwrap().is_none(),
        "the new riff stopped: {}",
        read(&err)
    );

    let mut tell = riff(&url, root.path(), &["tell", "b2", "wake up"]);
    tell.env("RIFF_SESSION", "a1").env("RIFF_HOME", &data);
    let told = run(tell).await;
    assert!(told.status.success(), "{}", text(&told.stderr));
    let woke = wait_for(WAIT, || !read(&out).is_empty()).await;
    let _ = (child.kill(), child.wait());
    assert!(woke, "no wake: {}", read(&err));
    assert!(!read(&err).contains("No such file"), "{}", read(&err));
}

/// Runs `git` in `dir`.
fn git(dir: &Path, args: &[&str]) {
    let out = Isolated::shared()
        .command("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A repository at `dir` with the `origin` `acme/NAME` and one commit.
fn repo(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    let url = format!("https://github.com/acme/{name}.git");
    git(dir, &["remote", "add", "origin", &url]);
    git(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "start",
        ],
    );
}

/// A real riff-server that answers 503 until `ready` is true.
async fn gated(ready: Arc<AtomicBool>) -> String {
    let router = riff_server::router().layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let ready = ready.load(Ordering::SeqCst);
            async move {
                if ready {
                    next.run(request).await
                } else {
                    axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response()
                }
            }
        },
    ));
    serve(router).await
}

/// The new riff keeps the place of the old one: a worktree of repo
/// `alpha` inside repo `beta` stays in `alpha` and keeps its worktree
/// after the update, also when the worktree is gone
/// (01M3NJGD45GF7Y4CZWQ7GRDHZN). The server answers only after the
/// update, so it knows only the place that the new riff gives.
#[tokio::test(flavor = "multi_thread")]
async fn tail_and_watch_keep_their_place_over_an_update() {
    let ready = Arc::new(AtomicBool::new(false));
    let url = gated(ready.clone()).await;
    let root = tempfile::tempdir().unwrap();
    let (alpha, beta, bin) = (
        root.path().join("alpha"),
        root.path().join("beta"),
        root.path().join("bin"),
    );
    repo(&alpha, "alpha");
    repo(&beta, "beta");
    let worktree = beta.join("wt/issue-12");
    git(
        &alpha,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "issue-12",
            worktree.to_str().unwrap(),
        ],
    );
    std::fs::create_dir_all(&bin).unwrap();
    install(&Isolated::shared().riff_path(), &bin.join("riff"));

    let mut runs = vec![];
    for (name, args) in [("tail", &["tail"][..]), ("watch", &["watch", "--once"])] {
        let logs = root.path().join(name);
        std::fs::create_dir(&logs).unwrap();
        let mut cmd = riff_at(&bin.join("riff"), &url, &worktree, args);
        cmd.env("RIFF_HOME", &logs);
        runs.push(spawn(cmd, &logs));
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    std::fs::remove_dir_all(&worktree).unwrap();
    install(&Isolated::shared().riff_path(), &bin.join("riff"));
    let moved = |err: &Path| read(err).contains("The new riff runs in");
    let both = wait_for(WAIT, || runs.iter().all(|(_, _, err)| moved(err))).await;
    assert!(both, "{}\n{}", read(&runs[0].2), read(&runs[1].2));
    ready.store(true, Ordering::SeqCst);

    // The new watch has the place in its arguments, not in its
    // environment. A riff that it starts, with that environment, takes
    // the place of its own directory.
    let proc = PathBuf::from(format!("/proc/{}", runs[1].0.id()));
    let cmdline = || {
        let bytes = std::fs::read(proc.join("cmdline")).unwrap_or_default();
        String::from_utf8_lossy(&bytes).replace('\0', " ")
    };
    let want = "--place heron/acme/alpha#issue-12 watch --once";
    let execed = wait_for(WAIT, || cmdline().contains(want)).await;
    assert!(execed, "{:?}: {}", cmdline(), read(&runs[1].2));
    let environ = std::fs::read(proc.join("environ")).unwrap();
    let vars = environ
        .split(|b| *b == 0)
        .filter_map(|var| {
            String::from_utf8_lossy(var)
                .split_once('=')
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
        })
        .collect::<Vec<_>>();
    let mut child = Command::new(Isolated::shared().riff_path());
    child
        .env_clear()
        .envs(vars)
        // No server, so that the watch session stays where it is.
        .env("RIFF_SERVER", "http://127.0.0.1:9")
        .current_dir(&beta)
        .arg("whoami");
    let whoami = run(child).await;
    let me = text(&whoami.stdout);
    assert!(
        me.contains("riff://brett@heron/acme/beta?session=b2"),
        "{me}{}",
        text(&whoami.stderr)
    );

    let (tail_out, tail_err) = (&runs[0].1, &runs[0].2);
    let showing = "showing new messages in acme/alpha";
    let again = wait_for(WAIT, || read(tail_err).matches(showing).count() == 2).await;
    assert!(again, "{}", read(tail_err));
    assert!(!read(tail_err).contains("acme/beta"), "{}", read(tail_err));
    // The tail connects again within one retry, and shows only new
    // messages. So post until it shows one.
    let span = isolated::Span::start();
    while !read(tail_out).contains("alpha two") && span.within(Duration::from_secs(20)) {
        let mut post = riff(
            &url,
            root.path(),
            &["post", "--thread", "acme/alpha", "alpha two"],
        );
        post.env("RIFF_SESSION", "a1").env("RIFF_HOME", root.path());
        let posted = run(post).await;
        assert!(posted.status.success(), "{}", text(&posted.stderr));
        wait_for(Duration::from_secs(2), || {
            read(tail_out).contains("alpha two")
        })
        .await;
    }
    assert!(read(tail_out).contains("alpha two"), "{}", read(tail_err));

    let arrived = || async {
        let mut who = riff(&url, root.path(), &["who"]);
        who.env("RIFF_SESSION", "a1").env("RIFF_HOME", root.path());
        text(&run(who).await.stdout)
    };
    let mut listed = arrived().await;
    let span = isolated::Span::start();
    while !listed.contains("brett@heron:alpha#issue-12") && span.within(Duration::from_secs(15)) {
        tokio::time::sleep(Duration::from_millis(200)).await;
        listed = arrived().await;
    }
    for (child, _, _) in &mut runs {
        let _ = (child.kill(), child.wait());
    }
    assert!(listed.contains("brett@heron:alpha#issue-12"), "{listed}");
    assert!(!listed.contains("beta"), "{listed}");
}

/// riff in a removed working directory names it in the error
/// (01M3NJGD6H8DNVHHHG80F9YFCE).
#[tokio::test(flavor = "multi_thread")]
async fn the_error_names_a_removed_working_directory() {
    let url = real(Build::this()).await;
    let root = tempfile::tempdir().unwrap();
    let gone = root.path().join("issue-12");
    std::fs::create_dir(&gone).unwrap();
    let mut cmd = riff_at(Path::new("sh"), &url, root.path(), &["-c"]);
    cmd.arg("cd \"$0\" && rmdir \"$0\" && exec \"$1\" whoami")
        .arg(&gone)
        .arg(Isolated::shared().riff_path());
    let out = run(cmd).await;
    let err = text(&out.stderr);
    assert!(!out.status.success(), "{err}");
    let named = format!(
        "riff cannot read its working directory {}. Change to a directory that exists",
        gone.display()
    );
    assert!(err.contains(&named), "{err}");
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

/// The book says what each level of a version means, and the test for
/// each release (01M3N73E7YTHFX2J2KXT7017QX to
/// 01M3N73EEQ4HPCPPAGCAHH3S6B).
#[test]
fn the_book_says_what_patch_minor_and_major_mean() {
    let book = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let part = &book[book.find("\n## Versions\n").expect("no ## Versions")..];
    let part = &part[..part[4..].find("\n## ").map_or(part.len(), |end| end + 4)];
    for heading in [
        "### Patch\n",
        "### Minor\n",
        "### Major\n",
        "### The test for each release\n",
    ] {
        assert!(part.contains(heading), "no {heading:?} in: {part}");
    }
    for minor in [
        "the wire",
        "the saved state of `riff-server`",
        "the plugin contract",
        "the behavior",
    ] {
        assert!(part.contains(minor), "no {minor:?} in: {part}");
    }
    assert!(part.contains("act differently"), "{part}");
    assert!(part.contains("fail to understand"), "{part}");
}

/// The notes and the error in the book are the real text
/// (01M3MX1E8M9TKBN90P4DYKH3H8, 01M3MX1E65XGWDZ062PQ9YXQ5T).
#[test]
fn the_book_shows_the_real_notes_and_error() {
    use riff_core::build::{Mismatch, other_build};

    let book = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let b = |s: &str| s.parse::<Build>().unwrap();
    let server = b("0.4.0 7213825ab1c2 2026-09-27T20:10:44Z");
    for text in [
        other_build(
            &b("0.4.0 929605821e54 2026-09-27T22:03:01Z"),
            &b("0.4.3 7213825ab1c2 2026-09-27T20:10:44Z"),
        ),
        other_build(&b("0.3.2 929605821e54 2026-09-20T22:03:01Z"), &server),
        Mismatch {
            riff: Some(b("0.2.0 929605821e54 2026-09-27T22:03:01Z")),
            server: Some(b("0.4.0 7213825ab1c2 2026-09-28T20:10:44Z")),
            seen: None,
        }
        .to_string(),
    ] {
        let block = format!("```text\nriff: {text}\n```");
        assert!(book.contains(&block), "the book has no {block}");
    }
}

/// A real riff-server behind a front end that answers the first `fails`
/// calls with `status` and no build header, as Cloud Run does while it
/// moves an instance (01M3QCMJ9F1GRTRRSB4AW9TC3D).
async fn behind_a_front_end(fails: usize, status: axum::http::StatusCode) -> String {
    let left = Arc::new(std::sync::atomic::AtomicUsize::new(fails));
    let router = riff_server::router().layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let fail = left
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok();
            async move {
                if fail {
                    status.into_response()
                } else {
                    next.run(request).await
                }
            }
        },
    ));
    serve(router).await
}

/// A 5xx or 429 of the front end with no build is an outage, not a
/// version error: riff tries again, and the call works
/// (01M3QCMJ9F1GRTRRSB4AW9TC3D).
#[tokio::test(flavor = "multi_thread")]
async fn a_front_end_error_with_no_build_is_tried_again() {
    use axum::http::StatusCode;
    for status in [
        StatusCode::BAD_GATEWAY,
        StatusCode::GATEWAY_TIMEOUT,
        StatusCode::TOO_MANY_REQUESTS,
    ] {
        let url = behind_a_front_end(3, status).await;
        let dir = tempfile::tempdir().unwrap();
        let out = run(riff(&url, dir.path(), &["who"])).await;
        let err = text(&out.stderr);
        assert!(out.status.success(), "{status}: {err}");
        assert!(!err.contains("do not match"), "{status}: {err}");
    }
}

/// A reply with no build that is not an outage is still a version
/// error: a 200 or a 404 comes from an old server. The error names the
/// status and the URL (01M3MX1E65XGWDZ062PQ9YXQ5T,
/// 01M3QCMJ9F1GRTRRSB4AW9TC3D).
#[tokio::test(flavor = "multi_thread")]
async fn a_success_or_client_error_with_no_build_is_still_a_mismatch() {
    use axum::http::StatusCode;
    let me: riff_core::name::SessionUri = "riff://brett@heron/como-technologies/riff?session=b2"
        .parse()
        .unwrap();
    for status in [StatusCode::OK, StatusCode::NOT_FOUND] {
        let router = axum::Router::new().fallback(move || async move { (status, "{}") });
        let api = riff::api::Api::new(&serve(router).await);
        let error = api.who(&me, false).await.unwrap_err();
        let mismatch = error.downcast_ref::<riff_core::build::Mismatch>();
        assert!(
            mismatch.is_some_and(|m| m.server.is_none()),
            "{status}: {error:#}"
        );
        // The text says what riff saw: the status and the URL.
        let seen = format!(
            "no riff build in the reply: status {status} from {}/v1/who",
            api.base()
        );
        assert!(error.to_string().contains(&seen), "{seen}: {error:#}");
        assert!(
            error.to_string().contains("Update riff-server"),
            "{error:#}"
        );
    }
}
