//! The platform parts: what each UI gives to `riff-client`
//! (01M54216S51X5VBXN3YAERW6J6).
//!
//! # Design
//!
//! A part that is not the same on each platform is a method of
//! [`Platform`]. A UI gives its platform one time, at its start, with
//! [`set`]. Before that, and in a process that gives none, the client
//! uses the defaults of the trait: no keyring of the OS, no local
//! folder, and plain lines on stderr (01M56S4M0BSV0KP825XSDNNXNX). A
//! test of the client needs no platform.
//!
//! | Part | Method | What `riff` gives |
//! |---|---|---|
//! | The store of the tokens and of the device key | [`Platform::open_store`] | The OS keyring: the Secret Service on Linux, the Keychain on macOS |
//! | The secret files of a test or of `just dev` | [`Platform::secret_files`] | `$RIFF_HOME/secrets` |
//! | The folder of the local files: the refresh lock of the sign-in | [`Platform::local_dir`] | The local folder of riff |
//! | The folder of the leave marks of the sessions | [`Platform::marks`] | The state folder of riff |
//! | The name of this machine, in the error of a locked keyring | [`Platform::host`] | The host of the session URI |
//! | The riff of this process, in a keep-alive | [`Platform::binary`] | Its build, and if its file was replaced |
//! | The style of a wait line, and where it goes | [`Platform::dim`], [`Platform::show_wait`] | Dim text on stderr |
//! | A reply of another build | [`Platform::server_build`], [`Platform::other_build`] | The update in the background, and one note on stderr |
//!
//! The store is a store of `keyring-core`: [`crate::secrets`] keeps
//! each secret in the default store of `keyring-core`. When a process
//! set no default store, the first call to a secret calls
//! [`Platform::open_store`] one time. A test sets the mock store of
//! `keyring-core` first.
//!
//! ```mermaid
//! flowchart LR
//!     UI["a UI: riff, a phone app"] -- "set, at the start" --> P[the platform]
//!     A[api, login, device, secrets, link] -- "get" --> P
//!     P -. "none given" .-> D[the defaults of the trait]
//! ```
//!
//! # Example
//!
//! ```standalone_crate
//! use riff_client::platform::{self, Platform};
//!
//! struct Phone;
//!
//! impl Platform for Phone {
//!     fn host(&self) -> String {
//!         "pixel".into()
//!     }
//! }
//!
//! assert_eq!(platform::get().host(), gethostname::gethostname().to_string_lossy());
//! platform::set(Box::new(Phone));
//! assert_eq!(platform::get().host(), "pixel");
//! ```

use std::path::PathBuf;
use std::sync::OnceLock;

use riff_core::build::Build;
use riff_core::wire::RiffBinary;

/// The parts of `riff-client` that each UI gives. Each method has a
/// default for a process that gives no platform.
pub trait Platform: Send + Sync {
    /// Sets the store of the OS as the default store of `keyring-core`.
    /// [`crate::secrets`] calls it one time, at its first call, when the
    /// process set no store. The default has no store of the OS.
    fn open_store(&self) -> keyring_core::Result<()> {
        Err(keyring_core::Error::NoDefaultStore)
    }

    /// The folder of the secret files, in place of the store: only for
    /// the tests and `just dev`. The default is `None`.
    fn secret_files(&self) -> Option<PathBuf> {
        None
    }

    /// The local folder of the client: the refresh lock of the sign-in
    /// is in it ([`crate::login::lock_path`]). `None` when there is
    /// none, the default.
    fn local_dir(&self) -> Option<PathBuf> {
        None
    }

    /// The folder of the leave marks of the sessions
    /// ([`crate::marks`]). `None` when there is none, the default.
    fn marks(&self) -> Option<PathBuf> {
        None
    }

    /// The name of this machine. The default is the name of the OS.
    fn host(&self) -> String {
        gethostname::gethostname().to_string_lossy().into_owned()
    }

    /// The riff of this process, for a keep-alive. The default is this
    /// build, with a file that was not replaced.
    fn binary(&self) -> RiffBinary {
        RiffBinary {
            build: riff_core::build::VERSION.into(),
            replaced: false,
        }
    }

    /// `text` with the style of a quiet line. The default is the text.
    fn dim(&self, text: &str) -> String {
        text.to_owned()
    }

    /// Shows the wait line `line` of a client that has no place of its
    /// own for it. The default prints it on stderr.
    fn show_wait(&self, line: &str) {
        eprintln!("{line}");
    }

    /// A reply of the server at `base` named its build `server`. The
    /// default does nothing.
    fn server_build(&self, base: &str, server: &Build) {
        let _ = (base, server);
    }

    /// Shows `note`: the server runs another build that the client can
    /// talk to. The default prints it on stderr.
    fn other_build(&self, note: &str) {
        eprintln!("riff: {note}");
    }
}

/// The platform of a process that gives none.
struct Defaults;

impl Platform for Defaults {}

static PLATFORM: OnceLock<Box<dyn Platform>> = OnceLock::new();

/// Gives the platform of this process. Only the first call counts: a
/// later call changes nothing.
pub fn set(platform: Box<dyn Platform>) {
    let _ = PLATFORM.set(platform);
}

/// The platform of this process: the one of [`set`], else the defaults.
pub fn get() -> &'static dyn Platform {
    PLATFORM
        .get()
        .map_or(&Defaults, |platform| platform.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_have_no_store_and_no_folder() {
        let defaults = Defaults;
        assert!(defaults.open_store().is_err());
        assert_eq!(defaults.secret_files(), None);
        assert_eq!(defaults.local_dir(), None);
        assert_eq!(defaults.marks(), None);
        assert_eq!(defaults.dim("waiting"), "waiting");
        assert_eq!(defaults.binary().build, riff_core::build::VERSION);
        assert!(!defaults.binary().replaced);
    }
}
