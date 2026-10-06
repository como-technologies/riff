//! Leave the riff, and join it again, from inside a session.
//!
//! # Design
//!
//! The person runs `/riff:leave` or `/riff:join` in the session, or
//! says "leave the riff" or "join the riff". The plugin command tells
//! the session to call the `leave` or the `join` tool
//! (01M3MEEFC9ZQVW2KC9FNJ75MTY, 01M3MEEFKX14QCQM0F9ZYW93PP).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as person
//!     participant A as session
//!     participant M as riff mcp
//!     participant F as left-ID
//!     participant W as riff watch
//!     participant S as riff-server
//!     P->>A: /riff:leave
//!     A->>M: leave
//!     M->>M: WIP commit and push, when it holds a claim
//!     M->>F: write
//!     M->>S: end: gone from who, claims free
//!     W->>F: sees it within 1 second
//!     W-->>A: "Do not start the watch again now", exit
//!     P->>A: /riff:join
//!     A->>M: join
//!     M->>F: remove
//!     M->>S: register, same ID
//!     A->>W: start the watch
//! ```
//!
//! Each call to `riff-server` brings a gone session back (R207). So a
//! session that left makes no call (01M3MEEFETT9A0DRWBKQTG77Z2): the
//! tools refuse, `riff mcp` sends no keep-alive, the watch stops, and
//! the hooks and the status line do not call the server. They all read
//! the mark of the leave: the file `left-ID` (see
//! [`crate::local::left`]). The file outlives `riff mcp`, so the leave
//! holds over `/clear` and a resume (01M3MEEFH79XXNZW6DWSPTEW2A). It is
//! in the state directory, so it holds over a restart of the machine
//! (01M3XQVJVJDX81QY38219SN96B).
//!
//! An entry that does not read the mark still sends nothing: the
//! client reads it before each request ([`crate::api`],
//! 01M3XQVJXWBC3DKAVWBPXPSGZS). The `leave` tool writes the mark before
//! the end call, so no other request of the session can come after the
//! end (01M3XQVK05FAT3PR43W8RNEYHY).

use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

/// The message of the WIP commit.
pub const WIP_MESSAGE: &str = "WIP: the session left the riff";

/// Commits each change in the worktree of `dir` as a WIP commit, and
/// pushes its branch to `origin`, as in a pause. It gives the branch.
/// It refuses on the default branch, on a detached `HEAD`, and when the
/// push fails.
///
/// ```
/// let dir = isolated::outside_git();
/// assert!(riff::leave::wip(dir.path()).is_err());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn wip(dir: &Path) -> Result<String> {
    let branch = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch == "HEAD" {
        bail!("the worktree has no branch, so riff cannot push its work. Check out a branch first");
    }
    let default = git(
        dir,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .ok()
    .and_then(|head| head.strip_prefix("origin/").map(str::to_owned));
    let on_default = match default {
        Some(default) => branch == default,
        None => branch == "main" || branch == "master",
    };
    if on_default {
        bail!(
            "the worktree is on the default branch {branch}. riff never pushes it. Move your \
             work to a branch first"
        );
    }
    if !git(dir, &["status", "--porcelain"])?.is_empty() {
        git(dir, &["add", "-A"])?;
        git(dir, &["commit", "--quiet", "-m", WIP_MESSAGE])?;
    }
    git(dir, &["push", "--quiet", "-u", "origin", "HEAD"])?;
    Ok(branch)
}

/// Runs git in `dir`, and gives its output with no space at the ends.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
