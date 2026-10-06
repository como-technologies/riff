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
//! ## The check of an author: `just check`
//!
//! The job `Gate` of GitHub runs `just ci-full`: the one full run of
//! each pushed commit (01M49HAZ5BZYGAR9PGC089RM3F). Before a push, an
//! author runs `just check` (01M49HAZA5K08XW2JQ11TG87JP): the fast
//! checks (`fmt-check`, `lint`, `doc`, `book`, `reqs`, `wrap`) and the
//! tests of the crates that the diff touches. `hygiene crates [BASE]`
//! prints the arguments of `cargo test` for these crates ([`tests`]),
//! and one line for the person to stderr.
//!
//! ```mermaid
//! flowchart LR
//!     D["the changed files"] --> T{"each file"}
//!     T -- "text" --> N["no test"]
//!     T -- "in crates/NAME/" --> C["NAME, and each crate<br/>that depends on it"]
//!     T -- "each other file" --> W["--workspace"]
//! ```
//!
//! ## One run at a time in a worktree
//!
//! Two runs in one worktree share its `target`. Each waits for the
//! cargo lock of the other, and the load and the time double. So
//! `just ci` and `just ci-full` first source `crates/hygiene/ci-lock.sh`
//! and call `ci_lock` (01M43DKYVAX0TJ2F5YYGYFSZ4G). The check is shell,
//! not this crate: `cargo run` of a second run waits for the build lock
//! of the first run, so it cannot stop at once.
//!
//! ```mermaid
//! flowchart TD
//!     S["just ci"] --> E{"RIFF_CI_LOCK set?<br/>(the holder started this run)"}
//!     E -- yes --> R["run the checks"]
//!     E -- no --> C{"create target/.riff-ci.lock<br/>(fails when it exists)"}
//!     C -- created --> H["hold it: pid, start time;<br/>remove it at the exit"] --> R
//!     C -- exists --> L{"does its pid run,<br/>with the same start time?"}
//!     L -- yes --> X["one line, exit 1"]
//!     L -- no --> D["remove the old lock"] --> C
//! ```
//!
//! The pid and the start time of `ps -o lstart=` name one process, so a
//! new process with the same pid does not hold the lock. The lock is in
//! the `target` of the worktree, so runs in two worktrees go on.
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

/// A crate of the workspace, as `cargo metadata --no-deps` gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The name of the package, for example `riff-core`.
    pub name: String,
    /// The directory of the crate from the top of the repository, for
    /// example `crates/riff-core`.
    pub dir: String,
    /// The names of each dependency: normal, dev and build.
    pub deps: Vec<String>,
}

/// The tests that `just check` runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tests {
    /// No test: the diff changes no crate.
    None,
    /// The tests of these crates, in name order.
    Crates(Vec<String>),
    /// The tests of each crate.
    Workspace,
}

impl Tests {
    /// The arguments of `cargo test`: empty, `-p NAME` for each crate, or
    /// `--workspace`.
    pub fn args(&self) -> String {
        match self {
            Self::None => String::new(),
            Self::Crates(names) => names
                .iter()
                .map(|name| format!("-p {name}"))
                .collect::<Vec<_>>()
                .join(" "),
            Self::Workspace => "--workspace".to_owned(),
        }
    }
}

/// The tests that `just check` runs, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestChoice {
    /// The tests.
    pub tests: Tests,
    /// The reason, for the line that `just check` prints.
    pub why: String,
}

impl fmt::Display for TestChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let set = match &self.tests {
            Tests::None => "no test".to_owned(),
            Tests::Crates(names) => format!("the tests of {}", names.join(", ")),
            Tests::Workspace => "the tests of each crate".to_owned(),
        };
        write!(f, "just check runs {set}: {}", self.why)
    }
}

/// The tests for the files `changed` that differ from the merge base
/// with `base` (01M49HAZA5K08XW2JQ11TG87JP): the tests of each crate
/// with a changed file, and of each crate that depends on one of them,
/// also through another crate. A text file ([`is_text`]) needs no test.
/// Each other file outside the crates, for example `Cargo.lock` or the
/// `justfile`, needs the tests of each crate. `None` says that git
/// cannot compare with `base`: then each crate.
///
/// ```
/// use hygiene::ci::{tests, Member, Tests};
///
/// let member = |name: &str, deps: &[&str]| Member {
///     name: name.into(),
///     dir: format!("crates/{name}"),
///     deps: deps.iter().map(|&d| d.into()).collect(),
/// };
/// let members = [
///     member("riff-core", &[]),
///     member("riff-server", &["riff-core"]),
///     member("riff", &["riff-core", "riff-server"]),
///     member("reqs", &[]),
/// ];
/// let changed = ["crates/riff-server/src/lib.rs".to_owned()];
/// let choice = tests("origin/main", Some(&changed), &members);
/// assert_eq!(choice.tests.args(), "-p riff -p riff-server");
///
/// let core = ["crates/riff-core/src/record.rs".to_owned()];
/// let choice = tests("origin/main", Some(&core), &members);
/// assert_eq!(choice.tests.args(), "-p riff -p riff-core -p riff-server");
///
/// let book = ["docs/src/development.md".to_owned()];
/// assert_eq!(tests("origin/main", Some(&book), &members).tests, Tests::None);
///
/// let lock = ["Cargo.lock".to_owned()];
/// let choice = tests("origin/main", Some(&lock), &members);
/// assert_eq!(choice.tests, Tests::Workspace);
/// assert_eq!(
///     choice.to_string(),
///     "just check runs the tests of each crate: Cargo.lock is in no crate, and it is not text"
/// );
/// ```
pub fn tests(base: &str, changed: Option<&[String]>, members: &[Member]) -> TestChoice {
    let Some(changed) = changed else {
        return TestChoice {
            tests: Tests::Workspace,
            why: format!("git cannot compare this tree with {base}"),
        };
    };
    let mut names: Vec<String> = Vec::new();
    for path in changed {
        let member = members
            .iter()
            .find(|m| path.starts_with(&format!("{}/", m.dir)));
        match member {
            Some(member) => names.push(member.name.clone()),
            None if is_text(path) => {}
            None => {
                return TestChoice {
                    tests: Tests::Workspace,
                    why: format!("{path} is in no crate, and it is not text"),
                };
            }
        }
    }
    if names.is_empty() {
        return TestChoice {
            tests: Tests::None,
            why: format!("no crate differs from {base}"),
        };
    }
    names.sort();
    names.dedup();
    let changed_crates = match &names[..] {
        [one] => format!("{one} differs"),
        many => format!("{} differ", many.join(", ")),
    };
    loop {
        let more: Vec<String> = members
            .iter()
            .filter(|m| !names.contains(&m.name))
            .filter(|m| m.deps.iter().any(|d| names.contains(d)))
            .map(|m| m.name.clone())
            .collect();
        if more.is_empty() {
            break;
        }
        names.extend(more);
    }
    names.sort();
    TestChoice {
        tests: Tests::Crates(names),
        why: format!(
            "{changed_crates} from {base}, and each crate that depends on a changed crate runs too"
        ),
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

    fn members() -> Vec<Member> {
        let member = |name: &str, deps: &[&str]| Member {
            name: name.into(),
            dir: format!("crates/{name}"),
            deps: deps.iter().map(|&d| d.into()).collect(),
        };
        vec![
            member("hygiene", &[]),
            member("isolated", &[]),
            member("riff-core", &[]),
            member("riff-server", &["riff-core", "isolated"]),
            member("riff", &["riff-core", "hygiene", "riff-server", "isolated"]),
        ]
    }

    #[test]
    fn a_crate_with_no_dependent_runs_only_its_own_tests() {
        let changed = paths(&["crates/riff/src/pr.rs", "docs/src/development.md"]);
        let choice = tests("origin/main", Some(&changed), &members());
        assert_eq!(choice.tests, Tests::Crates(vec!["riff".into()]));
        assert_eq!(
            choice.to_string(),
            "just check runs the tests of riff: riff differs from origin/main, and each crate \
             that depends on a changed crate runs too"
        );
    }

    #[test]
    fn the_dependents_of_a_crate_run_also_through_another_crate() {
        let changed = paths(&["crates/isolated/src/lib.rs"]);
        let choice = tests("origin/main", Some(&changed), &members());
        assert_eq!(choice.tests.args(), "-p isolated -p riff -p riff-server");
    }

    #[test]
    fn a_crate_dir_is_not_the_start_of_another_crate_dir() {
        let changed = paths(&["crates/riff-core/src/lib.rs"]);
        let choice = tests("origin/main", Some(&changed), &members());
        assert_eq!(choice.tests.args(), "-p riff -p riff-core -p riff-server");
        let hygiene = paths(&["crates/hygiene/ci-lock.sh"]);
        let choice = tests("origin/main", Some(&hygiene), &members());
        assert_eq!(choice.tests.args(), "-p hygiene -p riff");
    }

    #[test]
    fn a_file_outside_the_crates_runs_each_test_and_text_runs_none() {
        for path in ["justfile", ".github/workflows/ci.yml", "Cargo.toml"] {
            let choice = tests("origin/main", Some(&paths(&[path])), &members());
            assert_eq!(choice.tests, Tests::Workspace, "{path}");
            assert_eq!(choice.tests.args(), "--workspace");
        }
        let text = paths(&["CLAUDE.md", "docs/src/how-it-works.md"]);
        let choice = tests("origin/main", Some(&text), &members());
        assert_eq!(choice.tests, Tests::None);
        assert_eq!(choice.tests.args(), "");
        assert_eq!(
            choice.to_string(),
            "just check runs no test: no crate differs from origin/main"
        );
        let unknown = tests("origin/main", None, &members());
        assert_eq!(unknown.tests, Tests::Workspace);
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
