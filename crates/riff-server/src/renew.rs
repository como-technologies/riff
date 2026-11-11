//! The new rights of an install of a GitHub App (#1906): the tokens that
//! riff-server gave, and the renew of each token of an install.
//!
//! # Design
//!
//! GitHub makes a token of an install with the rights that the install
//! has at that time, and the token keeps them up to its end, one hour
//! later. So after the owner of an account accepts the new rights of an
//! App, each token of that install still lacks them. riff-server keeps
//! each token that it gave to a session and to a daemon
//! ([`Renews::keep`]): its install, its role and its rights, in memory,
//! up to its end.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant G as GitHub
//!     participant S as riff-server
//!     participant D as riff daemon
//!     participant C as the container
//!     D->>S: GET /v1/github/renew (as the person)
//!     G->>S: POST /hooks/github: installation, new_permissions_accepted
//!     S-->>D: renew: the install 2020
//!     D->>S: POST /v1/github/token for each session of the install 2020
//!     S->>G: a token with the new rights
//!     D->>C: FOLDER/gh/hosts.yml: the new token
//!     Note over S: REVOKE_AFTER later
//!     S->>G: DELETE /installation/token: each older token of the install 2020
//! ```
//!
//! - **The event** (01M74MB94Q4VPK724W7H0PZH9P). Each App of the riff has
//!   the webhook `/hooks/github`, and GitHub sends the event
//!   `installation` to each App with a webhook. With the action
//!   `new_permissions_accepted`, it names the install whose owner
//!   accepted the new rights. [`crate::hooks`] renews that install
//!   ([`crate::github`]).
//! - **A token with other rights** (01M74MB9786AE4YVEG10FNGDT5). A lead
//!   App that riff made before the webhook of the lead App sends no event.
//!   So riff-server compares each new token with the rights that GitHub
//!   asked for, with each kept token of the same install and the same
//!   role ([`Renews::changed`]): `riff github check`, `riff github
//!   install`, and each token of a session or a daemon. A token with other
//!   rights renews the install too.
//! - **The renew** (01M74MB92717A21MG97BF7SDP3). riff-server takes each
//!   kept token of the install out ([`Renews::take`]), and sends one
//!   [`GitHubRenew`] to each stream `GET /v1/github/renew`
//!   ([`Renews::send`]). The daemon of each host asks at once for a new
//!   token of each session and each write of the install. After
//!   [`REVOKE_AFTER`], riff-server revokes each token that it took out:
//!   the daemon wrote the new token first.
//! - **The memory**. A new instance of riff-server keeps no token of the
//!   old one. Those tokens end by themselves within the hour, and the
//!   event of GitHub still goes to each daemon.
//!
//! # Example
//!
//! ```
//! use std::time::Instant;
//! use riff_core::wire::{GitHubRenew, GitHubRole};
//! use riff_server::renew::Renews;
//!
//! let renews = Renews::default();
//! let mut stream = renews.subscribe();
//! let now = Instant::now();
//! let old = [("contents".to_owned(), "write".to_owned())].into();
//! renews.keep(2020, GitHubRole::Lead, &old, "ghs_old", now);
//! let new = [
//!     ("contents".to_owned(), "write".to_owned()),
//!     ("security_events".to_owned(), "write".to_owned()),
//! ].into();
//! assert!(renews.changed(2020, GitHubRole::Lead, &new, now), "the install has new rights");
//! assert!(!renews.changed(2020, GitHubRole::Lead, &old, now));
//! assert!(!renews.changed(1010, GitHubRole::Lead, &new, now), "another install");
//! assert_eq!(renews.take(2020, now), ["ghs_old"]);
//! assert!(renews.take(2020, now).is_empty());
//! renews.send(2020);
//! assert_eq!(stream.try_recv(), Ok(GitHubRenew { installation: 2020 }));
//! ```

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use riff_core::wire::{GitHubRenew, GitHubRole, Rights};
use tokio::sync::broadcast;

/// How long riff-server waits after the renew of an install before it
/// revokes each older token of the install: the daemons write the new
/// tokens first.
pub const REVOKE_AFTER: Duration = Duration::from_secs(60);

/// How long a token of an install lives on GitHub: riff-server forgets
/// a kept token after it.
pub const TOKEN_LIFE: Duration = Duration::from_secs(60 * 60);

/// Events that a slow stream may miss before it drops them.
const RENEW_BUFFER: usize = 64;

/// One token that riff-server gave. It has no `Debug`: no log shows the
/// token.
struct Kept {
    installation: u64,
    role: GitHubRole,
    rights: Rights,
    token: String,
    /// When riff-server gave it.
    at: Instant,
}

/// The tokens that riff-server gave to the sessions and to the daemons,
/// and the stream of the renews. See the module docs.
pub struct Renews {
    kept: Mutex<Vec<Kept>>,
    renews: broadcast::Sender<GitHubRenew>,
}

impl Default for Renews {
    fn default() -> Self {
        Renews {
            kept: Mutex::new(Vec::new()),
            renews: broadcast::channel(RENEW_BUFFER).0,
        }
    }
}

impl Renews {
    fn lock(&self) -> MutexGuard<'_, Vec<Kept>> {
        // A panic while the lock is held leaves plain data behind; keep going.
        self.kept
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Keeps the token `token` of the install `installation`, which GitHub
    /// gave for `role` with `rights` at `now`. It forgets each token older
    /// than [`TOKEN_LIFE`].
    pub fn keep(
        &self,
        installation: u64,
        role: GitHubRole,
        rights: &Rights,
        token: &str,
        now: Instant,
    ) {
        let mut kept = self.lock();
        kept.retain(|k| now.duration_since(k.at) < TOKEN_LIFE);
        kept.push(Kept {
            installation,
            role,
            rights: rights.clone(),
            token: token.to_owned(),
            at: now,
        });
    }

    /// True when a kept token of the install `installation` and of `role`
    /// has other rights than `rights`: the install has new rights.
    pub fn changed(
        &self,
        installation: u64,
        role: GitHubRole,
        rights: &Rights,
        now: Instant,
    ) -> bool {
        self.lock().iter().any(|k| {
            k.installation == installation
                && k.role == role
                && now.duration_since(k.at) < TOKEN_LIFE
                && k.rights != *rights
        })
    }

    /// Takes out each kept token of the install `installation`, and gives
    /// each one that has not ended: riff-server revokes them.
    pub fn take(&self, installation: u64, now: Instant) -> Vec<String> {
        let mut kept = self.lock();
        let (taken, left) = std::mem::take(&mut *kept)
            .into_iter()
            .partition::<Vec<Kept>, _>(|k| k.installation == installation);
        *kept = left;
        taken
            .into_iter()
            .filter(|k| now.duration_since(k.at) < TOKEN_LIFE)
            .map(|k| k.token)
            .collect()
    }

    /// A stream of each renew, for the `github/renew` streams.
    pub fn subscribe(&self) -> broadcast::Receiver<GitHubRenew> {
        self.renews.subscribe()
    }

    /// Sends the renew of the install `installation` to each stream.
    pub fn send(&self, installation: u64) {
        // A send fails only when nobody listens. That is not an error.
        let _ = self.renews.send(GitHubRenew { installation });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rights(pairs: &[(&str, &str)]) -> Rights {
        pairs
            .iter()
            .map(|(name, level)| ((*name).to_owned(), (*level).to_owned()))
            .collect()
    }

    /// 01M74MB9786AE4YVEG10FNGDT5: only a kept token of the same install
    /// and the same role counts, and a token after its end counts as none.
    #[test]
    fn only_a_live_token_of_the_same_install_and_role_counts() {
        let renews = Renews::default();
        let now = Instant::now();
        let fallback = rights(&[("contents", "write")]);
        let full = rights(&[("contents", "write"), ("security_events", "write")]);
        renews.keep(2020, GitHubRole::Lead, &fallback, "ghs_lead", now);
        renews.keep(1010, GitHubRole::Worker, &fallback, "ghs_worker", now);
        assert!(renews.changed(2020, GitHubRole::Lead, &full, now));
        assert!(
            !renews.changed(2020, GitHubRole::Worker, &full, now),
            "another role"
        );
        let later = now + TOKEN_LIFE;
        assert!(
            !renews.changed(2020, GitHubRole::Lead, &full, later),
            "the token ended"
        );
        assert!(
            renews.take(2020, later).is_empty(),
            "no revoke of an ended token"
        );
        assert_eq!(renews.take(1010, now), ["ghs_worker"]);
    }

    /// 01M74MB92717A21MG97BF7SDP3: a token goes out of the keep after its
    /// end, so the keep does not grow.
    #[test]
    fn the_keep_forgets_each_ended_token() {
        let renews = Renews::default();
        let now = Instant::now();
        let some = rights(&[("contents", "write")]);
        renews.keep(2020, GitHubRole::Lead, &some, "ghs_1", now);
        renews.keep(2020, GitHubRole::Lead, &some, "ghs_2", now + TOKEN_LIFE);
        assert_eq!(renews.lock().len(), 1);
        assert_eq!(renews.take(2020, now + TOKEN_LIFE), ["ghs_2"]);
    }
}
