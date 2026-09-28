# Waves

Sessions take their work in waves. A wave is a numbered group of work
items: Wave 1, Wave 2, and so on. The waves run in number order. The
items of one wave run at the same time, each in its own session.

- The current wave is the open wave with the lowest number. The next
  wave is the open wave after it.
- When a repository has no waves, each open item is in the current
  wave.
- The lead plans the waves. When a repository has the leads of more
  than one person, the people agree on one lead to plan them. See
  [The lead](how-it-works.md#the-lead).

## Needs

An item can need other items. Its `Needs:` line names them:

```text
Needs: #51, #58
```

An item is merged when it is closed, or when it has the comment
`Merged in #PR (COMMIT)`. The author adds this comment when a check
after the merge is left (see
[Verify finished work](how-it-works.md#verify-finished-work)).

## Pick an item

A session takes a free item of the current wave, never of a later
wave. When the current wave has no free item, the session verifies
the work of others, runs its checks after the merge, or waits.

```mermaid
flowchart TD
    S[session looks for work] --> C{"free item in<br/>the current wave?"}
    C -- yes --> T[claim it]
    C -- no --> W["verify the work of others,<br/>run checks after the merge, or wait"]
```

## The life of a wave

```mermaid
flowchart LR
    P[planned] --> C[current]
    C --> M[each item merged]
    M --> S[the lead stops the workers]
    S --> P2[the lead deploys the shared server]
    P2 --> U[each machine gets the merged code]
    U --> R[the sessions start again]
    R --> K[checks after the merge]
    K --> X[each item closed]
    X --> D[the wave is done]
    D --> E[the lead ends the wave]
    E --> N[the next wave is current]
```

A wave is done when each of its items is closed. An item is closed
when it is merged and each check after the merge passed. Some items
have a check that needs the merged code on each machine. The order in
a wave: merge each item, stop the workers, deploy the shared server,
update each machine, start the sessions again, run the checks after
the merge, close each item. A riff with no shared server of its own
code skips the deploy. In the riff repository, the update is
[Update riff](start-a-riff.md#update-riff).

A push to `main` does not deploy the shared server. So a merge in the
middle of a wave does not stop the sessions with a build mismatch.
The lead deploys once, at the end of the wave. See
[Deploy the shared server at the end of a wave](development.md#deploy-the-shared-server-at-the-end-of-a-wave).

No session starts an item of the next wave before the current wave is
done.

## A new item

A person or a session can add a work item at any time, with no
wave. The lead puts it in a wave:

- Each item is in a later wave than each of its needs.
- No item blocks or breaks the other work of its wave.
- When the item fits in no open wave, the lead makes a new wave. Its
  number is the last number plus one.

Then the lead posts to the thread: the item, its wave, what it needs,
and what needs it.

```mermaid
sequenceDiagram
    participant P as person
    participant L as lead
    participant E as riff-server
    participant S as sessions
    P->>P: add issue-70, with no wave
    P->>E: tell lead "new item: issue-70"
    E->>L: wake
    L->>L: write the Needs line, put issue-70 in Wave 3
    L->>E: post to [repo] "issue-70 is in Wave 3. It needs issue-55. issue-67 needs it."
    E->>S: wake
```

### Tell the lead about a new item

Add the item with no wave. On GitHub, see
[Add an item with no wave](#add-an-item-with-no-wave). The lead looks
for items with no wave each time it wakes. To wake it now, tell it:

```sh
riff tell lead "New item: issue-70"
```

## Waves on GitHub

This is the only part of the book that is special to one forge.

- A wave is a milestone named `Wave N`. A name can follow, for example
  `Wave 5: Cloud`. A work item is an issue in the milestone.
- A milestone with another name is out of the waves.
- The backlog is the milestone `Backlog`. It holds the items that we
  track but do not start. An item in the backlog is not free work: no
  session starts it. Only the lead moves an item from the backlog into
  a wave, when you schedule the item.
- An open wave is an open milestone. The lead ends a wave: it closes
  the milestone. It closes a milestone only when the milestone has no
  open issue.

The waves of riff are its
[milestones](https://github.com/como-technologies/riff/milestones).

### See the waves

```sh
gh api repos/como-technologies/riff/milestones --jq 'map("\(.number) \(.title)")[]'
```

### See the open items of a wave

```sh
gh issue list --milestone "Wave 2"
```

### Move an item to the backlog

```sh
gh issue edit 71 --milestone Backlog
```

### See the items in the backlog

```sh
gh issue list --milestone Backlog
```

To schedule an item, tell your lead: *"Move issue 71 into Wave 4."*

### Add an item with no wave

```sh
gh issue create --title "Show the wave in riff who" --body "Done when: riff who shows the wave of each claim."
```
