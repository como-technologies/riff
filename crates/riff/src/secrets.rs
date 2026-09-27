//! Secrets in the OS keyring (R21).
//!
//! # Design
//!
//! `riff` keeps each token and each key in the OS keyring, and nowhere
//! else. No secret goes to a file or to an environment variable.
//!
//! | OS | Keyring |
//! |---|---|
//! | macOS | Keychain |
//! | Windows | Credential Manager |
//! | Linux and BSD | Secret Service, for example GNOME Keyring or KWallet |
//!
//! Each secret is one keyring entry. The service is [`SERVICE`]. The
//! account is the name of the secret. A name says what the secret is and
//! which server it is for, so one machine can use two servers.
//!
//! The module uses the default store of `keyring-core`. When no store is
//! set, the first call sets the store of the OS. A test sets the mock
//! store of `keyring-core` first, so tests never touch the real keyring.
//!
//! Each keyring error is an error, also when riff cannot open the
//! keyring. [`has_keyring`] tells the caller which case it is.
//!
//! # Example
//!
//! ```
//! # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
//! use riff::secrets;
//!
//! let name = "refresh http://127.0.0.1:7878";
//! assert_eq!(secrets::get(name)?, None);
//! secrets::set(name, "r-1")?;
//! assert_eq!(secrets::get(name)?.as_deref(), Some("r-1"));
//! secrets::delete(name)?;
//! assert_eq!(secrets::get(name)?, None);
//! # Ok::<(), anyhow::Error>(())
//! ```

use anyhow::{Context, Result, anyhow};
use keyring_core::{Entry, Error};

/// The keyring service of each riff secret (R82).
pub const SERVICE: &str = "riff";

/// Returns the secret with this name, or `None` if there is none.
pub fn get(name: &str) -> Result<Option<String>> {
    match entry(name)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(fail("read", name, e)),
    }
}

/// Keeps a secret with this name. It replaces an older value.
pub fn set(name: &str, value: &str) -> Result<()> {
    entry(name)?
        .set_password(value)
        .map_err(|e| fail("write", name, e))
}

/// Removes the secret with this name. A missing secret is not an error.
pub fn delete(name: &str) -> Result<()> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(fail("delete", name, e)),
    }
}

/// True when a keyring store is set, or riff can open the keyring of
/// the OS.
pub fn has_keyring() -> bool {
    keyring_core::get_default_store().is_some() || keyring::Entry::store_status().is_ok()
}

fn entry(name: &str) -> Result<Entry> {
    if keyring_core::get_default_store().is_none() {
        keyring::Entry::store_status()
            .as_ref()
            .map_err(|e| anyhow!("{e}"))
            .context(NO_KEYRING)?;
    }
    Entry::new(SERVICE, name).map_err(|e| fail("open", name, e))
}

const NO_KEYRING: &str = "riff cannot open the OS keyring. On Linux, start a Secret \
     Service, for example GNOME Keyring or KWallet";

fn fail(what: &str, name: &str, e: Error) -> anyhow::Error {
    anyhow!("{e}").context(format!(
        "riff cannot {what} the secret \"{name}\" in the OS keyring"
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Once;

    use keyring_core::mock;

    use super::*;

    fn mock_store() {
        static MOCK: Once = Once::new();
        MOCK.call_once(|| keyring_core::set_default_store(mock::Store::new().unwrap()));
    }

    #[test]
    fn set_replaces_the_value() {
        mock_store();
        set("unit replace", "a").unwrap();
        set("unit replace", "b").unwrap();
        assert_eq!(get("unit replace").unwrap().as_deref(), Some("b"));
    }

    #[test]
    fn names_keep_secrets_apart() {
        mock_store();
        set("unit one", "1").unwrap();
        set("unit two", "2").unwrap();
        assert_eq!(get("unit one").unwrap().as_deref(), Some("1"));
        assert_eq!(get("unit two").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn a_missing_secret_is_none_and_deletes_quietly() {
        mock_store();
        assert_eq!(get("unit missing").unwrap(), None);
        delete("unit missing").unwrap();
    }

    #[test]
    fn a_keyring_failure_names_the_secret() {
        mock_store();
        set("unit broken", "x").unwrap();
        let entry = Entry::new(SERVICE, "unit broken").unwrap();
        let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
        cred.set_error(Error::NoStorageAccess(Box::new(std::io::Error::other(
            "locked",
        ))));
        let error = format!("{:#}", get("unit broken").unwrap_err());
        assert!(
            error.contains("cannot read the secret \"unit broken\""),
            "{error}"
        );
        assert!(error.contains("locked"), "{error}");
    }
}
