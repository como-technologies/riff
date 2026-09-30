//! Where `riff-server` saves its state (R30, R34).
//!
//! # Objects
//!
//! A store holds named objects. Each object is JSON.
//!
//! | Name | Holds |
//! |---|---|
//! | [`crate::log::LOG`] and the first position | One chunk of the log. See [`crate::log`]. |
//! | [`TOKENS`] | The token store. |
//! | [`LEASE`] | The ID of the instance that may serve (R137). |
//!
//! # Stores
//!
//! | Store | For |
//! |---|---|
//! | [`Memory`] | Tests, and a `riff-server` with no bucket and no directory. |
//! | [`Dir`] | Files in a directory (`--dir`, `RIFF_DIR`), for local development. |
//! | [`crate::gcs::Gcs`] | A Cloud Storage bucket (`--bucket`). |
//!
//! # Versions
//!
//! Each saved object has a [`Version`]. A save names the version that
//! the server knows, or `None` for an object that the server thinks is
//! new. When the store holds another version, the save fails with
//! [`StoreError::Conflict`] (R141). So a second instance cannot write
//! over the changes of the first without notice.
//!
//! # An object that the server cannot read
//!
//! A store can hold state of an old format, for example a token store
//! from before a change of its fields. riff-server does not migrate it:
//! the load fails with [`StoreError::NotValid`]. Its text names the
//! object, with [`Store::locate`], and the fix: remove the old state,
//! with [`Store::empty_command`] when the store has one
//! (01M3MMXYS1V8CA89D2XHKPR6C4).
//!
//! ```
//! use riff_server::store::{Memory, StoreError, TOKENS};
//!
//! let error = StoreError::not_valid(&Memory::default(), TOKENS, "missing field `users`");
//! assert_eq!(
//!     error.to_string(),
//!     "cannot read the saved object tokens: missing field `users`. \
//!      It can be state of an old format. To start again with an empty state, \
//!      stop each server of this store and remove the old state."
//! );
//! ```
//!
//! # Example
//!
//! ```
//! use futures::executor::block_on;
//! use riff_server::store::{Memory, Store, StoreError};
//!
//! let store = Memory::default();
//! block_on(async {
//!     let v1 = store.save("tokens", b"{}".to_vec(), None).await?;
//!     let loaded = store.load("tokens").await?.unwrap();
//!     assert_eq!(loaded.version, v1);
//!
//!     // A save that names an old version fails.
//!     let v2 = store.save("tokens", b"[]".to_vec(), Some(v1)).await?;
//!     let stale = store.save("tokens", b"{}".to_vec(), Some(v1)).await;
//!     assert!(matches!(stale, Err(StoreError::Conflict(_))));
//!     assert_eq!(store.load("tokens").await?.unwrap().version, v2);
//!     Ok::<(), StoreError>(())
//! })?;
//! # Ok::<(), StoreError>(())
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use futures::future::{BoxFuture, FutureExt};
use sha2::{Digest, Sha256};

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
    /// The store holds an object that the server cannot read. `object`
    /// is its full name, from [`Store::locate`].
    NotValid {
        object: String,
        why: String,
        /// The command that removes the old state, when the store has
        /// one.
        fix: Option<String>,
    },
}

/// A saved object that the server cannot read: its name in the store,
/// and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreadable {
    pub name: String,
    pub why: String,
}

impl StoreError {
    /// The error for the object `name` of `store`, which the server
    /// cannot read for the reason `why`.
    pub fn not_valid(store: &dyn Store, name: &str, why: impl fmt::Display) -> StoreError {
        StoreError::NotValid {
            object: store.locate(name),
            why: why.to_string(),
            fix: store.empty_command(),
        }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Conflict(name) => write!(f, "another version of {name} is in the store"),
            StoreError::Failed(message) => f.write_str(message),
            StoreError::NotValid { object, why, fix } => {
                write!(
                    f,
                    "cannot read the saved object {object}: {why}. \
                     It can be state of an old format. To start again with an empty state, \
                     stop each server of this store and remove the old state"
                )?;
                match fix {
                    Some(command) => write!(f, ": {command}"),
                    None => f.write_str("."),
                }
            }
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

    /// The full name of the object `name`, for a person, for example
    /// `gs://BUCKET/tokens`.
    fn locate(&self, name: &str) -> String {
        name.to_owned()
    }

    /// A command that removes each object of the store, when the store
    /// has one.
    fn empty_command(&self) -> Option<String> {
        None
    }
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

/// A store of files in a directory, for local development. The name of
/// an object is its path in the directory: `log/…` is a file in the
/// directory `log`. The version of an object is a hash of its bytes, so
/// a second server on the same directory finds a change of the first.
///
/// ```
/// use futures::executor::block_on;
/// use riff_server::store::{Dir, Store, StoreError};
///
/// let dir = tempfile::tempdir().unwrap();
/// let store = Dir::new(dir.path());
/// block_on(async {
///     let v1 = store.save("log/1.jsonl", b"a".to_vec(), None).await?;
///     assert!(dir.path().join("log/1.jsonl").exists());
///     assert_eq!(store.load("log/1.jsonl").await?.unwrap().version, v1);
///     assert_eq!(store.list("log/").await?, ["log/1.jsonl"]);
///     // An object is new only once.
///     let again = store.save("log/1.jsonl", b"b".to_vec(), None).await;
///     assert!(matches!(again, Err(StoreError::Conflict(_))));
///     Ok::<(), StoreError>(())
/// })?;
/// # Ok::<(), StoreError>(())
/// ```
pub struct Dir {
    root: PathBuf,
}

impl Dir {
    pub fn new(root: impl Into<PathBuf>) -> Dir {
        Dir { root: root.into() }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn read(&self, name: &str) -> Result<Option<Loaded>, StoreError> {
        match std::fs::read(self.path(name)) {
            Ok(bytes) => Ok(Some(Loaded {
                version: version_of(&bytes),
                bytes,
            })),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(self.failed(name, &e)),
        }
    }

    fn names(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let mut names = Vec::new();
        walk(&self.root, &self.root, &mut names).map_err(|e| self.failed(prefix, &e))?;
        names.retain(|name| name.starts_with(prefix));
        names.sort();
        Ok(names)
    }

    fn write(
        &self,
        name: &str,
        bytes: &[u8],
        known: Option<Version>,
    ) -> Result<Version, StoreError> {
        let path = self.path(name);
        let fail = |e: std::io::Error| self.failed(name, &e);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(fail)?;
        }
        let now = self.read(name)?.map(|loaded| loaded.version);
        if now != known {
            return Err(StoreError::Conflict(name.to_owned()));
        }
        let temp = path.with_extension(format!("tmp-{}", std::process::id()));
        std::fs::write(&temp, bytes).map_err(fail)?;
        let done = match known {
            // A hard link fails when the object exists, so a new object
            // never replaces another.
            None => std::fs::hard_link(&temp, &path).map_err(|e| {
                if e.kind() == ErrorKind::AlreadyExists {
                    StoreError::Conflict(name.to_owned())
                } else {
                    fail(e)
                }
            }),
            Some(_) => std::fs::rename(&temp, &path).map_err(fail),
        };
        let _ = std::fs::remove_file(&temp);
        done.map(|()| version_of(bytes))
    }

    fn failed(&self, name: &str, error: &std::io::Error) -> StoreError {
        StoreError::Failed(format!("{}: {error}", self.locate(name)))
    }
}

impl Store for Dir {
    fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
        futures::future::ready(self.read(name)).boxed()
    }

    fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>> {
        futures::future::ready(self.names(prefix)).boxed()
    }

    fn save<'a>(
        &'a self,
        name: &'a str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> BoxFuture<'a, Result<Version, StoreError>> {
        futures::future::ready(self.write(name, &bytes, known)).boxed()
    }

    fn locate(&self, name: &str) -> String {
        self.path(name).display().to_string()
    }

    fn empty_command(&self) -> Option<String> {
        Some(format!("rm -r {}", self.root.display()))
    }
}

/// The version of an object in a [`Dir`]: the first 8 bytes of the hash
/// of its bytes.
fn version_of(bytes: &[u8]) -> Version {
    let hash = Sha256::digest(bytes);
    let mut first = [0; 8];
    first.copy_from_slice(&hash[..8]);
    Version::from_be_bytes(first)
}

/// Adds the name of each file under `dir`, with `/` between the parts.
/// It skips the temporary files of a write.
fn walk(root: &Path, dir: &Path, names: &mut Vec<String>) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            walk(root, &path, names)?;
        } else if !path
            .extension()
            .is_some_and(|e| e.to_string_lossy().starts_with("tmp-"))
        {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let parts: Vec<String> = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            names.push(parts.join("/"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

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
            for name in [TOKENS, "log/a", "log/b"] {
                store.save(name, vec![], None).await.unwrap();
            }
            assert_eq!(store.list("log/").await.unwrap(), ["log/a", "log/b"]);
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
    fn a_dir_store_finds_a_change_of_another_server() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (Dir::new(dir.path()), Dir::new(dir.path()));
        block_on(async {
            let v1 = a.save(TOKENS, b"1".to_vec(), None).await.unwrap();
            let v2 = b.save(TOKENS, b"2".to_vec(), Some(v1)).await.unwrap();
            let stale = a.save(TOKENS, b"3".to_vec(), Some(v1)).await;
            assert_eq!(stale, Err(StoreError::Conflict(TOKENS.into())));
            assert_eq!(a.load(TOKENS).await.unwrap().unwrap().version, v2);
            assert!(a.load("missing").await.unwrap().is_none());
        });
    }

    #[test]
    fn a_dir_store_lists_nothing_in_a_new_directory() {
        let dir = tempfile::tempdir().unwrap();
        let store = Dir::new(dir.path().join("new"));
        assert!(block_on(store.list("log/")).unwrap().is_empty());
        assert!(block_on(store.save(LEASE, b"x".to_vec(), None)).is_ok());
        assert_eq!(block_on(store.list("")).unwrap(), [LEASE]);
    }
}
