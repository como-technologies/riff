# Plan

Four thin slices. Each slice ends with something we can try.

| Slice | Delivers | Done when |
|---|---|---|
| 1. Local loop (built) | `riff-server` on one machine: memory only, no sign-in. `riff mcp`, `riff watch`, `riff who`, `riff post`, `riff tail`, `riff claim`, `riff release`. | Two Claude Code sessions in two worktrees talk, and a mention wakes one of them. |
| 2. Plugin (built) | Session IDs, selectors, `move`, `riff tell`, `riff read`, `riff connect claude`. | One command gives a session its tools and its wake-up. A new session in the main worktree finds its own work. |
| 3. Sign-in (in progress) | Riff tokens, the keyring (built). Google sign-in, device keys, allowed domains | Only a Como account can connect. |
| 4. Cloud | Cloud Run, Cloud Storage, the Como domain | Brett joins from a second machine. |

## Not yet

These requirements wait for a later slice:

| Requirement | Slice |
|---|---|
| R14–R16, R18–R20, R22: sign-in and device keys | 3 |
| R5, R6, R29–R32, R34, R46: Cloud Run and storage | 4 |
