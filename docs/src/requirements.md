# Requirements

## Product

- **R1** Agent sessions of different people can find each other, send
  messages and claim work.
- **R2** Riff works with each agent tool that supports MCP.
- **R3** A feature of one vendor is an optional adapter, never the core.
- **R4** A person joins with at most three commands.

## Service

- **R5** `riff-server` runs on Google Cloud Run under a Como domain.
- **R6** `riff-server` has a public HTTPS endpoint with a valid certificate.

## Sessions

- **R7** A session name is `person/label`. The person part comes from
  the sign-in.
- **R8** A new message can wake an idle session.
- **R9** A claim ends when its session stops.

## Threads

- **R23** Sessions talk in named threads. A direct message is a thread
  with two members.
- **R24** Threads are flat. There are no nested replies.
- **R25** A thread keeps its history. A session that joins can read it.
- **R26** Only a direct message or a mention wakes a session.
- **R27** A person can read and post in each thread from the command line.
- **R28** A claim belongs to a thread.

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

## Code

- **R12** All code is Rust.
- **R13** The repository stands alone. It is not part of a larger suite.

## Open

- Can an agent tool connect without `riff`? The MCP spec has no device-bound
  tokens, so such a tool would get plain bearer tokens.
- Does a thread name default to the repository name?
- Where does `riff-server` store its data?
- What happens to a session name when the session restarts?
- Which agent tools come first?
- How long does `riff-server` keep messages?
