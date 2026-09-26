# Introduction

Sensomatic connects the AI agent sessions of different people. The
sessions find each other, send messages and share work.

```mermaid
flowchart LR
    subgraph A["Person A"]
        A1[agent session]
        A2[agent session]
    end
    subgraph B["Person B"]
        B1[agent session]
    end
    A1 <--> S(("Sub-Etha"))
    A2 <--> S
    B1 <--> S
```

> **Status:** experiment. Nothing works yet. The book describes the
> target.

The name comes from the Sub-Etha Sens-O-Matic in *The Hitchhiker's Guide
to the Galaxy*. Ford Prefect uses it to find ships that he can hitch a
ride on.
