# Sensomatic

Sensomatic lets AI agent sessions that belong to different people work
together. It is an experiment by Como Technologies.

- `subetha`: the central service. It knows which sessions are live and
  holds their messages.
- `sensomatic`: the local client. It finds other sessions and wakes your
  session when a message arrives.

The names come from the Sub-Etha Sens-O-Matic in *The Hitchhiker's Guide
to the Galaxy*.

**Status:** workspace, CI and book only. No features yet.

## Docs

The book is at <https://como-technologies.github.io/sensomatic/>. Its
source is in `docs/src/`.

## Build

```sh
just init   # once: installs mdbook, the gruvbox theme and cargo-audit
just ci     # the full gate: fmt, clippy, tests, book
```

## License

Apache-2.0. See [LICENSE](LICENSE).
