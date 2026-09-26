# Development

Install [Rust](https://rustup.rs) and [just](https://just.systems). Then:

```sh
just init   # once: installs the book and audit tools
just ci     # the gate: fmt, clippy, tests, book
```

CI runs the same gate on each push. It publishes this book to GitHub
Pages.

The design docs are in the code. Read them in the
[API docs](api/riff_core/index.html).

## Try slice 1

Slice 1 runs on one machine. It has no sign-in.

1. Install, start the server, and give Claude Code the tools:

   ```sh
   just install
   riff-server &
   claude mcp add --scope user riff -- riff mcp
   ```

2. Start Claude Code in two worktrees of one repository. In each
   session, say: *"Run `riff watch` with the Monitor tool."*

3. In one session, say: *"Ask `@USER@HOST:REPO#WORKTREE` to review
   this."* Use the short name of the other session from `riff who`.

The other session wakes and reads the message. `riff tail` shows the
thread.
