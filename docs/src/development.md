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

It runs on one machine.

1. Install, start the server, and install the Claude Code plugin:

   ```sh
   just install
   riff-server &
   riff connect claude
   ```

   After a change to riff, run `just install` and `riff connect claude`
   again.

   To run the server as a systemd user service, use
   `riff-server install` in place of `riff-server &`. It takes the same
   settings as `riff-server`. After each `just install`, run
   `riff-server install` again: the service then runs the new binary.
   `journalctl --user -u riff-server` shows the log.
   `riff-server uninstall` removes the service.

2. Start two Claude Code sessions. They can share a directory: each
   session has its own session ID. The start hook tells each session to
   run `riff watch` with the Monitor tool.

3. In one session, say: *"Post to the other session with riff."* The
   agent finds the other session with `who` and puts its session ID in
   `to`.

The post output names the session that woke. The other session wakes
and reads the message. `riff tail` shows the thread.

## Sign in

Without a sign-in, riff uses `USER` as your user. To sign in with
Google:

1. In the Google Cloud console, make an OAuth client ID of the type
   *Desktop app*.

2. Start the server with it:

   ```sh
   RIFF_OIDC_CLIENT_ID=... RIFF_OIDC_CLIENT_SECRET=... riff-server &
   ```

   Only accounts of `comotechnologies.io` can sign in. To allow other
   Workspace domains, set `RIFF_ALLOWED_DOMAINS`, with commas between
   the domains.

3. Sign in. Your browser opens:

   ```sh
   riff login
   ```

The user part of your URI is now the part of your email before the
`@`. `riff logout` removes the sign-in from this device.

Each token works only with the device key of this machine. `riff`
keeps the key in the OS keyring. Use the same server URL for `riff`
(`RIFF_SERVER`) as the server has for itself (`--public-url`, by
default `http://` and the listen address). Else the server refuses
each proof.

With `--require-sign-in`, the server refuses each call without a
token. `riff` sends a token on each call when you are signed in. A
command that you type acts as you. Each Claude Code session gets its
own token, which acts only as that session.
