#!/usr/bin/env bash
# Runs the tests of crates/riff in a container of the session image that
# riff.toml pins, as a session of `riff run` runs them
# (01M51117NDKF11FNZ915TTQED4): with --init (01M51117K3WXK35CE67KPRFMA8),
# as the user of this machine with an entry in the passwd and group
# files of the container, with a home of its own, with USER and
# LOGNAME (01M4Z60B7S8V9KKZPNBCQ6M99N), with the limits of a session
# (01M4Z60B7XT7462XMV49GJTVSJ), and with the values of `just cargo-test`.
#
#   deploy/session-test.sh [ARGS]
#   deploy/session-test.sh --part I/N
#   deploy/session-test.sh --build
#   deploy/session-test.sh --archive I/N
#
# ARGS go to cargo test, for example the name of one test. With --part,
# it runs `just test-part I/N` in place of the tests of crates/riff: part
# I of N of the tests of the workspace, with cargo. The Gate builds the
# test binaries one time (01M6QQ1NXY0CXMZEEEP413W4PK): the job Build runs
# --build, which is `just ci-build`: the checks and the test build. Each
# job Tests I/N unpacks the archive of the build, and runs --archive I/N,
# which is `just test-archived I/N`: part I of N from the archive, with
# no build (01M6QQ1P5YCT0JX90X5FCKM4W1). It runs podman, else docker, or
# the engine in RIFF_ENGINE. It pulls the image first, up to 3 times
# (01M6QPKG1X8VZTDF8RTTKFW67C). The image has a toolchain of its own, so
# the build goes to target/session.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."

if [ "${1:-}" = --part ]; then
    if [ $# -ne 2 ]; then
        echo "session-test.sh: give one part, for example --part 2/4." >&2
        exit 2
    fi
    run=(just test-part "$2")
elif [ "${1:-}" = --archive ]; then
    if [ $# -ne 2 ]; then
        echo "session-test.sh: give one part, for example --archive 2/4." >&2
        exit 2
    fi
    run=(just test-archived "$2")
elif [ "${1:-}" = --build ]; then
    if [ $# -ne 1 ]; then
        echo "session-test.sh: --build takes no argument." >&2
        exit 2
    fi
    run=(just ci-build)
else
    run=(just cargo-test -p riff "$@")
fi

ENGINE="${RIFF_ENGINE:-}"
if [ -z "$ENGINE" ]; then
    for name in podman docker; do
        if command -v "$name" >/dev/null; then
            ENGINE="$name"
            break
        fi
    done
fi
if [ -z "$ENGINE" ]; then
    echo "session-test.sh: the tests need podman or docker. Install one." >&2
    exit 1
fi

IMAGE=$(sed -n 's/^image = "\(.*\)"$/\1/p' riff.toml)
if [ -z "$IMAGE" ]; then
    echo "session-test.sh: riff.toml pins no session image." >&2
    exit 1
fi

# Pull the image first, up to 3 times, 10 seconds apart: a pull that
# fails once on the network must not fail a test part of the Gate.
for try in 1 2 3; do
    if "$ENGINE" pull "$IMAGE"; then
        break
    fi
    if [ "$try" -eq 3 ]; then
        echo "session-test.sh: the pull of the image $IMAGE failed 3 times." >&2
        exit 1
    fi
    echo "session-test.sh: the pull of the image $IMAGE failed. Try $((try + 1)) of 3 in 10 seconds." >&2
    sleep 10
done

# The user of this machine, so the files in target/session are its own.
# docker has no keep-id. So the container gets a passwd and a group file
# with an entry for the user, as keep-id of podman gives one: D-Bus, for
# example, refuses a user with no entry. Each run has new files in a new
# folder of TMPDIR, so no run changes the files of a container that runs.
# They stay there: exec leaves no step after the run.
case "$ENGINE" in
podman) user=(--userns=keep-id) ;;
*)
    ids=$(mktemp -d "${TMPDIR:-/tmp}/riff-session-ids.XXXXXX")
    "$ENGINE" run --rm "$IMAGE" cat /etc/passwd >"$ids/passwd"
    "$ENGINE" run --rm "$IMAGE" cat /etc/group >"$ids/group"
    if ! cut -d: -f3 "$ids/passwd" | grep -qx "$(id -u)"; then
        echo "$(id -un):x:$(id -u):$(id -g)::/tmp/home:/bin/bash" >>"$ids/passwd"
    fi
    if ! cut -d: -f3 "$ids/group" | grep -qx "$(id -g)"; then
        echo "$(id -gn):x:$(id -g):" >>"$ids/group"
    fi
    user=(--user "$(id -u):$(id -g)"
        -v "$ids/passwd:/etc/passwd:ro" -v "$ids/group:/etc/group:ro")
    ;;
esac

ROOT="$PWD"
binds=(-v "$ROOT:$ROOT")
# A linked worktree: its git folder is in the main clone. The build reads
# the last commit from it (crates/riff-core/build.rs).
common=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)
case "$common" in
"" | "$ROOT"/*) ;;
*) binds+=(-v "$common:$common:ro") ;;
esac

mkdir -p target/session
echo "session-test.sh: ${run[*]} in $IMAGE, with $ENGINE."
exec "$ENGINE" run --rm --init "${user[@]}" --cpus 4 --memory 8g \
    "${binds[@]}" -w "$ROOT" \
    -e HOME=/tmp/home -e USER="$(id -un)" -e LOGNAME="$(id -un)" \
    -e CARGO_HOME="$ROOT/target/session/cargo" \
    -e CARGO_TARGET_DIR="$ROOT/target/session" \
    -e CARGO_INCREMENTAL \
    "$IMAGE" \
    bash -c 'mkdir -p "$HOME" && exec "$@"' bash "${run[@]}"
