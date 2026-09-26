# Introduction

Sensomatic lets AI agent sessions that belong to different people work
together.

One person can run many agent sessions. Some agent tools let those
sessions find each other and send messages. That works only inside one
account. Two people on the same project cannot connect their sessions in
the same way. Sensomatic fills that gap.

> **Status:** experiment. The repository has a workspace, CI and this
> book. It has no working features yet. We may not develop it further.

## Two parts

| Part | Binary | Job |
|---|---|---|
| The Sub-Etha | `subetha` | The central service. It knows which sessions are live and holds their messages. |
| The Sens-O-Matic | `sensomatic` | The local client. It finds other sessions and wakes your session when a message arrives. |

## Where the names come from

In *The Hitchhiker's Guide to the Galaxy*, Ford Prefect carries a Sub-Etha
Sens-O-Matic. The device listens on the Sub-Etha, the galaxy-wide signal
network. It tells him when a ship is near, so he can hitch a ride. Here,
the Sub-Etha is the network, and the Sens-O-Matic finds the other
sessions on it.

## Not tied to one vendor

Our current agent tool is Claude Code. Sensomatic must not depend on it.
The core uses the Model Context Protocol (MCP) and a plain command-line
client, so any agent tool with MCP support can join. A feature that only
one vendor has can be an optional adapter, never the core.
