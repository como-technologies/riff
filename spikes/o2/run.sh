#!/usr/bin/env bash
# The spike of O2 (#707, #699): one work item in a rootless podman
# container, with claude -p in stream-json. See README.md.
#
# The script prints no secret. podman gets GH_TOKEN by its name, not by
# its value. The sign-in of claude is a bind with no write.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
top=$(git -C "$here" rev-parse --show-toplevel)
# The git dir of the main clone, also when this is a worktree.
common=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)
main=${MAIN:-$(dirname "$common")}
remote=${REMOTE:-https://github.com/como-technologies/riff.git}
image=${IMAGE:-localhost/riff-spike-o2:latest}
branch=${BRANCH:-spike-o2}
run=${RUN:-$(date -u +%Y%m%dT%H%M%SZ)}
dir=${SPIKE_DIR:-$HOME/.local/share/riff/spike}/o2-$run
memory=${MEMORY:-8g}
cpus=${CPUS:-4}
vol=riff-spike-o2
creds=$HOME/.claude/.credentials.json

case $branch in
    spike-*) ;;
    *) echo "BRANCH must start with spike-" >&2; exit 2 ;;
esac

mkdir -p "$dir/claude"
# Before a container makes them: are the cache volumes new?
fresh=new
podman volume exists "$vol-cargo" && podman volume exists "$vol-sccache" && fresh=warm
results=$dir/results.md
: > "$results"
failed=0
note() { printf '%s\n' "$*" | tee -a "$results"; }
pass() { note "- PASS: $*"; }
fail() { note "- FAIL: $*"; failed=1; }
since() { echo $(( $(date +%s) - $1 )); }

# No branch of the spike stays on GitHub, also after a failure.
pushed=0
remove_branch() {
    [ "$pushed" = 1 ] || return 0
    if [ -n "$(git ls-remote "$remote" "refs/heads/$branch")" ]; then
        git -C "$dir/riff" push -q origin --delete "$branch" || true
    fi
}
trap remove_branch EXIT

note "# The spike of O2, run $run"
note ""
note "The folder of the run: $dir"

note ""
note "## 1. Rootless podman"
if [ "$(podman info --format '{{.Host.Security.Rootless}}')" = true ]; then
    pass "$(podman --version) runs rootless"
else
    fail "podman does not run rootless"
fi
uid=$(id -u)
if [ "$(podman run --rm --userns=keep-id docker.io/library/alpine id -u)" = "$uid" ]; then
    pass "--userns=keep-id gives the uid $uid in the container"
else
    fail "--userns=keep-id does not give the uid $uid"
fi
for f in /etc/subuid /etc/subgid; do
    if grep -q "^$(id -un):" "$f"; then
        pass "$f has a range for $(id -un)"
    else
        fail "$f has no range for $(id -un)"
    fi
done
controllers=/sys/fs/cgroup/user.slice/user-$uid.slice/user@$uid.service/cgroup.controllers
for c in cpu memory; do
    if grep -qw "$c" "$controllers"; then
        pass "cgroup v2 gives $c to the user"
    else
        fail "cgroup v2 does not give $c to the user ($controllers)"
    fi
done

note ""
note "## 2. The image"
channel=$(sed -n 's/^channel = "\(.*\)"/\1/p' "$top/rust-toolchain.toml")
t=$(date +%s)
if podman build -t "$image" --build-arg RUST_CHANNEL="$channel" "$here" > "$dir/build.log" 2>&1; then
    pass "podman build of $image in $(since "$t") s (Rust $channel)"
else
    fail "podman build: see $dir/build.log"
    exit 1
fi
note "- Size: $(podman images --format '{{.Size}}' "$image" | head -n 1)"

note ""
note "## 3. The clone of the session"
git clone -q --reference "$main" "$remote" "$dir/riff"
pass "git clone --reference: the alternates name $(cat "$dir/riff/.git/objects/info/alternates")"
note "- The clone without its target: $(du -sh --exclude=target "$dir/riff" | cut -f1)"

# The sign-in of the person (decisions 30 and 17). Only the names go
# to podman and to a log.
GH_TOKEN=$(gh auth token)
export GH_TOKEN
minutes=$(python3 -I -c '
import json, sys, time
d = json.load(open(sys.argv[1]))["claudeAiOauth"]
print(round((d["expiresAt"] / 1000 - time.time()) / 60))
' "$creds")

cat > "$dir/claude/settings.json" <<'EOF'
{
  "permissions": {
    "allow": [
      "Read",
      "Edit",
      "Glob",
      "Grep",
      "Bash(git switch:*)",
      "Bash(git add:*)",
      "Bash(git commit:*)",
      "Bash(git push:*)",
      "Bash(git log:*)",
      "Bash(git status:*)",
      "Bash(git diff:*)"
    ]
  }
}
EOF
# claude applies the allow rules of the .claude/settings.json of the
# clone only in a trusted folder. The own Claude folder of the session
# trusts its clone, as the person trusts the main clone.
python3 -I -c '
import json, sys
json.dump({"projects": {sys.argv[2]: {"hasTrustDialogAccepted": True}}},
          open(sys.argv[1], "w"), indent=2)
' "$dir/claude/.claude.json" "$dir/riff"

# The spec of a session (decision 10 with decisions 30 to 32). The home
# is a tmpfs: each write there goes when the container ends. A login
# shell sets USER and LOGNAME; riff reads USER.
home=(--rm --userns=keep-id --memory "$memory" --cpus "$cpus"
    --mount "type=tmpfs,destination=$HOME,tmpfs-mode=0700,U=true"
    -e HOME="$HOME" -e USER="$(id -un)" -e LOGNAME="$(id -un)")
objects=(-v "$common/objects:$common/objects:ro")
gitcfg=(-v "$HOME/.gitconfig:$HOME/.gitconfig:ro")
nosign=(-e GIT_CONFIG_COUNT=1 -e GIT_CONFIG_KEY_0=commit.gpgsign -e GIT_CONFIG_VALUE_0=false)
caches=(-v "$vol-cargo:$HOME/.cargo" -e CARGO_HOME="$HOME/.cargo"
    -v "$vol-sccache:$HOME/.cache/sccache" -e SCCACHE_DIR="$HOME/.cache/sccache"
    -e RUSTC_WRAPPER=sccache -e CARGO_BUILD_JOBS="$cpus")
claude=(-v "$dir/claude:$HOME/.claude" -e CLAUDE_CONFIG_DIR="$HOME/.claude"
    -v "$creds:$HOME/.claude/.credentials.json:ro")
session=("${home[@]}" "${objects[@]}" "${gitcfg[@]}" "${nosign[@]}" -e GH_TOKEN
    "${caches[@]}" "${claude[@]}")
at() { echo -v "$1:$1" -w "$1"; }

note ""
note "## 4. The gaps of the lead review"
t=$(date +%s%N)
podman run --rm "$image" true
note "- A container of the image starts and ends in $(( ($(date +%s%N) - t) / 1000000 )) ms."

# Gap 1: the sign-in of gh.
# shellcheck disable=SC2046 # at gives the words of two options
if podman run "${session[@]}" $(at "$dir/riff") "$image" gh auth status > "$dir/gh-token.log" 2>&1; then
    pass "gap 1: gh signs in with GH_TOKEN from \`gh auth token\` (decision 30)"
else
    fail "gap 1: gh does not sign in with GH_TOKEN: see $dir/gh-token.log"
fi
if [ -d "$HOME/.config/gh" ]; then
    if podman run "${home[@]}" -v "$HOME/.config/gh:$HOME/.config/gh:ro" "$image" \
        gh auth status > "$dir/gh-bind.log" 2>&1; then
        note "- NOTE: gap 1: a bind of ~/.config/gh signs in too: the token is not in the keyring"
    else
        pass "gap 1: a bind of ~/.config/gh with no GH_TOKEN gives no sign-in: $(grep -m1 -i -E 'not logged|keyring|token' "$dir/gh-bind.log" | sed 's/^ *//')"
    fi
fi

# Gap 2: the signature.
sign='cd /tmp && git init -q t && git -C t commit -q --allow-empty -m one && git -C t log -1 --format=%G?'
if out=$(podman run "${home[@]}" "${gitcfg[@]}" "$image" sh -c "$sign" 2>&1); then
    note "- NOTE: gap 2: a commit with the git config of the person works with no change: $out"
else
    pass "gap 2: with the git config of the person, a commit fails: $(printf '%s' "$out" | grep -m1 -i -E 'gpg|sign' || printf '%s' "$out" | head -n1)"
fi
if out=$(podman run "${home[@]}" "${gitcfg[@]}" "${nosign[@]}" "$image" sh -c "$sign" 2>&1); then
    pass "gap 2: with commit.gpgsign=false from GIT_CONFIG_*, a commit works, signature status $out (decision 31)"
else
    fail "gap 2: a commit with commit.gpgsign=false fails: $out"
fi

# Gap 3: the alternates.
look='git log -1 --format=%h origin/main && git cat-file -e "origin/main^{tree}" && git status --short | wc -l'
# shellcheck disable=SC2046
if out=$(podman run "${home[@]}" "${objects[@]}" $(at "$dir/riff") "$image" sh -c "$look" 2>&1); then
    pass "gap 3: with the objects of the main clone bound with no write, git reads the clone: $(echo $out)"
else
    fail "gap 3: git does not read the clone with the objects bound: $out"
fi
# shellcheck disable=SC2046
if out=$(podman run "${home[@]}" $(at "$dir/riff") "$image" sh -c "$look" 2>&1); then
    note "- NOTE: gap 3: git reads the clone with no bind of the objects: $(echo $out)"
else
    pass "gap 3: with no bind of the objects, git fails: $(printf '%s' "$out" | head -n1)"
fi

# Gap 4 and the limits of decision 13.
want_mem=$(numfmt --from=iec "${memory^^}")
# shellcheck disable=SC2046
out=$(podman run "${session[@]}" $(at "$dir/riff") "$image" \
    sh -c 'echo $(cat /sys/fs/cgroup/memory.max) $(cat /sys/fs/cgroup/cpu.max) $(ls /proc | grep -c "^[0-9]")')
read -r got_mem got_quota got_period procs <<< "$out"
if [ "$got_mem" = "$want_mem" ] && [ "$got_quota" = "$((cpus * got_period))" ]; then
    pass "gap 4: --memory $memory and --cpus $cpus give memory.max $got_mem and cpu.max $got_quota $got_period; the container sees $procs processes"
else
    fail "gap 4: the limits are memory.max $got_mem and cpu.max $got_quota $got_period"
fi
# shellcheck disable=SC2046
podman run "${session[@]}" $(at "$dir/riff") "$image" sh -c \
    'claude --version; gh --version | head -n1; git --version; just --version; mdbook --version; sccache --version; rustc --version' \
    > "$dir/versions.log" 2>&1
note "- The tools: $(paste -sd ';' "$dir/versions.log" | sed 's/;/; /g')"

note ""
note "## 5. One item: claude -p in stream-json"
if [ "$minutes" -lt 30 ]; then
    fail "the sign-in of claude ends in $minutes minutes: run claude on the host first"
    exit 1
fi
note "- The access token of claude is good for $minutes more minutes. The bind has no write, so claude cannot refresh it."
if [ -n "$(git ls-remote "$remote" "refs/heads/$branch")" ]; then
    fail "the branch $branch is on GitHub already: remove it first"
    exit 1
fi
cat > "$dir/turn1.txt" <<EOF
You work in a git clone of riff, in the current folder. Do this one
item, then stop.

1. Run: git switch -c $branch
2. Add one empty line and then this line at the end of
   docs/src/development.md:
   <!-- $branch (#707): a line from a podman session -->
3. Commit it, with exactly this command:
   git commit -am "$branch: one comment line (#707)" --trailer "Riff-Session: $branch-$run"
4. Push it: git push -u origin $branch

Do not open a pull request. Do not print the environment or a token.
Reply with the commit ID in one line.
EOF
cat > "$dir/turn2.txt" <<'EOF'
This is a second message on the same input. Run these two commands,
and reply with what they print:
git log -1 --format='%h %G? %(trailers:key=Riff-Session,valueonly,separator=)'
git status --short --branch
EOF
pushed=1
t=$(date +%s)
# shellcheck disable=SC2046
if python3 -I "$here/drive.py" --log "$dir/events.jsonl" --stderr "$dir/claude-stderr.log" \
    --summary "$dir/events.md" --message "$dir/turn1.txt" --message "$dir/turn2.txt" \
    --secret-env GH_TOKEN --secret-file "$creds" -- \
    podman run -i "${session[@]}" $(at "$dir/riff") "$image" \
    claude -p --input-format stream-json --output-format stream-json --verbose --max-turns 25 \
    > /dev/null; then
    pass "claude -p in stream-json did 2 turns in $(since "$t") s: see $dir/events.md"
else
    fail "claude -p in stream-json: see $dir/events.md and $dir/claude-stderr.log"
fi
if [ -s "$dir/claude-stderr.log" ]; then
    note "- NOTE: the stderr of claude: $(head -c 400 "$dir/claude-stderr.log" | tr '\n' ' ')"
else
    pass "the stderr of claude is empty: the trusted clone applies its project rules"
fi
remote_commit=$(git ls-remote "$remote" "refs/heads/$branch" | cut -f1)
local_commit=$(git -C "$dir/riff" rev-parse -q --verify "refs/heads/$branch" || true)
if [ -n "$remote_commit" ] && [ "$remote_commit" = "$local_commit" ]; then
    pass "the push: $branch on GitHub is $remote_commit, the commit of the session"
    note "- The commit: $(git -C "$dir/riff" log -1 --format='signature %G?, author %an, trailer %(trailers:key=Riff-Session,valueonly,separator=)' "$local_commit")"
    note "- Its diff: $(git -C "$dir/riff" diff --shortstat "origin/main" "$local_commit")"
else
    fail "the push: GitHub has '${remote_commit}', the clone has '${local_commit}'"
fi

note ""
note "## 6. The time of just check"
note "- The cache volumes at the start of the run: $fresh."
# Each line of the log starts with the seconds since the start of just
# check. The parts: the fast checks up to the cargo test line, the build
# of the tests, and the tests.
parts() { # $1: the log
    awk '
        $2 ~ /^RIFF_SERVER=/ && !t { t = $1 }
        /Finished `test` profile/ && !b { b = $1 }
        $1 ~ /^[0-9]+$/ { e = $1 }
        END {
            if (t) printf "the fast checks %d s, the build of the tests %d s, the tests %d s", t, b - t, e - b
            else printf "the fast checks %d s, no tests", e
        }' "$1"
}
check() { # $1: the clone, $2: the name of the check
    local t log=$dir/check-$2.log
    t=$(date +%s)
    # shellcheck disable=SC2046
    if podman run "${session[@]}" $(at "$1") "$image" bash -c '
        set -o pipefail
        start=$EPOCHSECONDS
        just check 2>&1 | while IFS= read -r line; do
            printf "%d %s\n" $((EPOCHSECONDS - start)) "$line"
        done
        rc=$?
        echo "memory.peak $(cat /sys/fs/cgroup/memory.peak)"
        sccache --show-stats | grep -E "^(Compile requests|Cache hits|Cache misses) "
        exit $rc' > "$log" 2>&1; then
        pass "just check, $2: $(since "$t") s ($(parts "$log")), peak memory $(awk '/^memory.peak/ {printf "%.1f GiB", $2/2^30}' "$log"), sccache $(grep -E '^Cache (hits|misses) ' "$log" | awk '{print $1, $2, $NF}' | paste -sd ' ')"
    else
        fail "just check, $2: $(since "$t") s ($(parts "$log")): see $log"
    fi
}
check "$dir/riff" "1 (a new clone: no target)"
check "$dir/riff" "2 (the same clone again)"
if [ -n "$local_commit" ]; then
    git clone -q --reference "$main" --branch "$branch" "$remote" "$dir/riff-2"
    check "$dir/riff-2" "3 (a second new clone, the caches warm)"
fi

note ""
note "## 7. Clean up"
# shellcheck disable=SC2046
if podman run "${session[@]}" $(at "$dir/riff") "$image" git push -q origin --delete "$branch" \
    > "$dir/delete.log" 2>&1; then
    pass "the session removed the branch $branch from GitHub"
else
    note "- The session did not remove the branch: see $dir/delete.log"
fi
remove_branch
if [ -z "$(git ls-remote "$remote" "refs/heads/$branch")" ]; then
    pass "no branch $branch is on GitHub"
else
    fail "the branch $branch is still on GitHub"
fi
if [ "${KEEP:-0}" != 1 ]; then
    rm -rf "$dir/riff" "$dir/riff-2"
    note "- The clones are removed. KEEP=1 keeps them."
fi

note ""
note "Result: $([ "$failed" = 0 ] && echo pass || echo fail). The steps are in $results."
exit "$failed"
