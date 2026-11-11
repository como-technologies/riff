# Groom the backlog, batch 7 of 8: #1672 to #1810

Issue: #1892, part of #1885. Each issue was open when it was read. Each
fact was checked on `origin/main` at 206a3911. The lead applies the
verdicts after the merge. This report changes no issue.

| Issue | Verdict |
|---|---|
| #1672 | keep |
| #1675 | rewrite |
| #1679 | keep |
| #1682 | rewrite |
| #1685 | rewrite |
| #1720 | rewrite |
| #1723 | close |
| #1730 | keep |
| #1739 | keep |
| #1742 | rewrite |
| #1743 | keep |
| #1749 | keep |
| #1779 | keep |
| #1808 | keep |
| #1810 | keep |

## #1672 Watch the wave: the last sentence says a run with no change costs no point, without the 30 minutes

Verdict: keep

Why:

- "Watch the wave" (`docs/src/development.md:3309`) still ends its
  list with "With no pull request open and no change, a run costs no
  GraphQL point." (`docs/src/development.md:3327`).
- The list above that sentence names the second case of a full read:
  "The last full read is 30 minutes old or more."
  (`docs/src/development.md:3324`). The code agrees: `FULL_EVERY` is 30
  minutes (`crates/riff/src/changes.rs:58`), and a run keeps its read
  only while the last full read is younger than that
  (`crates/riff/src/wave_list.rs:211`).
- The body is in the form of today: its "Done when:" line names the
  heading of the book.

## #1675 The guard of the gh callers misreads a for_repo call with a parenthesis in a string literal

Verdict: rewrite

Why:

- `after_parens` (`crates/riff/tests/gh_callers.rs:108`) counts each
  `(` and `)`, and skips no string literal and no char literal. `chain`
  (`crates/riff/tests/gh_callers.rs:140`) and `nameless`
  (`crates/riff/tests/gh_callers.rs:159`) use it. So the fault is still
  there.
- No code of riff has such a call. The only `Gh::for_repo(&format!(` is
  in the test file itself (`crates/riff/tests/gh_callers.rs:228`).
  `grep -rn 'for_repo(&format' crates/*/src` finds nothing. So the
  fault can only hide a call to come.
- The body has no `Docs:` line. CLAUDE.md asks that each "Done when:"
  line names the docs.

New title: The guard of the gh callers misreads a for_repo call with a
parenthesis in a string literal

New body:

````markdown
The guard of the gh callers (`crates/riff/tests/gh_callers.rs`) finds the end of the arguments of a `Gh::for_repo(..)` with `after_parens`. It counts each `(` and `)`, and does not skip a string literal or a char literal. So a call such as `Gh::for_repo(&format!("({}", x))` is misread: the check can take the wrong end of the call, and miss or invent a `.by(..)`. No such call exists today.

From the verify of #1629 (PR #1670).

## What

- `after_parens` skips the parentheses inside a string literal (`"..."`, also with `\"` in it) and inside a char literal.

Done when:
- A unit test of the reader: `Gh::for_repo(&format!("({}", x)).by("a")` has a caller name, and `Gh::for_repo(&format!("({}", x))` has none.
- Docs: none: the guard is a test of riff, and no person uses it.
````

## #1679 riff chat: keys of one read wait in the buffer of rustyline until the next key, so a paste can look lost

Verdict: keep

Why:

- `riff chat` reads its line with rustyline. `Cargo.toml:54` pins
  rustyline 18.0.1 (`Cargo.lock:3419`). Upstream released 19.0.0 on
  2026-10-17 (the releases of kkawakam/rustyline on GitHub). The issue
  says that the fault is in both, so no update fixes it yet.
- The tests have a pseudo-terminal for `riff chat`
  (`crates/riff/tests/chat.rs:849`), but no test pastes:
  `grep -n paste crates/riff/tests/chat.rs` finds nothing.
- The Chat tab of the TUI takes a bracketed paste since #1764
  (`crates/riff/src/tui/chat.rs:70`). That is another UI, and it does
  not change `riff chat`.
- The heading of the "Done when:" line exists: "Type while lines come
  in" (`docs/src/how-it-works.md:3528`).

## #1682 Test names still name riff wave run after #1643

Verdict: rewrite

Why:

- The test
  `wave_run_with_host_makes_the_main_clone_on_a_host_that_does_not_serve_the_repository`
  is still at `crates/riff/tests/daemon_hosts.rs:1279`. Its body runs
  `riff join --host b` (`crates/riff/tests/daemon_hosts.rs:1297`), and
  its rustdoc names `riff join --host b`. Only the name is old.
- `grep -rnE "fn (wave_run|wave_stop|daemon_host)_" crates/riff/tests`
  finds only that test.
  `the_help_lists_join_and_leave_and_not_wave_run_wave_stop_or_daemon_host`
  (`crates/riff/tests/help.rs:125`) is right as it is: it checks that
  the help has no `riff wave run`, and the grep does not find it.
- The body has no `Docs:` line.

New title: The test of riff join --host still has the name of riff wave
run

New body:

````markdown
`riff wave run` is gone since #1643: `riff join` takes its place. The test `daemon_hosts::wave_run_with_host_makes_the_main_clone_on_a_host_that_does_not_serve_the_repository` runs `riff join --host b`, but its name still names the old command.

From the verify of #1643 (PR #1680).

## What

- Rename the test to name `riff join --host`, for example `join_with_host_makes_the_main_clone_on_a_host_that_does_not_serve_the_repository`.

Done when:
- `grep -rnE "fn (wave_run|wave_stop|daemon_host)_" crates/riff/tests` finds no test, and the tests of `daemon_hosts` pass.
- Docs: none: a test name changes no feature of riff.
````

## #1685 Two superseded requirements moved to another section of requirements-history.md, and just reqs does not check the section

Verdict: rewrite

Why:

- Both requirements sit under "The waves of the daemon"
  (`docs/src/requirements-history.md:1987`):
  01M5515CFHRA6AJG4GKX0ZV77W at `docs/src/requirements-history.md:1989`
  and 01M5515CJ7QDJ639VGR7SJTE08 at
  `docs/src/requirements-history.md:1993`.
- Before each move, both sat under "The re-cut: isolation by default"
  of `requirements.md`: `git show e9f8ced4^:docs/src/requirements.md`
  (PR #1680) and `git show 658b10b4^:docs/src/requirements.md`
  (PR #1684). The history page has that heading
  (`docs/src/requirements-history.md:1836`).
- `just reqs` runs `reqs check` (`justfile:96`). Its message says "in
  the same section" (`crates/reqs/src/lib.rs:235`), but no code
  compares the sections.
- A check of a change exists for a like rule: `reqs::uncut`
  (`crates/reqs/src/lib.rs:364`) compares `requirements.md` before and
  after a change, and Hygiene runs it as the rule `in-part`
  (`crates/hygiene/src/lib.rs:73`). A check of the section fits there.
  The issue asks for a check against "the history of git", which is
  harder and not needed.
- The docs line names no heading. The heading is "Move a superseded
  requirement" (`docs/src/development.md:4740`).

New title: Two superseded requirements moved to another section of
requirements-history.md, and Hygiene does not check the section

New body:

````markdown
CLAUDE.md: "A requirement that a new one supersedes in full moves, in the same change, to `requirements-history.md`: same section, same ID, same text." No check reads the section, and two requirements moved to another section:

- 01M5515CFHRA6AJG4GKX0ZV77W (moved by PR #1680) and 01M5515CJ7QDJ639VGR7SJTE08 (moved by PR #1684) came from "The re-cut: isolation by default", and now sit in "The waves of the daemon" of `docs/src/requirements-history.md`.

From the verify of #1644 (PR #1684).

## What

1. Move the two requirements to "The re-cut: isolation by default" of `requirements-history.md`.
2. Hygiene (`hygiene pr`, as the rule `in-part` does with `reqs::uncut`) compares `requirements.md` before and after the change. It fails when a requirement leaves a section of `requirements.md` and comes into another section of `requirements-history.md`. The error names the ID and the two sections.

Done when:
- The two requirements are in "The re-cut: isolation by default" of `requirements-history.md`, and `just reqs` passes.
- A test of the check fails on a requirement moved to another section, and passes on a move to the same section.
- A requirement (`just req`) says that Hygiene checks the section of each moved requirement.
- Docs: "Move a superseded requirement" in `docs/src/development.md` says that Hygiene fails a move to another section, with a copyable `sh` block that runs the check on a pull request (`just hygiene pr N`).
````

## #1720 Stale texts and code: two requirements without their in-part mark, the rustdoc of cycle.rs and gh_wrapper.rs, and the dead compact_lock

Verdict: rewrite

Why: each part still holds on `origin/main`, but the "What" and the
"Done when:" lines do not match. Three parts have no "Done when:" line
(the doubled word, R51 and `ReleaseNews`). Four "Done when:" lines
have no part in "What" (`Record::github`, the requirement of
`--no-github` and the two docs lines): three comments added them.

- 01M4Z60B7JF80TMP6FHQ04MHCZ still says "`gh` at `/usr/bin/gh`"
  (`docs/src/requirements.md:4869`). 01M6QCDHZ5KNMFXQB9X3ZYRF3N lets
  the `gh` of an image be at another path
  (`docs/src/requirements.md:6148`), and it names only
  01M6JGF7265VX2SDXZ4EJAGXVB in part.
- 01M5RGG9KJXCKH8C94145WS8SM (`docs/src/requirements.md:6628`) has no
  "Supersedes 01M5R1675HZ6TJPMQ6536X37T5 in part" mark.
- `crates/riff/src/cycle.rs:19` still names "Gate" for `riff release
  verify`.
- `crates/riff/src/gh_wrapper.rs:13` says that the wrapper runs "the
  real `gh` at [`REAL`]", and the diagram at
  `crates/riff/src/gh_wrapper.rs:36` shows `/usr/bin/gh`. The wrapper
  finds the real `gh` with `real_in`
  (`crates/riff/src/gh_wrapper.rs:195`). A new stale line:
  `crates/riff/src/gh_wrapper.rs:6` says that the call uses "the
  GraphQL points of the token of the person". Since #1838 no session
  gets that token (01M6Z87A04EXTT1FFJSJ1EM6TB,
  `docs/src/requirements.md:1066`).
- `riff::local::compact_lock` (`crates/riff/src/local.rs:643`) has no
  caller: `git grep compact_lock -- crates docs` finds only its own
  lines. This report names it too, so the old "Done when:" line
  (`git grep compact_lock` finds nothing) could not pass after the
  merge of this report. The rustdoc of `compact_lock`
  cites 01M3Q88G1K7N2EMPBA07X069A7, which is in
  `docs/src/requirements-history.md:858`. 01M59BTTH6ZATH7R2PFC6NGHJK
  says that riff has no compact of the lead
  (`docs/src/requirements.md:6403`).
- The doubled word "The `gh` `gh`" is at
  `crates/riff/src/daemon/gh_token.rs:667` and
  `crates/riff/src/daemon/gh_token.rs:774`.
- R51 (`docs/src/requirements.md:3100`) lists the selector fields
  `user` to `lead`. `Selector` also has `person` and `name`
  (`crates/riff-core/src/selector.rs:148`,
  `crates/riff-core/src/selector.rs:153`).
- The rustdoc of `Record::github` still says "or the token of the
  person and why" (`crates/riff/src/daemon.rs:1903`). `ActsAs` has no
  such case: `Bot`, `NoToken` and `NoGitHub`
  (`crates/riff/src/daemon/gh_token.rs:167`).
- `ReleaseNews` still says "Each session may: a worker cuts a release"
  (`crates/riff/src/daemon.rs:1290`). Since #1841 (PR #1845) the cut
  reads the alerts, which only the lead App can read.
- 01M6Z87JY83V3KR72E2AG8V8KG (`docs/src/requirements.md:7039`) does not
  say that a `riff run --no-github` session keeps the git identity of
  `~/.gitconfig`. The book says it (`docs/src/development.md:2026`).
- "How riff is built and installed" still opens with "riff ships as one
  static binary." (`docs/src/development.md:607`). A Mac gets other
  files since #1648 (PR #1836).
- "Change the session image" (`docs/src/development.md:790`) does not
  name `/usr/bin/gh`, so that part of the old docs line holds already.
  "What sessions share" says "The image has `gh` at `/usr/bin/gh`."
  (`docs/src/how-it-works.md:200`).

New title: Stale texts and code: four requirements, the rustdoc of the
gh wrapper, the daemon and the release cycle, two lines of the book,
and the dead compact_lock

New body:

````markdown
Texts that still say a rule that a newer one changed, and one dead function. The verifies of #1705 (PR #1716), #1783 (PR #1790), #1787 (PR #1796), #1815 (PR #1838), #1841 (PR #1845) and #1648 (PR #1836) found them. The `in-part` check cannot see the two requirements, because the newer text has no "Supersedes ... in part" mark.

## What

Requirements:
- 01M4Z60B7JF80TMP6FHQ04MHCZ says that an image has `gh` at `/usr/bin/gh`. 01M6QCDHZ5KNMFXQB9X3ZYRF3N lets `gh` be at another path. Mark 01M6QCDHZ5KNMFXQB9X3ZYRF3N as "Supersedes 01M4Z60B7JF80TMP6FHQ04MHCZ in part", and cut the old text to the part that holds.
- 01M5RGG9KJXCKH8C94145WS8SM is why two sentences of 01M5R1675HZ6TJPMQ6536X37T5 were cut (#1716). Add the mark "Supersedes 01M5R1675HZ6TJPMQ6536X37T5 in part".
- R51 lists the selector fields `user` to `lead`, with no `person` and no `name`. A new requirement (`just req`) names each field, and supersedes R51 in part.
- No requirement says that a `riff run --no-github` session keeps the git identity of `~/.gitconfig`. Add one (`just req`).

Rustdoc:
- `crates/riff/src/cycle.rs`: the table names "Gate" for `riff release verify`. Name the checks that it reads today: the label, the diff, the body and the full check of the head commit.
- `crates/riff/src/gh_wrapper.rs`: the top and its first diagram show `/usr/bin/gh` as the real `gh`. Say how riff finds it (`real_in`). The top also says that a call uses the GraphQL points of the token of the person: since #1838 it uses the token of the App of the session.
- `crates/riff/src/daemon/gh_token.rs`: "The `gh` `gh`" in the rustdoc of `Writes::gh` and of `writer`.
- `crates/riff/src/daemon.rs`: `Record::github` says "or the token of the person and why"; `ReleaseNews` says "Each session may: a worker cuts a release". Since #1841 a worker cannot cut a release.

Code:
- `riff::local::compact_lock` (`crates/riff/src/local.rs`) has no caller: riff has no compact of the lead (01M59BTTH6ZATH7R2PFC6NGHJK). Remove it and its doc test.

Book:
- "What sessions share" in `docs/src/how-it-works.md` says "The image has `gh` at `/usr/bin/gh`."
- "How riff is built and installed" in `docs/src/development.md` opens with "riff ships as one static binary". Since #1648 that holds for Linux only.

Done when:
- 01M6QCDHZ5KNMFXQB9X3ZYRF3N and 01M5RGG9KJXCKH8C94145WS8SM carry their marks, the old texts keep only the part that holds, and `just reqs` and Hygiene pass.
- A requirement names each selector field, `person` and `name` too, and R51 keeps only the part that holds.
- A requirement says that a `riff run --no-github` session keeps the git identity of `~/.gitconfig`.
- The rustdoc of `cycle.rs`, `gh_wrapper.rs`, `daemon/gh_token.rs` and `daemon.rs` agrees with the code, and the doc tests pass.
- `git grep compact_lock -- crates docs` finds nothing.
- Docs: "What sessions share" in `docs/src/how-it-works.md` says where a session finds the real `gh`, and does not say that each image has it at `/usr/bin/gh`.
- Docs: "How riff is built and installed" in `docs/src/development.md` says that riff ships as one static binary on Linux, and names the files of a Mac.
````

## #1723 Identity B: the token of each session: riff-server makes it, the daemon gives it to the container and renews it

Verdict: close

Why:

- The build merged in PR #1746 on 2026-11-08 (the comment "Merged in
  #1746 (4737cd1)" on the issue). The only part left was the live
  check, and it needed the Apps of #1722 first.
- #1722 closed on 2026-11-09 after its live check. In its comment, the
  `riff github check` of the owner shows the worker as
  `riff-como-technologies[bot]` with `statuses: read`, and the lead as
  `riff-como-technologies-lead[bot]` with `statuses: write`. With
  `statuses: read`, GitHub refuses a status from the worker token
  with 403 "Resource not accessible by integration".
- The workers open their pull requests as the bot: 30 of the last 200
  merged pull requests have the author `app/riff-como-technologies`,
  from #1768 on (for example #1838, #1842 and #1845; `gh pr list
  --state merged --limit 200 --json number,author`). The status
  `riff/verify` of the head of PR #1845 comes from
  `riff-como-technologies-lead[bot]`.
- The two requirements of the item moved:
  01M4Z60B89BHXQVJ2MGRRQ2CKJ (`docs/src/requirements-history.md:2944`)
  and 01M3JN4QVR3JCRWC8TFJJTCPHF
  (`docs/src/requirements-history.md:240`).
- The fallback to the token of the person (Q5) is gone since #1838:
  01M6Z87A04EXTT1FFJSJ1EM6TB (`docs/src/requirements.md:1066`). So no
  more work of this item is left to do.
- The docs exist: "Who acts on GitHub for each role"
  (`docs/src/how-it-works.md:6245`), with `riff whoami` in a `sh` block
  (`docs/src/how-it-works.md:6340`).
- The record does not say which host ran the worker. The "Done when:"
  line asks for pangolin, but the check of the token does not depend
  on the host.

## #1730 The deploy watch asks who before the deploy passed, and goes on without the other hosts when who fails

Verdict: keep

Why:

- `look_all` asks `lookout.hosts` for each repository with a watch of
  a deploy, at each look (`crates/riff/src/watches.rs:1021`). That
  comes before `read_all`, which reads whether the job `Deploy` passed
  (`crates/riff/src/watches.rs:1027`).
- On an error, the watch only logs "cannot read the hosts"
  (`crates/riff/src/watches.rs:1024`) and goes on. The daemon gives
  the watch no other hosts (`hosts: BTreeMap::new()` at
  `crates/riff/src/daemon.rs:3253`). So the check of the old hosts
  (`crates/riff/src/watches.rs:407`) finds none, and the watch can say
  "check now".
- Since #1728, only #1774 changed `look_all`, and only its call of
  `settle_one` (`git log -L` of `look_all` in
  `crates/riff/src/watches.rs`).
- The heading of the docs line exists: "Wait for the new riff before
  the check" (`docs/src/development.md:6895`).

## #1739 Design: take over or attach a session of another host through riff-server, with a terminal stream

Verdict: keep

Why:

- `riff takeover` has no `--host` (`crates/riff/src/main.rs:698`: only
  `--at-end` and NAME).
- The Sessions tab gives a note for a session of another host: "`ssh
  HOST`, then `riff takeover NAME`, or `riff lead start` for a lead"
  (`crates/riff/src/tui/sessions.rs:98`,
  `crates/riff/src/tui/sessions.rs:124`).
- The proof of presence that the design needs is still open (#1657).
- No design comment is on the issue yet. Its "Done when:" lines are in
  the form of a design item of today.

## #1742 When riff-server forgets a session with claims, it wakes the wave of each host, as a release does

Verdict: rewrite

Why: the code does this already. Only a test and the book are missing.

- The forget gives one `released` record for each claim of the session
  (`crates/riff-state/src/state/the_riff.rs:586`, `released_all` at
  `crates/riff-state/src/state/work.rs:245`). That has been so since
  2026-10-02 (`git blame`), and a requirement says it:
  01M3X9XCSBR11ACD86FNXKF8JH (`docs/src/requirements.md:1725`).
- `Record::plan_thread` gives the thread of a `released` record
  (`crates/riff-core/src/record.rs:639`). The writer of riff-server
  runs `effects` for each written entry, the forget too
  (`crates/riff-server/src/engine.rs:1240`). `effects` takes the
  thread of each record (`crates/riff-server/src/engine.rs:1307`), and
  sends one `PlanChanged` for each thread
  (`crates/riff-server/src/engine.rs:1313`).
- 01M6WJNV9KP5TN5SVBVT4JDN66 (`docs/src/requirements.md:360`) says that
  a command that releases a claim sends an event of the plan. The
  forget is such a command. So the issue needs no new requirement.
- No test covers it: `a_hold_a_free_and_a_release_each_give_one_event_of_the_plan`
  (`crates/riff-server/tests/plan.rs:145`) tests a release, and no test
  forgets a session with a claim.
- "Which host takes a free item" names a hold, a free and a release,
  but not the forget (`docs/src/how-it-works.md:135`).

New title: A test and the book show that the forget of a session with
claims wakes the wave of each host

New body:

````markdown
riff-server already wakes the waves when it forgets a session that holds claims: the forget gives one `released` record for each claim (01M3X9XCSBR11ACD86FNXKF8JH), and each `released` record sends one event of the plan (01M6WJNV9KP5TN5SVBVT4JDN66, #1729). No test shows it, and the book does not say it.

From the verify of #1729 (PR #1737).

## What

- A test of riff-server for the forget and the plan events.
- The book names the forget in the list of the changes that wake the waves.

Done when:
- A test of riff-server: a session with a claim that riff-server forgets gives one event of the plan for the thread of its claim; a forgotten session with no claim gives none.
- Docs: "Which host takes a free item" in `docs/src/how-it-works.md` says that the forget of a session with claims wakes the wave of each host, as a release does.
````

## #1743 The Machines tab and riff who show left riff also on a host that still serves another repository

Verdict: keep

Why:

- `facts::look` names the repository as left only in the facts of the
  look of that repository: `left(&mut facts, &wave.repo)` at
  `crates/riff/src/daemon/facts.rs:227`, when the daemon serves it no
  more.
- `riff_client::daemons::hosts` keeps only the youngest facts of each
  host (`crates/riff-client/src/daemons.rs:643`). The facts of the
  session of another repository have no `left` entry, so they hide it.
- Neither function changed since #1740: `git log -L` gives #1740 last
  for `facts::left`, and #1092 last for `daemons::hosts`.
- The headings exist: "The UIs" (`docs/src/requirements.md:7796`) and
  "See the hosts in the TUI" (`docs/src/how-it-works.md:1269`).

## #1749 A Needs merged: line waits only for the merge of its need, so a chain with checks after the release does not wait for each release

Verdict: keep

Why:

- The free work still blocks an item on "a need that is merged but not
  closed" (`crates/riff/src/rollout.rs:607`).
- No code and no page knows `Needs merged:`: `grep -rni "needs merged"
  crates docs/src` finds nothing.
- R215 (`docs/src/requirements.md:201`) defines "merged" (closed, or
  with the comment `Merged in #PR (COMMIT)`), but the needs wait for
  "closed".
- The lead still works around it by hand: the body of #1723 has a
  `Needs:` line with no `#` number for that reason.
- The headings exist: the word "need" in the table of
  `docs/src/design-plan.md:32`, and "Waves on GitHub"
  (`docs/src/requirements.md:391`). "Name the needs of an item" is a
  new how-to.

## #1779 Design: the commands of riff chat, from what people use

Verdict: keep

Why:

- The commands are the same as in the body: `/me`, `/quit` and `//`
  (`crates/riff/src/chat.rs:244` to `crates/riff/src/chat.rs:257`). A
  line with another `/WORD` is `Typed::Unknown`, and the chat does not
  send it.
- The Chat tab lists `/me TEXT` in its keys
  (`crates/riff/src/tui/chat.rs:1445`).
- `/msg` is still open and in the Backlog (#1653).
- No design comment is on the issue yet. Its "Done when:" lines are in
  the form of a design item of today.

## #1808 The page of the phone app and its requirements use the chat names of today

Verdict: keep

Why:

- `docs/src/riff-mobile.md:55` still says that "The parse of `@lead`
  and `@USER`" comes from riff-core.
- "The notices" still says that "the client makes the selector
  `user=USER, lead=true` from `@USER`" (`docs/src/riff-mobile.md:206`),
  and its diagram shows `to user=mike, lead=true`
  (`docs/src/riff-mobile.md:214`). Since #1784 each UI sends one
  selector `name=NAME` for each name, and riff-server resolves it
  (01M6Y1A5PTCVZ6GRJ0FXMART2T, `docs/src/requirements.md:3322`).
- 01M53NTQTHC8WHW9ZZ6S1VZ2W8 (`docs/src/requirements.md:4438`) and
  01M53NTRKQ894YHQD9YYRNJBHZ (`docs/src/requirements.md:4505`) still
  name `@lead` and `@USER`.
- 01M6VMACJGF4A0ZXC6YK13KNCH (`docs/src/requirements.md:8116`) says
  that the note of the prompt says why the chat will not send a line.
  The note also shows "Enter ends the TUI. The sessions go on." for
  `/quit` (`crates/riff/src/tui/chat.rs:1363`).
- The phone app is still a design (`docs/src/riff-mobile.md:3`). Its
  build items are open (#963, #964), so the page must be right before
  they start.

## #1810 A gcloud or git that ends before it reads its input gives its own error, not a broken pipe

Verdict: keep

Why:

- `Gcloud::run` gives the error of `write_all` with `?`
  (`crates/riff/src/cloud.rs:181`). So a broken pipe hides the exit
  status and the stderr of `gcloud`.
- The `git` helper of `build_id` does the same
  (`crates/riff/src/cloud.rs:1678`). It also sends the stderr of `git`
  to `Stdio::null()`, and reads no exit status. So it has no stderr to
  give yet. The worker must pipe the stderr and check the exit status
  too.
- `Gh::spawn` takes a broken pipe as no error
  (`crates/riff/src/pr.rs:1396`), per 01M6Y4XHQADF27SHMMMV8RQ7Q1
  (`docs/src/requirements.md:913`, in "Pull requests on GitHub").
