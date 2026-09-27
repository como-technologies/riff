//! Where `riff-server` saves its state (R30, R34).
//!
//! # Objects
//!
//! A store holds named objects. Each object is JSON.
//!
//! | Name | Holds |
//! |---|---|
//! | [`SESSIONS`] | The sessions, with their places, read cursors and claims. |
//! | [`THREADS`] + the thread name | One thread, with its members and messages. See [`thread_object`]. |
//! | [`TOKENS`] | The token store. |
//! | [`LEASE`] | The ID of the instance that may serve (R137). |
//!
//! The name of each thread object starts with [`THREADS`], and no other
//! name does (R147). A lifecycle rule of the bucket selects the thread
//! objects by that prefix (R46).
//!
//! # Versions
//!
//! Each saved object has a [`Version`]. A save names the version that
//! the server knows, or `None` for an object that the server thinks is
//! new. When the store holds another version, the save fails with
//! [`StoreError::Conflict`] (R141). So a second instance cannot write
//! over the changes of the first without notice.
//!
//! # Example
//!
//! ```
//! use futures::executor::block_on;
//! use riff_server::store::{Memory, Store, StoreError};
//!
//! let store = Memory::default();
//! block_on(async {
//!     let v1 = store.save("sessions", b"{}".to_vec(), None).await?;
//!     let loaded = store.load("sessions").await?.unwrap();
//!     assert_eq!(loaded.version, v1);
//!
//!     // A save that names an old version fails.
//!     let v2 = store.save("sessions", b"[]".to_vec(), Some(v1)).await?;
//!     let stale = store.save("sessions", b"{}".to_vec(), Some(v1)).await;
//!     assert!(matches!(stale, Err(StoreError::Conflict(_))));
//!     assert_eq!(store.load("sessions").await?.unwrap().version, v2);
//!     Ok::<(), StoreError>(())
//! })?;
//! # Ok::<(), StoreError>(())
//! ```

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::sync::{Arc, Mutex, MutexGuard};

use futures::future::{BoxFuture, FutureExt};
use riff_core::name::ThreadName;

/// The name of the object that holds the sessions.
pub const SESSIONS: &str = "sessions";

/// The start of the name of each thread object.
pub const THREADS: &str = "threads/";

/// The name of the object that holds the token store.
pub const TOKENS: &str = "tokens";

/// The name of the lease object.
pub const LEASE: &str = "lease";

/// The version of a saved object. Each save makes a new version. For
/// Cloud Storage, it is the generation of the object.
pub type Version = u64;

/// An object that a store holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded {
    pub bytes: Vec<u8>,
    pub version: Version,
}

/// Why a store call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// The store holds another version of the named object than the save
    /// named.
    Conflict(String),
    /// The store did not do the call.
    Failed(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Conflict(name) => write!(f, "another version of {name} is in the store"),
            StoreError::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for StoreError {}

/// A place that holds the saved state. See the module docs.
pub trait Store: Send + Sync {
    /// The object with this name, or `None` when there is none.
    fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>>;

    /// The name of each object whose name starts with `prefix`.
    fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>>;

    /// Saves an object if the store holds version `known` of it, or no
    /// object when `known` is `None`. Returns the new version.
    fn save<'a>(
        &'a self,
        name: &'a str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> BoxFuture<'a, Result<Version, StoreError>>;
}

/// The name of the object that holds a thread: [`THREADS`] and the
/// thread name. Each byte of the thread name outside `A-Z a-z 0-9 - _ .
/// ~` becomes `%XX`.
///
/// ```
/// use riff_server::store::thread_object;
///
/// let thread = "como-technologies/riff".parse()?;
/// assert_eq!(thread_object(&thread), "threads/como-technologies%2Friff");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn thread_object(thread: &ThreadName) -> String {
    let mut name = String::from(THREADS);
    for byte in thread.to_string().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            name.push(char::from(byte));
        } else {
            let _ = write!(name, "%{byte:02X}");
        }
    }
    name
}

/// A store in memory, for tests. Clones share the same objects, so a
/// test can start a new server on the store of an old one.
#[derive(Clone, Default)]
pub struct Memory(Arc<Mutex<Objects>>);

#[derive(Default)]
struct Objects {
    objects: BTreeMap<String, Loaded>,
    /// The last version that this store made.
    last: Version,
}

impl Memory {
    fn objects(&self) -> MutexGuard<'_, Objects> {
        self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

impl Store for Memory {
    fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
        let loaded = self.objects().objects.get(name).cloned();
        futures::future::ready(Ok(loaded)).boxed()
    }

    fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>> {
        let names = self
            .objects()
            .objects
            .keys()
            .filter(|name| name.starts_with(prefix))
            .cloned()
            .collect();
        futures::future::ready(Ok(names)).boxed()
    }

    fn save<'a>(
        &'a self,
        name: &'a str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> BoxFuture<'a, Result<Version, StoreError>> {
        let mut store = self.objects();
        let result = if store.objects.get(name).map(|o| o.version) == known {
            store.last += 1;
            let version = store.last;
            store
                .objects
                .insert(name.to_owned(), Loaded { bytes, version });
            Ok(version)
        } else {
            Err(StoreError::Conflict(name.to_owned()))
        };
        futures::future::ready(result).boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use riff_core::name::Who;

    #[test]
    fn a_new_object_needs_no_version() {
        let store = Memory::default();
        block_on(async {
            assert!(store.load("x").await.unwrap().is_none());
            let v = store.save("x", b"1".to_vec(), None).await.unwrap();
            // A second save as new fails: the object exists now.
            let again = store.save("x", b"2".to_vec(), None).await;
            assert_eq!(again, Err(StoreError::Conflict("x".into())));
            let loaded = store.load("x").await.unwrap().unwrap();
            assert_eq!(
                loaded,
                Loaded {
                    bytes: b"1".to_vec(),
                    version: v
                }
            );
        });
    }

    #[test]
    fn a_save_that_names_a_version_of_a_missing_object_fails() {
        let store = Memory::default();
        let result = block_on(store.save("x", b"1".to_vec(), Some(1)));
        assert!(matches!(result, Err(StoreError::Conflict(_))));
    }

    #[test]
    fn each_save_makes_a_new_version() {
        let store = Memory::default();
        block_on(async {
            let a = store.save("a", vec![], None).await.unwrap();
            let b = store.save("b", vec![], None).await.unwrap();
            let a2 = store.save("a", vec![], Some(a)).await.unwrap();
            assert!(a < b && b < a2);
        });
    }

    #[test]
    fn list_gives_the_names_with_a_prefix() {
        let store = Memory::default();
        block_on(async {
            for name in [SESSIONS, TOKENS, "threads/a", "threads/b"] {
                store.save(name, vec![], None).await.unwrap();
            }
            assert_eq!(
                store.list(THREADS).await.unwrap(),
                ["threads/a", "threads/b"]
            );
        });
    }

    #[test]
    fn clones_share_the_objects() {
        let store = Memory::default();
        let other = store.clone();
        block_on(async {
            store.save("x", b"1".to_vec(), None).await.unwrap();
            assert!(other.load("x").await.unwrap().is_some());
        });
    }

    #[test]
    fn only_thread_objects_start_with_the_thread_prefix() {
        for name in [SESSIONS, TOKENS, LEASE] {
            assert!(!name.starts_with(THREADS), "{name}");
        }
        let a = Who::new("mike", Some("a6cf")).unwrap();
        let b = Who::new("brett", Some("77e0")).unwrap();
        let direct = thread_object(&ThreadName::direct(&a, &b));
        assert!(direct.starts_with(THREADS));
        assert!(
            direct[THREADS.len()..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.~%".contains(c)),
            "{direct}"
        );
    }

    #[test]
    fn two_threads_never_share_an_object() {
        let a: ThreadName = "a/b".parse().unwrap();
        let b: ThreadName = "a%2Fb".parse().unwrap();
        assert_ne!(thread_object(&a), thread_object(&b));
    }
}
