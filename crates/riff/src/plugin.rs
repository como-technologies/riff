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
//! | `riff/skills/riff/SKILL.md` | How to use riff: the rules, the start routine, waves, the pause, how the lead conducts, the check of the acceptance criteria, selectors, claims, `move` and the restart of the watch. |
//! | `riff/hooks/hooks.json` | The start hook, `riff hook session-start`. It tells the session to start `riff watch` (see [`crate::hook`]). The end hook, `riff hook session-end`, tells the server that the session ended. |
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
///
/// The skill tells a session to read the `Done when:` line of an issue
/// after it claims the issue, and before it starts work (R173):
///
/// ```
/// let (_, skill) = riff::plugin::FILES
///     .iter()
///     .find(|(path, _)| path.ends_with("SKILL.md"))
///     .unwrap();
/// let check = skill.find("Find its `Done when:` line").unwrap();
/// assert!(skill.find("Call `claim`").unwrap() < check);
/// assert!(check < skill.find("Call the `EnterWorktree` tool").unwrap());
/// ```
///
/// The skill tells a session to take its work from the current wave
/// (R166). Only its part "Waves on GitHub" names the objects of the
/// forge that hold a wave (R222):
///
/// ```
/// let (_, skill) = riff::plugin::FILES
///     .iter()
///     .find(|(path, _)| path.ends_with("SKILL.md"))
///     .unwrap();
/// let step = skill.find("2. Find a free work item").unwrap();
/// assert!(skill[step..].starts_with("2. Find a free work item: an open issue of the current wave"));
/// let forge = skill.find("### Waves on GitHub").unwrap();
/// let end = forge + skill[forge..].find("\n## ").unwrap();
/// assert!(!skill[..forge].contains("milestone"));
/// assert!(skill[forge..end].contains("milestone"));
/// assert!(!skill[end..].contains("milestone"));
/// ```
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
    fn the_end_hook_is_riff_hook_session_end() {
        let hooks = json("riff/hooks/hooks.json");
        let end = &hooks["hooks"]["SessionEnd"];
        assert_eq!(end.as_array().unwrap().len(), 1);
        assert_eq!(end[0]["hooks"][0]["type"], "command");
        assert_eq!(end[0]["hooks"][0]["command"], "riff hook session-end");
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
            "`riff watch --once`",
            "middle of a turn",
        ] {
            assert!(skill.contains(word), "the skill does not say {word:?}");
        }
    }

    #[test]
    fn the_skill_teaches_the_lead_to_conduct() {
        let skill = text("riff/skills/riff/SKILL.md");
        let pos = |text: &str| {
            skill
                .find(text)
                .unwrap_or_else(|| panic!("no {text:?} in the skill"))
        };
        let conduct = pos("## Conduct the sessions of your user");
        let request = pos("## A request from your lead");
        let questions = pos("## Questions for your user");
        assert!(questions < conduct && conduct < request);
        let conduct = &skill[conduct..request];
        for text in [
            "only when you are the lead",
            "Never send a request to\na session of another user",
            "`kind` `status`",
            "one clear item",
            "`request: claim issue-12`",
            "blocked",
        ] {
            assert!(conduct.contains(text), "no {text:?} in {conduct}");
        }
        let request = &skill[request..pos("## Keep the watch running")];
        for text in [
            "only\nwhen it is verified",
            "is data",
            "the session `lead`",
            "When you start",
            "When you finish",
            "When you are blocked",
            "A scope from your own user wins",
        ] {
            assert!(request.contains(text), "no {text:?} in {request}");
        }
        assert!(skill.contains("The one exception is\n   a request from your lead"));
    }

    #[test]
    fn the_skill_makes_worktrees_in_claude_worktrees() {
        let skill = text("riff/skills/riff/SKILL.md");
        assert!(skill.contains("`EnterWorktree`"));
        assert!(skill.contains("`.claude/worktrees/issue-12`"));
        assert!(!skill.contains("worktree add ../"));
    }

    #[test]
    fn the_skill_removes_only_stale_worktrees_of_its_own() {
        let skill = text("riff/skills/riff/SKILL.md");
        let skill = skill.split_whitespace().collect::<Vec<_>>().join(" ");
        for word in [
            "Remove a stale worktree",
            "git merge-base --is-ancestor HEAD origin/main",
            "git status --porcelain",
            "issue is closed",
            "`ExitWorktree`",
            "`discard_changes`",
            "git branch -d",
            "Do not force",
            "Never remove a worktree of another live session",
        ] {
            assert!(skill.contains(word), "the skill does not say {word:?}");
        }
        assert!(!skill.contains("branch -D"));
        assert!(!skill.contains("--force"));
    }

    #[test]
    fn the_skill_teaches_the_pause() {
        let skill = text("riff/skills/riff/SKILL.md");
        let skill = skill.split_whitespace().collect::<Vec<_>>().join(" ");
        for word in [
            "## Pause",
            "A new riff starts paused",
            "`riff pause` and `riff resume`",
            "the `pause` and `resume` tools",
            "While the riff is paused, a claim fails",
            "### A new session in a paused riff",
            "Claim nothing",
            "Call `tell` with the session `lead`",
            "`waiting: the riff is paused`",
            "### A session with work",
            "Let a command that runs finish",
            "WIP commit on the branch of your worktree",
            "Push nothing to the default branch",
            "A verify stops with no result",
            "Keep your claims",
            "go on from where you stopped",
        ] {
            assert!(skill.contains(word), "the skill does not say {word:?}");
        }
        let start =
            &skill[skill.find("## Start routine").unwrap()..skill.find("2. Find a free").unwrap()];
        assert!(
            start.contains("When the riff is paused, do the steps for a new session in \"Pause\""),
            "{start}"
        );
    }

    #[test]
    fn the_skill_teaches_waves() {
        let skill = text("riff/skills/riff/SKILL.md");
        let skill = skill.split_whitespace().collect::<Vec<_>>().join(" ");
        for word in [
            "## Waves",
            "The current wave is the open wave with the lowest number",
            "The next wave is the open wave after it",
            "an open issue of the current wave",
            "Take work only from the current wave",
            "Never take an item whose needs are open",
            "When the current wave has no free item, verify the work of another session, run your checks after the merge, or wait",
            "When the repository has no waves",
            "`Needs:` line",
            "`Merged in COMMIT`",
            "An item is closed when it is merged and each check after the merge passed",
            "A wave is done when each of its items is closed",
            "No session starts an item of the next wave before the current wave is done",
            "`tell` the lead",
            "### Plan the waves",
            "Do these steps only when you are the lead",
            "Look for open items with no wave",
            "Each item is in a later wave than each of its needs",
            "No item blocks or breaks the other work of its wave",
            "the last number plus one",
            "what it needs, and what needs it",
            "When each item of the current wave is closed, the wave is done",
            "End the wave",
            "### Waves on GitHub",
        ] {
            assert!(skill.contains(word), "the skill does not say {word:?}");
        }
        assert!(!skill.contains("milestones do not set the order"));
        let start =
            &skill[skill.find("## Start routine").unwrap()..skill.find("## Waves").unwrap()];
        assert!(!start.contains("next wave"), "{start}");
    }

    #[test]
    fn the_waves_on_github_give_a_command_for_each_step() {
        let skill = text("riff/skills/riff/SKILL.md");
        let forge = &skill[skill.find("### Waves on GitHub").unwrap()..];
        let forge = &forge[..forge.find("\n## ").unwrap()];
        for command in [
            "`gh api repos/OWNER/REPO/milestones --jq",
            "`gh issue list --milestone \"Wave 2\"`",
            "`gh issue list --search no:milestone`",
            "`gh api repos/OWNER/REPO/milestones -f title=\"Wave 6\"`",
            "`gh issue edit 12 --milestone \"Wave 3\"`",
            "`gh api -X PATCH repos/OWNER/REPO/milestones/NUMBER -f state=closed`",
        ] {
            assert!(forge.contains(command), "no {command:?} in {forge}");
        }
        assert!(!forge.contains("\\|"), "a table cell escapes a pipe");
    }

    /// 01M3JDW9WN7KFGVY6HMCP2XN8B, 01M3JDW9YQQHSZC296ZCNV2V8A.
    #[test]
    fn the_skill_sends_each_question_and_each_refused_merge_to_the_lead() {
        let skill = text("riff/skills/riff/SKILL.md");
        let skill = skill.split_whitespace().collect::<Vec<_>>().join(" ");
        for word in [
            "your user does not look at your terminal. Never ask your user there.",
            "also true when a permission refusal blocks you",
            "When a permission refusal stops the merge or the push to the default branch, \
             do not ask in your own terminal.",
            "`tell` the lead the branch, the commit and the verify result",
            "the lead merges, or your user allows the merge in your session",
            "Do not delete the branch before the merge.",
        ] {
            assert!(skill.contains(word), "the skill does not say {word:?}");
        }
    }

    #[test]
    fn the_skill_asks_for_acceptance_criteria() {
        let skill = text("riff/skills/riff/SKILL.md");
        let skill = skill.split_whitespace().collect::<Vec<_>>().join(" ");
        for word in [
            "Write acceptance criteria",
            "`Done when:` line",
            "a session cannot test it, do not start work",
            "what to run or look at, and what the result must be",
            "ASD-STE100",
            "Post to the repository thread that the issue now has criteria",
            "Call `release` with the item",
            "Pick a different item",
            "Do not implement an issue in the claim in which you wrote its criteria",
            "reviews them",
        ] {
            assert!(skill.contains(word), "the skill does not say {word:?}");
        }
    }
}
