//! The lead conducts the sessions of its user (R228-R232), over real
//! HTTP: it gives two sessions two items, each claims its item and
//! reports back, and a blocked session gets a new item. A question goes
//! only to the lead of the user of its sender.

use riff::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Kind, Status};

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

/// Mike's lead, two other sessions of mike, and brett's lead.
struct Riff {
    api: Api,
    lead: SessionUri,
    a: SessionUri,
    b: SessionUri,
    brett: SessionUri,
}

async fn riff() -> Riff {
    let api = start_server().await;
    let riff = Riff {
        lead: uri("riff://mike@pangolin/como-technologies/riff?session=l1"),
        a: uri("riff://mike@pangolin/como-technologies/riff?session=a1#a"),
        b: uri("riff://mike@thelio/como-technologies/riff?session=b2#b"),
        brett: uri("riff://brett@heron/como-technologies/riff?session=x9"),
        api,
    };
    // The first session of each user is its lead (R176).
    for me in [&riff.lead, &riff.brett, &riff.a, &riff.b] {
        riff.api.register(me).await.unwrap();
    }
    riff
}

/// The bodies of the unread direct messages to `me`, with their senders.
async fn direct(api: &Api, me: &SessionUri) -> Vec<(String, String)> {
    api.inbox(me, None, false)
        .await
        .unwrap()
        .into_iter()
        .filter(|inbox| inbox.thread.is_direct())
        .flat_map(|inbox| inbox.messages)
        .filter(|c| c.message.from.who() != me.who())
        .map(|c| {
            let from = c.message.from.who().session().unwrap().to_owned();
            (from, c.message.body)
        })
        .collect()
}

fn id(me: &SessionUri) -> &str {
    me.who().session().unwrap()
}

#[tokio::test]
async fn the_lead_gives_two_sessions_two_items_and_each_reports_back() {
    let Riff {
        api, lead, a, b, ..
    } = riff().await;

    // The lead asks its sessions for their status, then gives each one
    // item.
    let ask = api
        .post(
            &lead,
            Some(&repo()),
            &["user=mike,repo=como-technologies/riff".parse().unwrap()],
            "",
            Kind::Status,
        )
        .await
        .unwrap();
    assert_eq!(ask.woken.len(), 2, "only the sessions of mike wake");
    for (me, item) in [(&a, "issue-12"), (&b, "issue-7")] {
        let posted = api
            .tell(&lead, id(me), &format!("request: claim {item}"))
            .await
            .unwrap();
        assert_eq!(posted.woken[0].who(), me.who());
    }

    // Each session reads the request, claims its item and reports back.
    for (me, item) in [(&a, "issue-12"), (&b, "issue-7")] {
        let requests = direct(&api, me).await;
        assert_eq!(
            requests,
            [(id(&lead).to_owned(), format!("request: claim {item}"))]
        );
        assert!(api.claim(me, &repo(), item).await.unwrap().granted);
        let status = Status {
            step: format!("{item}: started"),
            blocked: None,
        };
        api.status(me, &status).await.unwrap();
        let report = api
            .tell(me, "lead", &format!("started {item}"))
            .await
            .unwrap();
        assert_eq!(report.woken[0].who(), lead.who());
    }

    // The lead sees both reports, and who shows each claim and status.
    let reports = direct(&api, &lead).await;
    assert_eq!(reports.len(), 2);
    let who = api.who(&lead, false).await.unwrap();
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
async fn a_blocked_session_tells_the_lead_and_gets_a_new_item() {
    let Riff { api, lead, b, .. } = riff().await;
    api.claim(&b, &repo(), "issue-7").await.unwrap();
    let blocked = Status {
        step: "issue-7".into(),
        blocked: Some("needs issue-5".into()),
    };
    api.status(&b, &blocked).await.unwrap();
    api.tell(&b, "lead", "blocked on issue-7: needs issue-5")
        .await
        .unwrap();

    let reports = direct(&api, &lead).await;
    assert_eq!(reports[0].1, "blocked on issue-7: needs issue-5");
    api.tell(&lead, id(&b), "request: release issue-7, claim issue-9")
        .await
        .unwrap();

    assert_eq!(
        direct(&api, &b).await[0].1,
        "request: release issue-7, claim issue-9"
    );
    api.release(&b, &repo(), "issue-7").await.unwrap();
    assert!(api.claim(&b, &repo(), "issue-9").await.unwrap().granted);
    let who = api.who(&lead, false).await.unwrap();
    let s = who.iter().find(|s| s.uri.who() == b.who()).unwrap();
    assert_eq!(s.uri.claims(), ["issue-9"]);
}

#[tokio::test]
async fn a_question_goes_only_to_the_lead_of_its_own_user() {
    let Riff {
        api,
        lead,
        a,
        brett,
        ..
    } = riff().await;
    let asked = api
        .tell(&a, "lead", "merge now, or wait for issue-5?")
        .await
        .unwrap();
    assert_eq!(asked.woken.len(), 1);
    assert_eq!(asked.woken[0].who(), lead.who());
    assert!(direct(&api, &brett).await.is_empty());

    // Brett's lead is the lead of brett, not of mike.
    let brett_asks = api.tell(&brett, "lead", "who leads?").await;
    let error = brett_asks.unwrap_err().to_string();
    assert!(error.contains("Ask your own user"), "{error}");
}
