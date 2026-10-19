//! `riff build` for the lead in a container (#1204,
//! 01M5A5YJ8PKBAWWG6KW2MBK5Z3).
//!
//! # Design
//!
//! A container has no podman and no writable `~/.cache`, so `just
//! static` fails there. The lead asks the daemon of a host with podman:
//! `riff build --host HOST` posts
//! [`Request::Build`](super::remote::Request::Build). The daemon judges
//! it like each other request ([`super::remote::judge`]): never a work
//! session. Then it does what `riff build` does on a host
//! ([`crate::check::build`]) in the check clone of the host, and replies
//! with the path of the new riff on that host.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant L as lead in a container
//!     participant D as riff daemon on HOST
//!     L->>D: build
//!     D->>D: fetch origin, detach at origin/HEAD
//!     D->>D: just static
//!     D-->>L: path of the new riff on HOST
//! ```
//!
//! [`JUST_VAR`] names another program in place of `just`: a test sets it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The program that the daemon runs in place of `just`.
pub const JUST_VAR: &str = "RIFF_BUILD_JUST";

/// Builds the default branch of the `origin` of `main` in the check
/// clone of this host ([`crate::check::VAR`] or `~/.cache/riff/check`).
/// It gives the text of the reply: the path of the new riff.
pub fn run(main: &Path) -> Result<String> {
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .context("the daemon cannot find its home: HOME is not set")?;
    let check = crate::check::dir_from(std::env::var_os(crate::check::VAR), Path::new(&home));
    let just = std::env::var_os(JUST_VAR).map_or_else(|| PathBuf::from("just"), PathBuf::from);
    let riff = crate::check::build(main, &check, &just)?;
    Ok(riff.display().to_string())
}
