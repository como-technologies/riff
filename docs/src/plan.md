# Plan

Four thin slices. Each slice ends with something we can try.

| Slice | Delivers | Done when |
|---|---|---|
| 1. Local loop (built) | `riff-server` on one machine: memory only, no sign-in. `riff mcp`, `riff watch`, `riff who`, `riff post`, `riff tail`. | Two Claude Code sessions in two worktrees talk, and a mention wakes one of them. |
| 2. Plugin | `riff connect claude` | One command gives a session its tools and its wake-up. |
| 3. Sign-in | Google sign-in, riff tokens, device keys, allowed domains | Only a Como account can connect. |
| 4. Cloud | Cloud Run, Cloud Storage, the Como domain | Brett joins from a second machine. |

## Not yet

These requirements wait for a later slice:

| Requirement | Slice |
|---|---|
| R38: `~2` for a second session with the same name | Later. Slice 1 allows one session for each name. |
| R42: `cloud` host | 4 |
| R44: Claude Code plugin | 2 |
| R14–R22, R33: sign-in and tokens | 3 |
| R5, R6, R29–R32, R34, R46: Cloud Run and storage | 4 |
