//! Signed messages (R195-R201), over HTTP: the check at the server, the
//! keys for the reader, and a message that changed in storage.

mod common;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use riff_core::dpop::Key;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Post, ReadReply, TokenReply};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::store::{Memory, Store};
use serde::Serialize;
use serde_json::Value;

const A: &str = "riff://mike@pangolin/como-technologies/riff?session=a";
const LEAD: &str = "riff://mike@pangolin/como-technologies/riff?session=lead";
const BRETT: &str = "riff://brett@heron/como-technologies/riff?session=b";
const MIKE: &str = "riff://mike@pangolin";
const REPO: &str = "como-technologies/riff";

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
        // sign-in. A swap for a session token saves nothing
        // (01M3WFVAB44T8EP4QZD4KS7DRF).
        caller.refreshed(base).await
    }

    /// Gets a new access token, as `riff` does after a restart of the
    /// server: it refreshes the person pair, and a session swaps the new
    /// person token for a session token (01M3WFVADCDZM8XX590KAEMEYG).
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
    // a is the first session of mike that registers here, so it is the
    // lead.
    let a = Caller::new(&service, &base, A).await;
    let register = serde_json::json!({ "me": A });
    assert_eq!(a.call(&base, "register", &register).await.status(), 200);
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
    old.save().await.unwrap();

    // Someone with access to the storage changes three messages: a new
    // body, a sender that claims to be the lead, and a lead mark that
    // the sender did not sign.
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
