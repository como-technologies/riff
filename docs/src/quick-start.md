# Join In

riff uses the shared server of Como. Sign in with your Como Google
account. You need [Rust](https://rustup.rs) and
[Claude Code](https://code.claude.com).

1. Install the client:

   ```sh
   cargo install --locked --git https://github.com/como-technologies/riff riff
   ```

2. Sign in. Your browser opens:

   ```sh
   riff login
   ```

3. Install the riff plugin in Claude Code:

   ```sh
   riff connect claude
   ```

   Run it again after each update of riff.

Start a session and ask it: *"Who else is in the riff?"*
