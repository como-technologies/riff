#!/usr/bin/env bash
# Builds the image of riff-server from a static riff-server that CI built
# or that a release holds, checks the image, and pushes it
# (01M6QQ1PB372MQ7M527HVT323F). It compiles nothing.
#
#   deploy/cloud/image.sh IMAGE BINARY [--no-push]
#
# IMAGE is the name of the image with its tag. BINARY is the file of the
# static riff-server. The image is deploy/cloud/Dockerfile: the CA
# certificates and the file as /riff-server. The script copies
# /riff-server out of the image and compares its SHA-256 with the SHA-256
# of BINARY. It fails on a difference, and then it pushes nothing. With
# --no-push it builds and checks, and pushes nothing. It runs docker, or
# the engine in RIFF_ENGINE.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/../.."

IMAGE="${1:-}"
BINARY="${2:-}"
PUSH=true
if [ -z "$IMAGE" ] || [ -z "$BINARY" ] || [ $# -gt 3 ] ||
    { [ $# -eq 3 ] && [ "$3" != --no-push ]; }; then
    echo "image.sh: give the image and the file of riff-server: image.sh IMAGE BINARY [--no-push]." >&2
    exit 2
fi
if [ "${3:-}" = --no-push ]; then
    PUSH=false
fi
if [ ! -f "$BINARY" ]; then
    echo "image.sh: $BINARY is not a file. Give the static riff-server of a release or of a build." >&2
    exit 1
fi

ENGINE="${RIFF_ENGINE:-docker}"
CONTEXT=$(mktemp -d "${TMPDIR:-/tmp}/riff-image.XXXXXX")
trap 'rm -rf "$CONTEXT"' EXIT
install -m 755 "$BINARY" "$CONTEXT/riff-server"

"$ENGINE" build --tag "$IMAGE" --file deploy/cloud/Dockerfile "$CONTEXT"

# A container that never starts: its files are the files of the image.
CONTAINER=$("$ENGINE" create "$IMAGE")
"$ENGINE" cp "$CONTAINER:/riff-server" "$CONTEXT/from-image"
"$ENGINE" rm "$CONTAINER" >/dev/null
want=$(sha256sum <"$BINARY")
got=$(sha256sum <"$CONTEXT/from-image")
if [ "$got" != "$want" ]; then
    echo "image.sh: /riff-server in $IMAGE has the SHA-256 ${got%% *}, and $BINARY has ${want%% *}." >&2
    exit 1
fi
echo "image.sh: /riff-server in $IMAGE has the SHA-256 ${got%% *}, the SHA-256 of $BINARY."

if [ "$PUSH" = true ]; then
    "$ENGINE" push "$IMAGE"
fi
