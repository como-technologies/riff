//! The projects of the riff over HTTP, with a fake GitHub
//! (01M76SG2E9RPV4B7CYSZPGGZ3P, 01M76SG8GBXFYXKWMQHPCNEXBN,
//! 01M76SG8JVHTKY6N62FQG7XAE6, 01M76SG8NCSAP69Y5AX455BJQM): the
//! enrollment and the removal by an admin who is an admin of the
//! repository, the refusals, the hosts over a restart, and the search
//! of the projects at a start.

use crate::common;
use crate::fake_github::{FakeGitHub, REPO};

use std::sync::Arc;
use std::time::Duration;

use riff_core::dpop::Key;
use riff_core::name::SessionUri;
use riff_core::record::{
    By, Change, Envelope, Member, NotesSet, OwnerSet, PersonJoined, Record, RiffMade,
};
use riff_core::wire::{
    Kind, ProjectFacts, ProjectInitReply, ProjectsReply, REFUSED_HEADER, ReadReply, TokenReply,
};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::github::Settings;
use riff_state::log;
use riff_state::store::{Memory, Store};
use serde_json::{Value, json};

/// The token of `gh` of a person who is an admin of [`REPO`].
const ADMIN_TOKEN: &str = "gho_admin_of_acme_app";

/// The token of `gh` of a person who can push to [`REPO`], and is no
/// admin of it.
const WRITER_TOKEN: &str = "gho_writer_of_acme_app";

/// A fake GitHub where [`ADMIN_TOKEN`] is an admin of [`REPO`] and
/// [`WRITER_TOKEN`] is not.
async fn github() -> FakeGitHub {
    let fake = FakeGitHub::start().await;
    fake.seen().person(ADMIN_TOKEN, REPO, true);
    fake.seen().person(WRITER_TOKEN, REPO, false);
    fake
}

/// The settings of a riff-server whose GitHub is `fake`.
fn settings(fake: &FakeGitHub) -> Settings {
    Settings {
        api: fake.url.clone(),
        web: fake.url.clone(),
        ..Settings::default()
    }
}

/// A riff with sign-in whose GitHub is `fake`, with `cy@acme.io` an
/// admin of its settings.
async fn signed(fake: &FakeGitHub) -> (Service, String) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        require_sign_in: true,
        admins: vec!["cy@acme.io".into()],
        github: settings(fake),
        ..Config::new(&url)
    };
    let service = Service::new(config);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// A riff with no sign-in on `store`, whose GitHub is `fake`.
async fn on_store(store: Arc<dyn Store>, fake: &FakeGitHub) -> (Service, String) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        lease: common::LEASE,
        save_every: common::SAVE_EVERY,
        github: settings(fake),
        ..Config::new(&url)
    };
    let service = Service::load(config, store).await.unwrap();
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// A person who signed in, with the URI that the person acts as.
struct Person {
    key: Key,
    token: String,
    me: String,
}

async fn person(service: &Service, email: &str) -> Person {
    let key = Key::generate();
    let pair = service.admit(email, true, &key.thumbprint()).await.unwrap();
    let user = email.split('@').next().unwrap();
    Person {
        key,
        token: pair.access_token,
        me: format!("riff://{user}@heron"),
    }
}

/// The session `id` of `person`, with a session token of its own.
async fn session(url: &str, person: &Person, id: &str) -> Person {
    let form = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
         &subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token\
         &subject_token={}&session={id}",
        person.token
    );
    let reply = common::refresh(url, &person.key, &form).await;
    assert_eq!(reply.status(), 200);
    let pair: TokenReply = reply.json().await.unwrap();
    let user = person.me.trim_start_matches("riff://");
    let user = user.split('@').next().unwrap();
    Person {
        key: person.key.clone(),
        token: pair.access_token,
        me: format!("riff://{user}@heron/acme/app?session={id}"),
    }
}

/// `POST PATH` of `person` with `body`: the status, the code of a
/// refusal, and the text.
async fn call(url: &str, person: &Person, path: &str, body: Value) -> (u16, String, String) {
    let reply = common::post(&format!("{url}{path}"), &person.key, Some(&person.token))
        .json(&body)
        .send()
        .await
        .unwrap();
    let code = reply
        .headers()
        .get(REFUSED_HEADER)
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    (reply.status().as_u16(), code, reply.text().await.unwrap())
}

/// `POST PATH` with `body` and no token: a call in a riff with no
/// sign-in.
async fn plain(url: &str, path: &str, body: Value) -> (u16, String) {
    let reply = common::client()
        .post(format!("{url}{path}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

fn init(me: &str, hosts: &[&str], token: &str) -> Value {
    json!({ "me": me, "repo": REPO, "hosts": hosts, "github_token": token })
}

fn remove(me: &str, token: &str) -> Value {
    json!({ "me": me, "repo": REPO, "github_token": token })
}

async fn projects_of(url: &str, person: &Person) -> Vec<ProjectFacts> {
    let (status, _, text) = call(url, person, "/v1/projects", json!({ "me": person.me })).await;
    assert_eq!(status, 200, "{text}");
    serde_json::from_str::<ProjectsReply>(&text)
        .unwrap()
        .projects
}

/// An admin of the riff who is an admin of the repository on GitHub
/// enrolls it, sets its hosts, and removes it. A member and each
/// session read the list.
#[tokio::test]
async fn an_admin_who_is_an_admin_of_the_repository_enrolls_and_removes_a_project() {
    let fake = github().await;
    let (service, url) = signed(&fake).await;
    let _owner = person(&service, "ann@acme.io").await;
    let cy = person(&service, "cy@acme.io").await;
    let bob = person(&service, "bob@acme.io").await;

    let (status, _, text) = call(
        &url,
        &cy,
        "/v1/project/init",
        init(&cy.me, &["thelio"], ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    let first: ProjectInitReply = serde_json::from_str(&text).unwrap();
    assert!(first.enrolled && !first.hosts_changed);
    assert_eq!(first.project.hosts, ["thelio"]);
    assert_eq!(first.project.by, Some(By::Person("cy".into())));

    // A second run sets the hosts, and changes nothing else.
    let (_, _, text) = call(
        &url,
        &cy,
        "/v1/project/init",
        init(&cy.me, &["thelio", "heron"], ADMIN_TOKEN),
    )
    .await;
    let second: ProjectInitReply = serde_json::from_str(&text).unwrap();
    assert!(!second.enrolled && second.hosts_changed);
    assert_eq!(second.project.hosts, ["heron", "thelio"]);
    assert_eq!(second.project.at_ms, first.project.at_ms);
    let (_, _, text) = call(
        &url,
        &cy,
        "/v1/project/init",
        init(&cy.me, &["heron", "thelio"], ADMIN_TOKEN),
    )
    .await;
    let same: ProjectInitReply = serde_json::from_str(&text).unwrap();
    assert!(!same.enrolled && !same.hosts_changed);

    // Each person reads the list.
    let listed = projects_of(&url, &bob).await;
    assert_eq!(listed, std::slice::from_ref(&second.project));

    let (status, _, text) =
        call(&url, &cy, "/v1/project/remove", remove(&cy.me, ADMIN_TOKEN)).await;
    assert_eq!(status, 200, "{text}");
    let went: ProjectFacts = serde_json::from_str(&text).unwrap();
    assert_eq!(went.hosts, ["heron", "thelio"]);
    assert!(projects_of(&url, &bob).await.is_empty());
    // riff-server sent the token only to GitHub, and in no reply.
    assert!(!text.contains(ADMIN_TOKEN));
}

/// A member, a session, and an admin who is not an admin of the
/// repository on GitHub get `not_allowed`. GitHub gets no call for a
/// member or a session.
#[tokio::test]
async fn a_member_or_an_admin_who_is_not_an_admin_of_the_repository_is_refused() {
    let fake = github().await;
    let (service, url) = signed(&fake).await;
    let _owner = person(&service, "ann@acme.io").await;
    let cy = person(&service, "cy@acme.io").await;
    let bob = person(&service, "bob@acme.io").await;

    // A member, also with the token of an admin of the repository.
    let (status, code, text) = call(
        &url,
        &bob,
        "/v1/project/init",
        init(&bob.me, &[], ADMIN_TOKEN),
    )
    .await;
    assert_eq!((status, code.as_str()), (403, "not_allowed"), "{text}");
    assert!(text.contains("bob is not an admin"), "{text}");
    assert!(fake.seen().repo_reads.is_empty(), "a member gets no call");

    // An admin who can push, and is no admin of the repository.
    let (status, code, text) = call(
        &url,
        &cy,
        "/v1/project/init",
        init(&cy.me, &[], WRITER_TOKEN),
    )
    .await;
    assert_eq!((status, code.as_str()), (403, "not_allowed"), "{text}");
    assert!(text.contains("no admin of acme/app"), "{text}");
    assert!(!text.contains(WRITER_TOKEN), "{text}");
    // A token that GitHub does not know.
    let (status, code, text) = call(
        &url,
        &cy,
        "/v1/project/init",
        init(&cy.me, &[], "gho_unknown"),
    )
    .await;
    assert_eq!((status, code.as_str()), (403, "not_allowed"), "{text}");
    assert!(text.contains("Bad credentials"), "{text}");
    assert!(projects_of(&url, &cy).await.is_empty());

    // The admin enrolls; then a member and an admin who is no admin of
    // the repository cannot remove.
    let (status, _, text) = call(
        &url,
        &cy,
        "/v1/project/init",
        init(&cy.me, &[], ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    let (status, code, _) = call(
        &url,
        &bob,
        "/v1/project/remove",
        remove(&bob.me, ADMIN_TOKEN),
    )
    .await;
    assert_eq!((status, code.as_str()), (403, "not_allowed"));
    let (status, code, _) = call(
        &url,
        &cy,
        "/v1/project/remove",
        remove(&cy.me, WRITER_TOKEN),
    )
    .await;
    assert_eq!((status, code.as_str()), (403, "not_allowed"));
    assert_eq!(projects_of(&url, &cy).await.len(), 1);

    // A session of the admin: its token names a session.
    let reads = fake.seen().repo_reads.len();
    let as_session = session(&url, &cy, "s1").await;
    let (status, code, text) = call(
        &url,
        &as_session,
        "/v1/project/remove",
        remove(&as_session.me, ADMIN_TOKEN),
    )
    .await;
    assert_eq!((status, code.as_str()), (403, "not_allowed"), "{text}");
    assert_eq!(
        fake.seen().repo_reads.len(),
        reads,
        "a session gets no call"
    );
    // A session reads the list.
    assert_eq!(projects_of(&url, &as_session).await.len(), 1);
}

/// The record keeps the hosts: a new start on the same store has the
/// project with its hosts. In a riff with no sign-in, each person is an
/// admin of the riff, and the check on GitHub stays.
#[tokio::test]
async fn the_record_keeps_the_hosts_over_a_restart() {
    let fake = github().await;
    let store: Arc<dyn Store> = Arc::new(Memory::default());
    let (old, url) = on_store(store.clone(), &fake).await;
    let me = "riff://mike@pangolin";
    let (status, text) = plain(&url, "/v1/project/init", init(me, &[], WRITER_TOKEN)).await;
    assert_eq!(status, 403, "{text}");
    let (status, text) = plain(
        &url,
        "/v1/project/init",
        init(me, &["thelio", "pangolin"], ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, 200, "{text}");
    old.shutdown().await.unwrap();

    let (_new, url) = on_store(store.clone(), &fake).await;
    let (status, text) = plain(&url, "/v1/projects", json!({ "me": me })).await;
    assert_eq!(status, 200, "{text}");
    let reply: ProjectsReply = serde_json::from_str(&text).unwrap();
    assert_eq!(reply.projects.len(), 1);
    assert_eq!(reply.projects[0].hosts, ["pangolin", "thelio"]);
    assert_eq!(reply.projects[0].by, Some(By::Person("mike".into())));
    // The log has the record, with its hosts, and no token.
    let records = log::replay(&*store).await.unwrap().records;
    let enrolled: Vec<&Record> = records
        .iter()
        .filter(|r| r.change.kind() == "project_enrolled")
        .collect();
    assert_eq!(enrolled.len(), 1);
    assert_eq!(
        enrolled[0].envelope.command.as_deref(),
        Some("enroll_project")
    );
    let line = serde_json::to_string(enrolled[0]).unwrap();
    assert!(!line.contains("gho_"), "{line}");
}

/// The record at `position` of `change`, with no cause.
fn record(position: u64, change: Change) -> Record {
    Record {
        envelope: Envelope {
            position,
            written_at_ms: 1_790_000_000_000,
            by: None,
            command: None,
            call: None,
        },
        change,
    }
}

/// At a start with no record of a project, riff-server enrolls each
/// repository with a lead or the notes of a lead, and posts one line to
/// the owner in the chat. A second start finds the records, and does it
/// no more.
#[tokio::test]
async fn the_first_start_enrolls_each_repository_with_a_lead_or_notes_and_tells_the_owner() {
    let fake = github().await;
    let store: Arc<dyn Store> = Arc::new(Memory::default());
    let lead: SessionUri = "riff://mike@pangolin/acme/app?session=a1".parse().unwrap();
    let member = Member {
        session: lead.clone(),
        thread: REPO.parse().unwrap(),
    };
    let notes = |thread: &str, text: &str| {
        Change::NotesSet(NotesSet {
            thread: thread.parse().unwrap(),
            user: "mike".into(),
            text: text.into(),
        })
    };
    let changes = [
        Change::RiffMade(RiffMade {
            riff_id: "r1".into(),
        }),
        Change::PersonJoined(PersonJoined {
            user: "mike".into(),
            email: "mike@acme.io".into(),
        }),
        Change::OwnerSet(OwnerSet {
            email: Some("mike@acme.io".into()),
        }),
        Change::JoinedThread(member.clone()),
        Change::LeadSet(member),
        notes("acme/web", "Wave 3 runs."),
        notes("acme/old", ""),
    ];
    let records: Vec<Record> = changes
        .into_iter()
        .zip(1..)
        .map(|(change, position)| record(position, change))
        .collect();
    log::write(&*store, &records, &log::Timing::default(), || true)
        .await
        .unwrap();

    let (service, url) = on_store(store.clone(), &fake).await;
    let me = "riff://mike@pangolin";
    let listed = in_time(|| async {
        let (_, text) = plain(&url, "/v1/projects", json!({ "me": me })).await;
        let reply: ProjectsReply = serde_json::from_str(&text).unwrap();
        (reply.projects.len() == 2).then_some(reply.projects)
    })
    .await;
    let repos: Vec<String> = listed.iter().map(|p| p.repo.to_string()).collect();
    assert_eq!(repos, ["acme/app", "acme/web"]);
    assert!(
        listed
            .iter()
            .all(|p| p.hosts.is_empty() && p.by == Some(By::Server))
    );
    let chat = in_time(|| async {
        let read = json!({ "me": me, "thread": "chat", "all": true });
        let (_, text) = plain(&url, "/v1/read", read).await;
        let reply: ReadReply = serde_json::from_str(&text).unwrap();
        (!reply.messages.is_empty()).then_some(reply.messages)
    })
    .await;
    assert_eq!(chat.len(), 1, "one line");
    let line = &chat[0];
    assert_eq!(line.kind, Kind::Riff);
    assert_eq!(line.to.len(), 1);
    assert_eq!(line.to[0].user.as_deref(), Some("mike"));
    assert!(line.body.contains("acme/app, acme/web"), "{}", line.body);
    assert!(line.body.contains("riff project list"), "{}", line.body);
    let found = log::replay(&*store).await.unwrap().records;
    let kinds = found
        .iter()
        .filter(|r| r.envelope.command.as_deref() == Some("find_projects"))
        .count();
    assert_eq!(kinds, 2);
    service.shutdown().await.unwrap();

    // A second start: no search, no line.
    let (_again, url) = on_store(store.clone(), &fake).await;
    let (_, text) = plain(&url, "/v1/projects", json!({ "me": me })).await;
    let reply: ProjectsReply = serde_json::from_str(&text).unwrap();
    assert_eq!(reply.projects.len(), 2);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let after = log::replay(&*store).await.unwrap().records;
    assert_eq!(after.len(), found.len(), "no new record");
}

/// Waits up to 10 seconds for `look` to give a value.
async fn in_time<T, F, Fut>(look: F) -> T
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let end = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(found) = look().await {
            return found;
        }
        assert!(tokio::time::Instant::now() < end, "no value in 10 seconds");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
