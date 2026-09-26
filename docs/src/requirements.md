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

- **R5** `riff-server` runs on Google Cloud Run under a Como domain.
- **R6** `riff-server` has a public HTTPS endpoint with a valid certificate.
- **R29** Only one `riff-server` instance runs at a time.
- **R30** `riff-server` keeps its state in memory. It saves threads to Cloud
  Storage and loads them at start.
- **R31** A lost message is acceptable. Sessions and claims are not saved.
  After a restart, sessions register again.
- **R32** The token signing key is in Secret Manager.
- **R33** `riff-server` rejects a token that it does not know. A lost token
  record means the person signs in again.
- **R34** Storage is behind one interface. Tests use an in-memory store.
- **R46** A thread with no posts for 30 days is deleted by a Cloud Storage
  lifecycle rule.

## Sessions

- **R8** A new message can wake an idle session.
- **R9** A claim ends 5 minutes after its session stops, unless the session
  comes back first.
- **R35** A session has a URI:
  `riff://USER@HOST/OWNER/REPO?session=ID&claim=ITEM#WORKTREE`.
  It shows who the session is, where it works and what it works on.
- **R36** Who: USER comes from the sign-in. ID is the session ID of the
  agent tool. For Claude Code, this is `CLAUDE_CODE_SESSION_ID`.
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
- **R80** A sign-in ends when none of its refresh tokens is used for
  30 days. The person then signs in again.
- **R81** `riff-server` keeps only a hash of each token, never the
  token itself.
- **R90** `riff login` signs in with the provider that `riff-server`
  names. It opens the browser. The code comes back to a loopback port.
  The client uses PKCE with S256.
- **R91** `riff-server` swaps a valid ID token of its provider for the
  first riff tokens. The email in the ID token must be verified.
- **R92** USER is the part of the verified email before the `@`, in
  lower case.
- **R93** `riff logout` removes the sign-in at one server from the
  device.
- **R18** Each token is bound to a key that stays on the device.
- **R19** Each session gets its own token. The token works only for that
  session.
- **R20** A person or an admin can revoke all tokens of a person at once.
- **R101** `riff logout --all` ends each sign-in of the caller, on each
  device. An admin adds `--user USER` to end the sign-ins of another
  person.
- **R102** The admins are a setting of `riff-server`: `--admin USER` or
  `RIFF_ADMINS`. There are no admins by default.
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
  token endpoint and the metadata needs a live access token in the
  `Authorization` header.
- **R47** Each session connects through `riff`. Direct connections from an
  agent tool are not supported for now.

## Code

- **R12** All code is Rust.
- **R13** The repository stands alone. It is not part of a larger suite.

## Open

None.
