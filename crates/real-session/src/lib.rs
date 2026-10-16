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
//!     T->>D: riff daemon, in a process group of its own
//!     T->>R: riff run issue-N --image IMAGE
//!     R->>D: run the session in a container
//!     R->>G: the session pushes, and opens the pull request
//!     T->>T: list the home again: no new path
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
//! | 5. start the daemon | no riff daemon answers before, and `riff daemon` of the tree answers in [`DAEMON_WAIT`] (see below) |
//! | 6. run the item | `riff run issue-N --image IMAGE` ends with 0 |
//! | 7. check the home | the home has no new path (see below) |
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
//! `riff run` asks the riff daemon to run the session (#716). So the
//! test starts `riff daemon` of the tree, with the variables of the
//! test, and waits until `riff daemon status` of the tree answers
//! (01M4ZFPYNGFF49WWHR8EBWSTNG). When a riff daemon answers before the
//! start, for example the unit `riff.service` of an installed riff, the
//! step fails: the test does not stop a daemon of the person.
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
//! | `RIFF_HOME`, and `riff` in each XDG base folder | the folders of riff |
//! | `containers` in `XDG_DATA_HOME` and in `XDG_CONFIG_HOME` | the images and the volumes of podman |
//! | the main clone, and the worktree of the test | the build of the test, and the fetch of `riff run` |
//! | `XDG_CACHE_HOME` | the caches of the tools of the host |
//! | each folder of `--skip` | the other programs of the person that write while the test runs |
//!
//! The other sessions of the person write in the Claude folder of the
//! person (`CLAUDE_CONFIG_DIR`, else `~/.claude`) while the test runs.
//! So in that folder, only a new path that names the session counts: its
//! `ID8`, from the name of its folder `issue-N-ID8` ([`session_folders`]).

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fmt;
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

/// The most new paths that a failed home check shows.
pub const SHOWN: usize = 20;

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

/// The comment of the test when it closes the pull request and the issue.
pub const CLOSE_TEXT: &str = "The real-session test closes its scratch item.";

/// The usage line of the command.
pub const USAGE: &str = "usage: real-session --riff PATH [--skip DIR]... [--gate-wait SECONDS]";

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
    /// The folders that the home check also leaves out.
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
    /// assert!(Env::from_vars(|_| None).is_err());
    /// # Ok::<(), String>(())
    /// ```
    pub fn from_vars(var: impl Fn(&str) -> Option<OsString>) -> Result<Env, String> {
        let set = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = set("HOME").ok_or("set HOME")?;
        let base = |name: &str, default: &str| set(name).unwrap_or_else(|| home.join(default));
        Ok(Env {
            riff_home: set("RIFF_HOME"),
            config: base("XDG_CONFIG_HOME", ".config"),
            data: base("XDG_DATA_HOME", ".local/share"),
            state: base("XDG_STATE_HOME", ".local/state"),
            cache: base("XDG_CACHE_HOME", ".cache"),
            claude: base("CLAUDE_CONFIG_DIR", ".claude"),
            home,
        })
    }

    /// [`Env::from_vars`] with the variables of this process.
    pub fn of_process() -> Result<Env, String> {
        Env::from_vars(|name| std::env::var_os(name))
    }

    /// The folder of the session folders of `riff run`: `RIFF_HOME/sessions`,
    /// else `riff/sessions` in [`Env::data`], as `riff::run::sessions_dir`
    /// gives it.
    ///
    /// ```
    /// use std::path::Path;
    /// use real_session::Env;
    ///
    /// let env = Env::from_vars(|name| (name == "HOME").then(|| "/h".into()))?;
    /// assert_eq!(env.sessions(), Path::new("/h/.local/share/riff/sessions"));
    /// let env = Env { riff_home: Some("/r".into()), ..env };
    /// assert_eq!(env.sessions(), Path::new("/r/sessions"));
    /// # Ok::<(), String>(())
    /// ```
    pub fn sessions(&self) -> PathBuf {
        match &self.riff_home {
            Some(dir) => dir.join("sessions"),
            None => self.data.join("riff/sessions"),
        }
    }

    /// The folders that the home check leaves out: the folders of riff,
    /// the storage of podman, the cache, `clones`, and `skips`
    /// (01M4ZCFGHCS7WJ6CFYBZSX0AT5).
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use real_session::Env;
    ///
    /// let env = Env::from_vars(|name| (name == "HOME").then(|| "/h".into()))?;
    /// let skips = env.skips(&["/h/src/riff".into()], &["/h/.mozilla".into()]);
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
    ///         "/h/src/riff",
    ///         "/h/.mozilla",
    ///     ]
    /// );
    /// # Ok::<(), String>(())
    /// ```
    pub fn skips(&self, clones: &[PathBuf], skips: &[PathBuf]) -> Vec<PathBuf> {
        let mut all: Vec<PathBuf> = [&self.config, &self.data, &self.state, &self.cache]
            .iter()
            .map(|base| base.join("riff"))
            .collect();
        all.extend(self.riff_home.clone());
        all.push(self.data.join("containers"));
        all.push(self.config.join("containers"));
        all.push(self.cache.clone());
        all.extend(clones.iter().cloned());
        all.extend(skips.iter().cloned());
        all
    }
}

/// The paths of a home, each one by its hash: a home can hold millions
/// of paths.
#[derive(Debug, Default)]
pub struct Listing {
    paths: HashSet<u64>,
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
}

/// The hash of a path in a [`Listing`].
fn key(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

/// The home check (01M4ZCFGHCS7WJ6CFYBZSX0AT5).
///
/// ```
/// use real_session::Home;
///
/// let dir = tempfile::tempdir()?;
/// let root = dir.path().to_owned();
/// std::fs::create_dir_all(root.join(".claude/projects"))?;
/// std::fs::create_dir_all(root.join(".local/share/riff"))?;
/// let home = Home {
///     root: root.clone(),
///     skips: vec![root.join(".local/share/riff")],
///     claude: root.join(".claude"),
/// };
/// let before = home.list();
/// assert_eq!(before.len(), 4);
///
/// // A new file, and a new folder with a file in it: they count.
/// std::fs::write(root.join(".cvsignore"), "")?;
/// std::fs::create_dir_all(root.join("new/deep"))?;
/// // A file of riff, and a file of another Claude session: they do not.
/// std::fs::write(root.join(".local/share/riff/state"), "")?;
/// std::fs::write(root.join(".claude/projects/other.jsonl"), "")?;
/// // A folder in the Claude folder that names the session counts.
/// std::fs::create_dir_all(root.join(".claude/projects/-s-issue-12-1a2b3c4d-clone"))?;
///
/// assert_eq!(
///     home.new_paths(&before, &["1a2b3c4d".into()]),
///     [
///         root.join(".claude/projects/-s-issue-12-1a2b3c4d-clone"),
///         root.join(".cvsignore"),
///         root.join("new"),
///     ]
/// );
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    /// The home.
    pub root: PathBuf,
    /// The folders that the check leaves out.
    pub skips: Vec<PathBuf>,
    /// The Claude folder of the person.
    pub claude: PathBuf,
}

impl Home {
    /// Each path of the home, except the paths in [`Home::skips`].
    pub fn list(&self) -> Listing {
        let mut listing = Listing::default();
        self.walk(&mut |path| {
            listing.paths.insert(key(path));
            true
        });
        listing
    }

    /// Each path of the home that is not in `before`, and that counts:
    /// in the Claude folder, only a path that names one of `marks`. A
    /// new folder shows as one path. The paths are in order.
    pub fn new_paths(&self, before: &Listing, marks: &[String]) -> Vec<PathBuf> {
        let mut found = Vec::new();
        self.walk(&mut |path| {
            if before.contains(path) {
                return true;
            }
            let text = path.to_string_lossy();
            let counts = !path.starts_with(&self.claude)
                || marks
                    .iter()
                    .any(|mark| !mark.is_empty() && text.contains(mark.as_str()));
            if counts {
                found.push(path.to_owned());
            }
            // A new folder of another session can hold a path that names
            // this session.
            !counts
        });
        found.sort();
        found
    }

    /// Calls `visit` for each path of the home, except the paths in
    /// [`Home::skips`]. It goes into a folder when `visit` gives true,
    /// and the folder is on the file system of the home. It follows no
    /// link, and it skips a folder that it cannot read.
    fn walk(&self, visit: &mut dyn FnMut(&Path) -> bool) {
        let device = std::fs::symlink_metadata(&self.root).map(|m| m.dev()).ok();
        let mut folders = vec![self.root.clone()];
        while let Some(folder) = folders.pop() {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if self.skips.iter().any(|skip| path.starts_with(skip)) {
                    continue;
                }
                let go_in = visit(&path);
                let folder = entry.file_type().is_ok_and(|t| t.is_dir());
                if go_in && folder && entry.metadata().is_ok_and(|m| Some(m.dev()) == device) {
                    folders.push(path);
                }
            }
        }
    }
}

/// The reason of a failed home check.
///
/// ```
/// let text = real_session::new_paths_text(&["/h/.cvsignore".into()]);
/// assert!(text.starts_with("the home has 1 new path outside the folders of riff:\n  /h/.cvsignore\n"));
///
/// let many: Vec<std::path::PathBuf> = (0..22).map(|i| format!("/h/f{i}").into()).collect();
/// assert!(real_session::new_paths_text(&many).contains("\n  /h/f19\n  and 2 more\n"));
/// ```
pub fn new_paths_text(paths: &[PathBuf]) -> String {
    let noun = if paths.len() == 1 { "path" } else { "paths" };
    let mut text = format!(
        "the home has {} new {noun} outside the folders of riff:\n",
        paths.len()
    );
    for path in paths.iter().take(SHOWN) {
        text.push_str(&format!("  {}\n", path.display()));
    }
    if paths.len() > SHOWN {
        text.push_str(&format!("  and {} more\n", paths.len() - SHOWN));
    }
    text.push_str(
        "When another program wrote a path, leave out its folder with --skip DIR, \
         and run the test again.",
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

/// What the test made, for the clean-up.
#[derive(Debug, Default)]
struct Made {
    /// The scratch issue.
    issue: Option<u64>,
    /// True when the test holds `verify-issue-N`.
    held: bool,
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
        clean_up(options, issue, made, stop);
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
    made.sessions = Some(env.sessions());
    let skips: Vec<PathBuf> = options
        .skips
        .iter()
        .map(|skip| std::path::absolute(skip).unwrap_or_else(|_| skip.clone()))
        .collect();
    let home = Home {
        root: env.home.clone(),
        skips: env.skips(&clones, &skips),
        claude: env.claude.clone(),
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
    made.held = step(Step::Keep, stop, || keep_verify(&options.riff, issue, stop))?;
    step(Step::Daemon, stop, || {
        start_daemon(&options.riff, stop, &mut made.daemon)
    })?;
    let item = format!("issue-{issue}");
    step(Step::Run, stop, || {
        run_item(&options.riff, &item, &image, stop)
    })?;
    step(Step::Home, stop, || check_home(&home, &before, &env, issue))?;
    let pull = step(Step::Pull, stop, || find_pull(issue, stop))?;
    step(Step::Change, stop, || check_change(&pull, &line, stop))?;
    let gate = step(Step::Gate, stop, || {
        wait_for_gate(&pull, options.gate_wait, stop)
    })?;
    Ok(format!(
        "issue #{issue}, pull request #{} ({}), the Gate is {gate}, no new path in the home",
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

/// Runs `program ARGS`, and gives its stdout with no space at the ends.
/// An error names the command and its stderr. A child that ends by
/// SIGINT or SIGTERM sets `stop` ([`child_signal`]).
fn output(program: impl AsRef<OsStr>, args: &[&str], stop: &AtomicBool) -> Result<String, String> {
    let program = program.as_ref();
    let shown = || format!("{} {}", program.to_string_lossy(), args.join(" "));
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {}: {e}", program.to_string_lossy()))?;
    if !out.status.success() {
        child_signal(out.status, stop);
        return Err(format!(
            "{} ended with {}: {}",
            shown(),
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
    let image = output(&options.riff, &["image"], stop)?;
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
fn keep_verify(riff: &Path, issue: u64, stop: &AtomicBool) -> Result<bool, String> {
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
fn hold(riff: &Path, issue: u64, stop: &AtomicBool) -> bool {
    let item = format!("verify-issue-{issue}");
    match output(riff, &["plan", "hold", &item, HOLD_REASON], stop) {
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

/// Step 5: starts `riff daemon` of the tree in a process group of its
/// own, and puts it in `daemon`, also when the step fails. It waits until
/// `riff daemon status` answers (01M4ZFPYNGFF49WWHR8EBWSTNG).
fn start_daemon(riff: &Path, stop: &AtomicBool, daemon: &mut Option<Child>) -> Result<(), String> {
    use std::os::unix::process::CommandExt;

    if let Ok(status) = output(riff, &["daemon", "status"], stop) {
        return Err(daemon_runs_text(&status));
    }
    let child = Command::new(riff)
        .arg("daemon")
        .stdin(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("cannot run {} daemon: {e}", riff.display()))?;
    println!("  riff daemon, pid {}", child.id());
    let child = daemon.insert(child);
    let start = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            child_signal(status, stop);
            return Err(daemon_ended_text(status));
        }
        if output(riff, &["daemon", "status"], stop).is_ok() {
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

/// The reason of a failed step 5 when a riff daemon answers before the
/// start. `status` is the output of `riff daemon status`.
///
/// ```
/// let text = real_session::daemon_runs_text("pid 7");
/// assert!(text.starts_with("a riff daemon runs already:\npid 7\n"));
/// assert!(text.contains("systemctl --user stop riff"));
/// ```
pub fn daemon_runs_text(status: &str) -> String {
    format!(
        "a riff daemon runs already:\n{status}\n\
         The test runs the riff daemon of the tree. Stop the other daemon, for example with \
         systemctl --user stop riff, and run the test again."
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
fn run_item(riff: &Path, item: &str, image: &str, stop: &AtomicBool) -> Result<(), String> {
    let status = Command::new(riff)
        .args(["run", item, "--image", image])
        .stdin(Stdio::null())
        .status()
        .map_err(|e| format!("cannot run {}: {e}", riff.display()))?;
    if status.success() {
        Ok(())
    } else {
        child_signal(status, stop);
        Err(format!("riff run {item} ended with {status}"))
    }
}

/// Step 7: the home has no new path.
fn check_home(home: &Home, before: &Listing, env: &Env, issue: u64) -> Result<(), String> {
    let marks: Vec<String> = session_folders(&env.sessions(), issue)
        .into_iter()
        .map(|(id8, _)| id8)
        .collect();
    let new = home.new_paths(before, &marks);
    if new.is_empty() {
        println!("  no new path");
        Ok(())
    } else {
        Err(new_paths_text(&new))
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
/// folder of the session (01M4ZCFGCY301G2ARFMRXV7XDR).
fn clean_up(options: &Options, issue: u64, made: Made, stop: &AtomicBool) {
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
    if made.held {
        let item = format!("verify-issue-{issue}");
        report(
            &format!("free {item}"),
            output(&options.riff, &["plan", "free", &item], stop),
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
        let home = Home {
            root: root.clone(),
            skips: Vec::new(),
            claude: root.join(".claude"),
        };
        let before = home.list();
        let todos = root.join(".claude/todos");
        std::fs::create_dir_all(&todos).unwrap();
        std::fs::write(todos.join("other-agent.json"), "").unwrap();
        std::fs::write(todos.join("1a2b3c4d-agent.json"), "").unwrap();
        assert_eq!(
            home.new_paths(&before, &["1a2b3c4d".into()]),
            [todos.join("1a2b3c4d-agent.json")]
        );
        // No mark: nothing in the Claude folder counts.
        assert!(home.new_paths(&before, &[String::new()]).is_empty());
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
