#!/usr/bin/env bash
# Measures a reconnect after the network drops (#697). It runs
# `iroh-spike hold` in a network namespace of its own, made by pasta
# (it comes with podman). The namespace has a copy of the network of
# the host, and hold takes its link down and up again with `ip`. The
# host and its other programs keep their network.
#
#   drop.sh ENDPOINT_ID [DROP_AFTER] [DROP_FOR] [SECONDS]
#
# The defaults: the link goes down 20 s after the connect, stays down
# 10 s, and hold runs 90 s. The binary is target/release/iroh-spike,
# or the one in IROH_SPIKE.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"

if [ $# -lt 1 ]; then
    echo "usage: drop.sh ENDPOINT_ID [DROP_AFTER] [DROP_FOR] [SECONDS]" >&2
    exit 2
fi
PEER="$1"
AFTER="${2:-20}"
DOWN="${3:-10}"
SECONDS_="${4:-90}"
SPIKE="${IROH_SPIKE:-target/release/iroh-spike}"

if ! command -v pasta >/dev/null; then
    echo "drop.sh: pasta is missing. Install it: sudo apt install passt" >&2
    exit 1
fi
if [ ! -x "$SPIKE" ]; then
    echo "drop.sh: $SPIKE is missing. Build it: cargo build --release" >&2
    exit 1
fi

# pasta runs a shell, and the shell runs the spike: on Ubuntu, the
# AppArmor profile of pasta refuses to run the spike itself. The
# variable tells hold that it has a namespace of its own.
exec pasta --config-net --quiet -- sh -c 'export IROH_SPIKE_OWN_NETNS=1; exec "$0" "$@"' \
    "$SPIKE" hold "$PEER" --seconds "$SECONDS_" \
    --drop-after "$AFTER" --drop-for "$DOWN"
