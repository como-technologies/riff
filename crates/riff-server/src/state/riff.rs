//! The riff: the state that the log gives, and [`apply`].
//!
//! A [`Riff`] has one part for each group of commands. Each part is in
//! the file of its group, with its `apply` arms and its part of the
//! checkpoint:
//!
//! | Part | File | What it holds |
//! |---|---|---|
//! | [`Sessions`] | [`super::sessions`] | Each session that a record names. |
//! | [`Threads`] | [`super::threads`] | The members and the kept messages of each thread. |
//! | [`Work`] | [`super::work`] | The claims and the leads. |
//! | [`TheRiff`] | [`super::the_riff`] | Paused or running, and the settings. |
//!
//! # Only `apply` changes the riff
//!
//! The fields of [`Riff`] are private to this file, and [`apply`] is the
//! only function here that takes a `&mut Riff`
//! (01M3WNQRCBP0PHSA0H3THDH5NJ). Each other part of the server gets a
//! `&Riff`, and reads its parts.
//!
//! # `apply` only stores what a record says
//!
//! `apply` reads only the record and the riff, and no clock
//! (01M3WNQQWA7XGK4Y9ET8HJZ8NN). It reads no presence and no setting,
//! and it makes no decision. So a new rule changes only `handle`, and
//! the same log gives the same riff on each build that knows the
//! records.
//!
//! This list has each place where `apply` reads the riff, or does more
//! than a store of one fact. A new place needs a line here.
//!
//! | Record | What `apply` does | It stays |
//! |---|---|---|
//! | `session_forgotten` | It removes each thing of the session: its entry, its places in the threads, its claims, its lead, and each direct thread whose other session is not known. | Yes. |
//! | `left_thread` | It ends the place of the session in the thread, with its lead of that thread: the lead of a thread is a member of it. | Yes. |
//! | `posted` | A thread keeps its last [`KEEP_MESSAGES`](super::KEEP_MESSAGES) messages, so the oldest one goes (01M3TBZBT7MME9BG1RWX5SZAZ6). The number is a constant of the format. | Yes. |
//! | `claimed` | The new holder replaces the old one. | Until E2 (#392): then the old holder gets a `released` record first. |
//! | `released` | It frees the claim only when the session of the record holds it. If not, the record changes nothing. `handle` refuses such a release, so only a fault or an old build writes this record. | Yes. |
//! | each record that names a session | It keeps the URI of the session and the time of the record. | Yes. |

use riff_core::record::{Change, Record};

use super::sessions::Sessions;
use super::the_riff::TheRiff;
use super::threads::Threads;
use super::work::Work;

/// The state that the log gives. Only [`apply`] changes it.
///
/// The fields are private to this file. So this does not compile:
///
/// ```compile_fail,E0616
/// use riff_server::state::Riff;
///
/// let mut riff = Riff::default();
/// riff.position = 7;
/// ```
///
/// A record changes the riff, through [`apply`]:
///
/// ```
/// use riff_core::record::{Change, Record, RiffStateSet};
/// use riff_core::wire::RiffState;
/// use riff_server::state::{Riff, apply};
///
/// let mut riff = Riff::default();
/// let change = Change::RiffStateSet(RiffStateSet { state: RiffState::Running });
/// apply(&mut riff, &Record { position: 7, written_at_ms: 0, change });
/// assert_eq!(riff.position(), 7);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Riff {
    sessions: Sessions,
    threads: Threads,
    work: Work,
    the_riff: TheRiff,
    /// The position of the last record.
    position: u64,
}

impl Riff {
    /// The riff of a checkpoint at `position`.
    pub(super) fn restore(
        position: u64,
        sessions: Sessions,
        threads: Threads,
        work: Work,
        the_riff: TheRiff,
    ) -> Riff {
        Riff {
            sessions,
            threads,
            work,
            the_riff,
            position,
        }
    }

    /// The position of the last record.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Each session that a record names.
    pub(super) fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    /// The threads, with their members and messages.
    pub(super) fn threads(&self) -> &Threads {
        &self.threads
    }

    /// The claims and the leads.
    pub(super) fn work(&self) -> &Work {
        &self.work
    }

    /// Paused or running, and the settings.
    pub(super) fn the_riff(&self) -> &TheRiff {
        &self.the_riff
    }
}

/// Changes `riff` for one record. It does no I/O, reads no clock, and
/// does not fail. A record that the state cannot take changes nothing,
/// and logs a warning with its position. See the module docs for what
/// it does with each kind.
///
/// ```
/// use riff_core::record::{Change, Claimed, Record};
/// use riff_server::state::{Riff, apply};
///
/// let claimed = Claimed {
///     session: "riff://ann@heron/acme/app?session=s1".parse()?,
///     thread: "acme/app".parse()?,
///     item: "issue-7".into(),
/// };
/// let mut riff = Riff::default();
/// apply(&mut riff, &Record { position: 1, written_at_ms: 0, change: Change::Claimed(claimed.clone()) });
/// assert_eq!(riff.position(), 1);
///
/// // A release of a claim that the session does not hold changes nothing
/// // but the position.
/// let other = Claimed { item: "issue-8".into(), ..claimed };
/// apply(&mut riff, &Record { position: 2, written_at_ms: 0, change: Change::Released(other) });
/// assert_eq!(riff.position(), 2);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn apply(riff: &mut Riff, record: &Record) {
    riff.position = record.position;
    riff.sessions.named(record);
    let taken = match &record.change {
        Change::Posted(posted) => riff.threads.posted(posted),
        Change::JoinedThread(member) => riff.threads.joined(member),
        Change::LeftThread(member) => {
            let was_member = riff.threads.left(member);
            let was_lead = riff.work.left(member);
            if was_member || was_lead {
                Ok(())
            } else {
                Err("the session is not in the thread")
            }
        }
        Change::Claimed(claimed) => riff.work.claimed(claimed),
        Change::Released(claimed) => riff.work.released(claimed),
        Change::LeadSet(member) => riff.work.lead_set(member),
        Change::RiffStateSet(set) => riff.the_riff.state_set(set),
        Change::SettingChanged(changed) => riff.the_riff.setting_changed(changed),
        Change::SessionForgotten(forgotten) => {
            let who = forgotten.session.who();
            let known = riff.sessions.forgotten(who);
            riff.threads.forgotten(who, &riff.sessions);
            riff.work.forgotten(who);
            if known {
                Ok(())
            } else {
                Err("the session is not known")
            }
        }
    };
    if let Err(what) = taken {
        tracing::warn!(
            position = record.position,
            "a record changes nothing: {what}"
        );
    }
}

#[cfg(test)]
mod tests {
    use riff_core::name::{SessionUri, ThreadName};
    use riff_core::record::Claimed;

    use super::*;

    fn ann() -> SessionUri {
        "riff://ann@heron/acme/app?session=a1".parse().unwrap()
    }

    fn bob() -> SessionUri {
        "riff://bob@kite/acme/app?session=b1".parse().unwrap()
    }

    fn repo() -> ThreadName {
        "acme/app".parse().unwrap()
    }

    fn claim_of(me: &SessionUri) -> Claimed {
        Claimed {
            session: me.clone(),
            thread: repo(),
            item: "issue-7".into(),
        }
    }

    /// The riff that the changes give, with the positions 1, 2, 3, and
    /// so on.
    fn riff_of(changes: &[Change]) -> Riff {
        let mut riff = Riff::default();
        for (n, change) in changes.iter().enumerate() {
            let record = Record {
                position: u64::try_from(n).unwrap() + 1,
                written_at_ms: 5,
                change: change.clone(),
            };
            apply(&mut riff, &record);
        }
        riff
    }

    #[test]
    fn a_released_record_of_a_session_that_does_not_hold_the_item_changes_no_claim() {
        let held = riff_of(&[Change::Claimed(claim_of(&ann()))]);
        let after = riff_of(&[
            Change::Claimed(claim_of(&ann())),
            Change::Released(claim_of(&bob())),
        ]);
        assert_eq!(after.work(), held.work());
        assert_eq!(after.work().holder(&repo(), "issue-7"), Some(ann().who()));
        assert_eq!(after.position(), 2);
    }

    #[test]
    fn a_claimed_record_replaces_the_old_holder() {
        let riff = riff_of(&[
            Change::Claimed(claim_of(&ann())),
            Change::Claimed(claim_of(&bob())),
        ]);
        assert_eq!(riff.work().holder(&repo(), "issue-7"), Some(bob().who()));
        assert!(riff.work().items_of(ann().who()).is_empty());
    }

    #[test]
    fn the_same_records_give_the_same_riff() {
        let changes = [
            Change::Claimed(claim_of(&ann())),
            Change::Released(claim_of(&ann())),
            Change::Claimed(claim_of(&bob())),
        ];
        assert_eq!(riff_of(&changes), riff_of(&changes));
        assert_ne!(riff_of(&changes), riff_of(&changes[..2]));
    }
}
