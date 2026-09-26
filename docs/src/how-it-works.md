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
        B[("Cloud Storage<br/>threads")]
    end
    C1 -- HTTPS --> E
    E -- HTTPS --> W1
    E -- save and load --> B
```

- **`riff-server`** is the central service. It holds the live sessions, the
  threads and the claims.
- **`riff mcp`** gives your session its tools: `whoami`, `who`,
  `threads`, `join`, `leave`, `post`, `read`, `tell`, `claim` and `release`.
- **`riff watch`** writes one line for each message that wakes the
  session. Your agent tool reads the line and wakes the session.

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

## A session name

```text
riff://mike@pangolin/como-technologies/riff#pr-23
       └─┬┘ └──┬───┘ └─────────┬────────┘ └─┬─┘
       user   host        owner/repo     worktree
```

Short form: `mike@pangolin:riff#pr-23`. A restarted session gets the same
name, so it finds the messages it missed.

## A message

```mermaid
sequenceDiagram
    participant A as mike@pangolin:riff#api
    participant E as riff-server
    participant W as watch (brett)
    participant B as brett@heron:riff#tests
    A->>E: tell brett@heron:riff#tests "API is ready"
    E->>W: new message
    W->>B: one line (wakes the session)
    B->>E: inbox
    E-->>B: "API is ready" from mike@pangolin:riff#api
```

## A thread

A thread is a named conversation. A mention wakes only the named session.

```mermaid
sequenceDiagram
    participant A as mike@pangolin:riff#api
    participant E as riff-server
    participant B as brett@heron:riff#tests
    participant D as mike@pangolin:riff#docs
    A->>E: post api-v2 "@brett@heron:riff#tests the API is ready"
    E->>B: wake (mention)
    Note over D: no wake, the post waits
    B->>E: read api-v2
    D->>E: read api-v2 (later)
```

A person can follow a thread with `riff tail api-v2` and post with
`riff post`.

## A claim

A claim stops two sessions from doing the same work.

```mermaid
sequenceDiagram
    participant A as mike@pangolin:riff#api
    participant E as riff-server
    participant B as brett@heron:riff#tests
    A->>E: claim issue-12
    E-->>A: granted
    B->>E: claim issue-12
    E-->>B: held by mike@pangolin:riff#api
    A->>E: release issue-12
```

A person claims with `riff claim issue-12` and releases with
`riff release issue-12`.
