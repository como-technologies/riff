//! The key of the riff daemon (01M519Z2BB0YJY5CRMB54JVYFA,
//! 01M519Z2DSFGHNV5S88AYFK7PY, 01M51BFYNMYHSQ62BBJES3CCWT).
//!
//! # Design
//!
//! Each riff daemon has one ed25519 [`Key`]. The daemon makes it at its
//! first start, and keeps it in the OS keyring with [`crate::secrets`]:
//! the secret [`secret_name`] of the folder of the daemon holds the 32
//! bytes of the secret key as 64 hex characters. With `RIFF_HOME`, a
//! file of the tests holds it. Each folder of a daemon has its own key,
//! so the daemon of a test never reads the key of the daemon of the
//! machine (01M51JW1XRZWC3Q7G32V0X0D89).
//!
//! The 32 bytes are the form that iroh takes as the key of an endpoint:
//! `iroh::SecretKey::from_bytes(&key.to_bytes())`. So the public key
//! ([`Public`]) is also the endpoint ID of the daemon, and its text is
//! the text of an iroh endpoint ID: 64 hex characters in lower case.
//! The riff of the daemon has this text as its ID
//! ([`crate::daemon`]).
//!
//! The secret key never leaves the machine, and never goes into a log
//! line, a message, a record or a file of a clone. A [`Key`] has no
//! `Display` and no `Serialize`, and its `Debug` shows only the public
//! key. Its bytes, and the text of the secret, are zeroed after use.
//!
//! # A locked keyring
//!
//! [`wait`] reads the key. A keyring that is locked, does not answer or
//! gives another error is not a lost key (01M51BFYNMYHSQ62BBJES3CCWT,
//! 01M51JW20CSAECFRZTJ95P6F3E): [`wait`] looks again after the retry
//! time ([`crate::secrets::retry`]). A [`Gate`] says one line when the
//! keyring refuses, and one when it answers again. Only a keyring that
//! answers with no key ([`Found::None`]), or with a value that is not a
//! key ([`Found::NotAKey`]), gives the daemon a new key.
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Look
//!     Look --> Wait: an error of the keyring (one line)
//!     Wait --> Look: after the retry time
//!     Look --> Key: the key (one line after a wait)
//!     Look --> NoKey: no key, or a value that is not a key
//! ```
//!
//! # Example
//!
//! ```standalone_crate
//! # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
//! use std::path::Path;
//! use riff::daemon_key::{Found, Key, keep, read, secret_name};
//!
//! let name = secret_name(Path::new("/h/.local/share/riff/daemon"));
//! assert_eq!(name, "daemon-key /h/.local/share/riff/daemon");
//! assert!(matches!(read(&name)?, Found::None));
//!
//! let key = Key::generate();
//! keep(&name, &key)?;
//! let Found::Key(again) = read(&name)? else { panic!("no key") };
//! assert_eq!(again.public(), key.public());
//! assert_eq!(again.to_bytes(), key.to_bytes());
//!
//! // The Debug of a key shows only the public key.
//! let public = key.public().to_string();
//! assert_eq!(format!("{key:?}"), format!("Key {{ public: {public} }}"));
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::fmt;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use zeroize::{Zeroize, Zeroizing};

use crate::secrets::{self, Gate, Said};
use crate::text;

/// The name of the secret of the key of the daemon in the folder `dir`.
pub fn secret_name(dir: &Path) -> String {
    format!("daemon-key {}", dir.display())
}

/// The ed25519 key of a daemon. Its bytes are zeroed at its drop. It
/// stays in one place on the heap: a move of the key copies no secret.
pub struct Key(Box<SigningKey>);

impl Key {
    /// A new key from the random source of the OS.
    pub fn generate() -> Key {
        let mut bytes = Zeroizing::new([0u8; 32]);
        // The OS random source fails only when the OS is broken.
        getrandom::fill(bytes.as_mut()).expect("the OS gives random bytes");
        Key::from_bytes(&bytes)
    }

    /// The key with the 32 secret bytes `bytes`.
    pub fn from_bytes(bytes: &[u8; 32]) -> Key {
        Key(Box::new(SigningKey::from_bytes(bytes)))
    }

    /// The 32 secret bytes: the form of `iroh::SecretKey::from_bytes`.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// The public key.
    pub fn public(&self) -> Public {
        Public(self.0.verifying_key().to_bytes())
    }

    /// The ed25519 signature of `message` by this key.
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        use ed25519_dalek::Signer;
        self.0.sign(message).to_bytes()
    }

    /// The key from the text of its secret: 64 hex characters.
    fn from_secret(text: &str) -> Option<Key> {
        let bytes = Zeroizing::new(hex32(text.trim())?);
        Some(Key::from_bytes(&bytes))
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Key")
            .field("public", &format_args!("{}", self.public()))
            .finish()
    }
}

/// The public key of a daemon. Its text is the text of an iroh
/// endpoint ID: 64 hex characters in lower case.
///
/// ```
/// let key = riff::daemon_key::Key::from_bytes(&[1; 32]);
/// assert_eq!(
///     key.public().to_string(),
///     "8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c"
/// );
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Public([u8; 32]);

impl Public {
    /// The public key with the 32 bytes `bytes`, for example of the
    /// author of a record ([`crate::peer::record`]). It does not check
    /// that the bytes are a point of the curve: a check of a signature
    /// does.
    pub fn from_bytes(bytes: [u8; 32]) -> Public {
        Public(bytes)
    }

    /// The 32 bytes of the public key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The public key from its text: 64 hex characters.
    ///
    /// ```
    /// use riff::daemon_key::{Key, Public};
    ///
    /// let public = Key::from_bytes(&[1; 32]).public();
    /// assert_eq!(Public::parse(&public.to_string()), Some(public));
    /// assert_eq!(Public::parse("8a88"), None);
    /// ```
    pub fn parse(text: &str) -> Option<Public> {
        hex32(text).map(Public)
    }

    /// The signature `signature` by this key over `message`, checked with
    /// `verify_strict` (01M519Z1Z1FVZQQ612DEMSQ7PG).
    pub fn verifies(&self, message: &[u8], signature: &[u8; 64]) -> bool {
        let signature = ed25519_dalek::Signature::from_bytes(signature);
        ed25519_dalek::VerifyingKey::from_bytes(&self.0)
            .is_ok_and(|key| key.verify_strict(message, &signature).is_ok())
    }
}

/// The 32 bytes of the text `text`: 64 hex characters.
pub(crate) fn hex32(text: &str) -> Option<[u8; 32]> {
    let text = text.as_bytes();
    if text.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (byte, pair) in bytes.iter_mut().zip(text.chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(bytes)
}

impl serde::Serialize for Public {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Public {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Public::parse(&text)
            .ok_or_else(|| serde::de::Error::custom("not a public key: 64 hex characters"))
    }
}

impl fmt::Display for Public {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl fmt::Debug for Public {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Public({self})")
    }
}

/// What the keyring holds for the key of a daemon.
#[derive(Debug)]
pub enum Found {
    /// The key.
    Key(Key),
    /// The keyring answers with no key.
    None,
    /// The keyring holds a value that is not a key: not 64 hex
    /// characters.
    NotAKey,
}

/// Reads the key in the secret `name`. An error of the keyring is an
/// error.
pub fn read(name: &str) -> Result<Found> {
    let Some(text) = secrets::get(name)?.map(Zeroizing::new) else {
        return Ok(Found::None);
    };
    Ok(Key::from_secret(&text).map_or(Found::NotAKey, Found::Key))
}

/// Keeps `key` in the secret `name`. It replaces an older key.
pub fn keep(name: &str, key: &Key) -> Result<()> {
    let mut bytes = key.to_bytes();
    let text = Zeroizing::new(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
    bytes.zeroize();
    secrets::set(name, &text)
}

/// Reads the key in the secret `name`, and waits while the keyring
/// gives an error (01M51BFYNMYHSQ62BBJES3CCWT): it looks again after
/// `retry`, and makes no key. It gives `say` one line when the keyring
/// refuses, and one when it answers again.
pub async fn wait(name: &str, retry: Duration, mut say: impl FnMut(String)) -> Result<Found> {
    let mut gate = Gate::default();
    loop {
        let owned = name.to_owned();
        let found = tokio::task::spawn_blocking(move || read(&owned)).await?;
        let line = match &found {
            Ok(_) => gate
                .look(Instant::now(), false)
                .map(|_| text::daemon_keyring_back()),
            Err(e) => gate
                .look(Instant::now(), true)
                .filter(|said| *said == Said::Locked)
                .map(|_| text::daemon_keyring_waits(&format!("{e:#}"), retry)),
        };
        if let Some(line) = line {
            say(line);
        }
        if found.is_ok() {
            return found;
        }
        tokio::time::sleep(retry).await;
    }
}

#[cfg(test)]
mod tests {
    use keyring_core::{Entry, Error, mock};

    use super::*;
    use crate::secrets::mock_store;

    #[test]
    fn a_secret_that_is_not_a_key_is_no_key() {
        mock_store();
        for text in ["", "not a key", &"0".repeat(63), &"g".repeat(64)] {
            secrets::set("daemon-key /unit/not-a-key", text).unwrap();
            // The name of the variant only: no key goes to the message.
            let found = match read("daemon-key /unit/not-a-key").unwrap() {
                Found::Key(_) => "a key",
                Found::None => "no key",
                Found::NotAKey => "not a key",
            };
            assert_eq!(found, "not a key", "{text:?}");
        }
    }

    #[test]
    fn the_secret_is_the_hex_of_the_bytes() {
        mock_store();
        let key = Key::from_bytes(&[0xab; 32]);
        keep("daemon-key /unit/hex", &key).unwrap();
        let text = secrets::get("daemon-key /unit/hex").unwrap().unwrap();
        assert_eq!(text, "ab".repeat(32));
        assert!(!format!("{key:?}").contains(&text));
    }

    #[test]
    fn two_new_keys_differ() {
        assert_ne!(Key::generate().public(), Key::generate().public());
    }

    /// A refusal of the keyring is one line. The next look reads the
    /// same key, and says that the keyring answers again.
    #[tokio::test]
    async fn the_wait_looks_again_and_says_two_lines() {
        mock_store();
        let name = "daemon-key /unit/wait";
        let key = Key::generate();
        keep(name, &key).unwrap();
        let entry = Entry::new(secrets::SERVICE, name).unwrap();
        let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
        cred.set_error(Error::NoStorageAccess(Box::new(std::io::Error::other(
            "prompt dismissed",
        ))));
        let mut lines = Vec::new();
        let found = wait(name, Duration::from_millis(10), |line| lines.push(line))
            .await
            .unwrap();
        let Found::Key(again) = found else {
            panic!("the wait gave no key")
        };
        assert_eq!(again.public(), key.public());
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("prompt dismissed"), "{lines:?}");
        assert_eq!(lines[1], text::daemon_keyring_back());
    }
}
