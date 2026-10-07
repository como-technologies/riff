//! `riff who` for people, with the styles of `riff tail`
//! (01M3Q63MVZ74WPNBA3QJYQGHFG), and its `--color`
//! (01M3Q5VE2D244XDZRYXM8DNSRS), against a real server.

use isolated::Isolated;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use riff::style;
use riff_core::name::SessionUri;

const THREAD: &str = "como-technologies/riff";

async fn start_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
}

/// A git repository with a GitHub origin, so the place of each session
/// is `como-technologies/riff`.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ],
    ] {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status();
        assert!(status.unwrap().success());
    }
    dir
}

/// `riff ARGS` in `dir` through a pipe, as the agent session `session`
/// of mike, or as the person brett.
fn riff(server: &str, dir: &Path, session: Option<&str>, args: &[&str]) -> Command {
    let mut cmd = Isolated::shared().riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", dir)
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    match session {
        Some(id) => cmd.env("RIFF_USER", "mike").env("RIFF_SESSION", id),
        None => cmd.env("RIFF_USER", "brett"),
    };
    cmd
}

/// The stdout of `cmd`, after its exit.
async fn output(mut cmd: Command) -> String {
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

/// The URI of the agent session `id` of mike.
fn uri(id: &str) -> SessionUri {
    format!("riff://mike@pangolin/{THREAD}?session={id}")
        .parse()
        .unwrap()
}

/// `name` in the style of the session `id`.
fn in_color(id: &str, name: &str) -> String {
    style::styled(style::session(&uri(id)), name)
}

#[tokio::test(flavor = "multi_thread")]
async fn who_prints_each_session_in_the_color_of_tail() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();

    // Follow the thread, then post from two sessions.
    let mut tail = riff(&server, dir, None, &["tail", THREAD, "--color", "always"])
        .spawn()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    for id in ["a1", "b2"] {
        let post = riff(&server, dir, Some(id), &["post", "-t", THREAD, "hello"]);
        output(post).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    tail.kill().unwrap();
    let mut tailed = String::new();
    tail.stdout
        .take()
        .unwrap()
        .read_to_string(&mut tailed)
        .unwrap();
    tail.wait().unwrap();

    let who = output(riff(&server, dir, None, &["who", "--color", "always"])).await;
    assert_ne!(
        style::session(&uri("a1")),
        style::session(&uri("b2")),
        "pick two sessions with two colors"
    );
    for (id, name) in [
        ("a1", "mike@pangolin:riff (a1)"),
        ("b2", "mike@pangolin:riff (b2)"),
    ] {
        let name = in_color(id, name);
        assert!(tailed.contains(&name), "{tailed:?}");
        assert!(who.contains(&name), "{who:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pipe_gets_no_color_unless_asked() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    output(riff(&server, dir, Some("a1"), &["lead"])).await;

    let auto = output(riff(&server, dir, None, &["who"])).await;
    assert!(
        auto.contains("\nmike@pangolin:riff (a1)  paused   lead\n"),
        "{auto}"
    );
    assert!(!auto.contains('\x1b'), "{auto:?}");

    let mut never = riff(&server, dir, None, &["who", "--color", "never"]);
    never.env("CLICOLOR_FORCE", "1");
    let never = output(never).await;
    assert!(!never.contains('\x1b'), "{never:?}");

    let mut no_color = riff(&server, dir, None, &["who"]);
    no_color.env("NO_COLOR", "1");
    let no_color = output(no_color).await;
    assert!(!no_color.contains('\x1b'), "{no_color:?}");

    let always = output(riff(&server, dir, None, &["who", "--color", "always"])).await;
    assert!(always.contains("\x1b["), "{always:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_blocked_session_is_red() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let blocked = ["blocked", "waits for a review"];
    output(riff(&server, dir, Some("a1"), &["lead"])).await;
    output(riff(&server, dir, Some("a1"), &["resume", "--riff"])).await;
    output(riff(&server, dir, Some("b2"), &blocked)).await;
    // A watch makes b2 live, so its state is blocked.
    let api = riff::api::Api::new(&server);
    let b2 = uri("b2");
    let _watch = api.watch(&b2).await.unwrap();

    let who = output(riff(&server, dir, None, &["who", "--color", "always"])).await;
    let red = style::ERROR;
    assert!(who.contains(&format!("{red}blocked{red:#}")), "{who:?}");
    // The age of the block grows on a busy machine (#460).
    let start = format!("  {red}waits for a review (");
    let (_, rest) = who.split_once(&start).expect(&who);
    let (age, _) = rest.split_once(&format!("s ago){red:#}\n")).expect(&who);
    assert!(age.parse::<u64>().is_ok(), "{who:?}");
}

/// The line of the session `id` in `riff who`, with each run of spaces
/// as one space and each age in seconds as `Ns`.
async fn row(server: &str, dir: &Path, id: &str) -> String {
    let who = output(riff(server, dir, None, &["who", "--color", "never"])).await;
    let line = who
        .lines()
        .find(|l| l.contains(&format!("({id})")))
        .unwrap_or_else(|| panic!("{id}: {who}"));
    let words: Vec<String> = line
        .split_whitespace()
        .map(|w| {
            let n = w.trim_start_matches('(');
            match n.strip_suffix('s') {
                Some(n) if n.parse::<u64>().is_ok() => format!("{}Ns", &w[..w.len() - n.len() - 1]),
                _ => w.to_owned(),
            }
        })
        .collect();
    words.join(" ")
}

/// A lead that calls only the command line has no watch. It shows with
/// its status, not `offline` (01M48VDGQ5KETKPM4G6TKTC2MB).
#[tokio::test(flavor = "multi_thread")]
async fn a_lead_on_the_command_line_shows_with_its_status() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    output(riff(&server, dir, Some("a1"), &["lead"])).await;
    output(riff(&server, dir, Some("a1"), &["resume", "--riff"])).await;
    output(riff(&server, dir, Some("a1"), &["status", "plan the wave"])).await;
    output(riff(&server, dir, Some("b2"), &["status", "tests"])).await;

    assert_eq!(
        row(&server, dir, "a1").await,
        "mike@pangolin:riff (a1) idle lead monitoring work for Ns Ns ago: plan the wave"
    );
    // A session that is not the lead needs a watch.
    assert_eq!(
        row(&server, dir, "b2").await,
        "mike@pangolin:riff (b2) offline seen Ns ago"
    );
}

/// The state of a lead comes from its facts, not from its claims
/// (01M48VDS8RKJS9HG3KSEYGBFGV): `busy` in a turn, `waiting` for its
/// person, `idle` when nothing goes on. A message does not end the
/// wait, the next prompt of the person does (01M48VDSB4CHQS9P6XVDJ6FMKS,
/// 01M48VDWPDYRPEAXHR1MYDN1M7).
#[tokio::test(flavor = "multi_thread")]
async fn the_state_of_a_lead_comes_from_its_facts() {
    use riff_core::wire::Activity;

    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let api = riff::api::Api::new(&server);
    let a1 = uri("a1");
    output(riff(&server, dir, Some("a1"), &["lead"])).await;
    output(riff(&server, dir, Some("a1"), &["resume", "--riff"])).await;
    let _watch = api.watch(&a1).await.unwrap();
    assert_eq!(
        row(&server, dir, "a1").await,
        "mike@pangolin:riff (a1) idle lead monitoring work for Ns"
    );

    // In a turn: busy, with the work.
    let tool = Activity {
        tool: Some("Bash: deploy the stage".into()),
        turn: true,
        secs: 0,
    };
    api.alive_with(&a1, Some(tool.clone()), None).await.unwrap();
    assert_eq!(
        row(&server, dir, "a1").await,
        "mike@pangolin:riff (a1) busy lead runs Bash: deploy the stage for Ns"
    );

    // It waits for its person: waiting, with the person and the reason.
    let wait = ["blocked", "run riff owner --take"];
    let said = output(riff(&server, dir, Some("a1"), &wait)).await;
    assert!(said.starts_with("You wait for your person"), "{said}");
    let waiting =
        "mike@pangolin:riff (a1) waiting lead waiting for mike: run riff owner --take (Ns ago)";
    assert_eq!(row(&server, dir, "a1").await, waiting);

    // A message of a worker and more work do not end the wait.
    output(riff(&server, dir, Some("b2"), &["tell", "lead", "done"])).await;
    api.alive_with(&a1, Some(tool), None).await.unwrap();
    assert_eq!(row(&server, dir, "a1").await, waiting);

    // The next prompt of the person ends it. The turn ended: idle.
    let ended = Activity {
        tool: None,
        turn: false,
        secs: 0,
    };
    api.alive_with(&a1, Some(ended), Some(0)).await.unwrap();
    assert_eq!(
        row(&server, dir, "a1").await,
        "mike@pangolin:riff (a1) idle lead monitoring work for Ns"
    );
}

/// A long step shows with its age until it is done. A failed step shows
/// its reason and wakes the lead (01M48VDGTD40P8RBZMS0XB5M9N,
/// 01M48VDS663X064YS5ZGCCZSTB).
#[tokio::test(flavor = "multi_thread")]
async fn a_long_step_shows_and_a_failed_step_wakes_the_lead() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let api = riff::api::Api::new(&server);
    output(riff(&server, dir, Some("a1"), &["lead"])).await;
    output(riff(&server, dir, Some("a1"), &["resume", "--riff"])).await;
    let (a1, b2) = (uri("a1"), uri("b2"));
    let _watch = api.watch(&b2).await.unwrap();
    let mut wakes = Box::pin(api.watch(&a1).await.unwrap());

    let said = output(riff(
        &server,
        dir,
        Some("b2"),
        &["step", "start", "live window"],
    ))
    .await;
    assert!(said.starts_with("Your step is now: live window."), "{said}");
    assert_eq!(
        row(&server, dir, "b2").await,
        "mike@pangolin:riff (b2) idle ready for work for Ns live window for Ns"
    );

    let fail = ["step", "fail", "the stage gave 502"];
    let said = output(riff(&server, dir, Some("b2"), &fail)).await;
    assert_eq!(
        said.trim(),
        "Your step failed: the stage gave 502. The lead has the reason."
    );
    assert_eq!(
        row(&server, dir, "b2").await,
        "mike@pangolin:riff (b2) idle ready for work for Ns live window failed Ns ago: the stage gave 502"
    );
    let wake = isolated::in_time(
        Duration::from_secs(30),
        futures::StreamExt::next(&mut wakes),
    );
    let wake = wake.await.unwrap().unwrap().unwrap();
    assert_eq!(
        wake.from.who().session(),
        Some("b2"),
        "the failed step wakes the lead"
    );
    let read = output(riff(&server, dir, Some("a1"), &["read"])).await;
    assert!(
        read.contains("step failed: live window: the stage gave 502"),
        "{read}"
    );

    output(riff(&server, dir, Some("b2"), &["step", "done"])).await;
    assert_eq!(
        row(&server, dir, "b2").await,
        "mike@pangolin:riff (b2) idle ready for work for Ns"
    );
}

/// Each `riff who` command in the `sh` blocks of How It Works is real,
/// and the book shows `--color`.
#[test]
fn the_book_shows_real_who_commands() {
    let page = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let mut in_sh = false;
    let mut commands = Vec::new();
    for line in page.lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && (line == "riff who" || line.starts_with("riff who ")) {
            commands.push(line.split(['|', '>']).next().unwrap().trim().to_owned());
        }
    }
    assert!(
        commands.iter().any(|c| c == "riff who --color never"),
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
