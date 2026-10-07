//! The group "plan": the commands [`Hold`], [`Free`], [`SetPlan`] and
//! [`PlanOff`], and the plan of each repository thread. The design is in
//! the book: "Design: the plan on the server". This build has the holds
//! and the plan; the checks of a claim against the plan come later.
//!
//! - Part of the riff: [`Plans`]. The holds and the plan of each
//!   repository thread.
//! - `apply`: `Plans::held` for an `item_held` record, `Plans::freed`
//!   for an `item_freed` record, `Plans::set` for a `plan_set` record,
//!   and `Plans::ended` for a `plan_ended` record.
//! - Checkpoint: `Saved`, the field `plans`. An empty part is not
//!   written, so a log with no hold and no plan gives the checkpoint of
//!   1.0.0 (01M43GSGVYJW7C09SVRWRAQZDZ, 01M4A4Z3QVRC57RE7M43ZRF4T2).
//! - Presence: the time of the last look of each plan
//!   ([`Signal::PlanSeen`]). A plan is stale
//!   [`PLAN_TTL`] after it (01M4A4Z1QTHYXZDMCP9DZ39WVT).
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
//!
//! # The plan (01M4A4YTNSJR0R1T9JNXPBSKHC)
//!
//! A look of the lead reads the plan of the forge, and sends the full
//! plan with `base`: the position of the plan that it compared with. A
//! `base` that is not the position on the server is refused with
//! `stale_base`, so two looks that cross never write an old plan over
//! a new one (01M4A4YTR2NKVBPE6BT9EC3X75).
//!
//! | Command | Who (01M4A4YTWK68JDA0DKXX2HV4FA) | Else |
//! |---|---|---|
//! | `plan`, `plan_off` | a session in the repository thread; the owner and each admin | `not_allowed` |
//!
//! ```mermaid
//! flowchart LR
//!     P[plan, base] --> B{base is the position<br/>of the plan of the server?}
//!     B -->|no| X[stale_base]
//!     B -->|yes| S{the same plan?}
//!     S -->|yes| N[no record: a look]
//!     S -->|no| R[(plan_set)]
//!     O[plan_off] -->|a plan| E[(plan_ended)]
//!     O -->|no plan| M[no record]
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use riff_core::name::{ThreadName, check};
use riff_core::record::{
    Change, Envelope, ItemFreed, ItemHeld, Plan, PlanEnded, PlanItem, PlanSet, Record,
};
pub use riff_core::wire::HoldInfo;
use riff_core::wire::{
    Free, FreeReply, Hold, HoldReply, PlanOff, PlanOffReply, PlanReply, PlanShown, SetPlan,
};
use serde::{Deserialize, Serialize};

use super::command::{Caller, Class, Code, Command, CommandKind, Done, Now, Refused, Role};
use super::presence::Signal;
use super::view::View;

/// The most characters of the reason of a hold.
pub const REASON_MAX: usize = 200;

/// A plan that no look saw for this time is stale
/// (01M4A4Z1QTHYXZDMCP9DZ39WVT).
pub const PLAN_TTL: Duration = Duration::from_secs(10 * 60);

/// The plan of the server for one repository thread, with the position
/// and the time of its `plan_set` record (01M4A4Z3QVRC57RE7M43ZRF4T2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Current {
    #[serde(flatten)]
    pub plan: Plan,
    pub position: u64,
    /// The time of the record, in milliseconds since the Unix epoch.
    pub at_ms: u64,
}

/// The part of one repository thread: its holds, and its plan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
struct Part {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    holds: BTreeMap<String, HoldInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plan: Option<Current>,
}

impl Part {
    fn is_empty(&self) -> bool {
        self.holds.is_empty() && self.plan.is_none()
    }
}

/// The plan of each repository thread. A thread with no hold and no
/// plan has no entry.
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
    plans: BTreeMap<ThreadName, Part>,
}

impl Plans {
    /// The hold of `item` in `thread`, when it is held.
    pub fn hold(&self, thread: &ThreadName, item: &str) -> Option<&HoldInfo> {
        self.plans.get(thread)?.holds.get(item)
    }

    /// The plan of `thread`, when its plan is on.
    pub fn plan(&self, thread: &ThreadName) -> Option<&Current> {
        self.plans.get(thread)?.plan.as_ref()
    }

    /// Each hold of `thread`, by its item.
    pub fn holds(&self, thread: &ThreadName) -> impl Iterator<Item = (&str, &HoldInfo)> {
        self.plans
            .get(thread)
            .into_iter()
            .flat_map(|part| part.holds.iter().map(|(item, hold)| (item.as_str(), hold)))
    }

    /// The item of the record is held, with its reason. It keeps the
    /// `by` and the time of the record. A second record replaces the
    /// reason, the `by` and the time.
    pub(super) fn held(&mut self, held: &ItemHeld, record: &Record) -> Result<(), &'static str> {
        let ItemHeld {
            thread,
            item,
            reason,
        } = held;
        let Envelope {
            position: _,
            written_at_ms,
            by,
            command: _,
            call: _,
        } = &record.envelope;
        let hold = HoldInfo {
            reason: reason.clone(),
            by: by.clone(),
            at_ms: *written_at_ms,
        };
        let part = self.plans.entry(thread.clone()).or_default();
        part.holds.insert(item.clone(), hold);
        Ok(())
    }

    /// The hold of the item of the record ends. A thread with no hold
    /// left has no entry.
    pub(super) fn freed(&mut self, freed: &ItemFreed) -> Result<(), &'static str> {
        let Some(part) = self.plans.get_mut(&freed.thread) else {
            return Err("the item is not held");
        };
        let was_held = part.holds.remove(&freed.item).is_some();
        if part.is_empty() {
            self.plans.remove(&freed.thread);
        }
        if was_held {
            Ok(())
        } else {
            Err("the item is not held")
        }
    }

    /// The plan of the record replaces the plan of its thread. It keeps
    /// the position and the time of the record.
    pub(super) fn set(&mut self, set: &PlanSet, record: &Record) -> Result<(), &'static str> {
        let PlanSet { thread, plan } = set;
        let Envelope {
            position,
            written_at_ms,
            by: _,
            command: _,
            call: _,
        } = &record.envelope;
        let current = Current {
            plan: plan.clone(),
            position: *position,
            at_ms: *written_at_ms,
        };
        self.plans.entry(thread.clone()).or_default().plan = Some(current);
        Ok(())
    }

    /// The plan of the thread of the record ends. The holds stay.
    pub(super) fn ended(&mut self, ended: &PlanEnded) -> Result<(), &'static str> {
        let PlanEnded { thread } = ended;
        let Some(part) = self.plans.get_mut(thread) else {
            return Err("the thread has no plan");
        };
        let had = part.plan.take().is_some();
        if part.is_empty() {
            self.plans.remove(thread);
        }
        if had {
            Ok(())
        } else {
            Err("the thread has no plan")
        }
    }

    /// The plans, for a checkpoint.
    pub(super) fn saved(&self) -> Saved {
        let Plans { plans } = self;
        Saved {
            plans: plans.clone(),
        }
    }
}

/// The part of the checkpoint of this group: the plan of each
/// repository thread with the position and the time of its record, and
/// each hold with its reason, its `by` and its time. An empty part is
/// not written (01M43GSGVYJW7C09SVRWRAQZDZ, 01M4A4Z3QVRC57RE7M43ZRF4T2).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub(super) struct Saved {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    plans: BTreeMap<ThreadName, Part>,
}

impl Saved {
    pub(super) fn restore(self) -> Plans {
        let Saved { mut plans } = self;
        plans.retain(|_, part| !part.is_empty());
        Plans { plans }
    }
}

impl View<'_> {
    /// The hold of `item` in `thread`, when it is held.
    pub(super) fn hold(&self, thread: &ThreadName, item: &str) -> Option<&HoldInfo> {
        self.riff.plans().hold(thread, item)
    }

    /// The plan of `thread`, when its plan is on.
    pub(super) fn plan(&self, thread: &ThreadName) -> Option<&Current> {
        self.riff.plans().plan(thread)
    }

    /// True when no look saw the plan of `thread` for [`PLAN_TTL`]
    /// (01M4A4Z1QTHYXZDMCP9DZ39WVT). The time of the last look is in
    /// memory: with no look since the start of the server, the count
    /// starts at the start.
    pub(super) fn stale(&self, thread: &ThreadName, now: Instant) -> bool {
        let seen = self.presence.looks.get(thread).copied();
        match seen.or(self.presence.loaded) {
            Some(seen) => now.saturating_duration_since(seen) >= PLAN_TTL,
            None => true,
        }
    }

    /// The plan of the server for `thread`, and its holds
    /// (01M4A4Z1NKPDBXV2PRZCG86G6A).
    pub(super) fn plan_reply(&self, thread: &ThreadName, now: Now) -> PlanReply {
        let plan = self.plan(thread).map(|current| {
            let Current {
                plan,
                position,
                at_ms,
            } = current;
            let seen_ms = self.presence.looks.get(thread).map(|seen| {
                let ago = now.at.saturating_duration_since(*seen).as_millis();
                now.ms
                    .saturating_sub(u64::try_from(ago).unwrap_or(u64::MAX))
            });
            let work = self.riff.work();
            let holders = plan
                .items
                .iter()
                .filter_map(|PlanItem { item, needs: _ }| {
                    let holder = work.holder(thread, item)?;
                    Some((item.clone(), self.uri(holder, now.at)))
                })
                .collect();
            PlanShown {
                plan: plan.clone(),
                position: *position,
                set_ms: *at_ms,
                seen_ms,
                stale: self.stale(thread, now.at),
                holders,
            }
        });
        let holds = self
            .riff
            .plans()
            .holds(thread)
            .map(|(item, hold)| (item.to_owned(), hold.clone()))
            .collect();
        PlanReply { plan, holds }
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

/// Refuses a thread that is not a repository thread.
fn repository(thread: &ThreadName) -> Result<(), Refused> {
    if thread.is_direct() || !thread.to_string().contains('/') {
        return Err(format!("{thread} is not a repository thread: name OWNER/REPO").into());
    }
    Ok(())
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
    repository(thread)?;
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
            me: _,
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
        let Free {
            thread,
            item,
            me: _,
        } = self;
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

/// The checks of a `plan` and a `plan_off`: the thread, and who may
/// (01M4A4YTWK68JDA0DKXX2HV4FA). [`permits`](super::permits) refuses a
/// worker.
fn may_plan(
    kind: CommandKind,
    thread: &ThreadName,
    caller: &Caller,
    view: &View<'_>,
) -> Result<(), Refused> {
    repository(thread)?;
    let admin = caller.role() >= Role::Admin;
    let in_thread =
        caller.class() == Class::Session && view.riff.threads().member(caller.who(), thread);
    if admin || in_thread {
        Ok(())
    } else {
        Err(Refused::new(
            Code::NotAllowed,
            format!("only a session in {thread}, the owner or an admin can send {kind}"),
        ))
    }
}

/// True when `name` is `issue-N`: `issue-` and a whole number.
///
/// ```
/// use riff_server::state::plan::is_issue;
///
/// assert!(is_issue("issue-12"));
/// assert!(!is_issue("issue-"));
/// assert!(!is_issue("issue-1a"));
/// assert!(!is_issue("verify-issue-12"));
/// ```
pub fn is_issue(name: &str) -> bool {
    name.strip_prefix("issue-")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Checks the form of a plan (01M4A4YTTB24XNB4G49675QMHT): each item,
/// each need and each done need is `issue-N`, and no item is twice in
/// the items.
fn check_form(plan: &Plan) -> Result<(), Refused> {
    let Plan {
        wave: _,
        items,
        done,
    } = plan;
    let mut seen = BTreeSet::new();
    for PlanItem { item, needs: _ } in items {
        if !seen.insert(item.as_str()) {
            return Err(format!("{item} is twice in the items of the plan").into());
        }
    }
    let names = items
        .iter()
        .flat_map(|PlanItem { item, needs }| std::iter::once(item).chain(needs))
        .chain(done);
    match names.into_iter().find(|name| !is_issue(name)) {
        Some(bad) => Err(format!("{bad} is not a name of the form issue-N").into()),
        None => Ok(()),
    }
}

/// The reason of a `stale_base` (01M4A4YTR2NKVBPE6BT9EC3X75): the
/// position of the plan of the server, and the `base` of the call.
///
/// ```
/// use riff_server::state::plan::stale_base;
///
/// let thread = "acme/app".parse()?;
/// assert_eq!(
///     stale_base(&thread, Some(3050), Some(3001)),
///     "the plan of acme/app on the server is at position 3050, and the base is 3001. \
///      Read the plan with the query plan, and compare again."
/// );
/// assert_eq!(
///     stale_base(&thread, None, Some(3001)),
///     "acme/app has no plan on the server, and the base is 3001. \
///      Read the plan with the query plan, and compare again."
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn stale_base(thread: &ThreadName, server: Option<u64>, base: Option<u64>) -> String {
    let server = match server {
        Some(position) => format!("the plan of {thread} on the server is at position {position}"),
        None => format!("{thread} has no plan on the server"),
    };
    let base = base.map_or_else(|| "no plan".to_owned(), |base| base.to_string());
    format!(
        "{server}, and the base is {base}. Read the plan with the query plan, and compare again."
    )
}

/// Sets the plan of a repository thread (01M4A4YTNSJR0R1T9JNXPBSKHC).
/// A `base` that is not the position of the plan of the server is
/// refused with `stale_base`. A plan equal to the plan of the server
/// makes no record. Each `plan` that is not refused is a look of the
/// plan (01M4A4YTR2NKVBPE6BT9EC3X75).
impl Command for SetPlan {
    const KIND: CommandKind = CommandKind::Plan;
    type Reply = PlanReply;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let SetPlan {
            me: _,
            base,
            plan: PlanSet { thread, plan },
        } = self;
        may_plan(Self::KIND, thread, caller, view)?;
        check_form(plan)?;
        let current = view.plan(thread);
        let position = current.map(|current| current.position);
        if *base != position {
            return Err(Refused::new(
                Code::StaleBase,
                stale_base(thread, position, *base),
            ));
        }
        let mut changes = Vec::new();
        if current.map(|current| &current.plan) != Some(plan) {
            changes.push(Change::PlanSet(PlanSet {
                thread: thread.clone(),
                plan: plan.clone(),
            }));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, view: &View<'_>, _: &Done, (): (), now: Now) -> PlanReply {
        view.plan_reply(&self.plan.thread, now)
    }

    fn signal(&self, _: &Caller) -> Option<Signal> {
        Some(Signal::PlanSeen {
            thread: self.plan.thread.clone(),
        })
    }
}

/// The server forgets the plan of a repository thread. The holds stay.
/// A `plan_off` with no plan makes no record (01M4A4YTNSJR0R1T9JNXPBSKHC).
impl Command for PlanOff {
    const KIND: CommandKind = CommandKind::PlanOff;
    type Reply = PlanOffReply;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let PlanOff { me: _, thread } = self;
        may_plan(Self::KIND, thread, caller, view)?;
        let mut changes = Vec::new();
        if view.plan(thread).is_some() {
            changes.push(Change::PlanEnded(PlanEnded {
                thread: thread.clone(),
            }));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, done: &Done, (): (), _: Now) -> PlanOffReply {
        PlanOffReply {
            ended: !done.made.is_empty(),
        }
    }
}
