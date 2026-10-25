# Riff — notes for agents

The book in `docs/src/` holds the requirements and the big picture.
`docs/src/requirements.md` is binding. The code holds the design.

## Work

- Mantra: **GEN;SET**, Good Enough for Now; Safe Enough to Try. Decide,
  record the requirement, move on.
- `just check` passes before each push: the fast checks and the tests
  of the crates that you touched. Zero warnings. The Gate on GitHub is
  the one full run of each commit.
- Design docs live in the code as rustdoc, with doc tests. The book
  stays for people: what riff is, how to join, the big picture.
- Each change has unit tests, integration tests and doc tests.
- Update the book in the same commit as the change. See "User docs".
- A new decision is a new requirement statement. Do not write the
  reasoning. Add it with `just req SECTION "text"`: it gets
  the ID, wraps the text and writes it after the last requirement of
  the section. Never edit `requirements.md` by hand to add one. Never
  renumber a requirement.
- `requirements.md` holds the rules of today. A requirement that a new
  one supersedes in full moves, in the same change, to
  `requirements-history.md`: same section, same ID, same text. A
  requirement that a new one supersedes in part stays, and its text
  keeps only the part that holds. `just reqs` fails on a requirement
  in `requirements.md` that says "Superseded by" or "Replaced by" with
  no "in part".
- No users yet. Change any interface freely. No compatibility shims.
- Work on a branch. Merge by pull request with auto-merge. Never push
  to `main`.

## Issues

- Each "Done when:" line of an issue names the docs: where a person
  finds the feature in the book.

## Verify

- A verifier does not run `just ci`: it reads the Gate and reviews.
- A verifier reads the book, the rustdoc and the requirements, and
  checks that the change fits them.

## User docs (STRONG REQUIREMENT)

A feature is not done until a person can find it in the book and use
it. Requirements and rustdoc do not count: they are for agents.

- Each new command, subcommand, flag or setting that a person uses
  gets a how-to in the book: `start-a-riff.md`, `development.md` or
  `how-it-works.md`.
- A how-to has its own heading and a copyable `sh` block. Do not hide
  a new step in a paragraph of an old step.
- Check the words against `--help` output. Use the real flag names.
- Before each commit, run `riff --help` and `riff-server --help`.
  Find each command in the book. Add the missing ones.

## Writing

- Use ASD-STE100: short sentences, active voice, plain words. Use
  natural tenses for things in time: "I will tell you" for a future
  action, "it failed" for a past event. Never the present tense for a
  future action.
- Less is more. Say each thing in one place only.
- Name no version of riff in a text, in rustdoc or in the book: write
  "riff runs each session in a container". `just check` fails on a
  version name. The few uses that stay are in
  `crates/hygiene/versions.allow`.
- Explain how things work with mermaid diagrams.
- Keep "Start a Riff" short and current. It is for a person who knows
  nothing about riff.
