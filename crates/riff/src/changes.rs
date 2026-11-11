//! The conditional read of the forge for the look of a wave (#1566,
//! 01M6JFJQD4H4GG0H0TFSW5G1PD).
//!
//! # Design
//!
//! An idle wave looks each 5 minutes ([`crate::wave::IDLE_LOOK_EVERY`]),
//! and each full look reads the open issues and pull requests with
//! GraphQL. So the look asks GitHub a cheap question first: did anything
//! change? It sends two reads of REST, each with the `ETag` of its last
//! answer in `If-None-Match` ([`crate::pr::Gh::ask`]):
//!
//! - `GET /repos/R/issues?state=all&sort=updated&direction=desc&per_page=1`
//! - `GET /repos/R/pulls?state=all&sort=updated&direction=desc&per_page=1`
//!
//! Each answers the newest item that changed. GitHub answers `304` when
//! it is still the same, and a `304` costs no point. When both answer
//! `304`, the look reads nothing else: it takes the facts of its last full
//! read from [`Memory`]. An idle wave then costs 0 GraphQL points.
//!
//! ```mermaid
//! flowchart TD
//!     L["a look of the wave"] --> A["two conditional reads:<br/>issues, pull requests"]
//!     A --> S{"both 304, a full read<br/>less than 30 minutes ago?"}
//!     S -- yes --> K["the facts of the last full read<br/>(no GraphQL point)"]
//!     S -- no --> F["the full read"]
//!     F --> M{"a pull request is open?"}
//!     M -- no --> R["keep the facts and the ETags"]
//!     M -- yes --> D["keep nothing"]
//! ```
//!
//! # The limits
//!
//! - The checks of a pull request and its `mergeable` state do not change
//!   its `updated_at`. So while a pull request is open, [`Memory`] keeps
//!   nothing, and each look reads in full.
//! - A rename of a milestone changes no issue. The look keeps the
//!   milestones with the issues, so it sees the rename at its next full
//!   read.
//! - A full read comes at least each [`FULL_EVERY`], whatever the answer.
//! - A conditional read that fails, or that gives no `ETag`, is a change:
//!   the look reads in full, and keeps nothing.
//!
//! The look of the daemon and the sweep take the replies of the full read
//! ([`crate::pr::Gh::read`]). A `304` renews them
//! ([`crate::rollout::renew_open`]), so they go on to take them while the
//! wave is idle.
//!
//! While the events of the GitHub App of the repository are on, the look
//! makes no conditional read: it reads what the wakes name, and reads in
//! full at its safety look ([`crate::hooks`], #1656).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::Duration;

use anyhow::Result;

use crate::pr::{Answer, Gh};
use crate::rollout::{self, Open};

/// The longest time between two full reads, whatever GitHub answers.
pub const FULL_EVERY: Duration = Duration::from_secs(30 * 60);

/// The path of REST that gives the issue that changed last in `repo`. A
/// pull request is an issue too.
pub fn issues_path(repo: &str) -> String {
    format!("repos/{repo}/issues?state=all&sort=updated&direction=desc&per_page=1")
}

/// The path of REST that gives the pull request that changed last in
/// `repo`.
pub fn pulls_path(repo: &str) -> String {
    format!("repos/{repo}/pulls?state=all&sort=updated&direction=desc&per_page=1")
}

/// What a full read of the forge gives the look of a wave.
#[derive(Debug, Clone)]
pub struct Facts {
    /// The default branch of the repository.
    pub branch: String,
    /// The open milestones, issues and pull requests.
    pub open: Open,
}

/// What [`Memory`] keeps of one repository: the facts of its last full
/// read, the `ETag` of each conditional read from before that read, and
/// when it began.
#[derive(Debug, Clone)]
struct Kept {
    issues: String,
    pulls: String,
    at_ms: i64,
    facts: Facts,
}

/// The facts of the last full read of each repository, for the look of a
/// wave. The daemon has one ([`facts`]).
#[derive(Debug, Default)]
pub struct Memory {
    kept: Mutex<HashMap<String, Kept>>,
}

impl Memory {
    /// The facts of `repo` at `now_ms` (milliseconds since the epoch): the
    /// ones from the last full read when GitHub answers `304` to both
    /// conditional reads, no pull request was open, and that read began
    /// less than [`FULL_EVERY`] ago. Else `read` gives them. A `304` calls
    /// no GraphQL and renews the kept replies of the forge
    /// ([`rollout::renew_open`]).
    ///
    /// ```
    /// use riff::changes::{Facts, Memory};
    /// use riff::pr::Gh;
    /// use riff::rollout::Open;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let script = r#"#!/bin/sh
    /// case "$*" in
    ///   *If-None-Match*) echo 'gh: HTTP 304' >&2; exit 1 ;;
    ///   *) printf 'HTTP/2.0 200 OK\r\nEtag: "v1"\r\n\r\n[]\n' ;;
    /// esac
    /// "#;
    /// let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
    /// let memory = Memory::default();
    /// let reads = std::cell::Cell::new(0);
    /// let read = || {
    ///     reads.set(reads.get() + 1);
    ///     Ok(Facts { branch: "main".into(), open: Open::default() })
    /// };
    /// // The first look reads in full, and keeps the facts with the ETags.
    /// memory.facts(&gh, "o/r", 0, read)?;
    /// assert_eq!(reads.get(), 1);
    /// // Nothing changed: no read.
    /// memory.facts(&gh, "o/r", 60_000, read)?;
    /// assert_eq!(reads.get(), 1);
    /// // A full read at least each 30 minutes.
    /// memory.facts(&gh, "o/r", 31 * 60_000, read)?;
    /// assert_eq!(reads.get(), 2);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn facts(
        &self,
        gh: &Gh,
        repo: &str,
        now_ms: i64,
        read: impl FnOnce() -> Result<Facts>,
    ) -> Result<Facts> {
        let scope = format!("{}\u{1f}{repo}", gh.scope());
        let before = self.lock().get(&scope).cloned();
        // The answers come before the read: a change between the two reads
        // shows at the next look.
        let issues = ask(gh, &issues_path(repo), before.as_ref().map(|k| &k.issues));
        let pulls = ask(gh, &pulls_path(repo), before.as_ref().map(|k| &k.pulls));
        if let Some(kept) = &before
            && issues == Answer::Same
            && pulls == Answer::Same
            && recent(kept.at_ms, now_ms)
        {
            rollout::renew_open(gh, repo);
            return Ok(kept.facts.clone());
        }
        let facts = read()?;
        let etag = |answer: Answer, old: Option<&String>| match answer {
            Answer::Same => old.cloned(),
            Answer::Changed { etag } => etag,
        };
        let etags = (
            etag(issues, before.as_ref().map(|k| &k.issues)),
            etag(pulls, before.as_ref().map(|k| &k.pulls)),
        );
        let mut kept = self.lock();
        match etags {
            // The checks of an open pull request change no `updated_at`.
            (Some(issues), Some(pulls)) if facts.open.pulls.is_empty() => {
                kept.insert(
                    scope,
                    Kept {
                        issues,
                        pulls,
                        at_ms: now_ms,
                        facts: facts.clone(),
                    },
                );
            }
            _ => {
                kept.remove(&scope);
            }
        }
        Ok(facts)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Kept>> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The answer of GitHub to the conditional read of `path`. A fault of the
/// read is a change: the full read that follows reports its own fault.
fn ask(gh: &Gh, path: &str, etag: Option<&String>) -> Answer {
    gh.ask(path, etag.map(String::as_str))
        .unwrap_or(Answer::Changed { etag: None })
}

/// True when a full read that began at `at_ms` is less than [`FULL_EVERY`]
/// old at `now_ms`.
fn recent(at_ms: i64, now_ms: i64) -> bool {
    u64::try_from(now_ms - at_ms).is_ok_and(|ms| Duration::from_millis(ms) < FULL_EVERY)
}

static MEMORY: LazyLock<Memory> = LazyLock::new(Memory::default);

/// [`Memory::facts`] of the memory of this process, now.
pub fn facts(gh: &Gh, repo: &str, read: impl FnOnce() -> Result<Facts>) -> Result<Facts> {
    MEMORY.facts(gh, repo, crate::daemon::now_ms(), read)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::rollout::Pull;

    /// A fake `gh` that answers `304` to a conditional read that sends the
    /// `ETag` in the file `etag`, and `200` with that `ETag` to the other
    /// reads. It logs each call in `log`.
    fn fake(dir: &std::path::Path) -> Gh {
        let script = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/log"
tag=$(cat "$dir/etag")
case "$*" in
  *"If-None-Match: $tag"*) echo 'gh: HTTP 304' >&2; exit 1 ;;
  *) printf 'HTTP/2.0 200 OK\r\nEtag: %s\r\n\r\n[]\n' "$tag" ;;
esac
"#;
        std::fs::write(dir.join("etag"), "\"v1\"").unwrap();
        Gh::at(isolated::write_program(dir.join("gh"), script))
    }

    fn facts(branch: &str) -> Facts {
        Facts {
            branch: branch.into(),
            open: Open::default(),
        }
    }

    /// Runs `memory.facts` at `now_ms` with a read that counts itself and
    /// gives the branch `branch`. It gives the branch, and whether the
    /// read ran.
    fn look(memory: &Memory, gh: &Gh, now_ms: i64, branch: &str) -> (String, bool) {
        let read = Cell::new(false);
        let got = memory
            .facts(gh, "o/r", now_ms, || {
                read.set(true);
                Ok(facts(branch))
            })
            .unwrap();
        (got.branch, read.get())
    }

    const MINUTE: i64 = 60_000;

    /// A full read at least each 30 minutes, whatever GitHub answers: GitHub
    /// answers `304` all the time here.
    #[test]
    fn a_full_read_comes_at_least_each_30_minutes() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        let memory = Memory::default();
        assert_eq!(look(&memory, &gh, 0, "first"), ("first".into(), true));
        // The facts of the first read, until the 30th minute.
        for at in [MINUTE, 5 * MINUTE, 29 * MINUTE, 30 * MINUTE - 1] {
            assert_eq!(look(&memory, &gh, at, "later"), ("first".into(), false));
        }
        // The full read keeps its time from its own start.
        assert_eq!(
            look(&memory, &gh, 30 * MINUTE, "second"),
            ("second".into(), true)
        );
        assert_eq!(
            look(&memory, &gh, 59 * MINUTE, "later"),
            ("second".into(), false)
        );
        assert_eq!(
            look(&memory, &gh, 60 * MINUTE, "third"),
            ("third".into(), true)
        );
    }

    /// A milestone that is renamed changes no issue: the look sees it at the
    /// full read.
    #[test]
    fn a_rename_of_a_milestone_shows_at_the_next_full_read() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        let memory = Memory::default();
        look(&memory, &gh, 0, "main");
        let renamed = Cell::new(false);
        let got = memory
            .facts(&gh, "o/r", 10 * MINUTE, || {
                renamed.set(true);
                Ok(Facts {
                    branch: "main".into(),
                    open: Open {
                        milestones: vec![serde_json::from_str(r#"{"title":"Wave 2"}"#).unwrap()],
                        ..Open::default()
                    },
                })
            })
            .unwrap();
        assert!(!renamed.get());
        assert!(got.open.milestones.is_empty());
        memory
            .facts(&gh, "o/r", 31 * MINUTE, || {
                renamed.set(true);
                Ok(facts("main"))
            })
            .unwrap();
        assert!(renamed.get());
    }

    #[test]
    fn a_change_of_either_list_gets_the_full_read_and_the_new_etag() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        let memory = Memory::default();
        look(&memory, &gh, 0, "a");
        assert!(!look(&memory, &gh, MINUTE, "b").1);
        // A new `ETag`: both lists answer `200` in this fake.
        std::fs::write(dir.path().join("etag"), "\"v2\"").unwrap();
        assert_eq!(look(&memory, &gh, 2 * MINUTE, "c"), ("c".into(), true));
        // The look kept the new `ETag`.
        assert_eq!(look(&memory, &gh, 3 * MINUTE, "d"), ("c".into(), false));
    }

    /// The checks of a pull request and its `mergeable` state do not
    /// change its `updated_at`: with a pull request open, `304` is no
    /// answer.
    #[test]
    fn an_open_pull_request_gets_the_full_read_at_each_look() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        let memory = Memory::default();
        let busy = |branch: &str| Facts {
            open: Open {
                pulls: vec![Pull::default()],
                ..Open::default()
            },
            ..facts(branch)
        };
        for at in 0..3 {
            let read = Cell::new(false);
            memory
                .facts(&gh, "o/r", at * MINUTE, || {
                    read.set(true);
                    Ok(busy("main"))
                })
                .unwrap();
            assert!(read.get(), "look {at}");
        }
        // The pull request merged: the first look without it reads in full,
        // and the next looks are idle.
        assert!(look(&memory, &gh, 3 * MINUTE, "main").1);
        assert!(!look(&memory, &gh, 4 * MINUTE, "main").1);
        // A pull request opens: GitHub says `304` still, and the look
        // reads in full and forgets.
        std::fs::write(dir.path().join("etag"), "\"v2\"").unwrap();
        let read = Cell::new(false);
        memory
            .facts(&gh, "o/r", 5 * MINUTE, || {
                read.set(true);
                Ok(busy("main"))
            })
            .unwrap();
        assert!(read.get());
        assert!(look(&memory, &gh, 6 * MINUTE, "main").1);
    }

    #[test]
    fn a_conditional_read_that_fails_gets_the_full_read_and_keeps_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        let memory = Memory::default();
        look(&memory, &gh, 0, "a");
        // `gh` fails the call: a fault of the forge, not a `304`.
        let broken = Gh::at(isolated::write_program(
            dir.path().join("gh"),
            "#!/bin/sh\necho 'HTTP 500: boom' >&2\nexit 1\n",
        ));
        assert_eq!(look(&memory, &broken, MINUTE, "b"), ("b".into(), true));
        assert_eq!(look(&memory, &broken, 2 * MINUTE, "c"), ("c".into(), true));
    }

    #[test]
    fn a_read_that_fails_is_the_error_of_the_look() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        let memory = Memory::default();
        let error = memory
            .facts(&gh, "o/r", 0, || anyhow::bail!("no branch"))
            .unwrap_err();
        assert_eq!(error.to_string(), "no branch");
        // Nothing was kept: the next look reads.
        assert!(look(&memory, &gh, MINUTE, "main").1);
    }

    #[test]
    fn each_repository_has_its_own_facts() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path());
        let memory = Memory::default();
        look(&memory, &gh, 0, "a");
        // Another repository has no facts yet, and the first has its own.
        let read = Cell::new(false);
        let other = memory
            .facts(&gh, "o/other", MINUTE, || {
                read.set(true);
                Ok(facts("trunk"))
            })
            .unwrap();
        assert!(read.get());
        assert_eq!(other.branch, "trunk");
        assert_eq!(look(&memory, &gh, 2 * MINUTE, "b"), ("a".into(), false));
    }

    /// The paths are the ones of the design.
    #[test]
    fn the_paths_ask_for_the_newest_item_that_changed() {
        assert_eq!(
            issues_path("o/r"),
            "repos/o/r/issues?state=all&sort=updated&direction=desc&per_page=1"
        );
        assert_eq!(
            pulls_path("o/r"),
            "repos/o/r/pulls?state=all&sort=updated&direction=desc&per_page=1"
        );
    }

    /// The conditional reads cost no GraphQL point, and a `304` is no fault
    /// in the count of the calls.
    #[test]
    fn the_conditional_reads_cost_no_graphql_point() {
        let dir = tempfile::tempdir().unwrap();
        let calls = std::sync::Arc::new(crate::ghcalls::Calls::default());
        let gh = fake(dir.path()).by("wave").with_calls(calls.clone());
        let memory = Memory::default();
        look(&memory, &gh, 0, "a");
        look(&memory, &gh, MINUTE, "a");
        let shown = calls.shown(crate::daemon::now_ms());
        assert_eq!(shown.total(), (4, 0), "{:?}", shown.rows);
        assert!(shown.rows.iter().all(|row| row.kind.starts_with("api ")));
    }
}
