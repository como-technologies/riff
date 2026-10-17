//! The keyring store of riff on Linux and BSD (01M556QPPRSQ91HDED128G2X7G).
//!
//! # Design
//!
//! [`secrets`](crate::secrets) keeps each secret in the default store of
//! `keyring-core`. On Linux and BSD, riff sets this store: a store of the
//! Secret Service, for example GNOME Keyring or KWallet, through the
//! `secret-service` crate. On each other OS, riff sets the store of the
//! `keyring` crate.
//!
//! The store opens its session of the Secret Service as a plain session,
//! as `gh` does. A DH session costs GNOME Keyring about 0.6 kB of locked
//! memory, and the keyring keeps that memory also after the session
//! closes. Each riff process that calls the keyring opens one session.
//! So the processes of a busy machine fill the 8192 kB of locked memory
//! of the keyring in about two days, and then the keyring stays locked
//! (#1003). A plain session costs no locked memory.
//!
//! With a plain session, a secret goes over the D-Bus session bus of the
//! person with no encryption. Only the processes of that user, and root,
//! can use that bus. Those processes can also ask the Secret Service for
//! the secret itself.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as a riff process
//!     participant S as the Secret Service
//!     R->>S: OpenSession("plain"), one time for the process
//!     R->>S: SearchItems(service, username)
//!     R->>S: GetSecret, CreateItem, SetSecret or Delete
//! ```
//!
//! The store keeps a secret as the store of the `keyring` crate did: one
//! item in the default collection, with the attributes `service` and
//! `username`, the label `keyring:USERNAME@SERVICE` and the content type
//! [`CONTENT_TYPE`]. So riff reads each secret that an older riff wrote,
//! and an older riff reads each secret that this store writes.
//!
//! The test binary `keyring_store` checks the store on a private
//! keyring: a private D-Bus session bus with its own GNOME Keyring.
//!
//! # Example
//!
//! ```no_run
//! use keyring_core::api::CredentialStoreApi;
//!
//! // One D-Bus connection and one plain session of the Secret Service.
//! let store = riff::keyring_store::Store::new()?;
//! let entry = store.build("riff", "sign-in https://riff.example", None)?;
//! entry.set_password("a-1")?;
//! assert_eq!(entry.get_password()?, "a-1");
//! # Ok::<(), keyring_core::Error>(())
//! ```

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use keyring_core::api::{Credential, CredentialApi, CredentialStoreApi};
use keyring_core::{Entry, Error, Result};
use secret_service::EncryptionType;
use secret_service::blocking::{Item, SecretService};

/// The content type of each secret that the store writes.
pub const CONTENT_TYPE: &str = "application/octet-stream";

/// A store of the Secret Service on the session bus of the process. It
/// holds one D-Bus connection with one plain session, for each entry
/// that it builds.
pub struct Store {
    ss: Arc<Mutex<SecretService<'static>>>,
}

impl Store {
    /// Connects to the Secret Service on the session bus, and opens one
    /// plain session.
    pub fn new() -> Result<Arc<Store>> {
        let ss = SecretService::connect(EncryptionType::Plain).map_err(failure)?;
        Ok(Arc::new(Store {
            ss: Arc::new(Mutex::new(ss)),
        }))
    }
}

impl CredentialStoreApi for Store {
    fn vendor(&self) -> String {
        "riff: the Secret Service with plain sessions".to_owned()
    }

    fn id(&self) -> String {
        format!("{:p}", Arc::as_ptr(&self.ss))
    }

    /// The entry of the secret `user` of `service`. The store takes no
    /// modifiers.
    fn build(
        &self,
        service: &str,
        user: &str,
        modifiers: Option<&HashMap<&str, &str>>,
    ) -> Result<Entry> {
        if modifiers.is_some_and(|m| !m.is_empty()) {
            return Err(Error::NotSupportedByStore(
                "the keyring store of riff takes no modifiers".to_owned(),
            ));
        }
        Ok(Entry::new_with_credential(Arc::new(Cred {
            ss: self.ss.clone(),
            service: service.to_owned(),
            user: user.to_owned(),
        })))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The secret `user` of `service`.
struct Cred {
    ss: Arc<Mutex<SecretService<'static>>>,
    service: String,
    user: String,
}

impl Cred {
    fn lock(&self) -> MutexGuard<'_, SecretService<'static>> {
        self.ss.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `call` on the one item of this secret, after an unlock when
    /// it is locked. `None` when there is no item.
    fn with_item<T>(
        &self,
        ss: &SecretService<'_>,
        call: impl FnOnce(&Item<'_>) -> Result<T>,
    ) -> Result<Option<T>> {
        let found = ss
            .search_items(attributes(&self.service, &self.user))
            .map_err(decode)?;
        if !found.locked.is_empty() {
            let locked: Vec<&Item> = found.locked.iter().collect();
            ss.unlock_all(&locked).map_err(decode)?;
        }
        let items: Vec<&Item> = found.unlocked.iter().chain(&found.locked).collect();
        match items[..] {
            [] => Ok(None),
            [item] => call(item).map(Some),
            _ => Err(Error::PlatformFailure(
                format!(
                    "the keyring has {} items with the service \"{}\" and the username \"{}\"",
                    items.len(),
                    self.service,
                    self.user
                )
                .into(),
            )),
        }
    }
}

impl CredentialApi for Cred {
    fn set_secret(&self, secret: &[u8]) -> Result<()> {
        let ss = self.lock();
        let set = |item: &Item| item.set_secret(secret, CONTENT_TYPE).map_err(decode);
        if self.with_item(&ss, set)?.is_some() {
            return Ok(());
        }
        let collection = ss.get_default_collection().map_err(decode)?;
        if collection.is_locked().map_err(decode)? {
            collection.unlock().map_err(decode)?;
        }
        collection
            .create_item(
                &label(&self.service, &self.user),
                attributes(&self.service, &self.user),
                secret,
                true,
                CONTENT_TYPE,
            )
            .map_err(failure)?;
        Ok(())
    }

    fn get_secret(&self) -> Result<Vec<u8>> {
        let ss = self.lock();
        self.with_item(&ss, |item| item.get_secret().map_err(decode))?
            .ok_or(Error::NoEntry)
    }

    fn delete_credential(&self) -> Result<()> {
        let ss = self.lock();
        self.with_item(&ss, |item| item.delete().map_err(decode))?
            .ok_or(Error::NoEntry)
    }

    /// `None`: this entry is the credential.
    fn get_credential(&self) -> Result<Option<Arc<Credential>>> {
        let ss = self.lock();
        self.with_item(&ss, |_| Ok(()))?.ok_or(Error::NoEntry)?;
        Ok(None)
    }

    fn get_specifiers(&self) -> Option<(String, String)> {
        Some((self.service.clone(), self.user.clone()))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The attributes of the item of the secret `user` of `service`: the
/// attributes of the store of the `keyring` crate.
///
/// ```
/// let attributes = riff::keyring_store::attributes("riff", "device-key http://a");
/// assert_eq!(attributes.len(), 2);
/// assert_eq!(attributes["service"], "riff");
/// assert_eq!(attributes["username"], "device-key http://a");
/// ```
pub fn attributes<'a>(service: &'a str, user: &'a str) -> HashMap<&'a str, &'a str> {
    HashMap::from([("service", service), ("username", user)])
}

/// The label of a new item: the label of the store of the `keyring`
/// crate.
///
/// ```
/// assert_eq!(
///     riff::keyring_store::label("riff", "sign-in http://a"),
///     "keyring:sign-in http://a@riff",
/// );
/// ```
pub fn label(service: &str, user: &str) -> String {
    format!("keyring:{user}@{service}")
}

/// The error of `keyring-core` for an error of the Secret Service. A
/// locked keyring and a dismissed prompt give no access, so riff says
/// that the keyring is locked (01M4385CCATXC0B8HV1XD6EFWG).
///
/// ```
/// use keyring_core::Error;
/// use riff::keyring_store::decode;
///
/// assert!(matches!(decode(secret_service::Error::Locked), Error::NoStorageAccess(_)));
/// assert!(matches!(decode(secret_service::Error::Prompt), Error::NoStorageAccess(_)));
/// assert!(matches!(decode(secret_service::Error::Unavailable), Error::PlatformFailure(_)));
/// ```
pub fn decode(error: secret_service::Error) -> Error {
    use secret_service::Error::{Locked, NoResult, Prompt};
    match error {
        Locked | NoResult | Prompt => Error::NoStorageAccess(Box::new(error)),
        _ => failure(error),
    }
}

fn failure(error: secret_service::Error) -> Error {
    Error::PlatformFailure(Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An older riff finds the items of this store with the attributes of
    /// the store of the `keyring` crate, and this store finds its items.
    #[test]
    fn the_item_has_the_attributes_and_the_label_of_the_keyring_crate() {
        let name = crate::login::secret_name("https://riff.example");
        let attributes = attributes(crate::secrets::SERVICE, &name);
        assert_eq!(
            attributes,
            HashMap::from([("service", "riff"), ("username", name.as_str())])
        );
        assert_eq!(
            label(crate::secrets::SERVICE, &name),
            "keyring:sign-in https://riff.example@riff"
        );
        assert_eq!(CONTENT_TYPE, "application/octet-stream");
    }

    /// A locked keyring is a locked keyring to riff, and each other
    /// error is a failure.
    #[test]
    fn each_error_of_the_secret_service_has_its_kind() {
        for error in [
            secret_service::Error::Locked,
            secret_service::Error::NoResult,
            secret_service::Error::Prompt,
        ] {
            assert!(matches!(decode(error), Error::NoStorageAccess(_)));
        }
        for error in [
            secret_service::Error::Unavailable,
            secret_service::Error::PromptDisconnected,
            secret_service::Error::Crypto("bad"),
        ] {
            assert!(matches!(decode(error), Error::PlatformFailure(_)));
        }
    }
}
