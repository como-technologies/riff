//! The webhook of the session App over HTTP (#1655,
//! 01M74A2TT5N3BZPCT04XSHNDCV, 01M74A2TWZBEJF4HCPXJSDQ2Y0,
//! 01M74A379EQQXW6G86WVFRC1V0, 01M74A37CFS9QMB0PCW7J7XS3B,
//! 01M74A37FPGKYMHP0W588A88RJ, 01M74A37JNKW1R2WRXAW520WJM,
//! 01M74A37NMGDEHAD01SJ0DQT9B): the signature, a repeated delivery, the
//! debounce, the queue, the wakes of the plan and the last events. Each
//! delivery comes as from GitHub: with no riff token and no build.

use crate::common;
use crate::fake_github::{both_apps, webhook_secret};

use std::time::{Duration, Instant};

use riff_core::dpop::Key;
use riff_core::wire::{GitHubChange, GitHubEvent, GitHubEventsReply, GitHubWake, PlanChanged};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::github::Settings;
use riff_server::hooks::{DEBOUNCE, signature};
use riff_server::keys::{At, FILE};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// The daemon of ann on heron, and the daemon of bob on thelio: two
/// hosts of the wave of `acme/app`.
const HERON: &str = "riff://ann@heron/acme/app";
const THELIO: &str = "riff://bob@thelio/acme/app";

/// A short window of the debounce, for the tests that do not count it.
const SHORT: Duration = Duration::from_millis(200);

/// How long a test waits for an event that must come.
const WAIT: Duration = Duration::from_secs(10);

/// How long a test waits to see that no event comes.
const NONE: Duration = Duration::from_millis(800);

/// A riff whose session App has the webhook secret of the test, with the
/// window `debounce`. The folder keeps the keys of the Apps.
async fn serve(debounce: Duration, require_sign_in: bool) -> (Service, String, tempfile::TempDir) {
    common::client();
    let dir = tempfile::tempdir().unwrap();
    let apps = serde_json::to_vec(&both_apps()).unwrap();
    std::fs::write(dir.path().join(FILE), apps).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        require_sign_in,
        github: Settings {
            keys: At::Dir(dir.path().to_owned()),
            debounce,
            ..Settings::default()
        },
        ..Config::new(&url)
    };
    let service = Service::new(config);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url, dir)
}

/// A delivery of the kind `kind` with the ID `id`, as GitHub sends it,
/// signed with `secret`: the status and the text.
async fn deliver(
    url: &str,
    kind: &str,
    id: &str,
    body: &Value,
    secret: Option<&str>,
) -> (u16, String) {
    let body = serde_json::to_vec(body).unwrap();
    let mut request = reqwest::Client::new()
        .post(format!("{url}/hooks/github"))
        .header("x-github-event", kind)
        .header("x-github-delivery", id)
        .header("content-type", "application/json");
    if let Some(secret) = secret {
        request = request.header("x-hub-signature-256", signature(secret, &body));
    }
    let reply = request.body(body).send().await.unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

/// A signed delivery: it must get status 202.
async fn signed(url: &str, kind: &str, id: &str, body: &Value) {
    let (status, text) = deliver(url, kind, id, body, Some(webhook_secret())).await;
    assert_eq!(status, 202, "{text}");
}

fn repository(repo: &str) -> Value {
    json!({ "full_name": repo })
}

fn pull_request(repo: &str, number: u64, action: &str, merged: bool) -> Value {
    json!({
        "action": action,
        "number": number,
        "repository": repository(repo),
        "pull_request": { "number": number, "merged": merged },
    })
}

fn check_run(number: u64) -> Value {
    json!({
        "action": "completed",
        "repository": repository("acme/app"),
        "check_run": { "pull_requests": [{ "number": number }] },
    })
}

fn milestoned(number: u64, title: &str, state: &str) -> Value {
    json!({
        "action": "milestoned",
        "repository": repository("acme/app"),
        "issue": { "number": number },
        "milestone": { "title": title, "state": state },
    })
}

/// An open event stream, and the text that it gave and that no event
/// took yet.
struct Events {
    reply: reqwest::Response,
    buffer: String,
}

impl Events {
    /// `GET PATH` of the thread `thread` as `uri`, with the query `more`.
    async fn open(
        url: &str,
        path: &str,
        uri: &str,
        thread: &str,
        more: &[(&str, String)],
    ) -> Events {
        let mut query = vec![("uri", uri.to_owned()), ("thread", thread.to_owned())];
        query.extend(more.iter().map(|(k, v)| (*k, v.clone())));
        let reply = common::client()
            .get(format!("{url}{path}"))
            .query(&query)
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), 200);
        Events {
            reply,
            buffer: String::new(),
        }
    }

    /// The `github/watch` stream of `uri`, after the wake `after`.
    async fn wakes(url: &str, uri: &str, after: Option<u64>) -> Events {
        let after: Vec<(&str, String)> = after
            .map(|id| ("after", id.to_string()))
            .into_iter()
            .collect();
        Events::open(url, "/v1/github/watch", uri, "acme/app", &after).await
    }

    /// The `plan/watch` stream of `uri`.
    async fn plan(url: &str, uri: &str) -> Events {
        Events::open(url, "/v1/plan/watch", uri, "acme/app", &[]).await
    }

    /// The next event, or `None` when none comes in `wait`.
    async fn next<T: DeserializeOwned>(&mut self, wait: Duration) -> Option<T> {
        loop {
            if let Some(end) = self.buffer.find('\n') {
                let line: String = self.buffer.drain(..=end).collect();
                match line.trim().strip_prefix("data:") {
                    Some(data) => return Some(serde_json::from_str(data.trim()).unwrap()),
                    None => continue,
                }
            }
            let chunk = tokio::time::timeout(wait, self.reply.chunk()).await.ok()?;
            let chunk = chunk.unwrap().expect("the stream stays open");
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
    }

    /// The next wake: it must come.
    async fn wake(&mut self) -> GitHubWake {
        self.next(WAIT).await.expect("a wake")
    }
}

/// 01M74A379EQQXW6G86WVFRC1V0, 01M74A37FPGKYMHP0W588A88RJ: a signed
/// event wakes the daemon of its repository with the kind and the
/// number, and no daemon of another repository.
#[tokio::test]
async fn a_signed_event_wakes_the_daemon_of_the_repository_with_the_kind_and_the_number() {
    let (_service, url, _dir) = serve(SHORT, false).await;
    let mut app = Events::wakes(&url, HERON, None).await;
    let mut web = Events::open(&url, "/v1/github/watch", HERON, "acme/web", &[]).await;
    let body = pull_request("acme/app", 41, "synchronize", false);
    signed(&url, "pull_request", "d-1", &body).await;
    let wake = app.wake().await;
    assert_eq!(wake.repo.to_string(), "acme/app");
    assert_eq!(wake.kinds, [GitHubEvent::PullRequest]);
    assert_eq!((wake.number, wake.events), (Some(41), 1));
    assert_eq!(
        web.next::<GitHubWake>(NONE).await,
        None,
        "no wake of another repository"
    );
}

/// 01M74A2TT5N3BZPCT04XSHNDCV: a delivery with no signature, with the
/// signature of another secret, or with the signature of another body
/// gets status 401, and wakes nobody.
#[tokio::test]
async fn a_bad_signature_gets_401_and_wakes_nobody() {
    let (_service, url, _dir) = serve(SHORT, false).await;
    let mut app = Events::wakes(&url, HERON, None).await;
    let body = pull_request("acme/app", 41, "synchronize", false);
    let (status, text) = deliver(&url, "pull_request", "d-1", &body, None).await;
    assert_eq!(status, 401, "{text}");
    let (status, _) = deliver(
        &url,
        "pull_request",
        "d-2",
        &body,
        Some(&isolated::fake_secret()),
    )
    .await;
    assert_eq!(status, 401);
    let other = signature(webhook_secret(), b"{}");
    let reply = reqwest::Client::new()
        .post(format!("{url}/hooks/github"))
        .header("x-github-event", "pull_request")
        .header("x-hub-signature-256", other)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 401);
    assert_eq!(app.next::<GitHubWake>(SHORT + NONE).await, None);
}

/// 01M74A2TWZBEJF4HCPXJSDQ2Y0: a delivery that comes again, with the
/// same `X-GitHub-Delivery`, wakes nobody.
#[tokio::test]
async fn a_repeated_delivery_wakes_nobody() {
    let (_service, url, _dir) = serve(SHORT, false).await;
    let mut app = Events::wakes(&url, HERON, None).await;
    let body = json!({ "repository": repository("acme/app"), "issue": { "number": 7 } });
    signed(&url, "issue_comment", "d-1", &body).await;
    let wake = app.wake().await;
    assert_eq!(
        (wake.kinds, wake.number),
        (vec![GitHubEvent::IssueComment], Some(7))
    );
    let (status, text) = deliver(&url, "issue_comment", "d-1", &body, Some(webhook_secret())).await;
    assert_eq!(status, 202);
    assert!(text.contains("repeated"), "{text}");
    assert_eq!(
        app.next::<GitHubWake>(SHORT + NONE).await,
        None,
        "the repeat wakes nobody"
    );
}

/// 01M74A37CFS9QMB0PCW7J7XS3B: 20 `check_run` events of one pull request
/// in 5 seconds give one wake, at the end of the window of
/// [`DEBOUNCE`].
#[tokio::test]
async fn twenty_check_run_events_of_one_pull_request_in_5_seconds_give_one_wake() {
    let (_service, url, _dir) = serve(DEBOUNCE, false).await;
    let mut app = Events::wakes(&url, HERON, None).await;
    let start = Instant::now();
    for n in 0..20 {
        signed(&url, "check_run", &format!("d-{n}"), &check_run(12)).await;
    }
    assert!(
        start.elapsed() < DEBOUNCE,
        "the 20 events came in 5 seconds"
    );
    let wake = app.wake().await;
    assert!(
        start.elapsed() >= DEBOUNCE,
        "the wake waits for the end of the window"
    );
    assert_eq!(wake.kinds, [GitHubEvent::CheckRun]);
    assert_eq!((wake.number, wake.events), (Some(12), 20));
    assert_eq!(app.next::<GitHubWake>(NONE).await, None, "one wake");
}

/// 01M74A37FPGKYMHP0W588A88RJ: a daemon that was away gets the wakes of
/// the queue after the ID of its last wake when it comes back, then each
/// new wake. A stream with no ID gets only the new wakes.
#[tokio::test]
async fn a_daemon_that_was_away_gets_the_wakes_of_the_queue_when_it_comes_back() {
    let (_service, url, _dir) = serve(SHORT, false).await;
    let mut daemon = Events::wakes(&url, HERON, None).await;
    signed(&url, "check_run", "d-1", &check_run(12)).await;
    let last = daemon.wake().await;
    drop(daemon);

    // The daemon is away. Another host sees the wakes go out.
    let mut other = Events::wakes(&url, THELIO, None).await;
    let comment = json!({ "repository": repository("acme/app"), "issue": { "number": 7 } });
    signed(&url, "issue_comment", "d-2", &comment).await;
    let status = json!({ "repository": repository("acme/app"), "sha": "abc" });
    signed(&url, "status", "d-3", &status).await;
    let missed = [other.wake().await, other.wake().await];

    let mut back = Events::wakes(&url, HERON, Some(last.id)).await;
    let mut fresh = Events::wakes(&url, THELIO, None).await;
    let got = [back.wake().await, back.wake().await];
    assert_eq!(got, missed, "the wakes of the queue, the oldest first");
    let numbers: Vec<Option<u64>> = got.iter().map(|wake| wake.number).collect();
    assert!(
        numbers.contains(&Some(7)) && numbers.contains(&None),
        "{numbers:?}"
    );
    assert!(got[0].id < got[1].id && got[0].id > last.id);

    signed(&url, "check_run", "d-4", &check_run(13)).await;
    assert_eq!(back.wake().await.number, Some(13), "then each new wake");
    assert_eq!(fresh.wake().await.number, Some(13), "no wake of the queue");
}

/// 01M74A37JNKW1R2WRXAW520WJM: an issue that comes into an open wave
/// (`issues`, `milestoned`) and the merge of a pull request each wake the
/// wave of each host of the repository at once: one event on `GET
/// /v1/plan/watch`, with no wait of the debounce. An issue that goes to a
/// closed wave or to a milestone that is no wave, a pull request closed
/// with no merge, and a check run change no plan.
#[tokio::test]
async fn a_new_item_in_the_wave_and_a_merge_wake_the_wave_of_each_host() {
    let (_service, url, _dir) = serve(Duration::from_secs(60), false).await;
    let mut heron = Events::plan(&url, HERON).await;
    let mut thelio = Events::plan(&url, THELIO).await;

    let new_item = milestoned(12, "Wave 54: events", "open");
    signed(&url, "issues", "d-1", &new_item).await;
    let want = GitHubChange::NewItem {
        number: 12,
        wave: "Wave 54: events".into(),
    };
    for host in [&mut heron, &mut thelio] {
        let changed: PlanChanged = host.next(WAIT).await.expect("the wave wakes");
        assert_eq!(changed.thread.to_string(), "acme/app");
        assert_eq!(changed.github.as_ref(), Some(&want));
    }

    signed(
        &url,
        "pull_request",
        "d-2",
        &pull_request("acme/app", 41, "closed", true),
    )
    .await;
    for host in [&mut heron, &mut thelio] {
        let changed: PlanChanged = host.next(WAIT).await.expect("the wave wakes");
        assert_eq!(changed.github, Some(GitHubChange::Merged { number: 41 }));
    }

    signed(&url, "issues", "d-3", &milestoned(13, "Wave 50", "closed")).await;
    signed(&url, "issues", "d-4", &milestoned(14, "Backlog", "open")).await;
    signed(
        &url,
        "pull_request",
        "d-5",
        &pull_request("acme/app", 42, "closed", false),
    )
    .await;
    signed(&url, "check_run", "d-6", &check_run(43)).await;
    signed(
        &url,
        "pull_request",
        "d-7",
        &pull_request("acme/web", 44, "closed", true),
    )
    .await;
    assert_eq!(heron.next::<PlanChanged>(NONE).await, None);
    assert_eq!(thelio.next::<PlanChanged>(NONE).await, None);
}

/// 01M74A37NMGDEHAD01SJ0DQT9B: `POST /v1/github/events` gives the last
/// wakes of the repository and the counts of the deliveries, to each
/// person of the riff, for `riff github events`.
#[tokio::test]
async fn the_last_events_give_the_wakes_and_the_counts() {
    let (service, url, _dir) = serve(SHORT, true).await;
    let key = Key::generate();
    let pair = service
        .admit("ada@example.com", true, &key.thumbprint())
        .await
        .unwrap();
    let events = || async {
        let reply = common::post(
            &format!("{url}/v1/github/events"),
            &key,
            Some(&pair.access_token),
        )
        .json(&json!({ "repo": "acme/app" }))
        .send()
        .await
        .unwrap();
        assert_eq!(reply.status(), 200);
        reply.json::<GitHubEventsReply>().await.unwrap()
    };
    assert_eq!(events().await.wakes, []);
    signed(&url, "check_run", "d-1", &check_run(12)).await;
    signed(&url, "check_run", "d-1", &check_run(12)).await;
    signed(
        &url,
        "pull_request",
        "d-2",
        &pull_request("acme/web", 4, "opened", false),
    )
    .await;
    let body = check_run(12);
    let (status, _) = deliver(&url, "check_run", "d-3", &body, None).await;
    assert_eq!(status, 401);
    let start = Instant::now();
    let reply = loop {
        let reply = events().await;
        if !reply.wakes.is_empty() || start.elapsed() > WAIT {
            break reply;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(reply.wakes.len(), 1, "{reply:?}");
    assert_eq!(reply.wakes[0].number, Some(12));
    assert_eq!((reply.taken, reply.repeated, reply.refused), (2, 1, 1));
    assert!(reply.since_ms > 0);
    let reply = common::post(
        &format!("{url}/v1/github/events"),
        &key,
        Some(&pair.access_token),
    )
    .json(&json!({ "repo": "no repo" }))
    .send()
    .await
    .unwrap();
    assert_eq!(reply.status(), 400);
    let reply = common::client()
        .post(format!("{url}/v1/github/events"))
        .json(&json!({ "repo": "acme/app" }))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 401, "only a person of the riff");
}
