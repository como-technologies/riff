//! The `read` tool with no thread: its text has at most 20,000
//! characters, and names each thread with more messages
//! (01M76SB9ACMZWMY9Z6CC4WW3HD). A lead reads of the thread of another
//! repository only the messages that pick it (01M76SBCXR4A21D49WVKBGWMRY).

use riff::text;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, RiffState};

use crate::common;

const POSTER: &str = "riff://brett@heron/como-technologies/riff?session=a1";
const READER: &str = "riff://bob@kite/como-technologies/riff?session=b1";
const LEAD: &str = "riff://mike@thelio/como-technologies/riff?session=l1";
const BRETT: &str = "riff://brett@heron/como-technologies/strata?session=s1";

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

fn thread(text: &str) -> ThreadName {
    text.parse().unwrap()
}

fn selector(text: &str) -> Selector {
    text.parse().unwrap()
}

/// 300 signed messages of 180 characters of brett to bob in two threads: the
/// text of each read with no thread has at most 20,000 characters and
/// names each thread with more. The reads together show each message.
#[tokio::test]
async fn the_read_tool_with_no_thread_gives_at_most_20000_characters() {
    let server = common::start_signed().await;
    let (ann, bob) = (uri(POSTER), uri(READER));
    let (repo, docs) = (
        thread("como-technologies/riff"),
        thread("como-technologies/docs"),
    );
    for me in [&ann, &bob] {
        let api = common::api_of(&server, me);
        api.register(me).await.unwrap();
        api.join(me, &docs).await.unwrap();
    }
    let api = common::api_of(&server, &ann);
    for n in 0..300 {
        let thread = if n % 2 == 0 { &repo } else { &docs };
        let body = format!("{n:03} {}", "x".repeat(176));
        api.post(&ann, Some(thread), &[], &body, Kind::Message)
            .await
            .unwrap();
    }
    let reader = common::api_of(&server, &bob);
    let mut shown = 0;
    loop {
        let inbox = reader.inbox_page(&bob, None, false, None).await.unwrap();
        if inbox.is_empty() {
            break;
        }
        let read = text::inbox(&inbox, &bob);
        assert!(read.chars().count() <= 20_000, "{}", read.chars().count());
        for page in inbox.iter().filter(|i| i.next.is_some()) {
            let name = format!("Read the thread {}", page.thread);
            assert!(read.contains(&name), "{name}:\n{read}");
        }
        shown += read.matches("] brett@heron").count();
    }
    assert_eq!(shown, 300);
}

/// The lead of mike in riff is a member of the thread of strata. The
/// `read` tool and `riff read` show it only the posts of brett there that
/// pick it: a note to the lead of mike. Each post in its own thread
/// shows.
#[tokio::test]
async fn a_lead_reads_of_another_repository_only_the_messages_that_pick_it() {
    let server = common::start_signed().await;
    let (lead, brett) = (uri(LEAD), uri(BRETT));
    let (repo, strata) = (
        thread("como-technologies/riff"),
        thread("como-technologies/strata"),
    );
    let api = common::api_of(&server, &lead);
    api.register(&lead).await.unwrap();
    api.join(&lead, &strata).await.unwrap();
    common::name_lead(&server.api("mike", None), &lead).await;
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    let brett_api = common::api_of(&server, &brett);
    brett_api.register(&brett).await.unwrap();
    let posts = [
        (&strata, "repo=como-technologies/strata", "Board of strata."),
        (
            &strata,
            "user=brett,repo=como-technologies/strata,lead=true",
            "verify request to the lead of brett",
        ),
        (
            &strata,
            "user=mike,lead=true",
            "A note to the lead of mike.",
        ),
        (&repo, "repo=como-technologies/riff", "A note to riff."),
    ];
    for (thread, to, body) in posts {
        brett_api
            .post(&brett, Some(thread), &[selector(to)], body, Kind::Note)
            .await
            .unwrap();
    }
    let api = common::api_of(&server, &lead);
    let threads = api.threads(&lead).await.unwrap();
    let unread = |t: &ThreadName| threads.iter().find(|i| i.thread == *t).unwrap().unread;
    assert_eq!((unread(&strata), unread(&repo)), (1, 1));

    let read = text::inbox(
        &api.inbox_page(&lead, None, false, None).await.unwrap(),
        &lead,
    );
    assert!(read.contains("A note to the lead of mike."), "{read}");
    assert!(read.contains("A note to riff."), "{read}");
    assert!(!read.contains("Board of strata."), "{read}");
    assert!(!read.contains("verify request"), "{read}");

    // `riff read` reads each page of each thread with the same rule.
    common::api_of(&server, &brett)
        .post(
            &brett,
            Some(&strata),
            &[],
            "Board of strata again.",
            Kind::Note,
        )
        .await
        .unwrap();
    let api = common::api_of(&server, &lead);
    let read = text::inbox(&api.inbox(&lead, None, false).await.unwrap(), &lead);
    assert_eq!(read, "No unread messages.");
    // A read with all gives each kept message.
    let all = api.inbox(&lead, Some(&strata), true).await.unwrap();
    assert_eq!(all[0].messages.len(), 4);
}
