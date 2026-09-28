//! `riff invite`, `riff remove`, `riff members`, `riff admin` and `riff owner` against a real
//! server (01M3JN3AHMK532XMRDASD4XD5D). The sign-in is in the mock
//! store of `keyring-core`, so the tests run in process.

use std::sync::Once;
use std::time::Instant;

use riff::api::Api;
use riff::login::{self, SignIn};
use riff::text;
use riff_core::dpop::Key;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Kind, TokenReply};
use riff_server::Service;
use riff_server::auth::Config;

static MOCK_KEYRING: Once = Once::new();

async fn start() -> (Service, Api) {
    start_with(false).await
}

/// A server; with `require_sign_in`, each call needs a token.
async fn start_with(require_sign_in: bool) -> (Service, Api) {
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = Service::new(Config {
        require_sign_in,
        ..Config::new(&url)
    });
    let api = Api::new(&url);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, api)
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

/// The person `user` on this host: it posts the note of each change.
fn person(user: &str) -> SessionUri {
    format!("riff://{user}@pangolin").parse().unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

#[tokio::test]
async fn the_owner_invites_lists_and_removes() {
    let (service, api) = start().await;
    sign_in(&service, &api, "ada@gmail.com");
    let signed_in = api.clone().signed_in(None).unwrap();

    let invited = signed_in
        .invite(&person("ada"), "bob@gmail.com")
        .await
        .unwrap()
        .done;
    // The answer names the address of the riff, and the lines to join
    // it (01M3MEF4B33Z6WVJMDP29C7SS2).
    let answer = text::invited(&invited);
    assert!(
        answer.starts_with(&format!(
            "Invited bob@gmail.com to the riff at {}.",
            api.base()
        )),
        "{answer}"
    );
    assert!(
        answer.contains(&format!("export RIFF_SERVER={}'", api.base())),
        "{answer}"
    );
    assert!(answer.ends_with("\nriff connect claude"), "{answer}");
    let bob_key = Key::generate().thumbprint();
    let bob = service
        .tokens()
        .admit("bob@gmail.com", false, &[], &bob_key, Instant::now())
        .unwrap();

    let list = signed_in.members().await.unwrap();
    assert_eq!(
        text::members(&list),
        "owner: ada@gmail.com\nadmins: none\nmembers: bob@gmail.com\nallowed domains: none"
    );

    let removed = signed_in
        .remove(&person("ada"), "bob@gmail.com")
        .await
        .unwrap()
        .done;
    assert_eq!(
        text::removed(&removed),
        "Removed bob@gmail.com and ended 1 sign-in."
    );
    let check = service
        .tokens()
        .check(&bob.access_token, &bob_key, Instant::now());
    assert!(check.is_err());
}

#[tokio::test]
async fn a_member_cannot_invite() {
    let (service, api) = start().await;
    let owner_key = Key::generate().thumbprint();
    service
        .tokens()
        .admit("ada@gmail.com", false, &[], &owner_key, Instant::now())
        .unwrap();
    service.tokens().invite("bob@gmail.com").unwrap();
    sign_in(&service, &api, "bob@gmail.com");
    let error = api
        .clone()
        .signed_in(None)
        .unwrap()
        .invite(&person("bob"), "carol@gmail.com")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not an admin"), "{error}");
}

/// This server has no sign-in provider, so nobody can sign in (R227).
#[tokio::test]
async fn members_needs_a_sign_in() {
    let (_, api) = start().await;
    let error = api.members().await.unwrap_err();
    assert_eq!(error.to_string(), text::nobody_signs_in(api.base()));
}

#[tokio::test]
async fn the_owner_makes_an_admin_who_invites_and_removes() {
    let (service, api) = start().await;
    sign_in(&service, &api, "ada@gmail.com");
    let added = api
        .clone()
        .signed_in(None)
        .unwrap()
        .set_admin(&person("ada"), "Bob@gmail.com", true)
        .await
        .unwrap()
        .done;
    assert_eq!(
        text::admin_set(&added),
        "bob@gmail.com is now an admin. They can invite and remove members."
    );

    sign_in(&service, &api, "bob@gmail.com");
    let bob = api.clone().signed_in(None).unwrap();
    bob.invite(&person("bob"), "carol@gmail.com").await.unwrap();
    bob.invite(&person("bob"), "dan@gmail.com").await.unwrap();
    bob.remove(&person("bob"), "dan@gmail.com").await.unwrap();
    let list = bob.members().await.unwrap();
    assert_eq!(
        text::members(&list),
        "owner: ada@gmail.com\nadmins: bob@gmail.com\nmembers: carol@gmail.com\nallowed domains: none"
    );
}

#[tokio::test]
async fn only_the_owner_adds_an_admin() {
    let (service, api) = start().await;
    let owner_key = Key::generate().thumbprint();
    service
        .tokens()
        .admit("ada@gmail.com", false, &[], &owner_key, Instant::now())
        .unwrap();
    service.tokens().invite("bob@gmail.com").unwrap();
    service.tokens().add_admin("carol@gmail.com").unwrap();
    for email in ["bob@gmail.com", "carol@gmail.com"] {
        sign_in(&service, &api, email);
        let signed_in = api.clone().signed_in(None).unwrap();
        for admin in [true, false] {
            let error = signed_in
                .set_admin(&person("ada"), "carol@gmail.com", admin)
                .await
                .unwrap_err();
            assert!(error.to_string().contains("not the owner"), "{error}");
        }
    }
    assert_eq!(
        service.tokens().admins().collect::<Vec<_>>(),
        ["carol@gmail.com"]
    );
}

#[tokio::test]
async fn a_removed_admin_cannot_invite() {
    let (service, api) = start().await;
    sign_in(&service, &api, "ada@gmail.com");
    let ada = api.clone().signed_in(None).unwrap();
    ada.set_admin(&person("ada"), "bob@gmail.com", true)
        .await
        .unwrap();
    let removed = ada
        .set_admin(&person("ada"), "bob@gmail.com", false)
        .await
        .unwrap()
        .done;
    assert_eq!(
        text::admin_set(&removed),
        "bob@gmail.com is now a member, not an admin."
    );

    sign_in(&service, &api, "bob@gmail.com");
    let error = api
        .clone()
        .signed_in(None)
        .unwrap()
        .invite(&person("bob"), "carol@gmail.com")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not an admin"), "{error}");
}

#[tokio::test]
async fn the_owner_passes_the_role_to_a_member() {
    let (service, api) = start().await;
    sign_in(&service, &api, "ada@gmail.com");
    let ada = api.clone().signed_in(None).unwrap();
    ada.invite(&person("ada"), "bob@gmail.com").await.unwrap();
    let error = ada
        .pass_owner(&person("ada"), "carol@gmail.com")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not a member"), "{error}");
    let passed = ada
        .pass_owner(&person("ada"), "Bob@gmail.com")
        .await
        .unwrap()
        .done;
    assert_eq!(
        text::owner_passed(&passed),
        "bob@gmail.com is now the owner. ada@gmail.com stays an admin."
    );

    sign_in(&service, &api, "bob@gmail.com");
    let list = api
        .clone()
        .signed_in(None)
        .unwrap()
        .members()
        .await
        .unwrap();
    // The old owner shows once, as an admin (01M3MN157X8N9QKER1AJEPEJVX).
    assert_eq!(
        text::members(&list),
        "owner: bob@gmail.com\nadmins: ada@gmail.com\nmembers: none\nallowed domains: none"
    );
}

#[tokio::test]
async fn only_the_owner_passes_the_role() {
    let (service, api) = start().await;
    let owner_key = Key::generate().thumbprint();
    service
        .tokens()
        .admit("ada@gmail.com", false, &[], &owner_key, Instant::now())
        .unwrap();
    service.tokens().invite("bob@gmail.com").unwrap();
    sign_in(&service, &api, "bob@gmail.com");
    let error = api
        .clone()
        .signed_in(None)
        .unwrap()
        .pass_owner(&person("bob"), "bob@gmail.com")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not the owner"), "{error}");
    assert_eq!(service.tokens().owner(), Some("ada@gmail.com"));
}

/// Each change of the members posts one note to the thread of each
/// repository of the riff. The note names the user, the email and the
/// change, and wakes no session (01M3MN14ZCTRVD3T455P6TFK1B,
/// 01M3MN1537Z0K3BRK6H2BZKZT0).
#[tokio::test]
async fn each_change_of_the_members_posts_a_note() {
    let (service, api) = start_with(true).await;
    let pair = sign_in(&service, &api, "ada@gmail.com");
    // A live session of ada works in the repository.
    let s1: SessionUri = "riff://ada@pangolin/como-technologies/riff?session=s1"
        .parse()
        .unwrap();
    let session = api.clone().signed_in(Some("s1")).unwrap();
    session.register(&s1).await.unwrap();
    let ada = api.clone().signed_in(None).unwrap();
    let me = person("ada");

    let mut posted = vec![
        ada.invite(&me, "bob@gmail.com").await.unwrap().news,
        ada.set_admin(&me, "carol@gmail.com", true)
            .await
            .unwrap()
            .news,
        ada.set_admin(&me, "carol@gmail.com", false)
            .await
            .unwrap()
            .news,
        ada.remove(&me, "carol@gmail.com").await.unwrap().news,
        ada.pass_owner(&me, "bob@gmail.com").await.unwrap().news,
    ];
    for news in posted.drain(..) {
        let news = news.unwrap();
        assert_eq!(news.len(), 1, "{news:?}");
        assert_eq!(news[0].thread, repo());
        assert!(news[0].woken.is_empty(), "a note wakes no session");
    }

    let notes = session.read(&s1, &repo(), false).await.unwrap();
    let bodies: Vec<&str> = notes.iter().map(|n| n.message.body.as_str()).collect();
    assert_eq!(
        bodies,
        [
            "members: ada invited bob@gmail.com. bob@gmail.com is a member now.",
            "members: ada made carol@gmail.com an admin.",
            "members: ada made carol@gmail.com a member again, not an admin.",
            "members: ada removed carol@gmail.com. carol@gmail.com is not a member now.",
            "members: ada passed the owner role to bob@gmail.com. ada@gmail.com stays an admin.",
        ]
    );
    for note in &notes {
        assert_eq!(note.message.kind, Kind::Note);
        assert_eq!(note.message.from.who(), me.who());
        assert!(note.verified, "the client signs the note");
        assert!(!note.message.body.contains(&pair.access_token));
        assert!(!note.message.body.contains(&pair.refresh_token));
    }
}

/// The change stands when the post of its note fails
/// (01M3MN1537Z0K3BRK6H2BZKZT0).
#[tokio::test]
async fn a_failed_note_leaves_the_change() {
    let (service, api) = start_with(true).await;
    sign_in(&service, &api, "ada@gmail.com");
    let ada = api.clone().signed_in(None).unwrap();
    // The token of ada does not act as mallory, so the post fails.
    let changed = ada
        .invite(&person("mallory"), "bob@gmail.com")
        .await
        .unwrap();
    assert_eq!(changed.done.email, "bob@gmail.com");
    let line = text::members_news(&changed.news);
    assert!(
        line.starts_with("riff: the change is done, but riff cannot post a note of it:"),
        "{line}"
    );
    assert_eq!(
        service.tokens().members().collect::<Vec<_>>(),
        ["bob@gmail.com"]
    );
}
