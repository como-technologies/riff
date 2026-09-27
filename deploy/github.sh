#!/usr/bin/env bash
# Sets up the GitHub repository for the flow of pull requests
# (01M3JFEXG85AJK8ZE8N807EQVB). It is safe to run again.
#
#   deploy/github.sh [OWNER/REPO]
#
# - Repository settings: auto-merge on, squash merge only. The squash
#   commit takes the title and the body of the pull request. GitHub
#   deletes the branch after the merge.
# - The ruleset `main` on the default branch: a change needs a pull
#   request with 0 approvals and the checks Gate, Hygiene and
#   riff/verify. No force push, no deletion. Only the repository admin
#   role can bypass it.
set -euo pipefail

REPO="${1:-como-technologies/riff}"
GH="${GH:-gh}"
NAME=main

"$GH" api -X PATCH "repos/$REPO" \
    -F allow_auto_merge=true \
    -F allow_squash_merge=true \
    -F allow_merge_commit=false \
    -F allow_rebase_merge=false \
    -f squash_merge_commit_title=PR_TITLE \
    -f squash_merge_commit_message=PR_BODY \
    -F delete_branch_on_merge=true >/dev/null

# The repository role `admin` has the ID 5.
RULESET=$(cat <<'JSON'
{
  "name": "main",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "bypass_actors": [
    { "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always" }
  ],
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": false,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": false,
        "allowed_merge_methods": ["squash"]
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": false,
        "required_status_checks": [
          { "context": "Gate" },
          { "context": "Hygiene" },
          { "context": "riff/verify" }
        ]
      }
    }
  ]
}
JSON
)

ID=$("$GH" api "repos/$REPO/rulesets" --jq ".[] | select(.name == \"$NAME\") | .id")
if [ -n "$ID" ]; then
    echo "$RULESET" | "$GH" api -X PUT "repos/$REPO/rulesets/$ID" --input - >/dev/null
    echo "Updated the ruleset $NAME ($ID) of $REPO."
else
    echo "$RULESET" | "$GH" api -X POST "repos/$REPO/rulesets" --input - >/dev/null
    echo "Made the ruleset $NAME of $REPO."
fi
echo "Set up $REPO: auto-merge, squash only, the checks Gate, Hygiene and riff/verify."
