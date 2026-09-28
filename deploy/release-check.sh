#!/usr/bin/env bash
# Checks a release tag (01M3MRMASMP59PKHAV92XSV7XE): it has the form
# vX.Y.Z, and X.Y.Z is the version of the crates in Cargo.toml and
# Cargo.lock of ROOT. CI runs it for each pushed tag v*, and the deploy
# runs it for its input (01M3MRMAY3P1K151RGAP9K6GSH).
#
#   deploy/release-check.sh TAG [ROOT]
#
# ROOT is the checkout to check. It is the parent of deploy/ when it is
# not given.
set -euo pipefail

TAG="${1:-}"
ROOT="${2:-$(dirname "$(readlink -f "$0")")/..}"

if ! [[ "$TAG" =~ ^v([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    echo "release-check: \"$TAG\" is not a release tag. Give vX.Y.Z, for example v0.2.0." >&2
    exit 1
fi
VERSION="${TAG#v}"

CRATES=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -n 1)
if [ "$CRATES" != "$VERSION" ]; then
    echo "release-check: the tag $TAG is not the version of the crates in Cargo.toml: $CRATES." >&2
    exit 1
fi

for crate in riff riff-server; do
    LOCKED=$(awk -v name="name = \"$crate\"" \
        '$0 == name { getline; gsub(/version = |"/, ""); print; exit }' "$ROOT/Cargo.lock")
    if [ "$LOCKED" != "$VERSION" ]; then
        echo "release-check: the tag $TAG is not the version of $crate in Cargo.lock: $LOCKED. Run cargo update --workspace." >&2
        exit 1
    fi
done

echo "release-check: $TAG is the version of the crates."
