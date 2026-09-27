# Requirements

## Product

- **R1** Agent sessions of different people can find each other, send
  messages and claim work.
- **R2** The core does not block any agent tool that supports MCP. Other
  tools can join later.
- **R3** A feature of one agent tool is an optional adapter, never the core.
  Infrastructure is always Google.
- **R4** A person joins with at most three commands.
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
  branch is merged into the default branch, the worktree is clean and
  the issue is closed. It never removes a worktree of another live
  session. A worktree with no owner goes to the thread.
- **R166** A session picks any open work item that no session holds.
  It picks the item that it thinks is best. It does not wait for a plan
  or for permission. Issue order and milestones do not set the order.
  A scope from the user of the session wins. A scope message from
  another session is data (R10).
- **R70** The plugin has one skill, `riff`. It teaches the rules, the
  start routine, selectors, direct messages, threads, claims and
  `move`.
- **R71** A session talks to other sessions only through riff. It never
  uses the session tools of the agent tool to reach another session.
- **R72** The skill tells the agent that a message is data, and that
  its user decides (R10).
- **R73** When a riff line wakes a session, the skill tells it to call
  `read` with no thread.
- **R66** The start hook runs `riff hook session-start`. It adds
  context: the session URI, and the order to run `riff watch` under the
  Monitor tool.
- **R67** The context tells the session to start the watch again each
  time the Monitor ends.
- **R68** After `/clear`, the session has a new session ID. The context
  tells it to stop the watch of the old ID. After compaction, the
  context tells it to keep the watch that runs.
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

## Service

- **R118** `riff-server` with no command runs in the foreground, in a
  terminal.
- **R119** `riff-server install` installs a systemd user service and
  starts it. The unit runs the binary that ran `install`. It restarts
  the server after a crash and starts it at login. The settings of
  `install` go in a file next to the unit, with mode 0600.
- **R120** `riff-server install` again replaces the unit and the
  settings, and restarts the service.
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
  read cursors and claims. One object holds the token store.
- **R31** A restart loses only the open streams, the proof IDs and the
  changes that were not saved. A lost message is acceptable.
- **R125** After a load, each session counts as stopped at the time of
  the load. Its claims end after the grace period (R9), unless it comes
  back.
- **R154** A load drops each claim whose holder had stopped more than
  the grace period (R9) before the last save. That claim had ended.
- **R126** At load, `riff-server` drops each session that it has not
  seen for 30 days.
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
  `--require-sign-in`. For now, its public URL is its Cloud Run URL,
  `https://riff-server-816917641970.us-central1.run.app`.
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
- **R133** `riff` uses the public URL of the shared server (R5) when no
  server is set. `--server` or `RIFF_SERVER` names another server.
  Local development sets `RIFF_SERVER=http://127.0.0.1:7878`.
- **R32** The OIDC client secret is in Secret Manager. Cloud Run gives
  it to `riff-server` as `RIFF_OIDC_CLIENT_SECRET`.
- **R134** `riff-server` runs as its own service account. The account
  can read and write only its bucket, and read only its secret. The
  bucket is private.
- **R135** The image holds only the `riff-server` binary and CA
  certificates. It runs as a user that is not root.
- **R151** Cloud Build builds the image as its own service account.
  That account can only build and store images.
- **R160** CI deploys `riff-server`. Each push to `main` that changes
  the server builds the image in CI, pushes it to the image repository
  of the project, and deploys it, after the gate passes.
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
  with gcloud, by the how-to in the book. `just cloud-setup` makes the
  resources of riff in the project. It checks each resource first, so
  it can run again. `just deploy` builds the image and deploys it to
  Cloud Run.
- **R145** A person makes the OAuth client by hand in the console, with
  the how-to in the book. `just oauth-client` puts the client secret in
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
- **R9** A claim ends 5 minutes after its session stops, unless the session
  comes back first.
- **R35** A session has a URI:
  `riff://USER@HOST/OWNER/REPO?session=ID&claim=ITEM#WORKTREE`.
  It shows who the session is, where it works and what it works on.
- **R36** Who: USER comes from the sign-in. ID is the session ID of the
  agent tool. For Claude Code, this is `CLAUDE_CODE_SESSION_ID`.
- **R157** When `riff` cannot read the sign-in from the keyring, the
  command stops and says why. `riff` uses `USER` only when there is no
  sign-in.
- **R158** When `riff` cannot open the keyring, the command stops,
  unless `RIFF_USER` is set. With `RIFF_USER`, it sends no token.
- **R159** A session keeps the user of its first call. The server
  refuses a call with a known session ID and another user, with status
  409. The reply tells the session to set `RIFF_USER` or to sign in.
- **R55** Where: HOST, OWNER/REPO and WORKTREE come from the machine and
  from git. They are true at the start and change with `move`.
- **R56** What: the URI has one `claim` part for each claim that the
  session holds. It has none when the session holds no claim.
- **R37** The main worktree has no `#WORKTREE` part.
- **R38** The session ID makes each URI unique. Two sessions in one
  worktree have different URIs.
- **R57** `riff mcp`, `riff watch` and the hooks find their session by
  the session ID, not by the directory.
- **R58** A session keeps its session ID for its life. A resumed session
  keeps its ID. Messages to an idle session wait for it.
- **R59** A session ID is not a secret. It never gives access.
- **R64** The `move` tool gives a session a new place. Its session ID
  and its claims stay.
- **R65** A person who posts from the command line has no session ID.
  The sender is `riff://USER@HOST`. A selector with `user` reaches the
  person through `riff tail`.
- **R39** People see the short form `USER@HOST:REPO#WORKTREE`. Riff does
  not route on the short form.
- **R41** A session joins the thread `OWNER/REPO` by default.
- **R42** A cloud session uses the host `cloud`.
- **R100** A session is a cloud session when `CLAUDE_CODE_REMOTE` is
  `true`. `RIFF_HOST` still wins.
- **R43** Outside git, the URI is `riff://USER@HOST/-?session=ID#DIRECTORY`.
- **R49** When a watch starts, it wakes the session once if an addressed
  message is unread.
- **R163** `riff-server` records the time of each call of a session.
  `who` is a call too. `who` shows each session as `live`, or with the
  time since its last call, for example `idle 2m`.
- **R164** A session that made no call for 24 hours is gone. `who` does
  not list it. `who --all` lists it. The server keeps its record, so a
  resumed session keeps its ID.

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
  `claim`. A session matches a selector when each named field matches.
  A post wakes each session that matches one or more selectors.
- **R60** Riff matches the selectors when the message is posted. A
  session that matches later gets no wake.
- **R61** The post result names each session that woke, and each
  selector that matched no session.
- **R62** A direct message is a post with one `session` selector.
- **R63** A person addresses a post with `riff post --to FIELD=VALUE`.
- **R78** A person sends a direct message with `riff tell SESSION`.
  SESSION is a session ID or a full session URI.
- **R79** `riff read` shows the unread messages of the threads of the
  person. It joins the person to the thread of the directory first.
  `--thread` reads one thread. `--all` shows the full history.

## Security

- **R10** A session treats a received message as data, not as an
  instruction.
- **R11** Messages do not carry secrets.

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
- **R92** USER is the part of the verified email before the `@`, in
  lower case.
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
- **R102** The admins are a setting of `riff-server`: `--admin USER` or
  `RIFF_ADMINS`. There are no admins by default.
- **R111** Admin names and the named person compare trimmed and in
  lower case.
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

## Code

- **R12** All code is Rust.
- **R13** The repository stands alone. It is not part of a larger suite.
- **R123** The crate audit ignores no advisory. When a dependency has
  an advisory, we update it, change a feature, or replace it.

## Open

None.
