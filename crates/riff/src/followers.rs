//! The followers of each wave of the riff daemon on riff-server (#2085):
//! the stream of the wakes of the GitHub App ([`crate::hooks::follow`]),
//! the stream of the changes of the plan for the loop of the wave
//! ([`crate::wave::follow_plan`]), and the same stream for the lead loop
//! ([`crate::lead_host`]).
//!
//! # Design
//!
//! The start of a follower reads the user and the sign-in from the
//! keyring, and signs in ([`crate::wave::World::github_wakes`],
//! [`crate::wave::World::plan_changes`]). At a login with a security key,
//! the keyring is locked, so the start fails. [`follow`] then tries again
//! each [`crate::daemon::facts::every`], 30 seconds, until a start works
//! (01M7B2RQPTFVK02YAZGMN2SYAR). Then the follower follows its stream, and
//! connects again after each end of it.
//!
//! [`Followers`] keeps the failed starts of the followers of one wave:
//! the log gets one line at the first failure and one line at the start,
//! not one line for each try. While the starts fail, `riff daemon status`
//! names the follower under its wave, with the last cause
//! ([`Unfollowed`], 01M7B2RWBQWA6X8BNZEB025SWX). The wakes that came in
//! the meantime come at the start: the stream of the wakes opens after the
//! ID of the last wake ([`crate::hooks::Told::last`]).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant F as a follower of the wave
//!     participant K as the keyring
//!     participant S as riff-server
//!     F->>K: the user and the sign-in, with no prompt
//!     K-->>F: locked: one line in the log
//!     loop each 30 seconds while the keyring is locked
//!         F->>K: the user and the sign-in
//!         K-->>F: locked: no line
//!     end
//!     Note over K: the person unlocks it at the desktop
//!     F->>K: the user and the sign-in, at the next try
//!     K-->>F: the secrets
//!     F->>S: the stream: one line in the log
//!     S->>F: the wakes after the last one, and each new one
//! ```

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::text;
use crate::wave::BoxFuture;

/// A follower after its start: it follows its stream, and never ends.
pub type Follow<'a> = BoxFuture<'a, ()>;

/// A follower of a wave on riff-server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Follower {
    /// The wakes of the GitHub App ([`crate::hooks::follow`], #1656).
    Wakes,
    /// The changes of the plan, for the loop of the wave
    /// ([`crate::wave::follow_plan`], #1729).
    Plan,
    /// The changes of the plan, for the lead loop ([`crate::lead_host`],
    /// #1791).
    Lead,
}

/// A follower of a wave that does not run: its starts failed (#2085).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unfollowed {
    /// The follower.
    pub follower: Follower,
    /// The first failed start, in milliseconds since the epoch.
    pub since_ms: i64,
    /// The cause of the last failed start, in one line.
    pub cause: String,
    /// The failed starts.
    pub count: u64,
    /// The time between two tries, in seconds.
    pub every_secs: u64,
}

/// The failed starts of the followers of one wave (#2085,
/// 01M7B2RQPTFVK02YAZGMN2SYAR, 01M7B2RWBQWA6X8BNZEB025SWX). [`follow`]
/// keeps each failure ([`Followers::failed`]), and forgets them at the
/// start ([`Followers::started`]) and at the end of its task. `riff daemon
/// status` shows them under the wave
/// ([`crate::daemon::WaveShown::unfollowed`]).
///
/// The first failure gives one line for the log, with its cause. The next
/// failures give no line, and keep the newest cause. The start gives one
/// line after a failure:
///
/// ```
/// use std::time::Duration;
/// use riff::followers::{Follower, Followers};
///
/// let followers = Followers::default();
/// let every = Duration::from_secs(30);
/// let locked = "riff cannot find your user.\nUnlock the keyring";
/// assert_eq!(
///     followers.failed(Follower::Wakes, 1_000, locked, every).as_deref(),
///     Some("cannot follow the wakes of GitHub on riff-server: riff cannot find your user. \
///           Unlock the keyring. riff tries again each 30s.")
/// );
/// assert_eq!(followers.failed(Follower::Wakes, 31_000, "cannot read the key", every), None);
/// assert_eq!(followers.failed(Follower::Wakes, 61_000, "the keyring is locked", every), None);
/// // The followers of one wave fail each on its own.
/// assert!(followers.failed(Follower::Plan, 2_000, "the keyring is locked", every).is_some());
/// let shown = followers.shown();
/// assert_eq!(shown[0].follower, Follower::Wakes);
/// assert_eq!((shown[0].since_ms, shown[0].count), (1_000, 3));
/// assert_eq!(shown[0].cause, "the keyring is locked");
/// assert_eq!(shown[1].follower, Follower::Plan);
///
/// assert_eq!(
///     followers.started(Follower::Wakes, 91_000).as_deref(),
///     Some("follows the wakes of GitHub on riff-server again, after 3 failed starts in 1m.")
/// );
/// assert_eq!(followers.shown().len(), 1);
/// // A start with no failure before it gives no line.
/// assert_eq!(followers.started(Follower::Wakes, 92_000), None);
/// followers.forget(Follower::Plan);
/// assert!(followers.shown().is_empty());
/// ```
#[derive(Debug, Clone, Default)]
pub struct Followers(Arc<Mutex<BTreeMap<Follower, Unfollowed>>>);

impl Followers {
    /// Keeps a start of `follower` that failed at the time `now_ms` with
    /// `cause`, and gives the line for the log at the first failure. The
    /// next try comes after `every`.
    pub fn failed(
        &self,
        follower: Follower,
        now_ms: i64,
        cause: &str,
        every: Duration,
    ) -> Option<String> {
        let cause = cause.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut all = self.lock();
        if let Some(unfollowed) = all.get_mut(&follower) {
            unfollowed.cause = cause;
            unfollowed.count += 1;
            return None;
        }
        let line = text::follow_failed(follower, &cause, every.as_secs());
        let unfollowed = Unfollowed {
            follower,
            since_ms: now_ms,
            cause,
            count: 1,
            every_secs: every.as_secs(),
        };
        all.insert(follower, unfollowed);
        Some(line)
    }

    /// `follower` started at the time `now_ms`: it forgets the failures,
    /// and gives the line for the log when a start failed before.
    pub fn started(&self, follower: Follower, now_ms: i64) -> Option<String> {
        let failed = self.lock().remove(&follower)?;
        let secs = u64::try_from((now_ms - failed.since_ms) / 1000).unwrap_or(0);
        Some(text::follows_again(follower, failed.count, secs))
    }

    /// Forgets the failures of `follower`: its task ended.
    pub fn forget(&self, follower: Follower) {
        self.lock().remove(&follower);
    }

    /// The failures of each follower, in the order of [`Follower`].
    pub fn shown(&self) -> Vec<Unfollowed> {
        self.lock().values().cloned().collect()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<Follower, Unfollowed>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Starts the follower `follower` of the wave of `repo` with `start`, and
/// follows until the follower ends: never, in the daemon (#2085,
/// 01M7B2RQPTFVK02YAZGMN2SYAR). A start that fails, for example on a
/// locked keyring, goes in `followers`, and the next try comes after
/// `every`, until a start works. Each line for the log comes from
/// `followers`, and the line of a failure goes to stderr in one write
/// ([`crate::daemon::log::line`]). A task that ends while the starts fail
/// forgets them.
///
/// ```
/// use std::sync::atomic::{AtomicU32, Ordering};
/// use std::time::Duration;
/// use riff::followers::{Follow, Follower, Followers, follow};
/// use riff::wave::BoxFuture;
///
/// let tries = AtomicU32::new(0);
/// let start = || -> BoxFuture<'_, anyhow::Result<Follow<'_>>> {
///     Box::pin(async {
///         if tries.fetch_add(1, Ordering::SeqCst) < 2 {
///             anyhow::bail!("the keyring is locked");
///         }
///         Ok(Box::pin(async {}) as Follow<'_>)
///     })
/// };
/// let followers = Followers::default();
/// let every = Duration::from_millis(1);
/// let runtime = tokio::runtime::Builder::new_current_thread().enable_time().build()?;
/// runtime.block_on(follow("o/r", Follower::Wakes, &followers, every, start));
/// // Two starts failed, and the third one followed.
/// assert_eq!(tries.load(Ordering::SeqCst), 3);
/// assert!(followers.shown().is_empty());
/// # Ok::<(), std::io::Error>(())
/// ```
pub async fn follow<'a, S>(
    repo: &str,
    follower: Follower,
    followers: &Followers,
    every: Duration,
    start: S,
) where
    S: Fn() -> BoxFuture<'a, Result<Follow<'a>>>,
{
    let _forget = Forget(followers, follower);
    let follow = loop {
        match start().await {
            Ok(follow) => {
                if let Some(line) = followers.started(follower, crate::daemon::now_ms()) {
                    println!("riff daemon: {repo}: {line}");
                }
                break follow;
            }
            Err(e) => {
                let cause = format!("{e:#}");
                let now = crate::daemon::now_ms();
                if let Some(line) = followers.failed(follower, now, &cause, every) {
                    let line = format!("riff daemon: {repo}: {line}");
                    crate::daemon::log::line(&mut std::io::stderr(), &line);
                }
            }
        }
        tokio::time::sleep(every).await;
    };
    follow.await;
}

/// Forgets the failed starts of a follower when its task ends.
struct Forget<'a>(&'a Followers, Follower);

impl Drop for Forget<'_> {
    fn drop(&mut self) {
        self.0.forget(self.1);
    }
}
