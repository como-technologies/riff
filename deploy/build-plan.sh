#!/usr/bin/env bash
# Plans the run of CI for the tree of HEAD. The job Plan runs it before
# each other job (01M6QQ1P0GH9AGCDA5N8EMEFGF, 01M6QQ1P37XW7N465MQGPHCXZM,
# 01M75NAF4JQWQG3ZVGJM74XRBG): which artifacts of the tree exist already,
# which ones the job Build makes, and if the tests run.
#
#   deploy/build-plan.sh [TREE]
#
# TREE is the git tree of HEAD, `git rev-parse HEAD^{tree}`, unless the
# argument gives it. A tree names its artifacts, so a push to main, a
# merge group or a tag finds what the run of a pull request or of a merge
# group with the same tree made:
#
#   gate-TREE             the pass of the Gate of a run that ran the tests
#   tests-TREE            the test binaries and their manifest
#   static-PROFILE-TREE   the static riff and riff-server, PROFILE dev or release
#
# The input is in the environment: PROFILE, dev or release, the profile
# of the static build that the run wants (dev); TESTS, true or false, for
# a run that needs the tests (true); GH_REPO, the repository OWNER/REPO;
# GITHUB_RUN_ID, the run that makes what is missing. gh signs in with
# GH_TOKEN, which needs the right to read actions.
#
# The plan is lines `name=value`, in the file GITHUB_OUTPUT when it is
# set, else on stdout:
#
#   tree           the tree
#   run_tests      true when the tests and the job macOS run
#   passed_run     the run that holds gate-TREE (empty when none does)
#   build_tests    true when the job makes tests-TREE
#   tests_run      the run that holds tests-TREE (empty when no test runs)
#   build_static   true when the job makes the static binaries
#   static_profile the profile of the static binaries that the run takes
#   static_run     the run that holds static-PROFILE-TREE
#
# A tree with gate-TREE passed its Gate in that run, so the run builds no
# test binary and runs no test again. A run of the profile release wants
# the release binaries of the tree. A run of the profile dev takes the
# release binaries when they are there: the release of a tree is the
# better build. The plan takes only an artifact that counts: see
# deploy/artifact-run.sh (01M75TT13241HC22M0KBJ6HW96). When gh fails,
# the plan takes the artifact as missing.
set -euo pipefail
HERE="$(dirname "$(readlink -f "$0")")"

TREE="${1:-$(git rev-parse 'HEAD^{tree}')}"
PROFILE="${PROFILE:-dev}"
TESTS="${TESTS:-true}"
RUN="${GITHUB_RUN_ID:-}"

case "$PROFILE" in
release | dev) ;;
*)
    echo "build-plan: \"$PROFILE\" is not a profile. Give release or dev." >&2
    exit 2
    ;;
esac
case "$TESTS" in
true | false) ;;
*)
    echo "build-plan: TESTS is \"$TESTS\". Give true or false." >&2
    exit 2
    ;;
esac
if [ -z "${GH_REPO:-}" ]; then
    echo "build-plan: give the repository in GH_REPO, for example como-technologies/riff." >&2
    exit 2
fi

# The run that holds the artifact $1, or nothing.
find_run() {
    local run
    if ! run=$("$HERE/artifact-run.sh" "$1"); then
        echo "build-plan: gh could not list the artifacts named $1. The run makes it." >&2
        return 0
    fi
    echo "$run"
}

OUT=/dev/stdout
if [ -n "${GITHUB_OUTPUT:-}" ]; then
    OUT="$GITHUB_OUTPUT"
fi
emit() {
    echo "$1=$2" >>"$OUT"
}

emit tree "$TREE"

# The pass of the Gate of the tree: then no test runs again.
passed_run=""
run_tests=false
if [ "$TESTS" = true ]; then
    passed_run=$(find_run "gate-$TREE")
    if [ -z "$passed_run" ]; then
        run_tests=true
        echo "build-plan: no gate-$TREE: the tests and the job macOS run." >&2
    else
        echo "build-plan: gate-$TREE is in run $passed_run: the tree passed its Gate, so no test build and no test." >&2
    fi
fi
emit run_tests "$run_tests"
emit passed_run "$passed_run"

# The test binaries.
tests_run=""
build_tests=false
if [ "$run_tests" = true ]; then
    tests_run=$(find_run "tests-$TREE")
    if [ -z "$tests_run" ]; then
        build_tests=true
        tests_run="$RUN"
        echo "build-plan: no tests-$TREE: the job builds the test binaries." >&2
    else
        echo "build-plan: tests-$TREE is in run $tests_run: no test build." >&2
    fi
fi
emit build_tests "$build_tests"
emit tests_run "$tests_run"

# The static binaries: the release of the tree, else (for the profile
# dev) the dev binaries of the tree, else a build of the profile.
static_run=$(find_run "static-release-$TREE")
static_profile=release
if [ -z "$static_run" ] && [ "$PROFILE" = dev ]; then
    static_run=$(find_run "static-dev-$TREE")
    static_profile=dev
fi
build_static=false
if [ -z "$static_run" ]; then
    build_static=true
    static_profile="$PROFILE"
    static_run="$RUN"
    echo "build-plan: no static-$static_profile-$TREE: the job builds the static binaries of the profile $static_profile." >&2
else
    echo "build-plan: static-$static_profile-$TREE is in run $static_run: no static build." >&2
fi
emit build_static "$build_static"
emit static_profile "$static_profile"
emit static_run "$static_run"
