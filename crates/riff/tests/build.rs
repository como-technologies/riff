//! `riff` and a `riff-server` of another build (01M3JEE7P46GWXR1BD4Q1TTSGN
//! to 01M3JEE7WT04BKX377VW5GDSPY, 01M3MX1DYY6AVDW946NR0B9T2C to
//! 01M3MX1E8M9TKBN90P4DYKH3H8, 01M3MNVTC248YYJJQKFD9H1WY9,
//! 01M3NT6WXGCNKW3EQ7MBJDQTR4). A fake server
//! names a version that this riff cannot talk to, or no build. A real
//! server with another build in its reply names a version that it can
//! talk to.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response};
use isolated::Isolated;
use riff_core::build::{Build, HEADER, Semver, VERSION};

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

/// A server three lines after this riff: it refuses this riff. Two
/// lines after the line 1 is the line 3, which talks with the line 1
/// (01M55FCZNNZGKM4EKAXZQFTDWR).
fn much_newer() -> Build {
    at(this().line_after().line_after().line_after())
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
async fn real(build: Build) -> crate::common::Signed {
    crate::common::start_signed_with(move |router| real_router(router, build)).await
}

/// The router of [`real`].
fn real_router(router: axum::Router, build: Build) -> axum::Router {
    router.layer(axum::middleware::map_response(stamp(Some(build))))
}

fn riff_at(binary: &Path, server: &str, dir: &Path, args: &[&str]) -> Command {
    crate::common::sign_in(server, dir, "brett");
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
    let server = real(Build::this()).await;
    let url = &*server;
    let dir = tempfile::tempdir().unwrap();
    for args in [&["whoami"][..], &["who"]] {
        let out = run(riff(url, dir.path(), args)).await;
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
    let server = real(theirs.clone()).await;
    let url = &*server;
    let dir = tempfile::tempdir().unwrap();
    let out = run(riff(url, dir.path(), &["who"])).await;
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

/// 01M56Q55SHHJSKJWNHMW79X7HA: a server of the same version from an
/// older commit is the older side, and the note says to update it.
#[tokio::test(flavor = "multi_thread")]
async fn an_older_server_of_the_same_line_works_with_the_server_note() {
    let theirs = at(this());
    let note = format!(
        "riff-server runs build {theirs}; this riff runs build {VERSION}. Update riff-server \
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

/// 01M55FCZNNZGKM4EKAXZQFTDWR: a server of the other line of the
/// bridge talks with this riff. The note names the older side.
#[tokio::test(flavor = "multi_thread")]
async fn a_server_of_the_other_line_of_the_bridge_works_with_the_note() {
    let (version, step) = match this().major {
        1 => (
            "3.0.0",
            "riff-server 4 will refuse riff 1. Run riff update soon.",
        ),
        3 => ("1.8.2", "Update riff-server when you can."),
        // The bridge ends with the line 4: riff 4 refuses a server of
        // the line 3 or 1 (01M55FCZNNZGKM4EKAXZQFTDWR).
        4 => return,
        major => panic!("riff {major}.x is on no line of the bridge"),
    };
    let theirs = at(version.parse().unwrap());
    let note = format!("riff-server runs build {theirs}; this riff runs build {VERSION}. {step}");
    works_and_notes_once(theirs, &note).await;
}

/// Runs `cmd`, which writes an executable file. This process never
/// holds a write fd of the file. So a fork of a parallel test cannot
/// inherit one, and an exec of the file cannot fail with ETXTBSY.
fn by_child(cmd: &mut Command) {
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{out:?}");
}

/// `riff top` runs the new binary the same way
/// (01M3NT6WXGCNKW3EQ7MBJDQTR4).
#[tokio::test(flavor = "multi_thread")]
async fn top_runs_the_new_binary() {
    let asked = Arc::new(AtomicBool::new(false));
    let server = crate::common::start_signed_with({
        let asked = asked.clone();
        move |router| marked(real_router(router, Build::this()), asked)
    })
    .await;
    runs_the_new_binary(&server, &asked, &["top"]).await;
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
    assert_eq!(ran, args.join(" "), "{line}");
    assert!(
        read(&err).contains("a new riff is on disk"),
        "{}",
        read(&err)
    );
}

/// riff in a removed working directory names it in the error
/// (01M3NJGD6H8DNVHHHG80F9YFCE).
#[tokio::test(flavor = "multi_thread")]
async fn the_error_names_a_removed_working_directory() {
    let server = real(Build::this()).await;
    let url = &*server;
    let root = tempfile::tempdir().unwrap();
    let gone = root.path().join("issue-12");
    std::fs::create_dir(&gone).unwrap();
    let mut cmd = riff_at(Path::new("sh"), url, root.path(), &["-c"]);
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
/// 01M3N73EEQ4HPCPPAGCAHH3S6B). The minor names the line before, as
/// the server does (01M3MX1E1EY1M7JGNCN6FCEVQK). The major links to the
/// level of the line 1.x, and says its last minor and the next major
/// (01M53N7KN4QKZ70W2GNQ3VQWQ9, 01M53NGVTE186GE5E48E68N0BY).
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
        "the behavior",
    ] {
        assert!(part.contains(minor), "no {minor:?} in: {part}");
    }
    assert!(part.contains("act differently"), "{part}");
    assert!(part.contains("fail to understand"), "{part}");
    assert!(
        part.contains("`riff-server` still talks with a `riff` of the line before its own"),
        "{part}"
    );
    assert!(!part.contains("minor before"), "{part}");
    let major = &part[part.find("### Major\n").unwrap()..];
    let major = &major[..major[4..].find("\n### ").unwrap() + 4];
    assert!(
        major.contains(
            "The line\n1.x takes no major: see [Make a release](development.md#make-a-release)."
        ),
        "{major}"
    );
    assert!(
        major.contains("1.8.x is the last minor release of 1.x. The next major is 3.0.0"),
        "{major}"
    );
}

/// The notes and the error in the book are the real text
/// (01M3MX1E8M9TKBN90P4DYKH3H8, 01M3MX1E65XGWDZ062PQ9YXQ5T,
/// 01M55FCZNNZGKM4EKAXZQFTDWR).
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
        other_build(
            &b("0.4.3 929605821e54 2026-09-27T22:03:01Z"),
            &b("0.4.0 7213825ab1c2 2026-09-27T20:10:44Z"),
        ),
        other_build(&b("0.3.2 929605821e54 2026-09-20T22:03:01Z"), &server),
        other_build(
            &b("3.0.0 929605821e54 2026-10-18T22:03:01Z"),
            &b("1.8.2 7213825ab1c2 2026-10-17T20:10:44Z"),
        ),
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

/// The text of a reply of Cloud Run when it sends a call to an instance
/// that exited (01M56WNP3TEGH98B9EAM4H62P4).
const NO_INSTANCE: &str = "The request was aborted because there was no available instance.";

/// A real riff-server behind a front end that answers the first `fails`
/// calls with `status`, the text of Cloud Run and no build header, as
/// Cloud Run does while it moves an instance
/// (01M3QCMJ9F1GRTRRSB4AW9TC3D).
async fn behind_a_front_end(fails: usize, status: axum::http::StatusCode) -> crate::common::Signed {
    let left = Arc::new(std::sync::atomic::AtomicUsize::new(fails));
    crate::common::start_signed_with(move |router| {
        router.layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let fail = left
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                    .is_ok();
                async move {
                    if fail {
                        (status, NO_INSTANCE).into_response()
                    } else {
                        next.run(request).await
                    }
                }
            },
        ))
    })
    .await
}

/// A 5xx or 429 of the front end with no build is an outage, not a
/// version error: riff tries again, and the call works
/// (01M3QCMJ9F1GRTRRSB4AW9TC3D). The 500 and the 429 of Cloud Run with
/// no available instance are two of them (01M56WNP3TEGH98B9EAM4H62P4).
#[tokio::test(flavor = "multi_thread")]
async fn a_front_end_error_with_no_build_is_tried_again() {
    use axum::http::StatusCode;
    for status in [
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::BAD_GATEWAY,
        StatusCode::GATEWAY_TIMEOUT,
        StatusCode::TOO_MANY_REQUESTS,
    ] {
        let server = behind_a_front_end(3, status).await;
        let url = &*server;
        let dir = tempfile::tempdir().unwrap();
        let out = run(riff(url, dir.path(), &["who"])).await;
        let err = text(&out.stderr);
        assert!(out.status.success(), "{status}: {err}");
        assert!(!err.contains("do not match"), "{status}: {err}");
    }
}

/// The probe of `riff server` and `riff update` takes a 500 or a 429 of
/// Cloud Run as an outage, not as a server with no build
/// (01M56WNP3TEGH98B9EAM4H62P4). So `riff update` does not install the
/// Latest release in place of the release of the server.
#[tokio::test(flavor = "multi_thread")]
async fn the_probe_takes_a_front_end_error_as_an_outage() {
    use axum::http::StatusCode;
    for status in [
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::TOO_MANY_REQUESTS,
    ] {
        let server = behind_a_front_end(2, status).await;
        let url = &*server;
        let api = riff_client::api::Api::new(url);
        let error = api.probe(Duration::from_secs(3)).await.unwrap_err();
        let outage = error.downcast_ref::<riff_client::api::Outage>();
        assert!(outage.is_some_and(|o| o.status == status), "{error:#}");
        assert!(riff_client::api::passes(&error), "{error:#}");
        let error = riff::lifecycle::server_release(url).await.unwrap_err();
        assert!(
            format!("{error:#}").contains("riff update --tag"),
            "{error:#}"
        );
        // The next call gets riff-server.
        let probe = api.probe(Duration::from_secs(3)).await.unwrap();
        assert!(probe.build.is_some(), "{status}: {probe:?}");
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
        let api = riff_client::api::Api::new(&serve(router).await);
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
