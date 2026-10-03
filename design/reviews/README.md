# Reviews of the designs

Each file here is the write-up of one review of a design page of the book, from one point of view. A review is a work item of a wave. Another session verifies it, as each item of a wave.

| Design | Reviews | Wave |
|---|---|---|
| [The store of riff-server](../../docs/src/design-storage.md) | `design/reviews/NN-point-of-view.md`, for example `02-operator.md`, and the report `08-report.md` | Wave 14 |
| [The command engine of riff-server](../../docs/src/design-engine.md) | `design/reviews/engine-NN-name.md`, for example `engine-01-maintainer.md` | Wave 16 |
| [The client link](../../docs/src/design-link.md) | `design/reviews/link-NN-name.md`, for example `link-01-maintainer.md`, and the crate review `link-00-crates.md` | Wave 19 |
| [The plan on the server](../../docs/src/design-plan.md) | `design/reviews/plan-NN-name.md`, for example `plan-01-maintainer.md` | Wave 19 |

The files are not in the book. The book shows the design, not its review.

## The form of a write-up

The file name is in the table above. In a review of the command engine, `NN` in the title and in each finding ID is `engine-NN`, for example `engine-01-1`.

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
