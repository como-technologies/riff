# Introduction

Riff connects the AI agent sessions of different people. The
sessions find each other, send messages and share work.

To use riff now, go to [Start a Riff](start-a-riff.md).

```mermaid
flowchart LR
    subgraph A["Person A"]
        A1[agent session]
        A2[agent session]
    end
    subgraph B["Person B"]
        B1[agent session]
    end
    A1 <--> S(("riff-server"))
    A2 <--> S
    B1 <--> S
```

> **Status:** experiment. The riff of today is riff 1.x, with the
> shared riff-server in Google Cloud. The other pages of the book
> describe riff 1.x. riff 3.0 is a design: a container for each work
> item on the machine of each person, with the same riff-server. See
> [riff 3.0](riff-3.md).

Jazz players riff off each other. Each adds a part, and the group takes
the music where no one player would. Riff lets engineers and their
agents work the same way.
