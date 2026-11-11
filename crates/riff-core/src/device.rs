//! The devices of a person (01M75NGT9Z0NSZTG97TBAMP7RS): the check of
//! a name, and the limits.
//!
//! A device of the kind [`DeviceKind::Key`](crate::wire::DeviceKind) is a
//! FIDO2 / WebAuthn authenticator of the person: a security key, a
//! passkey on a phone, or the authenticator of the laptop. A person adds
//! one with `riff device add --key NAME`, lists each with `riff device
//! list`, and removes one with `riff device remove NAME`. riff-server
//! keeps them in the log: see [`crate::record::KeyAdded`].
//!
//! With the feature `test-support`, the module `fake` is an
//! authenticator for the tests of each crate.
//!
//! # Example
//!
//! ```
//! use riff_core::device::{NAME_MOST, check_name};
//!
//! assert!(check_name("yubikey-5c").is_ok());
//! assert!(check_name("phone.2").is_ok());
//! assert!(check_name("").unwrap_err().contains("empty"));
//! assert!(check_name("my key").unwrap_err().contains("my key"));
//! assert!(check_name(&"k".repeat(NAME_MOST + 1)).is_err());
//! ```

#[cfg(any(test, feature = "test-support"))]
pub mod fake;

/// The longest name of a device, in characters
/// (01M75NH33CVGBVASSACBPXRR2G).
pub const NAME_MOST: usize = 64;

/// The most keys of one person (01M75NH33CVGBVASSACBPXRR2G).
pub const KEYS_MOST: usize = 10;

/// Checks the name of a device (01M75NH33CVGBVASSACBPXRR2G): 1 to
/// [`NAME_MOST`] characters, each an ASCII letter, a digit, `-`, `_` or
/// `.`. The error says why, for a person.
pub fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("the name of a device is empty: name it, for example yubikey".into());
    }
    if name.chars().count() > NAME_MOST {
        return Err(format!(
            "the name of a device has at most {NAME_MOST} characters"
        ));
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
    if !name.chars().all(allowed) {
        return Err(format!(
            "the name {name:?} has a character that is not allowed: use letters, digits, -, _ and ."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_has_only_the_allowed_characters() {
        for good in ["a", "yubikey", "YubiKey_5", "phone-2", "touch.id"] {
            assert!(check_name(good).is_ok(), "{good}");
        }
        for bad in ["a b", "ä", "key/1", "key#1", "key\n"] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_name_has_at_most_the_longest_length() {
        assert!(check_name(&"k".repeat(NAME_MOST)).is_ok());
        let error = check_name(&"k".repeat(NAME_MOST + 1)).unwrap_err();
        assert!(error.contains("at most 64"), "{error}");
    }
}
