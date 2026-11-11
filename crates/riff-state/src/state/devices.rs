//! The group "devices": the commands [`AddKey`], [`RemoveDevice`] and
//! [`Prove`], and the devices of a person (01M75NGT9Z0NSZTG97TBAMP7RS). The storage
//! is a part of [`People`]: the keys of each USER, in the part of the
//! people of the checkpoint.
//!
//! | Command | Who sends it | Record |
//! |---|---|---|
//! | `add_key` ([`AddKey`]) | riff-server, as the person of an add, after its check of the reply of the authenticator | `key_added` |
//! | `remove_device` ([`RemoveDevice`]) | a person, for a device of their own | `device_removed` |
//! | `prove` ([`Prove`]) | riff-server, as the person of a proof, after its check of the signature | `proved` |
//!
//! [`permits`](super::permits) takes only a person for each
//! (01M75NH30PNATYN33MXZQMVBW8), and a riff with no sign-in refuses
//! each: it has no people. `handle` of `add_key` refuses a name that is
//! not allowed, a name of another device of the person, a credential
//! that a person of the riff has, and a key over [`KEYS_MOST`]
//! (01M75NH33CVGBVASSACBPXRR2G). `handle` of `remove_device` refuses a
//! name that the person has no device with. With the policy
//! `required`, the removal needs a proof (PROOF_KEYS). `handle` of
//! `prove` refuses a key that the person does not have, and a counter
//! that did not grow (PROOF_CHECK).
//!
//! ```mermaid
//! flowchart LR
//!     A[add_key NAME] --> C{name, credential, count}
//!     C -->|refused| X[bad_request]
//!     C -->|good| R[(key_added)]
//!     D[remove_device NAME] --> H{a device NAME?}
//!     H -->|no| X
//!     H -->|yes| G[(device_removed)]
//! ```

use riff_core::device::{KEYS_MOST, check_name};
use riff_core::proof::Action;
use riff_core::record::{Change, DeviceRemoved, KeyAdded, Proved};
use riff_core::wire::{DeviceInfo, DeviceKind, DevicesReply, RemoveDevice};
use serde::{Deserialize, Serialize};

use super::command::{Caller, Code, Command, CommandKind, Done, Now, Refused};
use super::people::People;
use super::view::View;

/// A key of a person, as the state keeps it (01M75NH2P3QHCYDKQH60KGSF97).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PersonKey {
    /// The credential ID, in base64url with no padding.
    pub credential: String,
    /// The public key: a COSE key in CBOR, in base64url with no padding.
    pub public_key: String,
    /// The time of the add, in milliseconds since the Unix epoch.
    pub added_ms: u64,
    /// The time of the last use. The add is a use, and so is each proof
    /// (PROOF_LOG).
    pub used_ms: u64,
    /// The counter of the last proof of the key: 0 for an authenticator
    /// that counts nothing, and before the first proof (PROOF_CHECK).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub counter: u64,
}

fn is_zero(counter: &u64) -> bool {
    *counter == 0
}

impl PersonKey {
    /// The key `name` as `riff device list` shows it.
    pub fn info(&self, name: &str) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::Key,
            name: name.to_owned(),
            added_ms: self.added_ms,
            used_ms: self.used_ms,
        }
    }
}

/// The add of a key of the caller: riff-server sends it after its check
/// of the reply of the authenticator (01M75NH2VF6HGZVY04GB358JG0). No
/// client can send it. The reply is the new key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddKey {
    pub name: String,
    /// The credential ID, in base64url with no padding.
    pub credential: String,
    /// The COSE key, in base64url with no padding.
    pub public_key: String,
}

impl Command for AddKey {
    const KIND: CommandKind = CommandKind::AddKey;
    type Reply = DeviceInfo;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let AddKey {
            name,
            credential,
            public_key,
        } = self;
        let user = caller.who().user();
        let people = view.riff.people();
        refuse_name(people, user, name)?;
        if credential.is_empty() || public_key.is_empty() {
            return Err("an add needs a credential ID and a public key".into());
        }
        if people.knows_credential(credential) {
            return Err(Refused::new(
                Code::BadRequest,
                "riff knows this authenticator already: riff device list shows your keys",
            ));
        }
        let change = Change::KeyAdded(KeyAdded {
            user: user.to_owned(),
            name: name.clone(),
            credential: credential.clone(),
            public_key: public_key.clone(),
        });
        Ok((vec![change], ()))
    }

    fn reply(&self, caller: &Caller, view: &View<'_>, _: &Done, (): (), now: Now) -> DeviceInfo {
        let user = caller.who().user();
        view.riff.people().device_of(user, &self.name).map_or_else(
            || DeviceInfo {
                kind: DeviceKind::Key,
                name: self.name.clone(),
                added_ms: now.ms,
                used_ms: now.ms,
            },
            |key| key.info(&self.name),
        )
    }
}

/// Refuses the name `name` of a new key of `user`: a name that is not
/// allowed, a name of another device of the person, or a key over
/// [`KEYS_MOST`] (01M75NH33CVGBVASSACBPXRR2G). riff-server checks it at
/// the start of an add too.
///
/// ```
/// use riff_state::state::People;
/// use riff_state::state::devices::refuse_name;
///
/// let people = People::default();
/// assert!(refuse_name(&people, "mike", "yubikey").is_ok());
/// assert!(refuse_name(&people, "mike", "my key").is_err());
/// ```
pub fn refuse_name(people: &People, user: &str, name: &str) -> Result<(), Refused> {
    check_name(name).map_err(|reason| Refused::new(Code::BadRequest, reason))?;
    if people.device_of(user, name).is_some() {
        return Err(Refused::new(
            Code::BadRequest,
            format!(
                "you have a device with the name {name}: pick another name, or remove it first: \
                 riff device remove {name}"
            ),
        ));
    }
    if people.keys_of(user).count() >= KEYS_MOST {
        return Err(Refused::new(
            Code::BadRequest,
            format!(
                "you have {KEYS_MOST} keys, the most that riff keeps: remove one first with \
                 riff device remove NAME"
            ),
        ));
    }
    Ok(())
}

/// Removes the device `name` of the caller at once
/// (01M75NH3625QATGDT6DK652ZK6). The reply is the device that went.
impl Command for RemoveDevice {
    const KIND: CommandKind = CommandKind::RemoveDevice;
    type Reply = DeviceInfo;
    /// The device before its removal: the written copy has it no more.
    type Note = Option<DeviceInfo>;

    /// With the policy `required`, the removal needs a proof of a key of
    /// the person (PROOF_KEYS).
    fn proof(&self, _: &Caller, _: &View<'_>, _: Now) -> Option<Action> {
        Some(Action::RemoveDevice {
            name: self.name.clone(),
        })
    }

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _: Now,
    ) -> Result<(Vec<Change>, Option<DeviceInfo>), Refused> {
        let RemoveDevice { name } = self;
        let user = caller.who().user();
        let Some(key) = view.riff.people().device_of(user, name) else {
            return Err(Refused::new(
                Code::BadRequest,
                format!("you have no device with the name {name}: riff device list shows each"),
            ));
        };
        let change = Change::DeviceRemoved(DeviceRemoved {
            user: user.to_owned(),
            name: name.clone(),
        });
        Ok((vec![change], Some(key.info(name))))
    }

    fn reply(
        &self,
        _: &Caller,
        _: &View<'_>,
        _: &Done,
        note: Option<DeviceInfo>,
        _: Now,
    ) -> DeviceInfo {
        // A repeated call after a start has no note: the device went.
        note.unwrap_or_else(|| DeviceInfo {
            kind: DeviceKind::Key,
            name: self.name.clone(),
            added_ms: 0,
            used_ms: 0,
        })
    }
}

/// A proof of presence that riff-server checked: the key `name` of the
/// caller signed the challenge of `action` (PROOF_CHECK, PROOF_LOG).
/// riff-server sends it after its check of the signature, as the person
/// of the start of the proof. No client can send it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prove {
    /// The name of the key that signed.
    pub name: String,
    pub action: Action,
    /// The counter of the authenticator in the signed data.
    pub counter: u64,
}

/// The record `proved` of a proof. `handle` refuses a key that the
/// person does not have, and a counter that did not grow: when the key
/// or the proof counts, the counter of the proof is greater than the
/// counter of the last proof (PROOF_CHECK). An authenticator that counts
/// nothing gives 0 each time.
impl Command for Prove {
    const KIND: CommandKind = CommandKind::Prove;
    type Reply = ();
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let user = caller.who().user();
        let Some(key) = view.riff.people().device_of(user, &self.name) else {
            return Err(Refused::new(
                Code::BadProof,
                format!(
                    "you have no key with the name {}: riff device list shows each",
                    self.name
                ),
            ));
        };
        if (key.counter > 0 || self.counter > 0) && self.counter <= key.counter {
            return Err(Refused::new(
                Code::BadProof,
                format!(
                    "the counter of the key {} did not grow: riff takes no copy of a key. Remove \
                     the key with riff device remove {}, and add it again",
                    self.name, self.name
                ),
            ));
        }
        let change = Change::Proved(Proved {
            user: user.to_owned(),
            name: self.name.clone(),
            action: self.action.text(),
            counter: self.counter,
        });
        Ok((vec![change], ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, _: &Done, (): (), _: Now) {}
}

/// The devices of `user`, sorted by name: the reply of `POST
/// /v1/devices` (01M75NH3625QATGDT6DK652ZK6).
pub fn devices(people: &People, user: &str) -> DevicesReply {
    DevicesReply {
        devices: people
            .keys_of(user)
            .map(|(name, key)| key.info(name))
            .collect(),
    }
}
