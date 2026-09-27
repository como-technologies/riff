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
ci: fmt-check lint test doc book

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

# Install riff-server as a user service, with the OAuth client of the cloud project
service *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    . deploy/cloud.env
    if [ -z "$RIFF_OIDC_CLIENT_ID" ]; then
        echo "deploy/cloud.env has no client ID. Run: just oauth-client" >&2
        exit 1
    fi
    secret=$(gcloud secrets versions access latest --secret "$CLOUD_SECRET" --project "$CLOUD_PROJECT")
    RIFF_OIDC_CLIENT_ID=$RIFF_OIDC_CLIENT_ID RIFF_OIDC_CLIENT_SECRET=$secret riff-server install {{ARGS}}

# Make the cloud resources of riff; it checks each one first (R136)
cloud-setup:
    deploy/cloud-setup.sh

# Build the image with Cloud Build and deploy it to Cloud Run (R136)
deploy:
    deploy/deploy.sh

# Store the OAuth client: the secret in Secret Manager, the ID in deploy/cloud.env
oauth-client:
    deploy/oauth-client.sh

# Clean build artifacts
clean:
    cargo clean
