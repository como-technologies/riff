#!/usr/bin/env bash
# The release test of a tree (01M75TN3HCK6VBB2GRCJG1Y5B2): a smoke test of
# the static riff and riff-server that build.sh made, with no cargo test.
# It runs the binaries as a person runs them:
#
# 1. riff --version and riff-server --version run, and name the same
#    build.
# 2. riff-server starts with no store and no sign-in on a free port of
#    127.0.0.1, and serves within 30 seconds.
# 3. riff server finds the same build there, and that it serves.
# 4. riff post puts a message in the thread smoke, and riff read shows it.
# 5. riff-server stops on SIGTERM within 30 seconds, with the exit code 0.
#
#   deploy/static/smoke.sh [release|dev]
#
# The folder RIFF_STATIC_DIR, when it is set, holds the binaries in place
# of the folder of the profile. RIFF_SMOKE_WAIT sets the seconds of each
# wait (30). Each binary runs with a clean environment and a new home: no
# setting of the machine counts, and no keyring is asked. At the first
# step that fails, the test names the step and exits with the code 1.
# The test needs only bash and the tools of a Mac, so the tests of riff
# run it on a Mac too: a Mac has no timeout of GNU
# (01M75YTFGK1SK5EP89V28FJH02).
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/../.."

PROFILE="${1:-release}"
case "$PROFILE" in
release) DIR=target/static/release ;;
dev) DIR=target/static/debug ;;
*)
    echo "smoke.sh: \"$PROFILE\" is not a profile. Give release or dev." >&2
    exit 2
    ;;
esac
DIR="$(readlink -f "${RIFF_STATIC_DIR:-$DIR}")"
WAIT="${RIFF_SMOKE_WAIT:-30}"

fail() {
    echo "smoke.sh: $1" >&2
    exit 1
}

for name in riff riff-server; do
    if [ ! -f "$DIR/$name" ] || [ ! -x "$DIR/$name" ]; then
        fail "$DIR/$name is missing, or it cannot run. Build it: deploy/static/build.sh $PROFILE"
    fi
done

TMP=$(mktemp -d)
SERVER=""
cleanup() {
    if [ -n "$SERVER" ]; then
        kill -KILL "$SERVER" 2>/dev/null || true
        wait "$SERVER" 2>/dev/null || true
    fi
    rm -rf "$TMP"
}
trap cleanup EXIT
mkdir "$TMP/home"
LOG="$TMP/riff-server.log"

# The clean environment of each binary: a new home, no RIFF_SERVER, no
# D-Bus, a RIFF_HOME and so no keyring (also on a Mac), and the user of
# the test.
CLEAN=(env -i "PATH=$PATH" "HOME=$TMP/home" "TMPDIR=$TMP" "RIFF_USER=smoke"
    "RIFF_HOME=$TMP/home/riff" "DBUS_SESSION_BUS_ADDRESS=unix:path=$TMP/no-bus"
    "NO_COLOR=1")

# Runs ARGS, and stops them with SIGTERM after WAIT seconds. The watch
# writes to no pipe of the caller, so a $(...) does not wait for it.
in_time() {
    "$@" &
    local pid=$! code=0
    (
        for _ in $(seq $((WAIT * 10))); do
            kill -0 "$pid" 2>/dev/null || exit 0
            sleep 0.1
        done
        kill -TERM "$pid" 2>/dev/null
    ) </dev/null >/dev/null 2>&1 &
    local watch=$!
    wait "$pid" || code=$?
    wait "$watch" || true
    return "$code"
}

# Runs riff with ARGS in the clean environment, in the folder of the test.
run_riff() {
    (cd "$TMP" && in_time "${CLEAN[@]}" "$DIR/riff" "$@")
}

# The last lines of the log of riff-server.
server_log() {
    printf 'The log of riff-server:\n%s' "$(tail -n 20 "$LOG")"
}

# 1. The binaries start, and are of one build.
riff_version=$(run_riff --version 2>&1) ||
    fail "riff does not start: riff --version failed: $riff_version"
server_version=$(in_time "${CLEAN[@]}" "$DIR/riff-server" --version 2>&1) ||
    fail "riff-server does not start: riff-server --version failed: $server_version"
case "$riff_version" in
"riff "?*) build="${riff_version#riff }" ;;
*) fail "riff --version printed \"$riff_version\", not \"riff VERSION BUILD TIME\"." ;;
esac
if [ "$server_version" != "riff-server $build" ]; then
    fail "riff --version printed \"$riff_version\", and riff-server --version \"$server_version\": not one build."
fi
echo "riff and riff-server start, and have the build $build."

# 2. riff-server serves on a free port.
(cd "$TMP" && exec "${CLEAN[@]}" "$DIR/riff-server" --listen 127.0.0.1:0) >"$LOG" 2>&1 &
SERVER=$!
port=""
for _ in $(seq $((WAIT * 10))); do
    if grep -q '"message":"riff-server serves"' "$LOG"; then
        port=$(sed -n 's/.*"message":"riff-server listens on 127\.0\.0\.1:\([0-9][0-9]*\)".*/\1/p' "$LOG" | head -n 1)
        break
    fi
    if ! kill -0 "$SERVER" 2>/dev/null; then
        wait "$SERVER" && code=0 || code=$?
        SERVER=""
        fail "riff-server does not start: it ended with the exit code $code. $(server_log)"
    fi
    sleep 0.1
done
if [ -z "$port" ]; then
    fail "riff-server does not serve after $WAIT seconds. $(server_log)"
fi
URL="http://127.0.0.1:$port"
echo "riff-server serves at $URL."

# 3. riff finds its own build on the server.
facts=$(run_riff --server "$URL" server 2>&1) ||
    fail "riff server failed: $facts"
if ! grep -Eq '^ +release +v[^ ]+ +same build' <<<"$facts" || ! grep -Eq '^ +serves +yes$' <<<"$facts"; then
    fail "riff server does not find the same build at $URL, or the server does not serve:
$facts"
fi
echo "riff server finds the same build at $URL, and it serves."

# 4. A message goes through the server.
message="smoke test $build"
out=$(run_riff --server "$URL" post --thread smoke "$message" 2>&1) ||
    fail "riff post failed: $out"
out=$(run_riff --server "$URL" read --thread smoke --all 2>&1) ||
    fail "riff read failed: $out"
if ! grep -qF -- "$message" <<<"$out"; then
    fail "riff read does not show the message of riff post in the thread smoke:
$out"
fi
echo "riff post put a message in the thread smoke, and riff read shows it."

# 5. riff-server stops on SIGTERM. The shell takes the exit code of a
# child that ended, so kill -0 fails then.
kill -TERM "$SERVER"
for _ in $(seq $((WAIT * 10))); do
    if ! kill -0 "$SERVER" 2>/dev/null; then
        break
    fi
    sleep 0.1
done
if kill -0 "$SERVER" 2>/dev/null; then
    fail "riff-server did not stop $WAIT seconds after SIGTERM. $(server_log)"
fi
wait "$SERVER" && code=0 || code=$?
SERVER=""
if [ "$code" != 0 ]; then
    fail "riff-server stopped on SIGTERM with the exit code $code, not 0. $(server_log)"
fi
echo "riff-server stops on SIGTERM with the exit code 0."
echo "The release test of $DIR passed."
