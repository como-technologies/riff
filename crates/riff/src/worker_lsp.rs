//! The language servers of a worker.
//!
//! # Design
//!
//! A plugin of Claude Code can bring a language server, for example
//! `rust-analyzer-lsp` brings `rust-analyzer`. Claude Code starts it as
//! a child of `claude`, and it lives as long as the `claude` process. A
//! worker keeps one `claude` process for many items (see
//! [`crate::next`]), and each item has its own worktree. So a worker
//! kept a language server of some GB for each worktree that was gone.
//!
//! `riff workers start` turns off each installed plugin with a language
//! server in the flag settings of the worker (01M3ZJ1FAF7EJXP9CSET8ZY1K3):
//! `"enabledPlugins": {"PLUGIN": false}`. The flag settings outrank the
//! user settings, and the user settings file does not change. The env
//! variable `ENABLE_LSP_TOOL=0` does not stop the server, so riff does
//! not use it.
//!
//! ```mermaid
//! flowchart LR
//!     I["plugins/installed_plugins.json"] --> P[plugins]
//!     K["plugins/known_marketplaces.json<br/>marketplace.json: lspServers"] --> P
//!     D[".lsp.json, plugin.json<br/>of the install path"] --> P
//!     P --> S["--settings<br/>enabledPlugins: PLUGIN false"]
//! ```
//!
//! A plugin has a language server when its entry in the file of its
//! marketplace has `lspServers`, or when its install path has the file
//! `.lsp.json`, or `.claude-plugin/plugin.json` with `lspServers`. A
//! file that is missing or not JSON counts as no server.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The config directory of Claude Code: `$CLAUDE_CONFIG_DIR`, else
/// `$HOME/.claude`. An empty value counts as unset.
///
/// ```
/// use riff::worker_lsp::claude_dir_from;
/// use std::path::PathBuf;
/// assert_eq!(claude_dir_from(Some("/c".into()), Some("/h".into())), Some(PathBuf::from("/c")));
/// assert_eq!(claude_dir_from(Some("".into()), Some("/h".into())), Some(PathBuf::from("/h/.claude")));
/// assert_eq!(claude_dir_from(None, None), None);
/// ```
pub fn claude_dir_from(
    config: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let set = |v: Option<std::ffi::OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    set(config).or_else(|| set(home).map(|h| h.join(".claude")))
}

/// The config directory of Claude Code of this process. See
/// [`claude_dir_from`].
pub fn claude_dir() -> Option<PathBuf> {
    claude_dir_from(
        std::env::var_os("CLAUDE_CONFIG_DIR"),
        std::env::var_os("HOME"),
    )
}

/// A JSON file, or `None` when it is missing or not JSON.
fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// True when the JSON file at `path` has the key `lspServers`.
fn names_servers(path: &Path) -> bool {
    read_json(path).is_some_and(|v| v.get("lspServers").is_some())
}

/// True when the plugin `name` of the marketplace `market` has a
/// language server in the file of the marketplace.
fn in_marketplace(dir: &Path, name: &str, market: &str) -> bool {
    let Some(known) = read_json(&dir.join("plugins/known_marketplaces.json")) else {
        return false;
    };
    let Some(place) = known
        .get(market)
        .and_then(|m| m.get("installLocation"))
        .and_then(Value::as_str)
    else {
        return false;
    };
    let Some(file) = read_json(&Path::new(place).join(".claude-plugin/marketplace.json")) else {
        return false;
    };
    file.get("plugins")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|p| {
            p.get("name").and_then(Value::as_str) == Some(name) && p.get("lspServers").is_some()
        })
}

/// True when an install path of a plugin has a language server.
fn in_install(entries: &Value) -> bool {
    entries
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e.get("installPath").and_then(Value::as_str))
        .map(Path::new)
        .any(|p| {
            p.join(".lsp.json").is_file() || names_servers(&p.join(".claude-plugin/plugin.json"))
        })
}

/// The installed plugins of the config directory `dir` that have a
/// language server, as `NAME@MARKETPLACE`, sorted
/// (01M3ZJ1FAF7EJXP9CSET8ZY1K3).
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let market = dir.path().join("m");
/// std::fs::create_dir_all(market.join(".claude-plugin"))?;
/// std::fs::write(
///     market.join(".claude-plugin/marketplace.json"),
///     r#"{"plugins": [{"name": "rust-lsp", "lspServers": {}}, {"name": "tools"}]}"#,
/// )?;
/// std::fs::create_dir_all(dir.path().join("plugins"))?;
/// std::fs::write(
///     dir.path().join("plugins/known_marketplaces.json"),
///     format!(r#"{{"official": {{"installLocation": {:?}}}}}"#, market),
/// )?;
/// std::fs::write(
///     dir.path().join("plugins/installed_plugins.json"),
///     r#"{"plugins": {"rust-lsp@official": [], "tools@official": [], "riff@riff": []}}"#,
/// )?;
/// assert_eq!(riff::worker_lsp::plugins(dir.path()), ["rust-lsp@official"]);
/// assert!(riff::worker_lsp::plugins(&dir.path().join("none")).is_empty());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn plugins(dir: &Path) -> Vec<String> {
    let Some(installed) = read_json(&dir.join("plugins/installed_plugins.json")) else {
        return Vec::new();
    };
    let Some(map) = installed.get("plugins").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut found: Vec<String> = map
        .iter()
        .filter(|(key, entries)| {
            let (name, market) = key.split_once('@').unwrap_or((key, ""));
            in_install(entries) || in_marketplace(dir, name, market)
        })
        .map(|(key, _)| key.clone())
        .collect();
    found.sort();
    found
}

/// The plugins with a language server of this process. See [`plugins`].
pub fn here() -> Vec<String> {
    claude_dir().map(|d| plugins(&d)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(dir: &Path, json: &str) {
        std::fs::create_dir_all(dir.join("plugins")).unwrap();
        std::fs::write(dir.join("plugins/installed_plugins.json"), json).unwrap();
    }

    #[test]
    fn a_plugin_with_lsp_json_in_its_install_path_counts() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cache/go");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join(".lsp.json"), "{}").unwrap();
        installed(
            dir.path(),
            &format!(r#"{{"plugins": {{"go@m": [{{"installPath": {p:?}}}]}}}}"#),
        );
        assert_eq!(plugins(dir.path()), ["go@m"]);
    }

    #[test]
    fn a_plugin_json_with_lsp_servers_counts() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cache/py");
        std::fs::create_dir_all(p.join(".claude-plugin")).unwrap();
        std::fs::write(
            p.join(".claude-plugin/plugin.json"),
            r#"{"name": "py", "lspServers": {"pyright": {}}}"#,
        )
        .unwrap();
        let q = dir.path().join("cache/skills");
        std::fs::create_dir_all(q.join(".claude-plugin")).unwrap();
        std::fs::write(
            q.join(".claude-plugin/plugin.json"),
            r#"{"name": "skills"}"#,
        )
        .unwrap();
        installed(
            dir.path(),
            &format!(
                r#"{{"plugins": {{"skills@m": [{{"installPath": {q:?}}}], "py@m": [{{"installPath": {p:?}}}]}}}}"#
            ),
        );
        assert_eq!(plugins(dir.path()), ["py@m"]);
    }

    #[test]
    fn a_file_that_is_not_json_counts_as_no_server() {
        let dir = tempfile::tempdir().unwrap();
        installed(dir.path(), "not json");
        assert!(plugins(dir.path()).is_empty());
        installed(dir.path(), r#"{"plugins": {"x@gone": []}}"#);
        std::fs::write(dir.path().join("plugins/known_marketplaces.json"), "{").unwrap();
        assert!(plugins(dir.path()).is_empty());
    }
}
