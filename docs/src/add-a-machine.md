# Add a Machine

The sessions of a second Linux machine can join the riff of your first
machine. A riff that takes connections from your network needs
sign-in. You sign in on each machine with the same Google account.

## Make your OAuth client

riff has no built-in sign-in app. Make your own Google OAuth client
once. See
[Make your own OAuth client](development.md#make-your-own-oauth-client).
Keep its client ID and its client secret. Do not commit them.

## On the first machine

1. Stop the riff of your first machine with Ctrl-C. Start it again,
   so that it takes connections from your network, with sign-in. Put
   your client ID in place of `ID`, your client secret in place of
   `SECRET`, and the email of your Google account in place of `EMAIL`:

   ```sh
   export RIFF_OIDC_CLIENT_ID=ID RIFF_OIDC_CLIENT_SECRET=SECRET
   riff-server --listen 0.0.0.0:7878 --owner EMAIL
   ```

   `riff-server` keeps no settings. Give them again at each start. A
   riff with sign-in listens on your network only when it has an
   owner. `--owner` names you as the owner.

2. The new start of the riff forgets its messages, its claims and its
   sessions. In a second terminal, sign in on the first machine. Your
   browser opens:

   ```sh
   riff login
   ```

To update riff, see
[Update riff on two machines](#update-riff-on-two-machines).

riff-server has no TLS. On your network, the traffic is plain HTTP.
Each call carries a proof from the key of its machine, so a copied
token does not work on another machine.

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

3. Open a new terminal, so that it has `RIFF_SERVER`. Add riff to
   Claude Code. It signs you in: use the same Google account as on the
   first machine:

   ```sh
   riff connect claude
   ```

## Check it

On the second machine, list the sessions of the riff. It shows the
sessions of both machines:

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

## Update riff on two machines

Update both machines to the same riff at the same time.

1. On the first machine, stop `riff-server` with Ctrl-C. Install
   riff, and start its riff again with the settings of step 1 of
   [On the first machine](#on-the-first-machine):

   ```sh
   cargo install --locked --git https://github.com/como-technologies/riff riff riff-server
   export RIFF_OIDC_CLIENT_ID=ID RIFF_OIDC_CLIENT_SECRET=SECRET
   riff-server --listen 0.0.0.0:7878 --owner EMAIL
   ```

   The new riff forgets each sign-in. In a second terminal, add riff
   to Claude Code. It signs you in again:

   ```sh
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
