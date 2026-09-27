# Plan

Four thin slices. Each slice ends with something we can try.

| Slice | Delivers | Done when |
|---|---|---|
| 1. Local loop (built) | `riff-server` on one machine: memory only, no sign-in. `riff mcp`, `riff watch`, `riff who`, `riff post`, `riff tail`, `riff claim`, `riff release`. | Two Claude Code sessions in two worktrees talk, and a mention wakes one of them. |
| 2. Plugin (built) | Session IDs, selectors, `move`, `riff tell`, `riff read`, `riff connect claude`. | One command gives a session its tools and its wake-up. A new session in the main worktree finds its own work. |
| 3. Sign-in (built) | Riff tokens, the keyring, `riff login`, `riff logout`, OAuth metadata, allowed domains, device keys, session tokens. | Only a Como account can connect. |
| 4. Cloud | Cloud Run at `riff.comotechnologies.io`, the state saved in Cloud Storage, one instance by a lease, `just cloud-setup`, `just deploy`. | Brett joins from a second machine with three commands. |

## Slice 4 steps

1. Save and load the state through the store interface. Test with the
   in-memory store. (R30, R31, R34, R124–R129)
2. Add the Cloud Storage store and the lease. (R86, R137–R142)
3. Change `riff`: the default server, connect again, try again on 503.
   (R131–R133)
4. Build the image, make the cloud resources, and deploy. (R5, R6, R29,
   R32, R46, R130, R134–R136)
5. Brett joins from a second machine.

## Not yet

These requirements wait for a later slice:

| Requirement | Slice |
|---|---|
| R5, R6, R29–R32, R34, R46, R86, R124–R142: Cloud Run and storage | 4 |
