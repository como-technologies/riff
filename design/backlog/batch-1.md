# Backlog batch 1: #502 to #828

Issue: #1886, part of #1885. Each verdict checks one issue against
`origin/main` at f789f08e (#1848): the code, the book (`docs/src/`),
the requirements, and the closed issues and merged pull requests. Each
`path:line` is at that commit.

The lead applies the verdicts after the merge. This file changes no
issue. A new body is in a `markdown` block, ready to copy.

| Issue | Verdict |
|---|---|
| #502 | rewrite |
| #561 | rewrite |
| #576 | rewrite |
| #585 | rewrite |
| #593 | rewrite |
| #631 | question |
| #639 | question |
| #643 | rewrite |
| #659 | question |
| #665 | close |
| #699 | close |
| #763 | rewrite |
| #764 | rewrite |
| #765 | rewrite |
| #828 | rewrite |

Count: close 2, rewrite 10, question 3, keep 0.

## #502 riff audit checks that no worker claimed a held item, and who held and freed it

Verdict: rewrite

Why:

- The need holds. `riff audit` still checks only the rules 1 to 7: the table at `crates/riff/src/audit.rs:34`, `RULES: [&str; 7]` at `crates/riff/src/audit.rs:207`, and 01M3ZWRC6H0P7EF6CMECCYZ2RC (`docs/src/requirements.md:230`, "It checks seven rules"). The replay in `audit` has no arm for `Change::ItemHeld` or `Change::ItemFreed` (the arms run from `crates/riff/src/audit.rs:457` to `:555`). No merged pull request adds a hold rule (`gh pr list --search audit`: #470, #1130, #1134, #1378, #1709).
- The records exist and reach the audit. `item_held` and `item_freed` are at `crates/riff-core/src/record.rs:301` and `:303`. `Record::of_repository` keeps them (`crates/riff-core/src/record.rs:594`), and `POST /v1/log` filters by it (`crates/riff-server/src/lib.rs:2983`).
- The writers exist: `riff plan hold` and `riff plan free` (`crates/riff/src/main.rs:1618`, `:1629`), also through the daemon from the container of the lead (`crates/riff/src/daemon.rs:1436`, #1619), the tools `hold` and `free` of the lead (`crates/riff/src/tools.rs:650`, `:665`; `LEAD_TOOLS` at `:125`), and the keys `h` and `f` of the Waves tab (`crates/riff/src/tui.rs:724`). The server writes them in `crates/riff-state/src/state/plan.rs:504` and `:546`.
- The server still refuses a claim of a held item by a worker with `on_hold` (`crates/riff-state/src/state/work.rs:327`, 01M43GSGPJ69TPWPA4935WR8RW).
- Each container session of a work item or a verify starts as a worker: the wave job at `crates/riff/src/wave.rs:3643`, `riff run` at `crates/riff/src/run.rs:2065`. The lead starts as no worker (`crates/riff/src/lead.rs:810`). So the audit knows a worker from the mark of `session_started`, as rule 3 does.
- The body is out of date in three points:
  - Its rule 8 asks for "the `by` of the lead of the user, the owner or an admin". The log of a repository has no record of the roles: `AdminSet` and `OwnerSet` are no records of a repository (`crates/riff-core/src/record.rs:604`). The design says so too: rule 8 "checks only the worker mark" (`docs/src/design-plan.md:380`). Design #1781 (Q4) and #1794 also remove the owner.
  - It says "ID from `just rid`". CLAUDE.md asks for `just req SECTION "text"`. The section of 01M3ZWRC6H0P7EF6CMECCYZ2RC is "Waves".
  - It does not name the prune requirement 01M588N3R2VP12A8X4EASX0BJ4 (`docs/src/requirements.md:254`), the Audit report of the TUI (01M6VT3JMABMEXSBPE414MBJZP, #1709), or part 5 of `docs/src/design-plan.md` (`:374` still says rule 5 reads `plan_set`). No client sends `plan` today (#501 closed as not planned), as the body says.

New title: riff audit rules 8 and 9: no worker holds or frees an item, and no worker claims a held item

New body:

```markdown
`riff audit` checks the rules 1 to 7 (`crates/riff/src/audit.rs`, 01M3ZWRC6H0P7EF6CMECCYZ2RC). It skips the records `item_held` and `item_freed`, though `POST /v1/log` gives them. The server refuses a claim of a held item by a worker with `on_hold` (01M43GSGPJ69TPWPA4935WR8RW), and refuses a hold or a free of a worker with `not_allowed` (01M43GSGGY0QMB5D5EH92M6ZFP). The audit cannot prove either after the wave.

Holds come from `riff plan hold` and `riff plan free` (also through the daemon from the container of the lead), the lead tools `hold` and `free`, and the keys `h` and `f` of the Waves tab. Each container session of a work item or a verify starts as a worker (`crates/riff/src/wave.rs`, `crates/riff/src/run.rs`). The audit knows a worker by the mark of its `session_started`, as rule 3 does.

Add two rules:

- Rule 8: no `item_held` or `item_freed` record has the `by` of a worker. The log has no record of the roles, so the rule checks only the worker mark. A `by` of a person or of a session that is not a worker passes.
- Rule 9: no claim of an item by a worker while the item was held: after its `item_held`, with no `item_freed` before the claim. A hold from before the span counts. A hold names one item: a hold of `issue-N` does not cover `verify-issue-N`. A claim of a session that is not a worker while the item was held is a line with a warning, and does not fail the rule.

Rule 5 stays as it is. No client sends `plan`, so the audit reads no `plan_set`.

Done when:

- A test for rule 8 and for rule 9, each with a good log and a log that breaks it. A test of the warning of rule 9.
- A test: a claim after `item_freed` passes rule 9. A test: a hold from before the span fails a claim of a worker in the span.
- After a prune, rule 8 and rule 9 are "not checked" when the span starts before the oldest kept record. Rule 9 is "not checked" for a claim of an item with no `item_held` or `item_freed` before it in the kept records, and for a claim of a session whose start is gone. A test.
- Requirements from `just req Waves "..."` for the rules 8 and 9 and their prune cases. 01M3ZWRC6H0P7EF6CMECCYZ2RC and 01M588N3R2VP12A8X4EASX0BJ4 are cut in part to name nine rules.
- The rustdoc table and `RULES` of `audit.rs`, and part 5 of `docs/src/design-plan.md`, have the rules 8 and 9. Part 5 no longer says that rule 5 reads `plan_set`.
- The Audit report of the TUI shows the rules 8 and 9 (01M6VT3JMABMEXSBPE414MBJZP).
- Docs: the table of "Check that a wave followed the rules" in `docs/src/development.md` has the rows 8 and 9. Its `sh` block stays `riff audit --wave 18`.
- `just check` passes with zero warnings.

Related: #366, #501, #1020, #1781
```

## #561 riff top blinks at each redraw when the board is taller than the window

Verdict: rewrite

Why:

- The cause is still there. `draw_top` writes `\x1b[H\x1b[2J`, flushes, then prints the whole board: `crates/riff/src/main.rs:4373` (`if clear {`) and `crates/riff/src/main.rs:4378` (`anstream::print!("{}", top.view())`). `clear` is `!once && stdout().is_terminal()` (`crates/riff/src/main.rs:4293`), so `--once` and a pipe get no erase.
- `riff::top::Top` has a `width` and no height (`crates/riff/src/top.rs:378`). `Top::view` cuts each line to the width only (`crates/riff/src/top.rs:933`). `riff::top::REFRESH` is 3 s (`crates/riff/src/top.rs:172`).
- The ratatui TUI did not replace `riff top`. The TUI is `riff` with no command (01M6VAZ7WH3MFAYF4PAZW6EDVY, `crates/riff/src/tui.rs:1771` `ratatui::try_init`). `riff top` stays the live table that it draws again in place (01M6QEXNM0QCGH6RG4Q8M9VH4W, merged in #1635). #1701 built the Sessions tab, removed `riff view` and kept `riff top`. #541 closed into the TUI design #994. #158 was an exploration. Neither one fixed `riff top`.
- No other issue or PR fixed the blink (searches "blink", "flicker", "alternate screen", "riff top height").
- The width rule 01M3QA8EZHX5B8C9CKF8Q3154X (`docs/src/requirements.md:2660`) and the live-table rule 01M3NB54P1RBHTA5TKXP8BMY3K (`docs/src/requirements.md:2586`) are as the issue says. No requirement covers the height yet. Its section is "Sessions" (`docs/src/requirements.md:2353`).
- The book section of `riff top` is now "See each session in a live table" (`docs/src/how-it-works.md:1714`), under "See what each session does" (`docs/src/how-it-works.md:1503`).
- `riff top` runs a new binary in its place (01M3NT6WXGCNKW3EQ7MBJDQTR4, `crates/riff/src/binary.rs:545`). With the alternate screen, Ctrl-C and that run must give the screen back. The old body does not say so.
- A pseudo-terminal harness exists for the integration test: `Tty` with `vt100` in `crates/riff/tests/tui.rs:20`.
- The body is out of date in form: `just ci` (today: `just check` and the Gate), the generic `Docs:` line, no `just req` line, and an "Optional" line that is not a test.

New title: riff top blinks at each redraw when the board is taller than the window

New body:

```markdown
When the board of `riff top` is taller than its window, for example in a tmux pane, the screen blinks at each redraw: each `riff::top::REFRESH` (3 s) and after each message of the thread. The board goes blank, then draws line by line and scrolls past the end of the pane.

The cause is in `draw_top` (`crates/riff/src/main.rs`): it writes `\x1b[H\x1b[2J` and flushes, then prints `top.view()` through line-buffered stdout. `riff::top::Top` has a width and no height, so a board with more lines than the window scrolls it. In tmux, `\x1b[2J` can also push the old screen into the scroll-back.

`riff top` stays the live table with the TUI (01M6QEXNM0QCGH6RG4Q8M9VH4W), so the fix goes in `riff top`.

Done when:

- `riff top` builds each frame first and writes it in one write: home the cursor, each line followed by `\x1b[K`, then `\x1b[J`. It writes no `\x1b[2J`. Each frame is between `\x1b[?2026h` and `\x1b[?2026l` (synchronized update). A terminal that does not know them ignores them.
- `riff top` fits the board to the height of the window. When the board has more lines than the window, it shows the lines that fit and one last line that says how many lines it did not show. The line of a look that failed (01M3Z8FXE2DY34ZP75WJE1S8HR) stays the first line. The width rule stays (01M3QA8EZHX5B8C9CKF8Q3154X).
- `riff top` uses the alternate screen while it runs. Ctrl-C, an error and the run of a new binary (01M3NT6WXGCNKW3EQ7MBJDQTR4) give the normal screen back. `riff top --once` and output that is not a terminal stay as they are.
- A new requirement with `just req "Sessions" "..."` says the height rule and the alternate screen.
- A unit test: a board taller than the window gives a frame with exactly the height of the window, and its last line gives the count of the lines not shown.
- An integration test in `crates/riff/tests/top.rs`: `riff top` in a pseudo-terminal of 80 x 24 (as `Tty` in `crates/riff/tests/tui.rs`) with a board of more than 24 lines draws two frames with no `\x1b[2J`, and no line goes past row 24.
- Docs: "See each session in a live table" in `docs/src/how-it-works.md` says what a person sees when the table is taller than the window, and that Ctrl-C gives the normal screen back.

Related: #158, #541
```

## #576 The test lease of riff-server is too short under load

Verdict: rewrite

Why:

- The short test lease is still there: `LEASE` at `crates/riff-server/tests/common/mod.rs:98` has `read_every` 50 ms (line 100) and `valid_for` 500 ms (line 101). The default `valid_for` of riff-server is 5 s (`crates/riff-server/src/lease.rs:241`).
- The lease is still in use. "One instance" in `docs/src/requirements.md` holds R137 to R156 and R139 (`docs/src/requirements.md:2296`). None of them moved to `requirements-history.md`.
- The failure path is as the issue says. `leased()` is false after `valid_for` with no good read (`crates/riff-server/src/lib.rs:826`). Then the gate replies 503 "riff-server does not serve now. Try again." (`crates/riff-server/src/lib.rs:3072`), and a save of the token store fails with "the server does not serve now" (`crates/riff-server/src/lib.rs:757`).
- `tests/import.rs` exists, but it is now a module of the one test binary `all` (`crates/riff-server/tests/all.rs:21`, `autotests = false` in `crates/riff-server/Cargo.toml:11`, since #596). So `cargo test -p riff-server --test import` in the old body no longer runs. Its `call` helper asserts 200 with no retry (`crates/riff-server/tests/import.rs:121`).
- Only some helpers work around a late read: `refresh` tries again on a 503 of the gate (`crates/riff-server/tests/common/mod.rs:297`), and so does `served` in `crates/riff-server/tests/lease.rs:284`. Their rustdoc names the 500 ms of `LEASE` as the cause.
- The tests of `riff` copy the same 500 ms lease for a server on a store: `crates/riff/tests/signed.rs:41`, `owner.rs:802`, `questions.rs:170`, `audit.rs:218`, `streams.rs:175`.
- The rule is now a requirement: a test passes on a busy machine (01M41A0M2XWCWTWGF7T9DR03W0, `docs/src/requirements.md:1249`). `just stress` (01M517P9QE2SQ45SPV3K2SM71B, `justfile:182`, book "Run a test on a busy machine" `docs/src/development.md:435`) replaces the `stress-ng` run of the old body.
- #294 (closed) fixed other flakes and added `just stress`. It did not touch the lease. #546 covers timed checks only. #485 fixed one lease test under load. `read_every` and `valid_for` of `LEASE` are unchanged since #45 (`git blame` of `crates/riff-server/tests/common/mod.rs:100-101` gives b1b5cdd0).
- The old body has the generic `Docs:` line and no requirement line.

New title: The test lease of riff-server is too short under load

New body:

```markdown
The tests of riff-server run a server on a store with a short lease: `LEASE` in `crates/riff-server/tests/common/mod.rs` has `read_every` 50 ms and `valid_for` 500 ms. riff-server uses 5 s (`Timing::default` in `crates/riff-server/src/lease.rs`). Under load, one read of the lease that comes more than 500 ms late stops the serve until the next read (`leased()` in `crates/riff-server/src/lib.rs`, R139). The gate then replies 503 "riff-server does not serve now. Try again.", and a save of the token store fails with "the server does not serve now". A test that expects 200, for example `call` in `crates/riff-server/tests/import.rs`, then fails.

Seen in #556: two tests of `tests/import.rs` failed under load 23 and passed alone.

Today only some helpers try again on a 503 (`refresh` in `tests/common/mod.rs`, `served` in `tests/lease.rs`). The tests of `riff` copy the same 500 ms lease: `crates/riff/tests/signed.rs`, `owner.rs`, `questions.rs`, `audit.rs` and `streams.rs`. A test passes on a busy machine (01M41A0M2XWCWTWGF7T9DR03W0).

Done when:

- `common::LEASE` and the copies in the tests of `riff` have a `valid_for` of 5 s. `wait` and `read_every` stay short.
- Each test that checks a loss of the lease (for example in `tests/lease.rs`, `tests/cut.rs`, `tests/start_order.rs`) sets its own short lease and makes the loss happen. It does not wait for a late read.
- `STRESS_RUNS=20 STRESS_LOAD=16 just stress -p riff-server --test all import::` passes 20 of 20.
- A new requirement with `just req "Service" "..."`: a test server on a store keeps its lease for the `valid_for` of riff-server; only a test of the loss of the lease uses a shorter one.
- The rustdoc of `common::LEASE` says the rule.
- Docs: "Write a test" in `docs/src/development.md` says which lease a test server on a store uses, and when a test sets a short one.

Related: #294, #546, #556
```

## #585 Design: gates at the front (criteria, design note, risk) and a human sign-off

Verdict: rewrite

Why:

- The need holds. riff has no review of the criteria, no design note, no risk level and no label `needs-sign-off`: a `grep -rniE 'needs-sign-off|risk level|design note|risk_level' crates docs/src` finds nothing.
- The front gate of today is only the presence of a "Done when" line: the wave builds no issue with none (01M5R0HEP1MXWYY643F1N1DWEM, `docs/src/requirements.md:5902`), `riff wave move` warns (01M5R0HERC566W14SEJ3ZCG6V5, `docs/src/requirements.md:490`), and the worker stops when the criteria are missing or cannot be tested (R173, `docs/src/requirements.md:44`; `run_start` in `crates/riff/src/text.rs:6427`). Nothing checks fit, missed cases or conflicts with the other items of the wave.
- The one sign-off of a person today is the deploy: `riff release approve` refuses each session, also the lead (`crates/riff/src/main.rs:1123`, `crates/riff/src/main.rs:3589`; 01M6WZ40SFVZ6CRQ1H4Z27CWP4, `docs/src/requirements.md:7350`; book: "Approve a release", `docs/src/start-a-riff.md:273`).
- Needs: #579 and #581 are open in Wave 58. Each ends with "Questions for mike" and has no answer on the issue. So this design still waits for them.
- The part "Proof of a human" is decided elsewhere: #571 (closed 2026-11-06) chose a FIDO2 / WebAuthn proof of presence, with a policy `off` or `required`. Its build item #1658 (open, Wave 55) already lists "the answer to a decision that waits for the person" as an action that needs a proof. #1657 adds the authenticators.
- The home of an approval that waits is the design #1489 (open, Backlog): a board, and riff asks in code at the moment the step waits.
- #1815 (closed 2026-11-09): no session gets the token of the person (01M6Z87A04EXTT1FFJSJ1EM6TB, `docs/src/requirements.md:1064`). The lead put this in the body on 2026-11-11.
- Stale parts of the body: "the approver or the owner": #1794 (open, Wave 56) folds the owner into the admin. "the sandbox (#549)": #549 closed 2026-10-07; riff runs each session in a container (`docs/src/how-it-works.md:28`). #578 is closed as not planned. The "Docs:" line sits inside the sign-off list, not in "Done when".
- The trigger "a new requirement" fires on each item: each decision is a new requirement (`CLAUDE.md:17`).

New title: Design: gates before the code (a review of the criteria, a risk level, a design note) and a sign-off of a person

New body:

```markdown
A design: gates at the front of the work. #581 reviews finished work. Many faults come before the code: in the criteria, in the design, in an assumption about the world. A fix there is a sentence, not a rework.

Needs: #579, #581 (the council).

## Evidence (Wave 21)

- #557: the design assumed the form of the GitHub job subject, with no check. One `gh api` call would have shown that it matched no job.
- #510: a second private copy of a record type was a choice of design, made in the code.
- #547: the criteria missed the files that the crates share. The lead added the check while the item was in work.

## Today

- The wave builds no issue with no "Done when" line (01M5R0HEP1MXWYY643F1N1DWEM). `riff wave move` warns (01M5R0HERC566W14SEJ3ZCG6V5). A worker stops when the criteria are missing or cannot be tested (R173, `run_start` in `crates/riff/src/text.rs`). Nothing checks that the criteria fit the design, miss no case, or agree with the other items of the wave.
- No risk level, no design note, no label `needs-sign-off`.
- One sign-off of a person: `riff release approve` (01M6WZ40SFVZ6CRQ1H4Z27CWP4).
- Decided in other issues, not here: the proof that a person acts (#571; build #1657, #1658, which covers "the answer to a decision that waits for the person"); the home of each approval (#1489); who gives it: an admin (#1794). No session gets the token of the person (#1815).

## Proposal

1. **A review of the criteria** when the lead moves an item into a wave. The council of #581 checks each "Done when" line: it is testable, fits the requirements and the design, misses no case, and has no conflict with another item of the wave. Each finding goes on the issue. The lead fixes the body before a claim.
2. **A risk level:** the label `risk-high`, set by the lead or by the review of the criteria. An item with no label skips the design note. The risk also sets the size of the verify council (#581).
3. **A design note** for an item with `risk-high`, before the code: the types and their invalid states, where the code goes, each assumption about the world with a command that checks it, and the tests. The council reviews it. The review blocks the code (Mike, 2026-10-06).
4. **A sign-off of a person** for a trigger. A file in the repository lists the triggers:
   - a one-way door: a kept format (the log, a checkpoint), a migration, a delete, an interface that other repositories use;
   - trust and access: the sign-in, the rights, a secret, the deploy, the container of a session, an act in the home or the repository of another person;
   - the architecture: a new crate, a new service, a new external dependency, a move of work between the client and the server;
   - cost: new cloud resources, a change of the token or CPU budget;
   - the council and the author do not agree after a second round;
   - a change of the trigger file;
   - when in doubt, ask.
   The note names the trigger, and gives 2 or 3 options with a recommendation. The item gets the label `needs-sign-off`, and the wave builds no item with it. The sign-off is one more kind of approval on the board of #1489, and it needs a proof of presence when the policy is `required` (#1658). The decision becomes a requirement (`just req`).
5. **Calibrate:** the lead shows an admin a sample of design notes with no trigger. A miss adds a trigger. A sign-off that nobody needed removes one. The count goes with "The measure" of #579.

## Questions for mike

- Q1: The review of the criteria at `riff wave move`, by the council of #581 (1)? Proposal: yes, with the members design and testing only.
- Q2: One risk label, `risk-high`, or none (2)? Proposal: yes. The lead sets it; the review of the criteria can add it.
- Q3: The design note is a comment on the issue with the heading `## Design note`, and a session of the wave reviews it as a verify does (3)? Proposal: yes. The wave builds the item after a pass.
- Q4: The triggers in `.riff/sign-off.md`, one line for each, changed by a pull request; "a new requirement" is no trigger, because each item adds one (`CLAUDE.md`) (4)? Proposal: yes.
- Q5: The sign-off waits for #1489 and #1658, and gates 1 to 3 come first? Proposal: yes.

Done when:

- Mike answers Q1 to Q5, and the decisions are in this body.
- The build items are filed in a wave or in the Backlog, each with a `Done when:` line.
- Docs: "How It Works" (`docs/src/how-it-works.md`) gets "The gates before the code", next to "Verify finished work"; "The names riff uses on the forge" in `docs/src/waves.md` lists each new label.

Related: #579, #581, #571, #1658, #1489, #1794, #643
```

## #593 riff tail marks the messages of an agent session as read, and can skip a message at its first connect

Verdict: rewrite

Why:

- Part 1, for an agent session, does not apply today. Each agent session runs in a container with no `RIFF_SERVER` and no sign-in, so the default server does not answer there (`crates/riff/src/reach.rs:6`). `riff tail` calls riff-server straight with `api`, not through the daemon (`crates/riff/src/main.rs:2554`). The daemon and the relay set no `RIFF_SESSION` (no match in `crates/riff/src/daemon.rs`, `crates/riff/src/daemon/` or `crates/riff/src/session_relay.rs`). #887 removed the workers in tmux.
- `riff chat` reads as the person (`crates/riff/src/main.rs:2066`), and shows each message that it reads (`Start::History`, `crates/riff/src/chat.rs:612`). It moves no read cursor of an agent session.
- A read with `all` still moves the read cursor, and that is now a rule: 01M6WBQQEGZKVV49QWCQGTE4CY (`docs/src/requirements.md:3235`, #1717), code at `crates/riff-state/src/state.rs:2779`. `riff tail` reads with `all` (`Api::read_after`, `crates/riff-client/src/api.rs:1567`). There is no `peek` flag on the server or in the client.
- What stays of part 1: `riff tail` starts with `Start::New` (`crates/riff/src/main.rs:3987`). Its first connect reads each kept message of the thread after 0 (`crates/riff/src/catch_up.rs:163`) and shows none of them (`:172`). So the first connect marks the whole kept thread as read for the caller, the person, and the next `riff read` of the person does not show those messages.
- Part 2 still holds. The first connect opens the stream (`crates/riff/src/catch_up.rs:161`), then takes the last `seq` of the read as the end of the history (`:172`). A message that comes between the open and the end of the read is in the read, so it counts as history. The stream gives it again, and `Shown::is_new` drops it. It never shows. No merged pull request changed this (#588 added the read after a break; `git log` of `catch_up.rs` has no later fix of the first connect).
- The body asks for `just ci`; CLAUDE.md asks for `just check`.

New title: riff tail skips a message at its first connect, and marks as read the history that it does not show

New body:

```markdown
`riff tail` starts with `Start::New` (`crates/riff/src/main.rs`). At its first connect, `catch_up::connect` (`crates/riff/src/catch_up.rs`) opens the stream, then reads each kept message of the thread with `all`, shows none of them, and takes the last `seq` of that read as the end of the history. Two faults follow:

1. A message that comes between the open of the stream and the end of that read counts as history. The stream gives it again, and `Shown::is_new` drops it. So it never shows.
2. A read with `all` marks as read each message up to its last one (01M6WBQQEGZKVV49QWCQGTE4CY). So the first connect marks each kept message of the thread as read for the person, and shows none of them. The next `riff read` of the person does not show them.

Agent sessions run in containers with no sign-in (`crates/riff/src/reach.rs`), so they cannot run `riff tail`. `riff chat` reads as the person and shows each message that it reads. Neither is part of this item.

Done when:

- At the first connect, `riff tail` shows each message with a `seq` after the last `seq` of the thread when the stream opened, for example a `seq` that riff-server gives at the open of the stream. A test: a message posted between the open of the stream and the end of the first read shows one time.
- The first connect of `riff tail` marks no message as read. A test: the person has an unread message, runs `riff tail` for a moment, and the next `riff read` still shows it.
- After a break, `riff tail` reads and shows as today (01M49Z4E8QB1QDXVCPE6MX5JX2).
- A requirement from `just req Threads "..."` for the first connect. 01M49Z4E8QB1QDXVCPE6MX5JX2 is cut in part where the new one supersedes it.
- Docs: "Follow a thread" in `docs/src/how-it-works.md` says from where the first connect shows messages; the rustdoc of `catch_up` has the design and its diagram.
- `just check` passes with zero warnings.

Related: #506, #588, #1717
```

## #631 Design: a thorough docs pass and a new information architecture of the book (user, maintainer, agent docs)

Verdict: question

Why:

- The direction of 2026-10-08 (the comment "the decided direction, from a talk with Mike") is not built. The book has no Quick start, User guide, Admin guide or Appendix (`docs/src/SUMMARY.md:6-21`).
- The design is still in the book: four `design-*.md` pages and "Design: the UIs of riff" (`docs/src/SUMMARY.md:11-15`). `requirements.md` is still a page of the book (`docs/src/SUMMARY.md:16`), and `requirements-history.md` is in the part History (`docs/src/SUMMARY.md:19-21`).
- Changes after the direction went the other way: `riff-uis.md` (#1004) and `riff-mobile.md` (#968), both design pages, came into the book on 2026-10-17. Requirement 01M5RGXD7WV4H3VR0YA7JW9GXR (`docs/src/requirements.md:4358`, #1432, 2026-10-25) says "The history page is in the part History of the book".
- The book grew: the issue counted about 15,600 lines; `wc -l docs/src/*.md` gives 32,018 today (development.md 8,123, requirements.md 8,534, how-it-works.md 6,428).
- No audit list, no build item and no agent-rules document came from this issue. `gh search issues` for "docs pass", "information architecture", "user guide", "admin guide", "agent rules" and "Rust style" finds only #631. Outside `docs/src/`, the repository has only `CLAUDE.md`, `README.md`, `design/` and `spikes/` as markdown (`git ls-files '*.md'`).
- Done already, or done before this issue: CI publishes the rustdoc in the book under `api/` (`.github/workflows/pages.yml:34-38`, since ea733162 of 2026-09-26); `just doc` runs `RUSTDOCFLAGS="-D warnings" cargo doc`, so a broken doc link fails (`justfile:186-188`); `CLAUDE.md:13-14` says "Design docs live in the code as rustdoc" and "The book stays for people" (4367ebc0, 2026-09-26).
- "Keep it current" is done: #629 (closed 2026-10-07) and "User docs (STRONG REQUIREMENT)" (`CLAUDE.md:45`). The docs member of the council is in the proposal of #581 (point 3), but #581 is open with no answer.
- Moot parts: the plan "after Wave 22 the lead pauses the repository" went with the line 2.x, which Mike retired on 2026-10-15 (#699 comment). The threat-model pass has no subject: the table "What sessions share" replaced the threat model (`docs/src/how-it-works.md:171`, 01M4Z60B8PW9GSQ79H176P6915), and `riff::confine` is gone (no `confine.rs` in `crates/riff/src`). "The skill" is gone (#887; `crates/riff/tests/no_skill.rs:1-5`); a session gets its rules from the `CLAUDE.md` of its repository (#1407).
- The comment items of 2026-10-08 are moot: "Allow an account" and 01M4CNN39TTK36GX34RCWKES80 are in neither `requirements.md` nor `requirements-history.md`, and #649 closed as not planned on 2026-10-15.
- Rust rules already live as requirements and items: 01M49W18QQKF4KYDRYP3ZK9F1Q (`docs/src/requirements.md:4333`), Wave 59 "strong types from end to end", #828.

Questions for mike:

- Q1: Does the direction of 2026-10-08 still hold: the book has four parts (Quick start, User guide, Admin guide, Appendix), and the design and the requirements leave the book? Proposal: yes. It matches `CLAUDE.md:13-14`. Do it in thin build items, with no pause of the repository.
- Q2: `requirements.md` and `requirements-history.md` leave the book. This supersedes 01M5RGXD7WV4H3VR0YA7JW9GXR in part. Proposal: yes. Both files move out of `docs/src/` to one folder of the repository, `just req`, `just reqs` and `hygiene versions` read them there, and the Appendix links to them on GitHub.
- Q3: The design pages (`design-storage.md`, `design-engine.md`, `design-link.md`, `design-plan.md`, `riff-uis.md`, `riff-mobile.md`). Proposal: each built part moves into the module doc of its crate, after a check against the code. A part that is not built (the phone app, the web UI, the desktop GUI) moves to its design issue.
- Q4: The three documents of agent rules (Writing, Rust style, Engineering rules). Proposal: no new files. `CLAUDE.md` gets the parts "Rust style" and "Engineering rules" next to "Writing", because a session reads the `CLAUDE.md` of its repository (#1407).
- Q5: The work from here. Proposal: drop the threat-model pass. File one audit item first: each page against the code and `--help` at `origin/main`, with a list of gaps, stale parts and repeats. Then one build item for each part of Q1 to Q4, each with a `Done when:` line and a `- Docs:` line. Close this issue when the items are filed.

## #639 Explore: experiments with models and effort levels for the lead, workers and verifiers

Verdict: question

Why:

- Mike decided the model after this item: "all sessions must run opus now. sonnet is not good enough for us to build riff" (#1691, closed 2026-11-07 by PR #1695). The open design #579 records it: "Another model: decided. Each session runs on the model of `riff.toml` (`model = "opus"`)."
- riff has one model for each repository, not one for each role: the key `model` in `[session]` of `riff.toml`, default `opus` (01M6VAQWECJ6N6VJF66KCMNYD1 at `docs/src/requirements.md:5030`, `MODEL` at `crates/riff/src/session_setup.rs:73`, `riff.toml:12`). The lead, each work item and each verify start with the same `claude --model MODEL`. So the "mix" experiment (a fast model for a worker, a strong one for a verifier) needs a new setting first. Book: "Set the model of the sessions", `docs/src/development.md:1095`.
- Mike decided the effort too: "I need the default for leads and workers to be xhigh. It should be configurable but default to xhigh" (#638, closed 2026-10-24 by PR #1333). The daemon starts each role with `claude --effort LEVEL`, xhigh by default, and `riff daemon limit --effort`, `--effort-lead` and `--effort-verify` set each role (01M5P2MG28EE75PPGYAWHTY2S1 at `docs/src/requirements.md:5447`, `crates/riff/src/main.rs:1428` to `1455`). Book: "Set the effort of the sessions", `docs/src/development.md:3718`.
- riff records the tokens of each claim for each model, on the issue (`crates/riff/src/usage.rs:1`, `riff usage` at `crates/riff/src/main.rs:267`, book "See the tokens of an issue" at `docs/src/development.md:5151`). The report has no effort: its fields are item, kind, session, times and models (`crates/riff/src/usage.rs:750`). So the data cannot tell one effort from another.
- An unplanned A/B run took place: each work and verify session ran on `claude-opus-5-5` up to 2026-10-18, on `claude-sonnet-5-5` from 2026-10-19 (after #1154), and on Opus again after PRs #1694 and #1695 on 2026-11-07 (facts in the body of #1691). Mike's decision came from that time.
- No closed issue or merged pull request ran the experiments of this item. `gh search issues "experiment"` and `"model per role"` find only #639.
- Related #579 and #581 are open. #592 is closed (the pressure of a host), and does not touch this item.

Questions for mike:

- Q1: You decided the model (Opus for each session, #1691) and the effort (xhigh for each role, #638), and both are settings today. Do you still want the experiments of this item? Proposal: no. Close this issue with a comment that names #1691, #638, 01M6VAQWECJ6N6VJF66KCMNYD1 and 01M5P2MG28EE75PPGYAWHTY2S1.
- Q2: Do you want a model for each role (for example a cheaper model for a verify)? Proposal: no, not now. Your word on Sonnet stands for each role, and the verify already has its own effort (`riff daemon limit --effort-verify`).
- Q3: If the limits of the plan start to stop the work, how do we choose an effort? Proposal: then open one item that adds the effort of the session to the usage report of each claim (`crates/riff/src/usage.rs:750`), so `riff usage --wave N` can compare the levels with no extra runs. No test repository and no seeded faults until then.

## #643 CODEOWNERS and a code-owner review for the files that rule riff

Verdict: rewrite

Why:

- The need holds. Mike said yes on 2026-10-15 (comment on #643). The repository has no `CODEOWNERS`: `git ls-files | grep -i codeowners` finds nothing, and no file of `crates/`, `docs/` or `.github/` names it.
- The ruleset `main` still needs no review: `required_approving_review_count: 0` and `require_code_owner_review: false` (`deploy/github.sh:74` and `deploy/github.sh:76`, 01M3JFEXG85AJK8ZE8N807EQVB; the live ruleset 24088143 from `gh api repos/como-technologies/riff/rulesets/24088143` says the same). `riff init` makes the same rule in other repositories (`crates/riff/src/repo_setup.rs:352` and `:354`).
- The body is out of date on the Apps. There are two Apps now: a worker and a verifier act as `riff-OWNER[bot]` of the session App, the lead as `riff-OWNER-lead[bot]` of the lead App (PR #1746, #1815 closed by PR #1838, `docs/src/how-it-works.md:6252`). The worker token HAS `workflows: write` (`crates/riff-server/src/github.rs:183`), so a worker can change `.github/workflows` today. The body says the opposite.
- Each file of the sandbox and the forge that the body names is gone: `crates/riff/src/profile.rs`, `confine.rs`, `broker.rs`, `role_rules.rs`, `forge.rs` and `crates/riff-server/src/forge.rs`. `.claude/`, `deploy/` and `docs/src/requirements.md` exist. The files that rule riff today: the rights of each role (`rights` in `crates/riff-server/src/github.rs`), the token of a session (`crates/riff/src/daemon/gh_token.rs`), the Claude Code rules of each role (`crates/riff/src/permissions.rs`), the guard (`crates/riff/src/guard.rs`, `.github/workflows/riff-guard.yml`), the rulesets (`deploy/github.sh`), and the checks (`justfile`, `crates/hygiene/`, `crates/reqs/`).
- `docs/src/requirements.md` in the list blocks nearly each pull request: 90 of the last 100 commits on `main` (2026-11-04 to 2026-11-09) changed it, because each change adds a requirement (`CLAUDE.md`). The paths of the new body below changed in 19 of the same 100 commits.
- The part "`riff verify pass` and the lead see the wait" is done: the wave of the daemon tells the lead one time for each head commit when `reviewDecision` is `REVIEW_REQUIRED`, and `riff top` and `riff wave list` show `stuck` with `review` (01M6FZ141Y0J8G2AYX0G5HY1CX, `crates/riff/src/rollout.rs:1844`, book "Approve a pull request that needs a review" at `docs/src/development.md:2877`). A code-owner review gives the same `REVIEW_REQUIRED`.
- `crates/riff/tests/github.rs:139` already runs `deploy/github.sh` with a fake `gh`: the place for the test of the ruleset.
- Mike is `n8behavior` on GitHub: the contributors API lists `n8behavior` with 785 contributions, and `git log` has 785 commits of Mike Sandman.
- The identity of each role is now the open design #1542, not #579 or #581.

New title: CODEOWNERS and a code-owner review for the files that rule riff

New body:

```markdown
From the lead. Mike (2026-10-07): "For versioned files we don't want certain roles to edit, we have CODEOWNERS available to us." Mike (2026-10-15): yes, riff gets CODEOWNERS.

## Today

- The repository has no `CODEOWNERS`. The ruleset `main` (`deploy/github.sh`) needs `Gate`, `Hygiene` and `riff/verify`, and no review: `required_approving_review_count: 0`, `require_code_owner_review: false`.
- A worker and a verifier act on GitHub as `riff-como-technologies[bot]` of the session App, the lead as `riff-como-technologies-lead[bot]` of the lead App (#1746, #1815). The worker token has `workflows: write` (`rights` in `crates/riff-server/src/github.rs`).
- So a change to a file that rules riff merges after a verify by another session, and no person sees it first.

## Change

- `.github/CODEOWNERS` names Mike (`@n8behavior`) as the owner of these paths:
  - `/.github/` (the workflows, the guard `riff-guard.yml`, and `CODEOWNERS`)
  - `/.claude/` and `/CLAUDE.md`
  - `/deploy/`
  - `/justfile`, `/crates/hygiene/`, `/crates/reqs/`
  - `/crates/riff-server/src/github.rs` (the rights of each role) and `/crates/riff/src/daemon/gh_token.rs` (the token of a session)
  - `/crates/riff/src/permissions.rs` (the Claude Code rules of each role)
  - `/crates/riff/src/guard.rs` (the guard)
- The ruleset `main` in `deploy/github.sh` sets `require_code_owner_review: true` and `dismiss_stale_reviews_on_push: true`. `just github` applies it. A pull request that changes an owned path merges only after the owner approves its last push, also when the verify passed. Each other pull request merges as today.
- A GitHub App cannot be a code owner, so the approval of a session does not count.
- The wait needs no new code: the wave tells the lead one time for each head commit, and `riff top` and `riff wave list` show `stuck` with `review` (01M6FZ141Y0J8G2AYX0G5HY1CX).
- Only this repository. `riff init` adds no `CODEOWNERS` to other repositories.

GEN;SET: `docs/src/requirements.md` is not in the list. 90 of the last 100 commits on `main` changed it, because each change adds a requirement.

Done when:

- `.github/CODEOWNERS` is on `main` with the paths above.
- A test in `crates/riff/tests/github.rs`: each path of `CODEOWNERS` exists in the tree, and each path of the list above is in `CODEOWNERS`. So a rename or a new file of the list cannot skip the owner.
- A test in `crates/riff/tests/github.rs`: the ruleset `main` of `deploy/github.sh` has `require_code_owner_review: true` and `dismiss_stale_reviews_on_push: true`.
- A new requirement in "Pull requests on GitHub" (`just req`): a change to a path of `CODEOWNERS` merges only after a review of its owner. 01M3JFEXG85AJK8ZE8N807EQVB keeps only the part that holds.
- Docs: a new how-to "Protect the files that rule riff" in `development.md`, under "Merge by pull request on GitHub", with an `sh` block for `just github` and for `gh api repos/como-technologies/riff/rulesets`. "Set up the repository" names the code-owner review. "Approve a pull request that needs a review" says that a change to an owned path waits for its owner.

Related: #585, #1542
```

## #659 A fresh lead gets the Claude memory of the old lead from riff-server

Verdict: question

Why:

- The issue says: build #1164 first, then Mike decides if the notes make this issue not needed. #1164 closed on 2026-10-19 by PR #1190. No comment on #659 (0 comments), #1164 or #1163 (usage comments only) holds that decision.
- The notes of the lead exist: requirements 01M59258K3TYR09HC4AWHWZPEX to 01M592593NSC1DE7A653Q8PCPS in "The notes of the lead" (`docs/src/requirements.md:8435`). `notes` is in `LEAD_TOOLS` (`crates/riff/src/tools.rs:125`). The book has "The notes of the lead" (`docs/src/how-it-works.md:5167`) with the how-tos at `how-it-works.md:5187`, `:5195` and `:5208`.
- The rules of the old memory moved into code: `text::LEAD_RULES` has 20 rules (`crates/riff/src/text.rs:11742`), and `MCP_LEAD` holds them (01M592593NSC1DE7A653Q8PCPS). The start text says "never only in the Claude memory of the host" (`crates/riff/src/text.rs:11884`, 01M592590XREH99ABQF2D2KT6H).
- `riff lead move HOST` exists (#1163, PR #1196, 01M596DH64SQ5AJ4Y6BETGTPSK). The book says "no `scp`, no copy of the Claude memory" (`docs/src/how-it-works.md:5222`, heading "Move the lead to another host" at `:5218`).
- The Claude folder of the lead still stays on one host: `leads/OWNER-REPO/claude/` (`crates/riff/src/lead.rs:32`, `lead::folder_of` at `crates/riff/src/lead.rs:106`, "stay between the starts" at `crates/riff/src/lead.rs:48`). No code mirrors the memory files of `claude`: a grep for `MEMORY.md`, "memory import" and "memory folder" in `crates/` and `docs/` finds nothing.
- A later decision of Mike on #1191 (comment of 2026-10-19 13:25 UTC): all sessions put their notes, logs, context and memories in one shared knowledge base, each a clean model with an address, "otherwise it becomes a heap of markdown files". A mirror of the raw memory files of `claude` is such a heap.

Questions for mike:

- Q1: Do the notes of the lead (#1164) and `riff lead move` (#1163) make this issue not needed? Proposal: yes. Close this issue as done by PR #1190 and PR #1196.
- Q2: If the memory that `claude` writes must still travel, where does it go? Proposal: in the knowledge base of #1191, as its model "memory", not as a mirror of the files in this issue.

## #665 A short outage does not take the owner role; it passes only on a request or after a long absence

Verdict: close

Why:

- The decided design of the roles removes the owner role. Design #1781 (closed as completed 2026-11-08), point 9: "each admin does what the owner does today ... `riff owner ...` and the checks of a gone owner go", and the answer of Mike, Q4: "Yes: the owner goes into the admin".
- Build item #1794 (open, in "Wave 56: the projects and the roles") removes `riff owner EMAIL`, `riff owner --take`, `riff owner --deny`, "the check of a gone owner and its settings", and the word `owner` in `riff who`, `riff members` and the TUI. With no owner, no outage can take the role. Each Done when line of #665 (a longer window, the `--take` flow, a warning for the owner, the owner how-to) builds on code and docs that #1794 removes.
- The comment on #665 ties it to design #666. #666 was closed as not planned on 2026-10-24: its hook is gone, and "a new outage design starts from the 4.x daemon".
- The fault still holds until #1794 merges. The owner logic moved from `crates/riff-server/src/owner.rs` (gone, #739) to `crates/riff-state/src/owner.rs`. `Timing::default()` still checks each 10 minutes and warns after 3 misses (`crates/riff-state/src/owner.rs:95`, `:96`); the next miss makes the owner gone (01M3Q546335NBTKG5BHQ27QC93, `docs/src/requirements.md:3655`). So 40 minutes with no sign still takes the role.
- An admin of the server can make the window longer today with no code change: `--owner-ping-minutes` (`RIFF_OWNER_PING_MINUTES`) and `--owner-pings` (`RIFF_OWNER_PINGS`) of `riff-server` (`crates/riff-server/src/main.rs:138`, `:145`; how-to "Change the times" in `docs/src/start-a-team-riff.md:523`). For example 1440 minutes and 6 pings give 7 days.
- Proposed close comment: "Closed: design #1781 (Q4) removes the owner role, and #1794 removes `riff owner --take` and the check of a gone owner. Until #1794 merges, set `RIFF_OWNER_PING_MINUTES` and `RIFF_OWNER_PINGS` on riff-server for a longer window."

## #699 Design: riff 3.0: one local binary, a container for each item, then peer to peer

Verdict: close

Why:

- Done: "each decision is a requirement". PR #723 (#710) put each decision, the gaps 30 to 35 and O2 into `docs/src/requirements.md`, part "Sessions in containers" (`docs/src/requirements.md:4558`), and marked 281 old requirements superseded. Example: decision 1 is 01M4Z60B7E6X53NBJ01R95RYGE (`docs/src/requirements.md:4703`).
- Done: O2 is stream-json. The spike #707 (PR #722) has its result on this issue (comment of 2026-10-15). The rule is 01M4Z60B82FHN2YG9YJHW8XZX1 (`docs/src/requirements.md:7050`, "How the relay drives claude").
- Done: the table of decision 26 is "What sessions share" (`docs/src/how-it-works.md:171`), rule 01M4Z60B8PW9GSQ79H176P6915 (`docs/src/requirements.md:7215`). Mike signed off the page (#709, PR #711; comment "Decision 35" of 2026-10-15).
- Done: Wave A (Wave 27): #712 to #721, #740 and #746 are closed. Release v3.0.0 merged in #1031. The item "remove the 2.0 code" became #705 (main went back to the line 1.x), a decision of Mike on 2026-10-15.
- Done: the touched issues. 23 of the 30 have a comment that names #699 (the triage of 2026-10-15). #673, #674 and #675 closed on 2026-10-09, before this design. #697 (iroh) and #666 are closed. #659 and #665 are open with no such comment; both are in the grooming batch #1886.
- Done: the book page. The page of this design became "Sessions in containers" in `docs/src/how-it-works.md:28`, with "The goal of isolation" (`:35`) and the picture (#1403 removed `riff-3.md` and the version names). The status note of the introduction says that riff runs each work item in a container (`docs/src/introduction.md:22`).
- Superseded: the re-cut #953 (closed 2026-10-17, signed off by Mike) keeps riff-server as the server of the riff (01M53NGS8FGRNY12DDJA3P0GH2, `docs/src/requirements.md:4570`; 01M4Z60B7FCREZDCTVR73WWRY1, `:4706`). That reverses decision 2 (the daemon is the server) and the "no server" of decision 1.
- Superseded: Wave B (#803, Wave 29) was built and then removed by #959 (PR #1021). Rule 01M53NGW0DWJ075C4MDP06WAGS (`docs/src/requirements.md:4659`). Decisions 20, 21 and 35 are in the history (`docs/src/requirements-history.md:2086`, `:2124`, `:2117`).
- Superseded: Wave C and decision 29. The design PR #896 merged. Rule 01M53NGWN190T2MYB1EZAD36VB (`docs/src/requirements.md:4692`). #887 is done (Wave 36). #881 to #886, #888 and #889 closed as not planned on 2026-10-19. Decision 29 is in the history (`docs/src/requirements-history.md:2985`).
- Reversed: decision 18 ("No GitHub App"). Each session acts as the bot of a GitHub App of the riff (01M6WG4PFA843R82J0N6MW33VP, `docs/src/requirements.md:7155`, #1543). No session gets the token of the person (01M6Z87A04EXTT1FFJSJ1EM6TB, `docs/src/requirements.md:1064`, #1815). Decision 30 (`GH_TOKEN`) is in the history (`docs/src/requirements-history.md:2944`).
- The rest of the body (future work: cloud workers, recovery after a reboot, macOS) is not in "Done when". The Mac work runs in its own waves (Wave 53, Wave 57).
- A side finding for the lead: 01M53NGW0DWJ075C4MDP06WAGS (`docs/src/requirements.md:4659`) and 01M53NGWN190T2MYB1EZAD36VB (`:4692`) still say that the items of Wave B and Wave C wait in the Backlog. Each of those items but #928 is closed as not planned (#846, #920 to #922, #941, #943, #944, #946, #957, #881 to #886, #888, #889). A new item can cut those two texts in part.

## #763 riff init installs the missing prerequisites (podman and its packages), after one question

Verdict: rewrite

Why:

- Not done. `riff init` installs no package and adds no range. When a check fails, it "installs nothing, pulls no image, names the check and the fix of each failed check, and exits with 1" (`crates/riff/src/first_run.rs:56`, 01M4ZJN6WR0JJ891R9JMQNPKGD at `docs/src/requirements.md:6564`). No file of `crates/` calls `apt`, `dnf` or `usermod`: they are only in the fix texts.
- The fixes are text for a person: `sudo apt install podman` (`INIT_FIX_PODMAN`, `crates/riff/src/text.rs:8552`), `sudo apt install catatonit` (`INIT_FIX_INIT`, `crates/riff/src/text.rs:8565`), and `sudo usermod --add-subuids START-END USER, then podman system migrate` with the free range (`init_fix_sub_ids`, `crates/riff/src/text.rs:8601`; `free_sub_id`, `crates/riff/src/first_run.rs:599`; 01M501K21EK8F8N9NPR4565FVP).
- The one question and `--yes` exist only for the steps of the repository on GitHub and the wide rules (`Question` at `crates/riff/src/first_run.rs:846`, `--yes` at `crates/riff/src/main.rs:637` to `640`).
- The book still tells a person to install the packages by hand: `sudo apt install podman catatonit` in "Get this machine ready" (`docs/src/start-a-riff.md:290`, block at `:297`). "Run riff init with no question" is at `docs/src/start-a-riff.md:592`.
- The body is out of date: the heading "Get ready for riff 3.0" does not exist (it is "Get this machine ready"), and it names a version of riff. A new check came after it: the init of podman, `catatonit` (01M511JYPZ8H73DCJ7867YSFGH, PR #804). The ID command is `just req SECTION "text"`, not `just rid`.
- No merged pull request did it: `gh pr list --state merged --search "riff init install"`, `"podman prerequisites"` and `"catatonit"` find #748, #793, #804, #891, #1434 and #1455, and none installs a package.
- The riff daemon never does a step that needs a person (01M51QFPW37EXVH4AXH70MSXBT at `docs/src/requirements.md:6868`, `docs/src/how-it-works.md:786`). That stays: only `riff init` in a terminal installs.

New title: riff init installs podman and catatonit, and adds the subuid and subgid ranges, after one question

New body:

```markdown
From Mike, 2026-10-15: "riff setup or something should install prereqs."

## Today

`riff init` checks the machine: podman, the init of podman (`catatonit`), `/etc/subuid`, `/etc/subgid` and cgroup v2 (`checks` in `crates/riff/src/first_run.rs`). When a check fails, it prints the fix and installs nothing (01M4ZJN6WR0JJ891R9JMQNPKGD). The fixes are text: `sudo apt install podman`, `sudo apt install catatonit`, and `sudo usermod --add-subuids START-END USER, then podman system migrate` (`INIT_FIX_PODMAN`, `INIT_FIX_INIT`, `init_fix_sub_ids` in `crates/riff/src/text.rs`). So a person types each fix. The book tells them to run `sudo apt install podman catatonit` first.

## Change

- When a check of the machine fails and riff can fix it, its line reads `step` with the command, as for the steps of the repository (`Check::planned_line`). Then `riff init` asks one question for all the steps of the machine.
- The packages: podman and catatonit, with the package manager of the machine: `apt-get install -y` on Ubuntu and Debian, `dnf install -y` on Fedora.
- The ranges: `usermod --add-subuids` and `--add-subgids` with the free range of `free_sub_id` (01M501K21EK8F8N9NPR4565FVP), then `podman system migrate`.
- riff runs only these commands with `sudo`, and prints each command before it runs it. `--yes` asks no question. A no, `--check`, or a machine with no `apt-get` and no `dnf` changes nothing and prints the fix lines, as today.
- A command that fails stops the steps. riff prints the command and its error, then the fix of each check that still fails, and exits with 1.
- After the steps, riff runs the checks again, then goes on to the daemon and the session image.
- cgroup v2 and a run as root keep their fix lines: riff does not change them.
- In a session container, `riff init` skips the checks of the machine, as today (01M5SMJMR5KS1Z1Z7CEPH9JVBB). The riff daemon still does no step that needs a person (01M51QFPW37EXVH4AXH70MSXBT).

GEN;SET: the name stays `riff init`. No new `riff setup` command.

Done when:

- Tests with a fake `sudo`, a fake `apt-get` and a fake `dnf` on `PATH`, and a folder for `/etc` (`ETC_VAR`): the commands for `apt-get` and for `dnf`; a yes, a no, `--yes` and `--check`; a machine with neither; a command that fails.
- A test: after a yes on a machine with no podman and no catatonit, each check of the machine passes in the second list.
- A new requirement in "The first run" (`just req`): `riff init` installs the missing packages and adds the missing ranges after one question. 01M4ZJN6WR0JJ891R9JMQNPKGD keeps only the part that holds.
- Docs: "Get this machine ready" in `start-a-riff.md` loses the `sudo apt install podman catatonit` block, says that `riff init` installs them after one question, and names the `step` lines of the machine. "Run riff init with no question" says that `--yes` installs them too, with its `sh` block.

Related: #721, #750
```

## #764 The lead needs no SSH to a worker host: each fact and step of a host goes through riff

Verdict: rewrite

Why:

- Remainder item 1 is done. PR #1197 (commit 15c0bd29) removed `riff workers` and the how-tos "Set the limit of workers" and "Limit the workers of a machine": a grep finds neither in `docs/src/`. "Manage the daemon of another host" (`docs/src/development.md:3793`) says "Your lead can act on the daemon of each of your hosts, with no SSH" (`:3795`), with `riff daemon limit --sessions 4 --host thelio` in a `sh` block (`:3838`). `docs/src/how-it-works.md:168` has it too.
- The docs line is done: "See the facts of each host" (`docs/src/how-it-works.md:1202`) has a `sh` block with `riff who` and `riff top --host thelio` (`:1215`).
- Remainder item 2 is open. `text::MCP_LEAD` (`crates/riff/src/text.rs:11824`), `text::LEAD_RULES` (`:11742`) and `text::lead_start` (`:11876`) say nothing about SSH or `--host`. In `requirements.md`, "SSH" shows only at line 8322 (a TUI note for a person). The skill that had the rule went with #887 (PR #1200).
- `lead_start` already says "Your rules are in the instructions of the riff tools" (`crates/riff/src/text.rs:11883`), and 01M592593NSC1DE7A653Q8PCPS puts each rule of the lead in `LEAD_RULES`. So the rule goes in `LEAD_RULES` only, not in `lead_start` too.
- Remainder item 3 is open. `Facts` (`crates/riff-client/src/daemons.rs:451`) has the version of podman (`:493`), but no check of the init, `/etc/subuid`, `/etc/subgid` or cgroup v2. Those checks are `first_run::machine_checks` (`crates/riff/src/first_run.rs:806`).
- The daemon already runs them at each start (`crates/riff/src/update_steps.rs:499`, `at_start` at `:519`) and keeps the first failed one in the file `needs-you` (`keep_needs_you` at `:506`, `local::needs_you` at `crates/riff/src/local.rs:578`). PR #1200 (commit 376ab967) removed the status line that read it, and no code reads it now. So 01M51QFPW37EXVH4AXH70MSXBT (`docs/src/requirements.md:6868`) names a status line that is gone.
- The comment about `riff workers jobs --host` is out of date: `riff workers` is gone (#1197), riff sets no build jobs (`docs/src/development.md:3707`), and `riff daemon limit --host` sets the limit (01M579Z9MPS86MX7890T6HV18S).

New title: The lead knows that it has no SSH to a host, and the facts of each host carry the failed checks of the machine

New body:

```markdown
From Mike, 2026-10-15: "leads should not expect to be able to SSH to worker hosts. if comms are needed let's add issues to bake that into riff."

#1063, #1065 and #1197 did most of it. The daemon of each host posts its facts (`crates/riff/src/daemon/facts.rs`, `riff_client::daemons::Facts`). The lead acts on another host with `--host HOST` (01M579Z9MPS86MX7890T6HV18S). The book has "See the facts of each host" (`how-it-works.md`) and "Manage the daemon of another host" (`development.md`). Two parts remain.

## 1. The rule is not in the text of the lead

The skill said "You have no SSH to a host". #887 removed the skill. `text::LEAD_RULES` and `text::MCP_LEAD` (`crates/riff/src/text.rs`) say nothing about SSH or `--host`.

Add a rule to `LEAD_RULES`: the lead has no SSH to a host. It acts on another host only through riff, with the commands that take `--host HOST`, for example `riff daemon status --host HOST` and `riff join --host HOST`. A step that only a person can do on a host, for example a fix with `sudo`, goes to the person in the chat, with the host and the command.

## 2. The facts do not carry the checks of the machine

The daemon runs `first_run::machine_checks` at each start (`update_steps::at_start`, `crates/riff/src/update_steps.rs`): podman, the init of podman (catatonit), `/etc/subuid` and `/etc/subgid` with 65536 IDs, and cgroup v2 with `cpu` and `memory` for the user. It keeps the first failed one in the file `needs-you` (`local::needs_you`). No code reads that file since #1200 removed the status line. So the lead does not see a host that is not ready.

Add each failed check, with its fix, to `Facts`. `riff who`, `riff top` and the Machines tab of the TUI show it in yellow under the host.

## Done when

- A requirement from `just req "The lead, the view and the takeover" "..."`: the lead has no SSH to a host, and reaches it only through riff. `LEAD_RULES` has the rule, and the test of `LEAD_RULES` finds its phrase in `MCP_LEAD`.
- A requirement from `just req "The riff daemon" "..."`: the facts of a host carry each failed check of the machine, with its fix. It supersedes 01M51QFPW37EXVH4AXH70MSXBT in part (the status line), and the text of that requirement keeps only the part that holds.
- A unit test of the facts: a host with no catatonit, a short `/etc/subuid` and no `cpu` for the user gives three failed checks, each with its fix. A ready host gives none. The line stays within `MACHINE_CHARS`.
- An integration test with two fake hosts: `riff who` and `riff top` show the failed check of the host that is not ready, and no check for the other host.
- Docs: "See the facts of each host" in `how-it-works.md` shows a failed check in its `text` block, and its list of the facts in yellow names it.

Related: #1063, #1065, #765.
```

## #765 The instructions of the lead say it does no work of a worker: no check after a merge, no step on a host

Verdict: rewrite

Why:

- The need holds. 01M3JD5QCVCB3VEK4KP955JSEA (`docs/src/requirements.md:2437`) still says only "no work item and no verify". `text::MCP_LEAD` still says only "Do not claim work" (`crates/riff/src/text.rs:11827`). That phrase is not in `text::LEAD_RULES` (`crates/riff/src/text.rs:11742`), so no test checks it. `text::lead_start` (`:11876`) has no such rule.
- No merged PR after 2026-10-19 added the rule: the changes to `lead_rules!` since then are #1407, #1561, #1622 and #1799, and none is about the work of a worker.
- The lead has a clone of its own in its container (`clone/` in `crates/riff/src/lead.rs:32`). A build, a test or a check after a merge needs no claim, so no rule stops the lead from doing it.
- The body names old places. `LEAD_TOOLS` is in `crates/riff/src/tools.rs:125` now; `crates/riff/src/mcp.rs` does not exist at origin/main.
- The body says the lead puts an item in a wave with `gh issue edit N --milestone`. The doc test of `MCP_LEAD` refuses both words (`crates/riff/src/text.rs:11817`, `:11818`). The lead uses `riff wave move ITEM --to WAVE` (`:11832`), and `riff run ITEM` in its container starts an item as a job of the wave (#1378, 01M5QVX8FFG2FPWZEW61A985CF, `docs/src/development.md:1643`).
- "A change of the text, no new ID" goes against `CLAUDE.md`: a new decision is a new requirement from `just req`. 01M3JD5QCVCB3VEK4KP955JSEA stays true, so the new requirement adds to it and supersedes nothing.
- "Remove the text about the skill" is done: "The lead" (`docs/src/how-it-works.md:5277`) names no skill, and #1561 made each text of riff name no skill.
- A line in `lead_start` is not needed: it already says "Your rules are in the instructions of the riff tools" (`crates/riff/src/text.rs:11883`). `CLAUDE.md` says to say each thing in one place.
- "No step on a host" overlaps #764 (no SSH to a host). The rewrite of #764 takes that part.
- The book says "It takes no claims: no work item and no verify." (`docs/src/how-it-works.md:5333`), and `crates/riff/tests/lead_book.rs:66` tests it. The comment of that test (`:62`) still says "the skill and the book say it".

New title: The rules of the lead say that it does no work of a session: no build, no test, no check after a merge

New body:

```markdown
From Mike, 2026-10-15: "you are the lead. you are not supposed to be taking claims. you coordinate the workers and talk with me. is this not in the riff prompts/rules/skills/etc?"

Today the rule covers only claims:

- Requirement 01M3JD5QCVCB3VEK4KP955JSEA: the lead takes no claims, no work item and no verify.
- `text::MCP_LEAD` (`crates/riff/src/text.rs`) says only "Do not claim work". That phrase is not in `text::LEAD_RULES`, so no test checks it.
- The lead has no claim tool (`LEAD_TOOLS` in `crates/riff/src/tools.rs`).

But the lead has a clone of its own in its container (`clone/` in `lead::folder_of`). A build, a test, or a check after a merge or a release needs no claim. So no rule stops the lead from doing such work itself, and the lead started to plan a check after a merge.

## Change

- A new rule in `text::LEAD_RULES`, so in `MCP_LEAD`: the lead coordinates the sessions and talks with its person. It does no work of a session: no build or test of a branch, no fix, and no check after a merge or a release. It makes an issue for the work, and gives it to a session with `/opt/riff/riff wave move ITEM --to WAVE`, or starts it now with `/opt/riff/riff run ITEM`. The rule gives the example of a check after a merge.
- "Do not claim work" in `MCP_LEAD` goes into that rule, so the rule is in one place.
- `text::lead_start` does not change: it already says "Your rules are in the instructions of the riff tools."

## Done when

- A requirement from `just req "Sessions" "..."` states the rule above. 01M3JD5QCVCB3VEK4KP955JSEA stays as it is.
- `LEAD_RULES` has the rule, and the test of `LEAD_RULES` finds its phrase in `MCP_LEAD`. The doc test of `MCP_LEAD` finds `/opt/riff/riff run ITEM`.
- A test in `crates/riff/tests/lead_book.rs` finds the rule in "The lead" of `how-it-works.md`. The comment of `the_lead_takes_no_claims_not_even_a_verify` names no skill.
- Docs: "The lead" in `how-it-works.md` says it in the list "What the lead does", next to "It takes no claims", in one place.

Related: #764, #1378.
```

## #828 Code hygiene: states as enums with exhaustive match, not if-chains over flags (daemon.rs first)

Verdict: rewrite

Why:

- The need holds. `SessionState::phase` is still the same if-chain over `record.terminal`, `record.takeover`, `self.held` and `record.closed` at `crates/riff/src/daemon.rs:1996`. `Record` is at `crates/riff/src/daemon.rs:1865`, with `closed` (`:1877`), `failed` (`:1879`), `terminal` (`:1887`), `takeover` (`:1891`), and a new flag `interrupt` (`:1896`, 01M51KJ1R9K7A7RW8T8XH8VW43). `SessionState` is at `:1955`, with `held` (`:1972`) and `ended` (`:1978`). `StartSpec.terminal` is at `:1037`.
- The lint is not on. `Cargo.toml` has no `[workspace.lints]`, and no `crates/*/Cargo.toml` has `[lints]` (`grep -n lints Cargo.toml crates/*/Cargo.toml` finds nothing).
- The review comment (at 78a645d) is out of date:
  - Item 1 (daemon.rs) and item 2 (the relay `Link`, now `crates/riff/src/session_relay.rs:470`) hold. Only the line numbers moved.
  - Items 3 and 4 are done. #858 (the pull request of #854, "Items 3 and 4 of the review on #828") made `of_people` (`crates/riff-state/src/state/command.rs:391`), `registers_first` (`:446`), the worker rule of `permits` (`:663`), `SignInError::for_log` (`crates/riff-server/src/oidc.rs:133`) and the `Failed` reply (`crates/riff-server/src/lib.rs:3143`) exhaustive. Requirement 01M51JBTEM10W68K6F267BB79N records it.
  - Item 5 (`auth::Config`) holds. See `crates/riff-server/src/auth.rs:90`, `:98` and `:172`, `lib.rs:327`, and `listen.rs:230`.
  - Item 6 is mostly gone. #1200 removed the plugin entries (`hook`, `statusline`). A session that left now bails for each command (`crates/riff/src/main.rs:2276-2281`). The last `match command` names each command (`main.rs:2283`, `unreachable!` for the ones that ran before). One wildcard stays, at `main.rs:2166`.
  - Item 7 (`Change`) holds, at new lines.
  - Item 8 is gone. #1199 removed `worktrees.rs` and `compact.rs`. `compact.rs` was also in "What stays".
  - Item 9: the idle-stop fields (`stopping`, `asked`, `told_stuck`) are gone (#1198). `Block` (`crates/riff-state/src/state/presence.rs:836`) and `BlockedInfo` (`crates/riff-core/src/wire.rs:921`) stay.
  - Item 10: `stopping` is gone from `SessionInfo` (`wire.rs:639`). `Known` (`crates/riff-state/src/state/sessions.rs:75`) and `SavedSession` (`:256`) stay.
  - Item 11 holds (`wire.rs:826`, `:850`, `crates/riff/src/text.rs:3333`).
  - Item 12 holds in part. `hook.rs` is gone (#1200). `audit.rs` now calls `is_fresh()` (`crates/riff/src/audit.rs:522`). The predicates stay at `wire.rs:2233` and `:2238`, `crates/riff-server/src/trace.rs:301`, `crates/riff/src/rollout.rs:767`, and `crates/riff/src/pr.rs:2974` (it moved from `wave.rs`).
  - Item 13: `enable.rs` is gone (#1200). `mcp.rs` is now `tools.rs` (#1200), and its wildcard is at `crates/riff/src/tools.rs:771`. The `Class` wildcard in `command.rs` is gone (#858).
- The clippy count at f789f08e is now 121 distinct hits. The review counted 47. The hits are 86 in src and 35 in tests. Of the 86 in src, 75 are on riff enums: 30 in the TUI, 10 `Change` and 35 others. The other 11 are on enums of other crates. Most of the new hits came with the daemon, the wave, the release cycle and the TUI, after the review.
- Related requirement: 01M49W18QQKF4KYDRYP3ZK9F1Q (`docs/src/requirements.md:4333`) says that a type of the log, the wire, the checkpoint or the state cannot hold an invalid state. The section `## Code` (`docs/src/requirements.md:4193`) is the place for the new rule.

New title: Code hygiene: states as enums with an exhaustive match, and the lint wildcard_enum_match_arm (daemon.rs first)

New body:

```markdown
Mike, in a review of `crates/riff/src/daemon.rs`: "This doesn't look idiomatic to me. I'd like to see exhaustive matching on enums."

`SessionState::phase` (`crates/riff/src/daemon.rs:1996`) is an if-chain over `record.terminal`, `record.takeover`, `self.held` and `record.closed`. The order of the chain decides the phase. So the record can hold states that never make sense, and a new state needs a new flag and a new place in the chain. 01M49W18QQKF4KYDRYP3ZK9F1Q says that a type of the log, the wire, the checkpoint or the state cannot hold an invalid state. #858 made the rules of `permits`, the sign-in errors and `Failed` exhaustive (01M51JBTEM10W68K6F267BB79N). The rest of the workspace is not exhaustive yet.

The rule: where flags together encode one state, make that state an enum, and decide on it with an exhaustive `match`. Put no `_ =>` arm on an enum of riff. Then a new variant breaks the build at each place that decides on it. A stored or a sent form keeps its wire form, or the change says how an old one reads.

This list replaces the review comment of 2026-10-16 (at 78a645d). That comment is out of date.

## The places, by value (origin/main f789f08e)

1. **The session state of the daemon. Stored.** `daemon.rs:1865` `Record { closed, failed, terminal, takeover, interrupt }` (`live/NAME.json`), `:1955` `SessionState { held, ended }`, and `:1037` `StartSpec.terminal`. The same flags decide again in `go_on` (`:2013`), `wake` (`:2048`), the relay hello and `Up::Held` (`:2206-2297`), the quiet limit (`:4862`, `:4873`) and `takeover` (`:5491`). `terminal` decides at 39 places in `daemon.rs`. Change: `terminal` becomes `kind: Kind { Worker, Lead }`. `closed`, `takeover`, `held` and `interrupt` become one phase: `Works`, `Ends`, `Holds`, `Taken`, and `GivesBack` (`held && !takeover`, `:2221`). `failed` stays a `bool`. An old record file still reads (`#[serde(from = ...)]`).
2. **The relay. Sent.** `crates/riff/src/session_relay.rs:470` `Link { stdin: Option, closed, exit, deadline, done, hold, held: Option }`. `close()` (`:603`) takes `stdin` and sets `closed`: two fields for one fact. `daemon.rs:1067` `SessionAsk::Relay { closed, held }` is sent at `session_relay.rs:724`. Change: one enum for the life of `claude`, and one value in the hello. A daemon still reads the two bools of an older relay (`PROTOCOL`, `daemon.rs:523`).
3. **The sign-in mode of the server. Security.** `crates/riff-server/src/auth.rs:90` `require_sign_in: bool` and `:98` `provider: Option<Provider>`. `trusted()` is at `:172`. `lib.rs:327` `SignInStore { needs_sign_in, trusted }`. `listen.rs:230` `check(listen, trusted, insecure, owned)` takes three bools. Change: one enum, for example `Access { Trusted, Required { provider } }`.
4. **Decisions that fail open through a wildcard. Security.** `crates/riff/src/daemon/remote.rs:739`: which `Request` a lead may send to a daemon. `daemon.rs:4691`: only `ActsAs::NoToken` refuses a start (01M6Z87A04EXTT1FFJSJ1EM6TB). `crates/riff-server/src/engine.rs:1330`: only `MemberRemoved` and `SigninsEnded` end sign-ins. Change: name each variant.
5. **`Change`, 10 hits in src.** Decisions: `crates/riff-core/src/record.rs:526` and `:538` (`Record::other`), and `crates/riff/src/audit.rs:574`. Projections: `record.rs:642` and `:652`, `crates/riff-state/src/state.rs:802` and `:810`, `state/sessions.rs:446`, and `state/threads.rs:485`. Change: `let ... else`, or one accessor on `Change` with an exhaustive `match`.
6. **The steps of a block. Sent.** `crates/riff-state/src/state/presence.rs:836` `Block { answered, woken_again, unanswered }` and `crates/riff-core/src/wire.rs:921` `BlockedInfo` (three bools in the `who` reply). Change: one enum (open, woken again, unanswered, answered).
7. **The worker marks. Stored and sent.** `crates/riff-state/src/state/sessions.rs:75` `Known { worker, must_clear }`, `:256` `SavedSession` (the checkpoint) and `wire.rs:639` `SessionInfo`. `must_clear && !worker` never makes sense. Change: `Mark { Session, Worker { must_clear } }` in `Known`. Keep the checkpoint form and the wire form.
8. **Derived states. Less value.** `wire.rs:826` `Facts` (8 bools) and `:850` `SessionState::of`, and `crates/riff/src/text.rs:3333` `SessionKind::of`. Both are if-chains. Change: a `match` on small enums.
9. **Predicates with `matches!`. The lint does not see them.** `wire.rs:2233` `is_fresh` and `:2238` `is_start`, `crates/riff-server/src/trace.rs:301` `proved`, `crates/riff/src/rollout.rs:767` `in_verify`, and `crates/riff/src/pr.rs:2974` `full_done`. Change: an exhaustive `match`.
10. **Each other wildcard on an enum of riff.** The lint finds each one. With the places above, it gives 75 hits on enums of riff in src at f789f08e, 30 of them in the TUI (`tui.rs`, `tui/*.rs`). Change: name each variant.
11. **Enums of other crates (11 hits).** `serde_json::Value` (5) and crossterm (5): name each variant, or use `#[expect]` with a reason where the list is too long. `secret_service::Error` (`crates/riff/src/keyring_store.rs:255`) is `#[non_exhaustive]`: `#[expect]` with a reason.

What stays: an if-chain over numbers, or over a value of another crate (for example an HTTP status), and `==` on an enum with two variants.

## The lint

The workspace has no `[workspace.lints]`. Add `[workspace.lints.clippy] wildcard_enum_match_arm = "warn"` to `Cargo.toml`, and `[lints] workspace = true` to each crate. `just lint` runs clippy with `-D warnings`, so each hit stops `just check`. For an exception, write `#[expect(clippy::wildcard_enum_match_arm, reason = "...")]`, not `#[allow]`.

At f789f08e, `cargo clippy --workspace --all-targets -- -W clippy::wildcard_enum_match_arm` gives 121 distinct hits: 86 in src and 35 in tests. In the tests, most hits are projections of `Change` (15) and `Say` (10), and `let ... else` fixes them.

Done when:

- `SessionState::phase` is an exhaustive `match` on one enum. `Record` has no `terminal`, `closed` or `takeover` flag. A unit test reads an old `live/NAME.json`.
- The relay `Link` holds one enum for the life of `claude`. A unit test shows that the daemon reads the hello of an older relay.
- Each place on the list is fixed, or this issue says why it stays.
- `Cargo.toml` has the lint, each crate has `[lints] workspace = true`, and `just check` passes with zero warnings.
- The tests of the daemon, the relay and riff-state pass with no change of their meaning. The round trips pass: `cargo test -p riff-core --lib round_trip`.
- A requirement from `just req Code "No match on an enum of riff has a wildcard arm. The workspace lint clippy::wildcard_enum_match_arm is on. An exception has #[expect] with a reason."`.
- The rustdoc of each new enum names each state.
- Docs: a new heading `### Name each variant in a match` in `docs/src/development.md`, after `## Add a field or a type of the log or the wire`. It says what the lint refuses and shows the `#[expect]` form, with a `sh` block: `cargo clippy --workspace --all-targets -- -D warnings`.

Related: #858, #854, #1198, #1199, #1200
```
