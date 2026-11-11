# Groom the backlog, batch 6 of 8: #1421 to #1659

Issue #1891, a part of #1885. I read each issue and its comments on
2026-11-11, and checked them against `origin/main` at f789f08e. Each
`path:line` is a line of that commit. This report changes no issue:
the lead applies the verdicts after the merge.

| Issue | Verdict |
|---|---|
| #1421 | question |
| #1489 | rewrite |
| #1520 | question |
| #1521 | rewrite |
| #1522 | keep |
| #1523 | rewrite |
| #1525 | question |
| #1532 | question |
| #1542 | close |
| #1583 | rewrite |
| #1602 | close |
| #1642 | rewrite |
| #1653 | keep |
| #1654 | keep |
| #1659 | keep |

## #1421 Design: the contract between riff and a repository: what each step of the workflow needs, in riff.toml

Verdict: question

Why:

- Each child issue is closed: #1418 (Wave 41) and #1422 to #1429
  (Wave 42), all on 2026-10-25.
- The parts of the contract are built:
  - `riff-base` and `riff-rust` (#1422):
    `crates/riff/src/session_image.rs:66-83`.
  - The release cycle only with `[release]` (#1425):
    01M5SMPX8NM75P8E38DGQ17W7B, `docs/src/requirements.md:7319`.
  - In another repository, `riff pr open` keeps only the link line and
    `Issue:` (#1428): `crates/riff/src/pr.rs:46-51`.
  - The CI of the repository is the gate, and `riff.toml` has no table
    for the check or the rules (#1418): 01M5RN89Z749VYS1CRYJQTKAWY,
    `requirements.md:6647`.
  - A repository with no CI (#1426): "A repository with no CI",
    `docs/src/start-a-riff.md:813`.
- The book has no page and no heading "What a repository gives riff".
  A grep of `docs/src` finds no such text. The parts of the contract
  are in "Add another repository to the riff" (`start-a-riff.md:612`),
  "Add riff.toml" (`start-a-riff.md:867`) and "Where the env of a
  session comes from" (`docs/src/how-it-works.md:204`).
- The names that riff uses are in "The names riff uses on the forge"
  (`docs/src/waves.md:362`), with a link from `start-a-riff.md:670`.
  01M5SMMP561HR1ZQY3ZE4361FD (`requirements.md:494`) holds them. So
  the decision of 2026-10-25 that the page "What a repository gives
  riff" lists the names does not hold now.
- No one requirement records the whole contract.
- The forge trait (open choice 4, default d) is not built.
  `cycle::Forge` (`crates/riff/src/cycle.rs:525`) is a trait for the
  release cycle only, with one impl `GitHub` (`cycle.rs:2739`) and a
  fake for tests (`cycle.rs:3436`). `pr::Gh`
  (`crates/riff/src/pr.rs:294`) is a struct. `wave::World`
  (`crates/riff/src/wave.rs:1459`) is a trait for the wave only.
- No issue designs the workflow in code or a forge trait. Two issues
  are close to it: #553 (strong types from end to end, Wave 59) and
  #828 (states as enums, Backlog).
- `[verify] check` is only an open idea in the body. No issue tracks
  it. sens-o-matic put its API tests in its required check (its PR
  #21, in the body).
- Mike, in a comment of 2026-10-25: "Don't start this issue until
  @n8behavior weighs in more. I think this starts to define the
  central workflow of riff and should likely be designed in code:
  `enum`s, state machine, type states ... behind traits so we can swap
  out a forge."

Questions for mike:

- Q1: Do you still want one table of the contract in the book?
  Proposal: yes, as a new section "What a repository gives riff" in
  "Add another repository to the riff" (`start-a-riff.md`), not as a
  new page. The table links to each how-to of today, and one
  requirement holds the contract (`just req`). It is a small new issue.
- Q2: Your comment asks for the workflow in code (enums, a state
  machine, type states) and the forge behind a trait. Is that a new
  design? Proposal: yes. File a Backlog issue "Design: the workflow of
  riff in code: each step a type, and the forge behind a trait", after
  #553. Then close #1421, because each child is done.
- Q3: Does `[verify] check` stay an open idea? Proposal: no, drop it.
  A repository puts its behavior tests in the check that its ruleset
  requires, as sens-o-matic did.

## #1489 Design: approvals have a home: riff asks for each approval in code when the step waits, and a board shows each open approval

Verdict: rewrite

Why:

- Step 1 of the design is done for the deploy. riff posts "waits for
  your approval" only when the job `Deploy` waits for `production`,
  and reminds the owner each 4 hours (#1437):
  01M5RGD0XTR4X1MEV726HMCEGQ, `docs/src/requirements.md:7489`;
  `waits_line`, `crates/riff/src/cycle.rs:1338`.
- `riff release cut` asks the owner for nothing (#1494, PR #1504):
  01M5TKDYAFA683KNAJTGYTQD5W, `requirements.md:7338`.
- Only a person approves a deploy, and `riff release approve` refuses
  each session, also the lead (PR #1753): 01M6WZ40SFVZ6CRQ1H4Z27CWP4,
  `requirements.md:7350`; the refusal text at
  `crates/riff/src/text.rs:2448-2450`.
- The rules of the lead have no rule about an approval: `LEAD_RULES`
  (`text.rs:11742`) and `lead_start` (`text.rs:11876`) do not say
  "approv".
- Each kind of approval has its own place today:
  - the deploy: the state of the release in `riff who` and in the TUI
    (PR #1774, 01M6XTS1GCW7PTWSMM1HKQTHSN, `requirements.md:8350`),
    in "See who acts next on the release"
    (`docs/src/development.md:6713`);
  - a question for the person: `riff questions` and the Waves tab
    (#1605), in "See all your open questions" (`development.md:3583`);
  - a pull request that waits for a review, or a fork run that waits
    for an approval: a message to the lead (PR #1536).
- No board lists each kind. The TUI has six tabs, and no tab is for
  approvals (`docs/src/riff-uis.md:156-164`).
- The kinds in the body changed. #278 (the refusals of auto mode) was
  closed as not planned on 2026-11-11. #1356 (accept an outside pull
  request) is open in the Backlog. #284 is now about the bound of a
  read, not a board.
- The closed design #571 depends on this issue: the approvals "live on
  the board of #1489", and "each approval on the board says whether it
  needs a proof". #1658 (Wave 55) builds that proof.

New title: Design: one place where a person sees each approval and
decision that waits for them

New body:

````markdown
## What

Mike (riff chat, 2026-10-25): "approvals should have a home that human can always go to and see easily without (re) reading back through a long chat log."

riff already asks in code, at the moment the step waits, for a deploy (#1437, #1504, #1753). Today each kind has its own place:

- the deploy of a release: the state of the release in `riff who` and in the TUI (#1774);
- a question for the person: `riff questions` and the Waves tab (#1605);
- a pull request that waits for a review, or a fork run that waits for an approval: a message to the lead (#1536);
- the accept of an outside pull request: #1356, not built.

Design one place that lists each approval and decision that waits for a person: its kind, its link, its age, and the command that gives it. It says when an approval needs the proof of #1658 (#571). The chat gets one line that links to it. A lead asks for no approval by itself: riff asks.

Proposals for Mike:

1. The place: a part of `riff questions` and of the Waves tab, not a new command or a new tab.
2. A new rule in the rules of the lead: the lead asks the person for no approval; riff asks when the step waits.

Done when:
- The design is in this issue, and Mike agreed to it.
- Each child issue is filed, with its `Done when:` lines and its `Docs:` line.
- Docs: a new heading "See what waits for you" in `docs/src/how-it-works.md`, from the first child.
````

## #1520 On a login with a security key, the riff daemon starts before the desktop, the keyring unlock prompt fails, and gh hangs for the whole login

Verdict: question

Why:

- The body still holds, and no pull request fixed it. The body
  chooses no fix, so a worker cannot start.
- `riff daemon install` writes a unit with `WantedBy=default.target`,
  and with no `After=` (`crates/riff/src/service.rs:208-227`). No file
  in `crates/` names `graphical-session`. The daemon waits neither for
  the desktop nor for the keyring.
- A keyring call of riff stops after 10 s (`KEYRING_WAIT`,
  `crates/riff-client/src/secrets.rs:135`). But on a locked item, the
  store of riff asks the Secret Service to unlock it
  (`crates/riff/src/keyring_store.rs:141-144`), and that asks for the
  prompt.
- No session gets the token of the person now (PR #1838,
  01M6Z87A04EXTT1FFJSJ1EM6TB; `crates/riff/src/daemon/gh_token.rs:58`).
  But the daemon still runs the `gh` of the person on the host.
- Some `gh` calls of the daemon have a time limit, some do not:
  - The look of the wave has 30 s (`GH_LIMIT`,
    `crates/riff/src/daemon/sweep.rs:133`) and one token for its calls
    (`crates/riff/src/wave.rs:3900`), since PR #1565 of 2026-11-03.
  - The `gh` of the loop and the jobs of the wave has no limit
    (`forge`, `wave.rs:1713-1715`; no limit is the default,
    `crates/riff/src/pr.rs:353`).
  - The `gh auth token` of `host_facts` has no limit
    (`crates/riff/src/run.rs:1465`, through `output` at `run.rs:1235`).
- "When the keyring is locked" (`docs/src/development.md:5488-5506`)
  names only a crash of `gnome-keyring-daemon`. No page of the book
  names a security key, `pam_u2f` or a YubiKey.

Questions for mike:

- Q1: When does the daemon start? Proposal: on a host with a desktop,
  `riff daemon install` writes `After=graphical-session.target` and
  `WantedBy=graphical-session.target`. On a host with no desktop, it
  keeps `default.target`.
- Q2: Can the daemon ask the keyring for the unlock prompt? Proposal:
  no. Only a command that a person types asks for the prompt. The
  daemon waits for an open keyring, and says "unlock it at the
  desktop".
- Q3: Does each `gh` call of the daemon on the host get a time limit?
  Proposal: yes. `forge` and `host_facts` get `GH_LIMIT`, as the look
  has. A `gh` that hangs then stops no wave, and the wave says why.
- Q4: Does #1522 come first? Proposal: yes. The recovery of today is a
  restart of the keyring, and the daemon sees that restart only after
  #1522.

## #1521 Setup finds the riff of a host and keeps it: each riff command then works with no --server and no RIFF_SERVER

Verdict: rewrite

Why:

- Nothing of the change is built. No merged pull request did a part
  of it.
- `server_of` still reads only `--server`, then `RIFF_SERVER`, then
  the default (`crates/riff/src/main.rs:3913-3924`). The body names
  `main.rs:3605`.
- 01M3K0Q7X3FEKWZK0B3854C4RV ("It keeps no chosen server in a file")
  is still in `docs/src/requirements.md:2005`, and R133 at
  `requirements.md:2002`.
- The body does not name 01M3Q5VE74608N5H2M73RB6Y2Z
  (`requirements.md:2013`): `riff server` names the source "`--server`,
  `RIFF_SERVER` or the default". The change supersedes it in part.
- The settings file has no `server` key
  (`crates/riff/src/settings.rs:29-42`). `riff login` takes no
  argument of its own (`main.rs:61`).
- `riff invite` still prints the line `echo 'export RIFF_SERVER=...' >>
  ~/.bashrc` (`text::invited`, `crates/riff/src/text.rs:2203`). The body
  names `text.rs:1758`.
- `Reach::of` is at `crates/riff/src/reach.rs:162`, not at `:153`.
- The book still tells a person to edit `~/.bashrc`: "Join the riff"
  and "Change to another riff" (`docs/src/join-a-riff.md:36`, `:98`),
  and "Sign in first" (`docs/src/start-a-team-riff.md:200`). "The riff
  that riff uses" is at `docs/src/how-it-works.md:246`.
- The line "#313 wants a riff for each repository" is old. #313 was
  rewritten on 2026-11-11: "A riff for each repository: obsolete. One
  riff holds the projects of a team (#1668, #1862)."
- #1358 (one way to configure riff) is still open in the Backlog. This
  issue is the slice of #1358 for the server.

New title: riff login finds the riff of a host and keeps it: each riff
command then works with no --server and no RIFF_SERVER

New body:

````markdown
From Mike, 2026-11-02: "installing riff on a host should assist the user in finding the server and once setup is done, all commands should just work on that machine without the user needing to pass or set the URL. It should still allow the user to override via --server or RIFF_SERVER when needed."

## What

Today riff finds its server only in `--server` or `RIFF_SERVER`, else `http://127.0.0.1:7878` (`server_of` in `crates/riff/src/main.rs`, 01M3K0Q7X3FEKWZK0B3854C4RV). The book and `riff invite` tell a person to add `export RIFF_SERVER=...` to `~/.bashrc`. A shell with no such line asks this machine, and says nothing about it. The daemon of the host knows the shared riff: each wave keeps its server in `waves.json`.

1. On a host with no server, `riff login` offers the server that it finds, and asks one question. First it looks at the servers of the waves of the daemon of this host, then at a riff-server of this machine that answers. With neither, it asks for the address. With no terminal, it asks nothing, names the command to run, and exits with 1. `riff init` runs the same step on a host with no server.
2. `riff login --server ADDRESS` keeps `server = "ADDRESS"` in `~/.config/riff/config.toml` with no question. It keeps no server that comes from `RIFF_SERVER`.
3. The order is `--server`, then `RIFF_SERVER`, then `server` of the settings file, then `http://127.0.0.1:7878`.
4. `riff server` names the source, for example `(from ~/.config/riff/config.toml)`.
5. A change to another riff is one command: `riff login --server ADDRESS`.
6. `riff invite` prints `riff login --server ADDRESS` in place of the `~/.bashrc` line.

This is the slice of #1358 for the server.

Requirements: a new statement for each rule, with `just req`. 01M3K0Q7X3FEKWZK0B3854C4RV is superseded in full, and moves to `requirements-history.md`. 01M3Q5VE74608N5H2M73RB6Y2Z is superseded in part. R133 stays.

Done when:
- A test (`isolated`): after `riff login --server ADDRESS`, `riff who` in a new process, in another folder, with no `--server` and no `RIFF_SERVER`, asks ADDRESS.
- A test: `--server` wins over `RIFF_SERVER`, and `RIFF_SERVER` wins over the settings file. `riff server` names the source of each.
- A test: on a host with no server, `riff login` offers the server of the waves of the daemon. With no terminal, it asks nothing and exits with 1.
- A test: `riff login` with `RIFF_SERVER` set keeps no server.
- Docs: "Join the riff" and "Change to another riff" in `docs/src/join-a-riff.md`, and "Sign in first" in `docs/src/start-a-team-riff.md`, use `riff login --server ADDRESS`, with no `~/.bashrc` line. "The riff that riff uses" in `docs/src/how-it-works.md` shows the settings file in the order of its mermaid diagram, and has a new how-to "Use another riff for one shell" with `RIFF_SERVER`. The text of `riff invite` and the `--help` of `riff login` and `--server` match the book.
````

## #1522 After a restart of gnome-keyring-daemon, the riff daemon says the keyring is locked until riff.service starts again

Verdict: keep

Why:

- Each fact of the body holds. No merged pull request and no test
  covers it.
- `Store::new` connects to the Secret Service one time
  (`crates/riff/src/keyring_store.rs:77-82`), and the store keeps that
  one connection (`keyring_store.rs:70-72`). No code connects again
  after an error. The rustdoc still says "OpenSession("plain"), one
  time for the process" (`keyring_store.rs:28`).
- The test binary `keyring_store` (`crates/riff/Cargo.toml:92-93`) has
  three tests (`crates/riff/tests/keyring_store.rs:58`, `:66`, `:74`).
  None starts the private keyring again under one `Store`.
- "When the keyring is locked" (`docs/src/development.md:5488`) says
  nothing about a restart of the keyring or of `riff.service`.

Notes for the worker, as a comment on the issue; the body can stay:

- A second place keeps a failure for the life of the process:
  `os_store` opens the store one time in a `LazyLock`, and keeps an
  error too (`crates/riff-client/src/secrets.rs:453-461`).
- 01M54510PS9FDKQKJB0RAH3XG3 says "A `riff` process opens the OS
  keyring at most one time" (`docs/src/requirements.md:3842`). The
  fix supersedes it in part.
- "A locked keyring stops the sessions of the daemon"
  (`development.md:1577-1590`) says `keyring locked` shows "until the
  keyring answers". The fix makes that line true after a restart of
  the keyring.

## #1523 Ctrl-Z in an attached lead stops claude, and nothing in the container can continue it

Verdict: rewrite

Why:

- The fault is still there. `session_relay::terminal` still runs
  `claude` with `exec` (`crates/riff/src/session_relay.rs:906`,
  `.exec()` at `:920`). No code in `crates/` names SIGTSTP, SIGCONT or
  Ctrl-Z.
- The attach is a plain `podman attach`
  (`crates/riff/src/podman.rs:621`). `lead::attach`
  (`crates/riff/src/lead.rs:585`) reads no state of the process.
- `riff view` is gone: 01M6VGHPP3ZT5TTMSGAXV3ZK7W
  (`docs/src/requirements.md:8194`), PR #1701. The body names
  `riff view` in its Done when, and "the table of `riff view`" in
  `development.md`. The attach is now `riff lead start`, or the key `l`
  in the Sessions tab of the TUI.
- Ctrl-P Ctrl-Q is in "Detach from the lead"
  (`docs/src/start-a-riff.md:75-92`), not in "Let the lead start the
  work". No page says what Ctrl-Z does.
- No issue or pull request names SIGTSTP, SIGCONT or a suspend of the
  lead, but this one.

New title: Ctrl-Z in an attached lead stops its claude, and the next
attach hangs and says nothing

New body:

````markdown
From Mike, 2026-11-02, on thelio.

## What

The container of the lead runs `riff relay --terminal -- claude ...`. The relay runs `claude` with `exec` (`session_relay::terminal`), so `claude` runs under catatonit, and no shell with job control is there. On Ctrl-Z, Claude Code stops itself ("Run `fg` to bring Claude Code back"). Nothing in the container can continue it. The next `riff lead start`, or the key `l` in the Sessions tab, attaches to the stopped `claude`, hangs, and says nothing. A person who then presses Ctrl-C or closes the terminal can end the lead.

On the host, `podman exec riff-lead-OWNER-REPO kill -CONT 2` continues the stopped `claude`.

This issue does not choose the fix: the lead runs again by itself, or each attach continues a stopped `claude` and says so in one line.

Related: #1520 (found in the same session).

Done when:
- A test: after a SIGTSTP to the `claude` of a lead, the lead runs again, or the next `riff lead start` and the key `l` of the Sessions tab continue it and say so in one line.
- A new requirement holds the rule (`just req`).
- Docs: "Detach from the lead" in `docs/src/start-a-riff.md` says what Ctrl-Z does in the lead, and how to get the lead back.
````

## #1525 The deploy of v4.6.0 had a run of 503 of 24 s, and riff release check failed on it

Verdict: question

Why:

- The limit is the same: `LONGEST_503` is 20 s
  (`crates/riff/src/cycle.rs:1666`), and 01M58KMM5ZPAQ289S2YJVQ5WW6
  (`docs/src/requirements.md:7404`) says 20 s. The failed line names
  only the seconds (`cycle.rs:1802`).
- The check passed in each later release, but one: v4.7.0 (#1551),
  v4.8.0 (#1576), v4.9.0 (#1599), v4.10.0 (#1660), v4.11.0 (#1686),
  v4.12.0 (#1711), v4.14.0 (#1820) and v4.15.0 (#1847). Some passed
  after a first "riff could not check it", when the sign-in of gcloud
  had ended.
- It failed once more, in v4.13.0 (#1755): 179 s. That cause is known:
  the secret of the keys of the GitHub Apps was missing, so only `POST
  /v1/github/token` got 503 (#1759). The same comment says that the
  switch of the revision gave about 1 s of 503.
- Nobody found the cause of the 24 s of v4.6.0. Mike resumed the riff
  and kept v4.6.0 (#1516).
- The book lists the check in "Check the release after the deploy"
  (`docs/src/development.md:6899`, the line at `:6951`). No text says
  what a failed 503 check means, or how to find its cause.

Questions for mike:

- Q1: Do you close #1525 as a one-off? Proposal: yes. The switch took
  about 1 s in v4.13.0, and eight later checks passed. If you want the
  cause, read the Cloud Run log of 2026-11-03, 04:40 to 04:50 UTC,
  before about 2026-12-03: a default log bucket keeps its lines for 30
  days.
- Q2: Do you want a new issue for a better failed line? Proposal: yes,
  "A failed 503 check of riff release check names the paths, the
  revision and the times of the longest run". Its Docs line: "Check the
  release after the deploy" in `development.md` says what the lead
  does after a failed 503 check. In v4.13.0, the owner had to read the
  log by hand to find the path.

## #1532 A wave item whose result is a report in its issue, not a pull request, waits for the person and is not started again

Verdict: question

Why:

- Nothing of the design is built. No label `report` exists: a grep of
  `crates/riff/src` and `docs/src` finds no `--label report`. No merged
  pull request built a kind of item with no pull request.
- `STARTS` is still 3 (`crates/riff/src/wave.rs:640`).
  01M4Z81B1913NF6HBZ5G38B3Q9 (`docs/src/requirements.md:6945`) is the
  same: `riff run` gives 0 only with an open pull request.
- A worker adds to an issue only with `gh issue comment`
  (`ISSUE_BY_COMMENT`, `crates/riff/src/text.rs:6346`;
  01M6FZXZ9267YKMH97E6A2Y2EP, `requirements.md:6826`; PR #1544). The
  start text of a worker does not say "alone". The book says it for
  each session in "One step in each call"
  (`docs/src/start-a-riff.md:549`).
- riff does a report today as a pull request that adds a file:
  `design/reviews/` and `design/measures.md`, and this batch
  (`design/backlog/batch-6.md`, #1885).
- Two tools of today cover the rest of the case:
  - The label `question` stops each new start of an issue, as a hold
    does (01M6QBJHM5BQR59QX2K2BMZVYH, `requirements.md:536`; #1605).
  - `riff pr open --refs` (`crates/riff/src/main.rs:1237`) writes
    `Refs #N`, so the merge leaves the issue open
    (01M6XSSCTA3E1GJ8EC9646PP1R, `requirements.md:598`).

Questions for mike:

- Q1: Do you close #1532, with the rule "a report is a pull request
  that adds a file"? Proposal: yes. It works today with the wave, the
  verify and the merge, with no new label. A small new issue records
  the rule (`just req`) and adds a how-to "File a report item", with a
  copyable `sh` block, under "Let riff run the wave" in
  `docs/src/development.md`.
- Q2: For a report that waits for the approval of the person (the case
  of sens-o-matic), how does the issue stay open? Proposal: the worker
  opens the pull request with `riff pr open --refs`, so the merge
  leaves the issue open. The lead then adds the label `question`, so
  the wave starts no new session. The person closes the issue to
  approve.
- Q3: Does the start text of a worker get the line "Run each `gh issue
  comment` alone, in its own call. Close no issue"? Proposal: yes, in
  the same small issue. It stops the refusal of #1562.

## #1542 Design: identity: who acts in riff, the person or a session, and with which rights

Verdict: close

Why: each line of its "Done when:" is met.

- Each question has the decision of Mike in the body (2026-11-06).
- Each decision has its own issue, and both sides name what they take
  from this design:
  - #1543 (each session acts on GitHub as itself) closed on 2026-11-08.
    Its body starts "Part of #1542", and names Q1 to Q3. Its build
    items are #1722 to #1726. Each is closed but #1723, whose code
    merged in PR #1746, and which waits for its check after the
    release.
  - #571 (a proof of presence) closed on 2026-11-06. Its body starts
    "Part of #1542". Its build items are #1657 and #1658 (Wave 55) and
    #1659 (Backlog).
- 01M6WG4PFA843R82J0N6MW33VP supersedes 01M4Z60B88DC67N3GVHY8JGRPQ in
  part (`docs/src/requirements.md:7155`), and the old text keeps only
  the plain `claude` of the person (`requirements.md:7147`).
- More of the design shipped: no session gets the token of the person
  (PR #1838, #1815), and the lead App reads the security alerts (PR
  #1842, #1839).
- The book says who acts on GitHub for each role: "Who acts on GitHub
  for each role" (`docs/src/how-it-works.md:6243`), under "The auto
  mode settings of a session" (`how-it-works.md:6117`).

## #1583 A token request that gets no OAuth error body says "refused: no reason", names no HTTP status, and is not tried again

Verdict: rewrite

Why:

- The main fault holds. `Api::token`
  (`crates/riff-client/src/api.rs:650`) makes `TokenRefused { error:
  "no reason" }` for each reply that is no success and has no OAuth
  error body (`api.rs:667-670`). The error names no HTTP status.
- The claim "is not tried again" is wrong for the replies that the
  body names. The token call goes through the link with its new tries
  (`api.rs:651-662`). `link::outcome` makes each 5xx or 429 with no
  build header a fault, and each 503 a fault
  (`crates/riff-client/src/link.rs:354-364`). A fault of the front end
  ends as an `Outage` error, which names the status
  (`api.rs:995-997`, `api.rs:2324-2331`).
- So "no reason" comes from two cases:
  - a 503 of riff-server with a plain body that lasts the whole budget
    of the call: the gate replies "riff-server does not serve now. Try
    again." (`crates/riff-server/src/lib.rs:3072-3081`), and the link
    gives the last reply when the budget ends (`api.rs:1032-1033`);
  - a refusal with a plain body, for example a 500 of riff-server.
- The token handler of riff-server gives an OAuth body also for its own
  503 (`temporarily_unavailable`, `riff-server/src/lib.rs:2883`).
- No requirement covers a token reply with no OAuth body. No pull
  request fixed it.
- The book: `docs/src/design-link.md:124` says "A 502, 503, 504 or 429
  with no build header", but the code makes each 5xx with no build
  header a fault (`link.rs:355`).

New title: A token request that gets a reply with no OAuth error body
says "refused: no reason" and names no HTTP status

New body:

````markdown
Seen by the lead on 2026-11-04 at about 02:25 UTC. A `read` of the riff tools failed with `riff-server refused the token request: no reason`. The next `read`, a few seconds later, worked.

## What

`Api::token` (`crates/riff-client/src/api.rs`) makes `TokenRefused { error: "no reason" }` for each reply that is no success and has no OAuth error body. The error names no HTTP status. The link already tries a fault again, and a fault of the front end ends as an `Outage` that names its status. So "no reason" comes from a 503 of riff-server with a plain body that lasts the whole budget of the call, or from a refusal with a plain body, for example a 500.

- A reply with no OAuth error body gives an error that names the HTTP status and the first line of a plain body, for example "riff-server answered the token request with HTTP 503: riff-server does not serve now. Try again." It is not a `TokenRefused`, so the sign-in stays.
- `TokenRefused` stays for an OAuth error body, for example `invalid_grant`.
- The rule of the new tries does not change.
- `docs/src/design-link.md` says "a 5xx or a 429 with no build header", as `link::outcome` does.

Done when:
- A test with a fake server: `/v1/token` gives a plain 503, then a 200, and the call passes.
- A test: a plain 503 for the whole budget gives an error that names 503 and the text.
- A test: an `invalid_grant` body still gives `TokenRefused`.
- A requirement records it (`just req`).
- Docs: the table of the faults in `docs/src/design-link.md` is right. No other page of the book changes, unless it shows the old text.
````

## #1602 Real-session test 1793773482

Verdict: close

Why:

- It is a scratch item of `just real-session`, and the test did not
  close it. Its pull request #1603 (branch `worktree-issue-1602`)
  closed with no merge on 2026-11-04 at 06:37:18 UTC, and its branch
  is gone. The issue has no close event.
- The scratch line is not in the book: `docs/src/introduction.md` ends
  with "agents work the same way."
- The clean-up of the test closes its issue as not planned, with the
  comment `CLOSE_TEXT` (`crates/real-session/src/lib.rs:2553-2567`,
  `CLOSE_TEXT` at `lib.rs:346`). Each other issue "Real-session test
  N" is closed; the newest is #1849.
- Close it as not planned, with the comment "The real-session test
  closes its scratch item."

## #1642 TUI G: the Devices tab: the list, the add with a QR code, the remove

Verdict: rewrite

Why:

- The tab is not built. It shows "The Devices tab is not built yet."
  (`crates/riff/src/tui/view.rs:580`; `crates/riff/src/tui.rs:38`;
  `docs/src/start-a-riff.md:136`). The other build items of #994, A to
  F (#1636 to #1641), are closed.
- No command `riff device` exists: `crates/riff/src/main.rs` has no
  such subcommand.
- The "Needs:" line is old. It names "the device commands of #962". But
  #1657 (Wave 55) now builds `riff device add --key NAME`, `riff device
  list` and `riff device remove NAME`, and says "#962 adds the phone
  later". #962 is open in the Backlog.
- The book has the old order: `docs/src/riff-uis.md:271-274` says that
  the phone design (#951) "has the commands of the devices, and builds
  them first".

New title: TUI G: the Devices tab: the list, the add, the remove

New body:

````markdown
Build item G of the TUI design #994.

Needs: #1636 (item A, done), #1657 (`riff device list`, `riff device add --key` and `riff device remove`), #962 (the pair of a phone with a QR code).

## What

The Devices tab: each device with its kind, its name, its scope and its last use; the add of a key, as `riff device add --key` does; the add of a phone with the QR code of `tui-qrcode`, as the pair of #962 does; the remove. `docs/src/riff-uis.md` says that #1657 builds the device commands first.

## Rules of each build item of the TUI

The design is in the body of #994, which Mike signed off on 2026-11-06 (https://github.com/como-technologies/riff/issues/994#issuecomment-6676359668): the design and the proposed answers to Q1-Q7, with one exception: `riff top` stays as it is (#1632). Read the part of the design for this item, and "The keys", "The narrow layout" and "The data, and what a refresh costs".

- Each choice is a new requirement statement in "The UIs" of `requirements.md`, added with `just req`.
- The book changes in the same commit.
- Screen tests draw the mock-up of the tab into a ratatui `Buffer` at 80 x 24, and at 40 x 24 for the narrow layout, and compare the rows. Unit tests and doc tests too.
- Each action calls the code of its CLI command. Each action that changes something asks `y` or `n` first.

Done when:
- A screen test of the Devices tab at 80 x 24 and at 40 x 24 passes.
- The Devices tab lists, adds a key, adds a phone with a QR code, and removes.
- Docs: "Pair a phone" in `docs/src/riff-mobile.md` tells the tab, with a copyable `sh` block, and "Add an authenticator" in `docs/src/how-it-works.md` (the how-to of #1657) tells the tab too.
````

## #1653 /msg USER TEXT in riff chat: a direct line to a person, in the direct thread of the two people

Verdict: keep

Why:

- `/msg` is not built. `chat::typed` knows only `/me`, `/quit` and `//`
  (`crates/riff/src/chat.rs:239`), and an unknown command names only
  "/me, /quit" (`chat.rs:271-273`). The book says "The chat knows only
  `/me` and `/quit`" (`docs/src/how-it-works.md:3895`).
- The book has no heading "Send a direct line to a person".
- Its need is met: #1652 (the highlight) closed with PR #1766. A line
  of the other member of a direct thread is already a highlight
  (`crates/riff/src/highlight.rs:56`).
- The body, its "Done when:" lines and its Docs line still fit.

## #1654 The Chat tab of the TUI shows the highlights and the /msg lines of riff chat

Verdict: keep

Why:

- The Chat tab marks no highlight. Only `chat.rs`, `highlight.rs`,
  `main.rs` and `settings.rs` in `crates/riff/src` use the highlight;
  no file of the TUI does.
- The book says so: "The Chat tab of the TUI and the phone show no
  highlight yet." (`docs/src/how-it-works.md:3797-3798`, in "Get a
  highlight in the chat").
- Its needs: #1640 and #1652 are closed; #1653 is open. So it still
  waits, and the body is right.
- A note for the lead: the highlight part needs only #1652. If Mike
  wants it sooner, the `[msg]` part can move to a new issue after
  #1653.

## #1659 Proof of presence 3 (later): a signed line in riff chat shows as proved

Verdict: keep

Why:

- Nothing built it. No `/sign` command exists in `crates/riff/src`, and
  no code of WebAuthn or FIDO2 is in `crates/`.
- The book has no heading "Sign a line in the chat".
- Its needs are open: #1657 and #1658, both in Wave 55.
- Mike asked for this Backlog item in Q3 of #571: "Signed chat lines
  `(3)` later, after the approvals. still want a backlog item for
  this."
