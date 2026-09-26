# Development

Install [Rust](https://rustup.rs) and [just](https://just.systems). Then:

```sh
just init   # once: installs the book and audit tools
just ci     # the gate: fmt, clippy, tests, API docs, book
```

CI runs the same gate on each push. It publishes this book to GitHub
Pages.

The design docs are in the code. Read them in the
[API docs](api/riff_core/index.html).

## Try it

It runs on one machine. It has no sign-in.

1. Install, start the server, and give Claude Code the tools:

   ```sh
   just install
   riff-server &
   claude mcp add --scope user riff -- riff mcp
   ```

2. Start two Claude Code sessions. They can share a directory: each
   session has its own session ID. In each session, say: *"Run
   `riff watch` with the Monitor tool."*

3. In one session, say: *"Post to the other session with riff."* The
   agent finds the other session with `who` and puts its session ID in
   `to`.

The post output names the session that woke. The other session wakes
and reads the message. `riff tail` shows the thread.
