//! The read of GitHub for `riff wave list`, kept on the host (#1609,
//! 01M6QBEV0PNMAESMW620AF5HH7).
//!
//! # Design
//!
//! `riff wave list` is a short command, and a person puts it in `watch`.
//! At each run it read the open milestones, the issues and the pull
//! requests again: three GraphQL calls and more, nonstop. Three `watch`
//! processes used most of the 5,000 GraphQL points of the hour, and the
//! waves of the riff stopped. So the command keeps its read in a file of
//! the host, one for each repository, and the runs share it:
//!
//! - A read that is less than [`KEEP`] old is the answer. The run makes
//!   no call of `gh`.
//! - A run with an older read asks GitHub first whether anything changed.
//!   It sends the two conditional reads of the idle wave
//!   ([`crate::changes`], #1566): the issue and the pull request that
//!   changed last, each with its `ETag`. When both answer `304`, no pull
//!   request was open at the last full read, and that read began less
//!   than [`FULL_EVERY`] ago, the run keeps the issues and the pull
//!   requests of that read. A `304` costs no point. It reads the open
//!   milestones again, because they cost no GraphQL point: a rename
//!   shows at once, and the run reads in full when the milestones are
//!   not the same.
//! - Else the run reads in full, and keeps the read.
//! - `riff wave list --fresh` reads in full at once, and keeps the read.
//!
//! The list names the age of the read when it is older than
//! [`SHOW_AGE_FROM`] ([`crate::text::waves_age`]).
//!
//! ```mermaid
//! flowchart TD
//!     R["riff wave list"] --> F{"--fresh?"}
//!     F -- yes --> A
//!     F -- no --> K{"a kept read<br/>less than 60 seconds old?"}
//!     K -- yes --> S["show it, with its age"]
//!     K -- no --> A["two conditional reads of REST,<br/>and the open milestones"]
//!     A --> N{"both 304, no pull request open,<br/>same milestones?"}
//!     N -- yes --> C["keep the issues of the last full read<br/>(no GraphQL point)"]
//!     N -- no --> G["the full read: issues and pull requests"]
//!     C --> W["keep the read as new as now"]
//!     G --> W
//!     W --> S
//! ```
//!
//! # The limits
//!
//! - The checks of a pull request do not change its update time. So
//!   while a pull request is open, a run with an older read reads in
//!   full: one full read each [`KEEP`] at the most.
//! - A read that fails is not kept: the next run tries again.
//! - The file is in the folder of the daemon of the machine
//!   ([`crate::daemon::dir`]). A run in a container has the folder of
//!   its container, and shares with the runs there only.
//! - `riff wave make`, `rename`, `end` and `move` delete the file of
//!   their repository ([`forget`]), so the next list shows what they did.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use riff_client::waves::{Issues, Milestone};
use serde::{Deserialize, Serialize};

use crate::changes::{FULL_EVERY, issues_path, pulls_path};
use crate::pr::{Answer, Gh};
use crate::{text, top, waves};

/// How long a read is the answer of `riff wave list`.
pub const KEEP: Duration = Duration::from_secs(60);

/// The list names the age of a read that is older than this.
pub const SHOW_AGE_FROM: Duration = Duration::from_secs(10);

/// The folder of the files, in the folder of the daemon.
const FOLDER: &str = "wave-list";

/// What the host keeps of one repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Kept {
    /// When the read was new, in milliseconds since the epoch: the end
    /// of the last full read, or of the last `304`.
    at_ms: i64,
    /// When the last full read began.
    full_ms: i64,
    /// The `ETag` of each conditional read from before the last full
    /// read.
    issues_etag: Option<String>,
    pulls_etag: Option<String>,
    /// True when the last full read saw no open pull request.
    no_pulls: bool,
    milestones: Vec<Milestone>,
    issues: Issues,
}

/// What `riff wave list` shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub milestones: Vec<Milestone>,
    /// The open issues and pull requests, or why riff could not read
    /// them ([`crate::text::waves_no_items`] tells it).
    pub issues: Result<Issues, String>,
    /// How old the read is.
    pub age: Duration,
}

/// The file of `repo` (`OWNER/REPO`) in the folder `dir` of the daemon.
///
/// ```
/// use std::path::Path;
///
/// assert_eq!(
///     riff::wave_list::path(Path::new("/d"), "o/r"),
///     Path::new("/d/wave-list/o~r.json")
/// );
/// ```
pub fn path(dir: &Path, repo: &str) -> PathBuf {
    // A name on GitHub has no `~`, so two repositories never share a file.
    dir.join(FOLDER)
        .join(format!("{}.json", repo.replace('/', "~")))
}

/// The list of `repo` (`OWNER/REPO`) from the read that the host keeps,
/// or from `gh` ([`read_in`]), now. With no `repo`, the folder has none:
/// the milestones come from `gh`, and the issues fail.
pub fn read(gh: &Gh, repo: Option<&str>, fresh: bool) -> Result<Listed> {
    let Some(repo) = repo else {
        return Ok(Listed {
            milestones: waves::open(gh)?,
            issues: Err(text::waves_no_repo()),
            age: Duration::ZERO,
        });
    };
    let dir = crate::daemon::dir().ok();
    read_in(dir.as_deref(), gh, repo, fresh, crate::daemon::now_ms())
}

/// Deletes the kept read of `repo`, after a step that changes the waves.
/// It never fails.
pub fn forget(repo: &str) {
    if let Ok(dir) = crate::daemon::dir() {
        let _ = std::fs::remove_file(path(&dir, repo));
    }
}

/// [`read`] with the folder `dir` of the daemon (`None`: keep nothing)
/// and the time `now_ms` in milliseconds since the epoch.
///
/// ```
/// use riff::pr::Gh;
/// use riff::wave_list::read_in;
///
/// let dir = tempfile::tempdir()?;
/// let script = r#"#!/bin/sh
/// echo "$*" >> "$(dirname "$0")/calls"
/// case "$1 $2" in
///   "api -i") printf 'HTTP/2.0 200 OK\r\nEtag: "v1"\r\n\r\n[]\n' ;;
///   api*) echo '[{"number":3,"title":"Wave 3","open_issues":1,"closed_issues":0}]' ;;
///   "issue list") echo '[{"number":12,"title":"t","milestone":{"title":"Wave 3"}}]' ;;
///   *) echo '[]' ;;
/// esac
/// "#;
/// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// let calls = || std::fs::read_to_string(dir.path().join("calls")).unwrap_or_default().lines().count();
///
/// // The first run reads GitHub in full.
/// let first = read_in(Some(dir.path()), &gh, "o/r", false, 0)?;
/// assert!(first.issues.is_ok());
/// let after_first = calls();
/// assert!(after_first > 2);
///
/// // 30 seconds later the run makes no call of gh, and the read is old.
/// let again = read_in(Some(dir.path()), &gh, "o/r", false, 30_000)?;
/// assert_eq!(again.age.as_secs(), 30);
/// assert_eq!(calls(), after_first);
///
/// // --fresh reads again at once.
/// let fresh = read_in(Some(dir.path()), &gh, "o/r", true, 31_000)?;
/// assert_eq!(fresh.age.as_secs(), 0);
/// assert!(calls() > after_first);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read_in(
    dir: Option<&Path>,
    gh: &Gh,
    repo: &str,
    fresh: bool,
    now_ms: i64,
) -> Result<Listed> {
    let file = dir.map(|dir| path(dir, repo));
    let kept = file.as_deref().and_then(load).filter(|_| !fresh);
    if let Some(kept) = &kept
        && let Some(age) = age_within(kept.at_ms, now_ms, KEEP)
    {
        return Ok(listed(kept, age));
    }
    // The answers come before the read: a change between the two reads
    // shows at the next run.
    let ask = |path: String, old: Option<&Option<String>>| {
        let etag = old.and_then(Option::as_deref);
        gh.ask(&path, etag)
            .unwrap_or(Answer::Changed { etag: None })
    };
    let issues_answer = ask(issues_path(repo), kept.as_ref().map(|k| &k.issues_etag));
    let pulls_answer = ask(pulls_path(repo), kept.as_ref().map(|k| &k.pulls_etag));
    let milestones = waves::open(gh)?;
    if let Some(kept) = &kept
        && issues_answer == Answer::Same
        && pulls_answer == Answer::Same
        && kept.no_pulls
        && age_within(kept.full_ms, now_ms, FULL_EVERY).is_some()
        && same_milestones(&kept.milestones, &milestones)
    {
        let renewed = Kept {
            at_ms: now_ms,
            milestones,
            ..kept.clone()
        };
        if let Some(file) = &file {
            save(file, &renewed);
        }
        return Ok(listed(&renewed, Duration::ZERO));
    }
    // A read that fails is not kept, and the milestones of the run show
    // with the line that says why.
    let (read, no_pulls) = match top::issues_and_pulls(gh, repo) {
        Ok((issues, no_pulls)) => (Ok(issues), no_pulls),
        Err(e) => (Err(format!("{e:#}")), false),
    };
    if let (Ok(issues), Some(file)) = (&read, &file) {
        // An answer `304` and a read that changed: the `ETag` is the old
        // one only when GitHub said that it still holds.
        let etag = |answer: Answer, old: Option<&Option<String>>| match answer {
            Answer::Same => old.cloned().flatten(),
            Answer::Changed { etag } => etag,
        };
        let fresh_read = Kept {
            at_ms: now_ms,
            full_ms: now_ms,
            issues_etag: etag(issues_answer, kept.as_ref().map(|k| &k.issues_etag)),
            pulls_etag: etag(pulls_answer, kept.as_ref().map(|k| &k.pulls_etag)),
            no_pulls,
            milestones: milestones.clone(),
            issues: issues.clone(),
        };
        save(file, &fresh_read);
    }
    Ok(Listed {
        milestones,
        issues: read,
        age: Duration::ZERO,
    })
}

/// The list of the read `kept`, `age` old.
fn listed(kept: &Kept, age: Duration) -> Listed {
    Listed {
        milestones: kept.milestones.clone(),
        issues: Ok(kept.issues.clone()),
        age,
    }
}

/// How old a read that began at `at_ms` is at `now_ms`, when it is less
/// than `limit` old. A read from the future is not within any limit.
fn age_within(at_ms: i64, now_ms: i64, limit: Duration) -> Option<Duration> {
    let age = Duration::from_millis(u64::try_from(now_ms - at_ms).ok()?);
    (age < limit).then_some(age)
}

/// True when both lists have the same milestones, by number and title.
/// The counts of open and closed issues may differ.
fn same_milestones(old: &[Milestone], new: &[Milestone]) -> bool {
    let key = |list: &[Milestone]| -> Vec<(u64, String)> {
        list.iter().map(|m| (m.number, m.title.clone())).collect()
    };
    key(old) == key(new)
}

/// The kept read in `file`. A file that is missing or that riff cannot
/// read is no read.
fn load(file: &Path) -> Option<Kept> {
    serde_json::from_slice(&std::fs::read(file).ok()?).ok()
}

/// Keeps `kept` in `file`, with a rename, so that a run never reads half
/// of a file. It never fails: a read that riff cannot keep is dropped.
fn save(file: &Path, kept: &Kept) {
    let _ = try_save(file, kept);
}

fn try_save(file: &Path, kept: &Kept) -> Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let tmp = file.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec(kept)?)?;
    std::fs::rename(&tmp, file).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn milestone(number: u64, title: &str, open: u64) -> Milestone {
        Milestone {
            number,
            title: title.into(),
            open_issues: open,
            closed_issues: 0,
        }
    }

    #[test]
    fn a_read_is_within_a_limit_only_when_it_is_younger_and_not_from_the_future() {
        let limit = Duration::from_secs(60);
        assert_eq!(
            age_within(0, 59_999, limit),
            Some(Duration::from_millis(59_999))
        );
        assert_eq!(age_within(0, 60_000, limit), None);
        assert_eq!(age_within(5_000, 4_000, limit), None);
    }

    #[test]
    fn milestones_are_the_same_by_number_and_title_not_by_count() {
        let old = [milestone(1, "Wave 1", 3), milestone(2, "Backlog", 0)];
        assert!(same_milestones(
            &old,
            &[milestone(1, "Wave 1", 1), milestone(2, "Backlog", 9)]
        ));
        assert!(!same_milestones(
            &old,
            &[milestone(1, "Wave 1: Cloud", 3), milestone(2, "Backlog", 0)]
        ));
        assert!(!same_milestones(&old, &old[..1]));
    }

    #[test]
    fn a_kept_read_survives_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = path(dir.path(), "o/r");
        let kept = Kept {
            at_ms: 5,
            full_ms: 3,
            issues_etag: Some("W/\"a\"".into()),
            pulls_etag: None,
            no_pulls: true,
            milestones: vec![milestone(1, "Wave 1", 3)],
            issues: Issues::default(),
        };
        assert_eq!(load(&file), None);
        save(&file, &kept);
        assert_eq!(load(&file), Some(kept));
        // No file of a half write stays.
        let names: Vec<_> = std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["o~r.json"]);
    }
}
