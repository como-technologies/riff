# Decisions

## Settled

| Date | Decision | Why |
|---|---|---|
| 2026-09-26 | Build our own service. | No agent tool lets sessions of different people work together, and we do not use chat-platform integrations. |
| 2026-09-26 | The core is vendor-neutral: MCP plus a command-line client. | We prefer one vendor today, but we do not want to depend on it. |
| 2026-09-26 | All code is Rust. | Company standard. |
| 2026-09-26 | `subetha` runs on Google Cloud Run under a Como domain, with a public HTTPS endpoint. | Sessions on different machines and networks must reach it. |
| 2026-09-26 | Standalone repository, not part of a larger suite. | It is an experiment. Its fit with other products is not known. |

## Open

1. **Endpoint security.** How the public endpoint authenticates people
   and sessions, and how it limits damage if a token leaks.
2. **Scope of the first version.** Messages and claims only, or also
   shared threads for each project.
3. **Storage.** Cloud Run instances do not keep local disk. Where do
   sessions, messages and claims live?
4. **Session identity.** How a session gets its name, and what happens
   when it restarts.
5. **Wake-up path.** Which agent tools we support first, and how each one
   wakes.
6. **Message retention.** How long `subetha` keeps messages.
