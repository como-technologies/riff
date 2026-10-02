//! Any admin can take the owner role: `riff owner --take`, `riff owner
//! --deny`, a request with no answer, and an owner who is gone, against a
//! real server with short times (01M3N7K3ZAZFGABN7032AYJWEM,
//! 01M3N7K41N03P26BEFFNX5617K, 01M3Q5460YESBSQHTV3M15PE53,
//! 01M3Q546335NBTKG5BHQ27QC93, 01M3Q63NNC6SC03BFCG80M7B4D,
//! 01M3N7K4BC1RPZKQ1XNDTBRPGF, 01M3N7K4DVHSF7AQ402F14J26Z). The sign-in
//! is in the mock store of `keyring-core`, so the tests run in process.
//!
//! Each riff here has sign-in: a riff with no sign-in has no owner. The
//! tests read the people through [`Service::members`] and the routes,
//! and never through the token store.

mod book;
mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use book::{commands_of_part, each_is_real, page};
use isolated::Isolated;
use riff::api::Api;
use riff::login::{self, SignIn};
use riff::text;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Kind, RiffOwner, TokenReply};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::Provider;
use riff_server::owner::{Timing, server_uri};
use riff_server::store::{Memory, Store};

/// A long time: no request ends, and no check runs, in a test.
const LONG: Duration = Duration::from_secs(3600);

/// The settings of a riff with sign-in at `url`, with these times of
/// the owner role. A call with no token is not refused, so a session
/// with no sign-in can register. No test calls the provider.
fn config(url: &str, timing: Timing) -> Config {
    Config {
        owner_role: timing,
        provider: Some(Provider {
            issuer: "https://accounts.example.com".into(),
            client_id: "riff".into(),
            client_secret: None,
            allowed_domains: Vec::new(),
        }),
        ..Config::new(url)
    }
}

/// A server with these times of the owner role. ada is the owner. bob
/// and carol are admins. Each of them has a live lead session in the
/// repository.
async fn start(timing: Timing) -> (Service, Api, Vec<TokenReply>) {
    let (listener, url) = common::listen().await;
    let service = Service::new(config(&url, timing));
    let api = Api::new(&url);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let pairs = people(&service, &api).await;
    (service, api, pairs)
}

/// The people of each test: ada signs in first, so ada is the owner.
/// As the owner, ada makes bob and carol admins. Then each of them
/// signs in, and each gets a live lead session in the repository. The
/// riff has no repository before, so the changes post no note.
async fn people(service: &Service, api: &Api) -> Vec<TokenReply> {
    let admins = ["bob@gmail.com", "carol@gmail.com"];
    let mut pairs = vec![sign_in(service, api, "ada@gmail.com").await];
    let ada = api.clone().signed_in(None).unwrap();
    for admin in admins {
        ada.set_admin(&person("ada"), admin, true).await.unwrap();
    }
    for admin in admins {
        pairs.push(sign_in(service, api, admin).await);
    }
    for user in ["ada", "bob", "carol"] {
        Api::new(api.base()).register(&lead(user)).await.unwrap();
    }
    pairs
}

/// Signs in `email` on this device, as the provider sign-in does.
async fn sign_in(service: &Service, api: &Api, email: &str) -> TokenReply {
    let jkt = riff::device::key(api.base()).unwrap().thumbprint();
    let pair = service.admit(email, false, &jkt).await.unwrap();
    let sign_in = SignIn {
        user: pair.user.clone(),
        access_token: pair.access_token.clone(),
        refresh_token: pair.refresh_token.clone(),
        expires_at: u64::MAX,
        // The client checks the riff ID of a riff with sign-in
        // (01M3JNVBRS35B3CD67367JF7SJ).
        riff_id: service.riff_id(),
    };
    login::store(api.base(), &sign_in).unwrap();
    pair
}

/// `email` on this device, from now on.
async fn as_person(service: &Service, api: &Api, email: &str) -> Api {
    sign_in(service, api, email).await;
    api.clone().signed_in(None).unwrap()
}

/// The owner ada invites `email`, through the route.
async fn invite(service: &Service, api: &Api, email: &str) {
    let ada = as_person(service, api, "ada@gmail.com").await;
    ada.invite(&person("ada"), email).await.unwrap();
}

/// The email of the owner, or `None` when the riff has no owner.
fn owner(service: &Service) -> Option<String> {
    service.members().owner
}

/// The email of each admin that is not the owner, sorted.
fn admins(service: &Service) -> Vec<String> {
    service.members().admins
}

/// The person `user` on this host: it posts the note of a pass.
fn person(user: &str) -> SessionUri {
    format!("riff://{user}@pangolin").parse().unwrap()
}

/// The lead session of `user` in the repository.
fn lead(user: &str) -> SessionUri {
    format!("riff://{user}@pangolin/como-technologies/riff?session={user}-lead")
        .parse()
        .unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// The direct messages of the server to the lead of `user`.
async fn told(api: &Api, user: &str) -> Vec<String> {
    let me = lead(user);
    let thread = ThreadName::direct(server_uri().who(), me.who());
    let Ok(messages) = Api::new(api.base()).read(&me, &thread, true).await else {
        return Vec::new();
    };
    messages.into_iter().map(|m| m.message.body).collect()
}

/// Waits until `done` is true, for at most 10 seconds.
async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < until, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn timing(answer: Duration, check_every: Duration, misses: u32) -> Timing {
    Timing {
        answer,
        check_every,
        misses,
    }
}

#[tokio::test]
async fn an_admin_asks_and_the_owner_passes() {
    let (service, api, _) = start(timing(LONG, LONG, 3)).await;
    let bob = as_person(&service, &api, "bob@gmail.com").await;
    let asked = bob.take_owner().await.unwrap();
    assert_eq!(asked.owner.as_deref(), Some("ada@gmail.com"));
    assert_eq!(
        text::owner_asked(&asked),
        "You asked ada@gmail.com for the owner role. The owner has 60 minutes to answer. \
         With no answer, you are the owner. The riff posts each step to the thread of each \
         repository."
    );
    // The riff tells the owner at once, in a direct message to its lead.
    let to_ada = told(&api, "ada").await;
    assert_eq!(to_ada.len(), 1, "{to_ada:?}");
    assert!(to_ada[0].starts_with("members: bob asks for the owner role."));
    assert!(to_ada[0].ends_with("Show this to your user."));

    assert_eq!(service.asks().as_deref(), Some("bob@gmail.com"));

    let ada = as_person(&service, &api, "ada@gmail.com").await;
    ada.pass_owner(&person("ada"), "bob@gmail.com")
        .await
        .unwrap();
    assert_eq!(owner(&service).as_deref(), Some("bob@gmail.com"));
    // The old owner stays an admin.
    assert_eq!(admins(&service), ["ada@gmail.com", "carol@gmail.com"]);
    assert_eq!(service.asks(), None, "the pass ends the request");
}

#[tokio::test]
async fn an_admin_asks_and_the_owner_denies() {
    let answer = Duration::from_millis(300);
    let (service, api, _) = start(timing(answer, LONG, 3)).await;
    as_person(&service, &api, "bob@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap();

    let ada = as_person(&service, &api, "ada@gmail.com").await;
    let denied = ada.deny_owner().await.unwrap();
    assert_eq!(
        text::owner_denied(&denied),
        "ada@gmail.com stays the owner. The riff tells bob@gmail.com."
    );
    // The riff tells the admin.
    let to_bob = told(&api, "bob").await;
    assert_eq!(
        to_bob,
        [
            "members: ada kept the owner role. bob@gmail.com asked for it. ada@gmail.com \
          stays the owner. Show this to your user."
        ]
    );
    assert_eq!(service.asks(), None, "the deny ends the request");
    tokio::time::sleep(answer * 3).await;
    assert_eq!(owner(&service).as_deref(), Some("ada@gmail.com"));

    // Only the owner denies, and only a request that waits. The words
    // of the refusal count, not the number of the status.
    let error = ada.deny_owner().await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("no admin asks for the owner role now"),
        "{error}"
    );
    let bob = as_person(&service, &api, "bob@gmail.com").await;
    bob.take_owner().await.unwrap();
    let error = bob.deny_owner().await.unwrap_err();
    assert!(error.to_string().contains("not the owner"), "{error}");
}

#[tokio::test]
async fn with_no_answer_the_admin_is_the_owner() {
    let (service, api, _) = start(timing(Duration::from_millis(300), LONG, 3)).await;
    as_person(&service, &api, "bob@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap();
    assert_eq!(owner(&service).as_deref(), Some("ada@gmail.com"));
    wait_for("bob is the owner", || {
        owner(&service).as_deref() == Some("bob@gmail.com")
    })
    .await;
    // The old owner stays an admin, and no request waits.
    assert_eq!(admins(&service), ["ada@gmail.com", "carol@gmail.com"]);
    assert_eq!(service.asks(), None);
}

#[tokio::test]
async fn a_second_request_waits_for_the_first() {
    let (service, api, _) = start(timing(LONG, LONG, 3)).await;
    as_person(&service, &api, "bob@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap();
    let error = as_person(&service, &api, "carol@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap_err()
        .to_string();
    // The words of the refusal count, not the number of the status.
    assert!(
        error.contains("bob@gmail.com asked for the owner role first"),
        "{error}"
    );
    assert_eq!(service.asks().as_deref(), Some("bob@gmail.com"));
    assert_eq!(owner(&service).as_deref(), Some("ada@gmail.com"));
}

#[tokio::test]
async fn a_member_cannot_take_the_owner_role() {
    let (service, api, _) = start(timing(LONG, LONG, 3)).await;
    invite(&service, &api, "dan@gmail.com").await;
    let error = as_person(&service, &api, "dan@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not an admin"), "{error}");
}

/// `riff owner --take` by the owner is not an error, and changes nothing
/// (01M3WRJAFS6W3J2ZRJ6XSW3SB5).
#[tokio::test]
async fn the_owner_is_the_owner_already() {
    let (service, api, _) = start(timing(LONG, LONG, 3)).await;
    let ada = as_person(&service, &api, "ada@gmail.com").await;
    let asked = ada.take_owner().await.unwrap();
    assert!(asked.already(), "{asked:?}");
    assert_eq!(
        text::owner_asked(&asked),
        "You are the owner already. Nothing changed."
    );
    assert_eq!(owner(&service).as_deref(), Some("ada@gmail.com"));
    assert_eq!(service.asks(), None, "no request waits");
    assert_eq!(admins(&service), ["bob@gmail.com", "carol@gmail.com"]);
    // The riff posts no note, and tells no lead.
    let notes = ada.read(&lead("ada"), &repo(), true).await.unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    for user in ["ada", "bob", "carol"] {
        assert_eq!(told(&api, user).await, [""; 0], "{user}");
    }

    // The request of an admin still waits after a take of the owner.
    as_person(&service, &api, "bob@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap();
    let ada = as_person(&service, &api, "ada@gmail.com").await;
    assert!(ada.take_owner().await.unwrap().already());
    assert_eq!(owner(&service).as_deref(), Some("ada@gmail.com"));
    assert_eq!(service.asks().as_deref(), Some("bob@gmail.com"));
}

/// Another user does not get the owner role while the riff has an owner:
/// an admin asks and waits, and a member is refused.
#[tokio::test]
async fn another_user_does_not_take_the_role_from_an_owner() {
    let (service, api, _) = start(timing(LONG, LONG, 3)).await;
    invite(&service, &api, "dan@gmail.com").await;
    let error = as_person(&service, &api, "dan@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("403"), "{error}");
    assert!(error.contains("not an admin"), "{error}");

    let asked = as_person(&service, &api, "bob@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap();
    assert!(!asked.already(), "{asked:?}");
    assert_eq!(asked.owner.as_deref(), Some("ada@gmail.com"));
    assert_eq!(owner(&service).as_deref(), Some("ada@gmail.com"));
    assert_eq!(service.asks().as_deref(), Some("bob@gmail.com"));
}

/// A server whose owner ada has no live lead: her lead ended. The checks
/// run each 50 ms, and 2 misses in a row make her gone.
async fn gone_owner() -> (Service, Api) {
    let (service, api, _) = start(timing(LONG, Duration::from_millis(50), 2)).await;
    Api::new(api.base()).end(&lead("ada")).await.unwrap();
    wait_for("ada is gone", || owner(&service).is_none()).await;
    (service, api)
}

#[tokio::test]
async fn the_first_volunteer_is_the_owner_at_once() {
    let (service, api) = gone_owner().await;
    // The old owner stays an admin.
    assert_eq!(
        admins(&service),
        ["ada@gmail.com", "bob@gmail.com", "carol@gmail.com"]
    );
    // The riff asks each admin for a volunteer.
    for admin in ["bob", "carol"] {
        let to_admin = told(&api, admin).await;
        assert_eq!(to_admin.len(), 1, "{to_admin:?}");
        assert!(
            to_admin[0].contains("The riff needs a volunteer"),
            "{to_admin:?}"
        );
    }
    let asked = as_person(&service, &api, "carol@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap();
    assert_eq!(asked.owner, None, "no wait");
    assert_eq!(
        text::owner_asked(&asked),
        "The riff had no owner. carol@gmail.com is now the owner."
    );
    assert_eq!(owner(&service).as_deref(), Some("carol@gmail.com"));
    assert_eq!(admins(&service), ["ada@gmail.com", "bob@gmail.com"]);
}

#[tokio::test]
async fn with_no_volunteer_the_riff_has_no_owner() {
    let (service, api) = gone_owner().await;
    let bob = as_person(&service, &api, "bob@gmail.com").await;
    let list = members(&bob.members().await.unwrap());
    assert!(list.starts_with("owner            none\n"), "{list}");
    assert!(list.contains("The riff has no owner."), "{list}");
    // riff who says it too (01M3Q63NK0AHM25MB258B0K8XP).
    let who = bob.roster(&lead("bob"), false).await.unwrap();
    assert_eq!(who.owner, RiffOwner::Nobody);

    // Each action of the owner is refused, and names riff owner --take.
    let ada = as_person(&service, &api, "ada@gmail.com").await;
    let refusals = [
        ada.set_admin(&person("ada"), "dan@gmail.com", true)
            .await
            .unwrap_err(),
        ada.pass_owner(&person("ada"), "bob@gmail.com")
            .await
            .unwrap_err(),
        ada.deny_owner().await.unwrap_err(),
    ];
    for error in refusals {
        assert!(error.to_string().contains("riff owner --take"), "{error}");
    }

    // The sign-in of ada made no owner, and no check makes one.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(owner(&service), None, "the riff waits");
    assert!(service.owned(), "the riff had an owner");
    let asked = as_person(&service, &api, "bob@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap();
    assert_eq!(asked.owner, None, "no wait");
    assert_eq!(owner(&service).as_deref(), Some("bob@gmail.com"));
}

/// The lead of the owner ends, but the owner has another live session:
/// the owner stays (01M3Q546335NBTKG5BHQ27QC93).
#[tokio::test]
async fn another_live_session_keeps_the_owner() {
    let (service, api, _) = start(timing(LONG, Duration::from_millis(50), 2)).await;
    let other: SessionUri = "riff://ada@thelio/como-technologies/riff?session=ada-other"
        .parse()
        .unwrap();
    Api::new(api.base()).register(&other).await.unwrap();
    Api::new(api.base()).end(&lead("ada")).await.unwrap();
    // 10 checks, more than the 3 misses of a drop.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(owner(&service).as_deref(), Some("ada@gmail.com"));

    // With no session and no call, the owner is gone.
    Api::new(api.base()).end(&other).await.unwrap();
    wait_for("ada is gone", || owner(&service).is_none()).await;
}

/// The server warns the owner one check before the drop: a note to the
/// owner in the thread of each repository, and one line in the chat
/// (01M3Q546335NBTKG5BHQ27QC93).
#[tokio::test]
async fn the_owner_gets_a_warning_one_check_before_the_drop() {
    let every = Duration::from_millis(100);
    let (service, api) = {
        let (service, api, _) = start(timing(LONG, every, 2)).await;
        Api::new(api.base()).end(&lead("ada")).await.unwrap();
        (service, api)
    };
    wait_for("ada is gone", || owner(&service).is_none()).await;
    let bob = Api::new(api.base());
    let notes = bob.read(&lead("bob"), &repo(), true).await.unwrap();
    let [warning, gone] = &notes[..] else {
        panic!("{notes:#?}");
    };
    assert_eq!(
        warning.message.body,
        "members: the owner ada@gmail.com was not seen at 2 checks in a row, less than a \
         minute apart. At the next check, in less than a minute, the riff has no owner. To \
         stay the owner, run a riff command, for example: riff who."
    );
    assert_eq!(warning.message.kind, Kind::Note);
    assert_eq!(warning.message.to.len(), 1);
    assert_eq!(warning.message.to[0].user.as_deref(), Some("ada"));
    assert!(
        gone.message.body.contains("is gone"),
        "{}",
        gone.message.body
    );
    // One check lies between the warning and the drop.
    let gap = gone.message.at_ms - warning.message.at_ms;
    assert!(gap + 20 >= 100, "{gap} ms");

    let chat = bob
        .read(&lead("bob"), &ThreadName::chat(), true)
        .await
        .unwrap();
    let bodies: Vec<&str> = chat.iter().map(|m| m.message.body.as_str()).collect();
    assert_eq!(bodies, [warning.message.body.as_str()]);
    assert_eq!(chat[0].message.from, server_uri());
}

/// A call of the owner as a person, for example `riff who`, is a sign
/// of life after the warning (01M3Q546335NBTKG5BHQ27QC93).
#[tokio::test]
async fn a_call_as_a_person_keeps_the_owner() {
    let (service, api, _) = start(timing(LONG, Duration::from_millis(100), 2)).await;
    Api::new(api.base()).end(&lead("ada")).await.unwrap();
    let ada = person("ada");
    let until = Instant::now() + Duration::from_millis(800);
    while Instant::now() < until {
        Api::new(api.base()).who(&ada, false).await.unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert_eq!(owner(&service).as_deref(), Some("ada@gmail.com"));
}

/// `--owner-pings` sets the window: more misses keep the owner longer
/// (01M3Q5460YESBSQHTV3M15PE53).
#[tokio::test]
async fn the_setting_changes_the_window() {
    let every = Duration::from_millis(100);
    let (short, short_api, _) = start(timing(LONG, every, 1)).await;
    let (long, long_api, _) = start(timing(LONG, every, 6)).await;
    Api::new(short_api.base()).end(&lead("ada")).await.unwrap();
    Api::new(long_api.base()).end(&lead("ada")).await.unwrap();
    wait_for("ada is gone", || owner(&short).is_none()).await;
    assert_eq!(owner(&long).as_deref(), Some("ada@gmail.com"));
    wait_for("ada is gone", || owner(&long).is_none()).await;
}

/// Each step posts one note to the thread of each repository. The notes
/// of the server come from `riff@server`, wake no session and hold no
/// token (01M3N7K4DVHSF7AQ402F14J26Z).
#[tokio::test]
async fn each_step_posts_one_note() {
    let answer = Duration::from_millis(1500);
    let (service, api, pairs) = start(timing(answer, Duration::from_millis(50), 2)).await;
    let bob = as_person(&service, &api, "bob@gmail.com").await;
    bob.take_owner().await.unwrap(); // the request
    let ada = as_person(&service, &api, "ada@gmail.com").await;
    ada.pass_owner(&person("ada"), "bob@gmail.com")
        .await
        .unwrap(); // the pass
    let carol = as_person(&service, &api, "carol@gmail.com").await;
    carol.take_owner().await.unwrap(); // the request
    let bob = as_person(&service, &api, "bob@gmail.com").await;
    bob.deny_owner().await.unwrap(); // the deny
    let carol = as_person(&service, &api, "carol@gmail.com").await;
    carol.take_owner().await.unwrap(); // the request
    wait_for("the grant", || {
        owner(&service).as_deref() == Some("carol@gmail.com")
    })
    .await;
    Api::new(api.base()).end(&lead("carol")).await.unwrap();
    wait_for("carol is gone", || owner(&service).is_none()).await;
    let ada = as_person(&service, &api, "ada@gmail.com").await;
    ada.take_owner().await.unwrap(); // the volunteer

    let notes = Api::new(api.base())
        .read(&lead("ada"), &repo(), true)
        .await
        .unwrap();
    let bodies: Vec<&str> = notes.iter().map(|n| n.message.body.as_str()).collect();
    let starts = [
        "members: bob asks for the owner role. The owner ada@gmail.com has less than a minute",
        "members: ada passed the owner role to bob@gmail.com.",
        "members: carol asks for the owner role. The owner bob@gmail.com",
        "members: bob kept the owner role. carol@gmail.com asked for it.",
        "members: carol asks for the owner role. The owner bob@gmail.com",
        "members: the owner bob@gmail.com did not answer in less than a minute. \
         carol@gmail.com is the owner now.",
        "members: the owner carol@gmail.com was not seen at 2 checks in a row",
        "members: the owner carol@gmail.com is gone: not seen at 3 checks in a row",
        "members: ada took the owner role. The riff had no owner.",
    ];
    assert_eq!(bodies.len(), starts.len(), "{bodies:#?}");
    for (body, start) in bodies.iter().zip(starts) {
        assert!(
            body.starts_with(start),
            "{body}\nshould start with\n{start}"
        );
    }
    for note in &notes {
        assert_eq!(note.message.kind, Kind::Note);
        for pair in &pairs {
            assert!(!note.message.body.contains(&pair.access_token));
            assert!(!note.message.body.contains(&pair.refresh_token));
        }
    }
    // Each note but the pass comes from the server.
    let from_server = notes
        .iter()
        .filter(|n| n.message.from == server_uri())
        .count();
    assert_eq!(from_server, starts.len() - 1);
}

/// The owner is gone while a request for the owner role waits: the
/// admin that asked is the owner at once, with no wait for the answer
/// time. The old owner stays an admin (01M3Q546335NBTKG5BHQ27QC93).
#[tokio::test]
async fn a_gone_owner_gives_the_role_to_the_admin_that_asked() {
    // Given a riff whose answer time is long. bob asks for the role.
    let (service, api, _) = start(timing(LONG, Duration::from_millis(50), 2)).await;
    as_person(&service, &api, "bob@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap();
    assert_eq!(service.asks().as_deref(), Some("bob@gmail.com"));
    assert_eq!(owner(&service).as_deref(), Some("ada@gmail.com"));

    // When the owner gives no sign of life at the checks.
    Api::new(api.base()).end(&lead("ada")).await.unwrap();
    wait_for("ada is gone", || {
        owner(&service).as_deref() != Some("ada@gmail.com")
    })
    .await;

    // Then bob is the owner at once, and ada stays an admin.
    assert_eq!(owner(&service).as_deref(), Some("bob@gmail.com"));
    assert_eq!(admins(&service), ["ada@gmail.com", "carol@gmail.com"]);
    assert_eq!(service.asks(), None, "the request ended");
    // The riff posts the note of the change.
    let until = Instant::now() + Duration::from_secs(10);
    let last = loop {
        let notes = Api::new(api.base())
            .read(&lead("bob"), &repo(), true)
            .await
            .unwrap();
        let last = notes.last().map(|n| n.message.body.clone());
        if let Some(last) = last.filter(|body| body.contains("is gone")) {
            break last;
        }
        assert!(Instant::now() < until, "timed out: the note");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(
        last.starts_with("members: the owner ada@gmail.com is gone"),
        "{last}"
    );
    assert!(
        last.ends_with(
            "bob@gmail.com asked for the owner role first, and is the owner now. \
             ada@gmail.com stays an admin."
        ),
        "{last}"
    );
}

/// A pass of the owner role to another person ends the request that
/// waits. A deny after it finds no request (01M3JYX8NPZASQY6031R35H39P).
#[tokio::test]
async fn a_pass_to_another_person_ends_the_request_that_waits() {
    // Given a request of bob that waits.
    let (service, api, _) = start(timing(LONG, LONG, 3)).await;
    as_person(&service, &api, "bob@gmail.com")
        .await
        .take_owner()
        .await
        .unwrap();

    // When the owner passes the role to carol, not to bob.
    let ada = as_person(&service, &api, "ada@gmail.com").await;
    ada.pass_owner(&person("ada"), "carol@gmail.com")
        .await
        .unwrap();

    // Then carol is the owner, and no request waits: the new owner has
    // none to deny.
    assert_eq!(owner(&service).as_deref(), Some("carol@gmail.com"));
    assert_eq!(admins(&service), ["ada@gmail.com", "bob@gmail.com"]);
    assert_eq!(service.asks(), None, "the pass ends the request");
    let carol = as_person(&service, &api, "carol@gmail.com").await;
    let error = carol.deny_owner().await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("no admin asks for the owner role now"),
        "{error}"
    );
}

/// Short lease times, so that a load does not wait 15 seconds.
const LEASE: riff_server::lease::Timing = riff_server::lease::Timing {
    wait: Duration::from_millis(50),
    read_every: Duration::from_millis(50),
    valid_for: Duration::from_millis(500),
    exit_after: Duration::from_secs(1),
};

/// A riff whose owner was gone keeps no owner after a restart on the
/// same store: `--owner` names none then, and a sign-in makes none. The
/// admins stay. The first admin that asks is the owner at once
/// (01M3N7K4GAKJ621V5AWJRQVF3M, 01M3Q63NNC6SC03BFCG80M7B4D).
#[tokio::test]
async fn a_riff_with_no_owner_keeps_none_after_a_restart() {
    // Given a riff on a store whose owner ada is gone.
    let (front, api) = Front::start().await;
    let store = Memory::default();
    let times = timing(LONG, Duration::from_millis(50), 2);
    let load = |owner: Option<&str>| {
        let config = Config {
            owner: owner.map(str::to_owned),
            lease: LEASE,
            save_every: Duration::from_millis(20),
            ..config(api.base(), times)
        };
        let store: Arc<dyn Store> = Arc::new(store.clone());
        async move { Service::load(config, store).await.unwrap() }
    };
    let old = load(None).await;
    front.serve(&old);
    people(&old, &api).await;
    Api::new(api.base()).end(&lead("ada")).await.unwrap();
    wait_for("ada is gone", || owner(&old).is_none()).await;
    old.save().await.unwrap();

    // When a new server starts on the same store, and its setting
    // names ada as the owner.
    let new = load(Some("ada@gmail.com")).await;
    tokio::time::timeout(Duration::from_secs(5), old.stopped())
        .await
        .unwrap();
    front.serve(&new);

    // Then the riff has no owner, and it had one. The admins stay.
    assert_eq!(owner(&new), None);
    assert!(new.owned(), "the riff had an owner");
    assert_eq!(
        admins(&new),
        ["ada@gmail.com", "bob@gmail.com", "carol@gmail.com"]
    );
    // A sign-in makes no owner.
    let ada = as_person(&new, &api, "ada@gmail.com").await;
    assert_eq!(owner(&new), None);
    let error = ada
        .pass_owner(&person("ada"), "bob@gmail.com")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("riff owner --take"), "{error}");

    // The first admin that asks is the owner at once.
    let asked = ada.take_owner().await.unwrap();
    assert_eq!(asked.owner, None, "no wait");
    assert_eq!(owner(&new).as_deref(), Some("ada@gmail.com"));
    assert_eq!(admins(&new), ["bob@gmail.com", "carol@gmail.com"]);
}

/// A server at a fixed URL, whose service a test can replace: a restart
/// on the same store.
struct Front {
    current: Arc<std::sync::Mutex<axum::Router>>,
}

impl Front {
    async fn start() -> (Front, Api) {
        let (listener, url) = common::listen().await;
        let current = Arc::new(std::sync::Mutex::new(axum::Router::new()));
        let serve = current.clone();
        let router = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let router = serve.lock().unwrap().clone();
            async move { tower::ServiceExt::oneshot(router, request).await }
        });
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (Front { current }, Api::new(&url))
    }

    /// Serves `service` from now on.
    fn serve(&self, service: &Service) {
        *self.current.lock().unwrap() = service.router();
    }
}

/// "Take the owner role" in "Start a Team Riff" has a how-to for each
/// step, and each setting of the times. Each command is real.
#[test]
fn the_book_shows_how_to_take_the_owner_role() {
    let commands = commands_of_part("start-a-team-riff.md", "Take the owner role");
    assert_eq!(
        commands,
        [
            "riff owner --take",
            "riff owner EMAIL",
            "riff owner --deny",
            "riff who",
            "riff-server --public-url URL --owner EMAIL --owner-take-minutes 30",
        ]
    );
    each_is_real(&commands[..4]);
    let help = Isolated::shared()
        .assert_riff_server()
        .arg("--help")
        .assert()
        .success();
    let help = String::from_utf8_lossy(&help.get_output().stdout).into_owned();
    let page = page("start-a-team-riff.md");
    for setting in [
        "--owner-take-minutes",
        "RIFF_OWNER_TAKE_MINUTES",
        "--owner-ping-minutes",
        "RIFF_OWNER_PING_MINUTES",
        "--owner-pings",
        "RIFF_OWNER_PINGS",
    ] {
        assert!(page.contains(&format!("`{setting}`")), "{setting}");
        assert!(help.contains(setting), "{setting}");
    }
}

/// `riff members` with no color.
fn members(list: &riff_core::wire::MembersReply) -> String {
    anstream::adapter::strip_str(&riff::view::members(list)).to_string()
}
