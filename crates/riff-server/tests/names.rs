//! The names of the chat over HTTP (01M6XZ27XV8RAC8B72DCM937ST,
//! 01M6XZ280AMTAMPD6R4212YC2T, 01M77XW0X8Z4M3EXGRDEXV6S2B): the query
//! `names`, and the resolve of each name of a post, on a real server with
//! no sign-in.

use crate::common;

use riff_core::wire::{NameKind, NamesReply, Posted, ReadReply};
use serde_json::{Value, json};

/// The terminal of mike: the sender of the chat lines.
const MIKE: &str = "riff://mike@thelio";
/// The terminal of brett.
const BRETT: &str = "riff://brett@heron";

async fn ok<T: serde::de::DeserializeOwned>(base: &str, path: &str, body: Value) -> T {
    let reply = common::client()
        .post(format!("{base}{path}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200, "{path}");
    reply.json().await.unwrap()
}

/// Registers the session `id` of `user` in `repo`, and makes it the lead
/// of `user` there, as the person does.
async fn lead(base: &str, user: &str, repo: &str, id: &str) {
    let host = if user == "mike" { "thelio" } else { "heron" };
    let person = format!("riff://{user}@{host}/{repo}");
    let session = format!("{person}?session={id}");
    let _: Value = ok(base, "/v1/register", json!({ "me": session })).await;
    let _: Value = ok(base, "/v1/lead", json!({ "me": person, "session": id })).await;
}

/// Posts `body` to the chat as mike, with a selector for each name.
async fn chat(base: &str, names: &[&str], body: &str) -> Posted {
    let to: Vec<Value> = names.iter().map(|name| json!({ "name": name })).collect();
    let post = json!({ "me": MIKE, "thread": "chat", "to": to, "body": body });
    ok(base, "/v1/post", post).await
}

/// The user and the session ID of each woken session of `posted`.
fn woken(posted: &Posted) -> Vec<String> {
    posted
        .woken
        .iter()
        .map(|uri| {
            let who = uri.who();
            format!("{}:{}", who.user(), who.session().unwrap_or("person"))
        })
        .collect()
}

/// Two projects with the name `riff`, and a project with the name of
/// the person `brett`: each of them has `OWNER/REPO`. The other names
/// are the user and the name after the slash. A project is online with
/// a live lead.
#[tokio::test]
async fn the_names_have_owner_repo_for_each_project_of_a_clash() {
    let (_service, base) = common::start(false, &[]).await;
    lead(&base, "mike", "como-technologies/riff", "m1").await;
    for session in [
        "riff://mike@thelio/acme/riff?session=m2",
        "riff://brett@heron/como-technologies/strata?session=b1",
        "riff://brett@heron/como-technologies/brett?session=b2",
    ] {
        let _: Value = ok(&base, "/v1/register", json!({ "me": session })).await;
    }
    let reply: NamesReply = ok(&base, "/v1/names", json!({ "me": MIKE })).await;
    let names: Vec<(&str, NameKind, bool)> = reply
        .names
        .iter()
        .map(|n| (n.name.as_str(), n.kind, n.online))
        .collect();
    assert_eq!(
        names,
        [
            ("acme/riff", NameKind::Project, false),
            ("brett", NameKind::Person, false),
            ("como-technologies/brett", NameKind::Project, false),
            ("como-technologies/riff", NameKind::Project, true),
            ("mike", NameKind::Person, false),
            ("strata", NameKind::Project, false),
        ]
    );
}

/// `@brett @riff` wakes the terminals of brett and the one lead of riff,
/// of any person: the session of brett that took the lead last, and not
/// the lead of the sender in another project. A line with no name wakes
/// no one. A line with an unknown name goes, and wakes no one.
#[tokio::test]
async fn a_post_wakes_the_person_and_the_lead_of_the_project() {
    let (_service, base) = common::start(false, &[]).await;
    lead(&base, "mike", "como-technologies/riff", "m1").await;
    lead(&base, "brett", "como-technologies/riff", "b1").await;
    lead(&base, "mike", "como-technologies/strata", "m3").await;
    let _: Value = ok(&base, "/v1/register", json!({ "me": BRETT })).await;

    let both = chat(&base, &["brett", "riff"], "@brett @riff is #12 done?").await;
    assert_eq!(woken(&both), ["brett:person", "brett:b1"]);
    assert!(both.unmatched.is_empty(), "{:?}", both.unmatched);

    let plain = chat(&base, &[], "the release is out").await;
    assert!(plain.woken.is_empty() && plain.unmatched.is_empty());

    let unknown = chat(&base, &["nobody"], "@nobody hi").await;
    assert!(unknown.woken.is_empty());
    let unmatched: Vec<String> = unknown.unmatched.iter().map(|s| s.to_string()).collect();
    assert_eq!(unmatched, ["name=nobody"]);

    let read = json!({ "me": MIKE, "thread": "chat", "all": true });
    let log: ReadReply = ok(&base, "/v1/read", read).await;
    let bodies: Vec<&str> = log.messages.iter().map(|m| m.body.as_str()).collect();
    assert_eq!(
        bodies,
        [
            "@brett @riff is #12 done?",
            "the release is out",
            "@nobody hi"
        ]
    );
}

/// A project with no lead that runs: the line waits in the log, and no
/// free session of the sender wakes in the place of the lead. A name
/// with another field is refused.
#[tokio::test]
async fn a_project_with_no_lead_wakes_no_one_and_a_name_stands_alone() {
    let (_service, base) = common::start(false, &[]).await;
    let free = "riff://mike@thelio/como-technologies/riff?session=m1";
    let _: Value = ok(&base, "/v1/register", json!({ "me": free })).await;

    let waits = chat(&base, &["riff"], "@riff is #12 done?").await;
    assert!(waits.woken.is_empty());
    assert_eq!(waits.seq, 1, "the line is in the log");

    let wide = json!({
        "me": MIKE, "thread": "chat", "to": [{ "name": "riff", "host": "thelio" }], "body": "hi",
    });
    let refused = common::client()
        .post(format!("{base}/v1/post"))
        .json(&wide)
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 400);
    let text = refused.text().await.unwrap();
    assert!(text.contains("a name stands alone"), "{text}");
}
