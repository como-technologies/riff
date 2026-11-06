#!/usr/bin/env bash
# Prints the run that holds the newest artifact called NAME, or nothing
# when no run holds one (01M6QQ1P0GH9AGCDA5N8EMEFGF). The build plan of
# CI and `deploy/static/fetch.sh` use it to find the binaries that a run
# built for a tree.
#
#   deploy/artifact-run.sh NAME
#
# GH_REPO is the repository OWNER/REPO, and gh signs in with GH_TOKEN or
# its own login. It counts only an artifact that did not expire, and that
# a run on a branch of this repository made: a run on a fork has another
# head repository, and its artifact is never a build of this repository.
set -euo pipefail

NAME="${1:-}"
if [ -z "$NAME" ] || [ $# -ne 1 ]; then
    echo "artifact-run: give the name of one artifact, for example tests-TREE." >&2
    exit 2
fi
if [ -z "${GH_REPO:-}" ]; then
    echo "artifact-run: give the repository in GH_REPO, for example como-technologies/riff." >&2
    exit 2
fi

gh api "repos/$GH_REPO/actions/artifacts?name=$NAME&per_page=100" \
    --jq '[.artifacts[] | select(.expired == false and .workflow_run.repository_id == .workflow_run.head_repository_id)] | sort_by(.created_at) | last | .workflow_run.id // empty'
