//! The steps that each item repeats on GitHub: open a pull request, wait
//! for its merge, and report a verify.
//!
//! # Design
//!
//! Each step is one plain `riff` command, a thin wrapper around the `gh`
//! of the machine. It needs no new service. A session runs one command
//! for one step, with no shell loop, so the worktree guard of Claude
//! Code accepts it, and each session does the step in the same way.
//!
//! | Command | `gh` calls |
//! |---|---|
//! | `riff pr open` | `issue view`, `pr create`, `pr merge --auto --squash` |
//! | `riff pr wait N` | `pr view`, `pr checks --required`, again each `--every` seconds. After the merge: the total of the tokens of the issue ([`crate::usage`]) |
//! | `riff verify pass\|fail N` | `pr view`, `pr comment`, `api …/statuses/COMMIT` |
//!
//! `riff pr open` takes the issue from the claim of the session, and
//! writes the body in the form of the hygiene check: the link line and
//! the trailers `Issue:` and `Milestone:` ([`body`]). It adds each of
//! them only when the summary does not have it, and refuses a summary
//! with a line for another issue or another milestone
//! (01M3W2627GYXR8CFW76KB6CB9W). It checks the pull request with
//! [`hygiene::check_pr`] before it opens it, and turns on auto-merge
//! with a squash at once (01M3NB6FTGPD0S5JTXXXNGNNDT).
//!
//! `riff pr wait N` looks at the pull request until it is merged, and
//! prints the merge commit. It stops with an error when the pull request
//! closes unmerged or a required check fails ([`Wait`],
//! 01M3NB6FWMGBQ9VTY6RCBPKBHK).
//!
//! `riff verify pass|fail N` puts the result on the pull request as a
//! comment that names the head commit, sets the status `riff/verify` of
//! that commit with the URL of the comment, and posts the result to the
//! holder of the issue of the `Issue:` trailer
//! (01M3NB6FYXXKX80VHEVA5CV6RY). A verify counts only for its commit: it
//! reports nothing when the head of the pull request is not the commit
//! that the verifier tested, `HEAD` of its worktree or `--commit`
//! ([`same_commit`]).
//!
//! The author of a pull request releases its item at the verify
//! request. So the issue can have no holder. Then the result wakes the
//! lead of the user of the verifier in the repository ([`result_to`],
//! 01M3Z9N70J4H79VJN4ZKKH3G6S): a result is never silent.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as author
//!     participant V as verifier
//!     participant G as gh
//!     participant E as riff-server
//!     A->>G: riff pr open: pr create, pr merge --auto --squash
//!     V->>G: riff verify pass 40: pr comment, status riff/verify
//!     V->>E: post to [claim=issue-12], and to the lead when no session holds issue-12
//!     A->>G: riff pr wait 40: pr view until MERGED
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::SessionInfo;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::text;

/// The context of the verify status of a commit.
pub const VERIFY_CONTEXT: &str = "riff/verify";

/// The `gh` of the machine.
pub struct Gh {
    program: PathBuf,
}

impl Default for Gh {
    fn default() -> Self {
        Self {
            program: "gh".into(),
        }
    }
}

impl Gh {
    /// The `gh` at `program`, for tests.
    pub fn at(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// Runs `gh ARGS` with `input` on stdin, and returns its stdout. It
    /// fails with the stderr of `gh` when `gh` fails.
    pub fn run(&self, args: &[&str], input: Option<&str>) -> Result<String> {
        let (ok, stdout, stderr) = self.output(args, input)?;
        if !ok {
            bail!("gh {}: {}", args.join(" "), stderr.trim());
        }
        Ok(stdout)
    }

    /// Runs `gh ARGS` and reads its JSON output.
    pub fn json<T: DeserializeOwned>(&self, args: &[&str]) -> Result<T> {
        let out = self.run(args, None)?;
        serde_json::from_str(&out).with_context(|| format!("gh {}", args.join(" ")))
    }

    /// Runs `gh ARGS`: whether it exits with status 0, its stdout and its
    /// stderr.
    fn output(&self, args: &[&str], input: Option<&str>) -> Result<(bool, String, String)> {
        let mut child = Command::new(&self.program)
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("cannot run {}", self.program.display()))?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            stdin.write_all(input.as_bytes())?;
        }
        let out = child.wait_with_output()?;
        Ok((
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ))
    }
}

/// The body of a pull request for the issue of `link` in the form of the
/// hygiene check: the link line, the summary, and the trailers.
///
/// It adds the link line and each trailer only when `summary` does not
/// have it. It refuses a link line, or a trailer `Issue:` or
/// `Milestone:`, of `summary` that is not the one of the pull request,
/// and names the line (01M3W2627GYXR8CFW76KB6CB9W).
///
/// ```
/// use hygiene::{Issue, Link, PullRequest};
///
/// let body = riff::pr::body(Link::Closes(12), "Show the wave.\n", "Wave 3").unwrap();
/// assert_eq!(body, "Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n");
/// let pr = PullRequest::new("Show the wave", &body, Some("Wave 3"));
/// assert!(hygiene::check_pr(&pr, Some(&Issue::new(12, "OPEN", Some("Wave 3")))).is_empty());
///
/// let body = riff::pr::body(Link::Refs(12), "", "Wave 3").unwrap();
/// assert_eq!(body, "Refs #12\n\nIssue: #12\nMilestone: Wave 3\n");
///
/// // A summary that is a full body stays as it is.
/// let full = "Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n";
/// assert_eq!(riff::pr::body(Link::Closes(12), full, "Wave 3").unwrap(), full);
///
/// // A line for another issue is refused.
/// let other = riff::pr::body(Link::Closes(12), "Text.\n\nIssue: #9\n", "Wave 3");
/// assert_eq!(
///     other.unwrap_err().to_string(),
///     "the body has the line `Issue: #9`, but this pull request needs `Issue: #12`. \
///      Change the line, or remove it."
/// );
/// ```
pub fn body(link: hygiene::Link, summary: &str, milestone: &str) -> Result<String> {
    let n = link.issue();
    let line = match link {
        hygiene::Link::Closes(_) => format!("Closes #{n}"),
        hygiene::Link::Refs(_) => format!("Refs #{n}"),
    };
    let summary = summary.replace("\r\n", "\n");
    let summary = summary.trim();

    let mut linked = false;
    for found in summary
        .lines()
        .filter(|l| hygiene::Link::parse(l).is_some())
    {
        if hygiene::Link::parse(found) != Some(link) {
            let found = found.trim();
            let refs = match link {
                hygiene::Link::Closes(_) => "Give --refs for a line `Refs #N`. ",
                hygiene::Link::Refs(_) => "Give no --refs for a line `Closes #N`. ",
            };
            bail!(
                "the body has the line `{found}`, but this pull request needs `{line}`. \
                 {refs}Change the line, or remove it."
            );
        }
        linked = true;
    }

    let trailers = hygiene::trailers(summary);
    let mut missing = String::new();
    for (key, want) in [
        ("Issue", format!("#{n}")),
        ("Milestone", milestone.to_owned()),
    ] {
        let mut values = trailers.iter().filter(|(k, _)| k == key).map(|(_, v)| v);
        if let Some(value) = values.find(|value| **value != want) {
            bail!(
                "the body has the line `{key}: {value}`, but this pull request needs \
                 `{key}: {want}`. Change the line, or remove it."
            );
        }
        if !trailers.iter().any(|(k, _)| k == key) {
            missing.push_str(&format!("{key}: {want}\n"));
        }
    }

    let mut body = String::new();
    if !linked {
        body.push_str(&format!("{line}\n\n"));
    }
    if !summary.is_empty() {
        body.push_str(summary);
        body.push('\n');
        // A new trailer joins the trailers of the summary, in its last
        // paragraph. With none, the trailers are a new paragraph.
        if trailers.is_empty() && !missing.is_empty() {
            body.push('\n');
        }
    }
    body.push_str(&missing);
    Ok(body)
}

/// The issue of the claims of a session: the one claim `issue-N`.
///
/// ```
/// let claims = ["verify-issue-3".to_owned(), "issue-12".to_owned()];
/// assert_eq!(riff::pr::claimed_issue(&claims).unwrap(), 12);
/// assert!(riff::pr::claimed_issue(&[]).is_err());
/// let two = ["issue-1".to_owned(), "issue-2".to_owned()];
/// assert!(riff::pr::claimed_issue(&two).is_err());
/// ```
pub fn claimed_issue(claims: &[String]) -> Result<u64> {
    let issues: Vec<u64> = claims
        .iter()
        .filter_map(|c| c.strip_prefix("issue-")?.parse().ok())
        .collect();
    match issues[..] {
        [n] => Ok(n),
        [] => bail!("this session holds no claim issue-N. Claim the issue, or give --issue N."),
        _ => bail!("this session holds more than one issue. Give --issue N."),
    }
}

/// Opens the pull request of the current branch for issue `issue`, and
/// turns on auto-merge with a squash (01M3NB6FTGPD0S5JTXXXNGNNDT).
/// Returns its number and its URL.
pub fn open(gh: &Gh, title: &str, summary: &str, issue: u64, refs: bool) -> Result<(u64, String)> {
    let n = issue.to_string();
    let found: hygiene::Issue =
        gh.json(&["issue", "view", &n, "--json", "number,state,milestone"])?;
    let Some(milestone) = found.milestone.as_ref().map(|m| m.title.clone()) else {
        bail!("issue #{issue} has no milestone. Ask the lead to put it in a wave.");
    };
    let link = if refs {
        hygiene::Link::Refs(issue)
    } else {
        hygiene::Link::Closes(issue)
    };
    let body = body(link, summary, &milestone)?;
    let errors = hygiene::check_pr(
        &hygiene::PullRequest::new(title, &body, Some(&milestone)),
        Some(&found),
    );
    if !errors.is_empty() {
        let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
        bail!(
            "the pull request breaks issue hygiene:\n{}",
            lines.join("\n")
        );
    }
    let args = [
        "pr",
        "create",
        "--title",
        title,
        "--milestone",
        &milestone,
        "--body-file",
        "-",
    ];
    let url = gh.run(&args, Some(&body))?.trim().to_owned();
    let number = url
        .rsplit('/')
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or_else(|| anyhow!("gh pr create printed no pull request URL: {url}"))?;
    let pr = number.to_string();
    gh.run(&["pr", "merge", &pr, "--auto", "--squash"], None)
        .with_context(|| {
            format!("pull request #{number} is open, but auto-merge is off. Run: gh pr merge {number} --auto --squash")
        })?;
    Ok((number, url))
}

/// Where a pull request is on its way to the merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wait {
    /// Merged, with the merge commit.
    Merged(String),
    /// Closed with no merge.
    Closed,
    /// Each required check that failed.
    Failed(Vec<String>),
    /// Open, and no required check failed.
    Open,
}

/// A pull request, as `gh pr view --json state,mergeCommit` gives it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrState {
    /// `OPEN`, `CLOSED` or `MERGED`.
    pub state: String,
    /// The merge commit, when it is merged.
    pub merge_commit: Option<Commit>,
}

/// A commit, as `gh --json` gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct Commit {
    /// The full hash.
    pub oid: String,
}

/// A check, as `gh pr checks --json name,bucket` gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct Check {
    /// The name, for example `Gate`.
    pub name: String,
    /// `pass`, `fail`, `pending`, `skipping` or `cancel`.
    pub bucket: String,
}

/// Where the pull request is, from its state and its required checks.
///
/// ```
/// use riff::pr::{Check, Commit, PrState, Wait, wait_state};
///
/// let open = PrState { state: "OPEN".into(), merge_commit: None };
/// let check = |name: &str, bucket: &str| Check { name: name.into(), bucket: bucket.into() };
/// assert_eq!(wait_state(&open, &[check("Gate", "pending")]), Wait::Open);
/// assert_eq!(
///     wait_state(&open, &[check("Gate", "pass"), check("riff/verify", "fail")]),
///     Wait::Failed(vec!["riff/verify".into()])
/// );
/// let merged = PrState { state: "MERGED".into(), merge_commit: Some(Commit { oid: "1a2b".into() }) };
/// assert_eq!(wait_state(&merged, &[]), Wait::Merged("1a2b".into()));
/// let closed = PrState { state: "CLOSED".into(), merge_commit: None };
/// assert_eq!(wait_state(&closed, &[]), Wait::Closed);
/// ```
pub fn wait_state(pr: &PrState, checks: &[Check]) -> Wait {
    match (pr.state.as_str(), &pr.merge_commit) {
        ("MERGED", Some(commit)) => return Wait::Merged(commit.oid.clone()),
        ("CLOSED", _) => return Wait::Closed,
        _ => {}
    }
    let failed: Vec<String> = checks
        .iter()
        .filter(|c| matches!(c.bucket.as_str(), "fail" | "cancel"))
        .map(|c| c.name.clone())
        .collect();
    if failed.is_empty() {
        Wait::Open
    } else {
        Wait::Failed(failed)
    }
}

/// Looks at pull request `number` each `every` until it is merged
/// (01M3NB6FWMGBQ9VTY6RCBPKBHK). Returns the merge commit. Fails when
/// it closes unmerged or a required check fails.
///
/// A look of `gh` can fail for a short time, for example while the
/// network is away. After one good look, a failed look does not end the
/// wait (01M3Z8GG5EGEYAVEXG0HS46ACT): it prints
/// [`text::pr_look_failed`] on stderr, one time until the next good
/// look, and looks again after `every`. When the first look fails, the
/// wait ends with its error: `gh` cannot see the pull request.
pub fn wait(gh: &Gh, number: u64, every: Duration) -> Result<String> {
    let n = number.to_string();
    let look = || {
        let pr: PrState = gh.json(&["pr", "view", &n, "--json", "state,mergeCommit"])?;
        // With no required check, `gh pr checks` prints no JSON.
        let (_, out, _) = gh.output(
            &["pr", "checks", &n, "--required", "--json", "name,bucket"],
            None,
        )?;
        let checks: Vec<Check> = serde_json::from_str(&out).unwrap_or_default();
        anyhow::Ok(wait_state(&pr, &checks))
    };
    let mut looked = false;
    let mut told = false;
    loop {
        match look() {
            Ok(Wait::Merged(commit)) => return Ok(commit),
            Ok(Wait::Closed) => bail!("pull request #{number} is closed, and not merged"),
            Ok(Wait::Failed(names)) => bail!(
                "pull request #{number} waits no more: a required check failed: {}",
                names.join(", ")
            ),
            Ok(Wait::Open) => {
                looked = true;
                told = false;
            }
            Err(e) if looked => {
                if !told {
                    let error = format!("{e:#}");
                    eprintln!("{}", text::pr_look_failed(number, &error, every.as_secs()));
                    told = true;
                }
            }
            Err(e) => return Err(e),
        }
        std::thread::sleep(every);
    }
}

/// The result of a verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Each criterion passed.
    Pass,
    /// One or more criteria failed.
    Fail,
}

impl Verdict {
    /// `PASS` or `FAIL`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
        }
    }

    /// The state of the commit status: `success` or `failure`.
    pub fn state(self) -> &'static str {
        match self {
            Self::Pass => "success",
            Self::Fail => "failure",
        }
    }
}

/// A pull request, as `gh pr view --json headRefOid,body` gives it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Head {
    /// The head commit.
    pub head_ref_oid: String,
    /// The body, with the `Issue:` trailer.
    pub body: String,
}

/// The issue of a body: its `Issue: #N` trailer.
///
/// ```
/// assert_eq!(riff::pr::trailer_issue("Closes #12\n\nIssue: #12\nMilestone: Wave 3\n"), Some(12));
/// assert_eq!(riff::pr::trailer_issue("Closes #12\n"), None);
/// ```
pub fn trailer_issue(body: &str) -> Option<u64> {
    hygiene::trailers(&body.replace("\r\n", "\n"))
        .iter()
        .find(|(key, _)| key == "Issue")
        .and_then(|(_, value)| hygiene::issue_ref(value))
}

/// The issue of pull request `number`: the `Issue:` trailer of its body.
pub fn issue_of(gh: &Gh, number: u64) -> Result<u64> {
    #[derive(Deserialize)]
    struct Body {
        body: String,
    }
    let pr: Body = gh.json(&["pr", "view", &number.to_string(), "--json", "body"])?;
    trailer_issue(&pr.body)
        .with_context(|| format!("pull request #{number} has no trailer `Issue: #N`"))
}

/// The comment of a verify on the pull request: it names the commit.
///
/// ```
/// use riff::pr::{Verdict, comment};
/// assert_eq!(
///     comment(Verdict::Pass, 12, "1a2b3c4d", "All good.\n"),
///     "verify result: PASS for issue-12, commit 1a2b3c4d.\n\nAll good.\n"
/// );
/// ```
pub fn comment(verdict: Verdict, issue: u64, commit: &str, result: &str) -> String {
    format!(
        "verify result: {} for issue-{issue}, commit {commit}.\n\n{}\n",
        verdict.word(),
        result.trim_end()
    )
}

/// A verify that is on GitHub: the issue, the head commit and the URL of
/// the comment.
#[derive(Debug, Clone)]
pub struct Reported {
    /// The issue of the `Issue:` trailer.
    pub issue: u64,
    /// The head commit that the status is on.
    pub commit: String,
    /// The URL of the comment.
    pub url: String,
}

/// The sessions that the result of a verify of `issue` wakes: the
/// holder of the issue. When no live session of `sessions` holds the
/// issue in the repository of the verifier `me`, also the lead of the
/// user of `me` there (01M3Z9N70J4H79VJN4ZKKH3G6S).
///
/// ```
/// use riff::pr::result_to;
/// use riff_core::wire::SessionInfo;
///
/// let info = |uri: &str, live| SessionInfo {
///     uri: uri.parse().unwrap(),
///     live,
///     idle_secs: 0,
///     status: None,
///     worker: true,
///     stopping: false,
///     claims_secs: 0,
///     must_clear: false,
///     fresh_secs: None,
///     state: None,
/// };
/// let to = |sessions: &[SessionInfo]| -> Vec<String> {
///     let me = "riff://mike@pangolin/o/r?session=v1&claim=verify-issue-12".parse().unwrap();
///     result_to(12, &me, sessions).iter().map(ToString::to_string).collect()
/// };
/// let holder = info("riff://mike@thelio/o/r?session=a1&claim=issue-12", true);
/// assert_eq!(to(&[holder]), ["claim=issue-12"]);
/// // The author released the item, its session is gone, or the claim
/// // is in another repository: the lead wakes.
/// let lead = ["claim=issue-12", "user=mike,repo=o/r,lead=true"];
/// assert_eq!(to(&[]), lead);
/// assert_eq!(to(&[info("riff://mike@thelio/o/r?session=a1&claim=issue-12", false)]), lead);
/// assert_eq!(to(&[info("riff://mike@thelio/o/s?session=a1&claim=issue-12", true)]), lead);
/// ```
pub fn result_to(issue: u64, me: &SessionUri, sessions: &[SessionInfo]) -> Vec<Selector> {
    let item = format!("issue-{issue}");
    let repo = me.place().repo_text();
    let held = sessions
        .iter()
        .any(|s| s.live && s.uri.place().repo_text() == repo && s.uri.claims().contains(&item));
    let mut to = vec![Selector {
        claim: Some(item),
        ..Selector::default()
    }];
    if !held {
        to.push(Selector::lead(me.who().user(), &repo));
    }
    to
}

/// The commit `HEAD` of the git worktree `dir`: the commit that the
/// verifier tested.
pub fn head_here(dir: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(dir)
        .output()
        .context("cannot run git")?;
    if !out.status.success() {
        bail!(
            "cannot read HEAD of {}: give the commit that you tested with --commit SHA",
            dir.display()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Checks that the verifier tested the head commit of the pull request:
/// a verify counts only for its commit. `tested` is a full hash, or its
/// first 7 or more characters.
///
/// ```
/// use riff::pr::same_commit;
/// assert!(same_commit("1a2b3c4d5e6f", "1a2b3c4d5e6f").is_ok());
/// assert!(same_commit("1a2b3c4d5e6f", "1a2b3c4").is_ok());
/// assert!(same_commit("1a2b3c4d5e6f", "1a2b").is_err(), "too short");
/// let moved = same_commit("9f8e7d6c5b4a", "1a2b3c4d5e6f").unwrap_err().to_string();
/// assert!(moved.starts_with("you tested 1a2b3c4d5e6f, but the head of the pull request is 9f8e7d6c5b4a"));
/// ```
pub fn same_commit(head: &str, tested: &str) -> Result<()> {
    if tested.len() >= 7 && head.starts_with(tested) {
        return Ok(());
    }
    bail!(
        "you tested {tested}, but the head of the pull request is {head}. A verify counts only \
         for its commit: nothing is reported. Test the head commit, or tell the author."
    )
}

/// Puts the result on pull request `number` as a comment, and sets the
/// status `riff/verify` of its head commit in `repo`
/// (01M3NB6FYXXKX80VHEVA5CV6RY). It reports nothing when the head is
/// not the commit `tested` ([`same_commit`]). The caller posts it to the
/// holder of the issue.
pub fn report(
    gh: &Gh,
    repo: &str,
    number: u64,
    tested: &str,
    verdict: Verdict,
    result: &str,
) -> Result<Reported> {
    let n = number.to_string();
    let head: Head = gh.json(&["pr", "view", &n, "--json", "headRefOid,body"])?;
    same_commit(&head.head_ref_oid, tested)?;
    let Some(issue) = trailer_issue(&head.body) else {
        bail!("pull request #{number} has no trailer `Issue: #N`");
    };
    let commit = head.head_ref_oid;
    let text = comment(verdict, issue, &commit, result);
    let url = gh
        .run(&["pr", "comment", &n, "--body-file", "-"], Some(&text))?
        .trim()
        .to_owned();
    let path = format!("repos/{repo}/statuses/{commit}");
    let state = format!("state={}", verdict.state());
    let description = format!("description={}: verify-issue-{issue}", verdict.word());
    let target = format!("target_url={url}");
    gh.run(
        &[
            "api",
            &path,
            "-f",
            &state,
            "-f",
            &format!("context={VERIFY_CONTEXT}"),
            "-f",
            &description,
            "-f",
            &target,
        ],
        None,
    )?;
    Ok(Reported { issue, commit, url })
}

#[cfg(test)]
mod tests {
    use hygiene::Link;

    use super::body;

    const FULL: &str = "Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n";

    fn refused(link: Link, summary: &str) -> String {
        body(link, summary, "Wave 3").unwrap_err().to_string()
    }

    #[test]
    fn a_body_with_the_link_line_and_the_trailers_has_each_one_time() {
        for summary in [
            FULL.to_owned(),
            FULL.replace('\n', "\r\n"),
            format!("\n{FULL}\n\n"),
        ] {
            assert_eq!(body(Link::Closes(12), &summary, "Wave 3").unwrap(), FULL);
        }
    }

    #[test]
    fn a_body_gets_only_the_lines_that_it_does_not_have() {
        for part in [
            "Show the wave.\n",
            "Closes #12\n\nShow the wave.\n",
            "Show the wave.\n\nIssue: #12\nMilestone: Wave 3\n",
            "Show the wave.\n\nIssue: #12\n",
            "Closes #12\n\nShow the wave.\n\nIssue: #12\n",
        ] {
            assert_eq!(
                body(Link::Closes(12), part, "Wave 3").unwrap(),
                FULL,
                "{part}"
            );
        }
        assert_eq!(
            body(
                Link::Closes(12),
                "Show the wave.\n\nMilestone: Wave 3\n",
                "Wave 3"
            )
            .unwrap(),
            "Closes #12\n\nShow the wave.\n\nMilestone: Wave 3\nIssue: #12\n"
        );
        assert_eq!(
            body(Link::Refs(12), "Issue: #12\nMilestone: Wave 3\n", "Wave 3").unwrap(),
            "Refs #12\n\nIssue: #12\nMilestone: Wave 3\n"
        );
    }

    #[test]
    fn a_line_that_is_not_of_the_pull_request_is_refused_by_name() {
        assert_eq!(
            refused(Link::Closes(12), &FULL.replace("Issue: #12", "Issue: #9")),
            "the body has the line `Issue: #9`, but this pull request needs `Issue: #12`. \
             Change the line, or remove it."
        );
        assert_eq!(
            refused(Link::Closes(12), &FULL.replace("Wave 3", "Wave 4")),
            "the body has the line `Milestone: Wave 4`, but this pull request needs \
             `Milestone: Wave 3`. Change the line, or remove it."
        );
        assert_eq!(
            refused(Link::Closes(12), &FULL.replace("Closes #12", "Closes #9")),
            "the body has the line `Closes #9`, but this pull request needs `Closes #12`. \
             Give --refs for a line `Refs #N`. Change the line, or remove it."
        );
        assert_eq!(
            refused(Link::Closes(12), &FULL.replace("Closes #12", "Refs #12")),
            "the body has the line `Refs #12`, but this pull request needs `Closes #12`. \
             Give --refs for a line `Refs #N`. Change the line, or remove it."
        );
        assert_eq!(
            refused(Link::Refs(12), FULL),
            "the body has the line `Closes #12`, but this pull request needs `Refs #12`. \
             Give no --refs for a line `Closes #N`. Change the line, or remove it."
        );
        assert!(refused(Link::Closes(12), "Text.\n\nIssue: 12\n").contains("`Issue: 12`"));
    }

    /// The same line two times stays: the hygiene check then refuses
    /// the pull request.
    #[test]
    fn a_line_that_the_body_has_two_times_stays() {
        let twice = "Closes #12\nCloses #12\n\nIssue: #12\nIssue: #12\nMilestone: Wave 3\n";
        assert_eq!(body(Link::Closes(12), twice, "Wave 3").unwrap(), twice);
    }
}
