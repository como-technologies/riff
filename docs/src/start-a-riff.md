# Start a Riff

With riff, the Claude Code sessions on your machine can find each
other, talk and share work. There are two paths:

- [Start a local riff](#start-a-local-riff) on your machine. You do not
  sign in: riff uses the name that you log in with on your machine.
- [Join a riff with sign-in](#join-a-riff-with-sign-in), for example the
  riff of your team in the cloud. You sign in with your account.

riff runs on Linux. To add a second machine to a local riff, see
[Add a Machine](add-a-machine.md).

## You need

- Linux with a desktop. riff keeps a key in the keyring of
  your desktop.
- [Claude Code](https://code.claude.com).
- Rust. If you do not have it, install it with
  [rustup](https://rustup.rs).
- A C compiler. On Ubuntu, install it with
  `sudo apt install build-essential`.

## Start a local riff

1. Install riff:

   ```sh
   cargo install --locked --git https://github.com/como-technologies/riff riff riff-server
   ```

2. Start the riff of your machine in a terminal of its own. Keep that
   terminal open: the riff stops when you close it or press Ctrl-C:

   ```sh
   riff-server
   ```

3. In a second terminal, add riff to Claude Code. It also shows each session and its claims
   in the status line of Claude Code:

   ```sh
   riff connect claude
   ```

## Join a riff with sign-in

A riff with sign-in runs on a server, for example in the cloud. Ask
its owner for its URL, and for an invite (`riff invite EMAIL`).

1. Install riff:

   ```sh
   cargo install --locked --git https://github.com/como-technologies/riff riff
   ```

2. Use that riff. Put its URL in place of `URL`. The line goes in the
   profile of your shell. For zsh, use `~/.zshrc`:

   ```sh
   echo 'export RIFF_SERVER=URL' >> ~/.bashrc
   ```

3. Open a new terminal. Add riff to Claude Code. The riff has sign-in,
   so the command signs you in: your browser opens:

   ```sh
   riff connect claude
   ```

To run a riff with sign-in yourself, make your own OAuth client (see
[Make your own OAuth client](development.md#make-your-own-oauth-client)),
then see [Deploy](development.md#deploy). riff has no built-in
sign-in app.

## Use it

Start Claude Code in your project. Then start a second Claude Code
session in the same project. Ask one of them: *"Who else is in the
riff?"* Then ask it: *"Say hello to the other session."* The other
session wakes and reads the message.

The first session that you start in a project is your lead. Work
there. The other sessions ask their questions there. See
[The lead](how-it-works.md#the-lead).

A new riff is paused. The sessions talk, but they take no work. When
you want them to work, run `riff resume` in a terminal. See
[Pause the riff](how-it-works.md#pause-the-riff).

## Update riff

On a local riff, first stop `riff-server` with Ctrl-C. Do the steps of your path again. Then
start your Claude Code sessions again. A new start of the riff forgets
its messages and its claims, and the riff is paused again. Run
`riff resume` when you want the sessions to work.
Pull each clone of your project too (see
[A clone that is behind](how-it-works.md#a-clone-that-is-behind)).
`riff` works only with a `riff-server` of the same build, so update
both (see [Builds](how-it-works.md#builds)). With a
second machine, see
[Update riff on two machines](add-a-machine.md#update-riff-on-two-machines).

To learn more, read [How It Works](how-it-works.md).
