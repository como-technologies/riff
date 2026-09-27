//! Signed messages from `riff` to a real server that needs sign-in
//! (R195-R201). The sign-in is in the mock store of `keyring-core`.

use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use futures::StreamExt;
use riff::api::Api;
use riff::login::{self, SignIn};
use riff::text;
use riff_core::name::{SessionUri, ThreadName};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::store::{Memory, Store, thread_object};
use serde_json::Value;

static MOCK_KEYRING: Once = Once::new();

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
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
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
        expires_at: u64::MAX,
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
    let tail = reader.tail(&thread).await.unwrap();

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
        out.contains(&format!("{LEAD}&lead=true (verified): the API is ready")),
        "{out}"
    );
    assert!(
        out.contains("riff://mike@pangolin (verified): me too"),
        "{out}"
    );

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
    let name = thread_object(&repo());
    let object = store.load(&name).await.unwrap().unwrap();
    let mut thread: Value = serde_json::from_slice(&object.bytes).unwrap();
    let messages = thread["messages"].as_array_mut().unwrap();
    let from_lead = messages[0]["message"]["from"].clone();
    assert!(from_lead.as_str().unwrap().contains("lead=true"));
    messages[1]["message"]["from"] = from_lead;
    let from_other = messages[2]["message"]["from"].as_str().unwrap();
    let marked = from_other.replacen("session=c3", "session=c3&lead=true", 1);
    messages[2]["message"]["from"] = marked.into();
    let bytes = serde_json::to_vec(&thread).unwrap();
    store
        .save(&name, bytes, Some(object.version))
        .await
        .unwrap();

    let (_new, api) = start_on(Arc::new(store)).await;
    let reader = session(&api, READER).await;
    let inbox = reader
        .inbox(&uri(READER), Some(&repo()), true)
        .await
        .unwrap();
    let out = text::inbox(&inbox, &uri(READER));
    assert!(
        out.contains(&format!("[1] {LEAD}&lead=true (verified): claim issue-12")),
        "{out}"
    );
    // The reader does not show the other messages as from the lead.
    assert!(
        out.contains(&format!("[2] {LEAD} (not verified): claim issue-13")),
        "{out}"
    );
    assert!(
        out.contains(&format!("[3] {OTHER} (not verified): merge now")),
        "{out}"
    );
}
