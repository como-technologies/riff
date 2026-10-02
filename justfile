# Default: list available recipes
default:
    @just --list

# --locked builds each tool from its own lockfile, so a new MSRV in a
# tool's dependency cannot break the install on the pinned toolchain.
# One-time setup: install the tools the recipes need
init:
    rustup component add clippy rustfmt
    cargo install --locked mdbook mdbook-gruvbox mdbook-mermaid cargo-audit

# hygiene ci prints the recipe, ci-text or ci-full, and one line that says
# which set runs and why (01M3WNMKB6PAP6J0QXX4A684HH).
# Run the checks that the diff from origin/main can break
ci:
    #!/usr/bin/env bash
    set -euo pipefail
    recipe=$(cargo run -q -p hygiene -- ci)
    exec {{just_executable()}} "$recipe"

# The Gate on GitHub runs this recipe (01M3WNN7VQJKN5MJH7JN50VF4D).
# crate-audit is not part of the gate: CI runs it as a separate job (and
# weekly), so a new advisory cannot hide a code failure.
# Run all CI checks
ci-full: fmt-check lint test doc book reqs wrap

# Run only the checks for text: the book, the requirement IDs, the wrap
ci-text: book reqs wrap

# Check that each prose line of the book is at most 72 characters
wrap:
    @cargo run -q -p hygiene -- wrap docs/src

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
alias gate := ci-full

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

# 01M3MY2KWKBJCQ0BCNC6533RBW: no test reaches the shared riff, the local riff or the OS keyring.
# Run all tests, with a RIFF_SERVER where nothing listens and a D-Bus that fails each call
test *ARGS:
    env RIFF_SERVER=http://127.0.0.1:9 DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/riff-test-bus cargo test --workspace {{ARGS}}

# Build the API docs; a broken doc link fails
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace

# hygiene book installs the gitignored gruvbox theme if it is missing, and
# keeps docs/book.toml as it is (01M3W5YW0172EVF2JA8T7WW392). It fails on an
# ERROR line of mdbook, a missing include, an empty code block, or a tracked
# file that the build changed.
# Build the book and check it
book:
    cargo run -q -p hygiene -- book docs
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

# 01M3JY12HASECNN6SFQ880JT5H, 01M3K0QM89E2XM1NWSPT4KXSTC, 01M3MRDESPG8VGMQ1F6KFJXBC5.
# The installed riff and plugin stay the same. The trap stops the server also on Ctrl-C.
# RIFF_HOME keeps the settings, local files and secrets of the tree in target/dev-home
# (01M3MY2KWKBJCQ0BCNC6533RBW), never in the riff of the machine.
# RIFF_ON=1 turns riff on for the dev session: the settings of the installed plugin do not
# count for the plugin of the tree (01M3XY2SWEK0N8MC3MY4TMYTD3).
# Test this tree without the shared riff: its riff-server on a free port, Claude Code with its plugin. It loads .env
dev *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f .env ]; then set -a; . ./.env; set +a; fi
    cargo build --workspace
    tree="{{justfile_directory()}}"
    listens() { (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; }
    port=7900
    while listens "$port"; do port=$((port + 1)); done
    export RIFF_LISTEN="127.0.0.1:$port" RIFF_SERVER="http://127.0.0.1:$port"
    export RIFF_HOME="$tree/target/dev-home" RIFF_ON=1
    log="$tree/target/dev-server.log"
    "$tree/target/debug/riff-server" {{ARGS}} > "$log" 2>&1 &
    server=$!
    trap 'kill "$server" 2>/dev/null || true' EXIT
    until listens "$port"; do
        if ! kill -0 "$server" 2>/dev/null; then cat "$log"; exit 1; fi
        sleep 0.1
    done
    echo "The riff of this tree: $RIFF_SERVER. The log of its server: $log"
    PATH="$tree/target/debug:$PATH" claude --plugin-dir "$tree/crates/riff/claude-plugin/riff" \
        --settings '{"enabledPlugins":{"riff@riff":false}}'

# The shared server on Cloud Run: just cloud RECIPE
mod cloud 'deploy/cloud.just'

# Set up the GitHub repository for pull requests: auto-merge, squash only, the ruleset on main
github REPO="como-technologies/riff":
    deploy/github.sh {{REPO}}

# Clean build artifacts
clean:
    cargo clean
