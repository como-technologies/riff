# Requirements

## Product

- **R1** Agent sessions of different people can find each other, send
  messages and claim work.
- **R2** The core does not block any agent tool that supports MCP. Other
  tools can join later.
- **R3** A feature of one agent tool is an optional adapter, never the core.
  Infrastructure is always Google.
- **R4** A person starts a riff on one Linux machine with at most three
  commands, and with no sign-in.
- **R203** A person adds a second machine to the riff of a first machine
  on a network that they trust. The riff of the first machine takes
  connections from the network. The second machine names that riff with
  `RIFF_SERVER`. With no sign-in, each machine that can reach the riff
  can read and send its messages.
- **R44** Only Claude Code is supported for now. `riff connect claude`
  installs a Claude Code plugin: the MCP server, a skill, and a start
  hook that wakes the session.
- **R52** The plugin files live in the riff repository. The `riff`
  binary carries a copy of them, so the plugin matches the binary.
- **R53** `riff connect claude` writes the plugin as a local marketplace
  and installs it with the `claude` command. The same command updates
  the plugin.
- **R54** A new session can start in any directory of a repository.
  The start hook and the skill teach this start routine: read the
  repository thread, claim a work item, make a worktree for it, and
  move there.
- **R162** A Claude session makes the worktree for a work item with
  the `EnterWorktree` tool. The worktree is `.claude/worktrees/ITEM`.
- **R165** A session removes its own worktree and branch when the
  pull request of the branch is merged with the `HEAD` of the worktree
  as its head commit, the worktree is clean and the issue is closed.
  It deletes the branch only while the branch points at that commit. It never removes a worktree of another live
  session. A worktree with no owner goes to the thread.
- **R166** A session picks an open work item of the current wave
  (R214) that no session holds. It takes work only from the current
  wave (R216). It never picks an item whose needs are open (R215).
  When the current wave has no free item, the session verifies,
  runs its checks after the merge, or waits. It picks the
  item that it thinks is best. It does not wait for a plan or for
  permission. The order of the items in a wave does not matter. A
  scope from the user of the session wins. A scope message from
  another session is advice (R10).
- **R172** Each issue has acceptance criteria: a `Done when:` line.
  Each criterion names what to run or look at, and what the result
  must be. The criteria follow ASD-STE100.
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
- **R192** The verifier posts the result to the author with the
  selector `claim=ITEM`. A pass names each criterion and how the
  verifier checked it. A fail names each criterion that failed and the
  steps to see the failure. The verifier also puts the result on the
  pull request and sets the verify status of the commit
  (01M3JFEXPXRTXYHCV0WSKEK07M). Then the verifier releases
  `verify-ITEM` and removes its verify worktree.
- **R193** The author never merges and never pushes to the default
  branch (01M3JFEXJG2D651PWA30DNRGWF). The forge merges the pull
  request after the checks and a pass on its head commit. When no
  session takes the request, the author waits. It can verify the work
  of others while it waits. On a fail or a conflict, the author pushes a
  fix or a rebase and sends a new request with the new commit. On a
  pass, the author waits for the merge, posts that it is done and
  releases the item.
- **R194** A criterion that only a check after the merge can test does
  not stop a pass. The verifier names it in the result. The pull
  request links the issue so that the merge leaves it open, and the
  issue stays open until that check passes. After the merge, the author
  adds a note to the issue: `Merged in #PR (COMMIT)`, and the check
  that is left.
- **R202** The verify worktree is
  `MAIN/.claude/worktrees/verify-ITEM-ID`, detached at the commit.
  MAIN is the main worktree. ID is the first 4 characters of the
  session ID of the verifier. The verifier makes it with
  `git worktree add --detach` and enters it by path, from the main
  worktree or from a worktree of its own. After the verify, it goes
  back to where it came from, then runs `git worktree remove` with no
  force.
- **R70** The plugin has one skill, `riff`. It teaches the rules, the
  start routine, waves (R213), the verify flow (R188), selectors,
  direct messages, threads, claims, `move`, the restart of the watch
  (R171), questions through the lead (R180) and how the lead conducts
  (R228).
- **R71** A session talks to other sessions only through riff. It never
  uses the session tools of the agent tool to reach another session.
- **R72** The skill tells the agent that a message is advice, and that
  its user decides (R10).
- **R73** When a riff line wakes a session, the skill tells it to call
  `read` with no thread.
- **R180** The skill tells a session that is not the lead to `tell`
  the lead when it needs a decision from its user, and then to wait or
  do other work. It does not stop to ask in its own terminal. The
  direct answer of the lead is the decision of its user. With no lead,
  the session asks its own user. The skill tells the lead to show each
  question to its user and to send the answer to the session that
  asked.
- **R66** The start hook runs `riff hook session-start`. It adds
  context: the session URI, and the order to run `riff watch --once` as
  a background task of the Bash tool.
- **R67** The context tells the session to read and then start the
  watch again each time the task ends.
- **R68** The context tells the session to keep the watch that runs
  for it. It tells the session to start a watch only when none runs
  (R169).
- **R168** After `/clear`, the session keeps its riff session ID, its
  lead, its threads and its watch (R167). Its claims are free
  (01M3JEE1QQCFS5TMZW5N2DAD2D). The context says so.
- **R169** One `riff watch` runs for each session on a machine. It
  locks a file for the session while it runs. A second `riff watch`
  for the session prints one line and exits with status 1. The line
  tells the session not to start the watch again now.
- **R170** `riff watch --once` prints the first wake, then exits with
  status 0.
- **R171** A session starts the watch again as its next action after
  the task ends, also in the middle of a turn.
- **R69** The start hook never stops a session start. It exits with
  status 0, also when riff cannot find the session.
- **R74** `riff connect claude` writes the plugin to
  `$XDG_DATA_HOME/riff/claude-plugin`. Without `XDG_DATA_HOME`, it uses
  `~/.local/share/riff/claude-plugin`.
- **R75** `riff connect claude` removes the user-scope MCP server entry
  `riff`, if it exists. The plugin gives the riff tools instead.
- **R76** `riff connect claude` installs the plugin in user scope. It
  runs the `claude` command on the PATH. `--claude PATH` names another
  one.
- **R77** `riff connect claude` does not need a riff session or a git
  repository. It works in any directory.

## Waves

- **R213** A wave is a numbered group of work items: Wave 1, Wave 2,
  and so on. The waves run in number order. The items of one wave run
  at the same time.
- **R214** The current wave is the open wave with the lowest number.
  The next wave is the open wave after it. When a repository has no
  waves, each open item is in the current wave. An item that the lead
  keeps out of the waves is not free work.
- **R215** An item names the items that it needs in a `Needs:` line.
  An item is merged when it is closed, or when it has the note
  `Merged in #PR (COMMIT)` (R194). An item is closed when it is merged and
  each check after the merge passed.
- **R216** A wave is done when each of its items is closed. The order
  in a wave: merge each item, update each machine when a check after
  the merge needs it, run the checks after the merge, close each item.
  Then the lead ends the wave. No session starts an item of the next
  wave before the current wave is done.
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
  tells its user to update each machine when a check needs it, and
  tells the sessions to run their checks after the merge. When each
  item is closed, the lead ends the wave.
- **R222** The skill, the requirements and the book name the concept
  of waves first. Each keeps the form of a forge in one part of its
  own. A new forge needs no change to the concept. Outside these
  parts, no text names the objects of a forge that hold a wave.

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
- **R225** The skill gives the `gh` command for each step of the
  waves: see the open waves, see the items of a wave, see the items
  with no wave, make a wave, put an item in a wave and end a wave.

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
  pull request (`milestone-trailer`).
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
- **01M3JFEXJG2D651PWA30DNRGWF** No session pushes to `main` or runs
  `gh pr merge --admin`. The project settings of Claude Code deny both,
  and the skill says it.
- **01M3JN4QVR3JCRWC8TFJJTCPHF** Each session uses the GitHub account
  of our user, so the author of a pull request can set `riff/verify`
  on its own commit. We accept this gap until the sessions act with
  their own GitHub identity (#97).
- **01M3JFEXMPNFEV4HBZJQ15JD25** The author opens a pull request with
  `gh pr create`, with the milestone of the issue and the body form of
  the hygiene check, and turns on auto-merge with
  `gh pr merge --auto --squash` at once, before any other push. No
  session runs `gh pr merge` after a push. The last pull request of an
  issue has `Closes #N`. Each other one, and one with a check after the
  merge left, has `Refs #N`.
- **01M3JFEXPXRTXYHCV0WSKEK07M** The verifier puts its result on the
  pull request as a comment that names the commit. It sets the commit
  status `riff/verify` on that commit: `success` on a pass, `failure`
  on a fail. A new commit has no status, so it needs a new verify.
- **01M3JFEXS6M5549TC6MH0G2MS0** The workflow `Hygiene` also runs on
  each push to `main`. It checks the commit message with the commit
  rule of the hygiene check.

## Pause

- **01M3JCFTWCR72HQB8CBTQKXJNF** A riff is paused or running. The
  state is one for each `riff-server`. The server saves it, so it stays
  when the sessions and the server restart. A new riff starts paused.
  A saved state from before this rule loads as paused.
- **01M3JCG3T8AJZN31SZQQTP3FAF** Only a person (a call with no session
  ID, for example `riff pause` in a shell) or a lead can pause or
  resume the riff. A call from another session fails.
- **01M3JCG3WBHDF0ZWM06XV94ZDC** While the riff is paused, a claim
  fails. The sessions keep the claims that they hold. A release still
  works.
- **01M3JCG3YD7C2Y3V0QJPF082YH** A pause or a resume that changes the
  state wakes each session of the riff that is not gone: the client
  posts to the thread of each repository of such a session, to that
  repository.
- **01M3JCG40FN0DP135EHHF403TY** While the riff is paused, a new
  session says hello to the lead, sets its status to waiting, and
  claims nothing.
- **01M3JCG42FYS8FJ0V6WK89KXAP** While the riff is paused, a session
  with work stops at its next step. A command that runs finishes
  first. The session commits each change as a WIP commit on the branch
  of its worktree, pushes that branch, sets its status, and waits.
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
  tells the session to call `whoami`.
- **01M3JCG4AV80MHFP73CWDY5E3M** `riff who`, `riff whoami` and the
  `whoami` tool show the state of the riff.

## Service

- **R118** `riff-server` with no command runs in the foreground, in a
  terminal.
- **R119** `riff-server install` installs a systemd user service and
  starts it. The unit runs the binary that ran `install`. It restarts
  the server after a crash and starts it at login. The settings of
  `install` go in a file next to the unit, with mode 0600.
- **R120** `riff-server install` again writes the unit and the
  settings again, and restarts the service.
- **01M3JCE5477135XSD740DG7KFT** `riff-server install` keeps each old
  setting that it does not get again. A setting that it gets, as an
  option or as a `RIFF_*` variable, replaces the old one. So a plain
  `riff-server install` keeps `--listen` and `--insecure`.
- **R121** `riff-server uninstall` stops and disables the service, and
  removes the unit and the settings.
- **R122** When `systemctl --user` does not work, `install` and
  `uninstall` fail and change nothing.
- **R33** `riff-server` rejects a token that it does not know. A lost token
  record means the person signs in again.

## Saved state

- **R30** `riff-server` keeps its state in memory. With `--bucket NAME`
  (`RIFF_BUCKET`), it saves the state to that Cloud Storage bucket. It
  loads the state at start.
- **R34** Storage is behind one interface. Tests use an in-memory store.
  Without `--bucket`, `riff-server` saves nothing.
- **R124** The bucket holds one object for each thread, with its members
  and its messages. One object holds the sessions, with their places,
  read cursors, claims and leads. One object holds the token store.
- **R31** A restart loses only the open streams, the proof IDs and the
  changes that were not saved. A lost message is acceptable.
- **R125** After a load, each session counts as stopped at the time of
  the load. Its claims end after the grace period (R9), unless it comes
  back.
- **R154** A load drops each claim whose holder had stopped more than
  the grace period (R9) before the last save. That claim had ended.
- **R126** At load, `riff-server` drops each session with no sign of
  life for 30 days. A session that was gone at the save stays gone.
- **R127** `riff-server` saves each changed object at most once each
  second.
- **R128** `riff-server` replies to a call that changes the token store
  only after it saved the change.
- **R150** When that save fails, `riff-server` replies 503. The next
  save tries the change again.
- **R129** On SIGTERM, `riff-server` replies 503 to each new call,
  saves each unsaved change, then exits. Ctrl-C does the same.
- **R46** A lifecycle rule of the bucket deletes each thread object 30
  days after its last change.
- **R147** The name of each thread object starts with `threads/`. The
  sessions, the token store and the lease have names outside
  `threads/`.
- **R149** When the lifecycle rule (R46) deleted a thread object, the
  next save of that thread saves it again as a new object. It is not a
  failed save (R141).

## Cloud

- **R5** `riff-server` runs on Google Cloud Run. It runs with
  `--require-sign-in`. For now, its public URL is its Cloud Run URL.
  The book never names that URL.
- **R181** For now, no shared server runs, and `CLOUD_DEPLOY` is not
  set. Each person runs `riff-server` on their own machine. The shared
  server runs again when the open issues are done.
- **R6** Later, its public URL is `https://riff.comotechnologies.io`.
  Cloud Run then maps the domain to the service. Google manages the
  certificate.
- **R29** Only one instance of `riff-server` serves at a time. An
  instance is one running `riff-server` process. Cloud Run keeps one
  instance, with its CPU on also between calls. During a deploy, a
  second instance runs for a short time. The lease (R137) stops one of
  them.
- **R130** An instance takes up to 1000 calls at a time. Each open
  stream is one call.
- **R131** Cloud Run ends each call after 60 minutes. `riff watch` and
  `riff tail` then connect again.
- **R132** `riff` tries a call again while the server replies 503, for
  up to 60 seconds.
- **R148** `riff watch` and `riff tail` connect again at once when a
  stream ends. When a connect fails, they try again every 5 seconds.
  They stop only when the person stops them.
- **R133** `riff` uses `http://127.0.0.1:7878`, the server on the same
  machine, when no server is set. `--server` or `RIFF_SERVER` names
  another server, for example the shared server (R5).
- **R32** The OIDC client secret is in Secret Manager. Cloud Run gives
  it to `riff-server` as `RIFF_OIDC_CLIENT_SECRET`.
- **R134** `riff-server` runs as its own service account. The account
  can read and write only its bucket, and read only its secret. The
  bucket is private.
- **R135** The image holds only the `riff-server` binary and CA
  certificates. It runs as a user that is not root.
- **R151** Cloud Build builds the image as its own service account.
  That account can only build and store images.
- **R160** CI deploys `riff-server` when the repository variable
  `CLOUD_DEPLOY` is `true`. Then each push to `main` that changes the
  server builds the image in CI, pushes it to the image repository of
  the project, and deploys it, after the gate passes.
- **R161** CI signs in to Google Cloud with the OIDC token of GitHub.
  No key exists. Only the `main` branch of the repository can sign in.
  The deploy account can push images, deploy the service, and run it
  as `riff-server`.
- **R152** Cloud Run lets each caller in. `riff-server` checks each
  token itself (R5).
- **R143** The Google Cloud project `como-riff` holds each cloud
  resource of riff. It holds nothing else.
- **R144** `deploy/cloud.env` holds the cloud settings and the OAuth
  client ID. The repository is public. No file in it holds the account
  data of a real person: an email address, a billing account ID or an
  organization ID.
- **R136** A person makes the project and links its billing account
  with gcloud, by the how-to in the book. `just cloud setup` makes the
  resources of riff in the project. It checks each resource first, so
  it can run again. `just cloud deploy` builds the image and deploys it to
  Cloud Run.
- **R145** A person makes the OAuth client by hand in the console, with
  the how-to in the book. `just cloud oauth-client` puts the client secret in
  Secret Manager and the client ID in `deploy/cloud.env`. The secret is
  never in the repository or in a downloaded file.

## One instance

- **R137** The lease is an object in the bucket. It holds the ID of the
  instance that may serve.
- **R138** At start, an instance makes a random ID and writes it to the
  lease. It then waits 15 seconds, loads the state, and starts to serve.
- **R139** An instance reads the lease every 2 seconds. It serves only
  for 5 seconds after the last read that showed its own ID. Else it
  replies 503.
- **R140** An instance that reads another ID in the lease stops for
  good. It closes each stream, replies 503 to each call and saves
  nothing more. It exits after 60 seconds.
- **R141** Each save names the version of the object that the instance
  knows. When the bucket holds another version, the save fails. The
  instance then stops as in R140.
- **R155** An instance saves only in the time that R139 gives it to
  serve.
- **R156** A new instance starts to serve at a whole second. When the
  lease shows another ID after its wait, it exits and does not serve.

## Sessions

- **R8** A new message can wake an idle session.
- **R9** A claim ends 5 minutes after the last sign of life of its
  session (R204), unless the same process comes back first, for
  example after a short network fault. A session that waits for its
  user keeps its claims. A claim ends at once when its session ends
  (R205) or starts again (01M3JEE1QQCFS5TMZW5N2DAD2D).
- **01M3JEE1QQCFS5TMZW5N2DAD2D** A new start of a session is blank: a new
  agent process, a resume or a `/clear`. The start hook sends a start
  call, and the claims of the session are free at once. The session
  keeps its ID (R58, R167, R168), its threads, its read cursors and its
  lead. A compaction is not a new start.
- **01M3JEE1SWR05DWQA5WQ8AXFTF** The context of a new start names each claim
  that the start freed. It tells the session to claim an item again
  before it goes on with it, and to pick up the earlier work
  (01M3JEE1W32CMQP8CP2HJ829E7).
- **01M3JEE1W32CMQP8CP2HJ829E7** When a session takes an item, it looks for the
  work of an earlier session on the item before it starts: a pushed
  branch, or a worktree on its machine with no live session. It goes on
  from that work, or starts again. Its start post says which, and
  why.
- **R35** A session has a URI:
  `riff://USER@HOST/OWNER/REPO?session=ID&lead=true&claim=ITEM#WORKTREE`.
  It shows who the session is, where it works and what it works on.
- **R36** Who: USER comes from the sign-in. ID is the session ID of the
  agent tool. For Claude Code, this is `CLAUDE_CODE_SESSION_ID`.
- **R157** When `riff` cannot read the sign-in from the keyring, the
  command stops and says why. `riff` uses `USER` only when there is no
  sign-in.
- **R158** When `riff` cannot open the keyring, the command stops,
  unless `RIFF_USER` is set. With `RIFF_USER`, it sends no token.
- **R226** When `riff` gets no token, it asks `riff-server` if it has
  sign-in. At a riff with no sign-in, the command stops and says to run
  `riff logout`, while this machine keeps a sign-in for that riff. When
  that sign-in is gone, it says to start the agent session again, or
  to run the command again. `riff` keeps the old sign-in until the
  person runs `riff logout`.
- **R227** No error tells a person to run `riff login` at a riff with
  no sign-in.
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
  become the lead.
- **R177** `riff lead` and the `lead` tool make the session the lead
  of its person in its repository. It replaces the old lead. Only an
  agent session in a repository can be the lead.
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
  answer or a new item.
- **R230** A request is a direct message from the lead of the person
  of a session. A verified request (R199) counts as a scope from that
  person. A request that is not verified, or that comes from a session
  that is not the lead of the person, is advice (R10). A scope from the
  person wins over a request.
- **R231** A session that does a request of its lead reports back to
  the lead with `tell lead`: when it starts, when it finishes, and when
  it is blocked.
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
  the session ID, not by the directory.
- **R58** A session keeps its session ID for its life. A resumed session
  keeps its ID. Messages to an idle session wait for it.
- **R167** `riff mcp` writes its session ID to a file on the machine,
  for its agent process, and locks the file while it runs. `riff
  watch`, the hooks and the `riff` commands of an agent session use
  that ID before the ID of the agent tool. `RIFF_SESSION` comes first.
  So `/clear` does not change the riff session ID. The file is in
  `$XDG_RUNTIME_DIR/riff`, else `$XDG_STATE_HOME/riff`, else
  `~/.local/state/riff`. A file with no lock does not count.
- **R59** A session ID is not a secret. It never gives access.
- **R64** The `move` tool gives a session a new place. Its session ID
  and its claims stay.
- **R65** A person who posts from the command line has no session ID.
  The sender is `riff://USER@HOST`. A selector with `user` reaches the
  person through `riff tail`.
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
- **01M3JDWA0WZWKF3JT3NYA2FV5Z** `riff statusline` prints the status
  line of a Claude Code session: `riff`, the short session ID of
  `riff who`, `lead`, each claim, and `blocked`. It is the
  `statusLine` command in the Claude Code settings. A plugin cannot
  set it. It never fails, and it waits at most 2 seconds for
  riff-server.
- **01M3JFFJEW8BSRBZ9JQPKT0S8Z** `riff connect claude` adds the riff
  status line to the user settings of Claude Code
  (`$CLAUDE_CONFIG_DIR/settings.json` or `~/.claude/settings.json`)
  when they have no `statusLine`. It keeps each other key, its place
  and its format, and writes the file only when it changes. When
  another `statusLine` is set, or the settings are not a JSON object,
  it changes nothing and names the manual how-to.
- **R41** A session joins the thread `OWNER/REPO` by default.
- **R42** A cloud session uses the host `cloud`.
- **R100** A session is a cloud session when `CLAUDE_CODE_REMOTE` is
  `true`. `RIFF_HOST` still wins.
- **R43** Outside git, the URI is `riff://USER@HOST/-?session=ID#DIRECTORY`.
- **R49** When a watch starts, it wakes the session once if an addressed
  message is unread.
- **R163** `riff-server` records the time of each call of a session.
  `who` is a call too. A keep-alive (R204) is not a call. `who` shows
  each session as `live`, or with the time since its last call, for
  example `idle 2m`.
- **R164** A gone session (R206) is not in `who`. `who --all` lists
  it. The server keeps its record until R126 drops it, so a resumed
  session keeps its ID.
- **R204** `riff mcp` sends a keep-alive to `riff-server` each 60
  seconds while it runs, also while no turn runs.
- **R205** When `riff mcp` stops (its stdin closes, or it gets SIGTERM,
  SIGINT or SIGHUP), it sends an end call for its session. The
  `SessionEnd` hook `riff hook session-end` sends the same call, except
  for the reason `clear` (R168).
- **R206** A session is gone when it ended (R205), or when the server
  got no call, no keep-alive and no watch from it for 3 minutes. A gone
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
- **R182** A session sets its status with `riff status` or the `status`
  tool. A status is the current step of the session, and a reason when
  the session is blocked (`--blocked REASON`). A new status replaces the
  old one.
- **R183** A status is one line. The step is not empty. The step and
  the reason each have at most 200 characters. `riff-server` refuses a
  status that breaks a rule, with status 400.
- **R184** `riff-server` keeps the last status of each session, with the
  time that the session set it. It saves the status with the session.
  `who` shows each status and its age, for example
  `status 4m ago: write the tests`. A blocked status starts with
  `blocked`.

## Threads

- **R23** Sessions talk in named threads. A direct message is a thread
  with two members.
- **R24** Threads are flat. There are no nested replies.
- **R25** A thread keeps its history. A session that joins can read it.
- **R26** Only an address wakes a session. Text in a message body
  never wakes a session.
- **R27** A person can read and post in each thread from the command line.
- **R28** A claim belongs to a thread.
- **R48** A person can claim and release work from the command line.
  `riff claim` exits with status 1 when another session holds the item.
- **R50** A session lists and reads by default only the threads that it
  joined. It can read any other thread by name.
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
- **R79** `riff read` shows the unread messages of the threads of the
  person. It joins the person to the thread of the directory first.
  `--thread` reads one thread. `--all` shows the full history.
- **R185** A post has a kind: `message` (the default) or `status`. A
  post of kind `status` is a status request. It wakes the sessions that
  its selectors match, as each post does. A person sends one with
  `riff post --kind status --to FIELD=VALUE`. A status request needs no
  body.
- **R186** The wake line and `read` show that a message is a status
  request. A session that a status request wakes answers with `status`.
  It does not post a reply.
- **01M3JPK82PN4F706MCHDH771MW** `read` does not give a session its own
  posts, and the unread counts of `threads` leave them out. `read` with
  `all` gives them.
- **01M3JPK85FT5CCQPF3WDCXSMDF** Each message in `read` shows a short
  sender: `USER@HOST:REPO#WORKTREE`, the first 8 characters of its
  session ID, and `lead=true` for a verified lead. A post to each session of
  the repository of its thread shows `to all`. `who` gives the full
  URI.
- **01M3JPK885GPD16FPK7D05R2RC** `tell` takes a session ID, the start
  of a session ID as `read` shows it, a full URI, or `lead`. A start
  that fits more than one session in `who` is an error.
- **R187** The skill tells a session to set its status when it claims,
  when it changes step, when it is blocked, and when it releases.

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
- **R11** Messages do not carry secrets.
- **R195** Each message carries a signature from the device key of
  its sender (R18). A message from the command line (R65) carries the
  signature of the device key of the person.
- **R196** The signature covers the user, the session ID and the lead
  mark of the sender, the thread, the `to` selectors, the body, the
  kind and the time. It does not cover the place and the claims of the
  sender. A direct message signs no thread. Its one selector must match
  the other session of its thread.
- **R197** With sign-in (R85), `riff-server` refuses a post with
  status 403 when it has no signature, when the signature is not
  valid, when the key is not the key of the token, or when the signed
  time is more than 5 minutes old or more than 10 seconds in the
  future.
- **R198** `riff-server` keeps the signature with the message, also in
  storage. The time of a signed message is its signed time. The sender
  of a signed message has `lead=true` only when the signature covers
  it. `riff-server` refuses a signed post with the lead mark from a
  session that is not the lead. A signed-in `riff` asks for its lead
  mark before it signs.
- **R199** The reader verifies each message before it shows it. Each
  message shows `verified` or `not verified`. A message is verified
  when its signature is valid for the message as the reader got it,
  and its key is the key of a live sign-in of the user of the sender.
  `read` and `tail` give these keys. A message from a riff with no
  sign-in is verified too (R212).
- **R200** A message that is not verified never counts as from the
  lead. The reader shows its sender without `lead=true`.
- **R201** Without sign-in, `riff-server` keeps no signature and gives
  no keys. So only a riff with no sign-in verifies such a message
  (R211).
- **01M3JEJVXXEPPNGT3FY4ZSFCWZ** `riff-server` refuses a signed post
  whose signature is the signature of a message in the same thread: a
  copy. So a session gets each request of its lead once. With R197, a
  copy older than 5 minutes fails the time check too.
- **R211** A `riff-server` with no sign-in provider and no
  `--require-sign-in` is a riff with no sign-in. It trusts each caller.
  A person runs it only on a network that they trust (R203). Each
  `read` reply and each `tail` event of such a riff says that it
  trusts its callers.
- **R212** The reader counts each message of a riff that trusts its
  callers as verified. The lead mark of such a message is the lead mark
  that the server gives.
- **01M3JCE4ZD4DZCQ21FA69RT52D** A riff with no sign-in (R211) listens
  only on a loopback address. `riff-server` refuses to start on another
  address, unless it gets `--insecure` (`RIFF_INSECURE`). With
  `--insecure` on such an address, it warns at start: each machine that
  can reach it can read, post and answer as any person. `install` does
  the same check. A riff that requires sign-in listens on any address.
- **01M3JCE51T84JZKJ0NR89TPDNY** `riff-server` never terminates TLS. A
  proxy or the platform in front of it does, for example Cloud Run.

## Sign-in and tokens

- **R14** The first sign-in provider is Google.
- **R15** `riff-server` accepts only accounts from its allowed domains. The
  allowed domains are a setting. The default is `comotechnologies.io`.
- **R94** The domain of an account is the `hd` claim of its ID token.
  An account without `hd` is refused.
- **R16** `riff-server` issues its own tokens. It accepts sign-in from each
  OpenID Connect provider in its settings.
- **R17** An access token expires in 10 minutes or less. A refresh token
  changes at each use. A reused refresh token revokes all tokens from
  that sign-in.
- **R110** Only the device key of a sign-in can revoke it by reuse. A
  reused refresh token with another key is refused and changes nothing.
- **R116** `riff-server` keeps a used refresh token for 24 hours, to
  find reuse. After that, the token is not known.
- **R80** A sign-in ends when none of its refresh tokens is used for
  30 days. The person then signs in again.
- **R81** `riff-server` keeps only a hash of each token, never the
  token itself.
- **R90** `riff login` signs in with the provider that `riff-server`
  names. It opens the browser. The code comes back to a loopback port.
  The client uses PKCE with S256.
- **R112** Only a request with the right `state` ends the wait on the
  loopback port. It reads at most 8 KiB of the request line.
- **R91** `riff-server` swaps a valid ID token of its provider for the
  first riff tokens. The email in the ID token must be verified.
- **R117** Each fetch from the sign-in provider stops after 10 seconds.
- **R146** At start, `riff-server` checks its OAuth client with the
  provider. When the provider refuses the client, `riff-server` stops.
- **R153** When `riff-server` cannot reach the provider at start, it
  logs a warning and serves.
- **R208** USER is the part of the verified email before the `@`, in
  lower case. Each character that a URI part cannot hold becomes `-`.
- **R209** A USER belongs to one verified email. The first email that
  signs in with a USER holds it. `riff-server` refuses each other email
  that gives the same USER. The refusal names the USER.
- **R93** `riff logout` removes the sign-in at one server from the
  device.
- **R18** Each token is bound to a key that stays on the device.
- **R86** Tokens use DPoP (RFC 9449) with ES256. Each request with a
  token carries a new proof from the device key. A proof is valid for
  5 minutes, and up to 10 seconds in the future.
- **R87** `riff-server` refuses a bearer token, a proof that it saw
  before, and a proof for another method or URL. The URL is the public
  URL of the server and the path.
- **R113** `riff-server` refuses a proof whose `jwk` holds a private
  key.
- **R114** `riff-server` keeps at most 100,000 proof IDs. When it must
  forget an ID early, it refuses each proof as old as that one.
- **R115** `riff-server` keeps a proof ID only after the request shows a
  valid token or ID token. A caller without one adds nothing.
- **R142** At start, `riff-server` knows no proof ID. It refuses each
  proof issued before it started to serve.
- **R88** `riff` keeps one device key for each server, in the OS
  keyring.
- **R19** Each session gets its own token. The token works only for that
  session.
- **R103** A session gets its token by token exchange (RFC 8693): it
  sends a person access token and its session ID. The session token
  has its own refresh token. A refresh keeps the session.
- **R104** A token acts only as its caller. The user and the session ID
  in `me` must match the token, or `riff-server` replies 403. A person
  token acts only as the person, with no session ID.
- **R105** Only a person access token gives a session token.
- **R106** `riff` keeps a session token only in the memory of the
  process. Only the person tokens go to the keyring.
- **R107** Only one `riff` process at a time refreshes the person
  tokens of one server. A lock file makes the others wait.
- **R20** A person or an admin can revoke all tokens of a person at once.
- **R101** `riff logout --all` ends each sign-in of the caller, on each
  device. An admin adds `--user USER` to end the sign-ins of another
  person.
- **R210** The admins are a setting of `riff-server`: `--admin EMAIL`
  or `RIFF_ADMINS`. Each admin is named by verified email. A name that
  is not an email names nobody, and the server logs a warning at start.
  There are no admins by default.
- **R111** Admin emails, user names and the named person compare
  trimmed and in lower case.
- **R21** A client keeps tokens and keys only in the OS keyring.
- **R82** `riff` keeps each secret under the keyring service `riff`.
  On Linux, it needs a Secret Service, for example GNOME Keyring or
  KWallet.
- **R22** `riff-server` follows the MCP authorization spec, revision
  2026-07-28.
- **R83** `riff-server` has no authorization endpoint for now. Its
  metadata lists no response type.
- **R84** The public URL of `riff-server` is a setting. It is the OAuth
  resource and the issuer. Each token is only for it.
- **R85** With the setting `--require-sign-in`, each route except the
  token endpoint, the sign-in route and the metadata needs a live
  access token in the `Authorization` header.
- **R47** Each session connects through `riff`. Direct connections from an
  agent tool are not supported for now.

## Terminal

- **01M3JD390F49HZSKEJ3VACX0ZA** When the lead runs in tmux, `riff mcp`
  of the lead adds a pane with `riff tail` of the repository thread
  beside it. It marks the pane. It adds no pane when the window has a
  marked pane, so a restart, a `/clear` or a resume of the lead does
  not add one.
- **01M3JD392Q5ANX0FPZ51W7B0E3** `riff workers start N` starts N
  workers in the tmux window `riff-workers`, one pane each. Each pane
  runs `claude "Join the riff."` in the main worktree, with
  `RIFF_WORKER=1`. A second start adds panes to the same window. No
  person types a key.
- **01M3JD394YFA3TQRE3E72ZER4Z** A worker starts with no Remote
  Control. The book starts the lead with `claude --remote-control`.
- **01M3JD3973J7A9BG8G9EP9TVDP** Outside tmux, `riff workers start`
  says that it needs tmux, starts nothing and exits with status 1.
- **01M3JD399ABBWE3DJT5BVXAFH5** tmux is one terminal backend. Its
  parts are in one module behind one interface, so that a later
  backend does the same.
- **01M3JD39BASN1GNJTZXXKBCNZ9** Each pane that riff makes gets the
  riff-server URL of the command that makes it, in `RIFF_SERVER`.

## Builds

- **01M3JEE7KZR5VVJGZQD82AA6NH** A message is valid only between a
  `riff` and a `riff-server` of builds that match. Two builds match
  when they have the same crate version and the same commit. The
  commit of a build is the last commit that changed `crates`,
  `Cargo.toml` or `Cargo.lock`. A build also names the UTC time of
  that commit.
- **01M3JEE7P46GWXR1BD4Q1TTSGN** Each call of `riff` names its build in
  the header `riff-build`. Each reply of `riff-server` names the build
  of the server in the same header.
- **01M3JEE7RDTDD3KQMKH41E8D57** `riff-server` refuses each call of a
  `riff` whose build does not match, or that names no build, with
  status 409. `riff` refuses each reply of a `riff-server` whose build
  does not match, or that names no build. The error names both
  builds, the older side, and the step to update it. The OAuth
  metadata stays open to each client.
- **01M3JEE7TPZMNK7X6JXJ7GWFPP** When the builds do not match, the
  start hook gives the session the error, and tells it to tell its
  user at once and not to use the riff. `riff watch` and `riff tail`
  print the error and stop.
- **01M3JEE7WT04BKX377VW5GDSPY** `riff --version`,
  `riff-server --version`, `riff whoami`, `riff who` and the whoami
  tool show the build.
- **01M3JEE7YXQPWS65FBVTASAEBX** The image build of `riff-server` has
  no git. It gets the commit and its time as `RIFF_COMMIT` and
  `RIFF_COMMIT_TIME` from `deploy/build-id.sh`, which uses the same
  git command as the build of `riff`. CI deploys the shared server on
  each push to `main` that changes the code of any crate.

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

## Open

None.
