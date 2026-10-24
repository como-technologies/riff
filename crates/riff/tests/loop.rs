//! The loop over real HTTP: an address and a direct message wake a
//! watching session, `tail` shows posts, claims block, and a moved
//! session keeps its watch.

use std::time::Duration;

use crate::common;
use futures::StreamExt;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, Status};

const WAIT: Duration = Duration::from_secs(5);

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

fn to(text: &str) -> Vec<Selector> {
    vec![text.parse().unwrap()]
}

fn mike() -> SessionUri {
    uri("riff://mike@pangolin/como-technologies/riff?session=a1#api")
}

fn brett() -> SessionUri {
    uri("riff://brett@heron/como-technologies/riff?session=b2#tests")
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

#[tokio::test]
async fn addresses_and_direct_messages_wake_a_watching_session() {
    let server = common::start_signed().await;
    let api = |me: &SessionUri| common::api_of(&server, me);
    let (mike, brett, thread) = (mike(), brett(), repo());
    api(&mike).register(&mike).await.unwrap();
    api(&brett).register(&brett).await.unwrap();
    common::name_lead(&server.api("mike", None), &mike).await;
    common::name_lead(&server.api("brett", None), &brett).await;

    let api_brett = api(&brett);
    let mut wakes = Box::pin(api_brett.watch(&brett).await.unwrap());
    let api_brett = api(&brett);
    let mut tail = Box::pin(api_brett.tail(&brett, &thread).await.unwrap());

    // The person of each user named it the lead.
    let (mike_lead, brett_lead) = (mike.clone().with_lead(true), brett.clone().with_lead(true));
    let who = api(&mike).who(&mike, false).await.unwrap();
    assert!(who.iter().any(|s| s.uri == brett_lead && s.live));
    // A lead with no watch is live while it is not gone
    // (01M48VDGQ5KETKPM4G6TKTC2MB).
    assert!(who.iter().any(|s| s.uri == mike_lead && s.live));
    // A call of who counts as a call: a new session is listed.
    let docs = uri("riff://mike@pangolin/como-technologies/riff?session=c3#docs");
    let who = api(&docs).who(&docs, false).await.unwrap();
    assert!(who.iter().any(|s| s.uri == docs && s.idle_secs == 0));

    api(&mike)
        .post(
            &mike,
            Some(&thread),
            &[],
            "@brett in text does not wake",
            Kind::Message,
        )
        .await
        .unwrap();
    let posted = api(&mike)
        .post(
            &mike,
            Some(&thread),
            &to("user=brett"),
            "the API is ready",
            Kind::Message,
        )
        .await
        .unwrap();
    assert_eq!(posted.woken, vec![brett_lead]);
    let wake = isolated::in_time(WAIT, wakes.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(wake.seq, 2, "the post without an address must not wake");
    assert_eq!(wake.from, mike_lead);

    let first = isolated::in_time(WAIT, tail.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(first.message.body, "@brett in text does not wake");

    api(&mike)
        .post(
            &mike,
            None,
            &to("session=b2"),
            "a direct message",
            Kind::Message,
        )
        .await
        .unwrap();
    let wake = isolated::in_time(WAIT, wakes.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(wake.thread.is_direct());

    let unread = api(&brett).read(&brett, &wake.thread, false).await.unwrap();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].message.body, "a direct message");
}

#[tokio::test]
async fn a_moved_session_keeps_its_watch() {
    let server = common::start_signed().await;
    let api = |me: &SessionUri| common::api_of(&server, me);
    let (mike, brett, thread) = (mike(), brett(), repo());
    api(&brett).register(&brett).await.unwrap();
    let api_brett = api(&brett);
    let mut wakes = Box::pin(api_brett.watch(&brett).await.unwrap());

    let moved = uri("riff://brett@heron/como-technologies/riff?session=b2#issue-6");
    api(&moved).register(&moved).await.unwrap();
    let who = api(&moved).who(&moved, false).await.unwrap();
    assert_eq!(who.len(), 1, "a move must not make a second session");

    api(&mike)
        .post(
            &mike,
            Some(&thread),
            &to("worktree=issue-6"),
            "hi",
            Kind::Message,
        )
        .await
        .unwrap();
    let wake = isolated::in_time(WAIT, wakes.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(wake.from.who(), mike.who());
}

#[tokio::test]
async fn a_claim_blocks_a_second_session_and_is_addressable() {
    let server = common::start_signed().await;
    let api = |me: &SessionUri| common::api_of(&server, me);
    let (mike, brett, thread) = (mike(), brett(), repo());
    let person = "riff://mike@pangolin/como-technologies/riff"
        .parse()
        .unwrap();
    api(&person)
        .set_riff(&person, riff_core::wire::RiffState::Running)
        .await
        .unwrap();

    assert!(
        api(&mike)
            .claim(&mike, &thread, "issue-12")
            .await
            .unwrap()
            .granted
    );
    let reply = api(&brett)
        .claim(&brett, &thread, "issue-12")
        .await
        .unwrap();
    assert!(!reply.granted);
    // The refusal of the server names the holder.
    let held = reply.held.expect("the refusal names the holder");
    assert!(held.starts_with(&mike.short()), "{held}");
    assert!(held.contains("holds issue-12"), "{held}");

    let posted = api(&brett)
        .post(
            &brett,
            Some(&thread),
            &to("claim=issue-12"),
            "status?",
            Kind::Message,
        )
        .await
        .unwrap();
    assert_eq!(posted.woken[0].who(), mike.who());

    assert!(
        api(&brett)
            .release(&brett, &thread, "issue-12")
            .await
            .is_err()
    );
    api(&mike)
        .release(&mike, &thread, "issue-12")
        .await
        .unwrap();
    assert!(
        api(&brett)
            .claim(&brett, &thread, "issue-12")
            .await
            .unwrap()
            .granted
    );
}

#[tokio::test]
async fn a_watch_starts_with_a_wake_for_a_missed_message() {
    let server = common::start_signed().await;
    let api = |me: &SessionUri| common::api_of(&server, me);
    let (mike, brett, thread) = (mike(), brett(), repo());
    api(&brett).register(&brett).await.unwrap();
    api(&mike)
        .post(
            &mike,
            Some(&thread),
            &to("user=brett"),
            "while you were away",
            Kind::Message,
        )
        .await
        .unwrap();

    let api_brett = api(&brett);
    let mut wakes = Box::pin(api_brett.watch(&brett).await.unwrap());
    let wake = isolated::in_time(WAIT, wakes.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(wake.seq, 1);

    // After a read, a new watch has nothing to report.
    api(&brett).read(&brett, &thread, false).await.unwrap();
    drop(wakes);
    let api_brett = api(&brett);
    let mut wakes = Box::pin(api_brett.watch(&brett).await.unwrap());
    assert!(
        tokio::time::timeout(Duration::from_millis(300), wakes.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_status_request_wakes_each_session_and_who_shows_each_answer() {
    let server = common::start_signed().await;
    let api = |me: &SessionUri| common::api_of(&server, me);
    let (mike, brett, thread) = (mike(), brett(), repo());
    // Each client takes the sign-in of its user at its first call, so
    // build the second after the first watch.
    let (api_mike, api_brett);
    api_mike = api(&mike);
    let mut mike_wakes = Box::pin(api_mike.watch(&mike).await.unwrap());
    api_brett = api(&brett);
    let mut brett_wakes = Box::pin(api_brett.watch(&brett).await.unwrap());

    // A person asks each session in the repository for its status.
    let person = uri("riff://brett@pangolin");
    let ask = to("repo=como-technologies/riff");
    let posted = api(&person)
        .post(&person, Some(&thread), &ask, "", Kind::Status)
        .await
        .unwrap();
    assert_eq!(posted.woken.len(), 2);

    // Each session wakes once, and the wake says that it is a status
    // request. Each answers with a status, not with a post.
    for (me, wakes, step) in [
        (&mike, &mut mike_wakes, "write the API"),
        (&brett, &mut brett_wakes, "merge"),
    ] {
        let wake = isolated::in_time(WAIT, wakes.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(wake.kind, Kind::Status);
        let messages = api(me).read(me, &thread, false).await.unwrap();
        assert_eq!(messages[0].message.kind, Kind::Status);
        let status = Status { step: step.into() };
        api(me).status(me, &status).await.unwrap();
    }

    let who = api(&person).who(&person, false).await.unwrap();
    let status_of = |me: &SessionUri| {
        let info = who.iter().find(|s| s.uri.who() == me.who()).unwrap();
        info.status.clone().unwrap()
    };
    assert_eq!(status_of(&mike).status.step, "write the API");
    assert_eq!(status_of(&mike).age_secs, 0);
    assert_eq!(status_of(&brett).status.step, "merge");
    // Nobody posted a reply: the thread holds only the request.
    assert_eq!(
        api(&person)
            .read(&person, &thread, true)
            .await
            .unwrap()
            .len(),
        1
    );

    // riff refuses a status that does not fit on one line.
    let bad = Status {
        step: "two\nlines".into(),
    };
    assert!(api(&mike).status(&mike, &bad).await.is_err());
}
