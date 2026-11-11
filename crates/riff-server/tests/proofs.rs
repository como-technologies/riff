//! A proof of presence over HTTP (PROOF_ASK, PROOF_CHECK, PROOF_LOG,
//! PROOF_POLICY, PROOF_SET, PROOF_REFUSE, PROOF_HOSTS, PROOF_GATE,
//! PROOF_APPROVE, PROOF_KEYS, PROOF_SHOW): the page of a proof with
//! the signature of a fake authenticator, the policy, and each action
//! with and with no proof.

use crate::common;

use std::time::Duration;

use riff_core::device::fake::{Authenticator, Proof, Shape};
use riff_core::dpop::Key;
use riff_core::name::{SessionUri, ThreadName, Who};
use riff_core::proof::{Action, PROOF_HEADER, Policy};
use riff_core::selector::Selector;
use riff_core::wire::{
    KeyOptions, KeyStarted, MembersReply, PolicyReply, Post, ProofOptions, ProofStarted,
    ProofState, REFUSED_HEADER, RiffReply, ThreadsReply, TokenReply, WhoReply,
};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::tools::show;
use riff_state::store::Memory;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;

/// A riff with sign-in on a free port, with its log in memory.
struct Riff {
    service: Service,
    store: Memory,
}

impl Riff {
    /// The records of the log of riff-server, as text, one on a line.
    async fn log(&self) -> String {
        let replayed = riff_state::log::replay(&self.store).await.unwrap();
        let lines: Vec<String> = replayed.records.iter().map(show).collect();
        lines.join("\n")
    }
}

/// Serves a riff with sign-in on a free port. Each proof is good for
/// `life`.
async fn serve(life: Duration) -> (Riff, String) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        require_sign_in: true,
        lease: common::LEASE,
        save_every: common::SAVE_EVERY,
        proof_life: life,
        ..Config::new(&url)
    };
    let store = Memory::default();
    let service = Service::load(config, Arc::new(store.clone()))
        .await
        .unwrap();
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (Riff { service, store }, url)
}

/// A caller: the key of its device, its token and its URI.
struct Caller {
    key: Key,
    token: String,
    me: SessionUri,
}

/// The reply of a call: the status, the code of a refusal, and the
/// body.
#[derive(Debug)]
struct Replied {
    status: u16,
    code: Option<String>,
    body: String,
}

impl Replied {
    fn json<T: serde::de::DeserializeOwned>(&self) -> T {
        assert_eq!(self.status, 200, "{self:?}");
        serde_json::from_str(&self.body).unwrap()
    }

    /// Asserts a refusal with the status `status` and the code `code`.
    fn refused(&self, status: u16, code: &str) {
        assert_eq!(self.status, status, "{self:?}");
        assert_eq!(self.code.as_deref(), Some(code), "{self:?}");
    }
}

/// The person `user` signs in: the first one is the owner.
async fn person(service: &Service, user: &str) -> Caller {
    let key = Key::generate();
    let email = format!("{user}@comotechnologies.io");
    let signed = service
        .admit(&email, true, &key.thumbprint())
        .await
        .unwrap();
    Caller {
        key,
        token: signed.access_token,
        me: format!("riff://{user}@pangolin/acme/app").parse().unwrap(),
    }
}

/// The session `id` of `person`, on the host `host`, in `acme/app`.
async fn session(url: &str, person: &Caller, id: &str, host: &str) -> Caller {
    let form = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
         &subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token\
         &subject_token={}&session={id}",
        person.token
    );
    let reply = common::refresh(url, &person.key, &form).await;
    assert_eq!(reply.status(), 200);
    let pair: TokenReply = reply.json().await.unwrap();
    let user = person.me.who().user();
    Caller {
        key: person.key.clone(),
        token: pair.access_token,
        me: format!("riff://{user}@{host}/acme/app?session={id}")
            .parse()
            .unwrap(),
    }
}

/// A call of `caller` to `path`, with the ticket of a proof when there is
/// one.
async fn call(
    url: &str,
    caller: &Caller,
    path: &str,
    body: &impl Serialize,
    ticket: Option<&str>,
) -> Replied {
    let mut request = common::post(&format!("{url}{path}"), &caller.key, Some(&caller.token));
    if let Some(ticket) = ticket {
        request = request.header(PROOF_HEADER, ticket);
    }
    let reply = request.json(body).send().await.unwrap();
    Replied {
        status: reply.status().as_u16(),
        code: reply
            .headers()
            .get(REFUSED_HEADER)
            .map(|code| code.to_str().unwrap().to_owned()),
        body: reply.text().await.unwrap(),
    }
}

/// A call of a page, with no token.
async fn page_call(url: &str, path: &str, body: &impl Serialize) -> (u16, String) {
    let reply = common::client()
        .post(format!("{url}{path}"))
        .json(body)
        .send()
        .await
        .unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

/// The origin of a page URL: its text up to the path.
fn origin(page: &str, path: &str) -> String {
    page.split(path).next().unwrap().to_owned()
}

/// `person` adds a key with a new fake authenticator, in the page of the
/// add, with the proof of `ticket` when the add needs one.
async fn add_key(url: &str, person: &Caller, name: &str, ticket: Option<&str>) -> Authenticator {
    let started: KeyStarted = call(
        url,
        person,
        "/v1/device/key",
        &json!({ "name": name }),
        ticket,
    )
    .await
    .json();
    let (status, body) = page_call(
        url,
        "/device/key/options",
        &json!({"ticket": started.ticket}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let options: KeyOptions = serde_json::from_str(&body).unwrap();
    let key = Authenticator::new();
    let reply = key.reply_as(
        &started.ticket,
        &options,
        &origin(&started.url, "/device/key"),
        Shape::Packed,
    );
    let (status, body) = page_call(url, "/device/key/reply", &reply).await;
    assert_eq!(status, 200, "{body}");
    key
}

/// The start of a proof of `action` by `person`.
async fn start(url: &str, person: &Caller, action: &Action) -> ProofStarted {
    call(url, person, "/v1/proof", &json!({ "action": action }), None)
        .await
        .json()
}

/// The page of the proof of `started` sends the assertion of `key` in
/// the shape `shape`: the status and the body.
async fn sign(
    url: &str,
    key: &Authenticator,
    started: &ProofStarted,
    shape: Proof,
) -> (u16, String) {
    let options = options(url, started).await;
    let reply = key.assert_as(
        &started.ticket,
        &options,
        &origin(&started.url, "/proof"),
        shape,
    );
    page_call(url, "/proof/reply", &reply).await
}

async fn options(url: &str, started: &ProofStarted) -> ProofOptions {
    let (status, body) = page_call(url, "/proof/options", &json!({"ticket": started.ticket})).await;
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

/// A good proof of `action` by `person` with `key`: its ticket.
async fn prove(url: &str, person: &Caller, key: &Authenticator, action: &Action) -> String {
    let started = start(url, person, action).await;
    let (status, body) = sign(url, key, &started, Proof::Good).await;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("riff checked your proof"), "{body}");
    started.ticket
}

async fn wait(url: &str, person: &Caller, ticket: &str) -> ProofState {
    call(
        url,
        person,
        "/v1/proof/wait",
        &json!({ "ticket": ticket }),
        None,
    )
    .await
    .json()
}

/// `admin` sets the policy with a proof of `key`.
async fn set_policy(url: &str, admin: &Caller, key: &Authenticator, proof: Policy) -> PolicyReply {
    let ticket = prove(url, admin, key, &Action::Policy { proof }).await;
    call(
        url,
        admin,
        "/v1/policy",
        &json!({ "proof": proof }),
        Some(&ticket),
    )
    .await
    .json()
}

fn approve_body(me: &SessionUri, tag: &str) -> Value {
    json!({ "me": me, "repo": "acme/app", "tag": tag })
}

fn approve_action(tag: &str) -> Action {
    Action::Approve {
        repo: "acme/app".parse().unwrap(),
        tag: tag.into(),
    }
}

/// With the policy `required`, an approval with no proof is refused, and
/// one with a good proof works and pauses the riff. A proof for another
/// action, a used proof, a refused signature and the challenge of
/// another proof are refused. With `off`, the approval needs no proof.
#[tokio::test]
async fn with_required_an_approval_needs_a_good_proof_of_its_own() {
    let (riff, url) = serve(riff_server::proofs::LIFE).await;
    let mike = person(&riff.service, "mike").await;
    let key = add_key(&url, &mike, "yubikey", None).await;

    // Off: no proof asked.
    let off = call(
        &url,
        &mike,
        "/v1/release/approve",
        &approve_body(&mike.me, "v1.0.0"),
        None,
    )
    .await;
    let reply: RiffReply = off.json();
    assert_eq!(reply.proof, Policy::Off);
    assert!(reply.riff.is_some());

    let set = set_policy(&url, &mike, &key, Policy::Required).await;
    assert_eq!(
        set,
        PolicyReply {
            proof: Policy::Required,
            changed: true
        }
    );

    // Required, with no proof: refused, with the way to give one.
    let none = call(
        &url,
        &mike,
        "/v1/release/approve",
        &approve_body(&mike.me, "v1.1.0"),
        None,
    )
    .await;
    none.refused(403, "needs_proof");
    assert!(
        none.body
            .contains("approve the deploy of v1.1.0 of acme/app needs a proof"),
        "{none:?}"
    );
    assert!(none.body.contains("riff device add --key NAME"), "{none:?}");

    // A proof for another action: refused.
    let other = prove(&url, &mike, &key, &approve_action("v9.9.9")).await;
    let wrong = call(
        &url,
        &mike,
        "/v1/release/approve",
        &approve_body(&mike.me, "v1.1.0"),
        Some(&other),
    )
    .await;
    wrong.refused(403, "bad_proof");
    assert!(
        wrong
            .body
            .contains("is for the action \"approve the deploy of v9.9.9"),
        "{wrong:?}"
    );

    // A good proof: the approval works, and the proof is used.
    let good = prove(&url, &mike, &key, &approve_action("v1.1.0")).await;
    assert_eq!(
        wait(&url, &mike, &good).await,
        ProofState::Proved {
            name: "yubikey".into()
        }
    );
    let approved = call(
        &url,
        &mike,
        "/v1/release/approve",
        &approve_body(&mike.me, "v1.1.0"),
        Some(&good),
    )
    .await;
    let reply: RiffReply = approved.json();
    assert_eq!(reply.proof, Policy::Required);
    let again = call(
        &url,
        &mike,
        "/v1/release/approve",
        &approve_body(&mike.me, "v1.1.0"),
        Some(&good),
    )
    .await;
    again.refused(403, "bad_proof");
    assert!(again.body.contains("used this proof already"), "{again:?}");

    // A bad signature: the page gets the reason, and the ticket is good
    // for nothing.
    let started = start(&url, &mike, &approve_action("v1.2.0")).await;
    let (status, body) = sign(&url, &key, &started, Proof::BadSignature).await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body.contains("the signature of the proof does not check"),
        "{body}"
    );
    assert!(matches!(
        wait(&url, &mike, &started.ticket).await,
        ProofState::Failed { .. }
    ));
    let bad = call(
        &url,
        &mike,
        "/v1/release/approve",
        &approve_body(&mike.me, "v1.2.0"),
        Some(&started.ticket),
    )
    .await;
    bad.refused(403, "bad_proof");

    // An old challenge: the signature of the challenge of one proof
    // does not prove another one.
    let first = start(&url, &mike, &approve_action("v1.3.0")).await;
    let first_options = options(&url, &first).await;
    let second = start(&url, &mike, &approve_action("v1.3.0")).await;
    let old = key.assert(
        &second.ticket,
        &first_options,
        &origin(&second.url, "/proof"),
    );
    let (status, body) = page_call(&url, "/proof/reply", &old).await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body.contains("a challenge that riff-server did not give"),
        "{body}"
    );
    let replayed = call(
        &url,
        &mike,
        "/v1/release/approve",
        &approve_body(&mike.me, "v1.3.0"),
        Some(&second.ticket),
    )
    .await;
    replayed.refused(403, "bad_proof");

    // Each other shape of a bad assertion is refused too.
    for (shape, part) in [
        (Proof::OtherOrigin, "not from the page of riff-server"),
        (Proof::OtherParty, "another relying party"),
        (Proof::NoPresence, "no user presence"),
        (Proof::OtherType, "not webauthn.get"),
    ] {
        let started = start(&url, &mike, &Action::Test).await;
        let (status, body) = sign(&url, &key, &started, shape).await;
        assert_eq!(status, 400, "{shape:?}: {body}");
        assert!(body.contains(part), "{shape:?}: {body}");
    }

    // The counter of a key that counts grows: the counter of the last
    // proof again is a copy of the key.
    prove(&url, &mike, &key, &Action::Test).await;
    let started = start(&url, &mike, &Action::Test).await;
    let (status, body) = sign(&url, &key, &started, Proof::SameCounter).await;
    assert_eq!(status, 403, "{body}");
    assert!(
        body.contains("the counter of the key yubikey did not grow"),
        "{body}"
    );

    // The log has a record of each checked proof, with its key and its
    // action.
    let log = riff.log().await;
    assert!(
        log.contains("proved  approve the deploy of v1.1.0 of acme/app with yubikey of mike"),
        "{log}"
    );
    assert!(log.contains("policy_set  proof required"), "{log}");

    // Off again: the approval needs no proof.
    set_policy(&url, &mike, &key, Policy::Off).await;
    let off = call(
        &url,
        &mike,
        "/v1/release/approve",
        &approve_body(&mike.me, "v1.4.0"),
        None,
    )
    .await;
    assert_eq!(off.status, 200, "{off:?}");
}

/// A proof is good for its life from its start: an old proof is refused.
#[tokio::test]
async fn an_old_proof_is_refused() {
    let (riff, url) = serve(Duration::from_millis(300)).await;
    let mike = person(&riff.service, "mike").await;
    let key = add_key(&url, &mike, "yubikey", None).await;
    let ticket = prove(&url, &mike, &key, &Action::PauseRiff).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let old = call(
        &url,
        &mike,
        "/v1/pause",
        &json!({"me": mike.me, "riff": true}),
        Some(&ticket),
    )
    .await;
    old.refused(403, "bad_proof");
    assert!(old.body.contains("the proof is old"), "{old:?}");
    // The page of an old proof gives no options.
    let started = start(&url, &mike, &Action::PauseRiff).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (status, body) =
        page_call(&url, "/proof/options", &json!({"ticket": started.ticket})).await;
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("the proof is old"), "{body}");
}

/// A session token cannot get a challenge or wait for a proof, and the
/// ticket of a proof of its person does not work for it. A person with no
/// key gets the way to add one.
#[tokio::test]
async fn a_session_token_cannot_get_a_challenge() {
    let (riff, url) = serve(riff_server::proofs::LIFE).await;
    let mike = person(&riff.service, "mike").await;
    let lead = session(&url, &mike, "l1", "pangolin").await;

    let refused = call(
        &url,
        &lead,
        "/v1/proof",
        &json!({"action": {"action": "test"}}),
        None,
    )
    .await;
    assert_eq!(refused.status, 403, "{refused:?}");
    assert!(
        refused.body.contains("a session never gives a proof"),
        "{refused:?}"
    );
    let wait = call(&url, &lead, "/v1/proof/wait", &json!({"ticket": "t"}), None).await;
    assert_eq!(wait.status, 403, "{wait:?}");

    let none = call(
        &url,
        &mike,
        "/v1/proof",
        &json!({"action": {"action": "test"}}),
        None,
    )
    .await;
    assert_eq!(none.status, 403, "{none:?}");
    assert!(none.body.contains("riff device add --key NAME"), "{none:?}");

    let key = add_key(&url, &mike, "yubikey", None).await;
    let ticket = prove(&url, &mike, &key, &Action::PauseRiff).await;
    let register = call(&url, &lead, "/v1/register", &json!({"me": lead.me}), None).await;
    assert_eq!(register.status, 200, "{register:?}");
    let pause = call(
        &url,
        &lead,
        "/v1/pause",
        &json!({"me": lead.me, "riff": true}),
        Some(&ticket),
    )
    .await;
    pause.refused(403, "bad_proof");
    assert!(
        pause.body.contains("a session never gives a proof"),
        "{pause:?}"
    );
    let used = call(
        &url,
        &lead,
        "/v1/proof/use",
        &json!({"action": {"action": "pause_riff"}}),
        Some(&ticket),
    )
    .await;
    used.refused(403, "bad_proof");

    // Another person learns nothing of the proof.
    let bob = person(&riff.service, "bob").await;
    let other = call(
        &url,
        &bob,
        "/v1/proof/wait",
        &json!({"ticket": ticket}),
        None,
    )
    .await;
    assert_eq!(other.status, 404, "{other:?}");
}

/// Only an admin with a proof changes the policy: a member with a proof
/// is refused, an admin with no proof or with a proof of another action
/// is refused, and a session cannot. `riff members` and the pauses show
/// the policy.
#[tokio::test]
async fn only_an_admin_with_a_proof_changes_the_policy() {
    let (riff, url) = serve(riff_server::proofs::LIFE).await;
    let mike = person(&riff.service, "mike").await;
    let bob = person(&riff.service, "bob").await;
    let mike_key = add_key(&url, &mike, "yubikey", None).await;
    let bob_key = add_key(&url, &bob, "phone", None).await;
    let body = json!({"proof": "required"});
    let required = Action::Policy {
        proof: Policy::Required,
    };

    let ticket = prove(&url, &bob, &bob_key, &required).await;
    let member = call(&url, &bob, "/v1/policy", &body, Some(&ticket)).await;
    member.refused(403, "not_allowed");
    assert!(member.body.contains("bob is not an admin"), "{member:?}");

    let none = call(&url, &mike, "/v1/policy", &body, None).await;
    none.refused(403, "needs_proof");

    let other = prove(
        &url,
        &mike,
        &mike_key,
        &Action::Policy { proof: Policy::Off },
    )
    .await;
    let wrong = call(&url, &mike, "/v1/policy", &body, Some(&other)).await;
    wrong.refused(403, "bad_proof");

    let lead = session(&url, &mike, "l1", "pangolin").await;
    let session = call(&url, &lead, "/v1/policy", &body, None).await;
    session.refused(403, "not_allowed");

    let members: MembersReply = call(&url, &mike, "/v1/members", &json!({}), None)
        .await
        .json();
    assert_eq!(members.proof, Policy::Off);

    let ticket = prove(&url, &mike, &mike_key, &required).await;
    let set: PolicyReply = call(&url, &mike, "/v1/policy", &body, Some(&ticket))
        .await
        .json();
    assert_eq!(
        set,
        PolicyReply {
            proof: Policy::Required,
            changed: true
        }
    );

    let members: MembersReply = call(&url, &bob, "/v1/members", &json!({}), None)
        .await
        .json();
    assert_eq!(members.proof, Policy::Required);
    let pauses: RiffReply = call(&url, &bob, "/v1/riff", &json!({"me": bob.me}), None)
        .await
        .json();
    assert_eq!(pauses.proof, Policy::Required);

    // The same policy again is no change, but needs a proof all the same.
    let ticket = prove(&url, &mike, &mike_key, &required).await;
    let same: PolicyReply = call(&url, &mike, "/v1/policy", &body, Some(&ticket))
        .await
        .json();
    assert!(!same.changed);
}

/// With `required`, each command of the list needs a proof, and works
/// with one: the pause and the resume of the whole riff, an invite, a
/// change of an admin, a removal, and the add of a second key and the
/// removal of a device. The first key needs no proof, and the pause of a
/// repository is no action of the list.
#[tokio::test]
async fn with_required_each_command_of_the_list_needs_a_proof() {
    let (riff, url) = serve(riff_server::proofs::LIFE).await;
    let mike = person(&riff.service, "mike").await;
    let key = add_key(&url, &mike, "yubikey", None).await;
    set_policy(&url, &mike, &key, Policy::Required).await;

    let calls: [(&str, Value, Action); 6] = [
        (
            "/v1/pause",
            json!({"me": mike.me, "riff": true}),
            Action::PauseRiff,
        ),
        (
            "/v1/resume",
            json!({"me": mike.me, "riff": true}),
            Action::ResumeRiff,
        ),
        (
            "/v1/invite",
            json!({"email": "Bob@comotechnologies.io"}),
            Action::invite("bob@comotechnologies.io"),
        ),
        (
            "/v1/admin",
            json!({"email": "bob@comotechnologies.io", "admin": true}),
            Action::admin("bob@comotechnologies.io", true),
        ),
        (
            "/v1/admin",
            json!({"email": "bob@comotechnologies.io", "admin": false}),
            Action::admin("bob@comotechnologies.io", false),
        ),
        (
            "/v1/remove",
            json!({"email": "bob@comotechnologies.io"}),
            Action::remove("bob@comotechnologies.io"),
        ),
    ];
    for (path, body, action) in calls {
        let none = call(&url, &mike, path, &body, None).await;
        none.refused(403, "needs_proof");
        let ticket = prove(&url, &mike, &key, &action).await;
        let done = call(&url, &mike, path, &body, Some(&ticket)).await;
        assert_eq!(done.status, 200, "{path}: {done:?}");
    }

    // The pause of a repository needs no proof.
    let repo = call(&url, &mike, "/v1/pause", &json!({"me": mike.me}), None).await;
    assert_eq!(repo.status, 200, "{repo:?}");

    // A second key needs a proof; then the removal of a device does.
    let none = call(
        &url,
        &mike,
        "/v1/device/key",
        &json!({"name": "phone"}),
        None,
    )
    .await;
    none.refused(403, "needs_proof");
    let ticket = prove(
        &url,
        &mike,
        &key,
        &Action::AddKey {
            name: "phone".into(),
        },
    )
    .await;
    add_key(&url, &mike, "phone", Some(&ticket)).await;
    let none = call(
        &url,
        &mike,
        "/v1/device/remove",
        &json!({"name": "phone"}),
        None,
    )
    .await;
    none.refused(403, "needs_proof");
    let ticket = prove(
        &url,
        &mike,
        &key,
        &Action::RemoveDevice {
            name: "phone".into(),
        },
    )
    .await;
    let removed = call(
        &url,
        &mike,
        "/v1/device/remove",
        &json!({"name": "phone"}),
        Some(&ticket),
    )
    .await;
    assert_eq!(removed.status, 200, "{removed:?}");

    // The first key of a person needs no proof.
    let cy = person(&riff.service, "cy").await;
    add_key(&url, &cy, "yubikey", None).await;
}

/// With `off`, an action with a proof is marked in the log, and the
/// proof is used. A step that riff-server does not do itself gets the
/// same rule through `/v1/proof/use`.
#[tokio::test]
async fn with_off_an_action_with_a_proof_is_marked_and_a_step_gets_the_same_rule() {
    let (riff, url) = serve(riff_server::proofs::LIFE).await;
    let mike = person(&riff.service, "mike").await;
    let key = add_key(&url, &mike, "yubikey", None).await;
    let answer = Action::Answer {
        repo: "acme/app".parse().unwrap(),
        number: 12,
    };
    let body = json!({ "action": answer });
    let used: Value = call(&url, &mike, "/v1/proof/use", &body, None).await.json();
    assert_eq!(used["proved"], false);
    let ticket = prove(&url, &mike, &key, &answer).await;
    let used: Value = call(&url, &mike, "/v1/proof/use", &body, Some(&ticket))
        .await
        .json();
    assert_eq!(used["proved"], true);
    let log = riff.log().await;
    assert!(
        log.contains(
            "proved  answer the questions of the issue #12 of acme/app with yubikey of mike"
        ),
        "{log}"
    );

    set_policy(&url, &mike, &key, Policy::Required).await;
    call(&url, &mike, "/v1/proof/use", &body, None)
        .await
        .refused(403, "needs_proof");
    let ticket = prove(&url, &mike, &key, &answer).await;
    let used: Value = call(&url, &mike, "/v1/proof/use", &body, Some(&ticket))
        .await
        .json();
    assert_eq!(used["proved"], true);
    // A second try of the same call gets the proof again; another call
    // does not.
    let again = common::post(&format!("{url}/v1/proof/use"), &mike.key, Some(&mike.token))
        .header(PROOF_HEADER, &ticket)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 403);
}

/// The unread messages of the threads of `caller`.
async fn unread(url: &str, caller: &Caller) -> usize {
    let threads: ThreadsReply = call(url, caller, "/v1/threads", &json!({"me": caller.me}), None)
        .await
        .json();
    threads.threads.iter().map(|t| t.unread).sum()
}

/// With `required`, `riff daemon end NAME --host H` with no proof is
/// refused by riff-server before it reaches the daemon of H: the daemon
/// gets no message. With a good proof, the daemon gets the request, and
/// it ends the session, as the riff daemon does: it frees the claim and
/// ends the session of the worker.
#[tokio::test]
async fn with_required_an_end_on_another_host_needs_a_proof_before_it_reaches_the_daemon() {
    let (riff, url) = serve(riff_server::proofs::LIFE).await;
    let mike = person(&riff.service, "mike").await;
    let key = add_key(&url, &mike, "yubikey", None).await;
    let resume = prove(&url, &mike, &key, &Action::ResumeRiff).await;
    call(
        &url,
        &mike,
        "/v1/resume",
        &json!({"me": mike.me, "riff": true}),
        Some(&resume),
    )
    .await;
    set_policy(&url, &mike, &key, Policy::Required).await;

    // The daemon of the host thelio, and a worker of it with a claim.
    let daemon = session(&url, &mike, "d1", "thelio").await;
    let worker = session(&url, &mike, "w1", "thelio").await;
    for caller in [&daemon, &worker] {
        let register = call(
            &url,
            caller,
            "/v1/register",
            &json!({"me": caller.me, "worker": caller.me.who().session() == Some("w1")}),
            None,
        )
        .await;
        assert_eq!(register.status, 200, "{register:?}");
    }
    let claim = call(
        &url,
        &worker,
        "/v1/claim",
        &json!({"me": worker.me, "thread": "acme/app", "item": "issue-12"}),
        None,
    )
    .await;
    assert_eq!(claim.status, 200, "{claim:?}");
    let before = unread(&url, &daemon).await;

    // The request of `riff daemon end NAME --host thelio`: a signed
    // direct message to the daemon.
    let name = "riff-issue-12-1a2b3c4d";
    let request = |mike: &Caller| {
        let mut post = Post::new(
            &mike.me,
            None,
            vec![Selector::session("d1")],
            &format!("daemon end {name}"),
        );
        post.sign(&mike.key, now_ms());
        post
    };
    let none = call(&url, &mike, "/v1/post", &request(&mike), None).await;
    none.refused(403, "needs_proof");
    assert!(
        none.body.contains(&format!(
            "end the session {name} on the host thelio needs a proof"
        )),
        "{none:?}"
    );
    assert_eq!(
        unread(&url, &daemon).await,
        before,
        "the daemon got no request"
    );

    // A proof for the end of a session on another host is no proof for
    // this one.
    let elsewhere = Action::End {
        host: "kadomony".into(),
        name: name.into(),
    };
    let ticket = prove(&url, &mike, &key, &elsewhere).await;
    call(&url, &mike, "/v1/post", &request(&mike), Some(&ticket))
        .await
        .refused(403, "bad_proof");

    // A good proof: the daemon gets the request.
    let end = Action::End {
        host: "thelio".into(),
        name: name.into(),
    };
    let ticket = prove(&url, &mike, &key, &end).await;
    let posted = call(&url, &mike, "/v1/post", &request(&mike), Some(&ticket)).await;
    assert_eq!(posted.status, 200, "{posted:?}");
    assert_eq!(unread(&url, &daemon).await, before + 1);
    let thread = ThreadName::direct(mike.me.who(), &Who::new("mike", Some("d1")).unwrap());
    let read: Value = call(
        &url,
        &daemon,
        "/v1/read",
        &json!({"me": daemon.me, "thread": thread}),
        None,
    )
    .await
    .json();
    let bodies: Vec<&str> = read["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["body"].as_str())
        .collect();
    assert!(
        bodies.contains(&format!("daemon end {name}").as_str()),
        "{read}"
    );

    // The daemon ends the session: it frees the item, and ends the
    // session of the worker.
    let release = call(
        &url,
        &worker,
        "/v1/release",
        &json!({"me": worker.me, "thread": "acme/app", "item": "issue-12"}),
        None,
    )
    .await;
    assert_eq!(release.status, 200, "{release:?}");
    let ended = call(&url, &worker, "/v1/end", &json!({"me": worker.me}), None).await;
    assert_eq!(ended.status, 200, "{ended:?}");
    let who: WhoReply = call(&url, &mike, "/v1/who", &json!({"me": mike.me}), None)
        .await
        .json();
    let worker_live = who
        .sessions
        .iter()
        .any(|s| s.uri.who().session() == Some("w1") && s.live);
    assert!(!worker_live, "the worker ended: {:?}", who.sessions);
    assert!(
        !who.sessions
            .iter()
            .any(|s| s.uri.claims().contains(&"issue-12".to_owned()))
    );

    // A step that cannot lose work needs no proof.
    let mut status = Post::new(
        &mike.me,
        None,
        vec![Selector::session("d1")],
        "daemon status",
    );
    status.sign(&mike.key, now_ms());
    let posted = call(&url, &mike, "/v1/post", &status, None).await;
    assert_eq!(posted.status, 200, "{posted:?}");
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
