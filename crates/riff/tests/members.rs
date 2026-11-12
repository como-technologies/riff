//! `riff invite`, `riff remove`, `riff members` and `riff admin` against a
//! real server (01M3JN3AHMK532XMRDASD4XD5D, 01M77671TQ92487GSM24RJG82Z), and
//! the people in `riff who`, with no owner (01M7767T8EFVR79NSHKTK53G7B). The
//! sign-in is in the mock store of `keyring-core`, so the tests run in process.
//!
//! A riff here has sign-in, unless the test is about a riff with no
//! sign-in: such a riff has no people. The tests read the people
//! through [`Service::members`] and the routes, and never through the
//! token store.

use crate::book;
use crate::common;

use std::fs;
use std::time::Instant;

use riff::text;
use riff_client::api::{Api, PauseScope};
use riff_client::login::{self, SignIn};
use riff_core::dpop::Key;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Kind, PersonRole, RiffState, TokenReply};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::Provider;
use riff_state::state::people::last_admin;

/// A riff with sign-in. A call with no token is not refused.
async fn start() -> (Service, Api) {
    start_with(false).await
}

/// A riff with sign-in; with `require_sign_in`, each call needs a
/// token. No test calls the provider.
async fn start_with(require_sign_in: bool) -> (Service, Api) {
    serve(|url| Config {
        require_sign_in,
        provider: Some(provider()),
        ..Config::new(url)
    })
    .await
}

/// A riff with no sign-in: it has no provider, and it trusts each
/// caller.
async fn start_with_no_sign_in() -> (Service, Api) {
    serve(Config::new).await
}

/// A sign-in provider with no allowed domain.
fn provider() -> Provider {
    Provider {
        issuer: "https://accounts.example.com".into(),
        client_id: "riff".into(),
        client_secret: None,
        allowed_domains: Vec::new(),
    }
}

/// A server with the config that `config` makes from its URL.
async fn serve(config: impl FnOnce(&str) -> Config) -> (Service, Api) {
    let (listener, url) = common::listen().await;
    let service = Service::new(config(&url));
    let api = Api::new(&url);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, api)
}

/// Signs in `email` on this device, as the provider sign-in does.
async fn sign_in(service: &Service, api: &Api, email: &str) -> TokenReply {
    let jkt = riff_client::device::key(api.base()).unwrap().thumbprint();
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

/// The agent session `id` of `user`.
fn session(user: &str, id: &str) -> SessionUri {
    format!("riff://{user}@pangolin/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

/// What `riff who` prints to the session `me`, with no color.
async fn who(api: &Api, me: &SessionUri) -> String {
    let pauses = api.pauses(me).await.unwrap();
    let who = api.roster(me, false).await.unwrap();
    let text = riff::view::who(&pauses, &who.sessions, me, false);
    anstream::adapter::strip_str(&text).to_string()
}

/// The line of the session `id` in `shown`.
fn line_of<'a>(shown: &'a str, id: &str) -> &'a str {
    let name = format!("({id})");
    shown.lines().find(|l| l.contains(&name)).unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

#[tokio::test]
async fn the_first_admin_invites_lists_and_removes() {
    let (service, api) = start().await;
    let signed_in = as_person(&service, &api, "ada@gmail.com").await;

    let invited = signed_in.invite("bob@gmail.com").await.unwrap();
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
    assert!(answer.ends_with("\nriff login"), "{answer}");
    let bob_key = Key::generate().thumbprint();
    let bob = service
        .admit("bob@gmail.com", false, &bob_key)
        .await
        .unwrap();

    let list = signed_in.members().await.unwrap();
    assert_eq!(
        members(&list),
        "admins           ada@gmail.com\nmembers          bob@gmail.com\n\
         allowed domains  none\nproof            off\n"
    );

    let removed = signed_in.remove("bob@gmail.com").await.unwrap();
    assert_eq!(
        text::removed(&removed),
        "Removed bob@gmail.com and ended 1 sign-in."
    );
    let check = service
        .tokens()
        .check(&bob.access_token, &bob_key, Instant::now());
    assert!(check.is_err());
    // The person cannot sign in again.
    let refused = service
        .admit("bob@gmail.com", false, &bob_key)
        .await
        .unwrap_err();
    assert!(refused.contains("riff invite bob@gmail.com"), "{refused}");
}

#[tokio::test]
async fn a_member_cannot_invite() {
    // Given the admin ada, who invites bob.
    let (service, api) = start().await;
    let ada = as_person(&service, &api, "ada@gmail.com").await;
    ada.invite("bob@gmail.com").await.unwrap();

    // When the member bob invites a person.
    let error = as_person(&service, &api, "bob@gmail.com")
        .await
        .invite("carol@gmail.com")
        .await
        .unwrap_err();

    // Then the riff refuses, and carol is no member.
    assert!(error.to_string().contains("not an admin"), "{error}");
    assert_eq!(service.members().members, ["bob@gmail.com"]);
}

/// This server has no sign-in provider, so nobody can sign in (R227).
#[tokio::test]
async fn members_needs_a_sign_in() {
    let (_, api) = start_with_no_sign_in().await;
    let error = api.members().await.unwrap_err();
    assert_eq!(
        error.to_string(),
        riff_client::text::nobody_signs_in(api.base())
    );
}

#[tokio::test]
async fn an_admin_makes_an_admin_who_invites_and_removes() {
    let (service, api) = start().await;
    let added = as_person(&service, &api, "ada@gmail.com")
        .await
        .set_admin("Bob@gmail.com", true)
        .await
        .unwrap();
    assert_eq!(
        text::admin_set(&added),
        "bob@gmail.com is now an admin. They can invite and remove members."
    );

    let bob = as_person(&service, &api, "bob@gmail.com").await;
    bob.invite("carol@gmail.com").await.unwrap();
    bob.invite("dan@gmail.com").await.unwrap();
    bob.remove("dan@gmail.com").await.unwrap();
    let list = bob.members().await.unwrap();
    assert_eq!(
        members(&list),
        "admins           ada@gmail.com, bob@gmail.com\nmembers          carol@gmail.com\n\
         allowed domains  none\nproof            off\n"
    );
}

/// A member cannot add or remove an admin (01M77671TQ92487GSM24RJG82Z). The
/// words of the refusal count, not the number of the status.
#[tokio::test]
async fn only_an_admin_adds_an_admin() {
    // Given the first admin ada, and the member bob.
    let (service, api) = start().await;
    let ada = as_person(&service, &api, "ada@gmail.com").await;
    ada.invite("bob@gmail.com").await.unwrap();

    // When the member bob adds or removes an admin, the riff refuses.
    let bob = as_person(&service, &api, "bob@gmail.com").await;
    for (email, admin) in [("bob@gmail.com", true), ("ada@gmail.com", false)] {
        let error = bob.set_admin(email, admin).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("bob is not an admin; only an admin adds or removes an admin"),
            "{error}"
        );
    }
    // Then the roles are as before.
    let list = service.members();
    assert_eq!(list.admins, ["ada@gmail.com"]);
    assert_eq!(list.members, ["bob@gmail.com"]);
}

/// Each admin adds and removes an admin, also the first admin, and the
/// riff keeps its last admin (01M77671TQ92487GSM24RJG82Z,
/// 01M7767SY5PW1AMB0PBMW2GXDX).
#[tokio::test]
async fn each_admin_adds_and_removes_an_admin_and_the_last_admin_stays() {
    // Given the first admin ada.
    let (service, api) = start().await;
    let ada = as_person(&service, &api, "ada@gmail.com").await;

    // The last admin stays, and nothing changes.
    let error = ada.set_admin("ada@gmail.com", false).await.unwrap_err();
    assert!(
        error.to_string().contains(&last_admin("ada@gmail.com")),
        "{error}"
    );
    assert_eq!(service.members().admins, ["ada@gmail.com"]);

    // ada makes bob an admin, and the admin bob removes ada.
    ada.set_admin("bob@gmail.com", true).await.unwrap();
    let bob = as_person(&service, &api, "bob@gmail.com").await;
    let removed = bob.set_admin("ada@gmail.com", false).await.unwrap();
    assert_eq!(
        text::admin_set(&removed),
        "ada@gmail.com is now a member, not an admin."
    );
    // Each person shows once, with the highest role
    // (01M3MN157X8N9QKER1AJEPEJVX).
    assert_eq!(
        members(&bob.members().await.unwrap()),
        "admins           bob@gmail.com\nmembers          ada@gmail.com\n\
         allowed domains  none\nproof            off\n"
    );
    // Now bob is the last admin.
    let error = bob.set_admin("bob@gmail.com", false).await.unwrap_err();
    assert!(
        error.to_string().contains(&last_admin("bob@gmail.com")),
        "{error}"
    );
}

#[tokio::test]
async fn a_removed_admin_cannot_invite() {
    let (service, api) = start().await;
    let ada = as_person(&service, &api, "ada@gmail.com").await;
    ada.set_admin("bob@gmail.com", true).await.unwrap();
    let removed = ada.set_admin("bob@gmail.com", false).await.unwrap();
    assert_eq!(
        text::admin_set(&removed),
        "bob@gmail.com is now a member, not an admin."
    );

    // The person stays a member, and signs in with no invite.
    let error = as_person(&service, &api, "bob@gmail.com")
        .await
        .invite("carol@gmail.com")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not an admin"), "{error}");
    assert_eq!(service.members().members, ["bob@gmail.com"]);
}

/// Each change of the members posts one line of riff-server in the chat,
/// and no note to the thread of a repository. The line names the user,
/// the email and the change, holds no token, and has no `to`, so it
/// wakes no session (01M76S6128EZRZS2D5CE2QCC05,
/// 01M77BSQQGGNE83ECS27YGVEP1).
#[tokio::test]
async fn each_change_of_the_members_posts_one_line_in_the_chat() {
    let (service, api) = start_with(true).await;
    let pair = sign_in(&service, &api, "ada@gmail.com").await;
    // A live session of ada works in the repository.
    let s1: SessionUri = "riff://ada@pangolin/como-technologies/riff?session=s1"
        .parse()
        .unwrap();
    let session = api.clone().signed_in(Some("s1")).unwrap();
    session.register(&s1).await.unwrap();
    let ada = api.clone().signed_in(None).unwrap();

    ada.invite("bob@gmail.com").await.unwrap();
    ada.set_admin("carol@gmail.com", true).await.unwrap();
    ada.set_admin("carol@gmail.com", false).await.unwrap();
    ada.remove("carol@gmail.com").await.unwrap();

    let notes = session.read(&s1, &repo(), true).await.unwrap();
    assert!(notes.is_empty(), "no note in the repository: {notes:#?}");
    let lines = session.read(&s1, &ThreadName::chat(), true).await.unwrap();
    let bodies: Vec<&str> = lines.iter().map(|l| l.message.body.as_str()).collect();
    assert_eq!(
        bodies,
        [
            "members: ada invited bob@gmail.com. bob@gmail.com is a member now.",
            "members: ada made carol@gmail.com an admin.",
            "members: ada made carol@gmail.com a member again, not an admin.",
            "members: ada removed carol@gmail.com. carol@gmail.com is not a member now.",
        ]
    );
    for line in &lines {
        assert_eq!(line.message.from, SessionUri::server());
        assert_eq!(line.message.kind, Kind::Riff);
        assert!(line.message.to.is_empty(), "{line:?}");
        assert!(!line.message.body.contains(&pair.access_token));
        assert!(!line.message.body.contains(&pair.refresh_token));
    }
}

/// The USER and the role of each person in the reply to `who`.
async fn people(api: &Api, me: &SessionUri) -> Vec<(String, PersonRole)> {
    let who = api.roster(me, false).await.unwrap();
    who.people.into_iter().map(|p| (p.user, p.role)).collect()
}

/// `riff who` names no owner (01M7767T8EFVR79NSHKTK53G7B): the line after the
/// state is the build. An admin is a person: no session gets the tag `admin`,
/// and `who` gives each member with a role (01M3NT4M159EHN5W8JRTQ417N4,
/// 01M3NT4M3A4E3K5S2NM7MS6PQD). The first person who signs in is an
/// admin (01M7767SRX4K1N1BCDHT7M85DN).
#[tokio::test]
async fn who_shows_each_admin_as_a_person() {
    let (service, api) = start_with(true).await;
    sign_in(&service, &api, "ada@gmail.com").await;
    let a1 = session("ada", "a1");
    let ada = api.clone().signed_in(Some("a1")).unwrap();
    ada.register(&a1).await.unwrap();
    let namer = api.clone().signed_in(None).unwrap();
    crate::common::name_lead(&namer, &a1).await;

    let shown = who(&ada, &a1).await;
    let second = shown.lines().nth(1).unwrap();
    assert!(second.starts_with("build  v"), "{shown}");
    assert!(!shown.contains("owner"), "{shown}");
    let line = line_of(&shown, "a1");
    assert!(line.contains("  you lead"), "{shown}");
    assert_eq!(people(&ada, &a1).await, [("ada".into(), PersonRole::Admin)]);

    // Only a person changes the people: the token of the person, not
    // of the session.
    let as_person = api.clone().signed_in(None).unwrap();
    as_person.set_admin("bob@gmail.com", true).await.unwrap();

    sign_in(&service, &api, "bob@gmail.com").await;
    let b1 = session("bob", "b1");
    let bob = api.clone().signed_in(Some("b1")).unwrap();
    bob.register(&b1).await.unwrap();
    let namer = api.clone().signed_in(None).unwrap();
    crate::common::name_lead(&namer, &b1).await;
    let shown = who(&bob, &b1).await;
    let line = line_of(&shown, "b1");
    assert!(line.contains("  you lead"), "{shown}");
    assert!(!line_of(&shown, "a1").contains("admin"), "{shown}");
    assert!(!shown.contains("owner"), "{shown}");
    let who = bob.roster(&b1, false).await.unwrap();
    let bob_row = who.people.iter().find(|p| p.user == "bob").unwrap();
    assert_eq!(bob_row.role, PersonRole::Admin);
    assert_eq!(bob_row.seen_secs, Some(0));
    assert_eq!(
        people(&bob, &b1).await,
        [
            ("ada".into(), PersonRole::Admin),
            ("bob".into(), PersonRole::Admin)
        ]
    );
}

/// A riff with no sign-in names no owner in `riff who`, also when its
/// settings name an admin (01M7767T8EFVR79NSHKTK53G7B).
#[tokio::test]
async fn who_on_a_riff_with_no_sign_in_names_no_owner() {
    let (_, api) = serve(|url| Config {
        admins: vec!["ada@gmail.com".into()],
        ..Config::new(url)
    })
    .await;
    let a1 = session("ada", "a1");
    api.register(&a1).await.unwrap();
    let shown = who(&api, &a1).await;
    let second = shown.lines().nth(1).unwrap();
    assert!(second.starts_with("build  v"), "{shown}");
    assert!(!shown.contains("owner"), "{shown}");
}

/// The `who` tool of `riff mcp` names no owner, and has no escape codes
/// (01M7767T8EFVR79NSHKTK53G7B).
#[tokio::test]
async fn the_who_tool_names_no_owner() {
    use riff::tools::Tools;
    use rmcp::ServiceExt;
    use rmcp::model::CallToolRequestParams;

    let (service, api) = start_with(true).await;
    sign_in(&service, &api, "ada@gmail.com").await;
    let a1 = session("ada", "a1");
    let ada = api.clone().signed_in(Some("a1")).unwrap();
    ada.register(&a1).await.unwrap();
    let namer = api.clone().signed_in(None).unwrap();
    crate::common::name_lead(&namer, &a1).await;
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let tools = Tools::new(ada, a1);
    tokio::spawn(async move {
        tools
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = ().serve(client_io).await.unwrap();
    let result = client
        .call_tool(CallToolRequestParams::new("who"))
        .await
        .unwrap();
    let shown: String = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect();
    assert!(!shown.contains("owner"), "{shown}");
    let line = line_of(&shown, "a1");
    assert!(line.contains(" (you) lead  riff://"), "{shown}");
    assert!(!shown.contains('\x1b'), "{shown:?}");
}

/// The `riff who` how-to of the book shows the facts with no owner, and
/// the tags of the sessions (01M7767T8EFVR79NSHKTK53G7B).
#[test]
fn the_book_shows_who_with_no_owner() {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let (_, section) = page.split_once("## See who is in the riff").unwrap();
    let (section, _) = section.split_once("\n## ").unwrap();
    assert!(section.contains("\nriff   running\nbuild  v"), "{section}");
    assert!(!section.contains("owner"), "{section}");
    assert!(section.contains("  you lead  working on #6\n"), "{section}");
    assert!(section.contains("  worker    working on #7  "), "{section}");
}

/// Only the admins pause and resume the whole riff, or a
/// repository by its name (01M3XAHZDSQR263QZVB41CK0MX). A lead that is
/// not an admin gets a refusal with the reason, and still pauses its
/// own repository. The other repository goes on.
#[tokio::test]
async fn only_the_admins_pause_the_whole_riff() {
    let (service, api) = start_with(true).await;
    sign_in(&service, &api, "ada@gmail.com").await;
    let a1 = session("ada", "a1");
    let ada = api.clone().signed_in(Some("a1")).unwrap();
    ada.register(&a1).await.unwrap();
    let namer = api.clone().signed_in(None).unwrap();
    crate::common::name_lead(&namer, &a1).await;
    // The first admin resumes the new riff, as its lead.
    let (reply, _) = ada.set_riff(&a1, RiffState::Running).await.unwrap();
    assert!(reply.changed);
    // Only a person changes the people: the token of the person.
    let as_person = api.clone().signed_in(None).unwrap();
    as_person.invite("bob@gmail.com").await.unwrap();

    sign_in(&service, &api, "bob@gmail.com").await;
    let b1: SessionUri = "riff://bob@kadomony/como-technologies/strata?session=b1"
        .parse()
        .unwrap();
    let bob = api.clone().signed_in(Some("b1")).unwrap();
    bob.register(&b1).await.unwrap();
    let namer = api.clone().signed_in(None).unwrap();
    crate::common::name_lead(&namer, &b1).await;
    for scope in [PauseScope::Riff, PauseScope::Repository(repo())] {
        for state in [RiffState::Paused, RiffState::Running] {
            let error = bob.set_pause(&b1, &scope, state).await.unwrap_err();
            let error = format!("{error:#}");
            let verb = match state {
                RiffState::Paused => "pause",
                RiffState::Running => "resume",
            };
            let reason = format!(
                "bob is not an admin; only an admin can {verb} the whole riff or a repository \
                 by its name"
            );
            assert!(error.contains("403"), "{error}");
            assert!(error.contains(&reason), "{error}");
        }
    }
    let pauses = bob.pauses(&b1).await.unwrap();
    assert!(pauses.riff.is_none() && pauses.repositories.is_empty());

    // The lead of strata pauses its own repository.
    let (reply, _) = bob
        .set_pause(&b1, &PauseScope::Here, RiffState::Paused)
        .await
        .unwrap();
    assert!(reply.changed && reply.riff.is_none());
    assert_eq!(reply.state, RiffState::Paused);

    // The admin ada goes on in its repository. It pauses the whole riff,
    // and it resumes the repository of bob by its name.
    sign_in(&service, &api, "ada@gmail.com").await;
    let ada = api.clone().signed_in(Some("a1")).unwrap();
    assert_eq!(ada.riff(&a1).await.unwrap(), RiffState::Running);
    assert!(ada.claim(&a1, &repo(), "issue-12").await.unwrap().granted);
    let strata = b1.default_thread().unwrap();
    let (reply, _) = ada
        .set_pause(&a1, &PauseScope::Repository(strata), RiffState::Running)
        .await
        .unwrap();
    assert!(reply.changed && reply.repositories.is_empty());
    let (reply, _) = ada.set_riff(&a1, RiffState::Paused).await.unwrap();
    assert!(reply.changed && reply.riff.is_some());
    let as_person = api.clone().signed_in(None).unwrap();
    as_person.set_admin("bob@gmail.com", true).await.unwrap();

    // The new admin bob resumes the whole riff.
    sign_in(&service, &api, "bob@gmail.com").await;
    let bob = api.clone().signed_in(Some("b1")).unwrap();
    let (reply, _) = bob.set_riff(&b1, RiffState::Running).await.unwrap();
    assert!(reply.changed && reply.riff.is_none());
}

/// The book has one how-to for `riff members`: "See the members" in
/// "Start a Team Riff". No other page has the heading or the command.
/// The example of the page is what the command prints.
#[test]
fn the_book_has_one_how_to_for_riff_members() {
    let mut headings = Vec::new();
    let mut commands = Vec::new();
    for entry in fs::read_dir(book::dir()).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if !name.ends_with(".md") || name == "requirements.md" || name == "requirements-history.md"
        {
            continue;
        }
        let page = book::page(&name);
        for line in page.lines().filter(|l| l.ends_with("See the members")) {
            headings.push(format!("{name}: {line}"));
        }
        if book::commands_in(&page).contains(&"riff members".to_owned()) {
            commands.push(name);
        }
    }
    assert_eq!(headings, ["start-a-team-riff.md: ## See the members"]);
    assert_eq!(commands, ["start-a-team-riff.md"]);

    let part = book::part("start-a-team-riff.md", "See the members");
    let shown = book::commands_in(&part);
    assert_eq!(shown, ["riff members"]);
    book::each_is_real(&shown);
    let list = riff_core::wire::MembersReply {
        admins: vec!["ada@example.com".into(), "mike@example.com".into()],
        members: vec!["bob@gmail.com".into()],
        allowed_domains: vec!["example.com".into()],
        proof: riff_core::proof::Policy::Off,
    };
    assert!(part.contains(&members(&list)), "{part}");
    assert!(!part.contains("owner"), "{part}");
}

/// `riff members` with no color.
fn members(list: &riff_core::wire::MembersReply) -> String {
    anstream::adapter::strip_str(&riff::view::members(list)).to_string()
}
