# Backlog batch 3: #995 to #1168

Issue: #1888, part of #1885. Each issue was read with its comments on 2026-11-11, against `origin/main` at f789f08e. Each file and line below is of that commit. The lead applies the verdicts after the merge. This report changes no issue.

| Issue | Verdict |
|---|---|
| #995 | question |
| #996 | question |
| #999 | question |
| #1000 | rewrite |
| #1012 | rewrite |
| #1020 | question |
| #1060 | rewrite |
| #1091 | close |
| #1097 | rewrite |
| #1123 | close |
| #1131 | keep |
| #1137 | rewrite |
| #1149 | rewrite |
| #1155 | keep |
| #1168 | rewrite |

#995, #996 and #999 hang on one decision: whether riff keeps the graphical UIs. #1226 (open) asks Mike the same about the devices and the phone app.

## #995 Design: the web UI of riff: a Slint app in Rust, built to WebAssembly, on Cloud Run

Verdict: question

Why:
- The web UI comes after the phone app (`docs/src/riff-uis.md:292`, "The order", and `docs/src/riff-uis.md:307`).
- The phone app is not built. Its build items are open in the Backlog: #962 (`riff device`), #963 (the Slint app), #964, #965 and #977. `riff --help` has no `riff device` (`docs/src/riff-mobile.md:6`), and `riff device list` fails with "unrecognized subcommand 'device'".
- The core library is done: #997 and #998 are closed, and the models are in `crates/riff-client/src` (`sessions.rs`, `threads.rs`, `waves.rs`, `machines.rs`, `daemons.rs`). But `riff-client` does not build for WebAssembly (see #999).
- No question of the body has an answer. The book keeps the start of the web design: a Cloud Run service of its own, and a router (`docs/src/riff-uis.md:248`).
- The TUI is built: the tabs Sessions, Chat, Waves, Machines and Reports (`docs/src/riff-uis.md:155`). It has a narrow layout "for a phone with SSH" (`docs/src/riff-uis.md:215`).
- #1226 (open) asks Mike to keep, cut or merge the devices and the phone app. Its rule until it ends: no new process feature.
- The body names versions of riff, which the writing rules of CLAUDE.md forbid.

Questions for mike:
- Q1: Does the web UI stay in the plan? Proposal: close #995 as not planned. The TUI serves each terminal, also a phone with SSH. Decide the phone app first, in #1226. The part "The web UI" of `docs/src/riff-uis.md` stays as the start of a later design. A new design issue starts from it when a person asks for a browser.

## #996 Design: the desktop GUI of riff: a Slint app in Rust

Verdict: question

Why:
- The desktop GUI is the last UI, after the web UI (`docs/src/riff-uis.md:292`, `docs/src/riff-uis.md:307`). The web UI has no design yet (#995), and the phone app is not built (see #995).
- One question of the body ("Linux only, or also macOS and Windows?") has new facts. riff runs on a Mac now: the release builds `aarch64-apple-darwin` and `riff update` works on a Mac (PR #1836). The book has "Install the daemon on a Mac" (`docs/src/development.md:1198`). The milestone "Wave 57: the sessions on a Mac" is open.
- The TUI is `riff` with no command (`docs/src/riff-uis.md:142`), and the release has the binary `riff` for Linux and for a Mac with Apple silicon (PR #1836). So the TUI does the work of a desktop app in a terminal on both.
- The body names versions of riff.

Questions for mike:
- Q1: Does the desktop GUI stay in the plan? Proposal: close #996 as not planned. The TUI covers the desktop on Linux and on a Mac. A new design issue starts from `docs/src/riff-uis.md` when a person asks for a desktop app.

## #999 riff-client builds for Android and for WebAssembly, and the Gate checks both

Verdict: question

Why:
- Nothing of it is built. `crates/riff-client/src` has one `cfg(target_os = "linux")` (`crates/riff-client/src/link.rs:498`), and no `cfg` for Android or WebAssembly. The `justfile` has no recipe `client-targets`. No workflow in `.github/workflows` builds `aarch64-linux-android` or `wasm32-unknown-unknown`.
- The work is not small. `riff-client` takes `tokio` with the feature `full` (`Cargo.toml:23`), and `keyring-core`, `tempfile` and `gethostname` (`crates/riff-client/Cargo.toml`). The docs of tokio say that on `wasm32-unknown-unknown` only the features `sync`, `macros`, `io-util`, `rt` and `time` build. `full` fails.
- Its Needs are done: #997 and #998 are closed.
- Only the graphical UIs need the two targets (`docs/src/riff-uis.md:71`): the phone app (#963) needs Android, and the web UI (#995) needs WebAssembly.

Questions for mike:
- Q1: When does this item start? Proposal: keep #999 in the Backlog as the first build item of the phone app, with the line "Needs: the decision of #1226 on the phone app". Close it with #963 if the phone app goes.
- Q2: If #995 closes, does #999 keep WebAssembly? Proposal: no. The lead rewrites it to Android only: the job of the Gate and `just client-targets` for that one target.

## #1000 --format json for each command that prints data, from the models of riff-client

Verdict: rewrite

Why:
- The requirement 01M542168PYP1WH78D3D9DKEK6 (`docs/src/requirements.md:7819`) and the book (`docs/src/riff-uis.md:129`) say that each command that prints data takes `--format text` or `--format json`. The code does not hold it.
- One command has the flag now: `riff denials --format json` (`DenialsFormat` at `crates/riff/src/main.rs:1756`; the how-to "Get the records as JSON", `docs/src/how-it-works.md:2239`). So the line of the body "No `--format` flag exists" is no longer true, and `riff denials` is the pattern.
- The `--help` of each command of the list shows no `--format`: `riff who`, `riff whoami`, `riff top`, `riff read`, `riff tail`, `riff usage`, `riff audit`, `riff wave list`, `riff daemon status` and `riff server`.
- The models derive serde, for example `crates/riff-client/src/waves.rs:73`, `crates/riff-client/src/machines.rs:114` and `crates/riff-client/src/sessions.rs:85`.
- The body names a version of riff, and `riff workers`, which is gone (#887 is closed).

New title: Each command that prints data takes --format json, as riff denials does

New body:

````markdown
The requirement 01M542168PYP1WH78D3D9DKEK6 says that each command that prints data takes `--format text` (the default) or `--format json`. Today only `riff denials` does (`DenialsFormat` in `crates/riff/src/main.rs`). The models of the views are in `crates/riff-client/src` (#998), with serde.

## What

- These commands take `--format text` (the default) or `--format json`: `riff who`, `riff whoami`, `riff top`, `riff read`, `riff tail`, `riff usage`, `riff audit`, `riff wave list`, `riff daemon status` and `riff server`. Also each other command that prints data, when the work finds one.
- `text` prints the lines of today. `json` prints the serde model of `riff-client` for the data, as one JSON document.
- A command that follows a stream (`riff tail`, `riff top` while it runs) prints one JSON document on each line for each new item.
- `riff denials` and these commands share one type for the flag.
- An error stays text on stderr, with the exit code of today.

Done when:
- A test for each command above: `--format json` prints JSON that parses back into its model of `riff-client`, and `--format text` prints the same lines as with no flag.
- A test: `riff tail --format json` prints one JSON document on each line.
- `riff COMMAND --help` names `--format` for each command above.
- Docs: a new how-to "Print the data of a command as JSON" in `how-it-works.md`, with its own heading and a copyable `sh` block (for example `riff who --format json`), and one line in the how-to of each command above. The words agree with `--help`.
````

## #1012 A gh token that GitHub refuses makes riff read the token again at the next call

Verdict: rewrite

Why:
- The fault is still there. A `gh` with one token reads it again only after `TOKEN_KEEP`, 10 minutes (`crates/riff/src/pr.rs:393`; `Gh::token` at `crates/riff/src/pr.rs:809`). `Gh::run` reads no token again after a refusal (`crates/riff/src/pr.rs:855`).
- The callers changed. `crates/riff/src/mcp.rs` and `crates/riff/src/worktrees.rs` are gone (#887 is closed). The callers of `with_one_token` today: `riff top` (`crates/riff/src/main.rs:4271`), the TUI (`crates/riff/src/tui.rs:1980`) and the look of the wave in the riff daemon (`crates/riff/src/wave.rs:3900`). The daemon runs for days, so a refused token there fails each `gh` call of the look for up to 10 minutes.
- The how-to "Count the calls to the keyring" exists (`docs/src/development.md:5508`). It still names "a clean of the worktrees" and "the rollout and the look of the lead" (`docs/src/development.md:5519`, `docs/src/development.md:5526`). So does 01M54510SFK1Z9VPFYBYYSV10F (`docs/src/requirements.md:3853`).

New title: A gh token that GitHub refuses makes riff read the token again at the next call

New body:

````markdown
A `gh` with one token (`Gh::with_one_token` in `crates/riff/src/pr.rs`) reads the token with `gh auth token` again only after `TOKEN_KEEP`, 10 minutes. A call that GitHub refuses does not read it again. So when the person signs in to `gh` again, or GitHub revokes the token, each `gh` call of the process fails for up to 10 minutes. Found by the verify of #1003 (PR #1010, comment 6230763969, note 1).

## Callers

- `riff top` (`crates/riff/src/main.rs`), the TUI (`crates/riff/src/tui.rs`) and the look of the wave in the riff daemon (`crates/riff/src/wave.rs`). The daemon runs for days, so it gains the most.
- The fix is in `Gh` (`pr.rs`), so each caller gets it.

Done when:
- A `gh` call that GitHub refuses for its token (`Bad credentials`, HTTP 401) makes the next call read the token again with `gh auth token`. Each refusal gives at most one new read. riff does not run the refused call again.
- A test with a fake `gh`: the first call is refused with the old token, and the next call reads the token again and works with the new one. A control: with no new read, the test fails.
- A new requirement (`just req`) supersedes 01M54510SFK1Z9VPFYBYYSV10F in part: the callers of today, and the read after a refusal.
- Docs: the how-to "Count the calls to the keyring" in `development.md` names the callers of today, and says when riff reads the `gh` token again.
````

## #1020 Design: rules that a model cannot break: a review of the prompts, guards in code, adversarial tests and an audit

Verdict: question

Why:
- Code holds more of the rules of harm than on the day of the body:
  - Each session in a container acts on GitHub with a token of a GitHub App, with the rights of its role, and gets no token of the person (PR #1838; "Who acts on GitHub for each role", `docs/src/how-it-works.md:6243`). A worker cannot set `riff/verify` or push a tag `v*`. The lead App cannot change `.github/workflows` or approve a deploy. So point 7 of the body ("the real limit is the token") is done for GitHub.
  - The guard workflow puts back an edit of the session App to the body or the title of an issue, or to a comment of another account (`crates/riff/src/guard.rs:1`, 01M6WPV2HK96JH900RX9ZPYHJ3).
  - A session has no sign-in of riff-server, and no riff tool of a session claims or releases (`docs/src/how-it-works.md:191`).
  - The permission rules of each role are one table in code, with allow, deny and person rows (`crates/riff/src/permissions.rs:71`).
- Many sources of point 1 are gone: the plugin and its skills (`crates/riff/claude-plugin`), `mcp.rs` and `hook.rs`. So are `riff workers start` and `riff retire`: `riff --help` has neither (#887 is closed). The start texts are in `crates/riff/src/text.rs` now, for example `run_start` (`crates/riff/src/text.rs:6405`) and `lead_start` (`crates/riff/src/text.rs:11876`).
- One way "around the guard" of the body is still open: `lead_only` reads `RIFF_WORKER` from the environment (`crates/riff/src/main.rs:3228`). The writes of `riff cloud` use it (`crates/riff/src/main.rs:3109`).
- Not built: no table of rules with a guard for each, no red-team command, no isolated attack run. `riff audit` checks the rules of one wave (`riff audit --help`: `--wave` only), and reads no transcript.
- #1226 (open): no new process feature until the narrowing ends.

Questions for mike:
- Q1: Does the red team stay (points 3 to 6 and 10: a session that tries to break each rule, with three kinds of attack, at each release)? Proposal: no. Close #1020 as not planned. The container, the App tokens and the guard workflow hold the rules of harm on GitHub in code, and the verify reads each change.
- Q2: Is one review of the prompts of today still worth it (points 1 and 2)? Proposal: yes, as one small new issue: read the start texts in `crates/riff/src/text.rs` and the instructions of the lead, and file one issue with the label `guard` for each rule of harm that no code holds. `lead_only` and `RIFF_WORKER` are the first row. No red team, no new source of the audit.
- Q3: Does the audit read the transcripts of the sessions (point 9)? Proposal: no. It goes with #1020.

## #1060 Design: resource management of riff 3.x on a host (container caps, build jobs, a shared build service)

Verdict: rewrite

Why:
- The caps are as the body says: `SESSIONS` 3, `FLOOR_GB` 8, `CPUS` 8 and `MEMORY_GB` 8 (`crates/riff/src/room.rs:60`, `:63`, `:67`, `:71`). `riff daemon limit` sets them.
- Two facts changed. riff sets no `CARGO_BUILD_JOBS` (01M5SMW28KT5D5HPRH7KAZ8TVC; the test at `crates/riff/src/daemon.rs:6707`; the row "The CPU" of "What sessions share", `docs/src/how-it-works.md:197`). The pool of build jobs (`jobserver.rs`) and the workers in tmux are gone (#887 is closed).
- The daemon reads no pressure. No code in `crates/riff/src`, `crates/riff-client/src` or `crates/riff-server/src` reads `/proc/pressure`. The facts of a host have the load and the free memory only (`crates/riff-client/src/machines.rs:115`; "See the facts of each host", `docs/src/how-it-works.md:1202`).
- The test crate `isolated` reads `/proc/pressure` for the span of a test (`crates/isolated/src/time.rs:62`, 01M56QW660NSKW0HC5W518EZZQ).
- Mike decided on 2026-10-24 (in the body): the daemon watches its host and wakes the lead at a threshold, with PSI triggers and a slow poll. The lead picked the triggers on the same day. Questions 1, 2, 3 and 5 of the body got no answer, and the simple design does not need them. So the body is a build item now, not a design.

New title: The riff daemon watches the memory and IO pressure of its host, and wakes the lead

New body:

````markdown
Mike (riff chat, 2026-10-24): the daemon of each host watches its host, and wakes the lead when a value crosses a threshold. Better: a notice from the kernel with no polling, and a slow poll as a fallback. Today the daemon reads only the free memory and the load (`crates/riff-client/src/machines.rs`), and no pressure (PSI).

## The triggers (GEN;SET, the lead on the word of Mike, 2026-10-24)

- Window: 2 s, the smallest window that an unprivileged process may use.
- Memory: `some 200000 2000000` (200 ms of stall in 2 s), the default of systemd.
- IO: `full 200000 2000000`. "full" means that each task that is not idle is stalled.
- CPU: no trigger. A host that builds is always busy. The daemon only shows the CPU averages.
- At a notice, the daemon reads `avg10`. It wakes the lead at most one time in 5 minutes for each kind, and tells the lead again when the pressure is back under the threshold.
- The slow poll reads the three files each 30 s.

Facts (2026-10-24, pangolin): `/proc/pressure/cpu`, `io` and `memory` have the mode 0666. An unprivileged process may write a trigger with a window of 2 s or a multiple of it, up to 10 s. One open file holds one trigger. The crate `isolated` reads these files for the span of a test (`crates/isolated/src/time.rs`).

Done when:
- The daemon holds one trigger for memory and one for IO, and wakes the lead at a notice, with the kind, `avg10` and the host. It tells the lead again when the pressure is back under the threshold.
- With no PSI (no files, or a refused trigger), the daemon polls each 30 s, and says one time that it polls.
- The facts of the host have the `avg10` of CPU, memory and IO. `riff who`, `riff top` and the Machines tab show them.
- A test with a fake pressure folder: a value over the threshold wakes the lead one time in 5 minutes, and a value back under it tells the lead again. A control: with no wake, the test fails.
- New requirement statements (`just req`) for the triggers, the wake and the poll.
- Docs: "See the facts of each host" in `how-it-works.md` shows the pressure in the facts, and says when the lead gets a wake.
````

## #1091 top_once_ends_with_the_error_when_a_connection_fails fails on a busy runner: the error says no reply in 3 seconds

Verdict: close

Why:
- PR #1090 (merged 2026-10-18) fixed the test that failed. It accepts each error that names the server, `riff-server at URL`, with a comment that the last try can end with no reply in time (`crates/riff/tests/top_fault.rs:522`, the check at `crates/riff/tests/top_fault.rs:537`). The comment of 2026-10-18 on the issue says so.
- The other checks of a fault text in `top_fault.rs` accept each text that they can get:
  - `stays_open` accepts the text of a connection that fails and the text of no reply (`crates/riff/tests/top_fault.rs:369`).
  - `top_once_and_a_first_look_end_with_the_error` connects to port 9, where nothing listens (`crates/riff/tests/top_fault.rs:502`). A refused connect to a loopback address of a server that never replied ends the call at once (`crates/riff-client/src/link.rs:63`, 01M4A804683G1EXM53893VHW7S). So no wait of 3 seconds can end it.
  - `top_ends_on_a_fault_that_a_new_try_cannot_repair` checks the text of a server of another kind, `do not match`, not a fault of the connection (`crates/riff/tests/top_fault.rs:485`).
- `just stress -p riff --test all top_fault::` passed 50 of 50 runs on 2026-11-11, on `origin/main` at f789f08e, with `STRESS_LOAD=4` (4 busy loops on a host of 32 threads that other sessions share). The third line of "Done when" asks for this run. Its Docs line asks for no change: the error of `riff top` did not change.
- A search of the issues for `top_fault` finds no new failure. It finds only #1091 and #972. #972 (open) is about a budget equal to the refresh and a fault after midnight: another item.

## #1097 The rebase of the wave takes the branch of the issue and main, not the head and base branch of the pull request

Verdict: rewrite

Why:
- The head part is still true. The rebase session of the wave gets its clone on `branch_of(issue)`, `worktree-issue-N` (`crates/riff/src/wave.rs:2473`, used at `crates/riff/src/wave.rs:2918`). The session that fixes failed checks does the same (`crates/riff/src/wave.rs:2929`).
- The wave takes a pull request on another branch as the work of an issue: by its trailer `Issue: #N`, then by its branch, then by `Closes #N` (`Pull::issue`, `crates/riff/src/rollout.rs:1398`; its doc test maps `fix-12-x` with `Closes #12` to issue 12). So the rebase of such a pull request goes to `worktree-issue-12`, a branch that is not its head. When the session pushes nothing, the lead gets `wave_not_rebased` (`crates/riff/src/wave.rs:2614`).
- The base part changed. The wave reads the default branch of the repository from the forge, not the word `main` (`crates/riff/src/wave.rs:297`, #1549), and the start text names it (`text::wave_rebase_start`, `crates/riff/src/text.rs:10934`). No flow of riff opens a pull request to another branch: the release pull request goes to `main` (`open_pr`, `crates/riff/src/cycle.rs:2865`). So a rebase on `baseRefName` has no case today, and the rewrite drops it.
- Its Need #1082 is closed. #1096 is closed as not planned.

New title: The rebase and the fix of checks of the wave take the head branch of the pull request, not the branch of the issue

New body:

````markdown
The wave of the daemon starts a session to rebase a pull request with a conflict (`Work::Rebase`), or to fix its failed checks (`Work::Checks`). Each session gets a clone on `branch_of(issue)`, `worktree-issue-N` (`crates/riff/src/wave.rs`), not on the head branch of the pull request. The wave takes a pull request on another branch as the work of the issue, by its trailer `Issue: #N` or its line `Closes #N` (`Pull::issue` in `crates/riff/src/rollout.rs`). Its session then cannot push to the pull request, and the lead gets `wave_not_rebased`. No work is lost. Found by the verify of #1082 (PR #1095, comment 6250126241).

Done when:
- The rebase session and the checks session get a clone on the head branch of the pull request (`headRefName`), and their start texts name that branch.
- A test with a fake forge: a pull request of issue 207 on the branch `worktree-issue-207-book` gets a rebase session and a checks session on that branch. A control: with `branch_of`, the test fails.
- A new requirement statement (`just req`) says which branch each session takes.
- Docs: "Run the wave" in `development.md` says on which branch the wave rebases and fixes the checks.
````

## #1123 Design: when does an item belong in a queue that waits and blocks no wave?

Verdict: close

Why:
- Mike (2026-10-19, in the body): the queue did not work out, and "we don't have a good definition of when to use it". Its own Done when allows the end "this issue closes with 'no queue'". This is that end.
- The two cases that the body says fit have tools today:
  - An item that waits for another item: its line `Needs:` (`docs/src/waves.md:15`). The TUI marks such an item `waiting` (`docs/src/riff-uis.md:225`).
  - A wave whose open items wait: the lead moves them in one command, `riff wave move 71 72 --to 7`, or ends the wave with `riff wave end 6 --move-open-to-backlog` (`docs/src/waves.md:188`, `docs/src/waves.md:197`). In Wave 33 the lead needed `gh issue edit --milestone` by hand.
- Facts of the body that changed: `riff wave run` is gone, and `riff join` gives the workers of a host (`crates/riff/tests/help.rs:125`, #1643). `riff plan hold ITEM REASON...` still has only a reason (`riff plan hold --help`).
- The case left is a timed wait of some hours, for example the VmLck reading of Wave 33. A hold with its reason and the lead serve it.
- #1104 and #1106, which go with it, are closed. #1226 (open): no new process feature until the narrowing ends.

## #1131 The requirement of devices still names riff remove --email

Verdict: keep

Why:
- The fault is still there: 01M53NTRGP2MJG6JT85BCVYE18 says "`riff logout --all` and `riff remove --email` also end the sign-in of each device" (`docs/src/requirements.md:4496`, the words at `docs/src/requirements.md:4501`).
- `riff remove --help` shows `Usage: riff remove [OPTIONS] <EMAIL>`, with no flag `--email`. `riff logout --help` has `--all`.
- `git grep -- '--email' docs/src` finds one live line, this one. Each other hit is in `docs/src/requirements-history.md` (`:2444` to `:2450`, and `:2706`).
- The requirement of the new form, 01M588J2QAF288ZB3M9PBN71ZS, is in `docs/src/requirements-history.md:2448`.
- #1226 part 1 also lists #1131 as stale. The fix is one partial supersede, so it needs no wait for #1226. The body and its Done when hold as they are.

## #1137 The woken part of a read has no bound, but a read gives at most 50 messages

Verdict: rewrite

Why:
- The fault is still there. `State::page` takes at most `limit` messages for the page (`crates/riff-state/src/state.rs:2757`), but it takes each woken message after the page, with no bound (`crates/riff-state/src/state.rs:2766`).
- 01M3TBZBX140GJWCV5GZ73Q5Z5 still says "at most 50 messages" (`docs/src/requirements.md:1854`). 01M588J4XNV1TXEPMGTKRVY9VK still does not say that it changes it (`docs/src/requirements.md:2706`).
- The fix of the comment is done: the book now shows `More messages follow. Read the thread chat again. With all, set after to 177.` (`docs/src/how-it-works.md:4259`), since PR #1717. The rewrite drops it.
- The heading of the Docs line exists (`docs/src/how-it-works.md:4245`).

New title: The woken part of a read has no bound, but a read gives at most 50 messages

New body:

````markdown
The part "Messages to you after this page" of an unread read (#1119, PR #1133) has no bound. `State::page` in `crates/riff-state/src/state.rs` takes each woken message after the page. The requirement 01M3TBZBX140GJWCV5GZ73Q5Z5 says that a read gives at most 50 messages, and 01M588J4XNV1TXEPMGTKRVY9VK does not say that it changes it.

In a direct thread, each message wakes the other session, and a lead wakes for each message to `lead=true`. So with more than 50 unread messages there, one read gives the page and each later addressed message: more than 50 messages. Found by the verify of #1133 (https://github.com/como-technologies/riff/pull/1133#issuecomment-6254678903, note 1).

Done when:
- The part has a bound (GEN;SET default: the 50 newest messages), and the read says how many it left out. Or a new requirement supersedes 01M3TBZBX140GJWCV5GZ73Q5Z5 in part, and the book says that the part has no bound. The author picks one, and records it with `just req`.
- A unit test of `State::page` with more than 50 addressed messages after the page.
- Docs: "One read after a wake shows the message of the wake" in `how-it-works.md` says the bound.
````

## #1149 At a start, a gap in the log names a cause that can be wrong

Verdict: rewrite

Why:
- The fault is still there. At a start, riff-server logs `e.at_start()` (`crates/riff-server/src/main.rs:396`). For `StoreError::Pruned`, `at_start` gives the same text as the reply to a client (`crates/riff-state/src/store.rs:164`), and that text names the cause "the checkpoint rule removed the chunks before position N" (`crates/riff-state/src/store.rs:186`). A doc test asserts that the two texts are the same (`crates/riff-state/src/store.rs:65`).
- The test checks a part of the text now: `a_gap_in_the_log_stops_the_load` asserts "the read needs the log from position 1" and no "remove the old state" (`crates/riff-server/tests/saved.rs:212`, the checks at `crates/riff-server/tests/saved.rs:231`). It does not check that the text names no cause.
- 01M588N3CKQMSDQPVFYGTJQQ2T asks for the cause in each such error (`docs/src/requirements.md:1418`). So the fix needs a new requirement that supersedes it in part, for the start. The body does not say so.
- The book quotes no text of the start. `docs/src/development.md:5359` shows a text of `riff audit` after a prune, which is another text. "Go back to a position" (`docs/src/development.md:6254`) is the how-to for a log that does not load.

New title: At a start, a gap in the log says which chunks are missing, and names no cause

New body:

````markdown
At a start of riff-server, a gap in the log stops the load with `StoreError::Pruned`. The log line is `at_start()`, which gives the same text as the reply to a client: "... the checkpoint rule removed the chunks before position N" (`crates/riff-state/src/store.rs`). At a start, that cause can be wrong: the checkpoint rule keeps each chunk that a kept checkpoint needs. The text has no delete command, so it does no harm. The cause can mislead a person.

The lead asked for it in the fix of PR #1134 (https://github.com/como-technologies/riff/issues/1118#issuecomment-6254659346, the last line). The second verify of #1134 found that it is not done (https://github.com/como-technologies/riff/pull/1134#issuecomment-6255020733, "Note").

Done when:
- At a start, a gap in the log gives a text that names the oldest position and the position that the load needs, with no cause and no delete command. The reply to a client keeps its text.
- `a_gap_in_the_log_stops_the_load` in `crates/riff-server/tests/saved.rs` checks the whole text of the start. The doc test of `StoreError::at_start` shows the two texts.
- A new requirement (`just req`) for the text of the start supersedes 01M588N3CKQMSDQPVFYGTJQQ2T in part.
- Docs: "Go back to a position" in `development.md` shows the text of the start.
````

## #1155 riff claude-token: a file sign-in session can mark a new token refused, and Ctrl-C leaves the echo off

Verdict: keep

Why:
- Fact 1 is still true. `Owned::token_refused` checks only that a token is set (`crate::claude_token::held()`), not how the session signed in (`crates/riff/src/daemon.rs:2117`). Neither `Owned` (`crates/riff/src/daemon.rs:1919`) nor its `Record` (`crates/riff/src/daemon.rs:1865`) keeps the kind of sign-in. The start knows it: `ClaudeSignIn::File` or `ClaudeSignIn::Token` (`crates/riff/src/run.rs:284`). A worker can still start with the file: only the lead needs the token (`lead::needs_token`, `crates/riff/src/lead.rs:402`).
- Fact 2 is still true. `riff claude-token` turns the echo off with the guard `Quiet`, and turns it on again only in its `Drop` (`crates/riff/src/claude_token.rs:247`, `crates/riff/src/claude_token.rs:264`). Nothing in the file handles SIGINT.
- The how-to of the Docs line exists: "Give the container sessions a long-lived sign-in of claude" (`docs/src/development.md:1518`). The body and its Done when hold as they are.

## #1168 A test: each riff command in an sh block of the book exists in riff --help

Verdict: rewrite

Why:
- The gap is still there. No test reads each `sh` block of each page. `each_is_real` (`crates/riff/tests/book/mod.rs:73`, not line 64) runs `--help` only for the commands that the test of one page gives it: `start_a_riff.rs`, `join_a_riff.rs`, `first_run.rs`, `owner.rs`, `members.rs`, `usage.rs`, `github_apps.rs` and `release_cycle.rs`. The files `dev.rs` and `enable.rs` that the body names are gone.
- A scan of `origin/main` on 2026-11-11 ran each line of an `sh` block of `docs/src/*.md` that starts with `riff` or `riff-server`, with `--help` (a script that splits the words with their quotes): 458 lines, 8 fail.
  - A real fault: `riff blocked run riff owner --take` (`docs/src/how-it-works.md:5003`, in "Say that the lead waits for you") fails with "unexpected argument '--take' found", also with no `--help`.
  - A placeholder that is not a number: `riff-server log cut --after POSITION` (`docs/src/development.md:6271`, `:6272`, `:6283`, `:6284`).
  - Commands of a design that is not built: `riff device add pixel`, `riff device list` and `riff device remove pixel` (`docs/src/riff-mobile.md:90`, `:170`, `:171`).
- So the test is cheap, and it finds a real fault today. The body has no Docs line.

New title: A test: each riff command in an sh block of the book exists in riff --help

New body:

````markdown
The rule of CLAUDE.md, "run `riff --help` and `riff-server --help`, find each command in the book", is manual. `each_is_real` in `crates/riff/tests/book/mod.rs` runs `--help` only for the commands that the test of one page gives it. No test reads each `sh` block of each page. From #888 (closed as not planned).

A scan of `origin/main` on 2026-11-11 ran each line of an `sh` block of `docs/src/*.md` that starts with `riff` or `riff-server`, with `--help`: 458 lines, 8 fail.

- `riff blocked run riff owner --take` ("Say that the lead waits for you" in `how-it-works.md`) fails with "unexpected argument '--take' found".
- `riff-server log cut --after POSITION` ("Go back to a position" in `development.md`, 4 lines): clap refuses the placeholder as a number.
- `riff device add pixel`, `riff device list` and `riff device remove pixel` (`riff-mobile.md`): commands of a design that is not built.

Done when:
- One test reads each `sh` block of `docs/src/*.md`, takes each line that starts with `riff` or `riff-server`, and runs it with `--help`. It splits the words as `sh` does, with their quotes.
- A line with a command or a flag that does not exist fails the test, and names the page and the line.
- A marker allows a line that cannot run: a placeholder, or a command of a design that is not built.
- The test passes on `main`: fix the line of `riff blocked`, and fix or mark the other 7 lines.
- A new requirement statement (`just req`).
- Docs: "Check the book" in `development.md` says what the test checks, and shows the marker.
````
