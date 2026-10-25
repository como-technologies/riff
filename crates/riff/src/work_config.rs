//! What riff needs from a repository to start a session: the table
//! `[work]` of the riff config `riff.toml`
//! (01M5RH1SE99SNN9R66ZTS6VDSX).
//!
//! # Design
//!
//! `riff.toml` has the table `[session]` for the image
//! ([`crate::session_image`]) and the table `[work]` for the work of a
//! session:
//!
//! ```toml
//! [work]
//! check = "just check"
//! rules = ["Each change has a test.", "Name no secret in a log."]
//! ```
//!
//! - `check` is the command that a session runs before each push and
//!   after a rebase.
//! - `rules` is an optional list of lines. riff gives them to each
//!   session of the repository in its start text.
//!
//! riff reads the table from `riff.toml` on the default branch of
//! `origin` ([`Work::merged`]), as it reads the image. The start texts of
//! a build, a rebase, a fix of checks and a verify name the `check` and
//! give the `rules` (01M5RH1SH11PTWKCJMNEMV8N2H). A `CLAUDE.md` or an
//! `AGENTS.md` is the choice of the repository: the texts send a
//! session to it only as a hint, and `riff init` checks no such file
//! (01M5RH1SPRX0ZY5Q3WA4ENP84M). riff guesses no `check`: it reads no
//! justfile and no file of a build tool. `riff init` writes the command
//! that the person gives (01M5RKRKN36FRPWZ53F6CCP4DQ).
//!
//! ```mermaid
//! flowchart LR
//!     T["riff.toml on origin/main<br/>[work] check, rules"] --> W["Work"]
//!     W --> S["the start texts of<br/>a build, a rebase,<br/>a fix of checks, a verify"]
//!     I["riff init"] -- "no [work] check:<br/>the command that the person gives,<br/>or no check" --> T
//! ```
//!
//! ```
//! use riff::work_config::Work;
//!
//! let work = Work::of_config(
//!     "[work]\ncheck = \"cargo test\"\nrules = [\"Each change has a test.\"]\n",
//!     "riff.toml",
//! )?;
//! assert_eq!(work.check.as_deref(), Some("cargo test"));
//! assert_eq!(work.rules, ["Each change has a test."]);
//!
//! // A riff.toml with no [work] table is good: the repository sets nothing.
//! let none = Work::of_config("[session]\nimage = \"x\"\n", "riff.toml")?;
//! assert_eq!(none, Work::default());
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::{session_image, text};

/// The work table of a riff config: what riff gives a session of the
/// repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Work {
    /// The command that a session runs before each push and after a
    /// rebase, for example `just check`.
    pub check: Option<String>,
    /// The lines for the start text of each session of the repository.
    pub rules: Vec<String>,
}

impl Work {
    /// The work table of the riff config `text`. `place` names the config
    /// in an error. A config with no `[work]` table gives [`Work::default`].
    ///
    /// ```
    /// use riff::work_config::Work;
    ///
    /// let work = Work::of_config("[work]\ncheck = \"just check\"\n", "riff.toml")?;
    /// assert_eq!(work.check.as_deref(), Some("just check"));
    /// assert!(work.rules.is_empty());
    ///
    /// let error = |text: &str| format!("{:#}", Work::of_config(text, "riff.toml").unwrap_err());
    /// assert_eq!(error("[work]\ncheck = 3\n"), "riff.toml: [work] check is not a text");
    /// assert_eq!(error("[work]\ncheck = \" \"\n"), "riff.toml: [work] check is empty");
    /// assert_eq!(error("[work]\nrules = \"x\"\n"), "riff.toml: [work] rules is not a list of texts");
    /// assert!(error("[work\n").starts_with("riff.toml is not valid TOML"));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn of_config(text: &str, place: &str) -> Result<Work> {
        let config: toml_edit::DocumentMut = text
            .parse()
            .with_context(|| format!("{place} is not valid TOML"))?;
        let Some(work) = config.get("work") else {
            return Ok(Work::default());
        };
        let check = match work.get("check") {
            None => None,
            Some(item) => {
                let Some(check) = item.as_str() else {
                    bail!("{place}: [work] check is not a text");
                };
                let check = text::line(check);
                if check.is_empty() {
                    bail!("{place}: [work] check is empty");
                }
                Some(check)
            }
        };
        let rules = match work.get("rules") {
            None => vec![],
            Some(item) => {
                let list: Option<Vec<String>> = item.as_array().and_then(|a| {
                    a.iter()
                        .map(|v| v.as_str().map(text::line))
                        .collect::<Option<_>>()
                });
                let Some(list) = list else {
                    bail!("{place}: [work] rules is not a list of texts");
                };
                list.into_iter().filter(|r| !r.is_empty()).collect()
            }
        };
        Ok(Work { check, rules })
    }

    /// The work table of the merged `riff.toml` of the main clone `main`:
    /// the file on the default branch of `origin`, as the clone has it
    /// after the fetch of [`session_image::merged`]. The edits of the
    /// working tree change nothing.
    pub fn merged(main: &Path) -> Result<Work> {
        let (head, config) = session_image::merged_config(main)?;
        Work::of_config(&config, &format!("{} of {head}", session_image::FILE))
    }
}

/// The text of `config` with `check` in its `[work]` table: it adds the
/// table when the config has none, and keeps the rest of the text, the
/// comments too (01M5RH1SKTT7JASP1VCCSTEE6F). A `[work]` table with a
/// `check` stays as it is.
///
/// ```
/// use riff::work_config::with_check;
///
/// let config = "# The riff config.\n[session]\nimage = \"x\"\n";
/// assert_eq!(
///     with_check(config, "just check")?,
///     "# The riff config.\n[session]\nimage = \"x\"\n\n[work]\ncheck = \"just check\"\n"
/// );
/// // A table with a check stays.
/// let set = "[work]\ncheck = \"make\"\n";
/// assert_eq!(with_check(set, "just check")?, set);
/// // A table with rules only gets the check.
/// let rules = with_check("[work]\nrules = [\"a\"]\n", "cargo test")?;
/// assert_eq!(rules, "[work]\nrules = [\"a\"]\ncheck = \"cargo test\"\n");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn with_check(config: &str, check: &str) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = config.parse().context("riff.toml is not valid TOML")?;
    if doc.get("work").is_none() {
        let mut table = toml_edit::Table::new();
        table.insert("check", toml_edit::value(check));
        doc.insert("work", toml_edit::Item::Table(table));
        let mut text = doc.to_string();
        // One blank line before the new table.
        if let Some(at) = text.find("\n[work]\n")
            && !text[..at].ends_with('\n')
        {
            text.insert(at, '\n');
        }
        return Ok(text);
    }
    if doc["work"].get("check").is_none() {
        doc["work"]["check"] = toml_edit::value(check);
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 01M5RH1SE99SNN9R66ZTS6VDSX: the table `[work]` of the riff config
    /// gives `check` and `rules`; the table `[session]` changes nothing.
    #[test]
    fn the_riff_config_gives_the_check_and_the_rules() {
        let text = "[session]\nimage = \"x\"\n\n[work]\ncheck = \"just check\"\n\
                    rules = [\"Each change has a test.\", \"  Use  short   words. \"]\n";
        let work = Work::of_config(text, "riff.toml").unwrap();
        assert_eq!(work.check.as_deref(), Some("just check"));
        assert_eq!(work.rules, ["Each change has a test.", "Use short words."]);
    }

    /// A riff config with no `[work]` table, or with `check` only, is good.
    #[test]
    fn a_riff_config_may_have_no_work_table_and_no_rules() {
        assert_eq!(Work::of_config("", "riff.toml").unwrap(), Work::default());
        let only = Work::of_config("[work]\ncheck = \"make\"\n", "riff.toml").unwrap();
        assert_eq!(only.check.as_deref(), Some("make"));
        assert!(only.rules.is_empty());
        let rules = Work::of_config("[work]\nrules = [\"a\"]\n", "riff.toml").unwrap();
        assert_eq!(rules.check, None);
        assert_eq!(rules.rules, ["a"]);
    }

    /// A control character in a check or a rule never reaches a start
    /// text, and a rule is one line.
    #[test]
    fn a_check_and_a_rule_are_one_line_of_plain_text() {
        let text = "[work]\ncheck = \"make\\n\\u001b[2Jcheck\"\nrules = [\"a\\nb\\u0007\"]\n";
        let work = Work::of_config(text, "riff.toml").unwrap();
        assert_eq!(work.check.as_deref(), Some("make check"));
        assert_eq!(work.rules, ["a b"]);
    }

    /// The merged config of a clone gives the table; the working tree of
    /// the main clone does not.
    #[test]
    fn the_merged_table_comes_from_origin_and_not_from_the_working_tree() {
        let (_dir, main, _origin) =
            session_image::test_clone("[session]\nimage = \"x\"\n[work]\ncheck = \"just check\"\n");
        let merged = Work::merged(&main).unwrap();
        assert_eq!(merged.check.as_deref(), Some("just check"));
        std::fs::write(
            main.join(session_image::FILE),
            "[work]\ncheck = \"make unmerged\"\n",
        )
        .unwrap();
        assert_eq!(Work::merged(&main).unwrap(), merged);
    }

    /// 01M5RH1SE99SNN9R66ZTS6VDSX: the `riff.toml` of riff has the check
    /// `just check`, and it still pins the session image.
    #[test]
    fn the_riff_toml_of_riff_names_just_check() {
        let text = include_str!("../../../riff.toml");
        let work = Work::of_config(text, "riff.toml").unwrap();
        assert_eq!(work.check.as_deref(), Some("just check"));
        assert!(
            work.rules.is_empty(),
            "the rules of riff are in its CLAUDE.md"
        );
        assert!(session_image::of_release().digest().starts_with("sha256:"));
    }
}
