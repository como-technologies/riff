//! The steps of an update of riff on a machine (#855).
//!
//! # Design
//!
//! The update by itself puts the binary of each new release in place
//! ([`crate::auto_update`]), and the riff daemon starts again on it
//! (01M517BTP6BAGTRBYWPX9GPVE0). But `riff init` also does steps that a
//! release can change. The opt-in to the update by itself,
//! `riff update --auto on`, is the consent of the person for each step
//! of an update. So at each start, the riff daemon of a release does
//! each step of `riff init` that needs no person and that is not in step
//! ([`at_start`], 01M51QFPJAABDP1V7WFY8NQR60):
//!
//! | Step | In step when | Else the daemon |
//! |---|---|---|
//! | the rules of Claude Code | the user settings have each rule of riff ([`crate::permissions::user_rules`]) | adds the missing rules ([`crate::rule_sync`]) |
//! | the unit of the daemon | the unit file is the text that this riff writes for its `ExecStart` and its variables ([`Unit::text`]) | writes it again, and starts again on it when it is idle (01M51QFPQ68JHS08TWXNNWZ0D7) |
//! | the session image of each wave | podman has the image that `riff.toml` of the main clone of the wave pins | pulls it (01M51QFPSMDDMFPZXAEYWZYJPV) |
//!
//! The gate ([`gate`]). The daemon changes the machine only when
//! `update.auto` is on and its own binary is a release: the SHA-256 of
//! `/proc/self/exe` is the line of `riff-TARGET` in `SHA256SUMS` of the
//! GitHub release of its version ([`release::is_release`]). Each step
//! that changes the machine trusts only GitHub
//! (01M51QFPMSG81YRDDGQQ1WJDSQ): with `RIFF_RELEASES` set, the gate stays
//! shut, and its line says why ([`rule_sync::sums_source`]). So a
//! session cannot build a branch, name its own `SHA256SUMS`, and get the
//! steps of that branch done. A check build or a dev build changes
//! nothing.
//!
//! A step that needs a person is never done by the daemon: the checks
//! of the machine of `riff init`, which are podman, the init of podman,
//! `/etc/subuid`, `/etc/subgid` and cgroup v2
//! ([`first_run::machine_checks`]). Their fix needs `sudo`
//! (01M51QFPW37EXVH4AXH70MSXBT). The daemon keeps the line of the first
//! one, with its fix, in the local files ([`local::needs_you`]). The
//! status line shows it, and `riff update --check` lists each step.
//!
//! ```mermaid
//! flowchart TD
//!     S["the riff daemon starts"] --> G{"update.auto on, and its binary<br/>is in SHA256SUMS of the<br/>GitHub release of its version?"}
//!     G -- "no: off, a dev build,<br/>or RIFF_RELEASES set" --> L["it changes nothing:<br/>one line for each step"]
//!     G -- "yes" --> R["the rules: add the missing rules"]
//!     R --> U["the unit: write it again,<br/>start again when idle"]
//!     U --> I["the image of each wave: pull it"]
//!     I --> P["the checks of the machine:<br/>never a change"]
//!     L --> P
//!     P --> N["the first step that needs a person:<br/>the status line, riff update --check"]
//! ```
//!
//! `riff update --check` ([`check`]) looks at each step and changes
//! nothing (01M51QFPYHK0916C5ZN2B55BR0): the binary, the rules, the unit,
//! the session image of each wave and of the repository of the current
//! folder, and the checks of the machine. It prints one line for each
//! step, keeps the step that needs a person for the status line, and
//! exits with 1 when a step is not in step.
//!
//! ```
//! use riff::update_steps::{State, Step};
//!
//! let step = |state| Step { name: "the init of podman".into(), state };
//! assert!(step(State::InStep("/usr/libexec/podman/catatonit".into())).in_step());
//! assert!(step(State::Done("riff pulled it".into())).in_step());
//! let person = step(State::Person { found: "no catatonit".into(), fix: "sudo apt install catatonit".into() });
//! assert!(!person.in_step());
//! assert_eq!(
//!     person.line(),
//!     "needs you  the init of podman: no catatonit. Fix: sudo apt install catatonit"
//! );
//! ```

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use riff_core::build::Build;

use crate::first_run::{self, Check};
use crate::release::{self, Source};
use crate::rule_sync;
use crate::runtime::Runtime;
use crate::service::{ServiceManager, Unit};
use crate::session_image::Image;
use crate::{lifecycle, local, permissions, settings, text};

/// A riff binary, and the place of its release files.
#[derive(Debug, Clone, Copy)]
pub struct Release<'a> {
    /// The riff binary.
    pub binary: &'a Path,
    /// The release tag of its version, for example `v1.5.0`.
    pub tag: &'a str,
    /// The place of the release files, or why riff trusts no place
    /// ([`rule_sync::sums_source`]).
    pub source: &'a Result<Source, String>,
    /// The target of the static binaries of this machine.
    pub target: Option<&'a str>,
}

/// The gate of the steps that change the machine
/// (01M51QFPJAABDP1V7WFY8NQR60).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    /// `update.auto` is off.
    Off,
    /// The binary is not a release, or riff trusts no place of its
    /// release files. It holds why.
    NotRelease(String),
    /// `update.auto` is on, and the binary is the release of this tag.
    Open(String),
}

impl Gate {
    /// True when the steps may change the machine.
    pub fn open(&self) -> bool {
        matches!(self, Gate::Open(_))
    }
}

/// Checks `release`: `Ok` when its place of the release files is
/// trusted and its binary is the release of its tag
/// ([`release::is_release`]), else why not. An error when riff cannot
/// read the binary.
pub async fn is_release(release: &Release<'_>) -> Result<Result<(), String>> {
    let source = match release.source {
        Ok(source) => source,
        Err(why) => return Ok(Err(why.clone())),
    };
    let bytes = std::fs::read(release.binary)
        .with_context(|| format!("cannot read {}", release.binary.display()))?;
    Ok(release::is_release(&bytes, release.tag, source, release.target).await)
}

/// The gate of `release` on a machine with `update.auto` = `auto`
/// (01M51QFPJAABDP1V7WFY8NQR60, 01M51QFPMSG81YRDDGQQ1WJDSQ).
///
/// ```
/// use riff::release::{asset, sha256, Source};
/// use riff::update_steps::{gate, Gate, Release};
///
/// let dir = tempfile::tempdir()?;
/// let target = "x86_64-unknown-linux-musl";
/// std::fs::create_dir(dir.path().join("v1.5.0"))?;
/// let line = format!("{}  {}\n", sha256(b"the riff"), asset("riff", target));
/// std::fs::write(dir.path().join("v1.5.0/SHA256SUMS"), line)?;
/// let binary = dir.path().join("riff");
/// std::fs::write(&binary, b"the riff")?;
/// let run = |auto, source: Result<Source, String>| {
///     let release = Release { binary: &binary, tag: "v1.5.0", source: &source, target: Some(target) };
///     let gate = gate(auto, &release);
///     tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(gate).unwrap()
/// };
/// let folder = || Ok(Source::Folder(dir.path().into()));
/// assert_eq!(run(true, folder()), Gate::Open("v1.5.0".into()));
/// assert_eq!(run(false, folder()), Gate::Off);
/// // A place that riff does not trust: the gate stays shut.
/// assert_eq!(run(true, Err("RIFF_RELEASES is set".into())), Gate::NotRelease("RIFF_RELEASES is set".into()));
/// // A dev build.
/// std::fs::write(&binary, b"a dev build")?;
/// assert!(matches!(run(true, folder()), Gate::NotRelease(why) if why.ends_with("a dev build")));
/// # Ok::<(), std::io::Error>(())
/// ```
pub async fn gate(auto: bool, release: &Release<'_>) -> Result<Gate> {
    if !auto {
        return Ok(Gate::Off);
    }
    Ok(match is_release(release).await? {
        Ok(()) => Gate::Open(release.tag.to_owned()),
        Err(why) => Gate::NotRelease(why),
    })
}

/// The trusted place of the release files of this process
/// ([`rule_sync::sums_source`]): the GitHub releases, or why riff trusts
/// no place. And the release tag of its version.
fn source_here() -> (Result<Source, String>, String) {
    let source = rule_sync::sums_source(std::env::var(release::RELEASES).ok().as_deref());
    (source, lifecycle::release_tag(env!("CARGO_PKG_VERSION")))
}

/// The binary of this process with `source` and `tag` of
/// [`source_here`].
fn release_here<'a>(source: &'a Result<Source, String>, tag: &'a str) -> Release<'a> {
    Release {
        // The binary that runs, also after a new riff replaced its file.
        binary: Path::new("/proc/self/exe"),
        tag,
        source,
        target: release::TARGET,
    }
}

/// The gate of this machine and process.
pub async fn gate_here() -> Gate {
    let (source, tag) = source_here();
    gate(auto_here(), &release_here(&source, &tag))
        .await
        .unwrap_or_else(|e| Gate::NotRelease(format!("{e:#}")))
}

/// `update.auto` of this machine. A bad settings file counts as off.
pub fn auto_here() -> bool {
    settings::path().is_ok_and(|p| settings::update_auto(&p).unwrap_or(false))
}

/// The state of one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// It is in step. It holds what riff found.
    InStep(String),
    /// The daemon brought it in step now. It holds what it did.
    Done(String),
    /// It is not in step: what riff found, and the fix.
    Behind {
        /// What riff found.
        found: String,
        /// The fix.
        fix: String,
    },
    /// It is not in step, and only a person can do its fix
    /// (01M51QFPW37EXVH4AXH70MSXBT): what riff found, and the fix.
    Person {
        /// What riff found.
        found: String,
        /// The fix.
        fix: String,
    },
    /// riff cannot check it, or its change failed. It holds why.
    Failed(String),
}

/// One step of an update: its name and its state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The name, for example `the unit of the daemon`.
    pub name: String,
    /// The state.
    pub state: State,
}

impl Step {
    /// True when the step is in step, also after a change now.
    pub fn in_step(&self) -> bool {
        matches!(self.state, State::InStep(_) | State::Done(_))
    }

    /// True when only a person can do the fix of the step.
    pub fn needs_you(&self) -> bool {
        matches!(self.state, State::Person { .. })
    }

    /// The line for people ([`text::step_line`]).
    pub fn line(&self) -> String {
        text::step_line(&self.name, &self.state)
    }
}

/// The step of the rules of Claude Code in the user settings at `user`.
/// It changes nothing: the sync of the rules adds them
/// ([`rule_sync`], 01M51M3ZHHBE61CEG9T6ENFG18).
///
/// ```
/// use riff::update_steps::{rules, State};
///
/// let dir = tempfile::tempdir()?;
/// let user = dir.path().join("settings.json");
/// let step = rules(&user, None);
/// assert!(matches!(&step.state, State::Behind { fix, .. } if fix == "riff init --yes"));
/// let want = riff::permissions::user_rules(None);
/// riff::permissions::add(&user, &want)?;
/// assert!(rules(&user, None).in_step());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn rules(user: &Path, home: Option<&Path>) -> Step {
    let want = permissions::user_rules(home);
    let missing = first_run::Settings::of(user, &want).missing;
    let state = if missing.is_empty() {
        State::InStep(text::init_rules_there(user))
    } else {
        State::Behind {
            found: text::init_rules_missing(user, missing.len()),
            fix: text::STEP_FIX_RULES.to_owned(),
        }
    };
    Step {
        name: text::INIT_RULES.to_owned(),
        state,
    }
}

/// The step of the unit of the daemon of `manager`
/// (01M51QFPQ68JHS08TWXNNWZ0D7). It is in step when the unit file is the
/// text that this riff writes for the `ExecStart` and the variables of
/// the file ([`Unit::text`]). With `change`, it writes that text and
/// reloads the manager ([`ServiceManager::rewrite`]).
pub fn unit(manager: &impl ServiceManager, change: bool) -> Step {
    let behind = |found: &str| State::Behind {
        found: found.to_owned(),
        fix: text::STEP_FIX_UNIT.to_owned(),
    };
    let state = match manager.installed_text() {
        Ok(None) => behind(text::STEP_NO_UNIT),
        Err(e) => State::Failed(format!("{e:#}")),
        Ok(Some(old)) => match Unit::parse(&old) {
            None => State::Failed(text::STEP_UNIT_UNREADABLE.to_owned()),
            Some(unit) => match unit.text() {
                Err(e) => State::Failed(format!("{e:#}")),
                Ok(new) if new == old => State::InStep(text::step_unit_there(&unit.exe)),
                Ok(_) if !change => behind(text::STEP_UNIT_OLD),
                Ok(_) => match manager.rewrite(&unit) {
                    Ok(path) => State::Done(text::step_unit_written(&path)),
                    Err(e) => State::Failed(format!("{e:#}")),
                },
            },
        },
    };
    Step {
        name: text::STEP_UNIT.to_owned(),
        state,
    }
}

/// The step of the session image `image` of the repository `repo`
/// (01M51QFPSMDDMFPZXAEYWZYJPV). It is in step when `runtime` has the
/// image. With `change`, it pulls it.
pub async fn image(runtime: &impl Runtime, repo: &str, image: &str, change: bool) -> Step {
    let state = match runtime.has_image(image).await {
        Ok(true) => State::InStep(image.to_owned()),
        Ok(false) if !change => State::Behind {
            found: text::step_no_image(image),
            fix: text::step_fix_image(image),
        },
        Ok(false) => match runtime.pull(image).await {
            Ok(()) => State::Done(text::step_pulled(image)),
            Err(e) => State::Failed(format!("{}: {e:#}", text::run_no_image(image))),
        },
        Err(e) => State::Failed(format!("{e:#}")),
    };
    Step {
        name: text::step_image(repo),
        state,
    }
}

/// The steps of the checks of the machine (01M51QFPW37EXVH4AXH70MSXBT):
/// a check that passed is in step, and a check that failed needs a
/// person.
///
/// ```
/// use riff::first_run::Check;
/// use riff::update_steps::{machine, State};
///
/// let checks = [
///     Check { name: "podman".into(), found: "podman version 5.7.0".into(), fix: None },
///     Check { name: "the init of podman".into(), found: "no catatonit".into(), fix: Some("sudo apt install catatonit".into()) },
/// ];
/// let steps = machine(&checks);
/// assert_eq!(steps[0].state, State::InStep("podman version 5.7.0".into()));
/// assert!(steps[1].needs_you());
/// ```
pub fn machine(checks: &[Check]) -> Vec<Step> {
    checks
        .iter()
        .map(|check| Step {
            name: check.name.clone(),
            state: match &check.fix {
                None => State::InStep(check.found.clone()),
                Some(fix) => State::Person {
                    found: check.found.clone(),
                    fix: fix.clone(),
                },
            },
        })
        .collect()
}

/// The session image of each wave of the daemon in the folder `dir`
/// ([`crate::wave::read_waves`]): the repository, and the image that
/// `riff.toml` of its main clone pins, or why riff has none.
pub fn wave_images(dir: &Path) -> Vec<(String, Result<String, String>)> {
    let Ok(waves) = crate::wave::read_waves(dir) else {
        return Vec::new();
    };
    waves
        .into_values()
        .map(|wave| {
            let image = Image::of_repository(&wave.main)
                .map(|image| image.to_string())
                .map_err(|e| format!("{e:#}"));
            (wave.repo, image)
        })
        .collect()
}

/// What the steps look at on a machine.
pub struct Machine<'a, M, R> {
    /// The user settings of Claude Code, or `None` when riff cannot find
    /// them.
    pub user: Option<&'a Path>,
    /// The home of the person, for `{HOME}` in a rule.
    pub home: Option<&'a Path>,
    /// The service manager of the unit of the daemon, or `None` when the
    /// daemon has no unit, for example with `RIFF_HOME`.
    pub manager: Option<&'a M>,
    /// The runtime of the sessions.
    pub runtime: &'a R,
    /// The session images: the repository, and the image or why riff
    /// has none ([`wave_images`]).
    pub images: Vec<(String, Result<String, String>)>,
    /// The checks of the machine ([`first_run::machine_checks`]).
    pub checks: Vec<Check>,
}

/// Each step on `at`, in order: [`rules`], [`unit`], [`image`] and
/// [`machine`]. Only with an open `gate` do the unit and the images
/// change (01M51QFPJAABDP1V7WFY8NQR60). The sync of the rules runs before
/// it ([`rule_sync::sync`]).
pub async fn sync<M: ServiceManager, R: Runtime>(gate: &Gate, at: &Machine<'_, M, R>) -> Vec<Step> {
    let change = gate.open();
    let mut steps = vec![match at.user {
        Some(user) => rules(user, at.home),
        None => Step {
            name: text::INIT_RULES.to_owned(),
            state: State::Failed(text::INIT_NO_SETTINGS.to_owned()),
        },
    }];
    if let Some(manager) = at.manager {
        steps.push(unit(manager, change));
    }
    for (repo, found) in &at.images {
        steps.push(match found {
            Ok(found) => image(at.runtime, repo, found, change).await,
            Err(why) => Step {
                name: text::step_image(repo),
                state: State::Failed(why.clone()),
            },
        });
    }
    steps.extend(machine(&at.checks));
    steps
}

/// The line of the first step of `steps` that needs a person, with the
/// count of the others (01M51QFPW37EXVH4AXH70MSXBT).
///
/// ```
/// use riff::update_steps::{needs_you, State, Step};
///
/// let person = |name: &str| Step {
///     name: name.into(),
///     state: State::Person { found: "missing".into(), fix: "sudo apt install it".into() },
/// };
/// assert_eq!(needs_you(&[]), None);
/// assert_eq!(needs_you(&[person("podman")]).unwrap(), "podman: sudo apt install it");
/// assert_eq!(
///     needs_you(&[person("podman"), person("cgroup v2")]).unwrap(),
///     "podman: sudo apt install it (and 1 more step: riff update --check)"
/// );
/// ```
pub fn needs_you(steps: &[Step]) -> Option<String> {
    let mut person = steps.iter().filter(|s| s.needs_you());
    let first = person.next()?;
    let State::Person { fix, .. } = &first.state else {
        return None;
    };
    Some(text::needs_you(&first.name, fix, person.count()))
}

/// True after the step of the unit of this daemon wrote the unit file
/// (01M51QFPQ68JHS08TWXNNWZ0D7).
static UNIT_WRITTEN: AtomicBool = AtomicBool::new(false);

/// True when the sync of this daemon wrote its unit file, so the daemon
/// starts again on the new unit when it is idle ([`crate::daemon`]).
pub fn unit_written() -> bool {
    UNIT_WRITTEN.load(Ordering::Relaxed)
}

/// The home of the person, for `{HOME}` in a rule.
fn home_here() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// The service manager of the unit of the daemon of this machine. None
/// with `RIFF_HOME`: that daemon is never the daemon of the unit.
fn manager_here() -> Option<crate::service::Systemd> {
    crate::service::Systemd::here()
        .ok()
        .filter(|_| crate::home::dir().is_none())
}

/// The steps on this machine with `runtime` and the session `images`,
/// behind `gate`.
async fn sync_here(gate: &Gate, runtime: &impl Runtime, images: Vec<(String, Result<String, String>)>) -> Vec<Step> {
    let user = crate::plugin::user_settings();
    let home = home_here();
    let manager = manager_here();
    let facts = first_run::Facts::here(&lifecycle::run_dir());
    let at = Machine {
        user: user.as_deref(),
        home: home.as_deref(),
        manager: manager.as_ref(),
        runtime,
        images,
        checks: first_run::machine_checks(&facts),
    };
    sync(gate, &at).await
}

/// Keeps the step of `steps` that needs a person in the local files,
/// for the status line ([`local::set_needs_you`]).
fn keep_needs_you(steps: &[Step]) {
    if let Some(dir) = local::dir()
        && let Err(e) = local::set_needs_you(&dir, needs_you(steps).as_deref())
    {
        eprintln!("riff: cannot keep the step that needs you: {e:#}");
    }
}

/// The steps at each start of the riff daemon in the folder `daemon` of
/// this machine, with `runtime` (01M51QFPJAABDP1V7WFY8NQR60). First the
/// sync of the rules ([`rule_sync::at_start`]), then the gate and each
/// step. It logs the gate and each step, and keeps the step that needs
/// a person. After a write of the unit, the daemon starts again when it
/// is idle ([`unit_written`]). It never fails the daemon.
pub async fn at_start(runtime: &impl Runtime, daemon: &Path) {
    rule_sync::at_start().await;
    let gate = gate_here().await;
    println!("{}", text::steps_gate(&Build::this().to_string(), &gate));
    let steps = sync_here(&gate, runtime, wave_images(daemon)).await;
    for step in &steps {
        println!("{}", step.line());
    }
    if steps
        .iter()
        .any(|s| s.name == text::STEP_UNIT && matches!(s.state, State::Done(_)))
    {
        UNIT_WRITTEN.store(true, Ordering::Relaxed);
    }
    keep_needs_you(&steps);
}

/// `riff update --check` (01M51QFPYHK0916C5ZN2B55BR0): it looks at each
/// step on this machine and changes nothing, with the daemon in the
/// folder `daemon`, the current folder `here` and `runtime`. It prints
/// the gate and one line for each step, and keeps the step that needs a
/// person for the status line. It gives the exit code: 1 when a step is
/// not in step.
pub async fn check(runtime: &impl Runtime, daemon: &Path, here: &Path) -> i32 {
    let gate = gate_here().await;
    println!("{}", text::steps_check_gate(&gate));
    let (source, tag) = source_here();
    let release = is_release(&release_here(&source, &tag))
        .await
        .unwrap_or_else(|e| Err(format!("{e:#}")));
    let binary = Step {
        name: text::STEP_BINARY.to_owned(),
        state: match release {
            Ok(()) => State::InStep(text::step_binary_release(&tag)),
            Err(why) => State::Behind {
                found: why,
                fix: text::STEP_FIX_BINARY.to_owned(),
            },
        },
    };
    let mut images = wave_images(daemon);
    if let Ok(found) = crate::session_image::of_folder(here) {
        let found = found.to_string();
        if !images.iter().any(|(_, i)| i.as_ref() == Ok(&found)) {
            images.push((text::STEP_HERE.to_owned(), Ok(found)));
        }
    }
    let mut steps = vec![binary];
    steps.extend(sync_here(&Gate::Off, runtime, images).await);
    for step in &steps {
        println!("{}", step.line());
    }
    keep_needs_you(&steps);
    i32::from(!steps.iter().all(Step::in_step))
}
