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
//! A call of the worker takes the mark back. The end of its watch at a
//! wake takes it back too. So a worker that claims work, or that the
//! lead wakes with a request, before its next keep-alive goes on.
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
//! ```
//!
//! The owner or an admin sets [`Idle`] with `riff workers idle`
//! (01M3Q5A0TF9K49V8Z1ZY9NDF74). The lead gets one note for each worker
//! that the server asks to stop ([`news`], 01M3Q5A0WRQT4SGPSD0CQFF011).

use std::time::Duration;

use riff_core::wire::Idle;

use crate::state::Stopping;

/// How often the server looks for idle workers.
pub const CHECK_EVERY: Duration = Duration::from_secs(5);

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
