# Development

## Tools

Install [Rust](https://rustup.rs) and [just](https://just.systems). Then
run this once:

```sh
just init
```

The command installs `mdbook`, the `mdbook-gruvbox` theme and
`cargo-audit`. `rust-toolchain.toml` sets the Rust version.

## Workspace

| Crate | Binary | Job |
|---|---|---|
| `crates/subetha` | `subetha` | The central service |
| `crates/sensomatic` | `sensomatic` | The local client |

## Gate

Run the full gate before you push:

```sh
just ci
```

The gate checks formatting, runs clippy with warnings as errors, runs
all tests and builds this book. CI runs the same gate on each push to
`main`. A separate CI job runs `cargo audit` on each push and each week.

## Book

`just book-serve` builds this book and opens it with live reload. CI
publishes the book and the API docs to GitHub Pages on each push to
`main`.
