//! Issue hygiene of this repository: the form of a pull request, and of
//! a commit on `main`. The module [`book`] checks the book.
//!
//! # Design
//!
//! A change reaches `main` through a pull request, as one squash commit.
//! GitHub makes the commit from the pull request: its title, then the
//! number of the pull request, then its body. So the form of the body is
//! the form of each commit on `main`:
//!
//! ```text
//! Closes #77
//!
//! Pause and resume the riff. A new riff starts paused.
//!
//! Issue: #77
//! Milestone: Wave 3
//! ```
//!
//! - The link line, `Closes #N` or `Refs #N`, links the pull request and
//!   the issue. The last pull request of an issue has `Closes #N`, so its
//!   merge closes the issue. Each other one has `Refs #N`.
//! - The trailers `Issue:` and `Milestone:` end the body. They name the
//!   issue and the wave in the same form in each commit.
//!
//! `hygiene pr N` reads pull request N and its issue with `gh`, and runs
//! [`check_pr`]. `hygiene commit [REV]` reads the message of a commit
//! with `git`, and runs [`check_commit`]. Each error names its rule:
//!
//! | Rule | Finds |
//! |---|---|
//! | `link` | A body with no link line, or with more than one. |
//! | `keyword` | Another closing keyword of GitHub before an issue number, for example `Fixes #12`. It closes an issue with no check. |
//! | `title` | A title of a pull request that ends with `(#N)`. GitHub adds the number of the pull request. |
//! | `issue-trailer` | No `Issue: #N` trailer, more than one, or one that names another issue than the link line. |
//! | `milestone-trailer` | No `Milestone:` trailer, more than one, or one that names another milestone than the pull request. |
//! | `milestone` | A pull request or an issue with no milestone, or two different milestones. |
//! | `issue-open` | An issue that is closed. |
//! | `commit-title` | A commit title that does not end with `(#PR)`, or that names the issue in place of the pull request. |
//!
//! ```
//! use hygiene::{Issue, PullRequest};
//!
//! let pr = PullRequest::new(
//!     "Pause and resume the riff",
//!     "Closes #77\n\nPause the riff.\n\nIssue: #77\nMilestone: Wave 3\n",
//!     Some("Wave 3"),
//! );
//! let issue = Issue::new(77, "OPEN", Some("Wave 3"));
//! assert!(hygiene::check_pr(&pr, Some(&issue)).is_empty());
//!
//! let commit = "Pause and resume the riff (#90)\n\nCloses #77\n\nIssue: #77\nMilestone: Wave 3\n";
//! assert!(hygiene::check_commit(commit).is_empty());
//! ```

use std::fmt;

use serde::Deserialize;

pub mod book;

/// A milestone, as `gh --json milestone` gives it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Milestone {
    /// The name, for example `Wave 3`.
    pub title: String,
}

/// A pull request, as `gh pr view --json title,body,milestone` gives it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PullRequest {
    /// The title. The squash commit gets it, and the number.
    pub title: String,
    /// The body. The squash commit gets it as its message.
    pub body: String,
    /// The milestone, or `None`.
    pub milestone: Option<Milestone>,
}

impl PullRequest {
    /// A pull request with a title, a body and a milestone.
    pub fn new(title: &str, body: &str, milestone: Option<&str>) -> Self {
        Self {
            title: title.to_owned(),
            body: body.to_owned(),
            milestone: milestone.map(milestone_of),
        }
    }
}

/// An issue, as `gh issue view --json number,state,milestone` gives it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Issue {
    /// The number.
    pub number: u64,
    /// `OPEN` or `CLOSED`.
    pub state: String,
    /// The milestone, or `None`.
    pub milestone: Option<Milestone>,
}

impl Issue {
    /// An issue with a number, a state and a milestone.
    pub fn new(number: u64, state: &str, milestone: Option<&str>) -> Self {
        Self {
            number,
            state: state.to_owned(),
            milestone: milestone.map(milestone_of),
        }
    }
}

fn milestone_of(title: &str) -> Milestone {
    Milestone {
        title: title.to_owned(),
    }
}

/// A broken rule of issue hygiene.
///
/// ```
/// let error = hygiene::Error::new("link", "the body has no link line");
/// assert_eq!(error.to_string(), "link: the body has no link line");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The rule, for example `link`. See the table of rules.
    pub rule: &'static str,
    /// What is wrong, and what to do.
    pub text: String,
}

impl Error {
    /// An error of `rule`.
    pub fn new(rule: &'static str, text: impl Into<String>) -> Self {
        Self {
            rule,
            text: text.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.rule, self.text)
    }
}

/// The link line of a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// `Closes #N`: the last pull request of issue N. Its merge closes
    /// the issue.
    Closes(u64),
    /// `Refs #N`: each other pull request of issue N.
    Refs(u64),
}

impl Link {
    /// The number of the issue.
    pub fn issue(self) -> u64 {
        match self {
            Self::Closes(n) | Self::Refs(n) => n,
        }
    }

    /// The link of one line: `Closes #N` or `Refs #N`, and nothing else.
    ///
    /// ```
    /// use hygiene::Link;
    /// assert_eq!(Link::parse("Closes #12"), Some(Link::Closes(12)));
    /// assert_eq!(Link::parse("  Refs #7 "), Some(Link::Refs(7)));
    /// assert_eq!(Link::parse("closes #12"), None, "a keyword, not a link line");
    /// assert_eq!(Link::parse("Closes #12 and #13"), None);
    /// ```
    pub fn parse(line: &str) -> Option<Self> {
        let mut words = line.split_whitespace();
        let (word, number, None) = (words.next()?, words.next()?, words.next()) else {
            return None;
        };
        let n = issue_ref(number)?;
        match word {
            "Closes" => Some(Self::Closes(n)),
            "Refs" => Some(Self::Refs(n)),
            _ => None,
        }
    }
}

/// The number of `#N`.
///
/// ```
/// assert_eq!(hygiene::issue_ref("#12"), Some(12));
/// assert_eq!(hygiene::issue_ref("12"), None);
/// assert_eq!(hygiene::issue_ref("#"), None);
/// ```
pub fn issue_ref(word: &str) -> Option<u64> {
    let digits = word.strip_prefix('#')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The closing keywords of GitHub.
const KEYWORDS: &[&str] = &[
    "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved",
];

/// Each closing keyword of GitHub before an issue number, outside the
/// link lines. GitHub closes such an issue at the merge.
///
/// ```
/// let body = "Closes #12\n\nThis fixes #13, and closes: #14. It closes a gap.";
/// assert_eq!(hygiene::stray_keywords(body), ["fixes #13", "closes: #14"]);
/// ```
pub fn stray_keywords(body: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in body.lines().filter(|l| Link::parse(l).is_none()) {
        let words: Vec<&str> = line.split_whitespace().collect();
        for pair in words.windows(2) {
            let keyword = pair[0].trim_end_matches(':').to_lowercase();
            if KEYWORDS.contains(&keyword.as_str()) && names_issue(pair[1]) {
                let number = pair[1].trim_end_matches(|c: char| !c.is_ascii_digit());
                found.push(format!("{} {number}", pair[0]));
            }
        }
    }
    found
}

/// True for `#N` or `OWNER/REPO#N`, with punctuation after it.
fn names_issue(word: &str) -> bool {
    let word = word.trim_end_matches(|c: char| !c.is_ascii_digit());
    word.rfind('#')
        .is_some_and(|i| issue_ref(&word[i..]).is_some())
}

/// The trailers of a message: the lines `Key: value` of its last
/// paragraph. A last paragraph with a line of another form has none.
///
/// ```
/// let body = "Closes #7\n\nText.\n\nIssue: #7\nMilestone: Wave 3\n";
/// assert_eq!(hygiene::trailers(body), [
///     ("Issue".to_owned(), "#7".to_owned()),
///     ("Milestone".to_owned(), "Wave 3".to_owned()),
/// ]);
/// assert!(hygiene::trailers("Text.\n\nIssue: #7\nmore text").is_empty());
/// ```
pub fn trailers(text: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let end = lines
        .iter()
        .rposition(|l| !l.is_empty())
        .map_or(0, |i| i + 1);
    let start = lines[..end]
        .iter()
        .rposition(|l| l.is_empty())
        .map_or(0, |i| i + 1);
    let mut found = Vec::new();
    for line in &lines[start..end] {
        let Some((key, value)) = line.split_once(": ") else {
            return Vec::new();
        };
        let key_ok = !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if !key_ok || value.trim().is_empty() {
            return Vec::new();
        }
        found.push((key.to_owned(), value.trim().to_owned()));
    }
    found
}

/// The issue of a pull request: the issue of its first link line, or of
/// its `Issue:` trailer. `hygiene pr` reads this issue.
///
/// ```
/// use hygiene::PullRequest;
/// let pr = PullRequest::new("T", "Refs #9\n\nIssue: #9\nMilestone: Wave 3", None);
/// assert_eq!(hygiene::issue_number(&pr), Some(9));
/// assert_eq!(hygiene::issue_number(&PullRequest::new("T", "Text.", None)), None);
/// ```
pub fn issue_number(pr: &PullRequest) -> Option<u64> {
    let body = pr.body.replace("\r\n", "\n");
    body.lines()
        .find_map(Link::parse)
        .map(Link::issue)
        .or_else(|| {
            trailers(&body)
                .iter()
                .find(|(k, _)| k == "Issue")
                .and_then(|(_, v)| issue_ref(v))
        })
}

/// The number `N` at the end of a title `TEXT (#N)`.
fn title_number(title: &str) -> Option<u64> {
    let (_, tail) = title.trim_end().strip_suffix(')')?.rsplit_once("(#")?;
    issue_ref(&format!("#{tail}"))
}

/// The values of the trailer `key`.
fn values<'a>(trailers: &'a [(String, String)], key: &str) -> Vec<&'a str> {
    trailers
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .collect()
}

/// The issue of the one `Issue: #N` trailer. Adds an `issue-trailer`
/// error when the trailer is missing, when it is there more than one
/// time, or when its value is not `#N`.
fn issue_trailer(trailers: &[(String, String)], errors: &mut Vec<Error>) -> Option<u64> {
    match values(trailers, "Issue")[..] {
        [] => {
            errors.push(Error::new(
                "issue-trailer",
                "the message does not end with the trailer `Issue: #N`",
            ));
            None
        }
        [value] => {
            let n = issue_ref(value);
            if n.is_none() {
                errors.push(Error::new(
                    "issue-trailer",
                    format!("`Issue: {value}` does not name an issue as `#N`"),
                ));
            }
            n
        }
        ref many => {
            errors.push(Error::new(
                "issue-trailer",
                format!("the message has {} `Issue:` trailers; keep one", many.len()),
            ));
            None
        }
    }
}

/// The value of the one `Milestone:` trailer. Adds a
/// `milestone-trailer` error when the trailer is missing, or when it is
/// there more than one time.
fn milestone_trailer<'a>(
    trailers: &'a [(String, String)],
    errors: &mut Vec<Error>,
) -> Option<&'a str> {
    match values(trailers, "Milestone")[..] {
        [] => {
            errors.push(Error::new(
                "milestone-trailer",
                "the message does not end with the trailer `Milestone: M`",
            ));
            None
        }
        [value] => Some(value),
        ref many => {
            errors.push(Error::new(
                "milestone-trailer",
                format!(
                    "the message has {} `Milestone:` trailers; keep one",
                    many.len()
                ),
            ));
            None
        }
    }
}

/// Checks a pull request and its issue. `issue` is `None` when the pull
/// request names no issue. Returns each broken rule, in the order of the
/// table of rules.
///
/// ```
/// use hygiene::{Issue, PullRequest};
///
/// let pr = PullRequest::new("Fix it (#12)", "Fixes #12\n\nIssue: #12\n", Some("Wave 3"));
/// let issue = Issue::new(12, "CLOSED", Some("Wave 4"));
/// let rules: Vec<_> = hygiene::check_pr(&pr, Some(&issue)).iter().map(|e| e.rule).collect();
/// assert_eq!(rules, ["link", "keyword", "title", "milestone-trailer", "milestone", "issue-open"]);
/// ```
pub fn check_pr(pr: &PullRequest, issue: Option<&Issue>) -> Vec<Error> {
    let body = pr.body.replace("\r\n", "\n");
    let mut errors = Vec::new();

    let links: Vec<Link> = body.lines().filter_map(Link::parse).collect();
    let link = match links[..] {
        [] => {
            errors.push(Error::new(
                "link",
                "the body has no line `Closes #N` or `Refs #N`",
            ));
            None
        }
        [link] => Some(link),
        ref many => {
            errors.push(Error::new(
                "link",
                format!(
                    "the body has {} link lines; keep one: `Closes #N` in the last pull request of the issue, `Refs #N` in each other one",
                    many.len()
                ),
            ));
            None
        }
    };
    for keyword in stray_keywords(&body) {
        errors.push(Error::new(
            "keyword",
            format!("`{keyword}` closes an issue with no check; use the link line"),
        ));
    }
    if let Some(n) = title_number(&pr.title) {
        errors.push(Error::new(
            "title",
            format!("the title ends with `(#{n})`; GitHub adds the number of the pull request"),
        ));
    }

    let trailers = trailers(&body);
    let named = issue_trailer(&trailers, &mut errors);
    if let (Some(named), Some(link)) = (named, link)
        && named != link.issue()
    {
        errors.push(Error::new(
            "issue-trailer",
            format!(
                "`Issue: #{named}` names another issue than the link line (#{})",
                link.issue()
            ),
        ));
    }
    let wave = milestone_trailer(&trailers, &mut errors);
    let pr_milestone = pr.milestone.as_ref().map(|m| m.title.as_str());
    if let (Some(wave), Some(own)) = (wave, pr_milestone)
        && wave != own
    {
        errors.push(Error::new(
            "milestone-trailer",
            format!("`Milestone: {wave}` is not the milestone of the pull request ({own})"),
        ));
    }

    if pr_milestone.is_none() {
        errors.push(Error::new("milestone", "the pull request has no milestone"));
    }
    if let Some(issue) = issue {
        let n = issue.number;
        match (
            pr_milestone,
            issue.milestone.as_ref().map(|m| m.title.as_str()),
        ) {
            (_, None) => errors.push(Error::new(
                "milestone",
                format!("issue #{n} has no milestone"),
            )),
            (Some(own), Some(its)) if own != its => errors.push(Error::new(
                "milestone",
                format!("the pull request has milestone `{own}`, and issue #{n} has `{its}`"),
            )),
            _ => {}
        }
        if issue.state != "OPEN" {
            errors.push(Error::new("issue-open", format!("issue #{n} is closed")));
        }
    }
    errors
}

/// Checks the message of a commit on `main`. Returns each broken rule.
///
/// ```
/// let errors = hygiene::check_commit("Fix the tail (#48)\n\nIssue: #48\n");
/// let rules: Vec<_> = errors.iter().map(|e| e.rule).collect();
/// assert_eq!(rules, ["milestone-trailer", "commit-title"]);
/// ```
pub fn check_commit(message: &str) -> Vec<Error> {
    let message = message.replace("\r\n", "\n");
    let (title, body) = message.split_once('\n').unwrap_or((&message, ""));
    let mut errors = Vec::new();
    let trailers = trailers(body);
    let issue = issue_trailer(&trailers, &mut errors);
    milestone_trailer(&trailers, &mut errors);
    match title_number(title) {
        None => errors.push(Error::new(
            "commit-title",
            "the title does not end with `(#PR)`; GitHub adds it at the squash merge of a pull request",
        )),
        Some(n) if Some(n) == issue => errors.push(Error::new(
            "commit-title",
            format!("the title ends with issue #{n}, not with a pull request"),
        )),
        Some(_) => {}
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "Closes #77\n\nPause the riff.\n\nIssue: #77\nMilestone: Wave 3\n";

    fn pr(body: &str, milestone: Option<&str>) -> PullRequest {
        PullRequest::new("Pause and resume the riff", body, milestone)
    }

    fn open(milestone: Option<&str>) -> Issue {
        Issue::new(77, "OPEN", milestone)
    }

    /// The rules of the errors of a pull request with an open issue 77.
    fn rules(pr: &PullRequest, issue: &Issue) -> Vec<&'static str> {
        check_pr(pr, Some(issue)).iter().map(|e| e.rule).collect()
    }

    #[test]
    fn a_good_pull_request_passes() {
        assert_eq!(
            check_pr(&pr(GOOD, Some("Wave 3")), Some(&open(Some("Wave 3")))),
            []
        );
        let refs = GOOD.replace("Closes #77", "Refs #77");
        assert_eq!(
            check_pr(&pr(&refs, Some("Wave 3")), Some(&open(Some("Wave 3")))),
            []
        );
    }

    #[test]
    fn a_body_from_the_web_with_crlf_passes() {
        let body = GOOD.replace('\n', "\r\n");
        assert_eq!(
            check_pr(&pr(&body, Some("Wave 3")), Some(&open(Some("Wave 3")))),
            []
        );
    }

    #[test]
    fn other_trailers_may_follow() {
        let body = format!("{GOOD}Claude-Session: https://claude.ai/code/session_x\n");
        assert!(rules(&pr(&body, Some("Wave 3")), &open(Some("Wave 3"))).is_empty());
    }

    #[test]
    fn no_link_line_fails() {
        let body = GOOD.replace("Closes #77\n\n", "");
        let errors = check_pr(&pr(&body, Some("Wave 3")), Some(&open(Some("Wave 3"))));
        assert_eq!(
            errors,
            [Error::new(
                "link",
                "the body has no line `Closes #N` or `Refs #N`"
            )]
        );
    }

    #[test]
    fn two_link_lines_fail() {
        let body = GOOD.replace("Closes #77\n", "Closes #77\nRefs #77\n");
        let errors = check_pr(&pr(&body, Some("Wave 3")), Some(&open(Some("Wave 3"))));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].rule, "link");
        assert!(
            errors[0].text.starts_with("the body has 2 link lines"),
            "{}",
            errors[0]
        );
    }

    #[test]
    fn another_closing_keyword_fails() {
        let body = GOOD.replace("Pause the riff.", "It also fixes #12.");
        let errors = check_pr(&pr(&body, Some("Wave 3")), Some(&open(Some("Wave 3"))));
        assert_eq!(
            errors,
            [Error::new(
                "keyword",
                "`fixes #12` closes an issue with no check; use the link line"
            )]
        );
    }

    #[test]
    fn a_title_with_a_number_fails() {
        let mut with_number = pr(GOOD, Some("Wave 3"));
        with_number.title = "Pause and resume the riff (#77)".into();
        assert_eq!(rules(&with_number, &open(Some("Wave 3"))), ["title"]);
    }

    #[test]
    fn no_issue_trailer_fails() {
        let body = GOOD.replace("Issue: #77\n", "");
        let errors = check_pr(&pr(&body, Some("Wave 3")), Some(&open(Some("Wave 3"))));
        assert_eq!(
            errors,
            [Error::new(
                "issue-trailer",
                "the message does not end with the trailer `Issue: #N`"
            )]
        );
    }

    #[test]
    fn an_issue_trailer_of_another_issue_fails() {
        let body = GOOD.replace("Issue: #77", "Issue: #78");
        let errors = check_pr(&pr(&body, Some("Wave 3")), Some(&open(Some("Wave 3"))));
        assert_eq!(
            errors,
            [Error::new(
                "issue-trailer",
                "`Issue: #78` names another issue than the link line (#77)"
            )]
        );
    }

    #[test]
    fn an_issue_trailer_that_is_not_a_number_fails() {
        let body = GOOD.replace("Issue: #77", "Issue: 77");
        assert_eq!(
            rules(&pr(&body, Some("Wave 3")), &open(Some("Wave 3"))),
            ["issue-trailer"]
        );
    }

    #[test]
    fn two_issue_trailers_fail() {
        let body = GOOD.replace("Issue: #77\n", "Issue: #77\nIssue: #77\n");
        assert_eq!(
            rules(&pr(&body, Some("Wave 3")), &open(Some("Wave 3"))),
            ["issue-trailer"]
        );
    }

    #[test]
    fn trailers_in_the_middle_do_not_count() {
        let body = format!("{GOOD}\nA last paragraph of text.\n");
        assert_eq!(
            rules(&pr(&body, Some("Wave 3")), &open(Some("Wave 3"))),
            ["issue-trailer", "milestone-trailer"]
        );
    }

    #[test]
    fn no_milestone_trailer_fails() {
        let body = GOOD.replace("Milestone: Wave 3\n", "");
        let errors = check_pr(&pr(&body, Some("Wave 3")), Some(&open(Some("Wave 3"))));
        assert_eq!(
            errors,
            [Error::new(
                "milestone-trailer",
                "the message does not end with the trailer `Milestone: M`"
            )]
        );
    }

    #[test]
    fn a_milestone_trailer_of_another_wave_fails() {
        let body = GOOD.replace("Wave 3", "Wave 4");
        let errors = check_pr(&pr(&body, Some("Wave 3")), Some(&open(Some("Wave 3"))));
        assert_eq!(
            errors,
            [Error::new(
                "milestone-trailer",
                "`Milestone: Wave 4` is not the milestone of the pull request (Wave 3)"
            )]
        );
    }

    #[test]
    fn a_milestone_of_the_pull_request_that_differs_from_the_issue_fails() {
        let errors = check_pr(&pr(GOOD, Some("Wave 3")), Some(&open(Some("Wave 4"))));
        assert_eq!(
            errors,
            [Error::new(
                "milestone",
                "the pull request has milestone `Wave 3`, and issue #77 has `Wave 4`"
            )]
        );
    }

    #[test]
    fn no_milestone_fails() {
        assert_eq!(rules(&pr(GOOD, None), &open(Some("Wave 3"))), ["milestone"]);
        assert_eq!(rules(&pr(GOOD, Some("Wave 3")), &open(None)), ["milestone"]);
    }

    #[test]
    fn a_closed_issue_fails() {
        let closed = Issue::new(77, "CLOSED", Some("Wave 3"));
        let errors = check_pr(&pr(GOOD, Some("Wave 3")), Some(&closed));
        assert_eq!(errors, [Error::new("issue-open", "issue #77 is closed")]);
    }

    #[test]
    fn with_no_issue_only_the_form_is_checked() {
        let errors = check_pr(&pr("Text only.", None), None);
        let rules: Vec<_> = errors.iter().map(|e| e.rule).collect();
        assert_eq!(
            rules,
            ["link", "issue-trailer", "milestone-trailer", "milestone"]
        );
    }

    #[test]
    fn a_good_commit_passes() {
        let message = format!("Pause and resume the riff (#90)\n\n{GOOD}");
        assert_eq!(check_commit(&message), []);
    }

    #[test]
    fn a_commit_with_no_number_or_trailers_fails() {
        let errors = check_commit("Pause and resume the riff\n\nText.\n");
        let rules: Vec<_> = errors.iter().map(|e| e.rule).collect();
        assert_eq!(
            rules,
            ["issue-trailer", "milestone-trailer", "commit-title"]
        );
    }

    #[test]
    fn a_commit_of_the_old_flow_fails() {
        let errors =
            check_commit("Pause and resume the riff (#77)\n\nIssue: #77\nMilestone: Wave 3\n");
        assert_eq!(
            errors,
            [Error::new(
                "commit-title",
                "the title ends with issue #77, not with a pull request"
            )]
        );
    }

    #[test]
    fn a_commit_with_only_a_title_has_no_trailers() {
        let rules: Vec<_> = check_commit("docs: fix a word (#90)")
            .iter()
            .map(|e| e.rule)
            .collect();
        assert_eq!(rules, ["issue-trailer", "milestone-trailer"]);
    }
}
