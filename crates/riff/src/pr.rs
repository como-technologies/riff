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
//! | `riff verify pass\|fail N` | `pr view`, `api …/rules/branches/BRANCH` and `api …/commits/COMMIT/check-runs` (a pass), `pr comment`, `api …/statuses/COMMIT` |
//!
//! `riff pr open` takes the issue from the claim of the session, and
//! writes the body in the form of the hygiene check: the link line and
//! the trailers `Issue:` and `Milestone:` ([`body`]). The trailer
//! `Milestone:` names a wave by its number, `Wave N` with no name
//! ([`hygiene::milestone_value`], 01M518A07QF1YACBYT91M0DD1R). It adds
//! each of them only when the summary does not have it, and refuses a
//! summary with a line for another issue or another wave
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
//! The full check of a commit is its one full test run
//! (01M49HAZ5BZYGAR9PGC089RM3F). In the repository of riff it is the job
//! `Gate`. In another repository it is each check that the ruleset of the
//! default branch requires, but `riff/verify`: `Gate` when it requires
//! none ([`repo_setup::full_check`], 01M5QYDE41FVFX26EC8K5QGYH4). The
//! verifier does not run the tests again: it reviews. So
//! `riff verify pass` reports nothing while the full check of the head
//! commit has no success ([`gate_of`], 01M49HAZ7P3JMWNCG1SWCMAXQP). A fail
//! needs no Gate.
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

use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::SessionInfo;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use zeroize::Zeroizing;

use crate::repo_setup;
use crate::text;

/// The status code in the first line of a reply that `gh api --include`
/// printed, for example `502` of `HTTP/2.0 502 Bad Gateway`.
pub fn http_status(reply: &str) -> Option<&str> {
    let line = reply.lines().next()?;
    let code = line.strip_prefix("HTTP/")?.split_whitespace().nth(1)?;
    (code.len() == 3 && code.bytes().all(|b| b.is_ascii_digit())).then_some(code)
}

/// The context of the verify status of a commit.
pub const VERIFY_CONTEXT: &str = "riff/verify";

/// The `gh` of the machine.
#[derive(Debug, Clone)]
pub struct Gh {
    program: PathBuf,
    /// The repository of each call, as `OWNER/REPO`: `GH_REPO`. With
    /// none, `gh` takes the repository of the current folder.
    repo: Option<String>,
    /// The time limit of each call. With none, a call can take each
    /// time that `gh` takes.
    limit: Option<Duration>,
    /// The token of each call, from one read for each time `keep`
    /// ([`Gh::with_token_kept`]). With none, each call of `gh` reads its
    /// token itself.
    token: Option<Shared>,
}

impl Default for Gh {
    fn default() -> Self {
        Self {
            program: "gh".into(),
            repo: None,
            limit: None,
            token: None,
        }
    }
}

/// The token that the clones of a `gh` share, and how long one read
/// stays.
#[derive(Debug, Clone)]
struct Shared {
    keep: Duration,
    read: Arc<Mutex<Option<GhToken>>>,
}

/// One read of the token of `gh`: its time, and the token, or `None`
/// when the read gave none. Its debug text has no value, and the memory
/// of the token is cleared when it drops.
struct GhToken {
    at: Instant,
    token: Option<Zeroizing<String>>,
}

impl fmt::Debug for GhToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GhToken")
    }
}

/// How long the calls of a `gh` with one token use one read of it
/// ([`Gh::with_one_token`]). A process that runs for a long time reads
/// it again after this time, so a new sign-in of `gh` reaches it.
pub const TOKEN_KEEP: Duration = Duration::from_secs(600);

/// The variables that give `gh` its token. When one is set, a call of
/// `gh` reads no keyring.
const TOKEN_VARS: [&str; 2] = ["GH_TOKEN", "GITHUB_TOKEN"];

/// The time between two looks at a `gh` with a time limit.
const LIMIT_POLL: Duration = Duration::from_millis(20);

impl Gh {
    /// The `gh` at `program`, for tests.
    pub fn at(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }

    /// The `gh` of the machine for the repository `repo` (`OWNER/REPO`)
    /// from each folder: it sets `GH_REPO`. The riff daemon uses it: it
    /// runs in no clone (#719).
    pub fn for_repo(repo: &str) -> Self {
        Self {
            repo: Some(repo.to_owned()),
            ..Self::default()
        }
    }

    /// The same `gh` with the time limit `limit` for each call
    /// (01M53S7WGM9DQV2DXFHBRSMTP8). At the end of the limit, riff stops
    /// `gh` and each process that it started, and the call fails.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let hangs = isolated::write_program(dir.path().join("gh"), "#!/bin/sh\nsleep 600\n");
    /// let gh = riff::pr::Gh::at(&hangs).with_limit(Duration::from_millis(500));
    /// assert_eq!(gh.limit(), Some(Duration::from_millis(500)));
    /// let start = Instant::now();
    /// let error = gh.run(&["pr", "list"], None).unwrap_err().to_string();
    /// assert_eq!(error, "gh pr list: no answer in 0.5 s. riff stopped it.");
    /// assert!(start.elapsed() < Duration::from_secs(10));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn with_limit(self, limit: Duration) -> Self {
        Self {
            limit: Some(limit),
            ..self
        }
    }

    /// The time limit of each call, when it has one.
    pub fn limit(&self) -> Option<Duration> {
        self.limit
    }

    /// The same `gh`, whose calls share the token of one read in each
    /// [`TOKEN_KEEP`] (01M54510SFK1Z9VPFYBYYSV10F). See
    /// [`Gh::with_token_kept`].
    pub fn with_one_token(self) -> Self {
        self.with_token_kept(TOKEN_KEEP)
    }

    /// The same `gh`, whose calls share the token of one read in each
    /// time `keep`. Each call of `gh` reads its token from the OS
    /// keyring, and each read opens a session of the Secret Service
    /// (#1003). So a call runs `gh auth token` when the last read is
    /// `keep` old or there is none, and each call gets the token in
    /// `GH_TOKEN`: in its environment, never in a command line or a
    /// file. The clones of this `gh` share the token. A `gh` that makes
    /// no call reads nothing. When the environment has `GH_TOKEN` or
    /// `GITHUB_TOKEN`, it reads nothing, and `gh` takes that token. When
    /// `gh auth token` fails, each call reads its token itself.
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let log = dir.path().join("gh.log");
    /// let script = format!(
    ///     "#!/bin/sh\necho \"${{GH_TOKEN:-none}} $*\" >> {}\n\
    ///      [ \"$1 $2\" = \"auth token\" ] && echo gho_one_read\nexit 0\n",
    ///     log.display(),
    /// );
    /// let gh = isolated::write_program(dir.path().join("gh"), &script);
    /// let shared = riff::pr::Gh::at(&gh).with_one_token();
    /// shared.run(&["pr", "view", "a"], None)?;
    /// shared.clone().run(&["pr", "view", "b"], None)?;
    /// // A token that stays no time: a read for each call.
    /// let fresh = riff::pr::Gh::at(&gh).with_token_kept(Duration::ZERO);
    /// fresh.run(&["pr", "view", "c"], None)?;
    /// # if std::env::var_os("GH_TOKEN").is_none() && std::env::var_os("GITHUB_TOKEN").is_none() {
    /// assert_eq!(
    ///     std::fs::read_to_string(&log)?,
    ///     "none auth token\ngho_one_read pr view a\ngho_one_read pr view b\n\
    ///      none auth token\ngho_one_read pr view c\n",
    /// );
    /// # }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn with_token_kept(self, keep: Duration) -> Self {
        Self {
            token: Some(Shared {
                keep,
                read: Arc::default(),
            }),
            ..self
        }
    }

    /// The token for the calls of this `gh`: `None` when it reads no
    /// token, or when the read fails.
    fn token(&self) -> Option<Zeroizing<String>> {
        let shared = self.token.as_ref()?;
        let mut read = shared.read.lock().unwrap_or_else(PoisonError::into_inner);
        if read.as_ref().is_none_or(|r| r.at.elapsed() >= shared.keep) {
            *read = Some(GhToken {
                at: Instant::now(),
                token: self.read_token(),
            });
        }
        read.as_ref()?.token.clone()
    }

    /// One read of the token with `gh auth token`, or `None` when the
    /// environment gives `gh` a token, or when the read fails.
    fn read_token(&self) -> Option<Zeroizing<String>> {
        if TOKEN_VARS
            .iter()
            .any(|var| std::env::var_os(var).is_some_and(|v| !v.is_empty()))
        {
            return None;
        }
        let plain = Gh {
            token: None,
            ..self.clone()
        };
        let (ok, stdout, _) = plain.output(&["auth", "token"], None).ok()?;
        let stdout = Zeroizing::new(stdout);
        let token = stdout.trim();
        (ok && !token.is_empty()).then(|| Zeroizing::new(token.to_owned()))
    }

    /// Runs `gh ARGS` with `input` on stdin, and returns its stdout. It
    /// fails with the stderr of `gh` when `gh` fails.
    pub fn run(&self, args: &[&str], input: Option<&str>) -> Result<String> {
        let (ok, stdout, stderr) = self.output(args, input)?;
        if !ok {
            bail!("gh {}: {}", args.join(" "), text::forge(stderr.trim()));
        }
        Ok(stdout)
    }

    /// Sends `body` with `gh api -X METHOD PATH`, and returns the reply
    /// with its headers. The body goes in a file, not on stdin: a body
    /// on stdin can get lost (#566, 01M49HF057R08DGW6X5A8EHR42). It
    /// fails with the stderr of `gh`, and with the HTTP status of the
    /// reply when `gh` got one.
    ///
    /// ```
    /// assert_eq!(riff::pr::http_status("HTTP/2.0 502 Bad Gateway\r\nServer: x\r\n"), Some("502"));
    /// assert_eq!(riff::pr::http_status(""), None);
    /// ```
    pub fn send(&self, method: &str, path: &str, body: &serde_json::Value) -> Result<String> {
        let mut file = tempfile::NamedTempFile::new().context("cannot make a file for the body")?;
        serde_json::to_writer(&mut file, body)?;
        let file_path = file.path().to_string_lossy().into_owned();
        let args = [
            "api",
            "-X",
            method,
            path,
            "--include",
            "--input",
            &file_path,
        ];
        let (ok, stdout, stderr) = self.output(&args, None)?;
        if !ok {
            let status = http_status(&stdout)
                .map(|s| format!("HTTP {s}: "))
                .unwrap_or_default();
            bail!(
                "gh api -X {method} {path}: {status}{}",
                text::forge(stderr.trim())
            );
        }
        Ok(stdout)
    }

    /// Runs `gh ARGS` and reads its JSON output.
    pub fn json<T: DeserializeOwned>(&self, args: &[&str]) -> Result<T> {
        let out = self.run(args, None)?;
        serde_json::from_str(&out).with_context(|| format!("gh {}", args.join(" ")))
    }

    /// Runs `gh ARGS`: whether it exits with status 0, its stdout and its
    /// stderr. With a time limit, it fails at the end of the limit
    /// ([`Gh::with_limit`]).
    fn output(&self, args: &[&str], input: Option<&str>) -> Result<(bool, String, String)> {
        use std::os::unix::process::CommandExt;
        let mut command = Command::new(&self.program);
        if let Some(repo) = &self.repo {
            command.env("GH_REPO", repo);
        }
        if let Some(token) = self.token() {
            command.env("GH_TOKEN", token.as_str());
        }
        command
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.limit.is_some() {
            // A group of its own: the stop at the end of the limit also
            // stops each process that `gh` started.
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("cannot run {}", self.program.display()))?;
        let Some(limit) = self.limit else {
            if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
                stdin.write_all(input.as_bytes())?;
            }
            let out = child.wait_with_output()?;
            return Ok((
                out.status.success(),
                String::from_utf8_lossy(&out.stdout).into_owned(),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ));
        };
        let end = Instant::now() + limit;
        // The pipes go on threads: a `gh` that does not read or that
        // writes much blocks no step of the wait.
        let writer = child
            .stdin
            .take()
            .zip(input.map(str::to_owned))
            .map(|(mut stdin, input)| {
                std::thread::spawn(move || {
                    let _ = stdin.write_all(input.as_bytes());
                })
            });
        let stdout = read_all(child.stdout.take());
        let stderr = read_all(child.stderr.take());
        let status: ExitStatus = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= end {
                if let Ok(group) = i32::try_from(child.id()) {
                    let _ = nix::sys::signal::killpg(
                        nix::unistd::Pid::from_raw(group),
                        nix::sys::signal::Signal::SIGKILL,
                    );
                }
                let _ = child.wait();
                bail!(text::gh_no_answer(&args.join(" "), limit));
            }
            std::thread::sleep(LIMIT_POLL);
        };
        if let Some(writer) = writer {
            let _ = writer.join();
        }
        let text = |pipe: JoinHandle<Vec<u8>>| {
            String::from_utf8_lossy(&pipe.join().unwrap_or_default()).into_owned()
        };
        Ok((status.success(), text(stdout), text(stderr)))
    }
}

/// Reads each byte of `pipe` on a thread of its own.
fn read_all(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// The body of a pull request for the issue of `link` in the form of the
/// hygiene check: the link line, the summary, and the trailers.
///
/// It adds the link line and each trailer only when `summary` does not
/// have it. It refuses a link line, or a trailer `Issue:` or
/// `Milestone:`, of `summary` that is not the one of the pull request,
/// and names the line (01M3W2627GYXR8CFW76KB6CB9W). The trailer
/// `Milestone:` names a wave by its number (01M518A07QF1YACBYT91M0DD1R),
/// and a trailer of the summary with the same number is the one of the
/// pull request.
///
/// ```
/// use hygiene::{Issue, Link, PullRequest};
///
/// let body = riff::pr::body(Link::Closes(12), "Show the wave.\n", "Wave 3").unwrap();
/// assert_eq!(body, "Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n");
/// let pr = PullRequest::new("Show the wave", &body, Some("Wave 3"));
/// assert!(hygiene::check_pr(&pr, Some(&Issue::new(12, "OPEN", Some("Wave 3")))).is_empty());
///
/// // A wave with a name: the trailer has only its number.
/// let body = riff::pr::body(Link::Closes(12), "Show the wave.\n", "Wave 3: Cloud").unwrap();
/// assert_eq!(body, "Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n");
/// let named = "Text.\n\nMilestone: Wave 3: Old name\n";
/// assert!(riff::pr::body(Link::Closes(12), named, "Wave 3: Cloud").is_ok());
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
    let issue = format!("#{n}");
    for (key, want) in [
        ("Issue", issue.clone()),
        ("Milestone", hygiene::milestone_value(milestone)),
    ] {
        let fits = |value: &str| match key {
            "Milestone" => hygiene::names_milestone(value, milestone),
            _ => value == issue,
        };
        let mut values = trailers.iter().filter(|(k, _)| k == key).map(|(_, v)| v);
        if let Some(value) = values.find(|value| !fits(value)) {
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
/// turns on auto-merge with a squash (01M3NB6FTGPD0S5JTXXXNGNNDT). With
/// `base`, the pull request is against that branch, for example
/// `release/1.x`, not the default branch (01M5QWJJ94T4M3KJB0R3WTJCQF).
/// Returns its number and its URL.
pub fn open(
    gh: &Gh,
    title: &str,
    summary: &str,
    issue: u64,
    refs: bool,
    base: Option<&str>,
) -> Result<(u64, String)> {
    let n = issue.to_string();
    let found: hygiene::Issue =
        gh.json(&["issue", "view", &n, "--json", "number,state,milestone"])?;
    let Some(milestone) = found.milestone.as_ref().map(|m| m.title.clone()) else {
        bail!("issue #{issue} has no milestone. Ask the lead to put it in a wave.");
    };
    // The milestone goes into the body and into `gh pr create`: a text
    // of the forge that `text::forge` changes is refused whole.
    if text::forge(&milestone) != milestone {
        bail!(
            "the milestone of issue #{issue} ({}) has a control character or is too long. Ask \
             the lead to rename it.",
            text::forge(&milestone)
        );
    }
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
    let mut args = vec![
        "pr",
        "create",
        "--title",
        title,
        "--milestone",
        &milestone,
        "--body-file",
        "-",
    ];
    args.extend(base.into_iter().flat_map(|b| ["--base", b]));
    let url = text::forge(gh.run(&args, Some(&body))?.trim());
    let number = url
        .rsplit('/')
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or_else(|| anyhow!("gh pr create printed no pull request URL: {url}"))?;
    let pr = number.to_string();
    gh.run(&["pr", "merge", &pr, "--auto", "--squash"], None)
        .with_context(|| {
            format!("pull request #{number} is open, but auto-merge is off. Run: gh pr merge --auto --squash {number}")
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
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub oid: String,
}

/// A check, as `gh pr checks --json name,bucket` gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct Check {
    /// The name, for example `Gate`.
    #[serde(deserialize_with = "crate::text::forge_de")]
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
    #[serde(deserialize_with = "crate::text::forge_de")]
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

/// The issue of the first line `Closes #N` of a body. A pull request
/// that closes an issue is the work of that item, on any branch.
///
/// ```
/// assert_eq!(riff::pr::closes_issue("Closes #12\n\nSummary\n"), Some(12));
/// assert_eq!(riff::pr::closes_issue("Refs #12\n"), None);
/// ```
pub fn closes_issue(body: &str) -> Option<u64> {
    body.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("Closes #")?;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
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
/// holder of the issue. When no session of `sessions` holds the issue
/// in the repository of the verifier `me`, also the lead of the user of
/// `me` there (01M3Z9N70J4H79VJN4ZKKH3G6S). `sessions` is the list of
/// `who`: it has no session that is gone.
///
/// ```
/// use riff::pr::result_to;
/// use riff_core::wire::SessionInfo;
///
/// let info = |uri: &str| SessionInfo {
///     uri: uri.parse().unwrap(),
///     live: true,
///     idle_secs: 0,
///     status: None,
///     worker: true,
///     container: false,
///     binary: None,
///     claims_secs: 0,
///     must_clear: false,
///     fresh_secs: None,
///     state: None,
///     work: None,
///     waits: None,
///     blocked: None,
///     step: None,
///     machine: None,
/// };
/// let to = |sessions: &[SessionInfo]| -> Vec<String> {
///     let me = "riff://mike@pangolin/o/r?session=v1&claim=verify-issue-12".parse().unwrap();
///     result_to(12, &me, sessions).iter().map(ToString::to_string).collect()
/// };
/// let holder = info("riff://mike@thelio/o/r?session=a1&claim=issue-12");
/// assert_eq!(to(&[holder]), ["claim=issue-12"]);
/// // The author released the item, or the claim is in another
/// // repository: the lead wakes.
/// let lead = ["claim=issue-12", "user=mike,repo=o/r,lead=true"];
/// assert_eq!(to(&[]), lead);
/// assert_eq!(to(&[info("riff://mike@thelio/o/r?session=a1")]), lead);
/// assert_eq!(to(&[info("riff://mike@thelio/o/s?session=a1&claim=issue-12")]), lead);
/// ```
pub fn result_to(issue: u64, me: &SessionUri, sessions: &[SessionInfo]) -> Vec<Selector> {
    let item = format!("issue-{issue}");
    let repo = me.place().repo_text();
    let held = sessions
        .iter()
        .any(|s| s.uri.place().repo_text() == repo && s.uri.claims().contains(&item));
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

/// The name of the check run of the full test run of a commit in the
/// repository of riff: the job `Gate` of `.github/workflows/ci.yml`
/// (01M49HAZ5BZYGAR9PGC089RM3F). It is also the full check of a repository
/// whose ruleset requires no check but [`VERIFY_CONTEXT`]
/// ([`repo_setup::full_check_of`]).
pub const GATE: &str = "Gate";

/// A check run, as `gh api repos/R/commits/C/check-runs` gives it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CheckRun {
    /// `queued`, `in_progress` or `completed`.
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub status: String,
    /// `success`, `failure`, `cancelled` and so on, when it is completed.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub conclusion: Option<String>,
    /// The commit of the run.
    #[serde(default, deserialize_with = "crate::text::forge_de")]
    pub head_sha: String,
    /// The page of the job, for example
    /// `https://github.com/o/r/actions/runs/38861280871/job/1`.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub html_url: Option<String>,
}

/// The check runs of one commit.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckRuns {
    /// Each run.
    pub check_runs: Vec<CheckRun>,
}

/// The state of the full check of a commit, for example of the [`GATE`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Gate {
    /// Each run of the Gate is completed with `success`.
    Passed,
    /// The commit has no run of the Gate.
    #[default]
    Missing,
    /// A run of the Gate is not completed.
    Running,
    /// A run of the Gate is completed with this conclusion, not `success`.
    Ended(String),
}

/// The state of the Gate from its check runs. A pass needs one run or
/// more, each completed with `success`.
///
/// ```
/// use riff::pr::{CheckRun, Gate, gate_state};
///
/// let run = |status: &str, conclusion: Option<&str>| CheckRun {
///     status: status.into(),
///     conclusion: conclusion.map(Into::into),
///     ..CheckRun::default()
/// };
/// assert_eq!(gate_state(&[run("completed", Some("success"))]), Gate::Passed);
/// assert_eq!(gate_state(&[]), Gate::Missing);
/// assert_eq!(gate_state(&[run("in_progress", None)]), Gate::Running);
/// assert_eq!(
///     gate_state(&[run("completed", Some("success")), run("completed", Some("failure"))]),
///     Gate::Ended("failure".into())
/// );
/// ```
pub fn gate_state(runs: &[CheckRun]) -> Gate {
    if runs.is_empty() {
        return Gate::Missing;
    }
    for run in runs {
        match (run.status.as_str(), run.conclusion.as_deref()) {
            ("completed", Some("success")) => {}
            ("completed", conclusion) => {
                return Gate::Ended(conclusion.unwrap_or("none").to_owned());
            }
            _ => return Gate::Running,
        }
    }
    Gate::Passed
}

/// The state of the full check of a commit from the state of each of its
/// checks. A check that runs makes it run. Else a check that ended with no
/// success gives its conclusion. Else a check with no run makes it missing.
///
/// ```
/// use riff::pr::{Gate, full_state};
///
/// assert_eq!(full_state([Gate::Passed, Gate::Passed]), Gate::Passed);
/// assert_eq!(full_state([Gate::Passed, Gate::Missing]), Gate::Missing);
/// assert_eq!(full_state([Gate::Missing, Gate::Running]), Gate::Running);
/// let failed = Gate::Ended("failure".into());
/// assert_eq!(full_state([failed.clone(), Gate::Missing]), failed);
/// assert_eq!(full_state([failed.clone(), Gate::Running]), Gate::Running);
/// assert_eq!(full_state([]), Gate::Passed);
/// ```
pub fn full_state(states: impl IntoIterator<Item = Gate>) -> Gate {
    let mut ended = None;
    let mut missing = false;
    for state in states {
        match state {
            Gate::Passed => {}
            Gate::Running => return Gate::Running,
            Gate::Ended(conclusion) => {
                ended.get_or_insert(conclusion);
            }
            Gate::Missing => missing = true,
        }
    }
    match (ended, missing) {
        (Some(conclusion), _) => Gate::Ended(conclusion),
        (None, true) => Gate::Missing,
        (None, false) => Gate::Passed,
    }
}

/// A name in a GitHub path, with each byte but a letter, a digit and
/// `-_.` as `%XX`: the check `checks / lint and test` has spaces and a
/// slash.
///
/// ```
/// assert_eq!(riff::pr::percent("Gate"), "Gate");
/// assert_eq!(
///     riff::pr::percent("checks / lint and test"),
///     "checks%20%2F%20lint%20and%20test"
/// );
/// ```
pub fn percent(name: &str) -> String {
    let mut out = String::new();
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The state of the full check `checks` of `commit` in `repo`
/// ([`repo_setup::full_check`]): the state of the check runs of each name
/// ([`gate_state`]), and the state of all of them ([`full_state`]).
pub fn gate_of(gh: &Gh, repo: &str, checks: &[String], commit: &str) -> Result<Gate> {
    let mut states = Vec::new();
    for check in checks {
        let name = percent(check);
        let path = format!("repos/{repo}/commits/{commit}/check-runs?check_name={name}");
        let runs: CheckRuns = gh.json(&["api", &path])?;
        states.push(gate_state(&runs.check_runs));
    }
    Ok(full_state(states))
}

/// Puts the result on pull request `number` as a comment, and sets the
/// status `riff/verify` of its head commit in `repo`
/// (01M3NB6FYXXKX80VHEVA5CV6RY). It reports nothing when the head is
/// not the commit `tested` ([`same_commit`]), and no pass while the full
/// check of the head has no success (01M49HAZ7P3JMWNCG1SWCMAXQP,
/// 01M5QYDE41FVFX26EC8K5QGYH4). The caller posts it to the holder of the
/// issue.
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
    if verdict == Verdict::Pass {
        let checks = repo_setup::full_check(gh, repo)?;
        let gate = gate_of(gh, repo, &checks, &commit)?;
        if gate != Gate::Passed {
            bail!("{}", text::gate_not_passed(number, &commit, &checks, &gate));
        }
    }
    let text = comment(verdict, issue, &commit, result);
    let url = text::forge(
        gh.run(&["pr", "comment", &n, "--body-file", "-"], Some(&text))?
            .trim(),
    );
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
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use hygiene::Link;

    use super::{Gh, body};
    use crate::text;

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

    /// At the end of its time limit, a `gh` stops with each process that
    /// it started, also a process that keeps its pipes open
    /// (01M53S7WGM9DQV2DXFHBRSMTP8).
    #[test]
    fn a_gh_with_a_time_limit_stops_with_each_process_that_it_started() {
        let dir = tempfile::tempdir().unwrap();
        let pid = dir.path().join("pid");
        let script = format!(
            "#!/bin/sh\nsleep 600 &\necho $! > {}\nwait\n",
            pid.display()
        );
        let limit = Duration::from_secs(2);
        let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script)).with_limit(limit);
        let start = Instant::now();
        let error = gh
            .json::<serde_json::Value>(&["pr", "view", "12"])
            .unwrap_err();
        assert_eq!(error.to_string(), text::gh_no_answer("pr view 12", limit));
        assert!(start.elapsed() < isolated::HANG);
        let sleep = std::fs::read_to_string(&pid).unwrap();
        let stat = PathBuf::from(format!("/proc/{}/stat", sleep.trim()));
        // A stopped process stays a zombie until its parent reaps it.
        let gone = || std::fs::read_to_string(&stat).map_or(true, |s| s.contains(") Z "));
        while !gone() {
            assert!(start.elapsed() < isolated::HANG, "sleep {sleep} runs on");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A `gh` with a time limit that answers in time gives its stdout,
    /// its stderr and its status, and reads its input.
    #[test]
    fn a_gh_with_a_time_limit_that_answers_gives_its_output() {
        let dir = tempfile::tempdir().unwrap();
        let script = "#!/bin/sh\ncat\necho \"$*\" >&2\nexit 3\n";
        let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script))
            .with_limit(isolated::HANG);
        let big = "a body\n".repeat(20_000);
        let (ok, stdout, stderr) = gh.output(&["issue", "comment"], Some(&big)).unwrap();
        assert!(!ok);
        assert_eq!(stdout, big);
        assert_eq!(stderr, "issue comment\n");
    }
}
