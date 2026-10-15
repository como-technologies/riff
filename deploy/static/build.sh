#!/usr/bin/env bash
# Builds the static riff and riff-server of this tree for
# x86_64-unknown-linux-musl (01M4ZPJ04D9MJHNPAV0WN1KCES). The build runs
# in the image rust:CHANNEL-alpine, where CHANNEL is the channel of
# rust-toolchain.toml. On Alpine, Rust links musl into the binary, so the
# binary needs no shared library and runs in each session image.
#
#   deploy/static/build.sh [release|dev]
#
# It runs podman, else docker, or the engine in RIFF_ENGINE. The binaries
# go to target/static/release or target/static/debug. The named volume
# riff-static-cargo keeps the downloaded crates for the next build.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/../.."

PROFILE="${1:-release}"
case "$PROFILE" in
release | dev) ;;
*)
    echo "build.sh: \"$PROFILE\" is not a profile. Give release or dev." >&2
    exit 2
    ;;
esac

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
    echo "build.sh: the static build needs podman or docker. Install one." >&2
    exit 1
fi

CHANNEL=$(sed -n 's/^channel *= *"\(.*\)"$/\1/p' rust-toolchain.toml)
IMAGE="docker.io/library/rust:$CHANNEL-alpine"

# The container has no git, so the build takes its commit from
# build-id.sh (01M3JEE7YXQPWS65FBVTASAEBX).
while IFS='=' read -r name value; do
    export "$name=$value"
done < <(deploy/build-id.sh)

# RUSTUP_TOOLCHAIN is the toolchain of the image: the image tag already
# has the channel of rust-toolchain.toml, so rustup downloads nothing.
"$ENGINE" run --rm \
    -v "$PWD:/src" -w /src \
    -v riff-static-cargo:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/src/target/static \
    -e CARGO_BUILD_JOBS \
    -e RIFF_COMMIT -e RIFF_COMMIT_TIME \
    "$IMAGE" \
    sh -euc '
        apk add -q --no-cache musl-dev linux-headers
        RUSTUP_TOOLCHAIN=$(rustup default | cut -d" " -f1)
        export RUSTUP_TOOLCHAIN
        cargo build --locked --profile "$1" -p riff -p riff-server
    ' sh "$PROFILE"
