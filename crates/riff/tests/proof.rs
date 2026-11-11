//! A proof of presence from `riff` (01M75WKC8RCPHQW3NADHRA31SG,
//! 01M75WKSADNAW4RNFAXD48D7DF, 01M75WKSFX84BHHGFX8VJ37DC6,
//! 01M75WKSJTB28ZQV8ADPBQ86WE, 01M75WKSNREGG95CK40W2A5JPQ,
//! 01M75WKT1A7K8RTNB4NVTXB41V, 01M75WKSYCMMJFGAM3W4ASGN85): the help, the
//! proof in the page of a fake browser with a fake authenticator, the
//! second try of an action with its proof, and the end of a session on
//! another host through a fake daemon.

use crate::book;
use crate::common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use isolated::Isolated;
use riff::daemon::remote::{self, Request};
use riff::device::Wait;
use riff::proof::{self, Asker};
use riff_client::api::{Api, PauseScope};
use riff_core::device::fake::{Authenticator, Proof, Shape};
use riff_core::name::SessionUri;
use riff_core::proof::{Action, Policy};
use riff_core::wire::{KeyOptions, KeyReply, ProofOptions, RiffState};

/// The stdout of `riff ARGS`, in a terminal of 200 columns.
fn riff(args: &[&str]) -> String {
    let out = Isolated::shared()
        .riff()
        .args(args)
        .env("COLUMNS", "200")
        .output()
        .unwrap();
    assert!(out.status.success(), "riff {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// `riff admin proof` takes `off` and `required`, and `riff admin` and
/// `riff device` list their new subcommands.
#[test]
fn the_help_names_the_policy_and_the_test_of_a_proof() {
    let admin = riff(&["admin", "--help"]);
    assert!(
        admin.contains("  proof   Set the proof of presence of the riff: off or required\n"),
        "{admin}"
    );
    let proof = riff(&["admin", "proof", "--help"]);
    assert!(
        proof.contains("[possible values: off, required]"),
        "{proof}"
    );
    assert!(proof.contains("riff device add --key NAME"), "{proof}");
    let out = Isolated::shared()
        .riff()
        .args(["admin", "proof", "on"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let top = riff(&["--help"]);
    assert!(
        top.contains("  admin         Make or remove an admin, or set the proof of presence\n"),
        "{top}"
    );
}

/// The how-tos of the book run with `--help`.
#[test]
fn each_how_to_of_the_proof_in_the_book_is_a_real_command() {
    let commands = book::commands_of_part("how-it-works.md", "A proof of presence");
    assert_eq!(
        commands,
        [
            "riff device add --key yubikey",
            "riff device list",
            "riff device remove yubikey",
            "riff device test",
            "riff admin proof required",
            "riff admin proof off",
        ]
    );
    book::each_is_real(&commands);
}

/// A short wait, for a test.
const WAIT: Wait = Wait {
    every: Duration::from_millis(20),
    most: Duration::from_secs(30),
};

/// The browser of the test for the page of an add: it sends the reply of
/// `key`, as the page does.
fn key_page(server: String, key: Arc<Authenticator>) -> impl FnOnce(&str) {
    move |url: &str| {
        let url = url.to_owned();
        tokio::spawn(async move {
            let (page, ticket) = url.split_once('#').unwrap();
            let origin = page.strip_suffix("/device/key").unwrap();
            let client = reqwest::Client::new();
            let options: KeyOptions = client
                .post(format!("{server}/device/key/options"))
                .json(&serde_json::json!({ "ticket": ticket }))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            let reply: KeyReply = key.reply_as(ticket, &options, origin, Shape::Packed);
            client
                .post(format!("{server}/device/key/reply"))
                .json(&reply)
                .send()
                .await
                .unwrap();
        });
    }
}

/// The asker of the test: its browser signs each proof with `key` in
/// the shape `shape`, and it keeps each line that riff shows and each
/// page that it opens.
struct Browser {
    asker: Asker,
    shown: Arc<Mutex<Vec<String>>>,
    opened: Arc<Mutex<Vec<String>>>,
}

fn browser(server: &str, key: Arc<Authenticator>, shape: Proof) -> Browser {
    let shown = Arc::new(Mutex::new(Vec::new()));
    let opened = Arc::new(Mutex::new(Vec::new()));
    let (lines, pages, server) = (shown.clone(), opened.clone(), server.to_owned());
    let asker = Asker {
        show: Box::new(move |text| lines.lock().unwrap().push(text.to_owned())),
        open: Box::new(move |url| {
            pages.lock().unwrap().push(url.to_owned());
            let (url, server, key) = (url.to_owned(), server.clone(), key.clone());
            tokio::spawn(async move {
                let (page, ticket) = url.split_once('#').unwrap();
                let origin = page.strip_suffix("/proof").unwrap();
                let client = reqwest::Client::new();
                let options: ProofOptions = client
                    .post(format!("{server}/proof/options"))
                    .json(&serde_json::json!({ "ticket": ticket }))
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                let reply = key.assert_as(ticket, &options, origin, shape);
                client
                    .post(format!("{server}/proof/reply"))
                    .json(&reply)
                    .send()
                    .await
                    .unwrap();
            });
        }),
        wait: WAIT,
        session: false,
        short: false,
    };
    Browser {
        asker,
        shown,
        opened,
    }
}

/// The asker of a session: riff asks for no proof there.
fn session_asker() -> Asker {
    Asker {
        show: Box::new(|_| {}),
        open: Box::new(|url| panic!("a session opens no page: {url}")),
        wait: WAIT,
        session: true,
        short: false,
    }
}

/// brett, the owner of a riff with sign-in, with the key `yubikey`, and
/// the riff resumed. Gives the server, the client of brett and the key.
async fn owner_with_a_key() -> (common::Signed, Api, Arc<Authenticator>) {
    let server = common::start_signed().await;
    let api = common::api(&server, "brett", None);
    let key = Arc::new(Authenticator::new());
    let open = key_page(server.to_string(), key.clone());
    riff::device::add_key(&api, "yubikey", |_| {}, open, WAIT)
        .await
        .unwrap();
    let me: SessionUri = "riff://brett@thelio".parse().unwrap();
    api.set_pause(&me, &PauseScope::Riff, RiffState::Running)
        .await
        .unwrap();
    (server, api, key)
}

/// With `required`, riff asks for a proof in the browser when
/// riff-server refuses an action with no proof, and sends the action
/// again with it. A session gets the refusal, and opens no page. A
/// refused signature says why.
#[tokio::test]
async fn riff_asks_for_a_proof_and_sends_the_action_again_with_it() {
    let (server, api, key) = owner_with_a_key().await;
    let good = browser(&server, key.clone(), Proof::Good);

    // The change of the policy needs a proof, also from off.
    let required = Action::Policy {
        proof: Policy::Required,
    };
    let set = proof::act(&api, &required, &good.asker, |api| async move {
        api.set_policy(Policy::Required).await
    })
    .await
    .unwrap();
    assert!(set.changed && set.proof == Policy::Required);
    assert_eq!(good.opened.lock().unwrap().len(), 1);
    let shown = good.shown.lock().unwrap().join("\n");
    assert!(
        shown.starts_with(
            "riff asks for a proof of presence: set the proof of presence of the riff to required."
        ),
        "{shown}"
    );
    assert!(shown.contains("/proof#"), "{shown}");

    // An invite with a proof.
    let me: SessionUri = "riff://brett@thelio".parse().unwrap();
    let invite = Action::invite("cy@comotechnologies.io");
    let changed = proof::act(&api, &invite, &good.asker, |api| {
        let me = me.clone();
        async move { api.invite(&me, "cy@comotechnologies.io").await }
    })
    .await
    .unwrap();
    assert_eq!(changed.done.email, "cy@comotechnologies.io");
    assert_eq!(good.opened.lock().unwrap().len(), 2);

    // A session gets the refusal, and opens no page.
    let refused = proof::act(&api, &Action::PauseRiff, &session_asker(), |api| {
        let me = me.clone();
        async move {
            api.set_pause(&me, &PauseScope::Riff, RiffState::Paused)
                .await
        }
    })
    .await
    .unwrap_err();
    assert!(proof::needs_proof(&refused), "{refused:#}");
    assert!(
        format!("{refused:#}").contains("pause the whole riff needs a proof of presence"),
        "{refused:#}"
    );

    // A bad signature: riff says why, and the action does not run.
    let bad = browser(&server, key.clone(), Proof::BadSignature);
    let error = proof::act(&api, &Action::PauseRiff, &bad.asker, |api| {
        let me = me.clone();
        async move {
            api.set_pause(&me, &PauseScope::Riff, RiffState::Paused)
                .await
        }
    })
    .await
    .unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.starts_with("riff has no proof of presence to pause the whole riff:"),
        "{error}"
    );
    assert!(error.contains("signature"), "{error}");
    let pauses = api.pauses(&me).await.unwrap();
    assert!(pauses.riff.is_none(), "the riff runs: {pauses:?}");
    assert_eq!(pauses.proof, Policy::Required);

    // The test of a proof works with each policy, and names the key.
    let proved = proof::prove(&api, &Action::Test, &good.asker)
        .await
        .unwrap();
    assert_eq!(proved.name, "yubikey");
    assert_eq!(
        riff::text::proof_tested(&proved.name),
        "riff: your key yubikey gave a good proof of presence. riff device list shows its last use."
    );
}

/// The check of a step on this host: with `off`, no page opens; with
/// `required`, the step gets a proof; when riff cannot reach
/// riff-server, it warns and goes on.
#[tokio::test]
async fn a_step_on_this_host_asks_riff_server_first() {
    let (server, api, key) = owner_with_a_key().await;
    let good = browser(&server, key.clone(), Proof::Good);
    let end = Action::End {
        host: "thelio".into(),
        name: "riff-issue-12-1a2b3c4d".into(),
    };
    proof::gate(&api, &end, &good.asker).await.unwrap();
    assert!(good.opened.lock().unwrap().is_empty());
    // With off, a session goes on to a step that waits first.
    let me: SessionUri = "riff://brett@thelio/o/r".parse().unwrap();
    proof::refuse_in_session(&api, &me, &end, &session_asker())
        .await
        .unwrap();

    let required = Action::Policy {
        proof: Policy::Required,
    };
    proof::act(&api, &required, &good.asker, |api| async move {
        api.set_policy(Policy::Required).await
    })
    .await
    .unwrap();
    proof::gate(&api, &end, &good.asker).await.unwrap();
    assert_eq!(good.opened.lock().unwrap().len(), 2);
    let refused = proof::gate(&api, &end, &session_asker()).await.unwrap_err();
    assert!(proof::needs_proof(&refused), "{refused:#}");
    // With required, a session learns it before a step that waits; a
    // person goes on.
    let refused = proof::refuse_in_session(&api, &me, &end, &session_asker())
        .await
        .unwrap_err();
    assert_eq!(
        format!("{refused:#}"),
        riff::text::proof_in_session(&end.text())
    );
    proof::refuse_in_session(&api, &me, &end, &good.asker)
        .await
        .unwrap();

    // No riff-server: a warning, and the step goes on.
    let nowhere = Api::new("http://127.0.0.1:9");
    proof::gate(&nowhere, &end, &good.asker).await.unwrap();
    let shown = good.shown.lock().unwrap().join("\n");
    assert!(
        shown.contains("riff: warning: riff cannot ask riff-server for the check of the proof"),
        "{shown}"
    );
}

/// A fake riff daemon of brett on the host pangolin in `o/r`, as the
/// session `d0d0d0d0`, with a worker session `w1` that holds `issue-12`.
/// It judges each request as a daemon does, keeps its body, and for
/// `daemon end NAME` ends the session as the daemon does: it frees the
/// item and ends the session of the worker. It gives the bodies.
async fn fake_daemon(url: &str) -> Arc<Mutex<Vec<String>>> {
    let me: SessionUri = "riff://brett@pangolin/o/r?session=d0d0d0d0"
        .parse()
        .unwrap();
    let worker: SessionUri = "riff://brett@pangolin/o/r?session=w1".parse().unwrap();
    let api = common::api_of(url, &me);
    let api_worker = common::api_of(url, &worker);
    api.register(&me).await.unwrap();
    let step = remote::status_line(812, "3.3.0", 1);
    api.status(&me, &riff_core::wire::Status { step })
        .await
        .unwrap();
    api_worker.register(&worker).await.unwrap();
    let thread = worker.default_thread().unwrap();
    api_worker
        .claim(&worker, &thread, "issue-12")
        .await
        .unwrap();
    let got = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&got);
    let (open, opened) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _watch = api.watch(&me).await.unwrap();
        open.send(()).unwrap();
        loop {
            let inbox = api.inbox(&me, None, false).await.unwrap_or_default();
            for checked in inbox.into_iter().flat_map(|i| i.messages) {
                let Some(judged) = remote::judge(&checked, &me) else {
                    continue;
                };
                let reply = match &judged {
                    Ok(Request::End(name)) => {
                        api_worker
                            .release(&worker, &thread, "issue-12")
                            .await
                            .unwrap();
                        api_worker.end(&worker).await.unwrap();
                        Ok(riff::text::daemon_ends(name))
                    }
                    Ok(_) => Ok(String::new()),
                    Err(refused) => Err(refused.clone()),
                };
                kept.lock().unwrap().push(match judged {
                    Ok(request) => request.to_string(),
                    Err(refused) => refused,
                });
                let body = remote::reply_body(checked.message.seq, "pangolin", &reply);
                let to = [riff_core::selector::Selector {
                    user: Some("brett".into()),
                    ..riff_core::selector::Selector::default()
                }];
                let kind = riff_core::wire::Kind::Note;
                api.post(&me, None, &to, &body, kind).await.unwrap();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    opened.await.unwrap();
    got
}

/// With `required`, `riff daemon end NAME --host pangolin` with no proof
/// is refused by riff-server before it reaches the daemon of pangolin.
/// With a good proof, the daemon gets the request and ends the session:
/// the item is free, and the session of the worker is gone.
#[tokio::test(flavor = "multi_thread")]
async fn an_end_on_another_host_needs_a_proof_before_it_reaches_the_daemon() {
    let (server, api, key) = owner_with_a_key().await;
    let good = browser(&server, key.clone(), Proof::Good);
    proof::act(
        &api,
        &Action::Policy {
            proof: Policy::Required,
        },
        &good.asker,
        |api| async move { api.set_policy(Policy::Required).await },
    )
    .await
    .unwrap();
    let got = fake_daemon(&server).await;
    let api = common::api(&server, "brett", None);
    let me: SessionUri = "riff://brett@thelio/o/r".parse().unwrap();
    let request = Request::End("riff-issue-12-1a2b3c4d".into());
    let wait = Duration::from_secs(30);

    // No proof: refused before it reaches the daemon.
    let refused = remote::ask_with_proof(&api, &me, "pangolin", &request, wait, &session_asker())
        .await
        .unwrap_err();
    assert!(proof::needs_proof(&refused), "{refused:#}");
    assert!(
        format!("{refused:#}")
            .contains("end the session riff-issue-12-1a2b3c4d on the host pangolin needs a proof"),
        "{refused:#}"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(got.lock().unwrap().is_empty(), "the daemon got no request");

    // A good proof: the daemon ends the session.
    let reply = remote::ask_with_proof(&api, &me, "pangolin", &request, wait, &good.asker)
        .await
        .unwrap();
    assert_eq!(reply, riff::text::daemon_ends("riff-issue-12-1a2b3c4d"));
    assert_eq!(*got.lock().unwrap(), ["daemon end riff-issue-12-1a2b3c4d"]);
    let who = api.who(&me, false).await.unwrap();
    assert!(
        !who.iter()
            .any(|s| s.uri.who().session() == Some("w1") && s.live),
        "the worker ended: {who:?}"
    );
    assert!(
        !who.iter()
            .any(|s| s.uri.claims().contains(&"issue-12".to_owned()))
    );

    // A step that cannot lose work asks for no proof.
    let opened = good.opened.lock().unwrap().len();
    remote::ask_with_proof(&api, &me, "pangolin", &Request::Status, wait, &good.asker)
        .await
        .unwrap();
    assert_eq!(good.opened.lock().unwrap().len(), opened);
}
