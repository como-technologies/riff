# Backlog batch 2: #852 to #984

Issue: #1887, part of #1885. The facts are of `origin/main` at commit
f789f08e (Release v4.15.0, #1848). Each path and line is of that
commit. Each issue of the batch was open when I read it.

| Issue | Verdict |
|---|---|
| #852 | rewrite |
| #910 | keep |
| #928 | keep |
| #936 | keep |
| #958 | question |
| #962 | question |
| #963 | question |
| #964 | question |
| #965 | question |
| #967 | close |
| #972 | rewrite |
| #974 | rewrite |
| #977 | question |
| #981 | rewrite |
| #984 | close |

The six phone and encryption issues (#958, #962, #963, #964, #965,
#977) share one open question: does Mike keep the phone app? #1226
(open, Backlog) lists "devices and the phone app (#962,
riff-mobile.md)" for a keep, cut or merge decision, and has no answer.
No wave plans the phone or the encryption. Each of the six sections
asks it, so that each issue gets its own questions.

## #852 The claim of a release item finds its pull request by the trailer, and the how-to shows the trailer with grep

Verdict: rewrite

Why:

- Point 1 does not fit the design of today. No session claims a
  release item now:
  - The wave starts no session, no build and no verify for a release
    issue (01M5NCZ6CVCSTN3ZZDWNVGFG5S, `docs/src/requirements.md:5809`).
  - A release pull request needs no model verify: `riff release verify
    N` sets the status (01M5ND0ZNFFAPT1AWD3DP1Y77N,
    `docs/src/requirements.md:7308`).
  - "A release issue and a tracker are no item"
    (`docs/src/requirements.md:7498`). The free work leaves them out:
    `.filter(|i| !i.release() && !i.tracker())`
    (`crates/riff/src/rollout.rs:731`).
- The gate of point 1 is still in the code, but it does no harm now.
  `rollout::at_claim` asks `gh` only when the clone knows a pushed
  branch of the item (`crates/riff/src/rollout.rs:1996`). A release
  pull request is on `release-vX.Y.Z`: #1848 is on `release-v4.15.0`,
  with the trailer `Issue: #1847`.
- Point 2 is still true, and worse than the issue says. The how-to
  "See the item of a pull request"
  (`docs/src/how-it-works.md:6101`) runs `gh pr view 40 --json
  headRefName,body --jq '.headRefName, .body' | tail -n 3`
  (`docs/src/how-it-works.md:6110`). On #1848 it prints
  `Issue: #1847`, `Milestone: Wave 53` and an empty line. The branch,
  the first line of the output, never shows.
- No merged pull request changed the how-to since #851 (64114ee7).

New title: The how-to "See the item of a pull request" hides the
branch, and can hide the trailer

New body:

```markdown
The how-to "See the item of a pull request" in `how-it-works.md` gives the branch and the body of a pull request to `tail -n 3`. The body ends with a newline, so the output is the last two lines of the body and an empty line. The branch never shows. A body that ends with blank lines also hides `Issue: #N`.

Done when:
- The how-to shows the branch of the pull request, and its trailer `Issue: #N` with `grep '^Issue:'`.
- A book test checks the `sh` block of the how-to: it has `grep '^Issue:'` and no `tail -n 3`.
- Docs: "See the item of a pull request" in `how-it-works.md`.

Needs: none.
```

## #910 The git config reader of real-session goes on after a comment that ends in a backslash, so it misses the next key

Verdict: keep

Why:

- The fault is still in the code. `real_session::git_config`
  (`crates/real-session/src/lib.rs:1142`) joins a line that ends in an
  odd count of backslashes with the next line
  (`crates/real-session/src/lib.rs:1151` to `1156`). Only after that
  does it remove the comment: `without_comment(rest)`
  (`crates/real-session/src/lib.rs:1173`).
- A copy of `git_config` in a scratch program gives only
  `core.editor=vi` for the sample of the issue, and for a full-line
  comment that ends in a backslash. `git config --file F --list` gives
  `core.editor=vi` and `core.hookspath=/tmp/x` for both.
- The home check of the real-session test still uses it: it reads each
  git config of the home with `read_git_config`
  (`crates/real-session/src/lib.rs:882` and `918`), for
  01M51X6JA3W2CMV1S5VZEY7FC3.
- The doc test of `git_config` has a continued line `Name = Ada \`,
  but no comment that ends in a backslash
  (`crates/real-session/src/lib.rs:1120`).

## #928 Small points of the verifies of Wave B that stay: the signed.rs comment and the durable revoke

Verdict: keep

Why:

- Point 1 is still true. `crates/riff/tests/signed.rs:62` to `65` says
  "a swap for a session token saves nothing
  (01M52JYGWD3ZJKNYE3HG5BJYTP)". That requirement says that the token
  store keeps the hash of each session token
  (`docs/src/requirements.md:3712`). The rustdoc of the swap says that
  "the swap is a change of it" (`crates/riff-server/src/lib.rs:3160`
  to `3162`).
- Point 2 is still true. A reused refresh token ends the sign-in in
  memory only: `self.revoke(id); return Err(Refused::Reused);`
  (`crates/riff-server/src/token.rs:717` and `718`). Only
  `Service::save_each_second` (`crates/riff-server/src/lib.rs:1912`)
  writes it later. The reply of `refresh()`
  (`crates/riff-server/src/lib.rs:3101`) does not wait for the write,
  and the revoke is no log record.
- The test `a_used_refresh_token_still_revokes_after_a_restart`
  (`crates/riff-server/src/token.rs:1994`) tests the store alone. No
  test stops riff-server between the revoke and the save.
- For the worker: 01M53T8XH45WSVKS9FV3VXF2X8 says that riff-server
  replies to a refresh before it saves the token store
  (`docs/src/requirements.md:1384`). A reply that waits for the write
  needs a new requirement that supersedes it in part.
- The peer points of the comments are gone with the peers code
  (#1021): `peer/talk.rs` and `riff-3-peers.md` do not exist. The body
  already drops them.

## #936 riff cloud log shows no instance or revision, and prints each JSON log line of riff-server as an empty line

Verdict: keep

Why:

- riff passes the output of `gcloud` through as it is. `riff cloud
  log` (`crates/riff/src/main.rs:926`) calls
  `gcloud.show(&cloud::log_args(...))`
  (`crates/riff/src/main.rs:3198`). `log_args` builds `gcloud run
  services logs read SERVICE` with no `--format`
  (`crates/riff/src/cloud.rs:1976` to `1990`). `Gcloud::show` runs it
  on the terminal of riff (`crates/riff/src/cloud.rs:216`).
- Each log line of riff-server is one JSON object
  (`crates/riff-server/src/logline.rs:1` and `2`). So Cloud Logging
  keeps it as `jsonPayload`, and the text form of `gcloud` shows no
  `message` for it. The verify of #923 saw the empty lines (comment
  6222236344). I could not run `gcloud` here.
- The test `log_reads_the_lines_of_the_service`
  (`crates/riff/tests/cloud.rs:1293`) checks only the arguments.
- The release check already reads the revision with its own
  `gcloud logging read` and
  `--format=value(timestamp,resource.labels.revision_name)`
  (`crates/riff/src/cycle.rs:1911`). `riff cloud log` does not use it.
- The how-to is "See the shared log" (`docs/src/development.md:7779`).
  "Count the 500s after a deploy" (`docs/src/development.md:7457`)
  counts the lines of `riff cloud log` with `wc -l`. A new form must
  keep one line for each entry.

## #958 End-to-end encryption of each riff message, also at rest; a message to a person only that person can read (design)

Verdict: question

Why:

- Nothing of it is built. No crate of the workspace encrypts: no
  `openmls`, `vodozemac`, `age` or `hpke` in a `Cargo.toml`. Messages
  are signed only (`crates/riff-core/src/signed.rs`).
- Most facts of the body still hold:
  - `KEEP_MESSAGES` is 200 (`crates/riff-state/src/state.rs:328`).
  - riff-server keeps its state in a bucket of Cloud Storage
    (`crates/riff-server/src/gcs.rs`).
  - `keyring-core` is in `crates/riff/Cargo.toml:32` and in
    `crates/riff-client/Cargo.toml:21`.
- Some facts are stale:
  - The device key and its keyring moved to
    `crates/riff-client/src/device.rs:1` to `7` (#997).
  - The bucket has a name for each instance, `{project}-{name}-state`
    (`crates/riff/src/cloud.rs:523`).
  - `@USER` is now a name that riff-server resolves
    (01M6Y1A5PTCVZ6GRJ0FXMART2T, `docs/src/requirements.md:3320`).
  - A new requirement comes from `just req`, not `just rid`
    (CLAUDE.md).
- The body says "It does not need a key of a daemon". That was for
  the daemon of the peers. Today a container session asks the riff
  daemon of its host to make each call to riff-server: `ask_server`
  (`crates/riff/src/session_relay.rs:1121` to `1124`).
- The phone design waits for this design: it chooses the encryption
  key of a phone (01M53NTSC8C86B8S0HN0A5XAHB,
  `docs/src/requirements.md:4552`, and "What the phone needs from
  #958" in `docs/src/riff-mobile.md:256`).
- No other issue covers this: a search for encrypt, e2e, end-to-end,
  at rest, openmls and vodozemac finds only #958.

Questions for mike:

- Q1: Do you keep the end-to-end encryption, and when? No wave plans
  it, and #1226 asks to narrow riff. Proposal: keep it in the Backlog
  with no wave. Do it before the app of #963, because the pair of a
  phone must admit its encryption key (01M53NTSC8C86B8S0HN0A5XAHB).
- Q2: Who decrypts for a session in a container? Today the riff daemon
  of the host makes each call to riff-server for its sessions.
  Proposal: the daemon holds the key of the machine and decrypts for
  its sessions. A container gets no key, as it gets no token of the
  person (#1838).
- Q3: The facts of the body are stale in parts (see Why). Proposal:
  after Q1 and Q2, the lead puts the new facts in the body: the device
  key in `riff-client`, the names that riff-server resolves, the
  bucket of each instance, the daemon that makes the calls, and
  `just req`.

## #962 riff device add, list and remove: the devices of a person, the pair of a phone with a QR code and a Google sign-in, and the end of one device

Verdict: question

Why:

- Nothing of it is built. No code has `riff device`, a pair code, a
  phone scope or a token exchange of a phone. A sign-in holds no kind,
  no name and no scope (`crates/riff-server/src/token.rs:12` to `14`).
- The design is signed off: Mike wrote "This is GEN;SET. approved" on
  #951 (2026-10-17). But `docs/src/riff-mobile.md:5` to `7` still says
  that the build items wait for that sign-off.
- #1657 (open, Wave 55) builds `riff device` first, for FIDO2 keys,
  with another form: `riff device add --key NAME`. This issue and
  01M53XPZTG203SNM9MBKBERMB5 (`docs/src/requirements.md:4454`) have
  `riff device add [NAME] [--kind KIND]`, with the default kind
  `phone`. This issue still says "Needs: none".
- The scope of a phone leaves out the names of the chat. The scope is
  the thread `chat` and the notices only (01M53NTRDKDBTCWX0QNFJY9D4K,
  `docs/src/requirements.md:4491`). But a chat UI reads the names of
  the chat at its start (01M6Y1EKKCCZ5215V49Z5T3HKH,
  `docs/src/requirements.md:3359`) with `/v1/names`
  (`crates/riff-core/src/wire.rs:248`).
- `riff remove --email` is now `riff remove EMAIL` (#1124,
  `crates/riff/src/main.rs:425` to `428`). The body and
  01M53NTRGP2MJG6JT85BCVYE18 (`docs/src/requirements.md:4501`) still
  have the flag.
- The TUI Devices tab (#1642, Backlog) also waits for `riff device`.

Questions for mike:

- Q1: Do you keep the phone app? #1226 asks it, with no answer.
  Proposal: keep it in the Backlog. The design is signed off (#951),
  and the phone is the next UI after the TUI
  (01M54217C256KCGWSVAVPHEQQB, `docs/src/requirements.md:7898`).
- Q2: Which form names the kind of a device: `--key` of #1657, or
  `--kind KIND` of this issue? Proposal: the form of #1657, because it
  comes first: one flag for each kind (`--key`, then `--phone`), and
  no default kind. `riff device list` shows each kind in one list, and
  the names of the devices and the hosts of a person are one name
  space. A new requirement supersedes 01M53XPZTG203SNM9MBKBERMB5 in
  part.
- Q3: Does this issue wait for #1657? Proposal: yes. "Needs: #1657",
  and the lead writes the body again after #1657 merges, on its code:
  the form of Q2, the scope with `/v1/names`, and `riff remove EMAIL`.

## #963 The Slint app of the phone chat for Android: the chat, the key in the Keystore, the Google sign-in, the APK in each release

Verdict: question

Why:

- Nothing of it is built, and no CI job builds for Android.
- The core library exists: `crates/riff-client`
  (01M54216KMTGS7KD1R3RY5JKQ0). Its build for Android is #999 (open,
  Backlog). The body says "the core library of #982, which comes in
  riff 3.1", and "Needs: #962, #982". #982 is closed, and riff is on
  the line 4 (#1848).
- The chat words of the body are stale since #1771:
  - `@lead` is gone. `@riff` wakes the lead of the project riff
    (`docs/src/how-it-works.md:3597`).
  - The names are read in `riff-client`
    (`crates/riff-client/src/chat.rs`), not in `riff-core`.
  - The marks of 01M3NJD3BR0XAYNNFTEY0CG761 are in the history
    (`docs/src/requirements-history.md:632`). The rule of today is
    01M6Y1FB5MZNZ971CJRBWZ2F9X (`docs/src/requirements.md:3302`).
  - #1808 (open, Backlog) puts the phone page and two of its
    requirements on the new names.
- The key in the Keystore has no hook in `riff-client`. The design
  wants a key that cannot be exported (01M53NTR7D6145FSQ3WVZ1H52W,
  `docs/src/requirements.md:4481`). But the platform of `riff-client`
  gives only a store of `keyring-core`
  (`crates/riff-client/src/platform.rs:63` to `69`), and
  `device::key` loads the private key from it
  (`crates/riff-client/src/device.rs:33` to `37`).

Questions for mike:

- Q1: Do you keep the phone app? Proposal: as Q1 of #962. One answer
  there counts for each phone issue.
- Q2: Where does the signer of the Keystore go? Proposal: a sign hook
  in the platform of `riff-client`, as one more Done when line of #999.
- Q3: Proposal: after Q1, the lead writes the body again: "Needs:
  #962, #999", the chat names of #1771 (after #1808), and the marks of
  01M6Y1FB5MZNZ971CJRBWZ2F9X.

## #964 Notices on the phone: a local notice while the app runs, and an FCM notification message with a fixed text in the background

Verdict: question

Why:

- Nothing of it is built: no FCM or Firebase code in `crates/`.
- The rule of the notices is stale. The body and
  01M53NTRZT0B7539TXA5BV2BR8 (`docs/src/requirements.md:4521`) take
  the selector with the user and `lead=true`, "the selector that
  `@USER` makes". Since #1771, a chat line has one selector `name=NAME`
  for each `@NAME`, and riff-server resolves it
  (01M6Y1A5PTCVZ6GRJ0FXMART2T, `docs/src/requirements.md:3320`). The
  lead answers with a name too (01M6Y1FB84QN0C70AJ5K1JCNMT,
  `docs/src/requirements.md:3313`).
- The highlight of a terminal also counts `USER:` at the start of a
  line (01M6XSHCZB0Y08XRJ3J16Q4RTF, `docs/src/requirements.md:3244`).
  riff-server reads no body, so it cannot see that.
- #1808 does not name 01M53NTRZT0B7539TXA5BV2BR8.
- "Set up the cloud project" exists (`docs/src/development.md:6296`).

Questions for mike:

- Q1: Do you keep the phone app? Proposal: as Q1 of #962.
- Q2: Which lines give a notice on a phone? Proposal: a line of
  another sender whose names riff-server resolved to the person of the
  phone, and each line of the verified lead of that person. A `USER:`
  at the start of a line gives no notice, because riff-server reads no
  body. A new requirement supersedes 01M53NTRZT0B7539TXA5BV2BR8, and
  #1808 takes it.

## #965 The live check of riff chat on a phone: Mike installs the APK, adds the phone, chats, gets a notice and removes the phone

Verdict: question

Why:

- It needs #964 and #977, and nothing of them is built.
- Two words are stale since #1771:
  - A line from the phone shows as `<mike>`, not `<mike@NAME>`. The
    host shows only in the detail of a line
    (01M6Y1FB5MZNZ971CJRBWZ2F9X, `docs/src/requirements.md:3302`).
  - `@riff` wakes the lead of Mike, not `@lead`
    (`docs/src/how-it-works.md:3597`).

Questions for mike:

- Q1: Do you keep the phone app? Proposal: as Q1 of #962.
- Q2: Proposal: after Q1, the lead puts `<mike>` and `@riff` in the
  body. Each other line holds.

## #967 Docs: the flowchart of "The test for each release" says "major after 1.0" with no word of the line 1.x

Verdict: close

Why:

- The rule that the issue wants in the flowchart is gone.
  01M53N7KN4QKZ70W2GNQ3VQWQ9 (on the line 1.x, a major is a minor) is
  in `docs/src/requirements-history.md:1816`, "Superseded by
  01M5RG6BTXAJY8DV6MT315VZWF".
- 01M5RG6BTXAJY8DV6MT315VZWF says that riff supports the latest
  release only, and has no text that keeps an older release working
  (`docs/src/requirements.md:4158`). #1435 removed the text of the
  line 1.x from "Make a release".
- The flowchart says "minor (major after 1.0)"
  (`docs/src/how-it-works.md:988`). That is the rule of today,
  01M3N73EEQ4HPCPPAGCAHH3S6B (`docs/src/requirements.md:4015`).

## #972 top_fault tests: a budget equal to the refresh, and a fault after midnight

Verdict: rewrite

Why:

- Fact 1 is still true. The budget of the tests is 3 seconds
  (`const BUDGET`, `crates/riff/tests/top_fault.rs:29`), and the
  refresh of `riff top` is 3 seconds too (`pub const REFRESH`,
  `crates/riff/src/top.rs:172`).
- Fact 2 is still true. `stays_open` reads the time of the line as
  `HH:MM:SS` only (`crates/riff/tests/top_fault.rs:360` and `361`). It
  builds the expected lines with the same time as `last` and as `now`
  (`crates/riff/tests/top_fault.rs:377`). `top_fault` adds the date
  when `now` is on another day (`crates/riff/src/text.rs:81` to `85`).
  So a good look before midnight and a fault after midnight fail the
  parse.
- The Done when line asks for "a test with a fake clock". riff has no
  fake clock: `riff top` runs as a child process of the test, and
  takes the time of the machine. The new body tests the read of the
  line in its place.
- No merged pull request changed the file for this since #969
  (c111e71b).

New title: top_fault tests: a budget that differs from the refresh,
and a fault after midnight

New body:

```markdown
Found by the verify of #952 (PR #969).

- `BUDGET` of the tests in `crates/riff/tests/top_fault.rs` is 3 seconds, the same as `top::REFRESH`. So a test cannot tell a line that names the budget from a line that names the refresh.
- `stays_open` reads only `HH:MM:SS` after "riff: no good look since ", and builds the expected line with the day of the fault. When the good look is before midnight and the fault after it, `top_fault` adds the date, and the test fails.

Done when:
- `BUDGET` differs from `top::REFRESH`, for example 4 seconds.
- `stays_open` reads the time of the line with or without a date, and builds the expected line with that day.
- A unit test of that read gives it a line of `top_fault` with `now` on the next day, and the read passes.
- Docs: none: the line of `riff top` does not change.

Needs: none.
```

## #974 Tests of riff run leave a riff relay and a fake claude that run with no end, and they keep a merged worktree

Verdict: rewrite

Why:

- Each cause is still in the code:
  - `Machine` holds a plain `daemon: Option<std::process::Child>`
    (`crates/riff/tests/run.rs:473`). `impl Drop for Machine` kills
    and waits for the daemon only (`crates/riff/tests/run.rs:486` to
    `493`).
  - `fake_claude` (`crates/riff/tests/run.rs:244`) still has the two
    loops with no end: `while [ ! -f "$dir/$stage.go" ]`
    (`crates/riff/tests/run.rs:268`) and the loop of `linger`
    (`crates/riff/tests/run.rs:275`).
- No test and no `just` step looks for a process that a test left.
- "Why it matters" is stale. `riff worktrees clean` is gone (#1199),
  and `riff workers reap` is gone (#1197). The leak still costs: each
  fake `claude` wakes 20 times each second until a reboot.
- #984 is the same fault for the daemons of `tests/daemon.rs`. One
  guard fixes both. So this body takes in #984.

New title: A test that ends before its stop leaves no riff daemon,
riff relay or fake claude running

New body:

```markdown
A test of `crates/riff/tests` that fails or panics before its stop leaves processes that run until a reboot. Each fake `claude` wakes 20 times each second. This item takes in #984.

- `Here::daemon` and `Here::daemon_with` (`tests/daemon.rs`) give a plain `std::process::Child`. A drop does not stop it. `tests/ps.rs` uses them too.
- The fake `systemctl` of `tests/run.rs` starts a `riff daemon` in the background at a `restart`. Only `bin/daemon.pid` names it.
- `Machine::drop` (`tests/run.rs`) kills only the daemon. The `riff relay` and the fake `claude` of each session stay, with `systemd --user` as their parent.
- Two wait loops of `fake_claude` do not end when their folder is gone: the loop of `$stage.go` and the loop of `linger`.

Done when:
- One guard in `crates/riff/tests/common` stops a process of a test, and each process that it started, at its drop. `Here::daemon`, `Here::daemon_with`, `Machine` and the daemon of the fake `systemctl` use it.
- Each wait loop of `fake_claude` ends when its folder is gone.
- A test drops a `Machine` with a live session and no stop, and sees that the daemon, the relay and the fake `claude` are gone.
- A test drops the guard of a daemon of `Here` with no stop, and sees that the daemon is gone.
- Docs: a how-to "Stop each process that a test starts" under "Write a test" in `development.md` names the guard.

Needs: none.
```

## #977 The QR scan of the phone pair: the NDK camera and rqrr, with the typed code as the fallback

Verdict: question

Why:

- Its facts still hold. `rqrr` and `tui-qrcode` are in the
  requirements of the phone (01M53WZ032G5W435GJMPVBHD71,
  `docs/src/requirements.md:4472`, and 01M53NTR47XBDX6C6QN49RNP29,
  `docs/src/requirements.md:4462`). Nothing of it is built.
- It needs #963. The heading "Chat on your phone" that its Docs line
  names does not exist yet: #963 makes it.
- Its only open point is the question of the whole phone app.

Questions for mike:

- Q1: Do you keep the phone app? Proposal: as Q1 of #962. With a
  keep, this issue stays as it is.

## #981 Docs: "When the riff tools cannot start again": link it from the table, fix the diagram, and say that riff top does not mark a riff mcp of 1.5.0

Verdict: rewrite

Why:

- The book part is gone. #1200 (for #1173) removed the `riff mcp` of
  a session on the host, its hooks, and the section "When the riff
  tools cannot start again" with its diagram (376ab967).
  - The table "Which processes start again"
    (`docs/src/how-it-works.md:848`) has no row of `riff mcp`.
  - "Update a session that runs an old riff"
    (`docs/src/how-it-works.md:1872`) already says that a session of a
    riff from before the mark gets no mark
    (`docs/src/how-it-works.md:1893` and `1894`).
- The third Done when line wants the book to name a version of riff.
  01M5RG6BTXAJY8DV6MT315VZWF (`docs/src/requirements.md:4158`) and
  CLAUDE.md forbid that.
- Point 4 (the `stuck-PID` files) is now dead code. Nothing writes the
  files, and nothing calls the code of the stuck line:
  - `crates/riff/src/binary.rs`: `STUCK_AFTER` (line 223), `Stuck`
    (line 228), `stuck_mcp` (line 242) and `stuck_at` (line 281). The
    module rustdoc (lines 39 to 45) still tells of the hook, and cites
    01M53QQRAJMB24B0804YHPMN18, which is in
    `docs/src/requirements-history.md:1755`.
  - `crates/riff/src/text.rs:3710`: `stuck_mcp`.
  - `crates/riff/src/local.rs`: `first_tell` (line 297), and the row
    `stuck-PID` of the table of files (line 30).
- The pull request #1200 says that more dead helpers stay in
  `local.rs`, `binary.rs`, `rollout::run` and `look::run`. #1720 is
  the open issue of stale code. The new body keeps to the code of
  point 4.

New title: Remove the code of the line for a stuck riff mcp, which
nothing calls since #1200

New body:

```markdown
#1200 removed the `riff mcp` of a session on the host, its hooks and the section "When the riff tools cannot start again". The code of the line for a stuck `riff mcp` stays, and nothing calls it. Its rustdoc cites 01M53QQRAJMB24B0804YHPMN18, which is in the history, and says that a hook writes `stuck-PID` files.

- `crates/riff/src/binary.rs`: `STUCK_AFTER`, `Stuck`, `stuck_mcp`, `stuck_at`, and the paragraph of the module rustdoc on a stuck `riff mcp`.
- `crates/riff/src/text.rs`: `stuck_mcp`.
- `crates/riff/src/local.rs`: `first_tell`, and the row `stuck-PID` of the table of files.

Done when:
- None of these items is in the code, and `just check` passes with no warning.
- No rustdoc names `stuck-PID` or 01M53QQRAJMB24B0804YHPMN18.
- Docs: none: the book names none of these parts.

Needs: none.
```

## #984 A test of tests/daemon.rs that fails before its stop leaves its riff daemon running

Verdict: close

Why:

- The fault is still in the code: `Here::daemon`
  (`crates/riff/tests/daemon.rs:1075`) and `Here::daemon_with`
  (`crates/riff/tests/daemon.rs:1080`) give a plain
  `std::process::Child`, and `Here` (`crates/riff/tests/daemon.rs:34`)
  has no `Drop`. Only `stop(child)`
  (`crates/riff/tests/daemon.rs:1151`) stops it. `tests/ps.rs:12` uses
  the same helpers.
- The new body of #974 takes in this fault, with the same guard and a
  test of its own. So this issue closes as a part of #974.
