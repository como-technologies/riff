#!/usr/bin/env bash
# Runs a test many times on a busy machine, to find a test that fails
# only under load (01M517P9QE2SQ45SPV3K2SM71B).
#
#   deploy/stress.sh ARGS
#
# ARGS go to cargo test, for example `-p riff --test all hosts::`. The
# script builds the test first. Then it starts STRESS_LOAD busy loops
# (default 8), and runs `cargo test ARGS` STRESS_RUNS times (default
# 50). It keeps the output of each failed run in
# $CARGO_TARGET_DIR/stress/run-N.log (default target/stress). It stops
# the busy loops at its end, also on Ctrl-C. It exits with 1 when a run
# failed.
set -euo pipefail

runs="${STRESS_RUNS:-50}"
load="${STRESS_LOAD:-8}"
if [ "$#" -eq 0 ]; then
    echo "usage: just stress ARGS, for example: just stress -p riff --test all hosts::" >&2
    exit 2
fi
case "$runs:$load" in
    *[!0-9:]* | :* | *:) echo "stress: STRESS_RUNS and STRESS_LOAD are numbers" >&2; exit 2 ;;
esac

logs="${CARGO_TARGET_DIR:-target}/stress"
mkdir -p "$logs"
rm -f "$logs"/run-*.log

# The build runs before the load, so the load slows only the tests.
cargo test -q --no-run "$@"

loops=()
stop() {
    if [ "${#loops[@]}" -gt 0 ]; then
        kill "${loops[@]}" 2>/dev/null || true
        wait "${loops[@]}" 2>/dev/null || true
    fi
}
trap stop EXIT
trap 'exit 130' INT TERM
for _ in $(seq "$load"); do
    (while :; do :; done) &
    loops+=("$!")
done

failed=0
for n in $(seq "$runs"); do
    log="$logs/run-$n.log"
    if cargo test -q "$@" >"$log" 2>&1; then
        rm -f "$log"
        echo "run $n of $runs: pass"
    else
        failed=$((failed + 1))
        echo "run $n of $runs: FAIL, see $log"
    fi
done
echo "stress: $((runs - failed)) of $runs runs passed with $load busy loops: cargo test $*"
[ "$failed" -eq 0 ]
