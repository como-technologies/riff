//! `riff chat` over real HTTP (01M3NB5MY93KV9RKZGGSMZW00D,
//! 01M3NB5N0D99JB5CE6RB4VEYPF): two people chat, a line with `@lead`
//! wakes a lead, a line with no `@` wakes no session, the answer of a
//! lead shows in each client, and a person who is not a member cannot
//! read or post the chat.

use std::process::Stdio;
use std::time::{Duration, Instant};

use futures::StreamExt;
use isolated::Isolated;
use riff::api::Api;
use riff_core::name::SessionUri;
use riff_core::wire::Kind;
use riff_server::Service;
use riff_server::auth::Config;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};

const WAIT: Duration = Duration::from_secs(10);

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// A running `riff chat` of one person.
struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    _dir: tempfile::TempDir,
}

impl Client {
    /// Starts `riff chat ARGS` as `user` on `host`, and waits until it
    /// is ready: it prints its hint on stderr.
    async fn start(api: &Api, user: &str, host: &str, args: &[&str]) -> Client {
        let dir = tempfile::tempdir().unwrap();
        let mut child = Isolated::shared()
            .tokio_riff()
            .arg("chat")
            .args(args)
            .current_dir(dir.path())
            .env("RIFF_SERVER", api.base())
            .env("RIFF_USER", user)
            .env("RIFF_HOST", host)
            .env("RIFF_HOME", dir.path())
            .env_remove("NO_COLOR")
            .env_remove("CLICOLOR_FORCE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
        let hint = tokio::time::timeout(WAIT, stderr.next_line())
            .await
            .expect("riff chat is ready in time")
            .unwrap()
            .expect("riff chat prints its hint");
        assert!(hint.contains(&format!("chat as {user}@{host}")), "{hint}");
        Client {
            stdin: child.stdin.take().unwrap(),
            stdout: BufReader::new(child.stdout.take().unwrap()).lines(),
            child,
            _dir: dir,
        }
    }

    async fn say(&mut self, line: &str) {
        self.stdin
            .write_all(format!("{line}\n").as_bytes())
            .await
            .unwrap();
        self.stdin.flush().await.unwrap();
    }

    /// The next line of stdout that contains `text`.
    async fn shows(&mut self, text: &str) -> String {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = tokio::time::timeout(left, self.stdout.next_line())
                .await
                .unwrap_or_else(|_| panic!("no line with {text:?} in time"))
                .unwrap()
                .expect("riff chat runs");
            if line.contains(text) {
                return line;
            }
        }
    }
}

fn lead_of(user: &str) -> SessionUri {
    format!("riff://{user}@thelio/como-technologies/riff?session={user}-lead")
        .parse()
        .unwrap()
}

/// A chat line: `HH:MM USER@HOST  BODY`.
fn is_line(line: &str, from: &str, body: &str) -> bool {
    let (time, rest) = line.split_at(5.min(line.len()));
    time.chars().enumerate().all(|(i, c)| match i {
        2 => c == ':',
        _ => c.is_ascii_digit(),
    }) && rest == format!(" {from}  {body}")
}

#[tokio::test]
async fn a_line_of_one_person_shows_at_the_other() {
    let api = start_server().await;
    let mut mike = Client::start(&api, "mike", "thelio", &[]).await;
    let mut brett = Client::start(&api, "brett", "heron", &[]).await;

    mike.say("hello brett").await;
    let line = brett.shows("hello brett").await;
    assert!(is_line(&line, "mike@thelio", "hello brett"), "{line:?}");
    let own = mike.shows("hello brett").await;
    assert!(is_line(&own, "mike@thelio", "hello brett"), "{own:?}");

    // A later client shows the history first.
    let mut late = Client::start(&api, "ann", "wren", &[]).await;
    let line = late.shows("hello brett").await;
    assert!(is_line(&line, "mike@thelio", "hello brett"), "{line:?}");

    mike.say("/quit").await;
    let status = tokio::time::timeout(WAIT, mike.child.wait()).await;
    assert!(status.unwrap().unwrap().success());
}

#[tokio::test]
async fn at_lead_wakes_a_lead_and_its_answer_shows_in_each_client() {
    let api = start_server().await;
    let (mike_lead, brett_lead) = (lead_of("mike"), lead_of("brett"));
    let mut mike_wakes = Box::pin(api.watch(&mike_lead).await.unwrap());
    let mut brett_wakes = Box::pin(api.watch(&brett_lead).await.unwrap());
    let mut mike = Client::start(&api, "mike", "thelio", &[]).await;
    let mut brett = Client::start(&api, "brett", "heron", &[]).await;

    // @lead wakes the lead of the sender.
    mike.say("@lead is #12 done?").await;
    let wake = tokio::time::timeout(WAIT, mike_wakes.next())
        .await
        .expect("the lead of mike wakes")
        .unwrap()
        .unwrap();
    assert_eq!(wake.thread, riff::chat::thread());
    assert_eq!(wake.from.who().user(), "mike");

    // @USER wakes the lead of USER.
    brett.say("@mike: can I take #14?").await;
    let wake = tokio::time::timeout(WAIT, mike_wakes.next())
        .await
        .expect("the lead of mike wakes")
        .unwrap()
        .unwrap();
    assert_eq!(wake.from.who().user(), "brett");

    // The lead reads the line, and answers in the chat.
    let read = api
        .read(&mike_lead, &riff::chat::thread(), false)
        .await
        .unwrap();
    assert!(
        read.iter()
            .any(|c| c.message.body == "@mike: can I take #14?")
    );
    api.post(
        &mike_lead,
        Some(&riff::chat::thread()),
        &[],
        "yes, take #14",
        Kind::Message,
    )
    .await
    .unwrap();
    for client in [&mut mike, &mut brett] {
        let line = client.shows("yes, take #14").await;
        assert!(line.contains(" mike@thelio"), "{line:?}");
    }

    // Neither line named brett: his lead stays asleep.
    let none = tokio::time::timeout(Duration::from_millis(500), brett_wakes.next()).await;
    assert!(none.is_err(), "the lead of brett woke: {none:?}");
}

#[tokio::test]
async fn a_line_with_no_at_wakes_no_session() {
    let api = start_server().await;
    let lead = lead_of("mike");
    let mut wakes = Box::pin(api.watch(&lead).await.unwrap());
    let mut brett = Client::start(&api, "brett", "heron", &[]).await;

    brett.say("mail mike@thelio about the lead").await;
    brett.shows("mail mike@thelio").await;
    let none = tokio::time::timeout(Duration::from_millis(500), wakes.next()).await;
    assert!(none.is_err(), "a session woke: {none:?}");
}

#[tokio::test]
async fn never_and_a_pipe_print_no_escape_codes() {
    let api = start_server().await;
    for args in [&[][..], &["--color", "never"][..]] {
        let mut mike = Client::start(&api, "mike", "thelio", args).await;
        mike.say("plain \x1b]0;owned\x07text").await;
        let line = mike.shows("plain").await;
        assert!(!line.contains('\x1b'), "{args:?}: {line:?}");
        assert!(line.ends_with("plain text"), "{line:?}");
    }
    let mut always = Client::start(&api, "mike", "thelio", &["--color", "always"]).await;
    always.say("in color").await;
    assert!(always.shows("in color").await.contains("\x1b["));
}

#[tokio::test]
async fn a_person_who_is_not_a_member_cannot_read_or_post_the_chat() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = Service::new(Config {
        require_sign_in: true,
        ..Config::new(&url)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let api = Api::new(&url);

    // The first sign-in makes the owner. Then an email that is not a
    // member gets no sign-in.
    service
        .tokens()
        .admit("ada@gmail.com", false, &[], "jkt", Instant::now())
        .unwrap();
    let error = service
        .tokens()
        .admit("eve@evil.example", false, &[], "jkt", Instant::now())
        .unwrap_err();
    assert!(error.to_string().contains("not a member"), "{error}");

    // With no sign-in, the chat refuses each call.
    let eve: SessionUri = "riff://eve@evil".parse().unwrap();
    let chat = riff::chat::thread();
    assert!(api.join(&eve, &chat).await.is_err());
    assert!(api.read(&eve, &chat, true).await.is_err());
    assert!(
        api.post(&eve, Some(&chat), &[], "hi", Kind::Message)
            .await
            .is_err()
    );
    assert!(api.tail(&chat).await.is_err());

    // `riff chat` says so, and stops.
    let dir = tempfile::tempdir().unwrap();
    let out = Isolated::shared()
        .tokio_riff()
        .arg("chat")
        .current_dir(dir.path())
        .env("RIFF_SERVER", &url)
        .env("RIFF_USER", "eve")
        .env("RIFF_HOST", "evil")
        .env("RIFF_HOME", dir.path())
        .stdin(Stdio::null())
        .output()
        .await
        .unwrap();
    assert!(!out.status.success());
    assert!(
        out.stdout.is_empty(),
        "{:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// How It Works has the how-to: its own heading, and real commands in
/// `sh` blocks.
#[test]
fn the_book_shows_how_to_chat() {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let start = page
        .find("\n## Chat with the people of the riff\n")
        .expect("the how-to has its own heading");
    let part = &page[start + 1..];
    let part = &part[..part[3..].find("\n## ").map_or(part.len(), |end| end + 3)];
    assert!(part.contains("```sh\nriff chat\n```"), "{part}");
    assert!(part.contains("@lead"), "{part}");
    assert!(part.contains("riff chat --color never"), "{part}");
    Isolated::shared()
        .assert_riff()
        .args(["chat", "--color", "never", "--help"])
        .assert()
        .success();
    let help = Isolated::shared()
        .assert_riff()
        .arg("--help")
        .assert()
        .success();
    let help = String::from_utf8_lossy(&help.get_output().stdout).to_string();
    assert!(help.contains("  chat "), "{help}");
}
