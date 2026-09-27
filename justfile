# Default: list available recipes
default:
    @just --list

# --locked builds each tool from its own lockfile, so a new MSRV in a
# tool's dependency cannot break the install on the pinned toolchain.
# One-time setup: install the tools the recipes need
init:
    rustup component add clippy rustfmt
    cargo install --locked mdbook mdbook-gruvbox mdbook-mermaid cargo-audit
    mdbook-gruvbox install docs

# Run all CI checks. crate-audit is not part of the gate: CI runs it as a
# separate job (and weekly), so a new advisory cannot hide a code failure.
ci: fmt-check lint test doc book reqs

# Check the requirement IDs: no duplicate, and each cited ID exists
reqs:
    cargo run -q -p reqs -- check

# Print a new requirement ID (a ULID)
rid:
    @cargo run -q -p reqs -- rid

# Check the form of a pull request (needs gh), or of a commit: just hygiene pr 90
hygiene *ARGS:
    @cargo run -q -p hygiene -- {{ARGS}}

# House vocabulary for the full local gate
alias gate := ci

# Format code
fmt:
    cargo fmt

# Check formatting without modifying files
fmt-check:
    cargo fmt --check

# Lint the whole workspace including tests and examples
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Build in debug mode
build:
    cargo build --workspace

# Run all tests
test *ARGS:
    cargo test --workspace {{ARGS}}

# Build the API docs; a broken doc link fails
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace

# Build the book (installs the gitignored gruvbox theme if it is missing)
book:
    @if [ ! -d docs/gruvbox ]; then mdbook-gruvbox install docs; fi
    mdbook build docs
    @echo "Book built -> docs/book"

# Serve the book locally with live reload
book-serve: book
    mdbook serve docs --open

# Audit dependencies for known vulnerabilities, using .cargo/audit.toml
# (skipped if cargo-audit is not installed; CI always runs it)
crate-audit:
    @if command -v cargo-audit >/dev/null 2>&1; then cargo audit; else echo "skip: cargo-audit not installed (run 'just init')"; fi

# Update Cargo.lock to the latest compatible versions
crate-update:
    cargo update

# Install riff and riff-server into ~/.cargo/bin
install:
    cargo install --locked --path crates/riff
    cargo install --locked --path crates/riff-server

# Run riff-server on this machine (memory only, no sign-in)
serve:
    cargo run -p riff-server

# riff-server on this machine: just local RECIPE
mod local 'deploy/local.just'

# The shared server on Cloud Run: just cloud RECIPE
mod cloud 'deploy/cloud.just'

# Print the shell line that points riff at a server: local or cloud. Run: eval "$(just use cloud)"
use TARGET="":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{TARGET}}" in
        local) echo "unset RIFF_SERVER" ;;
        cloud) . deploy/cloud.env; echo "export RIFF_SERVER=$CLOUD_URL" ;;
        "") echo "# riff uses ${RIFF_SERVER:-http://127.0.0.1:7878}" ;;
        *) echo "Use: just use local, or just use cloud" >&2; exit 1 ;;
    esac

# Set up the GitHub repository for pull requests: auto-merge, squash only, the ruleset on main
github REPO="como-technologies/riff":
    deploy/github.sh {{REPO}}

# Clean build artifacts
clean:
    cargo clean
