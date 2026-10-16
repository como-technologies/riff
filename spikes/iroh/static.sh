#!/usr/bin/env bash
# Builds the spike as one static binary for x86_64-unknown-linux-musl,
# as deploy/static/build.sh builds riff: in rust:CHANNEL-alpine, with
# the release profile of riff. The binary goes to
# target/static/release/iroh-spike, and the script prints its size.
#
#   static.sh
#
# It runs podman, else docker, or the engine in RIFF_ENGINE.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"

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
    echo "static.sh: the static build needs podman or docker. Install one." >&2
    exit 1
fi

CHANNEL=$(sed -n 's/^channel *= *"\(.*\)"$/\1/p' ../../rust-toolchain.toml)
IMAGE="docker.io/library/rust:$CHANNEL-alpine"

"$ENGINE" run --rm \
    -v "$PWD:/src" -w /src \
    -v riff-static-cargo:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/src/target/static \
    -e CARGO_BUILD_JOBS \
    "$IMAGE" \
    sh -euc '
        apk add -q --no-cache musl-dev linux-headers
        RUSTUP_TOOLCHAIN=$(rustup default | cut -d" " -f1)
        export RUSTUP_TOOLCHAIN
        cargo build --locked --release
    '
BIN=target/static/release/iroh-spike
if readelf -l "$BIN" | grep -q 'INTERP'; then
    echo "static.sh: $BIN has an ELF interpreter: it is not static." >&2
    exit 1
fi
echo "$BIN is static: $(stat -c %s "$BIN") bytes"
