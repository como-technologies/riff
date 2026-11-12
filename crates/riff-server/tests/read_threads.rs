//! `POST /v1/read/threads`: a read with no thread gives one page of each
//! thread of the caller, with at most 20,000 characters in all, and
//! names each thread with more messages (01M76SB9ACMZWMY9Z6CC4WW3HD).

use crate::common;

use std::collections::BTreeMap;

use riff_core::name::ThreadName;
use riff_core::wire::{ReadThreadsReply, ThreadsReply};
use riff_state::state::read::{READ_CHARS, read_len};
use serde_json::{Value, json};

const ANN: &str = "riff://ann@heron/acme/app?session=a";
const BOB: &str = "riff://bob@kite/acme/app?session=b";

async fn call(base: &str, op: &str, body: Value) -> reqwest::Response {
    let reply = common::client()
        .post(format!("{base}/v1/{op}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200, "{op}");
    reply
}

async fn read_threads(base: &str) -> ReadThreadsReply {
    let read = json!({ "me": BOB });
    call(base, "read/threads", read).await.json().await.unwrap()
}

/// The unread count of bob in each of its threads.
async fn unread(base: &str) -> BTreeMap<ThreadName, usize> {
    let reply: ThreadsReply = call(base, "threads", json!({ "me": BOB }))
        .await
        .json()
        .await
        .unwrap();
    reply
        .threads
        .into_iter()
        .map(|t| (t.thread, t.unread))
        .collect()
}

/// 300 messages of 180 characters: 200 in the thread of the repository,
/// 99 in the chat, and one direct message to bob.
#[tokio::test]
async fn a_read_with_no_thread_has_a_bound_and_names_each_thread_with_more() {
    let (_service, base) = common::start(false, &[]).await;
    for me in [ANN, BOB] {
        call(&base, "register", json!({ "me": me })).await;
    }
    call(&base, "join", json!({ "me": BOB, "thread": "chat" })).await;
    for n in 0..299 {
        let thread = if n % 3 == 2 { "chat" } else { "acme/app" };
        let body = format!("{n:03} {}", "x".repeat(176));
        let post = json!({ "me": ANN, "thread": thread, "body": body });
        call(&base, "post", post).await;
    }
    let tell = json!({ "me": ANN, "to": [{ "session": "b" }], "body": "bob?" });
    call(&base, "post", tell).await;
    let before = unread(&base).await;
    assert_eq!(before.values().sum::<usize>(), 300, "{before:?}");

    let reply = read_threads(&base).await;
    assert!(reply.trusted);
    let pages = reply.pages;
    assert!(read_len(&pages) <= READ_CHARS, "{}", read_len(&pages));
    assert!(pages[0].thread.is_direct());
    assert_eq!(pages[0].messages[0].body, "bob?");
    assert_eq!(pages[0].members.len(), 2);
    for page in &pages {
        let more = page.messages.len() < before[&page.thread];
        assert_eq!(page.next.is_some(), more, "{}", page.thread);
    }
    assert!(pages.iter().any(|p| p.next.is_some()));

    // Each read goes on after the last one, until no message is left.
    let mut shown = pages.iter().map(|p| p.messages.len()).sum::<usize>();
    loop {
        let pages = read_threads(&base).await.pages;
        if pages.is_empty() {
            break;
        }
        assert!(read_len(&pages) <= READ_CHARS, "{}", read_len(&pages));
        shown += pages.iter().map(|p| p.messages.len()).sum::<usize>();
    }
    assert_eq!(shown, 300);
    assert_eq!(unread(&base).await.values().sum::<usize>(), 0);
}
