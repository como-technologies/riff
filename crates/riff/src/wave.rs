//! Let riff run the wave (#719): the riff daemon starts one session for
//! each free item of the current wave, one for each pull request that
//! waits for a verify (decisions 8 and 34 of #699), one to rebase
//! each pull request with a conflict with `main` (#1082), and one to fix
//! each pull request with a failed check. It tells the lead of a pull
//! request that no session can fix (#279).
//!
//! # Design
//!
//! `riff host join` in a clone asks the daemon to run the wave of that
//! repository ([`Wave`]): the host gets the part workers
//! ([`crate::daemon::parts`], #1643). The daemon keeps each wave in
//! [`WAVES_FILE`] in its folder, and starts its loop again after its own
//! restart and after an update, while the host has the part workers.
//! `riff host leave` takes the part workers and ends the loop: the daemon
//! starts no new session there, and the sessions that run go on, or with
//! `--now` it ends them. The rollout of a lead starts no wave on a host
//! with no part workers ([`crate::rollout::WaveState::NoWorkers`]).
//!
//! A wave runs only for a project of the riff that allows the host
//! ([`crate::project::wave_refused`], 01M77FHMPV0NQ407TKC2F7J41Q,
//! 01M77FHMTD5Y5FXRD903S2ZGCX). `riff host join` refuses first, before the
//! image ([`ask`]). The loop reads the projects before each look
//! ([`World::projects`]): when they refuse the host, the look reads
//! nothing, the wave starts no new session, and its word says why
//! ([`crate::project::Refused::word`]). The sessions that run go on.
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
//!     P -- "no work" --> W["give the room back; wait for an end, a change of the plan,<br/>a wake of GitHub, LOOK_EVERY, IDLE_LOOK_EVERY or the safety look"] --> A
//!     P -- "an item" --> J["a job: the wait before the claim, claim, clone, start,<br/>wait for the end, the steps after the end, release"]
//!     J -.-> A
//! ```
//!
//! # The look and the limit of GitHub
//!
//! The token of a person has 5,000 GraphQL points an hour for all its
//! sessions, looks and leads (#1549, [`crate::ghcalls`]). A wave that has
//! no work, no session and no change since its last look waits
//! [`IDLE_LOOK_EVERY`] for the next look, not [`LOOK_EVERY`]. The end of a
//! session or of a job wakes it at once, and so does a change of the plan
//! on riff-server (see "The hosts of a wave").
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
//! The loop also follows the wakes of the GitHub App of its repository
//! ([`World::github_wakes`], [`crate::hooks`], #1656). A wake wakes the
//! wave, at most one look each [`hooks::WAKE_GAP`]. While the events are
//! on ([`hooks::Told::on`]), the look reads only what the wakes named, and
//! no conditional read ([`look_told`]). The safety look reads in full each
//! [`hooks::SAFETY_LOOK_EVERY`], and an idle wave waits as long. It finds
//! each change that no wake told: a dropped event. The look of the daemon
//! takes the replies of the loop for as long.
//!
//! ```mermaid
//! flowchart TD
//!     L["a look"] --> Q{"no work, no session,<br/>the same facts as the last look?"}
//!     Q -- "yes, the events of GitHub are on" --> E["wait SAFETY_LOOK_EVERY"]
//!     Q -- "yes, with no events" --> I["wait IDLE_LOOK_EVERY"]
//!     Q -- no --> N["wait LOOK_EVERY"]
//!     E -- "a session or a job ends,<br/>a change of the plan, a wake of GitHub" --> L
//!     I -- "a session or a job ends,<br/>a change of the plan" --> L
//!     N -- "a session or a job ends,<br/>a change of the plan, a wake of GitHub" --> L
//! ```
//!
//! # The hosts of a wave
//!
//! A wave can run on more hosts. The claims on riff-server give each item
//! to one session, and the host with the most free room takes a free item
//! first (#1729, 01M6WJNG3YA4PS8B5DQWPTBXBR, 01M6WJNV9KP5TN5SVBVT4JDN66):
//!
//! - **A change of the plan wakes each wave.** riff-server sends one
//!   event on `GET /v1/plan/watch` of the repository thread for each
//!   command that holds an item, frees an item, releases a claim, sets the
//!   plan or ends it ([`riff_core::record::Record::plan_thread`]), and for
//!   a new item of a wave and a merge on GitHub
//!   (`riff_server::hooks`, 01M74A37JNKW1R2WRXAW520WJM). The loop
//!   of each wave follows the stream ([`World::plan_changes`],
//!   [`follow_plan`]) and looks at once, also an idle wave. So the hosts
//!   see a free item at the same time.
//! - **The host with the most free room claims first.** Before the claim
//!   of an item, a job waits [`claim_wait`]: [`CLAIM_WAIT`] × the live
//!   sessions of the host / its limit, so 0 s on an empty host and 7.5 s
//!   with 3 sessions of 4. Then it reads the claims and the holds again,
//!   and claims the item only when it is still free (`wait_to_claim`).
//!   Else the start does not count (`Taken`), and the lead hears
//!   nothing. A job that the lead asked for does not wait.
//! - With the same load, the first host to look takes the item. No host
//!   has a fixed preference.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant S as riff-server
//!     participant B as the wave of pangolin: 3 of 4
//!     participant E as the wave of thelio: 0 of 8
//!     Note over S: the lead frees issue-12
//!     S->>B: plan/watch: a change
//!     S->>E: plan/watch: a change
//!     B->>B: look: issue-12 is free, wait 7.5 s
//!     E->>E: look: issue-12 is free, wait 0 s
//!     E->>S: claim issue-12: granted
//!     B->>S: who: issue-12 is claimed
//!     Note over B: no claim, the start does not count
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
//!   [`ghcalls::Wait::key`]: each wait of one outage has the key of its first
//!   wait, also when the reset moves at each try (#1872). It sleeps until the
//!   reset. A look that ran into the limit does the same at once.
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
//! Before a notice of auto-merge off, the loop asks the forge again
//! ([`World::merge_off`], 01M777N148MY8NR5RNDKXE9EVJ): a pull request that
//! the queue merged between the two reads of the look gets no notice.
//!
//! | Notice | Key | The lead gets it |
//! |---|---|---|
//! | An issue with no `Done when:` line ([`text::wave_no_done_when`]) | [`no_done_when_key`] | one time |
//! | A pull request that is stuck, or a failed check that no rule requires ([`text::wave_stuck`]) | [`Notice::post_key`] | one time |
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
//! # The question label
//!
//! A lead that needs the answer of a person writes the questions in the body
//! of the issue, and adds the label `question`
//! ([`riff_client::waves::QUESTION`], 01M6QBJA0J7PQ0J29HWM1KDVHJ). The wave
//! then starts no build of the issue ([`rollout::Issue::question`],
//! 01M6QBJHM5BQR59QX2K2BMZVYH), also no rebase and no fix of checks for it:
//! the work waits for the answer. A verify of its open pull request goes on.
//! riff posts no chat line for it (01M75QTMB42NME9Z2Y0XC0AN97): the Waves
//! tab of the TUI and `riff questions` show each open question. When the
//! person has answered, the lead removes the label, and the next look
//! takes the issue as free work.
//!
//! ```mermaid
//! flowchart LR
//!     L["the lead: questions in the body,<br/>label question"] --> F["the forge"]
//!     F --> W["the look of the wave:<br/>no build of the issue"]
//!     F --> T["the Waves tab and riff questions:<br/>the open questions"]
//!     T --> P["the person answers<br/>in the body or in a comment"]
//!     P --> R["the lead records the answer<br/>and removes the label"]
//!     R --> W2["the wave takes the issue"]
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
//! - an item while the repository has no live lead
//!   ([`Look::no_lead`], 01M776RSYKJK9NETFAD8G0CCVQ). This wait starts no
//!   session and stops none: the jobs that run go on to their end
//!   (01M776RT1GW3N4MJDMPXZ8JAJE). A new lead of the repository is a
//!   change of the plan, so the wave looks at once;
//! - a held item (01M4ZV3B6M7PZRJ39XTXBZP3ZH);
//! - a tracker issue, its build or its verify: it has no code of its
//!   own ([`rollout::Issue::tracker`], 01M5ND16V2SVN8A9K7W90VC6X9);
//! - a build of an issue with the label `question`, as for a hold: the work
//!   waits for the answer of a person ([`rollout::Issue::question`], #1605).
//!   The verify of its open pull request goes on, and the lead of the
//!   person hears of the label one time (see "The question label");
//! - a build of an issue whose body has no line that starts with
//!   `Done when:` ([`rollout::Issue::has_done_when`],
//!   01M5R0HEP1MXWYY643F1N1DWEM): a session stops at once without it. The
//!   lead gets one message for each such issue ([`Look::no_done_when`],
//!   [`text::wave_no_done_when`]). The wave takes the issue at its next
//!   look after the line comes. A verify and a rebase do not change;
//! - a build of an issue whose pull request merged with `Refs #N`: the
//!   item is merged, also before its comment `Merged in` (see "A merge
//!   that leaves the issue open"). Nor a build of an issue whose pull
//!   request merged with `Closes #N`, before the close of the issue
//!   reaches the look (see "A merge that closes the issue");
//! - an item that a session claims, or that a job of the wave runs;
//! - the verify of an item while a session holds the item: its author
//!   still works. The verify is a new session: an author never verifies
//!   its own item (01M4ZV3B6NKPEZ5FNE27X95GF0);
//! - a rebase or the fix of the checks of an item while a verify of the
//!   item runs (01M6QC6KV25VMRREXXFBJD7NEJ, #1593): the verify tests the
//!   head that the fix replaces, so its result would be for an old head.
//!   The fix or the rebase starts at the look after the verify ends;
//! - a verify before the full check of the head commit is done: each
//!   check that the rules of the default branch require, but `riff/verify`
//!   ([`repo_setup::full_check`], 01M5QYDHN27RC5SRHAB56PFKK0), has a run,
//!   and each newest run is completed ([`pr::full_done`]). A check with no
//!   run is not done, also when another check failed (#1575);
//! - a verify of a head with a failed check that the ruleset of the
//!   default branch requires: the fix of the checks comes first (see "A
//!   pull request that cannot merge", #1575). A failed check that it does
//!   not require holds no verify (01M749P62T24VDPX20D40KRJEK);
//! - a verify of a head whose verify passed, or whose verify failed with
//!   no new verify request after the FAIL (see "A look that does not see
//!   the verdict yet");
//! - a build or a verify of a head whose verify passed after a fail
//!   (01M77JWXEGDDMB7TW97APGKW9W, #2014). riff posts the PASS in the
//!   thread after it sets the status, so the look can still read the old
//!   failure. The PASS in the thread counts over it ([`said`],
//!   [`rollout::Pull::verify`]);
//! - a verify or a rebase of a pull request whose head conflicts with the
//!   head of a pull request before it: one whose verify runs or passed, or
//!   whose item had its verify request first ([`order()`],
//!   [`crate::order`], 01M6WCGNCC3HWS47158WTWNQ8B,
//!   01M6YPYTGR2JEBDHGX4K1RDCAW, 01M750ED646BKBZSA9KHF0953Y). Two heads
//!   that change a file in common conflict unless `git merge-tree` in the
//!   main clone says that they merge with no conflict
//!   ([`order::merges`]). A session of its item does not put a pull
//!   request before the others (01M74EP20TDQEXW42MK0XE73QF).
//!   It waits until that pull request merges or its verify fails. The loop
//!   keeps these waits for the look and for `riff wave list`
//!   ([`Daemon::order_waits`]);
//! - an item that started [`STARTS`] times in this run of the wave. The
//!   lead gets one message for it. A start whose claim riff-server
//!   refused for a pause does not count (01M558EYCXPFW8CJV3XJ7HK3NR).
//!   `riff host join` on the wave clears the starts
//!   (01M57A7A48A70Y7PTD95A3MBYC). The starts of a verify are for its head
//!   commit ([`Work::key`], 01M6QQ7H2B88AT3KS8NGM3DKCZ): a new head starts
//!   a new count, and a verify whose PASS or FAIL riff put on the pull
//!   request does not count ([`Finish::reported`]). The message for a
//!   verify names the head, and comes after the last session ended, for
//!   the head that the pull request has now (01M6QQ7NZTF2G4MRXKWWG30CFX,
//!   [`text::wave_spent`]);
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
//! # A look that does not see the verdict yet
//!
//! A head commit gets one verify that passes (01M75R717PH1NCQJ338ASVYT63,
//! #1960). After a FAIL, it gets a new verify only for a new verify
//! request that comes after the FAIL in the thread
//! (01M77K0EGH1HF0WA7WZH8BP2BP, #2019): the lead asks for it. The look
//! can still show the head as waiting for a verify for a short time:
//! before the event of its status `riff/verify` comes, or after a verify
//! of another host. So:
//!
//! - The daemon keeps the verdict of each head whose PASS or FAIL it put
//!   on the pull request ([`Finish::verdict`]), or that the read before a
//!   claim found. The loop takes each such verify out of its looks until
//!   a look sees the status ([`drop_verified`]).
//! - The job reads the status `riff/verify` of the head before the claim
//!   ([`World::verdict`]). A success starts no session, and the start
//!   does not count. So does a failure, unless the look shows a new
//!   verify request of the head after its result ([`Waiting::again`],
//!   [`asked_again`]).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant G as GitHub
//!     participant W as the loop of the wave
//!     participant T as the thread
//!     W->>G: the verify of c1 ends: FAIL, riff/verify failure
//!     W->>T: verify result: FAIL for issue-12, commit c1
//!     Note over W: the daemon keeps verify-issue-12@c1: FAIL
//!     W->>W: a look before the event of the status: c1 waits for a verify
//!     Note over W: drop_verified: no second verify
//!     T->>W: the lead: verify request: issue-12, commit c1
//!     W->>W: a look: a new request after the FAIL
//!     W->>G: the read before the claim: failure, asked again
//!     Note over W: claim verify-issue-12: a new verify of c1
//! ```
//!
//! # A merge that leaves the issue open
//!
//! The pull request of an issue with a check after the release has
//! `Refs #N` (R194), so the merge leaves the issue open. The look reads the
//! last 100 merged pull requests with `gh` ([`rollout::merges`],
//! 01M6XSSCTA3E1GJ8EC9646PP1R). An issue whose pull request merged with
//! `Refs #N` is merged at once ([`rollout::Issue::merged`]): it is no free
//! work, also before it has the comment `Merged in #PR (COMMIT)`. The loop
//! adds that comment at the first look that finds the issue with no
//! comment, before it starts any work ([`Look::merged`],
//! [`World::merged_in`]). It adds it one time. A comment that fails comes
//! at the next look. The sweep adds it too, when it is still missing (see
//! "The clones").
//!
//! ```mermaid
//! sequenceDiagram
//!     participant G as GitHub
//!     participant W as the loop of the wave
//!     G->>G: pull request #40 merges with Refs #12: the issue stays open
//!     W->>G: look: open issues, open and merged pull requests
//!     Note over W: #12 is merged: no free work
//!     W->>G: gh issue comment 12: Merged in #40 (COMMIT)
//!     W->>W: pick: the next free item
//! ```
//!
//! # A merge that closes the issue
//!
//! GitHub closes the issue of a pull request that merges with `Closes #N`.
//! The close comes to the look later than the merge: the event of the
//! issue can come after the event of the pull request, and the safety
//! look reads the open issues each [`hooks::SAFETY_LOOK_EVERY`]. In that
//! time the issue is open with no open pull request. So the merges of the
//! look ([`rollout::merges_of`], 01M75WEDFKXGHMRFR73D1QDZCE) also count a
//! merged pull request with no line `Refs #N`, for
//! [`rollout::CLOSES_FOR`] after its merge: its issue is merged, and the
//! wave starts no build of it. Such a merge gets no comment `Merged in`.
//! An issue that a person opens again is free work after that time.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant G as GitHub
//!     participant W as the loop of the wave
//!     G->>W: the event of pull request #40: merged, Closes #12
//!     W->>G: gh pr view 40: the kept list of the merged ones
//!     Note over W: #12 is still open in the kept list, but merged: no build
//!     G->>W: the event of issue #12: closed
//!     W->>G: gh issue view 12: #12 leaves the kept list
//! ```
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
//! verify request for the same head comes ([`said`], #1157): then it
//! waits for a verify again. A verify request and a verify result count
//! only from their right sender ([`right_sender`], #2027).
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
//! The rebases go in the order of the verifies ([`order()`], #1801). A
//! merge can give each open pull request whose head conflicts with its
//! head a conflict. Of these, only the next of the order gets its
//! rebase. The others keep their conflict until the pull request before
//! them merges or its verify fails, and the look sends no message of
//! their conflict.
//! A pull request keeps its place while a session rebases it, and after
//! its push, also while it reads `UNKNOWN`. The session does not make it
//! go first (01M74EP20TDQEXW42MK0XE73QF).
//!
//! Each head commit with a conflict starts one session in a run of the
//! wave: [`Work::key`] names the rebase by the item and the head. A pause
//! or a fault of the host takes the start back, as for each item.
//! `riff host join` clears the starts, so a person can start the rebase
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
//!     S["riff host join"] --> L["the loop: the work"]
//!     S --> W["the look, each minute"]
//!     W --> P["a pull request that stops:<br/>a message to its holder or the lead"]
//!     W --> M["the full check of main:<br/>a message, the fix item, a note"]
//!     X["riff host leave"] --> E["the loop and the look end"]
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
//!     P["an open pull request,<br/>the verify passed or waits"] --> Q{"a check of the head failed<br/>that the ruleset does not require?"}
//!     Q -- "yes" --> I["a message to the lead for each such check:<br/>the verify goes on"]
//!     Q --> C{"a required check of the head failed?<br/>(statusCheckRollup)"}
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
//! - **A failed check** (01M5QW2KVAS05Q72630RNW47G0,
//!   01M74MGTCR650HNX4E1THAAG69). [`rollout::Pull::verify`] reads only
//!   the status `riff/verify`. A
//!   check of the rollup that failed ([`rollout::Check::failed`]) and that
//!   the ruleset of the default branch requires, for example the `Gate` or
//!   `Hygiene`, is no free work for a verify: the verify of that head can
//!   only fail (#1575). The job `Gate` has no run
//!   until the other jobs end, so `Hygiene` can fail while the `Gate` has
//!   none. The fix starts before or after a verify, not while a verify
//!   of the item runs.
//!   The wave starts a build session for the item with the start text
//!   [`text::wave_checks_start`]: it names the failed required runs of
//!   Actions ([`rollout::Pull::failed_runs`]), tells to read the
//!   failed run, and to fix the code and push to the same pull request.
//!   For a failed pull of the session image, with no test run, it tells
//!   to change no code and to run the failed jobs again one time
//!   (01M6QPKG4MSK9AF70V6SZHSBKZ).
//!   For a known flake it allows one run of the job again. The rollup
//!   does not say which check a rule requires: the look reads the ruleset
//!   when a head has a failed check ([`rollout::Pull::required`]). Only
//!   the newest run of a check counts (#1540,
//!   [`rollout::Pull::failed_checks`]): an old failed run does not count
//!   when a newer run of the same check passed. A check that was
//!   cancelled or skipped did not fail.
//! - **A failed check that no rule requires**
//!   (01M749P62T24VDPX20D40KRJEK). A check that the ruleset of the default
//!   branch does not require blocks no merge, and `riff verify pass` does
//!   not read it. So it holds no verify, and no session fixes it
//!   ([`rollout::Pull::failed_optional`]). The lead gets one message for
//!   each such check and head commit, with its name and its link
//!   ([`rollout::Stuck::Optional`]). The pull request is not stuck: `riff
//!   top` shows no reason for it.
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
//! - **A review** (01M6FZ141Y0J8G2AYX0G5HY1CX, 01M74MGTCR650HNX4E1THAAG69).
//!   A pull request whose verify passed, with
//!   auto-merge on, can wait for a person. GitHub says `reviewDecision`
//!   `REVIEW_REQUIRED` when a rule of the branch asks for a review
//!   ([`rollout::Review::Required`]). It says nothing when the count of
//!   reviews is 0 and the rule "require extra approval for unattributed
//!   changes" asks one review for a commit with an author who has no
//!   GitHub account. Then the merge state is `BLOCKED`, no required check
//!   failed, no check goes, and no run waits ([`rollout::Review::Blocked`],
//!   [`rollout::Pull::settled`]). A failed check that no rule requires
//!   does not explain the block. No session approves a pull request. The
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
//! - **A release** (01M5QW2M4WX53N29SHRV0S32WM,
//!   01M74MGTCR650HNX4E1THAAG69). No session builds a release (the label
//!   `release`, or a branch `release-…`). A failed required check of a
//!   release pull request gets a message to the lead, with the names of
//!   the checks, at any state of its verify. A failed check of a release
//!   that no rule requires gets its message only when the verify waits or
//!   passed, as for each pull request.
//!
//! Each stuck state comes one time. A message is keyed by the pull
//! request, its head commit and its reason (and the run, for a run that
//! waits for approval); the build of a failed check
//! by the item and the head ([`Work::key`]). So a new head is a new
//! state. A message is not repeated while the wave runs. A build of a
//! failed check runs one time for a head in a run of the wave
//! ([`CHECKS`]), and `riff host join` clears the starts. When a pull
//! request has a conflict and a failed check, the rebase comes first:
//! the new head has new checks.
//!
//! The session may not push: for a flake it runs the job again
//! (01M5QW2KYFM4C287D9NMSPH8TA, 01M74MGTCR650HNX4E1THAAG69). After its
//! end the daemon reads the pull request. When the head is the same and
//! a required run of Actions still failed ([`rollout::Pull::failed_runs`]),
//! the lead gets a message ([`text::wave_checks_not_fixed`]); else the
//! lead gets a note ([`text::wave_checks_rerun`]). A failed run that no
//! rule requires gives the note. A push gets the verify request of the
//! new head, as each build does.
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
//! it first adds the comment `Merged in #PR (COMMIT)` to the issue, when
//! the issue does not have it (see "A merge that leaves the issue open").
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
    BotFact, GitHubRenew, HoldInfo, Kind, Message, ProjectFacts, PullState, RiffReply, RiffState,
    StartReason, WORKER_ALIVE_EVERY, Wake,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;

use crate::daemon::gh_token;
use crate::daemon::remote::Abort;
use crate::daemon::sweep::{self, Fate, InJob};
use crate::daemon::{Daemon, Ended, Out, PullFailed, Record, Say, Start};
use crate::pr::Verdict;
use crate::room::Slot;
use crate::run::{self, Facts};
use crate::runtime::Runtime;
use crate::session_setup::Setup;
use crate::{
    changes, ghcalls, hooks, identity, kept, look, order, pr, project, repo_setup, rollout,
    session_image, text, usage,
};
use riff_client::api::{self, Api};
use riff_client::link::STREAM_RETRY;

/// The file of the waves that the daemon runs, in its folder.
pub const WAVES_FILE: &str = "waves.json";

/// How often a wave looks for free work when nothing ends.
pub const LOOK_EVERY: Duration = Duration::from_secs(60);

/// How often an idle wave looks (#1549, [`crate::ghcalls`]): no session of
/// the wave runs, no work is free, and the look saw the same facts as the
/// look before ([`look_wait`]). A session that ends, a job that ends and a
/// change of the plan on riff-server wake the wave at once. While the
/// events of GitHub are on, an idle wave waits for its safety look
/// ([`hooks::SAFETY_LOOK_EVERY`], #1656).
pub const IDLE_LOOK_EVERY: Duration =
    Duration::from_secs(LOOK_EVERY.as_secs() * crate::watches::IDLE_FACTOR as u64);

/// The wait of a full host before the claim of free work (#1729): a host
/// waits a part of it that grows with its load ([`claim_wait`]).
pub const CLAIM_WAIT: Duration = Duration::from_secs(10);

/// How long a host with `live` sessions and the limit `limit` waits before
/// the claim of free work (#1729): `full` × `live` / `limit`, with `full`
/// [`CLAIM_WAIT`]. So the host with the most free room claims first. A
/// host at its limit or past it, and a limit of 0, wait `full`.
///
/// ```
/// use std::time::Duration;
/// use riff::wave::{CLAIM_WAIT, claim_wait};
///
/// assert_eq!(claim_wait(0, 8, CLAIM_WAIT), Duration::ZERO);
/// assert_eq!(claim_wait(3, 4, CLAIM_WAIT), Duration::from_millis(7_500));
/// assert_eq!(claim_wait(4, 8, CLAIM_WAIT), Duration::from_secs(5));
/// assert_eq!(claim_wait(5, 4, CLAIM_WAIT), CLAIM_WAIT);
/// assert_eq!(claim_wait(0, 0, CLAIM_WAIT), CLAIM_WAIT);
/// ```
pub fn claim_wait(live: usize, limit: u32, full: Duration) -> Duration {
    if limit == 0 {
        return full;
    }
    let live = u32::try_from(live).unwrap_or(u32::MAX).min(limit);
    full * live / limit
}

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

/// `riff host join`: asks the daemon in the folder `dir` to serve the
/// repository of the place `here`, to give the host the part workers, and
/// to run the current wave, on the riff-server `server`
/// (01M4ZV3B6J89KJQ0F14YWTZMJQ, #1643).
pub async fn ask_join(dir: &Path, here: &Place, server: &str) -> Result<()> {
    let repo = ask(dir, here, server, |line| println!("{line}")).await?;
    println!("{}", text::wave_asked(&repo));
    Ok(())
}

/// [`ask_join`] with each line of the steps to `show`, for a caller with
/// no stdout of its own, as `riff mcp` (01M53NGVE839TRH7XB7CDPQNZE). It
/// gives the repository, as `OWNER/REPO`.
///
/// First the repository must be a project of the riff that allows the
/// host ([`crate::project::wave_allowed`], #1863). Else riff asks the
/// daemon nothing, and fails with the fix.
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
    // A wave only for a project that allows the host (#1863).
    project::wave_allowed(here, server).await?;
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
    crate::daemon::ask_done(dir, crate::daemon::Ask::Join { wave }, show).await?;
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

/// True when the host of the daemon of `status` has the part workers in
/// `repo` (`OWNER/REPO`): it joined, and did not leave (#1643).
///
/// ```
/// let status: riff::daemon::Status = serde_json::from_value(serde_json::json!({
///     "pid": 7, "build": "3.0.0", "since_ms": 0, "dir": "/d", "sessions": [],
///     "parts": [
///         {"repo": "o/r", "workers": true, "lead": false},
///         {"repo": "o/lead", "workers": false, "lead": true},
///     ],
/// }))?;
/// assert!(riff::wave::gives_workers(&status, "o/r"));
/// assert!(!riff::wave::gives_workers(&status, "o/lead"));
/// assert!(!riff::wave::gives_workers(&status, "o/other"));
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn gives_workers(status: &crate::daemon::Status, repo: &str) -> bool {
    status
        .parts
        .iter()
        .any(|p| p.repo == repo && p.parts.workers)
}

/// `riff host leave`: asks the daemon in the folder `dir` to start no new
/// session for the repository of the place `here`. With `now`, it ends the
/// sessions that run, and frees their items (#1643).
pub async fn ask_leave(dir: &Path, here: &Place, now: bool) -> Result<()> {
    if here.default_thread().is_none() {
        bail!("{}", text::RUN_NO_REPO);
    }
    let repo = here.repo_text();
    let ask = crate::daemon::Ask::Leave { repo, now };
    crate::daemon::ask_done(dir, ask, |line| println!("{line}")).await
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
    /// head (#279). So does a verify: each head of a pull request has its
    /// own count (#1651).
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
    /// let verify = |commit: &str| Work::Verify { issue: 12, pull: 40, commit: commit.into() };
    /// assert_eq!(verify("1a2b").key(), "verify-issue-12@1a2b");
    /// assert_ne!(verify("1a2b").key(), verify("5e6f").key());
    /// ```
    pub fn key(&self) -> String {
        match self {
            Work::Rebase { commit, .. } | Work::Verify { commit, .. } => {
                format!("{}@{commit}", self.item())
            }
            Work::Checks { commit, .. } => format!("{}@{commit}:checks", self.item()),
            Work::Build { .. } => self.item(),
        }
    }

    /// The head commit that this work is for: none for a build.
    pub fn commit(&self) -> Option<&str> {
        match self {
            Work::Build { .. } => None,
            Work::Verify { commit, .. }
            | Work::Rebase { commit, .. }
            | Work::Checks { commit, .. } => Some(commit),
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
    /// True when a verify request for the head came after a verify result
    /// for it in the thread ([`asked_again`]): a head whose verify failed
    /// gets a new verify (01M77K0EGH1HF0WA7WZH8BP2BP).
    pub again: bool,
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
    /// Why it cannot merge, or the failed check that no rule requires
    /// ([`rollout::Stuck::Optional`]).
    pub stuck: rollout::Stuck,
}

impl Notice {
    /// The key of the message: the pull request, its head and its
    /// reason. A message with the same key goes one time. A failed check
    /// of another app than GitHub Actions, and a failed check that the
    /// ruleset does not require, have a message for each check, and a run
    /// that waits for approval one for each run.
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
    /// let optional = |name: &str| Stuck::Optional(OtherCheck { name: name.into(), app: None, link: None });
    /// assert_eq!(notice("1a2b", optional("knurl adapter")).key(), "#40@1a2b:optional check:knurl adapter");
    /// assert_ne!(notice("1a2b", optional("a")).key(), notice("1a2b", optional("b")).key());
    /// let review = notice("1a2b", Stuck::Review(Review::Required));
    /// assert_eq!(review.key(), "#40@1a2b:review");
    /// let run = |id| Stuck::Approve(Run { id, name: "Hygiene".into(), link: None });
    /// assert_eq!(notice("1a2b", run(7)).key(), "#40@1a2b:approve run:7");
    /// assert_ne!(notice("1a2b", run(7)).key(), notice("1a2b", run(8)).key());
    /// ```
    pub fn key(&self) -> String {
        let key = format!("#{}@{}:{}", self.pull, self.commit, self.stuck.word());
        match &self.stuck {
            rollout::Stuck::Other(check) | rollout::Stuck::Optional(check) => {
                format!("{key}:{}", check.name)
            }
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
    /// True when the repository has no live lead: the wave waits for a
    /// lead, and its word says why ([`text::wave_no_lead`],
    /// 01M776RSYKJK9NETFAD8G0CCVQ). It is no pause.
    pub no_lead: bool,
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
    /// The open pull requests of the items whose verify waits, runs or
    /// passed, or that wait for a rebase, by issue, with the head and the
    /// files of each, for the order of the verifies and the rebases
    /// ([`order()`], #1710, #1801). The live look adds the merge test of
    /// each two heads with a file in common ([`order::merges`], #1895).
    /// None while fewer than two are open or no verify and no rebase of
    /// them is free work.
    pub lanes: BTreeMap<u64, order::Lane>,
    /// The pull requests that cannot merge by themselves.
    pub fixes: Fixes,
    /// The issues that would be free work but for the missing line
    /// `Done when:` in their body
    /// ([`rollout::Open::no_done_when`]). The lead hears of each once.
    pub no_done_when: BTreeSet<u64>,
    /// The open issues whose pull request merged with `Refs #N` and that
    /// have no comment `Merged in #PULL` yet ([`rollout::Open::unsaid`]),
    /// by issue. They are no free work. The loop adds the comment to each
    /// before it starts any work ([`World::merged_in`], #1754).
    pub merged: BTreeMap<u64, rollout::Merge>,
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
///     pulls: [(9, Waiting { pull: 40, commit: "1a2b".into(), gate_done: true, again: false })].into(),
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
/// assert_eq!(pick(&Look { paused: true, ..look.clone() }, &none, &BTreeMap::new()), None);
/// // A repository with no live lead: the wave waits for a lead.
/// assert_eq!(pick(&Look { no_lead: true, ..look }, &none, &BTreeMap::new()), None);
///
/// // The pull request of 14 has a conflict: one rebase for its head.
/// let conflict = Conflict { pull: 54, commit: "5e6f".into() };
/// let look = Look { free: vec!["issue-14".into()], fixes: Fixes { conflicts: [(14, conflict)].into(), ..Fixes::default() }, ..Look::default() };
/// let rebase = Work::Rebase { issue: 14, pull: 54, commit: "5e6f".into() };
/// assert_eq!(pick(&look, &none, &BTreeMap::new()), Some(rebase.clone()));
/// assert_eq!(pick(&look, &none, &[(rebase.key(), 1)].into()), None);
/// // The starts of the build do not stop the rebase.
/// assert_eq!(pick(&look, &none, &[("issue-14".to_owned(), 3)].into()), Some(rebase.clone()));
/// // A verify of 14 runs, in this daemon or by a claim: the rebase waits.
/// let verifying: BTreeSet<String> = ["verify-issue-14".to_owned()].into();
/// assert_eq!(pick(&look, &verifying, &BTreeMap::new()), None);
/// let claimed = Look { claims: verifying, ..look.clone() };
/// assert_eq!(pick(&claimed, &none, &BTreeMap::new()), None);
/// // The verify of another item does not stop it.
/// let other: BTreeSet<String> = ["verify-issue-9".to_owned()].into();
/// assert_eq!(pick(&look, &other, &BTreeMap::new()), Some(rebase));
///
/// // The checks of the pull request of 15 failed after a pass: one fix for its head.
/// let failed = Failed { pull: 55, commit: "7a8b".into(), checks: vec!["Gate".into()] };
/// let look = Look { free: vec!["issue-15".into()], fixes: Fixes { failed: [(15, failed)].into(), ..Fixes::default() }, ..Look::default() };
/// let fix = Work::Checks { issue: 15, pull: 55, commit: "7a8b".into(), checks: vec!["Gate".into()] };
/// assert_eq!(pick(&look, &none, &BTreeMap::new()), Some(fix.clone()));
/// assert_eq!(pick(&look, &none, &[(fix.key(), 1)].into()), None);
/// // A verify of 15 runs: the fix waits for its end.
/// let verifying: BTreeSet<String> = ["verify-issue-15".to_owned()].into();
/// assert_eq!(pick(&look, &verifying, &BTreeMap::new()), None);
/// ```
///
/// A verify or a rebase that waits for another pull request in its group
/// does not start ([`order()`]).
pub fn pick(
    look: &Look,
    running: &BTreeSet<String>,
    starts: &BTreeMap<String, u32>,
) -> Option<Work> {
    if look.paused || look.no_lead {
        return None;
    }
    let busy = |item: &str| running.contains(item) || look.claims.contains(item);
    let waits = order(look, running, starts);
    look.free.iter().find_map(|item| {
        if look.holds.contains(item) || busy(item) {
            return None;
        }
        let work = if let Some(issue) = verify_issue(item) {
            // The author of the item still works on it, or the verify
            // waits for the merge of another pull request.
            if busy(&format!("issue-{issue}")) || waits.contains_key(&issue) {
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
            // A verify of the item tests the head that a fix replaces.
            if fix_of(look, item) && busy(&format!("verify-issue-{issue}")) {
                return None;
            }
            match (
                look.fixes.conflicts.get(&issue),
                look.fixes.failed.get(&issue),
            ) {
                // The rebase waits for its turn in the order (#1801).
                (Some(_), _) if waits.contains_key(&issue) => return None,
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

/// The verifies and the rebases of `look` that wait for another pull
/// request in their group ([`order::Lane::clashes`]), by issue, with the
/// number of that pull request ([`order::waits`],
/// 01M6WCGNCC3HWS47158WTWNQ8B, #1801, #1895). A pull request goes first
/// while a session claims its verify, or a job of the wave runs it
/// (`running`), and while its verify passed and nothing stops its merge.
/// A session of its item does not make it go first
/// (01M74EP20TDQEXW42MK0XE73QF, #1894). Then come the pull requests whose
/// next step the pick can start, in the order of the requests of their
/// items: the rebase of a conflict, or a verify. Each is in the free
/// work, not held, and has starts left (`starts`). A verify whose full
/// check is not done yet takes its place too: it starts when the check is
/// done. So does a pull request that reads `UNKNOWN` and whose verify is
/// not held, and one with a conflict while a session holds its item, for
/// example to rebase it. Each other pull request whose item a session
/// holds has no place: its verify cannot start, so no verify waits for
/// it.
///
/// ```
/// use std::collections::{BTreeMap, BTreeSet};
/// use riff::order::Lane;
/// use riff::wave::{Look, Waiting, Work, order, pick};
///
/// let lane = |issue, file: &str, asked| Lane {
///     issue,
///     pull: 40 + issue,
///     files: [file.to_owned()].into(),
///     asked: Some(asked),
///     ..Lane::default()
/// };
/// let waiting = |issue: u64| Waiting { pull: 40 + issue, commit: format!("c{issue}"), gate_done: true, again: false };
/// let look = Look {
///     free: vec!["verify-issue-1".into(), "verify-issue-2".into(), "verify-issue-3".into()],
///     pulls: [(1, waiting(1)), (2, waiting(2)), (3, waiting(3))].into(),
///     lanes: [(1, lane(1, "tui.rs", 2)), (2, lane(2, "tui.rs", 1)), (3, lane(3, "wave.rs", 3))].into(),
///     ..Look::default()
/// };
/// let none = BTreeSet::new();
/// // The request of 2 came first: 1 waits for #42, and 3 goes beside it.
/// assert_eq!(order(&look, &none, &BTreeMap::new()), BTreeMap::from([(1, 42)]));
/// assert_eq!(pick(&look, &none, &BTreeMap::new()), Some(Work::Verify { issue: 2, pull: 42, commit: "c2".into() }));
/// let running: BTreeSet<String> = ["verify-issue-2".to_owned()].into();
/// assert_eq!(pick(&look, &running, &BTreeMap::new()), Some(Work::Verify { issue: 3, pull: 43, commit: "c3".into() }));
/// // The lead holds the verify of 2: 1 waits for nobody.
/// let held = Look { holds: ["verify-issue-2".to_owned()].into(), ..look.clone() };
/// assert!(order(&held, &none, &BTreeMap::new()).is_empty());
/// // #1895: the heads of 1 and 2 merge with no conflict. Both verifies go.
/// let mut clean = look.clone();
/// clean.lanes.get_mut(&1).unwrap().clean = [42].into();
/// assert!(order(&clean, &running, &BTreeMap::new()).is_empty());
/// assert_eq!(pick(&clean, &running, &BTreeMap::new()), Some(Work::Verify { issue: 1, pull: 41, commit: "c1".into() }));
/// // #1894: a session holds issue-2, for example to fix its checks. 2 does
/// // not go first, and its verify cannot start: the verify of 1 starts.
/// let claimed = Look { claims: ["issue-2".to_owned()].into(), ..look.clone() };
/// assert!(order(&claimed, &none, &BTreeMap::new()).is_empty());
/// assert_eq!(pick(&claimed, &none, &BTreeMap::new()), Some(Work::Verify { issue: 1, pull: 41, commit: "c1".into() }));
/// // The verify of 2 passed: 1 waits for its merge.
/// let passed = Look {
///     free: vec!["verify-issue-1".into()],
///     lanes: [(1, lane(1, "tui.rs", 2)), (2, Lane { passed: true, ..lane(2, "tui.rs", 1) })].into(),
///     ..look
/// };
/// assert_eq!(pick(&passed, &none, &BTreeMap::new()), None);
///
/// // #1801: the merge of 2 gave 4 and 5 a conflict. Only the rebase of 4 starts.
/// use riff::wave::{Conflict, Fixes};
/// let conflict = |issue| (issue, Conflict { pull: 40 + issue, commit: format!("c{issue}") });
/// let rebases = Look {
///     free: vec!["issue-4".into(), "issue-5".into()],
///     lanes: [4, 5].map(|n| (n, Lane { conflict: true, ..lane(n, "tui.rs", n as usize) })).into(),
///     fixes: Fixes { conflicts: [conflict(4), conflict(5)].into(), ..Fixes::default() },
///     ..Look::default()
/// };
/// assert_eq!(order(&rebases, &none, &BTreeMap::new()), BTreeMap::from([(5, 44)]));
/// let rebase = Work::Rebase { issue: 4, pull: 44, commit: "c4".into() };
/// assert_eq!(pick(&rebases, &none, &BTreeMap::new()), Some(rebase.clone()));
/// // A job rebases 4: it keeps its place, and 5 still waits.
/// let rebasing: BTreeSet<String> = ["issue-4".to_owned()].into();
/// let started: BTreeMap<String, u32> = [(rebase.key(), 1)].into();
/// assert_eq!(order(&rebases, &rebasing, &started), BTreeMap::from([(5, 44)]));
/// assert_eq!(pick(&rebases, &rebasing, &started), None);
/// // 4 pushed, and reads UNKNOWN: it keeps its place.
/// let pushed = Look {
///     free: vec!["issue-5".into()],
///     lanes: [(4, Lane { unknown: true, ..lane(4, "tui.rs", 4) }), (5, rebases.lanes[&5].clone())].into(),
///     fixes: Fixes { conflicts: [conflict(5)].into(), ..Fixes::default() },
///     ..Look::default()
/// };
/// assert_eq!(order(&pushed, &none, &BTreeMap::new()), BTreeMap::from([(5, 44)]));
/// assert_eq!(pick(&pushed, &none, &BTreeMap::new()), None);
/// ```
pub fn order(
    look: &Look,
    running: &BTreeSet<String>,
    starts: &BTreeMap<String, u32>,
) -> BTreeMap<u64, u64> {
    let busy = |item: &str| running.contains(item) || look.claims.contains(item);
    let left = |work: Work| starts.get(&work.key()).copied().unwrap_or(0) < work.most();
    // Its verify runs, or it waits for its merge. A session of its item
    // does not make it go first (#1894).
    let goes = |lane: &order::Lane| lane.passed || busy(&format!("verify-issue-{}", lane.issue));
    // The pick can start its next step: the rebase of its conflict, or its
    // verify. A rebase that runs keeps its place, and so does one that
    // reads `UNKNOWN`, for the next look.
    let next = |lane: &order::Lane| {
        let issue = lane.issue;
        let item = format!("issue-{issue}");
        if lane.conflict {
            return look.free.contains(&item)
                && !look.holds.contains(&item)
                && (busy(&item)
                    || look.fixes.conflicts.get(&issue).is_some_and(|c| {
                        left(Work::Rebase {
                            issue,
                            pull: c.pull,
                            commit: c.commit.clone(),
                        })
                    }));
        }
        // No verify starts while a session holds the item.
        let verify = format!("verify-issue-{issue}");
        let free = !busy(&item)
            && look.free.contains(&verify)
            && look.pulls.get(&issue).is_some_and(|waiting| {
                left(Work::Verify {
                    issue,
                    pull: waiting.pull,
                    commit: waiting.commit.clone(),
                })
            });
        !look.holds.contains(&verify) && (lane.unknown || free)
    };
    let first = look.lanes.values().filter(|lane| goes(lane));
    let asked = look.lanes.values().filter(|lane| !goes(lane) && next(lane));
    order::waits(first, asked)
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

/// An item of the free work that reached [`STARTS`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct Spent {
    /// The item.
    item: String,
    /// The name of its starts ([`Work::key`]).
    key: String,
    /// The head commit of its pull request, for a verify: the starts of a
    /// verify are for a head (#1651).
    head: Option<String>,
}

/// The items of the free work of `look` that reached [`STARTS`]. A fix
/// of a pull request is not one: it starts one time for each head. The
/// verify of a head counts for that head, so a verify is spent only for
/// the head that the pull request has now. An item that runs is not spent
/// yet: the last session has not ended, so the reason of its end is not
/// known, and the pull request may get a new head. A pull request that
/// waits for no verify has no head, and no spent verify.
fn spent(look: &Look, running: &BTreeSet<String>, starts: &BTreeMap<String, u32>) -> Vec<Spent> {
    look.free
        .iter()
        .filter(|item| !fix_of(look, item) && !running.contains(*item))
        .filter_map(|item| {
            let (key, head) = match verify_issue(item) {
                Some(issue) => {
                    let waiting = look.pulls.get(&issue)?;
                    let verify = Work::Verify {
                        issue,
                        pull: waiting.pull,
                        commit: waiting.commit.clone(),
                    };
                    (verify.key(), Some(waiting.commit.clone()))
                }
                None => (item.clone(), None),
            };
            let n = starts.get(&key).copied().unwrap_or(0);
            (n >= STARTS).then(|| Spent {
                item: item.clone(),
                key,
                head,
            })
        })
        .collect()
}

/// The verify of each head that waits for a verify in `look`: the
/// `verify-issue-N` of its free work with the pull request of `N`, by
/// issue, as [`Work::key`], with [`Waiting::again`].
fn waiting_keys(look: &Look) -> BTreeMap<u64, (String, bool)> {
    look.free
        .iter()
        .filter_map(|item| {
            let issue = verify_issue(item)?;
            let waiting = look.pulls.get(&issue)?;
            let verify = Work::Verify {
                issue,
                pull: waiting.pull,
                commit: waiting.commit.clone(),
            };
            Some((issue, (verify.key(), waiting.again)))
        })
        .collect()
}

/// What the daemon knows of the verifies of the heads of one repository
/// (01M75R717PH1NCQJ338ASVYT63, 01M77K0EGH1HF0WA7WZH8BP2BP): see "A look
/// that does not see the verdict yet".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Verdicts {
    /// The verdict of each head, as the [`Work::key`] of its verify: the
    /// daemon put it on the pull request ([`Finish::verdict`]), or the read
    /// before a claim found it ([`World::verdict`]).
    pub heads: BTreeMap<String, Verdict>,
    /// The verifies that the last look showed with a new verify request
    /// after their result ([`Waiting::again`]), as [`Work::key`].
    pub again: BTreeSet<String>,
}

impl Verdicts {
    /// True when the verify `key` of a head with `verdict` starts no
    /// session: a head gets one verify that passes, and a head whose
    /// verify failed gets a new verify only for a new verify request after
    /// the FAIL ([`Verdicts::again`]).
    ///
    /// ```
    /// use riff::pr::Verdict;
    /// use riff::wave::Verdicts;
    ///
    /// let mut verdicts = Verdicts::default();
    /// assert!(verdicts.stops("verify-issue-1@c1", Verdict::Pass));
    /// assert!(verdicts.stops("verify-issue-1@c1", Verdict::Fail));
    /// verdicts.again.insert("verify-issue-1@c1".into());
    /// assert!(verdicts.stops("verify-issue-1@c1", Verdict::Pass));
    /// assert!(!verdicts.stops("verify-issue-1@c1", Verdict::Fail));
    /// ```
    pub fn stops(&self, key: &str, verdict: Verdict) -> bool {
        stops(&self.again, key, verdict)
    }

    /// True when it keeps no verdict and no new verify request.
    pub fn is_empty(&self) -> bool {
        self.heads.is_empty() && self.again.is_empty()
    }
}

/// [`Verdicts::stops`] with the new verify requests `again`.
fn stops(again: &BTreeSet<String>, key: &str, verdict: Verdict) -> bool {
    verdict == Verdict::Pass || !again.contains(key)
}

/// Takes out of `look` each verify of a head whose verdict in `verdicts`
/// stops it ([`Verdicts::stops`]). Its item leaves the free work. After a
/// PASS, its pull request goes first in the order of the verifies
/// ([`order()`]), as a pass does. After a FAIL, it leaves the order, as a
/// fail does. The look of the wave can show such a head as waiting for a
/// short time still: the event of the status comes after the end of the
/// verify ([`hooks::Told`]).
///
/// The verifies that the look shows with a new verify request after
/// their result go into [`Verdicts::again`]. A head that the look does not
/// show as waiting leaves [`Verdicts::heads`]: the look sees the status
/// now. A head whose item the look shows as claimed or held stays: such a
/// look cannot show the head as waiting, for example a look that read the
/// claims while the verify ran. A FAIL with a new verify request leaves
/// too: the lead asks for a new verify.
///
/// ```
/// use riff::pr::Verdict;
/// use riff::wave::{Look, Verdicts, Waiting, drop_verified, pick};
///
/// let waiting = |n: u64, commit: &str| (n, Waiting { pull: 40 + n, commit: commit.into(), gate_done: true, again: false });
/// let look = |waits: Vec<(u64, Waiting)>| Look {
///     free: waits.iter().map(|(n, _)| format!("verify-issue-{n}")).collect(),
///     pulls: waits.into_iter().collect(),
///     ..Look::default()
/// };
/// let mut verdicts = Verdicts::default();
/// verdicts.heads.insert("verify-issue-1@c1".into(), Verdict::Pass);
/// verdicts.heads.insert("verify-issue-2@c2".into(), Verdict::Fail);
/// verdicts.heads.insert("verify-issue-4@c4".into(), Verdict::Fail);
/// verdicts.heads.insert("verify-issue-5@c5".into(), Verdict::Fail);
/// let mut now = look(vec![waiting(1, "c1"), waiting(2, "c2"), waiting(3, "c3")]);
/// // The verifier of 5 still holds its claim.
/// now.claims.insert("verify-issue-5".into());
/// drop_verified(&mut now, &mut verdicts);
/// assert_eq!(now.free, ["verify-issue-3"]);
/// assert_eq!(now.pulls.keys().collect::<Vec<_>>(), [&3]);
/// // The look shows no wait for the head of 4: it sees the status now.
/// assert!(!verdicts.heads.contains_key("verify-issue-4@c4"));
/// assert!(verdicts.heads.contains_key("verify-issue-5@c5"));
/// // The lead asks for a new verify of c2 after its FAIL.
/// let (_, asked) = waiting(2, "c2");
/// let mut now = look(vec![waiting(1, "c1"), (2, Waiting { again: true, ..asked })]);
/// drop_verified(&mut now, &mut verdicts);
/// assert_eq!(now.free, ["verify-issue-2"]);
/// assert!(!verdicts.stops("verify-issue-2@c2", Verdict::Fail));
/// // A new head of 1 gets its verify.
/// let mut now = look(vec![waiting(1, "c1b")]);
/// drop_verified(&mut now, &mut verdicts);
/// assert!(pick(&now, &Default::default(), &Default::default()).is_some());
/// assert!(verdicts.is_empty());
/// ```
pub fn drop_verified(look: &mut Look, verdicts: &mut Verdicts) {
    let waits = waiting_keys(look);
    let Verdicts { heads, again } = verdicts;
    *again = waits
        .values()
        .filter(|(_, again)| *again)
        .map(|(key, _)| key.clone())
        .collect();
    // The item of a key: `verify-issue-N` of `verify-issue-N@COMMIT`.
    let busy = |key: &str| {
        let item = key.split_once('@').map_or(key, |(item, _)| item);
        look.claims.contains(item) || look.holds.contains(item)
    };
    heads.retain(|key, verdict| {
        let keep = waits.values().any(|(k, _)| k == key) || busy(key);
        keep && stops(again, key, *verdict)
    });
    for (issue, (key, _)) in waits {
        let Some(verdict) = heads.get(&key) else {
            continue;
        };
        let item = format!("verify-issue-{issue}");
        look.free.retain(|free| *free != item);
        look.pulls.remove(&issue);
        match verdict {
            Verdict::Pass => {
                if let Some(lane) = look.lanes.get_mut(&issue) {
                    lane.passed = true;
                }
            }
            Verdict::Fail => {
                look.lanes.remove(&issue);
            }
        }
    }
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
    /// The facts of the wave now. `told` holds what the wakes of the GitHub
    /// App named since the last look ([`hooks::Told`], #1656): while they
    /// come, the look reads only that.
    fn look<'a>(&'a self, wave: &'a Wave, told: Arc<hooks::Told>) -> BoxFuture<'a, Result<Look>>;

    /// Each project of the riff of the wave now
    /// (01M76SG8JVHTKY6N62FQG7XAE6). The loop reads them before each look:
    /// a wave runs only for a project that allows its host
    /// ([`crate::project::wave_refused`], #1863).
    fn projects<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Vec<ProjectFacts>>>;

    /// The session image of the repository of the wave, with its env and
    /// volumes: the merged pin of its main clone
    /// ([`session_image::merged_config`]).
    fn pinned<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Pinned>>;

    /// The facts of the host that no item changes (as `riff run` checks them):
    /// the fault of the first that fails (01M57A7A1W2K9440WZA1QZ3M9T).
    fn host<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<(), run::HostFault>>;

    /// The verdict of the status `riff/verify` of the head commit of the
    /// verify `work` of `wave` on the forge now ([`pr::verify_state`],
    /// [`Verdict::of_state`]). The job reads it before the claim
    /// (01M75R717PH1NCQJ338ASVYT63, 01M77K0EGH1HF0WA7WZH8BP2BP). A fault of
    /// the read is no verdict: the verify starts. The default is no
    /// verdict.
    fn verdict<'a>(&'a self, _wave: &'a Wave, _work: &'a Work) -> BoxFuture<'a, Option<Verdict>> {
        Box::pin(async { None })
    }

    /// True when the forge says now that the pull request `pull` of `wave`
    /// is open, out of the merge queue, at the head commit `head`, and with
    /// auto-merge off ([`rollout::merge_off`], 01M777N148MY8NR5RNDKXE9EVJ).
    /// The loop asks it before it tells the lead of
    /// [`rollout::Stuck::AutoMergeOff`]. A fault of the read is false: the
    /// next look asks again. The default is true.
    fn merge_off<'a>(&'a self, _wave: &'a Wave, _pull: u64, _head: &'a str) -> BoxFuture<'a, bool> {
        Box::pin(async { true })
    }

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
    /// line for each step, and tells if the end reported a verdict.
    fn finish<'a>(&'a self, record: &'a Record, ended: Ended) -> BoxFuture<'a, Result<Finish>>;

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
    /// days with `gh`, and gives their names
    /// ([`crate::daemon::sweep::delete_old_kept_on`]). `gh` writes as the
    /// daemon ([`gh_token::writer`]).
    fn delete_old_kept<'a>(
        &'a self,
        _gh: pr::Gh,
        _repo: &'a str,
    ) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// Adds the comment `Merged in #PULL (COMMIT)` to the issue `issue` of
    /// `repo` with `gh`, when it has none ([`crate::daemon::sweep`]). `gh`
    /// writes as the daemon ([`gh_token::writer`]).
    fn merged_in<'a>(
        &'a self,
        gh: pr::Gh,
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

    /// The items of the repository of `wave` that the lead holds now. The
    /// default holds none.
    fn holds<'a>(&'a self, _wave: &'a Wave) -> BoxFuture<'a, Result<HashSet<String>>> {
        Box::pin(async { Ok(HashSet::new()) })
    }

    /// Tells `changed` at each change of the free work of the repository of
    /// `wave` on riff-server (#1729): a hold, a free, a release, a plan, a
    /// new item of a wave or a merge on GitHub (#1655). It never ends. The
    /// default tells nothing.
    fn plan_changes<'a>(&'a self, _wave: &'a Wave, _changed: &'a Notify) -> BoxFuture<'a, ()> {
        Box::pin(std::future::pending())
    }

    /// Follows the wakes of the GitHub App of the repository of `wave` on
    /// riff-server into `told` ([`hooks::follow`], #1656). It never ends.
    /// The default follows none: the events stay off.
    fn github_wakes<'a>(&'a self, _wave: &'a Wave, _told: &'a hooks::Told) -> BoxFuture<'a, ()> {
        Box::pin(std::future::pending())
    }

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

    /// Tells riff-server the fact `fact` of the daemon, as the person of the
    /// lead of `record`: riff-server writes the line of riff-bot for it in
    /// riff chat, and the line wakes no session: the lead cannot act on it
    /// (01M5NJ87YEHHGJBANM7CRYV56Z).
    fn bot<'a>(&'a self, record: &'a Record, fact: BotFact) -> BoxFuture<'a, ()>;

    /// Posts `body` as a message to the lead of `record`, as the person
    /// of the lead: the lead wakes and acts on it. The default posts
    /// nothing (01M5SNV7H3739PXW73H14XM4B8).
    fn note_lead<'a>(&'a self, _record: &'a Record, _body: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    /// Ends the session of `record` on riff-server, and stops its
    /// keep-alive: the end of the lead.
    fn leave<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, ()>;

    /// True when the daemon runs the lead loop of each wave
    /// ([`crate::lead_host`], #1791): the daemon of a host claims the lead
    /// of the project. The default runs none.
    fn leads(&self) -> bool {
        false
    }

    /// The session that holds the claim lead of the project of the lead
    /// of `record` on riff-server now ([`riff_core::wire::PlanReply::lead`],
    /// #1791). `None` when the claim is free. The default holds none.
    fn lead_holder<'a>(&'a self, _record: &'a Record) -> BoxFuture<'a, Result<Option<SessionUri>>> {
        Box::pin(async { Ok(None) })
    }

    /// Gives the claim lead of the project of the lead of `record` to that
    /// lead, as the person, only when it is free (#1791). False when
    /// another session holds it. The default gives none.
    fn lead_claim<'a>(&'a self, _record: &'a Record) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async { Ok(false) })
    }

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

    /// The token on GitHub of the session `me` of the riff-server
    /// `server`, asked as the person (01M6WPMEEWX09DHJZF18BKBPYW,
    /// [`crate::daemon::gh_token`]): a token of a GitHub App, or no token
    /// and why, never the token of the person (01M6Z87A04EXTT1FFJSJ1EM6TB).
    /// An error is a fault: the daemon starts no container with it. The
    /// default gives no token: a riff with no App.
    fn github_token<'a>(
        &'a self,
        _server: &'a str,
        _me: &'a SessionUri,
    ) -> BoxFuture<'a, Result<gh_token::Grant>> {
        Box::pin(async { Ok(gh_token::Grant::none(text::GH_TOKEN_NONE)) })
    }

    /// Revokes the token `token` of a session of the repository `repo` on
    /// GitHub, at the end of the session. The default revokes nothing.
    fn revoke_token<'a>(&'a self, _repo: &'a str, _token: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async { Ok(()) })
    }

    /// How the daemon writes on GitHub ([`gh_token::Writes`],
    /// 01M6XZJFE2ZTR5RE3AWM30R704). `None`: each write has the token of
    /// the person. The default has none.
    fn writes(&self) -> Option<&gh_token::Writes> {
        None
    }

    /// A token of the lead App of the riff-server `server` for the writes
    /// of the daemon on `repo`, asked as the person
    /// (01M6XZJFGNVG6B7DF5VQ3H1DNX), or no token and why: then the daemon
    /// writes with the token of the person. An error is a fault: the daemon
    /// writes with the token of the person then. The default gives no
    /// token.
    fn daemon_token<'a>(
        &'a self,
        _server: &'a str,
        _repo: &'a str,
    ) -> BoxFuture<'a, Result<gh_token::Grant>> {
        Box::pin(async { Ok(gh_token::Grant::none(text::GH_TOKEN_NONE)) })
    }

    /// Sends to `to` each install of a GitHub App with new rights on the
    /// riff-server `server` ([`Api::github_renew`], 01M74MB99RT6B9MKSBMSVD4JRG), as the
    /// person at `place`. It never ends. The default sends none.
    fn github_renews<'a>(
        &'a self,
        _server: &'a str,
        _place: &'a Place,
        _to: mpsc::UnboundedSender<GitHubRenew>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(std::future::pending())
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
/// ([`crate::ghcalls`]). Each call has the time limit
/// [`sweep::GH_LIMIT`], as the look has, so a `gh` that hangs stops no
/// wave (01M77AB80MEFD9K8Y7FPDATY3D).
fn forge(repo: &str) -> pr::Gh {
    sweep::gh(repo).by("wave")
}

/// How long the wave waits after the look `look` (#1549). The wave is idle
/// when the look has the same facts as the `last` look, the pick found no
/// work (`picked` is false), and no session or job of the wave runs
/// (`running` is false): it waits [`IDLE_LOOK_EVERY`], or the time of the
/// safety look ([`hooks::SAFETY_LOOK_EVERY`], #1656) while the events of
/// GitHub are on (`events`). Else it waits [`LOOK_EVERY`]. A paused wave,
/// and a wave that waits for a lead, call no `gh`, and start at once on a
/// resume or a new lead, so they are never idle.
/// An end of a session or of a job, a change of the plan on riff-server
/// ([`World::plan_changes`]) and a wake of the GitHub App
/// ([`World::github_wakes`]) wake the wave in each case.
///
/// ```
/// use riff::hooks::SAFETY_LOOK_EVERY;
/// use riff::wave::{IDLE_LOOK_EVERY, LOOK_EVERY, Look, look_wait};
///
/// let look = Look { free: vec!["issue-12".into()], ..Look::default() };
/// // The first look, and a look that changed, wait one minute.
/// assert_eq!(look_wait(&look, None, false, false, false), LOOK_EVERY);
/// let other = Look { free: vec!["issue-13".into()], ..Look::default() };
/// assert_eq!(look_wait(&other, Some(&look), false, false, false), LOOK_EVERY);
/// // Nothing changed, no work, nothing runs: five minutes.
/// assert_eq!(look_wait(&look, Some(&look), false, false, false), IDLE_LOOK_EVERY);
/// // With the events of GitHub: the safety look, each 30 minutes.
/// assert_eq!(look_wait(&look, Some(&look), false, false, true), SAFETY_LOOK_EVERY);
/// assert_eq!(look_wait(&look, None, false, false, true), LOOK_EVERY);
/// // Work, or a session that runs, keeps the minute.
/// assert_eq!(look_wait(&look, Some(&look), true, false, true), LOOK_EVERY);
/// assert_eq!(look_wait(&look, Some(&look), false, true, false), LOOK_EVERY);
/// // A paused wave looks each minute.
/// let paused = Look { paused: true, ..Look::default() };
/// assert_eq!(look_wait(&paused, Some(&paused), false, false, true), LOOK_EVERY);
/// let no_lead = Look { no_lead: true, ..Look::default() };
/// assert_eq!(look_wait(&no_lead, Some(&no_lead), false, false, true), LOOK_EVERY);
/// ```
pub fn look_wait(
    look: &Look,
    last: Option<&Look>,
    picked: bool,
    running: bool,
    events: bool,
) -> Duration {
    match (
        !picked && !running && !look.paused && !look.no_lead && last == Some(look),
        events,
    ) {
        (true, true) => hooks::SAFETY_LOOK_EVERY,
        (true, false) => IDLE_LOOK_EVERY,
        (false, _) => LOOK_EVERY,
    }
}

/// Follows the changes of the free work of `thread` on riff-server as `me`
/// ([`Api::plan_watch`], #1729), and tells `changed` at each one. It
/// connects again after each end of the stream, and never ends. It prints
/// a fault of the stream one time until the fault changes.
pub async fn follow_plan(api: &Api, me: &SessionUri, thread: &ThreadName, changed: &Notify) {
    use futures::StreamExt;
    let changes = api::follow(|| api.plan_watch(me, thread), STREAM_RETRY);
    tokio::pin!(changes);
    let mut fault = None;
    while let Some(change) = changes.next().await {
        match change {
            Ok(_) => {
                fault = None;
                changed.notify_one();
            }
            Err(e) => {
                let e = format!("{e:#}");
                if fault.as_ref() != Some(&e) {
                    eprintln!("riff daemon: {thread}: {}", text::wave_no_plan(&e));
                }
                fault = Some(e);
            }
        }
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
/// and a new wait in the same outage has the same key (#1581, #1872).
/// `None` when no limit is out.
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

/// Adds the comment `Merged in #PULL (COMMIT)` to each issue of
/// `look.merged` ([`World::merged_in`], #1754) one time: `said` holds each
/// issue and pull request that got it from the loop, while the look still
/// finds it with no comment. A comment that fails comes at the next look.
/// The comment goes as the daemon ([`gh_token::writer`],
/// 01M6XZJFE2ZTR5RE3AWM30R704).
async fn say_merged(world: &dyn World, wave: &Wave, look: &Look, said: &mut BTreeSet<(u64, u64)>) {
    said.retain(|(issue, pull)| look.merged.get(issue).is_some_and(|m| m.pull == *pull));
    let mut writer = None;
    for (issue, merge) in &look.merged {
        if said.contains(&(*issue, merge.pull)) {
            continue;
        }
        let gh = match &writer {
            Some(gh) => pr::Gh::clone(gh),
            None => {
                let gh = pr::Gh::clone(&world.gh(&wave.repo)).by("wave");
                let gh = gh_token::writer(world, Some(&wave.server), &wave.repo, gh).await;
                writer.insert(gh).clone()
            }
        };
        match world
            .merged_in(gh, &wave.repo, *issue, merge.pull, &merge.commit)
            .await
        {
            Ok(()) => {
                said.insert((*issue, merge.pull));
            }
            Err(e) => {
                let e = format!("{e:#}");
                let line = text::wave_no_merged_in(*issue, merge.pull, &e);
                eprintln!("riff daemon: {}: {line}", wave.repo);
            }
        }
    }
}

/// The loop of the wave `wave` in `daemon`, until the daemon stops it.
/// `taken` holds the items of its jobs before the start, `starts` the
/// starts of each item, `word` what it did last, and `ordered` the verifies
/// that wait for another pull request at its last look ([`order()`]).
pub(crate) async fn run<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    wave: Wave,
    taken: Arc<Mutex<BTreeSet<String>>>,
    starts: Starts,
    word: Arc<Mutex<String>>,
    ordered: Waits,
) {
    let say = |text: String| *lock(&word) = text;
    // The starts of the spent items that the lead heard of ([`Work::key`]).
    let mut told: BTreeSet<String> = BTreeSet::new();
    // The stuck pull requests that the lead heard of ([`Notice::key`]).
    let mut noticed: BTreeSet<String> = BTreeSet::new();
    // The issues with no `Done when:` line that the lead heard of.
    let mut bare: BTreeSet<u64> = BTreeSet::new();
    // The issues and the merged pull requests whose comment `Merged in`
    // this loop added.
    let mut said: BTreeSet<(u64, u64)> = BTreeSet::new();
    // True while the wave waits for a fact of the host: the lead heard
    // of it.
    let mut waits = false;
    // The facts of the last look, to find an idle wave.
    let mut last: Option<Look> = None;
    // The key of the limit of GitHub that the lead heard of.
    let mut limited: Option<String> = None;
    // A change of the plan on riff-server: the wave looks at once (#1729).
    let plan = Arc::new(Notify::new());
    let _follow = {
        let (world, wave, plan) = (daemon.world(), wave.clone(), plan.clone());
        Abort(tokio::spawn(async move {
            world.plan_changes(&wave, &plan).await;
        }))
    };
    // New rights of an install: the writes of the wave ask for a new
    // token (01M74MB99RT6B9MKSBMSVD4JRG).
    if let Ok(place) = wave.place(None) {
        daemon.follow_renews(&wave.server, &place);
    }
    // The wakes of the GitHub App: the look reads only what they name
    // (#1656).
    let events = daemon.told(&wave.repo);
    let _wakes = {
        let (world, wave, events) = (daemon.world(), wave.clone(), events.clone());
        Abort(tokio::spawn(async move {
            world.github_wakes(&wave, &events).await;
        }))
    };
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
        // This look sees each change of the plan and each wake of GitHub
        // before it.
        let _ = futures::FutureExt::now_or_never(plan.notified());
        events.looks();
        hooks::log_dropped(&events);
        let looked = tokio::time::Instant::now();
        let mut wait = LOOK_EVERY;
        // A wave only for a project that allows the host: else the look
        // reads nothing, and the word says why (#1863).
        let seen = match world.projects(&wave).await {
            Ok(projects) => match project::wave_refused(&projects, &wave.repo, &wave.host) {
                Some(refused) => Ok(Err(refused)),
                None => world.look(&wave, events.clone()).await.map(Ok),
            },
            Err(e) => Err(e),
        };
        let next = match seen {
            Ok(Err(refused)) => {
                say(refused.word());
                last = None;
                None
            }
            Ok(Ok(mut look)) => {
                // A head whose verify passed gets no new verify (#1960),
                // and one whose verify failed only when the lead asks
                // (#2019).
                daemon.drop_verified(&wave.repo, &mut look);
                say_merged(&*world, &wave, &look, &mut said).await;
                let spent = spent(&look, &running(&daemon, &wave, &taken), &lock(&starts));
                // `riff host join` cleared the starts of these items.
                told.retain(|key| lock(&starts).get(key).is_some_and(|n| *n >= STARTS));
                for Spent { item, key, head } in spent {
                    if told.insert(key) {
                        let reason = daemon
                            .last_end(&item)
                            .unwrap_or_else(|| text::WAVE_END_UNKNOWN.to_owned());
                        let body = text::wave_spent(&item, head.as_deref(), STARTS, &reason);
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
                    if noticed.contains(&notice.key()) {
                        continue;
                    }
                    // A merge through the queue between two reads of the
                    // look looks like auto-merge off: the forge tells it
                    // again first (01M777N148MY8NR5RNDKXE9EVJ).
                    if notice.stuck == rollout::Stuck::AutoMergeOff
                        && !world.merge_off(&wave, notice.pull, &notice.commit).await
                    {
                        continue;
                    }
                    noticed.insert(notice.key());
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
                let next = pick(&look, &running(&daemon, &wave, &taken), &lock(&starts));
                *lock(&ordered) = order(&look, &running(&daemon, &wave, &taken), &lock(&starts));
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
                // A worker got no token of a GitHub App: the wave waits,
                // and no start counts (01M6Z87A04EXTT1FFJSJ1EM6TB).
                let no_token = fault.is_none()
                    && next.is_some()
                    && daemon.no_start_waits(&wave.repo).is_some();
                say(match (&next, &fault) {
                    _ if look.paused => text::WAVE_PAUSED.into(),
                    _ if look.no_lead => text::wave_no_lead(&wave.repo),
                    (Some(_), None) if no_token => text::WAVE_NO_TOKEN.into(),
                    (Some(_), Some(fault)) => text::wave_host_word(&fault.wave_text()),
                    (Some(work), None) => text::wave_started(&work.item()),
                    (None, _) => text::WAVE_NO_WORK.into(),
                });
                wait = look_wait(
                    &look,
                    last.as_ref(),
                    next.is_some(),
                    !running(&daemon, &wave, &taken).is_empty(),
                    events.on(crate::daemon::now_ms()),
                );
                last = Some(look);
                next.filter(|_| fault.is_none() && !pulling && !no_token)
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
                    () = plan.notified() => {}
                    () = events.woken(looked) => {}
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
        watch.waits = daemon.order_waits(&wave.repo);
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
        // While the events of GitHub are on, the loop of the wave keeps
        // the replies new with the reads of the wakes, and reads in full at
        // its safety look (#1656).
        let told = daemon.told(&wave.repo);
        watch.accepts = match told.on(crate::daemon::now_ms()) {
            true => (wait + every).max(told.safety() + every),
            false => wait + every,
        };
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
/// its jobs. `riff host join` on a wave that runs clears them
/// (01M57A7A48A70Y7PTD95A3MBYC).
pub(crate) type Starts = Arc<Mutex<BTreeMap<String, u32>>>;

/// The verifies and the rebases of a wave that wait for another pull
/// request, by issue, with the number of that pull request ([`order()`]).
/// Its loop writes them at each look. Its look and the status of the
/// daemon read them.
pub(crate) type Waits = Arc<Mutex<BTreeMap<u64, u64>>>;

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
    let no_start = match start_job(&daemon, &wave, &work, slot, asked).await {
        Ok(Some((record, ended))) => {
            // A worker or a verify that got no token of a GitHub App did not
            // start: no start counts, and the lead hears why one time
            // (01M6Z87A04EXTT1FFJSJ1EM6TB).
            let no_token = match &record.github {
                Some(gh_token::ActsAs::NoToken { why }) if !record.terminal => Some(why.clone()),
                _ => None,
            };
            // A verify with a PASS or FAIL on the pull request is no failed
            // start (#1651). The item is still taken: no start comes
            // between the end and the take back.
            if finish(daemon.clone(), record, ended, held).await.reported() || no_token.is_some() {
                take_back(&starts, &work);
            }
            if let Some(why) = daemon.no_start_untold(&wave.repo).filter(|_| !asked) {
                let body = text::wave_no_token(&wave.repo, &why);
                daemon.world().tell(&wave, &body, None).await;
            }
            no_token
        }
        Ok(None) => {
            println!("{}", text::wave_not_claimed(&wave.repo, &item));
            Some(text::RUN_NOT_CLAIMED.to_owned())
        }
        Err(ref e) if e.downcast_ref::<Taken>().is_some() => {
            println!("{}", text::wave_taken(&wave.repo, &item));
            take_back(&starts, &work);
            Some(text::RUN_NOT_CLAIMED.to_owned())
        }
        Err(ref e) if let Some(Verified(line)) = e.downcast_ref::<Verified>() => {
            println!("{line}");
            take_back(&starts, &work);
            Some(line.clone())
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

/// The item of a job is no free work after the wait before its claim
/// (#1729): another session claims it, or the lead holds it. The start
/// does not count, and the lead hears nothing.
#[derive(Debug)]
pub(crate) struct Taken;

impl std::fmt::Display for Taken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(text::RUN_NOT_CLAIMED)
    }
}

impl std::error::Error for Taken {}

/// The head of a verify has the status `riff/verify` success at the read
/// before the claim ([`World::verdict`], #1960), or a failure with no new
/// verify request after it (#2019), with the line for the log
/// ([`text::wave_verify_done`]). No session starts, the start does not
/// count, and the lead hears nothing. The loop starts no verify of that
/// head again ([`drop_verified`]).
#[derive(Debug)]
pub(crate) struct Verified(String);

impl std::fmt::Display for Verified {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Verified {}

/// The wait before the claim of `work` (#1729): the host with the most
/// free room claims first. The host waits [`claim_wait`] for its live
/// sessions and its limit now: an empty host does not wait. After a
/// wait, it reads the claims and the holds of riff-server again, and the
/// error is [`Taken`] when the item is no free work now. A read that
/// fails leaves the choice to the claim: riff-server gives each item to
/// one session.
async fn wait_to_claim<R: Runtime + 'static>(
    daemon: &Daemon<R>,
    world: &dyn World,
    wave: &Wave,
    work: &Work,
) -> Result<()> {
    let limit = daemon.room().limit().sessions;
    let wait = claim_wait(daemon.live_count(), limit, daemon.claim_wait());
    if wait.is_zero() {
        return Ok(());
    }
    tokio::time::sleep(wait).await;
    let item = work.item();
    let claimed = world.claims(wave).await.is_ok_and(|c| c.contains(&item));
    let held = world.holds(wave).await.is_ok_and(|h| h.contains(&item));
    match claimed || held {
        true => Err(Taken.into()),
        false => Ok(()),
    }
}

/// The steps of a job up to the end of its session: the wait before the
/// claim ([`wait_to_claim`]) when the lead did not ask for the job
/// (`asked`), the read of the status `riff/verify` of the head of a
/// verify ([`World::verdict`]: a verdict that stops the verify,
/// [`Verdicts::stops`], is the error [`Verified`]), the image that the
/// wave pins, the steps before the start, and the session.
/// `None` when the claim failed. Hold [`Daemon::in_job`]
/// of the item from before the call to the end of [`finish`]: the sweep
/// then keeps the folder.
async fn start_job<R: Runtime + 'static>(
    daemon: &Arc<Daemon<R>>,
    wave: &Wave,
    work: &Work,
    slot: Slot,
    asked: bool,
) -> Result<Option<(Record, Ended)>> {
    let world = daemon.world();
    if !asked {
        wait_to_claim(daemon, &*world, wave, work).await?;
    }
    // The look can show a head as waiting after its verify passed or
    // failed: on another host, or before the event of the status (#1960,
    // #2019).
    if let Work::Verify { commit, .. } = work
        && let Some(verdict) = world.verdict(wave, work).await
        && daemon.stops_verify(&wave.repo, work, verdict)
    {
        daemon.verified(&wave.repo, work, verdict);
        let line = text::wave_verify_done(&wave.repo, &work.item(), commit, verdict);
        return Err(Verified(line).into());
    }
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

/// What the steps after the end of a session did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Finish {
    /// The line of each step.
    pub lines: Vec<String>,
    /// The PASS or FAIL that riff put on the pull request, for a verify:
    /// the wave starts no new verify of that head that the verdict stops
    /// ([`drop_verified`], #1960, #2019).
    pub verdict: Option<Verdict>,
}

impl Finish {
    /// True for a verify whose PASS or FAIL riff put on the pull request.
    /// Such a start does not count toward [`STARTS`] (#1651).
    pub fn reported(&self) -> bool {
        self.verdict.is_some()
    }
}

impl From<Vec<String>> for Finish {
    fn from(lines: Vec<String>) -> Finish {
        Finish {
            lines,
            verdict: None,
        }
    }
}

/// The steps after the end of the session of `record`, as it `ended`.
/// It gives the line of each step. The job `held` of the item ends after
/// the last step: a step reads the folder of the session, so the sweep
/// keeps it to then (01M53S7WDPQ2GWHJWJ6SZXZ5PN). The verdict of a verify
/// that riff reported goes into [`Daemon::verified`] before the job ends
/// (#1960, #2019).
pub(crate) async fn finish<R: Runtime + 'static>(
    daemon: Arc<Daemon<R>>,
    record: Record,
    ended: Ended,
    held: InJob,
) -> Finish {
    // The reason of the end, for the lead that reads "started N times".
    let end = crate::ends::End::of(&record, ended, &crate::run::Scrub::new([]));
    daemon.note_end(&record.live.item, end.reason());
    let done = daemon.world().finish(&record, ended).await;
    if let (
        Ok(Finish {
            verdict: Some(verdict),
            ..
        }),
        Some(job),
    ) = (&done, &record.job)
    {
        daemon.verified(&job.wave.repo, &job.work, *verdict);
    }
    drop(held);
    daemon.tell_changed();
    done.unwrap_or_else(|e| {
        let error = format!("{e:#}");
        eprintln!("riff daemon: {}: {error}", record.live.name);
        vec![error].into()
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
    /// The token of the writes of the daemon on each repository
    /// (01M6XZJFE2ZTR5RE3AWM30R704).
    writes: gh_token::Writes,
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
        None => riff_core::selector::Selector::lead(&wave.repo),
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

/// The names of the checks of pull request `number` of `repo` that failed
/// now, that the ruleset of the default branch requires, and that are
/// runs of GitHub Actions, with `gh`: the session fixed no other
/// ([`rollout::Pull::failed_runs`]). The rules are read only when a run
/// failed ([`repo_setup::full_check`]).
async fn still_failed(gh: pr::Gh, repo: String, number: u64) -> Result<Vec<String>> {
    blocking(move || {
        let n = number.to_string();
        let mut pull: rollout::Pull =
            gh.json(&["pr", "view", &n, "--json", rollout::PULL_FIELDS])?;
        if !pull.failed_runs().is_empty() {
            pull.required = Some(repo_setup::full_check(&gh, &repo)?);
        }
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
    let to = [riff_core::selector::Selector::lead(&wave.repo)];
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
            still_failed(gh.clone(), wave.repo.clone(), pull.number)
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
///
/// A report that works sets [`Finish::verdict`]: the start of that verify
/// does not count toward [`STARTS`] (#1651).
///
/// With no PASS or FAIL, or a report that fails for another reason, the
/// lead gets a message that the pull request waits for a new verify. A
/// pull request that merged or closed waits for none: the lead gets a
/// note that says so ([`open_now`], 01M75R71BCDCP882W75GSE52VT).
async fn after_verify(
    gh: pr::Gh,
    api: &Api,
    record: &Record,
    wave: &Wave,
    (pull, commit): (u64, &str),
    ended: Ended,
) -> Result<Finish> {
    let me = &record.me;
    let thread = wave.thread()?;
    let lead = [riff_core::selector::Selector::lead(&wave.repo)];
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
        let open = open_now(&gh, pull).await;
        let body = text::wave_no_result(item, pull, open);
        api.post(me, Some(&thread), &lead, &body, waits_kind(open))
            .await?;
        return Ok(vec![body].into());
    };
    let (repo, tested, shown) = (wave.repo.clone(), commit.to_owned(), result.clone());
    let reporter = gh.clone();
    let reported =
        blocking(move || pr::report(&reporter, &repo, pull, &tested, verdict, &shown)).await;
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
                        (
                            text::wave_no_report(item, pull, &error, true),
                            Kind::Message,
                        )
                    }
                },
                false => {
                    let _ = std::fs::remove_dir_all(&record.folder);
                    let open = open_now(&gh, pull).await;
                    let body = text::wave_no_report(item, pull, &error, open);
                    (body, waits_kind(open))
                }
            };
            api.post(me, Some(&thread), &lead, &body, kind).await?;
            return Ok(vec![body].into());
        }
    };
    let line = text::verify_reported(verdict, pull, &reported);
    println!("{line}");
    if verdict == Verdict::Fail {
        let _ = std::fs::remove_dir_all(&record.folder);
    }
    post_report(api, me, &thread, (verdict, pull), &reported, &result).await?;
    Ok(Finish {
        lines: vec![line],
        verdict: Some(verdict),
    })
}

/// True when the pull request `pull` is open now, with `gh pr view`: a
/// merged or a closed one waits for no verify (#1960). A fault of `gh`
/// counts as open.
async fn open_now(gh: &pr::Gh, pull: u64) -> bool {
    let (gh, n) = (gh.clone(), pull.to_string());
    blocking(move || {
        gh.run(
            &["pr", "view", &n, "--json", "state", "--jq", ".state"],
            None,
        )
    })
    .await
    .map_or(true, |state| state.trim() == "OPEN")
}

/// The kind of the post to the lead after a verify that riff did not
/// report: a message while a new verify waits (`open`), else a note.
fn waits_kind(open: bool) -> Kind {
    match open {
        true => Kind::Message,
        false => Kind::Note,
    }
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
    let to = pr::result_to(reported.issue, thread, &api.who(me, false).await?);
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
        frame: Option<run::Frame>,
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
                let frame = frame.context("a build has a milestone")?;
                // The trailers of the session (01M6WZ2CB56PNSYKEX4E0ZJV3Y).
                let made = {
                    let (gh, session) = (forge(&wave.repo).by("token"), session.to_owned());
                    blocking(move || Ok(run::made_by(&gh, &session))).await?
                };
                let body = pr::with_session(&frame.body()?, &made)?;
                let milestone = &frame.milestone;
                let mut text = text::run_start(
                    &wave.repo, base, issue, &branch, session, &body, milestone, earlier,
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
                text::wave_verify_start(&wave.repo, base, *issue, *pull, commit, &checks)
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
            no_github: false,
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
///   check that the ruleset of the default branch requires
///   ([`rollout::Stuck::Checks`], [`rollout::Pull::failed_checks`]), where
///   only the newest run of a check counts, and whose verify passed or
///   waits. It is a build, with the names of the checks that are runs of
///   GitHub Actions ([`rollout::Pull::failed_runs`]). The free work has no
///   verify of such a head (#1575): a failed required check fails the
///   verify, so the fix comes first, and the verify of the new head after
///   it.
/// - A failed check that the ruleset does not require
///   ([`rollout::Pull::failed_optional`]) holds no verify and no merge.
///   The verify of the head goes on, no session starts, and the lead gets
///   one notice for each such check ([`rollout::Stuck::Optional`]), for a
///   pull request whose verify passed or waits, out of the merge queue,
///   and with no conflict.
/// - A pull request in the merge queue ([`rollout::Pull::queued`]) is none
///   of these: the wave moves nothing for it (01M5TKPNDET56VR8WZ74FSR71F).
///   One that the queue removed, with a conflict, is a pull request to
///   rebase; with no conflict it is a notice ([`rollout::Stuck::Dequeued`]).
/// - A pull request that no session can fix gets a notice for the lead:
///   one with auto-merge off after a pass ([`rollout::Stuck::AutoMergeOff`]),
///   a release or a tracker with a failed required check
///   (01M74MGTCR650HNX4E1THAAG69), and a failed check that
///   is no run of GitHub Actions ([`rollout::Stuck::Other`]), one notice
///   for each check. No session builds a release (the label `release`) or
///   a tracker. A verified pull request that waits for a person is a notice
///   too: a review that GitHub needs ([`rollout::Stuck::Review`],
///   01M6FZ141Y0J8G2AYX0G5HY1CX, 01M74MGTCR650HNX4E1THAAG69), and
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
///
/// // The ruleset requires the Gate only: a failed `knurl adapter` holds no verify, and the lead hears of it.
/// let knurl = |pull: Pull| Pull {
///     checks: [pull.checks, vec![Check::run("knurl adapter", "FAILURE")]].concat(),
///     required: Some(vec!["Gate".into()]),
///     ..pull
/// };
/// let waits = Open { pulls: vec![knurl(pull(53, 16, None, "MERGEABLE"))], ..Open::default() };
/// let (free, fixes) = riff::wave::fixes(&waits, vec!["verify-issue-16".to_owned()]);
/// assert_eq!(free, ["verify-issue-16"]);
/// assert!(fixes.failed.is_empty());
/// let optional = Stuck::Optional(OtherCheck { name: "knurl adapter".into(), app: None, link: None });
/// let notice = Notice { pull: 53, branch: "worktree-issue-16".into(), commit: "c16".into(), stuck: optional };
/// assert_eq!(fixes.notices, [notice.clone()]);
/// // A failed Gate too: the fix of the Gate, no verify, and the same notice.
/// let both = Open { pulls: vec![gate(knurl(pull(53, 16, None, "MERGEABLE")))], ..Open::default() };
/// let (free, fixes) = riff::wave::fixes(&both, vec!["verify-issue-16".to_owned()]);
/// assert_eq!(free, ["issue-16"]);
/// assert_eq!(fixes.failed[&16].checks, ["Gate"]);
/// assert_eq!(fixes.notices, [notice]);
/// // A conflict gets the rebase first, and a failed verify is a build: no notice.
/// let conflict = Open { pulls: vec![knurl(pull(53, 16, None, "CONFLICTING"))], ..Open::default() };
/// assert!(riff::wave::fixes(&conflict, vec![]).1.notices.is_empty());
/// let failed = Open { pulls: vec![knurl(pull(53, 16, Some("FAILURE"), "MERGEABLE"))], ..Open::default() };
/// assert!(riff::wave::fixes(&failed, vec![]).1.notices.is_empty());
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
    // A failed check that the ruleset does not require holds no verify and
    // no merge: the lead hears of each one. A conflict gets the rebase
    // first, and a failed verify is a build already.
    let on_way = |p: &&rollout::Pull| {
        matches!(
            p.verify(),
            Some(rollout::Verify::Asked | rollout::Verify::Passed)
        ) && !p.queued()
            && !p.conflict()
    };
    for p in open.pulls.iter().filter(on_way) {
        for check in p.failed_optional() {
            found.notices.push(Notice {
                pull: p.number,
                branch: p.branch.clone(),
                commit: p.head.clone(),
                stuck: rollout::Stuck::Optional(check),
            });
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
    // An issue that waits for an answer gets no rebase and no fix of checks
    // (#1605): its pull request waits as it is.
    let asking: HashSet<u64> = open
        .issues
        .iter()
        .filter(|i| i.question())
        .map(|i| i.number)
        .collect();
    let at = free.iter().take_while(|item| fix(item)).count();
    let new: Vec<String> = found
        .conflicts
        .keys()
        .chain(found.failed.keys())
        .filter(|n| !asking.contains(n))
        .map(|n| format!("issue-{n}"))
        .filter(|item| !free.contains(item))
        .collect();
    free.splice(at..at, new);
    (free, found)
}

/// What the thread says of the verify of each head: for each pair of an
/// issue and a commit (7 characters), its last verify request
/// ([`rollout::Verify::Asked`]) or verify result (`PASS` or `FAIL`) in
/// `bodies`, the bodies of the messages of the thread, oldest first.
/// [`rollout::Pull::verify`] takes it with the status of the head.
///
/// - A request after a FAIL answers the fail (#1157). A fix of the body
///   of a pull request makes no new commit, so the failed status of the
///   head stays.
/// - A PASS after a FAIL of the same head counts over the failed status
///   (01M77JWXEGDDMB7TW97APGKW9W, #2014). riff sets the status before it
///   posts the result, so the look can still read the old failure.
///
/// ```
/// use riff::rollout::Verify;
/// use riff::wave::said;
///
/// let request = "verify request: issue-12, PR #40, branch worktree-issue-12, commit 1a2b3c4, from the riff daemon (session 65f510d7).";
/// let result = |word| format!("verify result: {word} for issue-12, PR #40, commit 1a2b3c4. PR comment https://c.");
/// let (fail, pass) = (result("FAIL"), result("PASS"));
/// let of = |bodies: &[&str]| said(bodies).get(&(12, "1a2b3c4".into())).copied();
/// assert_eq!(of(&[]), None);
/// assert_eq!(of(&[request]), Some(Verify::Asked));
/// assert_eq!(of(&[request, &fail]), Some(Verify::Failed));
/// assert_eq!(of(&[request, &fail, request]), Some(Verify::Asked));
/// assert_eq!(of(&[request, &fail, request, &pass]), Some(Verify::Passed));
/// // A result names the full commit.
/// let full = "verify result: FAIL for issue-12, PR #40, commit 1a2b3c4d5e6f7a8b9c0d. PR comment https://c.";
/// assert_eq!(of(&[request, full]), Some(Verify::Failed));
/// ```
pub fn said(bodies: &[&str]) -> HashMap<(u64, String), rollout::Verify> {
    let mut said = HashMap::new();
    for body in bodies {
        if let Some(head) = request_of(body) {
            said.insert(head, rollout::Verify::Asked);
        } else if let Some((word, rest)) = body
            .strip_prefix(VERIFY_RESULT)
            .and_then(|r| r.split_once(" for "))
        {
            let verify = match word {
                "PASS" => rollout::Verify::Passed,
                "FAIL" => rollout::Verify::Failed,
                _ => continue,
            };
            said.extend(item_commit(rest).map(|head| (head, verify)));
        }
    }
    said
}

/// The verify requests that come after a verify result of their head:
/// the heads whose last word in `bodies` is a verify request ([`said`])
/// and that have a verify result before it (01M77K0EGH1HF0WA7WZH8BP2BP,
/// #2019). The lead asks for a new verify of a head whose verify failed.
/// The first verify request of a head is not one: a look that read the
/// thread before the result came sees only that request.
///
/// ```
/// use riff::wave::asked_again;
///
/// let request = "verify request: issue-12, PR #40, branch worktree-issue-12, commit 1a2b3c4, from the riff daemon (session 65f510d7).";
/// let result = "verify result: FAIL for issue-12, PR #40, commit 1a2b3c4d5e6f. PR comment https://c, riff/verify failure set.";
/// assert!(asked_again(&[request]).is_empty());
/// assert!(asked_again(&[request, result]).is_empty());
/// assert!(asked_again(&[request, result, request]).contains(&(12, "1a2b3c4".into())));
/// assert!(asked_again(&[request, result, request, result]).is_empty());
/// ```
pub fn asked_again(bodies: &[&str]) -> HashSet<(u64, String)> {
    let results: HashSet<(u64, String)> = bodies.iter().filter_map(|b| result_of(b)).collect();
    said(bodies)
        .into_iter()
        .filter(|(head, verify)| *verify == rollout::Verify::Asked && results.contains(head))
        .map(|(head, _)| head)
        .collect()
}

/// The start of a verify request in the thread.
const VERIFY_REQUEST: &str = "verify request: ";

/// The start of a verify result in the thread.
const VERIFY_RESULT: &str = "verify result: ";

/// The issue and the commit (7 characters) of the verify request `body`:
/// `verify request: issue-N, PR #P, ... commit ABCDEFG`.
fn request_of(body: &str) -> Option<(u64, String)> {
    body.strip_prefix(VERIFY_REQUEST).and_then(item_commit)
}

/// The issue and the commit (7 characters) of the verify result `body`:
/// `verify result: VERDICT for issue-N, PR #P, commit ABCDEFG`.
fn result_of(body: &str) -> Option<(u64, String)> {
    body.strip_prefix(VERIFY_RESULT)
        .and_then(|rest| rest.split_once(" for "))
        .and_then(|(_, rest)| item_commit(rest))
}

/// True unless `body` is a verify request or a verify result that `from`
/// may not send in the thread of the repository `repo` (#2027,
/// 01M77PF3MZ1NG1RCP3G8TZHT2E):
///
/// - a verify result for issue-N: only the verify session of issue-N (a
///   session of `repo` that claims `verify-issue-N`), or the riff daemon;
/// - a verify request for issue-N: only the work session of the item (a
///   session of `repo` that claims `issue-N`), the riff daemon, or the
///   lead of `repo`.
///
/// The riff daemon posts as the session of the job, or as the person: a
/// sender with no session, but not riff-server. `from` is the URI of the
/// sender when it posted, with its claims and its lead mark from
/// riff-server. GitHub still requires the status `riff/verify` of the
/// head, so a wrong sender cannot merge code with no verify.
///
/// ```
/// use riff::wave::right_sender;
/// use riff_core::name::SessionUri;
///
/// let uri = |text: &str| text.parse::<SessionUri>().unwrap();
/// let request = "verify request: issue-12, PR #40, branch worktree-issue-12, commit 1a2b3c4.";
/// let result = "verify result: PASS for issue-12, PR #40, commit 1a2b3c4.";
/// let author = uri("riff://ann@heron/o/r?session=a1&claim=issue-12#issue-12");
/// let verifier = uri("riff://ann@heron/o/r?session=v1&claim=verify-issue-12#verify-issue-12");
/// let lead = uri("riff://ann@heron/o/r?session=l1&lead=true");
/// let person = uri("riff://ann@heron/o/r");
/// let other = uri("riff://ann@heron/o/r?session=b2&claim=issue-13");
/// assert!(right_sender(&author, request, "o/r"));
/// assert!(right_sender(&lead, request, "o/r"));
/// assert!(right_sender(&person, request, "o/r"));
/// assert!(!right_sender(&verifier, request, "o/r"));
/// assert!(!right_sender(&other, request, "o/r"));
/// assert!(right_sender(&verifier, result, "o/r"));
/// assert!(right_sender(&person, result, "o/r"));
/// assert!(!right_sender(&author, result, "o/r"));
/// assert!(!right_sender(&lead, result, "o/r"));
/// // The lead of another repository.
/// assert!(!right_sender(&lead, request, "o/other"));
/// // riff-server is not the riff daemon.
/// assert!(!right_sender(&SessionUri::server(), result, "o/r"));
/// // Each other body is a plain message.
/// assert!(right_sender(&other, "hello", "o/r"));
/// ```
pub fn right_sender(from: &SessionUri, body: &str, repo: &str) -> bool {
    let session = from.who().session().is_some();
    let daemon = !session && !from.is_server();
    let here = session && from.place().repo_text() == repo;
    let claims = |item: String| here && from.claims().contains(&item);
    if let Some((issue, _)) = request_of(body) {
        daemon || here && from.lead() || claims(format!("issue-{issue}"))
    } else if let Some((issue, _)) = result_of(body) {
        daemon || claims(format!("verify-issue-{issue}"))
    } else {
        true
    }
}

/// The bodies of `messages`, the messages of the thread of the
/// repository `repo`, oldest first, that the wave reads: each body but a
/// verify request or a verify result from a wrong sender
/// ([`right_sender`], #2027). Such a body is a plain message, and the
/// wave does not count it.
///
/// ```
/// use riff_core::wire::{Kind, Message};
///
/// let said = |from: &str, body: &str| Message {
///     seq: 0,
///     from: from.parse().unwrap(),
///     to: vec![],
///     body: body.into(),
///     at_ms: 0,
///     kind: Kind::Message,
///     sig: None,
///     payload: None,
/// };
/// let request = "verify request: issue-12, PR #40, branch worktree-issue-12, commit 1a2b3c4.";
/// let messages = [
///     said("riff://ann@heron/o/r?session=b2", request),
///     said("riff://ann@heron/o/r?session=b2", "hello"),
///     said("riff://ann@heron/o/r?session=a1&claim=issue-12", request),
/// ];
/// assert_eq!(riff::wave::counted(&messages, "o/r"), ["hello", request]);
/// ```
pub fn counted<'a>(messages: &'a [Message], repo: &str) -> Vec<&'a str> {
    messages
        .iter()
        .filter(|m| right_sender(&m.from, &m.body, repo))
        .map(|m| m.body.as_str())
        .collect()
}

/// The issue and the commit (7 characters) of `issue-N, PR #P, ... commit
/// ABCDEFG`.
fn item_commit(rest: &str) -> Option<(u64, String)> {
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
}

/// The place of the first verify request of each issue in `bodies`, the
/// bodies of the messages of the thread, oldest first: the order of the
/// verifies and the rebases ([`order()`], #1710). A request for a new
/// head, after a rebase or a fix, keeps the place of the item (#1801,
/// 01M6YPYTKQKSJRS0A8WZQRCCFD).
///
/// ```
/// use riff::wave::requests;
///
/// let request = |issue, commit| format!("verify request: issue-{issue}, PR #4{issue}, branch worktree-issue-{issue}, commit {commit}, from the riff daemon (session 65f510d7).");
/// let (one, two) = (request(1, "1a2b3c4"), request(2, "5e6f7a8"));
/// let rebased = request(2, "9b0c1d2");
/// let places = requests(&["hello", &two, &one, &rebased]);
/// assert_eq!(places[&2], 1);
/// assert_eq!(places[&1], 2);
/// assert_eq!(places.len(), 2);
/// ```
pub fn requests(bodies: &[&str]) -> HashMap<u64, usize> {
    let mut places = HashMap::new();
    for (place, body) in bodies.iter().enumerate() {
        if let Some((issue, _)) = request_of(body) {
            places.entry(issue).or_insert(place);
        }
    }
    places
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
    look_with(gh, repo, claims, holds, &[])
}

/// [`look_on`] with `messages`, the messages of the thread of the
/// repository, oldest first. A verify request or a verify result counts
/// only from its right sender ([`counted`], #2027). What the thread says
/// of the verify of a head ([`said`]) counts with its status: a pull
/// request with a failed verify whose head has a new verify request waits
/// for a verify again, and one whose head has a PASS after the fail
/// passed. The places of the verify requests ([`requests`]) give the
/// order of the verifies ([`order()`]).
pub fn look_with(
    gh: &pr::Gh,
    repo: &str,
    claims: BTreeSet<String>,
    holds: BTreeSet<String>,
    messages: &[Message],
) -> Result<Look> {
    look_from(gh, repo, claims, holds, messages, None)
}

/// [`look_with`] with the events of GitHub of the repository in `told`
/// (#1656). While they are on ([`hooks::Told::on`]), the look reads only
/// what the wakes since the last look named, and reads in full at the
/// safety look ([`hooks::Told::facts`]). Else it asks GitHub first whether
/// anything changed ([`crate::changes`]).
pub fn look_told(
    gh: &pr::Gh,
    repo: &str,
    claims: BTreeSet<String>,
    holds: BTreeSet<String>,
    messages: &[Message],
    told: &hooks::Told,
) -> Result<Look> {
    look_from(gh, repo, claims, holds, messages, Some(told))
}

/// The look of [`look_with`] and [`look_told`].
fn look_from(
    gh: &pr::Gh,
    repo: &str,
    claims: BTreeSet<String>,
    holds: BTreeSet<String>,
    messages: &[Message],
    told: Option<&hooks::Told>,
) -> Result<Look> {
    let bodies = counted(messages, repo);
    let said = said(&bodies);
    // The wakes of GitHub name what changed (#1656). Else GitHub is asked
    // first whether anything changed (#1566). A branch that does not read
    // is the error of the look: the wave starts no work, and reads it
    // again at the next look.
    let read = |gh: &pr::Gh| {
        Ok(changes::Facts {
            branch: pr::default_branch(gh, repo)?,
            open: rollout::Open::of(gh, repo)?,
        })
    };
    let now = crate::daemon::now_ms();
    let changes::Facts { branch, mut open } = match told {
        Some(told) if told.on(now) => told.facts(gh, now, read)?,
        Some(told) => {
            told.off();
            changes::facts(gh, repo, || read(gh))?
        }
        None => changes::facts(gh, repo, || read(gh))?,
    };
    let claimed: HashSet<String> = claims.iter().cloned().collect();
    let held: HashSet<String> = holds.iter().cloned().collect();
    for pull in &mut open.pulls {
        let commit: String = pull.head.chars().take(7).collect();
        pull.said = pull.issue().and_then(|n| said.get(&(n, commit)).copied());
    }
    let (free, fixes) = fixes(&open, open.free_work(&claimed, &held));
    let no_done_when = open.no_done_when(&claimed, &held).into_iter().collect();
    let merged = open.unsaid();
    let mut pulls = waiting(gh, repo, &free, &open)?;
    // A new verify request after the result of the head (#2019).
    let again = asked_again(&bodies);
    for (issue, wait) in &mut pulls {
        let commit: String = wait.commit.chars().take(7).collect();
        wait.again = again.contains(&(*issue, commit));
    }
    let lanes = lanes(&open, &free, &requests(&bodies), |pull| {
        order::files(gh, repo, pull.number, &pull.head).unwrap_or_default()
    });
    Ok(Look {
        paused: false,
        no_lead: false,
        free,
        claims,
        holds,
        pulls,
        lanes,
        fixes,
        no_done_when,
        merged,
        branch,
    })
}

/// The lanes of the order of the verifies and the rebases ([`order()`],
/// #1710, #1801): each open pull request of an item whose verify waits or
/// runs, each with a conflict ([`rollout::Pull::conflict`]), also after a
/// pass, and each whose verify passed and that nothing stops
/// ([`rollout::Pull::stuck`]), with its head, the files that it changes
/// (`files`) and the place of the first verify request of its item
/// (`places`, [`requests`]). None when fewer than two are open, or no
/// verify and no rebase of them is in the free work `free`: then nothing
/// waits, and no file is read.
///
/// ```
/// use std::collections::HashMap;
/// use riff::rollout::{Check, Open, Pull};
///
/// let pull = |issue: u64, checks| Pull {
///     number: 40 + issue,
///     branch: format!("worktree-issue-{issue}"),
///     head: format!("c{issue}"),
///     checks,
///     mergeable: Some("MERGEABLE".into()),
///     auto_merge: true,
///     ..Pull::default()
/// };
/// let mergeable = |state: &str, pull: Pull| Pull { mergeable: Some(state.into()), ..pull };
/// let open = Open {
///     pulls: vec![
///         pull(1, vec![]),
///         pull(2, vec![Check::verify("SUCCESS")]),
///         pull(3, vec![Check::verify("FAILURE")]),
///         // The verify passed, and auto-merge is off: it cannot merge.
///         Pull { auto_merge: false, ..pull(4, vec![Check::verify("SUCCESS")]) },
///         // The verify passed, and then a merge gave it a conflict.
///         mergeable("CONFLICTING", pull(5, vec![Check::verify("SUCCESS")])),
///         mergeable("UNKNOWN", pull(6, vec![])),
///     ],
///     ..Open::default()
/// };
/// let places = HashMap::from([(1, 7)]);
/// let free = ["verify-issue-1".to_owned()];
/// let lanes = riff::wave::lanes(&open, &free, &places, |p| [format!("f{}", p.number)].into());
/// assert_eq!(lanes.keys().copied().collect::<Vec<_>>(), [1, 2, 5, 6]);
/// assert_eq!((lanes[&1].asked, lanes[&1].passed), (Some(7), false));
/// assert_eq!((lanes[&2].asked, lanes[&2].passed), (None, true));
/// assert_eq!(lanes[&2].files, ["f42".to_owned()].into());
/// assert_eq!((lanes[&5].conflict, lanes[&5].passed), (true, false));
/// assert!(lanes[&6].unknown);
/// // No verify in the free work: no lane, and no read of the files.
/// assert!(riff::wave::lanes(&open, &[], &places, |_| panic!("no read")).is_empty());
/// // The rebase of 5 in the free work: the lanes.
/// assert_eq!(riff::wave::lanes(&open, &["issue-5".into()], &places, |_| [].into()).len(), 4);
/// ```
pub fn lanes(
    open: &rollout::Open,
    free: &[String],
    places: &HashMap<u64, usize>,
    files: impl Fn(&rollout::Pull) -> BTreeSet<String>,
) -> BTreeMap<u64, order::Lane> {
    let stuck: HashSet<u64> = open.stuck().iter().map(|(p, _)| p.number).collect();
    let on_way: Vec<(&rollout::Pull, u64, bool)> = open
        .pulls
        .iter()
        .filter_map(|p| {
            let issue = p.issue()?;
            match p.verify()? {
                rollout::Verify::Asked => Some((p, issue, false)),
                // A pass is lost with a conflict: the rebase comes first.
                rollout::Verify::Passed if p.conflict() => Some((p, issue, false)),
                rollout::Verify::Passed if !stuck.contains(&p.number) => Some((p, issue, true)),
                _ => None,
            }
        })
        .collect();
    let next = on_way.iter().any(|(p, issue, passed)| {
        let rebase = p.conflict() && free.contains(&format!("issue-{issue}"));
        !passed && (rebase || free.contains(&format!("verify-issue-{issue}")))
    });
    if on_way.len() < 2 || !next {
        return BTreeMap::new();
    }
    on_way
        .into_iter()
        .map(|(p, issue, passed)| {
            let lane = order::Lane {
                issue,
                pull: p.number,
                head: p.head.clone(),
                files: files(p),
                clean: BTreeSet::new(),
                passed,
                conflict: p.conflict(),
                unknown: p.unknown(),
                asked: places.get(&issue).copied(),
            };
            (issue, lane)
        })
        .collect()
}

/// The look of a wave of the repository `thread` that starts nothing, from
/// the pauses of riff-server `pauses`: the riff or the repository is
/// paused, or the repository has no live lead (01M776RSYKJK9NETFAD8G0CCVQ).
/// `None` when the wave goes on with its look.
///
/// ```
/// use riff::wave::{Look, stopped};
/// use riff_core::wire::{RiffReply, RiffState};
///
/// let thread = "o/r".parse()?;
/// let running = RiffReply::from(RiffState::Running);
/// assert_eq!(stopped(&running, &thread), None);
/// let paused = stopped(&RiffState::Paused.into(), &thread).unwrap();
/// assert!(paused.paused && !paused.no_lead);
/// let no_lead = RiffReply { no_lead: vec![thread.clone()], ..running };
/// assert_eq!(stopped(&no_lead, &thread), Some(Look { no_lead: true, ..Look::default() }));
/// assert_eq!(stopped(&no_lead, &"o/other".parse()?), None);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn stopped(pauses: &RiffReply, thread: &ThreadName) -> Option<Look> {
    let paused = pauses.state == RiffState::Paused;
    let no_lead = pauses.waits_for_lead(thread);
    (paused || no_lead).then(|| Look {
        paused,
        no_lead: !paused && no_lead,
        ..Look::default()
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
                again: false,
            },
        );
    }
    Ok(pulls)
}

impl World for Live {
    fn look<'a>(&'a self, wave: &'a Wave, told: Arc<hooks::Told>) -> BoxFuture<'a, Result<Look>> {
        Box::pin(async move {
            let person = identity::person(&wave.place(None)?, &wave.server)?;
            let thread = wave.thread()?;
            let api = Api::new(&wave.server).signed_in(None)?;
            if let Some(look) = stopped(&api.pauses(&person).await?, &thread) {
                return Ok(look);
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
            let messages: Vec<Message> = api
                .read(&person, &thread, true)
                .await
                .map(|all| all.into_iter().map(|m| m.message).collect())
                .unwrap_or_default();
            let (repo, main) = (wave.repo.clone(), wave.main.clone());
            blocking(move || {
                let mut look = look_told(&forge(&repo), &repo, claims, holds, &messages, &told)?;
                // Two pull requests whose heads merge with no conflict are
                // in two groups of the order (#1895).
                order::merges(&main, &mut look.lanes);
                Ok(look)
            })
            .await
        })
    }

    fn projects<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<Vec<ProjectFacts>>> {
        Box::pin(async move { project::projects_of(&wave.place(None)?, &wave.server).await })
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

    fn host<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<(), run::HostFault>> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || run::host_facts().map(drop))
                .await
                .unwrap_or_else(|e| Err(run::HostFault::Missing(e.to_string())))
        })
    }

    fn verdict<'a>(&'a self, wave: &'a Wave, work: &'a Work) -> BoxFuture<'a, Option<Verdict>> {
        Box::pin(async move {
            let commit = work.commit()?.to_owned();
            let repo = wave.repo.clone();
            let state = blocking(move || pr::verify_state(&forge(&repo), &repo, &commit)).await;
            match state {
                Ok(state) => state.as_deref().and_then(Verdict::of_state),
                Err(e) => {
                    eprintln!("riff daemon: {}: {}: {e:#}", wave.repo, work.item());
                    None
                }
            }
        })
    }

    fn merge_off<'a>(&'a self, wave: &'a Wave, pull: u64, head: &'a str) -> BoxFuture<'a, bool> {
        let (repo, head) = (wave.repo.clone(), head.to_owned());
        Box::pin(async move {
            blocking(move || Ok(rollout::merge_off(&forge(&repo), &repo, pull, &head)))
                .await
                .unwrap_or(false)
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
            let host = blocking(move || Ok(run::host_facts()?)).await?;
            let base = {
                let repo = wave.repo.clone();
                blocking(move || pr::default_branch(&forge(&repo), &repo)).await?
            };
            let frame = match work {
                Work::Build { issue } => {
                    let (repo, issue) = (wave.repo.clone(), *issue);
                    Some(blocking(move || run::frame(&forge(&repo), issue)).await?)
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
            match Self::prepare(wave, work, pinned, &host, &base, frame, &session, &me).await {
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

    fn finish<'a>(&'a self, record: &'a Record, ended: Ended) -> BoxFuture<'a, Result<Finish>> {
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
                    .map(Finish::from)
                }
                Work::Rebase { issue, commit, .. } => {
                    let gh = forge(&wave.repo);
                    let fix = Some(Fix::Rebase(commit));
                    after_build(gh, &api, record, wave, *issue, (fix, None), ended)
                        .await
                        .map(Finish::from)
                }
                Work::Checks {
                    issue,
                    commit,
                    checks,
                    ..
                } => {
                    let gh = forge(&wave.repo);
                    let fix = Some(Fix::Checks(commit, checks));
                    after_build(gh, &api, record, wave, *issue, (fix, None), ended)
                        .await
                        .map(Finish::from)
                }
                Work::Verify { pull, commit, .. } => {
                    // The result and its status go as the daemon
                    // (01M6XZJFE2ZTR5RE3AWM30R704).
                    let gh = forge(&wave.repo);
                    let gh = gh_token::writer(self, Some(&wave.server), &wave.repo, gh).await;
                    after_verify(gh, &api, record, wave, (*pull, commit), ended).await
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

    fn delete_old_kept<'a>(
        &'a self,
        gh: pr::Gh,
        repo: &'a str,
    ) -> BoxFuture<'a, Result<Vec<String>>> {
        let repo = repo.to_owned();
        Box::pin(blocking(move || sweep::delete_old_kept_on(&gh, &repo)))
    }

    fn merged_in<'a>(
        &'a self,
        gh: pr::Gh,
        _: &'a str,
        issue: u64,
        pull: u64,
        commit: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        let commit = commit.to_owned();
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

    fn leads(&self) -> bool {
        true
    }

    fn lead_holder<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<Option<SessionUri>>> {
        Box::pin(async move {
            let place = record.me.place();
            let thread = place
                .default_thread()
                .ok_or_else(|| anyhow!("{}", text::RUN_NO_REPO))?;
            let person = identity::person(place, &record.server)?;
            let api = Api::new(&record.server).signed_in(None)?;
            Ok(api.plan_show(&person, &thread).await?.lead)
        })
    }

    fn lead_claim<'a>(&'a self, record: &'a Record) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async move {
            let person = identity::person(record.me.place(), &record.server)?;
            let api = Api::new(&record.server).signed_in(None)?;
            Ok(api.claim_lead(&person, &record.live.session).await?.is_ok())
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

    fn holds<'a>(&'a self, wave: &'a Wave) -> BoxFuture<'a, Result<HashSet<String>>> {
        Box::pin(async move {
            let person = identity::person(&wave.place(None)?, &wave.server)?;
            let api = Api::new(&wave.server).signed_in(None)?;
            let plan = api.plan_show(&person, &wave.thread()?).await?;
            Ok(plan.holds.into_keys().collect())
        })
    }

    fn plan_changes<'a>(&'a self, wave: &'a Wave, changed: &'a Notify) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let follow = async {
                let person = identity::person(&wave.place(None)?, &wave.server)?;
                let api = Api::new(&wave.server).signed_in(None)?;
                follow_plan(&api, &person, &wave.thread()?, changed).await;
                anyhow::Ok(())
            };
            if let Err(e) = follow.await {
                eprintln!(
                    "riff daemon: {}: {}",
                    wave.repo,
                    text::wave_no_plan(&format!("{e:#}"))
                );
            }
            std::future::pending().await
        })
    }

    fn github_wakes<'a>(&'a self, wave: &'a Wave, told: &'a hooks::Told) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let follow = async {
                let person = identity::person(&wave.place(None)?, &wave.server)?;
                let api = Api::new(&wave.server).signed_in(None)?;
                hooks::follow(&api, &person, &wave.thread()?, told).await;
                anyhow::Ok(())
            };
            if let Err(e) = follow.await {
                eprintln!(
                    "riff daemon: {}: {}",
                    wave.repo,
                    text::github_no_wakes(&format!("{e:#}"))
                );
            }
            std::future::pending().await
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

    fn bot<'a>(&'a self, record: &'a Record, fact: BotFact) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let posted = async {
                let person = identity::person(record.me.place(), &record.server)?;
                let api = Api::new(&record.server).signed_in(None)?;
                api.bot(&person, fact).await
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
                let gh = gh_token::writer(self, Some(&wave.server), &wave.repo, gh).await;
                Ok::<_, anyhow::Error>(kept::report_all(&gh, &api, &person, wave, &sessions).await)
            };
            reported.await.unwrap_or_else(|e| {
                eprintln!("riff daemon: {}: {e:#}", wave.repo);
                Vec::new()
            })
        })
    }

    fn github_token<'a>(
        &'a self,
        server: &'a str,
        me: &'a SessionUri,
    ) -> BoxFuture<'a, Result<gh_token::Grant>> {
        Box::pin(async move {
            let session = me
                .who()
                .session()
                .ok_or_else(|| anyhow!("the URI {me} names no session"))?;
            self.grant(server, &me.place().repo_text(), Some(session))
                .await
        })
    }

    fn writes(&self) -> Option<&gh_token::Writes> {
        Some(&self.writes)
    }

    fn daemon_token<'a>(
        &'a self,
        server: &'a str,
        repo: &'a str,
    ) -> BoxFuture<'a, Result<gh_token::Grant>> {
        Box::pin(self.grant(server, repo, None))
    }

    fn revoke_token<'a>(&'a self, repo: &'a str, token: &'a str) -> BoxFuture<'a, Result<()>> {
        let gh = self.gh(repo).as_ref().clone().by("token");
        let token = zeroize::Zeroizing::new(token.to_owned());
        Box::pin(blocking(move || gh_token::revoke(&gh, &token)))
    }

    fn github_renews<'a>(
        &'a self,
        server: &'a str,
        place: &'a Place,
        to: mpsc::UnboundedSender<GitHubRenew>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            use futures::StreamExt;
            // A fault prints one time until it changes. A sign-in that
            // does not read yet is tried again.
            let said = |fault: &mut Option<String>, e: &anyhow::Error| {
                let e = format!("{e:#}");
                if fault.as_ref() != Some(&e) {
                    eprintln!("riff daemon: {server}: {}", text::daemon_no_renews(&e));
                }
                *fault = Some(e);
            };
            let mut fault = None;
            let api = loop {
                let signed_in = identity::person(place, server)
                    .and_then(|person| Ok((person, Api::new(server).signed_in(None)?)));
                match signed_in {
                    Ok(api) => break api,
                    Err(e) => said(&mut fault, &e),
                }
                tokio::time::sleep(STREAM_RETRY).await;
            };
            let (person, api) = &api;
            let renews = api::follow(|| api.github_renew(person), STREAM_RETRY);
            tokio::pin!(renews);
            while let Some(renew) = renews.next().await {
                match renew {
                    Ok(renew) => {
                        fault = None;
                        if to.send(renew).is_err() {
                            return;
                        }
                    }
                    Err(e) => said(&mut fault, &e),
                }
            }
        })
    }
}

impl Live {
    /// A token of a GitHub App from the riff-server `server`, asked as the
    /// person: for the session `session`, or for the writes of the daemon
    /// on `repo` with none (01M6WPMEEWX09DHJZF18BKBPYW,
    /// 01M6XZJFGNVG6B7DF5VQ3H1DNX). No token, and why, when riff-server
    /// has no sign-in or gives none, and when the `gh` of the person cannot
    /// push to `repo` (01M6Z87A04EXTT1FFJSJ1EM6TB). For a session, the why
    /// ends with the prompt for `claude` on the host
    /// (01M6Z87JTXW0Y99X08VHA2SSSR), but while riff-server cannot read the
    /// keys of the Apps: its why has its own prompt
    /// (01M6YC3T0TXANJH6YMTJZR1HCR).
    async fn grant(
        &self,
        server: &str,
        repo: &str,
        session: Option<&str>,
    ) -> Result<gh_token::Grant> {
        let none = |why: &str| {
            let grant = gh_token::Grant::none(why);
            match session {
                Some(_) => grant.repaired(repo),
                None => grant,
            }
        };
        let api = Api::new(server).signed_in(None)?;
        if !api.sends_token() {
            return Ok(none(text::GH_NO_SIGN_IN));
        }
        // No more rights than the person (01M6WPMNXN1BSBEH74TG5DDRTF).
        let gh = self.gh(repo).as_ref().clone().by("token");
        let checked = repo.to_owned();
        if !blocking(move || gh_token::can_push(&gh, &checked)).await? {
            return Ok(none(&text::gh_no_push(repo)));
        }
        let reply = match session {
            Some(session) => api.github_token(session).await,
            None => api.github_daemon_token(repo).await,
        };
        match reply {
            Ok(Some(reply)) if reply.no_github => Ok(gh_token::Grant::of(reply)),
            Ok(Some(reply)) => {
                let grant = gh_token::Grant::of(reply);
                Ok(match session {
                    Some(_) => grant.repaired(repo),
                    None => grant,
                })
            }
            Ok(None) => Ok(none(text::GH_NO_SIGN_IN)),
            Err(e) if is_not_found(&e) => Ok(none(text::GH_OLD_SERVER)),
            Err(e) => Err(e),
        }
    }
}

/// True when `error` is a reply 404 of riff-server: a server of an older
/// build has no such call.
fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<api::Refusal>()
        .is_some_and(|refusal| refusal.status == reqwest::StatusCode::NOT_FOUND)
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

    /// 01M77AB80MEFD9K8Y7FPDATY3D: each `gh` call of the wave, of its jobs
    /// and of the facts of the host has the time limit of the look. A `gh`
    /// that hangs ends at the limit, and the error says why.
    #[test]
    fn each_gh_of_the_wave_has_the_limit_of_the_look() {
        assert_eq!(forge("o/r").limit(), Some(sweep::GH_LIMIT));
        assert_eq!(forge("o/r").caller(), "wave");
        assert_eq!(sweep::gh("o/r").limit(), Some(sweep::GH_LIMIT));
        assert_eq!(run::host_gh().limit(), Some(sweep::GH_LIMIT));

        let dir = tempfile::tempdir().unwrap();
        let hangs = isolated::write_program(dir.path().join("gh"), "#!/bin/sh\nsleep 600\n");
        let limit = Duration::from_millis(300);
        let start = Instant::now();
        let error = pr::default_branch(&pr::Gh::at(&hangs).with_limit(limit), "o/r").unwrap_err();
        assert!(
            format!("{error:#}").contains("no answer in 0.3 s. riff stopped it."),
            "{error:#}"
        );
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    /// The link of the failed check of another app than GitHub Actions in
    /// the fake forge.
    const OTHER_LINK: &str = "https://app.circleci.com/workflow/9";

    /// The checks that the ruleset of `main` requires in the fake forge:
    /// the `Gate` and the check `ci` of another app. The run
    /// `knurl adapter` is not one of them.
    const REQUIRED: [&str; 2] = ["Gate", "ci"];

    /// The link of the failed run `knurl adapter` in the fake forge.
    const KNURL_LINK: &str = "https://github.com/o/r/actions/runs/9/job/1";

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
        /// The status of the verify is the old failure, and the thread has
        /// a PASS of the head after it (#2014).
        Repassed,
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
        /// The files that the pull request of each issue changes, for the
        /// order of the verifies (#1710). With none, it changes no file.
        files: BTreeMap<u64, Vec<String>>,
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
        /// True while riff-server names the repository with no live lead
        /// ([`Look::no_lead`]).
        no_lead: bool,
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
        /// A verify reports a FAIL, not a PASS.
        fails: bool,
        /// The state of the pull request of each issue that the look shows,
        /// when the look does not see the last change yet: for example the
        /// event of the status of a verify did not come yet (#1960). Else
        /// the look shows `pulls`.
        shown: BTreeMap<u64, Pr>,
        /// The reads of the status `riff/verify` of the head of a verify
        /// before its claim ([`World::verdict`]).
        status_reads: usize,
        /// The messages of the thread of the repository, oldest first,
        /// for a look that reads the fake `gh` of `reads`.
        thread: Vec<Message>,
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
        /// The issues whose pull request has a failed run of GitHub Actions
        /// `knurl adapter` that the ruleset does not require.
        failing_optional: BTreeSet<u64>,
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
        /// Each fact of a line of riff-bot of the daemon.
        bots: Vec<BotFact>,
        /// Why riff-server gives a session no token of a GitHub App. With
        /// none, each session gets a token of an App.
        no_app: Option<String>,
        /// The full check of a pull request is what the fake `gh` tells
        /// ([`FAKE_GH`]), as for the real look. Else each is done.
        checked: bool,
        /// The image that the wave pins, else [`IMAGE`].
        image: Option<String>,
        /// A `gh` that the look calls, with its own count of calls: the look
        /// fails when it fails (#1557). Else the look calls no `gh`.
        limit_gh: Option<Arc<pr::Gh>>,
        /// The merged pull request with `Refs #N` of each issue (#1754).
        merges: BTreeMap<u64, rollout::Merge>,
        /// The comments of each issue.
        comments: BTreeMap<u64, Vec<String>>,
        /// Each comment `Merged in` of the wave: the issue, the pull
        /// request and the commit.
        merged_ins: Vec<(u64, u64, String)>,
        /// The number of the next comments `Merged in` that fail.
        merged_in_fails: usize,
        /// The projects of the riff (#1863). `None`: `o/r` is a project
        /// that allows each host.
        projects: Option<Vec<ProjectFacts>>,
    }

    /// The project `o/r` with the hosts `hosts`: none allows each host.
    fn project_of(hosts: &[&str]) -> ProjectFacts {
        ProjectFacts {
            repo: "o/r".parse().unwrap(),
            by: None,
            at_ms: 0,
            hosts: hosts.iter().map(|h| (*h).to_owned()).collect(),
            leads: Vec::new(),
        }
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
        /// requests, as `gh` gives them. A pull request in `shown` has the
        /// state there.
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
                comments: self
                    .comments
                    .get(n)
                    .into_iter()
                    .flatten()
                    .map(|body| rollout::Comment { body: body.clone() })
                    .collect(),
                merge: self.merges.get(n).cloned(),
            });
            let shown = |n: &u64, pr: &Pr| self.shown.get(n).copied().unwrap_or(*pr);
            let pulls = self.pulls.iter().map(|(n, pr)| (n, shown(n, pr)));
            let pulls = pulls.map(|(n, pr)| rollout::Pull {
                number: 40 + n,
                branch: branch_of(*n),
                head: self.head(*n),
                checks: match pr {
                    Pr::Asked => Vec::new(),
                    Pr::Passed => vec![rollout::Check::verify("SUCCESS")],
                    Pr::Failed | Pr::Reasked | Pr::Repassed => {
                        vec![rollout::Check::verify("FAILURE")]
                    }
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
                .chain(self.failing_optional.contains(n).then(|| rollout::Check {
                    details: Some(KNURL_LINK.into()),
                    ..rollout::Check::run("knurl adapter", "FAILURE")
                }))
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
                said: match pr {
                    Pr::Reasked => Some(rollout::Verify::Asked),
                    Pr::Repassed => Some(rollout::Verify::Passed),
                    _ => None,
                },
                required: Some(REQUIRED.map(String::from).to_vec()),
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
        /// The forge and the riff: two hosts of one wave share them.
        forge: Arc<Mutex<Forge>>,
        /// The fake `gh` of the look ([`FAKE_GH`]).
        gh: Arc<pr::Gh>,
        /// The stream of the changes of the plan on a real riff-server: the
        /// API, the caller and the thread. Else the plan never changes.
        plan: Mutex<Option<(Api, SessionUri, ThreadName)>>,
        /// The stream of the wakes of the GitHub App on a real riff-server
        /// (#1656): the API, the caller and the thread. Else no wake comes.
        github: Mutex<Option<(Api, SessionUri, ThreadName)>>,
        /// The look reads the forge from this `gh`, as the real look does
        /// ([`look_told`]), with the events of the wave (#1656). Else the
        /// look takes the forge in memory.
        reads: Mutex<Option<Arc<pr::Gh>>>,
    }

    impl World for Fakes {
        fn look<'a>(&'a self, _: &'a Wave, told: Arc<hooks::Told>) -> BoxFuture<'a, Result<Look>> {
            Box::pin(async move {
                let reads = lock(&self.reads).clone();
                if let Some(gh) = reads {
                    let claims: BTreeSet<String> =
                        lock(&self.forge).claims.keys().cloned().collect();
                    let holds = lock(&self.forge).holds.clone();
                    let thread = lock(&self.forge).thread.clone();
                    let look =
                        blocking(move || look_told(&gh, "o/r", claims, holds, &thread, &told));
                    let look = look.await;
                    lock(&self.forge).branch_reads += 1;
                    return look;
                }
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
                                // A new verify request after the FAIL.
                                again: forge.shown.get(n).unwrap_or(&forge.pulls[n])
                                    == &Pr::Reasked,
                            };
                            (*n, waiting)
                        })
                        .collect(),
                };
                // The order of the verifies: the requests come in the order
                // of the pull requests.
                let files = |p: &rollout::Pull| -> BTreeSet<String> {
                    let files = p.issue().and_then(|n| forge.files.get(&n));
                    files.into_iter().flatten().cloned().collect()
                };
                let lanes = lanes(&open, &free, &HashMap::new(), files);
                Ok(Look {
                    paused: false,
                    no_lead: forge.no_lead,
                    free,
                    claims: forge.claims.keys().cloned().collect(),
                    holds: forge.holds.clone(),
                    pulls,
                    lanes,
                    fixes,
                    no_done_when: no_done_when.into_iter().collect(),
                    merged: open.unsaid(),
                    branch: "main".into(),
                })
            })
        }

        fn projects<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<Vec<ProjectFacts>>> {
            let projects = lock(&self.forge).projects.clone();
            Box::pin(async move { Ok(projects.unwrap_or_else(|| vec![project_of(&[])])) })
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

        /// The forge as it is, not as the look shows it (`shown`). With
        /// `reads`, the status of the head that the fake `gh` gives
        /// ([`pr::verify_state`]).
        fn verdict<'a>(&'a self, _: &'a Wave, work: &'a Work) -> BoxFuture<'a, Option<Verdict>> {
            let reads = lock(&self.reads).clone();
            let mut forge = lock(&self.forge);
            forge.status_reads += 1;
            let issue = work.issue();
            let head = work.commit() == Some(forge.head(issue).as_str());
            let verdict = match forge.pulls.get(&issue) {
                Some(Pr::Passed) if head => Some(Verdict::Pass),
                Some(Pr::Failed | Pr::Reasked) if head => Some(Verdict::Fail),
                _ => None,
            };
            let commit = work.commit().unwrap_or_default().to_owned();
            Box::pin(async move {
                let Some(gh) = reads else {
                    return verdict;
                };
                let state = blocking(move || pr::verify_state(&gh, "o/r", &commit)).await;
                state.ok().flatten().as_deref().and_then(Verdict::of_state)
            })
        }

        /// The ask of the real loop to the fake `gh` of `reads`
        /// ([`rollout::merge_off`]). Else the forge in memory, as the look
        /// shows it.
        fn merge_off<'a>(&'a self, _: &'a Wave, pull: u64, head: &'a str) -> BoxFuture<'a, bool> {
            let (reads, head) = (lock(&self.reads).clone(), head.to_owned());
            Box::pin(async move {
                let Some(gh) = reads else {
                    return true;
                };
                blocking(move || Ok(rollout::merge_off(&gh, "o/r", pull, &head)))
                    .await
                    .unwrap_or(false)
            })
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
                    env: vec![Env::Host(crate::claude_token::VAR.into())],
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
                    values: vec![(
                        crate::claude_token::VAR.into(),
                        "sk-ant-oat01-secret1".into(),
                    )],
                    secrets: vec!["sk-ant-oat01-secret1".into()],
                    server: "http://127.0.0.1:1".into(),
                    me,
                    terminal: false,
                    no_github: false,
                }))
            })
        }

        fn resume(&self, _: &Record) {}

        fn finish<'a>(&'a self, record: &'a Record, _: Ended) -> BoxFuture<'a, Result<Finish>> {
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
                // A verify with a result reports it.
                let reported = matches!(job.work, Work::Verify { .. }) && !forge.no_result;
                let verdict = reported.then_some(match forge.fails {
                    true => Verdict::Fail,
                    false => Verdict::Pass,
                });
                match job.work {
                    Work::Build { .. } if forge.no_pull => None,
                    Work::Build { issue } => forge.pulls.insert(issue, Pr::Asked),
                    Work::Verify { .. } if forge.no_result => None,
                    Work::Verify { issue, .. } if forge.fails => {
                        forge.pulls.insert(issue, Pr::Failed)
                    }
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
                Ok(Finish {
                    lines: Vec::new(),
                    verdict,
                })
            })
        }

        fn fate<'a>(&'a self, _: &'a str, issue: u64) -> BoxFuture<'a, Result<Fate>> {
            let fate = lock(&self.forge).fates.get(&issue).cloned();
            Box::pin(async move { Ok(fate.unwrap_or(Fate::Stays)) })
        }

        fn merged_in<'a>(
            &'a self,
            _: pr::Gh,
            _: &'a str,
            issue: u64,
            pull: u64,
            commit: &'a str,
        ) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                let mut forge = lock(&self.forge);
                if forge.merged_in_fails > 0 {
                    forge.merged_in_fails -= 1;
                    bail!("gh issue comment: HTTP 502");
                }
                forge.merged_ins.push((issue, pull, commit.to_owned()));
                let body = text::wave_merged(pull, commit);
                forge.comments.entry(issue).or_default().push(body);
                Ok(())
            })
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

        fn holds<'a>(&'a self, _: &'a Wave) -> BoxFuture<'a, Result<HashSet<String>>> {
            let holds = lock(&self.forge).holds.iter().cloned().collect();
            Box::pin(async move { Ok(holds) })
        }

        fn plan_changes<'a>(&'a self, _: &'a Wave, changed: &'a Notify) -> BoxFuture<'a, ()> {
            let plan = lock(&self.plan).clone();
            Box::pin(async move {
                if let Some((api, me, thread)) = plan {
                    follow_plan(&api, &me, &thread, changed).await;
                }
                std::future::pending().await
            })
        }

        fn github_wakes<'a>(&'a self, _: &'a Wave, told: &'a hooks::Told) -> BoxFuture<'a, ()> {
            let github = lock(&self.github).clone();
            Box::pin(async move {
                if let Some((api, me, thread)) = github {
                    hooks::follow(&api, &me, &thread, told).await;
                }
                std::future::pending().await
            })
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

        fn bot<'a>(&'a self, _: &'a Record, fact: BotFact) -> BoxFuture<'a, ()> {
            Box::pin(async move { lock(&self.forge).bots.push(fact) })
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

        /// A token of the session App for one hour, or no token and the
        /// why of [`Forge::no_app`].
        fn github_token<'a>(
            &'a self,
            _: &'a str,
            _: &'a SessionUri,
        ) -> BoxFuture<'a, Result<gh_token::Grant>> {
            let no_app = lock(&self.forge).no_app.clone();
            Box::pin(async move {
                Ok(match no_app {
                    Some(why) => gh_token::Grant::none(&why),
                    None => app_token(),
                })
            })
        }
    }

    /// A token of the session App of `o/r` that ends in one hour.
    fn app_token() -> gh_token::Grant {
        let ends = crate::daemon::sweep::date_text(gh_token::now_secs() + 3600);
        gh_token::Grant {
            token: Some(zeroize::Zeroizing::new("ghs_wave".to_owned())),
            acts: gh_token::ActsAs::Bot {
                bot: "riff-o[bot]".into(),
                role: riff_core::wire::GitHubRole::Worker,
                rights: [("contents".to_owned(), "write".to_owned())].into(),
                ends,
                installation: Some(1010),
            },
            bot_id: Some(1101),
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
            let forge = Forge {
                items: items.to_vec(),
                ..Forge::default()
            };
            Here::on(
                Arc::new(Mutex::new(forge)),
                sessions,
                free,
                every,
                Duration::ZERO,
            )
            .await
        }

        /// A host of the wave of `forge`, with the limit `sessions`, a
        /// floor of 8 GB and the free memory of `free` in GB. It looks at
        /// `main` and the pull requests each `every`. A full host waits
        /// `claim_wait` before a claim ([`CLAIM_WAIT`]). [`Here::new`] gives
        /// 0: no host of the other tests waits.
        async fn on(
            forge: Arc<Mutex<Forge>>,
            sessions: u32,
            free: Arc<AtomicU64>,
            every: Duration,
            claim_wait: Duration,
        ) -> Here {
            let root = tempfile::tempdir().unwrap();
            let dir = dir_in(root.path());
            let fake = Fake::default().with_image(IMAGE);
            let bin = root.path().join("bin");
            std::fs::create_dir(&bin).unwrap();
            let gh = pr::Gh::at(isolated::write_program(bin.join("gh"), FAKE_GH));
            let world = Arc::new(Fakes {
                root: root.path().to_owned(),
                forge,
                gh: Arc::new(gh),
                plan: Mutex::new(None),
                github: Mutex::new(None),
                reads: Mutex::new(None),
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
                .with_claim_wait(claim_wait)
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

    /// 01M776RSYKJK9NETFAD8G0CCVQ, 01M776RT1GW3N4MJDMPXZ8JAJE: while
    /// riff-server names the repository with no live lead, the wave starts
    /// no session, and its word says why. The session that runs goes on to
    /// the end of its item. A lead that starts ends the wait.
    #[tokio::test]
    async fn a_wave_with_no_lead_starts_no_session_and_the_sessions_go_on() {
        let free = Arc::new(AtomicU64::new(64));
        // Room for two: the loop looks while the session of issue-1 runs.
        let here = Here::new(&[1], 2, free).await;
        here.run();
        here.until(2, |h| h.daemon.live_count() == 1).await;
        let item = here.daemon.records()[0].live.item.clone();
        // The lead ends: the next look names the wait.
        here.forge().no_lead = true;
        let word = text::wave_no_lead("o/r");
        here.until(2, |h| h.daemon.waves_shown()[0].word == word)
            .await;
        here.idle(2).await;
        assert!(here.running(&item).is_some(), "the session goes on");
        assert!(!here.fake.calls().iter().any(|c| c.starts_with("stop ")));
        here.end(&item).await;
        here.until(2, |h| h.forge().finished.len() == 1).await;
        here.idle(2).await;
        assert_eq!(here.daemon.live_count(), 0, "no new session");
        assert_eq!(here.forge().starts.len(), 1);
        assert_eq!(here.daemon.waves_shown()[0].word, word);

        // A lead starts: the wave goes on.
        here.forge().no_lead = false;
        here.until(2, |h| h.forge().starts.len() == 2).await;
    }

    /// The wait of a full host before a claim in
    /// [`a_free_item_goes_to_the_host_with_the_most_free_room`]: a host
    /// with 3 sessions of 4 waits 3 s.
    const TEST_CLAIM_WAIT: Duration = Duration::from_secs(4);

    /// Tells the waves of `hosts` to look again, in order, until `done`
    /// holds.
    async fn look_until(hosts: &[&Here], done: impl Fn() -> bool) {
        let started = Instant::now();
        while !done() {
            assert!(started.elapsed() < isolated::HANG, "it did not happen");
            for here in hosts {
                here.daemon.tell_changed();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// #1729: the host with the most free room claims a free item first.
    /// One host runs 3 sessions of its limit of 4, the other 0 of 8. A
    /// new item comes, and both waves look at it, the busy one first. The
    /// host with 0 of 8 claims it at once. The busy host waits 3 s, sees
    /// the claim, and claims nothing.
    #[tokio::test]
    async fn a_free_item_goes_to_the_host_with_the_most_free_room() {
        let free = Arc::new(AtomicU64::new(64));
        let forge = Arc::new(Mutex::new(Forge {
            items: vec![1, 2, 3],
            ..Forge::default()
        }));
        let on = |sessions| {
            let every = crate::look::LOOK_EVERY;
            Here::on(
                forge.clone(),
                sessions,
                free.clone(),
                every,
                TEST_CLAIM_WAIT,
            )
        };
        let busy = on(4).await;
        busy.run();
        busy.until(4, |h| h.daemon.live_count() == 3).await;
        let empty = on(8).await;
        empty.run();
        empty.idle(8).await;
        assert_eq!(empty.daemon.live_count(), 0);

        lock(&forge).items.push(4);
        look_until(&[&busy, &empty], || {
            lock(&forge).claims.contains_key("issue-4")
        })
        .await;
        assert!(empty.running("issue-4").is_some(), "{:?}", lock(&forge));
        // The busy host waited, and found the item claimed.
        tokio::time::sleep(TEST_CLAIM_WAIT).await;
        busy.idle(4).await;
        assert!(busy.running("issue-4").is_none());
        let forge = lock(&forge);
        let starts = forge.starts.iter().filter(|(item, _)| item == "issue-4");
        assert_eq!(starts.count(), 1, "{forge:?}");
        assert_eq!(busy.daemon.live_count(), 3);
    }

    /// #1729: a free on riff-server wakes an idle wave at once. The lead
    /// holds the only item. The wave looks two times with the same facts:
    /// then it waits [`IDLE_LOOK_EVERY`], and no session ends to wake it.
    /// The lead frees the item on a real riff-server. The wave follows the
    /// changes of the plan there, looks at once, and starts the item, long
    /// before its next look at [`LOOK_EVERY`].
    #[tokio::test]
    async fn a_free_on_riff_server_wakes_an_idle_wave_at_once() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, riff_server::router()).await.unwrap();
        });
        let api = Api::new(&server);
        let place = crate::identity::place_from_text("thelio/o/r").unwrap();
        let who = riff_core::name::Who::new("mike", Some("lead1")).unwrap();
        let lead = SessionUri::new(who, place);
        api.start(&lead, StartReason::Process, false).await.unwrap();
        let thread: ThreadName = "o/r".parse().unwrap();
        api.hold(&lead, &thread, "issue-1", "a reading")
            .await
            .unwrap();

        let here = Here::new(&[1], 2, Arc::new(AtomicU64::new(64))).await;
        here.forge().holds.insert("issue-1".into());
        *lock(&here.world.plan) = Some((api.clone(), lead.clone(), thread.clone()));
        here.run();
        here.until(2, |h| h.forge().branch_reads >= 2).await;
        // A hold of another item on riff-server wakes the wave too: so the
        // wave follows the changes of the plan now.
        let looks = here.forge().branch_reads;
        let woken = async {
            while here.forge().branch_reads == looks {
                api.hold(&lead, &thread, "issue-9", "a probe")
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        };
        isolated::in_time(LOOK_EVERY / 3, woken)
            .await
            .expect("the hold woke the wave");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let looks = here.forge().branch_reads;
        assert!(here.forge().starts.is_empty());

        // Only the free on riff-server can wake the wave now.
        here.forge().holds.remove("issue-1");
        api.free(&lead, &thread, "issue-1").await.unwrap();
        let started = async {
            while here.forge().starts.is_empty() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        isolated::in_time(LOOK_EVERY / 3, started)
            .await
            .expect("the free woke the wave");
        assert!(here.forge().branch_reads > looks);
        assert_eq!(here.forge().starts[0].0, "issue-1");
    }

    /// A real riff-server in a test whose session App has the webhook
    /// secret `secret`, with a window of the debounce of 100 ms, and its
    /// URL.
    async fn riff_with_a_webhook(
        secret: &str,
    ) -> (riff_server::Service, String, tempfile::TempDir) {
        use riff_server::keys::{AppKeys, Apps, At, FILE};
        let keys = tempfile::tempdir().unwrap();
        let apps = Apps {
            session: Some(AppKeys::new(
                101,
                "riff-o",
                "o",
                &isolated::fake_secret(),
                Some(secret),
            )),
            lead: None,
        };
        std::fs::write(keys.path().join(FILE), serde_json::to_vec(&apps).unwrap()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        let config = riff_server::auth::Config {
            github: riff_server::github::Settings {
                keys: At::Dir(keys.path().to_owned()),
                debounce: Duration::from_millis(100),
                ..riff_server::github::Settings::default()
            },
            ..riff_server::auth::Config::new(&server)
        };
        let service = riff_server::Service::new(config);
        let router = service.router();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (service, server, keys)
    }

    /// Sends the event `kind` of GitHub with the body `body` to the
    /// webhook of `server`, signed with `secret`, as the delivery `id`.
    async fn deliver(server: &str, secret: &str, kind: &str, id: &str, body: &serde_json::Value) {
        let body = serde_json::to_vec(body).unwrap();
        let reply = reqwest::Client::new()
            .post(format!("{server}/hooks/github"))
            .header("x-github-event", kind)
            .header("x-github-delivery", id)
            .header(
                "x-hub-signature-256",
                riff_server::hooks::signature(secret, &body),
            )
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), 202);
    }

    /// 01M74A37JNKW1R2WRXAW520WJM: an issue that comes into the wave on
    /// GitHub wakes an idle wave at once. The wave has no work, so it
    /// waits [`IDLE_LOOK_EVERY`]. The issue comes into the wave on the
    /// forge, and GitHub sends the event `issues`, `milestoned` to a real
    /// riff-server. The wave follows the changes of the plan there, looks
    /// at once, and starts the item, long before its next look at
    /// [`LOOK_EVERY`].
    #[tokio::test]
    async fn a_new_item_of_the_wave_on_github_wakes_an_idle_wave_at_once() {
        let secret = &isolated::fake_secret();
        let (_service, server, _keys) = riff_with_a_webhook(secret).await;
        let api = Api::new(&server);
        let place = crate::identity::place_from_text("thelio/o/r").unwrap();
        let who = riff_core::name::Who::new("mike", Some("lead1")).unwrap();
        let lead = SessionUri::new(who, place);
        api.start(&lead, StartReason::Process, false).await.unwrap();
        let thread: ThreadName = "o/r".parse().unwrap();

        let here = Here::new(&[], 2, Arc::new(AtomicU64::new(64))).await;
        *lock(&here.world.plan) = Some((api.clone(), lead.clone(), thread.clone()));
        here.run();
        here.until(2, |h| h.forge().branch_reads >= 2).await;
        // The merge of a pull request on GitHub wakes the wave too: so the
        // wave follows the changes of the plan now.
        let looks = here.forge().branch_reads;
        let mut probe = 0;
        let woken = async {
            while here.forge().branch_reads == looks {
                probe += 1;
                let merged = serde_json::json!({
                    "action": "closed",
                    "repository": { "full_name": "o/r" },
                    "pull_request": { "number": 90 + probe, "merged": true },
                });
                deliver(
                    &server,
                    secret,
                    "pull_request",
                    &format!("probe-{probe}"),
                    &merged,
                )
                .await;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        };
        isolated::in_time(LOOK_EVERY / 3, woken)
            .await
            .expect("the merge woke the wave");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let looks = here.forge().branch_reads;
        assert!(here.forge().starts.is_empty());

        // Only the event of the new item can wake the wave now.
        here.forge().items.push(2);
        let milestoned = serde_json::json!({
            "action": "milestoned",
            "repository": { "full_name": "o/r" },
            "issue": { "number": 2 },
            "milestone": { "title": "Wave 1", "state": "open" },
        });
        deliver(&server, secret, "issues", "new-item", &milestoned).await;
        let started = async {
            while here.forge().starts.is_empty() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        isolated::in_time(LOOK_EVERY / 3, started)
            .await
            .expect("the new item woke the wave");
        assert!(here.forge().branch_reads > looks);
        assert_eq!(here.forge().starts[0].0, "issue-2");
    }

    /// 01M74A37FPGKYMHP0W588A88RJ: [`Api::github_watch`] gives each wake of
    /// the GitHub App of the repository: with an ID, first the wakes of the
    /// queue after it.
    #[tokio::test]
    async fn the_api_follows_the_wakes_of_the_github_app() {
        use futures::StreamExt;
        let secret = &isolated::fake_secret();
        let (_service, server, _keys) = riff_with_a_webhook(secret).await;
        let api = Api::new(&server);
        let me: SessionUri = "riff://mike@thelio/o/r".parse().unwrap();
        let thread: ThreadName = "o/r".parse().unwrap();
        let live = api.github_watch(&me, &thread, None).await.unwrap();
        tokio::pin!(live);
        let check = serde_json::json!({
            "repository": { "full_name": "o/r" },
            "check_run": { "pull_requests": [{ "number": 12 }] },
        });
        deliver(&server, secret, "check_run", "d-1", &check).await;
        deliver(&server, secret, "check_run", "d-2", &check).await;
        let wake = isolated::in_time(LOOK_EVERY, live.next())
            .await
            .expect("a wake")
            .unwrap()
            .unwrap();
        assert_eq!(wake.kinds, [riff_core::wire::GitHubEvent::CheckRun]);
        assert_eq!((wake.number, wake.events), (Some(12), 2));

        let queued = api.github_watch(&me, &thread, Some(0)).await.unwrap();
        tokio::pin!(queued);
        let again = isolated::in_time(LOOK_EVERY, queued.next())
            .await
            .expect("the wake of the queue")
            .unwrap()
            .unwrap();
        assert_eq!(again, wake);
    }

    impl Here {
        /// A new start of the daemon of this host, in the same folder and
        /// with the same world: the old daemon stopped.
        fn restart(&self) -> Arc<Daemon<Fake>> {
            let memory: crate::room::Memory = Arc::new(|| Some(64 * GB));
            let daemon = Daemon::new(self.fake.clone(), dir_in(self._root.path()))
                .unwrap()
                .with_memory(memory)
                .with_world(self.world.clone());
            Arc::new(daemon)
        }
    }

    /// The wave of `o/r` in `daemon`, as [`Here::run`] runs it.
    fn run_on(daemon: &Arc<Daemon<Fake>>) {
        daemon
            .run_wave(Wave {
                repo: "o/r".into(),
                main: "/src/r".into(),
                server: "http://127.0.0.1:1".into(),
                host: "thelio".into(),
            })
            .unwrap();
    }

    /// The payload of the event `pull_request` of GitHub for the merge of
    /// the pull request `number` of `o/r`.
    fn merge_event(number: u64) -> serde_json::Value {
        serde_json::json!({
            "action": "closed",
            "repository": { "full_name": "o/r" },
            "pull_request": { "number": number, "merged": true },
        })
    }

    /// Waits until the wakes that `told` got since the last call name
    /// each number of `numbers`, and gives what they named.
    async fn named(told: &hooks::Told, numbers: &[u64]) -> hooks::Named {
        let mut all = hooks::Named::new();
        let got = async {
            while !numbers.iter().all(|n| all.contains_key(&Some(*n))) {
                all.extend(told.take());
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        isolated::in_time(LOOK_EVERY, got)
            .await
            .expect("the wakes came");
        all
    }

    /// 01M74N2SDHCYTY4M6RD1BDAFWZ: the daemon opens the stream of the wakes
    /// of its repository, and gets the wake of a merged pull request. After
    /// a restart it opens the stream with `after=ID`, the ID of its last
    /// wake from its folder, and gets the wakes that it missed while it was
    /// away, and not the wakes before.
    #[tokio::test]
    async fn the_daemon_gets_the_wakes_of_its_repository_and_after_a_restart_the_missed_ones() {
        let secret = &isolated::fake_secret();
        let (_service, server, _keys) = riff_with_a_webhook(secret).await;
        let api = Api::new(&server);
        let place = crate::identity::place_from_text("thelio/o/r").unwrap();
        let who = riff_core::name::Who::new("mike", Some("lead1")).unwrap();
        let lead = SessionUri::new(who, place);
        api.start(&lead, StartReason::Process, false).await.unwrap();
        let thread: ThreadName = "o/r".parse().unwrap();

        let here = Here::new(&[], 2, Arc::new(AtomicU64::new(64))).await;
        *lock(&here.world.github) = Some((api.clone(), lead.clone(), thread.clone()));
        here.run();
        let told = here.daemon.told("o/r");
        // The stream of the first start has no `after`: a probe of a merge
        // goes through it once it is open.
        let mut probe = 0;
        let opened = async {
            while told.last().is_none() {
                probe += 1;
                let id = format!("probe-{probe}");
                deliver(
                    &server,
                    secret,
                    "pull_request",
                    &id,
                    &merge_event(900 + probe),
                )
                .await;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        };
        isolated::in_time(LOOK_EVERY, opened)
            .await
            .expect("the stream opened");

        // A pull request merges: its wake comes to the daemon, and the ID
        // goes to the folder of the daemon.
        deliver(
            &server,
            secret,
            "pull_request",
            "merge-41",
            &merge_event(41),
        )
        .await;
        let got = named(&told, &[41]).await;
        assert_eq!(got[&Some(41)], [riff_core::wire::GitHubEvent::PullRequest]);
        let dir = here.daemon.status().await.dir;
        assert_eq!(hooks::Told::load(&dir, "o/r").last(), told.last());

        // The daemon stops. Two pull requests merge while it is away.
        assert!(here.daemon.stop_wave("o/r").unwrap());
        let last = told.last();
        deliver(
            &server,
            secret,
            "pull_request",
            "merge-42",
            &merge_event(42),
        )
        .await;
        deliver(
            &server,
            secret,
            "pull_request",
            "merge-43",
            &merge_event(43),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(told.last(), last, "no stream while it is away");

        // A new start: the stream opens after the ID of the last wake.
        let again = here.restart();
        let told = again.told("o/r");
        assert_eq!(told.last(), last);
        run_on(&again);
        let missed = named(&told, &[42, 43]).await;
        assert!(!missed.contains_key(&Some(41)), "{missed:?}");
        assert!(told.last() > last);
        assert!(told.on(crate::daemon::now_ms()));
        again.stop_wave("o/r").unwrap();
    }

    /// A fake `gh` for a look that reads the forge as the real look does
    /// (#1656): the lists and the items come from the files of its folder,
    /// and each call goes in `gh.log`. The queue is empty, and the ask for
    /// one pull request ([`rollout::merge_off`]) gives the file `ask`. The
    /// ruleset of `main` requires no check, and the state of the status
    /// `riff/verify` of a head ([`pr::verify_state`]) is the file `status`.
    const READ_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/gh.log"
show() { if [ -e "$dir/$1" ]; then cat "$dir/$1"; else echo '[]'; fi; }
case "$1 $2" in
  "api repos/o/r")
    case "$*" in
      *--jq*) echo main ;;
      *) echo '{"default_branch":"main"}' ;;
    esac ;;
  "api repos/o/r/rules/branches/main"*) echo '[]' ;;
  "api repos/o/r/milestones"*) echo '[{"title":"Wave 1"}]' ;;
  "api repos/o/r/commits/"*/status) if [ -e "$dir/status" ]; then cat "$dir/status"; fi ;;
  "api repos/o/r/commits/"*) echo 'a commit' ;;
  "api graphql")
    case "$*" in
      *"pullRequest(number"*) show ask ;;
      *) echo '{}' ;;
    esac ;;
  "pr list")
    case "$*" in
      *"--state merged"*) show merged ;;
      *) show pulls ;;
    esac ;;
  "issue list") show issues ;;
  "pr view") cat "$dir/pull-$3" ;;
  "issue view") cat "$dir/issue-$3" ;;
  *) exit 1 ;;
esac
"#;

    /// The open issue `n` of `Wave 1`, as `gh issue list` gives it.
    fn open_issue(n: u64) -> serde_json::Value {
        serde_json::json!({
            "number": n,
            "title": format!("item {n}"),
            "body": "Done when: it works.",
            "comments": [],
            "milestone": { "title": "Wave 1" },
            "labels": [],
        })
    }

    /// The pull request 41 of issue 1 with `Refs #1`: its verify passed,
    /// and it waits for its merge.
    fn pull_41() -> serde_json::Value {
        serde_json::json!({
            "number": 41,
            "headRefName": "worktree-issue-1",
            "headRefOid": "c1",
            "isDraft": false,
            "statusCheckRollup": [
                { "__typename": "StatusContext", "context": "riff/verify", "state": "SUCCESS" },
                { "__typename": "CheckRun", "name": "Gate", "status": "COMPLETED", "conclusion": "SUCCESS", "workflowName": "CI" },
            ],
            "mergeable": "MERGEABLE",
            "mergeStateStatus": "CLEAN",
            "autoMergeRequest": { "enabledAt": "2026-11-11T10:00:00Z" },
            "body": "Refs #1\n\nThe summary.\n\nIssue: #1\n",
        })
    }

    /// A wave whose look reads the fake `gh` [`READ_GH`] in the folder
    /// `bin`, with the events of GitHub on: the safety look waits an hour,
    /// and a change waits no time for its wake.
    async fn reading(bin: &Path) -> (Here, Arc<hooks::Told>) {
        let here = Here::new(&[], 2, Arc::new(AtomicU64::new(64))).await;
        let gh = pr::Gh::at(isolated::write_program(bin.join("gh"), READ_GH)).with_shared_reads();
        *lock(&here.world.reads) = Some(Arc::new(gh));
        let times = (Duration::from_secs(3600), Duration::ZERO, Duration::ZERO);
        let told = hooks::Told::new("o/r").with_times(times.0, times.1, times.2);
        let told = here.daemon.set_told(told);
        let now = u64::try_from(crate::daemon::now_ms()).unwrap();
        told.add(&riff_core::wire::GitHubWake {
            id: now,
            repo: "o/r".parse().unwrap(),
            kinds: vec![riff_core::wire::GitHubEvent::Issues],
            number: Some(1),
            events: 1,
        });
        (here, told)
    }

    /// The reads of issues and pull requests in the log of [`READ_GH`] in
    /// `bin` since the last call.
    fn item_reads(bin: &Path) -> Vec<String> {
        let log = bin.join("gh.log");
        let calls = std::fs::read_to_string(&log).unwrap_or_default();
        let _ = std::fs::remove_file(&log);
        calls
            .lines()
            .filter(|c| c.starts_with("pr ") || c.starts_with("issue "))
            .map(str::to_owned)
            .collect()
    }

    /// 01M74N34Q330SHBWWTQDPG0W0B, 01M74N350YF6KSXGFNCZTDC8QY: a wave with a
    /// fake forge. The event of a merged pull request makes the look read
    /// that pull request only, with `gh pr view`, and no list, and the wave
    /// goes on: the merge with `Refs #1` gets its comment `Merged in`. Then
    /// an issue comes into the wave with no event: the safety look finds
    /// it, the wave starts it, and the change is a dropped event.
    #[tokio::test]
    async fn a_merge_reads_that_pull_request_only_and_the_safety_look_finds_a_dropped_event() {
        let bin = tempfile::tempdir().unwrap();
        let put = |name: &str, value: serde_json::Value| {
            isolated::write_whole(bin.path().join(name), value.to_string());
        };
        put("issues", serde_json::json!([open_issue(1)]));
        put("pulls", serde_json::json!([pull_41()]));
        let (here, told) = reading(bin.path()).await;
        here.run();
        // The first look reads in full: it has no facts yet.
        here.until(0, |h| h.forge().branch_reads >= 1).await;
        let reads = item_reads(bin.path());
        assert!(
            reads.iter().any(|c| c.starts_with("issue list")),
            "{reads:?}"
        );
        assert!(here.forge().merged_ins.is_empty());

        // Pull request #41 merges, and GitHub sends its event.
        let mut merged = pull_41();
        merged["state"] = "MERGED".into();
        merged["mergeCommit"] = serde_json::json!({ "oid": "m41" });
        put("pull-41", merged.clone());
        put("pulls", serde_json::json!([]));
        put("merged", serde_json::json!([merged]));
        told.add(&riff_core::wire::GitHubWake {
            id: told.last().unwrap() + 1,
            repo: "o/r".parse().unwrap(),
            kinds: vec![riff_core::wire::GitHubEvent::PullRequest],
            number: Some(41),
            events: 3,
        });
        here.until(0, |h| !h.forge().merged_ins.is_empty()).await;
        assert_eq!(here.forge().merged_ins, [(1, 41, "m41".to_owned())]);
        let reads = item_reads(bin.path());
        assert_eq!(reads.len(), 1, "{reads:?}");
        assert!(
            reads[0].starts_with("pr view 41 --repo o/r --json"),
            "{reads:?}"
        );

        // Issue 2 comes into the wave, and its event is lost. The safety look
        // reads in full, finds it, and the wave starts it.
        put("issues", serde_json::json!([open_issue(2), open_issue(1)]));
        told.expire();
        here.until(1, |h| {
            h.forge().starts.iter().any(|(item, _)| item == "issue-2")
        })
        .await;
        let reads = item_reads(bin.path());
        assert!(
            reads.iter().any(|c| c.starts_with("issue list")),
            "{reads:?}"
        );
        let shown = told.shown(crate::daemon::now_ms()).unwrap();
        assert_eq!(shown.dropped, 1, "{shown:?}");
        let change = shown.last_dropped.unwrap().change;
        let issues = riff_core::wire::GitHubEvent::Issues;
        assert_eq!(
            (change.number, change.pull, change.kind),
            (2, false, issues)
        );
    }

    /// 01M75WEDFKXGHMRFR73D1QDZCE (#1975): a wave with a fake forge and the
    /// events of GitHub on. Pull request #41 merges with `Closes #1`, and
    /// GitHub sends its event, but no event of the close of #1: the kept
    /// list still has #1 open, with no open pull request. The wave starts
    /// no build of issue-1, and adds no comment `Merged in`. Then issue 2
    /// comes with its event, and the wave starts it.
    #[tokio::test]
    async fn a_merge_with_closes_starts_no_build_of_its_issue_before_the_close() {
        let bin = tempfile::tempdir().unwrap();
        let put = |name: &str, value: serde_json::Value| {
            isolated::write_whole(bin.path().join(name), value.to_string());
        };
        let mut pull = pull_41();
        pull["body"] = "Closes #1\n\nThe summary.\n\nIssue: #1\n".into();
        put("issues", serde_json::json!([open_issue(1)]));
        put("pulls", serde_json::json!([pull.clone()]));
        let (here, told) = reading(bin.path()).await;
        here.run();
        here.until(0, |h| h.forge().branch_reads >= 1).await;
        item_reads(bin.path());

        // Pull request #41 merges, and GitHub sends its event only.
        let mut merged = pull;
        merged["state"] = "MERGED".into();
        merged["mergeCommit"] = serde_json::json!({ "oid": "m41" });
        merged["mergedAt"] = chrono::Utc::now().to_rfc3339().into();
        put("pull-41", merged.clone());
        put("pulls", serde_json::json!([]));
        put("merged", serde_json::json!([merged]));
        told.add(&riff_core::wire::GitHubWake {
            id: told.last().unwrap() + 1,
            repo: "o/r".parse().unwrap(),
            kinds: vec![riff_core::wire::GitHubEvent::PullRequest],
            number: Some(41),
            events: 3,
        });
        let log = bin.path().join("gh.log");
        let viewed = || std::fs::read_to_string(&log).is_ok_and(|l| l.contains("pr view 41"));
        here.until(0, |_| viewed()).await;
        // No session starts: issue-1 is merged.
        here.idle(0).await;
        let reads = item_reads(bin.path());
        assert!(
            !reads.iter().any(|c| c.starts_with("issue")),
            "no read of the issue: {reads:?}"
        );

        // Issue 2 comes into the wave with its event: the wave starts it,
        // and still not issue-1.
        let mut view = open_issue(2);
        view["state"] = "OPEN".into();
        view["url"] = "https://github.com/o/r/issues/2".into();
        put("issue-2", view);
        put("issues", serde_json::json!([open_issue(2), open_issue(1)]));
        told.add(&riff_core::wire::GitHubWake {
            id: told.last().unwrap() + 1,
            repo: "o/r".parse().unwrap(),
            kinds: vec![riff_core::wire::GitHubEvent::Issues],
            number: Some(2),
            events: 1,
        });
        here.until(1, |h| h.running("issue-2").is_some()).await;
        here.idle(1).await;
        let forge = here.forge();
        let started: Vec<&str> = forge.starts.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(started, ["issue-2"], "no build of the merged item");
        assert!(forge.merged_ins.is_empty(), "{:?}", forge.merged_ins);
    }

    /// 01M77K0EGH1HF0WA7WZH8BP2BP (#2019): a wave with a fake `gh` and the
    /// events of GitHub on. The verify of the head `c1` of pull request #41
    /// fails, and neither the event of its status nor its FAIL in the
    /// thread comes to the look: each look still reads the head with no
    /// status `riff/verify` and with only its first verify request. The
    /// wave starts no second verify of `c1`, and reads the status of the
    /// head only before the first claim. Then the FAIL comes, and the lead
    /// sends a new verify request for `c1`: the read before the claim finds
    /// the failure, the look shows the request after the FAIL, and the head
    /// gets its second verify.
    #[tokio::test]
    async fn a_look_with_no_status_after_a_fail_starts_no_second_verify_until_the_lead_asks() {
        let bin = tempfile::tempdir().unwrap();
        let put = |name: &str, value: serde_json::Value| {
            std::fs::write(bin.path().join(name), value.to_string()).unwrap();
        };
        // Pull request #41 waits for the verify of its head c1.
        let mut pull = pull_41();
        pull["statusCheckRollup"].as_array_mut().unwrap().remove(0);
        put("issues", serde_json::json!([open_issue(1)]));
        put("pulls", serde_json::json!([pull]));
        let (here, _told) = reading(bin.path()).await;
        let request = "verify request: issue-1, PR #41, branch worktree-issue-1, commit c1, from \
                       the riff daemon (session 65f510d7).";
        {
            let mut forge = here.forge();
            forge.fails = true;
            forge.thread.push(posted(DAEMON, request));
        }
        here.run();
        verified(&here, 1).await;
        item_reads(bin.path());
        // GitHub has the failure. No event of the status comes, and the
        // looks read the thread from before the result.
        std::fs::write(bin.path().join("status"), "failure\n").unwrap();
        here.idle(3).await;
        {
            let forge = here.forge();
            assert_eq!(forge.works, [verify("c1")], "one verify");
            assert_eq!(forge.status_reads, 1, "one read, before the first claim");
            assert!(forge.told.is_empty(), "{:?}", forge.told);
        }
        let reads = item_reads(bin.path());
        assert!(
            !reads.iter().any(|c| c.starts_with("pr list")),
            "the look reads the kept list: {reads:?}"
        );
        // The FAIL comes, and the lead asks for a new verify of c1. It
        // passes.
        {
            let mut forge = here.forge();
            forge.fails = false;
            forge.thread.push(posted(
                DAEMON,
                "verify result: FAIL for issue-1, PR #41, commit c1. PR comment https://c, \
                 riff/verify failure set.",
            ));
            forge.thread.push(posted(LEAD, request));
        }
        verified(&here, 2).await;
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.works, [verify("c1"), verify("c1")]);
        assert_eq!(forge.status_reads, 2);
        assert!(forge.told.is_empty(), "{:?}", forge.told);
    }

    /// The reply of GitHub to the ask for #41 ([`rollout::merge_off`]):
    /// the state `state`, out of the queue, at the head `c1`, and with
    /// auto-merge off.
    fn ask_41(state: &str) -> serde_json::Value {
        serde_json::json!({ "data": { "repository": { "pullRequest": {
            "state": state,
            "isInMergeQueue": false,
            "headRefOid": "c1",
            "autoMergeRequest": null,
        } } } })
    }

    /// The asks for one pull request in the log of [`READ_GH`] in `bin`.
    fn merge_asks(bin: &Path) -> usize {
        std::fs::read_to_string(bin.join("gh.log"))
            .unwrap_or_default()
            .lines()
            .filter(|c| c.contains("pullRequest(number"))
            .count()
    }

    /// 01M777N148MY8NR5RNDKXE9EVJ (#2003): a wave with a fake forge. The
    /// list of the open pull requests has #41 with a passed verify and no
    /// auto-merge, as GitHub gives it in the merge queue, and the read of
    /// the queue after it names no #41: the queue merged it between the
    /// two reads. The wave asks GitHub again, GitHub says `MERGED`, and the
    /// lead gets no message. Then GitHub says that #41 is open with
    /// auto-merge off: the lead gets one message, and the next looks do
    /// not ask again.
    #[tokio::test]
    async fn a_merge_between_the_two_reads_of_the_look_tells_the_lead_nothing() {
        let bin = tempfile::tempdir().unwrap();
        let put = |name: &str, value: serde_json::Value| {
            isolated::write_whole(bin.path().join(name), value.to_string());
        };
        let mut pull = pull_41();
        pull["autoMergeRequest"] = serde_json::Value::Null;
        put("issues", serde_json::json!([open_issue(1)]));
        put("pulls", serde_json::json!([pull]));
        put("ask", ask_41("MERGED"));
        let (here, _told) = reading(bin.path()).await;
        here.run();
        here.until(0, |_| merge_asks(bin.path()) >= 2).await;
        assert!(here.forge().told.is_empty(), "{:?}", here.forge().told);
        let log = std::fs::read_to_string(bin.path().join("gh.log")).unwrap();
        let ask = log
            .lines()
            .find(|c| c.contains("pullRequest(number"))
            .unwrap();
        assert!(ask.contains("-f owner=o -f name=r -F number=41"), "{ask}");

        // Auto-merge is off now.
        put("ask", ask_41("OPEN"));
        here.until(0, |h| !h.forge().told.is_empty()).await;
        let asks = merge_asks(bin.path());
        here.idle(0).await;
        let forge = here.forge();
        assert_eq!(forge.told, [stuck("c1", rollout::Stuck::AutoMergeOff)]);
        assert_eq!(merge_asks(bin.path()), asks, "one ask after the message");
    }

    /// 01M74N3544C338R6BV06DQ1DH0: `riff daemon status` shows the line of
    /// the events of each repository with a wake: the age of the last wake,
    /// the time of the safety look, and the dropped events of the last day
    /// with the last one.
    #[tokio::test]
    async fn the_status_of_the_daemon_shows_the_dropped_events() {
        let here = Here::new(&[], 1, Arc::new(AtomicU64::new(64))).await;
        let now = crate::daemon::now_ms();
        let text = text::daemon_status(&here.daemon.status().await, now);
        assert!(!text.contains("GitHub events"), "no wake yet: {text}");

        let grace = Duration::from_secs(60);
        let told =
            hooks::Told::new("o/r").with_times(hooks::SAFETY_LOOK_EVERY, grace, Duration::ZERO);
        let told = here.daemon.set_told(told);
        told.add(&riff_core::wire::GitHubWake {
            id: u64::try_from(now - 120_000).unwrap(),
            repo: "o/r".parse().unwrap(),
            kinds: vec![riff_core::wire::GitHubEvent::CheckRun],
            number: Some(40),
            events: 20,
        });
        let change = |number| hooks::Change {
            number,
            pull: true,
            kind: riff_core::wire::GitHubEvent::CheckRun,
        };
        // A change waits for its wake for the grace: it is no dropped event
        // yet.
        told.found(vec![change(41)], now - 840_000);
        told.found(vec![change(42)], now - 10_000);
        let text = text::daemon_status(&here.daemon.status().await, now);
        let line = "GitHub events of o/r: the last wake 2m ago, the safety look each 30 minutes, 1 \
             dropped event in the last day, the last of pull request #41, check_run, 14m ago.\n";
        assert!(text.contains(line), "{text}");
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

    /// #1863: the daemon runs a wave only for a project, and only on a
    /// host that the project allows. A repository that is no project, and
    /// a project that allows only another host, start no session: the look
    /// reads nothing, and `riff daemon status` says why. A project that
    /// allows the host runs the wave.
    #[tokio::test]
    async fn a_wave_runs_only_for_a_project_that_allows_the_host() {
        let refused = [
            (vec![], text::WAVE_NO_PROJECT.to_owned()),
            (
                vec![project_of(&["pangolin"])],
                text::wave_host_not_allowed("thelio"),
            ),
        ];
        for (projects, word) in refused {
            let here = Here::new(&[1], 2, Arc::new(AtomicU64::new(64))).await;
            here.forge().projects = Some(projects);
            here.run();
            here.until(0, |h| h.daemon.waves_shown()[0].word == word)
                .await;
            here.idle(0).await;
            assert!(here.forge().starts.is_empty(), "{word}");
            assert_eq!(here.forge().branch_reads, 0, "{word}");
        }
        let here = Here::new(&[1], 2, Arc::new(AtomicU64::new(64))).await;
        here.forge().projects = Some(vec![project_of(&["pangolin", "thelio"])]);
        here.run();
        here.until(1, |h| h.forge().starts.len() == 1).await;
        assert_eq!(here.forge().starts[0].0, "issue-1");
    }

    /// #1863: a host that the project no longer allows starts no new
    /// session there. The session that runs goes on to its end and its
    /// steps after the end. When the project allows the host again, the
    /// wave goes on.
    #[tokio::test]
    async fn a_host_that_the_project_no_longer_allows_starts_no_new_session() {
        let here = Here::new(&[1, 2], 1, Arc::new(AtomicU64::new(64))).await;
        here.run();
        here.until(1, |h| h.forge().starts.len() == 1).await;
        here.forge().projects = Some(vec![project_of(&["pangolin"])]);
        let item = here.forge().starts[0].0.clone();
        here.end(&item).await;
        here.until(1, |h| h.forge().finished == [item.clone()])
            .await;
        let word = text::wave_host_not_allowed("thelio");
        here.until(1, |h| h.daemon.waves_shown()[0].word == word)
            .await;
        here.idle(1).await;
        assert_eq!(here.forge().starts.len(), 1, "no new session");
        here.forge().projects = None;
        here.until(1, |h| h.forge().starts.len() == 2).await;
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

    /// #1605, 01M6QBJHM5BQR59QX2K2BMZVYH: an issue with the label
    /// `question` gets no build, and no rebase of its pull request, as a
    /// held item. The verify of an open pull request goes on. The lead
    /// hears nothing about a missing line of such an issue. After the
    /// label goes, the issue is free work at the next look.
    #[tokio::test]
    async fn an_item_that_waits_for_an_answer_never_starts() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2, 3, 4, 5], 5, free).await;
        {
            let mut forge = here.forge();
            // Waits, no pull request.
            forge
                .labels
                .insert(1, vec!["bug".into(), "question".into()]);
            // Waits, and its pull request waits for a verify.
            forge.labels.insert(2, vec!["question".into()]);
            forge.pulls.insert(2, Pr::Asked);
            // Waits, and its pull request has a conflict: no rebase.
            forge.labels.insert(4, vec!["question".into()]);
            forge.pulls.insert(4, Pr::Passed);
            forge.conflicting.insert(4);
            // Waits, and its body has no line.
            forge.labels.insert(5, vec!["question".into()]);
            forge.bare.insert(5);
        }
        here.run();
        here.until(5, |h| {
            h.running("issue-3").is_some() && h.running("verify-issue-2").is_some()
        })
        .await;
        here.idle(5).await;
        {
            let forge = here.forge();
            let mut started: Vec<&str> = forge.starts.iter().map(|(i, _)| i.as_str()).collect();
            started.sort_unstable();
            assert_eq!(started, ["issue-3", "verify-issue-2"]);
            assert!(forge.told.is_empty(), "no message: {:?}", forge.told);
        }

        // The person answered: the lead removes the label.
        here.forge().labels.remove(&1);
        here.until(5, |h| h.running("issue-1").is_some()).await;
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

    /// 01M6XSSCTA3E1GJ8EC9646PP1R (#1754): the pull request of an item
    /// merges with `Refs #N`, so the issue stays open, with no comment
    /// `Merged in` yet. No session claims the item at the next look, and
    /// the wave adds the comment one time.
    #[tokio::test]
    async fn an_item_whose_pull_request_merged_with_refs_never_starts_again() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 2, free).await;
        here.run();
        for item in ["issue-1", "verify-issue-1"] {
            here.until(2, |h| h.running(item).is_some()).await;
            here.end(item).await;
        }
        here.until(2, |h| h.forge().finished.len() == 2).await;
        assert_eq!(here.forge().pulls.get(&1), Some(&Pr::Passed));

        // GitHub merges the pull request: it is no more open, and the issue
        // stays open with no comment.
        {
            let mut forge = here.forge();
            forge.pulls.remove(&1);
            let merge = rollout::Merge {
                pull: 41,
                commit: "1a2b3c4d".into(),
                closes: false,
            };
            forge.merges.insert(1, merge);
        }
        here.until(2, |h| !h.forge().merged_ins.is_empty()).await;
        here.idle(2).await;
        let forge = here.forge();
        let started: Vec<&str> = forge.starts.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(started, ["issue-1", "verify-issue-1"], "no new claim");
        assert_eq!(forge.merged_ins, [(1, 41, "1a2b3c4d".to_owned())]);
        assert_eq!(forge.comments[&1], [text::wave_merged(41, "1a2b3c4d")]);
    }

    /// 01M6XSSCTA3E1GJ8EC9646PP1R (#1754): a comment `Merged in` that
    /// fails comes at the next look. The item stays no free work in the
    /// meantime.
    #[tokio::test]
    async fn a_comment_merged_in_that_failed_comes_at_the_next_look() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 2, free).await;
        {
            let mut forge = here.forge();
            let merge = rollout::Merge {
                pull: 41,
                commit: "1a2b3c4d".into(),
                closes: false,
            };
            forge.merges.insert(1, merge);
            forge.merged_in_fails = 1;
        }
        here.run();
        here.until(2, |h| h.running("issue-2").is_some()).await;
        here.until(2, |h| !h.forge().merged_ins.is_empty()).await;
        here.idle(2).await;
        let forge = here.forge();
        let started: Vec<&str> = forge.starts.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(started, ["issue-2"], "no build of the merged item");
        assert_eq!(forge.merged_in_fails, 0);
        assert_eq!(forge.merged_ins, [(1, 41, "1a2b3c4d".to_owned())]);
    }

    /// The wave of the open items 1 and 2: each pull request waits for a
    /// verify and changes `files1` and `files2`.
    async fn two_verifies(files1: &[&str], files2: &[&str]) -> Here {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        {
            let mut forge = here.forge();
            for (issue, files) in [(1, files1), (2, files2)] {
                forge.pulls.insert(issue, Pr::Asked);
                let files = files.iter().map(|f| (*f).to_owned()).collect();
                forge.files.insert(issue, files);
            }
        }
        here
    }

    /// What `riff wave list` shows of the items 1 and 2 in verify, with the
    /// facts that the daemon of `here` sends.
    async fn listed(here: &Here) -> String {
        listed_with(here, &[1, 2], &[]).await
    }

    /// What `riff wave list` shows of the items `verify` in verify and the
    /// items `conflicts` with a conflict, with the facts that the daemon of
    /// `here` sends.
    async fn listed_with(here: &Here, verify: &[u64], conflicts: &[u64]) -> String {
        let facts = crate::daemon::facts::of_status(&here.daemon.status().await);
        let mut daemon: riff_core::wire::SessionInfo = serde_json::from_value(
            serde_json::json!({ "uri": "riff://mike@thelio/o/r?session=d1", "live": true }),
        )
        .unwrap();
        daemon.machine = Some(riff_core::wire::MachineInfo {
            numbers: facts.line(),
            age_secs: 5,
        });
        let issue = |number| riff_client::waves::Issue {
            number,
            title: format!("item {number}"),
            milestone: Some("Wave 1".into()),
            tracker: false,
            question: false,
        };
        let all: BTreeSet<u64> = verify.iter().chain(conflicts).copied().collect();
        let issues = riff_client::waves::Issues::of(all.iter().map(|n| issue(*n)).collect())
            .with_verify(all.iter().copied())
            .with_stuck(
                conflicts
                    .iter()
                    .map(|n| (*n, riff_client::waves::CONFLICT.to_owned())),
            );
        let items = crate::waves::Items::of("o/r", &issues, vec![daemon], &BTreeMap::new());
        let milestone = riff_client::waves::Milestone {
            number: 1,
            title: "Wave 1".into(),
            open_issues: all.len() as u64,
            closed_issues: 0,
        };
        text::waves_list(&[milestone], Some(&items))
    }

    /// #1710: two open pull requests that change the same file each wait
    /// for a verify. The wave starts one verify, and the second after the
    /// first merges. The pass of the first is not enough: it still waits
    /// for its merge, and a merge can give the second a conflict.
    #[tokio::test]
    async fn the_verifies_of_two_pull_requests_with_a_file_in_common_go_one_after_the_other() {
        let tui = "crates/riff/src/tui.rs";
        let here = two_verifies(&[tui], &[tui, "docs/src/waves.md"]).await;
        here.run();
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        here.idle(3).await;
        assert!(here.running("verify-issue-2").is_none(), "it waits");
        assert_eq!(here.daemon.order_waits("o/r"), BTreeMap::from([(2, 41)]));
        // The daemon sends the wait in its facts, and `riff wave list`
        // shows it.
        assert!(
            listed(&here).await.contains("  verify waits for #41: #2"),
            "{}",
            listed(&here).await
        );
        // The verify of 1 passes: its pull request waits for the merge.
        here.end("verify-issue-1").await;
        here.until(3, |h| h.forge().finished.len() == 1).await;
        here.idle(3).await;
        assert!(here.running("verify-issue-2").is_none(), "no merge yet");
        assert_eq!(here.daemon.order_waits("o/r"), BTreeMap::from([(2, 41)]));
        // The pull request of 1 merges: the verify of 2 starts.
        {
            let mut forge = here.forge();
            forge.pulls.remove(&1);
            forge.items.retain(|n| *n != 1);
        }
        here.until(3, |h| h.running("verify-issue-2").is_some())
            .await;
        assert!(here.daemon.order_waits("o/r").is_empty());
        let forge = here.forge();
        let items: Vec<String> = forge.works.iter().map(Work::item).collect();
        assert_eq!(items, ["verify-issue-1", "verify-issue-2"]);
    }

    /// #1710: two pull requests that change other files get their verifies
    /// side by side.
    #[tokio::test]
    async fn the_verifies_of_two_pull_requests_with_no_file_in_common_go_side_by_side() {
        let here = two_verifies(&["src/a.rs"], &["src/b.rs"]).await;
        here.run();
        here.until(3, |h| {
            h.running("verify-issue-1").is_some() && h.running("verify-issue-2").is_some()
        })
        .await;
        assert!(here.daemon.order_waits("o/r").is_empty());
    }

    /// #1710: the verify of a pull request that cannot merge by itself does
    /// not hold up the others: auto-merge is off on the pull request of 1,
    /// whose verify passed.
    #[tokio::test]
    async fn a_passed_pull_request_that_cannot_merge_holds_up_no_verify() {
        let here = two_verifies(&["src/a.rs"], &["src/a.rs"]).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.manual.insert(1);
        }
        here.run();
        here.until(3, |h| h.running("verify-issue-2").is_some())
            .await;
        assert!(here.daemon.order_waits("o/r").is_empty());
    }

    /// #1894: two pull requests change the same file, and a session of
    /// another host holds the item of the first, for example to fix its
    /// checks. The first does not go first: the verify of the second
    /// starts. The session ends: the verify of the first waits for the
    /// second.
    #[tokio::test]
    async fn a_session_of_the_item_of_a_pull_request_holds_up_no_verify() {
        let tui = "crates/riff/src/tui.rs";
        let here = two_verifies(&[tui], &[tui]).await;
        here.forge()
            .claims
            .insert("issue-1".into(), "65f510d7".into());
        here.run();
        here.until(3, |h| h.running("verify-issue-2").is_some())
            .await;
        assert!(here.running("verify-issue-1").is_none());
        assert!(here.daemon.order_waits("o/r").is_empty());
        // The session ends: the verify of 1 waits for the verify of 2.
        here.forge().claims.remove("issue-1");
        let after = BTreeMap::from([(1, 42)]);
        here.until(3, |h| h.daemon.order_waits("o/r") == after)
            .await;
        here.idle(3).await;
        assert!(here.running("verify-issue-1").is_none(), "it waits");
    }

    /// #1801: three pull requests change one file, and the verify of the
    /// first passed. Its merge gives the two others a conflict. Only the
    /// next of the order gets a rebase. The third keeps its conflict, and
    /// gets its rebase after the second merges.
    #[tokio::test]
    async fn the_rebases_of_pull_requests_with_a_file_in_common_go_one_after_the_other() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2, 3], 3, free).await;
        {
            let mut forge = here.forge();
            for issue in [1, 2, 3] {
                forge.pulls.insert(issue, Pr::Asked);
                forge
                    .files
                    .insert(issue, vec!["crates/riff/src/tui.rs".into()]);
            }
            forge.pulls.insert(1, Pr::Passed);
        }
        here.run();
        let first = BTreeMap::from([(2, 41), (3, 41)]);
        here.until(3, |h| h.daemon.order_waits("o/r") == first)
            .await;
        // 1 merges: 2 and 3 have a conflict.
        {
            let mut forge = here.forge();
            forge.pulls.remove(&1);
            forge.items.retain(|n| *n != 1);
            forge.conflicting.extend([2, 3]);
        }
        here.until(3, |h| h.running("issue-2").is_some()).await;
        here.idle(3).await;
        assert!(here.running("issue-3").is_none(), "3 keeps its conflict");
        assert_eq!(here.daemon.order_waits("o/r"), BTreeMap::from([(3, 42)]));
        let list = listed_with(&here, &[], &[2, 3]).await;
        assert!(list.contains("  rebase waits for #42: #3"), "{list}");
        // The rebase of 2 pushes a new head, and GitHub does not know yet
        // whether it has a conflict: 2 keeps its place.
        here.forge().unknown.insert(2);
        here.end("issue-2").await;
        here.until(3, |h| h.forge().finished == ["issue-2"]).await;
        here.idle(3).await;
        assert!(here.running("issue-3").is_none(), "2 keeps its place");
        // The verify of the new head of 2, and its pass: 3 still waits.
        here.forge().unknown.remove(&2);
        here.until(3, |h| h.running("verify-issue-2").is_some())
            .await;
        here.end("verify-issue-2").await;
        here.until(3, |h| h.forge().finished.len() == 2).await;
        here.idle(3).await;
        assert!(here.running("issue-3").is_none(), "2 waits for its merge");
        assert_eq!(here.daemon.order_waits("o/r"), BTreeMap::from([(3, 42)]));
        // 2 merges: the rebase of 3 starts.
        {
            let mut forge = here.forge();
            forge.pulls.remove(&2);
            forge.items.retain(|n| *n != 2);
        }
        here.until(3, |h| h.running("issue-3").is_some()).await;
        assert!(here.daemon.order_waits("o/r").is_empty());
        let rebase = |issue, commit: &str| Work::Rebase {
            issue,
            pull: 40 + issue,
            commit: commit.into(),
        };
        let verify = Work::Verify {
            issue: 2,
            pull: 42,
            commit: "c2r".into(),
        };
        assert_eq!(
            here.forge().works,
            [rebase(2, "c2"), verify, rebase(3, "c3")]
        );
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
            [text::wave_spent("issue-1", None, STARTS, &stopped())]
        );
    }

    fn verify(commit: &str) -> Work {
        Work::Verify {
            issue: 1,
            pull: 41,
            commit: commit.into(),
        }
    }

    /// The riff daemon in the thread of `o/r`: it posts as the person, a
    /// sender with no session ([`right_sender`]).
    const DAEMON: &str = "riff://ann@heron/o/r";

    /// The lead of `o/r` in its thread.
    const LEAD: &str = "riff://ann@heron/o/r?session=l1&lead=true";

    /// The message `body` from `from` in the thread of `o/r`.
    fn posted(from: &str, body: &str) -> Message {
        Message {
            seq: 0,
            from: from.parse().unwrap(),
            to: Vec::new(),
            body: body.into(),
            at_ms: 0,
            kind: Kind::Message,
            sig: None,
            payload: None,
        }
    }

    /// Waits for the verify of the item 1, and ends it: the `n`th end of a
    /// session in the fake forge.
    async fn verified(here: &Here, n: usize) {
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        here.end("verify-issue-1").await;
        here.until(3, |h| h.forge().finished.len() == n).await;
    }

    /// #1651: a pull request that gets 4 rebases in one run of the wave
    /// gets a verify for each head. Each verify reports a PASS, so none
    /// counts toward [`STARTS`], and the lead hears nothing.
    #[tokio::test]
    async fn a_pull_request_with_four_rebases_gets_a_verify_for_each_head() {
        let here = conflict(Pr::Passed).await;
        here.run();
        for n in 1..=4 {
            // The rebase pushes a new head, and the verify of that head
            // passes.
            here.until(3, |h| h.running("issue-1").is_some()).await;
            here.end("issue-1").await;
            verified(&here, 2 * n).await;
            // `main` moves: the new head has a conflict again.
            here.forge().conflicting.insert(1);
        }
        here.idle(3).await;
        let forge = here.forge();
        let heads = ["c1", "c1r", "c1rr", "c1rrr"];
        let works: Vec<Work> = heads
            .iter()
            .flat_map(|head| [rebase(head), verify(&format!("{head}r"))])
            .collect();
        assert_eq!(forge.works[..8], works[..], "{:?}", forge.works);
        assert!(forge.told.is_empty(), "{:?}", forge.told);
    }

    /// #1651: a verify whose PASS or FAIL riff reported does not count
    /// toward [`STARTS`]. Each verify of the head fails, and a person asks
    /// for the verify of the same head again 4 times: each ask gets its
    /// verify, and the lead hears nothing. A head that passed gets no new
    /// verify (#1960).
    #[tokio::test]
    async fn a_verify_with_a_result_does_not_count_toward_the_starts() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Asked);
            forge.fails = true;
            // The lead holds the build after a fail: only the verify starts.
            forge.holds.insert("issue-1".into());
        }
        here.run();
        for n in 1..=STARTS as usize + 1 {
            verified(&here, n).await;
            here.forge().pulls.insert(1, Pr::Reasked);
        }
        here.until(3, |h| h.forge().works.len() == STARTS as usize + 2)
            .await;
        let forge = here.forge();
        assert!(
            forge.works.iter().all(|w| *w == verify("c1")),
            "{:?}",
            forge.works
        );
        assert!(forge.told.is_empty(), "{:?}", forge.told);
    }

    /// 01M75R717PH1NCQJ338ASVYT63 (#1960): after a PASS, the look of the
    /// wave can show the head as waiting still, before the event of the
    /// status comes. The wave starts no second verify of that head, and it
    /// reads the status of GitHub only before the first claim. The look
    /// that sees the pass ends the wait, and a new head gets its verify.
    #[tokio::test]
    async fn a_head_whose_verify_passed_gets_no_second_verify() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Asked);
            // The look does not see the status of the verify yet.
            forge.shown.insert(1, Pr::Asked);
        }
        here.run();
        verified(&here, 1).await;
        here.idle(3).await;
        {
            let forge = here.forge();
            assert_eq!(forge.works, [verify("c1")], "one verify");
            assert_eq!(forge.status_reads, 1, "one read, before the first claim");
            assert!(forge.told.is_empty(), "{:?}", forge.told);
        }
        // The look sees the pass. Then a person pushes a new head.
        here.forge().shown.clear();
        here.idle(3).await;
        {
            let mut forge = here.forge();
            forge.heads.insert(1, "c1b".into());
            forge.pulls.insert(1, Pr::Asked);
        }
        verified(&here, 2).await;
        assert_eq!(here.forge().works, [verify("c1"), verify("c1b")]);
    }

    /// 01M75R717PH1NCQJ338ASVYT63 (#1960): a head with the status
    /// `riff/verify` success gets no verify session, also while the look
    /// shows it as waiting: for example a verify of another host passed
    /// it, and the event of the status did not come yet. The read before
    /// the claim finds the success. No session starts, the start does not
    /// count, the lead hears nothing, and the wave reads the status one
    /// time.
    #[tokio::test]
    async fn a_head_with_a_verify_success_gets_no_verify_session() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Passed);
            forge.shown.insert(1, Pr::Asked);
        }
        here.run();
        here.until(3, |h| h.forge().status_reads == 1).await;
        here.idle(3).await;
        let forge = here.forge();
        assert!(forge.starts.is_empty(), "no claim: {:?}", forge.starts);
        assert_eq!(forge.status_reads, 1);
        assert!(forge.told.is_empty(), "{:?}", forge.told);
        assert!(here.running("verify-issue-1").is_none());
    }

    /// 01M77K0EGH1HF0WA7WZH8BP2BP (#2019): after a FAIL, the look of the
    /// wave can show the head as waiting still, before the event of the
    /// status comes. The wave starts no second verify of that head, and it
    /// reads the status of GitHub only before the first claim. Then the
    /// lead sends a new verify request for the same head: it gets its
    /// second verify.
    #[tokio::test]
    async fn a_head_whose_verify_failed_gets_no_second_verify_until_the_lead_asks() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Asked);
            forge.fails = true;
            // The look does not see the status of the verify yet.
            forge.shown.insert(1, Pr::Asked);
        }
        here.run();
        verified(&here, 1).await;
        here.idle(3).await;
        {
            let forge = here.forge();
            assert_eq!(forge.works, [verify("c1")], "one verify");
            assert_eq!(forge.status_reads, 1, "one read, before the first claim");
            assert!(forge.told.is_empty(), "{:?}", forge.told);
        }
        // The lead asks for a new verify of c1, and it passes.
        {
            let mut forge = here.forge();
            forge.fails = false;
            forge.shown.insert(1, Pr::Reasked);
        }
        verified(&here, 2).await;
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.works, [verify("c1"), verify("c1")]);
        assert_eq!(forge.status_reads, 2);
        assert!(forge.told.is_empty(), "{:?}", forge.told);
    }

    /// 01M77K0EGH1HF0WA7WZH8BP2BP (#2019): a head with the status
    /// `riff/verify` failure gets no verify session while the look shows
    /// it as waiting with no new verify request: for example a verify of
    /// another host failed it, and the event of the status did not come
    /// yet. The read before the claim finds the failure. No session
    /// starts, the start does not count, the lead hears nothing, and the
    /// wave reads the status one time.
    #[tokio::test]
    async fn a_head_with_a_verify_failure_gets_no_verify_session() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Failed);
            forge.shown.insert(1, Pr::Asked);
        }
        here.run();
        here.until(3, |h| h.forge().status_reads == 1).await;
        here.idle(3).await;
        let forge = here.forge();
        assert!(forge.starts.is_empty(), "no claim: {:?}", forge.starts);
        assert_eq!(forge.status_reads, 1);
        assert!(forge.told.is_empty(), "{:?}", forge.told);
        assert!(here.running("verify-issue-1").is_none());
    }

    /// #1651: 3 verifies of one head that end with no result stop that
    /// head only, and the notice names it. A new head starts a new count.
    /// The notice is for the head of the pull request: the third verify
    /// of the head `c1` ends after a push of `c1b`, and the lead hears
    /// nothing of `c1`.
    #[tokio::test]
    async fn three_verifies_of_one_head_with_no_result_stop_that_head_only() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Asked);
            forge.no_result = true;
        }
        here.run();
        verified(&here, 1).await;
        verified(&here, 2).await;
        // The third verify of `c1` runs: a person pushes a new head.
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        here.forge().heads.insert(1, "c1b".into());
        here.end("verify-issue-1").await;
        here.until(3, |h| h.forge().finished.len() == 3).await;
        // The new head gets a verify, and no notice names `c1`.
        verified(&here, 4).await;
        assert!(here.forge().told.is_empty(), "{:?}", here.forge().told);
        verified(&here, 5).await;
        verified(&here, 6).await;
        let spent = text::wave_spent("verify-issue-1", Some("c1b"), STARTS, &stopped());
        here.until(3, |h| h.forge().told == [spent.clone()]).await;
        here.idle(3).await;
        assert_eq!(here.forge().works.len(), 6, "`c1b` is spent");
        // Another head: another count.
        here.forge().heads.insert(1, "c1c".into());
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        let forge = here.forge();
        let commits: Vec<&str> = forge.works.iter().filter_map(Work::commit).collect();
        assert_eq!(commits, ["c1", "c1", "c1", "c1b", "c1b", "c1b", "c1c"]);
        assert_eq!(forge.told, [spent], "one notice");
    }

    /// #1651: the key of the starts of a verify has its head commit, and
    /// the lead hears of a spent verify for the head that the pull request
    /// has now. A pull request that waits for no verify has no spent
    /// verify.
    #[test]
    fn a_verify_is_spent_for_the_head_of_its_pull_request() {
        let waiting = |commit: &str| Waiting {
            pull: 41,
            commit: commit.into(),
            gate_done: true,
            again: false,
        };
        let look = |commit: &str| Look {
            free: vec!["verify-issue-1".into(), "issue-2".into()],
            pulls: [(1, waiting(commit))].into(),
            ..Look::default()
        };
        let starts: BTreeMap<String, u32> =
            [(verify("c1").key(), STARTS), ("issue-2".to_owned(), STARTS)].into();
        // The old head is spent, the new head is not: the build is.
        let got: Vec<(String, Option<String>)> =
            super::spent(&look("c1b"), &BTreeSet::new(), &starts)
                .into_iter()
                .map(|s| (s.key, s.head))
                .collect();
        assert_eq!(got, [("issue-2".to_owned(), None)]);
        assert_eq!(
            pick(&look("c1b"), &BTreeSet::new(), &starts),
            Some(verify("c1b"))
        );
        // The head that started 3 times is spent, and it names the head.
        let got: Vec<(String, String, Option<String>)> =
            super::spent(&look("c1"), &BTreeSet::new(), &starts)
                .into_iter()
                .map(|s| (s.item, s.key, s.head))
                .collect();
        assert_eq!(
            got,
            [
                (
                    "verify-issue-1".to_owned(),
                    "verify-issue-1@c1".to_owned(),
                    Some("c1".to_owned())
                ),
                ("issue-2".to_owned(), "issue-2".to_owned(), None),
            ]
        );
        // The third session runs still: the item is not spent yet.
        let running: BTreeSet<String> = ["verify-issue-1".to_owned()].into();
        let items: Vec<String> = super::spent(&look("c1"), &running, &starts)
            .into_iter()
            .map(|s| s.item)
            .collect();
        assert_eq!(items, ["issue-2"]);
        // No pull request waits for a verify: no head, and no spent verify.
        let none = Look {
            pulls: BTreeMap::new(),
            ..look("c1")
        };
        let items: Vec<String> = super::spent(&none, &BTreeSet::new(), &starts)
            .into_iter()
            .map(|s| s.item)
            .collect();
        assert_eq!(items, ["issue-2"]);
    }

    /// #2027: a verify request or a verify result of the thread counts
    /// only from its right sender. A result from the author does not end
    /// a new request, a PASS from the author does not count over a FAIL
    /// (#2014), and a request from a session with no claim takes no place
    /// in the order.
    #[test]
    fn a_verify_body_from_a_wrong_sender_is_a_plain_message() {
        let post = |from: &str, body: &str| Message {
            seq: 0,
            from: from.parse().unwrap(),
            to: Vec::new(),
            body: body.into(),
            at_ms: 0,
            kind: Kind::Message,
            sig: None,
            payload: None,
        };
        let request = "verify request: issue-12, PR #40, branch worktree-issue-12, commit 1a2b3c4.";
        let result = |word: &str| {
            format!(
                "verify result: {word} for issue-12, PR #40, commit 1a2b3c4. PR comment https://c."
            )
        };
        let (fail, pass) = (result("FAIL"), result("PASS"));
        let author = "riff://ann@heron/o/r?session=a1&claim=issue-12#issue-12";
        let verifier = "riff://ann@heron/o/r?session=v1&claim=verify-issue-12#verify-issue-12";
        let of = |thread: &[Message]| {
            said(&counted(thread, "o/r"))
                .get(&(12, "1a2b3c4".into()))
                .copied()
        };
        let failed = [post(author, request), post(verifier, &fail)];
        assert_eq!(of(&failed), Some(rollout::Verify::Failed));
        let forged = [failed.to_vec(), vec![post(author, &pass)]].concat();
        assert_eq!(of(&forged), Some(rollout::Verify::Failed));
        let passed = [failed.to_vec(), vec![post(verifier, &pass)]].concat();
        assert_eq!(of(&passed), Some(rollout::Verify::Passed));
        let thread = [failed.to_vec(), vec![post(author, request)]].concat();
        assert_eq!(of(&thread), Some(rollout::Verify::Asked));
        let forged = [thread.to_vec(), vec![post(author, &fail)]].concat();
        assert_eq!(of(&forged), Some(rollout::Verify::Asked));
        let ended = [thread.to_vec(), vec![post(verifier, &fail)]].concat();
        assert_eq!(of(&ended), Some(rollout::Verify::Failed));
        let other = "verify request: issue-13, PR #41, branch worktree-issue-13, commit 5e6f7a8.";
        let first = [
            post("riff://ann@heron/o/r?session=n1", other),
            post(author, request),
        ];
        assert_eq!(requests(&counted(&first, "o/r")), HashMap::from([(12, 0)]));
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
        let spent = text::wave_spent("issue-1", None, STARTS, &reason);
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

    /// 01M6Z87A04EXTT1FFJSJ1EM6TB, 01M6Z87JTXW0Y99X08VHA2SSSR: riff-server
    /// gives a worker no token of a GitHub App. No container starts, no
    /// start counts, the lead hears why one time, and the wave starts no
    /// item for [`gh_token::NONE_AGAIN`]. Then it starts the items, when a
    /// worker gets a token.
    #[tokio::test]
    async fn a_worker_with_no_token_of_an_app_spends_no_start_and_the_lead_hears_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1, 2], 3, free).await;
        let why = text::gh_repair(
            "o/r",
            "riff-o is not installed on o/r: riff github install o",
        );
        here.forge().no_app = Some(why.clone());
        // The fake makes a pull request at the end of each build: not here.
        here.forge().no_pull = true;
        here.run();
        here.until(0, |h| h.daemon.waves_shown()[0].word == text::WAVE_NO_TOKEN)
            .await;
        here.idle(0).await;
        // The jobs that started before the first answer end too: one
        // for each item at most. Then no new claim while the wave waits.
        let claims = here.forge().starts.len();
        assert!((1..=2).contains(&claims), "{:?}", here.forge().starts);
        here.idle(0).await;
        {
            let forge = here.forge();
            assert_eq!(forge.starts.len(), claims, "{forge:?}");
            assert!(here.fake.calls().is_empty(), "{:?}", here.fake.calls());
            assert_eq!(forge.told, [text::wave_no_token("o/r", &why)]);
            assert_eq!(forge.told_keys, [None]);
        }
        assert_eq!(here.daemon.status().await.no_starts.len(), 1);
        // The wait ends, and a worker gets a token: the wave starts the
        // items, and the lead hears nothing more.
        here.forge().no_app = None;
        here.daemon.age_no_start("o/r", gh_token::NONE_AGAIN);
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.until(3, |h| h.running("issue-2").is_some()).await;
        assert_eq!(here.forge().told.len(), 1, "{:?}", here.forge().told);
        assert!(here.daemon.status().await.no_starts.is_empty());
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
            model: None,
            github: None,
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
    /// second `riff host join`, and the lead hears of it again when it is
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
        let spent = text::wave_spent("issue-1", None, STARTS, &stopped());
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

    /// 01M77JWXEGDDMB7TW97APGKW9W (#2014): the second verify of a head
    /// whose first verify failed passes. The look still reads the old
    /// failure of the head, and the thread has the PASS. The wave starts
    /// no build and no second verify of that head, and the lead hears
    /// nothing.
    #[tokio::test]
    async fn a_pass_after_a_failed_verify_of_the_head_starts_no_session() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().pulls.insert(1, Pr::Reasked);
        here.run();
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        // The status of the PASS does not reach the look yet.
        here.forge().shown.insert(1, Pr::Repassed);
        here.end("verify-issue-1").await;
        here.until(3, |h| h.forge().finished.len() == 1).await;
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.works, [verify("c1")]);
        assert!(forge.told.is_empty(), "{:?}", forge.told);
        assert!(here.running("issue-1").is_none());
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

    /// 01M6FZ141Y0J8G2AYX0G5HY1CX, 01M74MGTCR650HNX4E1THAAG69: a pull
    /// request with a passed verify that GitHub holds for an approving
    /// review (`REVIEW_REQUIRED`) never merges by itself. The
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

    /// The message to the lead about the failed run `knurl adapter`, which
    /// the ruleset of `main` does not require, at the head `commit` of the
    /// pull request #41.
    fn optional(commit: &str) -> String {
        let check = rollout::OtherCheck {
            name: "knurl adapter".into(),
            app: Some("github.com".into()),
            link: Some(KNURL_LINK.into()),
        };
        stuck(commit, rollout::Stuck::Optional(check))
    }

    /// 01M749P62T24VDPX20D40KRJEK: the run `knurl adapter` of GitHub
    /// Actions failed on the head, and the ruleset of `main` does not
    /// require it. Each required check passed. The wave starts the verify
    /// of the head, and no fix session. The lead gets one message for the
    /// run and the head.
    #[tokio::test]
    async fn a_failed_run_that_no_rule_requires_gets_its_verify_and_one_message_to_the_lead() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Asked);
            forge.failing_optional.insert(1);
        }
        here.run();
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        here.until(3, |h| !h.forge().told.is_empty()).await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1".into(),
        };
        let message = optional("c1");
        assert!(message.contains(KNURL_LINK), "{message}");
        {
            let forge = here.forge();
            assert_eq!(forge.works, [verify], "the verify, and no fix");
            assert_eq!(forge.told, std::slice::from_ref(&message));
        }
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.told, [message], "once");
        assert_eq!(forge.works.len(), 1, "{:?}", forge.works);
    }

    /// 01M749P62T24VDPX20D40KRJEK, 01M74MGTCR650HNX4E1THAAG69: a failed
    /// run that the ruleset requires (`Gate`) still gets the fix session
    /// and no verify of the head, also next to a failed run that no rule
    /// requires. The session gets the name of the required run only, and
    /// the lead hears of the other one.
    #[tokio::test]
    async fn a_failed_required_run_still_gets_the_fix_and_no_verify() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        {
            let mut forge = here.forge();
            forge.pulls.insert(1, Pr::Asked);
            forge.failing.insert(1);
            forge.failing_optional.insert(1);
            forge.no_push = true;
        }
        here.run();
        here.until(3, |h| h.running("issue-1").is_some()).await;
        here.end("issue-1").await;
        here.until(3, |h| h.forge().finished == ["issue-1"]).await;
        here.idle(3).await;
        let forge = here.forge();
        assert_eq!(forge.works, [checks("c1")], "no verify of c1");
        assert_eq!(forge.told, [optional("c1")]);
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

    /// 01M6QC6KV25VMRREXXFBJD7NEJ, #1593: `main` moves while the verify of
    /// the item runs, and the pull request gets a conflict. The wave
    /// starts no rebase while the verify runs: its result would be for
    /// the head that the rebase replaces. The rebase starts after the
    /// verify ends.
    #[tokio::test]
    async fn a_conflict_that_comes_while_a_verify_runs_gets_the_rebase_after_the_verify() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().pulls.insert(1, Pr::Asked);
        here.run();
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        here.forge().conflicting.insert(1);
        here.idle(3).await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1".into(),
        };
        assert_eq!(
            here.forge().works,
            std::slice::from_ref(&verify),
            "no rebase yet"
        );
        here.end("verify-issue-1").await;
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [verify, rebase("c1")]);
    }

    /// 01M6QC6KV25VMRREXXFBJD7NEJ, #1593: a check of the head fails while
    /// the verify of the item runs (a rerun, or a check that GitHub
    /// starts late). The wave starts no fix while the verify runs. The
    /// fix starts after the verify ends.
    #[tokio::test]
    async fn a_failed_check_that_comes_while_a_verify_runs_gets_the_fix_after_the_verify() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[1], 3, free).await;
        here.forge().pulls.insert(1, Pr::Asked);
        here.run();
        here.until(3, |h| h.running("verify-issue-1").is_some())
            .await;
        here.forge().failing.insert(1);
        here.idle(3).await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1".into(),
        };
        assert_eq!(
            here.forge().works,
            std::slice::from_ref(&verify),
            "no fix yet"
        );
        here.end("verify-issue-1").await;
        here.until(3, |h| h.running("issue-1").is_some()).await;
        assert_eq!(here.forge().works, [verify, checks("c1")]);
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

        /// Sets the file `name` of the fake forge in one step: the looks read
        /// it while the wave runs (01M779MKDQFY6982B7D533G8K2).
        fn put(&self, name: &str, text: &str) {
            isolated::write_whole(self.world.root.join("bin").join(name), text);
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

    /// #1816, 01M4ZV3B6M7PZRJ39XTXBZP3ZH: a held `verify-issue-1` starts no
    /// verify of a pull request whose required check passed, look after
    /// look. A free of the hold starts the verify at the next look. So a
    /// hold of the verify keeps a pull request from the merge while a
    /// person runs a check on a host.
    #[tokio::test]
    async fn a_held_verify_starts_no_verify_of_a_pull_request_whose_checks_passed_and_a_free_starts_it()
     {
        let here = Here::looking().await;
        here.put(
            "rules",
            r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[
                {"context":"checks / lint"},{"context":"riff/verify"}]}}]"#,
        );
        {
            let mut forge = here.forge();
            forge.checked = true;
            forge.pulls.insert(1, Pr::Asked);
            forge.holds.insert("verify-issue-1".into());
        }
        here.run();

        // The check of the head passed, and the wave looks again and again:
        // no verify starts.
        here.until(0, |h| !h.calls("check_name=checks%20%2F%20lint").is_empty())
            .await;
        here.looks_more(3).await;
        assert!(here.forge().starts.is_empty(), "{:?}", here.forge());

        // The lead frees the hold: the verify starts.
        here.forge().holds.remove("verify-issue-1");
        here.until(1, |h| h.running("verify-issue-1").is_some())
            .await;
        let verify = Work::Verify {
            issue: 1,
            pull: 41,
            commit: "c1".into(),
        };
        assert_eq!(here.forge().works, [verify]);
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

    /// `riff host leave` ends the look with the loop of the wave.
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
            again: false,
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

    /// 01M6QC6KV25VMRREXXFBJD7NEJ, #1593: a running verify of an item
    /// stops the rebase and the fix of the checks of that item, whether a
    /// job of the wave runs the verify or a session claims it. They start
    /// after the verify ends. The verify of another item stops neither,
    /// and the build of the item is no fix.
    #[test]
    fn the_pick_starts_no_fix_and_no_rebase_while_a_verify_of_the_item_runs() {
        let rebase = Work::Rebase {
            issue: 1,
            pull: 41,
            commit: "c1".into(),
        };
        let checks = Work::Checks {
            issue: 2,
            pull: 42,
            commit: "c2".into(),
            checks: vec!["Gate".into()],
        };
        let look = Look {
            free: vec!["issue-1".into(), "issue-2".into(), "issue-3".into()],
            fixes: Fixes {
                conflicts: [(
                    1,
                    Conflict {
                        pull: 41,
                        commit: "c1".into(),
                    },
                )]
                .into(),
                failed: [(
                    2,
                    Failed {
                        pull: 42,
                        commit: "c2".into(),
                        checks: vec!["Gate".into()],
                    },
                )]
                .into(),
                ..Fixes::default()
            },
            ..Look::default()
        };
        let none = BTreeSet::new();
        let starts = BTreeMap::new();
        assert_eq!(pick(&look, &none, &starts), Some(rebase.clone()));
        let verify = |n: u64| -> BTreeSet<String> { [format!("verify-issue-{n}")].into() };
        // The verify of 1 runs: the rebase of 1 waits, the fix of 2 goes.
        assert_eq!(pick(&look, &verify(1), &starts), Some(checks.clone()));
        // The verify of 2 runs too: the build of 3 goes.
        let both: BTreeSet<String> = verify(1).into_iter().chain(verify(2)).collect();
        assert_eq!(pick(&look, &both, &starts), Some(Work::Build { issue: 3 }));
        // A claim of the verify, from another host, stops the same.
        let claimed = Look {
            claims: both.clone(),
            ..look.clone()
        };
        assert_eq!(
            pick(&claimed, &none, &starts),
            Some(Work::Build { issue: 3 })
        );
        // The verify of 3 stops neither fix.
        assert_eq!(pick(&look, &verify(3), &starts), Some(rebase.clone()));
        // The verify ends: the rebase starts, then the fix.
        let after: BTreeMap<String, u32> = [(rebase.key(), 1)].into();
        assert_eq!(pick(&look, &none, &after), Some(checks));
    }

    /// 01M74EP20TDQEXW42MK0XE73QF, #1894: the pull requests of 1 and 2
    /// change `tui.rs`, and the verify request of 1 came first. A session
    /// holds `issue-1`, for example to fix its checks: 1 does not go
    /// first, and the verify of 2 is the pick. After the session, the
    /// verify of 1 waits while the verify of 2 runs.
    #[test]
    fn a_session_of_the_item_of_a_pull_request_does_not_make_it_go_first() {
        let lane = |issue: u64, asked| order::Lane {
            issue,
            pull: 40 + issue,
            files: ["crates/riff/src/tui.rs".to_owned()].into(),
            asked: Some(asked),
            ..order::Lane::default()
        };
        let waiting = |issue: u64| Waiting {
            pull: 40 + issue,
            commit: format!("c{issue}"),
            gate_done: true,
            again: false,
        };
        let verify = |issue: u64| Work::Verify {
            issue,
            pull: 40 + issue,
            commit: format!("c{issue}"),
        };
        let look = Look {
            free: vec!["verify-issue-1".into(), "verify-issue-2".into()],
            claims: ["issue-1".to_owned()].into(),
            pulls: [(1, waiting(1)), (2, waiting(2))].into(),
            lanes: [(1, lane(1, 0)), (2, lane(2, 1))].into(),
            ..Look::default()
        };
        let none = BTreeSet::new();
        let starts = BTreeMap::new();
        assert!(order(&look, &none, &starts).is_empty());
        assert_eq!(pick(&look, &none, &starts), Some(verify(2)));
        // A job of the wave holds the item: the same.
        let unclaimed = Look {
            claims: BTreeSet::new(),
            ..look
        };
        let job: BTreeSet<String> = ["issue-1".to_owned()].into();
        assert!(order(&unclaimed, &job, &starts).is_empty());
        assert_eq!(pick(&unclaimed, &job, &starts), Some(verify(2)));
        // The session ended, and the verify of 2 runs: 1 waits for #42.
        let verifying: BTreeSet<String> = ["verify-issue-2".to_owned()].into();
        assert_eq!(
            order(&unclaimed, &verifying, &starts),
            BTreeMap::from([(1, 42)])
        );
        assert_eq!(pick(&unclaimed, &verifying, &starts), None);
        // With no session, the request of 1 came first: 2 waits for #41.
        assert_eq!(order(&unclaimed, &none, &starts), BTreeMap::from([(2, 41)]));
        assert_eq!(pick(&unclaimed, &none, &starts), Some(verify(1)));
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
                model: None,
                github: None,
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
        record.job = Some(Job {
            wave: built.wave.clone(),
            work: Work::Verify {
                issue: 12,
                pull: 40,
                commit: "1a2b3c4d5e".into(),
            },
        });
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
            .unwrap()
            .lines;
        assert_eq!(lines, [text::wave_no_result("verify-issue-12", 40, true)]);
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
        assert_eq!(one.tag("head"), "1a2b3c4");
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
            shown.contains(
                "9f8e7d6c, for the head 1a2b3c4, ended: exit code 0, and no PASS or FAIL."
            ),
            "{shown}"
        );
        assert_eq!(
            crate::ends::why("issue-12", &records),
            text::session_why_none("issue-12")
        );
    }

    /// 01M75R71BCDCP882W75GSE52VT (#1960): a verify that ends with no PASS
    /// or FAIL after the merge of its pull request posts no "waits for a
    /// new verify": the lead gets a note that no verify waits. So does a
    /// verify whose report fails after the merge.
    #[tokio::test]
    async fn a_verify_with_no_result_after_the_merge_says_that_no_verify_waits() {
        let built = Built::new().await;
        let mut record = built.record.clone();
        record.live.item = "verify-issue-12".into();
        record.job = Some(Job {
            wave: built.wave.clone(),
            work: Work::Verify {
                issue: 12,
                pull: 40,
                commit: "1a2b3c4d5e".into(),
            },
        });
        // The `gh` is not in the folder of the session: the folder goes.
        let bin = tempfile::tempdir().unwrap();
        let gh = pr::Gh::at(isolated::write_program(
            bin.path().join("gh"),
            "#!/bin/sh\n\
             case \"$*\" in\n\
               'pr view 40 --json state --jq .state') echo MERGED ;;\n\
               *) echo 'HTTP 502' >&2; exit 1 ;;\n\
             esac\n",
        ));
        let ended = Ended {
            code: None,
            failed: true,
        };
        let verify = |record: Record| {
            let gh = gh.clone();
            let (api, wave) = (&built.api, &built.wave);
            async move {
                after_verify(gh, api, &record, wave, (40, "1a2b3c4"), ended)
                    .await
                    .unwrap()
            }
        };
        let done = verify(record.clone()).await;
        let note = text::wave_no_result("verify-issue-12", 40, false);
        assert_eq!(done.lines, std::slice::from_ref(&note));
        assert_eq!(done.verdict, None);
        // A PASS whose report fails.
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join(crate::daemon::EVENTS),
            "{\"type\":\"result\",\"result\":\"PASS\\n\\nIt works.\"}\n",
        )
        .unwrap();
        let done = verify(Record {
            folder: folder.path().to_owned(),
            ..record
        })
        .await;
        let [failed] = &done.lines[..] else {
            panic!("one line: {done:?}");
        };
        assert!(
            failed.starts_with("verify-issue-12: riff could not report"),
            "{failed}"
        );
        assert!(failed.ends_with("The pull request is not open, so no verify waits."));
        assert_eq!(done.verdict, None, "{done:?}");
        let thread = built.wave.thread().unwrap();
        let got: Vec<(Kind, String)> = built
            .api
            .read(&built.lead, &thread, true)
            .await
            .unwrap()
            .into_iter()
            .map(|c| (c.message.kind, c.message.body))
            .collect();
        assert_eq!(got, [(Kind::Note, note), (Kind::Note, failed.clone())]);
        assert!(
            got.iter()
                .all(|(_, body)| !body.contains("waits for a new verify")),
            "{got:?}"
        );
    }

    /// #1651: each verify session of an item that ended with no PASS or
    /// FAIL leaves its record, with its session and its head commit.
    /// `riff session why verify-issue-N` names each of them, and the
    /// output of the last.
    #[tokio::test]
    async fn session_why_names_each_verify_session_that_ended_with_no_result() {
        let built = Built::new().await;
        let ended = Ended {
            code: Some(0),
            failed: false,
        };
        let sessions = [
            ("1a2b3c4d-0000", "988cb7b1a2c3"),
            ("5e6f7a8b-0000", "988cb7b1a2c3"),
            ("9c0d1e2f-0000", "3dfbec5d4e5f"),
        ];
        for (n, (session, head)) in sessions.iter().enumerate() {
            let mut record = built.record.clone();
            record.live.name = "riff-verify-issue-12".into();
            record.live.item = "verify-issue-12".into();
            record.live.session = (*session).into();
            record.folder = built.dir.path().join(format!("verify-{n}"));
            record.job = Some(Job {
                wave: built.wave.clone(),
                work: Work::Verify {
                    issue: 12,
                    pull: 40,
                    commit: (*head).into(),
                },
            });
            std::fs::create_dir_all(&record.folder).unwrap();
            std::fs::write(
                record.folder.join(crate::daemon::EVENTS),
                format!("{{\"type\":\"result\",\"result\":\"I stopped {n}.\"}}\n"),
            )
            .unwrap();
            let gh = pr::Gh::at(isolated::write_program(
                built.dir.path().join("gh"),
                "#!/bin/sh\nexit 1\n",
            ));
            let done = after_verify(gh, &built.api, &record, &built.wave, (40, head), ended)
                .await
                .unwrap();
            assert!(!done.reported(), "no PASS or FAIL: the start counts");
        }
        let records = crate::ends::read(&built.api, &built.lead).await.unwrap();
        let heads: Vec<&str> = records.iter().map(|r| r.tag("head")).collect();
        assert_eq!(heads, ["988cb7b", "988cb7b", "3dfbec5"]);
        let no_result = "exit code 0, and no PASS or FAIL";
        assert_eq!(
            crate::ends::why("verify-issue-12", &records),
            format!(
                "verify-issue-12: 3 sessions ended with no PASS or FAIL.\n\
                 - 1a2b3c4d, for the head 988cb7b, ended: {no_result}.\n\
                 - 5e6f7a8b, for the head 988cb7b, ended: {no_result}.\n\
                 - 9c0d1e2f, for the head 3dfbec5, ended: {no_result}.\n\
                 The last one is 9c0d1e2f. Its last 1 line of output:\n\
                 {{\"type\":\"result\",\"result\":\"I stopped 2.\"}}"
            )
        );
    }

    /// #1651: a verify whose FAIL is on the pull request is no failed
    /// start: `Finish::reported()` is true, and the job takes the start back.
    /// A verify whose report fails, here because the head moved, counts.
    #[tokio::test]
    async fn a_verify_counts_as_a_start_only_when_riff_did_not_report_its_result() {
        let built = Built::new().await;
        let ended = Ended {
            code: Some(0),
            failed: false,
        };
        let after = async |n: usize, head: &str| {
            let mut record = built.record.clone();
            record.live.name = "riff-verify-issue-12".into();
            record.live.item = "verify-issue-12".into();
            record.folder = built.dir.path().join(format!("verify-{n}"));
            std::fs::create_dir_all(&record.folder).unwrap();
            std::fs::write(
                record.folder.join(crate::daemon::EVENTS),
                "{\"type\":\"result\",\"result\":\"FAIL\\n\\n1. It does not work.\"}\n",
            )
            .unwrap();
            let gh = limited_gh(built.dir.path(), 0, head);
            after_verify(gh, &built.api, &record, &built.wave, (40, "1a2b3c4"), ended)
                .await
                .unwrap()
        };
        let done = after(1, "1a2b3c4d5e").await;
        assert!(done.reported(), "{done:?}");
        assert!(
            done.lines[0].starts_with("Put FAIL for issue-12 on pull request #40"),
            "{done:?}"
        );
        let done = after(2, "ffffffffffff").await;
        assert!(!done.reported(), "{done:?}");
        assert!(
            done.lines[0].contains("riff could not report the verify"),
            "{done:?}"
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
    /// checks is `rollup`. The ruleset of `main` requires the `Gate`.
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
"api repos/o/r") echo '{{"default_branch":"main"}}' ;;
"api repos/o/r/rules/branches/main?per_page=100") echo '{GATE_RULES}' ;;
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

    /// 01M749P62T24VDPX20D40KRJEK, 01M74MGTCR650HNX4E1THAAG69: the session
    /// fixed the required runs only. A run of Actions that the ruleset does not require, and that
    /// still fails, is not its fix: the lead heard of it at the look, so
    /// the end of the session gives a note. A required run that still fails
    /// gives the message.
    #[tokio::test]
    async fn a_fix_of_checks_does_not_count_a_failed_run_that_no_rule_requires() {
        let riff = Built::new().await;
        let knurl = r#"{"name":"knurl adapter","conclusion":"FAILURE","workflowName":"knurl"}"#;
        let rollup = |gate: &str| {
            format!(
                r#"[{{"name":"Gate","conclusion":"{gate}","workflowName":"CI"}},{knurl},{{"context":"riff/verify","state":"SUCCESS"}}]"#
            )
        };
        let checks = ["Gate".to_owned()];
        let gh = gh_with_rollup(&riff, &rollup("SUCCESS"));
        let (lines, got) = riff
            .after(gh, Some(Fix::Checks("1a2b3c4d5e", &checks)))
            .await;
        assert_eq!(lines[1], text::run_lead_noted("issue-12"));
        let [(Kind::Note, body)] = &got[..] else {
            panic!("{got:?}");
        };
        assert!(!body.contains("knurl adapter"), "{body}");

        let riff = Built::new().await;
        let gh = gh_with_rollup(&riff, &rollup("FAILURE"));
        let (_, got) = riff
            .after(gh, Some(Fix::Checks("1a2b3c4d5e", &checks)))
            .await;
        let [(Kind::Message, body)] = &got[..] else {
            panic!("{got:?}");
        };
        assert!(body.contains("the check Gate still fails"), "{body}");
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
            forge: Arc::default(),
            gh: Arc::new(gh.clone().by("look")),
            plan: Mutex::new(None),
            github: Mutex::new(None),
            reads: Mutex::new(None),
        };
        let none = BTreeSet::new();
        look_with(&gh.clone().by("wave"), "o/r", none.clone(), none, &[]).unwrap();
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

    /// The fake world of the look at the issue 12 of `o/r`, in a new
    /// folder: the file `issues` has its labels ([`Asking::labels`]).
    struct Asking {
        dir: tempfile::TempDir,
        world: Fakes,
        wave: Wave,
        watch: look::Watch,
    }

    impl Asking {
        fn new() -> Asking {
            let dir = tempfile::tempdir().unwrap();
            let gh = pr::Gh::at(isolated::write_program(dir.path().join("gh"), FAKE_GH));
            let world = Fakes {
                root: dir.path().to_owned(),
                forge: Arc::default(),
                gh: Arc::new(gh),
                plan: Mutex::new(None),
                github: Mutex::new(None),
                reads: Mutex::new(None),
            };
            let wave = Wave {
                repo: "o/r".into(),
                main: dir.path().join("main"),
                server: "http://127.0.0.1:1".into(),
                host: "thelio".into(),
            };
            let asking = Asking {
                dir,
                world,
                wave,
                watch: look::Watch::default(),
            };
            asking.labels(&[]);
            asking
        }

        /// The issue 12 has these `labels` from now.
        fn labels(&self, labels: &[&str]) {
            let labels: Vec<String> = labels
                .iter()
                .map(|l| format!(r#"{{"name":"{l}"}}"#))
                .collect();
            let issue = format!(
                r#"[{{"number":12,"title":"Which design?","body":"Done when: x.","comments":[],"milestone":{{"title":"Wave 1"}},"labels":[{}]}}]"#,
                labels.join(",")
            );
            std::fs::write(self.dir.path().join("issues"), issue).unwrap();
        }

        /// One look of the daemon: true when it posted something.
        async fn look(&mut self) -> Result<bool> {
            look::look_wave(&self.world, &self.wave, &mut self.watch).await
        }

        /// The count of the lines of the look since the start: the posts,
        /// the notices and the facts of riff-bot.
        fn lines(&self) -> usize {
            let forge = lock(&self.world.forge);
            forge.posts.len() + forge.told.len() + forge.bots.len()
        }

        /// The issues of the asked rows of the Waves tab, from a read of
        /// the same forge, with the board of `o/r` open.
        fn asked_rows(&self) -> Vec<u64> {
            use crate::tui::waves::{ItemState, Row, Waves, rows};
            let issues = crate::top::titles_from_gh(&self.world.gh, "o/r").unwrap();
            let forge = crate::tui::forge::Forge {
                issues,
                ..Default::default()
            };
            let mut tab = Waves::new(Some("o/r".into()));
            tab.read(vec![("o/r".into(), Ok(forge))], 0);
            // o/r, the clone, is a project of the riff.
            let mut look = crate::tui::Look::default();
            look.who.projects = vec!["o/r".parse().unwrap()];
            for key in [crate::tui::Key::First, crate::tui::Key::Right] {
                let shown = rows(&tab, &look, 0);
                tab.on_key(key, &shown, (&look, 0), 10);
            }
            rows(&tab, &look, 0)
                .into_iter()
                .filter_map(|row| match row {
                    Row::Item { item, .. } if item.state == ItemState::Asked => Some(item.number),
                    _ => None,
                })
                .collect()
        }
    }

    /// 01M75QTMB42NME9Z2Y0XC0AN97 (#1917): the label `question` goes on an
    /// issue, and no line reaches the chat: the look sends no post, no
    /// notice and no fact of riff-bot, also at the next looks and when the
    /// label goes on again. It reads no event of the issue. The Waves tab
    /// still shows the issue in its questions.
    #[tokio::test]
    async fn the_label_question_gives_no_chat_line_and_the_waves_tab_shows_it() {
        let mut asking = Asking::new();
        assert!(!asking.look().await.unwrap(), "no label: a quiet look");
        assert!(asking.asked_rows().is_empty());

        asking.labels(&["bug", "question"]);
        for _ in 0..2 {
            assert!(!asking.look().await.unwrap(), "the label: a quiet look");
        }
        assert_eq!(asking.asked_rows(), [12]);

        // The lead removed the label after the answer, and added it later.
        asking.labels(&["bug"]);
        assert!(!asking.look().await.unwrap());
        asking.labels(&["question"]);
        assert!(!asking.look().await.unwrap());
        assert_eq!(asking.lines(), 0, "no line reaches the chat");
        let log = std::fs::read_to_string(asking.dir.path().join("gh.log")).unwrap();
        assert!(!log.contains("/events"), "{log}");
        assert_eq!(asking.asked_rows(), [12]);
    }

    /// 01M6GYN97HH70E4C1FGM6HA2B0, 01M6GYN92N3Z99GPXTYHDDWF4R (#1549): a wave
    /// with no session, 1 open pull request and 20 open issues reads the
    /// forge with 13 calls and 7 GraphQL points at its first tick: two of
    /// them are the conditional reads of the look (#1566), which cost no
    /// point, and one reads the last merged pull requests (#1754). The look
    /// of the daemon takes the reads of the loop of the wave. A second tick
    /// with no change makes 10 calls: it reads the default branch and the
    /// rules one time. A pull request is open, so the conditional reads
    /// save nothing here: see
    /// `an_idle_wave_with_no_pull_request_asks_github_first_and_reads_nothing`.
    /// A daemon that shares no read made 17 calls and 12 points in each
    /// tick. With a tick each 5 minutes the two waves of an idle host use
    /// 168 points an hour, under the target of 300.
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
        assert_eq!(first["wave"], (8, 6), "{first:?}");
        // The look of the daemon reads the rules and the Gate of `main`
        // only: the issues and the pull requests came from the wave.
        assert_eq!(first["look"], (3, 0), "{first:?}");
        assert_eq!(first["watches"], (2, 1), "{first:?}");
        assert_eq!(sum(&first), (13, 7));

        let second = tick(&forge, &gh, &calls, &mut watch).await;
        assert_eq!(sum(&second), (10, 7), "{second:?}");
        assert!(sum(&second).0 < sum(&first).0);

        // The same tick of a daemon that shares no read: each loop reads
        // the issues and the pull requests, and the default branch each time.
        let alone_calls = Arc::new(crate::ghcalls::Calls::default());
        let alone = pr::Gh::at(forge.path().join("gh")).with_calls(alone_calls.clone());
        let mut watch = look::Watch::default();
        let before = tick(&forge, &alone, &alone_calls, &mut watch).await;
        assert_eq!(sum(&before), (17, 12), "{before:?}");

        // The target of two idle waves: one tick each 5 minutes.
        let ticks = 3600 / IDLE_LOOK_EVERY.as_secs() as u32;
        let hour = 2 * ticks * sum(&second).1;
        assert_eq!(hour, 168);
        assert!(hour <= crate::ghcalls::IDLE_POINTS_AN_HOUR, "{hour}");
        // A daemon that shares no read and looks each minute.
        assert!(2 * 60 * sum(&before).1 > crate::ghcalls::IDLE_POINTS_AN_HOUR);
    }

    /// 01M6GYN92N3Z99GPXTYHDDWF4R, 01M6GYN97HH70E4C1FGM6HA2B0 (#1549): a wave
    /// with 8 sessions has 8 open pull requests, and a folder for each. The
    /// loop of the wave looks each minute, the look of the daemon takes its
    /// reads, the watch of the wave looks each 5 minutes, and the sweep of
    /// the folders makes no call: each branch has an open pull request in the
    /// reply of the wave. That is 372 GraphQL points an hour, under the
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
        assert_eq!((wave, look, watches), (6, 0, 1), "{second:?}");

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
        assert_eq!(hour, 372);
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
        assert_eq!(first["wave"], (8, 6), "{first:?}");
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
        assert_eq!(third["wave"], (7, 6), "{third:?}");
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

    /// A fake `gh` for the tests of #1872: `gh api rate_limit` says that
    /// the GraphQL counter is out, with a reset one second after each
    /// read. Each other call fails on the limit until `fails_until`.
    fn moving_gh(dir: &Path, fails_until: i64) -> pr::Gh {
        let script = format!(
            r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/limit.log"
if [ "$1 $2" = "api rate_limit" ]; then
  echo "{{\"resources\":{{\"graphql\":{{\"limit\":5000,\"remaining\":0,\"reset\":$(( $(date +%s) + 1 ))}}}}}}"
  exit 0
fi
if [ "$(date +%s)" -lt {fails_until} ]; then
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

    /// 01M74BJ5Y03F5DGT44N8EENFBG (#1872): the look fails at each try,
    /// and the counter that is out gives a later reset at each try. Each
    /// try starts a new wait of the same outage, and the lead hears one
    /// time, with the key of the first wait.
    #[tokio::test]
    async fn a_look_with_a_later_reset_at_each_try_tells_the_lead_once() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 3, free).await;
        let bin = here._root.path().join("bin");
        here.forge().limit_gh = Some(Arc::new(moving_gh(&bin, now_secs() + 6)));
        here.run();

        here.until(0, |h| !h.forge().told.is_empty()).await;
        let keys = here.forge().told_keys.clone();
        // Each try reads the counter again: three tries, three resets.
        let reads = |dir: &Path| {
            let log = limit_log(dir);
            log.iter().filter(|c| *c == "api rate_limit").count()
        };
        here.until(0, |_| reads(&bin) >= 3).await;
        here.until(0, |h| h.daemon.waves_shown()[0].word == text::WAVE_NO_WORK)
            .await;
        assert_eq!(here.forge().told.len(), 1, "{:?}", here.forge().told);
        assert_eq!(here.forge().told_keys, keys);
    }

    /// 01M74BJ5Y03F5DGT44N8EENFBG (#1872): the look fails again at each
    /// try, a minute apart, and each try gives a reset one minute later.
    /// The resets pass the half hour, where the hour of a key changes. The
    /// lead hears one time.
    #[tokio::test]
    async fn a_reset_one_minute_later_at_each_try_gives_the_lead_one_message() {
        let free = Arc::new(AtomicU64::new(64));
        let here = Here::new(&[], 3, free).await;
        // No try calls `gh`: each wait of the test is still to come.
        let program = here._root.path().join("no-gh");
        let calls = Arc::new(crate::ghcalls::Calls::default());
        let gh = pr::Gh::at(isolated::write_program(program, "#!/bin/sh\nexit 1\n"))
            .with_calls(calls.clone());
        here.forge().limit_gh = Some(Arc::new(gh));
        let wave = Wave {
            repo: "o/r".into(),
            main: "/src/r".into(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        };
        // The tries start at minute 20 of the next hour, so the wave sees
        // each wait.
        let hour = 3_600_000;
        let start = (crate::daemon::now_ms() / hour + 1) * hour + 20 * 60_000;
        let mut told = None;
        let mut keys = BTreeSet::new();
        for n in 0..20 {
            let at = start + n * 62_000;
            let wait = crate::ghcalls::Wait::until(crate::ghcalls::Resource::Graphql, at + 60_000);
            keys.insert(wait.key());
            assert!(calls.start_wait(wait, at), "the wait of try {n} starts");
            assert!(limit_pause(&*here.world, &wave, &mut told).await.is_some());
        }
        assert_eq!(keys.len(), 2, "the resets pass the half hour: {keys:?}");
        let forge = here.forge();
        let [body] = &forge.told[..] else {
            panic!("one message: {:?}", forge.told);
        };
        let first = crate::ghcalls::Wait::until(crate::ghcalls::Resource::Graphql, start + 60_000);
        assert_eq!(body, &text::wave_limit_waits("o/r", &first));
        assert_eq!(forge.told_keys, [Some(first.key())]);
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
        .unwrap()
        .lines;
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
            [
                "pr view 40 --json headRefOid,body,title,autoMergeRequest",
                "api rate_limit"
            ]
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
            .unwrap()
            .lines;
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
