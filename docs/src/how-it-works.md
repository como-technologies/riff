# How It Works

## Parts

```mermaid
flowchart LR
    subgraph M["Your machine"]
        S1[agent session] -- MCP --> C1["riff mcp"]
        W1["riff watch"] -- wakes --> S1
        L["riff-server<br/>in a terminal"]
    end
    subgraph N["A second machine"]
        S2[agent session] -- MCP --> C2["riff mcp"]
    end
    subgraph G["Google Cloud, off for now"]
        E["riff-server<br/>Cloud Run, one instance"]
        B[("Cloud Storage<br/>state")]
    end
    C1 -- "HTTP, loopback" --> L
    L -- wakes --> W1
    C2 -- "HTTP, your network" --> L
    C1 -. HTTPS .-> E
    E -- save and load --> B
```

- **`riff-server`** is the central service. It holds the live sessions,
  the threads, the claims and the leads. Now it runs on your machine, in
  a terminal (see [Start a Riff](start-a-riff.md)). It listens on
  loopback. A riff with sign-in listens on your network, so that other
  machines join it (see [Join a Riff](join-a-riff.md)). The shared
  server on Cloud Run is off.
- **`riff mcp`** gives your session its tools: `whoami`, `who`,
  `threads`, `join_thread`, `leave_thread`, `post`, `status`,
  `blocked`, `tell`, `read`, `claim`, `release`, `hold`, `free`,
  `lead`, `pause`, `resume`, `move`, `leave` and `join`.
- **`riff watch`** writes one line for each message that wakes the
  session. Your agent tool reads the line and wakes the session.
- **The start hook** runs `riff hook session-start` when a session
  starts. It tells the session to run `riff watch --once` as a
  background task. See [Wake a session](#wake-a-session).
- **The end hook** runs `riff hook session-end` when a session ends.
  It tells `riff-server` that the session left. See
  [When a session ends](#when-a-session-ends).

## Find a command

`riff --help` lists the commands that people use, under the headings
Get started, Work in the riff, Pull requests, Lead and Members. Each
command has one short line. `riff help` with a command shows its full
text:

```sh
riff --help
riff help invite
```

The plugin runs `riff hook`, `riff mcp`, `riff statusline` and
`riff watch`. `riff --help` does not list them, but `riff help hook`
shows the text of `riff hook`. `riff-server --help` lists the settings
of the server.

## Connect

`riff connect claude` adds the riff plugin to Claude Code. The plugin
gives a session the riff tools, the riff skill, a start hook and an
end hook. riff stays off in a session until you turn it on for the
repository of the session.

```mermaid
flowchart LR
    B[riff binary] -- writes --> D["~/.local/share/riff/claude-plugin"]
    D -- "claude plugin marketplace add" --> M[marketplace riff]
    M -- "riff enable" --> P["plugin riff@riff<br/>on in a repository"]
```

Claude Code loads the plugin from that directory. After you update
`riff`, run `riff connect claude` again.

In a terminal, `riff connect claude` asks one time where you want riff
on:

```text
Where do you want riff on?
  1) Only in this repository (default)
  2) In each repository on this machine
  3) Not now: I run `riff enable` later
Your choice [1]:
```

riff keeps your answer, and asks no more. With no terminal, it asks
nothing and changes nothing. `riff update` asks nothing too, also in a
terminal: an update never turns riff on in more repositories, and
never turns it off. On a machine where a release up to v0.8.0 turned
riff on in each repository, the command asks nothing and keeps that.
The last line of the command says where riff is on, and the command
to change it.

### Turn riff on or off for a repository

riff is off in a Claude Code session until you turn it on for the
repository. Where riff is off, it does nothing: no call to the riff,
no `git`, no text in the session, no tools, and an empty status line.
A directory that is not in a git repository is always off.

Run this in the repository:

```sh
riff enable
```

Then start a new Claude Code session there. To turn riff off again:

```sh
riff disable
```

`riff disable` changes only this repository. A session that runs
keeps riff until it ends. To take it out now, run `/riff:leave` in it.

`riff enable` writes one entry to a settings file of Claude Code:

```json
{
  "enabledPlugins": {
    "riff@riff": true
  }
}
```

Each other byte of the file stays. When the file is a symbolic link,
riff writes the file that the link names, and shows its path.

| Flag | File | Who gets riff |
|---|---|---|
| none, or `--local` | `.claude/settings.local.json` at the top of the repository | Only you, in this repository. |
| `--shared` | `.claude/settings.json` at the top of the repository | Each person of the team who has riff, after you commit the file. |
| `--global` | The user settings, `~/.claude/settings.json` | You, in each repository on this machine. |

The first file that has the entry decides, in the order local,
shared, global:

```mermaid
flowchart TD
    G{in a git repository?} -- no --> OFF[riff is off]
    G -- yes --> L{"local settings<br/>have the entry?"}
    L -- yes --> V[its value decides]
    L -- no --> S{"shared settings<br/>have the entry?"}
    S -- yes --> V
    S -- no --> U{"user settings<br/>have the entry?"}
    U -- yes --> V
    U -- no --> OFF
```

git does not track the local settings, so a linked worktree does not
have them. In a linked worktree, riff reads the settings of the main
clone too, and `riff enable` writes the local settings of the main
clone. It asks git for the main clone first, and writes nothing when
git does not know the worktree. It also writes nothing when the `.git`
of the worktree is a symbolic link: git makes a file there, not a
link.

### Turn riff on for the team

`--shared` writes the entry to the project settings. Commit the file.
Then each person who ran `riff connect claude` has riff in each clone:

```sh
riff enable --shared
```

A person who does not want riff in a clone runs `riff disable` there.
It writes `false` to the local settings of that clone.

### Turn riff on in each repository

`--global` turns riff on in each repository on this machine:

```sh
riff enable --global
```

`riff disable --global` takes that away. To keep riff out of one
repository, run `riff disable` there.

### Choose where riff is on with no question

`--scope` answers the question of `riff connect claude`, for a script:

```sh
riff connect claude --scope repo
riff connect claude --scope global
riff connect claude --scope none
```

### See whether riff is on here

`riff server` shows it in the line `repository`, with the file that
decides and the command to change it:

```sh
riff server
```

```text
repository  riff off. To turn it on: riff enable
```

### After an update from a release before the opt-in

A release up to v0.8.0 turned riff on in each repository: it wrote the
entry to the user settings. An update keeps that choice, and asks
nothing, in a terminal and with no terminal. riff stays on in each
repository of the machine, and you run no command.

To have riff only in some repositories, take the entry out, and turn
riff on in each one:

```sh
riff disable --global
riff enable
```

### When the riff server is turned off in /mcp

The `/mcp` dialog of Claude Code can turn the riff server off. Claude
Code keeps that for the project, so each new session there has no
riff tools. The start hook tells the session, and the status line
shows it:

```text
riff 2a880834 (no tools: the riff server is off, turn it on in /mcp)
```

To turn it on, run `/mcp` in a session of the project, and turn the
server `riff` on. A worker with no riff tools tells the lead with the
`riff tell` command.

### Use another claude command

`riff connect claude` runs the `claude` command on your `PATH`.
`--claude` names another one:

```sh
riff connect claude --claude ~/.local/bin/claude
```

## The riff that riff uses

`riff` finds its riff in `--server` or `RIFF_SERVER`. With neither, it
uses the riff of this machine, `http://127.0.0.1:7878`. riff keeps no
choice in a file.

```mermaid
flowchart LR
    F{"--server?"} -- yes --> U[that riff]
    F -- no --> E{"RIFF_SERVER?"}
    E -- yes --> U
    E -- no --> L["the riff of this machine<br/>http://127.0.0.1:7878"]
```

Each value is a URL, `HOST` or `HOST:PORT`. With no scheme, riff uses
`http://`. With no port, it uses port 7878. So `first` is
`http://first:7878`. An IPv6 address takes brackets with a port: `::1`
is `http://[::1]:7878`, and so is `[::1]:7878`. To use another riff, see
[Change to another riff](join-a-riff.md#change-to-another-riff).

### Show the forms of --server

The help of each command gives `--server` one short line.
`riff help server` shows the forms of a value and the default:

```sh
riff help server
```

### Show the riffs

`riff server` shows the riff that `riff` uses, and where that choice
comes from, with its release and your sign-in. It shows one fact on a
line:

```sh
riff server
```

```text
riff        v0.6.0  (75209ac, 2026-09-29)
server      https://riff.example.com  (from RIFF_SERVER)
  release   v0.6.0  same build ✓
  sign-in   yes, signed in as mike@example.com
```

- `local` shows the riff of this machine too, when it answers.
- `answer    none`: the riff does not answer.
- Under the sign-in line, it shows
  [the facts of the server](#see-the-facts-of-the-server).
- When you must act, the last line says what to run, for example
  `Run riff update` or `Run riff login`. It is yellow when the versions
  can talk, and red when they cannot or when you must sign in.

`localhost`, `127.0.0.1` and `[::1]` with the same port are one riff:
the riff of this machine. `riff server` shows it once.

To show the riffs with no color, use `--color never`. A pipe gets no
color either, so a `grep` finds a line:

```sh
riff server --color never
riff server | grep release
```

### See the facts of the server

`riff server` also asks the riff that `riff` uses for its facts. Use
them to see if the server is healthy:

```sh
riff server
```

```text
server      https://riff.example.com  (from RIFF_SERVER)
  release   v0.9.0  same build ✓
  sign-in   yes, signed in as mike@example.com
  serves    yes
  error     none since the start
  log       position 1234; the last chunk write was 3s ago and took 45 ms
  faults    0 write errors, 0 skipped records since the start
  saved     checkpoint at position 1000, 5m old, from v0.9.0
  counts    12 chunks, 8 sessions, 40 cursors, 9 threads, 5 live sign-ins
  memory    35 MB in use
  started   2h ago; the replay took 120 ms
```

| Line | Shows |
|---|---|
| `serves` | `yes`, or `no, it replies 503` and why. A server that does not serve still gives its facts. |
| `error` | The last error of the server since its start, with its age. |
| `log` | The position of the last record, and the time and the duration of the last chunk write. |
| `faults` | The failed tries of a chunk write, and the records of a later build that this build skipped: a record of a kind that this build does not know, or with a value that it does not know. |
| `saved` | The newest checkpoint: its position, its age and its release. When this build writes no checkpoint, the line says why. |
| `counts` | The chunks in the store, the sessions, the read cursors, the threads, and the live sign-ins. |
| `memory` | The memory that the server uses. |
| `started` | The age of the instance, and how long its load and its replay took. |

- A riff with sign-in gives its facts only to a person who is signed
  in. Else the lines are not there.
- An older `riff-server` has no facts. The lines are not there.

### Name the riff for one command

`--server` names the riff for one command. It wins over
`RIFF_SERVER`:

```sh
riff --server 127.0.0.1:7878 who
```

## Builds

A build of `riff` or `riff-server` is the crate version, the last
commit that changed the code, and the time of that commit. A commit
that changes only the book keeps the build.

The version is a semantic version, `MAJOR.MINOR.PATCH`. It tells which
versions can talk. The line of a version is its major, and its minor
while the major is 0: `0.4.1` is on the line `0.4`, and `1.2.0` is on
the line `1`. A change that another machine or session can notice
starts a new line (see [Versions](#versions)). Most merges keep the
line.

`riff-server` talks with a `riff` of its own line, and of the line
before. So you have one release to update `riff` in:

| riff | riff-server | Result |
|---|---|---|
| 0.4.0 | 0.4.3 | They talk. riff tells you once to update. |
| 0.3.2 | 0.4.0 | They talk. riff tells you to update soon. |
| 0.4.0 | 0.3.2 | riff-server refuses: update riff-server. |
| 0.2.0 | 0.4.0 | riff-server refuses: update riff. |

Each call of `riff` names its build, and each reply of `riff-server`
names its own. Each side compares the versions:

```mermaid
flowchart LR
    R["riff<br/>0.3.2 929605821e54"] -- "call, with the build of riff" --> S{"riff-server 0.4.0:<br/>riff on the line 0.4 or 0.3?"}
    S -- yes --> OK["the reply. Another build:<br/>riff tells you once to update"]
    S -- no --> E["409: both builds, and the side to update"]
```

### See the build

```sh
riff --version
riff-server --version
```

`riff whoami` and `riff who` also show the build, after the server
answers, in the fact `build`. When the builds differ, the fact
`riff-server` shows the build of the server, in yellow, and says that
the versions can talk:

```text
build        v0.7.0  (3c7b111, 2026-09-29)
riff-server  v0.7.0  (f45be4d, 2026-09-29)  another build; the versions can talk
```

### When the builds differ

`riff` works. Each `riff` process tells you once:

```text
riff: riff-server runs build 0.4.3 7213825ab1c2 2026-09-27T20:10:44Z; this riff runs build 0.4.0 929605821e54 2026-09-27T22:03:01Z. Run riff update when you can.
```

When `riff` is on the line before the server, the next line of the
server refuses it. The note says so:

```text
riff: riff-server runs build 0.4.0 7213825ab1c2 2026-09-27T20:10:44Z; this riff runs build 0.3.2 929605821e54 2026-09-20T22:03:01Z. riff-server 0.5 will refuse riff 0.3. Run riff update soon.
```

Update riff on this machine when you can:

```sh
riff update
```

`riff watch`, `riff tail`, `riff top`, `riff chat`, `riff workers
host` and the riff tools of each session (`riff mcp`) see the new
`riff` on disk, and run it.
They go on with no restart, in the same repository and worktree. This
is also true when you removed their worktree. You do not run `/mcp`.

- `riff chat` keeps its lines on the screen, and draws its prompt
  again. Text that you typed but did not send is lost.
- The riff tools of a session run the new `riff` when no tool call
  runs. The session keeps its tools and its claims. First they run the
  new `riff` once as a check. When the check fails, the tools keep the
  old `riff` and wait for the next new one. The error is in the MCP
  log of the session.
- `riff workers host` runs the new `riff` between two requests of the
  lead. It keeps its session, and its workers go on.

```mermaid
flowchart LR
    N["a new riff on disk"] --> F{"a tool call runs?"}
    F -- yes --> F
    F -- no --> C{"the new riff passes its check?"}
    C -- yes --> R["the tools run the new riff"]
    C -- no --> K["the tools keep the old riff"]
    K --> N
```

### When the riff tools of a session are gone

The riff tools of a session can stop while the session goes on, for
example after a crash. Claude Code does not start them again. Then
the status line of the session says it:

```text
riff 2a880834 (no tools: riff mcp stopped, reconnect riff in /mcp)
```

Each end of `riff watch` says it to the session too. To get the tools
back, run `/mcp` in the session, and reconnect the server `riff`. Or
start the session again. Until then, the session can use the riff
commands in a shell, for example:

```sh
riff read
```

### When riff cannot read its directory

A `riff` command in a removed directory fails. The error names the
directory:

```text
Error: riff cannot read its working directory /src/riff/.claude/worktrees/issue-12. Change to a directory that exists
```

Change to a directory that exists, for example your repository, and
run the command again:

```sh
cd ~/src/riff
```

### When the versions do not match

Each `riff` command fails with an error like this one:

```text
riff: this riff (0.2.0 929605821e54 2026-09-27T22:03:01Z) and its riff-server (0.4.0 7213825ab1c2 2026-09-28T20:10:44Z) do not match. riff-server 0.4 talks only with riff 0.4 and 0.3. Update riff on this machine, then start your sessions again. See https://como-technologies.github.io/riff/how-it-works.html#when-the-versions-do-not-match
```

A reply with no build names what riff saw in place of the build of
riff-server, for example
`no riff build in the reply: status 200 OK from https://…/v1/who`.

A new session gets the same error at its start. It tells you, and it
does not use the riff. `riff watch` and `riff tail` print the error
once, try again every 5 seconds, and go on when the versions can
talk.
[`riff server`](#show-the-riffs) shows both builds.

Update the older side:

- **riff** on this machine, and **riff-server** of your own riff: do
  [Update riff](start-a-riff.md#update-riff). When you joined a riff,
  see [Update riff](join-a-riff.md#update-riff) of Join a Riff.
- **The shared server:** an admin pushes a release tag at the end of
  each wave, and CI deploys it. A push to `main` does not deploy it.
  See
  [Deploy the shared server at the end of a wave](development.md#deploy-the-shared-server-at-the-end-of-a-wave).

Then start your Claude Code sessions again.

### Releases

`riff update` installs a release, not the newest commit of `main`. A
release is a git tag `vX.Y.Z` of the repository (see
[Make a release](development.md#make-a-release)). It picks the release
this way:

```mermaid
flowchart TD
    U[riff update] --> T{"--tag vX.Y.Z?"}
    T -- yes --> I[install that release]
    T -- no --> L{"riff uses the riff<br/>of this machine?"}
    L -- yes --> N[install the newest release tag]
    L -- no --> B{"riff can read<br/>the build of the riff?"}
    B -- yes --> S[install the release that the riff runs]
    B -- no --> N
```

So a new tag does not break your machine before the shared server
runs it. `riff server` shows the release of `riff` and of each riff.

An old riff cannot always read the build of a newer riff, for example
after a change of the build header. Then `riff update` installs the
newest release tag and prints this line:

```text
riff cannot read the build of the riff at https://riff.example.com, so riff installs the newest release, v0.3.0.
```

### Install one release

To install one release, for example the release of another riff, name
its tag:

```sh
riff update --tag v0.2.0
```

### See what changed in a release

The GitHub releases are the changelog of riff. The notes of a release
give its level, the command that each person runs, and each pull
request that the release adds. The pull requests come in three groups:
`Changes` for people first, then `Design` (design pages), then
`Internal` (tests and checks only). List the releases, and read one:

```sh
gh release list --repo como-technologies/riff
gh release view v0.4.0 --repo como-technologies/riff
```

### Update riff by itself

A machine can update riff by itself. When the riff runs a new
release, riff on the machine installs that release, with no
`riff update` by you. Turn it on for this machine:

```sh
riff update --auto on
```

Turn it off:

```sh
riff update --auto off
```

See the setting:

```sh
riff update --auto
```

```text
update.auto  true  (/home/mike/.config/riff/config.toml)
Turn it off with: riff update --auto off
```

The setting is the key `update.auto` in
`~/.config/riff/config.toml` (`$XDG_CONFIG_HOME/riff/config.toml`).
It is off by default.

Each reply of the riff names its release. When a `riff` process sees
a newer release than its own, it starts one update in the background,
and tells you once:

```text
riff: riff-server runs the release v0.4.0. riff installs it now, in the background (update.auto).
```

The update goes this way:

```mermaid
sequenceDiagram
    participant P as riff process
    participant S as the riff
    participant U as riff update, in the background
    participant L as your lead
    P->>S: call
    S-->>P: reply, release v0.4.0 (newer)
    P->>U: start: riff update --tag v0.4.0
    U->>U: take the update lock of the machine
    U->>U: install v0.4.0, update the plugin
    U->>L: "riff on pangolin updated itself from v0.3.0 to v0.4.0."
```

- One update runs at a time on the machine. Each release gets one
  try.
- `riff watch`, `riff tail`, `riff top`, `riff chat` and `riff mcp`
  run the new `riff` with no restart. A worker takes the new `riff` at
  its next item.
- The update stops no session, and a running test goes on. Your
  sign-in and the device key stay.
- Your lead gets one direct message: the host, the old release and
  the new release.
- A failed update keeps the old `riff`. The message to your lead holds
  the error. riff tries again at the next release. To try again now,
  run `riff update --tag` with the release.
- The update runs in a directory that exists, also when the `riff`
  process that saw the new release runs in a removed worktree. If the
  directory of the update is missing, the release does not count as
  tried, and the next `riff` command tries again.
- riff never installs an older release by itself.

The output of the update is in `update.log`, in the local files of
riff: `$XDG_RUNTIME_DIR/riff`, else `~/.local/state/riff`.

### A new machine asks about the update by itself

On a machine with no `update.auto` key, for example a new or rebuilt
machine, `riff login` and `riff connect claude` ask you once:

```text
Update riff by itself when the riff gets a new release? [Y/n]
```

Press Enter to turn it on, or type `n` to keep it off. They do not ask
again. With no terminal, for example in a script, they do not ask.
To ask again, remove the key, then run:

```sh
riff connect claude
```

## Versions

A riff is a contract between machines and between sessions. Two
sessions on different versions in one riff must not act differently
or fail to understand each other. The level of a release tells you
what changed. While riff is `0.x`, the minor has the role of the
major.

### Patch

`0.3.0` to `0.3.1`. Nothing changes that another machine or session
can notice:

- a fix that restores the intended behavior;
- the docs, an output format, the tests;
- words of the skill or of a requirement that make a rule clearer,
  with no change in behavior.

### Minor

`0.3.x` to `0.4.0`. A change that another machine or session can
notice:

- the wire: a route or a field that a client needs, a removed one, or
  a new meaning;
- the saved state of `riff-server`;
- the plugin contract: the hook input, and the names and arguments of
  the riff tools;
- the behavior: a change of the skill or of a requirement that changes
  what a session does, for example the pull request flow, the verify,
  the claims, the waves or the pause.

`riff-server` still talks with a `riff` of the minor before its own
(see [Builds](#builds)), so a wire change stays additive for one
minor. Riff does not bridge a difference in behavior: the version note
tells the older session to update.

### Major

`1.0.0` promises that the wire and the behavior stay stable. After
it, a major breaks, a minor adds and a patch fixes. A change of
behavior that can break a pipeline or a policy is a major.

### The test for each release

Ask: can a session on the old version and a session on the new
version, in the same riff, act differently or fail to understand each
other?

```mermaid
flowchart LR
    Q{"old and new session<br/>in one riff: act differently<br/>or fail to understand?"} -- yes --> M["minor<br/>(major after 1.0)"]
    Q -- no --> P[patch]
```

A wave release is a patch unless the wave has a minor change. Most
waves change the skill, so most wave releases are a minor.

## A clone that is behind

A session reads `CLAUDE.md` and the project settings from its clone.
When the clone was not pulled, the session uses old rules. So at each
session start, the start hook fetches `origin` for at most 2 seconds.
When the default branch is behind, the session tells you, through the
lead. The hook does not pull.

```mermaid
sequenceDiagram
    participant H as start hook
    participant O as origin
    participant S as session
    H->>O: git fetch (2 seconds at most)
    O-->>H: main has 3 new commits
    H-->>S: "This clone is 3 commits behind origin/main ..."
    S->>S: tells you, through the lead
```

With no remote, a remote that cannot be reached, or a slow fetch, the
session gets no line. The session start does not stop.

### Pull a clone that is behind

Run this in the main worktree of the clone. The line of the session
names the path. Then start your Claude Code sessions again:

```sh
git pull --ff-only
```

## Sign-in

```mermaid
sequenceDiagram
    participant C as riff login
    participant G as Google
    participant E as riff-server
    C->>G: open browser, sign in
    G-->>C: Google ID token
    C->>E: Google ID token + device public key
    E->>E: check signature; owner, admin, member or allowed domain
    E-->>C: riff tokens, bound to the device key
    Note over C: tokens and key go to the OS keyring
```

Each agent session then gets its own short-lived token from `riff`.
The token acts only as that session. `riff` keeps it in memory, not
in the keyring.

A session token has no refresh token. Each `riff` process of a session
swaps the person token for a session token of its own: `riff mcp`,
`riff watch`, each hook and each `riff` command. A new session token
ends no other token. A process that runs for a long time swaps the
person token again before its session token expires.

```mermaid
sequenceDiagram
    participant M as riff mcp
    participant C as riff claim
    participant S as riff-server
    M->>S: swap the person token, session a6cf
    S-->>M: session token 1
    C->>S: swap the person token, session a6cf
    S-->>C: session token 2
    M->>S: a call with token 1
    S-->>M: 200, token 1 stays live
    Note over M: before token 1 expires
    M->>S: swap the person token, session a6cf
    S-->>M: session token 3
```

A refresh token works once. It names its chain and its generation:
`chain.generation.secret`. A sign-in has one chain, for the person.
Each refresh gives the next generation of the chain. `riff-server`
keeps only the hash of the current generation, and of the one before
it.

A refresh token of an older generation ends the sign-in: somebody
copied it. A lost reply is not a copy. When the token of the generation
before the current one comes back, the reply of its refresh did not
come back, for example after a 503. `riff-server` then gives a new
pair, and the sign-in stays. Only the device key of the sign-in can end
it this way: the server checks the key first.

```mermaid
flowchart TD
    R[refresh token of generation G comes back] --> K{the device key of the sign-in?}
    K -- no --> W[refuse; the sign-in stays]
    K -- yes --> C{G is the current generation?}
    C -- yes --> N[new pair, generation G+1]
    C -- no --> L{G is the generation before it?}
    L -- yes --> P[end the current pair, give a new pair: the reply was lost]
    L -- no --> E[G is older: end the sign-in]
```

### Get back into the riff

`riff watch`, `riff tail` and each tool of `riff mcp` show the error
of a lost sign-in. Find it in this table, and do the step.

| Error | What it means | Step |
|---|---|---|
| `the sign-in ended: run riff login: riff-server refused the token request: invalid_grant` | `riff-server` ended the sign-in of this machine. | Run `riff login`. |
| `the sign-in ended: run riff login` | The sign-in of this machine ended before. `riff` sends no token request. | Run `riff login`. |
| `no sign-in for URL: run riff login` | This machine has no sign-in at the riff. | Run `riff login`. |
| `This riff is new. Run riff login.` | The riff restarted with no bucket. | Run `riff login`, then start your sessions again. See [A restart](#after-a-restart-with-no-bucket-run-riff-login). |
| `this riff (…) and its riff-server (…) do not match` | The versions do not match. | See [When the versions do not match](#when-the-versions-do-not-match). |

Sign in again on the machine:

```sh
riff login
```

`riff login` works also when the versions do not match. After it,
each session of the machine goes on by itself: a running `riff watch`,
`riff tail` and `riff mcp` need no restart. A watch says once that the
sign-in ended, and tries again every 5 seconds.

`riff` asks `riff-server` one time only with a sign-in that ended. It
keeps on the machine that the sign-in ended. Until `riff login`, each
`riff` command says so and sends no token request.

## A session URI

```text
riff://mike@pangolin/como-technologies/riff?session=a6cf&claim=issue-6#issue-6
       └─┬┘ └──┬───┘ └─────────┬────────┘ └────┬─────┘ └─────┬─────┘ └──┬──┘
       user   host        owner/repo        session ID     claim     worktree
```

The URI shows three things:

- **Who:** the user and the session ID of the agent tool. They never
  change. A resumed session keeps its ID. After `/clear`, the session
  keeps its ID too (see [After /clear](#after-clear)).
- **Where:** the host, the repository and the worktree. They change when
  the session moves.
- **What:** the claims that the session holds, and `lead=true` when
  the session is the lead (see [The lead](#the-lead)).

Short form, for people: `mike@pangolin:riff#issue-6`. It is not unique.

### See your URI

In a terminal, `whoami` shows your URI as a person, and the state of
the riff (see [Pause the riff](#pause-the-riff)). In Claude Code, type
it in the prompt with `!` in front to see the URI of the session. A
post from a terminal names the host where you ran the command.

```sh
riff whoami
```

```text
session  mike@pangolin:riff#issue-6 (a6cf2205)
uri      riff://mike@pangolin/como-technologies/riff?session=a6cf2205-…#issue-6
riff     running
build    v0.7.0  (f45be4d, 2026-09-29)
```

## See who is in the riff

```sh
riff who
```

The first lines are facts: the state of the riff, the owner, and the
build. The owner is `none` when the riff has no owner. A riff with no
sign-in shows no owner. Then a table shows a row for each session: its
name with the short session ID, its state, its role, and the detail of
its state:

```text
riff   running
owner  mike (mike@comotechnologies.io)
build  v0.7.0  (f45be4d, 2026-09-29)

SESSION                            STATE    ROLE      DETAIL
mike@pangolin:riff#issue-6 (a6cf)  busy     you lead  working on #6
mike@thelio:riff#issue-7 (5b1e)    busy     worker    working on #7  4m ago: write the tests
brett@heron:riff (77e0)            blocked            which of two designs? (1m ago)
```

`you` marks your own row. A tag shows the role of a session:

- `lead`: the lead of its user in the repository (see
  [The lead](#the-lead)).
- `worker`: a worker that `riff workers start` started. The worker
  tells the server, so each machine shows the same tag.

The owner is a person, not a session. So no session has the tag
`owner`. Only a line with no session, a person on the command line of
the owner, has it.

STATE is the state of the session, and DETAIL tells more about it. See
[The state of a session](#the-state-of-a-session). `riff top`,
`riff workers` and the `who` tool use the same words.

When you must act, the last line says so, in yellow: for example when
the riff or your repository is paused, or when the riff has no owner.

In a terminal, `riff who` has the colors of `riff tail`: each session
has the same color in both. `running` is green and `paused` is yellow.
A blocked status is red. The `who` tool of a session stays plain.

### Show the URI of each session

The table shows a short name. To see the full URI of each session in
its place, use `--long`:

```sh
riff who --long
```

### List the sessions without color

Color goes only to a terminal. `--color never` turns it off also in a
terminal. `NO_COLOR=1` does the same. `--color always` keeps the color
in a pipe. `--color` works the same on each `riff` command:

```sh
riff who --color never
riff who > sessions.txt
riff who --color always | less -R
```

`who` does not list a gone session. A session is gone when it ended, or
when it stopped for 3 minutes. To list gone sessions too:

```sh
riff who --all
```

## See what each session does

`riff top` shows a live table of each person and each session. It
draws the table again every 3 seconds, and after each message of the
thread. Ctrl-C stops it:

```sh
riff top
```

The header shows the state of the riff, the owner and the build, as in
`riff who`. A board comes next for each repository with a live
session, and for the repository where you run `riff top`: the current
wave with its repository, one line for the `free`
items, one for the `claimed` items, and one for the items in `verify`.
An item is in `verify` when a session verifies it, and when no session
holds it and its pull request waits for a verify or for the merge.
Under the boards, a tree shows each person, the hosts of the person,
the repositories on each host, and the sessions in each repository:

```text
riff   running
owner  mike (mike@example.com)
build  v1.0.0  (10df8a4, 2026-10-04)

blocked  3a3f8d5d issue-8, 32m, the lead gave no answer: which of two designs?

Wave 3 (como-technologies/riff)
  free: #9
  claimed: #7 #8

Wave 5 (como-technologies/strata)
  claimed: #88

ann  admin  offline  seen 1h ago
brett  online  › kadomony  › strata  1 session: 1 busy, 1 claim
└─ 7a8b9c0d  #issue-88  worker  busy
     working on #88 Split the store
mike  owner  online  6 sessions: 1 busy, 3 idle, 1 blocked, 2 claims
├─ pangolin  › riff  2 sessions: 1 busy, 1 idle, 1 claim
│  │  load 13.2 9.8/8  3000MHz now 2990  20GB avail/4  workers 3/4  jobs 2
│  │  last kill 14:03:12 by systemd-oomd
│  ├─ 5b1e2a90  #issue-7  worker  busy
│  │    working on #7 Fix the help
│  │    runs Bash: run just ci for 12m
│  │    20m ago: tests
│  └─ 9c0d1e2f  worker  idle
│       ready for work for 6m
└─ thelio  4 sessions: 2 idle, 1 blocked, 1 claim
   ├─ riff  3 sessions: 1 idle, 1 blocked, 1 claim
   │  ├─ 3a3f8d5d  #issue-8  blocked
   │  │    which of two designs? (32m ago)
   │  │    the lead gave no answer
   │  │    working on #8 Show the plan
   │  ├─ 4e54d4e5  lead  idle
   │  │    monitoring work for 2h
   │  │    5m ago: plan the next wave
   │  └─ 8f1c2d3e  offline
   │       seen 4m ago
   └─ strata  1 session: 1 idle
      └─ 6d2b7c1a  lead  idle
           monitoring work for 1h
```

- A blocked session has a red line before the boards: the session, its
  claims, the reason and the time that it waits. When the lead gave no
  answer, the line says so. See [A blocked session](#a-blocked-session).
- A person line: the user in a bold color, the tag `owner` or `admin`,
  and `online` when a session of the person is online. Else `offline`,
  with the time since the last call. Each member of the riff has a
  line, also when away.
- A person, host or repository line ends with its counts: the
  sessions, then the `busy`, `idle` and `blocked` sessions and the
  claims. A count of 0 is not shown.
- A line with only one line under it takes that line, after a `›`. In
  the example, brett has one host and one repository, so the person,
  the host and the repository are on one line. So a small riff stays
  short.
- A repository line has the short name of the repository. When the
  repositories have more than one owner, it has the owner too:
  `como-technologies/riff`.
- A session: the first line has the short session ID, the worktree of
  the session (`#issue-7` in the worktree `issue-7`, nothing in the
  main worktree), the tag `lead` or `worker`, and the state of the
  session in its color. Under it comes one line for each fact of the
  detail of the state. See
  [The state of a session](#the-state-of-a-session). A lead is the
  lead of the repository on the line above it.

riff makes the state of each session from facts: its claims, its tool
calls, the pull request of its item, and its block. No session types
its state. A session is `offline` with no open watch, and `paused`
while its repository is paused. Else it moves between these states:

```mermaid
stateDiagram-v2
    [*] --> idle
    idle --> busy: claim
    busy --> waiting: a verify is asked, a merge waits, a need is open
    waiting --> busy: the fact ends
    busy --> blocked: blocked REASON (wakes the lead)
    blocked --> busy: an answer, then a sign of work
    busy --> must_clear: the last release of a worker
    must_clear --> idle: a clear
```

The table of each state, its color and its detail is in
[The state of a session](#the-state-of-a-session).

- A machine that runs workers: under its host comes a line of
  numbers. `load 13.2 9.8/8` is the load average of 1 and 5 minutes,
  and the physical cores. `3000MHz now 2990` is the cap of the clock
  and the clock now. `20GB avail/4` is the available memory and the
  floor. `workers 3/4` is the workers and the limit. `jobs 2` is the
  jobs of each worker. A number over its limit is yellow. A second
  line shows the time and the cause of the last kill that the monitor
  saw. See
  [Watch the health of a machine](#watch-the-health-of-a-machine).
  The numbers of a host come from its `riff workers host`. The
  numbers of the machine where you run `riff top` come from that
  machine, when its limit of workers is more than 0.
- Each board has the issues of its own repository, and a session shows
  the title of an issue of its own repository. riff reads them with
  `gh` each minute, for each repository with a live session. A
  repository that `gh` cannot read has no board and no titles, and
  no error line.

```mermaid
flowchart LR
    T["riff top"] -->|each minute| G["gh: issues and pull requests"]
    G --> B1["board of como-technologies/riff"]
    G --> B2["board of como-technologies/strata"]
    T -->|each 3 seconds| S["riff-server: who"]
    S --> Tree["person › host › repository › session"]
```

The tree grows down, not across. No line is wider than your terminal,
or 80 columns in a pipe. riff cuts a longer line with `…`.

The tags mean the same as in [`riff who`](#see-who-is-in-the-riff).
People are in the order of user, hosts in the order of name, and
repositories in the order of `OWNER/REPO`. In a repository, a blocked
session comes first. With no `gh`, the tree shows no titles and no
board.

`riff top` only reads. It posts nothing and wakes no session.

The lead can keep it in a tmux pane beside `riff tail`.

### When riff cannot reach the server

A laptop sleeps, or the Wi-Fi drops. `riff top` stays open. It keeps
the last table, and its first line is red:

```text
riff: no good look since 21:35:07: riff-server gave no reply in 10 seconds
```

The line has the time of the last good look, then the fault. The fault
can be another text, for example `cannot reach riff-server`. riff
tries a look again for 10 seconds before the look fails. So the line
comes about 13 seconds after the fault starts.

The table under the line is old. It keeps its titles and its board,
also when `gh` fails too. `riff top` looks again every 3 seconds. You
do nothing: when the server is back, the line goes and the table is
new.

```mermaid
stateDiagram-v2
    [*] --> Good: the first look is good
    Good --> Good: a good look, a new table
    Good --> Fault: riff cannot reach the server
    Fault --> Fault: the last table, with the red line
    Fault --> Good: the server is back, a new table
```

`riff top` still ends with the error in these cases:

- The first look fails. Check the server, then start `riff top` again.
- A new try cannot repair the fault, for example when your sign-in
  ended. Do what the error says.
- You ran `riff top --once`.

### Print the table once

To print one table and exit, for example in a pipe:

```sh
riff top --once
riff top --once --color never > sessions.txt
```

`--color` works the same as in `riff who`.

### Show the sessions of one person

```sh
riff top --user brett
```

The tree shows only the sessions of that user. The boards are those of
the repositories of these sessions.

### Show the sessions on one host

```sh
riff top --host pangolin
```

### Show the sessions in one repository

```sh
riff top --repo como-technologies/strata
```

Give the repository as `OWNER/REPO`. You can use `--user`, `--host` and
`--repo` together. A session shows when it matches each one:

```sh
riff top --user mike --repo como-technologies/strata
```

### Put the repositories at the top of the tree

To see what each repository does across people:

```sh
riff top --by repo
```

The tree then has the levels repository, person, host, session.
`--by person` is the default. `--by` works with the other flags.

## When a session ends

A session that runs sends a sign of life to `riff-server` each minute,
also while it waits for its user. When the session ends, it tells the
server. It leaves `who` at once. Its claims are free at once, and its
lead does not count while it is gone.

```mermaid
sequenceDiagram
    participant A as agent tool
    participant M as riff mcp
    participant S as riff-server
    loop each minute
        M->>S: keep-alive
    end
    A->>M: /exit
    M->>S: end
    Note over S: gone: not in who, claims free, lead does not count
```

- The time of `offline` (`seen 2h ago`) does not change with a
  keep-alive. It is the time since the last call.
- A session that stops with no end, for example after `kill -9` or a
  network fault, is gone after 3 minutes. Its claims and its lead end
  together, 5 minutes after its last sign of life.
- A sign of life is a call or a keep-alive. `riff mcp` and `riff watch`
  each send a keep-alive each minute. An open watch is no sign of life:
  the server cannot see that the process of a watch died.
- A gone session gets no messages. A `tell` to it fails.
- When a gone session calls again, it comes back with the same ID and
  threads. After a stop with no end, it also gets back each claim that
  no other session took. After an end, it has no claims.
- `/clear` does not end the session.
- A lead that comes back after an end or a resume is the lead again,
  unless another session became the lead meanwhile. `/clear` keeps the
  lead.

### A new start is blank

A session that starts again comes back blank: after `/clear`, after a
resume, and in a new Claude Code process. It keeps its ID, its threads
and its lead, but its claims are free at once. The start hook tells
the session which claims it lost. Another session can take them.

```mermaid
sequenceDiagram
    participant C as Claude Code
    participant H as start hook
    participant S as riff-server
    participant B as other session
    Note over C: /clear, resume or a new process
    C->>H: start
    H->>S: start
    S-->>H: freed: issue-12
    H-->>C: "This new start freed your claims: issue-12 ..."
    B->>S: claim issue-12
    S-->>B: granted
    B->>B: finds branch worktree-issue-12, goes on from it
```

A session that takes an item looks for the work of an earlier session
first: a pushed branch, or a worktree on its machine. It goes on from
that work (see [A branch with WIP commits](#a-branch-with-wip-commits)).
A compaction is not a new start: the session keeps its claims.

To see that a new start freed the claims, run this after `/clear`:

```sh
riff who
```

The riff plugin runs the end hook for you. To end a session by hand,
for example one that runs with no plugin, run this in its directory
with its `RIFF_SESSION`:

```sh
echo '{"reason":"other"}' | riff hook session-end
```

### Start an item again

Only when the earlier work is wrong, the session starts again. It
deletes the pushed branch of the earlier work first, so that the old
commits do not mix with the new work. For `issue-12`:

```sh
git push origin --delete worktree-issue-12
```

The session says in its start post that it deleted the branch.

### A branch with WIP commits

A session can end at each moment, for example when the machine has no
memory left. So each session commits its work as WIP and pushes its
branch before each long run (`just ci`, a test loop, a build) and at
each change of step. The work is then not on one machine only, and the
next session goes on from the branch.

```mermaid
sequenceDiagram
    participant A as session on pangolin
    participant O as origin
    participant S as riff-server
    participant B as session on thelio
    A->>S: claim issue-12
    A->>O: push worktree-issue-12 with WIP commits
    Note over A: killed
    S->>S: the claim is free
    B->>S: claim issue-12
    S-->>B: granted
    B->>O: git fetch
    B->>B: "Earlier work on issue-12: the pushed branch ..."
    B->>O: goes on, and pushes more commits
```

The rule for a person who looks at such a branch:

- A WIP commit has `WIP` in its subject. It is work that is not
  done: it can fail the checks, and no session verified it.
- Do not review a WIP commit, and do not build on it. Look at the pull
  request: the verify result names the commit that passed.
- The pull request merges with a squash. So `main` gets one commit for
  each pull request, and no WIP commit shows there.
- The forge deletes the branch when the pull request merges.

To see the commits of the branch of an item, for `issue-12`:

```sh
git fetch --prune origin
git log --oneline origin/main..origin/worktree-issue-12
```

### See the earlier work on an item

When a session claims an item, riff shows the earlier work on it: each
pushed branch, and each worktree on the machine. You see the same
line when you claim by hand:

```sh
riff claim issue-12
```

```text
You hold issue-12 in como-technologies/riff.
Earlier work on issue-12: the pushed branch origin/worktree-issue-12 at 1a2b3c4 (a WIP commit, 2 hours ago); the worktree /home/mike/src/riff/.claude/worktrees/issue-12 (3 files not committed, 1 commit not pushed). Go on from it, and do not start again: see "Pick up dropped work" in the riff skill.
```

- riff fetches from `origin` first, for at most 5 seconds.
- A worktree can hold files that are not committed. They are the only
  copy. The session commits them as WIP and pushes them before it goes
  on.
- A commit that is not pushed is on no branch of `origin`. After a
  squash merge, the branch is gone, but `main` holds the work. riff
  then counts no commit as not pushed.
- A verify claim gets no such line: a verify worktree holds no work.
- The start hook lists the earlier work of the clone that no live
  session owns, at most 8 items. So a new session sees it before it
  picks an item.

### Free the claim of another session

A session can hold an item and not work on it: its process died and
the server did not see it yet, or it does not answer. Then no other
session can claim the item. The lead of your user frees the claim for
that session. Find the session ID in `riff who --all`:

```sh
riff who --all
```

Then run this in the lead session, for example `! riff release ...`
in Claude Code. The start of the session ID is enough, from 4
characters:

```sh
riff release issue-12 --session 068a2cc2
```

```text
You released issue-12 in como-technologies/riff for the session 068a2cc2. The item is free.
```

- Only the lead can do it, and only for a session of its own user in
  its repository. riff refuses each other session, and a person in a
  shell.
- The server posts a note in the thread. The note names the lead, the
  item and the session.
- The next session claims the item, and goes on from the pushed
  branch. See
  [See the earlier work on an item](#see-the-earlier-work-on-an-item).

## Leave and join the riff

Each Claude Code session joins the riff when it starts. You can take
one session out of the riff, and put it back later. Your other
sessions keep riffing.

```mermaid
sequenceDiagram
    participant P as you
    participant A as session
    participant S as riff-server
    P->>A: /riff:leave
    A->>A: WIP commit and push, when it holds a claim
    A->>S: end: not in who, claims free
    Note over A: watch stops, tools refuse, no wakes
    P->>A: /riff:join
    A->>S: register, same session ID
    A->>A: start the watch, start routine
```

- A session that holds a claim pushes its work first, as a WIP commit
  on its branch. On the default branch it refuses, and stays in the
  riff. It also refuses in a dir that is not the top of a git worktree,
  so it never commits to a repository above that dir.
- A session that left makes no call to `riff-server`. Its riff tools
  refuse, except `join`. The hooks add no riff context. The status line
  shows `(left)`. The watch, the hooks and each `riff` command of the
  session stop too.
- The leave holds over `/clear`, a resume, and a restart of the
  machine. A new session joins as usual.
- You can also say it in plain words: "leave the riff" or "join the
  riff".

### Leave the riff

In the Claude Code session, run:

```sh
/riff:leave
```

To see that the session left, run this in a shell:

```sh
riff who
```

The list does not show the session. The status line of the session
shows `(left)` after its ID.

### Check that a session stays out

The leave is a mark on your machine: an empty file `left-ID`, where ID
is the session ID. Each riff process of the session reads the mark
before each request, and sends nothing. To see the marks, run:

```sh
ls "${XDG_STATE_HOME:-$HOME/.local/state}"/riff/left-*
```

Then run `riff who` again after 3 minutes. If the list shows the
session, a process of the session sent a request: that is a fault of
riff.

### Join the riff again

In the same session, run:

```sh
/riff:join
```

The session comes back with the same session ID, starts its watch, and
looks for work.

## Join the work

A new session starts in the main worktree. It finds its own work: it
picks the free item of the current wave that it thinks is best (see
[Waves](waves.md)). It does not wait for a plan. A scope from its user
wins.

```mermaid
sequenceDiagram
    participant S as new session
    participant E as riff-server
    participant G as git
    S->>E: register (main worktree)
    S->>E: read como-technologies/riff
    S->>E: claim issue-6
    E-->>S: granted
    S->>G: worktree add .claude/worktrees/issue-6
    S->>E: move (worktree issue-6)
    S->>E: post "started issue-6"
    S->>E: post verify request
```

What comes after the verify request is in
[How a session works on an item](#how-a-session-works-on-an-item).

A session removes a worktree only when the work is safe on the default
branch: its own worktree, or the worktree of an item whose steps after
the merge it does. It uses the `ExitWorktree` tool only for a
worktree that it made in its current context. After a clear of its
context, the tool says that the session is not the owner.
Then the session runs `riff worktrees clean` in the main worktree
(see
[Clean the worktrees of sessions that ended](#clean-the-worktrees-of-sessions-that-ended)).

## How a session works on an item

One context holds one item. A worker does not wait for a verify, and
it does not start a second item in the same context. So the work of a
worker on an item ends at the verify request. The worker writes the
state on the issue, releases the item, and riff clears its context.
The session that verifies does the steps after the merge.

```mermaid
sequenceDiagram
    participant A as author (worker)
    participant G as GitHub
    participant E as riff-server
    participant V as verifier
    participant N as next session
    A->>E: claim issue-6
    A->>A: the work, the checks
    A->>G: riff pr open: the pull request, auto-merge on
    A->>E: post verify request
    A->>G: comment on issue 6: the state
    A->>E: release issue-6
    Note over A: the turn ends, riff clears the context
    V->>E: claim verify-issue-6
    V->>V: test each criterion
    V->>G: riff verify: the result, riff/verify
    alt pass
        G->>G: squash merge
        V->>E: note "done issue-6"
        V->>V: remove the worktree and the branch of issue-6
        V->>E: release verify-issue-6
    else fail
        V->>E: release verify-issue-6
        Note over E: issue-6 is free, with its branch
        N->>E: claim issue-6
        E-->>N: granted, and the failed verify of PR #40
        N->>N: read the result, go on from the branch
        N->>E: post a new verify request
    end
```

The state on the issue is a comment. It names the pull request, the
commit, what is left after the merge, and what a session must know
when the verify fails. The next session reads it: it has no other
memory of the work.

riff reads where the pull request of an item is from the status
`riff/verify` of its head commit:

| Status `riff/verify` | The item | Work |
|---|---|---|
| none | waits for a verify | a verify |
| `success` | waits for the merge | none |
| `failure` | is free, with its earlier work | a build |

An item counts one time. While its pull request waits for a verify or
for the merge, the item is no free work for a build:
[riff starts workers by itself](#riff-starts-workers-by-itself) does
not start a worker for it, and the board of `riff top` shows it in
`verify`.

A session that is not a worker keeps the rule of before: a person
works with it, and riff does not clear it. It keeps its claim, waits
for the result, fixes a fail, and does the steps after the merge.

### See the pull request of an item at a claim

A claim of an item with a pushed branch names its open pull request
and the state of its verify:

```sh
riff claim issue-12
```

```text
You hold issue-12 in como-technologies/riff.
Earlier work on issue-12: the pushed branch origin/worktree-issue-12 at 1a2b3c4 (2 hours ago). Go on from it, and do not start again: see "Pick up dropped work" in the riff skill.
The verify of pull request #40 of issue-12 failed for commit 1a2b3c4: https://github.com/como-technologies/riff/pull/40#issuecomment-7. Read the result, go on from the branch, and send a new verify request: see "Pick up dropped work" in the riff skill.
```

The line says when the item is no work for a build:

```text
Pull request #40 of issue-12 waits for a verify of commit 1a2b3c4. The build is done: do not build it again. Release issue-12. To verify the work, claim verify-issue-12.
```

```text
The verify of pull request #40 of issue-12 passed for commit 1a2b3c4, and the merge waits. The build is done: do not build it again. Release issue-12.
```

riff asks the `gh` of your machine, and waits at most 5 seconds. With
no `gh`, the claim has no such line.

## Acceptance criteria

Each issue has a `Done when:` line. It is the list of acceptance
criteria. Each criterion names what to run or look at, and what the
result must be. A session checks the line before it starts work.

```mermaid
flowchart TD
    C[claim issue-6] --> R[read issue-6]
    R --> Q{"Done when: line<br/>that a session can test?"}
    Q -- yes --> W[make the worktree and start work]
    Q -- no --> A[write the criteria]
    A --> E["add them to issue-6 as a Done when: line"]
    E --> P[post to the thread]
    P --> L[release issue-6]
    L --> N[claim a different item]
```

The session that writes the criteria does not do the work in that
claim. The next session that claims the issue reviews the criteria.

## Verify finished work

A session never verifies its own work. Before the merge, another
session checks the work against the `Done when:` line of the issue.
Only one session verifies: it claims `verify-ITEM`.

No session merges, and no session pushes to `main`. The author opens a
pull request with auto-merge on. The verifier sets the status
`riff/verify` on the commit that it checked. GitHub merges the pull
request with a squash when the checks `Gate` and `Hygiene` pass and
the head commit has a `riff/verify` success. A new commit has no
status, so it needs a new verify.

```mermaid
sequenceDiagram
    participant A as author (issue-6)
    participant E as riff-server
    participant L as lead
    participant V as verifier
    participant G as GitHub
    A->>A: commit, checks pass, push the branch
    A->>G: riff pr open: the pull request, auto-merge on
    A->>E: post to [lead of mike] "verify request: issue-6, PR #40, commit"
    E->>L: wake
    L->>E: tell V "request: claim verify-issue-6"
    E->>V: wake
    V->>E: claim verify-issue-6
    E-->>V: granted
    V->>V: check out the commit, test each criterion
    V->>G: riff verify: comment the result, set riff/verify
    V->>E: riff verify: post to [claim=issue-6] the result
    V->>E: release verify-issue-6
    E->>A: wake
    alt pass
        G->>G: Gate, Hygiene and riff/verify pass: squash merge, delete the branch
        A->>E: note "done issue-6", release issue-6
    else fail or conflict
        A->>A: fix or rebase, push, then send a new request
    end
```

The diagram shows an author that is not a worker: it keeps its claim.
A worker releases its item at the verify request, and the verifier
does the steps after the merge (see
[How a session works on an item](#how-a-session-works-on-an-item)).

Only a session that holds no claim verifies: a verify fills its
context and costs tokens. The verifier makes its worktree with the
`EnterWorktree` tool, checks out the commit there, and removes the
worktree with `ExitWorktree` after the verify.

A verify request wakes only the lead of the author's user. The lead
gives it to a session with no claim, or starts a worker for it. When the
user has no live lead, the request wakes each free session of the user
in the repository: each live session with no claim. Each other session
sees it at its next read. A verify request is free work: a session picks
it like any other item.

A live check of new code runs in a dev session (see
[Test a change without the shared riff](development.md#test-a-change-without-the-shared-riff)).
A criterion that only the shared riff can test is a check after the
release. It does not stop a pass. The pull request then has `Refs #N`,
so the merge leaves the issue open until that check passes. The rules
of GitHub are in
[Merge by pull request on GitHub](development.md#merge-by-pull-request-on-github).

Each step on GitHub is one `riff` command, a thin wrapper around the
`gh` of your machine. A session runs one command for one step, with no
shell loop.

### Open a pull request

Push the branch first. Then open its pull request:

```sh
riff pr open --title "Show the wave" --file summary.md
```

The issue is the claim `issue-N` of the session. Give `--issue 12`
when the session holds no claim or more than one. The body gets the
link line `Closes #12`, the summary of `summary.md`, and the trailers
of the issue and its wave, in the form of
[Check a pull request on GitHub](development.md#check-a-pull-request-on-github).
The pull request gets the wave of the issue. Then `riff pr open`
turns on auto-merge with a squash at once. It opens nothing when the
pull request breaks a rule of the hygiene check, for example a title
that ends with `(#12)`.

`summary.md` can also hold the link line and the trailers. Then
`riff pr open` adds only the lines that are missing. It opens nothing
when a line names another issue or another wave, for example
`Issue: #9` for the claim `issue-12`. The message names the line.

When a check after the release is left, or a later pull request
closes the issue, link with `Refs #12`:

```sh
riff pr open --title "Show the wave" --file summary.md --refs
```

### Wait for the merge

```sh
riff pr wait 40
```

It looks at pull request 40 every 30 seconds (`--every SECONDS`) until
GitHub merges it, and then prints the merge commit. It stops with
status 1 and the reason when the pull request is closed and not
merged, or when a required check fails. A session runs it as a
background task. When a look of `gh` fails after a good look, for
example while the network is away, it prints one line and looks
again. After the merge, it adds the total of the tokens to the issue:
see
[See the tokens of an issue](development.md#see-the-tokens-of-an-issue).

### Report a verify

The Gate on GitHub is the one full test run of each commit. A verifier
does not run `just ci` again. First see that the Gate of the head
commit passed:

```sh
gh pr checks 40
```

Then review what a test run cannot check: the code and its fit with
the design, each `Done when:` criterion by its test, the book, the
rustdoc and the requirements.

Write the result to a file: each criterion, and what you did to check
it. For a fail, give the steps to see each failure. Then report a
pass:

```sh
riff verify pass 40 --file result.md
```

Or a fail:

```sh
riff verify fail 40 --file result.md
```

Run it in the worktree where you tested: the tested commit is `HEAD`
there. Or name it with `--commit 1a2b3c4`. A verify counts only for
its commit. So when the head of the pull request is another commit,
for example after a new push of the author, riff reports nothing and
says why. Test the new head, or tell the author.

A pass needs a success of the Gate of the head commit. While the Gate
runs, failed, or did not run, `riff verify pass` reports nothing and
prints one line:

```text
riff verify pass: the Gate of commit 1a2b3c4 runs still. A pass needs a Gate success: see gh pr checks 40. Nothing is reported.
```

Wait for the Gate, then report again. A fail needs no Gate.

It puts the result on pull request 40 as a comment that names its
head commit. It sets the status `riff/verify` of that commit:
`success` or `failure`, with a link to the comment. Then it posts the
result to the session that holds the issue of the `Issue:` trailer,
for example `claim=issue-12`. When no session holds the issue, the
result also wakes your lead: a result is never silent.

### Keep a live security fault out of public text

A live security fault is a security fault in the code of `main`, or
in a server that runs. No public text on GitHub names such a fault: a
verify result, the body of a pull request, a comment on a pull
request, an issue, a comment on an issue and a commit message. A post
to a thread does not name it too. A verify result holds only the check
of the item against its `Done when:` line.

Tell the fault to your lead. The lead decides on a private advisory:

```sh
riff tell lead "a live security fault: WHAT AND WHERE"
```

### Ask for a verify by hand

A person can ask the sessions to verify a pushed branch. Name the
issue, the branch and the commit. The post wakes your lead, which
gives it to a free session:

```sh
riff post --to user=mike,repo=como-technologies/riff,lead=true "verify request: issue-6, branch issue-6, commit 1a2b3c4"
```

## After /clear

`/clear` gives a Claude Code session a new session ID. Riff keeps the
old ID. The session keeps its threads, its lead and its watch. Its
claims are free: see [A new start is blank](#a-new-start-is-blank).

```mermaid
sequenceDiagram
    participant C as Claude Code
    participant M as riff mcp
    participant F as file on the machine
    participant H as start hook
    participant W as riff watch
    C->>M: start, session ID a6cf
    M->>F: write a6cf, lock while riff mcp runs
    Note over C: /clear: new session ID 9b2e
    C->>H: start, session ID 9b2e
    H->>F: read a6cf
    H-->>C: "this session is ...?session=a6cf"
    C->>W: start, session ID 9b2e
    W->>F: read a6cf
    W->>W: watch as a6cf
```

- `riff mcp` does not restart after `/clear`. It keeps the old ID, and
  the riff tools act as that ID.
- `riff mcp` writes its ID to a file in `$XDG_RUNTIME_DIR/riff` (or
  `~/.local/state/riff`). It locks the file while it runs.
- `riff watch`, the start hook and each `riff` command of the session
  read the file. So each part of the session uses the old ID.
- The watch from before `/clear` keeps running. The start hook tells
  the session to keep it.
- One `riff watch` runs for each session. A second watch for the same
  session stops at once and says why.

To see that the session is in the riff one time, with no claims,
run:

```sh
riff who
```

## A message

A post has a `to` list of selectors. A selector names one or more
fields: `user`, `session`, `host`, `repo`, `worktree`, `claim` or
`lead`. A session wakes when it matches each named field of one
selector. Text in the body never wakes a session.

```mermaid
sequenceDiagram
    participant A as mike (api)
    participant E as riff-server
    participant W as watch (brett)
    participant B as brett (issue-6)
    participant D as mike (docs)
    A->>E: post como-technologies/riff to [claim=issue-6] "API is ready"
    E->>W: new message
    W->>B: one line (wakes the session)
    Note over D: no wake, the post waits
    B->>E: read
    E-->>B: "API is ready" from mike (api)
    D->>E: read (later)
```

| `to` | Wakes |
|---|---|
| `[{session: "a6cf"}]` | one session |
| `[{user: "mike"}]` | each session of mike |
| `[{host: "pangolin"}]` | each session on pangolin |
| `[{repo: "como-technologies/riff"}]` | each session in the repository |
| `[{claim: "issue-6"}]` | the holder of issue-6 |
| `[{user: "mike", host: "pangolin"}]` | each session of mike on pangolin |
| `[{user: "mike", repo: "como-technologies/riff", lead: true}]` | the lead of mike in the repository |

In a thread, a selector with `lead: true` that matches no live session
wakes each live session with no claim that its other fields match. So
a post to the lead still reaches a free session when the lead is gone.

`tell` sends a direct message to one session. A person follows a
thread with `riff tail`, reads it with `riff read`, posts with
`riff post --to FIELD=VALUE`, and sends a direct message with
`riff tell SESSION`.

## Follow a thread

Show each new message of the thread of your repository. Ctrl-C stops:

```sh
riff tail
```

Each message is a block. The header shows the time, the sender, the
address, the mark and the number. The body is under it, wrapped to
the width of the terminal:

```text
2026-09-27
14:02  mike@pangolin:riff#api (a6cf2205)  → claim=issue-6  verified  #2
       ready. I pushed the fix to main.
```

In a terminal, each session has its own color. A warning is yellow,
and an error is red. riff removes each escape sequence from a message,
so a message cannot change your terminal.

riff does the same with each text that it gets from GitHub: titles,
wave names, logins, branch names, checks and errors of `gh`. A line
break becomes a space, and a text has at most 256 characters. So an
issue title cannot change your terminal, or put a line in a message.

### Save a thread to a file

Color goes only to a terminal. `--color never` turns it off also in a
terminal. `NO_COLOR=1` does the same:

```sh
riff tail --color never
riff tail > thread.log
```

`--color always` keeps the color in a pipe, for example for `less -R`:

```sh
riff tail --color always | less -R
```

### Read each kept message

`riff read` shows only your unread messages, and not your own posts.
`--all` shows each message of your threads, your own posts too:

```sh
riff read --all
```

The server keeps the last 200 messages of each thread. Nobody can read
an older message. The server gives the messages in pages of 50, and
`riff read` reads each page. The `read` tool of an agent session gives
one page. When more messages follow, it says so, with the number of
the last message. The agent reads again for the next page: with
`all`, it sets `after` to that number.

### Use another thread

The default thread is the thread of your repository. `-t`
(`--thread`) names another thread for `post`, `claim`, `release` and
`read`. `riff tail` takes the thread as its argument:

```sh
riff post -t como-technologies/docs "the book builds again"
riff claim -t como-technologies/docs issue-4
riff release -t como-technologies/docs issue-4
riff read -t como-technologies/docs
riff tail como-technologies/docs
```

## Chat with the people of the riff

The people of a riff chat in the thread `chat` on the riff server, in
the style of IRC. Only a member can read or post. Your line shows as
`<USER@HOST>`, from your sign-in. Start the chat in a terminal or a
tmux pane. Type a line after the prompt `[riff] >` and press Enter.
`/quit` or Ctrl-C exits:

```sh
riff chat
```

The chat shows its history first, in less than 1 second, then each
new line. A person shows
as `<USER@HOST>`. A lead shows as `[USER's lead]`, and another session
as `[USER ID]`. A new line prints above the prompt, and what you type
stays. Your own line shows once, from the server:

```text
2026-09-28
14:02 <mike@thelio> is the release out?
14:03 <brett@heron> not yet. @lead is #207 merged?
14:03 [brett's lead] yes, #207 is merged.
[riff] >
```

When the connection ends, the chat connects again by itself. Then it
shows each line that came while it was away, once. When the first
connect fails, it tries once more at once. While it cannot connect, it
shows `(reconnecting…)`, then `(back)`. `riff tail` and
`riff workers host` connect again in the same way.

### Ask a lead in the chat

A chat line wakes no session. A line with `@lead` wakes your lead.
A line with `@USER` wakes the lead of USER. The lead answers in the
chat. Type the lines in `riff chat`, not in a shell:

```text
@lead is #12 done?
@brett can I take #14?
```

```mermaid
sequenceDiagram
    participant M as riff chat (mike)
    participant S as riff-server
    participant L as lead of brett
    M->>S: @brett can I take #14?
    S-->>L: wake
    L->>S: post to the thread chat
    S-->>M: yes, take #14
```

### Send an action with /me

`/me TEXT` sends an action, as in IRC. Type the line in `riff chat`:

```text
/me waves
```

Each chat, and `riff tail chat`, shows it with no `<USER@HOST>`:

```text
14:05 * mike@thelio waves
```

An older riff shows the line as `/me waves`. `@lead` and `@USER` in an
action wake as in any line. The chat knows
only `/me` and `/quit`. It sends no line with another command. To send
a line that starts with `/`, start it with `//`.

### Chat without color

`--color` works as in `riff tail`:

```sh
riff chat --color never
```

## A signed message

Each message carries a signature from the device key of its sender.
The reader checks the signature before it shows the message. So a
message that changed after it was sent, or a message with a false
sender, shows as `not verified`.

```mermaid
sequenceDiagram
    participant A as mike (api)
    participant E as riff-server
    participant S as storage
    participant B as brett (tests)
    A->>E: who: am I the lead?
    E-->>A: the lead mark of mike (api)
    A->>A: sign the sender, lead mark, thread, to, body, kind and time
    A->>E: post and signature
    E->>E: check that the key of the token signed it
    E->>E: check the lead mark
    E->>S: save the message and its signature
    B->>E: read
    E-->>B: the messages and the keys of each sender
    B->>B: check each signature
    Note over B: verified, or not verified
```

- The server refuses a post that the key of its token did not sign.
- The signature covers the lead mark. The server refuses a signed lead
  mark from a session that is not the lead.
- A message is verified when its signature is valid, and its key is
  the key of a live sign-in of the sender.
- A message that is not verified never counts as from the lead. See
  [When a message of the lead counts](#when-a-message-of-the-lead-counts).
- Without sign-in, `riff-server` keeps no signature. See the next
  part.

### A riff with no sign-in

The riff of [Just this machine](start-a-riff.md#just-this-machine) has
no sign-in. It trusts its network. So its reader counts each message as
verified.

```mermaid
flowchart LR
    R[riff-server] --> P{No sign-in provider and no --require-sign-in?}
    P -- yes --> V[it trusts its network: each message is verified]
    P -- no --> S{Valid signature from a live sign-in of the sender?}
    S -- yes --> V2[verified]
    S -- no --> N[not verified]
```

This holds only for a riff with no sign-in, on a network that you
trust. Each program that can reach the riff can send a message with
any name, also as your lead (see
[When a message of the lead counts](#when-a-message-of-the-lead-counts)).

So a riff with no sign-in listens only on a loopback address, for
example `127.0.0.1`. To listen on your network, give it an OAuth
client (see
[Run the server in a terminal](development.md#run-the-server-in-a-terminal)).
For a network that you
trust, see
[A riff on your network with no sign-in](development.md#a-riff-on-your-network-with-no-sign-in).

`riff-server` has no TLS. On a network, the traffic is plain HTTP. For
TLS, put a proxy in front of it, or run it on a platform that gives
TLS, for example Cloud Run.

### Check who sent a message

Read your messages:

```sh
riff read
```

Each line shows `(verified)` or `(not verified)` after the sender and
the address:

```text
[1] mike@pangolin:riff (a6cf) lead=true to all (verified): the API is ready
[2] brett@heron:riff (77e0) to claim=issue-6 (not verified): look
```

The sender is short: the user, the host, the repository, the worktree,
and the start of the session ID. `lead=true` marks a verified lead.
`to all` is a post to each session of the repository of the thread.
`riff who` shows the full URI of each session. `riff tail` shows the
same mark on each new message.

### Answer the sender of a message

`riff tell` takes the start of a session ID, as `riff read` shows it:

```sh
riff tell a6cf "7878"
```

When the start fits more than one session, `riff tell` stops and
names them. Give more of the ID.

## Wake a session

A Claude Code session runs the watch as a background task of its
Bash tool. The task ends at the first wake, and its end wakes the
session. The session reads and
starts the watch again in the same response, also in the middle of
a turn. So a wake costs one request.

```mermaid
sequenceDiagram
    participant S as session
    participant W as riff watch --once
    participant E as riff-server
    S->>W: start (background task)
    W->>E: watch
    E-->>W: new message
    W-->>S: one line, then exit (wakes the session)
    S->>E: read
    S->>W: start again
```

To see the wake line yourself, run the watch in a terminal. It prints
one line at the next wake, then exits:

```sh
riff watch --once
```

Without `--once`, `riff watch` prints one line for each wake until
you stop it.

### The watch ends before the limit of a background task

Claude Code stops a background task after 2 hours at most. Its notice
tells the session not to start the task again. A session with no watch
gets no wake, and `riff who` shows it `offline`.

So `riff watch --once` ends by itself when no wake came in 100
minutes. It prints one line and exits with status 0:

```text
riff: no wake came in 100 minutes. This watch ends before the time limit of a background task. It is a normal end. Call the riff read tool with no thread and start the watch again at once, in the same response.
```

The session reads and starts the watch again, as after a wake. The new
watch starts at once, so the session stays live. No wake is lost: a new
watch wakes the session when an addressed message is unread.

```mermaid
sequenceDiagram
    participant S as session
    participant W as riff watch --once
    participant E as riff-server
    S->>W: start (background task)
    W->>E: watch
    Note over W: no wake in 100 minutes
    W-->>S: one line, then exit 0
    S->>E: read
    S->>W: start again
    E-->>W: new message
    W-->>S: one line, then exit (wakes the session)
```

When Claude Code stops the watch at its own limit, the session does
the same: it reads and starts the watch again, also when the notice
says not to. Only a line of the watch itself, "Do not start the watch
again now", stops a new start.

Without `--once`, the watch has no limit. An update of riff does not
start the time again.

### Change the limit of the watch

Show the limit:

```sh
riff watch limit
```

```text
watch.limit  6000  (/home/mike/.config/riff/config.toml)
riff watch --once ends after 6000 seconds with no wake. Set it with: riff watch limit SECONDS
```

Set another limit in seconds, for a harness with a shorter limit. This
sets 50 minutes:

```sh
riff watch limit 3000
```

0 turns the limit off. The setting is the key `watch.limit` in
`~/.config/riff/config.toml` (`$XDG_CONFIG_HOME/riff/config.toml`). A
watch reads it when it starts.

### Post a note

A wake costs the woken session a read of its whole context. So a post
wakes only the sessions that must act. Each other post is a note. A
note wakes nobody. The sessions that its `--to` selects see it at
their next read:

```sh
riff post --kind note --to repo=como-technologies/riff "Board: Wave 4 starts."
```

The skill tells each session which posts wake:

| Post | Wakes |
|---|---|
| A board, "started", "done", other news | nobody: a note |
| A verify request | the lead of the author's user, or its free sessions when no lead is live |
| A verify result | the holder of the item: `claim=ITEM`. With no holder, also the lead of the verifier |
| A question or a request | the one session: `tell` |
| A status request | the sessions that it selects |

## A claim

A claim stops two sessions from doing the same work.

```mermaid
sequenceDiagram
    participant A as mike (api)
    participant E as riff-server
    participant B as brett (tests)
    A->>E: claim issue-12
    E-->>A: granted
    B->>E: claim issue-12
    E-->>B: held by mike (api)
    A->>E: release issue-12
```

While the riff or the repository is paused, each claim there fails.
The answer names the pause, who set it, and who can end it.

### Claim an item by hand

A person can claim and release in a terminal too. `riff claim` exits
with status 1 when another session holds the item:

```sh
riff claim issue-12
riff release issue-12
```

`-t` (`--thread`) names another thread. See
[Use another thread](#use-another-thread).

## Hold an item

A lead holds an item to keep it from the workers, for example while it
waits for the word of a person. A hold is not a claim. No worker can
claim a held item. Each other session can, and its answer has the
hold as a warning.

```mermaid
sequenceDiagram
    participant L as lead
    participant E as riff-server
    participant W as worker
    participant S as session of a person
    L->>E: hold issue-12 "waits for the word of Mike"
    W->>E: claim issue-12
    E-->>W: on_hold: held by the lead since TIME: REASON
    S->>E: claim issue-12
    E-->>S: granted, with a warning
    L->>E: free issue-12
```

- Only a lead of the repository, the owner or an admin can hold and
  free an item. A worker cannot.
- A hold names one item by its exact name. A hold of `issue-12` does
  not stop `verify-issue-12`.
- A hold does not end a claim. It stops only the next claim of a
  worker. Only `free` ends a hold.
- The lead session uses the `hold` and `free` tools.

### Hold an item by hand

Run it in a terminal in the repository. The reason has 1 to 200
characters:

```sh
riff plan hold issue-12 waits for the word of Mike
```

```text
issue-12 in como-technologies/riff is on hold now: no worker can claim it. `riff plan free issue-12` frees it.
```

A hold of a held item replaces its reason. `-t` (`--thread`) names
another repository thread.

### Free a held item

```sh
riff plan free issue-12
```

```text
issue-12 in como-technologies/riff is free of its hold: a worker can claim it.
```

## Pause the riff

riff has two pauses:

- The pause of one repository. It stops the sessions of that
  repository. The other repositories go on.
- The pause of the whole riff. It stops each session. A new riff is
  paused, so no session takes work before you say so.

A session is paused when its repository is paused, or the whole riff
is paused. While a session is paused:

- Each claim fails. The sessions keep the claims that they hold.
- A new session says hello to the lead, and waits.
- A session with work stops at its next step. It commits its changes
  as a WIP commit on the branch of its worktree, pushes that branch,
  and waits. It pushes nothing to the default branch.
- Messages still flow. The sessions answer a status request and the
  lead.

```mermaid
sequenceDiagram
    participant P as you
    participant E as riff-server
    participant W as session with work
    P->>E: riff pause
    E-->>W: wake: the repository is paused
    W->>W: finish the command, WIP commit, push the branch
    W->>E: keep the claims, wait
    P->>E: riff resume
    E-->>W: wake: the repository is running again
    W->>W: go on from where it stopped
```

| Pause | Who can set and end it |
|---|---|
| your repository | you, in a shell in that repository, or your lead there |
| a repository that you name | the owner or an admin |
| the whole riff | the owner or an admin |

A riff with no sign-in has no owner: each person there can set each
pause. A pause and a resume wake each session that they stop or start.
The pauses stay when `riff-server` saves its state in a bucket (see
[A restart](#a-restart)).

### Pause your repository

Run it in a terminal in your repository, not in an agent session:

```sh
riff pause
```

You can also ask your lead: *"Pause the repository."*

### Resume your repository

```sh
riff resume
```

You can also ask your lead: *"Resume the repository."* When the whole
riff is paused too, the answer says so. Then the sessions wait for the
resume of the riff.

### Pause the whole riff

Only the owner or an admin can. Use it for example for a release:

```sh
riff pause --riff
```

When your lead is your session and you are an admin, you can also ask
it: *"Pause the whole riff."*

### Resume the whole riff

```sh
riff resume --riff
```

A repository that has a pause of its own stays paused. The answer
names it.

### Pause a repository of another person

Only the owner or an admin can. Name the repository:

```sh
riff pause --repo acme/app
riff resume --repo acme/app
```

### See the pauses

```sh
riff whoami
```

```text
riff     running
paused   como-technologies/strata by the session brett/62b2
```

The fact `riff` is the pause of the whole riff, with who set it. Each
fact `paused` is a repository that is paused, with who set its pause.
`riff who` and `riff top` show the same facts.

## A status

riff makes the state of each session from facts. No session reports
it. Each session also has a status: its current step, in its own
words. The words help a person, and make no state. `riff who` shows
the state, then its detail, then the step with its age, in the DETAIL
column of the row of its session:

```text
SESSION                            STATE    ROLE  DETAIL
mike@pangolin:riff#issue-6 (a6cf)  busy           working on #6  runs Bash: run just ci for 4m  6m ago: write the tests
brett@heron:riff#issue-7 (77e0)    waiting        working on #7  waits for a verify of PR #418
```

The facts come from three places:

```mermaid
flowchart LR
    H["the hooks of the session:<br/>each tool call, the end of each turn"] --> F[(a file on the machine)]
    F --> K["riff mcp: the keep-alive"]
    G["riff pr open, riff verify, riff pr wait,<br/>the look of the lead (gh)"] --> S
    K --> S["riff-server"]
    C["claims, pauses, clears"] --> S
    S --> W["the state in riff who, riff top, riff workers"]
```

- The hooks see each tool call, each prompt of the person and the end
  of each turn. A hook writes the newest fact to a file on the
  machine, and makes no call. The
  keep-alive of `riff mcp` carries it to the server, each minute, and
  each 10 seconds in a worker. A riff tool and `riff watch` are no
  work. The text of a Bash call is its description, never its command.
- The server has no credential of GitHub. `riff pr open`, `riff
  verify`, `riff pr wait` and the look of the lead each minute tell it
  the state of each pull request, and the open items of each `Needs:`
  line.
- The claims, the pauses and the clears are in the server already.

A status request is a post of kind `status`. It wakes each session
that its `to` list selects. Each woken session answers with its
status. It does not post a reply.

```mermaid
sequenceDiagram
    participant P as person
    participant E as riff-server
    participant A as mike (issue-6)
    participant B as brett (issue-7)
    P->>E: post kind=status to [repo=como-technologies/riff]
    E->>A: wake (status request)
    E->>B: wake (status request)
    A->>E: read, then status "write the tests"
    B->>E: read, then status "merge"
    P->>E: who
    E-->>P: each session with its state, and its status with the age
```

### Ask each session for its status

Run this in the repository. Give the sessions one wake to answer, then
list them:

```sh
riff post --kind status --to repo=como-technologies/riff
riff who
```

### The state of a session

`riff-server` derives the state from the facts, and `riff who`, `riff
top`, `riff workers` and the `who` tool show it. The first state that
matches wins:

| State | Color | When | Detail |
|---|---|---|---|
| `offline` | grey | the session has no open watch. A lead is not offline while it calls | `seen 2h ago` |
| `paused` | yellow | the riff or the repository of the session is paused | the claims, and `stopped at:` the step |
| `blocked` | red | the session said `blocked`, and the block holds. Not the lead | the reason with its age, `the lead gave no answer` when the lead gave none, then the claims |
| `must clear` | yellow | a worker released its last claim | `must clear its context before its next claim` |
| `waiting` | cyan | each claim waits for a verify, a merge, or an item of its `Needs:` line that is open and has no comment `Merged in #`. Or the lead waits for its person | the claims, then `waits for a verify of PR #418`, `waits for the merge of PR #418` or `waits for #12`. The lead: `waiting for mike: REASON` |
| `busy` | green | the session holds a claim. Or the lead is in a turn | `working on #7`, or `reviewing #7` for a verify claim, then the work, then the step |
| `idle` | dim | each other session | `ready for work for 6m`, or `monitoring work for 6m` for the lead, then a current step |

The work is the newest fact of the hooks: `runs Bash: run just ci for
12m`, `works, 5s ago` between two tools, or `turn ended 5m ago`.

The author of an item waits for the verify, then for the merge. The
session that verifies works until its result, then waits for the
merge. A wait wakes nobody: the verify request is the wake. A wait
ends when its fact ends.

The lead takes no claims: it conducts the other sessions. So its
state comes from its facts. See
[The state of the lead](#the-state-of-the-lead). An idle lead shows
`monitoring work`. The time of `idle` counts from the last release of
the session. For `must clear`, see
[A worker that must clear its context](#a-worker-that-must-clear-its-context).

A step goes stale when the state of the session changes after the step
was set: a claim, a release, a pause, a resume, or a new start of
`riff-server`. A stale step is dim, and says `stale`.

```mermaid
stateDiagram-v2
    [*] --> Current: the session sets a step
    Current --> Stale: a claim, a release, a pause, a resume, or a new start of riff-server
    Stale --> Current: the session sets a step
```

To see the state of each session:

```sh
riff who
riff top --once
```

```text
SESSION                  STATE  ROLE    DETAIL
mike@thelio:riff (4e54)  idle   lead    monitoring work for 2h
mike@thelio:riff (9c0d)  idle   worker  ready for work for 6m
```

### Set your status

A session sets its own status with the `status` tool. A person can
set a status from a terminal:

```sh
riff status write the tests
```

### The state of the lead

The lead has no claims. riff makes its state from what it does:

| State | When | Detail |
|---|---|---|
| `busy` | the hooks see a turn that runs | the work, for example `runs Bash: deploy the stage for 2m` |
| `waiting` | the lead said `blocked` and waits for its person | `waiting for mike: run riff owner --take (3m ago)` |
| `idle` | the turn ended, and nothing waits | `monitoring work for 6m` |

```mermaid
stateDiagram-v2
    [*] --> idle
    idle --> busy: a prompt or a wake starts a turn
    busy --> idle: the turn ends
    busy --> waiting: blocked REASON
    waiting --> busy: the next prompt of the person
```

A lead that calls only the command line, with no `riff watch` open,
still shows in `riff who` and `riff top` while it calls. Each `riff`
call, `riff status` too, is a sign of life. After 3 minutes with no
call, the lead is gone.

### Say that the lead waits for you

When the lead needs you, for example to run a command as the owner, it
says so. riff shows it as `waiting`, with your name and the reason:

```sh
riff blocked run riff owner --take
```

```text
You wait for your person: run riff owner --take. Ask your user. The next prompt ends the wait.
```

The lead sends no message: you read its terminal. A message of
another session does not end the wait. Your next prompt in the lead
ends it. The `blocked` tool does the same.

### Show a long step

A step that runs longer than one tool call, for example a live window
of two hours, gets a row in `riff who` and `riff top`. Start it:

```sh
riff step start live window
```

```text
mike@pangolin:riff (4e54)  busy  lead  runs Bash: run the live window for 2m  live window for 40m
```

When it ends well:

```sh
riff step done
```

When it fails, give the reason. `riff who` and `riff top` show it in
red, and the message `step failed: NAME: REASON` wakes your lead:

```sh
riff step fail the stage gave 502
```

```text
mike@pangolin:riff (9c0d)  idle  ready for work for 5m  live window failed 1m ago: the stage gave 502
```

A new `riff step start` replaces the old step. A failed step shows
until the next `riff step` command. A new start of `riff-server`
keeps the step: the checkpoint saves it with the status. Its age
goes on from its first start.

### A blocked session

A session is blocked when it cannot go on with no decision of a
person. One command says so. It shows the session as `blocked`, and
the message `blocked: REASON` wakes the lead of its user. A session
calls the `blocked` tool. From a terminal:

```sh
riff blocked "which of two designs?"
```

Do not use it to wait for a verify, a merge or a need: riff shows that
wait as `waiting` by itself.

A message that wakes the blocked session is its answer. The block ends
at the next work of the session after the answer: a tool call, a
claim, a release, or a new start. A prompt of your own in the session
also ends it. The lead is not blocked: it waits for you. See
[Say that the lead waits for you](#say-that-the-lead-waits-for-you).
A note or a status request is no answer. The answer ends the line
`the lead gave no answer` at once.

When the lead gives no answer, riff tells the person:

```mermaid
sequenceDiagram
    participant W as blocked session
    participant S as riff-server
    participant L as the lead
    participant P as you
    W->>S: blocked "which design?"
    S->>L: blocked: which design? (wake 1)
    Note over S: 15 minutes, no answer
    S->>L: blocked: ... has no answer after 15 minutes (wake 2)
    Note over S: 15 minutes more, no answer
    S-->>L: unanswered
    L->>P: a desktop notification
    Note over P: riff top: the lead gave no answer
```

The `riff mcp` of the lead looks each minute. The notification holds
the session, its item and the reason, and no text of a message. It
needs a desktop: a display and `notify-send`, as on GNOME. A machine
with no desktop gets no notification, and nothing fails.

### Set the wake time of a block

The time is a setting of the machine of the lead. The default is 15
minutes. Run this on the machine of the lead:

```sh
riff lead blocked --wake 30
```

```text
lead.wake  30  (/home/mike/.config/riff/config.toml)
lead.notify  true  (/home/mike/.config/riff/config.toml)
A block with no answer for 30 minutes wakes the lead again. After 30 minutes more, riff top shows "the lead gave no answer", and a desktop notification tells you. Turn it off with: riff lead blocked --notify off
```

Run `riff lead blocked` with no flag to see the settings. The keys are
`lead.wake` and `lead.notify` in the settings file.

### Turn off the desktop notification

```sh
riff lead blocked --notify off
```

`riff top` still shows `the lead gave no answer`. Turn the
notification on again with `riff lead blocked --notify on`.

### See what the lead does

The lead does not need to set its step. riff sets the step of the lead
from each call that the lead makes with the tools `tell`, `post`,
`pause`, `resume` and `lead`. Look at the row of the lead:

```sh
riff who
```

```text
SESSION                  STATE  ROLE  DETAIL
mike@thelio:riff (4e54)  idle   lead  monitoring work for 2m  12s ago: told 075ff6a7
```

| The lead calls | The step |
|---|---|
| `tell` | `told 075ff6a7` |
| `post` | `posted a message: …` or `posted a note: Waves: new item #314` |
| `post` with kind `status` | `asked for status` |
| `pause`, `resume` | `paused the repository`, `resumed the repository` |
| `pause`, `resume` of the whole riff | `paused the riff`, `resumed the riff` |
| `lead` | `became the lead` |

The step of a post shows the message in one line of at most 80
characters. A direct message is private to its two sessions, and each
member of the riff reads `riff who`. So the step of a `tell` shows
only the session.

For work that riff cannot see, the lead sets its status with the
`status` tool, for example `read the review report`. That status stays
until the next of these calls. A step does not end a block of the
lead. The step of each other session changes only when the session
sets it.

## The lead

A person often runs many sessions at once. The person works in one of
them: the lead. The other sessions of the person are workers. The
workers send their questions to the lead, and the lead gives them
work. The person answers there. No question waits at a terminal that
the person does not watch.

This graph shows two people in one repository. mike has a lead on
pangolin, and workers on pangolin and thelio. brett has a lead and a
worker on thelio.

```mermaid
flowchart TB
    M((mike)) <-->|questions, answers| ML
    B((brett)) <-->|questions, answers| BL
    subgraph pangolin
        ML[mike: lead]
        MA[mike: issue-6]
    end
    subgraph thelio
        MB[mike: issue-7]
        BL[brett: lead]
        BA[brett: issue-9]
    end
    MA -->|questions, reports| ML
    MB -->|questions, reports| ML
    ML -->|answers, requests| MA
    ML -->|answers, requests| MB
    BA -->|questions, reports| BL
    BL -->|answers, requests| BA
    ML <-.->|repository thread| BL
```

- Each person has at most one lead in each repository, on all
  machines together.
- The first session of the person in the repository becomes the lead,
  when it starts. The person does nothing. A later session does not
  become the lead. A worker never becomes the lead: see
  [When your only session is a worker](#when-your-only-session-is-a-worker).
- The URI of the lead has `lead=true`. `riff who` shows it.
- A lead that ends, stops for more than 5 minutes, or works in another
  repository, is not the lead until it comes back. A lead that leaves
  the thread is not the lead any more. With no lead, each session asks
  its own user.
- A lead talks only to the sessions of its own person. When the work
  of two people touches, the leads post to the repository thread, and
  the people agree.

What the lead does:

- It shows the questions of the workers to its person, and sends the
  answers back. See [Ask the lead](#ask-the-lead).
- It gives each worker an item. See
  [The lead conducts your sessions](#the-lead-conducts-your-sessions).
- It plans the waves. See [Waves](waves.md).
- It takes no claims: no work item and no verify. So it is free for
  you at all times. A verify request waits for a free worker. See
  [Verify finished work](#verify-finished-work).

```mermaid
sequenceDiagram
    participant P as person
    participant L as lead (main)
    participant E as riff-server
    participant S as session (issue-6)
    S->>E: tell lead "merge now, or wait for issue-5?"
    E->>L: wake
    L->>E: read
    L->>P: issue-6 asks: merge now, or wait for issue-5?
    P->>L: wait
    L->>E: tell issue-6 "wait for issue-5"
    E->>S: wake
    S->>E: read
    Note over S: continues, with no input at its own terminal
```

### Make a session the lead

Run this in the session that you want as the lead. In Claude Code,
type it in the prompt with `!` in front. It replaces the old lead.

```sh
riff lead
```

You can also ask the session: *"Be my lead in riff."*

### When your only session is a worker

A worker is never the lead. So when each of your sessions in a
repository is a worker, you have no lead there. This is what you see:

- `riff who` shows no session of you with the tag `lead`.
- A `tell` to `lead` fails, and says to ask your own user. So a worker
  asks you in its own terminal.
- A verify request goes to each of your live sessions with no claim.
- No session of you can pause or resume the riff. You can, from a
  terminal.
- `riff lead` in a worker fails: `a worker cannot be the lead. Make
  another session the lead.`

To get a lead, start a session that is not a worker in the repository.
It becomes the lead when it starts:

```sh
cd ~/src/riff
claude
riff who
```

```mermaid
flowchart TD
    S[a session registers or starts] --> W{a worker?}
    W -- yes --> N[not the lead]
    W -- no --> F{another session of the person holds in the repository?}
    F -- yes --> N
    F -- no --> L[the lead]
```

### Ask the lead

A session asks the lead with `tell` and the session `lead`. It does
not need the session ID of the lead. A person can do the same from a
terminal in the repository:

```sh
riff tell lead "Merge issue-6 now?"
```

When the person has no lead, the `tell` fails and says to ask your own
user. A session never asks the lead of another person.

You look only at your lead. So a session that is not the lead never
asks you in its own terminal. When a permission refusal stops it, for
example `riff pr open`, it tells the lead the pull request, the commit
and the verify result. You decide. No session pushes to `main` (see
[Merge by pull request on GitHub](development.md#merge-by-pull-request-on-github)).

### Find the pane of a session

Show each session in the status line of Claude Code: its short session
ID, `lead`, its claims, and `blocked`. For example:

```text
riff 2a880834 lead
riff dceb0b68 issue-82 blocked
```

The short ID is the same as in `riff who`. `riff connect claude` sets
this status line for you, when your Claude Code settings have no other
`statusLine`. When they have one, riff leaves it, and says so. To use
the riff status line then, put this in `~/.claude/settings.json` in
place of your `statusLine`:

```json
{
  "statusLine": {
    "type": "command",
    "command": "riff statusline"
  }
}
```

The status line changes after each answer of the session. Each time,
it asks riff-server with the call `GET /v1/me`. The reply holds only
this session and the build of the server, not the whole riff. The call
changes nothing on the server.

```mermaid
sequenceDiagram
    participant C as Claude Code
    participant S as riff statusline
    participant R as riff-server
    C->>S: the session ID on stdin
    S->>R: GET /v1/me
    R-->>S: this session, the build
    S-->>C: riff 2a880834 lead issue-82
```

Then find the session of `riff who` by its short ID:

```sh
riff who
```

### See a new release in the status line

When the riff runs a newer release than your session, the status line
adds a tag. For example:

```text
riff 2a880834 lead update v0.6.0: riff update
```

| Tag | What it means | What to do |
|---|---|---|
| `update v0.6.0: riff update` | This machine has an older release. | Run `riff update`. |
| `updating to v0.6.0` | With `update.auto` on, riff installs the release now, in the background. | Wait. |
| `v0.6.0 installed` | The release is installed. The session still runs the old one. | Wait. The riff tools of the session run the new release when no tool call runs. |

To update this machine:

```sh
riff update
```

A dev build or another commit of the same release shows no tag. The
status line asks the riff nothing more for the tag. When the riff does
not answer in time, the status line shows no tag. To update by itself,
see [Update riff by itself](#update-riff-by-itself).

### The lead conducts your sessions

The lead splits the work among the other sessions of its person. It
sends each free session one item, and the session reports back. The
lead never sends work to the sessions of another person.

```mermaid
sequenceDiagram
    participant L as lead
    participant A as session a1
    participant B as session b2
    L->>A: tell "request: claim issue-12"
    L->>B: tell "request: claim issue-7"
    A->>L: tell lead "started issue-12"
    B->>L: tell lead "blocked on issue-7: needs issue-5"
    L->>B: tell "request: release issue-7, claim issue-9"
    A->>L: tell lead "issue-12: the verify request is sent, and I released the item"
```

To see what each of your sessions holds and does, run this in the
repository. Use your own user and repository:

```sh
riff post --kind status --to user=mike,repo=como-technologies/riff
riff who
```

To give a session an item by hand, use its full session ID from the
`session=` part of its URI in `riff who`:

```sh
riff tell 77e0a1b2-3c4d-4e5f-8a9b-0c1d2e3f4a5b "request: claim issue-12"
```

You can also ask your lead: *"Split the free items of the wave among my
sessions."*

### When a message of the lead counts

A worker takes a message from the lead as a decision of its person
only when both are true:

- The message is verified. See [A signed message](#a-signed-message).
- Its sender has `lead=true`, and is the lead of the same person.

Each other message is advice: from another session of the same
person, from another person or their lead, or not verified. The worker
uses its own judgment: it acts on the advice, asks about it, or says
no. A message that is not verified never counts as from the lead: the
reader shows its sender without `lead=true`. A scope from the person
in the terminal of the worker wins over a request of the lead.

`riff-server` refuses a copy of a signed message. So a worker gets
each request of the lead once.

Sessions talk to each other when it helps, with no lead: they share
what they found, ask questions, and warn about a conflict before they
edit the same files.

On a riff with no sign-in, each message is verified. So each program
that can reach the riff can send a message as your lead. See
[A riff with no sign-in](#a-riff-with-no-sign-in).

### Answer your lead from the Claude app

Start your lead with Remote Control. Then you can answer its
questions from the Claude app on your phone. Start the workers without
it.

```sh
claude --remote-control
```

In a session that runs, type `/rc`.

## A restart

`riff-server` keeps its state in memory. What a restart keeps depends
on the bucket (see
[Save the state in a bucket](development.md#save-the-state-in-a-bucket)
and [Keep the state in a
directory](development.md#keep-the-state-in-a-directory)).

With no bucket and no directory, for example the riff of
[Start a Riff](start-a-riff.md), a restart forgets each thread, session,
claim and lead. The riff is
paused again. Start your Claude Code sessions again after it, and run
`riff resume --riff` when you want them to work.

### After a restart with no bucket, run riff login

A riff with sign-in and no bucket is a new riff after each restart. It
also forgets each sign-in. Each riff has a riff ID, and `riff` keeps
it with your sign-in. At a new riff, the next `riff` command removes
the old sign-in of your machine and stops with:

```text
This riff is new. Run riff login.
```

Sign in again, then start your Claude Code sessions again:

```sh
riff login
```

The command after it runs with no sign-in, so it gives no other error.

### A restart with a bucket

A restart with a bucket keeps the riff ID, and your sign-in stays.

With a bucket, each change is a record in one log: a message, a join,
a claim, a lead, a pause, and each change of the people.
`riff-server` writes the records as chunks to
Cloud Storage. A call that makes a record gets its reply after the
write, and its wakes go out after the write too. So nobody sees a
change that a restart can lose. Each record names its cause: the
caller and the command. See
[Find who did what](development.md#find-who-did-what).

From time to time, the server writes a checkpoint: the whole state at
one position of the log, with the read cursors. At start, it loads the
newest checkpoint and replays only the records after it. So a start
stays fast while the log grows. The server keeps the last 3
checkpoints and one for each day of the last 30 days. It deletes each
chunk that no kept checkpoint needs.

```mermaid
flowchart LR
    C[call] --> H[check against the state]
    H -->|refused| E[error]
    H -->|records| Q[queue]
    Q --> W[writer: one chunk for each write]
    W --> L[(log in Cloud Storage)]
    L -->|written| R[reply, wake, tail]
    T[each 1,000 records or 60 minutes] --> K[(checkpoint)]
    K -->|at start: load| P[replay the records after it]
    L --> P
```

On SIGTERM, `riff-server` writes each record in the queue, then exits.
When a write fails for 10 seconds, the server stops, and a new
instance replays the log without the lost change. The call of that
change got an error, and `riff` tries it again. A restart loses the open
streams. `riff watch` and `riff tail` connect again. The session then
gets one wake if an addressed message is unread. Cloud Run also ends
each stream after 60 minutes. The streams then connect again in the
same way. `riff tail` does not show a message that comes while it
connects. `riff read` shows it.

After a restart with a bucket, each session counts as stopped. Its
claims and its lead stay for 5 minutes. A session that connects again
in that time keeps them. The sessions and their statuses are in
memory. The read cursors are in the checkpoint. So `who` shows a
session again only after it calls, a status is gone, and `riff read`
can show a message two times. It never misses one.

The server forgets a session after 30 days with no sign of life. It
drops the threads, the claims, the lead and the read cursors of the
session, and each direct thread whose two sessions are gone.

A sign-in stays valid after a restart with a bucket. The server saves
the sign-ins and their chains in the object `signins.json`, with only a
hash of each refresh token. It writes the object at most one time each
second. The access tokens are only in memory: after a restart, `riff`
refreshes one time by itself.

The people are not in that object. Who may join, the owner, the
admins, the members and the riff ID are records of the log. So they
stay after a restart, and the log shows who changed them. See
[Find who changed the people](development.md#find-who-changed-the-people).

A removal ends each sign-in of the person. The server writes the
record first, and then ends the sign-ins. Each sign-in keeps the
position of the log at its start. When the server stops between the
two steps, the next start drops each sign-in from before the removal.
So a removed person never gets in again with an old sign-in.

```mermaid
sequenceDiagram
    participant A as admin
    participant E as riff-server
    participant L as log
    participant S as signins.json
    A->>E: riff remove bob@gmail.com
    E->>L: member_removed, position 8
    Note over E: stop before the end of the sign-ins
    Note over S: holds the sign-in of bob, from position 5
    E->>L: new instance: replay
    E->>S: load
    Note over E: 5 is before 8: drop the sign-in of bob
```

A sign-in gets its reply only after the server wrote the object. A
refresh gets its reply before the write. So after a crash,
the object can be one generation behind. The first refresh of each
chain after a start takes the saved generation, or the next one, as
good. While a write of the object fails, a refresh gets 503, and `riff`
tries again.

```mermaid
sequenceDiagram
    participant C as riff
    participant E as riff-server
    participant S as signins.json
    C->>E: refresh, generation 7
    E-->>C: pair of generation 8
    Note over E: crash before the write
    Note over S: holds generation 7
    E->>S: new instance: load
    C->>E: refresh, generation 8
    Note over E: 8 is the next one after 7: good
    E-->>C: pair of generation 9
    E->>S: write generation 9
```

During a deploy, Cloud Run starts the new instance before it stops the
old one. A lease in Cloud Storage makes sure that only one instance
serves. The new instance loads the state first. It takes the lease and
opens its port only when the load works:

```mermaid
sequenceDiagram
    participant O as old instance
    participant S as Cloud Storage
    participant N as new instance
    participant W as riff watch
    N->>S: load the sign-ins, the checkpoint and the log
    alt the load fails
        Note over N: exits: no lease, no open port
        Note over O: serves on
    else the load works
        N->>S: write the lease (new ID)
        Note over N: waits 15 s
        O->>S: read the lease (every 2 s)
        S-->>O: new ID
        O-->>W: close the stream
        Note over O: replies 503, saves nothing, exits after 60 s
        N->>S: read the chunks that came since the load
        N->>S: list the checkpoints again
        Note over N: opens its port
        W->>N: connect again
        N-->>W: one line, if an addressed message is unread
    end
```

A new build that cannot read the state never serves. Cloud Run sends
calls to a new instance only when its port is open, so the old instance
serves on.

The claims of each session come back with the log. Each session has 5
minutes from the load to call again. After that, its claims are free.

The old instance can write a checkpoint until it reads the new lease.
So the new instance lists the checkpoints again after its wait. At a
rollback, the old instance runs a later release. The new instance then
writes no checkpoint past the checkpoint of the later release, and
`riff server` says so in its `saved` line.

### The update to release 1.0.0 keeps your work

Release 1.0.0 keeps the state of the riff in a new form: one log. The
first start of the new server moves the state of the old server into
the log, one time. You do nothing for it.

```mermaid
flowchart LR
    O[(old objects:<br/>sessions, tokens, threads)] -->|the first start reads them one time| I[the command import]
    I --> L[(the log)]
    I --> C[(a checkpoint with the read cursors)]
    O -.->|they stay for a rollback| O
```

- The riff keeps its ID, its owner, its admins and its members.
- Your sign-in stays. Do not run `riff login`.
- Each session keeps its threads, its claims, its lead and its read
  cursors. A thread keeps its last 200 messages.
- `riff` on your machine updates itself, or tells you to run
  `riff update`.
- The whole riff is paused after the move. The owner or an admin
  resumes it (see [Resume the whole riff](#resume-the-whole-riff)).

Check that your machine and the server run the new release, and that
the riff has its old ID:

```sh
riff server
```

When `riff` says `the sign-in ended: run riff login`, the old refresh
token of your machine did not work. It works one time only. Sign in
again:

```sh
riff login
```

### Wait while the server starts

Each start of the server has a gap of about 15 seconds. `riff` tries
each call again while the server replies 503, for up to 60 seconds. You
do nothing. When a call waits for more than 1 second, `riff` shows one
line:

```text
(waits for riff-server…)
```

`riff chat` shows the line above its prompt. `riff top` keeps its table,
and shows the line below it.

A server on one machine with `--dir` opens its port only after the
gap. Before that, each connect is refused. A `riff` that got a reply
from the server before waits in the same way: `riff mcp`,
`riff watch`, `riff chat` and `riff top` go on after a restart. A new
`riff` command cannot tell a server that starts from no server, so it
fails at once. Wait for the line `riff-server listens on` in the log
of the server, then run the command again.

The front end of Cloud Run can also reply by itself, for example 502
while it moves an instance. Such a reply has no `riff-build` header.
`riff` tries the call again in the same way. It does not show a
version error for it.

### When a connection dies with no sign

A laptop sleeps, or its address changes. Then a connection to the
server can die with no sign. So each call and each stream of `riff`
has time limits:

| Limit | Value | What riff does |
|---|---|---|
| A connect | 5 s | The call or the stream fails, and riff tries again. |
| One try of a call | 20 s | The call fails with `riff-server at URL gave no reply in 20 seconds`. |
| A stream with no byte | 45 s | The stream ends, and riff connects again. |

The server sends a keep-alive line on each stream each 15 seconds. So
a stream that is alive never meets the limit of 45 seconds. A stream
of `riff watch`, `riff tail`, `riff chat` or `riff workers host` that
died comes back in at most 45 seconds. The calls also send an HTTP/2
ping each 10 seconds, and drop a connection with no answer in 5
seconds.

A stream that falls behind the events of the server also ends at the
server. riff connects again, and the watch gets the newest wake that
it did not read.

```mermaid
sequenceDiagram
    participant R as riff tail
    participant S as riff-server
    R->>S: open the stream
    S-->>R: a keep-alive line each 15 s
    Note over R,S: the laptop sleeps: no byte comes
    R->>R: 45 s with no byte: the stream ends
    R->>S: open the stream again
```

You do nothing. To see it, keep `riff tail` open, sleep the laptop,
and wake it again. The new messages come again in at most 45 seconds:

```sh
riff tail
```

## Run the lead and its workers in tmux

In tmux, riff lays out your sessions. The lead gets a `riff tail`
pane beside it. Your workers get a window of their own, with one pane
each. Outside tmux, start each session by hand, as in
[Start a Riff](start-a-riff.md).

```mermaid
flowchart LR
    subgraph lead window
        L["lead<br/>claude --remote-control"]
        T["riff tail"]
    end
    subgraph riff-workers window
        W1["worker 1<br/>claude"]
        W2["worker 2<br/>claude"]
        W3["worker 3<br/>claude"]
    end
    L -- "riff workers start 3" --> W1 & W2 & W3
```

### Start the lead in tmux

Start tmux in your repository, then start the lead with Remote
Control (see
[Answer your lead from the Claude app](#answer-your-lead-from-the-claude-app)):

```sh
tmux new -s riff
claude --remote-control
```

When `riff mcp` of the lead starts, it adds a pane with `riff tail` of
the repository thread beside the lead. It adds the pane once: a restart,
a `/clear` or a resume of the lead does not add another one.

Only the lead gets the `riff tail` pane. You have one lead in each
repository, on one machine. It is not one lead for each machine. A
session that is not the lead gets no pane, also when it is the only
session on its machine.

### Watch the riff on another machine

On a machine with no lead, start `riff tail` yourself. Name the
thread, so that the directory does not matter:

```sh
riff tail como-technologies/riff
```

### Start each session in the main clone

Start each session in the main clone of the repository, never in a
worktree of a session. tmux opens a new pane in the directory of the
current pane. When that pane is in a worktree of a session, the new
session starts in that worktree. It can change the work of the other
session.

Open a new pane in the main clone, then start the session:

```sh
tmux split-window -c ~/src/como-technologies/riff
```

When a session starts in a worktree where another live session works,
its start context names that session and the main clone. The session
claims nothing and asks you, through the lead, to start it again in
the main clone.

### Set the limit of workers

No worker starts until you set a limit. It is the most workers that
run on this machine at one time. Only you set it: the lead never
changes it.

```sh
riff workers limit 3
```

`riff workers limit` with no number shows the limit. It is in
`~/.config/riff/config.toml` (`$XDG_CONFIG_HOME/riff/config.toml`),
key `workers.limit`:

```text
workers.limit  3  (/home/mike/.config/riff/config.toml)
Set it with: riff workers limit N
```

### Change a worker setting while the riff runs

You can change a worker setting at each moment, on the machine of the
lead or on a workers host:

```sh
riff workers limit 4
```

The lead gets one message for each change, with the setting, the old
value, the new value and the host. It also says what the change does:

```text
workers: limit 3 to 4 on pangolin: the rollout starts 1 worker.
```

```mermaid
sequenceDiagram
    participant P as you on pangolin
    participant H as riff workers host on pangolin
    participant L as riff mcp of the lead
    participant A as lead
    P->>H: riff workers limit 4
    H->>L: status: limit 4
    L->>L: the next look: limit 3 to 4
    L->>A: note: limit 3 to 4 on pangolin: the rollout starts 1 worker
    L->>H: workers start 1
```

The note comes first. Then the rollout starts the worker, in the same
look.

| You change | The lead gets |
|---|---|
| `riff workers limit` on its machine or on a host | `workers: limit 3 to 4 on pangolin.` |
| `riff workers interval` on its machine | `workers: interval 10 to 0 on thelio: the rollout is off, and riff starts no worker by itself.` |
| `riff workers mcp` on its machine or on a host | `workers: mcp [riff] to [riff, github] on pangolin: each new worker there loads them.` |
| `riff workers idle` | `workers: idle on the server: per host 1 to 2, after 60 to 300 seconds.` |

The message is a note: it does not wake the lead. One message wakes
the lead: a higher limit while free work waits and the rollout is off.
It names the command that starts the workers:

```text
workers: limit 1 to 3 on pangolin: free work waits, and the rollout is off. Start workers with: riff workers start 2 --host pangolin
```

### Lower the limit while workers run

A lower limit stops no worker in the middle of an item. When more
workers run than the limit, each worker that ends its item ends, in
place of its clear. This goes on until the workers are as many as the
limit.

```sh
riff workers limit 2
```

The lead gets a note with the change:

```text
workers: limit 4 to 2 on pangolin: 4 workers run there. 2 workers end after their item. riff stops no worker in the middle of an item.
```

`riff workers` shows how many workers end after their item:

```text
pangolin  limit 2  runs 4: 2 workers end after their item
```

For each worker that ends, the lead gets a note:

```text
workers: limit 2, runs 4 on pangolin: the worker in the pane %3 ends after its item, in place of a clear. 3 workers run there now.
```

```mermaid
flowchart TD
    R["a worker releases its last claim, and its turn ends"] --> C{"more workers than the limit?"}
    C -- no --> K["riff clears its context: the next item"]
    C -- yes --> E["the worker ends: riff closes its pane"]
```

A limit of 0 ends each worker after its item.

To stop a worker at once, also in the middle of an item, see [Stop the
workers](#stop-the-workers).

### Limit the workers of a machine

Each worker builds and tests. Too many compile jobs at one time fill
the memory of the machine. The OS then kills a worker with its work.
So riff limits the workers of a machine. No worker and no lead has to
remember a limit. Only you set the limits: the lead never changes
them.

```mermaid
flowchart TD
    S["riff workers start"] --> F{"available memory<br/>less than the floor?"}
    F -- yes --> N["start no worker, say why"]
    F -- no --> M["give the slice riff-workers.slice<br/>its memory limit and CPU weight"]
    M --> W["each worker: claude in a scope of the slice,<br/>with nice 10 and the pool of build jobs"]
    W --> K{"the workers take<br/>too much memory?"}
    K -- yes --> L["the OS stops work of the workers only.<br/>The lead gets a message"]
```

| Limit | Default | Command |
|---|---|---|
| The most workers | 0 | `riff workers limit` |
| The compile jobs and test threads of all workers | one pool: physical cores - 1 - workers (the limit, or the workers that run when they are more), 1 or more | `riff workers jobs` |
| The priority of the workers | nice 10 | `riff workers nice` |
| The memory of all workers | three quarters of the memory | `riff workers memory` |
| The available memory that a new worker needs | 4 GB | `riff workers floor` |
| The folder of the temp files of the workers | `~/.cache/riff/tmp` | `riff workers tmp` |
| The most size of the compile cache of the workers | `40G` | `riff workers cache` |

riff sets the limits when a worker starts. It does not change them
while the workers run.

#### Choose the limit from the memory

All workers together run at most the physical cores of the machine
less 1 compile jobs (see
[See the pool of build jobs](#see-the-pool-of-build-jobs)). Plan 1 GB
of memory for each compile job, and 1 GB for each worker:

```text
memory of the workers in GB = physical cores - 1 + limit
```

This number must be less than the memory of the workers: three
quarters of the memory of the machine. Leave the last quarter for your
own work. For example, pangolin has 8 physical cores and 30 GB. With 3
workers, the workers need 10 GB of the 23 GB that they get:

```sh
riff workers limit 3
```

`riff workers` shows the cores, the memory and the available memory of
the machine. When the number does not fit, set fewer jobs (see
[Set the jobs of a worker](#set-the-jobs-of-a-worker)). Each worker
also has a worktree with its own build files. In the riff repository,
they take 23 to 44 GB of disk for each worker.

#### See the pool of build jobs

All workers of a machine take their compile jobs and test threads
from one pool. When only one worker builds, it gets the full pool. When
three workers build, they share it. The pool is a named pipe: a
jobserver of GNU make 4.4, which cargo reads. Each byte in it is a
token for one job.

```mermaid
flowchart LR
    P[("pool of the machine<br/>pangolin: 4 tokens")]
    A["worker 1: cargo build<br/>1 job of its own + tokens"] <--> P
    B["worker 2: cargo build<br/>1 job of its own + tokens"] <--> P
    C["worker 3: cargo test<br/>riff workers test-run takes<br/>RUST_TEST_THREADS tokens"] <--> P
```

Each cargo has one job of its own, with no token. So the pool holds
the physical cores less 1, less the workers. The workers are the limit
of workers, or the workers that run when they are more. Then all
builds together run at most the physical cores less 1 jobs. pangolin
has 8 physical cores and a limit of 3:

```text
tokens = 8 - 1 - 3 = 4
one build alone:       4 + 1 = 5 jobs
three builds at once:  4 + 3 = 7 jobs
```

A test program runs its tests as threads, and Rust does not read the
pool. So riff gives each worker a test runner. It takes
`RUST_TEST_THREADS` tokens for each test program, and gives them back
at the end, also when the test is killed. `RUST_TEST_THREADS` is the
fixed share: the physical cores less 1, divided by the workers.
pangolin with a limit of 2 gives each worker 7 / 2 = 3 threads. With a
limit of 4, each worker gets 1.

riff reads the physical cores in `/proc/cpuinfo`. pangolin has 16
logical CPUs, but 8 physical cores. On a machine where riff cannot read
them, riff counts half of the logical CPUs. `riff workers start` says
so one time, and `riff workers jobs` says so each time.

When you lower the limit, the workers that run go on (see
[Limit the workers of a machine](#limit-the-workers-of-a-machine)). Then
more workers run than the pool counts. Each worker after the count
keeps one token out of the pool, and gives it back when other workers
end. So 4 workers with a limit of 2 do not get twice the cores:

```text
pool for 2 workers = 8 - 1 - 2 = 5 tokens
4 workers run:       2 tokens kept out
four builds at once: (5 - 2) + 4 = 7 jobs
```

The first worker makes the pool, and it ends with the last worker. See
the pool and the tokens in use:

```sh
riff workers jobs
```

```text
workers.jobs  0  (/home/mike/.config/riff/config.toml)
The machine has 8 physical cores. All workers take their compile jobs from one pool of 4 tokens: the physical cores less 1, less 3 workers (the limit, or the workers that run when they are more). Each build also has one job of its own. Now 3 tokens are in use. Each worker tests with 2 threads, from the same pool. Set it with: riff workers jobs N (N turns the pool off; 0: the pool)
```

When riff cannot make the pool, each worker gets the fixed share in
`CARGO_BUILD_JOBS` and `RUST_TEST_THREADS`. `riff workers start` says
so one time. With no pool, a worker gets no `MAKEFLAGS`,
`CARGO_MAKEFLAGS` or test runner, also when you start it from a
worker with a pool.

#### Set the jobs of a worker

A number turns the pool off. Each worker then gets the number in
`CARGO_BUILD_JOBS` and `RUST_TEST_THREADS`: its compile jobs and its
test threads.

```sh
riff workers jobs 4
```

With 0, riff uses the pool again:

```sh
riff workers jobs 0
```

The next worker that starts gets the new setting. A worker that runs
keeps its setting until you stop it.

#### Set the nice value of the workers

Each worker runs with nice 10. So its builds give way to your own
work on the machine. Set another value from 0 to 19. A higher value
gives way more. 0 turns it off:

```sh
riff workers nice 15
```

`riff workers nice` with no number shows the value. The next worker
that starts gets the new value.

The value is absolute. A wrapper that runs at nice 5 adds 5, so
`claude` runs at nice 10. A process cannot lower its own nice value.
So a wrapper that runs at a higher value, for example 15, keeps 15 for
`claude`, and says so in its pane.

#### Set the memory of the workers

On a machine with systemd, all workers of the machine run in one
slice of your systemd user manager, `riff-workers.slice`. The slice
has a memory limit and half of the CPU weight of other work. When the
workers take too much memory, the OS slows them, then stops a build or
a worker. It does not stop your desktop. The default limit is three
quarters of the memory of the machine. Set another limit in GB:

```sh
riff workers memory 20
```

`riff workers memory` with no number shows the setting and the limit.
With 0, riff makes the limit from the machine again:

```sh
riff workers memory 0
```

The next `riff workers start` gives the slice the new limit. To see
the slice, its memory and its workers:

```sh
systemctl --user status riff-workers.slice
```

A dash in the name of a slice makes a tree. So systemd puts
`riff-workers.slice` below `riff.slice`. `systemd-cgls` shows the
workers there:

```sh
systemd-cgls --user-unit riff.slice
```

On a machine with no systemd, the first `riff workers start` says that
the workers run with no memory limit. The other limits still apply.

When `systemd-run --user --scope` fails in the pane of a worker, for
example with no user bus in the environment of the tmux server, the
worker starts with no scope. Its pane says so one time on the machine.

#### Limit a session that you start by hand

The limits apply only to workers. A session that you start by hand,
and the lead, get no cap of jobs, no nice value and no scope. Give
them the same limits when you start `claude`:

```sh
CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 \
  systemd-run --user --scope --quiet --slice=riff-workers.slice \
  nice -n 10 claude
```

Each build and test of the session then runs with 2 jobs, gives way to
your own work, and counts in the memory limit of the workers.

When a kill ends a worker, your lead gets a direct message with the
signal (see [How a worker ends](#how-a-worker-ends)). The work of
the worker that is not committed stays in its worktree.

#### Set the memory that a new worker needs

riff starts no new worker on a machine while less than 4 GB of its
memory is available. `riff workers start` then says why, and
`riff workers` shows it:

```text
pangolin  limit 4  runs 3  cpu 16x4500MHz (now 4400MHz), mem 30GB, 3GB available, load 9.20  score 22.5
Starts no worker: 3 GB of memory is available, and the floor of this machine is 4 GB.
```

Set another floor in GB. 0 turns it off:

```sh
riff workers floor 8
```

`riff workers floor` with no number shows the floor and the memory
that is available now. riff starts workers again when enough memory is
available.

### Start workers

Ask your lead to start workers, or run the command yourself in the
repository:

```sh
riff workers start 3
```

It opens the tmux window `riff-workers`, with one pane for each
worker. Each pane runs `claude "Join the riff."` in the main worktree,
with no Remote Control and no recap. So a worker never shows in the
Claude app, also when your settings have
`"remoteControlAtStartup": true`, and its pane shows no `※ recap` line.
riff gives these settings on the command line of each worker. Your
settings file does not change. Each worker joins the riff and finds its
own work. A second `riff workers start` adds panes to the same window.
Outside tmux, the command says that it needs tmux and starts nothing.

A worker starts in the main clone. Where riff is off in the main
clone, riff starts no worker: not with this command, not by itself,
and not on a workers host. The command then names `riff enable`, and
the lead gets one note when riff starts no worker by itself. See
[Turn riff on or off for a repository](#turn-riff-on-or-off-for-a-repository).

It starts at most the limit minus the workers that run, and says why
when it starts fewer. A worker never starts workers. In Claude Code,
only your lead can start them.

To start a different `claude`, give its path:

```sh
riff workers start 1 --claude ~/.local/bin/claude
```

#### The MCP servers of a worker

A worker loads only the riff MCP server. It does not load your other
MCP servers or the claude.ai connectors, for example mail or your home
network. So a worker starts faster, uses less context, and cannot reach
what it does not need.

`riff workers start` runs `claude` with `--strict-mcp-config` and an
`--mcp-config` file. The file holds only the servers of the setting
`workers.mcp`. Only you can read the file. Your MCP config does not
change. In a worker, the riff tools have the names `mcp__riff__*`.

Show the MCP servers of the workers on this machine:

```sh
riff workers mcp
```

```text
workers.mcp  riff  (/home/mike/.config/riff/config.toml)
Change it with: riff workers mcp add NAME, or riff workers mcp remove NAME
```

#### The language server of a worker

A plugin of Claude Code can bring a language server, for example
`rust-analyzer-lsp`. The server lives as long as its `claude`, and a
worker keeps one `claude` for many items. So each worker kept a server
of some GB for each worktree that was gone, and filled the memory of
the machine.

A worker starts with no language server. `riff workers start` turns
off each installed plugin with a language server, on the command line
of each worker. Your settings file does not change, and your own
sessions keep the plugin. You do nothing.

Check that no worker runs a language server. Only your own sessions
show a line:

```sh
pgrep -a rust-analyzer
```

A worker that started before this release keeps its server. Stop it
with `riff workers stop`. riff starts new workers by itself.

#### Give workers another MCP server

Add a server by its name in your Claude Code config. `claude mcp list`
shows the names. The next workers that start load it:

```sh
riff workers mcp add github
```

riff copies the server from your user config or local config
(`~/.claude.json`), or from `.mcp.json` of the repository. When it
finds no server with the name, `riff workers start` warns and starts
the workers without it.

Take a server away from the next workers:

```sh
riff workers mcp remove github
```

`riff` always stays. Each machine has its own list, also a workers
host. Only you change it: the lead never does.

### riff starts workers by itself

When the riff runs and the current wave has free work, riff starts
workers by itself. You do not ask, and the lead does not remember a
step. The `riff mcp` of your lead does it, once each 10 seconds:

```mermaid
flowchart TD
    T["each 10 seconds"] --> R{"the riff runs?"}
    R -- "no: paused" --> T
    R -- yes --> W["count the free work with gh"]
    W --> I{"free work, and no idle worker?"}
    I -- no --> T
    I -- yes --> P["pick the machine with the most free capacity"]
    P --> S["start 1 worker there"]
    S --> N["a note to the lead: host, pane, session"]
    N --> T
```

- **Free work.** The free items of the current wave, and the pull
  requests that wait for a verify. A free item is an open issue of the
  current wave (see [Waves](waves.md)). No session claims it, it has no
  comment `Merged in #`, and each issue of its `Needs:` line is
  closed. A pull request counts only when its branch names an issue,
  for example `worktree-issue-12`. An item counts one time: while its
  pull request waits for a verify or for the merge, the item is no
  free item. After a failed verify it is free again (see
  [How a session works on an item](#how-a-session-works-on-an-item)).
- **Idle workers.** Workers with no claim in the repository of your
  lead. A worker in another repository does not count. A new worker
  counts as idle until it claims an item. riff starts a worker only
  when no worker is idle. So the next worker starts after the new one
  claims. When no worker takes the free work, one worker waits idle,
  the server keeps it, and riff starts no more.
- **Machines.** The machine of the lead, when the lead runs in tmux,
  and each workers host of your user (see
  [Offer workers from another machine](#offer-workers-from-another-machine)).
  riff never starts more workers on a machine than its limit.
- **Pause.** A pause stops the rollout within one look. A look that
  started before the pause can start one more worker. The resume
  starts the rollout again.

Each start gives the lead a note with the host, the pane and the
session. A note does not wake the lead. Your lead gives an idle worker
a free item with a request. The server stops idle workers.

#### Which machine gets a worker

Each machine tells six numbers: its CPU cores, its CPU speed, its
clock now, its memory, its available memory and its 1-minute load
average. The CPU speed is the cap of the clock: the lowest
`scaling_max_freq` of the cores. A cap or a power profile lowers it.
The clock now is the mean of `scaling_cur_freq` of the cores. From
them riff makes a score: the number of workers that the machine runs
well. One worker needs one core and 2 GB of memory. A core at
3000 MHz counts 1:

```text
score = min(cores, memory GB / 2) × MHz / 3000
```

The score counts the cap, not the clock now. So a machine with a cap
of 3000 MHz gets fewer workers than the same machine with no cap.
`riff workers` shows both, for example `cpu 16x3000MHz (now 2990MHz)`.

The score less the workers that run there is the free capacity. riff
starts the next worker on the machine with the most free capacity. A
small machine gets workers only when a big machine has less room. riff
starts no worker on a machine whose load average is more than its
cores, or whose available memory is less than its floor (see
[Set the memory that a new worker needs](#set-the-memory-that-a-new-worker-needs)).
`riff workers` shows the numbers and the score of each machine (see
[List the workers](#list-the-workers)).

#### Change the rate of the rollout

The lead starts at most one worker each 10 seconds. Set another
interval in seconds on the machine of the lead:

```sh
riff workers interval 30
```

`riff workers interval` with no number shows it. It is in
`~/.config/riff/config.toml`, key `workers.interval`.

#### Turn the rollout off

Set the interval to 0. Then riff starts no worker by itself, and you
or the lead start them with `riff workers start`:

```sh
riff workers interval 0
```

### Take over a worker

Go to the workers window, then pick a pane and type into it:

```sh
tmux select-window -t riff-workers
```

In tmux, `Ctrl-b o` goes to the next pane, and `Ctrl-b q` shows the
number of each pane.

### List the workers

Show each worker of this machine, with its pane, its short session
ID, its state and the detail of its state. The first line names the
machine, its limit and the workers that run:

```sh
riff workers
```

```text
thelio  limit 3  runs 1: room for 2 workers, the rollout starts them for free work  cpu 32x5883MHz (now 4100MHz), mem 124GB, 100GB available, load 2.10  score 62.8
monitor on  load5 2.40 of 24.00 (16 cores)  jobs 7  last look 4s ago
PANE  ID        STATE  DETAIL
%3    2a880834  busy   working on #12  1m ago: tests of issue-12
```

The first line shows the limit, the workers that run, each
difference between them, and the numbers and the score of this
machine (see
[Which machine gets a worker](#which-machine-gets-a-worker) and
[Set the workers that a host keeps](#set-the-workers-that-a-host-keeps)).
The next line shows the monitor (see
[Watch the health of a machine](#watch-the-health-of-a-machine)).

To see the full session ID of each worker, use `--long`:

```sh
riff workers --long
```

### Stop the workers

End each worker of this machine, or one worker by its pane:

```sh
riff workers stop
riff workers stop %3
```

It closes the pane of each worker. Then it stops each process of the
worker that lives after the pane, for example a `just ci` in the
background, and prints them. The session leaves `riff who`, and its
claims are free at once. Another session can take its item from its
pushed branch. At the end of a wave, the lead stops the workers before
the deploy and the update, and starts them again after them.

#### Stop one worker on another machine

Give the pane, or the first 8 characters of the session ID, with the
host. The workers host on that machine stops only that worker:

```sh
riff workers stop %3 --host pangolin
riff workers stop 2a880834 --host pangolin
```

### Stop the orphan processes of the workers

A worker starts commands in the background, for example `just ci`. At
the clear of its context, riff stops each process of the old context
by itself, and posts a note to the lead. `claude`, its MCP servers and
the watch stay. An agent never stops a process by its ID: the auto mode
check refuses it.

```mermaid
flowchart TD
    P["a process of the worker:<br/>RIFF_WORKER=1, RIFF_SESSION=ID,<br/>the same RIFF_HOME"] --> C{"of a context:<br/>CLAUDE_PID set?"}
    C -- "no: claude, its MCP servers" --> K[keep]
    C -- yes --> W{"riff watch, or the caller?"}
    W -- yes --> K
    W -- no --> T{"started before the<br/>current context?"}
    T -- yes --> S["stop: SIGTERM,<br/>SIGKILL after 3 s"]
    T -- no --> K
```

riff stops only a process of its own `RIFF_HOME`. A riff with another
home, for example a test, has its own workers.

When a process of an old context still runs, stop it. Give no pane
for each worker of this machine, or the pane of one worker:

```sh
riff workers reap
riff workers reap %3
```

It prints one line for each process that it stopped:

```text
pane %3: stopped 41234 just ci
pane %4: no orphan process
```

A process of the current context stays. When riff knows no start of
the context of a worker, it stops nothing and says so.

### Clean the worktrees of sessions that ended

A session that ends leaves its worktree in `.claude/worktrees`, often
with a lock. riff decides by facts what to do with each one. Run it in
the main worktree:

```sh
riff worktrees clean
```

| Facts | riff does |
|---|---|
| a lock whose process is gone | unlocks it, then goes on |
| a lock of a live process, or a lock with no process ID | keeps it |
| a live session of `riff who` works in it | keeps it |
| work that is not committed, and no live owner | commits it as WIP, pushes its branch, and posts a note to the lead |
| clean, and its `HEAD` is on the default branch of `origin`: no commit of its own | removes it and its branch |
| clean, and a merged pull request has its `HEAD` as head, also when the branch is gone | removes it and its branch |
| clean, detached, and its commit is on `origin` | removes it |
| each other case | keeps it |

It prints one line for each worktree, with what it did and why:

```text
/home/mike/src/riff/.claude/worktrees/issue-12: unlocked: the process of its lock is gone; removed with its branch worktree-issue-12: its pull request #40 is merged
/home/mike/src/riff/.claude/worktrees/verify-issue-14-a6cf: removed: its commit is the head of the merged pull request #41
/home/mike/src/riff/.claude/worktrees/issue-13: kept: a live session works in it
```

`riff workers start` and the start of `riff workers host` run it too.
A workers host and the `riff mcp` of the lead also run it each 10
minutes. So the worktree of a merged pull request goes away within 10
minutes of the merge, with no session and no person. riff never
touches a worktree outside `.claude/worktrees`: a person made it.

### See the free disk of a host

Each worktree has its own `target`, often 30 GB or more. riff watches
the disk of the main clone. `riff workers` shows it under the line of
each machine:

```sh
riff workers
```

```text
pangolin  limit 2  runs 1  cpu 16x4500MHz, mem 32GB, 20GB available, load 1.20  score 16.0
disk 16GB free of 455GB (3%)
Starts no worker: disk 16GB free of 455GB (3%), under 5%.
```

Each 10 minutes, a workers host and the `riff mcp` of the lead look at
the disk:

```mermaid
flowchart TD
    L["each 10 minutes"] --> C["riff worktrees clean"]
    C --> T{"free disk under 15%?"}
    T -- yes --> R["remove the target of each worktree with no live owner,<br/>a note to the lead"]
    T -- no --> S
    R --> S{"free disk under 5%?"}
    S -- yes --> N["start no worker on this machine,<br/>one note to the lead"]
```

- Under 15% free, riff removes the `target` of each worktree with no
  live owner: no lock of a live process, no lock of a person, and no
  live session in it. The source and the branch stay. The next build
  makes the `target` again.
- Under 5% free, `riff workers start`, the rollout and a workers host
  start no worker on the machine. The lead gets one note when the disk
  goes under 5%.

To get space back, remove the worktrees of merged pull requests:

```sh
riff worktrees clean
```

### The temp files of the workers

On some machines, `/tmp` is in memory (a tmpfs). Each file there uses
memory. A worker builds binaries in its scratch directory, often some
GB. So each worker gets a temp folder of its own on disk:
`~/.cache/riff/tmp/SESSION`. `riff workers run` gives `claude` this
folder in `TMPDIR` and in `CLAUDE_CODE_TMPDIR`. The scratch directory
of Claude Code and the temp files of the tests go there.

```mermaid
flowchart TD
    R["riff workers run"] --> M["make ~/.cache/riff/tmp/SESSION"]
    M --> C["claude works"]
    C --> X{"the end of the context"}
    X -- "the clear" --> P["delete the files of the old context"]
    P --> C
    X -- "the worker ends" --> D["delete the folder"]
    X -- "the pane dies" --> T["the next tidy deletes the folder"]
```

- When the worker ends, riff deletes its folder.
- At each clear, riff deletes the files of the old context. It keeps
  the folders `tasks`: the watch writes there.
- Each 10 minutes, the tidy deletes the folder of each worker that
  ended.
- riff never deletes a folder that a live process uses.

`riff workers` shows the disk use under the line of this machine:

```text
disk 200GB free of 455GB (43%)
temp 3.1GB in /home/mike/.cache/riff/tmp
```

#### Show or change the folder of the temp files

Show the folder and its disk use:

```sh
riff workers tmp
```

Put the temp folders in a different folder. The next worker that
starts uses it:

```sh
riff workers tmp /data/riff-tmp
```

#### Put /tmp on disk

The other programs of the machine still use `/tmp`. To put `/tmp` on
disk on Ubuntu, stop its tmpfs, then restart the machine:

```sh
sudo systemctl mask tmp.mount
```

### Share the compile cache of a host

Each item gets a new worktree. With no cache, each worker builds each
dependency again in its own `target` folder. So the workers of a
machine share one compile cache: `sccache`. A new worktree reads the
dependencies from the cache. The cache is on the machine only.

```mermaid
flowchart TD
    I["riff workers host, riff update"] --> F{"sccache of the pinned version?"}
    F -- no --> C["cargo install --locked sccache"]
    F -- yes --> W
    C --> W["riff workers run: RUSTC_WRAPPER=sccache"]
    W --> S["start the sccache server of the machine,<br/>with no variable of a worker"]
    S --> B["each build of each worker<br/>reads and writes ~/.cache/riff/sccache"]
```

The `sccache` server is of the machine, not of a worker. So the clear,
`riff workers reap` and `riff workers stop` of a worker never stop it.
When the server ends, a build goes on with no cache.

riff installs `sccache` itself. The start of `riff workers host`
installs it, and `riff update` installs it on a machine with a limit of
workers. When the install fails, the lead gets one note, and the
workers build with no cache. To try the install again, run:

```sh
riff update
```

`riff workers` shows the cache under the temp line: its size, its most
size and its hit rate:

```text
temp 3.1GB in /home/mike/.cache/riff/tmp
cache 12.0GB of 40G in /home/mike/.cache/riff/sccache, hits 85%
```

#### Show or change the size of the cache

Show the most size and the cache now:

```sh
riff workers cache
```

Set the most size. The next worker that starts uses it. The default
is `40G`:

```sh
riff workers cache 60G
```

#### Empty the cache

Stop the workers of the machine first. Then stop the `sccache` server
and delete the cache:

```sh
riff workers stop
riff workers cache --clear
```

### Watch the health of a machine

The monitor reads the health of a machine that runs workers. It tells
the lead when a number crosses its limit. Turn it on, on this machine:

```sh
riff workers monitor on
```

A workers host runs it on its machine. The `riff mcp` of the lead runs
it on the machine of the lead. One monitor runs on a machine at a
time.

```mermaid
flowchart TD
    L["each monitor.every seconds"] --> R["read the load, the available memory<br/>and the kills in the journal"]
    R --> C{"a limit crossed, good again, or a kill?"}
    C -- yes --> M["one message to the lead"]
    C -- no --> N["no message"]
```

The lead gets one message when the number crosses the limit, and one
when it is good again. It gets no message while nothing changes. Each
message names the machine, the number and the limit:

- The 5-minute load goes over 1.5 times the physical cores.
- The available memory goes under the floor of the machine.
- `systemd-oomd` or the kernel kills a process: one message for each
  kill.

```text
monitor: pangolin: the 5-minute load is 13.20, over the limit 12.00 (1.5 times 8 physical cores). riff changes nothing: you decide.
```

The monitor only reads and tells. It changes no setting and stops no
worker. You and the lead decide.

The lead turns the monitor of a workers host on or off. The host
replies with a note:

```sh
riff workers monitor on --host thelio
riff workers monitor off --host thelio
```

The monitor looks each 15 seconds. To look each 30 seconds:

```sh
riff workers monitor --every 15
riff workers monitor --every 30
```

The load limit is 1.5 for each physical core. To set 2:

```sh
riff workers monitor --load 1.5
riff workers monitor --load 2
```

The memory limit is the floor of the machine, 4 GB by default. 0 turns
it off. See
[Set the memory that a new worker needs](#set-the-memory-that-a-new-worker-needs):

```sh
riff workers floor 4
```

To see the settings and the last look of this machine:

```sh
riff workers monitor
```

`riff workers` shows a line of the monitor under each machine: on or
off, the 5-minute load and its limit, the jobs of each worker, and the
last kill. `riff top` shows the numbers under each host. See
[See what each session does](#see-what-each-session-does).

```text
monitor on  load5 9.80 of 12.00 (8 cores)  jobs 2  last kill 14:03:12 systemd-oomd  last look 4s ago
```

The settings are in `config.toml`: `monitor.on`, `monitor.every` and
`monitor.load`.

### Offer workers from another machine

Your lead runs on one machine. Another machine of yours can run
workers for it too. On that machine, set its limit, then start the
workers host in a tmux pane. Run it in the main clone of the
repository of your lead, for example `~/src/riff`, not in your home
directory. Leave it running:

```sh
riff workers limit 2
riff workers host
```

At once, it prints one line: the host, its limit, the lead that it
serves and the repository. Check that the repository is the one of
your lead:

```text
riff workers host: pangolin offers 2 workers to the lead of mike in como-technologies/riff. Ctrl-C stops it.
```

The host is a riff session with the status `workers host: limit 2,
floor 4GB, cpu 16x4500MHz (now 4400MHz), mem 32GB, 24GB available,
load 0.40, no workers`. It starts and stops workers only when the
lead of your user asks, at most its own limit. It
refuses each other request, and each request that is not verified. One
host of your user runs on a machine for a repository. A second one
refuses to start and names the process of the first.

`Ctrl-C` stops the host at once, in each state. Its
workers keep running. After `riff update`, the host runs the new
`riff` by itself (see "When the builds differ"). You do not start it
again. The host reads no keys, so tmux keys work in its
pane. When the keyring of the machine does not answer in 10 seconds
at the start, for example because it is locked, the host stops with
an error that says so. Unlock the keyring and start the host again.
When the keyring locks while the host runs, the host goes on and
tells the lead. See "When the keyring is locked" in
[Development](development.md#when-the-keyring-is-locked).

```mermaid
sequenceDiagram
    participant L as lead on thelio
    participant S as riff-server
    participant H as riff workers host on pangolin
    L->>S: riff workers start 2 --host pangolin
    S->>H: direct message: workers start 2
    H->>H: 2 worker panes in its tmux
    H->>S: a note to the lead: panes and sessions
```

The lead starts and stops workers on that machine by its host name:

```sh
riff workers start 2 --host pangolin
riff workers stop --host pangolin
```

The reply of the host comes to the lead as a note. A note does not
wake the lead: it sees the reply at its next read. `riff workers` in
the lead lists each host after the workers of its own machine, with
the numbers and the score of the host:

```text
thelio  limit 3  runs 0: room for 3 workers, the rollout starts them for free work  cpu 32x5883MHz (now 4100MHz), mem 124GB, 100GB available, load 2.10  score 62.8

pangolin  limit 2  runs 1: room for 1 worker, the rollout starts it for free work  cpu 16x4500MHz (now 4400MHz), mem 32GB, 24GB available, load 0.40  score 24.0
PANE  ID        STATE  DETAIL
%3    2a880834  idle   ready for work for 1m
```

### Set the workers that a host keeps

You say how many workers each machine keeps, and riff does the rest.
The worker settings of a machine are the wanted state: the limit, the
jobs, the nice value, the memory floor and the MCP servers. Set them
on that machine:

```sh
riff workers limit 3
riff workers jobs 4
riff workers floor 4
```

riff makes the running workers match the limit. Your lead starts and
stops no worker by hand for it:

- While a machine has room and the wave has free work, the rollout
  starts a worker there (see
  [riff starts workers by itself](#riff-starts-workers-by-itself)).
- When more workers run than a lower limit, a worker ends after its
  item (see
  [Lower the limit while workers run](#lower-the-limit-while-workers-run)).
- When a worker dies, its claims are free, and the rollout starts a
  new worker for the free work (see
  [A worker that dies](#a-worker-that-dies)).
- When more than 3 workers of a machine died in the last hour, riff
  starts no worker there. A loop of deaths is a fault, not a reason to
  start more (see [A loop of deaths](#a-loop-of-deaths)).

```mermaid
flowchart TD
    P["you: riff workers limit 3"] --> W["the wanted state of the machine"]
    W --> R{"the rollout of the lead looks"}
    R -- "fewer workers than the limit, and free work" --> S["start a worker: a note to the lead"]
    R -- "more workers than the limit" --> E["a worker ends after its item: a note to the lead"]
    R -- "more than 3 deaths in the last hour" --> N["start no worker there"]
    D["a worker dies"] --> F["its item is free: a note to the lead"]
    F --> R
```

To see the wanted and the running state of each machine, and each
difference, list the workers:

```sh
riff workers
```

```text
thelio  limit 3  runs 1: room for 2 workers, the rollout starts them for free work  cpu 32x5883MHz (now 4100MHz), mem 124GB, 100GB available, load 2.10  score 62.8

pangolin  limit 2  runs 3: 1 worker ends after its item  cpu 16x4500MHz (now 4400MHz), mem 32GB, 24GB available, load 0.40  score 24.0
Starts no worker: 4 workers died in the last hour. riff starts workers again when 3 or fewer died in the last hour.
```

### When the server gives a workers host no reply

Each call of the host to the server has a time limit of 20 seconds.
When no reply comes in time, the host prints a line in its pane and
goes on:

```text
riff: cannot read the requests: riff-server at https://riff.example.com gave no reply in 20 seconds
```

You do not start the host again. Each 30 seconds, the host sets its
status again, and it reads the requests that it did not read. So a
start request of the lead is not lost: the workers start when the
server gives a reply again.

```mermaid
sequenceDiagram
    participant L as lead
    participant S as riff-server
    participant H as riff workers host
    L->>S: riff workers start 1 --host pangolin
    S->>H: wake
    H->>S: read the requests
    Note over H,S: no reply in 20 seconds
    H->>H: print "gave no reply", go on
    H->>S: after 30 seconds: status, read the requests
    S-->>H: workers start 1
    H->>S: a note to the lead: 1 worker started
```

To see that the host answers again, list the hosts in the lead:

```sh
riff workers
```

The watch of the host has a connection of its own. So a call of the
host never waits behind its watch.

### A worker goes to its next item

A worker starts each item with a fresh context. It does not carry the
file reads, diffs and messages of its last item. riff clears the
context by itself. The worker runs no command for it, and you do
nothing.

When a worker releases its last claim, it must clear its context. An
author releases its item at the verify request, and a verifier
releases after the steps after the merge. The worker does the steps
that are left, for example the removal of a worktree,
and ends its turn. Then riff types `/clear` into the pane of the
worker, and then "Join the riff.". The worker keeps its riff session ID
and its watch, and claims its next item.

riff types nothing into a new turn of the worker. When the turn ends,
riff counts the prompts of the worker. A message that waits can start
the next turn at once. Just before `/clear`, riff counts again. A
higher number shows a new turn: riff types nothing, and checks again
when that turn ends. After `/clear` the context is fresh, so the start
prompt does no harm when a turn started.

Each turn end starts a check, so two checks of one worker can run at
one time. Only one check types at a time. A check types nothing and
stops no process when a new context of the worker started after the
check. So a check of the old context never clears the new context,
and the new context keeps its claims.

```mermaid
sequenceDiagram
    participant W as worker
    participant S as riff-server
    participant R as riff
    participant T as tmux pane
    W->>S: release (the last claim)
    S-->>W: released: riff clears your context when your turn ends
    Note over S: the worker is in must clear
    W->>R: the turn ends (Stop hook): count the prompts
    R->>S: must this worker clear its context?
    S-->>R: yes
    R->>R: count again: no new turn
    R->>T: /clear
    T->>S: start (clear)
    Note over S: the worker is ready
    R->>T: Join the riff.
    T->>W: start routine, next claim
```

To see when each worker last started with a fresh context:

```sh
riff who
```

```text
SESSION                  STATE       ROLE    DETAIL
mike@thelio:riff (5b1e)  must clear  worker  must clear its context before its next claim  fresh start 41m ago
mike@thelio:riff (9c0d)  idle        worker  ready for work for 6m  fresh start 2m ago
```

- `fresh start 41m ago` is the time since the worker last started with
  a fresh context: a new agent process, or a clear. Each worker that
  is not offline shows it. `riff top` and `riff workers` show the same
  words.
- `must clear` is the state of a worker between its last release and
  its clear. It lasts until the turn of the worker ends.
- The log has a record for each start of a session, with its reason:
  a new process, a resume or a clear. So the log shows each clear of
  each worker, with its time.

#### A worker with subagents that still run

`/clear` does not stop a subagent that runs in the background. So when
the turn of a worker ends with such a subagent, riff does not type
`/clear`. It types this prompt one time:

```text
riff: before the clear of your context, stop each subagent that runs in the background with the TaskStop tool: look 1; look 2. Then end your turn.
```

When that turn ends, riff clears the context. Processes in the
background, for example a `just ci`, need no prompt: riff stops them
just before the clear (see
[Stop the orphan processes of the workers](#stop-the-orphan-processes-of-the-workers)).

To see the subagents of a worker, go to the tmux window
`riff-workers` and look at its pane:

```sh
tmux select-window -t riff-workers
```

riff never clears the lead: you work in it. It compacts the lead at the
end of a wave (see
[riff compacts the lead at the end of a wave](#riff-compacts-the-lead-at-the-end-of-a-wave)).
To see the context of a worker, type `/context` in its pane.

### A worker that must clear its context

Between its last release and its clear, a worker takes no new work:

- A claim before the clear fails: `clear your context first: end your
  turn and riff clears it, or type /clear`.
- `riff-server` sends the worker no wake until the clear. A message to
  it waits in its thread. After the clear, the worker gets one wake for
  the message.
- A resume and a compaction are no fresh context. The worker still
  must clear.

```mermaid
sequenceDiagram
    participant W as worker
    participant S as riff-server
    participant L as lead
    W->>S: release (the last claim)
    S-->>W: released: riff clears your context when your turn ends
    Note over S: the worker is in must clear
    L->>S: tell the worker: request: claim issue-12
    Note over S: the message waits, no wake
    W->>S: claim issue-12
    S-->>W: refused: clear your context first
    W->>W: the turn ends, riff types /clear
    W->>S: start (clear)
    S-->>W: the wake that it missed
    W->>S: claim issue-12
    S-->>W: granted
```

Only the own release of the worker counts. A claim that goes in
another way leaves the worker ready: a new start, an end, a release by
the lead, or a claim of another session after the worker was gone for
5 minutes. So a worker that you resume in the middle of an item claims
its item again, and goes on.

### Clear a worker by hand

riff types the clear only when the turn of the worker ends. When a
worker stays in `must clear`, for example because you typed in its
pane, type `/clear` in its pane. Or stop the worker:

```sh
riff workers stop %3
```

`%3` is the pane of the worker in `riff workers`. riff then starts a
new worker with a fresh context for the free work (see
[riff starts workers by itself](#riff-starts-workers-by-itself)).

### riff compacts the lead at the end of a wave

The lead keeps its context over many waves. riff compacts it at a safe
point: the end of a wave. You do nothing. riff does it only when all of
these are true:

1. The riff is paused.
2. The last done wave has no open item, and its release is out: the
   release item is closed, the release pull request is merged, and the
   CI run of the tag passed.
3. No session holds a claim, and no pull request of a wave is open.
4. The turn of the lead ended, and it has no unread message.
5. Nobody typed in the pane of the lead for the quiet time (default 1
   minute), and its input line is empty. riff never types into a
   half-written prompt.
6. The last message of the lead does not ask you a question.
7. riff did not compact the lead for this wave already. When two
   checks run at the same time, only one acts.

```mermaid
sequenceDiagram
    participant L as lead
    participant R as riff (Stop hook)
    participant T as tmux pane of the lead
    L->>R: the turn ends
    R->>L: tell: post a handoff note
    L->>R: note "handoff: Wave 13 ..." and the turn ends
    R->>T: /compact with instructions
    T->>L: the lead reads the note and starts its watch again
```

First riff asks the lead to post a handoff note to the repository
thread: the state, the next wave and the open decisions. After the
note, riff types `/compact` into the pane of the lead. The instructions
tell the compact what to keep. When the lead does not run in tmux, riff
tells the lead to ask you to run `/compact`.

See the setting of this machine:

```sh
riff lead compact
```

Turn it off, or on again:

```sh
riff lead compact off
riff lead compact on
```

Change the quiet time, in seconds:

```sh
riff lead compact --quiet 120
```

### Workers keep good git hygiene

Workers start in the main clone, and each new worktree branches from
it. So `riff workers start`, and riff before it clears the context of
a worker, fast-forward the main clone to `origin` first. You do not
pull by hand.

```mermaid
flowchart TD
    A[riff workers start / the clear of a worker] --> B{main clone on main, no local changes?}
    B -- yes --> C[git fetch --prune, git merge --ff-only]
    C --> D["the main clone moved 2 commits forward to origin/main."]
    B -- no --> E["the main clone stays as it is: WHY"]
    E --> F[the clear of a worker tells the lead]
```

When the main clone is on another branch, has local changes, or has
commits that `origin` does not have, riff changes nothing and says
why. The clear of a worker tells the lead. With no `origin`, riff says
nothing. A worker in a dir that is not the top of a git worktree moves
no clone: riff never acts on a repository above that dir.

Each worker also fetches before it makes a worktree, pushes its work
as WIP before each long run (see
[A branch with WIP commits](#a-branch-with-wip-commits)), rebases on a
fresh `origin/main` before each verify request, and after the merge
removes its worktree and its branch and prunes. The
board of the lead lists each worktree and each branch that no live
session owns.

#### Let riff fast-forward the main clone

When riff says that the main clone stays as it is, look at it. Put the
main clone back on the default branch with no local changes, then
start the workers again:

```sh
git -C ~/src/riff status
git -C ~/src/riff switch main
riff workers start 1
```

### How a worker ends

Each worker pane runs `claude` through `riff workers run`. The wrapper
waits for `claude`, and never starts it again. The rollout starts a
new worker for the free work. A loop of deaths stops it (see
[A loop of deaths](#a-loop-of-deaths)).

```mermaid
flowchart TD
    W[a worker] --> Q{what happens?}
    Q -- "claude exits on its own, for example a crash" --> C["the wrapper posts a note to the lead:<br/>pane, session ID, exit code"]
    C --> R
    Q -- "no claim and no free item" --> I["it waits idle:<br/>riff shows idle,<br/>its watch runs"]
    I -- "a request of the lead" --> N[it claims the item]
    I -- "idle too long, and another idle worker on its host" --> X["the server stops it:<br/>the pane closes, the lead gets a note"]
    Q -- "riff workers stop" --> S[the pane closes, no message]
    Q -- "it sent a verify request" --> K["it releases the item:<br/>riff clears its context"]
    Q -- "the pane dies: a memory kill, a crash, a closed pane" --> D["riff ends its session:<br/>its claims are free at once,<br/>the lead gets a note"]
    D --> R[riff starts a new worker for the free item]
```

When `claude` exits on its own, your lead gets a note. It does not
wake the lead:

```text
worker stopped: pane %5, session 6072f384-d57d-463c-a837-6df28bc9bc8a, exit code 1.
The wrapper does not start it again: the rollout starts a new worker for the free work.
```

When a kill ends `claude`, for example when the workers took too much
memory, the message names the signal and says where the work is:

```text
worker stopped: pane %5, session 6072f384-d57d-463c-a837-6df28bc9bc8a, signal 9.
A kill ended it, for example when the workers took too much memory.
Its work that is not committed is in its worktree: the next worker of its item goes on from there.
The wrapper does not start it again: the rollout starts a new worker for the free work.
```

A worker never ends itself. The server stops idle workers (see
[The server stops idle workers](#the-server-stops-idle-workers)). The
lead or you end the other workers with `riff workers stop` (see
[Stop the workers](#stop-the-workers)).

### A worker that dies

A worker can die at each moment: the system kills it for memory, it
crashes, or its pane closes. Then no process of the worker is left to
tell the server. You and your lead do nothing: the work goes on.

`riff workers host` looks at the worker panes of its machine each 5
seconds. On the machine of your lead, the lead session does the same.
Each looks after the workers of its own repository only. It acts at
the second look after the end of a pane, so 5 to 10 seconds after it.

```mermaid
sequenceDiagram
    participant T as worker pane
    participant H as riff workers host, or the lead session
    participant S as riff-server
    participant L as lead
    participant N as new worker
    Note over T: the pane dies
    H->>H: the pane is gone
    H->>S: end the session of the worker
    Note over S: its claims are free at once
    H->>S: a note to the lead
    S-->>L: at its next read
    H->>N: riff starts a worker for the free item
    N->>S: claim the item
    N->>N: goes on from the pushed branch
```

The note does not wake your lead. It names the pane, the session, the
item, and the cause when riff finds it. riff finds a kill by
`systemd-oomd` in the journal:

```text
worker stopped: pane %5, session 6072f384-d57d-463c-a837-6df28bc9bc8a, on pangolin.
The pane ended with no end call, so riff ended the session. It held issue-12: free now.
Cause: systemd-oomd killed the pane: memory pressure for /user.slice/user-1000.slice/user@1000.service being 66.21% > 50.00% for > 20s with reclaim activity.
```

To see why the system killed a pane, read the journal:

```sh
journalctl -u systemd-oomd --since "-10min"
```

On a machine with no `riff workers host` and no lead session in tmux,
nobody looks at the panes. There the server frees the claims 5 minutes
after the last sign of life of the worker, or your lead frees a
claim. See
[Free the claim of another session](#free-the-claim-of-another-session).

### A loop of deaths

A fault of a machine can kill each new worker too, for example too
little memory. Then each new start costs tokens and gives nothing. So
riff counts the deaths of the workers of each machine: an exit of
`claude` with a fault, and a pane that ends with no end call. When
more than 3 workers of a machine died in the last hour, riff starts no
worker there. Your lead gets one message, and it wakes:

```text
workers: 4 workers died in the last hour on pangolin. A loop of deaths is a fault: riff starts no worker on pangolin until 3 or fewer died in the last hour. Tell your user. On pangolin, look at the panes, and at the memory kills with journalctl -u systemd-oomd --since -1h.
```

Find the cause on that machine. See what killed the workers:

```sh
journalctl -u systemd-oomd --since -1h
```

`riff workers` shows the machine with the line `Starts no worker: 4
workers died in the last hour.` riff starts workers there again by
itself when the old deaths are more than one hour old.

### A worker with no work waits idle

A worker with no claim, and no free item or verify request, waits. It
keeps its watch running, and ends its turn. An idle session costs
nothing. riff shows it as idle, with the time since its last release.
`riff workers` shows it:

```sh
riff workers
```

```text
thelio  limit 3  runs 1  cpu 32x5883MHz (now 4100MHz), mem 124GB, 100GB available, load 2.10  score 62.8
PANE  ID        STATE  DETAIL
%3    2a880834  idle   ready for work for 2m
```

A request of your lead wakes it, and it claims the item (see
[riff starts workers by itself](#riff-starts-workers-by-itself)).
A worker that finished an
item gets a fresh context first (see
[A worker goes to its next item](#a-worker-goes-to-its-next-item)). It
waits idle only when its start routine then finds no work. A worker
never waits for a verify: its work on an item ends at the verify
request.

### The server stops idle workers

Your lead starts a worker when it has work for it. So idle workers do
not pile up, the server looks at the workers each 5 seconds. An idle
worker is a worker with no claim that makes no call. On each host, the
server keeps the idle worker with the shortest idle time. It stops
each other worker that is idle for 60 seconds. It never stops a lead,
a session that is not a worker, or a worker with a claim. It also
does not stop a worker that has an unread request of your lead: a
direct message from the lead that starts with `request:`.

```mermaid
sequenceDiagram
    participant S as riff-server
    participant M as riff mcp of the worker
    participant W as riff workers run
    participant L as lead
    S->>S: each 5 s: find the idle workers past the limit
    S->>L: note: the server stops the idle worker
    M->>S: keep-alive, each 10 s
    S-->>M: stop
    M->>W: stop
    W-->>W: claude ends, the pane closes
    M->>S: end: the session leaves riff who
    opt the worker still runs 60 s after the ask
        S->>L: note: the worker still runs, and how to stop it
    end
```

The watch of a worker also sends a keep-alive each 10 seconds, and
stops the wrapper in the same way. So a worker whose `riff mcp` ended,
for example at a self-update, also stops.

A worker that the lead wakes, or that claims work, before its next
keep-alive goes on. Your lead gets one note for each worker that the
server stops:

```text
workers: the server stops the idle worker 2a880834 on pangolin. It made no call for 75 seconds. At most 1 idle worker stays on each host.
```

A pause or a resume wakes the worker and takes the ask back. The
server then asks again, but it does not send a second note.

#### Stop a worker that the server cannot stop

When the worker still runs 60 seconds after the ask, your lead gets
one more note:

```text
workers: the idle worker 2a880834 on pangolin still runs 65 seconds after the ask to stop. Its riff mcp and its watch did not stop its riff workers run: for example, its riff mcp ended and its watch is of an older riff, or no riff workers run wraps it. Stop it on pangolin: riff workers stop 2a880834
```

Stop it on that machine with the command of the note:

```sh
riff workers stop 2a880834
```

When your lead sent a request to the worker after the ask, and the
worker did not read it, the note names each request:

```text
... Stop it on pangolin: riff workers stop 2a880834. It did not read 1 request of the lead: "request: claim issue-12". Give it to another session.
```

Stop the worker, and send the request to another worker.

#### Change the idle workers

Show the settings of the riff:

```sh
riff workers idle
```

The owner or an admin of the riff changes them. Keep at most 2 idle
workers on each host, and stop the others after 5 minutes:

```sh
riff workers idle --per-host 2 --after 300
```

With `--per-host 0`, the server stops each worker that is idle for the
idle time.
