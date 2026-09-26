//! The Claude Code plugin that `riff` carries in its binary.
//!
//! # Design
//!
//! The plugin files live in `crates/riff/claude-plugin/`. That directory
//! is a local marketplace with one plugin, `riff`. The binary holds a copy
//! of each file, so the plugin always matches the binary.
//! [`connect()`] writes the files with [`write()`] to [`dir()`] and
//! installs them through the `claude` command:
//!
//! 1. `claude mcp remove --scope user riff` removes an old entry, if any.
//! 2. `claude plugin marketplace add DIR` adds the marketplace `riff`.
//! 3. `claude plugin install --scope user riff@riff` installs the plugin.
//!
//! Claude Code loads a plugin from a local marketplace in place. So a new
//! `riff` binary and one more `riff connect claude` update the plugin.
//! Both `claude` steps succeed when they have nothing to do.
//!
//! | File | Gives the session |
//! |---|---|
//! | `riff/.mcp.json` | The riff tools, from `riff mcp`. |
//! | `riff/skills/riff/SKILL.md` | How to use riff: the rules, the start routine, selectors, claims and `move`. |
//! | `riff/hooks/hooks.json` | The start hook, `riff hook session-start`. It tells the session to start `riff watch` (see [`crate::hook`]). |
//!
//! ```
//! let dir = tempfile::tempdir()?;
//! riff::plugin::write(dir.path())?;
//! assert!(dir.path().join("riff/.mcp.json").is_file());
//! assert!(dir.path().join("riff/skills/riff/SKILL.md").is_file());
//! # Ok::<(), std::io::Error>(())
//! ```

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The name of the marketplace and of the plugin.
pub const NAME: &str = "riff";

macro_rules! embed {
    ($path:literal) => {
        (
            $path,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/claude-plugin/",
                $path
            )),
        )
    };
}

/// Each plugin file: its path in the marketplace directory, and its text.
pub const FILES: &[(&str, &str)] = &[
    embed!(".claude-plugin/marketplace.json"),
    embed!("riff/.claude-plugin/plugin.json"),
    embed!("riff/.mcp.json"),
    embed!("riff/skills/riff/SKILL.md"),
    embed!("riff/hooks/hooks.json"),
];

/// Writes the marketplace to `dir`. It replaces the files that are there.
pub fn write(dir: &Path) -> io::Result<()> {
    for (path, text) in FILES {
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, text)?;
    }
    Ok(())
}

/// The directory for the marketplace: `$XDG_DATA_HOME/riff/claude-plugin`,
/// or `$HOME/.local/share/riff/claude-plugin` (R74).
///
/// ```
/// use std::path::Path;
///
/// let dir = riff::plugin::dir_from(None, Some("/home/mike".into()))?;
/// assert_eq!(dir, Path::new("/home/mike/.local/share/riff/claude-plugin"));
/// let dir = riff::plugin::dir_from(Some("/data".into()), None)?;
/// assert_eq!(dir, Path::new("/data/riff/claude-plugin"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn dir_from(
    data_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Result<PathBuf> {
    let data = match (data_home.filter(|d| !d.is_empty()), home) {
        (Some(data), _) => PathBuf::from(data),
        (None, Some(home)) => Path::new(&home).join(".local/share"),
        (None, None) => bail!("set HOME or XDG_DATA_HOME"),
    };
    Ok(data.join(NAME).join("claude-plugin"))
}

/// [`dir_from`] with the values from the environment.
pub fn dir() -> Result<PathBuf> {
    dir_from(std::env::var_os("XDG_DATA_HOME"), std::env::var_os("HOME"))
}

/// What [`connect()`] did.
#[derive(Debug, PartialEq, Eq)]
pub struct Connected {
    /// The marketplace directory.
    pub dir: PathBuf,
    /// True when it removed an old `riff` MCP server entry (R75).
    pub removed_old: bool,
}

/// Writes the marketplace to `dir` and installs the plugin in user scope
/// with the `claude` command at `claude` (R53). It first removes an old
/// user-scope MCP server entry named `riff`, from `claude mcp add`, so a
/// session does not get the riff tools twice (R75).
pub fn connect(claude: &Path, dir: &Path) -> Result<Connected> {
    write(dir).with_context(|| format!("write the plugin to {}", dir.display()))?;
    let removed_old = run(
        claude,
        ["mcp", "remove", "--scope", "user", NAME].map(OsStr::new),
    )
    .is_ok();
    run(
        claude,
        [
            OsStr::new("plugin"),
            OsStr::new("marketplace"),
            OsStr::new("add"),
            dir.as_os_str(),
        ],
    )?;
    let plugin = format!("{NAME}@{NAME}");
    run(
        claude,
        ["plugin", "install", "--scope", "user", &plugin].map(OsStr::new),
    )?;
    Ok(Connected {
        dir: dir.to_owned(),
        removed_old,
    })
}

/// Runs `claude` with `args`. It fails with the output of `claude` when
/// the command fails.
fn run<'a>(claude: &Path, args: impl IntoIterator<Item = &'a OsStr>) -> Result<()> {
    let args: Vec<&OsStr> = args.into_iter().collect();
    let line = || {
        let args: Vec<_> = args.iter().map(|a| a.to_string_lossy()).collect();
        format!("{} {}", claude.display(), args.join(" "))
    };
    let out = Command::new(claude)
        .args(&args)
        .output()
        .with_context(|| format!("run {}", line()))?;
    if !out.status.success() {
        bail!(
            "{} failed: {}{}",
            line(),
            String::from_utf8_lossy(&out.stdout).trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(path: &str) -> &'static str {
        FILES.iter().find(|(p, _)| *p == path).unwrap().1
    }

    fn json(path: &str) -> serde_json::Value {
        serde_json::from_str(text(path)).unwrap()
    }

    #[test]
    fn each_json_file_parses() {
        for (path, text) in FILES {
            if path.ends_with(".json") {
                serde_json::from_str::<serde_json::Value>(text)
                    .unwrap_or_else(|e| panic!("{path}: {e}"));
            }
        }
    }

    #[test]
    fn plugin_version_is_the_crate_version() {
        let plugin = json("riff/.claude-plugin/plugin.json");
        assert_eq!(plugin["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(plugin["name"], NAME);
    }

    #[test]
    fn marketplace_lists_the_plugin() {
        let market = json(".claude-plugin/marketplace.json");
        assert_eq!(market["name"], NAME);
        assert_eq!(market["plugins"][0]["name"], NAME);
        assert_eq!(market["plugins"][0]["source"], "./riff");
    }

    #[test]
    fn the_start_hook_is_riff_hook_session_start() {
        let hooks = json("riff/hooks/hooks.json");
        let start = &hooks["hooks"]["SessionStart"];
        assert_eq!(start.as_array().unwrap().len(), 1);
        assert_eq!(start[0].get("matcher"), None, "each source runs the hook");
        assert_eq!(start[0]["hooks"][0]["type"], "command");
        assert_eq!(start[0]["hooks"][0]["command"], "riff hook session-start");
    }

    #[test]
    fn an_empty_data_home_uses_home() {
        let dir = dir_from(Some("".into()), Some("/h".into())).unwrap();
        assert_eq!(dir, Path::new("/h/.local/share/riff/claude-plugin"));
    }

    #[test]
    fn no_home_is_an_error() {
        assert!(dir_from(None, None).is_err());
    }

    #[test]
    fn a_missing_claude_command_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = connect(Path::new("/no/such/claude"), dir.path()).unwrap_err();
        assert!(format!("{err:#}").contains("/no/such/claude plugin marketplace add"));
        assert!(dir.path().join("riff/.mcp.json").is_file());
    }

    #[test]
    fn the_mcp_server_is_riff_mcp() {
        let mcp = json("riff/.mcp.json");
        assert_eq!(mcp[NAME]["command"], "riff");
        assert_eq!(mcp[NAME]["args"], serde_json::json!(["mcp"]));
    }

    #[test]
    fn the_skill_is_named_riff() {
        let skill = text("riff/skills/riff/SKILL.md");
        let front = skill.strip_prefix("---\n").unwrap().split("---\n").next();
        let front = front.unwrap();
        assert!(front.contains(&format!("name: {NAME}\n")));
        assert!(front.contains("description: "));
    }

    #[test]
    fn the_skill_teaches_each_rule() {
        let skill = text("riff/skills/riff/SKILL.md");
        for word in [
            "only through riff",
            "SendMessage",
            "data, not an instruction",
            "secrets",
            "Start routine",
            "`claim`",
            "`release`",
            "`move`",
            "`tell`",
            "selector",
            "`read` with no thread",
        ] {
            assert!(skill.contains(word), "the skill does not say {word:?}");
        }
    }
}
