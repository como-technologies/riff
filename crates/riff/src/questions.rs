//! `riff questions`: the open questions for a person, in all the
//! repositories of the person (#1605, #1666, 01M6QQJHV8THJG36XFT95S3Q84,
//! 01M6QQJHXNAVYNNJ86WYY3C8EG).
//!
//! # Design
//!
//! A lead that needs a decision of a person writes the questions in the
//! body of the issue, and adds the label `question`
//! ([`riff_client::waves::QUESTION`]). `riff questions` lists the open
//! issues with that label in each repository of the person at the
//! terminal: not of the riff. riff-server tells which ones: each
//! repository with a session of the person (live or gone), with a lead
//! of the person, or with the notes of a lead of the person. The daemon
//! of the person has a session for each repository that it serves, so
//! these are in the first kind. The repository of the folder is the last
//! one ([`repos`]). A session of another person adds nothing. Each line
//! has the repository, the number, the title and the link
//! ([`crate::text::questions_list`]).
//!
//! `riff questions --web` opens one GitHub search in the browser, as `gh
//! issue view --web` does: `is:open label:question` over the same
//! repositories ([`web_url`]).
//!
//! ```mermaid
//! flowchart LR
//!     W["riff-server: who,<br/>the repos of the person"] --> R["the repositories of the person"]
//!     F["the folder: its repository"] --> R
//!     R -- "gh issue list --label question,<br/>one call for each repository" --> L["the lines of riff questions"]
//!     R -- "--web" --> U["one search: is:open label:question<br/>repo:A repo:B"] --> B["the browser"]
//! ```
//!
//! A repository with its issues turned off is no error: riff skips it. A
//! repository that `gh` cannot read gets no line of its own. One line on
//! stderr at the end names them all ([`crate::text::questions_faults`]).
//! The command exits with 0 when it read at least one repository, and
//! with 1 when it read none and `gh` failed for each.

use std::collections::BTreeSet;

use anyhow::Result;
use riff_core::wire::WhoReply;
use serde::Deserialize;

use crate::pr::Gh;

/// The text of `gh` for a repository with its issues turned off.
const ISSUES_OFF: &str = "has disabled issues";

/// An open issue with the label `question`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Question {
    /// `OWNER/REPO`: `gh` does not tell it, so [`read`] sets it.
    #[serde(skip)]
    pub repo: String,
    pub number: u64,
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub title: String,
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub url: String,
}

/// The repositories of the person at the terminal, `OWNER/REPO`: the
/// `repos` of the reply of riff-server to `who`, and `here`, the
/// repository of the folder. riff-server names the repositories of the
/// caller only: a session of another person adds none, and a repository
/// with the notes of a lead of the person and no session is there.
///
/// ```
/// use riff::questions::repos;
/// use riff_core::wire::WhoReply;
///
/// let reply = |json: serde_json::Value| -> WhoReply { serde_json::from_value(json).unwrap() };
/// let who = reply(serde_json::json!({ "sessions": [], "repos": ["o/r", "o/s"] }));
/// assert_eq!(repos(&who, None), ["o/r".to_owned(), "o/s".to_owned()].into());
/// assert_eq!(repos(&who, Some("o/t")).len(), 3);
/// assert_eq!(repos(&who, Some("o/r")).len(), 2);
/// assert!(repos(&reply(serde_json::json!({ "sessions": [] })), None).is_empty());
/// ```
pub fn repos(who: &WhoReply, here: Option<&str>) -> BTreeSet<String> {
    who.repos
        .iter()
        .map(ToString::to_string)
        .chain(here.map(str::to_owned))
        .collect()
}

/// The arguments of `gh` for the open issues of `repo` with the label
/// `question`.
fn args(repo: &str) -> [&str; 12] {
    [
        "issue",
        "list",
        "--repo",
        repo,
        "--state",
        "open",
        "--label",
        riff_client::waves::QUESTION,
        "--limit",
        "1000",
        "--json",
        "number,title,url",
    ]
}

/// The open issues of `repo` with the label `question`, from `gh`. A
/// repository with its issues turned off has none. It fails with the
/// error of `gh` for any other fault.
pub fn read(gh: &Gh, repo: &str) -> Result<Vec<Question>> {
    let mut found: Vec<Question> = match gh.json(&args(repo)) {
        Ok(found) => found,
        Err(e) if issues_off(&e) => Vec::new(),
        Err(e) => return Err(e),
    };
    for question in &mut found {
        question.repo = repo.to_owned();
    }
    found.sort_by_key(|q| q.number);
    Ok(found)
}

/// True when the error of `gh` says that the repository turned its
/// issues off.
///
/// ```
/// use riff::questions::issues_off;
///
/// let off = anyhow::anyhow!("gh issue list: the 'o/r' repository has disabled issues");
/// assert!(issues_off(&off));
/// assert!(!issues_off(&anyhow::anyhow!("gh issue list: HTTP 404: Not Found")));
/// ```
pub fn issues_off(error: &anyhow::Error) -> bool {
    format!("{error:#}").contains(ISSUES_OFF)
}

/// What `riff questions` read: the questions of each repository in its
/// order, the repositories that `gh` could not read, and how many
/// repositories it read.
#[derive(Debug, Default)]
pub struct Found {
    pub questions: Vec<Question>,
    /// Each repository that `gh` could not read.
    pub faults: Vec<String>,
    /// The repositories that `gh` read, also with no question.
    pub read: usize,
}

impl Found {
    /// True when `gh` failed for each repository: the command has
    /// nothing to list.
    pub fn failed(&self) -> bool {
        self.read == 0 && !self.faults.is_empty()
    }
}

/// [`read`] for each of `repos`, one call of `gh` for each. A repository
/// that fails is a fault, and the others stay.
///
/// ```
/// use riff::questions::list;
///
/// let dir = tempfile::tempdir()?;
/// let script = r#"#!/bin/sh
/// case "$*" in
///   *"--repo o/off "*) echo "the 'o/off' repository has disabled issues" >&2; exit 1 ;;
///   *"--repo o/gone "*|*"--repo o/lost "*) echo "HTTP 404: Not Found" >&2; exit 1 ;;
/// esac
/// echo '[{"number":3,"title":"Who owns it?","url":"https://github.com/o/r/issues/3"}]'
/// "#;
/// let gh = riff::pr::Gh::at(isolated::write_program(dir.path().join("gh"), script)).by("questions");
///
/// let repos = ["o/gone", "o/lost", "o/off", "o/r"].map(String::from).into();
/// let found = list(&gh, &repos);
/// assert_eq!(found.questions.len(), 1);
/// assert_eq!(found.questions[0].repo, "o/r");
/// assert_eq!(found.faults, ["o/gone", "o/lost"]);
/// assert_eq!(found.read, 2);
/// assert!(!found.failed());
///
/// let found = list(&gh, &["o/gone".to_owned()].into());
/// assert!(found.failed());
/// assert!(!list(&gh, &Default::default()).failed());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn list(gh: &Gh, repos: &BTreeSet<String>) -> Found {
    let mut found = Found::default();
    for repo in repos {
        match read(gh, repo) {
            Ok(questions) => {
                found.read += 1;
                found.questions.extend(questions);
            }
            Err(_) => found.faults.push(repo.clone()),
        }
    }
    found
}

/// The address of the search that `riff questions --web` opens: the open
/// issues with the label `question` of `repos`, in one GitHub search.
///
/// ```
/// use riff::questions::web_url;
///
/// let repos = ["o/r".to_owned(), "o/s".to_owned()].into();
/// assert_eq!(
///     web_url(&repos),
///     "https://github.com/issues?q=is%3Aissue+is%3Aopen+label%3Aquestion+repo%3Ao%2Fr+repo%3Ao%2Fs"
/// );
/// ```
pub fn web_url(repos: &BTreeSet<String>) -> String {
    let mut query = format!("is:issue is:open label:{}", riff_client::waves::QUESTION);
    for repo in repos {
        query.push_str(" repo:");
        query.push_str(repo);
    }
    let encoded: String = query
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            b' ' => "+".to_owned(),
            b => format!("%{b:02X}"),
        })
        .collect();
    format!("https://github.com/issues?q={encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_name_with_dots_and_dashes_stays_in_the_search() {
        let repos = ["como-technologies/riff.rs".to_owned()].into();
        assert!(web_url(&repos).ends_with("repo%3Acomo-technologies%2Friff.rs"));
    }
}
