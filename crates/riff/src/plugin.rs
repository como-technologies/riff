//! The Claude Code plugin that `riff` carries in its binary.
//!
//! # Design
//!
//! The plugin files live in `crates/riff/claude-plugin/`. That directory
//! is a local marketplace with one plugin, `riff`. The binary holds a copy
//! of each file, so the plugin always matches the binary.
//! `riff connect claude` writes the files with [`write()`] and installs
//! them through the `claude` command.
//!
//! | File | Gives the session |
//! |---|---|
//! | `riff/.mcp.json` | The riff tools, from `riff mcp`. |
//! | `riff/skills/riff/SKILL.md` | How to use riff: the rules, the start routine, selectors, claims and `move`. |
//!
//! ```
//! let dir = tempfile::tempdir()?;
//! riff::plugin::write(dir.path())?;
//! assert!(dir.path().join("riff/.mcp.json").is_file());
//! assert!(dir.path().join("riff/skills/riff/SKILL.md").is_file());
//! # Ok::<(), std::io::Error>(())
//! ```

use std::io;
use std::path::Path;

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
