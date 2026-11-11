#!/usr/bin/env bash
# Sets up the GitHub repository for the flow of pull requests
# (01M3JFEXG85AJK8ZE8N807EQVB). It is safe to run again.
#
#   deploy/github.sh [OWNER/REPO]
#   LEAD_APP=riff-acme-lead deploy/github.sh acme/app
#
# - Repository settings: auto-merge on, squash merge only. The squash
#   commit takes the title and the body of the pull request. GitHub
#   deletes the branch after the merge.
# - The ruleset `main` on the default branch: a change needs a pull
#   request with 0 approvals and the checks Gate, Hygiene and
#   riff/verify. No force push, no deletion. No actor can bypass it
#   (01M3JN4QQCM0GXK9BCGXVS2YC7). A merge queue with the squash method
#   merges each pull request, after the checks ran on its merge group
#   (01M75NAPJVW8FJZTJEF416MR4J). The rule is the one of
#   deploy/merge-queue.sh and riff init (riff::repo_setup::merge_queue).
# - The ruleset `releases` on the tags `v*`: only the repository admin
#   role and the lead App of the riff create, move or delete a release
#   tag (01M6WZ40PZNBHW2BZHMQX80WJ8). LEAD_APP is the slug of the lead
#   App; the default is riff-como-technologies-lead. The script reads its
#   App ID first, and changes nothing when GitHub has no such App.
#
# Run it from a checkout of `main` only (01M56H9Q0757ESXMJEDM16Z7AT): an
# older copy on another branch can remove a check.
set -euo pipefail

REPO="${1:-como-technologies/riff}"
GH="${GH:-gh}"
LEAD_APP="${LEAD_APP:-riff-como-technologies-lead}"

# The App ID of the lead App, before the first change.
LEAD_ID=$("$GH" api "apps/$LEAD_APP" --jq .id 2>/dev/null || true)
case "$LEAD_ID" in
'' | *[!0-9]*)
    echo "GitHub has no App $LEAD_APP, so the setup of $REPO changed nothing. Make the Apps with riff github create, or put the slug of the lead App in LEAD_APP." >&2
    exit 1
    ;;
esac

# Makes the ruleset NAME from the JSON on stdin, or updates it.
ruleset() {
    local name=$1 json id
    json=$(cat)
    id=$("$GH" api "repos/$REPO/rulesets" --jq ".[] | select(.name == \"$name\") | .id")
    if [ -n "$id" ]; then
        echo "$json" | "$GH" api -X PUT "repos/$REPO/rulesets/$id" --input - >/dev/null
        echo "Updated the ruleset $name ($id) of $REPO."
    else
        echo "$json" | "$GH" api -X POST "repos/$REPO/rulesets" --input - >/dev/null
        echo "Made the ruleset $name of $REPO."
    fi
}

"$GH" api -X PATCH "repos/$REPO" \
    -F allow_auto_merge=true \
    -F allow_squash_merge=true \
    -F allow_merge_commit=false \
    -F allow_rebase_merge=false \
    -f squash_merge_commit_title=PR_TITLE \
    -f squash_merge_commit_message=PR_BODY \
    -F delete_branch_on_merge=true >/dev/null

ruleset main <<'JSON'
{
  "name": "main",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "bypass_actors": [],
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
    },
    {
      "type": "merge_queue",
      "parameters": {
        "merge_method": "SQUASH",
        "grouping_strategy": "ALLGREEN",
        "max_entries_to_build": 5,
        "min_entries_to_merge": 1,
        "max_entries_to_merge": 5,
        "min_entries_to_merge_wait_minutes": 5,
        "check_response_timeout_minutes": 60
      }
    }
  ]
}
JSON

# RepositoryRole 5 is the admin role. An Integration is a GitHub App, by
# its App ID. riff init makes the same ruleset (riff::repo_setup::releases).
ruleset releases <<JSON
{
  "name": "releases",
  "target": "tag",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["refs/tags/v*"], "exclude": [] } },
  "bypass_actors": [
    { "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always" },
    { "actor_id": $LEAD_ID, "actor_type": "Integration", "bypass_mode": "always" }
  ],
  "rules": [
    { "type": "creation" },
    { "type": "update" },
    { "type": "deletion" }
  ]
}
JSON
echo "Set up $REPO: auto-merge, squash only, the checks Gate, Hygiene and riff/verify and a merge queue on main, release tags only by an admin and the lead App $LEAD_APP."
