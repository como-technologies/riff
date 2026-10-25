//! The tools of a session image that riff needs: `claude` and `gh`, at
//! the versions of the base image of riff (01M5SNTPHHXP54YBTV09JNRY3F).
//!
//! # Design
//!
//! riff publishes the image `riff-base` (01M5SNT57ANM99NQS5SNB8XPWV). Its
//! Dockerfile pins `claude` and `gh`. Those versions are the least that
//! this riff needs ([`Tool::needed`]). The base changes only when riff
//! needs a newer tool, or for a fix of security.
//!
//! A project builds its session image `FROM` a digest of the base, and
//! pins its own image in `riff.toml`. So its image keeps the old tools
//! until someone builds it again. At the first job of a wave of a
//! repository after the daemon starts, the daemon asks the merged
//! session image for the version of each tool. When one is older than
//! this riff needs, the daemon tells the lead of the repository once, and
//! names the digest of `riff-base` to build `FROM`
//! ([`crate::text::image_old_tools`]).
//!
//! ```mermaid
//! flowchart TD
//!     J["the first job of a wave of a repository<br/>after the daemon starts"] --> P["podman run IMAGE claude --version<br/>podman run IMAGE gh --version<br/>(no network)"]
//!     P --> V{"older than<br/>deploy/base/Dockerfile?"}
//!     V -- "no, or no version" --> S["the job goes on"]
//!     V -- yes --> L["one message to the lead:<br/>the tools, and FROM riff-base@sha256:…"]
//!     L --> S
//!     L -- "the lead files the item" --> B["the project builds its image FROM the new base<br/>and pins it in riff.toml"]
//! ```
//!
//! ```
//! use riff::image_tools::{Tool, Version};
//!
//! // The output of the two commands.
//! let claude = Tool::Claude.version_in("2.1.300 (Claude Code)\n").unwrap();
//! let gh = Tool::Gh.version_in("gh version 2.102.0 (2026-10-01)\nhttps://github.com/cli/cli\n").unwrap();
//! assert_eq!(claude, "2.1.300".parse::<Version>().unwrap());
//! assert_eq!(gh.to_string(), "2.102.0");
//!
//! // A version compares by its numbers, not by its text.
//! assert!("2.9.0".parse::<Version>()? < "2.10.0".parse::<Version>()?);
//!
//! // What riff needs is in the Dockerfile of riff-base.
//! assert!(Tool::Claude.needed() >= "2.0.0".parse::<Version>()?);
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::fmt;
use std::str::FromStr;

use anyhow::{Context, bail};

/// The Dockerfile of `riff-base` at the commit of the build.
pub(crate) const BASE_DOCKERFILE: &str = include_str!("../../../deploy/base/Dockerfile");

/// A version of a tool: three numbers, as `2.1.300`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u64, u64, u64);

impl FromStr for Version {
    type Err = anyhow::Error;

    /// Reads `X.Y.Z` with three numbers. A text with other parts is no
    /// version.
    fn from_str(text: &str) -> anyhow::Result<Version> {
        let parts: Vec<&str> = text.split('.').collect();
        let [x, y, z] = parts[..] else {
            bail!("{text} is not a version of three numbers");
        };
        let number = |part: &str| {
            part.parse::<u64>()
                .with_context(|| format!("{text} is not a version of three numbers"))
        };
        Ok(Version(number(x)?, number(y)?, number(z)?))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// A tool of a session image that riff needs at a version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// `claude`, the agent.
    Claude,
    /// `gh`, the client of GitHub.
    Gh,
}

impl Tool {
    /// Each tool.
    pub const ALL: [Tool; 2] = [Tool::Claude, Tool::Gh];

    /// The name of the program.
    pub fn name(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Gh => "gh",
        }
    }

    /// The command that prints the version of the tool.
    pub fn command(self) -> [&'static str; 2] {
        [self.name(), "--version"]
    }

    /// The version in the output of [`Tool::command`]: `claude` prints
    /// `2.1.300 (Claude Code)`, and `gh` prints `gh version 2.102.0 (DATE)`
    /// and a line with a link. `None` for any other output.
    pub fn version_in(self, output: &str) -> Option<Version> {
        let line = output.lines().next()?;
        let word = match self {
            Tool::Claude => line.split_whitespace().next()?,
            Tool::Gh => line
                .strip_prefix("gh version ")?
                .split_whitespace()
                .next()?,
        };
        word.parse().ok()
    }

    /// The `ARG` of the Dockerfile of `riff-base` that pins the tool.
    fn arg(self) -> &'static str {
        match self {
            Tool::Claude => "CLAUDE_VERSION",
            Tool::Gh => "GH_VERSION",
        }
    }

    /// The version of the tool in the Dockerfile of `riff-base` at the
    /// commit of the build: the least that this riff needs. A test checks
    /// that the Dockerfile pins it.
    pub fn needed(self) -> Version {
        needed_in(BASE_DOCKERFILE, self)
            .unwrap_or_else(|e| panic!("deploy/base/Dockerfile pins no {}: {e:#}", self.name()))
    }

    /// The version `has` of the tool in the output of its command, when
    /// it is older than [`Tool::needed`]. `None` when the output has no
    /// version: riff then says nothing.
    ///
    /// ```
    /// use riff::image_tools::Tool;
    ///
    /// let old = Tool::Claude.old_in("0.0.1 (Claude Code)").unwrap();
    /// assert_eq!(old.has.to_string(), "0.0.1");
    /// assert_eq!(old.needs, Tool::Claude.needed());
    ///
    /// assert!(Tool::Claude.old_in("999.0.0 (Claude Code)").is_none());
    /// assert!(Tool::Claude.old_in("not a version").is_none());
    /// ```
    pub fn old_in(self, output: &str) -> Option<Old> {
        let has = self.version_in(output)?;
        let needs = self.needed();
        (has < needs).then_some(Old {
            tool: self,
            has,
            needs,
        })
    }
}

/// A tool of a session image that is older than riff needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Old {
    /// The tool.
    pub tool: Tool,
    /// The version in the image.
    pub has: Version,
    /// The least version that riff needs.
    pub needs: Version,
}

/// The version of `tool` that `dockerfile` pins with its `ARG`.
fn needed_in(dockerfile: &str, tool: Tool) -> anyhow::Result<Version> {
    let prefix = format!("ARG {}=", tool.arg());
    dockerfile
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .with_context(|| format!("no line {prefix}VERSION"))?
        .parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        text.parse().unwrap()
    }

    #[test]
    fn a_version_has_three_numbers() {
        assert_eq!(version("2.1.300").to_string(), "2.1.300");
        for bad in [
            "", "2", "2.1", "2.1.3.4", "2.1.x", "v2.1.3", "2.1.-3", "2..3",
        ] {
            assert!(bad.parse::<Version>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn versions_compare_by_their_numbers() {
        assert!(version("2.1.300") < version("2.1.301"));
        assert!(version("2.9.0") < version("2.10.0"));
        assert!(version("1.999.999") < version("2.0.0"));
        assert_eq!(version("2.1.300"), version("2.1.300"));
    }

    #[test]
    fn the_version_is_in_the_first_line_of_the_output() {
        let gh =
            "gh version 2.102.0 (2026-10-01)\nhttps://github.com/cli/cli/releases/tag/v2.102.0\n";
        assert_eq!(Tool::Gh.version_in(gh), Some(version("2.102.0")));
        assert_eq!(
            Tool::Claude.version_in("2.1.300 (Claude Code)"),
            Some(version("2.1.300"))
        );
        for output in [
            "",
            "\n",
            "claude: not found",
            "gh version next",
            "version 2.1.0",
        ] {
            assert_eq!(Tool::Claude.version_in(output), None, "{output:?}");
            assert_eq!(Tool::Gh.version_in(output), None, "{output:?}");
        }
        // The program that prints the output counts: a gh line is no claude version.
        assert_eq!(Tool::Claude.version_in(gh), None);
        assert_eq!(Tool::Gh.version_in("2.1.300 (Claude Code)"), None);
    }

    /// The Dockerfile of `riff-base` pins both tools: riff needs them.
    #[test]
    fn riff_needs_the_versions_that_the_base_image_pins() {
        for tool in Tool::ALL {
            let needed = needed_in(BASE_DOCKERFILE, tool).unwrap();
            assert_eq!(tool.needed(), needed);
            assert!(needed >= version("1.0.0"), "{} {needed}", tool.name());
        }
        assert!(needed_in("FROM debian\n", Tool::Gh).is_err());
        assert!(needed_in("ARG GH_VERSION=latest\n", Tool::Gh).is_err());
    }

    #[test]
    fn a_tool_is_old_when_it_is_older_than_the_base_image_pins() {
        for tool in Tool::ALL {
            let needs = tool.needed();
            let print = |v: &str| match tool {
                Tool::Claude => format!("{v} (Claude Code)"),
                Tool::Gh => format!("gh version {v} (2020-01-01)"),
            };
            assert_eq!(tool.old_in(&print(&needs.to_string())), None);
            assert_eq!(tool.old_in(&print("999.0.0")), None);
            let old = tool.old_in(&print("0.0.1")).unwrap();
            assert_eq!(
                (old.tool, old.has, old.needs),
                (tool, version("0.0.1"), needs)
            );
        }
    }
}
