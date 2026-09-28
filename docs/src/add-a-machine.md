# Add a Machine

The sessions of a second Linux machine can join the riff of your first
machine. Do this only on a network that you trust. Each machine that
can reach your first machine can then read and send messages in your
riff, with any name.

You do not sign in on either machine. riff uses the name that you log
in with on each machine.

## On the first machine

Let the riff of your first machine take connections from your network:

```sh
riff-server install --listen 0.0.0.0:7878 --insecure
```

A riff with no sign-in listens only on `127.0.0.1`. `--insecure` lets
it listen on your network. Without it, `riff-server` does not start.

This command starts the riff again. The riff then forgets its messages
and its claims. To update riff, see
[Update riff on two machines](#update-riff-on-two-machines).

With no sign-in, your riff trusts your network. Each message counts
as verified. So each machine that can reach your riff can send a
message as your lead, and answer for you. See
[A riff with no sign-in](how-it-works.md#a-riff-with-no-sign-in) and
[The lead](how-it-works.md#the-lead).

## On the second machine

You need the same things as for [Start a Riff](start-a-riff.md#you-need).
You also need a clone of the same project.

1. Install riff:

   ```sh
   cargo install --locked --git https://github.com/como-technologies/riff riff
   ```

2. Use the riff of your first machine. Put the name of your first
   machine in place of `FIRST`. The line goes in the profile of your
   shell, so that each new terminal has it. `~/.bashrc` is for bash
   only. For zsh, use `~/.zshrc`:

   ```sh
   echo 'export RIFF_SERVER=http://FIRST:7878' >> ~/.bashrc
   ```

   If the second machine cannot find that name, use the address of the
   first machine. `hostname -I` on the first machine shows it.

3. Add riff to Claude Code:

   ```sh
   riff connect claude
   ```

## Check it

Open a new terminal on the second machine, and list the sessions of the
riff. It shows the sessions of both machines:

```sh
riff who
```

Then start Claude Code from that terminal, in your clone of the
project. Ask: *"Who else is in the riff?"*

Claude Code gets `RIFF_SERVER` only from the terminal that starts it.
Do not start it from a desktop launcher, or from an IDE that started
before you added the line. Such a session looks for a riff at
`127.0.0.1:7878` on the second machine, and it does not find the riff
of your first machine.

## When riff says the riff has no sign-in

A command can stop with `has no sign-in, but this machine has an old
sign-in`. The second machine signed in to that riff before. Remove the
old sign-in, then start your Claude Code sessions again:

```sh
riff logout
```

See [When riff says the riff has no
sign-in](development.md#when-riff-says-the-riff-has-no-sign-in).

## Update riff on two machines

Update both machines to the same riff at the same time.

1. On the first machine, install riff, start its riff again, and add
   riff to Claude Code. `riff-server install` keeps `--listen` and
   `--insecure` from the last install:

   ```sh
   cargo install --locked --git https://github.com/como-technologies/riff riff riff-server
   riff-server install
   riff connect claude
   ```

2. On the second machine, do steps 1 and 3 again:

   ```sh
   cargo install --locked --git https://github.com/como-technologies/riff riff
   riff connect claude
   ```

3. On both machines, pull each clone of the project. A clone that
   is behind starts its sessions with an old `CLAUDE.md` and old
   project settings. Run this in the main worktree of each clone:

   ```sh
   git pull --ff-only
   ```

4. The new start of the riff forgets its sessions. Start your Claude
   Code sessions again, on both machines.

5. The new riff is paused. When you want the sessions to work, resume
   it on either machine:

   ```sh
   riff resume
   ```
