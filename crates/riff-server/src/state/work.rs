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
//!   `View::is_lead` and `View::lead_if_first`.

use std::collections::BTreeMap;
use std::time::Instant;

use riff_core::name::{Place, SessionUri, ThreadName, Who, check};
use riff_core::record::{Change, Claimed, Member, Record};
use riff_core::wire::{
    Claim, ClaimReply, Kind, Lead, LeadReply, Message, Release, ReleaseFor, RiffState,
};
use serde::{Deserialize, Serialize};

use super::command::{Caller, Code, Command, CommandKind, Now, Refused};
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
    /// (RID_TAKEN) has such records.
    pub(super) fn claimed(&mut self, claimed: &Claimed) -> Result<(), &'static str> {
        let key = (claimed.thread.clone(), claimed.item.clone());
        self.claims.insert(key, claimed.session.who().clone());
        Ok(())
    }

    /// The item is free, when the session of the record holds it.
    pub(super) fn released(&mut self, claimed: &Claimed) -> Result<(), &'static str> {
        let key = (claimed.thread.clone(), claimed.item.clone());
        if self.claims.get(&key) == Some(claimed.session.who()) {
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
        Saved {
            claims: self
                .claims
                .iter()
                .map(|((thread, item), holder)| SavedClaim {
                    thread: thread.clone(),
                    item: item.clone(),
                    holder: holder.clone(),
                })
                .collect(),
            leads: self
                .leads
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Saved {
    #[serde(default)]
    claims: Vec<SavedClaim>,
    #[serde(default)]
    leads: Vec<SavedLead>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SavedClaim {
    thread: ThreadName,
    item: String,
    holder: Who,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SavedLead {
    thread: ThreadName,
    lead: Who,
}

impl Saved {
    pub(super) fn restore(self) -> Work {
        let mut work = Work::default();
        for c in self.claims {
            work.claims.insert((c.thread, c.item), c.holder);
        }
        for l in self.leads {
            work.leads
                .insert((l.lead.user().to_owned(), l.thread), l.lead);
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

    pub(super) fn holds_claim(&self, who: &Who) -> bool {
        self.riff.work().claims.values().any(|holder| holder == who)
    }

    /// The lead change that makes `who` the lead, when it arrives at
    /// `place` in the repository thread `thread`, its user has no lead
    /// there, and no other session of the user there holds (R176). A
    /// lead that arrives again stays the lead.
    pub(super) fn lead_if_first(
        &self,
        who: &Who,
        place: &Place,
        thread: &ThreadName,
        now: Instant,
    ) -> Option<Change> {
        who.session()?;
        let sessions = &self.presence.sessions;
        let key = (who.user().to_owned(), thread.clone());
        let lead = self.riff.work().leads.get(&key);
        if lead == Some(who) || self.lead_of(&key, now).is_some() {
            return None;
        }
        let others = sessions.iter().any(|(other, s)| {
            other != who
                && other.user() == who.user()
                && other.session().is_some()
                && s.place.default_thread().as_ref() == Some(thread)
                && self.holds(other, now)
        });
        (!others).then(|| {
            Change::LeadSet(Member {
                session: SessionUri::new(who.clone(), place.clone()),
                thread: key.1,
            })
        })
    }

    /// One release for each claim of `who`.
    pub(super) fn released_all(&self, who: &Who) -> Vec<Change> {
        self.riff
            .work()
            .claims
            .iter()
            .filter(|(_, holder)| *holder == who)
            .map(|((thread, item), _)| {
                Change::Released(Claimed {
                    session: self.plain(who),
                    thread: thread.clone(),
                    item: item.clone(),
                })
            })
            .collect()
    }
}

/// Takes a claim if nobody holds it, or if its holder stopped more than
/// [`CLAIM_GRACE`](super::CLAIM_GRACE) ago. While the riff is paused, a
/// claim fails (01M3JCG3WBHDF0ZWM06XV94ZDC). A claim of an item that
/// another session holds is refused with the code `held`, and the
/// reason names the holder (01M3WRD9JBQMNN96TXJH8EAJ3W). A claim that
/// takes the item of a holder that is gone gives a `released` record
/// for the old holder, then the `claimed` record (RID_TAKEN).
impl Command for Claim {
    const KIND: CommandKind = CommandKind::Claim;
    type Reply = ClaimReply;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let who = caller.who();
        let Claim { thread, item, .. } = self;
        check("claim", item).map_err(|e| e.to_string())?;
        if view.riff.the_riff().state == RiffState::Paused {
            return Err(Refused::new(
                Code::Paused,
                format!(
                    "the riff is paused, so nobody claims {item}. Wait until your user or \
                     the lead resumes it."
                ),
            ));
        }
        let claim = |holder: &Who| Claimed {
            session: view.plain(holder),
            thread: thread.clone(),
            item: item.clone(),
        };
        let mut changes = Vec::new();
        match view.riff.work().holder(thread, item) {
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
            // chunk (RID_TAKEN).
            Some(gone) => {
                changes.push(Change::Released(claim(gone)));
                changes.push(Change::Claimed(claim(who)));
            }
            None => changes.push(Change::Claimed(claim(who))),
        }
        Ok((changes, ()))
    }

    fn reply(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _: &[Record],
        (): (),
        now: Now,
    ) -> ClaimReply {
        ClaimReply {
            holder: view.uri(caller.who(), now.at),
        }
    }
}

/// Frees a claim. Only its holder can.
impl Command for Release {
    const KIND: CommandKind = CommandKind::Release;
    type Reply = ();
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let who = caller.who();
        let Release { thread, item, .. } = self;
        match view.riff.work().holder(thread, item) {
            Some(holder) if holder == who => Ok((
                vec![Change::Released(Claimed {
                    session: view.plain(who),
                    thread: thread.clone(),
                    item: item.clone(),
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

    fn reply(&self, _: &Caller, _: &View<'_>, _: &[Record], (): (), _: Now) {}
}

/// The lead of a user frees the claim of another session of that user
/// (01M3WG243BW7P6E1ME0DFNQF8C). `session` is the session ID of the
/// holder, or the start of it. The holder can be live, gone or ended.
///
/// The command also makes the note of the server in the thread of the
/// claim: a `posted` record after the `released` record, in the chunk
/// of the command (01M3WRD9MGSC3FTBAANT4ZSMKY). The note names the
/// lead, the item and the holder.
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
            ..
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
        let mut changes = vec![Change::Released(Claimed {
            session: view.plain(holder),
            thread: thread.clone(),
            item: item.clone(),
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
        let (mut put, _) = view.put(server.who(), thread.clone(), note, now.at);
        changes.append(&mut put);
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, made: &[Record], (): (), _: Now) {
        for record in made {
            if let Change::Posted(posted) = &record.change {
                tracing::info!("{}", posted.message.body);
            }
        }
    }
}

/// Makes the session the lead of its user in its repository. It
/// replaces the old lead (R177). The note is the old lead, if another
/// session was the lead. [`permits`](super::permits) refuses a person
/// and a worker.
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
        let who = caller.who();
        let thread = view
            .place(who)
            .default_thread()
            .ok_or("the lead needs a git repository. Run it in a repository.")?;
        let key = (who.user().to_owned(), thread.clone());
        let lead = view.lead_of(&key, now.at);
        let old = lead.filter(|old| *old != who).cloned();
        let mut changes = Vec::new();
        if lead != Some(who) {
            changes.push(Change::LeadSet(Member {
                session: view.plain(who),
                thread,
            }));
        }
        Ok((changes, old))
    }

    fn reply(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _: &[Record],
        old: Option<Who>,
        now: Now,
    ) -> LeadReply {
        LeadReply {
            lead: view.uri(caller.who(), now.at),
            replaced: old.map(|old| view.uri(&old, now.at)),
        }
    }
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
/// use riff_server::state::released_for;
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

/// True when `id` names the session `session`: the whole session ID, or
/// a start of it of 4 or more characters.
fn names(id: &str, session: &str) -> bool {
    id == session || (id.len() >= 4 && session.starts_with(id))
}
