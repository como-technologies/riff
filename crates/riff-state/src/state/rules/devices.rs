//! The rules of the devices of a person (01M75NGT9Z0NSZTG97TBAMP7RS,
//! 01M75NH2P3QHCYDKQH60KGSF97, 01M75NH30PNATYN33MXZQMVBW8,
//! 01M75NH33CVGBVASSACBPXRR2G, 01M75NH3625QATGDT6DK652ZK6): one test for
//! each rule of [`devices`](crate::state::devices), in the form given,
//! when, then.

use riff_core::device::KEYS_MOST;
use riff_core::record::{DeviceRemoved, KeyAdded};
use riff_core::wire::{DeviceKind, RemoveDevice};

use super::*;
use crate::state::AddKey;

fn add(name: &str, credential: &str) -> Whole<AddKey> {
    Whole(AddKey {
        name: name.into(),
        credential: credential.into(),
        public_key: "pQ".into(),
    })
}

fn key_added(user: &str, name: &str, credential: &str) -> Change {
    Change::KeyAdded(KeyAdded {
        user: user.into(),
        name: name.into(),
        credential: credential.into(),
        public_key: "pQ".into(),
    })
}

fn remove(name: &str) -> Whole<RemoveDevice> {
    Whole(RemoveDevice { name: name.into() })
}

fn device_removed(user: &str, name: &str) -> Change {
    Change::DeviceRemoved(DeviceRemoved {
        user: user.into(),
        name: name.into(),
    })
}

fn ann_person() -> Caller {
    Caller::of(&person())
}

fn bob_person() -> Caller {
    Caller::of(&"riff://bob@kite".parse().unwrap())
}

/// A person adds a key: the record names the USER of the caller.
#[test]
fn a_person_adds_a_key() {
    given(&[])
        .when_as(&ann_person(), add("yubikey", "c1"))
        .then(&[key_added("ann", "yubikey", "c1")]);
}

/// A session cannot add a key or remove a device, also as the owner.
#[test]
fn a_session_cannot_add_or_remove_a_device() {
    let session = Caller::of(&ann()).with_role(Role::Owner);
    given(&[])
        .when_as(&session, add("yubikey", "c1"))
        .then_refused_as(
            Code::NotAllowed,
            "a session cannot send the command add_key",
        );
    given(&[key_added("ann", "yubikey", "c1")])
        .when_as(&session, remove("yubikey"))
        .then_refused_as(
            Code::NotAllowed,
            "a session cannot send the command remove_device",
        );
}

/// Two keys of one person have two names. The same name for another
/// person is good.
#[test]
fn a_name_of_another_device_of_the_person_is_refused() {
    let has = [key_added("ann", "yubikey", "c1")];
    given(&has)
        .when_as(&ann_person(), add("yubikey", "c2"))
        .then_refused_as(Code::BadRequest, "you have a device with the name yubikey");
    given(&has)
        .when_as(&ann_person(), add("phone", "c2"))
        .then(&[key_added("ann", "phone", "c2")]);
    given(&has)
        .when_as(&bob_person(), add("yubikey", "c2"))
        .then(&[key_added("bob", "yubikey", "c2")]);
}

/// A credential that a person of the riff has is refused: one
/// authenticator gives one key.
#[test]
fn a_known_credential_is_refused() {
    let has = [key_added("ann", "yubikey", "c1")];
    for caller in [ann_person(), bob_person()] {
        given(&has)
            .when_as(&caller, add("other", "c1"))
            .then_refused_as(Code::BadRequest, "riff knows this authenticator already");
    }
}

/// A name with a character that is not allowed is refused.
#[test]
fn a_name_that_is_not_allowed_is_refused() {
    given(&[])
        .when_as(&ann_person(), add("my key", "c1"))
        .then_refused_as(Code::BadRequest, "not allowed");
    given(&[])
        .when_as(&ann_person(), add("", "c1"))
        .then_refused_as(Code::BadRequest, "empty");
}

/// A person has at most [`KEYS_MOST`] keys.
#[test]
fn a_key_over_the_most_is_refused() {
    let has: Vec<Change> = (0..KEYS_MOST)
        .map(|n| key_added("ann", &format!("k{n}"), &format!("c{n}")))
        .collect();
    given(&has)
        .when_as(&ann_person(), add("one-more", "c-more"))
        .then_refused_as(Code::BadRequest, "you have 10 keys");
    given(&has[1..])
        .when_as(&ann_person(), add("one-more", "c-more"))
        .then(&[key_added("ann", "one-more", "c-more")]);
}

/// A person removes a device of their own, by its name. A name that the
/// person has no device with is refused, also a name of a device of
/// another person.
#[test]
fn a_person_removes_a_device_of_their_own() {
    let has = [key_added("ann", "yubikey", "c1")];
    given(&has)
        .when_as(&ann_person(), remove("yubikey"))
        .then(&[device_removed("ann", "yubikey")]);
    given(&has)
        .when_as(&ann_person(), remove("phone"))
        .then_refused_as(Code::BadRequest, "you have no device with the name phone");
    given(&has)
        .when_as(&bob_person(), remove("yubikey"))
        .then_refused_as(Code::BadRequest, "you have no device with the name yubikey");
}

/// The state keeps the time of the record as the time of the add and of
/// the last use. A removed device is gone, and its name is free again.
#[test]
fn the_state_keeps_the_times_of_each_key_and_drops_a_removed_one() {
    let state = given(&[]).apply(&[key_added("ann", "yubikey", "c1")]);
    let devices = state.devices("ann").devices;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].kind, DeviceKind::Key);
    assert_eq!(devices[0].name, "yubikey");
    assert_eq!(devices[0].added_ms, devices[0].used_ms);
    assert_eq!(state.credentials("ann"), ["c1"]);
    assert!(state.devices("bob").devices.is_empty());

    let records = [
        key_added("ann", "yubikey", "c1"),
        device_removed("ann", "yubikey"),
    ];
    let state = given(&records).apply(&[]);
    assert!(state.devices("ann").devices.is_empty());
    assert!(state.refuse_key_name("ann", "yubikey").is_ok());
}

/// A record that `handle` refuses changes nothing in a replay: a second
/// name, a known credential, and the removal of a device that is gone.
#[test]
fn a_record_that_handle_refuses_changes_nothing() {
    let records = [
        key_added("ann", "yubikey", "c1"),
        key_added("ann", "yubikey", "c2"),
        key_added("bob", "other", "c1"),
        device_removed("bob", "gone"),
    ];
    let state = given(&records).apply(&[]);
    assert_eq!(state.credentials("ann"), ["c1"]);
    assert!(state.devices("bob").devices.is_empty());
}
