//! `riff questions`: the open questions for a person, in all the
//! projects of the person (#1605, #1666, #1864, 01M6QQJHV8THJG36XFT95S3Q84,
//! 01M6QQJHXNAVYNNJ86WYY3C8EG, 01M77F889Q3D6VCEDNNNXSWGNK).
//!
//! # Design
//!
//! A lead that needs a decision of a person writes the questions in the
//! body of the issue, and adds the label `question`
//! ([`riff_client::waves::QUESTION`]). `riff questions` lists the open
//! issues with that label in each project of the person at the
//! terminal: not of the riff. riff-server tells which ones: each
//! repository with a session of the person (live or gone), with a lead
//! of the person, or with the notes of a lead of the person, that is a
//! project of the riff. The daemon of the person has a session for each
//! repository that it serves, so these are in the first kind. The
//! repository of the folder is the last one, when it is a project
//! ([`repos`]). A session of another person adds nothing, and a
//! repository that is no project adds nothing. Each line has the
//! repository, the number, the title and the link
//! ([`crate::text::questions_list`]).
//!
//! `riff questions --web` opens one GitHub search in the browser, as `gh
//! issue view --web` does: `is:open label:question` over the same
//! projects ([`web_url`]).
//!
//! ```mermaid
//! flowchart LR
//!     W["riff-server: who,<br/>the repos of the person"] --> R["the projects of the person"]
//!     P["riff-server: who,<br/>the projects of the riff"] --> R
//!     F["the folder: its repository"] --> R
//!     R -- "gh issue list --label question,<br/>one call for each project" --> L["the lines of riff questions"]
//!     R -- "--web" --> U["one search: is:open label:question<br/>repo:A repo:B"] --> B["the browser"]
//! ```
//!
//! A repository with its issues turned off is no error: riff skips it. A
//! repository that `gh` cannot read gets no line of its own. One line on
//! stderr at the end names them all ([`crate::text::questions_faults`]).
//! The command exits with 0 when it read at least one repository, and
//! with 1 when it read none and `gh` failed for each.
//!
//! # Answered
//!
//! `riff questions --answered N` and the key `a` of the Waves tab
//! ([`crate::tui::Effect::Answered`]) mark the questions of the issue N
//! as answered ([`answered`], #1782, 01M6XZ2T9TPZC1X36Y2ZXZ83C0,
//! 01M6XZ2TCBVN69GTJYV9X8GBW5): riff removes the label `question`, and a
//! message wakes the lead of the person in the repository. The lead
//! records each answer in the body of the issue. An issue with no label
//! `question` gets a note, and riff changes nothing.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as the person
//!     participant G as gh
//!     participant S as riff-server
//!     participant L as the lead
//!     P->>G: issue view N --json labels
//!     G-->>P: the label question
//!     P->>G: issue edit N --remove-label question
//!     P->>S: post to the repository thread,<br/>user=PERSON,repo=REPO,lead=true
//!     S->>L: wake: the person answered #N
//! ```

use std::collections::BTreeSet;

use anyhow::Result;
use riff_client::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, WhoReply};
use serde::Deserialize;

use crate::pr::Gh;
use crate::text;

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

/// The projects of the person at the terminal, `OWNER/REPO`
/// (01M77F889Q3D6VCEDNNNXSWGNK): each of the `repos` of the reply of
/// riff-server to `who` that is one of its `projects`, and `here`, the
/// repository of the folder, when it is a project. riff-server names the
/// repositories of the caller only: a session of another person adds
/// none, and a repository with the notes of a lead of the person and no
/// session is there. A repository that is no project gives none.
///
/// ```
/// use riff::questions::repos;
/// use riff_core::wire::WhoReply;
///
/// let reply = |json: serde_json::Value| -> WhoReply { serde_json::from_value(json).unwrap() };
/// let who = reply(serde_json::json!({
///     "sessions": [], "repos": ["o/r", "o/s", "local/theme"], "projects": ["o/r", "o/s", "o/t"],
/// }));
/// assert_eq!(repos(&who, None), ["o/r".to_owned(), "o/s".to_owned()].into());
/// assert_eq!(repos(&who, Some("o/t")).len(), 3);
/// assert_eq!(repos(&who, Some("o/r")).len(), 2);
/// // The folder of a repository that is no project.
/// assert_eq!(repos(&who, Some("o/u")).len(), 2);
/// assert!(repos(&reply(serde_json::json!({ "sessions": [], "repos": ["o/r"] })), Some("o/r")).is_empty());
/// ```
pub fn repos(who: &WhoReply, here: Option<&str>) -> BTreeSet<String> {
    who.repos
        .iter()
        .map(ToString::to_string)
        .chain(here.map(str::to_owned))
        .filter(|repo| who.projects.iter().any(|p| p.to_string() == *repo))
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

/// The labels of an issue, from `gh issue view --json labels`.
#[derive(Deserialize)]
struct Labels {
    labels: Vec<crate::rollout::Label>,
}

/// True when the issue `number` of `repo` has the label `question`.
///
/// ```
/// use riff::questions::asked;
///
/// let dir = tempfile::tempdir()?;
/// let script = r#"#!/bin/sh
/// case "$*" in
///   "issue view 12 --repo o/r --json labels") echo '{"labels":[{"name":"question"}]}' ;;
///   *) echo '{"labels":[{"name":"bug"}]}' ;;
/// esac
/// "#;
/// let gh = riff::pr::Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// assert!(asked(&gh, "o/r", 12)?);
/// assert!(!asked(&gh, "o/r", 13)?);
/// # anyhow::Ok(())
/// ```
pub fn asked(gh: &Gh, repo: &str, number: u64) -> Result<bool> {
    let number = number.to_string();
    let args = ["issue", "view", &number, "--repo", repo, "--json", "labels"];
    let read: Labels = gh.json(&args)?;
    Ok(read
        .labels
        .iter()
        .any(|l| l.name == riff_client::waves::QUESTION))
}

/// Marks the questions of the issue `number` of `repo` as answered, as
/// the person `me` at `api` (#1782): riff removes the label `question`
/// with `gh`, and posts a message to the thread of `repo` with the
/// selector of the lead of the person there. The message wakes the lead,
/// and asks it to record each answer in the body. An issue with no label
/// `question` gets a note, and riff changes nothing. It gives the line
/// of the result ([`text::questions_answered`],
/// [`text::questions_not_asked`]). The calls of `gh` run on a thread of
/// their own ([`crate::pr::on_thread`]). The answer is a decision that
/// waits for the person: riff-server checks it first, with a proof that
/// `asker` asks for when the policy is `required` (01M75WKSNREGG95CK40W2A5JPQ).
pub async fn answered(
    gh: &Gh,
    api: &Api,
    me: &SessionUri,
    repo: &ThreadName,
    number: u64,
    asker: &crate::proof::Asker,
) -> Result<String> {
    let name = repo.to_string();
    let (read, at) = (gh.clone(), name.clone());
    if !crate::pr::on_thread(move || asked(&read, &at, number)).await?? {
        return Ok(text::questions_not_asked(&name, number));
    }
    let action = riff_core::proof::Action::Answer {
        repo: repo.clone(),
        number,
    };
    crate::proof::gate(api, &action, asker).await?;
    let (edit, at) = (gh.clone(), name.clone());
    crate::pr::on_thread(move || {
        let number = number.to_string();
        let label = riff_client::waves::QUESTION;
        let args = [
            "issue",
            "edit",
            &number,
            "--repo",
            &at,
            "--remove-label",
            label,
        ];
        edit.run(&args, None)
    })
    .await??;
    let user = me.who().user();
    let to = [Selector::lead(&name)];
    let body = text::questions_answered_message(user, &name, number);
    let posted = api.post(me, Some(repo), &to, &body, Kind::Message).await?;
    Ok(text::questions_answered(&name, number, &posted))
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
