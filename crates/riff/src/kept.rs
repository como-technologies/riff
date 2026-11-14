//! The result of a verify that riff could not report (#1557).
//!
//! # Design
//!
//! The daemon reports the result of a verify of a wave when the session
//! ends: the comment on the pull request, the status `riff/verify` of the
//! commit, and the result to the holder of the item and to the lead
//! ([`crate::wave`]). A verify can run for an hour, and it can end while the
//! limit of GitHub is out ([`crate::ghcalls`]). The report then fails, and
//! the whole verify would run again.
//!
//! So the end step keeps the result in the folder of the session, in the
//! file [`FILE`]: the verdict, the tested commit and the text. The loop of
//! the wave reports each kept result of its repository after the reset
//! ([`report_all`]), in the same way as the end step. The folder stays: the
//! sweep removes it when the pull request merges or closes
//! ([`crate::daemon::sweep`]). The report of a kept PASS reads the council
//! of the pull request in the clone of the folder ([`crate::pr::report`]).
//!
//! ```mermaid
//! flowchart TD
//!     E["a verify ends with PASS or FAIL"] --> R{"report to GitHub"}
//!     R -- "done" --> P["post the result to the holder and the lead"]
//!     R -- "fails: the limit is out" --> K["keep the result in the folder"]
//!     K --> W["the wave waits for the reset"]
//!     W --> H{"the pull request is open<br/>at the same head commit?"}
//!     H -- yes --> R2["report, post, and remove the file"]
//!     H -- "no: the head changed, or it closed" --> D["drop the result;<br/>the wave starts a new verify"]
//! ```
//!
//! A result of a head that changed counts for nothing: a verify counts only
//! for its commit ([`crate::pr::same_commit`]).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use riff_client::api::Api;
use riff_core::name::SessionUri;
use serde::{Deserialize, Serialize};

use crate::pr::{self, Gh, Verdict};
use crate::wave::Wave;
use crate::{ghcalls, run, text};

/// The file of a kept result, in the folder of the session of the verify.
pub const FILE: &str = "verify-kept.json";

/// The result of a verify that riff has not reported yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// The item of the verify, for example `verify-issue-12`.
    pub item: String,
    /// The pull request that the verify tested.
    pub pull: u64,
    /// The commit that the verifier tested.
    pub commit: String,
    /// The verdict.
    pub verdict: Verdict,
    /// The text after the verdict line.
    pub result: String,
}

impl Kept {
    /// Writes the result in `folder`.
    ///
    /// ```
    /// use riff::kept::Kept;
    /// use riff::pr::Verdict;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let kept = Kept {
    ///     repo: "o/r".into(),
    ///     item: "verify-issue-12".into(),
    ///     pull: 40,
    ///     commit: "1a2b3c4d5e".into(),
    ///     verdict: Verdict::Pass,
    ///     result: "- Done.".into(),
    /// };
    /// kept.keep(dir.path())?;
    /// assert_eq!(Kept::read(dir.path()), Some(kept));
    /// assert_eq!(Kept::read(&dir.path().join("none")), None);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn keep(&self, folder: &Path) -> Result<()> {
        let path = folder.join(FILE);
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json).with_context(|| format!("cannot write {}", path.display()))
    }

    /// The result kept in `folder`, when there is one.
    pub fn read(folder: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(folder.join(FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }
}

/// The folders in `sessions` with a kept result of `repo`.
pub fn find(sessions: &Path, repo: &str) -> Vec<(PathBuf, Kept)> {
    let Ok(entries) = std::fs::read_dir(sessions) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, Kept)> = entries
        .flatten()
        .filter_map(|entry| {
            let folder = entry.path();
            Kept::read(&folder)
                .filter(|kept| kept.repo == repo)
                .map(|kept| (folder, kept))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Reports each kept result of the wave `wave` that is in `sessions`, as
/// `me` (#1557). It does nothing while a limit of GitHub is out, and it
/// stops at a report that fails on a limit. A result of a pull request that
/// is not open at the tested commit goes. It gives a line for each result
/// that it reported or dropped.
pub async fn report_all(
    gh: &Gh,
    api: &Api,
    me: &SessionUri,
    wave: &Wave,
    sessions: &Path,
) -> Vec<String> {
    let mut lines = Vec::new();
    for (folder, kept) in find(sessions, &wave.repo) {
        if gh.limit_wait().is_some() {
            break;
        }
        match report_one(gh, api, me, wave, &folder, &kept).await {
            Ok(line) => lines.push(line),
            Err(e) if ghcalls::is_limit_error(&e) => break,
            Err(e) => eprintln!("riff daemon: {}: {}: {e:#}", wave.repo, kept.item),
        }
    }
    lines
}

/// Reports the result `kept` of the folder `folder`, and gives its line. A
/// report that fails on a limit is an error, and the result stays.
async fn report_one(
    gh: &Gh,
    api: &Api,
    me: &SessionUri,
    wave: &Wave,
    folder: &Path,
    kept: &Kept,
) -> Result<String> {
    let Kept {
        item,
        pull,
        commit,
        verdict,
        result,
        ..
    } = kept;
    let read = {
        let (gh, n) = (gh.clone(), pull.to_string());
        crate::wave::blocking(move || {
            gh.json::<run::Pull>(&["pr", "view", &n, "--json", "number,url,headRefOid,state"])
        })
        .await?
    };
    if read.state != "OPEN" || pr::same_commit(&read.head_ref_oid, commit).is_err() {
        let _ = std::fs::remove_dir_all(folder);
        return Ok(text::wave_kept_dropped(item, *pull, commit));
    }
    let reported = {
        let (gh, repo, tested, shown) = (
            gh.clone(),
            wave.repo.clone(),
            commit.clone(),
            result.clone(),
        );
        let (pull, verdict, clone) = (*pull, *verdict, folder.join("clone"));
        crate::wave::blocking(move || {
            pr::report(&gh, &repo, &clone, pull, &tested, verdict, &shown)
        })
        .await
    };
    let thread = wave.thread()?;
    let reported = match reported {
        Ok(reported) => reported,
        Err(e) if ghcalls::is_limit_error(&e) => return Err(e),
        Err(e) => {
            // The same end as a report that fails with no kept result.
            let _ = std::fs::remove_dir_all(folder);
            // The read above found the pull request open.
            let body = crate::wave::no_report(item, *pull, &e, true);
            let to = [riff_core::selector::Selector::lead(&wave.repo)];
            api.post(
                me,
                Some(&thread),
                &to,
                &body,
                riff_core::wire::Kind::Message,
            )
            .await?;
            return Ok(body);
        }
    };
    let line = text::verify_reported(*verdict, *pull, &reported);
    println!("{line}");
    let _ = std::fs::remove_file(folder.join(FILE));
    if *verdict == Verdict::Fail {
        let _ = std::fs::remove_dir_all(folder);
    }
    crate::wave::post_report(api, me, &thread, (*verdict, *pull), &reported, result).await?;
    Ok(line)
}
