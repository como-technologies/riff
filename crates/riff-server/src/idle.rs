//! Idle workers: the server stops the idle workers past a limit.
//!
//! # Design
//!
//! A lead starts workers when it has work for them. So idle workers do
//! not pile up, the server looks at the workers each [`CHECK_EVERY`]
//! ([`crate::state::State::stop_idle_workers`],
//! 01M3Q5A0NKY1FCS0YH6N6YD3GN). An idle worker is a live worker that is
//! not a lead, holds no claim, and made no call for a time. On each host
//! of each user, the server keeps the [`Idle::per_host`] idle workers
//! with the shortest idle time. It asks each other idle worker that made
//! no call for [`Idle::after_secs`] to stop.
//!
//! The ask is a mark on the session. The `riff mcp` of a worker sends a
//! keep-alive each [`riff_core::wire::WORKER_ALIVE_EVERY`]. The reply
//! carries the mark. Then `riff mcp` stops the `riff workers run`
//! wrapper of its worker, which stops `claude`, so the tmux pane closes.
//! `riff mcp` ends the session when its input closes: the session leaves
//! `riff who` (01M3Q5A0QZTSTXHHNYCE8HFJSB). This works on each machine,
//! with or without a workers host.
//!
//! The watch of a worker sends a keep-alive each
//! [`riff_core::wire::WORKER_ALIVE_EVERY`] too, and acts on the mark in
//! the same way (01M4385Z5BN03E6HTEB5GQVZ8X). So a worker whose `riff
//! mcp` ended, for example at a self-update, stops too.
//!
//! A call of the worker takes the mark back. The end of its watch at a
//! wake takes it back too. So a worker that claims work, or that the
//! lead wakes with a request, before its next keep-alive goes on.
//!
//! A worker with an unread request of its lead (a direct message that
//! starts with `request:`) is not idle: the server does not ask it to
//! stop (01M49KT28N4B07P4G80Z74GRAH). When the lead sends a request
//! after the ask, and the worker still runs, the note of [`stuck`]
//! names each unread request (01M49KT3JXZATXMA4WNTR9BJCK).
//!
//! The lead gets one note for the first ask after the last change of
//! the claims of the worker. A wake, for example the pause and the
//! resume of the repository, takes the mark back, and the server asks
//! again. It does not tell the lead again (01M4385Z039RCFSKWFPWZAETTX).
//! When the worker still shows life [`STOP_WAIT`] after the first ask,
//! while the mark holds, the lead gets one more note: why the server
//! could not stop it, and how to stop it ([`stuck`],
//! 01M4385Z2QAMEED30JYE81SMBY).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant S as riff-server
//!     participant M as riff mcp of the worker
//!     participant W as riff workers run
//!     participant C as claude
//!     participant L as lead
//!     S->>S: each 5 s: mark each idle worker past the limit
//!     S->>L: note: stops the idle worker
//!     M->>S: keep-alive
//!     S-->>M: stop
//!     M->>W: SIGTERM
//!     W->>C: SIGTERM
//!     C-->>M: input closes
//!     M->>S: end
//!     opt still alive 60 s after the ask
//!         S->>L: note: the worker still runs
//!     end
//! ```
//!
//! The owner or an admin sets [`Idle`] with `riff workers idle`
//! (01M3Q5A0TF9K49V8Z1ZY9NDF74). The lead gets one note for each worker
//! that the server asks to stop ([`news`], 01M3Q5A0WRQT4SGPSD0CQFF011),
//! at the first ask.

use std::time::Duration;

use riff_core::wire::Idle;

use crate::state::{Stopping, Stuck};

/// How often the server looks for idle workers.
pub const CHECK_EVERY: Duration = Duration::from_secs(5);

/// How long a worker can show life after the ask to stop before the
/// server tells the lead that it still runs (01M4385Z2QAMEED30JYE81SMBY).
pub const STOP_WAIT: Duration = Duration::from_secs(60);

/// The note to the lead for a worker that the server asks to stop
/// (01M3Q5A0WRQT4SGPSD0CQFF011).
///
/// ```
/// use std::time::Duration;
/// use riff_core::wire::Idle;
/// use riff_server::idle::news;
/// use riff_server::state::Stopping;
///
/// let stopping = Stopping {
///     worker: "riff://mike@pangolin/como-technologies/riff?session=1a2b3c4d5e".parse()?,
///     host: "pangolin".into(),
///     idle: Duration::from_secs(75),
///     first: true,
/// };
/// assert_eq!(
///     news(&stopping, &Idle::default()),
///     "workers: the server stops the idle worker 1a2b3c4d on pangolin. \
///      It made no call for 75 seconds. At most 1 idle worker stays on each host."
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn news(stopping: &Stopping, idle: &Idle) -> String {
    let id = stopping.worker.who().session().unwrap_or_default();
    let short: String = id.chars().take(8).collect();
    let stays = match idle.per_host {
        1 => "1 idle worker stays".to_owned(),
        n => format!("{n} idle workers stay"),
    };
    format!(
        "workers: the server stops the idle worker {short} on {}. It made no call for {} \
         seconds. At most {stays} on each host.",
        stopping.host,
        stopping.idle.as_secs()
    )
}

/// The note to the lead for a worker that still shows life
/// [`STOP_WAIT`] after the ask to stop (01M4385Z2QAMEED30JYE81SMBY).
///
/// ```
/// use std::time::Duration;
/// use riff_server::idle::stuck;
/// use riff_server::state::Stuck;
///
/// let stuck_worker = Stuck {
///     worker: "riff://mike@pangolin/como-technologies/riff?session=1a2b3c4d5e".parse()?,
///     host: "pangolin".into(),
///     asked: Duration::from_secs(65),
///     requests: Vec::new(),
/// };
/// assert_eq!(
///     stuck(&stuck_worker),
///     "workers: the idle worker 1a2b3c4d on pangolin still runs 65 seconds after \
///      the ask to stop. Its riff mcp and its watch did not stop its riff workers run: \
///      for example, its riff mcp ended and its watch is of an older riff, or no riff \
///      workers run wraps it. Stop it on pangolin: riff workers stop 1a2b3c4d"
/// );
///
/// // The note names each unread request of the lead (01M49KT3JXZATXMA4WNTR9BJCK).
/// let with_requests = Stuck {
///     requests: vec!["request: claim issue-12".into(), "request: claim verify-issue-9".into()],
///     ..stuck_worker
/// };
/// assert!(stuck(&with_requests).ends_with(
///     "riff workers stop 1a2b3c4d. It did not read 2 requests of the lead: \
///      \"request: claim issue-12\", \"request: claim verify-issue-9\". Give them to \
///      another session."
/// ));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn stuck(stuck: &Stuck) -> String {
    let id = stuck.worker.who().session().unwrap_or_default();
    let short: String = id.chars().take(8).collect();
    let mut news = format!(
        "workers: the idle worker {short} on {host} still runs {secs} seconds after the ask \
         to stop. Its riff mcp and its watch did not stop its riff workers run: for \
         example, its riff mcp ended and its watch is of an older riff, or no riff workers \
         run wraps it. Stop it on {host}: riff workers stop {short}",
        host = stuck.host,
        secs = stuck.asked.as_secs()
    );
    let requests: Vec<String> = stuck.requests.iter().map(|r| format!("{r:?}")).collect();
    match requests.len() {
        0 => {}
        1 => news.push_str(&format!(
            ". It did not read 1 request of the lead: {}. Give it to another session.",
            requests[0]
        )),
        n => news.push_str(&format!(
            ". It did not read {n} requests of the lead: {}. Give them to another session.",
            requests.join(", ")
        )),
    }
    news
}
