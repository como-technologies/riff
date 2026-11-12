//! The group "people": who may join the riff, and with which role.
//!
//! - Part of the riff: [`People`]. The riff ID, the email of each USER
//!   (R209), the members, the admins, and the position of the last end
//!   of the sign-ins of each USER (01M3XA87A9GGFA89RQXWSKY0V6). The keys
//!   of each USER (01M75NH2P3QHCYDKQH60KGSF97) are here too: the group
//!   [`super::devices`] has their commands.
//! - `apply`: one method of [`People`] for each of the kinds
//!   `riff_made`, `person_joined`, `member_invited`, `member_removed`,
//!   `admin_set`, `owner_set`, `signins_ended`, `key_added`,
//!   `device_removed` and `proved`. Only they change the people: see
//!   [`super::riff`]. The old kinds of the owner role stay readable
//!   (01M7767T393P46VTNNE5SY203K): an `owner_set` with an email makes it an
//!   admin and a member, and each other record of the role changes nothing.
//! - Checkpoint: `Saved`, the fields `riff_id`, `users`, `members`,
//!   `admins`, `signins_ended` and `keys`. A checkpoint from before
//!   also has `owner`, `no_owner` and `owner_asked`: the load makes the
//!   `owner` one more admin and member, and drops the other two.
//! - Commands: the wire types [`Invite`], [`Remove`], [`SetAdmin`] and
//!   [`Revoke`], the command [`Admit`] of the token path, and the
//!   command [`NameAdmins`] of the server. No client can send the last
//!   one.
//! - [`Invite`], [`Remove`] and [`SetAdmin`] are actions of the proof
//!   of presence ([`Command::proof`], 01M75WKSD5A66RHQMMM5CMKWJB).
//!
//! # The rules
//!
//! A person signs in with a verified email. The USER comes from the
//! email (R208). The first email that signs in with a USER holds it
//! (R209). The people have two roles: member and admin. An admin is a
//! member too (01M77671TQ92487GSM24RJG82Z). [`Admit`] lets a person in when one
//! of these is true:
//!
//! - The person is a member or an admin.
//! - The account is in an allowed domain (R15).
//! - The riff is new: it has no admin. The person becomes an admin.
//!
//! [`Admit`] checks the member first, and then the USER of the email
//! (01M3XGP011KGXDP9D1FNMT374F). So a person who may not join gets "not
//! a member", and learns nothing of the USER names.
//!
//! ```mermaid
//! flowchart TD
//!     A[verified email] --> O{member or admin?}
//!     O -- yes --> IN[sign in]
//!     O -- no --> D{allowed domain?}
//!     D -- yes --> IN
//!     D -- no --> N{no admin?}
//!     N -- yes --> IN
//!     N -- no --> R[refuse: ask an admin for an invite]
//!     IN --> F{no admin yet?}
//!     F -- yes --> AD[the person is an admin]
//! ```
//!
//! - The first person that [`Admit`] lets in to a riff with no admin is
//!   an admin and a member (01M7767SRX4K1N1BCDHT7M85DN).
//! - The setting `--admin EMAIL` (R210) names an admin at each start:
//!   the server sends [`NameAdmins`] one time after the load
//!   (01M7767SVH8AKK7144QTJGGGRY). So a riff with such a setting has an admin
//!   before the first sign-in.
//! - An admin adds a member with [`Invite`], and removes one with
//!   [`Remove`]. A removal ends each sign-in of that person (R20).
//! - An admin makes a person an admin, or an admin a member again, with
//!   [`SetAdmin`]. A new admin is also a member. The last admin stays
//!   (01M7767SY5PW1AMB0PBMW2GXDX): a riff with an admin keeps one.
//! - [`Revoke`] ends each sign-in of one person. It leaves the USER of
//!   the person: only that email signs in as that USER again.
//! - Each change of the people posts one line of the server in the chat,
//!   in the chunk of its command, and no note to the thread of a
//!   repository (01M76S6128EZRZS2D5CE2QCC05, 01M77BSQQGGNE83ECS27YGVEP1):
//!   an [`Invite`], a [`Remove`] and a [`SetAdmin`]. A command that
//!   changes nothing posts no line.
//!
//! ```mermaid
//! flowchart LR
//!     C[a change of the people] --> L[one line of riff-bot in the chat]
//!     C -. no note .-> R[the thread of a repository]
//! ```
//!
//! ```mermaid
//! stateDiagram-v2
//!     NoAdmin --> Admins: the first sign-in, or --admin EMAIL at a start
//!     Admins --> Admins: riff admin add EMAIL
//!     Admins --> Admins: riff admin remove EMAIL, while another admin stays
//! ```

use std::collections::{BTreeMap, BTreeSet};

use riff_core::name::{SERVER_USER, SessionUri, ThreadName, Who, check, sanitize};
use riff_core::proof::Action;
use riff_core::record::{
    self, Change, DeviceRemoved, Email, KeyAdded, OwnerSet, PersonJoined, Proved, RiffMade,
    SigninsEnded,
};
use riff_core::wire::{
    AdminSet, Invite, Invited, Kind, MembersReply, Message, PersonRole, Remove, Removed, Revoke,
    Revoked, SetAdmin,
};
use serde::{Deserialize, Serialize};

use super::command::{Caller, Code, Command, CommandKind, Done, Now, Refused, Role};
use super::devices::PersonKey;
use super::view::View;

/// An email as the people keep it: with no space around it, in lower
/// case.
///
/// ```
/// assert_eq!(riff_state::state::people::email(" Ada@X.io"), "ada@x.io");
/// ```
pub fn email(text: &str) -> String {
    text.trim().to_lowercase()
}

/// The USER of `email`: the part before the `@`, in lower case, with
/// each character that a URI part cannot hold replaced by `-` (R208).
/// Two emails can give the same USER, for example `o'brien@x.io` and
/// `o-brien@x.io`. Only [`Admit`] tells which email holds it (R209).
/// The error says why `email` gives no USER.
///
/// ```
/// use riff_state::state::people::user_of;
///
/// assert_eq!(user_of("Ada.Lovelace@comotechnologies.io").unwrap(), "ada.lovelace");
/// assert_eq!(user_of("a+b@x.io").unwrap(), "a-b");
/// assert!(user_of("no-at-sign").unwrap_err().contains("not an email"));
/// assert!(user_of("@x.io").is_err());
/// ```
pub fn user_of(email: &str) -> Result<String, String> {
    let local = email
        .rsplit_once('@')
        .map(|(local, _)| local)
        .ok_or_else(|| format!("not an email: {email}"))?;
    let user = sanitize(&local.to_lowercase());
    check("user", &user).map_err(|e| e.to_string())?;
    Ok(user)
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
    /// The email of each admin.
    admins: BTreeSet<String>,
    /// The position of the last `member_removed` or `signins_ended`
    /// record of each USER. A sign-in that started before it is ended
    /// (01M3XA87A9GGFA89RQXWSKY0V6).
    ended: BTreeMap<String, u64>,
    /// The keys of each USER, by the name of the key
    /// (01M75NH2P3QHCYDKQH60KGSF97).
    keys: BTreeMap<String, BTreeMap<String, PersonKey>>,
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

    /// The email is an admin, or it is not.
    pub(super) fn admin_set(&mut self, set: &record::AdminSet) -> Result<(), &'static str> {
        if set.admin {
            self.admins.insert(set.email.clone());
        } else {
            self.admins.remove(&set.email);
        }
        Ok(())
    }

    /// An old record of the owner role (01M7767T393P46VTNNE5SY203K): its email
    /// is an admin and a member. An old log has no `member_invited` of the
    /// owner, so `riff admin remove` then leaves a member. A record with no
    /// email said that the owner was gone: the old owner stayed an admin, so
    /// it changes nothing.
    pub(super) fn owner_set(&mut self, set: &OwnerSet) -> Result<(), &'static str> {
        if let Some(email) = &set.email {
            self.admins.insert(email.clone());
            self.members.insert(email.clone());
        }
        Ok(())
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

    /// The USER has the key of the record, from `at_ms`: the time of the
    /// add and of its first use (01M75NH2P3QHCYDKQH60KGSF97). A name that
    /// the USER has, or a credential that the riff knows, changes
    /// nothing: `handle` refuses such an add.
    pub(super) fn key_added(&mut self, added: &KeyAdded, at_ms: u64) -> Result<(), &'static str> {
        let KeyAdded {
            user,
            name,
            credential,
            public_key,
        } = added;
        if self.device_of(user, name).is_some() {
            return Err("the person has a device with the name");
        }
        if self.knows_credential(credential) {
            return Err("a person has the credential");
        }
        let key = PersonKey {
            credential: credential.clone(),
            public_key: public_key.clone(),
            added_ms: at_ms,
            used_ms: at_ms,
            counter: 0,
        };
        self.keys
            .entry(user.clone())
            .or_default()
            .insert(name.clone(), key);
        Ok(())
    }

    /// The USER has no device with the name of the record any more.
    pub(super) fn device_removed(&mut self, removed: &DeviceRemoved) -> Result<(), &'static str> {
        let DeviceRemoved { user, name } = removed;
        let Some(keys) = self.keys.get_mut(user) else {
            return Err("the person has no device");
        };
        if keys.remove(name).is_none() {
            return Err("the person has no device with the name");
        }
        if keys.is_empty() {
            self.keys.remove(user);
        }
        Ok(())
    }

    /// The key `name` of `user` signed a proof (01M75WKS4RF17WF85MWA4GFK0C): the time of
    /// the record is its last use, and the counter of the proof is its
    /// counter now. A key that the person does not have changes nothing.
    pub(super) fn proved(&mut self, proved: &Proved, at_ms: u64) -> Result<(), &'static str> {
        let Some(key) = self
            .keys
            .get_mut(&proved.user)
            .and_then(|keys| keys.get_mut(&proved.name))
        else {
            return Err("the person has no key with the name");
        };
        key.used_ms = at_ms;
        key.counter = proved.counter;
        Ok(())
    }

    /// The key `name` of `user`, when the person has one.
    pub fn device_of(&self, user: &str, name: &str) -> Option<&PersonKey> {
        self.keys.get(user)?.get(name)
    }

    /// Each key of `user`, by its name.
    pub fn keys_of(&self, user: &str) -> impl Iterator<Item = (&String, &PersonKey)> {
        self.keys.get(user).into_iter().flatten()
    }

    /// True when a person of the riff has a key with the credential ID
    /// `credential`.
    pub fn knows_credential(&self, credential: &str) -> bool {
        self.keys
            .values()
            .flat_map(BTreeMap::values)
            .any(|key| key.credential == credential)
    }

    /// The ID of the riff, when it has one.
    pub fn riff_id(&self) -> Option<&str> {
        self.riff_id.as_deref()
    }

    /// True when the riff has an admin (01M3JN3AQMHZHT6JP3P6GM9PWZ).
    pub fn has_admin(&self) -> bool {
        !self.admins.is_empty()
    }

    /// True when `email`, in lower case, is an admin.
    pub fn is_admin(&self, email: &str) -> bool {
        self.admins.contains(email)
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
        let People {
            riff_id,
            users,
            members,
            admins,
            ended,
            keys,
        } = self.clone();
        Saved {
            riff_id,
            users,
            members,
            admins,
            owner: None,
            no_owner: false,
            owner_asked: None,
            signins_ended: ended,
            keys,
        }
    }
}

/// The part of the checkpoint of this group. A field with nothing in
/// it is not written. So the checkpoint of a riff with no people has
/// the bytes of a checkpoint from before the people
/// (01M3WNQR41K41TV832GRQZ2CQS).
///
/// The fields `owner`, `no_owner` and `owner_asked` come only from a
/// checkpoint from before (01M7767T393P46VTNNE5SY203K). A read and a write of
/// such a checkpoint keep its bytes. Its load makes the owner one more admin
/// and member, and drops the other two. This build writes none of them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub(super) struct Saved {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    riff_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    users: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    members: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    admins: BTreeSet<String>,
    /// The owner of a checkpoint from before: the load makes it an
    /// admin and a member.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
    /// A riff whose owner was gone, in a checkpoint from before.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    no_owner: bool,
    /// A request for the owner role, in a checkpoint from before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner_asked: Option<record::OwnerAsked>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    signins_ended: BTreeMap<String, u64>,
    /// The keys of each USER, by name (01M75NH2P3QHCYDKQH60KGSF97).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    keys: BTreeMap<String, BTreeMap<String, PersonKey>>,
}

impl Saved {
    pub(super) fn restore(self) -> People {
        let Saved {
            riff_id,
            users,
            mut members,
            mut admins,
            owner,
            no_owner: _,
            owner_asked: _,
            signins_ended,
            keys,
        } = self;
        members.extend(owner.clone());
        admins.extend(owner);
        People {
            riff_id,
            users,
            members,
            admins,
            ended: signins_ended,
            keys,
        }
    }
}

/// The rules of the people for `handle` and for each query.
impl View<'_> {
    fn people(&self) -> &People {
        self.riff.people()
    }

    /// True when `user` holds an email that is an admin or a member.
    pub(super) fn may_join(&self, user: &str) -> bool {
        let people = self.people();
        people
            .email_of(user)
            .is_some_and(|email| people.is_admin(email) || people.members.contains(email))
    }

    /// The role of `user`: an admin or a member. A USER with no email
    /// is a member.
    pub fn role_of(&self, user: &str) -> Role {
        let people = self.people();
        match people.email_of(user) {
            Some(email) if people.is_admin(email) => Role::Admin,
            _ => Role::Member,
        }
    }

    /// Each person once, with the highest role: the admins and the
    /// members, each sorted (01M3MN157X8N9QKER1AJEPEJVX).
    pub fn roles(&self) -> (Vec<String>, Vec<String>) {
        let people = self.people();
        let admins: Vec<String> = people.admins.iter().cloned().collect();
        let members = people
            .members
            .iter()
            .filter(|member| !people.is_admin(member))
            .cloned()
            .collect();
        (admins, members)
    }

    /// Who may join the riff: the reply to `members`. The caller adds
    /// the allowed domains.
    pub fn members(&self) -> MembersReply {
        let (admins, members) = self.roles();
        MembersReply {
            admins,
            members,
            allowed_domains: Vec::new(),
            proof: riff_core::proof::Policy::Off,
        }
    }

    /// The USER and the role of each person with a USER, sorted by USER
    /// (01M3NT4M3A4E3K5S2NM7MS6PQD). A person who never signed in has
    /// no USER, and is not in it.
    pub fn persons(&self) -> Vec<(String, PersonRole)> {
        let (admins, members) = self.roles();
        let people = self.people();
        let mut persons: Vec<(String, PersonRole)> = admins
            .into_iter()
            .map(|email| (email, PersonRole::Admin))
            .chain(members.into_iter().map(|email| (email, PersonRole::Member)))
            .filter_map(|(email, role)| Some((people.user_of_email(&email)?.to_owned(), role)))
            .collect();
        persons.sort();
        persons
    }

    /// The USER of each admin, sorted: the lines of riff-bot for a
    /// release name each of them (01M7767T5VSWN9EYDVSTCEJ8AM). An admin who
    /// never signed in has no USER, and is not in it.
    pub fn admin_users(&self) -> Vec<String> {
        let people = self.people();
        people
            .users
            .iter()
            .filter(|(_, email)| people.is_admin(email))
            .map(|(user, _)| user.clone())
            .collect()
    }

    /// The thread of each repository of the riff, sorted: the
    /// repository of each known session, also a gone one.
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

    /// The `posted` record of one line of the server in the chat, for a
    /// change of the people (01M76S6128EZRZS2D5CE2QCC05,
    /// 01M77BSQQGGNE83ECS27YGVEP1). The line has no `to`: it wakes no
    /// session, and a person sees it in `riff chat`. It has the kind
    /// `riff`, so the chat shows it as `<riff-bot>`
    /// (01M6Y4ZCTSTVM971BDC1F7GJPK). A post of the server has no
    /// signature (01M3N7K4BC1RPZKQ1XNDTBRPGF). The record goes in the
    /// chunk of the command that causes it.
    fn chat(&self, body: &str, now: Now) -> Vec<Change> {
        let server = SessionUri::server();
        let line = Message {
            seq: 0,
            from: server.clone(),
            to: Vec::new(),
            body: body.to_owned(),
            at_ms: now.ms,
            kind: Kind::Riff,
            sig: None,
            payload: None,
        };
        self.put(server.who(), ThreadName::chat(), line, None, now.at)
            .0
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

/// The records that make `email` an admin and a member: a
/// `member_invited` when it is no member, and an `admin_set` when it is
/// no admin.
fn made_admin(people: &People, email: &str) -> Vec<Change> {
    let mut changes = Vec::new();
    if !people.members.contains(email) {
        changes.push(invited(email));
    }
    if !people.is_admin(email) {
        changes.push(admin_set(email, true));
    }
    changes
}

/// The line in the chat of an invite (01M76S6128EZRZS2D5CE2QCC05).
/// `user` made the change.
///
/// ```
/// assert_eq!(
///     riff_state::state::people::invited_news("ada", "bob@gmail.com"),
///     "members: ada invited bob@gmail.com. bob@gmail.com is a member now."
/// );
/// ```
pub fn invited_news(user: &str, email: &str) -> String {
    format!("members: {user} invited {email}. {email} is a member now.")
}

/// The line in the chat of a removal (01M76S6128EZRZS2D5CE2QCC05).
///
/// ```
/// assert_eq!(
///     riff_state::state::people::removed_news("ada", "bob@gmail.com"),
///     "members: ada removed bob@gmail.com. bob@gmail.com is not a member now."
/// );
/// ```
pub fn removed_news(user: &str, email: &str) -> String {
    format!("members: {user} removed {email}. {email} is not a member now.")
}

/// The line in the chat of `riff admin add` and `riff admin remove`
/// (01M76S6128EZRZS2D5CE2QCC05).
///
/// ```
/// use riff_state::state::people::admin_news;
///
/// assert_eq!(
///     admin_news("ada", "bob@gmail.com", true),
///     "members: ada made bob@gmail.com an admin."
/// );
/// assert_eq!(
///     admin_news("ada", "bob@gmail.com", false),
///     "members: ada made bob@gmail.com a member again, not an admin."
/// );
/// ```
pub fn admin_news(user: &str, email: &str, admin: bool) -> String {
    if admin {
        format!("members: {user} made {email} an admin.")
    } else {
        format!("members: {user} made {email} a member again, not an admin.")
    }
}

/// The email of a call, as the people keep it. It refuses an email that
/// gives no USER (R208).
fn valid(text: &str) -> Result<String, Refused> {
    let email = email(text);
    user_of(&email).map_err(|why| Refused::new(Code::BadRequest, why))?;
    Ok(email)
}

/// The refusal of the removal of the last admin (01M7767SY5PW1AMB0PBMW2GXDX).
///
/// ```
/// assert_eq!(
///     riff_state::state::people::last_admin("ada@x.io"),
///     "ada@x.io is the last admin of this riff, and the riff keeps one; \
///      make another person an admin first: riff admin add EMAIL"
/// );
/// ```
pub fn last_admin(email: &str) -> String {
    format!(
        "{email} is the last admin of this riff, and the riff keeps one; make another person an \
         admin first: riff admin add EMAIL"
    )
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
        // The member comes first (01M3XGP011KGXDP9D1FNMT374F): a person
        // who may not join learns nothing of the USER names.
        let new_riff = !people.has_admin();
        let may_join = new_riff
            || self.allowed_domain
            || people.is_admin(&email)
            || people.members.contains(&email);
        if !may_join {
            return Err(Refused::new(
                Code::NotMember,
                format!(
                    "{email} is not a member of this riff; ask an admin to run: riff invite {email}"
                ),
            ));
        }
        let user = user_of(&email).map_err(|why| Refused::new(Code::BadRequest, why))?;
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
        let mut changes = Vec::new();
        if !known {
            changes.push(Change::PersonJoined(PersonJoined {
                user: user.clone(),
                email: email.clone(),
            }));
        }
        if new_riff {
            changes.append(&mut made_admin(people, &email));
        }
        let position = view.riff.position();
        Ok((changes, Admitted { user, position }))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, note: Admitted, _: Now) -> Admitted {
        note
    }
}

/// Adds a member, by verified email. It needs an admin. The chunk holds
/// one line in the chat (01M76S6128EZRZS2D5CE2QCC05).
impl Command for Invite {
    const KIND: CommandKind = CommandKind::Invite;
    type Reply = Invited;
    type Note = String;

    fn needs(&self, _: &Caller) -> Role {
        Role::Admin
    }

    fn proof(&self, _: &Caller, _: &View<'_>, _: Now) -> Option<Action> {
        Some(Action::invite(&self.email))
    }

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, String), Refused> {
        let email = valid(&self.email)?;
        let mut changes = Vec::new();
        if !view.people().members.contains(&email) {
            changes.push(invited(&email));
            let news = invited_news(caller.who().user(), &email);
            changes.append(&mut view.chat(&news, now));
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
/// person of an allowed domain can still sign in. An admin cannot go:
/// `riff admin remove` makes the admin a member first. It needs an
/// admin. The chunk holds one line in the chat
/// (01M76S6128EZRZS2D5CE2QCC05).
impl Command for Remove {
    const KIND: CommandKind = CommandKind::Remove;
    type Reply = Removed;
    type Note = String;

    fn needs(&self, _: &Caller) -> Role {
        Role::Admin
    }

    fn proof(&self, _: &Caller, _: &View<'_>, _: Now) -> Option<Action> {
        Some(Action::remove(&self.email))
    }

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, String), Refused> {
        let people = view.people();
        let email = email(&self.email);
        if people.is_admin(&email) {
            return Err(Refused::new(
                Code::BadRequest,
                format!("{email} is an admin; run riff admin remove {email} first"),
            ));
        }
        let mut changes = Vec::new();
        if people.members.contains(&email) || people.user_of_email(&email).is_some() {
            changes.push(Change::MemberRemoved(Email {
                email: email.clone(),
            }));
            let news = removed_news(caller.who().user(), &email);
            changes.append(&mut view.chat(&news, now));
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
/// person stays a member after the role goes. The last admin stays
/// (01M7767SY5PW1AMB0PBMW2GXDX). It needs an admin
/// (01M77671TQ92487GSM24RJG82Z). The chunk holds one line in the chat
/// (01M76S6128EZRZS2D5CE2QCC05).
impl Command for SetAdmin {
    const KIND: CommandKind = CommandKind::SetAdmin;
    type Reply = AdminSet;
    type Note = String;

    fn needs(&self, _: &Caller) -> Role {
        Role::Admin
    }

    fn proof(&self, _: &Caller, _: &View<'_>, _: Now) -> Option<Action> {
        Some(Action::admin(&self.email, self.admin))
    }

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, String), Refused> {
        let people = view.people();
        let user = caller.who().user();
        if self.admin {
            let email = valid(&self.email)?;
            let mut changes = made_admin(people, &email);
            if !people.is_admin(&email) {
                changes.append(&mut view.chat(&admin_news(user, &email, true), now));
            }
            return Ok((changes, email));
        }
        let email = email(&self.email);
        if !people.is_admin(&email) {
            return Err(Refused::new(
                Code::BadRequest,
                format!("{email} is not an admin"),
            ));
        }
        if people.admins.len() == 1 {
            return Err(Refused::new(Code::BadRequest, last_admin(&email)));
        }
        let mut changes = vec![admin_set(&email, false)];
        changes.append(&mut view.chat(&admin_news(user, &email, false), now));
        Ok((changes, email))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, email: String, _: Now) -> AdminSet {
        AdminSet {
            email,
            admin: self.admin,
        }
    }
}

/// The setting `--admin EMAIL` (`RIFF_ADMINS`, R210) names the admins
/// at each start (01M7767SVH8AKK7144QTJGGGRY): a command of the server, one
/// time after the load. Each email that is not an admin becomes an admin and a
/// member. A name that gives no USER names nobody: the server warns of it at
/// start. The reply is each email that became an admin.
#[derive(Clone, Debug)]
pub struct NameAdmins {
    /// The verified email of each admin of the setting.
    pub emails: Vec<String>,
}

impl Command for NameAdmins {
    const KIND: CommandKind = CommandKind::NameAdmins;
    type Reply = Vec<String>;
    type Note = Vec<String>;

    fn handle(
        &self,
        _caller: &Caller,
        view: &View<'_>,
        _now: Now,
    ) -> Result<(Vec<Change>, Vec<String>), Refused> {
        let people = view.people();
        let mut named: Vec<String> = self.emails.iter().filter_map(|e| valid(e).ok()).collect();
        named.sort();
        named.dedup();
        named.retain(|email| !people.is_admin(email));
        let changes = named
            .iter()
            .flat_map(|email| made_admin(people, email))
            .collect();
        Ok((changes, named))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, named: Vec<String>, _: Now) -> Vec<String> {
        named
    }
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
