//! The first run of riff on a machine (#721): `riff init`
//! (01M4ZJN6R68YBD3XMM725ZRJ7W).
//!
//! # Design
//!
//! `riff init` gets a machine ready for `riff run`. It needs no
//! riff-server and no sign-in. It does these steps, in order:
//!
//! ```mermaid
//! flowchart LR
//!     C["the checks<br/>podman, its init, subuid, subgid,<br/>cgroup v2, riff.toml,<br/>the riff binary,<br/>the repository on GitHub"] --> X{"wide<br/>rules?"}
//!     X -- "one question:<br/>y in a terminal" --> V["remove them"]
//!     X -- "no" --> G{"steps of the<br/>repository?"}
//!     V --> G
//!     G -- "one question:<br/>y in a terminal, or --yes" --> T["set up the<br/>repository on GitHub"]
//!     G -- "none, or no" --> E{"each check<br/>passed?"}
//!     T --> P["git pull --ff-only<br/>in the clone, when it is safe"]
//!     P --> A["the checks again"]
//!     A --> E
//!     E -- "no" --> F["the name and the fix of<br/>each check that fails, exit 1"]
//!     E -- "yes" --> D["the daemon<br/>riff.service"]
//!     D --> I["the session image<br/>the daemon pulls it"]
//!     I --> R["ready for riff run"]
//! ```
//!
//! 1. The checks ([`checks`]). riff reads the [`Facts`] of the machine,
//!    and prints one line for each check: `pass`, or `fail` with the
//!    fix (01M4ZJN6TEWCYCB39XTH6BBD9H). When riff can do the step of a
//!    failed check now (step 3), the line reads `step` with the step, in
//!    place of `fail` with the fix ([`Check::planned_line`],
//!    01M5RGG9KJXCKH8C94145WS8SM).
//! 2. The wide rules ([`Wide`]). When the user settings, the project
//!    settings or the local settings of Claude Code have an allow rule
//!    that is wider than the steps of riff ([`crate::permissions::wide`]),
//!    riff names each rule with its file (01M51JCW04FAHZDQ144KDNCCPX).
//!    Then it asks one question ([`Question::Remove`]). Only a `y` in a
//!    terminal removes them ([`crate::permissions::remove`],
//!    01M51JCW2EKF8H8QFQYSVRSPET). `--yes` and `--check` ask nothing and
//!    remove nothing. A wide rule fails no check.
//! 3. The setup of the repository (01M5R1672NF56P15VD0JEX51K4).
//!    When a check of the repository on GitHub failed and has a step
//!    ([`crate::repo_setup::Step`]), and the person is an admin of the
//!    repository, riff names the steps and asks one question
//!    ([`Question::Setup`]). Only a `y` in a terminal does them. `--yes`
//!    does them with no question. `--check` does none. A person who is
//!    not an admin gets the fix of each check, and riff changes nothing
//!    there. After the steps, when the step of `riff.toml` merged the
//!    file and the clone has none, riff runs `git pull --ff-only` in the
//!    clone if it is on the default branch and has no change ([`pull`],
//!    01M5RGGD9AYNC12Z2CG5CDZWNA). Else it says why it did not, and the
//!    image of the release is the session image of this run. Then riff
//!    checks again and prints the list again: a step that worked passes
//!    its check (01M5RGGD6YR36EXZEG61XCJVRY).
//! 4. When a check still fails, riff installs nothing, pulls no image,
//!    names the check and the fix of each failed check, and exits with 1.
//!    `--check` stops here (01M4ZJN6WR0JJ891R9JMQNPKGD).
//! 5. The daemon. When a riff daemon answers, riff keeps it: it does
//!    not start it again. When no daemon answers, riff installs it as
//!    `riff daemon install` does ([`crate::daemon::install`],
//!    01M4ZJN6Z1DGD43CKZ19QPN2NK).
//! 6. The image. riff asks the daemon for the session image that
//!    `riff.toml` pins. The daemon pulls it when podman does not have
//!    it (01M4ZJN718EF1PC9YY9GFD0EJ2).
//!
//! # The checks
//!
//! | Check | riff reads | Pass |
//! |---|---|---|
//! | podman | `podman --version`, and the user ID | it runs, and riff does not run as root: so podman runs rootless |
//! | the init of podman | `catatonit` in the helper folders of podman ([`INIT_DIRS`]) | it is there: `podman run --init` of each session needs it (01M511JYPZ8H73DCJ7867YSFGH) |
//! | `/etc/subuid` | the lines of the person, by name or by user ID | [`MIN_IDS`] IDs or more, added over the lines |
//! | `/etc/subgid` | the same | the same |
//! | cgroup v2 | `/sys/fs/cgroup/cgroup.controllers`, and `cgroup.controllers` of `user@UID.service` | the file is there, and the controllers of the user have `cpu` and `memory` |
//! | the session image | `riff.toml` of the repository of the current folder ([`crate::session_image::of_folder`]) | it pins an image by its digest |
//! | the riff binary | the ELF of the riff of this process ([`crate::release::check_file`]) | it has no interpreter, so `riff relay` starts in each session image. The fix is `riff update` (01M510NW68CTZBHHW7ZYFV7Q3X) |
//! | `riff.toml` on the default branch | the files at the root of the default branch ([`crate::repo_setup`], 01M5QVF5535YRN1EF3DZAB4W8G) | it is there |
//! | auto-merge and squash merge | `allow_auto_merge` and `allow_squash_merge` of the repository on GitHub | both are on |
//! | the ruleset of the default branch | the active rules of the default branch | one requires the check `riff/verify` |
//! | the milestone `Backlog` | the open milestones | one has that title |
//! | the first wave | the open milestones | one is a wave |
//! | the checks of a pull request | the required checks and the check runs of the latest pull requests | the full check can end |
//!
//! The first five checks of the repository have a step that `riff init`
//! can do. The check of the session image has the step of `riff.toml` when
//! the default branch has none. The check of a pull request has the step of
//! the ruleset when no pull request ran a check yet: else its fix is the
//! work of the person. `riff init` reads no file of rules for agents, such
//! as `CLAUDE.md` or `AGENTS.md` (01M5RN89Z749VYS1CRYJQTKAWY).
//!
//! The fix of `/etc/subuid` or `/etc/subgid` adds a free range of
//! [`MIN_IDS`] IDs from [`free_sub_id`].
//!
//! The last six checks run only in a folder whose remote `origin` is on
//! GitHub, with the `gh` of the machine. They read the repository and
//! change nothing, also without `--check`; only the steps of the setup
//! change it. When `gh` cannot read the
//! repository, one check fails and its fix is `gh auth login`
//! ([`crate::repo_setup`]).
//!
//! riff reads the controllers of `user@UID.service`, not `podman info`:
//! a process in a scope that gives no `cpu` to its children still gets
//! containers with `cpu.max` (01M4Z60B7MW963MBQAPN2CJEQZ).
//!
//! ```
//! use riff::first_run::{Facts, checks};
//!
//! let mut facts = Facts {
//!     podman: Ok("podman version 5.7.0".into()),
//!     init: Some("/usr/libexec/podman/catatonit".into()),
//!     user: "mike".into(),
//!     uid: 1000,
//!     subuid: Some("mike:100000:65536\n".into()),
//!     subgid: Some("1000:100000:65536\n".into()),
//!     cgroup: Some("cpuset cpu io memory pids\n".into()),
//!     user_cgroup: Some("cpu memory pids\n".into()),
//!     image: Ok(format!("ghcr.io/o/i@sha256:{}", "0".repeat(64))),
//!     wide: vec![],
//!     riff: Ok("/home/mike/.cargo/bin/riff is static".into()),
//!     github: None,
//!     folder: "/home/mike/riff".into(),
//! };
//! assert!(checks(&facts).iter().all(|c| c.passed()));
//!
//! facts.user_cgroup = Some("memory pids\n".into());
//! let failed: Vec<String> = checks(&facts)
//!     .iter()
//!     .filter(|c| !c.passed())
//!     .map(|c| c.line())
//!     .collect();
//! assert_eq!(failed.len(), 1);
//! assert!(failed[0].starts_with("fail  cgroup v2: user@1000.service has no cpu. Fix: "), "{}", failed[0]);
//! assert!(failed[0].contains("Delegate=cpu cpuset io memory pids"), "{}", failed[0]);
//!
//! // No init of podman: no session can start (01M511JYPZ8H73DCJ7867YSFGH).
//! facts.user_cgroup = Some("cpu memory pids\n".into());
//! facts.init = None;
//! let failed: Vec<String> = checks(&facts)
//!     .iter()
//!     .filter(|c| !c.passed())
//!     .map(|c| c.line())
//!     .collect();
//! assert_eq!(failed.len(), 1);
//! assert!(failed[0].starts_with("fail  the init of podman: "), "{}", failed[0]);
//! assert!(failed[0].ends_with("Fix: install catatonit, the init of podman, for example: sudo apt install catatonit"), "{}", failed[0]);
//! ```
//!
//! For tests, [`ETC_VAR`] names a folder that riff reads in place of
//! `/etc`, [`CGROUP_VAR`] a folder in place of `/sys/fs/cgroup`, and
//! [`INIT_VAR`] a folder in place of the folders of [`INIT_DIRS`].

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use riff_core::build::Build;

use crate::permissions::{self, Rules};
use crate::repo_setup::{self, Forge};
use crate::service::{ServiceManager, Unit};
use crate::{daemon, session_image, text};

/// The IDs that `/etc/subuid` and `/etc/subgid` must each give the
/// person: the size of the range that `useradd` gives.
pub const MIN_IDS: u64 = 65536;

/// The lowest first ID of the range that the fix of `/etc/subuid` or
/// `/etc/subgid` adds: the first ID that `useradd` gives.
pub const FIRST_SUB_ID: u64 = 100000;

/// The variable that names a folder to read in place of `/etc`, for
/// tests: its files `subuid` and `subgid`.
pub const ETC_VAR: &str = "RIFF_ETC";

/// The variable that names a folder to read in place of
/// `/sys/fs/cgroup`, for tests.
pub const CGROUP_VAR: &str = "RIFF_CGROUP";

/// The variable that names a folder to look in for `catatonit` in
/// place of [`INIT_DIRS`], for tests.
pub const INIT_VAR: &str = "RIFF_PODMAN_INIT";

/// The init of podman: `podman run --init` runs it as PID 1
/// (01M51117K3WXK35CE67KPRFMA8).
pub const INIT: &str = "catatonit";

/// The helper folders where podman looks for [`INIT`]: the defaults of
/// `helper_binaries_dir` of `containers.conf`.
pub const INIT_DIRS: [&str; 4] = [
    "/usr/local/libexec/podman",
    "/usr/local/lib/podman",
    "/usr/libexec/podman",
    "/usr/lib/podman",
];

/// The time that riff waits for a new daemon to answer.
pub const WAIT: Duration = Duration::from_secs(10);

/// The facts of a machine that the first run checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// The first line of `podman --version`, or why riff cannot run
    /// podman.
    pub podman: Result<String, String>,
    /// The init of podman ([`INIT`]) that riff found, or `None`.
    pub init: Option<PathBuf>,
    /// The login name of the person.
    pub user: String,
    /// The user ID of the person.
    pub uid: u32,
    /// The text of `/etc/subuid`, or `None` when riff cannot read it.
    pub subuid: Option<String>,
    /// The text of `/etc/subgid`, or `None` when riff cannot read it.
    pub subgid: Option<String>,
    /// The controllers of the root of cgroup v2, or `None` when the
    /// file is not there: cgroup v1.
    pub cgroup: Option<String>,
    /// The controllers of `user@UID.service`, or `None` when riff
    /// cannot read them.
    pub user_cgroup: Option<String>,
    /// The session image that `riff.toml` pins, or why riff has none.
    pub image: Result<String, String>,
    /// The settings files of Claude Code with allow rules that are wider
    /// than the steps of riff.
    pub wide: Vec<Wide>,
    /// What riff found of its own binary: static, or why it is not
    /// ([`crate::release::check_file`]).
    pub riff: Result<String, String>,
    /// The facts of the repository on GitHub that riff reads
    /// ([`crate::repo_setup`]), or why it cannot. `None` when the folder
    /// has no remote `origin` on GitHub: riff checks nothing there.
    pub github: Option<Result<crate::repo_setup::Github, String>>,
    /// The folder that riff checks: the clone, where `git pull` runs
    /// after the step of `riff.toml` (01M5RGGD9AYNC12Z2CG5CDZWNA).
    pub folder: PathBuf,
}

/// The home of the person who runs riff.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// A settings file of Claude Code, and its allow rules that are wider
/// than the steps of riff (01M51JCW04FAHZDQ144KDNCCPX).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wide {
    /// The file.
    pub path: PathBuf,
    /// Its wide allow rules ([`permissions::wide`]).
    pub rules: Rules,
}

impl Wide {
    /// The settings files of `paths` that have wide allow rules, for the
    /// person with the home `home`, in the order of `paths`. riff reads a
    /// file once, also when two paths name it. A file that is not there,
    /// or that is not JSON, has none.
    ///
    /// ```
    /// use riff::first_run::Wide;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let (user, local) = (dir.path().join("user.json"), dir.path().join("local.json"));
    /// std::fs::write(&user, r#"{"permissions": {"allow": ["Bash(riff *)", "Bash(git push *)"]}}"#)?;
    /// let wide = Wide::of(&[user.clone(), user.clone(), local], None);
    /// assert_eq!(wide.len(), 1);
    /// assert_eq!(wide[0].path, user);
    /// assert_eq!(wide[0].rules.allow, ["Bash(git push *)"]);
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn of(paths: &[PathBuf], home: Option<&Path>) -> Vec<Wide> {
        let facts = permissions::Facts {
            home,
            ..permissions::Facts::default()
        };
        let mut seen: Vec<PathBuf> = Vec::new();
        let mut out = Vec::new();
        for path in paths {
            let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            if seen.contains(&real) {
                continue;
            }
            seen.push(real);
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let rules = permissions::wide_rules(&text, &facts);
            if !rules.is_empty() {
                out.push(Wide {
                    path: path.clone(),
                    rules,
                });
            }
        }
        out
    }

    /// The settings of the person who runs riff in the folder `dir`: the
    /// user settings, then the project settings and the local settings of
    /// the repository of `dir`.
    fn here(dir: &Path) -> Vec<Wide> {
        let project = permissions::Project::of(dir);
        let paths: Vec<PathBuf> = crate::claude_settings::user_settings()
            .into_iter()
            .chain([project.settings(), project.local_settings()])
            .collect();
        Wide::of(&paths, home().as_deref())
    }
}

/// The folder in the variable `var`, else `default`.
fn dir_of(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from(default), PathBuf::from)
}

/// The first [`INIT`] in the folders `dirs` that is a file that runs.
///
/// ```
/// use std::os::unix::fs::PermissionsExt;
///
/// let (a, b) = (tempfile::tempdir()?, tempfile::tempdir()?);
/// let dirs = [a.path().to_owned(), b.path().to_owned()];
/// assert_eq!(riff::first_run::find_init(&dirs), None);
/// let init = b.path().join("catatonit");
/// std::fs::write(&init, "")?;
/// assert_eq!(riff::first_run::find_init(&dirs), None, "it does not run");
/// std::fs::set_permissions(&init, std::fs::Permissions::from_mode(0o755))?;
/// assert_eq!(riff::first_run::find_init(&dirs), Some(init));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn find_init(dirs: &[PathBuf]) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    dirs.iter().map(|dir| dir.join(INIT)).find(|path| {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    })
}

/// The first line of `podman --version`, or why it did not run.
fn podman_version() -> Result<String, String> {
    let out = std::process::Command::new("podman")
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .map_err(|_| text::INIT_NO_PODMAN.to_owned())?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout.lines().next().unwrap_or_default().trim();
    if !out.status.success() || first.is_empty() {
        return Err(text::init_podman_failed(
            String::from_utf8_lossy(&out.stderr).trim(),
        ));
    }
    Ok(first.to_owned())
}

impl Facts {
    /// The facts of this machine, with the repository of the folder
    /// `dir`.
    pub fn here(dir: &Path) -> Facts {
        let etc = dir_of(ETC_VAR, "/etc");
        let cgroup = dir_of(CGROUP_VAR, "/sys/fs/cgroup");
        let uid = nix::unistd::getuid();
        let user = nix::unistd::User::from_uid(uid)
            .ok()
            .flatten()
            .map(|u| u.name)
            .or_else(|| std::env::var("USER").ok().filter(|u| !u.is_empty()))
            .unwrap_or_else(|| uid.to_string());
        let uid = uid.as_raw();
        let read = |path: PathBuf| std::fs::read_to_string(path).ok();
        let init_dirs: Vec<PathBuf> = match std::env::var_os(INIT_VAR).filter(|v| !v.is_empty()) {
            Some(dir) => vec![PathBuf::from(dir)],
            None => INIT_DIRS.iter().map(PathBuf::from).collect(),
        };
        Facts {
            podman: podman_version(),
            init: find_init(&init_dirs),
            user,
            uid,
            subuid: read(etc.join("subuid")),
            subgid: read(etc.join("subgid")),
            cgroup: read(cgroup.join("cgroup.controllers")),
            user_cgroup: read(user_cgroup(&cgroup, uid)),
            image: crate::session_image::of_folder(dir)
                .map(|image| image.to_string())
                .map_err(|e| format!("{e:#}")),
            wide: Wide::here(dir),
            riff: riff_binary(),
            github: crate::repo_setup::here(dir),
            folder: dir.to_owned(),
        }
    }
}

/// What riff finds of the riff binary of this process
/// (01M510NW68CTZBHHW7ZYFV7Q3X): `Ok` when it is static, or when
/// [`crate::release::DYNAMIC`] takes it, else `Err` with why it is not.
/// The daemon of `riff init` runs this binary, and binds it into each
/// session.
fn riff_binary() -> Result<String, String> {
    let path = crate::binary::this_on_disk()
        .map_err(|_| text::init_cannot_read("the path of its own binary"))?;
    match crate::release::check_file(&path) {
        Ok(()) => Ok(text::init_riff_static(&path)),
        Err(_) if crate::release::takes_dynamic() => Ok(text::init_riff_taken(&path)),
        Err(why) => Err(text::init_riff_not_static(&path, &why)),
    }
}

/// The `cgroup.controllers` of `user@UID.service` in the root `root` of
/// cgroup v2.
///
/// ```
/// use std::path::Path;
///
/// assert_eq!(
///     riff::first_run::user_cgroup(Path::new("/sys/fs/cgroup"), 1000),
///     Path::new("/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/cgroup.controllers")
/// );
/// ```
pub fn user_cgroup(root: &Path, uid: u32) -> PathBuf {
    root.join(format!(
        "user.slice/user-{uid}.slice/user@{uid}.service/cgroup.controllers"
    ))
}

/// The IDs that the text `text` of `/etc/subuid` or `/etc/subgid` gives
/// the person `user` with the user ID `uid`: the sum of the counts of
/// each line that names the person, by name or by user ID.
///
/// ```
/// use riff::first_run::sub_ids;
///
/// let text = "# a comment\nbrett:165536:65536\nmike:100000:65536\n1000:300000:10\n";
/// assert_eq!(sub_ids(text, "mike", 1000), 65546);
/// assert_eq!(sub_ids(text, "brett", 1001), 65536);
/// assert_eq!(sub_ids(text, "nate", 1002), 0);
/// assert_eq!(sub_ids("mike:100000:many\nmike:1\n", "mike", 1000), 0);
/// ```
pub fn sub_ids(text: &str, user: &str, uid: u32) -> u64 {
    let uid = uid.to_string();
    text.lines()
        .filter_map(|line| {
            let mut parts = line.trim().split(':');
            let (name, _start, count) = (parts.next()?, parts.next()?, parts.next()?);
            (name == user || name == uid)
                .then(|| count.trim().parse::<u64>().ok())
                .flatten()
        })
        .sum()
}

/// The first ID of a free range in the text `text` of `/etc/subuid` or
/// `/etc/subgid`: the first ID after the end of each range in the text,
/// and [`FIRST_SUB_ID`] or more (01M501K21EK8F8N9NPR4565FVP).
///
/// ```
/// use riff::first_run::free_sub_id;
///
/// assert_eq!(free_sub_id(""), 100000);
/// assert_eq!(free_sub_id("brett:100000:65536\n"), 165536);
/// assert_eq!(free_sub_id("# a comment\nbrett:300000:65536\nnate:100000:65536\n"), 365536);
/// assert_eq!(free_sub_id("root:1000:10\nbad:many:10\n"), 100000);
/// ```
pub fn free_sub_id(text: &str) -> u64 {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.trim().split(':');
            let (_name, start, count) = (parts.next()?, parts.next()?, parts.next()?);
            let start = start.trim().parse::<u64>().ok()?;
            let count = count.trim().parse::<u64>().ok()?;
            start.checked_add(count)
        })
        .fold(FIRST_SUB_ID, u64::max)
}

/// One check of the first run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// What riff checked, for example `podman`.
    pub name: String,
    /// What riff found.
    pub found: String,
    /// The fix, when the check failed.
    pub fix: Option<String>,
    /// The step that does the fix, when the check failed and riff has the
    /// code of the step ([`crate::repo_setup::Step`]).
    pub step: Option<crate::repo_setup::Step>,
}

impl Check {
    pub(crate) fn pass(name: &str, found: String) -> Check {
        Check {
            name: name.into(),
            found,
            fix: None,
            step: None,
        }
    }

    pub(crate) fn fail(name: &str, found: String, fix: String) -> Check {
        Check {
            name: name.into(),
            found,
            fix: Some(fix),
            step: None,
        }
    }

    /// The same check, with the step that does its fix.
    pub(crate) fn with_step(self, step: crate::repo_setup::Step) -> Check {
        Check {
            step: Some(step),
            ..self
        }
    }

    /// True when the check passed.
    pub fn passed(&self) -> bool {
        self.fix.is_none()
    }

    /// The line for people: `pass  NAME: FOUND`, or
    /// `fail  NAME: FOUND. Fix: FIX`.
    pub fn line(&self) -> String {
        match &self.fix {
            None => text::init_pass(&self.name, &self.found),
            Some(fix) => text::init_fail(&self.name, &self.found, fix),
        }
    }

    /// The line for people before the steps of the setup: as [`Check::line`],
    /// but a failed check that has a step reads `step  NAME: FOUND. Step:
    /// STEP` for the repository `github` (01M5RGG9KJXCKH8C94145WS8SM).
    ///
    /// ```
    /// use riff::first_run::Check;
    /// use riff::repo_setup::{Github, checks};
    ///
    /// let github = Github {
    ///     repo: "acme/app".into(),
    ///     default_branch: "main".into(),
    ///     auto_merge: true,
    ///     squash_merge: true,
    ///     required: vec!["riff/verify".into()],
    ///     milestones: vec!["Backlog".into()],
    ///     admin: true,
    ///     files: vec!["riff.toml".into()],
    ///     ci: vec!["lint".into()],
    /// };
    /// let lines: Vec<String> = checks(&github)
    ///     .iter()
    ///     .filter(|c| !c.passed())
    ///     .map(|c| c.planned_line(&github))
    ///     .collect();
    /// assert_eq!(
    ///     lines,
    ///     [
    ///         "step  the first wave: acme/app has no open wave. Step: make the milestone Wave 1",
    ///         "fail  the checks of a pull request: no pull request of acme/app ran a check named Gate, and no rule requires another check, so the full check never ends. Fix: add a workflow that runs on pull_request, with a job named Gate. See \"Add the job Gate\" in the book. Then run riff init again",
    ///     ]
    /// );
    /// ```
    pub fn planned_line(&self, github: &crate::repo_setup::Github) -> String {
        match (&self.fix, self.step) {
            (Some(_), Some(step)) => {
                text::init_planned(&self.name, &self.found, &step.title(github))
            }
            _ => self.line(),
        }
    }
}

/// The check of `/etc/subuid` (`kind` is `uid`) or `/etc/subgid`
/// (`kind` is `gid`) with its text `text`.
fn sub_check(kind: &str, text: Option<&str>, facts: &Facts) -> Check {
    let name = format!("/etc/sub{kind}");
    let fix = || {
        let start = text.map_or(FIRST_SUB_ID, free_sub_id);
        text::init_fix_sub_ids(kind, &facts.user, start, MIN_IDS)
    };
    let Some(text) = text else {
        return Check::fail(&name, text::init_cannot_read(&name), fix());
    };
    let ids = sub_ids(text, &facts.user, facts.uid);
    if ids >= MIN_IDS {
        Check::pass(&name, text::init_ids(ids, &facts.user))
    } else {
        Check::fail(&name, text::init_too_few_ids(ids, &facts.user), fix())
    }
}

/// The check of cgroup v2 and of the controllers of the user.
fn cgroup_check(facts: &Facts) -> Check {
    const NAME: &str = "cgroup v2";
    let unit = format!("user@{}.service", facts.uid);
    if facts.cgroup.is_none() {
        return Check::fail(
            NAME,
            text::INIT_NO_CGROUP_V2.into(),
            text::INIT_FIX_CGROUP_V2.into(),
        );
    }
    let Some(controllers) = &facts.user_cgroup else {
        return Check::fail(
            NAME,
            text::init_cannot_read(&format!("the controllers of {unit}")),
            text::INIT_FIX_USER_MANAGER.into(),
        );
    };
    let has: Vec<&str> = controllers.split_whitespace().collect();
    let missing: Vec<&str> = ["cpu", "memory"]
        .into_iter()
        .filter(|c| !has.contains(c))
        .collect();
    if missing.is_empty() {
        Check::pass(NAME, text::init_controllers(&unit))
    } else {
        Check::fail(
            NAME,
            text::init_no_controllers(&unit, &missing),
            text::INIT_FIX_DELEGATE.into(),
        )
    }
}

/// Each check of the first run on `facts`, in order
/// (01M4ZJN6TEWCYCB39XTH6BBD9H).
pub fn checks(facts: &Facts) -> Vec<Check> {
    // The step that makes riff.toml fixes the session image of a clone that
    // has none: the pull gives the clone the file (01M5RGG9KJXCKH8C94145WS8SM).
    let makes_config =
        matches!(&facts.github, Some(Ok(github)) if !github.has(session_image::FILE));
    let image = match &facts.image {
        Ok(image) => Check::pass("the session image", image.clone()),
        Err(why) => {
            let check = Check::fail(
                "the session image",
                why.clone(),
                text::INIT_FIX_IMAGE.into(),
            );
            if makes_config {
                check.with_step(repo_setup::Step::Config)
            } else {
                check
            }
        }
    };
    let riff = match &facts.riff {
        Ok(found) => Check::pass(text::INIT_RIFF, found.clone()),
        Err(found) => Check::fail(text::INIT_RIFF, found.clone(), text::INIT_FIX_RIFF.into()),
    };
    let mut checks = machine_checks(facts);
    checks.extend([image, riff]);
    checks.extend(facts.github.iter().flat_map(crate::repo_setup::checks_of));
    checks
}

/// The checks of the machine on `facts`, in order: podman, the init of
/// podman, `/etc/subuid`, `/etc/subgid` and cgroup v2. Only a person
/// can do their fix, for example with `sudo`, so the riff daemon never
/// does it (01M51QFPW37EXVH4AXH70MSXBT, [`crate::update_steps`]).
pub fn machine_checks(facts: &Facts) -> Vec<Check> {
    let podman = match &facts.podman {
        Ok(_) if facts.uid == 0 => {
            Check::fail("podman", text::INIT_ROOT.into(), text::INIT_FIX_ROOT.into())
        }
        Ok(version) => Check::pass("podman", version.clone()),
        Err(why) => Check::fail("podman", why.clone(), text::INIT_FIX_PODMAN.into()),
    };
    let init = match &facts.init {
        Some(path) => Check::pass("the init of podman", path.display().to_string()),
        None => Check::fail(
            "the init of podman",
            text::init_no_init(&INIT_DIRS),
            text::INIT_FIX_INIT.into(),
        ),
    };
    vec![
        podman,
        init,
        sub_check("uid", facts.subuid.as_deref(), facts),
        sub_check("gid", facts.subgid.as_deref(), facts),
        cgroup_check(facts),
    ]
}

/// What `riff init` may change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `--check`: riff changes nothing and asks nothing.
    Check,
    /// riff asks one question before it removes the wide rules, and one
    /// before it sets up the repository on GitHub.
    Ask,
    /// `--yes`: riff sets up the repository on GitHub with no question.
    /// It removes no wide rule.
    Yes,
}

/// A question of `riff init`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
    /// Remove the wide allow rules (01M51JCW2EKF8H8QFQYSVRSPET).
    Remove,
    /// Do the steps that set up the repository on GitHub
    /// (01M5R1672NF56P15VD0JEX51K4).
    Setup,
}

impl Question {
    /// The question for people, with its `[y/N]`.
    pub fn text(&self) -> String {
        match self {
            Question::Remove => text::INIT_ASK_WIDE.to_owned(),
            Question::Setup => text::INIT_ASK_SETUP.to_owned(),
        }
    }
}

/// Asks the question `question` of `riff init`, when `terminal` is true
/// (01M51JCW2EKF8H8QFQYSVRSPET). It writes
/// the question to `out`, and reads one line of `input`: `y` or `yes` is
/// a yes. Each other answer, the end of the input, and no terminal are a
/// no.
///
/// ```
/// use riff::first_run::{Question, ask};
///
/// let mut out = Vec::new();
/// assert!(ask(Question::Remove, true, &mut &b"y\n"[..], &mut out)?);
/// assert!(ask(Question::Remove, true, &mut &b"Yes\n"[..], &mut out)?);
/// assert!(!ask(Question::Remove, true, &mut &b"\n"[..], &mut out)?);
/// assert!(!ask(Question::Remove, true, &mut &b""[..], &mut out)?);
/// assert!(String::from_utf8(out)?.starts_with("Remove them?"));
///
/// let mut out = Vec::new();
/// assert!(!ask(Question::Remove, true, &mut &b"n\n"[..], &mut out)?);
/// assert!(String::from_utf8(out)?.starts_with("Remove them?"));
///
/// // No terminal: no question, and no.
/// let mut out = Vec::new();
/// assert!(!ask(Question::Remove, false, &mut &b"y\n"[..], &mut out)?);
/// assert!(out.is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn ask(
    question: Question,
    terminal: bool,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<bool> {
    if !terminal {
        return Ok(false);
    }
    write!(out, "{}", question.text())?;
    out.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// The first run on `facts`, with the daemon in the folder `dir`. It
/// prints each step, and gives the exit code: 1 when a check failed.
/// When settings have wide rules, it names them, and in [`Mode::Ask`]
/// asks `ask` the question [`Question::Remove`]. When a check of the
/// repository on GitHub failed and has a step, and the person is an admin,
/// it names the steps and does them in the `forge`: after the question
/// [`Question::Setup`] in [`Mode::Ask`], and with no question in
/// [`Mode::Yes`] (01M5R1672NF56P15VD0JEX51K4). It prints the list of the
/// checks again after the steps, and the end of a failed run names each
/// failed check with its fix. With [`Mode::Check`], it stops after the
/// checks. When no daemon answers, it installs `unit` with
/// `manager`, and waits up to `wait` for the daemon.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    facts: &Facts,
    mode: Mode,
    mut ask: impl FnMut(Question) -> Result<bool>,
    dir: &Path,
    manager: &impl ServiceManager,
    unit: &Unit,
    wait: Duration,
    forge: &Forge,
) -> Result<i32> {
    println!("{}", text::INIT_CHECKS);
    let mut list = checks(facts);
    // A step fixes a failed check only when riff can do the step now
    // (01M5RGG9KJXCKH8C94145WS8SM).
    let doable = match &facts.github {
        Some(Ok(github)) if mode != Mode::Check && github.admin => Some(github),
        _ => None,
    };
    for check in &list {
        println!(
            "  {}",
            doable.map_or_else(|| check.line(), |github| check.planned_line(github))
        );
    }
    if !facts.wide.is_empty() {
        println!("{}", text::init_wide(&facts.wide));
        if mode == Mode::Ask && ask(Question::Remove)? {
            for wide in &facts.wide {
                let removed = permissions::remove(&wide.path, &wide.rules)?;
                println!("{}", text::init_wide_removed(&wide.path, removed.len()));
            }
        } else {
            println!("{}", text::INIT_WIDE_KEPT);
        }
    }
    let after = if mode == Mode::Check {
        None
    } else {
        set_up(facts, &list, mode, &mut ask, forge)?
    };
    let facts = after.as_ref().unwrap_or(facts);
    if after.is_some() {
        // The second list: each step is done, so a check that fails has no
        // step to wait for (01M5RGGD6YR36EXZEG61XCJVRY).
        list = checks(facts);
        println!("{}", text::INIT_CHECKS_AGAIN);
        for check in &list {
            println!("  {}", check.line());
        }
    }
    let failed: Vec<(&str, &str)> = list
        .iter()
        .filter_map(|c| Some((c.name.as_str(), c.fix.as_deref()?)))
        .collect();
    if !failed.is_empty() {
        println!("{}", text::init_failed(&failed));
        return Ok(1);
    }
    if mode == Mode::Check {
        println!("{}", text::INIT_CHECKED);
        return Ok(0);
    }
    let Ok(image) = &facts.image else {
        unreachable!("the image check passed");
    };
    match daemon::status(dir).await {
        Ok(status) => println!(
            "{}",
            text::init_daemon_kept(&status, &Build::this().to_string())
        ),
        Err(_) => daemon::install(manager, unit, dir, wait).await?,
    }
    daemon::image(dir, image, |line| println!("{line}")).await?;
    println!("{}", text::init_image_here(image));
    println!("{}", text::INIT_READY);
    Ok(0)
}

/// The steps of the failed checks `list` that set up the repository on
/// GitHub (01M5R1672NF56P15VD0JEX51K4). It names them, and does them
/// in `forge` when the person is an admin, and says yes: always in
/// [`Mode::Yes`], else after the question [`Question::Setup`]. It
/// returns the facts after the steps, or `None` when it did no step.
/// The facts after the steps have the `riff.toml` of the clone as the
/// session image, when the step made the file and `git pull` ([`pull`])
/// gave it to the clone, else the image of this release.
fn set_up(
    facts: &Facts,
    list: &[Check],
    mode: Mode,
    ask: &mut impl FnMut(Question) -> Result<bool>,
    forge: &Forge,
) -> Result<Option<Facts>> {
    let Some(Ok(github)) = &facts.github else {
        return Ok(None);
    };
    let steps = repo_setup::steps_of(list);
    if steps.is_empty() {
        return Ok(None);
    }
    if !github.admin {
        println!("{}", text::init_not_admin(&github.repo));
        return Ok(None);
    }
    let titles: Vec<String> = steps.iter().map(|s| s.title(github)).collect();
    println!("{}", text::init_setup(&github.repo, &titles));
    if mode != Mode::Yes && !ask(Question::Setup)? {
        println!("{}", text::INIT_SETUP_KEPT);
        return Ok(None);
    }
    let mut after = github.clone();
    for (step, done) in repo_setup::setup(forge, &steps, &mut after) {
        let title = step.title(github);
        println!(
            "  {}",
            match done {
                Ok(what) => text::init_step_done(&title, &what),
                Err(why) => text::init_step_failed(&title, &why),
            }
        );
    }
    let mut facts = facts.clone();
    if facts.image.is_err() && !github.has(session_image::FILE) && after.has(session_image::FILE) {
        // The new riff.toml is on the default branch, not yet in this clone.
        let branch = &github.default_branch;
        let pulled = match pull(&facts.folder, branch) {
            Ok(()) => {
                println!("{}", text::init_pulled(branch));
                true
            }
            Err(why) => {
                println!("{}", text::init_not_pulled(branch, &why));
                false
            }
        };
        // The pulled file is the image of the clone. Else the image of this
        // release stands in for it, as the file will pin it.
        let image = pulled
            .then(|| session_image::of_folder(&facts.folder).ok())
            .flatten()
            .unwrap_or_else(session_image::of_release);
        facts.image = Ok(image.to_string());
    }
    facts.github = Some(Ok(after));
    Ok(Some(facts))
}

/// Gives the clone in `clone` the commits of `origin` with `git pull
/// --ff-only origin BRANCH` (01M5RGGD9AYNC12Z2CG5CDZWNA), when it is safe:
/// the clone is on the branch `branch`, and has no change. A clone with
/// commits that `origin` lacks does not fast-forward, so git stops and
/// changes nothing. It gives why riff ran no pull, or why git failed, in
/// one line.
///
/// ```
/// use riff::first_run::pull;
///
/// let dir = tempfile::tempdir()?;
/// let git = |dir: &std::path::Path, args: &[&str]| {
///     let status = std::process::Command::new("git")
///         .args(["-c", "user.name=a", "-c", "user.email=a@b.c", "-C"])
///         .arg(dir)
///         .args(args)
///         .status()
///         .unwrap();
///     assert!(status.success(), "git {args:?}");
/// };
/// let (origin, clone) = (dir.path().join("origin"), dir.path().join("clone"));
/// git(dir.path(), &["init", "-q", "-b", "main", "origin"]);
/// git(&origin, &["commit", "-q", "--allow-empty", "-m", "first"]);
/// git(dir.path(), &["clone", "-q", "origin", "clone"]);
/// git(&origin, &["commit", "-q", "--allow-empty", "-m", "second"]);
///
/// assert_eq!(pull(&clone, "dev"), Err("the clone is on main, not on dev".to_owned()));
/// std::fs::write(clone.join("note"), "")?;
/// assert_eq!(pull(&clone, "main"), Err("the clone has a change".to_owned()));
/// std::fs::remove_file(clone.join("note"))?;
/// assert_eq!(pull(&clone, "main"), Ok(()));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn pull(clone: &Path, branch: &str) -> Result<(), String> {
    let git = |args: &[&str]| session_image::git(clone, args).map_err(|e| format!("{e:#}"));
    let on = git(&["symbolic-ref", "--quiet", "--short", "HEAD"]).unwrap_or_default();
    if on != branch {
        return Err(text::init_clone_branch(&on, branch));
    }
    if !git(&["status", "--porcelain"])?.is_empty() {
        return Err(text::INIT_CLONE_CHANGED.to_owned());
    }
    git(&["pull", "--ff-only", "origin", branch]).map(drop)
}

/// `riff init`: the first run on this machine, with the repository of
/// the current folder and the daemon of this machine. It asks its
/// question on the terminal. It gives the exit code. On a host that is
/// not Linux, it changes nothing.
pub async fn init(mode: Mode) -> Result<i32> {
    #[cfg(target_os = "linux")]
    {
        use anyhow::Context;
        use std::io::IsTerminal;
        let here = std::env::current_dir().context("cannot read the current folder")?;
        let facts = Facts::here(&here);
        let manager = crate::service::Systemd::here()?;
        let unit = Unit::of_this()?;
        let question = |question: Question| {
            let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
            ask(
                question,
                terminal,
                &mut std::io::stdin().lock(),
                &mut std::io::stdout(),
            )
        };
        let forge = crate::identity::github_origin(&here)
            .map(|repo| Forge::of(&repo))
            .unwrap_or_default();
        let dir = daemon::dir()?;
        run(&facts, mode, question, &dir, &manager, &unit, WAIT, &forge).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = mode;
        anyhow::bail!("{}", text::INIT_LINUX_ONLY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use crate::daemon::{Daemon, dir_in, listen};
    use crate::runtime::Fake;

    const IMAGE: &str =
        "ghcr.io/o/i@sha256:0000000000000000000000000000000000000000000000000000000000000000";

    /// A machine with each part.
    fn ready() -> Facts {
        Facts {
            podman: Ok("podman version 5.7.0".into()),
            init: Some("/usr/libexec/podman/catatonit".into()),
            user: "mike".into(),
            uid: 1000,
            subuid: Some("mike:100000:65536\n".into()),
            subgid: Some("mike:100000:65536\n".into()),
            cgroup: Some("cpuset cpu io memory hugetlb pids rdma misc\n".into()),
            user_cgroup: Some("cpu memory pids\n".into()),
            image: Ok(IMAGE.into()),
            wide: vec![],
            riff: Ok("/home/mike/.cargo/bin/riff is static".into()),
            github: None,
            folder: "/nonexistent/clone".into(),
        }
    }

    /// The question of a run that must ask nothing.
    fn no_question(question: Question) -> Result<bool> {
        panic!("riff asked: {}", question.text())
    }

    fn failed(facts: &Facts) -> Vec<Check> {
        checks(facts).into_iter().filter(|c| !c.passed()).collect()
    }

    #[test]
    fn a_machine_with_each_part_passes_each_check_in_order() {
        let lines: Vec<String> = checks(&ready()).iter().map(Check::line).collect();
        assert_eq!(
            lines,
            [
                "pass  podman: podman version 5.7.0",
                "pass  the init of podman: /usr/libexec/podman/catatonit",
                "pass  /etc/subuid: 65536 IDs for mike",
                "pass  /etc/subgid: 65536 IDs for mike",
                "pass  cgroup v2: user@1000.service has cpu and memory",
                &format!("pass  the session image: {IMAGE}"),
                "pass  the riff binary: /home/mike/.cargo/bin/riff is static",
            ]
        );
    }

    #[test]
    fn each_missing_part_gives_its_own_fix_line() {
        let cases: Vec<(&str, Facts, &str, &str)> = vec![
            (
                "podman",
                Facts {
                    podman: Err(text::INIT_NO_PODMAN.into()),
                    ..ready()
                },
                "fail  podman: riff cannot start podman.",
                "sudo apt install podman",
            ),
            (
                "init",
                Facts {
                    init: None,
                    ..ready()
                },
                "fail  the init of podman: podman has no init: no catatonit in \
                 /usr/local/libexec/podman, /usr/local/lib/podman, /usr/libexec/podman, \
                 /usr/lib/podman, so no session can start.",
                "sudo apt install catatonit",
            ),
            (
                "root",
                Facts { uid: 0, ..ready() },
                "fail  podman: riff runs as root, so podman runs with no user namespace.",
                "as your own user",
            ),
            (
                "subuid",
                Facts {
                    subuid: Some("brett:100000:65536\n".into()),
                    ..ready()
                },
                "fail  /etc/subuid: 0 IDs for mike, and podman needs 65536.",
                "sudo usermod --add-subuids 165536-231071 mike",
            ),
            (
                "subgid",
                Facts {
                    subgid: None,
                    ..ready()
                },
                "fail  /etc/subgid: riff cannot read /etc/subgid.",
                "sudo usermod --add-subgids 100000-165535 mike",
            ),
            (
                "cgroup v1",
                Facts {
                    cgroup: None,
                    ..ready()
                },
                "fail  cgroup v2: this machine has no cgroup v2.",
                "systemd.unified_cgroup_hierarchy=1",
            ),
            (
                "no user manager",
                Facts {
                    user_cgroup: None,
                    ..ready()
                },
                "fail  cgroup v2: riff cannot read the controllers of user@1000.service.",
                "loginctl enable-linger",
            ),
            (
                "no cpu or memory",
                Facts {
                    user_cgroup: Some("pids\n".into()),
                    ..ready()
                },
                "fail  cgroup v2: user@1000.service has no cpu and no memory.",
                "Delegate=cpu cpuset io memory pids",
            ),
            (
                "no image",
                Facts {
                    image: Err("/tmp is not in a git repository".into()),
                    ..ready()
                },
                "fail  the session image: /tmp is not in a git repository.",
                "riff.toml",
            ),
            (
                "a riff that is not static",
                Facts {
                    riff: Err(text::init_riff_not_static(
                        "/home/mike/.cargo/bin/riff".as_ref(),
                        "it needs the shared libraries of /lib64/ld-linux-x86-64.so.2: it is not \
                         static",
                    )),
                    ..ready()
                },
                "fail  the riff binary: /home/mike/.cargo/bin/riff: it needs the shared \
                 libraries of /lib64/ld-linux-x86-64.so.2: it is not static.",
                "run riff update.",
            ),
        ];
        let mut fixes = Vec::new();
        for (case, facts, start, fix) in cases {
            let failed = failed(&facts);
            assert_eq!(failed.len(), 1, "{case}: {failed:?}");
            let line = failed[0].line();
            assert!(line.starts_with(start), "{case}: {line}");
            assert!(line.contains(" Fix: "), "{case}: {line}");
            assert!(line.contains(fix), "{case}: {line}");
            assert!(!line.contains('\n'), "{case}: one line: {line}");
            fixes.push(failed[0].fix.clone().unwrap());
        }
        // Each part has its own fix.
        let mut unique = fixes.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), fixes.len(), "{fixes:#?}");
    }

    /// A repository on GitHub with each part.
    fn github() -> crate::repo_setup::Github {
        crate::repo_setup::Github {
            repo: "acme/app".into(),
            default_branch: "main".into(),
            auto_merge: true,
            squash_merge: true,
            required: vec!["Gate".into(), "riff/verify".into()],
            milestones: vec!["Backlog".into(), "Wave 1".into()],
            admin: true,
            files: vec!["riff.toml".into()],
            ci: vec!["Gate".into()],
        }
    }

    /// 01M5QVF5535YRN1EF3DZAB4W8G: the checks of the repository come after
    /// the checks of the machine, and only for a folder with a remote on
    /// GitHub.
    #[test]
    fn the_repository_on_github_has_six_checks_after_the_machine() {
        let names = |facts: &Facts| {
            checks(facts)
                .into_iter()
                .map(|c| c.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&ready()).len(), 7, "no remote on GitHub: no check");
        let with = Facts {
            github: Some(Ok(github())),
            ..ready()
        };
        assert!(checks(&with).iter().all(Check::passed));
        assert_eq!(
            names(&with)[7..],
            [
                "riff.toml on main",
                "auto-merge and squash merge",
                "the ruleset of main",
                "the milestone Backlog",
                "the first wave",
                "the checks of a pull request"
            ]
        );
        let none = Facts {
            github: Some(Ok(crate::repo_setup::Github {
                auto_merge: false,
                required: vec![],
                milestones: vec![],
                ..github()
            })),
            ..ready()
        };
        assert_eq!(failed(&none).len(), 4);
        let unread = Facts {
            github: Some(Err("gh: not logged in".into())),
            ..ready()
        };
        let failed = failed(&unread);
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(failed[0].name, "the repository on GitHub");
    }

    #[test]
    fn a_range_that_is_too_small_fails_and_two_lines_add_up() {
        let small = Facts {
            subuid: Some("mike:100000:1000\n".into()),
            ..ready()
        };
        let line = failed(&small)[0].line();
        assert!(
            line.starts_with("fail  /etc/subuid: 1000 IDs for mike, and podman needs 65536."),
            "{line}"
        );
        let two = Facts {
            subuid: Some("mike:100000:32768\n1000:200000:32768\n".into()),
            ..ready()
        };
        assert!(failed(&two).is_empty());
    }

    #[test]
    fn the_fix_names_a_range_after_each_range_of_another_person() {
        let facts = Facts {
            subuid: Some("brett:100000:65536\n".into()),
            subgid: Some("brett:100000:65536\nnate:165536:65536\n".into()),
            ..ready()
        };
        let fixes: Vec<String> = failed(&facts).into_iter().map(|c| c.fix.unwrap()).collect();
        assert_eq!(
            fixes,
            [
                "sudo usermod --add-subuids 165536-231071 mike, then podman system migrate",
                "sudo usermod --add-subgids 231072-296607 mike, then podman system migrate",
            ]
        );
    }

    #[test]
    fn a_user_with_no_cpu_names_only_cpu() {
        let facts = Facts {
            user_cgroup: Some("memory pids".into()),
            ..ready()
        };
        let line = failed(&facts)[0].line();
        assert!(
            line.starts_with("fail  cgroup v2: user@1000.service has no cpu. Fix: "),
            "{line}"
        );
    }

    /// A service manager that installs nothing, and logs each call.
    #[derive(Default)]
    struct Manager(Arc<Mutex<Vec<String>>>);

    impl ServiceManager for Manager {
        fn install(&self, _unit: &Unit) -> Result<PathBuf> {
            self.0.lock().unwrap().push("install".into());
            anyhow::bail!("this test installs nothing")
        }

        fn uninstall(&self) -> Result<Option<PathBuf>> {
            self.0.lock().unwrap().push("uninstall".into());
            Ok(None)
        }

        fn state(&self) -> Result<String> {
            self.0.lock().unwrap().push("state".into());
            Ok("inactive".into())
        }

        fn installed(&self) -> Result<Option<Unit>> {
            self.0.lock().unwrap().push("installed".into());
            Ok(None)
        }

        fn rewrite(&self, _unit: &Unit) -> Result<PathBuf> {
            self.0.lock().unwrap().push("rewrite".into());
            anyhow::bail!("this test writes nothing")
        }

        fn try_restart(&self) -> Result<()> {
            self.0.lock().unwrap().push("try-restart".into());
            Ok(())
        }

        fn main_pid(&self) -> Result<Option<u32>> {
            self.0.lock().unwrap().push("main-pid".into());
            Ok(None)
        }
    }

    fn unit() -> Unit {
        Unit {
            exe: "/opt/riff".into(),
            env: vec![],
        }
    }

    #[tokio::test]
    async fn a_failed_check_installs_nothing_and_pulls_nothing() {
        let root = tempfile::tempdir().unwrap();
        let manager = Manager::default();
        let facts = Facts {
            podman: Err(text::INIT_NO_PODMAN.into()),
            ..ready()
        };
        let code = run(
            &facts,
            Mode::Ask,
            no_question,
            &dir_in(root.path()),
            &manager,
            &unit(),
            WAIT,
            &Forge::default(),
        )
        .await
        .unwrap();
        assert_eq!(code, 1);
        assert!(manager.0.lock().unwrap().is_empty());
        assert!(!dir_in(root.path()).exists());
    }

    #[tokio::test]
    async fn the_check_only_run_changes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let manager = Manager::default();
        let code = run(
            &ready(),
            Mode::Check,
            no_question,
            &dir_in(root.path()),
            &manager,
            &unit(),
            WAIT,
            &Forge::default(),
        )
        .await
        .unwrap();
        assert_eq!(code, 0);
        assert!(manager.0.lock().unwrap().is_empty());
    }

    /// The user settings of a test: `.claude/settings.json` in `root`,
    /// never the real config.
    fn settings_in(root: &Path, text: Option<&str>) -> PathBuf {
        let path = root.join(".claude/settings.json");
        if let Some(text) = text {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
        }
        path
    }

    /// 01M5084K09Z2S7N57315CX2R0F: `riff init` writes no rule into the user
    /// settings of Claude Code, with a yes, with `--yes`, with `--check`,
    /// and with no settings file.
    #[tokio::test]
    async fn riff_init_writes_no_rule_into_the_user_settings() {
        let root = tempfile::tempdir().unwrap();
        let text = r#"{"model": "opus"}"#;
        let there = settings_in(root.path(), Some(text));
        let absent = settings_in(&root.path().join("other"), None);
        // podman fails, so each run stops before the daemon.
        let facts = Facts {
            podman: Err(text::INIT_NO_PODMAN.into()),
            ..ready()
        };
        for mode in [Mode::Ask, Mode::Yes, Mode::Check] {
            let manager = Manager::default();
            let dir = dir_in(root.path());
            let code = run(
                &facts,
                mode,
                |_| Ok(true),
                &dir,
                &manager,
                &unit(),
                WAIT,
                &Forge::default(),
            )
            .await
            .unwrap();
            assert_eq!(code, 1);
            assert_eq!(std::fs::read_to_string(&there).unwrap(), text);
            assert!(!absent.exists());
            assert!(manager.0.lock().unwrap().is_empty());
        }
        let names: Vec<String> = checks(&ready()).into_iter().map(|c| c.name).collect();
        assert!(!names.iter().any(|n| n.contains("rules")), "{names:?}");
    }

    #[tokio::test]
    async fn a_daemon_that_answers_stays_and_pulls_the_image() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let fake = Fake::default().with_registry(IMAGE);
        let listener = listen(&dir).await.unwrap();
        riff_client::secrets::mock_store();
        let daemon = Arc::new(Daemon::new(fake.clone(), dir.clone()).unwrap());
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let served = tokio::spawn(daemon.serve(listener, async {
            let _ = stopped.await;
        }));

        let manager = Manager::default();
        let code = run(
            &ready(),
            Mode::Ask,
            no_question,
            &dir,
            &manager,
            &unit(),
            WAIT,
            &Forge::default(),
        )
        .await
        .unwrap();
        assert_eq!(code, 0);
        // riff kept the daemon: no install, no restart.
        assert!(manager.0.lock().unwrap().is_empty());
        assert_eq!(fake.calls(), [format!("pull {IMAGE}")]);

        // A second run finds the image, and pulls nothing.
        let code = run(
            &ready(),
            Mode::Ask,
            no_question,
            &dir,
            &manager,
            &unit(),
            WAIT,
            &Forge::default(),
        )
        .await
        .unwrap();
        assert_eq!(code, 0);
        assert_eq!(fake.calls(), [format!("pull {IMAGE}")]);

        let _ = stop.send(());
        served.await.unwrap().unwrap();
    }

    /// The user settings and the local settings of a test with wide
    /// rules, in `root`: never the real config.
    struct WideFiles {
        user: PathBuf,
        local: PathBuf,
        user_text: String,
        local_text: String,
    }

    impl WideFiles {
        fn new(root: &Path) -> WideFiles {
            let user = settings_in(&root.join("home"), None);
            let local = root.join("repo/.claude/settings.local.json");
            let user_text = r#"{"model": "opus", "permissions": {"allow": ["Bash(riff *)",
                "Bash(gh pr merge * --auto --squash)", "Bash(cargo test:*)"],
                "deny": ["Bash(git push * main)"]}, "statusLine": {"type": "command"}}"#;
            let local_text = r#"{"permissions": {"allow": ["Bash(git push *)", "Read(~/**)"]}}"#;
            for (path, text) in [(&user, user_text), (&local, local_text)] {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            WideFiles {
                user,
                local,
                user_text: user_text.into(),
                local_text: local_text.into(),
            }
        }

        /// The settings files with wide rules: the user settings, the
        /// project settings (not there) and the local settings.
        fn wide(&self, root: &Path) -> Vec<Wide> {
            let project = root.join("repo/.claude/settings.json");
            Wide::of(
                &[self.user.clone(), project, self.local.clone()],
                Some("/home/mike".as_ref()),
            )
        }

        /// True when no file changed.
        fn kept(&self) -> bool {
            std::fs::read_to_string(&self.user).unwrap() == self.user_text
                && std::fs::read_to_string(&self.local).unwrap() == self.local_text
        }

        fn value(path: &Path) -> serde_json::Value {
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
        }
    }

    /// 01M51JCW04FAHZDQ144KDNCCPX: riff finds each wide rule in its file,
    /// reads a file once, and a wide rule fails no check.
    #[test]
    fn riff_finds_the_wide_rules_of_each_file_and_fails_no_check() {
        let root = tempfile::tempdir().unwrap();
        let files = WideFiles::new(root.path());
        let wide = files.wide(root.path());
        assert_eq!(
            wide,
            [
                Wide {
                    path: files.user.clone(),
                    rules: Rules {
                        allow: vec!["Bash(gh pr merge * --auto --squash)".into()],
                        deny: vec![],
                    },
                },
                Wide {
                    path: files.local.clone(),
                    rules: Rules {
                        allow: vec!["Bash(git push *)".into()],
                        deny: vec![],
                    },
                },
            ]
        );
        // The project settings are the user settings: in the home.
        let link = root.path().join("home/link.json");
        std::os::unix::fs::symlink(&files.user, &link).unwrap();
        assert_eq!(
            Wide::of(&[files.user.clone(), link], None),
            wide[..1].to_vec()
        );
        let facts = Facts { wide, ..ready() };
        assert!(failed(&facts).is_empty());
        let text = text::init_wide(&facts.wide);
        assert!(
            text.contains(&format!(
                "\n  {}:\n    allow Bash(gh pr merge * --auto --squash)\n  {}:\n    allow Bash(git \
                 push *)",
                files.user.display(),
                files.local.display()
            )),
            "{text}"
        );
    }

    /// 01M51JCW2EKF8H8QFQYSVRSPET: a yes removes each wide rule, and keeps
    /// each other rule and key in its order.
    #[tokio::test]
    async fn a_yes_removes_each_wide_rule_and_keeps_each_other_key() {
        let root = tempfile::tempdir().unwrap();
        let files = WideFiles::new(root.path());
        // podman fails, so the run stops before the daemon.
        let facts = Facts {
            podman: Err(text::INIT_NO_PODMAN.into()),
            wide: files.wide(root.path()),
            ..ready()
        };
        let mut asked = Vec::new();
        let question = |q: Question| {
            asked.push(q.text());
            Ok(true)
        };
        let manager = Manager::default();
        let dir = dir_in(root.path());
        let code = run(
            &facts,
            Mode::Ask,
            question,
            &dir,
            &manager,
            &unit(),
            WAIT,
            &Forge::default(),
        )
        .await
        .unwrap();
        assert_eq!(code, 1);
        assert_eq!(asked, [text::INIT_ASK_WIDE]);

        let user = WideFiles::value(&files.user);
        let keys: Vec<&String> = user.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["model", "permissions", "statusLine"]);
        assert_eq!(
            user["permissions"]["allow"],
            serde_json::json!(["Bash(riff *)", "Bash(cargo test:*)"])
        );
        assert_eq!(
            user["permissions"]["deny"],
            serde_json::json!(["Bash(git push * main)"])
        );
        let local = WideFiles::value(&files.local);
        assert_eq!(
            local["permissions"]["allow"],
            serde_json::json!(["Read(~/**)"])
        );
        assert!(files.wide(root.path()).is_empty());
        assert!(!root.path().join("repo/.claude/settings.json").exists());
        assert!(manager.0.lock().unwrap().is_empty());
    }

    /// 01M51JCW2EKF8H8QFQYSVRSPET: a no keeps each rule.
    #[tokio::test]
    async fn a_no_keeps_each_wide_rule() {
        let root = tempfile::tempdir().unwrap();
        let files = WideFiles::new(root.path());
        let facts = Facts {
            podman: Err(text::INIT_NO_PODMAN.into()),
            wide: files.wide(root.path()),
            ..ready()
        };
        let mut asked = 0;
        let question = |q: Question| {
            assert_eq!(q, Question::Remove);
            asked += 1;
            Ok(false)
        };
        let dir = dir_in(root.path());
        let code = run(
            &facts,
            Mode::Ask,
            question,
            &dir,
            &Manager::default(),
            &unit(),
            WAIT,
            &Forge::default(),
        )
        .await
        .unwrap();
        assert_eq!(code, 1);
        assert_eq!(asked, 1);
        assert!(files.kept());
    }

    /// 01M51JCW04FAHZDQ144KDNCCPX, 01M51JCW2EKF8H8QFQYSVRSPET: `--check`
    /// and `--yes` name the wide rules, ask nothing and remove nothing.
    #[tokio::test]
    async fn check_and_yes_remove_no_wide_rule() {
        let root = tempfile::tempdir().unwrap();
        let files = WideFiles::new(root.path());
        let wide = files.wide(root.path());
        let dir = dir_in(root.path());
        let manager = Manager::default();
        let facts = Facts {
            wide: wide.clone(),
            ..ready()
        };
        let code = run(
            &facts,
            Mode::Check,
            no_question,
            &dir,
            &manager,
            &unit(),
            WAIT,
            &Forge::default(),
        )
        .await
        .unwrap();
        assert_eq!(code, 0, "a wide rule fails no check");
        let facts = Facts {
            podman: Err(text::INIT_NO_PODMAN.into()),
            ..facts
        };
        let code = run(
            &facts,
            Mode::Yes,
            no_question,
            &dir,
            &manager,
            &unit(),
            WAIT,
            &Forge::default(),
        )
        .await
        .unwrap();
        assert_eq!(code, 1);
        assert!(files.kept());
        assert!(manager.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_image_that_the_daemon_cannot_pull_fails_the_run() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let fake = Fake::default();
        let listener = listen(&dir).await.unwrap();
        riff_client::secrets::mock_store();
        let daemon = Arc::new(Daemon::new(fake.clone(), dir.clone()).unwrap());
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let served = tokio::spawn(daemon.serve(listener, async {
            let _ = stopped.await;
        }));

        let error = run(
            &ready(),
            Mode::Ask,
            no_question,
            &dir,
            &Manager::default(),
            &unit(),
            WAIT,
            &Forge::default(),
        )
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains(IMAGE), "{error:#}");

        let _ = stop.send(());
        served.await.unwrap().unwrap();
    }

    /// The fake forge of the tests of `riff init` (`tests/fake/forge`),
    /// in a folder of its own.
    struct Forged {
        dir: tempfile::TempDir,
    }

    impl Forged {
        /// A bare repository: no file, a pull request that ran two checks,
        /// no auto-merge, no ruleset, no milestone.
        fn bare() -> Forged {
            let dir = tempfile::tempdir().unwrap();
            let files = [
                ("files", ""),
                ("milestones", ""),
                (
                    "prior_pulls",
                    r#"[{"number":3,"html_url":"https://github.com/acme/app/pull/3","head":{"ref":"feature","sha":"abc1234"},"base":{"ref":"main"}}]"#,
                ),
                (
                    "checks_abc1234",
                    r#"{"check_runs":[{"name":"checks / lint"},{"name":"checks / test"}]}"#,
                ),
            ];
            for (name, text) in files {
                std::fs::write(dir.path().join(name), text).unwrap();
            }
            isolated::write_program(dir.path().join("gh"), include_str!("../tests/fake/forge"));
            Forged { dir }
        }

        fn forge(&self) -> Forge {
            Forge {
                gh: crate::pr::Gh::at(self.dir.path().join("gh")),
                waits: repo_setup::Waits::NONE,
            }
        }

        /// The facts of the machine, with the facts that riff reads from
        /// the forge now. The clone has no `riff.toml` when `image` is
        /// `None`.
        fn facts(&self, image: Option<&str>) -> Facts {
            Facts {
                image: image
                    .map(str::to_owned)
                    .ok_or("no riff.toml in this clone".into()),
                github: Some(repo_setup::read(&self.forge().gh)),
                ..ready()
            }
        }

        /// The calls that change the forge.
        fn changes(&self) -> Vec<String> {
            std::fs::read_to_string(self.dir.path().join("calls"))
                .unwrap_or_default()
                .lines()
                .filter(|c| c.contains(" -X ") || c.starts_with("label "))
                .map(Into::into)
                .collect()
        }

        fn has(&self, name: &str) -> bool {
            self.dir.path().join(name).exists()
        }
    }

    /// `run` with a podman that fails: it does the setup and the checks,
    /// and stops before the daemon.
    async fn run_stopped(forged: &Forged, facts: &Facts, mode: Mode, yes: bool) -> (i32, usize) {
        let root = tempfile::tempdir().unwrap();
        let facts = Facts {
            podman: Err(text::INIT_NO_PODMAN.into()),
            ..facts.clone()
        };
        let asked = std::cell::Cell::new(0);
        let question = |q: Question| {
            assert_eq!(q, Question::Setup, "riff asked: {}", q.text());
            asked.set(asked.get() + 1);
            Ok(yes)
        };
        let code = run(
            &facts,
            mode,
            question,
            &dir_in(root.path()),
            &Manager::default(),
            &unit(),
            WAIT,
            &forged.forge(),
        )
        .await
        .unwrap();
        (code, asked.get())
    }

    /// 01M5R1672NF56P15VD0JEX51K4: `riff init --yes` sets up a bare
    /// repository with no question; the second run changes nothing and
    /// asks nothing.
    #[tokio::test]
    async fn yes_sets_up_a_bare_repository_and_pulls_the_new_image_and_a_second_run_changes_nothing()
     {
        let forged = Forged::bare();
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let release = session_image::of_release().to_string();
        let fake = Fake::default().with_registry(&release);
        let listener = listen(&dir).await.unwrap();
        riff_client::secrets::mock_store();
        let daemon = Arc::new(Daemon::new(fake.clone(), dir.clone()).unwrap());
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let served = tokio::spawn(daemon.serve(listener, async {
            let _ = stopped.await;
        }));

        // The clone has no riff.toml: the new file is the image of the run.
        let manager = Manager::default();
        let code = run(
            &forged.facts(None),
            Mode::Yes,
            no_question,
            &dir,
            &manager,
            &unit(),
            WAIT,
            &forged.forge(),
        )
        .await
        .unwrap();
        assert_eq!(code, 0);
        assert_eq!(fake.calls(), [format!("pull {release}")]);
        let changes = forged.changes().len();
        assert!(changes >= 8, "{:?}", forged.changes());
        for name in ["auto_merge", "ruleset", "config.json", "merge.json"] {
            assert!(forged.has(name), "{name}");
        }
        let milestones = std::fs::read_to_string(forged.dir.path().join("milestones")).unwrap();
        assert_eq!(milestones, "Backlog\nWave 1\n");

        // The clone pulled the new file: a second run changes nothing.
        let code = run(
            &forged.facts(Some(&release)),
            Mode::Yes,
            no_question,
            &dir,
            &manager,
            &unit(),
            WAIT,
            &forged.forge(),
        )
        .await
        .unwrap();
        assert_eq!(code, 0);
        assert_eq!(
            forged.changes().len(),
            changes,
            "a second run changes the forge"
        );
        assert_eq!(fake.calls(), [format!("pull {release}")]);

        let _ = stop.send(());
        served.await.unwrap().unwrap();
    }

    /// 01M5R1672NF56P15VD0JEX51K4: in `Mode::Ask`, the steps wait for one
    /// question. A no does none, and a yes does each.
    #[tokio::test]
    async fn the_steps_wait_for_one_question_and_a_no_changes_nothing() {
        let forged = Forged::bare();
        let facts = forged.facts(Some(IMAGE));
        let (code, asked) = run_stopped(&forged, &facts, Mode::Ask, false).await;
        assert_eq!((code, asked), (1, 1));
        assert!(forged.changes().is_empty(), "{:?}", forged.changes());

        let (code, asked) = run_stopped(&forged, &facts, Mode::Ask, true).await;
        // podman fails, so a check still fails; the steps are done.
        assert_eq!((code, asked), (1, 1));
        assert!(forged.has("ruleset") && forged.has("config.json"));

        // Set up: no question at all.
        let facts = forged.facts(Some(IMAGE));
        assert!(
            failed(&Facts {
                podman: Ok("podman".into()),
                ..facts.clone()
            })
            .is_empty()
        );
        let (_, asked) = run_stopped(&forged, &facts, Mode::Ask, true).await;
        assert_eq!(asked, 0);
    }

    /// 01M5R1672NF56P15VD0JEX51K4: `--check` makes no call that changes the
    /// forge, and asks nothing.
    #[tokio::test]
    async fn check_names_the_steps_and_changes_nothing() {
        let forged = Forged::bare();
        let facts = forged.facts(Some(IMAGE));
        let (code, asked) = run_stopped(&forged, &facts, Mode::Check, true).await;
        assert_eq!((code, asked), (1, 0));
        assert!(forged.changes().is_empty(), "{:?}", forged.changes());
        assert!(!forged.has("ruleset"));
    }

    /// 01M5R1672NF56P15VD0JEX51K4: a person who is not an admin gets the
    /// steps named. riff asks nothing and changes nothing.
    #[tokio::test]
    async fn a_person_who_is_not_an_admin_gets_no_question_and_no_change() {
        let forged = Forged::bare();
        std::fs::write(forged.dir.path().join("not_admin"), "").unwrap();
        let facts = forged.facts(Some(IMAGE));
        assert!(!facts.github.as_ref().unwrap().as_ref().unwrap().admin);
        for mode in [Mode::Ask, Mode::Yes] {
            let (code, asked) = run_stopped(&forged, &facts, mode, true).await;
            assert_eq!((code, asked), (1, 0), "{mode:?}");
        }
        assert!(forged.changes().is_empty(), "{:?}", forged.changes());
    }

    /// A step that fails fails the run, and the next steps run.
    #[tokio::test]
    async fn a_step_that_fails_fails_the_run_and_the_next_steps_run() {
        let forged = Forged::bare();
        std::fs::write(forged.dir.path().join("fail"), "rulesets").unwrap();
        let facts = forged.facts(Some(IMAGE));
        let (code, _) = run_stopped(&forged, &facts, Mode::Yes, true).await;
        assert_eq!(code, 1);
        assert!(forged.has("auto_merge") && forged.has("config.json"));
        assert!(!forged.has("ruleset"));
        let milestones = std::fs::read_to_string(forged.dir.path().join("milestones")).unwrap();
        assert_eq!(milestones, "Backlog\nWave 1\n");
    }

    /// Runs `git ARGS` in `dir`, and gives its stdout.
    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=riff",
                "-c",
                "user.email=riff@example.com",
                "-C",
            ])
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// A clone of a repository on the branch `main` that lacks the last
    /// commit of its origin: the commit that adds `riff.toml` with the image
    /// `IMAGE`, as the merge of the step does. It gives the folders of
    /// the origin and of the clone.
    fn clone_behind(root: &Path) -> (PathBuf, PathBuf) {
        let (origin, clone) = (root.join("origin"), root.join("clone"));
        git(root, &["init", "-q", "-b", "main", "origin"]);
        std::fs::write(origin.join("CLAUDE.md"), "rules\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "-q", "-m", "first"]);
        git(root, &["clone", "-q", "origin", "clone"]);
        let image = IMAGE.parse().unwrap();
        std::fs::write(origin.join("riff.toml"), session_image::config_of(&image)).unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "-q", "-m", "Set up riff"]);
        (origin, clone)
    }

    /// `set_up` in `Mode::Yes` for the clone `folder`, in the forge `forged`:
    /// the facts after the steps.
    fn set_up_in(forged: &Forged, folder: &Path) -> Facts {
        let facts = Facts {
            folder: folder.to_owned(),
            ..forged.facts(None)
        };
        let list = checks(&facts);
        set_up(&facts, &list, Mode::Yes, &mut no_question, &forged.forge())
            .unwrap()
            .expect("the steps ran")
    }

    /// 01M5RGGD9AYNC12Z2CG5CDZWNA: the clone is on the default branch, has
    /// no change, and lacks only the commits of `origin`: after the step of
    /// `riff.toml`, riff runs `git pull --ff-only`, and the clone has the
    /// file. The session image is the one that the file pins.
    #[test]
    fn a_clone_that_lacks_only_commits_of_origin_gets_riff_toml_by_git_pull() {
        let forged = Forged::bare();
        let root = tempfile::tempdir().unwrap();
        let (origin, clone) = clone_behind(root.path());
        assert!(!clone.join("riff.toml").exists());

        let after = set_up_in(&forged, &clone);
        assert!(forged.has("config.json"), "the step ran");
        assert!(clone.join("riff.toml").exists());
        assert_eq!(
            git(&clone, &["rev-parse", "HEAD"]),
            git(&origin, &["rev-parse", "HEAD"])
        );
        assert_eq!(after.image, Ok(IMAGE.to_owned()));
        assert_ne!(IMAGE, session_image::of_release().to_string());
    }

    /// 01M5RGGD9AYNC12Z2CG5CDZWNA: a clone on another branch, with a change,
    /// or with a commit that `origin` lacks gets no `git pull`: riff changes
    /// nothing in it, and the image of the release stands in for the file.
    #[test]
    fn a_clone_that_is_not_safe_to_pull_gets_no_git_pull() {
        let release = session_image::of_release().to_string();
        for case in ["another branch", "a change", "a local commit"] {
            let forged = Forged::bare();
            let root = tempfile::tempdir().unwrap();
            let (_, clone) = clone_behind(root.path());
            match case {
                "another branch" => drop(git(&clone, &["checkout", "-q", "-b", "dev"])),
                "a change" => std::fs::write(clone.join("CLAUDE.md"), "mine\n").unwrap(),
                _ => drop(git(
                    &clone,
                    &["commit", "-q", "--allow-empty", "-m", "mine"],
                )),
            }
            let head = git(&clone, &["rev-parse", "HEAD"]);
            let after = set_up_in(&forged, &clone);
            assert!(forged.has("config.json"), "{case}: the step ran");
            assert!(!clone.join("riff.toml").exists(), "{case}");
            assert_eq!(git(&clone, &["rev-parse", "HEAD"]), head, "{case}");
            assert_eq!(after.image, Ok(release.clone()), "{case}");
        }
    }

    #[test]
    fn pull_says_why_it_ran_no_git_pull() {
        let root = tempfile::tempdir().unwrap();
        let (_, clone) = clone_behind(root.path());
        assert_eq!(
            pull(&clone, "trunk"),
            Err("the clone is on main, not on trunk".to_owned())
        );
        git(&clone, &["checkout", "-q", "--detach"]);
        assert_eq!(
            pull(&clone, "main"),
            Err("the clone is on no branch, not on main".to_owned())
        );
        git(&clone, &["checkout", "-q", "main"]);
        std::fs::write(clone.join("new"), "").unwrap();
        assert_eq!(
            pull(&clone, "main"),
            Err("the clone has a change".to_owned())
        );
        std::fs::remove_file(clone.join("new")).unwrap();
        git(&clone, &["commit", "-q", "--allow-empty", "-m", "mine"]);
        let why = pull(&clone, "main").unwrap_err();
        assert!(why.starts_with("git pull --ff-only origin main: "), "{why}");
        assert!(!why.contains('\n'), "{why}");
        // Not a repository: git says so.
        let none = tempfile::tempdir().unwrap();
        assert!(pull(none.path(), "main").is_err());
    }

    #[test]
    fn the_question_of_the_setup_is_a_yes_or_no() {
        let mut out = Vec::new();
        assert!(ask(Question::Setup, true, &mut &b"y\n"[..], &mut out).unwrap());
        assert!(!ask(Question::Setup, true, &mut &b"\n"[..], &mut out).unwrap());
        assert!(
            String::from_utf8(out)
                .unwrap()
                .starts_with("Do them? [y/N] ")
        );
        assert!(!ask(Question::Setup, false, &mut &b"y\n"[..], &mut Vec::new()).unwrap());
    }
}
