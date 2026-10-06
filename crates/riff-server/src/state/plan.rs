//! The group "plan": the commands [`Hold`] and [`Free`], and the plan of
//! each repository thread. The design is in the book: "Design: the plan
//! on the server". This build has only the holds.
//!
//! - Part of the riff: [`Plans`]. The holds of each repository thread.
//! - `apply`: `Plans::held` for an `item_held` record, and
//!   `Plans::freed` for an `item_freed` record.
//! - Checkpoint: `Saved`, the field `plans`. An empty part is not
//!   written, so a log with no hold gives the checkpoint of 1.0.0
//!   (01M43GSGVYJW7C09SVRWRAQZDZ).
//! - The check of a claim: `View::hold` (see [`super::work`]).
//!
//! # The holds (01M43GSGB9ZFHSG0Q83Y50FEGW)
//!
//! A lead holds an item with a reason, and frees it again. A hold is not
//! a claim: it stops only the claim of a worker
//! (01M43GSGPJ69TPWPA4935WR8RW). It names one item by its exact name: a
//! hold of `issue-12` does not stop `verify-issue-12`. It does not end a
//! claim, and only `free` ends it.
//!
//! | Command | Who (01M43GSGGY0QMB5D5EH92M6ZFP) | Else |
//! |---|---|---|
//! | `hold`, `free` | a lead of the repository thread; the owner and each admin | `not_allowed` |
//!
//! [`permits`](super::permits) refuses a worker. `handle` checks the
//! lead and the role, because "the lead of this thread" needs the state.
//!
//! ```mermaid
//! flowchart LR
//!     H[hold ITEM REASON] -->|a lead, the owner, an admin| R[(item_held)]
//!     F[free ITEM] -->|held| E[(item_freed)]
//!     F -->|not held| N[no record: a no_change line]
//!     R --> C{claim ITEM}
//!     C -->|a worker| X[on_hold]
//!     C -->|each other session| G[granted, with a warning]
//! ```

use std::collections::BTreeMap;

use riff_core::name::{ThreadName, check};
use riff_core::record::{By, Change, ItemFreed, ItemHeld, Record};
use riff_core::wire::{Free, FreeReply, Hold, HoldReply};
use serde::{Deserialize, Serialize};

use super::command::{Caller, Class, Code, Command, CommandKind, Done, Now, Refused, Role};
use super::view::View;

/// The most characters of the reason of a hold.
pub const REASON_MAX: usize = 200;

/// The hold of one item: its reason, and who held it and when: the `by`
/// and the time of its `item_held` record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldInfo {
    pub reason: String,
    /// The caller of the hold. `None` when the record has no cause.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<By>,
    /// The time of the record, in milliseconds since the Unix epoch.
    pub at_ms: u64,
}

/// The plan of one repository thread. This build has only its holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Plan {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    holds: BTreeMap<String, HoldInfo>,
}

impl Plan {
    fn is_empty(&self) -> bool {
        self.holds.is_empty()
    }
}

/// The plan of each repository thread. A thread with no hold has no
/// entry.
///
/// ```
/// use std::time::Instant;
/// use riff_core::name::{SessionUri, ThreadName};
/// use riff_core::wire::{Free, Hold, RiffState};
/// use riff_server::state::{Caller, State};
///
/// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a1".parse()?;
/// let thread: ThreadName = "como-technologies/riff".parse()?;
/// let now = Instant::now();
/// let mut state = State::default();
/// // The first session of its user that registers is the lead.
/// state.register(&mike, now);
/// state.riff(&mike, Some(RiffState::Running), now).unwrap();
///
/// let reason = "waits for the word of Mike".to_owned();
/// let hold = Hold { me: mike.clone(), thread: thread.clone(), item: "issue-366".into(), reason };
/// state.run(&Caller::of(&mike), &hold, now).unwrap();
/// let held = state.plans().hold(&thread, "issue-366").unwrap();
/// assert_eq!(held.reason, "waits for the word of Mike");
/// assert_eq!(held.by.as_ref().unwrap().to_string(), "the session mike/a1");
/// // A hold names one item by its exact name.
/// assert!(state.plans().hold(&thread, "verify-issue-366").is_none());
///
/// let free = Free { me: mike.clone(), thread: thread.clone(), item: "issue-366".into() };
/// let (made, ()) = state.run(&Caller::of(&mike), &free, now).unwrap();
/// assert_eq!(made.len(), 1);
/// assert!(state.plans().hold(&thread, "issue-366").is_none());
/// // A free of an item with no hold makes no record.
/// let (made, ()) = state.run(&Caller::of(&mike), &free, now).unwrap();
/// assert!(made.is_empty());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plans {
    plans: BTreeMap<ThreadName, Plan>,
}

impl Plans {
    /// The hold of `item` in `thread`, when it is held.
    pub fn hold(&self, thread: &ThreadName, item: &str) -> Option<&HoldInfo> {
        self.plans.get(thread)?.holds.get(item)
    }

    /// Each hold of `thread`, by its item.
    pub fn holds(&self, thread: &ThreadName) -> impl Iterator<Item = (&str, &HoldInfo)> {
        self.plans
            .get(thread)
            .into_iter()
            .flat_map(|plan| plan.holds.iter().map(|(item, hold)| (item.as_str(), hold)))
    }

    /// The item of the record is held, with its reason. It keeps the
    /// `by` and the time of the record. A second record replaces the
    /// reason, the `by` and the time.
    pub(super) fn held(&mut self, held: &ItemHeld, record: &Record) -> Result<(), &'static str> {
        let hold = HoldInfo {
            reason: held.reason.clone(),
            by: record.envelope.by.clone(),
            at_ms: record.envelope.written_at_ms,
        };
        let plan = self.plans.entry(held.thread.clone()).or_default();
        plan.holds.insert(held.item.clone(), hold);
        Ok(())
    }

    /// The hold of the item of the record ends. A thread with no hold
    /// left has no entry.
    pub(super) fn freed(&mut self, freed: &ItemFreed) -> Result<(), &'static str> {
        let Some(plan) = self.plans.get_mut(&freed.thread) else {
            return Err("the item is not held");
        };
        let was_held = plan.holds.remove(&freed.item).is_some();
        if plan.is_empty() {
            self.plans.remove(&freed.thread);
        }
        if was_held {
            Ok(())
        } else {
            Err("the item is not held")
        }
    }

    /// The plans, for a checkpoint.
    pub(super) fn saved(&self) -> Saved {
        Saved {
            plans: self.plans.clone(),
        }
    }
}

/// The part of the checkpoint of this group: the plan of each
/// repository thread, with each hold, its reason, its `by` and its
/// time. An empty part is not written (01M43GSGVYJW7C09SVRWRAQZDZ).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Saved {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    plans: BTreeMap<ThreadName, Plan>,
}

impl Saved {
    pub(super) fn restore(mut self) -> Plans {
        self.plans.retain(|_, plan| !plan.is_empty());
        Plans { plans: self.plans }
    }
}

impl View<'_> {
    /// The hold of `item` in `thread`, when it is held.
    pub(super) fn hold(&self, thread: &ThreadName, item: &str) -> Option<&HoldInfo> {
        self.riff.plans().hold(thread, item)
    }
}

/// The text of a hold for a claim: who held the item, when, and why
/// (01M43GSGPJ69TPWPA4935WR8RW).
///
/// ```
/// use riff_core::name::Who;
/// use riff_core::record::By;
/// use riff_server::state::plan::{HoldInfo, held_text};
///
/// let hold = HoldInfo {
///     reason: "waits for the word of Mike".into(),
///     by: Some(By::Session(Who::new("mike", Some("3511e217"))?)),
///     at_ms: 1_790_000_000_000,
/// };
/// assert_eq!(
///     held_text("issue-366", &hold),
///     "issue-366 is held by the lead (the session mike/3511e217) since 2026-09-21T14:13:20Z: waits for the word of Mike"
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn held_text(item: &str, hold: &HoldInfo) -> String {
    let by = hold
        .by
        .as_ref()
        .map_or_else(String::new, |by| format!(" ({by})"));
    format!(
        "{item} is held by the lead{by} since {}: {}",
        crate::tools::utc(hold.at_ms),
        hold.reason
    )
}

/// The checks of a hold and a free that do not read the plan: the item,
/// the thread, and who may (01M43GSGGY0QMB5D5EH92M6ZFP).
fn may_change(
    kind: CommandKind,
    thread: &ThreadName,
    item: &str,
    caller: &Caller,
    view: &View<'_>,
    now: Now,
) -> Result<(), Refused> {
    check(kind.as_str(), item).map_err(|e| e.to_string())?;
    if thread.is_direct() || !thread.to_string().contains('/') {
        return Err(format!("{thread} is not a repository thread: name OWNER/REPO").into());
    }
    let who = caller.who();
    let admin = caller.role() >= Role::Admin;
    let lead = caller.class() == Class::Session
        && view.lead_of(&(who.user().to_owned(), thread.clone()), now.at) == Some(who);
    if admin || lead {
        Ok(())
    } else {
        Err(Refused::new(
            Code::NotAllowed,
            format!(
                "only a lead of {thread}, the owner or an admin can {kind} an item. Tell the lead."
            ),
        ))
    }
}

/// Holds an item of a repository thread with a reason
/// (01M43GSGB9ZFHSG0Q83Y50FEGW). A hold of a held item replaces its
/// reason. A hold with the same reason changes nothing. The reply names
/// the session that holds a claim of the item.
impl Command for Hold {
    const KIND: CommandKind = CommandKind::Hold;
    type Reply = HoldReply;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let Hold {
            thread,
            item,
            reason,
            ..
        } = self;
        may_change(Self::KIND, thread, item, caller, view, now)?;
        let reason = reason.trim();
        let length = reason.chars().count();
        if !(1..=REASON_MAX).contains(&length) {
            return Err(format!(
                "a hold needs a reason of 1 to {REASON_MAX} characters, not {length}"
            )
            .into());
        }
        let mut changes = Vec::new();
        if view.hold(thread, item).map(|hold| hold.reason.as_str()) != Some(reason) {
            changes.push(Change::ItemHeld(ItemHeld {
                thread: thread.clone(),
                item: item.clone(),
                reason: reason.to_owned(),
            }));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, view: &View<'_>, done: &Done, (): (), now: Now) -> HoldReply {
        HoldReply {
            changed: !done.made.is_empty(),
            holder: view
                .riff
                .work()
                .holder(&self.thread, &self.item)
                .map(|holder| view.uri(holder, now.at)),
        }
    }
}

/// Ends the hold of an item (01M43GSGB9ZFHSG0Q83Y50FEGW). A free of an
/// item with no hold makes no record: the trace is a `no_change` line.
impl Command for Free {
    const KIND: CommandKind = CommandKind::Free;
    type Reply = FreeReply;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let Free { thread, item, .. } = self;
        may_change(Self::KIND, thread, item, caller, view, now)?;
        let mut changes = Vec::new();
        if view.hold(thread, item).is_some() {
            changes.push(Change::ItemFreed(ItemFreed {
                thread: thread.clone(),
                item: item.clone(),
            }));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, done: &Done, (): (), _: Now) -> FreeReply {
        FreeReply {
            freed: !done.made.is_empty(),
        }
    }
}
