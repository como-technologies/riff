//! The council of a pull request (#581): the members that join for the
//! files and the labels of the change, the order and the limit, and
//! `riff council show PR` (01M7CFBZ7839WHMFARR4T6W13W,
//! 01M7CFBZCDFGG1MV4DEAACQVXK, 01M7CFBZF1BZ0E7H0D4Q7KKY1T,
//! 01M7CFBZHQZ8FDT3Q0NGYKBZWB, 01M7CFBZMCE36ZRXD0GBCM4MFX,
//! 01M7CRX642WTS71F4D4YQVVFJY, 01M7CRX66K9Q750SB8M2MPG6V7).
//!
//! # Design
//!
//! `riff council show PR` reads the pull request with `gh` ([`Pull::read`]):
//! its base branch, the path of each file that it adds, changes, removes
//! or renames, with the old path of a rename, and the labels of the pull
//! request and of its issue. The issue is the issue of the link line or of
//! the trailer `Issue:`. Then riff fetches the base branch, and reads the
//! member files of `origin/BASE` over the default set
//! ([`super::of_branch`]). It reads no file of the head and no file of the
//! folder: a pull request does not change the council of its own verify
//! ([`show`]).
//!
//! ```mermaid
//! flowchart TD
//!     G["gh: the files and the labels of PR #40,<br/>and the labels of its issue"] --> C["the change"]
//!     B["git: the member files of origin/BASE<br/>over the default set"] --> P["the pick"]
//!     C --> P
//!     P --> E["first: the members that join each council"]
//!     P --> A["then: the members by area,<br/>a glob matches a path or a label matches"]
//!     E --> L{"at most 6"}
//!     A --> L
//!     L -- "in" --> S["each member: why it joins, and its text"]
//!     L -- "out" --> O["one line for each member that the limit leaves out"]
//! ```
//!
//! A member with no `files` and no `labels` joins each council. A member
//! by area joins when a glob of its `files` matches a path of the change
//! ([`glob_matches`]), or when one of its `labels` is a label of the change
//! ([`Why::of`]). The council takes first the members that join each
//! council, then the others. In each group, the default set comes in its
//! order, then each other member in the order of its name. A council has
//! at most [`LIMIT`] members ([`pick`]).
//!
//! ```
//! use riff::council::default_set;
//! use riff::council::pick::{Change, pick};
//!
//! let change = Change {
//!     paths: vec!["crates/riff/src/council.rs".into()],
//!     labels: vec!["security".into()],
//! };
//! let council = pick(default_set()?, &change);
//! let names: Vec<&str> = council.seats.iter().map(|seat| seat.member.name.as_str()).collect();
//! assert_eq!(names, ["design", "testing", "docs", "rust", "security", "attacker"]);
//! assert_eq!(council.seats[3].why.to_string(), "the path crates/riff/src/council.rs matches **/*.rs");
//! assert_eq!(council.seats[4].why.to_string(), "the label security");
//! assert!(council.left_out.is_empty());
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::fmt;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::{Joins, Member, of_branch};
use crate::pr::Gh;
use crate::session_image::top;
use crate::text;

/// The most members of a council (01M7CFBZHQZ8FDT3Q0NGYKBZWB).
pub const LIMIT: usize = 6;

/// The `jq` filter of the files of a pull request: the path of each file,
/// and after it the old path of a rename.
const PATHS: &str = ".[] | .filename, (.previous_filename // empty)";

/// The facts of a change that pick the members by area
/// (01M7CFBZF1BZ0E7H0D4Q7KKY1T).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    /// The path of each file that the change adds, changes, removes or
    /// renames, and the old path of each rename, in the order of the
    /// forge.
    pub paths: Vec<String>,
    /// The labels of the pull request and of its issue.
    pub labels: Vec<String>,
}

/// Why a member joins a council.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// Each council: its table has neither `files` nor `labels`.
    Each,
    /// A glob of its `files` matches a path of the change.
    Path {
        /// The first path of the change that a glob of the member matches.
        path: String,
        /// The first glob of the member that matches that path.
        glob: String,
    },
    /// One of its `labels` is a label of the change.
    Label(String),
}

impl Why {
    /// Why a member with the keys `joins` joins the council of `change`,
    /// or `None` when it does not join. A path that a glob matches comes
    /// before a label.
    ///
    /// ```
    /// use riff::council::Joins;
    /// use riff::council::pick::{Change, Why};
    ///
    /// let joins = Joins::Area { files: vec!["docs/**".into()], labels: vec!["book".into()] };
    /// let change = |paths: &[&str], labels: &[&str]| Change {
    ///     paths: paths.iter().map(|p| p.to_string()).collect(),
    ///     labels: labels.iter().map(|l| l.to_string()).collect(),
    /// };
    /// assert_eq!(Why::of(&joins, &change(&["src/a.rs"], &[])), None);
    /// assert_eq!(Why::of(&joins, &change(&["src/a.rs"], &["book"])), Some(Why::Label("book".into())));
    /// assert_eq!(
    ///     Why::of(&joins, &change(&["src/a.rs", "docs/a.md", "docs/b.md"], &["book"])),
    ///     Some(Why::Path { path: "docs/a.md".into(), glob: "docs/**".into() })
    /// );
    /// assert_eq!(Why::of(&Joins::Each, &Change::default()), Some(Why::Each));
    /// ```
    pub fn of(joins: &Joins, change: &Change) -> Option<Why> {
        let Joins::Area { files, labels } = joins else {
            return Some(Why::Each);
        };
        change
            .paths
            .iter()
            .find_map(|path| {
                files
                    .iter()
                    .find(|glob| glob_matches(glob, path))
                    .map(|glob| Why::Path {
                        path: path.clone(),
                        glob: glob.clone(),
                    })
            })
            .or_else(|| {
                labels
                    .iter()
                    .find(|label| change.labels.contains(label))
                    .map(|label| Why::Label(label.clone()))
            })
    }
}

impl fmt::Display for Why {
    /// `each council`, `the path PATH matches GLOB` or `the label LABEL`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Why::Each => f.write_str("each council"),
            Why::Path { path, glob } => write!(
                f,
                "the path {} matches {}",
                text::forge(path),
                text::forge(glob)
            ),
            Why::Label(label) => write!(f, "the label {}", text::forge(label)),
        }
    }
}

/// Whether the glob `glob` of `files` matches the path `path` of a change
/// (01M7CRX642WTS71F4D4YQVVFJY). A part is the text between two `/`. A
/// part `**` of the glob matches no part of the path or more. In each
/// other part, `*` matches each text, `?` matches one character, and each
/// other character matches itself.
///
/// ```
/// use riff::council::pick::glob_matches;
///
/// assert!(glob_matches("**/*.rs", "build.rs"));
/// assert!(glob_matches("**/*.rs", "crates/riff/src/council.rs"));
/// assert!(glob_matches("docs/**", "docs/src/start-a-riff.md"));
/// assert!(glob_matches("**/daemon/**", "crates/riff/src/daemon/sweep.rs"));
/// assert!(glob_matches("**/*token*", "crates/riff/src/claude_token.rs"));
/// assert!(glob_matches("Dockerfile", "Dockerfile"));
/// assert!(glob_matches("src/?.rs", "src/a.rs"));
/// assert!(!glob_matches("*.rs", "src/a.rs"));
/// assert!(!glob_matches("Dockerfile", "deploy/Dockerfile"));
/// assert!(!glob_matches("docs/**", "crates/docs.rs"));
/// assert!(!glob_matches("src/?.rs", "src/ab.rs"));
/// ```
pub fn glob_matches(glob: &str, path: &str) -> bool {
    let path: Vec<&str> = path.split('/').collect();
    // `upto[n]`: the parts of the glob so far match the first `n` parts
    // of the path.
    let mut upto = vec![false; path.len() + 1];
    upto[0] = true;
    for part in glob.split('/') {
        let mut next = vec![false; path.len() + 1];
        if part == "**" {
            let mut seen = false;
            for (n, slot) in next.iter_mut().enumerate() {
                seen |= upto[n];
                *slot = seen;
            }
        } else {
            for n in 1..=path.len() {
                next[n] = upto[n - 1] && part_matches(part, path[n - 1]);
            }
        }
        upto = next;
    }
    upto[path.len()]
}

/// Whether the part `glob` of a glob matches the part `text` of a path:
/// `*` matches each text, and `?` one character.
fn part_matches(glob: &str, text: &str) -> bool {
    let glob: Vec<char> = glob.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut g, mut t) = (0, 0);
    // The last `*` of the glob, and the character of the text where its
    // match ends now.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match glob.get(g) {
            Some('*') => {
                star = Some((g, t));
                g += 1;
            }
            Some(&c) if c == '?' || c == text[t] => {
                g += 1;
                t += 1;
            }
            _ => match star {
                Some((at, end)) => {
                    star = Some((at, end + 1));
                    g = at + 1;
                    t = end + 1;
                }
                None => return false,
            },
        }
    }
    glob[g..].iter().all(|&c| c == '*')
}

/// A member of a council, and why it joins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seat {
    /// The member.
    pub member: Member,
    /// Why it joins.
    pub why: Why,
}

/// The council of a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Council {
    /// Its members, at most [`LIMIT`], in the order of
    /// 01M7CFBZHQZ8FDT3Q0NGYKBZWB.
    pub seats: Vec<Seat>,
    /// The members that join, but that the limit leaves out, in the same
    /// order.
    pub left_out: Vec<Seat>,
}

/// The council of `change` from `members` (01M7CFBZHQZ8FDT3Q0NGYKBZWB).
/// `members` come in the order of [`super::of_branch`]: the default set in
/// its order, then each other member in the order of its name. The
/// council takes first the members that join each council, then the
/// members by area, each group in that order. It has at most [`LIMIT`]
/// members.
pub fn pick(members: Vec<Member>, change: &Change) -> Council {
    let (mut seats, by_area): (Vec<Seat>, Vec<Seat>) = members
        .into_iter()
        .filter_map(|member| Why::of(&member.joins, change).map(|why| Seat { member, why }))
        .partition(|seat| seat.why == Why::Each);
    seats.extend(by_area);
    let left_out = seats.split_off(seats.len().min(LIMIT));
    Council { seats, left_out }
}

impl Council {
    /// The text of `riff council show` for the pull request `number`,
    /// with the member files of `branch` (01M7CRX66K9Q750SB8M2MPG6V7): a
    /// line with the count of the members, then for each member a line
    /// `== NAME (SOURCE): WHY` and the text of its file after the table.
    /// At the end, the line `== Left out by the limit of 6`, and a line
    /// `NAME (SOURCE): WHY` for each member that the limit leaves out.
    ///
    /// ```
    /// use riff::council::default_set;
    /// use riff::council::pick::{Change, pick};
    ///
    /// let change = Change { paths: vec!["src/a.rs".into()], labels: vec![] };
    /// let text = pick(default_set()?, &change).text(40, "origin/main");
    /// let lines: Vec<&str> = text.lines().collect();
    /// assert_eq!(lines[0], "The council of pull request #40 has 4 members. riff read the member files of origin/main.");
    /// assert_eq!(lines[2], "== design (riff): each council");
    /// assert_eq!(lines[4], "# Design");
    /// assert!(lines.contains(&"== rust (riff): the path src/a.rs matches **/*.rs"), "{text}");
    /// assert!(!text.contains("Left out"), "{text}");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn text(&self, number: u64, branch: &str) -> String {
        let count = match self.seats.len() {
            0 => "no member".to_owned(),
            1 => "1 member".to_owned(),
            n => format!("{n} members"),
        };
        let mut out = format!(
            "The council of pull request #{number} has {count}. riff read the member files of {}.\n",
            text::forge(branch)
        );
        for seat in &self.seats {
            out.push_str(&format!(
                "\n== {}\n\n{}",
                heading(seat),
                text::safe(&seat.member.text)
            ));
        }
        if !self.left_out.is_empty() {
            out.push_str(&format!("\n== Left out by the limit of {LIMIT}\n\n"));
            for seat in &self.left_out {
                out.push_str(&format!("{}\n", heading(seat)));
            }
        }
        out
    }
}

/// `NAME (SOURCE): WHY` of a member of a council.
fn heading(seat: &Seat) -> String {
    format!(
        "{} ({}): {}",
        seat.member.name,
        text::forge(&seat.member.source.to_string()),
        seat.why
    )
}

/// A pull request, as its council needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pull {
    /// The name of its base branch.
    pub base: String,
    /// Its paths and its labels.
    pub change: Change,
}

impl Pull {
    /// Reads the pull request `number` with `gh`, in the repository of the
    /// folder of the process: its base branch, the paths of its files
    /// with the old path of each rename, and the labels of the pull
    /// request and of its issue. The issue is the issue of the link line
    /// or of the trailer `Issue:` ([`hygiene::issue_number`]). A label
    /// counts once.
    ///
    /// ```
    /// use riff::council::pick::Pull;
    /// use riff::pr::Gh;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let script = r#"#!/bin/sh
    /// case "$*" in
    ///   "pr view 40 --json baseRefName,body,labels")
    ///     printf '%s' '{"baseRefName":"main","body":"Closes #12\n\nIssue: #12\n","labels":[{"name":"docs"}]}' ;;
    ///   "api repos/{owner}/{repo}/pulls/40/files?per_page=100 --paginate --jq "*)
    ///     printf 'notes/start.md\ndocs/start.md\nsrc/a.rs\n' ;;
    ///   "issue view 12 --json labels")
    ///     printf '%s' '{"labels":[{"name":"security"},{"name":"docs"}]}' ;;
    ///   *) exit 1 ;;
    /// esac
    /// "#;
    /// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
    /// let pull = Pull::read(&gh, 40)?;
    /// assert_eq!(pull.base, "main");
    /// assert_eq!(pull.change.paths, ["notes/start.md", "docs/start.md", "src/a.rs"]);
    /// assert_eq!(pull.change.labels, ["docs", "security"]);
    /// assert!(Pull::read(&gh, 41).is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn read(gh: &Gh, number: u64) -> Result<Pull> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct View {
            base_ref_name: String,
            body: String,
            labels: Vec<Label>,
        }
        #[derive(Deserialize)]
        struct Labels {
            labels: Vec<Label>,
        }
        #[derive(Deserialize)]
        struct Label {
            name: String,
        }
        let view: View = gh.json(&[
            "pr",
            "view",
            &number.to_string(),
            "--json",
            "baseRefName,body,labels",
        ])?;
        let files = format!("repos/{{owner}}/{{repo}}/pulls/{number}/files?per_page=100");
        let paths = gh.run(&["api", &files, "--paginate", "--jq", PATHS], None)?;
        let mut labels: Vec<String> = Vec::new();
        let mut add = |list: Vec<Label>| {
            for label in list {
                if !labels.contains(&label.name) {
                    labels.push(label.name);
                }
            }
        };
        add(view.labels);
        let pr = hygiene::PullRequest::new("", &view.body, None);
        if let Some(issue) = hygiene::issue_number(&pr) {
            let of_issue: Labels =
                gh.json(&["issue", "view", &issue.to_string(), "--json", "labels"])?;
            add(of_issue.labels);
        }
        Ok(Pull {
            base: view.base_ref_name,
            change: Change {
                paths: paths
                    .lines()
                    .filter(|path| !path.is_empty())
                    .map(str::to_owned)
                    .collect(),
                labels,
            },
        })
    }
}

/// The text of `riff council show PR` in the folder `dir`: the council
/// of the pull request `number` (01M7CFBZMCE36ZRXD0GBCM4MFX,
/// 01M7CRX66K9Q750SB8M2MPG6V7). It reads the
/// pull request with `gh` ([`Pull::read`]), fetches its base branch
/// `BASE` from `origin`, and picks the council from the member files of
/// `origin/BASE` over the default set ([`pick`]). When the fetch fails,
/// it says so to `say`, and reads `origin/BASE` as the clone has it.
pub fn show(gh: &Gh, dir: &Path, number: u64, mut say: impl FnMut(&str)) -> Result<String> {
    let pull = Pull::read(gh, number)?;
    let top = top(dir)?;
    let branch = format!("origin/{}", pull.base);
    // Full names: no name of a branch reads as an option of git.
    let tracking = format!("refs/remotes/{branch}");
    let refspec = format!("+refs/heads/{}:{tracking}", pull.base);
    let fetch = ["fetch", "--quiet", "origin", &refspec];
    if let Err(e) = crate::git_credential::run(&top, "origin", &fetch) {
        say(&text::council_no_fetch(&branch, &format!("{e:#}")));
    }
    let members = of_branch(&top, &tracking)
        .with_context(|| format!("riff cannot read the council of {}", text::forge(&branch)))?;
    Ok(pick(members, &pull.change).text(number, &branch))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::council::{FOLDER, default_set, of_tree};

    /// The change of the paths `paths` and the labels `labels`.
    fn change(paths: &[&str], labels: &[&str]) -> Change {
        Change {
            paths: paths.iter().map(|path| path.to_string()).collect(),
            labels: labels.iter().map(|label| label.to_string()).collect(),
        }
    }

    /// The names of the members of `seats`.
    fn names(seats: &[Seat]) -> Vec<&str> {
        seats.iter().map(|seat| seat.member.name.as_str()).collect()
    }

    /// 01M7CFBZCDFGG1MV4DEAACQVXK, 01M7CFBZF1BZ0E7H0D4Q7KKY1T: a change of
    /// a `.rs` file gets design, testing, docs and rust. A change of
    /// `docs/src/start-a-riff.md` gets design, testing, docs, requirements
    /// and newcomer. The label `security` brings security and attacker.
    #[test]
    fn the_files_and_the_labels_of_the_change_pick_the_members() {
        for (paths, labels, want) in [
            (
                &["crates/riff/src/council.rs"][..],
                &[][..],
                &["design", "testing", "docs", "rust"][..],
            ),
            (
                &["docs/src/start-a-riff.md"],
                &[],
                &["design", "testing", "docs", "requirements", "newcomer"],
            ),
            (
                &["justfile"],
                &["security"],
                &["design", "testing", "docs", "security", "attacker"],
            ),
            (&["justfile"], &[], &["design", "testing", "docs"]),
        ] {
            let council = pick(default_set().unwrap(), &change(paths, labels));
            assert_eq!(names(&council.seats), want, "{paths:?} {labels:?}");
            assert!(council.left_out.is_empty(), "{paths:?} {labels:?}");
        }
        let council = pick(
            default_set().unwrap(),
            &change(&["justfile"], &["security"]),
        );
        assert_eq!(council.seats[0].why, Why::Each);
        assert_eq!(council.seats[3].why, Why::Label("security".into()));
    }

    /// 01M7CFBZHQZ8FDT3Q0NGYKBZWB: a change that each member matches gets
    /// 6 members in the order of the default set, and the output names
    /// the 3 members that the limit leaves out.
    #[test]
    fn a_change_that_each_member_matches_gets_6_and_names_the_3_left_out() {
        let each = change(
            &["crates/riff-server/src/main.rs", "docs/src/requirements.md"],
            &["operations"],
        );
        let council = pick(default_set().unwrap(), &each);
        assert_eq!(
            names(&council.seats),
            ["design", "testing", "docs", "rust", "security", "attacker"]
        );
        assert_eq!(
            names(&council.left_out),
            ["requirements", "newcomer", "operations"]
        );
        let text = council.text(40, "origin/main");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "The council of pull request #40 has 6 members. riff read the member files of origin/main."
        );
        let headings: Vec<&str> = lines
            .iter()
            .filter_map(|line| line.strip_prefix("== "))
            .collect();
        assert_eq!(
            headings,
            [
                "design (riff): each council",
                "testing (riff): each council",
                "docs (riff): each council",
                "rust (riff): the path crates/riff-server/src/main.rs matches **/*.rs",
                "security (riff): the path crates/riff-server/src/main.rs matches crates/riff-server/**",
                "attacker (riff): the path crates/riff-server/src/main.rs matches crates/riff-server/**",
                "Left out by the limit of 6",
            ]
        );
        assert_eq!(
            lines[lines.len() - 3..],
            [
                "requirements (riff): the path docs/src/requirements.md matches **/requirements*.md",
                "newcomer (riff): the path docs/src/requirements.md matches docs/**",
                "operations (riff): the label operations",
            ]
        );
        for seat in &council.seats {
            assert!(text.contains(&seat.member.text), "{}", seat.member.name);
        }
        for seat in &council.left_out {
            assert!(!text.contains(&seat.member.text), "{}", seat.member.name);
        }
    }

    /// A member file in the work tree.
    fn write(top: &Path, file: &str, text: &str) {
        fs::create_dir_all(top.join(FOLDER)).unwrap();
        fs::write(top.join(FOLDER).join(file), text).unwrap();
    }

    /// 01M7CFBZHQZ8FDT3Q0NGYKBZWB: a member of the repository with no keys
    /// comes after docs, and a new member by area comes after operations.
    /// Each other member comes in the order of its name.
    #[test]
    fn a_member_of_the_repository_comes_after_the_default_set_of_its_group() {
        let top = tempfile::tempdir().unwrap();
        write(top.path(), "attic.md", "+++\n+++\n# Attic\n");
        write(
            top.path(),
            "performance.md",
            "+++\nfiles = [\"**\"]\n+++\n# Performance\n",
        );
        write(
            top.path(),
            "big-o.md",
            "+++\nlabels = [\"slow\"]\n+++\n# Big O\n",
        );
        let members = of_tree(top.path()).unwrap();
        let council = pick(members.clone(), &change(&["notes/daemon.txt"], &[]));
        assert_eq!(
            names(&council.seats),
            [
                "design",
                "testing",
                "docs",
                "attic",
                "operations",
                "performance"
            ]
        );
        assert_eq!(
            council.seats[5].why,
            Why::Path {
                path: "notes/daemon.txt".into(),
                glob: "**".into()
            }
        );
        let council = pick(members, &change(&["notes/daemon.txt"], &["slow"]));
        assert_eq!(
            names(&council.seats),
            ["design", "testing", "docs", "attic", "operations", "big-o"]
        );
        assert_eq!(names(&council.left_out), ["performance"]);
    }

    /// A file of the default set that takes the place of a member keeps
    /// the place of that member in its group.
    #[test]
    fn a_file_in_the_place_of_a_default_member_keeps_its_place() {
        let top = tempfile::tempdir().unwrap();
        write(top.path(), "rust.md", "+++\n+++\n# Our Rust\n");
        let council = pick(of_tree(top.path()).unwrap(), &change(&["justfile"], &[]));
        assert_eq!(names(&council.seats), ["design", "testing", "docs", "rust"]);
        assert_eq!(council.seats[3].why, Why::Each);
    }

    /// A text of a member file, of a path or of a label goes to the
    /// terminal with no escape sequence.
    #[test]
    fn the_text_of_show_prints_no_escape_sequence() {
        let top = tempfile::tempdir().unwrap();
        write(
            top.path(),
            "perf.md",
            "+++\nlabels = [\"a\\u001b]0;x\\u0007b\"]\n+++\n# Perf\x1b[2J\n\nYou review.\n",
        );
        let council = pick(
            of_tree(top.path()).unwrap(),
            &change(&["src/\u{1b}[31ma.rs"], &["a\u{1b}]0;x\u{7}b"]),
        );
        let text = council.text(40, "origin/ma\u{1b}[2Kin");
        assert!(!text.contains('\u{1b}'), "{text:?}");
        assert!(!text.contains('\u{7}'), "{text:?}");
        assert!(
            text.contains("== perf (.riff/council/perf.md): the label ab"),
            "{text}"
        );
        assert!(text.contains("the path src/a.rs matches **/*.rs"), "{text}");
    }

    /// The glob matches by the parts of the path.
    #[test]
    fn a_glob_matches_by_the_parts_of_the_path() {
        for (glob, path, want) in [
            ("**", "a", true),
            ("**", "a/b/c", true),
            ("**/a", "a", true),
            ("**/a", "x/y/a", true),
            ("a/**/b", "a/b", true),
            ("a/**/b", "a/x/y/b", true),
            ("a/**/b", "a/x/y/c", false),
            ("**/requirements*.md", "docs/src/requirements.md", true),
            (
                "**/requirements*.md",
                "docs/src/requirements-history.md",
                true,
            ),
            ("**/requirements*.md", "docs/src/requirement.md", false),
            ("*", "a/b", false),
            ("*a*b*", "xxaxxbxx", true),
            ("*a*b", "xxaxxbxxc", false),
            ("a*", "a", true),
            ("README.md", "README.md", true),
            ("README.md", "docs/README.md", false),
            (".github/**", ".github/workflows/ci.yml", true),
            ("**/cloud.rs", "crates/riff/src/cloud.rs", true),
            ("**/cloud.rs", "crates/riff/src/cloud.rs.bak", false),
        ] {
            assert_eq!(glob_matches(glob, path), want, "{glob} {path}");
        }
    }
}
