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
ci: fmt-check lint test book

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

# Clean build artifacts
clean:
    cargo clean
