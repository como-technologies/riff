//! The riff entries that older releases wrote to the Claude config.
//!
//! # Design
//!
//! riff 1.3 and older wrote entries to the Claude config of the person:
//! `riff connect claude`, `riff enable` and `riff setup`. riff now gives
//! `claude` each of these at each start (see [`crate::launch`]), so the
//! old entries do nothing but harm: a plain `claude` loads the old
//! plugin. `riff` finds them, lists them, and removes them after the
//! person confirms. It keeps each other entry
//! (01M4BYH82P03FTXZBYC72BJ6F3).
//!
//! riff never changes a file that git tracks
//! (`git ls-files --error-unmatch`). It lists the riff entries of a
//! tracked `.claude/settings.json`, and the person removes them in a
//! pull request (01M4CMJPGS613K2FHQ6DKSY2WJ).
//!
//! | Place | The riff entries |
//! |---|---|
//! | the user settings (`~/.claude/settings.json`) | `enabledPlugins."riff@riff"`, `extraKnownMarketplaces.riff`, the `statusLine` of `riff statusline`, each rule of a riff tool or a `riff` command |
//! | `.claude/settings.json` and `.claude/settings.local.json` of each clone that riff knows | the same, and each rule that `riff setup` wrote ([`crate::permissions::rules`]) |
//! | the plugins of Claude Code (`~/.claude/plugins`) | the install of `riff@riff` and the marketplace `riff`: `claude plugin uninstall` and `claude plugin marketplace remove` remove them |
//!
//! ```mermaid
//! flowchart LR
//!     R[riff] --> F["find: settings files,<br/>installed plugins, marketplaces"]
//!     F --> L{"entries?"}
//!     L -- none --> S[start]
//!     L -- some --> A["list them, ask"]
//!     A -- "yes" --> X["claude plugin uninstall,<br/>marketplace remove,<br/>edit each untracked settings file"]
//!     A -- "no" --> S
//!     X --> S
//! ```
//!
//! ```
//! let text = r#"{"model": "opus", "enabledPlugins": {"riff@riff": true, "a@b": true}}"#;
//! let (new, what) = riff::old_config::without_riff(text, None);
//! assert_eq!(what, ["enabledPlugins.\"riff@riff\""]);
//! let new: serde_json::Value = serde_json::from_str(&new.unwrap())?;
//! assert_eq!(new, serde_json::json!({"model": "opus", "enabledPlugins": {"a@b": true}}));
//! # Ok::<(), serde_json::Error>(())
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value};

use crate::permissions::Rules;

/// The entry of the riff plugin: the plugin and its marketplace.
pub const PLUGIN: &str = "riff@riff";

/// The name of the marketplace of older releases.
pub const MARKETPLACE: &str = "riff";

/// A riff entry of an older release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Keys and rules in a settings file.
    Settings {
        /// The settings file.
        file: PathBuf,
        /// What riff wrote there, one item for each key or rule.
        what: Vec<String>,
        /// True when git tracks the file: riff does not change it.
        tracked: bool,
    },
    /// An install of the plugin `riff@riff` in one scope.
    Install {
        /// The scope: `user`, `project` or `local`.
        scope: String,
        /// The project of a `project` or a `local` install.
        project: Option<PathBuf>,
        /// True for a `project` install whose `.claude/settings.json`
        /// git tracks: riff does not uninstall it, because the
        /// uninstall changes that file.
        tracked: bool,
    },
    /// The marketplace `riff`.
    Marketplace,
}

/// Where riff looks.
#[derive(Debug, Clone, Default)]
pub struct Places {
    /// The user settings of Claude Code.
    pub user: Option<PathBuf>,
    /// The config directory of Claude Code.
    pub claude: Option<PathBuf>,
    /// The clones that riff knows.
    pub clones: Vec<PathBuf>,
}

impl Places {
    /// The places of this process, with the `clones` that riff knows.
    pub fn here(clones: Vec<PathBuf>) -> Places {
        Places {
            user: crate::plugin::user_settings(),
            claude: crate::worker_lsp::claude_dir(),
            clones,
        }
    }

    /// Each settings file, with the rules that `riff setup` wrote there.
    fn settings(&self) -> Vec<(PathBuf, Option<Rules>)> {
        let mut out: Vec<(PathBuf, Option<Rules>)> =
            self.user.iter().map(|u| (u.clone(), None)).collect();
        for clone in &self.clones {
            let project = crate::permissions::Project::of(clone);
            for name in ["settings.json", "settings.local.json"] {
                let file = project.top.join(".claude").join(name);
                if !out.iter().any(|(f, _)| *f == file) {
                    out.push((file, Some(project.rules())));
                }
            }
        }
        out
    }
}

/// True for a rule of a riff tool or of a `riff` command.
///
/// ```
/// use riff::old_config::is_riff_rule;
///
/// assert!(is_riff_rule("mcp__plugin_riff_riff"));
/// assert!(is_riff_rule("mcp__riff__read"));
/// assert!(is_riff_rule("Bash(riff)"));
/// assert!(is_riff_rule("Bash(riff *)"));
/// assert!(!is_riff_rule("Bash(riffle)"));
/// assert!(!is_riff_rule("mcp__riffle"));
/// ```
pub fn is_riff_rule(rule: &str) -> bool {
    ["mcp__plugin_riff_riff", "mcp__riff"]
        .iter()
        .any(|tool| rule == *tool || rule.starts_with(&format!("{tool}__")))
        || rule == "Bash(riff)"
        || rule.starts_with("Bash(riff ")
}

/// The settings `text` with no riff entry, and what it took out. The
/// text is `None` when it has no riff entry, or is not a JSON object.
/// `setup` holds the rules that `riff setup` wrote in a clone. A key
/// that is empty after the removal goes too. Each other key and rule
/// stays, in its order.
///
/// ```
/// use riff::old_config::without_riff;
/// use riff::permissions::Rules;
///
/// let text = r#"{
///   "statusLine": {"type": "command", "command": "riff statusline"},
///   "permissions": {"allow": ["Bash(riff *)", "Bash(ls)", "Bash(gh pr view *)"]}
/// }"#;
/// let setup = Rules { allow: vec!["Bash(gh pr view *)".into()], deny: vec![] };
/// let (new, what) = without_riff(text, Some(&setup));
/// assert_eq!(
///     what,
///     ["statusLine", "permissions.allow: Bash(riff *)", "permissions.allow: Bash(gh pr view *)"],
/// );
/// let new: serde_json::Value = serde_json::from_str(&new.unwrap())?;
/// assert_eq!(new, serde_json::json!({"permissions": {"allow": ["Bash(ls)"]}}));
/// assert_eq!(without_riff(r#"{"statusLine": {"command": "my line"}}"#, None), (None, vec![]));
/// assert_eq!(without_riff("not json", None), (None, vec![]));
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn without_riff(text: &str, setup: Option<&Rules>) -> (Option<String>, Vec<String>) {
    let Ok(Value::Object(mut settings)) = serde_json::from_str::<Value>(text) else {
        return (None, vec![]);
    };
    let mut what = Vec::new();
    for (key, name) in [
        ("enabledPlugins", PLUGIN),
        ("extraKnownMarketplaces", MARKETPLACE),
    ] {
        if take(&mut settings, key, |map| map.remove(name).is_some()) {
            what.push(format!("{key}.\"{name}\""));
        }
    }
    if settings
        .get("statusLine")
        .and_then(|s| s.get("command"))
        .is_some_and(|c| c == crate::launch::STATUSLINE)
    {
        settings.remove("statusLine");
        what.push("statusLine".to_owned());
    }
    take(&mut settings, "permissions", |permissions| {
        for list in ["allow", "deny"] {
            let wrote: &[String] = match setup {
                Some(rules) if list == "allow" => &rules.allow,
                Some(rules) => &rules.deny,
                None => &[],
            };
            let Some(Value::Array(rules)) = permissions.get_mut(list) else {
                continue;
            };
            rules.retain(|rule| {
                let Some(rule) = rule.as_str() else {
                    return true;
                };
                let riff = is_riff_rule(rule) || wrote.iter().any(|w| w == rule);
                if riff {
                    what.push(format!("permissions.{list}: {rule}"));
                }
                !riff
            });
            if rules.is_empty() {
                permissions.remove(list);
            }
        }
        true
    });
    if what.is_empty() {
        return (None, what);
    }
    let mut out = serde_json::to_string_pretty(&Value::Object(settings)).unwrap_or_default();
    out.push('\n');
    (Some(out), what)
}

/// Runs `edit` on the object at `key` of `settings`, and removes the
/// key when the object is empty after it. Gives what `edit` gives, or
/// false when `key` holds no object.
fn take(
    settings: &mut Map<String, Value>,
    key: &str,
    edit: impl FnOnce(&mut Map<String, Value>) -> bool,
) -> bool {
    let Some(Value::Object(map)) = settings.get_mut(key) else {
        return false;
    };
    let done = edit(map);
    if map.is_empty() {
        settings.remove(key);
    }
    done
}

/// True when git tracks `file`: `git ls-files --error-unmatch` in its
/// directory passes. False when git cannot tell, for example outside
/// a clone (01M4CMJPGS613K2FHQ6DKSY2WJ).
pub fn tracked(file: &Path) -> bool {
    let (Some(dir), Some(name)) = (file.parent(), file.file_name()) else {
        return false;
    };
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["ls-files", "--error-unmatch", "--"])
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A JSON file, or `None` when it is missing or not JSON.
fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Each riff entry of an older release in `places`.
pub fn find(places: &Places) -> Vec<Entry> {
    let mut found = Vec::new();
    if let Some(claude) = &places.claude {
        let plugins = claude.join("plugins");
        if let Some(Value::Array(installs)) = read_json(&plugins.join("installed_plugins.json"))
            .and_then(|v| v.get("plugins")?.get(PLUGIN).cloned())
        {
            for install in installs {
                let scope = install["scope"].as_str().unwrap_or("user").to_owned();
                let project = install["projectPath"].as_str().map(PathBuf::from);
                let tracked = scope == "project"
                    && project
                        .as_ref()
                        .is_some_and(|p| tracked(&p.join(".claude/settings.json")));
                found.push(Entry::Install {
                    scope,
                    project,
                    tracked,
                });
            }
        }
        if read_json(&plugins.join("known_marketplaces.json"))
            .is_some_and(|v| v.get(MARKETPLACE).is_some())
        {
            found.push(Entry::Marketplace);
        }
    }
    for (file, setup) in places.settings() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let (_, what) = without_riff(&text, setup.as_ref());
        if !what.is_empty() {
            let tracked = tracked(&file);
            found.push(Entry::Settings {
                file,
                what,
                tracked,
            });
        }
    }
    found
}

/// True when riff does not remove `entry`, because git tracks the file
/// that the removal changes (01M4CMJPGS613K2FHQ6DKSY2WJ).
///
/// ```
/// use riff::old_config::{Entry, is_tracked};
///
/// let file = "/h/app/.claude/settings.json".into();
/// assert!(is_tracked(&Entry::Settings { file, what: vec![], tracked: true }));
/// assert!(!is_tracked(&Entry::Marketplace));
/// ```
pub fn is_tracked(entry: &Entry) -> bool {
    matches!(
        entry,
        Entry::Settings { tracked: true, .. } | Entry::Install { tracked: true, .. }
    )
}

/// The lines that show `entry` to the person.
///
/// ```
/// use riff::old_config::{Entry, lines};
///
/// let file = "/h/.claude/settings.json".into();
/// let entry = Entry::Settings { file, what: vec!["statusLine".into()], tracked: false };
/// assert_eq!(lines(&entry), ["/h/.claude/settings.json: statusLine"]);
/// let project = Some("/h/app".into());
/// let install = Entry::Install { scope: "local".into(), project, tracked: false };
/// assert_eq!(lines(&install), ["the plugin riff@riff, installed in the scope local of /h/app"]);
/// assert_eq!(lines(&Entry::Marketplace), ["the plugin marketplace riff"]);
/// ```
pub fn lines(entry: &Entry) -> Vec<String> {
    match entry {
        Entry::Settings { file, what, .. } => what
            .iter()
            .map(|w| format!("{}: {w}", file.display()))
            .collect(),
        Entry::Install {
            scope,
            project: Some(project),
            ..
        } => vec![format!(
            "the plugin {PLUGIN}, installed in the scope {scope} of {}",
            project.display()
        )],
        Entry::Install {
            scope,
            project: None,
            ..
        } => vec![format!(
            "the plugin {PLUGIN}, installed in the scope {scope}"
        )],
        Entry::Marketplace => vec![format!("the plugin marketplace {MARKETPLACE}")],
    }
}

/// Removes each entry of `found` with the `claude` command at `claude`
/// and by an edit of each settings file in `places`. It does the steps
/// of `claude` first: they can change the settings files. It skips each
/// entry and each file that git tracks ([`is_tracked`], [`tracked`]).
/// Gives one line for each step that failed.
pub fn remove(found: &[Entry], claude: &Path, places: &Places) -> Vec<String> {
    let mut failed = Vec::new();
    for entry in found.iter().filter(|e| !is_tracked(e)) {
        let (args, dir): (Vec<&str>, Option<&Path>) = match entry {
            Entry::Install { scope, project, .. } => (
                vec!["plugin", "uninstall", PLUGIN, "--scope", scope],
                project.as_deref(),
            ),
            Entry::Marketplace => (vec!["plugin", "marketplace", "remove", MARKETPLACE], None),
            Entry::Settings { .. } => continue,
        };
        let mut command = Command::new(claude);
        command.args(&args);
        if let Some(dir) = dir.filter(|d| d.is_dir()) {
            command.current_dir(dir);
        }
        match command.output() {
            Ok(out) if out.status.success() => {}
            Ok(out) => failed.push(format!(
                "{} {} failed: {}",
                claude.display(),
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            )),
            Err(e) => failed.push(format!("cannot run {}: {e}", claude.display())),
        }
    }
    for (file, setup) in places.settings() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        if tracked(&file) {
            continue;
        }
        if let (Some(new), _) = without_riff(&text, setup.as_ref())
            && let Err(e) = std::fs::write(&file, new)
        {
            failed.push(format!("cannot write {}: {e}", file.display()));
        }
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settings_file_with_no_riff_entry_stays() {
        let text = r#"{"enabledPlugins": {"a@b": true}, "permissions": {"deny": ["X"]}}"#;
        assert_eq!(without_riff(text, None), (None, vec![]));
    }

    #[test]
    fn the_keys_that_are_empty_after_the_removal_go() {
        let text = r#"{
          "enabledPlugins": {"riff@riff": true},
          "extraKnownMarketplaces": {"riff": {"source": {}}},
          "permissions": {"allow": ["mcp__riff"], "deny": ["Bash(git push * main)"]},
          "theme": "dark"
        }"#;
        let setup = crate::permissions::rules(None, "main");
        let (new, what) = without_riff(text, Some(&setup));
        assert_eq!(what.len(), 4, "{what:?}");
        let new: Value = serde_json::from_str(&new.unwrap()).unwrap();
        assert_eq!(new, serde_json::json!({"theme": "dark"}));
    }

    #[test]
    fn find_names_the_install_and_the_marketplace() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(
            plugins.join("installed_plugins.json"),
            r#"{"version": 2, "plugins": {"riff@riff": [{"scope": "user"},
               {"scope": "local", "projectPath": "/h"}], "x@y": [{"scope": "user"}]}}"#,
        )
        .unwrap();
        std::fs::write(
            plugins.join("known_marketplaces.json"),
            r#"{"riff": {}, "other": {}}"#,
        )
        .unwrap();
        let places = Places {
            claude: Some(dir.path().to_owned()),
            ..Places::default()
        };
        assert_eq!(
            find(&places),
            [
                Entry::Install {
                    scope: "user".into(),
                    project: None,
                    tracked: false,
                },
                Entry::Install {
                    scope: "local".into(),
                    project: Some("/h".into()),
                    tracked: false,
                },
                Entry::Marketplace,
            ]
        );
    }
}
