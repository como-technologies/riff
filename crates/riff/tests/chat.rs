//! `riff chat` over real HTTP (01M3NB5MY93KV9RKZGGSMZW00D,
//! 01M3NB5N0D99JB5CE6RB4VEYPF): two people chat, a line with `@lead`
//! wakes a lead, a line with no `@` wakes no session, the answer of a
//! lead shows in each client, and a person who is not a member cannot
//! read or post the chat. `/me` sends an action line
//! (01M3NJD37CNQX580YC24S7K6ES). In a pseudo-terminal, the chat has a
//! prompt line, and your line shows once (01M3NJD39JVJHY5G71CD79JBY3).

use std::io::{Read, Write};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::StreamExt;
use isolated::Isolated;
use riff::api::Api;
use riff_core::name::SessionUri;
use riff_core::wire::Kind;
use riff_server::Service;
use riff_server::auth::Config;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};

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
    stderr: Lines<BufReader<ChildStderr>>,
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
            stderr,
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
        self.shows_within(text, WAIT).await
    }

    /// The next line of stdout that contains `text`, within `wait`.
    async fn shows_within(&mut self, text: &str, wait: Duration) -> String {
        let deadline = Instant::now() + wait;
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

/// A chat line of a person: `HH:MM <USER@HOST> BODY`.
fn is_line(line: &str, from: &str, body: &str) -> bool {
    is_shown(line, &format!("<{from}> {body}"))
}

/// A line of the chat: `HH:MM REST`.
fn is_shown(line: &str, rest: &str) -> bool {
    let (time, after) = line.split_at(5.min(line.len()));
    time.chars().enumerate().all(|(i, c)| match i {
        2 => c == ':',
        _ => c.is_ascii_digit(),
    }) && after == format!(" {rest}")
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
    // The lead line looks different from a person line, also with no
    // color.
    for client in [&mut mike, &mut brett] {
        let line = client.shows("yes, take #14").await;
        assert!(is_shown(&line, "[mike's lead] yes, take #14"), "{line:?}");
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
    assert!(api.tail(&eve, &chat).await.is_err());

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
    assert!(part.contains(PROMPT.trim_end()), "{part}");
    assert!(part.contains("<mike@thelio>"), "{part}");
    assert!(part.contains("[brett's lead]"), "{part}");
    let action = part
        .find("\n### Send an action with /me\n")
        .expect("the /me how-to has its own heading");
    let action = &part[action..];
    assert!(action.contains("```text\n/me waves\n```"), "{action}");
    assert!(action.contains("* mike@thelio waves"), "{action}");
    assert!(action.contains("//"), "{action}");
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

impl Client {
    /// The next line of stderr that contains `text`.
    async fn warns(&mut self, text: &str) -> String {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = tokio::time::timeout(left, self.stderr.next_line())
                .await
                .unwrap_or_else(|_| panic!("no stderr line with {text:?} in time"))
                .unwrap()
                .expect("riff chat runs");
            if line.contains(text) {
                return line;
            }
        }
    }
}

/// `riff tail chat --color never` as `user`, and its stdout.
fn tail(api: &Api, user: &str, dir: &std::path::Path) -> (Child, Lines<BufReader<ChildStdout>>) {
    let mut child = Isolated::shared()
        .tokio_riff()
        .args(["tail", "chat", "--color", "never"])
        .current_dir(dir)
        .env("RIFF_SERVER", api.base())
        .env("RIFF_USER", user)
        .env("RIFF_HOST", "wren")
        .env("RIFF_HOME", dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    (child, stdout)
}

#[tokio::test]
async fn me_sends_an_action_line_that_shows_in_each_client_and_in_tail() {
    let api = start_server().await;
    let mut brett = Client::start(&api, "brett", "heron", &["--color", "never"]).await;
    let mut mike = Client::start(&api, "mike", "thelio", &[]).await;
    let dir = tempfile::tempdir().unwrap();
    let (_tail, mut tailed) = tail(&api, "ann", dir.path());
    // riff tail shows only the new messages: give it time to connect.
    tokio::time::sleep(Duration::from_millis(500)).await;

    brett.say("/me waves").await;
    for client in [&mut mike, &mut brett] {
        let line = client.shows("waves").await;
        assert!(is_shown(&line, "* brett@heron waves"), "{line:?}");
        assert!(!line.contains('\x1b'), "{line:?}");
    }
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = tokio::time::timeout(left, tailed.next_line())
            .await
            .expect("riff tail shows the action in time")
            .unwrap()
            .expect("riff tail runs");
        if line.contains("waves") {
            assert_eq!(line.trim(), "* brett@heron waves");
            break;
        }
    }

    // The read tool of a session shows it as plain text.
    let thread = riff::chat::thread();
    let read = api.read(&lead_of("ann"), &thread, true).await.unwrap();
    let action = read.iter().find(|c| c.message.body == "/me waves").unwrap();
    let shown = riff::text::message(action, &thread);
    assert!(shown.ends_with(": * brett@heron waves"), "{shown}");

    // On the wire, an action is a plain message: no new kind and no new
    // field. So a riff of the release before reads it, and shows the
    // body `/me waves`.
    let wire = serde_json::to_value(&action.message).unwrap();
    let mut fields: Vec<&str> = wire
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["at_ms", "body", "from", "seq", "to"], "{wire}");
    assert_eq!(wire["body"], "/me waves");
}

#[tokio::test]
async fn me_with_at_lead_wakes_the_lead_of_the_sender() {
    let api = start_server().await;
    let lead = lead_of("brett");
    let mut wakes = Box::pin(api.watch(&lead).await.unwrap());
    let mut brett = Client::start(&api, "brett", "heron", &[]).await;

    brett.say("/me asks @lead to look").await;
    let wake = tokio::time::timeout(WAIT, wakes.next())
        .await
        .expect("the lead of brett wakes")
        .unwrap()
        .unwrap();
    assert_eq!(wake.thread, riff::chat::thread());
    assert_eq!(wake.kind, Kind::Message);
}

#[tokio::test]
async fn an_unknown_command_is_not_sent_and_a_double_slash_sends_a_slash() {
    let api = start_server().await;
    let mut mike = Client::start(&api, "mike", "thelio", &[]).await;

    mike.say("/foo bar").await;
    let warned = mike.warns("unknown command").await;
    assert_eq!(warned, riff::chat::unknown("foo"));
    mike.say("//foo bar").await;
    let line = mike.shows("foo bar").await;
    assert!(is_line(&line, "mike@thelio", "/foo bar"), "{line:?}");

    // The chat holds only the line with //.
    let read = api
        .read(&lead_of("mike"), &riff::chat::thread(), true)
        .await
        .unwrap();
    let bodies: Vec<&str> = read.iter().map(|c| c.message.body.as_str()).collect();
    assert_eq!(bodies, ["/foo bar"]);
}

const PROMPT: &str = riff::chat::PROMPT;

/// A running `riff chat` in a pseudo-terminal, and its screen.
struct Tty {
    _child: Child,
    master: std::fs::File,
    screen: Arc<Mutex<vt100::Parser>>,
    _dir: tempfile::TempDir,
}

impl Tty {
    const ROWS: u16 = 24;
    const COLUMNS: u16 = 80;

    /// Starts `riff chat` as `user` in a new pseudo-terminal, and waits
    /// for its prompt.
    async fn start(api: &Api, user: &str, host: &str) -> Tty {
        Self::start_with(api, user, host, Isolated::shared().tokio_riff()).await
    }

    /// [`Tty::start`] with `riff`, a command of a riff binary.
    async fn start_with(
        api: &Api,
        user: &str,
        host: &str,
        mut riff: tokio::process::Command,
    ) -> Tty {
        let size = nix::pty::Winsize {
            ws_row: Self::ROWS,
            ws_col: Self::COLUMNS,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = nix::pty::openpty(Some(&size), None).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let slave = std::fs::File::from(pty.slave);
        let child = riff
            .arg("chat")
            .current_dir(dir.path())
            .env("RIFF_SERVER", api.base())
            .env("RIFF_USER", user)
            .env("RIFF_HOST", host)
            .env("RIFF_HOME", dir.path())
            .env("TERM", "xterm")
            .env_remove("NO_COLOR")
            .env_remove("CLICOLOR_FORCE")
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let master = std::fs::File::from(pty.master);
        let screen = Arc::new(Mutex::new(vt100::Parser::new(Self::ROWS, Self::COLUMNS, 0)));
        let mut reader = master.try_clone().unwrap();
        let parser = Arc::clone(&screen);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            // The read fails when the chat exits.
            while let Ok(n @ 1..) = reader.read(&mut buf) {
                parser.lock().unwrap().process(&buf[..n]);
            }
        });
        let tty = Tty {
            _child: child,
            master,
            screen,
            _dir: dir,
        };
        tty.shows(|rows| rows.last().is_some_and(|row| row == PROMPT.trim_end()))
            .await;
        tty
    }

    /// Types `keys` one at a time, as a person does. The line editor
    /// with an external printer (rustyline 18) reads the next keys of
    /// one write only at the next key.
    async fn keys(&mut self, keys: &str) {
        for key in keys.chars() {
            self.master
                .write_all(key.encode_utf8(&mut [0; 4]).as_bytes())
                .unwrap();
            self.master.flush().unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// The rows of the screen, with no empty rows at the end, once
    /// `test` holds for them.
    async fn shows(&self, test: impl Fn(&[String]) -> bool) -> Vec<String> {
        let deadline = Instant::now() + WAIT;
        loop {
            let rows = self.rows();
            if test(&rows) {
                return rows;
            }
            assert!(Instant::now() < deadline, "the screen in time: {rows:#?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn rows(&self) -> Vec<String> {
        let parser = self.screen.lock().unwrap();
        let mut rows: Vec<String> = parser
            .screen()
            .rows(0, Self::COLUMNS)
            .map(|row| row.trim_end().to_owned())
            .collect();
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows
    }
}

#[tokio::test]
async fn in_a_terminal_your_line_shows_once_and_the_prompt_comes_back() {
    let api = start_server().await;
    let mut mike = Tty::start(&api, "mike", "thelio").await;

    mike.keys("hello all\r").await;
    let rows = mike
        .shows(|rows| {
            rows.iter()
                .any(|row| row.contains("<mike@thelio> hello all"))
                && rows.last().is_some_and(|row| row == PROMPT.trim_end())
        })
        .await;
    let seen = rows.iter().filter(|row| row.contains("hello all")).count();
    assert_eq!(seen, 1, "{rows:#?}");
    assert!(
        rows.iter()
            .any(|row| is_line(row, "mike@thelio", "hello all")),
        "{rows:#?}"
    );
    // The start line comes first.
    assert!(
        rows[0].starts_with("riff: chat as mike@thelio."),
        "{rows:#?}"
    );
}

#[tokio::test]
async fn in_a_terminal_a_new_line_prints_above_what_you_type() {
    let api = start_server().await;
    let mut mike = Tty::start(&api, "mike", "thelio").await;
    let mut brett = Client::start(&api, "brett", "heron", &[]).await;

    mike.keys("half a li").await;
    mike.shows(|rows| rows.last().is_some_and(|row| row.ends_with("> half a li")))
        .await;
    brett.say("a line from brett").await;
    let rows = mike
        .shows(|rows| rows.iter().any(|row| row.contains("a line from brett")))
        .await;
    let n = rows.len();
    assert!(
        is_line(&rows[n - 2], "brett@heron", "a line from brett"),
        "{rows:#?}"
    );
    assert_eq!(rows[n - 1], format!("{PROMPT}half a li"), "{rows:#?}");

    // The typed text is still there: Enter sends all of it.
    mike.keys("ne\r").await;
    let line = brett.shows("half a line").await;
    assert!(is_line(&line, "mike@thelio", "half a line"), "{line:?}");
}

/// Puts a copy of `from` at `to` as `cargo install` does: a new file
/// beside it, then a rename. A child process copies, so that no fork of
/// a parallel test holds a write fd of the file.
fn install(from: &std::path::Path, to: &std::path::Path) {
    let stage = to.with_extension("stage");
    let out = std::process::Command::new("cp")
        .arg(from)
        .arg(&stage)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    std::fs::rename(&stage, to).unwrap();
}

/// A chat in a terminal runs a new binary in place, and goes on: the
/// lines from before stay, each new line shows once, and the prompt
/// comes back (01M3NT6WXGCNKW3EQ7MBJDQTR4).
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_runs_the_new_binary_and_shows_the_next_line_once() {
    let api = start_server().await;
    let bin = tempfile::tempdir().unwrap();
    let binary = bin.path().join("riff");
    let riff = Isolated::shared().riff_path();
    install(&riff, &binary);
    let command = tokio::process::Command::from(Isolated::shared().command(&binary));
    let mut mike = Tty::start_with(&api, "mike", "thelio", command).await;
    let mut brett = Client::start(&api, "brett", "heron", &[]).await;
    brett.say("before the update").await;
    mike.shows(|rows| rows.iter().any(|row| row.contains("before the update")))
        .await;

    install(&riff, &binary);
    mike.shows(|rows| rows.iter().any(|row| row.contains("a new riff is on disk")))
        .await;
    brett.say("after the update").await;
    let rows = mike
        .shows(|rows| {
            rows.iter().any(|row| row.contains("after the update"))
                && rows.last().is_some_and(|row| row == PROMPT.trim_end())
        })
        .await;
    for line in ["before the update", "after the update"] {
        let seen = rows.iter().filter(|row| row.contains(line)).count();
        assert_eq!(seen, 1, "{line}: {rows:#?}");
    }
    let start = rows
        .iter()
        .filter(|row| row.starts_with("riff: chat as"))
        .count();
    assert_eq!(start, 1, "{rows:#?}");
    let is_date = |row: &String| row.len() == 10 && row.as_bytes()[4] == b'-';
    assert_eq!(
        rows.iter().filter(|row| is_date(row)).count(),
        1,
        "{rows:#?}"
    );

    // The new chat reads what the person types.
    mike.keys("hello from the new riff\r").await;
    let line = brett.shows("hello from the new riff").await;
    assert!(
        is_line(&line, "mike@thelio", "hello from the new riff"),
        "{line:?}"
    );
}

/// A TCP proxy in front of a riff server. It can cut each connection,
/// as Cloud Run does at the end of a long poll, and refuse new ones for
/// a time.
struct Proxy {
    url: String,
    open: Arc<std::sync::atomic::AtomicBool>,
    cut: tokio::sync::watch::Sender<u64>,
}

impl Proxy {
    async fn start(upstream: &str) -> Proxy {
        let upstream = upstream.trim_start_matches("http://").to_owned();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let open = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (cut, cuts) = tokio::sync::watch::channel(0);
        let is_open = Arc::clone(&open);
        tokio::spawn(async move {
            while let Ok((mut client, _)) = listener.accept().await {
                if !is_open.load(std::sync::atomic::Ordering::SeqCst) {
                    continue;
                }
                let upstream = upstream.clone();
                let mut cuts = cuts.clone();
                // Only a later cut ends this connection.
                cuts.borrow_and_update();
                tokio::spawn(async move {
                    let Ok(mut server) = tokio::net::TcpStream::connect(upstream).await else {
                        return;
                    };
                    tokio::select! {
                        _ = tokio::io::copy_bidirectional(&mut client, &mut server) => {}
                        _ = cuts.changed() => {}
                    }
                });
            }
        });
        Proxy { url, open, cut }
    }

    /// Cuts each connection. With `open` false, it refuses each new
    /// connection until [`Proxy::open`].
    fn cut(&self, open: bool) {
        self.open.store(open, std::sync::atomic::Ordering::SeqCst);
        self.cut.send_modify(|n| *n += 1);
    }

    fn open(&self) {
        self.open.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// A cut of the stream shows no error, and the running chat shows each
/// line: one posted while it could not connect, and one after, once
/// each (01M3NK7VHXB0PAR8VH8GQQA06K, 01M3NK7VM1J5DDB0PECNZ28P4E).
#[tokio::test]
async fn a_cut_stream_shows_no_error_and_loses_no_line() {
    let api = start_server().await;
    let proxy = Proxy::start(api.base()).await;
    let behind = Api::new(&proxy.url);
    let mut mike = Client::start(&behind, "mike", "thelio", &[]).await;
    let lead = lead_of("mike");
    let chat = riff::chat::thread();
    let post = |body: &'static str| {
        let (api, lead, chat) = (&api, &lead, &chat);
        async move {
            api.post(lead, Some(chat), &[], body, Kind::Message)
                .await
                .unwrap();
        }
    };
    post("before the cut").await;
    mike.shows("before the cut").await;

    // The server ends the long poll: the chat connects again at once.
    proxy.cut(true);
    post("just after the cut").await;
    mike.shows("just after the cut").await;

    // The chat cannot connect for a time. A line comes in the gap.
    proxy.cut(false);
    post("in the gap").await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    proxy.open();
    post("after the gap").await;
    let gap = mike.shows("in the gap").await;
    let after = mike.shows("after the gap").await;
    assert!(gap.ends_with("] in the gap"), "{gap:?}");
    assert!(after.ends_with("] after the gap"), "{after:?}");

    // Each line shows once: the next line is the one of mike.
    mike.say("the last line").await;
    let next = mike.shows("").await;
    assert!(next.ends_with("the last line"), "{next:?}");

    // stderr has one short line for the gap, and no error.
    mike.say("/quit").await;
    let status = tokio::time::timeout(WAIT, mike.child.wait()).await;
    assert!(status.unwrap().unwrap().success());
    let mut warned = Vec::new();
    while let Ok(Some(line)) = mike.stderr.next_line().await {
        warned.push(line);
    }
    assert_eq!(warned, [riff::api::RECONNECTING, riff::api::BACK]);
}

/// `riff tail` connects again at once after a cut, with no error line
/// (01M3NK7VHXB0PAR8VH8GQQA06K).
#[tokio::test]
async fn a_cut_stream_shows_no_error_in_tail() {
    let api = start_server().await;
    let proxy = Proxy::start(api.base()).await;
    let dir = tempfile::tempdir().unwrap();
    let mut child = Isolated::shared()
        .tokio_riff()
        .args(["tail", "chat", "--color", "never"])
        .current_dir(dir.path())
        .env("RIFF_SERVER", &proxy.url)
        .env("RIFF_USER", "ann")
        .env("RIFF_HOST", "wren")
        .env("RIFF_HOME", dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut err = BufReader::new(child.stderr.take().unwrap()).lines();
    let started = tokio::time::timeout(WAIT, err.next_line()).await.unwrap();
    assert!(
        started
            .unwrap()
            .unwrap()
            .contains("showing new messages in chat")
    );
    let (lead, chat) = (lead_of("mike"), riff::chat::thread());
    for body in ["before the cut", "after the cut"] {
        // Give the tail time to connect: it shows only new messages.
        tokio::time::sleep(Duration::from_millis(500)).await;
        api.post(&lead, Some(&chat), &[], body, Kind::Message)
            .await
            .unwrap();
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = tokio::time::timeout(left, out.next_line())
                .await
                .unwrap_or_else(|_| panic!("riff tail shows {body:?} in time"))
                .unwrap()
                .unwrap();
            if line.contains(body) {
                break;
            }
        }
        proxy.cut(true);
    }
    child.kill().await.unwrap();
    let mut warned = Vec::new();
    while let Ok(Some(line)) = err.next_line().await {
        warned.push(line);
    }
    assert!(warned.is_empty(), "{warned:?}");
}

/// A riff server behind a front end. While `down` is true, the front
/// end answers each call with 502 and no build header, as Cloud Run
/// does while it moves an instance (01M3QCMJ9F1GRTRRSB4AW9TC3D).
async fn behind_a_front_end(down: Arc<std::sync::atomic::AtomicBool>) -> String {
    let router = riff_server::router().layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let down = down.load(std::sync::atomic::Ordering::SeqCst);
            async move {
                if down {
                    axum::response::IntoResponse::into_response(axum::http::StatusCode::BAD_GATEWAY)
                } else {
                    next.run(request).await
                }
            }
        },
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

/// A 502 of the front end with no build is no version error: the chat
/// starts, and after a cut it connects again and goes on, with no error
/// on stderr (01M3QCMJ9F1GRTRRSB4AW9TC3D).
#[tokio::test]
async fn a_front_end_error_with_no_build_is_no_version_error() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let down = Arc::new(AtomicBool::new(true));
    let front = behind_a_front_end(Arc::clone(&down)).await;
    let proxy = Proxy::start(&front).await;
    let behind = Api::new(&proxy.url);
    let direct = Api::new(&front);

    // The front end is down at the start: the chat waits and starts.
    let up = {
        let down = Arc::clone(&down);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(800)).await;
            down.store(false, Ordering::SeqCst);
        })
    };
    let mut mike = Client::start(&behind, "mike", "thelio", &[]).await;
    up.await.unwrap();
    let (lead, chat) = (lead_of("mike"), riff::chat::thread());
    direct
        .post(&lead, Some(&chat), &[], "before the outage", Kind::Message)
        .await
        .unwrap();
    mike.shows("before the outage").await;

    // The stream ends while the front end is down.
    down.store(true, Ordering::SeqCst);
    proxy.cut(true);
    tokio::time::sleep(Duration::from_millis(800)).await;
    down.store(false, Ordering::SeqCst);
    direct
        .post(&lead, Some(&chat), &[], "after the outage", Kind::Message)
        .await
        .unwrap();
    // A dead connection in the pool can add one wait of `follow`.
    mike.shows_within("after the outage", 3 * WAIT).await;

    mike.say("/quit").await;
    let status = tokio::time::timeout(WAIT, mike.child.wait()).await;
    assert!(status.unwrap().unwrap().success());
    let mut warned = Vec::new();
    while let Ok(Some(line)) = mike.stderr.next_line().await {
        warned.push(line);
    }
    // At most the short lines of a cut: no version error.
    assert!(
        warned
            .iter()
            .all(|l| l == riff::api::RECONNECTING || l == riff::api::BACK),
        "{warned:?}"
    );
}
