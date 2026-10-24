# Requirements

## Product

- **R1** Agent sessions of different people can find each other, send
  messages and claim work.
- **R2** The core does not block any agent tool that supports MCP. Other
  tools can join later.
- **R3** A feature of one agent tool is an optional adapter, never the
  core. Infrastructure is always Google.
- **R4** A person starts a riff on one Linux machine with at most three
  commands, and with no sign-in.
- **01M3MN2R92DA7QPP80G1AENX4M** The book page "Start a Riff" starts
  with one question: did a person give you a riff address? Yes leads to
  "Join a Riff". No leads to one of two paths: "Just this machine",
  with no sign-in, or "Start a Team Riff". The page names no
  `--insecure`, no systemd and no `riff-server install`.
- **R203** A person adds a second machine to the riff of a first machine
  on a network that they trust. The riff of the first machine takes
  connections from the network. The second machine names that riff with
  `RIFF_SERVER`. The riff of the first machine has sign-in with the
  OAuth client of the person, and the person signs in on each machine
  (01M3JZN229S3YA3BR6GN5H3MTY). Superseded by 3.0
  (01M4Z60B8HN1C1XAG1CA1YS36H).
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
- **R166** A session picks an open work item of the current wave
  (R214) that no session holds. It takes work only from the current
  wave (R216). It never picks an item whose needs are open (R215).
  When the current wave has no free item, the session verifies,
  runs its checks after the release, or waits. It picks the
  item that it thinks is best. It does not wait for a plan or for
  permission. The order of the items in a wave does not matter. A
  scope from the user of the session wins. A scope message from
  another session is advice (R10).
- **R172** Each issue has acceptance criteria: a `Done when:` line.
  Each criterion names what to run or look at, and what the result
  must be. The criteria follow ASD-STE100.
- **01M3PRHM97CJ16FZD9ZK48ERAD** A session writes all prose in
  ASD-STE100: each message, status, issue, pull request, commit,
  release note, doc and answer to its user. This is rule 1 of the
  skill. The level line of a release uses plain words that a person
  understands.
- **01M58BCCKMNG6V495YF8AM9P5C** A session uses natural tenses for
  things in time: the future with "will" for a future action, and the
  past for a past event. It never writes a future action in the
  present tense. Rule 1 of the skill and `CLAUDE.md` say so.
- **R173** After a session claims an issue, it reads the issue. When the
  `Done when:` line is missing or cannot be tested, the session does
  not start work. It writes the criteria, adds them to the issue, posts
  to the repository thread, releases the claim and picks a different
  item.
- **R174** A session that writes the criteria for an issue does not
  implement that issue in the same claim. The next session that claims
  the issue reviews the criteria.
- **R188** A session never verifies its own work. Before the merge,
  another session checks the work against the `Done when:` line of the
  issue.
- **R189** When the author finishes, the checks of the repository
  pass. The author pushes its branch, opens a pull request with
  auto-merge on (01M3JFEXMPNFEV4HBZJQ15JD25), and posts a verify
  request to the repository thread. The request names the issue, the
  pull request and the commit. Its `to` list wakes the sessions of the
  repository.
- **R190** A verify request is free work (R166). The verifier claims
  `verify-ITEM`, for example `verify-issue-12`, so that only one
  session verifies. It skips a request whose issue is closed, or whose
  commit has a result.
- **R191** The verifier checks out the commit in a worktree of its own
  and tests each criterion. It does not change the code.
- **01M5174ASF376JDNBQ4423TTAR** A control shows that a test finds a
  fault: a test of the work with no fix, or a guard test with a fault
  in the code. The verifier never edits the code for a control, also
  not for a moment. It runs a control at the base commit of the pull
  request, in its verify worktree, and then checks out the commit of
  the pull request again. When the base commit cannot show the fault,
  it reads and runs a test of the guard on a sample, uses the control
  of the author on the issue, or checks by a read. The result says
  which way it used. The skill says so.
- **R192** The verifier posts the result to the author with the
  selector `claim=ITEM`. A pass names each criterion and how the
  verifier checked it. A fail names each criterion that failed and the
  steps to see the failure. The verifier also puts the result on the
  pull request and sets the verify status of the commit
  (01M3JFEXPXRTXYHCV0WSKEK07M). Then the verifier releases
  `verify-ITEM` and removes its verify worktree.
- **R193** The author never merges and never pushes to the default
  branch (01M3JFEXJG2D651PWA30DNRGWF). The forge merges the pull request
  after the checks and a pass on its head commit. An author that is
  not a worker keeps its claim and waits (01M3Z9N6NFNMW62JZ796RV4643),
  also when no session takes the request. It does not verify
  while it waits (01M3K0FZ5M08Z4YPSVKFADCAKC). On a fail or a conflict,
  the author pushes a fix or a rebase and sends a new request with the
  new commit. On a pass, the author waits for the merge, posts that it
  is done and releases the item.
- **R194** A criterion that only the shared riff can test is a check
  after the release. It does not stop a pass. The verifier names it in
  the result. The pull
  request links the issue so that the merge leaves it open, and the
  issue stays open until that check passes. After the merge, the
  session that does the steps after the merge adds a comment to the
  issue: `Merged in #PR (COMMIT)`, and the check that is left.
- **01M3Z9N6AK6W9KCA1MN72X78B6** One context holds one item. The work
  of a worker on an item ends at its verify request. After the request,
  the worker writes the state on the issue as a comment: the pull
  request, the commit, what is left after the merge, and what a session
  must know when the verify fails. Then it releases the item and ends
  its turn. It does not wait for the verify, and it starts no second
  item in that context. The release is its last release, so the worker
  must clear its context (01M3X9XAK1KPZZVM1AJR2H8DSS). The skill and
  the start hook say so. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M3Z9N6GQK0NYCMGQ66FW406V** On a pass, when no session holds the
  item, the verifier does the steps after the merge: it waits for the
  merge, adds the `Merged in` comment when a check after the release is
  left, posts the done note, and removes the worktree and the branch of
  the item on its machine. It releases `verify-ITEM` after the note. On
  a fail, it releases at once. The item is then free with its earlier
  work (01M3WFYEP1H3VPW8G90KQDE6FW): the next session that claims it
  reads the result, goes on from the branch, and sends a new verify
  request for the same pull request. The skill says so.
- **01M3Z9N6NFNMW62JZ796RV4643** A session that is not a worker keeps
  its claim after its verify request, and does the steps after the
  merge itself (R193). The skill says so.
- **01M3Z9MY0CDBB1G749XBVMVV8X** riff makes the state of the pull
  request of an item from the status `riff/verify` of its head commit.
  The pull request is open, is not a draft, and its branch names the
  issue. With no status, it waits for a verify. With `success`, it
  waits for the merge. With `failure` or `error`, the verify failed.
  Superseded in part by 01M51BF5S2YKB8JG10DF6YVT42.
- **01M3Z9N6SPWPSSCBEVDCKBESSV** The answer to a granted claim of
  `issue-N` names the open pull request of the item and the state of
  its verify (01M3Z9MY0CDBB1G749XBVMVV8X), from `riff claim` and from
  the `claim` tool. For a failed verify, the line names the commit and
  the URL of the result, and tells the session to go on from the
  branch. For a pull request that waits for a verify or for the merge,
  the line tells the session to release the item. riff asks `gh` only
  when the clone knows a pushed branch of the item, and waits at most 5
  seconds. With no `gh`, the claim has no such line. A verify claim
  gets no such line.
- **01M3Z9N70J4H79VJN4ZKKH3G6S** `riff verify` posts the result to the
  holder of the issue (01M3NB6FYXXKX80VHEVA5CV6RY). When no session
  holds the issue in the repository, the post also wakes the lead of
  the user of the verifier in that repository.
- **01M3W62QG36F9RD4SZ1X508T3A** A verify result holds only the check
  of the item against its `Done when:` line. No public text on the
  forge holds a live security fault: a security fault in the code of
  the default branch, or in a server that runs. The public text is a
  verify result, the body of a pull request, a comment on a pull
  request, an issue, a comment on an issue and a commit message. A post
  to a thread holds no live security fault too. The session tells the
  fault to the lead with `tell`. The lead decides on a private
  advisory.
- **01M3MRDEXSMFT0STXF2HAR2QZ6** A live check of new code, for example
  a new plugin command, hook or skill text, runs in a dev session
  (`just dev`) before the verify. It needs no release and no update of
  the machine. A check after the release runs after the release of the
  wave. Superseded by 4.0 (#1173).
- **01M3K0FZ5M08Z4YPSVKFADCAKC** Only a session that holds no claim
  takes a verify request. A session that holds an item, also one that
  waits for its own verify, does not verify. The lead gives a verify
  request to a session with no claim. When none is free, the lead
  starts a worker for it.
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
- **R71** A session talks to other sessions only through riff. It never
  uses the session tools of the agent tool to reach another session.
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
- **R67** The context tells the session to read and then start the
  watch again each time the task ends.
- **R68** The context tells the session to keep the watch that runs
  for it. It tells the session to start a watch only when none runs
  (R169).
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
- **R171** A session starts the watch again as its next action after
  the task ends, also in the middle of a turn.
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
- **01M3MYQ299XKJE9X9FHWZ7JFM4** At a new start in a linked worktree
  where another live session works, the start context names that
  session and the main worktree. It tells the session not to follow
  the start routine, to claim nothing, to change no file there, and to
  ask its user, through the lead, to start it again in the main
  worktree.
- **01M3MYQ2BFKS3KJ8DWNWDJKWB9** At a new start in a linked worktree
  where no other live session works, the start context names the
  worktree and the main worktree, and points to "Pick up dropped
  work". In the main worktree, the context has no worktree line.
- **01M3JN21YJSP7HM1JPS1TCFM9W** The update of each machine pulls each
  clone of the project with `git pull --ff-only`.
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
- **01M3XY2T2YEV7GT7DKJHSMMHYR** In a linked worktree, riff also reads
  the local settings and the project settings of the main clone. The
  local settings come before the project settings.
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
- **01M3XY2SWEK0N8MC3MY4TMYTD3** `RIFF_ON=1` turns riff on for the
  processes that have it, also outside a repository. `just dev` and
  the helper crate `isolated` set it. A worker that such a process
  starts gets `RIFF_ON=1`.
- **01M510NWEFSN0NWQH8KQ6FA7DC** `RIFF_DYNAMIC=1` lets `riff init` and
  the riff daemon take a riff that is not static. Only the helper crate
  `isolated` sets it: in a test, a fake `podman` runs `riff relay` on
  the host.
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
- **01M3NJDSQ23FFRMH8ZD4GC57WY** `riff --help` lists the commands that
  people use under the headings Get started, Work in the riff, Pull
  requests, Lead and Members, in that order. Each command has one
  short line: a phrase of at most 60 characters with no period.
  `riff help CMD` shows the long text. The plumbing that the plugin
  runs (`hook`, `mcp`, `statusline`, `watch`, `workers run`) is
  hidden, and `riff help CMD` still shows it. The help of `riff` and
  `riff-server` wraps at 80 columns and shows no value of an
  environment variable.
- **01M3NT228WA11PGNWDJ0WP7PQD** The help of `--server` on each
  command is one short line of at most 60 characters.
  `riff help server` shows the forms of a value and the default. `riff`
  with no argument shows the help of `riff --help`. `riff` with options
  and no command says to name a command. Neither names a hidden
  command.

## Waves

- **R213** A wave is a numbered group of work items: Wave 1, Wave 2,
  and so on. The waves run in number order. The items of one wave run
  at the same time.
- **R214** The current wave is the open wave with the lowest number.
  The next wave is the open wave after it. When a repository has no
  waves, each open item is in the current wave. An item that the lead
  keeps out of the waves is not free work.
- **R215** An item names the items that it needs in a `Needs:` line. An
  item is merged when it is closed, or when it has the comment
  `Merged in #PR (COMMIT)` (R194). An item is closed when it is merged
  and each check after the release passed.
- **R216** A wave is done when each of its items is closed. The order in
  a wave: merge each item, stop the workers, make a release, deploy the
  release to the shared server, update each machine, start the
  sessions again, run the checks after the release, close each item. A
  riff with no shared server of its own code skips the release and the
  deploy. Then the lead ends the wave. No session starts an item of the
  next wave before the current wave is done. Superseded in part by
  01M58Q03ZBVVXAQ2WZ8EJ5FWS4.
- **R217** The lead plans the waves. When a repository has the leads
  of more than one person, the people agree on one lead to plan them.
- **R218** A person or a session can add a work item at any time, with
  no wave. A session that adds an item tells its lead. The lead looks
  for items with no wave when it starts and each time a riff line
  wakes it.
- **R219** The lead puts each new item in a wave and writes its
  `Needs:` line. Each item is in a later wave than each of its needs.
  No item blocks or breaks the other work of its wave. When a new item
  fits in no open wave, the lead makes a new wave. Its number is the
  last number plus one.
- **R220** The lead posts each change of the waves to the repository
  thread: the item, its wave, what it needs and what needs it. When a
  wave starts, the lead posts the current wave with its items, the
  next wave, and the conflicts between items.
- **R221** When each item of the current wave is merged, the lead
  stops the workers, asks an admin to make a release and deploy it to
  the shared server, tells its user to update each machine, and tells
  the sessions to run their checks after the release. When each item is
  closed, the lead ends the wave.
- **01M3ZWRC6H0P7EF6CMECCYZ2RC** `riff audit --wave WAVE` checks
  that a wave followed the rules, from the log of the riff and from
  the forge. It changes nothing. It checks seven rules: (1) each item
  of the wave has a claim, a pull request, a verify by another session,
  a merge and a release; (2) each verifier held no other claim and is
  not the author; (3) each worker had a clear between its last release
  and its next claim; (4) no claim while the riff or the repository was
  paused; (5) no claim of an item of a later wave or with an open need;
  (6) the lead held no claim of a work item; (7) each request came from
  the lead of the user of its sender. It prints each rule with pass,
  fail or not checked, and names the records by their position. A
  rule with nothing to check, and a check that the facts cannot prove,
  is not checked, with the reason. It exits with 1 when a rule fails.
- **01M3ZWRCC46EHYKQ4Q2NZS9TEB** Rule 1 of the audit takes a release
  of an item by its holder as early when it comes before the merge and
  before a verify result of the item. A release of a worker after its
  verify request is not early (01M3Z9N6AK6W9KCA1MN72X78B6). The span of
  a wave starts when it is the current wave: at its start, or at the
  end of the last wave before it. It ends at the end of the wave, or
  now. Rule 1 looks at the whole log; the rules 2 to 7 look at the
  records in the span. A wave whose earlier wave is open has no span,
  and the audit refuses it.
- **01M3ZWRCF5H95R4SYX09CWAJS8** `riff audit` checks one wave. It has
  no span of time of its own.
- **01M588N3R2VP12A8X4EASX0BJ4** After the checkpoint rule removed the
  oldest chunks of the log, `POST /v1/log` gives the records from the
  oldest kept chunk, with the position and the time of the oldest kept
  record. `riff audit` prints them. A check that needs a gone record is
  not checked, with the reason: the rules 2 to 7 when the span starts
  before the oldest kept record; rule 1 for the facts of the log of an
  item with no claim in the kept records; rule 1 for a release after a
  verify request by a session whose start is gone; rule 7 for a request
  from a user with no lead in the kept records; rule 2 and rule 3 for a
  claim of a session with no fresh start in the kept records; rule 4
  when the kept records have no pause of the riff or of the repository;
  rule 6 for a claim of a user with no lead in the kept records.
- **01M43GSGB9ZFHSG0Q83Y50FEGW** A lead holds an item of its repository
  with a reason, and frees it again: `riff plan hold ITEM REASON`,
  `riff plan free ITEM`, and the tools `hold` and `free`. The server
  keeps each hold as the record `item_held`, with who held it and
  when, and each end as `item_freed`. A hold of a held item replaces
  its reason. A hold names one item by its exact name. A hold is not a
  claim: it does not end a claim, and only a free ends it. A held item
  is no free work. The lead holds an item with `hold`, not with a
  claim.
- **01M43GSGGY0QMB5D5EH92M6ZFP** Only a lead of the repository thread,
  the owner or an admin can hold and free an item. A worker gets
  `not_allowed`, also a worker of the owner. A hold needs a reason of 1
  to 200 characters. A free of an item with no hold makes no record:
  the trace is a `no_change` line.
- **01M43GSGPJ69TPWPA4935WR8RW** A claim of a held item by a worker is
  refused with the code `on_hold` and the status 409. The reason names
  the lead, the time and the reason of the hold. Each other session
  gets the claim, and the reply has the same text as a warning. The
  checks of a claim run in this order: `must_clear`, the name of the
  item, `paused`, the caller holds the item already, `on_hold`, `held`.
- **01M4A4YTNSJR0R1T9JNXPBSKHC** The server keeps the plan of each
  repository thread: the current wave, its items with their needs, and
  the done needs. The command `plan` writes the full plan as the record
  `plan_set`, and the record replaces the plan of its thread. The
  command `plan_off` writes the record `plan_ended`: the server forgets
  the plan, and the holds stay. A `plan_off` with no plan makes no
  record.
- **01M4A4YTR2NKVBPE6BT9EC3X75** A `plan` names its `base`: the
  position of the `plan_set` record of the plan that the client
  compared with, or none when the repository has no plan. When `base`
  is not that position on the server, the server refuses the command
  with the code `stale_base` and the status 409. The reason names the
  position of the plan of the server, and the client reads that plan
  with the query `plan`. So no old plan replaces a new one. A `plan`
  equal to the plan of the server makes no record, and counts as a
  `plan_seen`.
- **01M4A4YTTB24XNB4G49675QMHT** `plan` checks the form: each item,
  each need and each done need is `issue-N`, no item is twice in the
  items, and the thread is a repository thread. Else the server refuses
  it with `bad_request`.
- **01M4A4YTWK68JDA0DKXX2HV4FA** Each session in the repository thread
  that is not a worker, the owner and each admin can send `plan` and
  `plan_off`. A worker gets `not_allowed`, also a worker of the owner.
- **01M4A4YTYVHFGK0CJVACJQ8DQ3** The signal `plan_seen` names the
  position of the plan that a look saw. When it is the position of the
  plan of the server, and the caller is a session in the repository
  thread that is not a worker, the server keeps the time as the last
  look of the plan. Else the signal changes nothing. The reply is the
  plan of the server.
- **01M4A4Z1NKPDBXV2PRZCG86G6A** The query `plan` gives each member
  the plan of a repository thread: the wave, the items with their
  needs, the done needs, the holder of each item of the plan with a
  claim, the position and the time of the `plan_set` record, the time
  of the last look, and whether the plan is stale. It also gives each
  hold of the thread. The reply of `plan` and of `plan_seen` is the
  same.
- **01M4A4Z1QTHYXZDMCP9DZ39WVT** A plan is stale when no `plan` and no
  `plan_seen` came for `PLAN_TTL` (10 minutes). The time of the last
  look is in memory: after a new start of the server, the count starts
  at the start.
- **R222** The skill, the requirements and the book name the concept
  of waves first. Each keeps the form of a forge in one part of its
  own. A new forge needs no change to the concept. Outside these
  parts, no text names the objects of a forge that hold a wave.
- **01M5NBY42N0ACGG1WAY56ED8CT** A pull request on a branch that names
  no issue and has no trailer `Issue: #N` still counts for the item of
  the line `Closes #N` of its body. The wave starts no build of that
  item.

## Waves on GitHub

- **R223** On GitHub, a wave is a milestone named `Wave N`. A name can
  follow, for example `Wave 5: Cloud`. A work item is an issue in the
  milestone. A milestone with another name is out of the waves.
- **01M3JD8WWMK2ZQTFER4TJFV37V** On GitHub, the backlog is the
  milestone `Backlog`. It is out of the waves. An item in the backlog
  is not free work: no session starts it. Only the lead moves an item
  from the backlog into a wave, when its user schedules the item.
- **R224** On GitHub, an open wave is an open milestone. The lead ends
  a wave: it closes the milestone. It closes a milestone only when the
  milestone has no open issue.
- **R225** The skill gives a command for each step of the waves: see
  the open waves, see the items of a wave, see the items with no wave,
  make a wave, put an item in a wave and end a wave.
  Superseded by 4.0 (#1173).
- **01M509TJRBE2J7HYKKHRJ1ZFR0** `riff wave list` shows the open
  milestones of the repository of the current folder: the waves in
  number order, then each other milestone by its title. Each line has
  the number of the open and the closed issues of the milestone. riff
  reads them with the `gh` of the machine.
- **01M58ATZ408RBCX0PM0D0ZCY4B** `riff wave list` marks the current
  wave, the open wave with the lowest number, with `current`. The line
  of the current wave names each host whose riff daemon runs its loop:
  `runs on HOST`. Under the line of each wave come its open issues in
  the groups of the board of `riff top`: `free`, `held`, `claimed` and
  `verify`. riff reads riff-server with the sign-in of the caller, as
  `riff top` does.
- **01M58ATZ6FA5PJMD3D848CEWBJ** When `riff wave list` cannot read
  riff-server, it shows the milestones with no groups and no hosts, and
  one line on stderr says why.
- **01M509TJTV7TC32EKXW0TEQ5NZ** `riff wave make TITLE` makes a
  milestone. The title is `Wave N`, or `Wave N: NAME`. riff refuses
  each other title, and a number that an open wave has.
- **01M518A0BEQBX9RH9H0CEFYF9Q** `riff wave rename N NAME` gives the
  open milestone of the wave with the number N the title
  `Wave N: NAME`. The number stays. riff refuses a blank name, and a
  number that no open wave has. Then it changes nothing. The skill and
  the page "Waves" give it as the step to rename a wave.
- **01M509TJXE04CTG5D1K751WXJY** `riff wave end N` closes the open
  milestone of the wave with the number N. riff refuses when no open
  wave has the number, and when the milestone has an open issue or an
  open pull request. Then it closes nothing.
- **01M5924014WN63RCHJWGZS4AFZ** `riff wave move ITEM... --to WAVE`
  puts each issue ITEM in the open wave with the number WAVE, or in the
  open milestone Backlog when WAVE is `Backlog`. riff makes the `gh`
  call itself. It refuses an item that a session claims on riff-server,
  and a target that no open milestone has. Then it moves nothing.
- **01M592403Q4TNZEDNR76C7NK9M**
  `riff wave end N --move-open-to-backlog`
  moves each open issue and open pull request of the wave N to the
  Backlog, then ends the wave. It refuses an item that a session claims,
  and then it moves and ends nothing. Without the flag, `riff wave end`
  refuses a wave with an open item.
- **01M592406KTHSXHTWQWA3RN6Q2** `riff init` writes the allow rules of
  the lead for `riff wave move` and `riff wave end`, in a terminal and
  in a container. `text::MCP_LEAD` names these commands for the steps of
  the waves, and not `gh issue edit --milestone`.
- **01M588K2M7G0CV3RNJ4N074FTG** `--wave WAVE` of `riff usage` and
  `riff audit` takes the number of the wave, `N` or `Wave N`, or a full
  title. riff finds the milestone in the open and the closed milestones
  of the repository: the milestone with the same title, else the one
  milestone `Wave N` or `Wave N: NAME`. riff refuses a wave that
  matches no milestone, and names the open waves. It refuses a number
  of more than one milestone, and names them. Then it shows no tokens
  and checks no rule. The skill says that `gh` takes only the full
  title.
- **01M3Q5QE2EWGKVD57Y6YCWA4BR** The rollout of workers
  (01M3Q5QE01DB0FJQJWFKR450KQ) reads its free work with `gh`. The
  current wave is the open milestone `Wave N` with the lowest N. A free
  item is an open issue of it that no session claims, with no comment
  `Merged in #`, and with each issue of its `Needs:` line closed. An
  open need blocks the item, also a need outside the wave. A pull
  request that waits for a verify has a branch that names an issue
  (`worktree-issue-12`, `worktree-issue-12-book`), is open, not a draft,
  has no status `riff/verify` on its head, and no session claims
  `verify-issue-N` for it. Both count as free work. Superseded in part
  by 01M51BF5S2YKB8JG10DF6YVT42.
- **01M3Z9N5HHHS1E17NFGMVBKZ0K** An item counts one time as free work.
  An open issue with no claim whose pull request waits for a verify or
  for the merge (01M3Z9MY0CDBB1G749XBVMVV8X) is no free item: it is
  work for a verify, not for a build. An open issue with no claim whose
  verify failed is a free item.
- **01M3ZWRC9F7TGSHB966TPVVS9Q** On GitHub, `riff audit` reads with
  `gh`: each milestone `Wave N` with its start and its close, each
  issue with its milestone, its `Needs:` line and the time when it was
  merged (its close, or its first comment `Merged in #`, the earlier
  one), each pull request of the milestone with its `Issue:` trailer
  and its merge, and the last status `riff/verify` of the head of each.
- **01M5QDWWSYK0J3N24GF8J123JQ** The read of the open pull requests with
  `gh pr list --limit 100 --json` names no field that `gh` expands to a
  nested connection (`commits`, `comments`, `reviews`, `files`,
  `labels`). GitHub refuses a query of more than 500,000 possible nodes,
  and `commits` with the authors of each commit counts 1,000,000. The
  WIP check reads the head commit of a pull request that waits for its
  verify, and no other commit, with `gh api
  repos/OWNER/REPO/commits/SHA`. A fault of that read is no WIP. A test
  fails when the fields name such a connection.
- **01M5QVF58KEC9WNHAJ8WTZ18EP** `riff wave make` also makes the open
  milestone `Backlog` when the repository has none, and says so. When it
  cannot make it, it says why on stderr, and the wave stays made.
- **01M5QWA0H5KJ2SRRP50478X3J2** The check of the Backlog in `riff init`
  passes when the repository has the open milestone `Backlog`. Its step
  makes that milestone, as `riff wave make` does.
- **01M5R0HERC566W14SEJ3ZCG6V5** `riff wave move ITEM... --to WAVE`
  warns when an issue has no line that starts with `Done when:`, and
  moves it. The warning says that the wave takes only an issue with that
  line. `--to Backlog` gives no warning.

## Issue hygiene on GitHub

- **01M3JDRSW3E9ENAKC3S0KHPV8M** `just hygiene pr N` checks the form
  of pull request N with `gh`. The body has exactly one link line:
  `Closes #N` or `Refs #N` (`link`). No other closing keyword of GitHub
  stands before an issue number (`keyword`). The title does not end
  with `(#N)`, because GitHub adds the number of the pull request
  (`title`).
- **01M3JDRSYEGKYVNGF49SJMACEB** The body of a pull request ends with
  the trailers `Issue: #N` and `Milestone: M`, each one time. N is the
  issue of the link line (`issue-trailer`). M is the milestone of the
  pull request (`milestone-trailer`). Superseded in part by
  01M518A04JWYJE5FA1VR309SAA.
- **01M518A04JWYJE5FA1VR309SAA** The trailer `Milestone: M` names the
  milestone of the pull request when both are waves with the same
  number: `Wave N`, with a name or with none. A milestone that is not a
  wave needs its whole title. So a rename of a wave breaks no pull
  request (`milestone-trailer`).
- **01M3JDRT0SHF2J3NXNGZFWYH6W** A pull request and its issue have the
  same milestone (`milestone`). The issue is open (`issue-open`).
- **01M3JDRT31M12MJ1K592RAJQ78** `just hygiene commit [REV]` checks the
  message of a commit on `main`: the title ends with `(#PR)`, PR is not
  the issue (`commit-title`), and the message has the trailers
  `Issue: #N` and `Milestone: M`.
- **01M3JDRT5B52GDZK91VETC6VV2** The workflow `hygiene.yml` runs the
  job `Hygiene` (`hygiene pr`) on each pull request event: opened,
  edited, synchronize, reopened, milestoned and demilestoned. It does
  not run the `Gate` again. On a push to `main`, it runs
  `hygiene commit` on the new commit (01M3JFEXS6M5549TC6MH0G2MS0).
- **01M3JDRT7HMZD4FHWHDH3S1A1D** Each error of `hygiene` names its
  rule. It exits with status 1 on a broken rule, and with status 2 when
  `gh` or `git` fails.
## Pull requests on GitHub

- **01M3JFEXG85AJK8ZE8N807EQVB** `just github` sets up the repository
  with `gh api`, and is safe to run again. Auto-merge is on. Squash is
  the only merge. The squash commit takes the title and the body of the
  pull request. GitHub deletes the branch after the merge. The ruleset
  `main` on the default branch needs a pull request with 0 approvals
  and the checks `Gate`, `Hygiene` and `riff/verify`. A branch need not
  be up to date with `main`. No force push and no deletion of `main`.
- **01M3JN4QQCM0GXK9BCGXVS2YC7** The ruleset `main` has no bypass
  actor. GitHub refuses a push to `main` and a merge that skips a
  check, from each session and from each person. For an urgent fix,
  our user turns the ruleset off, pushes, and turns it on again with
  `just github`.
- **01M4YJ6KGBPTCSVMFQCDKACZPV** A patch of an older release line goes
  on the branch `release/MAJOR.x` of that line, by pull request. The
  branch starts at the last release tag of the line. CI runs the `Gate`
  and `Hygiene` on each pull request and each push of a branch
  `release/**`, as on `main`. A push to it does not deploy. The
  command `just github` makes the ruleset `release-branches` on
  `release/**`: a change needs a pull request with 0 approvals and the
  checks `Gate`, `Hygiene` and `riff/verify`, as on `main`. No force
  push, no deletion, no bypass actor. The checks do not stop the push
  that makes a new branch of a line.
- **01M56H9Q0757ESXMJEDM16Z7AT** Our user runs `just github` from a
  checkout of `main` only. The `deploy/github.sh` of `main` sets each
  ruleset, also the ruleset `release-branches`.
- **01M4YTW8788K4XYTEMRSXDH7B9** The GitHub release of the line that
  the shared riff runs is marked "Latest". A release of another line is
  not, also when its version is higher.
- **01M3JFEXJG2D651PWA30DNRGWF** No session pushes to `main` or runs
  `gh pr merge --admin`. The project settings of Claude Code deny both,
  and the skill says it.
- **01M3JN4QVR3JCRWC8TFJJTCPHF** Each session uses the GitHub account
  of our user, so the author of a pull request can set `riff/verify`
  on its own commit. We accept this gap until the sessions act with
  their own GitHub identity (#97).
- **01M3JFEXMPNFEV4HBZJQ15JD25** The author opens a pull request with
  `riff pr open`, with the milestone of the issue and the body form of
  the hygiene check, and auto-merge with a squash on at once, before
  any other push. No session runs `gh pr merge` after a push. The last
  pull request of an issue has `Closes #N`. Each other one, and one
  with a check after the release left, has `Refs #N`.
- **01M3JFEXPXRTXYHCV0WSKEK07M** The verifier puts its result on the
  pull request as a comment that names the commit. It sets the commit
  status `riff/verify` on that commit: `success` on a pass, `failure`
  on a fail. A new commit has no status, so it needs a new verify.
- **01M3JFEXS6M5549TC6MH0G2MS0** The workflow `Hygiene` also runs on
  each push to `main`. It checks the commit message with the commit
  rule of the hygiene check.
- **01M3NB6FTGPD0S5JTXXXNGNNDT** `riff pr open --title TITLE` opens
  the pull request of the current branch with `gh`. The issue is the
  one claim `issue-N` of the session, or `--issue N`. The body is the
  link line `Closes #N` (`Refs #N` with `--refs`), the summary of
  `--file`, and the trailers `Issue: #N` and `Milestone: M` of the
  issue. It refuses a pull request that breaks the hygiene check, and
  opens nothing then. After the create, it runs
  `gh pr merge N --auto --squash` at once. Superseded in part by
  01M518A07QF1YACBYT91M0DD1R.
- **01M518A07QF1YACBYT91M0DD1R** `riff pr open` writes the trailer
  `Milestone: Wave N` for a wave: its number, with no name. For a
  milestone that is not a wave, it writes the whole title. A trailer
  `Milestone:` of `--file` with the same wave number is the one of the
  pull request.
- **01M3W2627GYXR8CFW76KB6CB9W** `riff pr open` adds the link line and
  each of the trailers `Issue:` and `Milestone:` only when the text of
  `--file` does not have it. When that text has a link line, or a
  trailer `Issue:` or `Milestone:`, that is not the one of the pull
  request, it names the line, opens nothing, and exits with status 1.
- **01M3NB6FWMGBQ9VTY6RCBPKBHK** `riff pr wait N` looks at pull
  request N with `gh` each `--every` seconds (default 30), until it is
  merged. Then it prints the merge commit and exits with status 0. It
  exits with status 1 and the reason when the pull request is closed
  and not merged, or when a required check fails.
- **01M3Z8GG5EGEYAVEXG0HS46ACT** After one good look, `riff pr wait`
  goes on when a look of `gh` fails. It prints one line with the error
  on stderr, one time until the next good look, and looks again at its
  interval. When the first look fails, it exits with status 1 and the
  error.
- **01M3NB6FYXXKX80VHEVA5CV6RY** `riff verify pass|fail N --file
  RESULT` reports a verify with `gh`: one comment on pull request N
  that names its head commit and holds the result, one status
  `riff/verify` on that commit (`success` or `failure`) with the URL of
  the comment, and one riff post of the result to
  `[{"claim": "issue-M"}]`, where M is the `Issue:` trailer of the
  pull request. The tested commit is `HEAD` of the directory, or
  `--commit SHA`. When it is not the head of the pull request, it
  makes no comment, no status and no post, and exits with status 1.
- **01M49HAZ7P3JMWNCG1SWCMAXQP** `riff verify pass N` reads the check
  runs `Gate` of the head commit of pull request N with `gh`. When the
  commit has no run of the Gate, or a run is not completed, or a run
  has a conclusion that is not `success`, it makes no comment, no
  status and no post. It exits with status 1 and one line that names
  the commit, the state of the Gate and `gh pr checks N`.
  `riff verify fail N` needs no Gate. Superseded in part by
  01M5QYDE41FVFX26EC8K5QGYH4 for the checks that it reads.
- **01M3NB6G132QG4TAEJ5QPRJNAE** The skill names one `riff` command
  for each step of a pull request: open it, wait for the merge, report
  a verify. It has no `gh` recipe and no shell loop for these steps.
  Superseded by 4.0 (#1173).
- **01M5QWJJ94T4M3KJB0R3WTJCQF** riff pr open --base BRANCH opens the
  pull request against that branch, for example release/1.x, and not
  against the default branch. The body, the trailers and the auto-merge
  are the same. The steps for a patch of an older release line use it,
  not gh pr create.
- **01M5QYDE41FVFX26EC8K5QGYH4** The full check of a commit is each
  check that the ruleset of the default branch requires, but
  `riff/verify`: riff reads it with `gh api
  repos/OWNER/REPO/rules/branches/BRANCH`. When the ruleset requires no
  other check, the full check is the check run `Gate`. `riff verify pass
  N` and the verify of the wave read the check runs of the full check by
  name. A pass, and the start of a verify, need each check to be done:
  the pass needs a `success` of each. The line of a refused pass names
  the checks. Supersedes 01M49HAZ7P3JMWNCG1SWCMAXQP in part.
- **01M5R13DE5XRWB13E0BQ17C80F** A check of the full check that ends
  `skipped` or `neutral` counts as a success, as on GitHub: `riff verify
  pass`, `riff release verify`, the verify of the wave and the look at
  `main` use one rule. A conclusion of `failure`, `timed_out`,
  `cancelled`, `stale` or `action_required` is no success. Supersedes
  01M5QYDE41FVFX26EC8K5QGYH4 in part.

## Tokens on GitHub

- **01M3Y1YP0QY11VR28RF9MKPN0G** riff sums the tokens of each claim
  from the transcripts of its session, on the machine of the session:
  the input, output, cache write and cache read tokens of each model.
  A reply counts when its time is from the claim to before its end. A
  reply counts one time, also when the transcript has it on more than
  one line. The transcripts of the helper agents of the session count.
- **01M3Y1YP15C7AT2N70BWQP8PE2** riff keeps the marks of each session
  on its machine, in a directory that holds over a restart: each
  transcript of the session, and each claim with its start time and
  its end time. The start hook, the Stop hook and the end hook record
  the transcript. A granted claim records the start time.
  Superseded by 4.0 (#1173).
- **01M3Y1YP1JAQMJ66K2QXC7766C** Tokens in a time with two claims of
  one session count for the claim that started last.
- **01M3Y1YP1ZA5TBRA01MKWM3VC6** At the end of a claim, riff puts its
  sum on the issue of the claim as one comment, with `gh`. `issue-N`
  and `verify-issue-N` are claims of the issue N. A claim ends at its
  release, at a leave, at a new start of its session and at the end of
  its session. The log of `riff-server` holds no tokens.
- **01M3Y1YP2CSNHCWV7T4CE9HZ4Y** The comment has one line for a
  person, then the same numbers as JSON in a `details` block with the
  summary `riff:usage`. It holds only the item, the kind of the claim
  (work or verify), the first 8 characters of the session ID, the two
  times, and the four kinds of tokens for each model. It holds no text
  of a transcript, no path and no email.
- **01M3Y1YP2TVYQC7GCCAMN6111K** A second report of the same claim
  replaces the comment of that claim that the same person wrote. It
  adds no comment.
- **01M3Y1YP39VFX6GH33H7B8A8KR** A report never fails a release, a
  leave or a hook. With no `gh`, a `gh` that fails, an item that names
  no issue, or a thread that is no repository, the sum stays in the
  marks of the machine, and the result of the release says why. A
  claim with no tokens gets no comment.
- **01M588N85Z94PX1ZP0BDXTPX83** At the end of a container session,
  the riff daemon puts the sum of its claim on the issue of its item,
  as a release does: for a work claim and for a verify claim. The
  claim counts from the start of the container to its end. The report
  never fails or delays the end of the session. The daemon log has its
  line.
- **01M588N8AYSGVWFW05A5RV976H** The daemon reads the transcripts of a
  container session in the Claude folder of the session:
  `claude/projects/*/SESSION.jsonl`, with the riff session ID.
- **01M3Y1YP3QMKS6B35PJ42KNYXX** `riff usage ISSUE` sums the comments
  of the issue with the mark. It counts a claim one time: of two
  comments of one person for one claim, the later one counts. It shows
  the total with the four kinds, then the work claims and the verify
  claims, each with its session, its models and who wrote its comment.
- **01M3Y9TD41FZBDQBK42FVG89B8** `riff usage` trusts a comment of
  another person only for its numbers. It takes a report only when its
  item is `issue-N` or `verify-issue-N` of that issue, its kind agrees
  with the item, and its session and each of its models have only
  ASCII letters, digits, `.`, `_`, `-` and `:`. Each other report
  counts for nothing. A comment of another person never replaces a
  report: it counts as a report of that person. A sum past the largest
  number stays at the largest number, and fails nothing.
- **01M3Y1YP45VQS5HMJCXKRN3CCR** `riff usage --wave WAVE` lists each
  issue of the wave, open and closed, with its total, and shows the
  sum.
- **01M3Y1YP4KVHK1DTZ85YNGDG0T** `riff usage` with no issue and no
  wave shows each session with marks on this machine: its total, the
  tokens of each item, and the line `no issue` with the tokens outside
  each claim.
- **01M3Y1YP514MPX8DTKMTWDHE8Q** After the merge, `riff pr wait` puts
  the open claim of the issue on the issue as it is then, and adds one
  comment with the total of the issue and its models. Each later
  report of a claim of the issue writes that comment again. A failure
  of this step does not fail the wait, and a total that riff cannot
  write does not fail a report.
- **01M3ZRQY9F9P7DF187Q0PJDS30** `riff usage ISSUE` says how many
  comments with the mark it did not count, and why: an item of another
  issue, a text that is no name, or no report that it can read. When
  riff cannot write the total comment of an issue, the text of the
  release and of `riff pr wait` says that the comment of the claim is
  on the issue, that the total is not updated, and why. A total comment
  of another account is not updated: GitHub lets only that account
  edit it.
- **01M3Y1YP5GC5W9KVJQP6PXPM3G** The book has the how-to "See the
  tokens of an issue" with `riff usage`. It says that the numbers are
  public on the issue.
- **01M49HF057R08DGW6X5A8EHR42** riff gives `gh api` the body of a
  write in a file, never on stdin. When the write fails, the error
  names the HTTP status of the reply, when `gh` got one.
- **01M49HF07WQC3M5HGAQNHR8WA0** When riff cannot write the total
  comment of an issue, it reads the comments again and tries one more
  time.
- **01M49HF0ADEQ83XTJCQT0PK3RS** `riff usage ISSUE --total` writes the
  total comment of the issue again, then shows the tokens of the
  issue. The book has the how-to "Write the total of an issue again".

## Pause

- **01M3JCFTWCR72HQB8CBTQKXJNF** The whole riff is paused or running.
  The state is one for each `riff-server`. It is a record in the log,
  so it stays when the sessions and the server restart. A new riff
  starts paused.
- **01M3XAHZBGSSJB3YX23K88W01K** Each repository has a pause of its
  own. A session is paused when the whole riff is paused, or when the
  repository of its place is paused. A thread is paused when the whole
  riff is paused, or when it is the thread of a repository that is
  paused. A pause of one repository does not stop a session or a claim
  of another repository. `riff pause` and `riff resume`, and the MCP
  tools, name the repository of the call. `--riff` (the tools: `riff`
  true) names the whole riff. `--repo OWNER/REPO` names a repository.
  A resume of a repository while the whole riff is paused changes only
  the pause of the repository.
- **01M3JCG3T8AJZN31SZQQTP3FAF** Replaced by
  01M3XAHZDSQR263QZVB41CK0MX.
- **01M3XAHZDSQR263QZVB41CK0MX** A person (a call with no session ID,
  for example `riff pause` in a shell) or the lead of a repository can
  pause and resume the repository of the call. Only the owner or an
  admin can pause and resume the whole riff, or a repository that the
  call names: as a person, or as a session that is a lead. The server
  refuses each other caller with the code `not_allowed` and a reason.
  In a riff with no sign-in, each caller has the role of an admin.
- **01M3JCG3WBHDF0ZWM06XV94ZDC** While a thread is paused, a claim in
  it fails. The refusal says which pause stops it (the riff or the
  repository), who set it, and who can end it. The sessions keep the
  claims that they hold. A release still works.
- **01M3JCG3YD7C2Y3V0QJPF082YH** A pause or a resume that changes the
  state wakes each session that it stops or starts, and that is not
  gone: the client posts to the thread of the repository, to that
  repository. For the whole riff, it posts to the thread of each
  repository of such a session.
- **01M3XAHZSJ5914BRQBZ2G4ZBSA** A session that another pause still
  stops does not wake: a resume of a repository in a paused riff wakes
  nobody, and a pause or a resume of the whole riff does not wake the
  sessions of a repository that has a pause of its own. The answer to
  a resume names each pause that still stops work.
- **01M3XAHZG26ECNARX35JD73YXJ** The record `pause_set` holds a pause
  that is set or ended: `scope` (`"riff"`, or
  `{"repository":"OWNER/REPO"}`) and `state` (`paused` or `running`).
  A build reads a scope that it does not know as `other`, and such a
  record changes no pause.
  `make_riff` makes a `pause_set` record.
- **01M3XAHZQ92GGFHBC50FQ7FQ0K** The state keeps who set each pause
  (the `by` of its record) and when (the time of its record). The
  checkpoint holds each pause with the two. A start from a checkpoint
  gives the pauses of a full replay. A checkpoint from before the
  pause of a repository reads.
- **01M3XAHZJAF6YVDJ7WX74X8RBX** `riff whoami`, `riff who`, `riff top`,
  the MCP tools `whoami` and `who`, and the start hook show which
  pause stops a session and who set it. The reply to `/v1/riff` has
  the state for the place of the caller, the pause of the whole riff,
  and each repository that is paused.
- **01M3JCG40FN0DP135EHHF403TY** While the riff is paused, a new
  session says hello to the lead, waits, and claims nothing.
- **01M3JCG42FYS8FJ0V6WK89KXAP** While the riff is paused, a session
  with work stops at its next step. A command that runs finishes
  first. The session commits each change as a WIP commit on the branch
  of its worktree, pushes that branch, and waits.
  Nothing goes to the default branch: a session between a verify pass
  and its merge stops before the push, and a verify stops with no
  result.
- **01M3JCG44JKRVY8T4TZMB5PXNF** While the riff is paused, messages
  still flow. A session answers a status request and a question from
  the lead. The watch runs, so the session stays live.
- **01M3JCG46KKK97VD00KQ4DW6HK** When the riff resumes, each session
  goes on from where it stopped. A new session follows the start
  routine.
- **01M3JCG48QPCNNTKW34FTR0AMR** The start hook and the skill tell a
  session the state of the riff. They tell it to pick a free item only
  when the riff is running. When the hook cannot read the state, it
  tells the session to call `whoami`. Superseded by 4.0 (#1173).
- **01M3JCG4AV80MHFP73CWDY5E3M** `riff who`, `riff whoami` and the
  `whoami` tool show the state of the riff.
- **01M5P1XQ6WTGFHW0P5M114P535** The riff daemon sends no verify request
  for a pull request whose head commit is a WIP commit. The item goes
  back to a build.

## Service

- **R118** `riff-server` with no command runs in the foreground, in a
  terminal. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3K0QM5HY852J4E5M2YQDYEM** `riff-server` has one command, `log`
  (01M3TJWHNYRCA7RTPFNYM5ZNQS), and installs no service. A person runs
  it in a terminal. The cloud runs it on Cloud Run. It keeps no
  settings: it reads them only from its options and the environment at
  each start. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3TJWJ3VK671T9NM95F3ES82** Each log line of `riff-server` is one
  JSON object on stdout, with the fields `severity`, `time`, `message`
  and `target`, and each field of the event. The `severity` is `DEBUG`,
  `INFO`, `WARNING` or `ERROR`. An error of the options comes before
  the log starts: it is text on stderr.
- **01M3TJWJ12WEDCXW3W0529KRP2** `GET /v1/server` gives the facts of the
  instance: if it serves, or why it replies 503; its last error; the
  log position, and the time and the duration of the last chunk write;
  the numbers of write errors and skipped records since the start; the
  position, the time and the version of the newest checkpoint, and why
  this build writes none; the numbers of chunks, sessions, read
  cursors, threads and live token chains; the memory in use; the start
  time, and how long the load and the replay took. It answers also
  while the instance replies 503 to each other call, and to a `riff` of
  each version. With sign-in, it needs a token. `riff server` shows the
  facts under the riff that `riff` uses.
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
- **01M3MRDEVR5VPPV6B1BDDVYSBG** A worker never runs `riff update`,
  `cargo install` of riff, `just install` or `riff connect`. The tests
  run the binaries of the worktree with a home of their own, so the
  installed binaries stay the same.
- **01M3MY2KWKBJCQ0BCNC6533RBW** A test run and `just dev` never touch
  the riff of the machine: its sign-in, keyring entries, device key,
  settings, local files, the riff of the machine on 127.0.0.1:7878, or
  the shared server. Each integration test runs `riff` and
  `riff-server` only through the helper crate `isolated`. It removes
  each `RIFF_` and `CLAUDE_` variable, `TMUX`, `TMUX_PANE`, `GH_TOKEN`
  and `GITHUB_TOKEN`. It sets
  `RIFF_SERVER` to `http://127.0.0.1:9`, where nothing listens, unless
  the test names its own server. It sets `RIFF_HOME`, `HOME`, the XDG
  dirs and `TMPDIR` to a temp dir of the test, a D-Bus address that
  does not exist, and git with no config of the machine. A test fails
  each test file that names a binary of riff without the helper.
  `just test` and `just ci` run with the same `RIFF_SERVER` and a D-Bus
  that fails each call. `riff server` and `riff update` still ask the
  riff of the machine for its build, by design. `just dev` sets
  `RIFF_HOME` to `target/dev-home` of its tree.
- **01M3WG82ZMQYG1TGHE4ET0BDDW** In a test, git reaches no remote on
  the network: the helper `isolated` sets `GIT_ALLOW_PROTOCOL` to
  `file`.
- **01M43B48Z8R0SXAWBQP75CPR55** A test that needs a place outside each
  git repository gets its temp dir from `isolated::outside_git`. The
  dir is in the first temp root with no repository at it or above it:
  `TMPDIR`, then `/var/tmp`, `/tmp` and `/dev/shm`. So the test passes,
  and writes nothing, also when `TMPDIR` is in a repository, for
  example the home of the person.
- **01M43B491Z25KT0XBC7CANFS5G** Replaced by 01M49NP2907J4SH4S6MAY09VXE.
- **01M49NP2907J4SH4S6MAY09VXE** Each cargo run in the riff repository,
  also a plain `cargo test`, has `TMPDIR` set to `/var/tmp`:
  `.cargo/config.toml` sets it with `force`. So no test of a worker
  finds the repository of the home, or writes to it.
- **01M49NP2JW8JFWYY56K7AK3H05** The temp dir of the test helper
  `isolated` is outside each git repository, also when cargo runs from
  a dir outside the riff repository. So no command of a test writes
  settings in a repository above the dirs of the test.
- **01M3W98PMDPZW1CR3KJYMPHVQZ** A test server starts with no old
  sign-in at its URL. The tests of one test binary share one mock
  keyring, and the OS can give the port of an earlier test to a later
  test. A test in process that puts a new riff at its URL keeps its
  listener. A test that needs a URL where nothing listens holds the
  port with a socket that does not listen.
- **01M41A0M2XWCWTWGF7T9DR03W0** A test waits for the fact that it
  checks, for example a message, a line in a log or a look of a host.
  It does not wait for a fixed time. A time limit of a wait only ends
  a test that hangs, so it is generous. A test passes on a busy
  machine. Only a check that a thing does not come waits for a time.
- **01M49HCWEHTKSVQ6176G7C6H2S** A timed check in a test fails only
  when the logic is slow. It uses a `Span` of the crate `isolated`. A
  low CPU time does not pass a check. When a check is over its limit
  and the CPU pressure of the span (`/proc/pressure/cpu`, the `some`
  line) is 10 % or more, the test prints `slow under load` and the
  check passes.
- **01M56QW660NSKW0HC5W518EZZQ** A `Span` also reads the IO pressure
  and the memory pressure of the span (`/proc/pressure/io` and
  `/proc/pressure/memory`, the `some` line). When a check is over its
  limit and the highest of the three pressures is 10 % or more, the
  test prints `slow under load` with the name of the resource, and the
  check passes until 60 s.
- **01M49HCWH5HR0GPKHXKYZTXPW3** A wall-clock limit in a test only finds
  a hang: each check of a `Span` fails after 60 s. Each test that
  checks a time with a limit under 60 s uses a `Span` or
  `isolated::in_time`.
- **01M513N34NJ2G3TK3EH91M10FJ** A unit test of `riff` that runs a fake
  program has a child process write the file. A doc test that drops a
  lock, and then checks that the lock is free, is a `standalone_crate`.
  A unit test of that kind waits for the end of the lock.
- **01M517P9J77TZX28QYBR9SH6WW** Each integration test (a file under
  `crates/*/tests`) writes a program, and each new text of a program,
  with `isolated::write_program`. It copies a program with
  `isolated::copy_program`. A child process writes the file, so the
  test process never holds a write handle of it. A test of the crate
  `isolated` fails on each line of such a file that writes a program in
  the test process: a mode 755, a text that starts with `#!`, or a
  write to the file of a program that the test file made with the
  helper.
- **01M517P9MTBWPN0FQRMXEJ9JG5** The unit tests of `riff` set the mock
  keyring store in one place: `secrets::mock_store`, one time for the
  process. A unit test fails on each other line of code of `riff` that
  sets the store, apart from the one line that sets the store of the OS
  (01M556QPPRSQ91HDED128G2X7G).
- **01M517P9QE2SQ45SPV3K2SM71B** `just stress ARGS` runs `cargo test
  ARGS` many times while busy loops keep the CPUs busy: 50 runs and 8
  loops, or the numbers in `STRESS_RUNS` and `STRESS_LOAD`. It builds
  the test first, keeps the output of each failed run in
  `target/stress`, stops the loops at its end, and fails when a run
  failed.
- **01M3MY2KSV73WS8D902YCH2PRX** With `RIFF_HOME=DIR`, `riff` keeps its
  settings in `DIR/config.toml`, its local files in `DIR/state`, and
  each secret in a file of `DIR/secrets` that only the owner can read.
  It never opens the OS keyring. Only the tests and `just dev` set it.
- **01M51KE2HCRCM69EG1KHZAHH7A** With `RIFF_HOME`, the file of a secret
  is the name of the secret in hex. When that is longer than 255 bytes,
  the file is `sha256-` and the SHA-256 of the name in hex.
- **01M3K0QM89E2XM1NWSPT4KXSTC** `just dev` loads `.env` at the root of
  its tree, when the file exists, into the environment of `riff` and
  `riff-server`, for example `RIFF_OIDC_CLIENT_ID` and
  `RIFF_OIDC_CLIENT_SECRET`. Only `just dev` loads it. Git ignores
  `.env`.
- **R33** `riff-server` rejects a token that it does not know. A lost
  token record means the person signs in again. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3QA6TDF6FB5PH8E5V7HCYDQ** Each event stream of `riff-server`
  (`/v1/watch`, `/v1/tail`) sends the comment `: ready` when it opens.
  So a front end that holds a reply until its first body byte lets
  the connect finish at once. `riff chat` shows its history less than
  1 second after the connect.
- **01M5GVK79HV5KSDE626Z5AS1EG** A test that sets `RIFF_SOCKET` gives it
  a path in a temp folder of the test, never the socket of a session
  `/run/riff/riff.sock`. So a test gives the same result in a container
  session and on the Gate. The helper of the release tests refuses
  another path.

## Saved state

- **R30** `riff-server` keeps its state in memory. With `--bucket NAME`
  (`RIFF_BUCKET`), it keeps its log and its token store in that Cloud
  Storage bucket. With `--dir DIR` (`RIFF_DIR`), it keeps them as files
  in that directory. At start, it loads the newest checkpoint and
  replays the log after it (01M3TBZBMMSMNWP126ZQED13YG). Superseded by
  3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **R34** Storage is behind one interface. Tests use an in-memory store
  or a temporary directory, never the real bucket. Without `--bucket`
  and `--dir`, the log is in memory, and a restart loses it.
- **R124** The bucket holds the chunks of the log, the checkpoints, the
  token store and the lease. The token store is the object
  `signins.json`. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **R31** A restart loses the open streams, the proof IDs, the sessions
  with their places and statuses, each change of a read cursor since
  the last checkpoint, and each record whose chunk was not written.
  The call of such a record got no success.
- **R125** After a replay, each session counts as stopped at the time
  of the load. Its claims and its lead end after the grace period
  (R9), unless it comes back. A record that came since the load
  (01M3THEE08ZKV8WGHDSVWV69ZE) does not start the time again.
- **R154** Replaced by R125.
- **R126** After a replay, each session is gone until it calls again.
  `who --all` shows it in the place of the last record that names it.
- **R127** `riff-server` saves the changed token store at most once
  each second. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M53T8X5KH59R3P8H2D22WE32** The second of R127 counts from the
  end of one save of the token store to the start of the next. So the
  store gets at most one save of the object each second, also when one
  save is slow and the next one is fast. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R128** Replaced by 01M3XA877YZQ649SWB5TN60V5P.
- **01M3TFG527M04TA7ESM970X3B8** Replaced by 01M53T8XH45WSVKS9FV3VXF2X8.
- **01M53T8XH45WSVKS9FV3VXF2X8** `riff-server` replies to a refresh and
  to a swap for a session token before it saves the token store. While
  the last save of the token store failed, a refresh first saves it
  again, and gets 503 when that save fails too. A swap never waits for
  a save, and a failed or busy save does not refuse it. Superseded by
  3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R150** When that save fails, `riff-server` replies 503. The next
  save tries the change again. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3ZZQ9TRG9385GRQGM79RCXX** When the store refuses a save of the
  token store for its rate limit (HTTP 429), `riff-server` doubles the
  least time to the next save, to at most 32 seconds. A good save sets
  it back to one second. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3ZZQCEKEYK9CE8MGPM80Z2P** While the last save of the token store
  failed for the rate limit, a refresh does not save the token store.
  `riff-server` replies to the refresh while the last good save is
  younger than the least time to the next save, and replies 503 after
  that. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R129** On SIGTERM, `riff-server` replies 503 to each new call,
  writes each record in the queue, saves the token store, then exits.
  Ctrl-C does the same.
- **R46** A lifecycle rule of the bucket deletes each thread object 30
  days after its last change. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **R147** The name of each chunk of the log starts with `log/`. The
  name of each checkpoint starts with `checkpoint/`. The token store
  and the lease have names outside `log/` and `checkpoint/`.
- **R149** Replaced by 01M3T411BZQB8N4D2S0JFVESMS.
- **01M3MMXYS1V8CA89D2XHKPR6C4** When `riff-server` cannot read a saved
  object at start, for example state of an old format, it does not
  migrate it. It logs one error at the ERROR level and stops. The error
  names the object, for example `gs://BUCKET/signins.json`, and the
  fix: stop each server of the store and remove the old state, with the
  command.
- **01M588N3JF6MXF3ZKB5PBWP06G** The fix that removes the old state
  comes only in the error of a start of `riff-server`. The reply to a
  request of a client never has it. An error of the store in such a
  reply is 503 only when the store did not do the call.
- **01M588N3CKQMSDQPVFYGTJQQ2T** A read of the log from a position
  before the oldest chunk fails with an error that names the oldest
  position and the position that the read needs, and says that the
  checkpoint rule removed the chunks before it.

## The log

- **01M3T410XDD9W4EC0Y68FAA7XN** Each change that must not be lost is a
  record in one log: a message with the sessions that it woke, a join
  or a leave of a thread, a claim or a release, a lead, the state of
  the riff, a setting, and a forgotten session. Each record has a
  position: 1, 2, 3, and so
  on. A record is one line of JSON. The name of its change says what
  happened, in the past tense, for example `claimed`. Superseded by 3.0
  (01M4Z60B8BHAR9GHCC0M7V7CV5).
- **01M3T4111PFM0C6KPREWFS9EQQ** A new field of a record has a default,
  and the default means "as before". A build skips a field that it
  does not know. A change of a record never changes the type or the
  meaning of a field, and never uses the name of a removed field
  again. A new kind of change gets a new name. A build that does not
  know a kind skips the record, and logs a warning with its position.
- **01M3T411QW1SQV12RJVATEJ8YD** A record names a session by its URI
  with no lead mark and no claims: the user, the session ID and the
  place at the time of the change.
- **01M3T4115BF1F0JFHYMK0WRKCX** `riff-server` checks each call against
  the pending state: the written state and each record in the queue.
  Each read, wake, view and reply uses the written state: only the
  records whose chunk is written. A call that makes a record replies
  after its chunk is written. Its wakes and its `tail` events go out
  after the write too. A command that makes no record waits too
  (01M3WRD933ESXF33WDEDFCRFB8). A signal and a query do not wait.
- **01M3T4118SDERYGJ25TAT1RGR2** The writer takes each record in the
  queue into one chunk: a new object with the name `log/` and the
  first position in 20 digits, for example
  `log/00000000000000001234.jsonl`. The first line of a chunk is a
  header with the format and the first position. The writer writes at
  most one chunk at a time, outside the lock of the state.
- **01M3T411BZQB8N4D2S0JFVESMS** Each write of a chunk is "only when
  new" (`ifGenerationMatch=0`). On the first try, an object with the
  same name stops the instance for good (R141). Each other error, for
  example a 429, a 5xx, a timeout or a failed token, is tried again
  with a backoff for 10 seconds. A later try that finds an object with
  the same bytes is done. After 10 seconds, the instance logs one error
  that names the chunk, and stops for good (R140). The next instance
  replays without the chunk.
- **01M3T411F3K6FD28R3Q3ZE4VCN** The replay reads each chunk in name
  order, and checks the positions: each record is the last position
  plus 1. A gap, a repeat, a line that does not read, or a chunk of a
  later format stops the start. The error names the chunk
  (01M3MMXYS1V8CA89D2XHKPR6C4).
- **01M3WNQRCBP0PHSA0H3THDH5NJ** The state of `riff-server` has two
  types: the riff (the state that the log gives) and the presence
  (memory). Only `apply` changes the riff. A record changes the
  presence only in `Presence::applied`. Each command is a type with
  its `handle` rule, in the file of its group: sessions, threads,
  work, or the riff.
- **01M3WNQQWA7XGK4Y9ET8HJZ8NN** `apply` reads only the record and the
  riff, and no clock. It makes no decision. The places where it reads
  the riff are in a list in `state/riff.rs`, and a new place needs a
  line there. Three places stay for good: a `session_forgotten` record
  removes each thing of the session, a `left_thread` record ends the
  lead of the session in that thread, and a thread keeps its last 200
  messages. A `released` record of a session that does not hold the
  item changes nothing. A `claimed` record replaces the old holder. A
  `lead_set` record replaces the old lead of the user in the thread.
  The `posted` and the `session_forgotten` records keep the index of
  the signed messages.
- **01M3WNQR41K41TV832GRQZ2CQS** A log and a checkpoint that a build
  that was released wrote read with no change, from 1.0.0 on. A test
  reads the log and the checkpoint of each release, and compares the
  state and the bytes.
- **01M3XM2C18TT8VSKGD77YPZG53** A record with a value that a build
  read as `other`, in its change or in its `by`, counts as a skipped
  record: the build writes no checkpoint past it, and `riff server`
  counts it. The fields are `reason`, `scope` and the class in `by`.
  Each of them reads a text and each other form of JSON that the build
  does not know as `other`. The `state` of a `pause_set` record has no
  `other`.
- **01M3XSF90E9JYYTC13D9THY4WE** In a `posted` record, the kind of the
  message and each selector follow the rule of `other` too. A build
  reads a kind that it does not know as `other`, and a reader shows
  the message as a message, with its text. A selector that the build
  does not know is a selector `other`: an object with a field or a
  value that the build does not know, and each other form of JSON. It
  keeps its JSON as it came, and it matches no session. Such a record
  counts as a skipped record. A `post` call with such a kind or such a
  selector is refused, with the code `bad_request`: only the read of
  the log takes a value of a later build. The rustdoc of the records
  lists each type with named values in a record, with its rule.
- **01M3XYYSY536AEJVERBPTQFQYX** A session URI in JSON keeps each
  query part that the build does not know, as it came. The session is
  the same session: its user, its session ID and its place. The part
  gives no mark: no lead and no claim. A record with such a URI counts
  as a skipped record. `riff-server` refuses a call of a caller with
  such a URI, with the status 400. The rustdoc of the records lists
  each text with a grammar in a record, with its rule or with the
  reason why it never grows.
- **01M3XM2C3MND6YB24SGZ565353** A kind of record and a kind of
  command are never renamed, and the name of a removed kind is never
  used again. A file in the fixtures lists the kinds of each release. A
  test fails when a name of the list is gone from the code. The enum of
  the kinds and its list come from one place.
- **01M3XM2C60TKF05NETHY8EYP3P** The format of 1.0.0 has only the kinds
  of a build that was released: it has no `riff_state_set`. A fixture
  holds one record of each kind, and CI replays it. A test makes a
  checkpoint at each position of the fixture: a load of it gives the
  state of a replay up to that position, and a start from it gives the
  state of a full replay. The import of go-live writes only kinds of
  1.0.0.
- **01M43GSRSDJMGAH8SR1GD4Z3XF** The fixtures of each release after
  1.0.0 are in a directory of their own, for example
  `crates/riff-server/tests/fixtures/1.1.0/`. Its `kinds.json` lists
  only the new kinds of the release, and the test of the kinds takes
  the union of the lists. Its log has each new kind, and gets the same
  checks of a checkpoint at each position.
- **01M43GSGVYJW7C09SVRWRAQZDZ** The checkpoint has the part `plans`:
  each hold of each repository thread, with its reason, its caller
  and its time. An empty part is not written, so the log of 1.0.0
  gives the checkpoint of 1.0.0.
- **01M4A4Z3QVRC57RE7M43ZRF4T2** The part `plans` of the checkpoint
  also has the plan of each repository thread, with the position and
  the time of its `plan_set` record. The time of the last look is not
  in the checkpoint.
- **01M3WRD8WJ2JF9077PRDX04T9A** Each command of `riff-server` goes
  through `Engine::dispatch`, and each signal goes through
  `Engine::signal`. The engine module owns the state, its lock and the
  queue of the writer: no other code locks the state. A query reads the
  written state.
- **01M3WRD8YQFSKR2PENZC6CX24B** The stages of a command are types:
  `Authenticated`, `Checked`, `Queued` and `Applied`. Each one is made
  from the stage before it, and only the engine module makes one. Only
  `Applied` gives the reply.
- **01M3WRD90WBBCWTDGVQCBR6MNT** The writer finishes each command. It
  writes the chunk, applies its records to the written state in the
  order of their positions, sends the wakes and the `tail` events, and
  then tells each call of the chunk. A call that the client drops loses
  only its reply.
- **01M3WRD933ESXF33WDEDFCRFB8** Each command waits until the writer is
  done with its entry in the queue. This is also the rule for a command
  that makes no record, and for a command that is refused. So no reply
  and no refusal tells of a change that is not in the log.
- **01M3WRD8TBDPA4JNEZY6J4N2EX** Each call is one wire type that gives
  its path and the type of its reply (`Call`, in `riff-core`). The
  client sends each call with one function. The wire type of a command
  that a client sends is its command type, and one handler serves each
  such command (`Routed`).
- **01M3WRD959DYNZHDKP5ZT9Q1C7** One function, `permits`, says who can
  send each command. It reads only the caller (its class, its worker
  mark and its role) and the role that the command needs. A kind of
  command with no row does not compile. The token layer gives the class.
  The engine adds the worker mark and the role under the lock of the
  state. A refusal of `permits` has the code `not_allowed`. A worker
  cannot send `lead`.
- **01M51JBTEM10W68K6F267BB79N** A rule fails closed. A new kind of
  command, class of caller, sign-in error or failure of a call does
  not compile until a person gives it its rules. Each of these rules is
  a `match` that names each kind, with no wildcard arm and no
  `matches!`: the classes and the worker rule of `permits`, the
  commands of the people, the commands that register an unknown caller
  first, the log text of a caller and of a sign-in error, and the reply
  to a refused sign-in.
- **01M3WRD97EZJK3AABXECXEY133** A status, the place of a session, a
  keep-alive, the start and the end of a watch stream, and a read cursor
  are signals. A signal changes only the presence: it makes no record
  and no entry in the queue, and it does not wait for the writer. A
  command can give a signal: a `register` gives the place, and an `end`
  gives the end of the session. A refused command
  sets no signal. A signal or a query of a session that the state does
  not know first sends `register` through the dispatch, and waits for
  its write.
- **01M3WRD99M99PNGP8ME50KC6WS** The first start of a riff sends the
  command `make_riff` of the server: the first record of the log pauses
  the riff. A later start adds no record.
- **01M3WRD9BSBKS9TN66H29TGTBV** `pause`, `resume` and `set_idle` are
  commands, each with a path of its own: `/v1/pause`, `/v1/resume` and
  `/v1/idle/set`. A read of the pause (`/v1/riff`) and of the settings
  of idle workers (`/v1/idle`) is a query.
- **01M3WRD9DYJWVN1QRBAC3ZVVZD** Replaced by
  01M3Z8MRDZEKTXSKZTDTDSCZ3W.
- **01M3Z8MRDZEKTXSKZTDTDSCZ3W** Go live keeps the work of each
  repository of the riff. A start of `riff-server` on a store that has
  the objects of a `riff-server` from before the log (`sessions`,
  `tokens` or `threads/`) and no log is the import: the command
  `import` of the server writes the state of the old objects to the log
  as one chunk. The riff keeps its ID, its people, its settings, each
  session with its threads, its claims and its lead, and the last 200
  messages of each thread with their seq. A session with no sign of
  life for 30 days is not in the import. A session that ended, or that
  stopped more than 5 minutes before the save, keeps no claim and no
  lead. The riff is paused after the import. The server writes a
  checkpoint with the read cursors at once, and opens its port only
  after the import. An old object that does not read stops the start,
  and the server takes no lease.
- **01M3Z8MRKTAN8CBAQB721JNZAK** The import runs one time. The command
  `import` is refused in a log that has a record. A start on a store
  with a log does not read the old objects. The import changes no old
  object and deletes none.
- **01M3Z8MRGWWA0CNZ003D67H6R4** The import keeps each live sign-in of
  the old `tokens` object, at the position of the end of the import. A
  person refresh token of the old server that was not used works one
  time: it gives the first pair of a new chain. A used token, a
  session token and an access token of the old server give nothing. No
  person runs `riff login` at go-live.
- **01M3ZCDNQY2G9ET537B6SBYCBB** The import reads the old objects two
  times: before the server takes the lease, as the check, and again
  after its wait for the old instance. It imports from the second
  read. When the store has a log after the wait, the server does not
  import and does not change the sign-ins: it replays the log.
- **01M3ZCDNR0DT9J5XXXS89APTQ2** The import makes the payload of a
  signed message of release 0.8.0 from the fields of the message. A
  message that was verified before the import is verified after it. A
  message whose signature does not sign that payload stays as it is.
- **01M3WRD9G5GAF65EX8P6D5DMQM** A riff with no sign-in takes a call
  with no token: the caller is then the `me` of the body, with the role
  of an admin. A call whose body names no `me` needs a token.
- **01M3WRD9JBQMNN96TXJH8EAJ3W** A refusal of a command has a code and a
  reason. The reply has the status of the code: 403 for `not_allowed`;
  409 for `held`, `paused`, `not_holder` and `other_user`; 400 for
  `bad_request`. It has the code in the header `riff-refused`, and the
  reason as its text. A claim of an item that another session holds is
  refused with the code `held`, and the reason names the holder. `riff
  claim` and the `claim` tool show that reason.
- **01M3WRD9MGSC3FTBAANT4ZSMKY** `release_for` is a command of its own,
  with the path `/v1/release/for`: the lead of a user frees the claim of
  another session of that user. Only a session can send it. The note of
  the server for it is a `posted` record in the chunk of the command,
  after the `released` record.
- **01M3X4Z60G1FXQTDC5XDJ05BAX** The envelope of each record names its
  cause: `by`, the caller of the command, and `command`, the kind of the
  command. `by` is an object that names the class of the caller
  (`person`, `session`, `sign_in`), or the text `server`. A record with
  no `by` reads: its cause is not known. A build reads a class that it
  does not know as `other`. The records of one command are in one chunk,
  one after another. `riff-server log` prints the cause of each record.
- **01M3X4Z62RJREQ5H8F18Y85T6V** Each command leaves one trace: its
  records, or one log line. A command that is refused, or that makes no
  record, gives one line with `caller`, `key`, `command` and `result`
  (`refused` or `no_change`). A refusal also has `code` and `reason`.
  `key` is the thumbprint of the device key of the token. A chunk
  whose write fails gives one line with the result `failed` and the
  severity `ERROR` for each of its commands. A command that waits in
  the queue when the server stops gives a `failed` line with the
  severity `WARNING` and the reason of the stop. The writer makes these
  lines. A command with records, a signal and a query give no line.
- **01M3X4Z64ZNRD0G0F4JV1M64FN** The token layer writes one line with
  the result `denied` for each call that it refuses, up to the limit
  of rate (01M3Z67DZX9BC3TYF3PWGFGZJ7). The line has
  `path`, `code` (`no_token`, `bad_token`, `bad_proof`, `not_you` or
  `old_build`) and `named`: the caller that the call named, with
  `proved` false. The server reads the body of the call for the name
  only after the refusal, and at most 64 KiB of it. A name of more than
  200 characters is cut, and the line says so (`named_cut`).
- **01M3Z67B9RMVKY7TCXCG8HEZT4** The read of the body of a refused call
  takes at most 2 seconds (`trace::BODY_TIME`). After it, the server
  writes the `denied` line with no `named`, and the reply closes the
  call.
- **01M3Z67DZX9BC3TYF3PWGFGZJ7** The server writes at most 100 `denied`
  lines (`trace::DENIED_MAX`) in each window of 10 seconds
  (`trace::DENIED_INTERVAL`). Over the limit, it writes no line and
  does not read the body of the call. At the end of a window with such
  calls, it writes one line with the result `dropped`, the severity
  `WARNING` and their `count`. A stop of the server ends the window.
  The reply to a refused call is the same with a line and with no line.
- **01M419Z1RM0TDJ50F6SEJC40GB** The `dropped` line also has `counts`:
  a JSON object with the count of each code that lost a line, for
  example `{"no_token":900,"not_you":3}`.
- **01M419Z1V3YT48PR2NTFYWAXG9** The server keeps 20 of the `denied`
  lines of each window (`trace::DENIED_KEPT`) for a call with a valid
  token: the codes `not_you` and `bad_proof`. A call with another code
  takes a line only while the window has more than 20 lines left.
- **01M3X4Z675D0ZQX93E93F3M8FA** No line of a trace holds the body of a
  post, a token or a key.
- **01M3X4Z69CFV23V4QZBE8RP1GJ** The codes of a refusal are a fixed set.
  The status of the reply is 403 for `not_allowed`, `no_sign_in` and
  `not_member`; 409 for `held`, `paused`, `must_clear`, `not_holder` and
  `other_user`; 400 for `bad_request`. Each test of a refusal compares
  the code.
- **01M3X4Z6BKM251H7CS2CEGR205** A claim that takes the item of a holder
  that is gone gives a `released` record for the old holder, then the
  `claimed` record, in one chunk.
- **01M3X4Z6DSWKMJ2R549R4TSYP0** `Engine::finish` takes the proof of the
  write of a chunk (`Written`). Only the write of the log makes the
  proof. So only a written chunk reaches the written state.
- **01M3X4Z6G0TG0B4FT2N1FSPDHS** The role of a caller with no token
  comes from the trust of the riff: an admin in a riff with no sign-in,
  else a member.
- **01M3X9X9M079WGFPJZHNXH9VEP** The `start` call carries `reason`
  (`process`, `resume` or `clear`) and `worker`. A start makes one
  `released` record for each claim of the session, then a
  `session_started` record with the session, the reason and the worker
  mark. A `register` makes a `session_started` record with the reason
  `join` when the log does not know the session, or when the worker
  mark of the call is not the mark of the state. The `register` that
  the engine runs first keeps the mark of the state. So the worker mark
  is in the log, and not in the presence. A build reads a reason that
  it does not know as `other`, and such a record changes nothing. The
  start hook sends `process` for the source `startup`, `resume` for
  `resume`, `clear` for `clear`, and no start for `compact`. A person
  has no `session_started` record.
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
- **01M3X9XCSBR11ACD86FNXKF8JH** `forget` gives one `released` record
  for each claim of a session, then its `session_forgotten` record.
- **01M3X9XD8QWHS2CXTFSQK0PN1Y** The checkpoint holds the worker mark,
  the MustClear mark and the time of the last fresh start of each
  session. A start from a checkpoint gives the life cycle of a full
  replay. Superseded by 3.0 (01M4Z60B7NW4GHKGCYZZDG26R6).
- **01M4263ZXH4K23CSY6C5GJPVQH** After a start of `riff-server`, a
  session counts as seen at the later of its last call in the
  checkpoint and the last record of its own call. A record of the
  server, of a person or of another session only names the session,
  and does not make it newer: for example a record of the import of
  go-live, or a release by the lead. Only when neither is known does
  the last record that names the session count.
- **01M4263ZZVY8QJ2METTEVR1W26** The checkpoint holds the status of
  each session, with the time of its set. A start of `riff-server`
  gives each session its status again.
- **01M49NP8F3A9CTJWZ74MCNZG0M** The checkpoint holds the long step of
  each session in the same entry as its status: the name, the time of
  its start or its failure, and the reason of a failed step. A start
  of `riff-server` gives each session its step again, with its age
  from its first start. A checkpoint with no step loads.
- **01M4264028A3KVDK10PPERHM0C** On a shutdown (R129), `riff-server`
  writes a last checkpoint after it writes the log, also when no
  record came after the last checkpoint.
- **01M3XA875QZ584JBGA37853PWX** The people are state of the log: the
  riff ID, the email of each USER, the members, the admins that the
  owner made, the owner, a riff whose owner is gone, and the request
  for the owner role with its time. Only `apply` of the kinds
  `riff_made`, `person_joined`, `member_invited`, `member_removed`,
  `admin_set`, `owner_set`, `owner_asked`, `owner_denied` and
  `signins_ended` changes them. The checkpoint holds each of them, so
  they stay after a restart with a bucket. `who`, `members` and
  `GET /v1/sign-in` read them from the written copy.
- **01M3XA87NXAP6TEMB34QJ28HVP** Each change of the people is a command
  through `Engine::dispatch`: `admit`, `invite`, `remove`, `set_admin`,
  `pass_owner`, `take_owner`, `deny_owner`, `grant_owner`, `end_owner`,
  `name_owner` and `revoke`. Only a person sends `invite`, `remove`,
  `set_admin`, `pass_owner`, `take_owner`, `deny_owner`, `revoke` and
  `set_idle`. Only the sign-in sends `admit`. Only the server sends
  `grant_owner`, `end_owner` and `name_owner`. A riff with no sign-in
  refuses each command of the people with the code `no_sign_in`. A
  command that names a person who is not a member is refused with the
  code `not_member`.
- **01M3XA87F70CD3WH4STADSCW6S** The role of a caller with a token
  comes from the people of the pending copy, with the admins of the
  settings (R210). So a command that comes after a change of a role in
  the queue gets the new role. A query reads the people of the written
  copy. The admins of the settings, the public address and the times of
  the owner role are settings of the state: each view holds them, and
  they are not in the log.
- **01M3XA877YZQ649SWB5TN60V5P** The first step of a sign-in is the
  command `admit`. Its caller is the sign-in: the verified email of the
  provider. Then the token store starts the chain. `admit` makes no
  record for a person that the people know, so a second try after a
  stop makes no second record. A sign-in gets its reply after the write
  of the records of `admit` and the save of the token store. A revoke
  and a change of the people get their reply after the write of their
  records. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3XA87A9GGFA89RQXWSKY0V6** `signins.json` holds only the sign-ins
  and their chains. Each sign-in keeps the position of the pending copy
  at the check of its `admit`. After the write of a `member_removed` or
  a `signins_ended` record, the writer ends each sign-in of the person
  from before the position of the record. In the same step, the token
  store keeps that position for the person, and it starts no sign-in of
  the person below it. A load drops each sign-in below the last such
  position of its USER. So a stop between the write and the end of the
  sign-ins lets no removed person in. The reply to `remove` and to
  `revoke` has the number of sign-ins that ended. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3XA87HE06Z6M32ZJPSYSYRZ** Each riff has a riff ID. It is a field
  of the command `make_riff` and of its record `riff_made`: the first
  record of a new log. A new riff gets a random ID. `make_riff` makes
  `riff_made` only for a riff with no ID, and `apply` keeps the first
  ID. `GET /v1/sign-in` gives the ID of the written copy. A restart on
  the same bucket keeps it. A server with no bucket gets a new riff ID
  at each start.
- **01M3XA87KQ6ESMQ4W1W1PTW766** `apply` of `owner_set` stores only the
  owner, and ends the request that waits. A record with no email says
  that the owner is gone. `handle` keeps the old owner an admin and a
  member: it gives `member_invited` and `admin_set` for the old owner
  before `owner_set`, in the same chunk. `set_admin` for a person who
  is not a member gives `member_invited`, then `admin_set`. The notes
  of `take_owner`, `deny_owner`, `grant_owner` and `end_owner` are
  `posted` records in the chunk of the command.
- **01M3XA87CJHCGZX283ZQAFKARZ** No log line holds an email. An email
  is in a record, and in a reply to a member. The line of a refused
  command of the people has its code and no reason. The line of a
  sign-in names its USER.
- **01M3XGNZYD1E35DXYTHHJT1CR7** At a load, `riff-server` drops each
  sign-in that the log does not hold: a sign-in whose position is after
  the position of the log, and a sign-in of a USER that the people of
  the log do not know. So after a log that went back to an earlier
  position, for example after `riff-server log cut`, each sign-in that
  started after the new end is gone, and the person signs in again.
  Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3XGP011KGXDP9D1FNMT374F** `admit` checks first if the person may
  join, and then the USER of the email (R209). A person who may not
  join gets the refusal `not_member`, also when another email holds the
  USER. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3XGP03RDF6S15JYS718WWFC** The token layer gives the engine the
  position of the sign-in of each token. The engine refuses each
  command of a caller whose sign-in started before the last end of the
  sign-ins of its user in the pending copy, before `permits`: with the
  code `not_member` when the user is no member, else with
  `not_allowed`. So between the entry of a removal in the queue and
  the end of the sign-ins after its write, the removed person changes
  nothing, also not through a session. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).

- **01M3TBZBMMSMNWP126ZQED13YG** A checkpoint is one object of JSON.
  Its name is `checkpoint/`, the position in 20 digits, `-`, and the
  time of the write in milliseconds, for example
  `checkpoint/00000000000000001234-1790000000000.json`. It holds the
  format, the version of the build that wrote it, the state that the
  log gives up to the position, the last messages of each thread, the
  read cursors, and the last call of each session. A start loads the
  newest checkpoint that reads, and replays only the records after its
  position. When the newest checkpoint does not read, the start uses
  the one before it.
- **01M3TBZBQDF0ES4KM54FJQF6Z8** `riff-server` writes a checkpoint each
  1,000 records, or each 60 minutes when records came. It encodes the
  checkpoint outside the lock of the state. A build writes no
  checkpoint past the first record that it skipped. A build writes no
  checkpoint while the newest checkpoint comes from a later version,
  or does not read. The server keeps the last 3 checkpoints and the
  newest checkpoint of each day for 30 days. It deletes the others. It
  deletes a chunk only when each kept checkpoint is past it. No rule
  deletes chunks by age.
- **01M4YNCECQDQR3Z8QTGQ1PW3WV** The 1.x instance drops the forge state
  of 2.0. A replay drops each record of the kind `forge_allowed`. A
  dropped record is not a skipped record: the build writes checkpoints
  past it, and `riff server` does not count it.
- **01M4YNCEMNRCAXD37V03T4JQYR** Each major release line has its own
  instance. A start passes over each checkpoint whose `build` has a
  later major version: it does not use it, and it does not block the
  checkpoints of the build (01M3TBZBQDF0ES4KM54FJQF6Z8). A checkpoint
  of a later minor or patch of the same line still blocks.
- **01M3TBZBT7MME9BG1RWX5SZAZ6** Memory and the checkpoint keep the last
  200 messages of each thread. Nobody reads an older message. The copy
  check (01M3JEJVXXEPPNGT3FY4ZSFCWZ) knows only the kept messages.
- **01M3TBZBX140GJWCV5GZ73Q5Z5** `read` gives one page: at most 50
  messages. When more messages follow, the reply has the number of the
  last message of the page. An unread read again gives the next page.
  A read of all with `after` set to that number gives the next page.
  `riff read` and `riff read --all` read each page. The `read` tool of
  `riff mcp` gives one page, and says how to read the next.
- **01M51TKZSYR7PH629S29JX00ZD** A read marks as read only the messages
  that it shows. A read of all with `after` past the read cursor skips
  the unread messages of other sessions up to `after`, so it keeps the
  cursor. Those messages stay unread: the next unread read shows them,
  and the idle stop sees each request among them
  (01M49KT28N4B07P4G80Z74GRAH, superseded by
  01M59BRGZT6J45BWKPM9XGEPEJ). For example, the `read` tool with no
  thread gives `after` to each thread, so a direct message that came
  after the first page of a long repository thread stays unread.
- **01M58AG7Z2PETHBZG7X9QD8PK5** A read with no thread shows each
  direct thread first, then the other threads in the order of the list
  of the server. `riff read` and the `read` tool of `riff mcp` do the
  same. Only the part of 01M588J4XNV1TXEPMGTKRVY9VK comes before them.
- **01M3TBZBZVH907QD359AB8TBSX** Each hour, a timer of `riff-server`
  writes a `session_forgotten` record for each session with no sign of
  life for 30 days. It does not run in the first 3 minutes after a
  start. The record drops the session, its read cursors, its
  memberships, its claims, its lead, and each direct thread whose two
  sessions are gone.

- **01M3TJWHNYRCA7RTPFNYM5ZNQS** `riff-server log` prints each record of
  the log as one line of text: the position, the time, the kind of the
  change and its facts. `--from POSITION` names the first record.
  `--bucket` or `--dir` names the store. The command runs no server.
- **01M3TJWHRP49M66NYNHWSYD3XP** `riff-server log verify` reads each
  checkpoint, and each chunk from the oldest kept checkpoint. It names
  each object and each line that does not read, and each gap or repeat
  of a position. It does not stop at the first problem. For a problem
  in a chunk, it names the last good position and the cut that removes
  each record after it. It exits with 1 when it finds a problem.
- **01M3TJWHVN730ZWCWHT9ER186R** `riff-server log cut --after POSITION
  --yes` deletes each record and each checkpoint after the position.
  It prints each record and each checkpoint that it removes, and the
  threads of these records. It keeps the bytes of each line that
  stays. It removes a chunk whose header does not read. It deletes the
  chunks from the end of the log to its start, so a cut that stops
  leaves no gap. It refuses a position before the oldest kept
  checkpoint, when the log does not start at position 1.
- **01M3X342G8KF2W06PABGXTERMZ** `riff-server log cut` with no `--yes`
  removes nothing. It prints each record and each checkpoint that the
  cut removes, and the command with `--yes`.
- **01M3X342K007K3Z9G0CYWFKVMA** `riff-server log cut --yes` refuses
  while the lease is live (01M3X342DH98YEZ3X5CND43DGD). The refusal
  names the instance that holds the lease. A cut with no `--yes` runs,
  and names that instance. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3X5TP9CD4NXGPJEGP2Q4RS0** `riff-server log cut --yes` takes the
  lease before it reads the log: it writes an ID of its own to the
  lease, only when the lease is the version that it read. When an
  instance wrote the lease in between, the cut refuses and changes
  nothing. After a lease that ended by its age, the cut then waits 10
  seconds and reads the lease again. The cut ends its lease after its
  work, also after a refusal. When an instance took the lease during
  the cut, the command names the instance and exits with 1. A cut with
  no `--yes` does not write the lease. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3X342NQWZBJPS0GXV98BQME** `log verify` and `log cut` read each
  line of a chunk by its bytes, so a line that is not UTF-8 is one bad
  line. The good part of the log is its first records that read and
  have the right positions. `log cut` keeps only the good part, up to
  the position. It removes each line after the first problem, also a
  line with a lower position, and each later chunk. It prints only the
  lines that it removes. So the cut that `log verify` names repairs
  the log, and removes no record before the first problem. When the
  first problem is before the oldest kept checkpoint, `log verify`
  names no cut, and `log cut` refuses.
- **01M3ZWRC11R5M9V1KTF05P240W** `POST /v1/log` gives the records of
  one repository thread, for an audit. Only the owner and the admins
  can read them; each other person gets 403. The server reads the log
  from the store. A record of the repository is a post, a membership,
  a lead, a claim or a release in its thread; a direct message from a
  session in the repository; the start or the end of a session in the
  repository; a pause of the riff or of the repository; and a hold or
  a free of an item of the repository.
- **01M3ZWRC3XBFN8FJDGE8XWZ5EA** A post in the reply of `POST /v1/log`
  has no text of its body, no signature and no payload. Its body is
  only its mark: `request`, `verify request` or `verify result` when
  the body starts with that word and a colon, else empty. The kind,
  the sender, the `to` and the time stay.
- **01M43GSMZKCMET3DG07K538EDD** A reader of the reply of
  `POST /v1/log` skips a record of a kind that its build does not
  know. So `riff audit` of one build reads the log of a later server.
- **01M3TJWHYB9FTZ3G8G227V0N05** With a bucket, a tool of the log takes
  its access token from the metadata server of Cloud Run. When that
  server does not answer in 2 seconds, the tool takes the token of the
  Google sign-in of the person: the output of
  `gcloud auth print-access-token`. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M48VFX22S4811DYBBD7QDW24** `riff-server` runs each command with a
  call ID one time only. The call ID is in the header `riff-call`. The
  key of a call is its caller and its call ID. The writer keeps the
  result of each accepted call: its records and its note. A second try
  of a kept call runs no `handle`. Its reply comes from the kept
  result, on the written state of now, and has the header
  `riff-repeat: 1`. A query and a signal take no call ID.
- **01M48VFX8K93F8XRWDB2BDP240** A second try of a call whose first try
  waits for the writer waits for the same entry. Then it gets the reply
  of the kept call.
- **01M48VFXBGBW3PTC2JNHYNSE0W** A refused command keeps no key. A
  second try of it runs `handle` again.
- **01M48VFXEE2GGT7JE10DWBNZEV** The server keeps a call for
  `CALL_KEEP` (24 hours), and at most `CALL_KEEP_MOST` (1024) calls of
  each caller. The oldest call goes first.
- **01M48VFFY5CK9MRXJESV2NHY5F** The envelope of each record of a
  command with a call ID has the field `call`, next to `by` and
  `command`. A record with no `call` is of a command with no call ID.
  A build that does not know the field skips it.
- **01M48VFXHHND8SX4DBXZTFMJGQ** A checkpoint keeps the records of each
  kept call, in the field `calls`. A load makes the kept calls from the
  checkpoint and from each record after it, with the default note. So
  a start from a checkpoint and a start from the full log give the
  same reply to a repeated call. A command that made no record is not
  kept after a start.
- **01M48VFXMHBT491BA5XYSRB3BA** A rollback to a build with no call ID
  loses the kept calls. For `CALL_KEEP` after it, a repeated command
  can run two times. The steps of a rollback say so.

## Cloud

Most requirements of this section hold for riff 1.x only, until
riff 3.0 replaces them. Each of them ends with "Superseded by 3.0
(ID)."

- **R5** `riff-server` runs on Google Cloud Run. It runs with
  `--require-sign-in`. For now, its public URL is its Cloud Run URL.
  The book never names that URL. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **R181** For now, no shared server runs, and `CLOUD_DEPLOY` is not
  set. Each person runs `riff-server` on their own machine. The shared
  server runs again when the open issues are done. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **R6** Later, its public URL is `https://riff.comotechnologies.io`.
  Cloud Run then maps the domain to the service. Google manages the
  certificate. Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **R29** Only one instance of `riff-server` serves at a time. An
  instance is one running `riff-server` process. Cloud Run keeps one
  instance, with its CPU on also between calls. During a deploy, a
  second instance runs for a short time. The lease (R137) stops one of
  them. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **R130** An instance takes up to 1000 calls at a time. Each open
  stream is one call. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **R131** Cloud Run ends each call after 60 minutes. `riff watch` and
  `riff tail` then connect again. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **R132** `riff` tries a call again after a fault, while the budget of
  the call lasts (01M4A803Z4Q0KX6NT1KC6QR43H,
  01M4A8041F8EK1VYDE4C9QG8N8). A 503 of the server is a fault.
- **01M3THEE5V3RFHF9QTA8MA8QDF** When a call waits for more than 1
  second while the server replies 503, `riff` shows one dim line
  `(waits for riff-server…)` on stderr. It shows the line one time for
  each gap, also with more than one call. `riff chat` shows the line
  above its prompt. `riff top` keeps its table.
- **01M3TJWJ9914B7Z5EQJF310REK** `riff` tries a refused connect to a
  loopback address again only when its process got a reply from that
  server before (01M4A804683G1EXM53893VHW7S). It then waits as for a
  503 (R132), and shows the same line. A process that got no reply from
  the server fails at once.
- **R148** `riff watch`, `riff tail`, `riff chat`, `riff top` and
  `riff workers host` connect a stream again at once when it ends. The
  open of a stream is one try. When it fails, they try again after
  `STREAM_RETRY`, 5 seconds. They stop only when the person stops them.
- **R133** `riff` uses `http://127.0.0.1:7878`, the server on the same
  machine, when no server is set. `--server` or `RIFF_SERVER` names
  another server, for example the shared server (R5).
- **01M3K0Q7X3FEKWZK0B3854C4RV** `riff` finds its server only in
  `--server` or `RIFF_SERVER`. It keeps no chosen server in a file. A
  person sets `RIFF_SERVER` in the profile of the shell, so each new
  session uses it.
- **01M3K0Q80BCZQD7DNQQ333ZN09** `--server` and `RIFF_SERVER` take a
  URL, `HOST` or `HOST:PORT`. With no scheme, `riff` uses `http://`,
  and port 7878 when there is no port. A bare IPv6 address gets
  brackets in the URL.
- **01M3K0Q854K18DGXJKQ427W586** Replaced by 01M3Q5VE74608N5H2M73RB6Y2Z.
- **01M3Q5VE74608N5H2M73RB6Y2Z** `riff server` shows the server that
  `riff` uses, and where that choice comes from: `--server`,
  `RIFF_SERVER` or the default. For that server, it shows if the
  server answers, its build and the sign-in. 01M3NTEMQAY1Z10H1GX2K6PEAH
  says when it shows the server of the same machine.
- **01M3NTEMQAY1Z10H1GX2K6PEAH** `riff server` shows a short aligned
  table, one fact on a line: the release of `riff` with its commit and
  date short, the server with where it comes from, its release and the
  sign-in. The server of the same machine shows only when it answers or
  is the server of `riff`. When the person must act, the last line says
  what to run: yellow when the versions can talk, red when they cannot
  or when the person must sign in. `--color` works as in `riff tail`.
- **01M56Q55SHHJSKJWNHMW79X7HA** The last line of `riff server` and
  the note of each `riff` process say `riff update` only when the
  server runs a newer build than `riff`. When `riff` is newer, also on
  the same line, they say "Update riff-server when you can.", or
  `riff server` says "Update riff-server." when the versions cannot
  talk.
- **01M3Q5V313XQN86BA2PBTXHEZC** Each `riff` command that shows facts
  or a list has one look. Facts are `key  value` lines, aligned. A
  list is a table with a header row. A row shows a short session ID
  (8 characters). `--long` shows the full URI or session ID in its
  place; a row never shows both. A status is on the row of its
  session, in its own column. A setting shows as `key  value  (file)`,
  and one dim line at the end says how to change it. An action that
  the person must take comes last, in yellow or red.
- **01M3Q63NQQT3G30GMS55FY4499** `riff whoami` shows the facts
  `session`, `uri`, `riff` and `build`. When riff cannot read the
  state of the riff, the fact `riff` says `unknown` and the reason, in
  red.
- **01M3Q5VE4VVXT9FH4J4MAWX68V** `riff` reads `RIFF_SERVER` itself. A
  usage error of a `riff` command does not name `--server` as a
  required argument, also when `RIFF_SERVER` is set.
- **01M3MRMB2M0RM1JFVJ0W695SHB** `riff server` shows the release of
  `riff`, and the release of each server that answers with a build.
- **01M3MX598VTWZ02R7J6AYJB2E5** Only the network counts in the wait
  of `riff server` for an answer. A slow keyring does not make a server
  that answers show "no answer".
- **01M3MNT26K77E4RDHH42SB5AEG** Two server URLs name one riff when
  they have the same port, and the same host or two loopback names,
  for example `localhost` and `127.0.0.1`. `riff server` shows one
  riff once.
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
- **R32** The OIDC client secret is in Secret Manager. Cloud Run gives
  it to `riff-server` as `RIFF_OIDC_CLIENT_SECRET`. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **R134** `riff-server` runs as its own service account. The account
  can read and write only its bucket, and read only its secret. The
  bucket is private. Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **R135** The image holds only the `riff-server` binary and CA
  certificates. It runs as a user that is not root. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **R151** Cloud Build builds the image as its own service account.
  That account can only build and store images. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **R160** Replaced by 01M3NJAZ6BYH7TWKDYTVEK78PG.
- **01M3NJAZ6BYH7TWKDYTVEK78PG** CI deploys `riff-server` when the
  repository variable `CLOUD_DEPLOY` is `true`, in two cases: a push of
  a release tag `vX.Y.Z`, after the job `Release check` of the tag
  passes; or a person runs the CI workflow on `main` with the input
  `tag`, after the gate passes. CI builds the image of that release,
  pushes it to the image repository of the project, and deploys it.
  One deploy runs at a time. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M3MMZQ3KTF5Z3GXNR7DRQ65Z** Replaced by 01M3NJAZ8HSE87H0GZ8SNGWN31.
- **01M3NJAZ8HSE87H0GZ8SNGWN31** A push to `main` never deploys the
  shared server. A tag that is not `vX.Y.Z` never deploys. The release
  tag at the end of a wave deploys the release (R216). A run by hand
  is for a redeploy or a rollback to an older release. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M3MRMASMP59PKHAV92XSV7XE** Replaced by
  01M3N73AW2J3TVSZWFJ88A91PG.
- **01M3N73AW2J3TVSZWFJ88A91PG** A release is a git tag `vX.Y.Z`. X.Y.Z
  is the version of the crates in `Cargo.toml` and `Cargo.lock`. The
  rules 01M3N73E5JWFTYZ90JX5AVFGEP to 01M53N7KN4QKZ70W2GNQ3VQWQ9 pick
  the level of the release. CI fails a pushed tag `v*` whose version
  is not the version of the crates.
- **01M3MRMAVVKJ5WS8GWCJHWH0R4** `riff update` installs a release, not
  the head of `main`. With `--tag vX.Y.Z`, it installs that release.
  With no `--tag`, it installs the release that the server of `riff`
  runs, from the version of its build, also when a newer tag exists.
  When `riff` uses the server of the same machine, it installs the
  newest release tag. Superseded in part by 01M510MPR1S0EQNRK2195XTXG5.
- **01M3N73Y9DMVMCV0PJE1R8YCFH** When the server of `riff` answers
  with no build that `riff update` can read, `riff update` installs
  the newest release tag, and says so in one line. When the server
  does not answer, `riff update` installs nothing and names `--tag`.
  Superseded in part by 01M510MPR1S0EQNRK2195XTXG5.
- **01M510MPR1S0EQNRK2195XTXG5** Where 01M3MRMAVVKJ5WS8GWCJHWH0R4 and
  01M3N73Y9DMVMCV0PJE1R8YCFH say the newest release tag, `riff update`
  installs the release that GitHub marks "Latest"
  (01M4YTW8788K4XYTEMRSXDH7B9), not the highest tag. It asks the API of
  GitHub, with no sign-in. With `RIFF_RELEASES`, it reads the file
  `latest` there, in the form of the reply of the API. When it finds no
  Latest release, it installs nothing and names `--tag`.
- **01M510MPTKK285C9Z2TW54M5P7** Replaced by 01M56N6KSMFJ46PW99Z80GF9DM.
- **01M56N6KSMFJ46PW99Z80GF9DM** The GitHub releases of a retired line
  stay normal releases. When a line retires, no release of it becomes a
  prerelease, and its notes do not change. The line 1.x retired after
  riff 3.0.0 (#961): its 12 GitHub releases are normal releases, and
  GitHub marks `v3.0.0` "Latest". The line 2.x retired before this rule
  (#706): its one GitHub release, `v2.0.1`, is a prerelease. The tag
  `v2.0.0` has no GitHub release.
- **01M550XYBAW5QYXEMQ49Z34M4N** Replaced by 01M56N6KVZRGT3W7CEK05DHR57.
- **01M553AWMXYAG7CJGW7M30XDZ8** Replaced by 01M56N6KVZRGT3W7CEK05DHR57.
- **01M56N6KVZRGT3W7CEK05DHR57** riff has no command that retires a
  release line. `riff --help` lists no `retire`, and `riff retire`
  fails: riff does not know the command.
- **01M3MRMAY3P1K151RGAP9K6GSH** The deploy takes a release tag as its
  input. It checks out that tag and deploys only it. It refuses an
  input that is not a tag `vX.Y.Z` of the version of the crates.
  Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M3MRMB0AJVPD952AQYD7X1RN** Only an admin of the repository makes
  a release: the tag ruleset `releases` on `v*` lets only the
  repository admin role create, move or delete a tag. `just github`
  makes it.
- **01M3NB3EWE2V9PCTMNTZAKEXMA** Each release has a GitHub release on
  its tag. Its notes start with the level line of the release pull
  request and the command that each person runs. GitHub generates the
  rest: each pull request merged since the last release, except the
  pull requests with the label `release`. The GitHub releases are the
  changelog. The repository has no changelog file.
- **01M40AB1D5KQ4SERC6M709Z3JF** The generated notes of a release put
  the pull requests in three groups, in this order: `Changes` (each
  pull request with neither the label `design` nor `internal`),
  `Design` (the label `design`) and `Internal` (the label `internal`).
  `.github/release.yml` sets the groups.
- **R161** Replaced by 01M3NJAZAQ3AKMAM0EGM7R3S89.
- **01M3NJAZAQ3AKMAM0EGM7R3S89** CI signs in to Google Cloud with the
  OIDC token of GitHub. No key exists. Only the `main` branch and the
  tags `v*` of the repository can sign in. The deploy account can push
  images, deploy the service, and run it as `riff-server`. Superseded by
  3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M49M8W30M2084QN4HX1FJFKS** Only a CI job in the GitHub
  environment of an instance (`CLOUD_GITHUB_ENVIRONMENT`) signs in as
  its deploy account: `production` for the shared riff, `stage` for the
  stage. No deploy account takes each job of the repository. The
  binding names the attribute `environment` of the job, not its
  subject. `riff cloud create` sets it. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **R152** Cloud Run lets each caller in. `riff-server` checks each
  token itself (R5). Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **R143** The Google Cloud project `como-riff` holds each cloud
  resource of riff. It holds nothing else. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **R144** `deploy/cloud/shared.env` holds the cloud settings and the
  OAuth client ID of the shared riff. The repository is public. No file
  in it holds the account data of a real person: an email address, a
  billing account ID or an organization ID. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **R136** A person makes the project and links its billing account with
  gcloud, by the how-to in the book. `riff cloud create` makes the
  resources of riff in the project. It checks each resource first, so it
  can run again. `riff cloud deploy` with no tag builds the image and
  deploys it to Cloud Run. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M3TJWJEPTSF1S3S5PJD25Z7Y** The bucket is a standard bucket with
  object versioning. A lifecycle rule deletes each older version of an
  object after 7 days. The service has 1 GiB of memory.
  `riff cloud create` sets each of them, and each deploy sets the
  memory. Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M3TJWJ6J3M6JRXJTAETZ5M6F** `riff cloud create` makes an alert in
  the cloud project: a log line of the service with the severity
  `ERROR` or more sends an email to the owner, at most one each 5
  minutes. The email comes from `RIFF_OWNER`. With no `RIFF_OWNER`,
  the setup makes no alert, and says how to make it.
  `riff cloud log NAME --errors` shows these log lines. Superseded by
  3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **R145** A person makes the OAuth client by hand in the console, with
  the how-to in the book. `riff cloud signin` puts the client secret in
  Secret Manager and the client ID in the settings file. The secret is
  never in the repository or in a downloaded file. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M3ZE3Z26N1CG090D5D5FZ3NW** Replaced by 01M496JTN962N0AX378MA1MBPM.
- **01M496JTN962N0AX378MA1MBPM** The stage is a second riff-server in
  the project, for the rehearsal of a release and the check of each
  merge. `deploy/cloud/stage.env` holds its settings. It has its own
  service, bucket, accounts, sign-in client and secret, and no alert
  or domain. No setting of one riff names the bucket, the service, the
  secret or another resource of a different riff. A test proves it.
  Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M3ZE3Z580RB5AYAJX6321DFW** `riff cloud` takes the settings of a
  riff by name: `stage` reads `deploy/cloud/stage.env`, and `shared`
  reads `deploy/cloud/shared.env`, the shared riff.
  `riff cloud deploy NAME TAG` deploys the image that CI built for the
  release tag. No `riff cloud` command changes the GitHub variable
  `CLOUD_DEPLOY`. Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M3ZE3Z80274JFTN53DNJ2F2G** A release that moves the state of the
  shared riff is rehearsed on the stage before its tag, by the how-to
  in the book. The results go on its issue. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M3ZGRZ0F3G93Q3ET9GGWKT7E** `riff cloud log` gives its filter to
  `gcloud` whole, with its spaces and quotes. With no `--limit`, it
  shows the last 50 lines. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M4262DQ9RNFNJ07CRTSGEAM1** `riff cloud` makes and runs
  riff-server instances on Cloud Run with the `gcloud` of the machine,
  as `riff pr` runs `gh`: `create`, `signin`, `deploy`, `list`,
  `status`, `log` and `delete`. The shared riff and the stage take the
  same code path. The repository has no cloud scripts and no
  `just cloud`. Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M4262DSKVWP4064FKSMAQACZ** Each instance has one settings file
  `NAME.env`: in `deploy/cloud` of the repository of the directory when
  it has that folder, else in `cloud` beside the riff settings of the
  machine. `riff cloud create NAME --project P --region R` writes the
  file of a new instance. Its service, bucket, accounts and secret take
  the name of the instance, so its bucket has its own riff ID.
  Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M4262DVY8QCSZS61VDQ61SB3** `riff cloud delete` asks for the name
  of the instance first. `riff cloud deploy` asks for it when the
  settings have `CLOUD_CONFIRM=true`, as the shared riff has. With no
  terminal, only `--confirm NAME` gives it. `delete` keeps the bucket
  unless `--with-state`. Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M4262DY8NN30SC4REYX2G9DV** Replaced by 01M558C4VZ2PSMEYPAZBRYW1VV.
- **01M558C4VZ2PSMEYPAZBRYW1VV** These riff commands write outside the
  session that runs them, to the riff, the forge or the cloud: `riff
  workers start` (01M3JPQT79FE47518Z8DFFQYYG), `riff retire`
  (01M553AWMXYAG7CJGW7M30XDZ8), and the writes of `riff cloud`:
  `create`, `signin`, `deploy`, `smoke` and `delete`. Each one refuses,
  and changes nothing, in a worker (`RIFF_WORKER=1`) and in an agent
  session that is not the lead of its user. They use one check in code,
  and this check is the guard. A person in a plain terminal and the lead
  can run them. The flag settings of a worker deny the plain shape of
  each write of `riff cloud` (`riff cloud WRITE ...`) as a second layer.
  A write with a global flag before it passes these rules, and the check
  in code refuses it. Each session can run the reads of `riff cloud`:
  `list`, `status` and `log`. Superseded in part by
  01M56N6KVZRGT3W7CEK05DHR57: riff has no `riff retire`.
- **01M4262E0QJHCZXNH7EFG7FXN2** The CI deploy job deploys with
  `riff cloud deploy shared TAG --confirm shared`, from the riff of the
  tag. Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M496K1KN392S11YEH33JGK9N** The CI deploy job of a release runs in
  the GitHub environment `production`. It waits for the approval of the
  reviewer of that environment, the owner, and deploys only after it.
  Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M496JT94NQ686GVSY5CCGZK7** Each push to `main` deploys to the
  stage in CI, when the repository variable `STAGE_DEPLOY` is `true`,
  after the gate of the push passes. The job runs in the GitHub
  environment `stage`, with no approval, and signs in to Google Cloud
  as the deploy account of the stage. One stage deploy runs at a time.
  Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M496JTDB16G52G22CJZRA8J0** The stage job tags the image of a push
  with the full ID of its commit, and deploys it with
  `riff cloud deploy stage COMMIT`. Only an instance with
  `CLOUD_CONFIRM=false` takes the image of a commit. The shared riff
  takes only a release tag. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M496JT648QS9WTE1QVHAJEE5** The setting `CLOUD_MIN_INSTANCES` is
  the least number of instances of a service: 1, or 0. With no value it
  is 1. The shared riff has 1. The stage has 0, so it scales to zero
  between calls. Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M496JTHN19BZ7YN94993R35X** After each stage deploy, CI runs
  `riff cloud smoke stage`. It signs in as a test account with the
  refresh token of the provider in `RIFF_SMOKE_TOKEN`, a secret of the
  environment `stage`. It registers a session, posts a message to the
  thread `riff/smoke`, reads it back, checks that the server runs the
  build of the `riff` that runs the test, and ends the session. Each
  step prints one line. A failed step stops the test with its name and
  a code that is not 0. Only the stage admits the test account.
  Superseded by 3.0 (01M4Z60B7E6X53NBJ01R95RYGE).
- **01M4382RKERWAPKBRY9W8F2GSA** When a `gcloud` call of `riff cloud`
  fails, riff prints the error of `gcloud` in one line and exits with a
  code that is not 0. An ended sign-in gives
  `gcloud: the sign-in ended: run gcloud auth login`. riff says that a
  resource is missing only when `gcloud` replies that it does not
  exist. A reply that the account has no permission is an error, also
  when it says that the resource may not exist. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).

## One instance

The requirements of this section hold for riff 1.x only, until
riff 3.0 replaces them. Each of them ends with "Superseded by 3.0
(ID)."

- **R137** The lease is an object in the bucket. It holds the ID of the
  instance that may serve. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **R138** At start, an instance loads its state
  (01M3THEE08ZKV8WGHDSVWV69ZE). Then it makes a random ID and writes it
  to the lease. It then waits 15 seconds, applies the chunks that came
  since the load, and starts to serve. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3THEE08ZKV8WGHDSVWV69ZE** Replaced by 01M52JYGHHK5X0XS39VFT3ZGGM.
- **01M52JYGHHK5X0XS39VFT3ZGGM** The start order of an instance with a
  store is: load, then the port, then the lease. The load reads the
  sign-ins, the newest checkpoint and the log after it, with no lease.
  When the load fails, the instance exits. It opens no port and writes
  no lease, so the old instance serves on. After the wait for the
  lease, the instance applies the chunks that came since the load, and
  loads the sign-ins again. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3TJWJC08ZR5TWA1Y9CDE0QM** After the wait for the lease, the
  instance also lists the checkpoints again. When a checkpoint came
  since its load, it takes the position of the newest one. It writes
  no checkpoint when that one comes from a later version, or does not
  read (01M3TBZBQDF0ES4KM54FJQF6Z8). Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M51X8MG0YQT5KBZC9H7917J3** When an instance reads the ID of
  another instance in the lease, it stops as in R140. Then it writes a
  last checkpoint at its written position: the hand-over. It does not
  prune. It writes none when the lease holds the ID of a tool, when the
  holder ended the lease, when its own shutdown began, or when it
  writes no checkpoint (01M3TBZBQDF0ES4KM54FJQF6Z8). A shutdown waits
  for the end of that write. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M53T8XCTVEXGJVF9GSYVAT7Y** At the hand-over, before the
  checkpoint, the instance also saves the token store when it changed
  since its last save. The save names the version that the instance
  knows (R141). It does not wait for the second of R127. It has a time
  limit of half the wait for the lease. A save that waits for its
  second when the instance stops ends at once. So a token from the last
  second of the old instance works on the new instance: the new one
  loads the sign-ins again after its wait. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M51X8P4M8PHSSJMWF1J1KQS1** After the wait for the lease, when a
  checkpoint came since the load, the instance loads its state again
  from the newest checkpoint that reads, and replays the log after it.
  So the read cursors, the statuses and the last calls of the old
  instance stay after a deploy. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3THEE31H5QVV3JAFC4ZRGFR** Replaced by 01M52JYGM7WH565B7PCVBEJC17.
- **01M52JYGM7WH565B7PCVBEJC17** `riff-server` opens its port after the
  load, before it takes the lease. Until it serves, it replies 503 to
  each call, with the build of riff-server. So at an instance change,
  Cloud Run always has an open port for the calls. Before the end of
  the load, each connect is refused. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **R139** An instance reads the lease every 2 seconds. It serves only
  for 5 seconds after the last read that showed its own ID. Else it
  replies 503. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **R140** Replaced by 01M52JYGPX5ETP3HKR0S0Y0YCR.
- **01M52JYGPX5ETP3HKR0S0Y0YCR** An instance that reads another ID in
  the lease stops for good. It closes each stream, replies 503 to each
  call in flight, and saves nothing more, except the checkpoint and the
  token store of the hand-over (01M51X8MG0YQT5KBZC9H7917J3,
  01M53T8XCTVEXGJVF9GSYVAT7Y). It closes its port at once,
  so it takes no new call, and Cloud Run sends each call to the new
  instance. It exits when the hand-over is written, at most 10 seconds
  after the stop. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **R141** Each save of the token store names the version that the
  instance knows. Each write of a chunk is only when new
  (01M3T411BZQB8N4D2S0JFVESMS). When the bucket holds another version,
  or a chunk with the same name, the save fails. The instance then
  stops as in R140. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **R155** An instance saves and writes only in the time that R139
  gives it to serve. The writer checks it before each try of a chunk.
  Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **R156** A new instance starts to serve at a whole second. When the
  lease shows another ID after its wait, it exits and does not serve.
  Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M53NHJXGK1PN2A9F7NSX0B4G** The lease also holds the revision of
  its instance: the number of its Cloud Run revision, from
  `K_REVISION`. The number gives the order of the deploys. An instance
  of an older revision does not write over a live lease of a newer
  revision. It replies 503, reads the lease every 2 seconds, and takes
  the lease only when that lease ended. When the instance or the holder
  has no revision, the newest instance takes the lease (R138).
  Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M53NHK07KNXJ7FXS26NB1675** Replaced by 01M56T4SZ4Y6PSY1VTVA3758FT.
- **01M56T4SZ4Y6PSY1VTVA3758FT** A SIGTERM before the end of the wait
  for the lease ends the start of an instance at once. The instance
  ends the lease that it took, closes its port and exits with success.
  It saves nothing: it has no state to save yet. So an instance of an
  older revision takes the lease at its next read. When the end of the
  lease fails, the lease ends by its age. After a crash, the lease also
  ends by its age. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3X34282SG0DJ6X34F90HS26** The lease also holds a time. An
  instance writes the time when it takes the lease, and again every 30
  seconds. While a write of the time that is due fails, the instance
  does not serve (R139). Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3X5TPBMF81TDVZ7Q4NVXBQX** An instance whose last good write of
  the time to the lease is 90 seconds old stops for good, as in R140.
  It does not read or write the lease again, and it does not serve
  again from its state in memory. It counts the 90 seconds on the
  monotonic clock and on the wall clock. `riff-server` then exits at
  once with an error that says why, so that a new instance loads the
  state from the store. A shorter fault gives 503, and the instance
  goes on. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3X342ARX5Y7R9ZJDT12R9A1** On a shutdown (R129), an instance that
  holds the lease marks the lease as ended, after it saved. Superseded
  by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3X342DH98YEZ3X5CND43DGD** A lease is live until its instance
  ends it, and for 90 seconds after its time. A tool reads the time
  with its own clock, which can be at most 50 seconds ahead of the
  clock of the instance. A lease with no time is from an older build.
  It is live until a person deletes the object. The refusal of a tool
  names the object. Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3X342RCXX2GGK879VYK06TS** The shared server runs with CPU always
  on and with exactly one instance. So an instance writes the time to
  the lease also when it gets no call. Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).

## Sessions

- **R8** A new message can wake an idle session.
- **R9** A claim ends 5 minutes after the last sign of life of its
  session (R204), unless the same process comes back first, for
  example after a short network fault. A session that waits for its
  user keeps its claims. A claim ends at once when its session ends
  (R205) or starts again (01M3JEE1QQCFS5TMZW5N2DAD2D). Superseded by 3.0
  (01M4Z60B8FQ3GMWQH12CXE0JS9).
- **01M3JEE1QQCFS5TMZW5N2DAD2D** A new start of a session is blank: a
  new agent process, a resume or a `/clear`. The start hook sends a
  start call, and the claims of the session are free at once. The
  session keeps its ID (R58, R167, R168), its threads, its read cursors
  and its lead. A compaction is not a new start.
  Superseded by 4.0 (#1173).
- **01M3JEE1SWR05DWQA5WQ8AXFTF** The context of a new start names each
  claim that the start freed. It tells the session to claim an item
  again before it goes on with it, and to pick up the earlier work
  (01M3JEE1W32CMQP8CP2HJ829E7).
- **01M3JEE1W32CMQP8CP2HJ829E7** When a session takes an item, it looks
  for the work of an earlier session on the item before it starts: a
  pushed branch, or a worktree on its machine with no live session. It
  goes on from that work, or starts again. Its start post says which,
  and why.
- **01M3JY13Y75S9S0SMK5XQ529AD** When a session starts an item again, it
  deletes the pushed branch of the earlier work first
  (`git push origin --delete worktree-ITEM`). Its start post says so.
- **01M3WFYEKTWVVZ1FWVNQMGBNN0** A session commits its work as a WIP
  commit and pushes its branch before each long run (`just ci`, a test
  loop, a build) and at each change of step. A WIP commit has `WIP` in
  its subject. The pull request merges with a squash, so no WIP commit
  shows on the default branch. The skill says so in the start routine
  and in "Push your work as WIP". A WIP push needs no rebase
  (01M3MNP39172Y463WGQAW125KW).
- **01M3WFYEP1H3VPW8G90KQDE6FW** A session that claims an item with a
  pushed branch, or with a worktree of a session that is gone, goes on
  from that work. It first commits the files of that worktree that are
  not committed, and pushes them. It starts again only when the earlier
  work is wrong. Its start post says what it found.
- **01M3WFYER9QWA698KY2E1HNTCW** The answer to a granted claim names
  the earlier work on the item, from `riff claim` and from the `claim`
  tool: each branch of `origin` with its last commit, and each worktree
  on the machine with the count of its files that are not committed
  and of its commits that are not pushed. riff runs
  `git fetch --prune origin` first, for at most 5 seconds. A branch or
  a worktree belongs to the item when its name holds the item as a
  whole word. A verify claim gets no such line. riff does not show the
  subject of a commit.
- **01M3WFYETKXPWWE0R0EAKGCD1E** The context of a new start lists the
  earlier work of the clone that no live session owns: at most 8 items,
  each with its pushed worktree branch and its worktree. A live session
  owns the work when it holds the claim of the item, or works in its
  worktree. The fetch of the start hook prunes
  (01M3JN21T9C5GX6VX8N032JYWE). Superseded by 4.0 (#1173).
- **01M3ZT825YAA5FW85YXH7JR1K0** riff counts no commit of a worktree as
  not pushed when the default branch of `origin` holds its work: a
  merge of `HEAD` into that branch changes nothing. This is so after a
  squash merge deleted the branch of the worktree.
- **01M3ZT8296G8DZFRSKYM6V5XTH** The skill has one WIP block, in "Push
  your work as WIP". "Pause" and "Pick up dropped work" use it. Its
  push works after a rebase, and fails when it would drop newer work
  of the pushed branch (`--force-with-lease --force-if-includes`).
  When the push of an earlier worktree
  fails, the pull names the branch:
  `git -C PATH pull --rebase origin BRANCH`. Superseded by 4.0 (#1173).
- **R35** A session has a URI:
  `riff://USER@HOST/OWNER/REPO?session=ID&lead=true&claim=ITEM#WORKTREE`.
  It shows who the session is, where it works and what it works on.
- **R36** Who: USER comes from the sign-in. ID is the session ID of the
  agent tool. For Claude Code, this is `CLAUDE_CODE_SESSION_ID`.
- **R157** When `riff` cannot read the sign-in from the keyring, the
  command stops and says why. `riff` uses `USER` only when there is no
  sign-in. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R158** When `riff` cannot open the keyring, the command stops,
  unless `RIFF_USER` is set. With `RIFF_USER`, it sends no token.
  Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R226** When `riff` gets no token, it asks `riff-server` if it has
  sign-in. At a riff with no sign-in, the command stops and says to run
  `riff logout`, while this machine keeps a sign-in for that riff. When
  that sign-in is gone, it says to start the agent session again, or
  to run the command again. `riff` keeps the old sign-in until the
  person runs `riff logout`. At a riff with sign-in, the riff ID finds
  an old sign-in (01M3JNVBRS35B3CD67367JF7SJ). Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3JNVBPMZ1K9WX7Q7DP6Y0DH** Replaced by
  01M3XA87HE06Z6M32ZJPSYSYRZ.
- **01M3JNVBRS35B3CD67367JF7SJ** `riff` keeps the riff ID with each
  sign-in. Before its first token in a process, `riff` compares it with
  the riff ID of the server. When they differ, or the sign-in has none,
  `riff` removes the sign-in, and the command stops with "This riff is
  new. Run riff login." The next command runs with no sign-in. When
  `riff` cannot ask the server, it goes on. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R227** No error tells a person to run `riff login` at a riff with
  no sign-in. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R159** A session keeps the user of its first call. The server
  refuses a call with a known session ID and another user, with status
  409. The reply tells the session to set `RIFF_USER` or to sign in.
- **R55** Where: HOST, OWNER/REPO and WORKTREE come from the machine and
  from git. They are true at the start and change with `move`.
- **R56** What: the URI has one `claim` part for each claim that the
  session holds. It has none when the session holds no claim.
- **R175** Each person has at most one lead session in each
  repository. The lead is the session that the person works in. The
  URI of the lead has `lead=true`, so `who` shows the lead.
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
- **R228** The lead conducts the other sessions of its person in its
  repository. It never sends a request to a session of another person.
  The people of a repository agree among themselves: riff picks no
  lead for all people.
- **01M3JD5QCVCB3VEK4KP955JSEA** The lead takes no claims: no work
  item and no verify. A verify request waits for a free session that
  is not the lead. The skill teaches this to the lead.
- **R229** The skill tells the lead how to conduct: see the claims and
  the status of each session of its person (`who` and a status
  request), give each free session one clear item with `tell`, check
  the progress, answer questions (R180), and give a blocked session an
  answer or a new item. Superseded by 4.0 (#1173).
- **R230** A request is a direct message from the lead of the person
  of a session. A verified request (R199) counts as a scope from that
  person. A request that is not verified, or that comes from a session
  that is not the lead of the person, is advice (R10). A scope from the
  person wins over a request.
- **R231** A session that does a request of its lead reports back to
  the lead with `tell lead`: when it starts and when it finishes. When
  it is blocked, the `blocked` tool tells the lead
  (01M41FZPGEK4TNPSM2051W4VMS).
- **R232** A session asks only the lead of its own person for a
  decision. `tell lead` picks the lead of the person of the sender.
- **01M3JDW9WN7KFGVY6HMCP2XN8B** A session that is not the lead never
  asks its person in its own terminal, also when a permission refusal
  blocks it. It asks with `tell lead`, and names the refused action.
  The lead shows the question to the person.
- **01M3JDW9YQQHSZC296ZCNV2V8A** A permission refusal of a step of the
  pull request, for example a push of the branch or `gh pr create`, is
  a question for the person. The session tells the lead the pull
  request, the commit and the verify result. The session never asks to
  push to the default branch (01M3JFEXJG2D651PWA30DNRGWF).
- **R178** A lead counts only while it holds (R9) and works in its
  repository. A lead that comes back counts again, unless another
  session became the lead. A lead that leaves the repository thread is
  not the lead any more. While no lead counts, the person has no lead
  in that repository.
- **R37** The main worktree has no `#WORKTREE` part.
- **R38** The session ID makes each URI unique. Two sessions in one
  worktree have different URIs.
- **R57** `riff mcp`, `riff watch` and the hooks find their session by
  the session ID, not by the directory. Superseded by 4.0 (#1173).
- **R58** A session keeps its session ID for its life. A resumed session
  keeps its ID. Messages to an idle session wait for it.
- **R167** `riff mcp` writes its session ID to a file on the machine,
  for its agent process, and locks the file while it runs. `riff
  watch`, the hooks and the `riff` commands of an agent session use
  that ID before the ID of the agent tool. `RIFF_SESSION` comes first.
  So `/clear` does not change the riff session ID. The file is in
  `$XDG_RUNTIME_DIR/riff`, else `$XDG_STATE_HOME/riff`, else
  `~/.local/state/riff`. A file with no lock does not count.
  Superseded by 4.0 (#1173).
- **01M56RB1DQ396DQ75Q23G87C28** The local folder of riff is private.
  riff makes it with mode 0700, and each file in it with mode 0600, in
  each place: `$RIFF_HOME/state`, `$XDG_RUNTIME_DIR/riff`,
  `$XDG_STATE_HOME/riff` and `~/.local/state/riff`.
- **01M56RB1FZ575KQYHW9FF716CZ** At its start, each riff process gives
  each local folder of riff that exists mode 0700, and each file in it
  mode 0600. When it cannot, it says why on stderr, and goes on.
- **01M517N75C0Z5BAFQAWRN3YTYB** An agent that runs inside a riff
  session is not a riff session, for example a `claude -p` that a Bash
  command of the session starts. riff knows it by a live record of
  `riff mcp` (R167) of a process above the agent. The agent of a hook
  is the process in `CLAUDE_PID`. The agent of `riff mcp` is its
  parent. The hooks of that agent make no call: no start, no end. Its
  start hook gives one line of context that says so. Its `riff mcp`
  has no tools and makes no call. Superseded by 4.0 (#1173).
- **R59** A session ID is not a secret. It never gives access.
- **R64** The `move` tool gives a session a new place. Its session ID
  and its claims stay.
- **01M517N7ANC06S4GTEEZHAB255** The reply of `move` and `riff whoami`
  show the URI of the session from `riff-server`, with its claims and
  `lead=true`.
- **01M517N7FRG389BETVWA1SX5YD** Only these steps end a claim: its
  release, a release of the lead for the session, the end of the
  session (R205), a new start of the session
  (01M3JEE1QQCFS5TMZW5N2DAD2D), and a stop with no end (R9). `move`, a
  keep-alive, `status` and each other call keep each claim.
- **01M51JBEQRWWZKKP3GWW8QX96C** When a new start of a live session
  (01M3JEE1QQCFS5TMZW5N2DAD2D) or a release of the lead for it
  (01M3WG243BW7P6E1ME0DFNQF8C) ends its claims, `riff-server` sends
  that session one direct message from `riff@server`, of the kind
  `message`, so it wakes. The message names each claim, the reason and
  the step: after a new start, claim the item again to go on, or stop
  the work on it; after a release of the lead, stop the work on it. It
  is in the chunk of the command, after the `released` records. A
  session that is gone gets no message. The release of the holder
  itself sends none.
- **01M51JBET7R9QTT15BXDPM5QG7** The skill says what a session does
  with that message. After a new start, it claims the item again when
  it still works on it. After a release of the lead, it stops the work
  on the item, pushes its work as WIP, writes the state on the issue,
  and claims the item again only when the lead asks it to.
  Superseded by 4.0 (#1173).
- **R65** A person who posts from the command line has no session ID.
  The sender is `riff://USER@HOST`. A selector with `user` reaches the
  person through `riff tail`.
- **01M3MWW8KYJ3ZV91X22RBSAF33** A post of a person names the host
  where its command ran, also when the user has sessions or commands
  on other hosts.
- **R39** People see the short form `USER@HOST:REPO#WORKTREE`. Riff does
  not route on the short form.
- **01M3JDCA6R894JG6SDJ2R7AFMN** `riff tail` shows each message as a
  block. The header has the local time, the sender in bold, the
  address, the mark `verified` or `not verified`, and the number. Each
  session has its own color, from a hash of its session ID. A person
  has another style. A date line comes before the first message of a
  day. The body is under the header, with an indent, wrapped to the
  width of the terminal. The status lines go to stderr: a warning is
  yellow, an error is red. The MCP `read` tool and `riff watch` stay
  plain.
- **01M3JDCA9070MY30AYHK3Y67EF** `riff tail --color <auto|always|never>`
  controls the color. The default `auto` uses color only when stdout is
  a terminal, and obeys `NO_COLOR` and `CLICOLOR_FORCE`.
- **01M3JDCAB7K6QA58HDTN9BR1AH** Before `riff tail` prints a message, it
  removes each escape sequence and each control character from the
  body and the names. It keeps newlines and tabs. A body cannot change
  the terminal.
- **01M3ZRQY6YQ8QAKZPGWH1XD6WW** Each text that riff reads from the
  forge goes through one filter before riff prints it, stores it, or
  puts it in a message, a status or a comment: the title of an issue,
  a pull request or a wave, a login, a branch name, a check, a
  commit, a URL and an error of `gh`. The filter removes each escape
  sequence and each control character, makes each line break and tab a
  space, and keeps at most 256 characters. A body of the forge is only
  read for numbers and trailers, and is never printed. `riff pr open`
  refuses a wave name that the filter changes.
- **01M3MEW73CDSJDSKX32XW80WZH** Replaced by 01M3Q63MVZ74WPNBA3QJYQGHFG.
- **01M3Q63MVZ74WPNBA3QJYQGHFG** `riff who` shows the facts of the
  riff, then a table with a row for each session. The facts are
  `riff` (`running` green, `paused` yellow, with who set the pause),
  `paused` for each repository that is paused, `owner` and `build`. The
  columns are SESSION, STATE, ROLE and DETAIL. SESSION is the name
  with the short session ID, bold, in the same color as in `riff
  tail`. STATE and DETAIL are the state of the session and its detail
  (01M3QB6CJ1XCQG5B1BVR8AF3B4). ROLE has `you` in bold and the tags.
  With `--long`, the column URI takes the place of SESSION. The styles
  come from one shared module. The MCP `who` tool stays plain.
- **01M3MEW75WC7Y4M1BKQ7SXRPNR** Replaced by 01M3Q5VE2D244XDZRYXM8DNSRS.
- **01M3Q5VE2D244XDZRYXM8DNSRS** `--color <auto|always|never>` is an
  option of each `riff` command. `auto` uses color only when stdout is
  a terminal, and obeys `NO_COLOR` and `CLICOLOR_FORCE`. A pipe gets
  plain text.
- **01M3N754NY5JX4P0SN8R4ZYFG9** Replaced by 01M3Q63NK0AHM25MB258B0K8XP.
- **01M3Q63NK0AHM25MB258B0K8XP** `riff who` names the owner of the
  riff in the fact `owner`: `USER (EMAIL)`, or `none`. A riff with no
  owner ends with the line that names `riff owner --take`, in yellow.
  A riff with no sign-in shows no owner. The owner is a person: only a
  row with no session of the owner gets the tag `owner`. The MCP `who`
  tool shows the owner line `The owner is USER (EMAIL).` or
  `The riff has no owner.`, plain.
- **01M3NT4M159EHN5W8JRTQ417N4** A session has a role: `lead`, `worker`,
  or none. A worker (`RIFF_WORKER=1`) says so in each register call, and
  the server keeps it and gives it in `who`. `riff who`, the MCP `who`
  tool and `riff top` tag a session with its role, `lead` or `worker`,
  and with its kind (01M510MG2YSETB2CCAS8XDHPZ0), the same on each
  machine. No session gets the tag `owner`.
- **01M510MG2YSETB2CCAS8XDHPZ0** `riff who`, the MCP `who` tool and
  `riff top` tag a session in a container `container`, in place of
  `worker`. The lead in a container has `lead container`. The session
  of `riff workers host` has the tag `host`: its status is the status
  of a host.
- **01M589HH6HCWYJP1ZC7MRAXZ6E** The session of the riff daemon has the
  kind and the tag `daemon` in `riff who`, the MCP `who` tool and
  `riff top`: its status starts with `daemon host:`. It comes after the
  host in the order of the kinds. The line of a host in `riff top`
  counts it in `daemons`.
- **01M589HH8Z1TCYDCZHQQHTYWMK** The session of `riff workers host` and
  the session of the riff daemon take no claims. So an `idle` one does
  not show `ready for work for`. The session of `riff workers host`
  shows `hosts workers for` and the time. The session of the riff
  daemon shows `hosts containers for` and the time.
- **01M3NT4M3A4E3K5S2NM7MS6PQD** The reply to `who` lists each member of
  the riff with a USER, also when away: the USER, the role (`owner`,
  `admin` or `member`), whether a session of the person is live, and
  the seconds since the last call of a session of the person. A riff
  with no sign-in lists none.
- **01M3NT4M5D36KTZ5XZMDP6QFQT** `riff top` shows a tree with `├─` and
  `└─` (01M42KHN33M4K13GKTX2WM6CMM). The person line has the USER in a
  bold color, the tag `owner` or `admin`, and the state of the person
  (01M3QB6CJ1XCQG5B1BVR8AF3B4). Each member of `who` gets a line, also
  when away; with no sign-in, each user of a session. People come by
  USER, hosts by name, and repositories by `OWNER/REPO`. In a
  repository, blocked sessions come first, then by session ID. A
  person on the command line gets no session line.
- **01M3NB54P1RBHTA5TKXP8BMY3K** `riff top` shows a live table of the
  sessions of `riff who`, and draws it again in place every 3 seconds
  and after each message of the repository thread, until Ctrl-C.
  `riff top --once` prints one table and exits. The header has the
  facts of `riff who`: the state, the owner and the build. A board of
  the current wave of each repository follows
  (01M42KHN80V49HDDZF953HXDT0): the wave, then one line for each group
  of its open items, `free`, `held`, `claimed` and `verify`
  (01M3Z9N6X92KT051P10CKKV7EK). The first line of a
  session has the short session ID, the tag of its role, and its state.
  Under it comes one line for each fact of the detail of the state
  (01M3QB6CJ1XCQG5B1BVR8AF3B4). The titles and the wave come from
  `gh issue list`, kept for one minute. With no `gh`, the rows still
  print. `--color` works as in
  `riff who`.
- **01M3Z9N6X92KT051P10CKKV7EK** On the board of `riff top`, an item
  with no claim whose pull request waits for a verify or for the merge
  is in `verify`, not in `free`. An item with a failed verify is in
  `free`. `riff top` reads the open pull requests with `gh pr list`,
  and keeps them as long as the issues.
- **01M58ATZ1F42DWNNB24C2HJPEY** The board of `riff top` has the
  group `held`: each item of the wave that the lead holds, with no claim
  and no pull request in verify. The reason of each hold follows its
  items, for example `held: #1003 #1008 (VmLck reading at 19:40 UTC)`.
  A held item is never in `free`. `riff top` reads the holds from
  riff-server at each look.
- **01M588G5K0B58QXD35AA5MTZ6E** When the whole riff or a repository
  is paused, the wave line of its board in `riff top` ends with
  `paused`, and the board has no group `free`.
- **01M3WNHCD659FH3Z5VYYH69WWR** The first line of a session in
  `riff top` names its worktree after the short session ID:
  `#WORKTREE`, or nothing in the main worktree. The session is under
  the line of its repository. A repository line has the short name of
  the repository when the repositories of the sessions have one owner,
  else `OWNER/REPO`. The wave line names the repository of its board:
  `Wave N (OWNER/REPO)`. Only a claim in that repository is on its
  board, and a session shows the title of an issue of its own
  repository.
- **01M42KHN33M4K13GKTX2WM6CMM** The tree of `riff top` has four
  levels: person, host, repository, session. A person, host or
  repository line ends with its counts: the sessions, then the busy,
  idle and blocked sessions and the claims, each that is more than 0.
  A line with only one line under it, other than a session, takes that
  line after a `›`, so a small riff stays short.
- **01M510MG5J92PYDWZSDQS8HMWK** The line of a host in `riff top` also
  counts each kind of session, after the sessions and before the
  states: leads, containers, workers and hosts, each that is more than
  0, for example `6 sessions: 3 workers, 2 containers, 1 host; 2 busy,
  2 idle, 4 claims`. A session counts once, as the first of these that
  matches. The person and repository lines count no kinds. Superseded
  in part by 01M589HH6HCWYJP1ZC7MRAXZ6E.
- **01M514C142K5BM2746F1D9Y448** `riff top` puts the mark `↑`, in
  yellow, after the short ID of each session that is out of date: the
  file of its riff was replaced, or its build is older than the build
  of the server (01M514C19H3WQP1FSX9RBVSC69). A session with no riff
  in `who` gets no mark, and no error.
- **01M514C16VG9PD70A8424J173B** The line of a host in `riff top`
  counts the sessions that are out of date, after the kinds and before
  the states: `2 to update`. A count of 0 is not shown.
- **01M42KHN80V49HDDZF953HXDT0** `riff top` shows a board for each
  repository with a live session, and for the repository of the
  working directory when no `--user` and no `--host` is set and
  `--repo` matches it, in the order of `OWNER/REPO`. It
  reads the issues and the pull requests of each one with `gh`, each
  minute. A read that fails keeps the last issues of its repository. A
  repository with no `gh` read shows its sessions, no board and no
  error line.
- **01M42KHNCBMBCT3TFBYWE339H5** `riff top --user USER`, `--host HOST`
  and `--repo OWNER/REPO` show only the sessions that match each flag
  that is set, and only the boards and the blocked lines of these
  sessions. A person with no session shows only with no `--host` and
  no `--repo`. `riff top --by repo` puts the repository at the top of
  the tree: repository, person, host, session. `--by person` is the
  default.
- **01M3QA8EZHX5B8C9CKF8Q3154X** `riff top` grows down, not across. No
  line is wider than the terminal: the real width when riff knows it,
  else 80 columns. riff cuts a wider line with `…`. A session with no
  detail takes one line. No line has a column heading.
- **01M3NB589WMPRSAR43BSG9SP41** `riff top` makes only read calls: the
  `riff` and `who` calls of `riff who`, and the stream of `riff tail`.
  It posts nothing and wakes no session.
- **01M3Z8FXE2DY34ZP75WJE1S8HR** After one good look, `riff top` stays
  open when a look fails and a new try can repair the fault: riff
  cannot reach the server, a connection fails in the middle of a call,
  the front end replies by itself, or no reply comes in the budget of
  a call (01M4A803Z4Q0KX6NT1KC6QR43H). It keeps the last table. Its
  first line is then one red line with the time of the last good look
  and the fault. It looks again at its interval. The line goes at the
  next good look.
  `riff top --once`, a first look that fails, and each other fault end
  `riff top` with the error and a status that is not 0.
- **01M53NDCJ0EF0YKDV8AARXKYCQ** When a call of a look of `riff top`
  gets no reply in its budget, the line names that budget:
  `riff-server gave no reply in 60 seconds` with `SHORT_BUDGET`
  (01M4A803Z4Q0KX6NT1KC6QR43H). The line never names the limit of one
  try or the interval of the look. A test that sets the budget with
  `RIFF_LINK_BUDGET_MS` sees its own budget in the line.
- **01M3ZC09FA9DZPTHK31XECZ566** When a later read of `gh` fails,
  `riff top` keeps the titles and the board of its last good read of
  `gh`.
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
- **01M3Q53RNDJBDHVDFHJ9HCX9S1** `riff setup` adds the Claude Code
  permission rules of riff work to the project settings,
  `.claude/settings.json` at the top of the repository. Allow: each
  riff tool (`mcp__plugin_riff_riff`, `mcp__riff`), `Bash(riff)`,
  `Bash(riff *)`, and the pull request steps (`gh pr create`, `gh pr
  merge --auto --squash`, `gh pr comment` and `gh pr view`). Deny: a
  push to the default branch (`origin/HEAD`, else `main`) and `gh pr
  merge --admin`. It adds only the missing rules. A rule in the user,
  project or local settings counts as there. It keeps each other rule
  and key in its order, and writes the file only when it changes.
  `riff setup --check` changes nothing, names each missing rule, and
  exits with status 1 when a rule is missing.
- **01M3Q53RQGXMYVYGCQQMWA9380** When the project lacks a riff
  permission rule, the start hook of the lead tells the lead to ask
  its user to run `riff setup`. Superseded by 4.0 (#1173).
- **R41** A session joins the thread `OWNER/REPO` by default.
- **R42** A cloud session uses the host `cloud`.
- **R100** A session is a cloud session when `CLAUDE_CODE_REMOTE` is
  `true`. `RIFF_HOST` still wins.
- **R43** Outside git, the URI is
  `riff://USER@HOST/-?session=ID#DIRECTORY`.
- **R49** When a watch starts, it wakes the session once if an addressed
  message is unread.
- **01M588J4XNV1TXEPMGTKRVY9VK** An unread read of one page also gives
  each unread message after the page that woke the session, and that
  no read gave so yet. The `read` tool of `riff mcp` shows them first,
  in their own part. For R49, such a message is no longer unread: the
  next watch does not wake for it. The read cursor does not move over
  it, so a later page shows it again in its place.
- **R163** `riff-server` records the time of each call of a session.
  `who` is a call too. A keep-alive (R204) is not a call. `who` shows
  each session as `live`, or with the time since its last call, for
  example `idle 2m`.
- **R164** A gone session (R206) is not in `who`. `who --all` lists
  it. The server keeps its record until it forgets the session
  (01M3TBZBZVH907QD359AB8TBSX), so a resumed session keeps its ID.
- **R204** `riff mcp` sends a keep-alive to `riff-server` each 60
  seconds while it runs, also while no turn runs.
  Superseded by 4.0 (#1173).
- **R205** When `riff mcp` stops (its stdin closes, or it gets SIGTERM,
  SIGINT or SIGHUP), it sends an end call for its session. The
  `SessionEnd` hook `riff hook session-end` sends the same call, except
  for the reason `clear` (R168). Superseded by 4.0 (#1173).
- **R206** A session is gone when it ended (R205), or when the server
  got no call and no keep-alive from it for 3 minutes
  (01M3WG240PNMQYZ7TX6Z7ZF6M9). A gone
  session matches no selector. A direct message to it fails and says
  that the session is gone. An end frees the claims of the session at
  once. Its lead does not count while it is gone. After a stop with no
  end, the claims and the lead end together, 5 minutes after the last
  sign of life (R9).
- **R207** A call or a keep-alive from a gone session makes it live
  again, with the same ID, threads and read cursors. After a stop with
  no end, it gets back each claim that no other session took. After an
  end, it has no claims. A lead is the lead again, unless another
  session became the lead (R178).
- **01M3WG240PNMQYZ7TX6Z7ZF6M9** An open watch stream is no sign of
  life. The start of a watch is a call. The close of a watch stream is
  no sign of life. `riff watch` sends a keep-alive each 60 seconds
  while it runs. So a session whose process is killed with no end call
  is gone after 3 minutes, and its claims are free after 5 minutes,
  also when a front end holds its watch stream open.
  Superseded by 4.0 (#1173).
- **01M3WG243BW7P6E1ME0DFNQF8C** The lead of a user frees the claim of
  another session of that user in its repository, live or gone:
  `riff release ITEM --session ID`, or the `release` tool with
  `session`. ID is the session ID of the holder, or a start of it of 4
  or more characters. The server refuses each other caller. It posts a
  note in the thread of the claim that names the lead, the item and
  the holder.
- **R182** A session sets its status with `riff status` or the `status`
  tool. A status is the current step of the session, in its own words.
  It makes no state. A new status replaces the old one.
- **R183** A status is one line. The step is not empty. The step has
  at most 400 characters. The reason of a block has the same rules.
  `riff-server` refuses a status or a block that breaks a rule, with
  status 400.
- **R184** `riff-server` keeps the last status of each session, with the
  time that the session set it. It saves the status with the session.
  `who` shows each status and its age, for example
  `4m ago: write the tests`.
- **01M3Q551WCMPQRCNJ8FXQEBFY4** `who` gives for each session the
  seconds since its claims last changed: a claim, a release, a new
  start of the session, or a start of `riff-server`. A worker with no
  claim is idle for this time.
- **01M3Q551YHYZBFV2NDS1QCYXCD** A status is stale when the session set
  it before the last change of its state: a claim or a release of the
  session, or a pause or a resume of the riff or of the repository
  of the session. `who` marks a stale
  status. A claim that the session holds already, and a set of the
  riff to its state, are no change. A status from before a start of
  `riff-server` is stale.
- **01M3Q555KC1RKNEC4ZA9HQYJG2** A stale step is dim and says `stale`.
- **01M3QB6CJ1XCQG5B1BVR8AF3B4** `riff-server` derives the state of
  each session and gives it in `who`. No session reports its state.
  When a `who` reply has no state, riff derives it the same way from
  the other facts of the reply.
  The first state that matches wins (01M41FZQVEF8S2W9RCM4V87C3D).
  `riff top`, `riff
  who`, the MCP `who` tool and `riff workers` show the word of the
  state, then its detail: for `offline`, `seen 2h ago`; for `paused`,
  the claims and `stopped at:` the step; for `blocked`, the reason with
  its age, `the lead gave no answer` when the lead gave none, then the
  claims; for `waiting`, the claims, then what they wait for; for
  `busy`, `working on #N` or
  `reviewing #N` (a verify claim) for each claim, then the work
  (01M41FZNTPXQNCZ1S99HE42PYQ), then the step; for
  `idle`, `ready for work for` the time since the last release, then a
  current step. An `idle` lead shows `monitoring work for` and the
  time, not `ready for work for`. `riff top` adds the title of each
  issue. The colors:
  `blocked` red, `waiting` cyan, `busy` green, `idle` dim, `offline`
  grey, `paused` yellow. A `blocked` session comes first in `riff top`
  (01M41FZR4XRP55M409YBCPTHPH). A person is
  `online` when a session of the person is live, else `offline` with
  `seen` and the time since the last call. Superseded in part by
  01M522MRW21RHGF2B6F0VCP0BD and 01M589HH8Z1TCYDCZHQQHTYWMK.
- **01M522MRW21RHGF2B6F0VCP0BD** The detail of an `offline` session is
  `seen` with the time, then its claims, then its last step with its
  age: `seen 5m ago`, `working on #15`, `10m ago: docs`. So a session of
  a daemon that is offline shows its item and its status
  (01M519Z554SC3EAT69Q3M6JDAP).
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
- **01M3WKCYM623M66ATHCH3QGMKP** The automatic step of the lead shows
  no text of a direct message: the step of a `tell` is `told SESSION`.
  In TEXT, each run of white space or control characters is one space.
  An automatic step does not end a block of the lead
  (01M41FZPGEK4TNPSM2051W4VMS). A call that `riff-server` refuses sets
  no step.
- **01M3Q555NV8ZCQ8PVPBXQ7J82C** The skill, the start hook and the
  `status` tool do not tell a session to set its status for a fact
  that riff derives: a claim, a release, a pause, an idle worker, its
  work, or a wait for a verify, a merge or a need.
- **01M41FZQVEF8S2W9RCM4V87C3D** The first state that matches wins:
  `offline` (no open watch stream, and not a lead or a session in a
  container that is not gone, 01M510MG0CKDXAKF6YJ768K7E8),
  `paused` (the riff or the repository of the session is paused),
  `blocked` (a block that holds, not a lead), `must_clear`
  (01M3X9XC99KY4RQY36A7CYWY11), `waiting` (each claim of the session
  waits, or a lead with a block), `busy` (a claim, or a lead in a
  turn), `idle` (each other session).
- **01M48VDGQ5KETKPM4G6TKTC2MB** A lead is live while it is not gone,
  also with no open watch stream. So a lead that calls only the
  command line shows in `riff who` and `riff top` with its state and
  its status, not as `offline`. A `riff status` call and a `riff step`
  call are signs of life.
- **01M510MG0CKDXAKF6YJ768K7E8** The riff daemon and `riff run` send
  the keep-alive of a session in a container with the mark
  `container`. The server keeps the mark until the session ends. A
  session with the mark is live while it is not gone, also with no
  open watch stream: it shows its state from its facts, not
  `offline`. When its container stops, the daemon ends the session,
  and it is gone at once. With no keep-alive for 3 minutes, it is gone
  too.
- **01M514C11BPE93PJT4221PYVHS** The process that lives as long as a
  session names its riff in each keep-alive: the build, and
  `replaced` when Linux names the file of the process
  `PATH (deleted)`. These processes are `riff mcp`, the riff daemon
  and `riff run` for a session in a container, and `riff workers
  host`. Each other keep-alive, for example of `riff watch`, names no
  riff. The server keeps the last riff that a keep-alive named, and
  gives it in `who`. A session of an older riff names none, and `who`
  gives none for it.
- **01M510MG808MQT08W6VSPKM1A6** A session in a container is never an
  idle worker for the rollout of the lead: it does not count as idle,
  and it gets no request. The daemon gives it its item.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M48VDGTD40P8RBZMS0XB5M9N** A session shows a long step with
  `riff step start NAME`, `riff step done` and `riff step fail
  REASON`. A new start replaces the old step. `riff who` and `riff
  top` show the step after the detail of each state but `offline`:
  `NAME for 12m`, or in red `NAME failed 3m ago: REASON`. A failed
  step shows until the next change. Done removes it. The step is a
  signal, not a record of the log.
- **01M48VDS663X064YS5ZGCCZSTB** `riff step fail` sends `step failed:
  NAME: REASON` to the lead of the user, which wakes it. A lead sends
  no message to itself.
- **01M48VDS8RKJS9HG3KSEYGBFGV** The state of a lead comes from its
  facts, not from its claims: `busy` while the hooks see a turn that
  runs, with its work; `waiting` while its block holds; `idle` when
  its turn ended and nothing waits. Superseded by 4.0 (#1173).
- **01M48VDSB4CHQS9P6XVDJ6FMKS** A lead that waits for its person
  calls `blocked`. It sends no message, and shows `waiting for USER:
  REASON` with its age. A message that wakes the lead is no answer to
  its block. The lead is never red and never in the blocked lines of
  `riff top`.
- **01M48VDWPDYRPEAXHR1MYDN1M7** The plugin has a `UserPromptSubmit`
  hook, `riff hook prompt`. It writes the time of the last prompt of
  the person to a file on the machine, and makes no call. `riff mcp`
  puts its age into each keep-alive. A prompt at or after a block ends
  the block, for each session. Superseded by 4.0 (#1173).
- **01M41FZRF5HEZCDS515CP7DYCV** The work, the block and the facts of
  the items are signals in the memory of `riff-server`, not records of
  the log. A start of `riff-server` loses them, and the clients send
  them again.
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
- **01M57XY3Q6HEYQG27FE0CEQ7HB** The plugin has a `PermissionDenied`
  hook for each tool: `riff hook denied`. The settings of each
  container session, the lead and each worker of the riff daemon, run
  `/opt/riff/riff relay denied` for it. The hook reads its input,
  keeps one record, starts the send as a detached process, and returns
  at once. It never waits for riff-server. It does nothing where riff
  is off, and in a session that left the riff. In a container session,
  the riff daemon posts the record as the session.
- **01M57XY3SEG8HCXPZFBVATQ1W2** Each refusal of auto mode is one note
  in the thread `denials`: one line, the tags first, then a colon and
  the text: `denied reason=R tool=T head="H" role=O user=U host=N
  repo=P session=S claim=C: TEXT`. A note wakes no session.
- **01M57XY3VP35QDP2KK88ERF82M** The tags of a record: `reason` is the
  rule in the square brackets of the reason as a slug, `no-verdict`,
  or `checker-unavailable` for "Classifier unavailable". `tool` is the
  tool name, with `@SERVER` for an MCP tool. `head` is, for Bash, the
  first two words of the command after `nice -n N`, `env`, the
  variables and `cd X &&`; for another tool, the path, else the first
  field of the input; at most 60 characters. `role` is `lead`,
  `worker` (in tmux), `container` or `session`, with `+subagent` in a
  subagent. `session` is the first 8 characters of the session ID.
  `claim` is the claim of the session, or `-`. The text is the command
  or the input, on one line, at most 300 characters.
- **01M57XY3XXCKBXMVPFBY0C2KDM** No secret goes to the thread
  `denials`. The hook replaces each secret of the session with
  `[secret]` before it keeps the record: the secret variables, the
  values of the MCP servers of the session and the tokens of the
  sign-in file of `claude`.
- **01M57XY405BSXEH3K3Z977RWAW** After it posts the records, the
  session leaves the thread `denials`. So no session sees the records
  of other sessions at its `read`.
- **01M57XY42CNR16M5G024PWKANV** When riff-server or the riff daemon
  does not take a record, the record stays in a local file: in the
  local folder of riff on the host, and in the Claude folder of a
  container session. The next send posts the kept records first, in
  order.
- **01M57XY44K2N3PS9F8NQ6SZQP9** `riff denials` reads the thread
  `denials`. It counts the records by `reason`, by `tool` and `head`,
  by `role` and by `repo`, and then shows the last 20 records.
  `--since TIME` (for example `7d`, `12h`) keeps only the records of
  that time. `--format json` prints the records. It works with
  riff-server 3.2.0, with no change of riff-server.
- **01M41FZP2C4Z4J6WKRXZ5B31EH** `riff-server` has no credential of the
  forge. The clients send the facts of the items of their repository:
  `riff pr open` (a verify is asked), `riff verify` (passed or
  failed), `riff pr wait` (merged), and the `riff mcp` of the lead each
  minute, with each open pull request and each open item of each
  `Needs:` line. A list of the lead replaces each fact of the
  repository. A fact of one command replaces the fact of its item.
- **01M41FZP9A50CH4A2VX344DW49** A session is `waiting` when each of its
  claims waits: the author of an item with an open pull request with
  no verify result waits for a verify; the author, and the session
  that verifies, wait for the merge when the verify passed; the author
  of an item with an open item in its `Needs:` line waits for that
  item. A wait wakes nobody. It ends when its fact ends.
- **01M49HAW3NXNXNX02ETDZD3YCN** For a wait, an item of a `Needs:` line
  is open only when its issue is open and has no comment that starts
  with `Merged in #`.
- **01M49Q30XMVRFX42YTM1PHX0RZ** When the look of the lead sees an open
  pull request of an item with auto-merge on and the state
  `CONFLICTING`, it sends a message to the session that holds the item.
  When no session holds the item, the message goes to the lead.
- **01M49Q316RXNATJP587DWGDNCD** When the look of the lead sees a pull
  request that waits for a verify, and no session claims its `verify-`
  item, for 30 minutes, it sends a message to the lead. A `verify-`
  claim starts the 30 minutes again.
- **01M49Q31FASDM7CG3JEGPYCZB9** Each message of a pull request that
  stops comes one time for each pull request, head commit and state.
  The `riff mcp` of the lead keeps what it told in memory only.
  Superseded in part by 01M5QVH3MAEPPJN9PPVDX6N2EY: the daemon keeps it.
- **01M51QEYKTD7ZEB0YA0C3MQTPN** The look of the lead reads the check
  run `Gate` of the head commit of `main` each minute. When the Gate
  failed (`failure` or `timed_out`) at a commit that the lead did not
  get, and `main` passed before, the lead gets one message: the commit,
  the run, the failed jobs and the fix item. When `main` was broken at
  an older commit, the lead gets a note. A Gate that runs, a cancelled
  Gate and a head with no Gate give nothing. The `riff mcp` of the lead
  keeps the last broken commit that it told in memory only. Superseded
  in part by 01M5QVH3MAEPPJN9PPVDX6N2EY: the daemon keeps it.
- **01M51QEYPGD6W345MZ4F85C38F** When the Gate of `main` fails, riff
  files one fix item: an issue with the label `broken-main` in the
  current wave, with the commit, the run, the failed jobs, a `Needs:`
  line and a `Done when:` line. riff makes the label when it is
  missing. When an issue with the label is open, riff files no new
  one: it puts a comment with the new commit and run on the open one.
  When riff cannot file the fix item, the message to the lead says why.
- **01M51QEYS538XA58KDRY844T52** An open issue with the label
  `broken-main` is free work before each other item: its verify first,
  then the item. This is true for the rollout of the lead and for the
  wave of the daemon. It is free work also outside the current wave and
  outside each wave, when no session claims it, it has no comment
  `Merged in #`, no pull request of it waits for a verify or the merge,
  each issue of its `Needs:` line is closed, and no lead holds it. The
  skill tells a session that picks its own item to take it first.
- **01M51QEYVQN74Q1VDTVHB0445Q** When the Gate of `main` passes, riff
  closes each open issue with the label `broken-main` that no session
  claims and that no open pull request names, with a comment that names
  the commit. The lead gets a note when `main` was broken before, or
  when riff closed an item. The note names the items that stay open.
- **01M41FZPGEK4TNPSM2051W4VMS** A session that cannot go on with no
  decision of a person calls the `blocked` tool, or runs `riff blocked
  REASON`. One command sets the block and sends `blocked: REASON` to
  the lead of the user, which wakes it. No command does one with no
  other. With no lead, the block holds, and the reply tells the
  session to ask its own user. The `status` tool and `riff status`
  have no reason of a block.
- **01M41FZPT31ATXP75QW965P3JB** A message, not a note and not a
  status request, that wakes a blocked session is its answer. The
  block ends at the next sign of work after the answer: a fact of the
  hooks with a turn that runs, at or after the answer. A claim, a
  release and a new start of the session also end its block.
- **01M41FZQ545HQ9Q75CSKX8HF8H** The `riff mcp` of the lead looks at the
  blocks of the sessions of its user in its repository each minute,
  with `lead.wake` minutes of the machine of the lead (default 15).
  A block with no answer for that
  time gets a second wake of the lead: a message of the server,
  `blocked: SESSION (ITEM) has no answer after N minutes: REASON`. Only
  the lead can look.
- **01M41FZQCHWY1YVGAZ60ZHJK21** A block with no answer for `lead.wake`
  minutes after the second wake is unanswered. `who` gives it, and
  `riff top` shows `the lead gave no answer` under the reason. An
  answer ends each step.
- **01M41FZQKZKW131Z8822G31T5G** When a block becomes unanswered, the
  `riff mcp` of the lead shows one desktop notification with
  `notify-send`: the short session ID, its claims and the reason, and
  no text of a message. A machine with no display or no `notify-send`
  gets none, and nothing fails. `lead.notify` turns it off. It is on by
  default.
- **01M41FZR4XRP55M409YBCPTHPH** `riff top` has one red line for each
  blocked session before the board: `blocked`, the short session ID,
  its claims, the time that it waits, `the lead gave no answer` when
  the lead gave none, then the reason. A `waiting` session is cyan, not
  red.
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
- **01M3MEEFH79XXNZW6DWSPTEW2A** The leave holds for the life of the
  session, also over `/clear` and a resume: `riff` records it in a
  file `left-ID`, the mark of the leave. A new session joins as
  usual.
- **01M3MEEFKX14QCQM0F9ZYW93PP** A session that left joins again with
  the `join` tool. The plugin command `/riff:join` tells the session to
  call it. The session registers again with the same ID, starts its
  watch and follows the start routine. It becomes the lead only as at
  a start: as the first session of its user, or on the word of its
  user. Superseded by 4.0 (#1173).
- **01M3MEEFPEYXTZ89XR28E02W7P** The words "leave the riff" and "join
  the riff" of the user of a session do the same as `/riff:leave` and
  `/riff:join`. The skill says so. Superseded by 4.0 (#1173).
- **01M3MEEFSD4TEQESRDJENCFW7N** The tools that join and leave a thread
  are `join_thread` and `leave_thread`.
- **01M3XQVJXWBC3DKAVWBPXPSGZS** A leave is a fact of one session on
  one machine. `riff-server` does not keep it. The one function of the
  client that sends each request reads the mark of the session before
  each request. With the mark, it sends nothing, and the call fails
  with a text that names `/riff:join`. So each entry of the client has
  the same check: the status line, each hook, the watch, each command,
  each tool, and the keep-alive, the rollout and the reap of
  `riff mcp`. The rollout of a session that left looks at nothing.
  Superseded by 4.0 (#1173).
- **01M3XQVK05FAT3PR43W8RNEYHY** The `leave` tool writes the mark
  before the end call. The end call is the one request that goes out
  with the mark. When the end call fails, the tool removes the mark,
  refuses, and the session stays. With no directory for the mark, the
  tool refuses. The `join` tool removes the mark before the register.
- **01M3XQVJVJDX81QY38219SN96B** The mark of a leave is in the state
  directory of riff: `$XDG_STATE_HOME/riff`, else
  `~/.local/state/riff`. It is never in `$XDG_RUNTIME_DIR`, which the
  system clears at a logout. So the leave holds over a restart of the
  machine, of `riff mcp` and of `riff-server`.
  Superseded by 4.0 (#1173).
- **01M5GVQQ8NK3RD9CM3VTGXEHZQ** The settings of each container session
  have a `UserPromptSubmit` hook, `/opt/riff/riff relay prompt`. It
  tells the riff daemon of the prompt through the socket of the session,
  and always exits with status 0. For the lead, the daemon sends a
  keep-alive with a prompt of age 0 to riff-server: the next prompt of
  the person ends the block of the lead. A prompt in a worker ends no
  block.
- **01M5MTWEN9F58X3B0H02DSTQG6** A riff daemon that runs the wave of a
  repository also serves --host for it with no riff daemon host. At its
  start it writes served.json from waves.json for each repository that
  is missing there, with the server of the wave.
- **01M5NJAXTQ0YKN7Q7XN0A7ZMPF** Only a person names the lead: the
  command `lead` of a person names the session ID of the new lead, or
  its start. riff-server refuses `lead` from a session, also for itself,
  so `riff lead` in a session fails, and the riff tools have no `lead`
  tool. A worker cannot be the lead. Supersedes R177.
- **01M5NJAY3FTR005TDJJY1TMGNX** No session becomes the lead by itself:
  a `register` or a `start` never makes a lead, also for the first
  session of a person in a repository. Supersedes R176 and
  01M3X9XA3H6YF0QCYSNB2P0CT2.
- **01M5NJAYC7MXS13TA3VJB4N6RP** `riff lead start` and the daemon for
  `lead move` make the new session the lead as the person, with no
  session ID. A person in a terminal with no session ID names a lead
  with `riff lead --session ID`, for example a session on the host when
  the lead container ended.
- **01M5NJAYMYF50B9WXH5RF9W737** When the lead changes, riff-server
  sends the old lead a direct message: it is not the lead now, and the
  session that is. A gone old lead gets none.
- **01M5P1XN4YA3S1CWTN939DQCWW** A session in a container starts `riff
  relay watch --follow` again when its Monitor ends, and only then. The
  instructions `text::MCP_LEAD` and `text::lead_start` say so.
- **01M5P1XN9EX680KQNKRHWGRWJT** A new watch does not wake a session for
  a message that a `read` returned, also a `read` with `all` and
  `after`. Such a read keeps the cursor.

## Threads

- **R23** Sessions talk in named threads. A direct message is a thread
  with two members.
- **R24** Threads are flat. There are no nested replies.
- **R25** A thread keeps its last messages (01M3TBZBT7MME9BG1RWX5SZAZ6).
  A session that joins can read them.
- **R26** Only an address wakes a session. Text in a message body
  never wakes a session.
- **R27** A person can read and post in each thread from the command
  line.
- **R28** A claim belongs to a thread.
- **R48** A person can claim and release work from the command line.
  `riff claim` exits with status 1 when another session holds the item.
- **R50** A session lists and reads by default only the threads that it
  joined. It can read any other thread by name, except a direct thread
  of two other sessions (01M3T411J3TN00FER230V3YX17).
- **01M3T411J3TN00FER230V3YX17** `read`, `tail` and `watch` use one
  rule. The caller acts as its token (R104), and gets a direct thread
  only when it is one of its two sessions. Else `read` and `tail` reply
  404, and `watch` gives no wake of it. `riff tail` sends the URI of the
  caller.
- **R51** A post has a `to` list of selectors. A selector names one or
  more of these fields: `user`, `session`, `host`, `repo`, `worktree`,
  `claim`, `lead`. A session matches a selector when each named field
  matches. A post wakes each session that matches one or more
  selectors.
- **R60** Riff matches the selectors when the message is posted. A
  session that matches later gets no wake.
- **R61** The post result names each session that woke, and each
  selector that matched no session.
- **R62** A direct message is a post with one `session` selector, or
  one selector with `lead` set to true (R179).
- **R63** A person addresses a post with `riff post --to FIELD=VALUE`.
- **R78** A person sends a direct message with `riff tell SESSION`.
  SESSION is a session ID, a full session URI, or `lead`.
- **R179** The selector field `lead` is `true` for the lead and
  `false` for each other session. `tell` with `lead` sends a direct
  message to the lead of the user in the repository of the sender.
  The sender does not need the session ID of the lead. When the user
  has no other session as the lead there, the `tell` fails and tells
  the sender to ask its own user.
- **01M56P1T2AZCP48RJBM01RRRZ4** A direct message with `lead=true` goes
  to the lead while its lead holds (R9, R125), also while the lead is
  gone (R206). So after a start of `riff-server`, the lead gets it
  before it calls again.
- **01M57D4FR8TPY47WPT9G66H4PE** A session can send a direct message to
  the person of its own user: a post with one selector that names only
  that user. The direct thread is the thread of the session and the
  person. Only those two read it, and no other session joins it.
- **R79** `riff read` shows the unread messages of the threads of the
  person. It joins the person to the thread of the directory first.
  `--thread` reads one thread. `--all` shows each kept message. It
  reads each page (01M3TBZBX140GJWCV5GZ73Q5Z5).
- **01M58AJE14M9TPTQE4E8R9PA9M** A session names one of its direct
  threads `dm:PEER`. PEER is the session ID of the other side or the
  start of it, or the user of a person. `threads` and `read` show
  each direct thread as `dm:PEER (direct with SHORT)`, with the first
  8 characters of the session ID. The `thread` of the `read` tool and
  `riff read --thread` take this name or this label. riff looks for
  PEER only in the threads of the caller, so a session reads only its
  own direct threads. The line of a next page names the thread.
- **R185** A post has a kind: `message` (the default), `status` or
  `note`. A post of kind `status` is a status request. It wakes the
  sessions that its selectors match, as each post does. A person sends
  one with `riff post --kind status --to FIELD=VALUE`. A status request
  needs no body.
- **01M3JPMQE6S7YM4HPEVGXWK7ET** A post of kind `note` wakes no
  session. Each session that its selectors match joins the thread, so a
  direct note reaches its receiver. `read` shows a note with the word
  `note`. A person sends one with `riff post --kind note`.
- **01M3JPMQG9FDB719BC8MDCBNBA** The skill tells a session to wake
  only the sessions that must act. A board, "started", "done" and other
  news are notes. A verify request wakes the lead of the author's user.
  A verify result wakes the holder of the item, and the lead when no
  session holds it (01M3Z9N70J4H79VJN4ZKKH3G6S). A question or a request
  goes to one session with `tell`. Superseded by 4.0 (#1173).
- **01M3JY1TBPQHH6WPPBTF42T64H** In a thread, a selector with
  `lead=true` that matches no live session matches each live session
  with no claim that its other fields match. So a verify request
  reaches a free session of the author's user when that user has no
  live lead. With a live lead, it wakes only the lead. The skill and
  the book say so.
- **01M58AQX46RH9KFP91QAFS43N7** The sender of a post counts as a live
  session for the fallback of 01M3JY1TBPQHH6WPPBTF42T64H. A lead that
  posts to its own `lead=true` selector wakes no other session for that
  selector, and the post result names no unmatched selector for it.
  The skill and the book say so.
- **01M3JPMQJCC3F19QAJ84EKMVKA** The skill and the start hook tell a
  session to call `read` and start the watch again in the same
  response, so that a wake costs one request. Superseded by 4.0 (#1173).
- **R186** The wake line and `read` show that a message is a status
  request. A session that a status request wakes answers with `status`.
  It does not post a reply.
- **01M3JPK82PN4F706MCHDH771MW** `read` does not give a session its own
  posts, and the unread counts of `threads` leave them out. `read` with
  `all` gives them.
- **01M3JPK85FT5CCQPF3WDCXSMDF** Each message in `read` shows a short
  sender: `USER@HOST:REPO#WORKTREE`, the first 8 characters of its
  session ID, and `lead=true` for a verified lead. A post to each
  session of the repository of its thread shows `to all`. `who` gives
  the full URI.
- **01M3JPK885GPD16FPK7D05R2RC** `tell` takes a session ID, the start
  of a session ID as `read` shows it, a full URI, or `lead`. A start
  that fits more than one session in `who` is an error.
- **R187** The skill tells a session to set its status when it changes
  step, and to call `blocked` when it cannot go on with no decision of
  a person. It says what the words of a status are for.
  Superseded by 4.0 (#1173).
- **01M3NB5MY93KV9RKZGGSMZW00D** The people of a riff chat in the
  thread `chat` on the riff server. `riff chat` is a line client in
  the style of IRC. It shows the history and each new line with the
  time and `USER@HOST`, and posts each line that the person types.
  `--color` works as in `riff tail`. `/quit` or Ctrl-C exits. The
  chat has no private channels, no nick changes and no files.
- **01M3NB5N0D99JB5CE6RB4VEYPF** A chat line wakes the lead of the
  sender, unless it names a lead: `@lead` wakes the lead of the sender,
  and `@USER` wakes the lead of USER. The skill tells a lead to
  answer a chat line with a post to the thread `chat`.
- **01M3NJD39JVJHY5G71CD79JBY3** In a terminal, `riff chat` has a
  prompt line at the bottom. A new line prints above the prompt, and
  the typed text stays. After Enter, the typed line goes away, so the
  line shows once: as the chat line from the server. The start line
  comes before the history. With a pipe, `riff chat` has no prompt and
  no line editor.
- **01M3NJD3BR0XAYNNFTEY0CG761** A chat line shows a person as
  `<USER@HOST>`, a verified lead as `[USER's lead]`, and each other
  session as `[USER ID]`, with the first 8 characters of its session
  ID.
- **01M3NJD37CNQX580YC24S7K6ES** `/me TEXT` in `riff chat` posts an
  action line: a plain message with the body `/me TEXT`, so the riff of
  the release before reads it. `riff chat` and `riff tail` show it as
  `* USER@HOST TEXT`, and `read` as `* USER@HOST TEXT` after the head.
  `@lead` and `@USER` in an action wake as in a chat line.
  `riff chat` sends no line that starts with an unknown command, and
  names its commands. A line that starts with `//` sends the line from
  its second `/`.
- **01M3NK7VHXB0PAR8VH8GQQA06K** A stream that ends is normal for a
  long poll. `riff chat`, `riff tail` and `riff workers host` connect
  again at once, and show nothing for it. Only a connect that fails
  shows one short dim line `(reconnecting…)`, and the next item
  `(back)`. A server that riff cannot talk to shows its error.
- **01M3NK7VM1J5DDB0PECNZ28P4E** Replaced by
  01M49Z4E8QB1QDXVCPE6MX5JX2.
- **01M49Z4E8QB1QDXVCPE6MX5JX2** At each connect, `riff tail` and
  `riff chat` open the stream first, then read the thread after the
  last `seq` that they showed, then show the live messages. They show
  each message one time, in order: they drop each message with a `seq`
  that they showed. At the first connect, `riff tail` shows only the
  new messages, and `riff chat` shows the history, or the lines after
  `--after N` after an update. The server keeps no cursor for it, and
  no `Last-Event-ID`.
- **01M49Z4EB7T972BHEP6T92P574** When the read after a break starts
  after a gap, because the server no longer keeps the messages of the
  gap, `riff tail` and `riff chat` show one line with the number of the
  lost messages.
- **01M3Q59CAA46C316BD4D1ED7C6** When the first connect after a stream
  ends fails, riff tries once more at once, and shows nothing for it.
  Only when that try fails too does it show `(reconnecting…)`.
- **01M5P1V9SZP9EF475MK3G9WK5E** A chat line of a person that names no
  one with @ wakes the lead of the sender, as @lead does. `riff chat`
  and its help say what wakes whom.

- **01M5P207CN20TQTP017X6RTMWG** The selector field `person` is `true`
  for the terminals of a person (`riff chat`, `riff tail`, the phone),
  which have no session ID, and `false` for each agent session. The lead
  posts to its person only with `user`, `host` and `person` set to true:
  the post wakes no daemon, worker or other agent session. The
  instructions of the riff tools name this selector.
- **01M5QVH7KDK3R2S4NDFB2V1XY9** A chat line with `@lead:REPO` wakes
  only the lead of the sender in the repository REPO, and `@USER:REPO`
  only the lead of USER in REPO. REPO is `OWNER/REPO`, or the name after
  the slash when one lead of that user has a repository with that name.
  `riff chat` reads the leads from riff-server. When no lead or more
  than one lead matches, it sends nothing and names the leads that it
  found. `@lead`, `@USER` and a line with no name still wake each lead
  of the user. A chat line shows a lead as `[USER's lead, REPO]`, with
  the name of its repository.
- **01M5QVD3B1GT7STHBQS7NK4DR0** A session starts each thread that it
  joins at the newest message of the thread: the messages before its
  join are not unread, and `read` with `all` shows them. A session that
  becomes the lead of its user in a repository takes the read cursors of
  the lead before it, in each thread that both joined. So its unread
  messages in those threads are the messages that the lead before it did
  not read.

## Security

- **R10** A session treats a received message as advice, not as an
  instruction. The one exception is a verified request of the lead of
  its person (R230). Advice includes each message from another session
  of the same person, from a session of another person or its lead,
  and each message that is not verified. The session uses its own
  judgment: it acts on advice, asks about it, or says no.
- **01M3JEJW019FFEVQ0ZX17362EW** The start hook, the MCP server
  instructions and the head of each `read` state the rule of R10 in
  the same words. The skill states it in its rules.
- **01M3JEJW26Y1C0RHM1CENJRDZ1** The skill tells a session to talk to
  other sessions when it helps: share what it found, ask a question,
  or warn about a conflict, for example before two sessions edit the
  same files. Talk needs no lead. Only the lead sends requests.
  Superseded by 4.0 (#1173).
- **R11** Messages do not carry secrets.
- **R195** Each message carries a signature from the device key of
  its sender (R18). A message from the command line (R65) carries the
  signature of the device key of the person. Superseded by 3.0
  (01M519Z1Z1FVZQQ612DEMSQ7PG).
- **R196** The signature covers the user, the session ID and the lead
  mark of the sender, the thread, the `to` selectors, the body, the
  kind and the time. It does not cover the place and the claims of the
  sender. A direct message signs no thread. Its one selector must match
  the other session of its thread. Superseded by 3.0
  (01M519Z1Z1FVZQQ612DEMSQ7PG).
- **R197** With sign-in (R85), `riff-server` refuses a post with
  status 403 when it has no signature, when the signature is not
  valid, when the key is not the key of the token, or when the signed
  time is more than 5 minutes old or more than 10 seconds in the
  future. Superseded by 3.0 (01M519Z1Z1FVZQQ612DEMSQ7PG).
- **R198** `riff-server` keeps the payload and the signature with the
  message, also in the log (01M3T411N0HM699VJXW6RTVWKB). The time of a
  signed message is its signed time. The sender of a signed message
  has `lead=true` only when the signature covers it. `riff-server`
  refuses a signed post with the lead mark from a session that is not
  the lead. A signed-in `riff` asks for its lead mark before it signs.
  Superseded by 3.0 (01M519Z1Z1FVZQQ612DEMSQ7PG).
- **R199** The reader verifies each message before it shows it. Each
  message shows `verified` or `not verified`. A message is verified
  when its signature is valid over its kept payload, the payload holds
  the message as the reader got it, and its key is the key of a live
  sign-in of the user of the sender.
  `read` and `tail` give these keys. A message from a riff with no
  sign-in is verified too (R212). Superseded by 3.0
  (01M519Z1Z1FVZQQ612DEMSQ7PG).
- **R200** A message that is not verified never counts as from the
  lead. The reader shows its sender without `lead=true`.
- **R201** Without sign-in, `riff-server` keeps no signature and gives
  no keys. So only a riff with no sign-in verifies such a message
  (R211). Superseded by 3.0 (01M519Z1Z1FVZQQ612DEMSQ7PG).
- **01M3T411N0HM699VJXW6RTVWKB** A signed post carries its payload:
  the bytes that the signature covers, the base64url of the JSON of the
  signed fields. `riff-server` checks the signature over the payload,
  and checks that the payload holds the fields of the post. No build
  encodes a payload again. A build skips a field of a payload that it
  does not know. Superseded by 3.0 (01M519Z1Z1FVZQQ612DEMSQ7PG).
- **01M3JEJVXXEPPNGT3FY4ZSFCWZ** `riff-server` refuses a signed post
  whose payload is the payload of a message in the same thread: a
  copy. It compares a hash of the payload, not the signature. So a
  session gets each request of its lead once. With R197, a copy older
  than 5 minutes fails the time check too. Superseded by 3.0
  (01M519Z21ECQY89RS60DZKZQVR).
- **R211** A `riff-server` with no sign-in provider and no
  `--require-sign-in` is a riff with no sign-in. It trusts each caller.
  A person runs it only on a network that they trust (R203). Each
  `read` reply and each `tail` event of such a riff says that it
  trusts its callers. Superseded by 3.0 (01M519Z2WAGQEX1H4EGZXWBZT6).
- **R212** The reader counts each message of a riff that trusts its
  callers as verified. The lead mark of such a message is the lead mark
  that the server gives. Superseded by 3.0 (01M519Z2WAGQEX1H4EGZXWBZT6).
- **01M3JCE4ZD4DZCQ21FA69RT52D** A riff with no sign-in (R211) listens
  only on a loopback address. `riff-server` refuses to start on another
  address, unless it gets `--insecure` (`RIFF_INSECURE`). With
  `--insecure` on such an address, it warns at start: each machine that
  can reach it can read, post and answer as any person. A riff with
  sign-in listens on any address once it has an owner
  (01M3JN3AQMHZHT6JP3P6GM9PWZ). Superseded by 3.0
  (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3JZN1VEF73EPFE2FJY36EY4** When `riff-server` refuses an address
  that is not loopback because it has no sign-in, its error names
  `RIFF_OIDC_CLIENT_ID`, `RIFF_OIDC_CLIENT_SECRET` and `--insecure`.
  Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).
- **01M3JCE51T84JZKJ0NR89TPDNY** `riff-server` never terminates TLS. A
  proxy or the platform in front of it does, for example Cloud Run.
  Superseded by 3.0 (01M4Z60B7FCREZDCTVR73WWRY1).

## Sign-in and tokens

Most requirements of this section hold for riff 1.x only, until
riff 3.0 replaces them. Each of them ends with "Superseded by 3.0
(ID)."

- **R14** The first sign-in provider is Google. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3JZN229S3YA3BR6GN5H3MTY** riff has no built-in sign-in provider.
  No client ID and no client secret is in the source or in a binary.
  Each person who runs a riff with sign-in makes their own OAuth client
  and gives it to `riff-server` in the environment:
  `RIFF_OIDC_CLIENT_ID` and `RIFF_OIDC_CLIENT_SECRET`. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3JZN1XQVVNVD0MJVM8J91HC** A `riff-server` with an OAuth client
  (`RIFF_OIDC_CLIENT_ID`) requires sign-in, as with
  `--require-sign-in`. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3JZN1ZZED3FXQEFNJ4KVCN5** `riff connect claude` installs the
  plugin, then runs `riff login` when the riff has sign-in and this
  machine has no sign-in for it. An old sign-in
  (01M3JNVBRS35B3CD67367JF7SJ) does not count. When riff cannot check
  or the sign-in fails, the command warns and says what to do.
  Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R15** `riff-server` accepts the accounts of its allowed domains, and
  the people of 01M3JN3AFA2SAX0CEC1Y6E4NM5. The allowed domains are a
  setting. The default is `comotechnologies.io`. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R94** The domain of an account is the `hd` claim of its ID token.
  An account without `hd` is in no allowed domain. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3JN3AD44CC98AGMVP43F56G** The first person who signs in to a
  riff with sign-in is its owner. On a riff with admins (R210), only an
  admin becomes the owner. The owner is an admin. A riff that has an
  owner keeps it. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3JN3AFA2SAX0CEC1Y6E4NM5** These people can sign in to a riff
  with sign-in: the owner, the admins, the members, and the accounts of
  the allowed domains (R15). A member is named by verified email. A
  member needs no allowed domain and no `hd`. On a riff with no owner
  and no admin, the first person signs in. The refusal of a person
  tells them to ask the owner for `riff invite EMAIL`. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3JN3AHMK532XMRDASD4XD5D** `riff invite EMAIL` adds a member.
  `riff remove EMAIL` removes a member and ends each sign-in of that
  person (R20). Only the owner or an admin can do either. The owner
  cannot be removed. `riff members` shows the owner, the admins, the
  members and the allowed domains to each person who signed in.
  Superseded by 3.0 (01M519Z4AEMDFGBF22RZ3HQR1R).
- **01M3MEF4B33Z6WVJMDP29C7SS2** `riff invite EMAIL` prints the public
  address of the riff (the `--public-url` of `riff-server`) and the
  lines that the person runs to join: install `riff`, set
  `RIFF_SERVER` to the address, and `riff connect claude`. The lines
  hold no secret. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3MEFG6F102T1H8DFJ38EJ4A** The book page "Start a Team Riff"
  starts a riff with sign-in for a team, on a Linux host that the team
  reaches, behind a TLS proxy: the OIDC app of the team in the
  environment of `riff-server`, `--public-url`, `--owner`, a sign-in of
  the owner, and `riff invite EMAIL` for each person. No secret is on
  the page. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3JN3ANE676DT5WQ2NTG47DK** Replaced by
  01M3XA875QZ584JBGA37853PWX.
- **01M3JY7T109BR860EQBSKEFDHY** `riff admin add EMAIL` makes a person
  an admin and a member. `riff admin remove EMAIL` makes an admin a
  member again. Only the owner can do either. The owner stays an admin.
  `riff remove` refuses an admin that the owner made. Superseded by 3.0
  (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3JY7T3645CMQ8CS4T4ABZTP** `riff-server` keeps the admins that
  the owner made with the owner and the members. They stay after a
  restart with a bucket. The admins of R210 add to them. `riff members`
  shows both. Superseded by 3.0 (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3JYX8NPZASQY6031R35H39P** `riff owner EMAIL` passes the owner
  role to a member or an admin. Only the owner can. A riff has one
  owner at a time. The old owner stays an admin. Superseded by 3.0
  (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3JYX8QSEZDB5RZJ3Y57DR4Y** `riff-server` keeps a passed owner
  role in its state. It stays after a restart with a bucket, also when
  `--owner` names the old owner. Superseded by 3.0
  (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3MN14ZCTRVD3T455P6TFK1B** After `riff-server` makes a change of
  the members, `riff invite`, `riff remove`, `riff admin add`,
  `riff admin remove` and `riff owner` post a note of the change to the
  thread of each repository of the riff. The note names the user that
  made the change, the email and the change. It wakes no session. The
  client signs it. A repository of the riff is the repository of a
  session in `riff who --all`.
- **01M3MN1537Z0K3BRK6H2BZKZT0** A note of a change of the members
  holds no secret and no token. When the post of the note fails, the
  command still prints the change, and prints the error of the post.
- **01M3MN157X8N9QKER1AJEPEJVX** `riff members` lists each person
  once, with the highest role: owner, then admin, then member.
  Superseded by 3.0 (01M519Z4AEMDFGBF22RZ3HQR1R).
- **01M3N7K3ZAZFGABN7032AYJWEM** `riff owner --take`: an admin asks
  for the owner role. Only an admin can. `riff-server` tells the owner
  at once: a direct message to each live lead of the owner, and a note.
  One request waits at a time. A second request is refused, and the
  refusal names the admin that asked first. Superseded by 3.0
  (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3WRJAFS6W3J2ZRJ6XSW3SB5** `riff owner --take` by the owner is
  not an error. It says that the user is the owner already, and exits
  with 0. It changes nothing: no request, no note and no message.
  Superseded by 3.0 (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3N7K41N03P26BEFFNX5617K** The owner answers a request in N
  minutes. `riff owner EMAIL` passes the role and ends the request.
  `riff owner --deny` keeps the role: the riff tells the admin with a
  direct message to each live lead of the admin, and a note. With no
  answer in N minutes, the admin that asked is the owner. The old owner
  stays an admin. Superseded by 3.0 (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3N7K443DGPZ8XH5WWKK6M35** Replaced by
  01M3Q5460YESBSQHTV3M15PE53.
- **01M3Q5460YESBSQHTV3M15PE53** `riff-server` has three settings of
  the owner role, each 1 or more: N is `--owner-take-minutes`
  (`RIFF_OWNER_TAKE_MINUTES`, default 10). M is `--owner-ping-minutes`
  (`RIFF_OWNER_PING_MINUTES`, default 10). P is `--owner-pings`
  (`RIFF_OWNER_PINGS`, default 3). Superseded by 3.0
  (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3N7K46H5BRFJCB46P3JNAFZ** Replaced by
  01M3Q546335NBTKG5BHQ27QC93.
- **01M3Q546335NBTKG5BHQ27QC93** `riff-server` checks the owner each M
  minutes, while the riff has an owner and an admin who is not the
  owner. A check misses when no session of the owner is live, and the
  owner made no call since the last check, also as a person. A check
  wakes no session. After P misses in a row, the server warns the
  owner: a note to the sessions of the owner in the thread of each
  repository, and one line in the chat. When the next check misses too,
  the owner is gone, and stays an admin. The admin of a request that
  waits is the owner at once. With no request, the riff has no owner,
  and asks for a volunteer: a direct message to each live lead of each
  admin, and a note. Superseded by 3.0 (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3N7K48XQ8XSP7R0HD535ZX3** Replaced by 01M3Q63NNC6SC03BFCG80M7B4D.
- **01M3Q63NNC6SC03BFCG80M7B4D** On a riff with no owner,
  `riff owner --take` of an admin makes that admin the owner at once.
  `riff members` shows the owner `none` and ends with the line that
  says that the riff has no owner, in yellow. `riff admin`,
  `riff owner EMAIL` and `riff owner --deny` are refused, with a text
  that names `riff owner --take`. A sign-in makes no owner. Superseded
  by 3.0 (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3N7K4BC1RPZKQ1XNDTBRPGF** `riff-server` posts its own notes and
  messages as `riff://riff@server`. The USER `riff` belongs to the
  server: no person signs in with it. A post of the server has no
  signature. The server is not a session: `who` does not show it.
- **01M3N7K4DVHSF7AQ402F14J26Z** Each change of the owner role posts
  one note to the thread of each repository of the riff: a request, a
  pass, a deny, a grant after N minutes, a gone owner, and a take on a
  riff with no owner. The note wakes no session and holds no token. The
  server posts each of these notes, except the note of a pass
  (01M3MN14ZCTRVD3T455P6TFK1B). Superseded by 3.0
  (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3N7K4GAKJ621V5AWJRQVF3M** `riff-server` keeps a request that
  waits, and a riff with no owner, with the tokens in its state. They
  stay after a restart with a bucket. A request whose time ended while
  the server was down is granted at the first look after the load.
  `--owner` names no owner on a riff with no owner. A riff with no
  owner counts as a riff with an owner for the listen address
  (01M3JN3AQMHZHT6JP3P6GM9PWZ). Superseded by 3.0
  (01M519Z3Q27V93QTFKXPQX42J1).
- **01M3JN3AQMHZHT6JP3P6GM9PWZ** A riff with sign-in listens only on a
  loopback address until it has an owner, so the owner signs in from
  the machine of the server. `riff-server` refuses another address at
  start, also with `--insecure`. It checks again after it loads its
  bucket. Before the load, it counts `RIFF_OWNER` or a bucket as an
  owner. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3JN3ASSV9SA0QZKXXJ0RTEV** `--owner EMAIL` (`RIFF_OWNER`) names
  the owner of a riff that has none. The cloud deploy passes it from
  the GitHub Actions variable `RIFF_OWNER`. The deploy stops with an
  error when the variable is missing. Superseded by 3.0
  (01M4Z60B7E6X53NBJ01R95RYGE).
- **R16** `riff-server` issues its own tokens. It accepts sign-in from
  each OpenID Connect provider in its settings. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R17** An access token expires in 10 minutes or less. A refresh token
  changes at each use. A reused refresh token revokes all tokens from
  that sign-in, except after a lost reply
  (01M3MX4TG7PNNETZ986DQS10JJ). Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R110** Only the device key of a sign-in can revoke it by reuse. A
  reused refresh token with another key is refused and changes nothing.
  Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R116** Replaced by 01M3TFG4SJ5C96NH8W7XRXJG6Z.
- **01M3TFG4SJ5C96NH8W7XRXJG6Z** A refresh token names its chain and
  its generation: `chain.generation.secret`. A refresh with the current
  generation gives the next generation. `riff-server` keeps the hash of
  the refresh token of the current generation of each chain, and of the
  generation before it. A refresh token of an older generation is
  reused (R17), at any time. A refresh token with a wrong secret is not
  known, and changes nothing. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3TFG4PN1DWY1FXX0SVB3H3R** Replaced by 01M3WFVAB44T8EP4QZD4KS7DRF.
- **01M3WFVAB44T8EP4QZD4KS7DRF** Replaced by 01M52JYGWD3ZJKNYE3HG5BJYTP.
- **01M52JYGWD3ZJKNYE3HG5BJYTP** A sign-in has one chain: the chain of
  the person. A session token has no refresh token and no chain: the
  reply to the swap has an access token and an empty `refresh_token`.
  A new session token ends no other token, so each process of a
  session keeps its own. The token store keeps the hash of each session
  token (01M52JYGSNTFPQ0ZP50AD2BG89). Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3TFG4WE7CZQ4TCJE2NTC52E** After a start, the saved token store
  can be one generation behind. So the first refresh of each chain
  after a start takes the current generation of the saved store, or
  the next one, as good. `riff-server` checks the device key first
  (R110). Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3TFG4ZCWS98R7W6RYZFWZXF** Replaced by 01M3WFVADCDZM8XX590KAEMEYG.
- **01M3TFG551C76BP4TRA32P7VC3** Replaced by 01M52JYGSNTFPQ0ZP50AD2BG89.
- **01M52JYGSNTFPQ0ZP50AD2BG89** The token store keeps the hash of each
  live access token, of a person or of a session, with its sign-in, its
  session and the time when it expires. So an access token works after
  a restart and after a deploy, until it expires, and a new start makes
  no client refresh. A new token from the last second before a crash
  can be lost: its client refreshes one time. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3MX4TG7PNNETZ986DQS10JJ** A used refresh token counts as
  reused only after the refresh token of the pair that its last use
  gave was used. Before that, the reply of the last use was lost: when
  the token comes back with the device key of its sign-in,
  `riff-server` ends that unused pair and gives a new pair. So a stolen
  refresh token that comes back after the next refresh still revokes
  the sign-in. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3MX4TSEH18FSNQ28GEH2GFJ** `riff` says that the sign-in ended
  only when `riff-server` refuses the grant with `invalid_grant`. Each
  other error of a refresh keeps its own text. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3W947QF6PFBWR28ZVXCVQHG** `riff` sends a refresh token that
  `riff-server` refused one time only. It keeps the sign-in of the
  machine as ended, with its user and with no token. Then each `riff`
  process says that the sign-in ended with no call to `/v1/token`,
  until `riff login`. `riff connect claude` signs in again. Superseded
  by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3MX4V43SF2XFCZWANHD19WV** `riff-server` checks no version on
  `/v1/token` and `/v1/sign-in`, and `riff` checks none on their
  replies. So `riff login` and a refresh work when the versions do not
  match. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3MX4VCEBTY0DN4JMF624WYE** When `riff-server` replies 401 to a
  call with a token, `riff` drops that token, gets a new one, and sends
  the call once more. When the server refuses the person token in a
  swap for a session token, `riff` refreshes the person token once and
  asks again. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3ND6R8YXN1KTRTRAV5A7F14** `riff` gets each access token in a task
  of its own. One refresh runs at a time, and a call that waits for a
  token never waits on a call that its own task does not poll.
  Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M3MX4VM8CK1GAGJAM2P29NWH** When the sign-in of the machine ended,
  `riff watch` and `riff tail` say so once and try again (R148). Each
  tool of `riff mcp` says so in its result. After `riff login` on the
  machine, each of them goes on with no restart. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R80** A sign-in ends when none of its refresh tokens is used for
  30 days. The person then signs in again. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R81** `riff-server` keeps only a hash of each token, never the
  token itself. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R90** `riff login` signs in with the provider that `riff-server`
  names. It opens the browser. The code comes back to a loopback port.
  The client uses PKCE with S256. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R112** Only a request with the right `state` ends the wait on the
  loopback port. It reads at most 8 KiB of the request line. Superseded
  by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R91** `riff-server` swaps a valid ID token of its provider for the
  first riff tokens. The email in the ID token must be verified.
  Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R117** Each fetch from the sign-in provider stops after 10 seconds.
  Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R146** At start, `riff-server` checks its OAuth client with the
  provider. When the provider refuses the client, `riff-server` stops.
  Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R153** When `riff-server` cannot reach the provider at start, it
  logs a warning and serves. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R208** USER is the part of the verified email before the `@`, in
  lower case. Each character that a URI part cannot hold becomes `-`.
- **R209** A USER belongs to one verified email. The first email that
  signs in with a USER holds it. `riff-server` refuses each other email
  that gives the same USER. The refusal names the USER.
- **R93** `riff logout` removes the sign-in at one server from the
  device. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R18** Each token is bound to a key that stays on the device.
  Superseded by 3.0 (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R86** Tokens use DPoP (RFC 9449) with ES256. Each request with a
  token carries a new proof from the device key. A proof is valid for
  5 minutes, and up to 10 seconds in the future. Superseded by 3.0
  (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R87** `riff-server` refuses a bearer token, a proof that it saw
  before, and a proof for another method or URL. The URL is the public
  URL of the server and the path. Superseded by 3.0
  (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R113** `riff-server` refuses a proof whose `jwk` holds a private
  key. Superseded by 3.0 (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R114** `riff-server` keeps at most 100,000 proof IDs. When it must
  forget an ID early, it refuses each proof as old as that one.
  Superseded by 3.0 (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R115** `riff-server` keeps a proof ID only after the request shows a
  valid token or ID token. A caller without one adds nothing. Superseded
  by 3.0 (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R142** At start, `riff-server` knows no proof ID. It refuses each
  proof issued before it started to serve. Superseded by 3.0
  (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R88** `riff` keeps one device key for each server, in the OS
  keyring. Superseded by 3.0 (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R19** Each session gets its own token. The token works only for that
  session. Superseded by 3.0 (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R103** Replaced by 01M3WFVADCDZM8XX590KAEMEYG.
- **01M3WFVADCDZM8XX590KAEMEYG** A session gets its token by token
  exchange (RFC 8693): it sends a person access token and its session
  ID. Before the session token expires, `riff` swaps the person token
  for a new one. Superseded by 3.0 (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R104** A token acts only as its caller. The user and the session ID
  in `me` must match the token, or `riff-server` replies 403. A person
  token acts only as the person, with no session ID. Superseded by 3.0
  (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R105** Only a person access token gives a session token. Superseded
  by 3.0 (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R106** `riff` keeps a session token only in the memory of the
  process. Only the person tokens go to the keyring. Superseded by 3.0
  (01M4Z60B7W6F3CC5XJE0VS7Y86).
- **R107** Only one `riff` process at a time refreshes the person
  tokens of one server. A lock file makes the others wait. Superseded by
  3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **01M52JMEKD4P2ZT659CBNPN9CC** The lock file of R107 is in the local
  directory of riff (`riff::local::dir`), not in `TMPDIR`. So each
  `riff` process of one user on one machine finds the same lock file,
  also a worker with a `TMPDIR` of its own. With `RIFF_HOME`, it is in
  `$RIFF_HOME/state`. A process that sets the store of its secrets
  itself, a test, keeps its sign-in in that store, and its lock file in
  `TMPDIR` (01M3MY2KWKBJCQ0BCNC6533RBW). Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R20** A person or an admin can revoke all tokens of a person at
  once.
- **R101** `riff logout --all` ends each sign-in of the caller, on each
  device. An admin adds `--user USER` to end the sign-ins of another
  person.
- **R210** The admins are a setting of `riff-server`: `--admin EMAIL`
  or `RIFF_ADMINS`. Each admin is named by verified email. A name that
  is not an email names nobody, and the server logs a warning at start.
  There are no admins by default.
- **R111** Admin emails, user names and the named person compare
  trimmed and in lower case.
- **R21** A client keeps tokens and keys only in the OS keyring. The
  one exception is `RIFF_HOME` (01M3MY2KSV73WS8D902YCH2PRX).
- **R82** `riff` keeps each secret under the keyring service `riff`.
  On Linux, it needs a Secret Service, for example GNOME Keyring or
  KWallet.
- **01M3NBTZDT67WD9ZX0RHDVCW9T** `riff` waits at most 10 seconds for
  one call to the OS keyring. A keyring that does not answer in that
  time is an error that says so. It never blocks the process or its
  Ctrl-C.
- **01M4385CCATXC0B8HV1XD6EFWG** When the OS keyring is locked or does
  not answer, the error of `riff` names the host, says to unlock the
  keyring at the desktop, and says that `gh` stops too.
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
- **01M54510PS9FDKQKJB0RAH3XG3** A `riff` process opens the OS keyring
  at most one time, and reads each secret at most one time. It keeps
  the value for its later calls, and a write or a delete of the process
  changes the kept value. Only the refresh of the sign-in reads it
  again, under the refresh lock (R107).
- **01M556QPPRSQ91HDED128G2X7G** On Linux and BSD, `riff` opens each
  session of the Secret Service as a plain session, as `gh` does, not
  as a DH session. It keeps each secret as the store of the `keyring`
  crate did: an item in the default collection with the attributes
  `service` and `username`. So it reads and writes the secrets of an
  older `riff`.
- **01M54510SFK1Z9VPFYBYYSV10F** The `gh` calls of one clean of the
  worktrees (01M3ZV0TKSHNW5QC2NG1XTJEJB), of `riff top`, and of the
  rollout and the look of the lead share one token. riff reads it with
  `gh auth token` at the first call, and again at the first call after
  10 minutes. It gives the token to each call in `GH_TOKEN`: in the
  environment of `gh` only, never in a command line or a file. With no
  `gh` call, riff reads no token. When the environment has `GH_TOKEN`
  or `GITHUB_TOKEN`, riff reads no token.
- **01M54510W507A3T5JRTZ55H0ZQ** When `RIFF_KEYRING_LOG` names a file,
  `riff` adds one line to it for each call to the keyring store: the
  process ID and `read NAME`, `write NAME` or `delete NAME`. The first
  call of a process adds the line `open` before it. The lines hold no
  value of a secret. It is for tests and checks.
- **R22** `riff-server` follows the MCP authorization spec, revision
  2026-07-28. Superseded by 3.0 (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R83** `riff-server` has no authorization endpoint for now. Its
  metadata lists no response type. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R84** The public URL of `riff-server` is a setting. It is the OAuth
  resource and the issuer. Each token is only for it. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R85** With the setting `--require-sign-in`, each route except the
  token endpoint, the sign-in route and the metadata needs a live
  access token in the `Authorization` header. Superseded by 3.0
  (01M4Z60B8C8JBV1T8T0W6ZKFE6).
- **R47** Each session connects through `riff`. Direct connections from
  an agent tool are not supported for now.

## Terminal

Most requirements of this section hold for riff 1.x only, until
riff 3.0 replaces them. Each of them ends with "Superseded by 3.0
(ID)."

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
- **01M3JPQT13ANVA7DNJDVNJ0S8P** The settings of riff on a machine are
  in `$XDG_CONFIG_HOME/riff/config.toml`, or
  `~/.config/riff/config.toml`. A change keeps each other key.
- **01M41FMA3FGF3TKTXPBND9NDW1** A change of the settings writes a new
  file and renames it over the old one. A process that reads the
  settings at the same time reads the old or the new settings, never a
  part. A settings file that is a symbolic link stays a link.
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
- **01M49JW9Y8SNT3J242SF646DF4** The fast-forward of the main clone and
  the WIP push of the `leave` tool act only on a dir that is the top of
  a git worktree. For a dir in a repository above it, the fast-forward
  changes nothing and says nothing, and the `leave` tool refuses.
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
- **01M3WN72ECF0WKR4M7M6ZYAF9J** Each stream of the client (`watch`,
  `tail`) has a connection of its own. That connection is never in the
  pool of the client, so no call uses the connection of an open stream.
- **01M48RW9E8NS2FPHFHG2S10R7A** Each call of the client has time
  limits: 5 seconds for a connect (`CONNECT_WAIT`), and 20 seconds for
  each try (`TRY_WAIT`). The client of the calls sends an HTTP/2 ping
  each 10 seconds, also with no open call, and drops a connection with
  no answer in 5 seconds. On Linux, a TCP connection with data that
  gets no answer for 20 seconds closes. So a call never waits for ever
  on a dead connection. A try with no reply in its limit is a fault
  (01M4A8041F8EK1VYDE4C9QG8N8). When the budget of the call ends with
  no reply, the error says that the server gave no reply in the budget.
- **01M48RW9HNKPNZ75H9R01BG6V5** A stream of the client has the same
  limit for a connect, and no total limit. A stream that gives no byte
  for 45 seconds (`STREAM_IDLE`, three keep-alive comments of the
  server) ends, and `follow` connects again. So a stream that died
  with no sign, for example after a sleep of the machine, comes back.
- **01M4A803WN0KTDGGAX2E771XDF** Only the link talks to
  `riff-server`. A process has one link for each server: a map by the
  URL of the server. Each `Api` of one server shares the link: one
  HTTP client of the calls, one client of the streams, and what the
  process knows of the server.
- **01M4A803Z4Q0KX6NT1KC6QR43H** Each call has a budget: the time from
  its first try to its end. A short command, a tool call of `riff mcp`,
  a post of `riff chat`, a look of `riff top` and a call of
  `riff workers host` have `SHORT_BUDGET`, 60 seconds. The start hook
  has `STATE_WAIT`, the end of a session `END_WAIT`, and the status
  line one try in `STATUSLINE_WAIT`. The budget is a deadline: the link
  cuts the open try at it. Each try has the limit
  `min(TRY_WAIT, the budget that is left)`. The open of a stream is one
  try, and a stream has no budget.
- **01M4A8041F8EK1VYDE4C9QG8N8** Each try ends in a reply, a fault or a
  refusal. A fault is no connect, a cut before the end of the reply, no
  reply in the limit of the try, a 5xx or 429 with no build header, or
  a 503 of `riff-server`. A fault gets a new try after a wait, while the
  budget lasts. The wait grows from 250 ms, double each time, to
  `MOST_WAIT`, 5 seconds, less a random part of up to half of it. A
  refusal is each other reply of `riff-server`, and a build that riff
  cannot talk to. It gets no new try. A 401 to a token gets one new try
  with a new token.
- **01M4A8043S2ZCKRH19Z3Q8AJ1F** A fault with no HTTP reply (no
  connect, a cut, no reply in time) makes a new HTTP client of the
  calls of the link, which each `Api` of the server shares. A 503 or a
  429 keeps the client.
- **01M4A804683G1EXM53893VHW7S** A refused connect to a loopback address
  of a server that never replied to the process ends the call at once.
  It is no fault. Each other connect error is a fault.
- **01M4A8048J60YSVNVYF2432KE8** Each call of `riff` has a call ID: 16
  random bytes in base 64 with no padding, in the header `riff-call`.
  Each try of the call sends the same ID. So `riff-server` runs a
  command one time only (01M48VFX22S4811DYBBD7QDW24), also when riff
  sent it again after a cut or after no reply in time.
- **01M4A804AWYEHPYZ966A6PXRPF** `riff chat` posts the typed lines from
  a task of their own, in their order. Its screen takes input while a
  post waits. At its end, it waits for the posts of the typed lines.
- **01M4A804D7JAPRAT1YKD09ATK3** A test sets the budget of the short
  calls of a `riff` process in milliseconds with the environment
  variable `RIFF_LINK_BUDGET_MS`.
- **01M4ZQVVK4P72XMM37RXG0C8ZH** A test sets the limit of each try of a
  `riff` process in milliseconds with the environment variable
  `RIFF_LINK_TRY_MS`.
- **01M48RW9MA30A12E7XWX047CJ0** The server ends a `watch` or `tail`
  stream when the stream lags behind its buffer of events. It does not
  drop the events with no sign. The client connects again, and a
  `watch` gets the newest wake that its session did not read.
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
- **01M51BF5S2YKB8JG10DF6YVT42** The trailer `Issue: #N` of a pull
  request names its issue, as for `riff verify`. With no trailer, its
  branch names the issue (`worktree-issue-12`). The free work of the
  rollout and of `riff wave run`, the line of a claim and the look of
  the lead find the pull request of an item so. A pull request on a
  branch that names no issue, for example a release branch, counts for
  the issue of its trailer: while it waits for a verify or for the
  merge, the item is no free item (01M3Z9N5HHHS1E17NFGMVBKZ0K). After a
  failed verify, the item is free.
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
- **01M3MX1DYY6AVDW946NR0B9T2C** A build names the crate version, the
  last commit that changed `crates`, `Cargo.toml` or `Cargo.lock`, and
  the UTC time of that commit. The crate version is a semantic version.
  Its line is the major, and the minor while the major is 0. A `riff`
  and a `riff-server` of the same line talk.
- **01M3MX1E1EY1M7JGNCN6FCEVQK** `riff-server` also talks with a `riff`
  of the line before its own: `0.3.x` on a `0.4` server, `1.x` on a
  `2` server. It does not talk with a `riff` of a later line.
  Superseded in part by 01M55FCZNNZGKM4EKAXZQFTDWR: the lines 1 and 3
  talk, in both directions.
- **01M514C19H3WQP1FSX9RBVSC69** A build is older than another build
  when its version is lower, or when the version is the same and the
  time of its commit is older. A build of a higher version is not
  older, also from an older commit.
- **01M3MX1E3R5WESVHA8RZXFQR1J** Replaced by
  01M3N73E5JWFTYZ90JX5AVFGEP to 01M3N73EEQ4HPCPPAGCAHH3S6B.
- **01M3N73E5JWFTYZ90JX5AVFGEP** While the major is 0, the minor has
  the role of the major.
- **01M3N73E7YTHFX2J2KXT7017QX** A patch release (`0.3.0` to `0.3.1`)
  changes nothing that another machine or session can notice: a fix
  that restores the intended behavior, the docs, an output format, the
  tests, or words of the skill or of a requirement that make a rule
  clearer with no change in behavior.
- **01M3N73EAAD1G88TG7SNFWE4P1** A minor release (`0.3.x` to `0.4.0`)
  has a change that another machine or session can notice:
  - the wire: a route or a field that a client needs, a removed one,
    or a new meaning;
  - the saved state of `riff-server`;
  - the plugin contract: the hook input, and the names and arguments
    of the riff tools;
  - the behavior: a change of the skill or of a requirement that
    changes what a session does, for example the pull request flow,
    the verify, the claims, the waves or the pause.

  A wire change stays additive for one line, so that `riff-server`
  serves the line before its own (01M3MX1E1EY1M7JGNCN6FCEVQK). Riff
  does not bridge a difference in behavior: the version note tells the
  older session to update.
- **01M3N73ECKSDSH6820AKW9J8F7** The major release `1.0.0` promises
  that the wire and the behavior stay stable. After it, a major
  breaks, a minor adds and a patch fixes. A change of behavior that
  can break a pipeline or a policy is a major. Superseded in part by
  01M53N7KN4QKZ70W2GNQ3VQWQ9: on the line 1.x, a minor.
- **01M3N73EEQ4HPCPPAGCAHH3S6B** The test for each release: can a
  session on the old version and a session on the new version, in the
  same riff, act differently or fail to understand each other? Yes: a
  minor, and a major after 1.0. No: a patch. A wave release is a patch
  unless the wave has a minor change. Superseded in part by
  01M53N7KN4QKZ70W2GNQ3VQWQ9: on the line 1.x, a minor in place of a
  major.
- **01M53N7KN4QKZ70W2GNQ3VQWQ9** On the line 1.x, a release that the
  test of 01M3N73EEQ4HPCPPAGCAHH3S6B makes a major is a minor. The
  next major is `3.0.0` (01M4Z60B8N9W9TBE7KKDJQAN2C). A release of only
  fixes, where the old and the new builds act the same, is a patch.
- **01M3N73EGY4NQDQQP8Y185E9VZ** The pull request of a release states
  the level and the reason in one line. The release notes list the
  commands that each person runs, for example
  `riff update --tag vX.Y.Z` when the old `riff` cannot read the new
  server.
- **01M3JEE7P46GWXR1BD4Q1TTSGN** Each call of `riff` names its build in
  the header `riff-build`. Each reply of `riff-server` names the build
  of the server in the same header.
- **01M3JEE7RDTDD3KQMKH41E8D57** Replaced by
  01M3MX1E65XGWDZ062PQ9YXQ5T.
- **01M3MX1E65XGWDZ062PQ9YXQ5T** `riff-server` refuses each call of a
  `riff` that it cannot talk to, or that names no build, with status
  409. `riff` refuses each reply of a `riff-server` that it cannot
  talk to, or that names no build. The error names both builds, the
  lines that the server talks with, the older side, the step to update
  it, and the link to the book. The OAuth metadata stays open to each
  client.
- **01M3QCMJ9F1GRTRRSB4AW9TC3D** `riff` checks the build only of a
  reply that comes from `riff-server`. A reply with status 5xx or 429
  and no build header comes from the front end: a short outage, not
  another build. `riff` tries the call again, as after a 503 (R132).
  After the last try, the error is a plain error, not a version
  error. So `riff chat`, `riff tail`, `riff watch` and `riff mcp` go
  on quietly. A reply with another status and no build header is
  still a version error. Its text names the status and the URL of the
  reply.
- **01M56WNP3TEGH98B9EAM4H62P4** At a deploy, Cloud Run can send calls
  to the old instance for a few seconds after it exits. Cloud Run then
  replies 500 or 429 with the text "The request was aborted because
  there was no available instance", and no build header. Each client
  of `riff-server` tries the call again, as after a 503
  (01M3QCMJ9F1GRTRRSB4AW9TC3D): each command, `riff mcp`, `riff watch`,
  `riff tail`, `riff chat` and the daemon. The status line shows
  `(not in the riff)` for one refresh, as after a 503. `riff server`
  and `riff update` take that reply as a server that does not answer,
  not as a server with no build.
- **01M56WQBZ629ENP5E12RN3WYDH** The check after a deploy counts the
  500s of `riff-server` and the replies of Cloud Run with "no available
  instance" apart. A 500 of `riff-server` is a bug. A reply of Cloud
  Run in the hand-over is not.
- **01M3MNVT9TYNXZ8V845BHKQADV** Replaced by
  01M3MX1E8M9TKBN90P4DYKH3H8.
- **01M3MX1E8M9TKBN90P4DYKH3H8** With another build that it can talk
  to, `riff` goes on. Each `riff` process prints one note to stderr:
  `riff-server runs build X; this riff runs build Y. Run riff update
  when you can.` When `riff` is on the line before the server, the
  note names the next line of the server that refuses it, and ends
  `Run riff update soon.` When `riff` is newer, the note names the
  older side (01M56Q55SHHJSKJWNHMW79X7HA). `riff server` shows both
  builds and that the versions can talk.
- **01M3JEE7TPZMNK7X6JXJ7GWFPP** When the versions cannot talk, the
  start hook gives the session the error, and tells it to tell its
  user at once and not to use the riff. Superseded by 4.0 (#1173).
- **01M3MNVTC248YYJJQKFD9H1WY9** On a version that they cannot talk
  to, `riff watch` and `riff tail` do not stop. They print the error
  once, try again every 5 seconds, and go on when the versions can
  talk. When a new `riff` binary is on disk, they run it in their
  place with the same arguments.
- **01M3NJGD45GF7Y4CZWQ7GRDHZN** After an update, the new binary of
  `riff watch` and `riff tail` keeps the host, repository and worktree
  of the old one. It does not read them from its directory. When the
  working directory is gone, the new binary runs in the nearest parent
  directory that exists, and the process says so on stderr.
- **01M3NJGD6H8DNVHHHG80F9YFCE** When `riff` cannot read its working
  directory, the error names the directory from `PWD`, and tells the
  person to change to a directory that exists.
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
- **01M3NT6WXGCNKW3EQ7MBJDQTR4** When a new `riff` binary is on disk,
  `riff top` and `riff chat` run it in their place, as `riff tail`
  does. The chat does it between two lines. The new chat shows no
  line of the old chat again, and draws its prompt again.
- **01M3Q55KJ8BKMPE9RADB63X8SP** When a new `riff` binary is on disk,
  `riff workers host` runs it in its place, as `riff tail` does, but
  only between two requests of the lead. The new host keeps the
  session of the old one. Its workers go on.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M3Q55KMQSSJVQEN86XFB8PSG** Each process that `riff` starts, for
  example a worker pane, runs the `riff` binary on disk. It never runs
  the path of a binary that a new one replaced.
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
- **01M3JEE7WT04BKX377VW5GDSPY** `riff --version`,
  `riff-server --version`, `riff whoami`, `riff who` and the whoami
  tool show the build.
- **01M3JEE7YXQPWS65FBVTASAEBX** The image build of `riff-server` has
  no git. It gets the commit and its time as `RIFF_COMMIT` and
  `RIFF_COMMIT_TIME` from `deploy/build-id.sh`, which uses the same
  git command as the build of `riff`. Superseded by 3.0
  (01M4Z60B7H67YSP22NMGT3DM7X).
- **01M3N7JJC5WQBJ7SJZSZNBAVVR** `riff update --auto on` and
  `riff update --auto off` set `update.auto` in the settings of the
  machine, and install nothing. The default is off. `riff update
  --auto` with no value shows the setting.
- **01M3NT6WV8Q8EFZBK8DHYKW5CC** On a machine with no `update.auto`
  key, `riff login` and `riff connect claude` ask the person once in a
  terminal: `Update riff by itself when the riff gets a new release?
  [Y/n]`. The answer sets the key: `n` or `no` is off, each other
  answer is on. With no terminal, they do not ask.
- **01M3N7JJEKZMN1E5NJQRK2QYVB** With `update.auto = true`, when a
  `riff` process gets a reply from a `riff-server` of a newer version
  than its own, it starts `riff update --tag vX.Y.Z` of the release of
  that server in the background, and prints one note. It never
  installs an older release. With `update.auto = false`, it starts
  nothing, and the note to update stays.
- **01M3N7JJH0SXXQYYBAHWPCNQGX** One update in the background runs at
  a time on a machine. It tries each release once: many processes
  that see the same release start one update, and a failed release
  waits for the next release.
- **01M3N7JJKBME6VSNTHD8VPN3K9** After an update in the background,
  the lead of the user gets one direct message from the person: the
  host, the old release and the new release. After a failed update,
  the message holds the error, and the old binaries stay.
- **01M56P1T4KK03MF7EK5CC0YZNB** When the direct message to the lead
  after an update in the background fails, the update tries it again
  every 5 seconds, for at most 2 minutes. The lead gets it once.
- **01M58AW5SBG3ZP7149ZY66VTYW** The update in the background tells
  the lead in the repository of the process that started it: a
  session, or `riff workers host`. The daemon never takes a place
  from its working folder, and starts the update with no place. An
  update with no place, or with a place outside each repository,
  tells the lead in each repository whose wave the daemon of the
  machine runs on the same riff-server. With no such repository, it
  tells no lead, and its log says why.
- **01M58AVFM0D62R48HKK8CFY4VW** The update in the background tells
  the lead before its step for the daemon
  (01M517BTDRKDXD3X84P6AEAF7P, 01M517BTJ9EV4ZGPYRB6HXJJC9). A new start
  of the unit stops the update that the daemon started, so the message
  goes first.
- **01M58AW9YR84SKSYBYBJ6V7TNJ** The daemon does not ask systemd to
  start it again (01M517BTP6BAGTRBYWPX9GPVE0) while an update of riff
  holds the update lock of the machine.
- **01M53NEE7W0ZC8B7EV9ET6QHBB** The update in the background prints
  no note that `riff-server` runs another build
  (01M3MX1E8M9TKBN90P4DYKH3H8): it installs that build. So no line of
  `update.log` names the old build as the build that runs.
- **01M3NT2Q0RNM9PVHT42V459624** An update in the background runs in
  the local files of riff, not in the working directory of the process
  that started it. It gets the place of that process, so the message
  to the lead finds the repository also when that directory is gone.
- **01M3NT2Q30P9GCQGENWMP1NKN2** `riff update` runs `cargo`, `git`,
  `riff connect` and `riff-server --version` in the home directory,
  else in `/`. So it works in a removed directory.
- **01M3NT2PYFHPB0C19Q2QB2AE6W** When the working directory of an
  update in the background is missing, the update stops before `cargo`
  runs, and does not mark the release as tried. The next `riff` process
  tries again, and the message to the lead says so. Each other failure
  marks the release as tried, so one release gets at most one install
  and one message to the lead.
- **01M3N7JJNPPJNTXFDTEY4MSDVJ** An update in the background stops no
  session, and changes no sign-in and no device key. The sessions take
  the new `riff` as 01M3MNVTC248YYJJQKFD9H1WY9,
  01M3NT6WXGCNKW3EQ7MBJDQTR4 and 01M3NT6WZTKAFKGDWGCFKC8TB5 say.
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
- **01M5A72C25A1FDZPW3FK2R1F92** The bridge of the lines 1 and 3 ends
  with the line 4. riff-server 4 talks with the line 4 and the line 3
  (the line before), and refuses riff 1. riff 4 refuses a server of the
  line 3 or 1. The bridge tests know the line 4 and fail on each later
  major with no rule.
- **01M5P1VAX6WNM9DX16NS4Z0SWC** The actions of the pipeline stay on a
  version that GitHub supports. Dependabot opens a pull request each
  week for an action with a newer version.

## Code

- **R12** All code is Rust.
- **R13** The repository stands alone. It is not part of a larger suite.
- **R123** The crate audit ignores no advisory. When a dependency has
  an advisory, we update it, change a feature, or replace it.
- **01M3J3FGCENZTA3JGZ6S3NX7YM** A new requirement gets a ULID from
  `just rid` as its ID. The ID has no meaning. Nobody writes it by
  hand. The old IDs R1 to R232 stay as they are. No new requirement
  gets an old ID. A requirement is never renumbered.
- **01M3J3FGEND8977RZTRS9C0V5Z** `just ci` fails when two requirements
  have the same ID, and when code, tests, the book or the notes for
  agents cite an ID that no requirement has. It warns when a
  requirement ID is not an old ID (R1 to R232) and not a ULID.
- **01M3MNT28NXA9VRST119QR8AQK** Each prose line of the book and the
  requirements is at most 72 characters. A line with one link or one
  code span and nothing to break is longer when it must be.
- **01M3WNN836EFG7GJQKZRTSK5FT** `just wrap` runs `hygiene wrap
  docs/src`. It fails on each line that breaks
  01M3MNT28NXA9VRST119QR8AQK (`wrap`). No test of a crate checks the
  wrap.
- **01M51K1VMWTNZGD6PT1BYDGWM3** No Markdown file of the book or of
  the plugin has two blank lines in a row outside a code block. `just
  wrap` also runs `hygiene blank docs/src crates/riff/claude-plugin`.
  It fails on each blank line after a blank line (`blank`).
- **01M3T5MG7VVYG4A8YBM75FGSMB** `just book` runs `hygiene book docs`.
  It builds the book with mdbook, and fails on an `ERROR` line of
  mdbook (`mdbook`), on an include line that mdbook left in the text
  of a page outside a code block (`include`), and on an empty code
  block (`empty-code`). It skips an escaped include example in a code
  block.
- **01M3W5YW0172EVF2JA8T7WW392** `just ci` changes no tracked file.
  `hygiene book` installs the theme of the book when it is missing,
  and keeps `book.toml` as it is. It fails when the install or the
  build of the book changed a tracked file (`tracked`).
- **01M3WNMKB6PAP6J0QXX4A684HH** `just ci` runs only the checks that
  the diff can break. The diff is each file that differs from the
  merge base of `HEAD` and `origin/main`: committed, not committed, or
  not tracked. When each file of the diff is text, `just ci` runs only
  `book`, `reqs` and `wrap`. In each other case it runs each check,
  also when no file differs and when git cannot compare. A file is
  text when it is in `docs/` or `design/`, or when it is a `.md` file
  outside `crates/`. `just ci` prints one line that names the set and
  the reason.
- **01M3WNN7VQJKN5MJH7JN50VF4D** `just ci-full` runs each check for
  each diff: `fmt-check`, `lint`, `test`, `doc`, `book`, `reqs` and
  `wrap`. The Gate on GitHub runs the same checks
  (01M513W2K3HNRWPQTQNBQPT8MZ).
- **01M43DKYVAX0TJ2F5YYGYFSZ4G** One `just ci` or `just ci-full` runs
  at a time in a worktree. A run holds the lock file
  `target/.riff-ci.lock` with its pid and its start time, and removes
  it at its end. A second run while the holder lives stops at once
  with the exit code 1 and one line: `a just ci runs in this worktree
  already (pid N, started M min ago); wait for it, or stop it`. A lock
  of a process that does not run does not count. A run that the
  holder starts takes no lock. The check uses no cargo.
- **01M43DKYYEYW3TQ2CKS36VRZ0V** The skill tells a session to look for
  a full check of its own that runs before it starts one, and to wait
  for its end. A run in the background is one task.
  Superseded by 4.0 (#1173).
- **01M49HAZ5BZYGAR9PGC089RM3F** Each pushed commit gets one full test
  run: the Gate on GitHub. The forge merges only on a Gate pass. An
  author runs `just check` before a push, not `just ci`. A verifier
  does not run `just ci`. It reads the Gate of the commit, and does
  the work that a test run cannot do: a review of the code and of its
  fit with the design, a check of each `Done when:` criterion by its
  test, the book, the rustdoc and the requirements.
- **01M49HAZA5K08XW2JQ11TG87JP** `just check` runs `fmt-check`,
  `lint`, `doc`, `book`, `reqs` and `wrap`, then the tests of each
  crate of the diff and of each crate that depends on one of them.
  The diff is the diff of 01M3WNMKB6PAP6J0QXX4A684HH. A file in no
  crate needs the tests of each crate, also a text file: a test can
  read it. A tree that git cannot compare needs them too.
  `hygiene crates` prints the arguments of `cargo test`, and one line
  that names the tests and the reason. `just check` holds the lock of
  01M43DKYVAX0TJ2F5YYGYFSZ4G.
- **01M513W2K3HNRWPQTQNBQPT8MZ** The Gate on GitHub runs its checks as
  jobs at the same time: `Clippy` runs `just lint`, `Tests I/N` runs
  part I of N of the tests in the session image, and `Checks` runs
  `just fmt-check doc book reqs wrap`. Together they run each check of
  `just ci-full`. The last job, `Gate`, needs each of them. Each of
  them has the condition
  `github.event_name != 'schedule' && github.ref_type != 'tag'`, and
  `Gate` has `always()` and the same condition. So `Gate` runs also
  when one of them fails or is cancelled, and it passes only when each
  of them passed. The ruleset needs only `Gate`.
- **01M513W2NSE303E6XAM18BW7T6** `just test-part I/N` lists each test of
  the workspace, also each doc test, with `cargo test -- --list`. Of
  the tests that are not doc tests, it runs each test whose name has
  the 64-bit FNV-1a hash H with H mod N = I − 1, with `cargo test
  --tests --no-fail-fast -- --exact`. Part 1 also runs each doc test,
  with `cargo test --doc --no-fail-fast`: the name of a doc test has
  spaces, and rustdoc splits a filter at each space. It prints one line
  with the count of its tests and of all tests. A list with no test
  fails. The N parts together run each test.
- **01M513W2RERJES76ZXKKHCV0DP** Each job of the CI workflow runs on the
  runner that the repository variable `RIFF_RUNNER` names. With no
  `RIFF_RUNNER`, the runner is `ubuntu-latest`.
- **01M4A4T67MPF0BCNX7AFZRDA8J** The dev and test profiles of the
  workspace keep only the line tables for the code of the workspace,
  and no debug info for the dependencies. A backtrace still names the
  file and the line.
- **01M4A4T69XGN1XVJHGHVZN1MRJ** The integration tests of a crate are
  one test binary, `all`: `tests/all.rs` has one module for each file
  of `tests`. A file whose tests change a thing of the whole process
  is a test binary of its own: `back_in.rs` and `join_a_riff.rs` of
  `riff` set the keyring store, and `isolation.rs` listens on the port
  of the riff of the machine. Each other test of `riff` that needs a
  mock keyring uses `common::mock_keyring`.
- **01M514XARN6564BBWHWWYVCH9K** Each doc test of `riff` that sets the
  default store of `keyring_core` is a `standalone_crate`, so it runs
  in a process of its own. A unit test of `secrets` checks each file of
  `crates/riff/src`.
- **01M4A4T6C5DH311AXMM6AG54DV** A test fails when a file of `tests`
  of a crate is in no test binary: not a module of `tests/all.rs`, and
  not the path of a `[[test]]` in the `Cargo.toml` of the crate.
- **01M49W17GGV1K5FZEKJRPV0GNA** A type of the log, the wire or the
  checkpoint has one struct. A reader, a saved form or a reply that
  carries the data of a type holds that type, not a second struct
  with its fields. A tolerant reader holds it with
  `#[serde(flatten)]`. The read of the frozen format of v0.8.0
  (`import.rs`) is the one exception.
- **01M49W17M2JVNYSHDQJWHZ8A7X** Each conversion between two types of
  the log, the wire, the checkpoint or the state names each field of
  its source in a destructure with no `..`. A field that the target
  does not need is named with `_`. So a new field fails the build at
  each conversion. A type of the frozen format of v0.8.0 gets no new
  field, so the read in `import.rs` needs no destructure.
- **01M49W18ETF4KZJN848M91VY35** Each type of the log, the wire and
  the checkpoint has a round-trip test: a value of each variant with
  each optional field set, written to JSON, read back and compared.
  The schema of the type shows each field that a value leaves out,
  and each variant that no value shows. A test lists the serde types
  of `record.rs`, `wire.rs`, `selector.rs`, `signed.rs` and `dpop.rs`,
  and fails when one has no round trip. The round trip of the
  checkpoint starts at its root type, so it reaches each type in it,
  and a load of the state gives the same checkpoint again. The round
  trip of the log goes through the store.
- **01M49W18QQKF4KYDRYP3ZK9F1Q** A new or changed type of the log,
  the wire, the checkpoint or the state cannot hold an invalid state.
  For example, a saved session has a status, a step, or both: an
  enum, not two options.
- **01M5P2CWS1ENC30XNENRX11MY3** An integration test of a command that
  calls riff-server uses a riff-server that needs a sign-in
  (`common::start_signed`), and signs in the command with
  `common::sign_in`. The server notes each call with no token, and the
  test fails when it ends.
- **01M5R2XVDN99TE9NEJYY1GJE91** A text of riff, a line of rustdoc, a
  comment, a test and a page of the book name no version of riff as the
  riff of today: they say "riff runs each session in a container", not
  "riff N runs ...". A version name stays in only four places: a
  requirement, which is a dated statement; the release notes on GitHub;
  a rule between versions, with its tests; and the code that removes
  what a named old release left on a machine, with its texts and
  how-tos.
- **01M5R2XVGMTB2HC9PRMS85ZWTY** `just versions` runs `hygiene
  versions`. It fails on `riff N` or `riff N.M`, also `riff-server N`,
  in each file of `crates/*/src`, `crates/*/tests` and `docs/src`, but
  `docs/src/requirements.md` (`versions`). Each use that must stay is a
  line `PATH | TEXT | REASON` in `crates/hygiene/versions.allow`. It
  fails on a line of the list that allows no name. `just check`, `just
  ci-text` and the job Checks of the Gate run it.

## The phone app

The phone app is the design of #951, a child design of #982. The book
page [riff on a phone](riff-mobile.md) has the pictures. The build
items are #962, #963, #977, #964 and #965.

- **01M53NTQTHC8WHW9ZZ6S1VZ2W8** The first phone app shows the thread
  `chat` (the history, then each new line), posts a line, sends
  `@lead`, `@USER` and `/me` as `riff chat` does, and shows a notice
  when a line names its user. `riff top`, the questions of the lead
  and the steps of a lead come later.
- **01M53NTQXQTM29BT67E6KHZFCK** The phone reaches the chat through
  riff-server over HTTPS, on riff 1.x and on riff 3.0. It acts as a
  person, as `riff chat` does, with the calls of `riff chat`: join,
  read, tail and post.
- **01M53NTR10TGJ9PHHCEN1P15K7** The phone app is for Android 10 or
  later. It is a Slint app, all in Rust. The riff project writes no
  Kotlin or Java source for it. A crate or a library may ship its own
  Java. No part of the app draws in a web view.
- **01M53WZ08JY80X32PP6P7FGNWP** The app uses the royalty-free license
  of Slint, and shows the AboutSlint widget in its About screen.
- **01M53RWDFBC3Q8R6PEX9R0XAVD** The app builds on the core library of
  #982 (riff 3.1): the client of riff-server and the models of the
  views. It calls the parts of Android through JNI: the Keystore,
  Credential Manager, the camera and the notices.
- **01M53XPZTG203SNM9MBKBERMB5** One command adds, lists and removes
  each kind of device of a person:
  `riff device add [NAME] [--kind KIND]`, `riff device list` and
  `riff device remove NAME`. The kind
  sets the scope of the device at add time. The first kind is `phone`,
  the default. Each child design of #982 adds its kind and its scope.
  A machine of `riff login` is a device of the kind `machine`, with
  its host as its name and the scope of today.
- **01M53NTR47XBDX6C6QN49RNP29** A person adds a phone from a machine
  with a live sign-in: `riff device add [NAME]`, in a terminal. NAME
  is the name of the device, as the host name of a machine, and not a
  Google ID. With no NAME, the name is the kind, for example `phone`.
  The command refuses with no terminal. riff-server makes a pair code
  that works one time, for 10 minutes, ties it to the Google account
  of the sign-in of the terminal and to the kind, and keeps only its
  hash. The command draws a QR code with `tui-qrcode` that holds the
  address of riff-server and the code, and prints the code as text.
  The code is in no URL.
- **01M53WZ032G5W435GJMPVBHD71** The app reads the QR code with the
  camera, through the NDK camera API and the decoder `rqrr`. A typed
  code is the fallback.
- **01M53WZ05VFPG8SN581VZSWHKJ** After the pair code, the app signs the
  person in with Google through Credential Manager. riff-server takes
  the code, the name, the Google ID token and a DPoP proof of the key
  of the phone. The audience of the ID token is a Web OAuth client of
  the riff. riff-server makes the sign-in of the phone only when the
  phone signs in with the Google account that the code is tied to.
- **01M53NTR7D6145FSQ3WVZ1H52W** The app makes the device key in the
  Android Keystore: ECDSA P-256, not exportable, in StrongBox when the
  phone has it. The key never leaves the Keystore. riff-server binds
  the sign-in of the phone to the thumbprint of that key. Each call of
  the app has a DPoP proof of the key, as for `riff login`.
- **01M53NTRAKPS7QVCTFS2GJPX4G** riff-server sets the host of each call
  of a phone to the name of its pair. A line of a phone shows as
  `<USER@NAME>`. riff-server refuses a name that is the name of
  another device of the same person, or the host of one of its
  sessions or posts.
- **01M53NTRDKDBTCWX0QNFJY9D4K** The scope of the kind `phone` is the
  chat only: the token of a phone acts only on the thread `chat` and
  on the notices of that phone. riff-server refuses each other call of
  a phone token, for example a claim, a pause, an invite, `who`, a
  session token and a read of another thread.
- **01M53NTRGP2MJG6JT85BCVYE18** `riff device list` shows each device
  of the person: its kind, its name, the time of its sign-in and the
  time of its last call. `riff device remove NAME` ends the sign-in of
  that device at once, also of a machine. With `--user`, an admin
  names a device of another person. `riff logout --all` and
  `riff remove --email` also end the sign-in of each device of the
  person.
- **01M53NTRKQ894YHQD9YYRNJBHZ** The app takes the parse of `@lead` and
  `@USER`, the signed payload of a post, the check of a signature and
  the DPoP proof from riff-core. So the app and `riff chat` cannot
  differ.
- **01M53NTRPS73Z0TPVDH7DCM2J3** The app sends its own build, as each
  client of riff does. When riff-server refuses its release line, the
  app says to install the APK of the release of riff-server.
- **01M53RWDJ4J5TERZMBB3FC2ZKE** Gradle builds the APK, with no Java
  source of the riff project. Each release of riff attaches
  `riff-phone.apk`, of the same build, signed with the release key of
  the riff project. The key is a secret of the GitHub environment of
  the release. A person installs the APK by hand. When Android asks
  for a verified developer, the riff project uses a free account of
  limited distribution. Google Play comes later, if at all.
- **01M53NTRSTXCK3VP8YGQETEZX5** The app keeps no text of a message in
  its storage: only the alias of its key, its tokens and the number of
  the last line that it showed. A key of the Keystore encrypts the
  tokens.
- **01M53NTRZT0B7539TXA5BV2BR8** A phone gets a notice for a line of
  the thread `chat` whose `to` has a selector with its user and
  `lead=true`, from a sender of another user. It also gets a notice
  for each line of the verified lead of its user. A line of a person
  client of its own user gives no notice. riff-server finds the lines
  from the `to`, the sender and the lead mark, not from the body.
- **01M53NTS32HD38TSK335EY7V9C** While the app runs, it follows the
  chat, and shows a local notice through JNI with the sender and the
  line. In the background, the phone gets an FCM notification message
  with a fixed text, and no text of the line. The Firebase library
  shows it.
- **01M53NTRWVX0Z7R211JF1T6GKJ** riff-server sends the notification
  messages through Firebase Cloud Messaging (FCM), with the HTTP v1 API
  and its Cloud Run service identity, with no key file. The Firebase
  project is the Google Cloud project of riff-server. A riff-server
  with no FCM project sends no push. A phone with no Google Play
  services gets no background notice.
- **01M53NTS63FNFS2MESV5E9JZH4** The person turns the notices on and
  off in the app. The app asks Android for the permission to post
  notices at the first turn on. riff-server deletes the FCM token of a
  phone when FCM answers that it is not registered. A push never
  delays a post.
- **01M53NTSFADCRYKW71JSKW2504** The app calls only its riff-server,
  Google for the sign-in, and FCM through the Firebase Messaging
  library. It has no analytics, no crash report and no other library
  of Firebase.
- **01M53NTS97JVY20GS4BRR3NDDP** If a later release removes
  riff-server, the phone talks over iroh to a daemon of its user, as a
  client of that daemon. It is no peer of the riff, and holds no copy
  of its records. That daemon pairs the phone. That release chooses
  how a notice reaches the phone.
- **01M53NTSC8C86B8S0HN0A5XAHB** The design of end-to-end encryption
  (#958) chooses the encryption key of a phone, and whether a new
  phone reads the old lines. The app holds the encryption of #958, in
  Rust. The pair of a phone is the step where a machine of the person
  admits the new keys of that phone.

## riff 3.0

riff 3.0 is the design of #699. riff 1.x runs today. Each
requirement of this section holds for riff 3.0. The book part
[Sessions in containers](how-it-works.md#sessions-in-containers) has
the picture and the table of what sessions share. The re-cut of #953
changes the plan: see the next part.

### The re-cut: isolation by default

The requirements of the re-cut (#953) replace the plan of Wave C and
the riff of peers. A requirement that the re-cut replaces ends with
"Superseded by the re-cut (ID)." or "Superseded in part by the re-cut
(ID)." The build items of the re-cut hold the requirements of this
part when they merge.

- **01M53NGS8FGRNY12DDJA3P0GH2** riff 3.0.0 is isolation by default,
  with riff-server. The daemon of each machine runs each work item, a
  build or a verify, as a container session
  (01M4Z60B7NW4GHKGCYZZDG26R6). riff-server stays the server of the
  riff: the claims, the messages, the threads, the chat, the sign-in,
  the pauses, the holds and the update of each machine. The waves stay
  on the forge (R223). riff 3.0.0 has no riff of peers
  (01M53NGW0DWJ075C4MDP06WAGS).
- **01M53NGVB7SZJW7X8SYS3B0ZHG** In riff 3, each part of riff 1.x stays
  as it is, except the riff of peers (01M53NGW0DWJ075C4MDP06WAGS) and
  the start of a worker in tmux by the rollout
  (01M53NGVE839TRH7XB7CDPQNZE). The parts that stay include
  riff-server and its log, `riff cloud`, the sign-in, the tokens, the
  device key and the signed messages, the members and the roles of
  riff-server, the workers in tmux (01M53NGVQDKCTNMG54YDFE22T2), and a
  session on the host with the plugin, also a lead in tmux. A
  requirement of a part that stays that ends with "Superseded by 3.0
  (ID)" or "Superseded in part by 3.0 (ID)" still holds in riff 3, for
  each ID. R203 is one of them: a second machine can still use the
  riff-server of a first machine.
- **01M53NGWTNC3655ZZJ76M1R5R7** In riff 3.0.0, the claim of a
  container session is a claim on riff-server, as in riff 1.x. The
  daemon makes it as the worker session of the item
  (01M4ZV3B6Q0Y6AX0KSKC55S31R). The riff tools of each container
  session act on riff-server (01M4ZV3B6T2KB09WJ59F07HXET).
- **01M53NGVE839TRH7XB7CDPQNZE** The rollout of riff 3 runs the wave of
  the daemon (01M4ZV3B6J89KJQ0F14YWTZMJQ). When the rollout of a lead
  finds free work, it asks the daemon of its machine to run the wave of
  the repository, when no wave of that repository runs there. It starts
  no worker in tmux by itself. It still gives free work to each idle
  worker in tmux, as in riff 1.x. `riff lead start` also starts the
  wave of the repository on its daemon, when no wave of it runs.
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
- **01M5515CFHRA6AJG4GKX0ZV77W** `riff wave stop` stops the wave of the
  daemon until the next look of the rollout with free work. To keep it
  stopped, a person pauses the repository.
- **01M5515CJ7QDJ639VGR7SJTE08** When `riff lead start` cannot start the
  wave of the repository, it prints the reason and the step
  `riff wave run`, and it starts the lead.
- **01M558EY88CACNZ55FWMRT8RP7** Each look of the rollout waits at most
  5 seconds for the status of the daemon of its machine. A daemon that
  gives no answer in that time runs no wave for that look. The look
  goes on: the idle workers in tmux still get their requests, and the
  lead gets the note with the reason (01M5515CA4CZ6KV4ZD6Y0744Z7).
- **01M558EYAJFZK7Y822NVWGBS9H** Before the ask of the wave, the
  rollout and `riff wave run` ask the daemon to have the session image
  that the repository pins: podman runs, and the image is on the
  machine, or the daemon pulls it. When the daemon cannot, it gets no
  ask of the wave, and the lead gets the note with the reason. So a
  machine that cannot start a container runs no wave, and sends no
  message of a failed start.
- **01M558EYCXPFW8CJV3XJ7HK3NR** When riff-server refuses the claim of
  a job of a wave, the daemon ends the new worker session of the job at
  once. A refusal for a pause does not count as one of the 3 starts of
  the item (01M4ZV3B6PSRH40EYQEVYH2MBA), and the lead gets no message
  for it. Each other refusal counts, and the lead gets the message of a
  failed start.
- **01M57A7A1W2K9440WZA1QZ3M9T** Before each start of a wave, the
  daemon checks the facts of the host that no item changes: the
  sign-in of `gh`, and the sign-in of `claude` with 60 minutes or more
  left (01M4Z81B3A324KKE8M8Z71TM4M). When a fact fails, the wave starts
  no item, and no start of an item counts. It waits, as it waits for
  room, and looks again. The lead gets one message when the wave starts
  to wait, with the step for the person. `riff daemon status` shows
  the wave as waiting, with the reason.
- **01M57A7A48A70Y7PTD95A3MBYC** `riff wave run` on a wave that the
  daemon runs clears the starts of each item of the wave
  (01M4ZV3B6PSRH40EYQEVYH2MBA). A spent item then starts again.
- **01M57FJ3WWMQSDBFRRHXN3SMFQ** The wave of the daemon starts a build
  session for the item of an open pull request with auto-merge on and
  a conflict with `main`, also after its verify passed. A pull request
  whose verify failed is free work as a build already. The start text
  tells the session to rebase the branch on `main`, resolve the
  conflict, run the checks, and push to the same pull request. Such an
  item comes after the fix items of a broken `main`
  (01M51QEYS538XA58KDRY844T52), before each other item. The wave
  starts no verify of a head commit with a conflict. Each head commit
  with a conflict starts one session in a run of the wave. A start that
  riff-server refused for a pause, or a fault of the host, does not
  count. `riff wave run` clears the starts of the rebases too
  (01M57A7A48A70Y7PTD95A3MBYC).
- **01M57FJ3Z8AFDY9G0KTAYZJESY** When the rebase session of a wave ends
  and its pull request still has the head commit with the conflict,
  the daemon sends no verify request. The lead gets a message with the
  pull request, the commit and the steps for the person.
- **01M53NGVH8A19QS3CQ79ENVVXT** A second machine of a person takes work
  in containers when its daemon runs the wave of the repository:
  `riff wave run` in a clone on that machine. The claims on riff-server
  keep two machines from one item (01M4ZV3B6M7PZRJ39XTXBZP3ZH). The
  limit of the containers of each machine is the limit of its daemon
  (01M4ZV3B6HAFTSC2K63FMC5FYA). `riff workers host` stays, for the
  workers in tmux.
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
- **01M599XR3NMJD9RAJ6ZAMW6KG5** riff 4.0 starts no worker in tmux. The
  rollout of the lead asks the daemon for the wave, and gives no
  request to a session in tmux. riff has no `riff workers`, no workers
  host, no pool of build jobs, no monitor of workers, and no clear,
  reap or stop of a worker pane. `riff top` has no row for a worker in
  tmux. The daemon runs all work in containers.
- **01M59BRGZT6J45BWKPM9XGEPEJ** riff 4.0 stops no idle worker. The
  server has no idle stop: no ask to stop in the reply to a keep-alive,
  no note to the lead for it, and no hold for an unread request. A
  message that starts with `request:` is a plain message. The settings
  of idle workers stay in the log so that an old record reads, and they
  do nothing. See #887.
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
  1.8.2.
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
  (01M4YNCEMNRCAXD37V03T4JQYR).
- **01M53NGW0DWJ075C4MDP06WAGS** riff 3.0.0 removes the riff of peers of
  Wave B (#803), with its code, its tests and its book text (#959). No
  cargo feature and no flag keeps it. Its items wait in the Backlog:
  the live checks (#846, #920, #921, #922), the fixes (#928, #943,
  #944, #946, #957) and the facts of n0 (#941). A new design brings
  them back when Mike decides.
- **01M53NGW3FR555TWP8Z8HHRVDJ** The daemon of riff 3.0.0 binds no iroh
  endpoint, contacts no service of n0, reads and makes no daemon key in
  the OS keyring, opens no riff of peers and no folder `state`, writes
  no record, and reads and releases no claim on the forge.
  `crates/riff/Cargo.toml` names no dependency that only the riff of
  peers uses, and `Cargo.lock` has no `iroh`.
- **01M53NGW6F5HP1DZ9QRGP50MGT** riff 3.0.0 has none of these, also not
  in `--help`: `riff invite USER`, `riff join`, `riff leave`,
  `riff remove USER`, `riff members` with no `--email`, the flag
  `--peers` of `riff run` and of `riff wave run`, and the flag
  `--forge` of `riff release`. The forms with `--email` of
  `riff invite`, `riff remove` and `riff members` stay, and act on
  riff-server (superseded by 01M588J2QAF288ZB3M9PBN71ZS).
  `riff daemon status`, `riff view` and `riff top` show no key, no riff
  of peers and no peer link. With no `--host`, they show no session of
  another daemon.
- **01M53NGW9FH02EM74GKY829808** The re-cut supersedes each requirement
  of Wave B (#803), and each requirement of the riff of peers in "Data
  and peers": the log of peers, the keys of the members, the claims on
  the forge and the transport. Each of them holds for riff 1.6 to 1.8,
  until #959 merges. 01M4ZV3B6T2KB09WJ59F07HXET and
  01M501P9XWXWDBSXP4QRTHTZ4P hold again in riff 3.0.0, except their
  words of peer to peer: the riff tools of a container session and of
  a lead in a container act on riff-server. 01M4ZHW2083VFTQJ772ZJ27XAC
  stays superseded (01M53NGW3FR555TWP8Z8HHRVDJ).
- **01M53NGWFBWR1WMJWC2DHVHJ3T** riff 3.0.0 does not read, change or
  delete the state of the riff of peers that riff 1.6 to 1.8 made on a
  machine: the folders `state` and `state.lost-MS` and the files
  `invites.json` and `forges.json` in the folder of the daemon, and the
  secret `daemon-key DIR` in the OS keyring. A claim ref
  `refs/riff/claims/ITEM` on the forge stays until a person deletes it.
  The book page "riff 3.0" names each of them.
- **01M53NGWN190T2MYB1EZAD36VB** The items of Wave C wait in the
  Backlog: the update with no riff-server (#881), the move (#882), the
  delete of the cloud parts (#883), the live check of the move (#884),
  the commands of a person on the daemon (#885), the removal of the
  sign-in (#886), the removal of the plugin and of the workers in tmux
  (#887), the removal of riff-server (#888), and the steps of #889 that
  end riff-server. A new design brings them back when Mike decides that
  riff-server goes. A requirement that ends with "Superseded by Wave C
  (ID)" or "Superseded in part by Wave C (ID)" holds again when the
  re-cut supersedes that ID.
- **01M53NGWJ48MMYPQNJQ0EY9T47** The book page "riff 3.0" has the plan
  of the re-cut, with a mermaid diagram of a container session with
  riff-server, what riff 3.0.0 removes, and the state that it leaves on
  a machine. It has no other text of the riff of peers. The page
  "Design: the riff of peers" holds the how-tos of the riff of peers of
  riff 1.6 to 1.8, and goes with #959. The page "Design: no cloud, a
  later plan" says that its plan is not in riff 3.0.0, and that its
  items wait in the Backlog.

### Shape

- **01M4Z60B7E6X53NBJ01R95RYGE** riff 3.0 is one binary: the CLI, a
  daemon, the MCP shim and the relay. It runs on the machine of each
  person. Cloud workers are future work. Superseded in part by the
  re-cut (01M53NGVB7SZJW7X8SYS3B0ZHG). riff-server stays a second
  binary.
- **01M4Z60B7FCREZDCTVR73WWRY1** The daemon of riff is a systemd user
  unit. It owns the state, the keys, the sessions and, later, the peer
  links. It is the server of riff 3.0. Superseded in part by the re-cut
  (01M53NGS8FGRNY12DDJA3P0GH2). riff-server stays the server of the
  riff.
- **01M4Z60B7GTV4HHF9R3TMZRS7J** riff 3.0 runs on Linux only. Each
  platform part is a trait, and its Linux implementation is behind
  `cfg(target_os = "linux")`: `Runtime` (the start, the stop, the attach
  and the events of a session) and `ServiceManager`. A session is a
  `SessionSpec` of plain data: the image, the binds, the environment and
  the limits. The OCI implementation makes a container from it. A pod
  backend can use the same spec later.
- **01M4ZBGK3A977KZW50AZFR95E9** The steps of a `Runtime` are: find an
  image, pull an image, start a `SessionSpec`, stop a session, and
  attach a terminal to a session. A start gives the input, the events,
  the errors and the end of the session. A spec holds no secret: the
  value of each variable that comes from the host goes to the start
  beside the spec.
- **01M4ZBGK5G3EFXWT61DR2NHMDD** The code that drives a session takes
  its `Runtime` as a parameter. Its tests use the fake runtime of riff,
  which starts no container. A session of the fake runtime prints the
  events that the test gives, keeps its input, and ends with the exit 0
  when its input closes. A stop ends it with no exit code.
- **01M5MRG35BYRGZYD4VQVV8ZTYX** Each command of `riff`, and
  `riff-server log`, ends with the exit 0 and prints nothing more when
  its stdout or stderr is a pipe that the reader closed, for example in
  `riff daemon status | head -1`. A panic hook does it. The process
  keeps SIGPIPE ignored, so a write to a socket or to the pipe of a
  child process still fails with an error and does not kill the process.
  `riff daemon`, `riff relay` and the server of `riff-server` keep the
  default panic hook.

### Ship

- **01M4Z60B7H67YSP22NMGT3DM7X** riff ships as one static binary, from a
  GitHub release or with `cargo install`. riff has no image of its own.
- **01M4ZPJ04D9MJHNPAV0WN1KCES** The static `riff` and `riff-server`
  are built for `x86_64-unknown-linux-musl` in the image
  `rust:CHANNEL-alpine`, where CHANNEL is the channel of
  `rust-toolchain.toml`. `just static` builds them with podman, else
  docker, into `target/static`. The ELF of each has no interpreter.
  Superseded in part by Wave C (01M51R8GFNC8Y3PY290CPZ2SY0).
- **01M4ZPJ06QYVA23N9SQ45T8WQC** The CI job "Static riff" builds the
  static binaries of each pull request and each push. It fails when an
  ELF has an interpreter, or when `riff --version` does not run in the
  session image that `riff.toml` pins, with `riff` bound at
  `/opt/riff/riff` with no write.
- **01M51117NDKF11FNZ915TTQED4** `just session-test` runs the tests of
  `crates/riff` in a container of the session image that `riff.toml`
  pins. The container has `--init`, the user of the host with `USER`
  and `LOGNAME` and an entry in its passwd and group files, a home of
  its own and the limits of a session (`--cpus 4`, `--memory 8g`), as a
  session has. In CI, the test parts of the
  Gate run there (01M515Z3Y7GSFNTJGXJD727MQK). Superseded in part by
  01M56Q4AVRM8F7Y585TSGD53K3.
- **01M56Q4AVRM8F7Y585TSGD53K3** `just session-test` keeps `--cpus 4`
  and `--memory 8g`: a runner of the Gate has 4 CPUs. These are not
  the caps of a session of the host (01M56Q4AQ86V99RC8CA1KGCMSZ).
- **01M515Z3Y7GSFNTJGXJD727MQK** The jobs `Tests I/N` of the Gate run
  `deploy/session-test.sh --part I/N`: part I of N of the tests of the
  workspace, in the container of 01M51117NDKF11FNZ915TTQED4, with
  docker and a cache of `target/session`. The key of the cache is the
  hash of `riff.toml`, `rust-toolchain.toml` and `Cargo.lock`. No other
  job runs the tests in the session image.
- **01M511FGMFF17G2G8RV6ST9070** Before the tests, `just cargo-test`
  builds each binary of the workspace. So a test of one crate finds the
  binary of another crate, for example `riff-server` in a test of
  `riff`, also in a new clone with no `target/`.
- **01M513W2V4G94R7F4D4Q3M2RA1** For a pull request and a push to a
  branch, the CI job "Static riff" builds the profile `dev`, with a
  cache of `target/static` and of the downloaded crates. The key of the
  cache is the hash of `rust-toolchain.toml` and `Cargo.lock`. For a
  tag, it builds the profile `release` with no cache. `build.sh` keeps
  the downloaded crates in the volume `riff-static-cargo`, or in the
  volume or the absolute folder that `RIFF_STATIC_CARGO` names.
- **01M4ZPJ091W3NYV1V0FJS9K6MS** For a release tag, the CI job "Static
  riff release" adds `riff-x86_64-unknown-linux-musl`,
  `riff-server-x86_64-unknown-linux-musl` and `SHA256SUMS` to the
  GitHub release of the tag. It waits up to 60 minutes for the release.
  The job "Deploy" of the tag runs only after it passes. Superseded in
  part by Wave C (01M51R8GFNC8Y3PY290CPZ2SY0).
- **01M4ZPJ0BAQZ6AN06FGXS5D4FP** On Linux x86-64, `riff update`
  downloads `riff` and `riff-server` from the GitHub release of the tag.
  It checks the SHA-256 of each file against `SHA256SUMS` of the
  release, and that each ELF is for this CPU and has no interpreter.
  When a check fails, it installs nothing. Else it renames each file
  over the old one in the bin folder of cargo, so a process that runs
  keeps its old file. Superseded in part by Wave C
  (01M51R8GFNC8Y3PY290CPZ2SY0).
- **01M4ZPJ0DJ9FTYNPTBBVJCTB37** When the release has no `SHA256SUMS`,
  or no line for the binaries of this machine, `riff update` builds the
  release with `cargo install`, and says so.
- **01M510NWBNC20GXFRMHPW12TW9** With `update.auto = true`, when the
  `riff` of the bin folder of cargo gets a reply from a `riff-server` of
  its own build, and that `riff` or the `riff-server` beside it is not
  static, it starts `riff update --tag vX.Y.Z` of its own release in the
  background. So `riff update` puts the static files of the release in
  place. This fix gets one try for each release, apart from the try of
  the update to that release. A riff that runs from another folder
  starts no fix.
- **01M4ZPJ0FXTC78Z8GG2QA65167** `just real-session` runs the static
  riff of the tree, so its daemon can run `riff relay` in the session
  image.
- **01M4Z60B7JF80TMP6FHQ04MHCZ** The session image has a release train
  of its own. Each repository names its session image, pinned by digest,
  in its riff config. The first base is the Dockerfile of #707. An image
  has `claude`, `git`, `gh` at `/usr/bin/gh`, CA certificates, a POSIX
  shell and the toolchain of the repository. It holds no riff.
- **01M4Z7SP3KSKJYKNRP1MQ70C4Q** The riff config of a repository is the
  file `riff.toml` at its root. The key `image` of its table `[session]`
  names the session image as `HOST/PATH@sha256:DIGEST`. riff refuses a
  tag, also beside a digest, and a name with no registry host.
- **01M4Z7SP5TGJ3DMWFGE5SN68N1** The session image of this repository is
  built from the folder `deploy/session/`. Its Dockerfile installs the
  Rust channel and the components of `rust-toolchain.toml`. A test
  checks that they agree.
- **01M4Z7SP7Y19BQ8QTV1T6AE8SR** The workflow "Session image" runs on
  each change of `deploy/session/`, of `riff.toml` or of the workflow,
  in a pull request and on `main`. It tags the image `tree-ID`, where ID
  is the git tree of `deploy/session/`, and pushes it to
  `ghcr.io/como-technologies/riff-session`. When the registry has that
  tag, it builds nothing. The summary of the job names the digest.
- **01M4Z7SP9YZ5JCHA8YD8R5A4Y9** The workflow "Session image" fails when
  `riff.toml` does not pin the digest of the image of `deploy/session/`
  of its commit. So a change of the Dockerfile and the change of its
  pin go in one pull request.
- **01M4Z7SPC20RWG218EWPEGYZTF** `riff image` prints the session image
  that the riff config of the repository of the current folder pins.
- **01M51STPHXSSZMHF9T7Z85097R** The merged pin of a main clone is the
  session image that `riff.toml` pins on the default branch of
  `origin`, the branch of `origin/HEAD`. riff fetches that branch
  first. The wave of the daemon, `riff wave run`, `riff run`,
  `riff lead start` and the image step of an update use only the
  merged pin. So an edit of `riff.toml` that is not merged never
  chooses the image that riff pulls and runs.
- **01M51STPMEM62Z8B26YM2CH6QH** When `riff.toml` in the working tree of
  the main clone pins another image, or no good image, riff says so in
  one line, and uses the merged pin.
- **01M51STPPWYFKQ1ESCNA6V3WV2** When the fetch of the default branch
  fails, riff says so in one line, and reads the branch that the clone
  has. A main clone with no `origin/HEAD` has no merged pin: riff stops,
  and gives the fix `git remote set-head origin --auto`.
- **01M51SVXQK037E20R7WAY66532** `riff image`, `riff init` and the image
  of this folder in `riff update --check` read `riff.toml` of the
  working tree. So a person can try the pin of a branch before its
  merge.
- **01M4Z7SPE5Q02RQWW5VZBEK2G3** The package `riff-session` on ghcr.io
  is public, as the repository is. So podman pulls the image with no
  sign-in.
- **01M4Z60B7KEV0H161RBZ1SDRY2** riff binds its own binary into each
  session with no write, and runs it as the entrypoint: `riff relay`. So
  the riff code in a session is always the riff code of the host.
- **01M4Z60B7K13MSYM8ZY20WN6NV** The first run of `riff` checks for
  podman, installs the user unit of the daemon and pulls the image of
  the repository.
- **01M4Z60B7MW963MBQAPN2CJEQZ** The first run of `riff` also checks
  rootless podman: `/etc/subuid` and `/etc/subgid` have a range for the
  person, and cgroup v2 gives `cpu` and `memory` to the user. It reads
  `user@UID.service/cgroup.controllers`, not `podman info`. It names the
  fix of each check that fails.
- **01M5QY976MTPRX5J4N1MKXAGDK** A main clone whose default branch of
  `origin` has no `riff.toml` has no merged pin: riff stops, and the
  error names `riff.toml`, the branch and the fix. The fix adds
  `riff.toml` with the key `image` in the table `[session]`, and points
  to the how-to "Add another repository to the riff". `riff lead start`
  checks the merged pin before it asks the wave and before it prints
  that the lead starts: with no merged pin, it prints no line that the
  lead starts.

### Sessions

- **01M4Z60B7NW4GHKGCYZZDG26R6** Each work item has one session. The
  session starts, does the item and ends. No session waits idle, and no
  session clears its context between items. A verify is an item too.
- **01M4Z60B7PTJMB37578K8FV22Q** Each session has its own clone:
  `git clone --reference MAIN` into a folder of riff, bound into the
  container with write. The session reads the objects of the main clone
  through the alternates, and pushes from its own clone. No session
  writes the git dir of the main clone or of another session. The clone
  of a session goes when its item ends, after its push.
- **01M4Z60B7PQ07HRJBXS1TA73PX** The container sees the objects of the
  main clone (`MAIN/.git/objects`) at the same path, with no write. So
  git reads the alternates of the clone of the session.
- **01M4Z60B7QDWEFQ96H448RE9N2** Each session is a rootless podman
  container (`podman run --rm --userns=keep-id`) from the image of its
  repository. riff binds with write only: the clone of the session, its
  Claude folder, the folder of its socket, and the cache volumes
  (01M4Z60B7RPSQ1ABGVMJSZWS5P). riff binds `~/.gitconfig` and
  `~/.config/gh` with no write, or as an overlay (`:O`) whose changes go
  when the session ends. Each other write goes to the copy-on-write
  layer of the container, and goes when the session ends.
- **01M4Z60B7RPSQ1ABGVMJSZWS5P** The cache volumes are named volumes:
  one for all of `CARGO_HOME`, and one for the folder of sccache. The
  tools lock them themselves. The toolchain is in the image, with no
  write, so riff makes no volume for rustup. The sccache server of a
  session has `SCCACHE_IDLE_TIMEOUT=0`.
- **01M4Z60B7SHAMXXFK15BE3KZSP** The home of the person is a tmpfs in
  the container, at the same path, with `HOME` set:
  `--mount type=tmpfs,destination=$HOME,tmpfs-mode=0700,U=true`. The
  binds of `~/.gitconfig` and of the Claude folder go on top of it. Each
  other write in the home goes when the session ends.
- **01M4Z60B7S8V9KKZPNBCQ6M99N** The `SessionSpec` sets `USER` and
  `LOGNAME` from the host, as a login shell does.
- **01M4Z60B7TQV799Z4J5KAW2XMZ** Each container has its own `/tmp`, its
  own processes and its own loopback. So a temp file, a `pkill` or a
  test port of one session does not hit another. A session reaches the
  internet, as a plain `claude` does: no proxy and no filter.
- **01M4Z60B7VCTG76S8N1CBWWG6D** riff adds no bubblewrap, no Landlock
  and no seccomp filter to a session. The defaults of podman stay.
- **01M4Z60B7WXBRPYW38XW9NVC17** A session runs its builds and its tests
  in its own container, with its own rights. riff has no broker, and no
  separate sandbox for a test run.
- **01M4Z60B7W6F3CC5XJE0VS7Y86** The relay talks to the daemon through
  one Unix socket, in a folder that only that session sees. The socket
  tells the daemon which session asks. A local session needs no session
  grant, no session key and no DPoP token.
- **01M4Z60B7XT7462XMV49GJTVSJ** Each container gets `--memory` and
  `--cpus`. The defaults are `--memory 8g` and `--cpus 4`. `podman stop`
  ends a session and each of its processes. Superseded in part by
  01M56Q4AQ86V99RC8CA1KGCMSZ.
- **01M56Q4AQ86V99RC8CA1KGCMSZ** The caps of a container are settings
  of the host, not constants. Each new container gets `--cpus N`,
  `CARGO_BUILD_JOBS=N` and `--memory GBg`. The defaults are 8 CPUs and
  8 GB. The daemon puts the caps in the spec when it starts the
  session. A session that runs keeps its caps.
- **01M51117K3WXK35CE67KPRFMA8** Each container of a session starts
  with `podman run --init`. The init of podman (catatonit) is PID 1. It
  reaps each orphan process of the session, and it passes each signal
  to the command of the session. The init must be on the host.
- **01M4Z60B7Y4XD6TJCYEYJT2R4S** The daemon keeps one limit for the
  host, in addition to the limits of each container: the number of live
  sessions, and a floor of free memory. It starts no session while the
  free memory is below the floor. The defaults are at most 3 live
  sessions and a floor of 8 GB.
- **01M4Z60B7Z24WBD6HV6ER0WN9K** A session lives through a stop, a
  restart or an update of the daemon. conmon, the monitor of podman, is
  the parent of each container, not riff. So the container runs on when
  the daemon stops. `riff relay` starts `claude` and holds its stdin and
  stdout. While the daemon is away, the relay keeps `claude` running and
  keeps its events. When the daemon comes back, the relay connects again
  and sends the kept events by sequence number. `claude` does not see
  the break.
- **01M4Z60B800BH5BHX0H9JVDG6A** An update of riff reaches new sessions
  only: the bind keeps the old binary in a session that runs. So the
  protocol between the relay and the daemon stays compatible across one
  version. The recovery of the sessions after a reboot or a crash of the
  host is future work.
- **01M4Z60B81RTZT71PN5HV0GZHX** The lead runs in a container with a
  terminal (`podman run -it`). conmon holds its PTY. The ratatui view of
  riff shows the sessions, and attaches to the lead or to a takeover.
  riff 3.0 uses no tmux.
- **01M59BXNPR3AZ0HAS9GQCZ4PH2** riff has no plugin, no hooks and no
  commands for a session on the host: `riff connect`, `riff enable`,
  `riff disable`, `riff watch`, `riff hook`, `riff statusline` and
  `riff mcp` do not exist. A session runs in a container, and gets its
  tools from `riff relay mcp`.

### The riff daemon

- **01M4ZDTVH57VE77YT20G65A4BQ** `riff daemon` runs the daemon in the
  foreground. The systemd user unit runs the same command. At a SIGTERM
  or a SIGINT, the daemon stops, and stops no session.
- **01M4ZDTVK74596M0ZA46N51NKV** `riff daemon install` writes the unit
  `riff.service` in the folder of the user units of systemd
  (`XDG_CONFIG_HOME/systemd/user`, else `~/.config/systemd/user`). The
  unit runs `riff daemon` with the absolute path of this riff. It gets
  the `PATH` of the shell that installs it, and `XDG_DATA_HOME` when it
  is set. Then riff runs `systemctl --user daemon-reload`, `enable` and
  `restart`, and waits up to 10 seconds for the daemon to answer. The
  unit file and the link of its `enable` are the only files of riff in
  the home outside its own folders. With `RIFF_HOME` set, riff installs
  nothing.
- **01M501M0T0JBWGMMHCT7HT95T9** `riff daemon uninstall` runs
  `systemctl --user disable --now riff.service`, deletes the unit file
  `riff.service`, and runs `systemctl --user daemon-reload`. It deletes
  only a unit file whose first line riff wrote. It keeps the folder of
  the daemon and each container. With no unit file, it says that no
  daemon is installed, and exits with 0. With `RIFF_HOME` set, riff
  removes nothing.
- **01M4ZDTVNBKDM64043GGWQEV4S** `riff daemon status` shows the process
  ID, the build and the age of the daemon, its folder, and each session
  that it owns. When no daemon answers, it shows the state of the unit
  `riff.service`, and exits with 1.
- **01M4ZDTVQEHD87WD5TSN93XFSS** The folder of the daemon is `daemon` in
  the data folder of riff, with mode 0700. It holds the control socket
  `riff.sock`, and a folder `sessions/NAME` for the socket of each
  session, with mode 0700. The daemon starts no session whose spec binds
  the folder of the daemon, a folder above it or a path in it, or binds
  a path at `/run/riff`.
- **01M53YP97S9BDMFBN0X5G444MY** With `RIFF_DATA=DIR`, the data folder
  of riff is `DIR`: the folder of the daemon is `DIR/daemon`, and the
  session folders are in `DIR/sessions`. `RIFF_DATA` wins over
  `RIFF_HOME` for the data folder. The settings, the local files and the
  secrets stay where they are. Only the real-session test sets it.
- **01M53YP9AHPZ9YPNQFJ1SYZGWF** A daemon with `RIFF_DATA` or
  `RIFF_HOME` set is never the daemon of the unit: it changes no unit
  at its start, and it does not start again for a new riff. With either
  variable set, `riff daemon install` installs nothing,
  `riff daemon uninstall` removes nothing, and `riff update` does no
  step for the daemon.
- **01M4ZDTVSP0T1YENKAWGKA114D** The daemon binds the folder of the
  socket of a session at `/run/riff` in its container, with write, and
  sets `RIFF_SOCKET=/run/riff/riff.sock`. Each session has its own
  listener. So the listener tells the daemon which session asks: a
  hello on the socket gets the session ID, the item and the name of the
  container. The folder goes at the end of the session.
- **01M4ZDTVVV56X964S28GS6K2Z2** `riff` and the daemon send JSON, one
  message on each line. The first line of each connection has the
  version of the protocol and one ask. The daemon refuses another
  version, and says to start it again with `riff daemon install`. An
  error of the daemon shows no part of a line that it cannot read.
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
- **01M4ZKX0NXN7E33VC3PSFGSPDT** `riff run ITEM` does one work item in
  one rootless podman container, through the daemon. It asks the daemon
  for the image (the daemon pulls it when it must), then for the
  session. The host does each riff step with riff 1.x: it checks the
  facts, starts a worker session, claims the item, makes the clone,
  sends the verify request and releases the item. In the container,
  `riff relay` starts `claude` (01M4ZHSZZDH0C01A5EVD0KFY14). The daemon
  writes `events.jsonl`, and sends each event, with no secret, to
  `riff run`. With no daemon, `riff run` stops before the claim, and
  says how to start the daemon. Ctrl-C asks the daemon to stop the
  container. When `riff run` ends with no stop, the session goes on in
  the daemon. Superseded in part by 01M519Z4RWHWP3TKNP4T5GKKE9 for
  `riff run --peers` (01M526Z1Q3QM3X0C7N6MCX8Z6T).
- **01M4ZDTW00B7N6T9P3MK066KA7** A stop or a restart of the daemon stops
  no container. The daemon starts each `podman` with no
  `INVOCATION_ID`, so podman puts conmon in a scope of its own, not in
  the cgroup of the unit of the daemon. `podman run` has
  `--sig-proxy=false`, so a signal to it does not go to the container.
  The daemon stops a container only when `riff run` asks, or at the
  quiet limit (01M4ZSZX77AJDGWNQMGBPV2QXG).
- **01M517BTDRKDXD3X84P6AEAF7P** `riff update` reads the unit
  `riff.service` that riff wrote. When its `ExecStart` runs another
  riff than the `riff` on `PATH`, riff writes the unit again with that
  riff and the same variables, and runs `systemctl --user
  daemon-reload`. With no unit file, or with `RIFF_HOME` set, it
  changes nothing. A failed step fails no update.
- **01M517BTJ9EV4ZGPYRB6HXJJC9** The daemon runs an old riff when its
  binary was replaced, or when its unit names another riff. Then
  `riff update` runs `systemctl --user --no-block try-restart
  riff.service` only when the daemon owns no session, no start waits
  for room, and no job of a wave runs. Else it says that the daemon
  starts again after its last session. A daemon that does not answer
  stays as it is.
- **01M517BTP6BAGTRBYWPX9GPVE0** The daemon that the unit runs looks
  for an old riff at the end of each session and of each job, and each
  minute. When it runs an old riff and owns no session, no start
  waits and no job runs, it asks systemd one time to start the unit
  again. A failed ask is one line in its log.
- **01M53NEE2AVNHA8RD8N40FPH03** The new start of the unit stops the
  `systemctl` child of the daemon that asks for it. When that child
  ends by a signal with no text on stderr, the new start runs: the
  daemon logs no failure and no `riff daemon install` step. An exit
  code that is not 0, or a signal with text on stderr, is a failed ask,
  and its line names the error.
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
- **01M4ZHT01HH88KPA8628M7VBEY** The daemon keeps a record of each live
  session in `live/NAME.json` in its folder, with no secret. At its
  start, the daemon takes up each session whose container runs, or when
  podman cannot say: it binds the socket of the session again in the
  same folder. It removes the record of each other session.
- **01M4ZHT03MAH0B5Q4S6GZC5JRP** A session ends when the daemon has the
  exit of its relay, or when its container ends.
- **01M4ZSZX77AJDGWNQMGBPV2QXG** After the daemon closed the input of
  `claude`, it stops the container of the session when no event comes
  for the quiet limit: 30 minutes. Each event starts the quiet limit
  again. The daemon tells each `riff` that follows the session why it
  stops the container, and writes it to its own output. The quiet
  limit is longer than the longest check of a session.
- **01M4ZHT05QHQRQ2KGVJFB9F7HD** On the control socket, `riff` can
  follow a live session: the daemon sends each event after a number,
  then each new event, then the end.
- **01M4ZKX0KMS58NR2E9AF8229A8** `riff daemon events NAME` shows each
  event of the live session in the container `NAME` from its start, as
  `riff run` shows them, and each new event until the end. Ctrl-C ends
  the view. The session goes on.
- **01M579Z9JE6HY9HG3PWR1RR0RQ** The daemon of a host has a session of
  its own on riff-server for each repository that it serves. It serves
  a repository after `riff daemon host` or `riff wave run` in its main
  clone. It keeps each one in `served.json` in its folder, with its
  session ID, and serves with the same session after its own restart.
  Its status is `daemon host: pid P, build B, N sessions`.
- **01M579Z9MPS86MX7890T6HV18S** `riff daemon status`, `riff daemon
  limit`, `riff daemon cache`, `riff daemon events`, `riff daemon
  sweep`, `riff daemon end`, `riff daemon restart`, `riff wave run` and
  `riff wave stop` take `--host HOST`. With it, riff posts the request
  as a direct message to the session of the daemon of HOST. The daemon
  does it as the command does on HOST, and posts the reply as a direct
  note to the sender. riff prints the reply, and exits with 1 when the
  daemon refused or failed.
- **01M57D4FTP2NXAE18XFRXCJ79H** Each request to the daemon of another
  host and each reply go in the direct thread of the sender and the
  daemon. No other session and no other user reads them. A person in a
  terminal gets the reply in the direct thread of the person.
- **01M57D4FX5VHQR150QTDS6BA1C** `riff daemon events NAME --host HOST`
  shows the same lines as `riff daemon events NAME` on HOST: each event
  from the start of the session, then each new event, then the end.
  riff asks for one page after the other: each page has the lines of
  the events after the last event of the page before, and the lines of
  the daemon after the last line of the page before, at most 200
  lines. With no new event, the daemon holds a request for at most 20
  seconds. A session that ends between two pages ends the follow with
  its end, as on HOST. riff exits with 1 when the session failed.
- **01M57GCZVJAHAMPEYY1T6GA48P** After the end of a session, the daemon
  keeps its end for 5 minutes: the events, the lines of the daemon
  about the session, and how it ended. A follow of the session in that
  time gets the events after its number, the lines and the end, as a
  follow that stayed.
- **01M57GCZXZZRPGGWC5HZS49DGM** The daemon keeps each line that it
  says about a session, for example why it stops the container. A new
  follow gets each line in its place among the events. `riff daemon
  events --host` shows each line one time.
- **01M57D4G1TS8BHJF5A8PW2D79C** The events of a session go over the
  wire only from the events that the daemon keeps: the daemon took each
  secret out of each event before it kept it. So no secret leaves the
  host.
- **01M57D4FZGCVE62AP4MVG59HM4** `riff view --host HOST` shows the view
  of `riff view` for the daemon of HOST, through riff-server. riff asks
  the daemon of HOST for its status each 3 seconds. Enter and `t` work
  only on HOST: in the view of another host they show a note.
- **01M579Z9PZZCVWQKG7V8WZ5KRB** The daemon of a host acts only on a
  verified request from its person, or from the lead of its person in
  its repository. It replies to each other request with a refusal, and
  changes nothing: another user, and a session that is not the lead,
  for example a worker.
- **01M588QXAGD3B8BTK7N17M29WM** The refusal of the daemon of a host
  names its cause: the user of the request when it is another user,
  else the session of the request, which is not the lead of its user in
  the repository of the daemon. The help of each `--host` flag that
  asks the daemon or the workers host of another host says who can use
  the flag, with the same words as the book.
- **01M579Z9SAN24QGN1VV5C7EHM9** `riff daemon restart` starts the
  daemon again on the new riff on disk only when it runs an old riff
  and no session, no start waits and no job of a wave runs. Else it
  says why not. It asks systemd 5 seconds after the reply. `riff
  daemon status` shows the new pid and build.
- **01M579Z9VRD12FRK6DSYVKDHQB** `riff daemon end NAME` ends one
  session of the daemon: the daemon frees its item on riff-server at
  once, ends its session there, then stops its container. It refuses
  the lead.
- **01M57D3F50347RD8ED79KEA9V7** Each 30 seconds, each session of the
  daemon of a host sends the facts of the host to riff-server, as the
  numbers of its machine: `daemon facts: ` and one line of JSON, at
  most 4000 characters. The facts are the riff build, the limit and
  the floor, the live sessions with their item, phase and last step,
  the starts that wait, the state of each wave, the load, the free
  memory, the disk, the version of podman, the session image of the
  repository, and the sign-ins of riff, `gh` and `claude`. Facts older
  than 180 seconds count as none.
- **01M57D3F7FXTPZB3M72Z1DQ1T7** `riff who` ends with the part `Hosts`:
  the facts of the daemon of each host, with their age.
- **01M5923PW9YW5S58RQG5Y532RS** The `who` tool of a session ends with
  the same part `Hosts` as `riff who`, in plain text.

- **01M57D3F9SWMKP22WNRPRY3X5T** `riff top` shows the facts of the
  daemon of a host under the row of that host, with the container
  sessions of that daemon. A host with facts gets no `no numbers`.
- **01M57D3FC5PB5EQ6Y38Y5R2DBB** When the sign-in of `claude` on a host
  has less than 2 hours left, or is missing, the daemon posts one note
  to the lead with the step for the person. It posts no new note at
  each look. When the sign-in has 2 hours or more again, it posts one
  note.
- **01M57D3FEJPHD962GW0H82N8E6** When a sweep of the daemon keeps a
  folder whose work is over, because its clone holds work that is not
  on `origin`, the daemon posts one note to the lead of its repository.
  It posts a new note only when the reason changes. Superseded in part
  by 01M5NVKNS35SC8P514GYDJSRQM.
- **01M57D3FGV57W96A7SWTEFRCGW** The facts give the minutes left of the
  sign-in of `claude`. The sign-ins of riff and `gh` refresh or have no
  end: the facts say `on` or `off` for each, or `?` when riff cannot
  read it.
- **01M588HSV1APXX4TQ1J1VRZCBF** The facts of a daemon show the
  1-minute load of the host against its hardware threads, the logical
  CPUs, with the word `threads`: `load 1.5/32 threads`. The load has
  the warning style when it is more than the threads. The line of
  numbers of `riff top` shows the same part for a host that tells no
  numbers of the monitor. The word `cores` names only the physical
  cores.
- **01M588HSXQ2KWX0V8RYM65CPFR** The line of numbers of `riff top`
  shows the 5-minute load against the limit of the monitor:
  `load5 9.8 of 12`. It has the warning style when the load is over
  the limit. This supersedes the load part of
  01M421QPZ9E01PQ62PDBH378SJ.
  Superseded by riff 4.0 (01M599XR3NMJD9RAJ6ZAMW6KG5).
- **01M5MS2T8VY8NX4VBMQTCKWDKH** A riff command that asks the riff
  daemon of this machine tries again for 5 s when no daemon answers
  while systemd says that riff.service is active or activating: the
  daemon listens a short time after its start. With no unit that runs,
  it fails at once.
- **01M5MS1Q78AFEY3ZVXNAQXM1FJ** A start of a wave that waits for room
  does not keep the daemon on an old riff: the wave is in `waves.json`,
  and the new daemon waits for room for it again, also with the limit 0.
  A start of `riff run` that waits for room keeps the daemon on the old
  riff, as a session and a job of a wave do.
- **01M5NVKNGZPK0GG7B7ZWBS0DFE** When the sweep of
  01M53NJ5V27BCHDNHK2THQQZZQ finds a folder whose work is over, and its
  clone holds work that is not on origin, it pushes that work to the
  branch riff-kept/ITEM-ID8 of origin and then removes the folder. It
  removes a folder with files of the container with podman unshare. The
  push has the commits of HEAD that origin has not, and the changes of
  the tracked files (git add -u). It never has a file that git does not
  track. A clone with only such files pushes nothing. When the push
  fails, or a commit is on a local branch that is not HEAD, or git
  cannot read the clone, the folder stays and riff daemon sweep says
  why. This supersedes 01M53S7WAXD46YPN0DR96XSTMG in part.
- **01M5NVKNKN3PTCWB45BV03M1TW** The sweep never pushes or removes the
  folder of a session that runs, of a job of the daemon, or of an item
  whose pull request is open or whose issue is open with no pull
  request. The folder of an open item stays.
- **01M5NVKNPD938ASWY4TD5BGK67** The sweep deletes each branch
  riff-kept/* of origin whose tip commit is older than 30 days, for each
  repository of the folders that it looked at.
- **01M5NVKNS35SC8P514GYDJSRQM** After a sweep that pushed work, removed
  a folder with files that git does not track, or deleted a riff-kept
  branch, the daemon posts one digest note to the lead of the
  repository. It posts no note at each start for a folder that it
  already pushed and removed. This supersedes 01M57D3FEJPHD962GW0H82N8E6
  for folders whose work the sweep can push.
- **01M5NVKNVTVR722EA148V5127M** The book says where the kept work goes,
  riff-kept/ITEM-ID8 of origin, and how to get it back with git fetch
  origin riff-kept/ITEM-ID8.
- **01M5P1XBJ6ZX3Q8ZVHR1EJFMFF** The lead does not keep the daemon of
  its host on an old riff. The daemon counts no lead (a session with a
  terminal) as work: with only the lead, `riff update` and the look of
  the daemon start the daemon again on the new riff
  (01M517BTJ9EV4ZGPYRB6HXJJC9). The new daemon takes the lead up with
  its container, its terminal and its socket
  (01M4ZDTW00B7N6T9P3MK066KA7), so `riff release check` passes "each
  host takes the build" while the lead runs.

- **01M5P2MG28EE75PPGYAWHTY2S1** The daemon starts the lead, each work
  item and each verify with claude --effort LEVEL. The effort of the
  three roles is xhigh by default. The limit of the host holds the three
  levels in limit.json, and riff daemon limit sets them with --effort
  for work items, --effort-lead and --effort-verify, also with --host.
  riff refuses a level that Claude Code does not know: low, medium,
  high, xhigh and max. A session that runs keeps its effort.
- **01M5QVCAXF5ANTTT46XHR6SF7H** When the sweep of
  01M5NVKNGZPK0GG7B7ZWBS0DFE finds a folder whose clone has commits on a
  local branch that is not HEAD, and no branch of origin has them, it
  pushes each such branch to the branch riff-kept/ITEM-ID8-BRANCH of
  origin, with the same rule as for HEAD: the commits and the tracked
  files, never a file that git does not track. Then it removes the
  folder. A commit on a local branch that is not HEAD no longer keeps
  the folder. The daemon tells the lead one time, in the digest note of
  01M5NVKNS35SC8P514GYDJSRQM, with each branch that it pushed. It posts
  no note at the next sweep. This supersedes 01M5NVKNGZPK0GG7B7ZWBS0DFE
  in part.

### riff relay

- **01M4ZHSZZDH0C01A5EVD0KFY14** The daemon binds its own binary at
  `/opt/riff/riff` in each container, with no write. The command of a
  session is `/opt/riff/riff relay -- COMMAND`. The daemon starts no
  session whose spec binds a path at `/opt/riff/riff`.
- **01M510NW8YEVMQKD6RWJPW38N3** Before each start of a session, the
  daemon reads the ELF of the binary that it binds at `/opt/riff/riff`.
  When the ELF has an interpreter, the daemon starts no container, and
  its error names `riff update`. It reads the file at each start, so
  after `riff update` the next start works with no restart.
- **01M4ZHSZMH4Y8CB41FYGE3C3R2** The relay and the daemon count the
  events and the inputs. Each event has a sequence number: the event N
  is line N of `events.jsonl`. At each connection, the relay says how
  many inputs it gave `claude`, and the daemon says how many events it
  has. The relay sends each event that the daemon does not have. The
  daemon sends each input that the relay did not give. So no event and
  no input goes two times, and none is lost. The daemon acks no event
  that it cannot keep, and acks the exit of `claude` only when it has
  each event before it. In place of that ack, it closes the connection,
  and the relay sends the line again at the next connection.
- **01M4ZHSZPRYK42C20ETXM3HJZ2** On the socket of a session, the daemon
  takes a hello of its own version of the protocol, of the version
  before it, and of the version after it. A change of the protocol adds
  a line or a field. Each side skips a line that it does not know.
- **01M4ZHSZRY0P9HHNVSFET8HEEN** The relay takes each secret of the
  session out of each event and out of each line of the stderr of
  `claude`, before it sends or shows the line: the values of
  `GH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN` and `ANTHROPIC_API_KEY`, and the
  tokens of the sign-in file of `claude`. The relay writes no file.
- **01M4ZHSZV46RF3XDXDTQXWGXRG** The relay gives `claude` the riff
  tools: an MCP server `riff` that runs `riff relay mcp`. It passes each
  line between `claude` and the socket of the session. The daemon
  serves the tools as that session.
- **01M4ZKX0H9N82ZBC9PKSPTQV4N** Until #719, the daemon serves the riff
  tools of a session on the riff-server of `riff run`, as the worker
  session that `riff run` started. That session holds the claim of the
  item. The daemon uses the sign-in of the person on its machine. A
  session gets only the tools that act on riff-server, and on no folder
  of the host and no claim: `whoami`, `who`, `threads`, `read`, `post`,
  `tell`, `status` and `blocked`. The daemon refuses each other riff
  tool with a text that says why. Superseded by
  01M4ZV3B6T2KB09WJ59F07HXET.
- **01M4ZV3B6T2KB09WJ59F07HXET** The daemon serves the riff tools of
  each session that it owns, of `riff run` or of a wave, on the
  riff-server of the session, as the worker session that holds the
  claim of the item. The daemon uses the sign-in of the person on its
  machine. A session gets only the tools that act on riff-server, and
  on no folder of the host and no claim: `whoami`, `who`, `threads`,
  `read`, `post`, `tell`, `status` and `blocked`. The daemon refuses
  each other riff tool with a text that says why. The state of the
  daemon gets the tools with peer to peer. Superseded by Wave B
  (01M519Z3FTAZ20DQKDTW09M6NS).
- **01M4ZHSZXAY7F7E17B3CMHG562** When the daemon goes away,
  `riff relay mcp` answers each call in flight with an error, and keeps
  each new line until the daemon comes back. At the new connection, it
  gives the daemon the initialize request of `claude`, so the daemon
  skips the handshake.
- **01M4ZKX0R850RPSS63B001NNJZ** After the end of `claude`, the relay
  waits up to 10 minutes for the daemon to take the last events. Then
  it ends with the exit code of `claude`, and the container goes.
- **01M4ZKX0F25S284R0N7KZVTHNA** When the daemon goes away during a
  session, `riff run` says so, and waits up to 10 minutes for a new
  daemon. Then it follows the session again after the last event that
  it showed. So it shows each event one time. When no daemon comes
  back, `riff run` says so, finds the pull request, and releases the
  item.

### The waves of the daemon

- **01M4ZV3B6G5VB89SRSWCSBZ630** Each start of a session waits for room
  in the limit of the host (01M4Z60B7Y4XD6TJCYEYJT2R4S), in the order of
  the asks: a start of `riff run` and a start of a wave wait in the same
  line. A start that has room counts as a live session until the daemon
  owns its session. The free memory is `MemAvailable` of
  `/proc/meminfo`. When the host does not say, only the sessions count.
- **01M4ZV3B6VS0RAR0P7MRHKE8CQ** While a start of `riff run` waits for
  room, the daemon says why to `riff run`. When `riff run` stops it, or
  leaves, the daemon starts nothing.
- **01M4ZV3B6HAFTSC2K63FMC5FYA** `riff daemon limit` shows the limit of
  the host. `--sessions N` and `--floor GB` set it in `limit.json` in
  the folder of the daemon. The daemon reads the file at each look for
  room, so a change counts at once. `riff daemon status` shows the
  limit, each start that waits for room, and each wave with what it did
  last.
- **01M56Q4ASHTDWVA2MS2CBQ2PAM** `riff daemon limit --cpus N` sets the
  CPUs and the build jobs of each new container of the host.
  `--memory GB` sets its memory cap. Both go in `limit.json`. A
  `limit.json` with no value for one of them gives the default.
  `riff daemon limit` and `riff daemon status` show both. riff refuses
  0 for each.
- **01M57818168N3AQGE3G5GDYDDD** `riff daemon cache SIZE` sets the most
  size of the compile cache of the container sessions of the host in
  `cache.json` in the folder of the daemon. The default is `40G`. The
  daemon gives the size to each new session in `SCCACHE_CACHE_SIZE`. A
  session that runs keeps the size of its start.
- **01M5781930TF6XBX7ME7RAMX2X** `riff daemon cache` shows the size of
  the volume `riff-sccache` now and the most size. `riff daemon status`
  shows the same line.
- **01M5781A4PRTTYRHH5GJ1GHN7G** `riff daemon cache --clear` deletes
  each file and folder in the volume `riff-sccache`, and keeps the
  volume. It works while sessions run. Each session has
  `SCCACHE_IGNORE_SERVER_IO_ERROR=1`, and a build compiles each file
  that the clear deleted.
- **01M588JW2KS6EVX1SE49686FQ7** `riff daemon cache` and `riff daemon
  status` show the size on the disk of each file in the volume
  `riff-sccache`, with 1GB = 1024³ bytes: the unit of the most size,
  which `sccache` reads as 1024³ bytes for each `G`. `podman system
  df -v` counts 1GB as 1000³ bytes, so its number is about 7% larger,
  for example 8.3GB for 7.7GB. When the person cannot read a folder of
  the volume (rootless podman), riff reads it with `podman unshare`.
  When riff cannot read it, it says so, and never shows 0. `riff
  daemon cache --clear` also deletes with `podman unshare` when the
  person may not, and fails with the reason when it cannot delete.
- **01M57840K7SNPS243CYMYNYVFX** `riff daemon mcp` shows the MCP
  servers of each new container session of the host. The default is
  `riff` only. `riff daemon mcp add NAME` copies the server NAME from
  the MCP config of the person into `mcp.json` in the folder of the
  daemon, readable only by the person. `riff daemon mcp remove NAME`
  takes it out. `riff` always stays. `riff daemon status` shows the
  list.
- **01M57840NN08NA4PH0YR81PDGT** `riff daemon mcp add NAME` refuses a
  name that the MCP config of the person does not have. It refuses a
  stdio server whose program is not in the session image of the
  repository of the current folder, and names the program.
- **01M57840R2HNXGP9B2ENSKTWD3** The daemon gives each new container
  session the servers of `mcp.json` in a variable whose value is in no
  spec and in no command line. The relay adds them to the MCP config of
  `claude`, beside `riff`, and gives `claude` no such variable. Each
  value of the `env` and the `headers` of a server is a secret that no
  event shows.
- **01M4ZV3B6J89KJQ0F14YWTZMJQ** `riff wave run` in a clone asks the
  daemon to run the current wave of the repository of the clone. The
  daemon keeps each wave in `waves.json` in its folder, with the main
  clone, the riff-server and the host, and starts it again after its
  own restart. `riff wave stop` ends the wave: the daemon starts no new
  session for it, and the sessions that run go on.
- **01M4ZV3B6KP33YV750DJ89SMFF** The loop of a wave first waits for
  room. Then it looks at the pause, the claims and the holds on
  riff-server, and at the free work of `gh`: the pull requests that wait
  for a verify, then the free items of the current wave, as the rollout
  of the lead takes them. It starts one session for the first work that
  it may start. With no such work, it waits for the end of a session,
  or 60 seconds. Superseded in part by 01M519Z4RWHWP3TKNP4T5GKKE9 for
  `riff wave run --peers` (01M526Z1Q3QM3X0C7N6MCX8Z6T).
- **01M4ZV3B6M7PZRJ39XTXBZP3ZH** A wave starts nothing while the riff or
  the repository is paused. It never starts a held item, an item that a
  session claims, or an item that a session of the wave runs.
- **01M4ZV3B6NKPEZ5FNE27X95GF0** A wave starts the verify of an item
  only when no session holds the item, and the Gate of the head commit
  of its pull request is done. The verify is a new session, so an
  author never verifies its own item. Superseded in part by
  01M5QYDHN27RC5SRHAB56PFKK0 for the checks that it waits for.
- **01M4ZV3B6PSRH40EYQEVYH2MBA** A wave starts one item at most 3 times
  in a run of the wave. Then the lead of the person gets one message,
  and the wave leaves the item.
- **01M4ZV3B6Q0Y6AX0KSKC55S31R** For each item, the daemon does the
  host steps of `riff run` itself: the checks, a new worker session,
  the claim, the keep-alive, the clone and the start. After the end of
  a build, it sends a verify request for the open pull request of the
  branch to the lead (but see 01M51KH0H7J35PEKMHP5KQB84T), then
  releases the item and ends the session. The
  record of the session holds the job of the wave, with no secret. So a
  new daemon goes on with the keep-alive, and does the steps after the
  end. When the session ended while no daemon ran, the new daemon does
  them at its start. The daemon runs `gh` with `GH_REPO`: it runs in no
  clone.
- **01M4ZV3B6R4WSEJXYT7QPRWT6X** A verify session has a clone at the
  head commit of the pull request, detached. Its start text says to
  check the work against the `Done when:` line and to change nothing.
  Its last reply is the result: the first line is `PASS` or `FAIL`. The
  daemon reads it from `events.jsonl`, and reports it as `riff verify`
  does: the comment on the pull request, the status `riff/verify` of the
  commit, and the result to the holder of the item and to the lead. With
  no verdict, or when the report fails, the lead gets a message, and the
  pull request waits for a new verify.
- **01M4ZV3B6S1A64TYK8MTDNRCQ3** At each look, a wave removes the folder
  of each session of its repository that does not run, when the pull
  request of its branch merged. For a pull request with `Refs #N`, it
  first adds the comment `Merged in #PR (COMMIT)` to the issue, when the
  issue does not have it. The folder of a verify that did not pass goes
  at its end, and the folder of a verify of a closed pull request goes
  at a look.
- **01M5NCZ6CVCSTN3ZZDWNVGFG5S** The wave starts no session for the
  release issue of a release: the issue with the label `release`, made
  by `riff release cut`. It starts no build, no verify and no rebase for
  it.
- **01M5ND16V2SVN8A9K7W90VC6X9** A tracker issue is an issue with the
  label `tracker`, or an issue whose title starts with `Tracker:`. It
  has no code of its own: its parts are separate items. A wave starts no
  build session and no verify session for a tracker issue, and it is no
  free work of the rollout of the lead. `riff wave list` and `riff top`
  show it in the group `tracker`, never in `free`. `riff wave make`
  makes the label `tracker` when it is missing.
- **01M5P20DPK4GAQRDEKC1E6V718** A verify request for a head commit, in
  the thread after the last verify result for it, answers a failed
  verify of that head: the wave starts a verify of the head, not a
  build, when its Gate and Hygiene pass.
- **01M5P26VPJA73WCPVZY6CPG9HD** A session of a wave has a grace of 60
  seconds after the daemon closed the input of claude. When no event
  comes in that time, the daemon stops the container, and the item is
  free. It does not wait for the quiet limit.
  Superseded in part by 01M5R0QFM265CCKTADK4T5XREQ.
- **01M5P26VSA7AA99K1RYG83R8J9** When a build of a wave ends and its
  branch has no pushed commit, the daemon commits each change of the
  clone as a WIP commit on that branch and pushes it. The lead gets one
  message with the branch and the commit.
- **01M5P26VW5GBJDQZGEYW9A77Y0** The daemon removes the folder of one
  ended session on purpose with riff daemon sweep --remove NAME. It
  first checks that the work of the folder is on a merged branch, or
  that the person gave --force. The lead can run it with --host.
- **01M5QVARWYFHTQV5THNRB09SZA** The clone of a verify session has the
  head commit of a pull request from a fork too. After the clone, the
  daemon fetches refs/pull/N/head from origin, with N the number of the
  pull request, then checks out the head commit, detached. It checks out
  the commit, not the ref, so a push to the pull request after the look
  does not change what the verify reads.
- **01M5QVH3MAEPPJN9PPVDX6N2EY** The riff daemon runs the look of the
  repository of each wave that it runs, each minute, until `riff wave
  stop`: it reads the open pull requests and the Gate of `main` with
  "gh", and posts as the person. The messages of
  01M49Q30XMVRFX42YTM1PHX0RZ, 01M49Q316RXNATJP587DWGDNCD and
  01M51QEYKTD7ZEB0YA0C3MQTPN come from this look, and the steps of
  01M51QEYPGD6W345MZ4F85C38F and 01M51QEYVQN74Q1VDTVHB0445Q too. A
  message goes to the lead of the person, or to the session that holds
  the item of a pull request with a conflict. The daemon keeps what it
  told in memory only. The wave goes on when the look fails: the daemon
  prints the fault one time until it changes.
- **01M5QW2KVAS05Q72630RNW47G0** A pull request whose verify passed can
  fail a check of its head commit: a check run that ended with failure,
  timed out or startup failure, or a status in failure or error, except
  riff/verify. The wave of the daemon reads the rollup of each open pull
  request at each look. For the item of such a pull request, it starts a
  build session with the names of the failed checks in the start text.
  The text tells the session to read the failed run, to fix the work and
  push to the same pull request, and, for a known flake, to run the job
  again one time. Each head commit starts one such session in a run of
  the wave. A pull request with a conflict gets the rebase first. A
  cancelled or skipped check did not fail.
- **01M5QW2KYFM4C287D9NMSPH8TA** When a session of a wave for failed
  checks ends with the same head commit, the daemon sends no verify
  request. It reads the checks of the pull request. The lead gets a
  message when a check still fails, and a note when none fails because
  the session ran the job again.
- **01M5QW2M1M5QZJXSP0BY6XG118** A pull request whose verify passed and
  that has auto-merge off never merges. The wave starts no session for
  it. It tells the lead one time for each head commit, with the reason
  and the command gh pr merge --auto --squash N.
- **01M5QW2M4WX53N29SHRV0S32WM** No session builds a release: a pull
  request on a branch release- or of an issue with the label release. A
  failed check of a release pull request, at any state of its verify,
  gets one message to the lead for each head commit, with the names of
  the checks.
- **01M5QW2M8FPBMMF4S0D8PC39AB** riff top and riff wave list show an
  item with no claim whose pull request cannot merge by itself in the
  group stuck, with the reason: conflict, checks, or auto-merge off. The
  group comes after verify.
- **01M5QYDHN27RC5SRHAB56PFKK0** A wave starts the verify of an item
  when the full check of the head commit of its pull request is done
  (01M5QYDE41FVFX26EC8K5QGYH4). A repository whose full check is not
  named `Gate` gets its verify when each required check of the default
  branch passed or ended. A wave that cannot read the required checks
  fails its look, and says why. Supersedes 01M4ZV3B6NKPEZ5FNE27X95GF0 in
  part.
- **01M5R0QFM265CCKTADK4T5XREQ** While the last background_tasks_changed
  event of a session of a wave lists one task or more, the daemon uses
  the quiet limit for it, not the grace (01M5P26VPJA73WCPVZY6CPG9HD).
  When the list is empty, the grace of 60 seconds counts from the last
  event. It supersedes 01M5P26VPJA73WCPVZY6CPG9HD in part.
- **01M5R13DGMTNWPY88A3EG49WWS** The look of the daemon watches the head
  commit of `main` with the full check of the repository
  (01M5QYDE41FVFX26EC8K5QGYH4), not with the check run `Gate`. It reads
  the required checks of the default branch at each look, and the check
  runs of each of them on `main`. `main` is broken when a check ended
  with `failure` or `timed_out`, and passed when each check passed. A
  check that runs, a cancelled check and a head with no run of any check
  give nothing. The messages, the fix item and the note say "the full
  check". Supersedes 01M51QEYKTD7ZEB0YA0C3MQTPN in part.
- **01M5R0HEP1MXWYY643F1N1DWEM** The free work of a wave has no build of
  an issue whose body has no line that starts with `Done when:`. The
  wave tells the lead one time for each such issue, with the fix: add
  the line. The wave takes the issue at its next look after the line
  comes. A verify and a rebase do not change.

### The lead, the view and the takeover

- **01M501P9MXNAP5QXNVBVF5JJFN** `riff lead start` in a clone starts
  the lead of the repository of the clone through the daemon, when it
  does not run, in the container `riff-lead-OWNER-REPO`. Then it
  attaches the terminal of the person to it. When the lead runs
  already, it only attaches. Ctrl-P Ctrl-Q detaches, and the lead runs
  on. `riff lead` with no subcommand stays the command of riff 1.x.
- **01M51PD7M04HZR2PVVJA4WCBK5** Before each attach to the lead, from
  `riff lead start` and from `riff view`, riff sets the terminal of the
  container to a size that is not the size of the terminal of the
  person. The attach then sets the size of the person, so `claude`
  draws its screen at once. When riff cannot set the size, it says why,
  and attaches.
- **01M51PD7Q24N252V66NFEHY49N** After an attach to the lead, riff
  tells a detach from the end of the lead: the lead ended when its
  container does not run. After a detach, `riff lead start` and `riff
  view` say that the person left the terminal. After the end, they say
  that the lead ended and how to start it again. `riff lead start` also
  says that the `claude --resume` line of `claude` works only in the
  container.
- **01M501P9Q51AYN9GBPWKVHA229** The folder of the lead is
  `leads/OWNER-REPO` in the data folder of riff. At the first start,
  riff makes `clone/` with `git clone --reference MAIN`, on the default
  branch of `origin`, and the Claude folder `claude/`. Both stay
  between the starts of the lead. Each start is a new session of
  riff-server and of `claude`.
- **01M50RRQ9DT0JPSB358EAZRP8Z** Each later start of the lead fetches
  `origin` into the clone of the lead, and fast-forwards its branch to
  its upstream. A clone with a local change stays as it is, and riff
  shows a note. A local change is a change that is not committed, a
  commit that is not on the upstream, or a branch with no upstream.
- **01M51JYDHAQDJ3GH2J8DT3AZQS** When git cannot fast-forward the
  clone of the lead, for example because of an untracked file with the
  path of a new file of `origin`, the clone stays as it is. riff shows
  the note with the error of git, and starts the lead.
- **01M501P9SDGVWG3MTVCW5BDRQQ** The Claude folder of the lead gets the
  first-run answers of `claude`: `hasCompletedOnboarding`, the `theme`
  of the `.claude.json` of the person (else `auto`), and the trust of
  the clone. A key that is there stays. riff writes the `settings.json`
  of a session (01M4ZD48DHAKJ1EHX6QHYAQ7ZM) only when it is not there.
- **01M51JACH6TQP318ZFG6NQ9GRM** The Claude folder of each session of
  `riff run` and of the daemon gets the same first-run answers as the
  Claude folder of the lead (01M501P9SDGVWG3MTVCW5BDRQQ). So a takeover
  shows the history of the session with no first-run question. One
  function of riff writes the Claude folder of each session.
- **01M501P9VN47QAKZ6B8P39ZSW4** The spec of the lead is the spec of a
  worker with the name of the lead, and the command `claude
  --session-id ID --permission-mode auto START`, where START is the
  first prompt of the lead. The daemon runs it as `/opt/riff/riff relay
  --terminal -- COMMAND`: the relay adds the riff tools, and runs the
  command in its own place, so the command has the terminal.
  Superseded in part by 01M5GG2X5F1HXCQJ2JJWV6305K.
- **01M501P9XWXWDBSXP4QRTHTZ4P** The host starts a session of
  riff-server for the lead, not a worker. While the container of the
  lead runs, the daemon keeps that session alive, also after its own
  restart. At the end of the lead, the daemon ends that session. The
  lead gets the riff tools that talk, and `lead`, `hold`, `free`,
  `pause` and `resume`. The daemon refuses each other tool with a text
  that says why. Superseded by Wave B (01M519Z3FTAZ20DQKDTW09M6NS).
- **01M501PA04NP79RJQEDP4D7YTM** `riff relay watch` in a session asks
  the daemon, on the socket of the session, to wait for the next wake
  of the session on riff-server. It prints the line of the wake, as
  `riff watch --once` does, and ends. While the daemon is away, it
  tries again. The instructions of the riff tools of the lead tell it
  to keep `riff relay watch` running as a background task. Superseded by
  Wave B (01M519Z3DB3MVP0FK3FBC9KSGD).
- **01M501PA2C087SEN4M4QTK5J1E** The daemon starts a session with a
  terminal with `podman run --rm -d -i -t`: conmon holds the terminal.
  `podman wait` gives its end. Such a session does not wait for room,
  and does not count in the limit of the host. The daemon says `done`
  to `riff` when its container runs.
- **01M501PA4MG5NYNK7FBBRRAT20** `riff view` shows the daemon of this
  machine with ratatui, and draws it again each second: each session
  with its container, its item, its state, its age and its last step;
  the daemon and the limit of the host; each start that waits for room;
  each wave. Up and Down select a session. Enter attaches to the lead:
  the selected session when it is a lead, else the first lead. `t`
  takes over the selected session. `q`, Esc and Ctrl-C end the view;
  the sessions go on. Its tests draw it on the `TestBackend` of
  ratatui.
- **01M51PD7T7451SR11W7JA7QW3X** The SESSION column of `riff view` is
  as wide as the longest name of a session, so each name shows in full.
- **01M50RRQBZ8ASPT25DYPX78HP9** After a takeover from `riff view`,
  the note under the rows says that the session goes on, or that it
  ended before the takeover. The view reads the keys of the person in
  a blocking task of tokio, not in its async loop.
- **01M501PA6W0NCG9JEK017F9304** `riff takeover NAME` takes over the
  worker in the container NAME. It waits for the end of the turn of the
  session. Then it runs `podman exec -it NAME /opt/riff/riff relay
  --terminal -- claude --resume ID --permission-mode auto` with the
  terminal of the person. When that ends, it tells the daemon to go on.
  It exits with 0 when the session goes on, and with 1 when the session
  ended before the takeover. Superseded in part by
  01M51KJ1R9K7A7RW8T8XH8VW43.
- **01M51KJ1R9K7A7RW8T8XH8VW43** `riff takeover NAME` and `t` in
  `riff view` stop the turn of the session at once, also in the middle
  of a tool call. `riff takeover --at-end NAME` waits for the end of the
  turn. A takeover also waits for the end when the input of `claude`
  closed after the result of its turn, and when the relay of the
  session cannot stop a turn. `riff takeover` and the view say which
  one they do. The result of a turn that a takeover stops is no fail of
  the session. The new input after the takeover says that the stop can
  come in the middle of a step.
- **01M501PA94CDZDH214GGFMTH5V** On the socket of a session, `hold`
  tells the relay to close the stdin of `claude`, and to keep the
  container at the end of `claude`. The relay then says `held`, and
  waits. `go on` tells it to start `claude` again, with `--resume ID`
  in place of `--session-id ID`. The sequence numbers of the events go
  on. The hello of the relay says `held` while it waits, so a new
  daemon knows.
- **01M51KJ1V0STB4QCN4N5SBYGSP** `hold` has the field `interrupt`.
  With it, the relay writes the interrupt control request of
  stream-json on the stdin of `claude` before the close, as the Agent
  SDK does. The hello of the relay says `interrupts` when the relay can
  do it. A relay of riff 1.5 reads a `hold` with `interrupt` as its own
  `hold`. The protocol stays 4.
- **01M501PABB010P4S8YF7R29XXS** The daemon keeps a takeover in the
  record of the session, from the ask until it gives the session back.
  It gives the session back at the `go on` of `riff takeover`, or at
  the end of its connection: it says `go on` to the relay, and gives
  `claude` one new input that says that a person used the session. The
  quiet limit does not count during a takeover. After a restart of the
  daemon, a takeover stays, and a new `riff takeover NAME` gets the
  terminal at once. The daemon refuses a takeover of a session with a
  terminal, of a session with no relay, and of a relay of a protocol
  before 4.
- **01M501PADKZNRHBZD9XPMJGW06** `riff daemon status` and `riff view`
  show the state of each session and its last step. The state is
  `works`, `ends` after the result of its turn, `holds for a takeover`
  while a takeover waits, `taken over` while a person has it, or `lead`
  for the lead. The last step is the last line that `riff run` shows of
  its events.
- **01M5GG2X5F1HXCQJ2JJWV6305K** The lead starts claude with
  --dangerously-skip-permissions, in place of --permission-mode auto:
  claude --session-id ID --dangerously-skip-permissions START. A worker
  and a verify keep --permission-mode auto.
- **01M5GVPNAKAPA596K9361HSAAV** In a container, a riff command that
  needs riff-server, with no `--server` and no `RIFF_SERVER`, asks the
  riff daemon through the socket of its session (`RIFF_SOCKET`). The
  daemon makes the call on the riff-server of the session with the
  sign-in of the person on the host. So `riff wave list` and `riff
  release check` work in the container of the lead. Through the socket,
  each session can read the build of riff-server, the sessions and the
  holds of a thread. Only the lead can resume the whole riff.
- **01M5GXYC201EFKMGA3ZNJM2J7Z** In a container, a --host request (riff
  release test --host, riff daemon status --host, riff daemon sweep
  --host, riff wave run --host, riff build --host) goes through the
  socket of the session. The daemon posts it to the daemon of the host
  through riff-server, as the session. Only the lead can send it. riff
  daemon status with no --host in a container asks the daemon on the
  socket of the session. It never looks for the socket or the unit of
  the host.

- **01M5GY61WSDXG5G6E2SQQ82AWC** In a container, riff wave move and riff
  wave end --move-open-to-backlog read the claims of riff-server through
  the riff daemon, as riff wave list does. No riff wave command calls
  riff-server straight when RIFF_SOCKET is set and no server is named.
- **01M5NG4Z0XS1B4QYXFJBW9CWGX** `riff lead start` makes the new lead
  session the lead of its user in the repository, and prints which
  session was the lead before; that session is not the lead any more. In
  a lead container, `whoami` says in its first line when the session is
  not the lead, and names the lead (#1301).
- **01M5NJ87NXF6077Y9W32840BR8** riff lead start refuses to start the
  lead on a host with no claude token of riff claude-token. It names the
  step: claude setup-token, then riff claude-token, on that host.
- **01M5NJ87YEHHGJBANM7CRYV56Z** When the lead runs with the sign-in
  file of claude of the host and the host replaced that file, riff tells
  the person that the sign-in of the lead is stale, with the step to fix
  it and to start the lead again: at each attach of riff lead start and
  riff view, and once in riff chat from the daemon.
- **01M5P21M7MSP503125GD2PJGRE** In the container of the lead, riff
  release cut, publish and approve reach riff-server through the socket
  of the session, as riff release check does: the roster, the chat line,
  the read of the chat and the pause of the riff go through the riff
  daemon.
- **01M5QV9STQF1P9GN65P35E0FJH** The lead has no Remote Control. A
  person answers the lead in riff chat (#1363). The book names no flag
  or command of Remote Control, and the start of the lead is riff lead
  start.
- **01M5QVX8D3JE51FAYRWXMAYFAY** In a container, riff members, riff
  audit and riff session why read riff-server through the riff daemon on
  the socket of the session (RIFF_SOCKET), as riff wave list does. They
  need no RIFF_SERVER and no sign-in in the container. Only the lead can
  read the log for riff audit, and riff-server still gives it only to
  the owner and the admins. riff invite, riff remove, riff admin and
  riff owner stay for a person in a terminal on the host (#1338).
- **01M5QVX8FFG2FPWZEW61A985CF** In the container of the lead, riff run
  ITEM asks the riff daemon on the socket of the session to run the item
  as a job of the wave of the repository: the claim, the clone, the
  session in the room of the limit, and the steps after its end, as the
  wave does them. riff run answers at once and does not follow the
  session. It takes no --image. Only the lead can ask it. When the job
  does not start, the daemon tells the lead why (#1338).
- **01M5QW31TFA1W2GWPGRS2Y65GP** In a container, riff who and riff
  whoami read riff-server through the riff daemon on the socket of the
  session, as riff members does. Each session can read them. The URI
  that riff whoami shows is the URI that riff-server holds for the
  session (#1338).
- **01M5QVCVXWBBG1BMGRVBQS25W9** In a session with a terminal, the lead
  and a takeover, the relay puts the folder of /opt/riff/riff first on
  the PATH of claude. So a person who types ! riff in the terminal runs
  the riff of the container. The path /opt/riff/riff stays, and the
  permission rules that name it still match.

### The first run

- **01M4ZJN6R68YBD3XMM725ZRJ7W** `riff init` does the first run of riff
  3.0 on a machine (01M4Z60B7K13MSYM8ZY20WN6NV): first the checks, then
  the daemon, then the session image. It needs no riff-server and no
  sign-in. On a host that is not Linux, it changes nothing, and says
  that riff 3.0 needs Linux.
- **01M4ZJN6TEWCYCB39XTH6BBD9H** The checks of `riff init`, in this
  order: `podman --version` runs, and riff does not run as root, so
  podman runs rootless; `/etc/subuid` and `/etc/subgid` each give the
  person 65536 IDs or more, by the login name or by the user ID, added
  over the lines; cgroup v2 is on, and the `cgroup.controllers` of
  `user@UID.service` has `cpu` and `memory`
  (01M4Z60B7MW963MBQAPN2CJEQZ); `riff.toml` of the repository of the
  current folder pins the session image (01M4Z7SP3KSKJYKNRP1MQ70C4Q).
  riff prints one line for each check: `pass`, or `fail` with what it
  found and the fix. Each failed check has a fix of its own.
- **01M510NW68CTZBHHW7ZYFV7Q3X** `riff init` also checks the riff
  binary of its own process: its ELF has no interpreter. A riff of
  `cargo install` or `just install` links the libc of the machine, so
  this check fails, and its fix is `riff update`.
- **01M511JYPZ8H73DCJ7867YSFGH** After the check of podman, `riff init`
  checks the init of podman: `catatonit` in a helper folder of podman
  (`/usr/local/libexec/podman`, `/usr/local/lib/podman`,
  `/usr/libexec/podman` or `/usr/lib/podman`). Each session needs it
  (01M51117K3WXK35CE67KPRFMA8). The fix installs `catatonit`.
- **01M501K21EK8F8N9NPR4565FVP** The fix of `/etc/subuid` or
  `/etc/subgid` adds a free range of 65536 IDs: it starts at the first
  ID after the end of each range in the file, and at 100000 or more.
- **01M4ZJN6WR0JJ891R9JMQNPKGD** When a check fails, `riff init`
  installs nothing and pulls nothing, and exits with 1.
  `riff init --check` runs the checks and changes nothing.
- **01M4ZJN6Z1DGD43CKZ19QPN2NK** When a riff daemon answers, `riff init`
  keeps it, and does not start it again. When the build of that daemon
  is not the build of this riff, `riff init` says so, and names
  `riff daemon install`. When no daemon answers, `riff init` installs
  the daemon as `riff daemon install` does
  (01M4ZDTVK74596M0ZA46N51NKV).
- **01M4ZJN718EF1PC9YY9GFD0EJ2** Then `riff init` asks the daemon for
  the session image. The daemon pulls it when podman does not have it.
  The last line says that the machine is ready for `riff run`.
- **01M5QVF5535YRN1EF3DZAB4W8G** `riff init` also checks the repository
  of the current folder on GitHub, with the `gh` of the machine, when
  the remote `origin` of the folder names one. It prints one line for
  each check: the repository allows auto-merge and squash merge; a
  ruleset of its default branch requires the check `riff/verify`; the
  repository has the open Backlog. Each failed check names its step,
  and each step has its own fix. A folder with no remote `origin` has
  no such check. When `gh` cannot read the repository, one check fails,
  and its fix is `gh auth login`. `riff init --check` only reads.
- **01M5QVF5C3CYJPMQ9CXNZJ8E93** The book page "Start a Riff" has the
  how-to "Add another repository to the riff": the checks of `riff
  init`, `riff.toml` with the session image on the default branch, the
  settings of the repository, the ruleset of the default branch, the
  Backlog and the first wave, and a session image with another
  toolchain, built from the session image of riff. Each step has a
  copyable `sh` block.
- **01M5QWA0E9A4ZJ8X49EM2MJM5Q** Each failed check of the repository on
  GitHub carries its step in code (`riff::repo_setup::Step`): allow
  auto-merge and squash merge, make the ruleset `riff` that requires a
  pull request and the checks `Gate` and `riff/verify`, and make the
  Backlog. A step is safe to do again. The how-to makes the same
  ruleset: a test compares the two. `riff init` does the steps after a
  question (01M5R1672NF56P15VD0JEX51K4).
- **01M5QZ01Y14Z76RC220RT7YTX7** The how-to "Add another repository to
  the riff" has a step "Use the checks that you have" with a copyable
  `sh` block that lists the checks that riff waits for: the required
  checks of the ruleset of the default branch, but `riff/verify`
  (01M5QYDE41FVFX26EC8K5QGYH4). The step says that an empty list means
  the check `Gate`, and that a repository with checks already needs no
  job `Gate`. The verifier of the wave gets the same rule in its start
  text.
- **01M5R1672NF56P15VD0JEX51K4** In a clone of a repository on GitHub
  that is not set up for riff, `riff init` does the setup of the
  repository after one question, `Question::Setup`, with `[y/N]`: only a
  `y` in a terminal does it. `riff init --yes` does it with no question.
  `riff init --check` names each step and changes nothing. A step is one
  of these, in this order: allow auto-merge and squash merge; make
  `riff.toml` on the default branch of `origin` (the branch
  `riff-setup`, the pull request "Set up riff", a squash merge); make
  the open `Backlog`; make the open wave `Wave 1`; make the
  ruleset `riff`. Each step uses `gh` and the API of GitHub, never the
  working tree of the clone. A second run of `riff init` in a repository
  that is set up asks nothing and makes no call that changes GitHub.
- **01M5R1675HZ6TJPMQ6536X37T5** The `riff.toml` that `riff init` makes
  pins the session image of the release of riff that runs
  (`riff::session_image::of_release`): the `riff.toml` of riff at the
  commit of the build. The ruleset `riff` that `riff init` makes
  requires `riff/verify` and the checks that the CI runs on a pull
  request: the check runs of the head of the latest pull request of the
  default branch that has some. A repository with no such check run gets
  `Gate`. When the setup pull request is the only one, riff waits for
  its check runs. `riff init` also checks that the repository has a CI
  check and a `CLAUDE.md` on the default branch. It has no step for
  either: the check names the fix. A person who is not an admin of the
  repository gets each step named, and riff changes nothing. A step that
  fails prints why and its fix, and the next steps run.

### The permission rules

- **01M5084JXTWZQEG7DJHS2E96NQ** riff owns the Claude Code permission
  rules of each role in one table in its code
  (`riff::permissions::TABLE`). Each row has the rule, its kind (allow,
  deny, or the step of a person), the roles that need it (lead, worker,
  verifier), and its places: the project settings (`riff setup`), the
  user settings (`riff init`) and the settings of each session of the
  daemon. A test checks that each `riff` and `gh` step of the skill
  matches an allow rule of the user place, a deny rule, or the row of a
  person. It also checks that each such step of the start text of a
  session of the daemon matches an allow rule of the session place for
  the role of the session. Superseded in part by Wave C
  (01M51R8G2RZAVZN1432EGZSW0X), and by 4.0 (01M59EWWDC7EBQEG02TVNN81HD):
  the table has no user place.
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
- **01M5084K4YQCZF20XA0P2485SG** The daemon writes the allow rules of
  the session place into the `settings.json` of the own Claude folder
  of each session. The Claude folder of the lead stays between its
  starts: at each start, riff adds each rule that it lacks.
- **01M51K1VJ67KYHRD7DQ86DSX1X** The lead in a container plans the
  waves with `/opt/riff/riff wave`. The session rule
  `Bash(/opt/riff/riff wave *)` of the lead allows these steps. The
  instructions of the riff tools of the lead name them.
- **01M5084K79M5XJT3MP8FFVQKDX** A check that needs a new build of riff
  builds it in the one clone `~/.cache/riff/check` of the machine. One
  check at a time uses the clone. The check calls the riff of the build
  by its path, `~/.cache/riff/check/target/static/release/riff`, with
  no variable and no prefix. A rule of the user place allows that path,
  with `~` and with the full path of the home. The check runs `riff
  init` of the build with `--yes`. Superseded in part by 4.0
  (01M59EWWDC7EBQEG02TVNN81HD): no rule allows the path.
- **01M56P380A5PXGMVJVCSD8KBRG** `riff build` makes the build of a
  check in `~/.cache/riff/check`. When the folder is not there, it
  clones the `origin` of the repository of its folder into it. Then it
  fetches `origin`, puts the clone on the default branch and runs `just
  static`. It prints the path of the riff of the build. The skill tells
  a check to run `riff build`, and names no git step in the clone: a
  session in a worktree runs no git step in another folder.
- **01M509TJNREVNYR9MNN4P9P4RA** Each allow rule of the table names its
  program and its words. Its only `*` is at its end, after a space. No
  allow rule names a path of the API of the forge: the steps of the
  waves are `riff wave` commands. A test checks each allow rule, and a
  list of commands that no allow rule of a place may match.
- **01M509TK01PW3RN2YPRJD38RSX** riff writes a settings file of Claude
  Code to a new file in the same folder, then renames it. A process
  that reads the file at the same time sees the old text or the new
  text, never a part. The file keeps its mode. A settings file that is
  a symbolic link stays a link: riff writes the file that it names.
- **01M51JCW04FAHZDQ144KDNCCPX** `riff init` and `riff init --check`
  name each wide allow rule, with its file: in the user settings of
  Claude Code, and in the project settings and the local settings of
  the repository of the current folder. A rule is wide when it is a
  rule of the Bash tool, and it has no pattern, or it has a `*` that is
  not at its end after a space, or it matches a command of the list
  `riff::permissions::OUT_OF_BOUNDS`. The old end `:*` of a pattern is
  ` *`. A rule of another tool is never wide. A wide rule fails no
  check.
- **01M51JCW2EKF8H8QFQYSVRSPET** `riff init` asks one yes or no
  question before it removes the wide rules. Only a `y` in a terminal
  removes them. A no, no terminal, `--yes` and `--check` keep each
  rule. riff removes only the wide rules, and keeps each other rule and
  key in its order.
- **01M51M3ZHHBE61CEG9T6ENFG18** With `update.auto = true`, at each
  start the riff daemon adds each rule of the user place that the user
  settings of Claude Code lack, when its own binary is a release. It
  writes as `riff init` does, keeps each other rule and key in its
  order, and removes no rule. With `update.auto = false`, it changes
  nothing. `riff update --auto` is the only switch, and it asks no other
  question. Superseded by 4.0 (01M59EWWDC7EBQEG02TVNN81HD).
- **01M51M3ZKWQYBBPD97R0G9QAX0** A binary is a release when its
  SHA-256 is the line of `riff-TARGET` in `SHA256SUMS` of the GitHub
  release of its version tag. With each other binary, for example a
  check build or a dev build, and when riff cannot read `SHA256SUMS`,
  the riff daemon changes no settings of Claude Code.
- **01M51QARV45MGEKC6KGWX1N5NS** The sync of the rules reads
  `SHA256SUMS` only from the GitHub releases. When `RIFF_RELEASES` is
  set for the riff daemon, its sync changes no settings of Claude Code,
  and its line names the variable.
- **01M51M3ZPAEMJ64266QSJBNQ46** The riff daemon logs each rule that
  its sync adds, and one line of the result. It keeps that line in the
  local files of riff. `riff init` and `riff init --check` show whether
  the sync is on, and that line. Superseded by 4.0
  (01M59EWWDC7EBQEG02TVNN81HD).
- **01M51M3ZTV61C9BMFT3J1M06NF** The unit of the riff daemon gets
  `CLAUDE_CONFIG_DIR` when it is set at the install, so the daemon and
  `riff init` change the same user settings of Claude Code.
- **01M5084K9J566981EXZ7YJRGRR** When the auto mode check refuses a
  step, the session stops that step, writes the command and the reason
  on the issue of its item, and tells the lead. It does not try the
  step again in another form. The fix is a new rule in the table (the
  step is in bounds) or a change of the skill (the step is out of
  bounds). The fix is never that a person does the refused step by
  hand.
- **01M51V0Q5E7MNPWP9T7TSJFT5W** A session runs each `riff` and `gh`
  step alone, in its own Bash call, and gives each file of the step by
  its absolute path. No `cd` comes before the step, and no `&&`, `||`
  or `;` joins another command to it. The skill says so. A test checks
  that no code line of the skill joins a step to another command.
- **01M516HETNZCBFBJNHT8BKFB1Y** riff gives each session that it starts
  the auto mode settings of one place in its code
  (`riff::permissions::auto_mode`): `skillOverrides` with
  `auto-mode-setup` off, so no session shows the dialog "Teach auto
  mode about your environment?" and no session has `/auto-mode-setup`;
  and `autoMode.environment` with `"$defaults"` first, then the places
  of riff work: the GitHub owner of the repository, the repository of
  riff with its releases, `crates.io`, `ghcr.io` and the riff server of
  the session.
- **01M516HEX3GVTN9K2QS12A4PG3** `riff workers start` gives each worker
  the auto mode settings in its flag settings (`--settings`). The user
  settings file does not change. The daemon writes them into the
  `settings.json` of the own Claude folder of each session. The Claude
  folder of the lead stays between its starts: at each start, riff adds
  each setting and each entry that it lacks. riff writes no `autoMode`
  into the project settings: Claude Code does not read it there.
- **01M5QWJJBY5VW53F0XAWWDQ79K** A worker session in a container may
  read the checks of a pull request, read the log of a failed run, and
  run a failed job again: the session rules Bash(gh pr checks *),
  Bash(gh run view *) and Bash(gh run rerun *). The start text of a
  build session for failed checks uses them.

### The steps of an update

- **01M51QFPJAABDP1V7WFY8NQR60** The opt-in to the update by itself
  (`update.auto = true`) is the consent of the person for each step of
  an update. At each start, the riff daemon of a release does each step
  of `riff init` that needs no person and that is not in step: it writes
  its unit again (01M51QFPQ68JHS08TWXNNWZ0D7), and pulls the session
  image of each wave (01M51QFPSMDDMFPZXAEYWZYJPV). It logs one line for
  the gate and one line for each step. With `update.auto = false`, or a
  binary that is not a release, it changes nothing.
- **01M51QFPMSG81YRDDGQQ1WJDSQ** Each step of an update that changes
  the machine trusts `SHA256SUMS` only from the GitHub releases, as
  01M51QARV45MGEKC6KGWX1N5NS says for the rules. With `RIFF_RELEASES`
  set, the riff daemon does no step, and its line names the variable.
- **01M51QFPQ68JHS08TWXNNWZ0D7** The unit of the riff daemon is in step
  when its file is the text that this riff writes for the `ExecStart`
  and the variables of the file. Else the daemon writes that text,
  runs `systemctl --user daemon-reload`, and starts again on the new
  unit when it is idle, as for a new riff (01M517BTP6BAGTRBYWPX9GPVE0).
- **01M51QFPSMDDMFPZXAEYWZYJPV** The session image of a wave is in step
  when podman has the merged pin of the main clone of the wave
  (01M51STPHXSSZMHF9T7Z85097R). Else the daemon pulls it.
- **01M51QFPW37EXVH4AXH70MSXBT** The riff daemon never does a step that
  needs a person: the checks of the machine of `riff init` (podman, the
  init of podman, `/etc/subuid`, `/etc/subgid` and cgroup v2). It keeps
  the first such step that is not in step, with its fix, in the local
  files of riff. The status line of each session shows it as a tag
  `needs you: STEP: FIX`, until a later look finds it in step.
- **01M51QFPYHK0916C5ZN2B55BR0** `riff update --check` shows what the
  riff daemon of this riff does at its next start, and one line for
  each step of an update: the riff binary,
  the unit of the daemon, the session image of each wave and of the
  repository of the current folder, and the checks of the machine.
  Each line says `in step`, `not in step`, `needs you` or `failed`,
  with the fix. It changes nothing, except the step that needs a
  person for the status line. It exits with 1 when a step is not in
  step.
- **01M5GT0QV72BZA4VVWDPR1WG17** riff keeps no command only so that the
  update of an older line can finish. When the update from the line
  before stops on the new riff, the book gives the step that a person
  runs by hand.
- **01M5JFF7TAVA87TMJXK25HDJ19** riff init removes the plugin of riff 3
  that is left on the machine: the folder claude-plugin in the data
  folder of riff, and the keys enabledPlugins.riff@riff,
  extraKnownMarketplaces.riff and a statusLine that runs riff statusline
  in the user, project and local settings of Claude Code. Each other key
  stays. riff init --check names them and removes nothing. The riff
  daemon of a release removes them at its start behind the gate of the
  update by itself, and riff update --check names them with the fix riff
  init.

### riff run

- **01M4Z81APKC14AVNJWCGRQKNVW** `riff run ITEM` does one work item in
  one rootless podman container, with no daemon. The host does each
  riff step with riff 1.x: it starts a worker session, claims the item,
  sends the verify request and releases the item. The container does
  the work. No riff runs in the container. riff shows each event of
  `claude` on the terminal, with no secret (01M4Z60B8KDXD190RWC7D184Q3).
  Superseded by 01M4ZDTVXY84ETYJWMWDMQ34CN.
- **01M4Z81ARSTAK8HR96S084SDFD** Each session of `riff run` has a folder
  of its own, `sessions/ITEM-ID8` in the data folder of riff, where
  `ID8` is the start of the session ID. With `RIFF_HOME`, the folder is
  `RIFF_HOME/sessions/ITEM-ID8`. It holds the clone, the own Claude
  folder and `events.jsonl`, each event with no secret. The clone stays
  until the work of its branch is over (01M53NJ5V27BCHDNHK2THQQZZQ).
- **01M53NJ5V27BCHDNHK2THQQZZQ** The riff daemon removes the folder of
  each session of `riff run` and of a wave that does not run, also when
  no wave runs: at its start, then each 10 minutes, and before each
  look of a wave. It removes the folder when the branch of its item has
  no open pull request and a merged or a closed one, or when the branch
  has no pull request and the issue is closed. A session runs while the
  daemon has its record, while a job of the daemon starts it, or while
  the `riff run` that made the folder runs: it holds the lock
  `ITEM-ID8.lock` beside the folder from before the clone to its end.
  Superseded in part by 01M53S7WAXD46YPN0DR96XSTMG and
  01M53S7WDPQ2GWHJWJ6SZXZ5PN.
- **01M53NJ5Y2CMY5MWF9328KW7DE** `riff daemon sweep` asks the daemon to
  remove the folders of 01M53NJ5V27BCHDNHK2THQQZZQ at once. It prints
  each folder that the daemon removed, with the reason, or one line that
  it removed none. Superseded in part by 01M53S7WAXD46YPN0DR96XSTMG.
- **01M53S7WAXD46YPN0DR96XSTMG** The sweep of
  01M53NJ5V27BCHDNHK2THQQZZQ removes a folder only when its clone has no
  file that is not committed (`git status --porcelain`) and no commit of
  `HEAD` or of a local branch that no branch of `origin` has. Else, and
  when git cannot read the clone, the folder stays. `riff daemon sweep`
  prints each folder that stays for this reason, with the reason. The
  log of the daemon gets that line one time, and again when the reason
  changes. Superseded in part by 01M5NVKNGZPK0GG7B7ZWBS0DFE.
- **01M53S7WDPQ2GWHJWJ6SZXZ5PN** A job of the daemon keeps each folder
  of its item from before the clone of its session to the end of its
  last step after the end. The steps after the end read the folder, for
  example `events.jsonl` of a verify, and the daemon has no record of
  the session then. This is also true for a job that a new daemon takes
  up.
- **01M53S7WGM9DQV2DXFHBRSMTP8** Each `gh` call of the sweep of
  01M53NJ5V27BCHDNHK2THQQZZQ has a time limit of 30 seconds. At the end
  of the limit, riff stops `gh` and each process that it started. The
  sweep then removes nothing for that folder, says why, and goes on to
  the next folder. The next sweep tries again.
- **01M4Z81ATZK3FX9GRFTM236FE6** `claude` in a session of `riff run`
  takes the riff session ID as its own session ID (`--session-id`), and
  runs in the auto permission mode (`--permission-mode auto`).
- **01M4Z81AX3ND5RX7V1GEY4HMF5** The branch of a session of `riff run`
  is `worktree-ITEM`, the branch name of a worker of riff 1.x. When
  `origin` has the branch, the clone goes on from it.
- **01M4Z81AZ88ZS0HPHV5KGQF3HV** `riff run` takes only an issue,
  `issue-N`. The session opens the pull request with `gh`, with the
  frame of `riff pr open` (`Closes #N`, the summary, and the trailers
  of the issue and its wave), and turns on auto-merge at once.
- **01M4Z81B1913NF6HBZ5G38B3Q9** When `claude` ends, `riff run` finds
  the open pull request of the branch with `gh`, and sends a verify
  request to the lead of its user. Then it releases the item, and ends
  the session. Ctrl-C or SIGTERM stops the container with
  `podman stop`, and riff then releases the item. The exit code is 0
  only when the turn ended with no error and the branch has an open
  pull request.
- **01M5157TYDEHZHDXSC1AHCRDRN** `riff run` sends no verify request
  when a lead holds `verify-ITEM`. It reads the holds of the repository
  thread after the session, in the end step of the verify request, and
  prints the hold: who, when and why. It still releases the item, and
  the exit code does not change.
- **01M5157V0P92EBHF4M7N9K0RGR** An issue with the label `no-verify`
  gets no verify. Its pull request is no free work for a verify, for the
  rollout of the lead and for the wave of the daemon. `riff run` sends
  no verify request for it, and prints why. riff reads the labels of the
  issue with `gh`. When `gh` cannot read them, `riff run` prints the
  fault and goes on to the holds (01M5157TYDEHZHDXSC1AHCRDRN).
- **01M51KH0H7J35PEKMHP5KQB84T** After the end of a build, the wave of
  the daemon does the check of `riff run`: when a lead holds
  `verify-ITEM`, or the issue has the label `no-verify`, the lead gets
  a note with the reason in place of the verify request
  (01M5157TYDEHZHDXSC1AHCRDRN, 01M5157V0P92EBHF4M7N9K0RGR).
- **01M4ZQVVGQYXTAVRB69C7YPC3Y** The verify request and the release of
  `riff run` have the budget `END_BUDGET`, 10 minutes, not
  `SHORT_BUDGET`. A fault gets a new try while the budget lasts
  (01M4A8041F8EK1VYDE4C9QG8N8). So a new instance of riff-server at the
  end of a run drops neither. The keep-alive of the session runs until
  the release.
- **01M4ZYFJFTX9ZEX7QT4QH7E9T6** A SIGINT or a SIGTERM during the end
  steps of `riff run` (the verify request, the release and the end of
  the session) stops the wait for riff-server at once. Then `riff run`
  sends no more end step, names each step that it did not send, and
  ends with the exit code 1. riff-server frees the claim when the
  session stops, as for each other session.
- **01M51JG5Y47184401EWT4HVEG0** `riff run` listens for SIGINT and
  SIGTERM with one set of signal streams, from the start of the session
  to its last end step. A signal after the end of the session and
  before the first end step, for example while `gh` finds the pull
  request, stops `riff run` before that step
  (01M4ZYFJFTX9ZEX7QT4QH7E9T6). A second signal while the container
  stops does the same.
- **01M4ZBGK7SFEXE1AGH6Z0YQHJP** On a host that is not Linux,
  `riff run` starts nothing, and says that it needs Linux.
- **01M4Z81B3A324KKE8M8Z71TM4M** Before the claim, `riff run` checks
  podman and the image, the sign-in of `gh`, the sign-in of `claude`
  (01M4Z60B869HWDTZMSYPVYACD8), `HOME`, `USER`, and the wave of the
  issue. A failed check takes no claim.
- **01M4Z81B5A456HDKW7G0WFGCY6** `riff run` takes the merged pin of the
  main clone (01M4Z7SP3KSKJYKNRP1MQ70C4Q, 01M51STPHXSSZMHF9T7Z85097R).
  `--image` gives another image, for example a local build. When podman
  does not have the image, `riff run` pulls it before the claim. When
  podman cannot pull it, riff takes no claim.
- **01M4ZD48DHAKJ1EHX6QHYAQ7ZM** The own Claude folder of a session of
  `riff run` has a `settings.json` with `includeCoAuthoredBy: false`.
  So `claude` adds no attribution line, and the trailers end each
  commit and each pull request body. It is a setting of riff for its
  sessions: riff does not read or copy the settings of the person.
- **01M4ZSZX9H59T04QSJ01V4F9TE** After the `result` of a turn,
  `riff run` and `riff daemon events` show a line with the description
  of each background task that `claude` waits for, from the event
  `system/background_tasks_changed`. They show a new line at each
  change of the tasks until the next turn starts.
- **01M4ZSZXBXM6T6MGQ00ACQA66X** The start text of `riff run` tells the
  session to stop each background task that it started, and to leave
  none at the end of its work.

### How the relay drives claude

- **01M4Z60B82FHN2YG9YJHW8XZX1** A worker and a verifier run `claude -p`
  with `--input-format stream-json`, `--output-format stream-json` and
  `--verbose`, under `riff relay`. One `claude` process holds the whole
  session. The daemon sends each wake as a message. The relay writes it
  as one line on stdin, and keeps stdin open between the wakes. No
  session runs with `--bare`. Superseded in part by
  01M5792DP1KZR2XGSCMRWDHCGZ.
- **01M5792DP1KZR2XGSCMRWDHCGZ** While a worker or a verifier runs, the
  daemon watches its wakes on riff-server. Each `riff tell` to it and
  each message that wakes it is a wake. The input of `claude` is open
  from the start until the `result` of the first turn, and closes
  there. While it is open, the daemon gives each wake as one input:
  the line of `riff watch --once`. During a takeover, the wake waits
  for the go on. The daemon gives each wake once.
- **01M5792DRA12JD032F3H8JVC0Z** For each wake of a worker or a verifier
  after its input closed, the daemon tells the sender in a direct
  message that the session ended its work and does not read the
  message. After the end of the session on riff-server, a `riff tell`
  to it fails.
- **01M57AVKBHFVYJBRT0W2ZTAJDK** The daemon refuses each sender at most
  one time for each session.
- **01M4Z60B828HF30H7H017WPXGS** The relay sends the events of `claude`
  to the daemon: `system/init` at the start of each turn, each tool call
  and its result, each retry, `system/permission_denied`,
  `system/vcs_state_changed` for a commit and a push, `rate_limit_event`
  for the windows of the plan, and the `result` of each turn with its
  usage. `total_cost_usd` and `duration_api_ms` add up over the turns of
  a session: the daemon takes the difference for each turn.
- **01M4Z60B83ZH10HYKW7X1C0SSS** The permission rules decide each tool
  call of a session. `--permission-prompt-tool` sends each other call to
  riff. A takeover stops a worker at the end of a turn, and opens
  `claude --resume ID` with a terminal in the same container.
  Superseded in part by 01M51KJ1R9K7A7RW8T8XH8VW43.
- **01M4Z60B84FTGMXDZ1F52DC98C** riff keeps the MCP servers of the
  claude.ai account of the person in a session, as a plain `claude` has
  them.

### The home of the person

- **01M4Z60B85X62PNYSVJWKXEWH3** riff writes nothing in the home of the
  person outside its own folders. It does not write `~/.claude`,
  `~/.claude.json`, a git config or a shell file.
- **01M4Z60B86N999JRN7Z1G52PG4** Each session has its own Claude folder
  in a folder of riff. So riff never changes the Claude config of the
  person, and that config does not change the sessions. `claude` signs
  in with the token of the plan (`riff claude-token`).
- **01M4Z60B869HWDTZMSYPVYACD8** The daemon gives each session the
  long-lived token of `claude setup-token` in `CLAUDE_CODE_OAUTH_TOKEN`.
  riff gives podman only the name of the variable
  (`-e CLAUDE_CODE_OAUTH_TOKEN`), so the value is in no command line.
  riff keeps the token in its own folder, with mode 0600, and never in a
  log, a message or a clone. Only on a host with no token, riff binds
  `~/.claude/.credentials.json` into the Claude folder of the session
  with no write, and starts no session while the access token has less
  than 60 minutes left (01M58B7H50RCG2VXXV9XX5Y7HM).
- **01M58B7GZT7WFSBBG1XWJHR4MH** `riff claude-token` reads the token
  from stdin, with no echo. It refuses when stdin is not a terminal, so
  an agent session cannot run it. It keeps the token in the file
  `claude-token` in the folder of the daemon, with mode 0600, in the
  place of the old one. The token goes through no riff-server, no
  message, no log and no clone.
- **01M58B7H2HMYT6VTY0B8T2G0KW** When a token is set, the daemon starts
  each container with `-e CLAUDE_CODE_OAUTH_TOKEN` and binds no
  `.credentials.json`. The value goes in the environment of podman, and
  the scrub of the session takes it out of each event.
- **01M58B7H50RCG2VXXV9XX5Y7HM** When a token is set, the daemon,
  `riff run` and `riff lead start` do not read the sign-in file of
  `claude`, and have no rule of 60 minutes. When no token is set, the
  bind and the rule of 60 minutes hold.
- **01M58B7H7HD0H48A0C79F8M4CR** When `claude` in a session refuses the
  token, the daemon writes the mark `claude-token-refused` beside the
  token, and says it in the lines of the session. While the mark is
  there, each start fails with the step for the person, so the wave
  starts no new session and the lead gets one message
  (01M57A7A1W2K9440WZA1QZ3M9T). `riff claude-token` removes the mark.
- **01M58B7HA8CRCW0MJ17QP3R5QG** `riff daemon status` and the part
  `Hosts` of `riff who` show `claude token`: `set`, `refused` or
  `not set`, and never the value. With no token, `riff daemon status`
  gives the step: run `claude setup-token`, then `riff claude-token`,
  on this machine.
- **01M58XZF928T82YAMF4DSJBKBF** This supersedes
  01M58B7GZT7WFSBBG1XWJHR4MH, 01M4Z60B869HWDTZMSYPVYACD8 and
  01M58B7HA8CRCW0MJ17QP3R5QG in part, for the place of the token
  (R21). `riff claude-token` keeps the token in the OS keyring under
  the name `claude setup-token`, with `riff_client::secrets::set`, and
  writes no file for it. It removes the old file `claude-token` of the
  daemon folder when it is there, and says so. The daemon reads the
  token from the keyring at each container start, with `KEYRING_WAIT`.
  A keyring that is locked, or does not answer, starts no container:
  the wave tells the lead one time, as for the sign-in of riff
  (01M4385CEWGCP31DP5PAMPXZ97). The daemon never falls back to the
  sign-in of `claude` on the host then. The mark `claude-token-refused`
  stays a file. `riff daemon status` and `riff who` show `claude token`:
  `set`, `not set`, `refused` or `keyring locked`.
- **01M58B7HCXKZF39DCF6KV84JRV** When a token is set, the daemon posts
  no note for the sign-in of `claude` (01M57D3FC5PB5EQ6Y38Y5R2DBB), and
  the facts of the host do not warn about its minutes left.
- **01M4Z60B874MBWZWDP8YDEM3HR** riff trusts the clone of each session
  in its own Claude folder, as the person trusts the main clone: the
  `.claude.json` of the folder has
  `projects[CLONE].hasTrustDialogAccepted: true`.
- **01M4Z60B88DC67N3GVHY8JGRPQ** A session uses the sign-in of the
  person for git and `gh` (01M4Z60B7QDWEFQ96H448RE9N2,
  01M4Z60B89BHXQVJ2MGRRQ2CKJ), as a plain `claude` does. A commit has
  the git identity of the person and a `Riff-Session:` trailer. riff has
  no GitHub App.
- **01M4Z60B89BHXQVJ2MGRRQ2CKJ** The daemon reads `gh auth token` on the
  host, and gives it to the container in `GH_TOKEN`. riff gives podman
  only the name of the variable (`-e GH_TOKEN`), so the value is in no
  command line.
- **01M4Z60B89G8YNVMTEPGX7FQCV** A session commits with no signature.
  riff sets `commit.gpgsign=false` in the environment of the container
  (`GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_0` and `GIT_CONFIG_VALUE_0`), so
  it applies to each git call of the session. The squash merge on GitHub
  makes the commit on `main`. No gpg agent goes into a container.

### Data and peers

- **01M4Z60B8AFV68ZSAFA540G73C** The state machine of riff-server
  becomes a library in the daemon. Superseded by the re-cut
  (01M53NGW3FR555TWP8Z8HHRVDJ).
- **01M4ZHW1Y1X21ZPV053D0CXWMC** The crate `riff-state` is the state
  machine: the state and its commands, the log and its replay, the
  checkpoints and the stores. It does no HTTP and knows no sign-in.
  riff-server 1.x and the riff daemon use it. Its log and its
  checkpoints have the format of riff-server: the format tests of
  riff-server check the fixtures of each release against it.
  Superseded in part by Wave C (01M51R8GAE5E6ZKD4PSNJYGGMW). Superseded
  in part by the re-cut (01M53NGW3FR555TWP8Z8HHRVDJ). Only riff-server
  uses it.
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
- **01M59EWWDC7EBQEG02TVNN81HD** riff 4.0 has no user place of the
  permission rules. `riff init` writes no rule into the user settings of
  Claude Code, and the riff daemon syncs no rule into them. The table
  `riff::permissions::TABLE` has the project place and the session place
  only. `riff init` still names the wide allow rules of the user
  settings (01M51JCW04FAHZDQ144KDNCCPX).
- **01M59BTTH6ZATH7R2PFC6NGHJK** riff 4.0 has no `riff worktrees`, no
  `riff lead compact`, no `riff lead blocked`, no tidy of the worktrees
  of a host, and no compact of the lead. The tidy of the folders of
  container sessions stays (the sweep of the riff daemon). `lead.wake`
  and `lead.notify` stay as keys of the settings file.
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
- **01M51R8H3TXPKAEWDJQGQ01PBK** A repository with no session image of
  its own pins the session image of `como-technologies/riff`: its
  `riff.toml` has the same key `image` as the `riff.toml` of riff. That
  image has `claude`, `git`, `gh`, a POSIX shell and Rust. The people of
  the repository can make an image of its own later
  (01M4Z60B7JF80TMP6FHQ04MHCZ).
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

- **01M4Z60B8JEX15M1Q2435FZSDM** A session gets no power that a plain
  `claude` lacks, other than the riff tools. An owner or admin step, for
  example to admit a member or to change a role, needs a person at the
  CLI. No MCP tool does it.
- **01M4Z60B8K4C6DHFAKNCAWR91S** The daemon checks each peer and each
  record: iroh checks the node key, each record carries the signature of
  its author, and the daemon checks the size and the type of each
  message before it applies it. Superseded by the re-cut
  (01M53NGW3FR555TWP8Z8HHRVDJ).
- **01M4Z60B8KDXD190RWC7D184Q3** No secret goes into a log line, a
  message or a file of a clone. riff builds no shell string from input.
  CI keeps `clippy -D warnings` and `cargo audit`.

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
- **01M4Z60B8PW9GSQ79H176P6915** Before code, one short table says what
  sessions share and how riff keeps them apart: the files, git, the
  processes, `/tmp`, the ports, the caches, the home, the CPU, the
  memory and the network. It is on the book page riff 3.0 (#709). It
  replaces the threat model and the surface table of 2.0.
- **01M4Z60B8Q8YH6DRSZWPV42CQ0** Each release passes a real-session
  test: the riff binary and the session image, with a real `claude`, on
  a host with a real home (a `~/.gitconfig`, project settings, first-run
  state), do one item from the start to a pull request. The test runs in
  the `SessionSpec` of a session. CI runs the same image.
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
- **01M5940ES080P07A5NGWWRWF1Z** A new requirement goes into
  docs/src/requirements.md with `just req SECTION "text"`, never by a
  hand edit. The command takes the heading of the section, gets an ID
  like `just rid`, wraps the text at 72 characters, and writes it after
  the last requirement of the section. The check `block-end` of
  `reqs::check` fails when a requirement block, its first line and the
  indented lines under it, does not end with a full stop. The start text
  of a container session and CLAUDE.md say to use the command.

### The release commands

- **01M58B0HZYMAK1B2AJMWKHCXG0** `riff release cut X.Y.Z` makes the
  release pull request from `origin/main`. A lead or a worker runs it.
  It sets X.Y.Z in `Cargo.toml`, in the `plugin.json` of the plugin
  and in `Cargo.lock`, runs `deploy/release-check.sh`, and pushes the
  branch `release-vX.Y.Z`. The pull request "Release vX.Y.Z" has the
  label `release`, the current wave, auto-merge on, and a body with the
  level line, the list "Waves in this release", and the items of each
  wave since the last release tag. It says `Refs #N` and `Part of #N`
  for the release issue, never `Closes #N`. riff makes the release
  issue in the current wave when there is none. It refuses a version
  that is not newer than the last published release.
- **01M593FVJRTCGD1RPWXYJP5BNH** The release issue that `riff release
  cut` files has two parts. "Done when" is the release: published,
  deployed, and checked after the deploy. "Verify of the release pull
  request" is the verify of the pull request: only the change of the
  version, the format of the body, and the Gate. The issue lists the
  steps after the merge apart. The verify of a pull request with the
  label `release` reads that part, and not the line "Done when".
- **01M58B0J4VQ61ER69Q09JYP7CG** `riff release cut` refuses while the
  version on `origin/main` is newer than the last published release
  and has no published release. Only `--skip VERSION --reason TEXT`
  with that version lets it go on. The reason goes into the body of the
  new release pull request, and from there into its GitHub release.
- **01M58B0JABCET719T3HFB19NDW** `riff release publish vX.Y.Z` refuses,
  and changes nothing, when one of these is false: the pull request
  "Release vX.Y.Z" with the label `release` merged, and the status
  `riff/verify` of its head commit is a success; its merge commit is on
  `origin/main`, and `Cargo.toml` there has X.Y.Z; the tag does not
  exist on GitHub; the riff is paused; the account of `gh` is an admin
  of the repository. Then it makes the annotated tag on the merge
  commit, pushes only that tag, and makes the GitHub release: the body
  of the pull request with no link line, no line `Part of` and no
  trailer, then the generated notes from the last release tag, marked
  as the latest. It prints the URL of the CI run of the tag, and says
  that the deploy waits for the approval of `production` by a person.
  When GitHub refuses the push of the tag, it says so, names the
  ruleset of the tags, and deletes the local tag. Superseded in part
  by 01M58KMM1BHX3YY81BPZG04JSX and 01M58KMKZ2EG8NZG4F085S6NQX.
- **01M58B0JEYNAW67NS226FMX5XH** `riff release status` shows, for each
  session: the version on `origin/main`, the last published release
  tag, a merged release pull request whose version has no published
  release, and each CI run that waits for the environment `production`.
- **01M58B0JMFQ5E40TWTQ48MPSGK** `riff release approve vX.Y.Z` finds
  the CI run of the tag that waits for the environment `production`,
  shows the tag, the commit and the run, asks yes or no, and on yes
  approves it with the comment `Release vX.Y.Z`. With no waiting run,
  it says so and changes nothing. Superseded in part by
  01M58KMM3NFGJRW4CCV50KJQB2.
- **01M58B0JSQMWNCMSNX19536XT8** `riff release publish` refuses, and
  changes nothing, in a worker, in a container session, and in an
  agent session that is not the lead of its user, also when riff
  cannot ask riff-server, and for a member, in a terminal or in its
  lead: a user that is not the owner and not an admin of the riff. In a
  terminal it also refuses when riff cannot ask riff-server for the
  role. A riff with no sign-in has no roles, so there each person and
  each lead may. `riff release approve` refuses
  in each agent session: a worker, a container session, a process with
  a session ID, and a process under Claude Code (`CLAUDECODE`), also a
  `!` command.
  The words "a person only approves" are superseded in part by
  01M594XQJMXAXY3JXP66Y8EVQ7.
  The words "in a container session" for `publish` are superseded by
  01M59269HHZ5C82DR3RG3QHAM3.
  The lead skill makes each release with these commands, and has no
  tag line and no `gh release` line.
- **01M59269HHZ5C82DR3RG3QHAM3** `riff release publish`, `check`,
  `retry` and `rollback` allow the lead of the owner or of an admin
  in a container (`riff lead start`). The daemon tells the lead from
  a work session by the socket of the session: only the session with
  a terminal is the lead, and a work session cannot forge that. A
  work session in a container is still refused. The container of the
  lead has `RIFF_USER`, so the riff commands find their user without
  a keyring, and `RIFF_CHECK` names a folder that it can write for
  `riff build`.
- **01M5ND0ZNFFAPT1AWD3DP1Y77N** A release pull request needs no model
  verify. `riff release verify N` checks that the pull request has the
  label `release`, that its diff is only `Cargo.toml` and `Cargo.lock`,
  that its body has the format of `riff release cut`, and that the Gate
  of its head commit passed. Then it sets the status `riff/verify`. It
  refuses else, and sets nothing.
- **01M5NCYW06FJ8P5X629VFK1BNR** One message of the owner or of an admin
  in riff chat approves a whole release: "approved", "approve 4.0.1" or
  "approve v4.0.1", from a person and not a session, after the line
  `riff release vX.Y.Z is cut` that `riff release cut` writes in riff
  chat. A message with no version counts for the version of the last
  release line of a session before it. With it, the lead runs the
  real-session test, `riff release publish`, `riff release approve` and
  `riff release check`. `riff release approve` of the lead accepts this
  message instead of the publish line, and replaces
  01M594XQJMXAXY3JXP66Y8EVQ7 in part. A message with `not`, `never` or
  `don't` does not approve. A message of the owner or of an admin with
  `stop` or `wait` after the approval cancels it, and the lead stops
  before the next step.
- **01M5R13DK401K3TZ91E2QK2DYM** The refusal of `riff release verify`
  for the full check of the head commit names the commit, the checks of
  the full check and their state, as the line of `riff verify pass`
  does. The full check is the same as for 01M5QYDE41FVFX26EC8K5QGYH4.
  Supersedes 01M5ND0ZNFFAPT1AWD3DP1Y77N in part.

### The release cycle

- **01M58KMKR33P1FSCFDQRZ56A8P** Each wave with a change that a person
  sees since the last release tag ends with one release. Each step of
  the cycle is a riff command: `riff release cut`, `publish`,
  `status`, `approve`, `check`, `retry` and `rollback`, `just
  real-session`, and `riff wave end`. No session skips a release: only
  a person skips one.
- **01M58KMKTE2XC35CNJBFKZWAXF** `riff release status` shows the state
  of the current release: the open release pull request, else the
  version on `origin/main`. The states are `cut`, `merged`, `tested`,
  `published`, `waiting`, `deploying`, `deployed`, `checked`, `failed`
  and `rolled back`. A closed release issue, or a passed check on it,
  is `checked`. A rollback on it is `rolled back`.
- **01M58KMKWRKSV1DR42FRH2K43X** Each command of the release cycle
  prints the next step in a line `next: …`, also in a refusal.
- **01M58KMKZ2EG8NZG4F085S6NQX** `just real-session` sets the commit
  status `riff/real-session` of the commit `HEAD` of its clone:
  `success` after a pass, `failure` after a failed step, none after a
  signal. `riff release publish` refuses while no commit from the merge
  commit of the release pull request to `origin/main` has a pass, and
  tags the newest commit with a pass.
- **01M58KMM1BHX3YY81BPZG04JSX** `riff release publish` does not need a
  paused riff. After the release, it posts a chat line to the owner in
  `riff chat`: the version, the CI run, and `riff release approve
  vX.Y.Z`.
- **01M58KMM3NFGJRW4CCV50KJQB2** HUMAN GATE 1: on yes, `riff release
  approve` pauses the whole riff, which tells each other lead, and then
  approves the deploy. While a deploy waits for the approval,
  `riff release cut` refuses, the riff is not paused, and the watch of
  the lead reminds the owner in `riff chat` each 4 hours.
- **01M58KMM5ZPAQ289S2YJVQ5WW6** `riff release check vX.Y.Z` refuses
  while the job `Deploy` of the newest CI run of the tag did not end.
  After a pass of the job, it checks: `riff server` shows the build; the
  Cloud Run log after the start of the job has no run of 503 longer
  than 20 s, no 500 of riff-server, no 401 or 429 of the token store,
  and no line "riff-server serves" of an older revision after the new
  one; each host of the part `Hosts` of `riff who` has the build. It
  writes the result on the release issue. When each check passes, it
  closes the release issue and resumes the whole riff. After a failed
  job, it files an issue with the failed jobs and the run in the
  current wave, writes it on the release issue, and resumes the riff.
- **01M58KMM8CV9731AWGVZQBB80D** `riff release retry vX.Y.Z` runs the
  failed jobs of the newest CI run of the tag again. It refuses a run
  that did not fail.
- **01M58KMMAPJ33CQWR8GRX62GMR** HUMAN GATE 2: `riff release rollback
  vPREV` starts the CI workflow on `main` with the input `tag=vPREV`,
  and writes `riff release rollback vPREV` on the release issue of the
  newest release. It refuses a tag that is not a published release
  older than the newest one. The run by hand has the name
  `Deploy vPREV`, so `riff release approve vPREV` and `riff release
  check vPREV` find it.
- **01M58KMMCZCE7HDNVT9PVHVZX2** In a repository with
  `deploy/release-check.sh`, `riff wave end N` refuses while a merged
  pull request of the wave, other than a release pull request, is in a
  release tag with no checked release, and names the next step. It
  refuses while one is in no release tag, unless `--no-release TEXT`
  is given: then riff writes "No release: TEXT" in the description of
  the wave.
- **01M58KMMF9WMH39ZMD0MKGRVZB** `riff init` writes the permission
  rules of the lead for `riff release cut`, `publish`, `status`,
  `check`, `retry` and `rollback`, and no rule that names `approve`.
  `publish`, `check`, `retry` and `rollback` refuse a worker, a
  container session, an agent session that is not the lead, and a
  member, as `publish` does (01M58B0JSQMWNCMSNX19536XT8).
- **01M58KMMHH70JEMBX3EYT9H3WM** The watch of the lead looks at the
  release cycle each 5 minutes, in a clone with
  `deploy/release-check.sh`. Each new state of the current release
  wakes the lead with the state and the next step. The first look
  after a start only learns the state.
- **01M58KMMKZ8T11KT3R3Z7D0RE1** HUMAN GATE 3: `riff release cut
  --skip` refuses in each agent session, and for a member.
- **01M58KMMP9FKRFYX58ESVRSZQX** When riff cannot read the Cloud Run
  log or ask the shared riff, `riff release check` names each check
  that it could not do, says `gcloud auth login` for the log, and
  changes nothing but the comment on the release issue.
- **01M58Q03ZBVVXAQ2WZ8EJ5FWS4** The order in a wave: merge each item,
  make a release, deploy the release to the shared server, stop the
  workers, update each machine, start the sessions again, run the
  checks after the release, close each item. The work goes on while the
  deploy waits for a person. The lead stops the workers after the
  deploy, before the update of each machine.
- **01M5GSDJ3YWANWEZNZ6VSDC1BE** `riff release check` counts a host as
  one that has the build when the version of the build of its daemon is
  the version of the release. The commit and the time of the build do
  not count.
- **01M5GVPND32X3R9MYXYB5SV83W** When the container of the lead has no
  `gcloud`, each check of `riff release check` that reads the Cloud Run
  log names the step for a person on the host: `riff release check
  vX.Y.Z` in a terminal.
- **01M5MQHDMBYG187T0Q622XWBMN** The check "no 500 of riff-server" of
  riff release check counts only the 500 replies of the newest revision:
  the revision of the last line "riff-server serves" after the deploy.
  An instance of an older revision that starts in the switch to the new
  one stops before it serves, and its 500 replies do not count. With no
  line "riff-server serves", each 500 reply counts.
- **01M5ND5B14MXGGK0RJ7R2BREW6** In the container of the lead, riff
  release check reads the Cloud Run log through the riff daemon of the
  host, with the gcloud and the sign-in of the person on the host. The
  container gets no Google credential. The daemon refuses a worker. When
  the sign-in of gcloud ended, each check of the log says that a person
  on the host runs gcloud auth login, and the daemon tells the lead in
  chat.

### The real-session test

- **01M4ZCFGCY301G2ARFMRXV7XDR** `just real-session` is the
  real-session test (01M4Z60B8Q8YH6DRSZWPV42CQ0). It makes a scratch
  issue in the backlog that adds one line to the book, and holds
  `verify-issue-N`. The `riff` of the tree runs the issue with
  `riff run` and the image that `riff image` of the tree names. The test
  passes when `riff run` ends with 0, the home check passes
  (01M4ZCFGHCS7WJ6CFYBZSX0AT5), the branch of the item has an open pull
  request that adds the line, and the check run `Gate` of its head
  starts in 10 minutes. Then the test closes the pull request, deletes
  its branch, cancels each CI run of the branch that did not end, closes
  the issue as not planned, frees the hold and removes the folder of the
  session. It cleans up also after a failed step, and after SIGINT or
  SIGTERM.
- **01M5157V34H7FWFH6S2DRQXRMP** Step 4 of the real-session test keeps
  the verify of the scratch item from the workers, also when a worker
  runs the test. It makes the label `no-verify` with
  `gh label create --force`, and puts it on the scratch issue
  (01M5157V0P92EBHF4M7N9K0RGR). The step fails when `gh` cannot. Then
  it holds `verify-issue-N`. A failed hold is a warning.
- **01M4ZCFGF6QN6KZEVMBNW9G4EW** The first step of the real-session
  test that fails stops it. Its message names the step by its number
  and its name, and gives the reason. The exit code is 1, or 130 after
  a signal.
- **01M504HBQ3Y0123HWGFP99Z6NV** A child of a step of the real-session
  test that ends by SIGINT or SIGTERM counts as a signal to the test:
  the step fails with `a signal stopped the test`, the test cleans up,
  and the exit code is 130. Only a second signal to the test itself
  ends it with no clean-up.
- **01M4ZCFGHCS7WJ6CFYBZSX0AT5** The home check of the real-session
  test lists each path of the home before and after `riff run`, on the
  file system of the home, with no link followed. A new path fails the
  test. A new folder shows as one path. The check leaves out the folders
  of riff (`RIFF_HOME`, and `riff` in each XDG base folder),
  `containers` in `XDG_DATA_HOME` and in `XDG_CONFIG_HOME`, the main
  clone and the worktree of the test, `XDG_CACHE_HOME`, and each folder
  of `--skip`.
  In the Claude folder of the person, only a new path that names the
  `ID8` of the session counts. Superseded in part by
  01M51K051FR0WN71NJXNA86XWY, 01M51K053WS0KK3E376V6DH6GR and
  01M51X6JA3W2CMV1S5VZEY7FC3.
- **01M51K051FR0WN71NJXNA86XWY** The home check of the real-session
  test also sees a change to a file that is there already, but only in
  the files that riff must not write (01M4Z60B85X62PNYSVJWKXEWH3):
  `~/.gitconfig`, `git` and `gh` in `XDG_CONFIG_HOME`, the shell files
  (`.profile`, `.bashrc`, `.bash_profile`, `.bash_login`,
  `.bash_logout`, `.zshenv`, `.zprofile`, `.zshrc`, `.zlogin`,
  `.zlogout`, and `fish/config.fish` and `fish/conf.d` in
  `XDG_CONFIG_HOME`), `.claude.json`, and `settings.json` in the Claude
  folder. Before `riff run`, it keeps the size, the time of change and
  the inode of each of these files. A file with a new value fails the
  test, and the message names it. A change of another file of the home
  does not count. Each path of `--skip` is left out, also a file.
  Superseded in part by 01M51X6JA3W2CMV1S5VZEY7FC3.
- **01M51K053WS0KK3E376V6DH6GR** In the home check, the Claude folder of
  the person and each file whose name starts with `.claude.json` beside
  it, for example its lock, count as the Claude folder: only a new path
  that names the `ID8` of the session counts. A changed `.claude.json`
  counts only when its text names the `ID8`.
- **01M51MQB46AYY39YSAY18NMD8T** The home check of the real-session
  test also leaves out, with no flag, `snap` in the home and
  `claude/locks` in `XDG_STATE_HOME`. The snap apps and each `claude`
  on the host write there while the test runs. `--skip` stays for the
  other programs.
- **01M51SPS50QQ9Y5C6VE26A18FZ** The home check of the real-session
  test also leaves out, with no flag, each path in a folder `.git`
  except `hooks`, `config` and `config.worktree`, and the folder
  `.claude/worktrees` of each git clone of the home (a folder with
  `.git` in it). The other sessions of the person build and commit in
  their worktrees, and make new worktrees, while the test runs. A new
  path in the rest of each clone still counts, also a new hook and a
  new config file of git.
- **01M51X6JA3W2CMV1S5VZEY7FC3** The home check of the real-session
  test also reads `config` and `config.worktree` in the folder `.git`
  of each git clone of the home, before and after `riff run`. This
  includes the main clone and the worktree of the test: the check
  leaves out the rest of them. A new, a changed or a removed key fails
  the test, and the message names the file and each key. A key of a
  section `[branch "NAME"]` does not count. A change to a file in
  `.git/hooks` fails the test too. `config.worktree` in
  `.git/worktrees/NAME`, and the git folder of each submodule in
  `.git/modules`, stay left out.
- **01M51X6JCHZB8HJ8C8M3J1G6GZ** When the home is itself a git clone,
  the home check of the real-session test leaves out
  `.claude/worktrees` in the home, as in each other clone.
- **01M4ZFPYNGFF49WWHR8EBWSTNG** Before `riff run`, the real-session
  test starts `riff daemon` of the tree in a process group of its own,
  and waits up to 10 seconds until `riff daemon status` answers. When a
  riff daemon answers before the start, the step fails, and the test
  does not stop that daemon. The clean-up stops the daemon of the test
  with SIGTERM, and kills it when it does not end in 10 seconds.
  Superseded in part by 01M53YP9DATAD95EZEZ6R75WEF.
- **01M53YP9DATAD95EZEZ6R75WEF** The real-session test runs its riff
  daemon beside the daemon of the machine. Each `riff` that the test
  runs gets `RIFF_DATA` set to `real-session` in the data folder of riff
  (01M53YP97S9BDMFBN0X5G444MY). So the daemon of the test has its own
  socket, its own session folders, and its own key and riff, and keeps
  them for the next run. The daemon of the machine keeps running with
  its peer links. The test does not talk to it, and no step of the test,
  the book or the skill stops, starts or changes it. Step 5 fails only
  when a daemon of the test answers before the start. Superseded in part
  by the re-cut (01M53NGW3FR555TWP8Z8HHRVDJ): the daemon of the test has
  no key and no riff, and the daemon of the machine has no peer links.
- **01M4ZCFGKDKH16KGKR5NBEK3Y4** Before each tag, an admin runs
  `just real-session` on the merge commit of the release pull request,
  on a host with podman. It passes before the tag.
- **01M59235BBXEKPZBRH23C39574** `riff relay watch --follow` in a
  session asks the daemon, on the socket of the session, for each wake
  of the session. It prints one line for each wake, once for each
  message, and never ends. While the daemon is away, it tries again.
  The lead starts it once with the Monitor tool of Claude Code: each
  line wakes the session. The model does not start it again. The
  instructions of the lead say so.

- **01M5924C8BTT6CW05Q0YZJ1GCY** `riff release test vX.Y.Z --host HOST`
  asks the riff daemon of HOST to run the real-session test on the
  newest commit of `origin/main` for a merged release that no pass
  covers. The daemon clones `origin`, detaches at the commit, sets the
  commit status `riff/real-session` to `pending`, replies, and runs
  `just real-session` in the clone. At the end it sets `success` for
  exit 0, else `failure`, also after a signal. It runs one test at a
  time. The daemon acts only on the person and the lead of its user:
  riff refuses a worker, and the daemon refuses each other session. The
  daemon is not the author of any item. `riff release status` shows
  `real-session test: running|passed|failed|none on COMMIT` for a merged
  release. `just real-session` stays for a person. The check of a
  reading that a container cannot do, for example the VmLck reading of
  #1003, is the same mechanism with a name for the check: not built.
- **01M5MR8CM887EFGMKZM5BQA9JJ** The riff daemon runs the real-session
  test of `riff release test --host` with `RIFF_SERVER` set to the
  riff-server of the repository of the request, from `served.json` of
  the daemon. So `riff run` and `riff plan hold` in the test reach the
  riff-server of the repository, also when it is not on the host.
- **01M5MR9FGEBKQWHH7RZ9THAHQB** The home check of the real-session test
  leaves out DATA/gvfs-metadata, for example
  ~/.local/share/gvfs-metadata: gvfsd-metadata of GNOME writes there
  when the desktop touches a file.
- **01M5NBZRJ71MREAJCPNVFYVKCM** While the real-session test of a merged
  release runs, `next:` of `riff release status` and `riff release test`
  says to wait for the test on its commit. After a fail on the newest
  commit, it names the fix. After a pass, it names `riff release
  publish`.
- **01M5NC1CANKNF7JWVVFPX1X340** When the real-session test of `riff
  release test` fails, the riff daemon writes the failed step and the
  last 30 lines of the output of the test, with no secret, as a comment
  on the release issue. It sets the description of the status
  `riff/real-session` to the failed step. So the lead in a container
  reads the reason with no access to the journal of the host. `riff
  release test` gives the tag to the daemon for the issue.
- **01M5ND3M84D8MHQ66M9P4DFNS1** The real-session test uses a
  riff-server of its own, never the shared one. At the end of step 1 it
  starts the riff-server of the tree, from the folder of the riff binary
  of the tree, in memory, on a free loopback port. It gives the address
  as RIFF_SERVER to the daemon of the test and to each riff of the test,
  whatever RIFF_SERVER of the person says. The clean-up stops that
  server last. So a release of a new major line passes while the shared
  riff-server runs the line before.
- **01M5NNQ24F2PX74859CV7SMFBZ** At the end of step 1, the real-session
  test resumes the whole riff on the riff-server of the test with `riff
  resume --riff`: a new riff starts paused and refuses each claim. A
  failed resume fails step 1.
- **01M5QVBTXE8YRY44A1C1VP71QS** The riff daemon shows the real-session
  test that it runs, with its tag, its commit and its minutes. `riff
  daemon status` shows it on a line of its own: "real-session test of
  TAG on COMMIT, for N min". `riff top`, `riff who` and `riff view` show
  it on the line of the host. `Status::busy` counts the test as one, so
  the daemon starts again on a new riff only after the test.

## The UIs

The UIs are the design of #982: one Rust core library, and a child
design for each UI. The book page [Design: the UIs of riff](riff-uis.md)
has the pictures. A requirement of this part holds when the build item
that builds it merges.

- **01M54215XK0QAM58RBRMS8F4ZX** Each UI of riff is Rust: the CLI, the
  TUI, the phone app, the web UI and the desktop GUI. The riff project
  writes no JavaScript, uses no JavaScript framework, writes no UI in
  HTML and CSS, and uses no npm tooling. A Rust app built to
  WebAssembly can run in a browser, when its Rust toolkit makes the
  small HTML, JavaScript or CSS that starts it.
- **01M542160DDTZMMDCZYZZ0Q4WC** The phone app is for Android (#951).
  riff-server stays on Google Cloud Run while the UIs come. A server
  part of a UI runs on Cloud Run, in the Google Cloud project of
  riff-server.
- **01M5421633BKPPMANYRVZ76BVP** Each UI shows the sessions in one view:
  the sessions on riff-server and the container sessions of the
  daemons, in one list. In the TUI, the sessions tab replaces the live
  table of `riff top` and the view of `riff view`, with each of their
  actions, also the attach to a session.
- **01M542165T96VW72HAWFXVD160** The CLI stays complete: it serves
  scripts, agents through the MCP server, and hooks. Each command prints
  plain lines that `grep` can find.
- **01M542168PYP1WH78D3D9DKEK6** Each command that prints data takes
  `--format text` (the default) or `--format json`. serde makes the JSON
  from the types of the data. It has the same facts as the text.
- **01M54216BCJQTQFKK5SD1PRJK9** When the TUI ships, `riff top` prints
  one look of the sessions and ends, as `riff top --once` does. The
  live view is the sessions tab of the TUI. The TUI design picks a
  plain alias of `riff top`, or none.
- **01M54216E2GY49GFV5SNPWTBEB** `riff` with no command starts the TUI
  in a terminal. With no terminal, it prints the help of the CLI.
  `riff --help` stays the help of the CLI, and says that
  `riff` with no command starts the TUI. `riff COMMAND` stays the CLI.
  When the TUI design finds that a bare `riff` makes the CLI awkward,
  the TUI is `riff ui`.
- **01M54216GT8SX6NVMKRGNR2H0T** The TUI is a ratatui app with a tab for
  each area: the sessions, the chat, the waves (the items, the claims,
  the pull requests and the verifies), the machines, the devices, and
  the reports (the usage, the audit and the log of a session). Each
  action of the TUI calls the same code as its CLI command.
- **01M54216KMTGS7KD1R3RY5JKQ0** riff 3.1 adds the crate `riff-client`,
  the core library of each UI. The client of riff-server (the sign-in,
  the tokens, the API calls and the watch stream) and the models of the
  views (the sessions, the threads, the waves and the machines) move
  into it, out of the crate `riff`. `riff-core` stays the model that the
  clients and riff-server share.
- **01M54216PBE319RASGNMY9F0MR** `riff-client` holds no terminal code.
  It builds for Linux, for Android (`aarch64-linux-android`) and for
  WebAssembly (`wasm32-unknown-unknown`). The Gate builds it for each of
  these targets, and fails when it depends on a crate of the terminal,
  for example ratatui, clap or rustyline.
- **01M54216S51X5VBXN3YAERW6J6** Each part of `riff-client` that is not
  the same on each platform is a trait that each UI gives, for example
  the store of the tokens and of the device key. The socket of the riff
  daemon stays in the crate `riff`. A terminal UI gives the container
  sessions of its daemon to the sessions model of `riff-client`.
- **01M56S4M0BSV0KP825XSDNNXNX** A UI gives its platform parts to
  `riff-client` one time, at its start. The store of the tokens and of
  the device key is a store of `keyring-core`. A process that gives no
  platform gets the defaults: no OS keyring, no local folder, and plain
  lines on stderr.
- **01M56S4M2NMATA3XZAMPSRAW8X** The browser step of `riff login` (the
  local port of the OAuth redirect) stays in the crate `riff`.
  `riff-client` swaps the ID token of the provider for a riff sign-in.
- **01M56Y8KAPHJMSK89ZW1PA8B0E** `riff-client` holds the models of the
  views as plain data with serde: the sessions, the threads, the waves
  and the machines. It makes each model from the replies of
  riff-server, and from what the UI gives: the container sessions of
  its riff daemon, and the reads of the forge. A UI only draws a
  model. The plain lines, the colors and the ratatui widgets stay in
  the crate `riff`.
- **01M56Y8KD05JH2J7AMK636WGT7** The sessions model has one entry for
  each session ID: a session of riff-server, a container session of
  the riff daemon of this machine, or both. An entry with both has the
  state of its container. `riff top` gives the model no container
  session, and `riff view` gives it no session of riff-server. Each
  draws its part of the model.
- **01M56Y8KF7EDWHX1JE5RA0XMCW** The machines model holds the numbers
  that a host tells in its status line: the limit, the floor, the
  workers, and the numbers of its machine, its disk and its monitor.
  `riff-client` writes and reads that line. The reads of this machine
  (`/proc`, the disk, the settings and tmux) stay in the crate `riff`.
- **01M56Y8KHD61NXC57V7H546KWW** The threads model has one line for
  each message: its number, its time, its sender, its receivers with
  the mark of the whole thread, the mark `verified`, the lead mark of a
  verified lead, its kind and its body. `riff read`, `riff tail` and
  `riff chat` draw these lines.
- **01M54216VVY6072F2X2PM2Z0DQ** The CLI, the TUI, the MCP server and
  each graphical UI use `riff-client` for each call to riff-server.
- **01M54216YKW1X70KDV4TQCQJTE** The graphical UIs (the phone app, the
  web UI and the desktop GUI) use Slint, and share their screens. Each
  child design checks that Slint does its platform today.
- **01M5421718PRVCDJP42J8X5J3E** The web UI runs on Cloud Run, at one
  address with the API of riff-server, so the browser needs no CORS.
  The start is a Cloud Run service of its own for the web app, behind a
  router (for example the rewrites of Firebase Hosting, or a load
  balancer with a URL map), so a deploy of the UI does not restart
  riff-server. The web design compares it with a riff-server that serves
  the web app itself, and picks one.
- **01M542179DFVKMFQ008TV9DM8C** Each UI that signs in is a device of
  the person, with the device commands of the phone design
  (01M53XPZTG203SNM9MBKBERMB5): `riff device list`, `riff device add`
  and `riff device remove`. The
  devices tab of the TUI shows each device with its kind, its name, its
  scope and its last use. It adds a
  device, and draws the QR code with `tui-qrcode`. It removes a device.
  Each graphical UI has a devices view that lists and removes devices,
  and adds a device where it can show a QR code.
- **01M54217C256KCGWSVAVPHEQQB** The UIs come in this order: the TUI,
  the phone app, the web UI and the desktop GUI. After riff 3.1, the TUI
  and the phone app can be built at the same time: the TUI does not
  block the phone app.
- **01M54217ET3G7R4ZJ33CQRE5P9** riff 3.0 (#953) comes first, with the
  patch releases that make it stable. riff 3.1 comes after a stable
  3.0. It moves the core library out of
  the CLI in small steps. Each step keeps each test and changes no
  command. The UIs come after riff 3.1.
- **01M54217HF6JK1KYK4Q11551KR** Each UI has a child design issue that
  links #982: the TUI (#994), the phone app (#951), the web UI (#995)
  and the desktop GUI (#996). The build items of a UI come from its
  child design, after Mike signs it off.
- **01M594XQJMXAXY3JXP66Y8EVQ7** `riff release approve vX.Y.Z` also
  works for the lead of the owner, also in a container, when
  riff-server holds a verified message of the owner or of an admin in
  the thread `chat`, from a person and not a session, that comes after
  the publish line of `riff release publish vX.Y.Z`, names the
  version, and says `approve` or `deploy`. Code reads the thread from
  riff-server. The lead asks no yes or no. The approval comment names
  the message number. It refuses a message of a session, of another
  version, before the publish, or of a member, and a worker, a work
  session in a container, and a session that is not the lead. In a
  plain terminal it works as before. When the auto mode checker blocks
  a release command, `riff denials` has a record, and the lead files an
  issue for it.

## The notes of the lead

- **01M59258K3TYR09HC4AWHWZPEX** riff-server keeps the notes of the lead
  for each user and each repository thread: one text, with the record
  `notes_set` (`thread`, `user`, `text`) and the command `notes`
  (`POST /v1/notes/set`). A new text replaces the old text. Each change
  is a record of the log, and the server keeps each old text in the
  history, also in the checkpoint. The same text makes no record. The
  text has 64 KiB at most. A longer text gets `bad_request` that says
  the limit.
- **01M59258NTXPMWV18TKRKWNTYN** The user of the lead and the owner
  of the riff read the notes (`POST /v1/notes`, with the history when
  asked). Another member gets `not_allowed`. The person of the user and
  a session of the user that is not a worker write the notes of the
  user. A worker gets `not_allowed`.
- **01M59258RJ3CG498Z8AVQ5TV5Q** riff refuses a notes text that looks
  like a token (a known prefix such as `sk-ant-` or `ghp_`, a private
  key, or a long run of letters and digits with both). The refusal does
  not repeat the text.
- **01M59258VCNV9AMGMQTDH936SK** `riff lead notes` shows the notes of
  the person in the repository thread, with `--history` the old texts,
  with `--user` the notes of another user (owner only), and with
  `--thread` another repository thread. `riff lead notes edit` opens
  `$VISUAL`, `$EDITOR` or `vi` on the notes, and `riff lead notes edit
  --file FILE` takes the new text from a file (`-` is the standard
  input).
- **01M59258Y62VSAHDRRY7765RZJ** The MCP tool `notes` reads the notes
  of the lead with no `text`, and replaces them with a `text`. It is in
  `LEAD_TOOLS`, and not in `CONTAINER_TOOLS`: only the lead has notes.
  The instructions of the lead name each tool of `LEAD_TOOLS`, and a
  test checks it.
- **01M592590XREH99ABQF2D2KT6H** The start text of the lead sends it to
  the notes and to the unread messages at each start, after each resume
  and after each compaction. It says to keep the running state in the
  notes and on GitHub, never only in the Claude memory of the host.
- **01M592593NSC1DE7A653Q8PCPS** The instructions of the lead
  (`text::MCP_LEAD`) hold each rule of the lead (`text::LEAD_RULES`).
  No rule stays only in the skill or in the Claude memory of one
  host, and a test checks each rule. A rule for each agent of the
  repository goes in `CLAUDE.md` or here. A choice of one person or one
  host, and the running state, go in the notes of the lead.
- **01M596DH64SQ5AJ4Y6BETGTPSK** `riff lead move HOST` moves the lead
  of the repository to HOST with no other step by the person. It asks
  the old lead to write its handover to the notes, and waits at most 5
  minutes. A lead that writes none in time gets a warning, and the move
  goes on. It asks the daemon of HOST for the new lead, and then sends
  the old lead a direct message that it is not the lead any more. It
  prints where the person attaches. A session that is not the lead
  refuses it, and so does a HOST with no daemon, and the old lead stays
  the lead then. The lead itself writes its handover first, because it
  cannot answer its own message.
- **01M596DH8G4A5XQFHJFBX1X379** The request `lead move` to the daemon
  of a host (`remote::Request::LeadMove`) starts the lead of the
  repository there, as `riff lead start` does and with no attach, when
  none runs. Its first prompt sends the lead to the handover in the
  notes (`text::lead_moved`). The daemon then makes the new session the
  lead (R177) and replies with its session ID. Only the person and the
  lead of the person act on it.
- **01M5A5HXKZTDP5M5HY2FHCEBZA** A session of a wave that ends with no
  open pull request leaves one record as a note in the thread `ends` on
  riff-server (`ends::post`). The first line has the tags `item`,
  `session`, `code`, `failed` and `denied`, then a colon and the reason
  in words. The lines after it are the last 50 lines of its output, each
  with no secret and at most 300 characters. The session leaves the
  thread after the post. A session with an open pull request leaves no
  record.
- **01M5A5HXP9X3AXTS6N3A57DMQM** `riff session why ITEM` reads the
  thread `ends` and shows the last record of ITEM, and how many records
  it has. It works in a container: the lead reads the cause there, with
  no look at the host. An item with no record says so.
- **01M5A5HXRH4PFQDFA7XPFFRHWS** The message "started N times" to the
  lead names the reason of the last end of the item, and sends the lead
  to `riff session why ITEM`.

## Open

None.
