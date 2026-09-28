//! The `read` text of a fixture riff: 14 messages from 3 sessions in 2
//! threads, with full URIs. A read gives no own posts
//! (01M3JPK82PN4F706MCHDH771MW), and each sender is short
//! (01M3JPK85FT5CCQPF3WDCXSMDF).

use riff::api::Api;
use riff::text;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, RiffState};

/// The length of the `read` text of the fixture before the change
/// (riff d792af7).
const BEFORE: usize = 3337;

const READER: &str = "riff://sandman@thelio/como-technologies/riff?session=6072f384-d57d-463c-a837-6df28bc9bc8a&claim=issue-73#issue-73";
const LEAD: &str = "riff://sandman@thelio/como-technologies/riff?session=2a880834-3707-4672-ba4a-50438db97e1f&lead=true";
const WORKER: &str = "riff://sandman@thelio/como-technologies/riff?session=686bf974-0a86-47f2-a81f-3f5174c619cf&claim=issue-80&claim=issue-84#issue-84";

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// Posts the fixture. Gives the reader and its threads.
async fn fixture(api: &Api) -> SessionUri {
    let (reader, lead, worker) = (uri(READER), uri(LEAD), uri(WORKER));
    let repo: ThreadName = "como-technologies/riff".parse().unwrap();
    let docs: ThreadName = "como-technologies/docs".parse().unwrap();
    // The first session of the user is the lead.
    for me in [&lead, &reader, &worker] {
        api.register(me).await.unwrap();
        api.join(me, &repo).await.unwrap();
        api.join(me, &docs).await.unwrap();
    }
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    api.claim(&reader, &repo, "issue-73").await.unwrap();
    api.claim(&worker, &repo, "issue-80").await.unwrap();
    api.claim(&worker, &repo, "issue-84").await.unwrap();
    let all = [Selector {
        repo: Some("como-technologies/riff".into()),
        ..Selector::default()
    }];
    let posts: [(&SessionUri, &ThreadName, &str); 13] = [
        (
            &lead,
            &repo,
            "Board: Wave 4 starts now. Items: #93, #88, #87, #84, #80, #73, #72.",
        ),
        (
            &reader,
            &repo,
            "Started issue-73. Branch worktree-issue-73, worktree .claude/worktrees/issue-73.",
        ),
        (
            &worker,
            &repo,
            "verify request: issue-84, PR #98, branch worktree-issue-84, commit 25f6da3",
        ),
        (&lead, &repo, "Board: new item #99, Wave 5, needs #87."),
        (
            &reader,
            &repo,
            "verify request: issue-87, PR #103, branch worktree-issue-87, commit f4ce17a",
        ),
        (
            &worker,
            &repo,
            "verify result: PASS for issue-87, PR #103, commit f4ce17a.",
        ),
        (
            &lead,
            &docs,
            "The book builds again. Check the links of each page.",
        ),
        (&reader, &docs, "The links of how-it-works.md pass."),
        (&worker, &docs, "The links of development.md pass."),
        (
            &lead,
            &repo,
            "The riff is running again. Go on from where you stopped.",
        ),
        (
            &worker,
            &repo,
            "verify request: issue-80, PR #104, branch worktree-issue-80, commit c68f782",
        ),
        (
            &reader,
            &repo,
            "verify result: PASS for issue-80, PR #104, commit c68f782.",
        ),
        (
            &lead,
            &docs,
            "Thanks. Keep the development.md section until #89.",
        ),
    ];
    for (me, thread, body) in posts {
        api.post(me, Some(thread), &all, body, Kind::Message)
            .await
            .unwrap();
    }
    reader
}

#[tokio::test]
async fn a_read_is_at_most_half_as_long_as_before() {
    let api = start_server().await;
    let reader = fixture(&api).await;
    let read = text::inbox(&api.inbox(&reader, None, false).await.unwrap(), &reader);
    assert!(
        read.len() * 2 <= BEFORE,
        "{} of {BEFORE}:\n{read}",
        read.len()
    );
}

/// A read gives no own posts, and the unread counts leave them out.
/// `all` gives them.
#[tokio::test]
async fn a_read_gives_no_own_posts() {
    let api = start_server().await;
    let reader = fixture(&api).await;
    let threads = api.threads(&reader).await.unwrap();
    let unread: usize = threads.iter().map(|t| t.unread).sum();
    assert_eq!(unread, 10);
    let read = text::inbox(&api.inbox(&reader, None, false).await.unwrap(), &reader);
    assert!(!read.contains("(6072f384)"), "{read}");
    assert_eq!(read.matches("] sandman@thelio").count(), 10, "{read}");

    let all = text::inbox(&api.inbox(&reader, None, true).await.unwrap(), &reader);
    assert_eq!(all.matches("(6072f384)").count(), 4, "{all}");
}

/// A reader still sees which session sent each message, and whether it
/// is the lead and verified.
#[tokio::test]
async fn a_read_shows_the_sender_the_lead_and_the_mark() {
    let api = start_server().await;
    let reader = fixture(&api).await;
    let read = text::inbox(&api.inbox(&reader, None, false).await.unwrap(), &reader);
    assert!(
        read.contains(
            "[2] sandman@thelio:riff (2a880834) lead=true to all (verified): Board: Wave 4 starts now."
        ),
        "{read}"
    );
    assert!(
        read.contains("sandman@thelio:riff#issue-84 (686bf974) to all (verified): verify request"),
        "{read}"
    );
}

/// `tell` takes the start of a session ID, as `read` shows it
/// (01M3JPK885GPD16FPK7D05R2RC).
#[tokio::test]
async fn tell_takes_the_start_of_a_session_id() {
    let api = start_server().await;
    let reader = fixture(&api).await;
    let posted = api.tell(&reader, "686bf974", "thanks").await.unwrap();
    assert_eq!(posted.woken.len(), 1, "{posted:?}");
    assert!(posted.woken[0].to_string().contains("686bf974-0a86"));

    // "6" starts 6072f384 and 686bf974.
    let error = api.tell(&reader, "6", "thanks").await.unwrap_err();
    assert!(error.to_string().contains("more than one"), "{error}");
}
