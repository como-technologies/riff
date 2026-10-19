//! The riff daemon keeps the user rules of Claude Code in step with each
//! release (#787).
//!
//! # Design
//!
//! `riff init` adds the rules of the user place of
//! [`crate::permissions::TABLE`] after one yes
//! (01M5084K09Z2S7N57315CX2R0F). A later release can add a rule. A
//! session cannot add its own rules: the auto mode check refuses it as
//! self-modification, also with the approval of the person. The riff
//! daemon is not a session. So at each start, it adds each rule of its
//! release that the user settings lack ([`sync`],
//! 01M51M3ZHHBE61CEG9T6ENFG18). It never removes a rule: only
//! `riff init` does, after a yes (01M51JCW2EKF8H8QFQYSVRSPET).
//!
//! The yes of the person is the opt-in to the update by itself,
//! `riff update --auto on` ([`crate::auto_update`]). Each release waits
//! for the approval of the owner, so that approval is also the approval
//! of its rules. `riff update --auto off` turns the sync off. There is
//! no other question and no other switch.
//!
//! The daemon of the machine starts again on each new riff
//! ([`crate::daemon`], 01M517BTP6BAGTRBYWPX9GPVE0), so each release
//! gets its sync. Only a release changes the user settings: a binary
//! whose SHA-256 is the line of `riff` in `SHA256SUMS` of the GitHub
//! release of its version ([`release::is_release`],
//! 01M51M3ZKWQYBBPD97R0G9QAX0). A check build or a dev build has the
//! version of a release, but another SHA-256. The sync reads
//! `SHA256SUMS` only from GitHub ([`sums_source`],
//! 01M51QARV45MGEKC6KGWX1N5NS): a session can set
//! [`release::RELEASES`] and name a `SHA256SUMS` of its own. So a
//! session cannot build a branch with a wide rule and get it applied.
//!
//! ```mermaid
//! flowchart TD
//!     S["the riff daemon starts"] --> A{"update.auto?"}
//!     A -- "off" --> N["no change"]
//!     A -- "on" --> G{"RIFF_RELEASES set?"}
//!     G -- "yes" --> N
//!     G -- "no" --> R{"its binary is in SHA256SUMS<br/>of the GitHub release of its version?"}
//!     R -- "no: a check build or a dev build" --> N
//!     R -- "yes" --> M{"rules missing in the<br/>user settings?"}
//!     M -- "no" --> I["in step"]
//!     M -- "yes" --> W["add them: a new file, then a rename"]
//!     W --> K["mark the add: each worker<br/>starts again at its next clear"]
//! ```
//!
//! The daemon logs each rule that it adds, and the line of the result.
//! It keeps the line in the local files ([`local::rules_sync`]).
//! `riff init` shows whether the sync is on, and that line
//! (01M51M3ZPAEMJ64266QSJBNQ46).
//!
//! # The mark
//!
//! Claude Code reads its rules only at its start. So each time riff adds
//! rules to the user settings, here or in `riff init`, it marks the add
//! ([`mark_added`]).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use riff_core::build::Build;

use crate::permissions::{self, Rules};
use crate::release::{self, Source};
use crate::update_steps::{self, Gate, Release};
use crate::{lifecycle, local, text};

/// What a sync of the rules did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Synced {
    /// `update.auto` is off: it changed nothing.
    Off,
    /// The riff is not a release: it changed nothing. It holds why.
    NotRelease(String),
    /// The user settings have each rule.
    InStep,
    /// It added these rules.
    Added(Rules),
}

/// The facts of a sync.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// `update.auto` of the machine: the opt-in of the person.
    pub auto: bool,
    /// The user settings of Claude Code.
    pub user: &'a Path,
    /// The home of the person, for `{HOME}` in a rule.
    pub home: Option<&'a Path>,
    /// The riff binary of the daemon.
    pub binary: &'a Path,
    /// The release tag of its version, for example `v1.5.0`.
    pub tag: &'a str,
    /// The place of the release files, or why the sync trusts no place
    /// ([`sums_source`]).
    pub source: &'a Result<Source, String>,
    /// The target of the static binaries of this machine.
    pub target: Option<&'a str>,
}

impl Facts<'_> {
    /// The binary of the facts and the place of its release files, for
    /// the gate ([`update_steps::gate`]).
    pub fn release(&self) -> Release<'_> {
        Release {
            binary: self.binary,
            tag: self.tag,
            source: self.source,
            target: self.target,
        }
    }
}

/// The sync of the rules (01M51M3ZHHBE61CEG9T6ENFG18). With `auto` off,
/// it changes nothing. With no trusted place of the release files
/// (01M51QARV45MGEKC6KGWX1N5NS), it changes nothing. Else, when the
/// binary is a release ([`release::is_release`],
/// 01M51M3ZKWQYBBPD97R0G9QAX0), it adds each
/// rule of [`permissions::user_rules`] that the user settings lack, as
/// [`permissions::add`] does. It keeps each other rule and key in its
/// order, and removes no rule. The checks before the add are the gate
/// of each step of an update ([`update_steps::gate`]).
pub async fn sync(facts: &Facts<'_>) -> Result<Synced> {
    match update_steps::gate(facts.auto, &facts.release()).await? {
        Gate::Off => return Ok(Synced::Off),
        Gate::NotRelease(why) => return Ok(Synced::NotRelease(why)),
        Gate::Open(_) => {}
    }
    let added = permissions::add(facts.user, &permissions::user_rules(facts.home))?;
    Ok(if added.is_empty() {
        Synced::InStep
    } else {
        Synced::Added(added)
    })
}

/// The place of `SHA256SUMS` for the release check of the sync
/// (01M51QARV45MGEKC6KGWX1N5NS): only the GitHub releases. `releases`
/// is the value of [`release::RELEASES`]. A person or a session can set
/// it and name a `SHA256SUMS` of their own, so with a value, `Err` says
/// why the sync changes nothing.
///
/// ```
/// use riff::release::Source;
/// use riff::rule_sync::sums_source;
///
/// assert_eq!(sums_source(None), Ok(Source::GitHub));
/// assert_eq!(
///     sums_source(Some("/srv/riff")),
///     Err("RIFF_RELEASES is set to /srv/riff, and the sync reads SHA256SUMS only from the \
///          GitHub release"
///         .to_owned())
/// );
/// ```
pub fn sums_source(releases: Option<&str>) -> Result<Source, String> {
    match releases {
        None => Ok(Source::GitHub),
        Some(place) => Err(text::not_release_releases(place)),
    }
}

/// Marks an add of rules to the user settings in the local files `dir`
/// ([`local::set_rules_added`], 01M51M3ZRKQZ8KDVRGJR3XAY00), so each
/// worker that started before it starts again at its next clear.
pub fn mark_added(dir: &Path) {
    if let Err(e) = local::set_rules_added(dir) {
        eprintln!("riff: cannot mark the new rules for the workers: {e:#}");
    }
}

/// What `riff init` shows of the sync (01M51M3ZPAEMJ64266QSJBNQ46).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    /// `update.auto` is on, so the sync is on.
    pub auto: bool,
    /// The line of the last sync of the riff daemon
    /// ([`local::rules_sync`]).
    pub last: Option<String>,
}

impl State {
    /// The sync of this machine, with the local files `local`.
    pub fn here(local: Option<&Path>) -> State {
        State {
            auto: update_steps::auto_here(),
            last: local.and_then(local::rules_sync),
        }
    }
}

/// The sync at each start of the riff daemon of this machine
/// (01M51M3ZHHBE61CEG9T6ENFG18), with the binary of this process. It
/// logs each line of [`run`], and keeps the last line in the local
/// files (01M51M3ZPAEMJ64266QSJBNQ46). It never fails the daemon.
pub async fn at_start() {
    let build = Build::this().to_string();
    let dir = local::dir();
    let lines = match here(&build, dir.as_deref()).await {
        Ok(lines) => lines,
        Err(e) => vec![text::rules_sync_failed(&build, &format!("{e:#}"))],
    };
    for line in &lines {
        println!("{line}");
    }
    if let Some(dir) = dir
        && let Some(line) = lines.last()
        && let Err(e) = local::set_rules_sync(&dir, line)
    {
        eprintln!("riff: cannot keep the line of the sync of the rules: {e:#}");
    }
}

/// [`run`] with the facts of this machine and process.
async fn here(build: &str, local: Option<&Path>) -> Result<Vec<String>> {
    let user = crate::plugin::user_settings().context(text::INIT_NO_SETTINGS)?;
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from);
    let source = sums_source(std::env::var(release::RELEASES).ok().as_deref());
    let tag = lifecycle::release_tag(env!("CARGO_PKG_VERSION"));
    let facts = Facts {
        auto: update_steps::auto_here(),
        user: &user,
        home: home.as_deref(),
        // The binary that runs, also after a new riff replaced its file.
        binary: Path::new("/proc/self/exe"),
        tag: &tag,
        source: &source,
        target: release::TARGET,
    };
    run(build, &facts, local).await
}

/// [`sync`] by the riff daemon `build`, with the local files `local`:
/// the log lines (01M51M3ZPAEMJ64266QSJBNQ46). It gives one line for
/// each rule that it adds, and the line of the result last. After an
/// add, it marks the add ([`mark_added`]).
pub async fn run(build: &str, facts: &Facts<'_>, local: Option<&Path>) -> Result<Vec<String>> {
    let synced = sync(facts).await?;
    let mut lines = Vec::new();
    if let Synced::Added(rules) = &synced {
        for (kind, list) in [("allow", &rules.allow), ("deny", &rules.deny)] {
            for rule in list {
                lines.push(text::sync_added_rule(build, facts.user, kind, rule));
            }
        }
        if let Some(dir) = local {
            mark_added(dir);
        }
    }
    lines.push(text::rules_synced(build, facts.user, &synced));
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: &str = "x86_64-unknown-linux-musl";

    /// A machine in a temp folder: the user settings, a riff binary, and
    /// the release files of `v1.5.0` with the SHA-256 of `release`.
    struct Machine {
        dir: tempfile::TempDir,
        source: Result<Source, String>,
    }

    impl Machine {
        fn new(settings: &str, binary: &[u8], release: &[u8]) -> Machine {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("settings.json"), settings).unwrap();
            std::fs::write(dir.path().join("riff"), binary).unwrap();
            let files = dir.path().join("releases/v1.5.0");
            std::fs::create_dir_all(&files).unwrap();
            let line = format!(
                "{}  {}\n",
                release::sha256(release),
                release::asset("riff", TARGET)
            );
            std::fs::write(files.join(release::SUMS), line).unwrap();
            let source = Ok(Source::Folder(dir.path().join("releases")));
            Machine { dir, source }
        }

        /// The release files of the machine, named by `RIFF_RELEASES`.
        fn by_variable(mut self) -> Machine {
            let place = self.dir.path().join("releases");
            self.source = sums_source(Some(&place.display().to_string()));
            self
        }

        fn user(&self) -> PathBuf {
            self.dir.path().join("settings.json")
        }

        fn settings(&self) -> String {
            std::fs::read_to_string(self.user()).unwrap()
        }

        fn facts<'a>(&'a self, auto: bool, user: &'a Path, binary: &'a Path) -> Facts<'a> {
            Facts {
                auto,
                user,
                home: Some(Path::new("/home/mike")),
                binary,
                tag: "v1.5.0",
                source: &self.source,
                target: Some(TARGET),
            }
        }

        async fn sync(&self, auto: bool) -> Synced {
            let (user, binary) = (self.user(), self.dir.path().join("riff"));
            sync(&self.facts(auto, &user, &binary)).await.unwrap()
        }

        async fn run(&self, local: &Path) -> Vec<String> {
            let (user, binary) = (self.user(), self.dir.path().join("riff"));
            run(
                "1.5.0 (abc)",
                &self.facts(true, &user, &binary),
                Some(local),
            )
            .await
            .unwrap()
        }
    }

    /// Settings with other keys, an allow rule of the person, a rule of
    /// riff, and a deny rule.
    const SETTINGS: &str = r#"{
  "model": "opus",
  "permissions": {
    "allow": ["Read(~/**)", "Bash(riff *)", "Bash(cargo test *)"],
    "deny": ["Bash(rm -rf *)"]
  },
  "statusLine": {"type": "command", "command": "riff statusline"}
}
"#;

    #[tokio::test]
    async fn auto_update_on_and_a_release_adds_the_missing_rules_and_keeps_each_other_rule_and_key()
    {
        let m = Machine::new(SETTINGS, b"the riff of v1.5.0", b"the riff of v1.5.0");
        let want = permissions::user_rules(Some(Path::new("/home/mike")));
        let Synced::Added(added) = m.sync(true).await else {
            panic!("no add: {}", m.settings());
        };
        assert_eq!(added, permissions::missing(&want, &[SETTINGS]));
        assert!(!added.allow.contains(&"Bash(riff *)".to_owned()));
        let after: serde_json::Value = serde_json::from_str(&m.settings()).unwrap();
        let keys: Vec<&String> = after.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["model", "permissions", "statusLine"]);
        let allow: Vec<&str> = after["permissions"]["allow"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        assert_eq!(
            allow[..3],
            ["Read(~/**)", "Bash(riff *)", "Bash(cargo test *)"]
        );
        assert_eq!(
            allow[3..],
            added.allow.iter().map(String::as_str).collect::<Vec<_>>()
        );
        assert_eq!(
            after["permissions"]["deny"],
            serde_json::json!(["Bash(rm -rf *)"])
        );
        assert!(permissions::missing(&want, &[&m.settings()]).is_empty());
        // The next start finds each rule.
        let text = m.settings();
        assert_eq!(m.sync(true).await, Synced::InStep);
        assert_eq!(m.settings(), text);
    }

    #[tokio::test]
    async fn run_logs_each_added_rule_and_the_result_and_marks_the_add() {
        let m = Machine::new(SETTINGS, b"the riff of v1.5.0", b"the riff of v1.5.0");
        let local = tempfile::tempdir().unwrap();
        let want = permissions::missing(
            &permissions::user_rules(Some(Path::new("/home/mike"))),
            &[SETTINGS],
        );
        let lines = m.run(local.path()).await;
        let user = m.user();
        let mut rules: Vec<String> = want
            .allow
            .iter()
            .map(|r| text::sync_added_rule("1.5.0 (abc)", &user, "allow", r))
            .collect();
        rules.extend(
            want.deny
                .iter()
                .map(|r| text::sync_added_rule("1.5.0 (abc)", &user, "deny", r)),
        );
        rules.push(text::rules_synced(
            "1.5.0 (abc)",
            &user,
            &Synced::Added(want),
        ));
        assert_eq!(lines, rules);
        let mark = local::rules_added(local.path()).expect("no mark");
        // In step: one line, and the mark stays.
        let lines = m.run(local.path()).await;
        assert_eq!(
            lines,
            [text::rules_synced("1.5.0 (abc)", &user, &Synced::InStep)]
        );
        assert_eq!(local::rules_added(local.path()), Some(mark));
    }

    /// 01M51QARV45MGEKC6KGWX1N5NS: a `SHA256SUMS` that `RIFF_RELEASES`
    /// names counts for nothing, also with the SHA-256 of the binary.
    #[tokio::test]
    async fn a_sums_file_of_riff_releases_changes_nothing_also_with_the_sha256_of_the_binary() {
        let m = Machine::new(SETTINGS, b"the riff of v1.5.0", b"the riff of v1.5.0").by_variable();
        let Synced::NotRelease(why) = m.sync(true).await else {
            panic!("a change: {}", m.settings());
        };
        assert!(why.starts_with("RIFF_RELEASES is set to "), "{why}");
        assert_eq!(m.settings(), SETTINGS);
        let local = tempfile::tempdir().unwrap();
        let lines = m.run(local.path()).await;
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("changed no rule"), "{lines:?}");
        assert_eq!(local::rules_added(local.path()), None);
        assert_eq!(m.settings(), SETTINGS);
        // With auto update off, the line says that.
        assert_eq!(m.sync(false).await, Synced::Off);
    }

    #[tokio::test]
    async fn auto_update_off_changes_nothing() {
        let m = Machine::new(SETTINGS, b"the riff of v1.5.0", b"the riff of v1.5.0");
        assert_eq!(m.sync(false).await, Synced::Off);
        assert_eq!(m.settings(), SETTINGS);
    }

    #[tokio::test]
    async fn a_binary_that_is_not_in_the_sums_changes_nothing() {
        let m = Machine::new(SETTINGS, b"a dev build of 1.5.0", b"the riff of v1.5.0");
        let Synced::NotRelease(why) = m.sync(true).await else {
            panic!("a change: {}", m.settings());
        };
        assert!(why.ends_with("it is a check build or a dev build"), "{why}");
        assert_eq!(m.settings(), SETTINGS);
        // A release with no SHA256SUMS: no change either.
        std::fs::remove_file(m.dir.path().join("releases/v1.5.0/SHA256SUMS")).unwrap();
        assert_eq!(
            m.sync(true).await,
            Synced::NotRelease("the release v1.5.0 has no SHA256SUMS".into())
        );
        assert_eq!(m.settings(), SETTINGS);
    }
}
