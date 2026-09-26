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
- Update the book in the same commit as the change.
- A new decision is a new requirement statement. Do not write the
  reasoning.
- No users yet. Change any interface freely. No compatibility shims.
- Work on `main`, or fast-forward a local branch into it.

## Writing

- Use ASD-STE100: short sentences, active voice, plain words.
- Less is more. Say each thing in one place only.
- Explain how things work with mermaid diagrams.
- Keep the quick start ("Join In") short and current.
