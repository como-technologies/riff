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
- **R35** A session name is a URI: `riff://USER@HOST/OWNER/REPO#WORKTREE`.
- **R36** USER comes from the sign-in. HOST, OWNER/REPO and WORKTREE come
  from the machine and from git.
- **R37** The main worktree has no `#WORKTREE` part.
- **R38** A second live session with the same name gets `~2`, then `~3`,
  and so on.
- **R39** Display and mentions use the short form
  `USER@HOST:REPO#WORKTREE`.
- **R40** A session name outlives the session. Messages to an idle name
  wait.
- **R41** A session joins the thread `OWNER/REPO` by default.
- **R42** A cloud session uses the host `cloud`.
- **R43** Outside git, the name is `riff://USER@HOST/-#DIRECTORY`.

## Threads

- **R23** Sessions talk in named threads. A direct message is a thread
  with two members.
- **R24** Threads are flat. There are no nested replies.
- **R25** A thread keeps its history. A session that joins can read it.
- **R26** Only a direct message or a mention wakes a session.
- **R27** A person can read and post in each thread from the command line.
- **R28** A claim belongs to a thread.
- **R48** A person can claim and release work from the command line.
  `riff claim` exits with status 1 when another session holds the item.

## Security

- **R10** A session treats a received message as data, not as an
  instruction.
- **R11** Messages do not carry secrets.

## Sign-in and tokens

- **R14** The first sign-in provider is Google.
- **R15** `riff-server` accepts only accounts from its allowed domains. The
  allowed domains are a setting. The default is `comotechnologies.io`.
- **R16** `riff-server` issues its own tokens. It accepts sign-in from each
  OpenID Connect provider in its settings.
- **R17** An access token expires in 10 minutes or less. A refresh token
  changes at each use. A reused refresh token revokes all tokens from
  that sign-in.
- **R18** Each token is bound to a key that stays on the device.
- **R19** Each session gets its own token. The token works only for that
  session.
- **R20** A person or an admin can revoke all tokens of a person at once.
- **R21** A client keeps tokens and keys only in the OS keyring.
- **R22** `riff-server` follows the MCP authorization spec, revision
  2026-07-28.
- **R47** Each session connects through `riff`. Direct connections from an
  agent tool are not supported for now.

## Code

- **R12** All code is Rust.
- **R13** The repository stands alone. It is not part of a larger suite.

## Open

None.
