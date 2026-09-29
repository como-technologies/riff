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
  `threads`, `join_thread`, `leave_thread`, `post`, `status`, `tell`,
  `read`, `claim`, `release`, `lead`, `pause`, `resume`, `move`,
  `leave` and `join`.
- **`riff watch`** writes one line for each message that wakes the
  session. Your agent tool reads the line and wakes the session.
- **The start hook** runs `riff hook session-start` when a session
  starts. It tells the session to run `riff watch --once` as a
  background task. See [Wake a session](#wake-a-session).
- **The end hook** runs `riff hook session-end` when a session ends.
  It tells `riff-server` that the session left. See
  [When a session ends](#when-a-session-ends).

## Connect

`riff connect claude` installs the riff plugin in Claude Code. The
plugin gives each new session the riff tools, the riff skill, a
start hook and an end hook.

```mermaid
flowchart LR
    B[riff binary] -- writes --> D["~/.local/share/riff/claude-plugin"]
    D -- "claude plugin marketplace add" --> M[marketplace riff]
    M -- "claude plugin install" --> P[plugin riff@riff]
```

Claude Code loads the plugin from that directory. After you update
`riff`, run `riff connect claude` again.

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

### Show the riffs

`riff server` shows the riff that `riff` uses, and where that choice
comes from. For that riff and for the riff of this machine, it shows
if the riff answers, its build, and your sign-in:

```sh
riff server
```

`localhost`, `127.0.0.1` and `[::1]` with the same port are one riff:
the riff of this machine. `riff server` shows it once.

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
answers: `riff and riff-server have the build 0.4.0 929605821e54
2026-09-27T22:03:01Z.` When the builds differ, the line shows both
builds and says that the versions can talk.

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

`riff watch` and `riff tail` see the new `riff` on disk, and run it.
They go on with no restart.

`riff mcp` cannot run the new `riff` by itself. At its next tool call,
it tells the session to reconnect, and stops. Claude Code does not
start it again. In the session, reconnect the riff server:

```text
/mcp
```

Pick the riff server, then **Reconnect**. `/mcp reconnect all`
reconnects each server that failed.

### When the versions do not match

Each `riff` command fails with an error like this one:

```text
riff: this riff (0.2.0 929605821e54 2026-09-27T22:03:01Z) and its riff-server (0.4.0 7213825ab1c2 2026-09-28T20:10:44Z) do not match. riff-server 0.4 talks only with riff 0.4 and 0.3. Update riff on this machine, then start your sessions again. See https://como-technologies.github.io/riff/how-it-works.html#when-the-versions-do-not-match
```

A new session gets the same error at its start. It tells you, and it
does not use the riff. `riff watch` and `riff tail` print the error
once, try again every 5 seconds, and go on when the versions can
talk.
[`riff server`](#show-the-riffs) shows both builds.

Update the older side:

- **riff** on this machine, and **riff-server** of your own riff: do
  [Update riff](start-a-riff.md#update-riff). When you joined a riff,
  see [Update riff](join-a-riff.md#update-riff) of Join a Riff.
- **The shared server:** an admin deploys a release at the end of
  each wave. A push to `main` does not deploy it. See
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
request that the release adds. List the releases, and read one:

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
- `riff watch` and `riff tail` run the new `riff` with no restart.
  `riff mcp` tells its session to reconnect with `/mcp`. A worker
  takes the new `riff` at its next item.
- The update stops no session, and a running test goes on. Your
  sign-in and the device key stay.
- Your lead gets one direct message: the host, the old release and
  the new release.
- A failed update keeps the old `riff`. The message to your lead holds
  the error. riff tries again at the next release. To try again now,
  run `riff update --tag` with the release.
- riff never installs an older release by itself.

The output of the update is in `update.log`, in the local files of
riff: `$XDG_RUNTIME_DIR/riff`, else `~/.local/state/riff`.

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

A refresh token works once. A refresh token that comes back after the
next refresh ends the sign-in: somebody copied it. A lost reply is not
a copy. When the pair of the last refresh is still unused, the reply
did not come back, for example after a 503. `riff-server` then gives a
new pair, and the sign-in stays.

```mermaid
flowchart TD
    R[refresh token R1 comes back] --> U{R1 used before?}
    U -- no --> N[new pair P2]
    U -- yes --> L{P2 of the last use unused?}
    L -- yes --> G[end P2, give a new pair: the reply was lost]
    L -- no --> E[end the sign-in]
```

### Get back into the riff

`riff watch`, `riff tail` and each tool of `riff mcp` show the error
of a lost sign-in. Find it in this table, and do the step.

| Error | What it means | Step |
|---|---|---|
| `the sign-in ended: run riff login: riff-server refused the token request: invalid_grant` | `riff-server` ended the sign-in of this machine. | Run `riff login`. |
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

## See who is in the riff

```sh
riff who
```

The first line shows the state of the riff. The second line names the
owner of the riff, or says `The riff has no owner.`. A riff with no
sign-in has no owner line. Each other line shows a session: its name,
its state, `(you)`, `owner`, `lead` and its claims, and its URI.
`owner` marks each session of the owner:

```text
The riff is running.
The owner is mike (mike@comotechnologies.io).
riff and riff-server have the build 0.1.0 ...
mike@pangolin:riff#issue-6 (a6cf)  live  (you)  owner lead issue-6  riff://mike@pangolin/...
brett@heron:riff (77e0)  idle 2m  riff://brett@heron/...
```

`live` means the session has an open watch. `idle 2m` means its last
call was 2 minutes ago. A session with a status has a second line. See
[A status](#a-status).

In a terminal, `riff who` has the colors of `riff tail`: each session
has the same color in both. `running` is green and `paused` is yellow.
A blocked status is red. The `who` tool of a session stays plain.

### List the sessions without color

Color goes only to a terminal. `--color never` turns it off also in a
terminal. `NO_COLOR=1` does the same. `--color always` keeps the color
in a pipe:

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

- `idle` does not change with a keep-alive. It is the time since the
  last call.
- A session that stops with no end, for example after `kill -9` or a
  network fault, is gone after 3 minutes. Its claims and its lead end
  together, 5 minutes after its last sign of life.
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
that work, or starts again, and says why. A compaction is not a new
start: the session keeps its claims.

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

When the earlier work is wrong or too old, the session starts again.
It deletes the pushed branch of the earlier work first, so that the
old commits do not mix with the new work. For `issue-12`:

```sh
git push origin --delete worktree-issue-12
```

The session says in its start post that it deleted the branch.

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
  riff.
- A session that left makes no call to `riff-server`. Its riff tools
  refuse, except `join`. The hooks add no riff context. The status line
  shows `(left)`.
- The leave holds over `/clear` and a resume. A new session joins as
  usual.
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
    S->>E: post verify request, wait for a pass
    S->>E: post "done issue-6", release issue-6
    S->>G: branch merged, worktree clean, issue closed?
    S->>E: move (main worktree)
    S->>G: remove .claude/worktrees/issue-6 and its branch
```

A session removes only its own worktree, and only when the work is
safe on the default branch. It uses the `ExitWorktree` tool only for a
worktree that it made in its current context. After `/clear` or
`riff workers next`, the tool says that the session is not the owner.
Then the session runs `git worktree remove` in the main worktree.

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

Only a session that holds no claim verifies. A session that waits for
its own verify keeps its claim and does not verify: a verify fills its
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
background task.

### Report a verify

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

It puts the result on pull request 40 as a comment that names its
head commit. It sets the status `riff/verify` of that commit:
`success` or `failure`, with a link to the comment. Then it posts the
result to the session that holds the issue of the `Issue:` trailer,
for example `claim=issue-12`.

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

### Read the full history

`riff read` shows only your unread messages, and not your own posts.
`--all` shows each message of your threads, your own posts too:

```sh
riff read --all
```

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
`USER@HOST`, from your sign-in. Start the chat in a terminal or a tmux
pane. Type a line and press Enter. `/quit` or Ctrl-C exits:

```sh
riff chat
```

The chat shows its history first, then each new line:

```text
2026-09-28
14:02 mike@thelio  is the release out?
14:03 brett@heron  not yet. @lead is #207 merged?
14:03 brett@heron lead  yes, #207 is merged.
```

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
Bash tool. The task does not expire like a Monitor task. It ends at
the first wake, and its end wakes the session. The session reads and
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
| A verify result | the author: `claim=ITEM` |
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

While the riff is paused, each claim fails.

### Claim an item by hand

A person can claim and release in a terminal too. `riff claim` exits
with status 1 when another session holds the item:

```sh
riff claim issue-12
riff release issue-12
```

`-t` (`--thread`) names another thread. See
[Use another thread](#use-another-thread).

## Pause the riff

A riff is paused or running. A new riff is paused, so no session takes
work before you say so. While the riff is paused:

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
    E-->>W: wake: the riff is paused
    W->>W: finish the command, WIP commit, push the branch
    W->>E: status "paused at: tests of issue-12"
    P->>E: riff resume
    E-->>W: wake: the riff is running again
    W->>W: go on from where it stopped
```

Only you, in a shell, or your lead can pause or resume the riff. A
pause and a resume wake each session. The state stays when
`riff-server` saves its state in a bucket (see [A restart](#a-restart)).

### Resume the riff

Run it in a terminal, not in an agent session:

```sh
riff resume
```

You can also ask your lead: *"Resume the riff."*

### Pause the riff now

```sh
riff pause
```

You can also ask your lead: *"Pause the riff."*

### See the state of the riff

```sh
riff whoami
```

`riff who` shows the state in its first line too.

## A status

Each session has a status: its current step, and a reason when it is
blocked. A session sets its status when it claims, when it changes
step, when it is blocked, and when it releases. `riff who` shows each
status with its age:

```text
mike@pangolin:riff#issue-6 (a6cf)  live  issue-6  riff://mike@pangolin/...
       status 4m ago: write the tests
brett@heron:riff#issue-7 (77e0)  live  issue-7  riff://brett@heron/...
       blocked 1m ago: waits for a review (step: merge)
```

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
    B->>E: read, then status "merge", blocked "waits for a review"
    P->>E: who
    E-->>P: each session with its status and age
```

### Ask each session for its status

Run this in the repository. Give the sessions one wake to answer, then
list them:

```sh
riff post --kind status --to repo=como-technologies/riff
riff who
```

### Set your status

A session sets its own status with the `status` tool. A person can
set a status from a terminal:

```sh
riff status write the tests
```

When you cannot go on, give the reason:

```sh
riff status --blocked "waits for a review" merge
```

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
- The first session of the person in the repository becomes the lead.
  The person does nothing. A later session does not become the lead.
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

The status line changes after each answer of the session. Then find
the session of `riff who` by its short ID:

```sh
riff who
```

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
    A->>L: tell lead "issue-12 waits for a verify"
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
[Save the state in a bucket](development.md#save-the-state-in-a-bucket)).

With no bucket, for example the riff of [Start a Riff](start-a-riff.md),
a restart forgets each thread, session, claim and lead. The riff is
paused again. Start your Claude Code sessions again after it, and run
`riff resume` when you want them to work.

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

With a bucket, `riff-server` saves each change to Cloud Storage
within one second, and it loads the state at start. On SIGTERM,
it saves each unsaved change, then exits. A restart loses the open
streams. `riff watch` and `riff tail` connect again. The session then
gets one wake if an addressed message is unread. Cloud Run also ends
each stream after 60 minutes. The streams then connect again in the
same way. `riff tail` does not show a message that comes while it
connects. `riff read` shows it.

After a restart with a bucket, each session counts as stopped. Its
claims stay for 5 minutes. A claim that ended before the restart stays
ended. A session that connects again in that time keeps them. The
server forgets each session that has not called for 30 days.

Tokens stay valid after a restart with a bucket. The server saves only
a hash of each token. A sign-in, a refresh or a revoke gets its reply
only after the server saved the tokens. So a restart never forgets a
token that a person already has.

During a deploy, Cloud Run starts the new instance before it stops the
old one. A lease in Cloud Storage makes sure that only one instance
serves:

```mermaid
sequenceDiagram
    participant O as old instance
    participant S as Cloud Storage
    participant N as new instance
    participant W as riff watch
    N->>S: write the lease (new ID)
    Note over N: waits 15 s
    O->>S: read the lease (every 2 s)
    S-->>O: new ID
    O-->>W: close the stream
    Note over O: replies 503, saves nothing, exits after 60 s
    N->>S: load the state
    W->>N: connect again
    N-->>W: one line, if an addressed message is unread
```

`riff` tries each call again while the server replies 503. A deploy
stops riff for less than one minute.

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
key `workers.limit`.

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
Each new worker on this machine loads these MCP servers: riff (/home/mike/.config/riff/config.toml).
```

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

#### The lead keeps a worker on each free item

Your lead checks each time a riff line wakes it, and each time it
frees an item. It counts the free items of the current wave and the
free verify requests. It gives each one to a free worker first: a
worker with no claim, which waits idle. When free work is left, and
fewer workers run than the limit, it starts the difference. It does
not wait for your word:

```sh
riff workers start 2
```

The lead starts at most one worker for each free item. It starts them
on each host with room first, and on its own machine last (see
[Offer workers from another machine](#offer-workers-from-another-machine)).
The limit still caps it: set the limit to 0 to stop new workers.

### Take over a worker

Go to the workers window, then pick a pane and type into it:

```sh
tmux select-window -t riff-workers
```

In tmux, `Ctrl-b o` goes to the next pane, and `Ctrl-b q` shows the
number of each pane.

### List the workers

Show each worker of this machine, with its pane, its session ID, its
claims and its status:

```sh
riff workers
```

```text
%3  2a880834  2a880834-3707-4672-ba4a-50438db97e1f  live  claims: issue-12
  status 1m ago: tests of issue-12
```

### Stop the workers

End each worker of this machine, or one worker by its pane:

```sh
riff workers stop
riff workers stop %3
```

It closes the pane of each worker. The session leaves `riff who`, and
its claims are free at once. Another session can take its item from
its pushed branch. At the end of a wave, the lead stops the workers
before the deploy and the update, and starts them again after them.

### Offer workers from another machine

Your lead runs on one machine. Another machine of yours can run
workers for it too. On that machine, set its limit, then start the
workers host in a tmux pane in the main clone, and leave it running:

```sh
riff workers limit 2
riff workers host
```

The host is a riff session with the status `workers host: limit 2,
no workers`. It starts and stops workers only when the lead of your
user asks, at most its own limit. It refuses each other request, and
each request that is not verified. `Ctrl-C` stops the host. Its
workers keep running.

```mermaid
sequenceDiagram
    participant L as lead on thelio
    participant S as riff-server
    participant H as riff workers host on pangolin
    L->>S: riff workers start 2 --host pangolin
    S->>H: direct message: workers start 2
    H->>H: 2 worker panes in its tmux
    H->>S: reply to the lead: panes and sessions
```

The lead starts and stops workers on that machine by its host name:

```sh
riff workers start 2 --host pangolin
riff workers stop --host pangolin
```

The reply of the host comes to the lead as a direct message.
`riff workers` in the lead lists each host after the workers of its
own machine:

```text
No worker runs on this machine.
Host pangolin: limit 2, 1 worker runs.
%3  2a880834  2a880834-3707-4672-ba4a-50438db97e1f  live  no claims
  status 1m ago: idle: waits for work
```

### A worker goes to its next item

A worker starts each item with a fresh context. It does not carry the
file reads, diffs and messages of its last item. When its item is
merged, its claim is released and its worktree is removed, the worker
runs:

```sh
riff workers next
```

Then it ends its turn. riff types `/clear` into the pane of the worker,
and then "Join the riff.". The worker keeps its riff session ID and its
watch, and claims its next item. You do nothing.

```mermaid
sequenceDiagram
    participant W as worker
    participant R as riff
    participant T as tmux pane
    W->>R: riff workers next
    R-->>W: end your turn now
    W->>R: the turn ends (Stop hook)
    R->>T: /clear
    R->>T: Join the riff.
    T->>W: start routine, next claim
```

`riff workers next` works only in a worker that holds no claims. riff
never clears the lead: you work in it. To see the context of a worker,
type `/context` in its pane.

### Workers keep good git hygiene

Workers start in the main clone, and each new worktree branches from
it. So `riff workers start`, and `riff workers next` before the fresh
context, fast-forward the main clone to `origin` first. You do not
pull by hand.

```mermaid
flowchart TD
    A[riff workers start / next] --> B{main clone on main, no local changes?}
    B -- yes --> C[git fetch --prune, git merge --ff-only]
    C --> D["the main clone moved 2 commits forward to origin/main."]
    B -- no --> E["the main clone stays as it is: WHY"]
    E --> F[riff workers next tells the lead]
```

When the main clone is on another branch, has local changes, or has
commits that `origin` does not have, riff changes nothing and says
why. A worker also tells the lead. With no `origin`, riff says
nothing.

Each worker also fetches before it makes a worktree, rebases on a
fresh `origin/main` before each push and each verify request, and
after the merge removes its worktree and its branch and prunes. The
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
waits for `claude`, and never starts it again: a crash loop costs
tokens. Your lead decides.

```mermaid
flowchart TD
    W[a worker] --> Q{what happens?}
    Q -- "claude exits on its own, for example a crash" --> C["the wrapper tells the lead:<br/>pane, session ID, exit code"]
    Q -- "no claim and no free item" --> I["it waits idle:<br/>status idle: waits for work,<br/>its watch runs"]
    I -- "a request of the lead" --> N[it claims the item]
    Q -- "riff workers stop" --> S[the pane closes, no message]
    Q -- "it waits for a verify" --> K[it keeps its claim and waits]
```

When `claude` exits on its own, your lead gets a direct message:

```text
worker stopped: pane %5, session 6072f384-d57d-463c-a837-6df28bc9bc8a, exit code 1.
riff does not start it again. Look at the pane, then start a worker again with riff workers start 1.
```

A worker never ends itself. The lead or you end workers with
`riff workers stop` (see [Stop the workers](#stop-the-workers)).

### A worker with no work waits idle

A worker with no claim, and no free item or verify request, waits. It
sets its status, keeps its watch running, and ends its turn. An idle
session costs nothing. `riff workers` shows it:

```sh
riff workers
```

```text
%3  2a880834  2a880834-3707-4672-ba4a-50438db97e1f  live  no claims
  status 2m ago: idle: waits for work
```

A request of your lead wakes it, and it claims the item (see
[The lead keeps a worker on each free item](#the-lead-keeps-a-worker-on-each-free-item)).
A worker that finished an
item runs `riff workers next` first (see
[A worker goes to its next item](#a-worker-goes-to-its-next-item)). It
waits idle only when its start routine then finds no work. A worker
that waits for a verify keeps its claim, and waits.
