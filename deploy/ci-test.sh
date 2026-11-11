#!/usr/bin/env bash
# Runs the tests whose name has NAME from the test binaries that CI built
# for the tree of HEAD, and compiles nothing (01M75NDQY6BHZBV9PF735E3FGY).
# A verifier runs a test of the commit with it: `just ci-test NAME`.
#
#   deploy/ci-test.sh NAME
#
# 1. deploy/artifact-run.sh finds the CI run that holds tests-TREE (TREE
#    is `git rev-parse HEAD^{tree}`, as the job Build names it), and gh
#    downloads it into target/ci-test/TREE of this clone.
# 2. A test binary reads the files of the path that built it, and the
#    manifest of the archive names that path: CI builds at
#    /var/tmp/riff-ci/riff (01M75NDR15DNBFHDBNKA3CA1BE). A linked worktree
#    of this clone at the commit of HEAD goes there, and the archive is
#    unpacked in it. A clone at that path is the checkout itself.
# 3. In that checkout, `just test-named NAME DIR` runs the tests in the
#    environment of the jobs Tests I/N: a clean environment with a new
#    home, CARGO_HOME and CARGO_TARGET_DIR in target/session, and the
#    values of the tests.
#
# A second run for the same tree takes the download and the checkout of
# the first. It removes a folder at the path of the archive only when an
# earlier run made it. The exit status is 0 when the tests pass, 1 when
# one fails, and 2 when the call or the archive is wrong. It is 3 when CI
# has no test build of the tree that runs here: then build the test, for
# example with `just test NAME`. GH_REPO names the repository (the
# repository of the folder, with no value).
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."
HERE=deploy

say() {
    echo "ci-test: $*" >&2
}

NAME="${1:-}"
if [ $# -ne 1 ] || [ -z "$NAME" ]; then
    say "give the name of one test, for example: just ci-test the_archive_runs"
    exit 2
fi

# CI has no test build that runs here: exit 3, and build nothing.
no_build() {
    say "$1. Say so in your result, and build the test: just test $NAME"
    exit 3
}

# The lock FILE on the file descriptor FD, or exit 2 when another run
# holds it.
lock() {
    eval "exec $2>\"\$1\""
    if ! flock -n "$2"; then
        say "another ci-test runs with $1. Wait for its end, then run again."
        exit 2
    fi
}

if [ "$(uname -s)-$(uname -m)" != Linux-x86_64 ]; then
    no_build "the test binaries of CI are for Linux on x86-64"
fi

if [ -z "${GH_REPO:-}" ]; then
    GH_REPO=$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>/dev/null || true)
fi
if [ -z "$GH_REPO" ]; then
    no_build "gh does not know the repository"
fi
export GH_REPO

COMMIT=$(git rev-parse HEAD)
TREE=$(git rev-parse 'HEAD^{tree}')
ARTIFACT="tests-$TREE"
CACHE="$(pwd -P)/target/ci-test"
DIR="$CACHE/$TREE"

mkdir -p target
lock target/ci-test.lock 8

# Only the download of this tree stays: each is more than 1 GB.
if [ ! -f "$DIR/manifest.json" ] || [ ! -f "$DIR/tests.tar" ]; then
    if ! run=$("$HERE/artifact-run.sh" "$ARTIFACT"); then
        no_build "gh could not list the artifacts named $ARTIFACT"
    fi
    if [ -z "$run" ]; then
        no_build "CI kept no test build of the tree $TREE of HEAD: no run has $ARTIFACT"
    fi
    rm -rf "$CACHE"
    mkdir -p "$CACHE"
    if ! gh run download "$run" --name "$ARTIFACT" --dir "$DIR.part" >&2; then
        rm -rf "$DIR.part"
        no_build "gh could not download $ARTIFACT of the CI run $run"
    fi
    mv "$DIR.part" "$DIR"
    say "took $ARTIFACT of the CI run $run."
fi

ROOT=$(sed -n 's/^  "root": "\(.*\)",$/\1/p' "$DIR/manifest.json")
case "$ROOT" in
/*[!/]) ;;
*)
    say "the manifest of $ARTIFACT names no path of a checkout: \"$ROOT\"."
    exit 2
    ;;
esac
case "$ROOT" in
*[!A-Za-z0-9._/-]* | */./* | */../* | */. | */..)
    say "the manifest of $ARTIFACT names the path \"$ROOT\". ci-test takes only a plain path."
    exit 2
    ;;
esac

if ! mkdir -p "$(dirname "$ROOT")" 2>/dev/null; then
    no_build "the archive runs only at $ROOT, and ci-test cannot make that folder"
fi
lock "$ROOT.lock" 9

# The stamp says that ci-test made the checkout, and for which tree.
STAMP="$ROOT/target/ci-test.stamp"
WANT="$TREE $(git rev-parse --path-format=absolute --git-common-dir)"
here=$(pwd -P)
if [ "$ROOT" = "$here" ]; then
    if [ "$(cat "$STAMP" 2>/dev/null)" != "$WANT" ]; then
        tar -xf "$DIR/tests.tar"
        echo "$WANT" >"$STAMP"
    fi
elif [ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$WANT" ] &&
    [ "$(git -C "$ROOT" rev-parse HEAD 2>/dev/null)" = "$COMMIT" ]; then
    say "takes the checkout of an earlier run at $ROOT."
else
    if [ -e "$ROOT" ]; then
        if [ ! -f "$STAMP" ]; then
            say "$ROOT is there, and ci-test did not make it. Move it away, then run again."
            exit 2
        fi
        rm -rf "$ROOT"
    fi
    git worktree prune
    git worktree add -q --detach -f "$ROOT" "$COMMIT" >&2
    mkdir -p "$ROOT/target"
    echo made >"$STAMP"
    tar -xf "$DIR/tests.tar" -C "$ROOT"
    echo "$WANT" >"$STAMP"
    say "made a checkout of $COMMIT at $ROOT."
fi

# The environment of the jobs Tests I/N: a new home outside each git
# repository, and the folders of deploy/session-test.sh.
HOME_DIR="$(dirname "$ROOT")/ci-test-home"
rm -rf "$HOME_DIR"
mkdir -p "$HOME_DIR"
keep=(PATH="$PATH" USER="${USER:-$(id -un)}" LOGNAME="${LOGNAME:-$(id -un)}")
for name in RUSTUP_HOME LANG; do
    if [ -n "${!name:-}" ]; then
        keep+=("$name=${!name}")
    fi
done
say "runs the tests with $NAME in their name from $ARTIFACT, in $ROOT."
cd "$ROOT"
set +e
env -i "${keep[@]}" HOME="$HOME_DIR" \
    CARGO_HOME="$ROOT/target/session/cargo" CARGO_TARGET_DIR="$ROOT/target/session" \
    just test-named "$NAME" "$DIR" 8>&- 9>&-
exit $?
