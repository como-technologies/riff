//! The lead of the project conducts the workers of each person and of
//! each host (01M77XMS645VZNTP22DTCB7RXE, 01M77XMS8R4B8B0DQCK70HBAEW,
//! R231), over real HTTP: it gives two workers two items, each claims its
//! item and reports back, and a blocked worker gets a new item. A worker
//! of another person tells the lead of the project, and the verified
//! request of the lead reaches it.

use crate::common::{self, Signed};
use riff_client::api::{Api, Checked};
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Kind, RiffState, Status};

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// The lead of the project, a session of brett; two workers of mike on
/// two hosts; a worker of brett.
struct Riff {
    server: Signed,
    lead: SessionUri,
    a: SessionUri,
    b: SessionUri,
    c: SessionUri,
}

async fn riff() -> Riff {
    let server = common::start_signed().await;
    let riff = Riff {
        lead: uri("riff://brett@heron/como-technologies/riff?session=l1"),
        a: uri("riff://mike@pangolin/como-technologies/riff?session=a1#a"),
        b: uri("riff://mike@thelio/como-technologies/riff?session=b2#b"),
        c: uri("riff://brett@heron/como-technologies/riff?session=c3#c"),
        server,
    };
    let api = |me: &SessionUri| common::api_of(&riff.server, me);
    for me in [&riff.lead, &riff.a, &riff.b, &riff.c] {
        api(me).register(me).await.unwrap();
    }
    // The person of brett names the one lead of the project
    // (01M5NJAXTQ0YKN7Q7XN0A7ZMPF).
    let person = riff.server.api("brett", None);
    common::name_lead(&person, &riff.lead).await;
    // A new riff is paused. The lead resumes it.
    api(&riff.lead)
        .set_riff(&riff.lead, RiffState::Running)
        .await
        .unwrap();
    for me in [&riff.lead, &riff.a, &riff.b, &riff.c] {
        api(me).inbox(me, None, false).await.unwrap();
    }
    riff
}

/// The unread direct messages to `me`, from other sessions.
async fn direct(api: &Api, me: &SessionUri) -> Vec<Checked> {
    api.inbox(me, None, false)
        .await
        .unwrap()
        .into_iter()
        .filter(|inbox| inbox.thread.is_direct())
        .flat_map(|inbox| inbox.messages)
        .filter(|c| c.message.from.who() != me.who())
        .collect()
}

/// The sender ID and the body of each message of `messages`.
fn bodies(messages: &[Checked]) -> Vec<(String, String)> {
    messages
        .iter()
        .map(|c| {
            let from = c.message.from.who().session().unwrap().to_owned();
            (from, c.message.body.clone())
        })
        .collect()
}

fn id(me: &SessionUri) -> &str {
    me.who().session().unwrap()
}

#[tokio::test]
async fn the_lead_gives_the_workers_of_each_person_items_and_each_reports_back() {
    let Riff {
        server, lead, a, b, ..
    } = riff().await;
    let api = |me: &SessionUri| common::api_of(&server, me);

    // The lead asks each session of the project for its status, of each
    // person, then gives two workers of mike one item each.
    let ask = api(&lead)
        .post(
            &lead,
            Some(&repo()),
            &["repo=como-technologies/riff".parse().unwrap()],
            "",
            Kind::Status,
        )
        .await
        .unwrap();
    assert_eq!(ask.woken.len(), 3, "the workers of mike and of brett wake");
    for (me, item) in [(&a, "issue-12"), (&b, "issue-7")] {
        let posted = api(&lead)
            .tell(&lead, id(me), &format!("request: claim {item}"))
            .await
            .unwrap();
        assert_eq!(posted.woken[0].who(), me.who());
    }

    // Each worker reads the request, claims its item and reports back to
    // the lead of the project, a session of brett.
    for (me, item) in [(&a, "issue-12"), (&b, "issue-7")] {
        let requests = bodies(&direct(&api(me), me).await);
        assert_eq!(
            requests,
            [(id(&lead).to_owned(), format!("request: claim {item}"))]
        );
        assert!(api(me).claim(me, &repo(), item).await.unwrap().granted);
        let status = Status {
            step: format!("{item}: started"),
        };
        api(me).status(me, &status).await.unwrap();
        let report = api(me)
            .tell(me, "lead", &format!("started {item}"))
            .await
            .unwrap();
        assert_eq!(report.woken[0].who(), lead.who());
    }

    // The lead sees both reports, and who shows each claim and status.
    let reports = direct(&api(&lead), &lead).await;
    assert_eq!(reports.len(), 2);
    let who = api(&lead).who(&lead, false).await.unwrap();
    for (me, item) in [(&a, "issue-12"), (&b, "issue-7")] {
        let s = who.iter().find(|s| s.uri.who() == me.who()).unwrap();
        assert_eq!(s.uri.claims(), [item]);
        assert_eq!(
            s.status.as_ref().unwrap().status.step,
            format!("{item}: started")
        );
    }
}

#[tokio::test]
async fn a_blocked_worker_tells_the_lead_and_gets_a_new_item() {
    let Riff {
        server, lead, b, ..
    } = riff().await;
    let api = |me: &SessionUri| common::api_of(&server, me);
    api(&b).claim(&b, &repo(), "issue-7").await.unwrap();
    // One command: the block, and the message that wakes the lead.
    assert_eq!(
        api(&b).blocked(&b, "issue-7 needs issue-5").await.unwrap(),
        riff_client::api::Told::Lead
    );

    let reports = bodies(&direct(&api(&lead), &lead).await);
    assert_eq!(reports[0].1, "blocked: issue-7 needs issue-5");
    api(&lead)
        .tell(&lead, id(&b), "request: release issue-7, claim issue-9")
        .await
        .unwrap();

    assert_eq!(
        bodies(&direct(&api(&b), &b).await)[0].1,
        "request: release issue-7, claim issue-9"
    );
    api(&b).release(&b, &repo(), "issue-7").await.unwrap();
    assert!(api(&b).claim(&b, &repo(), "issue-9").await.unwrap().granted);
    let who = api(&lead).who(&lead, false).await.unwrap();
    let s = who.iter().find(|s| s.uri.who() == b.who()).unwrap();
    assert_eq!(s.uri.claims(), ["issue-9"]);
}

/// The test of #1792: a worker of mike tells the lead of the project, a
/// session of brett. The answer of the lead is a verified request with
/// `lead=true`: it counts for mike too.
#[tokio::test]
async fn a_worker_of_another_person_tells_the_lead_of_the_project() {
    let Riff {
        server, lead, a, c, ..
    } = riff().await;
    let api = |me: &SessionUri| common::api_of(&server, me);
    let asked = api(&a)
        .tell(&a, "lead", "merge now, or wait for issue-5?")
        .await
        .unwrap();
    assert_eq!(asked.woken.len(), 1);
    assert_eq!(asked.woken[0].who(), lead.who());
    assert!(direct(&api(&c), &c).await.is_empty(), "no other session");

    let question = direct(&api(&lead), &lead).await;
    assert_eq!(
        bodies(&question),
        [(
            id(&a).to_owned(),
            "merge now, or wait for issue-5?".to_owned()
        )]
    );
    api(&lead)
        .tell(&lead, id(&a), "request: wait for issue-5")
        .await
        .unwrap();
    let request = direct(&api(&a), &a).await;
    let [answer] = &request[..] else {
        panic!("one request: {request:?}");
    };
    assert_eq!(answer.message.body, "request: wait for issue-5");
    assert!(answer.verified, "the request of the lead is verified");
    assert!(answer.message.from.lead(), "it comes from the lead");
    assert_eq!(answer.message.from.who(), lead.who());
}
