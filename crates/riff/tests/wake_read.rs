//! One read after a wake shows the message of the wake, also behind
//! older pages (01M588J4XNV1TXEPMGTKRVY9VK), over real HTTP. The
//! session `w1` has 200 unread messages in `chat` and one message to it
//! at the end. `riff watch --once` runs as a real process: it wakes
//! once. The read of the `read` tool with no thread shows the message
//! before the first page, and the next watch does not wake.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use isolated::Isolated;
use riff_client::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, RiffState, Wake};

/// How long a watch may take to start, or to end after its wake.
const WAIT: Duration = Duration::from_secs(20);

/// How long a live watch must run with no wake.
const QUIET: Duration = Duration::from_secs(2);

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

fn uri(id: &str) -> SessionUri {
    format!("riff://mike@pangolin/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

fn chat() -> ThreadName {
    "chat".parse().unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// `riff watch --once` of `w1` in `clone`.
fn watch(api: &Api, clone: &Path) -> Child {
    Isolated::shared()
        .riff()
        .args(["watch", "--once"])
        .current_dir(clone)
        .env("RIFF_HOME", clone.join(".riff"))
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SERVER", api.base())
        .env("RIFF_SESSION", "w1")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("RIFF_WORKER")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

/// The output of the watch `child`, when it ends in time.
async fn woke(child: Child) -> String {
    let out = tokio::task::spawn_blocking(move || child.wait_with_output().unwrap());
    let out = tokio::time::timeout(WAIT, out)
        .await
        .expect("the watch ends with a wake")
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// True when `who` shows `w1` as live: it has an open watch.
async fn watches(api: &Api) -> bool {
    api.who(&uri("x0"), false)
        .await
        .unwrap()
        .into_iter()
        .any(|s| s.uri.who().session() == Some("w1") && s.live)
}

#[tokio::test(flavor = "multi_thread")]
async fn one_read_after_a_wake_ends_the_wakes_of_its_message() {
    let api = start_server().await;
    let (lead, w1) = (uri("l1"), uri("w1"));
    // The first session of mike is the lead (R176).
    api.register(&lead).await.unwrap();
    api.register(&w1).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    api.join(&w1, &chat()).await.unwrap();
    api.inbox(&w1, None, false).await.unwrap();
    for n in 0..200 {
        let body = format!("old {n}");
        api.post(&lead, Some(&chat()), &[], &body, Kind::Message)
            .await
            .unwrap();
    }
    let to = [Selector::session("w1")];
    let posted = api
        .post(&lead, Some(&chat()), &to, "w1, a question", Kind::Message)
        .await
        .unwrap();
    let clone = tempfile::tempdir().unwrap();
    git(clone.path(), &["init", "-q"]);
    let origin = "https://github.com/como-technologies/riff.git";
    git(clone.path(), &["remote", "add", "origin", origin]);

    // One wake, at once.
    let out = woke(watch(&api, clone.path())).await;
    let wake = Wake {
        thread: chat(),
        seq: posted.seq,
        from: lead.clone(),
        kind: Kind::Message,
    };
    assert!(out.contains(&riff::text::wake_line(&wake)), "{out}");

    // One read of the `read` tool with no thread shows the message
    // before the first page.
    let inbox = api.inbox_page(&w1, None, false, None).await.unwrap();
    let text = riff::text::inbox(&inbox, &w1);
    let head = text.find(riff::text::WOKEN_HEAD).expect(&text);
    let question = text.find("w1, a question").expect(&text);
    let page = text.find("old ").expect(&text);
    assert!(head < question && question < page, "{text}");
    assert!(inbox.iter().any(|t| t.next.is_some()), "older pages follow");

    // The next watch does not wake.
    let mut child = watch(&api, clone.path());
    let span = isolated::Span::start();
    while !watches(&api).await {
        assert!(child.try_wait().unwrap().is_none(), "the watch woke again");
        assert!(span.within(WAIT), "the watch does not start");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(QUIET).await;
    let ended = child.try_wait().unwrap();
    child.kill().unwrap();
    let _ = child.wait();
    assert!(ended.is_none(), "the watch woke again: {ended:?}");
}
