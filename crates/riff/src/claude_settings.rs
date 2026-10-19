//! Where the user settings of Claude Code are.
//!
//! `riff setup` and `riff init` add the permission rules of riff to a
//! settings file of Claude Code, and look in the user settings for a
//! rule that is there already.

use std::path::{Path, PathBuf};

/// The user settings of Claude Code: `$CLAUDE_CONFIG_DIR/settings.json`,
/// or `$HOME/.claude/settings.json`.
///
/// ```
/// use std::path::Path;
///
/// let p = riff::claude_settings::settings_from(None, Some("/home/mike".into()));
/// assert_eq!(p.as_deref(), Some(Path::new("/home/mike/.claude/settings.json")));
/// let p = riff::claude_settings::settings_from(Some("/cfg".into()), Some("/home/mike".into()));
/// assert_eq!(p.as_deref(), Some(Path::new("/cfg/settings.json")));
/// ```
pub fn settings_from(
    config_dir: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    match (config_dir.filter(|d| !d.is_empty()), home) {
        (Some(dir), _) => Some(PathBuf::from(dir).join("settings.json")),
        (None, Some(home)) => Some(Path::new(&home).join(".claude/settings.json")),
        (None, None) => None,
    }
}

/// [`settings_from`] with the values from the environment.
pub fn user_settings() -> Option<PathBuf> {
    settings_from(
        std::env::var_os("CLAUDE_CONFIG_DIR"),
        std::env::var_os("HOME"),
    )
}
