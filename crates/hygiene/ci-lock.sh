# One `just ci` at a time in a worktree (01M43DKYVAX0TJ2F5YYGYFSZ4G).
# Source this file in a recipe, then call `ci_lock DIR`. See the module
# `hygiene::ci` for the design. It uses no cargo: a second cargo waits
# for the build lock of the first run, so the check would not stop at
# once.

# The start time of process $1, or nothing when it does not run. The pid
# and this time name one process: a new process can get the same pid.
ci_lock_start() {
    # shellcheck disable=SC2046 # the split makes the spaces of ps one form
    echo $(ps -o lstart= -p "$1" 2>/dev/null)
}

# Takes the lock DIR/.riff-ci.lock for this shell, and removes it at the
# exit. A run that the holder starts (RIFF_CI_LOCK is set) takes no
# lock. A lock of a process that does not run does not count. A live
# lock stops the run with one line and the exit code 1.
ci_lock() {
    [ -n "${RIFF_CI_LOCK:-}" ] && return 0
    local dir=$1 lock=$1/.riff-ci.lock pid start epoch
    mkdir -p "$dir"
    for _ in 1 2; do
        if (set -C; printf '%s\n%s\n%s\n' "$$" "$(ci_lock_start $$)" "$(date +%s)" > "$lock") 2>/dev/null; then
            export RIFF_CI_LOCK=$$
            trap 'rm -f "'"$lock"'"' EXIT
            return 0
        fi
        { read -r pid; read -r start; read -r epoch; } < "$lock" 2>/dev/null || true
        if [ -n "${pid:-}" ] && [ -n "${start:-}" ] && [ "$(ci_lock_start "$pid")" = "$start" ]; then
            local min=$(( ($(date +%s) - ${epoch:-0}) / 60 ))
            echo "a just ci runs in this worktree already (pid $pid, started $min min ago); wait for it, or stop it" >&2
            exit 1
        fi
        rm -f "$lock"
    done
    echo "just ci cannot take the lock $lock" >&2
    exit 1
}
