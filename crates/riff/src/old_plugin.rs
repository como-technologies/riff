//! The plugin of riff 3 for Claude Code, left on a machine after the
//! update to riff 4 (01M5JFF7TAVA87TMJXK25HDJ19).
//!
//! # Design
//!
//! riff 3 put a plugin of Claude Code in `DATA/claude-plugin`
//! ([`crate::run::data_dir`]), and wrote these keys into the settings of
//! Claude Code ([`KEYS`]):
//!
//! | Key | riff 3 wrote it with |
//! |---|---|
//! | `enabledPlugins."riff@riff"` | `riff enable` |
//! | `extraKnownMarketplaces.riff` | `riff connect claude`, or the person |
//! | `statusLine`, with the command `riff statusline` | `riff connect claude` |
//!
//! riff 4 has no plugin, and no `riff hook`, `riff statusline` or
//! `riff mcp`. So each hook of the old plugin fails in each session of
//! Claude Code on the host. riff removes the folder and each key
//! ([`Found::remove`]):
//!
//! - `riff init` removes them, and `riff init --check` names them
//!   ([`crate::first_run::run`]).
//! - The riff daemon of a release removes them at its start, when the
//!   update by itself is on, and `riff update --check` names them
//!   ([`crate::update_steps::old_plugin`]).
//!
//! ```mermaid
//! flowchart LR
//!     F["the folder DATA/claude-plugin<br/>and the keys of riff 3<br/>in the settings"] --> I["riff init"]
//!     F --> D["the riff daemon of a release<br/>update.auto on"]
//!     I --> R["removed"]
//!     D --> R
//!     F --> C["riff init --check,<br/>riff update --check"] --> N["named, kept"]
//! ```
//!
//! Each other key of a settings file stays, in its order.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

/// The name of the plugin and of its marketplace.
pub const NAME: &str = "riff";

/// The plugin in `enabledPlugins`.
pub const PLUGIN: &str = "riff@riff";

/// The command of the status line of riff 3.
pub const STATUSLINE: &str = "riff statusline";

/// The keys of riff 3 in a settings file, as [`keys`] names them.
pub const KEYS: [&str; 3] = [
    "enabledPlugins.riff@riff",
    "extraKnownMarketplaces.riff",
    "statusLine",
];

/// The keys of riff 3 that the settings text `text` has ([`KEYS`]). A
/// text that is not JSON has none.
///
/// ```
/// use riff::old_plugin::keys;
///
/// let text = r#"{
///   "enabledPlugins": {"riff@riff": true, "rust-analyzer-lsp@claude-plugins-official": true},
///   "extraKnownMarketplaces": {"riff": {"source": {"source": "directory", "path": "/p"}}},
///   "statusLine": {"type": "command", "command": "riff statusline"}
/// }"#;
/// assert_eq!(keys(text), riff::old_plugin::KEYS);
/// // A status line of the person stays.
/// assert!(keys(r#"{"statusLine": {"type": "command", "command": "mine"}}"#).is_empty());
/// assert!(keys("not json").is_empty());
/// ```
pub fn keys(text: &str) -> Vec<&'static str> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let mut keys = Vec::new();
    if value["enabledPlugins"].get(PLUGIN).is_some() {
        keys.push(KEYS[0]);
    }
    if value["extraKnownMarketplaces"].get(NAME).is_some() {
        keys.push(KEYS[1]);
    }
    if value["statusLine"]["command"].as_str().map(str::trim) == Some(STATUSLINE) {
        keys.push(KEYS[2]);
    }
    keys
}

/// The settings text `text` with no key of riff 3 ([`keys`]), or `None`
/// when it has none. It keeps each other key in its order, and removes
/// `enabledPlugins` or `extraKnownMarketplaces` when it is empty after.
///
/// ```
/// use riff::old_plugin::without;
///
/// let text = r#"{"model":"opus","enabledPlugins":{"riff@riff":true},"statusLine":{"command":"riff statusline"},"theme":"auto"}"#;
/// let new = without(text)?.unwrap();
/// assert_eq!(new, "{\n  \"model\": \"opus\",\n  \"theme\": \"auto\"\n}\n");
/// assert_eq!(without(&new)?, None);
/// assert!(without("[1]").is_ok_and(|n| n.is_none()), "not an object: no key of riff 3");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn without(text: &str) -> Result<Option<String>> {
    let found = keys(text);
    if found.is_empty() {
        return Ok(None);
    }
    let mut value: Value = serde_json::from_str(text)?;
    let root = value
        .as_object_mut()
        .context("the settings are not a JSON object")?;
    for (key, name) in [("enabledPlugins", PLUGIN), ("extraKnownMarketplaces", NAME)] {
        let empty = match root.get_mut(key) {
            Some(Value::Object(map)) => {
                map.shift_remove(name);
                map.is_empty()
            }
            _ => false,
        };
        if empty {
            root.shift_remove(key);
        }
    }
    if found.contains(&KEYS[2]) {
        root.shift_remove("statusLine");
    }
    let mut out = serde_json::to_string_pretty(&value)?;
    out.push('\n');
    Ok(Some(out))
}

/// What is left of the plugin of riff 3 on a machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    /// The folder of the plugin, when it is there.
    pub dir: Option<PathBuf>,
    /// Each settings file with keys of riff 3, and its keys ([`keys`]).
    pub settings: Vec<(PathBuf, Vec<&'static str>)>,
}

impl Found {
    /// What is left in the plugin folder `dir` and in the settings files
    /// `settings`. riff reads a file once, also when two paths name it.
    ///
    /// ```
    /// use riff::old_plugin::Found;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let plugin = dir.path().join("claude-plugin");
    /// let user = dir.path().join("settings.json");
    /// let none = Found::of(&plugin, &[user.clone()]);
    /// assert!(none.is_empty());
    ///
    /// std::fs::create_dir_all(plugin.join("riff/hooks"))?;
    /// std::fs::write(&user, r#"{"enabledPlugins":{"riff@riff":true},"model":"opus"}"#)?;
    /// let found = Found::of(&plugin, &[user.clone(), user.clone()]);
    /// assert_eq!(found.dir.as_deref(), Some(plugin.as_path()));
    /// assert_eq!(found.settings, [(user.clone(), vec!["enabledPlugins.riff@riff"])]);
    ///
    /// found.remove()?;
    /// assert!(!plugin.exists());
    /// assert_eq!(std::fs::read_to_string(&user)?, "{\n  \"model\": \"opus\"\n}\n");
    /// assert!(Found::of(&plugin, &[user]).is_empty());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn of(dir: &Path, settings: &[PathBuf]) -> Found {
        let mut seen: Vec<PathBuf> = Vec::new();
        let mut files = Vec::new();
        for path in settings {
            let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            if seen.contains(&real) {
                continue;
            }
            seen.push(real);
            let found = keys(&std::fs::read_to_string(path).unwrap_or_default());
            if !found.is_empty() {
                files.push((path.clone(), found));
            }
        }
        Found {
            dir: dir.is_dir().then(|| dir.to_owned()),
            settings: files,
        }
    }

    /// What is left on this machine: the plugin folder in the data folder
    /// of riff, the user settings of Claude Code and, with `here`, the
    /// project and the local settings of the repository of `here`.
    pub fn here(here: Option<&Path>) -> Found {
        let Ok(data) = crate::run::data_dir() else {
            return Found::default();
        };
        let mut settings: Vec<PathBuf> = crate::claude_settings::user_settings()
            .into_iter()
            .collect();
        if let Some(here) = here {
            let project = crate::permissions::Project::of(here);
            settings.extend([project.settings(), project.local_settings()]);
        }
        Found::of(&data.join("claude-plugin"), &settings)
    }

    /// True when nothing of the plugin is left.
    pub fn is_empty(&self) -> bool {
        self.dir.is_none() && self.settings.is_empty()
    }

    /// Removes the folder of the plugin and each key of riff 3 in each
    /// settings file ([`without`]).
    pub fn remove(&self) -> Result<()> {
        if let Some(dir) = &self.dir {
            std::fs::remove_dir_all(dir).with_context(|| format!("remove {}", dir.display()))?;
        }
        for (path, _) in &self.settings {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("read {}", path.display()))?;
            if let Some(new) = without(&text).with_context(|| path.display().to_string())? {
                crate::permissions::write(path, &new)?;
            }
        }
        Ok(())
    }
}
