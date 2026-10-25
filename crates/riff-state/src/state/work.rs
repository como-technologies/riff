//! The group "work": the commands [`Claim`], [`Release`],
//! [`ReleaseFor`] and [`Lead`], the claims and the leads. The wire type
//! of each command is its command type.
//!
//! - Part of the riff: [`Work`]. The holder of each claimed item, and
//!   the lead of each user in each repository thread (R175).
//! - `apply`: `Work::claimed`, `Work::released` and
//!   `Work::lead_set` for the records of these names. `Work::left`
//!   for a `left_thread` record, and `Work::forgotten` for a
//!   `session_forgotten` record: see [`super::riff`].
//! - Checkpoint: `Saved`, the fields `claims` and `leads`.
//! - The rules of a lead for `handle`: `View::lead_of`,
//!   `View::is_lead` and `View::new_lead`.

use std::collections::BTreeMap;
use std::time::Instant;

use riff_core::name::{SessionUri, ThreadName, Who, check};
use riff_core::record::{Change, Claimed, Member, Released, Scope};
use riff_core::selector::Selector;
use riff_core::wire::{
    Claim, ClaimReply, Freed, Kind, Lead, LeadReply, Message, Release, ReleaseFor, ReleaseReply,
    StartReason,
};
use serde::{Deserialize, Serialize};

use super::command::{Caller, Code, Command, CommandKind, Done, Now, Refused};
use super::plan::held_text;
use super::view::View;

/// The claims and the leads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Work {
    pub(super) claims: BTreeMap<(ThreadName, String), Who>,
    /// The lead of each user in each repository thread (R175).
    pub(super) leads: BTreeMap<(String, ThreadName), Who>,
}

impl Work {
    /// The session of the record holds the item. It replaces the old
    /// holder: a log from before the `released` record of a taken item
    /// (01M3X4Z6BKM251H7CS2CEGR205) has such records.
    pub(super) fn claimed(&mut self, claimed: &Claimed) -> Result<(), &'static str> {
        let key = (claimed.thread.clone(), claimed.item.clone());
        self.claims.insert(key, claimed.session.who().clone());
        Ok(())
    }

    /// The item is free, when the session of the record holds it.
    pub(super) fn released(&mut self, released: &Released) -> Result<(), &'static str> {
        let key = (released.thread.clone(), released.item.clone());
        if self.claims.get(&key) == Some(released.session.who()) {
            self.claims.remove(&key);
            Ok(())
        } else {
            Err("the session does not hold the claim")
        }
    }

    /// The session of the record is the lead of its user in the thread.
    pub(super) fn lead_set(&mut self, member: &Member) -> Result<(), &'static str> {
        let who = member.session.who();
        let key = (who.user().to_owned(), member.thread.clone());
        self.leads.insert(key, who.clone());
        Ok(())
    }

    /// A session that leaves a thread is no longer the lead there. True
    /// when it was the lead.
    pub(super) fn left(&mut self, member: &Member) -> bool {
        let who = member.session.who();
        let key = (who.user().to_owned(), member.thread.clone());
        let lead = self.leads.get(&key) == Some(who);
        if lead {
            self.leads.remove(&key);
        }
        lead
    }

    /// Drops the claims and the lead of a forgotten session.
    pub(super) fn forgotten(&mut self, who: &Who) {
        self.claims.retain(|_, holder| holder != who);
        self.leads.retain(|_, lead| lead != who);
    }

    /// The holder of `item` in `thread`.
    pub(super) fn holder(&self, thread: &ThreadName, item: &str) -> Option<&Who> {
        self.claims.get(&(thread.clone(), item.to_owned()))
    }

    /// Each item that `who` holds.
    pub(super) fn items_of(&self, who: &Who) -> Vec<String> {
        self.claims
            .iter()
            .filter(|(_, holder)| *holder == who)
            .map(|((_, item), _)| item.clone())
            .collect()
    }

    /// The claims and the leads, for a checkpoint.
    pub(super) fn saved(&self) -> Saved {
        let Work { claims, leads } = self;
        Saved {
            claims: claims
                .iter()
                .map(|((thread, item), holder)| SavedClaim {
                    thread: thread.clone(),
                    item: item.clone(),
                    holder: holder.clone(),
                })
                .collect(),
            leads: leads
                .iter()
                .map(|((_, thread), lead)| SavedLead {
                    thread: thread.clone(),
                    lead: lead.clone(),
                })
                .collect(),
        }
    }
}

/// The part of the checkpoint of this group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub(super) struct Saved {
    #[serde(default)]
    claims: Vec<SavedClaim>,
    #[serde(default)]
    leads: Vec<SavedLead>,
}

/// A claim: the map entry of [`Work::claims`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
struct SavedClaim {
    thread: ThreadName,
    item: String,
    holder: Who,
}

/// A lead: the map entry of [`Work::leads`]. The user of the key is
/// the user of the lead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
struct SavedLead {
    thread: ThreadName,
    lead: Who,
}

impl Saved {
    pub(super) fn restore(self) -> Work {
        let Saved { claims, leads } = self;
        let mut work = Work::default();
        for SavedClaim {
            thread,
            item,
            holder,
        } in claims
        {
            work.claims.insert((thread, item), holder);
        }
        for SavedLead { thread, lead } in leads {
            work.leads.insert((lead.user().to_owned(), thread), lead);
        }
        work
    }
}

impl View<'_> {
    /// The lead of a user in a repository thread, while it holds and
    /// works in that repository.
    pub(super) fn lead_of(&self, key: &(String, ThreadName), now: Instant) -> Option<&Who> {
        self.riff.work().leads.get(key).filter(|who| {
            self.holds(who, now)
                && self
                    .presence
                    .sessions
                    .get(*who)
                    .is_some_and(|s| s.place.default_thread().as_ref() == Some(&key.1))
        })
    }

    pub(super) fn is_lead(&self, who: &Who, now: Instant) -> bool {
        self.presence
            .sessions
            .get(who)
            .and_then(|s| s.place.default_thread())
            .and_then(|thread| self.lead_of(&(who.user().to_owned(), thread), now))
            == Some(who)
    }

    /// The direct message of the server that tells `holder` that the
    /// step `why` ended its claims `freed` (01M51JBEQRWWZKKP3GWW8QX96C).
    /// It is a message, so it wakes the holder. A holder that is gone,
    /// or that has no session ID, gets none: no session waits for it.
    pub(super) fn tell_dropped(
        &self,
        holder: &Who,
        freed: &[Freed],
        why: &Dropped,
        now: Now,
    ) -> Vec<Change> {
        if freed.is_empty() {
            return Vec::new();
        }
        self.tell(holder, dropped(why, freed), now)
    }

    /// The direct message `body` of the server to the session `who`. It
    /// is a message, so it wakes the session. A session that is gone,
    /// or a caller with no session ID, gets none: no session waits for
    /// it.
    fn tell(&self, who: &Who, body: String, now: Now) -> Vec<Change> {
        let Some(id) = who.session() else {
            return Vec::new();
        };
        if self.gone(who, now.at) {
            return Vec::new();
        }
        let server = crate::owner::server_uri();
        let message = Message {
            seq: 0,
            from: server.clone(),
            to: vec![Selector::session(id)],
            body,
            at_ms: now.ms,
            kind: Kind::Message,
            sig: None,
            payload: None,
        };
        let thread = ThreadName::direct(server.who(), who);
        self.put(server.who(), thread, message, None, now.at).0
    }

    /// One release for each claim of `who`.
    pub(super) fn released_all(&self, who: &Who) -> Vec<Change> {
        self.riff
            .work()
            .claims
            .iter()
            .filter(|(_, holder)| *holder == who)
            .map(|((thread, item), _)| {
                Change::Released(Released {
                    session: self.plain(who),
                    thread: thread.clone(),
                    item: item.clone(),
                    must_clear: false,
                })
            })
            .collect()
    }
}

/// Takes a claim if nobody holds it, or if its holder stopped more than
/// [`CLAIM_GRACE`](super::CLAIM_GRACE) ago. While the riff or the
/// repository of the thread is paused, a claim fails
/// (01M3JCG3WBHDF0ZWM06XV94ZDC). The refusal names the pause and who
/// set it. A claim of an item that
/// another session holds is refused with the code `held`, and the
/// reason names the holder (01M3WRD9JBQMNN96TXJH8EAJ3W). A claim that
/// takes the item of a holder that is gone gives a `released` record
/// for the old holder, then the `claimed` record (01M3X4Z6BKM251H7CS2CEGR205).
/// A worker that must clear its context is refused first, with the code
/// `must_clear` (01M3X9XAK1KPZZVM1AJR2H8DSS).
///
/// A claim of a held item by a worker is refused with the code
/// `on_hold`, and the reason names the lead, the time and the reason of
/// the hold (01M43GSGPJ69TPWPA4935WR8RW). Each other session gets the
/// claim, with the same text as a warning in the reply. The order of
/// the checks: `must_clear`, the name of the item, `paused`, the caller
/// holds the item already, `on_hold`, `held`.
impl Command for Claim {
    const KIND: CommandKind = CommandKind::Claim;
    type Reply = ClaimReply;
    /// The warning for a caller that is not a worker.
    type Note = Option<String>;

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, Option<String>), Refused> {
        let who = caller.who();
        let Claim {
            thread,
            item,
            me: _,
        } = self;
        if view.riff.sessions().must_clear(who) {
            return Err(Refused::new(Code::MustClear, MUST_CLEAR));
        }
        check("claim", item).map_err(|e| e.to_string())?;
        if let Some((scope, pause)) = view.riff.the_riff().pauses.check(thread) {
            let by = pause
                .by
                .as_ref()
                .map_or_else(String::new, |by| format!(" by {by}"));
            let who = match scope {
                Scope::Riff => "the owner or an admin resumes it with `riff resume --riff`",
                Scope::Repository(_) | Scope::Other => "your user or the lead resumes it",
            };
            return Err(Refused::new(
                Code::Paused,
                format!("{scope} is paused{by}, so nobody claims {item}. Wait until {who}."),
            ));
        }
        let claim = |holder: &Who| Claimed {
            session: view.plain(holder),
            thread: thread.clone(),
            item: item.clone(),
        };
        let holder = view.riff.work().holder(thread, item);
        if holder == Some(who) {
            return Ok((Vec::new(), None));
        }
        let warning = view.hold(thread, item).map(|hold| held_text(item, hold));
        if let Some(text) = &warning
            && caller.worker()
        {
            return Err(Refused::new(
                Code::OnHold,
                format!("{text}. Pick another item."),
            ));
        }
        let mut changes = Vec::new();
        match holder {
            Some(holder) if holder == who => {}
            Some(holder) if view.holds(holder, now.at) => {
                return Err(Refused::new(
                    Code::Held,
                    format!(
                        "{} holds {item} in {thread}.",
                        label(&view.uri(holder, now.at))
                    ),
                ));
            }
            // The holder is gone: its claim ends first, in the same
            // chunk (01M3X4Z6BKM251H7CS2CEGR205).
            Some(gone) => {
                changes.push(Change::Released(Released::of(claim(gone))));
                changes.push(Change::Claimed(claim(who)));
            }
            None => changes.push(Change::Claimed(claim(who))),
        }
        Ok((changes, warning))
    }

    fn reply(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _: &Done,
        warning: Option<String>,
        now: Now,
    ) -> ClaimReply {
        ClaimReply {
            holder: view.uri(caller.who(), now.at),
            warning,
        }
    }
}

/// The reason of a claim that is refused with the code `must_clear`
/// (01M3X9XAK1KPZZVM1AJR2H8DSS).
pub const MUST_CLEAR: &str =
    "clear your context first: end your turn and riff clears it, or type /clear";

/// Frees a claim. Only its holder can. The `released` record of the last
/// claim of a worker has `must_clear`: the worker must clear its context
/// before its next claim (01M3X9XAK1KPZZVM1AJR2H8DSS). The reply carries the ask to clear
/// (01M3X9XB37TQCXWPNFZRMRGJB4).
impl Command for Release {
    const KIND: CommandKind = CommandKind::Release;
    type Reply = ReleaseReply;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let who = caller.who();
        let Release {
            thread,
            item,
            me: _,
        } = self;
        match view.riff.work().holder(thread, item) {
            Some(holder) if holder == who => Ok((
                vec![Change::Released(Released {
                    session: view.plain(who),
                    thread: thread.clone(),
                    item: item.clone(),
                    must_clear: caller.worker() && view.riff.work().items_of(who).len() == 1,
                })],
                (),
            )),
            Some(holder) => Err(Refused::new(
                Code::NotHolder,
                format!("{item} is held by {}", view.uri(holder, now.at).short()),
            )),
            None => Err(Refused::new(
                Code::NotHolder,
                format!("nobody holds {item}"),
            )),
        }
    }

    fn reply(&self, _: &Caller, _: &View<'_>, done: &Done, (): (), _: Now) -> ReleaseReply {
        let must_clear = done
            .made
            .iter()
            .any(|record| matches!(&record.change, Change::Released(r) if r.must_clear));
        ReleaseReply { must_clear }
    }
}

/// The lead of a user frees the claim of another session of that user
/// (01M3WG243BW7P6E1ME0DFNQF8C). `session` is the session ID of the
/// holder, or the start of it. The holder can be live, gone or ended.
///
/// The command also makes the note of the server in the thread of the
/// claim: a `posted` record after the `released` record, in the chunk
/// of the command (01M3WRD9MGSC3FTBAANT4ZSMKY). The note names the
/// lead, the item and the holder. A live holder that is not the lead
/// also gets a direct message of the server that wakes it, after the
/// note: see [`dropped`] (01M51JBEQRWWZKKP3GWW8QX96C).
impl Command for ReleaseFor {
    const KIND: CommandKind = CommandKind::ReleaseFor;
    type Reply = ();
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let who = caller.who();
        let ReleaseFor {
            thread,
            item,
            session: id,
            me: _,
        } = self;
        let Some(holder) = view.riff.work().holder(thread, item) else {
            return Err(Refused::new(
                Code::NotHolder,
                format!("nobody holds {item}"),
            ));
        };
        let held_by = view.uri(holder, now.at).short();
        if !holder.session().is_some_and(|s| names(id, s)) {
            return Err(Refused::new(
                Code::NotHolder,
                format!("{item} is held by {held_by}, not by the session {id}"),
            ));
        }
        let sessions = &view.presence.sessions;
        let repo = |who: &Who| sessions.get(who).map(|s| s.place.default_thread());
        if holder.user() != who.user() || repo(holder) != repo(who) {
            return Err(Refused::new(
                Code::NotAllowed,
                format!(
                    "{item} is held by {held_by}. Only the lead of its user in its \
                     repository frees it."
                ),
            ));
        }
        if holder != who && !view.is_lead(who, now.at) {
            return Err(Refused::new(
                Code::NotAllowed,
                format!(
                    "{item} is held by {held_by}. Only the lead of your user frees the \
                     claim of another session. Tell the lead."
                ),
            ));
        }
        let mut changes = vec![Change::Released(Released {
            session: view.plain(holder),
            thread: thread.clone(),
            item: item.clone(),
            must_clear: false,
        })];
        let server = crate::owner::server_uri();
        let news = released_for(&view.uri(who, now.at), item, &view.uri(holder, now.at));
        let note = Message {
            seq: 0,
            from: server.clone(),
            to: Vec::new(),
            body: news,
            at_ms: now.ms,
            kind: Kind::Note,
            sig: None,
            payload: None,
        };
        let (mut put, _) = view.put(server.who(), thread.clone(), note, None, now.at);
        changes.append(&mut put);
        if holder != who {
            let freed = [Freed {
                thread: thread.clone(),
                item: item.clone(),
            }];
            let why = Dropped::Lead(view.uri(who, now.at));
            changes.extend(view.tell_dropped(holder, &freed, &why, now));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, done: &Done, (): (), _: Now) {
        for record in &done.made {
            if let Change::Posted(posted) = &record.change {
                tracing::info!("{}", posted.message.body);
            }
        }
    }
}

/// The person names the session `session` the lead of the user in its
/// repository (01M5NJAXTQ0YKN7Q7XN0A7ZMPF). It replaces the old lead,
/// and tells it (01M5NJAYMYF50B9WXH5RF9W737). The note is the old lead,
/// if another session was the lead. [`permits`](super::permits)
/// refuses a session, and `handle` refuses a worker.
impl Command for Lead {
    const KIND: CommandKind = CommandKind::Lead;
    type Reply = LeadReply;
    type Note = Option<Who>;

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, Option<Who>), Refused> {
        let thread = caller
            .me()
            .place()
            .default_thread()
            .ok_or("the lead needs a git repository. Run it in a repository.")?;
        let who = view.new_lead(caller.who().user(), &thread, &self.session)?;
        let key = (who.user().to_owned(), thread.clone());
        let lead = view.lead_of(&key, now.at);
        let old = lead.filter(|old| **old != who).cloned();
        let mut changes = Vec::new();
        if lead != Some(&who) {
            changes.push(Change::LeadSet(Member {
                session: view.plain(&who),
                thread,
            }));
        }
        // The old lead hears of it (01M5NJAYMYF50B9WXH5RF9W737).
        if let Some(old) = &old {
            changes.extend(view.tell(old, not_lead(&view.uri(&who, now.at)), now));
        }
        Ok((changes, old))
    }

    fn reply(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _: &Done,
        old: Option<Who>,
        now: Now,
    ) -> LeadReply {
        let lead = caller.me().place().default_thread().and_then(|thread| {
            view.new_lead(caller.who().user(), &thread, &self.session)
                .ok()
        });
        LeadReply {
            lead: view.uri(lead.as_ref().unwrap_or(caller.who()), now.at),
            replaced: old.map(|old| view.uri(&old, now.at)),
        }
    }
}

impl View<'_> {
    /// The session of `user` in `thread` that the session ID `id`, or
    /// its start, names: the new lead of a [`Lead`]. A worker cannot be
    /// the lead.
    fn new_lead(&self, user: &str, thread: &ThreadName, id: &str) -> Result<Who, Refused> {
        let found: Vec<&Who> = self
            .presence
            .sessions
            .iter()
            .filter(|(who, s)| {
                who.user() == user
                    && who.session().is_some_and(|s| names(id, s))
                    && s.place.default_thread().as_ref() == Some(thread)
            })
            .map(|(who, _)| who)
            .collect();
        let who = match found[..] {
            [who] => who.clone(),
            [] => {
                return Err(Refused::new(
                    Code::BadRequest,
                    format!("{user} has no session {id} in {thread}. Run riff who."),
                ));
            }
            _ => {
                return Err(Refused::new(
                    Code::BadRequest,
                    format!("more than one session of {user} starts with {id}. Give more of it."),
                ));
            }
        };
        if self.riff.sessions().worker(&who) {
            return Err(Refused::new(
                Code::NotAllowed,
                "a worker cannot be the lead. Make another session the lead.",
            ));
        }
        Ok(who)
    }
}

/// The text of the message to an old lead: the person made `lead` the
/// lead.
///
/// ```
/// use riff_core::name::SessionUri;
/// use riff_state::state::not_lead;
///
/// let lead: SessionUri = "riff://mike@thelio/como-technologies/riff?session=b2c3d4e5-ffff".parse()?;
/// assert_eq!(
///     not_lead(&lead),
///     "lead: you are not the lead now. Your person made the session mike@thelio:riff \
///      (b2c3d4e5) the lead. Stop the work of the lead, and tell your person."
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn not_lead(lead: &SessionUri) -> String {
    format!(
        "lead: you are not the lead now. Your person made the session {} the lead. Stop the \
         work of the lead, and tell your person.",
        label(lead)
    )
}

/// The name of a session for people: its short form, and the start of
/// its session ID.
fn label(uri: &SessionUri) -> String {
    match uri.who().session() {
        Some(id) => format!(
            "{} ({})",
            uri.short(),
            id.chars().take(8).collect::<String>()
        ),
        None => uri.short(),
    }
}

/// The note of the server in the thread of a claim that the lead `lead`
/// freed for the session `holder` (01M3WG243BW7P6E1ME0DFNQF8C). It names
/// the lead, the item and the holder.
///
/// ```
/// use riff_core::name::SessionUri;
/// use riff_state::state::released_for;
///
/// let lead: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=518bd482-fcc4&lead=true".parse()?;
/// let holder: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=068a2cc2-11aa#issue-347".parse()?;
/// assert_eq!(
///     released_for(&lead, "issue-347", &holder),
///     "claims: the lead mike@pangolin:riff (518bd482) released issue-347 for the session \
///      mike@pangolin:riff#issue-347 (068a2cc2). issue-347 is free."
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn released_for(lead: &SessionUri, item: &str, holder: &SessionUri) -> String {
    let id = |uri: &SessionUri| -> String {
        let id = uri.who().session().unwrap_or_default();
        id.chars().take(8).collect()
    };
    format!(
        "claims: the lead {} ({}) released {item} for the session {} ({}). {item} is free.",
        lead.short(),
        id(lead),
        holder.short(),
        id(holder)
    )
}

/// The step that ended the claims of a live session, when it is not the
/// release of that session (01M51JBEQRWWZKKP3GWW8QX96C).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dropped {
    /// A new start of the session, with its reason
    /// (01M3JEE1QQCFS5TMZW5N2DAD2D).
    Start(StartReason),
    /// The lead released the claim for the session
    /// (01M3WG243BW7P6E1ME0DFNQF8C).
    Lead(SessionUri),
}

/// The text of the direct message of the server to a live session
/// whose claims `freed` the step `why` ended
/// (01M51JBEQRWWZKKP3GWW8QX96C). It names each claim, the reason and
/// the step to take.
///
/// ```
/// use riff_core::name::SessionUri;
/// use riff_core::wire::{Freed, StartReason};
/// use riff_state::state::{Dropped, dropped};
///
/// let freed = |item: &str| Freed {
///     thread: "como-technologies/riff".parse().unwrap(),
///     item: item.into(),
/// };
/// assert_eq!(
///     dropped(&Dropped::Start(StartReason::Clear), &[freed("issue-12")]),
///     "claims: a new start of this session (/clear) freed your claim issue-12 in \
///      como-technologies/riff. Another session can take it. To go on with it, claim \
///      it again, then see \"Pick up dropped work\" in the riff skill. Else stop the \
///      work on it."
/// );
/// let two = [freed("issue-12"), freed("verify-issue-7")];
/// let text = dropped(&Dropped::Start(StartReason::Process), &two);
/// assert!(text.starts_with(
///     "claims: a new start of this session (a new process) freed your claims issue-12 \
///      in como-technologies/riff, verify-issue-7 in como-technologies/riff. Another \
///      session can take them. To go on with one,"
/// ), "{text}");
///
/// let lead: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=518bd482-fcc4&lead=true".parse()?;
/// assert_eq!(
///     dropped(&Dropped::Lead(lead), &[freed("issue-12")]),
///     "claims: the lead mike@pangolin:riff (518bd482) released your claim issue-12 in \
///      como-technologies/riff. Another session can take it. Stop the work on it: push \
///      your work as WIP, and write the state on the issue. Claim it again only when \
///      the lead asks you to."
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn dropped(why: &Dropped, freed: &[Freed]) -> String {
    let items: Vec<String> = freed
        .iter()
        .map(|f| format!("{} in {}", f.item, f.thread))
        .collect();
    let (claims, them, one) = match freed {
        [_] => ("claim", "it", "it"),
        _ => ("claims", "them", "one"),
    };
    let items = items.join(", ");
    match why {
        Dropped::Start(reason) => {
            let start = match reason {
                StartReason::Process => "a new process",
                StartReason::Clear => "/clear",
                other => other.word(),
            };
            format!(
                "claims: a new start of this session ({start}) freed your {claims} {items}. \
                 Another session can take {them}. To go on with {one}, claim it again, then \
                 see \"Pick up dropped work\" in the riff skill. Else stop the work on it."
            )
        }
        Dropped::Lead(lead) => format!(
            "claims: the lead {} released your {claims} {items}. Another session can take \
             {them}. Stop the work on it: push your work as WIP, and write the state on the \
             issue. Claim it again only when the lead asks you to.",
            label(lead)
        ),
    }
}

/// True when `id` names the session `session`: the whole session ID, or
/// a start of it of 4 or more characters.
fn names(id: &str, session: &str) -> bool {
    id == session || (id.len() >= 4 && session.starts_with(id))
}
