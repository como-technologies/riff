//! The real-session test of a release (01M4Z60B8Q8YH6DRSZWPV42CQ0,
//! decision 27 of #699). `just real-session` runs it.
//!
//! # Design
//!
//! The hermetic tests give riff a fake home. The faults of riff 2.0 came
//! from a real home (#693, #678), and those tests did not see them. So
//! before each tag, a person runs this test on a host with podman: the
//! `riff` of the tree does one scratch item with `riff run`, the riff
//! daemon of the tree, the session image that the tree pins, and a real
//! `claude`, from the start to a pull request
//! (01M4ZCFGCY301G2ARFMRXV7XDR). An admin runs it on the
//! merge commit of the release pull request, before the tag
//! (01M4ZCFGKDKH16KGKR5NBEK3Y4).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant T as real-session
//!     participant G as GitHub
//!     participant S as riff-server
//!     participant D as riff daemon (the tree)
//!     participant R as riff run (the tree)
//!     T->>T: check the host, list the home
//!     T->>G: a scratch issue in the milestone Backlog
//!     T->>G: the label no-verify on the issue
//!     T->>S: hold verify-issue-N
//!     T->>D: riff daemon with RIFF_DATA, in a process group of its own
//!     T->>R: riff run issue-N --image IMAGE, with RIFF_DATA
//!     R->>D: run the session in a container
//!     R->>G: the session pushes, and opens the pull request
//!     T->>T: list the home again: no new path, no changed file
//!     T->>G: the pull request adds the line, and its Gate starts
//!     T->>D: SIGTERM
//!     T->>G: close the pull request and the issue, delete the branch, cancel its runs
//!     T->>S: free verify-issue-N
//! ```
//!
//! The steps, in order. The first step that fails stops the test
//! (01M4ZCFGF6QN6KZEVMBNW9G4EW):
//!
//! | Step | Passes when |
//! |---|---|
//! | 1. check the host | podman runs rootless, `gh` has a sign-in, and `riff image` names the image of the tree |
//! | 2. list the home | always |
//! | 3. make the scratch item | `gh issue create` prints the URL of an issue |
//! | 4. keep its verify from the workers | the issue has the label [`NO_VERIFY`]; a failed hold is a warning |
//! | 5. start the daemon | no daemon of the test answers before, and `riff daemon` of the tree answers in [`DAEMON_WAIT`] (see below) |
//! | 6. run the item | `riff run issue-N --image IMAGE` ends with 0 |
//! | 7. check the home | the home has no new path and no changed file that riff must not write (see below) |
//! | 8. find the pull request | the branch `worktree-issue-N` has an open pull request |
//! | 9. check the change | its diff adds the line of the item to [`BOOK_FILE`] |
//! | 10. wait for the Gate | the head commit has a check run [`GATE`] in [`GATE_WAIT`] |
//!
//! Then the test cleans up, also after a failed step, and also after
//! Ctrl-C or SIGTERM ([`watch_signals`]). A child of a step that ends
//! by SIGINT or SIGTERM counts as a signal too ([`child_signal`]): Ctrl-C
//! reaches the child as well, and the child can end first. It stops its daemon, closes the
//! pull request and deletes its branch, cancels each CI run of the
//! branch that did not end, closes the issue as not planned, frees the
//! hold, and removes the folder of the session. A pass gives the exit code 0, a failed step 1,
//! and a signal 130. A second signal ends the test at once, with no more
//! clean-up.
//!
//! # The scratch item
//!
//! The issue is in the milestone [`MILESTONE`], so the rollout of the
//! lead gives it to no worker. It adds one line to the book
//! ([`scratch_line`], [`issue_body`]). Step 4 keeps its verify from the
//! workers (01M5157V34H7FWFH6S2DRQXRMP), also when a worker runs the
//! test:
//!
//! - The issue gets the label [`NO_VERIFY`]. riff offers no verify of
//!   its pull request, and `riff run` sends no verify request for it
//!   (01M5157V0P92EBHF4M7N9K0RGR). The step fails when `gh` cannot put
//!   the label on.
//! - Then the test holds `verify-issue-N`, so a claim of the verify by a
//!   worker fails. Only a lead, the owner or an admin can hold an item,
//!   not a worker. When the hold fails, the test goes on with a
//!   warning: the label keeps the verify.
//!
//! ```mermaid
//! flowchart LR
//!     L[the label no-verify] --> O[the rollout and the wave offer no verify]
//!     L --> R[riff run sends no verify request]
//!     H[the hold of verify-issue-N] --> C[a claim of a worker fails]
//! ```
//!
//! # The daemon
//!
//! `riff run` asks the riff daemon to run the session (#716). The
//! daemon of the machine, for example the unit `riff.service` of an
//! installed riff, runs on with its peer links: the test never stops,
//! starts or changes it, and does not talk to it
//! (01M53YP9DATAD95EZEZ6R75WEF). The test runs a daemon of its own
//! beside it. Each `riff` that the test runs ([`Riff`]) gets `RIFF_DATA`
//! ([`DATA_VAR`]): the data folder of the test ([`Env::test_data`]), in
//! the folders of riff. So the daemon of the test has its own socket, its
//! own session folders, and its own key and riff
//! (01M53YP97S9BDMFBN0X5G444MY, 01M51JW1XRZWC3Q7G32V0X0D89). It keeps
//! them in that folder for the next run.
//!
//! ```mermaid
//! flowchart LR
//!     T["real-session"] -- "RIFF_DATA" --> R["riff run"]
//!     T -- "RIFF_DATA" --> D["the daemon of the test<br/>DATA/daemon/riff.sock"]
//!     R --> D
//!     M["the daemon of the machine<br/>riff/daemon/riff.sock"]
//! ```
//!
//! The test starts `riff daemon` of the tree, and waits until `riff
//! daemon status` of the tree answers (01M4ZFPYNGFF49WWHR8EBWSTNG). When
//! a daemon of the test answers before the start, the step fails: another
//! real-session test runs, or a test that a second signal stopped left
//! its daemon.
//!
//! The daemon runs in a process group of its own. So Ctrl-C reaches the
//! test and `riff run`, not the daemon: `riff run` then asks the daemon
//! to stop the container. The clean-up stops the daemon with SIGTERM,
//! and kills it when it does not end in [`DAEMON_WAIT`]. After a second
//! signal, the daemon runs on: stop it with its PID, from the output of
//! step 5.
//!
//! # The home check
//!
//! riff writes nothing in the home of the person outside its own folders
//! (01M4Z60B85X62PNYSVJWKXEWH3). [`Home`] lists each path of the home
//! before `riff run` and after it, on the file system of the home, with
//! no link followed. A new path fails the test. A new folder shows as one
//! path. The check leaves out these folders (01M4ZCFGHCS7WJ6CFYBZSX0AT5,
//! [`Env::skips`]):
//!
//! | Folder | Why |
//! |---|---|
//! | `RIFF_HOME`, and `riff` in each XDG base folder | the folders of riff, with the data folder of the test |
//! | `containers` in `XDG_DATA_HOME` and in `XDG_CONFIG_HOME` | the images and the volumes of podman |
//! | the main clone, and the worktree of the test ([`Home::clones`]), except their `.git` | the build of the test, and the fetch of `riff run` |
//! | `XDG_CACHE_HOME` | the caches of the tools of the host |
//! | `snap` in the home | the snap apps, for example the Firefox snap (01M51MQB46AYY39YSAY18NMD8T) |
//! | `claude/locks` in `XDG_STATE_HOME` | the locks of each `claude` on the host (01M51MQB46AYY39YSAY18NMD8T) |
//! | `.claude/worktrees` and `.git` in each git clone, also in the home when it is a clone, except the hooks and the config files of git ([`GIT_KEPT`]) | the worktrees and the builds of the other sessions, and the git of each session (01M51SPS50QQ9Y5C6VE26A18FZ, 01M51X6JCHZB8HJ8C8M3J1G6GZ, [`of_other_sessions`]) |
//! | each path of `--skip` | the other programs of the person that write while the test runs |
//!
//! So the test also passes on a busy machine, for example while workers
//! build and commit in the worktrees of another clone of the person.
//! A new path in the rest of that clone still counts, for example a new
//! hook in `.git/hooks`.
//!
//! The check also sees a change to a file that is there already, in the
//! files that riff must not write (01M51K051FR0WN71NJXNA86XWY,
//! [`Env::guarded`]). It keeps the size, the time of change and the
//! inode of each of these files ([`Listing`]). A file with a new value
//! fails the test. The other programs of the person change their own
//! files while the test runs, so a change of another file does not count.
//!
//! | Files | Why |
//! |---|---|
//! | `~/.gitconfig`, `git` in `XDG_CONFIG_HOME` | the git config |
//! | `gh` in `XDG_CONFIG_HOME` | the config of `gh`: the session gets it by a bind |
//! | [`SHELL_FILES`] in the home, `fish/config.fish` and `fish/conf.d` in `XDG_CONFIG_HOME` | the shell files |
//! | `.claude.json`, and `settings.json` in the Claude folder | the config of Claude Code |
//! | each file in `.git/hooks` of a clone | the hooks of git (01M51X6JA3W2CMV1S5VZEY7FC3) |
//!
//! A line of a git config can run code at the next git command, for
//! example `core.hooksPath`, `core.fsmonitor` or an alias. So the check
//! reads `config` and `config.worktree` in the `.git` of each clone of
//! the home, also of the clones of the test, before `riff run` and after
//! it, as keys ([`git_config`], 01M51X6JA3W2CMV1S5VZEY7FC3). A new, a
//! changed or a removed key fails the test, and the message names the
//! file and the key. A key of a section `[branch "NAME"]` does not
//! count: each `git push -u` of a session writes one. `config.worktree`
//! in `.git/worktrees/NAME`, and the git folder of each submodule in
//! `.git/modules`, stay left out.
//!
//! The other sessions of the person write in the Claude folder of the
//! person (`CLAUDE_CONFIG_DIR`, else `~/.claude`) and in `.claude.json`
//! and its lock beside it, while the test runs
//! (01M51K053WS0KK3E376V6DH6GR). So there, only a new path that names
//! the session counts: its `ID8`, from the name of its folder
//! `issue-N-ID8` ([`session_folders`]). A changed `.claude.json` counts
//! only when its text names the `ID8`.
//!
//! ```mermaid
//! flowchart TD
//!     P[a path after riff run] --> S{in a skip, or of the other sessions?}
//!     S -- yes --> N[does not count]
//!     S -- no --> B{in the listing before?}
//!     B -- no --> C{in the Claude folder or .claude.json*?}
//!     C -- no --> F[fails the test]
//!     C -- yes --> M{names the ID8?}
//!     M -- yes --> F
//!     M -- no --> N
//!     B -- yes --> G{a file that riff must not write, with a new value, or a git config with a new key?}
//!     G -- no --> N
//!     G -- yes --> J{.claude.json?}
//!     J -- no --> F
//!     J -- yes --> T{its text names the ID8?}
//!     T -- yes --> F
//!     T -- no --> N
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::Metadata;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

/// The milestone of the scratch item: no wave, so the rollout gives it
/// to no worker.
pub const MILESTONE: &str = "Backlog";

/// The page of the book that the scratch item changes.
pub const BOOK_FILE: &str = "docs/src/riff-3.md";

/// The name of the check run of the full check of a commit.
pub const GATE: &str = "Gate";

/// The longest wait for the Gate, when `--gate-wait` does not give one.
/// The job Gate starts when each part of the Gate ended
/// (01M513W2K3HNRWPQTQNBQPT8MZ), so the wait holds the time of the
/// parts.
pub const GATE_WAIT: Duration = Duration::from_secs(1800);

/// The time between two looks at the Gate.
pub const POLL: Duration = Duration::from_secs(10);

/// The longest wait for the daemon of the test to answer, and to end
/// after SIGTERM.
pub const DAEMON_WAIT: Duration = Duration::from_secs(10);

/// The most paths that a failed home check shows.
pub const SHOWN: usize = 20;

/// The shell files in the home that the home check guards
/// ([`Env::guarded`]).
pub const SHELL_FILES: [&str; 10] = [
    ".profile",
    ".bashrc",
    ".bash_profile",
    ".bash_login",
    ".bash_logout",
    ".zshenv",
    ".zprofile",
    ".zshrc",
    ".zlogin",
    ".zlogout",
];

/// The reason of the hold of `verify-issue-N`.
pub const HOLD_REASON: &str = "the real-session test makes and closes this item: no verify";

/// The label of the scratch issue: riff offers no verify of an issue
/// with it (01M5157V0P92EBHF4M7N9K0RGR).
///
/// ```
/// assert_eq!(real_session::NO_VERIFY, riff::rollout::NO_VERIFY);
/// ```
pub const NO_VERIFY: &str = "no-verify";

/// The description of the label [`NO_VERIFY`] on GitHub.
pub const NO_VERIFY_TEXT: &str = "riff offers no verify of this item";

/// The color of the label [`NO_VERIFY`] on GitHub.
pub const NO_VERIFY_COLOR: &str = "ededed";

/// The variable that gives each `riff` of the test the data folder of
/// the test ([`Env::test_data`], 01M53YP9DATAD95EZEZ6R75WEF).
///
/// ```
/// assert_eq!(real_session::DATA_VAR, riff::run::DATA_VAR);
/// ```
pub const DATA_VAR: &str = "RIFF_DATA";

/// The name of the data folder of the test, in the data folder of riff.
pub const TEST_DATA: &str = "real-session";

/// The comment of the test when it closes the pull request and the issue.
pub const CLOSE_TEXT: &str = "The real-session test closes its scratch item.";

/// The usage line of the command.
pub const USAGE: &str = "usage: real-session --riff PATH [--skip PATH]... [--gate-wait SECONDS]";

/// One step of the test, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Host,
    List,
    Item,
    Keep,
    Daemon,
    Run,
    Home,
    Pull,
    Change,
    Gate,
}

impl Step {
    /// Each step, in order.
    pub const ALL: [Step; 10] = [
        Step::Host,
        Step::List,
        Step::Item,
        Step::Keep,
        Step::Daemon,
        Step::Run,
        Step::Home,
        Step::Pull,
        Step::Change,
        Step::Gate,
    ];

    /// The name of the step, as the output shows it.
    pub fn name(self) -> &'static str {
        match self {
            Step::Host => "check the host",
            Step::List => "list the home",
            Step::Item => "make the scratch item",
            Step::Keep => "keep its verify from the workers",
            Step::Daemon => "start the daemon",
            Step::Run => "run the item",
            Step::Home => "check the home",
            Step::Pull => "find the pull request",
            Step::Change => "check the change",
            Step::Gate => "wait for the Gate",
        }
    }

    /// The number of the step: 1 for the first.
    ///
    /// ```
    /// use real_session::Step;
    ///
    /// assert_eq!(Step::Host.number(), 1);
    /// assert_eq!(Step::Gate.number(), Step::ALL.len());
    /// ```
    pub fn number(self) -> usize {
        Step::ALL
            .iter()
            .position(|s| *s == self)
            .map_or(0, |i| i + 1)
    }
}

/// A step that failed, and why (01M4ZCFGF6QN6KZEVMBNW9G4EW).
///
/// ```
/// use real_session::{Failure, Step};
///
/// let failure = Failure {
///     step: Step::Run,
///     reason: "riff run issue-12 ended with exit status: 1".into(),
/// };
/// assert_eq!(
///     failure.to_string(),
///     "step 6 of 10, run the item: riff run issue-12 ended with exit status: 1"
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The step that failed.
    pub step: Step,
    /// Why it failed.
    pub reason: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "step {} of {}, {}: {}",
            self.step.number(),
            Step::ALL.len(),
            self.step.name(),
            self.reason
        )
    }
}

/// The options of the command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The riff binary of the tree.
    pub riff: PathBuf,
    /// The paths that the home check also leaves out.
    pub skips: Vec<PathBuf>,
    /// The longest wait for the Gate.
    pub gate_wait: Duration,
}

impl Options {
    /// The options from the arguments of the command.
    ///
    /// ```
    /// use std::path::{Path, PathBuf};
    /// use std::time::Duration;
    /// use real_session::{GATE_WAIT, Options};
    ///
    /// let args = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
    /// let options = Options::parse(&args("--riff target/debug/riff --skip /h/.mozilla --gate-wait 60"))?;
    /// assert_eq!(options.riff, Path::new("target/debug/riff"));
    /// assert_eq!(options.skips, [PathBuf::from("/h/.mozilla")]);
    /// assert_eq!(options.gate_wait, Duration::from_secs(60));
    /// assert_eq!(Options::parse(&args("--riff r"))?.gate_wait, GATE_WAIT);
    ///
    /// assert!(Options::parse(&args("--skip /h")).is_err());
    /// assert!(Options::parse(&args("--riff r --gate-wait soon")).is_err());
    /// assert!(Options::parse(&args("--riff r --skip")).is_err());
    /// assert!(Options::parse(&args("--riff r --other")).is_err());
    /// # Ok::<(), String>(())
    /// ```
    pub fn parse(args: &[String]) -> Result<Options, String> {
        let mut riff = None;
        let mut skips = Vec::new();
        let mut gate_wait = GATE_WAIT;
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
            match arg.as_str() {
                "--riff" => riff = Some(PathBuf::from(value()?)),
                "--skip" => skips.push(PathBuf::from(value()?)),
                "--gate-wait" => {
                    let text = value()?;
                    let seconds = text
                        .parse()
                        .map_err(|_| format!("--gate-wait {text}: not a number of seconds"))?;
                    gate_wait = Duration::from_secs(seconds);
                }
                other => return Err(format!("unknown argument {other}")),
            }
        }
        let riff = riff.ok_or("give the riff binary of the tree with --riff PATH")?;
        Ok(Options {
            riff,
            skips,
            gate_wait,
        })
    }
}

/// The folders of the home that the test needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Env {
    /// `HOME`.
    pub home: PathBuf,
    /// `RIFF_HOME`, when it is set.
    pub riff_home: Option<PathBuf>,
    /// `XDG_CONFIG_HOME`, else `~/.config`.
    pub config: PathBuf,
    /// `XDG_DATA_HOME`, else `~/.local/share`.
    pub data: PathBuf,
    /// `XDG_STATE_HOME`, else `~/.local/state`.
    pub state: PathBuf,
    /// `XDG_CACHE_HOME`, else `~/.cache`.
    pub cache: PathBuf,
    /// The Claude folder of the person: `CLAUDE_CONFIG_DIR`, else
    /// `~/.claude`.
    pub claude: PathBuf,
    /// The config file of Claude Code: `.claude.json` in
    /// `CLAUDE_CONFIG_DIR`, else `~/.claude.json`.
    pub claude_json: PathBuf,
}

impl Env {
    /// The folders from `var`, which reads a variable. An empty value
    /// counts as unset.
    ///
    /// ```
    /// use std::path::Path;
    /// use real_session::Env;
    ///
    /// let env = Env::from_vars(|name| match name {
    ///     "HOME" => Some("/h".into()),
    ///     "XDG_CACHE_HOME" => Some("/c".into()),
    ///     "XDG_DATA_HOME" => Some("".into()),
    ///     _ => None,
    /// })?;
    /// assert_eq!(env.data, Path::new("/h/.local/share"));
    /// assert_eq!(env.cache, Path::new("/c"));
    /// assert_eq!(env.claude, Path::new("/h/.claude"));
    /// assert_eq!(env.claude_json, Path::new("/h/.claude.json"));
    /// assert!(Env::from_vars(|_| None).is_err());
    ///
    /// let env = Env::from_vars(|name| match name {
    ///     "HOME" => Some("/h".into()),
    ///     "CLAUDE_CONFIG_DIR" => Some("/h/cc".into()),
    ///     _ => None,
    /// })?;
    /// assert_eq!(env.claude_json, Path::new("/h/cc/.claude.json"));
    /// # Ok::<(), String>(())
    /// ```
    pub fn from_vars(var: impl Fn(&str) -> Option<OsString>) -> Result<Env, String> {
        let set = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = set("HOME").ok_or("set HOME")?;
        let base = |name: &str, default: &str| set(name).unwrap_or_else(|| home.join(default));
        let claude_json = match set("CLAUDE_CONFIG_DIR") {
            Some(dir) => dir.join(".claude.json"),
            None => home.join(".claude.json"),
        };
        Ok(Env {
            riff_home: set("RIFF_HOME"),
            config: base("XDG_CONFIG_HOME", ".config"),
            data: base("XDG_DATA_HOME", ".local/share"),
            state: base("XDG_STATE_HOME", ".local/state"),
            cache: base("XDG_CACHE_HOME", ".cache"),
            claude: base("CLAUDE_CONFIG_DIR", ".claude"),
            claude_json,
            home,
        })
    }

    /// [`Env::from_vars`] with the variables of this process.
    pub fn of_process() -> Result<Env, String> {
        Env::from_vars(|name| std::env::var_os(name))
    }

    /// The data folder of the test (01M53YP9DATAD95EZEZ6R75WEF):
    /// [`TEST_DATA`] in `RIFF_HOME`, else in `riff` of [`Env::data`]. Each
    /// `riff` of the test gets it as `RIFF_DATA` ([`Riff`]): the daemon of
    /// the test and the session folders of its `riff run` are there. It is
    /// in the folders of riff, so the home check leaves it out
    /// ([`Env::skips`]).
    ///
    /// ```
    /// use std::path::Path;
    /// use real_session::Env;
    ///
    /// let env = Env::from_vars(|name| (name == "HOME").then(|| "/h".into()))?;
    /// assert_eq!(env.test_data(), Path::new("/h/.local/share/riff/real-session"));
    /// assert!(env.skips(&[]).iter().any(|skip| env.test_data().starts_with(skip)));
    /// let env = Env { riff_home: Some("/r".into()), ..env };
    /// assert_eq!(env.test_data(), Path::new("/r/real-session"));
    /// # Ok::<(), String>(())
    /// ```
    pub fn test_data(&self) -> PathBuf {
        match &self.riff_home {
            Some(dir) => dir.join(TEST_DATA),
            None => self.data.join("riff").join(TEST_DATA),
        }
    }

    /// The folder of the session folders of the `riff run` of the test:
    /// `sessions` in [`Env::test_data`], as `riff::run::sessions_dir`
    /// gives it with `RIFF_DATA`.
    ///
    /// ```
    /// use std::path::Path;
    /// use real_session::Env;
    ///
    /// let env = Env::from_vars(|name| (name == "HOME").then(|| "/h".into()))?;
    /// assert_eq!(env.sessions(), Path::new("/h/.local/share/riff/real-session/sessions"));
    /// let env = Env { riff_home: Some("/r".into()), ..env };
    /// assert_eq!(env.sessions(), Path::new("/r/real-session/sessions"));
    /// # Ok::<(), String>(())
    /// ```
    pub fn sessions(&self) -> PathBuf {
        self.test_data().join("sessions")
    }

    /// The folders that the home check leaves out: the folders of riff,
    /// the storage of podman, the cache, the snap apps, the locks of
    /// `claude`, and `skips` (01M4ZCFGHCS7WJ6CFYBZSX0AT5,
    /// 01M51MQB46AYY39YSAY18NMD8T). The clones of the test are in
    /// [`Home::clones`].
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use real_session::Env;
    ///
    /// let env = Env::from_vars(|name| (name == "HOME").then(|| "/h".into()))?;
    /// let skips = env.skips(&["/h/.mozilla".into()]);
    /// let skips: Vec<String> = skips.iter().map(|p| p.display().to_string()).collect();
    /// assert_eq!(
    ///     skips,
    ///     [
    ///         "/h/.config/riff",
    ///         "/h/.local/share/riff",
    ///         "/h/.local/state/riff",
    ///         "/h/.cache/riff",
    ///         "/h/.local/share/containers",
    ///         "/h/.config/containers",
    ///         "/h/.cache",
    ///         "/h/snap",
    ///         "/h/.local/state/claude/locks",
    ///         "/h/.mozilla",
    ///     ]
    /// );
    /// # Ok::<(), String>(())
    /// ```
    pub fn skips(&self, skips: &[PathBuf]) -> Vec<PathBuf> {
        let mut all: Vec<PathBuf> = [&self.config, &self.data, &self.state, &self.cache]
            .iter()
            .map(|base| base.join("riff"))
            .collect();
        all.extend(self.riff_home.clone());
        all.push(self.data.join("containers"));
        all.push(self.config.join("containers"));
        all.push(self.cache.clone());
        all.push(self.home.join("snap"));
        all.push(self.state.join("claude/locks"));
        all.extend(skips.iter().cloned());
        all
    }

    /// The files that riff must not write (01M4Z60B85X62PNYSVJWKXEWH3):
    /// the home check sees a change to them (01M51K051FR0WN71NJXNA86XWY).
    /// A folder guards each file in it.
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use real_session::{Env, SHELL_FILES};
    ///
    /// let env = Env::from_vars(|name| (name == "HOME").then(|| "/h".into()))?;
    /// let guarded = env.guarded();
    /// for path in ["/h/.gitconfig", "/h/.config/git", "/h/.config/gh", "/h/.bashrc", "/h/.zshrc"] {
    ///     assert!(guarded.contains(&PathBuf::from(path)), "{path}");
    /// }
    /// assert!(guarded.contains(&PathBuf::from("/h/.config/fish/config.fish")));
    /// assert!(guarded.contains(&PathBuf::from("/h/.claude.json")));
    /// assert!(guarded.contains(&PathBuf::from("/h/.claude/settings.json")));
    /// assert_eq!(guarded.len(), SHELL_FILES.len() + 7);
    /// # Ok::<(), String>(())
    /// ```
    pub fn guarded(&self) -> Vec<PathBuf> {
        let mut all = vec![
            self.home.join(".gitconfig"),
            self.config.join("git"),
            self.config.join("gh"),
        ];
        all.extend(SHELL_FILES.iter().map(|name| self.home.join(name)));
        all.push(self.config.join("fish/config.fish"));
        all.push(self.config.join("fish/conf.d"));
        all.push(self.claude_json.clone());
        all.push(self.claude.join("settings.json"));
        all
    }
}

/// The paths of a home, each one by its hash: a home can hold millions
/// of paths. Each file that the check guards also has its state
/// ([`Home::guarded`]), and each git config its keys ([`git_config`]).
#[derive(Debug, Default)]
pub struct Listing {
    paths: HashSet<u64>,
    states: HashMap<u64, u64>,
    configs: HashMap<u64, GitConfig>,
}

impl Listing {
    /// The number of paths.
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// True when the listing has no path.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// True when the listing has `path`.
    pub fn contains(&self, path: &Path) -> bool {
        self.paths.contains(&key(path))
    }

    /// The state of `path`, when the check guards it.
    fn state(&self, path: &Path) -> Option<u64> {
        self.states.get(&key(path)).copied()
    }

    /// The keys of `path`, when it is a git config.
    fn config(&self, path: &Path) -> Option<&GitConfig> {
        self.configs.get(&key(path))
    }
}

/// The hash of a path in a [`Listing`].
fn key(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

/// The state of a file in a [`Listing`]: the hash of its size, its time
/// of change and its inode. A write changes the size or the time. A new
/// file in place of the old one changes the inode.
fn state(meta: &Metadata) -> u64 {
    let mut hasher = DefaultHasher::new();
    (meta.len(), meta.mtime(), meta.mtime_nsec(), meta.ino()).hash(&mut hasher);
    hasher.finish()
}

/// What changed in the home and counts: the result of the home check.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Changes {
    /// The files that riff must not write, with a new size, time of
    /// change or inode, and the git configs with a new, a changed or a
    /// removed key.
    pub changed: Vec<PathBuf>,
    /// The keys that changed in each git config of `changed`.
    pub keys: BTreeMap<PathBuf, Vec<String>>,
    /// The new paths. A new folder shows as one path.
    pub new: Vec<PathBuf>,
}

impl Changes {
    /// True when nothing counts.
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.new.is_empty()
    }
}

/// The home check (01M4ZCFGHCS7WJ6CFYBZSX0AT5,
/// 01M51K051FR0WN71NJXNA86XWY, 01M51K053WS0KK3E376V6DH6GR).
///
/// ```
/// use real_session::{Changes, Home};
///
/// let dir = tempfile::tempdir()?;
/// let root = dir.path().to_owned();
/// std::fs::create_dir_all(root.join(".claude/projects"))?;
/// std::fs::create_dir_all(root.join(".local/share/riff"))?;
/// std::fs::write(root.join(".gitconfig"), "[user]\n")?;
/// std::fs::write(root.join(".bash_history"), "ls\n")?;
/// let home = Home {
///     root: root.clone(),
///     skips: vec![root.join(".local/share/riff")],
///     claude: root.join(".claude"),
///     claude_json: root.join(".claude.json"),
///     guarded: vec![root.join(".gitconfig")],
///     clones: Vec::new(),
/// };
/// let before = home.list();
/// assert_eq!(before.len(), 6);
///
/// // A new file, a new folder with a file in it, and a change to a file
/// // that riff must not write: they count.
/// std::fs::write(root.join(".cvsignore"), "")?;
/// std::fs::create_dir_all(root.join("new/deep"))?;
/// std::fs::write(root.join(".gitconfig"), "[user]\n\tname = x\n")?;
/// // A file of riff, a file of another Claude session, the lock of
/// // .claude.json, and a change to another file: they do not.
/// std::fs::write(root.join(".local/share/riff/state"), "")?;
/// std::fs::write(root.join(".claude/projects/other.jsonl"), "")?;
/// std::fs::write(root.join(".claude.json.lock"), "")?;
/// std::fs::write(root.join(".bash_history"), "ls\npwd\n")?;
/// // A folder in the Claude folder that names the session counts.
/// std::fs::create_dir_all(root.join(".claude/projects/-s-issue-12-1a2b3c4d-clone"))?;
///
/// assert_eq!(
///     home.changes(&before, &["1a2b3c4d".into()]),
///     Changes {
///         changed: vec![root.join(".gitconfig")],
///         new: vec![
///             root.join(".claude/projects/-s-issue-12-1a2b3c4d-clone"),
///             root.join(".cvsignore"),
///             root.join("new"),
///         ],
///         ..Changes::default()
///     }
/// );
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    /// The home.
    pub root: PathBuf,
    /// The paths that the check leaves out.
    pub skips: Vec<PathBuf>,
    /// The Claude folder of the person.
    pub claude: PathBuf,
    /// The config file of Claude Code: `.claude.json`.
    pub claude_json: PathBuf,
    /// The files that riff must not write: a change to one of them
    /// counts. A folder guards each file in it.
    pub guarded: Vec<PathBuf>,
    /// The main clone and the worktree of the test: the check leaves
    /// them out, except their `.git`. There, as in each other clone, the
    /// hooks and the config files of git count (01M51X6JA3W2CMV1S5VZEY7FC3).
    pub clones: Vec<PathBuf>,
}

impl Home {
    /// Each path of the home, except the paths that the check leaves out
    /// ([`Home::skips`], [`Home::clones`] and [`of_other_sessions`]),
    /// the state of each file that it guards
    /// ([`Home::guarded`], and the hooks of git), and the keys of each
    /// git config ([`git_config`]).
    pub fn list(&self) -> Listing {
        let mut listing = Listing::default();
        self.walk(&mut |path, meta| {
            let key = key(path);
            listing.paths.insert(key);
            if self.guards(path, meta) {
                listing.states.insert(key, state(meta));
            }
            if is_git_config(path, meta)
                && let Some(config) = read_git_config(path)
            {
                listing.configs.insert(key, config);
            }
            true
        });
        listing
    }

    /// What changed in the home since `before`, and counts. Each path
    /// that is not in `before` counts, except in the Claude folder and
    /// `.claude.json*`: there only a path that names one of `marks`
    /// counts. A new folder shows as one path. Each file of
    /// [`Home::guarded`] and each hook of git with a new state counts,
    /// except a `.claude.json` whose text names none of `marks`. Each
    /// git config with a new, a changed or a removed key counts, except
    /// a key of a section `[branch "NAME"]`, with its keys in
    /// [`Changes::keys`]. The paths are in order.
    pub fn changes(&self, before: &Listing, marks: &[String]) -> Changes {
        let names = |text: &str| {
            marks
                .iter()
                .any(|mark| !mark.is_empty() && text.contains(mark.as_str()))
        };
        let mut changes = Changes::default();
        self.walk(&mut |path, meta| {
            if !before.contains(path) {
                let counts = !self.of_claude(path) || names(&path.to_string_lossy());
                if counts {
                    changes.new.push(path.to_owned());
                }
                // A new folder of another session can hold a path that
                // names this session.
                return !counts;
            }
            if let Some(old) = before.config(path) {
                let keys = changed_keys(old, &read_git_config(path).unwrap_or_default());
                if !keys.is_empty() {
                    changes.changed.push(path.to_owned());
                    changes.keys.insert(path.to_owned(), keys);
                }
                return true;
            }
            let changed = before
                .state(path)
                .is_some_and(|old| self.guards(path, meta) && old != state(meta));
            if changed
                && (path != self.claude_json
                    || std::fs::read(path).is_ok_and(|text| names(&String::from_utf8_lossy(&text))))
            {
                changes.changed.push(path.to_owned());
            }
            true
        });
        changes.changed.sort();
        changes.new.sort();
        changes
    }

    /// True when the check guards `path`: a file or a link in
    /// [`Home::guarded`], or in the folder `hooks` of a git folder `.git`
    /// (01M51X6JA3W2CMV1S5VZEY7FC3).
    fn guards(&self, path: &Path, meta: &Metadata) -> bool {
        !meta.is_dir()
            && (self.guarded.iter().any(|guard| path.starts_with(guard))
                || path.ancestors().skip(1).any(|hooks| {
                    named(hooks, "hooks") && hooks.parent().is_some_and(|git| named(git, ".git"))
                }))
    }

    /// True when the check leaves out `path`: it is in [`Home::skips`],
    /// it is of the other sessions ([`of_other_sessions`]), or it is in
    /// one of [`Home::clones`] and not in its `.git`.
    fn left_out(&self, path: &Path) -> bool {
        self.skips.iter().any(|skip| path.starts_with(skip))
            || self.clones.iter().any(|clone| {
                path.strip_prefix(clone).is_ok_and(|rest| {
                    rest.components()
                        .next()
                        .is_some_and(|first| first.as_os_str() != ".git")
                })
            })
            || of_other_sessions(path)
    }

    /// True when `path` is in the Claude folder, or its name starts with
    /// `.claude.json` beside it, for example its lock.
    fn of_claude(&self, path: &Path) -> bool {
        let json = self.claude_json.as_os_str().as_encoded_bytes();
        path.starts_with(&self.claude) || path.as_os_str().as_encoded_bytes().starts_with(json)
    }

    /// Calls `visit` for each path of the home, except the paths that
    /// the check leaves out ([`Home::left_out`]), with the metadata of
    /// the path itself. It
    /// goes into a folder when `visit` gives true, and the folder is on
    /// the file system of the home. It follows no link, and it skips a
    /// folder that it cannot read.
    fn walk(&self, visit: &mut dyn FnMut(&Path, &Metadata) -> bool) {
        let device = std::fs::symlink_metadata(&self.root).map(|m| m.dev()).ok();
        let mut folders = vec![self.root.clone()];
        while let Some(folder) = folders.pop() {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if self.left_out(&path) {
                    continue;
                }
                // The metadata of a link, not of its target.
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                if visit(&path, &meta) && meta.is_dir() && Some(meta.dev()) == device {
                    folders.push(path);
                }
            }
        }
    }
}

/// The paths in a git folder `.git` that the home check still lists
/// (01M51SPS50QQ9Y5C6VE26A18FZ): a new hook or a new config file runs
/// code at the next git command. The check also sees a change to a
/// hook, and a change to the keys of a config file
/// (01M51X6JA3W2CMV1S5VZEY7FC3).
pub const GIT_KEPT: [&str; 3] = ["hooks", "config", "config.worktree"];

/// The config files in a git folder `.git` that the home check reads as
/// keys ([`git_config`], 01M51X6JA3W2CMV1S5VZEY7FC3).
pub const GIT_CONFIGS: [&str; 2] = ["config", "config.worktree"];

/// The keys of a git config, each with its values in order
/// ([`git_config`]).
pub type GitConfig = BTreeMap<String, Vec<String>>;

/// True when the other sessions of the person write `path` while the
/// test runs, so the home check leaves it out (01M51SPS50QQ9Y5C6VE26A18FZ):
///
/// - each path in a git folder `.git`, except [`GIT_KEPT`]: the git of
///   each session writes its objects, refs and logs, and the folder of
///   each worktree, in the git folder of its clone;
/// - the folder `.claude/worktrees` of a git clone (a folder with
///   `.git` in it): the worktrees of the other sessions, with their
///   builds, and each new worktree. When the home is a clone, this is
///   also `.claude/worktrees` in the home (01M51X6JCHZB8HJ8C8M3J1G6GZ).
///
/// Each other path of a clone still counts, also a new `.git`.
///
/// ```
/// use real_session::of_other_sessions;
///
/// let dir = tempfile::tempdir()?;
/// let clone = dir.path().join("src/riff");
/// std::fs::create_dir_all(clone.join(".git"))?;
/// assert!(of_other_sessions(&clone.join(".git/objects")));
/// assert!(of_other_sessions(&clone.join(".git/worktrees")));
/// assert!(of_other_sessions(&clone.join(".git/modules")));
/// assert!(of_other_sessions(&clone.join(".claude/worktrees")));
/// // The hooks and the config files of git still count.
/// assert!(!of_other_sessions(&clone.join(".git/hooks")));
/// assert!(!of_other_sessions(&clone.join(".git/config")));
/// assert!(!of_other_sessions(&clone.join(".git/config.worktree")));
/// assert!(!of_other_sessions(&clone.join(".git")));
/// assert!(!of_other_sessions(&clone.join(".claude/worktrees/issue-1")));
/// assert!(!of_other_sessions(&clone.join(".claude/settings.json")));
/// assert!(!of_other_sessions(&clone.join("notes.md")));
/// // A folder with no `.git` in it is not a clone.
/// assert!(!of_other_sessions(&dir.path().join("src/.claude/worktrees")));
///
/// // A home that is a clone, for example of the dotfiles of the person:
/// // `.claude/worktrees` in the home holds the worktrees of its sessions.
/// let home = dir.path().join("home");
/// std::fs::create_dir_all(home.join(".git"))?;
/// assert!(of_other_sessions(&home.join(".claude/worktrees")));
/// assert!(!of_other_sessions(&home.join(".claude/settings.json")));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn of_other_sessions(path: &Path) -> bool {
    if path.parent().is_some_and(|git| named(git, ".git")) {
        return !GIT_KEPT.iter().any(|name| named(path, name));
    }
    named(path, "worktrees")
        && path.parent().is_some_and(|claude| {
            named(claude, ".claude")
                && claude
                    .parent()
                    .is_some_and(|clone| std::fs::symlink_metadata(clone.join(".git")).is_ok())
        })
}

/// True when the name of `path` is `name`.
fn named(path: &Path, name: &str) -> bool {
    path.file_name() == Some(OsStr::new(name))
}

/// True when `path` is a file of [`GIT_CONFIGS`] in a git folder `.git`.
fn is_git_config(path: &Path, meta: &Metadata) -> bool {
    !meta.is_dir()
        && path.parent().is_some_and(|git| named(git, ".git"))
        && GIT_CONFIGS.iter().any(|name| named(path, name))
}

/// The keys of the git config `path`, when the check can read it.
fn read_git_config(path: &Path) -> Option<GitConfig> {
    let text = std::fs::read(path).ok()?;
    Some(git_config(&String::from_utf8_lossy(&text)))
}

/// The keys of the text of a git config, each as git names it:
/// `SECTION.KEY`, or `SECTION.SUBSECTION.KEY`, with the section and the
/// key in lower case. Each key has its values in order, with no comment.
/// A key with no `=` has the value `""`. The keys of a section `[branch
/// "NAME"]` are left out: each `git push -u` of a session writes one
/// (01M51X6JA3W2CMV1S5VZEY7FC3). The keys of `[branch]` itself stay.
///
/// ```
/// use real_session::git_config;
///
/// let config = git_config(
///     "[core]\n\
///      \tbare = false\n\
///      \thooksPath = /tmp/hooks ; a comment\n\
///      \t# a comment\n\
///      [remote \"origin\"]\n\
///      \turl = git@github.com:o/r.git\n\
///      [branch \"worktree-issue-1\"]\n\
///      \tremote = origin\n\
///      [branch.old-form]\n\
///      \tremote = origin\n\
///      [branch]\n\
///      \tsort = -committerdate\n\
///      [alias] st = \"status # not a comment\"\n\
///      [include]\n\
///      \tpath = a\n\
///      \tpath = b\n\
///      [User]\n\
///      \tName = Ada \\\n\
///      Lovelace\n\
///      \tnoValue\n",
/// );
/// let keys: Vec<String> = config
///     .iter()
///     .map(|(key, values)| format!("{key}={}", values.join("|")))
///     .collect();
/// assert_eq!(
///     keys,
///     [
///         "alias.st=\"status # not a comment\"",
///         "branch.sort=-committerdate",
///         "core.bare=false",
///         "core.hookspath=/tmp/hooks",
///         "include.path=a|b",
///         "remote.origin.url=git@github.com:o/r.git",
///         "user.name=Ada Lovelace",
///         "user.novalue=",
///     ]
/// );
/// ```
pub fn git_config(text: &str) -> GitConfig {
    let mut config = GitConfig::new();
    let mut section = String::new();
    let mut branch = false;
    let mut lines = text.lines();
    while let Some(first) = lines.next() {
        let mut line = first.to_owned();
        // A line that ends with an odd number of backslashes goes on in
        // the next line.
        while line.chars().rev().take_while(|&c| c == '\\').count() % 2 == 1 {
            line.pop();
            match lines.next() {
                Some(next) => line.push_str(next),
                None => break,
            }
        }
        let mut rest = line.trim_start();
        if let Some(header) = rest.strip_prefix('[') {
            match section_of(header) {
                Some((name, of_branch, after)) => {
                    (section, branch) = (name, of_branch);
                    rest = after;
                }
                // Git does not read a broken header. Its keys still count
                // under the text of the header.
                None => {
                    (section, branch) = (header.to_lowercase(), false);
                    rest = "";
                }
            }
        }
        let rest = without_comment(rest).trim();
        if rest.is_empty() || branch {
            continue;
        }
        let (key, value) = rest
            .split_once('=')
            .map_or((rest, ""), |(key, value)| (key.trim(), value.trim()));
        config
            .entry(format!("{section}.{}", key.to_lowercase()))
            .or_default()
            .push(value.to_owned());
    }
    config
}

/// The section of a header line of a git config, from the text after
/// its `[`: the name of the section with its subsection, true for a
/// section `[branch "NAME"]` or `[branch.NAME]`, and the text after the
/// `]`. `None` for a broken header.
fn section_of(header: &str) -> Option<(String, bool, &str)> {
    let end = header.find(|c: char| c == ']' || c == '"' || c.is_whitespace())?;
    let name = header[..end].to_lowercase();
    let rest = header[end..].trim_start();
    let Some(quoted) = rest.strip_prefix('"') else {
        // The old form `[section.subsection]`.
        let branch = name.starts_with("branch.");
        return Some((name, branch, rest.strip_prefix(']')?));
    };
    let mut sub = String::new();
    let mut chars = quoted.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => sub.extend(chars.next().map(|(_, c)| c)),
            '"' => {
                let after = quoted[i + 1..].strip_prefix(']')?;
                return Some((format!("{name}.{sub}"), name == "branch", after));
            }
            c => sub.push(c),
        }
    }
    None
}

/// The text of a line of a git config up to its comment: a `#` or a `;`
/// that is not in quotes.
fn without_comment(text: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            '#' | ';' if !quoted => return &text[..i],
            _ => {}
        }
    }
    text
}

/// The keys that are new, changed or removed in `after`, in order.
///
/// ```
/// use real_session::{changed_keys, git_config};
///
/// let before = git_config("[core]\n\tbare = false\n\teditor = vi\n");
/// let after = git_config(
///     "[core]\n\tbare = true\n\thooksPath = /x\n[branch \"x\"]\n\tremote = origin\n",
/// );
/// assert_eq!(changed_keys(&before, &after), ["core.bare", "core.editor", "core.hookspath"]);
/// assert!(changed_keys(&before, &before).is_empty());
/// ```
pub fn changed_keys(before: &GitConfig, after: &GitConfig) -> Vec<String> {
    let mut keys: Vec<String> = before
        .keys()
        .chain(after.keys())
        .filter(|key| before.get(*key) != after.get(*key))
        .cloned()
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// The reason of a failed home check.
///
/// ```
/// use real_session::{Changes, changes_text};
///
/// let new = Changes { new: vec!["/h/.cvsignore".into()], ..Changes::default() };
/// assert!(changes_text(&new).starts_with(
///     "the home has 1 new path outside the folders of riff:\n  /h/.cvsignore\n"
/// ));
///
/// let both = Changes {
///     changed: vec!["/h/.gitconfig".into()],
///     new: vec!["/h/a".into(), "/h/b".into()],
///     ..Changes::default()
/// };
/// assert!(changes_text(&both).starts_with(
///     "the home has 1 changed file and 2 new paths outside the folders of riff:\n  \
///      /h/.gitconfig (changed)\n  /h/a\n  /h/b\n"
/// ));
///
/// // A git config names its keys.
/// let config = std::path::PathBuf::from("/h/src/riff/.git/config");
/// let keys = Changes {
///     changed: vec![config.clone()],
///     keys: [(config, vec!["alias.st".into(), "core.hookspath".into()])].into(),
///     ..Changes::default()
/// };
/// assert!(changes_text(&keys).starts_with(
///     "the home has 1 changed file outside the folders of riff:\n  \
///      /h/src/riff/.git/config (changed: alias.st, core.hookspath)\n"
/// ));
/// let changed = Changes {
///     changed: vec!["/h/.bashrc".into(), "/h/.gitconfig".into()],
///     ..Changes::default()
/// };
/// assert!(changes_text(&changed).starts_with("the home has 2 changed files outside"));
/// assert!(changes_text(&changed).ends_with("--skip PATH, and run the test again."));
///
/// let many = Changes {
///     new: (0..22).map(|i| format!("/h/f{i}").into()).collect(),
///     ..Changes::default()
/// };
/// assert!(changes_text(&many).contains("\n  /h/f19\n  and 2 more\n"));
/// ```
pub fn changes_text(changes: &Changes) -> String {
    let count = |n: usize, noun: &str| format!("{n} {noun}{}", if n == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    if !changes.changed.is_empty() {
        parts.push(count(changes.changed.len(), "changed file"));
    }
    if !changes.new.is_empty() {
        parts.push(count(changes.new.len(), "new path"));
    }
    let mut text = format!(
        "the home has {} outside the folders of riff:\n",
        parts.join(" and ")
    );
    let lines: Vec<String> = changes
        .changed
        .iter()
        .map(|path| match changes.keys.get(path) {
            Some(keys) => format!("{} (changed: {})", path.display(), keys.join(", ")),
            None => format!("{} (changed)", path.display()),
        })
        .chain(changes.new.iter().map(|path| path.display().to_string()))
        .collect();
    for line in lines.iter().take(SHOWN) {
        text.push_str(&format!("  {line}\n"));
    }
    if lines.len() > SHOWN {
        text.push_str(&format!("  and {} more\n", lines.len() - SHOWN));
    }
    text.push_str(
        "When another program wrote a path, leave out the path or its folder with \
         --skip PATH, and run the test again.",
    );
    text
}

/// The line that the scratch item adds to [`BOOK_FILE`].
///
/// ```
/// assert_eq!(real_session::scratch_line(1792054064), "Real-session test 1792054064.");
/// ```
pub fn scratch_line(stamp: u64) -> String {
    format!("Real-session test {stamp}.")
}

/// The body of the scratch item: one line in the book, and its `Done
/// when:` line.
///
/// ```
/// let body = real_session::issue_body("Real-session test 7.");
/// assert!(body.contains("add this line at the end of `docs/src/riff-3.md`"));
/// assert!(body.contains("\n```text\nReal-session test 7.\n```\n"));
/// assert!(body.contains("Done when:\n- `docs/src/riff-3.md` ends with the line `Real-session test 7.`"));
/// ```
pub fn issue_body(line: &str) -> String {
    format!(
        "This is a scratch item of the real-session test of riff. `just real-session` makes \
         it, runs it with `riff run`, and closes it with its pull request. No person merges \
         it, and no session verifies it.\n\n\
         Do only this: add this line at the end of `{BOOK_FILE}`, after one empty line:\n\n\
         ```text\n{line}\n```\n\n\
         Change no other file. The change needs no requirement, no test and no other doc.\n\n\
         Done when:\n\
         - `{BOOK_FILE}` ends with the line `{line}`\n\
         - `just check` passes.\n"
    )
}

/// The number of the issue at `url`, as `gh issue create` prints it.
///
/// ```
/// use real_session::issue_number;
///
/// assert_eq!(issue_number("https://github.com/o/r/issues/801\n"), Some(801));
/// assert_eq!(issue_number("Creating issue\nhttps://github.com/o/r/issues/801"), Some(801));
/// assert_eq!(issue_number("https://github.com/o/r/pull/801"), None);
/// ```
pub fn issue_number(url: &str) -> Option<u64> {
    url.trim()
        .lines()
        .last()?
        .rsplit_once("/issues/")?
        .1
        .parse()
        .ok()
}

/// The branch of the session of an issue: the branch of `riff run`.
///
/// ```
/// assert_eq!(real_session::branch(12), "worktree-issue-12");
/// ```
pub fn branch(issue: u64) -> String {
    format!("worktree-issue-{issue}")
}

/// The folders of the sessions of `issue` in `sessions`, with the
/// `ID8` of each: a folder `issue-N-ID8`, where `ID8` is 8 hex digits.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// for name in ["issue-12-1a2b3c4d", "issue-120-ffffffff", "issue-12-x", "issue-12"] {
///     std::fs::create_dir(dir.path().join(name))?;
/// }
/// assert_eq!(
///     real_session::session_folders(dir.path(), 12),
///     [("1a2b3c4d".to_owned(), dir.path().join("issue-12-1a2b3c4d"))]
/// );
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn session_folders(sessions: &Path, issue: u64) -> Vec<(String, PathBuf)> {
    let prefix = format!("issue-{issue}-");
    let Ok(entries) = std::fs::read_dir(sessions) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let id8 = name.strip_prefix(&prefix)?;
            (id8.len() == 8 && id8.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| (id8.to_owned(), entry.path()))
        })
        .collect();
    found.sort();
    found
}

/// Sets `stop` at the first SIGINT or SIGTERM, and ends the process
/// with the exit code 130 at the second. A child keeps the default
/// action of each signal: an exec resets a handler.
pub fn watch_signals(stop: Arc<AtomicBool>) -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (mut int, mut term) = runtime.block_on(async {
        Ok::<_, std::io::Error>((
            signal(SignalKind::interrupt())?,
            signal(SignalKind::terminate())?,
        ))
    })?;
    std::thread::spawn(move || {
        runtime.block_on(async move {
            // A child that ends by the same signal can set `stop` first
            // ([`child_signal`]): so the count is of the signals of this
            // process, not of `stop`.
            let mut first = true;
            loop {
                tokio::select! {
                    _ = int.recv() => {}
                    _ = term.recv() => {}
                }
                if !first {
                    std::process::exit(130);
                }
                first = false;
                stop.store(true, Ordering::SeqCst);
            }
        });
    });
    Ok(())
}

/// True when SIGINT or SIGTERM ended the child with `status`
/// (01M504HBQ3Y0123HWGFP99Z6NV). Then it
/// sets `stop`, as [`watch_signals`] does: Ctrl-C sends SIGINT to each
/// process of the group, and the child can end before the test sees the
/// signal. So the step fails with `a signal stopped the test`, and the
/// exit code is 130.
///
/// ```
/// use std::os::unix::process::ExitStatusExt;
/// use std::process::ExitStatus;
/// use std::sync::atomic::{AtomicBool, Ordering};
/// use real_session::child_signal;
///
/// let stop = AtomicBool::new(false);
/// // SIGKILL, and the exit code 1, are no stop.
/// assert!(!child_signal(ExitStatus::from_raw(9), &stop));
/// assert!(!child_signal(ExitStatus::from_raw(1 << 8), &stop));
/// assert!(!stop.load(Ordering::SeqCst));
/// // SIGINT is.
/// assert!(child_signal(ExitStatus::from_raw(2), &stop));
/// assert!(stop.load(Ordering::SeqCst));
/// ```
pub fn child_signal(status: ExitStatus, stop: &AtomicBool) -> bool {
    use std::os::unix::process::ExitStatusExt;

    const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;
    let signaled = matches!(status.signal(), Some(SIGINT | SIGTERM));
    if signaled {
        stop.store(true, Ordering::SeqCst);
    }
    signaled
}

/// A pull request, as `gh pr view --json number,url,headRefOid,state`
/// gives it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pull {
    number: u64,
    url: String,
    head_ref_oid: String,
    state: String,
}

/// The check runs of a commit, from the API of GitHub.
#[derive(Debug, Deserialize)]
struct CheckRuns {
    #[serde(default)]
    check_runs: Vec<CheckRun>,
}

#[derive(Debug, Deserialize)]
struct CheckRun {
    status: String,
}

/// A CI run, as `gh run list --json databaseId,status` gives it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Run {
    database_id: u64,
    status: String,
}

/// The `riff` of the tree, with the data folder of the test
/// (01M53YP9DATAD95EZEZ6R75WEF).
///
/// ```
/// use std::path::Path;
/// use real_session::{DATA_VAR, Riff};
///
/// let riff = Riff {
///     bin: "target/debug/riff".into(),
///     data: "/h/.local/share/riff/real-session".into(),
/// };
/// let command = riff.command(&["daemon", "status"]);
/// assert_eq!(command.get_program(), "target/debug/riff");
/// assert_eq!(command.get_args().collect::<Vec<_>>(), ["daemon", "status"]);
/// let data = command.get_envs().find(|(name, _)| *name == DATA_VAR);
/// assert_eq!(data, Some((DATA_VAR.as_ref(), Some(riff.data.as_os_str()))));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Riff {
    /// The riff binary of the tree.
    pub bin: PathBuf,
    /// The data folder of the test ([`Env::test_data`]).
    pub data: PathBuf,
}

impl Riff {
    /// The riff binary `bin`, with the data folder of the test of `env`.
    pub fn new(bin: &Path, env: &Env) -> Riff {
        Riff {
            bin: bin.to_owned(),
            data: env.test_data(),
        }
    }

    /// `riff ARGS`, with `RIFF_DATA` ([`DATA_VAR`]) set to the data
    /// folder of the test.
    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.bin);
        command.args(args).env(DATA_VAR, &self.data);
        command
    }
}

/// What the test made, for the clean-up.
#[derive(Debug, Default)]
struct Made {
    /// The scratch issue.
    issue: Option<u64>,
    /// True when the test holds `verify-issue-N`.
    held: bool,
    /// The `riff` of the test.
    riff: Option<Riff>,
    /// The folder of the session folders.
    sessions: Option<PathBuf>,
    /// The riff daemon of the test.
    daemon: Option<Child>,
}

/// Runs the test, and cleans up. It gives the exit code: 0 on a pass, 1
/// when a step fails, and 130 when a signal stopped it
/// (01M4ZCFGF6QN6KZEVMBNW9G4EW).
pub fn run(options: &Options, stop: &AtomicBool) -> u8 {
    let mut made = Made::default();
    let result = steps(options, stop, &mut made);
    if let Some(issue) = made.issue {
        clean_up(issue, made, stop);
    }
    match result {
        Ok(summary) => {
            println!("real-session: PASS: {summary}");
            0
        }
        Err(failure) => {
            eprintln!("real-session: FAIL at {failure}");
            if stop.load(Ordering::SeqCst) { 130 } else { 1 }
        }
    }
}

/// Each step, in order. The first one that fails stops the test.
fn steps(options: &Options, stop: &AtomicBool, made: &mut Made) -> Result<String, Failure> {
    let (env, image, clones) = step(Step::Host, stop, || host(options, stop))?;
    let riff = Riff::new(&options.riff, &env);
    made.riff = Some(riff.clone());
    made.sessions = Some(env.sessions());
    let skips: Vec<PathBuf> = options
        .skips
        .iter()
        .map(|skip| std::path::absolute(skip).unwrap_or_else(|_| skip.clone()))
        .collect();
    let home = Home {
        root: env.home.clone(),
        skips: env.skips(&skips),
        claude: env.claude.clone(),
        claude_json: env.claude_json.clone(),
        guarded: env.guarded(),
        clones,
    };
    let before = step(Step::List, stop, || {
        let listing = home.list();
        println!("  {} paths", listing.len());
        Ok(listing)
    })?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let line = scratch_line(stamp);
    let issue = step(Step::Item, stop, || make_item(stamp, &line, stop))?;
    made.issue = Some(issue);
    made.held = step(Step::Keep, stop, || keep_verify(&riff, issue, stop))?;
    step(Step::Daemon, stop, || {
        start_daemon(&riff, stop, &mut made.daemon)
    })?;
    let item = format!("issue-{issue}");
    step(Step::Run, stop, || run_item(&riff, &item, &image, stop))?;
    step(Step::Home, stop, || check_home(&home, &before, &env, issue))?;
    let pull = step(Step::Pull, stop, || find_pull(issue, stop))?;
    step(Step::Change, stop, || check_change(&pull, &line, stop))?;
    let gate = step(Step::Gate, stop, || {
        wait_for_gate(&pull, options.gate_wait, stop)
    })?;
    Ok(format!(
        "issue #{issue}, pull request #{} ({}), the Gate is {gate}, no change in the home",
        pull.number, pull.url
    ))
}

/// Runs one step: it shows the step, and names it when it fails. A
/// signal fails the step.
fn step<T>(
    step: Step,
    stop: &AtomicBool,
    work: impl FnOnce() -> Result<T, String>,
) -> Result<T, Failure> {
    let stopped = || Failure {
        step,
        reason: "a signal stopped the test".into(),
    };
    if stop.load(Ordering::SeqCst) {
        return Err(stopped());
    }
    println!(
        "real-session: step {} of {}: {}",
        step.number(),
        Step::ALL.len(),
        step.name()
    );
    let result = work();
    if stop.load(Ordering::SeqCst) {
        return Err(stopped());
    }
    result.map_err(|reason| Failure { step, reason })
}

/// Runs `program ARGS`, and gives its stdout with no space at the ends
/// ([`output_of`]).
fn output(program: impl AsRef<OsStr>, args: &[&str], stop: &AtomicBool) -> Result<String, String> {
    let mut command = Command::new(program);
    command.args(args);
    output_of(command, stop)
}

/// Runs `command`, and gives its stdout with no space at the ends. An
/// error names the command and its stderr. A child that ends by SIGINT
/// or SIGTERM sets `stop` ([`child_signal`]).
fn output_of(mut command: Command, stop: &AtomicBool) -> Result<String, String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let out = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if !out.status.success() {
        child_signal(out.status, stop);
        return Err(format!(
            "{program} {} ended with {}: {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Step 1: podman runs rootless, `gh` has a sign-in, and `riff image`
/// names the image of the tree. It gives the folders, the image, and the
/// main clone and the worktree of the test.
fn host(options: &Options, stop: &AtomicBool) -> Result<(Env, String, Vec<PathBuf>), String> {
    let env = Env::of_process()?;
    let riff = Riff::new(&options.riff, &env);
    let rootless = output(
        "podman",
        &["info", "--format", "{{.Host.Security.Rootless}}"],
        stop,
    )?;
    if rootless != "true" {
        return Err(format!(
            "podman does not run rootless: podman info says {rootless:?}"
        ));
    }
    output("gh", &["auth", "status"], stop)?;
    let image = output_of(riff.command(&["image"]), stop)?;
    if image.is_empty() {
        return Err("riff image printed no image".into());
    }
    let common = PathBuf::from(output(
        "git",
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        stop,
    )?);
    let main = common
        .parent()
        .ok_or_else(|| format!("the git dir {} has no parent", common.display()))?
        .to_owned();
    let top = PathBuf::from(output("git", &["rev-parse", "--show-toplevel"], stop)?);
    println!("  image {image}");
    Ok((env, image, vec![main, top]))
}

/// Step 3: the scratch issue. It gives its number.
fn make_item(stamp: u64, line: &str, stop: &AtomicBool) -> Result<u64, String> {
    let title = format!("Real-session test {stamp}");
    let url = output(
        "gh",
        &[
            "issue",
            "create",
            "--title",
            &title,
            "--milestone",
            MILESTONE,
            "--body",
            &issue_body(line),
        ],
        stop,
    )?;
    let issue =
        issue_number(&url).ok_or_else(|| format!("gh issue create printed no issue: {url}"))?;
    println!("  issue #{issue}: {url}");
    Ok(issue)
}

/// Step 4: keeps the verify of the scratch item from the workers
/// (01M5157V34H7FWFH6S2DRQXRMP). The issue gets the label [`NO_VERIFY`]:
/// `gh label create --force` makes the label or updates it. Then the
/// test holds `verify-issue-N`. True when it holds it.
fn keep_verify(riff: &Riff, issue: u64, stop: &AtomicBool) -> Result<bool, String> {
    output(
        "gh",
        &[
            "label",
            "create",
            NO_VERIFY,
            "--description",
            NO_VERIFY_TEXT,
            "--color",
            NO_VERIFY_COLOR,
            "--force",
        ],
        stop,
    )?;
    output(
        "gh",
        &[
            "issue",
            "edit",
            &issue.to_string(),
            "--add-label",
            NO_VERIFY,
        ],
        stop,
    )?;
    println!("  issue #{issue} has the label {NO_VERIFY}");
    Ok(hold(riff, issue, stop))
}

/// Holds `verify-issue-N`. True when it did. A failed hold is a warning:
/// the label [`NO_VERIFY`] keeps the verify.
fn hold(riff: &Riff, issue: u64, stop: &AtomicBool) -> bool {
    let item = format!("verify-issue-{issue}");
    match output_of(riff.command(&["plan", "hold", &item, HOLD_REASON]), stop) {
        Ok(_) => {
            println!("  held {item}");
            true
        }
        Err(e) => {
            println!(
                "  warning: riff did not hold {item}, and the label {NO_VERIFY} keeps its \
                 verify from the workers: {e}"
            );
            false
        }
    }
}

/// Step 5: starts `riff daemon` of the tree with the data folder of the
/// test, in a process group of its own, and puts it in `daemon`, also
/// when the step fails. It waits until `riff daemon status` of the data
/// folder answers (01M4ZFPYNGFF49WWHR8EBWSTNG,
/// 01M53YP9DATAD95EZEZ6R75WEF).
fn start_daemon(riff: &Riff, stop: &AtomicBool, daemon: &mut Option<Child>) -> Result<(), String> {
    use std::os::unix::process::CommandExt;

    let status = || output_of(riff.command(&["daemon", "status"]), stop);
    if let Ok(status) = status() {
        return Err(daemon_runs_text(&status));
    }
    let child = riff
        .command(&["daemon"])
        .stdin(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("cannot run {} daemon: {e}", riff.bin.display()))?;
    println!(
        "  riff daemon, pid {}, in {}",
        child.id(),
        riff.data.display()
    );
    let child = daemon.insert(child);
    let start = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            child_signal(status, stop);
            return Err(daemon_ended_text(status));
        }
        if status().is_ok() {
            println!("  it answers");
            return Ok(());
        }
        if start.elapsed() >= DAEMON_WAIT {
            return Err(format!(
                "riff daemon did not answer in {} s",
                DAEMON_WAIT.as_secs()
            ));
        }
        sleep(Duration::from_millis(100), stop);
        if stop.load(Ordering::SeqCst) {
            return Err("a signal stopped the wait".into());
        }
    }
}

/// The reason of a failed step 5 when a daemon of the test answers
/// before the start. `status` is the output of `riff daemon status` with
/// the data folder of the test. The daemon of the machine is not that
/// daemon, and the text names no step for it
/// (01M53YP9DATAD95EZEZ6R75WEF).
///
/// ```
/// let text = real_session::daemon_runs_text("pid 7");
/// assert!(text.starts_with("a riff daemon of the test runs already:\npid 7\n"));
/// assert!(text.contains("Another real-session test runs"));
/// assert!(!text.contains("systemctl"));
/// ```
pub fn daemon_runs_text(status: &str) -> String {
    format!(
        "a riff daemon of the test runs already:\n{status}\n\
         Another real-session test runs, or a test that a second signal stopped left its \
         daemon. Let the other test end, or stop its daemon by the pid above, and run the test \
         again."
    )
}

/// The reason of a failed step 5 when the daemon ends before it answers.
/// Its own message is above, in the output of the test.
fn daemon_ended_text(status: ExitStatus) -> String {
    format!("riff daemon ended with {status} before it answered: see its message above")
}

/// Stops the daemon of the test with SIGTERM. When it does not end in
/// [`DAEMON_WAIT`], it kills it.
fn stop_daemon(daemon: &mut Child, stop: &AtomicBool) -> Result<String, String> {
    if let Ok(Some(status)) = daemon.try_wait() {
        return Ok(status.to_string());
    }
    let term = output("kill", &["-TERM", &daemon.id().to_string()], stop);
    let start = Instant::now();
    while term.is_ok() && start.elapsed() < DAEMON_WAIT {
        if let Ok(Some(status)) = daemon.try_wait() {
            return Ok(status.to_string());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = daemon.kill();
    let _ = daemon.wait();
    Err(match term {
        Ok(_) => format!(
            "it did not end in {} s after SIGTERM, so the test killed it",
            DAEMON_WAIT.as_secs()
        ),
        Err(e) => format!("{e}, so the test killed it"),
    })
}

/// Step 6: `riff run ITEM --image IMAGE`. The person sees its output.
/// When it ends by SIGINT or SIGTERM, it sets `stop` ([`child_signal`]).
fn run_item(riff: &Riff, item: &str, image: &str, stop: &AtomicBool) -> Result<(), String> {
    let status = riff
        .command(&["run", item, "--image", image])
        .stdin(Stdio::null())
        .status()
        .map_err(|e| format!("cannot run {}: {e}", riff.bin.display()))?;
    if status.success() {
        Ok(())
    } else {
        child_signal(status, stop);
        Err(format!("riff run {item} ended with {status}"))
    }
}

/// Step 7: the home has no new path and no changed file that riff
/// must not write.
fn check_home(home: &Home, before: &Listing, env: &Env, issue: u64) -> Result<(), String> {
    let marks: Vec<String> = session_folders(&env.sessions(), issue)
        .into_iter()
        .map(|(id8, _)| id8)
        .collect();
    let changes = home.changes(before, &marks);
    if changes.is_empty() {
        println!("  no new path, no changed file");
        Ok(())
    } else {
        Err(changes_text(&changes))
    }
}

/// The open pull request of `branch`, when it has one.
fn pull_of(branch: &str, stop: &AtomicBool) -> Option<Pull> {
    let text = output(
        "gh",
        &[
            "pr",
            "view",
            branch,
            "--json",
            "number,url,headRefOid,state",
        ],
        stop,
    )
    .ok()?;
    serde_json::from_str::<Pull>(&text)
        .ok()
        .filter(|pull| pull.state == "OPEN")
}

/// Step 8: the open pull request of the branch of the item.
fn find_pull(issue: u64, stop: &AtomicBool) -> Result<Pull, String> {
    let branch = branch(issue);
    let pull = pull_of(&branch, stop)
        .ok_or_else(|| format!("the branch {branch} has no open pull request"))?;
    if !pull.head_ref_oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!(
            "pull request #{} has a head that is not a commit: {}",
            pull.number, pull.head_ref_oid
        ));
    }
    println!("  pull request #{}: {}", pull.number, pull.url);
    Ok(pull)
}

/// Step 9: the diff of the pull request adds the line to [`BOOK_FILE`].
fn check_change(pull: &Pull, line: &str, stop: &AtomicBool) -> Result<(), String> {
    let number = pull.number.to_string();
    let diff = output("gh", &["pr", "diff", &number, "--color", "never"], stop)?;
    let file = format!("+++ b/{BOOK_FILE}");
    let added = format!("+{line}");
    if diff.lines().any(|l| l == file) && diff.lines().any(|l| l == added) {
        println!("  it adds {line:?} to {BOOK_FILE}");
        Ok(())
    } else {
        Err(format!(
            "the diff of pull request #{number} does not add the line {line:?} to {BOOK_FILE}"
        ))
    }
}

/// Step 10: waits until the head commit has a check run [`GATE`]. It
/// gives the status of the run.
fn wait_for_gate(pull: &Pull, wait: Duration, stop: &AtomicBool) -> Result<String, String> {
    let commit = &pull.head_ref_oid;
    let path = format!("repos/{{owner}}/{{repo}}/commits/{commit}/check-runs?check_name={GATE}");
    let start = Instant::now();
    let mut last = String::new();
    loop {
        match output("gh", &["api", &path], stop) {
            Ok(text) => match serde_json::from_str::<CheckRuns>(&text) {
                Ok(runs) => {
                    if let Some(run) = runs.check_runs.first() {
                        println!("  the Gate is {}", run.status);
                        return Ok(run.status.clone());
                    }
                }
                Err(e) => last = format!(": the answer of GitHub has no check runs: {e}"),
            },
            Err(e) => last = format!(": {e}"),
        }
        let waited = start.elapsed();
        if waited >= wait {
            let short = commit.get(..7).unwrap_or(commit);
            return Err(format!(
                "the Gate did not start on {short} in {} s{last}",
                wait.as_secs()
            ));
        }
        sleep(POLL.min(wait - waited), stop);
        if stop.load(Ordering::SeqCst) {
            return Err("a signal stopped the wait".into());
        }
    }
}

/// Sleeps for `time`, or until `stop` is set.
fn sleep(time: Duration, stop: &AtomicBool) {
    let end = Instant::now() + time;
    while !stop.load(Ordering::SeqCst) {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        std::thread::sleep(left.min(Duration::from_millis(100)));
    }
}

/// Shows the result of one part of the clean-up. A failed part is a
/// warning: the other parts still run.
fn report(what: &str, result: Result<String, String>) {
    match result {
        Ok(_) => println!("  {what}: done"),
        Err(e) => println!("  warning: {what}: {e}"),
    }
}

/// Stops the daemon of the test (01M4ZFPYNGFF49WWHR8EBWSTNG), closes the
/// pull request and deletes its branch, cancels each CI run of the branch
/// that did not end, closes the issue, frees the hold, and removes the
/// folder of the session (01M4ZCFGCY301G2ARFMRXV7XDR). It stops only the
/// daemon of the test (01M53YP9DATAD95EZEZ6R75WEF).
fn clean_up(issue: u64, made: Made, stop: &AtomicBool) {
    println!("real-session: clean up issue #{issue}");
    if let Some(mut daemon) = made.daemon {
        report("stop the riff daemon", stop_daemon(&mut daemon, stop));
    }
    let branch = branch(issue);
    match pull_of(&branch, stop) {
        Some(pull) => report(
            &format!("close pull request #{} and delete {branch}", pull.number),
            output(
                "gh",
                &[
                    "pr",
                    "close",
                    &pull.number.to_string(),
                    "--delete-branch",
                    "--comment",
                    CLOSE_TEXT,
                ],
                stop,
            ),
        ),
        // The session can push the branch and open no pull request. When
        // the branch is not there, GitHub says so: that is no fault.
        None => {
            let _ = output(
                "gh",
                &[
                    "api",
                    "-X",
                    "DELETE",
                    &format!("repos/{{owner}}/{{repo}}/git/refs/heads/{branch}"),
                ],
                stop,
            );
        }
    }
    match output(
        "gh",
        &[
            "run",
            "list",
            "--branch",
            &branch,
            "--json",
            "databaseId,status",
        ],
        stop,
    ) {
        Ok(text) => {
            let runs: Vec<Run> = serde_json::from_str(&text).unwrap_or_default();
            for run in runs.iter().filter(|run| run.status != "completed") {
                let id = run.database_id.to_string();
                report(
                    &format!("cancel the CI run {id}"),
                    output("gh", &["run", "cancel", &id], stop),
                );
            }
        }
        Err(e) => println!("  warning: list the CI runs of {branch}: {e}"),
    }
    report(
        &format!("close issue #{issue}"),
        output(
            "gh",
            &[
                "issue",
                "close",
                &issue.to_string(),
                "--reason",
                "not planned",
                "--comment",
                CLOSE_TEXT,
            ],
            stop,
        ),
    );
    if made.held
        && let Some(riff) = &made.riff
    {
        let item = format!("verify-issue-{issue}");
        report(
            &format!("free {item}"),
            output_of(riff.command(&["plan", "free", &item]), stop),
        );
    }
    for (_, folder) in made
        .sessions
        .map(|sessions| session_folders(&sessions, issue))
        .unwrap_or_default()
    {
        report(
            &format!("remove {}", folder.display()),
            std::fs::remove_dir_all(&folder)
                .map(|()| String::new())
                .map_err(|e| e.to_string()),
        );
        // The lock of the riff run of the folder is beside it: `ITEM-ID8.lock`
        // (`riff::daemon::sweep::lock_of`).
        let mut lock = folder.clone().into_os_string();
        lock.push(".lock");
        let _ = std::fs::remove_file(lock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Holds each write of a script and each spawn of a child: a fork
    /// while a script is open to write makes its exec fail with ETXTBSY.
    static SPAWN: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn a_signal_fails_the_step_that_runs_and_each_step_after_it() {
        let stop = AtomicBool::new(false);
        let failure = step(Step::Run, &stop, || {
            stop.store(true, Ordering::SeqCst);
            Ok(())
        })
        .unwrap_err();
        assert_eq!(failure.step, Step::Run);
        assert_eq!(failure.reason, "a signal stopped the test");
        let mut ran = false;
        let failure = step(Step::Home, &stop, || {
            ran = true;
            Ok(())
        })
        .unwrap_err();
        assert_eq!(failure.step, Step::Home);
        assert!(!ran);
    }

    #[test]
    fn a_child_that_ends_by_a_signal_stops_the_test() {
        let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        let stop = AtomicBool::new(false);
        let failure = step(Step::Item, &stop, || {
            output("sh", &["-c", "kill -TERM $$"], &stop)
        })
        .unwrap_err();
        assert_eq!(failure.reason, "a signal stopped the test");
        assert!(stop.load(Ordering::SeqCst));
    }

    #[test]
    fn a_child_that_fails_does_not_stop_the_test() {
        let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        let stop = AtomicBool::new(false);
        let failure =
            step(Step::Item, &stop, || output("sh", &["-c", "exit 3"], &stop)).unwrap_err();
        assert!(
            failure.reason.contains("exit status: 3"),
            "{}",
            failure.reason
        );
        assert!(!stop.load(Ordering::SeqCst));
    }

    #[test]
    fn a_riff_run_that_ends_by_a_signal_stops_the_test() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let riff = dir.path().join("riff");
        std::fs::write(&riff, "#!/bin/sh\nkill -TERM $$\n").unwrap();
        std::fs::set_permissions(&riff, std::fs::Permissions::from_mode(0o755)).unwrap();
        let riff = Riff {
            bin: riff,
            data: dir.path().join("data"),
        };
        let stop = AtomicBool::new(false);
        let failure = step(Step::Run, &stop, || {
            run_item(&riff, "issue-12", "image", &stop)
        })
        .unwrap_err();
        assert_eq!(failure.step, Step::Run);
        assert_eq!(failure.reason, "a signal stopped the test");
    }

    #[test]
    fn a_sleep_ends_when_the_stop_is_set() {
        let stop = AtomicBool::new(true);
        let start = Instant::now();
        sleep(Duration::from_secs(30), &stop);
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_new_folder_in_the_claude_folder_counts_only_for_its_path_with_the_mark() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_owned();
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        let home = home_in(&root, Vec::new());
        let before = home.list();
        let todos = root.join(".claude/todos");
        std::fs::create_dir_all(&todos).unwrap();
        std::fs::write(todos.join("other-agent.json"), "").unwrap();
        std::fs::write(todos.join("1a2b3c4d-agent.json"), "").unwrap();
        assert_eq!(
            home.changes(&before, &["1a2b3c4d".into()]).new,
            [todos.join("1a2b3c4d-agent.json")]
        );
        // No mark: nothing in the Claude folder counts.
        assert!(home.changes(&before, &[String::new()]).is_empty());
    }

    #[test]
    fn the_worktrees_and_the_git_of_a_clone_do_not_count_and_the_rest_of_it_does() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_owned();
        let clone = root.join("src/riff");
        let worktree = clone.join(".claude/worktrees/issue-1");
        std::fs::create_dir_all(clone.join(".git/objects/ab")).unwrap();
        std::fs::create_dir_all(clone.join(".git/hooks")).unwrap();
        std::fs::write(clone.join(".git/config"), "[core]\n").unwrap();
        std::fs::create_dir_all(&worktree).unwrap();
        // A folder `.claude/worktrees` in a folder that is not a clone.
        let not_clone = root.join("notes/.claude/worktrees");
        std::fs::create_dir_all(&not_clone).unwrap();
        let home = home_in(&root, Vec::new());
        let before = home.list();

        // The other sessions build and commit in their worktrees, and
        // make a new worktree.
        std::fs::create_dir_all(worktree.join("target/debug")).unwrap();
        std::fs::write(worktree.join("target/debug/riff"), "").unwrap();
        std::fs::write(worktree.join("new.rs"), "").unwrap();
        std::fs::create_dir_all(clone.join(".claude/worktrees/issue-2")).unwrap();
        std::fs::write(clone.join(".git/objects/ab/cdef"), "").unwrap();
        std::fs::create_dir_all(clone.join(".git/worktrees/issue-2")).unwrap();
        std::fs::write(clone.join(".git/worktrees/issue-2/HEAD"), "").unwrap();
        assert_eq!(home.changes(&before, &[]), Changes::default());

        // A new path in the rest of the clone, a new hook and config file
        // of git, and a new path in the folder that is not a clone count.
        std::fs::write(clone.join("notes.md"), "").unwrap();
        std::fs::write(clone.join(".claude/settings.json"), "{}").unwrap();
        std::fs::write(clone.join(".git/hooks/pre-commit"), "").unwrap();
        std::fs::write(clone.join(".git/config.worktree"), "").unwrap();
        std::fs::write(not_clone.join("x"), "").unwrap();
        assert_eq!(
            home.changes(&before, &[]).new,
            [
                not_clone.join("x"),
                clone.join(".claude/settings.json"),
                clone.join(".git/config.worktree"),
                clone.join(".git/hooks/pre-commit"),
                clone.join("notes.md"),
            ]
        );
    }

    /// A [`Home`] in `root` with the Claude folder `.claude`, its
    /// `.claude.json` beside it, and `guarded`.
    fn home_in(root: &Path, guarded: Vec<PathBuf>) -> Home {
        Home {
            root: root.to_owned(),
            skips: Vec::new(),
            claude: root.join(".claude"),
            claude_json: root.join(".claude.json"),
            guarded,
            clones: Vec::new(),
        }
    }

    #[test]
    fn a_changed_claude_json_counts_only_when_its_text_names_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_owned();
        let json = root.join(".claude.json");
        std::fs::write(&json, r#"{"numStartups":1}"#).unwrap();
        let home = home_in(&root, vec![json.clone()]);
        let before = home.list();
        // Another session of the person writes it, and its lock.
        std::fs::write(&json, r#"{"numStartups":2}"#).unwrap();
        std::fs::write(root.join(".claude.json.lock"), "").unwrap();
        let marks = ["1a2b3c4d".to_owned()];
        assert_eq!(home.changes(&before, &marks), Changes::default());
        // A project of the session in it counts.
        std::fs::write(&json, r#"{"projects":{"/s/issue-12-1a2b3c4d/clone":{}}}"#).unwrap();
        assert_eq!(home.changes(&before, &marks).changed, [json]);
    }

    #[test]
    fn a_guarded_folder_guards_each_file_in_it_and_a_new_inode_counts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_owned();
        let gh = root.join(".config/gh");
        std::fs::create_dir_all(&gh).unwrap();
        let hosts = gh.join("hosts.yml");
        std::fs::write(&hosts, "github.com:\n").unwrap();
        let other = root.join(".config/other.yml");
        std::fs::write(&other, "a\n").unwrap();
        let home = home_in(&root, vec![gh.clone()]);
        let before = home.list();
        assert_eq!(home.changes(&before, &[]), Changes::default());

        // The same size and time of change, in a new file: a write by a
        // rename.
        let time = std::fs::metadata(&hosts).unwrap().modified().unwrap();
        let temp = gh.join("hosts.yml.new");
        std::fs::write(&temp, "gitlab.com:\n").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&temp)
            .unwrap()
            .set_modified(time)
            .unwrap();
        std::fs::rename(&temp, &hosts).unwrap();
        // A file that the check does not guard can change.
        std::fs::write(&other, "b\n").unwrap();
        assert_eq!(
            home.changes(&before, &[]),
            Changes {
                changed: vec![hosts],
                ..Changes::default()
            }
        );
    }

    /// A clone with a branch section and a hook, as `git push -u` and
    /// the person leave them.
    fn clone_in(clone: &Path) {
        std::fs::create_dir_all(clone.join(".git/hooks")).unwrap();
        std::fs::write(
            clone.join(".git/config"),
            "[core]\n\tbare = false\n[branch \"main\"]\n\tremote = origin\n",
        )
        .unwrap();
        std::fs::write(clone.join(".git/hooks/pre-push"), "exit 0\n").unwrap();
    }

    #[test]
    fn a_new_branch_section_does_not_count_and_each_other_key_and_a_changed_hook_do() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_owned();
        let clone = root.join("src/main");
        clone_in(&clone);
        let config = clone.join(".git/config");
        let home = home_in(&root, Vec::new());
        let before = home.list();

        // A session pushes a new branch with -u, and deletes the old one.
        std::fs::write(
            &config,
            "[core]\n\tbare = false\n[branch \"worktree-issue-1\"]\n\tremote = origin\n\t\
             merge = refs/heads/worktree-issue-1\n",
        )
        .unwrap();
        assert_eq!(home.changes(&before, &[]), Changes::default());

        // A new key, a changed key and a new section count, by name.
        std::fs::write(
            &config,
            "[core]\n\tbare = true\n\thooksPath = /tmp/h\n[alias]\n\tst = status\n",
        )
        .unwrap();
        // A write to a hook that is there already counts.
        std::fs::write(clone.join(".git/hooks/pre-push"), "curl -s x | sh\n").unwrap();
        let changes = home.changes(&before, &[]);
        assert_eq!(
            changes.changed,
            [config.clone(), clone.join(".git/hooks/pre-push")]
        );
        assert_eq!(
            changes.keys,
            [(
                config.clone(),
                vec![
                    "alias.st".to_owned(),
                    "core.bare".into(),
                    "core.hookspath".into()
                ]
            )]
            .into()
        );
        assert!(changes.new.is_empty());

        // A removed key counts.
        std::fs::write(clone.join(".git/hooks/pre-push"), "exit 0\n").unwrap();
        std::fs::write(&config, "[branch \"main\"]\n\tremote = origin\n").unwrap();
        assert_eq!(home.changes(&before, &[]).keys[&config], ["core.bare"]);
    }

    #[test]
    fn a_clone_of_the_test_is_left_out_except_its_hooks_and_its_git_config() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_owned();
        let clone = root.join("src/riff");
        let worktree = clone.join(".claude/worktrees/issue-9");
        clone_in(&clone);
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join(".git"), "gitdir: x\n").unwrap();
        let home = Home {
            clones: vec![clone.clone(), worktree.clone()],
            ..home_in(&root, Vec::new())
        };
        let before = home.list();

        // The build of the test and the fetch of riff run.
        std::fs::create_dir_all(worktree.join("target/debug")).unwrap();
        std::fs::write(worktree.join("target/debug/riff"), "").unwrap();
        std::fs::write(clone.join("notes.md"), "").unwrap();
        std::fs::write(clone.join(".git/FETCH_HEAD"), "").unwrap();
        assert_eq!(home.changes(&before, &[]), Changes::default());

        // Its hooks and its git config count.
        let config = clone.join(".git/config");
        std::fs::write(&config, "[core]\n\tbare = false\n\tfsmonitor = x\n").unwrap();
        std::fs::write(clone.join(".git/hooks/post-checkout"), "").unwrap();
        let changes = home.changes(&before, &[]);
        assert_eq!(changes.keys[&config], ["core.fsmonitor"]);
        assert_eq!(changes.changed, [config]);
        assert_eq!(changes.new, [clone.join(".git/hooks/post-checkout")]);
    }

    #[test]
    fn each_step_has_its_own_name_and_number() {
        let names: HashSet<&str> = Step::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(names.len(), Step::ALL.len());
        for (i, step) in Step::ALL.iter().enumerate() {
            assert_eq!(step.number(), i + 1);
        }
    }
}
