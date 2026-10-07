//! The secrets of `riff` live in the keyring store, and only there.

use keyring_core::Entry;
use riff::secrets::{self, SERVICE};

#[test]
fn secrets_round_trip_through_the_keyring_store() {
    crate::common::mock_keyring();
    let name = "access http://127.0.0.1:7878";

    secrets::set(name, "a-1").unwrap();
    // The value is in the store, under the riff service.
    let entry = Entry::new(SERVICE, name).unwrap();
    assert_eq!(entry.get_password().unwrap(), "a-1");
    assert_eq!(secrets::get(name).unwrap().as_deref(), Some("a-1"));

    secrets::delete(name).unwrap();
    assert!(entry.get_password().is_err());
    assert_eq!(secrets::get(name).unwrap(), None);
}
