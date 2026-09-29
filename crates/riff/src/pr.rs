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
//! | `riff pr wait N` | `pr view`, `pr checks --required`, again each `--every` seconds |
//! | `riff verify pass\|fail N` | `pr view`, `pr comment`, `api …/statuses/COMMIT` |
//!
//! `riff pr open` takes the issue from the claim of the session, and
//! writes the body in the form of the hygiene check: the link line and
//! the trailers `Issue:` and `Milestone:` ([`body`]). It checks the pull
//! request with [`hygiene::check_pr`] before it opens it, and turns on
//! auto-merge with a squash at once (01M3NB6FTGPD0S5JTXXXNGNNDT).
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
//! ```mermaid
//! sequenceDiagram
//!     participant A as author
//!     participant V as verifier
//!     participant G as gh
//!     participant E as riff-server
//!     A->>G: riff pr open: pr create, pr merge --auto --squash
//!     V->>G: riff verify pass 40: pr comment, status riff/verify
//!     V->>E: post to [claim=issue-12]
//!     A->>G: riff pr wait 40: pr view until MERGED
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;

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

/// The body of a pull request for issue `issue` in the form of the
/// hygiene check: the link line, the summary, and the trailers.
///
/// ```
/// use hygiene::{Issue, Link, PullRequest};
///
/// let body = riff::pr::body(Link::Closes(12), "Show the wave.\n", "Wave 3");
/// assert_eq!(body, "Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n");
/// let pr = PullRequest::new("Show the wave", &body, Some("Wave 3"));
/// assert!(hygiene::check_pr(&pr, Some(&Issue::new(12, "OPEN", Some("Wave 3")))).is_empty());
///
/// let body = riff::pr::body(Link::Refs(12), "", "Wave 3");
/// assert_eq!(body, "Refs #12\n\nIssue: #12\nMilestone: Wave 3\n");
/// ```
pub fn body(link: hygiene::Link, summary: &str, milestone: &str) -> String {
    let (word, n) = match link {
        hygiene::Link::Closes(n) => ("Closes", n),
        hygiene::Link::Refs(n) => ("Refs", n),
    };
    let summary = summary.trim();
    let summary = if summary.is_empty() {
        String::new()
    } else {
        format!("{summary}\n\n")
    };
    format!("{word} #{n}\n\n{summary}Issue: #{n}\nMilestone: {milestone}\n")
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
    let body = body(link, summary, &milestone);
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
pub fn wait(gh: &Gh, number: u64, every: Duration) -> Result<String> {
    let n = number.to_string();
    loop {
        let pr: PrState = gh.json(&["pr", "view", &n, "--json", "state,mergeCommit"])?;
        // With no required check, `gh pr checks` prints no JSON.
        let (_, out, _) = gh.output(
            &["pr", "checks", &n, "--required", "--json", "name,bucket"],
            None,
        )?;
        let checks: Vec<Check> = serde_json::from_str(&out).unwrap_or_default();
        match wait_state(&pr, &checks) {
            Wait::Merged(commit) => return Ok(commit),
            Wait::Closed => bail!("pull request #{number} is closed, and not merged"),
            Wait::Failed(names) => bail!(
                "pull request #{number} waits no more: a required check failed: {}",
                names.join(", ")
            ),
            Wait::Open => std::thread::sleep(every),
        }
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
