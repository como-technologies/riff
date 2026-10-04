//! The state that a checkpoint keeps.
//!
//! A [`Snapshot`] has the position, and one part for each part of the
//! state. Each part is in the file of its group, and gives its fields
//! to the one JSON object of the snapshot:
//!
//! | Part | Fields |
//! |---|---|
//! | `the_riff::Saved` | `riff`, `idle` |
//! | `threads::Saved` | `threads` |
//! | `work::Saved` | `claims`, `leads` |
//! | `sessions::Saved` | `sessions` |
//! | `presence::Saved` | `cursors`, `statuses` |
//! | `people::Saved` | `riff_id`, `users`, `members`, `admins`, `owner`, `no_owner`, `owner_asked`, `signins_ended` |
//!
//! A group that adds a part to the state adds a field to its own
//! `Saved`, with a default (01M3T4111PFM0C6KPREWFS9EQQ). This file does
//! not change.

use std::collections::BTreeMap;

use riff_core::name::Who;
use serde::{Deserialize, Serialize};

use super::riff::Riff;
use super::{people, presence, sessions, the_riff, threads, work};

/// The state that a checkpoint keeps: the state that the log gives up to
/// [`Snapshot::position`], the read cursors, and the last call of each
/// session. See [`crate::checkpoint`]. A new field has a default, as in
/// a record (01M3T4111PFM0C6KPREWFS9EQQ).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The position of the last record in the state.
    pub position: u64,
    #[serde(flatten)]
    the_riff: the_riff::Saved,
    #[serde(flatten)]
    threads: threads::Saved,
    #[serde(flatten)]
    work: work::Saved,
    #[serde(flatten)]
    sessions: sessions::Saved,
    #[serde(flatten)]
    presence: presence::Saved,
    #[serde(flatten)]
    people: people::Saved,
}

/// The proof that a call comes from the load of a checkpoint. Only this
/// file makes one, so only [`Snapshot::into_parts`] makes a riff with
/// no `apply` (`Riff::restore`).
pub(super) struct LoadPath(());

impl Snapshot {
    /// The snapshot of `riff` and the read cursors of `presence`, at
    /// `position`. `seen` gives the last call of a session, or 0.
    pub(super) fn new(
        position: u64,
        riff: &Riff,
        presence: &presence::Presence,
        seen: impl Fn(&Who) -> u64,
    ) -> Snapshot {
        Snapshot {
            position,
            the_riff: riff.the_riff().saved(),
            threads: riff.threads().saved(),
            work: riff.work().saved(),
            sessions: riff.sessions().saved(seen),
            presence: presence.saved(),
            people: riff.people().saved(),
        }
    }

    /// The state that the log gives, the part of the presence, and the
    /// last call of each session.
    pub(super) fn into_parts(self) -> (Riff, presence::Saved, BTreeMap<Who, u64>) {
        let (sessions, seen) = self.sessions.restore();
        let riff = Riff::restore(
            LoadPath(()),
            self.position,
            sessions,
            self.threads.restore(),
            self.work.restore(),
            self.the_riff.restore(),
            self.people.restore(),
        );
        (riff, self.presence, seen)
    }
}
