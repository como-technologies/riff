//! `riff questions`: the open questions for a person, in all the
//! repositories of the riff (#1605).
//!
//! # Design
//!
//! A lead that needs a decision of a person writes the questions in the
//! body of the issue, and adds the label `question`
//! ([`riff_client::waves::QUESTION`]). `riff questions` lists the open
//! issues with that label in each repository of the riff: the repository
//! of each session on riff-server, and the repository of the folder
//! ([`repos`]). Each line has the repository, the number, the title and
//! the link ([`crate::text::questions_list`]).
//!
//! `riff questions --web` opens one GitHub search in the browser, as `gh
//! issue view --web` does: `is:open label:question` over the same
//! repositories ([`web_url`]).
//!
//! ```mermaid
//! flowchart LR
//!     W["riff-server: who"] --> R["the repositories of the riff"]
//!     F["the folder: its repository"] --> R
//!     R -- "gh issue list --label question,<br/>one call for each repository" --> L["the lines of riff questions"]
//!     R -- "--web" --> U["one search: is:open label:question<br/>repo:A repo:B"] --> B["the browser"]
//! ```
//!
//! A repository that `gh` cannot read gets one line on stderr, and the
//! list shows the other repositories. The command then exits with 1.

use std::collections::BTreeSet;

use anyhow::Result;
use riff_core::name::Repo;
use riff_core::wire::SessionInfo;
use serde::Deserialize;

use crate::pr::Gh;

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

/// The repositories of the riff, `OWNER/REPO`: the repository of each
/// session of `who` (the gone sessions too, when the caller read them),
/// and `here`, the repository of the folder. A session outside git has
/// none.
///
/// ```
/// use riff::questions::repos;
/// use riff_core::wire::SessionInfo;
///
/// let session = |uri: &str| -> SessionInfo {
///     serde_json::from_value(serde_json::json!({ "uri": uri, "live": true })).unwrap()
/// };
/// let who = [
///     session("riff://mike@thelio/o/r?session=a1"),
///     session("riff://mike@thelio/o/s?session=a2&claim=issue-3"),
///     session("riff://brett@pangolin/o/r?session=b1"),
///     session("riff://brett@pangolin?session=b2"),
/// ];
/// assert_eq!(repos(&who, None), ["o/r".to_owned(), "o/s".to_owned()].into());
/// assert_eq!(repos(&who, Some("o/t")).len(), 3);
/// assert!(repos(&[], None).is_empty());
/// ```
pub fn repos(who: &[SessionInfo], here: Option<&str>) -> BTreeSet<String> {
    who.iter()
        .filter(|s| matches!(s.uri.place().repo(), Repo::Git { .. }))
        .map(|s| s.uri.place().repo_text())
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

/// The open issues of `repo` with the label `question`, from `gh`. It
/// fails with the error of `gh`.
pub fn read(gh: &Gh, repo: &str) -> Result<Vec<Question>> {
    let mut found: Vec<Question> = gh.json(&args(repo))?;
    for question in &mut found {
        question.repo = repo.to_owned();
    }
    found.sort_by_key(|q| q.number);
    Ok(found)
}

/// What `riff questions` read: the questions of each repository in its
/// order, and the repositories that `gh` could not read, each with its
/// error.
#[derive(Debug, Default)]
pub struct Found {
    pub questions: Vec<Question>,
    pub faults: Vec<(String, String)>,
}

/// [`read`] for each of `repos`, one call of `gh` for each. A repository
/// that fails is a fault, and the others stay.
pub fn list(gh: &Gh, repos: &BTreeSet<String>) -> Found {
    let mut found = Found::default();
    for repo in repos {
        match read(gh, repo) {
            Ok(questions) => found.questions.extend(questions),
            Err(e) => found.faults.push((repo.clone(), format!("{e:#}"))),
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
