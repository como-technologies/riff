//! The release cycle of riff is only for a repository with the table
//! `[release]` in its riff config (01M5SMPX8NM75P8E38DGQ17W7B).
//!
//! # Design
//!
//! The release cycle is the rules of riff for its own releases: the
//! release pull request, the real-session test, the deploy to Cloud Run
//! and the checks after it. Another repository has its own release, or
//! none. So a repository opts in. Its riff config, the file
//! [`FILE`] at its root, has the table
//! [`TABLE`]:
//!
//! ```toml
//! [release]
//! ```
//!
//! The table holds no key now. Its presence is the fact. The probe for
//! `deploy/release-check.sh` decides nothing.
//!
//! | Where | What it reads | With no `[release]` |
//! |---|---|---|
//! | `riff release SUBCOMMAND`, `riff build`, `riff release test`, the request `build` and the request `release test` of the daemon | the working tree: [`require`] | refuses with [`NO_RELEASE`](crate::text::NO_RELEASE) before it touches the forge |
//! | `riff wave end N`, the hint of the end of a wave | the working tree: [`of_folder`] | no release gate; the hint is `riff wave end N` |
//! | the verify start text of the wave | the merged file: [`merged`] | no part for a release pull request |
//!
//! ```mermaid
//! flowchart TD
//!     C["riff release …, riff build,<br/>the real-session test"] --> R{"riff.toml has<br/>[release]?"}
//!     R -- "no" --> X["one line: this repository has no release<br/>cycle of riff. Nothing on the forge."]
//!     R -- "yes" --> G["the release cycle"]
//!     W["riff wave end N"] --> Q{"riff.toml has<br/>[release]?"}
//!     Q -- "no" --> E["ends with no release gate"]
//!     Q -- "yes" --> L["the release gate"]
//! ```
//!
//! A person runs a command in a clone, so the commands read the working
//! tree, as `riff image` does. The wave of the daemon reads the merged
//! file, as it does for the session image
//! ([`session_image::merged`](crate::session_image::merged)).
//!
//! ```
//! use riff::release_config::in_config;
//!
//! assert!(in_config("[session]\nimage = \"x\"\n\n[release]\n")?);
//! assert!(!in_config("[session]\nimage = \"x\"\n")?);
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::session_image::{FILE, git};
use crate::text;

/// The table of the riff config that opts a repository in to the release
/// cycle.
pub const TABLE: &str = "release";

/// True when the riff config `text` has the table [`TABLE`]. A key with
/// that name and a value that is not a table is no opt-in.
///
/// ```
/// use riff::release_config::in_config;
///
/// assert!(in_config("[release]")?);
/// assert!(in_config("release = {}")?);
/// assert!(!in_config("release = true")?);
/// assert!(!in_config("")?);
/// assert!(in_config("[release").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn in_config(text: &str) -> Result<bool> {
    let config: toml_edit::DocumentMut = text
        .parse()
        .with_context(|| format!("{FILE} is not valid TOML"))?;
    Ok(config
        .get(TABLE)
        .is_some_and(toml_edit::Item::is_table_like))
}

/// True when the riff config at the root `root` of a repository has the
/// table [`TABLE`]. A repository with no config has none.
///
/// ```
/// use riff::release_config::of_repository;
/// use riff::session_image::FILE;
///
/// let root = tempfile::tempdir()?;
/// assert!(!of_repository(root.path())?);
/// std::fs::write(root.path().join(FILE), "[session]\nimage = \"x\"\n")?;
/// assert!(!of_repository(root.path())?);
/// std::fs::write(root.path().join(FILE), "[release]\n")?;
/// assert!(of_repository(root.path())?);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn of_repository(root: &Path) -> Result<bool> {
    let path = root.join(FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => in_config(&text).with_context(|| path.display().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_context(|| format!("cannot read the riff config {}", path.display())),
    }
}

/// True when the repository of the folder `dir` has the table [`TABLE`]:
/// the config at the top of its git work tree, else in `dir`.
///
/// ```
/// use riff::release_config::of_folder;
/// use riff::session_image::FILE;
///
/// let root = tempfile::tempdir()?;
/// let status = std::process::Command::new("git").args(["init", "-q"]).arg(root.path()).status()?;
/// assert!(status.success());
/// std::fs::write(root.path().join(FILE), "[release]\n")?;
/// std::fs::create_dir(root.path().join("src"))?;
/// assert!(of_folder(&root.path().join("src"))?);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn of_folder(dir: &Path) -> Result<bool> {
    of_repository(&crate::cloud::top(dir).unwrap_or_else(|| dir.to_owned()))
}

/// Refuses with [`NO_RELEASE`](text::NO_RELEASE) when the repository of
/// the folder `dir` has no table [`TABLE`]. It reads one file: it touches
/// nothing on the forge.
///
/// ```
/// use riff::release_config::require;
/// use riff::session_image::FILE;
///
/// let root = tempfile::tempdir()?;
/// let error = require(root.path()).unwrap_err().to_string();
/// assert_eq!(error, "this repository has no release cycle of riff: riff.toml has no [release]");
/// std::fs::write(root.path().join(FILE), "[release]\n")?;
/// require(root.path())?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn require(dir: &Path) -> Result<()> {
    if !of_folder(dir)? {
        bail!("{}", text::NO_RELEASE);
    }
    Ok(())
}

/// True when [`FILE`] on the default branch of `origin` in the main clone
/// `main`, the branch of `origin/HEAD`, has the table [`TABLE`]. A clone
/// with no such file or no good file has no opt-in: the wave texts then
/// have no release part.
///
/// ```
/// use std::process::Command;
/// use riff::release_config::merged;
/// use riff::session_image::FILE;
///
/// let git = |dir: &std::path::Path, args: &[&str]| {
///     let status = Command::new("git")
///         .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
///         .arg("-C")
///         .arg(dir)
///         .args(args)
///         .env("GIT_CONFIG_GLOBAL", "/dev/null")
///         .status()
///         .unwrap();
///     assert!(status.success(), "git {args:?}");
/// };
/// let dir = tempfile::tempdir()?;
/// let (origin, main) = (dir.path().join("origin.git"), dir.path().join("main"));
/// git(dir.path(), &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
/// git(dir.path(), &["init", "-q", "-b", "main", main.to_str().unwrap()]);
/// git(&main, &["remote", "add", "origin", origin.to_str().unwrap()]);
/// std::fs::write(main.join(FILE), "[session]\nimage = \"x\"\n")?;
/// git(&main, &["add", FILE]);
/// git(&main, &["commit", "-q", "-m", "pin"]);
/// git(&main, &["push", "-q", "origin", "main"]);
/// git(&main, &["remote", "set-head", "origin", "main"]);
/// assert!(!merged(&main));
///
/// // An edit that is not merged opts in nothing.
/// std::fs::write(main.join(FILE), "[release]\n")?;
/// assert!(!merged(&main));
///
/// git(&main, &["commit", "-q", "-a", "-m", "release"]);
/// git(&main, &["push", "-q", "origin", "main"]);
/// assert!(merged(&main));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn merged(main: &Path) -> bool {
    let from_origin = || -> Result<bool> {
        let head = git(
            main,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        )?;
        in_config(&git(main, &["show", &format!("{head}:{FILE}")])?)
    };
    from_origin().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_with_a_release_table_opts_in_and_a_config_without_it_does_not() {
        assert!(in_config("[session]\nimage = \"x\"\n[release]\n").unwrap());
        assert!(!in_config("[session]\nimage = \"x\"\n").unwrap());
        // A key of another table is no opt-in.
        assert!(!in_config("[session]\nrelease = \"x\"\n").unwrap());
    }

    #[test]
    fn a_folder_outside_a_repository_with_no_config_is_refused() {
        let outside = isolated::outside_git();
        let error = require(outside.path()).unwrap_err().to_string();
        assert_eq!(error, text::NO_RELEASE);
    }

    #[test]
    fn a_config_that_is_not_toml_is_an_error_that_names_the_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(FILE), "[release").unwrap();
        let error = format!("{:#}", require(root.path()).unwrap_err());
        assert!(error.contains("riff.toml is not valid TOML"), "{error}");
        assert!(
            error.contains(&root.path().display().to_string()),
            "{error}"
        );
    }
}
