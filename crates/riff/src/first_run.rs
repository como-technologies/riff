//! The first run of riff 3.0 on a machine (#721): `riff init`
//! (01M4ZJN6R68YBD3XMM725ZRJ7W).
//!
//! # Design
//!
//! `riff init` gets a machine ready for `riff run`. It needs no
//! riff-server and no sign-in. It does these steps, in order:
//!
//! ```mermaid
//! flowchart LR
//!     C["the checks<br/>podman, subuid, subgid,<br/>cgroup v2, riff.toml,<br/>the rules of Claude Code,<br/>the riff binary"] --> Q{"rules<br/>missing?"}
//!     Q -- "one question:<br/>y, or --yes" --> W["add them to<br/>the user settings"]
//!     Q -- "no" --> E{"each check<br/>passed?"}
//!     W --> E
//!     E -- "no" --> F["a fix line for each,<br/>exit 1"]
//!     E -- "yes" --> D["the daemon<br/>riff.service"]
//!     D --> I["the session image<br/>the daemon pulls it"]
//!     I --> R["ready for riff run"]
//! ```
//!
//! 1. The checks ([`checks`]). riff reads the [`Facts`] of the machine,
//!    and prints one line for each check: `pass`, or `fail` with the
//!    fix (01M4ZJN6TEWCYCB39XTH6BBD9H).
//! 2. The rules. When the user settings of Claude Code lack a permission
//!    rule of riff ([`crate::permissions::user_rules`]), riff names each
//!    rule and asks one question (01M5084K09Z2S7N57315CX2R0F). `--yes`
//!    answers yes ([`Mode`]). On a yes, riff adds the rules
//!    ([`crate::permissions::add`]), and that check passes. riff asks also
//!    when another check failed (01M5084K2Q3HH4BK7DJMYQD7C1). With no
//!    terminal and no `--yes`, it asks nothing. `--check` names the rules
//!    and asks nothing.
//! 3. When a check still fails, riff installs nothing, pulls nothing, and
//!    exits with 1. `--check` stops here (01M4ZJN6WR0JJ891R9JMQNPKGD).
//! 4. The daemon. When a riff daemon answers, riff keeps it: it does
//!    not start it again. When no daemon answers, riff installs it as
//!    `riff daemon install` does ([`crate::daemon::install`],
//!    01M4ZJN6Z1DGD43CKZ19QPN2NK).
//! 5. The image. riff asks the daemon for the session image that
//!    `riff.toml` pins. The daemon pulls it when podman does not have
//!    it (01M4ZJN718EF1PC9YY9GFD0EJ2).
//!
//! # The checks
//!
//! | Check | riff reads | Pass |
//! |---|---|---|
//! | podman | `podman --version`, and the user ID | it runs, and riff does not run as root: so podman runs rootless |
//! | `/etc/subuid` | the lines of the person, by name or by user ID | [`MIN_IDS`] IDs or more, added over the lines |
//! | `/etc/subgid` | the same | the same |
//! | cgroup v2 | `/sys/fs/cgroup/cgroup.controllers`, and `cgroup.controllers` of `user@UID.service` | the file is there, and the controllers of the user have `cpu` and `memory` |
//! | the session image | `riff.toml` of the repository of the current folder ([`crate::session_image::of_folder`]) | it pins an image by its digest |
//! | the rules of Claude Code | the user settings of Claude Code: `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json` ([`Settings`]) | they have each rule of [`crate::permissions::user_rules`] |
//! | the riff binary | the ELF of the riff of this process ([`crate::release::check_file`]) | it has no interpreter, so `riff relay` starts in each session image. The fix is `riff update` (01M510NW68CTZBHHW7ZYFV7Q3X) |
//!
//! The fix of `/etc/subuid` or `/etc/subgid` adds a free range of
//! [`MIN_IDS`] IDs from [`free_sub_id`].
//!
//! riff reads the controllers of `user@UID.service`, not `podman info`:
//! a process in a scope that gives no `cpu` to its children still gets
//! containers with `cpu.max` (01M4Z60B7MW963MBQAPN2CJEQZ).
//!
//! ```
//! use riff::first_run::{Facts, Settings, checks};
//! use riff::permissions::Rules;
//!
//! let mut facts = Facts {
//!     podman: Ok("podman version 5.7.0".into()),
//!     user: "mike".into(),
//!     uid: 1000,
//!     subuid: Some("mike:100000:65536\n".into()),
//!     subgid: Some("1000:100000:65536\n".into()),
//!     cgroup: Some("cpuset cpu io memory pids\n".into()),
//!     user_cgroup: Some("cpu memory pids\n".into()),
//!     image: Ok(format!("ghcr.io/o/i@sha256:{}", "0".repeat(64))),
//!     settings: Ok(Settings {
//!         path: "/home/mike/.claude/settings.json".into(),
//!         missing: Rules::default(),
//!     }),
//!     riff: Ok("/home/mike/.cargo/bin/riff is static".into()),
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
//! ```
//!
//! For tests, [`ETC_VAR`] names a folder that riff reads in place of
//! `/etc`, and [`CGROUP_VAR`] a folder in place of `/sys/fs/cgroup`.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use riff_core::build::Build;

use crate::permissions::{self, Rules};
use crate::service::{ServiceManager, Unit};
use crate::{daemon, text};

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

/// The time that riff waits for a new daemon to answer.
pub const WAIT: Duration = Duration::from_secs(10);

/// The facts of a machine that the first run checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// The first line of `podman --version`, or why riff cannot run
    /// podman.
    pub podman: Result<String, String>,
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
    /// The user settings of Claude Code, or why riff cannot find them.
    pub settings: Result<Settings, String>,
    /// What riff found of its own binary: static, or why it is not
    /// ([`crate::release::check_file`]).
    pub riff: Result<String, String>,
}

/// The user settings of Claude Code, and the permission rules of riff
/// that they lack (01M5084K09Z2S7N57315CX2R0F).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The file: `$CLAUDE_CONFIG_DIR/settings.json`, else
    /// `~/.claude/settings.json` ([`crate::plugin::user_settings`]).
    pub path: PathBuf,
    /// The rules of riff that the file does not have.
    pub missing: Rules,
}

impl Settings {
    /// The settings at `path`, with each rule of `rules` that they lack.
    /// A file that is not there, or that is not JSON, has no rule.
    ///
    /// ```
    /// use riff::first_run::Settings;
    /// use riff::permissions::Rules;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let path = dir.path().join("settings.json");
    /// let rules = Rules { allow: vec!["A".into(), "B".into()], deny: vec![] };
    /// assert_eq!(Settings::of(&path, &rules).missing, rules);
    /// std::fs::write(&path, r#"{"permissions": {"allow": ["A"]}}"#)?;
    /// assert_eq!(Settings::of(&path, &rules).missing.allow, ["B"]);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn of(path: &Path, rules: &Rules) -> Settings {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        Settings {
            path: path.to_owned(),
            missing: permissions::missing(rules, &[&text]),
        }
    }

    /// The user settings of the person who runs riff, with the rules of
    /// [`permissions::user_rules`].
    fn here() -> Result<Settings, String> {
        let path =
            crate::plugin::user_settings().ok_or_else(|| text::INIT_NO_SETTINGS.to_owned())?;
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from);
        Ok(Settings::of(
            &path,
            &permissions::user_rules(home.as_deref()),
        ))
    }
}

/// The folder in the variable `var`, else `default`.
fn dir_of(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from(default), PathBuf::from)
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
        Facts {
            podman: podman_version(),
            user,
            uid,
            subuid: read(etc.join("subuid")),
            subgid: read(etc.join("subgid")),
            cgroup: read(cgroup.join("cgroup.controllers")),
            user_cgroup: read(user_cgroup(&cgroup, uid)),
            image: crate::session_image::of_folder(dir)
                .map(|image| image.to_string())
                .map_err(|e| format!("{e:#}")),
            settings: Settings::here(),
            riff: riff_binary(),
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
}

impl Check {
    fn pass(name: &str, found: String) -> Check {
        Check {
            name: name.into(),
            found,
            fix: None,
        }
    }

    fn fail(name: &str, found: String, fix: String) -> Check {
        Check {
            name: name.into(),
            found,
            fix: Some(fix),
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
    let podman = match &facts.podman {
        Ok(_) if facts.uid == 0 => {
            Check::fail("podman", text::INIT_ROOT.into(), text::INIT_FIX_ROOT.into())
        }
        Ok(version) => Check::pass("podman", version.clone()),
        Err(why) => Check::fail("podman", why.clone(), text::INIT_FIX_PODMAN.into()),
    };
    let image = match &facts.image {
        Ok(image) => Check::pass("the session image", image.clone()),
        Err(why) => Check::fail(
            "the session image",
            why.clone(),
            text::INIT_FIX_IMAGE.into(),
        ),
    };
    let riff = match &facts.riff {
        Ok(found) => Check::pass(text::INIT_RIFF, found.clone()),
        Err(found) => Check::fail(text::INIT_RIFF, found.clone(), text::INIT_FIX_RIFF.into()),
    };
    vec![
        podman,
        sub_check("uid", facts.subuid.as_deref(), facts),
        sub_check("gid", facts.subgid.as_deref(), facts),
        cgroup_check(facts),
        image,
        rules_check(&facts.settings),
        riff,
    ]
}

/// The check of the permission rules of riff in the user settings of
/// Claude Code (01M5084K09Z2S7N57315CX2R0F).
fn rules_check(settings: &Result<Settings, String>) -> Check {
    match settings {
        Ok(s) if s.missing.is_empty() => {
            Check::pass(text::INIT_RULES, text::init_rules_there(&s.path))
        }
        Ok(s) => Check::fail(
            text::INIT_RULES,
            text::init_rules_missing(&s.path, s.missing.len()),
            text::INIT_FIX_RULES.into(),
        ),
        Err(why) => Check::fail(
            text::INIT_RULES,
            why.clone(),
            text::INIT_FIX_SETTINGS.into(),
        ),
    }
}

/// What `riff init` may change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `--check`: riff changes nothing and asks nothing.
    Check,
    /// riff asks one question before it changes the user settings of
    /// Claude Code.
    Ask,
    /// `--yes`: riff changes them with no question.
    Yes,
}

/// Asks the one question of `riff init` about the rules for the user
/// settings at `path`, when `terminal` is true
/// (01M5084K09Z2S7N57315CX2R0F). It writes the question to `out`, and
/// reads one line of `input`: `y` or `yes` is a yes. Each other answer,
/// the end of the input, and no terminal are a no.
///
/// ```
/// use riff::first_run::ask;
///
/// let path = std::path::Path::new("/h/s.json");
/// let mut out = Vec::new();
/// assert!(ask(path, true, &mut &b"y\n"[..], &mut out)?);
/// assert!(ask(path, true, &mut &b"Yes\n"[..], &mut out)?);
/// assert!(!ask(path, true, &mut &b"\n"[..], &mut out)?);
/// assert!(!ask(path, true, &mut &b""[..], &mut out)?);
/// assert_eq!(String::from_utf8(out)?, "Add them to /h/s.json? [y/N] ".repeat(4));
///
/// // No terminal: no question, and no.
/// let mut out = Vec::new();
/// assert!(!ask(path, false, &mut &b"y\n"[..], &mut out)?);
/// assert!(String::from_utf8(out)?.contains("riff init --yes"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn ask(
    path: &Path,
    terminal: bool,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<bool> {
    if !terminal {
        writeln!(out, "{}", text::INIT_NO_TERMINAL)?;
        return Ok(false);
    }
    write!(out, "{}", text::init_ask_rules(path))?;
    out.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// The first run on `facts`, with the daemon in the folder `dir`. It
/// prints each step, and gives the exit code: 1 when a check failed.
/// When rules are missing, it names them, and in [`Mode::Ask`] asks
/// `ask` with the path of the user settings. With [`Mode::Check`], it
/// stops after the checks. When no daemon answers, it installs `unit`
/// with `manager`, and waits up to `wait` for the daemon.
pub async fn run(
    facts: &Facts,
    mode: Mode,
    ask: impl FnOnce(&Path) -> Result<bool>,
    dir: &Path,
    manager: &impl ServiceManager,
    unit: &Unit,
    wait: Duration,
) -> Result<i32> {
    println!("{}", text::INIT_CHECKS);
    let mut checks = checks(facts);
    for check in &checks {
        println!("  {}", check.line());
    }
    if let Ok(settings) = &facts.settings
        && !settings.missing.is_empty()
    {
        println!("{}", text::init_rules(&settings.path, &settings.missing));
        let yes = match mode {
            Mode::Check => false,
            Mode::Ask => ask(&settings.path)?,
            Mode::Yes => true,
        };
        if yes {
            let added = permissions::add(&settings.path, &settings.missing)?;
            println!("{}", text::init_rules_added(&settings.path, added.len()));
            for check in checks.iter_mut().filter(|c| c.name == text::INIT_RULES) {
                *check = Check::pass(text::INIT_RULES, text::init_rules_there(&settings.path));
            }
        } else if mode == Mode::Ask {
            println!("{}", text::init_rules_kept(&settings.path));
        }
    }
    let failed = checks.iter().filter(|c| !c.passed()).count();
    if failed > 0 {
        println!("{}", text::init_failed(failed));
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
        let question = |path: &Path| {
            let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
            ask(
                path,
                terminal,
                &mut std::io::stdin().lock(),
                &mut std::io::stdout(),
            )
        };
        let dir = daemon::dir()?;
        run(&facts, mode, question, &dir, &manager, &unit, WAIT).await
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
            user: "mike".into(),
            uid: 1000,
            subuid: Some("mike:100000:65536\n".into()),
            subgid: Some("mike:100000:65536\n".into()),
            cgroup: Some("cpuset cpu io memory hugetlb pids rdma misc\n".into()),
            user_cgroup: Some("cpu memory pids\n".into()),
            image: Ok(IMAGE.into()),
            settings: Ok(Settings {
                path: "/home/mike/.claude/settings.json".into(),
                missing: Rules::default(),
            }),
            riff: Ok("/home/mike/.cargo/bin/riff is static".into()),
        }
    }

    /// The rules that `riff init` writes for the home `/home/mike`.
    fn user_rules() -> Rules {
        permissions::user_rules(Some("/home/mike".as_ref()))
    }

    /// A machine with each part, but with no rule of riff in the user
    /// settings at `path`.
    fn no_rules(path: &Path) -> Facts {
        Facts {
            settings: Ok(Settings::of(path, &user_rules())),
            ..ready()
        }
    }

    /// The question of a run that must ask nothing.
    fn no_question(path: &Path) -> Result<bool> {
        panic!("riff asked about {}", path.display())
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
                "pass  /etc/subuid: 65536 IDs for mike",
                "pass  /etc/subgid: 65536 IDs for mike",
                "pass  cgroup v2: user@1000.service has cpu and memory",
                &format!("pass  the session image: {IMAGE}"),
                "pass  the rules of Claude Code: /home/mike/.claude/settings.json has each \
                 permission rule of riff",
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
                "a missing rule",
                Facts {
                    settings: Ok(Settings {
                        path: "/home/mike/.claude/settings.json".into(),
                        missing: Rules {
                            allow: vec!["Bash(gh issue list *)".into()],
                            deny: vec![],
                        },
                    }),
                    ..ready()
                },
                "fail  the rules of Claude Code: /home/mike/.claude/settings.json lacks 1 \
                 permission rule of riff.",
                "riff init --yes",
            ),
            (
                "no user settings",
                Facts {
                    settings: Err(text::INIT_NO_SETTINGS.into()),
                    ..ready()
                },
                "fail  the rules of Claude Code: riff cannot find the user settings of Claude \
                 Code.",
                "CLAUDE_CONFIG_DIR",
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

    /// 01M5084K09Z2S7N57315CX2R0F, 01M5084K2Q3HH4BK7DJMYQD7C1: a yes adds
    /// each missing rule, also when another check failed.
    #[tokio::test]
    async fn a_yes_adds_the_rules_and_keeps_each_other_key() {
        let root = tempfile::tempdir().unwrap();
        let path = settings_in(
            root.path(),
            Some(r#"{"model": "opus", "permissions": {"allow": ["Bash(ls)"]}}"#),
        );
        // podman fails, so the run stops before the daemon.
        let facts = Facts {
            podman: Err(text::INIT_NO_PODMAN.into()),
            ..no_rules(&path)
        };
        assert_eq!(failed(&facts).len(), 2);
        let manager = Manager::default();
        let mut asked = None;
        let question = |p: &Path| {
            asked = Some(p.to_owned());
            Ok(true)
        };
        let dir = dir_in(root.path());
        let code = run(&facts, Mode::Ask, question, &dir, &manager, &unit(), WAIT)
            .await
            .unwrap();
        assert_eq!(code, 1);
        assert_eq!(asked.as_deref(), Some(path.as_path()));
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["model"], "opus");
        assert_eq!(value["permissions"]["allow"][0], "Bash(ls)");
        let after = Settings::of(&path, &user_rules());
        assert!(after.missing.is_empty(), "{after:?}");
        let now = Facts {
            settings: Ok(after),
            ..facts
        };
        assert_eq!(failed(&now).len(), 1, "only podman fails now");
        assert!(manager.0.lock().unwrap().is_empty());
    }

    /// 01M5084K09Z2S7N57315CX2R0F: `--yes` adds the rules with no
    /// question.
    #[tokio::test]
    async fn yes_adds_the_rules_with_no_question() {
        let root = tempfile::tempdir().unwrap();
        let path = settings_in(root.path(), None);
        let facts = Facts {
            podman: Err(text::INIT_NO_PODMAN.into()),
            ..no_rules(&path)
        };
        let dir = dir_in(root.path());
        let code = run(
            &facts,
            Mode::Yes,
            no_question,
            &dir,
            &Manager::default(),
            &unit(),
            WAIT,
        )
        .await
        .unwrap();
        assert_eq!(code, 1);
        assert!(Settings::of(&path, &user_rules()).missing.is_empty());
    }

    /// 01M5084K09Z2S7N57315CX2R0F: a no changes nothing, and the check of
    /// the rules fails, so riff installs no daemon.
    #[tokio::test]
    async fn a_no_changes_nothing_and_installs_nothing() {
        let root = tempfile::tempdir().unwrap();
        let text = r#"{"model": "opus"}"#;
        let path = settings_in(root.path(), Some(text));
        let facts = no_rules(&path);
        assert_eq!(failed(&facts).len(), 1);
        let manager = Manager::default();
        let dir = dir_in(root.path());
        let code = run(
            &facts,
            Mode::Ask,
            |_| Ok(false),
            &dir,
            &manager,
            &unit(),
            WAIT,
        )
        .await
        .unwrap();
        assert_eq!(code, 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert!(manager.0.lock().unwrap().is_empty());
        assert!(!dir.exists());
    }

    /// 01M5084K09Z2S7N57315CX2R0F: `--check` names the missing rules,
    /// asks nothing, and fails.
    #[tokio::test]
    async fn the_check_only_run_fails_on_missing_rules_and_writes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let path = settings_in(root.path(), None);
        let manager = Manager::default();
        let dir = dir_in(root.path());
        let code = run(
            &no_rules(&path),
            Mode::Check,
            no_question,
            &dir,
            &manager,
            &unit(),
            WAIT,
        )
        .await
        .unwrap();
        assert_eq!(code, 1);
        assert!(!path.exists());
        assert!(manager.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_daemon_that_answers_stays_and_pulls_the_image() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let fake = Fake::default().with_registry(IMAGE);
        let listener = listen(&dir).await.unwrap();
        let daemon = Arc::new(Daemon::open(fake.clone(), dir.clone()).await.unwrap());
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
        )
        .await
        .unwrap();
        assert_eq!(code, 0);
        assert_eq!(fake.calls(), [format!("pull {IMAGE}")]);

        let _ = stop.send(());
        served.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn an_image_that_the_daemon_cannot_pull_fails_the_run() {
        let root = tempfile::tempdir().unwrap();
        let dir = dir_in(root.path());
        let fake = Fake::default();
        let listener = listen(&dir).await.unwrap();
        let daemon = Arc::new(Daemon::open(fake.clone(), dir.clone()).await.unwrap());
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
        )
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains(IMAGE), "{error:#}");

        let _ = stop.send(());
        served.await.unwrap().unwrap();
    }
}
