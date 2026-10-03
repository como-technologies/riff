//! riff tidies the worktrees of a workers machine by itself, and keeps
//! their build folders in the disk.
//!
//! # Design
//!
//! With one context for one item, the author releases its item at the
//! verify request, and the verifier often ends before the merge. So no
//! session removes the worktree after the merge. riff does it itself:
//! each [`EVERY`], a workers host and the `riff mcp` of the lead run a
//! tidy of the clone of their machine (01M41A118QPQKFAAHGQFFX4F3B). A
//! worktree of a merged pull request goes away within [`EVERY`] of the
//! merge, with no session and no person.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant H as workers host, or riff mcp of the lead
//!     participant G as git and gh
//!     participant S as riff-server
//!     loop each 10 minutes
//!         H->>G: riff worktrees clean
//!         H->>H: measure the disk
//!         opt under 15% free
//!             H->>H: remove the target of each worktree with no live owner
//!             H->>S: a note to the lead
//!         end
//!         opt under 5% free, the first look
//!             H->>S: one note to the lead: no worker starts here
//!         end
//!     end
//! ```
//!
//! - The clean is [`crate::worktrees::clean_here`]: the same facts as
//!   `riff worktrees clean`.
//! - A worktree has no live owner when no live process holds its lock,
//!   no person locked it, and `riff who` shows no live session in it
//!   ([`crate::worktrees::ownerless`]). riff removes only its `target`:
//!   the next build makes it again. The source and the branch stay
//!   (01M41A11BB4HAD8595DNSBAZ0D).
//! - Under [`crate::disk::LOW_PERCENT`], `riff workers start`, the
//!   rollout and a workers host start no worker on the machine
//!   (01M41A11DX1QRP48YPTDNT67W4). The lead gets one note when the
//!   disk goes under the mark ([`Guard`]), not one at each look.
//!
//! ```
//! use riff::disk::Disk;
//! use riff::tidy::Guard;
//!
//! let mut guard = Guard::default();
//! let low = Some(Disk { free_gb: 4, total_gb: 100 });
//! let fine = Some(Disk { free_gb: 50, total_gb: 100 });
//! assert!(guard.crossed(low), "the first look under the mark");
//! assert!(!guard.crossed(low), "one note, not one at each look");
//! assert!(!guard.crossed(fine));
//! assert!(guard.crossed(low), "under the mark again");
//! ```

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::Kind;

use crate::api::Api;
use crate::disk::Disk;
use crate::worktrees::{self, Tree};
use crate::{identity, text};

/// The time between two tidies (01M41A118QPQKFAAHGQFFX4F3B).
pub const EVERY: Duration = Duration::from_secs(600);

/// The variable that gives the time between two tidies in seconds, for
/// tests.
pub const EVERY_VAR: &str = "RIFF_TIDY_EVERY";

/// The time between two tidies: [`EVERY`], or the seconds of
/// [`EVERY_VAR`].
pub fn every() -> Duration {
    std::env::var(EVERY_VAR)
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&s| s > 0)
        .map_or(EVERY, Duration::from_secs)
}

/// The state of the disk at the look before, so that the lead gets one
/// note when the disk goes under [`crate::disk::LOW_PERCENT`].
#[derive(Debug, Default)]
pub struct Guard {
    low: bool,
}

impl Guard {
    /// True when `disk` is low and the disk of the look before was not.
    pub fn crossed(&mut self, disk: Option<Disk>) -> bool {
        let low = disk.is_some_and(|d| d.low());
        let crossed = low && !self.low;
        self.low = low;
        crossed
    }
}

/// Removes the `target` of each worktree of `trees`
/// (01M41A11BB4HAD8595DNSBAZ0D). Returns one line for each `target`.
pub fn trim(trees: &[Tree]) -> Vec<String> {
    trees
        .iter()
        .filter_map(|tree| {
            let target = tree.path.join("target");
            if !target.is_dir() {
                return None;
            }
            let path = tree.path.display();
            Some(match std::fs::remove_dir_all(&target) {
                Ok(()) => format!("{path}: {}", text::TARGET_REMOVED),
                Err(e) => format!("{path}: kept its target: {e}"),
            })
        })
        .collect()
}

/// One tidy of the clone of `dir` as the person `me`
/// (01M41A118QPQKFAAHGQFFX4F3B): the clean, then the guard of the disk.
/// It posts a note to the lead for the targets that it removed and when
/// the disk goes under the low mark. Returns the lines to print.
pub async fn tidy(
    api: &Api,
    me: &SessionUri,
    dir: &Path,
    guard: &mut Guard,
) -> Result<Vec<String>> {
    let mut lines = worktrees::clean_here(api, me, dir).await?;
    let main = identity::main_worktree(dir)
        .ok_or_else(|| anyhow::anyhow!("run it in a git repository"))?;
    let host = me.place().host().to_owned();
    let repo = me.place().repo_text();
    let mut notes = Vec::new();
    if Disk::here(&main).is_some_and(|d| d.tight()) {
        let who = api.who(me, false).await?;
        let trimmed = {
            let (main, here, host, repo) =
                (main.clone(), dir.to_owned(), host.clone(), repo.clone());
            tokio::task::spawn_blocking(move || {
                let trees = worktrees::ownerless(&main, &here, |tree| {
                    worktrees::owned_by(&who, &host, &repo, tree)
                });
                trim(&trees)
            })
            .await?
        };
        if !trimmed.is_empty() {
            notes.push(text::targets_removed(&host, Disk::here(&main), &trimmed));
        }
        lines.extend(trimmed);
    }
    let disk = Disk::here(&main);
    if guard.crossed(disk)
        && let Some(disk) = disk
    {
        notes.push(text::disk_low(&host, &disk));
    }
    let to = [Selector::lead(me.who().user(), &repo)];
    for note in &notes {
        lines.push(note.clone());
        if let Err(e) = api.post(me, None, &to, note, Kind::Note).await {
            eprintln!("riff: cannot post the note to the lead: {e:#}");
        }
    }
    Ok(lines)
}

/// [`tidy`] in `dir` as the person of this host.
pub async fn tidy_as_person(dir: &Path, server: &str, guard: &mut Guard) -> Result<Vec<String>> {
    let here = identity::place(dir)?;
    let me = identity::person(&here, server)?;
    let api = Api::new(server).signed_in(None)?;
    tidy(&api, &me, dir, guard).await
}

/// A timer that ticks each [`every`], first after one wait: the start
/// of a host cleans already.
pub fn timer() -> tokio::time::Interval {
    let every = every();
    let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    timer
}
