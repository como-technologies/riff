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
//! A locked keyring, or one that does not answer, gives a [`Locked`]
//! error. Its text names the host, says to unlock the keyring at the
//! desktop, and says that `gh` stops too (01M4385CCATXC0B8HV1XD6EFWG).
//! [`is_locked`] finds it in an error. A process that runs for a long
//! time, for example a workers host, looks at the keyring each
//! [`KEYRING_RETRY`] with a [`Gate`]. The gate says one line when the
//! keyring locks and one when it answers again, not one line for each
//! call (01M4385CEWGCP31DP5PAMPXZ97).
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Answers
//!     Answers --> Locked: a look is refused (one line, one note to the lead)
//!     Locked --> Locked: a look each 30 s is refused (no line)
//!     Locked --> Answers: a look gets an answer (one line, one note to the lead)
//! ```
//!
//! In a session with its secrets in the environment
//! ([`crate::grant::in_session`]), each call is an error, and
//! [`has_keyring`] is false: no process of a session opens the keyring
//! of the person, also with `RIFF_HOME` (01M4CVXJ7ZDAVRKJ8Y59R3KPDV).
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

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use keyring_core::{Entry, Error};

/// The keyring service of each riff secret (R82).
pub const SERVICE: &str = "riff";

/// The longest time that riff waits for one call to the OS keyring.
pub const KEYRING_WAIT: Duration = Duration::from_secs(10);

/// The time between two looks at a keyring that is locked
/// (01M4385CEWGCP31DP5PAMPXZ97).
pub const KEYRING_RETRY: Duration = Duration::from_secs(30);

/// The variable that gives [`KEYRING_RETRY`] in seconds, for tests.
pub const RETRY_VAR: &str = "RIFF_KEYRING_RETRY";

/// The time between two looks at the keyring: [`KEYRING_RETRY`], or
/// the seconds of [`RETRY_VAR`].
pub fn retry() -> Duration {
    std::env::var(RETRY_VAR)
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&s| s > 0)
        .map_or(KEYRING_RETRY, Duration::from_secs)
}

/// The OS keyring of `host` is locked, or it does not answer
/// (01M4385CCATXC0B8HV1XD6EFWG).
///
/// ```
/// use riff::secrets::{Locked, is_locked};
///
/// let locked = Locked { host: "pangolin".into() };
/// assert_eq!(
///     locked.to_string(),
///     "the OS keyring of pangolin is locked or does not answer: unlock it at the desktop. \
///      gh uses the same keyring, so gh stops too",
/// );
/// let error = anyhow::anyhow!("prompt dismissed").context(locked);
/// assert!(is_locked(&error.context("riff cannot find your user")));
/// assert!(!is_locked(&anyhow::anyhow!("no entry")));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locked {
    pub host: String,
}

impl Locked {
    /// The keyring of this machine.
    pub fn here() -> Self {
        Locked {
            host: crate::identity::this_host(),
        }
    }
}

impl fmt::Display for Locked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&crate::text::keyring_locked(&self.host))
    }
}

/// True when `error` holds a [`Locked`]: the keyring is locked or does
/// not answer.
pub fn is_locked(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Locked>().is_some()
}

/// What a process that runs for a long time knows of its keyring, so
/// that it says one line when the keyring locks and one when it answers
/// again (01M4385CEWGCP31DP5PAMPXZ97).
///
/// ```
/// use std::time::{Duration, Instant};
/// use riff::secrets::{Gate, Said};
///
/// let mut gate = Gate::default();
/// let start = Instant::now();
/// let retry = Duration::from_secs(30);
/// assert!(gate.due(start, retry));
/// assert_eq!(gate.look(start, false), None);
/// assert!(!gate.due(start + Duration::from_secs(29), retry));
/// assert!(gate.due(start + Duration::from_secs(30), retry));
/// assert_eq!(gate.look(start + Duration::from_secs(30), true), Some(Said::Locked));
/// assert_eq!(gate.look(start + Duration::from_secs(60), true), None);
/// assert_eq!(gate.look(start + Duration::from_secs(90), false), Some(Said::Back));
/// assert_eq!(gate.look(start + Duration::from_secs(120), false), None);
/// ```
#[derive(Debug, Default)]
pub struct Gate {
    locked: bool,
    looked: Option<Instant>,
}

/// The line that a [`Gate`] says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    /// The keyring locked.
    Locked,
    /// The keyring answers again.
    Back,
}

impl Gate {
    /// True when the last look is `retry` or more before `now`, or when
    /// there was no look.
    pub fn due(&self, now: Instant, retry: Duration) -> bool {
        self.looked
            .is_none_or(|at| now.saturating_duration_since(at) >= retry)
    }

    /// True while the keyring is locked.
    pub fn locked(&self) -> bool {
        self.locked
    }

    /// Takes one look at `now`: `locked` is true when the keyring
    /// refused. Returns the line to say, only at a change.
    pub fn look(&mut self, now: Instant, locked: bool) -> Option<Said> {
        self.looked = Some(now);
        let was = std::mem::replace(&mut self.locked, locked);
        match (was, locked) {
            (false, true) => Some(Said::Locked),
            (true, false) => Some(Said::Back),
            _ => None,
        }
    }
}

/// Looks at the keyring with a read of the secret `name`. True when the
/// keyring is locked or does not answer. A secret that is not there,
/// or another error, is an answer.
pub fn refuses(name: &str) -> bool {
    get(name).is_err_and(|e| is_locked(&e))
}

/// The error of a keyring call in a session (01M4CVXJ7ZDAVRKJ8Y59R3KPDV).
fn in_session() -> Result<()> {
    if crate::grant::in_session() {
        bail!(crate::grant::NO_KEYRING);
    }
    Ok(())
}

/// Returns the secret with this name, or `None` if there is none.
pub fn get(name: &str) -> Result<Option<String>> {
    in_session()?;
    if let Some(dir) = files() {
        return file_get(&dir, name).map_err(file_locked);
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
    in_session()?;
    if let Some(dir) = files() {
        return file_set(&dir, name, value).map_err(file_locked);
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
    in_session()?;
    if let Some(dir) = files() {
        return file_delete(&dir, name).map_err(file_locked);
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
    if crate::grant::in_session() {
        return false;
    }
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
        .context(Locked::here())
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
    // A locked Secret Service, or a dismissed prompt, gives no access.
    let locked = matches!(e, Error::NoStorageAccess(_));
    let error = anyhow!("{e}").context(format!(
        "riff cannot {what} the secret \"{name}\" in the OS keyring"
    ));
    if locked {
        error.context(Locked::here())
    } else {
        error
    }
}

/// A secret file that riff may not read or write stands for a locked
/// keyring, so that `just dev` and the tests can lock it.
fn file_locked(error: anyhow::Error) -> anyhow::Error {
    let denied = error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied);
    if denied {
        error.context(Locked::here())
    } else {
        error
    }
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

    /// A store that refuses gives an error that names the host and the
    /// unlock. A gate says one line for the lock, and no new line for
    /// the next looks in 30 seconds (01M4385CCATXC0B8HV1XD6EFWG,
    /// 01M4385CEWGCP31DP5PAMPXZ97).
    #[test]
    fn a_store_that_refuses_names_the_host_and_says_one_line() {
        mock_store();
        set("unit locked", "x").unwrap();
        let entry = Entry::new(SERVICE, "unit locked").unwrap();
        let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
        let refuse = || {
            cred.set_error(Error::NoStorageAccess(Box::new(std::io::Error::other(
                "SS error: prompt dismissed",
            ))));
        };
        refuse();
        let error = get("unit locked").unwrap_err();
        assert!(is_locked(&error), "{error:#}");
        let text = format!("{error:#}");
        let host = crate::identity::this_host();
        assert!(
            text.starts_with(&format!("the OS keyring of {host} is locked")),
            "{text}"
        );
        assert!(text.contains("unlock it at the desktop"), "{text}");
        assert!(text.contains("gh stops too"), "{text}");
        assert!(text.contains("prompt dismissed"), "{text}");

        let mut gate = Gate::default();
        let start = Instant::now();
        assert_eq!(gate.look(start, true), Some(Said::Locked));
        for second in 1..30 {
            refuse();
            assert!(refuses("unit locked"));
            let now = start + Duration::from_secs(second);
            assert!(!gate.due(now, KEYRING_RETRY));
            assert_eq!(gate.look(now, true), None, "a new line at {second} s");
        }
        assert!(gate.locked());
        // The mock gives its error one time, so the store answers again.
        assert!(!refuses("unit locked"));
        let later = start + Duration::from_secs(29) + KEYRING_RETRY;
        assert!(gate.due(later, KEYRING_RETRY));
        assert_eq!(gate.look(later, false), Some(Said::Back));
    }

    #[test]
    fn another_failure_is_not_locked() {
        mock_store();
        set("unit other", "x").unwrap();
        let entry = Entry::new(SERVICE, "unit other").unwrap();
        let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
        cred.set_error(Error::PlatformFailure(Box::new(std::io::Error::other(
            "broken",
        ))));
        let error = get("unit other").unwrap_err();
        assert!(!is_locked(&error), "{error:#}");
    }

    #[test]
    fn a_secret_file_that_riff_may_not_read_is_locked() {
        let dir = tempfile::tempdir().unwrap();
        let file = |e: std::io::Error| file_locked(file_fail("read", "k", dir.path(), e));
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(is_locked(&file(denied)));
        let other = std::io::Error::from(std::io::ErrorKind::InvalidData);
        assert!(!is_locked(&file(other)));
    }
}
