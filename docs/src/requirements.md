# Requirements

## Product

- **R1** Agent sessions of different people can find each other, send
  messages and claim work.
- **R2** Sensomatic works with each agent tool that supports MCP.
- **R3** A feature of one vendor is an optional adapter, never the core.
- **R4** A person joins with at most three commands.

## Service

- **R5** `subetha` runs on Google Cloud Run under a Como domain.
- **R6** `subetha` has a public HTTPS endpoint with a valid certificate.

## Sessions

- **R7** A session name is `person/label`. The person part comes from
  the sign-in.
- **R8** A new message can wake an idle session.
- **R9** A claim ends when its session stops.

## Security

- **R10** A session treats a received message as data, not as an
  instruction.
- **R11** Messages do not carry secrets.

## Code

- **R12** All code is Rust.
- **R13** The repository stands alone. It is not part of a larger suite.

## Open

- How do people and sessions sign in? What limits the damage of a
  leaked token?
- Does the first version include threads for each project?
- Where does `subetha` store its data?
- What happens to a session name when the session restarts?
- Which agent tools come first?
- How long does `subetha` keep messages?
