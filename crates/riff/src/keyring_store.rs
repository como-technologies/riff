//! The keyring store of riff on Linux and BSD (01M556QPPRSQ91HDED128G2X7G).
//!
//! # Design
//!
//! [`secrets`](riff_client::secrets) keeps each secret in the default store of
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
//! # A new Secret Service
//!
//! A session belongs to one process of the Secret Service. After a
//! restart of `gnome-keyring-daemon`, the new process does not know the
//! session of the old one, and a prompt of the old one never completes.
//! A riff daemon runs for days, so the store sees each new process
//! (01M775GYAC3TYEJ2B504F8REE2). Before each call, it asks the bus, not
//! the keyring, for the owner of [`SECRETS`]. When the owner is another
//! process, or none, the store closes the old D-Bus connection and opens
//! a new connection with a new plain session. The close ends each call
//! that still waits on the old connection, for example a prompt: it
//! fails, and it frees the old session. The store makes one call at a
//! time on a session, so a call that waits for a prompt holds back the
//! later calls of its session. They fail after
//! [`KEYRING_WAIT`](riff_client::secrets::KEYRING_WAIT) as a keyring
//! that does not answer, up to the new process.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as a riff process
//!     participant B as the bus
//!     participant S as the Secret Service
//!     R->>S: OpenSession("plain"), at the first call
//!     R->>B: GetNameOwner(org.freedesktop.secrets), at each call
//!     R->>S: SearchItems(service, username)
//!     R->>S: GetSecret, CreateItem, SetSecret or Delete
//!     Note over S: gnome-keyring-daemon starts again
//!     R->>B: GetNameOwner: another process
//!     R->>R: closes the old connection
//!     R->>S: OpenSession("plain") on a new connection
//!     R->>S: SearchItems, then GetSecret
//! ```
//!
//! The store keeps a secret as the store of the `keyring` crate did: one
//! item in the default collection, with the attributes `service` and
//! `username`, the label `keyring:USERNAME@SERVICE` and the content type
//! [`CONTENT_TYPE`]. So riff reads each secret that an older riff wrote,
//! and an older riff reads each secret that this store writes.
//!
//! The test binary `keyring_store` checks the store on a private
//! keyring: a private D-Bus session bus with its own GNOME Keyring. It
//! also starts that keyring again under one store.
//!
//! # No prompt in the daemon
//!
//! A read or a write of a locked item asks the Secret Service to unlock
//! it, and that shows a prompt. In a process that asks for no prompt,
//! the riff daemon ([`riff_client::secrets::ask_no_prompt`]), the store
//! makes no `Unlock` call: a locked item, or a locked default collection
//! for a new item, is a locked keyring at once
//! (01M77AB7XZV8KG8RRZ8C6R5A8X). The `SearchItems` call before it shows
//! no prompt.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as the riff daemon
//!     participant S as the Secret Service
//!     D->>S: SearchItems(service, username)
//!     S-->>D: the item, locked
//!     Note over D: no Unlock call: the keyring is locked
//!     Note over S: a person unlocks it at the desktop
//!     D->>S: SearchItems, at the next look
//!     S-->>D: the item, unlocked
//!     D->>S: GetSecret
//! ```
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
//! // After a restart of the keyring, the same entry opens a new session.
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
use zbus::blocking::Connection;
use zbus::blocking::fdo::DBusProxy;

/// The content type of each secret that the store writes.
pub const CONTENT_TYPE: &str = "application/octet-stream";

/// The name of the Secret Service on the session bus.
pub const SECRETS: &str = "org.freedesktop.secrets";

/// A store of the Secret Service on the session bus of the process. It
/// holds one D-Bus connection with one plain session, for each entry
/// that it builds, and opens a new one for a new process of the Secret
/// Service.
pub struct Store {
    secrets: Arc<Secrets>,
}

impl Store {
    /// Connects to the Secret Service on the session bus, and opens one
    /// plain session.
    pub fn new() -> Result<Arc<Store>> {
        let link = Link::open()?;
        Ok(Arc::new(Store {
            secrets: Arc::new(Secrets {
                link: Mutex::new(Some(Arc::new(link))),
            }),
        }))
    }
}

impl CredentialStoreApi for Store {
    fn vendor(&self) -> String {
        "riff: the Secret Service with plain sessions".to_owned()
    }

    fn id(&self) -> String {
        format!("{:p}", Arc::as_ptr(&self.secrets))
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
            secrets: self.secrets.clone(),
            service: service.to_owned(),
            user: user.to_owned(),
        })))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The Secret Service of the session bus, as one store sees it.
struct Secrets {
    /// The link of the last open, or `None` after an open that failed.
    link: Mutex<Option<Arc<Link>>>,
}

impl Secrets {
    /// The link to the process that owns [`SECRETS`] now. When another
    /// process owns it, or none, it closes the old link and opens a new
    /// one.
    fn link(&self) -> Result<Arc<Link>> {
        let mut slot = lock(&self.link);
        if let Some(link) = slot.as_ref().filter(|link| link.current()) {
            return Ok(link.clone());
        }
        if let Some(old) = slot.take() {
            old.close();
        }
        let link = Arc::new(Link::open()?);
        *slot = Some(link.clone());
        Ok(link)
    }
}

/// One D-Bus connection with one plain session of one process of the
/// Secret Service.
struct Link {
    conn: Connection,
    bus: DBusProxy<'static>,
    /// The unique name of the process that owned [`SECRETS`] after the
    /// open of the session.
    owner: Option<String>,
    /// The session. It takes one call at a time.
    ss: Mutex<SecretService<'static>>,
}

impl Link {
    /// Connects to the session bus, and opens one plain session.
    fn open() -> Result<Link> {
        let conn = Connection::session().map_err(|e| failure(e.into()))?;
        let ss = SecretService::connect_with_existing(EncryptionType::Plain, conn.clone())
            .map_err(failure)?;
        let bus = DBusProxy::builder(&conn)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .map_err(|e| failure(e.into()))?;
        let owner = owner(&bus);
        Ok(Link {
            conn,
            bus,
            owner,
            ss: Mutex::new(ss),
        })
    }

    /// True when the process of the session still owns [`SECRETS`].
    fn current(&self) -> bool {
        same_process(self.owner.as_deref(), owner(&self.bus).as_deref())
    }

    /// Closes the connection. Each call that waits on it fails.
    fn close(&self) {
        let _ = self.conn.clone().close();
    }
}

/// The unique name of the owner of [`SECRETS`] on the bus, or `None`
/// when it has no owner or the bus does not answer.
fn owner(bus: &DBusProxy<'_>) -> Option<String> {
    let name = zbus::names::BusName::try_from(SECRETS).ok()?;
    bus.get_name_owner(name).ok().map(|owner| owner.to_string())
}

/// True when a session still belongs to the Secret Service: `opened` is
/// the owner of [`SECRETS`] at the open of the session, and `now` its
/// owner now. Only the same process knows the session.
///
/// ```
/// use riff::keyring_store::same_process;
///
/// assert!(same_process(Some(":1.7"), Some(":1.7")));
/// // gnome-keyring-daemon started again.
/// assert!(!same_process(Some(":1.7"), Some(":1.9")));
/// // No Secret Service now, or the bus does not answer.
/// assert!(!same_process(Some(":1.7"), None));
/// assert!(!same_process(None, None));
/// ```
pub fn same_process(opened: Option<&str>, now: Option<&str>) -> bool {
    opened.is_some() && opened == now
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Ok before an unlock that can show a prompt: this process can ask for
/// one ([`riff_client::secrets::prompts`]). Else the error of a locked
/// keyring, and riff asks the Secret Service for nothing
/// (01M77AB7XZV8KG8RRZ8C6R5A8X).
fn may_prompt() -> Result<()> {
    match riff_client::secrets::prompts() {
        true => Ok(()),
        false => Err(decode(secret_service::Error::Locked)),
    }
}

/// The secret `user` of `service`.
struct Cred {
    secrets: Arc<Secrets>,
    service: String,
    user: String,
}

impl Cred {
    /// Runs `call` on the session of the process that owns [`SECRETS`]
    /// now, after the calls before it on that session.
    fn on_session<T>(&self, call: impl FnOnce(&SecretService<'_>) -> Result<T>) -> Result<T> {
        let link = self.secrets.link()?;
        let ss = lock(&link.ss);
        call(&ss)
    }

    /// Runs `call` on the one item of this secret, after an unlock when
    /// it is locked. In a process that asks for no prompt, a locked item
    /// is a locked keyring at once ([`may_prompt`]). `None` when there is no
    /// item.
    fn with_item<T>(
        &self,
        ss: &SecretService<'_>,
        call: impl FnOnce(&Item<'_>) -> Result<T>,
    ) -> Result<Option<T>> {
        let found = ss
            .search_items(attributes(&self.service, &self.user))
            .map_err(decode)?;
        if !found.locked.is_empty() {
            may_prompt()?;
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
        self.on_session(|ss| {
            let set = |item: &Item| item.set_secret(secret, CONTENT_TYPE).map_err(decode);
            if self.with_item(ss, set)?.is_some() {
                return Ok(());
            }
            let collection = ss.get_default_collection().map_err(decode)?;
            if collection.is_locked().map_err(decode)? {
                may_prompt()?;
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
        })
    }

    fn get_secret(&self) -> Result<Vec<u8>> {
        self.on_session(|ss| {
            self.with_item(ss, |item| item.get_secret().map_err(decode))?
                .ok_or(Error::NoEntry)
        })
    }

    fn delete_credential(&self) -> Result<()> {
        self.on_session(|ss| {
            self.with_item(ss, |item| item.delete().map_err(decode))?
                .ok_or(Error::NoEntry)
        })
    }

    /// `None`: this entry is the credential.
    fn get_credential(&self) -> Result<Option<Arc<Credential>>> {
        self.on_session(|ss| self.with_item(ss, |_| Ok(()))?.ok_or(Error::NoEntry))?;
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
        let name = riff_client::login::secret_name("https://riff.example");
        let attributes = attributes(riff_client::secrets::SERVICE, &name);
        assert_eq!(
            attributes,
            HashMap::from([("service", "riff"), ("username", name.as_str())])
        );
        assert_eq!(
            label(riff_client::secrets::SERVICE, &name),
            "keyring:sign-in https://riff.example@riff"
        );
        assert_eq!(CONTENT_TYPE, "application/octet-stream");
    }

    /// A session of an old process of the Secret Service gets a new
    /// session: after a restart of the keyring, or when it has no owner.
    #[test]
    fn only_the_process_that_opened_a_session_keeps_it() {
        assert!(same_process(Some(":1.42"), Some(":1.42")));
        for (opened, now) in [
            (Some(":1.42"), Some(":1.43")),
            (Some(":1.42"), None),
            (None, Some(":1.42")),
            (None, None),
        ] {
            assert!(!same_process(opened, now), "{opened:?} {now:?}");
        }
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
