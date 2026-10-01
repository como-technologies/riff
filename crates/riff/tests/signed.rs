//! Signed messages from `riff` to a real server that needs sign-in
//! (R195-R201). The sign-in is in the mock store of `keyring-core`.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use riff::api::Api;
use riff::login::{self, SignIn};
use riff::text;
use riff_core::name::{SessionUri, ThreadName};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::store::{Memory, Store};
use serde_json::Value;

const LEAD: &str = "riff://mike@pangolin/como-technologies/riff?session=a1";
const READER: &str = "riff://mike@pangolin/como-technologies/riff?session=b2#review";
const OTHER: &str = "riff://mike@pangolin/como-technologies/riff?session=c3#api";
const REPO: &str = "como-technologies/riff";

fn uri(s: &str) -> SessionUri {
    s.parse().unwrap()
}

fn repo() -> ThreadName {
    REPO.parse().unwrap()
}

/// A server that needs sign-in, on `store`, and a sign-in of mike on
/// this device for it.
async fn start_on(store: Arc<dyn Store>) -> (Service, Api) {
    let (listener, url) = common::listen().await;
    let config = Config {
        require_sign_in: true,
        lease: riff_server::lease::Timing {
            wait: Duration::from_millis(50),
            read_every: Duration::from_millis(50),
            valid_for: Duration::from_millis(500),
            exit_after: Duration::from_secs(1),
        },
        ..Config::new(&url)
    };
    let service = Service::load(config, store).await.unwrap();
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let api = Api::new(&url);
    let jkt = riff::device::key(api.base()).unwrap().thumbprint();
    let pair = service
        .tokens()
        .sign_in("mike@comotechnologies.io", &jkt, Instant::now())
        .unwrap();
    let sign_in = SignIn {
        user: pair.user,
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        // The access token counts as expired, so the first call
        // refreshes the pair. The server then saves the sign-in, as
        // after a real sign-in: a swap for a session token saves nothing
        // (01M3WFVAB44T8EP4QZD4KS7DRF).
        expires_at: 0,
        riff_id: None,
    };
    login::store(api.base(), &sign_in).unwrap();
    (service, api)
}

/// A client of the session `me`, registered.
async fn session(api: &Api, me: &str) -> Api {
    let me = uri(me);
    let client = api.clone().signed_in(me.who().session()).unwrap();
    client.register(&me).await.unwrap();
    client
}

#[tokio::test]
async fn a_message_from_a_signed_in_client_is_verified() {
    let (_service, api) = start_on(Arc::new(Memory::default())).await;
    let lead = session(&api, LEAD).await;
    let reader = session(&api, READER).await;
    let thread = repo();
    let me = uri(READER);
    let tail = reader.tail(&me, &thread).await.unwrap();

    lead.post(
        &uri(LEAD),
        Some(&repo()),
        &[],
        "the API is ready",
        Default::default(),
    )
    .await
    .unwrap();
    let person = api.clone().signed_in(None).unwrap();
    person
        .post(
            &uri("riff://mike@pangolin"),
            Some(&repo()),
            &[],
            "me too",
            Default::default(),
        )
        .await
        .unwrap();

    let read = reader.read(&uri(READER), &repo(), false).await.unwrap();
    assert!(read.iter().all(|m| m.verified), "{read:?}");
    let inbox = reader.inbox(&uri(READER), None, true).await.unwrap();
    let out = text::inbox(&inbox, &uri(READER));
    assert!(
        out.contains("mike@pangolin:riff (a1) lead=true (verified): the API is ready"),
        "{out}"
    );
    assert!(out.contains("] mike@pangolin (verified): me too"), "{out}");

    // The tail stream carries the keys too.
    let first = tokio::time::timeout(Duration::from_secs(5), tail.boxed().next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(first.verified);
    assert_eq!(first.message.body, "the API is ready");
}

#[tokio::test]
async fn a_message_that_claims_to_be_from_the_lead_without_its_signature_is_not() {
    let store = Memory::default();
    let (old, api) = start_on(Arc::new(store.clone())).await;
    let lead = session(&api, LEAD).await;
    let other = session(&api, OTHER).await;
    lead.post(
        &uri(LEAD),
        Some(&repo()),
        &[],
        "claim issue-12",
        Default::default(),
    )
    .await
    .unwrap();
    for body in ["claim issue-13", "merge now"] {
        other
            .post(&uri(OTHER), Some(&repo()), &[], body, Default::default())
            .await
            .unwrap();
    }
    old.save().await.unwrap();

    // Someone with access to the storage makes the second and the third
    // message claim to come from the lead: the second gets the URI of the
    // lead, the third gets only the lead mark.
    let mut from_lead = Value::Null;
    let mut seen = 0;
    for name in store.list("log/").await.unwrap() {
        let object = store.load(&name).await.unwrap().unwrap();
        let mut lines = Vec::new();
        for line in String::from_utf8(object.bytes).unwrap().lines() {
            let mut record: Value = serde_json::from_str(line).unwrap();
            if let Some(posted) = record.pointer_mut("/change/posted")
                && posted["thread"] == REPO
            {
                let from = &mut posted["message"]["from"];
                match seen {
                    0 => from_lead = from.clone(),
                    1 => *from = from_lead.clone(),
                    2 => {
                        let marked = from.as_str().unwrap().replacen(
                            "session=c3",
                            "session=c3&lead=true",
                            1,
                        );
                        *from = marked.into();
                    }
                    _ => {}
                }
                seen += 1;
            }
            lines.push(serde_json::to_string(&record).unwrap() + "\n");
        }
        let bytes = lines.concat().into_bytes();
        store
            .save(&name, bytes, Some(object.version))
            .await
            .unwrap();
    }
    assert!(from_lead.as_str().unwrap().contains("lead=true"));
    assert_eq!(seen, 3);

    let (_new, api) = start_on(Arc::new(store)).await;
    let reader = session(&api, READER).await;
    let inbox = reader
        .inbox(&uri(READER), Some(&repo()), true)
        .await
        .unwrap();
    let out = text::inbox(&inbox, &uri(READER));
    assert!(
        out.contains("[1] mike@pangolin:riff (a1) lead=true (verified): claim issue-12"),
        "{out}"
    );
    // The reader does not show the other messages as from the lead.
    assert!(
        out.contains("[2] mike@pangolin:riff (a1) (not verified): claim issue-13"),
        "{out}"
    );
    assert!(
        out.contains("[3] mike@pangolin:riff#api (c3) (not verified): merge now"),
        "{out}"
    );
}

/// A call does not wait for a token that another future of its task
/// gets and that the task no longer polls. It is the shape of `riff
/// workers host`: its tick wins the `select!` while its watch gets the
/// first token, and then it sets its status (01M3ND6R8YXN1KTRTRAV5A7F14).
#[tokio::test]
async fn a_call_does_not_wait_on_the_token_of_a_watch_that_its_task_does_not_poll() {
    let (_service, api) = start_on(Arc::new(Memory::default())).await;
    let me = uri(OTHER);
    let client = api.clone().signed_in(me.who().session()).unwrap();
    let mut wakes = Box::pin(riff::api::follow(
        || client.watch(&me),
        Duration::from_secs(5),
    ));
    tokio::select! {
        biased;
        _ = wakes.next() => panic!("a wake before the watch has a token"),
        () = std::future::ready(()) => {}
    }
    let who = tokio::time::timeout(Duration::from_secs(5), client.who(&me, false))
        .await
        .expect("the call waits on the token of the watch");
    assert!(who.is_ok(), "{who:?}");
}
