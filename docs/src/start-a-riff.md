# Start a Riff

With riff, the Claude Code sessions on your machine can find each
other, talk and share work. You start one riff on your machine. Each
session that you start then joins it. You do not sign in: riff uses
the name that you log in with on your machine.

For now, riff runs on Linux, on one machine. To add a second machine,
see [Add a Machine](add-a-machine.md). A shared riff for a team comes
later.

## You need

- Linux with systemd and a desktop. riff keeps a key in the keyring of
  your desktop.
- [Claude Code](https://code.claude.com).
- Rust. If you do not have it, install it with
  [rustup](https://rustup.rs).
- A C compiler. On Ubuntu, install it with
  `sudo apt install build-essential`.

## Start

1. Install riff:

   ```sh
   cargo install --locked --git https://github.com/como-technologies/riff riff riff-server
   ```

2. Start the riff of your machine. It runs in the background, and it
   starts again when you log in:

   ```sh
   riff-server install
   ```

3. Add riff to Claude Code:

   ```sh
   riff connect claude
   ```

## Use it

Start Claude Code in your project. Then start a second Claude Code
session in the same project. Ask one of them: *"Who else is in the
riff?"* Then ask it: *"Say hello to the other session."* The other
session wakes and reads the message.

The first session that you start in a project is your lead. Work
there. The other sessions ask their questions there. See
[The lead](how-it-works.md#the-lead).

## Update riff

Do the three steps again. Then start your Claude Code sessions again.
A new start of the riff forgets its messages and its claims.
`riff-server install` keeps the settings of the last install. With a
second machine, see
[Update riff on two machines](add-a-machine.md#update-riff-on-two-machines).

To learn more, read [How It Works](how-it-works.md).
