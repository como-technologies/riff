//! A riff with no sign-in trusts its network (R211, R212). A worker
//! sends a question to the lead. The lead answers. On a riff with no
//! sign-in, the answer is verified and shows the lead mark, so the
//! worker takes it as the decision of its user. On a riff with sign-in,
//! an answer with no signature is not verified.

use std::time::Duration;

use futures::StreamExt;
use riff::api::Api;
use riff::text;
use riff_core::name::{SessionUri, ThreadName};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::Provider;

const LEAD: &str = "riff://mike@pangolin/como-technologies/riff?session=a1";
const WORKER: &str = "riff://mike@pangolin/como-technologies/riff?session=b2#issue-62";
const REPO: &str = "como-technologies/riff";

fn uri(s: &str) -> SessionUri {
    s.parse().unwrap()
}

/// A server with a sign-in provider or with none. It does not need a
/// sign-in, so the sessions of the test post with no signature.
async fn start(sign_in: bool) -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let provider = sign_in.then(|| Provider {
        issuer: "https://accounts.google.com".into(),
        client_id: "riff".into(),
        client_secret: None,
        allowed_domains: vec!["comotechnologies.io".into()],
    });
    let config = Config {
        provider,
        ..Config::new(&url)
    };
    let router = Service::new(config).router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    Api::new(&url)
}

/// The worker asks the lead, and the lead answers. Returns what the
/// worker reads, as `riff read` shows it.
async fn ask_and_answer(api: &Api) -> String {
    let (lead, worker) = (uri(LEAD), uri(WORKER));
    api.register(&lead).await.unwrap();
    api.register(&worker).await.unwrap();
    let repo: ThreadName = REPO.parse().unwrap();
    api.post(&lead, Some(&repo), &[], "hello", Default::default())
        .await
        .unwrap();

    let asked = api
        .tell(&worker, "lead", "Which port? 7878 or 8080")
        .await
        .unwrap();
    assert_eq!(asked.woken.len(), 1, "{asked:?}");
    let question = text::inbox(&api.inbox(&lead, None, false).await.unwrap(), &lead);
    assert!(question.contains("Which port?"), "{question}");
    api.tell(&lead, "b2", "Our user says 7878").await.unwrap();
    text::inbox(&api.inbox(&worker, None, true).await.unwrap(), &worker)
}

#[tokio::test]
async fn on_a_riff_with_no_sign_in_the_answer_of_the_lead_is_verified() {
    let api = start(false).await;
    let repo: ThreadName = REPO.parse().unwrap();
    let person: SessionUri = "riff://mike@pangolin".parse().unwrap();
    let tail = api.tail(&person, &repo).await.unwrap();

    let out = ask_and_answer(&api).await;
    assert!(
        out.contains(
            "mike@pangolin:riff (a1) lead=true to session=b2 (verified): Our user says 7878"
        ),
        "{out}"
    );
    assert!(
        out.contains("mike@pangolin:riff (a1) lead=true (verified): hello"),
        "{out}"
    );

    // `riff tail` shows the same mark.
    let first = isolated::in_time(Duration::from_secs(5), tail.boxed().next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(first.verified);
    assert!(first.message.from.lead(), "{:?}", first.message.from);
}

#[tokio::test]
async fn on_a_riff_with_sign_in_an_answer_with_no_signature_is_not_verified() {
    let api = start(true).await;
    let repo: ThreadName = REPO.parse().unwrap();
    let person: SessionUri = "riff://mike@pangolin".parse().unwrap();
    let tail = api.tail(&person, &repo).await.unwrap();

    let out = ask_and_answer(&api).await;
    assert!(
        out.contains("mike@pangolin:riff (a1) to session=b2 (not verified): Our user says 7878"),
        "{out}"
    );
    assert!(!out.contains("(a1) lead=true"), "{out}");

    let first = isolated::in_time(Duration::from_secs(5), tail.boxed().next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!first.verified);
}
