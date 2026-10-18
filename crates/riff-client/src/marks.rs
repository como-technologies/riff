//! The marks of the sessions that left the riff
//! (01M3MEEFC9ZQVW2KC9FNJ75MTY). A mark is the file `left-ID` in a
//! folder that the platform gives
//! ([`Platform::marks`](crate::platform::Platform::marks)). The client
//! of a session reads it before each request (see "A session that left
//! the riff" in [`crate::api`]).

use std::io;
use std::path::{Path, PathBuf};

/// Writes the mark that the session `session` left the riff
/// (01M3MEEFC9ZQVW2KC9FNJ75MTY) in `dir`, a folder of marks. The mark
/// has no lock: it outlives `riff mcp`, so it holds over a resume,
/// `/clear` and a restart of the machine. The mark has mode 0600 in a
/// folder of mode 0700 ([`crate::files`]).
///
/// ```
/// use std::os::unix::fs::PermissionsExt;
///
/// let state = tempfile::tempdir()?;
/// let marks = state.path().join("riff");
/// let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
/// assert!(!riff_client::marks::left(&marks, "a6cf"));
/// riff_client::marks::leave(&marks, "a6cf")?;
/// assert!(riff_client::marks::left(&marks, "a6cf"));
/// assert_eq!(mode(&marks), 0o700);
/// assert_eq!(mode(&marks.join("left-a6cf")), 0o600);
/// assert!(!riff_client::marks::left(&marks, "b2"));
/// riff_client::marks::join(&marks, "a6cf")?;
/// assert!(!riff_client::marks::left(&marks, "a6cf"));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn leave(dir: &Path, session: &str) -> io::Result<()> {
    crate::files::write(&left_file(dir, session), "")
}

/// Removes the mark of [`leave`]. A session with no mark is in the
/// riff already.
pub fn join(dir: &Path, session: &str) -> io::Result<()> {
    match std::fs::remove_file(left_file(dir, session)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// True when the session `session` left the riff.
pub fn left(dir: &Path, session: &str) -> bool {
    left_file(dir, session).exists()
}

fn left_file(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("left-{}", riff_core::name::sanitize(session)))
}
