//! The sandbox of a test run.
//!
//! # Design
//!
//! `riff test-run PROGRAM ARGS` runs a test run (for example
//! `cargo test`) in namespaces of its own, from the profile of the role
//! test run ([`Profile::of`] with [`Role::TestRun`]). The tool is
//! bubblewrap (`bwrap`): Ubuntu gives it the AppArmor profile
//! `bwrap-userns-restrict`, so it makes namespaces with no root also
//! when `kernel.apparmor_restrict_unprivileged_userns` is 1
//! (01M4BTG70M8MPPCNTJCJ649BW1).
//!
//! | Part | In the test run |
//! |---|---|
//! | Home | a new empty folder at the path of the home, in the temp folder ([`Run`]) |
//! | `/tmp` and `/var/tmp` | one new empty folder, in the temp folder |
//! | Files | the system, the tools and the worktree with no write, the target with write ([`args`]) |
//! | Processes | a PID namespace: the run sees only its own processes |
//! | Network | a network namespace with the loopback interface only |
//! | End | the first process of the run ends each process of the run when it ends; the run ends when riff ends |
//!
//! The run gets `TMPDIR=/tmp` and [`TEST_RUN_VAR`], and loses
//! `XDG_RUNTIME_DIR` and `SSH_AUTH_SOCK`. riff removes the folder of
//! the run at the end (01M4BTG72XPKSTDF4KYRKS4Z0D).
//!
//! Before each run, [`check`] looks for `bwrap` and makes one empty
//! sandbox. When one fails, riff runs nothing and prints one line with
//! the `sudo` command for the host ([`Missing::line`],
//! 01M4BTG755XSCBZ0F8TMFG7NVF).
//!
//! A `bwrap` in a test run fails: the stacked AppArmor profile refuses
//! a new user namespace. So a test of the sandbox runs inside the test
//! run, and [`TEST_RUN_VAR`] tells it so.
//!
//! The build is not in the sandbox: `just test` builds the tests
//! first, with the compile cache and the network, and then runs them
//! in `riff test-run` (01M4BTG77E656440W5JSGTK4E5).
//!
//! ```mermaid
//! flowchart LR
//!     J["just test"] --> B["cargo test --no-run: build, outside"]
//!     B --> R["riff test-run cargo test"]
//!     R --> C{"check: bwrap, namespaces"}
//!     C -- "missing" --> L["one line with the sudo command; exit 1"]
//!     C -- "good" --> P["Profile::of(TestRun, session here)"]
//!     P --> W["bwrap: new home, new /tmp, PID and network namespaces"]
//!     W --> T["cargo test: runs the tests"]
//! ```
//!
//! ```
//! use riff::profile::{Endpoint, Profile, Role, Session};
//! use std::path::Path;
//!
//! let session = Session {
//!     home: "/home/ada".into(),
//!     runtime: "/run/user/1000".into(),
//!     clone: "/home/ada/src/app".into(),
//!     worktree: "/home/ada/src/app".into(),
//!     target: "/home/ada/src/app/target".into(),
//!     temp: "/var/tmp".into(),
//!     claude: "/home/ada/.claude".into(),
//!     state: "/run/user/1000/riff".into(),
//!     tools: vec!["/home/ada/.cargo".into()],
//!     server: Endpoint::of_url("http://127.0.0.1:9").unwrap(),
//! };
//! let profile = Profile::of(Role::TestRun, &session)?;
//! let run = Path::new("/var/tmp/riff-test-run.x");
//! let args = riff::sandbox::args(&profile, &session, run, Path::new("/home/ada/src/app"), None);
//! let line = args.join(" ");
//! assert!(line.starts_with("--unshare-all --die-with-parent"));
//! assert!(line.contains("--bind /var/tmp/riff-test-run.x/home /home/ada "));
//! assert!(line.contains("--ro-bind-try /home/ada/src/app /home/ada/src/app "));
//! assert!(line.contains("--bind /home/ada/src/app/target /home/ada/src/app/target"));
//! assert!(line.contains("--bind /var/tmp/riff-test-run.x/tmp /tmp "));
//! # Ok::<(), riff::profile::Refused>(())
//! ```

use crate::profile::{Endpoint, Profile, Role, Session};
use anyhow::{Context, Result, bail};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The program of the sandbox.
pub const BWRAP: &str = "bwrap";

/// The AppArmor profile of Ubuntu that lets `bwrap` make namespaces.
pub const APPARMOR_PROFILE: &str = "/etc/apparmor.d/bwrap-userns-restrict";

/// The variable that tells a process that it runs in a test run. Its
/// value is `1`.
pub const TEST_RUN_VAR: &str = "RIFF_TEST_RUN";

/// The variables that a test run loses: they name the places of the
/// person outside the run.
pub const DROPPED: [&str; 2] = ["XDG_RUNTIME_DIR", "SSH_AUTH_SOCK"];

/// What a host needs before a test run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    /// No `bwrap` on the `PATH`.
    Tool,
    /// `bwrap` cannot make namespaces: AppArmor refuses them. `profile`
    /// is true when the AppArmor profile of Ubuntu is on the disk.
    Namespaces {
        /// The file [`APPARMOR_PROFILE`] is on the disk.
        profile: bool,
    },
}

impl Missing {
    /// One line: what is missing, and the `sudo` command that adds it.
    ///
    /// ```
    /// use riff::sandbox::Missing;
    ///
    /// assert!(Missing::Tool.line().contains("`sudo apt install bubblewrap`"));
    /// let load = Missing::Namespaces { profile: true }.line();
    /// assert!(load.contains("`sudo apparmor_parser -r /etc/apparmor.d/bwrap-userns-restrict`"));
    /// let install = Missing::Namespaces { profile: false }.line();
    /// assert!(install.contains("`sudo apt install apparmor && sudo apparmor_parser -r"));
    /// assert!(!load.contains('\n'));
    /// ```
    pub fn line(&self) -> String {
        let load = format!("sudo apparmor_parser -r {APPARMOR_PROFILE}");
        match self {
            Missing::Tool => "riff: a test run needs bubblewrap, and this host has no `bwrap`. \
                 Run one time on this host: `sudo apt install bubblewrap`"
                .to_owned(),
            Missing::Namespaces { profile } => {
                let command = if *profile {
                    load
                } else {
                    format!("sudo apt install apparmor && {load}")
                };
                format!(
                    "riff: AppArmor does not let bubblewrap make namespaces on this host. \
                     Run one time on this host: `{command}`"
                )
            }
        }
    }
}

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.line())
    }
}

impl std::error::Error for Missing {}

/// Checks that this host can make the sandbox of a test run: `bwrap`
/// is on the `PATH`, and it makes one empty sandbox.
pub fn check() -> Result<PathBuf, Missing> {
    check_in(std::env::var_os("PATH"), Path::new(APPARMOR_PROFILE))
}

/// [`check`] with the value of `PATH` and the path of the AppArmor
/// profile.
///
/// ```
/// use riff::sandbox::{Missing, check_in};
///
/// let none = check_in(Some("/nonexistent".into()), "/nonexistent/profile".as_ref());
/// assert_eq!(none, Err(Missing::Tool));
/// ```
pub fn check_in(path: Option<OsString>, profile: &Path) -> Result<PathBuf, Missing> {
    let bwrap = path
        .iter()
        .flat_map(std::env::split_paths)
        .map(|dir| dir.join(BWRAP))
        .find(|p| p.is_file())
        .ok_or(Missing::Tool)?;
    let works = Command::new(&bwrap)
        .args(["--unshare-all", "--ro-bind", "/", "/", "true"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if works {
        Ok(bwrap)
    } else {
        Err(Missing::Namespaces {
            profile: profile.is_file(),
        })
    }
}

/// The session of a test run in `dir`, from the environment and from
/// git: the worktree is the top of the git worktree of `dir` (else
/// `dir`), the clone is the folder of its common git dir, the target is
/// `CARGO_TARGET_DIR` (else the `target` of the worktree), and the temp
/// folder is the temp folder of the process.
pub fn here(dir: &Path) -> Result<Session> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
    let home = PathBuf::from(var("HOME").context("riff test-run needs HOME")?);
    let worktree = git(dir, &["rev-parse", "--show-toplevel"]).unwrap_or_else(|| dir.to_owned());
    let common = git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    let clone = common
        .as_deref()
        .and_then(Path::parent)
        .map_or_else(|| worktree.clone(), Path::to_owned);
    let target = var("CARGO_TARGET_DIR").map_or_else(|| worktree.join("target"), PathBuf::from);
    let target = if target.is_absolute() {
        target
    } else {
        dir.join(target)
    };
    let runtime = var("XDG_RUNTIME_DIR").map_or_else(
        || {
            use std::os::unix::fs::MetadataExt;
            let uid = std::fs::metadata("/proc/self").map_or(0, |m| m.uid());
            PathBuf::from(format!("/run/user/{uid}"))
        },
        PathBuf::from,
    );
    let tools = vec![
        var("CARGO_HOME").map_or_else(|| home.join(".cargo"), PathBuf::from),
        var("RUSTUP_HOME").map_or_else(|| home.join(".rustup"), PathBuf::from),
    ];
    let server = var("RIFF_SERVER")
        .and_then(|s| Endpoint::of_url(&s.to_string_lossy()))
        .or_else(|| Endpoint::of_url("http://127.0.0.1:9"))
        .context("no endpoint")?;
    Ok(Session {
        claude: var("CLAUDE_CONFIG_DIR").map_or_else(|| home.join(".claude"), PathBuf::from),
        state: crate::local::dir().unwrap_or_else(|| runtime.join("riff")),
        temp: std::env::temp_dir(),
        home,
        runtime,
        clone,
        worktree,
        target,
        tools,
        server,
    })
}

fn git(dir: &Path, args: &[&str]) -> Option<PathBuf> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let line = String::from_utf8(out.stdout).ok()?;
    let line = line.trim();
    (!line.is_empty()).then(|| PathBuf::from(line))
}

/// The arguments of `bwrap` for a test run of `profile`, with the run
/// folder `run` (it holds `home` and `tmp`), the working folder `cwd`,
/// and the folder of the pipe of the pool of build jobs, when there is
/// one. The arguments end before the program.
///
/// - The run has new namespaces: user, PID, network, IPC, UTS and
///   cgroup. It dies with riff, in a new session.
/// - Each read path of the profile, with no write. Each write path,
///   with write, except the temp folder: the run writes only its run
///   folder there. `run/home` is at the path of the home.
/// - A parent comes before its children, so a child path is not under
///   a later bind. `/tmp` and `/var/tmp` come last.
///
/// ```
/// use riff::profile::{Endpoint, Profile, Role, Session};
/// use std::path::Path;
///
/// let session = Session {
///     home: "/h".into(),
///     runtime: "/run/user/7".into(),
///     clone: "/h/app".into(),
///     worktree: "/h/app/.claude/worktrees/w".into(),
///     target: "/h/app/.claude/worktrees/w/target".into(),
///     temp: "/h/.cache/tmp".into(),
///     claude: "/h/.claude".into(),
///     state: "/run/user/7/riff".into(),
///     tools: vec!["/h/.cargo".into()],
///     server: Endpoint::of_url("http://127.0.0.1:9").unwrap(),
/// };
/// let profile = Profile::of(Role::TestRun, &session)?;
/// let args = riff::sandbox::args(
///     &profile,
///     &session,
///     Path::new("/h/.cache/tmp/r"),
///     Path::new("/h/app/.claude/worktrees/w"),
///     Some(Path::new("/run/riff/jobs")),
/// );
/// let at = |s: &str| args.iter().position(|a| a == s).unwrap();
/// // The home comes before the tools, the worktree before its target.
/// assert!(at("/h/.cache/tmp/r/home") < at("/h/.cargo"));
/// assert!(at("/h/app/.claude/worktrees/w") < at("/h/app/.claude/worktrees/w/target"));
/// // The run reads the git dir of the clone, and writes the pool.
/// assert!(args.windows(3).any(|w| w == ["--ro-bind-try", "/h/app/.git", "/h/app/.git"]));
/// assert!(args.windows(3).any(|w| w == ["--bind", "/run/riff/jobs", "/run/riff/jobs"]));
/// // No bind gives the temp folder as a whole.
/// assert!(!args.iter().any(|a| a == "/h/.cache/tmp"));
/// assert_eq!(args[args.len() - 2..], ["--chdir", "/h/app/.claude/worktrees/w"]);
/// # Ok::<(), riff::profile::Refused>(())
/// ```
pub fn args(
    profile: &Profile,
    session: &Session,
    run: &Path,
    cwd: &Path,
    pool: Option<&Path>,
) -> Vec<String> {
    #[derive(Clone, Copy)]
    enum Mount {
        Read,
        Write,
        Home,
    }
    let mut mounts: Vec<(PathBuf, Mount)> = vec![(session.home.clone(), Mount::Home)];
    mounts.extend(profile.read_paths().map(|p| (p.to_owned(), Mount::Read)));
    mounts.extend(
        profile
            .write_paths()
            .filter(|p| *p != session.temp)
            .map(|p| (p.to_owned(), Mount::Write)),
    );
    mounts.extend(pool.map(|p| (p.to_owned(), Mount::Write)));
    mounts.sort_by_key(|(p, _)| p.components().count());

    let text = |p: &Path| p.display().to_string();
    let mut args: Vec<String> = ["--unshare-all", "--die-with-parent", "--new-session"]
        .map(String::from)
        .to_vec();
    for (path, mount) in mounts {
        let path = text(&path);
        match (mount, path.as_str()) {
            (_, "/dev") => args.extend(["--dev".into(), path]),
            (_, "/proc") => args.extend(["--proc".into(), path]),
            (Mount::Read, _) => args.extend(["--ro-bind-try".into(), path.clone(), path]),
            (Mount::Write, _) => args.extend(["--bind".into(), path.clone(), path]),
            (Mount::Home, _) => args.extend(["--bind".into(), text(&run.join("home")), path]),
        }
    }
    let tmp = text(&run.join("tmp"));
    for at in ["/tmp", "/var/tmp"] {
        args.extend(["--bind".into(), tmp.clone(), at.into()]);
    }
    args.extend(["--setenv".into(), "TMPDIR".into(), "/tmp".into()]);
    args.extend(["--setenv".into(), TEST_RUN_VAR.into(), "1".into()]);
    for name in DROPPED {
        args.extend(["--unsetenv".into(), name.into()]);
    }
    args.extend(["--chdir".into(), text(cwd)]);
    args
}

/// A run folder in the temp folder: `home` and `tmp`, both empty. riff
/// removes it when the run ends.
pub struct Run(tempfile::TempDir);

impl Run {
    /// Makes a new run folder in `temp`.
    pub fn new(temp: &Path) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("riff-test-run.")
            .tempdir_in(temp)
            .with_context(|| format!("make a run folder in {}", temp.display()))?;
        for part in ["home", "tmp"] {
            std::fs::create_dir(dir.path().join(part))?;
        }
        Ok(Self(dir))
    }

    /// The path of the run folder.
    pub fn path(&self) -> &Path {
        self.0.path()
    }
}

/// `riff test-run PROGRAM ARGS`: runs PROGRAM in the sandbox of a test
/// run, in the current folder. Returns its exit code: the code of the
/// program, or 128 and the signal.
pub fn test_run(program: &OsStr, program_args: &[OsString]) -> Result<i32> {
    let bwrap = check()?;
    let cwd = std::env::current_dir()?;
    let session = here(&cwd)?;
    let profile = Profile::of(Role::TestRun, &session)?;
    std::fs::create_dir_all(&session.target)
        .with_context(|| format!("make the target {}", session.target.display()))?;
    let run = Run::new(&session.temp)?;
    let pool = std::env::var("MAKEFLAGS")
        .ok()
        .and_then(|m| crate::jobserver::fifo_of(&m))
        .and_then(|f| f.parent().map(Path::to_owned));
    let status = Command::new(bwrap)
        .args(args(&profile, &session, run.path(), &cwd, pool.as_deref()))
        .arg("--")
        .arg(program)
        .args(program_args)
        .status()
        .context("start bwrap")?;
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => Ok(code),
        (None, Some(signal)) => Ok(128 + signal),
        (None, None) => bail!("bwrap ended with no code and no signal"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session {
            home: "/home/ada".into(),
            runtime: "/run/user/1000".into(),
            clone: "/home/ada/src/app".into(),
            worktree: "/home/ada/src/app/.claude/worktrees/issue-12".into(),
            target: "/home/ada/src/app/.claude/worktrees/issue-12/target".into(),
            temp: "/var/tmp".into(),
            claude: "/home/ada/.claude".into(),
            state: "/run/user/1000/riff".into(),
            tools: vec!["/home/ada/.cargo".into(), "/home/ada/.rustup".into()],
            server: Endpoint::of_url("http://127.0.0.1:9").unwrap(),
        }
    }

    fn of(pool: Option<&Path>) -> Vec<String> {
        let s = session();
        let p = Profile::of(Role::TestRun, &s).unwrap();
        args(&p, &s, Path::new("/var/tmp/r"), &s.worktree, pool)
    }

    #[test]
    fn a_test_run_has_each_namespace_and_dies_with_riff() {
        let a = of(None);
        assert_eq!(
            a[..3],
            ["--unshare-all", "--die-with-parent", "--new-session"]
        );
        assert!(!a.iter().any(|x| x.starts_with("--share")), "{a:?}");
    }

    #[test]
    fn the_home_and_the_tmp_are_new_and_the_secrets_are_gone() {
        let a = of(None);
        let line = a.join(" ");
        assert!(line.contains("--bind /var/tmp/r/home /home/ada "), "{line}");
        assert!(line.contains("--bind /var/tmp/r/tmp /tmp --bind /var/tmp/r/tmp /var/tmp"));
        for secret in session().secrets() {
            assert!(!a.contains(&secret.display().to_string()), "{secret:?}");
        }
        for gone in ["/home/ada/.claude", "/run/user/1000/riff", "/run/user/1000"] {
            assert!(!a.iter().any(|x| x == gone), "{gone}");
        }
        assert!(line.contains("--unsetenv XDG_RUNTIME_DIR --unsetenv SSH_AUTH_SOCK"));
        assert!(line.contains("--setenv TMPDIR /tmp --setenv RIFF_TEST_RUN 1"));
    }

    #[test]
    fn the_run_reads_the_worktree_and_writes_only_the_target() {
        let a = of(None);
        let line = a.join(" ");
        let wt = "/home/ada/src/app/.claude/worktrees/issue-12";
        assert!(line.contains(&format!("--ro-bind-try {wt} {wt} ")));
        assert!(line.contains(&format!("--bind {wt}/target {wt}/target")));
        let writes = a.iter().filter(|x| x.starts_with("--bind")).count();
        assert_eq!(writes, 4, "home, target, /tmp, /var/tmp: {a:?}");
        assert!(line.contains("--dev /dev") && line.contains("--proc /proc"));
    }

    #[test]
    fn the_pool_of_build_jobs_is_writable_in_the_run() {
        let a = of(Some(Path::new("/run/user/1000/riff/jobs")));
        assert!(
            a.windows(3).any(|w| w
                == [
                    "--bind",
                    "/run/user/1000/riff/jobs",
                    "/run/user/1000/riff/jobs"
                ]),
            "{a:?}"
        );
    }

    #[test]
    fn a_run_folder_has_an_empty_home_and_tmp_and_goes_at_the_end() {
        let temp = tempfile::tempdir().unwrap();
        let run = Run::new(temp.path()).unwrap();
        let path = run.path().to_owned();
        for part in ["home", "tmp"] {
            assert_eq!(std::fs::read_dir(path.join(part)).unwrap().count(), 0);
        }
        drop(run);
        assert!(!path.exists());
    }

    #[test]
    fn here_finds_the_worktree_and_the_clone_of_a_git_dir() {
        let temp = tempfile::tempdir().unwrap();
        let top = temp.path().canonicalize().unwrap();
        let ok = |args: &[&str], dir: &Path| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(dir)
                    .output()
                    .unwrap()
                    .status
                    .success()
            )
        };
        ok(&["init", "-q"], &top);
        std::fs::create_dir(top.join("src")).unwrap();
        let s = here(&top.join("src")).unwrap();
        assert_eq!(s.worktree, top);
        assert_eq!(s.clone, top);
    }
}
