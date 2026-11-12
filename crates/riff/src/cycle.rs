//! The release cycle in riff commands: `riff release test`, `cut`,
//! `status`, `approve`, `check`, `retry` and `rollback` (#671, #1104,
//! #1949, #1983).
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
//! The version of the next release is on `main` from the start of its
//! wave: the cut of a release opens the pull request of the next version
//! (01M7641T2JH2K6SYBCWQGM56FY). So each commit of `main` has the version
//! of its release, and CI builds and tests it once, with its release
//! build and its release test ([`Head`], 01M7641T03M977BBF4M8AQZ74M). The
//! cut tags the last commit of `main`: no version commit, no release pull
//! request and no new build (01M7641BHHND5JJA8DGJPFNAN7).
//!
//! | Command | Who | What |
//! |---|---|---|
//! | `riff release test vX.Y.Z --host HOST` | the lead or a person; not a worker | the real-session test of the last commit of `origin/main` on the daemon of a host, and the release issue ([`test_target`], [`crate::daemon::release_test`], 01M76428TRD7KWC4C8V5NNPNC7) |
//! | `riff release cut X.Y.Z` | an admin of the riff, or its lead; the ruleset `releases` lets the admin role and the lead App push the tag | the tag on the last commit of `origin/main`, the GitHub release, the pull request of the next version, the chat line to each admin, when each alert has a review ([`cut`], [`crate::alerts`], 01M7641BHHND5JJA8DGJPFNAN7, 01M7641T7FX9416CYY1XKHTJE6) |
//! | `riff release status` | each session | the versions, what waits, the state and the next step ([`status`], 01M76428NPN7HZCNFCJW8H67RS) |
//! | `riff release approve vX.Y.Z` | only a person in a terminal: never a session, also not the lead (01M6WZ40SFVZ6CRQ1H4Z27CWP4) | the pause of the riff, then the approval of `production`; it refuses while a lead holds the version, unless a person passes `--over-holds` ([`approve`], 01M58KMM3NFGJRW4CCV50KJQB2, 01M5SNV7E775DRY9QY60E8JPJR) |
//! | `riff release hold vX.Y.Z REASON` | the lead of any user, also of a member | the hold of the version: each other lead hears it, `riff release status` shows it ([`crate::hold`], 01M5SNV78EBAYY4SZ891DM2MQG) |
//! | `riff release check vX.Y.Z` | the lead | the checks after the deploy, the result on the release issue ([`check`], 01M58KMM5ZPAQ289S2YJVQ5WW6) |
//! | `riff release retry vX.Y.Z` | the lead | the failed jobs of the CI run again ([`retry`], 01M58KMM8CV9731AWGVZQBB80D) |
//! | `riff release rollback vPREV` | the lead | the deploy of an older release ([`rollback`], 01M58KMMAPJ33CQWR8GRX62GMR) |
//! | `riff wave end N` | the lead | the end of the wave, after a checked release ([`wave_gate`], 01M58KMMCZCE7HDNVT9PVHVZX2) |
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> building: the last item of the wave merges
//!     building --> built: the release build and the release test of the last commit pass
//!     built --> tested: riff release test, or just real-session
//!     tested --> published: riff release cut (the tag, the release, the pull request of the next version)
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
//! The state of a release ([`stage`], 01M76428NPN7HZCNFCJW8H67RS) comes
//! from the forge. Before the tag, it is the version on `origin/main`,
//! and the last commit there with its release build, its release test and
//! the status [`REAL_SESSION`] ([`Head`]). After it, it is the newest CI
//! run that deploys the tag ([`Deploy`]), and the release issue. `riff
//! release check` writes its result on the release issue with a first line
//! [`check_mark`], and closes the issue on a pass. `riff release
//! rollback` writes [`rollback_mark`] on the release issue that it
//! replaces.
//!
//! `cut` refuses a version that is not the version on `origin/main` or
//! not newer than the last release, while a deploy waits for a person,
//! while the last commit of `origin/main` lacks a check, and while an
//! alert of the repository has no review ([`crate::alerts`]). A second
//! cut of the same version finishes a cut that stopped part way
//! (01M7641T51JY09YVVKR4WGEE72). The pull request of the next version
//! says `Refs #N` and `Part of #N` for the release issue, never
//! `Closes #N`: the release issue closes only when the checks after the
//! deploy pass. It has no label: riff finds it by its title
//! ([`next_title`]). The cut wrote its one commit, so the cut sets its
//! status `riff/verify`, and the Gate decides its merge, as for each other
//! pull request (01M76SBEV64SNKCS1AE1KXWNFM).
//!
//! ```mermaid
//! flowchart TD
//!     C["riff release cut 3.3.0"] --> V{"origin/main has 3.3.0,<br/>newer than the last release?"}
//!     V -- no --> R["refused: riff changed nothing"]
//!     V -- yes --> H{"its last commit: the release build,<br/>riff/release-test and riff/real-session passed?"}
//!     H -- no --> R
//!     H -- yes --> A{"each alert has a review?"}
//!     A -- no --> R
//!     A -- yes --> T["the tag v3.3.0 on that commit:<br/>CI takes its release build"]
//!     T --> G["the GitHub release, with the notes"]
//!     G --> B["the branch release-next-v3.4.0:<br/>one commit, riff/verify passed"]
//!     B --> P["the pull request: Set the next version 3.4.0,<br/>auto-merge on"]
//!     P --> M["the Gate passes: it merges"]
//! ```
//!
//! Each step also posts one note to each other lead of the riff
//! ([`crate::hold`], 01M5SNV75DRFHR26BT2M8243VV): the command posts the
//! note of its own step, and the watch of the daemon posts the note of the
//! wait and of the end of the deploy ([`crate::watches`]).
//!
//! The guards of the callers are in [`Caller`]: the steps of the lead
//! refuse a worker, a container session and a session that is not the
//! lead ([`Caller::lead_step`], 01M7641T7FX9416CYY1XKHTJE6). The steps of a person refuse each agent
//! session, also the lead ([`Caller::person_step`],
//! 01M58B0JSQMWNCMSNX19536XT8, 01M6WZ40SFVZ6CRQ1H4Z27CWP4). riff reads no
//! approval from riff chat. GitHub decides the push of the tag: the
//! ruleset `releases` lets the admin role of the repository and the lead
//! App of the riff push it (01M6WZ40PZNBHW2BZHMQX80WJ8), and `cut`
//! checks no role of `gh` (01M7641BHHND5JJA8DGJPFNAN7).
//!
//! ```mermaid
//! flowchart LR
//!     L["the lead in its container<br/>riff-OWNER-lead[bot]"] -- "riff release cut:<br/>push the tag v*" --> R{"the ruleset releases"}
//!     A["an admin in a terminal"] -- "riff release cut" --> R
//!     W["a worker<br/>riff-OWNER[bot]"] -. "refused" .-> R
//!     R --> T["the tag and the CI run"]
//!     T --> D["the job Deploy waits<br/>for production"]
//!     P["a person in a terminal"] -- "riff release approve" --> D
//!     L -. "refused, also with the word<br/>of an admin in riff chat" .-> D
//! ```
//!
//! Each step goes through [`Forge`]. [`GitHub`] runs `gh` and `git`; a
//! test gives a fake forge. No person signs in to read the Cloud Run log
//! after a deploy. The CI job `Check the deploy` of the run of the tag
//! signs in as `riff-check`, which only reads the log. It runs `riff
//! cloud check`: [`LogMeasures::read`] reads the log through [`RunLog`],
//! and [`set_deploy_check`] sets the status [`DEPLOY_CHECK`] of the
//! tagged commit (01M76SS6T89DPNV7S4SWY1R7F3). `riff release check`
//! takes the four checks of the log from that status
//! ([`Forge::deploy_check`], [`log_of`]), and runs no `gcloud`
//! (01M76SSFTGW41RHHV445ZM5RHY). Before the end of the job, with no
//! status of this deploy, it refuses and changes nothing
//! ([`check_waits`], 01M77BT4XB4T8MQVCY7MK0B843): the watch of the deploy
//! gives the lead the step of the check only then
//! ([`crate::watches`], 01M77BT0489T9R084WEFHPMCR5).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as the job Deploy
//!     participant C as the job Check the deploy (riff-check)
//!     participant G as Cloud Logging
//!     participant H as GitHub
//!     participant L as riff release check (the lead)
//!     D->>C: the time before the deploy
//!     Note over C: waits 5 minutes
//!     C->>G: gcloud logging read, 4 filters
//!     G-->>C: the lines
//!     C->>H: the status riff/deploy-check of the tagged commit
//!     L->>H: the status riff/deploy-check of the tag
//!     H-->>L: the 4 checks of the log
//! ```
//!
//! A command of the cycle calls `gh` through [`Gh::retrying`]: a 502, 503
//! or 504 of GitHub runs the call again, up to 3 times
//! (01M6H12ET0FQ553QHGVF5XJMQ9). A limit of GitHub gets no new try: the
//! call fails with [`crate::ghcalls::LimitOut`], which says when the
//! counter starts again and to run the command again then
//! ([`Gh::stopping_on_limit`], 01M6QB459TJVK6CS9FHWH6EVK2). [`check`]
//! gives the result of the checks to its caller before it writes the
//! comment, so a fault of the comment hides nothing
//! (01M6H12QRZFBDTT5D5VQ4FB4C1).
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

use crate::alerts::{self, Alert};
use crate::hold::{Hold, Step};
use crate::pr::{self, Gh};
use crate::text;

/// The label of a release issue. No pull request has it
/// (01M76SBEV64SNKCS1AE1KXWNFM).
pub const LABEL: &str = "release";

/// The GitHub environment of the deploy to the shared riff.
pub const PRODUCTION: &str = "production";

/// The workflow that runs for a tag.
pub const WORKFLOW: &str = "CI";

/// The check of a release tag in the tree.
pub const CHECK: &str = "deploy/release-check.sh";

/// The script of a commit that names the run whose artifact of a tree a
/// run of CI takes (01M75YKD063DE6ZFA0H3K8BNZB). The cut asks it as a tag
/// does, so the cut and the tag agree on the release build
/// (01M7641T03M977BBF4M8AQZ74M).
pub const ARTIFACT_RUN: &str = "deploy/artifact-run.sh";

/// The artifacts of the release build of a tree, with no tree: the static
/// binaries of the profile `release`, and the binaries of a Mac
/// (01M75TMWKT8E0YYEENWVX4B67A). The name of each is `NAME-TREE`.
pub const RELEASE_BUILD: [&str; 3] = ["static-release", "macos-riff", "mac-session-riff"];

/// The commit status of the release test of the release binaries
/// (01M75TN3M9JEVZQXEA2E8FTBKS). `deploy/release-test-status.sh` sets it
/// in the CI run of a push to `main`.
pub const RELEASE_TEST: &str = "riff/release-test";

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

/// The title of the release issue.
pub fn title(version: Semver) -> String {
    format!("Release {}", tag(version))
}

/// The title of the pull request that sets the next version `version` on
/// `main` (01M7641T2JH2K6SYBCWQGM56FY).
///
/// ```
/// let title = riff::cycle::next_title(riff::cycle::version("3.4.0").unwrap());
/// assert_eq!(title, "Set the next version 3.4.0");
/// ```
pub fn next_title(version: Semver) -> String {
    format!("Set the next version {version}")
}

/// The version of the pull request of the next version with the title
/// `title` ([`next_title`]), or `None` for another title.
///
/// ```
/// use riff::cycle::{next_of_title, version};
///
/// assert_eq!(next_of_title("Set the next version 3.4.0"), Some(version("3.4.0").unwrap()));
/// assert_eq!(next_of_title("Release v3.3.0"), None);
/// ```
pub fn next_of_title(title: &str) -> Option<Semver> {
    version(title.strip_prefix("Set the next version ")?).ok()
}

/// The default next version after the release `version`: the next minor
/// (01M7641T2JH2K6SYBCWQGM56FY).
///
/// ```
/// use riff::cycle::{next_minor, version};
///
/// assert_eq!(next_minor(version("3.3.1").unwrap()).to_string(), "3.4.0");
/// ```
pub fn next_minor(version: Semver) -> Semver {
    Semver {
        minor: version.minor + 1,
        patch: 0,
        ..version
    }
}

/// The body of the release issue of `version`: the line "Done when" and
/// the steps of the release, with no part for the verify of a pull request
/// (01M76SBPDSGS0SATQV9RA1FRG1).
///
/// ```
/// let body = riff::cycle::issue_body(riff::cycle::version("3.3.0").unwrap());
/// assert!(body.starts_with("The release v3.3.0.\n"));
/// assert!(body.contains("Done when: the release is published"));
/// assert!(body.contains("- The real-session test passed on the last commit of `main`.\n"));
/// assert!(body.contains("- The pull request of the next version merged.\n"));
/// assert!(!body.contains("verify"));
/// ```
pub fn issue_body(version: Semver) -> String {
    format!(
        "The release {}.\n\n\
         Done when: the release is published, the deploy to the shared riff passed, and the \
         checks after the deploy passed.\n\n\
         Steps of the release:\n\
         - The real-session test passed on the last commit of `main`.\n\
         - The tag is on that commit, and the release is published.\n\
         - The pull request of the next version merged.\n\
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

/// A merged pull request since the last release, for the notes of the
/// release.
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
    /// True for a pull request of the next version: its title is a
    /// [`next_title`] (01M76SBEV64SNKCS1AE1KXWNFM).
    pub next: bool,
    /// Its merge commit.
    pub commit: String,
}

/// The pull request of the next version, as its watch reads it
/// ([`crate::watches`], 01M76SBPGH2218K3ED569T051D).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pull {
    /// Its state: `OPEN`, `CLOSED` or `MERGED`.
    pub state: String,
    /// Its head commit.
    pub head: String,
}

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
    /// The end of the job `Deploy`, as RFC 3339, or empty.
    pub ended: String,
    /// The names of the jobs that failed.
    pub failed: Vec<String>,
    /// True for a run by hand on `main` with the input `tag`, as of
    /// `riff release rollback`: it has no job `Check the deploy`.
    pub by_hand: bool,
    /// Where its job `Check the deploy` ([`CHECK_JOB`]) is.
    pub check_job: CheckJob,
}

/// The name of the CI job that checks the Cloud Run log after the job
/// `Deploy` (01M76SS6T89DPNV7S4SWY1R7F3).
pub const CHECK_JOB: &str = "Check the deploy";

/// Where the job [`CHECK_JOB`] of a CI run is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckJob {
    /// It did not end: it waits for the job `Deploy` or for a runner, or
    /// it runs. A run that did not end and has no such job yet counts too.
    Runs,
    /// It ended, with its conclusion, for example `success` or `failure`.
    Ended(String),
    /// The run ended with no such job.
    Missing,
}

/// A commit status of GitHub, as [`log_of`] reads it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitStatus {
    /// `success`, `failure`, `error` or `pending`.
    pub state: String,
    /// Its description.
    pub description: String,
    /// The time of its last change, as RFC 3339.
    pub at: String,
}

/// The status [`DEPLOY_CHECK`] of the commit of a release tag.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeployCheck {
    /// The commit of the tag.
    pub commit: String,
    /// The status, or `None` when the commit has none.
    pub status: Option<CommitStatus>,
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

/// The state of a test on a commit: the commit status [`REAL_SESSION`] of
/// the real-session test (01M5924C8BTT6CW05Q0YZJ1GCY), or [`RELEASE_TEST`]
/// of the release test (01M7641T03M977BBF4M8AQZ74M).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestState {
    /// The test runs: on the daemon of a host, or in CI.
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
    /// The last commit of `origin/main`.
    fn main_commit(&self) -> Result<String>;
    /// The artifacts of the release build of `commit` ([`RELEASE_BUILD`])
    /// that a tag of the commit cannot take: [`ARTIFACT_RUN`] of the
    /// commit names no run for them (01M7641T03M977BBF4M8AQZ74M). Empty
    /// when the commit has its release build.
    fn release_build(&self, commit: &str) -> Result<Vec<String>>;
    /// The state of the release test of `commit`: the status
    /// [`RELEASE_TEST`]. `None` with no status.
    fn release_test(&self, commit: &str) -> Result<Option<TestState>>;
    /// Makes the branch of the next version `version` from `origin/main`,
    /// with the version in each file, checks it with [`CHECK`], and
    /// pushes it (01M7641T2JH2K6SYBCWQGM56FY). Returns the name of the
    /// branch and its one commit.
    fn push_branch(&self, version: Semver) -> Result<(String, String)>;
    /// Opens the pull request of `branch`, with no label, and turns on
    /// auto-merge. Returns its number and its URL.
    fn open_pr(&self, branch: &str, title: &str, wave: &str, body: &str) -> Result<(u64, String)>;
    /// The open pull request of the next version, by its title
    /// ([`next_title`]): its number and its version.
    fn next_pr(&self) -> Result<Option<(u64, Semver)>>;
    /// The pull request `number`, for its watch.
    fn pull(&self, number: u64) -> Result<Pull>;
    /// The checks of the full check of the repository: each check that the
    /// ruleset of its default branch requires, but `riff/verify`
    /// (01M5QYDE41FVFX26EC8K5QGYH4).
    fn full_check(&self) -> Result<Vec<String>>;
    /// The state of the full check `checks` of `commit`.
    fn gate(&self, checks: &[String], commit: &str) -> Result<pr::Gate>;
    /// Sets the status `riff/verify` of `commit` to a success.
    fn pass_verify(&self, commit: &str, description: &str) -> Result<()>;
    /// True when the forge has the tag `tag`.
    fn tag_exists(&self, tag: &str) -> Result<bool>;
    /// Makes the annotated tag `tag` on `commit`, and pushes only it. The
    /// ruleset `releases` of the forge decides the push.
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
    /// The state of the test [`REAL_SESSION`] on `commit`: running, passed
    /// or failed. `None` with no status.
    fn test_state(&self, commit: &str) -> Result<Option<TestState>>;
    /// The newest CI run that deploys `tag`, or `None`.
    fn deploy(&self, tag: &str) -> Result<Option<Deploy>>;
    /// The commit of the release tag `tag` and its status [`DEPLOY_CHECK`]
    /// (01M76SSFTGW41RHHV445ZM5RHY).
    fn deploy_check(&self, tag: &str) -> Result<DeployCheck>;
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
    /// The title of the newest release issue of the wave `title`, open or
    /// closed, or `None`: `riff release test` or the cut made it
    /// ([`open_issue`], 01M75N6YNHQQHGS0VGKTKSNJTR, 01M7642H27T4862WZENFAN4277).
    fn wave_release(&self, title: &str) -> Result<Option<String>>;
    /// True when the repository has the table `[release]` in its riff
    /// config ([`crate::release_config`]): its waves end with a release.
    fn has_release(&self) -> bool;
    /// The open and the dismissed alerts of the repository, of each kind
    /// ([`crate::alerts::read`]).
    fn alerts(&self) -> Result<Vec<Alert>>;
}

/// The state of a release in the cycle (01M76428NPN7HZCNFCJW8H67RS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The version is on `origin/main`, and its last commit has no release
    /// build or no passed release test yet ([`Head::built`]).
    Building,
    /// The last commit of `origin/main` has its release build and a passed
    /// release test: the real-session test comes next.
    Built,
    /// The real-session test passed on the last commit of `origin/main`:
    /// the cut comes next.
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
    /// assert_eq!(riff::cycle::State::Building.word(), "building");
    /// ```
    pub fn word(self) -> &'static str {
        match self {
            State::Building => "building",
            State::Built => "built",
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

/// The last commit of `origin/main` before the tag of its release, with
/// the checks that the cut needs (01M7641BHHND5JJA8DGJPFNAN7,
/// 01M7641T03M977BBF4M8AQZ74M).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Head {
    /// The commit.
    pub commit: String,
    /// The artifacts of its release build that a tag cannot take
    /// ([`Forge::release_build`]). Empty when it has its release build.
    pub missing: Vec<String>,
    /// The state of its release test ([`RELEASE_TEST`]).
    pub release_test: Option<TestState>,
    /// The state of its real-session test ([`REAL_SESSION`]).
    pub real_session: Option<TestState>,
}

impl Head {
    /// True when the commit has its release build and a passed release
    /// test: the real-session test can run on it.
    ///
    /// ```
    /// use riff::cycle::{Head, TestState};
    ///
    /// let head = Head { commit: "c1".into(), release_test: Some(TestState::Passed), ..Head::default() };
    /// assert!(head.built());
    /// assert!(!Head { missing: vec!["macos-riff-t1".into()], ..head.clone() }.built());
    /// assert!(!Head { release_test: Some(TestState::Failed), ..head }.built());
    /// ```
    pub fn built(&self) -> bool {
        self.missing.is_empty() && self.release_test == Some(TestState::Passed)
    }

    /// The line of the commit and of its checks in `riff release status`.
    ///
    /// ```
    /// use riff::cycle::{Head, TestState};
    ///
    /// let head = Head {
    ///     commit: "c1".into(),
    ///     missing: vec!["macos-riff-t1".into()],
    ///     release_test: Some(TestState::Passed),
    ///     real_session: None,
    /// };
    /// assert_eq!(
    ///     head.line(),
    ///     "last commit of origin/main: c1. release build: missing macos-riff-t1. release test: \
    ///      passed. real-session test: none."
    /// );
    /// ```
    pub fn line(&self) -> String {
        let build = match self.missing.is_empty() {
            true => "passed".to_owned(),
            false => format!("missing {}", self.missing.join(", ")),
        };
        let word = |state: Option<TestState>| state.map_or("none", TestState::word);
        format!(
            "last commit of origin/main: {}. release build: {build}. release test: {}. \
             real-session test: {}.",
            self.commit,
            word(self.release_test),
            word(self.real_session)
        )
    }
}

/// A release and its state in the cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    /// Its version.
    pub version: Semver,
    /// Its state.
    pub state: State,
    /// Before the tag: the last commit of `origin/main` and its checks.
    pub head: Option<Head>,
    /// The newest CI run that deploys it.
    pub deploy: Option<Deploy>,
    /// For [`State::Failed`]: true when the checks after the deploy
    /// failed, false when the deploy failed.
    pub checks_failed: bool,
    /// The newest published release before it.
    pub previous: Option<Semver>,
}

impl Stage {
    /// A release of `version` at `state`, with no fact more.
    ///
    /// ```
    /// use riff::cycle::{Stage, State, version};
    ///
    /// let stage = Stage::at(version("3.3.0")?, State::Published);
    /// assert_eq!((stage.head, stage.deploy, stage.previous), (None, None, None));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn at(version: Semver, state: State) -> Stage {
        Stage {
            version,
            state,
            head: None,
            deploy: None,
            checks_failed: false,
            previous: None,
        }
    }
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

/// The last commit of `origin/main` with its checks
/// (01M7641T03M977BBF4M8AQZ74M).
pub fn head(forge: &dyn Forge) -> Result<Head> {
    let commit = forge.main_commit()?;
    Ok(Head {
        missing: forge.release_build(&commit)?,
        release_test: forge.release_test(&commit)?,
        real_session: forge.test_state(&commit)?,
        commit,
    })
}

/// The commit for `riff release test TAG` (01M76428TRD7KWC4C8V5NNPNC7):
/// the last commit of `origin/main`. It refuses a version that is not on
/// `origin/main` before its tag, a commit with no release build or no
/// passed release test, and a commit that has a pass or a test that runs.
pub fn test_target(forge: &dyn Forge, tag_text: &str) -> Result<String> {
    let version = tag_version(tag_text)?;
    let tag = tag(version);
    let stage = stage(forge, version)?;
    let head = match stage {
        Some(Stage {
            state: State::Built,
            head: Some(head),
            ..
        }) => head,
        Some(stage) if stage.state == State::Building => bail!(
            "riff release test: the last commit of origin/main has no release build and no \
             passed release test yet. riff changed nothing. next: {}",
            next(&stage)
        ),
        Some(stage) if stage.state == State::Tested => bail!(
            "riff release test: the real-session test passed on the last commit of origin/main \
             already. riff changed nothing. next: {}",
            next(&stage)
        ),
        Some(_) => bail!(
            "riff release test: the release {tag} has its tag. The real-session test runs \
             before the tag. riff changed nothing. next: riff release status"
        ),
        None => bail!(
            "riff release test: {version} is not the version on origin/main with no release. \
             riff changed nothing. next: riff release status"
        ),
    };
    if head.real_session == Some(TestState::Running) {
        bail!(
            "riff release test: the test runs on {} already. riff changed nothing. next: riff \
             release status",
            head.commit
        );
    }
    Ok(head.commit)
}

/// The open release issue of `version`, made in the current wave when
/// there is none (01M7641T9XH5NAXHE8YKBR4JJY). `riff release test` makes
/// it, so the daemon can write the reason of a fail there
/// (01M5NC1CANKNF7JWVVFPX1X340), and the cut finds it.
pub fn open_issue(forge: &dyn Forge, version: Semver) -> Result<u64> {
    let wave = forge.current_wave()?;
    forge.release_issue(&title(version), &wave, &issue_body(version))
}

/// The state of the release `version` in the cycle
/// (01M76428NPN7HZCNFCJW8H67RS), or `None` when it has no tag and it is
/// not the version on `origin/main`.
pub fn stage(forge: &dyn Forge, version: Semver) -> Result<Option<Stage>> {
    stage_in(forge, version, &forge.releases()?)
}

/// [`stage`] with the published releases `releases`, read once.
fn stage_in(forge: &dyn Forge, version: Semver, releases: &[Semver]) -> Result<Option<Stage>> {
    let tag = tag(version);
    let title = title(version);
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
        previous,
        ..Stage::at(version, State::Building)
    };
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
    } else if forge.main_version()? == version {
        let head = head(forge)?;
        let state = if !head.built() {
            State::Building
        } else if head.real_session == Some(TestState::Passed) {
            State::Tested
        } else {
            State::Built
        };
        stage.head = Some(head);
        state
    } else {
        return Ok(None);
    };
    Ok(Some(stage))
}

/// The next step of `stage`, for the line `next:` of each command
/// (01M58KMKWRKSV1DR42FRH2K43X).
///
/// ```
/// use riff::cycle::{Head, Stage, State, TestState, next, version};
///
/// let mut stage = Stage { previous: Some(version("3.2.0")?), ..Stage::at(version("3.3.0")?, State::Tested) };
/// assert_eq!(next(&stage), "riff release cut 3.3.0");
/// stage.state = State::Built;
/// stage.head = Some(Head { commit: "c1".into(), release_test: Some(TestState::Passed), ..Head::default() });
/// assert_eq!(next(&stage), "riff release test v3.3.0 --host HOST, or just real-session on origin/main");
/// stage.state = State::Waiting;
/// assert_eq!(next(&stage), "a person runs riff release approve v3.3.0 in a terminal");
/// stage.state = State::Failed;
/// assert_eq!(next(&stage), "riff release retry v3.3.0, or riff release rollback v3.2.0");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn next(stage: &Stage) -> String {
    let version = stage.version;
    let tag = tag(version);
    let run = stage.deploy.as_ref().map(|d| d.url.as_str());
    let previous = stage
        .previous
        .map_or_else(|| "vPREVIOUS".to_owned(), self::tag);
    let ahead = next_minor(version);
    let forward = format!(
        "after the fix merges, riff release test {} --host HOST and riff release cut {ahead}",
        self::tag(ahead)
    );
    let head = stage.head.clone().unwrap_or_default();
    let commit = match head.commit.is_empty() {
        true => "the last commit of origin/main".to_owned(),
        false => head.commit.clone(),
    };
    match stage.state {
        State::Building if head.release_test == Some(TestState::Failed) => format!(
            "the release test of {commit} failed: file the bug in the wave; the release build \
             of its fix runs after its merge to main"
        ),
        State::Building if !head.missing.is_empty() || stage.head.is_none() => format!(
            "wait for the release build of {commit} in the CI run of its push to main; when a \
             job of it failed, run that job again"
        ),
        State::Building => {
            format!("wait for the release test of {commit} in the CI run of its push to main")
        }
        State::Built => match head.real_session {
            Some(TestState::Running) => {
                format!("wait for the real-session test on {commit}; riff release status shows it")
            }
            Some(TestState::Failed) => format!(
                "file the bug in the wave, then riff release test {tag} --host HOST after its \
                 fix merges"
            ),
            _ => {
                format!("riff release test {tag} --host HOST, or just real-session on origin/main")
            }
        },
        State::Tested => format!("riff release cut {version}"),
        State::Published => match run {
            Some(run) => format!("wait for the jobs of the CI run {run}"),
            None => format!("wait for the CI run of the tag {tag}"),
        },
        State::Waiting => format!("a person runs riff release approve {tag} in a terminal"),
        State::Deploying => format!("wait for the deploy: {}", run.unwrap_or("the CI run")),
        State::Deployed => format!("riff release check {tag}"),
        State::Failed if stage.checks_failed => {
            format!("fix forward: {forward}; or roll back with riff release rollback {previous}")
        }
        State::Failed => format!("riff release retry {tag}, or riff release rollback {previous}"),
        State::Checked => "riff wave end N, for the wave of the release".to_owned(),
        State::RolledBack => format!("fix forward: {forward}"),
    }
}

/// The lines `release:` and `next:` of `stage`.
///
/// ```
/// use riff::cycle::{Stage, State, stage_lines, version};
///
/// let stage = Stage::at(version("3.3.0")?, State::Deployed);
/// assert_eq!(stage_lines(Some(&stage)), "release: v3.3.0 deployed\nnext: riff release check v3.3.0\n");
/// assert_eq!(
///     stage_lines(None),
///     "release: none\nnext: riff release test vX.Y.Z --host HOST at the end of the wave, then \
///      riff release cut X.Y.Z\n"
/// );
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
        None => "release: none\nnext: riff release test vX.Y.Z --host HOST at the end of the \
                 wave, then riff release cut X.Y.Z\n"
            .to_owned(),
    }
}

/// The release that the cycle is at now (01M76428NPN7HZCNFCJW8H67RS): the
/// newest published release until its check passes, else the version on
/// `origin/main`. The cut opens the pull request of the next version, so
/// `origin/main` moves on before the deploy of its release.
pub fn current(forge: &dyn Forge) -> Result<Option<Stage>> {
    current_in(forge, &forge.releases()?)
}

/// [`current`] with the published releases `releases`, read once.
fn current_in(forge: &dyn Forge, releases: &[Semver]) -> Result<Option<Stage>> {
    if let Some(newest) = last(releases)
        && let Some(stage) = stage_in(forge, newest, releases)?
        && stage.state != State::Checked
    {
        return Ok(Some(stage));
    }
    stage_in(forge, forge.main_version()?, releases)
}

/// The last published release: the highest version of `releases`.
pub fn last(releases: &[Semver]) -> Option<Semver> {
    releases.iter().copied().max()
}

/// The release that [`cut`] made, when it made one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Released {
    /// The commit of the tag, when the cut pushed the tag. `None` when the
    /// tag was there, from a cut that stopped part way.
    pub commit: Option<String>,
    /// The URL of the GitHub release.
    pub url: String,
    /// The URL of the CI run of the tag, when GitHub shows it.
    pub run: Option<String>,
}

/// What [`cut`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    /// The release issue.
    pub issue: u64,
    /// The release, or `None` when it was there and the cut opened only the
    /// pull request of the next version (01M7641T51JY09YVVKR4WGEE72).
    pub release: Option<Released>,
    /// The next version.
    pub next: Semver,
    /// The number of the pull request of the next version.
    pub number: u64,
    /// Its URL.
    pub url: String,
}

/// The waves of `merged`, in the order of their numbers, each with its
/// pull requests. A pull request of the next version is left out.
fn waves(merged: &[Merged]) -> Vec<(String, Vec<&Merged>)> {
    let mut waves: Vec<(String, Vec<&Merged>)> = Vec::new();
    for m in merged.iter().filter(|m| !m.next) {
        let wave = m.wave.clone().unwrap_or_else(|| "No wave".to_owned());
        match waves.iter_mut().find(|(w, _)| *w == wave) {
            Some((_, list)) => list.push(m),
            None => waves.push((wave, vec![m])),
        }
    }
    waves.sort_by_key(|(w, _)| hygiene::wave_number(w).unwrap_or(u64::MAX));
    waves
}

/// The words of the lead for the notes of a release
/// (01M765808DM2V10BRGAAPE8CCT): `riff release cut --why` and `--run`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Words {
    /// The reason of the level, in one line for a person. It follows the
    /// level line.
    pub why: Option<String>,
    /// Each command that each person runs, for example `riff update --tag
    /// v3.3.0` when the old `riff` cannot read the new server.
    pub run: Vec<String>,
}

/// The notes of the release `new` (01M7641BHHND5JJA8DGJPFNAN7,
/// 01M765808DM2V10BRGAAPE8CCT): the level line with the reason of
/// `words`, the commands that each person runs, the list "Waves in this
/// release", and the items of each wave. GitHub adds its generated notes
/// after them.
///
/// ```
/// use riff::cycle::{Merged, Words, notes, version};
///
/// let m = |n, wave: &str, issue| Merged {
///     number: n, title: format!("Change {n}"), wave: Some(wave.into()), issue: Some(issue), ..Merged::default()
/// };
/// let merged = [m(41, "Wave 34: Status", 12), m(40, "Wave 33", 11)];
/// let words = Words { why: Some("New commands.".into()), run: vec!["riff update --tag v3.3.0".into()] };
/// let text = notes(version("3.2.0")?, version("3.3.0")?, &words, &merged);
/// assert!(text.starts_with(
///     "Level: minor, 3.3.0. New commands.\n\nEach person runs:\n\n- `riff update --tag v3.3.0`\n\n"
/// ));
/// assert!(text.contains("Waves in this release: Wave 33, Wave 34: Status.\n"));
/// assert!(text.contains("Wave 34: Status:\n\n- #12: Change 41 (#41).\n"));
/// let plain = notes(version("3.2.0")?, version("3.3.0")?, &Words::default(), &merged);
/// assert!(plain.starts_with("Level: minor, 3.3.0.\n\nWaves in this release:"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn notes(last: Semver, new: Semver, words: &Words, merged: &[Merged]) -> String {
    let mut out = format!("Level: {}, {new}.", level(last, new));
    if let Some(why) = words
        .why
        .as_deref()
        .map(str::trim)
        .filter(|w| !w.is_empty())
    {
        out.push(' ');
        out.push_str(why);
    }
    out.push_str("\n\n");
    let run: Vec<&str> = words
        .run
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .collect();
    if !run.is_empty() {
        out.push_str("Each person runs:\n\n");
        for command in run {
            out.push_str(&format!("- `{command}`\n"));
        }
        out.push('\n');
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

/// The summary of the pull request of the next version `next` after the
/// release `release` (01M7641T2JH2K6SYBCWQGM56FY): the line `Part of` for
/// the release issue `issue`, and the version. [`pr::body`] adds the link
/// line and the trailers.
///
/// ```
/// use riff::cycle::{next_summary, version};
///
/// let text = next_summary(7, version("3.3.0")?, version("3.4.0")?);
/// assert!(text.starts_with("Part of #7: the next version after v3.3.0."));
/// assert!(text.contains("3.4.0 in `Cargo.toml` and `Cargo.lock`"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn next_summary(issue: u64, release: Semver, next: Semver) -> String {
    format!(
        "Part of #{issue}: the next version after {}. The issue closes when the checks after the \
         deploy pass.\n\nThis sets {next} in `Cargo.toml` and `Cargo.lock`. It is the first commit \
         of the next wave, so each commit of the wave has the version of its release.\n",
        tag(release)
    )
}

/// The body of the pull request of the next version, with the issue
/// hygiene of the release issue `issue` in `wave` checked.
fn next_body(issue: u64, release: Semver, next: Semver, wave: &str) -> Result<String> {
    let body = pr::body(
        pr::Link::Refs(issue),
        &next_summary(issue, release, next),
        wave,
    )?;
    let errors = hygiene::check_pr(
        &hygiene::PullRequest::new(&next_title(next), &body, Some(wave)),
        Some(&hygiene::Issue::new(issue, "OPEN", Some(wave))),
    );
    if !errors.is_empty() {
        let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
        bail!(
            "riff release cut: the pull request of the next version breaks issue hygiene:\n{}\n\
             next: fix the release issue #{issue}, then riff release cut {release}",
            lines.join("\n")
        );
    }
    Ok(body)
}

/// The description of the status `riff/verify` that the cut sets on the
/// commit of the pull request of the next version
/// (01M76SBEV64SNKCS1AE1KXWNFM).
pub const NEXT_VERIFY: &str = "pass: riff release cut wrote this commit, the next version only";

/// Opens the pull request of the next version `next` after the release
/// `release` (01M7641T2JH2K6SYBCWQGM56FY). The cut wrote its one commit,
/// so it sets the status `riff/verify` of that commit before it opens the
/// pull request, and the Gate decides the merge
/// (01M76SBEV64SNKCS1AE1KXWNFM).
fn open_next(
    forge: &dyn Forge,
    release: Semver,
    next: Semver,
    issue: u64,
    wave: &str,
) -> Result<(u64, String)> {
    let body = next_body(issue, release, next, wave)?;
    let (branch, commit) = forge.push_branch(next)?;
    forge.pass_verify(&commit, NEXT_VERIFY)?;
    forge.open_pr(&branch, &next_title(next), wave, &body)
}

/// Why the cut refuses the last commit of `origin/main`, with the next
/// step, or `None` when it has each check (01M7641BHHND5JJA8DGJPFNAN7).
fn head_refusal(head: &Head, version: Semver) -> Option<String> {
    let state = match (head.built(), head.real_session) {
        (true, Some(TestState::Passed)) => return None,
        (true, _) => State::Built,
        (false, _) => State::Building,
    };
    let stage = Stage {
        head: Some(head.clone()),
        ..Stage::at(version, state)
    };
    let why = match state {
        State::Built => format!(
            "the last commit {} of origin/main has no pass of the real-session test \
             ({REAL_SESSION})",
            head.commit
        ),
        _ if !head.missing.is_empty() => format!(
            "the last commit {} of origin/main has no release build: no run of a merge group or of \
             a push to main gives {} to its tag, and the cut builds nothing",
            head.commit,
            head.missing.join(", ")
        ),
        _ => format!(
            "the release test of the last commit {} of origin/main did not pass ({RELEASE_TEST}: \
             {})",
            head.commit,
            head.release_test.map_or("none", TestState::word)
        ),
    };
    Some(format!(
        "riff release cut: {why}. riff changed nothing. next: {}",
        next(&stage)
    ))
}

/// Cuts the release `new` (01M7641BHHND5JJA8DGJPFNAN7): the tag on the
/// last commit of `origin/main`, the GitHub release, then the pull request
/// of the next version `next`, the next minor by default
/// (01M7641T2JH2K6SYBCWQGM56FY). The notes have the `words` of the lead
/// ([`notes`]). It makes no version commit and builds
/// nothing. It checks each fact first, and refuses before the release
/// issue while an alert of the repository has no review, and when it
/// cannot read the alerts ([`crate::alerts`], 01M6ZM68Y1QCTA2M8DHYRMKBPN).
/// A second cut of the same version does the steps that a cut that stopped
/// did not do (01M7641T51JY09YVVKR4WGEE72). It checks no role of the
/// account of the forge: the ruleset `releases` decides the push of the
/// tag. A limit of GitHub fails with the error of the limit.
pub fn cut(forge: &dyn Forge, new: Semver, next: Option<Semver>, words: &Words) -> Result<Cut> {
    let releases = forge.releases()?;
    let Some(last) = last(&releases) else {
        bail!(
            "riff release cut: the forge has no published release. next: a person makes the \
             first release by hand"
        );
    };
    let next = next.unwrap_or_else(|| next_minor(new));
    if next <= new {
        bail!(
            "riff release cut: the next version {next} is not newer than {new}. riff changed \
             nothing. next: riff release cut {new} --next {}",
            next_minor(new)
        );
    }
    let main = forge.main_version()?;
    if main != new {
        bail!(
            "riff release cut: the version on origin/main is {main}, not {new}. The cut tags the \
             last commit of origin/main, and makes no version commit. riff changed nothing. next: \
             riff release status"
        );
    }
    let tag = tag(new);
    if releases.contains(&new) {
        // The release is there: only the pull request of the next version
        // can be missing (01M7641T51JY09YVVKR4WGEE72).
        if let Some((number, version)) = forge.next_pr()? {
            bail!(
                "riff release cut: the release {tag} exists, and pull request #{number} sets the \
                 next version {version}. riff changed nothing. next: {}",
                wait_next(number)
            );
        }
        // The pull request refers to the release issue, so it stays open.
        let issue = match forge.issue_of(&title(new))? {
            Some(issue) if issue.open => issue.number,
            _ => bail!(
                "riff release cut: the release {tag} exists, it has no open release issue, and \
                 origin/main still has {new}. riff changed nothing. next: a pull request sets the \
                 next version in Cargo.toml and Cargo.lock"
            ),
        };
        let wave = forge.current_wave()?;
        let (number, url) = open_next(forge, new, next, issue, &wave)?;
        return Ok(Cut {
            issue,
            release: None,
            next,
            number,
            url,
        });
    }
    if new < last {
        bail!(
            "riff release cut: {new} is not newer than the last release {}. riff changed \
             nothing. next: riff release status",
            self::tag(last)
        );
    }
    // A tag with no release is the tag of a cut that stopped: the checks
    // passed before it.
    let commit = match forge.tag_exists(&tag)? {
        true => None,
        false => {
            if let Some(run) = forge.waiting()?.first() {
                bail!(
                    "riff release cut: the deploy of {} waits for the approval of {PRODUCTION}. \
                     A person runs riff release approve {} first. next: a person runs riff \
                     release approve {} in a terminal",
                    run.tag,
                    run.tag,
                    run.tag
                );
            }
            let head = head(forge)?;
            if let Some(refusal) = head_refusal(&head, new) {
                bail!("{refusal}");
            }
            let all = match forge.alerts() {
                Ok(all) => all,
                Err(e) if crate::ghcalls::is_limit_error(&e) => return Err(e),
                Err(e) => bail!("{}", alerts::unreadable(new, &format!("{e:#}"))),
            };
            let left = alerts::unreviewed(&all);
            if !left.is_empty() {
                bail!("{}", alerts::refusal(new, &left));
            }
            Some(head.commit)
        }
    };
    let wave = forge.current_wave()?;
    let issue = forge.release_issue(&title(new), &wave, &issue_body(new))?;
    let since = self::tag(last);
    let notes = notes(last, new, words, &forge.merged_since(Some(&since))?);
    // The body is checked before the tag, so a fault of hygiene stops the
    // cut before it changes the forge.
    next_body(issue, new, next, &wave)?;
    if let Some(commit) = &commit {
        forge.push_tag(&tag, commit)?;
    }
    let url = forge.make_release(&tag, &notes, Some(&since))?;
    let run = forge.tag_run(&tag)?;
    let release = Released { commit, url, run };
    let (number, url) = open_next(forge, new, next, issue, &wave).with_context(|| {
        format!(
            "riff release cut: riff made the release {tag}: {}, but not the pull request of the \
             next version {next}. next: riff release cut {new}",
            release.url
        )
    })?;
    Ok(Cut {
        issue,
        release: Some(release),
        next,
        number,
        url,
    })
}

/// The next step while the pull request `number` of the next version is
/// open: the cut passed its verify, so it merges by itself when its Gate
/// passes (01M76SBEV64SNKCS1AE1KXWNFM).
///
/// ```
/// assert_eq!(
///     riff::cycle::wait_next(1141),
///     "wait for the Gate of pull request #1141: it merges when the Gate passes; when a job \
///      failed, run that job again"
/// );
/// ```
pub fn wait_next(number: u64) -> String {
    format!(
        "wait for the Gate of pull request #{number}: it merges when the Gate passes; when a job \
         failed, run that job again"
    )
}

/// The ID of the CI run at the page `url` on GitHub, for the fact of
/// `riff release cut` ([`riff_core::wire::BotFact::ReleasePublished`]):
/// riff-server writes the page of the run itself.
///
/// ```
/// use riff::cycle::run_id;
///
/// assert_eq!(run_id("https://github.com/o/r/actions/runs/9"), Some(9));
/// assert_eq!(run_id("https://github.com/o/r/actions/runs/9/"), Some(9));
/// assert_eq!(run_id("the CI run of the tag"), None);
/// ```
pub fn run_id(url: &str) -> Option<u64> {
    let (_, id) = url.trim_end_matches('/').rsplit_once("/runs/")?;
    id.parse().ok()
}

/// What `riff release status` shows (01M58B0JEYNAW67NS226FMX5XH,
/// 01M76428NPN7HZCNFCJW8H67RS).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The version on `origin/main`.
    pub main: Semver,
    /// The last published release.
    pub last: Option<Semver>,
    /// The open pull request of the next version: its number and its
    /// version.
    pub next_pr: Option<(u64, Semver)>,
    /// Each CI run that waits for [`PRODUCTION`].
    pub waiting: Vec<Waiting>,
    /// The release that the cycle is at now ([`current`]).
    pub stage: Option<Stage>,
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
    Ok(Status {
        main,
        last: last(&releases),
        next_pr: forge.next_pr()?,
        waiting: forge.waiting()?,
        stage: current_in(forge, &releases)?,
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

/// The lines of `riff release status` (01M76428NPN7HZCNFCJW8H67RS). While
/// the version on `origin/main` has its release, and no pull request of
/// the next version is open, the line of the next version says that the
/// cut of that version opens it (01M7641T51JY09YVVKR4WGEE72). After the
/// check of that release, its release issue is closed: then a pull request
/// sets the next version.
///
/// ```
/// use riff::cycle::{Head, Stage, State, Status, TestState, render_status, version};
///
/// let head = Head { commit: "c1".into(), release_test: Some(TestState::Passed), ..Head::default() };
/// let stage = Stage { head: Some(head), ..Stage::at(version("3.3.0")?, State::Built) };
/// let status = Status {
///     main: version("3.3.0")?, last: Some(version("3.2.0")?), next_pr: None, waiting: vec![],
///     stage: Some(stage), holds: vec![],
/// };
/// assert_eq!(
///     render_status(&status),
///     "version on origin/main: 3.3.0\nlast release: v3.2.0\npull request of the next version: \
///      none\nwaits for production: none\nlast commit of origin/main: c1. release build: passed. \
///      release test: passed. real-session test: none.\nrelease: v3.3.0 built\nnext: riff release \
///      test v3.3.0 --host HOST, or just real-session on origin/main\n"
/// );
/// // The cut made the release, but not its pull request of the next version.
/// let published = Stage::at(version("3.2.0")?, State::Published);
/// let released = Status { main: version("3.2.0")?, stage: Some(published), ..status };
/// assert!(render_status(&released).contains(
///     "pull request of the next version: none: riff release cut 3.2.0 opens it\n"
/// ));
/// let checked = Status { stage: Some(Stage::at(version("3.2.0")?, State::Checked)), ..released };
/// assert!(render_status(&checked).contains(
///     "pull request of the next version: none, and origin/main still has 3.2.0: a pull request \
///      sets the next version\n"
/// ));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn render_status(status: &Status) -> String {
    let last = status.last.map_or("none".to_owned(), tag);
    let next = match status.next_pr {
        Some((number, version)) => format!("#{number} sets {version}"),
        None if status.last.is_some_and(|l| l >= status.main) => {
            match status.stage.as_ref().map(|s| s.state) {
                Some(State::Checked) | None => format!(
                    "none, and origin/main still has {}: a pull request sets the next version",
                    status.main
                ),
                Some(_) => format!("none: riff release cut {} opens it", status.main),
            }
        }
        None => "none".to_owned(),
    };
    let mut out = format!(
        "version on origin/main: {}\nlast release: {last}\npull request of the next version: \
         {next}\n",
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
    if let Some(head) = status.stage.as_ref().and_then(|s| s.head.as_ref()) {
        out.push_str(&head.line());
        out.push('\n');
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

/// One line of the log of the Cloud Run service of the shared riff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// Its time.
    pub at: DateTime<Utc>,
    /// The revision of the instance that wrote it.
    pub revision: String,
}

/// The log of the Cloud Run service of an instance of riff. [`RunLog`]
/// reads it with `gcloud logging read`. A test gives a fake.
pub trait CloudLog {
    /// Each line of the log that `filter` matches, the oldest first.
    fn lines(&self, filter: &str) -> Result<Vec<LogLine>>;
}

/// The facts of riff-server for [`check`]: the build that `riff server`
/// shows, the build of the daemon of each host, as the part `Hosts` of
/// `riff who` shows it, and the read of the keys of the GitHub Apps. An
/// error is the reason that riff cannot tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Served {
    /// The version of the build of the shared riff.
    pub server: Result<String, String>,
    /// `USER@HOST` and the build of its daemon, for each host.
    pub hosts: Result<Vec<(String, String)>, String>,
    /// Why riff-server cannot read the keys of the GitHub Apps, from
    /// `GET /v1/server` (01M6YC3T3B8JKT5BSQJTYMXBKY), or `None` when it reads them.
    pub keys: Result<Option<String>, String>,
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

/// The commit status of the checks of the Cloud Run log after a deploy
/// (01M76SS6T89DPNV7S4SWY1R7F3). The CI job `Check the deploy` sets it on
/// the tagged commit with `riff cloud check`: `success` when each check
/// of the log passed, `failure` when one failed, and `error` when it
/// could not read the log. Its description holds the [`LogMeasures`].
pub const DEPLOY_CHECK: &str = "riff/deploy-check";

/// The minutes that the job `Check the deploy` waits after the job
/// `Deploy` before it reads the log: Cloud Run moves the calls to the
/// new revision in this time (01M76SS6T89DPNV7S4SWY1R7F3).
pub const DEPLOY_CHECK_WAIT: i64 = 5;

/// The minutes from the end of the job `Deploy` to the end of the job
/// `Check the deploy`: its wait, and one minute for its other steps
/// (01M76SSFTGW41RHHV445ZM5RHY).
pub const DEPLOY_CHECK_END: i64 = DEPLOY_CHECK_WAIT + 1;

/// The four checks of the Cloud Run log, in the order of [`checks`].
pub const LOG_CHECKS: [&str; 4] = [
    "no run of 503 longer than 20 s",
    "no 500 of riff-server",
    "no 401 or 429 of the token store",
    "no old instance takes the lease",
];

/// What the four checks of the Cloud Run log after a deploy found
/// (01M76SS6T89DPNV7S4SWY1R7F3). The description of the status
/// [`DEPLOY_CHECK`] holds it, so `riff release check` makes the same
/// verdicts from it.
///
/// ```
/// use riff::cycle::{LogMeasures, Verdict};
///
/// let found = LogMeasures { longest_503: 25, ..LogMeasures::default() };
/// let text = found.description();
/// assert_eq!(text, "503 run: 25 s, 500: 0, token store: 0, old lease: no");
/// assert_eq!(LogMeasures::parse(&text), Some(found));
/// assert_eq!(found.verdicts()[0].1, Verdict::Failed("a run of 503 took 25 s".into()));
/// assert!(!found.passed());
/// assert!(LogMeasures::default().passed());
/// assert_eq!(LogMeasures::parse("503 run: x s"), None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LogMeasures {
    /// The seconds of the longest run of 503 ([`longest_run`]).
    pub longest_503: i64,
    /// The 500 replies that count ([`counted_500s`]).
    pub replies_500: usize,
    /// The lines of a 401 or a 429 of the token store.
    pub token_store: usize,
    /// True when an older revision served after the new one
    /// ([`old_took_lease`]).
    pub old_lease: bool,
}

impl LogMeasures {
    /// Reads the four checks from `log`, from the time `since`, as RFC
    /// 3339. It fails on the first read that fails.
    pub fn read(log: &dyn CloudLog, since: &str) -> Result<LogMeasures> {
        let read = |part: &str| log.lines(&format!("timestamp>=\"{since}\" AND {part}"));
        let serves = read("jsonPayload.message:\"riff-server serves\"")?;
        let newest = serves.last().map(|l| l.revision.as_str());
        let replies_500 =
            read("httpRequest.status=500 AND NOT textPayload:\"no available instance\"")?;
        let token_store = read(
            "jsonPayload.message:\"token store\" AND (jsonPayload.message:\"401\" OR \
             jsonPayload.message:\"429\")",
        )?;
        Ok(LogMeasures {
            longest_503: longest_run(&read("httpRequest.status=503")?),
            replies_500: counted_500s(&replies_500, newest),
            token_store: token_store.len(),
            old_lease: old_took_lease(&serves),
        })
    }

    /// The text of the description of the status [`DEPLOY_CHECK`].
    pub fn description(&self) -> String {
        format!(
            "503 run: {} s, 500: {}, token store: {}, old lease: {}",
            self.longest_503,
            self.replies_500,
            self.token_store,
            if self.old_lease { "yes" } else { "no" }
        )
    }

    /// The measures of the description `text` of the status
    /// [`DEPLOY_CHECK`], or `None` for a text of another form.
    pub fn parse(text: &str) -> Option<LogMeasures> {
        let rest = text.trim().strip_prefix("503 run: ")?;
        let (longest, rest) = rest.split_once(" s, 500: ")?;
        let (replies, rest) = rest.split_once(", token store: ")?;
        let (store, lease) = rest.split_once(", old lease: ")?;
        Some(LogMeasures {
            longest_503: longest.parse().ok()?,
            replies_500: replies.parse().ok()?,
            token_store: store.parse().ok()?,
            old_lease: match lease {
                "yes" => true,
                "no" => false,
                _ => return None,
            },
        })
    }

    /// Each of the four checks of the log ([`LOG_CHECKS`]) and its
    /// verdict.
    pub fn verdicts(&self) -> Vec<(&'static str, Verdict)> {
        let [runs, replies, store, lease] = LOG_CHECKS;
        let count = |n: usize, what: &str| match n {
            0 => Verdict::Passed,
            n => Verdict::Failed(format!("{n} {what}")),
        };
        vec![
            (
                runs,
                match self.longest_503 {
                    s if s > LONGEST_503 => Verdict::Failed(format!("a run of 503 took {s} s")),
                    _ => Verdict::Passed,
                },
            ),
            (replies, count(self.replies_500, "replies 500")),
            (store, count(self.token_store, "lines")),
            (
                lease,
                if self.old_lease {
                    Verdict::Failed("an older revision served after the new one".to_owned())
                } else {
                    Verdict::Passed
                },
            ),
        ]
    }

    /// True when each of the four checks passed.
    pub fn passed(&self) -> bool {
        self.verdicts().iter().all(|(_, v)| *v == Verdict::Passed)
    }
}

/// The four checks of the Cloud Run log of `riff cloud check`, from the
/// time `since` (01M76SSFZG959CQ1JHQTVTZCRA). A log that riff cannot
/// read gives the reason.
pub fn cloud_check(log: &dyn CloudLog, since: &str) -> Result<LogMeasures, String> {
    LogMeasures::read(log, since).map_err(|e| {
        if e.is::<NoGcloud>() {
            text::cloud_log_unread("gcloud is not on PATH")
        } else {
            text::cloud_log_unread(&format!("{e:#}"))
        }
    })
}

/// The time from which `riff cloud check` reads the log: `given`, an RFC
/// 3339 time, or 1 hour before `now` (01M76SSFZG959CQ1JHQTVTZCRA).
///
/// ```
/// let now = chrono::DateTime::parse_from_rfc3339("2026-10-18T20:30:00Z")?.to_utc();
/// assert_eq!(riff::cycle::since(None, now)?, "2026-10-18T19:30:00Z");
/// assert_eq!(riff::cycle::since(Some("2026-10-18T20:00:00Z"), now)?, "2026-10-18T20:00:00Z");
/// assert!(riff::cycle::since(Some("yesterday"), now).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn since(given: Option<&str>, now: DateTime<Utc>) -> Result<String> {
    let at = match given {
        Some(text) => DateTime::parse_from_rfc3339(text)
            .map_err(|_| anyhow!(text::cloud_check_bad_since(text)))?
            .to_utc(),
        None => now - chrono::Duration::hours(1),
    };
    Ok(at.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// The state and the description of the status [`DEPLOY_CHECK`] for what
/// the checks of the log found: the description of an error is cut to
/// the limit of GitHub.
///
/// ```
/// use riff::cycle::{LogMeasures, deploy_check_status};
///
/// let (state, text) = deploy_check_status(&Ok(LogMeasures::default()));
/// assert_eq!((state, text.as_str()), ("success", "503 run: 0 s, 500: 0, token store: 0, old lease: no"));
/// let failed = LogMeasures { replies_500: 2, ..LogMeasures::default() };
/// assert_eq!(deploy_check_status(&Ok(failed)).0, "failure");
/// let (state, text) = deploy_check_status(&Err("x".repeat(200)));
/// assert_eq!((state, text.len()), ("error", 130));
/// ```
pub fn deploy_check_status(found: &Result<LogMeasures, String>) -> (&'static str, String) {
    match found {
        Ok(m) if m.passed() => ("success", m.description()),
        Ok(m) => ("failure", m.description()),
        Err(why) => ("error", why.chars().take(130).collect()),
    }
}

/// Sets the status [`DEPLOY_CHECK`] of `commit` from what the checks of
/// the log found (01M76SSFZG959CQ1JHQTVTZCRA). It gives the state and the
/// description.
pub fn set_deploy_check(
    gh: &Gh,
    commit: &str,
    found: &Result<LogMeasures, String>,
) -> Result<(&'static str, String)> {
    let (state, description) = deploy_check_status(found);
    let body = serde_json::json!({
        "state": state,
        "context": DEPLOY_CHECK,
        "description": description,
    });
    gh.send(
        "POST",
        &format!("repos/{{owner}}/{{repo}}/statuses/{commit}"),
        &body,
    )?;
    Ok((state, description))
}

/// The four checks of the log for `riff release check` of `tag`, from
/// the status [`DEPLOY_CHECK`] of `check` (01M76SSFTGW41RHHV445ZM5RHY). A
/// status that is missing, or older than the start of the job `Deploy`
/// of `deploy`, gives the step: while the job `Check the deploy` runs,
/// when it ends ([`check_waits`]); after its end, the `riff cloud check`
/// of a person; for a run by hand, the same. Each other state than
/// `success` and `failure` gives its description.
///
/// ```
/// use riff::cycle::{CheckJob, CommitStatus, Deploy, DeployCheck, DeployState, LogMeasures, log_of};
///
/// let deploy = Deploy {
///     run: 9,
///     url: "https://github.com/o/r/actions/runs/9".into(),
///     state: DeployState::Passed,
///     at: "2026-10-18T20:00:00Z".into(),
///     ended: "2026-10-18T20:03:00Z".into(),
///     failed: vec![],
///     by_hand: false,
///     check_job: CheckJob::Runs,
/// };
/// let status = |state: &str, at: &str| DeployCheck {
///     commit: "c1".into(),
///     status: Some(CommitStatus {
///         state: state.into(),
///         description: "503 run: 0 s, 500: 1, token store: 0, old lease: no".into(),
///         at: at.into(),
///     }),
/// };
/// let found = log_of("v3.3.0", &deploy, &status("failure", "2026-10-18T20:09:10Z"));
/// assert_eq!(found, Ok(LogMeasures { replies_500: 1, ..LogMeasures::default() }));
/// // A status of an older deploy counts as none.
/// let old = log_of("v3.3.0", &deploy, &status("success", "2026-10-17T10:00:00Z"));
/// assert!(old.unwrap_err().contains("ends at about 20:09 UTC"));
/// let failed = Deploy { check_job: CheckJob::Ended("failure".into()), ..deploy.clone() };
/// let none = DeployCheck { commit: "c1".into(), status: None };
/// assert!(log_of("v3.3.0", &failed, &none).unwrap_err().contains("ended with failure"));
/// let by_hand = Deploy { by_hand: true, ..deploy.clone() };
/// let why = log_of("v3.3.0", &by_hand, &none).unwrap_err();
/// assert!(why.contains("riff cloud check shared --since 2026-10-18T20:00:00Z --commit c1"));
/// assert!(log_of("v3.3.0", &deploy, &status("error", "2026-10-18T20:09:10Z")).is_err());
/// ```
pub fn log_of(tag: &str, deploy: &Deploy, check: &DeployCheck) -> Result<LogMeasures, String> {
    if let Some(why) = check_none(deploy, check) {
        let how = text::check_by_hand(tag, &deploy.at, &check.commit);
        return Err(format!("{why}. {how}"));
    }
    let Some(status) = fresh(deploy, check) else {
        return Err(if deploy.by_hand {
            text::deploy_check_by_hand(tag, &deploy.at, &check.commit)
        } else {
            text::deploy_check_missing(tag, &deploy.url, &check_end(&deploy.ended))
        });
    };
    match status.state.as_str() {
        "success" | "failure" => LogMeasures::parse(&status.description)
            .ok_or_else(|| text::deploy_check_unread(tag, &status.description)),
        _ => Err(text::deploy_check_error(tag, &status.description)),
    }
}

/// The status [`DEPLOY_CHECK`] of `check` when it is one of `deploy`: not
/// older than the start of its job `Deploy`.
fn fresh<'a>(deploy: &Deploy, check: &'a DeployCheck) -> Option<&'a CommitStatus> {
    check.status.as_ref().filter(|s| !older(&s.at, &deploy.at))
}

/// Why the check of the log of `deploy` has no result after the end of
/// its job `Check the deploy`: the job ended, or the run ended with no
/// such job, and `check` has no status [`DEPLOY_CHECK`] of this deploy
/// (01M77BT0489T9R084WEFHPMCR5). `None` for a run by hand, while the job
/// runs, and with a status.
///
/// ```
/// use riff::cycle::{CheckJob, Deploy, DeployCheck, DeployState, check_none};
///
/// let deploy = Deploy {
///     run: 9,
///     url: "https://x/runs/9".into(),
///     state: DeployState::Passed,
///     at: String::new(),
///     ended: String::new(),
///     failed: vec![],
///     by_hand: false,
///     check_job: CheckJob::Ended("cancelled".into()),
/// };
/// let none = DeployCheck::default();
/// let why = check_none(&deploy, &none).unwrap();
/// assert_eq!(why, riff::text::deploy_check_none("https://x/runs/9", Some("cancelled")));
/// assert_eq!(check_none(&Deploy { check_job: CheckJob::Runs, ..deploy.clone() }, &none), None);
/// assert_eq!(check_none(&Deploy { by_hand: true, ..deploy.clone() }, &none), None);
/// ```
pub fn check_none(deploy: &Deploy, check: &DeployCheck) -> Option<String> {
    if deploy.by_hand || fresh(deploy, check).is_some() {
        return None;
    }
    match &deploy.check_job {
        CheckJob::Runs => None,
        CheckJob::Ended(conclusion) => Some(text::deploy_check_none(&deploy.url, Some(conclusion))),
        CheckJob::Missing => Some(text::deploy_check_none(&deploy.url, None)),
    }
}

/// True while the check of the lead after `deploy` waits for the job
/// `Check the deploy` (01M77BT0489T9R084WEFHPMCR5,
/// 01M77BT4XB4T8MQVCY7MK0B843): the job did not end, and `check` has no
/// status [`DEPLOY_CHECK`] of this deploy. A run by hand has no such job,
/// and waits for none.
///
/// ```
/// use riff::cycle::{CheckJob, CommitStatus, Deploy, DeployCheck, DeployState, check_waits};
///
/// let deploy = Deploy {
///     run: 9,
///     url: "https://github.com/o/r/actions/runs/9".into(),
///     state: DeployState::Passed,
///     at: "2026-10-18T20:00:00Z".into(),
///     ended: "2026-10-18T20:03:00Z".into(),
///     failed: vec![],
///     by_hand: false,
///     check_job: CheckJob::Runs,
/// };
/// let none = DeployCheck { commit: "c1".into(), status: None };
/// assert!(check_waits(&deploy, &none));
/// let set = |at: &str| DeployCheck {
///     commit: "c1".into(),
///     status: Some(CommitStatus { state: "error".into(), description: "x".into(), at: at.into() }),
/// };
/// assert!(!check_waits(&deploy, &set("2026-10-18T20:09:10Z")));
/// // A status of an older deploy counts as none.
/// assert!(check_waits(&deploy, &set("2026-10-17T10:00:00Z")));
/// for job in [CheckJob::Ended("failure".into()), CheckJob::Missing] {
///     assert!(!check_waits(&Deploy { check_job: job, ..deploy.clone() }, &none));
/// }
/// assert!(!check_waits(&Deploy { by_hand: true, ..deploy.clone() }, &none));
/// ```
pub fn check_waits(deploy: &Deploy, check: &DeployCheck) -> bool {
    !deploy.by_hand && deploy.check_job == CheckJob::Runs && fresh(deploy, check).is_none()
}

/// True when the RFC 3339 time `at` is before `start`. A time that riff
/// cannot read is not before.
fn older(at: &str, start: &str) -> bool {
    match (
        DateTime::parse_from_rfc3339(at),
        DateTime::parse_from_rfc3339(start),
    ) {
        (Ok(at), Ok(start)) => at < start,
        _ => false,
    }
}

/// When the job `Check the deploy` ends, after the job `Deploy` that
/// ended at `ended` ([`DEPLOY_CHECK_END`]): `HH:MM UTC`, or the minutes
/// after the end of the job `Deploy` when riff cannot read the time.
///
/// ```
/// assert_eq!(riff::cycle::check_end("2026-10-18T20:03:00Z"), "20:09 UTC");
/// assert_eq!(riff::cycle::check_end(""), "6 minutes after the end of the job Deploy");
/// ```
pub fn check_end(ended: &str) -> String {
    match DateTime::parse_from_rfc3339(ended) {
        Ok(at) => (at.to_utc() + chrono::Duration::minutes(DEPLOY_CHECK_END))
            .format("%H:%M UTC")
            .to_string(),
        Err(_) => format!("{DEPLOY_CHECK_END} minutes after the end of the job Deploy"),
    }
}

/// The checks after the deploy of `version` (01M58KMM5ZPAQ289S2YJVQ5WW6,
/// 01M58KMMP9FKRFYX58ESVRSZQX): each check by its name, and its verdict.
/// The four checks of the log come from `log`, the result of [`log_of`]:
/// its reason makes each of them [`Verdict::Unknown`]
/// (01M76SSFTGW41RHHV445ZM5RHY).
pub fn checks(
    version: Semver,
    served: &Served,
    log: &Result<LogMeasures, String>,
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
    out.push((
        "riff-server reads the keys of the GitHub Apps",
        match &served.keys {
            Ok(None) => Verdict::Passed,
            Ok(Some(why)) => Verdict::Failed(text::release_keys_fault(why)),
            Err(e) => Verdict::Unknown(format!("riff cannot ask the shared riff: {e}")),
        },
    ));
    out.extend(log_verdicts(log));
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
/// machine: [`cloud_check`] says that it is not on PATH.
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
    format!("{}\n\n{}", check_mark(tag, result), verdict_lines(lines))
}

/// One line `- NAME: VERDICT` for each check of `lines`.
///
/// ```
/// use riff::cycle::{Verdict, verdict_lines};
///
/// let lines = [("a", Verdict::Failed("2 lines".into())), ("b", Verdict::Passed)];
/// assert_eq!(verdict_lines(&lines), "- a: failed: 2 lines\n- b: passed\n");
/// ```
pub fn verdict_lines(lines: &[(&'static str, Verdict)]) -> String {
    let mut out = String::new();
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

/// The text of `riff cloud check` for what the checks of the log found
/// (01M76SSFZG959CQ1JHQTVTZCRA): each check and its verdict.
///
/// ```
/// use riff::cycle::{LogMeasures, cloud_check_text};
///
/// let text = cloud_check_text(&Ok(LogMeasures { token_store: 3, ..LogMeasures::default() }));
/// assert!(text.contains("- no 401 or 429 of the token store: failed: 3 lines\n"), "{text}");
/// let text = cloud_check_text(&Err("no log".into()));
/// assert!(text.starts_with("- no run of 503 longer than 20 s: riff could not check it: no log\n"), "{text}");
/// ```
pub fn cloud_check_text(found: &Result<LogMeasures, String>) -> String {
    verdict_lines(&log_verdicts(found))
}

/// The four checks of the log and their verdicts: the reason of an error
/// makes each of them [`Verdict::Unknown`].
fn log_verdicts(found: &Result<LogMeasures, String>) -> Vec<(&'static str, Verdict)> {
    match found {
        Ok(m) => m.verdicts(),
        Err(why) => LOG_CHECKS
            .map(|name| (name, Verdict::Unknown(why.clone())))
            .to_vec(),
    }
}

/// The checks after the deploy of `tag` (01M58KMM5ZPAQ289S2YJVQ5WW6). It
/// refuses while the deploy did not end, and while it waits for the job
/// `Check the deploy` ([`check_waits`], 01M77BT4XB4T8MQVCY7MK0B843): then
/// it makes no release issue, writes nothing, and its caller tells no
/// other lead. After a failed job `Deploy`, it files an issue with the
/// failed jobs and the run in the current wave. Else it runs [`checks`],
/// with the four checks of the log from the status [`DEPLOY_CHECK`] of
/// the tag ([`log_of`]), and writes the result on the release issue.
/// When each check passed, it closes the release issue. The caller
/// resumes the riff after a failed deploy, and after a pass. It runs no
/// `gcloud` (01M76SSFTGW41RHHV445ZM5RHY).
///
/// It gives the text of the result ([`check_text`]) to `show` before it
/// writes the comment (01M6H12QRZFBDTT5D5VQ4FB4C1). So a fault of the
/// comment does not hide the result: the error says that the checks ran,
/// and names `riff release check` as the next step.
pub fn check(
    forge: &dyn Forge,
    tag_text: &str,
    served: &Served,
    show: &mut dyn FnMut(&str),
) -> Result<Checked> {
    let version = tag_version(tag_text)?;
    let tag = tag(version);
    let Some(deploy) = forge.deploy(&tag)? else {
        bail!("riff release check: no CI run deploys {tag}. next: riff release status");
    };
    // The status of the job Check the deploy, read before any write: a
    // check before the end of the job changes nothing
    // (01M77BT4XB4T8MQVCY7MK0B843).
    let status = match deploy.state {
        DeployState::Passed => {
            let status = forge.deploy_check(&tag)?;
            if check_waits(&deploy, &status) {
                bail!(text::release_check_early(
                    &tag,
                    &deploy.url,
                    &check_end(&deploy.ended)
                ));
            }
            Some(status)
        }
        _ => None,
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
            let log = log_of(&tag, &deploy, &status.unwrap_or_default());
            let lines = checks(version, served, &log);
            let passed = lines.iter().all(|(_, v)| *v == Verdict::Passed);
            let result = check_text(&tag, &lines);
            show(&result);
            forge
                .comment(issue, &result)
                .with_context(|| text::release_check_unwritten(&tag, issue))?;
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
                deploy: Some(deploy),
                ..Stage::at(version, state)
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
        .filter(|m| !m.next && m.wave.as_deref() == Some(wave.as_str()))
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
                 release: {}. next: riff release test vX.Y.Z --host HOST, then riff release cut \
                 X.Y.Z. When no change of the wave is one that a person sees: riff wave end \
                 {number} --no-release TEXT",
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
    /// The user is a member, not an admin.
    Member,
    /// riff cannot ask riff-server for the role of the person.
    RoleUnknown,
    /// An agent session, for a step of a person: also the lead.
    Agent,
}

impl Caller {
    /// Why this caller may not run a step of the lead: `riff release
    /// cut`, `check`, `retry` and `rollback`, or `None`
    /// (01M58KMMF9WMH39ZMD0MKGRVZB, 01M7641T7FX9416CYY1XKHTJE6). A person in
    /// a plain terminal, or the lead of its user, may when the user is an
    /// admin of the riff, or when the riff has no sign-in.
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
    /// approve`, or `None` (01M58B0JSQMWNCMSNX19536XT8). Only a
    /// person in a plain terminal may: never an agent session, also not
    /// the lead of an admin, in a container or in a terminal
    /// (01M6WZ40SFVZ6CRQ1H4Z27CWP4). A member may not, when riff knows the
    /// role.
    ///
    /// ```
    /// use riff::cycle::{Caller, Refusal};
    /// use riff_core::wire::PersonRole;
    ///
    /// assert!(Caller::default().person_step().is_none());
    /// let lead = Caller { agent: true, lead: Some(true), ..Caller::default() };
    /// assert_eq!(lead.person_step(), Some(Refusal::Agent));
    /// let admin = Caller { role: Some(PersonRole::Admin), container: true, lead_container: true, ..lead };
    /// assert_eq!(admin.person_step(), Some(Refusal::Agent));
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

    /// The state of the commit status `context` of `commit`, or `None`.
    fn status_of(&self, commit: &str, context: &str) -> Result<Option<TestState>> {
        let path = format!("repos/{{owner}}/{{repo}}/commits/{commit}/status");
        let status: GhStatuses = self.gh.json(&["api", &path])?;
        Ok(status
            .statuses
            .iter()
            .find(|s| s.context == context)
            .and_then(|s| TestState::of(&s.state)))
    }

    /// The change of the branch of the next version `version`, in the
    /// worktree `tree`: the version in each file, checked with [`CHECK`].
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
        let title = next_title(version);
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
    body: String,
    #[serde(default)]
    milestone: Option<GhMilestone>,
    #[serde(default)]
    merge_commit: Option<GhCommit>,
    #[serde(default)]
    head_ref_oid: String,
    #[serde(default)]
    state: String,
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
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    updated_at: String,
}

#[derive(Deserialize)]
struct GhStatuses {
    #[serde(default)]
    sha: String,
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
    #[serde(default)]
    completed_at: String,
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
            "number,title,body,milestone,mergeCommit",
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
                next: next_of_title(&p.title).is_some(),
                title: text::forge(&p.title),
                wave: p.milestone.map(|m| text::forge(&m.title)),
                issue: pr::trailer_issue(&p.body),
                commit: p.merge_commit.map(|c| c.oid).unwrap_or_default(),
            })
            .collect())
    }

    fn push_branch(&self, version: Semver) -> Result<(String, String)> {
        let branch = format!("release-next-{}", tag(version));
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
            // A cut that stopped can leave the branch: the new commit
            // replaces it.
            let refspec = format!("+HEAD:refs/heads/{branch}");
            self.git_in(&path, &["push", "-q", "origin", &refspec])?;
            Ok(self
                .git_in(&path, &["rev-parse", "HEAD"])?
                .trim()
                .to_owned())
        });
        let _ = self.git(&["worktree", "remove", "--force", &path_text]);
        Ok((branch, done?))
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

    fn main_commit(&self) -> Result<String> {
        Ok(self.git(&["rev-parse", "origin/main"])?.trim().to_owned())
    }

    fn release_build(&self, commit: &str) -> Result<Vec<String>> {
        let tree = self.git(&["rev-parse", &format!("{commit}^{{tree}}")])?;
        let script = self
            .git(&["show", &format!("{commit}:{ARTIFACT_RUN}")])
            .with_context(|| {
                format!("the commit {commit} has no {ARTIFACT_RUN}, so riff cannot find its release build")
            })?;
        let file = tempfile::Builder::new()
            .prefix("riff-artifact-run-")
            .tempfile()
            .context("cannot make a file for the script of the artifacts")?;
        std::fs::write(file.path(), script).context("cannot write the script of the artifacts")?;
        let repo = crate::identity::place(&self.dir)?.repo_text();
        let mut missing = Vec::new();
        for name in RELEASE_BUILD {
            let artifact = format!("{name}-{}", tree.trim());
            // The script of the commit decides as for a tag of it: a push.
            let out = Command::new("bash")
                .arg(file.path())
                .arg(&artifact)
                .current_dir(&self.dir)
                .env("GH_REPO", &repo)
                .env("GITHUB_EVENT_NAME", "push")
                .env_remove("GITHUB_RUN_ID")
                .output()
                .with_context(|| format!("cannot run {ARTIFACT_RUN}"))?;
            if !out.status.success() {
                bail!(
                    "{ARTIFACT_RUN} {artifact} failed: {}",
                    text::forge(String::from_utf8_lossy(&out.stderr).trim())
                );
            }
            if String::from_utf8_lossy(&out.stdout).trim().is_empty() {
                missing.push(artifact);
            }
        }
        Ok(missing)
    }

    fn release_test(&self, commit: &str) -> Result<Option<TestState>> {
        self.status_of(commit, RELEASE_TEST)
    }

    fn next_pr(&self) -> Result<Option<(u64, Semver)>> {
        let list: Vec<GhPr> = self.gh.json(&[
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            "number,title",
        ])?;
        Ok(list
            .iter()
            .find_map(|p| Some((p.number, next_of_title(&p.title)?))))
    }

    fn pull(&self, number: u64) -> Result<Pull> {
        let n = number.to_string();
        let pr: GhPr = self
            .gh
            .json(&["pr", "view", &n, "--json", "number,state,headRefOid"])?;
        Ok(Pull {
            state: pr.state,
            head: pr.head_ref_oid,
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

    fn tag_exists(&self, tag: &str) -> Result<bool> {
        let name = format!("refs/tags/{tag}");
        Ok(!self
            .git(&["ls-remote", "--tags", "origin", &name])?
            .trim()
            .is_empty())
    }

    fn push_tag(&self, tag: &str, commit: &str) -> Result<()> {
        let message = format!("Release {tag}");
        self.git(&["tag", "-a", tag, commit, "-m", &message])?;
        let name = format!("refs/tags/{tag}");
        if let Err(e) = self.git(&["push", "-q", "origin", &name]) {
            let _ = self.git(&["tag", "-d", tag]);
            bail!("{} {e}", crate::text::tag_refused(tag));
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

    fn test_state(&self, commit: &str) -> Result<Option<TestState>> {
        self.status_of(commit, REAL_SESSION)
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
        let check_job = match jobs.jobs.iter().find(|j| j.name == CHECK_JOB) {
            Some(j) if j.status == "completed" => CheckJob::Ended(text::forge(&j.conclusion)),
            None if run.status == "completed" => CheckJob::Missing,
            _ => CheckJob::Runs,
        };
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
            ended: job
                .map(|j| text::forge(&j.completed_at))
                .unwrap_or_default(),
            failed,
            by_hand: tag_version(&run.head_branch).is_err(),
            check_job,
        }))
    }

    fn deploy_check(&self, tag: &str) -> Result<DeployCheck> {
        let path = format!("repos/{{owner}}/{{repo}}/commits/{tag}/status");
        let found: GhStatuses = self.gh.json(&["api", &path])?;
        let status = found
            .statuses
            .into_iter()
            .find(|s| s.context == DEPLOY_CHECK)
            .map(|s| CommitStatus {
                state: text::forge(&s.state),
                description: text::forge(&s.description.unwrap_or_default()),
                at: text::forge(&s.updated_at),
            });
        Ok(DeployCheck {
            commit: text::forge(&found.sha),
            status,
        })
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

    fn wave_release(&self, title: &str) -> Result<Option<String>> {
        // `gh issue list` gives the newest issue first.
        let found: Vec<GhOpenIssue> = self.gh.json(&[
            "issue",
            "list",
            "--state",
            "all",
            "--milestone",
            title,
            "--label",
            LABEL,
            "--limit",
            "1",
            "--json",
            "title",
        ])?;
        Ok(found.into_iter().next().map(|i| text::forge(&i.title)))
    }

    fn has_release(&self) -> bool {
        crate::release_config::of_folder(&self.dir).unwrap_or(false)
    }

    fn alerts(&self) -> Result<Vec<Alert>> {
        crate::alerts::read(&self.gh)
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
        /// The published releases whose release issue is closed: checked.
        pub(crate) checked: Vec<&'static str>,
        pub(crate) merged: Vec<Merged>,
        /// The last commit of origin/main.
        pub(crate) commit: &'static str,
        /// The artifacts of the release build that no run gives to a tag.
        pub(crate) missing: Vec<String>,
        /// The state of the release test of each commit.
        pub(crate) release_test: Option<TestState>,
        /// The open pull request of the next version.
        pub(crate) next_pr: Option<(u64, &'static str)>,
        /// The pull request that the watch reads.
        pub(crate) pull: Pull,
        /// The checks of the full check.
        pub(crate) checks: Vec<String>,
        pub(crate) gate: pr::Gate,
        pub(crate) tags: Vec<String>,
        pub(crate) waiting: Vec<Waiting>,
        /// The commits with a pass or a fail of the real-session test.
        pub(crate) real: Vec<(&'static str, bool)>,
        /// The commits whose real-session test runs.
        pub(crate) running: Vec<&'static str>,
        pub(crate) deploy: Option<Deploy>,
        pub(crate) issue: Option<ReleaseIssue>,
        /// The release tags that hold each commit.
        pub(crate) released: Vec<(&'static str, Vec<&'static str>)>,
        pub(crate) log: RefCell<Vec<String>>,
        /// The failed jobs of the full check.
        pub(crate) failed: Vec<String>,
        /// The current wave and its open items.
        pub(crate) wave_open: Option<(String, usize)>,
        /// The title of the newest release issue of the current wave.
        pub(crate) wave_release: Option<String>,
        /// True when the repository has the table `[release]`.
        pub(crate) has_release: bool,
        /// True when the comment on an issue fails.
        pub(crate) comment_fails: bool,
        /// True when the pull request of the next version fails.
        pub(crate) pr_fails: bool,
        /// The status riff/deploy-check of the tag.
        pub(crate) deploy_check: DeployCheck,
        /// The open and the dismissed alerts of the repository.
        pub(crate) alerts: Vec<Alert>,
        /// The error of the read of the alerts, when it fails.
        pub(crate) alerts_fail: Option<&'static str>,
    }

    impl Fake {
        /// The forge after the release 3.2.0, with 3.2.0 on origin/main.
        pub(crate) fn new() -> Self {
            Fake {
                main: "3.2.0".into(),
                checks: vec!["Gate".into()],
                releases: vec!["3.0.0", "3.2.0"],
                commit: "c1",
                release_test: Some(TestState::Passed),
                ..Fake::default()
            }
        }

        /// The forge whose last commit c1 of origin/main has the version
        /// `main`, its release build, and a pass of its release test and
        /// of its real-session test: the cut of `main` comes next.
        pub(crate) fn ready(main: &str) -> Self {
            Fake {
                main: main.into(),
                real: vec![("c1", true)],
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
        fn main_commit(&self) -> Result<String> {
            Ok(self.commit.to_owned())
        }
        fn release_build(&self, _commit: &str) -> Result<Vec<String>> {
            Ok(self.missing.clone())
        }
        fn release_test(&self, _commit: &str) -> Result<Option<TestState>> {
            Ok(self.release_test)
        }
        fn push_branch(&self, version: Semver) -> Result<(String, String)> {
            self.write(format!("branch {version}"));
            Ok((format!("release-next-v{version}"), "n1".into()))
        }
        fn open_pr(
            &self,
            branch: &str,
            title: &str,
            wave: &str,
            body: &str,
        ) -> Result<(u64, String)> {
            if self.pr_fails {
                bail!("gh pr create: HTTP 504");
            }
            self.write(format!("pr {branch} {title} {wave}\n{body}"));
            Ok((1141, "https://github.com/o/r/pull/1141".into()))
        }
        fn next_pr(&self) -> Result<Option<(u64, Semver)>> {
            self.next_pr.map(|(n, v)| Ok((n, version(v)?))).transpose()
        }
        fn pull(&self, _number: u64) -> Result<Pull> {
            Ok(self.pull.clone())
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
        fn tag_exists(&self, tag: &str) -> Result<bool> {
            Ok(self.tags.iter().any(|t| t == tag))
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
        fn deploy(&self, _tag: &str) -> Result<Option<Deploy>> {
            Ok(self.deploy.clone())
        }
        fn deploy_check(&self, _tag: &str) -> Result<DeployCheck> {
            Ok(self.deploy_check.clone())
        }
        fn issue_of(&self, title: &str) -> Result<Option<ReleaseIssue>> {
            if self
                .checked
                .iter()
                .any(|v| version(v).is_ok_and(|v| super::title(v) == title))
            {
                return Ok(Some(ReleaseIssue {
                    number: 6,
                    open: false,
                    comments: vec![],
                }));
            }
            Ok(self.issue.clone())
        }
        fn comment(&self, issue: u64, body: &str) -> Result<()> {
            if self.comment_fails {
                bail!("gh issue comment {issue} --body-file -: HTTP 504");
            }
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
        fn wave_release(&self, _title: &str) -> Result<Option<String>> {
            Ok(self.wave_release.clone())
        }
        fn has_release(&self) -> bool {
            self.has_release
        }
        fn alerts(&self) -> Result<Vec<Alert>> {
            match self.alerts_fail {
                Some(why) => Err(anyhow!("{why}")),
                None => Ok(self.alerts.clone()),
            }
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
            ended: "2026-10-18T20:03:00Z".into(),
            failed: if state == DeployState::Failed {
                vec!["Deploy".into()]
            } else {
                vec![]
            },
            by_hand: false,
            check_job: match state {
                DeployState::Passed => CheckJob::Ended("success".into()),
                _ => CheckJob::Runs,
            },
        }
    }

    /// 01M7641BHHND5JJA8DGJPFNAN7, 01M7641T2JH2K6SYBCWQGM56FY: the cut
    /// tags the last commit of origin/main, makes the release with the
    /// notes of the merged pull requests, and opens the pull request of the
    /// next minor. It makes no branch and no commit of its own version.
    /// 01M76SBEV64SNKCS1AE1KXWNFM: the cut passes the verify of the one
    /// commit of that pull request before it opens it, and the notes leave
    /// out a pull request of the next version by its title.
    #[test]
    fn cut_tags_the_last_commit_makes_the_release_and_opens_the_next_version() {
        let mut forge = Fake::ready("3.3.0");
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
                title: "Set the next version 3.3.0".into(),
                wave: Some("Wave 33".into()),
                issue: Some(6),
                next: true,
                ..Merged::default()
            },
        ];
        let words = Words {
            why: Some("New commands.".into()),
            run: vec!["riff update --tag v3.3.0".into()],
        };
        let cut = cut(&forge, v("3.3.0"), None, &words).unwrap();
        assert_eq!(
            cut,
            Cut {
                issue: 7,
                release: Some(Released {
                    commit: Some("c1".into()),
                    url: "https://github.com/o/r/releases/tag/v3.3.0".into(),
                    run: Some("https://github.com/o/r/actions/runs/9".into()),
                }),
                next: v("3.4.0"),
                number: 1141,
                url: "https://github.com/o/r/pull/1141".into(),
            }
        );
        let log = forge.log();
        assert_eq!(log.len(), 7, "{log:#?}");
        assert!(
            log[0].starts_with("issue Release v3.3.0 in Wave 34: Release\nThe release v3.3.0.\n"),
            "{}",
            log[0]
        );
        for part in [
            "Done when: the release is published",
            "Steps of the release:",
            "- The pull request of the next version merged.",
        ] {
            assert!(log[0].contains(part), "{part}: {}", log[0]);
        }
        assert!(!log[0].contains("verify"), "{}", log[0]);
        assert_eq!(log[1], "merged since Some(\"v3.2.0\")");
        assert_eq!(log[2], "tag v3.3.0 c1");
        let release = &log[3];
        assert!(
            release.starts_with(
                "release v3.3.0 since Some(\"v3.2.0\")\nLevel: minor, 3.3.0. New commands.\n\n\
                 Each person runs:\n\n- `riff update --tag v3.3.0`\n\n\
                 Waves in this release: Wave 34: Release.\n"
            ),
            "{release}"
        );
        assert!(
            release.contains("- #12: Show the line (#41).\n"),
            "{release}"
        );
        assert!(!release.contains("#40"), "{release}");
        assert_eq!(log[4], "branch 3.4.0");
        assert_eq!(log[5], format!("verify n1 {NEXT_VERIFY}"));
        let pr = &log[6];
        assert!(
            pr.starts_with(
                "pr release-next-v3.4.0 Set the next version 3.4.0 Wave 34: Release\nRefs #7\n"
            ),
            "{pr}"
        );
        assert!(
            pr.contains("Part of #7: the next version after v3.3.0."),
            "{pr}"
        );
        assert!(!pr.contains("Closes"), "{pr}");
        assert!(pr.ends_with("Issue: #7\nMilestone: Wave 34\n"), "{pr}");
        assert!(!log.iter().any(|l| l == "branch 3.3.0"), "{log:#?}");
    }

    /// 01M7641T2JH2K6SYBCWQGM56FY: `--next` names the next version. It must
    /// be newer than the release.
    #[test]
    fn cut_with_next_opens_the_pull_request_of_that_version() {
        let forge = Fake::ready("3.3.0");
        let done = cut(&forge, v("3.3.0"), Some(v("4.0.0")), &Words::default()).unwrap();
        assert_eq!(done.next, v("4.0.0"));
        assert!(forge.log().contains(&"branch 4.0.0".to_owned()));
        let forge = Fake::ready("3.3.0");
        let error = cut(&forge, v("3.3.0"), Some(v("3.3.0")), &Words::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(
                "the next version 3.3.0 is not newer than 3.3.0. riff changed nothing. next: riff \
                 release cut 3.3.0 --next 3.4.0"
            ),
            "{error}"
        );
        assert!(forge.log().is_empty());
    }

    /// 01M7641BHHND5JJA8DGJPFNAN7: the cut tags only the version of
    /// origin/main, and only one newer than the last release.
    #[test]
    fn cut_refuses_a_version_that_is_not_on_main_or_not_newer() {
        let forge = Fake::ready("3.3.0");
        let error = cut(&forge, v("3.4.0"), None, &Words::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(
                "the version on origin/main is 3.3.0, not 3.4.0. The cut tags the last commit of \
                 origin/main, and makes no version commit. riff changed nothing. next: riff \
                 release status"
            ),
            "{error}"
        );
        let forge = Fake::ready("3.1.0");
        let error = cut(&forge, v("3.1.0"), None, &Words::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("3.1.0 is not newer than the last release v3.2.0. riff changed nothing"),
            "{error}"
        );
        assert!(forge.log().is_empty());
    }

    /// 01M7641BHHND5JJA8DGJPFNAN7, 01M7641T03M977BBF4M8AQZ74M: with no
    /// release build, with a release test that did not pass, and with no
    /// pass of the real-session test, the cut refuses, builds nothing and
    /// writes nothing.
    #[test]
    fn cut_refuses_while_the_commit_has_no_release_build_or_a_failed_test() {
        let cases: Vec<(Fake, &str, &str)> = vec![
            (
                Fake {
                    missing: vec!["macos-riff-t1".into(), "mac-session-riff-t1".into()],
                    ..Fake::ready("3.3.0")
                },
                "the last commit c1 of origin/main has no release build: no run of a merge group \
                 or of a push to main gives macos-riff-t1, mac-session-riff-t1 to its tag, and \
                 the cut builds nothing",
                "next: wait for the release build of c1 in the CI run of its push to main; when \
                 a job of it failed, run that job again",
            ),
            (
                Fake {
                    release_test: Some(TestState::Failed),
                    ..Fake::ready("3.3.0")
                },
                "the release test of the last commit c1 of origin/main did not pass \
                 (riff/release-test: failed)",
                "next: the release test of c1 failed: file the bug in the wave; the release \
                 build of its fix runs after its merge to main",
            ),
            (
                Fake {
                    release_test: None,
                    ..Fake::ready("3.3.0")
                },
                "(riff/release-test: none)",
                "next: wait for the release test of c1 in the CI run of its push to main",
            ),
            (
                Fake {
                    real: vec![],
                    ..Fake::ready("3.3.0")
                },
                "the last commit c1 of origin/main has no pass of the real-session test \
                 (riff/real-session)",
                "next: riff release test v3.3.0 --host HOST, or just real-session on origin/main",
            ),
            (
                Fake {
                    real: vec![("c1", false)],
                    ..Fake::ready("3.3.0")
                },
                "has no pass of the real-session test",
                "next: file the bug in the wave, then riff release test v3.3.0 --host HOST after \
                 its fix merges",
            ),
        ];
        for (forge, why, step) in cases {
            let error = cut(&forge, v("3.3.0"), None, &Words::default())
                .unwrap_err()
                .to_string();
            assert!(error.contains(why), "{why}: {error}");
            assert!(error.ends_with(step), "{step}: {error}");
            assert!(error.contains("riff changed nothing"), "{error}");
            assert!(forge.log().is_empty(), "{why}: {:?}", forge.log());
        }
    }

    /// HUMAN GATE 1 with no answer: the next cut refuses
    /// (01M58KMM3NFGJRW4CCV50KJQB2).
    #[test]
    fn cut_refuses_while_a_deploy_waits_for_a_person() {
        let mut forge = Fake::ready("3.3.0");
        forge.waiting = vec![run("v3.2.0")];
        let error = cut(&forge, v("3.3.0"), None, &Words::default())
            .unwrap_err()
            .to_string();
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

    /// 01M7641T51JY09YVVKR4WGEE72: a second cut finishes a cut that stopped
    /// after the tag, or after the release. While the pull request of the
    /// next version is open, it refuses and names it.
    #[test]
    fn a_second_cut_finishes_a_cut_that_stopped() {
        // The tag is there with no release: no check of the commit.
        let forge = Fake {
            tags: vec!["v3.3.0".into()],
            missing: vec!["macos-riff-t1".into()],
            ..Fake::ready("3.3.0")
        };
        let cut1 = cut(&forge, v("3.3.0"), None, &Words::default()).unwrap();
        assert_eq!(cut1.release.as_ref().map(|r| r.commit.clone()), Some(None));
        let log = forge.log();
        assert!(!log.iter().any(|l| l.starts_with("tag ")), "{log:#?}");
        assert!(
            log.iter().any(|l| l.starts_with("release v3.3.0 ")),
            "{log:#?}"
        );
        assert!(
            log.iter().any(|l| l.starts_with("pr release-next-v3.4.0 ")),
            "{log:#?}"
        );
        // The release is there, and its release issue is open: only the
        // pull request of the next version.
        let forge = Fake {
            releases: vec!["3.0.0", "3.2.0", "3.3.0"],
            tags: vec!["v3.3.0".into()],
            issue: Some(ReleaseIssue {
                number: 7,
                open: true,
                comments: vec![],
            }),
            ..Fake::ready("3.3.0")
        };
        let cut2 = cut(&forge, v("3.3.0"), None, &Words::default()).unwrap();
        assert_eq!((cut2.release, cut2.issue, cut2.number), (None, 7, 1141));
        let log = forge.log();
        assert_eq!(log.len(), 3, "{log:#?}");
        assert_eq!(log[0], "branch 3.4.0");
        assert_eq!(log[1], format!("verify n1 {NEXT_VERIFY}"));
        assert!(log[2].starts_with("pr release-next-v3.4.0 "), "{log:#?}");
        // With no open release issue, the pull request has no issue: the cut
        // refuses.
        let closed = Fake {
            issue: None,
            ..forge.clone()
        };
        closed.log.borrow_mut().clear();
        let error = cut(&closed, v("3.3.0"), None, &Words::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("the release v3.3.0 exists, it has no open release issue"),
            "{error}"
        );
        assert!(closed.log().is_empty());
        // The pull request of the next version is open.
        let forge = Fake {
            next_pr: Some((1141, "3.4.0")),
            ..forge
        };
        forge.log.borrow_mut().clear();
        let error = cut(&forge, v("3.3.0"), None, &Words::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(
                "the release v3.3.0 exists, and pull request #1141 sets the next version 3.4.0. \
                 riff changed nothing. next: wait for the Gate of pull request #1141: it merges \
                 when the Gate passes; when a job failed, run that job again"
            ),
            "{error}"
        );
        assert!(forge.log().is_empty());
    }

    /// A pull request of the next version that GitHub refuses leaves the
    /// release: the error names it and the cut that finishes it.
    #[test]
    fn cut_names_the_release_when_the_pull_request_of_the_next_version_fails() {
        let forge = Fake {
            pr_fails: true,
            ..Fake::ready("3.3.0")
        };
        let error = format!(
            "{:#}",
            cut(&forge, v("3.3.0"), None, &Words::default()).unwrap_err()
        );
        assert!(
            error.starts_with(
                "riff release cut: riff made the release v3.3.0: \
                 https://github.com/o/r/releases/tag/v3.3.0, but not the pull request of the next \
                 version 3.4.0. next: riff release cut 3.3.0: gh pr create: HTTP 504"
            ),
            "{error}"
        );
    }

    fn alert(kind: alerts::Kind, number: u64, state: alerts::State) -> Alert {
        Alert {
            kind,
            number,
            rule: format!("rule-{number}"),
            path: Some("src/a.rs".into()),
            line: Some(number),
            url: format!("https://github.com/o/r/security/{number}"),
            state,
        }
    }

    fn dismissed(comment: &str) -> alerts::State {
        alerts::State::Dismissed {
            comment: comment.into(),
        }
    }

    /// 01M6ZM68Y1QCTA2M8DHYRMKBPN: an open alert of each kind, and a
    /// dismissed alert with no comment, stop the cut before the release
    /// issue. The refusal lists each one, and not the one with a reason.
    #[test]
    fn cut_refuses_while_an_alert_has_no_review_and_writes_nothing() {
        use alerts::{Kind, State};
        let mut forge = Fake::ready("3.3.0");
        forge.alerts = vec![
            alert(Kind::Security, 1, State::Open),
            alert(Kind::Quality, 2, State::Open),
            alert(Kind::Secret, 3, State::Open),
            alert(Kind::Dependabot, 4, State::Open),
            alert(Kind::Quality, 5, dismissed("A test fixture.")),
            alert(Kind::Secret, 6, dismissed("  ")),
        ];
        let error = cut(&forge, v("3.3.0"), None, &Words::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("riff release cut: 5 alerts of the repository have no review."),
            "{error}"
        );
        for line in [
            "\n- code scanning (security) #1 rule-1, src/a.rs:1, open: https://github.com/o/r/security/1\n",
            "\n- code scanning (quality) #2 rule-2, src/a.rs:2, open: ",
            "\n- secret scanning #3 rule-3, src/a.rs:3, open: ",
            "\n- Dependabot #4 rule-4, src/a.rs:4, open: ",
            "\n- secret scanning #6 rule-6, src/a.rs:6, dismissed with no reason: ",
            "\nnext: fix each alert in the code, or dismiss it on GitHub with the reason in the \
             comment, then riff release cut 3.3.0",
        ] {
            assert!(error.contains(line), "{line} in {error}");
        }
        assert!(!error.contains("#5"), "{error}");
        assert!(forge.log().is_empty(), "{:?}", forge.log());
    }

    /// 01M6ZM68Y1QCTA2M8DHYRMKBPN: dismissed alerts with a reason let the
    /// cut go on.
    #[test]
    fn cut_goes_on_when_each_alert_is_dismissed_with_a_reason() {
        use alerts::Kind;
        let mut forge = Fake::ready("3.3.0");
        forge.alerts = vec![
            alert(Kind::Security, 1, dismissed("The name misleads the scan.")),
            alert(Kind::Dependabot, 2, dismissed("Only the tests use it.")),
        ];
        let cut = cut(&forge, v("3.3.0"), None, &Words::default()).unwrap();
        assert_eq!(cut.number, 1141);
    }

    /// 01M748V0HMV7D9T4F6NCB7HQHG: a revoked secret and a Dependabot
    /// alert that a rule dismissed need no comment. A code scanning alert
    /// dismissed with no comment still stops the cut.
    #[test]
    fn cut_goes_on_with_a_closed_alert_and_refuses_a_dismiss_with_no_comment() {
        use alerts::{Kind, State};
        let mut forge = Fake::ready("3.3.0");
        forge.alerts = vec![
            alert(Kind::Secret, 1, State::Closed),
            alert(Kind::Dependabot, 2, State::Closed),
            alert(Kind::Security, 3, dismissed("")),
        ];
        let error = cut(&forge, v("3.3.0"), None, &Words::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("riff release cut: 1 alert of the repository has no review."),
            "{error}"
        );
        assert!(
            error.contains(
                "\n- code scanning (security) #3 rule-3, src/a.rs:3, dismissed with no reason: "
            ),
            "{error}"
        );
        assert!(forge.log().is_empty(), "{:?}", forge.log());
        forge.alerts.pop();
        let cut = cut(&forge, v("3.3.0"), None, &Words::default()).unwrap();
        assert_eq!(cut.number, 1141);
    }

    /// 01M6ZM68Y1QCTA2M8DHYRMKBPN: a read of the alerts that fails stops
    /// the cut with the read and the fix. A limit of GitHub stays the
    /// error of the limit.
    #[test]
    fn cut_refuses_when_it_cannot_read_the_alerts() {
        let mut forge = Fake::ready("3.3.0");
        forge.alerts_fail = Some(
            "the read of the code scanning alerts failed: gh: Resource not accessible by \
             integration (HTTP 403)",
        );
        let error = cut(&forge, v("3.3.0"), None, &Words::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with(
                "riff release cut: the read of the code scanning alerts failed: gh: Resource not \
                 accessible by integration (HTTP 403). riff cuts no release while it cannot check"
            ),
            "{error}"
        );
        assert!(
            error.ends_with("next: fix it, then riff release cut 3.3.0"),
            "{error}"
        );
        assert!(forge.log().is_empty());
        let limit = "gh: API rate limit exceeded for user ID 1. (HTTP 403)";
        forge.alerts_fail = Some(limit);
        let error = cut(&forge, v("3.3.0"), None, &Words::default())
            .unwrap_err()
            .to_string();
        assert_eq!(error, limit);
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
    fn status_shows_the_last_commit_the_next_version_a_waiting_run_and_the_next_step() {
        let mut forge = Fake::ready("3.3.0");
        forge.waiting = vec![run("v3.2.0")];
        forge.next_pr = None;
        let text = render_status(&status(&forge).unwrap());
        assert_eq!(
            text,
            "version on origin/main: 3.3.0\nlast release: v3.2.0\n\
             pull request of the next version: none\n\
             waits for production: v3.2.0 https://github.com/o/r/actions/runs/9. A person runs: \
             riff release approve v3.2.0\n\
             last commit of origin/main: c1. release build: passed. release test: passed. \
             real-session test: passed.\n\
             release: v3.3.0 tested\nnext: riff release cut 3.3.0\n"
        );
        // The cut is done, and its pull request of the next version is open.
        let cut = Fake {
            releases: vec!["3.0.0", "3.2.0", "3.3.0"],
            tags: vec!["v3.3.0".into()],
            next_pr: Some((1141, "3.4.0")),
            ..Fake::ready("3.3.0")
        };
        let text = render_status(&status(&cut).unwrap());
        assert!(
            text.contains("pull request of the next version: #1141 sets 3.4.0\n"),
            "{text}"
        );
        assert!(
            text.ends_with(
                "release: v3.3.0 published\nnext: wait for the CI run of the tag v3.3.0\n"
            ),
            "{text}"
        );
        assert!(!text.contains("last commit of origin/main"), "{text}");
    }

    /// 01M76428TRD7KWC4C8V5NNPNC7: the test runs on the last commit of
    /// origin/main, when it has its release build and a passed release
    /// test, and no pass or test that runs.
    #[test]
    fn test_target_is_the_last_commit_of_main_with_its_release_build() {
        let built = Fake {
            real: vec![],
            ..Fake::ready("3.3.0")
        };
        assert_eq!(test_target(&built, "v3.3.0").unwrap(), "c1");
        // A failed test on the last commit can run again.
        let failed = Fake {
            real: vec![("c1", false)],
            ..built.clone()
        };
        assert_eq!(test_target(&failed, "v3.3.0").unwrap(), "c1");
        let refused: Vec<(Fake, &str, &str)> = vec![
            (
                Fake {
                    running: vec!["c1"],
                    ..built.clone()
                },
                "v3.3.0",
                "the test runs on c1 already. riff changed nothing",
            ),
            (
                Fake::ready("3.3.0"),
                "v3.3.0",
                "the real-session test passed on the last commit of origin/main already. riff \
                 changed nothing. next: riff release cut 3.3.0",
            ),
            (
                Fake {
                    missing: vec!["static-release-t1".into()],
                    ..built.clone()
                },
                "v3.3.0",
                "has no release build and no passed release test yet. riff changed nothing. \
                 next: wait for the release build of c1",
            ),
            (
                Fake {
                    tags: vec!["v3.3.0".into()],
                    ..built.clone()
                },
                "v3.3.0",
                "the release v3.3.0 has its tag. The real-session test runs before the tag",
            ),
            (
                built.clone(),
                "v3.4.0",
                "3.4.0 is not the version on origin/main with no release",
            ),
        ];
        for (forge, tag, want) in refused {
            let error = test_target(&forge, tag).unwrap_err().to_string();
            assert!(error.contains(want), "{want}: {error}");
        }
        assert!(test_target(&built, "3.3.0").is_err());
        assert!(built.log().is_empty());
    }

    /// `riff release test` makes the release issue in the current wave
    /// (01M7641T9XH5NAXHE8YKBR4JJY).
    #[test]
    fn open_issue_makes_the_release_issue_in_the_current_wave() {
        let forge = Fake::ready("3.3.0");
        assert_eq!(open_issue(&forge, v("3.3.0")).unwrap(), 7);
        let log = forge.log();
        assert!(
            log[0].starts_with("issue Release v3.3.0 in Wave 34: Release\nThe release v3.3.0.\n"),
            "{log:?}"
        );
    }

    /// Each state of the cycle, with its next step
    /// (01M76428NPN7HZCNFCJW8H67RS, 01M58KMKWRKSV1DR42FRH2K43X).
    #[test]
    fn stage_finds_each_state_of_the_cycle() {
        let issue = |open, comments: &[&str]| ReleaseIssue {
            number: 7,
            open,
            comments: comments.iter().map(|c| (*c).to_owned()).collect(),
        };
        let tagged = |d: Option<Deploy>| Fake {
            releases: vec!["3.0.0", "3.2.0", "3.3.0"],
            tags: vec!["v3.3.0".into()],
            deploy: d,
            ..Fake::ready("3.3.0")
        };
        let failed_deploy = "riff release check v3.3.0: failed deploy\n\nThe CI run x failed.";
        let unknown = check_text("v3.3.0", &[("a", Verdict::Unknown("no log".into()))]);
        let forward = "fix forward: after the fix merges, riff release test v3.4.0 --host HOST \
                       and riff release cut 3.4.0";
        let cases: Vec<(Fake, State, String)> = vec![
            (
                Fake {
                    missing: vec!["static-release-t1".into()],
                    ..Fake::ready("3.3.0")
                },
                State::Building,
                "wait for the release build of c1 in the CI run of its push to main; when a job \
                 of it failed, run that job again"
                    .into(),
            ),
            (
                Fake {
                    release_test: Some(TestState::Running),
                    ..Fake::ready("3.3.0")
                },
                State::Building,
                "wait for the release test of c1 in the CI run of its push to main".into(),
            ),
            (
                Fake {
                    real: vec![],
                    ..Fake::ready("3.3.0")
                },
                State::Built,
                "riff release test v3.3.0 --host HOST, or just real-session on origin/main".into(),
            ),
            (
                Fake {
                    real: vec![],
                    running: vec!["c1"],
                    ..Fake::ready("3.3.0")
                },
                State::Built,
                "wait for the real-session test on c1; riff release status shows it".into(),
            ),
            (
                Fake {
                    real: vec![("c1", false)],
                    ..Fake::ready("3.3.0")
                },
                State::Built,
                "file the bug in the wave, then riff release test v3.3.0 --host HOST after its \
                 fix merges"
                    .into(),
            ),
            (
                Fake::ready("3.3.0"),
                State::Tested,
                "riff release cut 3.3.0".into(),
            ),
            (
                tagged(None),
                State::Published,
                "wait for the CI run of the tag v3.3.0".into(),
            ),
            (
                tagged(Some(deploy(DeployState::Before))),
                State::Published,
                "wait for the jobs of the CI run https://github.com/o/r/actions/runs/9".into(),
            ),
            (
                tagged(Some(deploy(DeployState::Waiting))),
                State::Waiting,
                "a person runs riff release approve v3.3.0 in a terminal".into(),
            ),
            (
                tagged(Some(deploy(DeployState::Running))),
                State::Deploying,
                "wait for the deploy: https://github.com/o/r/actions/runs/9".into(),
            ),
            (
                tagged(Some(deploy(DeployState::Passed))),
                State::Deployed,
                "riff release check v3.3.0".into(),
            ),
            (
                tagged(Some(deploy(DeployState::Failed))),
                State::Failed,
                "riff release retry v3.3.0, or riff release rollback v3.2.0".into(),
            ),
            // After a failed deploy and riff release check, the CI run
            // decides: before, while and after riff release retry.
            (
                Fake {
                    issue: Some(issue(true, &[failed_deploy])),
                    ..tagged(Some(deploy(DeployState::Failed)))
                },
                State::Failed,
                "riff release retry v3.3.0, or riff release rollback v3.2.0".into(),
            ),
            (
                Fake {
                    issue: Some(issue(true, &[failed_deploy])),
                    ..tagged(Some(deploy(DeployState::Waiting)))
                },
                State::Waiting,
                "a person runs riff release approve v3.3.0 in a terminal".into(),
            ),
            (
                Fake {
                    issue: Some(issue(true, &[failed_deploy])),
                    ..tagged(Some(deploy(DeployState::Running)))
                },
                State::Deploying,
                "wait for the deploy: https://github.com/o/r/actions/runs/9".into(),
            ),
            (
                Fake {
                    issue: Some(issue(true, &[failed_deploy])),
                    ..tagged(Some(deploy(DeployState::Passed)))
                },
                State::Deployed,
                "riff release check v3.3.0".into(),
            ),
            // A check that riff could not do changes no state.
            (
                Fake {
                    issue: Some(issue(true, &[unknown.as_str()])),
                    ..tagged(Some(deploy(DeployState::Passed)))
                },
                State::Deployed,
                "riff release check v3.3.0".into(),
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
                format!("{forward}; or roll back with riff release rollback v3.2.0"),
            ),
            (
                Fake {
                    issue: Some(issue(true, &["riff release check v3.3.0: failed\n\n- x"])),
                    ..tagged(Some(deploy(DeployState::Passed)))
                },
                State::Failed,
                format!("{forward}; or roll back with riff release rollback v3.2.0"),
            ),
            (
                Fake {
                    issue: Some(issue(true, &["riff release check v3.3.0: passed"])),
                    ..tagged(Some(deploy(DeployState::Passed)))
                },
                State::Checked,
                "riff wave end N, for the wave of the release".into(),
            ),
            (
                Fake {
                    issue: Some(issue(false, &[])),
                    ..tagged(None)
                },
                State::Checked,
                "riff wave end N, for the wave of the release".into(),
            ),
            (
                Fake {
                    issue: Some(issue(true, &["riff release rollback v3.2.0\n\nx"])),
                    ..tagged(Some(deploy(DeployState::Passed)))
                },
                State::RolledBack,
                forward.into(),
            ),
        ];
        for (forge, state, step) in cases {
            let stage = current(&forge).unwrap().unwrap();
            assert_eq!(stage.state, state, "{step}");
            assert_eq!(next(&stage), step, "{state:?}");
        }
        assert_eq!(stage(&Fake::new(), v("3.4.0")).unwrap(), None);
    }

    /// 01M76428NPN7HZCNFCJW8H67RS: the current release is the newest
    /// published release until its check passes, also after the pull
    /// request of the next version merged. Then it is the version on
    /// origin/main.
    #[test]
    fn the_current_release_is_the_newest_until_its_check_passes() {
        let deploying = Fake {
            main: "3.4.0".into(),
            releases: vec!["3.0.0", "3.2.0", "3.3.0"],
            tags: vec!["v3.3.0".into()],
            deploy: Some(deploy(DeployState::Waiting)),
            ..Fake::ready("3.4.0")
        };
        let stage = current(&deploying).unwrap().unwrap();
        assert_eq!((stage.version, stage.state), (v("3.3.0"), State::Waiting));
        let checked = Fake {
            checked: vec!["3.0.0", "3.2.0", "3.3.0"],
            ..deploying
        };
        let stage = current(&checked).unwrap().unwrap();
        assert_eq!((stage.version, stage.state), (v("3.4.0"), State::Tested));
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
                "next: an admin of the riff runs riff release cut",
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
            previous: Some(version("3.2.0").unwrap()),
            ..Stage::at(version("3.3.0").unwrap(), State::Waiting)
        };
        let mut status = Status {
            main: version("3.3.0").unwrap(),
            last: Some(version("3.2.0").unwrap()),
            next_pr: None,
            waiting: vec![run("v3.2.0"), run("v3.3.0")],
            stage: Some(stage),
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
            keys: Ok(None),
        }
    }

    /// The status riff/deploy-check of the commit c1 of the tag: `state`
    /// with the measures `found`, set after the deploy of [`deploy`].
    fn deploy_status(state: &str, found: LogMeasures) -> DeployCheck {
        DeployCheck {
            commit: "c1".into(),
            status: Some(CommitStatus {
                state: state.into(),
                description: found.description(),
                at: "2026-10-18T20:09:10Z".into(),
            }),
        }
    }

    /// The forge after a passed job Deploy of v3.3.0, with the open
    /// release issue #7 and the status `check` of the tag.
    fn deployed(check: DeployCheck) -> Fake {
        Fake {
            tags: vec!["v3.3.0".into()],
            deploy: Some(deploy(DeployState::Passed)),
            issue: Some(ReleaseIssue {
                number: 7,
                open: true,
                comments: vec![],
            }),
            deploy_check: check,
            ..Fake::ready("3.3.0")
        }
    }

    #[test]
    fn check_passes_writes_the_result_and_closes_the_release_issue() {
        let forge = deployed(deploy_status("success", LogMeasures::default()));
        let done = check(&forge, "v3.3.0", &served("3.3.0"), &mut |_| {}).unwrap();
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

    /// 01M6H12QRZFBDTT5D5VQ4FB4C1: the result goes to `show` before the
    /// comment. A comment that fails hides nothing, closes nothing and
    /// names the next step.
    #[test]
    fn check_shows_the_result_before_it_writes_the_comment() {
        let ready = |comment_fails| Fake {
            comment_fails,
            ..deployed(deploy_status("success", LogMeasures::default()))
        };
        let forge = ready(false);
        let mut shown = Vec::new();
        check(&forge, "v3.3.0", &served("3.3.0"), &mut |text| {
            forge.write("show".into());
            shown.push(text.to_owned());
        })
        .unwrap();
        let log_of = forge.log();
        assert_eq!(log_of[0], "show", "{log_of:?}");
        assert!(log_of[1].starts_with("comment #7\n"), "{log_of:?}");
        assert_eq!(shown.len(), 1);
        assert!(log_of[1].ends_with(&shown[0]), "{log_of:?}");

        let forge = ready(true);
        let mut shown = Vec::new();
        let error = check(&forge, "v3.3.0", &served("3.3.0"), &mut |text| {
            shown.push(text.to_owned())
        })
        .unwrap_err();
        assert_eq!(shown.len(), 1, "the result was shown before the fault");
        assert!(
            shown[0].starts_with("riff release check v3.3.0: passed\n"),
            "{shown:?}"
        );
        assert_eq!(
            error.to_string(),
            text::release_check_unwritten("v3.3.0", 7)
        );
        assert!(format!("{error:#}").contains("HTTP 504"), "{error:#}");
        assert!(forge.log().is_empty(), "{:?}", forge.log());
    }

    /// 01M76SSFTGW41RHHV445ZM5RHY: the four checks of the log come from
    /// the status riff/deploy-check, with the same verdicts as the job
    /// Check the deploy.
    #[test]
    fn check_fails_on_each_bad_fact_and_keeps_the_issue_open() {
        let none = LogMeasures::default();
        let cases: Vec<(Served, LogMeasures, &str)> = vec![
            (
                served("3.2.0"),
                none,
                "riff server shows the build: failed: riff server shows 3.2.0, not 3.3.0",
            ),
            (
                served("3.3.0"),
                LogMeasures {
                    longest_503: 25,
                    ..none
                },
                "no run of 503 longer than 20 s: failed: a run of 503 took 25 s",
            ),
            (
                served("3.3.0"),
                LogMeasures {
                    replies_500: 1,
                    ..none
                },
                "no 500 of riff-server: failed: 1 replies 500",
            ),
            (
                served("3.3.0"),
                LogMeasures {
                    token_store: 1,
                    ..none
                },
                "no 401 or 429 of the token store: failed: 1 lines",
            ),
            (
                served("3.3.0"),
                LogMeasures {
                    old_lease: true,
                    ..none
                },
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
                none,
                "each host takes the build: failed: bob@pangolin has 3.2.0 929605821e54",
            ),
            // 01M6YC3T3B8JKT5BSQJTYMXBKY: a riff-server that cannot read the secret of the
            // keys fails the check, with the fix.
            (
                Served {
                    keys: Ok(Some(
                        "projects/como-riff/secrets/riff-github-apps: 403 Forbidden".into(),
                    )),
                    ..served("3.3.0")
                },
                none,
                "riff-server reads the keys of the GitHub Apps: failed: riff-server cannot read the \
                 secret of the keys of the GitHub Apps: projects/como-riff/secrets/riff-github-apps: \
                 403 Forbidden. The riff is paused. An admin runs git pull, riff cloud create shared",
            ),
        ];
        for (served, found, want) in cases {
            let state = if found.passed() { "success" } else { "failure" };
            let forge = deployed(deploy_status(state, found));
            let done = check(&forge, "v3.3.0", &served, &mut |_| {}).unwrap();
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

    /// 01M76SS6T89DPNV7S4SWY1R7F3: the read of the job Check the deploy
    /// measures each check of the log from the time of the deploy.
    #[test]
    fn the_read_measures_each_check_of_the_log() {
        let run_503: Vec<LogLine> = (0..6).map(|i| line(i * 5, "new")).collect();
        let log = Log(Ok(vec![
            ("503", run_503),
            ("token store", vec![line(0, "new")]),
            (
                "riff-server serves",
                vec![line(0, "new"), line(5, "old"), line(9, "new")],
            ),
        ]));
        let found = LogMeasures::read(&log, "2026-10-18T20:00:00Z").unwrap();
        assert_eq!(
            found,
            LogMeasures {
                longest_503: 25,
                replies_500: 0,
                token_store: 1,
                old_lease: true,
            }
        );
        assert_eq!(
            deploy_check_status(&Ok(found)),
            (
                "failure",
                "503 run: 25 s, 500: 0, token store: 1, old lease: yes".to_owned()
            )
        );
        assert_eq!(
            LogMeasures::read(&Log(Ok(vec![])), "t").unwrap(),
            LogMeasures::default()
        );
    }

    /// 01M5MQHDMBYG187T0Q622XWBMN: a 500 of an instance of the old
    /// revision that stops in the switch does not count. A 500 of the
    /// newest revision does.
    #[test]
    fn the_read_counts_only_the_500s_of_the_newest_revision() {
        let serves = vec![line(0, "old"), line(5, "new")];
        for (on, count) in [("old", 0), ("new", 1)] {
            let log = Log(Ok(vec![
                ("riff-server serves", serves.clone()),
                ("status=500", vec![line(40, on)]),
            ]));
            let found = LogMeasures::read(&log, "t").unwrap();
            assert_eq!(found.replies_500, count, "a 500 of {on}");
        }
    }

    /// 01M76SSFZG959CQ1JHQTVTZCRA: a log that riff cannot read gives the
    /// reason, also for a machine with no gcloud.
    #[test]
    fn the_check_of_a_log_that_riff_cannot_read_gives_the_reason() {
        let found = cloud_check(&Log(Err("gcloud: PERMISSION_DENIED")), "t");
        assert_eq!(
            found,
            Err("riff could not read the Cloud Run log: gcloud: PERMISSION_DENIED".into())
        );
        assert_eq!(deploy_check_status(&found).0, "error");
        let log = RunLog {
            gcloud: crate::cloud::Gcloud::at("/nonexistent/riff-test/gcloud"),
            settings: crate::cloud::Settings::new("shared", "acme", "123", "europe-west1"),
        };
        assert_eq!(
            cloud_check(&log, "t"),
            Err("riff could not read the Cloud Run log: gcloud is not on PATH".into())
        );
    }

    /// 01M77BT4XB4T8MQVCY7MK0B843: before the job Check the deploy sets the
    /// status, the check refuses, names when the job ends, and changes
    /// nothing: no release issue and no comment. Its caller tells the other
    /// leads nothing.
    #[test]
    fn check_refuses_before_the_job_check_the_deploy_ends() {
        let mut forge = Fake {
            issue: None,
            ..deployed(DeployCheck {
                commit: "c1".into(),
                status: None,
            })
        };
        forge.deploy.as_mut().unwrap().check_job = CheckJob::Runs;
        let error = check(&forge, "v3.3.0", &served("3.3.0"), &mut |_| {}).unwrap_err();
        assert_eq!(
            error.to_string(),
            text::release_check_early(
                "v3.3.0",
                "https://github.com/o/r/actions/runs/9",
                "20:09 UTC"
            )
        );
        assert!(forge.log().is_empty(), "{:?}", forge.log());

        // A status of this deploy counts before the end of the job.
        forge.deploy_check = deploy_status("success", LogMeasures::default());
        let done = check(&forge, "v3.3.0", &served("3.3.0"), &mut |_| {}).unwrap();
        assert!(
            matches!(done, Checked::Ran { passed: true, .. }),
            "{done:?}"
        );
    }

    /// 01M76SSFTGW41RHHV445ZM5RHY: a job Check the deploy that ended with no
    /// status makes each check of the log unknown, and names the check of
    /// a person. The checks of riff-server still run, and riff closes
    /// nothing.
    #[test]
    fn check_names_the_check_of_a_person_after_a_job_that_set_no_status() {
        let mut forge = Fake {
            issue: None,
            ..deployed(DeployCheck {
                commit: "c1".into(),
                status: None,
            })
        };
        forge.deploy.as_mut().unwrap().check_job = CheckJob::Ended("failure".into());
        let done = check(&forge, "v3.3.0", &served("3.3.0"), &mut |_| {}).unwrap();
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
        let why = format!(
            "{}. {}",
            text::deploy_check_none("https://github.com/o/r/actions/runs/9", Some("failure")),
            text::check_by_hand("v3.3.0", "2026-10-18T20:00:00Z", "c1")
        );
        let names: Vec<&str> = lines
            .iter()
            .filter(|(_, v)| *v == Verdict::Unknown(why.clone()))
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(names, LOG_CHECKS, "{lines:?}");
        assert!(lines.contains(&("riff server shows the build", Verdict::Passed)));
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
        assert!(!log.iter().any(|l| l.starts_with("close")), "{log:?}");
        assert!(!log[1].contains("gcloud auth login"), "{log:?}");
    }

    /// 01M76SSFTGW41RHHV445ZM5RHY: a status of an older deploy counts as
    /// none. A run by hand names the check of a person on a host. A
    /// status `error` gives its description.
    #[test]
    fn check_takes_only_a_status_of_this_deploy() {
        let unknown = |forge: &Fake| -> Vec<String> {
            let done = check(forge, "v3.3.0", &served("3.3.0"), &mut |_| {}).unwrap();
            let Checked::Ran { lines, .. } = done else {
                panic!("{done:?}");
            };
            lines
                .into_iter()
                .filter_map(|(_, v)| match v {
                    Verdict::Unknown(why) => Some(why),
                    _ => None,
                })
                .collect()
        };
        let mut old = deploy_status("success", LogMeasures::default());
        old.status.as_mut().unwrap().at = "2026-10-17T10:00:00Z".into();
        let why = unknown(&deployed(old.clone()));
        assert_eq!(why.len(), 4, "{why:?}");
        assert!(
            why[0].contains("ended with success, and set no status riff/deploy-check"),
            "{why:?}"
        );

        let mut by_hand = deployed(old);
        by_hand.deploy.as_mut().unwrap().by_hand = true;
        let why = unknown(&by_hand);
        assert_eq!(
            why[0],
            text::deploy_check_by_hand("v3.3.0", "2026-10-18T20:00:00Z", "c1")
        );

        let mut error = deploy_status("error", LogMeasures::default());
        error.status.as_mut().unwrap().description =
            "riff could not read the Cloud Run log: denied".into();
        let why = unknown(&deployed(error));
        assert_eq!(
            why[0],
            "the status riff/deploy-check of v3.3.0 says: riff could not read the Cloud Run log: \
             denied"
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
        let done = check(&forge, "v3.3.0", &served("3.2.0"), &mut |_| {}).unwrap();
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
            let error = check(&forge, "v3.3.0", &served("3.3.0"), &mut |_| {})
                .unwrap_err()
                .to_string();
            assert!(error.contains(want), "{want}: {error}");
            assert!(forge.log().is_empty());
        }
        let error = check(&Fake::new(), "v3.3.0", &served("3.3.0"), &mut |_| {})
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
        assert!(
            error.contains(
                "next: riff release test vX.Y.Z --host HOST, then riff release cut X.Y.Z"
            ),
            "{error}"
        );
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

        // Each item in a checked release, and a pull request of the next
        // version in the wave that counts for nothing.
        let forge = Fake {
            merged: vec![
                wave_item(41, "c41"),
                Merged {
                    next: true,
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
        for role in [None, Some(PersonRole::Admin)] {
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
        for role in [None, Some(PersonRole::Admin)] {
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
        for role in [None, Some(PersonRole::Admin)] {
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

    /// 01M6WZ40SFVZ6CRQ1H4Z27CWP4: `riff release approve` refuses each
    /// agent session, also the lead of an admin, in a container and in a
    /// terminal. The guard reads no word of an admin in riff chat: only a
    /// person in a terminal gets past it.
    #[test]
    fn approve_refuses_each_agent_session_also_the_lead_of_an_admin() {
        let lead = Caller {
            agent: true,
            lead: Some(true),
            ..Caller::default()
        };
        let lead_container = Caller {
            container: true,
            lead_container: true,
            ..lead
        };
        let sessions = [
            Caller {
                worker: true,
                agent: true,
                lead: Some(false),
                ..Caller::default()
            },
            Caller {
                container: true,
                agent: true,
                ..Caller::default()
            },
            Caller {
                agent: true,
                lead: Some(false),
                ..Caller::default()
            },
            Caller {
                agent: true,
                ..Caller::default()
            },
            lead,
            lead_container,
        ];
        let roles = [None, Some(PersonRole::Admin), Some(PersonRole::Member)];
        for session in sessions {
            for role in roles {
                let caller = Caller { role, ..session };
                assert_eq!(caller.person_step(), Some(Refusal::Agent), "{caller:?}");
            }
        }
        // The lead of an admin still runs the steps of the lead.
        for lead in [lead, lead_container] {
            let admin = Caller {
                role: Some(PersonRole::Admin),
                ..lead
            };
            assert_eq!(admin.lead_step(), None, "{admin:?}");
        }
        // Only a person in a terminal approves.
        for role in &roles[..2] {
            let person = Caller {
                role: *role,
                ..Caller::default()
            };
            assert_eq!(person.person_step(), None, "{person:?}");
        }
    }
}
