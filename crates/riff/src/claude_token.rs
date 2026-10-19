//! The long-lived token of `claude setup-token` for the container
//! sessions (01M4Z60B869HWDTZMSYPVYACD8, 01M58B7GZT7WFSBBG1XWJHR4MH,
//! 01M58B7H2HMYT6VTY0B8T2G0KW, 01M58B7H50RCG2VXXV9XX5Y7HM,
//! 01M58B7H7HD0H48A0C79F8M4CR, 01M58XZF928T82YAMF4DSJBKBF).
//!
//! # Design
//!
//! The person runs `claude setup-token` on the host one time. Then the
//! person runs `riff claude-token` on the same host, in a terminal, and
//! pastes the token. riff reads it from stdin with no echo, and keeps it
//! in the OS keyring under [`NAME`] ([`store`], R21). It writes no file
//! for the token, and removes the old file [`FILE`] of riff 3.3.0 when
//! it is there. With no terminal on stdin, `riff claude-token` refuses:
//! an agent session has none ([`text::CLAUDE_TOKEN_NO_TERMINAL`]).
//!
//! The daemon reads the token from the keyring at each container start
//! ([`load`], with `KEYRING_WAIT`). A keyring that is locked, or that
//! does not answer, starts no container: the start fails with
//! [`crate::run::HostFault::KeyringLocked`], so the wave tells the lead
//! one time. The daemon never falls back to the sign-in of `claude` on
//! the host then.
//!
//! The daemon gives each container `CLAUDE_CODE_OAUTH_TOKEN` ([`VAR`])
//! with only the name on the podman command line. The value goes in the
//! environment of podman, and is a secret of the scrub of the session.
//! With a token, no `.credentials.json` goes into the container, and the
//! daemon has no rule of [`crate::run::MIN_MINUTES`]. With no token, the
//! old bind and the old rule hold ([`crate::run::Claude`]).
//!
//! When `claude` refuses the token ([`refusal`]), the daemon writes
//! [`REFUSED`] beside the token. Each later start fails with
//! [`crate::run::HostFault::TokenRefused`], so the wave tells the lead
//! one time and starts no new session. A new `riff claude-token`
//! removes the mark. The mark holds no secret, so it stays a file.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as person on thelio
//!     participant R as riff claude-token
//!     participant D as riff daemon
//!     participant C as container
//!     P->>P: claude setup-token
//!     P->>R: paste the token on stdin
//!     R->>R: OS keyring, name "claude setup-token"
//!     D->>D: read the OS keyring (locked: no start)
//!     D->>C: podman run -e CLAUDE_CODE_OAUTH_TOKEN
//!     opt claude refuses the token
//!         C-->>D: authentication_failed
//!         D->>D: daemon/claude-token-refused
//!         D->>D: no new start, one message to the lead
//!     end
//! ```

use std::io::BufRead;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use riff_client::daemons::Token;
use riff_client::secrets;
use serde_json::Value;

use crate::text;

/// The variable that gives `claude` the token in a container.
///
/// ```
/// assert_eq!(riff::claude_token::VAR, "CLAUDE_CODE_OAUTH_TOKEN");
/// ```
pub const VAR: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// The name of the token in the OS keyring (R21): the account of the
/// entry under [`secrets::SERVICE`].
pub const NAME: &str = "claude setup-token";

/// The file of riff 3.3.0 for the token in the folder of the daemon.
/// `riff claude-token` removes it.
pub const FILE: &str = "claude-token";

/// The mark in the folder of the daemon: `claude` refused the token.
pub const REFUSED: &str = "claude-token-refused";

/// The token in the first line of `input` that is not empty, with no
/// space at its ends. A token with a space in it, or no token, is an
/// error.
///
/// ```
/// use riff::claude_token::read;
///
/// assert_eq!(read(&b"\n  sk-ant-oat01-abc  \n"[..])?, "sk-ant-oat01-abc");
/// assert!(read(&b"\n\n"[..]).is_err());
/// assert!(read(&b"sk-ant a b\n"[..]).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read(input: impl BufRead) -> Result<String> {
    for line in input.lines() {
        let line = line.context("cannot read stdin")?;
        let token = line.trim();
        if token.is_empty() {
            continue;
        }
        if token.contains(char::is_whitespace) {
            bail!("{}", text::CLAUDE_TOKEN_BAD);
        }
        return Ok(token.to_owned());
    }
    bail!("{}", text::CLAUDE_TOKEN_BAD)
}

/// The path of the old token file in the folder `dir` of the daemon.
/// riff 3.3.0 kept the token there. [`store`] removes it.
pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Keeps `token` in the OS keyring under [`NAME`], in the place of the
/// old one, and removes the mark [`REFUSED`] in the folder `dir` of the
/// daemon. It removes the old file [`FILE`] of riff 3.3.0 too, and gives
/// its path when it was there. It writes no file for the token.
///
/// ```standalone_crate
/// # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
/// use riff::claude_token::{load, refuse, refused, store};
///
/// let dir = tempfile::tempdir()?;
/// let old = riff::claude_token::path(dir.path());
/// std::fs::write(&old, "sk-ant-oat01-old\n")?;
/// refuse(dir.path())?;
/// assert_eq!(store(dir.path(), "sk-ant-oat01-abc")?, Some(old.clone()));
/// assert!(!old.exists());
/// assert_eq!(load()?.as_deref(), Some("sk-ant-oat01-abc"));
/// assert!(!refused(dir.path()));
/// assert_eq!(store(dir.path(), "sk-ant-oat01-def")?, None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn store(dir: &Path, token: &str) -> Result<Option<PathBuf>> {
    secrets::set(NAME, token).context("cannot keep the token in the OS keyring")?;
    let removed = |file: &Path| match std::fs::remove_file(file) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_context(|| format!("cannot remove {}", file.display())),
    };
    removed(&dir.join(REFUSED))?;
    let old = path(dir);
    Ok(removed(&old)?.then_some(old))
}

/// The token in the OS keyring, or `None`. It reads the keyring at each
/// call. A keyring that is locked, or that does not answer, is an error
/// that [`secrets::is_locked`] knows. With no keyring on the machine,
/// there is no token.
pub fn load() -> Result<Option<String>> {
    if !secrets::has_keyring() {
        return Ok(None);
    }
    Ok(secrets::get(NAME)?
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty()))
}

/// True when a token is set. It uses the value that this process kept,
/// so it makes no new call to the keyring after the first one. A keyring
/// that does not answer gives false.
pub fn held() -> bool {
    secrets::kept(NAME).is_ok_and(|t| t.is_some_and(|t| !t.trim().is_empty()))
}

/// Writes the mark [`REFUSED`] in the folder `dir` of the daemon.
pub fn refuse(dir: &Path) -> Result<()> {
    let mark = dir.join(REFUSED);
    std::fs::write(&mark, "").with_context(|| format!("cannot write {}", mark.display()))
}

/// True when `claude` refused the token of the folder `dir`.
pub fn refused(dir: &Path) -> bool {
    dir.join(REFUSED).exists()
}

/// The state of the token, with no value (01M58B7HA8CRCW0MJ17QP3R5QG,
/// 01M58XZF928T82YAMF4DSJBKBF). It reads the keyring one time. The mark
/// is in the folder `dir` of the daemon.
///
/// ```standalone_crate
/// # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
/// use riff_client::daemons::Token;
/// use riff::claude_token::{refuse, state, store};
///
/// let dir = tempfile::tempdir()?;
/// assert_eq!(state(dir.path()), Token::NotSet);
/// store(dir.path(), "sk-ant-oat01-x")?;
/// assert_eq!(state(dir.path()), Token::Set);
/// refuse(dir.path())?;
/// assert_eq!(state(dir.path()), Token::Refused);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn state(dir: &Path) -> Token {
    match load() {
        Err(e) if secrets::is_locked(&e) => Token::Locked,
        Err(_) => Token::Unknown,
        Ok(None) => Token::NotSet,
        Ok(Some(_)) if refused(dir) => Token::Refused,
        Ok(Some(_)) => Token::Set,
    }
}

/// The words of `claude` for a sign-in that it refuses.
const REFUSALS: [&str; 5] = [
    "Invalid API key",
    "OAuth token has expired",
    "OAuth token revoked",
    "authentication_error",
    "Please run /login",
];

/// True when the stream-json `event` of `claude` says that it refuses
/// its sign-in: an assistant message with the error
/// `authentication_failed`, or a result that failed with the words of a
/// refusal.
///
/// ```
/// use riff::claude_token::refusal;
/// use serde_json::json;
///
/// assert!(refusal(&json!({"type": "assistant", "error": "authentication_failed"})));
/// assert!(refusal(&json!({"type": "result", "is_error": true,
///     "result": "Invalid API key · Please run /login"})));
/// assert!(!refusal(&json!({"type": "result", "is_error": false,
///     "result": "I read: Invalid API key"})));
/// assert!(!refusal(&json!({"type": "result", "is_error": true, "result": "error_max_turns"})));
/// ```
pub fn refusal(event: &Value) -> bool {
    if event["type"] == "assistant" && event["error"] == "authentication_failed" {
        return true;
    }
    event["type"] == "result"
        && event["is_error"] == true
        && event["result"]
            .as_str()
            .is_some_and(|r| REFUSALS.iter().any(|w| r.contains(w)))
}

/// `riff claude-token`: reads the token from stdin, which must be a
/// terminal, with no echo, and keeps it in the OS keyring. It removes
/// the old file in the folder `dir` of the daemon. It gives the text for
/// the person.
pub fn run(dir: &Path) -> Result<String> {
    use std::io::IsTerminal;
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        bail!("{}", text::CLAUDE_TOKEN_NO_TERMINAL);
    }
    eprint!("{}", text::CLAUDE_TOKEN_ASK);
    let token = {
        let _quiet = Quiet::new(&stdin);
        read(stdin.lock())
    };
    eprintln!();
    let old = store(dir, &token?)?;
    Ok(text::claude_token_kept(old.as_deref()))
}

/// The terminal of stdin with no echo, until the drop.
struct Quiet {
    old: Option<nix::sys::termios::Termios>,
}

impl Quiet {
    fn new(stdin: &std::io::Stdin) -> Self {
        use nix::sys::termios::{LocalFlags, SetArg, tcgetattr, tcsetattr};
        let old = tcgetattr(stdin).ok();
        if let Some(old) = &old {
            let mut quiet = old.clone();
            quiet.local_flags.remove(LocalFlags::ECHO);
            let _ = tcsetattr(stdin, SetArg::TCSANOW, &quiet);
        }
        Self { old }
    }
}

impl Drop for Quiet {
    fn drop(&mut self) {
        if let Some(old) = &self.old {
            let _ = nix::sys::termios::tcsetattr(
                std::io::stdin(),
                nix::sys::termios::SetArg::TCSANOW,
                old,
            );
        }
    }
}

/// The one token of the mock keyring, for the unit tests of this crate:
/// they run as threads of one process, and share the store. The guard
/// holds the lock, with no token in the store.
#[cfg(test)]
pub(crate) fn test_token() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    secrets::mock_store();
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    secrets::delete(NAME).unwrap();
    guard
}

/// The next read of the token fails as a locked keyring does.
#[cfg(test)]
pub(crate) fn test_lock() {
    use keyring_core::{Entry, Error, mock};
    let entry = Entry::new(secrets::SERVICE, NAME).unwrap();
    let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
    cred.set_error(Error::NoStorageAccess(Box::new(std::io::Error::other(
        "locked",
    ))));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 01M58XZF928T82YAMF4DSJBKBF: the token goes to the keyring and to
    /// no file. The old file goes away, and the mark goes with a new token.
    #[test]
    fn the_token_is_in_the_keyring_and_in_no_file() {
        let _token = test_token();
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(state(dir.path()), Token::NotSet);
        std::fs::write(path(dir.path()), "sk-ant-oat01-old\n").unwrap();
        refuse(dir.path()).unwrap();
        let old = store(dir.path(), "sk-ant-oat01-new").unwrap();
        assert_eq!(old, Some(path(dir.path())));
        assert_eq!(load().unwrap().as_deref(), Some("sk-ant-oat01-new"));
        assert!(held());
        assert_eq!(state(dir.path()), Token::Set);
        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert!(files.is_empty(), "{files:?}");
        assert_eq!(store(dir.path(), "sk-ant-oat01-newer").unwrap(), None);
    }

    /// A locked keyring is a locked error and the state `Locked`, never
    /// `NotSet`.
    #[test]
    fn a_locked_keyring_is_locked_and_not_unset() {
        let _token = test_token();
        let dir = tempfile::tempdir().unwrap();
        store(dir.path(), "sk-ant-oat01-x").unwrap();
        test_lock();
        assert!(secrets::is_locked(&load().unwrap_err()));
        test_lock();
        assert_eq!(state(dir.path()), Token::Locked);
        assert_eq!(state(dir.path()), Token::Set);
    }
}
