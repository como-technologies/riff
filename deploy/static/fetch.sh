#!/usr/bin/env bash
# Takes the static riff and riff-server of the commit of HEAD, and prints
# the folder that holds them (01M6QQ1PDNMHQ47FG8TD85P7CN). CI built them
# for the tree of the commit, so this compiles nothing when CI did. The
# real-session test runs them.
#
#   deploy/static/fetch.sh
#
# The first of these steps that finds the files wins:
#
# 1. The artifact static-release-TREE, else static-dev-TREE, of a run of
#    CI that counts, as deploy/artifact-run.sh says (TREE is `git
#    rev-parse HEAD^{tree}`, as the job Build names it).
# 2. The files of the GitHub release of a tag vX.Y.Z on HEAD.
# 3. A build here: `deploy/static/build.sh dev`, then the check.
#
# The files of step 1 and step 2 go to target/static/fetched. Only Linux
# on x86-64 takes them: the binaries are for that CPU, so another host
# builds. The folder is the only line on stdout. GH_REPO names the
# repository (the repository of the folder, with no value).
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/../.."
HERE=deploy

say() {
    echo "fetch.sh: $*" >&2
}

DIR=target/static/fetched

build() {
    say "$1. It builds the static riff with the profile dev."
    deploy/static/build.sh dev >&2
    deploy/static/check.sh dev >&2
    echo target/static/debug
    exit 0
}

if [ "$(uname -s)-$(uname -m)" != Linux-x86_64 ]; then
    build "the static binaries of CI are for Linux on x86-64"
fi

if [ -z "${GH_REPO:-}" ]; then
    GH_REPO=$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>/dev/null || true)
fi
if [ -z "$GH_REPO" ]; then
    build "gh does not know the repository"
fi
export GH_REPO

# The files of step 1 and step 2 are in $DIR as riff and riff-server.
done_with() {
    chmod +x "$DIR/riff" "$DIR/riff-server"
    say "$1"
    echo "$DIR"
    exit 0
}

TREE=$(git rev-parse 'HEAD^{tree}')
for profile in release dev; do
    name="static-$profile-$TREE"
    run=$("$HERE/artifact-run.sh" "$name" 2>/dev/null || true)
    if [ -n "$run" ]; then
        rm -rf "$DIR"
        mkdir -p "$DIR"
        if gh run download "$run" --name "$name" --dir "$DIR" >&2; then
            done_with "taking $name of the CI run $run"
        fi
        say "gh could not download $name of the CI run $run."
    fi
done

for tag in $(git tag --points-at HEAD); do
    case "$tag" in
    v[0-9]*.[0-9]*.[0-9]*)
        rm -rf "$DIR"
        mkdir -p "$DIR"
        if gh release download "$tag" --dir "$DIR" \
            --pattern '*-x86_64-unknown-linux-musl' --pattern SHA256SUMS >&2 &&
            (cd "$DIR" && sha256sum --check SHA256SUMS >&2); then
            mv "$DIR/riff-x86_64-unknown-linux-musl" "$DIR/riff"
            mv "$DIR/riff-server-x86_64-unknown-linux-musl" "$DIR/riff-server"
            rm -f "$DIR/SHA256SUMS"
            done_with "taking the files of the release $tag"
        fi
        say "the release $tag has no good static files."
        ;;
    esac
done

build "no CI run has the static binaries of the tree $TREE, and no release of a tag on HEAD has them"
