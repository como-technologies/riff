//! The device key of this machine (R18).
//!
//! `riff` makes one P-256 key for each server, the first time that it
//! needs one. The private key goes to the OS keyring (see [`secrets`])
//! and never leaves the machine. The server binds each sign-in to the
//! key, so each request carries a proof from it (see
//! [`riff_core::dpop`]).
//!
//! # Example
//!
//! ```
//! # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
//! let key = riff::device::key("http://127.0.0.1:7878")?;
//! // The same server gives the same key.
//! let again = riff::device::key("http://127.0.0.1:7878")?;
//! assert_eq!(again.thumbprint(), key.thumbprint());
//! # Ok::<(), anyhow::Error>(())
//! ```

use anyhow::{Context, Result};
use riff_core::dpop::Key;

use crate::secrets;

/// The keyring name of the device key for `server`.
pub fn secret_name(server: &str) -> String {
    format!("device-key {}", server.trim_end_matches('/'))
}

/// The device key for `server`. Makes and keeps a new key if there is
/// none.
pub fn key(server: &str) -> Result<Key> {
    let name = secret_name(server);
    if let Some(secret) = secrets::get(&name)? {
        return Key::from_secret(&secret)
            .with_context(|| format!("the keyring entry \"{name}\" is not a device key"));
    }
    let key = Key::generate();
    secrets::set(&name, &key.to_secret())?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use std::sync::Once;

    use super::*;

    fn mock_store() {
        static MOCK: Once = Once::new();
        MOCK.call_once(|| {
            keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap())
        });
    }

    #[test]
    fn each_server_has_its_own_key() {
        mock_store();
        let a = key("http://a").unwrap();
        assert_eq!(key("http://a/").unwrap().thumbprint(), a.thumbprint());
        assert_ne!(key("http://b").unwrap().thumbprint(), a.thumbprint());
    }

    #[test]
    fn the_private_key_is_in_the_keyring() {
        mock_store();
        let k = key("http://kept").unwrap();
        let secret = secrets::get(&secret_name("http://kept")).unwrap().unwrap();
        assert_eq!(
            Key::from_secret(&secret).unwrap().thumbprint(),
            k.thumbprint()
        );
    }

    #[test]
    fn a_broken_entry_is_an_error() {
        mock_store();
        secrets::set(&secret_name("http://broken"), "not a key").unwrap();
        let error = format!("{:#}", key("http://broken").unwrap_err());
        assert!(error.contains("is not a device key"), "{error}");
    }
}
