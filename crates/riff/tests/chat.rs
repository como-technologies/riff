//! `riff chat` over real HTTP (01M3NB5MY93KV9RKZGGSMZW00D,
//! 01M6Y1A5PTCVZ6GRJ0FXMART2T): two people chat, a line posts with the
//! names that it holds, `@riff` wakes the lead of the sender in riff, a
//! line with no name wakes no one, no line is refused for a name, the
//! answer of a lead shows in each client, and a person who is not a
//! member cannot read or post the chat. `/me` sends an action line
//! (01M3NJD37CNQX580YC24S7K6ES). In a pseudo-terminal, the chat has a
//! prompt line, and your line shows once (01M3NJD39JVJHY5G71CD79JBY3).

use std::io::{Read, Write};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use isolated::{Isolated, Span, in_time};
use riff_client::api::Api;
use riff_core::name::SessionUri;
use riff_core::wire::{BotFact, Kind};
use riff_server::Service;
use riff_server::auth::Config;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};

/// The bound of each wait. It is generous: it only ends a test that
/// hangs.
const WAIT: Duration = Duration::from_secs(30);

/// A riff-server with no sign-in, for the tests that go through the
/// [`Proxy`]: a token binds to the URL of the server (DPoP), so a signed
/// call through the proxy would fail.
async fn start_open() -> Api {
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
    async fn start(url: &str, user: &str, host: &str, args: &[&str]) -> Client {
        let dir = tempfile::tempdir().unwrap();
        crate::common::sign_in(url, dir.path(), user);
        let mut child = Isolated::shared()
            .tokio_riff()
            .arg("chat")
            .args(args)
            .current_dir(dir.path())
            .env("RIFF_SERVER", url)
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
        // A start that waits for the server can show the wait line first.
        let hint = in_time(WAIT, async {
            loop {
                let line = stderr.next_line().await.unwrap();
                match line {
                    Some(line) if line.contains(riff_client::link::WAITING) => continue,
                    line => break line,
                }
            }
        })
        .await
        .expect("riff chat is ready in time")
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
        in_time(wait, async {
            loop {
                let line = self
                    .stdout
                    .next_line()
                    .await
                    .unwrap()
                    .expect("riff chat runs");
                if line.contains(text) {
                    return line;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no line with {text:?} in time"))
    }
}

fn lead_of(user: &str) -> SessionUri {
    format!("riff://{user}@thelio/como-technologies/riff?session={user}-lead")
        .parse()
        .unwrap()
}

/// A chat line of a name: `HH:MM <NAME> BODY`.
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
    let server = crate::common::start_signed().await;
    let mut mike = Client::start(&server, "mike", "thelio", &[]).await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;

    mike.say("hello brett").await;
    let line = brett.shows("hello brett").await;
    assert!(is_line(&line, "mike", "hello brett"), "{line:?}");
    let own = mike.shows("hello brett").await;
    assert!(is_line(&own, "mike", "hello brett"), "{own:?}");

    // A later client shows the history first.
    let mut late = Client::start(&server, "bob", "wren", &[]).await;
    let line = late.shows("hello brett").await;
    assert!(is_line(&line, "mike", "hello brett"), "{line:?}");

    mike.say("/quit").await;
    let status = in_time(WAIT, mike.child.wait()).await;
    assert!(status.unwrap().unwrap().success());
}

/// The selectors of the chat line `body`, as riff-server keeps them, read
/// as `me`.
async fn to_of(api: &Api, me: &SessionUri, body: &str) -> Vec<String> {
    let read = api.read(me, &riff::chat::thread(), true).await.unwrap();
    let line = read
        .iter()
        .find(|c| c.message.body == body)
        .unwrap_or_else(|| panic!("no chat line {body:?}"));
    line.message.to.iter().map(ToString::to_string).collect()
}

/// True when no wake comes on `wakes` for a short time.
async fn quiet<S: futures::Stream + Unpin>(wakes: &mut S) -> bool {
    tokio::time::timeout(Duration::from_millis(500), wakes.next())
        .await
        .is_err()
}

#[tokio::test]
async fn at_a_project_wakes_the_lead_of_the_sender_and_its_answer_shows_in_each_client() {
    let server = crate::common::start_signed().await;
    let (mike_lead, brett_lead) = (lead_of("mike"), lead_of("brett"));
    // The person of each user names its lead. A client of one process
    // has one sign-in per server, so each call has its own client.
    let common = |user| crate::common::api(&server, user, None);
    let session = |lead| crate::common::api_of(&server, lead);
    session(&mike_lead).register(&mike_lead).await.unwrap();
    crate::common::name_lead(&common("mike"), &mike_lead).await;
    let mike_api = session(&mike_lead);
    let mut mike_wakes = Box::pin(mike_api.watch(&mike_lead).await.unwrap());
    session(&brett_lead).register(&brett_lead).await.unwrap();
    crate::common::name_lead(&common("brett"), &brett_lead).await;
    let brett_api = session(&brett_lead);
    let mut brett_wakes = Box::pin(brett_api.watch(&brett_lead).await.unwrap());
    let mut mike = Client::start(&server, "mike", "thelio", &[]).await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;

    // @riff wakes the lead of the sender in riff, and not the lead of
    // brett there.
    mike.say("@riff is #12 done?").await;
    let wake = in_time(WAIT, mike_wakes.next())
        .await
        .expect("the lead of mike wakes")
        .unwrap()
        .unwrap();
    assert_eq!(wake.thread, riff::chat::thread());
    assert_eq!(wake.from.who().user(), "mike");

    // @mike names the person mike: it wakes no lead.
    brett.say("@mike: can I take #14?").await;
    brett.shows("can I take #14?").await;
    assert!(quiet(&mut mike_wakes).await, "the lead of mike woke");

    // Each line posts with its names, and with no selector of riff.
    let to = |body| to_of(&mike_api, &mike_lead, body);
    assert_eq!(to("@riff is #12 done?").await, ["name=riff"]);
    assert_eq!(to("@mike: can I take #14?").await, ["name=mike"]);

    // The lead reads the lines, and answers in the chat.
    let read = session(&mike_lead)
        .read(&mike_lead, &riff::chat::thread(), true)
        .await
        .unwrap();
    assert!(
        read.iter()
            .any(|c| c.message.body == "@mike: can I take #14?")
    );
    session(&mike_lead)
        .post(
            &mike_lead,
            Some(&riff::chat::thread()),
            &[],
            "yes, take #14",
            Kind::Message,
        )
        .await
        .unwrap();
    // The line of the lead shows the name of its project, also with no
    // color.
    for client in [&mut mike, &mut brett] {
        let line = client.shows("yes, take #14").await;
        assert!(is_shown(&line, "<riff> yes, take #14"), "{line:?}");
    }

    // No line woke the lead of brett.
    assert!(quiet(&mut brett_wakes).await, "the lead of brett woke");
}

/// The lead of `user` in the repository `repo`, on one host.
fn lead_in(user: &str, repo: &str) -> SessionUri {
    let id = repo.replace('/', "-");
    format!("riff://{user}@thelio/{repo}?session={user}-{id}-lead")
        .parse()
        .unwrap()
}

/// The leads of `user` in each of `repos`: registered, and named by the
/// person. Each comes with a client of its own session.
async fn named_leads(
    server: &crate::common::Signed,
    user: &str,
    repos: &[&str],
) -> Vec<(SessionUri, Api)> {
    let mut leads = Vec::new();
    for repo in repos {
        let lead = lead_in(user, repo);
        let api = crate::common::api_of(server, &lead);
        api.register(&lead).await.unwrap();
        crate::common::name_lead(&server.api(user, None), &lead).await;
        leads.push((lead, api));
    }
    leads
}

/// #1786 (01M6Y1FB5MZNZ971CJRBWZ2F9X): each line shows its sender by the
/// name to type to reach it: a person as `<brett>`, a lead as the name of
/// its project, `<riff>`, and riff-server itself as `<riff-bot>`. The
/// host shows nowhere. On a clash, the project has the name `OWNER/REPO`
/// from the names of riff-server, which the chat reads at its start.
#[tokio::test]
async fn each_line_shows_its_sender_by_the_name_to_type() {
    let server = crate::common::start_signed().await;
    let repos = ["como-technologies/riff", "como-technologies/strata"];
    let leads = named_leads(&server, "mike", &repos).await;
    // A session of brett in acme/strata: two projects have the name
    // strata.
    let other = lead_in("brett", "acme/strata");
    let api = crate::common::api_of(&server, &other);
    api.register(&other).await.unwrap();
    let mut mike = Client::start(&server, "mike", "thelio", &[]).await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;

    brett.say("is #12 merged?").await;
    let line = mike.shows("is #12 merged?").await;
    assert!(is_line(&line, "brett", "is #12 merged?"), "{line:?}");
    let chat = riff::chat::thread();
    let answers = ["@brett yes, #12 is merged", "@brett strata is green"];
    for ((lead, api), body) in leads.iter().zip(answers) {
        let post = api.post(lead, Some(&chat), &[], body, Kind::Message);
        post.await.unwrap();
    }
    let line = mike.shows("#12 is merged").await;
    assert!(is_line(&line, "riff", answers[0]), "{line:?}");
    let line = mike.shows("strata is green").await;
    assert!(
        is_line(&line, "como-technologies/strata", answers[1]),
        "{line:?}"
    );

    // Only a line of riff-server itself shows as <riff-bot>
    // (01M6Y4ZCTSTVM971BDC1F7GJPK). riff-server writes the line that riff
    // writes for mike from its fact: the ask of a release names the owner
    // brett, and the note of the daemon names no one. A line that mike
    // types with the words of riff-bot shows as mike.
    let person: SessionUri = "riff://mike@thelio/como-technologies/riff".parse().unwrap();
    let api = server.api("mike", None);
    let waits = BotFact::ReleaseWaits {
        tag: "v3.3.0".into(),
        run: 9,
        reminder: false,
    };
    api.bot(&person, waits).await.unwrap();
    api.bot(&person, BotFact::LeadStarted).await.unwrap();
    let asked = "@brett riff release v3.3.0 waits for your approval: \
                 https://github.com/como-technologies/riff/actions/runs/9. In a terminal, run: \
                 riff release approve v3.3.0";
    let started = "riff started the lead of mike in como-technologies/riff on thelio again on \
                   the new riff, and its conversation goes on. Attach with: riff lead start, or \
                   l in the Sessions tab of riff.";
    for client in [&mut mike, &mut brett] {
        let line = client.shows("waits for your approval").await;
        assert!(is_line(&line, "riff-bot", asked), "{line:?}");
        let line = client.shows("riff started the lead").await;
        assert!(is_line(&line, "riff-bot", started), "{line:?}");
    }
    let typed = "@brett riff release v9.9.9 waits for your approval";
    mike.say(typed).await;
    let line = brett.shows("v9.9.9").await;
    assert!(is_line(&line, "mike", typed), "{line:?}");
    // brett, the owner, passes the role: riff-server posts the line.
    let mike_email = "mike@comotechnologies.io";
    let pass = server.api("brett", None);
    pass.pass_owner(mike_email).await.unwrap();
    let passed = riff_state::owner::passed_news("brett", mike_email, "brett@comotechnologies.io");
    for client in [&mut mike, &mut brett] {
        let line = client.shows("passed the owner role").await;
        assert!(is_line(&line, "riff-bot", &passed), "{line:?}");
    }

    // The line of mike shows at brett as <mike>.
    mike.say("thanks").await;
    let line = brett.shows("thanks").await;
    assert!(is_line(&line, "mike", "thanks"), "{line:?}");
    assert!(!line.contains("thelio"), "{line:?}");
}

#[tokio::test]
async fn the_name_of_a_project_wakes_only_the_lead_in_that_project() {
    let server = crate::common::start_signed().await;
    let leads = named_leads(
        &server,
        "mike",
        &["como-technologies/riff", "como-technologies/strata"],
    )
    .await;
    let mut wakes = Vec::new();
    for (lead, api) in &leads {
        wakes.push(Box::pin(api.watch(lead).await.unwrap()));
    }
    let [riff_wakes, strata_wakes] = &mut wakes[..] else {
        unreachable!()
    };
    let mut mike = Client::start(&server, "mike", "thelio", &[]).await;

    // The name after the slash names the project.
    mike.say("@strata is #12 done?").await;
    let wake = in_time(WAIT, strata_wakes.next())
        .await
        .expect("the lead in strata wakes")
        .unwrap()
        .unwrap();
    assert_eq!(wake.from.who().user(), "mike");
    assert!(quiet(&mut *riff_wakes).await, "the lead in riff woke");

    // OWNER/REPO names the project too.
    mike.say("@como-technologies/riff, and you?").await;
    in_time(WAIT, riff_wakes.next())
        .await
        .expect("the lead in riff wakes")
        .unwrap()
        .unwrap();
    assert!(quiet(&mut *strata_wakes).await, "the lead in strata woke");

    // The line of a lead shows the name of its project.
    let (strata, strata_api) = &leads[1];
    let chat = riff::chat::thread();
    let post = strata_api.post(strata, Some(&chat), &[], "yes, #12 is done", Kind::Message);
    post.await.unwrap();
    let line = mike.shows("yes, #12 is done").await;
    assert!(is_shown(&line, "<strata> yes, #12 is done"), "{line:?}");

    // A line with no name wakes no lead.
    mike.say("and a line with no name").await;
    mike.shows("and a line with no name").await;
    for wakes in [&mut *riff_wakes, &mut *strata_wakes] {
        assert!(
            quiet(&mut *wakes).await,
            "a lead woke for a line with no name"
        );
    }
}

#[tokio::test]
async fn no_line_is_refused_for_a_name() {
    let server = crate::common::start_signed().await;
    let leads = named_leads(
        &server,
        "mike",
        &["como-technologies/strata", "acme/strata"],
    )
    .await;
    let mut wakes = Vec::new();
    for (lead, api) in &leads {
        wakes.push(Box::pin(api.watch(lead).await.unwrap()));
    }
    let [como, acme] = &mut wakes[..] else {
        unreachable!()
    };
    let mut mike = Client::start(&server, "mike", "thelio", &[]).await;

    // Two projects have the name strata, so each one has the name
    // OWNER/REPO. A name that no one has goes too, with its name, and
    // wakes no one: the chat checks no name.
    for (line, name) in [
        ("@strata hello", "name=strata"),
        ("@nobody hello", "name=nobody"),
        ("@lead:riff hello", "name=lead"),
    ] {
        mike.say(line).await;
        let shown = mike.shows(line).await;
        assert!(is_line(&shown, "mike", line), "{shown:?}");
        let (lead, api) = &leads[0];
        assert_eq!(to_of(api, lead, line).await, [name], "{line}");
    }
    assert!(
        quiet(&mut *como).await,
        "the lead in como-technologies/strata woke"
    );
    assert!(quiet(&mut *acme).await, "the lead in acme/strata woke");

    // OWNER/REPO wakes the one lead.
    mike.say("@acme/strata hello").await;
    let wake = in_time(WAIT, acme.next())
        .await
        .expect("the lead in acme/strata wakes")
        .unwrap()
        .unwrap();
    assert_eq!(wake.from.who().user(), "mike");
    assert!(quiet(&mut *como).await, "the other lead woke");
}

#[tokio::test]
async fn a_line_with_no_name_wakes_no_one() {
    let server = crate::common::start_signed().await;
    let lead = lead_of("brett");
    let api = crate::common::api_of(&server, &lead);
    api.register(&lead).await.unwrap();
    crate::common::name_lead(&crate::common::api(&server, "brett", None), &lead).await;
    let api = crate::common::api_of(&server, &lead);
    let mut wakes = Box::pin(api.watch(&lead).await.unwrap());
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;

    let line = "is the running riff in the way of my release?";
    brett.say(line).await;
    brett.shows("my release").await;
    assert!(quiet(&mut wakes).await, "the lead of brett woke");
    assert!(to_of(&api, &lead, line).await.is_empty());
}

#[tokio::test]
async fn never_and_a_pipe_print_no_escape_codes() {
    let server = crate::common::start_signed().await;
    for args in [&[][..], &["--color", "never"][..]] {
        let mut mike = Client::start(&server, "mike", "thelio", args).await;
        mike.say("plain \x1b]0;owned\x07text").await;
        let line = mike.shows("plain").await;
        assert!(!line.contains('\x1b'), "{args:?}: {line:?}");
        assert!(line.ends_with("plain text"), "{line:?}");
    }
    let mut always = Client::start(&server, "mike", "thelio", &["--color", "always"]).await;
    always.say("in color").await;
    assert!(always.shows("in color").await.contains("\x1b["));
}

#[tokio::test]
async fn a_person_who_is_not_a_member_cannot_read_or_post_the_chat() {
    // This server has its own sign-in flow: it admits a member and refuses a stranger.
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
    service.admit("ada@gmail.com", false, "jkt").await.unwrap();
    let error = service
        .admit("eve@evil.example", false, "jkt")
        .await
        .unwrap_err();
    assert!(error.contains("not a member"), "{error}");

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
    assert!(part.contains("@riff"), "{part}");
    assert!(part.contains("riff chat --color never"), "{part}");
    assert!(part.contains(PROMPT.trim_end()), "{part}");
    assert!(part.contains("<mike>"), "{part}");
    assert!(part.contains("<riff>"), "{part}");
    assert!(part.contains("<riff-bot>"), "{part}");
    let ask = part
        .find("\n### Ask a lead in the chat\n")
        .expect("the how-to to ask a lead has its own heading");
    let ask = &part[ask + 1..];
    let ask = &ask[..ask[3..].find("\n### ").map_or(ask.len(), |end| end + 3)];
    // 01M6Y1A5PTCVZ6GRJ0FXMART2T: a project and a person by name, and
    // a line with no name.
    assert!(ask.contains("```text\n@riff is #12 done?\n"), "{ask}");
    assert!(ask.contains("\n@brett can I take #14?\n"), "{ask}");
    assert!(ask.contains("A line with no name wakes no one"), "{ask}");
    assert!(!ask.contains("@lead"), "{ask}");
    // 01M6Y8N8W5RE5H6MW3TNDH5BSY: when ↑ brings back a sent line.
    let again = part
        .find("\n### Send a line again\n")
        .expect("the how-to to send a line again has its own heading");
    let again = &part[again..];
    assert!(again.contains("```sh\nriff chat\n```"), "{again}");
    assert!(
        again.contains("On an empty line, `↑` brings back the last line"),
        "{again}"
    );
    assert!(
        part.contains("`↑` brings back a line that you sent only on an empty line"),
        "{part}"
    );
    let action = part
        .find("\n### Send an action with /me\n")
        .expect("the /me how-to has its own heading");
    let action = &part[action..];
    assert!(action.contains("```text\n/me waves\n```"), "{action}");
    assert!(action.contains("* mike waves"), "{action}");
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

/// How It Works has the how-tos of the list of `#`
/// (01M74CX2V9YAF0KFPJDRM4DA83): "Pick an issue with #" next to "Pick a
/// name with @", and "Pick an issue in the Chat tab" next to "Pick a
/// name in the Chat tab", each with its own heading and a copyable `sh`
/// block, and the row of `#` in the table of the keys of the Chat tab.
#[test]
fn the_book_shows_how_to_pick_an_issue() {
    let page = crate::book::page("how-it-works.md");
    // The text of the part under `heading`, up to the next heading of
    // its level or a higher one.
    let part = |heading: &str| {
        let start = page
            .find(&format!("\n{heading}\n"))
            .unwrap_or_else(|| panic!("{heading} has its own heading"));
        let level = heading.split(' ').next().unwrap().len();
        let rest = &page[start + heading.len() + 2..];
        let end = rest
            .match_indices("\n#")
            .find(|(at, _)| {
                let hashes = rest[at + 1..].chars().take_while(|c| *c == '#').count();
                hashes <= level && rest[at + 1 + hashes..].starts_with(' ')
            })
            .map_or(rest.len(), |(at, _)| at);
        (start, rest[..end].to_owned())
    };
    let (name, _) = part("### Pick a name with @");
    let (issue, text) = part("### Pick an issue with #");
    assert!(name < issue, "next to Pick a name with @");
    assert!(text.contains("```sh\nriff chat\n```"), "{text}");
    assert!(text.contains("[riff] > is #186\n  #1862  riff  "), "{text}");
    assert!(text.contains("Tab or Enter picks #1862."), "{text}");
    assert!(text.contains("`OWNER/REPO#N`"), "{text}");
    let (name, _) = part("#### Pick a name in the Chat tab");
    let (issue, text) = part("#### Pick an issue in the Chat tab");
    assert!(name < issue, "next to Pick a name in the Chat tab");
    assert!(text.contains("```sh\nriff\n```"), "{text}");
    assert!(text.contains("> is #186\n"), "{text}");
    let (_, tui) = part("### Chat in the TUI");
    assert!(
        tui.contains(
            "| `#` | Show the list of the open issues: see \"Pick an issue in the Chat tab\". |"
        ),
        "{tui}"
    );
}

/// How It Works has the how-to of a highlight: its own heading, each key
/// of `[chat]`, and a copyable `sh` block that riff reads. "Ask a lead in
/// the chat" says that a name is a highlight.
#[test]
fn the_book_shows_how_to_get_a_highlight() {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let section = |heading: &str| {
        let start = page
            .find(&format!("\n### {heading}\n"))
            .unwrap_or_else(|| panic!("the how-to {heading:?} has its own heading"));
        let part = &page[start + 1..];
        part[..part[3..].find("\n### ").map_or(part.len(), |end| end + 3)].to_owned()
    };
    let ask = section("Ask a lead in the chat");
    assert!(
        ask.contains("is also a highlight on the terminal of that person"),
        "{ask}"
    );
    let how = section("Get a highlight in the chat");
    for key in [
        "highlight = []",
        "bold = true",
        "bell = true",
        "title = false",
        "toast = false",
        "on_highlight = \"\"",
        "quiet = \"\"",
    ] {
        assert!(how.contains(&format!("\n{key} ")), "{key}: {how}");
    }
    for words in [
        "--color never",
        "NO_COLOR",
        "RIFF_SENDER",
        "RIFF_THREAD",
        "(2) riff chat",
    ] {
        assert!(how.contains(words), "{words}: {how}");
    }
    let sh = how.find("```sh\n").expect("a copyable sh block");
    let sh = &how[sh + 6..];
    let sh = &sh[..sh.find("```").unwrap()];
    assert!(sh.contains("~/.config/riff/config.toml"), "{sh}");
    assert!(sh.ends_with("riff chat\n"), "{sh}");
    // riff reads the table of the how-to.
    let table = sh
        .split_once("<<'EOF'\n")
        .and_then(|(_, rest)| rest.split_once("EOF\n"))
        .expect("the sh block writes a table")
        .0;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, table).unwrap();
    let chat = riff::settings::chat(&path).unwrap();
    assert_eq!(chat.words, ["release", "approve"]);
    assert!(chat.title && chat.toast && chat.hook.is_some() && chat.quiet.is_some());
}

impl Client {
    /// The next line of stderr that contains `text`.
    async fn warns(&mut self, text: &str) -> String {
        in_time(WAIT, async {
            loop {
                let line = self
                    .stderr
                    .next_line()
                    .await
                    .unwrap()
                    .expect("riff chat runs");
                if line.contains(text) {
                    return line;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no stderr line with {text:?} in time"))
    }
}

/// `riff tail chat --color never` as `user`, and its stdout.
fn tail(url: &str, user: &str, dir: &std::path::Path) -> (Child, Lines<BufReader<ChildStdout>>) {
    crate::common::sign_in(url, dir, user);
    let mut child = Isolated::shared()
        .tokio_riff()
        .args(["tail", "chat", "--color", "never"])
        .current_dir(dir)
        .env("RIFF_SERVER", url)
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

/// Waits until a `riff tail` of the chat is connected. It shows only
/// the new messages, so the test posts a line again and again, until
/// the tail shows one.
async fn connected(api: &Api, tailed: &mut Lines<BufReader<ChildStdout>>) {
    const PROBE: &str = "is the tail there";
    let (lead, chat) = (lead_of("mike"), riff::chat::thread());
    let span = Span::start();
    // The tail is there when it shows the last probe: then it saw each
    // probe, and no probe comes in a later break of a test.
    for n in 1.. {
        let probe = format!("{PROBE} {n}.");
        api.post(&lead, Some(&chat), &[], &probe, Kind::Message)
            .await
            .unwrap();
        while let Ok(line) =
            tokio::time::timeout(Duration::from_millis(500), tailed.next_line()).await
        {
            if line.unwrap().expect("riff tail runs").contains(&probe) {
                return;
            }
        }
        assert!(span.within(WAIT), "riff tail connects in time");
    }
}

#[tokio::test]
async fn me_sends_an_action_line_that_shows_in_each_client_and_in_tail() {
    let server = crate::common::start_signed().await;
    let api = crate::common::api_of(&server, &lead_of("mike"));
    let mut brett = Client::start(&server, "brett", "heron", &["--color", "never"]).await;
    let mut mike = Client::start(&server, "mike", "thelio", &[]).await;
    let dir = tempfile::tempdir().unwrap();
    let (_tail, mut tailed) = tail(&server, "bob", dir.path());
    connected(&api, &mut tailed).await;

    brett.say("/me waves").await;
    for client in [&mut mike, &mut brett] {
        let line = client.shows("waves").await;
        assert!(is_shown(&line, "* brett waves"), "{line:?}");
        assert!(!line.contains('\x1b'), "{line:?}");
    }
    let line = in_time(WAIT, async {
        loop {
            let line = tailed.next_line().await.unwrap().expect("riff tail runs");
            if line.contains("waves") {
                return line;
            }
        }
    })
    .await
    .expect("riff tail shows the action in time");
    assert_eq!(line.trim(), "* brett waves");

    // The read tool of a session shows it as plain text.
    let thread = riff::chat::thread();
    let api = crate::common::api_of(&server, &lead_of("bob"));
    let read = api.read(&lead_of("bob"), &thread, true).await.unwrap();
    let action = read.iter().find(|c| c.message.body == "/me waves").unwrap();
    let shown = riff::text::message(&riff_client::threads::Line::of(action, &thread));
    assert!(shown.ends_with(": * brett@heron waves"), "{shown}");

    // On the wire, an action is a plain message: no new kind and no new
    // field. So a riff of the release before reads it, and shows the
    // body `/me waves`. The signature of the
    // signed server is the one addition.

    let wire = serde_json::to_value(&action.message).unwrap();
    let mut fields: Vec<&str> = wire
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .filter(|key| !["sig", "payload"].contains(key))
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["at_ms", "body", "from", "seq", "to"], "{wire}");
    assert_eq!(wire["body"], "/me waves");
}

#[tokio::test]
async fn me_with_the_name_of_a_project_wakes_the_lead_of_the_sender() {
    let server = crate::common::start_signed().await;
    let lead = lead_of("brett");
    let api = crate::common::api_of(&server, &lead);
    api.register(&lead).await.unwrap();
    crate::common::name_lead(&crate::common::api(&server, "brett", None), &lead).await;
    let api = crate::common::api_of(&server, &lead);
    let mut wakes = Box::pin(api.watch(&lead).await.unwrap());
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;

    brett.say("/me asks @riff to look").await;
    let wake = in_time(WAIT, wakes.next())
        .await
        .expect("the lead of brett wakes")
        .unwrap()
        .unwrap();
    assert_eq!(wake.thread, riff::chat::thread());
    assert_eq!(wake.kind, Kind::Message);
    assert_eq!(
        to_of(&api, &lead, "/me asks @riff to look").await,
        ["name=riff"]
    );
}

#[tokio::test]
async fn an_unknown_command_is_not_sent_and_a_double_slash_sends_a_slash() {
    let server = crate::common::start_signed().await;
    let mut mike = Client::start(&server, "mike", "thelio", &[]).await;

    mike.say("/foo bar").await;
    let warned = mike.warns("unknown command").await;
    assert_eq!(warned, riff::chat::unknown("foo"));
    mike.say("//foo bar").await;
    let line = mike.shows("foo bar").await;
    assert!(is_line(&line, "mike", "/foo bar"), "{line:?}");

    // The chat holds only the line with //.
    let api = crate::common::api_of(&server, &lead_of("mike"));
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
    screen: Arc<Mutex<vt100::Parser<Marks>>>,
    dir: tempfile::TempDir,
}

/// What the terminal got besides the screen: each bell, the title, and
/// each other OSC (a toast).
#[derive(Default)]
struct Marks {
    bells: usize,
    title: String,
    osc: Vec<String>,
}

impl vt100::Callbacks for Marks {
    fn audible_bell(&mut self, _: &mut vt100::Screen) {
        self.bells += 1;
    }

    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.title = String::from_utf8_lossy(title).into_owned();
    }

    fn unhandled_osc(&mut self, _: &mut vt100::Screen, params: &[&[u8]]) {
        let params: Vec<_> = params.iter().map(|p| String::from_utf8_lossy(p)).collect();
        self.osc.push(params.join(";"));
    }
}

impl Tty {
    const ROWS: u16 = 24;
    const COLUMNS: u16 = 80;

    /// Starts `riff chat` as `user` in a new pseudo-terminal, and waits
    /// for its prompt.
    async fn start(url: &str, user: &str, host: &str) -> Tty {
        Self::start_with(url, user, host, Isolated::shared().tokio_riff()).await
    }

    /// [`Tty::start`] with `riff`, a command of a riff binary.
    async fn start_with(url: &str, user: &str, host: &str, riff: tokio::process::Command) -> Tty {
        Self::start_in(url, user, host, riff, "", &[]).await
    }

    /// [`Tty::start_with`] with the settings file `settings`, and the
    /// variables `env`.
    async fn start_in(
        url: &str,
        user: &str,
        host: &str,
        riff: tokio::process::Command,
        settings: &str,
        env: &[(&str, &str)],
    ) -> Tty {
        let dir = tempfile::tempdir().unwrap();
        Self::start_at(dir, url, user, host, riff, settings, env).await
    }

    /// [`Tty::start_in`] in the folder `dir`: the working folder and
    /// `RIFF_HOME` of the chat.
    async fn start_at(
        dir: tempfile::TempDir,
        url: &str,
        user: &str,
        host: &str,
        mut riff: tokio::process::Command,
        settings: &str,
        env: &[(&str, &str)],
    ) -> Tty {
        let size = nix::pty::Winsize {
            ws_row: Self::ROWS,
            ws_col: Self::COLUMNS,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = nix::pty::openpty(Some(&size), None).unwrap();
        crate::common::sign_in(url, dir.path(), user);
        std::fs::write(dir.path().join("config.toml"), settings).unwrap();
        let slave = std::fs::File::from(pty.slave);
        let child = riff
            .arg("chat")
            .current_dir(dir.path())
            .env("RIFF_SERVER", url)
            .env("RIFF_USER", user)
            .env("RIFF_HOST", host)
            .env("RIFF_HOME", dir.path())
            .env("TERM", "xterm")
            .env_remove("NO_COLOR")
            .env_remove("CLICOLOR_FORCE")
            .env_remove("VTE_VERSION")
            .envs(env.iter().copied())
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let master = std::fs::File::from(pty.master);
        let screen = Arc::new(Mutex::new(vt100::Parser::new_with_callbacks(
            Self::ROWS,
            Self::COLUMNS,
            0,
            Marks::default(),
        )));
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
            dir,
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

    /// Presses one key that the terminal sends as an escape sequence,
    /// for example `\x1b[D` for the left arrow: one write.
    async fn press(&mut self, sequence: &str) {
        self.master.write_all(sequence.as_bytes()).unwrap();
        self.master.flush().unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    /// `RIFF_HOME` of the chat.
    fn home(&self) -> &std::path::Path {
        self.dir.path()
    }

    /// The cursor of the screen: its row and its column.
    fn cursor(&self) -> (u16, u16) {
        self.screen.lock().unwrap().screen().cursor_position()
    }

    /// The rows of the screen, with no empty rows at the end, once
    /// `test` holds for them.
    async fn shows(&self, test: impl Fn(&[String]) -> bool) -> Vec<String> {
        let span = Span::start();
        loop {
            let rows = self.rows();
            if test(&rows) {
                return rows;
            }
            assert!(span.within(WAIT), "the screen in time: {rows:#?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// True when the first cell of `text` on the screen is bold. `None`
    /// when no row has `text`.
    fn bold(&self, text: &str) -> Option<bool> {
        let rows = self.rows();
        let (row, at) = rows
            .iter()
            .enumerate()
            .find_map(|(row, line)| line.find(text).map(|at| (row, at)))?;
        let parser = self.screen.lock().unwrap();
        let cell = parser.screen().cell(row as u16, at as u16)?;
        Some(cell.bold())
    }

    /// The marks of the terminal, once `test` holds for them.
    async fn marked(&self, test: impl Fn(&Marks) -> bool) {
        let span = Span::start();
        loop {
            {
                let parser = self.screen.lock().unwrap();
                let marks = parser.callbacks();
                if test(marks) {
                    return;
                }
                assert!(
                    span.within(WAIT),
                    "the marks in time: {} bells, title {:?}, OSC {:?}",
                    marks.bells,
                    marks.title,
                    marks.osc
                );
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// The number of bells so far.
    fn bells(&self) -> usize {
        self.screen.lock().unwrap().callbacks().bells
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

/// A line that names the person is a highlight (01M6XSHCZB0Y08XRJ3J16Q4RTF,
/// 01M6XSHMNNTRC66CMZ0DF2X5RA): it is bold, it rings the bell, and the
/// title counts it. A key clears the number. The hook gets the line
/// (01M6XSHMR46J353DK9HKSHVN0P). Another line is plain.
#[tokio::test]
async fn in_a_terminal_a_highlight_is_bold_rings_and_marks_the_title() {
    let server = crate::common::start_signed().await;
    let out = tempfile::tempdir().unwrap();
    let (stdin, env) = (out.path().join("stdin"), out.path().join("env"));
    let settings = format!(
        "[chat]\ntitle = true\non_highlight = '''cat > \"{}\"; \
         printf '%s|%s' \"$RIFF_SENDER\" \"$RIFF_THREAD\" > \"{}\"'''\n",
        stdin.display(),
        env.display()
    );
    let riff = Isolated::shared().tokio_riff();
    let mut mike = Tty::start_in(&server, "mike", "thelio", riff, &settings, &[]).await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;

    brett.say("a line for all").await;
    mike.shows(|rows| rows.iter().any(|row| row.contains("a line for all")))
        .await;
    brett.say("mike: look at #12").await;
    mike.shows(|rows| rows.iter().any(|row| row.contains("look at #12")))
        .await;
    mike.marked(|m| m.bells == 1 && m.title == "(1) riff chat")
        .await;
    assert_eq!(mike.bold("look at #12"), Some(true));
    assert_eq!(mike.bold("a line for all"), Some(false));
    let span = Span::start();
    while std::fs::read_to_string(&env).unwrap_or_default() != "brett|chat" {
        assert!(span.within(WAIT), "the hook runs in time");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        std::fs::read_to_string(&stdin).unwrap(),
        "<brett> mike: look at #12\n"
    );

    brett.say("can @mike see #13?").await;
    mike.marked(|m| m.bells == 2 && m.title == "(2) riff chat")
        .await;
    mike.keys("x").await;
    mike.marked(|m| m.title == "riff chat").await;
    assert_eq!(mike.bells(), 2);
}

/// `--color never` and `NO_COLOR` turn off the bold of a highlight, and
/// the bell still rings (01M6XSHMNNTRC66CMZ0DF2X5RA).
#[tokio::test]
async fn in_a_terminal_never_and_no_color_turn_off_the_bold_of_a_highlight() {
    let server = crate::common::start_signed().await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;
    let never: (&[&str], &[(&str, &str)]) = (&["--color", "never"], &[]);
    let no_color: (&[&str], &[(&str, &str)]) = (&[], &[("NO_COLOR", "1")]);
    for (n, (args, env)) in [never, no_color].into_iter().enumerate() {
        let mut riff = Isolated::shared().tokio_riff();
        riff.args(args);
        let mike = Tty::start_in(&server, "mike", "thelio", riff, "", env).await;
        let body = format!("mike: plain {n}");
        brett.say(&body).await;
        mike.shows(|rows| rows.iter().any(|row| row.contains(&body)))
            .await;
        mike.marked(|m| m.bells == 1).await;
        assert_eq!(mike.bold(&body), Some(false), "{args:?} {env:?}");
    }
}

/// 01M74BPHQZHXFYYNXAJ1WFASQQ: riff-server posts the lines that riff
/// writes for mike, as `<riff-bot>`. The ask of a release names the owner
/// brett, and is a highlight for brett (01M6XSHMGM82QSABRWCYH595VH): it
/// is bold, and rings the bell. The line "riff started the lead ..."
/// names no one, and is plain.
#[tokio::test]
async fn in_a_terminal_the_ask_of_riff_bot_is_a_highlight_for_its_person() {
    let server = crate::common::start_signed().await;
    let brett = Tty::start(&server, "brett", "heron").await;
    let person: SessionUri = "riff://mike@thelio/como-technologies/riff".parse().unwrap();
    let api = server.api("mike", None);
    api.bot(&person, BotFact::LeadStarted).await.unwrap();
    let started = "<riff-bot> riff started the lead of mike in como-technologies/riff on";
    brett
        .shows(|rows| rows.iter().any(|row| row.contains(started)))
        .await;
    let waits = BotFact::ReleaseWaits {
        tag: "v3.3.0".into(),
        run: 9,
        reminder: false,
    };
    api.bot(&person, waits).await.unwrap();
    let asked = "<riff-bot> @brett riff release v3.3.0 waits for your approval";
    brett
        .shows(|rows| rows.iter().any(|row| row.contains(asked)))
        .await;
    brett.marked(|m| m.bells == 1).await;
    assert_eq!(brett.bold(asked), Some(true));
    assert_eq!(brett.bold(started), Some(false));
    assert_eq!(brett.bells(), 1, "the line of the daemon rings no bell");
}

/// A highlight in the history at the start of the chat is bold, and
/// rings no bell. A new one rings (01M6XSHMX4VJKQHAHP8H4XDSBQ).
#[tokio::test]
async fn in_a_terminal_a_highlight_of_the_history_rings_no_bell() {
    let server = crate::common::start_signed().await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;
    brett.say("mike: an old line").await;
    brett.shows("an old line").await;

    let mike = Tty::start(&server, "mike", "thelio").await;
    mike.shows(|rows| rows.iter().any(|row| row.contains("an old line")))
        .await;
    assert_eq!(mike.bold("an old line"), Some(true));
    brett.say("mike: a new line").await;
    mike.shows(|rows| rows.iter().any(|row| row.contains("a new line")))
        .await;
    mike.marked(|m| m.bells >= 1).await;
    assert_eq!(mike.bells(), 1);
}

/// With `toast`, a highlight shows a desktop toast: OSC 9, or OSC 777 in
/// a terminal of VTE (01M6XSHMNNTRC66CMZ0DF2X5RA).
#[tokio::test]
async fn in_a_terminal_a_highlight_shows_a_toast() {
    let server = crate::common::start_signed().await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;
    let settings = "[chat]\ntoast = true\n";
    let osc9: (&[(&str, &str)], &str) = (&[], "9;<brett> mike: toast 0");
    let osc777: (&[(&str, &str)], &str) = (
        &[("VTE_VERSION", "7600")],
        "777;notify;riff chat;<brett> mike: toast 1",
    );
    for (n, (env, toast)) in [osc9, osc777].into_iter().enumerate() {
        let riff = Isolated::shared().tokio_riff();
        let mike = Tty::start_in(&server, "mike", "thelio", riff, settings, env).await;
        brett.say(&format!("mike: toast {n}")).await;
        mike.marked(|m| m.osc.iter().any(|osc| osc == toast)).await;
    }
}

/// `riff tail` runs the hook of a highlight, also with no terminal
/// (01M6XSHMR46J353DK9HKSHVN0P).
#[tokio::test]
async fn tail_runs_the_hook_of_a_highlight() {
    let server = crate::common::start_signed().await;
    let api = crate::common::api_of(&server, &lead_of("mike"));
    let dir = tempfile::tempdir().unwrap();
    let env = dir.path().join("env");
    std::fs::write(
        dir.path().join("config.toml"),
        format!(
            "[chat]\non_highlight = '''printf '%s|%s|%s' \"$RIFF_SENDER\" \"$RIFF_THREAD\" \
             \"$(cat)\" > \"{}\"'''\n",
            env.display()
        ),
    )
    .unwrap();
    let (_tail, mut tailed) = tail(&server, "mike", dir.path());
    connected(&api, &mut tailed).await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;

    brett.say("@mike the release is out").await;
    let span = Span::start();
    while std::fs::read_to_string(&env).unwrap_or_default().is_empty() {
        assert!(span.within(WAIT), "the hook of riff tail runs in time");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        std::fs::read_to_string(&env).unwrap(),
        "brett|chat|<brett> @mike the release is out"
    );
}

#[tokio::test]
async fn in_a_terminal_your_line_shows_once_and_the_prompt_comes_back() {
    let server = crate::common::start_signed().await;
    let mut mike = Tty::start(&server, "mike", "thelio").await;

    mike.keys("hello all\r").await;
    let rows = mike
        .shows(|rows| {
            rows.iter().any(|row| row.contains("<mike> hello all"))
                && rows.last().is_some_and(|row| row == PROMPT.trim_end())
        })
        .await;
    let seen = rows.iter().filter(|row| row.contains("hello all")).count();
    assert_eq!(seen, 1, "{rows:#?}");
    assert!(
        rows.iter().any(|row| is_line(row, "mike", "hello all")),
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
    let server = crate::common::start_signed().await;
    let mut mike = Tty::start(&server, "mike", "thelio").await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;

    mike.keys("half a li").await;
    mike.shows(|rows| rows.last().is_some_and(|row| row.ends_with("> half a li")))
        .await;
    brett.say("a line from brett").await;
    let rows = mike
        .shows(|rows| rows.iter().any(|row| row.contains("a line from brett")))
        .await;
    let n = rows.len();
    assert!(
        is_line(&rows[n - 2], "brett", "a line from brett"),
        "{rows:#?}"
    );
    assert_eq!(rows[n - 1], format!("{PROMPT}half a li"), "{rows:#?}");

    // The typed text is still there: Enter sends all of it.
    mike.keys("ne\r").await;
    let line = brett.shows("half a line").await;
    assert!(is_line(&line, "mike", "half a line"), "{line:?}");
}

/// A line that comes while the person types does not change the typed
/// text or the cursor (#1665). The typed line is longer than one row, and
/// the cursor is in the middle of it.
#[tokio::test]
async fn in_a_terminal_a_new_line_keeps_the_typed_text_and_the_cursor() {
    let server = crate::common::start_signed().await;
    let mut mike = Tty::start(&server, "mike", "thelio").await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;

    let typed = "0123456789".repeat(8);
    mike.keys(&typed).await;
    for _ in 0..3 {
        mike.press("\x1b[D").await;
    }
    let shown = format!("{PROMPT}{typed}");
    let wrapped = |rows: &[String]| rows.join("").ends_with(&shown);
    let before = mike.shows(|rows| wrapped(rows)).await;
    let cursor = mike.cursor();

    brett.say("a line from brett").await;
    let long = "word ".repeat(30);
    brett.say(&long).await;
    let after = mike
        .shows(|rows| rows.iter().any(|row| row.contains("word word")))
        .await;
    assert!(wrapped(&after), "{after:#?}");
    // The prompt moved down by the rows of the new lines, and the cursor
    // with it.
    let moved = u16::try_from(after.len() - before.len()).unwrap();
    assert!(moved >= 4, "{after:#?}");
    assert_eq!(mike.cursor(), (cursor.0 + moved, cursor.1), "{after:#?}");

    // The cursor is where it was: the next key goes in at the cursor.
    mike.keys("X\r").await;
    let line = brett.shows("0123456789").await;
    let want = format!(
        "{}X{}",
        &typed[..typed.len() - 3],
        &typed[typed.len() - 3..]
    );
    assert!(is_line(&line, "mike", &want), "{line:?}");
}

/// 01M6Y8N8W5RE5H6MW3TNDH5BSY: with a typed text, `↑` keeps the text and
/// goes to its start (#1788). On an empty line, `↑` brings back the last
/// sent line, and `↓` empties the line again.
#[tokio::test]
async fn in_a_terminal_up_keeps_the_typed_text_and_brings_back_a_sent_line_on_an_empty_line() {
    let server = crate::common::start_signed().await;
    let mut mike = Tty::start(&server, "mike", "thelio").await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;
    let prompt = |text: &str| {
        let shown = format!("{PROMPT}{text}");
        move |rows: &[String]| rows.last().is_some_and(|row| *row == shown.trim_end())
    };

    mike.keys("the first line\r").await;
    brett.shows("the first line").await;
    mike.shows(prompt("")).await;
    mike.keys("my words").await;
    mike.press("\x1b[A").await;
    mike.keys("Note: ").await;
    mike.shows(prompt("Note: my words")).await;
    mike.keys("\r").await;
    let line = brett.shows("my words").await;
    assert!(is_line(&line, "mike", "Note: my words"), "{line:?}");

    mike.shows(prompt("")).await;
    mike.press("\x1b[A").await;
    mike.shows(prompt("Note: my words")).await;
    mike.press("\x1b[A").await;
    mike.shows(prompt("the first line")).await;
    mike.press("\x1b[B").await;
    mike.shows(prompt("Note: my words")).await;
    mike.press("\x1b[B").await;
    mike.shows(prompt("")).await;
}

/// `@` shows the names of the chat on the rows under the prompt
/// (01M6Y1EKGKNW2MH970PDF6PJK1, 01M6Y1EKKCCZ5215V49Z5T3HKH): `@r` shows
/// `riff`, the project of the lead of mike, and `Tab` completes it.
/// `Enter` picks the name too, `Esc` closes the list, and a name that is
/// not in the list still posts.
#[tokio::test]
async fn in_a_terminal_at_shows_the_names_and_tab_completes_one() {
    let server = crate::common::start_signed().await;
    let lead = lead_of("mike");
    crate::common::api_of(&server, &lead)
        .register(&lead)
        .await
        .unwrap();
    crate::common::name_lead(&server.api("mike", None), &lead).await;
    let mut mike = Tty::start(&server, "mike", "thelio").await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;
    let note = "  Tab or Enter picks riff. Esc closes the list.";
    let listed = |rows: &[String]| {
        rows.last().is_some_and(|row| row == note)
            && rows.iter().any(|row| row.starts_with("  riff  project"))
    };
    let prompt = |text: &str| {
        let want = format!("{PROMPT}{text}").trim_end().to_owned();
        move |rows: &[String]| rows.last().is_some_and(|row| *row == want)
    };

    // Tab completes the name, with a space after it.
    mike.keys("@r").await;
    // `@` shows the same note: wait for the prompt too.
    let want = format!("{PROMPT}@r");
    mike.shows(|rows| listed(rows) && rows.len() >= 3 && rows[rows.len() - 3] == want)
        .await;
    mike.keys("\t").await;
    mike.shows(prompt("@riff ")).await;
    // The cursor is after the space: the next key types there.
    let after = u16::try_from(PROMPT.len() + "@riff ".len()).unwrap();
    let span = Span::start();
    while mike.cursor().1 != after {
        assert!(span.within(WAIT), "the cursor in time: {:?}", mike.cursor());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    mike.keys("is #12 done?\r").await;
    let line = brett.shows("is #12 done?").await;
    assert!(is_line(&line, "mike", "@riff is #12 done?"), "{line:?}");

    // Enter picks the name too, and sends nothing.
    mike.keys("ask @r").await;
    mike.shows(listed).await;
    mike.keys("\r").await;
    mike.shows(prompt("ask @riff ")).await;
    mike.keys("now\r").await;
    let line = brett.shows("ask @riff now").await;
    assert!(is_line(&line, "mike", "ask @riff now"), "{line:?}");

    // Esc closes the list: then Enter sends the line as it is.
    mike.keys("@r").await;
    mike.shows(listed).await;
    mike.press("\x1b").await;
    mike.shows(prompt("@r")).await;
    mike.keys("\r").await;
    let line = brett.shows("@r").await;
    assert!(is_line(&line, "mike", "@r"), "{line:?}");

    // A name that is not in the list shows no list, and posts.
    mike.keys("@zed hi").await;
    mike.shows(prompt("@zed hi")).await;
    mike.keys("\r").await;
    let line = brett.shows("@zed hi").await;
    assert!(is_line(&line, "mike", "@zed hi"), "{line:?}");
}

/// `→` at the end of the line puts none of the list of the names in the
/// line, and the list stays (01M6YDSREH39WWV6QHB0M7Z6P5): `Enter` then
/// picks the name, as the note says, and the line that posts holds no
/// row of the list.
#[tokio::test]
async fn in_a_terminal_the_right_arrow_puts_none_of_the_list_in_the_line() {
    let server = crate::common::start_signed().await;
    let lead = lead_of("mike");
    crate::common::api_of(&server, &lead)
        .register(&lead)
        .await
        .unwrap();
    crate::common::name_lead(&server.api("mike", None), &lead).await;
    let mut mike = Tty::start(&server, "mike", "thelio").await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;
    let note = "  Tab or Enter picks riff. Esc closes the list.";
    let listed = |text: &'static str| {
        move |rows: &[String]| {
            rows.len() >= 3
                && rows[rows.len() - 3] == format!("{PROMPT}{text}")
                && rows.last().is_some_and(|row| row == note)
        }
    };

    mike.keys("hi @r").await;
    mike.shows(listed("hi @r")).await;
    mike.press("\x1b[C").await;
    let rows = mike.shows(listed("hi @r")).await;
    assert!(
        rows.iter().any(|row| row == "  riff  project  online"),
        "{rows:#?}"
    );
    // The line is still `hi @r`: Enter picks the name, and sends nothing.
    mike.keys("\r").await;
    let want = format!("{PROMPT}hi @riff");
    mike.shows(|rows| rows.last().is_some_and(|row| *row == want))
        .await;
    mike.keys("\r").await;
    let line = brett.shows("hi @r").await;
    assert!(is_line(&line, "mike", "hi @riff"), "{line:?}");
}

/// `#` shows the open issues from `gh` on the rows under the prompt
/// (01M74CX2V9YAF0KFPJDRM4DA83, 01M74CX9H7WT1T4EN70AVD2TGZ,
/// 01M74CX9KVGQG4Y7ATT6268NMY): in a clone of como-technologies/riff,
/// `#186` shows #1862 and #1860, the newest first, and `Tab` puts
/// `#1862 ` in the line. Then Enter sends the line. The read is the
/// conditional read of the open issues.
#[tokio::test]
async fn in_a_terminal_hash_shows_the_open_issues_and_tab_puts_one() {
    let server = crate::common::start_signed().await;
    let bin = tempfile::tempdir().unwrap();
    isolated::write_program(bin.path().join("gh"), crate::common::ISSUES_GH);
    let path = crate::common::path_with(bin.path());
    let clone = crate::common::clone_of("como-technologies/riff");
    let riff = Isolated::shared().tokio_riff();
    let env = [("PATH", path.as_str())];
    let mut mike = Tty::start_at(clone, &server, "mike", "thelio", riff, "", &env).await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;
    let listed = |rows: &[String]| {
        rows.len() >= 4
            && rows[rows.len() - 4..]
                == [
                    format!("{PROMPT}is #186"),
                    "  #1862  riff  Projects A: riff-server keeps the projects of the riff".into(),
                    "  #1860  riff  Pick an issue with #".into(),
                    "  Tab or Enter picks #1862. Esc closes the list.".into(),
                ]
    };

    // The chat reads the issues after its start, and a key shows the
    // list: type the word again until the list shows.
    let span = Span::start();
    loop {
        mike.keys("is #186").await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        if listed(&mike.rows()) {
            break;
        }
        assert!(span.within(WAIT), "the list in time: {:#?}", mike.rows());
        // Ctrl-U: the line goes.
        mike.keys("\x15").await;
    }
    mike.keys("\t").await;
    let want = format!("{PROMPT}is #1862");
    mike.shows(|rows| rows.last().is_some_and(|row| *row == want))
        .await;
    // The cursor is after the space: the next key types there.
    let after = u16::try_from(PROMPT.len() + "is #1862 ".len()).unwrap();
    let span = Span::start();
    while mike.cursor().1 != after {
        assert!(span.within(WAIT), "the cursor in time: {:?}", mike.cursor());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    mike.keys("done?\r").await;
    let line = brett.shows("done?").await;
    assert!(is_line(&line, "mike", "is #1862 done?"), "{line:?}");
    let calls = std::fs::read_to_string(bin.path().join("calls")).unwrap();
    assert!(
        calls.contains("api -i repos/como-technologies/riff/issues?state=open"),
        "{calls}"
    );
}

/// A server that names a build and holds each post for `wait`: the
/// reply of a post comes a while after the post.
struct Deploy {
    newer: Arc<std::sync::atomic::AtomicBool>,
}

impl Deploy {
    /// The server, and the switch that makes it name a newer release.
    async fn start(wait: Duration) -> (Deploy, crate::common::Signed) {
        use axum::extract::Request;
        use axum::http::HeaderValue;
        use axum::middleware::{Next, from_fn};
        use riff_core::build::{Build, HEADER};

        let newer = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let switch = Arc::clone(&newer);
        let version = Build::this().semver().unwrap();
        let build = Build {
            version: format!("{}.{}.0", version.major, version.minor + 1),
            ..Build::this()
        };
        let build = HeaderValue::from_str(&build.to_string()).unwrap();
        let server = crate::common::start_signed_with(move |router| {
            router.layer(from_fn(move |request: Request, next: Next| {
                let (switch, build) = (Arc::clone(&switch), build.clone());
                async move {
                    let post = request.uri().path() == "/v1/post";
                    let mut reply = next.run(request).await;
                    if post {
                        tokio::time::sleep(wait).await;
                    }
                    if switch.load(std::sync::atomic::Ordering::SeqCst) {
                        reply.headers_mut().insert(HEADER, build);
                    }
                    reply
                }
            }))
        })
        .await;
        (Deploy { newer }, server)
    }

    fn deploy(&self) {
        self.newer.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// A notice of riff itself shows above the prompt, and the typed text
/// stays (#1665). The server runs a newer release, and the update by
/// itself is on: the first reply that names the release starts the
/// update, and riff says so while the person types.
#[tokio::test]
async fn in_a_terminal_a_notice_of_riff_prints_above_what_you_type() {
    let (deploy, server) = Deploy::start(Duration::from_millis(600)).await;
    let bin = tempfile::tempdir().unwrap();
    let cargo = bin.path().join("cargo");
    isolated::write_program(&cargo, "#!/bin/sh\nexit 0\n");
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").unwrap()
    );
    let mut riff = Isolated::shared().tokio_riff();
    riff.env("PATH", path);
    let mut mike = Tty::start_with(&server, "mike", "thelio", riff).await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;
    std::fs::write(mike.home().join("config.toml"), "[update]\nauto = true\n").unwrap();

    // The reply to this line names the newer release 600 ms later.
    deploy.deploy();
    mike.keys("one\r").await;
    mike.keys("half a li").await;
    let rows = mike
        .shows(|rows| rows.iter().any(|row| row.contains("installs it now")))
        .await;
    let n = rows.len();
    assert_eq!(rows[n - 1], format!("{PROMPT}half a li"), "{rows:#?}");
    let prompts = rows.iter().filter(|row| row.contains(PROMPT)).count();
    assert_eq!(prompts, 1, "{rows:#?}");

    mike.keys("ne\r").await;
    let line = brett.shows("half a line").await;
    assert!(is_line(&line, "mike", "half a line"), "{line:?}");
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
    let server = crate::common::start_signed().await;
    let bin = tempfile::tempdir().unwrap();
    let binary = bin.path().join("riff");
    let riff = Isolated::shared().riff_path();
    install(&riff, &binary);
    let command = tokio::process::Command::from(Isolated::shared().command(&binary));
    let mut mike = Tty::start_with(&server, "mike", "thelio", command).await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;
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
        is_line(&line, "mike", "hello from the new riff"),
        "{line:?}"
    );
}

/// A chat that runs a new binary gives the new chat the text that the
/// person typed and did not send (#1665, 01M6QR3JMCZ63ZYSRFBSWJ6WD7): it
/// shows after the prompt, and Enter sends all of it.
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_runs_the_new_binary_and_keeps_what_you_type() {
    let server = crate::common::start_signed().await;
    let bin = tempfile::tempdir().unwrap();
    let binary = bin.path().join("riff");
    let riff = Isolated::shared().riff_path();
    install(&riff, &binary);
    let command = tokio::process::Command::from(Isolated::shared().command(&binary));
    let mut mike = Tty::start_with(&server, "mike", "thelio", command).await;
    let mut brett = Client::start(&server, "brett", "heron", &[]).await;

    // A text with a leading hyphen and a quote is no option of the command.
    mike.keys("--half a \"li").await;
    mike.shows(|rows| {
        rows.last()
            .is_some_and(|row| row.ends_with("> --half a \"li"))
    })
    .await;
    install(&riff, &binary);
    mike.shows(|rows| rows.iter().any(|row| row.contains("a new riff is on disk")))
        .await;
    brett.say("after the update").await;
    let rows = mike
        .shows(|rows| {
            rows.iter().any(|row| row.contains("after the update"))
                && rows
                    .last()
                    .is_some_and(|row| row == &format!("{PROMPT}--half a \"li"))
        })
        .await;
    let start = rows
        .iter()
        .filter(|row| row.starts_with("riff: chat as"))
        .count();
    assert_eq!(start, 1, "{rows:#?}");

    mike.keys("ne\r").await;
    let line = brett.shows("half a").await;
    assert!(is_line(&line, "mike", "--half a \"line"), "{line:?}");
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
    let api = start_open().await;
    let proxy = Proxy::start(api.base()).await;
    let behind = Api::new(&proxy.url);
    let mut mike = Client::start(behind.base(), "mike", "thelio", &[]).await;
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
    let status = in_time(WAIT, mike.child.wait()).await;
    assert!(status.unwrap().unwrap().success());
    let mut warned = Vec::new();
    while let Ok(Some(line)) = mike.stderr.next_line().await {
        warned.push(line);
    }
    assert_eq!(warned, [riff::view::RECONNECTING, riff::view::BACK]);
}

/// `riff tail` connects again at once after a cut, with no error line
/// (01M3NK7VHXB0PAR8VH8GQQA06K).
#[tokio::test]
async fn a_cut_stream_shows_no_error_in_tail() {
    let api = start_open().await;
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
    let started = in_time(WAIT, err.next_line()).await.unwrap();
    assert!(
        started
            .unwrap()
            .unwrap()
            .contains("showing new messages in chat")
    );
    let (lead, chat) = (lead_of("mike"), riff::chat::thread());
    for body in ["before the cut", "after the cut"] {
        connected(&api, &mut out).await;
        api.post(&lead, Some(&chat), &[], body, Kind::Message)
            .await
            .unwrap();
        in_time(WAIT, async {
            loop {
                let line = out.next_line().await.unwrap().unwrap();
                if line.contains(body) {
                    break;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("riff tail shows {body:?} in time"));
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
/// end answers each call with 500, the text of Cloud Run and no build
/// header, as Cloud Run does after the old instance exits at a deploy
/// (01M3QCMJ9F1GRTRRSB4AW9TC3D, 01M56WNP3TEGH98B9EAM4H62P4).
async fn behind_a_front_end(down: Arc<std::sync::atomic::AtomicBool>) -> String {
    let router = riff_server::router().layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let down = down.load(std::sync::atomic::Ordering::SeqCst);
            async move {
                if down {
                    axum::response::IntoResponse::into_response((
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        "The request was aborted because there was no available instance.",
                    ))
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

/// A 500 of the front end with no build is no version error: the chat
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
    let mut mike = Client::start(behind.base(), "mike", "thelio", &[]).await;
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
    mike.shows_within("after the outage", 2 * WAIT).await;

    mike.say("/quit").await;
    let status = in_time(WAIT, mike.child.wait()).await;
    assert!(status.unwrap().unwrap().success());
    let mut warned = Vec::new();
    while let Ok(Some(line)) = mike.stderr.next_line().await {
        warned.push(line);
    }
    // At most the short lines of a cut or a wait: no version error.
    let short = [
        riff::view::RECONNECTING,
        riff::view::BACK,
        riff_client::link::WAITING,
    ];
    assert!(
        warned.iter().all(|l| short.contains(&l.as_str())),
        "{warned:?}"
    );
}

/// `riff tail chat --color never` as ann, through `server`, with its
/// stdout and stderr. It waits for the start line on stderr.
async fn tail_with_errors(
    server: &str,
    dir: &std::path::Path,
) -> (
    Child,
    Lines<BufReader<ChildStdout>>,
    Lines<BufReader<ChildStderr>>,
) {
    let mut child = Isolated::shared()
        .tokio_riff()
        .args(["tail", "chat", "--color", "never"])
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "ann")
        .env("RIFF_HOST", "wren")
        .env("RIFF_HOME", dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let out = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut err = BufReader::new(child.stderr.take().unwrap()).lines();
    let started = tokio::time::timeout(WAIT, err.next_line()).await.unwrap();
    assert!(started.unwrap().unwrap().contains("showing new messages"));
    (child, out, err)
}

/// The lines of `lines` that contain `mark`, up to the first line that
/// contains `end`.
async fn marked_until(
    lines: &mut Lines<BufReader<ChildStdout>>,
    mark: &str,
    end: &str,
) -> Vec<String> {
    let span = Span::start();
    let mut seen = Vec::new();
    loop {
        assert!(
            span.within(WAIT),
            "no line with {end:?} in time; seen {seen:#?}"
        );
        let line = in_time(WAIT, lines.next_line())
            .await
            .unwrap_or_else(|late| panic!("no line with {end:?}: {late}; seen {seen:#?}"))
            .unwrap()
            .expect("riff runs");
        if line.contains(mark) {
            seen.push(line.clone());
        }
        if line.contains(end) {
            return seen;
        }
    }
}

/// A break of the stream of `riff tail`: 3 messages come in the break.
/// After the new connection, the tail shows each of them one time, in
/// order, and then the live messages (01M49Z4E8QB1QDXVCPE6MX5JX2).
#[tokio::test]
async fn tail_shows_each_message_of_a_break_once_in_order() {
    let api = start_open().await;
    let proxy = Proxy::start(api.base()).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut child, mut out, _err) = tail_with_errors(&proxy.url, dir.path()).await;
    connected(&api, &mut out).await;
    let (lead, chat) = (lead_of("mike"), riff::chat::thread());
    let post = |body: String| {
        let (api, lead, chat) = (&api, &lead, &chat);
        async move {
            api.post(lead, Some(chat), &[], &body, Kind::Message)
                .await
                .unwrap();
        }
    };
    post("break-0 before".into()).await;
    assert_eq!(marked_until(&mut out, "break-", "break-0").await.len(), 1);

    // The stream breaks, and riff tail cannot connect for a time.
    proxy.cut(false);
    for n in 1..=3 {
        post(format!("break-{n} in the break")).await;
    }
    proxy.open();
    let seen = marked_until(&mut out, "break-", "break-3").await;
    post("break-4 live".into()).await;
    let live = marked_until(&mut out, "break-", "break-4").await;
    let bodies: Vec<&str> = seen.iter().chain(&live).map(|line| line.trim()).collect();
    assert_eq!(
        bodies,
        [
            "break-1 in the break",
            "break-2 in the break",
            "break-3 in the break",
            "break-4 live"
        ]
    );
    child.kill().await.unwrap();
}

/// The same for `riff chat`: each line of a break shows one time, in
/// order, and no line shows two times (01M49Z4E8QB1QDXVCPE6MX5JX2).
#[tokio::test]
async fn chat_shows_each_line_of_a_break_once_in_order() {
    let api = start_open().await;
    let proxy = Proxy::start(api.base()).await;
    let behind = Api::new(&proxy.url);
    let (lead, chat) = (lead_of("mike"), riff::chat::thread());
    let post = |body: String| {
        let (api, lead, chat) = (&api, &lead, &chat);
        async move {
            api.post(lead, Some(chat), &[], &body, Kind::Message)
                .await
                .unwrap();
        }
    };
    post("break-0 history".into()).await;
    let mut mike = Client::start(behind.base(), "mike", "thelio", &[]).await;
    let history = marked_until(&mut mike.stdout, "break-", "break-0").await;
    assert_eq!(history.len(), 1);

    // The server ends the stream, then the chat cannot connect for a
    // time.
    proxy.cut(true);
    post("break-1 after a cut".into()).await;
    let cut = marked_until(&mut mike.stdout, "break-", "break-1").await;
    assert_eq!(cut.len(), 1);
    proxy.cut(false);
    for n in 2..=4 {
        post(format!("break-{n} in the break")).await;
    }
    proxy.open();
    let seen = marked_until(&mut mike.stdout, "break-", "break-4").await;
    mike.say("break-5 typed").await;
    let live = marked_until(&mut mike.stdout, "break-", "break-5").await;
    let bodies: Vec<&str> = seen
        .iter()
        .map(|line| {
            line.rsplit_once("] ")
                .map_or(line.as_str(), |(_, body)| body)
        })
        .collect();
    assert_eq!(
        bodies,
        [
            "break-2 in the break",
            "break-3 in the break",
            "break-4 in the break"
        ]
    );
    assert_eq!(live.len(), 1, "{live:#?}");
    assert!(live[0].ends_with("<mike> break-5 typed"), "{live:?}");
}

/// A break with more new messages than the server keeps (200) shows one
/// line with the number of the lost messages, then the kept messages
/// (01M49Z4EB7T972BHEP6T92P574).
#[tokio::test]
async fn a_break_longer_than_the_kept_messages_shows_the_lost_number() {
    let api = start_open().await;
    let proxy = Proxy::start(api.base()).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut child, mut out, mut err) = tail_with_errors(&proxy.url, dir.path()).await;
    connected(&api, &mut out).await;
    let (lead, chat) = (lead_of("mike"), riff::chat::thread());

    proxy.cut(false);
    for n in 1..=210 {
        let body = format!("many-{n:03}");
        api.post(&lead, Some(&chat), &[], &body, Kind::Message)
            .await
            .unwrap();
    }
    proxy.open();
    let lost = riff::text::lost_messages(10);
    let span = Span::start();
    loop {
        assert!(span.within(WAIT), "the line of the lost messages in time");
        let line = in_time(WAIT, err.next_line())
            .await
            .expect("the line of the lost messages in time")
            .unwrap()
            .expect("riff tail runs");
        if line.contains("lost") {
            assert_eq!(line, lost);
            break;
        }
    }
    let shown = marked_until(&mut out, "many-", "many-210").await;
    let first = shown.first().map(|line| line.trim());
    assert_eq!((shown.len(), first), (200, Some("many-011")));
    child.kill().await.unwrap();
}
