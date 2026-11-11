#!/usr/bin/env bash
# Prints the run that holds the newest artifact called NAME that counts,
# or nothing when no run holds one (01M6QQ1P0GH9AGCDA5N8EMEFGF,
# 01M75TT13241HC22M0KBJ6HW96, 01M75Y0JGMPRZPNJA1EFM7EM7S,
# 01M75YKD063DE6ZFA0H3K8BNZB). The build plan of CI,
# `deploy/static/fetch.sh` and `deploy/ci-test.sh` use it to find the
# binaries and the pass that a run made for a tree.
#
#   deploy/artifact-run.sh NAME
#
# NAME is gate-TREE, tests-TREE, static-PROFILE-TREE, macos-riff-TREE or
# mac-session-riff-TREE. An artifact that did not expire counts when each
# of these is true of its run:
#
# 1. It ran on a branch of this repository: a run on a fork has another
#    head repository, and its artifact is never a build of this
#    repository.
# 2. It ran the workflow .github/workflows/ci.yml.
# 3. It is this run (GITHUB_RUN_ID), or a run that this run trusts.
#    GITHUB_EVENT_NAME is the event of this run:
#    - A run of CI that is not of a pull request, for example a push to
#      main, a merge group or a tag, trusts only a merge group and a push
#      to main whose commit has the tree TREE. The job Build of a pull
#      request runs the code of its tree, and that code did not merge.
#    - A run of a pull request, and a call outside CI with no
#      GITHUB_EVENT_NAME, for example of `just ci-test`, trust a merge
#      group, a push to main, and a run whose head commit has the folders
#      .github/workflows and deploy and the justfile of main. These files
#      decide what the jobs run and how the artifacts are named.
# 4. Its job that made the artifact passed: Gate for gate-TREE, macOS
#    riff for macos-riff-TREE, Linux riff of a Mac for
#    mac-session-riff-TREE, and Build for the others.
#
# Each run that holds the artifact and does not count is named on
# stderr. GH_REPO is the repository OWNER/REPO, and gh signs in with
# GH_TOKEN or its own login. The exit status is 1 when gh cannot list
# the artifacts.
set -euo pipefail

NAME="${1:-}"
if [ -z "$NAME" ] || [ $# -ne 1 ]; then
    echo "artifact-run: give the name of one artifact, for example tests-TREE." >&2
    exit 2
fi
case "$NAME" in
gate-?*) JOB=Gate ;;
tests-?* | static-dev-?* | static-release-?*) JOB=Build ;;
macos-riff-?*) JOB="macOS riff" ;;
mac-session-riff-?*) JOB="Linux riff of a Mac" ;;
*)
    echo "artifact-run: $NAME is no artifact of a tree. Give gate-TREE, tests-TREE, static-PROFILE-TREE, macos-riff-TREE or mac-session-riff-TREE." >&2
    exit 2
    ;;
esac
if [ -z "${GH_REPO:-}" ]; then
    echo "artifact-run: give the repository in GH_REPO, for example como-technologies/riff." >&2
    exit 2
fi
TREE="${NAME##*-}"
case "${GITHUB_EVENT_NAME:-}" in
"" | pull_request) MERGED_ONLY=false ;;
*) MERGED_ONLY=true ;;
esac

runs=$(gh api "repos/$GH_REPO/actions/artifacts?name=$NAME&per_page=100" \
    --jq '[.artifacts[] | select(.expired == false and .workflow_run.repository_id == .workflow_run.head_repository_id)] | sort_by(.created_at) | reverse | .[].workflow_run.id')

# The IDs of the files of rule 3 in the commit or branch $1. Nothing when
# gh lists only a part of the files.
files_of() {
    gh api "repos/$GH_REPO/git/trees/$1?recursive=1" \
        --jq 'if .truncated then empty else [.tree[] | select(.path == ".github/workflows" or .path == "deploy" or .path == "justfile") | "\(.path)=\(.sha)"] | join(" ") end'
}

skip() {
    echo "artifact-run: $NAME of the run $run does not count: $1." >&2
}

main_files=""
read_main=false

# Rule 3 for the run $run of the event $1 on the branch $2, at the
# commit $3 with the tree $4.
trusted() {
    if [ "$run" = "${GITHUB_RUN_ID:-}" ]; then
        return 0
    fi
    if [ "$1" = merge_group ] || [ "$1 $2" = "push main" ]; then
        if [ "$MERGED_ONLY" = true ] && [ "$4" != "$TREE" ]; then
            skip "the tree of its commit $3 is $4, not $TREE"
            return 1
        fi
        return 0
    fi
    if [ "$MERGED_ONLY" = true ]; then
        skip "it is a run of $1 on $2, and a run of $GITHUB_EVENT_NAME takes only a merge group or a push to main"
        return 1
    fi
    if [ "$read_main" = false ]; then
        main_files=$(files_of main) || main_files=""
        read_main=true
    fi
    files=$(files_of "$3") || files=""
    if [ -z "$main_files" ] || [ "$files" != "$main_files" ]; then
        skip "its head commit $3 does not have the .github/workflows, the deploy and the justfile of main"
        return 1
    fi
}

for run in $(printf '%s\n' "$runs" | awk 'NF && !seen[$0]++'); do
    if ! info=$(gh api "repos/$GH_REPO/actions/runs/$run" \
        --jq '"\(.event) \(.head_branch) \(.head_sha) \(.head_commit.tree_id) \(.path)"'); then
        skip "gh could not read the run"
        continue
    fi
    read -r event branch sha tree path <<<"$info"
    if [ "${path%%@*}" != .github/workflows/ci.yml ]; then
        skip "it ran the workflow $path, not .github/workflows/ci.yml"
        continue
    fi
    if ! trusted "$event" "$branch" "$sha" "$tree"; then
        continue
    fi
    jobs=$(gh api "repos/$GH_REPO/actions/runs/$run/jobs?filter=all&per_page=100" \
        --jq '.jobs[] | "\(.name)=\(.conclusion)"') || jobs=""
    if ! grep -qx "$JOB=success" <<<"$jobs"; then
        skip "its job $JOB did not pass"
        continue
    fi
    echo "$run"
    exit 0
done
