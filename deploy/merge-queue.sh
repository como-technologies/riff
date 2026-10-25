#!/usr/bin/env bash
# Turns the merge queue of a ruleset on or off (01M5TKPHDNHY75BHPD4XVBJRN9).
# It changes the rule merge_queue of the ruleset and no other rule. It needs
# the right of an admin of the repository. It writes the ruleset as it was
# to a file first, so a person can restore it.
#
#   deploy/merge-queue.sh on|off OWNER/REPO [RULESET]
#
# RULESET is the name of the ruleset. It is main when it is not given. The
# queue merges with the method squash, builds up to 5 merge groups at the
# same time, and waits 60 minutes for the checks of a group.
set -euo pipefail

USAGE="usage: deploy/merge-queue.sh on|off OWNER/REPO [RULESET]"
MODE="${1:-}"
REPO="${2:-}"
NAME="${3:-main}"

if { [ "$MODE" != on ] && [ "$MODE" != off ]; } || [ -z "$REPO" ]; then
    echo "$USAGE" >&2
    exit 2
fi

RULE='{"type":"merge_queue","parameters":{"merge_method":"SQUASH","grouping_strategy":"ALLGREEN","max_entries_to_build":5,"min_entries_to_merge":1,"max_entries_to_merge":5,"min_entries_to_merge_wait_minutes":5,"check_response_timeout_minutes":60}}'

ID=$(gh api "repos/$REPO/rulesets?per_page=100" --jq "[.[] | select(.name == \"$NAME\")][0].id // \"\"")
if [ -z "$ID" ]; then
    echo "merge-queue: $REPO has no ruleset named $NAME." >&2
    exit 1
fi

DIR=$(mktemp -d)
gh api "repos/$REPO/rulesets/$ID" > "$DIR/before.json"
echo "merge-queue: the ruleset $NAME ($ID) as it was: $DIR/before.json"

# The body of the update: the fields that the update accepts, and the
# rules without a merge_queue rule, then the rule when the mode is on.
ADD='[]'
if [ "$MODE" = on ]; then
    ADD="[$RULE]"
fi
gh api "repos/$REPO/rulesets/$ID" \
    --jq "{name, target, enforcement, conditions, bypass_actors, rules: ([.rules[] | select(.type != \"merge_queue\")] + $ADD)}" \
    > "$DIR/after.json"

gh api -X PUT "repos/$REPO/rulesets/$ID" --input "$DIR/after.json" > /dev/null
RULES=$(gh api "repos/$REPO/rulesets/$ID" --jq '[.rules[].type] | join(", ")')
echo "merge-queue: $MODE. The rules of the ruleset $NAME: $RULES"
