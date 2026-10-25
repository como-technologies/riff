//! The release cycle in riff commands: `riff release cut`, `publish`,
//! `status`, `approve`, `check`, `retry` and `rollback` (#671, #1104).
//!
//! # Design
//!
//! Each wave of a repository with the table `[release]` in its riff config
//! ends with a release in a fixed cycle (01M58KMKR33P1FSCFDQRZ56A8P,
//! 01M5SMPX8NM75P8E38DGQ17W7B, [`crate::release_config`]). Without the
//! table, each command refuses with one line before it touches the
//! forge. Each step is a riff command. No session
//! writes a script or a raw `gh` or `git` line for it. Each command finds
//! the versions, the commits, the tags and the IDs itself, names the
//! failed check when it refuses, and prints the next step ([`next`],
//! 01M58KMKWRKSV1DR42FRH2K43X). A person gives the human gates.
//!
//! | Command | Who | What |
//! |---|---|---|
//! | `riff release cut X.Y.Z` | a lead or a worker; `--skip` only a person | the release pull request from `origin/main` ([`cut`], 01M58B0HZYMAK1B2AJMWKHCXG0) |
//! | `riff release verify N` | each session; no model | the status `riff/verify` of a release pull request, after the checks of its label, diff, body and Gate ([`verify`], 01M5ND0ZNFFAPT1AWD3DP1Y77N) |
//! | `riff release publish vX.Y.Z` | the owner or an admin of the riff, or its lead, with a `gh` of an admin | the tag on the tested commit, the GitHub release, the chat line to the owner ([`publish`], 01M58B0JABCET719T3HFB19NDW) |
//! | `riff release test vX.Y.Z --host HOST` | the lead or a person; not a worker | the real-session test on the daemon of a host ([`test_target`], [`crate::daemon::release_test`], 01M5924C8BTT6CW05Q0YZJ1GCY) |
//! | `riff release status` | each session | the versions, what waits, the state and the next step ([`status`], 01M58KMKTE2XC35CNJBFKZWAXF) |
//! | `riff release approve vX.Y.Z` | a person in a terminal, or the lead of the owner with a word of the owner in riff chat | the pause of the riff, then the approval of `production`; it refuses while a lead holds the version, unless a person passes `--over-holds` ([`approve`], 01M58KMM3NFGJRW4CCV50KJQB2, 01M5SNV7E775DRY9QY60E8JPJR) |
//! | `riff release hold vX.Y.Z REASON` | the lead of any user, also of a member | the hold of the version: each other lead hears it, `riff release status` shows it ([`crate::hold`], 01M5SNV78EBAYY4SZ891DM2MQG) |
//! | `riff release check vX.Y.Z` | the lead | the checks after the deploy, the result on the release issue ([`check`], 01M58KMM5ZPAQ289S2YJVQ5WW6) |
//! | `riff release retry vX.Y.Z` | the lead | the failed jobs of the CI run again ([`retry`], 01M58KMM8CV9731AWGVZQBB80D) |
//! | `riff release rollback vPREV` | the lead | the deploy of an older release ([`rollback`], 01M58KMMAPJ33CQWR8GRX62GMR) |
//! | `riff wave end N` | the lead | the end of the wave, after a checked release ([`wave_gate`], 01M58KMMCZCE7HDNVT9PVHVZX2) |
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> cut: riff release cut
//!     cut --> merged: verify, merge
//!     merged --> tested: riff release test, or just real-session
//!     tested --> published: riff release publish
//!     published --> waiting: the jobs before Deploy pass
//!     waiting --> deploying: riff release approve (a person)
//!     deploying --> deployed
//!     deploying --> failed: the job Deploy failed
//!     failed --> waiting: riff release retry
//!     deployed --> checked: riff release check passes
//!     deployed --> failed: riff release check fails
//!     failed --> rolled_back: riff release rollback, approve (a person)
//!     checked --> [*]: riff wave end N
//! ```
//!
//! The state of a release ([`stage`], 01M58KMKTE2XC35CNJBFKZWAXF) comes
//! from the forge: the release pull request, the status
//! [`REAL_SESSION`] of its commits, the tag, the newest CI run that
//! deploys the tag ([`Deploy`]), and the release issue. `riff release
//! check` writes its result on the release issue with a first line
//! [`check_mark`], and closes the issue on a pass. `riff release
//! rollback` writes [`rollback_mark`] on the release issue that it
//! replaces.
//!
//! `cut` refuses a version that is not newer than the last release,
//! while a deploy waits for a person, and while the version on
//! `origin/main` has no published release. Only `--skip VERSION
//! --reason TEXT` of a person lets it go on: the reason goes into the
//! body of the pull request, and from there into the notes of the
//! release (01M58B0J4VQ61ER69Q09JYP7CG). The pull request says `Refs #N`
//! and `Part of #N` for the release issue, never `Closes #N`: the
//! release issue closes only when the checks after the deploy pass.
//!
//! Each step also posts one note to each other lead of the riff
//! ([`crate::hold`], 01M5SNV75DRFHR26BT2M8243VV): the command posts the
//! note of its own step, and the watch of the daemon posts the note of the
//! wait and of the end of the deploy ([`crate::watches`]).
//!
//! The guards of the callers are in [`Caller`]: the steps of the lead
//! refuse a worker, a container session and a session that is not the
//! lead ([`Caller::lead_step`]). The steps of a person refuse each agent
//! session ([`Caller::person_step`], 01M58B0JSQMWNCMSNX19536XT8).
//!
//! Each step goes through [`Forge`]. [`GitHub`] runs `gh` and `git`; a
//! test gives a fake forge. The checks after a deploy read the Cloud Run
//! log through [`CloudLog`]: [`RunLog`] runs `gcloud`, and in the container
//! of the lead [`crate::reach::DaemonLog`] asks the riff daemon of the host
//! to run it (#1247).
//!
//! ```
//! use riff::cycle::{level, version};
//!
//! assert_eq!(level(version("3.2.0")?, version("3.3.0")?), "minor");
//! assert!(version("3.3").is_err());
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use riff_core::build::Semver;
use riff_core::wire::PersonRole;
use serde::Deserialize;

use crate::hold::{Hold, Step};
use crate::pr::{self, Gh};
use crate::text;

/// The label of a release pull request and of a release issue. The
/// notes of a release leave out the pull requests with it.
pub const LABEL: &str = "release";

/// The GitHub environment of the deploy to the shared riff.
pub const PRODUCTION: &str = "production";

/// The workflow that runs for a tag.
pub const WORKFLOW: &str = "CI";

/// The check of a release tag in the tree.
pub const CHECK: &str = "deploy/release-check.sh";

/// A version `X.Y.Z` of three numbers, with no `v`.
///
/// ```
/// use riff::cycle::version;
///
/// assert_eq!(version("3.3.0")?.to_string(), "3.3.0");
/// assert!(version("v3.3.0").is_err());
/// assert!(version("3.3.0-rc1").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn version(text: &str) -> Result<Semver> {
    let parts: Vec<&str> = text.split('.').collect();
    let numbers = parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    if !numbers {
        bail!("\"{text}\" is not a version X.Y.Z, for example 3.3.0.");
    }
    text.parse()
        .map_err(|_| anyhow!("\"{text}\" is not a version X.Y.Z, for example 3.3.0."))
}

/// The version of a release tag `vX.Y.Z`.
///
/// ```
/// use riff::cycle::tag_version;
///
/// assert_eq!(tag_version("v3.3.0")?.to_string(), "3.3.0");
/// assert!(tag_version("3.3.0").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn tag_version(tag: &str) -> Result<Semver> {
    let refused = || {
        anyhow!(
            "\"{tag}\" is not a release tag vX.Y.Z, for example v3.3.0. next: riff release status"
        )
    };
    let rest = tag.strip_prefix('v').ok_or_else(refused)?;
    version(rest).map_err(|_| refused())
}

/// The tag of a version: `vX.Y.Z`.
pub fn tag(version: Semver) -> String {
    format!("v{version}")
}

/// The title of the release pull request and of the release issue.
pub fn title(version: Semver) -> String {
    format!("Release {}", tag(version))
}

/// The body of the release issue of `version`. The verify of the
/// release pull request reads its part "Verify of the release pull
/// request" (01M593FVJRTCGD1RPWXYJP5BNH).
///
/// ```
/// let body = riff::cycle::issue_body(riff::cycle::version("3.3.0").unwrap());
/// assert!(body.starts_with("The release v3.3.0.\n"));
/// assert!(body.contains("Done when: the release is published"));
/// assert!(body.contains("Verify of the release pull request"));
/// ```
pub fn issue_body(version: Semver) -> String {
    format!(
        "The release {}.\n\n\
         Done when: the release is published, the deploy to the shared riff passed, and the \
         checks after the deploy passed.\n\n\
         Verify of the release pull request. The pull request has the label `release`. Its \
         verify checks only this, and not the steps after the merge:\n\
         - The diff is only the change of the version in `Cargo.toml` and `Cargo.lock`.\n\
         - The format of the body is right.\n\
         - The Gate passed.\n\n\
         Steps after the merge. The verify of the pull request does not check these:\n\
         - The release is published.\n\
         - The deploy to the shared riff passed.\n\
         - The checks after the deploy passed.\n",
        tag(version)
    )
}

/// The level of the release `new` after the release `last`: `major`,
/// `minor` or `patch`.
///
/// ```
/// use riff::cycle::{level, version};
///
/// let v = |s| version(s).unwrap();
/// assert_eq!(level(v("3.2.0"), v("4.0.0")), "major");
/// assert_eq!(level(v("3.2.0"), v("3.2.1")), "patch");
/// ```
pub fn level(last: Semver, new: Semver) -> &'static str {
    if new.major != last.major {
        "major"
    } else if new.minor != last.minor {
        "minor"
    } else {
        "patch"
    }
}

/// The version of the first line `version = "…"` of a `Cargo.toml`: the
/// version of the crates of the workspace.
///
/// ```
/// let toml = "[workspace.package]\nversion = \"3.2.0\"\nedition = \"2024\"\n";
/// assert_eq!(riff::cycle::cargo_version(toml)?.to_string(), "3.2.0");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn cargo_version(toml: &str) -> Result<Semver> {
    let line = toml
        .lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .context("Cargo.toml has no line version = \"X.Y.Z\"")?;
    version(line.trim_end_matches('"'))
}

/// `Cargo.toml` with the version `new` in its first line `version`.
///
/// ```
/// let toml = "[workspace.package]\nversion = \"3.2.0\"\n\n[dependencies]\nx = { version = \"1\" }\n";
/// let new = riff::cycle::set_cargo(toml, riff::cycle::version("3.3.0")?)?;
/// assert_eq!(new, "[workspace.package]\nversion = \"3.3.0\"\n\n[dependencies]\nx = { version = \"1\" }\n");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn set_cargo(toml: &str, new: Semver) -> Result<String> {
    let mut done = false;
    let lines: Vec<String> = toml
        .split('\n')
        .map(|line| {
            if !done && line.starts_with("version = \"") {
                done = true;
                format!("version = \"{new}\"")
            } else {
                line.to_owned()
            }
        })
        .collect();
    if !done {
        bail!("Cargo.toml has no line version = \"X.Y.Z\"");
    }
    Ok(lines.join("\n"))
}

/// `Cargo.lock` with the version `new` for each crate of the workspace:
/// each package with no `source`. A crate of a registry or of git has a
/// `source`, and keeps its version. It does what
/// `cargo update --workspace` does for a new version, with no network.
///
/// ```
/// let lock = "[[package]]\nname = \"riff\"\nversion = \"3.2.0\"\ndependencies = [\n \"serde\",\n]\n\n\
///             [[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry+https://x\"\n";
/// let new = riff::cycle::set_lock(lock, riff::cycle::version("3.3.0")?);
/// assert!(new.contains("name = \"riff\"\nversion = \"3.3.0\"\n"));
/// assert!(new.contains("name = \"serde\"\nversion = \"1.0.0\"\n"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn set_lock(lock: &str, new: Semver) -> String {
    let lines: Vec<&str> = lock.split('\n').collect();
    let starts: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i] == "[[package]]")
        .chain([lines.len()])
        .collect();
    let mut out: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
    for pair in starts.windows(2) {
        let package = pair[0]..pair[1];
        if lines[package.clone()]
            .iter()
            .any(|l| l.starts_with("source = "))
        {
            continue;
        }
        if let Some(i) = package
            .clone()
            .find(|&i| lines[i].starts_with("version = \""))
        {
            out[i] = format!("version = \"{new}\"");
        }
    }
    out.join("\n")
}

/// A merged pull request since the last release, for the body of the
/// release pull request.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Merged {
    /// Its number.
    pub number: u64,
    /// Its title.
    pub title: String,
    /// The title of its wave, or `None`.
    pub wave: Option<String>,
    /// The issue of its trailer `Issue:`.
    pub issue: Option<u64>,
    /// True for a release pull request.
    pub release: bool,
    /// Its merge commit.
    pub commit: String,
}

/// A merged release pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasePr {
    /// Its number.
    pub number: u64,
    /// Its URL.
    pub url: String,
    /// Its merge commit.
    pub merge: String,
    /// Its head commit: the commit of its verify.
    pub head: String,
    /// Its body.
    pub body: String,
}

/// A pull request as `riff release verify` reads it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Candidate {
    /// Its title.
    pub title: String,
    /// Its body.
    pub body: String,
    /// Its state: `OPEN`, `CLOSED` or `MERGED`.
    pub state: String,
    /// Its labels.
    pub labels: Vec<String>,
    /// The title of its wave.
    pub milestone: Option<String>,
    /// Its head commit.
    pub head: String,
    /// The path of each file that it changes.
    pub files: Vec<String>,
}

/// The files that a release pull request may change: the version.
pub const VERSION_FILES: [&str; 2] = ["Cargo.toml", "Cargo.lock"];

/// A CI run that waits for the approval of [`PRODUCTION`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// The ID of the run.
    pub run: u64,
    /// The tag of the run.
    pub tag: String,
    /// The commit of the run.
    pub commit: String,
    /// The URL of the run.
    pub url: String,
    /// The ID of the environment [`PRODUCTION`].
    pub environment: u64,
}

/// Where the job `Deploy` of a CI run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployState {
    /// The jobs before the job `Deploy` run.
    Before,
    /// The job `Deploy` waits for the approval of [`PRODUCTION`].
    Waiting,
    /// The job `Deploy` runs.
    Running,
    /// The job `Deploy` passed.
    Passed,
    /// The job `Deploy`, or a job before it, failed.
    Failed,
}

/// The newest CI run that deploys a release tag: the run of the push of
/// the tag, or a run by hand on `main` with the input `tag`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deploy {
    /// The ID of the run.
    pub run: u64,
    /// The URL of the run.
    pub url: String,
    /// Where its job `Deploy` is.
    pub state: DeployState,
    /// The start of the job `Deploy`, as RFC 3339, or empty.
    pub at: String,
    /// The names of the jobs that failed.
    pub failed: Vec<String>,
}

/// A release issue: `Release vX.Y.Z` with the label [`LABEL`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReleaseIssue {
    /// Its number.
    pub number: u64,
    /// True while it is open.
    pub open: bool,
    /// The body of each comment, oldest first.
    pub comments: Vec<String>,
}

/// The commit status of the real-session test of a release
/// (01M58KMKZ2EG8NZG4F085S6NQX). `just real-session` sets it on the
/// commit that it tested.
pub const REAL_SESSION: &str = "riff/real-session";

/// The state of the real-session test on a commit: the commit status
/// [`REAL_SESSION`] (01M5924C8BTT6CW05Q0YZJ1GCY).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestState {
    /// The daemon of a host runs the test.
    Running,
    /// The test passed.
    Passed,
    /// The test failed.
    Failed,
}

impl TestState {
    /// The state of a GitHub status `state`, or `None` for one that
    /// riff does not know.
    ///
    /// ```
    /// use riff::cycle::TestState;
    ///
    /// assert_eq!(TestState::of("pending"), Some(TestState::Running));
    /// assert_eq!(TestState::of("success"), Some(TestState::Passed));
    /// assert_eq!(TestState::of("error"), Some(TestState::Failed));
    /// assert_eq!(TestState::of("other"), None);
    /// ```
    pub fn of(state: &str) -> Option<TestState> {
        match state {
            "pending" => Some(TestState::Running),
            "success" => Some(TestState::Passed),
            "failure" | "error" => Some(TestState::Failed),
            _ => None,
        }
    }

    /// The GitHub status state of this state.
    pub fn github(self) -> &'static str {
        match self {
            TestState::Running => "pending",
            TestState::Passed => "success",
            TestState::Failed => "failure",
        }
    }

    /// The word of the state, as `riff release status` shows it.
    pub fn word(self) -> &'static str {
        match self {
            TestState::Running => "running",
            TestState::Passed => "passed",
            TestState::Failed => "failed",
        }
    }
}

/// The `gh` arguments that set the status [`REAL_SESSION`] of `commit`
/// to `state`, with a `description` cut to fit the limit of GitHub.
///
/// ```
/// use riff::cycle::{TestState, status_args};
///
/// let args = status_args("abc", TestState::Running, "on thelio");
/// assert_eq!(args[..4], ["api", "-X", "POST", "repos/{owner}/{repo}/statuses/abc"]);
/// assert!(args.contains(&"state=pending".to_owned()));
/// assert!(args.contains(&"context=riff/real-session".to_owned()));
/// ```
pub fn status_args(commit: &str, state: TestState, description: &str) -> Vec<String> {
    let description: String = description.chars().take(130).collect();
    vec![
        "api".into(),
        "-X".into(),
        "POST".into(),
        format!("repos/{{owner}}/{{repo}}/statuses/{commit}"),
        "-f".into(),
        format!("state={}", state.github()),
        "-f".into(),
        format!("context={REAL_SESSION}"),
        "-f".into(),
        format!("description={description}"),
    ]
}

/// The steps of the release cycle on a forge. [`GitHub`] runs `gh` and
/// `git`. A test gives a fake.
pub trait Forge {
    /// The version of the crates on `origin/main`.
    fn main_version(&self) -> Result<Semver>;
    /// The version of each published release: not a draft.
    fn releases(&self) -> Result<Vec<Semver>>;
    /// The title of the current wave: the open wave with the lowest
    /// number.
    fn current_wave(&self) -> Result<String>;
    /// The open release issue `title`, made in `wave` with `body` when there is
    /// none.
    fn release_issue(&self, title: &str, wave: &str, body: &str) -> Result<u64>;
    /// Each pull request merged to `origin/main` after the tag `since`.
    fn merged_since(&self, since: Option<&str>) -> Result<Vec<Merged>>;
    /// Makes the branch of the release `version` from `origin/main`,
    /// with the new version in each file, checks it with [`CHECK`], and
    /// pushes it. Returns the name of the branch.
    fn push_branch(&self, version: Semver) -> Result<String>;
    /// Opens the pull request of `branch`, with the label [`LABEL`],
    /// and turns on auto-merge. Returns its number and its URL.
    fn open_pr(&self, branch: &str, title: &str, wave: &str, body: &str) -> Result<(u64, String)>;
    /// The merged release pull request `title`, or `None`.
    fn release_pr(&self, title: &str) -> Result<Option<ReleasePr>>;
    /// True when the status `riff/verify` of `commit` is a success.
    fn verify_passed(&self, commit: &str) -> Result<bool>;
    /// The pull request `number`, for its verify.
    fn candidate(&self, number: u64) -> Result<Candidate>;
    /// The checks of the full check of the repository: each check that the
    /// ruleset of its default branch requires, but `riff/verify`
    /// (01M5QYDE41FVFX26EC8K5QGYH4).
    fn full_check(&self) -> Result<Vec<String>>;
    /// The state of the full check `checks` of `commit`.
    fn gate(&self, checks: &[String], commit: &str) -> Result<pr::Gate>;
    /// Sets the status `riff/verify` of `commit` to a success.
    fn pass_verify(&self, commit: &str, description: &str) -> Result<()>;
    /// True when `commit` is on `origin/main`.
    fn on_main(&self, commit: &str) -> Result<bool>;
    /// True when the forge has the tag `tag`.
    fn tag_exists(&self, tag: &str) -> Result<bool>;
    /// True when the account of the forge is an admin of the
    /// repository: the ruleset of the tags lets it push a tag.
    fn admin(&self) -> Result<bool>;
    /// Makes the annotated tag `tag` on `commit`, and pushes only it.
    fn push_tag(&self, tag: &str, commit: &str) -> Result<()>;
    /// Makes the GitHub release of `tag`: `notes`, then the generated
    /// notes from `since`, marked as the latest. Returns its URL.
    fn make_release(&self, tag: &str, notes: &str, since: Option<&str>) -> Result<String>;
    /// The URL of the CI run of the push of `tag`, or `None`.
    fn tag_run(&self, tag: &str) -> Result<Option<String>>;
    /// Each CI run that waits for [`PRODUCTION`].
    fn waiting(&self) -> Result<Vec<Waiting>>;
    /// Approves `run` for [`PRODUCTION`], with `comment`.
    fn approve(&self, run: &Waiting, comment: &str) -> Result<()>;
    /// The open release pull request: its number and its version.
    fn open_release(&self) -> Result<Option<(u64, Semver)>>;
    /// The status [`REAL_SESSION`] of `commit`: `Some(true)` for a pass,
    /// `Some(false)` for a fail, `None` with no result.
    fn real_session(&self, commit: &str) -> Result<Option<bool>>;
    /// The state of the test [`REAL_SESSION`] on `commit`: running, passed
    /// or failed. `None` with no status.
    fn test_state(&self, commit: &str) -> Result<Option<TestState>>;
    /// `commit` and each newer commit of `origin/main`, the newest first.
    fn commits_since(&self, commit: &str) -> Result<Vec<String>>;
    /// The newest CI run that deploys `tag`, or `None`.
    fn deploy(&self, tag: &str) -> Result<Option<Deploy>>;
    /// The release issue `title`, open or closed, or `None`.
    fn issue_of(&self, title: &str) -> Result<Option<ReleaseIssue>>;
    /// Writes the comment `body` on the issue `issue`.
    fn comment(&self, issue: u64, body: &str) -> Result<()>;
    /// Closes the issue `issue`.
    fn close(&self, issue: u64) -> Result<()>;
    /// Files the issue `title` with `body` in `wave`. Returns its number.
    fn file_issue(&self, title: &str, body: &str, wave: &str) -> Result<u64>;
    /// Runs the failed jobs of the CI run `run` again.
    fn rerun(&self, run: u64) -> Result<()>;
    /// Starts the CI workflow on `main` with the input `tag`: the deploy
    /// of an older release.
    fn dispatch(&self, tag: &str) -> Result<()>;
    /// The version of each release tag that holds `commit`.
    fn released_in(&self, commit: &str) -> Result<Vec<Semver>>;
    /// The title of the open wave `number`.
    fn wave_title(&self, number: u64) -> Result<String>;
    /// Writes `text` in the description of the open wave `number`.
    fn note_wave(&self, number: u64, text: &str) -> Result<()>;
    /// The names of the jobs of the full check `checks` of `commit` that
    /// did not pass, with no `Gate`: the Gate only sums the other jobs
    /// ([`crate::look::failed_jobs`]).
    fn failed_jobs(&self, checks: &[String], commit: &str) -> Result<Vec<String>>;
    /// The current wave and the number of its open items, or `None` with
    /// no open wave. A release issue and a tracker are no item: they
    /// have no code of their own (01M5ND16V2SVN8A9K7W90VC6X9).
    fn wave_open(&self) -> Result<Option<(String, usize)>>;
    /// True when the repository has the table `[release]` in its riff
    /// config ([`crate::release_config`]): its waves end with a release.
    fn has_release(&self) -> bool;
}

/// The state of a release in the cycle (01M58KMKTE2XC35CNJBFKZWAXF).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The release pull request is open.
    Cut,
    /// The release pull request merged.
    Merged,
    /// The real-session test passed on the merge commit.
    Tested,
    /// The tag and the GitHub release exist. The jobs before the deploy
    /// run.
    Published,
    /// The job `Deploy` waits for a person.
    Waiting,
    /// The job `Deploy` runs.
    Deploying,
    /// The job `Deploy` passed.
    Deployed,
    /// The checks after the deploy passed.
    Checked,
    /// The deploy, or the checks after it, failed.
    Failed,
    /// An older release replaced it.
    RolledBack,
}

impl State {
    /// The word of the state, as `riff release status` shows it.
    ///
    /// ```
    /// assert_eq!(riff::cycle::State::RolledBack.word(), "rolled back");
    /// ```
    pub fn word(self) -> &'static str {
        match self {
            State::Cut => "cut",
            State::Merged => "merged",
            State::Tested => "tested",
            State::Published => "published",
            State::Waiting => "waiting",
            State::Deploying => "deploying",
            State::Deployed => "deployed",
            State::Checked => "checked",
            State::Failed => "failed",
            State::RolledBack => "rolled back",
        }
    }
}

/// A release and its state in the cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    /// Its version.
    pub version: Semver,
    /// Its state.
    pub state: State,
    /// Its release pull request.
    pub pr: Option<u64>,
    /// The merge commit of its release pull request.
    pub merge: Option<String>,
    /// The newest CI run that deploys it.
    pub deploy: Option<Deploy>,
    /// For [`State::Failed`]: true when the checks after the deploy
    /// failed, false when the deploy failed.
    pub checks_failed: bool,
    /// For [`State::Merged`]: the newest commit of `origin/main` and the
    /// state of its real-session test, when it has one.
    pub test: Option<(String, TestState)>,
    /// The newest published release before it.
    pub previous: Option<Semver>,
}

/// The first line of the comment of `riff release check` on a release
/// issue.
///
/// ```
/// assert_eq!(riff::cycle::check_mark("v3.3.0", "passed"), "riff release check v3.3.0: passed");
/// ```
pub fn check_mark(tag: &str, result: &str) -> String {
    format!("riff release check {tag}: {result}")
}

/// The first line of the comment of `riff release rollback` on the
/// release issue of the release that it replaces.
///
/// ```
/// assert_eq!(riff::cycle::rollback_mark("v3.2.0"), "riff release rollback v3.2.0");
/// ```
pub fn rollback_mark(previous: &str) -> String {
    format!("riff release rollback {previous}")
}

/// The patch release after `version`: the version of a fix forward.
fn patch_after(version: Semver) -> Semver {
    Semver {
        patch: version.patch + 1,
        ..version
    }
}

/// The commit to tag (01M58KMKZ2EG8NZG4F085S6NQX): the newest commit from
/// the merge commit `merge` to `origin/main` whose real-session test
/// passed. After a failed test, the fix merges, and the test passes on a
/// newer commit.
pub fn tested(forge: &dyn Forge, merge: &str) -> Result<Option<String>> {
    for commit in forge.commits_since(merge)? {
        if forge.real_session(&commit)? == Some(true) {
            return Ok(Some(commit));
        }
    }
    Ok(None)
}

/// The commit that the real-session test runs on for a merged release:
/// the newest commit of `origin/main`, which holds the merge commit
/// (01M5924C8BTT6CW05Q0YZJ1GCY).
fn test_commit(forge: &dyn Forge, merge: &str) -> Result<Option<String>> {
    Ok(forge.commits_since(merge)?.into_iter().next())
}

/// The commit for `riff release test TAG`. It refuses a release that is
/// not merged, and a commit that has a pass or a test that runs.
pub fn test_target(forge: &dyn Forge, tag_text: &str) -> Result<String> {
    let tag = tag(tag_version(tag_text)?);
    let stage = stage(forge, tag_version(&tag)?)?;
    let Some(stage) = stage.filter(|s| s.state == State::Merged) else {
        bail!(
            "riff release test: the release {tag} is not merged and untested. riff changed \
             nothing. next: riff release status"
        );
    };
    let commit = test_commit(forge, stage.merge.as_deref().unwrap_or_default())?
        .ok_or_else(|| anyhow!("riff release test: origin/main has no commit of {tag}"))?;
    if forge.test_state(&commit)? == Some(TestState::Running) {
        bail!(
            "riff release test: the test runs on {commit} already. riff changed nothing. \
             next: riff release status"
        );
    }
    Ok(commit)
}

/// The state of the release `version` in the cycle, or `None` when it
/// has no release pull request and no tag (01M58KMKTE2XC35CNJBFKZWAXF).
pub fn stage(forge: &dyn Forge, version: Semver) -> Result<Option<Stage>> {
    let tag = tag(version);
    let title = title(version);
    let releases = forge.releases()?;
    let previous = releases.iter().copied().filter(|r| *r < version).max();
    let issue = forge.issue_of(&title)?;
    let comments = issue.as_ref().map(|i| i.comments.as_slice()).unwrap_or(&[]);
    let passed = check_mark(&tag, "passed");
    let failed = check_mark(&tag, "failed");
    // Only a check that ran decides: a failed deploy and an unknown
    // result leave the state to the CI run of the tag.
    let last_check = comments
        .iter()
        .rev()
        .filter_map(|c| c.lines().next())
        .find(|l| *l == passed || *l == failed)
        .map(|l| l == passed);
    let rolled_back = comments
        .iter()
        .any(|c| c.starts_with("riff release rollback v"));
    let mut stage = Stage {
        version,
        state: State::Cut,
        pr: None,
        merge: None,
        deploy: None,
        checks_failed: false,
        test: None,
        previous,
    };
    if let Some(pr) = forge.release_pr(&title)? {
        stage.pr = Some(pr.number);
        stage.merge = Some(pr.merge);
    }
    stage.state = if rolled_back {
        State::RolledBack
    } else if issue.as_ref().is_some_and(|i| !i.open) || last_check == Some(true) {
        State::Checked
    } else if last_check == Some(false) {
        stage.checks_failed = true;
        State::Failed
    } else if forge.tag_exists(&tag)? {
        stage.deploy = forge.deploy(&tag)?;
        match stage.deploy.as_ref().map(|d| d.state) {
            None | Some(DeployState::Before) => State::Published,
            Some(DeployState::Waiting) => State::Waiting,
            Some(DeployState::Running) => State::Deploying,
            Some(DeployState::Passed) => State::Deployed,
            Some(DeployState::Failed) => State::Failed,
        }
    } else if let Some(merge) = &stage.merge {
        match tested(forge, merge)? {
            Some(_) => State::Tested,
            None => {
                if let Some(commit) = test_commit(forge, merge)? {
                    stage.test = forge.test_state(&commit)?.map(|state| (commit, state));
                }
                State::Merged
            }
        }
    } else {
        match forge.open_release()? {
            Some((number, open)) if open == version => {
                stage.pr = Some(number);
                State::Cut
            }
            _ => return Ok(None),
        }
    };
    Ok(Some(stage))
}

/// The next step of `stage`, for the line `next:` of each command
/// (01M58KMKWRKSV1DR42FRH2K43X).
///
/// ```
/// use riff::cycle::{Stage, State, next, version};
///
/// let mut stage = Stage {
///     version: version("3.3.0")?, state: State::Tested, pr: Some(1140), merge: Some("m1".into()),
///     deploy: None, checks_failed: false, test: None, previous: Some(version("3.2.0")?),
/// };
/// assert_eq!(next(&stage), "riff release publish v3.3.0");
/// stage.state = State::Waiting;
/// assert_eq!(next(&stage), "a person runs riff release approve v3.3.0 in a terminal");
/// stage.state = State::Failed;
/// assert_eq!(next(&stage), "riff release retry v3.3.0, or riff release rollback v3.2.0");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn next(stage: &Stage) -> String {
    let tag = tag(stage.version);
    let run = stage.deploy.as_ref().map(|d| d.url.as_str());
    let previous = stage
        .previous
        .map_or_else(|| "vPREVIOUS".to_owned(), self::tag);
    let patch = patch_after(stage.version);
    match stage.state {
        State::Cut => match stage.pr {
            Some(n) => format!("wait for the verify and the merge of pull request #{n}"),
            None => "wait for the verify and the merge of the release pull request".to_owned(),
        },
        State::Merged => match &stage.test {
            Some((commit, TestState::Running)) => {
                format!("wait for the real-session test on {commit}; riff release status shows it")
            }
            Some((_, TestState::Failed)) => {
                "file the bug in the wave, then just real-session on origin/main \
                 after its fix merges"
                    .to_owned()
            }
            _ => {
                format!("riff release test {tag} --host HOST, or just real-session on origin/main")
            }
        },
        State::Tested => format!("riff release publish {tag}"),
        State::Published => match run {
            Some(run) => format!("wait for the jobs of the CI run {run}"),
            None => format!("wait for the CI run of the tag {tag}"),
        },
        State::Waiting => format!("a person runs riff release approve {tag} in a terminal"),
        State::Deploying => format!("wait for the deploy: {}", run.unwrap_or("the CI run")),
        State::Deployed => format!("riff release check {tag}"),
        State::Failed if stage.checks_failed => format!(
            "fix forward with riff release cut {patch}, or roll back with riff release rollback \
             {previous}"
        ),
        State::Failed => format!("riff release retry {tag}, or riff release rollback {previous}"),
        State::Checked => "riff wave end N, for the wave of the release".to_owned(),
        State::RolledBack => format!("fix forward with riff release cut {patch}"),
    }
}

/// The lines `release:` and `next:` of `stage`.
///
/// ```
/// use riff::cycle::{Stage, State, stage_lines, version};
///
/// let stage = Stage {
///     version: version("3.3.0")?, state: State::Deployed, pr: None, merge: None,
///     deploy: None, checks_failed: false, test: None, previous: None,
/// };
/// assert_eq!(stage_lines(Some(&stage)), "release: v3.3.0 deployed\nnext: riff release check v3.3.0\n");
/// assert_eq!(stage_lines(None), "release: none\nnext: riff release cut X.Y.Z at the end of the wave\n");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn stage_lines(stage: Option<&Stage>) -> String {
    match stage {
        Some(stage) => format!(
            "release: {} {}\nnext: {}\n",
            tag(stage.version),
            stage.state.word(),
            next(stage)
        ),
        None => "release: none\nnext: riff release cut X.Y.Z at the end of the wave\n".to_owned(),
    }
}

/// The release that the cycle is at now: the open release pull request,
/// else the version on `origin/main`.
pub fn current(forge: &dyn Forge) -> Result<Option<Stage>> {
    let version = match forge.open_release()? {
        Some((_, version)) => version,
        None => forge.main_version()?,
    };
    stage(forge, version)
}

/// The last published release: the highest version of `releases`.
pub fn last(releases: &[Semver]) -> Option<Semver> {
    releases.iter().copied().max()
}

/// The skip of a version with no release: `--skip VERSION --reason TEXT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skip {
    /// The version on `origin/main` that gets no release.
    pub version: Semver,
    /// Why.
    pub reason: String,
}

/// The release pull request that [`cut`] made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    /// Its number.
    pub number: u64,
    /// Its URL.
    pub url: String,
    /// The release issue.
    pub issue: u64,
}

/// The waves of `merged`, in the order of their numbers, each with its
/// pull requests. A release pull request is left out.
fn waves(merged: &[Merged]) -> Vec<(String, Vec<&Merged>)> {
    let mut waves: Vec<(String, Vec<&Merged>)> = Vec::new();
    for m in merged.iter().filter(|m| !m.release) {
        let wave = m.wave.clone().unwrap_or_else(|| "No wave".to_owned());
        match waves.iter_mut().find(|(w, _)| *w == wave) {
            Some((_, list)) => list.push(m),
            None => waves.push((wave, vec![m])),
        }
    }
    waves.sort_by_key(|(w, _)| hygiene::wave_number(w).unwrap_or(u64::MAX));
    waves
}

/// The summary of the release pull request of `new`: the line `Part of`,
/// the level line, the skip, the list "Waves in this release", and the
/// items of each wave. [`pr::body`] adds the link line and the trailers.
///
/// ```
/// use riff::cycle::{Merged, Skip, summary, version};
///
/// let m = |n, wave: &str, issue| Merged {
///     number: n, title: format!("Change {n}"), wave: Some(wave.into()), issue: Some(issue), ..Merged::default()
/// };
/// let merged = [m(41, "Wave 34: Status", 12), m(40, "Wave 33", 11)];
/// let skip = Skip { version: version("3.3.0")?, reason: "the tag was refused".into() };
/// let text = summary(7, version("3.2.0")?, version("3.4.0")?, Some(&skip), None, &merged);
/// assert!(text.starts_with("Part of #7: the release pull request."));
/// assert!(text.contains("Level: minor, 3.4.0."));
/// assert!(text.contains("No release 3.3.0 exists: the tag was refused."));
/// assert!(text.contains("Waves in this release: Wave 33, Wave 34: Status."));
/// assert!(text.contains("Wave 34: Status:\n\n- #12: Change 41 (#41).\n"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn summary(
    issue: u64,
    last: Semver,
    new: Semver,
    skip: Option<&Skip>,
    why: Option<&str>,
    merged: &[Merged],
) -> String {
    let mut out = format!(
        "Part of #{issue}: the release pull request. The issue closes when the checks after \
         the deploy pass.\n\nLevel: {}, {new}.",
        level(last, new)
    );
    if let Some(why) = why.map(str::trim).filter(|w| !w.is_empty()) {
        out.push(' ');
        out.push_str(why);
    }
    out.push_str("\n\n");
    if let Some(skip) = skip {
        out.push_str(&format!(
            "No release {} exists: {}. This release has each item since {}.\n\n",
            skip.version,
            skip.reason.trim().trim_end_matches('.'),
            tag(last)
        ));
    }
    let waves = waves(merged);
    let names: Vec<&str> = waves.iter().map(|(w, _)| w.as_str()).collect();
    if names.is_empty() {
        out.push_str("Waves in this release: none.\n");
    } else {
        out.push_str(&format!("Waves in this release: {}.\n", names.join(", ")));
    }
    for (wave, list) in &waves {
        out.push_str(&format!("\n{wave}:\n\n"));
        for m in list {
            let issue = m.issue.map(|n| format!("#{n}: ")).unwrap_or_default();
            let title = m.title.trim_end_matches('.');
            out.push_str(&format!("- {issue}{title} (#{}).\n", m.number));
        }
    }
    out
}

/// Makes the release pull request of `new` from `origin/main`
/// (01M58B0HZYMAK1B2AJMWKHCXG0, 01M58B0J4VQ61ER69Q09JYP7CG).
pub fn cut(forge: &dyn Forge, new: Semver, skip: Option<&Skip>, why: Option<&str>) -> Result<Cut> {
    let releases = forge.releases()?;
    let Some(last) = last(&releases) else {
        bail!(
            "riff release cut: the forge has no published release. next: a person makes the \
             first release by hand"
        );
    };
    if new <= last {
        bail!(
            "riff release cut: {new} is not newer than the last release {}. Give a version after \
             {last}. next: riff release cut {}",
            tag(last),
            patch_after(last)
        );
    }
    let main = forge.main_version()?;
    if main > last && !releases.contains(&main) {
        match skip {
            Some(skip) if skip.version == main && !skip.reason.trim().is_empty() => {}
            Some(skip) if skip.version != main => bail!(
                "riff release cut: --skip {} is not the version on origin/main: {main}. next: \
                 riff release cut {new} --skip {main} --reason TEXT",
                skip.version
            ),
            _ => bail!(
                "riff release cut: the version {main} on origin/main has no published release. \
                 To go on with no release {main}, give --skip {main} --reason TEXT. next: riff \
                 release publish {}",
                tag(main)
            ),
        }
    } else if let Some(skip) = skip {
        bail!(
            "riff release cut: --skip {} names no version to skip: the version {main} on \
             origin/main has its release. next: riff release cut {new}",
            skip.version
        );
    }
    if let Some(run) = forge.waiting()?.first() {
        bail!(
            "riff release cut: the deploy of {} waits for the approval of {PRODUCTION}. A person \
             runs riff release approve {} first. next: a person runs riff release approve {} in a \
             terminal",
            run.tag,
            run.tag,
            run.tag
        );
    }
    let wave = forge.current_wave()?;
    let title = title(new);
    let issue = forge.release_issue(&title, &wave, &issue_body(new))?;
    let merged = forge.merged_since(Some(&tag(last)))?;
    let summary = summary(issue, last, new, skip, why, &merged);
    let body = pr::body(hygiene::Link::Refs(issue), &summary, &wave)?;
    let errors = hygiene::check_pr(
        &hygiene::PullRequest::new(&title, &body, Some(&wave)),
        Some(&hygiene::Issue::new(issue, "OPEN", Some(&wave))),
    );
    if !errors.is_empty() {
        let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
        bail!(
            "riff release cut: the pull request breaks issue hygiene:\n{}\nnext: fix the release \
             issue #{issue}, then riff release cut {new}",
            lines.join("\n")
        );
    }
    let branch = forge.push_branch(new)?;
    let (number, url) = forge.open_pr(&branch, &title, &wave, &body)?;
    Ok(Cut { number, url, issue })
}

/// The verify of the release pull request `number` with no model
/// session (01M5ND0ZNFFAPT1AWD3DP1Y77N). The pull request is open and
/// has the label [`LABEL`], the body has the format of [`cut`], the
/// diff is only [`VERSION_FILES`], and the full check of the head commit
/// passed (each check that the ruleset requires, but `riff/verify`:
/// [`crate::repo_setup::full_check`], 01M5R13DK401K3TZ91E2QK2DYM). Then it
/// sets the status `riff/verify` of the head commit. Else it refuses, and
/// sets nothing. The refusal for the full check names the checks
/// ([`text::release_gate_not_passed`]). Returns the head commit.
pub fn verify(forge: &dyn Forge, number: u64) -> Result<String> {
    let refuse = |why: String| {
        anyhow!(
            "riff release verify: pull request #{number} {why}. next: fix it, then riff release \
             verify {number}"
        )
    };
    let pr = forge.candidate(number)?;
    if pr.state != "OPEN" {
        return Err(refuse(format!("is {}, not open", pr.state.to_lowercase())));
    }
    if !pr.labels.iter().any(|l| l == LABEL) {
        return Err(refuse(format!(
            "has no label {LABEL}. A pull request with code needs the verify of a session"
        )));
    }
    let extra: Vec<&str> = pr
        .files
        .iter()
        .map(String::as_str)
        .filter(|f| !VERSION_FILES.contains(f))
        .collect();
    if !extra.is_empty() || pr.files.is_empty() {
        let changed = if extra.is_empty() {
            "no file".to_owned()
        } else {
            extra.join(", ")
        };
        return Err(refuse(format!(
            "changes {changed}, and a release changes only {}",
            VERSION_FILES.join(" and ")
        )));
    }
    let errors = hygiene::check_pr(
        &hygiene::PullRequest::new(&pr.title, &pr.body, pr.milestone.as_deref()),
        None,
    );
    if !errors.is_empty() {
        let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
        return Err(refuse(format!(
            "breaks issue hygiene:\n{}",
            lines.join("\n")
        )));
    }
    let checks = forge.full_check()?;
    let gate = forge.gate(&checks, &pr.head)?;
    if gate != pr::Gate::Passed {
        bail!(
            "{}",
            text::release_gate_not_passed(number, &pr.head, &checks, &gate)
        );
    }
    forge.pass_verify(&pr.head, "pass: release pull request, full check and diff")?;
    Ok(pr.head)
}

/// The tag of the release pull request with the title `title`
/// (`Release vX.Y.Z`, see [`title`]).
///
/// ```
/// assert_eq!(riff::cycle::tag_of_title("Release v3.3.0")?, "v3.3.0");
/// assert!(riff::cycle::tag_of_title("Fix the line").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn tag_of_title(title: &str) -> Result<String> {
    let version = title
        .strip_prefix("Release ")
        .ok_or_else(|| anyhow!("\"{title}\" is no title of a release pull request"))?;
    Ok(tag(tag_version(version)?))
}

/// The notes of a release from the body of its pull request: the body
/// with no link line, no line `Part of`, and no trailer.
///
/// ```
/// let body = "Refs #7\n\nPart of #7: the release pull request.\n\nLevel: minor, 3.3.0.\n\nIssue: #7\nMilestone: Wave 34\n";
/// assert_eq!(riff::cycle::notes(body), "Level: minor, 3.3.0.\n");
/// ```
pub fn notes(body: &str) -> String {
    let kept: Vec<&str> = body
        .lines()
        .filter(|l| {
            hygiene::Link::parse(l).is_none()
                && !l.starts_with("Part of #")
                && !l.starts_with("Issue: #")
                && !l.starts_with("Milestone: ")
        })
        .collect();
    let mut out = String::new();
    let mut blank = true;
    for line in kept {
        if line.trim().is_empty() {
            if !blank {
                out.push('\n');
            }
            blank = true;
        } else {
            out.push_str(line);
            out.push('\n');
            blank = false;
        }
    }
    out.trim_end().to_owned() + "\n"
}

/// What [`publish`] made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    /// The URL of the GitHub release.
    pub release: String,
    /// The URL of the CI run of the tag, when GitHub shows it.
    pub run: Option<String>,
}

/// The chat line of `riff release publish` to the owner `owner`
/// (01M58KMM1BHX3YY81BPZG04JSX): the version, the CI run that builds, and
/// that riff tells the owner when the deploy waits. It says nothing of a
/// wait: the jobs before the job `Deploy` still run
/// ([`waits_line`], 01M5RGD0XTR4X1MEV726HMCEGQ).
///
/// ```
/// let line = riff::cycle::publish_line("mike", "v3.3.0", Some("https://x/runs/9"));
/// assert_eq!(
///     line,
///     "@mike riff release v3.3.0 is published. Its CI run builds: https://x/runs/9. \
///      riff tells you when its deploy waits for your approval."
/// );
/// assert!(riff::cycle::publish_line("", "v3.3.0", None).starts_with("riff release v3.3.0"));
/// ```
pub fn publish_line(owner: &str, tag: &str, run: Option<&str>) -> String {
    let run = run.unwrap_or("the CI run of the tag");
    format!(
        "{}riff release {tag} is published. Its CI run builds: {run}. riff tells you when its \
         deploy waits for your approval.",
        mention(owner)
    )
}

/// The chat line to the owner `owner` while the job `Deploy` of `tag`
/// waits for [`PRODUCTION`] (01M5RGD0XTR4X1MEV726HMCEGQ): the run, and the
/// command of the person. A `reminder` starts with a word that says so.
/// Each hold of `holds` follows in the same words as `riff release
/// status` (01M5SNV7BC5J58WRZRJ5HS4NSJ).
///
/// ```
/// let line = riff::cycle::waits_line("mike", "v3.3.0", "https://x/runs/9", false, &[]);
/// assert_eq!(
///     line,
///     "@mike riff release v3.3.0 waits for your approval: https://x/runs/9. \
///      In a terminal, run: riff release approve v3.3.0"
/// );
/// let again = riff::cycle::waits_line("", "v3.3.0", "https://x/runs/9", true, &[]);
/// assert!(again.starts_with("Reminder: riff release v3.3.0 still waits"));
/// let held = riff::hold::Hold { user: "nate".into(), repo: "o/s".into(), reason: "A demo runs.".into() };
/// let line = riff::cycle::waits_line("mike", "v3.3.0", "https://x/runs/9", false, &[held]);
/// assert!(line.ends_with(" A hold stands: the lead of o/s asks to wait: A demo runs."));
/// ```
pub fn waits_line(owner: &str, tag: &str, run: &str, reminder: bool, holds: &[Hold]) -> String {
    let (start, waits) = match reminder {
        true => ("Reminder: ", "still waits"),
        false => ("", "waits"),
    };
    let mut line = format!(
        "{start}{}riff release {tag} {waits} for your approval: {run}. In a terminal, run: riff \
         release approve {tag}",
        mention(owner)
    );
    if !holds.is_empty() {
        let each: Vec<String> = holds.iter().map(Hold::line).collect();
        line.push_str(&format!(" A hold stands: {}", each.join(" ")));
    }
    line
}

/// `@owner ` for a chat line, or nothing when the riff has no owner.
fn mention(owner: &str) -> String {
    match owner.is_empty() {
        true => String::new(),
        false => format!("@{owner} "),
    }
}

/// Publishes the release `tag` (01M58B0JABCET719T3HFB19NDW,
/// 01M58KMM1BHX3YY81BPZG04JSX): it checks each fact first, then makes and
/// pushes the tag, then makes the GitHub release. The riff runs on: only
/// the approval of the deploy pauses it.
pub fn publish(forge: &dyn Forge, tag_text: &str) -> Result<Published> {
    let version = tag_version(tag_text)?;
    let tag = tag(version);
    let title = title(version);
    let Some(pr) = forge.release_pr(&title)? else {
        bail!(
            "riff release publish: no merged pull request \"{title}\" with the label {LABEL}. \
             next: riff release cut {version}"
        );
    };
    if !forge.verify_passed(&pr.head)? {
        bail!(
            "riff release publish: the check riff/verify of pull request #{} did not pass. \
             next: riff release status",
            pr.number
        );
    }
    if !forge.on_main(&pr.merge)? {
        bail!(
            "riff release publish: the merge commit {} of pull request #{} is not on \
             origin/main. next: riff release status",
            pr.merge,
            pr.number
        );
    }
    let Some(commit) = tested(forge, &pr.merge)? else {
        if forge.real_session(&pr.merge)? == Some(false) {
            bail!(
                "riff release publish: the real-session test failed on the merge commit {}. \
                 next: file the bug in the wave, then just real-session on origin/main after its \
                 fix merges",
                pr.merge
            );
        }
        bail!(
            "riff release publish: no commit from the merge commit {} to origin/main has the \
             status {REAL_SESSION}. next: just real-session on origin/main",
            pr.merge
        );
    };
    let main = forge.main_version()?;
    if main != version {
        bail!(
            "riff release publish: Cargo.toml on origin/main has the version {main}, not \
             {version}. next: riff release status"
        );
    }
    if forge.tag_exists(&tag)? {
        bail!(
            "riff release publish: the tag {tag} exists on GitHub already. next: riff release status"
        );
    }
    if !forge.admin()? {
        bail!(
            "riff release publish: the account of gh is not an admin of the repository, so the \
             ruleset of the tags refuses its push. next: gh auth login as an admin, then riff \
             release publish {tag}"
        );
    }
    let since = last(&forge.releases()?).map(self::tag);
    forge.push_tag(&tag, &commit)?;
    let release = forge.make_release(&tag, &notes(&pr.body), since.as_deref())?;
    let run = forge.tag_run(&tag)?;
    Ok(Published { release, run })
}

/// What `riff release status` shows (01M58B0JEYNAW67NS226FMX5XH).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The version on `origin/main`.
    pub main: Semver,
    /// The last published release.
    pub last: Option<Semver>,
    /// The merged release pull request of the version on `origin/main`
    /// when that version has no published release.
    pub unpublished: Option<ReleasePr>,
    /// Each CI run that waits for [`PRODUCTION`].
    pub waiting: Vec<Waiting>,
    /// The release that the cycle is at now ([`current`]).
    pub stage: Option<Stage>,
    /// The newest commit of a merged release, and the state of the
    /// real-session test on it (01M5924C8BTT6CW05Q0YZJ1GCY).
    pub test: Option<(String, Option<TestState>)>,
    /// Each hold of the release and of each deploy that waits
    /// (01M5SNV7BC5J58WRZRJ5HS4NSJ). [`status`] leaves it empty: the holds
    /// are in the thread `release` of riff-server, and the command fills
    /// them in ([`status_tags`]).
    pub holds: Vec<Hold>,
}

/// The facts of the release cycle.
pub fn status(forge: &dyn Forge) -> Result<Status> {
    let main = forge.main_version()?;
    let releases = forge.releases()?;
    let unpublished = if releases.contains(&main) {
        None
    } else {
        forge.release_pr(&title(main))?
    };
    let stage = current(forge)?;
    let test = match stage.as_ref().filter(|s| s.state == State::Merged) {
        Some(stage) => test_commit(forge, stage.merge.as_deref().unwrap_or_default())?
            .map(|commit| forge.test_state(&commit).map(|state| (commit, state)))
            .transpose()?,
        None => None,
    };
    Ok(Status {
        main,
        last: last(&releases),
        unpublished,
        waiting: forge.waiting()?,
        stage,
        test,
        holds: Vec::new(),
    })
}

/// The tags whose holds `riff release status` shows: the current release
/// and each deploy that waits for [`PRODUCTION`] (01M5SNV7BC5J58WRZRJ5HS4NSJ).
pub fn status_tags(status: &Status) -> Vec<String> {
    let mut tags: Vec<String> = status.stage.iter().map(|s| tag(s.version)).collect();
    tags.extend(status.waiting.iter().map(|w| w.tag.clone()));
    tags.sort();
    tags.dedup();
    tags
}

/// The lines of `riff release status`.
///
/// ```
/// use riff::cycle::{Status, render_status, version};
///
/// let status = Status {
///     main: version("3.3.0")?, last: Some(version("3.2.0")?), unpublished: None, waiting: vec![], stage: None, test: None,
///     holds: vec![],
/// };
/// assert_eq!(
///     render_status(&status),
///     "version on origin/main: 3.3.0\nlast release: v3.2.0\nmerged, not published: none\n\
///      waits for production: none\nrelease: none\nnext: riff release cut X.Y.Z at the end of the wave\n"
/// );
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn render_status(status: &Status) -> String {
    let last = status.last.map_or("none".to_owned(), tag);
    let unpublished = status.unpublished.as_ref().map_or("none".to_owned(), |pr| {
        format!("{} (pull request #{})", tag(status.main), pr.number)
    });
    let mut out = format!(
        "version on origin/main: {}\nlast release: {last}\nmerged, not published: {unpublished}\n",
        status.main
    );
    if status.waiting.is_empty() {
        out.push_str("waits for production: none\n");
    }
    for w in &status.waiting {
        out.push_str(&format!(
            "waits for production: {} {}. A person runs: riff release approve {}\n",
            w.tag, w.url, w.tag
        ));
    }
    if let Some((commit, state)) = &status.test {
        let word = state.map_or("none", TestState::word);
        out.push_str(&format!("real-session test: {word} on {commit}\n"));
    }
    for hold in &status.holds {
        out.push_str(&format!("hold: {}\n", hold.line()));
    }
    out.push_str(&stage_lines(status.stage.as_ref()));
    out
}

/// What [`approve`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approved {
    /// No CI run of the tag waits: riff changed nothing.
    NoRun,
    /// The person said no: riff changed nothing.
    No,
    /// The person said yes to this run. [`approve_run`] approves it after
    /// the pause of the riff.
    Yes(Waiting),
    /// A hold stands, and the person did not pass `--over-holds`: riff
    /// changed nothing (01M5SNV7E775DRY9QY60E8JPJR).
    Held(Vec<Hold>),
}

/// Asks for the approval of the deploy of `tag` to [`PRODUCTION`]
/// (01M58B0JMFQ5E40TWTQ48MPSGK). It shows the tag, the commit and the
/// run to `out`, and asks `yes` on `answer`. It changes nothing.
///
/// While a hold of `holds` stands, it asks nothing and gives
/// [`Approved::Held`], unless `over_holds` is true: then it shows each
/// hold above the question (01M5SNV7E775DRY9QY60E8JPJR).
pub fn approve(
    forge: &dyn Forge,
    tag_text: &str,
    holds: &[Hold],
    over_holds: bool,
    answer: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Approved> {
    let tag = tag(tag_version(tag_text)?);
    let Some(run) = forge.waiting()?.into_iter().find(|w| w.tag == tag) else {
        return Ok(Approved::NoRun);
    };
    if !holds.is_empty() && !over_holds {
        return Ok(Approved::Held(holds.to_vec()));
    }
    writeln!(
        out,
        "tag: {}\ncommit: {}\nrun: {}",
        run.tag, run.commit, run.url
    )?;
    for hold in holds {
        writeln!(out, "hold: {}", hold.line())?;
    }
    write!(
        out,
        "Approve the deploy of {tag} to {PRODUCTION}? (yes or no) "
    )?;
    out.flush()?;
    let mut line = String::new();
    answer.read_line(&mut line)?;
    if !matches!(line.trim(), "yes" | "y") {
        return Ok(Approved::No);
    }
    Ok(Approved::Yes(run))
}

/// Approves `run` with the comment `Release vX.Y.Z`. `riff release
/// approve` pauses the riff first, and that tells each other lead
/// (01M58KMM3NFGJRW4CCV50KJQB2).
pub fn approve_run(forge: &dyn Forge, run: &Waiting) -> Result<()> {
    forge.approve(run, &format!("Release {}", run.tag))
}

/// Approves `run` for the lead, with the comment that names the message
/// `seq` of the owner in riff chat (01M594XQJMXAXY3JXP66Y8EVQ7).
pub fn approve_run_said(forge: &dyn Forge, run: &Waiting, seq: u64) -> Result<()> {
    forge.approve(
        run,
        &format!(
            "Release {}, approved by the owner in riff chat, message {seq}",
            run.tag
        ),
    )
}

/// The CI run of `tag_text` that waits for [`PRODUCTION`], or `None`.
pub fn waiting_run(forge: &dyn Forge, tag_text: &str) -> Result<Option<Waiting>> {
    let tag = tag(tag_version(tag_text)?);
    Ok(forge.waiting()?.into_iter().find(|w| w.tag == tag))
}

/// The messages of the thread `chat` as [`ChatLine`]s.
pub fn chat_lines(read: Vec<riff_client::api::Checked>) -> Vec<ChatLine> {
    read.into_iter()
        .map(|c| ChatLine {
            seq: c.message.seq,
            user: c.message.from.who().user().to_owned(),
            session: c.message.from.who().session().is_some(),
            verified: c.verified,
            body: c.message.body,
        })
        .collect()
}

/// One message of the thread `chat`, as [`owner_said`] reads it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatLine {
    /// The number of the message in the thread.
    pub seq: u64,
    /// The USER of the sender.
    pub user: String,
    /// True when the sender is a session, not a person.
    pub session: bool,
    /// True when riff proved the sender.
    pub verified: bool,
    /// The text.
    pub body: String,
}

/// The words of `body`, in lower case, with no mark around them.
fn words(body: &str) -> Vec<String> {
    body.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .collect()
}

/// True when `word` is a version: three numbers with dots, and maybe a `v`.
fn is_version(word: &str) -> bool {
    let bare = word.strip_prefix('v').unwrap_or(word);
    let parts: Vec<&str> = bare.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// How `body` approves `tag`: `Some(true)` when it names the version
/// (with or without the `v`), `Some(false)` when it names no version,
/// and `None` when it does not approve, names another version, or denies.
fn approves(body: &str, tag: &str) -> Option<bool> {
    let words = words(body);
    let has = |f: &dyn Fn(&str) -> bool| words.iter().any(|w| f(w));
    if has(&|w| matches!(w, "not" | "never" | "dont"))
        || body.to_lowercase().contains("don't")
        || !has(&|w| matches!(w, "approve" | "approved" | "deploy"))
    {
        return None;
    }
    let tag = tag.to_lowercase();
    let bare = tag.strip_prefix('v').unwrap_or(&tag);
    let named: Vec<&str> = words
        .iter()
        .map(String::as_str)
        .filter(|w| is_version(w))
        .collect();
    if named.is_empty() {
        Some(false)
    } else if named
        .iter()
        .any(|w| w.strip_prefix('v').unwrap_or(w) == bare)
    {
        Some(true)
    } else {
        None
    }
}

/// True when `body` says `stop` or `wait`.
fn says_stop(body: &str) -> bool {
    words(body)
        .iter()
        .any(|w| matches!(w.as_str(), "stop" | "wait"))
}

/// The line that `riff release cut` writes in riff chat to the owner
/// `owner`: the version, and the one message that lets the lead run the
/// whole release (01M5NCYW06FJ8P5X629VFK1BNR).
///
/// ```
/// let line = riff::cycle::cut_line("mike", "v3.3.0");
/// assert_eq!(
///     line,
///     "@mike riff release v3.3.0 is cut. Say \"approve 3.3.0\" in riff chat, and the lead \
///      runs the test, the publish, the approval of the deploy and the check."
/// );
/// ```
pub fn cut_line(owner: &str, tag: &str) -> String {
    let to = if owner.is_empty() {
        String::new()
    } else {
        format!("@{owner} ")
    };
    let bare = tag.strip_prefix('v').unwrap_or(tag);
    format!(
        "{to}riff release {tag} is cut. Say \"approve {bare}\" in riff chat, and the lead runs \
         the test, the publish, the approval of the deploy and the check."
    )
}

/// The number of the message in which the owner approved the release
/// `tag` (01M5NCYW06FJ8P5X629VFK1BNR, in part instead of
/// 01M594XQJMXAXY3JXP66Y8EVQ7). `lines` are the messages of the thread
/// `chat`. `may` tells whether a USER is the owner or an admin.
///
/// The release starts at the first verified line of a session that says
/// `riff release TAG is cut` or `is published`. After it, a verified
/// message of a person who `may` and says `approve`, `approved` or
/// `deploy` counts when it names the version, with or without the `v`.
/// With no version, it counts when the last such line of a session
/// before it is of this tag: the owner answers the line of the lead.
/// A message that denies (`not`, `never`, `don't`) does not count. A
/// message of the owner or of an admin with `stop` or `wait` after the
/// approval cancels it.
///
/// ```
/// use riff::cycle::{owner_said, ChatLine};
///
/// let line = |seq, session, body: &str| ChatLine {
///     seq, user: "mike".into(), session, verified: true, body: body.into(),
/// };
/// let may = |user: &str| user == "mike";
/// let lines = [
///     line(1, true, "riff release v3.3.0 is cut. Say approve 3.3.0"),
///     line(2, false, "approve 3.3.0"),
///     line(3, false, "approved"),
/// ];
/// assert_eq!(owner_said(&lines, "v3.3.0", &may), Some(3));
/// assert_eq!(owner_said(&lines, "v3.3.1", &may), None);
/// let stopped = [lines[0].clone(), lines[1].clone(), line(3, false, "stop")];
/// assert_eq!(owner_said(&stopped, "v3.3.0", &may), None);
/// ```
pub fn owner_said(lines: &[ChatLine], tag: &str, may: &dyn Fn(&str) -> bool) -> Option<u64> {
    let release = |l: &ChatLine, tag: &str| {
        l.verified
            && l.session
            && (l.body.contains(&format!("riff release {tag} is cut"))
                || l.body.contains(&format!("riff release {tag} is published")))
    };
    let start = lines
        .iter()
        .filter(|l| release(l, tag))
        .map(|l| l.seq)
        .min()?;
    // The tag of the last line of a session about a release, before `seq`.
    let asked = |seq: u64| {
        lines
            .iter()
            .filter(|l| l.seq < seq && l.verified && l.session)
            .filter(|l| l.body.contains(" is cut") || l.body.contains(" is published"))
            .max_by_key(|l| l.seq)
            .is_some_and(|l| release(l, tag))
    };
    let owner = |l: &ChatLine| l.verified && !l.session && may(&l.user);
    let approved = lines
        .iter()
        .filter(|l| l.seq > start && owner(l))
        .filter(|l| match approves(&l.body, tag) {
            Some(named) => named || asked(l.seq),
            None => false,
        })
        .map(|l| l.seq)
        .max()?;
    let stopped = lines
        .iter()
        .any(|l| l.seq > approved && owner(l) && says_stop(&l.body));
    (!stopped).then_some(approved)
}

/// One line of the log of the Cloud Run service of the shared riff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// Its time.
    pub at: DateTime<Utc>,
    /// The revision of the instance that wrote it.
    pub revision: String,
}

/// The log of the Cloud Run service of the shared riff. [`RunLog`] reads
/// it with `gcloud logging read`. A test gives a fake.
pub trait CloudLog {
    /// Each line of the log that `filter` matches, the oldest first.
    fn lines(&self, filter: &str) -> Result<Vec<LogLine>>;
}

/// The facts of riff-server for [`check`]: the build that `riff server`
/// shows, and the build of the daemon of each host, as the part `Hosts`
/// of `riff who` shows it. An error is the reason that riff cannot tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Served {
    /// The version of the build of the shared riff.
    pub server: Result<String, String>,
    /// `USER@HOST` and the build of its daemon, for each host.
    pub hosts: Result<Vec<(String, String)>, String>,
}

/// The result of one check after a deploy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The check passed.
    Passed,
    /// The check failed, and why.
    Failed(String),
    /// riff could not do the check, and why.
    Unknown(String),
}

/// The longest gap between two lines of one run of 503s.
pub const RUN_GAP: i64 = 5;

/// The longest time of a run of 503s that is no fault: Cloud Run moves
/// the calls to the new instance in this time.
pub const LONGEST_503: i64 = 20;

/// The seconds of the longest run of `lines`: lines with at most
/// [`RUN_GAP`] seconds between two of them.
///
/// ```
/// use riff::cycle::{LogLine, longest_run};
///
/// let at = |s: i64| LogLine { at: chrono::DateTime::from_timestamp(s, 0).unwrap(), revision: "r".into() };
/// assert_eq!(longest_run(&[]), 0);
/// assert_eq!(longest_run(&[at(0), at(4), at(8), at(30), at(31)]), 8);
/// ```
pub fn longest_run(lines: &[LogLine]) -> i64 {
    let mut longest = 0;
    let mut start = None;
    let mut last: Option<DateTime<Utc>> = None;
    for line in lines {
        let begin = match (start, last) {
            (Some(s), Some(l)) if (line.at - l).num_seconds() <= RUN_GAP => s,
            _ => line.at,
        };
        longest = longest.max((line.at - begin).num_seconds());
        start = Some(begin);
        last = Some(line.at);
    }
    longest
}

/// The 500 replies of `lines` that count: the lines of the newest
/// revision `newest`, the revision of the last line "riff-server
/// serves". An instance of an older revision that starts in the switch
/// to the new one sees the lease of the new one and stops before it
/// serves, so a call that reaches it gets a 500. With no newest
/// revision, each line counts (01M5MQHDMBYG187T0Q622XWBMN).
///
/// ```
/// use riff::cycle::{LogLine, counted_500s};
///
/// let at = |s: i64, r: &str| LogLine { at: chrono::DateTime::from_timestamp(s, 0).unwrap(), revision: r.into() };
/// let lines = [at(1, "old"), at(2, "new")];
/// assert_eq!(counted_500s(&lines, Some("new")), 1);
/// assert_eq!(counted_500s(&lines[..1], Some("new")), 0);
/// assert_eq!(counted_500s(&lines, None), 2);
/// ```
pub fn counted_500s(lines: &[LogLine], newest: Option<&str>) -> usize {
    lines
        .iter()
        .filter(|l| newest.is_none_or(|n| l.revision == n))
        .count()
}

/// True when an instance of an older revision took the lease after the
/// newest revision served: a line "riff-server serves" of another
/// revision after the first line of the newest one. The newest revision
/// is the revision of the last line.
///
/// ```
/// use riff::cycle::{LogLine, old_took_lease};
///
/// let at = |s: i64, r: &str| LogLine { at: chrono::DateTime::from_timestamp(s, 0).unwrap(), revision: r.into() };
/// assert!(!old_took_lease(&[at(0, "old"), at(5, "new")]));
/// assert!(old_took_lease(&[at(5, "new"), at(9, "old"), at(12, "new")]));
/// ```
pub fn old_took_lease(serves: &[LogLine]) -> bool {
    let Some(newest) = serves.last().map(|l| l.revision.as_str()) else {
        return false;
    };
    let Some(first) = serves.iter().position(|l| l.revision == newest) else {
        return false;
    };
    serves[first..].iter().any(|l| l.revision != newest)
}

/// The filters of the checks of the log after the deploy at `at`.
fn log_filter(at: &str, part: &str) -> String {
    format!("timestamp>=\"{at}\" AND {part}")
}

/// The step of a check that could not read the Cloud Run log: it names the
/// cause and its fix. Either `gcloud` is not on PATH (install it), or its
/// sign-in ended (`gcloud auth login`).
fn unreadable_log(e: &anyhow::Error, version: Semver) -> String {
    let again = format!("then run: riff release check {}", tag(version));
    if e.is::<NoGcloud>() {
        format!(
            "riff cannot read the Cloud Run log: gcloud is not on PATH. A person on the host \
             installs gcloud or adds it to PATH, {again}"
        )
    } else {
        format!(
            "riff cannot read the Cloud Run log: {e:#}. The sign-in of gcloud may have ended \
             (check: gcloud auth list). A person on the host runs: gcloud auth login, {again}"
        )
    }
}

/// The checks after the deploy of `version` at `at`
/// (01M58KMM5ZPAQ289S2YJVQ5WW6, 01M58KMMP9FKRFYX58ESVRSZQX): each check by
/// its name, and its verdict. A log that riff cannot read gives
/// [`Verdict::Unknown`]: it says which check it could not do.
pub fn checks(
    version: Semver,
    at: &str,
    served: &Served,
    log: &dyn CloudLog,
) -> Vec<(&'static str, Verdict)> {
    let want = version.to_string();
    let mut out = Vec::new();
    out.push((
        "riff server shows the build",
        match &served.server {
            Ok(build) if *build == want => Verdict::Passed,
            Ok(build) => Verdict::Failed(format!("riff server shows {build}, not {want}")),
            Err(e) => Verdict::Unknown(format!("riff cannot ask the shared riff: {e}")),
        },
    ));
    let read = |part: &str| log.lines(&log_filter(at, part));
    let serves = read("jsonPayload.message:\"riff-server serves\"");
    let newest = serves
        .as_ref()
        .ok()
        .and_then(|lines| lines.last())
        .map(|l| l.revision.clone());
    let unknown = |e: anyhow::Error| Verdict::Unknown(unreadable_log(&e, version));
    out.push((
        "no run of 503 longer than 20 s",
        match read("httpRequest.status=503") {
            Ok(lines) => match longest_run(&lines) {
                s if s > LONGEST_503 => Verdict::Failed(format!("a run of 503 took {s} s")),
                _ => Verdict::Passed,
            },
            Err(e) => unknown(e),
        },
    ));
    out.push((
        "no 500 of riff-server",
        match read("httpRequest.status=500 AND NOT textPayload:\"no available instance\"") {
            Ok(lines) => match counted_500s(&lines, newest.as_deref()) {
                0 => Verdict::Passed,
                n => Verdict::Failed(format!("{n} replies 500")),
            },
            Err(e) => unknown(e),
        },
    ));
    out.push((
        "no 401 or 429 of the token store",
        match read(
            "jsonPayload.message:\"token store\" AND (jsonPayload.message:\"401\" OR \
             jsonPayload.message:\"429\")",
        ) {
            Ok(lines) if lines.is_empty() => Verdict::Passed,
            Ok(lines) => Verdict::Failed(format!("{} lines", lines.len())),
            Err(e) => unknown(e),
        },
    ));
    out.push((
        "no old instance takes the lease",
        match serves {
            Ok(lines) if old_took_lease(&lines) => {
                Verdict::Failed("an older revision served after the new one".to_owned())
            }
            Ok(_) => Verdict::Passed,
            Err(e) => unknown(e),
        },
    ));
    out.push((
        "each host takes the build",
        match &served.hosts {
            Ok(hosts) => {
                let old: Vec<String> = hosts
                    .iter()
                    .filter(|(_, build)| !same_version(build, &want))
                    .map(|(host, build)| format!("{host} has {build}"))
                    .collect();
                if old.is_empty() {
                    Verdict::Passed
                } else {
                    Verdict::Failed(old.join(", "))
                }
            }
            Err(e) => Verdict::Unknown(format!("riff cannot ask the shared riff: {e}")),
        },
    ));
    out
}

/// True when `build`, the build line of the daemon of a host, has the
/// version `want`. The facts of a daemon give the whole build line; a
/// line that is only a version still counts. The commit and the time of
/// the build do not count (01M5GSDJ3YWANWEZNZ6VSDC1BE).
///
/// ```
/// use riff::cycle::same_version;
///
/// assert!(same_version("4.0.0 bbf9227fd8cf 2026-10-22T02:02:45Z", "4.0.0"));
/// assert!(same_version("4.0.0", "4.0.0"));
/// assert!(!same_version("3.3.0 929605821e54 2026-10-18T23:57:37Z", "4.0.0"));
/// ```
pub fn same_version(build: &str, want: &str) -> bool {
    match build.parse::<riff_core::build::Build>() {
        Ok(b) => b.version == want,
        Err(_) => build == want,
    }
}

/// The log of the Cloud Run service of an instance of riff, read with
/// `gcloud logging read`.
pub struct RunLog {
    /// The `gcloud` of this machine.
    pub gcloud: crate::cloud::Gcloud,
    /// The settings of the instance, for example `shared`.
    pub settings: crate::cloud::Settings,
}

/// The arguments of `gcloud` that read the lines of `filter` of the
/// service `service` in `project`, with the time and the revision of
/// each line.
///
/// ```
/// let a = riff::cycle::log_read_args("riff", "como-riff", "httpRequest.status=500");
/// assert_eq!(a[..2], ["logging", "read"]);
/// assert!(a[2].contains("resource.labels.service_name=\"riff\" AND (httpRequest.status=500)"));
/// assert!(a.contains(&"--order=asc".to_owned()));
/// ```
pub fn log_read_args(service: &str, project: &str, filter: &str) -> Vec<String> {
    vec![
        "logging".into(),
        "read".into(),
        format!(
            "resource.type=\"cloud_run_revision\" AND resource.labels.service_name=\"{service}\" \
             AND ({filter})"
        ),
        "--project".into(),
        project.into(),
        "--limit".into(),
        "1000".into(),
        "--order=asc".into(),
        "--format=value(timestamp,resource.labels.revision_name)".into(),
    ]
}

/// The error of a [`CloudLog`] when `gcloud` does not run on this
/// machine, for example in the container of the lead
/// (01M5GVPND32X3R9MYXYB5SV83W).
#[derive(Debug)]
pub struct NoGcloud;

impl std::fmt::Display for NoGcloud {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("gcloud does not run here")
    }
}

impl std::error::Error for NoGcloud {}

impl CloudLog for RunLog {
    fn lines(&self, filter: &str) -> Result<Vec<LogLine>> {
        let args = log_read_args(&self.settings.service, &self.settings.project, filter);
        let out = self.gcloud.run(&args, None).map_err(|e| {
            if gcloud_missing(&e) {
                anyhow!(NoGcloud)
            } else {
                e
            }
        })?;
        log_lines(&out)
    }
}

/// True when `e` is the error of a `gcloud` that is not on this machine.
pub fn gcloud_missing(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
    })
}

/// The lines of the output of `gcloud logging read` with the arguments of
/// [`log_read_args`]: the time and the revision of each line.
///
/// ```
/// let lines = riff::cycle::log_lines("2026-10-22T10:00:00Z riff-00001\n\n")?;
/// assert_eq!(lines[0].revision, "riff-00001");
/// assert!(riff::cycle::log_lines("no time\n").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn log_lines(out: &str) -> Result<Vec<LogLine>> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut parts = l.split_whitespace();
            let at = parts.next().unwrap_or_default();
            let at = DateTime::parse_from_rfc3339(at)
                .with_context(|| format!("gcloud printed no time: {}", text::forge(l)))?;
            Ok(LogLine {
                at: at.with_timezone(&Utc),
                revision: text::forge(parts.next().unwrap_or_default()),
            })
        })
        .collect()
}

/// What [`check`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checked {
    /// The job `Deploy` failed: the old revision still serves. riff filed
    /// the issue `issue` with the log.
    DeployFailed {
        /// The run.
        deploy: Deploy,
        /// The new issue.
        issue: u64,
    },
    /// The checks ran. `passed` is true when each passed; then riff
    /// closed the release issue.
    Ran {
        /// Each check and its verdict.
        lines: Vec<(&'static str, Verdict)>,
        /// True when each check passed.
        passed: bool,
        /// The release issue, with the result.
        issue: u64,
    },
}

/// The step that the other leads hear for the result of [`checks`]
/// (01M5TMZE0AAER3F5Q8DT3K0AJJ): [`Step::Checked`] when each check passed,
/// [`Step::CheckFailed`] when one check failed, and [`Step::CheckUnknown`]
/// when no check failed and one or more could not run.
///
/// ```
/// use riff::cycle::{Verdict, check_step};
/// use riff::hold::Step;
///
/// let passed = ("a", Verdict::Passed);
/// let failed = ("b", Verdict::Failed("1 replies 500".into()));
/// let unknown = ("c", Verdict::Unknown("no log".into()));
/// assert_eq!(check_step(&[passed.clone()]), Step::Checked);
/// assert_eq!(check_step(&[passed.clone(), unknown.clone()]), Step::CheckUnknown);
/// assert_eq!(check_step(&[passed, unknown, failed.clone()]), Step::CheckFailed);
/// assert_eq!(check_step(&[failed]), Step::CheckFailed);
/// ```
pub fn check_step(lines: &[(&'static str, Verdict)]) -> Step {
    if lines.iter().all(|(_, v)| *v == Verdict::Passed) {
        Step::Checked
    } else if lines.iter().any(|(_, v)| matches!(v, Verdict::Failed(_))) {
        Step::CheckFailed
    } else {
        Step::CheckUnknown
    }
}

/// The text of the result of [`checks`], for the release issue.
///
/// ```
/// use riff::cycle::{Verdict, check_text};
///
/// let text = check_text("v3.3.0", &[("a", Verdict::Passed), ("b", Verdict::Unknown("no log".into()))]);
/// assert_eq!(text, "riff release check v3.3.0: unknown\n\n- a: passed\n- b: riff could not check it: no log\n");
/// ```
pub fn check_text(tag: &str, lines: &[(&'static str, Verdict)]) -> String {
    let result = match check_step(lines) {
        Step::Checked => "passed",
        Step::CheckFailed => "failed",
        _ => "unknown",
    };
    let mut out = format!("{}\n\n", check_mark(tag, result));
    for (name, verdict) in lines {
        let said = match verdict {
            Verdict::Passed => "passed".to_owned(),
            Verdict::Failed(why) => format!("failed: {why}"),
            Verdict::Unknown(why) => format!("riff could not check it: {why}"),
        };
        out.push_str(&format!("- {name}: {said}\n"));
    }
    out
}

/// The checks after the deploy of `tag` (01M58KMM5ZPAQ289S2YJVQ5WW6). It
/// refuses while the deploy did not end. After a failed job `Deploy`,
/// it files an issue with the failed jobs and the run in the current
/// wave. Else it runs [`checks`], and writes the result on the release
/// issue. When each check passed, it closes the release issue. The
/// caller resumes the riff after a failed deploy, and after a pass.
pub fn check(
    forge: &dyn Forge,
    tag_text: &str,
    served: &Served,
    log: &dyn CloudLog,
) -> Result<Checked> {
    let version = tag_version(tag_text)?;
    let tag = tag(version);
    let Some(deploy) = forge.deploy(&tag)? else {
        bail!("riff release check: no CI run deploys {tag}. next: riff release status");
    };
    let wave = forge.current_wave()?;
    let issue = match forge.issue_of(&title(version))? {
        Some(issue) => issue.number,
        None => forge.release_issue(&title(version), &wave, &issue_body(version))?,
    };
    match deploy.state {
        DeployState::Failed => {
            let jobs = if deploy.failed.is_empty() {
                "none named".to_owned()
            } else {
                deploy.failed.join(", ")
            };
            let body = format!(
                "The CI run {} of {tag} failed. The old revision still serves.\n\nThe failed jobs: \
                 {jobs}.\n\nDone when: the deploy of {tag} passed, or an older release serves.\n\n\
                 Part of #{issue}.\n",
                deploy.url
            );
            let filed = forge.file_issue(&format!("The deploy of {tag} failed"), &body, &wave)?;
            forge.comment(
                issue,
                &format!(
                    "{}\n\nThe CI run {} failed: {jobs}. The old revision still serves. See #{filed}.\n",
                    check_mark(&tag, "failed deploy"),
                    deploy.url
                ),
            )?;
            Ok(Checked::DeployFailed {
                deploy,
                issue: filed,
            })
        }
        DeployState::Passed => {
            let lines = checks(version, &deploy.at, served, log);
            let passed = lines.iter().all(|(_, v)| *v == Verdict::Passed);
            forge.comment(issue, &check_text(&tag, &lines))?;
            if passed {
                forge.close(issue)?;
            }
            Ok(Checked::Ran {
                lines,
                passed,
                issue,
            })
        }
        other => {
            let state = match other {
                DeployState::Waiting => State::Waiting,
                DeployState::Running => State::Deploying,
                _ => State::Published,
            };
            let stage = Stage {
                version,
                state,
                pr: None,
                merge: None,
                deploy: Some(deploy),
                checks_failed: false,
                test: None,
                previous: None,
            };
            bail!(
                "riff release check: the deploy of {tag} did not end: it is {}. next: {}",
                state.word(),
                next(&stage)
            );
        }
    }
}

/// Runs the failed jobs of the CI run of `tag` again
/// (01M58KMM8CV9731AWGVZQBB80D). It refuses when the run did not fail.
/// The job `Deploy` then waits for a person again.
pub fn retry(forge: &dyn Forge, tag_text: &str) -> Result<Deploy> {
    let tag = tag(tag_version(tag_text)?);
    let Some(deploy) = forge.deploy(&tag)? else {
        bail!("riff release retry: no CI run deploys {tag}. next: riff release status");
    };
    if deploy.state != DeployState::Failed {
        bail!(
            "riff release retry: the CI run {} of {tag} did not fail. riff changed nothing. \
             next: riff release status",
            deploy.url
        );
    }
    forge.rerun(deploy.run)?;
    Ok(deploy)
}

/// What [`rollback`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolledBack {
    /// The release that the older one replaces.
    pub from: Semver,
    /// The older release that deploys now.
    pub to: Semver,
}

/// Starts the deploy of the older release `tag` (01M58KMMAPJ33CQWR8GRX62GMR):
/// the CI workflow on `main` with the input `tag`. It refuses a tag that
/// is not a published release older than the newest one. It writes the
/// rollback on the release issue of the newest release. The job `Deploy`
/// then waits for a person: HUMAN GATE 2.
pub fn rollback(forge: &dyn Forge, tag_text: &str) -> Result<RolledBack> {
    let to = tag_version(tag_text)?;
    let releases = forge.releases()?;
    let Some(from) = last(&releases) else {
        bail!(
            "riff release rollback: the forge has no published release. next: riff release status"
        );
    };
    if !releases.contains(&to) || to >= from {
        bail!(
            "riff release rollback: {} is not a published release older than {}. riff changed \
             nothing. next: riff release status",
            tag(to),
            tag(from)
        );
    }
    forge.dispatch(&tag(to))?;
    let issue = match forge.issue_of(&title(from))? {
        Some(issue) => issue.number,
        None => forge.release_issue(&title(from), &forge.current_wave()?, &issue_body(from))?,
    };
    forge.comment(
        issue,
        &format!(
            "{}\n\nThe deploy of {} replaces {}. It waits for the approval of a person: riff \
             release approve {}.\n",
            rollback_mark(&tag(to)),
            tag(to),
            tag(from),
            tag(to)
        ),
    )?;
    Ok(RolledBack { from, to })
}

/// What [`wave_gate`] decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaveGate {
    /// Each merged item of the wave is in a checked release.
    Released,
    /// The wave ends with no release, for the reason in the wave.
    NoRelease,
}

/// The release gate of `riff wave end N` (01M58KMMCZCE7HDNVT9PVHVZX2).
/// It refuses while a merged pull request of the wave is in no checked
/// release, and names the next step. With `no_release`, a wave whose
/// items are in no release ends: riff writes the reason in the wave. A
/// release that is not checked always stops the end.
pub fn wave_gate(forge: &dyn Forge, number: u64, no_release: Option<&str>) -> Result<WaveGate> {
    let wave = forge.wave_title(number)?;
    let merged: Vec<Merged> = forge
        .merged_since(None)?
        .into_iter()
        .filter(|m| !m.release && m.wave.as_deref() == Some(wave.as_str()))
        .collect();
    let mut checked: Vec<(Semver, Option<Stage>)> = Vec::new();
    let mut unreleased = Vec::new();
    let mut open: Option<Stage> = None;
    for m in &merged {
        let versions = forge.released_in(&m.commit)?;
        if versions.is_empty() {
            unreleased.push(m.number);
            continue;
        }
        let mut stages = Vec::new();
        for v in &versions {
            let known = checked.iter().find(|(c, _)| c == v).map(|(_, s)| s.clone());
            let s = match known {
                Some(s) => s,
                None => {
                    let s = stage(forge, *v)?;
                    checked.push((*v, s.clone()));
                    s
                }
            };
            stages.push(s);
        }
        let done = stages
            .iter()
            .any(|s| s.as_ref().is_some_and(|s| s.state == State::Checked));
        if !done && open.is_none() {
            open = stages.into_iter().flatten().next();
        }
    }
    if let Some(stage) = open {
        bail!(
            "riff wave end {number}: the release {} of the wave is {}, not checked. next: {}",
            tag(stage.version),
            stage.state.word(),
            next(&stage)
        );
    }
    if unreleased.is_empty() {
        return Ok(WaveGate::Released);
    }
    match no_release.map(str::trim).filter(|r| !r.is_empty()) {
        Some(reason) => {
            forge.note_wave(number, &format!("No release: {reason}"))?;
            Ok(WaveGate::NoRelease)
        }
        None => {
            let list: Vec<String> = unreleased.iter().map(|n| format!("#{n}")).collect();
            bail!(
                "riff wave end {number}: these merged pull requests of the wave are in no \
                 release: {}. next: riff release cut X.Y.Z. When no change of the wave is one \
                 that a person sees: riff wave end {number} --no-release TEXT",
                list.join(", ")
            );
        }
    }
}

/// The process that runs a command of the release cycle
/// (01M58B0JSQMWNCMSNX19536XT8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Caller {
    /// A worker: `RIFF_WORKER=1`.
    pub worker: bool,
    /// A container session: it has the socket of the riff daemon.
    pub container: bool,
    /// A container session that the daemon says is the lead: it has a
    /// terminal. A work session cannot be one, because the daemon
    /// answers by the socket of the session (01M59269HHZ5C82DR3RG3QHAM3).
    pub lead_container: bool,
    /// An agent session: it has a session ID, or runs under Claude Code.
    pub agent: bool,
    /// For an agent session: `Some(true)` when it is the lead of its
    /// user, `None` when riff cannot tell.
    pub lead: Option<bool>,
    /// The role of its user in the riff: of the agent session, or else
    /// of the person in the terminal. `None` in a riff with no sign-in: it
    /// has no people, and it trusts its network.
    pub role: Option<PersonRole>,
    /// True when riff could not ask riff-server for the role of the
    /// person in the terminal.
    pub role_unknown: bool,
}

/// Why a caller may not run a command of the release cycle. The text is
/// [`text::release_refused`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// A worker.
    Worker,
    /// A container session.
    Container,
    /// An agent session that is not the lead of its user.
    NotTheLead,
    /// An agent session, and riff cannot ask riff-server for the lead.
    LeadUnknown,
    /// The user is a member: not the owner, not an admin.
    Member,
    /// riff cannot ask riff-server for the role of the person.
    RoleUnknown,
    /// An agent session, for a step of a person.
    Agent,
    /// The lead, for `riff release approve`: riff chat has no verified
    /// message of the owner that says approve for the version.
    OwnerSilent,
}

impl Caller {
    /// Why this caller may not run a step of the lead: `riff release
    /// publish`, `check`, `retry` and `rollback`, or `None`
    /// (01M58KMMF9WMH39ZMD0MKGRVZB). A person in a plain terminal, or the
    /// lead of its user, may when the user is the owner or an admin of
    /// the riff, or when the riff has no sign-in.
    ///
    /// ```
    /// use riff::cycle::{Caller, Refusal};
    /// use riff_core::wire::PersonRole;
    ///
    /// assert!(Caller::default().lead_step().is_none());
    /// let person = Caller { role: Some(PersonRole::Member), ..Caller::default() };
    /// assert_eq!(person.lead_step(), Some(Refusal::Member));
    /// let lead = Caller { agent: true, lead: Some(true), ..Caller::default() };
    /// assert!(lead.lead_step().is_none());
    /// let admin = Caller { role: Some(PersonRole::Admin), ..lead };
    /// assert!(admin.lead_step().is_none());
    /// let member = Caller { role: Some(PersonRole::Member), ..lead };
    /// assert_eq!(member.lead_step(), Some(Refusal::Member));
    /// let worker = Caller { worker: true, agent: true, lead: Some(false), ..Caller::default() };
    /// assert_eq!(worker.lead_step(), Some(Refusal::Worker));
    /// ```
    pub fn lead_step(&self) -> Option<Refusal> {
        if self.worker {
            Some(Refusal::Worker)
        } else if self.container && !self.lead_container {
            Some(Refusal::Container)
        } else if !self.agent {
            if self.role_unknown {
                Some(Refusal::RoleUnknown)
            } else if self.role == Some(PersonRole::Member) {
                Some(Refusal::Member)
            } else {
                None
            }
        } else {
            match self.lead {
                Some(true) if self.role_unknown => Some(Refusal::RoleUnknown),
                Some(true) if self.role == Some(PersonRole::Member) => Some(Refusal::Member),
                Some(true) => None,
                Some(false) => Some(Refusal::NotTheLead),
                None => Some(Refusal::LeadUnknown),
            }
        }
    }

    /// Why this caller may not run `riff release test`, or `None`
    /// (01M5924C8BTT6CW05Q0YZJ1GCY). The lead runs it in a container,
    /// where podman does not run: so riff refuses only a worker here,
    /// and the daemon of the host judges the rest. It acts only on the
    /// person of its user and the lead of that person.
    ///
    /// ```
    /// use riff::cycle::{Caller, Refusal};
    ///
    /// let lead = Caller { container: true, agent: true, ..Caller::default() };
    /// assert!(lead.test_step().is_none());
    /// let worker = Caller { worker: true, ..lead };
    /// assert_eq!(worker.test_step(), Some(Refusal::Worker));
    /// ```
    pub fn test_step(&self) -> Option<Refusal> {
        self.worker.then_some(Refusal::Worker)
    }

    /// Why this caller may not run `riff release hold`, or `None`
    /// (01M5SNV78EBAYY4SZ891DM2MQG). Only the lead of its user may, and
    /// the lead of a member may too: a hold needs no role. A person in a
    /// plain terminal may not, because a hold names the lead of a
    /// repository.
    ///
    /// ```
    /// use riff::cycle::{Caller, Refusal};
    /// use riff_core::wire::PersonRole;
    ///
    /// let lead = Caller { agent: true, lead: Some(true), ..Caller::default() };
    /// assert!(lead.hold_step().is_none());
    /// let member = Caller { role: Some(PersonRole::Member), ..lead };
    /// assert!(member.hold_step().is_none());
    /// let worker = Caller { worker: true, agent: true, lead: Some(false), ..Caller::default() };
    /// assert_eq!(worker.hold_step(), Some(Refusal::Worker));
    /// let container = Caller { container: true, agent: true, ..Caller::default() };
    /// assert_eq!(container.hold_step(), Some(Refusal::Container));
    /// let other = Caller { agent: true, lead: Some(false), ..Caller::default() };
    /// assert_eq!(other.hold_step(), Some(Refusal::NotTheLead));
    /// let unknown = Caller { agent: true, ..Caller::default() };
    /// assert_eq!(unknown.hold_step(), Some(Refusal::LeadUnknown));
    /// assert_eq!(Caller::default().hold_step(), Some(Refusal::NotTheLead));
    /// ```
    pub fn hold_step(&self) -> Option<Refusal> {
        if self.worker {
            Some(Refusal::Worker)
        } else if self.container && !self.lead_container {
            Some(Refusal::Container)
        } else if self.lead_container {
            None
        } else if !self.agent {
            Some(Refusal::NotTheLead)
        } else {
            match self.lead {
                Some(true) => None,
                Some(false) => Some(Refusal::NotTheLead),
                None => Some(Refusal::LeadUnknown),
            }
        }
    }

    /// Why this caller may not run a step of a person: `riff release
    /// approve`, and `riff release cut --skip`, or `None`
    /// (01M58B0JSQMWNCMSNX19536XT8, 01M58KMMKZ8T11KT3R3Z7D0RE1). Only a
    /// person in a plain terminal may: never an agent session, also not
    /// the lead. A member may not, when riff knows the role.
    ///
    /// ```
    /// use riff::cycle::{Caller, Refusal};
    /// use riff_core::wire::PersonRole;
    ///
    /// assert!(Caller::default().person_step().is_none());
    /// let lead = Caller { agent: true, lead: Some(true), ..Caller::default() };
    /// assert_eq!(lead.person_step(), Some(Refusal::Agent));
    /// let member = Caller { role: Some(PersonRole::Member), ..Caller::default() };
    /// assert_eq!(member.person_step(), Some(Refusal::Member));
    /// ```
    pub fn person_step(&self) -> Option<Refusal> {
        if self.worker || self.container || self.agent {
            Some(Refusal::Agent)
        } else if self.role_unknown {
            Some(Refusal::RoleUnknown)
        } else if self.role == Some(PersonRole::Member) {
            Some(Refusal::Member)
        } else {
            None
        }
    }
}

/// True when this process runs in an agent session or under Claude
/// Code: a `!` command of Claude Code is one.
pub fn in_agent() -> bool {
    crate::identity::session_id().is_some()
        || std::env::var_os("CLAUDECODE").is_some_and(|v| !v.is_empty())
}

/// True when this process runs in a container session.
pub fn in_container() -> bool {
    std::env::var_os(crate::daemon::SOCKET_VAR).is_some_and(|s| !s.is_empty())
}

/// The forge GitHub, with the `gh` and the `git` of the machine, in the
/// clone `dir`. It fetches `origin` first.
pub struct GitHub {
    gh: Gh,
    dir: PathBuf,
    /// The waits for the CI run of a new tag.
    run_waits: Vec<Duration>,
}

impl GitHub {
    /// The forge of the clone `dir`. It fetches `origin` and its tags.
    pub fn new(gh: Gh, dir: &Path) -> Result<Self> {
        let forge = Self::at(gh, dir);
        forge.git(&["fetch", "-q", "--tags", "--force", "origin"])?;
        Ok(forge)
    }

    /// The forge of the clone `dir`, with no fetch: for a read that
    /// needs only `gh`.
    pub fn at(gh: Gh, dir: &Path) -> Self {
        Self {
            gh,
            dir: dir.to_owned(),
            run_waits: vec![Duration::from_secs(5); 6],
        }
    }

    fn git_in(&self, dir: &Path, args: &[&str]) -> Result<String> {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .with_context(|| format!("cannot run git {}", args.join(" ")))?;
        if !out.status.success() {
            bail!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        self.git_in(&self.dir, args)
    }

    /// The changes of the branch of `version`, in the worktree `tree`.
    fn bump(&self, tree: &Path, version: Semver) -> Result<()> {
        let edit = |name: &str, change: &dyn Fn(&str) -> Result<String>| -> Result<()> {
            let path = tree.join(name);
            let old =
                std::fs::read_to_string(&path).with_context(|| format!("cannot read {name}"))?;
            std::fs::write(&path, change(&old)?).with_context(|| format!("cannot write {name}"))
        };
        edit("Cargo.toml", &|t| set_cargo(t, version))?;
        edit("Cargo.lock", &|t| Ok(set_lock(t, version)))?;
        let check = tree.join(CHECK);
        let out = Command::new(&check)
            .arg(tag(version))
            .arg(tree)
            .output()
            .with_context(|| format!("cannot run {CHECK}"))?;
        if !out.status.success() {
            bail!(
                "riff release cut: {CHECK} {} failed: {}",
                tag(version),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let title = title(version);
        self.git_in(tree, &["commit", "-q", "-a", "-m", &title])?;
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRelease {
    tag_name: String,
}

#[derive(Deserialize)]
struct GhMilestone {
    title: String,
}

#[derive(Deserialize)]
struct GhLabel {
    name: String,
}

#[derive(Deserialize)]
struct GhCommit {
    oid: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPr {
    number: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    milestone: Option<GhMilestone>,
    #[serde(default)]
    labels: Vec<GhLabel>,
    #[serde(default)]
    merge_commit: Option<GhCommit>,
    #[serde(default)]
    head_ref_oid: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    files: Vec<GhFile>,
}

#[derive(Deserialize)]
struct GhFile {
    path: String,
}

#[derive(Deserialize)]
struct GhIssue {
    number: u64,
    title: String,
}

#[derive(Deserialize)]
struct GhOpenIssue {
    #[serde(default)]
    title: String,
    #[serde(default)]
    labels: Vec<GhLabel>,
}

#[derive(Deserialize)]
struct GhStatus {
    context: String,
    state: String,
}

#[derive(Deserialize)]
struct GhStatuses {
    #[serde(default)]
    statuses: Vec<GhStatus>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRun {
    database_id: u64,
    #[serde(default)]
    head_branch: String,
    #[serde(default)]
    head_sha: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    display_title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    conclusion: String,
}

impl GhRun {
    /// The release tag that the run deploys: its branch for the push of a
    /// tag, else the tag of its name `Deploy vX.Y.Z`, the name of a run by
    /// hand with the input `tag`.
    fn tag(&self) -> Option<String> {
        let name = self
            .display_title
            .strip_prefix("Deploy ")
            .unwrap_or_default();
        [self.head_branch.as_str(), name]
            .into_iter()
            .find(|t| tag_version(t).is_ok())
            .map(text::forge)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhJob {
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    conclusion: String,
    #[serde(default)]
    started_at: String,
}

#[derive(Deserialize)]
struct GhJobs {
    #[serde(default)]
    jobs: Vec<GhJob>,
}

#[derive(Deserialize)]
struct GhComment {
    #[serde(default)]
    body: String,
}

#[derive(Deserialize)]
struct GhFullIssue {
    number: u64,
    title: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    comments: Vec<GhComment>,
}

#[derive(Deserialize)]
struct GhEnvironment {
    id: u64,
    name: String,
}

#[derive(Deserialize)]
struct GhPending {
    environment: GhEnvironment,
}

/// The number at the end of a URL that `gh … create` prints.
fn number_of(url: &str) -> Result<u64> {
    url.trim()
        .rsplit('/')
        .next()
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| {
            anyhow!(
                "gh printed no URL with a number: {}",
                text::forge(url.trim())
            )
        })
}

impl Forge for GitHub {
    fn main_version(&self) -> Result<Semver> {
        cargo_version(&self.git(&["show", "origin/main:Cargo.toml"])?)
    }

    fn releases(&self) -> Result<Vec<Semver>> {
        let list: Vec<GhRelease> = self.gh.json(&[
            "release",
            "list",
            "--exclude-drafts",
            "--limit",
            "200",
            "--json",
            "tagName",
        ])?;
        Ok(list
            .iter()
            .filter_map(|r| tag_version(&r.tag_name).ok())
            .collect())
    }

    fn current_wave(&self) -> Result<String> {
        crate::waves::open(&self.gh)?
            .into_iter()
            .find(|m| hygiene::wave_number(&m.title).is_some())
            .map(|m| m.title)
            .context("riff release cut: the repository has no open wave")
    }

    fn release_issue(&self, title: &str, wave: &str, body: &str) -> Result<u64> {
        let search = format!("\"{title}\" in:title");
        let found: Vec<GhIssue> = self.gh.json(&[
            "issue",
            "list",
            "--state",
            "open",
            "--label",
            LABEL,
            "--search",
            &search,
            "--json",
            "number,title",
        ])?;
        if let Some(issue) = found.iter().find(|i| i.title == title) {
            return Ok(issue.number);
        }
        let url = self.gh.run(
            &[
                "issue",
                "create",
                "--title",
                title,
                "--label",
                LABEL,
                "--milestone",
                wave,
                "--body-file",
                "-",
            ],
            Some(body),
        )?;
        number_of(&url)
    }

    fn merged_since(&self, since: Option<&str>) -> Result<Vec<Merged>> {
        let range = match since {
            Some(tag) => format!("{tag}..origin/main"),
            None => "origin/main".to_owned(),
        };
        let commits = self.git(&["rev-list", &range])?;
        let commits: Vec<&str> = commits.lines().collect();
        let list: Vec<GhPr> = self.gh.json(&[
            "pr",
            "list",
            "--state",
            "merged",
            "--base",
            "main",
            "--limit",
            "300",
            "--json",
            "number,title,body,milestone,labels,mergeCommit",
        ])?;
        Ok(list
            .into_iter()
            .filter(|p| {
                p.merge_commit
                    .as_ref()
                    .is_some_and(|c| commits.contains(&c.oid.as_str()))
            })
            .map(|p| Merged {
                number: p.number,
                title: text::forge(&p.title),
                wave: p.milestone.map(|m| text::forge(&m.title)),
                issue: pr::trailer_issue(&p.body),
                release: p.labels.iter().any(|l| l.name == LABEL),
                commit: p.merge_commit.map(|c| c.oid).unwrap_or_default(),
            })
            .collect())
    }

    fn push_branch(&self, version: Semver) -> Result<String> {
        let branch = format!("release-{}", tag(version));
        let tree = tempfile::Builder::new()
            .prefix("riff-release-")
            .tempdir()
            .context("cannot make a folder for the release worktree")?;
        let path = tree.path().join("tree");
        let path_text = path.to_string_lossy().into_owned();
        self.git(&[
            "worktree",
            "add",
            "-q",
            "--detach",
            &path_text,
            "origin/main",
        ])?;
        let done = self.bump(&path, version).and_then(|()| {
            let refspec = format!("HEAD:refs/heads/{branch}");
            self.git_in(&path, &["push", "-q", "origin", &refspec])
        });
        let _ = self.git(&["worktree", "remove", "--force", &path_text]);
        done?;
        Ok(branch)
    }

    fn open_pr(&self, branch: &str, title: &str, wave: &str, body: &str) -> Result<(u64, String)> {
        let url = self.gh.run(
            &[
                "pr",
                "create",
                "--head",
                branch,
                "--base",
                "main",
                "--title",
                title,
                "--label",
                LABEL,
                "--milestone",
                wave,
                "--body-file",
                "-",
            ],
            Some(body),
        )?;
        let url = text::forge(url.trim());
        let number = number_of(&url)?;
        let n = number.to_string();
        self.gh
            .run(&["pr", "merge", &n, "--auto", "--squash"], None)
            .with_context(|| {
                format!("pull request #{number} is open, but auto-merge is off. Run: gh pr merge --auto --squash {number}")
            })?;
        Ok((number, url))
    }

    fn release_pr(&self, title: &str) -> Result<Option<ReleasePr>> {
        let search = format!("\"{title}\" in:title");
        let list: Vec<GhPr> = self.gh.json(&[
            "pr",
            "list",
            "--state",
            "merged",
            "--label",
            LABEL,
            "--search",
            &search,
            "--json",
            "number,title,url,body,mergeCommit,headRefOid",
        ])?;
        Ok(list.into_iter().find(|p| p.title == title).and_then(|p| {
            Some(ReleasePr {
                number: p.number,
                url: p.url,
                merge: p.merge_commit?.oid,
                head: p.head_ref_oid,
                body: p.body,
            })
        }))
    }

    fn verify_passed(&self, commit: &str) -> Result<bool> {
        let path = format!("repos/{{owner}}/{{repo}}/commits/{commit}/status");
        let status: GhStatuses = self.gh.json(&["api", &path])?;
        Ok(status
            .statuses
            .iter()
            .any(|s| s.context == pr::VERIFY_CONTEXT && s.state == "success"))
    }

    fn candidate(&self, number: u64) -> Result<Candidate> {
        let n = number.to_string();
        let pr: GhPr = self.gh.json(&[
            "pr",
            "view",
            &n,
            "--json",
            "number,title,body,state,labels,milestone,headRefOid,files",
        ])?;
        Ok(Candidate {
            title: pr.title,
            body: pr.body,
            state: pr.state,
            labels: pr.labels.into_iter().map(|l| l.name).collect(),
            milestone: pr.milestone.map(|m| m.title),
            head: pr.head_ref_oid,
            files: pr.files.into_iter().map(|f| f.path).collect(),
        })
    }

    fn full_check(&self) -> Result<Vec<String>> {
        crate::repo_setup::full_check(&self.gh, "{owner}/{repo}")
    }

    fn gate(&self, checks: &[String], commit: &str) -> Result<pr::Gate> {
        pr::gate_of(&self.gh, "{owner}/{repo}", checks, commit)
    }

    fn pass_verify(&self, commit: &str, description: &str) -> Result<()> {
        let path = format!("repos/{{owner}}/{{repo}}/statuses/{commit}");
        self.gh.run(
            &[
                "api",
                &path,
                "-f",
                "state=success",
                "-f",
                &format!("context={}", pr::VERIFY_CONTEXT),
                "-f",
                &format!("description={description}"),
            ],
            None,
        )?;
        Ok(())
    }

    fn on_main(&self, commit: &str) -> Result<bool> {
        let out = Command::new("git")
            .args(["merge-base", "--is-ancestor", commit, "origin/main"])
            .current_dir(&self.dir)
            .output()
            .context("cannot run git merge-base")?;
        Ok(out.status.success())
    }

    fn tag_exists(&self, tag: &str) -> Result<bool> {
        let name = format!("refs/tags/{tag}");
        Ok(!self
            .git(&["ls-remote", "--tags", "origin", &name])?
            .trim()
            .is_empty())
    }

    fn admin(&self) -> Result<bool> {
        let out = self.gh.run(
            &["api", "repos/{owner}/{repo}", "--jq", ".permissions.admin"],
            None,
        )?;
        Ok(out.trim() == "true")
    }

    fn push_tag(&self, tag: &str, commit: &str) -> Result<()> {
        let message = format!("Release {tag}");
        self.git(&["tag", "-a", tag, commit, "-m", &message])?;
        let name = format!("refs/tags/{tag}");
        if let Err(e) = self.git(&["push", "-q", "origin", &name]) {
            let _ = self.git(&["tag", "-d", tag]);
            bail!(
                "riff release publish: GitHub refused the push of the tag {tag}. The ruleset of the \
                 tags lets only an admin of the repository make a tag: sign in to git and gh as an \
                 admin. riff deleted the tag on this machine. {e}"
            );
        }
        Ok(())
    }

    fn make_release(&self, tag: &str, notes: &str, since: Option<&str>) -> Result<String> {
        let mut args = vec![
            "release",
            "create",
            tag,
            "--verify-tag",
            "--title",
            tag,
            "--notes-file",
            "-",
            "--generate-notes",
            "--latest",
        ];
        if let Some(since) = since {
            args.extend(["--notes-start-tag", since]);
        }
        Ok(text::forge(self.gh.run(&args, Some(notes))?.trim()))
    }

    fn tag_run(&self, tag: &str) -> Result<Option<String>> {
        let waits = std::iter::once(Duration::ZERO).chain(self.run_waits.iter().copied());
        for wait in waits {
            std::thread::sleep(wait);
            let runs: Vec<GhRun> = self.gh.json(&[
                "run",
                "list",
                "--workflow",
                WORKFLOW,
                "--branch",
                tag,
                "--event",
                "push",
                "--limit",
                "1",
                "--json",
                "databaseId,url",
            ])?;
            if let Some(run) = runs.into_iter().next() {
                return Ok(Some(text::forge(&run.url)));
            }
        }
        Ok(None)
    }

    fn waiting(&self) -> Result<Vec<Waiting>> {
        let runs: Vec<GhRun> = self.gh.json(&[
            "run",
            "list",
            "--workflow",
            WORKFLOW,
            "--status",
            "waiting",
            "--limit",
            "20",
            "--json",
            "databaseId,headBranch,headSha,url,displayTitle",
        ])?;
        let mut out = Vec::new();
        for run in runs {
            let Some(tag) = run.tag() else {
                continue;
            };
            let path = format!(
                "repos/{{owner}}/{{repo}}/actions/runs/{}/pending_deployments",
                run.database_id
            );
            let pending: Vec<GhPending> = self.gh.json(&["api", &path])?;
            if let Some(p) = pending.iter().find(|p| p.environment.name == PRODUCTION) {
                out.push(Waiting {
                    run: run.database_id,
                    tag,
                    commit: text::forge(&run.head_sha),
                    url: text::forge(&run.url),
                    environment: p.environment.id,
                });
            }
        }
        Ok(out)
    }

    fn approve(&self, run: &Waiting, comment: &str) -> Result<()> {
        let path = format!(
            "repos/{{owner}}/{{repo}}/actions/runs/{}/pending_deployments",
            run.run
        );
        let body = serde_json::json!({
            "environment_ids": [run.environment],
            "state": "approved",
            "comment": comment,
        });
        self.gh.send("POST", &path, &body)?;
        Ok(())
    }

    fn open_release(&self) -> Result<Option<(u64, Semver)>> {
        let list: Vec<GhPr> = self.gh.json(&[
            "pr",
            "list",
            "--state",
            "open",
            "--label",
            LABEL,
            "--json",
            "number,title",
        ])?;
        Ok(list.iter().find_map(|p| {
            let version = tag_version(p.title.strip_prefix("Release ")?).ok()?;
            Some((p.number, version))
        }))
    }

    fn real_session(&self, commit: &str) -> Result<Option<bool>> {
        let path = format!("repos/{{owner}}/{{repo}}/commits/{commit}/status");
        let status: GhStatuses = self.gh.json(&["api", &path])?;
        Ok(status
            .statuses
            .iter()
            .find(|s| s.context == REAL_SESSION)
            .and_then(|s| match s.state.as_str() {
                "success" => Some(true),
                "failure" | "error" => Some(false),
                _ => None,
            }))
    }

    fn test_state(&self, commit: &str) -> Result<Option<TestState>> {
        let path = format!("repos/{{owner}}/{{repo}}/commits/{commit}/status");
        let status: GhStatuses = self.gh.json(&["api", &path])?;
        Ok(status
            .statuses
            .iter()
            .find(|s| s.context == REAL_SESSION)
            .and_then(|s| TestState::of(&s.state)))
    }

    fn commits_since(&self, commit: &str) -> Result<Vec<String>> {
        let range = format!("{commit}^..origin/main");
        let list = self
            .git(&["rev-list", "--first-parent", &range])
            .or_else(|_| self.git(&["rev-list", "--first-parent", "origin/main", "-1"]))?;
        Ok(list.lines().map(str::to_owned).collect())
    }

    fn deploy(&self, tag: &str) -> Result<Option<Deploy>> {
        let runs: Vec<GhRun> = self.gh.json(&[
            "run",
            "list",
            "--workflow",
            WORKFLOW,
            "--limit",
            "40",
            "--json",
            "databaseId,headBranch,headSha,url,displayTitle,status,conclusion",
        ])?;
        let Some(run) = runs.into_iter().find(|r| r.tag().as_deref() == Some(tag)) else {
            return Ok(None);
        };
        let id = run.database_id.to_string();
        let jobs: GhJobs = self.gh.json(&["run", "view", &id, "--json", "jobs"])?;
        let job = jobs.jobs.iter().find(|j| j.name == "Deploy");
        let failed: Vec<String> = jobs
            .jobs
            .iter()
            .filter(|j| matches!(j.conclusion.as_str(), "failure" | "timed_out" | "cancelled"))
            .map(|j| text::forge(&j.name))
            .collect();
        let state = match (run.status.as_str(), job) {
            ("waiting", _) => DeployState::Waiting,
            (_, Some(j)) if j.status == "completed" => match j.conclusion.as_str() {
                "success" => DeployState::Passed,
                "skipped" if run.conclusion == "success" => DeployState::Before,
                _ => DeployState::Failed,
            },
            (_, Some(j)) if j.status == "in_progress" => DeployState::Running,
            ("completed", _) if run.conclusion != "success" => DeployState::Failed,
            _ => DeployState::Before,
        };
        Ok(Some(Deploy {
            run: run.database_id,
            url: text::forge(&run.url),
            state,
            at: job.map(|j| text::forge(&j.started_at)).unwrap_or_default(),
            failed,
        }))
    }

    fn issue_of(&self, title: &str) -> Result<Option<ReleaseIssue>> {
        let search = format!("\"{title}\" in:title");
        let found: Vec<GhFullIssue> = self.gh.json(&[
            "issue",
            "list",
            "--state",
            "all",
            "--label",
            LABEL,
            "--search",
            &search,
            "--json",
            "number,title,state,comments",
        ])?;
        Ok(found
            .into_iter()
            .find(|i| i.title == title)
            .map(|i| ReleaseIssue {
                number: i.number,
                open: i.state == "OPEN",
                comments: i.comments.into_iter().map(|c| c.body).collect(),
            }))
    }

    fn comment(&self, issue: u64, body: &str) -> Result<()> {
        let n = issue.to_string();
        self.gh
            .run(&["issue", "comment", &n, "--body-file", "-"], Some(body))?;
        Ok(())
    }

    fn close(&self, issue: u64) -> Result<()> {
        let n = issue.to_string();
        self.gh.run(&["issue", "close", &n], None)?;
        Ok(())
    }

    fn file_issue(&self, title: &str, body: &str, wave: &str) -> Result<u64> {
        let url = self.gh.run(
            &[
                "issue",
                "create",
                "--title",
                title,
                "--milestone",
                wave,
                "--body-file",
                "-",
            ],
            Some(body),
        )?;
        number_of(&url)
    }

    fn rerun(&self, run: u64) -> Result<()> {
        let id = run.to_string();
        self.gh.run(&["run", "rerun", &id, "--failed"], None)?;
        Ok(())
    }

    fn dispatch(&self, tag: &str) -> Result<()> {
        let input = format!("tag={tag}");
        self.gh.run(
            &["workflow", "run", WORKFLOW, "--ref", "main", "-f", &input],
            None,
        )?;
        Ok(())
    }

    fn released_in(&self, commit: &str) -> Result<Vec<Semver>> {
        let tags = self.git(&["tag", "--contains", commit, "--list", "v*"])?;
        let mut versions: Vec<Semver> = tags.lines().filter_map(|t| tag_version(t).ok()).collect();
        versions.sort();
        Ok(versions)
    }

    fn wave_title(&self, number: u64) -> Result<String> {
        crate::waves::open(&self.gh)?
            .into_iter()
            .find(|m| hygiene::wave_number(&m.title) == Some(number))
            .map(|m| m.title)
            .ok_or_else(|| anyhow!(text::waves_not_open(number)))
    }

    fn note_wave(&self, number: u64, text: &str) -> Result<()> {
        let wave = crate::waves::open(&self.gh)?
            .into_iter()
            .find(|m| hygiene::wave_number(&m.title) == Some(number))
            .ok_or_else(|| anyhow!(text::waves_not_open(number)))?;
        let path = format!("repos/{{owner}}/{{repo}}/milestones/{}", wave.number);
        self.gh
            .send("PATCH", &path, &serde_json::json!({ "description": text }))?;
        Ok(())
    }

    fn failed_jobs(&self, checks: &[String], commit: &str) -> Result<Vec<String>> {
        let repo = "{owner}/{repo}";
        let mut runs: Vec<String> = Vec::new();
        for check in checks {
            for run in pr::check_runs(&self.gh, repo, commit, check)? {
                let failed = run
                    .conclusion
                    .as_deref()
                    .is_some_and(|c| !pr::is_success(c));
                let page = run.html_url.as_deref().and_then(crate::look::run_of);
                if let Some(page) = page.filter(|_| failed)
                    && !runs.iter().any(|r| r == page)
                {
                    runs.push(page.to_owned());
                }
            }
        }
        let mut names: Vec<String> = Vec::new();
        for run in runs {
            for name in crate::look::read_failed_jobs(&self.gh, repo, &run)? {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        Ok(names)
    }

    fn wave_open(&self) -> Result<Option<(String, usize)>> {
        let Some(wave) = crate::waves::open(&self.gh)?
            .into_iter()
            .find(|m| hygiene::wave_number(&m.title).is_some())
        else {
            return Ok(None);
        };
        let items: Vec<GhOpenIssue> = self.gh.json(&[
            "issue",
            "list",
            "--state",
            "open",
            "--milestone",
            &wave.title,
            "--limit",
            "1000",
            "--json",
            "title,labels",
        ])?;
        let open = items
            .iter()
            .filter(|i| {
                let labels = i.labels.iter().map(|l| l.name.as_str());
                let is_release = labels.clone().any(|l| l == LABEL);
                !is_release && !riff_client::waves::is_tracker(&i.title, labels)
            })
            .count();
        Ok(Some((wave.title, open)))
    }

    fn has_release(&self) -> bool {
        crate::release_config::of_folder(&self.dir).unwrap_or(false)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A forge in memory. It logs each write.
    #[derive(Default, Clone)]
    pub(crate) struct Fake {
        pub(crate) main: String,
        pub(crate) releases: Vec<&'static str>,
        pub(crate) merged: Vec<Merged>,
        pub(crate) release_pr: Option<ReleasePr>,
        pub(crate) open_pr: Option<(u64, &'static str)>,
        pub(crate) verify: bool,
        pub(crate) candidate: Candidate,
        /// The checks of the full check.
        pub(crate) checks: Vec<String>,
        pub(crate) gate: pr::Gate,
        pub(crate) on_main: bool,
        pub(crate) tags: Vec<String>,
        pub(crate) admin: bool,
        pub(crate) waiting: Vec<Waiting>,
        /// The commits with a pass or a fail of the real-session test.
        pub(crate) real: Vec<(&'static str, bool)>,
        /// The commits whose real-session test runs.
        pub(crate) running: Vec<&'static str>,
        /// The commits from the merge commit to origin/main, newest first.
        pub(crate) since: Vec<&'static str>,
        pub(crate) deploy: Option<Deploy>,
        pub(crate) issue: Option<ReleaseIssue>,
        /// The release tags that hold each commit.
        pub(crate) released: Vec<(&'static str, Vec<&'static str>)>,
        pub(crate) log: RefCell<Vec<String>>,
        /// The failed jobs of the full check.
        pub(crate) failed: Vec<String>,
        /// The current wave and its open items.
        pub(crate) wave_open: Option<(String, usize)>,
        /// True when the repository has the table `[release]`.
        pub(crate) has_release: bool,
    }

    impl Fake {
        pub(crate) fn new() -> Self {
            Fake {
                main: "3.2.0".into(),
                checks: vec!["Gate".into()],
                releases: vec!["3.0.0", "3.2.0"],
                verify: true,
                on_main: true,
                admin: true,
                ..Fake::default()
            }
        }

        pub(crate) fn ready(main: &str) -> Self {
            Fake {
                main: main.into(),
                release_pr: Some(ReleasePr {
                    number: 1140,
                    url: "https://github.com/o/r/pull/1140".into(),
                    merge: "m1".into(),
                    head: "h1".into(),
                    body: "Refs #7\n\nPart of #7: x.\n\nLevel: minor, 3.3.0.\n\nIssue: #7\nMilestone: Wave 34\n"
                        .into(),
                }),
                real: vec![("m1", true)],
                since: vec!["m1"],
                ..Fake::new()
            }
        }

        pub(crate) fn log(&self) -> Vec<String> {
            self.log.borrow().clone()
        }

        fn write(&self, line: String) {
            self.log.borrow_mut().push(line);
        }
    }

    impl Forge for Fake {
        fn main_version(&self) -> Result<Semver> {
            version(&self.main)
        }
        fn releases(&self) -> Result<Vec<Semver>> {
            self.releases.iter().map(|r| version(r)).collect()
        }
        fn current_wave(&self) -> Result<String> {
            Ok("Wave 34: Release".into())
        }
        fn release_issue(&self, title: &str, wave: &str, body: &str) -> Result<u64> {
            self.write(format!("issue {title} in {wave}\n{body}"));
            Ok(7)
        }
        fn merged_since(&self, since: Option<&str>) -> Result<Vec<Merged>> {
            self.write(format!("merged since {since:?}"));
            Ok(self.merged.clone())
        }
        fn push_branch(&self, version: Semver) -> Result<String> {
            self.write(format!("branch {version}"));
            Ok(format!("release-v{version}"))
        }
        fn open_pr(
            &self,
            branch: &str,
            title: &str,
            wave: &str,
            body: &str,
        ) -> Result<(u64, String)> {
            self.write(format!("pr {branch} {title} {wave}\n{body}"));
            Ok((1140, "https://github.com/o/r/pull/1140".into()))
        }
        fn release_pr(&self, _title: &str) -> Result<Option<ReleasePr>> {
            Ok(self.release_pr.clone())
        }
        fn verify_passed(&self, _commit: &str) -> Result<bool> {
            Ok(self.verify)
        }
        fn candidate(&self, _number: u64) -> Result<Candidate> {
            Ok(self.candidate.clone())
        }
        fn full_check(&self) -> Result<Vec<String>> {
            Ok(self.checks.clone())
        }
        fn gate(&self, _checks: &[String], _commit: &str) -> Result<pr::Gate> {
            Ok(self.gate.clone())
        }
        fn pass_verify(&self, commit: &str, description: &str) -> Result<()> {
            self.write(format!("verify {commit} {description}"));
            Ok(())
        }
        fn on_main(&self, _commit: &str) -> Result<bool> {
            Ok(self.on_main)
        }
        fn tag_exists(&self, tag: &str) -> Result<bool> {
            Ok(self.tags.iter().any(|t| t == tag))
        }
        fn admin(&self) -> Result<bool> {
            Ok(self.admin)
        }
        fn push_tag(&self, tag: &str, commit: &str) -> Result<()> {
            self.write(format!("tag {tag} {commit}"));
            Ok(())
        }
        fn make_release(&self, tag: &str, notes: &str, since: Option<&str>) -> Result<String> {
            self.write(format!("release {tag} since {since:?}\n{notes}"));
            Ok(format!("https://github.com/o/r/releases/tag/{tag}"))
        }
        fn tag_run(&self, _tag: &str) -> Result<Option<String>> {
            Ok(Some("https://github.com/o/r/actions/runs/9".into()))
        }
        fn waiting(&self) -> Result<Vec<Waiting>> {
            Ok(self.waiting.clone())
        }
        fn approve(&self, run: &Waiting, comment: &str) -> Result<()> {
            self.write(format!("approve {} {} {comment}", run.run, run.environment));
            Ok(())
        }
        fn open_release(&self) -> Result<Option<(u64, Semver)>> {
            self.open_pr.map(|(n, v)| Ok((n, version(v)?))).transpose()
        }
        fn real_session(&self, commit: &str) -> Result<Option<bool>> {
            Ok(self
                .real
                .iter()
                .find(|(c, _)| *c == commit)
                .map(|(_, p)| *p))
        }
        fn test_state(&self, commit: &str) -> Result<Option<TestState>> {
            if self.running.contains(&commit) {
                return Ok(Some(TestState::Running));
            }
            Ok(self.real.iter().find(|(c, _)| *c == commit).map(|(_, p)| {
                if *p {
                    TestState::Passed
                } else {
                    TestState::Failed
                }
            }))
        }
        fn commits_since(&self, _commit: &str) -> Result<Vec<String>> {
            Ok(self.since.iter().map(|c| (*c).to_owned()).collect())
        }
        fn deploy(&self, _tag: &str) -> Result<Option<Deploy>> {
            Ok(self.deploy.clone())
        }
        fn issue_of(&self, _title: &str) -> Result<Option<ReleaseIssue>> {
            Ok(self.issue.clone())
        }
        fn comment(&self, issue: u64, body: &str) -> Result<()> {
            self.write(format!("comment #{issue}\n{body}"));
            Ok(())
        }
        fn close(&self, issue: u64) -> Result<()> {
            self.write(format!("close #{issue}"));
            Ok(())
        }
        fn file_issue(&self, title: &str, body: &str, wave: &str) -> Result<u64> {
            self.write(format!("file {title} in {wave}\n{body}"));
            Ok(88)
        }
        fn rerun(&self, run: u64) -> Result<()> {
            self.write(format!("rerun {run}"));
            Ok(())
        }
        fn dispatch(&self, tag: &str) -> Result<()> {
            self.write(format!("dispatch {tag}"));
            Ok(())
        }
        fn released_in(&self, commit: &str) -> Result<Vec<Semver>> {
            let tags = self
                .released
                .iter()
                .find(|(c, _)| *c == commit)
                .map(|(_, t)| t.clone())
                .unwrap_or_default();
            tags.iter().map(|t| version(t)).collect()
        }
        fn wave_title(&self, number: u64) -> Result<String> {
            Ok(format!("Wave {number}: Release"))
        }
        fn note_wave(&self, number: u64, text: &str) -> Result<()> {
            self.write(format!("note wave {number}: {text}"));
            Ok(())
        }
        fn failed_jobs(&self, _checks: &[String], _commit: &str) -> Result<Vec<String>> {
            Ok(self.failed.clone())
        }
        fn wave_open(&self) -> Result<Option<(String, usize)>> {
            Ok(self.wave_open.clone())
        }
        fn has_release(&self) -> bool {
            self.has_release
        }
    }

    fn v(text: &str) -> Semver {
        version(text).unwrap()
    }

    fn deploy(state: DeployState) -> Deploy {
        Deploy {
            run: 9,
            url: "https://github.com/o/r/actions/runs/9".into(),
            state,
            at: "2026-10-18T20:00:00Z".into(),
            failed: if state == DeployState::Failed {
                vec!["Deploy".into()]
            } else {
                vec![]
            },
        }
    }

    #[test]
    fn cut_makes_the_pull_request_with_the_waves_and_part_of() {
        let mut forge = Fake::new();
        forge.merged = vec![
            Merged {
                number: 41,
                title: "Show the line".into(),
                wave: Some("Wave 34: Release".into()),
                issue: Some(12),
                ..Merged::default()
            },
            Merged {
                number: 40,
                title: "Release v3.2.0".into(),
                wave: Some("Wave 33".into()),
                issue: Some(6),
                release: true,
                ..Merged::default()
            },
        ];
        let cut = cut(&forge, v("3.3.0"), None, Some("New commands.")).unwrap();
        assert_eq!((cut.number, cut.issue), (1140, 7));
        let log = forge.log();
        assert!(
            log[0].starts_with("issue Release v3.3.0 in Wave 34: Release\nThe release v3.3.0.\n"),
            "{}",
            log[0]
        );
        assert!(
            log[0].contains("Done when: the release is published"),
            "{}",
            log[0]
        );
        assert!(
            log[0].contains("Verify of the release pull request."),
            "{}",
            log[0]
        );
        assert!(log[0].contains("The Gate passed."), "{}", log[0]);
        assert!(log[0].contains("Steps after the merge."), "{}", log[0]);
        assert_eq!(log[1], "merged since Some(\"v3.2.0\")");
        assert_eq!(log[2], "branch 3.3.0");
        let pr = &log[3];
        assert!(
            pr.starts_with("pr release-v3.3.0 Release v3.3.0 Wave 34: Release\nRefs #7\n"),
            "{pr}"
        );
        assert!(pr.contains("Part of #7"), "{pr}");
        assert!(!pr.contains("Closes"), "{pr}");
        assert!(pr.contains("Level: minor, 3.3.0. New commands."), "{pr}");
        assert!(
            pr.contains("Waves in this release: Wave 34: Release.\n"),
            "{pr}"
        );
        assert!(pr.contains("- #12: Show the line (#41).\n"), "{pr}");
        assert!(!pr.contains("#40"), "{pr}");
        assert!(pr.ends_with("Issue: #7\nMilestone: Wave 34\n"), "{pr}");
    }

    #[test]
    fn cut_refuses_a_version_that_is_not_newer() {
        let forge = Fake::new();
        for old in ["3.2.0", "3.1.0"] {
            let error = cut(&forge, v(old), None, None).unwrap_err().to_string();
            assert!(
                error.contains("is not newer than the last release v3.2.0. Give a version after 3.2.0. next: riff release cut 3.2.1"),
                "{error}"
            );
        }
        assert!(forge.log().is_empty());
    }

    #[test]
    fn cut_refuses_while_the_version_on_main_has_no_release() {
        let mut forge = Fake::new();
        forge.main = "3.3.0".into();
        let error = cut(&forge, v("3.4.0"), None, None).unwrap_err().to_string();
        assert!(
            error.contains("the version 3.3.0 on origin/main has no published release"),
            "{error}"
        );
        assert!(
            error.contains("next: riff release publish v3.3.0"),
            "{error}"
        );
        let other = Skip {
            version: v("3.1.0"),
            reason: "x".into(),
        };
        let error = cut(&forge, v("3.4.0"), Some(&other), None)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("--skip 3.1.0 is not the version on origin/main: 3.3.0. next: riff release cut 3.4.0 --skip 3.3.0 --reason TEXT"),
            "{error}"
        );
        assert!(forge.log().is_empty());
    }

    /// HUMAN GATE 1 with no answer: the next cut refuses
    /// (01M58KMM3NFGJRW4CCV50KJQB2).
    #[test]
    fn cut_refuses_while_a_deploy_waits_for_a_person() {
        let mut forge = Fake::new();
        forge.waiting = vec![run("v3.2.0")];
        let error = cut(&forge, v("3.3.0"), None, None).unwrap_err().to_string();
        assert!(
            error.contains("the deploy of v3.2.0 waits for the approval of production"),
            "{error}"
        );
        assert!(
            error.contains("next: a person runs riff release approve v3.2.0"),
            "{error}"
        );
        assert!(forge.log().is_empty());
    }

    #[test]
    fn cut_with_a_skip_puts_the_reason_in_the_body() {
        let mut forge = Fake::new();
        forge.main = "3.3.0".into();
        let skip = Skip {
            version: v("3.3.0"),
            reason: "the push of the tag was refused".into(),
        };
        cut(&forge, v("3.4.0"), Some(&skip), None).unwrap();
        let log = forge.log();
        assert!(
            log[3].contains("No release 3.3.0 exists: the push of the tag was refused. This release has each item since v3.2.0."),
            "{}", log[3]
        );
    }

    #[test]
    fn cut_refuses_a_skip_with_nothing_to_skip() {
        let forge = Fake::new();
        let skip = Skip {
            version: v("3.2.0"),
            reason: "x".into(),
        };
        let error = cut(&forge, v("3.3.0"), Some(&skip), None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("names no version to skip"), "{error}");
        assert!(
            error.contains("has its release. next: riff release cut 3.3.0"),
            "{error}"
        );
    }

    #[test]
    fn publish_tags_the_tested_commit_and_makes_the_release() {
        let forge = Fake::ready("3.3.0");
        let published = publish(&forge, "v3.3.0").unwrap();
        assert_eq!(
            published.release,
            "https://github.com/o/r/releases/tag/v3.3.0"
        );
        assert_eq!(
            published.run.as_deref(),
            Some("https://github.com/o/r/actions/runs/9")
        );
        assert_eq!(
            forge.log(),
            [
                "tag v3.3.0 m1".to_owned(),
                "release v3.3.0 since Some(\"v3.2.0\")\nLevel: minor, 3.3.0.\n".to_owned(),
            ]
        );
    }

    fn release_candidate() -> Candidate {
        Candidate {
            title: "Release v3.3.0".into(),
            body: "Refs #7\n\nPart of #7: x.\n\nLevel: minor, 3.3.0.\n\nIssue: #7\nMilestone: Wave 34\n"
                .into(),
            state: "OPEN".into(),
            labels: vec!["release".into()],
            milestone: Some("Wave 34: Release".into()),
            head: "h1".into(),
            files: vec!["Cargo.toml".into(), "Cargo.lock".into()],
        }
    }

    /// A release pull request with the version files and a passed Gate
    /// gets the status riff/verify (01M5ND0ZNFFAPT1AWD3DP1Y77N).
    #[test]
    fn verify_passes_a_release_pull_request() {
        let forge = Fake {
            candidate: release_candidate(),
            gate: pr::Gate::Passed,
            ..Fake::new()
        };
        assert_eq!(verify(&forge, 1140).unwrap(), "h1");
        let log = forge.log();
        assert_eq!(log.len(), 1, "{log:?}");
        assert!(log[0].starts_with("verify h1 pass"), "{log:?}");
    }

    #[test]
    fn verify_refuses_an_extra_file() {
        let mut candidate = release_candidate();
        candidate.files.push("crates/riff/src/main.rs".into());
        let forge = Fake {
            candidate,
            gate: pr::Gate::Passed,
            ..Fake::new()
        };
        let e = verify(&forge, 1140).unwrap_err().to_string();
        assert!(e.contains("crates/riff/src/main.rs"), "{e}");
        assert!(forge.log().is_empty());
    }

    #[test]
    fn verify_refuses_a_failed_gate_and_a_pull_request_with_no_label() {
        let forge = Fake {
            candidate: release_candidate(),
            gate: pr::Gate::Ended("failure".into()),
            ..Fake::new()
        };
        let e = verify(&forge, 1140).unwrap_err().to_string();
        assert!(
            e.contains("the full check of commit h1 (Gate) ended with failure."),
            "{e}"
        );
        assert!(e.contains("next: wait for the checks, then riff release verify 1140"));
        let mut candidate = release_candidate();
        candidate.labels.clear();
        let forge = Fake {
            candidate,
            gate: pr::Gate::Passed,
            ..Fake::new()
        };
        let e = verify(&forge, 1140).unwrap_err().to_string();
        assert!(e.contains("no label release"), "{e}");
        assert!(forge.log().is_empty());
    }

    /// #1393, 01M5R13DK401K3TZ91E2QK2DYM: a repository whose ruleset
    /// requires `checks / test` and `checks / docs` has no check `Gate`.
    /// The refusal names these checks, and a pass sets the status.
    #[test]
    fn verify_refuses_with_the_names_of_the_checks_of_the_repository() {
        let checks = vec!["checks / test".to_owned(), "checks / docs".to_owned()];
        let forge = Fake {
            candidate: release_candidate(),
            checks: checks.clone(),
            gate: pr::Gate::Running,
            ..Fake::new()
        };
        let e = verify(&forge, 1140).unwrap_err().to_string();
        assert!(
            e.contains("the full check of commit h1 (checks / test, checks / docs) runs still."),
            "{e}"
        );
        assert!(!e.contains("Gate"), "{e}");
        assert!(forge.log().is_empty());
        let forge = Fake {
            candidate: release_candidate(),
            checks,
            gate: pr::Gate::Passed,
            ..Fake::new()
        };
        assert_eq!(verify(&forge, 1140).unwrap(), "h1");
    }

    #[test]
    fn verify_refuses_a_body_without_the_format() {
        let mut candidate = release_candidate();
        candidate.body = "A release.".into();
        let forge = Fake {
            candidate,
            gate: pr::Gate::Passed,
            ..Fake::new()
        };
        let e = verify(&forge, 1140).unwrap_err().to_string();
        assert!(e.contains("hygiene"), "{e}");
        assert!(forge.log().is_empty());
    }

    /// The real-session test failed on the merge commit, the fix merged,
    /// and the test passed on the fix: the tag goes on the fix
    /// (01M58KMKZ2EG8NZG4F085S6NQX).
    #[test]
    fn publish_tags_the_fix_after_a_failed_real_session() {
        let forge = Fake {
            real: vec![("m1", false), ("fix", true)],
            since: vec!["later", "fix", "m1"],
            ..Fake::ready("3.3.0")
        };
        publish(&forge, "v3.3.0").unwrap();
        assert_eq!(forge.log()[0], "tag v3.3.0 fix");
    }

    #[test]
    fn publish_refuses_each_false_fact_and_changes_nothing() {
        let cases: Vec<(Fake, &str)> = vec![
            (
                Fake {
                    release_pr: None,
                    ..Fake::ready("3.3.0")
                },
                "no merged pull request \"Release v3.3.0\"",
            ),
            (
                Fake {
                    verify: false,
                    ..Fake::ready("3.3.0")
                },
                "riff/verify of pull request #1140 did not pass",
            ),
            (
                Fake {
                    on_main: false,
                    ..Fake::ready("3.3.0")
                },
                "is not on origin/main",
            ),
            (
                Fake {
                    real: vec![],
                    ..Fake::ready("3.3.0")
                },
                "has the status riff/real-session. next: just real-session on origin/main",
            ),
            (
                Fake {
                    real: vec![("m1", false)],
                    ..Fake::ready("3.3.0")
                },
                "the real-session test failed on the merge commit m1",
            ),
            (Fake::ready("3.2.0"), "has the version 3.2.0, not 3.3.0"),
            (
                Fake {
                    tags: vec!["v3.3.0".into()],
                    ..Fake::ready("3.3.0")
                },
                "the tag v3.3.0 exists",
            ),
            (
                Fake {
                    admin: false,
                    ..Fake::ready("3.3.0")
                },
                "not an admin of the repository",
            ),
        ];
        for (forge, want) in cases {
            let error = publish(&forge, "v3.3.0").unwrap_err().to_string();
            assert!(error.contains(want), "{want}: {error}");
            assert!(error.contains(". next: "), "{error}");
            assert!(forge.log().is_empty(), "{want}");
        }
        let error = publish(&Fake::ready("3.3.0"), "3.3.0")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(
                "is not a release tag vX.Y.Z, for example v3.3.0. next: riff release status"
            ),
            "{error}"
        );
    }

    fn run(tag: &str) -> Waiting {
        Waiting {
            run: 9,
            tag: tag.into(),
            commit: "m1".into(),
            url: "https://github.com/o/r/actions/runs/9".into(),
            environment: 5,
        }
    }

    #[test]
    fn status_shows_an_unpublished_release_a_waiting_run_and_the_next_step() {
        let mut forge = Fake::ready("3.3.0");
        forge.waiting = vec![run("v3.2.0")];
        let text = render_status(&status(&forge).unwrap());
        assert_eq!(
            text,
            "version on origin/main: 3.3.0\nlast release: v3.2.0\n\
             merged, not published: v3.3.0 (pull request #1140)\n\
             waits for production: v3.2.0 https://github.com/o/r/actions/runs/9. A person runs: \
             riff release approve v3.2.0\n\
             release: v3.3.0 tested\nnext: riff release publish v3.3.0\n"
        );
    }

    /// 01M5924C8BTT6CW05Q0YZJ1GCY: the test runs on the newest commit
    /// of a merged release, and `status` shows its state.
    #[test]
    fn test_target_is_the_newest_commit_of_a_merged_release() {
        let merged = Fake {
            real: vec![],
            since: vec!["m3", "m2", "m1"],
            ..Fake::ready("3.3.0")
        };
        assert_eq!(test_target(&merged, "v3.3.0").unwrap(), "m3");
        assert_eq!(
            render_status(&status(&merged).unwrap()).lines().nth(4),
            Some("real-session test: none on m3")
        );
        // A failed test on the newest commit can run again.
        let failed = Fake {
            real: vec![("m3", false)],
            ..merged
        };
        assert_eq!(test_target(&failed, "v3.3.0").unwrap(), "m3");
        assert_eq!(
            render_status(&status(&failed).unwrap()).lines().nth(4),
            Some("real-session test: failed on m3")
        );
        // A tested release and an open pull request refuse.
        let tested = Fake::ready("3.3.0");
        let err = test_target(&tested, "v3.3.0").unwrap_err().to_string();
        assert!(err.contains("not merged and untested"), "{err}");
        let cut = Fake {
            open_pr: Some((1140, "3.3.0")),
            release_pr: None,
            ..Fake::new()
        };
        assert!(test_target(&cut, "v3.3.0").is_err());
        assert!(test_target(&failed, "3.3.0").is_err());
    }

    /// Each state of the cycle, with its next step
    /// (01M58KMKTE2XC35CNJBFKZWAXF, 01M58KMKWRKSV1DR42FRH2K43X).
    #[test]
    fn stage_finds_each_state_of_the_cycle() {
        let issue = |open, comments: &[&str]| ReleaseIssue {
            number: 7,
            open,
            comments: comments.iter().map(|c| (*c).to_owned()).collect(),
        };
        let tagged = |d: Option<Deploy>| Fake {
            tags: vec!["v3.3.0".into()],
            deploy: d,
            ..Fake::ready("3.3.0")
        };
        let failed_deploy = "riff release check v3.3.0: failed deploy\n\nThe CI run x failed.";
        let unknown = check_text("v3.3.0", &[("a", Verdict::Unknown("no log".into()))]);
        let cases: Vec<(Fake, State, &str)> = vec![
            (
                Fake {
                    open_pr: Some((1140, "3.3.0")),
                    release_pr: None,
                    ..Fake::new()
                },
                State::Cut,
                "wait for the verify and the merge of pull request #1140",
            ),
            (
                Fake {
                    real: vec![],
                    ..Fake::ready("3.3.0")
                },
                State::Merged,
                "riff release test v3.3.0 --host HOST, or just real-session on origin/main",
            ),
            (
                Fake {
                    real: vec![],
                    running: vec!["m1"],
                    ..Fake::ready("3.3.0")
                },
                State::Merged,
                "wait for the real-session test on m1; riff release status shows it",
            ),
            (
                Fake {
                    real: vec![("m1", false)],
                    ..Fake::ready("3.3.0")
                },
                State::Merged,
                "file the bug in the wave, then just real-session on origin/main after its fix merges",
            ),
            (
                Fake::ready("3.3.0"),
                State::Tested,
                "riff release publish v3.3.0",
            ),
            (
                tagged(None),
                State::Published,
                "wait for the CI run of the tag v3.3.0",
            ),
            (
                tagged(Some(deploy(DeployState::Before))),
                State::Published,
                "wait for the jobs of the CI run https://github.com/o/r/actions/runs/9",
            ),
            (
                tagged(Some(deploy(DeployState::Waiting))),
                State::Waiting,
                "a person runs riff release approve v3.3.0 in a terminal",
            ),
            (
                tagged(Some(deploy(DeployState::Running))),
                State::Deploying,
                "wait for the deploy: https://github.com/o/r/actions/runs/9",
            ),
            (
                tagged(Some(deploy(DeployState::Passed))),
                State::Deployed,
                "riff release check v3.3.0",
            ),
            (
                tagged(Some(deploy(DeployState::Failed))),
                State::Failed,
                "riff release retry v3.3.0, or riff release rollback v3.2.0",
            ),
            // After a failed deploy and riff release check, the CI run
            // decides: before, while and after riff release retry.
            (
                Fake {
                    issue: Some(issue(true, &[failed_deploy])),
                    ..tagged(Some(deploy(DeployState::Failed)))
                },
                State::Failed,
                "riff release retry v3.3.0, or riff release rollback v3.2.0",
            ),
            (
                Fake {
                    issue: Some(issue(true, &[failed_deploy])),
                    ..tagged(Some(deploy(DeployState::Waiting)))
                },
                State::Waiting,
                "a person runs riff release approve v3.3.0 in a terminal",
            ),
            (
                Fake {
                    issue: Some(issue(true, &[failed_deploy])),
                    ..tagged(Some(deploy(DeployState::Running)))
                },
                State::Deploying,
                "wait for the deploy: https://github.com/o/r/actions/runs/9",
            ),
            (
                Fake {
                    issue: Some(issue(true, &[failed_deploy])),
                    ..tagged(Some(deploy(DeployState::Passed)))
                },
                State::Deployed,
                "riff release check v3.3.0",
            ),
            // A check that riff could not do changes no state.
            (
                Fake {
                    issue: Some(issue(true, &[unknown.as_str()])),
                    ..tagged(Some(deploy(DeployState::Passed)))
                },
                State::Deployed,
                "riff release check v3.3.0",
            ),
            // The newest check that ran decides.
            (
                Fake {
                    issue: Some(issue(
                        true,
                        &["riff release check v3.3.0: failed\n\n- x", unknown.as_str()],
                    )),
                    ..tagged(Some(deploy(DeployState::Passed)))
                },
                State::Failed,
                "fix forward with riff release cut 3.3.1, or roll back with riff release \
                 rollback v3.2.0",
            ),
            (
                Fake {
                    issue: Some(issue(true, &["riff release check v3.3.0: failed\n\n- x"])),
                    ..tagged(Some(deploy(DeployState::Passed)))
                },
                State::Failed,
                "fix forward with riff release cut 3.3.1, or roll back with riff release \
                 rollback v3.2.0",
            ),
            (
                Fake {
                    issue: Some(issue(true, &["riff release check v3.3.0: passed"])),
                    ..tagged(Some(deploy(DeployState::Passed)))
                },
                State::Checked,
                "riff wave end N, for the wave of the release",
            ),
            (
                Fake {
                    issue: Some(issue(false, &[])),
                    ..tagged(None)
                },
                State::Checked,
                "riff wave end N, for the wave of the release",
            ),
            (
                Fake {
                    issue: Some(issue(true, &["riff release rollback v3.2.0\n\nx"])),
                    ..tagged(Some(deploy(DeployState::Passed)))
                },
                State::RolledBack,
                "fix forward with riff release cut 3.3.1",
            ),
        ];
        for (forge, state, step) in cases {
            let stage = current(&forge).unwrap().unwrap();
            assert_eq!(stage.state, state, "{step}");
            assert_eq!(next(&stage), step, "{state:?}");
        }
        assert_eq!(stage(&Fake::new(), v("3.4.0")).unwrap(), None);
    }

    /// Each refusal of the guard names the next step
    /// (01M58KMKWRKSV1DR42FRH2K43X).
    #[test]
    fn each_refusal_of_the_guard_has_a_next_line() {
        let cases = [
            (Refusal::Worker, "next: the lead runs riff release cut"),
            (Refusal::Container, "next: the lead runs riff release cut"),
            (Refusal::NotTheLead, "next: the lead runs riff release cut"),
            (Refusal::LeadUnknown, "next: riff whoami"),
            (
                Refusal::Member,
                "next: the owner or an admin of the riff runs riff release cut",
            ),
            (Refusal::RoleUnknown, "next: riff whoami"),
            (
                Refusal::Agent,
                "next: a person runs riff release cut in a plain terminal, outside Claude Code",
            ),
        ];
        for (refusal, step) in cases {
            let text = crate::text::release_refused("cut", refusal);
            assert!(text.ends_with(step), "{text}");
            assert!(text.contains("changed nothing"), "{text}");
        }
    }

    #[test]
    fn approve_asks_and_approve_run_approves_with_a_comment() {
        let mut forge = Fake::new();
        forge.waiting = vec![run("v3.3.0")];
        let mut out = Vec::new();
        let done = approve(
            &forge,
            "v3.3.0",
            &[],
            false,
            &mut "yes\n".as_bytes(),
            &mut out,
        )
        .unwrap();
        assert_eq!(done, Approved::Yes(run("v3.3.0")));
        let shown = String::from_utf8(out).unwrap();
        assert!(
            shown.contains("tag: v3.3.0\ncommit: m1\nrun: https://github.com/o/r/actions/runs/9\n"),
            "{shown}"
        );
        // The ask changes nothing: riff pauses the riff, then approves.
        assert!(forge.log().is_empty());
        approve_run(&forge, &run("v3.3.0")).unwrap();
        assert_eq!(forge.log(), ["approve 9 5 Release v3.3.0"]);
    }

    #[test]
    fn approve_with_no_or_no_run_changes_nothing() {
        let mut forge = Fake::new();
        let ask = |forge: &Fake, answer: &str| {
            approve(
                forge,
                "v3.3.0",
                &[],
                false,
                &mut answer.as_bytes(),
                &mut Vec::new(),
            )
            .unwrap()
        };
        assert_eq!(ask(&forge, "yes\n"), Approved::NoRun);
        forge.waiting = vec![run("v3.3.0")];
        assert_eq!(ask(&forge, "no\n"), Approved::No);
        assert!(forge.log().is_empty());
    }

    fn a_hold() -> Hold {
        Hold {
            user: "nate".into(),
            repo: "n8behavior/sens-o-matic".into(),
            reason: "I migrate a database.".into(),
        }
    }

    /// 01M5SNV7E775DRY9QY60E8JPJR: a hold stops the approval, with no
    /// question. `--over-holds` shows the hold and asks.
    #[test]
    fn approve_refuses_while_a_hold_stands_unless_the_person_passes_over_holds() {
        let mut forge = Fake::new();
        forge.waiting = vec![run("v3.3.0")];
        let holds = [a_hold()];
        let mut out = Vec::new();
        let held = approve(
            &forge,
            "v3.3.0",
            &holds,
            false,
            &mut "yes\n".as_bytes(),
            &mut out,
        )
        .unwrap();
        assert_eq!(held, Approved::Held(holds.to_vec()));
        assert!(out.is_empty(), "no question while a hold stands");

        let mut out = Vec::new();
        let over = approve(
            &forge,
            "v3.3.0",
            &holds,
            true,
            &mut "yes\n".as_bytes(),
            &mut out,
        )
        .unwrap();
        assert_eq!(over, Approved::Yes(run("v3.3.0")));
        let shown = String::from_utf8(out).unwrap();
        assert!(
            shown.contains(
                "hold: the lead of n8behavior/sens-o-matic asks to wait: I migrate a database.\nApprove"
            ),
            "{shown}"
        );
        // With no run, a hold changes nothing and says nothing.
        forge.waiting.clear();
        let none = approve(
            &forge,
            "v3.3.0",
            &holds,
            false,
            &mut "yes\n".as_bytes(),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(none, Approved::NoRun);
    }

    /// 01M5SNV7BC5J58WRZRJ5HS4NSJ: the status shows each hold above the
    /// release, and the tags of the status are the release and each
    /// deploy that waits.
    #[test]
    fn the_status_shows_each_hold_and_names_the_tags_to_look_at() {
        let stage = Stage {
            version: version("3.3.0").unwrap(),
            state: State::Waiting,
            pr: None,
            merge: None,
            deploy: None,
            checks_failed: false,
            test: None,
            previous: Some(version("3.2.0").unwrap()),
        };
        let mut status = Status {
            main: version("3.3.0").unwrap(),
            last: Some(version("3.2.0").unwrap()),
            unpublished: None,
            waiting: vec![run("v3.2.0"), run("v3.3.0")],
            stage: Some(stage),
            test: None,
            holds: vec![a_hold()],
        };
        assert_eq!(status_tags(&status), ["v3.2.0", "v3.3.0"]);
        let shown = render_status(&status);
        assert!(
            shown.contains(
                "hold: the lead of n8behavior/sens-o-matic asks to wait: I migrate a database.\n\
                 release: v3.3.0 waiting\n"
            ),
            "{shown}"
        );
        status.holds.clear();
        assert!(!render_status(&status).contains("hold:"));
    }

    /// A log in memory: the lines of each filter that has a part of
    /// `lines`, or an error for each filter.
    struct Log(Result<Vec<(&'static str, Vec<LogLine>)>, &'static str>);

    impl CloudLog for Log {
        fn lines(&self, filter: &str) -> Result<Vec<LogLine>> {
            match &self.0 {
                Ok(parts) => Ok(parts
                    .iter()
                    .find(|(part, _)| filter.contains(part))
                    .map(|(_, l)| l.clone())
                    .unwrap_or_default()),
                Err(e) => bail!("{e}"),
            }
        }
    }

    fn line(secs: i64, revision: &str) -> LogLine {
        LogLine {
            at: DateTime::from_timestamp(1_800_000_000 + secs, 0).unwrap(),
            revision: revision.into(),
        }
    }

    fn served(build: &str) -> Served {
        Served {
            server: Ok(build.into()),
            hosts: Ok(vec![(
                "mike@thelio".into(),
                format!("{build} bbf9227fd8cf 2026-10-22T02:02:45Z"),
            )]),
        }
    }

    #[test]
    fn check_passes_writes_the_result_and_closes_the_release_issue() {
        let forge = Fake {
            tags: vec!["v3.3.0".into()],
            deploy: Some(deploy(DeployState::Passed)),
            issue: Some(ReleaseIssue {
                number: 7,
                open: true,
                comments: vec![],
            }),
            ..Fake::ready("3.3.0")
        };
        let log = Log(Ok(vec![(
            "503",
            vec![line(0, "new"), line(3, "new"), line(10, "new")],
        )]));
        let done = check(&forge, "v3.3.0", &served("3.3.0"), &log).unwrap();
        let Checked::Ran { passed, issue, .. } = done else {
            panic!("{done:?}")
        };
        assert!(passed);
        assert_eq!(issue, 7);
        let log = forge.log();
        assert!(
            log[0].starts_with("comment #7\nriff release check v3.3.0: passed\n"),
            "{log:?}"
        );
        assert_eq!(log[1], "close #7");
    }

    #[test]
    fn check_fails_on_each_bad_fact_and_keeps_the_issue_open() {
        let ready = || Fake {
            tags: vec!["v3.3.0".into()],
            deploy: Some(deploy(DeployState::Passed)),
            issue: Some(ReleaseIssue {
                number: 7,
                open: true,
                comments: vec![],
            }),
            ..Fake::ready("3.3.0")
        };
        let run_503: Vec<LogLine> = (0..6).map(|i| line(i * 5, "new")).collect();
        let cases: Vec<(Served, Log, &str)> = vec![
            (
                served("3.2.0"),
                Log(Ok(vec![])),
                "riff server shows the build: failed: riff server shows 3.2.0, not 3.3.0",
            ),
            (
                served("3.3.0"),
                Log(Ok(vec![("503", run_503)])),
                "no run of 503 longer than 20 s: failed: a run of 503 took 25 s",
            ),
            (
                served("3.3.0"),
                Log(Ok(vec![("status=500", vec![line(0, "new")])])),
                "no 500 of riff-server: failed: 1 replies 500",
            ),
            (
                served("3.3.0"),
                Log(Ok(vec![("token store", vec![line(0, "new")])])),
                "no 401 or 429 of the token store: failed: 1 lines",
            ),
            (
                served("3.3.0"),
                Log(Ok(vec![(
                    "riff-server serves",
                    vec![line(0, "new"), line(5, "old"), line(9, "new")],
                )])),
                "no old instance takes the lease: failed",
            ),
            (
                Served {
                    hosts: Ok(vec![(
                        "bob@pangolin".into(),
                        "3.2.0 929605821e54 2026-10-18T15:40:18Z".into(),
                    )]),
                    ..served("3.3.0")
                },
                Log(Ok(vec![])),
                "each host takes the build: failed: bob@pangolin has 3.2.0 929605821e54",
            ),
        ];
        for (served, log, want) in cases {
            let forge = ready();
            let done = check(&forge, "v3.3.0", &served, &log).unwrap();
            assert!(matches!(done, Checked::Ran { passed: false, .. }), "{want}");
            let log = forge.log();
            assert!(
                log[0].contains("riff release check v3.3.0: failed\n"),
                "{log:?}"
            );
            assert!(log[0].contains(want), "{want}: {log:?}");
            assert!(!log.iter().any(|l| l.starts_with("close")), "{want}");
        }
    }

    /// 01M5MQHDMBYG187T0Q622XWBMN: a 500 of an instance of the old
    /// revision that stops in the switch does not fail the check. A 500
    /// of the newest revision does.
    #[test]
    fn check_counts_only_the_500s_of_the_newest_revision() {
        let forge = || Fake {
            tags: vec!["v3.3.0".into()],
            deploy: Some(deploy(DeployState::Passed)),
            issue: Some(ReleaseIssue {
                number: 7,
                open: true,
                comments: vec![],
            }),
            ..Fake::ready("3.3.0")
        };
        let serves = vec![line(0, "old"), line(5, "new")];
        for (on, passed) in [("old", true), ("new", false)] {
            let log = Log(Ok(vec![
                ("riff-server serves", serves.clone()),
                ("status=500", vec![line(40, on)]),
            ]));
            let forge = forge();
            let done = check(&forge, "v3.3.0", &served("3.3.0"), &log).unwrap();
            assert!(
                matches!(done, Checked::Ran { passed: p, .. } if p == passed),
                "a 500 of {on}: {done:?}"
            );
            let want = if passed {
                "no 500 of riff-server: passed"
            } else {
                "no 500 of riff-server: failed: 1 replies 500"
            };
            assert!(forge.log()[0].contains(want), "{:?}", forge.log());
        }
    }

    /// The sign-in of gcloud is gone: check says which check it could
    /// not do (01M58KMMP9FKRFYX58ESVRSZQX).
    #[test]
    fn check_names_each_check_that_it_could_not_do() {
        let forge = Fake {
            tags: vec!["v3.3.0".into()],
            deploy: Some(deploy(DeployState::Passed)),
            ..Fake::ready("3.3.0")
        };
        let log = Log(Err("gcloud: Reauthentication failed"));
        let done = check(&forge, "v3.3.0", &served("3.3.0"), &log).unwrap();
        let Checked::Ran {
            lines,
            passed: false,
            ..
        } = &done
        else {
            panic!("{done:?}");
        };
        // No check failed: the other leads hear an unknown result.
        assert_eq!(check_step(lines), Step::CheckUnknown);
        let log = forge.log();
        // No release issue: check makes it.
        assert!(
            log[0].starts_with("issue Release v3.3.0 in Wave 34: Release\n"),
            "{}",
            log[0]
        );
        assert!(
            log[1].contains("riff release check v3.3.0: unknown\n"),
            "{log:?}"
        );
        assert!(
            log[1].contains(
                "- no 500 of riff-server: riff could not check it: riff cannot read the Cloud Run \
                 log: gcloud: Reauthentication failed. The sign-in of gcloud may have ended (check: gcloud auth list). A person on the host runs: gcloud auth login"
            ),
            "{log:?}"
        );
        assert!(
            log[1].contains("- riff server shows the build: passed"),
            "{log:?}"
        );
    }

    /// 01M5GVPND32X3R9MYXYB5SV83W: in the container of the lead, no
    /// `gcloud` runs. Each check of the log names the step for a person
    /// on the host, and the checks of riff-server still run.
    #[test]
    fn with_no_gcloud_each_check_of_the_log_names_the_step_on_the_host() {
        let settings = crate::cloud::Settings::new("shared", "acme", "123", "europe-west1");
        let log = RunLog {
            gcloud: crate::cloud::Gcloud::at("/nonexistent/riff-test/gcloud"),
            settings,
        };
        let version = tag_version("v3.3.0").unwrap();
        let lines = checks(version, "2026-10-22T00:00:00Z", &served("3.3.0"), &log);
        let step = "riff cannot read the Cloud Run log: gcloud is not on PATH. A person on the \
                    host installs gcloud or adds it to PATH, then run: riff release check v3.3.0";
        let names: Vec<&str> = lines
            .iter()
            .filter(|(_, v)| *v == Verdict::Unknown(step.into()))
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            names,
            [
                "no run of 503 longer than 20 s",
                "no 500 of riff-server",
                "no 401 or 429 of the token store",
                "no old instance takes the lease",
            ],
            "{lines:?}"
        );
        assert!(lines.contains(&("riff server shows the build", Verdict::Passed)));
        assert!(lines.contains(&("each host takes the build", Verdict::Passed)));
        let text = check_text("v3.3.0", &lines);
        assert!(!text.contains("gcloud auth login"), "{text}");
    }

    /// When the sign-in of gcloud ended, the step names `gcloud auth login`.
    #[test]
    fn a_failed_gcloud_names_the_sign_in() {
        let version = tag_version("v3.3.0").unwrap();
        let step = unreadable_log(&anyhow!("gcloud: reauthentication needed"), version);
        assert!(step.contains("gcloud auth list"), "{step}");
        assert!(
            step.contains("gcloud auth login, then run: riff release check v3.3.0"),
            "{step}"
        );
    }

    /// The deploy job failed: the old revision still serves. check files
    /// an issue with the log, and the caller resumes the riff.
    #[test]
    fn check_after_a_failed_deploy_files_an_issue_with_the_log() {
        let forge = Fake {
            tags: vec!["v3.3.0".into()],
            deploy: Some(deploy(DeployState::Failed)),
            issue: Some(ReleaseIssue {
                number: 7,
                open: true,
                comments: vec![],
            }),
            ..Fake::ready("3.3.0")
        };
        let done = check(&forge, "v3.3.0", &served("3.2.0"), &Log(Ok(vec![]))).unwrap();
        assert!(
            matches!(done, Checked::DeployFailed { issue: 88, .. }),
            "{done:?}"
        );
        let log = forge.log();
        assert!(
            log[0].starts_with("file The deploy of v3.3.0 failed in Wave 34: Release\n"),
            "{log:?}"
        );
        assert!(log[0].contains("The failed jobs: Deploy."), "{log:?}");
        assert!(
            log[0].contains("https://github.com/o/r/actions/runs/9"),
            "{log:?}"
        );
        assert!(
            log[1].starts_with("comment #7\nriff release check v3.3.0: failed deploy"),
            "{log:?}"
        );
    }

    #[test]
    fn check_refuses_while_the_deploy_did_not_end() {
        for (state, want) in [
            (
                DeployState::Waiting,
                "it is waiting. next: a person runs riff release approve",
            ),
            (
                DeployState::Running,
                "it is deploying. next: wait for the deploy",
            ),
            (
                DeployState::Before,
                "it is published. next: wait for the jobs",
            ),
        ] {
            let forge = Fake {
                deploy: Some(deploy(state)),
                issue: Some(ReleaseIssue::default()),
                ..Fake::ready("3.3.0")
            };
            let error = check(&forge, "v3.3.0", &served("3.3.0"), &Log(Ok(vec![])))
                .unwrap_err()
                .to_string();
            assert!(error.contains(want), "{want}: {error}");
            assert!(forge.log().is_empty());
        }
        let error = check(&Fake::new(), "v3.3.0", &served("3.3.0"), &Log(Ok(vec![])))
            .unwrap_err()
            .to_string();
        assert!(error.contains("no CI run deploys v3.3.0"), "{error}");
    }

    #[test]
    fn retry_runs_the_failed_jobs_again_and_refuses_a_run_that_did_not_fail() {
        let forge = Fake {
            deploy: Some(deploy(DeployState::Failed)),
            ..Fake::new()
        };
        retry(&forge, "v3.3.0").unwrap();
        assert_eq!(forge.log(), ["rerun 9"]);
        for state in [
            DeployState::Passed,
            DeployState::Waiting,
            DeployState::Running,
        ] {
            let forge = Fake {
                deploy: Some(deploy(state)),
                ..Fake::new()
            };
            let error = retry(&forge, "v3.3.0").unwrap_err().to_string();
            assert!(
                error.contains("did not fail. riff changed nothing. next: riff release status"),
                "{error}"
            );
            assert!(forge.log().is_empty());
        }
        let error = retry(&Fake::new(), "v3.3.0").unwrap_err().to_string();
        assert!(error.contains("no CI run deploys v3.3.0"), "{error}");
    }

    #[test]
    fn rollback_deploys_an_older_release_and_marks_the_newest() {
        let forge = Fake {
            releases: vec!["3.0.0", "3.2.0", "3.3.0"],
            issue: Some(ReleaseIssue {
                number: 7,
                open: true,
                comments: vec![],
            }),
            ..Fake::new()
        };
        let done = rollback(&forge, "v3.2.0").unwrap();
        assert_eq!((done.from, done.to), (v("3.3.0"), v("3.2.0")));
        let log = forge.log();
        assert_eq!(log[0], "dispatch v3.2.0");
        assert!(
            log[1].starts_with("comment #7\nriff release rollback v3.2.0\n"),
            "{log:?}"
        );
        for bad in ["v3.3.0", "v3.1.0", "v3.4.0"] {
            let forge = Fake {
                releases: vec!["3.0.0", "3.2.0", "3.3.0"],
                ..Fake::new()
            };
            let error = rollback(&forge, bad).unwrap_err().to_string();
            assert!(
                error.contains("is not a published release older than v3.3.0. riff changed nothing. next: riff release status"),
                "{error}"
            );
            assert!(forge.log().is_empty());
        }
    }

    fn wave_item(number: u64, commit: &str) -> Merged {
        Merged {
            number,
            wave: Some("Wave 34: Release".into()),
            commit: commit.into(),
            ..Merged::default()
        }
    }

    /// `riff wave end N` refuses while the release of the wave is not
    /// checked (01M58KMMCZCE7HDNVT9PVHVZX2).
    #[test]
    fn the_wave_gate_needs_a_checked_release_or_a_reason() {
        // An item in no release.
        let forge = Fake {
            merged: vec![wave_item(41, "c41")],
            ..Fake::new()
        };
        let error = wave_gate(&forge, 34, None).unwrap_err().to_string();
        assert!(error.contains("are in no release: #41"), "{error}");
        assert!(error.contains("next: riff release cut X.Y.Z"), "{error}");
        assert!(
            error.contains("riff wave end 34 --no-release TEXT"),
            "{error}"
        );
        assert_eq!(
            wave_gate(&forge, 34, Some("only the book changed")).unwrap(),
            WaveGate::NoRelease
        );
        assert!(
            forge
                .log()
                .contains(&"note wave 34: No release: only the book changed".to_owned())
        );

        // An item in a release that waits for a person: also no skip.
        let forge = Fake {
            merged: vec![wave_item(41, "c41")],
            released: vec![("c41", vec!["3.3.0"])],
            tags: vec!["v3.3.0".into()],
            deploy: Some(deploy(DeployState::Waiting)),
            ..Fake::ready("3.3.0")
        };
        for reason in [None, Some("x")] {
            let error = wave_gate(&forge, 34, reason).unwrap_err().to_string();
            assert!(
                error.contains("the release v3.3.0 of the wave is waiting, not checked. next: a person runs riff release approve v3.3.0"),
                "{error}"
            );
        }

        // Each item in a checked release, and a release pull request of
        // the wave that counts for nothing.
        let forge = Fake {
            merged: vec![
                wave_item(41, "c41"),
                Merged {
                    release: true,
                    ..wave_item(42, "c42")
                },
                Merged {
                    wave: Some("Wave 33".into()),
                    ..wave_item(30, "c30")
                },
            ],
            released: vec![("c41", vec!["3.3.0"])],
            issue: Some(ReleaseIssue {
                number: 7,
                open: false,
                comments: vec![],
            }),
            ..Fake::new()
        };
        assert_eq!(wave_gate(&forge, 34, None).unwrap(), WaveGate::Released);
    }

    #[test]
    fn the_guards_refuse_a_worker_a_container_and_a_session_that_is_not_the_lead() {
        let worker = Caller {
            worker: true,
            ..Caller::default()
        };
        let container = Caller {
            container: true,
            agent: true,
            ..Caller::default()
        };
        let other = Caller {
            agent: true,
            lead: Some(false),
            ..Caller::default()
        };
        let unknown = Caller {
            agent: true,
            lead: None,
            ..Caller::default()
        };
        assert_eq!(worker.lead_step(), Some(Refusal::Worker));
        assert_eq!(container.lead_step(), Some(Refusal::Container));
        assert_eq!(other.lead_step(), Some(Refusal::NotTheLead));
        assert_eq!(unknown.lead_step(), Some(Refusal::LeadUnknown));
        let lead = |role| Caller {
            agent: true,
            lead: Some(true),
            role,
            ..Caller::default()
        };
        assert_eq!(
            lead(Some(PersonRole::Member)).lead_step(),
            Some(Refusal::Member)
        );
        for role in [None, Some(PersonRole::Owner), Some(PersonRole::Admin)] {
            assert_eq!(lead(role).lead_step(), None, "{role:?}");
        }
        // A person in a terminal: the role decides.
        let person = |role| Caller {
            role,
            ..Caller::default()
        };
        assert_eq!(
            person(Some(PersonRole::Member)).lead_step(),
            Some(Refusal::Member)
        );
        for role in [None, Some(PersonRole::Owner), Some(PersonRole::Admin)] {
            assert_eq!(person(role).lead_step(), None, "{role:?}");
            assert_eq!(person(role).person_step(), None, "{role:?}");
        }
        let unknown_role = Caller {
            role_unknown: true,
            ..Caller::default()
        };
        assert_eq!(unknown_role.lead_step(), Some(Refusal::RoleUnknown));
        assert_eq!(unknown_role.person_step(), Some(Refusal::RoleUnknown));
        // A member that is not the lead gets the refusal of the lead.
        let member = Caller {
            role: Some(PersonRole::Member),
            ..other
        };
        assert_eq!(member.lead_step(), Some(Refusal::NotTheLead));
        for caller in [worker, container, other, unknown, lead(None)] {
            assert_eq!(caller.person_step(), Some(Refusal::Agent));
        }
    }

    #[test]
    fn the_lead_container_passes_the_guard_of_the_lead_and_not_that_of_a_person() {
        let lead = |role, role_unknown| Caller {
            container: true,
            lead_container: true,
            agent: true,
            lead: Some(true),
            role,
            role_unknown,
            ..Caller::default()
        };
        for role in [None, Some(PersonRole::Owner), Some(PersonRole::Admin)] {
            assert_eq!(lead(role, false).lead_step(), None, "{role:?}");
        }
        assert_eq!(
            lead(Some(PersonRole::Member), false).lead_step(),
            Some(Refusal::Member)
        );
        assert_eq!(lead(None, true).lead_step(), Some(Refusal::RoleUnknown));
        assert_eq!(lead(None, false).person_step(), Some(Refusal::Agent));
        // A work container is no lead container.
        let work = Caller {
            lead_container: false,
            ..lead(None, false)
        };
        assert_eq!(work.lead_step(), Some(Refusal::Container));
    }

    #[test]
    fn the_branch_gets_the_version_in_each_file() {
        let toml =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"))
                .unwrap();
        let new = set_cargo(&toml, v("9.8.7")).unwrap();
        assert_eq!(cargo_version(&new).unwrap(), v("9.8.7"));
        assert_eq!(
            new.len(),
            toml.len() + "9.8.7".len() - cargo_version(&toml).unwrap().to_string().len()
        );
        let lock =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock"))
                .unwrap();
        let new = set_lock(&lock, v("9.8.7"));
        assert!(new.contains("name = \"riff\"\nversion = \"9.8.7\"\n"));
        assert!(new.contains("name = \"riff-server\"\nversion = \"9.8.7\"\n"));
        assert_eq!(new.lines().count(), lock.lines().count());
    }

    fn chat(seq: u64, user: &str, session: bool, body: &str) -> ChatLine {
        ChatLine {
            seq,
            user: user.into(),
            session,
            verified: true,
            body: body.into(),
        }
    }

    fn mike(user: &str) -> bool {
        user == "mike"
    }

    #[test]
    fn owner_said_takes_a_version_with_and_without_the_v() {
        let cut = chat(1, "mike", true, "riff release v4.0.1 is cut.");
        for body in [
            "approve 4.0.1",
            "approve v4.0.1",
            "I approve 4.0.1. proceed",
        ] {
            let lines = [cut.clone(), chat(2, "mike", false, body)];
            assert_eq!(owner_said(&lines, "v4.0.1", &mike), Some(2), "{body}");
            assert_eq!(owner_said(&lines, "v4.0.2", &mike), None, "{body}");
        }
    }

    #[test]
    fn owner_said_takes_approved_as_the_answer_to_the_last_release_line() {
        let lines = [
            chat(1, "mike", true, "riff release v4.0.0 is published."),
            chat(2, "mike", true, "riff release v4.0.1 is cut."),
            chat(3, "mike", false, "approved"),
        ];
        assert_eq!(owner_said(&lines, "v4.0.1", &mike), Some(3));
        assert_eq!(owner_said(&lines, "v4.0.0", &mike), None);
    }

    #[test]
    fn owner_said_ignores_a_message_before_the_cut_a_member_a_session_and_a_denial() {
        let cut = chat(2, "mike", true, "riff release v4.0.1 is cut.");
        let before = [chat(1, "mike", false, "approve 4.0.1"), cut.clone()];
        assert_eq!(owner_said(&before, "v4.0.1", &mike), None);
        for (who, session, body) in [
            ("bob", false, "approve 4.0.1"),
            ("mike", true, "approve 4.0.1"),
            ("mike", false, "do not approve 4.0.1"),
            ("mike", false, "I don't approve 4.0.1"),
        ] {
            let lines = [cut.clone(), chat(3, who, session, body)];
            assert_eq!(owner_said(&lines, "v4.0.1", &mike), None, "{body}");
        }
        let mut loud = chat(3, "mike", false, "approved");
        loud.verified = false;
        assert_eq!(owner_said(&[cut, loud], "v4.0.1", &mike), None);
    }

    #[test]
    fn owner_said_stops_at_a_stop_after_the_approval() {
        let cut = chat(1, "mike", true, "riff release v4.0.1 is cut.");
        let said = chat(2, "mike", false, "approved");
        for body in ["stop", "wait for me"] {
            let lines = [cut.clone(), said.clone(), chat(3, "mike", false, body)];
            assert_eq!(owner_said(&lines, "v4.0.1", &mike), None, "{body}");
        }
        let again = [
            cut.clone(),
            said.clone(),
            chat(3, "mike", false, "stop"),
            chat(4, "mike", false, "approved"),
        ];
        assert_eq!(owner_said(&again, "v4.0.1", &mike), Some(4));
        let member = [cut, said, chat(3, "bob", false, "stop")];
        assert_eq!(owner_said(&member, "v4.0.1", &mike), Some(2));
    }
}
