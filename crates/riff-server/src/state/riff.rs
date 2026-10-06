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
//! | [`TheRiff`] | [`super::the_riff`] | The pauses, and the settings. |
//! | [`People`] | [`super::people`] | The riff ID, the email of each USER, the members, the admins, the owner, and the request for the owner role. |
//! | [`Plans`] | [`super::plan`] | The holds of each repository thread. |
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
//! | `session_forgotten` | It removes each thing of the session: its entry, its places in the threads, its claims, its lead, and each direct thread whose other session is not known. | Yes. `forget` gives each claim its `released` record first (01M3X9XCSBR11ACD86FNXKF8JH). |
//! | `left_thread` | It ends the place of the session in the thread, with its lead of that thread: the lead of a thread is a member of it. | Yes. |
//! | `posted` | A thread keeps its last [`KEEP_MESSAGES`](super::KEEP_MESSAGES) messages, so the oldest one goes (01M3TBZBT7MME9BG1RWX5SZAZ6). The number is a constant of the format. | Yes. |
//! | `claimed` | The new holder replaces the old one. | Yes, for a log from before the `released` record of a taken item (01M3X4Z6BKM251H7CS2CEGR205). A new claim gives the old holder that record first. |
//! | `released` | It frees the claim only when the session of the record holds it. If not, the record changes nothing. `handle` refuses such a release, so only a fault or an old build writes this record. | Yes. |
//! | `lead_set` | The session of the record replaces the old lead of its user in the thread. | Yes. |
//! | `posted`, `session_forgotten` | They keep the index of the signed messages for the copy check: a `posted` record adds the hash of its payload, and removes the hash of the message that goes at the limit. A `session_forgotten` record removes the hashes of each direct thread that goes. | Yes. |
//! | each record that names a session | It keeps the URI of the session and the time of the record. | Yes. |
//! | `session_started` | It stores the worker mark. A record with the reason `process` or `clear` also stores its time, and ends MustClear. A record with the reason `other` changes nothing. | Yes. |
//! | `released` | A record with `must_clear` sets the MustClear mark of its session. | Yes. |
//! | `pause_set` | It keeps the `by` and the time of the record with the pause: who set it, and when (01M3XAHZBGSSJB3YX23K88W01K). A scope that this build does not know changes nothing. | Yes. |
//! | `riff_made` | A riff keeps its first ID: a second record changes nothing. `handle` makes the record only for a riff with no ID. | Yes. |
//! | `person_joined` | The first email keeps a USER (R209): a record for a USER that another email holds changes nothing. `handle` refuses such a sign-in. | Yes. |
//! | `member_removed` | It finds each USER of the email, and keeps the position of the record for each: the end of their sign-ins (01M3XA87A9GGFA89RQXWSKY0V6). | Yes. |
//! | `signins_ended` | It keeps the position of the record for the USER. | Yes. |
//! | `owner_set` | It ends the request for the owner role that waits. A record with no email says that the owner is gone. | Yes. |
//! | `item_held` | It keeps the `by` and the time of the record with the hold: who held the item, and when (01M43GSGB9ZFHSG0Q83Y50FEGW). | Yes. |

use riff_core::record::{Change, Record};

use super::people::People;
use super::plan::Plans;
use super::sessions::Sessions;
use super::snapshot::LoadPath;
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
/// use riff_core::record::{Change, PauseSet, Record, Scope};
/// use riff_core::wire::RiffState;
/// use riff_server::state::{Riff, apply};
///
/// let mut riff = Riff::default();
/// let change = Change::PauseSet(PauseSet { scope: Scope::Riff, state: RiffState::Running });
/// apply(&mut riff, &Record { position: 7, written_at_ms: 0, by: None, command: None, call: None, change });
/// assert_eq!(riff.position(), 7);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Riff {
    sessions: Sessions,
    threads: Threads,
    work: Work,
    the_riff: TheRiff,
    people: People,
    plans: Plans,
    /// The position of the last record.
    position: u64,
}

impl Riff {
    /// The riff of a checkpoint at `position`. Only the load path of a
    /// checkpoint can call it: only [`super::snapshot`] makes a
    /// `LoadPath`. So no other code makes a riff with no `apply`. It
    /// takes one argument for each part.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn restore(
        _: LoadPath,
        position: u64,
        sessions: Sessions,
        threads: Threads,
        work: Work,
        the_riff: TheRiff,
        people: People,
        plans: Plans,
    ) -> Riff {
        Riff {
            sessions,
            threads,
            work,
            the_riff,
            people,
            plans,
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

    /// The pauses, and the settings.
    pub(super) fn the_riff(&self) -> &TheRiff {
        &self.the_riff
    }

    /// The people: who may join the riff, and with which role.
    pub fn people(&self) -> &People {
        &self.people
    }

    /// The plan of each repository thread: the holds.
    pub fn plans(&self) -> &Plans {
        &self.plans
    }
}

/// Changes `riff` for one record. It does no I/O, reads no clock, and
/// does not fail. A record that the state cannot take changes nothing,
/// and logs a warning with its position. See the module docs for what
/// it does with each kind.
///
/// ```
/// use riff_core::record::{Change, Claimed, Record, Released};
/// use riff_server::state::{Riff, apply};
///
/// let claimed = Claimed {
///     session: "riff://ann@heron/acme/app?session=s1".parse()?,
///     thread: "acme/app".parse()?,
///     item: "issue-7".into(),
/// };
/// let mut riff = Riff::default();
/// let record = |position, change| Record { position, written_at_ms: 0, by: None, command: None, call: None, change };
/// apply(&mut riff, &record(1, Change::Claimed(claimed.clone())));
/// assert_eq!(riff.position(), 1);
///
/// // A release of a claim that the session does not hold changes nothing
/// // but the position.
/// let other = Claimed { item: "issue-8".into(), ..claimed };
/// apply(&mut riff, &record(2, Change::Released(Released::of(other))));
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
        Change::Released(released) => {
            riff.sessions.released(released);
            riff.work.released(released)
        }
        Change::SessionStarted(started) => riff.sessions.started(started, record.written_at_ms),
        Change::LeadSet(member) => riff.work.lead_set(member),
        Change::PauseSet(set) => riff.the_riff.pause_set(&set.scope, set.state, record),
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
        Change::RiffMade(made) => riff.people.made(made),
        Change::PersonJoined(joined) => riff.people.joined(joined),
        Change::MemberInvited(invited) => riff.people.invited(invited),
        Change::MemberRemoved(removed) => riff.people.removed(removed, record.position),
        Change::AdminSet(set) => riff.people.admin_set(set),
        Change::OwnerSet(set) => riff.people.owner_set(set),
        Change::OwnerAsked(asked) => riff.people.owner_asked(asked),
        Change::OwnerDenied(denied) => riff.people.owner_denied(denied),
        Change::SigninsEnded(ended) => riff.people.signins_ended(ended, record.position),
        Change::ItemHeld(held) => riff.plans.held(held, record),
        Change::ItemFreed(freed) => riff.plans.freed(freed),
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
    use riff_core::record::{Claimed, Released, SessionStarted};
    use riff_core::wire::StartReason;

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
                by: None,
                command: None,
                call: None,
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
            Change::Released(Released::of(claim_of(&bob()))),
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
            Change::Released(Released::of(claim_of(&ann()))),
            Change::Claimed(claim_of(&bob())),
        ];
        assert_eq!(riff_of(&changes), riff_of(&changes));
        assert_ne!(riff_of(&changes), riff_of(&changes[..2]));
    }

    fn started(me: &SessionUri, reason: StartReason, worker: bool) -> Change {
        Change::SessionStarted(SessionStarted {
            session: me.clone(),
            reason,
            worker,
        })
    }

    fn last_release(me: &SessionUri) -> Change {
        Change::Released(Released {
            must_clear: true,
            ..Released::of(claim_of(me))
        })
    }

    #[test]
    fn a_session_started_record_stores_the_worker_mark_and_the_time_of_a_fresh_start() {
        let who = ann();
        let who = who.who();
        let riff = riff_of(&[started(&ann(), StartReason::Join, true)]);
        assert!(riff.sessions().worker(who));
        assert_eq!(riff.sessions().fresh_ms(who), None, "a join is not fresh");
        let riff = riff_of(&[started(&ann(), StartReason::Resume, false)]);
        assert!(!riff.sessions().worker(who));
        assert_eq!(riff.sessions().fresh_ms(who), None, "a resume is not fresh");
        for fresh in [StartReason::Process, StartReason::Clear] {
            let riff = riff_of(&[started(&ann(), fresh, true)]);
            assert_eq!(riff.sessions().fresh_ms(who), Some(5), "{fresh:?}");
        }
    }

    #[test]
    fn a_released_record_with_must_clear_sets_the_mark_and_a_fresh_start_ends_it() {
        let who = ann();
        let who = who.who();
        let mut changes = vec![
            started(&ann(), StartReason::Process, true),
            Change::Claimed(claim_of(&ann())),
            last_release(&ann()),
        ];
        assert!(riff_of(&changes).sessions().must_clear(who));
        assert!(riff_of(&changes).work().items_of(who).is_empty());
        // A start with no fresh context keeps the mark.
        for stays in [StartReason::Resume, StartReason::Join, StartReason::Other] {
            changes.push(started(&ann(), stays, true));
            assert!(riff_of(&changes).sessions().must_clear(who), "{stays:?}");
        }
        for fresh in [StartReason::Process, StartReason::Clear] {
            let mut changes = changes.clone();
            changes.push(started(&ann(), fresh, true));
            assert!(!riff_of(&changes).sessions().must_clear(who), "{fresh:?}");
        }
    }

    #[test]
    fn a_released_record_with_no_must_clear_sets_no_mark() {
        let riff = riff_of(&[
            Change::Claimed(claim_of(&ann())),
            Change::Released(Released::of(claim_of(&ann()))),
        ]);
        assert!(!riff.sessions().must_clear(ann().who()));
    }

    #[test]
    fn a_session_started_record_with_the_reason_other_changes_no_mark() {
        let with = riff_of(&[
            started(&ann(), StartReason::Process, true),
            started(&ann(), StartReason::Other, false),
        ]);
        assert!(with.sessions().worker(ann().who()));
        assert_eq!(with.sessions().fresh_ms(ann().who()), Some(5));
    }

    #[test]
    fn a_session_forgotten_record_drops_the_life_cycle() {
        let riff = riff_of(&[
            started(&ann(), StartReason::Process, true),
            Change::Claimed(claim_of(&ann())),
            last_release(&ann()),
            Change::SessionForgotten(riff_core::record::Forgotten { session: ann() }),
        ]);
        assert!(!riff.sessions().knows(ann().who()));
        assert!(!riff.sessions().must_clear(ann().who()));
        assert!(!riff.sessions().worker(ann().who()));
    }
}
