# Groom the backlog, batch 4 of 8: #1169 to #1334

Issue: #1889. Part of #1885. Read on 2026-11-11 against `origin/main`
at f789f08e (the release v4.15.0). Each line number is on that commit.
This report changes no issue. The lead applies the verdicts after the
merge.

| Issue | Verdict |
|---|---|
| #1169 | rewrite |
| #1181 | rewrite |
| #1191 | question |
| #1204 | rewrite |
| #1206 | question |
| #1211 | close |
| #1214 | close |
| #1216 | question |
| #1218 | rewrite |
| #1223 | rewrite |
| #1226 | question |
| #1234 | close |
| #1302 | question |
| #1306 | question |
| #1334 | rewrite |

A new body is in a `~~~markdown` block. Copy the text in the block.

## #1169 Two requirements are stale: the phone over iroh if riff-server ends, and Wave C removes riff-server

Verdict: rewrite

Why:

- Half of it is done. 01M51R8FXQFXWHRD374577HZ3H ("Wave C removes
  riff-server") is no longer in `requirements.md`. It is at
  `docs/src/requirements-history.md:2689`, and its text ends
  "Superseded by the re-cut (01M53NGWN190T2MYB1EZAD36VB)" (`:2700`).
  PR #1432 moved it.
- 01M53NTS97JVY20GS4BRR3NDDP is still live, at
  `docs/src/requirements.md:4547`: "If a later release removes
  riff-server, the phone talks over iroh to a daemon of its user".
- The live rules of today say the other thing:
  - 01M53NGS8FGRNY12DDJA3P0GH2 (`requirements.md:4570`): "riff-server
    stays the server of the riff".
  - 01M53NTQXQTM29BT67E6KHZFCK (`requirements.md:4441`): "The phone
    reaches the chat through riff-server over HTTPS."
  - 01M53NGW3FR555TWP8Z8HHRVDJ (`requirements.md:4665`): "The daemon
    binds no iroh endpoint".
  - 01M53NGWN190T2MYB1EZAD36VB (`requirements.md:4692`): "A new design
    brings them back when Mike decides that riff-server goes."
- `riff-3-no-cloud.md` is gone: PR #1403 deleted it.
- `docs/src/riff-mobile.md:26` "## How the phone reaches the chat" is
  right. `riff-mobile.md:292` "## If a later release removes
  riff-server" still has the path over iroh.
- The phone issues #958, #962, #963, #964, #965 and #977 are open.
  None of them has the words of iroh or Wave C.
- A new requirement now comes from `just req` (`justfile:104`), not
  `just rid`, and the old one moves by "## Move a superseded
  requirement" (`docs/src/development.md:4685`).

New title: The phone requirement 01M53NTS97JVY20GS4BRR3NDDP and riff-mobile.md still plan a phone over iroh with no riff-server

New body:

~~~markdown
riff-server stays, and riff has no iroh (01M53NGS8FGRNY12DDJA3P0GH2, 01M53NGW3FR555TWP8Z8HHRVDJ). The phone talks to riff-server (01M53NTQXQTM29BT67E6KHZFCK). But 01M53NTS97JVY20GS4BRR3NDDP (`docs/src/requirements.md:4547`) and the part "If a later release removes riff-server" of `riff-mobile.md` still plan a phone over iroh. 01M53NGWN190T2MYB1EZAD36VB already says that a new design comes when Mike decides that riff-server goes. The other half of the old issue is done: PR #1432 moved 01M51R8FXQFXWHRD374577HZ3H to the history page.

Done when:
- A new requirement from `just req` in "The phone app" says that the phone talks only to riff-server, and that a new design chooses the path of the phone if riff-server goes. It supersedes 01M53NTS97JVY20GS4BRR3NDDP.
- 01M53NTS97JVY20GS4BRR3NDDP is in `requirements-history.md` under "## The phone app", with the same ID and text. `just reqs` passes.
- `riff-mobile.md` has no part "## If a later release removes riff-server".
- Docs: `riff-mobile.md`, "## How the phone reaches the chat", which already says it.
~~~

## #1181 Design: build the same git tree once: reuse the CI build for riff build, real-session, the verify and the tag

Verdict: rewrite

Why:

- #1664 and PR #1678 (merged 2026-11-06), "Build once for each tree",
  did most of steps 1 and 2. #1664 does not name #1181.
  - The job `Build` keeps `tests-TREE` and `static-PROFILE-TREE`
    (`.github/workflows/ci.yml:130-144`).
  - A push to main, a merge group and a tag take the artifacts of
    their tree (`ci.yml:44-51`). When a passed `Build` made them,
    `Build` compiles nothing (01M6QQ1P37XW7N465MQGPHCXZM,
    `requirements.md:4393`).
  - `just real-session` takes the static riff of CI with
    `deploy/static/fetch.sh` (`justfile:246-251`,
    01M6QQ1PDNMHQ47FG8TD85P7CN at `requirements.md:5026`). Book:
    `development.md:4414` "### Take the static riff of CI".
- Two claims of the body are stale. A push now keeps the files of its
  tree. The clean release build moved to the pull request with the
  label `release` (01M513W2V4G94R7F4D4Q3M2RA1 at `requirements.md:4833`,
  `development.md:236-240`).
- One part of step 1 is left: a push to main runs the test parts and
  the job `macOS` again. Both jobs have `if: github.event_name !=
  'schedule' && github.ref_type != 'tag'` (`ci.yml:156`, `ci.yml:187`).
  The book says "The tests run again" (`development.md:242-243`).
- The measure, on the tree 96e60551 of f789f08e (from `gh run view
  --json jobs`):
  - The pull request run 42591780102: Build 1,449 s, macOS 761 s,
    Tests 1/4 to 4/4 680 s.
  - The push run 42596088074: Build 8 s, but macOS 702 s and Tests
    1/4 to 4/4 671 s again.
- `riff build` still compiles with `just static`
  (`crates/riff/src/check.rs:75-93`, `crates/riff/src/main.rs:568`).
  The rewrite of #1204 below takes that part.
- Steps 3 and 4 are not done. `crates/riff/src/sccache.rs:28`: "The
  cache is on the machine only: no cloud store, no network."
  `sccache.rs:10-12` says that a shared `CARGO_TARGET_DIR` is not the
  way. #1060 (open) is the design of the build resources of a host.

New title: A push to main runs no test part and no macOS job again when a run on the same tree passed

New body:

~~~markdown
"Build once for each tree" (#1664, PR #1678) made a push to main compile nothing on Linux. But it still runs the test parts and the job macOS again. The push run 42596088074 (tree 96e60551) took Build 8 s, macOS 702 s and Tests 1/4 to 4/4 671 s, after the pull request run 42591780102 on the same tree had passed. The jobs have `if: github.event_name != 'schedule' && github.ref_type != 'tag'` (`.github/workflows/ci.yml:156`, `:187`).

Done when:
- On a push to `main`, when a run of a pull request or a merge group on the same tree passed its Gate, the jobs `Tests I/N` and `macOS` do not run, and `Gate` passes on that fact. `Stage` and `Deploy` run as today.
- A tree with no passed run runs each job as today.
- A test of the plan shows both cases, as the tests of `deploy/build-plan.sh` do.
- A new requirement from `just req` says it.
- The lead puts steps 3 and 4 of the old body (a shared sccache store, a shared target folder) in a comment on #1060.
- Docs: `development.md`, "### Build once for each tree": the line "The tests run again" says when they do not.
~~~

## #1191 Design: riff uses llm-wiki of taps as its knowledge base for requirements, decisions, plans, waves and the notes of the lead

Verdict: question

Why:

- riff has no llm-wiki. `git grep -i` for `llm-wiki`, `wiki`, `taps`,
  `adroit` and `knowledge base` in `crates/`, `docs/src`, `CLAUDE.md`
  and `justfile` finds nothing. No requirement names a knowledge base.
  No spike result is on the issue.
- The two "first users" of the comment of chat 471 are built in
  riff-server, with no knowledge base:
  - The notes of the lead: #1164 closed (PR #1190), after this issue
    was made. Code: `crates/riff-state/src/state/notes.rs:1-11`.
    Requirement 01M59258K3TYR09HC4AWHWZPEX (`requirements.md:8437`).
    Book: `how-it-works.md:5167` "## The notes of the lead".
  - The end reasons of sessions: #1203 closed (PR #1205). Book:
    `development.md:3058` "### See why a session ended with no pull
    request".
- The decision of chat 471, "all sessions need access to a shared
  knowledge base", is not met. Only the lead has the tool `notes`
  (`crates/riff/src/tools.rs:125-128`). A container session has
  blocked, post, read, status, tell, threads, who and whoami
  (`tools.rs:119-121`).
- The other links are done: #1068 (`riff daemon mcp`, `development.md:2080`
  "### Choose the MCP servers of the container sessions") and #1189
  (`just req`, `justfile:104`).
- #1216 bars a new process feature for now, and #1226 lists this
  design as a keep or cut question. #659 (batch 1) overlaps on the
  memories of a lead.

Questions for mike:

- Q1. Do you still want llm-wiki, now that the notes of the lead and
  the end reasons live in riff-server? Proposal: close this issue as
  not planned. Open a new issue when a fact shows that the notes cannot
  hold what a lead needs.
- Q2. Does the decision of chat 471 still hold: each session reads a
  shared knowledge base? Proposal: GEN;SET, a worker and a verifier
  get a read of the notes of their lead with a new tool, and no write.
  That is one small issue. If not, record with `just req` that only
  the lead reads and writes the notes.

## #1204 riff build in the lead container: the host daemon does the build

Verdict: rewrite

Why:

- The first fault of the body is fixed. The lead container has
  `RIFF_CHECK=$HOME/.riff-check`, because `~/.cache` can belong to
  root (`crates/riff/src/lead.rs:239-245`; PR #1185;
  01M59269HHZ5C82DR3RG3QHAM3 at `requirements.md:7299`).
- The second fault still holds. `riff build` runs `just static`
  (`crates/riff/src/check.rs:75-93`), and `deploy/static/build.sh:40`
  stops: "the static build needs podman or docker". The base image has
  "no language, no build tool" (`deploy/base/Dockerfile:2-4`).
- The shape of this issue was refused. PR #1207, the pull request of
  this issue, closed with no merge: its verify said that the daemon ran the build itself,
  and its hold said that the shape needs the design #1206 first.
- Later, PR #1250, for issue 1239, merged `riff build --host HOST`
  (01M5GXYC201EFKMGA3ZNJM2J7Z at `requirements.md:6337`). The daemon of
  HOST runs `check::build` as a host process
  (`crates/riff/src/daemon/remote.rs:1150-1161`), and the path that it
  gives is on the host. Book: `development.md:758` "### Build riff on
  another host". #1206 asks if that may stay (see there, Q1).
- The need of the body is gone: the lead no longer needs the owner to
  build riff for a release.
  - The daemon starts the lead again on the new riff after an update
    (`how-it-works.md:1924` "### The lead starts again on the new
    riff").
  - The lead cut and published v4.15.0: PR #1848 is by
    `app/riff-como-technologies-lead`.
- The book is wrong for a container. `development.md:7297-7299`: "The
  lead in a container builds the riff of main with `riff build` in its
  own folder `RIFF_CHECK`". `development.md:7333-7340` runs
  `~/.cache/riff/check/target/static/release/riff release publish`, a
  path on the host.
- CI keeps `static-PROFILE-TREE` for each tree of main (see #1181), and
  `deploy/static/fetch.sh` already takes it for `just real-session`.

New title: riff build takes the static riff of CI, so it works in the lead container

New body:

~~~markdown
In a container, `riff build` runs `just static`, which needs podman or docker, and fails (`deploy/static/build.sh:40`). The book says that the lead in a container builds with `riff build` (`development.md:7297-7299`). CI already keeps `static-PROFILE-TREE` for each tree of main, and `deploy/static/fetch.sh` takes it for `just real-session`. The old shape, a daemon that builds for the lead, was refused in PR #1207.

Done when:
- `riff build` takes the static `riff` of the tree of the default branch from CI into the check clone, as `deploy/static/fetch.sh` does, and prints its path. It runs `just static` only when CI has no file for that tree and podman or docker is there.
- In a container with no file from CI, `riff build` says in one line that a container cannot build, and names `riff build --host HOST`.
- A test with a fake `gh` checks the path of the file, and a test checks the line in a container.
- 01M59269HHZ5C82DR3RG3QHAM3 is superseded in part by a new requirement from `just req`.
- Docs: `development.md`, "### Build riff for a check", and "#### Publish the release" with the real path of `RIFF_CHECK`.
~~~

## #1206 Design: how riff works end to end, and where session work runs (local, VPS, cloud host)

Verdict: question

Why:

- The book has no page "How riff works end to end". `SUMMARY.md` has
  none, and `grep -i "end to end"` in `docs/src` finds only text about
  encryption.
- Most parts and flows are in `how-it-works.md` with mermaid diagrams:
  - `:3` "## Parts"
  - `:48` "### The picture"
  - `:102` "### How a session works in a container"
  - `:2547` "## Verify finished work"
  - `:5167` "## The notes of the lead"
  - `:5218` "## Move the lead to another host"

  Deploy is in `development.md:6453` "## Deploy".
- The book names no VPS and no cloud host for sessions:
  `grep -i "vps\|cloud host"` in `docs/src` finds nothing.
- "The daemon only starts containers" is not true today, and no
  requirement says it:
  - `riff build --host` runs the build on the host
    (`crates/riff/src/daemon/remote.rs:1150-1161`, PR #1250).
  - The real-session test runs on the host, because "podman does not
    run inside a container" (`crates/riff/src/daemon/release_test.rs:6-7`).
  - The nearest rule, 01M599XR3NMJD9RAJ6ZAMW6KG5
    (`requirements.md:4647-4652`), says "The daemon runs all work in
    containers" in a text about workers in tmux.
- Two lines of "### The picture" are stale. `how-it-works.md:70` has
  "push with your sign-in", and `:100` says "The session pushes its
  work with your sign-in of GitHub." Since PR #1746 and PR #1838 a
  session pushes with the token of an App of its role
  (01M6Z87A04EXTT1FFJSJ1EM6TB at `requirements.md:1064`).
- The linked issues: #666 closed as not planned, #1203 closed. #699
  and #764 (batch 1), #1191, #1204 and #1211 are open.

Questions for mike:

- Q1. May the daemon run the build of `riff build --host` and the
  real-session test as host processes? Proposal: yes, GEN;SET. Record
  with `just req`: "The daemon runs each item and each verify in a
  container. On the host it runs only its reads and writes on GitHub,
  the build of `riff build --host` and the real-session test."
- Q2. Do sessions on a VPS or a cloud host come now? Proposal: no.
  Open an issue when a person needs one.
- Q3. A new page, or the parts that exist? Proposal: no new page. Say
  each thing in one place. Add one table "### Where each step runs" to
  "## Sessions in containers" of `how-it-works.md`, and fix `:70` and
  `:100`.

If Mike takes the proposals, the new title is "The book says where
each step of riff runs: a container, the host daemon, riff-server or
CI", and the new body is:

~~~markdown
The book has the parts and the flows of riff in `how-it-works.md`, but it does not say where each step runs. The daemon runs two steps on the host: the build of `riff build --host` (`crates/riff/src/daemon/remote.rs:1150-1161`) and the real-session test (`crates/riff/src/daemon/release_test.rs:6-7`). "### The picture" still says that a session pushes with the sign-in of the person (`how-it-works.md:70`, `:100`).

Done when:
- A new requirement from `just req` names the steps that the daemon runs on the host.
- `how-it-works.md` has a table of each step (an item, a verify, a build of riff, the real-session test, the reads and writes on GitHub, the release tag, the deploy) and where it runs.
- "### The picture" says that a session pushes with the token of the App of its role.
- Docs: `how-it-works.md`, "## Sessions in containers", a new heading "### Where each step runs".
~~~

## #1211 Review the chicken-and-egg of the release: the riff that cuts the release is the riff that it ships

Verdict: close

Why:

- Each of the three faults of that night is fixed, or its cause is
  gone:
  - (a) An old riff could not run `riff release cut`. Now the daemon
    starts the lead again on the new riff after an update
    (`how-it-works.md:1924` "### The lead starts again on the new
    riff"), and `riff update --auto on` updates by itself
    (`crates/riff/src/main.rs:510-517`, and `start-a-riff.md:197`
    "## Update riff").
  - (b) `riff build` failed on `~/.cache`: fixed by `RIFF_CHECK` (PR
    #1185). It still fails on podman: the rewrite of #1204 takes it.
  - (c) The Gate failed on a hard-coded version rule: #1210 closed
    (PR #1215). PR #1435 removed each part for an older release, and
    `hygiene versions` fails on a version name
    (`crates/hygiene/src/versions.rs:1-11`).
- The lead now runs the whole release from its container. PR #1848
  (v4.15.0) is by `app/riff-como-technologies-lead`, and the lead
  posted "riff release check v4.15.0: passed" on #1847.
- The four points of the Done when:
  - (1) a riff of main with no podman: the rewrite of #1204;
  - (2) the installed riff updates with no owner step: done on the
    host, as in (a);
  - (3) a test of the new version before the cut: the Gate of the
    release pull request runs with the new version before the merge
    and the tag, and the class of fault is gone (c);
  - (4) the books of the lead: the stale lines are in the rewrite of
    #1204.
- The design page of #1206 does not exist, and #1206 is a question
  above.

## #1214 riff pr check: the lint of a pull request before the push (trailer, milestone, fmt)

Verdict: close

Why:

- riff makes the trailer and the milestone right before the pull
  request opens:
  - The start text of a worker gives the body frame and the
    milestone of the issue (`crates/riff/src/run.rs:1526-1530`,
    `crates/riff/src/text.rs:6437-6440`).
  - `riff pr open` (`crates/riff/src/main.rs:1211`) writes the
    trailers and runs the lints of Hygiene before `gh pr create`
    (`crates/riff/src/pr.rs:2040-2052`).
- Hygiene runs the same lints on each change of a pull request, also
  on an edit of the body and a change of the milestone
  (`.github/workflows/hygiene.yml:13`). Its rules are `issue-trailer`,
  `milestone-trailer`, `trailer-wrap` and `milestone`
  (`crates/hygiene/src/lib.rs:67-70`). A person runs them with `just
  hygiene pr N` (`development.md:4838` "## Check a pull request on
  GitHub").
- `just check` runs `fmt-check` before each push (`justfile:74`,
  `justfile:119-120`).
- The cost in "Why" is gone. A failed check gets a fix session, and
  the wave starts no verify of that head (01M6JEQEQ45YMNR1WTX2RRTQ5B
  at `requirements.md:6072`).
- A line in the start text that names `just fmt-check` breaks
  01M5RN89WH3PHV3BHMJ25T3SX4 (`requirements.md:7027`): the start texts
  "name no file as required and no command".

## #1216 Decision: keep building riff, not OpenRig; narrow riff and give it an outside project

Verdict: question

Why:

- v4.0.0 was published 2026-10-22 (`gh release view v4.0.0`). The four
  weeks of the plan end about 2026-11-19. The issue has no comment and
  no numbers yet.
- Step 1 is done: #1208 closed, with PR #1222.
- Step 2, "no new process features", did not hold. For example:
  #1475 (a release hold), #1684 (`riff lead stop`), #1709 (the tab
  Reports), #1842 and #1845 (the alerts and the release gate), and the
  open waves 55 (a proof of presence) and 58 (a council of reviewers).
- Step 3 happened in another way. strata (Brett) uses riff: 198
  merged pull requests in `como-technologies/strata` since 2026-10-22,
  and many riff issues come from the lead of brett, for example #1855,
  #1858 and #1859. The milestone "Wave 40: riff on other projects" is
  closed. #698 is closed (PR #1389). #313 is still open.
- The page "Use riff on your repository" does not exist. Its job is
  done by `start-a-riff.md:612` "## Add another repository to the
  riff".
- Step 4: `riff usage` gives the tokens (`development.md:5151` "## See
  the tokens of an issue").
- Step 5, the benchmark of OpenRig: no sign of it. "OpenRig" is only
  in #1216.
- Step 6: a reasoning effort for each session exists
  (`development.md:3718` "### Set the effort of the sessions"). No
  Codex verifier: `grep -i codex` in `crates/` and `docs/src` finds
  nothing.
- Step 7: Brett uses riff on strata each week.

Questions for mike:

- Q1. Is strata the outside project of the plan? Proposal: yes. On
  2026-11-19 the lead writes in a comment the merged pull requests of
  strata, the riff issues from the lead of brett, and the tokens for
  each merged pull request from `riff usage`.
- Q2. Drop the benchmark of OpenRig (step 5)? Proposal: drop it.
  Nobody ran it, and two people use riff each week.
- Q3. A Codex verifier (step 6)? Proposal: take it out of #1216. Open
  an issue only when you want a second vendor.
- Q4. Which row of the table of signals? Proposal: the lead proposes
  the row "riff carries the outside project ... Commit to riff" with
  the numbers of Q1. You pick, and the issue closes with that record.

## #1218 A person can paste an image into the lead container

Verdict: rewrite

Why:

- The lead container exists: `riff lead start`
  (`crates/riff/src/main.rs:1653-1660`) and the Sessions tab of the
  TUI (`start-a-riff.md:75` "## Detach from the lead").
- `riff view` is gone: PR #1701 removed it, and
  01M6VGHPP3ZT5TTMSGAXV3ZK7W (`requirements.md:8194`) says "riff has
  no command `riff view`". The body still names it.
- riff has no image support. `grep -i` for `clipboard`, `xclip`,
  `wl-paste` and `screenshot` in `crates/`, `deploy/` and `docs/src`
  finds nothing. The base image has "bash, CA certificates, git, curl,
  procps, gh and claude" (`deploy/base/Dockerfile:2-4`).
- The lead container binds its clone at the same path, with write
  (`crates/riff/src/run.rs:356-362`, used by
  `crates/riff/src/lead.rs:237`). The clone is `leads/OWNER-REPO/clone`
  in the data folder of riff (01M501P9Q51AYN9GBPWKVHA229 at
  `requirements.md:6238`). So a file there has the same path in the
  lead, and a path that the terminal pastes works. But an image in the
  clone can get into a commit, and the book does not say it.
- Nobody confirmed the fault with a paste. The issue has no comment.

New title: A person shows a screenshot to the lead: a folder that the lead container binds at the same path

New body:

~~~markdown
The lead container has no clipboard tool and no display (`deploy/base/Dockerfile:2-4`), so a paste of an image in the lead gives nothing. Keep the sandbox: the container does not get the display of the host. The lead binds its clone at the same path (`crates/riff/src/run.rs:356-362`), so a folder of the host that the lead binds at the same path has the same path in the lead. A path that the terminal pastes then works.

Done when:
- At each start of the lead, riff makes `leads/OWNER-REPO/images` in the data folder of riff, and binds it at the same path with write. A unit test of `lead::spec` shows the bind.
- A new requirement from `just req` records the folder.
- A person copies a PNG there, gives its path to the lead, and the lead tells what the image shows. The result is in a comment on this issue.
- Docs: `start-a-riff.md`, a new heading "## Show a screenshot to the lead" after "## Detach from the lead", with a copyable `sh` block that copies a file into the folder.
~~~

## #1223 The auto mode checker denies safe commands of the lead and of workers: 30 records in denials

Verdict: rewrite

Why:

- The lead half is moot. The lead runs with
  `--dangerously-skip-permissions` (`crates/riff/src/lead.rs:250`,
  #1202 closed). Workers and verifiers keep `--permission-mode auto`
  (`crates/riff/src/run.rs:409-410`).
- `permissions::session_rules` exists
  (`crates/riff/src/permissions.rs:579`). Its table has no allow rule
  for `just check`, `git checkout -B`, `git status`, `git diff`, `git
  commit`, `git push` or `cargo fmt`. `Bash(gh pr view *)` is allowed
  for each role (`permissions.rs:381`).
- `riff denials` exists (`main.rs:287`, 01M57XY44K2N3PS9F8NQ6SZQP9 at
  `requirements.md:2885`). Book: `how-it-works.md:2180` "## See the
  refusals of auto mode". Nobody counted the records again after
  2026-10-22.
- The note of the body is right, and the requirement is wrong.
  01M57XY405BSXEH3K3Z977RWAW (`requirements.md:2877`) says "no session
  sees the records of other sessions at its `read`". But a read of a
  thread checks only `may_read`
  (`crates/riff-state/src/state/threads.rs:265-267`,
  `crates/riff-state/src/state.rs:2745`), and with `all` it shows each
  message. `riff denials` reads with `all` (`crates/riff/src/denied.rs:827`).
- Stale text: 01M57XY3Q6HEYQG27FE0CEQ7HB (`requirements.md:2849`) says
  "The plugin has a `PermissionDenied` hook ... `riff hook denied`",
  and `:2868` says "`worker` (in tmux)". `denied.rs:6-8` says the
  same. riff has no plugin and no `riff hook` (#887, PR #1200).

New title: Auto mode: the steps of a worker that the checker refuses, and the text of who reads the thread denials

New body:

~~~markdown
The lead skips the permission checks since #1202. Workers and verifiers keep auto mode (`crates/riff/src/run.rs:409-410`). No session rule covers `just check`, `git checkout -B`, `git status`, `git diff`, `git commit`, `git push` or `cargo fmt` (`permissions::session_rules`). 01M57XY405BSXEH3K3Z977RWAW says that no session sees the records of other sessions, but a read of `denials` with `all` shows each record of the riff (`may_read` in `crates/riff-state/src/state/threads.rs`).

Done when:
- The lead runs `riff denials --since 21d`, and lists on this issue each group of records of a worker or a verifier, each marked fix or correct.
- Each fix is a rule of `permissions::session_rules` or a line of the `environment` of auto mode, and a test checks each one.
- A new requirement from `just req` supersedes 01M57XY405BSXEH3K3Z977RWAW in part, and says who can read the records.
- 01M57XY3Q6HEYQG27FE0CEQ7HB and the rustdoc of `denied.rs` name only `riff relay denied`: no plugin, no `riff hook denied`, no worker in tmux.
- In the next wave, `riff denials --since 7d` shows no new record of a fixed group.
- Docs: `how-it-works.md`, "## See the refusals of auto mode", says who reads the records. `start-a-riff.md`, "### The permission rules of Claude Code", lists each new rule.
~~~

## #1226 Narrow riff: cut the book, the requirements, the features nobody needs and the gates that only stall

Verdict: question

Why:

- riff grew, it did not narrow. Then (v4.0.0, bbf9227f) and now
  (f789f08e):
  - Rust: about 172,000 lines, now 283,856 (`git ls-files '*.rs' |
    xargs cat | wc -l`).
  - The book: 21,986 lines, now 32,018 (`docs/src/*.md`).
  - `development.md`: 4,675 lines, now 8,123. `how-it-works.md`:
    4,043 lines, now 6,428.
  - Open issues: 95, now 159.
- Part 1, the move of each superseded requirement, is done: PR #1432
  made `requirements-history.md` and the check `superseded-in-full`.
  `requirements.md` has 7 lines with "superseded" (594 then). How-to:
  `development.md:4685` "## Move a superseded requirement".
- Part 1, "at most half" of `development.md` and `how-it-works.md`, is
  not done. #631 (batch 1, open) is the design of a docs pass.
- The stale requirements of Part 1: #1169 is half done (above). #1131
  (batch 3) is open. #1167 closed on 2026-11-08.
- Part 2, the inventory: nothing. No comment, issue or pull request
  holds it. The commands that Part 2 names:
  - `riff cloud` exists (`main.rs:784-790`), book
    `start-a-team-riff.md:72` "## Host your own riff on Cloud Run".
  - `riff invite` (`main.rs:411`), `riff remove` (`main.rs:425`) and
    `riff members` (`main.rs:433`) exist, book `start-a-team-riff.md:363`
    "## Invite a person" and `:376` "## See the members".
  - `riff join` now means "Give workers to this repository from this
    host" (`main.rs:354`).
  - `riff device` does not exist. #962 is open.
- Part 3, the gates: nothing. A new workflow, `riff-guard.yml`, came
  after the issue. #1225 (the stall of a major release) closed.
- The rule "no new process feature" of #1216 did not hold (see #1216).

Questions for mike:

- Q1. Does the narrow of #1216 still hold, with the waves 54 to 60
  open? Proposal: you say yes or no. If no, close this issue as not
  planned.
- Q2. If yes, the new title is "Narrow riff: an inventory of the
  commands, the crates and the gates, with keep, cut or merge for
  each", and the new body is:

~~~markdown
On f789f08e, riff has 283,856 lines of Rust and 32,018 lines of book (`development.md` 8,123, `how-it-works.md` 6,428). The move of the superseded requirements is done (PR #1432). The book shrink is #631.

Done when:
- A comment on this issue lists each command of `riff --help` and `riff-server --help`, and each crate with its lines of code and its tests.
- The same comment lists each gate (Gate, Hygiene, riff guard, `riff/verify`, `riff/real-session`, the human approval, the rules of auto mode) with the real faults it caught and the stalls it caused in the last two weeks.
- Mike marks each line keep, cut or merge. Each cut is its own issue with a Done when and a Docs line.
- Docs: none for the inventory. Each cut removes its how-to, for example `start-a-team-riff.md` "## Host your own riff on Cloud Run" if `riff cloud` goes.
~~~

## #1234 Fallback: the lead on the host, if the lead in a container cannot run riff wave and riff release by itself

Verdict: close

Why:

- The Done when says: "If it does, this issue closes as not needed."
  The lead in a container does each step now:
  - #1228 closed (PR #1238): a riff command in a container reaches
    riff-server through the daemon (01M5GVPNAKAPA596K9361HSAAV at
    `requirements.md:6329`: "So `riff wave list` and `riff release
    check` work in the container of the lead").
  - #1239 closed (PR #1250): `riff release test --host` and `riff
    daemon status --host` go through the daemon.
  - #1247 closed (PR #1296): `riff release check` reads the Cloud Run
    log through the daemon, so the container needs no `gcloud`
    (`development.md:6899` "#### Check the release after the
    deploy").
- The lead in its container posted "riff release check v4.15.0:
  passed" on #1847, and the status `riff/real-session` of f789f08e
  ran on thelio.
- riff has no host lead: `riff lead` has only `start`, `stop`, `move`
  and `notes` (`main.rs:1653-1722`). `development.md:4020`: "The lead
  runs in a container."

## #1302 Exploration: a start digest of the history, the state and the messages for a new lead, with drill-down

Verdict: question

Why:

- riff has no digest for a lead. No requirement names one.
- The fault that started it is fixed. A new lead no longer starts with
  hundreds of old unread messages: it takes the read cursors of the
  lead before it (#1350, PR #1381; 01M5QVD3B1GT7STHBQS7NK4DR0 at
  `requirements.md:3216`). Book: `how-it-works.md:5389` "### What a new
  lead reads". The comment of 2026-10-24 on #1302 is from before that
  fix.
- A new lead reads the notes and the unread messages
  (`crates/riff/src/text.rs:11876`, `lead_start`). The notes are #1164
  (closed).
- A lead can already go into the past: `riff read --thread T --all`,
  the notes with `--history`, `riff session why ITEM`
  (`development.md:3058`) and `riff audit`. A thread keeps its last
  200 messages (`KEEP_MESSAGES`, `crates/riff-state/src/state.rs:328`).
- #1301 closed (PR #1303). #1191 is a question above.
- A digest is a new process feature, and #1216 bars those for now.

Questions for mike:

- Q1. With the cursors of the old lead (#1381) and the notes (#1164),
  does a new lead still miss things? Proposal: close as not planned.
  Open it again with a fact: a lead that missed a decision that was
  not in the notes.
- Q2. If you keep it, who makes the digest? Proposal: riff code, no
  model session. It joins facts that riff has: the waves, the holds,
  the pauses, the open `blocked` questions and the last end reasons.
  A lead tool gives it, at most 4 KiB. Docs: `how-it-works.md`, a new
  heading "### The digest at the start of a lead" under "## The lead".

## #1306 Design: riff-server knows that a person, not an agent, named the lead or approved a release

Verdict: question

Why:

- A later design took layer 1. #571 ("Design: a proof of presence")
  closed on 2026-11-06: WebAuthn, a challenge bound to the action, and
  a policy `off` or `required`. Its build items are open: #1657 and
  #1658 (Wave 55) and #1659 (Backlog). None of them names #1306.
- The list of actions of #1658 has `riff release approve` and the
  change of owners and admins. It has no "name the lead". #1306 has
  Mike's word of 2026-10-23: "The actions of layer 1: naming the lead,
  `riff release approve`, and changing owners or admins."
- #571 puts "a short certificate for the lead" (layer 2) under
  "Later, not in this design". Layer 3 (a key for each session) is not
  built: the only key is the device key
  (`crates/riff-core/src/dpop.rs`).
- Fact 4 of the body is stale. `lead_if_first` is gone, and #1305
  closed (PR #1308). Only a person names the lead
  (01M5NJAXTQ0YKN7Q7XN0A7ZMPF at `requirements.md:3002`), and no
  session becomes the lead by itself (01M5NJAY3FTR005TDJJY1TMGNX at
  `requirements.md:3007`).
- The gap of fact 5 still holds: riff takes a call as the person when
  `RIFF_SESSION` and `CLAUDE_CODE_SESSION_ID` are not set
  (`crates/riff/src/identity.rs:53`).
- No session has the GitHub token of the person now (PR #1838,
  01M6Z87A04EXTT1FFJSJ1EM6TB at `requirements.md:1064`).

Questions for mike:

- Q1. Does the naming of the lead need a proof of presence, as you said
  on 2026-10-23? Proposal: yes. The lead adds `riff lead start` and
  `riff lead --session ID` to the list of actions of #1658 with a
  comment.
- Q2. Then close this issue as done by the design #571? Proposal: yes. Layer 1 is
  #1657 and #1658, layer 2 is "later" in #571, and layer 3 does not
  hold against an agent with the keyring, as the order of 2026-10-23
  says.

## #1334 The WIP check of a pull request head matches each headline that starts with WIP, for example "WIPE cache"

Verdict: rewrite

Why:

- The fault holds. `rollout::wip` is `message.starts_with("WIP")`
  (`crates/riff/src/rollout.rs:905-907`). So "WIPE the cache" is WIP,
  and the daemon sends no verify request for that head
  (`rollout.rs:2165`).
- A second check is wider. `dropped::pushed` marks a branch as WIP
  when its subject has `WIP` at any place:
  `subject.contains("WIP")` (`crates/riff/src/dropped.rs:461`). So "Add
  WIP support" is "a WIP commit" in the note of earlier work
  (`dropped.rs:135`).
- The rule of today says "contains". 01M3WFYEKTWVVZ1FWVNQMGBNN0
  (`requirements.md:2373-2376`): "A WIP commit has `WIP` in its
  subject." The book says the same (`how-it-works.md:2323`, under
  "### A branch with WIP commits").
- riff writes only the form `WIP: ...`: `leave::WIP_MESSAGE` is "WIP:
  the session left the riff" (`crates/riff/src/leave.rs:54`), and the
  save step commits "WIP: STEP" (`crates/riff/src/permissions.rs:1878`).

New title: A WIP commit is one whose subject is WIP, or starts with "WIP:" or "WIP ": "WIPE the cache" and "Add WIP support" are no WIP

New body:

~~~markdown
`rollout::wip` takes a head as WIP when its subject `starts_with("WIP")` (`crates/riff/src/rollout.rs:905-907`), so "WIPE the cache" gets no verify request. `dropped::pushed` takes a branch as WIP when its subject `contains("WIP")` (`crates/riff/src/dropped.rs:461`), so "Add WIP support" is "a WIP commit" in the note of earlier work. The rule 01M3WFYEKTWVVZ1FWVNQMGBNN0 and the book say "has `WIP` in its subject". riff itself writes only "WIP: ..." (`leave::WIP_MESSAGE`).

Done when:
- One function decides: the subject is "WIP", or starts with "WIP:" or "WIP ". `rollout::wip` and `dropped::pushed` both use it.
- A doc test and a unit test take "WIP", "WIP: the riff is paused" and "WIP the riff is paused" as WIP, and "WIPE the cache", "Add WIP support" and "Add it\n\nWIP: one" as no WIP.
- A new requirement from `just req` says the form, and supersedes 01M3WFYEKTWVVZ1FWVNQMGBNN0 in part.
- Docs: `how-it-works.md`, "### A branch with WIP commits", names the form of the subject, and "## Pause the riff" names it for a WIP head.
~~~
