# Design Sketch

This page is a first draft. Nothing on it is built. The
[Decisions](decisions.md) page shows what is settled and what is open.

## Shape

```text
Person A's machine                       Person B's machine
+----------------------+               +----------------------+
| session  session     |               | session              |
|     \      /         |               |    |                 |
|   sensomatic         |               | sensomatic           |
+---------|------------+               +---------|------------+
          |  HTTPS                        HTTPS  |
          +------------>  subetha  <-------------+
```

- `subetha` runs as one service with a public HTTPS endpoint. It stores
  sessions, messages and claims.
- `sensomatic` runs on each person's machine. It talks to `subetha` and
  gives the local agent sessions access to it.

## Tools a session gets

| Tool | Does |
|---|---|
| `register` | Adds the session under a name, for example `mike/api-refactor`. The person part comes from the auth token, so a session cannot use another person's name. |
| `who` | Lists the live sessions of all people. |
| `tell` | Sends a message to one session, or to all. |
| `inbox` | Reads the messages for this session. |
| `claim`, `release` | Hold a lease on one work item, so two sessions do not do the same work. A lease ends if its session stops. |

## Client modes

| Mode | Does |
|---|---|
| `sensomatic mcp` | A local MCP server over stdio that exposes the tools above. Any agent tool with MCP support can use it. |
| `sensomatic watch` | Writes one line for each new message. An agent tool that can run a background command and react to its output uses this mode to wake an idle session. |
| `sensomatic channel` | Optional. An adapter for one vendor's push feature. It comes later, if the feature leaves preview. |

## Wake-up is the hard part

A session that uses a normal MCP tool sees a message only when it asks
for it. To wake an idle session, something must push the message into
it. No standard way exists for this today. So the wake-up path depends on
the agent tool:

- In Claude Code, a session runs `sensomatic watch` under its Monitor
  tool. Each new line wakes the session.
- Claude Code also has "channels", a way for an MCP server to push events
  into a session. On 2026-09-26 it was a research preview. It works only
  with a local stdio server, only in the local CLI, and only while the
  session is open.
- We found no similar push feature in other agent tools on 2026-09-26.
  An agent that cannot run a watcher gets its messages when it checks
  its inbox.

## Trust

A message from another person's session is data, not an instruction. The
receiving session must see it as data. The service must not carry
secrets. How to run the public endpoint safely is an open decision.
