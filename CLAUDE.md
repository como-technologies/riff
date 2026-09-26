# Sensomatic — working notes for agents

Read `docs/src/` first. The book is the source of truth for the design
and for the settled and open decisions.

## What it is

Sensomatic lets AI agent sessions that belong to different people work
together. `subetha` is the central service. `sensomatic` is the local
client. It is an experiment, and it is not part of a larger suite.

## Rules

- **All code is Rust.** Scripts are shell or `just` recipes.
- **Stay vendor-neutral.** The core uses MCP and a plain command-line
  client. A feature that only one agent vendor has can be an optional
  adapter, never the core.
- **Zero warnings.** `just ci` must pass before you push. It runs fmt,
  clippy with `-D warnings`, the tests and the book build.
- **Docs change with the code.** Update the book in the same commit as
  the change. Record each settled decision on the Decisions page.
- **Pre-GA.** No users yet, so change any schema or interface freely. Do
  not add compatibility shims.
- **Trunk-based.** Work on `main`, or on a local branch that you
  fast-forward into `main`.

## Writing

Prose follows ASD-STE100 (Simplified Technical English): short
sentences, active voice, one idea per sentence, plain words. This applies
to the book, code comments, CLI output and commit messages. Do not coin
jargon. If a term is not defined where the reader stands, do not use it.
