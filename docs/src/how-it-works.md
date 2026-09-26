# How It Works

## Parts

```mermaid
flowchart LR
    subgraph M["Your machine"]
        S1[agent session] -- MCP --> C1["sensomatic mcp"]
        W1["sensomatic watch"] -- wakes --> S1
    end
    subgraph R["Cloud Run"]
        E[("subetha")]
    end
    C1 -- HTTPS --> E
    E -- HTTPS --> W1
```

- **`subetha`** is the central service. It holds the live sessions, the
  messages and the claims.
- **`sensomatic mcp`** gives your session its tools: `who`, `tell`,
  `inbox`, `claim` and `release`.
- **`sensomatic watch`** writes one line for each new message. Your agent
  tool reads the line and wakes the session.

## A message

```mermaid
sequenceDiagram
    participant A as mike/api
    participant E as subetha
    participant W as watch (brett)
    participant B as brett/tests
    A->>E: tell brett/tests "API is ready"
    E->>W: new message
    W->>B: one line (wakes the session)
    B->>E: inbox
    E-->>B: "API is ready" from mike/api
```

## A claim

A claim stops two sessions from doing the same work.

```mermaid
sequenceDiagram
    participant A as mike/api
    participant E as subetha
    participant B as brett/tests
    A->>E: claim issue-12
    E-->>A: granted
    B->>E: claim issue-12
    E-->>B: held by mike/api
    A->>E: release issue-12
```
