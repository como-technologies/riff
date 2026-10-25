//! The rules of the hygiene check that `riff pr open` applies in a
//! repository (01M5SMSHNQ9E4P9W6RCB8E1WTN, 01M5SMSMWE89SC3Q7XFBQCFY32).
//!
//! # Design
//!
//! riff reads two things from a pull request: the link line `Closes #N`
//! or `Refs #N`, and the trailer `Issue: #N`. The other rules of the
//! hygiene check are the habits of riff itself: a milestone on each
//! issue and pull request, the trailer `Milestone:`, no closing keyword
//! in the text, no `(#N)` at the end of a title. A repository that is not
//! riff gets only what riff reads. It asks for the rest in its riff
//! config ([`crate::session_image::FILE`]), with the table `[hygiene]`:
//!
//! ```toml
//! [hygiene]
//! rules = "riff"
//! ```
//!
//! ```mermaid
//! flowchart TD
//!     F["riff.toml at the top of the work tree"] --> T{"[hygiene] rules = riff?"}
//!     T -- "no table, or no file" --> R["Rules::Read: link, Issue:"]
//!     T -- yes --> A["Rules::All: each rule of the hygiene check"]
//!     R --> O["riff pr open"]
//!     A --> O
//!     J["the job Hygiene of the CI of riff"] --> A
//! ```
//!
//! The CI of riff does not read the config: its job `Hygiene` runs each
//! rule on each pull request of riff. The `riff.toml` of riff has the
//! table, so `riff pr open` in riff applies the same rules.
//!
//! The file is the one in the working tree, so a pull request that adds
//! the table is already under the rules. No file is the same as no table.
//!
//! ```
//! use hygiene::Rules;
//! use riff::hygiene_rules::of_config;
//!
//! assert_eq!(of_config("", "riff.toml")?, Rules::Read);
//! assert_eq!(of_config("[session]\nimage = \"x\"\n", "riff.toml")?, Rules::Read);
//! assert_eq!(of_config("[hygiene]\nrules = \"riff\"\n", "riff.toml")?, Rules::All);
//!
//! let error = of_config("[hygiene]\nrules = \"strict\"\n", "riff.toml").unwrap_err();
//! assert!(error.to_string().contains("[hygiene] rules is \"strict\""), "{error}");
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::path::Path;

use anyhow::{Context, Result, bail};
use hygiene::Rules;

use crate::session_image::{FILE, top};

/// The only value of `rules`.
const RIFF: &str = "riff";

/// The rules that the riff config `text` asks for. `place` names the
/// config in an error. With no `rules` in the table `[hygiene]`, they
/// are the rules that riff reads ([`Rules::Read`]).
pub fn of_config(text: &str, place: &str) -> Result<Rules> {
    let config: toml_edit::DocumentMut = text
        .parse()
        .with_context(|| format!("{place} is not valid TOML"))?;
    let Some(rules) = config.get("hygiene").and_then(|t| t.get("rules")) else {
        return Ok(Rules::Read);
    };
    match rules.as_str() {
        Some(RIFF) => Ok(Rules::All),
        _ => bail!(
            "{place}: [hygiene] rules is {}. The one value is \"{RIFF}\". Give it, or remove the key.",
            rules.to_string().trim()
        ),
    }
}

/// The rules of the repository of the folder `dir`: what the file
/// [`FILE`] at the top of its git work tree asks for. A repository with
/// no such file gets [`Rules::Read`].
///
/// ```
/// use hygiene::Rules;
/// use riff::hygiene_rules::of_folder;
///
/// let root = tempfile::tempdir()?;
/// let git = std::process::Command::new("git").args(["init", "-q"]).arg(root.path()).status()?;
/// assert!(git.success());
/// assert_eq!(of_folder(root.path())?, Rules::Read);
///
/// std::fs::write(root.path().join("riff.toml"), "[hygiene]\nrules = \"riff\"\n")?;
/// std::fs::create_dir(root.path().join("src"))?;
/// assert_eq!(of_folder(&root.path().join("src"))?, Rules::All);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn of_folder(dir: &Path) -> Result<Rules> {
    let path = top(dir)?.join(FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => of_config(&text, &path.display().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Rules::Read),
        Err(e) => Err(e).with_context(|| format!("cannot read the riff config {}", path.display())),
    }
}
