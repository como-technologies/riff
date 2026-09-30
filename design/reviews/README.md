# Reviews of the design of the store and the wire

Each file here is the write-up of one review of [the design](../../docs/src/design-storage.md), from one point of view. A review is a work item of Wave 14. Another session verifies it, as each item of a wave.

The files are not in the book. The book shows the design, not its review.

## The form of a write-up

File: `design/reviews/NN-point-of-view.md`, for example `02-operator.md`.

```markdown
# Review NN: <point of view>

Issue: #N. Design at commit <sha>.

## Summary

Three to five sentences: the view of this reviewer on the design.

## Findings

| ID | Finding | Level | Section of the design | Proposal |
|---|---|---|---|---|
| NN-1 | ... | must-fix | The log / Chunks | ... |

Levels: must-fix (the design fails its goals without it), should-fix (a real gain), note (for the lead to know).

## The questions of this point of view

Each question of the issue, with its answer.

## Questions for the lead

Things that the reviewer cannot decide.
```

## Rules

- Write in ASD-STE100.
- Each finding is concrete: it names a section of the design, a failure or a cost, and a proposal.
- Do not edit the design page. The lead decides what goes into the design.
- A finding that needs a test or a measure says what to measure. A spike is not part of a review.
