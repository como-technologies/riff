# Backlog, batch 5: #1339 to #1408

The grooming of #1890, part of #1885. Each fact is from `origin/main`
at 206a3911, on 2026-11-11. A path with a line number is a line of that
commit. Each issue of the batch was open when I read it.

| Issue | Verdict |
|---|---|
| #1339 | close |
| #1340 | keep |
| #1341 | rewrite |
| #1345 | rewrite |
| #1349 | rewrite |
| #1356 | rewrite |
| #1357 | rewrite |
| #1358 | question |
| #1379 | rewrite |
| #1384 | question |
| #1385 | question |
| #1392 | rewrite |
| #1400 | close |
| #1405 | rewrite |
| #1408 | rewrite |

## #1339 broken_main.rs, the last integration test on a riff-server with no sign-in, moves to common::start_signed

Verdict: close

Why:

- PR #1335 did it. It merged at 2026-10-24 06:08 UTC, after the issue
  (04:48 UTC). It added the comment
  `crates/riff/tests/broken_main.rs:91`: "Unsigned on purpose:
  `look::once` posts as the person with the api of the lead session, so
  no one token can make its calls." (`git log -S` finds ce2c1a64,
  #1335.) The "Done when:" line accepts "a comment in it says why".
- The grep of the issue, in `crates/riff/tests`, lists
  `broken_main.rs` and `first_run.rs`. `first_run.rs` came later, and it
  says why in a comment too: `crates/riff/tests/first_run.rs:791-793`
  ("a riff-server with no sign-in: it has no session App") and :799.
- Five other `riff_server::router()` calls of a test have a comment
  that says why: `states.rs:31`, `restart.rs:19`, `chat.rs:29`,
  `run.rs:505` and `top_fault.rs:56`, in `crates/riff/tests`.
- One call has no comment: `behind_a_front_end`
  (`crates/riff/tests/chat.rs:1799-1804`). Its test goes through the
  `Proxy` (chat.rs:1833), so the reason of chat.rs:29 holds for it too,
  but no comment says so. The "Done when:" line names only
  `broken_main.rs`, so the close holds. The missing comment is one line
  in a later change of `chat.rs`, with no issue of its own.
- "Use a riff-server with a sign-in" (`docs/src/development.md:464`)
  names no exception. The "Docs:" part of the issue accepts that.
- #1153 is closed.

## #1340 The host lines of riff who and riff top show the effort of the lead, of a work item and of a verify

Verdict: keep

Why:

- The limit has the three levels: `effort`, `effort_lead` and
  `effort_verify` (`crates/riff/src/room.rs:194-199`,
  01M5P2MG28EE75PPGYAWHTY2S1).
- `riff daemon limit` and `riff daemon status` show them:
  `text::daemon_limit` says "Effort of the lead {}, of a work item {},
  of a verify {}." (`crates/riff/src/text.rs:9617`).
- The facts that the daemon posts have no effort:
  `riff_client::daemons::Facts` has no such field
  (`crates/riff-client/src/daemons.rs:451-516`), and
  `daemon::facts::of_status` sets none
  (`crates/riff/src/daemon/facts.rs:148-160`).
- `riff who` (`view::hosts`, `crates/riff/src/view.rs:643-657`) and
  `riff top` (`crates/riff/src/top.rs:1039-1053`) print the host lines
  with `text::daemon_facts`. Its first line has the build, the sessions
  against the limit and the floor, and no effort
  (`crates/riff/src/text.rs:7575-7592`).
- No pull request did it: `gh pr list --search effort --state all`
  finds #325, #641, #1333, #1343 and #1695. None changes the host
  lines. #325 and #641 came before the issue, and #1343 is the release
  v4.1.0.
- "Set the effort of the sessions" exists
  (`docs/src/development.md:3773`), as the issue names it.

## #1341 The WIP save of the daemon fails on a host with no git identity: Author identity unknown

Verdict: rewrite

Why:

- The fault is still there. The daemon saves an ended build with
  `save_work` (`crates/riff/src/wave.rs:2480-2490`), which calls
  `leave::save` and then `leave::wip`
  (`crates/riff/src/leave.rs:152-167`). `wip` commits with
  `-c commit.gpgsign=false` and no identity
  (`crates/riff/src/leave.rs:125-138`). git runs on the host, in the
  clone of the session folder.
- The identity of the bot is only in the environment of the container
  (`give_author`, `crates/riff/src/daemon/gh_token.rs:590-593`).
- The sweep commits as `riff <riff@localhost>` (`push_kept`,
  `crates/riff/src/daemon/sweep.rs:668-675`).
- Each test gives the clone an identity
  (`crates/riff/tests/save.rs:30-31`, the doc test at
  `crates/riff/src/leave.rs:88-89`).
- The body names commit 3ffb037, which is not on `main`: PR #1337
  merged as b7248d25. The body has no line numbers.

New title: The WIP save of an ended build fails on a host with no git identity

```markdown
## Facts

- The daemon saves a build that pushed nothing with `leave::save`
  (crates/riff/src/wave.rs:2480-2490). git runs on the host, in the
  clone of the session folder.
- `leave::wip` commits with no identity
  (crates/riff/src/leave.rs:125-138). On a host with no `user.name`
  and `user.email`, git fails with "Author identity unknown". The
  daemon prints the error (wave.rs:2486-2489). The work is not pushed,
  and the lead gets no message.
- The sweep of a kept folder commits as `riff <riff@localhost>`
  (crates/riff/src/daemon/sweep.rs:668-675), so it works with no
  identity.
- The identity of the bot is only in the environment of the container
  (crates/riff/src/daemon/gh_token.rs:590-593).
- Each test gives the clone an identity
  (crates/riff/tests/save.rs:30-31).

## What

- `leave::wip` keeps the identity of git when git has one. When git
  has none (`git var GIT_AUTHOR_IDENT` fails), it commits as
  `riff <riff@localhost>`, as the sweep does.
- A requirement records it.

## Done when:

- A test: `leave::save` in a clone with no identity commits and
  pushes, and the commit has the author `riff <riff@localhost>`. It
  runs with `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_NOSYSTEM=1` and
  `isolated::rerun_without_git_config`, as
  crates/riff/tests/save.rs:70 does.
- Docs: "When a session of the wave ends with no push" in
  docs/src/development.md says which identity the WIP commit has.
```

## #1345 After riff release cut, the next step says "wait for the verify", but only riff release verify N sets it

Verdict: rewrite

Why:

- A part is done. PR #1437 added the watch of the release pull
  request. When the Gate passes and the head has no `riff/verify`, the
  lead gets "next: riff release verify N"
  (`crates/riff/src/watches.rs:1401-1412`). A failed Gate gives "run
  the failed jobs again, then riff release verify N" (:1393-1395).
  "Verify the release pull request" (`docs/src/development.md:7223`)
  and "Cut the release pull request" (:7213-7217) name the command.
- The two lines of the issue still have the old words. `riff release
  cut` prints "next: wait for the verify and the merge of pull request
  #N" (`crates/riff/src/main.rs:3515-3518`). `cycle::next` gives the
  same words for `State::Cut` (`crates/riff/src/cycle.rs:862-865`),
  and `riff release status` prints them. The test at
  `crates/riff/src/cycle.rs:4148-4157` checks them.
- Only `riff release verify N` sets `riff/verify`
  (`crates/riff/src/cycle.rs:1181`, 01M5ND0ZNFFAPT1AWD3DP1Y77N).
- `Stage` keeps no state of the Gate or of `riff/verify`
  (`crates/riff/src/cycle.rs:672-691`).
- The book has the old words too: the diagram of "The release cycle"
  says "Verify by another session" (`docs/src/development.md:6557`),
  and its table says "a session that did not write the change" (:6587).

New title: riff release cut and riff release status name riff release verify N as the next step

```markdown
## Facts

- `riff release cut` prints "next: wait for the verify and the merge
  of pull request #N" (crates/riff/src/main.rs:3515-3518).
- `cycle::next` gives the same words for `State::Cut`
  (crates/riff/src/cycle.rs:862-865). `riff release status` prints
  them.
- Only `riff release verify N` sets `riff/verify`
  (crates/riff/src/cycle.rs:1181, 01M5ND0ZNFFAPT1AWD3DP1Y77N). No
  session verifies a release pull request.
- The watch of the daemon names the step already
  (crates/riff/src/watches.rs:1393-1412).
- "The release cycle" in docs/src/development.md says "Verify by
  another session" (:6557) and "a session that did not write the
  change" (:6587).

## What

- For `State::Cut`, `cycle::next` reads the Gate and `riff/verify` of
  the head of the release pull request, and gives:
  - the Gate runs: "wait for the Gate of pull request #N, then riff
    release verify N";
  - the Gate failed: "run the failed jobs again, then riff release
    verify N";
  - the Gate passed, and no `riff/verify`: "riff release verify N";
  - `riff/verify` passed: "wait for the auto-merge of pull request #N".
- `riff release cut` prints the line of `cycle::next`.
- A requirement records the next steps.

## Done when:

- A test: a unit test of `cycle::next` with a fake forge gives each of
  the four lines.
- A test: `riff release cut` prints "next: wait for the Gate of pull
  request #N, then riff release verify N".
- Docs: "The release cycle" in docs/src/development.md: the diagram
  and the table say that the lead runs `riff release verify N`.
```

## #1349 A release check that fails only on a host step proposes a fix release or a rollback

Verdict: rewrite

Why:

- The fault is still there. "each host takes the build" fails with
  the old hosts, and names no step
  (`crates/riff/src/cycle.rs:1839-1856`). One failed check makes the
  state `Failed` with `checks_failed` (:803-805). Then `cycle::next`
  gives "fix forward with riff release cut X.Y.Z, or roll back with
  riff release rollback vPREV" (:887-890).
- Two parts of the case were there before the issue. A check that
  cannot run does not decide: the stage counts only the marks "passed"
  and "failed" (comment at `crates/riff/src/cycle.rs:774-775`, PR
  #1160, merged 2026-10-19), so the next step stays `riff release check
  vX.Y.Z`. The Cloud Run log that riff cannot read names `gcloud auth
  login` (PR #1283, merged 2026-10-23). The issue came on 2026-10-24.
  So only the failed check of a host is open.
- A new check fails with the step of an admin: "riff-server reads the
  keys of the GitHub Apps" (`crates/riff/src/cycle.rs:1782-1789`,
  `crates/riff/src/text.rs:1804-1810`).
- The deploy watch now waits up to 30 minutes for each host with
  `update.auto` on (#1718, #1728, `docs/src/development.md:6932-6934`).
  A host with `update.auto` off, or a late host, still fails the check.
- #1324, the cause of the case in the issue, is closed.

New title: riff release check names the steps of a host or of a person when only such a check failed

```markdown
## Facts

- Two checks of `cycle::checks` (crates/riff/src/cycle.rs:1766) can
  fail with no fault of the release:
  - "each host takes the build" (cycle.rs:1839-1856): a host runs an
    old riff. It names no step.
  - "riff-server reads the keys of the GitHub Apps"
    (cycle.rs:1782-1789): an admin runs `git pull`, `riff cloud create
    shared` and `riff resume --riff` (crates/riff/src/text.rs:1804-1810).
- One failed check makes the release "failed" (cycle.rs:803-805). Then
  `cycle::next` gives "fix forward with riff release cut X.Y.Z, or
  roll back with riff release rollback vPREV" (cycle.rs:887-890).
- The note to the other leads says the same
  (crates/riff/src/text.rs:2621-2623).
- A check that could not run already gives `riff release check vX.Y.Z`
  as the next step (cycle.rs:774-775).

## What

- When each failed check is a step of a host or of a person, the
  `next:` line names those steps, then `riff release check vX.Y.Z`:
  - a host on an old riff: "a person on USER@HOST runs riff update";
  - the keys of the Apps: the steps of the admin of text.rs:1804-1810.
- A fix release or a rollback comes only when a check of the release
  failed: the build of `riff server`, a 503 run, a 500, a 401 or a
  429, or an old instance on the lease.
- The mark on the release issue keeps the kind of the failure, so
  `riff release status` and the note to the other leads give the same
  step.
- A requirement records it.

## Done when:

- A test: a unit test with only "each host takes the build" failed
  gives the host step, then `riff release check vX.Y.Z`, as `next:`.
- A test: a unit test with a 500 still gives the fix release or the
  rollback.
- Docs: "Check the release after the deploy" in
  docs/src/development.md has a table of each failed check and its
  next step.
```

## #1356 riff pr accept: only the owner or an admin, as a person, brings in an outside pull request; the wave verifies none before it

Verdict: rewrite

Why:

- Nothing of it is built. `riff pr` has only `open` and `wait`
  (`crates/riff/src/main.rs:1213-1258`). The look reads no head
  repository (`PULL_FIELDS`, `crates/riff/src/rollout.rs:782-783`), so
  nothing marks a pull request as outside.
- Three facts changed after the issue:
  - PR #1371 makes the clone of a verify fetch `refs/pull/N/head`, so
    the wave verifies a pull request from a fork
    (`crates/riff/src/wave.rs:2994-3003`, 01M5QVARWYFHTQV5THNRB09SZA,
    `docs/src/how-it-works.md:2616`).
  - PR #1536 tells the lead of each fork run that waits, with the `gh
    api ... /approve` command (01M6FZ144M0BTEDWNB9V4MGWDD, "Approve the
    run of a pull request from a fork",
    `docs/src/development.md:2914`). So an agent still approves it.
  - #571 is closed as a design. #1658 (open) puts "the accept of an
    outside pull request" in the approvals that need a proof under the
    policy `required`.
- `Caller::person_step` (`crates/riff/src/cycle.rs:2468`) already
  refuses each agent session and a member, for `riff release approve`.
- No code writes a thanks line: no "Thanks" in `crates/riff/src`.

New title: riff pr accept: only the owner or an admin, as a person, lets the wave take an outside pull request

```markdown
Asked by Mike (riff chat, 2026-10-24). An outside pull request has its
head in another repository, a fork. Example: #1348.

## Facts

- The wave takes a pull request from a fork as the work of its item:
  `Pull::issue` (crates/riff/src/rollout.rs:1398) reads `Issue: #N`,
  the branch, then `Closes #N`. The look reads no head repository
  (rollout.rs:782). Nothing marks a pull request as outside.
- The wave verifies a pull request from a fork
  (crates/riff/src/wave.rs:2994-3003, 01M5QVARWYFHTQV5THNRB09SZA).
- The lead gets the command that approves a fork run that waits
  (01M6FZ144M0BTEDWNB9V4MGWDD). The lead is an agent.
- Hygiene needs a milestone (crates/hygiene/src/lib.rs:70). The author
  of a fork cannot set one.
- `riff pr` has only `open` and `wait` (crates/riff/src/main.rs:1213).
- `Caller::person_step` (crates/riff/src/cycle.rs:2468) refuses each
  agent session and a member.

## What

- The look reads the head repository of each pull request. An outside
  pull request gets no verify, no rebase, no fix of checks and no
  message about a run that waits, until a person accepts it.
- `riff pr accept N`: only the owner or an admin, in a terminal, as
  `riff release approve`. It refuses each agent session, the lead too,
  also on the word of a person in chat. It approves each run that
  waits, sets the milestone of the current wave and the `Milestone:`
  line of the body, and moves the issue to the current wave. Under the
  policy `required` of #1658, it needs a proof.
- `riff pr outside` lists the outside pull requests that wait: the
  number, the author, the files and the size.
- After the merge, riff thanks the author in a comment on the pull
  request, and `riff release cut` puts "Thanks to @LOGIN for #PR" in
  the notes. The squash commit keeps the author and the
  `Co-authored-by:` lines.
- New requirements record each rule. They supersede
  01M5QVARWYFHTQV5THNRB09SZA and 01M6FZ144M0BTEDWNB9V4MGWDD in part.

## Done when:

- A test with a fake forge: the wave starts no verify for an outside
  pull request that is not accepted, and starts it after the accept.
- A test: `riff pr accept` refuses an agent session, the lead, and a
  member that is not an admin.
- A test: `riff release cut` puts the thanks line of a merged outside
  pull request in the notes.
- Docs: a new heading "Accept an outside pull request" in
  docs/src/how-it-works.md, with `sh` blocks for `riff pr outside` and
  `riff pr accept N`. "Approve the run of a pull request from a fork"
  in docs/src/development.md links to it.

Related: #1306, #1658, #1348.
```

## #1357 riff run --host HOST ITEM: run one item on the daemon of another host

Verdict: rewrite

Why:

- It is still valid: `riff run` takes ITEM, `--image` and
  `--no-github`, and no `--host` (`crates/riff/src/main.rs:667-680`).
- The list of commands with `--host` in the issue is old. `riff wave
  run` and `riff wave stop` are gone: `riff join` and `riff leave`
  replaced them (PR #1680). `riff view` is gone (PR #1701,
  `docs/src/riff-uis.md:120`). `riff release test`, `riff daemon
  status`, `riff daemon sweep` and `riff build` of the list still
  take `--host`.
- 13 commands ask the daemon of HOST today.
  01M579Z9MPS86MX7890T6HV18S names nine of them.
  - 12 with `--host HOST`: `riff join`, `riff leave`, `riff build`,
    `riff lead stop`, `riff release test` and seven `riff daemon`
    subcommands (the fields `host` at `crates/riff/src/main.rs:375`,
    :392, :589, :1198, :1397 to :1537 and :1681). On `riff release
    test`, `--host` is a flag that the command needs (`host: String`,
    main.rs:1196-1198, `Request::ReleaseTest` at
    `crates/riff/src/daemon/remote.rs:497-505`).
  - `riff lead move HOST`, with HOST as an argument (main.rs:1697-1700,
    `Request::LeadMove` at remote.rs:509-511).
- A part exists. In the lead container, `riff run ITEM` asks the
  daemon on the socket of the session to run the item as a job of the
  wave (PR #1378, 01M5QVX8FFG2FPWZEW61A985CF,
  `crates/riff/src/reach.rs:427-435`, "Run an item from the container
  of the lead", `docs/src/development.md:1643`). #1338 is closed.
- The request to the daemon of a host has no run
  (`crates/riff/src/daemon/remote.rs:445-515`).
- The new body follows the proposal of Q6 in #1358 below: it builds
  the flag now, with no wait for the design. When mike says no to Q6,
  the lead adds the line "Needs: #1358" again.

New title: riff run --host HOST ITEM: the daemon of HOST runs one item as a job of its wave

```markdown
Asked by Mike (riff chat, 2026-10-24).

## Facts

- `riff run` takes ITEM, `--image` and `--no-github`, and no `--host`
  (crates/riff/src/main.rs:667-680). The daemon of this machine runs
  the item.
- In the lead container, `riff run ITEM` asks the daemon on the socket
  of the session to run the item as a job of the wave
  (01M5QVX8FFG2FPWZEW61A985CF, crates/riff/src/reach.rs:427).
- `riff join`, `riff leave`, `riff build`, `riff lead stop`, `riff
  release test` and seven `riff daemon` subcommands take `--host HOST`
  (crates/riff/src/main.rs:375-1681, 01M579Z9MPS86MX7890T6HV18S).
  `riff lead move HOST` takes HOST as an argument (main.rs:1697-1700).
  Each of them asks the daemon of HOST. The request to the daemon of a
  host has no run (crates/riff/src/daemon/remote.rs:445-515).
- Only the person and the lead may ask the daemon of a host
  (01M579Z9PZZCVWQKG7V8WZ5KRB).

## What

- `riff run --host HOST ITEM` asks the daemon of HOST, through
  riff-server, to run ITEM as a job of the wave of the repository of
  this folder. It is the same job as `riff run` in the lead container.
- The daemon of HOST refuses a repository that it does not serve, and
  names `riff join --host HOST`. It refuses each session but the
  person and the lead.
- riff answers at once, and names `riff daemon status --host HOST`.
- With `--host`, riff refuses `--image` and `--no-github`.
- A requirement records it.

## Done when:

- A test in crates/riff/tests/daemon_hosts.rs with a real riff-server:
  `riff run --host b issue-12` from a terminal on `a` starts the job on
  the daemon of `b`, and a worker gets the refusal.
- A test: riff refuses `--host` with `--image` or `--no-github`.
- Docs: a new heading "Run an item on another host" in
  docs/src/development.md, after "Run an item from the container of the
  lead", with `riff run --host thelio issue-12` in an `sh` block.

Part of #1358.
```

## #1358 Design: one way to configure riff: --host as a global flag with RIFF_HOST, one config file of the user, and riff config shows each source

Verdict: question

Why:

- The design is not decided: the issue has no comment, and its two
  questions have no answer.
- Two facts of the issue are wrong:
  - riff has a config file of the user:
    `$XDG_CONFIG_HOME/riff/config.toml`, or `~/.config/riff/config.toml`
    (`crates/riff/src/settings.rs:1-12`, 01M3JPQT13ANVA7DNJDVNJ0S8P).
    It holds `workers.*`, `lead.*`, `update.auto`, `tui.mouse`, `chat.*`
    and more, but no server, host or color.
  - `RIFF_HOST` exists with another meaning: the name of this machine
    in the session URI (`crates/riff/src/identity.rs:10`, :295).
- `--server`, `--color` and a hidden `--place` are the global flags
  (`crates/riff/src/main.rs:36-48`). `--server` takes `RIFF_SERVER`,
  then `http://127.0.0.1:7878`. The book says "riff keeps no choice in
  a file" (`docs/src/how-it-works.md:249-250`).
- `crates/riff/src/main.rs` still has 12 fields `host: Option<String>`.
  `riff top --host` filters (main.rs:211-213). The 11 others ask the
  daemon of HOST. The issue also leaves out two fields `host: String`,
  and they ask the daemon of HOST too: `riff release test --host`, a
  flag that the command needs (main.rs:1196-1198), and `riff lead move
  HOST`, an argument (main.rs:1697-1700). So 13 commands ask the daemon
  of HOST. `riff view`, `riff wave run` and `riff wave stop` are gone
  (PR #1680, PR #1701).
- There is no `riff config` command. #1338 is closed. #1000 is open.
- Mike said "be careful about command sprawl" (#1658).

Questions for mike:

- Q1: `RIFF_HOST` names this machine today. Which variable names the
  host that a command acts on? Proposal: none. `--host` stays a flag.
- Q2: The config file of the user exists. Which settings join it?
  Proposal: the server only. The order is `--server`, then
  `RIFF_SERVER`, then `config.toml`, then this machine. No host and no
  color in the file.
- Q3: `riff config` in place of `riff server`, or both? Proposal:
  neither. `riff server` names `config.toml` as a source, as `riff
  update --auto` does with `(file)`.
- Q4: Is `--host` a global flag? Proposal: yes, for the 13 commands
  that ask the daemon of HOST and for `riff run` (#1357). `riff release
  test` refuses to run with no `--host`, as it does today. `riff lead
  move --host HOST` takes the place of `riff lead move HOST`. Each
  other command refuses `--host` with an error.
- Q5: The filter `riff top --host` then needs another name. Proposal:
  `riff top --on HOST`.
- Q6: Does #1357 wait for this design? Proposal: no. #1357 adds
  `--host` to `riff run` now, as the other commands have it. After the
  answers, the lead closes #1358 and files one item: "a global --host,
  and the server in config.toml", with the docs under a new heading
  "Choose the host and the server" in `docs/src/how-it-works.md`.

## #1379 The containers of a real-session test do not count in the limit of the host

Verdict: rewrite

Why:

- The fault is still there. The room starts a session only when `live
  + slots < sessions` and the free memory is over the floor
  (`crates/riff/src/room.rs:353-367`). `live` is `Daemon::live_count`:
  the sessions that the daemon owns, and not the lead
  (`crates/riff/src/daemon.rs:2926-2933`).
- `riff release test --host` runs the test in the daemon
  (`crates/riff/src/daemon/release_test.rs:1-12`). The test runs a
  daemon of its own, with its own data folder
  (`crates/real-session/src/lib.rs:146-158`), so its container is in
  no count of the daemon of the host.
- `Status::busy` counts the test as one, but only for the restart on a
  new riff (`crates/riff/src/daemon.rs:1610`, the check at :3816,
  01M5QVBTXE8YRY44A1C1VP71QS).
- PR #1375 left it out on purpose: "the containers of the test still
  do not count in the limit of the host".
- The body names a version of riff and has no line numbers. It says
  "each live container", but the test runs one container session.

New title: The limit of a host counts a real-session test that runs

```markdown
## Facts

- The daemon starts a session only when the live sessions and the
  starts that took a slot are fewer than the limit, and the free memory
  is over the floor (crates/riff/src/room.rs:353-367, :538-539). A
  start keeps its slot until the daemon owns the session
  (room.rs:449-450).
- The live sessions are the sessions that this daemon owns
  (crates/riff/src/daemon.rs:2926-2933).
- `riff release test --host` runs the test in the daemon
  (crates/riff/src/daemon/release_test.rs). The test runs a daemon of
  its own (crates/real-session/src/lib.rs:146-158), and one container
  session. The limit of the host does not count it. Only the floor of
  free memory sees it.
- `Status::busy` counts the test as one for the restart on a new riff
  (crates/riff/src/daemon.rs:1610, the check at :3816,
  01M5QVBTXE8YRY44A1C1VP71QS).

## What

- While a test runs (`release_test::running()` is `Some`), the room
  counts it as one live session, for each start and for the wait
  before a claim.
- A requirement records it.

## Done when:

- A test: a unit test of the room with a limit of 4, a test that runs
  and 3 live sessions: the next start waits, and it starts when the
  test ends.
- Docs: "See the test run on a host" in docs/src/development.md says
  that the test takes one place in the limit of the host.
```

## #1384 look::once and look::run have no runner since 4.0: the facts of the items, the blocks and the release look do not run

Verdict: question

Why:

- Two of the parts are done:
  - `look::run` is gone (PR #1618, which says "`look::once` stays...
    That is the scope of #1384").
  - The release look moved into the watches of the daemon (PR #1437,
    `crates/riff/src/watches.rs`).
- `look::once` (`crates/riff/src/look.rs:1531`) still has no caller
  but its tests: `crates/riff/tests/states.rs` and
  `crates/riff/tests/broken_main.rs:175`. Its module doc says so
  (`crates/riff/src/look.rs:33-37`).
- The look of the daemon is `look::look_wave`
  (`crates/riff/src/look.rs:1329`). It sends no full list of item facts
  and makes no `look_blocks` call. Only `once` makes them
  (`crates/riff/src/look.rs:1547`, :1553).
- The default fix of the issue needs a caller that may look.
  `look_blocks` takes only the lead: "only the lead looks at the blocks
  of its sessions" (`crates/riff-state/src/state.rs:1906-1909`). The
  look of the daemon posts as the person. But the daemon signs the
  calls of each session that it runs as that session, the lead too
  (`server_call`, `crates/riff/src/daemon.rs:1343-1348`). So the
  daemon that runs the lead can call `look_blocks` as the lead, with no
  change of riff-server.
- So a block gets only its first wake. There is no second wake after
  `lead.wake`, and no notification. `settings::lead_wake` and
  `settings::lead_notify` have no caller outside
  `crates/riff/src/settings.rs`: `crates/riff/src/look.rs:43` names
  `lead_wake` only in a doc link.
- The facts of one item are still sent, for example by `riff pr wait`
  and `riff verify`. The full list, which clears old facts, is not.

Questions for mike:

- Q1: Who looks at the blocks: the second wake, the "unanswered" state
  and the notification? There are two ways:
  - A: the daemon that runs the lead calls `look_blocks` as the lead,
    as `server_call` signs each call of a session. riff-server does not
    change. With no lead, no one looks, and there is no lead to wake.
  - B: riff-server lets the person look at the blocks of the sessions
    of its user, and `look_wave` makes the call as the person on each
    host of the wave.

  Proposal: A, at each look of `look_wave` on the host of the lead,
  with `lead.wake` and `lead.notify` of that host. It is the default
  fix of the issue, and it needs no change of riff-server.
- Q2: Does the daemon send the full list of item facts? Proposal: yes.
  `look_wave` reads the issues and the pull requests already, and it
  sends `item_facts` with `all: true` as the person.
- Q3: Is the release look done? Proposal: yes, by #1437. This issue
  does nothing for it.
- Q4: Does `look::once` go? Proposal: yes, after Q1 and Q2. Its tests
  move to `look_wave`. The words that name "the look of the lead" or a
  look of the `riff mcp` of the lead change:
  - `docs/src/how-it-works.md:4863`, :4874 and :5128;
  - `docs/src/design-plan.md:36`, :44, :73, :102, :419 and :429;
  - the requirements 01M41FZP2C4Z4J6WKRXZ5B31EH,
    01M41FZQ545HQ9Q75CSKX8HF8H, 01M41FZQKZKW131Z8822G31T5G,
    01M49Q30XMVRFX42YTM1PHX0RZ, 01M49Q316RXNATJP587DWGDNCD,
    01M51BF5S2YKB8JG10DF6YVT42, 01M54510SFK1Z9VPFYBYYSV10F,
    01M5R0J131464KZX180BW2AK98 and 01M6QBP23SQJV3FJB8SWDQWT1Z.

After the answers, the lead rewrites the issue as "The daemon look
sends the facts of the items and looks at the blocks; look::once
goes".

## #1385 A session that becomes the lead can lose its own read cursor: keep the later of the two

Verdict: question

Why:

- The fault is still there. `Presence::handed_over`
  (`crates/riff-state/src/state/presence.rs:552-565`) gives the new
  lead the cursor of the old lead, also when its own cursor is later
  (:563). No pull request changed it after #1381.
- The fix of the issue, "keep the later of the two", breaks the case
  that the design wants. A join sets the cursor to the newest message
  (`crates/riff-state/src/state/presence.rs:458-470`). In the test
  `a_new_lead_finds_only_the_messages_after_the_ones_the_lead_before_it_read_unread`
  (`crates/riff-state/src/state.rs:4414`), B starts after five
  messages, and the old lead read three. B must find two unread. With
  "the later of the two", B finds none. The test at
  `crates/riff-state/src/state/presence.rs:1194-1220` and "What a new
  lead reads" (`docs/src/how-it-works.md:5391`) show the same case.
- The cursors do not tell a read from a join
  (`crates/riff-state/src/state/presence.rs:64-66`).
- The cost of the fault is small: after `riff lead --session ID`,
  messages that the session read show again. Nothing is lost.

Questions for mike:

- Q1: Does riff fix it? Proposal: yes. Each cursor knows if a read or
  a join set it. At a hand-over, the new lead keeps its own cursor when
  a read set it and it is later. Else it takes the cursor of the old
  lead.
- Q2: Does the checkpoint keep that mark? Proposal: yes. A cursor of
  an old checkpoint counts as a read.
- Q3: Or close it under GEN;SET, because the only cost is messages
  that show twice? Proposal: no, fix it as in Q1, with a requirement
  that supersedes 01M5QVD3B1GT7STHBQS7NK4DR0 in part. The "Done when:"
  of the issue stays, and adds: the test at
  `crates/riff-state/src/state.rs:4414` still passes.

## #1392 riff session why shows plain lines, and --json shows the raw stream of claude

Verdict: rewrite

Why:

- The need still holds, and no pull request did it. `riff session why`
  takes only ITEM (`crates/riff/src/main.rs:1642-1651`).
  `text::session_why` prints the lines of the record as they are
  (`crates/riff/src/text.rs:10642-10673`).
- The fix of the issue cannot work on the records of today. The daemon
  cuts each line at 300 characters (`LINE_MAX`,
  `crates/riff/src/ends.rs:84`, :174, 01M5A5HXKZTDP5M5HY2FHCEBZA). A
  cut line is not JSON, so `View::shown`
  (`crates/riff/src/run.rs:1788`) gives it back as it is. The daemon
  must make the plain lines before the cut.
- Each command that prints data takes `--format text` or `--format
  json` (01M542168PYP1WH78D3D9DKEK6). So the flag is `--format json`,
  not `--json`. After the change, no record keeps the raw stream of
  claude: `--format json` prints the records.
- The reason gives only the number of the code: "exit code {code}"
  (`crates/riff/src/ends.rs:199`).
- `riff view` is gone (`docs/src/riff-uis.md:120`).
- A CLI test over a fake socket exists:
  `session_why_reads_the_records_through_the_socket`
  (`crates/riff/tests/lead_container.rs:105`).

New title: riff session why shows the last lines in plain words, and --format json prints the records

```markdown
Asked by Mike (riff chat, 2026-10-25).

## Facts

- `riff session why ITEM` prints the last lines of the record as they
  are (crates/riff/src/text.rs:10642-10673): the raw stream-json lines
  of `claude -p`. A person cannot read them.
- The daemon cuts each line at 300 characters
  (crates/riff/src/ends.rs:84, :174). A cut line is not JSON, so
  `View::shown` (crates/riff/src/run.rs:1788) gives it back as it is.
- The reason gives the exit code as a number only (ends.rs:199).
- Each command that prints data takes `--format text` or `--format
  json` (01M542168PYP1WH78D3D9DKEK6). `riff session why` has no
  `--format`.

## What

- The daemon turns the events into plain lines with `View::shown`
  before the cut. The record keeps the last 50 plain lines: the words
  of the model, the tool calls, the tool results, the background tasks
  and the end of the turn.
- The reason says what a code means: 143 "SIGTERM, for example the
  daemon stopped the container", 137 "SIGKILL", and each other code as
  it is.
- `riff session why ITEM --format json` prints the records of ITEM as
  JSON, with the same facts as the text.
- A requirement supersedes 01M5A5HXKZTDP5M5HY2FHCEBZA in part.

## Done when:

- A test: `End::from_events` with an `assistant` event of more than
  300 characters keeps its plain line, and no `{"type":`.
- A test: the CLI test over the fake socket
  (crates/riff/tests/lead_container.rs:105) shows the words for 143,
  and with `--format json` it prints the records as JSON.
- Doc tests of the new text functions.
- Docs: "See why a session ended with no pull request" in
  docs/src/development.md shows the plain output in a `text` block,
  and `riff session why issue-1203 --format json` in an `sh` block.
```

## #1400 A pause and a resume of the wave send the message about an issue with no Done when line again

Verdict: close

Why:

- The lead gets the message one time. PR #1547 (it closed #1439) gave
  the message a key: `no_done_when_key(n)`, "no-done-when-N"
  (`crates/riff/src/wave.rs:1094`, the post at :1926-1928).
- riff-server drops a post whose key is the key of a kept message of
  the same user in the thread: it adds no message and wakes no session
  (01M5TKG2K1AYK1KE7C2AA2W9DS,
  `crates/riff-state/src/state/threads.rs:564-565`). The wave posts as
  the person in the thread of the repository.
- The code of the issue is still there: a paused look is `Look {
  paused: true, ..Look::default() }` (`crates/riff/src/wave.rs:3565`),
  and `bare.retain` empties `bare` (:1923). But the post after the
  resume carries the same key, so riff-server drops it.
- The drop holds while the first message is in the last 200 messages
  of the thread. A new start of the daemon has the same limit, and
  01M6FZY6NABXGXVRYF6W1509XE accepts it.

## #1405 After an update, the old daemon still starts sessions that run the new riff, and their new calls fail until it restarts

Verdict: rewrite

Why:

- The fault is still there. The daemon binds the riff at its own path
  into each session (`own_binary`, `crates/riff/src/daemon.rs:738-749`,
  01M4Z60B800BH5BHX0H9JVDG6A). It starts again on a new riff only at
  `busy() == 0` (`crates/riff/src/daemon.rs:3816`, `Status::busy` at
  :1610). The wave admits new starts with no check of a replaced riff:
  `wave.rs` has no "replaced".
- A part is done. PR #1718 added two rules:
  - the daemon on a replaced riff starts no lead again (`leads_again`,
    `crates/riff/src/daemon.rs:3001-3003`, 01M6WBX2FT7Y9FV9VGGPDBKAHM);
  - in a container, a command that cannot read the answer of the
    daemon names the two builds (01M6WBX2FYPH0K8RAKXMRKDGX7).
- That error does not come for a call that the old daemon cannot read.
  The daemon answers `Say::Failed` with "The riff daemon cannot read
  the first line: it is not a hello of riff."
  (`crates/riff/src/text.rs:7360-7362`), and the relay makes a bare
  error of it (`crates/riff/src/session_relay.rs:1133-1137`).
- The line numbers of the body are old, and its two parts about
  releases of October are history now.
- The fix of the issue changes the answer of the old daemon. An old
  daemon does not get a fix. So the new body puts the error in the
  client, and keeps the drain of the waves.

New title: After an update, a busy old daemon starts no new work, and a call that it cannot read names the two builds

```markdown
## Facts

- The daemon binds the riff at its own path into each new session
  (crates/riff/src/daemon.rs:738-749). After `riff update`, the path
  holds the new riff (01M4Z60B800BH5BHX0H9JVDG6A).
- A daemon on a replaced riff starts again only at `busy() == 0`
  (daemon.rs:3816). Until then, its waves start new sessions, which
  run the new riff under the old daemon. Only a lead waits
  (daemon.rs:3001-3003, 01M6WBX2FT7Y9FV9VGGPDBKAHM).
- A new riff sends calls that an old daemon cannot read. The daemon
  answers "The riff daemon cannot read the first line: it is not a
  hello of riff." (crates/riff/src/text.rs:7360-7362), and the relay
  makes a bare error of it (crates/riff/src/session_relay.rs:1133-1137).
- The error that names the two builds (01M6WBX2FYPH0K8RAKXMRKDGX7)
  comes only for an answer that the command cannot read.

## What

- While its riff is replaced, the waves of the daemon start no new
  session and no new job. The sessions that run go on. The daemon
  starts again on the new riff after its last session.
- `riff daemon status` says that the daemon runs an old riff, starts
  no new work, and starts again on the new riff after its last session.
- In a container, a call that gets the answer "not a hello of riff"
  reads the build of the daemon, and gives the error of
  01M6WBX2FYPH0K8RAKXMRKDGX7.
- A requirement records each rule.

## Done when:

- A test: a daemon test with `Daemon::with_own_replaced`
  (daemon.rs:2626): the riff is replaced while one session runs, the
  wave starts no new session, and the daemon starts again after the
  end of the session.
- A test: a fake daemon answers a call with "not a hello of riff", and
  the error names the two builds and the next step.
- Docs: "Update a session that runs an old riff" in
  docs/src/how-it-works.md says that the daemon starts no new work on
  an old riff.
```

## #1408 No wave test checks that the verify start text names the checks of the repository

Verdict: rewrite

Why:

- The gap in the tests is still there. `Live::prepare` reads the
  checks with `repo_setup::full_check`, and gives them to
  `text::wave_verify_start` (`crates/riff/src/wave.rs:2936-2949`). No
  test runs that part. The fake world of the wave has its own `begin`
  (`crates/riff/src/wave.rs:4455`). The daemon tests in
  `crates/riff/tests/run.rs` read only the start text of a build, and
  their fake `gh` requires `Gate` (`crates/riff/tests/run.rs:101`).
- `repo_setup::full_check` has tests of its own now
  (`crates/riff/src/repo_setup.rs:2577`, :2611).
- The part about the requirement is done. 01M5R580VCRCHJM8FY7NT485R1
  says "The verify text names the checks of the full check as riff read
  them." 01M5SMNKDWBD1J6Y18ENQG87CK supersedes
  01M5QZ01Y14Z76RC220RT7YTX7 in part already. No new requirement is
  needed.
- The book does not say that the start text of a verify names the
  checks: see the part on a verify session in "What the wave does"
  (`docs/src/development.md:4046-4054`).

New title: A test checks that the start text of a verify of the wave names the required checks of the repository

```markdown
## Facts

- `Live::prepare` makes the start text of a verify. It reads the checks
  with `repo_setup::full_check`, and gives them to
  `text::wave_verify_start` (crates/riff/src/wave.rs:2936-2949).
- No test runs that part. The fake world of the wave has its own
  `begin` (wave.rs:4455). The daemon tests in crates/riff/tests/run.rs
  read only the start text of a build.
- So when `Live::prepare` gives `["Gate"]` or no checks, each test
  still passes.
- 01M5R580VCRCHJM8FY7NT485R1 says that the verify text names the
  checks as riff read them.

## What

- A test starts a verify through the wave, with a fake `gh` whose
  ruleset requires `checks / test`, `checks / docs` and `riff/verify`,
  and reads the start text.

## Done when:

- A test: the start text of the verify names `checks / test` and
  `checks / docs`, and not `Gate`. The test fails when the text gets
  `["Gate"]` in place of the checks that riff read.
- Docs: "What the wave does" in docs/src/development.md: the part on a
  verify session says that its start text names the required checks
  of the default branch, or says that the repository has no CI.
```
