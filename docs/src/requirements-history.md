# Requirements history

A requirement that a new requirement replaced in full lives here. It
keeps its ID, its text and its section. It is not a rule of today: the
page [Requirements](requirements.md) holds the rules of today.

## Product

- **01M3MEHCGZ4AG4C2A77J5HA3P7** The book page "Join a Riff" has the
  steps to join a riff with sign-in: install `riff`, put the address of
  the riff in `RIFF_SERVER`, and run `riff connect claude`. The steps
  are the same for a second machine of the owner and for a person that
  the owner invites. The page replaces "Add a Machine".
  Superseded by 4.0 (#1173).
- **R44** Only Claude Code is supported for now. `riff connect claude`
  installs a Claude Code plugin: the MCP server, a skill, and a start
  hook that wakes the session. Superseded by 4.0 (#1173).
- **R52** The plugin files live in the riff repository. The `riff`
  binary carries a copy of them, so the plugin matches the binary.
  Superseded by 4.0 (#1173).
- **R53** `riff connect claude` writes the plugin as a local marketplace
  and installs it with the `claude` command. The same command updates
  the plugin. Superseded by 4.0 (#1173).
- **R54** A new session can start in any directory of a repository.
  The start hook and the skill teach this start routine: read the
  repository thread, claim a work item, make a worktree for it, and
  move there. Superseded by 4.0 (#1173).
- **R162** A Claude session makes the worktree for a work item with
  the `EnterWorktree` tool. The worktree is `.claude/worktrees/ITEM`.
  Superseded by 3.0 (01M4Z60B7PTJMB37578K8FV22Q).
- **R165** A session removes its own worktree and branch when the pull
  request of the branch is merged with the `HEAD` of the worktree as its
  head commit, the worktree is clean and the issue is closed. It deletes
  the branch only while the branch points at that commit. It never
  removes a worktree of another live session. A worktree with no owner
  goes to the thread. Superseded by 3.0 (01M4Z60B7PTJMB37578K8FV22Q).
- **01M3Z9N6AK6W9KCA1MN72X78B6** One context holds one item. The work
  of a worker on an item ends at its verify request. After the request,
  the worker writes the state on the issue as a comment: the pull
  request, the commit, what is left after the merge, and what a session
  must know when the verify fails. Then it releases the item and ends
  its turn. It does not wait for the verify, and it starts no second
  item in that context. The release is its last release, so the worker
  must clear its context (01M3X9XAK1KPZZVM1AJR2H8DSS). The skill and
  the start hook say so. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3MRDEXSMFT0STXF2HAR2QZ6** A live check of new code, for example
  a new plugin command, hook or skill text, runs in a dev session
  (`just dev`) before the verify. It needs no release and no update of
  the machine. A check after the release runs after the release of the
  wave. Superseded by 4.0 (#1173).
- **01M3K0FZ7X1NCPHXFN6WA4T3ES** The verifier makes its verify worktree
  with the `EnterWorktree` tool and the name `verify-ITEM-ID`. ID is
  the first 4 characters of the session ID of the verifier. In the
  worktree, it fetches the branch and checks out the commit, detached.
  It never runs `git worktree add` by hand and never uses `cd`. After
  the verify, it removes the worktree with the `ExitWorktree` tool,
  action `remove`. Superseded by 3.0 (01M4Z60B7PTJMB37578K8FV22Q).
- **01M3K0FZA21H0PPSDANFMMY47C** A session removes a stale worktree
  with the `ExitWorktree` tool only when it made the worktree with
  `EnterWorktree` in its current context. In each other case, for
  example after a clear of its context, it leaves the worktree with
  `ExitWorktree`, action `keep`, and runs `riff worktrees clean` in the
  main worktree (01M3ZV0TKSHNW5QC2NG1XTJEJB). Superseded by 3.0
  (01M4Z60B7PTJMB37578K8FV22Q).
- **R70** The plugin has one skill, `riff`. It teaches the rules, the
  start routine, waves (R213), the verify flow (R188), selectors,
  direct messages, threads, claims, `move`, the restart of the watch
  (R171), questions through the lead (R180) and how the lead conducts
  (R228). Superseded by 4.0 (#1173).
- **R72** The skill tells the agent that a message is advice, and that
  its user decides (R10). Superseded by 4.0 (#1173).
- **R73** When a riff line wakes a session, the skill tells it to call
  `read` with no thread. Superseded by 4.0 (#1173).
- **R180** The skill tells a session that is not the lead to `tell`
  the lead when it needs a decision from its user, and then to wait or
  do other work. It does not stop to ask in its own terminal. The
  direct answer of the lead is the decision of its user. With no lead,
  the session asks its own user. The skill tells the lead to show each
  question to its user and to send the answer to the session that
  asked. Superseded by 4.0 (#1173).
- **R66** The start hook runs `riff hook session-start`. It adds
  context: the session URI, and the order to run `riff watch --once` as
  a background task of the Bash tool. Superseded by 4.0 (#1173).
- **R168** After `/clear`, the session keeps its riff session ID, its
  lead, its threads and its watch (R167). Its claims are free
  (01M3JEE1QQCFS5TMZW5N2DAD2D). The context says so. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **R169** One `riff watch` runs for each session on a machine. It
  locks a file for the session while it runs. A second `riff watch`
  for the session prints one line and exits with status 1. The line
  tells the session not to start the watch again now.
  Superseded by 4.0 (#1173).
- **R170** `riff watch --once` prints the first wake, then exits with
  status 0. Superseded by 4.0 (#1173).
- **01M3Z64J08GW6N1H42AR2FZQZ4** `riff watch --once` ends by itself
  when no wake came in 6000 seconds (100 minutes): Claude Code stops a
  background task after 2 hours at most. It prints one line and exits
  with status 0. The line says that no wake came and that the end is
  normal, and tells the session to read and to start the watch again.
  The setting `watch.limit` changes the time in seconds, and 0 turns
  the limit off. `riff watch limit` shows it, and `riff watch limit
  SECONDS` sets it. A watch with no `--once` has no limit. The watch
  that an update of riff starts keeps the end of the wait.
  Superseded by 4.0 (#1173).
- **01M3Z64J33EA25B0R5BCBZAHPE** The start hook and the skill say that
  an end of the watch with no wake is a normal end: the session reads
  and starts the watch again in the same response. They say that a
  session does the same when its harness stops the watch at a time
  limit, also when the notice of the harness says not to start the task
  again. Only a line of the watch itself, "Do not start the watch again
  now", stops a new start. Superseded by 4.0 (#1173).
- **R69** The start hook never stops a session start. It exits with
  status 0, also when riff cannot find the session.
  Superseded by 4.0 (#1173).
- **01M3JN21T9C5GX6VX8N032JYWE** The start hook runs `git fetch
  origin` for at most 2 seconds, at the same time as it reads the
  state of the riff. When the default branch of the clone is behind
  `origin`, the context says by how many commits. It tells the session
  to ask its user to pull, through the lead, and names the main
  worktree. The hook does not pull. Superseded by 4.0 (#1173).
- **01M3JN21WDXWTHDKXKQ80ZPYPK** With no remote, a remote that cannot
  be reached, or a fetch that takes longer than its limit, the context
  has no such line. The hook still exits with status 0 (R69).
  Superseded by 4.0 (#1173).
- **R74** `riff connect claude` writes the plugin to
  `$XDG_DATA_HOME/riff/claude-plugin`. Without `XDG_DATA_HOME`, it uses
  `~/.local/share/riff/claude-plugin`. Superseded by 4.0 (#1173).
- **R75** `riff connect claude` removes the user-scope MCP server entry
  `riff`, if it exists. The plugin gives the riff tools instead.
  Superseded by 4.0 (#1173).
- **R76** Replaced by 01M3XY2SNXQJRSH5QX82AFVM2S.
- **R77** `riff connect claude` does not need a riff session or a git
  repository. It works in any directory. Superseded by 4.0 (#1173).
- **01M3XY2SHGXQR9NVXF7QJBN09T** riff is off in a Claude Code session
  until a person turns it on for the repository of the session. The
  state is the entry `riff@riff` in the key `enabledPlugins` of the
  Claude Code settings. The first file that has the entry decides, in
  this order: the local settings of the repository
  (`.claude/settings.local.json` at its top), its project settings
  (`.claude/settings.json`), the user settings. A directory that is
  not in a git repository is off. Superseded by 4.0 (#1173).
- **01M3XY2SKQ27K3TE4NV28FHTVV** `riff enable` writes the entry `true`
  to the local settings of the repository of the working directory:
  in a linked worktree, to those of the main clone. `--shared` writes
  it to the project settings. `--global` writes it to the user
  settings. `riff disable` removes the entry from the same file. With
  no flag, when another file then still turns riff on, `riff disable`
  writes `false` to the local settings. Each command changes only the
  entry of riff, as text: each other byte of the file stays. It
  changes no other file, and says whether riff is on in the working
  directory. With `--global`, the answer of
  01M3XY2SNXQJRSH5QX82AFVM2S becomes `global` or `none`.
  Superseded by 4.0 (#1173).
- **01M3YCGKGP3VC93S8FA1G4K3QK** `riff enable` and `riff disable`
  write through a symbolic link, and name the real path of the file
  that they wrote. Before they write the local settings of the main
  clone of a linked worktree, git must confirm the worktree: its
  common directory (`git rev-parse --git-common-dir`) is the `.git` of
  that main clone, and the file `gitdir` of the entry of the worktree
  in the main clone names the `.git` file of the tree. If not, the
  command writes nothing, and says why. Superseded by 4.0 (#1173).
- **01M3ZGT8ST7HCK6J7VZJ09XE0M** The `.git` of a linked worktree is
  the file that the entry of the worktree names, not a symbolic link
  to it. When `TOP/.git` is a symbolic link, `riff enable` and
  `riff disable` write nothing, and say why. The check of the `gitdir`
  file follows no link at the last part of a path.
  Superseded by 4.0 (#1173).
- **01M3XY2SNXQJRSH5QX82AFVM2S** `riff connect claude` adds the
  marketplace to Claude Code with the `claude` command on the PATH.
  `--claude PATH` names another one. It installs the plugin in no
  scope, and turns riff on nowhere by itself. In a terminal, it asks
  one time where the person wants riff on: only in this repository
  (the default), in each repository on this machine, or not now.
  `--scope repo|global|none` gives the answer with no question. riff
  keeps the answer in its settings, `connect.scope`. With an answer
  there, or with no terminal, it asks nothing and turns riff on
  nowhere. Superseded by 4.0 (#1173).
- **01M3XY2SR3VJZAKEPC6CBCS292** An update never turns riff on for
  each repository: only the answer `global` writes the entry `true` to
  the user settings. `riff update` asks nothing, also in a terminal:
  it runs `riff connect claude` with no terminal. With no terminal,
  `riff connect claude` asks nothing, and a new install stays off. An
  install of a release up to v0.8.0 is an old install: the user
  settings have the entry `true`, because that release installed the
  plugin in the user scope, and riff has no answer. Its choice is each
  repository. On an old install `riff connect claude` asks nothing,
  with a terminal and with no terminal: it keeps the entry, records
  the answer `global`, and says in one line that riff stays on in each
  repository and that `riff disable --global` changes it. So no person
  does a thing at the update from v0.8.0. With `--scope repo` or
  `--scope none`, `riff connect claude` removes that entry, and names
  each repository of the Claude Code state file whose settings have a
  riff permission rule, with the command `riff enable`.
  Superseded by 4.0 (#1173).
- **01M3XY2ST8R67SKTXJECAYJZRX** Where riff is off, each entry of the
  plugin does nothing: `riff hook` and `riff statusline` make no call
  to the server, run no `git`, and print nothing. `riff mcp` serves no
  tool, makes no call to the server, and names `riff enable` in its
  instructions. A `riff mcp` that an update starts again serves a
  session that runs, so it goes on. Superseded by 4.0 (#1173).
- **01M3XY2SYKG91SAB2FS1QNCZ2H** The state is obvious. The last line
  of `riff connect claude` says where riff is on, or
  `riff is installed but off`, with the command to change it.
  `riff server` shows `riff on` or `riff off` for the working
  directory, the file that decides, and the command to change it. The
  start context names the file that turned riff on, and
  `riff disable`. Superseded by 4.0 (#1173).
- **01M53NEE536G72QMVXCVTS28ES** When riff is off in the working
  directory, the last line of `riff connect claude` names each
  repository of the Claude Code state file where riff is on: the main
  clone of each project. It says `riff is installed but off` only when
  riff is on in none of them. So the line of an update, which runs in
  the home directory, is true. Superseded by 4.0 (#1173).
- **01M3XY2T0R2Q39XYX8AYV7T0RK** When the Claude Code state file has
  `plugin:riff:riff` in `disabledMcpServers` of the project, a person
  turned the riff server off there in `/mcp`. The start context then
  says that the session has no riff tools, how to turn the server on,
  and to tell the lead with `riff tell lead`. The status line says it
  too. Superseded by 4.0 (#1173).
- **01M3XY2T542DCHBN95H9PX4AGQ** riff starts no worker where riff is
  off in the main clone of the repository: `riff workers start`
  starts nothing and names `riff enable`, and the rollout of the lead
  and a workers host start none.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3YCGKKRDNFC338K1JSK30JK** When riff is off in the main clone of
  the lead, the rollout posts one note to the lead: the host, the
  reason and `riff enable`. The next note comes only after riff was on
  there again.
  Superseded by the re-cut (01M53NGVE839TRH7XB7CDPQNZE).

## Waves on GitHub

- **R225** The skill gives a command for each step of the waves: see
  the open waves, see the items of a wave, see the items with no wave,
  make a wave, put an item in a wave and end a wave.
  Superseded by 4.0 (#1173).

## Pull requests on GitHub

- **01M3NB6G132QG4TAEJ5QPRJNAE** The skill names one `riff` command
  for each step of a pull request: open it, wait for the merge, report
  a verify. It has no `gh` recipe and no shell loop for these steps.
  Superseded by 4.0 (#1173).
- **01M4YJ6KGBPTCSVMFQCDKACZPV** A patch of an older release line goes
  on the branch `release/MAJOR.x` of that line, by pull request. The
  branch starts at the last release tag of the line. CI runs the `Gate`
  and `Hygiene` on each pull request and each push of a branch
  `release/**`, as on `main`. A push to it does not deploy. The
  command `just github` makes the ruleset `release-branches` on
  `release/**`: a change needs a pull request with 0 approvals and the
  checks `Gate`, `Hygiene` and `riff/verify`, as on `main`. No force
  push, no deletion, no bypass actor. The checks do not stop the push
  that makes a new branch of a line. Superseded by
  01M5RG6BZQC8T6SZ8NX9W8TT5M.
- **01M5QWJJ94T4M3KJB0R3WTJCQF** riff pr open --base BRANCH opens the
  pull request against that branch, for example release/1.x, and not
  against the default branch. The body, the trailers and the auto-merge
  are the same. The steps for a patch of an older release line use it,
  not gh pr create. Superseded by 01M5RG6BZQC8T6SZ8NX9W8TT5M.

## Tokens on GitHub

- **01M3Y1YP15C7AT2N70BWQP8PE2** riff keeps the marks of each session
  on its machine, in a directory that holds over a restart: each
  transcript of the session, and each claim with its start time and
  its end time. The start hook, the Stop hook and the end hook record
  the transcript. A granted claim records the start time.
  Superseded by 4.0 (#1173).

## Pause

- **01M3JCG3T8AJZN31SZQQTP3FAF** Replaced by
  01M3XAHZDSQR263QZVB41CK0MX.
- **01M3JCG48QPCNNTKW34FTR0AMR** The start hook and the skill tell a
  session the state of the riff. They tell it to pick a free item only
  when the riff is running. When the hook cannot read the state, it
  tells the session to call `whoami`. Superseded by 4.0 (#1173).

## Service

- **01M3JY12HASECNN6SFQ880JT5H** `just dev [ARGS]` builds the
  workspace in debug. It runs the debug `riff-server` of its tree with
  `ARGS` on the first free local port from 7900, with its log in
  `target/dev-server.log`. Then it runs Claude Code with the plugin of
  its tree (`--plugin-dir`), the installed plugin off, the debug
  `riff` first on `PATH`, and `RIFF_SERVER` set to that server. When
  Claude Code ends, also on Ctrl-C, it stops the server.
  Superseded by 4.0 (#1173).
- **01M3MRDESPG8VGMQ1F6KFJXBC5** Each machine has two tracks. The
  installed `riff`, `riff-server` and plugin are a release. Only
  `riff update` changes them, and the sessions riff with them. The
  code under test runs from its worktree, against a `riff-server` of
  the same worktree on a free local port. It never talks to the shared
  riff. `just dev` changes nothing that is installed.
  Superseded by 4.0 (#1173).
- **01M43B491Z25KT0XBC7CANFS5G** Replaced by 01M49NP2907J4SH4S6MAY09VXE.

## Saved state

- **R154** Replaced by R125.
- **R128** Replaced by 01M3XA877YZQ649SWB5TN60V5P.
- **01M3TFG527M04TA7ESM970X3B8** Replaced by 01M53T8XH45WSVKS9FV3VXF2X8.
- **R149** Replaced by 01M3T411BZQB8N4D2S0JFVESMS.

## The log

- **01M3WRD9DYJWVN1QRBAC3ZVVZD** Replaced by
  01M3Z8MRDZEKTXSKZTDTDSCZ3W.
- **01M3X9XA3H6YF0QCYSNB2P0CT2** Only a `register` or a `start` that
  the session sends makes the first lead of a user in a repository
  (R176), and never for a worker. The `register` that the engine runs
  first makes no lead. `lead` refuses a worker. Superseded by
  01M5NJAY3FTR005TDJJY1TMGNX.
- **01M3X9XAK1KPZZVM1AJR2H8DSS** A worker that frees its last claim
  with its own `release` is in MustClear: the `released` record has
  `must_clear`. `apply` only stores it. A `session_started` record with
  the reason `process` or `clear` ends it. `claim` refuses a session in
  MustClear before each other check, with the code `must_clear` and the
  text "clear your context first: end your turn and riff clears it, or
  type /clear". A claim that goes by a start, an end, a release by the
  lead or a claim of another session gives no MustClear. Superseded by
  3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3X9XB37TQCXWPNFZRMRGJB4** The reply to the release that puts a
  worker in MustClear carries the ask to clear (`must_clear`). The
  reply to each keep-alive of a worker in MustClear carries it too
  (`clear`). `riff release` and the `release` tool show the ask.
  Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3X9XBMB3R718Z81BYXTHMZ0** `riff-server` sends no wake to a
  session in MustClear. The message is in its thread, and its `posted`
  record names the session. A watch that starts gets no missed wake
  while its session is in MustClear. The `session_started` record that
  ends MustClear gives the session the wake that it missed. A watch
  that starts after that record gets it too. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3XV0588C2XZKZ3NM67JXCKJ** One message gives a session one wake.
  When a `posted` record and the `session_started` record that ends
  the MustClear of its session are in one chunk, the session gets only
  the missed wake, and no second wake for the `posted` record.
  Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3X9XC99KY4RQY36A7CYWY11** `who` gives `must_clear` and
  `fresh_secs` for each session: the seconds since its last
  `session_started` record with the reason `process` or `clear`. The
  state `must_clear` comes after `blocked` and before `waiting`
  (01M3QB6CJ1XCQG5B1BVR8AF3B4). `riff who`, `riff top`, `riff workers`
  and the `who` tool show it as `must clear` in yellow, with the detail
  `must clear its context before its next claim`. Each worker that is
  not offline shows `fresh start 12m ago` as the last fact of its
  detail, when the log has such a start. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3X9XD8QWHS2CXTFSQK0PN1Y** The checkpoint holds the worker mark,
  the MustClear mark and the time of the last fresh start of each
  session. A start from a checkpoint gives the life cycle of a full
  replay. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).

## Cloud

- **01M3K0Q854K18DGXJKQ427W586** Replaced by 01M3Q5VE74608N5H2M73RB6Y2Z.
- **01M3K0Q892KWM76R9DJC1P37JA** `riff update` updates riff on a
  machine: it installs `riff` and `riff-server` of a release from the
  repository with `cargo`, then updates the plugin with
  `riff connect claude`.
  When the server on the same machine runs another build than the new
  `riff-server`, it tells the person to start `riff-server` again. It
  does not restart it. That server is the server of `riff` when it is
  on the same machine, else `http://127.0.0.1:7878`. Superseded by 3.0
  (01M4Z60B7H67YSP22NMGT3DM7X).
- **01M3K0Q8C9NK4NY6TJWRMJS7ZQ** Replaced by 01M4262DQ9RNFNJ07CRTSGEAM1.
- **R160** Replaced by 01M3NJAZ6BYH7TWKDYTVEK78PG.
- **01M3MMZQ3KTF5Z3GXNR7DRQ65Z** Replaced by 01M3NJAZ8HSE87H0GZ8SNGWN31.
- **01M3MRMASMP59PKHAV92XSV7XE** Replaced by
  01M3N73AW2J3TVSZWFJ88A91PG.
- **01M510MPTKK285C9Z2TW54M5P7** Replaced by 01M56N6KSMFJ46PW99Z80GF9DM.
- **01M550XYBAW5QYXEMQ49Z34M4N** Replaced by 01M56N6KVZRGT3W7CEK05DHR57.
- **01M553AWMXYAG7CJGW7M30XDZ8** Replaced by 01M56N6KVZRGT3W7CEK05DHR57.
- **R161** Replaced by 01M3NJAZAQ3AKMAM0EGM7R3S89.
- **01M3ZE3Z26N1CG090D5D5FZ3NW** Replaced by 01M496JTN962N0AX378MA1MBPM.
- **01M4262DY8NN30SC4REYX2G9DV** Replaced by 01M558C4VZ2PSMEYPAZBRYW1VV.
- **01M56N6KSMFJ46PW99Z80GF9DM** The GitHub releases of a retired line
  stay normal releases. When a line retires, no release of it becomes a
  prerelease, and its notes do not change. The line 1.x retired after
  riff 3.0.0 (#961): its 12 GitHub releases are normal releases, and
  GitHub marks `v3.0.0` "Latest". The line 2.x retired before this rule
  (#706): its one GitHub release, `v2.0.1`, is a prerelease. The tag
  `v2.0.0` has no GitHub release. Superseded by
  01M5RG6BTXAJY8DV6MT315VZWF.
- **01M56N6KVZRGT3W7CEK05DHR57** riff has no command that retires a
  release line. `riff --help` lists no `retire`, and `riff retire`
  fails: riff does not know the command. Superseded by
  01M5RG6BTXAJY8DV6MT315VZWF.

## One instance

- **01M3THEE08ZKV8WGHDSVWV69ZE** Replaced by 01M52JYGHHK5X0XS39VFT3ZGGM.
- **01M3THEE31H5QVV3JAFC4ZRGFR** Replaced by 01M52JYGM7WH565B7PCVBEJC17.
- **R140** Replaced by 01M52JYGPX5ETP3HKR0S0Y0YCR.
- **01M53NHK07KNXJ7FXS26NB1675** Replaced by 01M56T4SZ4Y6PSY1VTVA3758FT.

## Sessions

- **01M3JEE1QQCFS5TMZW5N2DAD2D** A new start of a session is blank: a
  new agent process, a resume or a `/clear`. The start hook sends a
  start call, and the claims of the session are free at once. The
  session keeps its ID (R58, R167, R168), its threads, its read cursors
  and its lead. A compaction is not a new start.
  Superseded by 4.0 (#1173).
- **01M3WFYETKXPWWE0R0EAKGCD1E** The context of a new start lists the
  earlier work of the clone that no live session owns: at most 8 items,
  each with its pushed worktree branch and its worktree. A live session
  owns the work when it holds the claim of the item, or works in its
  worktree. The fetch of the start hook prunes
  (01M3JN21T9C5GX6VX8N032JYWE). Superseded by 4.0 (#1173).
- **01M3ZT8296G8DZFRSKYM6V5XTH** The skill has one WIP block, in "Push
  your work as WIP". "Pause" and "Pick up dropped work" use it. Its
  push works after a rebase, and fails when it would drop newer work
  of the pushed branch (`--force-with-lease --force-if-includes`).
  When the push of an earlier worktree
  fails, the pull names the branch:
  `git -C PATH pull --rebase origin BRANCH`. Superseded by 4.0 (#1173).
- **01M3JNVBPMZ1K9WX7Q7DP6Y0DH** Replaced by
  01M3XA87HE06Z6M32ZJPSYSYRZ.
- **R176** The first session of a person in a repository becomes the
  lead, with no action. It is first when no other session of the
  person in that repository holds (R9). A later session does not
  become the lead. A worker never becomes the lead
  (01M3X9XA3H6YF0QCYSNB2P0CT2). Superseded by
  01M5NJAY3FTR005TDJJY1TMGNX.
- **R177** `riff lead` and the `lead` tool make the session the lead
  of its person in its repository. It replaces the old lead. Only an
  agent session in a repository can be the lead. Superseded by
  01M5NJAXTQ0YKN7Q7XN0A7ZMPF.
- **R229** The skill tells the lead how to conduct: see the claims and
  the status of each session of its person (`who` and a status
  request), give each free session one clear item with `tell`, check
  the progress, answer questions (R180), and give a blocked session an
  answer or a new item. Superseded by 4.0 (#1173).
- **R57** `riff mcp`, `riff watch` and the hooks find their session by
  the session ID, not by the directory. Superseded by 4.0 (#1173).
- **R167** `riff mcp` writes its session ID to a file on the machine,
  for its agent process, and locks the file while it runs. `riff
  watch`, the hooks and the `riff` commands of an agent session use
  that ID before the ID of the agent tool. `RIFF_SESSION` comes first.
  So `/clear` does not change the riff session ID. The file is in
  `$XDG_RUNTIME_DIR/riff`, else `$XDG_STATE_HOME/riff`, else
  `~/.local/state/riff`. A file with no lock does not count.
  Superseded by 4.0 (#1173).
- **01M517N75C0Z5BAFQAWRN3YTYB** An agent that runs inside a riff
  session is not a riff session, for example a `claude -p` that a Bash
  command of the session starts. riff knows it by a live record of
  `riff mcp` (R167) of a process above the agent. The agent of a hook
  is the process in `CLAUDE_PID`. The agent of `riff mcp` is its
  parent. The hooks of that agent make no call: no start, no end. Its
  start hook gives one line of context that says so. Its `riff mcp`
  has no tools and makes no call. Superseded by 4.0 (#1173).
- **01M51JBET7R9QTT15BXDPM5QG7** The skill says what a session does
  with that message. After a new start, it claims the item again when
  it still works on it. After a release of the lead, it stops the work
  on the item, pushes its work as WIP, writes the state on the issue,
  and claims the item again only when the lead asks it to.
  Superseded by 4.0 (#1173).
- **01M3MEW73CDSJDSKX32XW80WZH** Replaced by 01M3Q63MVZ74WPNBA3QJYQGHFG.
- **01M3MEW75WC7Y4M1BKQ7SXRPNR** Replaced by 01M3Q5VE2D244XDZRYXM8DNSRS.
- **01M3N754NY5JX4P0SN8R4ZYFG9** Replaced by 01M3Q63NK0AHM25MB258B0K8XP.
- **01M3JDWA0WZWKF3JT3NYA2FV5Z** `riff statusline` prints the status
  line of a Claude Code session: `riff`, the short session ID of
  `riff who`, `lead`, each claim, and `blocked`. It is the
  `statusLine` command in the Claude Code settings. A plugin cannot
  set it. It never fails, and it waits at most 2 seconds for
  riff-server. Superseded by 4.0 (#1173).
- **01M3T5GFVS8NMA992KHZN4VE17** `riff statusline` calls
  `GET /v1/me`, not `who`. The reply holds only the session of the
  caller: its state, claims and status, and the build of the server.
  The call changes nothing: it is not a call of the session, and it
  adds no session. Superseded by 4.0 (#1173).
- **01M53TZ0RBSJQ9ZZRXKMBM5R3E** `riff statusline` keeps a good answer
  of riff-server for 10 seconds, for each session and each server. In
  that time, it asks riff-server nothing: no call, so no swap for a
  session token. It makes its tags at each run, from the local files
  and the build of the kept answer. The time is now 90 seconds
  (01M547VMQ5PCJ1GGC4M0GDCT31). Superseded by 4.0 (#1173).
- **01M547VMMDPB4DXBMWVERF60C4** The `riff mcp` of a session keeps the
  answer of riff-server for the status line of the session: at a
  keep-alive, at most each 30 seconds, and after each call of the tools
  `claim`, `release`, `lead`, `blocked` and `join`. It uses the token
  that it holds, so `riff statusline` shows the line with no call to
  riff-server and no call to the OS keyring. Superseded by 4.0 (#1173).
- **01M547VMQ5PCJ1GGC4M0GDCT31** `riff statusline` keeps a good answer
  for 90 seconds: longer than the 60 seconds of the keep-alive of `riff
  mcp` (R204). Superseded by 4.0 (#1173).
- **01M58ASFT4VBG13X5TXPHS67AQ** The status line of a lead session
  starts with the build of riff and the repository, as `OWNER/REPO`:
  `riff 3.2.0 como-technologies/riff 2a880834 lead issue-78`. The build
  is the build of the `riff mcp` of the session, else of this riff. The
  repository is the repository of the working folder. Both are local
  facts: the kept answer (01M53TZ0RBSJQ9ZZRXKMBM5R3E) does not hold
  them. The line of each other session does not change.
  Superseded by 4.0 (#1173).
- **01M3JFFJEW8BSRBZ9JQPKT0S8Z** `riff connect claude` adds the riff
  status line to the user settings of Claude Code
  (`$CLAUDE_CONFIG_DIR/settings.json` or `~/.claude/settings.json`)
  when they have no `statusLine`. It keeps each other key, its place
  and its format, and writes the file only when it changes. When
  another `statusLine` is set, or the settings are not a JSON object,
  it changes nothing and names the manual how-to.
  Superseded by 4.0 (#1173).
- **01M3Q53RQGXMYVYGCQQMWA9380** When the project lacks a riff
  permission rule, the start hook of the lead tells the lead to ask
  its user to run `riff setup`. Superseded by 4.0 (#1173).
- **R204** `riff mcp` sends a keep-alive to `riff-server` each 60
  seconds while it runs, also while no turn runs.
  Superseded by 4.0 (#1173).
- **R205** When `riff mcp` stops (its stdin closes, or it gets SIGTERM,
  SIGINT or SIGHUP), it sends an end call for its session. The
  `SessionEnd` hook `riff hook session-end` sends the same call, except
  for the reason `clear` (R168). Superseded by 4.0 (#1173).
- **01M3WG240PNMQYZ7TX6Z7ZF6M9** An open watch stream is no sign of
  life. The start of a watch is a call. The close of a watch stream is
  no sign of life. `riff watch` sends a keep-alive each 60 seconds
  while it runs. So a session whose process is killed with no end call
  is gone after 3 minutes, and its claims are free after 5 minutes,
  also when a front end holds its watch stream open.
  Superseded by 4.0 (#1173).
- **01M3W8AYDFPZNZ898WAJS7JEZA** `riff mcp` sets the step of the lead
  from each `tell`, `post`, `pause`, `resume` and `lead` call of the
  lead that `riff-server` accepts: `told SESSION` with the short
  session ID, `posted a message: TEXT`, `posted a note: TEXT`, `asked
  for status`, `paused the repository`, `resumed the repository`,
  `paused the riff`, `resumed the riff` and `became the lead`. TEXT is
  the message in one line, cut to 80 characters with `…`. The step
  replaces the step of the lead. A `status` call of the lead replaces
  the step until the next of these calls. The step
  of a session that is not the lead does not change. The skill tells
  the lead to set its status for work that riff cannot see.
  Superseded by 4.0 (#1173).
- **01M510MG808MQT08W6VSPKM1A6** A session in a container is never an
  idle worker for the rollout of the lead: it does not count as idle,
  and it gets no request. The daemon gives it its item.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M48VDS8RKJS9HG3KSEYGBFGV** The state of a lead comes from its
  facts, not from its claims: `busy` while the hooks see a turn that
  runs, with its work; `waiting` while its block holds; `idle` when
  its turn ended and nothing waits. Superseded by 4.0 (#1173).
- **01M48VDWPDYRPEAXHR1MYDN1M7** The plugin has a `UserPromptSubmit`
  hook, `riff hook prompt`. It writes the time of the last prompt of
  the person to a file on the machine, and makes no call. `riff mcp`
  puts its age into each keep-alive. A prompt at or after a block ends
  the block, for each session. Superseded by 4.0 (#1173).
- **01M41FZNTPXQNCZ1S99HE42PYQ** The plugin has a `PreToolUse` and a
  `PostToolUse` hook, `riff hook tool` and `riff hook tool --done`.
  These hooks and the Stop hook write the newest fact of the session to
  a file on the machine, and make no call: the tool that runs, a turn
  that runs, or a turn that ended. The text of a tool is its name, and
  for Bash `Bash: ` and the description, never the command. It is one
  line of at most 80 characters. A riff tool and `riff watch` give no
  fact. `riff mcp` puts the newest fact, with its age, into each
  keep-alive. `who` gives it as `work`, and shows it as `runs TOOL for
  12m`, `works, 5s ago` or `turn ended 5m ago`.
  Superseded by 4.0 (#1173).
- **01M3MEEFC9ZQVW2KC9FNJ75MTY** A session leaves the riff with the
  `leave` tool. The plugin command `/riff:leave` tells the session to
  call it. The tool acts on its own session only. When the session
  holds a claim, the tool first commits each change of its worktree as
  a WIP commit and pushes the branch, as in a pause. On the default
  branch, or when the push fails, the tool refuses and the session
  stays. Then the tool sends the end call (R205): the session leaves
  `who` and its claims are free. Superseded by 4.0 (#1173).
- **01M3MEEFETT9A0DRWBKQTG77Z2** A session that left makes no call to
  `riff-server`, so it stays gone (R207). Each riff tool except `join`
  refuses and names `/riff:join`. `riff mcp` sends no keep-alive and no
  register. `riff watch` stops within 1 second, and says not to start it
  again. The start hook adds no context. The status line shows `(left)`
  after the short session ID. Each `riff` command that acts as the
  session refuses. Superseded by 4.0 (#1173).
- **01M3MEEFKX14QCQM0F9ZYW93PP** A session that left joins again with
  the `join` tool. The plugin command `/riff:join` tells the session to
  call it. The session registers again with the same ID, starts its
  watch and follows the start routine. It becomes the lead only as at
  a start: as the first session of its user, or on the word of its
  user. Superseded by 4.0 (#1173).
- **01M3MEEFPEYXTZ89XR28E02W7P** The words "leave the riff" and "join
  the riff" of the user of a session do the same as `/riff:leave` and
  `/riff:join`. The skill says so. Superseded by 4.0 (#1173).
- **01M3XQVJXWBC3DKAVWBPXPSGZS** A leave is a fact of one session on
  one machine. `riff-server` does not keep it. The one function of the
  client that sends each request reads the mark of the session before
  each request. With the mark, it sends nothing, and the call fails
  with a text that names `/riff:join`. So each entry of the client has
  the same check: the status line, each hook, the watch, each command,
  each tool, and the keep-alive, the rollout and the reap of
  `riff mcp`. The rollout of a session that left looks at nothing.
  Superseded by 4.0 (#1173).
- **01M3XQVJVJDX81QY38219SN96B** The mark of a leave is in the state
  directory of riff: `$XDG_STATE_HOME/riff`, else
  `~/.local/state/riff`. It is never in `$XDG_RUNTIME_DIR`, which the
  system clears at a logout. So the leave holds over a restart of the
  machine, of `riff mcp` and of `riff-server`.
  Superseded by 4.0 (#1173).

## Threads

- **01M3JPMQG9FDB719BC8MDCBNBA** The skill tells a session to wake
  only the sessions that must act. A board, "started", "done" and other
  news are notes. A verify request wakes the lead of the author's user.
  A verify result wakes the holder of the item, and the lead when no
  session holds it (01M3Z9N70J4H79VJN4ZKKH3G6S). A question or a request
  goes to one session with `tell`. Superseded by 4.0 (#1173).
- **01M3JPMQJCC3F19QAJ84EKMVKA** The skill and the start hook tell a
  session to call `read` and start the watch again in the same
  response, so that a wake costs one request. Superseded by 4.0 (#1173).
- **R187** The skill tells a session to set its status when it changes
  step, and to call `blocked` when it cannot go on with no decision of
  a person. It says what the words of a status are for.
  Superseded by 4.0 (#1173).
- **01M3NK7VM1J5DDB0PECNZ28P4E** Replaced by
  01M49Z4E8QB1QDXVCPE6MX5JX2.

## Security

- **01M3JEJW26Y1C0RHM1CENJRDZ1** The skill tells a session to talk to
  other sessions when it helps: share what it found, ask a question,
  or warn about a conflict, for example before two sessions edit the
  same files. Talk needs no lead. Only the lead sends requests.
  Superseded by 4.0 (#1173).

## Sign-in and tokens

- **01M3JN3AHMK532XMRDASD4XD5D** `riff invite EMAIL` adds a member.
  `riff remove EMAIL` removes a member and ends each sign-in of that
  person (R20). Only the owner or an admin can do either. The owner
  cannot be removed. `riff members` shows the owner, the admins, the
  members and the allowed domains to each person who signed in.
  Superseded by 3.0 (01M519Z4AEMDFGBF22RZ3HQR1R).
- **01M3JN3ANE676DT5WQ2NTG47DK** Replaced by
  01M3XA875QZ584JBGA37853PWX.
- **01M3MN157X8N9QKER1AJEPEJVX** `riff members` lists each person
  once, with the highest role: owner, then admin, then member.
  Superseded by 3.0 (01M519Z4AEMDFGBF22RZ3HQR1R).
- **01M3N7K443DGPZ8XH5WWKK6M35** Replaced by
  01M3Q5460YESBSQHTV3M15PE53.
- **01M3N7K46H5BRFJCB46P3JNAFZ** Replaced by
  01M3Q546335NBTKG5BHQ27QC93.
- **01M3N7K48XQ8XSP7R0HD535ZX3** Replaced by 01M3Q63NNC6SC03BFCG80M7B4D.
- **R116** Replaced by 01M3TFG4SJ5C96NH8W7XRXJG6Z.
- **01M3TFG4PN1DWY1FXX0SVB3H3R** Replaced by 01M3WFVAB44T8EP4QZD4KS7DRF.
- **01M3WFVAB44T8EP4QZD4KS7DRF** Replaced by 01M52JYGWD3ZJKNYE3HG5BJYTP.
- **01M3TFG4ZCWS98R7W6RYZFWZXF** Replaced by 01M3WFVADCDZM8XX590KAEMEYG.
- **01M3TFG551C76BP4TRA32P7VC3** Replaced by 01M52JYGSNTFPQ0ZP50AD2BG89.
- **R103** Replaced by 01M3WFVADCDZM8XX590KAEMEYG.
- **01M4385CEWGCP31DP5PAMPXZ97** A workers host looks at the OS
  keyring each 30 seconds. When the keyring locks, the host says one
  line and posts one note to the lead. While it stays locked, the host
  says no new line. When the keyring answers again, the host says one
  line, posts one note to the lead, and goes on with no new start.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M54510M36SZ1C3AE2C8DS585** `riff statusline` with a kept answer
  (01M53TZ0RBSJQ9ZZRXKMBM5R3E), and each `riff hook` event that does
  not talk to riff-server, make no call to the OS keyring.
  Superseded by 4.0 (#1173).

## Terminal

- **01M3JD390F49HZSKEJ3VACX0ZA** When the lead runs in tmux, `riff mcp`
  of the lead adds a pane with `riff tail` of the repository thread
  beside it. It marks the pane. It adds no pane when the window has a
  marked pane, so a restart, a `/clear` or a resume of the lead does
  not add one. Superseded by 3.0 (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M3XM68N5M5DKB86W5079X2G9** `riff mcp` reads the lead mark of its
  session for the tail pane only after its `register` ends. A session
  that is not the lead then gets no pane. Superseded by 3.0
  (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M3JD392Q5ANX0FPZ51W7B0E3** `riff workers start N` starts N
  workers in the tmux window `riff-workers`, one pane each. Each pane
  runs `claude "Join the riff."` in the main worktree, with
  `RIFF_WORKER=1`. A second start adds panes to the same window. No
  person types a key. Superseded by 3.0 (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M3JD394YFA3TQRE3E72ZER4Z** A worker starts with no Remote
  Control. The book starts the lead with `claude --remote-control`.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3JV0ZNGKDFMRR9ACT0480V9** `riff workers start` runs each worker
  with the flag settings `{"remoteControlAtStartup":false}`. So a
  worker has no Remote Control, also when the user settings turn on
  `remoteControlAtStartup`.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3MN0D429T4Q80DYBE9S9XR7** `riff workers start` runs each worker
  with the flag settings `{"awaySummaryEnabled":false}` too. So a worker
  shows no recap of Claude Code. The user settings file does not change.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3ZJ1FAF7EJXP9CSET8ZY1K3** `riff workers start` turns off each
  installed plugin with a language server in the flag settings of each
  worker: `"enabledPlugins": {"PLUGIN": false}`. A plugin has a
  language server when its marketplace entry, its `.lsp.json` or its
  `plugin.json` has `lspServers`. So a worker starts no language
  server, and keeps none for a worktree that is gone. The user settings
  file does not change.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3JD3973J7A9BG8G9EP9TVDP** Outside tmux, `riff workers start`
  says that it needs tmux, starts nothing and exits with status 1.
  Superseded by 3.0 (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M3JD399ABBWE3DJT5BVXAFH5** tmux is one terminal backend. Its
  parts are in one module behind one interface, so that a later
  backend does the same. Superseded by 3.0 (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M3JD39BASN1GNJTZXXKBCNZ9** Each pane that riff makes gets the
  riff-server URL of the command that makes it, in `RIFF_SERVER`.
  Superseded by 3.0 (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M3JPQT35BMR7XMAMMFSCDC2B** `riff workers limit N` sets the most
  workers on the machine, in the key `workers.limit`. The default is 0.
  With 0, `riff workers start` starts no worker, and names
  `riff workers limit`. Superseded by 3.0 (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M3NB5R6X5AV79DQNKKJBH5J8** `riff workers mcp` shows the MCP
  servers that each worker of the machine loads, in the key
  `workers.mcp`. The default is `["riff"]`. `riff workers mcp add NAME`
  and `riff workers mcp remove NAME` change it. `riff` always stays.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3NB5R92ZC61VW6Y45SJEAY9** `riff workers start` runs each worker
  with `--strict-mcp-config` and an `--mcp-config` file that holds only
  the servers of `workers.mcp`: `riff` as `riff mcp`, and each other
  name from the MCP config of the person. A name that the person does
  not have is left out, with a warning. Only the person can read the
  file. The command line names only the file.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3NB5RB539RGCVXEZN575ERE** The lead never changes `workers.mcp`.
  Only the person does.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3JPQT57PJCRBQYJNDVESS04** `riff workers start N` starts at most
  the limit minus the workers that run on the machine. It says how many
  it started, and when it started fewer, why. Superseded by 3.0
  (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M3JPQT79FE47518Z8DFFQYYG** `riff workers start` refuses and
  starts nothing in a worker (`RIFF_WORKER=1`), and in an agent session
  that is not the lead of its user. A person in a plain terminal can
  run it.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3JPQT9BA7JVMZPV68FY4MQ6** Each worker gets a new riff session ID
  in `RIFF_SESSION`. Its tmux pane gets the mark `@riff-session` with
  that ID. A worker is a pane with that mark, in any tmux session of
  the machine. Superseded by 3.0 (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M3JPQTBDGT54WN7FZP9CD6B5** `riff workers` lists each worker of the
  machine: its pane, its session ID, its state and the detail of the
  state (01M3QB6CJ1XCQG5B1BVR8AF3B4). Superseded by 3.0
  (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M3JPQTDFW3C7QBSZZ2M831MH** `riff workers stop` ends each worker of
  the machine, and `riff workers stop PANE` ends one. It closes the
  pane, then sends the end call of the session. The session leaves
  `riff who`, and its claims are free at once. Superseded by 3.0
  (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M402VFGAJQM1QW8B42NKMJM4** When more workers run on a machine
  than its limit, a worker that ends its item ends in place of its
  clear (01M3XV0562D3H3P22CJDBPAZBH), as `riff workers stop PANE`
  does. riff does this only while the workers are more than the limit.
  One lock file on the machine makes the count and the end one step.
  A limit of 0 ends each worker after its item. riff never stops a
  worker that holds a claim. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M402VFKXEJARG7CM60TDCMKW** When a worker ends over the limit,
  the lead gets a note: the host, the pane, the limit and the workers
  that ran. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M402VFQHC5PH39DTFV6AH60F** When more workers run than the limit,
  the heading of the machine in `riff workers` says how many end
  after their item. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3JQCCX22R4R4MN7XZPTS391** No command asks for a fresh context.
  `riff workers next` and the file `next-ID` are gone. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3JQCCZ5M9VY3RGXWJYJN9Q9** The plugin has a Stop hook,
  `riff hook stop`. In a worker (`RIFF_WORKER=1`) in tmux that did not
  leave the riff, it starts the check of the clear
  (01M3XV0562D3H3P22CJDBPAZBH) as a detached process. The hook returns
  at once, and always exits with status 0. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3XV0562D3H3P22CJDBPAZBH** riff clears the context of a worker
  by itself, when its turn ends after the release of its last claim.
  The check of the Stop hook sends one keep-alive. When the reply asks
  for the clear (01M3X9XB37TQCXWPNFZRMRGJB4), a detached process types
  `/clear` and the start prompt into the pane of the worker. With no
  ask, the check does nothing. It sends a failed keep-alive again, at
  most 5 times. So the clear does not depend on a call of the worker,
  and a session that is no worker is never cleared. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3XZCWQED9M9ZB29F730EA58** The check of the Stop hook types
  nothing when a new turn of the worker started after the Stop hook.
  It compares the prompts in the transcript of the agent with the
  count of 01M3ZS67FTAC1784GEVEDXJ837. A prompt is a line of the user
  that is not the result of a tool. The Stop hook of the new turn
  starts a new check. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3ZS67FTAC1784GEVEDXJ837** The Stop hook counts the prompts of
  the transcript before it returns, and gives the number to
  `riff hook clear`. The check counts again as the last step before
  `/clear`. It does not count before the start prompt. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M43STEE72Q9TD8ZNFS3273M3** The check of the Stop hook types
  nothing and stops no process when a new context of the worker
  started after the check: the start in `context-ID`
  (01M3ZV0TJX2H77RW6ZA3ERZT9H) is later than the start of the check.
  It looks before it stops the old context, and again as the last step
  before `/clear`. One check of a worker types at a time: it holds the
  lock file `clear-ID.lock` in the local dir from the reply to its last
  key. So a check of an old context never clears the new context, and
  a new context keeps its claims. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M43STEHMTWKJDP48M1DZQPXE** When the reply asks for the clear and
  subagents of the worker still run in the background, the check
  types a prompt in place of `/clear`: stop each of them, then end the
  turn. The check of that turn clears the context. riff types the
  prompt one time in a context: when the transcript has it already,
  the check clears the context. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3XV05AD98S415V3SWN8ZDXC** The lead clears a worker that stays in
  MustClear with `riff workers stop PANE`: the rollout starts a new
  worker with a fresh context. A person can also type `/clear` in its
  pane. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded in part by the re-cut (01M53NGVE839TRH7XB7CDPQNZE).
- **01M3JQCD16CNWN5FCQBRKHXYMP** After the fresh context, the worker
  keeps its riff session ID, its lead and its watch. It follows the
  start routine and claims its next item with no person. Superseded by
  3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3JQCD373XZWNSSQYBE561TM** The keys that clear the context and
  start the next item are in one adapter for each agent tool. Superseded
  by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3JQCD5BS2ZSGZSD3CTWGPB8** The skill tells a worker: after the
  release of its last claim, do the steps that are left for the item,
  then end the turn. riff then clears its context. The worker runs no
  command for the clear. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3Q88G1K7N2EMPBA07X069A7** riff compacts the lead at the end of
  a wave, once for each wave. The Stop hook of each session that is not
  a worker starts a detached check and returns at once. The check does
  nothing when the session is not the lead. Only one check acts at a
  time: it holds a lock from the load of the record of the wave to its
  save. A check that cannot take the lock does nothing.
  Superseded by 01M59BTTH6ZATH7R2PFC6NGHJK.
- **01M3Q88G45ERD0XJNYNB5C1RVN** riff compacts the lead only when all
  of these are true: the riff is paused; the last done wave has no open
  item, and its release is out (its release item is closed, its release
  pull request is merged, and the CI run of its tag passed); no session
  in the repository holds a claim and no pull request of a wave is
  open; the turn of the lead ended and it has no unread message; no
  input came to the lead for the quiet time and its input line is
  empty; the last message of the lead does not end with a question; and
  riff did not compact the lead for this wave already.
  Superseded by 01M59BTTH6ZATH7R2PFC6NGHJK.
- **01M56P28EH72XA0F5S1B3ZP4Q5** The last done wave of the compact is
  the wave that just ended: the wave with the highest number that has a
  closed item, with no open item in it or in a wave before it. When an
  item of a later wave closes, no wave just ended, so a later pause of
  the lead, for example for a release, gives no handoff ask and no
  compact.
  Superseded by 01M59BTTH6ZATH7R2PFC6NGHJK.
- **01M3Q88G6PM8KM5PR875PZXTRZ** Before it compacts, riff tells the
  lead, as its person, to post a handoff note to the repository thread,
  with a body that starts with `handoff: Wave N`. riff compacts only
  after the lead posted that note.
  Superseded by 01M59BTTH6ZATH7R2PFC6NGHJK.
- **01M3Q88G98MKH364WEQGT4ZE7A** In tmux, riff types `/compact` with
  instructions into the pane of the lead. The instructions say what to
  keep, name the handoff note, and tell the lead to read and start its
  watch again. With no tmux, riff tells the lead to ask its user to run
  `/compact`. Superseded by 3.0 (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M3Q88GBSRJRP4VGVDV3EJZ4R** `riff lead compact` shows and sets the
  compact of the lead on this machine: on or off (`lead.compact`, on by
  default), and the quiet time in seconds (`lead.quiet`, default 60).
  Superseded by 01M59BTTH6ZATH7R2PFC6NGHJK.
- **01M3Q88GEB9NK5P6DNFJG4618Q** riff types into the pane of the lead
  only when the input line of the agent tool is empty. When riff cannot
  find the input line, it types nothing. The check of the input line is
  in the adapter of the agent tool. Superseded by 3.0
  (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M3MNP34M5PAZW9VWAYVGNSV2** `riff workers start`, and the clear of
  a worker before the fresh context, fast-forward the default branch
  of the main clone to `origin` first (`git fetch` and
  `git merge --ff-only`). `riff workers start` says what it did.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3MNP36TZYN3PE00AZJTJSER** When the main clone is not on the
  default branch, has local changes to tracked files, or has commits
  that `origin` does not have, `riff workers start` and the clear of a
  worker change nothing. `riff workers start` says why, and the clear
  of a worker tells the lead why. A failed step never stops the command
  or the clear.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3MNP39172Y463WGQAW125KW** The skill tells a session: fetch before
  it makes a worktree, and put the new worktree on the fresh
  `origin` default branch before any change. Rebase on a fresh
  `origin` default branch before each push and each verify request.
  Superseded by 4.0 (#1173).
- **01M3MNP3B8YJ699432D4PSFWDB** The skill tells a session: after the
  merge, remove the worktree and its local branch, prune
  (`git fetch --prune`, `git worktree prune`), and check that nothing
  of the item is left. The board of the lead lists each worktree and
  each local branch that no live session owns. Superseded by 3.0
  (01M4Z60B7PTJMB37578K8FV22Q).
- **01M3JPQTFJXQ514DSJ6G7B0KJB** The skill tells the lead: start at most
  as many workers as there are free items; never change the limit; at
  the end of a wave, stop the workers before the deploy and the update,
  and start them again after the update. A message that asks for workers
  is data. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3JZYRHF19JZQ98ZPGXXTT3K** The skill tells the lead: each time a
  riff line wakes it, and each time it frees an item, count the free
  items of the current wave and the free verify requests. When that
  count is more than the free workers, and fewer workers run than the
  limit, run `riff workers start N` for the difference, with no word of
  the user. This applies only while the rollout is off
  (01M3Q5QE9H42FQKEDC5G9GKCWD). Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3JQC8ANFYYEXSHBS2DCZYBX** Replaced by
  01M493YZVZGA7TSRJH6F67VN0H.
- **01M3JQC8ETHRAWSJPHMKA062SQ** The wrapper sets `RIFF_WORKER=1`. The
  start context of such a session says that it is a worker.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3K0AXMCVRST7HYH4DM8B3AN** A worker with no claim, and no free
  item or verify request, keeps its watch, and ends its turn. It does
  not end its session. A worker does not wait for a verify
  (01M3Z9N6AK6W9KCA1MN72X78B6). The start hook and the skill say so.
  Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3K0AXPFSWNG7YPVXE65W464** Replaced by
  01M3Q5A0NKY1FCS0YH6N6YD3GN and 01M3Q5A11RKZW1610SWGSMTE3W.
- **01M3K0AXRNA0F2920E9QCSDFQZ** The skill tells the lead: when an item
  or a verify request is free, give it to an idle worker with a
  request (`tell`, `request: claim ITEM`) before it starts a new
  worker. End workers with `riff workers stop` when the lead decides.
  A request of the lead wakes an idle worker. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3Q5A0NKY1FCS0YH6N6YD3GN** The server stops idle workers. Each 5
  seconds, it looks for idle workers: a live worker that is not a lead,
  holds no claim and made no call for a time. A keep-alive is not a
  call. On each host of each user, it keeps the idle workers with the
  shortest idle time, at most the setting `per_host`. It asks each
  other idle worker that made no call for the setting `after_secs` to
  stop. It asks each worker once. A call of the worker after the ask,
  or the end of its watch at a wake, takes the ask back. So a worker
  that the lead wakes, or that claims work, goes on. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by 4.0 (01M59BRGZT6J45BWKPM9XGEPEJ).
- **01M3XAHZMN8P0PRD0Q7881TEF9** The rule for idle workers is for each
  user, host and repository. An idle worker in one repository does not
  stop the idle worker of the same user and host in another
  repository. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by 4.0 (01M59BRGZT6J45BWKPM9XGEPEJ).
- **01M3Q5A0QZTSTXHHNYCE8HFJSB** The reply to a keep-alive tells a
  worker that the server asks it to stop. `riff mcp` in a worker sends
  a keep-alive each 10 seconds. On the ask, it sends SIGTERM to the
  `riff workers run` wrapper of its worker, which the variable
  `RIFF_WORKER_WRAPPER` names. The wrapper stops `claude` and sends no
  message, so the pane closes. `riff mcp` sends the end call, so the
  session leaves `riff who`. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by 4.0 (01M59BRGZT6J45BWKPM9XGEPEJ).
- **01M3Q5A0TF9K49V8Z1ZY9NDF74** The riff keeps the settings of idle
  workers: `per_host` (default 1) and `after_secs` (default 60, at
  least 1). The server saves them. `riff workers idle` shows them.
  `riff workers idle --per-host N --after SECS` sets them. In a riff
  with sign-in, only the owner or an admin sets them. In a riff with no
  sign-in, each person can. The change goes as the person, also from
  the shell of an agent session (01M3XA87NXAP6TEMB34QJ28HVP). Superseded
  by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6). Superseded by
  riff 4.0 (01M59BRGZT6J45BWKPM9XGEPEJ).
- **01M3Q5A0WRQT4SGPSD0CQFF011** For each worker that the server asks
  to stop, the server posts a note to the repository thread of the
  worker, to the lead of its user: the short session ID, the host, the
  idle time and the setting `per_host`. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by 4.0 (01M59BRGZT6J45BWKPM9XGEPEJ).
- **01M4385Z039RCFSKWFPWZAETTX** The server posts the note of
  01M3Q5A0WRQT4SGPSD0CQFF011 only for the first ask after the last
  change of the claims of the worker. A wake that takes the ask back,
  for example a pause or a resume, does not give a second note when
  the server asks again. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by 4.0 (01M59BRGZT6J45BWKPM9XGEPEJ).
- **01M4385Z2QAMEED30JYE81SMBY** When a worker still shows life 60
  seconds after the first ask to stop, while the ask holds, the server
  posts one more note to the lead: the short session ID, the host, the
  time since the ask, why riff could not stop it, and the command
  `riff workers stop ID` to stop it. It posts this note one time for
  each first ask. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by 4.0 (01M59BRGZT6J45BWKPM9XGEPEJ).
- **01M49KT28N4B07P4G80Z74GRAH** The server does not ask an idle
  worker to stop while it has an unread direct message from the lead
  of its user that starts with `request:`. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by 4.0 (01M59BRGZT6J45BWKPM9XGEPEJ).
- **01M49KT3JXZATXMA4WNTR9BJCK** The note of 01M4385Z2QAMEED30JYE81SMBY
  names the first line of each unread request of the lead to the
  worker (01M49KT28N4B07P4G80Z74GRAH), so that the lead can give it
  to another session. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by 4.0 (01M59BRGZT6J45BWKPM9XGEPEJ).
- **01M4385Z5BN03E6HTEB5GQVZ8X** `riff watch` in a worker
  (`RIFF_WORKER=1` and `RIFF_WORKER_WRAPPER` set) sends its keep-alive
  each 10 seconds. When the reply asks the worker to stop, it prints
  one line, sends SIGTERM to the wrapper and ends. So a worker whose
  `riff mcp` ended also stops. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by 4.0 (01M59BRGZT6J45BWKPM9XGEPEJ).
- **01M3Q5A0Z5DK0YV1MWTM4AQD5Z** `riff workers stop PANE --host HOST`
  asks the workers host on HOST to stop only the worker in PANE. PANE
  can also be the session ID of the worker, or its first 4 or more
  characters, also in `riff workers stop PANE` on the same machine.
  The other workers go on. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3Q5A11RKZW1610SWGSMTE3W** The skill tells the lead: start a
  worker when you have work for it. The server stops idle workers past
  the limit. The skill does not say that workers wait idle until the
  lead ends them. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M588G6V79FPSFY6AZGW7GTBN** The skill and the book say that the
  limit of idle workers is a setting of the riff, by default 1 on each
  host, and that `riff workers idle` shows it. They name no fixed
  number as the limit.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3Q5A1483TGCQYB1Q9CCP5BP** The book has the how-to "The server
  stops idle workers", with `riff workers idle` in a `sh` block, and
  the how-to "Stop one worker on another machine". Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3N7AK8TVYV8S0WR3RP0TN8X** `riff workers host` offers the workers
  of a machine to the lead of its user in the repository. It runs in
  tmux in the main clone until Ctrl-C. It is a riff session with a
  watch that never becomes the lead. Its status is `workers host:
  limit L, …` with the pane and the short session ID of each worker.
  With no tmux, or a limit of 0, it refuses to start. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M517W4NP2WKSRCK52ZBVWBD3** The status of a workers host has at
  most 400 characters (R183) at each size. When the line with the first
  8 characters of each session ID is too long, it has the first 4. When
  that line is too long too, it lists the workers that fit and ends
  with `N more`: the count of the workers that it does not list. Each
  count of the workers of a host counts them too. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3N7AKB3KXS2XYK0309C4M18** `riff workers start N --host HOST` and
  `riff workers stop --host HOST` send the workers host on HOST a
  signed direct message: `workers start N` or `workers stop`. The host
  starts at most its own limit minus its workers, in its own tmux, or
  stops each of its workers. It replies to the sender with a note: the
  pane and the session of each new worker, the count of stopped
  workers, or the reason. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3N7AKDE7DEA6NXS9ZMECRMH** A workers host acts only on a verified
  request from the lead of its user in its repository. It replies to
  each other request with a refusal, and changes nothing. Superseded by
  3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3N7AKFPX3ZGQARSG2V64GBD** `riff workers` lists the workers of
  its machine, then each live workers host of the user on another
  machine: its limit, and each of its workers with its claims and
  status. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3N7AKHXGYQ58G61BEHS89WG** The skill tells the lead: count the
  free workers and the room of each host. Start workers where there is
  room: on each host first, on the machine of the lead last. This
  applies only while the rollout is off (01M3Q5QE9H42FQKEDC5G9GKCWD).
  Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3NBV405PVYHKTMQ5VN87FYN** Ctrl-C, SIGTERM and SIGHUP stop
  `riff workers host` at once in each state: at start, waiting for a
  wake, answering a request, setting its status, and retrying after an
  error. The stop does not wait for a step that blocks. It ends its
  session first. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3NBV4294DS3WZFEKR7M3PNF** At start, `riff workers host` prints
  one line: the host, its limit, the lead that it serves and the
  repository. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3NBV44GKAX6WS391PN6R72W** One workers host of a user runs on a
  machine for a repository. A second one refuses to start, and names
  the process and the session of the first. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3NBV46R0VB0JQNQ1ERG16J6** `riff workers host` reads no input
  and leaves the mode of the terminal as it is. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3WN72M02P3J24ACCHTMNSFY** Each call of `riff workers host` to
  the server has the budget of a short command
  (01M4A803Z4Q0KX6NT1KC6QR43H). When no reply comes in the budget, the
  host says so on its output and goes on. At its next
  refresh, it sets its status again, and it reads the requests that it
  did not read. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3Q5QE01DB0FJQJWFKR450KQ** `riff mcp` of the lead runs the
  rollout of workers. Once each interval, while the session is the lead
  and the riff runs, it looks at the free work and the idle workers.
  When there is free work and no idle worker
  (01M3Q5QEJNP1JGQM7VXXEBJ9J9), it starts one worker. So riff starts at
  most one worker each interval, with no step of an agent. An idle
  worker is a worker with no claim, also a new worker that did not
  join yet, and a live worker of another user. A worker that the
  server asked to stop is not idle. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by the re-cut (01M53NGVE839TRH7XB7CDPQNZE,
  01M5515CA4CZ6KV4ZD6Y0744Z7).
- **01M3W27BJYFQCHY5MTZ2J4SKW4** An idle worker counts for the rollout
  only when it is in the repository of the lead. A worker in another
  repository does not count, also a worker of the user of the lead. A
  worker that did not join yet counts. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3Q5QE4SQ8VYN2PSF42KB3QJ** Each machine that runs workers tells
  its CPU cores, its CPU speed, its clock now, its memory, its
  available memory and its 1-minute load average. A workers host puts
  them in its status. `riff workers` shows the numbers and the score of
  this machine and of each host. Superseded by 3.0
  (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M419XAX31FF9Z1647E881CSH** The CPU speed of a machine is the cap
  of its clock: the lowest `scaling_max_freq` of its cores. With no cap,
  it is `cpuinfo_max_freq`, then the most `cpu MHz` of `/proc/cpuinfo`,
  then 3000 MHz. The clock now is the mean of `scaling_cur_freq` of the
  cores. The score counts the cap. Superseded by 3.0
  (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M419XAZBPV0Y08CAR51KQSZS** The lead reads the numbers of a
  workers host of the release before, with no clock now. The clock now
  is then the CPU speed. Superseded by 3.0 (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M3Q5QE76BZ27SZ14FFE8HM1G** The score of a machine is
  `min(cores, memory GB / 2) × MHz / 3000`. Its free capacity is the
  score less its workers. The rollout starts a worker on the machine
  with the most free capacity: the machine of the lead when it runs in
  tmux, or a live workers host of the user. It skips a machine at its
  limit, and a machine whose load average is more than its cores. On a
  tie, the machine of the lead wins. A host with no numbers counts its
  limit as its score. Superseded by 3.0 (01M4Z60B7Y4XD6TJCYEYJT2R4S).
  Superseded in part by the re-cut (01M53NGVE839TRH7XB7CDPQNZE).
- **01M3Q5QE9H42FQKEDC5G9GKCWD** The interval of the rollout is the
  setting `workers.interval` of the machine of the lead, in seconds.
  The default is 10. 0 turns the rollout off. `riff workers interval`
  shows it and sets it. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3Q5QEBTNM90SPYXNVTT7RJA** A pause stops the rollout within one
  look. A look that started before the pause can start one more
  worker. After it, the rollout starts no worker while the riff or the
  repository of the lead is paused. The resume starts the rollout
  again. A pause of another repository does not stop the rollout.
  Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded in part by the re-cut (01M5515CA4CZ6KV4ZD6Y0744Z7).
- **01M3Q5QEE4MQNCRKVJK3D54G9Z** Each start of the rollout gives the
  lead a note with the host, the pane and the session of the new
  worker. A note wakes nobody. On the machine of the lead, the person
  posts it. On a host, the host posts it as its reply. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by the re-cut (01M5515CA4CZ6KV4ZD6Y0744Z7).
- **01M3Q5QEJNP1JGQM7VXXEBJ9J9** The rollout starts a worker only when
  no worker is idle: each worker with no claim, also a new one that did
  not claim yet, counts. So when no worker takes the counted work, one
  worker waits idle, the server keeps it, and riff starts no more
  workers. A worker that refused its request does not count
  (01M49ZK1C1P817KE63EXV3EJDC). Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded in part by the re-cut (01M5515CA4CZ6KV4ZD6Y0744Z7).
- **01M518P1N8R893MG9Z3VAQZR6Z** A request of the lead with no work
  item names a task claim `task-NAME`, for example
  `request: claim task-iroh-listener, then ...`. The worker claims it
  before it starts the task, and releases it when the task ends. When a
  request of the lead names no claim, the worker claims a `task-NAME`
  of its own first. A task claim is a claim: the rollout sends its
  worker no request and does not count it as idle
  (01M3Q5QEJNP1JGQM7VXXEBJ9J9), and the server does not ask its worker
  to stop as idle. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M49ZK19GQP79Z8HH14PK85QQ** At each look of a running riff, the
  rollout gives free work to each idle worker of the user of the lead
  that joined the riff, before it decides on a start. The worker is
  live, in the repository of the lead, has no claim, and does not wait
  for a stop or a clear. It gets one direct message of the lead:
  `request: claim verify-issue-N` for a pull request that waits for a
  verify, or `request: claim issue-N` for a free item. The verifies
  come first. A worker has one open request at a time, and an item goes
  to one worker at a time. A worker in the worktree `issue-N` gets no
  request for `verify-issue-N`. The lead can still give work by hand.
  Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M49ZK1C1P817KE63EXV3EJDC** The rollout never sends the same
  request to the same worker again. A request is open until its worker
  claims or goes, or its item is no longer free. A worker that does not
  claim in 6 intervals of the rollout refused the item. Then it can get
  a request for another free item. When it has no item to take, it
  does not count as idle for the start of a worker. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M49ZK1EG52YAKG974XH201RK** An item that two workers refused is no
  free work for the start of a worker. So the rollout starts at most one
  more worker for a refused item, and no loop of starts and stops.
  Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by the re-cut (01M5515CA4CZ6KV4ZD6Y0744Z7).
- **01M51533BJSD0RB7BFQR9CAFYW** The rollout never sends
  `request: claim verify-issue-N` to a session that held `issue-N`,
  also after a `/clear` of that session. The author is each session
  that a look of the rollout saw with the claim, and each session of a
  usage comment of the work claim on the issue N. When the author is
  the only idle worker, it has no item to take, and the rollout can
  start a new worker for the verify. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M51A55Q2MDZT38JDCEJ1QECP** A held item is no free work for the
  rollout: the rollout starts no worker for it and sends no request for
  it. A hold names one item, so a hold of `issue-N` does not stop
  `verify-issue-N`. Each look reads the holds of the repository thread
  again, so a free of the item makes it free work at the next look.
  Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded in part by the re-cut (01M53NGVE839TRH7XB7CDPQNZE).
- **01M3X30KHKB6W11C3NBAW7KCGW** The lead gets one message for each
  change of a worker setting: the setting, the old value, the new
  value and the host. The settings are the limit of the machine of the
  lead and of each live workers host, the interval and the MCP servers
  of the machine of the lead, and the idle settings of the server.
  `riff mcp` of the lead reads them at each look of the rollout, also
  while the rollout is off. The first look gives no message. Superseded
  by 3.0 (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M3X30R4PSBP3RQWM02BJ6GK3** The message for a new limit says what
  the change does. When the rollout starts a worker because of a
  higher limit, it says so. When more workers run than a lower limit,
  it says how many run and how many end after their item
  (01M402VFGAJQM1QW8B42NKMJM4). Superseded by 3.0
  (01M4Z60B7Y4XD6TJCYEYJT2R4S).
  Superseded in part by the re-cut (01M5515CCV7WNKVZVAMQ375J0M).
- **01M3X30RA3X08JBJ2JBVCCNEH3** The message for a change of a worker
  setting is a note. It wakes the lead only when free work waits that
  a higher limit lets start and the rollout is off. Then it names the
  `riff workers start` command. Superseded by 3.0
  (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M3X30RJS8YE5TXJBQDC2FT0C** A workers host reads the worker
  settings of its machine each 5 seconds. It sets its status at once
  when its limit or its floor changes. It posts a note to the lead
  when its MCP servers change. Superseded by 3.0
  (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M3XFHSYJEN9V6QEKWJGJWQ8Q** One look of the rollout reads each
  worker limit one time. It uses that value for the changes and for
  the start of a worker. It tells the lead each change first, and then
  it starts the worker. A limit that changes in the middle of a look
  has no effect before the next look. Superseded by 3.0
  (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M3WFYZP9Y4N41QGH5SWKFZZC** The skill tells each session: to run
  a test many times, run that test by its name in a loop, not the full
  `just ci`. In a worker, the pool of build jobs shares the cores: do
  not change the variables that riff sets. Superseded by 3.0
  (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3WFYZRK5CT22GJW6ZHYT9CC** The fixed share of a worker is the
  physical cores of the machine less 1, divided by the workers, and 1
  or more. The workers are the worker limit, or the workers that run on
  the machine when they are more. With no pool, each worker gets it in
  `RUST_TEST_THREADS` and `CARGO_BUILD_JOBS`. The setting
  `workers.jobs` replaces the number and turns the pool off. 0, the
  default, means the number from the machine and the pool. Superseded by
  3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZGZMJ9RF1C4AHG78GQ2NM4** Each `riff workers run` holds one pool
  of build jobs for its machine: a named pipe in the GNU make 4.4
  jobserver form. It holds the hardware threads (the logical CPUs) less
  2, less the workers, and 1 or more tokens. The first worker makes it.
  The pool ends with the last worker. The worker gets `MAKEFLAGS` with
  `--jobserver-auth=fifo:PATH`, and no `CARGO_BUILD_JOBS` and no
  `RUST_TEST_THREADS`. Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZZGRB5NDAA419ZNEWN0811** When riff cannot read the physical
  cores of a machine, it counts half of the logical CPUs, and 1 or
  more. `riff workers start` says so one time. `riff workers jobs` says
  so each time. Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZZGRFYH0KSYMM71EK3TT6T** The pool keeps the number of workers
  that its size counts. While more workers run, each worker after that
  number, in the order of their start, keeps one token out of the pool.
  It gives the token back when it is within the number again.
  `riff workers jobs` shows the workers that run over the number.
  Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZGZMNH1YM56GYNYBMH7AWM** With a pool, each worker gets
  `riff workers test-run` as its cargo test runner in
  `CARGO_TARGET_<TRIPLE>_RUNNER`. For a program in a `deps` directory,
  it waits for one token, then takes each free token, at most the pool.
  It runs the program with one test thread for each token, in
  `RUST_TEST_THREADS`. When `RUST_TEST_THREADS` is set, it takes that
  number of tokens and does not change it. It gives the tokens back
  when the program ends, also when a signal kills it. One runner at a
  time collects tokens. A runner waits at most 10 minutes, then runs
  with the tokens that it has. Superseded by 3.0
  (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M515PDCD1RRBKJPZHKKZWMY3** The `TRIPLE` of the test runner of a
  worker is the `host` of `rustc -vV`, or of `$RUSTC -vV` when `RUSTC`
  is set, not the target of the riff build. When riff cannot run it or
  reads no `host`, the triple is the target of the riff build.
  Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M49XNPMXD3SF6JHBYV9DN59M** The first worker of a machine reads
  the memory pressure of the machine (`some avg10` of
  `/proc/pressure/memory`) each 5 seconds. While it is above 10 %, the
  worker keeps each free token out of the pool, also each token that
  comes back. When it is 10 % or less, or riff cannot read it, the
  worker gives the tokens back. `riff workers jobs` shows the pressure
  and the limit. Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZGZMRHXRBP762QPVCV0YX8** When riff cannot make the pool, each
  worker gets the fixed share. `riff workers start` says so one time.
  Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M41CR2HJRFW6R7YMJTPVEMJ1** With no pool, the wrapper of a worker
  unsets `MAKEFLAGS`, `CARGO_MAKEFLAGS` and the cargo test runner. So
  `claude` gets no variable of the pool of another worker. Superseded by
  3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZGZMV78G3BNVFGHAZWQQDX** `riff workers jobs` shows the size of
  the pool and the tokens in use, or that no worker runs. Superseded by
  3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M517W4JKFA0E2YJC5BQQ3ZWS** A read of the pool (its state, or a
  take of tokens) looks for a holder under the lock that keeps the
  start and the end of a hold in order. So a read while the first
  worker of a machine makes the pool never makes that worker join the
  old pipe, which has no tokens. Superseded by 3.0
  (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3WFYZTX05CGDP2NQF9B356K** `riff workers run` starts its worker
  with a nice value: the setting `workers.nice`, 0 to 19. The default
  is 10. 0 means no nice. `riff workers nice` shows it and sets it.
  Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3WFYZX6GVFYW6NTTTKF144R** On a machine with systemd, the workers
  of the machine run in the slice `riff-workers.slice` of the systemd
  user manager, each in a scope of its own. `riff workers start` gives
  the slice `MemoryMax`, `MemoryHigh` at nine tenths of it, and
  `CPUWeight=50`, until the next start of the machine. `MemoryMax` is
  the setting `workers.memory` in GB. 0, the default, means three
  quarters of the memory of the machine. `riff workers memory` shows it
  and sets it. `riff workers run` stays outside the slice. Superseded by
  3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3WFYZZENNHVH8Z2BAFSR6TS** On a machine where `systemctl --user`
  cannot set the slice, `riff workers start` says one time that the
  workers run with no memory limit, and starts them with no scope.
  Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M407J8R79WVYVABVCSHFAMJ9** The nice value of `workers.nice` is
  absolute: `claude` of a worker runs at that value, also when
  `riff workers run` runs at a nice value of its own. When the wrapper
  runs at a higher value, `claude` keeps the value of the wrapper, and
  the wrapper says so in its pane. Superseded by 3.0
  (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M407J8X25H9AT8M789EG5RQZ** When `systemd-run --user --scope`
  fails in the pane of a worker, `riff workers run` starts `claude`
  with no scope. It says so one time on the machine, in the pane.
  Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M407J917F9AH072C8DE80CRJ** The lead reads the status line of a
  workers host of the release before: a line with no floor and no
  available memory. riff then counts the default floor and all of the
  memory as available. Superseded by 3.0 (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M3WFZ01PTAYYKG3T5CFA2W4D** `riff workers start` and the rollout
  start no new worker on a machine while its available memory is less
  than the setting `workers.floor` in GB. The default is 4. 0 turns the
  floor off. `riff workers start` says why. `riff workers` shows why,
  for this machine and for each host. A workers host puts its floor in
  its status. `riff workers floor` shows it and sets it. Superseded by
  3.0 (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M3WFZ03Z9Y60HPHJJ9ZE6AQZ** When a signal ends the `claude` of a
  worker, the message of the wrapper to the lead names the signal, says
  that a kill ended the worker, for example for the memory of the
  workers, and says that its work that is not committed is in its
  worktree.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3WFZ0676C5HJDXCGVZ715K2** The skill tells the lead: never change
  `workers.jobs`, `workers.nice`, `workers.memory` or `workers.floor`.
  Only the user sets them. riff does not watch the memory, and does not
  change a limit while the workers run. Superseded by 3.0
  (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3WFZ08D8VT9KD6HXY09NHSE** The test environment gives each run of
  `riff` the numbers of a machine with free memory and no load, in
  `RIFF_MACHINE`. So no test depends on the load or the memory of the
  machine that runs it. Superseded by 3.0 (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M3Q5QEGBD5JB4ZZWNVVS09KV** The skill tells the lead: riff starts
  workers by itself. The lead gives free work to an idle worker with a
  request. It starts workers by hand only while the rollout is off. It
  never changes `workers.interval`. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded by the re-cut (01M53NGVE839TRH7XB7CDPQNZE).
- **01M3WG2460P4GF7GEVBY92Q33W** A worker can die at each moment.
  `riff workers host`, and `riff mcp` of the lead on the machine of the
  lead, look at the worker panes of the machine each 5 seconds. When a
  pane is gone and its session is live, riff ends the session, so its
  claims are free at once. It posts one note to the lead with the pane,
  the session, the items and the cause when it finds it: a kill by
  `systemd-oomd` of the scope of the pane. The note wakes nobody. The
  rollout starts a worker for the free item. riff acts at the second
  look after the end of a pane, so `riff workers stop`, which kills
  the pane and then sends the end call, gives no note. riff ends only
  a session of its user in its repository, and posts no note for
  another one. Superseded by 3.0 (01M4Z60B81RTZT71PN5HV0GZHX).
- **01M493YZRPS33RR47Q6T9V6WP9** riff keeps the workers of each machine
  as the person set them. The worker settings of a machine are the
  wanted state: `workers.limit`, `workers.jobs`, `workers.nice`,
  `workers.floor` and `workers.mcp`. The rollout of the lead starts a
  worker for free work while a machine has room
  (01M3Q5QE01DB0FJQJWFKR450KQ). A worker over the limit ends after its
  item (01M402VFGAJQM1QW8B42NKMJM4). A worker that dies frees its
  claims, and the rollout starts a new worker for the free work
  (01M3WG2460P4GF7GEVBY92Q33W). Each such action is one line on the
  output of the process that acts and one note to the lead. The lead
  starts and stops no worker by hand for this. Superseded by 3.0
  (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded in part by the re-cut (01M53NGVE839TRH7XB7CDPQNZE).
- **01M493YZVZGA7TSRJH6F67VN0H** Each worker pane runs `claude` through
  `riff workers run`. When `claude` exits on its own, the wrapper posts
  a note to the lead of the person in the repository, as the person:
  the pane, the session ID and the exit code. It never starts `claude`
  again: the rollout starts a new worker for the free work. On SIGTERM
  or SIGHUP, it stops `claude` and posts no note. Superseded by 3.0
  (01M4Z60B81RTZT71PN5HV0GZHX).
  Superseded in part by the re-cut (01M53NGVE839TRH7XB7CDPQNZE).
- **01M493YZZEW1FTDBNA090WT2AG** A machine records each death of its
  workers, one time for each session, in the file `worker-deaths` of
  its local riff dir: an exit of `claude` on its own with an exit code
  that is not 0 or with a signal, and a pane that ends with no end
  call. When more than 3 workers of a machine died in the last hour,
  the rollout starts no worker on that machine. It starts workers there
  again when 3 or fewer died in the last hour. A workers host tells its
  deaths of the last hour in its status, after its floor, as
  `deaths N`, when N is more than 0. Superseded by 3.0
  (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M493Z02KS82B3CVZEVFA3D6E** The death that makes more than 3
  deaths in the last hour on a machine sends one message to the lead,
  which wakes it: the count, the host, that riff starts no worker
  there, and where to look for the cause. A later death in the same
  loop sends no message. Superseded by 3.0 (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M493Z063SSTAJS2KNDFBTEBJ** `riff workers` shows for each machine
  the wanted state and the running state: the limit and the workers
  that run. After them it says each difference: room for more workers,
  workers that end after their item, and why the machine starts no
  worker: low memory, or more than 3 deaths in the last hour. Superseded
  by 3.0 (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M493Z6WAKE05A3RGPQR223ZE** The skill tells the lead: a note
  `worker stopped` needs no step, because riff starts a new worker for
  the free work. The message of a loop of deaths goes to the user of
  the lead. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
  Superseded in part by the re-cut (01M53NGVE839TRH7XB7CDPQNZE).
- **01M3ZV0QSFVCHRSEKYK57B88VA** riff stops the processes of a worker
  itself, in code. The agent never names a process ID. With no scope
  of the worker (01M49SVFW0FZ3DK57PACS7W5EY), a process is of the
  worker `ID` when its environment has `RIFF_WORKER=1` and
  `RIFF_SESSION=ID`. A process is of a context when it also has the
  variable that the agent tool gives to each command and hook, and not
  to itself or its MCP servers: `CLAUDE_PID` for Claude Code.
  `riff workers run` starts the agent tool with no such variable. riff
  never stops `riff watch`, the process that calls it, or a parent of
  one. It sends SIGTERM, then SIGKILL after 3 seconds. It sends no
  signal to a process ID that a new process took. Superseded by 3.0
  (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M49SV9W4S1HJ4BYANA388VD2** `riff workers run` runs the agent
  tool of the worker `ID` in the systemd scope
  `riff-worker-ID.PID.scope`, where PID is the wrapper. In `ID`, each
  character but a letter, a digit, `_` and `-` becomes `_`. Superseded
  by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M49SV9Z2A7TXWFTMVNYXSQNM** When a process is in a scope
  `riff-worker-ID.N.scope`, the clear, the reap and the stop of the
  worker `ID` select by the scope: a process is of the worker only
  when its cgroup is such a scope. Its environment does not count.
  Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M49SVFW0FZ3DK57PACS7W5EY** When no process is in a scope of the
  worker, for example on a machine with no systemd, riff selects by
  the environment (01M3ZV0QSFVCHRSEKYK57B88VA). It says so one time
  on the machine, until a selection by a scope works again. Superseded
  by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M438620PJHSVSPAENBKKJ6C2** A process is of a worker of this
  riff only when its `RIFF_HOME` is the `RIFF_HOME` of the riff that
  looks, or both have none (01M3ZV0QSFVCHRSEKYK57B88VA). riff never
  stops a process of a riff of another home, also with the same
  session ID. Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZV0TJDQ6JCM7XG0036MSV1** Just before the clear of a worker
  (01M3XV0562D3H3P22CJDBPAZBH), riff stops each process of the old
  context of the worker, for example a `just ci` in the background.
  When it stopped one, it posts a note to the lead with the pane and
  each process. Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZV0TJX2H77RW6ZA3ERZT9H** The start hook of a worker writes the
  start of the new context to the file `context-ID` in the local dir:
  the boot ID and the start of the hook process. A file of an earlier
  boot gives no start. Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZV0TKBP201FKY32ZD81G4E** `riff workers reap` stops the orphan
  processes of each worker of the machine, and `riff workers reap
  PANE` of one: each process of a context that started before the
  start of the current context (01M3ZV0TJX2H77RW6ZA3ERZT9H). It prints
  one line for each process that it stopped, and one line for a worker
  with none. With no start of the context, it stops nothing and says
  so. Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZV0TMNQDK9WC3BR1NPGAC2** `riff workers stop` stops each process
  of the worker after it closes the pane (01M3JPQTDFW3C7QBSZZ2M831MH),
  and prints them. Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZV0TKSHNW5QC2NG1XTJEJB** `riff worktrees clean` decides by
  facts for each worktree in `.claude/worktrees` of the main worktree.
  It unlocks a lock whose process is gone or has another start. It
  keeps a worktree with a lock of a live process, a lock with no
  process ID, or a live session of `riff who` in it, the worktree of
  an item that a live session holds (01M515233TRCW2PK3V4QBR4V17), a
  worktree that a running process uses (01M5152361VDRV7NZ2N2Z7BVNP),
  and the worktree of the caller. It commits the work of a worktree
  with no live owner as WIP, pushes its branch, and posts a note to the
  lead. It removes a clean worktree whose pull request is merged with
  its `HEAD`, and its branch while the branch points at that `HEAD`. It
  removes a clean worktree whose pull request merged with another head
  when each of its commits is in the merge
  (01M51JCQ7V7SHTY1ANF0WSTBDF). It removes a clean detached worktree
  whose commit is on a branch of `origin`. It keeps each other
  worktree. It prints one line for each worktree with what it did and
  why. It keeps each worktree outside `.claude/worktrees`. Superseded
  by 3.0 (01M4Z60B7PTJMB37578K8FV22Q).
- **01M41XFFXEQPEPDVM4HNT69FVP** `riff worktrees clean` removes a clean
  worktree with no live owner whose `HEAD` is on the default branch of
  `origin`, with or with no pull request, and its branch. It also
  removes a clean worktree with no live owner whose `HEAD` is the head
  of a merged pull request that it finds by the commit, also when the
  branch is gone. Superseded by 3.0 (01M4Z60B7PTJMB37578K8FV22Q).
- **01M515233TRCW2PK3V4QBR4V17** `riff worktrees clean` and the
  removal of a `target` (01M41A11BB4HAD8595DNSBAZ0D) keep the worktree
  of an item that a live session of `riff who` holds, on the same
  host and in the same repository, also when that session works in
  another worktree. The item of a worktree is its name, for example
  `issue-12`. The verify worktree `verify-issue-12-a6cf` is of the
  claim `verify-issue-12` of the session whose ID starts with `a6cf`.
  The line says `kept: a live session holds its item ITEM`. Superseded
  by 3.0 (01M4Z60B7PTJMB37578K8FV22Q).
- **01M5152361VDRV7NZ2N2Z7BVNP** `riff worktrees clean` and the
  removal of a `target` (01M41A11BB4HAD8595DNSBAZ0D) keep a worktree
  that a running process uses: its program (`/proc/PID/exe`) or its
  working directory (`/proc/PID/cwd`) is in the worktree. A program
  that a new build deleted still counts. riff reads each process that
  it can read: the processes of its user. The line says `kept: the
  running process PID (NAME) uses it`. Superseded by 3.0
  (01M4Z60B7PTJMB37578K8FV22Q).
- **01M51JCQ7V7SHTY1ANF0WSTBDF** `riff worktrees clean` removes a clean
  worktree with no live owner whose pull request merged with another
  head, for example after a rebase on another machine, when each of its
  commits is in the merge. It also removes its branch while the branch
  points at `HEAD`. A commit is in the merge when the pull request has
  it or a commit with the same patch, or when the default branch of
  `origin` has it. When the clone does not have the head of the pull
  request, riff fetches `refs/pull/N/head` of `origin`, with no ref.
  riff keeps a worktree with a commit that is not in the merge. The
  line names the commits, at most 3: `kept: its commit 1a2b3c4 is not
  in the merged pull request #N`. Superseded by 3.0
  (01M4Z60B7PTJMB37578K8FV22Q).
- **01M3ZV0TM7ANJ1QQ7XTBDJQE1V** `riff workers start` and the start of
  `riff workers host` run `riff worktrees clean`. Superseded by 3.0
  (01M4Z60B7PTJMB37578K8FV22Q).
- **01M41A118QPQKFAAHGQFFX4F3B** A workers host and the `riff mcp` of
  the lead run `riff worktrees clean` on the clone of their machine
  each 10 minutes. The `riff mcp` does it only while its session is
  the lead. A merge starts no clean of its own: the 10 minutes bound
  the time to the removal. Superseded by 3.0
  (01M4Z60B7PTJMB37578K8FV22Q).
- **01M41A11BB4HAD8595DNSBAZ0D** At each such clean, when less than
  15% of the disk of the main clone is free, riff removes the `target`
  directory of each worktree in `.claude/worktrees` with no live
  owner: no lock of a live process, no lock with no process ID, no
  live session of `riff who` in it, no live session that holds its
  item (01M515233TRCW2PK3V4QBR4V17), no running process that uses it
  (01M5152361VDRV7NZ2N2Z7BVNP), and not the worktree of the caller. It
  keeps the source and the branch. It posts one note to the lead with
  each `target` that it removed. Superseded by 3.0
  (01M4Z60B7PTJMB37578K8FV22Q).
- **01M41A11DX1QRP48YPTDNT67W4** When less than 5% of the disk of the
  main clone is free, `riff workers start`, the rollout and a workers
  host start no worker on the machine, and say why. The lead gets one
  note when the disk of a machine goes under 5%, not one at each
  clean. Superseded by 3.0 (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M41A11GHP78E2VYN14JSE27P** `riff workers` shows the free disk of
  each machine under its line. A workers host tells its disk in its
  status, after the numbers of its machine. The lead reads a status
  with no disk as a host that does not tell it. Superseded by 3.0
  (01M4Z60B7Y4XD6TJCYEYJT2R4S).
- **01M41VAGJC69S9R2TD1B1EQ4W4** `riff workers run` gives `claude` a
  temp folder of its own on disk: `ROOT/SESSION`, in `TMPDIR` and
  `CLAUDE_CODE_TMPDIR`. It also puts both in the `env` of the flag
  settings of `claude`, so that the user settings do not replace them.
  ROOT is `workers.tmp`, else `~/.cache/riff/tmp`.
  `riff workers tmp` shows and sets it. Superseded by 3.0
  (01M4Z60B7TQV799Z4J5KAW2XMZ).
- **01M41VAGMQPBDPV8XPGEKYRXZZ** A process uses a folder when its
  working directory or an open file is in it, or when its `TMPDIR` or
  `CLAUDE_CODE_TMPDIR` is in it. riff never deletes a temp folder that
  a live process uses. Superseded by 3.0 (01M4Z60B7TQV799Z4J5KAW2XMZ).
- **01M41VAGQ2VA2Q0VSFJNG4H08W** The wrapper deletes the temp folder
  of its worker when `claude` ended. `riff workers stop` deletes it
  after it stopped the processes of the worker. Superseded by 3.0
  (01M4Z60B7TQV799Z4J5KAW2XMZ).
- **01M41VAGSCNESHTZ6216P2E133** At the clear, after the stop of the
  old context, riff deletes each file and folder of the temp folder of
  the worker that no process holds open or works in. It keeps each
  folder `tasks`. Superseded by 3.0 (01M4Z60B7TQV799Z4J5KAW2XMZ).
- **01M41VAGVR2PPVAYDN0SWK2F02** Each tidy deletes each temp folder in
  ROOT that no process uses and that did not change in the last
  minute. Superseded by 3.0 (01M4Z60B7TQV799Z4J5KAW2XMZ).
- **01M41VAGY396K07BTPSW9TNBX5** `riff workers` shows the disk use of
  ROOT of this machine under the line of its disk. Superseded by 3.0
  (01M4Z60B7TQV799Z4J5KAW2XMZ).
- **01M492379BGA3AERT1AM12C650** The workers of a machine share one
  compile cache. `riff workers run` gives `claude` the variables
  `RUSTC_WRAPPER` (the `sccache` of the machine), `SCCACHE_DIR`,
  `SCCACHE_CACHE_SIZE`, `SCCACHE_SERVER_PORT` and
  `SCCACHE_IGNORE_SERVER_IO_ERROR=1`. With no `sccache`, it unsets the
  five variables and says one line. The cache is on the
  machine only: no cloud store, no network. Superseded by 3.0
  (01M4Z60B7RPSQ1ABGVMJSZWS5P).
- **01M49AB2QYGJ73Y19KGAY1WDW7** `riff workers run` starts the
  `sccache` server of the machine before it starts `claude`, with no
  variable of a worker and in its own process group. Superseded by 3.0
  (01M4Z60B7RPSQ1ABGVMJSZWS5P).
- **01M49SVFZS7HTYM3FSCV4ZCS0Q** The `sccache` server of the machine
  runs outside the scope of each worker. When the worker has a scope,
  `riff workers run` starts the server in the systemd scope
  `riff-sccache-PORT.scope`. The server has
  `SCCACHE_IDLE_TIMEOUT=0`, so it never ends for idle time, and a
  build of a worker does not start it again in the scope of that
  worker. Superseded by 3.0 (01M4Z60B7RPSQ1ABGVMJSZWS5P).
- **01M49AB2TBMHGNXM3GE4NDFYYG** A process with
  `SCCACHE_START_SERVER=1` whose binary (`/proc/PID/exe`, by device and
  inode) is the `sccache` of the machine is the `sccache` server of the
  machine. riff never trusts the name in `argv[0]`. No clear, reap or
  stop of a worker stops the server. Each other process with the
  variable stays a process of its worker. Superseded by 3.0
  (01M4Z60B7RPSQ1ABGVMJSZWS5P).
- **01M49237BM12PVBERD6JXDSX5V** The folder of the cache is
  `$RIFF_HOME/sccache`, else `~/.cache/riff/sccache`. Its most size is
  `workers.cache`, default `40G`. `riff workers cache` shows and sets
  it. Superseded by 3.0 (01M4Z60B7RPSQ1ABGVMJSZWS5P).
- **01M49237DWTBSEM6CVFH2BYC4V** The port of the `sccache` server
  comes from the folder of the cache: 20000 to 29999. So the workers of
  a machine share one server, and it never mixes with another
  `sccache` server. Superseded by 3.0 (01M4Z60B7RPSQ1ABGVMJSZWS5P).
- **01M4923963S666V9YWTZ46ZZ50** The start of `riff workers host`, and
  `riff update` on a machine with a limit of workers more than 0, run
  `cargo install --locked sccache --version` of the version that riff
  pins, when `sccache` is missing or older. The host installs while it
  serves. A failed install is one note to the lead from a host, and one
  line from `riff update`. The workers then build with no cache.
  Superseded by 3.0 (01M4Z60B7RPSQ1ABGVMJSZWS5P).
- **01M492398HA0AXX0J8BZCKNGTG** `riff workers` shows the compile cache
  of this machine under its temp line: the size, the most size and the
  hit rate, from `sccache --show-stats`. Superseded by 3.0
  (01M4Z60B7RPSQ1ABGVMJSZWS5P).
- **01M49239AWKEPKEPMZZRRTVRAT** `riff workers cache --clear` stops the
  `sccache` server of the cache and deletes the folder of the cache. It
  refuses while workers run on the machine. Superseded by 3.0
  (01M4Z60B7RPSQ1ABGVMJSZWS5P).
- **01M421QPKWPX00X24F8V6DT8Z3** The monitor of a machine reads its
  health each `monitor.every` seconds (default 15) while `monitor.on`
  is true (default false): the 1-minute and 5-minute load average, the
  available memory, and each kill of `systemd-oomd` or of the kernel.
  A workers host runs it, and the `riff mcp` of the lead runs it while
  its session is the lead. `riff workers monitor` shows and sets the
  settings.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M421QPP5QFBB0YN25HY2MG1Z** The monitor sends the lead one
  message when the 5-minute load goes over `monitor.load` (default
  1.5) times the physical cores, or the available memory goes under
  `workers.floor`, and one message when the number is good again. It
  sends one message for each kill. It sends no message while nothing
  changes. A message names the machine, the number and the limit.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M421QPRF45DQDA8S4PT1Q12V** The monitor only reads and tells. It
  changes no setting and stops no worker.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M421QPTQ8BQ0KMG8F7CRHNMX** `riff workers monitor on --host HOST`
  and `riff workers monitor off --host HOST` ask the workers host on
  HOST to set `monitor.on` on its machine. The host replies with a
  note.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M421QPX01BB15GJXHFYRETTX** `riff workers` shows for each machine
  whether the monitor is on, and its last numbers. A workers host
  tells them in its status: the 5-minute load, its limit, the physical
  cores, the pool or the jobs (01M515PDFZZXQ9ZCQE7SQ6KB8G) and the last
  kill.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M515PDFZZXQ9ZCQE7SQ6KB8G** The monitor numbers show the pool of
  build jobs as `pool SIZE (USED in use)`: when a worker holds the
  pool, its size and the tokens that are not in the pool; when no
  worker runs and the settings give a pool, the size of the pool of
  the next worker and 0. Else they show the fixed share of each worker
  as `jobs N`. `riff top` shows `pool SIZE` with no tokens in use.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M421QPZ9E01PQ62PDBH378SJ** `riff top` shows one line under each
  host that runs workers: the load average of 1 and 5 minutes against
  the physical cores, the cap of the clock and the clock now, the
  available memory against the floor, the workers against the limit,
  and the pool or the jobs (01M515PDFZZXQ9ZCQE7SQ6KB8G). A number over
  its limit has the warning style. The line fits in 80 columns. A
  second line shows the time and the cause of the last kill. The
  sources of the numbers are in 01M515KST9Z0B4B1D4HJV3Y2CM. The load
  part is superseded by 01M588HSXQ2KWX0V8RYM65CPFR.
- **01M515KST9Z0B4B1D4HJV3Y2CM** `riff top` on each machine shows the
  numbers of each machine that runs workers. The first source of a
  machine wins: the status of its live workers host; for the machine
  of `riff top`, that machine, when its limit of workers is more than
  0; the numbers that a live session of the machine sent in the last 3
  minutes.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M515KSWVGCBS76AWZV74TJQ3** The `riff mcp` of the lead sends the
  numbers of its machine to riff-server after each look of its monitor,
  also with the monitor off, while its session is the lead and the
  limit of workers of the machine is more than 0. The numbers are the
  text of the status of a workers host. `POST /v1/machine` is a
  signal: the server keeps the last numbers of each session in memory,
  and `who` shows them with their age.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M515KSZHC9Y2XQSS73PZ27R0** In `riff top`, a host with a live
  worker and no numbers shows the line `no numbers`.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M421QQ1K7EFDV2PVPTSTE5FK** One monitor runs on a machine at a
  time: it holds the lock `monitor.lock` of the local directory.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3ZVS08G1PES6N2MRM9N3PH4** The skill names `riff workers reap`
  and `riff worktrees clean` for orphan processes and stale worktrees.
  No line of the skill tells a session to run `kill`, `pkill`,
  `git worktree unlock` or `git worktree remove`. A test checks it.
  Superseded by 3.0 (01M4Z60B7XT7462XMV49GJTVSJ).
- **01M3ZVS08H3PWV5WDJ31SDZM9G** The skill tells a session: a refusal
  of a raw command never blocks work. When no riff command covers the
  case, the session files an issue for it and tells the lead.
  Superseded by 4.0 (#1173).

## Builds

- **01M3JEE7KZR5VVJGZQD82AA6NH** Replaced by
  01M3MNVT7G701SDP1Z1THMRDQ2.
- **01M3MNVT7G701SDP1Z1THMRDQ2** Replaced by
  01M3MX1DYY6AVDW946NR0B9T2C and 01M3MX1E3R5WESVHA8RZXFQR1J.
- **01M3MX1E3R5WESVHA8RZXFQR1J** Replaced by
  01M3N73E5JWFTYZ90JX5AVFGEP to 01M3N73EEQ4HPCPPAGCAHH3S6B.
- **01M3JEE7RDTDD3KQMKH41E8D57** Replaced by
  01M3MX1E65XGWDZ062PQ9YXQ5T.
- **01M3MNVT9TYNXZ8V845BHKQADV** Replaced by
  01M3MX1E8M9TKBN90P4DYKH3H8.
- **01M3JEE7TPZMNK7X6JXJ7GWFPP** When the versions cannot talk, the
  start hook gives the session the error, and tells it to tell its
  user at once and not to use the riff. Superseded by 4.0 (#1173).
- **01M3MNVTE6GAK4WRSCFGYVS0BE** Replaced by
  01M3NT6WZTKAFKGDWGCFKC8TB5.
- **01M3NT6WZTKAFKGDWGCFKC8TB5** When a new `riff` binary is on disk,
  `riff mcp` runs it in its place at the first moment with no request
  in flight, with no end call, so that the claims of the session stay.
  It keeps stdin and stdout, so the connection to Claude Code stays.
  The new process answers the next request with no new handshake. No
  person runs `/mcp`. Superseded by 4.0 (#1173).
- **01M43F5F9AQ9S39E1JZF8EBJEH** Before `riff mcp` runs a new
  binary in its place, it runs that binary once as a check, with the
  same arguments and the client of the session. The check does each
  step of the start that can fail, and serves nothing. Only when the
  check passes does `riff mcp` run the new binary. When the check
  fails, the old process keeps the tools of the session, says the
  error once on stderr, and waits for the next new binary.
  Superseded by 4.0 (#1173).
- **01M43F5KH7RE8241T18ZJJH7DV** Replaced by
  01M53QQR7J7DG86SMAERKMYSAF.
- **01M53QQR7J7DG86SMAERKMYSAF** A `riff mcp` whose client sent no
  initialize request runs a new binary too. A client of the MCP
  protocol `2026-07-28`, as Claude Code is, starts with
  `server/discover` and sends its facts with each request. The old
  process gives the new one the hidden option `--client modern`, and the
  new process serves the connection with no handshake and no peer
  info. Only a `riff mcp` whose handshake did not end does not run a
  new binary. Superseded by 4.0 (#1173).
- **01M53QQRAJMB24B0804YHPMN18** When the `riff mcp` of a session still
  runs a replaced riff 2 minutes after the new riff came, it cannot
  start again by itself. Then the PostToolUse hook of the session gives
  the session one line, once for each new riff: the tools run the old
  riff, and the fix is `/mcp`, then reconnect `riff`. The reconnect
  frees the claims of the session. A worker gets no line: it ends after
  its item (01M517BT9WBQ4R79Z9KVY9SG1B). Superseded by 4.0 (#1173).
- **01M5512RG2TTTS85VY8CEFF8R4** `riff mcp` answers the MCP handshake
  at once. It does not wait for its `register` to riff-server: the
  register runs beside the handshake, and each tool call waits for the
  end of the register. The keep-alive and the other tasks of
  `riff mcp` start after the register. Superseded by 4.0 (#1173).
- **01M5512RKABQ70QPGC48S5F67W** When the riff tools refuse the
  `tools/list` of Claude Code, the session has no riff tools.
  `riff mcp` then keeps a mark for its agent process, and removes it
  after a list that works. While the mark is there, the SessionStart
  hook gives the session one line at each start of a context, and the
  PostToolUse hook gives it once for each mark. The line says to use
  the riff commands with the Bash tool. It tells a worker to tell its
  lead, and each other session to tell its user to reconnect riff in
  `/mcp`. Superseded by 4.0 (#1173).
- **01M43F5KE7G2A2A9PSVRJPPNET** When the `riff mcp` of a session
  ended and the session goes on, the status line of the session and
  each end of `riff watch` say it in one line, with the fix: reconnect
  riff in `/mcp`. Superseded by 4.0 (#1173).
- **01M3Q55KJ8BKMPE9RADB63X8SP** When a new `riff` binary is on disk,
  `riff workers host` runs it in its place, as `riff tail` does, but
  only between two requests of the lead. The new host keeps the
  session of the old one. Its workers go on.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M517BT9WBQ4R79Z9KVY9SG1B** When the `riff workers run` of a
  worker runs a binary that a new one replaced, the worker ends in
  place of its next clear (01M3XV0562D3H3P22CJDBPAZBH), as
  `riff workers stop PANE` does. The lead gets a note with the pane and
  the host. riff never ends a worker that holds a claim. The rollout
  starts a new worker on the new riff for the free work.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M51M3ZRKQZ8KDVRGJR3XAY00** When `riff init` or the riff daemon
  adds rules to the user settings of Claude Code, it marks the add in
  the local files of riff. `riff workers run` gives `claude` the mark of
  its start. A worker whose mark is not the last mark ends in place of
  its next clear, as in 01M517BT9WBQ4R79Z9KVY9SG1B. The lead gets a
  note with the pane and the host.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3NJCRVZW5BFQYZ9N185K2D2** `riff mcp` records its build on the
  machine, with its session record, while it runs.
  Superseded by 4.0 (#1173).
- **01M3NJCWBFJK03AC64XN04TTH0** Replaced by
  01M3NT6X22A4GNFTNKRYV8Z4N1.
- **01M3NT6X22A4GNFTNKRYV8Z4N1** When `riff-server` runs a newer
  release than the `riff mcp` of a session, the status line of the
  session adds a tag: `update vX.Y.Z: riff update`; with
  `update.auto = true`, `updating to vX.Y.Z` while the update by itself
  runs or starts; and `vX.Y.Z installed` when the installed riff has
  the release but the session does not yet. Only the release counts,
  not the commit. Superseded by 4.0 (#1173).
- **01M3NJCWDN5APKZ3Z53XQR8P0B** The tag makes no extra call to
  `riff-server`. It comes from the build in the answer of the status
  line, also an answer that riff cannot talk to. With no answer within
  the wait of the status line, it shows no tag.
  Superseded by 4.0 (#1173).
- **01M53N7KN4QKZ70W2GNQ3VQWQ9** On the line 1.x, a release that the
  test of 01M3N73EEQ4HPCPPAGCAHH3S6B makes a major is a minor. The
  next major is `3.0.0` (01M4Z60B8N9W9TBE7KKDJQAN2C). A release of only
  fixes, where the old and the new builds act the same, is a patch.
  Superseded by 01M5RG6BTXAJY8DV6MT315VZWF.

## Code

- **01M43DKYYEYW3TQ2CKS36VRZ0V** The skill tells a session to look for
  a full check of its own that runs before it starts one, and to wait
  for its end. A run in the background is one task.
  Superseded by 4.0 (#1173).

## Sessions in containers

### The re-cut: isolation by default

- **01M5515CA4CZ6KV4ZD6Y0744Z7** The rollout gives free work to the
  idle workers in tmux first. It asks the daemon for the wave only when
  free work is left and no worker is idle. A worker that refused its
  request does not count (01M49ZK1C1P817KE63EXV3EJDC). A daemon that
  does not answer runs no wave. After an ask, the lead gets a note.
  When no daemon answers, or the ask fails, the lead gets one note with
  the reason, and the rollout asks again at its next look. The next
  note comes for a new reason, or after a wave runs or an ask works.
  While a wave of the repository runs and no idle worker joined, the
  look makes no call of `gh`.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M5515CCV7WNKVZVAMQ375J0M** While the rollout is on, the message of
  a higher limit says only the change: the rollout starts no worker in
  tmux. While it is off, a higher limit that gives room for free work
  wakes the lead with the `riff workers start` command
  (01M3X30RA3X08JBJ2JBVCCNEH3). `riff workers interval` and the message
  of a new interval say that the rollout gives free work to the idle
  workers, and runs the wave of the daemon for the rest.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M53NGVMD2PGEC0P9S3X4CTHT** The lead of a person runs in a
  container with `riff lead start` (01M501P9MXNAP5QXNVBVF5JJFN), or on
  the host as a session of Claude Code with the plugin of riff, as in
  riff 1.x. Each lead gets its wakes from riff-server. The plugin, its
  skill and its hooks, `riff connect`, `riff enable`, `riff disable`,
  `riff mcp`, `riff watch` and the status line stay in riff 3.0.0, for
  a session on the host. Superseded by 4.0 (#1173).
- **01M53NGVQDKCTNMG54YDFE22T2** riff 3.0.0 keeps the workers in tmux
  and each of their parts, as in riff 1.x: `riff workers` and each of
  its subcommands, the workers host, the clear of a worker, the reap,
  the pool of build jobs, the monitor, `riff worktrees`, the worker
  role of a session on the host (`RIFF_WORKER`), the text of the
  workers in the skill of the plugin, and the permission rules of a
  worker. A person starts a worker in tmux with `riff workers start`.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M53NGVTE186GE5E48E68N0BY** The last minor release of riff 1.x ends
  Wave 30: 1.8.0. riff 3.0.0 releases when each build item of the
  re-cut merged. Its tag deploys it to riff-server, as each release.
  Each machine with `update.auto` on then takes it from riff-server by
  itself (01M3N7JJEKZMN1E5NJQRK2QYVB). Then the line 1.x retires: each
  GitHub release of riff 1.x becomes a prerelease with a first line
  that says that the line is retired (#795). No patch of riff 1.x comes
  after riff 3.0.0. Superseded in part by 01M56N6KSMFJ46PW99Z80GF9DM:
  the releases of riff 1.x stay normal releases.
- **01M53NGVXEMBTJ8E4K3MHPQ7W9** riff-server 3.0.0 serves the API of
  riff 1.x. So a machine of riff 1.x works until it takes riff 3.0.0.
  Superseded in part by 01M55FCZNNZGKM4EKAXZQFTDWR: only from riff
  1.8.2. Superseded by 01M5RG6BXBQVFABA1N4NRZBMQF.
- **01M55FCZNNZGKM4EKAXZQFTDWR** The bridge: a `riff` and a
  `riff-server` of the lines 1 and 3 talk, in both directions, from
  riff 1.8.2 and riff 3.0.0. The same line and the line before still
  talk (01M3MX1DYY6AVDW946NR0B9T2C, 01M3MX1E1EY1M7JGNCN6FCEVQK). Each
  other pair is refused. On the side of the server, riff-server of the
  line 3 takes a `riff` of the line 1 only from 1.8.2. It refuses an
  older `riff` of the line 1 with 409 and the step to update riff,
  before the call takes effect. The check of `riff` does not change.
  riff 1.8.2 releases from v1.8.1 on the branch `release/1.8`, before
  riff 3.0.0. riff-server 1.8.2 still uses no checkpoint of the line 3
  (01M4YNCEMNRCAXD37V03T4JQYR). Superseded by
  01M5RG6BXBQVFABA1N4NRZBMQF.
- **01M53NGWJ48MMYPQNJQ0EY9T47** The book page "riff 3.0" has the plan
  of the re-cut, with a mermaid diagram of a container session with
  riff-server, what riff 3.0.0 removes, and the state that it leaves on
  a machine. It has no other text of the riff of peers. The page
  "Design: the riff of peers" holds the how-tos of the riff of peers of
  riff 1.6 to 1.8, and goes with #959. The page "Design: no cloud, a
  later plan" says that its plan is not in riff 3.0.0, and that its
  items wait in the Backlog.

### The riff daemon

- **01M4ZDTVXY84ETYJWMWDMQ34CN** `riff run ITEM` does one work item in
  one rootless podman container, through the daemon. It asks the daemon
  for the image (the daemon pulls it when it must), then for the
  session. The host does each riff step with riff 1.x: it checks the
  facts, starts a worker session, claims the item, makes the clone,
  sends the verify request and releases the item. The daemon starts the
  container, writes `events.jsonl`, and sends each event, with no
  secret, to `riff run`. No riff runs in the container. With no daemon,
  `riff run` stops before the claim, and says how to start the daemon.
  Ctrl-C asks the daemon to stop the container. When `riff run` ends
  with no stop, the session goes on in the daemon. Superseded by
  01M4ZKX0NXN7E33VC3PSFGSPDT.
- **01M4ZHW2083VFTQJ772ZJ27XAC** The daemon keeps its state in the
  folder `state` in its folder: the log and the checkpoints of
  `riff-state`. At each start, it loads the newest checkpoint that it
  reads, and replays the log after it. At its first start, it makes the
  riff of its machine with a random ID. At a stop, it writes a
  checkpoint. Superseded by Wave B (01M519Z2JMAKN0T6JZFBPSTKC8).
- **01M4ZHW22F8HE2E1WDSTNDJW4H** The daemon runs one command at a time.
  The records of a command are in its log before the command gets its
  reply. When a write of the log fails for good, each next command
  fails until the daemon starts again. Superseded by the re-cut
  (01M53NGW3FR555TWP8Z8HHRVDJ).
- **01M4ZHW24JZZXM3JJXG3PRJS7N** `riff daemon status` also shows the ID
  of the riff of the daemon and the position of its log. Superseded by
  the re-cut (01M53NGW3FR555TWP8Z8HHRVDJ, 01M53NGW6F5HP1DZ9QRGP50MGT).
- **01M4ZDTW25YV0GMMVGWN6JTJC5** Until `riff relay` (#717), the input
  and the events of a session go through the `podman run` of the
  daemon. At a stop of the daemon they end: `claude` ends after its
  turn, and its container goes. `riff run` then says that the daemon
  stopped, finds the pull request, and releases the item. Superseded by
  01M4ZKX0F25S284R0N7KZVTHNA.
- **01M588HSXQ2KWX0V8RYM65CPFR** The line of numbers of `riff top`
  shows the 5-minute load against the limit of the monitor:
  `load5 9.8 of 12`. It has the warning style when the load is over
  the limit. This supersedes the load part of
  01M421QPZ9E01PQ62PDBH378SJ.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).

### riff relay

- **01M4ZKX0H9N82ZBC9PKSPTQV4N** Until #719, the daemon serves the riff
  tools of a session on the riff-server of `riff run`, as the worker
  session that `riff run` started. That session holds the claim of the
  item. The daemon uses the sign-in of the person on its machine. A
  session gets only the tools that act on riff-server, and on no folder
  of the host and no claim: `whoami`, `who`, `threads`, `read`, `post`,
  `tell`, `status` and `blocked`. The daemon refuses each other riff
  tool with a text that says why. Superseded by
  01M4ZV3B6T2KB09WJ59F07HXET.

### The lead, the view and the takeover

- **01M51KJ1V0STB4QCN4N5SBYGSP** `hold` has the field `interrupt`.
  With it, the relay writes the interrupt control request of
  stream-json on the stdin of `claude` before the close, as the Agent
  SDK does. The hello of the relay says `interrupts` when the relay can
  do it. A relay of riff 1.5 reads a `hold` with `interrupt` as its own
  `hold`. The protocol stays 4. Superseded by
  01M5RG6C25DB49ZKM03Y93QB1F.

### The permission rules

- **01M5084K09Z2S7N57315CX2R0F** `riff init` checks that the user
  settings of Claude Code (`$CLAUDE_CONFIG_DIR/settings.json`, else
  `~/.claude/settings.json`) have each rule of the user place. When
  rules are missing, it names each rule, and asks one yes or no
  question before it adds them. `--yes` answers yes with no question. A
  no, or no terminal with no `--yes`, changes nothing, and the check
  fails. `riff init --check` names each missing rule and asks nothing.
  riff adds only the missing rules, and keeps each other rule and key in
  its order. The rules of the user place name no repository and deny
  nothing. Superseded by 4.0 (01M59EWWDC7EBQEG02TVNN81HD).
- **01M5084K2Q3HH4BK7DJMYQD7C1** `riff init` asks its question also
  when another check fails. After a yes, the check of the rules passes.
  When another check still fails, riff installs nothing and pulls
  nothing. Superseded by 4.0
  (01M59EWWDC7EBQEG02TVNN81HD).
- **01M51M3ZHHBE61CEG9T6ENFG18** With `update.auto = true`, at each
  start the riff daemon adds each rule of the user place that the user
  settings of Claude Code lack, when its own binary is a release. It
  writes as `riff init` does, keeps each other rule and key in its
  order, and removes no rule. With `update.auto = false`, it changes
  nothing. `riff update --auto` is the only switch, and it asks no other
  question. Superseded by 4.0 (01M59EWWDC7EBQEG02TVNN81HD).
- **01M51M3ZPAEMJ64266QSJBNQ46** The riff daemon logs each rule that
  its sync adds, and one line of the result. It keeps that line in the
  local files of riff. `riff init` and `riff init --check` show whether
  the sync is on, and that line. Superseded by 4.0
  (01M59EWWDC7EBQEG02TVNN81HD).

### The steps of an update

- **01M5GT0QV72BZA4VVWDPR1WG17** riff keeps no command only so that the
  update of an older line can finish. When the update from the line
  before stops on the new riff, the book gives the step that a person
  runs by hand. Superseded by 01M5RG6C4H3YPACS41S6EPS87E.
- **01M5JFF7TAVA87TMJXK25HDJ19** riff init removes the plugin of riff 3
  that is left on the machine: the folder claude-plugin in the data
  folder of riff, and the keys enabledPlugins.riff@riff,
  extraKnownMarketplaces.riff and a statusLine that runs riff statusline
  in the user, project and local settings of Claude Code. Each other key
  stays. riff init --check names them and removes nothing. The riff
  daemon of a release removes them at its start behind the gate of the
  update by itself, and riff update --check names them with the fix riff
  init. Superseded by 01M5RG6C4H3YPACS41S6EPS87E.

### riff run

- **01M4Z81APKC14AVNJWCGRQKNVW** `riff run ITEM` does one work item in
  one rootless podman container, with no daemon. The host does each
  riff step with riff 1.x: it starts a worker session, claims the item,
  sends the verify request and releases the item. The container does
  the work. No riff runs in the container. riff shows each event of
  `claude` on the terminal, with no secret (01M4Z60B8KDXD190RWC7D184Q3).
  Superseded by 01M4ZDTVXY84ETYJWMWDMQ34CN.

### Data and peers

- **01M4Z60B8AFV68ZSAFA540G73C** The state machine of riff-server
  becomes a library in the daemon. Superseded by the re-cut
  (01M53NGW3FR555TWP8Z8HHRVDJ).
- **01M4Z60B8BHAR9GHCC0M7V7CV5** The log has the shape of peer to peer:
  each author has an append-only signed log, and each type of record has
  one writer. A message or a status: its author. The plan and the waves:
  the lead of the repository. The members: the owner
  (01M4Z60B8C8JBV1T8T0W6ZKFE6). A claim: a compare-and-swap push to the
  forge (01M4Z60B8C3W5TMPVWNFXD1SYN). The keys are ed25519.
  Superseded by the re-cut (01M53NGW9FH02EM74GKY829808).
- **01M4Z60B8C8JBV1T8T0W6ZKFE6** The owner signs the key of each member.
  riff 3.0 has no sign-in with an OpenID Connect provider, no OAuth
  client and no riff token. Superseded by the re-cut
  (01M53NGVB7SZJW7X8SYS3B0ZHG, 01M53NGW9FH02EM74GKY829808).
- **01M4Z60B8C3W5TMPVWNFXD1SYN** A claim is the ref
  `refs/riff/claims/ITEM` on the forge, for example
  `refs/riff/claims/issue-12` or `refs/riff/claims/verify-issue-12`. It
  is not a branch. Superseded by the re-cut
  (01M53NGW9FH02EM74GKY829808).
- **01M4Z60B8DYAVEQF6F8N9QP7MS** Each claim is a new commit: the empty
  tree, no parent, no signature, and a message with the item, the user,
  the session and the time. The daemon makes each claim and each
  release, not the session. A claim is a create with
  `git push --force-with-lease=REF:`: the empty value means that the ref
  does not exist yet. A release is a delete with the lease of the claim
  commit. Superseded by the re-cut (01M53NGW9FH02EM74GKY829808).
- **01M4Z60B8EB7A44A41CWWP7G8K** A claim fails when its lease fails in
  the client (`stale info`) or on the server (`cannot lock ref`). riff
  reads both as: another session holds the item. Superseded by the
  re-cut (01M53NGW9FH02EM74GKY829808).
- **01M4Z60B8FQ3GMWQH12CXE0JS9** A claim ref has no time limit. It holds
  until its release, or until the lead deletes it
  (01M4Z60B8FFRMT3DVG8H1CCZ13). Superseded by the re-cut
  (01M53NGW9FH02EM74GKY829808).
- **01M4Z60B8FFRMT3DVG8H1CCZ13** The lead deletes the claim of a session
  that is gone, with the lease of its claim commit. Superseded by the
  re-cut (01M53NGW9FH02EM74GKY829808).
- **01M4Z60B8G1VWGNKBH8ZJGJ7QJ** In a public repository, each claim ref
  and each claim commit is public, also after the release. A claim
  commit holds no secret. Superseded by the re-cut
  (01M53NGW9FH02EM74GKY829808).
- **01M4Z60B8HN1C1XAG1CA1YS36H** Peer to peer comes right after local
  isolation works. The transport is iroh (#697). The first release
  with two peers uses the defaults of iroh: the DNS discovery and the
  public relays of n0. A small discovery service on Cloud Run comes when
  riff leaves those defaults. DNS discovery on comotechnologies.io is
  future work. Superseded by the re-cut (01M53NGW9FH02EM74GKY829808).

### Wave B: the record

The requirements of Wave B (#803) say how the daemons of two or more
people share one riff. A requirement of Wave B holds when the item of
Wave B that builds it merges. A requirement that ends with "Superseded
by Wave B (ID)." holds until then. The re-cut supersedes each
requirement of Wave B (01M53NGW9FH02EM74GKY829808).

- **01M519Z1WKH6THH971AQWHH8CN** A record is one change of the riff by
  one author. Its bytes are, in this order: the format version (1, one
  byte), the type (one byte), the riff ID (32 bytes), the key of the
  author (32 bytes), the sequence number (8 bytes), the hash of the
  record before it (32 bytes), the time of the author (8 bytes, in
  milliseconds since the Unix epoch), the length of the body (4 bytes),
  the body, and the signature (64 bytes). Each number is big-endian.
  The riff ID and the key of the author are ed25519 public keys. The
  body is the JSON of the type, in UTF-8.
- **01M519Z1Z1FVZQQ612DEMSQ7PG** The signature of a record is an
  ed25519 signature by the author over the tag `riff record 1` and each
  byte of the record before the signature. A reader checks it with
  `verify_strict`. Each daemon keeps the bytes of a record as it got
  them, and never encodes a record again. The ID of a record is the
  BLAKE3 hash of its bytes.
- **01M519Z21ECQY89RS60DZKZQVR** The first record of an author has the
  sequence number 1 and a hash of 32 zero bytes. Each next record of
  that author has the next sequence number and the ID of the record
  before it. So the records of one author make one chain with no gap.
- **01M519Z23WX0A142N74BKAKKYY** Each type of record has one writer and
  a largest size of the whole record. `riff`: the owner, when it makes
  the riff, 1 KiB. `member` and `removed`: the owner, 1 KiB. `left`: a
  member, for its own key, 1 KiB. `session`: the daemon of the session,
  for its start, its place, its lead mark, its claims, its status and
  its end, 4 KiB. `message`: the daemon of the session that posts, for
  a message, a note or a status request, 64 KiB. `plan`: a daemon of a
  lead, or the daemon of the owner, for a hold, a free, a pause and a
  resume, 4 KiB.
- **01M51Q0JX87PPFGPABFPVWWW0E** The byte of each type of record:
  `riff` 1, `member` 2, `removed` 3, `left` 4, `session` 5, `message` 6
  and `plan` 7. The byte 0 is no type: a reader refuses it. Each other
  byte is a type that a reader does not know.
- **01M51Q0JZR00GHDQSGGSAQR4HG** A `session` record holds the whole
  state of its session: its place, its lead mark, its claims, its
  status and its end. Its daemon writes a new record at each change.
  The last record of the session in the chain of its author counts.
- **01M519Z26AP5NZSFHWRQ2QCV9D** A reader skips a field of a body that
  it does not know. It keeps and passes on a record of a type that it
  does not know, after the checks of the bytes, the chain and the
  signature, with a largest size of 64 KiB. It does not apply that
  record.
- **01M519Z28S7FB4T1H52642JXZ1** The time of a record is the time of
  its author. riff shows it, and never orders records by it. Each daemon
  gives each message the next number of its own: the order in which it
  kept the messages. Another daemon can give the same message another
  number.

### Wave B: the keys

- **01M519Z2BB0YJY5CRMB54JVYFA** Each daemon has one ed25519 key. The
  daemon makes it at its first start, and keeps it in the OS keyring
  (R21). The private key never leaves the machine, and never goes into
  a log line, a message, a record or a file of a clone.
- **01M519Z2DSFGHNV5S88AYFK7PY** The key of the daemon is also the key
  of its iroh endpoint. So one public key is the endpoint ID of the
  daemon, the author of each record that it writes, and the member key
  that the owner signs.
- **01M519Z2G77CT94Y6XPW9BCRK9** The key and the records of a daemon go
  together. When a daemon finds its key but not its own records, or its
  records but not its key, it makes a new key and a riff of its own.
  The owner then invites the new key. Recovery of a lost key is future
  work.
- **01M51BFYNMYHSQ62BBJES3CCWT** A keyring that is locked or does not
  answer is not a lost key. The daemon then waits for the keyring, as
  `riff` does (01M4385CCATXC0B8HV1XD6EFWG): it looks again each 30
  seconds, makes no new key and no riff of its own, binds no endpoint
  and writes no record. When the keyring answers, the daemon loads its
  key and goes on. Only a keyring that answers with no key of the
  daemon is a lost key (01M519Z2G77CT94Y6XPW9BCRK9).
- **01M519Z2JMAKN0T6JZFBPSTKC8** The daemon keeps its state in the
  folder `state` in its folder: the records of each author, and the log
  and the checkpoints of `riff-state`. At each start, it loads the
  newest checkpoint that it reads, and replays the log after it. At its
  first start, it makes its key and a riff of its own: the riff ID is
  its public key, and its first record is `riff` with the user name of
  the person (`USER`). At a stop, it writes a checkpoint.
- **01M519Z2N2PMZJ0CYAPV0A47BZ** `riff daemon status` shows the public
  key of the daemon, the riff ID, and the user of the owner.
- **01M51JW1XRZWC3Q7G32V0X0D89** The OS keyring keeps the key of a
  daemon in the secret `daemon-key DIR`, where `DIR` is the folder of
  the daemon. So each folder of a daemon has its own key.
- **01M51JW20CSAECFRZTJ95P6F3E** Each error of the keyring is a wait of
  the daemon, also a keyring that riff cannot open. The daemon says the
  error in one line, and looks again each 30 seconds.
- **01M51JW233MXB72MHHRAX4E4NF** The daemon serves at its start, and
  reads its key in the background. While it waits for its key, it runs
  its sessions, and `riff daemon status` shows the line of the wait.
  Until the records of Wave B, a session needs no key of the daemon.
- **01M51WPSH4KBQMVT3XGDD45G63** The daemon says the line of its riff
  (a first start, a new key, or the key that it read) only after it
  has its riff and its records. So `riff daemon status` after that
  line shows the key, the riff ID and the owner.
- **01M51JW25SPDW564A3DG31D7XM** When the daemon makes a new key
  because its key and its state do not go together, it moves the old
  folder `state` to `state.lost-MS` beside it. riff never reads that
  folder.

### Wave B: the sync

- **01M519Z2QE6QYPJX8HVY17GR6Z** Each daemon binds one iroh endpoint at
  its start, with the preset `N0`: the DNS discovery and the public
  relays of n0 (01M4Z60B8HN1C1XAG1CA1YS36H). It keeps the endpoint
  bound while it runs. Its ALPN is `riff/3`, the major version of riff,
  so a daemon of another major version does not connect. riff takes
  iroh with no feature `metrics`.
- **01M519Z2SV9XSQA4N59N10JAQC** A daemon keeps one connection to each
  other daemon of its riff that is online. Of two daemons, the one with
  the lower key connects. When a connection ends, it connects again:
  each try has a limit of 5 seconds, and the wait between two tries
  starts at 1 second and doubles up to 60 seconds.
- **01M519Z2WAGQEX1H4EGZXWBZT6** At each connect, each side checks that
  iroh gave the key of the other side, and that the key is the owner or
  a member of its riff that is not removed and did not leave. Else it
  closes the connection. The one other connect is a join
  (01M519Z3YA9WBH9TWQPAH6A64Z).
- **01M519Z2YSXADT45A1R0N70WRQ** After the connect, each side sends its
  heads: for each author of the riff, the highest sequence number that
  it has. Then each side sends each record that the other side lacks,
  for each author in sequence order, the records of the owner first.
  After that, each side sends each new record at once: its own, and
  each record that it got from another daemon. So a record reaches each
  member through each daemon that has it.
- **01M519Z317AT7KT7Q643NVYZD1** A daemon checks each record before it
  keeps it (01M4Z60B8K4C6DHFAKNCAWR91S), in this order: the size of its
  type; the version and the type; the riff ID; the author is the owner,
  or a member that the records of the owner admit and that is not
  removed and did not leave; the sequence number is the next one of the
  author; the hash of the record before it; the signature; the body is
  the JSON of its type; the writer of the type. The user of a `session`
  or a `message` record is the user of the member key of its author.
- **01M519Z33N2MW43XY62NYC6RRH** A daemon does not keep a record that it
  cannot check yet: a sequence number after the next one, or an author
  that no record of the owner admits yet. The next heads give it again,
  in order.
- **01M51BFYQVCFEWCJZERBD4S5N5** A daemon drops a record that it has
  already: a record with the ID of a record that it keeps. It drops it
  before the checks of 01M519Z317AT7KT7Q643NVYZD1, with no fault: it
  logs nothing, and the connection stays. It does not apply the record
  again, and does not pass it on again. A daemon passes no record back
  to the daemon that sent it. So in a full mesh, a record that comes
  over two paths is kept one time, and each connection stays.
- **01M519Z363W5GV4MV5RDB6ECG0** A daemon refuses a record that fails a
  check. It closes the connection to the daemon that sent it, and logs
  the author, the sequence number and the check that failed, with no
  body. It connects again after the wait of 01M519Z2SV9XSQA4N59N10JAQC.
- **01M519Z38G3RAZPKGFA3R2CB3P** A fork is two records of one author,
  each with a good signature, with the same sequence number and other
  bytes. The daemon keeps the first one that it kept, and keeps no next
  record of that author. It keeps both records as the proof, shows the
  author as forked in `riff members`, and posts a note to the lead of
  its user. The owner removes the forked key, and the person joins
  again with a new key.
- **01M519Z3AXFKBHJSAW46KWFTXS** A daemon applies each record that it
  keeps to its state, in the order that it keeps them. The state of each
  type comes from one writer, so two daemons that keep the same records
  have the same state.
- **01M519Z3DB3MVP0FK3FBC9KSGD** When a daemon keeps a message, also a
  message of another daemon, it wakes each session of its own that the
  `to` selectors of the message match. `riff relay watch` in a session
  waits for the next wake of the session on its daemon.
- **01M519Z3FTAZ20DQKDTW09M6NS** The daemon serves the riff tools of
  each session that it owns on its own state, which it shares with its
  peers. It does not use riff-server. A worker and a verifier get the
  tools that talk: `whoami`, `who`, `threads`, `read`, `post`, `tell`,
  `status` and `blocked`. The lead also gets `lead`, `hold`, `free`,
  `pause` and `resume`. The daemon refuses each other riff tool with a
  text that says why.
- **01M519Z3J7Z0M3PNDY249Q57FH** A hold and a pause are `plan`
  records. A free names the ID of each hold record that it ends, and a
  resume names the ID of each pause record that it ends. An item is
  held while one hold record of it has no free that ends it. A
  repository, or the whole riff, is paused in the same way. A free or a
  resume ends a record only when its author is a daemon of the user of
  that record, or the daemon of the owner. Only the owner pauses and
  resumes the whole riff. So the holds and the pauses come from the set
  of records, not from their order: a free that a daemon keeps before
  its hold ends that hold when the hold comes.
- **01M51QQGYDR9NZ702NFEQR512W** The daemon applies each record to the
  state of its riff of peers, and not to `riff-state`. That state comes
  only from the set of records, with no clock and no presence of one
  machine. `riff-state` decides a post by the live sessions and the
  clock of one server, so two daemons with the same records could get
  two states.
- **01M51T470TNWVFF97E474FV7BY** In the riff of peers, the user of a
  session of a daemon is the user of the key of that daemon, also when
  riff-server gave the session another user.
- **01M51T4734E3B3Z4BZZJQYYR32** A daemon that runs a session with a
  lead mark may write a `plan` record, also when another lead mark
  names that mark. So the writer check of a `plan` record reads only
  the records of its author, and does not depend on the order of the
  records of two authors.
- **01M51VZG27DXSRHBT8M14S7B9A** The riff tools of a session of the
  daemon with a riff-server act on that riff-server, as in riff 1.x:
  each session of `riff run`, of a wave and of `riff lead start` that
  has one. `riff relay watch` of that session waits for its wake on
  riff-server, and the riff of peers has no record of that session.
  The daemon serves the riff tools on its riff of peers
  (01M519Z3FTAZ20DQKDTW09M6NS) only to a session with no riff-server.
  So the work of one repository is on one riff only
  (01M519Z506XWTSV1CJ6CCVFXEF). Superseded by Wave C
  (01M51R8FXQFXWHRD374577HZ3H).
- **01M51BFYT2ZGN4W5E5R87GPVCM** A session takes the lead with a
  `session` record that has the lead mark. The record names the ID of
  the record that gave the old lead of its user in the repository its
  lead mark, when its daemon knows one. The lead of a user in a
  repository is the session that runs with a lead mark that no other
  lead mark names. When two such sessions are left, the session of the
  daemon with the higher key counts. So no daemon orders the leads by a
  time (01M519Z28S7FB4T1H52642JXZ1).
- **01M519Z3MMW5RKAR1R14PPQ4T9** A new member gets each record of the
  riff, from the first one. A prune of old records is future work.
- **01M51Z9ZT69AQ0V2PX8T439NA5** The heads of a daemon name, for each
  author, the last sequence number and the ID of that record. When the
  heads of the other side name a record that a daemon has with another
  ID, the daemon sends its own record with that number first, and no
  next record of that author. So the other side sees the fork
  (01M519Z38G3RAZPKGFA3R2CB3P).
- **01M51Z9ZX4P0HEH53YFJ6XAWST** A daemon sends its heads on a link at
  the connect, at once when it drops a record that it cannot check yet
  (at most one time in each second), and again each 60 seconds. At the
  heads of the other side, it sends each record that the other side
  lacks.
- **01M51ZA001C8MZM4BTBYTXRJ8M** A daemon sends the proof of each fork
  that it keeps first at each heads of the other side, and the proof of
  a new fork to each link at once. A daemon that knows the fork drops
  both records with no fault. So each daemon of the riff learns the
  fork.
- **01M51ZA02XQ31PWNBSD46MTKA6** The note of a fork goes to the lead of
  the user of the key of the daemon, in each repository where that user
  has a lead. It is a `message` record of the kind note, from the user
  of the key on the host of the daemon, with no session. With no lead,
  the log line of the daemon is the only report.
- **01M51ZA05RTRE9TSMBP20AS20H** A daemon takes a connection of each
  key that may link, also of a higher key, for example at a join. Its
  own connect to that key waits while that link runs. A new connection
  of a key takes the place of the old link of that key.
- **01M51ZA08MGF20MPHATHE81B44** A link that ends for a fault of a side
  does not start the wait of 01M519Z2SV9XSQA4N59N10JAQC again at 1
  second: a key that is not a member, a refused record, a frame that the
  daemon cannot read, or a record that it cannot keep. Each other end
  does. A link that falls more than 4096 new records behind ends. Its
  next connect sends what the other side lacks.
- **01M51ZA0BDZNXBB7R5SB8CEGA9** `riff daemon status` shows each peer
  link of the daemon: the start of the key, its user and its host, and
  its state. The state is up since a time, on a direct path or on a
  relay; connects again after a number of tries, and why; or waits for
  the other daemon to connect. A daemon that cannot bind its endpoint
  logs why, runs its sessions, and shows no peer links.
- **01M51ZA0E79V2FPBMQ6F1MYZJ5** With `RIFF_PEER_NET=local`, the daemon
  binds its iroh endpoint on 127.0.0.1, with no relay and no DNS. It
  finds a daemon only by an address that it gets. Each test of riff sets
  it, so no test reaches n0.

### Wave B: the members

- **01M519Z3Q27V93QTFKXPQX42J1** A riff of Wave B has an owner and
  members. The owner is the key that made the riff. The owner role does
  not pass to another key, and the riff has no admin: both are future
  work. A member is one key, with one user and one host. A user can have
  many keys: one for each machine.
- **01M519Z3SGEHVMS85HZD169CAP** `riff invite USER` makes an invite for
  one new key of the user USER. Only the owner runs it, at the CLI, with
  a terminal (01M4Z60B8JEX15M1Q2435FZSDM). It prints one invite string:
  `riff1-` and the base32 of the riff ID, the user, a random secret of
  32 bytes and the end time. An invite works one time, for 24 hours.
  The daemon of the owner keeps the hash of the secret, the user and the
  end time in its folder. They go into no record.
- **01M519Z3VXY4R3WDSSBQCVYTS4** An invite string is a secret until it
  is used or ends: a person who has it can join one time as USER. The
  owner sends it to the person by hand. `riff invite` says so.
- **01M52JPVT5DY1VJQMQCNY9Z0PC** The user of the owner stays `USER` of
  its machine (01M519Z2JMAKN0T6JZFBPSTKC8), also when the riff user of
  the person has another name. A new key gets the user of its invite. So
  the book says to add a second machine of a person with `riff invite`
  of the user that `riff members` shows for that person (#846).
- **01M519Z3YA9WBH9TWQPAH6A64Z** `riff join STRING` on the machine of a
  person: the daemon connects to the daemon of the owner by the riff ID,
  and sends the secret and its host name. The daemon of the owner checks
  that the invite is known, not used and not ended. Then it writes a
  `member` record with the key of the new daemon, the user of the invite
  and the host. The new daemon gets the records of the riff and syncs
  with each member. The daemon of the owner must be online. A failed
  join changes nothing, and says why.
- **01M519Z40SHPJ1BB2SZN9AW43F** A daemon is in one riff at a time.
  `riff join` refuses while the riff of the daemon has another member.
  The records of the old riff stay in the folder of the daemon.
- **01M519Z435DTR693NAK5GSB6XA** `riff leave` on a member: the daemon
  writes a `left` record, sends it to its peers, ends its peer links,
  and makes a riff of its own with the same key. The owner cannot leave.
  Each session of the daemon leaves the riff with it.
- **01M519Z45J9RC734KBY1G8D49V** `riff remove USER` writes a `removed`
  record for each key of USER, or for one key with `--host HOST`. Only
  the owner runs it, at the CLI, with a terminal. The record names the
  highest sequence number of that key that the daemon of the owner has.
  Each daemon then refuses each connection of that key, and each record
  of it after that number. The owner cannot remove itself.
- **01M51RGNHBCY7GDSCKPE8REY6A** When a daemon keeps a `removed` record,
  it drops each record of that key after its last number that it kept
  before. Its book holds them no more, and its state comes again from
  its other records. So the order of the `removed` record and those
  records does not change the state.
- **01M519Z4823WC7D3VVTFSDYBS4** `riff members` shows the owner and each
  member key: the user, the host, the start of the key, online or the
  time when it was last seen, and forked, removed or left.
- **01M519Z4AEMDFGBF22RZ3HQR1R** Until Wave C, the member commands of
  riff-server 1.x stay with the flag `--email`:
  `riff invite --email EMAIL`, `riff remove --email EMAIL` and
  `riff members --email`. Superseded by Wave C
  (01M51R8G06XMJF3GAP7MFW06R0), and by 01M588J2QAF288ZB3M9PBN71ZS.
- **01M588J2QAF288ZB3M9PBN71ZS** The member commands of riff-server take
  the email as an argument, with no flag: `riff invite EMAIL`,
  `riff remove EMAIL` and `riff members`. They have no flag `--email`.
- **01M521J6CEJB24QFN17077KG9S** An invite string is `riff1-` and the
  base32 (RFC 4648, lower case, no padding) of these bytes: the riff ID
  (32 bytes), the secret (32 bytes), the end time in milliseconds since
  the Unix epoch (8 bytes, big-endian), and the user in UTF-8.
- **01M521J6F5QH9MW4GVY0H2JD8N** The daemon of the owner keeps each
  invite in the file `invites.json` of its folder, with the mode 0600:
  the BLAKE3 hash of the secret, the user, the end time, and whether a
  join used it. A join marks the invite used before the daemon writes
  the `member` record. When that write fails, the daemon gives the
  invite back.
- **01M521J6HX5BN0FNDBSKZDVQE7** A join is a connection to the daemon of
  the owner with the ALPN `riff/3/join`. The new daemon sends the secret
  and its host on one stream. The daemon of the owner checks, in this
  order, that it is the owner, the key, the host and the invite, and
  answers with the join or why not. After a join, the connection goes
  on as the peer link of the two daemons.
- **01M521J6MKSBAA2QP6ZFHYT9ZQ** A key has one `member` record. The
  daemon of the owner refuses a join of a key that its riff knows: the
  owner, a member, a removed key, or a key that left. The writer check
  of a `member` record refuses the key of the owner and a key that the
  riff knows. So the user of a key does not depend on the order of the
  records. Superseded in part by 01M5328FPJT7Q5Q1509BC5F0J7 and
  01M5328FVZJGCY9VPQFERM7P81: a key that left joins again.
- **01M5328FPJT7Q5Q1509BC5F0J7** `riff leave` only leaves. A key that
  left joins again with a new invite string of the owner. The daemon of
  the owner then writes a new `member` record of the key, with the user
  and the host of the new invite, and with `after`: the sequence number
  of the `left` record of the key. It writes it only when it kept that
  `left` record. The records of the key after that number count with
  the new user. The first of them names the new `member` record as the
  record before it. So a record that the key signed after its `left`
  record and before the new `member` record stays refused.
- **01M5328FS8XGVW3X6N94F65FA8** The daemon of the owner refuses a join
  of the key of the owner, of a member key, and of a removed key. The
  refusal names which of the three. A key that `riff remove` removed
  stays refused, also with a new invite: a remove is the choice of the
  owner.
- **01M5328FVZJGCY9VPQFERM7P81** Each `member` record of a key starts a
  join of the key. The user of a record of the key is the user of its
  join: the last `member` record whose `after` is lower than the number
  of the record. The writer check of a `member` record reads only the
  records of the owner: a first `member` record needs a key that the
  riff does not know, and a `member` record with `after` needs a key
  with a `member` record, not removed, and an `after` higher than the
  `after` of its last `member` record. So two daemons that get the
  `member`, `left` and new `member` records of a key in different orders
  agree on the user and the state of the key.
- **01M532QC66ER7BZPYWR7QDXAP1** A daemon that joined again writes no
  record until it has its own records up to its `left` record: the
  record would fork its chain. At `riff leave`, a removed daemon writes
  no `left` record, because each daemon refuses it, and it still gets a
  riff of its own.
- **01M521J6QBQFRDWRPR0NGHE7TM** `riff join` and `riff leave` need a
  terminal too, as `riff invite` and `riff remove` do. An agent session
  has no terminal, so it cannot move the daemon of the machine to
  another riff.
- **01M521J6T2PWA9JREDXJK6DZ0V** The `riff` record also holds the host
  of the daemon of the owner. A `riff` record with no host shows no
  host.
- **01M521J6WZT2KR5ZQE9DZ9A5AC** The file `state/joined` of a daemon
  holds the riff ID of the riff that it joined. With no such file, the
  daemon is in its riff of its own. The records of its riff of its own
  are in `state/records`, and the records of a joined riff are in
  `state/riffs/RIFF/records`. Each start of the daemon opens the riff of
  that file. `riff daemon status` shows the riff that the daemon is in,
  and the user of its owner.
- **01M521J6ZRNWBSPK7WB23TVY59** At a join and at a leave, each session
  of the daemon on the riff of peers writes its end record in the old
  riff, and its start record in the new riff. At a leave, those end
  records come before the `left` record. Each peer link sends the `left`
  record, ends its stream, and waits up to 5 seconds for the other
  daemon to close the connection. Then the daemon is in its riff of its
  own again.
- **01M52JF709YHYE7SG6V3SDHR74** At `riff leave`, the daemon goes to its
  riff of its own only after a peer link sent the `left` record and no
  link runs, or after 5 seconds. A link sent the record only when the
  other daemon then closed it with the code of a key that left: it does
  so when it keeps the record, and at the end of the stream of a key
  that left. A link that a new connection replaced does not count, and a
  moment with no link does not end the wait. A link that starts during
  the leave sends the `left` record with the records that the other
  daemon lacks, and then ends its stream. When no link sent the record
  in 5 seconds, `riff leave` says so, and names the `riff remove`
  command that the owner runs.
- **01M521J72EC8Z5HAKA1Q9P5Z24** `riff members` shows a key that has no
  link as last seen at the later of two times: the end of its last link
  since the start of the daemon, and the time of its last record.

### Wave B: the claims

- **01M519Z4CT0PRXPRM22037Z5RY** The daemon makes each claim and each
  release (01M4Z60B8DYAVEQF6F8N9QP7MS) in the main clone of the
  repository, with the git sign-in of the person. The message of a
  claim commit is `riff claim ITEM`, then the trailers `Riff-Item`,
  `Riff-User`, `Riff-Session`, `Riff-Key` (the public key of the daemon)
  and `Riff-Time` (UTC, RFC 3339).
- **01M519Z4F7Z87BXQYT7F8EABCN** The `session` record of a session names
  its claims. So each peer shows the claims in `who`, `top` and `view`
  with no call to the forge. The forge decides: before a wave picks an
  item, it reads `git ls-remote origin 'refs/riff/claims/*'`.
- **01M519Z4HK333JWVXAWQHCGK6M** The daemon releases the claim of a
  session at the end of the session. At its start, a daemon deletes each
  claim whose commit names its key and whose session does not run.
- **01M519Z4M0F2RY0VDQNB2QFAPJ** The daemon of the lead of a user
  deletes a claim of another session of that user, with the lease of
  its claim commit (01M4Z60B8FFRMT3DVG8H1CCZ13), when the daemon of the
  session wrote the end of the session, or when it had no connection to
  that daemon for 15 minutes. A claim of another user whose daemon is
  offline holds.
- **01M519Z4PESWFV8Y03SKTSGV4Q** `riff release ITEM --forge` at the CLI
  deletes a claim with the lease of its claim commit. A person deletes a
  claim of their own user. The owner deletes each claim. It refuses a
  claim of another user for a member. Superseded in part by Wave C
  (01M51R8G59KA4WFCD7CBAYP5DN).
- **01M51BFYW85YBEWAB20YTCJQFK** Until Wave C, `riff claim ITEM` and
  `riff release ITEM` act on riff-server 1.x, as `riff --help` says
  today. Only `riff release ITEM --forge` acts on a claim of riff 3.0
  (01M519Z4PESWFV8Y03SKTSGV4Q). A person makes no claim of riff 3.0 at
  the CLI: the daemon makes each one. Superseded by Wave C
  (01M51R8G06XMJF3GAP7MFW06R0).
- **01M519Z4RWHWP3TKNP4T5GKKE9** `riff run` and the waves of the daemon
  do each riff step with the daemon, and with no riff-server: the
  daemon claims the item on the forge, the session talks through the
  state of the daemon, and at the end the daemon sends the verify
  request to the lead as a `message` record and releases the item. This
  replaces the steps with riff 1.x of 01M4ZKX0NXN7E33VC3PSFGSPDT and
  01M4ZV3B6KP33YV750DJ89SMFF.
- **01M5232XDH1B3BGX7FXT0A2BRE** The daemon keeps the main clone of each
  repository where it made a claim in the file `forges.json` of its
  folder, by repository. It writes the main clone there before the push
  of the claim. The release at its start
  (01M519Z4HK333JWVXAWQHCGK6M) and the step of the lead
  (01M519Z4M0F2RY0VDQNB2QFAPJ) read the claims on the forge of each main
  clone of that file and of each wave of the daemon. The release at its
  start deletes only a claim with a `Riff-Time` before the second of its
  start: a claim of this run of the daemon stays, for example the claim
  of a job of a wave before its session runs.
- **01M5232XGM1GYENFCWBQ5TFYBN** The daemon of a lead looks at the
  claims of its user each minute. A claim of a session with no `session`
  record counts as a claim of a session that runs. When the daemon of
  the lead had no link to the daemon of the claim since its own start,
  the 15 minutes count from that start.
- **01M5232XKPG96CTQ7HSAM4726S** `riff release ITEM --forge` asks the
  daemon of the machine to delete the claim on the forge of the main
  clone of the working folder. The daemon decides with its riff of
  peers (01M519Z4PESWFV8Y03SKTSGV4Q). A ref whose commit is not a claim
  commit of riff names no user: only the owner deletes it. Superseded
  in part by Wave C (01M51R8G59KA4WFCD7CBAYP5DN).
- **01M526Z1Q3QM3X0C7N6MCX8Z6T** `riff run ITEM --peers` and
  `riff wave run --peers` do the steps of 01M519Z4RWHWP3TKNP4T5GKKE9.
  With no `--peers`, `riff run` and `riff wave run` use riff-server 1.x
  until Wave C (01M519Z506XWTSV1CJ6CCVFXEF). A wave with no riff-server
  keeps an empty riff-server, and the daemon picks the world of each
  wave and of each session by it. Superseded in part by Wave C
  (01M51R8G59KA4WFCD7CBAYP5DN).
- **01M526Z1SVG3ZK38GH51AHCRHP** `riff run ITEM --peers` does the checks
  before the claim, then asks the daemon to do the build of ITEM as a
  job of a wave, in the room of the host. It shows each line, each event
  and the end of the session, then a line for each step after the end.
  A SIGINT or a SIGTERM asks the daemon to stop the container. The
  daemon does the steps after the end in each case, so `riff run` has
  no end step of its own.
- **01M526Z1WH3865PKGFPYPFWQT7** A job with no riff-server does its
  steps after the end in this order: the verify request, the note or
  the result of a verify, as a `message` record to the lead; then the
  release of its claim on the forge. The end of the session writes the
  end record of the session, and releases no claim of a job.
- **01M526Z1Z80JFAS0GS04PVF1KG** The steps after the end of a job with
  no riff-server wait for the riff of the daemon: for example a job
  whose session ended while no daemon ran, after a new start of the
  daemon that waits for its keyring.
- **01M5274Z2NQEKN6B4H7V95KKW0** On the riff of peers, a step of a lead
  (`hold`, `free`, `pause`, `resume`) needs the lead, or a session of
  the daemon of the owner that is not a worker. A worker of the daemon
  of the owner is refused, as each worker (01M519Z3FTAZ20DQKDTW09M6NS).
- **01M527NXZ6KRFAA641ZKPVGC4V** Each git call of a claim, a release
  and a read of the claims runs with `GIT_TERMINAL_PROMPT=0`: git asks
  for no sign-in, and a push or a fetch with no sign-in fails at once.
- **01M526Z224A1CVPJQE8W7ERXV6** A session with no riff-server has no
  riff-server in the environment of its auto mode
  (01M516HETNZCBFBJNHT8BKFB1Y).
- **01M5232XPKES7R8P3F3H8X6V4B** A claim reads as held only when the
  forge refuses its ref for a failed lease: `stale info`, `cannot lock
  ref`, a ref that is there already, or `failed to update ref`. Each
  other refusal and each fault of git is an error, and riff claims
  nothing. A push of a claim or of a release runs no pre-push hook of
  the clone (`git push --no-verify`).

### Wave B: the scope and the way over

- **01M519Z4V991WXHQ9WRANDF63N** One riff for each owner. It holds each
  repository that a session of a member works in: one thread for each
  repository, and the thread `chat`. Each member reads each thread.
- **01M519Z4XPS0AE026ZC3CPATP5** riff-server 1.x runs on until Wave C. A
  daemon of 3.0 does not talk to it, and riff-server does not see the
  riff of 3.0. A session of 1.x and a session of 3.0 do not see the
  messages or the claims of each other. Superseded by Wave C
  (01M51R8H8W8ZBEBXFKWTK7ZB14).
- **01M519Z506XWTSV1CJ6CCVFXEF** The work of one repository runs on the
  riff of 1.x or on the riff of 3.0, not on both at one time. riff does
  not check it. Mike decides when a repository moves. Superseded by
  Wave C (01M51R8GQ8AKGGEEBYPS1ED8RQ).
- **01M519Z52NHWATA0D61N8A2C3V** iroh encrypts each connection between
  two daemons. The relays of n0 see the endpoint IDs, and no record.
  Each member reads each record of the riff, so a record holds no
  secret (01M4Z60B8KDXD190RWC7D184Q3).
- **01M519Z554SC3EAT69Q3M6JDAP** `riff view` and `riff top` show each
  session of each daemon of the riff: the user, the host, the item, the
  state and the status. The sessions of a daemon that is offline show
  as offline, with the time when the daemon was last seen. A takeover
  works only for a session of this machine.
- **01M522MRYR89SY6WRV720806RA** A daemon of the riff is online while
  this daemon has a peer link to it. Else it was last seen at the later
  of two times: the end of its last link since the start of this
  daemon, and the time of the author of its last record. `riff view` and
  `riff top` take the sessions of the riff of peers from the status of
  the daemon of this machine. `riff top` adds them to the sessions of
  riff-server. With no daemon, it shows the sessions of riff-server
  only. A takeover of a session of another daemon, in `riff view` or
  with `riff takeover` and its session ID, refuses with a text that
  names the host of that session. Superseded in part by Wave C
  (01M51R8G7R46NC07YVWYJ211NM).
- **01M519Z57K3SPRN8Z517VZ62BB** The book page "riff 3.0: peers" (#803)
  has the picture of a riff of peers, the steps to share a riff, and a
  mermaid diagram of the sync, of a join and of a claim. Each item of
  Wave B adds its how-to to it. Superseded in part by the re-cut
  (01M53NGWJ48MMYPQNJQ0EY9T47).

### Wave C: what goes and what stays

The requirements of Wave C (#879) say how riff 3.0 ends riff-server and
the cloud parts of riff. The book has no page for this plan: its items
wait in the Backlog (#881 to #889). A requirement of Wave C holds when
the item of Wave C that builds it merges, or when its step is done. A
requirement that ends with "Superseded by Wave C (ID)." holds until
then. The re-cut puts the items of Wave C that end riff-server in the
Backlog (01M53NGWN190T2MYB1EZAD36VB).

- **01M51R8FXQFXWHRD374577HZ3H** Wave C removes riff-server and each
  part that exists only for it: the crate and the binary `riff-server`;
  `riff cloud`; the jobs `Deploy` and `Stage` of CI; the files
  `deploy/alert.json` and `deploy/lifecycle.json`, the `Dockerfile` of
  riff-server, and at the end the settings files `deploy/cloud`
  (01M51TD2ZCA717T5BWB72QTBTK); the sign-in of a person with OpenID
  Connect and its OAuth client; the design pages of riff-server
  (`design-storage.md`, `design-engine.md`, `design-link.md` and
  `design-plan.md`); and, in the Google Cloud project `como-riff`, the
  Cloud Run services, the buckets, the service accounts, the Secret
  Manager entries, the image repository, the sign-in of GitHub Actions
  and the alert of each instance. Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8G06XMJF3GAP7MFW06R0** Wave C removes each part of `riff` that
  acts only on riff-server, with its tests and its book text:
  `riff login`, `riff logout`, `riff server`, the flag `--server` and
  `RIFF_SERVER`, the device key, `riff claim`, the riff-server form of
  `riff release`, the member commands with `--email`, `riff admin`,
  `riff owner`, `riff step`, `riff audit`, `riff move` of the bridge
  release, and the shell forms of the session tools: `riff who`,
  `riff whoami`, `riff read`, `riff post`, `riff tell`, `riff status`,
  `riff blocked`, and `riff lead` with no subcommand. The pages
  `start-a-team-riff.md` and `join-a-riff.md` go. Superseded by the
  re-cut (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8G2RZAVZN1432EGZSW0X** Wave C removes the parts of riff 1.x
  for a session on the host, with their tests and their book text: the
  plugin of Claude Code with its skill and its hooks, `riff connect`,
  `riff enable`, `riff disable`, `riff mcp`, `riff watch`, the status
  line, the workers in tmux (`riff workers`, the rollout, the clear, the
  reap, the pool of build jobs and the monitor), `riff worktrees`,
  `riff lead compact` and `riff lead blocked`. Each session of riff 3.0
  runs in a container of the daemon (01M4Z60B7NW4GHKGCYZZDG26R6). The
  user place of the permission rules goes. The project place
  (`riff setup`) and the session place stay. Superseded by the re-cut
  (01M53NGVMD2PGEC0P9S3X4CTHT, 01M53NGVQDKCTNMG54YDFE22T2,
  01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8G59KA4WFCD7CBAYP5DN** After Wave C, riff has these commands:
  `riff init`, `riff daemon`, `riff run`, `riff wave`,
  `riff lead start`, `riff view`, `riff takeover`, `riff image`,
  `riff update`, `riff setup`, `riff pr`, `riff verify`, `riff usage`,
  the hidden `riff relay`, the peer commands of Wave B (`riff invite`,
  `riff join`, `riff leave`, `riff remove`, `riff members` and
  `riff release`), and the commands of a person of
  01M51R8G7R46NC07YVWYJ211NM. `riff release ITEM` acts on the claim on
  the forge (01M519Z4PESWFV8Y03SKTSGV4Q), with no flag `--forge`.
  `riff run` and `riff wave run` do the steps of
  01M519Z4RWHWP3TKNP4T5GKKE9, with no flag `--peers`. The
  crates `riff`, `riff-core`, `riff-state`, `hygiene`, `reqs`,
  `real-session` and `isolated` stay. Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8G7R46NC07YVWYJ211NM** After Wave C, a person at the CLI has
  `riff top`, `riff view`, `riff chat`, `riff tail`, `riff pause`,
  `riff resume`, `riff plan hold` and `riff plan free`. Each one acts on
  the daemon of this machine and its peers, with no riff-server. Only
  the owner pauses and resumes the whole riff
  (01M519Z3J7Z0M3PNDY249Q57FH). Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8GAE5E6ZKD4PSNJYGGMW** After Wave C, `riff-state` serves only
  the daemon. The parts of `riff-core` that only the HTTP API of
  riff-server uses go: its wire types, DPoP and the signed messages of
  the device key. The format fixtures of riff-server go with it. The
  test environment `isolated` starts no riff-server. Superseded by the
  re-cut (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8GD2EY0A0GF2X82PMA5B** Each item of Wave C that removes a part
  marks each requirement of that part that has no mark yet with
  "Superseded by Wave C (ID).", where ID is the requirement of Wave C
  that removes it. No requirement is deleted or renumbered.
- **01M51R8GFNC8Y3PY290CPZ2SY0** A release of riff 3 has the files
  `riff-x86_64-unknown-linux-musl` and `SHA256SUMS`, and no
  riff-server. The job "Static riff" builds and checks only `riff`.
  `riff update` installs only `riff` from such a release. Superseded by
  the re-cut (01M53NGVB7SZJW7X8SYS3B0ZHG). A release of riff 3 has
  riff-server.

### Wave C: the order

- **01M51R8GJ7XTR40J95F99DSQVN** Wave C has five steps, in this order:
  the bridge release (01M51R8GMRND1SFCYZSM53KS5M), the move of each
  repository (01M51R8GQ8AKGGEEBYPS1ED8RQ), the stop of riff-server
  (01M51R8H8W8ZBEBXFKWTK7ZB14), the go-back window
  (01M51R8HBB98JQ6QG6RFZWBY74), and the end
  (01M51R8HK0VPBD4W6CKACWC1WW). Each step starts only when the step
  before it is done. The items of Wave C that remove code merge on
  `main` after the stop. No release of riff 3 comes before the end.
  Superseded by the re-cut (01M53NGS8FGRNY12DDJA3P0GH2,
  01M53NGVTE186GE5E48E68N0BY).
- **01M51R8GMRND1SFCYZSM53KS5M** The bridge release is the last minor
  release of riff 1.x. It has each item of Wave B, and the items of Wave
  C that the move needs: the update with no riff-server
  (01M51R8HTE7X1Q5NKZAS16E9PD, 01M51R8HZF5KQ28PBJA2DZZYKJ),
  `riff move` and `riff cloud delete --all`. riff-server runs it. Before
  the first item of Wave C that removes code merges, an admin points the
  branch `release/1.x` at the tag of the bridge release. A patch of riff
  1.x then goes on `release/1.x` (01M4YJ6KGBPTCSVMFQCDKACZPV).
  Superseded by the re-cut (01M53NGVTE186GE5E48E68N0BY).
- **01M51R8GQ8AKGGEEBYPS1ED8RQ** riff 1.x and riff 3.0 run side by side
  from the bridge release to the stop of riff-server. The work of one
  repository runs on one of them at a time, and riff does not check it.
  A repository moves only after the live check of Wave B (#846) passed.
  The repositories move one at a time, in this order:
  `como-technologies/riff`, then `n8behavior/dotfiles`, then
  `como-technologies/strata`. Mike decides when each of his
  repositories moves. Mike and Brett decide together when
  `como-technologies/strata` moves. Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).

### Wave C: the move of a repository

- **01M51R8GSP3ZAJKGCY0KYGQYXT** `riff move` in a clone moves the work
  of its repository from riff-server to riff 3.0. It is a command of a
  person at the CLI, not the `move` tool of a session. First it checks,
  and a failed check changes nothing: the riff daemon of the machine
  answers; `riff.toml` of the repository pins a session image
  (01M4Z7SP3KSKJYKNRP1MQ70C4Q); riff-server answers and the person is
  signed in; no session holds a claim in the repository on riff-server;
  and no worker of riff 1.x runs for the repository. Each failed check
  names its fix. Superseded by the re-cut (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8GW67GFMFZ6S2MAAV6AX** After its checks, `riff move` pauses
  the repository on riff-server. It copies each hold of the repository
  on riff-server into a hold of the daemon, with the same item and
  reason (01M519Z3J7Z0M3PNDY249Q57FH). It turns riff 1.x off in the
  repository, as `riff disable` does. It posts a note to the thread of
  the repository on riff-server and on the daemon: who moved it, when,
  and that `riff move --back` undoes it. Then it names the next step:
  `riff lead start`. Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8GYRF329RKH4F7HP1TAA** `riff move` copies the memory of the
  lead of riff 1.x into the Claude folder of the lead of riff 3.0, when
  that folder has no memory yet. The memory of the lead of riff 1.x is
  the folder `memory` of the project of the main clone, in the Claude
  folder of the person. riff reads the Claude folder of the person, and
  writes nothing there (01M4Z60B85X62PNYSVJWKXEWH3). Superseded by the
  re-cut (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8H1BXVQ5D8SRTFFQBWDQ** `riff move --back` undoes a move while
  riff-server runs. It stops the wave of the daemon and the lead of
  riff 3.0 of the repository. It resumes the repository on riff-server,
  removes the entry that `riff move` wrote to turn riff 1.x off, and
  posts the note on both threads. The holds of the daemon stay.
  Superseded by the re-cut (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8H6CD0C49GQW46QVK2VH** The state of riff-server does not move
  to riff 3.0, except the holds that `riff move` copies. riff 3.0 starts
  with no message, no thread and no claim of riff-server. A repository
  moves with no open claim on riff-server. The sign-ins, the tokens and
  the device keys of riff-server go with it. Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).

### Wave C: the end of riff-server

- **01M51TD2ZCA717T5BWB72QTBTK** The owner runs each `riff cloud` step
  of Wave C with the riff of the bridge release, in a clone of
  `como-technologies/riff`: the stop, the go-back and the delete of the
  cloud parts. `riff cloud` reads the settings file of each instance
  from `deploy/cloud` of that clone (R144). So `deploy/cloud` stays on
  `main` until the end: the item that removes riff-server keeps it.
  After `riff cloud delete NAME --all` of each instance, the live check
  of the end removes `deploy/cloud` from `main`, before the release of
  riff 3.0.0. Superseded by the re-cut (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8H8W8ZBEBXFKWTK7ZB14** The owner stops riff-server with
  `riff cloud delete shared`: the service goes, and the bucket and each
  other part stay. The owner stops it only when each repository of
  riff-server with work moved, and no session of riff 1.x is live.
  Superseded by the re-cut (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8HBB98JQ6QG6RFZWBY74** For 7 days after the stop, each cloud
  part stays: the go-back window. In that time, the owner brings
  riff-server back with its state with `riff cloud deploy shared TAG`,
  where TAG is the tag of the bridge release, and the email of the
  owner is in `RIFF_OWNER` (01M3JN3ASSV9SA0QZKXXJ0RTEV). Then
  `riff move --back` in a clone moves a repository back. Superseded by
  the re-cut (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8HDY6BVG735NDKDM39Y5** `riff cloud delete NAME --all` deletes
  each part of the instance NAME: the service and its domain mapping,
  the alert and its channel, the service accounts, the secret, the
  bucket, the GitHub environment of the deploy, and, for the instance
  `shared`, the repository variables `CLOUD_DEPLOY` and `RIFF_OWNER`.
  The image repository and the sign-in of GitHub Actions go when no
  other instance of the settings folder uses them. It asks for the name
  first, as `riff cloud delete` does. It says what it deleted and what
  was gone already, so a second run deletes nothing. It keeps the
  project and the settings file. With `--project`, it then deletes the
  Google Cloud project, after the person types its ID. Google keeps a
  deleted project for 30 days. Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8HGD8TV6BMHNZ3QCXFWZ** Before `riff cloud delete --all`
  deletes a bucket, it writes each record of the log of the instance as
  text, as `riff-server log` prints them, into the file
  `archive/NAME-log-DATE.txt` in the data folder of riff, with mode
  0600. It writes no token and no secret. riff never reads the file: it
  is for people. Superseded by the re-cut (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8HK0VPBD4W6CKACWC1WW** The end comes after the go-back window.
  The owner runs `riff cloud delete NAME --all` for each instance that
  `riff cloud list` shows, with the riff of the bridge release. Then
  riff 3.0.0 releases (01M51R8J200TMFS4ANDN9ZE6TB), and the line 1.x
  retires (01M51R8J4G95J1HPG16SRG25JY). Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8HNF5GDNAHMEG7W2Q273** The GitHub App `riff-como-technologies`
  of riff 2.0 goes at the start of Wave C. No part of riff uses it since
  riff 2.x retired (01M4Z60B8N9W9TBE7KKDJQAN2C). The owner of the GitHub
  organization deletes it in the settings of the organization: only the
  App itself can delete it with the API of GitHub. Superseded in part
  by the re-cut (01M53NGWN190T2MYB1EZAD36VB): Wave C waits in the
  Backlog, so the owner of the organization deletes the App at any
  time.
- **01M51R8HQZJV6PFTAMACVDWDBR** Each step of Wave C has a way back
  until the end. A release goes back with `riff update --tag` of the
  release before it. A move goes back with `riff move --back`. The stop
  goes back in the go-back window (01M51R8HBB98JQ6QG6RFZWBY74). After
  the end, riff 1.x does not come back: a fault of riff 3.0 gets a fix
  in a release of riff 3. Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).

### Wave C: the update and the line 1.x

- **01M51R8HTE7X1Q5NKZAS16E9PD** With `update.auto` on, the daemon looks
  at the release that GitHub marks Latest at its start and each hour.
  When it is a release of riff 3 or later, and newer than the riff of
  the daemon, the daemon installs it as `riff update` does, and starts
  again after its last session (01M517BTP6BAGTRBYWPX9GPVE0). A release
  of riff 1.x still comes only from riff-server
  (01M510NWBNC20GXFRMHPW12TW9). Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8HWXNCSXEWR95T0HHJ5B** After Wave C, `riff update` with no
  `--tag` installs the release that GitHub marks Latest. riff never
  installs an older release by itself. Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8HZF5KQ28PBJA2DZZYKJ** When `riff update`, by a person or by
  itself, installs a release of riff 3 or later over a riff of 1.x, it
  first removes the parts of riff 1.x that `riff connect`,
  `riff enable` and `riff init` wrote in the Claude config of the
  person: the plugin of riff and its marketplace, each `enabledPlugins`
  entry of riff, the status line of riff, and each allow rule of the
  user place of riff 1.x. It keeps each other entry and key in its
  order, and says what it removed. The setting `update.auto` is the
  consent of the person for this step (#855). So no hook of riff 1.x
  runs after the update. The riff 1.x that runs the update does this
  step. riff 3 writes nothing in the Claude config of the person
  (01M4Z60B85X62PNYSVJWKXEWH3). Superseded by the re-cut
  (01M53NGWN190T2MYB1EZAD36VB).
- **01M51R8J200TMFS4ANDN9ZE6TB** riff 3.0.0 releases at the end of Wave
  C, as the release that GitHub marks Latest. Each machine with
  `update.auto` on takes it by itself (01M51R8HTE7X1Q5NKZAS16E9PD). On
  each other machine, the person runs `riff update` one time. Superseded
  by the re-cut (01M53NGVTE186GE5E48E68N0BY).
- **01M51R8J4G95J1HPG16SRG25JY** At the end of Wave C, the line 1.x
  retires: each GitHub release of riff 1.x becomes a prerelease with a
  first line that says that the line is retired (#795). No patch of 1.x
  comes after that. The branch `release/1.x` stays as history.
  Superseded by the re-cut (01M53NGVTE186GE5E48E68N0BY).
- **01M51R8J6ZRE60NYEJX9KS2XWD** The book page "riff 3.0: no cloud"
  (#879) says what goes and what stays, the order of the steps with a
  mermaid diagram, how a person moves a repository, the end of
  riff-server, and how to go back. Each item of Wave C adds its how-to
  to it. Superseded by the re-cut (01M53NGWJ48MMYPQNJQ0EY9T47).

### The security of riff's own code

- **01M4Z60B8K4C6DHFAKNCAWR91S** The daemon checks each peer and each
  record: iroh checks the node key, each record carries the signature of
  its author, and the daemon checks the size and the type of each
  message before it applies it. Superseded by the re-cut
  (01M53NGW3FR555TWP8Z8HHRVDJ).

### Process

- **01M4Z60B8MRBB8WTCVNE0G22JG** riff 1.3 runs the riff while we build
  3.0. It stays up, and nothing forces it off. Its patches go on
  `release/1.x` (01M4YJ6KGBPTCSVMFQCDKACZPV). If 1.3 does not hold, we
  build 3.0 with no riff until 3.0 runs its own work. Superseded by Wave
  C (01M51R8GJ7XTR40J95F99DSQVN).
- **01M4Z60B8N9W9TBE7KKDJQAN2C** riff 2.x is retired. `main` went back
  to the line 1.x (#705), and 3.0 builds on 1.3.1. 2.x stays only in the
  git history and in the tags `v2.0.0` and `v2.0.1`. A part of 2.0 that
  3.0 wants comes back by a cherry-pick.
- **01M4Z60B8R57HG5Z0BA8XM3EM8** Each requirement of the server, the
  cloud, the sign-in, tmux, the workers and their clear, the tidy of
  worktrees and the pool of build jobs ends with "Superseded by 3.0
  (ID)." It holds for riff 1.x until riff 3.0 replaces it. No
  requirement is deleted or renumbered. The requirements of the GitHub
  App and of the 2.0 sandbox left `main` with 2.x
  (01M4Z60B8N9W9TBE7KKDJQAN2C). Superseded in part by the re-cut
  (01M53NGVB7SZJW7X8SYS3B0ZHG).
- **01M4Z60B8RES9JSQDHNBFVR8AH** After 3.0 replaces 1.3, the Cloud Run
  service, the GCS bucket, the Secret Manager entries and the GitHub App
  of riff go. Mike decides when. Superseded by Wave C
  (01M51R8HK0VPBD4W6CKACWC1WW).

### The release cycle

- **01M58KMMHH70JEMBX3EYT9H3WM** The watch of the lead looks at the
  release cycle each 5 minutes, in a clone with
  `deploy/release-check.sh`. Each new state of the current release
  wakes the lead with the state and the next step. The first look
  after a start only learns the state. Superseded by
  01M5RGCW5NCKQYA61RM63914TX and 01M5RGD0RTXYA7FF5K1TH72MGX.

## The UIs

- **01M54217ET3G7R4ZJ33CQRE5P9** riff 3.0 (#953) comes first, with the
  patch releases that make it stable. riff 3.1 comes after a stable
  3.0. It moves the core library out of
  the CLI in small steps. Each step keeps each test and changes no
  command. The UIs come after riff 3.1.
