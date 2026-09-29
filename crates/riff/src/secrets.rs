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
//! Each call to the OS keyring runs on a thread of its own, and riff
//! waits at most [`KEYRING_WAIT`] for it (01M3NBTZDT67WD9ZX0RHDVCW9T).
//! A keyring that does not answer, for example a Secret Service that
//! waits for an unlock, is an error that says so. It never stops the
//! process that asked, and never stops its Ctrl-C.
//!
//! With `RIFF_HOME`, riff keeps each secret in a file of
//! `$RIFF_HOME/secrets` and never opens the OS keyring
//! (01M3MY2KSV73WS8D902YCH2PRX). Only the tests and `just dev` set it.
//! The file name is the name of the secret in hex. Only the owner can
//! read the files.
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

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use keyring_core::{Entry, Error};

/// The keyring service of each riff secret (R82).
pub const SERVICE: &str = "riff";

/// The longest time that riff waits for one call to the OS keyring.
pub const KEYRING_WAIT: Duration = Duration::from_secs(10);

/// Returns the secret with this name, or `None` if there is none.
pub fn get(name: &str) -> Result<Option<String>> {
    if let Some(dir) = files() {
        return file_get(&dir, name);
    }
    let owned = name.to_owned();
    in_time(KEYRING_WAIT, move || match entry(&owned)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(fail("read", &owned, e)),
    })?
}

/// Keeps a secret with this name. It replaces an older value.
pub fn set(name: &str, value: &str) -> Result<()> {
    if let Some(dir) = files() {
        return file_set(&dir, name, value);
    }
    let (owned, value) = (name.to_owned(), value.to_owned());
    in_time(KEYRING_WAIT, move || {
        entry(&owned)?
            .set_password(&value)
            .map_err(|e| fail("write", &owned, e))
    })?
}

/// Removes the secret with this name. A missing secret is not an error.
pub fn delete(name: &str) -> Result<()> {
    if let Some(dir) = files() {
        return file_delete(&dir, name);
    }
    let owned = name.to_owned();
    in_time(KEYRING_WAIT, move || {
        match entry(&owned)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(fail("delete", &owned, e)),
        }
    })?
}

/// True when riff keeps secrets in files, a keyring store is set, or
/// riff can open the keyring of the OS. A keyring that does not answer
/// counts as one, so that the next call says that it does not answer.
pub fn has_keyring() -> bool {
    files().is_some()
        || keyring_core::get_default_store().is_some()
        || in_time(KEYRING_WAIT, || keyring::Entry::store_status().is_ok()).unwrap_or(true)
}

/// Runs `call` on a thread of its own, and waits at most `wait` for it.
/// A call that takes longer is an error; its thread goes on alone.
///
/// ```
/// use std::time::Duration;
/// use riff::secrets::in_time;
///
/// assert_eq!(in_time(Duration::from_secs(5), || 7).unwrap(), 7);
/// let slow = in_time(Duration::from_millis(10), || std::thread::sleep(Duration::from_secs(5)));
/// assert!(format!("{:#}", slow.unwrap_err()).contains("the OS keyring does not answer"));
/// ```
pub fn in_time<T: Send + 'static>(
    wait: Duration,
    call: impl FnOnce() -> T + Send + 'static,
) -> Result<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("keyring".into())
        .spawn(move || {
            let _ = tx.send(call());
        })
        .context("riff cannot start a thread for the OS keyring")?;
    rx.recv_timeout(wait).map_err(|_| {
        anyhow!(
            "the OS keyring does not answer in {} seconds. Unlock it, or check its Secret Service",
            wait.as_secs_f32()
        )
    })
}

/// The directory of the secret files: `$RIFF_HOME/secrets`, or `None`
/// without `RIFF_HOME`.
fn files() -> Option<PathBuf> {
    crate::home::dir().map(|home| home.join("secrets"))
}

/// The file of the secret `name` in `dir`.
///
/// ```
/// # use std::path::Path;
/// let file = riff::secrets::file_of(Path::new("/s"), "access http://a");
/// assert_eq!(file, Path::new("/s/61636365737320687474703a2f2f61"));
/// ```
pub fn file_of(dir: &Path, name: &str) -> PathBuf {
    dir.join(name.bytes().map(|b| format!("{b:02x}")).collect::<String>())
}

/// Returns the secret `name` from the files in `dir`.
///
/// ```
/// use riff::secrets::{file_get, file_set, file_delete};
///
/// let dir = tempfile::tempdir()?;
/// let dir = dir.path().join("secrets");
/// assert_eq!(file_get(&dir, "k")?, None);
/// file_set(&dir, "k", "v-1")?;
/// file_set(&dir, "k", "v-2")?;
/// assert_eq!(file_get(&dir, "k")?.as_deref(), Some("v-2"));
/// file_delete(&dir, "k")?;
/// file_delete(&dir, "k")?;
/// assert_eq!(file_get(&dir, "k")?, None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn file_get(dir: &Path, name: &str) -> Result<Option<String>> {
    let file = file_of(dir, name);
    match std::fs::read_to_string(&file) {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(file_fail("read", name, &file, e)),
    }
}

/// Keeps the secret `name` in a file of `dir`. It replaces an older
/// value. Only the owner can read the file.
pub fn file_set(dir: &Path, name: &str, value: &str) -> Result<()> {
    let file = file_of(dir, name);
    let fail = |e| file_fail("write", name, &file, e);
    std::fs::create_dir_all(dir).map_err(fail)?;
    let mut new = tempfile::NamedTempFile::new_in(dir).map_err(fail)?;
    std::io::Write::write_all(&mut new, value.as_bytes()).map_err(fail)?;
    new.persist(&file).map_err(|e| fail(e.error))?;
    Ok(())
}

/// Removes the secret `name` from the files in `dir`. A missing secret
/// is not an error.
pub fn file_delete(dir: &Path, name: &str) -> Result<()> {
    let file = file_of(dir, name);
    match std::fs::remove_file(&file) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(file_fail("delete", name, &file, e)),
    }
}

fn file_fail(what: &str, name: &str, file: &Path, e: std::io::Error) -> anyhow::Error {
    anyhow::Error::new(e).context(format!(
        "riff cannot {what} the secret \"{name}\" in {}",
        file.display()
    ))
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
