# 01M6XWACAZH79NK727KMC3AQ0D: the git config and the gh sign-in of a session
# container with the token of an App, also on a host and on the Gate. The
# token in crates/isolated/session-gh is not real.
session_env := (
    "GIT_CONFIG_COUNT=5" +
    " GIT_CONFIG_KEY_0=commit.gpgsign GIT_CONFIG_VALUE_0=false" +
    " GIT_CONFIG_KEY_1=credential.https://github.com.helper GIT_CONFIG_VALUE_1=" +
    " GIT_CONFIG_KEY_2=credential.https://github.com.helper GIT_CONFIG_VALUE_2='!gh auth git-credential'" +
    " GIT_CONFIG_KEY_3=user.name GIT_CONFIG_VALUE_3='riff-test[bot]'" +
    " GIT_CONFIG_KEY_4=user.email GIT_CONFIG_VALUE_4='1101+riff-test[bot]@users.noreply.github.com'" +
    " GH_CONFIG_DIR='" + justfile_directory() / "crates/isolated/session-gh" + "'"
)

# 01M3MY2KWKBJCQ0BCNC6533RBW: no test reaches the shared riff, the local riff or the OS keyring.
# The environment of each test run: a RIFF_SERVER where nothing listens, a D-Bus that fails each call, and the session_env.
test_env := "RIFF_SERVER=http://127.0.0.1:9 DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/riff-test-bus " + session_env

# Default: list available recipes
default:
    @just --list

# --locked builds each tool from its own lockfile, so a new MSRV in a
# tool's dependency cannot break the install on the pinned toolchain.
# One-time setup: install the tools the recipes need
init:
    rustup component add clippy rustfmt
    rustup target add aarch64-apple-darwin
    cargo install --locked mdbook mdbook-gruvbox mdbook-mermaid cargo-audit

# hygiene ci prints the recipe, ci-text or ci-full, and one line that says
# which set runs and why (01M3WNMKB6PAP6J0QXX4A684HH). ci_lock stops a
# second run in this worktree at once (01M43DKYVAX0TJ2F5YYGYFSZ4G).
# Run the checks that the diff from origin/main can break
ci:
    #!/usr/bin/env bash
    set -euo pipefail
    . "{{justfile_directory()}}/crates/hygiene/ci-lock.sh"
    ci_lock "$PWD/target"
    recipe=$(cargo run -q -p hygiene -- ci)
    {{just_executable()}} "$recipe"

# The Gate on GitHub runs the same checks as jobs at the same time
# (01M3WNN7VQJKN5MJH7JN50VF4D, 01M513W2K3HNRWPQTQNBQPT8MZ). crate-audit is
# not part of the gate: CI runs it as a separate job (and weekly), so a new
# advisory cannot hide a code failure.
# Run all CI checks
ci-full:
    #!/usr/bin/env bash
    set -euo pipefail
    . "{{justfile_directory()}}/crates/hygiene/ci-lock.sh"
    ci_lock "$PWD/target"
    {{just_executable()}} ci-checks

# ci-full runs these checks with the lock held.
[private]
ci-checks: fmt-check lint test doc book reqs wrap versions

# The job Build of the Gate runs these checks, and the test build, in the
# session image (01M6QQ1NXY0CXMZEEEP413W4PK). Its checks and the parts of
# the tests together are the checks of ci-checks. The cheap checks come
# first, so a broken one stops the job before the long builds.
[private]
ci-build: fmt-check reqs wrap versions book lint doc test-build

# The Gate is the one full run of each commit (01M49HAZ5BZYGAR9PGC089RM3F).
# hygiene crates prints the cargo test arguments for the crates of the
# diff and their dependents, and one line that says why
# (01M49HAZA5K08XW2JQ11TG87JP).
# Before a push: the fast checks, and the tests of the crates that the diff touches
check:
    #!/usr/bin/env bash
    set -euo pipefail
    . "{{justfile_directory()}}/crates/hygiene/ci-lock.sh"
    ci_lock "$PWD/target"
    {{just_executable()}} fmt-check lint lint-macos doc book reqs wrap versions
    crates=$(cargo run -q -p hygiene -- crates)
    if [ -n "$crates" ]; then
        # shellcheck disable=SC2086 # each word is one argument of cargo test
        {{just_executable()}} cargo-test $crates
    fi

# Run only the checks for text: the book, the requirement IDs, the wrap, the version names
ci-text: book reqs wrap versions

# 01M51K1VMWTNZGD6PT1BYDGWM3: the book.
# Check the wrap of the book, and that no page has two blank lines in a row
wrap:
    @cargo run -q -p hygiene -- wrap docs/src
    @cargo run -q -p hygiene -- blank docs/src

# 01M5R2XVGMTB2HC9PRMS85ZWTY: no version name of riff in the code and the book.
# Check that the code, the tests and the book name no version of riff
versions:
    @cargo run -q -p hygiene -- versions

# Check the requirement IDs: no duplicate, and each cited ID exists
reqs:
    cargo run -q -p reqs -- check

# Print a new requirement ID (a ULID)
rid:
    @cargo run -q -p reqs -- rid

# Add a requirement to the end of a section: just req "Sessions" "The text."
req $SECTION $TEXT:
    @cargo run -q -p reqs -- req "$SECTION" "$TEXT"

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

# The crates of the job macOS of the Gate (01M6YPB5DZ9M3VEJ7V7P7EF797).
macos_crates := "-p riff-core -p riff-client -p riff"

# 01M77NFYCGDD3GKCE4RXDPKRNE: the clippy of test-macos, for a Mac, on
# each host. It needs the Rust target of a Mac: just init adds it.
# Lint riff, riff-client and riff-core for a Mac, also on Linux
lint-macos:
    deploy/lint-macos.sh {{macos_crates}} --all-targets -- -D warnings

# Build in debug mode
build:
    cargo build --workspace

# Run all tests, with a RIFF_SERVER where nothing listens and a D-Bus that fails each call
test *ARGS:
    {{just_executable()}} cargo-test --workspace {{ARGS}}

# The Gate runs the tests in parts, each part in a job of its own, in the
# session image (01M513W2NSE303E6XAM18BW7T6, 01M515Z3Y7GSFNTJGXJD727MQK).
# hygiene test-part lists each test and runs the tests whose name falls in
# part I of N. It runs as a built binary, not with cargo run, so its cargo
# test gets no variable of cargo run. The session image has its own target.
# Run part I of N of the tests, for example: just test-part 2/4
test-part PART:
    cargo build -q -p hygiene
    {{test_env}} "${CARGO_TARGET_DIR:-target}/debug/hygiene" test-part {{PART}}

# The Gate builds the test binaries one time, in the job Build, and packs
# them into target/test-archive (01M6QQ1NXY0CXMZEEEP413W4PK). The doc
# tests run here, because rustdoc compiles each of them when it runs.
# The job keeps the folder as the artifact tests-TREE
# (01M6QQ1P0GH9AGCDA5N8EMEFGF).
# Build the test binaries one time, run the doc tests, and pack the binaries into target/test-archive
test-build:
    {{test_env}} cargo run -q -p hygiene -- test-build target/test-archive

# The jobs Tests I/N of the Gate download the artifact tests-TREE into
# target/test-archive, unpack it in the checkout, and run part I of N from
# it with no build (01M6QQ1P5YCT0JX90X5FCKM4W1). hygiene is a binary of
# the archive.
# Unpack the archive of test-build, and run part I of N of the tests from it, for example: just test-archived 2/4
test-archived PART:
    tar -xf target/test-archive/tests.tar
    {{test_env}} "${CARGO_TARGET_DIR:-target}/debug/hygiene" test-part {{PART}} --archive target/test-archive

# A verifier runs a test of the commit with this (01M75NDQY6BHZBV9PF735E3FGY,
# 01M75T9RGR89MAC6S78K42QN7Q). deploy/ci-test.sh takes the artifact
# tests-TREE of the tree that the newest CI run at HEAD built, else of the
# tree of HEAD, puts a worktree of HEAD at the path of the archive,
# unpacks the archive there, and runs test-named there in the environment
# of the jobs Tests I/N. It compiles nothing. Exit 3: CI kept no test
# build of the tree that runs here.
# Run the tests whose name has NAME from the test binaries that CI built for HEAD, with no build, for example: just ci-test the_archive_runs
[positional-arguments]
ci-test NAME:
    deploy/ci-test.sh "$1"

# deploy/ci-test.sh runs this in the checkout of the archive in ARCHIVE.
[positional-arguments]
[private]
test-named NAME ARCHIVE:
    {{test_env}} "${CARGO_TARGET_DIR:-target}/debug/hygiene" test-name "$1" --archive "$2"

# The job macOS of the Gate runs this on a Mac with Apple silicon
# (01M6YPB5DZ9M3VEJ7V7P7EF797). A test that needs Linux has
# cfg(target_os = "linux") (01M6YPB5GTHFQG8HGM57DVBGBN).
# Lint and test riff, riff-client and riff-core on a Mac
test-macos:
    cargo clippy {{macos_crates}} --all-targets -- -D warnings
    {{just_executable()}} cargo-test {{macos_crates}} --no-fail-fast

# cargo test ARGS for test and check, in the environment of the tests.
# 01M49NP2907J4SH4S6MAY09VXE: .cargo/config.toml sets the TMPDIR.
# 01M511FGMFF17G2G8RV6ST9070: first each binary of the workspace, so a test
# of one crate finds the binary of another, also in a new clone.
[private]
cargo-test *ARGS:
    cargo build -q --workspace --bins
    {{test_env}} cargo test {{ARGS}}

# 01M517P9QE2SQ45SPV3K2SM71B: STRESS_RUNS runs (50) with STRESS_LOAD busy
# loops (8), in the environment of the tests. ARGS go to cargo test.
# Run a test many times on a busy machine: just stress -p riff --test all hosts::
stress *ARGS:
    cargo build -q --workspace --bins
    {{test_env}} deploy/stress.sh {{ARGS}}

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

# The static riff and riff-server of this tree (01M4ZPJ04D9MJHNPAV0WN1KCES,
# 01M4ZPJ06QYVA23N9SQ45T8WQC): a build in rust:CHANNEL-alpine with podman
# or docker, then the check: no interpreter, and riff --version runs in the
# session image of riff.toml. PROFILE: release or dev.
# Build the static riff and riff-server into target/static, and check them
static PROFILE="release":
    deploy/static/build.sh {{PROFILE}}
    deploy/static/check.sh {{PROFILE}}

# The tests of crates/riff in the session image of riff.toml, as a session
# runs them (01M51117NDKF11FNZ915TTQED4). It needs podman or docker. ARGS
# go to cargo test. --part I/N runs part I of N of the tests of the
# workspace in place of them, as the Gate does (01M515Z3Y7GSFNTJGXJD727MQK).
# Run the tests of crates/riff in a container of the session image
session-test *ARGS:
    deploy/session-test.sh {{ARGS}}

# The real-session test of a release (01M4ZCFGCY301G2ARFMRXV7XDR): the
# static riff of this tree (01M4ZPJ0FXTC78Z8GG2QA65167) runs one scratch
# item with riff run, the session image of riff.toml and a real claude. It
# needs podman. fetch.sh takes the static riff of the commit from the
# artifacts of CI, or from the release, and builds it only when it finds
# neither (01M6QQ1PDNMHQ47FG8TD85P7CN). ARGS: --skip PATH, --gate-wait
# SECONDS.
# Run the real-session test on this host: one scratch item to a pull request
real-session *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    static=$(deploy/static/fetch.sh)
    cargo build -q -p real-session
    target/debug/real-session --riff "$static/riff" {{ARGS}}

# Set up the GitHub repository for pull requests: auto-merge, squash only, the rulesets on main and the tags
github REPO="como-technologies/riff":
    deploy/github.sh {{REPO}}

# Clean build artifacts
clean:
    cargo clean
