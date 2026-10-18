//! The release cycle in riff commands: `riff release cut`, `publish`,
//! `status` and `approve` (#671).
//!
//! # Design
//!
//! A release is four riff commands. No session writes a script or a raw
//! `gh` or `git` line for it. Each command finds the versions, the
//! commits, the tags and the IDs itself, and names the failed check
//! when it refuses. A person only approves the deploy.
//!
//! | Command | Who | What |
//! |---|---|---|
//! | `riff release cut X.Y.Z` | a lead or a worker | the release pull request from `origin/main` ([`cut`], 01M58B0HZYMAK1B2AJMWKHCXG0) |
//! | `riff release publish vX.Y.Z` | the lead, or a person, with a `gh` of an admin | the tag and the GitHub release ([`publish`], 01M58B0JABCET719T3HFB19NDW) |
//! | `riff release status` | each session | the version on `origin/main`, the last release, what waits ([`status`], 01M58B0JEYNAW67NS226FMX5XH) |
//! | `riff release approve vX.Y.Z` | a person in a terminal | the approval of the environment `production` ([`approve`], 01M58B0JMFQ5E40TWTQ48MPSGK) |
//!
//! ```mermaid
//! sequenceDiagram
//!     participant L as the lead
//!     participant G as GitHub
//!     participant P as a person
//!     L->>G: riff release cut 3.3.0: branch release-v3.3.0, pull request "Release v3.3.0"
//!     G->>G: verify, Gate, merge
//!     L->>G: riff release publish v3.3.0: tag, GitHub release
//!     G->>G: CI of the tag: Release check, Static riff
//!     G-->>P: the job Deploy waits for production
//!     P->>G: riff release approve v3.3.0
//! ```
//!
//! `cut` refuses a version that is not newer than the last release, and
//! refuses while the version on `origin/main` has no published release.
//! Only `--skip VERSION --reason TEXT` lets it go on: the reason goes
//! into the body of the pull request, and from there into the notes of
//! the release (01M58B0J4VQ61ER69Q09JYP7CG). The pull request says
//! `Refs #N` and `Part of #N` for the release issue, never `Closes #N`:
//! the release issue closes only when the checks after the deploy pass.
//!
//! The guards of the callers are in [`Caller`]: `publish` refuses a
//! worker, a container session and a session that is not the lead.
//! `approve` refuses each agent session (01M58B0JSQMWNCMSNX19536XT8).
//!
//! Each step goes through [`Forge`]. [`GitHub`] runs `gh` and `git`; a
//! test gives a fake forge.
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
use riff_core::build::Semver;
use serde::Deserialize;

use crate::pr::{self, Gh};
use crate::text;

/// The label of a release pull request and of a release issue. The
/// notes of a release leave out the pull requests with it.
pub const LABEL: &str = "release";

/// The GitHub environment of the deploy to the shared riff.
pub const PRODUCTION: &str = "production";

/// The workflow that runs for a tag.
pub const WORKFLOW: &str = "CI";

/// The file of the plugin with its version.
pub const PLUGIN: &str = "crates/riff/claude-plugin/riff/.claude-plugin/plugin.json";

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
    let Some(rest) = tag.strip_prefix('v') else {
        bail!("\"{tag}\" is not a release tag vX.Y.Z, for example v3.3.0.");
    };
    version(rest).map_err(|_| anyhow!("\"{tag}\" is not a release tag vX.Y.Z, for example v3.3.0."))
}

/// The tag of a version: `vX.Y.Z`.
pub fn tag(version: Semver) -> String {
    format!("v{version}")
}

/// The title of the release pull request and of the release issue.
pub fn title(version: Semver) -> String {
    format!("Release {}", tag(version))
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

/// The `plugin.json` of the plugin with the version `new`.
///
/// ```
/// let json = "{\n  \"name\": \"riff\",\n  \"version\": \"3.2.0\"\n}\n";
/// let new = riff::cycle::set_plugin(json, riff::cycle::version("3.3.0")?)?;
/// assert_eq!(new, "{\n  \"name\": \"riff\",\n  \"version\": \"3.3.0\"\n}\n");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn set_plugin(json: &str, new: Semver) -> Result<String> {
    let mut done = false;
    let lines: Vec<String> = json
        .split('\n')
        .map(|line| {
            let key = line.trim_start();
            if !done && key.starts_with("\"version\": \"") {
                done = true;
                let indent = &line[..line.len() - key.len()];
                let comma = if key.trim_end().ends_with(',') {
                    ","
                } else {
                    ""
                };
                format!("{indent}\"version\": \"{new}\"{comma}")
            } else {
                line.to_owned()
            }
        })
        .collect();
    if !done {
        bail!("{PLUGIN} has no line \"version\"");
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The open release issue `title`, made in `wave` when there is
    /// none.
    fn release_issue(&self, title: &str, wave: &str) -> Result<u64>;
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
///     number: n, title: format!("Change {n}"), wave: Some(wave.into()), issue: Some(issue), release: false,
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
            "riff release cut: the forge has no published release. Make the first release by hand."
        );
    };
    if new <= last {
        bail!(
            "riff release cut: {new} is not newer than the last release {}. Give a version after {last}.",
            tag(last)
        );
    }
    let main = forge.main_version()?;
    if main > last && !releases.contains(&main) {
        match skip {
            Some(skip) if skip.version == main && !skip.reason.trim().is_empty() => {}
            Some(skip) if skip.version != main => bail!(
                "riff release cut: --skip {} is not the version on origin/main: {main}.",
                skip.version
            ),
            _ => bail!(
                "riff release cut: the version {main} on origin/main has no published release. \
                 Publish it first: riff release publish {}. To go on with no release {main}, give \
                 --skip {main} --reason TEXT.",
                tag(main)
            ),
        }
    } else if let Some(skip) = skip {
        bail!(
            "riff release cut: --skip {} names no version to skip: the version {main} on \
             origin/main has its release.",
            skip.version
        );
    }
    let wave = forge.current_wave()?;
    let title = title(new);
    let issue = forge.release_issue(&title, &wave)?;
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
            "riff release cut: the pull request breaks issue hygiene:\n{}",
            lines.join("\n")
        );
    }
    let branch = forge.push_branch(new)?;
    let (number, url) = forge.open_pr(&branch, &title, &wave, &body)?;
    Ok(Cut { number, url, issue })
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

/// Publishes the release `tag` (01M58B0JABCET719T3HFB19NDW): it checks
/// each fact first, then makes and pushes the tag, then makes the GitHub
/// release. `paused` is the state of the riff: `None` when riff cannot
/// tell.
pub fn publish(forge: &dyn Forge, tag_text: &str, paused: Option<bool>) -> Result<Published> {
    let version = tag_version(tag_text)?;
    let tag = tag(version);
    match paused {
        Some(true) => {}
        Some(false) => bail!(
            "riff release publish: the riff is not paused. Stop the workers and pause the riff \
             first: riff pause --riff."
        ),
        None => bail!(
            "riff release publish: riff cannot tell that the riff is paused. Check the riff with \
             riff whoami."
        ),
    }
    let title = title(version);
    let Some(pr) = forge.release_pr(&title)? else {
        bail!(
            "riff release publish: no merged pull request \"{title}\" with the label {LABEL}. \
             Make it with riff release cut {version}."
        );
    };
    if !forge.verify_passed(&pr.head)? {
        bail!(
            "riff release publish: the check riff/verify of pull request #{} did not pass.",
            pr.number
        );
    }
    if !forge.on_main(&pr.merge)? {
        bail!(
            "riff release publish: the merge commit {} of pull request #{} is not on origin/main.",
            pr.merge,
            pr.number
        );
    }
    let main = forge.main_version()?;
    if main != version {
        bail!(
            "riff release publish: Cargo.toml on origin/main has the version {main}, not {version}."
        );
    }
    if forge.tag_exists(&tag)? {
        bail!("riff release publish: the tag {tag} exists on GitHub already.");
    }
    if !forge.admin()? {
        bail!(
            "riff release publish: the account of gh is not an admin of the repository, so the \
             ruleset of the tags refuses its push. Sign in to gh as an admin."
        );
    }
    let since = last(&forge.releases()?).map(self::tag);
    forge.push_tag(&tag, &pr.merge)?;
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
    Ok(Status {
        main,
        last: last(&releases),
        unpublished,
        waiting: forge.waiting()?,
    })
}

/// The lines of `riff release status`.
///
/// ```
/// use riff::cycle::{Status, render_status, version};
///
/// let status = Status { main: version("3.3.0")?, last: Some(version("3.2.0")?), unpublished: None, waiting: vec![] };
/// assert_eq!(
///     render_status(&status),
///     "version on origin/main: 3.3.0\nlast release: v3.2.0\nmerged, not published: none\n\
///      waits for production: none\n"
/// );
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn render_status(status: &Status) -> String {
    let last = status.last.map_or("none".to_owned(), tag);
    let unpublished = status.unpublished.as_ref().map_or("none".to_owned(), |pr| {
        format!(
            "{} (pull request #{}). Run: riff release publish {}",
            tag(status.main),
            pr.number,
            tag(status.main)
        )
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
    out
}

/// What [`approve`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approved {
    /// No CI run of the tag waits: riff changed nothing.
    NoRun,
    /// The person said no: riff changed nothing.
    No,
    /// riff approved the run.
    Yes(Waiting),
}

/// Approves the deploy of `tag` to [`PRODUCTION`]
/// (01M58B0JMFQ5E40TWTQ48MPSGK). It shows the tag, the commit and the
/// run to `out`, and asks `yes` on `answer`.
pub fn approve(
    forge: &dyn Forge,
    tag_text: &str,
    answer: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Approved> {
    let tag = tag(tag_version(tag_text)?);
    let Some(run) = forge.waiting()?.into_iter().find(|w| w.tag == tag) else {
        return Ok(Approved::NoRun);
    };
    write!(
        out,
        "tag: {}\ncommit: {}\nrun: {}\nApprove the deploy of {tag} to {PRODUCTION}? (yes or no) ",
        run.tag, run.commit, run.url
    )?;
    out.flush()?;
    let mut line = String::new();
    answer.read_line(&mut line)?;
    if !matches!(line.trim(), "yes" | "y") {
        return Ok(Approved::No);
    }
    forge.approve(&run, &format!("Release {tag}"))?;
    Ok(Approved::Yes(run))
}

/// The process that runs a command of the release cycle
/// (01M58B0JSQMWNCMSNX19536XT8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Caller {
    /// A worker: `RIFF_WORKER=1`.
    pub worker: bool,
    /// A container session: it has the socket of the riff daemon.
    pub container: bool,
    /// An agent session: it has a session ID, or runs under Claude Code.
    pub agent: bool,
    /// For an agent session: `Some(true)` when it is the lead of its
    /// user, `None` when riff cannot tell.
    pub lead: Option<bool>,
}

impl Caller {
    /// Why this caller may not run `riff release publish`, or `None`. A
    /// person in a plain terminal may, and the lead may.
    ///
    /// ```
    /// use riff::cycle::Caller;
    ///
    /// assert!(Caller::default().may_not_publish().is_none());
    /// let lead = Caller { agent: true, lead: Some(true), ..Caller::default() };
    /// assert!(lead.may_not_publish().is_none());
    /// let worker = Caller { worker: true, agent: true, lead: Some(false), ..Caller::default() };
    /// assert!(worker.may_not_publish().unwrap().contains("a worker"));
    /// ```
    pub fn may_not_publish(&self) -> Option<&'static str> {
        if self.worker {
            Some(text::PUBLISH.worker)
        } else if self.container {
            Some(text::PUBLISH_CONTAINER)
        } else if !self.agent {
            None
        } else {
            match self.lead {
                Some(true) => None,
                Some(false) => Some(text::PUBLISH.not_the_lead),
                None => Some(text::PUBLISH.lead_unknown),
            }
        }
    }

    /// Why this caller may not run `riff release approve`, or `None`.
    /// Only a person in a plain terminal may: never an agent session,
    /// also not the lead.
    ///
    /// ```
    /// use riff::cycle::Caller;
    ///
    /// assert!(Caller::default().may_not_approve().is_none());
    /// let lead = Caller { agent: true, lead: Some(true), ..Caller::default() };
    /// assert!(lead.may_not_approve().is_some());
    /// ```
    pub fn may_not_approve(&self) -> Option<&'static str> {
        (self.worker || self.container || self.agent).then_some(text::APPROVE_AGENT)
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
        let forge = Self {
            gh,
            dir: dir.to_owned(),
            run_waits: vec![Duration::from_secs(5); 6],
        };
        forge.git(&["fetch", "-q", "--tags", "--force", "origin"])?;
        Ok(forge)
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
        edit(PLUGIN, &|t| set_plugin(t, version))?;
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
}

#[derive(Deserialize)]
struct GhIssue {
    number: u64,
    title: String,
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

    fn release_issue(&self, title: &str, wave: &str) -> Result<u64> {
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
        let body = format!(
            "The release {}.\n\nDone when: the release is published, the deploy to the shared riff \
             passed, and the checks after the deploy passed.\n",
            title.trim_start_matches("Release ")
        );
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
            Some(&body),
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
            "databaseId,headBranch,headSha,url",
        ])?;
        let mut out = Vec::new();
        for run in runs {
            let path = format!(
                "repos/{{owner}}/{{repo}}/actions/runs/{}/pending_deployments",
                run.database_id
            );
            let pending: Vec<GhPending> = self.gh.json(&["api", &path])?;
            if let Some(p) = pending.iter().find(|p| p.environment.name == PRODUCTION) {
                out.push(Waiting {
                    run: run.database_id,
                    tag: text::forge(&run.head_branch),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A forge in memory. It logs each write.
    #[derive(Default)]
    struct Fake {
        main: String,
        releases: Vec<&'static str>,
        merged: Vec<Merged>,
        release_pr: Option<ReleasePr>,
        verify: bool,
        on_main: bool,
        tags: Vec<String>,
        admin: bool,
        waiting: Vec<Waiting>,
        log: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                main: "3.2.0".into(),
                releases: vec!["3.0.0", "3.2.0"],
                verify: true,
                on_main: true,
                admin: true,
                ..Fake::default()
            }
        }

        fn ready(main: &str) -> Self {
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
                ..Fake::new()
            }
        }

        fn log(&self) -> Vec<String> {
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
        fn release_issue(&self, title: &str, wave: &str) -> Result<u64> {
            self.write(format!("issue {title} in {wave}"));
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
    }

    fn v(text: &str) -> Semver {
        version(text).unwrap()
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
                release: false,
            },
            Merged {
                number: 40,
                title: "Release v3.2.0".into(),
                wave: Some("Wave 33".into()),
                issue: Some(6),
                release: true,
            },
        ];
        let cut = cut(&forge, v("3.3.0"), None, Some("New commands.")).unwrap();
        assert_eq!((cut.number, cut.issue), (1140, 7));
        let log = forge.log();
        assert_eq!(log[0], "issue Release v3.3.0 in Wave 34: Release");
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
                error.contains("is not newer than the last release v3.2.0"),
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
        assert!(error.contains("riff release publish v3.3.0"), "{error}");
        let other = Skip {
            version: v("3.1.0"),
            reason: "x".into(),
        };
        let error = cut(&forge, v("3.4.0"), Some(&other), None)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("--skip 3.1.0 is not the version on origin/main"),
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
    }

    #[test]
    fn publish_tags_the_merge_commit_and_makes_the_release() {
        let forge = Fake::ready("3.3.0");
        let published = publish(&forge, "v3.3.0", Some(true)).unwrap();
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

    #[test]
    fn publish_refuses_each_false_fact_and_changes_nothing() {
        let cases: Vec<(Fake, Option<bool>, &str)> = vec![
            (Fake::ready("3.3.0"), Some(false), "the riff is not paused"),
            (
                Fake::ready("3.3.0"),
                None,
                "cannot tell that the riff is paused",
            ),
            (
                Fake {
                    release_pr: None,
                    ..Fake::ready("3.3.0")
                },
                Some(true),
                "no merged pull request \"Release v3.3.0\"",
            ),
            (
                Fake {
                    verify: false,
                    ..Fake::ready("3.3.0")
                },
                Some(true),
                "riff/verify of pull request #1140 did not pass",
            ),
            (
                Fake {
                    on_main: false,
                    ..Fake::ready("3.3.0")
                },
                Some(true),
                "is not on origin/main",
            ),
            (
                Fake::ready("3.2.0"),
                Some(true),
                "has the version 3.2.0, not 3.3.0",
            ),
            (
                Fake {
                    tags: vec!["v3.3.0".into()],
                    ..Fake::ready("3.3.0")
                },
                Some(true),
                "the tag v3.3.0 exists",
            ),
            (
                Fake {
                    admin: false,
                    ..Fake::ready("3.3.0")
                },
                Some(true),
                "not an admin of the repository",
            ),
        ];
        for (forge, paused, want) in cases {
            let error = publish(&forge, "v3.3.0", paused).unwrap_err().to_string();
            assert!(error.contains(want), "{want}: {error}");
            assert!(forge.log().is_empty(), "{want}");
        }
        let error = publish(&Fake::ready("3.3.0"), "3.3.0", Some(true))
            .unwrap_err()
            .to_string();
        assert!(error.contains("is not a release tag"), "{error}");
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
    fn status_shows_an_unpublished_release_and_a_waiting_run() {
        let mut forge = Fake::ready("3.3.0");
        forge.waiting = vec![run("v3.2.0")];
        let text = render_status(&status(&forge).unwrap());
        assert_eq!(
            text,
            "version on origin/main: 3.3.0\nlast release: v3.2.0\n\
             merged, not published: v3.3.0 (pull request #1140). Run: riff release publish v3.3.0\n\
             waits for production: v3.2.0 https://github.com/o/r/actions/runs/9. A person runs: \
             riff release approve v3.2.0\n"
        );
    }

    #[test]
    fn approve_asks_and_approves_with_a_comment() {
        let mut forge = Fake::new();
        forge.waiting = vec![run("v3.3.0")];
        let mut out = Vec::new();
        let done = approve(&forge, "v3.3.0", &mut "yes\n".as_bytes(), &mut out).unwrap();
        assert_eq!(done, Approved::Yes(run("v3.3.0")));
        let shown = String::from_utf8(out).unwrap();
        assert!(
            shown.contains("tag: v3.3.0\ncommit: m1\nrun: https://github.com/o/r/actions/runs/9\n"),
            "{shown}"
        );
        assert_eq!(forge.log(), ["approve 9 5 Release v3.3.0"]);
    }

    #[test]
    fn approve_with_no_or_no_run_changes_nothing() {
        let mut forge = Fake::new();
        let none = approve(&forge, "v3.3.0", &mut "yes\n".as_bytes(), &mut Vec::new()).unwrap();
        assert_eq!(none, Approved::NoRun);
        forge.waiting = vec![run("v3.3.0")];
        let no = approve(&forge, "v3.3.0", &mut "no\n".as_bytes(), &mut Vec::new()).unwrap();
        assert_eq!(no, Approved::No);
        assert!(forge.log().is_empty());
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
        assert_eq!(worker.may_not_publish(), Some(text::PUBLISH.worker));
        assert_eq!(container.may_not_publish(), Some(text::PUBLISH_CONTAINER));
        assert_eq!(other.may_not_publish(), Some(text::PUBLISH.not_the_lead));
        assert_eq!(unknown.may_not_publish(), Some(text::PUBLISH.lead_unknown));
        for caller in [
            worker,
            container,
            other,
            unknown,
            Caller {
                agent: true,
                lead: Some(true),
                ..Caller::default()
            },
        ] {
            assert_eq!(caller.may_not_approve(), Some(text::APPROVE_AGENT));
        }
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
        let plugin = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../",
            "crates/riff/claude-plugin/riff/.claude-plugin/plugin.json"
        ))
        .unwrap();
        let new: serde_json::Value =
            serde_json::from_str(&set_plugin(&plugin, v("9.8.7")).unwrap()).unwrap();
        assert_eq!(new["version"], "9.8.7");
        let lock =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock"))
                .unwrap();
        let new = set_lock(&lock, v("9.8.7"));
        assert!(new.contains("name = \"riff\"\nversion = \"9.8.7\"\n"));
        assert!(new.contains("name = \"riff-server\"\nversion = \"9.8.7\"\n"));
        assert_eq!(new.lines().count(), lock.lines().count());
    }
}
