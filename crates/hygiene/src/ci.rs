//! The checks that `just ci` runs for a diff.
//!
//! # Design
//!
//! `just ci` runs only the checks that the diff of the branch can break
//! (01M3WNMKB6PAP6J0QXX4A684HH). `hygiene ci [BASE]` finds each file
//! that differs from the merge base of `HEAD` and BASE (the default is
//! `origin/main`): committed, not committed, and not tracked. Then
//! [`choose`] gives the recipe of the justfile to run:
//!
//! | Recipe | Checks | When |
//! |---|---|---|
//! | `ci-text` | `book`, `reqs`, `wrap` | Each changed file is text. |
//! | `ci-full` | Each check. | Each other case. |
//!
//! A file is text ([`is_text`]) when it is in `docs/` or `design/`, or
//! when it is a `.md` file outside `crates/`. A `.md` file in `crates/`
//! is not text: `riff` holds the files of its plugin, and tests read
//! them.
//!
//! When git cannot compare with BASE, or when no file differs, the
//! recipe is `ci-full`. `hygiene ci` prints the recipe to stdout and
//! one line for the person to stderr: the set, and why. The Gate on
//! GitHub runs `just ci-full` (01M3WNN7VQJKN5MJH7JN50VF4D).
//!
//! ```
//! use hygiene::ci::{choose, Checks};
//!
//! let text = ["design/reviews/x.md".to_owned()];
//! let choice = choose("origin/main", Some(&text));
//! assert_eq!(choice.checks, Checks::Text);
//! assert_eq!(choice.checks.recipe(), "ci-text");
//!
//! let code = ["docs/src/waves.md".to_owned(), "justfile".to_owned()];
//! let choice = choose("origin/main", Some(&code));
//! assert_eq!(choice.checks, Checks::Full);
//! assert_eq!(
//!     choice.to_string(),
//!     "just ci runs each check: justfile is not a text file"
//! );
//! ```

use std::fmt;

/// A set of checks: a recipe of the justfile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checks {
    /// The checks for text: `book`, `reqs` and `wrap`.
    Text,
    /// Each check.
    Full,
}

impl Checks {
    /// The recipe of the justfile that runs this set.
    pub fn recipe(self) -> &'static str {
        match self {
            Self::Text => "ci-text",
            Self::Full => "ci-full",
        }
    }
}

/// The set that `just ci` runs, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// The set.
    pub checks: Checks,
    /// The reason, for the line that `just ci` prints.
    pub why: String,
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let set = match self.checks {
            Checks::Text => "only the text checks (book, reqs, wrap)",
            Checks::Full => "each check",
        };
        write!(f, "just ci runs {set}: {}", self.why)
    }
}

/// True when no code check can fail from a change of the file `path`.
/// `path` is relative to the top of the repository.
///
/// ```
/// use hygiene::ci::is_text;
///
/// assert!(is_text("docs/src/how-it-works.md"));
/// assert!(is_text("docs/book.toml"));
/// assert!(is_text("design/reviews/x.md"));
/// assert!(is_text("CLAUDE.md"));
/// assert!(!is_text("crates/riff/src/main.rs"));
/// assert!(!is_text("crates/riff/claude-plugin/riff/skills/riff/SKILL.md"));
/// assert!(!is_text("justfile"));
/// ```
pub fn is_text(path: &str) -> bool {
    if path.starts_with("crates/") {
        return false;
    }
    path.starts_with("docs/") || path.starts_with("design/") || path.ends_with(".md")
}

/// The set for the files `changed` that differ from the merge base with
/// `base`. `None` says that git cannot compare with `base`.
pub fn choose(base: &str, changed: Option<&[String]>) -> Choice {
    let full = |why: String| Choice {
        checks: Checks::Full,
        why,
    };
    let Some(changed) = changed else {
        return full(format!("git cannot compare this tree with {base}"));
    };
    if changed.is_empty() {
        return full(format!("no file differs from {base}"));
    }
    if let Some(code) = changed.iter().find(|path| !is_text(path)) {
        return full(format!("{code} is not a text file"));
    }
    let why = match changed {
        [one] => format!("{one} is the only file that differs from {base}, and it is text"),
        _ => format!(
            "each of the {} files that differ from {base} is text",
            changed.len()
        ),
    };
    Choice {
        checks: Checks::Text,
        why,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|&name| name.to_owned()).collect()
    }

    #[test]
    fn a_diff_of_only_text_files_gets_the_text_checks() {
        let one = choose("origin/main", Some(&paths(&["design/reviews/x.md"])));
        assert_eq!(one.checks, Checks::Text);
        assert_eq!(
            one.to_string(),
            "just ci runs only the text checks (book, reqs, wrap): design/reviews/x.md \
             is the only file that differs from origin/main, and it is text"
        );
        let two = choose(
            "origin/main",
            Some(&paths(&["docs/src/how-it-works.md", "README.md"])),
        );
        assert_eq!(two.checks, Checks::Text);
        assert!(two.why.starts_with("each of the 2 files"), "{}", two.why);
    }

    #[test]
    fn one_file_that_is_not_text_gets_each_check() {
        let changed = paths(&["docs/src/development.md", "crates/riff/src/main.rs"]);
        let choice = choose("origin/main", Some(&changed));
        assert_eq!(choice.checks, Checks::Full);
        assert_eq!(choice.why, "crates/riff/src/main.rs is not a text file");
    }

    #[test]
    fn a_file_of_the_plugin_is_not_text() {
        for path in [
            "crates/riff/claude-plugin/riff/skills/riff/SKILL.md",
            "crates/riff/claude-plugin/riff/commands/join.md",
            "Cargo.lock",
            ".github/workflows/ci.yml",
            "documents.rs",
        ] {
            assert!(!is_text(path), "{path}");
        }
    }

    #[test]
    fn no_diff_and_no_base_get_each_check() {
        let none = choose("origin/main", Some(&[]));
        assert_eq!(none.checks, Checks::Full);
        assert_eq!(none.why, "no file differs from origin/main");
        let unknown = choose("origin/main", None);
        assert_eq!(unknown.checks, Checks::Full);
        assert_eq!(unknown.why, "git cannot compare this tree with origin/main");
    }
}
