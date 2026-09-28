//! Wake only the sessions that must act (01M3JPMQE6S7YM4HPEVGXWK7ET,
//! 01M3JPMQG9FDB719BC8MDCBNBA). The test replays the posts that a worker
//! saw in Wave 2 against riff-server, over real HTTP. Each post goes
//! once with `to repo=OWNER/REPO` (the old way), and once with the `kind`
//! and `to` that the skill gives for it. The worker gets at most half
//! the wakes, and still wakes for its verify result, and for the
//! question and the request to it.

use riff::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, RiffState};

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// Who sends a post in the fixture.
#[derive(Clone, Copy)]
enum From {
    Lead,
    A,
    B,
}

/// Where the skill sends a post.
#[derive(Clone, Copy)]
enum To {
    /// A note to the repository thread.
    NoteRepo,
    /// A note to the lead of our user, for a "started" or a "done".
    NoteLead,
    /// A verify request: it wakes the lead of the author, which gives it
    /// to a free session.
    Lead,
    /// A verify result: it wakes the author, the holder of the item.
    Claim(&'static str),
    /// A direct message to the worker: a question or a request.
    Worker,
}

/// The posts that a worker saw in Wave 2, with where the skill sends
/// each one now. The worker is `w`, and holds `issue-12`.
const WAVE_2: &[(From, &str, To)] = &[
    (
        From::Lead,
        "Board: Wave 2 starts. Items #10, #11, #12, #13.",
        To::NoteRepo,
    ),
    (From::A, "started: issue-10", To::NoteLead),
    (From::B, "started: issue-11", To::NoteLead),
    (From::Lead, "Board: #14 moves to Wave 3.", To::NoteRepo),
    (
        From::A,
        "verify request: issue-10, PR #40, commit 1a2b3c4",
        To::Lead,
    ),
    (
        From::B,
        "verify result: PASS for issue-10",
        To::Claim("issue-10"),
    ),
    (From::A, "done: issue-10 merged in #40", To::NoteRepo),
    (From::Lead, "request: claim issue-12", To::Worker),
    (From::B, "I took R210 to R212.", To::NoteRepo),
    (From::A, "Do you edit hook.rs for issue-12?", To::Worker),
    (
        From::B,
        "verify request: issue-11, PR #41, commit 5d6e7f8",
        To::Lead,
    ),
    (
        From::A,
        "verify result: PASS for issue-11",
        To::Claim("issue-11"),
    ),
    (From::B, "done: issue-11 merged in #41", To::NoteRepo),
    (From::A, "started: issue-13", To::NoteLead),
    (
        From::B,
        "verify result: PASS for issue-12",
        To::Claim("issue-12"),
    ),
    (
        From::A,
        "verify request: issue-13, PR #43, commit 9a8b7c6",
        To::Lead,
    ),
    (From::A, "done: issue-13 merged in #43", To::NoteRepo),
    (From::Lead, "Board: Wave 2 is done.", To::NoteRepo),
];

/// The posts that must still wake the worker.
const MUST_WAKE: &[&str] = &[
    "request: claim issue-12",
    "Do you edit hook.rs for issue-12?",
    "verify result: PASS for issue-12",
];

struct Riff {
    api: Api,
    lead: SessionUri,
    a: SessionUri,
    b: SessionUri,
    w: SessionUri,
}

impl Riff {
    fn from(&self, from: From) -> &SessionUri {
        match from {
            From::Lead => &self.lead,
            From::A => &self.a,
            From::B => &self.b,
        }
    }
}

async fn riff() -> Riff {
    let api = start_server().await;
    let riff = Riff {
        lead: uri("riff://mike@pangolin/como-technologies/riff?session=l1"),
        a: uri("riff://mike@pangolin/como-technologies/riff?session=a1#issue-10"),
        b: uri("riff://mike@thelio/como-technologies/riff?session=b2#issue-11"),
        w: uri("riff://mike@thelio/como-technologies/riff?session=w3#issue-12"),
        api,
    };
    // The first session of mike is the lead (R176).
    for me in [&riff.lead, &riff.a, &riff.b, &riff.w] {
        riff.api.register(me).await.unwrap();
    }
    riff.api
        .set_riff(&riff.lead, RiffState::Running)
        .await
        .unwrap();
    for (me, item) in [
        (&riff.a, "issue-10"),
        (&riff.b, "issue-11"),
        (&riff.w, "issue-12"),
    ] {
        riff.api.claim(me, &repo(), item).await.unwrap();
    }
    riff
}

fn selector(text: &str) -> Selector {
    text.parse().unwrap()
}

/// Sends each post of the fixture. With `skill` false, each goes to
/// `repo=OWNER/REPO` as a message. Returns the bodies of the posts that
/// woke the worker.
async fn replay(riff: &Riff, skill: bool) -> Vec<&'static str> {
    let mut woke = Vec::new();
    for &(from, body, to) in WAVE_2 {
        let me = riff.from(from);
        let everyone = [selector("repo=como-technologies/riff")];
        let lead = [selector("user=mike,repo=como-technologies/riff,lead=true")];
        let worker = [selector("session=w3")];
        let claim;
        let (thread, to, kind) = match (skill, to) {
            (false, _) => (Some(repo()), &everyone[..], Kind::Message),
            (true, To::NoteRepo) => (Some(repo()), &everyone[..], Kind::Note),
            (true, To::NoteLead) => (Some(repo()), &lead[..], Kind::Note),
            (true, To::Lead) => (Some(repo()), &lead[..], Kind::Message),
            (true, To::Claim(item)) => {
                claim = [selector(&format!("claim={item}"))];
                (Some(repo()), &claim[..], Kind::Message)
            }
            (true, To::Worker) => (None, &worker[..], Kind::Message),
        };
        let posted = riff
            .api
            .post(me, thread.as_ref(), to, body, kind)
            .await
            .unwrap();
        if posted.woken.iter().any(|u| u.who() == riff.w.who()) {
            woke.push(body);
        }
    }
    woke
}

#[tokio::test]
async fn the_skill_halves_the_wakes_of_a_worker() {
    assert!(WAVE_2.len() >= 16);
    let old = replay(&riff().await, false).await;
    let new = replay(&riff().await, true).await;
    assert_eq!(old.len(), WAVE_2.len(), "the old way wakes on each post");
    assert!(
        new.len() * 2 <= old.len(),
        "{} wakes of {}: {new:?}",
        new.len(),
        old.len()
    );
    assert_eq!(new, MUST_WAKE);
}

#[tokio::test]
async fn the_worker_reads_each_note() {
    let riff = riff().await;
    riff.api.inbox(&riff.w, None, false).await.unwrap();
    replay(&riff, true).await;
    let seen: Vec<(Kind, String)> = riff
        .api
        .inbox(&riff.w, None, false)
        .await
        .unwrap()
        .into_iter()
        .flat_map(|inbox| inbox.messages)
        .map(|c| (c.message.kind, c.message.body))
        .collect();
    for &(_, body, to) in WAVE_2 {
        if matches!(to, To::NoteRepo | To::NoteLead) {
            assert!(
                seen.iter().any(|(k, b)| *k == Kind::Note && b == body),
                "the worker did not read {body:?}: {seen:?}"
            );
        }
    }
}
