//! Any admin can take the owner role: `riff owner --take`, `riff owner
//! --deny`, a request with no answer, and an owner who is gone, against a
//! real server with short times (01M3N7K3ZAZFGABN7032AYJWEM,
//! 01M3N7K41N03P26BEFFNX5617K, 01M3N7K443DGPZ8XH5WWKK6M35,
//! 01M3N7K46H5BRFJCB46P3JNAFZ, 01M3N7K48XQ8XSP7R0HD535ZX3,
//! 01M3N7K4BC1RPZKQ1XNDTBRPGF, 01M3N7K4DVHSF7AQ402F14J26Z). The sign-in
//! is in the mock store of `keyring-core`, so the tests run in process.

mod book;

use std::sync::Once;
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
use riff_server::owner::{Timing, server_uri};

static MOCK_KEYRING: Once = Once::new();

/// A long time: no request ends, and no check runs, in a test.
const LONG: Duration = Duration::from_secs(3600);

/// A server with these times of the owner role. ada is the owner. bob
/// and carol are admins. Each of them has a live lead session in the
/// repository.
async fn start(timing: Timing) -> (Service, Api, Vec<TokenReply>) {
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = Service::new(Config {
        owner_role: timing,
        ..Config::new(&url)
    });
    let api = Api::new(&url);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut pairs = vec![sign_in(&service, &api, "ada@gmail.com")];
    for admin in ["bob@gmail.com", "carol@gmail.com"] {
        service.tokens().add_admin(admin).unwrap();
        pairs.push(sign_in(&service, &api, admin));
    }
    for user in ["ada", "bob", "carol"] {
        Api::new(&url).register(&lead(user)).await.unwrap();
    }
    (service, api, pairs)
}

/// Signs in `email` on this device, as the provider sign-in does.
fn sign_in(service: &Service, api: &Api, email: &str) -> TokenReply {
    let jkt = riff::device::key(api.base()).unwrap().thumbprint();
    let pair = service
        .tokens()
        .admit(email, false, &[], &jkt, Instant::now())
        .unwrap();
    let sign_in = SignIn {
        user: pair.user.clone(),
        access_token: pair.access_token.clone(),
        refresh_token: pair.refresh_token.clone(),
        expires_at: u64::MAX,
        riff_id: None,
    };
    login::store(api.base(), &sign_in).unwrap();
    pair
}

/// `email` on this device, from now on.
fn as_person(service: &Service, api: &Api, email: &str) -> Api {
    sign_in(service, api, email);
    api.clone().signed_in(None).unwrap()
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
    let bob = as_person(&service, &api, "bob@gmail.com");
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

    let ada = as_person(&service, &api, "ada@gmail.com");
    ada.pass_owner(&person("ada"), "bob@gmail.com")
        .await
        .unwrap();
    assert_eq!(service.tokens().owner(), Some("bob@gmail.com"));
    assert!(service.tokens().is_admin("ada", &[]));
    assert_eq!(service.tokens().asks(), None, "the pass ends the request");
}

#[tokio::test]
async fn an_admin_asks_and_the_owner_denies() {
    let answer = Duration::from_millis(300);
    let (service, api, _) = start(timing(answer, LONG, 3)).await;
    as_person(&service, &api, "bob@gmail.com")
        .take_owner()
        .await
        .unwrap();

    let ada = as_person(&service, &api, "ada@gmail.com");
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
    tokio::time::sleep(answer * 3).await;
    assert_eq!(service.tokens().owner(), Some("ada@gmail.com"));

    // Only the owner denies, and only a request that waits.
    let error = ada.deny_owner().await.unwrap_err();
    assert!(error.to_string().contains("no admin asks"), "{error}");
    let bob = as_person(&service, &api, "bob@gmail.com");
    bob.take_owner().await.unwrap();
    let error = bob.deny_owner().await.unwrap_err();
    assert!(error.to_string().contains("not the owner"), "{error}");
}

#[tokio::test]
async fn with_no_answer_the_admin_is_the_owner() {
    let (service, api, _) = start(timing(Duration::from_millis(300), LONG, 3)).await;
    as_person(&service, &api, "bob@gmail.com")
        .take_owner()
        .await
        .unwrap();
    assert_eq!(service.tokens().owner(), Some("ada@gmail.com"));
    wait_for("bob is the owner", || {
        service.tokens().owner() == Some("bob@gmail.com")
    })
    .await;
    let (_, admins, _) = service.tokens().roles(&[]);
    assert_eq!(admins, ["ada@gmail.com", "carol@gmail.com"]);
}

#[tokio::test]
async fn a_second_request_waits_for_the_first() {
    let (service, api, _) = start(timing(LONG, LONG, 3)).await;
    as_person(&service, &api, "bob@gmail.com")
        .take_owner()
        .await
        .unwrap();
    let error = as_person(&service, &api, "carol@gmail.com")
        .take_owner()
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("409"), "{error}");
    assert!(
        error.contains("bob@gmail.com asked for the owner role first"),
        "{error}"
    );
    assert_eq!(service.tokens().asks(), Some("bob@gmail.com"));
}

#[tokio::test]
async fn a_member_cannot_take_the_owner_role() {
    let (service, api, _) = start(timing(LONG, LONG, 3)).await;
    service.tokens().invite("dan@gmail.com").unwrap();
    let error = as_person(&service, &api, "dan@gmail.com")
        .take_owner()
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not an admin"), "{error}");
}

/// A server whose owner ada has no live lead: her lead ended. The checks
/// run each 50 ms, and 2 misses in a row make her gone.
async fn gone_owner() -> (Service, Api) {
    let (service, api, _) = start(timing(LONG, Duration::from_millis(50), 2)).await;
    Api::new(api.base()).end(&lead("ada")).await.unwrap();
    wait_for("ada is gone", || service.tokens().owner().is_none()).await;
    (service, api)
}

#[tokio::test]
async fn the_first_volunteer_is_the_owner_at_once() {
    let (service, api) = gone_owner().await;
    assert!(service.tokens().is_admin("ada", &[]));
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
        .take_owner()
        .await
        .unwrap();
    assert_eq!(asked.owner, None, "no wait");
    assert_eq!(
        text::owner_asked(&asked),
        "The riff had no owner. carol@gmail.com is now the owner."
    );
    assert_eq!(service.tokens().owner(), Some("carol@gmail.com"));
}

#[tokio::test]
async fn with_no_volunteer_the_riff_has_no_owner() {
    let (service, api) = gone_owner().await;
    let bob = as_person(&service, &api, "bob@gmail.com");
    let list = text::members(&bob.members().await.unwrap());
    assert!(list.starts_with("owner: none\n"), "{list}");
    assert!(list.contains("The riff has no owner."), "{list}");
    // riff who says it too (01M3N754NY5JX4P0SN8R4ZYFG9).
    assert_eq!(service.tokens().riff_owner(), RiffOwner::Nobody);

    // Each action of the owner is refused, and names riff owner --take.
    let ada = as_person(&service, &api, "ada@gmail.com");
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

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(service.tokens().owner(), None, "the riff waits");
    let asked = as_person(&service, &api, "bob@gmail.com")
        .take_owner()
        .await
        .unwrap();
    assert_eq!(asked.owner, None, "no wait");
    assert_eq!(service.tokens().owner(), Some("bob@gmail.com"));
}

/// Each step posts one note to the thread of each repository. The notes
/// of the server come from `riff@server`, wake no session and hold no
/// token (01M3N7K4DVHSF7AQ402F14J26Z).
#[tokio::test]
async fn each_step_posts_one_note() {
    let answer = Duration::from_millis(1500);
    let (service, api, pairs) = start(timing(answer, Duration::from_millis(50), 2)).await;
    let bob = as_person(&service, &api, "bob@gmail.com");
    bob.take_owner().await.unwrap(); // the request
    let ada = as_person(&service, &api, "ada@gmail.com");
    ada.pass_owner(&person("ada"), "bob@gmail.com")
        .await
        .unwrap(); // the pass
    let carol = as_person(&service, &api, "carol@gmail.com");
    carol.take_owner().await.unwrap(); // the request
    let bob = as_person(&service, &api, "bob@gmail.com");
    bob.deny_owner().await.unwrap(); // the deny
    let carol = as_person(&service, &api, "carol@gmail.com");
    carol.take_owner().await.unwrap(); // the request
    wait_for("the grant", || {
        service.tokens().owner() == Some("carol@gmail.com")
    })
    .await;
    Api::new(api.base()).end(&lead("carol")).await.unwrap();
    wait_for("carol is gone", || service.tokens().owner().is_none()).await;
    let ada = as_person(&service, &api, "ada@gmail.com");
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
        "members: the owner carol@gmail.com is gone: no live lead at 2 checks in a row",
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
            "riff-server --public-url URL --owner EMAIL --owner-take-minutes 30",
        ]
    );
    each_is_real(&commands[..3]);
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
