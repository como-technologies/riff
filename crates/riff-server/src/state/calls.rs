//! The calls that the server keeps: the result of each command with a
//! call ID (01M48VFX22S4811DYBBD7QDW24).
//!
//! The link of `riff` sends a command again until it gets a reply. Each
//! try of one call has the same call ID, in the header
//! [`CALL_HEADER`](riff_core::wire::CALL_HEADER). The key of a call is
//! its caller ([`By`]) and its call ID ([`Key`]). The writer keeps the
//! result of each accepted command with a key ([`Kept`]): its records,
//! its note and its time. A second try of a kept call runs no `handle`:
//! the engine makes its reply from the kept result (see
//! [`crate::engine`]).
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Pending: the first try, accepted or refused
//!     Pending --> Kept: the writer wrote the records
//!     Pending --> [*]: refused, or the server stopped
//!     Kept --> [*]: older than CALL_KEEP, or the oldest of CALL_KEEP_MOST
//! ```
//!
//! - The table keeps a key for [`CALL_KEEP`], and at most
//!   [`CALL_KEEP_MOST`] keys of each caller. The oldest key goes first
//!   (01M48VFXEE2GGT7JE10DWBNZEV).
//! - Each record of a command with a call ID has the ID in its envelope
//!   ([`riff_core::record::Envelope::call`], 01M48VFFY5CK9MRXJESV2NHY5F). A checkpoint keeps
//!   the records of each kept call ([`Saved`]). A load makes [`Kept`]
//!   from the checkpoint and from each record after it, with the default
//!   note (01M48VFXHHND8SX4DBXZTFMJGQ). So a start from a checkpoint and
//!   a start from the full log give the same reply to a repeated call.
//! - A command that the engine accepted with no record is kept until
//!   the next start: the log has no trace of it.
//!
//! ```
//! use std::time::Instant;
//! use riff_core::name::SessionUri;
//! use riff_core::record::Record;
//! use riff_core::wire::Join;
//! use riff_server::state::{Caller, Cause, CommandKind, Key, State};
//!
//! let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
//! let now = Instant::now();
//! let mut state = State::default();
//! state.register(&mike, now);
//! let cause = Cause::of(&Caller::of(&mike), CommandKind::Join).with_call(Some("c1".into()));
//! let join = Join { me: mike.clone(), thread: "design".parse()? };
//! let (changes, ()) = state.check(&Caller::of(&mike), &join, now).result.unwrap();
//! let made = state.queue(&cause, &changes, now);
//! assert_eq!(made[0].envelope.call.as_deref(), Some("c1"));
//! let mut log: Vec<Record> = state.take_queue();
//! log.extend(made);
//!
//! // A start from the log keeps the call.
//! let loaded = State::replay(log, now, 0);
//! let key = Key::new(&Caller::of(&mike).by(), "c1");
//! assert_eq!(loaded.kept(&key, now).unwrap().done.made.len(), 1);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::any::Any;
use std::collections::BTreeMap;
use std::time::Duration;

use riff_core::record::{By, Record};
use serde::{Deserialize, Serialize};

use super::command::Done;

/// How long the server keeps the result of a call
/// (01M48VFXEE2GGT7JE10DWBNZEV).
pub const CALL_KEEP: Duration = Duration::from_secs(24 * 60 * 60);

/// The most keys that the server keeps for one caller. The oldest key
/// goes first (01M48VFXEE2GGT7JE10DWBNZEV).
pub const CALL_KEEP_MOST: usize = 1024;

/// The key of a call: its caller and its call ID.
///
/// ```
/// use riff_core::name::Who;
/// use riff_core::record::By;
/// use riff_server::state::Key;
///
/// let ann = By::Session(Who::new("ann", Some("s1"))?);
/// let bob = By::Session(Who::new("bob", Some("s1"))?);
/// // One call ID of two callers is two calls.
/// assert_ne!(Key::new(&ann, "c1"), Key::new(&bob, "c1"));
/// assert_eq!(Key::new(&ann, "c1"), Key::new(&ann, "c1"));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    /// The caller, as the JSON of its [`By`].
    caller: String,
    call: String,
}

impl Key {
    /// The key of the call `call` of the caller `by`.
    pub fn new(by: &By, call: &str) -> Key {
        Key {
            caller: by.json().to_string(),
            call: call.to_owned(),
        }
    }

    /// The key of the command that made `record`. `None` for a record
    /// with no caller or with no call ID.
    pub fn of(record: &Record) -> Option<Key> {
        Some(Key::new(
            record.envelope.by.as_ref()?,
            record.envelope.call.as_deref()?,
        ))
    }

    /// The call ID.
    pub fn call(&self) -> &str {
        &self.call
    }

    /// The first key of `caller`, in the order of the table.
    fn first(caller: &str) -> Key {
        Key {
            caller: caller.to_owned(),
            call: String::new(),
        }
    }
}

/// The result of a call that the writer wrote.
pub struct Kept {
    /// The records of the command, and the sign-ins that they ended.
    pub done: Done,
    /// The note of `handle`. `None` after a load: the reply then gets
    /// the default note.
    note: Option<Box<dyn Any + Send>>,
    /// The time of the write, in milliseconds since the Unix epoch.
    pub at_ms: u64,
}

impl Kept {
    /// The result of a call: its records, its note and its time.
    pub fn new(done: Done, note: Option<Box<dyn Any + Send>>, at_ms: u64) -> Kept {
        Kept { done, note, at_ms }
    }

    /// The note of `handle`, or the default note when the table has
    /// none.
    ///
    /// ```
    /// use riff_server::state::{Done, Kept};
    ///
    /// let kept = Kept::new(Done::default(), Some(Box::new(7_u32)), 0);
    /// assert_eq!(kept.note::<u32>(), 7);
    /// let loaded = Kept::new(Done::default(), None, 0);
    /// assert_eq!(loaded.note::<u32>(), 0);
    /// ```
    pub fn note<N: Clone + Default + 'static>(&self) -> N {
        self.note
            .as_ref()
            .and_then(|note| note.downcast_ref::<N>())
            .cloned()
            .unwrap_or_default()
    }
}

/// The table of the kept calls. The engine owns it under its lock, in
/// the state.
#[derive(Default)]
pub struct Calls {
    kept: BTreeMap<Key, Kept>,
}

impl Calls {
    /// The kept result of `key`, when it is younger than [`CALL_KEEP`]
    /// at `now_ms`.
    pub fn get(&self, key: &Key, now_ms: u64) -> Option<&Kept> {
        self.kept
            .get(key)
            .filter(|kept| now_ms.saturating_sub(kept.at_ms) < keep_ms())
    }

    /// Keeps the result of `key`. Then it drops each key of the same
    /// caller that is older than [`CALL_KEEP`], and the oldest keys past
    /// [`CALL_KEEP_MOST`].
    ///
    /// ```
    /// use riff_core::record::By;
    /// use riff_server::state::{CALL_KEEP_MOST, Calls, Done, Key, Kept};
    ///
    /// let mut calls = Calls::default();
    /// for n in 0..=CALL_KEEP_MOST as u64 {
    ///     calls.keep(Key::new(&By::Server, &format!("c{n}")), Kept::new(Done::default(), None, n));
    /// }
    /// let now = CALL_KEEP_MOST as u64;
    /// // The oldest key went first.
    /// assert!(calls.get(&Key::new(&By::Server, "c0"), now).is_none());
    /// assert!(calls.get(&Key::new(&By::Server, "c1"), now).is_some());
    /// // A key older than CALL_KEEP is gone.
    /// assert!(calls.get(&Key::new(&By::Server, "c1"), 25 * 60 * 60 * 1000).is_none());
    /// ```
    pub fn keep(&mut self, key: Key, kept: Kept) {
        let caller = key.caller.clone();
        let now_ms = kept.at_ms;
        self.kept.insert(key, kept);
        self.prune(&caller, now_ms);
    }

    /// The kept result of `key`, to change it.
    pub fn get_mut(&mut self, key: &Key) -> Option<&mut Kept> {
        self.kept.get_mut(key)
    }

    /// Drops the keys of `caller` that are too old at `now_ms`, and the
    /// oldest ones past [`CALL_KEEP_MOST`].
    fn prune(&mut self, caller: &str, now_ms: u64) {
        let mut of_caller: Vec<(u64, Key)> = self
            .kept
            .range(Key::first(caller)..)
            .take_while(|(key, _)| key.caller == caller)
            .map(|(key, kept)| (kept.at_ms, key.clone()))
            .collect();
        of_caller.sort();
        let past = of_caller.len().saturating_sub(CALL_KEEP_MOST);
        for (n, (at_ms, key)) in of_caller.into_iter().enumerate() {
            if n < past || now_ms.saturating_sub(at_ms) >= keep_ms() {
                self.kept.remove(&key);
            }
        }
    }

    /// Adds a record of the log to the result of its call, at a load.
    /// A record of a new command with an old call ID starts a new
    /// result: its position does not follow the records of the kept
    /// result.
    pub(super) fn record(&mut self, record: &Record) {
        let Some(key) = Key::of(record) else {
            return;
        };
        let follows = self.kept.get(&key).is_some_and(|kept| {
            let last = kept
                .done
                .made
                .last()
                .map_or(0, |last| last.envelope.position);
            last + 1 == record.envelope.position
        });
        if follows {
            if let Some(kept) = self.kept.get_mut(&key) {
                kept.done.made.push(record.clone());
            }
        } else {
            let done = Done::of(vec![record.clone()]);
            self.keep(key, Kept::new(done, None, record.envelope.written_at_ms));
        }
    }

    /// Drops each key that is older than [`CALL_KEEP`] at `now_ms`.
    pub(super) fn expire(&mut self, now_ms: u64) {
        self.kept
            .retain(|_, kept| now_ms.saturating_sub(kept.at_ms) < keep_ms());
    }

    /// The part of a checkpoint: the records of each call that is
    /// younger than [`CALL_KEEP`] at `now_ms`, in the order of their
    /// positions. A kept call with no record is not in it.
    pub(super) fn saved(&self, now_ms: u64) -> Saved {
        let Calls { kept } = self;
        let mut calls: Vec<Record> = kept
            .values()
            .filter(|kept| now_ms.saturating_sub(kept.at_ms) < keep_ms())
            .flat_map(|kept| kept.done.made.iter().cloned())
            .collect();
        calls.sort_by_key(|record| record.envelope.position);
        Saved { calls }
    }
}

/// [`CALL_KEEP`] in milliseconds.
fn keep_ms() -> u64 {
    u64::try_from(CALL_KEEP.as_millis()).unwrap_or(u64::MAX)
}

/// The part of the calls in a checkpoint: the records of each kept
/// call (01M48VFXHHND8SX4DBXZTFMJGQ). A checkpoint of 1.0.0 has none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Saved {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    calls: Vec<Record>,
}

impl Saved {
    /// The table that the checkpoint gives.
    pub(super) fn restore(self) -> Calls {
        let Saved { calls: records } = self;
        let mut calls = Calls::default();
        for record in &records {
            calls.record(record);
        }
        calls
    }
}

#[cfg(test)]
mod tests {
    use riff_core::name::Who;
    use riff_core::record::{Change, Email};

    use super::*;
    use riff_core::record::Envelope;

    fn record(position: u64, by: &By, call: Option<&str>, at_ms: u64) -> Record {
        Record {
            envelope: Envelope {
                position,
                written_at_ms: at_ms,
                by: Some(by.clone()),
                command: Some("invite".into()),
                call: call.map(str::to_owned),
            },
            change: Change::MemberInvited(Email {
                email: format!("p{position}@acme.io"),
            }),
        }
    }

    fn ann() -> By {
        By::Session(Who::new("ann", Some("s1")).unwrap())
    }

    fn positions(kept: &Kept) -> Vec<u64> {
        kept.done.made.iter().map(|r| r.envelope.position).collect()
    }

    #[test]
    fn the_records_of_one_command_make_one_result() {
        let mut calls = Calls::default();
        let ann = ann();
        for position in [4, 5] {
            calls.record(&record(position, &ann, Some("c1"), 10));
        }
        // A record with no call ID makes no result.
        calls.record(&record(6, &ann, None, 10));
        let kept = calls.get(&Key::new(&ann, "c1"), 10).unwrap();
        assert_eq!(positions(kept), [4, 5]);
        assert_eq!(kept.at_ms, 10);
        assert_eq!(calls.kept.len(), 1);
    }

    #[test]
    fn a_new_command_with_an_old_call_id_starts_a_new_result() {
        let mut calls = Calls::default();
        let ann = ann();
        calls.record(&record(4, &ann, Some("c1"), 10));
        calls.record(&record(9, &ann, Some("c1"), 20));
        let kept = calls.get(&Key::new(&ann, "c1"), 20).unwrap();
        assert_eq!(positions(kept), [9]);
        assert_eq!(kept.at_ms, 20);
    }

    #[test]
    fn the_limit_of_one_caller_drops_no_key_of_another_caller() {
        let mut calls = Calls::default();
        let ann = ann();
        calls.keep(
            Key::new(&By::Server, "old"),
            Kept::new(Done::default(), None, 0),
        );
        for n in 0..=CALL_KEEP_MOST as u64 {
            let key = Key::new(&ann, &format!("c{n}"));
            calls.keep(key, Kept::new(Done::default(), None, n + 1));
        }
        let now = CALL_KEEP_MOST as u64 + 1;
        assert!(calls.get(&Key::new(&By::Server, "old"), now).is_some());
        assert!(calls.get(&Key::new(&ann, "c0"), now).is_none());
        assert!(calls.get(&Key::new(&ann, "c1"), now).is_some());
    }

    #[test]
    fn a_checkpoint_keeps_the_records_of_each_young_call() {
        let mut calls = Calls::default();
        let ann = ann();
        let day = keep_ms();
        calls.record(&record(1, &ann, Some("old"), 0));
        calls.record(&record(2, &ann, Some("c1"), day));
        calls.record(&record(3, &By::Server, Some("c2"), day));
        let saved = calls.saved(day + 1);
        let saved_at: Vec<u64> = saved.calls.iter().map(|r| r.envelope.position).collect();
        assert_eq!(saved_at, [2, 3]);

        let json = serde_json::to_string(&saved).unwrap();
        let restored = serde_json::from_str::<Saved>(&json).unwrap().restore();
        assert!(restored.get(&Key::new(&ann, "c1"), day + 1).is_some());
        assert!(
            restored
                .get(&Key::new(&By::Server, "c2"), day + 1)
                .is_some()
        );
        assert!(restored.get(&Key::new(&ann, "old"), day + 1).is_none());
        // An empty part writes nothing: a checkpoint of 1.0.0 has none.
        assert_eq!(serde_json::to_string(&Saved::default()).unwrap(), "{}");
    }
}
