//! A home of its own for riff: `RIFF_HOME` (01M3MY2KSV73WS8D902YCH2PRX).
//!
//! # Design
//!
//! Without `RIFF_HOME`, riff keeps its files where the person has them:
//! the settings in [`settings`](crate::settings), the local files in
//! [`local`](crate::local), and the secrets in the OS keyring (see
//! [`secrets`](crate::secrets)).
//!
//! With `RIFF_HOME=DIR`, riff keeps each of them under `DIR`, and never
//! opens the OS keyring:
//!
//! | What | Where |
//! |---|---|
//! | The settings | `DIR/config.toml` |
//! | The local files | `DIR/state` |
//! | The secrets | one file for each secret in `DIR/secrets` |
//!
//! Only the tests and `just dev` set it, so that they never touch the
//! riff of the machine (01M3MY2KWKBJCQ0BCNC6533RBW). An empty value
//! counts as unset.
//!
//! ```
//! use riff::home::dir_from;
//! use std::path::PathBuf;
//!
//! assert_eq!(dir_from(Some("/h".into())), Some(PathBuf::from("/h")));
//! assert_eq!(dir_from(Some("".into())), None);
//! assert_eq!(dir_from(None), None);
//! ```

use std::ffi::OsString;
use std::path::PathBuf;

/// The variable that names the home of riff.
pub const VAR: &str = "RIFF_HOME";

/// The home of riff in this process, or `None` when `RIFF_HOME` is
/// unset.
pub fn dir() -> Option<PathBuf> {
    dir_from(std::env::var_os(VAR))
}

/// [`dir`] from the value of `RIFF_HOME`.
pub fn dir_from(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty()).map(PathBuf::from)
}
