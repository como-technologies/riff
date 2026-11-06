//! The parts of a host in each repository (#1643).
//!
//! # Design
//!
//! A host has two parts in a repository: it gives workers (its daemon
//! runs the wave: the work items and the verifies), and it runs the lead
//! of a person ([`Parts`]). The daemon keeps the parts of each
//! repository in [`FILE`] in its folder, so a join and a leave hold over
//! its restarts and over an update. A repository with no part has no
//! entry. The daemon sends the parts in its facts, so `riff who` shows
//! them for each host ([`riff_client::daemons::Facts::parts`]), and
//! `riff daemon status` shows them too.
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> none
//!     none --> workers: riff join
//!     workers --> none: riff leave
//!     none --> lead: a lead starts
//!     lead --> none: the lead ends
//!     lead --> both: riff join
//!     both --> lead: riff leave
//!     workers --> both: a lead starts
//!     both --> workers: the lead ends
//! ```
//!
//! | Step | Parts |
//! |---|---|
//! | `riff join` | the host gets `workers`, and the daemon serves the repository |
//! | `riff leave` | the host loses `workers`: the daemon starts no new session there |
//! | A lead starts on the host | the host gets `lead` |
//! | The lead ends | the host loses `lead` |
//! | The first start of the daemon on this riff | a repository with a wave in [`crate::wave::WAVES_FILE`] gets `workers`, and a repository with a lead gets `lead` ([`first_start`]) |
//!
//! When a host has no part in a repository any more, its daemon stops
//! serving it, and removes the main clone that it made for it
//! ([`crate::daemon::Daemon::retire`]).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use riff_client::daemons::{Parts, RepoParts};

/// The file of the parts of the host, in the folder of the daemon.
pub const FILE: &str = "parts.json";

/// One change at a time: the daemon answers each ask in a task of its own.
static LOCK: Mutex<()> = Mutex::new(());

/// The parts in [`FILE`] of the folder `dir`, by repository. `None`
/// when the daemon never wrote the file: it is the first start of the
/// daemon on a riff with parts ([`first_start`]).
///
/// ```
/// use riff::daemon::parts::{read, set};
///
/// let dir = tempfile::tempdir()?;
/// assert_eq!(read(dir.path())?, None);
/// set(dir.path(), "o/r", |p| p.workers = true)?;
/// assert!(read(dir.path())?.unwrap()["o/r"].workers);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read(dir: &Path) -> Result<Option<BTreeMap<String, Parts>>> {
    let path = dir.join(FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .with_context(|| format!("cannot read the parts in {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Writes `parts` to [`FILE`] in the folder `dir`, in place of the old
/// file. A repository with no part has no entry.
fn write(dir: &Path, parts: &BTreeMap<String, Parts>) -> Result<()> {
    let kept: BTreeMap<&String, &Parts> = parts.iter().filter(|(_, p)| !p.none()).collect();
    let path = dir.join(FILE);
    let tmp = path.with_extension("json.new");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&kept)?)
        .with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))
}

/// The parts of the host in `repo`: none, when the file does not say or
/// cannot be read.
///
/// ```
/// use riff::daemon::parts::{of, set};
///
/// let dir = tempfile::tempdir()?;
/// assert!(of(dir.path(), "o/r").none());
/// set(dir.path(), "o/r", |p| p.lead = true)?;
/// assert_eq!(of(dir.path(), "o/r").word(), "lead");
/// assert!(of(dir.path(), "o/other").none());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn of(dir: &Path, repo: &str) -> Parts {
    read(dir)
        .ok()
        .flatten()
        .and_then(|all| all.get(repo).copied())
        .unwrap_or_default()
}

/// Changes the parts of `repo` with `change`, and gives the new parts.
/// A repository that ends with no part has no entry.
///
/// ```
/// use riff::daemon::parts::{read, set};
///
/// let dir = tempfile::tempdir()?;
/// set(dir.path(), "o/r", |p| p.workers = true)?;
/// let both = set(dir.path(), "o/r", |p| p.lead = true)?;
/// assert_eq!(both.word(), "workers and lead");
/// set(dir.path(), "o/r", |p| p.workers = false)?;
/// let none = set(dir.path(), "o/r", |p| p.lead = false)?;
/// assert!(none.none());
/// assert!(read(dir.path())?.unwrap().is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn set(dir: &Path, repo: &str, change: impl FnOnce(&mut Parts)) -> Result<Parts> {
    let _one = LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut all = read(dir)?.unwrap_or_default();
    let parts = all.entry(repo.to_owned()).or_default();
    change(parts);
    let now = *parts;
    write(dir, &all)?;
    Ok(now)
}

/// The parts of each repository, in order, for `riff daemon status` and
/// the facts of the host. A file that cannot be read gives none.
///
/// ```
/// use riff::daemon::parts::{set, shown};
///
/// let dir = tempfile::tempdir()?;
/// set(dir.path(), "o/b", |p| p.lead = true)?;
/// set(dir.path(), "o/a", |p| p.workers = true)?;
/// let repos: Vec<String> = shown(dir.path()).into_iter().map(|p| p.repo).collect();
/// assert_eq!(repos, ["o/a", "o/b"]);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn shown(dir: &Path) -> Vec<RepoParts> {
    read(dir)
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, parts)| !parts.none())
        .map(|(repo, parts)| RepoParts { repo, parts })
        .collect()
}

/// The first start of the daemon on a riff with parts (#1643): when the
/// daemon never wrote [`FILE`], each repository in `waves` gets the part
/// workers, and each repository in `leads` gets the part lead. So a
/// host that runs a wave goes on, and a host with a lead keeps it. It
/// writes the file, so it does this one time. It gives true when it
/// wrote the file.
///
/// ```
/// use riff::daemon::parts::{first_start, of};
///
/// let dir = tempfile::tempdir()?;
/// assert!(first_start(dir.path(), ["o/r", "o/s"], ["o/s", "o/t"])?);
/// assert_eq!(of(dir.path(), "o/r").word(), "workers");
/// assert_eq!(of(dir.path(), "o/s").word(), "workers and lead");
/// assert_eq!(of(dir.path(), "o/t").word(), "lead");
/// // The second start changes nothing.
/// assert!(!first_start(dir.path(), ["o/new"], [])?);
/// assert!(of(dir.path(), "o/new").none());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn first_start<'a>(
    dir: &Path,
    waves: impl IntoIterator<Item = &'a str>,
    leads: impl IntoIterator<Item = &'a str>,
) -> Result<bool> {
    let _one = LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if read(dir)?.is_some() {
        return Ok(false);
    }
    let mut all: BTreeMap<String, Parts> = BTreeMap::new();
    for repo in waves {
        all.entry(repo.to_owned()).or_default().workers = true;
    }
    for repo in leads {
        all.entry(repo.to_owned()).or_default().lead = true;
    }
    write(dir, &all)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_cannot_be_read_gives_an_error_and_no_parts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "not json").unwrap();
        assert!(read(dir.path()).is_err());
        assert!(of(dir.path(), "o/r").none());
        assert!(shown(dir.path()).is_empty());
    }

    #[test]
    fn the_file_holds_the_parts_as_plain_json() {
        let dir = tempfile::tempdir().unwrap();
        set(dir.path(), "o/r", |p| p.workers = true).unwrap();
        let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"o/r": {"workers": true, "lead": false}})
        );
    }

    #[test]
    fn a_repository_with_no_part_is_not_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        set(dir.path(), "o/r", |p| p.workers = true).unwrap();
        set(dir.path(), "o/r", |p| p.workers = false).unwrap();
        assert_eq!(read(dir.path()).unwrap(), Some(BTreeMap::new()));
    }
}
