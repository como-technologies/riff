//! The platform of `riff`: the parts that `riff` gives to
//! [`riff_client`] (01M54216S51X5VBXN3YAERW6J6). `main` gives them
//! with [`install`] at its start.
//!
//! | Part | What `riff` gives |
//! |---|---|
//! | The store of the tokens and of the device key | The OS keyring: on Linux and BSD the store of riff, [`keyring_store`](crate::keyring_store), with plain sessions of the Secret Service (01M556QPPRSQ91HDED128G2X7G); the store of the `keyring` crate on each other OS |
//! | The secret files | `$RIFF_HOME/secrets` (see [`home`](crate::home)) |
//! | The local folder | [`local::dir`] |
//! | The folder of the leave marks | [`local::marks`] |
//! | The name of the machine | [`identity::this_host`] |
//! | The riff of the process | [`binary::this_riff`] |
//! | A wait line | Dim, on stderr |
//! | A reply of another build | [`auto_update::begin`]; one note on stderr, none in the update in the background ([`auto_update::in_background`]) |
//!
//! ```standalone_crate
//! riff::platform::install();
//! let a = riff_client::login::lock_path("http://a")?;
//! assert_ne!(a, riff_client::login::lock_path("http://b")?);
//! assert_eq!(a.parent(), riff::local::dir().as_deref());
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::path::PathBuf;

use riff_client::platform::Platform;
use riff_core::build::Build;
use riff_core::wire::RiffBinary;

use crate::style::{DIM, styled};
use crate::{auto_update, binary, identity, local};

/// The platform of `riff`.
struct Terminal;

impl Platform for Terminal {
    fn open_store(&self) -> keyring_core::Result<()> {
        open_store()
    }

    fn secret_files(&self) -> Option<PathBuf> {
        crate::home::dir().map(|home| home.join("secrets"))
    }

    fn local_dir(&self) -> Option<PathBuf> {
        local::dir()
    }

    fn marks(&self) -> Option<PathBuf> {
        local::marks()
    }

    fn host(&self) -> String {
        identity::this_host()
    }

    fn binary(&self) -> RiffBinary {
        binary::this_riff()
    }

    fn dim(&self, text: &str) -> String {
        styled(DIM, text)
    }

    fn show_wait(&self, line: &str) {
        anstream::eprintln!("{line}");
    }

    fn server_build(&self, base: &str, server: &Build) {
        auto_update::begin(base, server);
    }

    fn other_build(&self, note: &str) {
        if !auto_update::in_background() {
            eprintln!("riff: {note}");
        }
    }
}

/// Gives the platform of `riff` to [`riff_client`], one time for the
/// process.
pub fn install() {
    riff_client::platform::set(Box::new(Terminal));
}

/// Sets the store of riff with plain sessions of the Secret Service
/// (01M556QPPRSQ91HDED128G2X7G).
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
fn open_store() -> keyring_core::Result<()> {
    let store = crate::keyring_store::Store::new()?;
    keyring_core::set_default_store(store);
    Ok(())
}

/// Sets the store of the `keyring` crate: the Keychain of macOS, or the
/// Credential Manager of Windows.
#[cfg(not(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
)))]
fn open_store() -> keyring_core::Result<()> {
    keyring::Entry::store_status()
        .as_ref()
        .map(|_| ())
        .map_err(|e| keyring_core::Error::PlatformFailure(e.to_string().into()))
}
