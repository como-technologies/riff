//! Let riff run the wave (#719): the riff daemon starts one session for
//! each free item of the current wave, one for each pull request that
//! waits for a verify (decisions 8 and 34 of #699), one to rebase
//! each pull request with a conflict with `main` (#1082), and one to fix
//! each pull request with a failed check. It tells the lead of a pull
//! request that no session can fix (#279).
//!
//! # Design
//!
//! `riff wave run` in a clone asks the daemon to run the wave of that
//! repository ([`Wave`]). The daemon keeps each wave in [`WAVES_FILE`]
//! in its folder, and starts its loop again after its own restart.
//! `riff wave stop` ends the loop: the daemon starts no new session
//! there, and the sessions that run go on.
//!
//! Each item has one session: it starts, does the item and ends
//! (01M4Z60B7NW4GHKGCYZZDG26R6). A verify is an item too: a new session
//! with a clone at the head commit of the pull request. No session
//! waits idle, and no session clears its context.
//!
//! ```mermaid
//! flowchart TD
//!     A["wait for room (crate::room)"] --> S["sweep: the clone of each item whose work is over goes"]
//!     S --> L["look: the pause, the claims, the holds,<br/>the free work of gh, the full check of each waiting pull request"]
//!     L --> P{"pick"}
//!     P -- "an item, and a fact of the host fails" --> W
//!     P -- "no work" --> W["give the room back; wait for an end,<br/>LOOK_EVERY, or IDLE_LOOK_EVERY"] --> A
//!     P -- "an item" --> J["a job: claim, clone, start, wait for the end,<br/>the steps after the end, release"]
//!     J -.-> A
//! ```
//!
//! # The look and the limit of GitHub
//!
//! The token of a person has 5,000 GraphQL points an hour for all its
//! sessions, looks and leads (#1549, [`crate::ghcalls`]). A wave that has
//! no work, no session and no change since its last look waits
//! [`IDLE_LOOK_EVERY`] for the next look, not [`LOOK_EVERY`]. The end of a
//! session or of a job wakes it at once.
//! A paused wave calls no `gh`, so it keeps [`LOOK_EVERY`]. The look of the
//! loop asks GitHub first whether anything changed, with two conditional
//! reads that cost no GraphQL point. When both answer `304`, no pull
//! request is open and the last full read is less than 30 minutes old, it
//! reads nothing else ([`crate::changes`], 01M6JFJQD4H4GG0H0TFSW5G1PD).
//! Else it reads the forge as it is. The look of the daemon at the pull
//! requests and at `main` takes the reads of the loop
//! ([`crate::pr::Gh::read`]), and waits as long, so an idle wave makes one
//! read of its issues and its pull requests in each wait. A `304` renews
//! those reads.
//!
//! ```mermaid
//! flowchart TD
//!     L["a look"] --> Q{"no work, no session,<br/>the same facts as the last look?"}
//!     Q -- yes --> I["wait IDLE_LOOK_EVERY"]
//!     Q -- no --> N["wait LOOK_EVERY"]
//!     I -- "a session or a job ends" --> L
//!     N -- "a session or a job ends" --> L
//! ```
//!
//! # When the limit of GitHub runs out
//!
//! A call of `gh` that fails on a limit of GitHub starts a wait for the
//! reset of that resource ([`crate::ghcalls`], #1557). Until the reset the
//! `gh` of the daemon makes no call of the resource, so the loops need no
//! rule of their own:
//!
//! - The loop looks at the wait before it admits to the room (`limit_pause`).
//!   While a limit waits, it makes no look, no sweep and no report of a kept
//!   result. It tells the lead one time for each outage, with the key
//!   [`ghcalls::Wait::key`], and sleeps until the reset. A look that ran into
//!   the limit does the same at once.
//! - The look of the daemon (`watch`) sleeps until the reset.
//! - The watches make no look ([`crate::watches::Lookout::limit`]).
//! - The end step of a verify (`after_verify`) keeps its result when the
//!   report fails on a limit ([`crate::kept`]), and the loop reports it
//!   after the reset ([`World::report_kept`]). The result of a head that
//!   changed is dropped. A build that ends then cannot read its pull
//!   request, and the lead gets a note ([`text::wave_build_limit`]).
//!
//! ```mermaid
//! flowchart TD
//!     F["a look fails:<br/>the limit is out"] --> P{"limit_pause"}
//!     P -- "a wait" --> T["tell the lead, one time for each outage"]
//!     T --> S["sleep until the reset:<br/>no look, no sweep, no call"]
//!     S --> R["report_kept: each kept result of a verify"]
//!     R --> L["look"]
//!     E["a verify ends"] --> Q{"the report fails on the limit?"}
//!     Q -- yes --> K["keep the result, note to the lead"]
//!     K -.-> R
//! ```
//!
//! # The notices to the lead
//!
//! A wave can run on more hosts, and the daemon of each host tells the
//! lead ([`World::tell`]). A notice that comes from the facts of the
//! repository has a key from those facts, so the lead gets it one time.
//! `riff-server` drops a post with the key of a message that it keeps
//! from the same user in the thread (01M5TKG2K1AYK1KE7C2AA2W9DS,
//! 01M6FZY6NABXGXVRYF6W1509XE). A notice about one host has no key: each
//! host tells it.
//!
//! | Notice | Key | The lead gets it |
//! |---|---|---|
//! | An issue with no `Done when:` line ([`text::wave_no_done_when`]) | [`no_done_when_key`] | one time |
//! | A pull request that is stuck ([`text::wave_stuck`]) | [`Notice::post_key`] | one time |
//! | "Wave N is complete" ([`crate::watches::wave_complete_key`]) | `wave-complete-N` | one time |
//! | The GitHub limit is out ([`text::wave_limit_waits`]) | [`ghcalls::Wait::key`] | one time for each outage |
//! | The host waits for a fact, the pull of the image failed, a job failed, an item is spent | none | from each host |
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as the daemon of pangolin
//!     participant B as the daemon of thelio
//!     participant S as riff-server
//!     participant L as the lead
//!     A->>S: post, key no-done-when-12
//!     S->>L: the message
//!     B->>S: post, key no-done-when-12
//!     Note over S: it keeps this key: it drops the post
//! ```
//!
//! # The pick
//!
//! [`pick`] takes the first item of the free work of the look
//! ([`look_on`]): the free work of the rollout of the lead
//! ([`crate::rollout::free_work`]) with the fixes ([`fixes`]). A fix
//! item of a broken `main` first
//! ([`crate::rollout::BROKEN_MAIN`], 01M51QEYS538XA58KDRY844T52), then
//! the rebase of each pull request with a conflict (see "A conflict with
//! main"), then the fix of each pull request with a failed check (see
//! "A pull request that cannot merge"), then the pull requests that wait
//! for a verify, then the free items of the current wave. It never
//! starts:
//!
//! - an item while the riff or the repository is paused;
//! - a held item (01M4ZV3B6M7PZRJ39XTXBZP3ZH);
//! - a tracker issue, its build or its verify: it has no code of its
//!   own ([`rollout::Issue::tracker`], 01M5ND16V2SVN8A9K7W90VC6X9);
//! - a build of an issue whose body has no line that starts with
//!   `Done when:` ([`rollout::Issue::has_done_when`],
//!   01M5R0HEP1MXWYY643F1N1DWEM): a session stops at once without it. The
//!   lead gets one message for each such issue ([`Look::no_done_when`],
//!   [`text::wave_no_done_when`]). The wave takes the issue at its next
//!   look after the line comes. A verify and a rebase do not change;
//! - an item that a session claims, or that a job of the wave runs;
//! - the verify of an item while a session holds the item: its author
//!   still works. The verify is a new session: an author never verifies
//!   its own item (01M4ZV3B6NKPEZ5FNE27X95GF0);
//! - a verify before the full check of the head commit is done: each
//!   check that the rules of the default branch require, but `riff/verify`
//!   ([`repo_setup::full_check`], 01M5QYDHN27RC5SRHAB56PFKK0), has a run,
//!   and each newest run is completed ([`pr::full_done`]). A check with no
//!   run is not done, also when another check failed (#1575);
//! - a verify of a head with a failed check: the fix of the checks comes
//!   first (see "A pull request that cannot merge", #1575);
//! - an item that started [`STARTS`] times in this run of the wave. The
//!   lead gets one message for it. A start whose claim riff-server
//!   refused for a pause does not count (01M558EYCXPFW8CJV3XJ7HK3NR).
//!   `riff wave run` on the wave clears the starts
//!   (01M57A7A48A70Y7PTD95A3MBYC);
//! - the rebase of a head commit that started one time in this run of
//!   the wave ([`REBASES`], 01M57FJ3WWMQSDBFRRHXN3SMFQ);
//! - the fix of the failed checks of a head commit that started one time
//!   in this run of the wave ([`CHECKS`]);
//! - any item while a fact of the host fails ([`World::host`]): the
//!   sign-in of `gh`, or the sign-in of `claude` with less than
//!   [`run::MIN_MINUTES`] left. No item can start then, so no start
//!   counts. The wave waits, and looks again at [`LOOK_EVERY`]. The lead
//!   gets one message when the wave starts to wait, and `riff daemon
//!   status` shows the reason (01M57A7A1W2K9440WZA1QZ3M9T);
//! - any item for one look after podman failed to pull the
//!   session image of the wave (`Daemon::failed_pull`). Such a start
//!   does not count. `riff daemon status` shows the image and the reason
//!   in the line of the wave until a pull works. The lead gets one message
//!   for each image, with the fix: make the package public, or run
//!   `podman login REGISTRY` on each host of the wave
//!   (01M5SMSB3H1E9NXMBDN8ZDJ6CD).
//!
//! # A conflict with main
//!
//! `main` is the default branch of the repository, also in the start text
//! of a verify and in the look of the Gate. The wave reads it from the forge
//! ([`Look::branch`], [`pr::default_branch`], 01M5R56GN3DC09GAPAF69GENEZ),
//! and keeps the read for [`pr::Gh::SLOW`] (#1549), so `master` and `trunk`
//! work the same, and `waves.json` has no branch. A branch that does not
//! read is the error of the look: the wave starts no work at that look.
//!
//! A pull request of an item with auto-merge on and a conflict with
//! `main` ([`rollout::Pull::conflict`]) cannot merge, also after its
//! verify passed. No session holds the item then: the session of a
//! wave ends after its pull request. So the wave starts a build session
//! for the item, with the start text of a rebase
//! ([`text::wave_rebase_start`]): rebase the branch on the default branch, resolve
//! the conflict, run the checks, and push to the same pull request
//! (01M57FJ3WWMQSDBFRRHXN3SMFQ). A new full check and a new verify of the new
//! head follow. The verify is a new session, as each verify is.
//!
//! ```mermaid
//! flowchart LR
//!     C["auto-merge on, CONFLICTING,<br/>no failed verify"] --> R["a build: Work::Rebase<br/>at the head commit"]
//!     R -- "a push" --> V["a new head: a new full check, then a new verify"]
//!     R -- "no push" --> L["a message to the lead"]
//! ```
//!
//! [`fixes`] puts each such item in the free work after the fix items
//! of a broken `main`, before each other item. The wave starts no
//! verify of a head with a conflict: a pass counts only for its commit,
//! and the rebase makes a new one. A pull request whose verify failed is
//! free work as a build already, with the failed result, until a new
//! verify request for the same head comes ([`reasked`], #1157): then it
//! waits for a verify again.
//!
//! A pull request whose `mergeable` is `UNKNOWN` has no verify in the
//! free work either ([`rollout::Pull::unknown`], #1464). GitHub gives
//! `UNKNOWN` after a push, and for each open pull request after `main`
//! moved, until it has looked at them. The pull request may have a
//! conflict then: a verify would test a head that the rebase replaces,
//! and a verify that ends with no result starts again. The verify waits
//! for the next look. It reads `MERGEABLE` (the verify) or
//! `CONFLICTING` (the rebase).
//!
//! Each head commit with a conflict starts one session in a run of the
//! wave: [`Work::key`] names the rebase by the item and the head. A pause
//! or a fault of the host takes the start back, as for each item.
//! `riff wave run` clears the starts, so a person can start the rebase
//! of the same head again. When the session ends with the head that had
//! the conflict, the lead gets a message in place of the verify request
//! (01M57FJ3Z8AFDY9G0KTAYZJESY).
//!
//! # The look
//!
//! Beside its loop, each wave has a look ([`crate::look::look_wave`],
//! 01M5QVH3MAEPPJN9PPVDX6N2EY). It takes no room, so a host that is
//! full still sees a broken `main`. Each minute it reads the open issues
//! and pull requests with `gh` and the claims on riff-server, and posts
//! as the person ([`World::gh`], [`World::claims`], [`World::post`]):
//!
//! ```mermaid
//! flowchart LR
//!     S["riff wave run"] --> L["the loop: the work"]
//!     S --> W["the look, each minute"]
//!     W --> P["a pull request that stops:<br/>a message to its holder or the lead"]
//!     W --> M["the full check of main:<br/>a message, the fix item, a note"]
//!     X["riff wave stop"] --> E["the loop and the look end"]
//! ```
//!
//! # A pull request that cannot merge
//!
//! A conflict is one reason that a pull request cannot merge by itself
//! ([`rollout::Stuck`], #279). The wave reads more from `gh` at each look,
//! and nobody has to remember to check:
//!
//! ```mermaid
//! flowchart TD
//!     P["an open pull request,<br/>the verify passed or waits"] --> C{"a check of the head failed?<br/>(statusCheckRollup)"}
//!     C -- "yes, an item" --> R{"a run of<br/>GitHub Actions?"}
//!     R -- "yes" --> B["a build: Work::Checks at the head commit,<br/>with the names of the checks"]
//!     R -- "no, another app" --> O["a message to the lead:<br/>the check, its app and its link"]
//!     C -- "yes, a release" --> M["a message to the lead, with the checks"]
//!     C -- "no" --> A{"auto-merge off?"}
//!     A -- "yes" --> N["a message to the lead,<br/>with the command gh pr merge --auto --squash N"]
//!     A -- "no" --> E{"reviewDecision<br/>REVIEW_REQUIRED?"}
//!     E -- "yes" --> H["a message to the lead,<br/>with the command gh pr review N --approve"]
//!     E -- "no" --> K{"a run of a fork waits<br/>for approval?"}
//!     K -- "yes" --> Y["a message to the lead for each run,<br/>with the command gh api -X POST .../approve"]
//!     K -- "no" --> D{"BLOCKED, no check open,<br/>no decision?"}
//!     D -- "yes" --> H2["a message to the lead:<br/>probably a review (an unattributed author)"]
//!     D -- "no" --> W["it merges by itself"]
//!     B -- "a push" --> V["a new full check and a new verify of the new head"]
//!     B -- "no push, the check still failed" --> L["a message to the lead"]
//!     B -- "no push, the check runs again" --> T["a note to the lead"]
//! ```
//!
//! - **A failed check** (01M5QW2KVAS05Q72630RNW47G0).
//!   [`rollout::Pull::verify`] reads only the status `riff/verify`. A
//!   check of the rollup that failed ([`rollout::Check::failed`]), for
//!   example the `Gate` or `Hygiene`, is no free work for a verify: the
//!   verify of that head can only fail (#1575). The job `Gate` has no run
//!   until the other jobs end, so `Hygiene` can fail while the `Gate` has
//!   none. The fix starts at once, before or after a verify.
//!   The wave starts a build session for the item with the start text
//!   [`text::wave_checks_start`]: it names the checks, tells to read the
//!   failed run, and to fix the code and push to the same pull request.
//!   For a known flake it allows one run of the job again. The rollup
//!   does not say which check a rule requires, so each failed check
//!   counts, but only its newest run (#1540,
//!   [`rollout::Pull::failed_checks`]): an old failed run does not count
//!   when a newer run of the same check passed. A check that was
//!   cancelled or skipped did not fail.
//! - **A failed check of another app** (#1427). The start text reads the
//!   log with `gh run view` and runs the job again with `gh run rerun`.
//!   That works for a run of GitHub Actions only
//!   ([`rollout::Check::actions`]). A repository whose CI is CircleCI or
//!   Buildkite has check runs with no run of Actions. The wave starts no
//!   build for such a check ([`rollout::Pull::failed_others`]). The lead
//!   gets one message for each check and head commit, with the name of
//!   the check, its app and its link ([`rollout::Stuck::Other`]). A pull
//!   request with both kinds gets the build for the runs of Actions, and
//!   the message for each other check.
//! - **Auto-merge off** (01M5QW2M1M5QZJXSP0BY6XG118). A pull request
//!   whose verify passed and that has no auto-merge never merges: nothing
//!   turns it on. A conflict gets no rebase then
//!   ([`rollout::Pull::conflict`] needs auto-merge). The lead gets one
//!   message with the reason and the command
//!   (`gh pr merge --auto --squash N`).
//! - **A review** (01M6FZ141Y0J8G2AYX0G5HY1CX). A pull request whose verify passed, with
//!   auto-merge on, can wait for a person. GitHub says `reviewDecision`
//!   `REVIEW_REQUIRED` when a rule of the branch asks for a review
//!   ([`rollout::Review::Required`]). It says nothing when the count of
//!   reviews is 0 and the rule "require extra approval for unattributed
//!   changes" asks one review for a commit with an author who has no
//!   GitHub account. Then the merge state is `BLOCKED`, no check failed or
//!   goes, and no run waits ([`rollout::Review::Blocked`],
//!   [`rollout::Pull::settled`]). No session approves a pull request. The
//!   lead gets one message with the reason and the command
//!   (`gh pr review N --approve`).
//! - **A run that waits for approval** (01M6FZ144M0BTEDWNB9V4MGWDD). A run of GitHub Actions of
//!   a pull request from a fork waits in `action_required` for a
//!   maintainer. It makes no check run, so GitHub counts its required
//!   check as not reported, and the newest run of a workflow counts. The look
//!   reads the runs of the head ([`rollout::runs_waiting`]) for a pull
//!   request that GitHub blocks with no check open
//!   ([`rollout::Pull::settled`]). The lead gets one message for each run,
//!   with the run and the command
//!   (`gh api -X POST repos/OWNER/REPO/actions/runs/RUN/approve`).
//! - **A release** (01M5QW2M4WX53N29SHRV0S32WM). No session builds a
//!   release (the label `release`, or a branch `release-…`). A failed
//!   check of a release pull request gets a message to the lead, with the
//!   names of the checks, at any state of its verify.
//!
//! Each stuck state comes one time. A message is keyed by the pull
//! request, its head commit and its reason (and the run, for a run that
//! waits for approval); the build of a failed check
//! by the item and the head ([`Work::key`]). So a new head is a new
//! state. A message is not repeated while the wave runs. A build of a
//! failed check runs one time for a head in a run of the wave
//! ([`CHECKS`]), and `riff wave run` clears the starts. When a pull
//! request has a conflict and a failed check, the rebase comes first:
//! the new head has new checks.
//!
//! The session may not push: for a flake it runs the job again
//! (01M5QW2KYFM4C287D9NMSPH8TA). After its end the daemon reads the pull
//! request. When the head is the same and a check still failed, the lead
//! gets a message ([`text::wave_checks_not_fixed`]); else the lead gets a
//! note ([`text::wave_checks_rerun`]). A push gets the verify request of
//! the new head, as each build does.
//!
//! `riff top` and `riff wave list` show the item of a stuck pull request
//! in the group `stuck`, with the reason, for example
//! `stuck: #250 (checks)`, while no session claims it
//! ([`riff_client::waves::Board`], 01M5QW2M8FPBMMF4S0D8PC39AB).
//!
//! # The merge queue
//!
//! A repository can have a merge queue on `main`. GitHub then merges each
//! pull request through it: it builds a merge group of `main` and the
//! pull request, runs the required checks on the group, and merges the
//! group when they pass. A pull request needs no rebase because `main`
//! moved. The look reads the queue with one `gh api graphql` call for all
//! open pull requests ([`rollout::queue_of`]); a repository with no queue,
//! and a call that fails, give [`rollout::Queue::Out`] for each of them
//! (01M5TKPNDET56VR8WZ74FSR71F).
//!
//! ```mermaid
//! flowchart TD
//!     P["an open pull request of an item"] --> Q{"rollout::Queue"}
//!     Q -- "In" --> N["no rebase, no verify, no message"]
//!     Q -- "Removed, a conflict" --> R["a build: Work::Rebase at the head commit,<br/>also with auto-merge off"]
//!     Q -- "Removed, other" --> M["a message to the lead: Stuck::Dequeued<br/>with the reason of GitHub"]
//!     Q -- "Out" --> O["as without a queue"]
//! ```
//!
//! A removal counts while it is the last event of the pull request on
//! GitHub, before any push: a push makes a new head, and the new head has
//! not been in the queue (01M5TKPNGG27Z1SQKTA296D9H7). The message comes one
//! time for each head ([`Notice::key`]). The rebase text tells the
//! session to turn auto-merge on again after its push
//! ([`text::wave_rebase_start`]).
//!
//! # A job
//!
//! A job does the steps of the host of `riff run` itself
//! ([`crate::run`]), with the sign-in of the person on this machine: a
//! new worker session on riff-server, the claim, the keep-alive,
//! the clone, the start text, and the start of the container in the
//! room that the loop took. The [`Record`] of the session holds the
//! [`Job`], so a new daemon goes on with the keep-alive and does the
//! steps after the end ([`crate::daemon::Daemon::recover`]).
//!
//! When riff-server refuses the claim, the job ends the new worker
//! session at once, so no idle worker with no item stays
//! (01M558EYCXPFW8CJV3XJ7HK3NR). A pause can come between the look and
//! the claim. Its refusal (`paused`) is no fault: the lead gets no
//! message, and the start does not count. Each other refusal is a failed
//! start: the lead gets a message, and the start counts.
//!
//! After the end ([`Live::finish`]):
//!
//! - A build sends a verify request for the open pull request of its
//!   branch to the lead, as `riff run` does. When the issue has the
//!   label [`crate::rollout::NO_VERIFY`], or a lead holds `verify-ITEM`,
//!   the lead gets a note with the reason in place of the verify request
//!   ([`crate::run::no_verify`], 01M51KH0H7J35PEKMHP5KQB84T).
//! - A verify reads the result from the last turn of `claude` in
//!   `events.jsonl`: its first line is `PASS` or `FAIL` ([`verdict`]).
//!   The daemon reports it as `riff verify` does: the comment on the
//!   pull request, the status `riff/verify` of the commit, and the
//!   result to the holder of the item and to the lead.
//! - Then the daemon releases the item and ends the worker session.
//!
//! # The clones
//!
//! The clone of a build stays until the work of its branch is over
//! (01M4Z81ARSTAK8HR96S084SDFD). Before each look, the sweep of the
//! daemon removes the folder of each session of the repository that does
//! not run, when the pull request of its branch merged or closed, or its
//! issue is closed, and its clone holds no work that is not on `origin`
//! ([`crate::daemon::sweep`], 01M53NJ5V27BCHDNHK2THQQZZQ,
//! 01M53S7WAXD46YPN0DR96XSTMG). For a merged pull request with `Refs #N`,
//! it first adds the comment `Merged in #PR (COMMIT)` to the issue, as
//! the verifier of a worker does, so the item is not free work again.
//! The folder of a verify that did not pass goes at its end. The daemon
//! also sweeps with no wave.
//!
//! A job holds `Daemon::in_job` of its item from before the clone of
//! its session to the end of `finish`, so the sweep keeps its folder
//! while a step after the end reads it (01M53S7WDPQ2GWHJWJ6SZXZ5PN). Each
//! `gh` call of the sweep has a time limit, so a `gh` that never ends
//! stops no wave (01M53S7WGM9DQV2DXFHBRSMTP8).
//!
//! # The world
//!
//! The loop reaches the forge and riff-server through a [`World`]. The
//! daemon uses [`Live`]: `gh` and riff-server. A test gives a fake
//! world, with the fake runtime ([`crate::runtime::Fake`]). The daemon
//! also keeps the session of the lead alive through it, and follows the
//! wakes of a session for `riff relay watch` ([`World::wake`]).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::wire::{
    HoldInfo, Kind, PullState, RiffState, StartReason, WORKER_ALIVE_EVERY, Wake,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::daemon::sweep::{self, Fate, InJob};
use crate::daemon::{Daemon, Ended, Out, PullFailed, Record, Say, Start};
use crate::pr::Verdict;
use crate::room::Slot;
use crate::run::{self, Facts};
use crate::runtime::Runtime;
use crate::session_setup::Setup;
use crate::{
    changes, ghcalls, identity, kept, look, pr, release_config, repo_setup, rollout, session_image,
    text, usage,
};
use riff_client::api::{self, Api};

/// The file of the waves that the daemon runs, in its folder.
pub const WAVES_FILE: &str = "waves.json";

/// How often a wave looks for free work when nothing ends.
pub const LOOK_EVERY: Duration = Duration::from_secs(60);

/// How often an idle wave looks (#1549, [`crate::ghcalls`]): no session of
/// the wave runs, no work is free, and the look saw the same facts as the
/// look before ([`look_wait`]). A session that ends, and a job that ends,
/// wake the wave at once.
pub const IDLE_LOOK_EVERY: Duration =
    Duration::from_secs(LOOK_EVERY.as_secs() * crate::watches::IDLE_FACTOR as u64);

/// The most starts of one item in one run of a wave.
pub const STARTS: u32 = 3;

/// The most starts of the rebase of one head commit in one run of a
/// wave (01M57FJ3WWMQSDBFRRHXN3SMFQ).
pub const REBASES: u32 = 1;

/// The most starts of the fix of the failed checks of one head commit in
/// one run of a wave (#279).
pub const CHECKS: u32 = 1;

/// A wave that the daemon runs: a repository, and where it is on this
/// machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wave {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// The main clone of the repository on this machine.
    pub main: PathBuf,
    /// The riff-server of the riff of the repository.
    pub server: String,
    /// The name of this host in the URI of a session.
    pub host: String,
}

impl Wave {
    /// The place of a session of the wave in the worktree `worktree`.
    ///
    /// ```
    /// use riff::wave::Wave;
    ///
    /// let wave = Wave {
    ///     repo: "o/r".into(),
    ///     main: "/src/r".into(),
    ///     server: "http://127.0.0.1:1".into(),
    ///     host: "thelio".into(),
    /// };
    /// assert_eq!(riff::identity::place_text(&wave.place(Some("issue-12"))?), "thelio/o/r#issue-12");
    /// assert_eq!(wave.thread()?.to_string(), "o/r");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn place(&self, worktree: Option<&str>) -> Result<Place> {
        let worktree = worktree.map(|w| format!("#{w}")).unwrap_or_default();
        identity::place_from_text(&format!("{}/{}{worktree}", self.host, self.repo))
    }

    /// The thread of the repository.
    pub fn thread(&self) -> Result<ThreadName> {
        self.place(None)?
            .default_thread()
            .ok_or_else(|| anyhow!("{}", text::RUN_NO_REPO))
    }
}

/// The waves in [`WAVES_FILE`] of the folder `dir` of the daemon, by
/// repository. With no file, none.
pub fn read_waves(dir: &Path) -> Result<BTreeMap<String, Wave>> {
    let path = dir.join(WAVES_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("cannot read the waves in {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Writes `waves` to [`WAVES_FILE`] in the folder `dir`, in place of
/// the old file.
pub fn write_waves(dir: &Path, waves: &BTreeMap<String, Wave>) -> Result<()> {
    let path = dir.join(WAVES_FILE);
    let tmp = path.with_extension("json.new");
    std::fs::write(&tmp, serde_json::to_vec_pretty(waves)?)
        .with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))
}

/// `riff wave run`: asks the daemon in the folder `dir` to run the
/// current wave of the repository of the place `here`, on the
/// riff-server `server` (01M4ZV3B6J89KJQ0F14YWTZMJQ).
pub async fn ask_run(dir: &Path, here: &Place, server: &str) -> Result<()> {
    let repo = ask(dir, here, server, |line| println!("{line}")).await?;
    println!("{}", text::wave_asked(&repo));
    Ok(())
}

/// [`ask_run`] with each line of the steps to `show`, for a caller with
/// no stdout of its own, as `riff mcp` (01M53NGVE839TRH7XB7CDPQNZE). It
/// gives the repository, as `OWNER/REPO`.
///
/// Before the ask of the wave, the daemon must have the session image
/// that the repository pins: podman runs, and the image is on the
/// machine, or the daemon pulls it ([`crate::daemon::image`]). When it
/// cannot, the daemon gets no ask of the wave: a machine that cannot
/// start a container runs no wave (01M558EYAJFZK7Y822NVWGBS9H). The git
/// steps of the pin block, so they run on a thread that may block: they
/// stop no task of `riff mcp`.
pub async fn ask(dir: &Path, here: &Place, server: &str, show: impl FnMut(&str)) -> Result<String> {
    ask_with(dir, identity::working_dir()?, here, server, show).await
}

/// [`ask`] for the clone of the folder `work`: the daemon of a host
/// asks it for the lead of another host, in its main clone
/// ([`crate::daemon::remote`]).
pub async fn ask_with(
    dir: &Path,
    work: PathBuf,
    here: &Place,
    server: &str,
    mut show: impl FnMut(&str),
) -> Result<String> {
    if here.default_thread().is_none() {
        bail!("{}", text::RUN_NO_REPO);
    }
    let (main, image, lines) = blocking(move || {
        let (main, _) = run::main_clone(&work)?;
        let mut lines = Vec::new();
        let image = session_image::merged(&main, |line| lines.push(line.to_owned()))?;
        Ok((main, image.to_string(), lines))
    })
    .await?;
    for line in &lines {
        show(line);
    }
    crate::daemon::image(dir, &image, &mut show)
        .await
        .map_err(|e| anyhow!("{}", text::wave_no_container(&format!("{e:#}"))))?;
    let repo = here.repo_text();
    let wave = Wave {
        repo: repo.clone(),
        main,
        server: server.to_owned(),
        host: here.host().to_owned(),
    };
    crate::daemon::ask_done(dir, crate::daemon::Ask::Wave { wave }, show).await?;
    Ok(repo)
}

/// True when the daemon of `status` runs the wave of `repo`
/// (`OWNER/REPO`).
///
/// ```
/// let status: riff::daemon::Status = serde_json::from_value(serde_json::json!({
///     "pid": 7, "build": "3.0.0", "since_ms": 0, "dir": "/d", "sessions": [],
///     "waves": [{"repo": "o/r", "word": "no work"}],
/// }))?;
/// assert!(riff::wave::runs(&status, "o/r"));
/// assert!(!riff::wave::runs(&status, "o/other"));
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn runs(status: &crate::daemon::Status, repo: &str) -> bool {
    status.waves.iter().any(|w| w.repo == repo)
}

/// `riff wave stop`: asks the daemon in the folder `dir` to start no new
/// session for the wave of the repository of the place `here`.
pub async fn ask_stop(dir: &Path, here: &Place) -> Result<()> {
    if here.default_thread().is_none() {
        bail!("{}", text::RUN_NO_REPO);
    }
    let repo = here.repo_text();
    let ask = crate::daemon::Ask::WaveStop { repo: repo.clone() };
    crate::daemon::ask_done(dir, ask, |line| println!("{line}")).await?;
    println!("{}", text::wave_stops(&repo));
    Ok(())
}

/// What a session of a wave does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Work {
    /// The build of the issue on its branch.
    Build {
        /// The issue.
        issue: u64,
    },
    /// The verify of the pull request `pull` of the issue, at the commit
    /// `commit`.
    Verify {
        /// The issue.
        issue: u64,
        /// The pull request.
        pull: u64,
        /// The head commit that the verify tests.
        commit: String,
    },
    /// A build that rebases the pull request `pull` of the issue on
    /// `main`: its head `commit` has a conflict with `main`
    /// (01M57FJ3WWMQSDBFRRHXN3SMFQ).
    Rebase {
        /// The issue.
        issue: u64,
        /// The pull request.
        pull: u64,
        /// The head commit with the conflict.
        commit: String,
    },
    /// A build that fixes the failed checks of the pull request `pull`
    /// of the issue: its verify passed, and the checks `checks` of its
    /// head `commit` failed (#279).
    Checks {
        /// The issue.
        issue: u64,
        /// The pull request.
        pull: u64,
        /// The head commit with the failed checks.
        commit: String,
        /// The names of the checks that failed.
        checks: Vec<String>,
    },
}

impl Work {
    /// The work item: `issue-N` or `verify-issue-N`. A rebase and a fix
    /// of checks are builds: their item is `issue-N`.
    ///
    /// ```
    /// use riff::wave::Work;
    ///
    /// assert_eq!(Work::Build { issue: 12 }.item(), "issue-12");
    /// let verify = Work::Verify { issue: 12, pull: 40, commit: "1a2b".into() };
    /// assert_eq!(verify.item(), "verify-issue-12");
    /// let rebase = Work::Rebase { issue: 12, pull: 40, commit: "1a2b".into() };
    /// assert_eq!(rebase.item(), "issue-12");
    /// let checks = Work::Checks { issue: 12, pull: 40, commit: "1a2b".into(), checks: vec!["Gate".into()] };
    /// assert_eq!(checks.item(), "issue-12");
    /// ```
    pub fn item(&self) -> String {
        match self {
            Work::Build { issue } | Work::Rebase { issue, .. } | Work::Checks { issue, .. } => {
                format!("issue-{issue}")
            }
            Work::Verify { issue, .. } => format!("verify-issue-{issue}"),
        }
    }

    /// The role of the session that does this work: a verify is a
    /// verifier, each build is a worker (01M6FZXZ9267YKMH97E6A2Y2EP).
    ///
    /// ```
    /// use riff::permissions::Role;
    /// use riff::wave::Work;
    ///
    /// assert_eq!(Work::Build { issue: 12 }.role(), Role::Worker);
    /// let verify = Work::Verify { issue: 12, pull: 40, commit: "1a2b".into() };
    /// assert_eq!(verify.role(), Role::Verifier);
    /// let rebase = Work::Rebase { issue: 12, pull: 40, commit: "1a2b".into() };
    /// assert_eq!(rebase.role(), Role::Worker);
    /// ```
    pub fn role(&self) -> crate::permissions::Role {
        match self {
            Work::Verify { .. } => crate::permissions::Role::Verifier,
            Work::Build { .. } | Work::Rebase { .. } | Work::Checks { .. } => {
                crate::permissions::Role::Worker
            }
        }
    }

    /// The issue.
    pub fn issue(&self) -> u64 {
        match self {
            Work::Build { issue }
            | Work::Verify { issue, .. }
            | Work::Rebase { issue, .. }
            | Work::Checks { issue, .. } => *issue,
        }
    }

    /// The name of the starts of this work in a run of the wave: its
    /// item. A rebase counts for its head commit, so that each conflict
    /// starts one session (01M57FJ3WWMQSDBFRRHXN3SMFQ). The fix of checks
    /// counts for its head commit too, apart from the rebase of the same
    /// head (#279).
    ///
    /// ```
    /// use riff::wave::Work;
    ///
    /// assert_eq!(Work::Build { issue: 12 }.key(), "issue-12");
    /// let rebase = |commit: &str| Work::Rebase { issue: 12, pull: 40, commit: commit.into() };
    /// assert_eq!(rebase("1a2b").key(), "issue-12@1a2b");
    /// assert_ne!(rebase("1a2b").key(), rebase("5e6f").key());
    /// let checks = Work::Checks { issue: 12, pull: 40, commit: "1a2b".into(), checks: vec![] };
    /// assert_eq!(checks.key(), "issue-12@1a2b:checks");
    /// assert_ne!(checks.key(), rebase("1a2b").key());
    /// ```
    pub fn key(&self) -> String {
        match self {
            Work::Rebase { commit, .. } => format!("{}@{commit}", self.item()),
            Work::Checks { commit, .. } => format!("{}@{commit}:checks", self.item()),
            _ => self.item(),
        }
    }

    /// The most starts of this work in a run of the wave: [`STARTS`],
    /// [`REBASES`] for a rebase, or [`CHECKS`] for a fix of checks.
    pub fn most(&self) -> u32 {
        match self {
            Work::Rebase { .. } => REBASES,
            Work::Checks { .. } => CHECKS,
            _ => STARTS,
        }
    }
}

/// The job of a session that the daemon started for a wave. It is in the
/// [`Record`] of the session, with no secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    /// The wave.
    pub wave: Wave,
    /// The work.
    pub work: Work,
}

/// A pull request that waits for a verify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// The pull request.
    pub pull: u64,
    /// Its head commit.
    pub commit: String,
    /// True when the full check of the head is done: each required check
    /// has a run, and each newest run is completed, with a success or not
    /// ([`pr::full_done`], 01M5QYDHN27RC5SRHAB56PFKK0).
    pub gate_done: bool,
}

/// A pull request with auto-merge on and a conflict with `main`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The pull request.
    pub pull: u64,
    /// Its head commit, which has the conflict.
    pub commit: String,
}

/// A pull request of an item whose verify passed and whose checks
/// failed (#279).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    /// The pull request.
    pub pull: u64,
    /// Its head commit, whose checks failed.
    pub commit: String,
    /// The names of the checks that failed.
    pub checks: Vec<String>,
}

/// A pull request that no session can fix: the lead gets one message
/// for it (#279).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// The pull request.
    pub pull: u64,
    /// Its branch.
    pub branch: String,
    /// Its head commit.
    pub commit: String,
    /// Why it cannot merge.
    pub stuck: rollout::Stuck,
}

impl Notice {
    /// The key of the message: the pull request, its head and its
    /// reason. A message with the same key goes one time. A failed check
    /// of another app than GitHub Actions has a message for each check, and a
    /// run that waits for approval one for each run.
    ///
    /// ```
    /// use riff::rollout::{OtherCheck, Review, Run, Stuck};
    /// use riff::wave::Notice;
    ///
    /// let notice = |commit: &str, stuck| Notice { pull: 40, branch: "b".into(), commit: commit.into(), stuck };
    /// assert_eq!(notice("1a2b", Stuck::AutoMergeOff).key(), "#40@1a2b:auto-merge off");
    /// assert_ne!(notice("1a2b", Stuck::AutoMergeOff).key(), notice("5e6f", Stuck::AutoMergeOff).key());
    /// let other = |name: &str| Stuck::Other(OtherCheck { name: name.into(), app: None, link: None });
    /// assert_eq!(notice("1a2b", other("ci")).key(), "#40@1a2b:checks:ci");
    /// assert_ne!(notice("1a2b", other("ci")).key(), notice("1a2b", other("lint")).key());
    /// let review = notice("1a2b", Stuck::Review(Review::Required));
    /// assert_eq!(review.key(), "#40@1a2b:review");
    /// let run = |id| Stuck::Approve(Run { id, name: "Hygiene".into(), link: None });
    /// assert_eq!(notice("1a2b", run(7)).key(), "#40@1a2b:approve run:7");
    /// assert_ne!(notice("1a2b", run(7)).key(), notice("1a2b", run(8)).key());
    /// ```
    pub fn key(&self) -> String {
        let key = format!("#{}@{}:{}", self.pull, self.commit, self.stuck.word());
        match &self.stuck {
            rollout::Stuck::Other(check) => format!("{key}:{}", check.name),
            rollout::Stuck::Approve(run) => format!("{key}:{}", run.id),
            _ => key,
        }
    }

    /// The key of the post of the message (01M6FZY6NABXGXVRYF6W1509XE):
    /// `stuck-` and [`Notice::key`]. The wave of each host makes the same
    /// key for the same facts, and riff-server drops the second post.
    ///
    /// ```
    /// use riff::rollout::Stuck;
    /// use riff::wave::Notice;
    ///
    /// let notice = Notice { pull: 40, branch: "b".into(), commit: "1a2b".into(), stuck: Stuck::AutoMergeOff };
    /// assert_eq!(notice.post_key(), "stuck-#40@1a2b:auto-merge off");
    /// ```
    pub fn post_key(&self) -> String {
        format!("stuck-{}", self.key())
    }
}

/// The key of the post of the message for an issue with no `Done when:`
/// line (01M6FZY6NABXGXVRYF6W1509XE, [`text::wave_no_done_when`]). The wave
/// of each host makes the same key for the same issue, and riff-server
/// drops the second post.
///
/// ```
/// assert_eq!(riff::wave::no_done_when_key(12), "no-done-when-12");
/// ```
pub fn no_done_when_key(issue: u64) -> String {
    format!("no-done-when-{issue}")
}

/// The pull requests that cannot merge by themselves, by what the wave
/// does for them (#279).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fixes {
    /// The pull requests to rebase, by issue.
    pub conflicts: BTreeMap<u64, Conflict>,
    /// The pull requests whose checks failed after a pass, by issue:
    /// a build for each.
    pub failed: BTreeMap<u64, Failed>,
    /// The pull requests that the lead hears of.
    pub notices: Vec<Notice>,
}

/// The facts of a wave at one look.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Look {
    /// True when the riff or the repository is paused.
    pub paused: bool,
    /// The free work, in order ([`fixes`]): `verify-issue-N` and
    /// `issue-N`. An `issue-N` in `fixes.conflicts` is a rebase, and one
    /// in `fixes.failed` the fix of its checks.
    pub free: Vec<String>,
    /// The items of the repository that a session claims.
    pub claims: BTreeSet<String>,
    /// The held items of the repository.
    pub holds: BTreeSet<String>,
    /// The pull requests that wait for a verify, by issue.
    pub pulls: BTreeMap<u64, Waiting>,
    /// The pull requests that cannot merge by themselves.
    pub fixes: Fixes,
    /// The issues that would be free work but for the missing line
    /// `Done when:` in their body
    /// ([`rollout::Open::no_done_when`]). The lead hears of each once.
    pub no_done_when: BTreeSet<u64>,
    /// The default branch of the repository on the forge, for example
    /// `main` or `master`. The look reads it from the forge (a read of the
    /// last [`pr::Gh::SLOW`] serves), and keeps it in no file
    /// (01M5R56GN3DC09GAPAF69GENEZ).
    pub branch: String,
}

/// The issue of `verify-issue-N`.
fn verify_issue(item: &str) -> Option<u64> {
    item.strip_prefix("verify-issue-")?.parse().ok()
}

/// The next work of a wave, from the facts of `look`, the items that its
/// jobs run (`running`), and the number of starts of each item
/// (`starts`). See "The pick" in the design.
///
/// ```
/// use std::collections::{BTreeMap, BTreeSet};
/// use riff::wave::{Conflict, Failed, Fixes, Look, Waiting, Work, pick};
///
/// let look = Look {
///     free: vec!["verify-issue-9".into(), "issue-12".into(), "issue-13".into()],
///     holds: ["issue-12".to_owned()].into(),
///     claims: ["issue-9".to_owned()].into(),
///     pulls: [(9, Waiting { pull: 40, commit: "1a2b".into(), gate_done: true })].into(),
///     ..Look::default()
/// };
/// let none = BTreeSet::new();
/// // The author of 9 still holds it, and 12 is held.
/// assert_eq!(pick(&look, &none, &BTreeMap::new()), Some(Work::Build { issue: 13 }));
/// let look = Look { claims: BTreeSet::new(), ..look };
/// assert_eq!(
///     pick(&look, &none, &BTreeMap::new()),
///     Some(Work::Verify { issue: 9, pull: 40, commit: "1a2b".into() })
/// );
/// assert_eq!(pick(&Look { paused: true, ..look }, &none, &BTreeMap::new()), None);
///
/// // The pull request of 14 has a conflict: one rebase for its head.
/// let conflict = Conflict { pull: 54, commit: "5e6f".into() };
/// let look = Look { free: vec!["issue-14".into()], fixes: Fixes { conflicts: [(14, conflict)].into(), ..Fixes::default() }, ..Look::default() };
/// let rebase = Work::Rebase { issue: 14, pull: 54, commit: "5e6f".into() };
/// assert_eq!(pick(&look, &none, &BTreeMap::new()), Some(rebase.clone()));
/// assert_eq!(pick(&look, &none, &[(rebase.key(), 1)].into()), None);
/// // The starts of the build do not stop the rebase.
/// assert_eq!(pick(&look, &none, &[("issue-14".to_owned(), 3)].into()), Some(rebase));
///
/// // The checks of the pull request of 15 failed after a pass: one fix for its head.
/// let failed = Failed { pull: 55, commit: "7a8b".into(), checks: vec!["Gate".into()] };
/// let look = Look { free: vec!["issue-15".into()], fixes: Fixes { failed: [(15, failed)].into(), ..Fixes::default() }, ..Look::default() };
/// let fix = Work::Checks { issue: 15, pull: 55, commit: "7a8b".into(), checks: vec!["Gate".into()] };
/// assert_eq!(pick(&look, &none, &BTreeMap::new()), Some(fix.clone()));
/// assert_eq!(pick(&look, &none, &[(fix.key(), 1)].into()), None);
/// ```
pub fn pick(
    look: &Look,
    running: &BTreeSet<String>,
    starts: &BTreeMap<String, u32>,
) -> Option<Work> {
    if look.paused {
        return None;
    }
    let busy = |item: &str| running.contains(item) || look.claims.contains(item);
    look.free.iter().find_map(|item| {
        if look.holds.contains(item) || busy(item) {
            return None;
        }
        let work = if let Some(issue) = verify_issue(item) {
            // The author of the item still works on it.
            if busy(&format!("issue-{issue}")) {
                return None;
            }
            let waiting = look.pulls.get(&issue).filter(|w| w.gate_done)?;
            Work::Verify {
                issue,
                pull: waiting.pull,
                commit: waiting.commit.clone(),
            }
        } else {
            let issue = item.strip_prefix("issue-")?.parse().ok()?;
            match (
                look.fixes.conflicts.get(&issue),
                look.fixes.failed.get(&issue),
            ) {
                (Some(conflict), _) => Work::Rebase {
                    issue,
                    pull: conflict.pull,
                    commit: conflict.commit.clone(),
                },
                (None, Some(failed)) => Work::Checks {
                    issue,
                    pull: failed.pull,
                    commit: failed.commit.clone(),
                    checks: failed.checks.clone(),
                },
                (None, None) => Work::Build { issue },
            }
        };
        (starts.get(&work.key()).copied().unwrap_or(0) < work.most()).then_some(work)
    })
}

/// True when the item `issue-N` of the free work of `look` is a fix of
/// a pull request: a rebase, or the fix of its checks.
fn fix_of(look: &Look, item: &str) -> bool {
    item.strip_prefix("issue-")
        .and_then(|n| n.parse().ok())
        .is_some_and(|n| {
            look.fixes.conflicts.contains_key(&n) || look.fixes.failed.contains_key(&n)
        })
}

/// The items of the free work of `look` that reached [`STARTS`]. A fix
/// of a pull request is not one: it starts one time for each head.
fn spent<'a>(
    look: &'a Look,
    starts: &'a BTreeMap<String, u32>,
) -> impl Iterator<Item = &'a String> {
    look.free
        .iter()
        .filter(|item| !fix_of(look, item))
        .filter(|item| starts.get(*item).copied().unwrap_or(0) >= STARTS)
}

/// A future of a [`World`].
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The wait before the next connection of the wakes of a session.
const WAKE_RETRY: Duration = Duration::from_secs(1);

/// What the merged `riff.toml` of the main clone gives to the sessions of a
/// wave: the pinned image, and the env and the volumes
/// (01M5SMVVC4GQS310CVK3H9CMQ7).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pinned {
    /// The pinned image, as podman pulls it.
    pub image: String,
    /// The env and the volumes.
    pub setup: Setup,
}

/// The forge and the riff of the waves: [`Live`], or a fake in a test.
pub trait World: Send + Sync {
    /// The facts of the wave now.
    fn look<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Look>>;

    /// The session image of the repository of the wave, with its env and
    /// volumes: the merged pin of its main clone
    /// ([`session_image::merged_config`]).
    fn pinned<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Pinned>>;

    /// The facts of the host that no item changes (as `riff run` checks them):
    /// the fault of the first that fails (01M57A7A1W2K9440WZA1QZ3M9T).
    fn host<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<(), run::HostFault>>;

    /// The steps before the start: the checks, the claim, the clone and
    /// the start text. `None` when another session holds the item, or
    /// the item is held. Each other refusal of the claim, for example
    /// `paused`, is the error ([`api::Refusal`]), and the new session
    /// ends. After the claim, an error releases the item.
    fn begin<'a>(
        &'a self,
        wave: &'a Wave,
        work: &'a Work,
        pinned: &'a Pinned,
    ) -> BoxFuture<'a, Result<Option<Start>>>;

    /// Goes on with the claim of the session of `record` after a new
    /// start of the daemon: its keep-alive.
    fn resume(&self, record: &Record);

    /// The steps after the end of the session of `record`. It gives a
    /// line for each step.
    fn finish<'a>(&'a self, record: &'a Record, ended: Ended)
    -> BoxFuture<'a, Result<Vec<String>>>;

    /// The marks of this machine and the `gh` of the repository `repo`:
    /// the daemon reports the tokens of each container session with them
    /// ([`usage::Meter::session_ended`]). `None` counts nothing.
    fn meter(&self, _repo: &str) -> Option<usage::Meter> {
        None
    }

    /// The folder of the leave marks of this machine
    /// ([`crate::local::marks`]). The daemon posts no refusal of auto
    /// mode of a session with a mark in it
    /// ([`crate::daemon::Daemon::post_denials`]). `None` has no mark.
    fn marks(&self) -> Option<PathBuf> {
        None
    }

    /// The fate of the folder of a session of the issue `issue` of the
    /// repository `repo`, from the forge ([`Fate::of`]).
    fn fate<'a>(&'a self, repo: &'a str, issue: u64) -> BoxFuture<'a, Result<Fate>>;

    /// Deletes each branch `riff-kept/*` of `repo` that is older than 30
    /// days, and gives their names ([`crate::daemon::sweep::delete_old_kept_on`]).
    fn delete_old_kept<'a>(&'a self, _repo: &'a str) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// Adds the comment `Merged in #PULL (COMMIT)` to the issue `issue` of
    /// `repo`, when it has none ([`crate::daemon::sweep`]).
    fn merged_in<'a>(
        &'a self,
        repo: &'a str,
        issue: u64,
        pull: u64,
        commit: &'a str,
    ) -> BoxFuture<'a, Result<()>>;

    /// Tells the lead of the wave `body`, as a message. The post has `key`,
    /// when there is one: riff-server drops a post with the key of a kept
    /// message of the person in the thread of the wave
    /// (01M5TKG2K1AYK1KE7C2AA2W9DS). A notice that the wave of each host
    /// makes from the same facts has a key, so the lead hears it one time.
    fn tell<'a>(&'a self, wave: &'a Wave, body: &'a str, key: Option<&'a str>)
    -> BoxFuture<'a, ()>;

    /// The `gh` of the repository `repo` for the look
    /// ([`crate::look::look_wave`]).
    fn gh(&self, repo: &str) -> Arc<pr::Gh>;

    /// The items that a session of the repository of `wave` claims now.
    fn claims<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<HashSet<String>>>;

    /// Posts `body` of `kind` as the person of the wave: to the session
    /// that claims `item`, or to the lead of the person when `item` is
    /// `None`. The error is a post that did not go out. A post with a
    /// `key` is dropped by riff-server when a kept message of the person
    /// has the key in the thread of the wave (01M5TKG2K1AYK1KE7C2AA2W9DS).
    fn post<'a>(
        &'a self,
        wave: &'a Wave,
        item: Option<&'a str>,
        body: &'a str,
        kind: Kind,
        key: Option<&'a str>,
    ) -> BoxFuture<'a, Result<()>>;

    /// Writes `body` in riff chat as the person of the lead of `record`,
    /// and wakes no session: the lead cannot act on it
    /// (01M5NJ87YEHHGJBANM7CRYV56Z). The line has the kind `riff`: it is
    /// no word of the person (01M5TKMVB7CF0QNAWXEVGEQETJ).
    fn chat<'a>(&'a self, record: &'a Record, body: &'a str) -> BoxFuture<'a, ()>;

    /// Posts `body` as a message to the lead of `record`, as the person
    /// of the lead: the lead wakes and acts on it. The default posts
    /// nothing (01M5SNV7H3739PXW73H14XM4B8).
    fn note_lead<'a>(&'a self, _record: &'a Record, _body: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    /// Ends the session of `record` on riff-server, and stops its
    /// keep-alive: the end of the lead.
    fn leave<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, ()>;

    /// The [`Start`] of the lead of `record` for a start again on the riff of
    /// this daemon: the same session and folder, `claude --resume`
    /// (`wave` is the wave of the repository, when the daemon runs one: it gives
    /// the main clone, [`crate::lead::again`], 01M5TC2919TB9XPF42YFNJBP79).
    fn lead_again<'a>(
        &'a self,
        wave: Option<&'a Wave>,
        record: &'a Record,
    ) -> BoxFuture<'a, Result<Start>>;

    /// Frees the item of the session of `record` on riff-server at once,
    /// ends that session, and stops its keep-alive: a person ends the
    /// session ([`crate::daemon::Ask::End`], 01M579Z9VRD12FRK6DSYVKDHQB).
    fn free<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<()>>;

    /// Waits for the next wake of the session of `record` on riff-server,
    /// and gives its line, as `riff watch --once` prints it.
    fn wake<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<String>>;

    /// Sends each wake of the session of `record` on riff-server to `to`,
    /// until `to` closes: the wakes of a worker and of a verifier
    /// ([`crate::daemon`], 01M5792DP1KZR2XGSCMRWDHCGZ).
    fn wakes<'a>(
        &'a self,
        record: &'a Record,
        to: mpsc::UnboundedSender<Wake>,
    ) -> BoxFuture<'a, ()>;

    /// Tells the sender of `wake` that the session of `record` ended its
    /// work and does not read the message (01M5792DRA12JD032F3H8JVC0Z).
    fn refuse<'a>(&'a self, record: &'a Record, wake: &'a Wake) -> BoxFuture<'a, ()>;

    /// Reports each kept result of a verify of `wave` ([`crate::kept`],
    /// #1557), and gives a line for each. The default reports none.
    fn report_kept<'a>(&'a self, _wave: &'a Wave) -> BoxFuture<'a, Vec<String>> {
        Box::pin(async { Vec::new() })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The items that the daemon runs for the wave `wave`: its jobs before
/// the start (`taken`), and its live sessions.
fn running<R: Runtime + 'static>(
    daemon: &Daemon<R>,
    wave: &Wave,
    taken: &Mutex<BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut items = lock(taken).clone();
    items.extend(
        daemon
            .records()
            .into_iter()
            .filter(|r| r.job.as_ref().is_some_and(|j| j.wave.repo == wave.repo))
            .map(|r| r.live.item),
    );
    items
}

/// The `gh` of the loop and the jobs of a wave for `repo`: its calls are
/// the calls of the caller `wave` in the count of the daemon
/// ([`crate::ghcalls`]).
fn forge(repo: &str) -> pr::Gh {
    pr::Gh::for_repo(repo).by("wave")
}

/// How long the wave waits after the look `look` (#1549). The wave is idle
/// when the look has the same facts as the `last` look, the pick found no
/// work (`picked` is false), and no session or job of the wave runs
/// (`running` is false): it waits [`IDLE_LOOK_EVERY`]. Else it waits
/// [`LOOK_EVERY`]. A paused wave calls no `gh`, and it starts at once on a
/// resume, so it is never idle. An end of a session or of a job wakes the
/// wave in each case.
///
/// ```
/// use riff::wave::{IDLE_LOOK_EVERY, LOOK_EVERY, Look, look_wait};
///
/// let look = Look { free: vec!["issue-12".into()], ..Look::default() };
/// // The first look, and a look that changed, wait one minute.
/// assert_eq!(look_wait(&look, None, false, false), LOOK_EVERY);
/// let other = Look { free: vec!["issue-13".into()], ..Look::default() };
/// assert_eq!(look_wait(&other, Some(&look), false, false), LOOK_EVERY);
/// // Nothing changed, no work, nothing runs: five minutes.
/// assert_eq!(look_wait(&look, Some(&look), false, false), IDLE_LOOK_EVERY);
/// // Work, or a session that runs, keeps the minute.
/// assert_eq!(look_wait(&look, Some(&look), true, false), LOOK_EVERY);
/// assert_eq!(look_wait(&look, Some(&look), false, true), LOOK_EVERY);
/// // A paused wave looks each minute.
/// let paused = Look { paused: true, ..Look::default() };
/// assert_eq!(look_wait(&paused, Some(&paused), false, false), LOOK_EVERY);
/// ```
pub fn look_wait(look: &Look, last: Option<&Look>, picked: bool, running: bool) -> Duration {
    match !picked && !running && !look.paused && last == Some(look) {
        true => IDLE_LOOK_EVERY,
        false => LOOK_EVERY,
    }
}

/// How long until riff may call again after the wait `wait` of a limit of
/// GitHub.
pub fn until(wait: &ghcalls::Wait) -> Duration {
    let left = wait.until_ms() - crate::daemon::now_ms();
    Duration::from_millis(u64::try_from(left).unwrap_or(0))
}

/// What a wave does while a limit of GitHub is out (#1557): how long it
/// waits for the reset, and its word. The lead hears of an outage one
/// time: `told` holds the key of the last message ([`ghcalls::Wait::key`]),
/// and a new wait in the same outage has the same key (#1581). `None` when
/// no limit is out.
async fn limit_pause(
    world: &dyn World,
    wave: &Wave,
    told: &mut Option<String>,
) -> Option<(Duration, String)> {
    let wait = world.gh(&wave.repo).limit_wait()?;
    let key = wait.key();
    if told.as_ref() != Some(&key) {
        let body = text::wave_limit_waits(&wave.repo, &wait);
        world.tell(wave, &body, Some(&key)).await;
        *told = Some(key);
    }
    Some((until(&wait), text::wave_limit_word(&wait)))
}

/// The loop of the wave `wave` in `daemon`, until the daemon stops it.
/// `taken` holds the items of its jobs before the start, `starts` the
/// starts of each item, and `word` what it did last.
pub(crate) async fn run<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    wave: Wave,
    taken: Arc<Mutex<BTreeSet<String>>>,
    starts: Starts,
    word: Arc<Mutex<String>>,
) {
    let say = |text: String| *lock(&word) = text;
    // The spent items that the lead heard of.
    let mut told: BTreeSet<String> = BTreeSet::new();
    // The stuck pull requests that the lead heard of ([`Notice::key`]).
    let mut noticed: BTreeSet<String> = BTreeSet::new();
    // The issues with no `Done when:` line that the lead heard of.
    let mut bare: BTreeSet<u64> = BTreeSet::new();
    // True while the wave waits for a fact of the host: the lead heard
    // of it.
    let mut waits = false;
    // The facts of the last look, to find an idle wave.
    let mut last: Option<Look> = None;
    // The key of the limit of GitHub that the lead heard of.
    let mut limited: Option<String> = None;
    loop {
        let world = daemon.world();
        // While a limit of GitHub is out, the wave makes no look, no sweep
        // and no report (#1557).
        if let Some((pause, word)) = limit_pause(&*world, &wave, &mut limited).await {
            say(word);
            tokio::select! {
                () = tokio::time::sleep(pause) => {}
                () = daemon.changed() => {}
            }
            continue;
        }
        for line in world.report_kept(&wave).await {
            println!("{line}");
        }
        let slot = daemon
            .room()
            .admit(
                &text::wave_slot(&wave.repo),
                || daemon.live_count(),
                |full| say(text::wave_full(full)),
            )
            .await;
        for line in daemon.sweep_news(Some(&wave.repo)).await {
            println!("{line}");
        }
        let mut wait = LOOK_EVERY;
        let next = match world.look(&wave).await {
            Ok(look) => {
                let spent: Vec<String> = spent(&look, &lock(&starts)).cloned().collect();
                // `riff wave run` cleared the starts of these items.
                told.retain(|item| lock(&starts).get(item).is_some_and(|n| *n >= STARTS));
                for item in spent {
                    if told.insert(item.clone()) {
                        let reason = daemon
                            .last_end(&item)
                            .unwrap_or_else(|| text::WAVE_END_UNKNOWN.to_owned());
                        let body = text::wave_spent(&item, STARTS, &reason);
                        world.tell(&wave, &body, None).await;
                    }
                }
                // An issue that got the line, or left the free work, can come
                // again.
                bare.retain(|n| look.no_done_when.contains(n));
                for n in &look.no_done_when {
                    if bare.insert(*n) {
                        let key = no_done_when_key(*n);
                        let body = text::wave_no_done_when(*n);
                        world.tell(&wave, &body, Some(&key)).await;
                    }
                }
                for notice in &look.fixes.notices {
                    if noticed.insert(notice.key()) {
                        let body = text::wave_stuck(
                            &wave.repo,
                            notice.pull,
                            &notice.branch,
                            &look.branch,
                            &notice.commit,
                            &notice.stuck,
                        );
                        world.tell(&wave, &body, Some(&notice.post_key())).await;
                    }
                }
                let next = pick(&look, &running(&daemon, &wave, &taken), &lock(&starts));
                // A fault of the host stops each item: the wave waits,
                // and no start counts (01M57A7A1W2K9440WZA1QZ3M9T).
                let fault = match &next {
                    Some(_) => world.host(&wave).await.err(),
                    None => None,
                };
                match &fault {
                    Some(fault) if !waits => {
                        waits = true;
                        let body = text::wave_host_waits(&wave.repo, &fault.wave_text());
                        world.tell(&wave, &body, None).await;
                    }
                    Some(_) => {}
                    None if next.is_some() => waits = false,
                    None => {}
                }
                // A pull of the image that failed: the wave waits for the next
                // try, and no start counts (01M5SMSB3H1E9NXMBDN8ZDJ6CD).
                let pulling = fault.is_none()
                    && next.is_some()
                    && daemon
                        .failed_pull(&wave.repo)
                        .is_some_and(|pull| pull.waits);
                say(match (&next, &fault) {
                    _ if look.paused => text::WAVE_PAUSED.into(),
                    (Some(_), Some(fault)) => text::wave_host_word(&fault.wave_text()),
                    (Some(work), None) => text::wave_started(&work.item()),
                    (None, _) => text::WAVE_NO_WORK.into(),
                });
                wait = look_wait(
                    &look,
                    last.as_ref(),
                    next.is_some(),
                    !running(&daemon, &wave, &taken).is_empty(),
                );
                last = Some(look);
                next.filter(|_| fault.is_none() && !pulling)
            }
            Err(e) => {
                say(text::wave_no_look(&format!("{e:#}")));
                last = None;
                // A look that ran into the limit tells the lead now, and the
                // wave waits for the reset.
                if let Some((pause, word)) = limit_pause(&*world, &wave, &mut limited).await {
                    say(word);
                    wait = pause;
                }
                None
            }
        };
        match next {
            Some(work) => {
                *lock(&starts).entry(work.key()).or_default() += 1;
                lock(&taken).insert(work.item());
                let (taken, starts) = (taken.clone(), starts.clone());
                let (daemon, wave) = (daemon.clone(), wave.clone());
                tokio::spawn(job(daemon, wave, work, slot, taken, starts, false));
            }
            None => {
                drop(slot);
                tokio::select! {
                    () = tokio::time::sleep(wait) => {}
                    () = daemon.changed() => {}
                }
            }
        }
    }
}

/// The look of the wave `wave` in `daemon`, until the daemon stops it
/// (01M5QVH3MAEPPJN9PPVDX6N2EY): [`crate::look::look_wave`] at once, then
/// each [`Daemon::look_every`]. It takes no room: a host that is full
/// still sees a broken `main`. A look that fails prints its fault one
/// time until the fault changes.
pub(crate) async fn watch<R: Runtime + 'static>(daemon: Arc<Daemon<R>>, wave: Wave) {
    let mut watch = look::Watch::default();
    watch.accepts = daemon.look_every();
    let mut last_error = None;
    loop {
        let world = daemon.world();
        // The look makes no call while a limit of GitHub is out (#1557). The
        // loop of the wave tells the lead.
        if let Some(wait) = world.gh(&wave.repo).limit_wait() {
            tokio::time::sleep(until(&wait)).await;
            continue;
        }
        // A quiet look of a wave with no session: the next look comes later
        // (#1549). The loop of the wave reads the same issues and pull
        // requests, so this look takes its reply when the reply is not older
        // than the wait and one more look.
        let quiet = match look::look_wave(&*world, &wave, &mut watch).await {
            Ok(told) => {
                last_error = None;
                !told
            }
            Err(e) => {
                let e = format!("{e:#}");
                if last_error.as_ref() != Some(&e) {
                    eprintln!("riff daemon: {}: {}", wave.repo, text::wave_no_look(&e));
                }
                last_error = Some(e);
                false
            }
        };
        let every = daemon.look_every();
        let idle = quiet && !has_session(&daemon, &wave);
        let wait = if idle {
            every * crate::watches::IDLE_FACTOR
        } else {
            every
        };
        watch.accepts = wait + every;
        tokio::time::sleep(wait).await;
    }
}

/// True when a session of the wave `wave` runs in `daemon`.
fn has_session<R: Runtime + 'static>(daemon: &Daemon<R>, wave: &Wave) -> bool {
    daemon
        .records()
        .iter()
        .any(|r| r.job.as_ref().is_some_and(|j| j.wave.repo == wave.repo))
}

/// The starts of each item in one run of a wave, shared by its loop and
/// its jobs. `riff wave run` on a wave that runs clears them
/// (01M57A7A48A70Y7PTD95A3MBYC).
pub(crate) type Starts = Arc<Mutex<BTreeMap<String, u32>>>;

/// True when `error` is a refusal of riff-server for a pause: the riff
/// or the repository is paused.
fn paused(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<api::Refusal>()
        .is_some_and(|refusal| refusal.code.as_deref() == Some("paused"))
}

/// Takes back one start of `work` in `starts`.
fn take_back(starts: &Starts, work: &Work) {
    if let Some(count) = lock(starts).get_mut(&work.key()) {
        *count = count.saturating_sub(1);
    }
}

/// One job of the wave `wave`: the work `work` in the room of `slot`. A
/// claim that riff-server refused for a pause takes back its start in
/// `starts`, and tells the lead nothing (01M558EYCXPFW8CJV3XJ7HK3NR). So
/// does a fault of the host that came after the look of the loop
/// (01M57A7A1W2K9440WZA1QZ3M9T): the next look waits for it.
///
/// A job that the lead asked for (`asked`: `riff run` in the container
/// of the lead, #1338) has no next look. So each case that stops its
/// start tells the lead why.
pub(crate) async fn job<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    wave: Wave,
    work: Work,
    slot: Slot,
    taken: Arc<Mutex<BTreeSet<String>>>,
    starts: Starts,
    asked: bool,
) {
    let item = work.item();
    let held = daemon.in_job(&item);
    let no_start = match start_job(&daemon, &wave, &work, slot).await {
        Ok(Some((record, ended))) => {
            finish(daemon.clone(), record, ended, held).await;
            None
        }
        Ok(None) => {
            println!("{}", text::wave_not_claimed(&wave.repo, &item));
            Some(text::RUN_NOT_CLAIMED.to_owned())
        }
        Err(e) if paused(&e) => {
            println!("{}", text::wave_claim_paused(&wave.repo, &item));
            take_back(&starts, &work);
            Some(text::RUN_PAUSED.to_owned())
        }
        Err(ref e) if let Some(fault) = e.downcast_ref::<run::HostFault>() => {
            let why = fault.wave_text();
            println!("{}", text::wave_host_fault(&wave.repo, &item, &why));
            take_back(&starts, &work);
            Some(why)
        }
        Err(ref e) if let Some(pull) = e.downcast_ref::<PullFailed>() => {
            let error = format!("{e:#}");
            eprintln!("riff daemon: {}: {item}: {error}", wave.repo);
            daemon.note_end(&item, text::wave_no_start(&error));
            take_back(&starts, &work);
            if daemon.pull_failed(&wave.repo, pull) && !asked {
                let body = text::wave_pull_failed(&wave.repo, &pull.image, &pull.reason);
                daemon.world().tell(&wave, &body, None).await;
            }
            Some(error)
        }
        Err(e) => {
            let error = format!("{e:#}");
            eprintln!("riff daemon: {}: {item}: {error}", wave.repo);
            daemon.note_end(&item, text::wave_no_start(&error));
            if !asked {
                let body = text::wave_job_failed(&item, &error);
                daemon.world().tell(&wave, &body, None).await;
            }
            Some(error)
        }
    };
    if let (true, Some(why)) = (asked, no_start) {
        daemon
            .world()
            .tell(&wave, &text::run_job_failed(&item, &why), None)
            .await;
    }
    lock(&taken).remove(&item);
    daemon.tell_changed();
}

/// The steps of a job up to the end of its session: the image that the
/// wave pins, the steps before the start, and the session. `None` when
/// the claim failed. Hold [`Daemon::in_job`] of the item from before the
/// call to the end of [`finish`]: the sweep then keeps the folder.
async fn start_job<R: Runtime + 'static>(
    daemon: &Arc<Daemon<R>>,
    wave: &Wave,
    work: &Work,
    slot: Slot,
) -> Result<Option<(Record, Ended)>> {
    let world = daemon.world();
    let pinned = world.pinned(wave).await?;
    daemon
        .have_image(&pinned.image, |line| println!("{line}"))
        .await?;
    daemon.pull_worked(&wave.repo);
    daemon.check_tools(wave, &pinned.image).await;
    let Some(start) = world.begin(wave, work, &pinned).await? else {
        return Ok(None);
    };
    let job = Job {
        wave: wave.clone(),
        work: work.clone(),
    };
    let (out, said) = Out::keep();
    let ended = tokio::spawn(ended_of(said));
    let record = daemon
        .launch(start, Some(job), Some(slot), None, &out)
        .await;
    drop(out);
    let ended = ended.await.unwrap_or(Ended {
        code: None,
        failed: true,
    });
    Ok(Some((record, ended)))
}

/// The end of a session from the lines that the daemon says to one of
/// its followers. A failed start, or no end, is a failed session.
async fn ended_of(mut said: mpsc::UnboundedReceiver<Say>) -> Ended {
    let failed = Ended {
        code: None,
        failed: true,
    };
    while let Some(say) = said.recv().await {
        match say {
            Say::Ended(ended) => return ended,
            Say::Failed { error } => {
                eprintln!("riff daemon: {error}");
                return failed;
            }
            _ => {}
        }
    }
    failed
}

/// Waits for the end of the session of `record` that a new daemon took
/// up, on the lines `said`, and does the steps after the end, in the job
/// `held` of its item.
pub(crate) async fn after<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    record: Record,
    said: mpsc::UnboundedReceiver<Say>,
    held: InJob,
) {
    let ended = ended_of(said).await;
    finish(daemon, record, ended, held).await;
}

/// The steps after the end of the session of `record`, as it `ended`.
/// It gives the line of each step. The job `held` of the item ends after
/// the last step: a step reads the folder of the session, so the sweep
/// keeps it to then (01M53S7WDPQ2GWHJWJ6SZXZ5PN).
pub(crate) async fn finish<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    record: Record,
    ended: Ended,
    held: InJob,
) -> Vec<String> {
    // The reason of the end, for the lead that reads "started N times".
    let end = crate::ends::End::of(&record, ended, &crate::run::Scrub::new([]));
    daemon.note_end(&record.live.item, end.reason());
    let lines = daemon.world().finish(&record, ended).await;
    drop(held);
    daemon.tell_changed();
    lines.unwrap_or_else(|e| {
        let error = format!("{e:#}");
        eprintln!("riff daemon: {}: {error}", record.live.name);
        vec![error]
    })
}

/// The text of the last result of `claude` in the file `events` of a
/// session: the `result` of its last turn.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let events = dir.path().join("events.jsonl");
/// std::fs::write(&events, concat!(
///     r#"{"type":"result","result":"first"}"#, "\n",
///     r#"{"type":"assistant"}"#, "\n",
///     r#"{"type":"result","result":"PASS\n\nAll good."}"#, "\n",
/// ))?;
/// assert_eq!(riff::wave::last_result(&events).as_deref(), Some("PASS\n\nAll good."));
/// assert_eq!(riff::wave::last_result(&dir.path().join("none")), None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn last_result(events: &Path) -> Option<String> {
    let text = std::fs::read_to_string(events).ok()?;
    text.lines().rev().find_map(|line| {
        let event: serde_json::Value = serde_json::from_str(line).ok()?;
        (event["type"] == "result")
            .then(|| event["result"].as_str().map(str::to_owned))
            .flatten()
    })
}

/// The verdict of the result of a verify session, and the result after
/// its verdict line. The verdict is the first line that holds only
/// `PASS` or `FAIL`, also with marks of markdown around it.
///
/// ```
/// use riff::pr::Verdict;
/// use riff::wave::verdict;
///
/// assert_eq!(verdict("PASS\n\n- Done.\n"), Some((Verdict::Pass, "- Done.".into())));
/// assert_eq!(verdict("I checked it.\n\n**FAIL**\n1. fails\n"), Some((Verdict::Fail, "1. fails".into())));
/// assert_eq!(verdict("## PASS"), Some((Verdict::Pass, String::new())));
/// assert_eq!(verdict("It may PASS."), None);
/// ```
pub fn verdict(result: &str) -> Option<(Verdict, String)> {
    let mut lines = result.lines();
    let verdict = lines.by_ref().find_map(|line| {
        match line.trim_matches(|c: char| c.is_whitespace() || "#*_`:".contains(c)) {
            "PASS" => Some(Verdict::Pass),
            "FAIL" => Some(Verdict::Fail),
            _ => None,
        }
    })?;
    let rest: Vec<&str> = lines.collect();
    Some((verdict, rest.join("\n").trim().to_owned()))
}

/// The forge and the riff of the waves on this machine: `gh` with the
/// sign-in of the person, riff-server with the sign-in of riff, and
/// git.
#[derive(Default)]
pub struct Live {
    /// The keep-alive of the claim of each session, by session ID.
    alive: Mutex<HashMap<String, JoinHandle<()>>>,
    /// The last text that the lead of each wave got: a text goes one
    /// time in a row.
    told: Mutex<HashMap<String, String>>,
    /// The `gh` of the look of each repository: its calls share one token
    /// (01M54510SFK1Z9VPFYBYYSV10F).
    ghs: Mutex<HashMap<String, Arc<pr::Gh>>>,
}

/// Posts `body` of `kind` as the person of `wave`: to the session that
/// claims `item`, or to the lead of the person. The post has `key`, when
/// there is one (01M5TKG2K1AYK1KE7C2AA2W9DS).
pub(crate) async fn post_as_person(
    wave: &Wave,
    item: Option<&str>,
    body: &str,
    kind: Kind,
    key: Option<&str>,
) -> Result<()> {
    let person = identity::person(&wave.place(None)?, &wave.server)?;
    let api = Api::new(&wave.server).signed_in(None)?;
    let to = match item {
        Some(item) => format!("claim={item},repo={}", wave.repo).parse()?,
        None => riff_core::selector::Selector::lead(person.who().user(), &wave.repo),
    };
    api.post_keyed(&person, Some(&wave.thread()?), &[to], body, kind, key)
        .await?;
    Ok(())
}

/// Runs `work` on a thread that may block: `gh` and git.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| anyhow!("{e}"))?
}

/// The branch of the issue `issue`.
fn branch_of(issue: u64) -> String {
    run::branch(&format!("issue-{issue}"))
}

/// Saves the work of the clone of `record` when its build pushed nothing:
/// a WIP commit on the branch of `issue`, pushed ([`crate::leave::save`],
/// 01M5P26VSA7AA99K1RYG83R8J9). It gives the commit. A fault shows on
/// stderr.
async fn save_work(record: &Record, issue: u64) -> Option<String> {
    let (clone, branch) = (record.folder.join("clone"), branch_of(issue));
    match blocking(move || crate::leave::save(&clone, &branch)).await {
        Ok(saved) => saved.map(|(_, commit)| commit),
        Err(e) => {
            eprintln!("riff daemon: {}: {e:#}", record.live.name);
            None
        }
    }
}

/// What a build fixed on a pull request. When the pull request still
/// has the head commit, the session pushed nothing.
#[derive(Debug, Clone, Copy)]
enum Fix<'a> {
    /// A rebase of the head commit that had the conflict.
    Rebase(&'a str),
    /// The fix of the checks `names` that failed at the head commit.
    Checks(&'a str, &'a [String]),
}

impl Fix<'_> {
    /// The head commit that the build started from.
    fn commit(&self) -> &str {
        match self {
            Fix::Rebase(commit) | Fix::Checks(commit, _) => commit,
        }
    }
}

/// The names of the checks of pull request `number` that failed now and
/// are runs of GitHub Actions, with `gh`: the session fixed no other.
async fn still_failed(gh: pr::Gh, number: u64) -> Result<Vec<String>> {
    blocking(move || {
        let n = number.to_string();
        let pull: rollout::Pull = gh.json(&["pr", "view", &n, "--json", rollout::PULL_FIELDS])?;
        Ok(pull.failed_runs())
    })
    .await
}

/// The steps after the end of a build, with `gh` for the forge: the
/// verify request for the open pull request of its branch, or a note
/// that it has none. When the issue has the label
/// [`rollout::NO_VERIFY`], or a lead holds `verify-ITEM`, the lead gets
/// a note with the reason in place of the verify request, as `riff run`
/// does ([`run::no_verify`], 01M51KH0H7J35PEKMHP5KQB84T).
///
/// The pair is `(fix, saved)`: the commit of `saved` is the WIP commit
/// that riff pushed for a build with no push.
///
/// After a rebase, `fix` is the head commit that had the conflict. When
/// the pull request still has that head, the session pushed nothing: the
/// lead gets a message in place of the verify request
/// (01M57FJ3Z8AFDY9G0KTAYZJESY).
///
/// After the fix of failed checks, `fix` is the head commit whose checks
/// failed. A session that pushed nothing may have run the job again.
/// So riff reads the checks of the pull request: when one still failed,
/// the lead gets a message, and else a note (#279).
async fn after_build(
    gh: pr::Gh,
    api: &Api,
    record: &Record,
    wave: &Wave,
    issue: u64,
    (fix, saved): (Option<Fix<'_>>, Option<&str>),
    ended: Ended,
) -> Result<Vec<String>> {
    let branch = branch_of(issue);
    let (view, at) = (gh.clone(), branch.clone());
    let read = blocking(move || {
        Ok(view.json::<run::Pull>(&["pr", "view", &at, "--json", "number,url,headRefOid,state"]))
    })
    .await?;
    let me = &record.me;
    let to = [riff_core::selector::Selector::lead(
        me.who().user(),
        &wave.repo,
    )];
    let thread = wave.thread()?;
    let item = &record.live.item;
    let pull = match read {
        Ok(pull) => Some(pull).filter(|p| p.state == "OPEN"),
        // A limit of GitHub hides the pull request: it is not missing. The
        // wave finds it after the reset (#1557).
        Err(e) if ghcalls::is_limit_error(&e) => {
            let body = text::wave_build_limit(item, &branch);
            api.post(me, Some(&thread), &to, &body, Kind::Note).await?;
            return Ok(vec![body]);
        }
        Err(_) => None,
    };
    let session = run::short(&record.live.session);
    let mut lines = Vec::new();
    let same = |pull: &run::Pull| fix.is_some_and(|f| f.commit() == pull.head_ref_oid);
    // The checks of the pull request that still failed, after a fix of
    // checks that pushed nothing.
    let still = match (&pull, fix) {
        (Some(pull), Some(Fix::Checks(_, names))) if same(pull) => Some(
            still_failed(gh.clone(), pull.number)
                .await
                .unwrap_or_else(|_| names.to_vec()),
        ),
        _ => None,
    };
    let (body, kind) = match &pull {
        Some(pull) if same(pull) => match &still {
            Some(checks) if checks.is_empty() => (
                text::wave_checks_rerun(item, pull.number, &branch, &pull.head_ref_oid, session),
                Kind::Note,
            ),
            Some(checks) => (
                text::wave_checks_not_fixed(
                    item,
                    pull.number,
                    &branch,
                    &pull.head_ref_oid,
                    session,
                    checks,
                ),
                Kind::Message,
            ),
            None => {
                // The default branch is not in a file: the message says
                // "the default branch" when it does not read.
                let base = blocking({
                    let (gh, repo) = (gh.clone(), wave.repo.clone());
                    move || pr::default_branch(&gh, &repo)
                })
                .await
                .unwrap_or_else(|_| text::WAVE_BASE_UNREAD.to_owned());
                (
                    text::wave_not_rebased(
                        item,
                        pull.number,
                        &branch,
                        &base,
                        &pull.head_ref_oid,
                        session,
                    ),
                    Kind::Message,
                )
            }
        },
        Some(pull) if wip_head(&gh, &wave.repo, &pull.head_ref_oid).await => (
            text::wave_wip_head(item, pull.number, &branch, &pull.head_ref_oid, session),
            Kind::Note,
        ),
        Some(pull) => match no_verify(api, me, &thread, gh, item, issue).await? {
            Some(why) => (
                text::wave_no_verify(
                    item,
                    pull.number,
                    &branch,
                    &pull.head_ref_oid,
                    session,
                    &why,
                ),
                Kind::Note,
            ),
            None => (
                text::wave_verify_request(item, pull.number, &branch, &pull.head_ref_oid, session),
                Kind::Message,
            ),
        },
        None => (
            match saved {
                Some(commit) => {
                    text::wave_saved(item, &branch, commit, &record.folder.join("clone"))
                }
                None => text::wave_no_pull(item, &branch, &record.folder.join("clone")),
            },
            Kind::Note,
        ),
    };
    match &pull {
        Some(pull) => {
            lines.push(text::run_pull(pull.number, &pull.url));
            lines.push(match kind {
                Kind::Message if same(pull) => text::run_lead_told(item),
                Kind::Message => text::run_verify_sent(item, pull.number),
                _ => text::run_lead_noted(item),
            });
        }
        None => {
            lines.push(text::run_no_pull(&branch));
            // A fault of the post does not hide the note to the lead.
            let scrub = crate::run::Scrub::new(crate::session_relay::secrets_here());
            let end = crate::ends::End::of(record, ended, &scrub);
            if let Err(e) = crate::ends::post(api, me, &end).await {
                eprintln!("riff daemon: {}: {e:#}", record.live.name);
            }
        }
    }
    api.post(me, Some(&thread), &to, &body, kind).await?;
    Ok(lines)
}

/// True when the head commit `sha` of the repository `repo` is a WIP
/// commit ([`rollout::head_wip`]). A fault of `gh` is no WIP.
async fn wip_head(gh: &pr::Gh, repo: &str, sha: &str) -> bool {
    let (gh, repo, sha) = (gh.clone(), repo.to_owned(), sha.to_owned());
    blocking(move || Ok(rollout::head_wip(&gh, &repo, &sha)))
        .await
        .unwrap_or(false)
}

/// [`run::no_verify`] now: the labels of the issue with `gh`, then the
/// holds of `thread` on riff-server, as `me`. It asks for the holds only
/// for an issue with no label. A fault of `gh` shows on stderr, and riff
/// goes on to the holds.
async fn no_verify(
    api: &Api,
    me: &SessionUri,
    thread: &ThreadName,
    gh: pr::Gh,
    item: &str,
    issue: u64,
) -> Result<Option<String>> {
    let labelled = blocking(move || run::labelled(&gh, issue))
        .await
        .unwrap_or_else(|e| {
            eprintln!("riff: {e:#}");
            false
        });
    if labelled {
        return Ok(run::no_verify(item, issue, true, &BTreeMap::new()));
    }
    let holds: BTreeMap<String, HoldInfo> = api.plan_show(me, thread).await?.holds;
    Ok(run::no_verify(item, issue, false, &holds))
}

/// The steps after the end of a verify: the report of its result, as
/// `riff verify` does. The folder of a verify that did not pass goes. It
/// gives a line for each step.
///
/// A verify that ended with no PASS or FAIL leaves a record in the thread
/// `ends`, as a build with no pull request does (#1441): the record has
/// the reason of its `ended` and the last lines of its output, so the
/// post comes before the folder goes.
///
/// A report that fails because a limit of GitHub is out keeps the result in
/// the folder, and the loop of the wave reports it after the reset
/// ([`kept`], #1557). The lead gets a note.
async fn after_verify(
    gh: pr::Gh,
    api: &Api,
    record: &Record,
    wave: &Wave,
    (pull, commit): (u64, &str),
    ended: Ended,
) -> Result<Vec<String>> {
    let me = &record.me;
    let thread = wave.thread()?;
    let lead = [riff_core::selector::Selector::lead(
        me.who().user(),
        &wave.repo,
    )];
    let item = &record.live.item;
    let found = last_result(&record.folder.join(crate::daemon::EVENTS));
    let Some((verdict, result)) = found.as_deref().and_then(verdict) else {
        // A fault of the post does not hide the message to the lead.
        let scrub = crate::run::Scrub::new(crate::session_relay::secrets_here());
        let end = crate::ends::End::of(record, ended, &scrub);
        if let Err(e) = crate::ends::post(api, me, &end).await {
            eprintln!("riff daemon: {}: {e:#}", record.live.name);
        }
        let _ = std::fs::remove_dir_all(&record.folder);
        let body = text::wave_no_result(item, pull);
        api.post(me, Some(&thread), &lead, &body, Kind::Message)
            .await?;
        return Ok(vec![body]);
    };
    let (repo, tested, shown) = (wave.repo.clone(), commit.to_owned(), result.clone());
    let reported = blocking(move || pr::report(&gh, &repo, pull, &tested, verdict, &shown)).await;
    let reported = match reported {
        Ok(reported) => reported,
        Err(e) => {
            let error = format!("{e:#}");
            let result = kept::Kept {
                repo: wave.repo.clone(),
                item: item.clone(),
                pull,
                commit: commit.to_owned(),
                verdict,
                result,
            };
            let (body, kind) = match ghcalls::is_limit_error(&e) {
                true => match result.keep(&record.folder) {
                    Ok(()) => (text::wave_report_kept(item, pull, &error), Kind::Note),
                    Err(keep) => {
                        let error = format!("{error}. riff could not keep the result: {keep:#}");
                        let _ = std::fs::remove_dir_all(&record.folder);
                        (text::wave_no_report(item, pull, &error), Kind::Message)
                    }
                },
                false => {
                    let _ = std::fs::remove_dir_all(&record.folder);
                    (text::wave_no_report(item, pull, &error), Kind::Message)
                }
            };
            api.post(me, Some(&thread), &lead, &body, kind).await?;
            return Ok(vec![body]);
        }
    };
    let line = text::verify_reported(verdict, pull, &reported);
    println!("{line}");
    if verdict == Verdict::Fail {
        let _ = std::fs::remove_dir_all(&record.folder);
    }
    post_report(api, me, &thread, (verdict, pull), &reported, &result).await?;
    Ok(vec![line])
}

/// Sends the result of a verify that is on GitHub: the fact of the pull
/// request, and the result to the holder of the item, and to the lead when
/// no session holds the item ([`pr::result_to`]). `me` is the verifier, or
/// the person when the wave reports a kept result ([`kept`]).
pub(crate) async fn post_report(
    api: &Api,
    me: &SessionUri,
    thread: &ThreadName,
    (verdict, pull): (Verdict, u64),
    reported: &pr::Reported,
    result: &str,
) -> Result<()> {
    let state = match verdict {
        Verdict::Pass => PullState::Passed,
        Verdict::Fail => PullState::Failed,
    };
    look::tell_fact(api, me, reported.issue, pull, state).await;
    let to = pr::result_to(reported.issue, me, thread, &api.who(me, false).await?);
    let body = text::verify_post(verdict, pull, reported, result);
    api.post(me, Some(thread), &to, &body, Kind::Message)
        .await?;
    Ok(())
}

impl Live {
    /// Starts the keep-alive of the claim of the session `me`, a session
    /// in a container: it is live while the keep-alive comes
    /// (01M510MG0CKDXAKF6YJ768K7E8).
    fn keep_alive(&self, api: Api, me: SessionUri, session: &str) {
        let task = tokio::spawn(async move {
            api::keep_alive_in_container(&api, &me, WORKER_ALIVE_EVERY).await;
        });
        if let Some(old) = lock(&self.alive).insert(session.to_owned(), task) {
            old.abort();
        }
    }

    /// Releases `item` of the session `me`, ends the session, and stops
    /// its keep-alive.
    async fn end(&self, api: &Api, me: &SessionUri, thread: &ThreadName, item: &str) {
        if let Err(e) = api.release(me, thread, item).await {
            eprintln!("riff daemon: {e:#}");
        }
        if let Some(session) = me.who().session()
            && let Some(task) = lock(&self.alive).remove(session)
        {
            task.abort();
        }
        let _ = api.end(me).await;
    }

    /// The steps before the start after the claim: the clone, the
    /// Claude folder, the spec and the start text. `base` is the default
    /// branch of the repository.
    #[allow(clippy::too_many_arguments)]
    async fn prepare(
        wave: &Wave,
        work: &Work,
        pinned: &Pinned,
        host: &run::Host,
        base: &str,
        milestone: Option<String>,
        session: &str,
        me: &SessionUri,
    ) -> Result<Start> {
        let item = work.item();
        let folder = run::sessions_dir()?.join(format!("{item}-{}", run::short(session)));
        let clone = folder.join("clone");
        let claude = folder.join("claude");
        let sources = session_image::merged_sources(&wave.main)?;
        let auto = crate::permissions::Trust::of(me.place().repo(), &wave.server, &sources);
        run::claude_folder(
            &claude,
            &clone,
            &host.home.join(".claude.json"),
            &auto,
            work.role(),
        )?;
        let (main, common) = run::main_clone(&wave.main)?;
        let start = match work {
            Work::Build { issue } => {
                let issue = *issue;
                let branch = branch_of(issue);
                let (repo, at) = (wave.repo.clone(), clone.clone());
                let earlier = {
                    let (main, branch) = (main.clone(), branch.clone());
                    blocking(move || run::make_clone(&main, &at, &branch)).await?
                };
                let failed = blocking(move || {
                    let pulls = rollout::pulls(&forge(&repo), &repo)?;
                    Ok(failed_verify(&pulls, issue))
                })
                .await
                .unwrap_or_default();
                let milestone = milestone.context("a build has a milestone")?;
                let frame = pr::body(hygiene::Link::Closes(issue), "SUMMARY", &milestone)?;
                let mut text = text::run_start(
                    &wave.repo, base, issue, &branch, session, &frame, &milestone, earlier,
                );
                if let Some(failed) = failed {
                    text.push_str("\n\n");
                    text.push_str(&failed);
                }
                text
            }
            Work::Rebase {
                issue,
                pull,
                commit,
            } => {
                let branch = branch_of(*issue);
                let (main, at, on) = (main.clone(), clone.clone(), branch.clone());
                blocking(move || run::make_clone(&main, &at, &on)).await?;
                text::wave_rebase_start(&wave.repo, base, *issue, *pull, commit, &branch, session)
            }
            Work::Checks {
                issue,
                pull,
                commit,
                checks,
            } => {
                let branch = branch_of(*issue);
                let (main, at, on) = (main.clone(), clone.clone(), branch.clone());
                blocking(move || run::make_clone(&main, &at, &on)).await?;
                text::wave_checks_start(
                    &wave.repo, base, *issue, *pull, commit, checks, &branch, session,
                )
            }
            Work::Verify {
                issue,
                pull,
                commit,
            } => {
                let (at, commit_at) = (clone.clone(), commit.clone());
                let (main_at, pull_at) = (main.clone(), *pull);
                blocking(move || clone_at(&main_at, &at, pull_at, &commit_at)).await?;
                let repo = wave.repo.clone();
                let checks = blocking(move || repo_setup::full_check(&forge(&repo), &repo)).await?;
                let main_at = main.clone();
                let release = blocking(move || Ok(release_config::merged(&main_at))).await?;
                text::wave_verify_start(&wave.repo, base, *issue, *pull, commit, &checks, release)
            }
        };
        let existing = |p: PathBuf| p.exists().then_some(p);
        let facts = Facts {
            session: session.to_owned(),
            item: item.clone(),
            image: pinned.image.clone(),
            home: host.home.clone(),
            user: host.user.clone(),
            clone,
            objects: common.join("objects"),
            claude,
            sign_in: host.claude.sign_in(),
            gitconfig: existing(host.home.join(".gitconfig")),
            gh_config: existing(host.home.join(".config/gh")),
            setup: pinned.setup.clone(),
        };
        let secrets = host.secrets();
        Ok(Start {
            session: session.to_owned(),
            item,
            folder,
            spec: run::spec(&facts),
            start,
            values: host.values(),
            secrets,
            server: wave.server.clone(),
            me: me.clone(),
            terminal: false,
        })
    }
}

/// The line for the start text of a build when the verify of the pull
/// request of its issue failed: where the result is.
fn failed_verify(pulls: &[rollout::Pull], issue: u64) -> Option<String> {
    let pull = pulls.iter().find(|p| p.issue() == Some(issue))?;
    (pull.verify() == Some(rollout::Verify::Failed)).then(|| {
        let url = pull
            .verify_status()
            .and_then(|c| c.url.as_deref())
            .map(text::safe)
            .unwrap_or_default();
        text::wave_failed_verify(pull.number, &pull.head, &url)
    })
}

/// Makes the clone of a verify at `commit`, detached: the head commit of
/// the pull request `pull`. The clone has the branches of `origin` only,
/// and the head of a pull request from a fork is on no such branch. So
/// it fetches `refs/pull/PULL/head` from `origin` first. It checks out
/// the commit, not the ref: a push to the pull request after the look
/// does not change what the verify reads
/// (01M5QVARWYFHTQV5THNRB09SZA).
fn clone_at(main: &Path, clone: &Path, pull: u64, commit: &str) -> Result<()> {
    let remote = run::output("git", main, &["remote", "get-url", "origin"])?;
    let top = main.to_string_lossy();
    let path = clone.to_string_lossy();
    run::output(
        "git",
        main,
        &["clone", "-q", "--reference", &top, "--", &remote, &path],
    )?;
    run::output(
        "git",
        clone,
        &["fetch", "-q", "origin", &format!("refs/pull/{pull}/head")],
    )?;
    run::output(
        "git",
        clone,
        &[
            "-c",
            "advice.detachedHead=false",
            "checkout",
            "-q",
            "--detach",
            commit,
        ],
    )?;
    Ok(())
}

/// The free work of a wave with the fixes of the pull requests that
/// cannot merge by themselves, from the free work `free` of the facts
/// `open` ([`rollout::Open::free_work`]) (01M57FJ3WWMQSDBFRRHXN3SMFQ,
/// #279). They are [`Fixes`]:
///
/// - A pull request to rebase is an open pull request of an item with
///   auto-merge on and a conflict with `main` ([`rollout::Pull::conflict`])
///   whose verify did not fail: a failed verify is free work as a build
///   already. The free work has no verify of a head with a conflict.
/// - The free work has no verify of a pull request whose state GitHub
///   does not know yet: its `mergeable` is `UNKNOWN`
///   ([`rollout::Pull::unknown`], #1464). That is so for a short time
///   after each push, and after each move of `main`. The verify waits
///   for the next look: with `CONFLICTING` the pull request gets the
///   rebase, with `MERGEABLE` the verify.
/// - A pull request with failed checks is one whose head has a failed
///   check ([`rollout::Stuck::Checks`]), where only the newest run of a
///   check counts, and whose verify passed or waits. It is a build, with
///   the names of the checks that are runs of GitHub Actions
///   ([`rollout::Pull::failed_runs`]). The free work has no verify of such
///   a head (#1575): a failed check fails the verify, so the fix comes
///   first, and the verify of the new head after it.
/// - A pull request in the merge queue ([`rollout::Pull::queued`]) is none
///   of these: the wave moves nothing for it (01M5TKPNDET56VR8WZ74FSR71F).
///   One that the queue removed, with a conflict, is a pull request to
///   rebase; with no conflict it is a notice ([`rollout::Stuck::Dequeued`]).
/// - A pull request that no session can fix gets a notice for the lead:
///   one with auto-merge off after a pass ([`rollout::Stuck::AutoMergeOff`]),
///   a release or a tracker with a failed check, and a failed check that
///   is no run of GitHub Actions ([`rollout::Stuck::Other`]), one notice
///   for each check. No session builds a release (the label `release`) or
///   a tracker. A verified pull request that waits for a person is a notice
///   too: a review that GitHub needs ([`rollout::Stuck::Review`], 01M6FZ141Y0J8G2AYX0G5HY1CX), and
///   a run of a fork that waits for approval, one notice for each run
///   ([`rollout::Stuck::Approve`]).
///
/// The items `issue-N` of the rebases come after the fix items of a
/// broken `main` (01M51QEYS538XA58KDRY844T52), before each other item,
/// then the items of the failed checks. [`pick`] skips a held or a
/// claimed item, as each item.
///
/// ```
/// use riff::rollout::{Check, Issue, Label, Open, OtherCheck, Pull, Review, Run, Stuck};
/// use riff::wave::{Conflict, Failed, Notice, fixes};
///
/// let pull = |number, issue: u64, state: Option<&str>, mergeable: &str| Pull {
///     number,
///     branch: format!("worktree-issue-{issue}"),
///     head: format!("c{issue}"),
///     checks: state.map(Check::verify).into_iter().collect(),
///     mergeable: Some(mergeable.into()),
///     auto_merge: true,
///     ..Pull::default()
/// };
/// let gate = |pull: Pull| Pull { checks: [pull.checks, vec![Check::run("Gate", "FAILURE")]].concat(), ..pull };
/// let again = |pull: Pull| {
///     let runs = [
///         Check::run("Gate", "FAILURE").started("2026-11-03T05:50:00Z"),
///         Check::run("Gate", "SUCCESS").started("2026-11-03T05:55:00Z"),
///     ];
///     Pull { checks: [pull.checks, runs.to_vec()].concat(), ..pull }
/// };
/// let circle = || Check::other("ci", "FAILURE", "https://app.circleci.com/workflow/9");
/// let other = |pull: Pull| Pull { checks: [pull.checks, vec![circle()]].concat(), ..pull };
/// let review = |pull: Pull| Pull { review: Some("REVIEW_REQUIRED".into()), ..pull };
/// let hygiene = || Run { id: 7, name: "Hygiene".into(), link: None };
/// let fork = |pull: Pull| Pull { merge_state: Some("BLOCKED".into()), waiting: vec![hygiene()], ..pull };
/// let open = Open {
///     issues: vec![
///         Issue { number: 9, labels: vec![Label { name: "broken-main".into() }], ..Issue::default() },
///         Issue { number: 8, labels: vec![Label { name: "release".into() }], ..Issue::default() },
///     ],
///     pulls: vec![
///         pull(41, 1, Some("SUCCESS"), "CONFLICTING"),
///         pull(42, 2, None, "CONFLICTING"),
///         pull(43, 3, Some("FAILURE"), "CONFLICTING"),
///         pull(44, 4, Some("SUCCESS"), "MERGEABLE"),
///         Pull { auto_merge: false, ..pull(45, 5, Some("SUCCESS"), "CONFLICTING") },
///         gate(pull(46, 6, Some("SUCCESS"), "MERGEABLE")),
///         gate(pull(47, 8, None, "MERGEABLE")),
///         other(pull(48, 10, Some("SUCCESS"), "MERGEABLE")),
///         again(pull(52, 14, Some("SUCCESS"), "MERGEABLE")),
///         pull(49, 11, None, "UNKNOWN"),
///         review(pull(50, 12, Some("SUCCESS"), "MERGEABLE")),
///         fork(pull(51, 13, Some("SUCCESS"), "MERGEABLE")),
///     ],
///     ..Open::default()
/// };
/// let free = ["issue-9", "verify-issue-2", "issue-3", "issue-7", "verify-issue-11"].map(String::from).to_vec();
/// let (free, fixes) = fixes(&open, free);
/// // The rebases of 1 and 2, the fix of the checks of 6.
/// assert_eq!(free, ["issue-9", "issue-1", "issue-2", "issue-6", "issue-3", "issue-7"]);
/// assert_eq!(fixes.conflicts.keys().copied().collect::<Vec<_>>(), [1, 2]);
/// assert_eq!(fixes.conflicts[&1], Conflict { pull: 41, commit: "c1".into() });
/// assert_eq!(fixes.failed[&6], Failed { pull: 46, commit: "c6".into(), checks: vec!["Gate".into()] });
/// // The lead hears of 5 (auto-merge off), of the release 8, of the check of 10 (no run of Actions, so no fix),
/// // of the review of 12, and of the run of the fork of 13.
/// let told: Vec<(u64, Stuck)> = fixes.notices.iter().map(|n| (n.pull, n.stuck.clone())).collect();
/// let ci = OtherCheck { name: "ci".into(), app: Some("app.circleci.com".into()), link: Some("https://app.circleci.com/workflow/9".into()) };
/// assert_eq!(told, [
///     (45, Stuck::AutoMergeOff),
///     (47, Stuck::Checks(vec!["Gate".into()])),
///     (48, Stuck::Other(ci)),
///     (50, Stuck::Review(Review::Required)),
///     (51, Stuck::Approve(hygiene())),
/// ]);
/// assert!(!fixes.failed.contains_key(&10), "no session fixes it");
/// // An old failed run and a newer run that passed (#1540): no fix, no notice.
/// assert!(!fixes.failed.contains_key(&14) && fixes.notices.iter().all(|n| n.pull != 52));
/// assert_eq!(fixes.notices[0], Notice { pull: 45, branch: "worktree-issue-5".into(), commit: "c5".into(), stuck: Stuck::AutoMergeOff });
///
/// // The pull request of 11 reads UNKNOWN: its verify waits, and the pull request is no conflict yet.
/// assert!(!free.contains(&"verify-issue-11".to_owned()));
/// assert!(!fixes.conflicts.contains_key(&11));
/// // The next look reads MERGEABLE: the verify is free work.
/// let next = |mergeable| Open { pulls: vec![pull(49, 11, None, mergeable)], ..Open::default() };
/// let waiting = || vec!["verify-issue-11".to_owned()];
/// let (free, _) = riff::wave::fixes(&next("MERGEABLE"), waiting());
/// assert_eq!(free, ["verify-issue-11"]);
/// // Or it reads CONFLICTING: the rebase, and no verify.
/// let (free, fixes) = riff::wave::fixes(&next("CONFLICTING"), waiting());
/// assert_eq!(free, ["issue-11"]);
/// assert!(fixes.conflicts.contains_key(&11));
///
/// // #1575: the pull request of 16 waits for a verify, and its Gate failed: the fix, and no verify.
/// let waits = Open { pulls: vec![gate(pull(53, 16, None, "MERGEABLE"))], ..Open::default() };
/// let (free, fixes) = riff::wave::fixes(&waits, vec!["verify-issue-16".to_owned()]);
/// assert_eq!(free, ["issue-16"]);
/// assert_eq!(fixes.failed[&16], Failed { pull: 53, commit: "c16".into(), checks: vec!["Gate".into()] });
/// // A check that goes did not fail: the verify waits for the full check, as before.
/// let goes = |pull: Pull| Pull { checks: vec![Check::run("Gate", "")], ..pull };
/// let waits = Open { pulls: vec![goes(pull(53, 16, None, "MERGEABLE"))], ..Open::default() };
/// let (free, fixes) = riff::wave::fixes(&waits, vec!["verify-issue-16".to_owned()]);
/// assert_eq!(free, ["verify-issue-16"]);
/// assert!(fixes.failed.is_empty());
/// ```
pub fn fixes(open: &rollout::Open, free: Vec<String>) -> (Vec<String>, Fixes) {
    // No session builds a release or a tracker.
    let no_build: HashSet<u64> = open
        .issues
        .iter()
        .filter(|i| i.release() || i.tracker())
        .map(|i| i.number)
        .collect();
    let mut found = Fixes::default();
    // The items whose head has a failed check: their verify waits for the
    // fix, as it would fail (#1575).
    let mut failing: HashSet<u64> = HashSet::new();
    for (p, stuck) in open.stuck() {
        let item = p.issue().filter(|n| !no_build.contains(n));
        if matches!(stuck, rollout::Stuck::Checks(_)) {
            failing.extend(p.issue());
        }
        match (stuck, item) {
            // A conflict has an issue: its verify is not `None`.
            (rollout::Stuck::Conflict, _) => {
                if let Some(n) = p.issue() {
                    let conflict = Conflict {
                        pull: p.number,
                        commit: p.head.clone(),
                    };
                    found.conflicts.insert(n, conflict);
                }
            }
            (rollout::Stuck::Checks(_), Some(n)) => {
                // A session fixes a run of GitHub Actions only: it reads
                // the log with `gh run view`. The lead hears of each
                // other check.
                let runs = p.failed_runs();
                if !runs.is_empty() {
                    let failed = Failed {
                        pull: p.number,
                        commit: p.head.clone(),
                        checks: runs,
                    };
                    found.failed.insert(n, failed);
                }
                for other in p.failed_others() {
                    found.notices.push(Notice {
                        pull: p.number,
                        branch: p.branch.clone(),
                        commit: p.head.clone(),
                        stuck: rollout::Stuck::Other(other),
                    });
                }
            }
            // One notice for each run that waits: each run has its own
            // command.
            (rollout::Stuck::Approve(_), _) => {
                for run in &p.waiting {
                    found.notices.push(Notice {
                        pull: p.number,
                        branch: p.branch.clone(),
                        commit: p.head.clone(),
                        stuck: rollout::Stuck::Approve(run.clone()),
                    });
                }
            }
            (stuck, _) => found.notices.push(Notice {
                pull: p.number,
                branch: p.branch.clone(),
                commit: p.head.clone(),
                stuck,
            }),
        }
    }
    let fix_items: HashSet<u64> = open
        .issues
        .iter()
        .filter(|i| i.broken_main())
        .map(|i| i.number)
        .collect();
    let fix = |item: &String| usage::issue_of(item).is_some_and(|(n, _)| fix_items.contains(&n));
    // The verify of a pull request whose state GitHub does not know yet
    // waits for the next look (#1464).
    let unknown = |n: u64| {
        open.pulls
            .iter()
            .find(|p| p.issue() == Some(n))
            .is_some_and(rollout::Pull::unknown)
    };
    let mut free: Vec<String> = free
        .into_iter()
        .filter(|item| {
            verify_issue(item).is_none_or(|n| {
                !found.conflicts.contains_key(&n) && !unknown(n) && !failing.contains(&n)
            })
        })
        .collect();
    let at = free.iter().take_while(|item| fix(item)).count();
    let new: Vec<String> = found
        .conflicts
        .keys()
        .chain(found.failed.keys())
        .map(|n| format!("issue-{n}"))
        .filter(|item| !free.contains(item))
        .collect();
    free.splice(at..at, new);
    (free, found)
}

/// The verify requests that answer a failed verify: the pairs of an
/// issue and a commit (7 characters) whose last verify request in
/// `bodies`, the bodies of the messages of the thread, oldest first,
/// came after the last verify result for them (#1157). A fix of the
/// body of a pull request makes no new commit, so the failed status
/// of the head stays. The new request is the sign that the fail is
/// answered.
///
/// ```
/// use riff::wave::reasked;
///
/// let request = "verify request: issue-12, PR #40, branch worktree-issue-12, commit 1a2b3c4, from the riff daemon (session 65f510d7).";
/// let result = "verify result: FAIL for issue-12, PR #40, commit 1a2b3c4. PR comment https://c, riff/verify failure set.";
/// assert!(reasked(&[request, result]).is_empty());
/// assert!(reasked(&[request, result, request]).contains(&(12, "1a2b3c4".into())));
/// assert!(reasked(&[request, result, request, result]).is_empty());
/// // A result names the full commit.
/// let full = "verify result: FAIL for issue-12, PR #40, commit 1a2b3c4d5e6f7a8b9c0d. PR comment https://c.";
/// assert!(reasked(&[request, full]).is_empty());
/// ```
pub fn reasked(bodies: &[&str]) -> HashSet<(u64, String)> {
    // `issue-N, PR #P, ... commit ABCDEFG`.
    let key = |rest: &str| -> Option<(u64, String)> {
        let issue = rest.strip_prefix("issue-")?;
        let issue = issue
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()?;
        let commit = rest.split_once("commit ")?.1;
        let commit: String = commit
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .take(7)
            .collect();
        Some((issue, commit))
    };
    let mut asked = HashSet::new();
    for body in bodies {
        if let Some(rest) = body.strip_prefix("verify request: ") {
            asked.extend(key(rest));
        } else if let Some(found) = body
            .strip_prefix("verify result: ")
            .and_then(|r| r.split_once(" for "))
            .and_then(|(_, rest)| key(rest))
        {
            asked.remove(&found);
        }
    }
    asked
}

/// The facts of the wave of the repository `repo` from the forge, with
/// `gh`, and the items that a session claims (`claims`) and the held
/// items (`holds`): the default branch ([`pr::default_branch`]), the free
/// work with the fixes ([`fixes`]), and
/// the pull request of each verify among it, with the full check of its
/// head ([`repo_setup::full_check`]).
pub fn look_on(
    gh: &pr::Gh,
    repo: &str,
    claims: BTreeSet<String>,
    holds: BTreeSet<String>,
) -> Result<Look> {
    look_with(gh, repo, claims, holds, &HashSet::new())
}

/// [`look_on`] with the verify requests `asked` that answer a failed
/// verify ([`reasked`]): a pull request with a failed verify whose
/// head is in it waits for a verify again.
pub fn look_with(
    gh: &pr::Gh,
    repo: &str,
    claims: BTreeSet<String>,
    holds: BTreeSet<String>,
    asked: &HashSet<(u64, String)>,
) -> Result<Look> {
    // GitHub is asked first whether anything changed (#1566). A branch that
    // does not read is the error of the look: the wave starts no work, and
    // reads it again at the next look.
    let changes::Facts { branch, mut open } = changes::facts(gh, repo, || {
        Ok(changes::Facts {
            branch: pr::default_branch(gh, repo)?,
            open: rollout::Open::of(gh, repo)?,
        })
    })?;
    let claimed: HashSet<String> = claims.iter().cloned().collect();
    let held: HashSet<String> = holds.iter().cloned().collect();
    for pull in &mut open.pulls {
        let commit: String = pull.head.chars().take(7).collect();
        pull.reasked = pull.issue().is_some_and(|n| asked.contains(&(n, commit)));
    }
    let (free, fixes) = fixes(&open, open.free_work(&claimed, &held));
    let no_done_when = open.no_done_when(&claimed, &held).into_iter().collect();
    let pulls = waiting(gh, repo, &free, &open)?;
    Ok(Look {
        paused: false,
        free,
        claims,
        holds,
        pulls,
        fixes,
        no_done_when,
        branch,
    })
}

/// The pull request of each verify among the free work `free` of `open`,
/// with the full check of its head (01M5QYDHN27RC5SRHAB56PFKK0). The wave
/// reads the full check of the repository one time, and only when a
/// verify waits ([`repo_setup::full_check`]).
fn waiting(
    gh: &pr::Gh,
    repo: &str,
    free: &[String],
    open: &rollout::Open,
) -> Result<BTreeMap<u64, Waiting>> {
    let mut pulls = BTreeMap::new();
    let mut checks: Option<Vec<String>> = None;
    for issue in free.iter().filter_map(|item| verify_issue(item)) {
        let Some(pull) = open.pulls.iter().find(|p| p.issue() == Some(issue)) else {
            continue;
        };
        if checks.is_none() {
            checks = Some(repo_setup::full_check(gh, repo)?);
        }
        let states = pr::gate_states(gh, repo, checks.as_deref().unwrap_or_default(), &pull.head)?;
        pulls.insert(
            issue,
            Waiting {
                pull: pull.number,
                commit: pull.head.clone(),
                gate_done: pr::full_done(&states),
            },
        );
    }
    Ok(pulls)
}

impl World for Live {
    fn look<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Look>> {
        Box::pin(async move {
            let person = identity::person(&wave.place(None)?, &wave.server)?;
            let thread = wave.thread()?;
            let api = Api::new(&wave.server).signed_in(None)?;
            if api.pauses(&person).await?.state == RiffState::Paused {
                return Ok(Look {
                    paused: true,
                    ..Look::default()
                });
            }
            let claims: BTreeSet<String> = api
                .who(&person, false)
                .await?
                .iter()
                .filter(|s| s.uri.place().repo_text() == wave.repo)
                .flat_map(|s| s.uri.claims().iter().cloned())
                .collect();
            let holds: BTreeSet<String> = api
                .plan_show(&person, &thread)
                .await?
                .holds
                .into_keys()
                .collect();
            let bodies: Vec<String> = api
                .read(&person, &thread, true)
                .await
                .map(|all| all.into_iter().map(|m| m.message.body).collect())
                .unwrap_or_default();
            let asked = reasked(&bodies.iter().map(String::as_str).collect::<Vec<_>>());
            let repo = wave.repo.clone();
            blocking(move || look_with(&forge(&repo), &repo, claims, holds, &asked)).await
        })
    }

    fn pinned<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Pinned>> {
        let (main, repo) = (wave.main.clone(), wave.repo.clone());
        Box::pin(blocking(move || {
            let merged = session_image::merged_config(&main, |line| println!("{repo}: {line}"))?;
            Ok(Pinned {
                image: merged.image.to_string(),
                setup: merged.setup,
            })
        }))
    }

    fn host<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<(), run::HostFault>> {
        let main = wave.main.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || run::host_facts(&main).map(drop))
                .await
                .unwrap_or_else(|e| Err(run::HostFault::Missing(e.to_string())))
        })
    }

    fn begin<'a>(
        &'a self,
        wave: &'a Wave,
        work: &'a Work,
        pinned: &'a Pinned,
    ) -> BoxFuture<'a, Result<Option<Start>>> {
        Box::pin(async move {
            let item = work.item();
            let main = wave.main.clone();
            let host = blocking(move || Ok(run::host_facts(&main)?)).await?;
            let base = {
                let repo = wave.repo.clone();
                blocking(move || pr::default_branch(&forge(&repo), &repo)).await?
            };
            let milestone = match work {
                Work::Build { issue } => {
                    let (repo, issue) = (wave.repo.clone(), *issue);
                    Some(blocking(move || run::milestone(&forge(&repo), issue)).await?)
                }
                Work::Verify { .. } | Work::Rebase { .. } | Work::Checks { .. } => None,
            };
            let session = identity::new_session_id();
            let place = wave.place(Some(&item))?;
            let me = identity::agent(&place, &session, &wave.server)?;
            let thread = wave.thread()?;
            let api = Api::new(&wave.server).signed_in(Some(&session))?;
            api.start(&me, StartReason::Process, true).await?;
            // A refused claim ends the new session: else it lives on as
            // an idle worker with no item (01M558EYCXPFW8CJV3XJ7HK3NR).
            let claimed = match api.claim(&me, &thread, &item).await {
                Ok(claimed) => claimed,
                Err(e) => {
                    let _ = api.end(&me).await;
                    return Err(e);
                }
            };
            if !claimed.granted {
                let _ = api.end(&me).await;
                return Ok(None);
            }
            self.keep_alive(api.clone(), me.clone(), &session);
            println!("{}", text::wave_claimed(&wave.repo, &item, &session));
            match Self::prepare(wave, work, pinned, &host, &base, milestone, &session, &me).await {
                Ok(start) => Ok(Some(start)),
                Err(e) => {
                    self.end(&api, &me, &thread, &item).await;
                    Err(e)
                }
            }
        })
    }

    fn resume(&self, record: &Record) {
        match Api::new(&record.server).signed_in(Some(&record.live.session)) {
            Ok(api) => self.keep_alive(api, record.me.clone(), &record.live.session),
            Err(e) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
        }
    }

    fn finish<'a>(
        &'a self,
        record: &'a Record,
        ended: Ended,
    ) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            let job = record
                .job
                .as_ref()
                .ok_or_else(|| anyhow!("the session {} has no job", record.live.name))?;
            let wave = &job.wave;
            // The steps after the end outlast a restart of riff-server,
            // as in `riff run` (01M4ZQVVGQYXTAVRB69C7YPC3Y).
            let api = Api::new(&record.server)
                .signed_in(Some(&record.live.session))?
                .with_budget(run::END_BUDGET);
            let thread = wave.thread()?;
            println!(
                "{}",
                text::wave_ended(&wave.repo, &record.live.item, ended.code, ended.failed)
            );
            let done = match &job.work {
                Work::Build { issue } => {
                    let gh = forge(&wave.repo);
                    let saved = save_work(record, *issue).await;
                    after_build(
                        gh,
                        &api,
                        record,
                        wave,
                        *issue,
                        (None, saved.as_deref()),
                        ended,
                    )
                    .await
                }
                Work::Rebase { issue, commit, .. } => {
                    let gh = forge(&wave.repo);
                    let fix = Some(Fix::Rebase(commit));
                    after_build(gh, &api, record, wave, *issue, (fix, None), ended).await
                }
                Work::Checks {
                    issue,
                    commit,
                    checks,
                    ..
                } => {
                    let gh = forge(&wave.repo);
                    let fix = Some(Fix::Checks(commit, checks));
                    after_build(gh, &api, record, wave, *issue, (fix, None), ended).await
                }
                Work::Verify { pull, commit, .. } => {
                    after_verify(
                        forge(&wave.repo),
                        &api,
                        record,
                        wave,
                        (*pull, commit),
                        ended,
                    )
                    .await
                }
            };
            self.end(&api, &record.me, &thread, &record.live.item).await;
            done
        })
    }

    fn meter(&self, repo: &str) -> Option<usage::Meter> {
        Some(usage::Meter {
            dir: crate::local::marks()?,
            gh: sweep::gh(repo).by("usage"),
        })
    }

    fn marks(&self) -> Option<PathBuf> {
        crate::local::marks()
    }

    fn fate<'a>(&'a self, repo: &'a str, issue: u64) -> BoxFuture<'a, Result<Fate>> {
        let (gh, repo) = (sweep::gh(repo), repo.to_owned());
        Box::pin(blocking(move || sweep::fate_on(&gh, &repo, issue)))
    }

    fn delete_old_kept<'a>(&'a self, repo: &'a str) -> BoxFuture<'a, Result<Vec<String>>> {
        let (gh, repo) = (sweep::gh(repo), repo.to_owned());
        Box::pin(blocking(move || sweep::delete_old_kept_on(&gh, &repo)))
    }

    fn merged_in<'a>(
        &'a self,
        repo: &'a str,
        issue: u64,
        pull: u64,
        commit: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        let (gh, commit) = (sweep::gh(repo), commit.to_owned());
        Box::pin(blocking(move || {
            sweep::merged_in_on(&gh, issue, pull, &commit)
        }))
    }

    fn leave<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if let Some(task) = lock(&self.alive).remove(&record.live.session) {
                task.abort();
            }
            match Api::new(&record.server).signed_in(Some(&record.live.session)) {
                Ok(api) => {
                    let _ = api.end(&record.me).await;
                }
                Err(e) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
            }
        })
    }

    fn lead_again<'a>(
        &'a self,
        wave: Option<&'a Wave>,
        record: &'a Record,
    ) -> BoxFuture<'a, Result<Start>> {
        let (wave, record) = (wave.cloned(), record.clone());
        Box::pin(blocking(move || crate::lead::again(wave.as_ref(), &record)))
    }

    fn free<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let api = Api::new(&record.server).signed_in(Some(&record.live.session))?;
            let thread = record
                .me
                .default_thread()
                .ok_or_else(|| anyhow!("{}", text::RUN_NO_REPO))?;
            api.release(&record.me, &thread, &record.live.item).await?;
            if let Some(task) = lock(&self.alive).remove(&record.live.session) {
                task.abort();
            }
            api.end(&record.me).await
        })
    }

    fn wake<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            use futures::StreamExt;
            let api = Api::new(&record.server).signed_in(Some(&record.live.session))?;
            let me = &record.me;
            let wakes = api::follow(|| api.watch(me), WAKE_RETRY);
            tokio::pin!(wakes);
            loop {
                match wakes.next().await {
                    Some(Ok(wake)) => return Ok(text::wake_line(&wake)),
                    Some(Err(e)) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
                    None => bail!("{}", text::DAEMON_GONE),
                }
            }
        })
    }

    fn wakes<'a>(
        &'a self,
        record: &'a Record,
        to: mpsc::UnboundedSender<Wake>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            use futures::StreamExt;
            let api = match Api::new(&record.server).signed_in(Some(&record.live.session)) {
                Ok(api) => api,
                Err(e) => {
                    eprintln!("riff daemon: {}: {e:#}", record.live.name);
                    return;
                }
            };
            let me = &record.me;
            let wakes = api::follow(|| api.watch(me), WAKE_RETRY);
            tokio::pin!(wakes);
            while let Some(wake) = wakes.next().await {
                match wake {
                    Ok(wake) => {
                        if to.send(wake).is_err() {
                            return;
                        }
                    }
                    Err(e) => eprintln!("riff daemon: {}: {e:#}", record.live.name),
                }
            }
        })
    }

    fn refuse<'a>(&'a self, record: &'a Record, wake: &'a Wake) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let body = text::wake_refused(&record.live.session, wake);
            let told = async {
                let api = Api::new(&record.server).signed_in(Some(&record.live.session))?;
                api.tell(&record.me, &wake.from.to_string(), &body).await
            };
            if let Err(e) = told.await {
                eprintln!("riff daemon: {}: {e:#}", record.live.name);
            }
        })
    }

    fn tell<'a>(
        &'a self,
        wave: &'a Wave,
        body: &'a str,
        key: Option<&'a str>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if lock(&self.told).get(&wave.repo).is_some_and(|t| t == body) {
                return;
            }
            match post_as_person(wave, None, body, Kind::Message, key).await {
                Ok(()) => {
                    lock(&self.told).insert(wave.repo.clone(), body.to_owned());
                }
                Err(e) => eprintln!("riff daemon: {}: {e:#}", wave.repo),
            }
        })
    }

    fn gh(&self, repo: &str) -> Arc<pr::Gh> {
        lock(&self.ghs)
            .entry(repo.to_owned())
            .or_insert_with(|| Arc::new(sweep::gh(repo).by("look").with_one_token()))
            .clone()
    }

    fn claims<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<HashSet<String>>> {
        Box::pin(async move {
            let person = identity::person(&wave.place(None)?, &wave.server)?;
            let api = Api::new(&wave.server).signed_in(None)?;
            let mut sessions = api.who(&person, false).await?;
            sessions.retain(|s| s.uri.place().repo_text() == wave.repo);
            Ok(rollout::claims(&sessions))
        })
    }

    fn post<'a>(
        &'a self,
        wave: &'a Wave,
        item: Option<&'a str>,
        body: &'a str,
        kind: Kind,
        key: Option<&'a str>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(post_as_person(wave, item, body, kind, key))
    }

    fn chat<'a>(&'a self, record: &'a Record, body: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let posted = async {
                let person = identity::person(record.me.place(), &record.server)?;
                let api = Api::new(&record.server).signed_in(None)?;
                crate::chat::post_by_riff(&api, &person, &[], body).await
            };
            if let Err(e) = posted.await {
                eprintln!("riff daemon: {}: {e:#}", record.live.name);
            }
        })
    }

    fn note_lead<'a>(&'a self, record: &'a Record, body: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let posted = async {
                let person = identity::person(record.me.place(), &record.server)?;
                let api = Api::new(&record.server).signed_in(None)?;
                crate::hold::tell_session(&api, &person, &record.live.session, body).await?;
                Ok::<(), anyhow::Error>(())
            };
            if let Err(e) = posted.await {
                eprintln!("riff daemon: {}: {e:#}", record.live.name);
            }
        })
    }

    fn report_kept<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Vec<String>> {
        Box::pin(async move {
            let reported = async {
                let sessions = run::sessions_dir()?;
                if kept::find(&sessions, &wave.repo).is_empty() {
                    return Ok(Vec::new());
                }
                // The session of the verify ended: the person reports.
                let person = identity::person(&wave.place(None)?, &wave.server)?;
                let api = Api::new(&wave.server).signed_in(None)?;
                let gh = self.gh(&wave.repo).as_ref().clone().by("wave");
                Ok::<_, anyhow::Error>(kept::report_all(&gh, &api, &person, wave, &sessions).await)
            };
            reported.await.unwrap_or_else(|e| {
                eprintln!("riff daemon: {}: {e:#}", wave.repo);
                Vec::new()
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    use super::*;
    use crate::daemon::dir_in;
    use crate::room::Limit;
    use crate::runtime::{Bind, Env, Fake, SessionSpec, Source};

    const IMAGE: &str = "localhost/riff-session:test";
    const GB: u64 = 1 << 30;

    /// The link of the failed check of another app than GitHub Actions in
    /// the fake forge.
    const OTHER_LINK: &str = "https://app.circleci.com/workflow/9";

    /// A pull request of the fake forge.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Pr {
        /// It waits for a verify.
        Asked,
        /// The verify passed.
        Passed,
        /// The verify failed.
        Failed,
        /// The verify failed, and a new verify request for the head came.
        Reasked,
    }

    /// The forge and the riff of a test.
    #[derive(Debug, Default)]
    struct Forge {
        /// The open items of the wave, in order.
        items: Vec<u64>,
        holds: BTreeSet<String>,
        /// The claims: the item, and the session that holds it.
        claims: BTreeMap<String, String>,
        /// The pull requests, by issue.
        pulls: BTreeMap<u64, Pr>,
        /// Each claim of a session: the item, and the session.
        starts: Vec<(String, String)>,
        /// True while the default branch of the repository does not read.
        branch_unread: bool,
        /// The looks that read the default branch.
        branch_reads: usize,
        /// Each item after its end.
        finished: Vec<String>,
        /// Each message to the lead.
        told: Vec<String>,
        /// The key of each message to the lead.
        told_keys: Vec<Option<String>>,
        sessions: u64,
        /// A build ends with no pull request.
        no_pull: bool,
        /// Each folder is `ITEM-ID8` with a git clone of `o/r`, so that
        /// the sweep acts on it.
        git: bool,
        /// The fate of the folders of each issue, else it stays.
        fates: BTreeMap<u64, Fate>,
        /// The steps after the end wait for a permit of it.
        gate: Option<Arc<tokio::sync::Semaphore>>,
        /// Each item whose steps after the end started.
        finishing: Vec<String>,
        /// The codes of the refusals of the next claims, in order, as
        /// riff-server gives them: for example `paused`.
        refusals: Vec<String>,
        /// Each item whose claim riff-server refused.
        refused: Vec<String>,
        /// `claude` refused the token of the host.
        token_refused: bool,
        /// The minutes that the sign-in of `claude` on the host has
        /// left. `None`: many.
        minutes: Option<i64>,
        /// The number of the next steps before the start that fail for
        /// a sign-in of `claude` that ends soon, after the check of the
        /// host passed.
        late_faults: usize,
        /// The head commit of the pull request of each issue, else
        /// `cN`.
        heads: BTreeMap<u64, String>,
        /// The issues whose pull request has a conflict with `main`.
        conflicting: BTreeSet<u64>,
        /// The issues whose pull request reads `UNKNOWN`: GitHub did not
        /// find yet if it has a conflict. It wins over `conflicting`.
        unknown: BTreeSet<u64>,
        /// A verify ends with no result: its pull request waits still.
        no_result: bool,
        /// The issues whose pull request has auto-merge off.
        manual: BTreeSet<u64>,
        /// The issues whose pull request reads `REVIEW_REQUIRED`: GitHub
        /// waits for an approving review.
        review: BTreeSet<u64>,
        /// The issues whose pull request GitHub blocks, and whose runs of
        /// Actions wait for approval, newest first (a fork).
        approvals: BTreeMap<u64, Vec<rollout::Run>>,
        /// The place of the pull request of each issue in the merge queue,
        /// else `Queue::Out`.
        queue: BTreeMap<u64, rollout::Queue>,
        /// The issues whose pull request has a failed check `Gate`.
        failing: BTreeSet<u64>,
        /// The issues whose pull request has a failed check of another app
        /// than GitHub Actions ([`OTHER_LINK`]).
        failing_other: BTreeSet<u64>,
        /// The issues whose pull request has an old failed run of the check
        /// `Gate` and a newer run of it that passed (#1540).
        rerun: BTreeSet<u64>,
        /// A rebase and a fix of checks end with no push.
        no_push: bool,
        /// The work of each claim of a session, in order.
        works: Vec<Work>,
        /// The title of each item, else `item N`.
        titles: BTreeMap<u64, String>,
        /// The labels of each item.
        labels: BTreeMap<u64, Vec<String>>,
        /// The items whose body has no `Done when:` line.
        bare: BTreeSet<u64>,
        /// Each post of the look: the item whose holder gets it, or `None`
        /// for the lead, the kind, and the body.
        posts: Vec<(Option<String>, Kind, String)>,
        /// The key of each post of `posts`, in the same order.
        post_keys: Vec<Option<String>>,
        /// The number of the next posts of the look that fail.
        post_fails: usize,
        /// The full check of a pull request is what the fake `gh` tells
        /// ([`FAKE_GH`]), as for the real look. Else each is done.
        checked: bool,
        /// The image that the wave pins, else [`IMAGE`].
        image: Option<String>,
        /// A `gh` that the look calls, with its own count of calls: the look
        /// fails when it fails (#1557). Else the look calls no `gh`.
        limit_gh: Option<Arc<pr::Gh>>,
    }

    impl Forge {
        /// The fault of the host now.
        fn host(&self) -> Result<(), run::HostFault> {
            if self.token_refused {
                return Err(run::HostFault::TokenRefused);
            }
            match self.minutes.and_then(run::HostFault::of_minutes) {
                Some(fault) => Err(fault),
                None => Ok(()),
            }
        }

        /// The head commit of the pull request of `issue`.
        fn head(&self, issue: u64) -> String {
            self.heads
                .get(&issue)
                .cloned()
                .unwrap_or_else(|| format!("c{issue}"))
        }

        /// The open milestone `Wave 1` with the items, and the open pull
        /// requests, as `gh` gives them.
        fn open(&self) -> rollout::Open {
            let wave = || rollout::Milestone {
                title: "Wave 1".into(),
            };
            let issues = self.items.iter().map(|n| rollout::Issue {
                number: *n,
                title: self
                    .titles
                    .get(n)
                    .cloned()
                    .unwrap_or_else(|| format!("item {n}")),
                body: match self.bare.contains(n) {
                    true => "No line.".into(),
                    false => "Done when: it works.".into(),
                },
                milestone: Some(wave()),
                labels: self
                    .labels
                    .get(n)
                    .into_iter()
                    .flatten()
                    .map(|name| rollout::Label { name: name.clone() })
                    .collect(),
                ..rollout::Issue::default()
            });
            let pulls = self.pulls.iter().map(|(n, pr)| rollout::Pull {
                number: 40 + n,
                branch: branch_of(*n),
                head: self.head(*n),
                checks: match pr {
                    Pr::Asked => Vec::new(),
                    Pr::Passed => vec![rollout::Check::verify("SUCCESS")],
                    Pr::Failed | Pr::Reasked => vec![rollout::Check::verify("FAILURE")],
                }
                .into_iter()
                .chain(
                    self.failing
                        .contains(n)
                        .then(|| rollout::Check::run("Gate", "FAILURE")),
                )
                .chain(
                    self.failing_other
                        .contains(n)
                        .then(|| rollout::Check::other("ci", "FAILURE", OTHER_LINK)),
                )
                .chain(
                    self.rerun
                        .contains(n)
                        .then_some([
                            rollout::Check::run("Gate", "FAILURE").started("2026-11-03T05:50:00Z"),
                            rollout::Check::run("Gate", "SUCCESS").started("2026-11-03T05:55:00Z"),
                        ])
                        .into_iter()
                        .flatten(),
                )
                .collect(),
                mergeable: Some(
                    match (self.unknown.contains(n), self.conflicting.contains(n)) {
                        (true, _) => "UNKNOWN",
                        (false, true) => "CONFLICTING",
                        (false, false) => "MERGEABLE",
                    }
                    .into(),
                ),
                auto_merge: !self.manual.contains(n),
                review: self.review.contains(n).then(|| "REVIEW_REQUIRED".into()),
                merge_state: self.approvals.contains_key(n).then(|| "BLOCKED".into()),
                waiting: self.approvals.get(n).cloned().unwrap_or_default(),
                queue: self.queue.get(n).cloned().unwrap_or_default(),
                reasked: *pr == Pr::Reasked,
                ..rollout::Pull::default()
            });
            rollout::Open {
                milestones: vec![wave()],
                issues: issues.collect(),
                pulls: pulls.collect(),
            }
        }
    }

    /// A fake [`World`]: a forge in memory, a claim that fails for a
    /// held item as on riff-server, and a session folder in `root`.
    struct Fakes {
        root: PathBuf,
        forge: Mutex<Forge>,
        /// The fake `gh` of the look ([`FAKE_GH`]).
        gh: Arc<pr::Gh>,
    }

    impl World for Fakes {
        fn look<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<Look>> {
            Box::pin(async move {
                let limit_gh = lock(&self.forge).limit_gh.clone();
                if let Some(gh) = limit_gh {
                    blocking(move || gh.run(&["pr", "list"], None)).await?;
                }
                let mut forge = lock(&self.forge);
                forge.branch_reads += 1;
                if forge.branch_unread {
                    bail!("gh api repos/o/r: HTTP 502");
                }
                // The free work of the forge, as `look_on` takes it. The
                // pick skips a held item.
                let open = forge.open();
                let claims: HashSet<String> = forge.claims.keys().cloned().collect();
                let (free, fixes) = fixes(&open, open.free_work(&claims, &HashSet::new()));
                let no_done_when = open.no_done_when(&claims, &HashSet::new());
                let pulls = match forge.checked {
                    true => waiting(&self.gh, "o/r", &free, &open)?,
                    false => forge
                        .pulls
                        .keys()
                        .map(|n| {
                            let waiting = Waiting {
                                pull: 40 + n,
                                commit: forge.head(*n),
                                gate_done: true,
                            };
                            (*n, waiting)
                        })
                        .collect(),
                };
                Ok(Look {
                    paused: false,
                    free,
                    claims: forge.claims.keys().cloned().collect(),
                    holds: forge.holds.clone(),
                    pulls,
                    fixes,
                    no_done_when: no_done_when.into_iter().collect(),
                    branch: "main".into(),
                })
            })
        }

        fn pinned<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<Pinned>> {
            let image = lock(&self.forge).image.clone();
            Box::pin(async move {
                Ok(Pinned {
                    image: image.unwrap_or_else(|| IMAGE.into()),
                    setup: Setup::default(),
                })
            })
        }

        fn host<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<(), run::HostFault>> {
            let host = lock(&self.forge).host();
            Box::pin(async move { host })
        }

        fn begin<'a>(
            &'a self,
            _: &'a Wave,
            work: &'a Work,
            pinned: &'a Pinned,
        ) -> BoxFuture<'a, Result<Option<Start>>> {
            Box::pin(async move {
                let item = work.item();
                let mut forge = lock(&self.forge);
                if forge.holds.contains(&item) || forge.claims.contains_key(&item) {
                    return Ok(None);
                }
                // As `run::host_facts` in the steps of `Live`.
                forge.host()?;
                if forge.late_faults > 0 {
                    forge.late_faults -= 1;
                    forge.refused.push(item);
                    return Err(run::HostFault::SignInEnds(42).into());
                }
                if !forge.refusals.is_empty() {
                    let code = forge.refusals.remove(0);
                    forge.refused.push(item);
                    return Err(api::Refusal {
                        op: "claim".into(),
                        status: reqwest::StatusCode::CONFLICT,
                        text: format!("refused: {code}"),
                        code: Some(code),
                    }
                    .into());
                }
                forge.sessions += 1;
                let session = format!("{:08x}", forge.sessions);
                forge.claims.insert(item.clone(), session.clone());
                forge.starts.push((item.clone(), session.clone()));
                forge.works.push(work.clone());
                let name = format!("riff-{item}-{session}");
                let folder = match forge.git {
                    true => self.root.join("sessions").join(format!("{item}-{session}")),
                    false => self.root.join("sessions").join(&name),
                };
                let clone = folder.join("clone");
                std::fs::create_dir_all(&clone)?;
                if forge.git {
                    crate::daemon::sweep::tests::origin(&clone, "https://github.com/o/r.git");
                }
                let spec = SessionSpec {
                    name,
                    image: pinned.image.clone(),
                    home: "/home/mike".into(),
                    workdir: clone.clone(),
                    binds: vec![Bind {
                        source: Source::Path(clone.clone()),
                        target: clone,
                        write: true,
                    }],
                    env: vec![Env::Host("GH_TOKEN".into())],
                    memory: "8g".into(),
                    cpus: 4,
                    command: vec!["claude".into()],
                };
                let me = format!("riff://mike@thelio/o/r?session={session}#{item}").parse()?;
                Ok(Some(Start {
                    session,
                    item,
                    folder,
                    spec,
                    start: "Do it.".into(),
                    values: vec![("GH_TOKEN".into(), "gho_secret1".into())],
                    secrets: vec!["gho_secret1".into()],
                    server: "http://127.0.0.1:1".into(),
                    me,
                    terminal: false,
                }))
            })
        }

        fn resume(&self, _: &Record) {}

        fn finish<'a>(
            &'a self,
            record: &'a Record,
            _: Ended,
        ) -> BoxFuture<'a, Result<Vec<String>>> {
            Box::pin(async move {
                let gate = {
                    let mut forge = lock(&self.forge);
                    forge.finishing.push(record.live.item.clone());
                    forge.gate.clone()
                };
                if let Some(gate) = gate {
                    gate.acquire().await?.forget();
                }
                let mut forge = lock(&self.forge);
                let job = record.job.as_ref().expect("a job");
                match job.work {
                    Work::Build { .. } if forge.no_pull => None,
                    Work::Build { issue } => forge.pulls.insert(issue, Pr::Asked),
                    Work::Verify { .. } if forge.no_result => None,
                    Work::Verify { issue, .. } => forge.pulls.insert(issue, Pr::Passed),
                    Work::Rebase { .. } if forge.no_push => None,
                    // The push of the rebase: a new head with no conflict
                    // and no verify.
                    Work::Rebase { issue, .. } => {
                        let head = format!("{}r", forge.head(issue));
                        forge.heads.insert(issue, head);
                        forge.conflicting.remove(&issue);
                        forge.pulls.insert(issue, Pr::Asked)
                    }
                    Work::Checks { .. } if forge.no_push => None,
                    // The push of the fix: a new head with no failed check
                    // and no verify.
                    Work::Checks { issue, .. } => {
                        let head = format!("{}f", forge.head(issue));
                        forge.heads.insert(issue, head);
                        forge.failing.remove(&issue);
                        forge.pulls.insert(issue, Pr::Asked)
                    }
                };
                forge.claims.remove(&record.live.item);
                forge.finished.push(record.live.item.clone());
                Ok(Vec::new())
            })
        }

        fn fate<'a>(&'a self, _: &'a str, issue: u64) -> BoxFuture<'a, Result<Fate>> {
            let fate = lock(&self.forge).fates.get(&issue).cloned();
            Box::pin(async move { Ok(fate.unwrap_or(Fate::Stays)) })
        }

        fn merged_in<'a>(
            &'a self,
            _: &'a str,
            _: u64,
            _: u64,
            _: &'a str,
        ) -> BoxFuture<'a, Result<()>> {
            Box::pin(async { Ok(()) })
        }

        fn tell<'a>(
            &'a self,
            _: &'a Wave,
            body: &'a str,
            key: Option<&'a str>,
        ) -> BoxFuture<'a, ()> {
            Box::pin(async move {
                let mut forge = lock(&self.forge);
                forge.told.push(body.to_owned());
                forge.told_keys.push(key.map(str::to_owned));
            })
        }

        fn gh(&self, _: &str) -> Arc<pr::Gh> {
            let limit_gh = lock(&self.forge).limit_gh.clone();
            limit_gh.unwrap_or_else(|| self.gh.clone())
        }

        fn claims<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<HashSet<String>>> {
            let claims = lock(&self.forge).claims.keys().cloned().collect();
            Box::pin(async move { Ok(claims) })
        }

        fn post<'a>(
            &'a self,
            _: &'a Wave,
            item: Option<&'a str>,
            body: &'a str,
            kind: Kind,
            key: Option<&'a str>,
        ) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                let mut forge = lock(&self.forge);
                if forge.post_fails > 0 {
                    forge.post_fails -= 1;
                    bail!("riff-server does not answer");
                }
                forge
                    .posts
                    .push((item.map(str::to_owned), kind, body.to_owned()));
                forge.post_keys.push(key.map(str::to_owned));
                Ok(())
            })
        }

        fn chat<'a>(&'a self, _: &'a Record, _: &'a str) -> BoxFuture<'a, ()> {
            Box::pin(async {})
        }

        fn leave<'a>(&'a self, _: &'a Record) -> BoxFuture<'a, ()> {
            Box::pin(async {})
        }

        fn lead_again<'a>(
            &'a self,
            _: Option<&'a Wave>,
            _: &'a Record,
        ) -> BoxFuture<'a, Result<Start>> {
            Box::pin(async { bail!("no lead in this test") })
        }

        fn free<'a>(&'a self, _: &'a Record) -> BoxFuture<'a, Result<()>> {
            Box::pin(async { Ok(()) })
        }

        fn wake<'a>(&'a self, _: &'a Record) -> BoxFuture<'a, Result<String>> {
            Box::pin(std::future::pending())
        }

        fn wakes<'a>(&'a self, _: &'a Record, _: mpsc::UnboundedSender<Wake>) -> BoxFuture<'a, ()> {
            Box::pin(std::future::pending())
        }

        fn refuse<'a>(&'a self, _: &'a Record, _: &'a Wake) -> BoxFuture<'a, ()> {
            Box::pin(async {})
        }
    }

    /// A daemon on the fake runtime with the fake world, in a temp
    /// folder.
    struct Here {
        _root: tempfile::TempDir,
        fake: Fake,
        world: Arc<Fakes>,
        daemon: Arc<Daemon<Fake>>,
    }

    impl Here {
        /// The wave of the open `items`, with the limit `sessions` and
        /// a floor of 8 GB, and the free memory of `free` in GB.
        async fn new(items: &[u64], sessions: u32, free: Arc<AtomicU64>) -> Here {
            Here::new_looking(items, sessions, free, crate::look::LOOK_EVERY).await
        }

        /// [`Here::new`] with a look at `main` and the pull requests each
        /// `every`.
        async fn new_looking(
            items: &[u64],
            sessions: u32,
            free: Arc<AtomicU64>,
            every: Duration,
        ) -> Here {
            let root = tempfile::tempdir().unwrap();
            let dir = dir_in(root.path());
            let fake = Fake::default().with_image(IMAGE);
            let bin = root.path().join("bin");
            std::fs::create_dir(&bin).unwrap();
            let gh = pr::Gh::at(isolated::write_program(bin.join("gh"), FAKE_GH));
            let world = Arc::new(Fakes {
                root: root.path().to_owned(),
                forge: Mutex::new(Forge {
                    items: items.to_vec(),
                    ..Forge::default()
                }),
                gh: Arc::new(gh),
            });
            let memory: crate::room::Memory =
                Arc::new(move || Some(free.load(Ordering::SeqCst) * GB));
            let relay = root.path().join("static-riff");
            riff_client::secrets::mock_store();
            let daemon = Daemon::new(fake.clone(), dir.clone())
                .unwrap()
                .with_memory(memory)
                .with_world(world.clone())
                .with_look_every(every)
                .with_relay(crate::release::fake_riff(&relay, None));
            Limit {
                sessions,
                floor_gb: 8,
                ..Limit::default()
            }
            .write(&dir)
            .unwrap();
            Here {
                _root: root,
                fake,
                world,
                daemon: Arc::new(daemon),
            }
        }

        fn run(&self) {
            self.daemon
                .run_wave(Wave {
                    repo: "o/r".into(),
                    main: "/src/r".into(),
                    server: "http://127.0.0.1:1".into(),
                    host: "thelio".into(),
                })
                .unwrap();
        }

        fn forge(&self) -> MutexGuard<'_, Forge> {
            lock(&self.world.forge)
        }

        /// Waits until `done` holds. It tells the wave to look again
        /// while it waits. At most `limit` sessions live the whole time.
        async fn until(&self, limit: usize, done: impl Fn(&Here) -> bool) {
            let started = Instant::now();
            while !done(self) {
                assert!(self.daemon.live_count() <= limit, "more than {limit} live");
                assert!(started.elapsed() < isolated::HANG, "it did not happen");
                self.daemon.tell_changed();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(self.daemon.live_count() <= limit, "more than {limit} live");
        }

        /// The name of the container of `item`, while it runs.
        fn running(&self, item: &str) -> Option<String> {
            self.daemon
                .records()
                .into_iter()
                .find(|r| r.live.item == item)
                .map(|r| r.live.name)
        }

        /// Ends the live session of `item`.
        async fn end(&self, item: &str) {
            let name = self.running(item).expect("it runs");
            self.fake.stop(&name).await.unwrap();
        }

        /// Lets the wave look for a time. At most `limit` sessions live.
        async fn idle(&self, limit: usize) {
            for _ in 0..20 {
                self.daemon.tell_changed();
                tokio::time::sleep(Duration::from_millis(20)).await;
                assert!(self.daemon.live_count() <= limit, "more than {limit} live");
            }
        }
    }

    #[tokio::test]
    async fn the_limit_of_the_host_holds_while_the_wave_runs() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2, 3], 2, free).await;
        here.run();
        here.until(2, |h| h.daemon.live_count() == 2).await;
        here.idle(2).await;
        assert_eq!(here.forge().starts.len(), 2);
        // Each end makes room for the next work: the verify of an item,
        // or the next item. Two sessions live at most.
        for n in 1..=6 {
            here.until(2, |h| !h.daemon.records().is_empty()).await;
            let item = here.daemon.records()[0].live.item.clone();
            here.end(&item).await;
            here.until(2, |h| h.forge().finished.len() == n).await;
        }
        here.idle(2).await;
        let forge = here.forge();
        assert!(
            forge.pulls.values().all(|pr| *pr == Pr::Passed),
            "{forge:?}"
        );
        assert_eq!(
            forge.starts.len(),
            6,
            "one session for each item and each verify"
        );
    }

    /// 01M53S7WDPQ2GWHJWJ6SZXZ5PN: after the end of a session, the daemon
    /// has no record of it, and the wave sweeps before each look. The
    /// folder still stays until the last step after the end of the job,
    /// which reads the folder.
    #[tokio::test]
    async fn the_folder_of_a_job_stays_until_its_last_step_after_the_end() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 1, free).await;
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        {
            let mut forge = here.forge();
            forge.git = true;
            forge.gate = Some(gate.clone());
            forge.fates.insert(1, Fate::Closed { pull: 41 });
        }
        here.run();
        here.until(1, |h| h.running("issue-1").is_some()).await;
        let folder = here.daemon.records()[0].folder.clone();
        here.end("issue-1").await;
        let finishing = |h: &Here| h.forge().finishing.contains(&"issue-1".to_owned());
        here.until(1, finishing).await;
        here.idle(1).await;

        assert!(here.running("issue-1").is_none());
        assert!(folder.exists(), "the steps after the end run");
        assert!(here.daemon.sweep(None).await.is_empty());

        gate.add_permits(1);
        here.until(1, |h| h.forge().finished.contains(&"issue-1".to_owned()))
            .await;
        here.until(1, |_| !folder.exists()).await;
    }

    /// 01M51STPHXSSZMHF9T7Z85097R: the wave of the daemon takes the
    /// merged pin of its main clone, not the pin of its working tree.
    #[tokio::test]
    async fn the_wave_takes_the_merged_pin_of_its_main_clone() {
        let pin = |n: &str| format!("ghcr.io/o/i@sha256:{}", n.repeat(64));
        let config = |n: &str| format!("[session]\nimage = \"{}\"\n", pin(n));
        let (_dir, main, _origin) = session_image::test_clone(&config("0"));
        std::fs::write(main.join(session_image::FILE), config("1")).unwrap();
        let wave = Wave {
            repo: "acme/app".into(),
            main,
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        assert_eq!(Live::default().pinned(&wave).await.unwrap().image, pin("0"));
    }

    #[tokio::test]
    async fn the_wave_starts_nothing_while_the_free_memory_is_below_the_floor() {
        let free = Arc::new(AtomicU64::new(2));
        let here = Here::new(&[1], 3, free.clone()).await;
        here.run();
        here.idle(0).await;
        assert!(here.forge().starts.is_empty());
        free.store(9, Ordering::SeqCst);
        here.until(1, |h| h.daemon.live_count() == 1).await;
    }

    /// 01M5R56GN3DC09GAPAF69GENEZ (#1404): a default branch that does not
    /// read is the error of the look. The wave starts no work at that
    /// look, says why, and reads the branch again at the next look.
    #[tokio::test]
    async fn a_default_branch_that_does_not_read_starts_no_work_and_the_next_look_reads_it_again() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().branch_unread = true;
        here.run();
        here.until(0, |h| h.forge().branch_reads >= 3).await;
        assert!(here.forge().starts.is_empty());
        let word = here.daemon.waves_shown()[0].word.clone();
        assert_eq!(word, text::wave_no_look("gh api repos/o/r: HTTP 502"));

        // The branch reads again at the next look: the work starts.
        let reads = here.forge().branch_reads;
        here.forge().branch_unread = false;
        here.until(1, |h| h.running("issue-1").is_some()).await;
        assert!(here.forge().branch_reads > reads);
        assert_eq!(here.forge().starts.len(), 1);
    }

    #[tokio::test]
    async fn a_held_item_never_starts() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        here.forge().holds.insert("issue-1".into());
        here.forge().holds.insert("verify-issue-2".into());
        here.run();
        here.until(3, |h| h.running("issue-2").is_some()).await;
        here.end("issue-2").await;
        here.until(3, |h| h.forge().finished == ["issue-2"]).await;
        here.idle(3).await;
        let forge = here.forge();
        let started: Vec<&str> = forge.starts.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(started, ["issue-2"], "no held item and no held verify");
        assert_eq!(forge.pulls.get(&2), Some(&Pr::Asked), "the verify waits");
    }

    /// A tracker issue, by its label or by its title, gets no build and
    /// no verify (01M5ND16V2SVN8A9K7W90VC6X9).
    #[tokio::test]
    async fn a_tracker_never_starts() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2, 3, 4], 4, free).await;
        {
            let mut forge = here.forge();
            forge.labels.insert(1, vec!["bug".into(), "tracker".into()]);
            forge.titles.insert(2, "Tracker: the cloud".into());
            forge.labels.insert(3, vec!["tracker".into()]);
            forge.pulls.insert(3, Pr::Asked);
        }
        here.run();
        here.until(4, |h| h.running("issue-4").is_some()).await;
        here.idle(4).await;
        let forge = here.forge();
        let started: Vec<&str> = forge.starts.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(started, ["issue-4"], "no build and no verify of a tracker");
    }

    /// 01M5R0HEP1MXWYY643F1N1DWEM: the wave starts no build of an issue
    /// whose body has no `Done when:` line, and the lead hears of it one
    /// time, not at each look. A verify is not changed. The wave takes
    /// the issue at its next look after the line comes.
    #[tokio::test]
    async fn an_issue_with_no_done_when_line_is_no_free_work_and_the_lead_hears_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2, 3], 4, free).await;
        {
            let mut forge = here.forge();
            forge.bare.extend([1, 3]);
            forge.pulls.insert(3, Pr::Asked);
        }
        here.run();
        here.until(4, |h| {
            h.running("issue-2").is_some() && h.running("verify-issue-3").is_some()
        })
        .await;
        here.idle(4).await;
        {
            let forge = here.forge();
            let mut started: Vec<&str> = forge.starts.iter().map(|(i, _)| i.as_str()).collect();
            started.sort_unstable();
            assert_eq!(
                started,
                ["issue-2", "verify-issue-3"],
                "no build of issue-1"
            );
            assert_eq!(forge.told, [text::wave_no_done_when(1)], "one message");
            assert_eq!(forge.told_keys, [Some("no-done-when-1".to_owned())]);
        }

        // The line comes: the same issue is free work at the next look.
        here.forge().bare.remove(&1);
        here.until(4, |h| h.running("issue-1").is_some()).await;
        here.idle(4).await;
        assert_eq!(here.forge().told.len(), 1, "no more message");
    }

    #[tokio::test]
    async fn an_author_never_verifies_its_own_item() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        // The author opens its pull request, and works on: no verify
        // starts while it holds the item.
        here.forge().pulls.insert(1, Pr::Asked);
        here.idle(3).await;
        assert_eq!(here.forge().starts.len(), 1);
        here.end("issue-1").await;
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let forge = here.forge();
        let [(build, author), (verify, verifier)] = &forge.starts[..] else {
            panic!("{forge:?}");
        };
        assert_eq!(
            (build.as_str(), verify.as_str()),
            ("issue-1", "verify-issue-1")
        );
        assert_ne!(author, verifier, "the verify is a new session");
    }

    /// The reason of the end of a session that the fake daemon stops.
    fn stopped() -> String {
        "no exit code: a signal, or the container ended before its relay".into()
    }

    #[tokio::test]
    async fn an_item_starts_at_most_three_times_and_the_lead_hears_of_it() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        // Each session of the item ends with no pull request.
        here.forge().no_pull = true;
        here.run();
        for n in 1..=STARTS {
            here.until(3, |h| h.running("issue-1").is_some()).await;
            here.end("issue-1").await;
            here.until(3, |h| h.forge().finished.len() == n as usize)
                .await;
        }
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.starts.len(), 3);
        assert_eq!(
            forge.told,
            [text::wave_spent("issue-1", STARTS, &stopped())]
        );
    }

    /// 01M558EYCXPFW8CJV3XJ7HK3NR: riff-server refuses the claim of each
    /// job for a pause, more times than [`STARTS`]. Such a start does not
    /// count, and the lead hears nothing of it: after the pause, the item
    /// starts.
    #[tokio::test]
    async fn a_claim_refused_for_a_pause_is_no_start_of_the_item() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        let pauses = STARTS as usize + 1;
        here.forge().refusals = vec!["paused".into(); pauses];
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        let forge = here.forge();
        assert_eq!(forge.refused, vec!["issue-1"; pauses]);
        assert_eq!(forge.starts.len(), 1);
        assert!(forge.told.is_empty(), "{:?}", forge.told);
    }

    /// 01M558EYCXPFW8CJV3XJ7HK3NR: each other refusal of the claim is a
    /// failed start: it counts, and the lead hears of it.
    #[tokio::test]
    async fn each_other_refusal_of_the_claim_counts_as_a_start() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().refusals = vec!["must_clear".into(); STARTS as usize];
        here.run();
        let reason = text::wave_no_start("claim failed (409 Conflict): refused: must_clear");
        let spent = text::wave_spent("issue-1", STARTS, &reason);
        here.until(3, |h| h.forge().told.contains(&spent)).await;
        here.until(3, |h| h.forge().told.len() == STARTS as usize + 1)
            .await;
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.refused.len(), STARTS as usize);
        assert!(forge.starts.is_empty(), "{forge:?}");
        let failed = text::wave_job_failed(
            "issue-1",
            "claim failed (409 Conflict): refused: must_clear",
        );
        let count = |text: &str| forge.told.iter().filter(|t| *t == text).count();
        assert_eq!(count(&failed), STARTS as usize, "{:?}", forge.told);
        assert_eq!(count(&spent), 1, "{:?}", forge.told);
        // The notices about one host have no key: each host tells them
        // (01M6FZY6NABXGXVRYF6W1509XE).
        assert!(
            forge.told_keys.iter().all(Option::is_none),
            "{:?}",
            forge.told_keys
        );
    }

    /// 01M57A7A1W2K9440WZA1QZ3M9T: the sign-in of `claude` on the host
    /// ends in 42 minutes. No item starts, no start counts, and the lead
    /// gets one message. When the sign-in has 60 minutes or more, the
    /// wave starts the item.
    #[tokio::test]
    async fn a_sign_in_that_ends_soon_spends_no_start_and_the_lead_hears_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        here.forge().minutes = Some(42);
        here.run();
        let why = run::HostFault::SignInEnds(42).wave_text();
        let word = text::wave_host_word(&why);
        here.until(0, |h| h.daemon.waves_shown()[0].word == word)
            .await;
        // Many looks, and a sign-in with one minute less: still one
        // message.
        here.idle(0).await;
        here.forge().minutes = Some(41);
        here.idle(0).await;
        {
            let forge = here.forge();
            assert!(forge.starts.is_empty(), "{forge:?}");
            assert!(forge.refused.is_empty(), "{forge:?}");
            assert_eq!(forge.told, [text::wave_host_waits("o/r", &why)]);
            assert!(!forge.told[0].contains("riff run "), "{:?}", forge.told);
            assert_eq!(forge.told_keys, [None]);
        }
        here.forge().minutes = Some(60);
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.until(3, |h| h.running("issue-2").is_some()).await;
        assert_eq!(here.forge().told.len(), 1, "{:?}", here.forge().told);
    }

    /// 01M58B7H7HD0H48A0C79F8M4CR: `claude` refused the token of the
    /// host. No item starts, no start counts, and the lead gets one
    /// message with the step. A new token starts the items.
    #[tokio::test]
    async fn a_refused_token_starts_nothing_and_the_lead_hears_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        here.forge().token_refused = true;
        here.run();
        let why = run::HostFault::TokenRefused.wave_text();
        let word = text::wave_host_word(&why);
        here.until(0, |h| h.daemon.waves_shown()[0].word == word)
            .await;
        here.idle(0).await;
        {
            let forge = here.forge();
            assert!(forge.starts.is_empty(), "{forge:?}");
            assert_eq!(forge.told, [text::wave_host_waits("o/r", &why)]);
            assert!(
                forge.told[0].contains("riff claude-token"),
                "{:?}",
                forge.told
            );
        }
        here.forge().token_refused = false;
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().told.len(), 1, "{:?}", here.forge().told);
    }

    /// 01M57A7A1W2K9440WZA1QZ3M9T: the sign-in ends soon after the check
    /// of the host of the loop, more times than [`STARTS`]. Such a start
    /// does not count, and the lead hears nothing of it.
    #[tokio::test]
    async fn a_fault_of_the_host_in_a_job_is_no_start_of_the_item() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        let faults = STARTS as usize + 1;
        here.forge().late_faults = faults;
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        let forge = here.forge();
        assert_eq!(forge.refused, vec!["issue-1"; faults]);
        assert_eq!(forge.starts.len(), 1);
        assert!(forge.told.is_empty(), "{:?}", forge.told);
    }

    /// 01M5SMSB3H1E9NXMBDN8ZDJ6CD: podman cannot pull the image that the
    /// wave pins, more times than [`STARTS`]. No item starts, no start
    /// counts, `riff daemon status` shows the image and the reason, and
    /// the lead hears once. When a pull works, the wave starts the item.
    #[tokio::test]
    async fn a_pull_that_fails_shows_in_the_status_and_the_lead_hears_once() {
        const PRIVATE: &str = "ghcr.io/o/private@sha256:0000";
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new_looking(&[1], 3, free, Duration::from_millis(60)).await;
        here.forge().image = Some(PRIVATE.into());
        here.run();
        let pulls = |h: &Here| {
            h.fake
                .calls()
                .iter()
                .filter(|c| c.starts_with("pull "))
                .count()
        };
        // More tries than the starts of an item: a start that fails to
        // pull does not count.
        here.until(0, |h| pulls(h) > STARTS as usize + 1).await;

        let reason = format!("the registry has no image {PRIVATE}");
        let word = text::wave_pull_word(PRIVATE, &reason);
        assert_eq!(here.daemon.waves_shown()[0].word, word);
        let shown = text::daemon_status(&here.daemon.status().await, crate::daemon::now_ms());
        assert!(
            shown.contains(&format!("Waves:\n  o/r  {word}\n")),
            "{shown}"
        );
        {
            let forge = here.forge();
            assert!(forge.starts.is_empty(), "{forge:?}");
            let told = text::wave_pull_failed("o/r", PRIVATE, &reason);
            assert_eq!(forge.told, std::slice::from_ref(&told));
            assert!(told.contains("podman login ghcr.io"), "{told}");
            assert!(told.contains("make the package public"), "{told}");
        }

        // A pull works: the item starts, the line goes, and the lead
        // hears nothing more.
        here.fake.clone().with_registry(PRIVATE);
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_ne!(here.daemon.waves_shown()[0].word, word);
        assert_eq!(here.forge().told.len(), 1, "{:?}", here.forge().told);

        // A new failure after a pull that worked is news again.
        let failed = |reason: &str| PullFailed {
            image: PRIVATE.into(),
            reason: reason.into(),
        };
        assert!(here.daemon.pull_failed("o/r", &failed("a")));
        assert!(!here.daemon.pull_failed("o/r", &failed("b")));
        here.daemon.pull_worked("o/r");
        assert!(here.daemon.pull_failed("o/r", &failed("c")));
    }

    /// The output of `claude --version` and `gh --version` in a fake
    /// image, for the version `claude` of claude and `gh` of gh.
    fn tools(here: &Here, claude: &str, gh: &str) {
        here.fake
            .clone()
            .with_tool(IMAGE, "claude", &format!("{claude} (Claude Code)\n"))
            .with_tool(
                IMAGE,
                "gh",
                &format!("gh version {gh} (2026-01-01)\nhttps://github.com/cli/cli\n"),
            );
    }

    /// 01M5SNTPHHXP54YBTV09JNRY3F: an image with a `claude` older than
    /// riff needs makes one message to the lead with the `riff-base` to
    /// build `FROM`, also when the wave starts more than one job.
    #[tokio::test]
    async fn an_image_with_an_old_tool_tells_the_lead_once_with_the_new_base() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        tools(
            &here,
            "0.0.1",
            &crate::image_tools::Tool::Gh.needed().to_string(),
        );
        here.run();
        here.until(3, |h| {
            h.running("issue-1").is_some() && h.running("issue-2").is_some()
        })
        .await;
        here.idle(3).await;
        let old = crate::image_tools::Old {
            tool: crate::image_tools::Tool::Claude,
            has: "0.0.1".parse().unwrap(),
            needs: crate::image_tools::Tool::Claude.needed(),
        };
        let base = crate::session_image::base();
        assert_eq!(
            here.forge().told,
            [text::image_old_tools("o/r", &[old], &base)]
        );
        assert!(here.forge().told[0].contains(&format!("FROM {base}")));
    }

    /// 01M5SNTPHHXP54YBTV09JNRY3F: both tools old make one message that
    /// names both.
    #[tokio::test]
    async fn an_image_with_two_old_tools_makes_one_message_for_both() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        tools(&here, "0.0.1", "0.0.2");
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        let told = here.forge().told.clone();
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(told[0].contains("claude 0.0.1 and gh 0.0.2"), "{told:?}");
    }

    /// 01M5SNTPHHXP54YBTV09JNRY3F: an image with tools that are new enough,
    /// an image where a tool prints no version, and an image that does
    /// not run the tools, tell the lead nothing, and the job goes on.
    #[tokio::test]
    async fn an_image_with_new_tools_or_no_version_tells_the_lead_nothing() {
        let current = |h: &Here| {
            let (claude, gh) = (
                crate::image_tools::Tool::Claude,
                crate::image_tools::Tool::Gh,
            );
            tools(h, &claude.needed().to_string(), &gh.needed().to_string());
        };
        for case in ["new enough", "newer", "no version", "no tools"] {
            let free = Arc::new(AtomicU64::new(64));
            let here = Here::new(&[1], 3, free).await;
            match case {
                "new enough" => current(&here),
                "newer" => tools(&here, "999.0.0", "999.0.0"),
                "no version" => {
                    here.fake
                        .clone()
                        .with_tool(IMAGE, "claude", "unknown\n")
                        .with_tool(IMAGE, "gh", "");
                }
                _ => {}
            }
            here.run();
            here.until(3, |h| h.running("issue-1").is_some()).await;
            here.idle(3).await;
            let told = here.forge().told.clone();
            assert!(told.is_empty(), "{case}: {told:?}");
        }
    }

    /// The lead of `o/r` on this host, as the record of its session.
    fn lead_record() -> Record {
        let wave = Wave {
            repo: "o/r".into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        let who = riff_core::name::Who::new("mike", Some("lead1")).unwrap();
        Record {
            live: riff_client::sessions::Live {
                digest: String::new(),
                name: "riff-lead-o-r".into(),
                session: "lead1".into(),
                item: "lead".into(),
                image: IMAGE.into(),
                since_ms: 0,
                phase: Default::default(),
                last: String::new(),
                lead: true,
            },
            folder: PathBuf::new(),
            server: wave.server.clone(),
            me: SessionUri::new(who, wave.place(None).unwrap()),
            inputs: Vec::new(),
            closed: false,
            failed: false,
            job: None,
            terminal: true,
            takeover: false,
            interrupt: false,
        }
    }

    /// The wave of `o/r` in the file of the daemon, with no loop.
    fn keep_wave(here: &Here) {
        let wave = Wave {
            repo: "o/r".into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        write_waves(here.daemon.dir(), &[("o/r".to_owned(), wave)].into()).unwrap();
    }

    /// #1338: `riff run ITEM` in the container of the lead. The daemon
    /// runs the item as a job of the wave, with no loop of the wave:
    /// the claim, the session and the steps after its end. The answer
    /// comes at once.
    #[tokio::test]
    async fn the_lead_runs_an_item_as_a_job_with_no_loop_of_the_wave() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 2, free).await;
        keep_wave(&here);
        let line = here.daemon.run_item(&lead_record(), "issue-5").unwrap();
        assert_eq!(line, serde_json::json!(text::run_asked("issue-5")));
        here.until(2, |h| h.running("issue-5").is_some()).await;
        assert_eq!(here.forge().works, [Work::Build { issue: 5 }]);
        // The item runs: a second ask is refused.
        let again = here.daemon.run_item(&lead_record(), "issue-5");
        assert_eq!(again.unwrap_err().to_string(), text::run_already("issue-5"));
        here.end("issue-5").await;
        here.until(2, |h| h.forge().finished == ["issue-5"]).await;
        assert_eq!(here.forge().pulls.get(&5), Some(&Pr::Asked));
        assert!(here.forge().told.is_empty(), "{:?}", here.forge().told);
    }

    /// #1338: the daemon refuses a session that is not the lead, an item
    /// that is not an issue, and a repository with no wave.
    #[tokio::test]
    async fn the_daemon_refuses_the_run_of_a_work_session_and_of_an_unknown_repository() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 2, free).await;
        let mut worker = lead_record();
        worker.terminal = false;
        let refused = here.daemon.run_item(&worker, "issue-5").unwrap_err();
        assert_eq!(refused.to_string(), text::DAEMON_RUN_ONLY_LEAD);
        let lead = lead_record();
        let refused = here.daemon.run_item(&lead, "verify-issue-5").unwrap_err();
        assert_eq!(
            refused.to_string(),
            text::run_not_an_issue("verify-issue-5")
        );
        let refused = here.daemon.run_item(&lead, "issue-5").unwrap_err();
        assert_eq!(refused.to_string(), text::run_no_wave("o/r"));
        assert!(here.forge().starts.is_empty());
    }

    /// #1338: the daemon serves the repository of the lead, and the wave
    /// is not on: the job uses the wave of the file of the repositories
    /// that it serves.
    #[tokio::test]
    async fn a_run_uses_the_repository_that_the_daemon_serves_when_no_wave_runs() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 2, free).await;
        let wave = Wave {
            repo: "o/r".into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        crate::daemon::remote::serve_in_file(here.daemon.dir(), &wave).unwrap();
        here.daemon.run_item(&lead_record(), "issue-5").unwrap();
        here.until(2, |h| h.running("issue-5").is_some()).await;
    }

    /// #1338: a job that the lead asked for has no next look of a loop.
    /// So each case that stops its start tells the lead why: a hold or
    /// another claim, a pause, a fault of the host, and an error. The
    /// start of the loop of a wave tells less.
    #[tokio::test]
    async fn the_lead_hears_why_the_run_of_an_item_did_not_start() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 2, free).await;
        keep_wave(&here);
        let lead = lead_record();
        let told = |n: usize| {
            let here = &here;
            async move {
                here.until(2, |h| h.forge().told.len() == n).await;
                here.forge().told[n - 1].clone()
            }
        };

        here.forge().holds.insert("issue-5".to_owned());
        here.daemon.run_item(&lead, "issue-5").unwrap();
        let body = told(1).await;
        assert_eq!(body, text::run_job_failed("issue-5", text::RUN_NOT_CLAIMED));
        here.forge().holds.clear();

        here.forge().refusals = vec!["paused".into()];
        here.daemon.run_item(&lead, "issue-5").unwrap();
        let body = told(2).await;
        assert_eq!(body, text::run_job_failed("issue-5", text::RUN_PAUSED));

        here.forge().late_faults = 1;
        here.daemon.run_item(&lead, "issue-6").unwrap();
        let body = told(3).await;
        let fault = run::HostFault::SignInEnds(42).wave_text();
        assert_eq!(body, text::run_job_failed("issue-6", &fault));

        here.forge().refusals = vec!["not_allowed".into()];
        here.daemon.run_item(&lead, "issue-7").unwrap();
        let body = told(4).await;
        assert!(
            body.starts_with("The riff daemon started no session for issue-7: ")
                && body.contains("not_allowed"),
            "{body}"
        );
        // None of them started a session, and none counted.
        assert!(here.forge().starts.is_empty());
    }

    /// #1338: when the loop of the wave runs, the job of a run joins its
    /// items, so the loop does not pick the item and a second ask meets
    /// the first at once.
    #[tokio::test]
    async fn a_run_joins_the_items_of_the_loop_of_the_wave() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 2, free).await;
        here.run();
        let lead = lead_record();
        here.daemon.run_item(&lead, "issue-5").unwrap();
        let again = here.daemon.run_item(&lead, "issue-5").unwrap_err();
        assert_eq!(again.to_string(), text::run_already("issue-5"));
        here.until(2, |h| h.running("issue-5").is_some()).await;
        assert_eq!(here.forge().starts.len(), 1);
    }

    /// 01M57A7A48A70Y7PTD95A3MBYC: a spent item starts again after a
    /// second `riff wave run`, and the lead hears of it again when it is
    /// spent again.
    #[tokio::test]
    async fn a_second_wave_run_clears_the_spent_starts() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().no_pull = true;
        here.run();
        let mut n = 0;
        let mut once = async |here: &Here| {
            for _ in 0..STARTS {
                n += 1;
                here.until(3, |h| h.running("issue-1").is_some()).await;
                here.end("issue-1").await;
                here.until(3, |h| h.forge().finished.len() == n).await;
            }
            here.idle(3).await;
        };
        once(&here).await;
        assert_eq!(here.forge().starts.len(), 3);
        here.run();
        once(&here).await;
        let forge = here.forge();
        assert_eq!(forge.starts.len(), 6, "{forge:?}");
        let spent = text::wave_spent("issue-1", STARTS, &stopped());
        assert_eq!(forge.told, [spent.clone(), spent]);
    }

    /// The wave of the item 1, whose pull request #41 has the verify
    /// `pr`, auto-merge on and a conflict with `main` at `c1`.
    async fn conflict(pr: Pr) -> Here {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().pulls.insert(1, pr);
        here.forge().conflicting.insert(1);
        here
    }

    fn rebase(commit: &str) -> Work {
        Work::Rebase {
            issue: 1,
            pull: 41,
            commit: commit.into(),
        }
    }

    /// 01M57FJ3WWMQSDBFRRHXN3SMFQ: a pull request with a passed verify
    /// and a conflict gets one rebase for its head. The same head gets no
    /// second session. A new head with a conflict is a new conflict.
    #[tokio::test]
    async fn a_passed_pull_request_with_a_conflict_gets_one_rebase_for_each_head() {
        let here = conflict(Pr::Passed).await;
        here.forge().no_push = true;
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [rebase("c1")]);
        here.end("issue-1").await;
        here.until(3, |h| h.forge().finished == ["issue-1"]).await;
        here.idle(3).await;
        {
            let forge = here.forge();
            assert_eq!(
                forge.works,
                [rebase("c1")],
                "the same head: no second session"
            );
            assert!(forge.told.is_empty(), "{:?}", forge.told);
        }
        // main moves again, and a person pushed a new head with a conflict.
        here.forge().heads.insert(1, "c1b".into());
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [rebase("c1"), rebase("c1b")]);
    }

    /// #1157: a FAIL for the head C, then a new verify request for C: the
    /// next start is a verify, not a build. With no new request, the
    /// item is free for a build.
    #[tokio::test]
    async fn a_new_verify_request_after_a_fail_starts_a_verify_not_a_build() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free.clone()).await;
        here.forge().pulls.insert(1, Pr::Reasked);
        here.run();
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1".into(),
        };
        assert_eq!(here.forge().works, [verify]);

        let here = Here::new(&[1], 3, free).await;
        here.forge().pulls.insert(1, Pr::Failed);
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [Work::Build { issue: 1 }]);
    }

    /// The message to the lead about the pull request #41 of the item 1
    /// at the head `commit`.
    fn stuck(commit: &str, why: rollout::Stuck) -> String {
        text::wave_stuck("o/r", 41, "worktree-issue-1", "main", commit, &why)
    }

    /// 01M57FJ3WWMQSDBFRRHXN3SMFQ, #279: the conflict of a pull request
    /// with auto-merge off gets no rebase: no merge waits for it. The lead
    /// hears of the auto-merge, one time.
    #[tokio::test]
    async fn a_pull_request_with_auto_merge_off_gets_no_rebase() {
        let here = conflict(Pr::Passed).await;
        here.forge().manual.insert(1);
        here.run();
        here.idle(3).await;
        let forge = here.forge();
        assert!(forge.works.is_empty(), "{forge:?}");
        assert_eq!(forge.told, [stuck("c1", rollout::Stuck::AutoMergeOff)]);
    }

    /// #279: a pull request with a passed verify and auto-merge off never
    /// merges. The wave starts no session, and tells the lead one time for
    /// each head, with the reason and the command. A pull request with
    /// auto-merge on gets no message.
    #[tokio::test]
    async fn a_verified_pull_request_with_auto_merge_off_tells_the_lead_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.pulls.insert(2, Pr::Passed);
            forge.manual.insert(1);
        }
        here.run();
        here.idle(3).await;
        let message = stuck("c1", rollout::Stuck::AutoMergeOff);
        assert!(
            message.ends_with("gh pr merge --auto --squash 41"),
            "{message}"
        );
        assert_eq!(
            here.forge().told,
            std::slice::from_ref(&message),
            "once, for 1 only"
        );
        assert!(here.forge().works.is_empty());

        // A new head is a new state.
        here.forge().heads.insert(1, "c1b".into());
        here.idle(3).await;
        let second = stuck("c1b", rollout::Stuck::AutoMergeOff);
        assert_eq!(here.forge().told, [message, second]);
        // The key has the pull request, the head and the reason: a new head
        // is a new key.
        assert_eq!(
            here.forge().told_keys,
            [
                Some("stuck-#41@c1:auto-merge off".to_owned()),
                Some("stuck-#41@c1b:auto-merge off".to_owned())
            ]
        );
        assert!(here.forge().works.is_empty());
    }

    /// 01M6FZ141Y0J8G2AYX0G5HY1CX: a pull request with a passed verify that GitHub holds for an
    /// approving review (`REVIEW_REQUIRED`) never merges by itself. The
    /// wave starts no session, and tells the lead one time for each head,
    /// with the reason and the step of a person. A pull request with no
    /// review rule gets no message.
    #[tokio::test]
    async fn a_verified_pull_request_that_needs_a_review_tells_the_lead_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.pulls.insert(2, Pr::Passed);
            forge.review.insert(1);
        }
        here.run();
        here.idle(3).await;
        let required = rollout::Stuck::Review(rollout::Review::Required);
        let message = stuck("c1", required.clone());
        assert!(message.ends_with("gh pr review 41 --approve"), "{message}");
        assert_eq!(
            here.forge().told,
            std::slice::from_ref(&message),
            "once, for 1 only"
        );
        assert!(here.forge().works.is_empty());

        // A new head is a new state.
        here.forge().heads.insert(1, "c1b".into());
        here.idle(3).await;
        let second = stuck("c1b", required);
        assert_eq!(here.forge().told, [message, second]);
        assert!(here.forge().works.is_empty());
    }

    /// 01M6FZ144M0BTEDWNB9V4MGWDD: a run of a pull request from a fork waits for the approval
    /// of a maintainer. The pull request has a passed verify, and GitHub
    /// blocks the merge. The lead hears of each run one time, with the run
    /// and the command. A new run for the same head is a new message.
    #[tokio::test]
    async fn a_run_that_waits_for_approval_tells_the_lead_once_for_each_run() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        let run = |id, name: &str| rollout::Run {
            id,
            name: name.into(),
            link: Some(format!("https://github.com/o/r/actions/runs/{id}")),
        };
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.approvals.insert(1, vec![run(40151175338, "Hygiene")]);
        }
        here.run();
        here.idle(3).await;
        let first = stuck("c1", rollout::Stuck::Approve(run(40151175338, "Hygiene")));
        assert!(
            first.contains("the run Hygiene (run 40151175338)"),
            "{first}"
        );
        assert!(
            first.ends_with("gh api -X POST repos/o/r/actions/runs/40151175338/approve"),
            "{first}"
        );
        assert_eq!(here.forge().told, std::slice::from_ref(&first), "once");
        assert!(here.forge().works.is_empty());

        // A second run waits for the same head: one more message. The
        // first run does not come again.
        here.forge().approvals.insert(
            1,
            vec![run(40151175339, "Gate"), run(40151175338, "Hygiene")],
        );
        here.idle(3).await;
        let second = stuck("c1", rollout::Stuck::Approve(run(40151175339, "Gate")));
        assert_eq!(here.forge().told, [first, second]);
        assert!(here.forge().works.is_empty());
    }

    /// 01M5TKPNDET56VR8WZ74FSR71F: a pull request in the merge queue gets
    /// no rebase and no message, also with a conflict or with auto-merge
    /// off. GitHub builds its merge group and merges it, or removes it.
    #[tokio::test]
    async fn a_queued_pull_request_gets_no_rebase_and_no_message() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.pulls.insert(2, Pr::Passed);
            forge.conflicting.insert(1);
            forge.manual.insert(2);
            forge.queue.insert(1, rollout::Queue::In);
            forge.queue.insert(2, rollout::Queue::In);
        }
        here.run();
        here.idle(3).await;
        let forge = here.forge();
        assert!(forge.works.is_empty(), "{forge:?}");
        assert!(forge.told.is_empty(), "{forge:?}");
    }

    /// 01M5TKPNGG27Z1SQKTA296D9H7: a pull request that the queue removed
    /// and that has a conflict gets one rebase for its head, also with
    /// auto-merge off.
    #[tokio::test]
    async fn a_pull_request_that_the_queue_removed_with_a_conflict_gets_a_rebase() {
        let here = conflict(Pr::Passed).await;
        {
            let mut forge = here.forge();
            forge.manual.insert(1);
            forge.no_push = true;
            forge
                .queue
                .insert(1, rollout::Queue::Removed("MERGE_CONFLICT".into()));
        }
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.works, [rebase("c1")], "one rebase for the head");
        assert!(
            !forge
                .told
                .contains(&stuck("c1", rollout::Stuck::AutoMergeOff)),
            "{forge:?}"
        );
    }

    /// 01M5TKPNGG27Z1SQKTA296D9H7: a pull request that the queue removed
    /// and that has no conflict gets no session. The lead hears of it one
    /// time for each head, with the reason of GitHub. After a push the
    /// queue has no removal for the new head.
    #[tokio::test]
    async fn a_pull_request_that_the_queue_removed_tells_the_lead_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        let removed = rollout::Stuck::Dequeued("CI_FAILURE".into());
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge
                .queue
                .insert(1, rollout::Queue::Removed("CI_FAILURE".into()));
        }
        here.run();
        here.idle(3).await;
        let message = stuck("c1", removed.clone());
        assert!(message.contains("(reason: CI_FAILURE)"), "{message}");
        assert!(
            message.ends_with("gh pr merge --auto --squash 41"),
            "{message}"
        );
        assert_eq!(here.forge().told, std::slice::from_ref(&message));
        assert!(here.forge().works.is_empty());

        // A push makes a new head, and the queue has no removal for it.
        here.forge().heads.insert(1, "c1b".into());
        here.forge().queue.clear();
        here.idle(3).await;
        assert_eq!(here.forge().told, [message], "no second message");
        assert!(here.forge().works.is_empty());
    }

    fn checks(commit: &str) -> Work {
        Work::Checks {
            issue: 1,
            pull: 41,
            commit: commit.into(),
            checks: vec!["Gate".into()],
        }
    }

    /// #279: a pull request with a passed verify and a failed check gets
    /// one build session for each head, with the name of the check. The
    /// same head gets no second session, and no message: the pull request
    /// is in the group `stuck`. A new head with a failed check is a new
    /// state.
    #[tokio::test]
    async fn a_failed_check_after_a_pass_gets_one_build_for_each_head() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.failing.insert(1);
            forge.no_push = true;
        }
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [checks("c1")]);
        here.end("issue-1").await;
        here.until(3, |h| h.forge().finished == ["issue-1"]).await;
        here.idle(3).await;
        {
            let forge = here.forge();
            assert_eq!(
                forge.works,
                [checks("c1")],
                "the same head: no second session"
            );
            assert!(forge.told.is_empty(), "{:?}", forge.told);
        }
        // A person pushed a new head, and its Gate failed too.
        here.forge().heads.insert(1, "c1b".into());
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [checks("c1"), checks("c1b")]);
    }

    /// #1540, 01M5RGM8HBVQMY25H5PYESK17J: the rollup keeps an old failed
    /// run of a check next to the newer run of it that passed. The wave
    /// reads the newest run only: such a pull request gets no fix session
    /// and no message, as a pull request with no failed check. A pull
    /// request with a failed check, in the same look, gets its session.
    #[tokio::test]
    async fn an_old_failed_run_of_a_check_that_passed_again_gets_no_session_and_no_message() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.failing.insert(1);
            forge.pulls.insert(2, Pr::Passed);
            forge.rerun.insert(2);
        }
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(
            forge.works,
            [checks("c1")],
            "no session for the pull request #42"
        );
        assert!(forge.told.is_empty(), "{:?}", forge.told);
    }

    /// #279: the fix pushes a new head with no failed check: the new head
    /// waits for its verify, and a new session verifies it.
    #[tokio::test]
    async fn the_fix_of_a_failed_check_is_verified_by_a_new_session() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().pulls.insert(1, Pr::Passed);
        here.forge().failing.insert(1);
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.end("issue-1").await;
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1f".into(),
        };
        assert_eq!(here.forge().works, [checks("c1"), verify]);
    }

    /// #279: a pull request with a conflict and a failed check gets the
    /// rebase first. A held item gets no fix of its checks. A failed
    /// verify is no fix of checks. A failed check with no verify yet gets
    /// its fix, and no verify (#1575).
    #[tokio::test]
    async fn a_conflict_comes_before_the_failed_check_and_a_hold_stops_the_fix() {
        let here = conflict(Pr::Passed).await;
        here.forge().failing.insert(1);
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [rebase("c1")]);

        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2, 3], 3, free).await;
        {
            let mut forge = here.forge();
            for n in 1..=3 {
                forge.failing.insert(n);
            }
            forge.pulls.insert(1, Pr::Passed);
            forge.pulls.insert(2, Pr::Asked);
            forge.pulls.insert(3, Pr::Failed);
            forge.holds.insert("issue-1".into());
            forge.items.retain(|n| *n != 3);
        }
        here.run();
        here.until(3, |h| h.running("issue-2").is_some()).await;
        here.idle(3).await;
        let forge = here.forge();
        let fix = Work::Checks {
            issue: 2,
            pull: 42,
            commit: "c2".into(),
            checks: vec!["Gate".into()],
        };
        assert_eq!(
            forge.works,
            [fix],
            "no verify of the head with a failed check"
        );
    }

    /// #1575: the pull request waits for a verify, and a check of its head
    /// failed (the job `Hygiene` failed, and the `Gate` has no run yet).
    /// The wave starts the fix of the check, not the verify. The fix
    /// pushes a new head with no failed check, and a new session verifies
    /// it.
    #[tokio::test]
    async fn a_failed_check_before_the_verify_gets_the_fix_and_then_the_verify() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().pulls.insert(1, Pr::Asked);
        here.forge().failing.insert(1);
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [checks("c1")], "no verify of c1");
        here.end("issue-1").await;
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1f".into(),
        };
        assert_eq!(here.forge().works, [checks("c1"), verify]);
        assert!(here.forge().told.is_empty(), "{:?}", here.forge().told);
    }

    /// #1575: the fix of the failed check pushes nothing, and the check
    /// still fails: the wave starts no verify of the head, and no second
    /// fix for it.
    #[tokio::test]
    async fn a_failed_check_before_the_verify_that_stays_failed_starts_no_verify() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Asked);
            forge.failing.insert(1);
            forge.no_push = true;
        }
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.end("issue-1").await;
        here.until(3, |h| h.forge().finished == ["issue-1"]).await;
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.works, [checks("c1")], "no verify, no second fix");
    }

    /// #279: no session builds a release. A failed check of a release
    /// pull request tells the lead, one time, at any state of its verify.
    #[tokio::test]
    async fn a_failed_check_of_a_release_tells_the_lead_and_starts_no_session() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.labels.insert(1, vec!["release".into()]);
            forge.pulls.insert(1, Pr::Asked);
            forge.failing.insert(1);
        }
        here.run();
        here.idle(3).await;
        let forge = here.forge();
        assert!(forge.works.is_empty(), "{forge:?}");
        let checks = rollout::Stuck::Checks(vec!["Gate".into()]);
        assert_eq!(forge.told, [stuck("c1", checks)]);
    }

    /// The message to the lead about the failed check `ci` of another app
    /// than GitHub Actions, at the head `commit` of the pull request #41.
    fn other(commit: &str) -> String {
        let check = rollout::OtherCheck {
            name: "ci".into(),
            app: Some("app.circleci.com".into()),
            link: Some(OTHER_LINK.into()),
        };
        stuck(commit, rollout::Stuck::Other(check))
    }

    /// #1427: a failed check that is no run of GitHub Actions has no log
    /// for `gh run view`. The wave starts no session for it. The lead gets
    /// one message with the check, its app and its link. A new head is a
    /// new state.
    #[tokio::test]
    async fn a_failed_check_of_another_app_starts_no_session_and_tells_the_lead_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.failing_other.insert(1);
        }
        here.run();
        here.idle(3).await;
        let message = other("c1");
        assert!(message.contains(OTHER_LINK), "{message}");
        assert!(message.contains("app.circleci.com"), "{message}");
        {
            let forge = here.forge();
            assert!(forge.works.is_empty(), "{:?}", forge.works);
            assert_eq!(forge.told, std::slice::from_ref(&message), "one message");
        }
        here.idle(3).await;
        assert_eq!(here.forge().told.len(), 1, "once");

        // A new head with a failed check is a new state.
        here.forge().heads.insert(1, "c1b".into());
        here.idle(3).await;
        let forge = here.forge();
        assert!(forge.works.is_empty(), "{:?}", forge.works);
        assert_eq!(forge.told, [message, other("c1b")]);
    }

    /// #1427: a pull request with a failed run of GitHub Actions and a
    /// failed check of another app gets the build for the run, and the
    /// lead hears of the other check.
    #[tokio::test]
    async fn a_failed_run_of_actions_still_starts_one_session_beside_a_check_of_another_app() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.failing.insert(1);
            forge.failing_other.insert(1);
            forge.no_push = true;
        }
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.until(3, |h| !h.forge().told.is_empty()).await;
        let forge = here.forge();
        assert_eq!(forge.works, [checks("c1")], "the name of the run only");
        assert_eq!(forge.told, [other("c1")]);
    }

    /// 01M57FJ3WWMQSDBFRRHXN3SMFQ: a pull request that waits for a verify
    /// and has a conflict gets the rebase, not the verify of the old head.
    /// After the push, a new session verifies the new head: it wrote
    /// neither side.
    #[tokio::test]
    async fn the_rebase_comes_before_the_verify_and_a_new_session_verifies_the_new_head() {
        let here = conflict(Pr::Asked).await;
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.end("issue-1").await;
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let forge = here.forge();
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1r".into(),
        };
        assert_eq!(forge.works, [rebase("c1"), verify]);
        let [(_, rebaser), (_, verifier)] = &forge.starts[..] else {
            panic!("{forge:?}");
        };
        assert_ne!(rebaser, verifier, "the verify is a new session");
    }

    /// #1464, #1468: the cause of three verifies of a head with a
    /// conflict. When a merge moves `main`, GitHub gives `UNKNOWN` for the
    /// other open pull requests until it has looked at them. `UNKNOWN` is
    /// no conflict ([`rollout::Pull::conflict`]), so the pull request was
    /// free work as a verify. Each verify ended with no result, the pull
    /// request still waited, and the next look started it again: three
    /// starts for one head. The wave starts no verify while the state is
    /// `UNKNOWN`. The next look reads `CONFLICTING`: one rebase, no
    /// verify.
    #[tokio::test]
    async fn a_pull_request_that_reads_unknown_gets_no_verify_and_then_the_rebase() {
        let here = conflict(Pr::Asked).await;
        {
            let mut forge = here.forge();
            forge.unknown.insert(1);
            forge.no_result = true;
        }
        here.run();
        here.idle(3).await;
        assert!(here.forge().works.is_empty(), "{:?}", here.forge());
        // The next look: GitHub knows that the head has a conflict.
        here.forge().unknown.clear();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.end("issue-1").await;
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1r".into(),
        };
        assert_eq!(here.forge().works, [rebase("c1"), verify]);
    }

    /// #1464: the next look reads `MERGEABLE`: the verify starts for the
    /// head, one time.
    #[tokio::test]
    async fn a_pull_request_that_reads_unknown_gets_its_verify_when_it_reads_mergeable() {
        let here = Here::new(&[1], 3, Arc::new(AtomicU64::new(64))).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Asked);
            forge.unknown.insert(1);
        }
        here.run();
        here.idle(3).await;
        assert!(here.forge().works.is_empty(), "{:?}", here.forge());
        here.forge().unknown.clear();
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1".into(),
        };
        assert_eq!(here.forge().works, [verify]);
    }

    /// 01M57FJ3WWMQSDBFRRHXN3SMFQ, 01M558EYCXPFW8CJV3XJ7HK3NR: a claim of
    /// the rebase that riff-server refused for a pause is no start: after
    /// the pause, the rebase of the same head starts.
    #[tokio::test]
    async fn a_rebase_refused_for_a_pause_starts_after_the_pause() {
        let here = conflict(Pr::Passed).await;
        here.forge().refusals = vec!["paused".into(); 2];
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        let forge = here.forge();
        assert_eq!(forge.refused, ["issue-1", "issue-1"]);
        assert_eq!(forge.works, [rebase("c1")]);
    }

    /// 01M57FJ3WWMQSDBFRRHXN3SMFQ, 01M57A7A48A70Y7PTD95A3MBYC: `riff wave
    /// run` clears the start of a rebase too: the same head starts again.
    #[tokio::test]
    async fn a_second_wave_run_starts_the_rebase_of_the_same_head_again() {
        let here = conflict(Pr::Passed).await;
        here.forge().no_push = true;
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.end("issue-1").await;
        here.until(3, |h| h.forge().finished.len() == 1).await;
        here.idle(3).await;
        assert_eq!(here.forge().works.len(), 1);
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [rebase("c1"), rebase("c1")]);
    }

    #[tokio::test]
    async fn a_stop_of_the_wave_starts_no_new_session_and_the_sessions_go_on() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 1, free).await;
        here.run();
        here.until(1, |h| h.running("issue-1").is_some()).await;
        assert!(here.daemon.stop_wave("o/r").unwrap());
        assert!(here.running("issue-1").is_some(), "the session goes on");
        here.end("issue-1").await;
        here.until(1, |h| h.forge().finished == ["issue-1"]).await;
        here.idle(1).await;
        assert_eq!(here.forge().starts.len(), 1);
        assert!(!here.daemon.stop_wave("o/r").unwrap());
    }

    /// The run of the Gate in the fake forge.
    const RUN: &str = "https://github.com/o/r/actions/runs/38861280871";

    /// A fake `gh` for the look: it logs each call to `gh.log` on one
    /// line. The file `gate` holds the check runs of the head of
    /// `main`, `jobs` the jobs of its run, `pulls` the open pull
    /// requests, and `issues` the open issues. `issue list` gives the file
    /// `with-fix` after `issue create` and before `issue close`. A file
    /// that is missing gives `[]`: no Gate, no pull request. The repository
    /// `o/r` has `main` as its default branch. The file `rules` holds the
    /// rules of `main`, and the file `docs` the check runs of the check
    /// `checks / docs` on any commit but `main`. The other checks of a
    /// commit passed, and `Gate` has no run.
    const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s' "$*" | tr '\n' ' ' >> "$dir/gh.log"
printf '\n' >> "$dir/gh.log"
show() { if [ -e "$dir/$1" ]; then cat "$dir/$1"; else echo '[]'; fi; }
runs() { if [ -e "$dir/$1" ]; then cat "$dir/$1"; else echo '{"check_runs":[]}'; fi; }
case "$1 $2" in
  api*)
    case "$2" in
      repos/o/r)
        case "$*" in
          *--jq*) echo main ;;
          *) echo '{"default_branch":"main"}' ;;
        esac ;;
      */rules/branches/main*) show rules ;;
      */commits/main/check-runs*) runs gate ;;
      *check_name=Gate) echo '{"check_runs":[]}' ;;
      *check_name=checks%20%2F%20docs) show docs ;;
      *check_name=checks%20%2F%20*) echo '{"check_runs":[{"status":"completed","conclusion":"success"}]}' ;;
      */actions/runs/*/jobs*) show jobs ;;
      */milestones*) echo '[{"title":"Wave 1"}]' ;;
      *) exit 1 ;;
    esac ;;
  "pr list") show pulls ;;
  "issue list")
    if [ -e "$dir/created" ] && [ ! -e "$dir/closed" ]; then show with-fix; else show issues; fi ;;
  "label create") ;;
  "issue create") touch "$dir/created"; echo "https://github.com/o/r/issues/901" ;;
  "issue comment") ;;
  "issue close") touch "$dir/closed" ;;
  *) exit 1 ;;
esac
"#;

    /// The open issues of the fake forge, and the same with the fix item.
    const ISSUES: &str =
        r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]}]"#;
    const WITH_FIX: &str = r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":901,"body":"Needs: nothing","comments":[],"milestone":{"title":"Wave 1"},"labels":[{"name":"broken-main"}]}]"#;

    /// The rules of `main` in the repository of riff: the job `Gate` and the
    /// verify.
    const GATE_RULES: &str = r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[
        {"context":"Gate"},{"context":"riff/verify"}]}}]"#;

    /// The failed jobs of the run.
    const JOBS: &str =
        r#"[{"name":"Clippy","conclusion":"failure"},{"name":"Gate","conclusion":"failure"}]"#;

    /// The Gate of the head `commit` of `main` ended with `conclusion`.
    fn gate(commit: &str, conclusion: &str) -> String {
        format!(
            r#"{{"check_runs":[{{"status":"completed","conclusion":"{conclusion}","head_sha":"{commit}","html_url":"{RUN}/job/1"}}]}}"#
        )
    }

    /// The pull request of issue 12 with auto-merge on and a conflict, at
    /// the head `head`.
    fn conflicting(head: &str) -> String {
        format!(
            r#"[{{"number":52,"headRefName":"worktree-issue-12","headRefOid":"{head}","isDraft":false,"statusCheckRollup":[],"mergeable":"CONFLICTING","autoMergeRequest":{{"enabledAt":"2026-10-24T00:00:00Z"}},"body":"Closes #12\n"}}]"#
        )
    }

    impl Here {
        /// A look each 20 ms, with the fake forge of `gh` and no work.
        async fn looking() -> Here {
            let free = Arc::new(AtomicU64::new(64));
            let here = Here::new_looking(&[], 1, free, Duration::from_millis(20)).await;
            here.put("issues", ISSUES);
            here.put("with-fix", WITH_FIX);
            here.put("jobs", JOBS);
            // The ruleset of riff: the job Gate and the verify. A repository with
            // no required check has no full check (01M5SMNKDWBD1J6Y18ENQG87CK).
            here.put("rules", GATE_RULES);
            here
        }

        /// Sets the file `name` of the fake forge.
        fn put(&self, name: &str, text: &str) {
            std::fs::write(self.world.root.join("bin").join(name), text).unwrap();
        }

        /// The calls of the fake `gh` that contain `call`.
        fn calls(&self, call: &str) -> Vec<String> {
            std::fs::read_to_string(self.world.root.join("bin/gh.log"))
                .unwrap_or_default()
                .lines()
                .filter(|line| line.contains(call))
                .map(str::to_owned)
                .collect()
        }

        /// The number of looks at the Gate of `main` so far.
        fn looks(&self) -> usize {
            self.calls("/commits/main/check-runs").len()
        }

        /// Waits for `n` more looks.
        async fn looks_more(&self, n: usize) {
            let end = self.looks() + n;
            self.until(0, |h| h.looks() >= end).await;
        }

        /// Each post of the look.
        fn posts(&self) -> Vec<(Option<String>, Kind, String)> {
            self.forge().posts.clone()
        }
    }

    /// #698, 01M5QYDE41FVFX26EC8K5QGYH4, 01M5QYDHN27RC5SRHAB56PFKK0: a
    /// repository with no check named `Gate`. Its ruleset requires the
    /// checks `checks / lint` and `checks / docs`. The wave starts no
    /// verify while one of them runs, and starts it when both passed. It
    /// never asks for the check `Gate`.
    #[tokio::test]
    async fn the_wave_starts_the_verify_when_the_required_checks_passed_and_the_repository_has_no_gate()
     {
        let here = Here::looking().await;
        here.put(
            "rules",
            r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[
                {"context":"checks / lint"},{"context":"checks / docs"},{"context":"riff/verify"}]}}]"#,
        );
        here.put(
            "docs",
            r#"{"check_runs":[{"status":"in_progress","conclusion":null}]}"#,
        );
        {
            let mut forge = here.forge();
            forge.checked = true;
            forge.pulls.insert(1, Pr::Asked);
        }
        here.run();

        // The wave looks at the checks of the head again and again, and
        // starts no verify.
        here.until(0, |h| !h.calls("check_name=checks%20%2F%20docs").is_empty())
            .await;
        here.idle(0).await;
        assert!(here.forge().starts.is_empty(), "{:?}", here.forge());

        here.put(
            "docs",
            r#"{"check_runs":[{"status":"completed","conclusion":"success"}]}"#,
        );
        here.until(1, |h| h.running("verify-issue-1").is_some())
            .await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1".into(),
        };
        assert_eq!(here.forge().works, [verify]);
        // Neither the pull request nor the watch of `main` asks for the
        // check `Gate`: both read the required checks.
        assert!(here.calls("check_name=Gate").is_empty());
        assert!(!here.calls("/rules/branches/main").is_empty());
    }

    /// The look of the daemon at `main` (#1351, 01M5QVH3MAEPPJN9PPVDX6N2EY,
    /// 01M51QEYKTD7ZEB0YA0C3MQTPN, 01M51QEYPGD6W345MZ4F85C38F,
    /// 01M51QEYVQN74Q1VDTVHB0445Q): the Gate of the head of `main` fails
    /// at a new commit, and the lead gets one message and riff files the
    /// fix item. It passes again, and riff closes the item.
    #[tokio::test]
    async fn the_daemon_tells_the_lead_when_main_breaks_and_closes_the_fix_item_when_it_passes() {
        let here = Here::looking().await;
        here.put("gate", &gate("c0ffee1234", "success"));
        here.run();
        here.looks_more(3).await;
        assert!(here.posts().is_empty());

        here.put("gate", &gate("5103311abcdef", "failure"));
        here.until(0, |h| h.posts().len() == 1).await;
        let (to, kind, body) = here.posts().remove(0);
        assert_eq!((to, kind), (None, Kind::Message));
        assert_eq!(
            body,
            format!(
                "main is broken: the full check failed at commit 5103311 ({RUN}). Failed jobs: Clippy. \
                 The fix item is #901: the rollout gives it to a worker before each other item. \
                 When a pull request already has the fix, hold #901."
            )
        );
        let created = here.calls("issue create");
        assert_eq!(created.len(), 1, "{created:?}");
        assert!(
            created[0].contains("--label broken-main") && created[0].contains("--milestone Wave 1"),
            "{created:?}"
        );

        // The same Gate at the next looks: no message, no second fix item.
        here.looks_more(3).await;
        assert_eq!(here.posts().len(), 1);
        assert_eq!(here.calls("issue create").len(), 1);
        assert!(here.calls("issue comment").is_empty());

        // The fix merges, and the Gate passes: riff closes the fix item.
        here.put("gate", &gate("d00d1234", "success"));
        here.until(0, |h| h.posts().len() == 2).await;
        let (to, kind, body) = here.posts().remove(1);
        assert_eq!((to, kind), (None, Kind::Note));
        assert_eq!(
            body,
            format!(
                "main passes again: the full check passed at commit d00d123 ({RUN}). riff closed the \
                 fix item #901."
            )
        );
        assert_eq!(here.calls("issue close 901").len(), 1);
        here.looks_more(3).await;
        assert_eq!(here.posts().len(), 2);
        // Each message about main has the key of its commit, its run and its
        // state (01M6FYH7BFFSZQ2TAJSX8A5QQV, 01M6GX8CN4ZTV0X40GD5AWPXH9).
        assert_eq!(
            here.forge().post_keys,
            [
                Some("main-5103311abcdef-38861280871-1-broken".to_owned()),
                Some("main-d00d1234-38861280871-1-passes".to_owned())
            ]
        );
        assert_eq!(here.calls("issue close").len(), 1);
    }

    /// #1393, 01M5R13DGMTNWPY88A3EG49WWS: the repository has no check
    /// named `Gate`. Its ruleset requires `checks / test`. A failure of
    /// `checks / test` on the head of `main` tells the lead one time and
    /// files the fix item, and the daemon never asks for the check `Gate`.
    #[tokio::test]
    async fn the_daemon_watches_main_with_the_required_checks_and_not_the_gate() {
        let here = Here::looking().await;
        here.put(
            "rules",
            r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[
                {"context":"checks / test"},{"context":"riff/verify"}]}}]"#,
        );
        here.put(
            "jobs",
            r#"[{"name":"checks / test","conclusion":"failure"}]"#,
        );
        here.put("gate", &gate("5103311abcdef", "failure"));
        here.run();
        here.until(0, |h| h.posts().len() == 1).await;
        let (to, kind, body) = here.posts().remove(0);
        assert_eq!((to, kind), (None, Kind::Message));
        assert_eq!(
            body,
            format!(
                "main is broken: the full check failed at commit 5103311 ({RUN}). Failed jobs: \
                 checks / test. The fix item is #901: the rollout gives it to a worker before \
                 each other item. When a pull request already has the fix, hold #901."
            )
        );
        let created = here.calls("issue create");
        assert_eq!(created.len(), 1, "{created:?}");
        assert!(
            created[0].contains("--title main is broken at 5103311: the full check failed"),
            "{created:?}"
        );

        // The same failure at the next looks: no message, no second fix item.
        here.looks_more(3).await;
        assert_eq!(here.posts().len(), 1);
        assert_eq!(here.calls("issue create").len(), 1);
        assert!(
            !here
                .calls("/commits/main/check-runs?check_name=checks%20%2F%20test")
                .is_empty()
        );
        assert!(here.calls("check_name=Gate").is_empty());
    }

    /// A post that does not go out comes again at the next look, and
    /// riff files no second fix item for it (01M51QEYPGD6W345MZ4F85C38F).
    #[tokio::test]
    async fn a_message_that_did_not_go_out_comes_at_the_next_look_with_one_fix_item() {
        let here = Here::looking().await;
        here.forge().post_fails = 1;
        here.put("gate", &gate("5103311abcdef", "failure"));
        here.run();
        here.until(0, |h| h.posts().len() == 1).await;
        assert!(here.posts()[0].2.contains("The fix item is #901"));
        assert_eq!(here.calls("issue create").len(), 1);
        assert_eq!(here.calls("issue comment 901").len(), 1);
        here.looks_more(3).await;
        assert_eq!(here.posts().len(), 1);
    }

    /// A pull request with a conflict gives one message for each head:
    /// to the session that holds the item, else to the lead
    /// (01M49Q30XMVRFX42YTM1PHX0RZ, 01M49Q31FASDM7CG3JEGPYCZB9).
    #[tokio::test]
    async fn the_daemon_tells_the_holder_or_the_lead_of_a_conflict_once_for_each_head() {
        let here = Here::looking().await;
        here.forge()
            .claims
            .insert("issue-12".into(), "1a2b3c4d".into());
        here.put("pulls", &conflicting("1a2b3c4d5e"));
        here.run();
        here.until(0, |h| h.posts().len() == 1).await;
        let (to, kind, body) = here.posts().remove(0);
        assert_eq!((to.as_deref(), kind), (Some("issue-12"), Kind::Message));
        assert!(
            body.contains("Pull request #52 of issue-12 has a conflict")
                && body.contains("at commit 1a2b3c4,"),
            "{body}"
        );
        here.looks_more(3).await;
        assert_eq!(here.posts().len(), 1);

        // A new head with a conflict, and no holder: the lead.
        here.forge().claims.clear();
        here.put("pulls", &conflicting("5e6f7a8b9c"));
        here.until(0, |h| h.posts().len() == 2).await;
        let (to, _, body) = here.posts().remove(1);
        assert_eq!(to, None);
        assert!(body.contains("No session holds issue-12"), "{body}");
        // Each post has the key of its news: the look of another host makes
        // the same keys, and riff-server drops the second post
        // (01M5TKG2K1AYK1KE7C2AA2W9DS).
        assert_eq!(
            here.forge().post_keys,
            [
                Some("pull-52-1a2b3c4d5e-conflict".to_owned()),
                Some("pull-52-5e6f7a8b9c-conflict".to_owned())
            ]
        );
    }

    /// `riff wave stop` ends the look with the loop of the wave.
    #[tokio::test]
    async fn a_stop_of_the_wave_ends_its_look() {
        let here = Here::looking().await;
        here.run();
        here.looks_more(2).await;
        assert!(here.daemon.stop_wave("o/r").unwrap());
        // A call in flight can still end.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let looks = here.looks();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(here.looks(), looks);
    }

    #[tokio::test]
    async fn a_new_ask_with_another_main_clone_runs_the_wave_with_it() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 3, free).await;
        here.run();
        let other = Wave {
            repo: "o/r".into(),
            main: "/src/other".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        here.daemon.run_wave(other.clone()).unwrap();
        let kept = read_waves(here.daemon.dir()).unwrap();
        assert_eq!(kept.get("o/r"), Some(&other));
        assert_eq!(here.daemon.waves_shown().len(), 1);
        here.until(0, |h| h.daemon.waves_shown()[0].word == text::WAVE_NO_WORK)
            .await;
    }

    #[test]
    fn a_wave_goes_to_its_file_and_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_waves(dir.path()).unwrap().is_empty());
        let wave = Wave {
            repo: "o/r".into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        let waves: BTreeMap<String, Wave> = [("o/r".to_owned(), wave)].into();
        write_waves(dir.path(), &waves).unwrap();
        assert_eq!(read_waves(dir.path()).unwrap(), waves);
    }

    #[test]
    fn the_pick_skips_a_verify_until_its_gate_is_done_and_counts_the_starts() {
        let waiting = |gate_done| Waiting {
            pull: 40,
            commit: "c".into(),
            gate_done,
        };
        let look = Look {
            free: vec!["verify-issue-9".into(), "issue-12".into()],
            pulls: [(9, waiting(false))].into(),
            ..Look::default()
        };
        let none = BTreeSet::new();
        assert_eq!(
            pick(&look, &none, &BTreeMap::new()),
            Some(Work::Build { issue: 12 })
        );
        let spent: BTreeMap<String, u32> = [("issue-12".to_owned(), STARTS)].into();
        assert_eq!(pick(&look, &none, &spent), None);
        let running: BTreeSet<String> = ["issue-12".to_owned()].into();
        assert_eq!(pick(&look, &running, &BTreeMap::new()), None);
        let look = Look {
            pulls: [(9, waiting(true))].into(),
            ..look
        };
        assert_eq!(
            pick(&look, &none, &BTreeMap::new()),
            Some(Work::Verify {
                issue: 9,
                pull: 40,
                commit: "c".into()
            })
        );
        // The author of 9 runs in this daemon.
        let running: BTreeSet<String> = ["issue-9".to_owned()].into();
        assert_eq!(
            pick(&look, &running, &BTreeMap::new()),
            Some(Work::Build { issue: 12 })
        );
    }

    #[test]
    fn a_job_goes_into_the_record_and_comes_back() {
        let job = Job {
            wave: Wave {
                repo: "o/r".into(),
                main: "/src/r".into(),
                server: "http://127.0.0.1:1".into(),
                host: "thelio".into(),
            },
            work: Work::Verify {
                issue: 12,
                pull: 40,
                commit: "1a2b".into(),
            },
        };
        let text = serde_json::to_string(&job).unwrap();
        assert!(text.contains(r#""kind":"verify""#), "{text}");
        assert_eq!(serde_json::from_str::<Job>(&text).unwrap(), job);
    }

    /// Runs `git ARGS` in `dir` (with an identity and no signing), and
    /// gives its output.
    fn git_in(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// Commits a change of the file `a` in `dir`, and gives the commit.
    fn commit_a(dir: &Path, words: &str) -> String {
        std::fs::write(dir.join("a"), words).unwrap();
        git_in(dir, &["add", "a"]);
        git_in(dir, &["commit", "-q", "-m", words]);
        git_in(dir, &["rev-parse", "HEAD"])
    }

    /// The git of a test of [`clone_at`]: a bare `origin` that a `file://`
    /// URL names, as a URL of GitHub does (a path would copy each
    /// object), a main clone of it, and the head commit of the pull
    /// request 7 of a fork. `origin` has that commit only on
    /// `refs/pull/7/head`, as GitHub has it.
    struct Fork {
        dir: tempfile::TempDir,
        main: PathBuf,
        /// The work tree of the fork.
        fork: PathBuf,
        head: String,
    }

    impl Fork {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let (origin, fork, main) = (
                dir.path().join("origin.git"),
                dir.path().join("fork"),
                dir.path().join("main"),
            );
            let url = format!("file://{}", origin.display());
            std::fs::create_dir(&fork).unwrap();
            git_in(
                dir.path(),
                &["init", "-q", "--bare", "-b", "main", "origin.git"],
            );
            git_in(&fork, &["init", "-q", "-b", "main"]);
            commit_a(&fork, "one");
            git_in(&fork, &["remote", "add", "origin", &url]);
            git_in(&fork, &["push", "-q", "origin", "main"]);
            git_in(dir.path(), &["clone", "-q", "--", &url, "main"]);
            git_in(&fork, &["switch", "-q", "-c", "patch"]);
            let head = commit_a(&fork, "two");
            git_in(&fork, &["push", "-q", "origin", "HEAD:refs/pull/7/head"]);
            Fork {
                dir,
                main,
                fork,
                head,
            }
        }

        /// Whether the repository `repo` has the commit `commit`.
        fn has(repo: &Path, commit: &str) -> bool {
            std::process::Command::new("git")
                .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
                .current_dir(repo)
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        }
    }

    /// 01M5QVARWYFHTQV5THNRB09SZA: the head commit of a pull request
    /// from a fork is on no branch of `origin`. The clone of the verify
    /// fetches `refs/pull/N/head` and checks out the commit, detached.
    #[test]
    fn the_clone_of_a_verify_has_the_head_commit_of_a_pull_request_from_a_fork() {
        let fork = Fork::new();
        assert!(!Fork::has(&fork.main, &fork.head), "origin has no branch");
        let clone = fork.dir.path().join("verify/clone");
        clone_at(&fork.main, &clone, 7, &fork.head).unwrap();
        assert_eq!(git_in(&clone, &["rev-parse", "HEAD"]), fork.head);
        assert_eq!(git_in(&clone, &["branch", "--show-current"]), "");
        assert_eq!(std::fs::read_to_string(clone.join("a")).unwrap(), "two");
    }

    /// 01M5QVARWYFHTQV5THNRB09SZA: a push to the pull request after the
    /// look moves `refs/pull/N/head`. The clone still has the commit of
    /// the look.
    #[test]
    fn a_push_to_the_pull_request_after_the_look_does_not_change_the_commit_of_the_clone() {
        let fork = Fork::new();
        commit_a(&fork.fork, "three");
        git_in(
            &fork.fork,
            &["push", "-q", "origin", "HEAD:refs/pull/7/head"],
        );
        let clone = fork.dir.path().join("verify/clone");
        clone_at(&fork.main, &clone, 7, &fork.head).unwrap();
        assert_eq!(git_in(&clone, &["rev-parse", "HEAD"]), fork.head);
        assert_eq!(std::fs::read_to_string(clone.join("a")).unwrap(), "two");
    }

    /// 01M5QVARWYFHTQV5THNRB09SZA: a pull request that `origin` does not
    /// have fails, and the error names the ref.
    #[test]
    fn the_clone_of_a_verify_fails_with_the_ref_when_origin_does_not_have_it() {
        let fork = Fork::new();
        let clone = fork.dir.path().join("verify/clone");
        let error = clone_at(&fork.main, &clone, 8, &fork.head).unwrap_err();
        assert!(error.to_string().contains("refs/pull/8/head"), "{error}");
    }

    /// The riff of a test of [`after_build`]: a riff-server in this
    /// process, the lead `mike/lead1` of `o/r`, and the session of a
    /// build of `issue-12` that ended.
    struct Built {
        api: Api,
        wave: Wave,
        lead: SessionUri,
        record: Record,
        dir: tempfile::TempDir,
    }

    impl Built {
        async fn new() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let server = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(async move {
                axum::serve(listener, riff_server::router()).await.unwrap();
            });
            let api = Api::new(&server);
            let wave = Wave {
                repo: "o/r".into(),
                main: "/src/r".into(),
                server: server.clone(),
                host: "thelio".into(),
            };
            let who = |session| riff_core::name::Who::new("mike", Some(session)).unwrap();
            let lead = SessionUri::new(who("lead1"), wave.place(None).unwrap());
            api.start(&lead, StartReason::Process, false).await.unwrap();
            let session = "9f8e7d6c-0000-4000-8000-000000000000";
            let me = SessionUri::new(who(session), wave.place(Some("issue-12")).unwrap());
            api.start(&me, StartReason::Process, true).await.unwrap();
            let dir = tempfile::tempdir().unwrap();
            let record = Record {
                live: riff_client::sessions::Live {
                    digest: String::new(),
                    name: "riff-issue-12".into(),
                    session: session.into(),
                    item: "issue-12".into(),
                    image: IMAGE.into(),
                    since_ms: 0,
                    phase: Default::default(),
                    last: String::new(),
                    lead: false,
                },
                folder: dir.path().to_owned(),
                server,
                me,
                inputs: Vec::new(),
                closed: true,
                failed: false,
                job: Some(Job {
                    wave: wave.clone(),
                    work: Work::Build { issue: 12 },
                }),
                terminal: false,
                takeover: false,
                interrupt: false,
            };
            Self {
                api,
                wave,
                lead,
                record,
                dir,
            }
        }

        /// A fake `gh`: the branch of issue 12 has the open pull request
        /// #40, and the issue has the label `no-verify` when `labelled`.
        fn gh(&self, labelled: bool) -> pr::Gh {
            let labels = if labelled {
                r#"[{"name":"no-verify"}]"#
            } else {
                "[]"
            };
            let script = format!(
                r#"#!/bin/sh
case "$*" in
"api repos/o/r --jq .default_branch") echo main ;;
"pr view worktree-issue-12 --json number,url,headRefOid,state")
    echo '{{"number":40,"url":"https://github.com/o/r/pull/40","headRefOid":"1a2b3c4d5e","state":"OPEN"}}' ;;
"issue view 12 --json number,labels")
    echo '{{"number":12,"labels":{labels}}}' ;;
*)
    echo "the fake gh has no reply for: $*" >&2
    exit 1 ;;
esac
"#
            );
            pr::Gh::at(isolated::write_program(self.dir.path().join("gh"), script))
        }

        /// Runs [`after_build`] with `gh`, and gives each message that the
        /// lead can read, with its kind.
        async fn after_build(&self, gh: pr::Gh) -> Vec<(Kind, String)> {
            self.after(gh, None).await.1
        }

        /// [`Built::after_build`] after the fix `fix`, with the line of
        /// each step.
        async fn after(
            &self,
            gh: pr::Gh,
            fix: Option<Fix<'_>>,
        ) -> (Vec<String>, Vec<(Kind, String)>) {
            let lines = after_build(
                gh,
                &self.api,
                &self.record,
                &self.wave,
                12,
                (fix, None),
                Ended {
                    code: Some(1),
                    failed: true,
                },
            )
            .await
            .unwrap();
            let thread = self.wave.thread().unwrap();
            let got = self
                .api
                .read(&self.lead, &thread, true)
                .await
                .unwrap()
                .into_iter()
                .map(|c| (c.message.kind, c.message.body))
                .collect();
            (lines, got)
        }
    }

    /// 01M5A5HXKZTDP5M5HY2FHCEBZA: a build that ends with no open pull
    /// request leaves a record in the thread `ends`: its reason and the
    /// last lines of its output. `riff session why` reads it.
    #[tokio::test]
    async fn a_build_with_no_pull_request_leaves_the_reason_of_its_end() {
        let built = Built::new().await;
        std::fs::write(
            built.dir.path().join(crate::daemon::EVENTS),
            "{\"type\":\"assistant\"}\nlast line of the output\n",
        )
        .unwrap();
        let gh = pr::Gh::at(isolated::write_program(
            built.dir.path().join("gh"),
            "#!/bin/sh\nexit 1\n",
        ));
        let (lines, got) = built.after(gh, None).await;
        assert_eq!(lines, [text::run_no_pull("worktree-issue-12")]);
        assert_eq!(got.len(), 1, "only the note to the lead: {got:?}");
        let records = crate::ends::read(&built.api, &built.lead).await.unwrap();
        let [record] = &records[..] else {
            panic!("one record: {records:?}");
        };
        assert_eq!(record.tag("item"), "issue-12");
        assert_eq!(record.tag("session"), "9f8e7d6c");
        assert_eq!(record.reason, "exit code 1");
        assert_eq!(
            record.tail,
            ["{\"type\":\"assistant\"}", "last line of the output"]
        );
        let shown = crate::ends::why("issue-12", &records);
        assert!(shown.contains("ended: exit code 1."), "{shown}");
    }

    /// 01M5P26VSA7AA99K1RYG83R8J9: a build that pushed nothing, and that
    /// riff saved, gives the lead one note with the branch and the commit.
    #[tokio::test]
    async fn a_saved_build_tells_the_lead_the_branch_and_the_commit() {
        let built = Built::new().await;
        let gh = pr::Gh::at(isolated::write_program(
            built.dir.path().join("gh"),
            "#!/bin/sh\nexit 1\n",
        ));
        let ended = Ended {
            code: Some(0),
            failed: false,
        };
        after_build(
            gh,
            &built.api,
            &built.record,
            &built.wave,
            12,
            (None, Some("1a2b3c4")),
            ended,
        )
        .await
        .unwrap();
        let thread = built.wave.thread().unwrap();
        let got = built.api.read(&built.lead, &thread, true).await.unwrap();
        let [one] = &got[..] else {
            panic!("one message: {got:?}");
        };
        assert!(one.message.body.contains("worktree-issue-12"), "{one:?}");
        assert!(one.message.body.contains("1a2b3c4"), "{one:?}");
    }

    /// 01M5SMP2D4MGME90EHVKAQHQ26 (#1441): a verify that ends with no PASS
    /// or FAIL leaves a record in the thread `ends`, and `riff session why`
    /// shows it. The lead gets the message too, and the folder goes.
    #[tokio::test]
    async fn a_verify_with_no_result_leaves_the_reason_of_its_end() {
        let built = Built::new().await;
        let mut record = built.record.clone();
        record.live.name = "riff-verify-issue-12".into();
        record.live.item = "verify-issue-12".into();
        std::fs::write(
            record.folder.join(crate::daemon::EVENTS),
            "{\"type\":\"assistant\"}\n{\"type\":\"result\",\"result\":\"I looked, and I stopped.\"}\n",
        )
        .unwrap();
        let ended = Ended {
            code: Some(0),
            failed: false,
        };
        let gh = pr::Gh::at(isolated::write_program(
            built.dir.path().join("gh"),
            "#!/bin/sh\nexit 1\n",
        ));
        let lines = after_verify(gh, &built.api, &record, &built.wave, (40, "1a2b3c4"), ended)
            .await
            .unwrap();
        assert_eq!(lines, [text::wave_no_result("verify-issue-12", 40)]);
        assert!(!record.folder.exists(), "the folder of the verify goes");
        let thread = built.wave.thread().unwrap();
        let got = built.api.read(&built.lead, &thread, true).await.unwrap();
        let [message] = &got[..] else {
            panic!("one message: {got:?}");
        };
        assert_eq!(message.message.body, lines[0]);
        let records = crate::ends::read(&built.api, &built.lead).await.unwrap();
        let [one] = &records[..] else {
            panic!("one record: {records:?}");
        };
        assert_eq!(one.tag("item"), "verify-issue-12");
        assert_eq!(one.tag("session"), "9f8e7d6c");
        assert_eq!(one.reason, "exit code 0, and no PASS or FAIL");
        assert_eq!(
            one.tail,
            [
                "{\"type\":\"assistant\"}",
                "{\"type\":\"result\",\"result\":\"I looked, and I stopped.\"}"
            ]
        );
        let shown = crate::ends::why("verify-issue-12", &records);
        assert!(
            shown.starts_with("verify-issue-12: 1 session ended with no PASS or FAIL."),
            "{shown}"
        );
        assert!(
            shown.contains("ended: exit code 0, and no PASS or FAIL."),
            "{shown}"
        );
        assert_eq!(
            crate::ends::why("issue-12", &records),
            text::session_why_none("issue-12")
        );
    }

    /// 01M6H25QH6FSNBF5F6TE6B1JXM (#1557): a build that ends while a limit of
    /// GitHub is out cannot read its pull request. The lead gets a note, not
    /// the message that the branch has no pull request.
    #[tokio::test]
    async fn a_build_that_ends_while_the_github_limit_is_out_gets_a_note_and_no_false_report() {
        let built = Built::new().await;
        let gh = pr::Gh::at(isolated::write_program(
            built.dir.path().join("gh"),
            "#!/bin/sh\necho 'GraphQL: API rate limit already exceeded for user ID 1.' >&2\nexit 1\n",
        ));
        let (lines, got) = built.after(gh, None).await;
        let note = text::wave_build_limit("issue-12", "worktree-issue-12");
        assert_eq!(got, [(Kind::Note, note.clone())]);
        assert_eq!(lines, [note]);
    }

    /// 01M5A5HXKZTDP5M5HY2FHCEBZA: a build with an open pull request
    /// leaves no record.
    #[tokio::test]
    async fn a_build_with_a_pull_request_leaves_no_record_of_its_end() {
        let built = Built::new().await;
        let gh = built.gh(false);
        built.after_build(gh).await;
        let records = crate::ends::read(&built.api, &built.lead).await.unwrap();
        assert!(records.is_empty(), "{records:?}");
    }

    /// With no hold and no label, the lead gets the verify request
    /// (01M4ZV3B6Q0Y6AX0KSKC55S31R).
    #[tokio::test]
    async fn a_build_sends_the_verify_request_to_the_lead() {
        let riff = Built::new().await;
        let got = riff.after_build(riff.gh(false)).await;
        assert_eq!(
            got,
            [(
                Kind::Message,
                text::wave_verify_request(
                    "issue-12",
                    40,
                    "worktree-issue-12",
                    "1a2b3c4",
                    "9f8e7d6c"
                )
            )]
        );
    }

    /// The head commit of the pull request is a WIP commit of a pause: the
    /// lead gets a note, and no verify request. "Pause the riff" in the book
    /// shows the same note.
    #[tokio::test]
    async fn a_build_that_ends_with_a_wip_head_sends_no_verify_request() {
        let riff = Built::new().await;
        let script = r#"#!/bin/sh
case "$*" in
"pr view worktree-issue-12 --json number,url,headRefOid,state")
    echo '{"number":40,"url":"https://github.com/o/r/pull/40","headRefOid":"1a2b3c4d5e","state":"OPEN"}' ;;
"api repos/o/r/commits/1a2b3c4d5e --jq .commit.message")
    printf 'WIP: the riff is paused\n\nThe session left the riff.\n' ;;
*) exit 1 ;;
esac
"#;
        let gh = pr::Gh::at(isolated::write_program(riff.dir.path().join("gh"), script));
        let got = riff.after_build(gh).await;
        let note = text::wave_wip_head("issue-12", 40, "worktree-issue-12", "1a2b3c4", "9f8e7d6c");
        assert_eq!(got, [(Kind::Note, note.clone())]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/how-it-works.md"
        ))
        .unwrap();
        assert!(book.contains(&note), "{note}");
    }

    /// A lead holds the verify of the item: the lead gets a note with the
    /// hold, and no verify request (01M51KH0H7J35PEKMHP5KQB84T).
    #[tokio::test]
    async fn a_build_whose_verify_is_held_posts_a_note_with_the_hold() {
        let riff = Built::new().await;
        let thread = riff.wave.thread().unwrap();
        riff.api
            .hold(&riff.lead, &thread, "verify-issue-12", "a scratch item")
            .await
            .unwrap();
        let got = riff.after_build(riff.gh(false)).await;
        assert_eq!(got.len(), 1, "{got:?}");
        let (kind, body) = &got[0];
        assert_eq!(*kind, Kind::Note);
        assert!(
            body.starts_with(
                "issue-12: the session of the wave ended, and the item is free. The riff daemon \
                 sends no verify request for PR #40, branch worktree-issue-12, commit 1a2b3c4 \
                 (session 9f8e7d6c): verify-issue-12 is held by the lead (the session \
                 mike/lead1) since "
            ),
            "{body}"
        );
        assert!(body.ends_with(": a scratch item"), "{body}");
        assert!(!body.contains("verify request:"), "{body}");
    }

    /// The issue has the label `no-verify`: the lead gets a note with the
    /// label, and no verify request (01M51KH0H7J35PEKMHP5KQB84T). The book
    /// shows the same note.
    #[tokio::test]
    async fn a_build_of_an_issue_with_the_label_no_verify_posts_a_note() {
        let riff = Built::new().await;
        let got = riff.after_build(riff.gh(true)).await;
        let note = text::wave_no_verify(
            "issue-12",
            40,
            "worktree-issue-12",
            "1a2b3c4",
            "9f8e7d6c",
            &text::no_verify_label(12),
        );
        assert_eq!(got, [(Kind::Note, note.clone())]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/how-it-works.md"
        ))
        .unwrap();
        assert!(book.contains(&format!("\n{note}\n")), "{note}");
    }

    /// 01M57FJ3Z8AFDY9G0KTAYZJESY: the rebase session ended, and the pull
    /// request still has the head with the conflict: the lead gets a
    /// message, and no verify request. "Run the wave" in the book shows
    /// the same message.
    #[tokio::test]
    async fn a_rebase_with_no_push_tells_the_lead_and_sends_no_verify_request() {
        let riff = Built::new().await;
        let (lines, got) = riff
            .after(riff.gh(false), Some(Fix::Rebase("1a2b3c4d5e")))
            .await;
        assert_eq!(lines[1], text::run_lead_told("issue-12"));
        let message = text::wave_not_rebased(
            "issue-12",
            40,
            "worktree-issue-12",
            "main",
            "1a2b3c4d5e",
            "9f8e7d6c",
        );
        assert_eq!(got, [(Kind::Message, message.clone())]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/development.md"
        ))
        .unwrap();
        let run_the_wave = book
            .split("\n### Run the wave\n")
            .nth(1)
            .and_then(|rest| rest.split("\n### ").next())
            .expect("the how-to Run the wave");
        assert!(
            run_the_wave.contains(&format!("\n{message}\n")),
            "{message}"
        );
    }

    /// 01M5R56GN3DC09GAPAF69GENEZ (#1404): the default branch is not in a
    /// file. When it does not read after a rebase with no push, the lead
    /// still gets the message, with "the default branch" in place of its
    /// name.
    #[tokio::test]
    async fn a_rebase_with_no_push_tells_the_lead_when_the_default_branch_does_not_read() {
        let riff = Built::new().await;
        let gh = pr::Gh::at(isolated::write_program(
            riff.dir.path().join("gh"),
            r#"#!/bin/sh
case "$*" in
"pr view worktree-issue-12 --json number,url,headRefOid,state")
    echo '{"number":40,"url":"https://github.com/o/r/pull/40","headRefOid":"1a2b3c4d5e","state":"OPEN"}' ;;
*) echo "gh: HTTP 502" >&2; exit 1 ;;
esac
"#,
        ));
        let (_, got) = riff.after(gh, Some(Fix::Rebase("1a2b3c4d5e"))).await;
        let message = text::wave_not_rebased(
            "issue-12",
            40,
            "worktree-issue-12",
            text::WAVE_BASE_UNREAD,
            "1a2b3c4d5e",
            "9f8e7d6c",
        );
        assert_eq!(got, [(Kind::Message, message)]);
    }

    /// A fake `gh` for the fix of checks: the pull request #40 of the
    /// branch of issue 12 has the head `1a2b3c4d5e`, and the rollup of its
    /// checks is `rollup`.
    fn gh_with_rollup(riff: &Built, rollup: &str) -> pr::Gh {
        let script = format!(
            r#"#!/bin/sh
case "$*" in
"pr view worktree-issue-12 --json number,url,headRefOid,state")
    echo '{{"number":40,"url":"https://github.com/o/r/pull/40","headRefOid":"1a2b3c4d5e","state":"OPEN"}}' ;;
"pr view 40 --json {fields}")
    echo '{{"number":40,"headRefName":"worktree-issue-12","headRefOid":"1a2b3c4d5e","isDraft":false,"statusCheckRollup":{rollup}}}' ;;
"issue view 12 --json number,labels")
    echo '{{"number":12,"labels":[]}}' ;;
*) echo "the fake gh has no reply for: $*" >&2; exit 1 ;;
esac
"#,
            fields = rollout::PULL_FIELDS
        );
        pr::Gh::at(isolated::write_program(riff.dir.path().join("gh"), script))
    }

    /// #279: the session that fixed a failed check pushed nothing, and the
    /// check still fails at the head: the lead gets a message, and no
    /// verify request. "Run the wave" in the book shows the same message.
    #[tokio::test]
    async fn a_fix_of_checks_with_no_push_and_a_failed_check_tells_the_lead() {
        let riff = Built::new().await;
        let gh = gh_with_rollup(
            &riff,
            r#"[{"name":"Gate","conclusion":"FAILURE","workflowName":"CI"},{"context":"riff/verify","state":"SUCCESS"}]"#,
        );
        let checks = ["Gate".to_owned()];
        let (lines, got) = riff
            .after(gh, Some(Fix::Checks("1a2b3c4d5e", &checks)))
            .await;
        assert_eq!(lines[1], text::run_lead_told("issue-12"));
        let message = text::wave_checks_not_fixed(
            "issue-12",
            40,
            "worktree-issue-12",
            "1a2b3c4d5e",
            "9f8e7d6c",
            &checks,
        );
        assert_eq!(got, [(Kind::Message, message.clone())]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/development.md"
        ))
        .unwrap();
        assert!(book.contains(&format!("\n{message}\n")), "{message}");
    }

    /// #279: the session ran the job again and pushed nothing: no check
    /// fails now (the new run goes). The lead gets a note, not a message,
    /// and no verify request: the verify of the head passed.
    #[tokio::test]
    async fn a_fix_of_checks_that_ran_the_job_again_gives_the_lead_a_note() {
        let riff = Built::new().await;
        let gh = gh_with_rollup(
            &riff,
            r#"[{"name":"Gate","conclusion":""},{"context":"riff/verify","state":"SUCCESS"}]"#,
        );
        let checks = ["Gate".to_owned()];
        let (lines, got) = riff
            .after(gh, Some(Fix::Checks("1a2b3c4d5e", &checks)))
            .await;
        assert_eq!(lines[1], text::run_lead_noted("issue-12"));
        let note = text::wave_checks_rerun(
            "issue-12",
            40,
            "worktree-issue-12",
            "1a2b3c4d5e",
            "9f8e7d6c",
        );
        assert_eq!(got, [(Kind::Note, note.clone())]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/development.md"
        ))
        .unwrap();
        assert!(book.contains(&format!("\n{note}\n")), "{note}");
    }

    /// #1540, 01M5RGM8HBVQMY25H5PYESK17J: the rollup of the pull request
    /// has an old failed run of `Gate` and a newer run of it that passed.
    /// The session pushed nothing, and no check fails now: the lead gets a
    /// note, not a message that the check still fails.
    #[tokio::test]
    async fn a_fix_of_checks_with_an_old_failed_run_and_a_newer_passed_run_gives_the_lead_a_note() {
        let riff = Built::new().await;
        let gh = gh_with_rollup(
            &riff,
            r#"[{"name":"Gate","conclusion":"FAILURE","workflowName":"CI","startedAt":"2026-11-03T05:50:00Z","completedAt":"2026-11-03T05:50:30Z"},
                {"name":"Gate","conclusion":"SUCCESS","workflowName":"CI","startedAt":"2026-11-03T05:55:00Z","completedAt":"2026-11-03T05:55:30Z"},
                {"context":"riff/verify","state":"SUCCESS"}]"#,
        );
        let checks = ["Gate".to_owned()];
        let (lines, got) = riff
            .after(gh, Some(Fix::Checks("1a2b3c4d5e", &checks)))
            .await;
        assert_eq!(lines[1], text::run_lead_noted("issue-12"));
        let [(kind, body)] = &got[..] else {
            panic!("{got:?}");
        };
        assert_eq!(*kind, Kind::Note);
        assert!(!body.contains("still fails"), "{body}");
    }

    /// #1540: the pull request merged before the session ended. The wave
    /// reads no open pull request, so the lead gets no message that a
    /// check still fails, also when the rollup of the merged head has a
    /// failed run.
    #[tokio::test]
    async fn a_fix_of_checks_for_a_merged_pull_request_sends_no_message_that_a_check_still_fails() {
        let riff = Built::new().await;
        let script = format!(
            r#"#!/bin/sh
case "$*" in
"pr view worktree-issue-12 --json number,url,headRefOid,state")
    echo '{{"number":40,"url":"https://github.com/o/r/pull/40","headRefOid":"1a2b3c4d5e","state":"MERGED"}}' ;;
"pr view 40 --json {fields}")
    echo '{{"number":40,"headRefName":"worktree-issue-12","headRefOid":"1a2b3c4d5e","isDraft":false,"statusCheckRollup":[{{"name":"Gate","conclusion":"FAILURE","workflowName":"CI"}}]}}' ;;
*) echo "the fake gh has no reply for: $*" >&2; exit 1 ;;
esac
"#,
            fields = rollout::PULL_FIELDS
        );
        let gh = pr::Gh::at(isolated::write_program(riff.dir.path().join("gh"), script));
        let checks = ["Gate".to_owned()];
        let (lines, got) = riff
            .after(gh, Some(Fix::Checks("1a2b3c4d5e", &checks)))
            .await;
        assert_eq!(lines, [text::run_no_pull("worktree-issue-12")]);
        assert!(
            got.iter().all(|(kind, _)| *kind == Kind::Note),
            "no message: {got:?}"
        );
        assert!(
            got.iter().all(|(_, body)| !body.contains("still fails")),
            "{got:?}"
        );
    }

    /// #1427: the session fixed the runs of GitHub Actions only. A check
    /// of another app that still fails is not its fix: the lead heard of
    /// it at the look, so the end of the session gives a note.
    #[tokio::test]
    async fn a_fix_of_checks_does_not_count_a_failed_check_of_another_app() {
        let riff = Built::new().await;
        let gh = gh_with_rollup(
            &riff,
            r#"[{"name":"Gate","conclusion":"SUCCESS","workflowName":"CI"},{"name":"ci","conclusion":"FAILURE","workflowName":"","detailsUrl":"https://app.circleci.com/workflow/9"},{"context":"riff/verify","state":"SUCCESS"}]"#,
        );
        let checks = ["Gate".to_owned()];
        let (lines, got) = riff
            .after(gh, Some(Fix::Checks("1a2b3c4d5e", &checks)))
            .await;
        assert_eq!(lines[1], text::run_lead_noted("issue-12"));
        let [(kind, _)] = &got[..] else {
            panic!("{got:?}");
        };
        assert_eq!(*kind, Kind::Note);
    }

    /// #279: when `gh` cannot read the checks, the lead gets the message:
    /// riff does not call a fix good when it cannot see it.
    #[tokio::test]
    async fn a_fix_of_checks_with_no_read_of_the_checks_tells_the_lead() {
        let riff = Built::new().await;
        let checks = ["Gate".to_owned()];
        let (_, got) = riff
            .after(riff.gh(false), Some(Fix::Checks("1a2b3c4d5e", &checks)))
            .await;
        let [(kind, body)] = &got[..] else {
            panic!("{got:?}");
        };
        assert_eq!(*kind, Kind::Message);
        assert!(body.contains("the check Gate still fails"), "{body}");
    }

    /// #279: the fix pushed a new head: the lead gets the verify request
    /// of the new head.
    #[tokio::test]
    async fn a_fix_of_checks_that_pushed_sends_the_verify_request_of_the_new_head() {
        let riff = Built::new().await;
        let checks = ["Gate".to_owned()];
        let (lines, got) = riff
            .after(riff.gh(false), Some(Fix::Checks("0ld0ld0ld0", &checks)))
            .await;
        assert_eq!(lines[1], text::run_verify_sent("issue-12", 40));
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].0, Kind::Message);
        assert!(got[0].1.starts_with("verify request:"), "{got:?}");
    }

    /// 01M57FJ3Z8AFDY9G0KTAYZJESY: the rebase pushed a new head: the lead
    /// gets the verify request of the new head.
    #[tokio::test]
    async fn a_rebase_that_pushed_sends_the_verify_request_of_the_new_head() {
        let riff = Built::new().await;
        let (lines, got) = riff
            .after(riff.gh(false), Some(Fix::Rebase("0ld0ld0ld0")))
            .await;
        assert_eq!(lines[1], text::run_verify_sent("issue-12", 40));
        assert_eq!(
            got,
            [(
                Kind::Message,
                text::wave_verify_request(
                    "issue-12",
                    40,
                    "worktree-issue-12",
                    "1a2b3c4",
                    "9f8e7d6c"
                )
            )]
        );
    }

    /// A fake `gh` for the repository `o/r` that logs nothing and answers
    /// from the files of its folder: the open pull requests, the open
    /// issues, the milestones, the rules and the Gate of `main`. It
    /// answers the query of the merge queue with the cost of the query.
    /// It answers the conditional reads of the look as GitHub does
    /// (#1566): `304` for the `ETag` in the file `etag-issues` or
    /// `etag-pulls`, else `200` with that `ETag`.
    const READS_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
case "$1 $2" in
  "api graphql") cat "$dir/queue" ;;
  "api -i")
    case "$*" in *issues*) tag=$(cat "$dir/etag-issues") ;; *) tag=$(cat "$dir/etag-pulls") ;; esac
    case "$*" in
      *"If-None-Match: $tag"*) echo 'gh: HTTP 304' >&2; exit 1 ;;
      *) printf 'HTTP/2.0 200 OK\r\nEtag: %s\r\n\r\n[]\n' "$tag" ;;
    esac ;;
  api*)
    case "$2" in
      repos/o/r) case "$*" in *--jq*) echo main ;; *) echo '{"default_branch":"main"}' ;; esac ;;
      */rules/branches/main*) cat "$dir/rules" ;;
      */commits/main/check-runs*) cat "$dir/gate" ;;
      */milestones*) cat "$dir/milestones" ;;
      *) exit 1 ;;
    esac ;;
  "pr list") cat "$dir/pulls" ;;
  "issue list") cat "$dir/issues" ;;
  *) exit 1 ;;
esac
"#;

    /// The forge of [`READS_GH`]: `pulls` open pull requests, each with its
    /// verify passed and its Gate running, and 20 open issues, one in the
    /// current wave for each pull request. The files are in the new
    /// folder.
    fn reads_forge(pulls: u64) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        isolated::write_program(dir.path().join("gh"), READS_GH);
        let put = |name: &str, text: &str| std::fs::write(dir.path().join(name), text).unwrap();
        let pull = |n: u64| {
            format!(
                r#"{{"number":{},"headRefName":"worktree-issue-{n}","headRefOid":"1a2b3c4d{n}","isDraft":false,
"statusCheckRollup":[{{"context":"riff/verify","state":"SUCCESS"}},{{"__typename":"CheckRun","name":"Gate","status":"IN_PROGRESS"}}],
"mergeable":"MERGEABLE","autoMergeRequest":{{"enabledAt":"2026-11-03T05:00:00Z"}},"body":"Closes #{n}\n"}}"#,
                40 + n
            )
        };
        let pulls: Vec<String> = (1..=pulls).map(pull).collect();
        put("pulls", &format!("[{}]", pulls.join(",")));
        let issue = |n: u64| {
            let wave = if n <= 8 { "Wave 1" } else { "Backlog" };
            format!(
                r#"{{"number":{n},"title":"item {n}","body":"Done when: it works.","comments":[],"milestone":{{"title":"{wave}"}},"labels":[]}}"#
            )
        };
        let issues: Vec<String> = (1..=20).map(issue).collect();
        put("issues", &format!("[{}]", issues.join(",")));
        put(
            "milestones",
            r#"[{"number":1,"title":"Wave 1","open_issues":8,"closed_issues":0},
{"number":2,"title":"Backlog","open_issues":12,"closed_issues":0}]"#,
        );
        put("rules", GATE_RULES);
        put(
            "gate",
            r#"{"check_runs":[{"status":"in_progress","conclusion":null,"head_sha":"c0ffee1"}]}"#,
        );
        put(
            "queue",
            r#"{"data":{"rateLimit":{"cost":1,"remaining":4300,"resetAt":"2026-11-03T15:00:00Z"},
"repository":{"pullRequests":{"nodes":[]}}}}"#,
        );
        put("etag-issues", r#"W/"issues-1""#);
        put("etag-pulls", r#"W/"pulls-1""#);
        dir
    }

    /// The calls and the GraphQL points of each caller so far.
    fn spent(calls: &crate::ghcalls::Calls) -> BTreeMap<String, (u32, u32)> {
        let mut by_caller: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        for row in calls.shown(crate::daemon::now_ms()).rows {
            let sum = by_caller.entry(row.caller).or_default();
            sum.0 += row.calls;
            sum.1 += row.points;
        }
        by_caller
    }

    /// One tick of a wave with no session, as the daemon makes it: the
    /// look of the loop of the wave (`wave`), the look of the daemon
    /// (`look`), and the look at the watches (`watches`), all with the
    /// `gh` of `forge`. It gives the calls and the points of each caller.
    async fn tick(
        forge: &tempfile::TempDir,
        gh: &pr::Gh,
        calls: &crate::ghcalls::Calls,
        watch: &mut look::Watch,
    ) -> BTreeMap<String, (u32, u32)> {
        use crate::cycle::Forge as _;
        let before = spent(calls);
        let wave = Wave {
            repo: "o/r".into(),
            main: forge.path().join("main"),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        let world = Fakes {
            root: forge.path().to_owned(),
            forge: Mutex::default(),
            gh: Arc::new(gh.clone().by("look")),
        };
        let none = BTreeSet::new();
        look_with(
            &gh.clone().by("wave"),
            "o/r",
            none.clone(),
            none,
            &HashSet::new(),
        )
        .unwrap();
        let told = look::look_wave(&world, &wave, watch).await.unwrap();
        assert!(!told, "nothing changed: a quiet look");
        let watches = crate::cycle::GitHub::at(gh.clone().by("watches"), &wave.main);
        watches.wave_open().unwrap();
        let mut spent = spent(calls);
        for (caller, (calls, points)) in &before {
            let sum = spent.entry(caller.clone()).or_default();
            *sum = (sum.0 - calls, sum.1 - points);
        }
        spent
    }

    /// The calls and the points of all callers in `tick`.
    fn sum(tick: &BTreeMap<String, (u32, u32)>) -> (u32, u32) {
        tick.values()
            .fold((0, 0), |(c, p), (calls, points)| (c + calls, p + points))
    }

    /// 01M6GYN97HH70E4C1FGM6HA2B0, 01M6GYN92N3Z99GPXTYHDDWF4R (#1549): a wave
    /// with no session, 1 open pull request and 20 open issues reads the
    /// forge with 12 calls and 6 GraphQL points at its first tick: two of
    /// them are the conditional reads of the look (#1566), which cost no
    /// point. The look of the daemon takes the reads of the loop of the
    /// wave. A second tick with no change makes 9 calls: it reads the
    /// default branch and the rules one time. A pull request is open, so
    /// the conditional reads save nothing here: see
    /// `an_idle_wave_with_no_pull_request_asks_github_first_and_reads_nothing`.
    /// A daemon that shares no read made 16 calls and 11 points in each
    /// tick. With a tick each 5 minutes the two waves of an idle host use
    /// 144 points an hour, under the target of 300.
    #[tokio::test]
    async fn an_idle_wave_reads_the_forge_with_few_calls_and_fewer_at_the_second_tick() {
        let forge = reads_forge(1);
        let calls = Arc::new(crate::ghcalls::Calls::default());
        let gh = pr::Gh::at(forge.path().join("gh"))
            .with_shared_reads()
            .with_calls(calls.clone());
        // The look of the daemon takes a reply of the wave that is not older
        // than the wait and one more look.
        let mut watch = look::Watch::default();
        watch.accepts = 2 * LOOK_EVERY;

        let first = tick(&forge, &gh, &calls, &mut watch).await;
        assert_eq!(first["wave"], (7, 5), "{first:?}");
        // The look of the daemon reads the rules and the Gate of `main`
        // only: the issues and the pull requests came from the wave.
        assert_eq!(first["look"], (3, 0), "{first:?}");
        assert_eq!(first["watches"], (2, 1), "{first:?}");
        assert_eq!(sum(&first), (12, 6));

        let second = tick(&forge, &gh, &calls, &mut watch).await;
        assert_eq!(sum(&second), (9, 6), "{second:?}");
        assert!(sum(&second).0 < sum(&first).0);

        // The same tick of a daemon that shares no read: each loop reads
        // the issues and the pull requests, and the default branch each time.
        let alone_calls = Arc::new(crate::ghcalls::Calls::default());
        let alone = pr::Gh::at(forge.path().join("gh")).with_calls(alone_calls.clone());
        let mut watch = look::Watch::default();
        let before = tick(&forge, &alone, &alone_calls, &mut watch).await;
        assert_eq!(sum(&before), (16, 11), "{before:?}");

        // The target of two idle waves: one tick each 5 minutes.
        let ticks = 3600 / IDLE_LOOK_EVERY.as_secs() as u32;
        let hour = 2 * ticks * sum(&second).1;
        assert_eq!(hour, 144);
        assert!(hour <= crate::ghcalls::IDLE_POINTS_AN_HOUR, "{hour}");
        // A daemon that shares no read and looks each minute.
        assert!(2 * 60 * sum(&before).1 > crate::ghcalls::IDLE_POINTS_AN_HOUR);
    }

    /// 01M6GYN92N3Z99GPXTYHDDWF4R, 01M6GYN97HH70E4C1FGM6HA2B0 (#1549): a wave
    /// with 8 sessions has 8 open pull requests, and a folder for each. The
    /// loop of the wave looks each minute, the look of the daemon takes its
    /// reads, the watch of the wave looks each 5 minutes, and the sweep of
    /// the folders makes no call: each branch has an open pull request in the
    /// reply of the wave. That is 312 GraphQL points an hour, under the
    /// target of 1,500.
    #[tokio::test]
    async fn a_wave_with_eight_sessions_stays_under_its_target() {
        let forge = reads_forge(8);
        let calls = Arc::new(crate::ghcalls::Calls::default());
        let gh = pr::Gh::at(forge.path().join("gh"))
            .with_shared_reads()
            .with_calls(calls.clone());
        let mut watch = look::Watch::default();
        watch.accepts = 2 * LOOK_EVERY;
        tick(&forge, &gh, &calls, &mut watch).await;
        let second = tick(&forge, &gh, &calls, &mut watch).await;
        let (wave, look, watches) = (second["wave"].1, second["look"].1, second["watches"].1);
        assert_eq!((wave, look, watches), (5, 0, 1), "{second:?}");

        // The sweep: 8 folders, 8 branches with an open pull request.
        let before = spent(&calls);
        let sweeper = gh.clone().by("sweep");
        for issue in 1..=8 {
            let fate = sweep::fate_on(&sweeper, "o/r", issue).unwrap();
            assert_eq!(fate, Fate::Stays);
        }
        assert_eq!(spent(&calls), before, "the sweep made a call");

        // A look each minute for the wave and the daemon, a look each 5
        // minutes for the watch.
        let minutes = 3600 / LOOK_EVERY.as_secs() as u32;
        let watches_ticks = 3600 / IDLE_LOOK_EVERY.as_secs() as u32;
        let hour = minutes * (wave + look) + watches_ticks * watches;
        assert_eq!(hour, 312);
        assert!(hour <= crate::ghcalls::BUSY_POINTS_AN_HOUR, "{hour}");
    }

    /// 01M6JFJQD4H4GG0H0TFSW5G1PD (#1566): a wave with no session, no pull
    /// request and 20 open issues asks GitHub first at each look. At its
    /// second tick the conditional reads answer `304`: the loop of the wave
    /// makes 2 calls and no GraphQL point, and the look of the daemon
    /// takes the replies of the first tick, which the `304` renewed. Only
    /// the watch of the wave still reads one point.
    #[tokio::test]
    async fn an_idle_wave_with_no_pull_request_asks_github_first_and_reads_nothing() {
        let forge = reads_forge(0);
        let calls = Arc::new(crate::ghcalls::Calls::default());
        let gh = pr::Gh::at(forge.path().join("gh"))
            .with_shared_reads()
            .with_calls(calls.clone());
        // The look of the daemon accepts a reply that is not older than a
        // quarter of a second: only a renewed reply is so new.
        let mut watch = look::Watch::default();
        watch.accepts = Duration::from_millis(250);

        let first = tick(&forge, &gh, &calls, &mut watch).await;
        assert_eq!(first["wave"], (7, 5), "{first:?}");
        tokio::time::sleep(Duration::from_millis(400)).await;

        let second = tick(&forge, &gh, &calls, &mut watch).await;
        assert_eq!(second["wave"], (2, 0), "{second:?}");
        assert_eq!(second["look"].1, 0, "{second:?}");
        assert_eq!(second["watches"].1, 1, "{second:?}");
        let kinds: Vec<String> = calls
            .shown(crate::daemon::now_ms())
            .rows
            .into_iter()
            .filter(|row| row.caller == "wave")
            .map(|row| row.kind)
            .collect();
        assert!(kinds.contains(&"api issues".to_owned()), "{kinds:?}");
        assert!(kinds.contains(&"api pulls".to_owned()), "{kinds:?}");

        // A new issue changes the newest issue and its `ETag`: the next look
        // reads in full again.
        std::fs::write(forge.path().join("etag-issues"), r#"W/"issues-2""#).unwrap();
        let third = tick(&forge, &gh, &calls, &mut watch).await;
        // The default branch is a shared reply of the first tick.
        assert_eq!(third["wave"], (6, 5), "{third:?}");
    }

    /// The seconds since the epoch now.
    fn now_secs() -> i64 {
        crate::daemon::now_ms() / 1000
    }

    /// A fake `gh` for the tests of the limit of GitHub (#1557). Each call
    /// goes in `limit.log`. `gh api rate_limit` says that GraphQL has no
    /// points until `reset` (seconds since the epoch). Before `reset` each
    /// other call fails on the limit. After it, `gh pr view --json state`
    /// gives the head commit `head`, and `gh pr view` the pull request with
    /// the trailer of issue 12. Each other call works.
    fn limited_gh(dir: &Path, reset: i64, head: &str) -> pr::Gh {
        let script = format!(
            r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/limit.log"
if [ "$1 $2" = "api rate_limit" ]; then
  echo '{{"resources":{{"graphql":{{"limit":5000,"remaining":0,"reset":{reset}}}}}}}'
  exit 0
fi
if [ "$(date +%s)" -lt {reset} ]; then
  echo 'GraphQL: API rate limit already exceeded for user ID 1.' >&2
  exit 1
fi
case "$1 $2" in
  "pr view")
    case "$*" in
      *state*) printf '%s\n' '{{"number":40,"url":"https://github.com/o/r/pull/40","headRefOid":"{head}","state":"OPEN"}}' ;;
      *) printf '%s\n' '{{"headRefOid":"{head}","body":"Closes #12\n\nIssue: #12\nMilestone: Wave 1\n"}}' ;;
    esac ;;
  "pr comment") echo https://github.com/o/r/pull/40#issuecomment-1 ;;
  "pr list") echo '[]' ;;
  *) echo '{{}}' ;;
esac
"#
        );
        pr::Gh::at(isolated::write_program(dir.join("gh"), script))
            .with_calls(Arc::new(crate::ghcalls::Calls::default()))
    }

    /// The calls that `limited_gh` got so far.
    fn limit_log(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("limit.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// 01M6H1N2XYKK6QRCJS02RZJHKZ (#1557): a look that fails on the limit of
    /// GitHub starts the wait. The wave makes no call until the reset, the
    /// lead hears of the wait one time with the reset time, and the look
    /// goes on after the reset.
    #[tokio::test]
    async fn a_rate_limit_makes_the_look_wait_until_the_reset_and_the_lead_hears_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 3, free).await;
        let bin = here._root.path().join("bin");
        let reset = now_secs() + 2;
        here.forge().limit_gh = Some(Arc::new(limited_gh(&bin, reset, "c1")));
        here.run();
        let wait = crate::ghcalls::Wait::until(crate::ghcalls::Resource::Graphql, reset * 1000);

        // The loop and the look of the daemon ran into the limit. The lead
        // hears of it one time, with the reset.
        here.until(0, |h| !h.forge().told.is_empty()).await;
        assert_eq!(here.forge().told, [text::wave_limit_waits("o/r", &wait)]);
        assert_eq!(here.forge().told_keys, [Some(wait.key())]);
        let word = here.daemon.waves_shown()[0].word.clone();
        assert_eq!(word, text::wave_limit_word(&wait));
        // A call that was on its way when the wait started ends.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let calls = limit_log(&bin);
        assert!(calls.iter().any(|c| c == "api rate_limit"), "{calls:?}");
        let pulls = |calls: &[String]| calls.iter().filter(|c| c.starts_with("pr list")).count();

        // The wave looks again at each end of a session, and yet it makes no
        // call before the reset, and says nothing more.
        here.idle(0).await;
        assert_eq!(limit_log(&bin), calls, "no call before the reset");
        assert_eq!(here.forge().told.len(), 1);

        // After the reset the look calls `gh` again, and the wave has no wait.
        here.until(0, |_| pulls(&limit_log(&bin)) > pulls(&calls))
            .await;
        here.until(0, |h| h.daemon.waves_shown()[0].word == text::WAVE_NO_WORK)
            .await;
        assert_eq!(here.forge().told.len(), 1, "no second message");
    }

    /// A fake `gh` for the tests of #1581: GitHub keeps two counters for the
    /// token. `gh api rate_limit` answers from the counter that has points
    /// (it resets at `other`). The first read of `rateLimit` lands on that
    /// counter too, each later read on the counter that is out (it resets at
    /// `reset`). Each other call fails on the limit until `fails_until`, and
    /// the failures go in `limit.failed` with their second.
    fn split_gh(dir: &Path, reset: i64, other: i64, fails_until: i64) -> pr::Gh {
        let at = |secs: i64| {
            chrono::DateTime::from_timestamp(secs, 0)
                .unwrap()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        };
        let (good, out) = (at(other), at(reset));
        let script = format!(
            r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/limit.log"
case "$1 $2" in
  "api rate_limit")
    echo '{{"resources":{{"graphql":{{"limit":5000,"remaining":3204,"reset":{other}}}}}}}'
    exit 0 ;;
  "api graphql")
    if [ "$(grep -c 'api graphql' "$dir/limit.log")" -lt 2 ]; then
      echo '{{"data":{{"rateLimit":{{"remaining":3204,"resetAt":"{good}"}}}}}}'
    else
      echo '{{"data":{{"rateLimit":{{"remaining":0,"resetAt":"{out}"}}}}}}'
    fi
    exit 0 ;;
esac
if [ "$(date +%s)" -lt {fails_until} ]; then
  printf '%s %s\n' "$(date +%s)" "$*" >> "$dir/limit.failed"
  echo 'GraphQL: API rate limit already exceeded for user ID 1.' >&2
  exit 1
fi
case "$1 $2" in
  "pr list") echo '[]' ;;
  *) echo '{{}}' ;;
esac
"#
        );
        pr::Gh::at(isolated::write_program(dir.join("gh"), script))
            .with_calls(Arc::new(crate::ghcalls::Calls::default()))
    }

    /// 01M6H1N2XYKK6QRCJS02RZJHKZ (#1581): a call fails on the counter that
    /// is out, and `gh api rate_limit` shows points left. The wave waits for
    /// the reset of the counter that failed, and the lead hears one time. A
    /// second call that fails in the same outage starts a new wait, and
    /// gives no message.
    #[tokio::test]
    async fn the_wait_ends_at_the_reset_of_the_counter_that_failed_and_the_lead_hears_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 3, free).await;
        let bin = here._root.path().join("bin");
        let reset = now_secs() + 2;
        // The counter of `rate_limit` resets 34 seconds later, and the
        // counter that failed still fails 2 seconds after its reset.
        let gh = split_gh(&bin, reset, reset + 34, reset + 3);
        here.forge().limit_gh = Some(Arc::new(gh));
        here.run();
        let wait = crate::ghcalls::Wait::until(crate::ghcalls::Resource::Graphql, reset * 1000);

        // The lead hears of the wait with the reset of the counter that
        // failed, not the reset of the counter that `rate_limit` shows.
        here.until(0, |h| !h.forge().told.is_empty()).await;
        assert_eq!(here.forge().told, [text::wave_limit_waits("o/r", &wait)]);
        assert_eq!(here.forge().told_keys, [Some(wait.key())]);
        let word = here.daemon.waves_shown()[0].word.clone();
        assert_eq!(word, text::wave_limit_word(&wait));
        let calls = limit_log(&bin);
        assert!(
            calls.iter().any(|c| c.starts_with("api graphql")),
            "{calls:?}"
        );

        // The wait ends one second after the reset, and the call fails
        // again: the counter is not over yet. It is the same outage.
        let failed_after_reset = |dir: &Path| {
            std::fs::read_to_string(dir.join("limit.failed"))
                .unwrap_or_default()
                .lines()
                .filter_map(|line| line.split_once(' ')?.0.parse::<i64>().ok())
                .any(|at| at > reset)
        };
        here.until(0, |_| failed_after_reset(&bin)).await;
        here.until(0, |h| h.daemon.waves_shown()[0].word == text::WAVE_NO_WORK)
            .await;
        assert_eq!(here.forge().told.len(), 1, "no second message");
        assert_eq!(here.forge().told_keys, [Some(wait.key())]);
    }

    /// 01M6H1N332TZ17CQQ8A5CQKG7B (#1557): a verify whose report fails on the
    /// limit of GitHub keeps its result, and the loop of the wave reports it
    /// after the reset, as the end step does.
    #[tokio::test]
    async fn a_verify_report_that_fails_on_the_limit_is_kept_and_sent_after_the_reset() {
        let built = Built::new().await;
        let sessions = tempfile::tempdir().unwrap();
        let mut record = built.record.clone();
        record.live.name = "riff-verify-issue-12".into();
        record.live.item = "verify-issue-12".into();
        record.folder = sessions.path().join("verify-issue-12-9f8e7d6c");
        std::fs::create_dir_all(&record.folder).unwrap();
        std::fs::write(
            record.folder.join(crate::daemon::EVENTS),
            "{\"type\":\"result\",\"result\":\"FAIL\\n\\n1. It does not work.\"}\n",
        )
        .unwrap();
        let reset = now_secs() + 2;
        let gh = limited_gh(built.dir.path(), reset, "1a2b3c4d5e");
        let ended = Ended {
            code: Some(0),
            failed: false,
        };

        // The end step: the report fails on the limit.
        let lines = after_verify(
            gh.clone(),
            &built.api,
            &record,
            &built.wave,
            (40, "1a2b3c4"),
            ended,
        )
        .await
        .unwrap();
        let [line] = &lines[..] else {
            panic!("one line: {lines:?}");
        };
        assert!(
            line.starts_with(
                "verify-issue-12: riff could not report the verify of pull request #40: "
            ),
            "{line}"
        );
        assert!(
            line.ends_with("riff keeps the result, and reports it after the reset."),
            "{line}"
        );
        assert_eq!(
            kept::Kept::read(&record.folder),
            Some(kept::Kept {
                repo: "o/r".into(),
                item: "verify-issue-12".into(),
                pull: 40,
                commit: "1a2b3c4".into(),
                verdict: Verdict::Fail,
                result: "1. It does not work.".into(),
            })
        );
        let thread = built.wave.thread().unwrap();
        let got = built.api.read(&built.lead, &thread, true).await.unwrap();
        let [note] = &got[..] else {
            panic!("one note: {got:?}");
        };
        assert_eq!((note.message.kind, &note.message.body), (Kind::Note, line));

        // Before the reset: no call, and the result stays.
        let calls = limit_log(built.dir.path());
        assert_eq!(
            calls,
            ["pr view 40 --json headRefOid,body", "api rate_limit"]
        );
        let report =
            || kept::report_all(&gh, &built.api, &built.lead, &built.wave, sessions.path());
        assert!(report().await.is_empty());
        assert_eq!(
            limit_log(built.dir.path()),
            calls,
            "no call before the reset"
        );
        assert!(kept::Kept::read(&record.folder).is_some());

        // After the reset: the comment, the status, and the result to the lead.
        let started = Instant::now();
        while gh.limit_wait().is_some() {
            assert!(started.elapsed() < isolated::HANG, "the wait did not end");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let lines = report().await;
        let [line] = &lines[..] else {
            panic!("one line: {lines:?}");
        };
        assert!(
            line.starts_with("Put FAIL for issue-12 on pull request #40"),
            "{line}"
        );
        let after = limit_log(built.dir.path());
        assert!(
            after.iter().any(|c| c.starts_with("pr comment 40")),
            "{after:?}"
        );
        assert!(
            after
                .iter()
                .any(|c| c.contains("repos/o/r/statuses/1a2b3c4d5e")),
            "{after:?}"
        );
        let got = built.api.read(&built.lead, &thread, true).await.unwrap();
        let [_, result] = &got[..] else {
            panic!("the note and the result: {got:?}");
        };
        assert_eq!(result.message.kind, Kind::Message);
        assert!(
            result.message.body.contains("1. It does not work."),
            "{result:?}"
        );
        assert!(!record.folder.exists(), "the folder of a FAIL goes");
        assert!(kept::find(sessions.path(), "o/r").is_empty());
        assert!(report().await.is_empty(), "reported one time");
    }

    /// 01M6H1N332TZ17CQQ8A5CQKG7B (#1557): a kept result of a pull request
    /// whose head changed is dropped. The wave starts a new verify, and
    /// reports nothing.
    #[tokio::test]
    async fn a_kept_result_of_an_old_head_is_dropped() {
        let built = Built::new().await;
        let sessions = tempfile::tempdir().unwrap();
        let folder = sessions.path().join("verify-issue-12-9f8e7d6c");
        std::fs::create_dir_all(&folder).unwrap();
        let old = kept::Kept {
            repo: "o/r".into(),
            item: "verify-issue-12".into(),
            pull: 40,
            commit: "1a2b3c4".into(),
            verdict: Verdict::Pass,
            result: "- Done.".into(),
        };
        old.keep(&folder).unwrap();
        // A result of another repository stays where it is.
        let other = sessions.path().join("verify-issue-3-aaaaaaaa");
        std::fs::create_dir_all(&other).unwrap();
        kept::Kept {
            repo: "o/s".into(),
            ..old.clone()
        }
        .keep(&other)
        .unwrap();

        // The pull request has a new head: a push after the verify.
        let gh = limited_gh(built.dir.path(), 0, "9f8e7d6c5b4a");
        let lines =
            kept::report_all(&gh, &built.api, &built.lead, &built.wave, sessions.path()).await;
        assert_eq!(
            lines,
            [text::wave_kept_dropped("verify-issue-12", 40, "1a2b3c4")]
        );
        assert!(!folder.exists(), "the result goes");
        assert!(kept::Kept::read(&other).is_some());
        let calls = limit_log(built.dir.path());
        assert_eq!(
            calls,
            ["pr view 40 --json number,url,headRefOid,state"],
            "no report"
        );
        let thread = built.wave.thread().unwrap();
        let got = built.api.read(&built.lead, &thread, true).await.unwrap();
        assert!(got.is_empty(), "nothing for the lead: {got:?}");
    }

    /// 01M4ZV3B6R4WSEJXYT7QPRWT6X: a report that fails for another reason
    /// than a limit keeps nothing: the folder goes, and the lead gets the
    /// message, as before.
    #[tokio::test]
    async fn a_verify_report_that_fails_for_another_reason_is_not_kept() {
        let built = Built::new().await;
        let mut record = built.record.clone();
        record.live.item = "verify-issue-12".into();
        std::fs::write(
            record.folder.join(crate::daemon::EVENTS),
            "{\"type\":\"result\",\"result\":\"PASS\\n\\n- Done.\"}\n",
        )
        .unwrap();
        let gh = pr::Gh::at(isolated::write_program(
            built.dir.path().join("gh"),
            "#!/bin/sh\necho 'gh: Bad Gateway (HTTP 502)' >&2\nexit 1\n",
        ));
        let ended = Ended {
            code: Some(0),
            failed: false,
        };
        let lines = after_verify(gh, &built.api, &record, &built.wave, (40, "1a2b3c4"), ended)
            .await
            .unwrap();
        let [line] = &lines[..] else {
            panic!("one line: {lines:?}");
        };
        assert!(
            line.ends_with("The pull request waits for a new verify."),
            "{line}"
        );
        assert!(!record.folder.exists());
        let thread = built.wave.thread().unwrap();
        let got = built.api.read(&built.lead, &thread, true).await.unwrap();
        let [message] = &got[..] else {
            panic!("one message: {got:?}");
        };
        assert_eq!(message.message.kind, Kind::Message);
    }
}
