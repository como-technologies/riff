//! The group "people": who may join the riff, and with which role.
//!
//! - Part of the riff: [`People`]. The riff ID, the email of each USER
//!   (R209), the members, the admins that the owner made, the owner, a
//!   riff whose owner is gone, the request for the owner role, and the
//!   position of the last end of the sign-ins of each USER
//!   (01M3XA87A9GGFA89RQXWSKY0V6).
//! - `apply`: one method of [`People`] for each of the kinds
//!   `riff_made`, `person_joined`, `member_invited`, `member_removed`,
//!   `admin_set`, `owner_set`, `owner_asked`, `owner_denied` and
//!   `signins_ended`. Only they change the people: see [`super::riff`].
//! - Checkpoint: `Saved`, the fields `riff_id`, `users`, `members`,
//!   `admins`, `owner`, `no_owner`, `owner_asked` and `signins_ended`.
//! - Commands: the wire types [`Invite`], [`Remove`], [`SetAdmin`],
//!   [`PassOwner`], [`TakeOwner`], [`DenyOwner`] and [`Revoke`], the
//!   command [`Admit`] of the token path, and the commands
//!   [`GrantOwner`], [`EndOwner`] and [`NameOwner`] of the server. No
//!   client can send the last four.
//!
//! # The rules
//!
//! A person signs in with a verified email. The USER comes from the
//! email (R208). The first email that signs in with a USER holds it
//! (R209). [`Admit`] lets a person in when one of these is true:
//!
//! - The person is the owner, a member or an admin.
//! - The account is in an allowed domain (R15).
//! - The riff is new, with no owner and no admin: the person becomes
//!   the owner. A riff whose owner was gone is not new.
//!
//! ```mermaid
//! flowchart TD
//!     A[verified email] --> O{owner, member or admin?}
//!     O -- yes --> IN[sign in]
//!     O -- no --> D{allowed domain?}
//!     D -- yes --> IN
//!     D -- no --> N{no owner and no admin?}
//!     N -- yes --> IN
//!     N -- no --> R[refuse: ask the owner for an invite]
//!     IN --> F{no owner yet, and an admin or no admins?}
//!     F -- yes --> OW[the person is the owner]
//! ```
//!
//! - The first person that [`Admit`] lets in is the owner
//!   (01M3JN3AD44CC98AGMVP43F56G). On a riff with admins of the
//!   settings, only an admin becomes the owner. The owner is an admin.
//! - An admin adds a member with [`Invite`], and removes one with
//!   [`Remove`]. A removal ends each sign-in of that person (R20).
//! - The owner makes a person an admin, or an admin a member again,
//!   with [`SetAdmin`]. The admins of the settings (R210) add to the
//!   admins that the owner made. They are not in the log: the
//!   [`View`] holds them.
//! - The owner passes the owner role with [`PassOwner`]. An admin asks
//!   for it with [`TakeOwner`]. The owner answers with [`PassOwner`] or
//!   [`DenyOwner`], and [`GrantOwner`] grants the request when its time
//!   ends. [`EndOwner`] ends the role of an owner who is gone. A riff
//!   has one owner at a time. The old owner stays an admin: `handle`
//!   decides it, and gives the `member_invited` and `admin_set` records
//!   of the old owner before the `owner_set` record.
//! - [`Revoke`] ends each sign-in of one person. It leaves the USER of
//!   the person: only that email signs in as that USER again.
//!
//! See [`crate::owner`] for the times and the notes of the owner role.

use std::collections::{BTreeMap, BTreeSet};

use riff_core::name::{ThreadName, Who};
use riff_core::record::{self, Change, Email, OwnerSet, PersonJoined, RiffMade, SigninsEnded};
use riff_core::selector::Selector;
use riff_core::wire::{
    AdminSet, DenyOwner, Invite, Invited, Kind, MembersReply, Message, OwnerAsked, OwnerDenied,
    OwnerPassed, PassOwner, PersonRole, Remove, Removed, Revoke, Revoked, RiffOwner, SetAdmin,
    TakeOwner,
};
use serde::{Deserialize, Serialize};

use super::command::{Caller, Code, Command, CommandKind, Done, Now, Refused, Role};
use super::view::View;
use crate::oidc::user_of;
use crate::owner;
use crate::token::SERVER_USER;

/// The refusal of an action of the owner on a riff with no owner
/// (01M3Q63NNC6SC03BFCG80M7B4D).
pub const NO_OWNER: &str =
    "the riff has no owner; an admin takes the owner role with: riff owner --take";

/// An email as the people keep it: with no space around it, in lower
/// case.
///
/// ```
/// assert_eq!(riff_server::state::people::email(" Ada@X.io"), "ada@x.io");
/// ```
pub fn email(text: &str) -> String {
    text.trim().to_lowercase()
}

/// The people of the riff. Only `apply` changes them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct People {
    /// The ID of the riff, from its `riff_made` record
    /// (01M3JNVBPMZ1K9WX7Q7DP6Y0DH).
    riff_id: Option<String>,
    /// The verified email of each USER (R209).
    users: BTreeMap<String, String>,
    /// The email of each member.
    members: BTreeSet<String>,
    /// The email of each admin that the owner made.
    admins: BTreeSet<String>,
    /// The email of the owner.
    owner: Option<String>,
    /// True when the owner was gone and no admin took the role yet
    /// (01M3Q63NNC6SC03BFCG80M7B4D). A sign-in then makes no owner.
    no_owner: bool,
    /// The request for the owner role that waits for the owner
    /// (01M3N7K3ZAZFGABN7032AYJWEM).
    asked: Option<Asked>,
    /// The position of the last `member_removed` or `signins_ended`
    /// record of each USER. A sign-in that started before it is ended
    /// (01M3XA87A9GGFA89RQXWSKY0V6).
    ended: BTreeMap<String, u64>,
}

/// A request for the owner role that waits for the answer of the owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Asked {
    /// The email of the admin that asked.
    email: String,
    /// With no answer before this time, the admin is the owner. In
    /// milliseconds since the Unix epoch.
    due_ms: u64,
}

impl People {
    /// The riff has this ID. A riff keeps its first ID.
    pub(super) fn made(&mut self, made: &RiffMade) -> Result<(), &'static str> {
        if self.riff_id.is_some() {
            return Err("the riff has an ID already");
        }
        self.riff_id = Some(made.riff_id.clone());
        Ok(())
    }

    /// The email holds the USER. The first email keeps a USER (R209).
    pub(super) fn joined(&mut self, joined: &PersonJoined) -> Result<(), &'static str> {
        match self.users.get(&joined.user) {
            Some(held) if *held != joined.email => Err("another email holds the user"),
            _ => {
                self.users.insert(joined.user.clone(), joined.email.clone());
                Ok(())
            }
        }
    }

    /// The email is a member.
    pub(super) fn invited(&mut self, invited: &Email) -> Result<(), &'static str> {
        self.members.insert(invited.email.clone());
        Ok(())
    }

    /// The email is no member. Each sign-in of its USER that started
    /// before `position` is ended.
    pub(super) fn removed(&mut self, removed: &Email, position: u64) -> Result<(), &'static str> {
        self.members.remove(&removed.email);
        for user in self.users_of(&removed.email) {
            self.ended.insert(user, position);
        }
        Ok(())
    }

    /// The email is an admin that the owner made, or it is not.
    pub(super) fn admin_set(&mut self, set: &record::AdminSet) -> Result<(), &'static str> {
        if set.admin {
            self.admins.insert(set.email.clone());
        } else {
            self.admins.remove(&set.email);
        }
        Ok(())
    }

    /// The email is the owner. A record with no email says that the
    /// owner is gone. The request that waits ends.
    pub(super) fn owner_set(&mut self, set: &OwnerSet) -> Result<(), &'static str> {
        self.owner.clone_from(&set.email);
        self.no_owner = set.email.is_none();
        self.asked = None;
        Ok(())
    }

    /// The email asks for the owner role.
    pub(super) fn owner_asked(&mut self, asked: &record::OwnerAsked) -> Result<(), &'static str> {
        self.asked = Some(Asked {
            email: asked.email.clone(),
            due_ms: asked.due_ms,
        });
        Ok(())
    }

    /// The owner keeps the role: the request ends.
    pub(super) fn owner_denied(&mut self, _: &Email) -> Result<(), &'static str> {
        match self.asked.take() {
            Some(_) => Ok(()),
            None => Err("no request for the owner role waits"),
        }
    }

    /// Each sign-in of the USER that started before `position` is ended.
    pub(super) fn signins_ended(
        &mut self,
        ended: &SigninsEnded,
        position: u64,
    ) -> Result<(), &'static str> {
        self.ended.insert(ended.user.clone(), position);
        Ok(())
    }

    /// The ID of the riff, when it has one.
    pub fn riff_id(&self) -> Option<&str> {
        self.riff_id.as_deref()
    }

    /// The email of the owner, or `None` when the riff has no owner.
    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    /// True when the riff has an owner, or had one: a riff whose owner
    /// was gone counts (01M3JN3AQMHZHT6JP3P6GM9PWZ).
    pub fn owned(&self) -> bool {
        self.owner.is_some() || self.no_owner
    }

    /// The email of the admin whose request for the owner role waits.
    pub fn asks(&self) -> Option<&str> {
        self.asked.as_ref().map(|asked| asked.email.as_str())
    }

    /// True when a request waits and its time ended at `now_ms`.
    pub fn is_due(&self, now_ms: u64) -> bool {
        self.asked.as_ref().is_some_and(|a| a.due_ms <= now_ms)
    }

    /// The verified email that holds `user` (R209).
    pub fn email_of(&self, user: &str) -> Option<&str> {
        self.users.get(user).map(String::as_str)
    }

    /// The USER that holds `email`, or `None` when nobody signed in
    /// with it.
    pub fn user_of_email(&self, email: &str) -> Option<&str> {
        self.users
            .iter()
            .find(|(_, held)| *held == email)
            .map(|(user, _)| user.as_str())
    }

    /// Each USER that holds `email`.
    pub fn users_of(&self, email: &str) -> Vec<String> {
        self.users
            .iter()
            .filter(|(_, held)| *held == email)
            .map(|(user, _)| user.clone())
            .collect()
    }

    /// The position of the last end of the sign-ins of each USER
    /// (01M3XA87A9GGFA89RQXWSKY0V6). A sign-in that started before the
    /// position of its USER is ended.
    pub fn ended(&self) -> &BTreeMap<String, u64> {
        &self.ended
    }

    /// The people, for a checkpoint.
    pub(super) fn saved(&self) -> Saved {
        Saved {
            riff_id: self.riff_id.clone(),
            users: self.users.clone(),
            members: self.members.clone(),
            admins: self.admins.clone(),
            owner: self.owner.clone(),
            no_owner: self.no_owner,
            owner_asked: self.asked.clone(),
            signins_ended: self.ended.clone(),
        }
    }
}

/// The part of the checkpoint of this group. A field with nothing in
/// it is not written. So the checkpoint of a riff with no people has
/// the bytes of a checkpoint from before the people
/// (01M3WNQR41K41TV832GRQZ2CQS).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Saved {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    riff_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    users: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    members: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    admins: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    no_owner: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner_asked: Option<Asked>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    signins_ended: BTreeMap<String, u64>,
}

impl Saved {
    pub(super) fn restore(self) -> People {
        People {
            riff_id: self.riff_id,
            users: self.users,
            members: self.members,
            admins: self.admins,
            owner: self.owner,
            no_owner: self.no_owner,
            asked: self.owner_asked,
            ended: self.signins_ended,
        }
    }
}

/// The rules of the people for `handle` and for each query.
impl View<'_> {
    fn people(&self) -> &People {
        self.riff.people()
    }

    /// True when `email` is an admin: the owner, an admin that the owner
    /// made, or an admin of the settings (R210).
    fn is_admin(&self, email: &str) -> bool {
        let people = self.people();
        people.owner() == Some(email)
            || people.admins.contains(email)
            || self.settings.is_admin(email)
    }

    /// The role of `user`: the owner, an admin, or a member. A USER
    /// with no email is a member.
    pub fn role_of(&self, user: &str) -> Role {
        let people = self.people();
        match people.email_of(user) {
            Some(email) if people.owner() == Some(email) => Role::Owner,
            Some(email) if self.is_admin(email) => Role::Admin,
            _ => Role::Member,
        }
    }

    /// Each person once, with the highest role: the owner, the admins
    /// and the members, each sorted (01M3MN157X8N9QKER1AJEPEJVX). The
    /// admins of the settings add to the admins that the owner made.
    pub fn roles(&self) -> (Option<String>, Vec<String>, Vec<String>) {
        let people = self.people();
        let owner = people.owner.clone();
        let mut all: BTreeSet<String> = people.admins.clone();
        all.extend(self.settings.admins().iter().cloned());
        let not_owner = |email: &String| Some(email) != owner.as_ref();
        let members = people
            .members
            .iter()
            .filter(|m| not_owner(m) && !all.contains(*m))
            .cloned()
            .collect();
        let admins = all.into_iter().filter(not_owner).collect();
        (owner, admins, members)
    }

    /// Who may join the riff: the reply to `members`. The caller adds
    /// the allowed domains.
    pub fn members(&self) -> MembersReply {
        let (owner, admins, members) = self.roles();
        MembersReply {
            owner,
            admins,
            members,
            allowed_domains: Vec::new(),
        }
    }

    /// The USER and the role of each person with a USER, sorted by USER
    /// (01M3NT4M3A4E3K5S2NM7MS6PQD). A person who never signed in has
    /// no USER, and is not in it.
    pub fn persons(&self) -> Vec<(String, PersonRole)> {
        let (owner, admins, members) = self.roles();
        let people = self.people();
        let mut persons: Vec<(String, PersonRole)> = owner
            .into_iter()
            .map(|email| (email, PersonRole::Owner))
            .chain(admins.into_iter().map(|email| (email, PersonRole::Admin)))
            .chain(members.into_iter().map(|email| (email, PersonRole::Member)))
            .filter_map(|(email, role)| Some((people.user_of_email(&email)?.to_owned(), role)))
            .collect();
        persons.sort();
        persons
    }

    /// The owner for `who` (01M3Q63NK0AHM25MB258B0K8XP): the USER that
    /// holds the email of the owner, or else the USER that the email
    /// gives. [`RiffOwner::Nobody`] when the riff has no owner.
    pub fn riff_owner(&self) -> RiffOwner {
        let people = self.people();
        let Some(email) = people.owner.clone() else {
            return RiffOwner::Nobody;
        };
        let user = people
            .user_of_email(&email)
            .map(str::to_owned)
            .or_else(|| user_of(&email).ok())
            .unwrap_or_else(|| email.clone());
        RiffOwner::Owner { user, email }
    }

    /// The USER of the owner, or `None` when the riff has no owner or
    /// the owner never signed in.
    pub fn owner_user(&self) -> Option<&str> {
        let people = self.people();
        people.user_of_email(people.owner()?)
    }

    /// The USER of each admin that is not the owner, sorted.
    fn admin_users(&self) -> Vec<String> {
        let (_, admins, _) = self.roles();
        let people = self.people();
        people
            .users
            .iter()
            .filter(|(_, email)| admins.contains(email))
            .map(|(user, _)| user.clone())
            .collect()
    }

    /// True when the riff has an owner and one more admin: only then
    /// the server checks the owner (01M3Q546335NBTKG5BHQ27QC93).
    pub fn has_other_admin(&self) -> bool {
        let (owner, admins, _) = self.roles();
        owner.is_some() && !admins.is_empty()
    }

    /// The thread of each repository of the riff, sorted: the
    /// repository of each known session, also a gone one
    /// (01M3MN14ZCTRVD3T455P6TFK1B).
    pub(super) fn repositories(&self) -> Vec<ThreadName> {
        let threads: BTreeSet<ThreadName> = self
            .presence
            .sessions
            .values()
            .filter_map(|s| s.place.default_thread())
            .collect();
        threads.into_iter().collect()
    }

    /// The live lead of `user` in each repository, sorted.
    pub(super) fn live_leads(&self, user: &str, now: std::time::Instant) -> Vec<Who> {
        let leads: BTreeSet<Who> = self
            .riff
            .work()
            .leads
            .keys()
            .filter(|key| key.0 == user)
            .filter_map(|key| self.lead_of(key, now))
            .filter(|who| !self.gone(who, now))
            .cloned()
            .collect();
        leads.into_iter().collect()
    }

    /// The `posted` records of a note of the server about the people
    /// (01M3N7K4DVHSF7AQ402F14J26Z): one note in the thread of each
    /// repository of the riff, which wakes no session. Each live lead
    /// of `tell` gets the same text as a direct message. The records go
    /// in the chunk of the command that causes the note.
    fn news(&self, body: &str, tell: &[String], now: Now) -> Vec<Change> {
        let server = owner::server_uri();
        let message = |to: Vec<Selector>, body: String, kind: Kind| Message {
            seq: 0,
            from: server.clone(),
            to,
            body,
            at_ms: now.ms,
            kind,
            sig: None,
            payload: None,
        };
        let mut changes = Vec::new();
        for thread in self.repositories() {
            let to = vec![Selector {
                repo: Some(thread.to_string()),
                ..Selector::default()
            }];
            let note = message(to, body.to_owned(), Kind::Note);
            changes.append(&mut self.put(server.who(), thread, note, now.at).0);
        }
        let direct = owner::to_lead(body);
        for lead in tell.iter().flat_map(|user| self.live_leads(user, now.at)) {
            let Some(id) = lead.session() else {
                continue;
            };
            let to = vec![Selector::session(id)];
            let thread = ThreadName::direct(server.who(), &lead);
            let told = message(to, direct.clone(), Kind::Message);
            changes.append(&mut self.put(server.who(), thread, told, now.at).0);
        }
        changes
    }

    /// The records that keep `old`, the owner who loses the role, an
    /// admin and a member.
    fn keeps(&self, old: &str) -> Vec<Change> {
        let people = self.people();
        let mut changes = Vec::new();
        if !people.members.contains(old) {
            changes.push(invited(old));
        }
        if !people.admins.contains(old) {
            changes.push(admin_set(old, true));
        }
        changes
    }
}

fn invited(email: &str) -> Change {
    Change::MemberInvited(Email {
        email: email.to_owned(),
    })
}

fn admin_set(email: &str, admin: bool) -> Change {
    Change::AdminSet(record::AdminSet {
        email: email.to_owned(),
        admin,
    })
}

fn owner_set(email: Option<&str>) -> Change {
    Change::OwnerSet(OwnerSet {
        email: email.map(str::to_owned),
    })
}

/// The email of a call, as the people keep it. It refuses an email that
/// gives no USER (R208).
fn valid(text: &str) -> Result<String, Refused> {
    let email = email(text);
    user_of(&email).map_err(|e| Refused::new(Code::BadRequest, e.to_string()))?;
    Ok(email)
}

/// The sign-in of a person from the provider: the command of the token
/// path (01M3XA877YZQ649SWB5TN60V5P). The caller is the sign-in: the
/// verified email, before a token is there. See "The rules" in the
/// module docs. It makes no record for a person that the people know,
/// so a second try after a stop makes no second record.
///
/// The reply is the USER, and the position of the pending copy at the
/// check: the sign-in keeps it (01M3XA87A9GGFA89RQXWSKY0V6).
#[derive(Clone, Debug)]
pub struct Admit {
    /// The verified email of the account.
    pub email: String,
    /// True when the account is in an allowed domain (R15).
    pub allowed_domain: bool,
}

/// The reply to [`Admit`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admitted {
    /// The USER of the person.
    pub user: String,
    /// The position of the log at the start of the sign-in.
    pub position: u64,
}

impl Command for Admit {
    const KIND: CommandKind = CommandKind::Admit;
    type Reply = Admitted;
    type Note = Admitted;

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, Admitted), Refused> {
        let people = view.people();
        let email = email(&self.email);
        let user = user_of(&email).map_err(|e| Refused::new(Code::BadRequest, e.to_string()))?;
        if user == SERVER_USER {
            return Err(Refused::new(
                Code::BadRequest,
                format!("the user {SERVER_USER} is the riff server; sign in with another email"),
            ));
        }
        let known = match people.email_of(&user) {
            Some(held) if held != email => {
                return Err(Refused::new(
                    Code::NotAllowed,
                    format!("the user {user} belongs to another account; ask an admin"),
                ));
            }
            held => held.is_some(),
        };
        let admin = people.admins.contains(&email) || view.settings.is_admin(&email);
        let new_riff = !people.owned() && view.settings.admins().is_empty();
        let may_join = admin
            || self.allowed_domain
            || new_riff
            || people.owner() == Some(email.as_str())
            || people.members.contains(&email);
        if !may_join {
            return Err(Refused::new(
                Code::NotMember,
                format!(
                    "{email} is not a member of this riff; ask its owner to run: riff invite {email}"
                ),
            ));
        }
        let mut changes = Vec::new();
        if !known {
            changes.push(Change::PersonJoined(PersonJoined {
                user: user.clone(),
                email: email.clone(),
            }));
        }
        if !people.owned() && (view.settings.admins().is_empty() || admin) {
            changes.push(owner_set(Some(&email)));
        }
        let position = view.riff.position();
        Ok((changes, Admitted { user, position }))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, note: Admitted, _: Now) -> Admitted {
        note
    }
}

/// Adds a member, by verified email. It needs an admin.
impl Command for Invite {
    const KIND: CommandKind = CommandKind::Invite;
    type Reply = Invited;
    type Note = String;

    fn needs(&self, _: &Caller) -> Role {
        Role::Admin
    }

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, String), Refused> {
        let email = valid(&self.email)?;
        let mut changes = Vec::new();
        if !view.people().members.contains(&email) {
            changes.push(invited(&email));
        }
        Ok((changes, email))
    }

    fn reply(&self, _: &Caller, view: &View<'_>, _: &Done, email: String, _: Now) -> Invited {
        Invited {
            email,
            address: view.settings.address().to_owned(),
        }
    }
}

/// Removes a member, by verified email, and ends each sign-in of that
/// person (R20): the writer ends them after the write of the record. A
/// person of an allowed domain can still sign in. The owner cannot go.
/// It needs an admin.
impl Command for Remove {
    const KIND: CommandKind = CommandKind::Remove;
    type Reply = Removed;
    type Note = String;

    fn needs(&self, _: &Caller) -> Role {
        Role::Admin
    }

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, String), Refused> {
        let people = view.people();
        let email = email(&self.email);
        if people.owner() == Some(email.as_str()) {
            return Err(Refused::new(
                Code::BadRequest,
                format!("{email} is the owner of this riff; the owner stays"),
            ));
        }
        if people.admins.contains(&email) {
            return Err(Refused::new(
                Code::BadRequest,
                format!("{email} is an admin; the owner runs riff admin remove {email} first"),
            ));
        }
        let mut changes = Vec::new();
        if people.members.contains(&email) || people.user_of_email(&email).is_some() {
            changes.push(Change::MemberRemoved(Email {
                email: email.clone(),
            }));
        }
        Ok((changes, email))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, done: &Done, email: String, _: Now) -> Removed {
        Removed {
            email,
            sign_ins: done.ended,
        }
    }
}

/// Makes a person an admin, or an admin a member again
/// (01M3JY7T109BR860EQBSKEFDHY). A new admin is also a member, so the
/// person stays a member after the role goes. The owner stays an admin.
/// It needs the owner.
impl Command for SetAdmin {
    const KIND: CommandKind = CommandKind::SetAdmin;
    type Reply = AdminSet;
    type Note = String;

    fn needs(&self, _: &Caller) -> Role {
        Role::Owner
    }

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, String), Refused> {
        let people = view.people();
        let mut changes = Vec::new();
        if self.admin {
            let email = valid(&self.email)?;
            if !people.members.contains(&email) {
                changes.push(invited(&email));
            }
            if !people.admins.contains(&email) {
                changes.push(admin_set(&email, true));
            }
            return Ok((changes, email));
        }
        let email = email(&self.email);
        if people.owner() == Some(email.as_str()) {
            return Err(Refused::new(
                Code::BadRequest,
                format!("{email} is the owner of this riff; the owner stays an admin"),
            ));
        }
        if !people.admins.contains(&email) {
            return Err(Refused::new(
                Code::BadRequest,
                format!("{email} is not an admin that the owner made"),
            ));
        }
        changes.push(admin_set(&email, false));
        Ok((changes, email))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, email: String, _: Now) -> AdminSet {
        AdminSet {
            email,
            admin: self.admin,
        }
    }
}

/// Passes the owner role to a member or an admin
/// (01M3JYX8NPZASQY6031R35H39P). The old owner stays an admin and a
/// member. It ends a request for the owner role that waits. It needs
/// the owner.
impl Command for PassOwner {
    const KIND: CommandKind = CommandKind::PassOwner;
    type Reply = OwnerPassed;
    /// The new owner and the old owner.
    type Note = (String, String);

    fn needs(&self, _: &Caller) -> Role {
        Role::Owner
    }

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, (String, String)), Refused> {
        let people = view.people();
        let email = email(&self.email);
        let Some(old) = people.owner() else {
            return Err(Refused::new(Code::NotAllowed, NO_OWNER));
        };
        if old == email {
            return Err(Refused::new(
                Code::BadRequest,
                format!("{email} is the owner of this riff already"),
            ));
        }
        if !people.members.contains(&email) && !view.is_admin(&email) {
            return Err(Refused::new(
                Code::NotMember,
                format!("{email} is not a member of this riff; run riff invite {email} first"),
            ));
        }
        let mut changes = view.keeps(old);
        changes.push(owner_set(Some(&email)));
        Ok((changes, (email, old.to_owned())))
    }

    fn reply(
        &self,
        _: &Caller,
        _: &View<'_>,
        _: &Done,
        (owner, admin): (String, String),
        _: Now,
    ) -> OwnerPassed {
        OwnerPassed { owner, admin }
    }
}

/// An admin asks for the owner role (01M3N7K3ZAZFGABN7032AYJWEM).
///
/// On a riff with no owner, the admin is the owner at once
/// (01M3Q63NNC6SC03BFCG80M7B4D). Else the request waits: the owner
/// answers with [`PassOwner`] or [`DenyOwner`], and [`GrantOwner`]
/// grants it when its time ends (01M3Q5460YESBSQHTV3M15PE53). One
/// request waits at a time. The owner is the owner already: the command
/// changes nothing, and posts no note (01M3WRJAFS6W3J2ZRJ6XSW3SB5).
///
/// The note of the server goes in the chunk of the command, and each
/// live lead of the owner gets it as a direct message.
impl Command for TakeOwner {
    const KIND: CommandKind = CommandKind::TakeOwner;
    type Reply = OwnerAsked;
    type Note = OwnerAsked;

    fn needs(&self, _: &Caller) -> Role {
        Role::Admin
    }

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, OwnerAsked), Refused> {
        let people = view.people();
        let user = caller.who().user();
        let Some(admin) = people.email_of(user).map(str::to_owned) else {
            return Err(Refused::new(
                Code::NotAllowed,
                format!("{user} is not an admin; only an admin can take the owner role"),
            ));
        };
        let Some(owner) = people.owner().map(str::to_owned) else {
            let mut changes = vec![owner_set(Some(&admin))];
            changes.append(&mut view.news(&owner::took_news(user, &admin), &[], now));
            let asked = OwnerAsked {
                admin,
                owner: None,
                answer_secs: 0,
            };
            return Ok((changes, asked));
        };
        if owner == admin {
            let asked = OwnerAsked {
                admin,
                owner: Some(owner),
                answer_secs: 0,
            };
            return Ok((Vec::new(), asked));
        }
        if let Some(first) = people.asks() {
            return Err(Refused::new(
                Code::BadRequest,
                format!("{first} asked for the owner role first; wait for the answer of the owner"),
            ));
        }
        let answer = view.settings.owner_role().answer;
        let wait = u64::try_from(answer.as_millis()).unwrap_or(u64::MAX);
        let mut changes = vec![Change::OwnerAsked(record::OwnerAsked {
            email: admin.clone(),
            due_ms: now.ms.saturating_add(wait),
        })];
        let news = owner::asked_news(user, &admin, &owner, answer);
        let tell: Vec<String> = people
            .user_of_email(&owner)
            .map(str::to_owned)
            .into_iter()
            .collect();
        changes.append(&mut view.news(&news, &tell, now));
        let asked = OwnerAsked {
            admin,
            owner: Some(owner),
            answer_secs: answer.as_secs(),
        };
        Ok((changes, asked))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, asked: OwnerAsked, _: Now) -> OwnerAsked {
        asked
    }
}

/// The owner keeps the owner role that an admin asks for
/// (01M3N7K41N03P26BEFFNX5617K). The note of the server goes in the
/// chunk of the command, and each live lead of the admin gets it as a
/// direct message. It needs the owner.
impl Command for DenyOwner {
    const KIND: CommandKind = CommandKind::DenyOwner;
    type Reply = OwnerDenied;
    type Note = OwnerDenied;

    fn needs(&self, _: &Caller) -> Role {
        Role::Owner
    }

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, OwnerDenied), Refused> {
        let people = view.people();
        let Some(owner) = people.owner().map(str::to_owned) else {
            return Err(Refused::new(Code::NotAllowed, NO_OWNER));
        };
        let Some(admin) = people.asks().map(str::to_owned) else {
            return Err(Refused::new(
                Code::BadRequest,
                "no admin asks for the owner role now",
            ));
        };
        let mut changes = vec![Change::OwnerDenied(Email {
            email: admin.clone(),
        })];
        let news = owner::denied_news(caller.who().user(), &owner, &admin);
        let tell: Vec<String> = people
            .user_of_email(&admin)
            .map(str::to_owned)
            .into_iter()
            .collect();
        changes.append(&mut view.news(&news, &tell, now));
        Ok((changes, OwnerDenied { owner, admin }))
    }

    fn reply(
        &self,
        _: &Caller,
        _: &View<'_>,
        _: &Done,
        denied: OwnerDenied,
        _: Now,
    ) -> OwnerDenied {
        denied
    }
}

/// A change of the owner role that no person made: the server makes it
/// when a time ends (01M3N7K41N03P26BEFFNX5617K,
/// 01M3Q546335NBTKG5BHQ27QC93).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerChange {
    /// The owner `old` did not answer the request in time. The admin
    /// that asked is the `owner` now. `old` stays an admin.
    Granted { owner: String, old: String },
    /// The owner `old` is gone and stays an admin. The admin of a
    /// request that waited is the `owner` now. With no request, the
    /// riff has no owner.
    Gone { old: String, owner: Option<String> },
}

/// The timer of the server grants a request for the owner role whose
/// time ended with no answer of the owner
/// (01M3N7K41N03P26BEFFNX5617K). The old owner stays an admin. It
/// changes nothing when no request waits, or its time did not end yet.
/// The reply is the change that it made.
#[derive(Clone, Copy, Debug)]
pub struct GrantOwner;

impl Command for GrantOwner {
    const KIND: CommandKind = CommandKind::GrantOwner;
    type Reply = Option<OwnerChange>;
    type Note = Option<OwnerChange>;

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, Option<OwnerChange>), Refused> {
        let people = view.people();
        let (true, Some(owner), Some(old)) = (people.is_due(now.ms), people.asks(), people.owner())
        else {
            return Ok((Vec::new(), None));
        };
        let change = OwnerChange::Granted {
            owner: owner.to_owned(),
            old: old.to_owned(),
        };
        let mut changes = view.keeps(old);
        changes.push(owner_set(Some(owner)));
        let news = owner::change_news(&change, view.settings.owner_role());
        changes.append(&mut view.news(&news, &[], now));
        Ok((changes, Some(change)))
    }

    fn reply(
        &self,
        _: &Caller,
        _: &View<'_>,
        _: &Done,
        change: Option<OwnerChange>,
        _: Now,
    ) -> Option<OwnerChange> {
        change
    }
}

/// The timer of the server ends the role of an owner who is gone
/// (01M3Q546335NBTKG5BHQ27QC93). The old owner stays an admin. The
/// admin of a request that waits is the owner at once. With no request,
/// the riff has no owner (01M3Q63NNC6SC03BFCG80M7B4D): the note then
/// also goes to each live lead of each admin, to ask for a volunteer.
/// It changes nothing in a riff with no owner. The reply is the change
/// that it made.
#[derive(Clone, Copy, Debug)]
pub struct EndOwner;

impl Command for EndOwner {
    const KIND: CommandKind = CommandKind::EndOwner;
    type Reply = Option<OwnerChange>;
    type Note = Option<OwnerChange>;

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, Option<OwnerChange>), Refused> {
        let people = view.people();
        let Some(old) = people.owner() else {
            return Ok((Vec::new(), None));
        };
        let owner = people.asks();
        let change = OwnerChange::Gone {
            old: old.to_owned(),
            owner: owner.map(str::to_owned),
        };
        // Each admin after the change: the old owner is one of them.
        let tell = match owner {
            Some(_) => Vec::new(),
            None => {
                let mut admins = view.admin_users();
                admins.extend(view.owner_user().map(str::to_owned));
                admins.sort();
                admins
            }
        };
        let mut changes = view.keeps(old);
        changes.push(owner_set(owner));
        let news = owner::change_news(&change, view.settings.owner_role());
        changes.append(&mut view.news(&news, &tell, now));
        Ok((changes, Some(change)))
    }

    fn reply(
        &self,
        _: &Caller,
        _: &View<'_>,
        _: &Done,
        change: Option<OwnerChange>,
        _: Now,
    ) -> Option<OwnerChange> {
        change
    }
}

/// The setting `--owner` names the owner of a new riff
/// (01M3JN3ASSV9SA0QZKXXJ0RTEV): a command of the server, one time
/// after the load. A riff that has an owner keeps it. A riff whose
/// owner was gone keeps no owner (01M3N7K4GAKJ621V5AWJRQVF3M).
#[derive(Clone, Debug)]
pub struct NameOwner {
    /// The verified email of the owner.
    pub email: String,
}

impl Command for NameOwner {
    const KIND: CommandKind = CommandKind::NameOwner;
    type Reply = ();
    type Note = ();

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let mut changes = Vec::new();
        if !view.people().owned() {
            changes.push(owner_set(Some(&email(&self.email))));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, (): (), _: Now) {}
}

/// The USER whose sign-ins a revoke ends: the USER of the call, or the
/// caller. Names compare trimmed and in lower case, as at sign-in
/// (R111).
fn revoked(revoke: &Revoke, caller: &Caller) -> String {
    revoke
        .user
        .as_deref()
        .map_or_else(|| caller.who().user().to_owned(), email)
}

/// Ends each sign-in of a person, and each token of them (R20): the
/// writer ends them after the write of the record. A person ends their
/// own sign-ins. Only an admin names another person. The USER of the
/// person stays with its email.
impl Command for Revoke {
    const KIND: CommandKind = CommandKind::Revoke;
    type Reply = Revoked;
    type Note = String;

    fn needs(&self, caller: &Caller) -> Role {
        if revoked(self, caller) == caller.who().user() {
            Role::Member
        } else {
            Role::Admin
        }
    }

    fn handle(
        &self,
        caller: &Caller,
        _view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, String), Refused> {
        let user = revoked(self, caller);
        let changes = vec![Change::SigninsEnded(SigninsEnded { user: user.clone() })];
        Ok((changes, user))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, done: &Done, user: String, _: Now) -> Revoked {
        Revoked {
            user,
            sign_ins: done.ended,
        }
    }
}
