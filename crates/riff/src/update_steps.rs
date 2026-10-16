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
//! | the unit of the daemon | the unit file is the text that this riff writes for its `ExecStart` and its variables | writes it again, and starts again on it when it is idle (01M51QFPQ68JHS08TWXNNWZ0D7) |
//! | the session image of each wave | podman has the image that `riff.toml` of the main clone of the wave pins | pulls it (01M51QFPSMDDMFPZXAEYWZYJPV) |
//!
//! The gate. The daemon changes the machine only when `update.auto` is
//! on and its own binary is a release ([`gate`]): the SHA-256 of
//! `/proc/self/exe` is the line of `riff-TARGET` in `SHA256SUMS` of the
//! GitHub release of its version ([`release::is_release`]). A step that
//! changes the machine trusts only GitHub. With `RIFF_RELEASES` set, the
//! gate stays shut, and its line says why ([`release_here`],
//! 01M51QFPMSG81YRDDGQQ1WJDSQ). So a session cannot build a branch, name
//! its own `SHA256SUMS`, and get the steps of that branch done. A check
//! build or a dev build changes nothing.
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
//! let step = |state| Step { name: "the unit of the daemon".into(), state };
//! assert!(step(State::InStep("riff.service".into())).in_step());
//! assert!(step(State::Done("riff wrote riff.service".into())).in_step());
//! let person = step(State::Person { found: "no catatonit".into(), fix: "sudo apt install catatonit".into() });
//! assert!(!person.in_step());
//! assert_eq!(
//!     person.line(),
//!     "needs you  the unit of the daemon: no catatonit. Fix: sudo apt install catatonit"
//! );
//! ```

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use riff_core::build::Build;

use crate::first_run::{self, Check};
use crate::permissions::{self, Rules};
use crate::release::{self, Source};
use crate::rule_sync::{self, Synced};
use crate::runtime::Runtime;
use crate::service::{ServiceManager, Unit};
use crate::session_image::Image;
use crate::{lifecycle, local, settings, text};

/// A riff binary, and where its release files are.
#[derive(Debug, Clone, Copy)]
pub struct Release<'a> {
    /// The riff binary.
    pub binary: &'a Path,
    /// The release tag of its version, for example `v1.5.0`.
    pub tag: &'a str,
    /// The place of the release files.
    pub source: &'a Source,
    /// The target of the static binaries of this machine.
    pub target: Option<&'a str>,
}

/// The gate of the steps that change the machine
/// (01M51QFPJAABDP1V7WFY8NQR60).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    /// `update.auto` is off.
    Off,
    /// The binary is not a release. It holds why.
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

/// Checks that `release.binary` is the release `release.tag`
/// ([`release::is_release`]). `Err` says why it is not.
pub async fn is_release(release: &Release<'_>) -> Result<(), String> {
    let bytes = std::fs::read(release.binary)
        .map_err(|e| text::not_release_binary(release.binary, &e.to_string()))?;
    release::is_release(&bytes, release.tag, release.source, release.target).await
}

/// The gate of `release` for a machine with `update.auto` = `auto`.
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
/// std::fs::write(dir.path().join("riff"), b"the riff")?;
/// let source = Source::Folder(dir.path().into());
/// let binary = dir.path().join("riff");
/// let release = Release { binary: &binary, tag: "v1.5.0", source: &source, target: Some(target) };
/// let run = |auto| {
///     tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(gate(auto, &release))
/// };
/// assert_eq!(run(true), Gate::Open("v1.5.0".into()));
/// assert_eq!(run(false), Gate::Off);
/// std::fs::write(&binary, b"a dev build")?;
/// assert!(matches!(run(true), Gate::NotRelease(why) if why.ends_with("a dev build")));
/// # Ok::<(), std::io::Error>(())
/// ```
pub async fn gate(auto: bool, release: &Release<'_>) -> Gate {
    if !auto {
        return Gate::Off;
    }
    match is_release(release).await {
        Ok(()) => Gate::Open(release.tag.to_owned()),
        Err(why) => Gate::NotRelease(why),
    }
}

/// Checks that the binary of this process is a release: its SHA-256 is
/// in `SHA256SUMS` of the GitHub release of its version. It gives the
/// release tag, or why the binary is not a release. With
/// [`release::RELEASES`] set, it never reads another place: it gives
/// [`text::NOT_RELEASE_RELEASES`] (01M51QFPMSG81YRDDGQQ1WJDSQ).
pub async fn release_here() -> Result<String, String> {
    if std::env::var_os(release::RELEASES).is_some() {
        return Err(text::NOT_RELEASE_RELEASES.to_owned());
    }
    let tag = lifecycle::release_tag(env!("CARGO_PKG_VERSION"));
    let release = Release {
        // The binary that runs, also after a new riff replaced its file.
        binary: Path::new("/proc/self/exe"),
        tag: &tag,
        source: &Source::GitHub,
        target: release::TARGET,
    };
    is_release(&release).await.map(|()| tag)
}

/// The gate of this machine and process.
pub async fn gate_here() -> Gate {
    if !auto_here() {
        return Gate::Off;
    }
    match release_here().await {
        Ok(tag) => Gate::Open(tag),
        Err(why) => Gate::NotRelease(why),
    }
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

/// The step of the rules of Claude Code in the user settings at `user`
/// (01M51M3ZHHBE61CEG9T6ENFG18). With `change`, it adds the missing
/// rules ([`rule_sync::add`]). It gives the step and the rules that it
/// added.
pub fn rules(user: &Path, home: Option<&Path>, change: bool) -> (Step, Rules) {
    let name = text::INIT_RULES.to_owned();
    let want = permissions::user_rules(home);
    let missing = first_run::Settings::of(user, &want).missing;
    let state = if missing.is_empty() {
        State::InStep(text::init_rules_there(user))
    } else if !change {
        State::Behind {
            found: text::init_rules_missing(user, missing.len()),
            fix: text::STEP_FIX_RULES.to_owned(),
        }
    } else {
        match rule_sync::add(user, home) {
            Ok(added) => {
                let state = State::Done(text::step_rules_added(user, added.len()));
                return (Step { name, state }, added);
            }
            Err(e) => State::Failed(format!("{e:#}")),
        }
    };
    (Step { name, state }, Rules::default())
}

/// The step of the unit of the daemon of `manager`
/// (01M51QFPQ68JHS08TWXNNWZ0D7). It is in step when the unit file is the
/// text that this riff writes for the `ExecStart` and the variables of
/// the file ([`Unit::text`]). With `change`, it writes that text and
/// reloads the manager ([`ServiceManager::rewrite`]).
pub fn unit(manager: &impl ServiceManager, change: bool) -> Step {
    let name = text::STEP_UNIT.to_owned();
    let behind = |found: &str| State::Behind {
        found: found.to_owned(),
        fix: text::STEP_FIX_UNIT.to_owned(),
    };
    let old = match manager.installed_text() {
        Ok(Some(old)) => old,
        Ok(None) => {
            let state = behind(text::STEP_NO_UNIT);
            return Step { name, state };
        }
        Err(e) => {
            let state = State::Failed(format!("{e:#}"));
            return Step { name, state };
        }
    };
    let Some(unit) = Unit::parse(&old) else {
        let state = State::Failed(text::STEP_UNIT_UNREADABLE.to_owned());
        return Step { name, state };
    };
    let state = match unit.text() {
        Err(e) => State::Failed(format!("{e:#}")),
        Ok(new) if new == old => State::InStep(text::step_unit_there(&unit.exe)),
        Ok(_) if !change => behind(text::STEP_UNIT_OLD),
        Ok(_) => match manager.rewrite(&unit) {
            Ok(path) => State::Done(text::step_unit_written(&path)),
            Err(e) => State::Failed(format!("{e:#}")),
        },
    };
    Step { name, state }
}

/// The step of the session image `image` of the repository `repo`
/// (01M51QFPSMDDMFPZXAEYWZYJPV). It is in step when `runtime` has the
/// image. With `change`, it pulls it.
pub async fn image(runtime: &impl Runtime, repo: &str, image: &str, change: bool) -> Step {
    let name = text::step_image(repo);
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
    Step { name, state }
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

/// What [`sync`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synced {
    /// Each step, in order.
    pub steps: Vec<Step>,
    /// The rules that the step of the rules added.
    pub added: Rules,
    /// True when the step of the unit wrote the unit file.
    pub unit_written: bool,
}

impl Synced {
    /// The line of the first step that needs a person, if any.
    pub fn needs_you(&self) -> Option<String> {
        needs_you(&self.steps)
    }
}

/// The line of the first step of `steps` that needs a person, with the
/// count of the others (01M51QFPW37EXVH4AXH70MSXBT).
pub fn needs_you(steps: &[Step]) -> Option<String> {
    let mut person = steps.iter().filter(|s| s.needs_you());
    let first = person.next()?;
    Some(text::needs_you(first, person.count()))
}

/// Each step on `machine` ([`rules`], [`unit`], [`image`], [`machine`]),
/// in order. Only with an open `gate` does a step change the machine
/// (01M51QFPJAABDP1V7WFY8NQR60).
pub async fn sync<M: ServiceManager, R: Runtime>(gate: &Gate, at: &Machine<'_, M, R>) -> Synced {
    let change = gate.open();
    let mut steps = Vec::new();
    let mut added = Rules::default();
    match at.user {
        Some(user) => {
            let (step, rules) = rules(user, at.home, change);
            steps.push(step);
            added = rules;
        }
        None => steps.push(Step {
            name: text::INIT_RULES.to_owned(),
            state: State::Failed(text::INIT_NO_SETTINGS.to_owned()),
        }),
    }
    let mut unit_written = false;
    if let Some(manager) = at.manager {
        let step = unit(manager, change);
        unit_written = matches!(step.state, State::Done(_));
        steps.push(step);
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
    Synced {
        steps,
        added,
        unit_written,
    }
}

/// True after the step of the unit of this daemon wrote the unit file:
/// the daemon starts again on it when it is idle
/// (01M51QFPQ68JHS08TWXNNWZ0D7).
static UNIT_WRITTEN: AtomicBool = AtomicBool::new(false);

/// True when the sync of this daemon wrote its unit file, so the daemon
/// starts again on the new unit ([`crate::daemon`]).
pub fn unit_written() -> bool {
    UNIT_WRITTEN.load(Ordering::Relaxed)
}

/// The machine of this process: its user settings, its unit with
/// `manager`, the images of the waves of the daemon in the folder
/// `daemon`, and the checks of the machine.
fn machine_here<'a, M, R>(
    user: Option<&'a Path>,
    home: Option<&'a Path>,
    manager: Option<&'a M>,
    runtime: &'a R,
    images: Vec<(String, Result<String, String>)>,
) -> Machine<'a, M, R> {
    let facts = first_run::Facts::here(&lifecycle::run_dir());
    Machine {
        user,
        home,
        manager,
        runtime,
        images,
        checks: first_run::machine_checks(&facts),
    }
}

/// The home of the person, for `{HOME}` in a rule.
fn home_here() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// The sync at each start of the riff daemon in the folder `daemon` of
/// this machine, with `runtime` (01M51QFPJAABDP1V7WFY8NQR60). It logs
/// its gate, each rule that it adds and each step. It keeps the line of
/// the rules for `riff init` (01M51M3ZPAEMJ64266QSJBNQ46) and the step
/// that needs a person ([`local::needs_you`]). After an add of rules, it
/// marks the add ([`rule_sync::mark_added`]). It never fails the daemon.
pub async fn at_start(runtime: &impl Runtime, daemon: &Path) {
    let build = Build::this().to_string();
    let gate = gate_here().await;
    println!("{}", text::steps_gate(&build, &gate));
    let user = crate::plugin::user_settings();
    let home = home_here();
    let manager = crate::service::Systemd::here()
        .ok()
        .filter(|_| crate::home::dir().is_none());
    let at = machine_here(
        user.as_deref(),
        home.as_deref(),
        manager.as_ref(),
        runtime,
        wave_images(daemon),
    );
    let synced = sync(&gate, &at).await;
    if let Some(user) = &user {
        for (kind, list) in [("allow", &synced.added.allow), ("deny", &synced.added.deny)] {
            for rule in list {
                println!("{}", text::sync_added_rule(&build, user, kind, rule));
            }
        }
    }
    for step in &synced.steps {
        println!("{}", step.line());
    }
    if synced.unit_written {
        UNIT_WRITTEN.store(true, Ordering::Relaxed);
    }
    let Some(dir) = local::dir() else {
        return;
    };
    if !synced.added.is_empty() {
        rule_sync::mark_added(&dir);
    }
    let rules = rules_line(&build, user.as_deref(), &gate, &synced);
    if let Err(e) = local::set_rules_sync(&dir, &rules) {
        eprintln!("riff: cannot keep the line of the sync of the rules: {e:#}");
    }
    if let Err(e) = local::set_needs_you(&dir, synced.needs_you().as_deref()) {
        eprintln!("riff: cannot keep the step that needs you: {e:#}");
    }
}

/// The line of the sync of the rules for `riff init`
/// (01M51M3ZPAEMJ64266QSJBNQ46), from the gate and the step of the rules.
fn rules_line(build: &str, user: Option<&Path>, gate: &Gate, synced: &Synced) -> String {
    let Some(user) = user else {
        return text::rules_sync_failed(build, text::INIT_NO_SETTINGS);
    };
    let state = synced
        .steps
        .iter()
        .find(|s| s.name == text::INIT_RULES)
        .map(|s| &s.state);
    let rules = match (gate, state) {
        (Gate::Off, _) => Synced::Off,
        (Gate::NotRelease(why), _) => Synced::NotRelease(why.clone()),
        (Gate::Open(_), Some(State::Failed(why))) => return text::rules_sync_failed(build, why),
        (Gate::Open(_), _) if synced.added.is_empty() => Synced::InStep,
        (Gate::Open(_), _) => Synced::Added(synced.added.clone()),
    };
    text::rules_synced(build, user, &rules)
}

/// `riff update --check` (01M51QFPYHK0916C5ZN2B55BR0): it looks at each
/// step on this machine and changes nothing, with the daemon in the
/// folder `daemon` and the current folder `here`. It prints the gate and
/// one line for each step, and keeps the step that needs a person for
/// the status line. It gives the exit code: 1 when a step is not in
/// step.
pub async fn check(runtime: &impl Runtime, daemon: &Path, here: &Path) -> i32 {
    let auto = auto_here();
    let release = release_here().await;
    println!("{}", text::steps_check_gate(auto, &release));
    let binary = Step {
        name: text::STEP_BINARY.to_owned(),
        state: match &release {
            Ok(tag) => State::InStep(text::step_binary_release(tag)),
            Err(why) => State::Behind {
                found: why.clone(),
                fix: text::STEP_FIX_BINARY.to_owned(),
            },
        },
    };
    let user = crate::plugin::user_settings();
    let home = home_here();
    let manager = crate::service::Systemd::here()
        .ok()
        .filter(|_| crate::home::dir().is_none());
    let mut images = wave_images(daemon);
    if let Ok(found) = crate::session_image::of_folder(here) {
        let repo = crate::enable::Repo::of(here)
            .map(|r| r.to_string())
            .unwrap_or_else(|| here.display().to_string());
        let found = found.to_string();
        if !images.iter().any(|(_, i)| i.as_ref() == Ok(&found)) {
            images.push((repo, Ok(found)));
        }
    }
    let at = machine_here(
        user.as_deref(),
        home.as_deref(),
        manager.as_ref(),
        runtime,
        images,
    );
    let mut steps = vec![binary];
    steps.extend(sync(&Gate::Off, &at).await.steps);
    for step in &steps {
        println!("{}", step.line());
    }
    if let Some(dir) = local::dir()
        && let Err(e) = local::set_needs_you(&dir, needs_you(&steps).as_deref())
    {
        eprintln!("riff: cannot keep the step that needs you: {e:#}");
    }
    i32::from(!steps.iter().all(Step::in_step))
}
