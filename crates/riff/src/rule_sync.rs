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
//! whose SHA-256 is the line of `riff` in `SHA256SUMS` of the release of
//! its version ([`release::is_release`], 01M51M3ZKWQYBBPD97R0G9QAX0). A
//! check build or a dev build has the version of a release, but another
//! SHA-256. So a session cannot build a branch with a wide rule and get
//! it applied.
//!
//! ```mermaid
//! flowchart TD
//!     S["the riff daemon starts"] --> A{"update.auto?"}
//!     A -- "off" --> N["no change"]
//!     A -- "on" --> R{"its binary is in SHA256SUMS<br/>of the release of its version?"}
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
//! # The workers
//!
//! Claude Code reads its rules only at its start. So each time riff adds
//! rules to the user settings, here or in `riff init`, it marks the add
//! ([`mark_added`]). `riff workers run` gives each worker the mark of its
//! start in [`WORKER_RULES`]. When the mark changed, the worker ends in
//! place of its next clear, as a worker on an old riff does
//! ([`crate::next`], 01M51M3ZRKQZ8KDVRGJR3XAY00). The rollout starts a
//! new worker, with the new rules, for the free work.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant W as riff workers run
//!     participant C as claude of a worker
//!     participant D as riff daemon
//!     participant H as riff hook stop
//!     W->>C: start, RIFF_WORKER_RULES = the mark
//!     D->>D: add rules, a new mark
//!     C->>H: the end of the last turn of its item
//!     H->>H: the mark changed
//!     H->>W: end the worker, in place of the clear
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use riff_core::build::Build;

use crate::permissions::{self, Rules};
use crate::release::{self, Source};
use crate::{lifecycle, local, settings, text};

/// The variable with the mark of the last add of rules
/// ([`local::rules_added`]) at the start of a worker
/// (01M51M3ZRKQZ8KDVRGJR3XAY00). It is empty when riff had added no
/// rule.
pub const WORKER_RULES: &str = "RIFF_WORKER_RULES";

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
    /// The place of the release files.
    pub source: &'a Source,
    /// The target of the static binaries of this machine.
    pub target: Option<&'a str>,
}

/// The sync of the rules (01M51M3ZHHBE61CEG9T6ENFG18). With `auto` off,
/// it changes nothing. Else, when the binary is a release
/// ([`release::is_release`], 01M51M3ZKWQYBBPD97R0G9QAX0), it adds each
/// rule of [`permissions::user_rules`] that the user settings lack, as
/// [`permissions::add`] does. It keeps each other rule and key in its
/// order, and removes no rule.
pub async fn sync(facts: &Facts<'_>) -> Result<Synced> {
    if !facts.auto {
        return Ok(Synced::Off);
    }
    let bytes = std::fs::read(facts.binary)
        .with_context(|| format!("cannot read {}", facts.binary.display()))?;
    if let Err(why) = release::is_release(&bytes, facts.tag, facts.source, facts.target).await {
        return Ok(Synced::NotRelease(why));
    }
    let added = permissions::add(facts.user, &permissions::user_rules(facts.home))?;
    Ok(if added.is_empty() {
        Synced::InStep
    } else {
        Synced::Added(added)
    })
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
            auto: auto_here(),
            last: local.and_then(local::rules_sync),
        }
    }
}

/// `update.auto` of this machine. A bad settings file counts as off.
fn auto_here() -> bool {
    settings::path().is_ok_and(|p| settings::update_auto(&p).unwrap_or(false))
}

/// True when riff added rules after the start of a worker
/// (01M51M3ZRKQZ8KDVRGJR3XAY00): `now`, the mark in the local files,
/// is not `seen`, the mark of [`WORKER_RULES`] at the start. A worker
/// with no `seen` started before riff gave the variable.
///
/// ```
/// use riff::rule_sync::new_rules;
///
/// assert!(!new_rules(Some(""), None));
/// assert!(!new_rules(None, None));
/// assert!(!new_rules(Some("42"), Some("42")));
/// assert!(new_rules(Some(""), Some("42")));
/// assert!(new_rules(Some("41"), Some("42")));
/// assert!(new_rules(None, Some("42")));
/// ```
pub fn new_rules(seen: Option<&str>, now: Option<&str>) -> bool {
    now.is_some_and(|now| seen != Some(now))
}

/// The sync at each start of the riff daemon of this machine
/// (01M51M3ZHHBE61CEG9T6ENFG18), with the binary of this process. It
/// logs each rule that it adds and the line of the result, and keeps
/// the line in the local files (01M51M3ZPAEMJ64266QSJBNQ46). After an
/// add, it marks the add ([`mark_added`]). It never fails the daemon.
pub async fn at_start() {
    let build = Build::this().to_string();
    let line = match here(&build).await {
        Ok(line) => line,
        Err(e) => text::rules_sync_failed(&build, &format!("{e:#}")),
    };
    println!("{line}");
    if let Some(dir) = local::dir()
        && let Err(e) = local::set_rules_sync(&dir, &line)
    {
        eprintln!("riff: cannot keep the line of the sync of the rules: {e:#}");
    }
}

/// [`sync`] with the facts of this machine and process: the line of the
/// result.
async fn here(build: &str) -> Result<String> {
    let user = crate::plugin::user_settings().context(text::INIT_NO_SETTINGS)?;
    let auto = auto_here();
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from);
    let source = Source::of(std::env::var(release::RELEASES).ok().as_deref());
    let tag = lifecycle::release_tag(env!("CARGO_PKG_VERSION"));
    let facts = Facts {
        auto,
        user: &user,
        home: home.as_deref(),
        // The binary that runs, also after a new riff replaced its file.
        binary: Path::new("/proc/self/exe"),
        tag: &tag,
        source: &source,
        target: release::TARGET,
    };
    let synced = sync(&facts).await?;
    if let Synced::Added(rules) = &synced {
        for (kind, list) in [("allow", &rules.allow), ("deny", &rules.deny)] {
            for rule in list {
                println!("{}", text::sync_added_rule(build, &user, kind, rule));
            }
        }
        if let Some(dir) = local::dir() {
            mark_added(&dir);
        }
    }
    Ok(text::rules_synced(build, &user, &synced))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: &str = "x86_64-unknown-linux-musl";

    /// A machine in a temp folder: the user settings, a riff binary, and
    /// the release files of `v1.5.0` with the SHA-256 of `release`.
    struct Machine {
        dir: tempfile::TempDir,
        source: Source,
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
            let source = Source::Folder(dir.path().join("releases"));
            Machine { dir, source }
        }

        fn user(&self) -> PathBuf {
            self.dir.path().join("settings.json")
        }

        fn settings(&self) -> String {
            std::fs::read_to_string(self.user()).unwrap()
        }

        async fn sync(&self, auto: bool) -> Synced {
            let (user, binary) = (self.user(), self.dir.path().join("riff"));
            let facts = Facts {
                auto,
                user: &user,
                home: Some(Path::new("/home/mike")),
                binary: &binary,
                tag: "v1.5.0",
                source: &self.source,
                target: Some(TARGET),
            };
            sync(&facts).await.unwrap()
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
        assert_eq!(allow[3..], added.allow.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(after["permissions"]["deny"], serde_json::json!(["Bash(rm -rf *)"]));
        assert!(permissions::missing(&want, &[&m.settings()]).is_empty());
        // The next start finds each rule.
        let text = m.settings();
        assert_eq!(m.sync(true).await, Synced::InStep);
        assert_eq!(m.settings(), text);
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
