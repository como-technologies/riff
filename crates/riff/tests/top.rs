//! `riff top`: a live table of each session, its item and its status
//! (01M3NB54P1RBHTA5TKXP8BMY3K). It makes only read calls
//! (01M3NB589WMPRSAR43BSG9SP41).

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use riff::api::Api;
use riff_core::name::SessionUri;
use riff_core::wire::StartReason;

/// The path of each call that the server gets.
type Calls = Arc<Mutex<Vec<String>>>;

async fn start_server() -> (String, Calls) {
    let calls = Calls::default();
    let seen = calls.clone();
    let router = riff_server::router().layer(axum::middleware::map_request(
        move |r: axum::extract::Request| {
            seen.lock().unwrap().push(r.uri().path().to_owned());
            async move { r }
        },
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), calls)
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

/// A dir for `PATH` with `git`, a fake `tmux` that lists one worker
/// pane of the session `c3` on this machine, and a fake `gh` when `gh`
/// is true. The pane does not make `c3` a worker: only the server says
/// who is a worker (01M3NT4M159EHN5W8JRTQ417N4).
fn bin(gh: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = String::from_utf8(Command::new("which").arg("git").output().unwrap().stdout).unwrap();
    std::os::unix::fs::symlink(git.trim(), dir.path().join("git")).unwrap();
    script(dir.path(), "tmux", "echo '%3 c3'");
    if gh {
        script(
            dir.path(),
            "gh",
            r#"echo '[{"number": 12, "title": "Show the wave", "milestone": {"title": "Wave 3"}}]'"#,
        );
    }
    dir
}

fn script(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// `riff ARGS` in `dir` through a pipe, as the agent session `session`
/// of mike, or as the person brett, with `path` as `PATH`.
fn riff(server: &str, dir: &Path, session: Option<&str>, path: &Path, args: &[&str]) -> Command {
    let mut cmd = Isolated::shared().riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", dir)
        .env("PATH", path)
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    String::from_utf8(out.stdout).unwrap()
}

/// Opens a watch for the session `id` of mike on `host`, so that the
/// session is live until the test ends.
async fn live(server: &str, host: &str, id: &str) {
    let uri = format!("riff://mike@{host}/como-technologies/riff?session={id}");
    live_at(server, uri.parse().unwrap()).await;
}

/// Opens a watch for the session `uri`, so that the session is live
/// until the test ends.
async fn live_at(server: &str, uri: SessionUri) {
    let api = Api::new(server);
    let (open, opened) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _watch = api.watch(&uri).await.unwrap();
        open.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    opened.await.unwrap();
}

/// A lead `a1`, a worker `b2` with a claim and a status, and a blocked
/// session `c3`, all of mike and each live. `b2` registers as a worker
/// on the host thelio. Each command runs on the host pangolin.
async fn three_sessions(server: &str, dir: &Path, path: &Path) {
    for (id, args) in [("a1", &["lead"][..]), ("a1", &["resume", "--riff"][..])] {
        output(riff(server, dir, Some(id), path, args)).await;
    }
    let b2: SessionUri = "riff://mike@thelio/como-technologies/riff?session=b2"
        .parse()
        .unwrap();
    Api::new(server).register_as(&b2, true).await.unwrap();
    for (id, args) in [
        ("b2", &["claim", "issue-12"][..]),
        ("b2", &["status", "tests"][..]),
        (
            "c3",
            &["status", "--blocked", "waits for a review", "merge"][..],
        ),
    ] {
        output(riff(server, dir, Some(id), path, args)).await;
    }
    for (host, id) in [("pangolin", "a1"), ("thelio", "b2"), ("pangolin", "c3")] {
        live(server, host, id).await;
    }
}

/// A session `id` of mike on pangolin at `place`, `OWNER/REPO` or
/// `OWNER/REPO#WORKTREE`, with the claim `item` in the thread of its
/// repository. It is live until the test ends.
async fn session_at(server: &str, place: &str, id: &str, item: Option<&str>) {
    let (repo, worktree) = place.split_once('#').unwrap_or((place, ""));
    let fragment = if worktree.is_empty() {
        String::new()
    } else {
        format!("#{worktree}")
    };
    let uri: SessionUri = format!("riff://mike@pangolin/{repo}?session={id}{fragment}")
        .parse()
        .unwrap();
    let api = Api::new(server);
    api.register(&uri).await.unwrap();
    if let Some(item) = item {
        let thread = uri.default_thread().unwrap();
        api.claim(&uri, &thread, item).await.unwrap();
    }
    live_at(server, uri).await;
}

/// The worker `b2` of [`three_sessions`] clears its context: a start
/// with a fresh context, as the start hook sends after `/clear`.
async fn clear_b2(server: &str) {
    let b2: SessionUri = "riff://mike@thelio/como-technologies/riff?session=b2"
        .parse()
        .unwrap();
    let api = Api::new(server);
    api.start(&b2, StartReason::Clear, true).await.unwrap();
}

/// `line` with each time in seconds, for example `0s` or `12s`, as `Ns`:
/// a slow machine can take a second more.
fn secs(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let word_start = start == 0 || !chars[start - 1].is_alphanumeric();
        let word_end = i + 1 >= chars.len() || !chars[i + 1].is_alphanumeric();
        if i > start && word_start && chars.get(i) == Some(&'s') && word_end {
            out.push_str("Ns");
            i += 1;
        } else if i > start {
            out.extend(&chars[start..i]);
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// The lines of the tree, after the header and the board, with each
/// time in seconds as `Ns`.
fn rows(top: &str) -> Vec<String> {
    top.rsplit("\n\n")
        .next()
        .unwrap()
        .lines()
        .map(secs)
        .collect()
}

/// The lines of the session `id`: its first line, then the lines under
/// it.
fn session(top: &str, id: &str) -> Vec<String> {
    let rows = rows(top);
    let head = rows
        .iter()
        .position(|r| r.contains(&format!("─ {id}  ")))
        .unwrap_or_else(|| panic!("{id}: {top}"));
    let under = rows[head + 1..]
        .iter()
        .take_while(|r| !r.contains("─ ") && (r.starts_with(' ') || r.starts_with('│')));
    std::iter::once(&rows[head]).chain(under).cloned().collect()
}

/// The detail lines of the session `id`, with no lead-in.
fn detail(top: &str, id: &str) -> Vec<String> {
    session(top, id)
        .into_iter()
        .skip(1)
        .map(|l| l.trim_start_matches([' ', '│']).to_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn top_once_prints_a_row_for_each_session_blocked_first() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;

    let top = output(riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["top", "--once"],
    ))
    .await;
    assert!(top.starts_with("riff   running\n"), "{top}");
    assert!(
        top.contains("\n\nWave 3 (como-technologies/riff)\n  claimed: #12\n\n"),
        "{top}"
    );
    let rows = rows(&top);
    assert_eq!(
        rows,
        [
            "mike  online",
            "├─ pangolin",
            "│  ├─ c3  riff  blocked",
            "│  │    waits for a review (step: merge, Ns ago)",
            "│  └─ a1  riff  lead  idle",
            "│       monitoring work for Ns",
            "└─ thelio",
            "   └─ b2  riff  worker  busy",
            "        working on #12 Show the wave",
            "        Ns ago: tests",
        ],
        "a local pane is no worker; blocked comes first: {top}"
    );
}

/// A worker that registered on thelio shows `worker` in `riff top` and
/// `riff who` on pangolin (01M3NT4M159EHN5W8JRTQ417N4).
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_of_another_host_shows_worker() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert!(session(&top, "b2")[0].contains("  worker  "), "{top}");

    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    let line = |id: &str| {
        who.lines()
            .find(|l| l.contains(&format!("({id})")))
            .unwrap_or_else(|| panic!("{id}: {who}"))
            .to_owned()
    };
    assert!(line("b2").starts_with("mike@thelio:riff (b2)"), "{who}");
    assert!(line("b2").contains("  worker  "), "{who}");
    assert!(line("b2").contains("  busy  "), "{who}");
    assert!(line("b2").contains("  working on #12  "), "{who}");
    assert!(!line("c3").contains("worker"), "{who}");
    assert!(line("a1").contains("  you lead"), "{who}");
}

#[tokio::test(flavor = "multi_thread")]
async fn top_never_prints_color_to_a_pipe() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;

    let pipe = output(riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["top", "--once"],
    ))
    .await;
    assert!(!pipe.contains('\x1b'), "{pipe:?}");
    let mut never = riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["top", "--once", "--color", "never"],
    );
    never.env("CLICOLOR_FORCE", "1");
    let never = output(never).await;
    assert!(!never.contains('\x1b'), "{never:?}");
    let always = ["top", "--once", "--color", "always"];
    let always = output(riff(&server, dir, Some("a1"), bin.path(), &always)).await;
    let red = riff::style::ERROR;
    assert!(
        always.contains(&format!("{red}blocked{red:#}")),
        "{always:?}"
    );
    let green = riff::style::GOOD;
    assert!(
        always.contains(&format!("{green}busy{green:#}")),
        "{always:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn with_no_gh_the_row_still_prints() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(false);
    three_sessions(&server, dir, bin.path()).await;

    let top = output(riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["top", "--once"],
    ))
    .await;
    assert!(!top.contains("Wave 3"), "{top}");
    assert_eq!(
        detail(&top, "b2"),
        ["working on #12", "Ns ago: tests"],
        "{top}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn top_makes_only_read_calls() {
    let (server, calls) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    calls.lock().unwrap().clear();

    // As the person and as an agent session, once and live.
    output(riff(&server, dir, None, bin.path(), &["top", "--once"])).await;
    output(riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["top", "--once"],
    ))
    .await;
    let mut live = riff(&server, dir, None, bin.path(), &["top"])
        .spawn()
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    live.kill().unwrap();
    live.wait().unwrap();

    let calls = calls.lock().unwrap().clone();
    assert!(
        calls.iter().filter(|c| *c == "/v1/who").count() >= 4,
        "{calls:?}"
    );
    let reads = ["/v1/riff", "/v1/who", "/v1/tail"];
    for call in &calls {
        assert!(reads.contains(&call.as_str()), "{call} in {calls:?}");
    }
}

/// Each `riff top` command in the `sh` blocks of How It Works is real.
#[test]
fn the_book_shows_real_top_commands() {
    let page = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let mut in_sh = false;
    let mut commands = Vec::new();
    for line in page.lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && (line == "riff top" || line.starts_with("riff top ")) {
            commands.push(line.split(['|', '>']).next().unwrap().trim().to_owned());
        }
    }
    for want in ["riff top", "riff top --once"] {
        assert!(commands.iter().any(|c| c == want), "{want}: {commands:?}");
    }
    for command in commands {
        Isolated::shared()
            .assert_riff()
            .args(command.split_whitespace().skip(1))
            .arg("--help")
            .assert()
            .success();
    }
}

/// The example table of "See what each session does" in How It Works
/// has the form of the output: the wave line names its repository, and
/// each session line names its place after the session ID. It shows
/// two repositories and a worktree (01M3WNHCD659FH3Z5VYYH69WWR).
#[test]
fn the_book_example_names_the_place_of_each_session() {
    let page = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let part = page
        .split("\n## See what each session does\n")
        .nth(1)
        .unwrap();
    let example = part.split("```text\n").nth(1).unwrap();
    let example = example.split("```").next().unwrap();
    assert!(
        example.contains("\nWave 3 (como-technologies/riff)\n"),
        "{example}"
    );
    let mut places = Vec::new();
    for line in example.lines().filter(|l| l.contains("─ ")) {
        let words: Vec<&str> = line.split("─ ").nth(1).unwrap().split("  ").collect();
        // A host line has one word. A session line starts with its ID.
        if words.len() > 1 {
            assert_eq!(words[0].len(), 8, "{line}");
            let repo = words[1].split('#').next().unwrap();
            assert!(["riff", "strata"].contains(&repo), "{line}");
            places.push(words[1]);
        }
        assert!(line.chars().count() <= 80, "{line}");
    }
    for want in ["riff", "riff#issue-7", "strata#issue-88"] {
        assert!(places.contains(&want), "{want}: {places:?}");
    }
    assert!(
        example.contains("  strata#issue-88  lead  "),
        "a lead line names its repository: {example}"
    );
}

/// A pause makes each live session `paused`, with the step it stopped
/// at. A stale block does not come first (01M3QB6CJ1XCQG5B1BVR8AF3B4).
#[tokio::test(flavor = "multi_thread")]
async fn a_pause_shows_paused_and_the_step() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    output(riff(&server, dir, Some("a1"), bin.path(), &["pause"])).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    for id in ["a1", "b2", "c3"] {
        assert!(session(&top, id)[0].ends_with("  paused"), "{top}");
    }
    // The pause is the one of the repository, and top says who set it
    // (01M3XAHZJAF6YVDJ7WX74X8RBX).
    assert!(
        top.contains("como-technologies/riff by the session mike/a1"),
        "{top}"
    );
    assert_eq!(
        detail(&top, "b2"),
        ["working on #12 Show the wave", "stopped at: tests"],
        "{top}"
    );
    assert_eq!(detail(&top, "c3"), ["stopped at: merge"], "{top}");
    let rows = rows(&top);
    assert!(
        rows[2].starts_with("│  ├─ a1  "),
        "a stale block does not come first: {top}"
    );
}

/// A worker that releases its claim is `idle`, ready for work, with no
/// old step (01M3Q551WCMPQRCNJ8FXQEBFY4, 01M3QB6CJ1XCQG5B1BVR8AF3B4).
/// After the release of its last claim it shows `must clear`, until it
/// starts with a fresh context. Then it shows the time since that start
/// (01M3X9XC99KY4RQY36A7CYWY11).
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_with_no_claim_shows_must_clear_then_idle_not_its_old_step() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    let release = ["release", "issue-12"];
    let released = output(riff(&server, dir, Some("b2"), bin.path(), &release)).await;
    // The reply to the release carries the ask to clear.
    assert!(
        released.contains("clear your context before your next claim"),
        "{released}"
    );

    let top = ["top", "--once"];
    let before = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    let head = &session(&before, "b2")[0];
    assert!(head.ends_with("  worker  must clear"), "{before}");
    let must = ["must clear its context before its next claim"];
    assert_eq!(detail(&before, "b2"), must, "{before}");
    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    let b2 = who.lines().find(|l| l.contains("(b2)")).unwrap();
    assert!(b2.contains("  must clear  worker  "), "{who}");
    // A claim before the clear is refused.
    let mut claim = riff(&server, dir, Some("b2"), bin.path(), &["claim", "issue-12"]);
    let refused = claim.output().unwrap();
    assert!(!refused.status.success());
    let why = String::from_utf8_lossy(&refused.stderr).into_owned();
    let text = "clear your context first: type /clear, or run riff workers next";
    assert!(why.contains(text), "{why}");

    clear_b2(&server).await;
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert!(session(&top, "b2")[0].ends_with("  worker  idle"), "{top}");
    let ready = ["ready for work for Ns", "fresh start Ns ago"];
    assert_eq!(detail(&top, "b2"), ready, "{top}");
    assert!(
        top.contains("\nWave 3 (como-technologies/riff)\n  free: #12\n"),
        "{top}"
    );
}

/// Each session line names the repository and the worktree of its
/// session, as `riff who` does, and each line fits in 80 columns. A
/// lead line names the repository of its lead. The wave line names its
/// repository, and a claim in another repository is not on its board
/// (01M3WNHCD659FH3Z5VYYH69WWR).
#[tokio::test(flavor = "multi_thread")]
async fn sessions_of_two_repositories_show_their_repository_and_worktree() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    let release = ["release", "issue-12"];
    output(riff(&server, dir, Some("b2"), bin.path(), &release)).await;
    clear_b2(&server).await;
    // The first session of mike in strata is its lead.
    let strata = "como-technologies/strata";
    session_at(&server, strata, "d4", Some("issue-12")).await;
    session_at(
        &server,
        &format!("{strata}#issue-88"),
        "e5",
        Some("issue-88"),
    )
    .await;
    session_at(&server, "como-technologies/riff#issue-12", "f6", None).await;

    let top = ["top", "--once", "--color", "never"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert!(!top.contains('\x1b'), "{top:?}");
    for line in top.lines() {
        assert!(line.chars().count() <= 80, "{line:?} in\n{top}");
    }
    let heads: Vec<String> = ["a1", "c3", "d4", "e5", "f6", "b2"]
        .iter()
        .map(|id| session(&top, id)[0].clone())
        .collect();
    assert_eq!(
        heads,
        [
            "│  ├─ a1  riff  lead  idle",
            "│  ├─ c3  riff  blocked",
            "│  ├─ d4  strata  lead  busy",
            "│  ├─ e5  strata#issue-88  busy",
            "│  └─ f6  riff#issue-12  idle",
            "   └─ b2  riff  worker  idle",
        ],
        "{top}"
    );
    assert!(
        top.contains("\n\nWave 3 (como-technologies/riff)\n  free: #12\n\n"),
        "the claims of strata are not on the board of riff: {top}"
    );
    assert_eq!(
        detail(&top, "d4"),
        ["working on #12"],
        "no title of riff for an issue of strata: {top}"
    );

    // `riff who` names the same places.
    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    for name in [
        "mike@pangolin:strata (d4)",
        "mike@pangolin:strata#issue-88 (e5)",
        "mike@pangolin:riff#issue-12 (f6)",
    ] {
        assert!(who.contains(name), "{name}: {who}");
    }
}

/// When the repositories of the sessions have more than one owner, each
/// session line has `OWNER/REPO` (01M3WNHCD659FH3Z5VYYH69WWR).
#[tokio::test(flavor = "multi_thread")]
async fn two_owners_show_the_owner_of_each_repository() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    session_at(&server, "acme/strata#issue-88", "d4", Some("issue-88")).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert_eq!(
        session(&top, "d4")[0],
        "│  └─ d4  acme/strata#issue-88  lead  busy",
        "{top}"
    );
    assert_eq!(
        session(&top, "a1")[0],
        "│  ├─ a1  como-technologies/riff  lead  idle",
        "{top}"
    );
}

/// A session with no claim and no status is `idle` in `riff top` and in
/// `riff who`, with no status call. An idle lead monitors work. An idle
/// worker is ready for work (01M3QB6CJ1XCQG5B1BVR8AF3B4).
#[tokio::test(flavor = "multi_thread")]
async fn a_session_with_no_status_is_idle() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert_eq!(detail(&top, "a1"), ["monitoring work for Ns"], "{top}");
    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    let a1 = who.lines().find(|l| l.contains("(a1)")).unwrap();
    assert!(a1.contains("  idle  "), "{who}");
    assert!(secs(a1).ends_with("  monitoring work for Ns"), "{who}");

    let release = ["release", "issue-12"];
    output(riff(&server, dir, Some("b2"), bin.path(), &release)).await;
    clear_b2(&server).await;
    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    let ready = ["ready for work for Ns", "fresh start Ns ago"];
    assert_eq!(detail(&top, "b2"), ready, "{top}");
    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    let b2 = who.lines().find(|l| l.contains("(b2)")).unwrap();
    let ready = "  ready for work for Ns  fresh start Ns ago";
    assert!(secs(b2).ends_with(ready), "{who}");
}

/// With 12 sessions, long titles and long statuses, `riff top --once`
/// in a pipe fits in 80 columns: riff cuts each wider line with `…`
/// (01M3QA8EZHX5B8C9CKF8Q3154X).
#[tokio::test(flavor = "multi_thread")]
async fn twelve_sessions_fit_in_80_columns() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(false);
    let long = "The owner check does not drop an owner that has a live session on another host";
    script(
        bin.path(),
        "gh",
        &format!(
            r#"echo '[{{"number": 12, "title": "{long}", "milestone": {{"title": "Wave 3: A long name"}}}}, {{"number": 13, "title": "{long}", "milestone": {{"title": "Wave 3: A long name"}}}}]'"#
        ),
    );
    output(riff(&server, dir, Some("s00"), bin.path(), &["lead"])).await;
    output(riff(&server, dir, Some("s00"), bin.path(), &["resume", "--riff"])).await;
    for n in 0..12 {
        let id = format!("s{n:02}");
        let uri: SessionUri = format!("riff://mike@thelio/como-technologies/riff?session={id}")
            .parse()
            .unwrap();
        Api::new(&server)
            .register_as(&uri, n % 2 == 0)
            .await
            .unwrap();
        let claim = match n {
            0 => Some("issue-12"),
            3 => Some("verify-issue-12"),
            6 => Some("issue-13"),
            _ => None,
        };
        if let Some(claim) = claim {
            output(riff(&server, dir, Some(&id), bin.path(), &["claim", claim])).await;
        }
        live(&server, "thelio", &id).await;
        let status = [
            "status",
            "--blocked",
            long,
            "waits for a verify of the pull request",
        ];
        let step = ["status", long];
        let args: &[&str] = if n % 4 == 0 { &status } else { &step };
        output(riff(&server, dir, Some(&id), bin.path(), args)).await;
    }

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("s00"), bin.path(), &top)).await;
    for line in top.lines() {
        assert!(line.chars().count() <= 80, "{line:?} in\n{top}");
    }
    assert!(top.lines().any(|l| l.ends_with('…')), "{top}");
    assert!(top.contains("\n  claimed: #13\n  verify: #12\n"), "{top}");
    assert!(
        top.contains("│    working on #12 The owner check does not"),
        "{top}"
    );
    for n in 0..12 {
        assert!(!session(&top, &format!("s{n:02}")).is_empty(), "{top}");
    }
}
