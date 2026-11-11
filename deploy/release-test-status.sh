#!/usr/bin/env bash
# Sets the status riff/release-test of a commit from the release test of
# its release binaries (01M75TN3M9JEVZQXEA2E8FTBKS): success when the
# check and the smoke test passed, else failure. The CI job Static riff
# runs it for a push, to main or of a tag, when it checked the binaries
# of the profile release. No ruleset requires the status.
#
#   deploy/release-test-status.sh OUTCOME COMMIT RUN_URL
#
# OUTCOME is the outcome of the step of the smoke test: success, failure,
# cancelled or skipped. A step is skipped when an earlier step, for
# example the check, failed. RUN_URL is the URL of the run, the target of
# the status. The repository is GH_REPO (OWNER/REPO), and gh signs in
# with GH_TOKEN, which needs the right to write statuses.
set -euo pipefail

OUTCOME="${1:-}"
COMMIT="${2:-}"
RUN_URL="${3:-}"
CONTEXT="riff/release-test"

case "$OUTCOME" in
success)
    state=success
    description="The check and the smoke test of the release binaries passed."
    ;;
failure | cancelled | skipped)
    state=failure
    description="The check or the smoke test of the release binaries did not pass: $OUTCOME."
    ;;
*)
    echo "release-test-status: \"$OUTCOME\" is not an outcome of a step. Give success, failure, cancelled or skipped." >&2
    exit 2
    ;;
esac
if [ -z "$COMMIT" ] || [ -z "$RUN_URL" ] || [ $# -ne 3 ]; then
    echo "release-test-status: give the outcome, the commit and the URL of the run." >&2
    exit 2
fi
if [ -z "${GH_REPO:-}" ]; then
    echo "release-test-status: give the repository in GH_REPO, for example como-technologies/riff." >&2
    exit 2
fi

gh api -X POST "repos/$GH_REPO/statuses/$COMMIT" \
    -f state="$state" -f context="$CONTEXT" -f description="$description" \
    -f target_url="$RUN_URL" > /dev/null
echo "The status $CONTEXT of $COMMIT is $state: $description"
