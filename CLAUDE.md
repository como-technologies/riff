# Riff — notes for agents

The book in `docs/src/` holds the requirements and the big picture.
`docs/src/requirements.md` is binding. The code holds the design.

## Work

- Mantra: **GEN;SET**, Good Enough for Now; Safe Enough to Try. Decide,
  record the requirement, move on.
- `just ci` passes before each push. Zero warnings.
- Design docs live in the code as rustdoc, with doc tests. The book
  stays for people: what riff is, how to join, the big picture.
- Each change has unit tests, integration tests and doc tests.
- Update the book in the same commit as the change. See "User docs".
- A new decision is a new requirement statement. Do not write the
  reasoning.
- No users yet. Change any interface freely. No compatibility shims.
- Work on `main`, or fast-forward a local branch into it.

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

- Use ASD-STE100: short sentences, active voice, plain words.
- Less is more. Say each thing in one place only.
- Explain how things work with mermaid diagrams.
- Keep "Start a Riff" short and current. It is for a person who knows
  nothing about riff.
