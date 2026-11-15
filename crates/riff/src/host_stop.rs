//! The work of the sessions at a stop of the host (#2164,
//! 01M7EWYHX092A3QWM38RNXYWMA, 01M7EWYR79TXXNCD731FHX9K6B,
//! 01M7EWYR9V4FHSJ83KBH3GP53X).
//!
//! # Design
//!
//! A stop or a restart of the host ends the container of each session.
//! At a pause, each worker pushes its work as a WIP commit at its next
//! step. A stop of the host gives a worker no such time. So the daemon
//! pushes the work itself, in the container of each work session, before
//! the stop goes on.
//!
//! On Linux, logind tells each program of a stop or a restart of the
//! host: the signal `PrepareForShutdown`. The daemon holds a delay lock of
//! logind for `shutdown` ([`crate::platform::host_stops`]). logind then
//! waits for the lock before the stop, at most `InhibitDelayMaxSec` of
//! `logind.conf`: 5 seconds by default. At the signal, the daemon
//! ([`crate::daemon::Daemon::push_work`]):
//!
//! 1. finds each work session: each session that is not the lead and not
//!    a verify ([`works`]);
//! 2. runs [`wip_command`] in the container of each one, all at the same
//!    time ([`crate::runtime::Runtime::run_in`]): the git of the container
//!    commits each change as the commit [`WIP_MESSAGE`], and pushes the
//!    branch of the item;
//! 3. waits at most [`limit_of`] the delay of logind. A session that does
//!    not answer in that time ends with its container, as before;
//! 4. writes one line for each session in its log, and drops the lock:
//!    the stop goes on ([`each_stop`]).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant L as logind
//!     participant D as the riff daemon
//!     participant C as the container of issue-12
//!     participant O as origin
//!     D->>L: Inhibit shutdown, delay: the lock
//!     Note over L: a person restarts the host
//!     L->>D: PrepareForShutdown(true)
//!     D->>C: podman exec: git add, commit, push
//!     C->>O: push worktree-issue-12 (WIP: the host stops)
//!     D->>D: the log: issue-12 pushed its work
//!     D->>L: drop the lock
//!     Note over L: the stop goes on: each container ends
//! ```
//!
//! The git of the push runs in the container, with the config and the
//! token on GitHub of the session: the host runs no git in a clone
//! ([`crate::host_git`]). The commit has no hook and no signature. The
//! push needs the clone on the branch of the item, so it never goes to
//! the default branch. It pushes with `--force-with-lease`: the branch of
//! a rebase goes too, and a commit of another session on origin stays.
//! When origin has each commit of the clone already, and the clone has no
//! change, it pushes nothing: a session that did no work yet makes no
//! branch.
//!
//! At its next start, the daemon does the steps after the end of each
//! session whose container does not run. The wave starts the item again,
//! and the clone of the new session goes on from the branch on origin
//! ([`crate::run::make_clone`]). The daemon sends no verify request for a
//! WIP head (01M5P1XQ6WTGFHW0P5M114P535).
//!
//! When logind cancels the stop (`PrepareForShutdown(false)`), the daemon
//! takes the lock again. The sessions go on, and their WIP commits stay
//! on origin. When the daemon cannot take the lock, its log says so once
//! ([`crate::text::host_stop_unheard`]): at a stop of the host, the
//! sessions then push nothing. On macOS, no session runs, and the daemon
//! takes no lock.

use std::future::Future;
use std::process::Output;
use std::time::Duration;

use anyhow::Result;

use crate::daemon::Record;
use crate::room::Role;

/// The message of the WIP commit of a session at a stop of the host. A
/// head with it is a WIP head ([`crate::rollout::wip`]).
pub const WIP_MESSAGE: &str = "WIP: the host stops";

/// The last line that [`WIP_SCRIPT`] prints when origin has each commit
/// of the clone already, and the clone has no change: it pushed nothing.
pub const ON_ORIGIN: &str = "on origin";

/// The script of [`wip_command`], for `sh` in the clone of the session:
/// `$1` is the branch of the item, and `$2` the message of the commit.
pub const WIP_SCRIPT: &str = "branch=$1
head=$(git symbolic-ref --quiet --short HEAD) || head=
if [ \"$head\" != \"$branch\" ]; then
    echo \"the clone is on ${head:-no branch}, not on $branch\" >&2
    exit 3
fi
git add -A || exit
if ! git diff --cached --quiet; then
    git -c commit.gpgsign=false commit --quiet --no-verify -m \"$2\" || exit
fi
if [ -n \"$(git for-each-ref --contains HEAD --count=1 --format=x refs/remotes/origin)\" ]; then
    echo 'on origin'
    exit 0
fi
git push --quiet --no-verify --force-with-lease origin \"HEAD:refs/heads/$branch\"
";

/// The command that pushes the work of a session on `branch`, in its
/// container: [`WIP_SCRIPT`]. The branch and the message are arguments
/// of `sh`, not parts of the script.
///
/// ```
/// use riff::host_stop::{WIP_MESSAGE, WIP_SCRIPT, wip_command};
///
/// let command = wip_command("worktree-issue-12");
/// assert_eq!(command, ["sh", "-c", WIP_SCRIPT, "sh", "worktree-issue-12", WIP_MESSAGE]);
/// assert!(WIP_SCRIPT.contains("--force-with-lease origin \"HEAD:refs/heads/$branch\""));
/// assert!(WIP_SCRIPT.contains("commit --quiet --no-verify"));
/// assert!(riff::rollout::wip(WIP_MESSAGE));
/// ```
pub fn wip_command(branch: &str) -> Vec<String> {
    ["sh", "-c", WIP_SCRIPT, "sh", branch, WIP_MESSAGE]
        .map(str::to_owned)
        .into()
}

/// True for a work session: a session that is not the lead and not a
/// verify. At a stop of the host, each one pushes its work.
pub fn works(record: &Record) -> bool {
    !record.terminal && Role::of(&record.live.item) == Role::Work
}

/// The part of the delay of logind that is kept for the lines of the
/// log after the pushes.
pub const MARGIN: Duration = Duration::from_secs(1);

/// The shortest time limit of the pushes.
pub const FLOOR: Duration = Duration::from_secs(1);

/// The time limit of the pushes for the delay `delay` of logind: the
/// delay less [`MARGIN`], and at least [`FLOOR`].
///
/// ```
/// use std::time::Duration;
/// use riff::host_stop::limit_of;
///
/// assert_eq!(limit_of(Duration::from_secs(5)), Duration::from_secs(4));
/// assert_eq!(limit_of(Duration::from_secs(30)), Duration::from_secs(29));
/// assert_eq!(limit_of(Duration::ZERO), Duration::from_secs(1));
/// ```
pub fn limit_of(delay: Duration) -> Duration {
    delay.saturating_sub(MARGIN).max(FLOOR)
}

/// What a push did, from the output `out` of [`wip_command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pushed {
    /// It pushed the work to the branch.
    Pushed,
    /// origin has each commit of the clone already: it pushed nothing.
    OnOrigin,
    /// It failed: the last line of its stderr, or its exit.
    Failed(String),
}

impl Pushed {
    /// What the push of `out` did.
    ///
    /// ```
    /// use std::os::unix::process::ExitStatusExt;
    /// use std::process::{ExitStatus, Output};
    /// use riff::host_stop::Pushed;
    ///
    /// let out = |status: i32, stdout: &str, stderr: &str| Output {
    ///     status: ExitStatus::from_raw(status),
    ///     stdout: stdout.into(),
    ///     stderr: stderr.into(),
    /// };
    /// assert_eq!(Pushed::of(&out(0, "", "")), Pushed::Pushed);
    /// assert_eq!(Pushed::of(&out(0, "on origin\n", "")), Pushed::OnOrigin);
    /// let rejected = "To github.com:o/r.git\n ! [rejected] HEAD -> worktree-issue-12 (stale info)\n";
    /// assert_eq!(
    ///     Pushed::of(&out(1 << 8, "", rejected)),
    ///     Pushed::Failed("! [rejected] HEAD -> worktree-issue-12 (stale info)".into())
    /// );
    /// assert_eq!(Pushed::of(&out(3 << 8, "", "")), Pushed::Failed("exit status: 3".into()));
    /// ```
    pub fn of(out: &Output) -> Pushed {
        let last = |bytes: &[u8]| {
            String::from_utf8_lossy(bytes)
                .lines()
                .map(str::trim)
                .rfind(|line| !line.is_empty())
                .map(str::to_owned)
        };
        if out.status.success() {
            return match last(&out.stdout).as_deref() {
                Some(ON_ORIGIN) => Pushed::OnOrigin,
                _ => Pushed::Pushed,
            };
        }
        Pushed::Failed(last(&out.stderr).unwrap_or_else(|| out.status.to_string()))
    }
}

/// The notices of a stop of the host, and the lock that holds the stop:
/// logind on Linux ([`crate::platform::logind`]).
pub trait Notices {
    /// The lock: while it lives, the stop waits, at most for the delay of
    /// the OS.
    type Lock: Send;

    /// Takes the lock.
    fn lock(&mut self) -> impl Future<Output = Result<Self::Lock>> + Send;

    /// The next notice: true when the host stops, false when the OS
    /// cancelled a stop. An error when the notices end.
    fn next(&mut self) -> impl Future<Output = Result<bool>> + Send;
}

/// Runs `at_stop` at each notice of a stop of the host from `notices`,
/// while it holds the lock, and then drops the lock: the stop goes on.
/// After a cancel of the stop, it takes the lock again. It ends only with
/// an error of `notices`.
pub async fn each_stop<N: Notices, F: Future<Output = ()>>(
    mut notices: N,
    mut at_stop: impl FnMut() -> F,
) -> Result<()> {
    loop {
        let lock = notices.lock().await?;
        while !notices.next().await? {}
        at_stop().await;
        drop(lock);
        while notices.next().await? {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// The notices of a test, and what happened, in order.
    struct Script {
        notices: VecDeque<bool>,
        said: Arc<Mutex<Vec<String>>>,
    }

    /// The lock of a [`Script`]: its drop says so.
    struct Held(Arc<Mutex<Vec<String>>>);

    impl Drop for Held {
        fn drop(&mut self) {
            self.0.lock().unwrap().push("drop".into());
        }
    }

    impl Notices for Script {
        type Lock = Held;

        async fn lock(&mut self) -> Result<Held> {
            self.said.lock().unwrap().push("lock".into());
            Ok(Held(self.said.clone()))
        }

        async fn next(&mut self) -> Result<bool> {
            self.notices
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("the notices ended"))
        }
    }

    /// 01M7EWYHX092A3QWM38RNXYWMA: the pushes run while the lock holds
    /// the stop, and the lock goes after them. After a cancel, the lock
    /// comes again, and a cancel with no stop runs nothing.
    #[tokio::test]
    async fn the_pushes_run_while_the_lock_holds_the_stop_and_a_cancel_takes_the_lock_again() {
        let said = Arc::new(Mutex::new(Vec::new()));
        let script = Script {
            notices: [false, true, false, true].into(),
            said: said.clone(),
        };
        let pushes = said.clone();
        let ended = each_stop(script, || {
            let pushes = pushes.clone();
            async move { pushes.lock().unwrap().push("push".into()) }
        })
        .await;
        assert_eq!(ended.unwrap_err().to_string(), "the notices ended");
        assert_eq!(
            *said.lock().unwrap(),
            ["lock", "push", "drop", "lock", "push", "drop"]
        );
    }
}
