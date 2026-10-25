#!/usr/bin/env bash
# Sets the status riff/verify on the merge group commit of the merge queue
# (01M5TKPHH2627K9NDS4P1Y1N3T). The queue runs the required checks again
# on the merge group commit, and the verify is a status of the head commit
# of the pull request. So this script copies the pass: success when the
# head commit of the pull request of the group has the status riff/verify
# with the state success, else failure. The CI job of the event
# merge_group runs it. It never starts a verify.
#
#   deploy/merge-group-verify.sh HEAD_REF GROUP_SHA
#
# HEAD_REF is the branch of the merge group, for example
# refs/heads/gh-readonly-queue/main/pr-1234-5e6f7a8. GROUP_SHA is the
# merge group commit. The repository is GH_REPO (OWNER/REPO), and gh
# signs in with GH_TOKEN, which needs the right to write statuses.
set -euo pipefail

HEAD_REF="${1:-}"
GROUP_SHA="${2:-}"
CONTEXT="riff/verify"

# The branch of each entry names its own pull request: the group of an
# entry holds the entries before it, and each of them has its own group.
if ! [[ "$HEAD_REF" =~ /pr-([0-9]+)-[0-9a-f]+$ ]] || [ -z "$GROUP_SHA" ]; then
    echo "merge-group-verify: give the merge group branch and its commit. Got \"$HEAD_REF\" and \"$GROUP_SHA\"." >&2
    exit 1
fi
PULL="${BASH_REMATCH[1]}"

set_status() {
    local url=()
    if [ -n "${3:-}" ]; then
        url=(-f target_url="$3")
    fi
    gh api -X POST "repos/$GH_REPO/statuses/$GROUP_SHA" \
        -f state="$1" -f context="$CONTEXT" -f description="$2" ${url[@]+"${url[@]}"} > /dev/null
}

HEAD=$(gh api "repos/$GH_REPO/pulls/$PULL" --jq .head.sha)
STATUS=$(gh api "repos/$GH_REPO/commits/$HEAD/status" \
    --jq "[.statuses[] | select(.context == \"$CONTEXT\")][0] | [.state // \"\", .target_url // \"\"] | @tsv")
STATE="${STATUS%%$'\t'*}"
URL="${STATUS#*$'\t'}"

if [ "$STATE" = success ]; then
    set_status success "The verify of pull request #$PULL passed at ${HEAD:0:7}" "$URL"
    echo "merge-group-verify: $CONTEXT is success on $GROUP_SHA: pull request #$PULL passed its verify at $HEAD."
else
    set_status failure "Pull request #$PULL has no passed verify at ${HEAD:0:7}"
    echo "merge-group-verify: pull request #$PULL has no passed $CONTEXT at $HEAD (state: ${STATE:-none}). $CONTEXT is failure on $GROUP_SHA." >&2
    exit 1
fi
