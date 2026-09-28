//! `riff tail` shows each message as a block for people
//! (01M3JDCA6R894JG6SDJ2R7AFMN). `--color` and `NO_COLOR` control the
//! color through a pipe (01M3JDCA9070MY30AYHK3Y67EF). A body cannot
//! change the terminal (01M3JDCAB7K6QA58HDTN9BR1AH). A fake server gives
//! one message on each stream.

use isolated::Isolated;
use std::convert::Infallible;
use std::io::Read;
use std::process::Stdio;
use std::time::Duration;

use axum::response::sse::{Event, Sse};
use axum::routing::get;
use futures::Stream;
use riff_core::wire::{Kind, Message, Tailed};

/// A body with an escape sequence that sets the title of the terminal,
/// and a long line.
const BODY: &str = "hi\x1b]0;owned\x07 there\nword word word word word word word word word word \
word word word word word word word word word word word word";

async fn tail() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let message = Message {
        seq: 1,
        from: "riff://mike@pangolin/como-technologies/riff?session=a1"
            .parse()
            .unwrap(),
        to: vec!["claim=issue-6".parse().unwrap()],
        body: BODY.into(),
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
    // Keep the stream open, so that riff does not connect again.
    Sse::new(futures::StreamExt::chain(
        futures::stream::iter([Ok(event)]),
        futures::stream::pending(),
    ))
}

/// A fake server names the build of this `riff` in each reply, like
/// `riff-server` (01M3JEE7P46GWXR1BD4Q1TTSGN).
async fn stamp_build(mut response: axum::response::Response) -> axum::response::Response {
    response.headers_mut().insert(
        riff_core::build::HEADER,
        axum::http::HeaderValue::from_static(riff_core::build::VERSION),
    );
    response
}

async fn start_fake() -> String {
    let router = axum::Router::new()
        .route("/v1/tail", get(tail))
        .layer(axum::middleware::map_response(stamp_build));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

/// The stdout of `riff tail ARGS` through a pipe, for one second.
async fn tail_output(server: &str, args: &[&str], envs: &[(&str, &str)]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Isolated::shared().riff();
    cmd.arg("tail")
        .arg("como-technologies/riff")
        .args(args)
        .current_dir(dir.path())
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "brett")
        .env("RIFF_HOST", "heron")
        .env("RIFF_SESSION", "b2")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("RIFF_HOME", dir.path())
        .envs(envs.iter().copied())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    tokio::task::spawn_blocking(move || {
        let mut child = cmd.spawn().unwrap();
        std::thread::sleep(Duration::from_secs(1));
        child.kill().unwrap();
        let mut out = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut out)
            .unwrap();
        child.wait().unwrap();
        out
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn a_pipe_gets_no_color_unless_asked() {
    let server = start_fake().await;
    let auto = tail_output(&server, &[], &[]).await;
    assert!(auto.contains("mike@pangolin:riff (a1)"), "{auto}");
    assert!(!auto.contains('\x1b'), "{auto:?}");

    let always = tail_output(&server, &["--color", "always"], &[]).await;
    assert!(always.contains("\x1b["), "{always:?}");

    let never = tail_output(&server, &["--color", "never"], &[("CLICOLOR_FORCE", "1")]).await;
    assert!(!never.contains('\x1b'), "{never:?}");

    let no_color = tail_output(&server, &[], &[("NO_COLOR", "1")]).await;
    assert!(!no_color.contains('\x1b'), "{no_color:?}");
}

#[tokio::test]
async fn a_body_cannot_change_the_terminal() {
    let server = start_fake().await;
    for args in [&[][..], &["--color", "always"][..]] {
        let out = tail_output(&server, args, &[]).await;
        assert!(!out.contains("\x1b]"), "{out:?}");
        assert!(!out.contains('\x07'), "{out:?}");
        assert!(out.contains("hi there"), "{out:?}");
    }
}

#[tokio::test]
async fn a_long_body_wraps_with_the_indent() {
    let server = start_fake().await;
    let out = tail_output(&server, &[], &[]).await;
    let body: Vec<&str> = out.lines().filter(|l| l.contains("word")).collect();
    assert!(body.len() >= 2, "{out}");
    for line in body {
        assert!(line.starts_with("       word"), "{line:?}");
        // A pipe has no width: the default is 80 columns.
        assert!(line.chars().count() <= 80, "{line:?}");
    }
}

/// Each `riff tail` command in the `sh` blocks of How It Works is real.
#[test]
fn the_book_shows_real_tail_commands() {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let mut in_sh = false;
    let mut commands = Vec::new();
    for line in page.lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && line.starts_with("riff tail") {
            commands.push(line.split(['|', '>']).next().unwrap().trim().to_owned());
        }
    }
    assert!(
        commands.iter().any(|c| c == "riff tail --color never"),
        "{commands:?}"
    );
    for command in commands {
        Isolated::shared()
            .assert_riff()
            .args(command.split_whitespace().skip(1))
            .arg("--help")
            .assert()
            .success();
    }
}

/// How It Works says that only the lead gets the `riff tail` pane, and
/// shows how to watch the riff on another machine with a real command
/// (01M3JD390F49HZSKEJ3VACX0ZA).
#[test]
fn the_book_shows_how_to_watch_the_riff_on_another_machine() {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    assert!(page.contains("Only the lead gets the `riff tail` pane."));
    let start = page
        .find("\n### Watch the riff on another machine\n")
        .expect("the how-to has its own heading");
    let part = &page[start + 1..];
    let part = &part[..part[4..].find("\n#").map_or(part.len(), |end| end + 4)];
    assert!(
        part.contains("```sh\nriff tail como-technologies/riff\n```"),
        "{part}"
    );
    let help = Isolated::shared()
        .assert_riff()
        .args(["tail", "como-technologies/riff", "--help"])
        .assert()
        .success();
    let help = String::from_utf8_lossy(&help.get_output().stdout).into_owned();
    assert!(help.contains("[THREAD]"), "{help}");
}
