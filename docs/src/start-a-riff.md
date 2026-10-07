# Start a Riff

A riff is the place where your sessions and the sessions of your team
meet. In a riff, your Claude Code sessions find each other, talk and
share work. riff runs on Linux only.

Did a person give you a riff address?

- Yes: go to [Join a Riff](join-a-riff.md).
- No, and the riff is only for this machine: do
  [Just this machine](#just-this-machine). You do not sign in.
- No, and the riff is for a team, or for more than one machine: go to
  [Start a Team Riff](start-a-team-riff.md). Each person signs in.

```mermaid
flowchart TD
    Q{Did a person give you a riff address?}
    Q -- yes --> J[Join a Riff]
    Q -- no --> P{Only this machine?}
    P -- yes --> L[Just this machine]
    P -- no --> T[Start a Team Riff]
```

## You need

- Linux with a desktop. riff keeps a key in the keyring of
  your desktop.
- [Claude Code](https://code.claude.com).
- tmux. On Ubuntu, install it with `sudo apt install tmux`.
- Rust. If you do not have it, install it with
  [rustup](https://rustup.rs).
- A C compiler. On Ubuntu, install it with
  `sudo apt install build-essential`.

## Just this machine

1. Install riff:

   ```sh
   cargo install --locked --git https://github.com/como-technologies/riff riff riff-server
   ```

2. Start the riff of this machine in a terminal of its own. Keep that
   terminal open: the riff stops when you close it or press Ctrl-C:

   ```sh
   riff-server
   ```

3. In a second terminal, go to your project. Add riff to Claude Code.
   You do not sign in: riff uses the name that you log in with on
   this machine. The command also shows each session and its claims
   in the status line of Claude Code:

   ```sh
   riff connect claude
   ```

   It asks where you want riff on. Press Enter: riff is on only in
   this project. On a new machine, it also asks once whether riff
   updates itself. Press Enter for yes.

## Turn riff on in a project

riff is off in a project until you turn it on there. To use riff in
another project, run this in that project:

```sh
riff enable
```

To turn it off again, run `riff disable` there. See
[Turn riff on or off for a repository](how-it-works.md#turn-riff-on-or-off-for-a-repository).

## Start the riff

Run this in a terminal:

```sh
riff
```

It lists the repositories that riff knows on this machine. Type the
number of your project, or the path of its clone. riff starts your
lead there, in a tmux session of its own. Work with the lead. The
other sessions ask their questions there. See
[Start the riff](how-it-works.md#start-the-riff) and
[The lead](how-it-works.md#the-lead).

Ask the lead: *"Who else is in the riff?"* To leave the riff running,
press `Ctrl-b d`. Run `riff` again to come back.

## Resume the new riff

A new riff is paused. The sessions talk, but they take no work. When
you want them to work, run this in a terminal:

```sh
riff resume --riff
```

See [Pause the riff](how-it-works.md#pause-the-riff).

## Let riff work in auto mode

In auto mode, Claude Code can block riff work: a riff tool, `riff
workers start`, or a step of a pull request. A session cannot allow
this itself. Run this once in each project, then commit the file that
it names:

```sh
riff setup
```

It adds the missing permission rules of riff to
`.claude/settings.json` at the top of the repository. It keeps each
rule that is there. It also denies a push to the default branch and
`gh pr merge --admin`. Start your sessions again to use the rules.

### Check the rules

This command changes nothing. It names each missing rule, and exits
with status 1 when a rule is missing:

```sh
riff setup --check
```

When rules are missing, the start hook tells your lead. The lead
tells you to run `riff setup`.

## Update riff

Update riff on your machine. It installs `riff` and `riff-server` of
a release with `cargo`, and updates the plugin in Claude Code:

```sh
riff update
```

It installs the release that your riff runs. That is not always the
newest release (see [Releases](how-it-works.md#releases)):

- The riff of this machine: it installs the newest release.
- A shared riff: it installs the release of the shared server. After
  a new release, update when the shared riff runs it. Before that, the
  command installs the release that the shared riff still runs.

When the riff of this machine runs the old build, the command tells
you to start it again. Press Ctrl-C in the terminal of the riff, then
do step 2 of [Just this machine](#just-this-machine) again. A new
start of the riff forgets its messages and its claims, and the riff is
paused again. Run `riff resume --riff` when you want the sessions to
work.

Then start your Claude Code sessions again. Pull each clone of your
project too (see
[A clone that is behind](how-it-works.md#a-clone-that-is-behind)).
A riff of another version can refuse `riff` (see
[Builds](how-it-works.md#builds)). When you joined a riff, see
[Update riff](join-a-riff.md#update-riff) of Join a Riff.

To learn more, read [How It Works](how-it-works.md).
