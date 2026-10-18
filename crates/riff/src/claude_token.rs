//! The long-lived token of `claude setup-token` for the container
//! sessions (01M4Z60B869HWDTZMSYPVYACD8, 01M58B7GZT7WFSBBG1XWJHR4MH,
//! 01M58B7H2HMYT6VTY0B8T2G0KW, 01M58B7H50RCG2VXXV9XX5Y7HM,
//! 01M58B7H7HD0H48A0C79F8M4CR).
//!
//! # Design
//!
//! The person runs `claude setup-token` on the host one time. Then the
//! person runs `riff claude-token` on the same host, in a terminal, and
//! pastes the token. riff reads it from stdin with no echo, and keeps it
//! in [`FILE`] in the folder of the daemon, mode 0600 ([`store`]). With
//! no terminal on stdin, `riff claude-token` refuses: an agent session
//! has none ([`text::CLAUDE_TOKEN_NO_TERMINAL`]).
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
//! removes the mark.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as person on thelio
//!     participant R as riff claude-token
//!     participant D as riff daemon
//!     participant C as container
//!     P->>P: claude setup-token
//!     P->>R: paste the token on stdin
//!     R->>R: daemon/claude-token, mode 0600
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
use serde_json::Value;

use crate::text;

/// The variable that gives `claude` the token in a container.
///
/// ```
/// assert_eq!(riff::claude_token::VAR, "CLAUDE_CODE_OAUTH_TOKEN");
/// ```
pub const VAR: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// The file of the token in the folder of the daemon.
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

/// The path of the token in the folder `dir` of the daemon.
pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Keeps `token` in [`FILE`] in the folder `dir` of the daemon, mode
/// 0600, in the place of the old one, and removes the mark [`REFUSED`].
/// It gives the path.
///
/// ```
/// use std::os::unix::fs::PermissionsExt;
/// use riff::claude_token::{load, refuse, refused, store};
///
/// let dir = tempfile::tempdir()?;
/// refuse(dir.path())?;
/// let path = store(dir.path(), "sk-ant-oat01-abc")?;
/// assert_eq!(std::fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
/// assert_eq!(load(dir.path()).as_deref(), Some("sk-ant-oat01-abc"));
/// assert!(!refused(dir.path()));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn store(dir: &Path, token: &str) -> Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .with_context(|| format!("cannot make {}", dir.display()))?;
    let path = path(dir);
    let new = dir.join(format!("{FILE}.{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&new)
        .with_context(|| format!("cannot write {}", new.display()))?;
    file.write_all(format!("{token}\n").as_bytes())
        .with_context(|| format!("cannot write {}", new.display()))?;
    std::fs::rename(&new, &path).with_context(|| format!("cannot write {}", path.display()))?;
    match std::fs::remove_file(dir.join(REFUSED)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(e)
                .with_context(|| format!("cannot remove {}", dir.join(REFUSED).display()));
        }
        _ => {}
    }
    Ok(path)
}

/// The token in the folder `dir` of the daemon, or `None`.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// assert_eq!(riff::claude_token::load(dir.path()), None);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn load(dir: &Path) -> Option<String> {
    let file = std::fs::read_to_string(path(dir)).ok()?;
    Some(file.trim().to_owned()).filter(|t| !t.is_empty())
}

/// True when a token is set in the folder `dir` of the daemon.
pub fn set(dir: &Path) -> bool {
    load(dir).is_some()
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

/// The state of the token in the folder `dir` of the daemon, with no
/// value (01M58B7HA8CRCW0MJ17QP3R5QG).
///
/// ```
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
    match (set(dir), refused(dir)) {
        (false, _) => Token::NotSet,
        (true, false) => Token::Set,
        (true, true) => Token::Refused,
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
/// terminal, with no echo, and keeps it in the folder `dir` of the
/// daemon. It gives the text for the person.
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
    let path = store(dir, &token?)?;
    Ok(text::claude_token_kept(&path))
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
