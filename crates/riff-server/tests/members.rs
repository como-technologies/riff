//! The owner and the members of a riff, over HTTP
//! (01M3JN3AD44CC98AGMVP43F56G, 01M3JN3AFA2SAX0CEC1Y6E4NM5,
//! 01M3JN3AHMK532XMRDASD4XD5D, 01M3JN3ANE676DT5WQ2NTG47DK,
//! 01M3JY7T109BR860EQBSKEFDHY, 01M3JY7T3645CMQ8CS4T4ABZTP,
//! 01M3JYX8NPZASQY6031R35H39P, 01M3JYX8QSEZDB5RZJ3Y57DR4Y).

mod common;

use std::sync::Arc;
use std::time::Duration;

use riff_core::dpop::Key;
use riff_core::wire::{
    AdminSet, ID_TOKEN_TYPE, Idle, Invite, Invited, MembersReply, OwnerPassed, PassOwner, Remove,
    Removed, SetAdmin, TOKEN_EXCHANGE, TokenError, TokenReply, TokenRequest,
};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::{DEFAULT_DOMAIN, Provider};
use riff_server::store::{Memory, Store};

/// A riff with sign-in. `admins` are the admin emails of the settings.
fn config(url: &str, issuer: &str, admins: &[&str]) -> Config {
    Config {
        provider: Some(Provider {
            issuer: issuer.into(),
            client_id: "riff-client".into(),
            client_secret: None,
            allowed_domains: vec![DEFAULT_DOMAIN.into()],
        }),
        admins: admins.iter().map(|a| a.to_string()).collect(),
        lease: common::LEASE,
        save_every: common::SAVE_EVERY,
        ..Config::new(url)
    }
}

/// Serves a riff on a free port. With a store, it loads from it.
async fn serve(
    issuer: &str,
    admins: &[&str],
    owner: Option<&str>,
    store: Option<Arc<dyn Store>>,
) -> (Service, String) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut config = config(&url, issuer, admins);
    config.owner = owner.map(str::to_owned);
    let service = match store {
        Some(store) => Service::load(config, store).await.unwrap(),
        None => Service::new(config),
    };
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// A person who signed in: the key of the device and the tokens.
struct Person {
    key: Key,
    pair: TokenReply,
}

/// Signs in `email` at `url`. `domain` is the `hd` of the account:
/// `None` for a personal account. Gives the person, or the refusal.
async fn sign_in(
    url: &str,
    issuer: &str,
    email: &str,
    domain: Option<&str>,
) -> Result<Person, TokenError> {
    let key = Key::generate();
    let form = TokenRequest {
        grant_type: TOKEN_EXCHANGE.into(),
        subject_token: Some(common::id_token(issuer, email, domain)),
        subject_token_type: Some(ID_TOKEN_TYPE.into()),
        ..TokenRequest::default()
    };
    let reply = common::post(&format!("{url}/v1/token"), &key, None)
        .form(&form)
        .send()
        .await
        .unwrap();
    if reply.status() == 200 {
        Ok(Person {
            key,
            pair: reply.json().await.unwrap(),
        })
    } else {
        Err(reply.json().await.unwrap())
    }
}

/// A call by `person` to one of the member routes. Gives the status
/// and the body.
async fn call(url: &str, person: &Person, op: &str, body: serde_json::Value) -> (u16, String) {
    let reply = common::post(
        &format!("{url}/v1/{op}"),
        &person.key,
        Some(&person.pair.access_token),
    )
    .json(&body)
    .send()
    .await
    .unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

async fn invite(url: &str, by: &Person, email: &str) -> (u16, String) {
    let body = serde_json::to_value(Invite {
        email: email.into(),
    })
    .unwrap();
    call(url, by, "invite", body).await
}

async fn remove(url: &str, by: &Person, email: &str) -> (u16, String) {
    let body = serde_json::to_value(Remove {
        email: email.into(),
    })
    .unwrap();
    call(url, by, "remove", body).await
}

async fn set_admin(url: &str, by: &Person, email: &str, admin: bool) -> (u16, String) {
    let body = serde_json::to_value(SetAdmin {
        email: email.into(),
        admin,
    })
    .unwrap();
    call(url, by, "admin", body).await
}

async fn pass_owner(url: &str, by: &Person, email: &str) -> (u16, String) {
    let body = serde_json::to_value(PassOwner {
        email: email.into(),
    })
    .unwrap();
    call(url, by, "owner", body).await
}

async fn members(url: &str, by: &Person) -> MembersReply {
    let (status, body) = call(url, by, "members", serde_json::json!({})).await;
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

/// The path of the acceptance test: the owner signs in to a new riff,
/// invites a second person, the riff refuses a third person, the owner
/// removes the second person, and the sign-ins of the second person
/// end. Each of them has a personal account, with no `hd`.
#[tokio::test]
async fn the_owner_invites_and_removes_a_person() {
    let issuer = common::fake_provider().await;
    let (service, url) = serve(&issuer, &[], None, None).await;

    let ada = sign_in(&url, &issuer, "Ada@gmail.com", None).await.unwrap();
    assert_eq!(service.tokens().owner(), Some("ada@gmail.com"));

    // Before the invite, the riff refuses the personal account.
    let refused = sign_in(&url, &issuer, "bob@gmail.com", None)
        .await
        .err()
        .unwrap();
    assert_eq!(refused.error, "access_denied");
    let why = refused.error_description.unwrap();
    assert!(why.contains("riff invite bob@gmail.com"), "{why}");

    let (status, body) = invite(&url, &ada, " Bob@Gmail.com").await;
    assert_eq!(status, 200, "{body}");
    let invited: Invited = serde_json::from_str(&body).unwrap();
    assert_eq!(invited.email, "bob@gmail.com");
    // The reply names the public address of the riff (01M3MEF4B33Z6WVJMDP29C7SS2).
    assert_eq!(invited.address, url);
    let bob = sign_in(&url, &issuer, "bob@gmail.com", None).await.unwrap();
    assert_eq!(bob.pair.user, "bob");

    // A third person, with no invite, is refused.
    let carol = sign_in(&url, &issuer, "carol@gmail.com", None).await;
    assert_eq!(carol.err().unwrap().error, "access_denied");

    let list = members(&url, &bob).await;
    assert_eq!(list.owner.as_deref(), Some("ada@gmail.com"));
    assert_eq!(list.members, ["bob@gmail.com"]);
    assert_eq!(list.allowed_domains, [DEFAULT_DOMAIN]);

    let (status, body) = remove(&url, &ada, "bob@gmail.com").await;
    assert_eq!(status, 200, "{body}");
    let removed: Removed = serde_json::from_str(&body).unwrap();
    assert_eq!(
        (removed.email.as_str(), removed.sign_ins),
        ("bob@gmail.com", 1)
    );

    // Each sign-in of bob ended, and he cannot sign in again.
    let (status, _) = call(&url, &bob, "members", serde_json::json!({})).await;
    assert_eq!(status, 401);
    assert!(sign_in(&url, &issuer, "bob@gmail.com", None).await.is_err());
    assert!(members(&url, &ada).await.members.is_empty());
}

/// Only the owner or an admin invites and removes. A member gets 403.
/// The owner stays.
#[tokio::test]
async fn only_an_admin_invites_and_removes() {
    let issuer = common::fake_provider().await;
    let (_, url) = serve(&issuer, &[], None, None).await;
    let ada = sign_in(&url, &issuer, "ada@gmail.com", None).await.unwrap();
    assert_eq!(invite(&url, &ada, "bob@gmail.com").await.0, 200);
    let bob = sign_in(&url, &issuer, "bob@gmail.com", None).await.unwrap();

    let (status, body) = invite(&url, &bob, "carol@gmail.com").await;
    assert_eq!(status, 403);
    assert!(body.contains("only an admin can invite"), "{body}");
    let (status, body) = remove(&url, &bob, "ada@gmail.com").await;
    assert_eq!(status, 403);
    assert!(body.contains("only an admin can remove"), "{body}");
    assert!(
        sign_in(&url, &issuer, "carol@gmail.com", None)
            .await
            .is_err()
    );

    let (status, body) = remove(&url, &ada, "ada@gmail.com").await;
    assert_eq!(status, 400);
    assert!(body.contains("owner stays"), "{body}");
    let (status, _) = invite(&url, &ada, "not-an-email").await;
    assert_eq!(status, 400);
}

/// On a riff with admins, only an admin becomes the owner. An admin
/// who is not the owner invites too.
#[tokio::test]
async fn on_a_riff_with_admins_only_an_admin_becomes_the_owner() {
    let issuer = common::fake_provider().await;
    let boss = "boss@comotechnologies.io";
    let (service, url) = serve(&issuer, &[boss], None, None).await;

    // A person of an allowed domain signs in, but is not the owner.
    let worker = sign_in(
        &url,
        &issuer,
        "worker@comotechnologies.io",
        Some(DEFAULT_DOMAIN),
    )
    .await
    .unwrap();
    assert_eq!(service.tokens().owner(), None);
    // A personal account is refused: the riff has admins.
    assert!(sign_in(&url, &issuer, "eve@gmail.com", None).await.is_err());
    assert_eq!(invite(&url, &worker, "eve@gmail.com").await.0, 403);

    let boss = sign_in(&url, &issuer, boss, Some(DEFAULT_DOMAIN))
        .await
        .unwrap();
    assert_eq!(service.tokens().owner(), Some("boss@comotechnologies.io"));
    assert_eq!(invite(&url, &boss, "eve@gmail.com").await.0, 200);
    assert!(sign_in(&url, &issuer, "eve@gmail.com", None).await.is_ok());
}

/// `--owner` names the owner up front. The first other person is then
/// not the owner, and needs an invite or an allowed domain.
#[tokio::test]
async fn the_owner_setting_names_the_owner() {
    let issuer = common::fake_provider().await;
    let (service, url) = serve(&issuer, &[], Some("Ada@gmail.com"), None).await;
    assert_eq!(service.tokens().owner(), Some("ada@gmail.com"));
    assert!(sign_in(&url, &issuer, "bob@gmail.com", None).await.is_err());
    let ada = sign_in(&url, &issuer, "ada@gmail.com", None).await.unwrap();
    assert_eq!(invite(&url, &ada, "bob@gmail.com").await.0, 200);
}

/// The owner and the members stay after a restart on the same store.
#[tokio::test]
async fn the_owner_and_the_members_stay_after_a_restart() {
    let issuer = common::fake_provider().await;
    let store = Memory::default();
    let (old, url) = serve(&issuer, &[], None, Some(Arc::new(store.clone()))).await;
    let ada = sign_in(&url, &issuer, "ada@gmail.com", None).await.unwrap();
    assert_eq!(invite(&url, &ada, "bob@gmail.com").await.0, 200);
    old.save().await.unwrap();

    // A new server on the same store. A setting does not replace the
    // owner that the store holds.
    let (new, url) = serve(&issuer, &[], Some("other@gmail.com"), Some(Arc::new(store))).await;
    tokio::time::timeout(Duration::from_secs(5), old.stopped())
        .await
        .unwrap();
    assert_eq!(new.tokens().owner(), Some("ada@gmail.com"));
    assert_eq!(
        new.tokens().members().collect::<Vec<_>>(),
        ["bob@gmail.com"]
    );
    assert!(sign_in(&url, &issuer, "bob@gmail.com", None).await.is_ok());
    assert!(
        sign_in(&url, &issuer, "carol@gmail.com", None)
            .await
            .is_err()
    );
}

/// The owner makes an admin, who invites and removes. Only the owner
/// adds or removes an admin. The admins stay after a restart on the
/// same store, and the admins of the settings add to them.
#[tokio::test]
async fn the_owner_makes_an_admin_that_stays_after_a_restart() {
    let issuer = common::fake_provider().await;
    let store = Memory::default();
    let (old, url) = serve(&issuer, &[], None, Some(Arc::new(store.clone()))).await;
    let ada = sign_in(&url, &issuer, "ada@gmail.com", None).await.unwrap();
    let (status, body) = set_admin(&url, &ada, "Bob@gmail.com", true).await;
    assert_eq!(status, 200, "{body}");
    let set: AdminSet = serde_json::from_str(&body).unwrap();
    assert_eq!((set.email.as_str(), set.admin), ("bob@gmail.com", true));

    let bob = sign_in(&url, &issuer, "bob@gmail.com", None).await.unwrap();
    assert_eq!(invite(&url, &bob, "carol@gmail.com").await.0, 200);
    let carol = sign_in(&url, &issuer, "carol@gmail.com", None)
        .await
        .unwrap();
    // Neither an admin nor a member adds or removes an admin.
    for (by, admin) in [(&bob, true), (&bob, false), (&carol, true)] {
        let (status, body) = set_admin(&url, by, "carol@gmail.com", admin).await;
        assert_eq!(status, 403);
        assert!(body.contains("only the owner"), "{body}");
    }
    // An admin cannot remove an admin, nor the owner remove the owner role.
    let (status, body) = remove(&url, &bob, "bob@gmail.com").await;
    assert_eq!(status, 400);
    assert!(body.contains("riff admin remove"), "{body}");
    assert_eq!(set_admin(&url, &ada, "ada@gmail.com", false).await.0, 400);
    old.save().await.unwrap();

    let (new, url) = serve(&issuer, &["Dan@gmail.com"], None, Some(Arc::new(store))).await;
    tokio::time::timeout(Duration::from_secs(5), old.stopped())
        .await
        .unwrap();
    assert_eq!(new.tokens().admins().collect::<Vec<_>>(), ["bob@gmail.com"]);
    let bob = sign_in(&url, &issuer, "bob@gmail.com", None).await.unwrap();
    let list = members(&url, &bob).await;
    assert_eq!(list.admins, ["bob@gmail.com", "dan@gmail.com"]);
    assert_eq!(remove(&url, &bob, "carol@gmail.com").await.0, 200);

    // The owner makes bob a member again. He can no longer invite.
    let ada = sign_in(&url, &issuer, "ada@gmail.com", None).await.unwrap();
    assert_eq!(set_admin(&url, &ada, "bob@gmail.com", false).await.0, 200);
    let (status, body) = invite(&url, &bob, "carol@gmail.com").await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(members(&url, &ada).await.members, ["bob@gmail.com"]);
}

/// The owner passes the role to a member. The member is the owner, and
/// the old owner an admin. Only the owner passes it, and only to a
/// member or an admin. The new owner stays after a restart on the same
/// store, also with the old `--owner` setting.
#[tokio::test]
async fn the_owner_passes_the_role_that_stays_after_a_restart() {
    let issuer = common::fake_provider().await;
    let store = Memory::default();
    let ada_owner = Some("ada@gmail.com");
    let (old, url) = serve(&issuer, &[], ada_owner, Some(Arc::new(store.clone()))).await;
    let ada = sign_in(&url, &issuer, "ada@gmail.com", None).await.unwrap();
    assert_eq!(invite(&url, &ada, "bob@gmail.com").await.0, 200);
    let bob = sign_in(&url, &issuer, "bob@gmail.com", None).await.unwrap();

    // A member cannot pass the role.
    let (status, body) = pass_owner(&url, &bob, "bob@gmail.com").await;
    assert_eq!(status, 403);
    assert!(body.contains("only the owner"), "{body}");
    // The owner cannot pass it to a person who is not a member.
    let (status, body) = pass_owner(&url, &ada, "carol@gmail.com").await;
    assert_eq!(status, 400);
    assert!(body.contains("not a member"), "{body}");
    assert_eq!(old.tokens().owner(), Some("ada@gmail.com"));

    let (status, body) = pass_owner(&url, &ada, "Bob@gmail.com").await;
    assert_eq!(status, 200, "{body}");
    let passed: OwnerPassed = serde_json::from_str(&body).unwrap();
    assert_eq!(
        (passed.owner.as_str(), passed.admin.as_str()),
        ("bob@gmail.com", "ada@gmail.com")
    );
    // The old owner is an admin now: can invite, but not pass the role.
    assert_eq!(invite(&url, &ada, "carol@gmail.com").await.0, 200);
    assert_eq!(pass_owner(&url, &ada, "carol@gmail.com").await.0, 403);
    old.save().await.unwrap();

    let (new, url) = serve(&issuer, &[], ada_owner, Some(Arc::new(store))).await;
    tokio::time::timeout(Duration::from_secs(5), old.stopped())
        .await
        .unwrap();
    assert_eq!(new.tokens().owner(), Some("bob@gmail.com"));
    let bob = sign_in(&url, &issuer, "bob@gmail.com", None).await.unwrap();
    let list = members(&url, &bob).await;
    assert_eq!(list.owner.as_deref(), Some("bob@gmail.com"));
    // The old owner shows once, as an admin (01M3MN157X8N9QKER1AJEPEJVX).
    assert_eq!(list.admins, ["ada@gmail.com"]);
    assert_eq!(list.members, ["carol@gmail.com"]);
    assert_eq!(set_admin(&url, &bob, "ada@gmail.com", false).await.0, 200);
}

/// Only the owner or an admin sets the settings of idle workers. A
/// member reads them, and gets 403 on a change
/// (01M3Q5A0TF9K49V8Z1ZY9NDF74).
#[tokio::test]
async fn only_an_admin_sets_the_idle_workers() {
    let issuer = common::fake_provider().await;
    let (_, url) = serve(&issuer, &[], None, None).await;
    let ada = sign_in(&url, &issuer, "ada@gmail.com", None).await.unwrap();
    assert_eq!(invite(&url, &ada, "bob@gmail.com").await.0, 200);
    let bob = sign_in(&url, &issuer, "bob@gmail.com", None).await.unwrap();
    let idle = |me: &str, per_host: Option<u16>, after: Option<u64>| serde_json::json!({ "me": me, "per_host": per_host, "after_secs": after });
    let read = |body: &str| serde_json::from_str::<Idle>(body).unwrap();

    let (status, body) = call(&url, &bob, "idle", idle("riff://bob@b", None, None)).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(read(&body), Idle::default());
    let (status, body) = call(&url, &bob, "idle", idle("riff://bob@b", Some(3), None)).await;
    assert_eq!(status, 403);
    assert!(
        body.contains("only an admin can change the settings of idle workers"),
        "{body}"
    );

    let set = Idle {
        per_host: 3,
        after_secs: 30,
    };
    let (status, body) = call(&url, &ada, "idle", idle("riff://ada@a", Some(3), Some(30))).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(read(&body), set);
    let (status, body) = call(&url, &ada, "idle", idle("riff://ada@a", None, Some(0))).await;
    assert_eq!(status, 400, "{body}");
    let (_, body) = call(&url, &bob, "idle", idle("riff://bob@b", None, None)).await;
    assert_eq!(read(&body), set);
}
