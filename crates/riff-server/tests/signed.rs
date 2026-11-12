//! Signed messages (R195-R201), over HTTP: the check at the server, the
//! keys for the reader, a message that changed in storage, the kind
//! `riff` that only riff-server itself posts, and the lines of riff-bot
//! that it writes from a fact.

use crate::common;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use riff_core::dpop::Key;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{
    BotFact, BotPost, Invite, Kind, PassOwner, Post, Posted, ReadReply, TokenReply,
};
use riff_server::Service;
use riff_server::auth::Config;
use riff_state::state::threads::KIND_RIFF;
use riff_state::store::{Memory, Store};
use serde::Serialize;
use serde_json::Value;

const A: &str = "riff://mike@pangolin/como-technologies/riff?session=a";
const LEAD: &str = "riff://mike@pangolin/como-technologies/riff?session=lead";
const BRETT: &str = "riff://brett@heron/como-technologies/riff?session=b";
const MIKE: &str = "riff://mike@pangolin";
const REPO: &str = "como-technologies/riff";
const CHAT: &str = "chat";

fn now_ms() -> u64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    u64::try_from(since.as_millis()).unwrap()
}

/// A caller that is signed in: its device key, its access token, the
/// refresh token of its person pair, and its URI. A URI with a session
/// ID gets a session token (R19).
struct Caller {
    key: Key,
    token: String,
    refresh: String,
    me: SessionUri,
}

impl Caller {
    async fn new(service: &Service, base: &str, me: &str) -> Caller {
        let me: SessionUri = me.parse().unwrap();
        let key = Key::generate();
        // The sign-in of a person, as the provider does it: the log
        // knows the person, so the sign-in stays after a restart.
        let email = format!("{}@comotechnologies.io", me.who().user());
        let person = service
            .admit(&email, true, &key.thumbprint())
            .await
            .unwrap();
        let caller = Caller {
            key,
            token: person.access_token,
            refresh: person.refresh_token,
            me,
        };
        // The refresh makes the server save the sign-in, as after a real
        // sign-in. The swap for a session token is a change of the token
        // store too (01M52JYGSNTFPQ0ZP50AD2BG89).
        caller.refreshed(base).await
    }

    /// Gets a new access token, as `riff` does before its token expires:
    /// it refreshes the person pair, and a session swaps the new person
    /// token for a session token (01M3WFVADCDZM8XX590KAEMEYG).
    async fn refreshed(mut self, base: &str) -> Caller {
        let form = format!("grant_type=refresh_token&refresh_token={}", self.refresh);
        let reply = common::refresh(base, &self.key, &form).await;
        assert_eq!(reply.status(), 200);
        let person: TokenReply = reply.json().await.unwrap();
        self.token = person.access_token;
        self.refresh = person.refresh_token;
        let Some(session) = self.me.who().session() else {
            return self;
        };
        let form = format!(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
             &subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token\
             &subject_token={}&session={session}",
            self.token
        );
        let reply = common::refresh(base, &self.key, &form).await;
        assert_eq!(reply.status(), 200);
        let session: TokenReply = reply.json().await.unwrap();
        self.token = session.access_token;
        self
    }

    async fn call(&self, base: &str, op: &str, body: &impl Serialize) -> reqwest::Response {
        common::post(&format!("{base}/v1/{op}"), &self.key, Some(&self.token))
            .json(body)
            .send()
            .await
            .unwrap()
    }

    /// A post to `thread` with no signature.
    fn unsigned(&self, thread: &str, body: &str) -> Post {
        Post::new(&self.me, Some(thread.parse().unwrap()), vec![], body)
    }

    /// A post to `thread`, signed now with the key of the caller.
    fn post(&self, thread: &str, body: &str) -> Post {
        let mut post = self.unsigned(thread, body);
        post.sign(&self.key, now_ms());
        post
    }

    async fn read(&self, base: &str, thread: &str) -> ReadReply {
        let request = serde_json::json!({ "me": self.me, "thread": thread, "all": true });
        let reply = self.call(base, "read", &request).await;
        assert_eq!(reply.status(), 200);
        reply.json().await.unwrap()
    }
}

/// Starts a server that needs sign-in, and loads its state from `store`.
async fn start_on(store: Arc<dyn Store>) -> (Service, String) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        require_sign_in: true,
        lease: common::LEASE,
        save_every: common::SAVE_EVERY,
        ..Config::new(&url)
    };
    let service = Service::load(config, store).await.unwrap();
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// True when each message of the reply is verified.
fn verified(reply: &ReadReply, thread: &str) -> Vec<bool> {
    let thread: ThreadName = thread.parse().unwrap();
    reply
        .messages
        .iter()
        .map(|m| m.verified(&thread, &reply.keys))
        .collect()
}

#[tokio::test]
async fn a_signed_message_shows_as_verified_to_the_reader() {
    let (service, base) = common::start(true, &[]).await;
    let a = Caller::new(&service, &base, A).await;
    let person = Caller::new(&service, &base, MIKE).await;
    let brett = Caller::new(&service, &base, BRETT).await;

    assert_eq!(
        a.call(&base, "post", &a.post(REPO, "ready")).await.status(),
        200
    );
    // A post from the command line carries the key of the person (R195).
    let from_person = person.post(REPO, "me too");
    assert_eq!(person.call(&base, "post", &from_person).await.status(), 200);

    let reply = brett.read(&base, REPO).await;
    assert_eq!(verified(&reply, REPO), [true, true]);
    assert!(reply.messages.iter().all(|m| m.sig.is_some()));
    let mut keys = vec![a.key.thumbprint(), person.key.thumbprint()];
    keys.sort();
    assert_eq!(reply.keys["mike"], keys);
}

#[tokio::test]
async fn the_server_refuses_a_post_that_its_caller_did_not_sign() {
    let (service, base) = common::start(true, &[]).await;
    let a = Caller::new(&service, &base, A).await;
    let refused = |post: Post| {
        let a = &a;
        let base = &base;
        async move {
            let reply = a.call(base, "post", &post).await;
            (reply.status().as_u16(), reply.text().await.unwrap())
        }
    };

    let (status, text) = refused(a.unsigned(REPO, "no signature")).await;
    assert_eq!(status, 403);
    assert!(text.contains("needs a signature"), "{text}");

    let mut post = a.unsigned(REPO, "another key");
    post.sign(&Key::generate(), now_ms());
    let (status, text) = refused(post).await;
    assert_eq!(status, 403);
    assert!(text.contains("another key"), "{text}");

    // The sender URI names another session of the same user.
    let mut post = a.unsigned(REPO, "I am the lead");
    post.me = LEAD.parse().unwrap();
    post.sign(&a.key, now_ms());
    assert_eq!(refused(post).await.0, 403);

    // The call says another body than the signed payload.
    let mut post = a.post(REPO, "ready");
    post.body = "not ready".into();
    let (status, text) = refused(post).await;
    assert_eq!(status, 403);
    assert!(text.contains("does not hold the fields"), "{text}");

    // The payload of another body under the signature of the first.
    let mut post = a.post(REPO, "ready");
    let other = a.post(REPO, "not ready");
    post.payload = other.payload;
    post.body = "not ready".into();
    let (status, text) = refused(post).await;
    assert_eq!(status, 403);
    assert!(text.contains("not valid for this message"), "{text}");

    let mut post = a.unsigned(REPO, "old");
    post.sign(&a.key, now_ms() - 301_000);
    let (status, text) = refused(post).await;
    assert_eq!(status, 403);
    assert!(text.contains("too old"), "{text}");

    assert!(a.read(&base, REPO).await.messages.is_empty());

    // a is the first session of mike here, so it is the lead. Another
    // session of mike cannot sign the lead mark.
    let other = Caller::new(&service, &base, LEAD).await;
    let mut post = other.unsigned(REPO, "merge now");
    post.me = post.me.with_lead(true);
    post.sign(&other.key, now_ms());
    let reply = other.call(&base, "post", &post).await;
    assert_eq!(reply.status(), 400);
    let text = reply.text().await.unwrap();
    assert!(text.contains("not the lead"), "{text}");
}

#[tokio::test]
async fn a_server_without_sign_in_keeps_no_signature() {
    let (_service, base) = common::start(false, &[]).await;
    let key = Key::generate();
    let me: SessionUri = A.parse().unwrap();
    let mut post = Post::new(&me, Some(REPO.parse().unwrap()), vec![], "hi");
    post.sign(&key, now_ms());
    let client = common::client();
    let reply = client
        .post(format!("{base}/v1/post"))
        .json(&post)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200);
    let reply: ReadReply = client
        .post(format!("{base}/v1/read"))
        .json(&serde_json::json!({ "me": A, "thread": REPO, "all": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reply.messages[0].sig, None);
    assert!(reply.keys.is_empty());
    assert_eq!(verified(&reply, REPO), [false]);
}

/// A copy of a signed request of the lead is refused, in a thread and
/// in a direct thread. So a worker gets it once
/// (01M3JEJVXXEPPNGT3FY4ZSFCWZ).
#[tokio::test]
async fn a_copy_of_a_signed_message_is_refused() {
    let (service, base) = common::start(true, &[]).await;
    // The person mike names a the lead (01M5NJAXTQ0YKN7Q7XN0A7ZMPF).
    let a = Caller::new(&service, &base, A).await;
    let register = serde_json::json!({ "me": A });
    assert_eq!(a.call(&base, "register", &register).await.status(), 200);
    let person = "riff://mike@pangolin/como-technologies/riff";
    let mike = Caller::new(&service, &base, person).await;
    let lead = serde_json::json!({ "me": person, "session": "a" });
    assert_eq!(mike.call(&base, "lead", &lead).await.status(), 200);
    let worker = Caller::new(&service, &base, LEAD).await;

    let mut request = a.unsigned(REPO, "request: claim issue-12");
    request.me = request.me.with_lead(true);
    request.sign(&a.key, now_ms());
    assert_eq!(a.call(&base, "post", &request).await.status(), 200);
    let copy = a.call(&base, "post", &request).await;
    assert_eq!(copy.status(), 400);
    let text = copy.text().await.unwrap();
    assert!(text.contains("a copy of message 1"), "{text}");
    assert_eq!(worker.read(&base, REPO).await.messages.len(), 1);

    let to = format!("session={}", worker.me.who().session().unwrap());
    let mut direct = Post::new(
        &a.me.clone().with_lead(true),
        None,
        vec![to.parse().unwrap()],
        "request: claim issue-7",
    );
    direct.sign(&a.key, now_ms());
    assert_eq!(a.call(&base, "post", &direct).await.status(), 200);
    assert_eq!(a.call(&base, "post", &direct).await.status(), 400);
}

/// The waves of two hosts of mike post the same news with the same key.
/// The second post is no error: the server drops it, and answers with
/// the first message, and it wakes no one. A key is the key of a user
/// (01M5TKG2K1AYK1KE7C2AA2W9DS).
#[tokio::test]
async fn a_post_with_the_key_of_a_kept_message_is_dropped_and_the_reply_names_the_first() {
    let (service, base) = common::start(true, &[]).await;
    let pangolin = Caller::new(&service, &base, A).await;
    let thelio = Caller::new(
        &service,
        &base,
        "riff://mike@thelio/como-technologies/riff?session=t",
    )
    .await;
    let brett = Caller::new(&service, &base, BRETT).await;
    for caller in [&pangolin, &thelio, &brett] {
        let register = serde_json::json!({ "me": caller.me });
        assert_eq!(
            caller.call(&base, "register", &register).await.status(),
            200
        );
    }
    let keyed = |caller: &Caller, key: &str| {
        let mut post = caller.unsigned(REPO, "Pull request #40 has a conflict.");
        post.key = Some(key.into());
        post.to = vec!["user=brett".parse().unwrap()];
        post.sign(&caller.key, now_ms());
        post
    };
    let mut told = Vec::new();
    for (caller, key) in [
        (&pangolin, "pull-40-1a2b3c4d-conflict"),
        (&thelio, "pull-40-1a2b3c4d-conflict"),
        (&thelio, "pull-40-5e6f7a8b-conflict"),
        (&brett, "pull-40-1a2b3c4d-conflict"),
    ] {
        let reply = caller.call(&base, "post", &keyed(caller, key)).await;
        assert_eq!(reply.status(), 200);
        told.push(reply.json::<Posted>().await.unwrap());
    }

    // The second post names the first message, and wakes no session.
    assert_eq!(told.iter().map(|p| p.seq).collect::<Vec<_>>(), [1, 1, 2, 3]);
    assert_eq!(told[0].woken.len(), 1);
    assert!(told[1].woken.is_empty(), "{:?}", told[1].woken);
    // The key of mike is no key of brett.
    assert_eq!(told[3].seq, 3);
    assert_eq!(brett.read(&base, REPO).await.messages.len(), 3);
}

#[tokio::test]
async fn a_message_changed_in_storage_is_not_verified() {
    let store = Memory::default();
    let (old, base) = start_on(Arc::new(store.clone())).await;
    let a = Caller::new(&old, &base, A).await;
    let brett = Caller::new(&old, &base, BRETT).await;
    for body in [
        "claim issue-12",
        "claim issue-13",
        "claim issue-14",
        "merge",
    ] {
        assert_eq!(
            a.call(&base, "post", &a.post(REPO, body)).await.status(),
            200
        );
    }
    assert_eq!(verified(&brett.read(&base, REPO).await, REPO), [true; 4]);
    old.shutdown().await.unwrap();

    // Someone with access to the storage changes three messages: a new
    // body, a sender that claims to be the lead, and a lead mark that
    // the sender did not sign. The last checkpoint of the stop holds the
    // messages too: they remove it, so that the next start replays the
    // log.
    for name in store.list("checkpoint/").await.unwrap() {
        store.delete(&name).await.unwrap();
    }
    let mut seen = 0;
    for name in store.list("log/").await.unwrap() {
        let object = store.load(&name).await.unwrap().unwrap();
        let mut lines = Vec::new();
        for line in String::from_utf8(object.bytes).unwrap().lines() {
            let mut record: Value = serde_json::from_str(line).unwrap();
            if let Some(posted) = record.pointer_mut("/change/posted")
                && posted["thread"] == REPO
            {
                let message = &mut posted["message"];
                match seen {
                    1 => message["body"] = "claim issue-99".into(),
                    2 => message["from"] = format!("{LEAD}&lead=true").into(),
                    3 => message["from"] = format!("{A}&lead=true").into(),
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
    assert_eq!(seen, 4);

    let (_new, base) = start_on(Arc::new(store)).await;
    let brett = brett.refreshed(&base).await;
    let reply = brett.read(&base, REPO).await;
    assert_eq!(reply.messages[1].body, "claim issue-99");
    assert!(reply.messages[2].from.lead());
    assert!(reply.messages[3].from.lead());
    assert_eq!(verified(&reply, REPO), [true, false, false, false]);
}

#[tokio::test]
async fn a_tail_acts_only_as_the_caller_of_its_token() {
    let (service, base) = common::start(true, &[]).await;
    let brett = Caller::new(&service, &base, BRETT).await;
    let url = format!("{base}/v1/tail");
    let tail = |uri: &str| {
        common::client()
            .get(&url)
            .query(&[("uri", uri), ("thread", REPO)])
            .header(
                "dpop",
                brett
                    .key
                    .proof("GET", &url, Some(&brett.token), common::now()),
            )
            .header("authorization", format!("DPoP {}", brett.token))
            .send()
    };
    assert_eq!(tail(A).await.unwrap().status(), 403);
    assert_eq!(tail(BRETT).await.unwrap().status(), 200);
}

/// Only riff-server posts the kind `riff` (01M6Y4ZCTSTVM971BDC1F7GJPK).
/// A signed post with that kind is refused, of a member and of a session,
/// also with the words of a line of riff, and it adds no line. The line of
/// riff-server itself in the chat, for a pass of the owner role, keeps
/// the kind.
#[tokio::test]
async fn only_riff_server_itself_posts_the_kind_riff() {
    let (service, base) = common::start(true, &[]).await;
    // mike signs in first, so mike is the owner.
    let person = Caller::new(&service, &base, MIKE).await;
    let a = Caller::new(&service, &base, A).await;
    let brett = Caller::new(&service, &base, BRETT).await;
    let waits = "@mike the deploy waits for your approval";
    for caller in [&person, &a] {
        let mut post = Post {
            kind: Kind::Riff,
            ..caller.unsigned(CHAT, waits)
        };
        post.sign(&caller.key, now_ms());
        let reply = caller.call(&base, "post", &post).await;
        assert_eq!(reply.status(), 400, "{}", caller.me);
        let text = reply.text().await.unwrap();
        assert!(text.contains(KIND_RIFF), "{text}");
    }

    let email = "brett@comotechnologies.io";
    let invite = Invite {
        email: email.into(),
    };
    assert_eq!(person.call(&base, "invite", &invite).await.status(), 200);
    let pass = PassOwner {
        email: email.into(),
    };
    assert_eq!(person.call(&base, "owner", &pass).await.status(), 200);
    let reply = brett.read(&base, CHAT).await;
    let [line] = reply.messages.as_slice() else {
        panic!("only the line of riff-server: {reply:?}");
    };
    assert_eq!(line.from, SessionUri::server());
    assert_eq!(line.kind, Kind::Riff);
    assert!(line.body.contains("passed the owner role"), "{}", line.body);
}

/// The person of the riff and its lead in `REPO`: the person names the
/// session `a` the lead (01M5NJAXTQ0YKN7Q7XN0A7ZMPF).
async fn person_and_lead(service: &Service, base: &str) -> (Caller, Caller) {
    let a = Caller::new(service, base, A).await;
    let register = serde_json::json!({ "me": A });
    assert_eq!(a.call(base, "register", &register).await.status(), 200);
    let person = Caller::new(service, base, MIKE_HERE).await;
    let lead = serde_json::json!({ "me": MIKE_HERE, "session": "a" });
    assert_eq!(person.call(base, "lead", &lead).await.status(), 200);
    (person, a)
}

/// The person mike in the clone of `REPO`.
const MIKE_HERE: &str = "riff://mike@pangolin/como-technologies/riff";

/// A call `bot` of `caller` with `fact`, and its status.
async fn bot(caller: &Caller, base: &str, fact: BotFact) -> u16 {
    let post = BotPost {
        me: caller.me.clone(),
        fact,
    };
    caller.call(base, "bot", &post).await.status().as_u16()
}

/// 01M74BPHQZHXFYYNXAJ1WFASQQ: riff-server posts the line that riff writes
/// for a person, as `<riff-bot>`: the ask of a release and the line "riff
/// started the lead ...". It writes the text from the fact, the owner, the
/// place of the caller and the holds of the version. The ask names the
/// owner, and wakes the owner by the name; the line of the daemon names no
/// one.
#[tokio::test]
async fn riff_server_posts_the_line_of_riff_bot_for_a_fact() {
    let (service, base) = common::start(true, &[]).await;
    // mike signs in first, so mike is the owner.
    let (person, lead) = person_and_lead(&service, &base).await;
    let mut hold = lead.unsigned(
        "release",
        &riff_core::hold::hold_body(REPO, "v3.3.0", "A demo runs."),
    );
    hold.me = hold.me.with_lead(true);
    hold.sign(&lead.key, now_ms());
    assert_eq!(lead.call(&base, "post", &hold).await.status(), 200);

    let waits = BotFact::ReleaseWaits {
        tag: "v3.3.0".into(),
        run: 9,
        reminder: false,
    };
    assert_eq!(bot(&person, &base, waits).await, 200);
    assert_eq!(bot(&person, &base, BotFact::LeadStarted).await, 200);
    let cut = BotFact::ReleasePublished {
        tag: "v3.3.0".into(),
        run: None,
    };
    assert_eq!(bot(&lead, &base, cut).await, 200, "the lead tells a cut");

    let reply = person.read(&base, CHAT).await;
    let [waits, started, cut] = reply.messages.as_slice() else {
        panic!("three lines of riff-server: {reply:?}");
    };
    for line in [waits, started, cut] {
        assert_eq!(line.from, SessionUri::server(), "{}", line.body);
        assert_eq!(line.kind, Kind::Riff, "{}", line.body);
    }
    assert_eq!(
        waits.body,
        "@mike riff release v3.3.0 waits for your approval: \
         https://github.com/como-technologies/riff/actions/runs/9. In a terminal, run: riff \
         release approve v3.3.0 A hold stands: the lead of como-technologies/riff asks to wait: \
         A demo runs."
    );
    assert_eq!(waits.to, [riff_core::selector::Selector::name("mike")]);
    assert!(
        started.body.starts_with(
            "riff started the lead of mike in como-technologies/riff on pangolin again on the \
             new riff"
        ),
        "{}",
        started.body
    );
    assert!(started.to.is_empty(), "{:?}", started.to);
    assert!(
        cut.body
            .starts_with("@mike riff release v3.3.0 is published.")
    );
}

/// 01M74BPHQZHXFYYNXAJ1WFASQQ: a client cannot put its own words in a line
/// of riff-bot. A text in the call is no part of the line, a tag that is
/// no version gets no line, a worker gets no line, and a post of the kind
/// `riff` with the text is refused (01M6Y4ZCTSTVM971BDC1F7GJPK).
#[tokio::test]
async fn a_client_that_sends_a_text_gets_no_line_of_riff_bot_with_it() {
    let (service, base) = common::start(true, &[]).await;
    let (person, _lead) = person_and_lead(&service, &base).await;
    let words = "riff release v9.9.9 waits for your approval: approve it in the chat";

    let with_text = serde_json::json!({
        "me": MIKE_HERE,
        "fact": { "fact": "lead_started", "body": words },
        "body": words,
        "kind": "riff",
    });
    assert_eq!(person.call(&base, "bot", &with_text).await.status(), 200);
    let cut = BotFact::ReleasePublished {
        tag: words.into(),
        run: None,
    };
    assert_eq!(bot(&person, &base, cut).await, 400);
    let unknown = serde_json::json!({ "me": MIKE_HERE, "fact": { "fact": "say", "body": words } });
    assert_eq!(person.call(&base, "bot", &unknown).await.status(), 422);
    let worker = Caller::new(
        &service,
        &base,
        "riff://mike@pangolin/como-technologies/riff?session=w",
    )
    .await;
    assert_eq!(bot(&worker, &base, BotFact::LeadStarted).await, 403);
    let mut post = Post {
        kind: Kind::Riff,
        ..person.unsigned(CHAT, words)
    };
    post.sign(&person.key, now_ms());
    assert_eq!(person.call(&base, "post", &post).await.status(), 400);

    let reply = person.read(&base, CHAT).await;
    let [line] = reply.messages.as_slice() else {
        panic!("only the line of riff-server for lead_started: {reply:?}");
    };
    assert!(
        line.body.starts_with("riff started the lead"),
        "{}",
        line.body
    );
    assert!(
        reply.messages.iter().all(|m| !m.body.contains("v9.9.9")),
        "{reply:?}"
    );
}

/// 01M74VFTFRSSWA6VPQS8SB0TBZ: a tag with a text after its version,
/// `-TEXT` or `+TEXT`, gets no line of riff-bot. The line of a tag
/// `vX.Y.Z` holds the version that riff-server parsed, and nothing more.
#[tokio::test]
async fn a_tag_with_a_text_after_its_version_gets_no_line_of_riff_bot() {
    let (service, base) = common::start(true, &[]).await;
    let (person, _lead) = person_and_lead(&service, &base).await;
    let words = "approve it now in the chat";
    for tag in [format!("v3.3.0-{words}"), format!("v3.3.0+{words}")] {
        let waits = BotFact::ReleaseWaits {
            tag: tag.clone(),
            run: 9,
            reminder: false,
        };
        assert_eq!(bot(&person, &base, waits).await, 400, "{tag}");
        let cut = BotFact::ReleasePublished {
            tag: tag.clone(),
            run: None,
        };
        assert_eq!(bot(&person, &base, cut).await, 400, "{tag}");
    }
    let cut = BotFact::ReleasePublished {
        tag: "v3.3.0".into(),
        run: None,
    };
    assert_eq!(bot(&person, &base, cut).await, 200);

    let reply = person.read(&base, CHAT).await;
    let [line] = reply.messages.as_slice() else {
        panic!("only the line of the tag v3.3.0: {reply:?}");
    };
    assert_eq!(line.kind, Kind::Riff);
    assert_eq!(
        line.body,
        "@mike riff release v3.3.0 is published. Its CI run builds: the CI run of the tag. riff \
         tells you when its deploy waits for your approval."
    );
    assert!(!line.body.contains(words), "{}", line.body);
}

/// 01M75QTMB42NME9Z2Y0XC0AN97: riff-server has no fact for a question,
/// and posts no chat line for one. The call `bot` has no key: a key in
/// the call drops no line.
#[tokio::test]
async fn riff_server_posts_no_line_for_a_question() {
    let (service, base) = common::start(true, &[]).await;
    let (person, _lead) = person_and_lead(&service, &base).await;
    let question = serde_json::json!({
        "me": MIKE_HERE,
        "fact": { "fact": "question", "issue": 12 },
        "key": "question-12",
    });
    assert_eq!(person.call(&base, "bot", &question).await.status(), 422);
    let started = serde_json::json!({
        "me": MIKE_HERE,
        "fact": { "fact": "lead_started" },
        "key": "started",
    });
    for _ in 0..2 {
        assert_eq!(person.call(&base, "bot", &started).await.status(), 200);
    }
    let reply = person.read(&base, CHAT).await;
    let bodies: Vec<&str> = reply.messages.iter().map(|m| m.body.as_str()).collect();
    assert_eq!(bodies.len(), 2, "{bodies:?}");
    assert!(
        bodies
            .iter()
            .all(|b| b.starts_with("riff started the lead")),
        "{bodies:?}"
    );
}
