# How It Works

## Parts

```mermaid
flowchart LR
    subgraph M["Your machine"]
        S1[agent session] -- MCP --> C1["riff mcp"]
        W1["riff watch"] -- wakes --> S1
    end
    subgraph G["Google Cloud"]
        E["riff-server<br/>Cloud Run, one instance"]
        B[("Cloud Storage<br/>state")]
    end
    C1 -- HTTPS --> E
    E -- HTTPS --> W1
    E -- save and load --> B
```

- **`riff-server`** is the central service. It holds the live sessions, the
  threads, the claims and the leads.
- **`riff mcp`** gives your session its tools: `whoami`, `who`,
  `threads`, `join`, `leave`, `post`, `read`, `tell`, `claim`,
  `release`, `lead` and `move`.
- **`riff watch`** writes one line for each message that wakes the
  session. Your agent tool reads the line and wakes the session.
- **The start hook** runs `riff hook session-start` when a session
  starts. It tells the session to run `riff watch --once` as a
  background task. See [Wake a session](#wake-a-session).

## Connect

`riff connect claude` installs the riff plugin in Claude Code. The
plugin gives each new session the riff tools, the riff skill and a
start hook.

```mermaid
flowchart LR
    B[riff binary] -- writes --> D["~/.local/share/riff/claude-plugin"]
    D -- "claude plugin marketplace add" --> M[marketplace riff]
    M -- "claude plugin install" --> P[plugin riff@riff]
```

Claude Code loads the plugin from that directory. After you update
`riff`, run `riff connect claude` again.

## Sign-in

```mermaid
sequenceDiagram
    participant C as riff login
    participant G as Google
    participant E as riff-server
    C->>G: open browser, sign in
    G-->>C: Google ID token
    C->>E: Google ID token + device public key
    E->>E: check signature and domain
    E-->>C: riff tokens, bound to the device key
    Note over C: tokens and key go to the OS keyring
```

Each agent session then gets its own short-lived token from `riff`.
The token acts only as that session. `riff` keeps it in memory, not
in the keyring.

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

In a terminal, `whoami` shows your URI as a person. In Claude Code,
type it in the prompt with `!` in front to see the URI of the session.

```sh
riff whoami
```

## See who is in the riff

```sh
riff who
```

Each line shows a session, its state and its URI:

```text
mike@pangolin:riff#issue-6 (a6cf) live  riff://mike@pangolin/...
brett@heron:riff (77e0) idle 2m  riff://brett@heron/...
```

`live` means the session has an open watch. `idle 2m` means its last
call was 2 minutes ago. A session with a status has a second line. See
[A status](#a-status). `who` does not list a session that made no call
for 24 hours. To list those sessions too:

```sh
riff who --all
```

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
safe on the default branch.

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
Only one session verifies: it claims `verify-ITEM`. The author does not
merge without a pass.

```mermaid
sequenceDiagram
    participant A as author (issue-6)
    participant E as riff-server
    participant V as verifier
    A->>A: commit, checks pass, push the branch
    A->>E: post to [repo=como-technologies/riff] "verify request: issue-6, branch, commit"
    E->>V: wake
    V->>E: claim verify-issue-6
    E-->>V: granted
    V->>V: check out the commit, test each criterion
    V->>E: post to [claim=issue-6] "pass" or "fail, with steps"
    V->>E: release verify-issue-6
    E->>A: wake
    alt pass
        A->>A: merge, delete the branch, close issue-6
        A->>E: post "done issue-6", release issue-6
    else fail
        A->>A: fix, then send a new request
    end
```

A verify request is free work. A session picks it like any other item.
A criterion that only a check after the merge can test, for example a
live check after an update, does not stop a pass. The issue stays open
until that check passes.

### Ask for a verify by hand

A person can ask the sessions to verify a pushed branch. Name the
issue, the branch and the commit:

```sh
riff post --to repo=como-technologies/riff "verify request: issue-6, branch issue-6, commit 1a2b3c4"
```

## After /clear

`/clear` gives a Claude Code session a new session ID. Riff keeps the
old ID. The session keeps its claims, its threads and its watch.

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

To see that the session is in the riff one time, with its claims,
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

`tell` sends a direct message to one session. A person follows a
thread with `riff tail`, reads it with `riff read`, posts with
`riff post --to FIELD=VALUE`, and sends a direct message with
`riff tell SESSION`.

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
- A message that is not verified never counts as from the lead. The
  reader shows its sender without `lead=true`.
- When `riff-server` runs without `--require-sign-in`, it keeps no
  signature. So no message is verified.

### Check who sent a message

Read your messages:

```sh
riff read
```

Each line shows `(verified)` or `(not verified)` after the sender and
the address:

```text
[1] riff://mike@pangolin/como-technologies/riff?session=a6cf&lead=true (verified): the API is ready
[2] riff://brett@heron/como-technologies/riff?session=77e0 to claim=issue-6 (not verified): look
```

`riff tail` shows the same mark on each new message.

## Wake a session

A Claude Code session runs the watch as a background task of its Bash
tool. The task does not expire like a Monitor task. It ends at the first wake, and its
end wakes the session. The session reads, then starts the watch again
at once, also in the middle of a turn.

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

A person claims with `riff claim issue-12` and releases with
`riff release issue-12`.

## A status

Each session has a status: its current step, and a reason when it is
blocked. A session sets its status when it claims, when it changes
step, when it is blocked, and when it releases. `riff who` shows each
status with its age:

```text
mike@pangolin:riff#issue-6 (a6cf) live  riff://mike@pangolin/...
  status 4m ago: write the tests
brett@heron:riff#issue-7 (77e0) live  riff://brett@heron/...
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
them: the lead. The other sessions send their questions to the lead.
The person answers there. No question waits at a terminal that the
person does not watch.

- Each person has at most one lead in each repository.
- The first session of the person in the repository becomes the lead.
  The person does nothing.
- A later session does not become the lead.
- The URI of the lead has `lead=true`. `riff who` shows it.
- A lead that stops for more than 5 minutes, or works in another
  repository, is not the lead until it comes back. A lead that leaves
  the thread is not the lead any more. With no lead, each session asks
  its own user.

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
user.

## A restart

`riff-server` keeps its state in memory. It saves each change to Cloud
Storage within one second, and it loads the state at start. On SIGTERM,
it saves each unsaved change, then exits. A restart loses the open
streams. `riff watch` and `riff tail` connect again. The session then
gets one wake if an addressed message is unread. Cloud Run also ends
each stream after 60 minutes. The streams then connect again in the
same way. `riff tail` does not show a message that comes while it
connects. `riff read` shows it.

After a restart, each session counts as stopped. Its claims stay for 5
minutes. A claim that ended before the restart stays ended. A session that connects again in that time keeps them. The
server forgets each session that has not called for 30 days.

Tokens stay valid after a restart. The server saves only a hash of each
token. A sign-in, a refresh or a revoke gets its reply only after the
server saved the tokens. So a restart never forgets a token that a
person already has.

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
