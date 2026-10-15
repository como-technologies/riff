#!/usr/bin/env bash
# Checks the static riff and riff-server that build.sh made
# (01M4ZPJ06QYVA23N9SQ45T8WQC):
#
# - The ELF of each has no interpreter, so it needs no shared library.
# - riff --version runs in the session image that riff.toml pins, with
#   riff bound at /opt/riff/riff with no write, as the daemon binds it
#   (01M4Z60B7KEV0H161RBZ1SDRY2). It prints the line that it prints on
#   this machine.
#
#   deploy/static/check.sh [release|dev]
#
# It runs podman, else docker, or the engine in RIFF_ENGINE. The folder
# RIFF_STATIC_DIR, when it is set, holds the binaries in place of the
# folder of the profile.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/../.."

PROFILE="${1:-release}"
case "$PROFILE" in
release) DIR=target/static/release ;;
dev) DIR=target/static/debug ;;
*)
    echo "check.sh: \"$PROFILE\" is not a profile. Give release or dev." >&2
    exit 2
    ;;
esac
DIR="${RIFF_STATIC_DIR:-$DIR}"

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
    echo "check.sh: the check needs podman or docker. Install one." >&2
    exit 1
fi

for name in riff riff-server; do
    file="$DIR/$name"
    if [ ! -f "$file" ]; then
        echo "check.sh: $file is missing. Build it: deploy/static/build.sh $PROFILE" >&2
        exit 1
    fi
    if ! readelf -h "$file" > /dev/null 2>&1; then
        echo "check.sh: $file is not an ELF file." >&2
        exit 1
    fi
    interpreter=$(readelf -lW "$file" | sed -n 's/.*Requesting program interpreter: \(.*\)]$/\1/p')
    if [ -n "$interpreter" ]; then
        echo "check.sh: $file needs the shared libraries of $interpreter: it is not static." >&2
        exit 1
    fi
    echo "$file has no interpreter: it is static."
done

RIFF="$(readlink -f "$DIR/riff")"
IMAGE=$("$RIFF" image)
want=$("$RIFF" --version)
got=$("$ENGINE" run --rm -v "$RIFF:/opt/riff/riff:ro" "$IMAGE" /opt/riff/riff --version)
if [ "$got" != "$want" ]; then
    echo "check.sh: in $IMAGE, riff --version printed \"$got\", not \"$want\"." >&2
    exit 1
fi
echo "In $IMAGE, /opt/riff/riff --version prints \"$got\"."
