//! The events of the GitHub App (#1655): the webhook of the session App,
//! the queue of the wakes, and the wake of the daemons.
//!
//! # Design
//!
//! GitHub sends a delivery to `POST /hooks/github` for each change on a
//! repository with the session App ([`crate::github::HOOK_PATH`], the
//! [`crate::github::EVENTS`]). riff-server wakes the daemons of the
//! repository. A wake names what changed, so a daemon can read only that
//! on GitHub. The daemon opens each stream, so a host behind a firewall
//! needs no inbound port.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant G as GitHub
//!     participant S as riff-server
//!     participant D as riff daemon
//!     D->>S: GET /v1/github/watch?after=ID
//!     G->>S: POST /hooks/github
//!     S->>S: check X-Hub-Signature-256, drop a repeated X-GitHub-Delivery
//!     S-->>D: plan/watch at once: a new item of a wave, a merge
//!     S->>S: one window of 5 seconds for each number
//!     S->>D: github/watch: the repository, the kinds, the number
//! ```
//!
//! - **The signature** (01M74A2TT5N3BZPCT04XSHNDCV). [`signed`] checks
//!   the header `X-Hub-Signature-256`: the HMAC-SHA256 of the body with
//!   the webhook secret of the session App. A delivery with no signature,
//!   a bad one, or to a riff with no secret gets status 401. riff-server
//!   logs one warning, at most one each [`REFUSED_LOG_EVERY`].
//! - **A repeat** (01M74A2TWZBEJF4HCPXJSDQ2Y0). [`Deliveries`] keeps each
//!   `X-GitHub-Delivery` for [`DELIVERY_LIFE`]. A delivery that came
//!   before gets status 202, and wakes nobody.
//! - **What an event names** (01M74A379EQQXW6G86WVFRC1V0). [`named`]
//!   reads the repository, the number of each issue or pull request, and
//!   a change of the plan. A kind that riff does not use, for example
//!   `ping`, gets status 202, and wakes nobody.
//! - **A change of the plan** (01M74A37JNKW1R2WRXAW520WJM). An issue that
//!   comes into an open wave (`issues`, `milestoned`), and a merge
//!   (`pull_request`, `closed` with `merged`), send one event on `GET
//!   /v1/plan/watch` of the repository thread at once
//!   ([`crate::engine::Engine::plan_changed_on_github`]). The riff daemon
//!   of each host follows that stream, and its wave looks at once.
//!   riff-server does not know which open wave is the current one, so it
//!   takes each open milestone `Wave N`.
//! - **The debounce** (01M74A37CFS9QMB0PCW7J7XS3B). The first event of a
//!   number opens a window of [`DEBOUNCE`]. Each event of the same number
//!   in the window goes into its wake ([`Hooks::add`]). At the end of the
//!   window, the wake goes into the queue and to each stream
//!   ([`Hooks::flush`]). So 20 `check_run` events of one pull request in
//!   5 seconds give one wake. The events of a commit with no pull request
//!   share the number `None`.
//! - **The queue** (01M74A37FPGKYMHP0W588A88RJ). riff-server keeps the
//!   wakes of each repository for [`QUEUE_LIFE`], at most [`QUEUE_MOST`].
//!   A daemon that was away asks for each wake after the ID of its last
//!   one ([`Hooks::queued`]). The queue is in memory: a new instance of
//!   riff-server starts with an empty queue. GitHub does not promise the
//!   order of its events, and a wake names no state: a daemon reads the
//!   state on GitHub.
//! - **The last events** (01M74A37NMGDEHAD01SJ0DQT9B). `POST
//!   /v1/github/events` gives the last [`EVENTS_SHOWN`] wakes of a
//!   repository and the counts of the deliveries, for `riff github
//!   events` ([`Hooks::events`]).
//!
//! # Example
//!
//! ```
//! use std::time::Instant;
//! use riff_core::wire::GitHubEvent;
//! use riff_server::hooks::{DEBOUNCE, Hooks, named};
//!
//! let hooks = Hooks::new(DEBOUNCE, 0);
//! let check = serde_json::json!({
//!     "repository": { "full_name": "acme/app" },
//!     "check_run": { "pull_requests": [{ "number": 12 }] },
//! });
//! let event = named("check_run", &check).unwrap();
//! let opened = hooks.add(&event);
//! assert_eq!(opened.len(), 1, "the first event opens a window");
//! assert!(hooks.add(&event).is_empty(), "the second one goes into it");
//! let now = Instant::now();
//! let wake = hooks.flush(&opened[0], now, 1_000).unwrap();
//! assert_eq!((wake.kinds, wake.number, wake.events), (vec![GitHubEvent::CheckRun], Some(12), 2));
//! assert_eq!(hooks.queued(&"acme/app".parse()?, 0, now).len(), 1);
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::State as AxumState;
use axum::http::{HeaderMap, StatusCode};
use axum::{Extension, Json};
use hmac::{Hmac, KeyInit, Mac};
use riff_core::name::ThreadName;
use riff_core::wire::{
    GitHubApp, GitHubChange, GitHubEvent, GitHubEvents, GitHubEventsReply, GitHubWake,
};
use serde_json::Value;
use sha2::Sha256;
use tokio::sync::broadcast;

use crate::auth::SignedIn;

/// The window of the debounce of one number.
pub const DEBOUNCE: Duration = Duration::from_secs(5);

/// How long the queue keeps a wake.
pub const QUEUE_LIFE: Duration = Duration::from_secs(60 * 60);

/// The most wakes that the queue keeps for one repository.
pub const QUEUE_MOST: usize = 1000;

/// How long riff-server keeps the ID of a delivery.
pub const DELIVERY_LIFE: Duration = Duration::from_secs(24 * 60 * 60);

/// The most IDs of deliveries that riff-server keeps.
pub const DELIVERIES_MOST: usize = 100_000;

/// How many wakes `POST /v1/github/events` gives.
pub const EVENTS_SHOWN: usize = 50;

/// riff-server logs at most one warning of a refused delivery this
/// often.
pub const REFUSED_LOG_EVERY: Duration = Duration::from_secs(60);

/// The header of the signature of a delivery.
pub const SIGNATURE_HEADER: &str = "x-hub-signature-256";

/// The header of the kind of a delivery.
pub const EVENT_HEADER: &str = "x-github-event";

/// The header of the ID of a delivery.
pub const DELIVERY_HEADER: &str = "x-github-delivery";

/// Events that a slow stream may miss before it drops them.
const WAKE_BUFFER: usize = 1024;

/// The signature of `body` with `secret`, as GitHub writes it in
/// `X-Hub-Signature-256`: `sha256=` and the HMAC-SHA256 in hex.
///
/// ```
/// use riff_server::hooks::signature;
///
/// // The example of the docs of GitHub.
/// assert_eq!(
///     signature("It's a Secret to Everybody", b"Hello, World!"),
///     "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17"
/// );
/// ```
pub fn signature(secret: &str, body: &[u8]) -> String {
    let tag = mac(secret, body).finalize().into_bytes();
    let hex: String = tag.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("sha256={hex}")
}

/// True when `header` is the signature of `body` with `secret`
/// ([`signature`]). The compare takes the same time for each wrong
/// signature of the same length.
///
/// ```
/// use riff_server::hooks::{signature, signed};
///
/// let good = signature("s", b"{}");
/// assert!(signed("s", b"{}", &good));
/// assert!(!signed("other", b"{}", &good));
/// assert!(!signed("s", b"{ }", &good));
/// assert!(!signed("s", b"{}", ""));
/// assert!(!signed("s", b"{}", &good.replace("sha256=", "sha1=")));
/// ```
pub fn signed(secret: &str, body: &[u8], header: &str) -> bool {
    let Some(tag) = header.strip_prefix("sha256=").and_then(unhex) else {
        return false;
    };
    mac(secret, body).verify_slice(&tag).is_ok()
}

/// The HMAC-SHA256 of `body` with `secret`.
fn mac(secret: &str, body: &[u8]) -> Hmac<Sha256> {
    // HMAC takes a key of each length.
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("a key of each length");
    mac.update(body);
    mac
}

/// The bytes of the hex text `hex`, or `None`.
fn unhex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok())
        .collect()
}

/// The number of the wave of the milestone `title`: `Wave N`, or `Wave
/// N: NAME`.
///
/// ```
/// use riff_server::hooks::wave_number;
///
/// assert_eq!(wave_number("Wave 54: events from GitHub"), Some(54));
/// assert_eq!(wave_number("Wave 7"), Some(7));
/// assert_eq!(wave_number("Backlog"), None);
/// assert_eq!(wave_number("Wave 2x"), None);
/// ```
pub fn wave_number(title: &str) -> Option<u64> {
    let rest = title.strip_prefix("Wave ")?;
    rest.split(':').next()?.trim().parse().ok()
}

/// What one delivery names (#1655).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    /// The repository thread, `OWNER/NAME`.
    pub repo: ThreadName,
    pub kind: GitHubEvent,
    /// The number of each issue or pull request of the event: one, or
    /// each pull request of a check run or a workflow run. `None` for a
    /// commit with no pull request.
    pub numbers: Vec<Option<u64>>,
    /// The change of the plan, when the event is one.
    pub change: Option<GitHubChange>,
}

/// What the delivery of the kind `kind` with the body `payload` names.
/// `None` for a kind that riff does not use, and for an event with no
/// repository.
///
/// ```
/// use riff_core::wire::{GitHubChange, GitHubEvent};
/// use riff_server::hooks::named;
///
/// let repository = serde_json::json!({ "full_name": "acme/app" });
/// let merged = serde_json::json!({
///     "action": "closed", "number": 41, "repository": repository,
///     "pull_request": { "number": 41, "merged": true },
/// });
/// let event = named("pull_request", &merged).unwrap();
/// assert_eq!(event.repo.to_string(), "acme/app");
/// assert_eq!((event.kind, event.numbers), (GitHubEvent::PullRequest, vec![Some(41)]));
/// assert_eq!(event.change, Some(GitHubChange::Merged { number: 41 }));
///
/// let milestoned = serde_json::json!({
///     "action": "milestoned", "repository": repository,
///     "issue": { "number": 12 },
///     "milestone": { "title": "Wave 54: events", "state": "open" },
/// });
/// let change = named("issues", &milestoned).unwrap().change;
/// assert_eq!(change, Some(GitHubChange::NewItem { number: 12, wave: "Wave 54: events".into() }));
///
/// let status = serde_json::json!({ "sha": "abc", "repository": repository });
/// assert_eq!(named("status", &status).unwrap().numbers, [None]);
/// assert!(named("ping", &status).is_none());
/// assert!(named("status", &serde_json::json!({})).is_none());
/// ```
pub fn named(kind: &str, payload: &Value) -> Option<Named> {
    let kind = GitHubEvent::parse(kind)?;
    let repo = payload["repository"]["full_name"].as_str()?.parse().ok()?;
    let number = |value: &Value| value.as_u64();
    let action = payload["action"].as_str().unwrap_or_default();
    let pulls = |runs: &Value| -> Vec<Option<u64>> {
        let numbers: Vec<Option<u64>> = runs["pull_requests"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|pull| number(&pull["number"]))
            .map(Some)
            .collect();
        if numbers.is_empty() {
            vec![None]
        } else {
            numbers
        }
    };
    let (numbers, change) = match kind {
        GitHubEvent::Issues => {
            let issue = number(&payload["issue"]["number"]);
            let milestone = &payload["milestone"];
            let title = milestone["title"].as_str().unwrap_or_default();
            let new_item = action == "milestoned"
                && milestone["state"] == "open"
                && wave_number(title).is_some();
            let change = issue
                .filter(|_| new_item)
                .map(|number| GitHubChange::NewItem {
                    number,
                    wave: title.to_owned(),
                });
            (vec![issue], change)
        }
        GitHubEvent::IssueComment => (vec![number(&payload["issue"]["number"])], None),
        GitHubEvent::PullRequest => {
            let pull = &payload["pull_request"];
            let pull_number = number(&pull["number"]).or_else(|| number(&payload["number"]));
            let merged = action == "closed" && pull["merged"] == true;
            let change = pull_number
                .filter(|_| merged)
                .map(|number| GitHubChange::Merged { number });
            (vec![pull_number], change)
        }
        GitHubEvent::CheckRun => (pulls(&payload["check_run"]), None),
        GitHubEvent::WorkflowRun => (pulls(&payload["workflow_run"]), None),
        GitHubEvent::Status => (vec![None], None),
        GitHubEvent::MergeGroup => {
            let head = payload["merge_group"]["head_ref"]
                .as_str()
                .unwrap_or_default();
            (vec![merge_group_number(head)], None)
        }
    };
    Some(Named {
        repo,
        kind,
        numbers,
        change,
    })
}

/// The number of the pull request of the branch `head` of a merge
/// group: `refs/heads/gh-readonly-queue/BASE/pr-N-SHA`.
///
/// ```
/// use riff_server::hooks::merge_group_number;
///
/// assert_eq!(merge_group_number("refs/heads/gh-readonly-queue/main/pr-123-f00d"), Some(123));
/// assert_eq!(merge_group_number("refs/heads/main"), None);
/// ```
pub fn merge_group_number(head: &str) -> Option<u64> {
    let (_, last) = head.rsplit_once('/')?;
    let rest = last.strip_prefix("pr-")?;
    rest.split('-').next()?.parse().ok()
}

/// The IDs of the deliveries of the last [`DELIVERY_LIFE`], at most
/// [`DELIVERIES_MOST`].
///
/// ```
/// use std::time::{Duration, Instant};
/// use riff_server::hooks::{DELIVERY_LIFE, Deliveries};
///
/// let now = Instant::now();
/// let mut deliveries = Deliveries::default();
/// assert!(deliveries.take("72d3162e", now));
/// assert!(!deliveries.take("72d3162e", now), "a repeat");
/// assert!(deliveries.take("72d3162f", now));
/// let later = now + DELIVERY_LIFE + Duration::from_secs(1);
/// assert!(deliveries.take("72d3162e", later), "riff-server forgot it");
/// ```
#[derive(Debug)]
pub struct Deliveries {
    life: Duration,
    most: usize,
    order: VecDeque<(String, Instant)>,
    seen: HashSet<String>,
}

impl Default for Deliveries {
    fn default() -> Self {
        Deliveries::new(DELIVERY_LIFE, DELIVERIES_MOST)
    }
}

impl Deliveries {
    /// The IDs of the last `life`, at most `most`.
    pub fn new(life: Duration, most: usize) -> Self {
        Deliveries {
            life,
            most,
            order: VecDeque::new(),
            seen: HashSet::new(),
        }
    }

    /// True when the delivery `id` comes for the first time: riff-server
    /// keeps it.
    pub fn take(&mut self, id: &str, now: Instant) -> bool {
        while let Some((old, at)) = self.order.front() {
            if now.duration_since(*at) <= self.life && self.order.len() < self.most {
                break;
            }
            self.seen.remove(old);
            self.order.pop_front();
        }
        if !self.seen.insert(id.to_owned()) {
            return false;
        }
        self.order.push_back((id.to_owned(), now));
        true
    }
}

/// The key of a window and of its wake: the repository and the number.
pub type Key = (ThreadName, Option<u64>);

/// One open window of the debounce.
#[derive(Debug)]
struct Window {
    kinds: Vec<GitHubEvent>,
    events: u32,
}

/// The webhook of a riff-server: the deliveries, the windows of the
/// debounce, the queue of the wakes, and the counts. See the module docs.
#[derive(Debug)]
pub struct Hooks {
    debounce: Duration,
    /// The start of this instance, in milliseconds since the Unix epoch.
    since_ms: u64,
    inner: Mutex<Inner>,
    wakes: broadcast::Sender<GitHubWake>,
}

#[derive(Debug, Default)]
struct Inner {
    deliveries: Deliveries,
    windows: HashMap<Key, Window>,
    queues: HashMap<ThreadName, VecDeque<(GitHubWake, Instant)>>,
    /// The ID of the last wake.
    last_id: u64,
    taken: u64,
    repeated: u64,
    refused: u64,
    /// The time of the last warning of a refused delivery.
    refused_logged: Option<Instant>,
}

impl Hooks {
    /// The webhook of an instance that started at `since_ms`, with a
    /// window of `debounce`.
    pub fn new(debounce: Duration, since_ms: u64) -> Self {
        Hooks {
            debounce,
            since_ms,
            inner: Mutex::new(Inner::default()),
            wakes: broadcast::channel(WAKE_BUFFER).0,
        }
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        // A panic while the lock is held leaves plain data behind; keep going.
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// The window of the debounce.
    pub fn debounce(&self) -> Duration {
        self.debounce
    }

    /// A stream of each new wake, for the `github/watch` streams.
    pub fn subscribe(&self) -> broadcast::Receiver<GitHubWake> {
        self.wakes.subscribe()
    }

    /// True when the delivery `id` comes for the first time. A delivery
    /// with no ID always counts as new.
    pub fn delivery(&self, id: Option<&str>, now: Instant) -> bool {
        let mut inner = self.inner();
        let first = id.is_none_or(|id| inner.deliveries.take(id, now));
        if first {
            inner.taken += 1;
        } else {
            inner.repeated += 1;
        }
        first
    }

    /// Counts a refused delivery. True when riff-server logs a warning
    /// of it: the first one, then at most one each
    /// [`REFUSED_LOG_EVERY`].
    pub fn refused(&self, now: Instant) -> bool {
        let mut inner = self.inner();
        inner.refused += 1;
        let log = inner
            .refused_logged
            .is_none_or(|at| now.duration_since(at) >= REFUSED_LOG_EVERY);
        if log {
            inner.refused_logged = Some(now);
        }
        log
    }

    /// Adds the event `named` to the window of each of its numbers. It
    /// gives the key of each window that the event opened: the caller
    /// calls [`Hooks::flush`] for it after [`Hooks::debounce`].
    pub fn add(&self, named: &Named) -> Vec<Key> {
        let mut inner = self.inner();
        let mut opened = Vec::new();
        for number in &named.numbers {
            let key = (named.repo.clone(), *number);
            let window = inner.windows.entry(key.clone()).or_insert_with(|| {
                opened.push(key);
                Window {
                    kinds: Vec::new(),
                    events: 0,
                }
            });
            if !window.kinds.contains(&named.kind) {
                window.kinds.push(named.kind);
            }
            window.events += 1;
        }
        opened
    }

    /// Ends the window of `key`: its wake goes into the queue and to each
    /// stream. `None` when the window is not open.
    pub fn flush(&self, key: &Key, now: Instant, now_ms: u64) -> Option<GitHubWake> {
        let mut inner = self.inner();
        let window = inner.windows.remove(key)?;
        let id = now_ms.max(inner.last_id + 1);
        inner.last_id = id;
        let wake = GitHubWake {
            id,
            repo: key.0.clone(),
            kinds: window.kinds,
            number: key.1,
            events: window.events,
        };
        let queue = inner.queues.entry(key.0.clone()).or_default();
        queue.push_back((wake.clone(), now));
        while queue.len() > QUEUE_MOST {
            queue.pop_front();
        }
        inner.forget(now);
        // A send fails only when nobody listens. That is not an error.
        let _ = self.wakes.send(wake.clone());
        Some(wake)
    }

    /// Each wake of the queue of `repo` with an ID after `after`, the
    /// oldest first.
    pub fn queued(&self, repo: &ThreadName, after: u64, now: Instant) -> Vec<GitHubWake> {
        let mut inner = self.inner();
        inner.forget(now);
        inner
            .queues
            .get(repo)
            .into_iter()
            .flatten()
            .filter(|(wake, _)| wake.id > after)
            .map(|(wake, _)| wake.clone())
            .collect()
    }

    /// The last [`EVENTS_SHOWN`] wakes of `repo`, and the counts.
    pub fn events(&self, repo: &ThreadName, now: Instant) -> GitHubEventsReply {
        let mut wakes = self.queued(repo, 0, now);
        let skip = wakes.len().saturating_sub(EVENTS_SHOWN);
        wakes.drain(..skip);
        let inner = self.inner();
        GitHubEventsReply {
            wakes,
            taken: inner.taken,
            repeated: inner.repeated,
            refused: inner.refused,
            since_ms: self.since_ms,
        }
    }
}

impl Inner {
    /// Forgets each wake older than [`QUEUE_LIFE`].
    fn forget(&mut self, now: Instant) {
        for queue in self.queues.values_mut() {
            while queue
                .front()
                .is_some_and(|(_, at)| now.duration_since(*at) > QUEUE_LIFE)
            {
                queue.pop_front();
            }
        }
        self.queues.retain(|_, queue| !queue.is_empty());
    }
}

/// `POST /hooks/github`: one delivery of the webhook of the session App
/// (#1655). See the module docs. The reply is for GitHub: it shows the
/// status and the text on the page of the delivery.
pub(crate) async fn hook(
    AxumState(s): AxumState<crate::Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, &'static str) {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let kind = header(EVENT_HEADER).unwrap_or_default().to_owned();
    let delivery = header(DELIVERY_HEADER).map(str::to_owned);
    let apps = match crate::github::apps_of(&s).await {
        Ok(apps) => apps,
        Err(e) => {
            tracing::warn!(
                "a delivery of the webhook, and the keys of the GitHub Apps did not read: {e}"
            );
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "riff-server cannot read the webhook secret",
            );
        }
    };
    let secret = apps
        .of(GitHubApp::Session)
        .and_then(|keys| keys.webhook_secret.as_deref());
    let signature = header(SIGNATURE_HEADER).unwrap_or_default();
    if !secret.is_some_and(|secret| signed(secret, &body, signature)) {
        if s.hooks.refused(Instant::now()) {
            let why = match (secret, signature.is_empty()) {
                (None, _) => "the riff has no webhook secret of the session App",
                (Some(_), true) => "the delivery has no signature",
                (Some(_), false) => "the signature does not check",
            };
            tracing::warn!(
                kind = %kind,
                delivery = delivery.as_deref().unwrap_or_default(),
                "riff-server refused a delivery of the webhook with 401: {why}. \
                 riff github events counts each refused delivery"
            );
        }
        return (
            StatusCode::UNAUTHORIZED,
            "the signature of the delivery does not check",
        );
    }
    if !s.hooks.delivery(delivery.as_deref(), Instant::now()) {
        tracing::info!(kind = %kind, delivery = delivery.as_deref().unwrap_or_default(), "a repeated delivery of the webhook: dropped");
        return (
            StatusCode::ACCEPTED,
            "a repeated delivery: riff-server dropped it",
        );
    }
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return (StatusCode::BAD_REQUEST, "the body is not JSON");
    };
    let Some(named) = named(&kind, &payload) else {
        return (
            StatusCode::ACCEPTED,
            "an event that riff does not use: riff-server dropped it",
        );
    };
    if let Some(change) = named.change.clone() {
        tracing::info!(repo = %named.repo, kind = %kind, "an event of GitHub changes the plan: {change:?}");
        s.engine.plan_changed_on_github(named.repo.clone(), change);
    }
    for key in s.hooks.add(&named) {
        let server = Arc::downgrade(&s);
        let wait = s.hooks.debounce();
        tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            let Some(s) = server.upgrade() else {
                return;
            };
            if let Some(wake) = s.hooks.flush(&key, Instant::now(), crate::now_ms()) {
                let kinds: Vec<&str> = wake.kinds.iter().map(|kind| kind.name()).collect();
                tracing::info!(
                    repo = %wake.repo,
                    number = wake.number,
                    kinds = kinds.join(" "),
                    events = wake.events,
                    "a wake of the GitHub App"
                );
            }
        });
    }
    (StatusCode::ACCEPTED, "riff-server took the event")
}

/// `POST /v1/github/events`: the last wakes of the GitHub App for one
/// repository, and the counts of the deliveries (#1655).
pub(crate) async fn events(
    AxumState(s): AxumState<crate::Shared>,
    Extension(_): Extension<SignedIn>,
    Json(r): Json<GitHubEvents>,
) -> crate::Reply<GitHubEventsReply> {
    let repo = r
        .repo
        .parse::<ThreadName>()
        .ok()
        .filter(|_| crate::github::valid_repo(&r.repo))
        .ok_or_else(|| {
            let why = format!("{:?} is no repository OWNER/NAME of GitHub", r.repo);
            (StatusCode::BAD_REQUEST, why)
        })?;
    Ok(Json(s.hooks.events(&repo, Instant::now())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_run(number: u64) -> Named {
        let payload = serde_json::json!({
            "action": "completed",
            "repository": { "full_name": "acme/app" },
            "check_run": { "pull_requests": [{ "number": number }] },
        });
        named("check_run", &payload).unwrap()
    }

    /// 01M74A37CFS9QMB0PCW7J7XS3B: 20 `check_run` events of one pull
    /// request in one window give one wake, with the count of the events.
    /// A pull request with its own window gets its own wake.
    #[test]
    fn twenty_check_runs_of_one_pull_request_give_one_wake() {
        let hooks = Hooks::new(DEBOUNCE, 0);
        let mut opened = Vec::new();
        for _ in 0..20 {
            opened.extend(hooks.add(&check_run(12)));
        }
        opened.extend(hooks.add(&check_run(13)));
        assert_eq!(opened.len(), 2, "{opened:?}");
        let now = Instant::now();
        let wakes: Vec<GitHubWake> = opened
            .iter()
            .filter_map(|key| hooks.flush(key, now, 5_000))
            .collect();
        assert_eq!(wakes[0].number, Some(12));
        assert_eq!(wakes[0].events, 20);
        assert_eq!(wakes[1].number, Some(13));
        assert!(wakes[1].id > wakes[0].id, "each ID is new");
        assert_eq!(
            hooks.flush(&opened[0], now, 5_000),
            None,
            "the window ended"
        );
        let after = hooks.add(&check_run(12));
        assert_eq!(
            after.len(),
            1,
            "a new event after the window opens the next one"
        );
    }

    /// 01M74A37FPGKYMHP0W588A88RJ: the queue gives each wake after the ID
    /// of the last one that a daemon got, and forgets each wake older than
    /// [`QUEUE_LIFE`].
    #[test]
    fn the_queue_gives_each_wake_after_an_id_and_forgets_old_ones() {
        let hooks = Hooks::new(DEBOUNCE, 0);
        let repo: ThreadName = "acme/app".parse().unwrap();
        let now = Instant::now();
        let mut ids = Vec::new();
        for number in [1, 2, 3] {
            let key = hooks.add(&check_run(number)).remove(0);
            ids.push(hooks.flush(&key, now, 9_000).unwrap().id);
        }
        let after: Vec<Option<u64>> = hooks
            .queued(&repo, ids[0], now)
            .iter()
            .map(|wake| wake.number)
            .collect();
        assert_eq!(after, [Some(2), Some(3)]);
        let other: ThreadName = "acme/web".parse().unwrap();
        assert!(hooks.queued(&other, 0, now).is_empty());
        let later = now + QUEUE_LIFE + Duration::from_secs(1);
        assert!(hooks.queued(&repo, 0, later).is_empty());
    }

    /// 01M74A37FPGKYMHP0W588A88RJ, 01M74A37NMGDEHAD01SJ0DQT9B: the queue
    /// keeps at most [`QUEUE_MOST`] wakes of a repository, and `events`
    /// gives the last [`EVENTS_SHOWN`].
    #[test]
    fn the_queue_keeps_the_newest_wakes() {
        let hooks = Hooks::new(DEBOUNCE, 7);
        let repo: ThreadName = "acme/app".parse().unwrap();
        let now = Instant::now();
        for number in 0..(QUEUE_MOST as u64 + 5) {
            let key = hooks.add(&check_run(number)).remove(0);
            hooks.flush(&key, now, 1).unwrap();
        }
        let queued = hooks.queued(&repo, 0, now);
        assert_eq!(queued.len(), QUEUE_MOST);
        assert_eq!(queued[0].number, Some(5));
        let events = hooks.events(&repo, now);
        assert_eq!(events.wakes.len(), EVENTS_SHOWN);
        assert_eq!(events.wakes.last(), queued.last());
        assert_eq!(events.since_ms, 7);
    }

    /// 01M74A2TT5N3BZPCT04XSHNDCV, 01M74A2TWZBEJF4HCPXJSDQ2Y0: the counts
    /// of the deliveries, and the warning of a refused one at most each
    /// [`REFUSED_LOG_EVERY`].
    #[test]
    fn the_counts_of_the_deliveries() {
        let hooks = Hooks::new(DEBOUNCE, 0);
        let now = Instant::now();
        assert!(hooks.delivery(Some("d1"), now));
        assert!(!hooks.delivery(Some("d1"), now));
        assert!(hooks.delivery(None, now));
        assert!(hooks.refused(now));
        assert!(!hooks.refused(now + Duration::from_secs(1)));
        assert!(hooks.refused(now + REFUSED_LOG_EVERY));
        let repo: ThreadName = "acme/app".parse().unwrap();
        let events = hooks.events(&repo, now);
        assert_eq!((events.taken, events.repeated, events.refused), (2, 1, 3));
    }

    /// 01M74A379EQQXW6G86WVFRC1V0, 01M74A37JNKW1R2WRXAW520WJM: what each
    /// kind names: the numbers of a check run, a workflow run and a merge
    /// group, an issue comment, and no change of the plan for an issue that
    /// goes to a closed milestone, to a milestone that is no wave, or for a
    /// pull request closed with no merge.
    #[test]
    fn what_each_kind_names() {
        let repository = serde_json::json!({ "full_name": "acme/app" });
        let check = serde_json::json!({
            "repository": repository,
            "check_run": { "pull_requests": [{ "number": 3 }, { "number": 4 }] },
        });
        assert_eq!(
            named("check_run", &check).unwrap().numbers,
            [Some(3), Some(4)]
        );
        let main =
            serde_json::json!({ "repository": repository, "check_run": { "pull_requests": [] } });
        assert_eq!(named("check_run", &main).unwrap().numbers, [None]);
        let run = serde_json::json!({
            "repository": repository,
            "workflow_run": { "pull_requests": [{ "number": 5 }] },
        });
        assert_eq!(named("workflow_run", &run).unwrap().numbers, [Some(5)]);
        let group = serde_json::json!({
            "repository": repository,
            "merge_group": { "head_ref": "refs/heads/gh-readonly-queue/main/pr-6-abc" },
        });
        assert_eq!(named("merge_group", &group).unwrap().numbers, [Some(6)]);
        let comment = serde_json::json!({ "repository": repository, "issue": { "number": 7 } });
        let comment = named("issue_comment", &comment).unwrap();
        assert_eq!((comment.numbers, comment.change), (vec![Some(7)], None));
        for (title, state) in [("Wave 54", "closed"), ("Backlog", "open")] {
            let issue = serde_json::json!({
                "action": "milestoned", "repository": repository, "issue": { "number": 8 },
                "milestone": { "title": title, "state": state },
            });
            assert_eq!(
                named("issues", &issue).unwrap().change,
                None,
                "{title} {state}"
            );
        }
        let closed = serde_json::json!({
            "action": "closed", "repository": repository,
            "pull_request": { "number": 9, "merged": false },
        });
        assert_eq!(named("pull_request", &closed).unwrap().change, None);
    }
}
