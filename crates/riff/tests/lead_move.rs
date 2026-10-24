//! `riff lead move HOST` (#1163, 01M596DH64SQ5AJ4Y6BETGTPSK,
//! 01M596DH8G4A5XQFHJFBX1X379): the handover, the daemon of the host,
//! the new lead and the message to the old lead. The old lead and the
//! daemon of `thelio` are tasks on a real riff-server.

use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use riff::daemon::remote::{PREFIX, read_reply, reply_body};
use riff_client::api::Api;
use riff_core::name::{SessionUri, Who};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, StartReason, Status};

const WAIT: Duration = Duration::from_secs(20);

struct World {
    api: Api,
    dir: tempfile::TempDir,
}

async fn world() -> World {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
    }
    World {
        api: Api::new(&format!("http://{addr}")),
        dir,
    }
}

impl World {
    fn uri(&self, user: &str, host: &str, session: Option<&str>) -> SessionUri {
        let place = riff::identity::place_in(self.dir.path(), host).unwrap();
        SessionUri::new(Who::new(user, session).unwrap(), place)
    }

    /// A registered session of mike, the lead when `lead`.
    async fn session(&self, host: &str, id: &str, lead: bool) -> SessionUri {
        let me = self.uri("mike", host, Some(id));
        self.api.register(&me).await.unwrap();
        if lead {
            let person = self.uri("mike", host, None);
            self.api.lead(&person, id).await.unwrap();
        }
        me
    }

    /// The session IDs of the leads in `who`.
    async fn leads(&self, me: &SessionUri) -> Vec<String> {
        let who = self.api.who(me, false).await.unwrap();
        who.iter()
            .filter(|s| s.uri.lead())
            .filter_map(|s| s.uri.who().session().map(str::to_owned))
            .collect()
    }

    /// The old lead: it writes the handover when asked, unless `silent`.
    /// It gives the texts that it got.
    fn old_lead(&self, me: SessionUri, silent: bool) -> Arc<Mutex<Vec<String>>> {
        let got = Arc::new(Mutex::new(Vec::new()));
        let (api, seen) = (self.api.clone(), got.clone());
        let thread = me.default_thread().unwrap();
        tokio::spawn(async move {
            loop {
                for inbox in api.inbox(&me, None, false).await.unwrap_or_default() {
                    for checked in inbox.messages {
                        let body = checked.message.body;
                        if body.contains("Write your handover") && !silent {
                            api.notes_set(&me, &thread, "Handover: wave 36 runs.")
                                .await
                                .unwrap();
                        }
                        seen.lock().unwrap().push(body);
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        got
    }

    /// The daemon `d1` of `thelio`: it answers `lead move` with the
    /// session `n1` as the new lead.
    async fn daemon(&self, host: &str) {
        let me = self.uri("mike", host, Some("d1"));
        self.api
            .start(&me, StartReason::Process, false)
            .await
            .unwrap();
        let step = format!("{PREFIX} pid 1, build test, 0 sessions");
        self.api.status(&me, &Status { step }).await.unwrap();
        let (api, host) = (self.api.clone(), host.to_owned());
        let (ready, opened) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            // An open watch makes the daemon live.
            let _watch = api.watch(&me).await.unwrap();
            let _ = ready.send(());
            loop {
                for inbox in api.inbox(&me, None, false).await.unwrap_or_default() {
                    for checked in inbox.messages {
                        if checked.message.body != "lead move" {
                            continue;
                        }
                        let new = me.clone();
                        let new = SessionUri::new(
                            Who::new("mike", Some("n1")).unwrap(),
                            new.place().clone(),
                        );
                        api.start(&new, StartReason::Process, false).await.unwrap();
                        let person =
                            SessionUri::new(Who::new("mike", None).unwrap(), new.place().clone());
                        api.lead(&person, "n1").await.unwrap();
                        let text = riff::text::lead_taken(
                            "como-technologies/riff",
                            &host,
                            "riff-lead",
                            "n1",
                        );
                        let body = reply_body(checked.message.seq, &host, &Ok(text));
                        let from = checked.message.from.who();
                        let to = [match from.session() {
                            Some(id) => Selector::session(id),
                            None => Selector {
                                user: Some("mike".into()),
                                ..Selector::default()
                            },
                        }];
                        api.post(&me, None, &to, &body, Kind::Note).await.unwrap();
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        opened.await.unwrap();
    }
}

async fn until<F: std::future::Future<Output = bool>>(what: &str, mut f: impl FnMut() -> F) {
    let start = std::time::Instant::now();
    while !f().await {
        assert!(start.elapsed() < WAIT, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The person moves the lead: the old lead writes the handover, the
/// daemon of thelio starts the new lead, and the old lead hears that it
/// is not the lead any more.
#[tokio::test]
async fn the_move_makes_the_new_session_the_lead_and_tells_the_old_lead() {
    let w = world().await;
    let old = w.session("pangolin", "l1", true).await;
    let got = w.old_lead(old.clone(), false);
    w.daemon("thelio").await;
    let person = w.uri("mike", "pangolin", None);

    let session = riff::lead::move_to(&w.api, &person, "thelio", WAIT)
        .await
        .unwrap();

    assert_eq!(session, "n1");
    assert_eq!(w.leads(&person).await, ["n1"]);
    let notes = w
        .api
        .notes_show(&person, &person.default_thread().unwrap(), None, false)
        .await
        .unwrap();
    assert_eq!(notes.text, "Handover: wave 36 runs.");
    until("the old lead hears it", || async {
        got.lock()
            .unwrap()
            .iter()
            .any(|m| m.contains("You are not the lead any more"))
    })
    .await;
    assert!(got.lock().unwrap()[0].contains("Write your handover"));
    assert!(read_reply("x", 1).is_none());
}

/// The lead runs the command itself: it asks nobody for a handover, and
/// the daemon makes the new lead.
#[tokio::test]
async fn the_lead_moves_itself_and_asks_nobody_for_a_handover() {
    let w = world().await;
    let old = w.session("pangolin", "l1", true).await;
    w.daemon("thelio").await;

    let session = riff::lead::move_to(&w.api, &old, "thelio", WAIT)
        .await
        .unwrap();

    assert_eq!(session, "n1");
    assert_eq!(w.leads(&old).await, ["n1"]);
}

/// A session that is not the lead cannot move the lead.
#[tokio::test]
async fn a_session_that_is_not_the_lead_cannot_move_the_lead() {
    let w = world().await;
    w.session("pangolin", "l1", true).await;
    let worker = w.session("pangolin", "w1", false).await;
    w.daemon("thelio").await;

    let err = riff::lead::move_to(&w.api, &worker, "thelio", WAIT)
        .await
        .unwrap_err();

    assert_eq!(format!("{err:#}"), riff::text::LEAD_MOVE_NOT_LEAD);
    assert_eq!(w.leads(&worker).await, ["l1"]);
}

/// A lead that writes no handover in time does not stop the move.
#[tokio::test]
async fn a_lead_that_writes_no_handover_in_time_is_moved_with_the_notes_as_they_are() {
    let w = world().await;
    let old = w.session("pangolin", "l1", true).await;
    w.old_lead(old, true);
    w.daemon("thelio").await;
    let person = w.uri("mike", "pangolin", None);

    let session = riff::lead::move_to(&w.api, &person, "thelio", Duration::from_millis(600))
        .await
        .unwrap();

    assert_eq!(session, "n1");
    assert_eq!(w.leads(&person).await, ["n1"]);
}

/// A host with no daemon fails, asks the old lead for nothing, and the old
/// lead stays the lead. The same host is refused too.
#[tokio::test]
async fn a_host_with_no_daemon_or_the_same_host_fails_and_the_old_lead_stays() {
    let w = world().await;
    let old = w.session("pangolin", "l1", true).await;
    let got = w.old_lead(old, false);
    let person = w.uri("mike", "pangolin", None);

    let err = riff::lead::move_to(&w.api, &person, "thelio", WAIT)
        .await
        .unwrap_err();
    assert_eq!(
        format!("{err:#}"),
        riff::text::no_daemon_host("thelio", "como-technologies/riff")
    );
    let err = riff::lead::move_to(&w.api, &person, "pangolin", WAIT)
        .await
        .unwrap_err();
    assert_eq!(
        format!("{err:#}"),
        riff::text::lead_move_same_host("pangolin")
    );

    assert_eq!(w.leads(&person).await, ["l1"]);
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(got.lock().unwrap().is_empty());
}
