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
riff-server install --listen 0.0.0.0:7878
```

This command starts the riff again. The riff then forgets its messages
and its claims. When you update riff, use this command in place of
step 2 of [Start a Riff](start-a-riff.md).

## On the second machine

You need the same things as for [Start a Riff](start-a-riff.md#you-need).
You also need a clone of the same project.

1. Install riff:

   ```sh
   cargo install --locked --git https://github.com/como-technologies/riff riff
   ```

2. Use the riff of your first machine. Put the name of your first
   machine in place of `FIRST`. The line goes in your shell profile, so
   that each new terminal has it:

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

Then start Claude Code in your clone of the project. Ask: *"Who else is
in the riff?"*
