//! The written plugin passes the Claude Code validator, and `riff connect
//! claude` installs it.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn claude_accepts_the_written_plugin() {
    let dir = tempfile::tempdir().unwrap();
    riff::plugin::write(dir.path()).unwrap();
    let Ok(out) = Command::new("claude").arg("--version").output() else {
        eprintln!("skip: the claude command is not installed");
        return;
    };
    assert!(out.status.success());
    for path in [dir.path().to_owned(), dir.path().join(riff::plugin::NAME)] {
        let out = Command::new("claude")
            .args(["plugin", "validate", "--strict"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}: {}{}",
            path.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// A fake `claude` command that logs its arguments. It fails `mcp
/// remove`, as `claude` does when there is no old entry.
fn fake_claude(dir: &Path, remove_status: u8) -> PathBuf {
    let path = dir.join("claude");
    let log = dir.join("log");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\necho \"$*\" >> {}\n[ \"$1\" = mcp ] && exit {remove_status}\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn connect(bin: &Path, data: &Path, cwd: &Path) -> assert_cmd::assert::Assert {
    assert_cmd::Command::cargo_bin("riff")
        .unwrap()
        .args(["connect", "claude", "--claude"])
        .arg(bin)
        .env("XDG_DATA_HOME", data)
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(cwd)
        .assert()
}

#[test]
fn connect_writes_the_plugin_and_runs_claude() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_claude(tmp.path(), 1);
    let market = tmp.path().join("riff/claude-plugin");
    connect(&bin, tmp.path(), tmp.path())
        .success()
        .stdout(format!(
            "Installed the riff plugin from {}. Start a new Claude Code session to use it.\n",
            market.display()
        ));
    assert!(market.join("riff/.mcp.json").is_file());
    let log = std::fs::read_to_string(tmp.path().join("log")).unwrap();
    assert_eq!(
        log,
        format!(
            "mcp remove --scope user riff\n\
             plugin marketplace add {}\n\
             plugin install --scope user riff@riff\n",
            market.display()
        )
    );
}

#[test]
fn connect_writes_the_skill_with_the_criteria_check() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_claude(tmp.path(), 1);
    connect(&bin, tmp.path(), tmp.path()).success();
    let skill = tmp
        .path()
        .join("riff/claude-plugin/riff/skills/riff/SKILL.md");
    let skill = std::fs::read_to_string(skill).unwrap();
    let start = skill.find("## Start routine").unwrap();
    let check = skill.find("Find its `Done when:` line").unwrap();
    let section = skill.find("## Write acceptance criteria").unwrap();
    assert!(start < check && check < section, "{skill}");
    assert!(skill[section..].contains("Call `release` with the item."));
}

#[test]
fn connect_writes_the_skill_with_the_verify_flow() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_claude(tmp.path(), 1);
    connect(&bin, tmp.path(), tmp.path()).success();
    let skill = tmp
        .path()
        .join("riff/claude-plugin/riff/skills/riff/SKILL.md");
    let skill = std::fs::read_to_string(skill).unwrap();
    let pos = |text: &str| {
        skill
            .find(text)
            .unwrap_or_else(|| panic!("no {text:?} in {skill}"))
    };

    // The start routine asks for a verify before the merge and the release.
    let start = pos("## Start routine");
    let verify = pos("ask another session to verify the work");
    let merge = pos("On a pass, merge to the default branch");
    let release = pos("that you are done, then call `release`.");
    assert!(start < verify && verify < merge && merge < release);

    // The author and the verifier each have their steps.
    let section = &skill[pos("## Verify finished work")..pos("## Remove a stale worktree")];
    let ask = section.find("### Ask for a verify").unwrap();
    let check = section
        .find("### Verify the work of another session")
        .unwrap();
    assert!(ask < check);
    for text in [
        "A session never verifies its own work.",
        "`[{\"repo\": \"OWNER/REPO\"}]`",
        "Do not merge without a pass.",
        "`verify-issue-12`",
        "Do not change the code.",
        "`[{\"claim\": \"issue-12\"}]`",
    ] {
        assert!(section.contains(text), "no {text:?} in {section}");
    }

    // The verify worktree has a name of its own and works from any
    // worktree (R202): add it by path, then remove it with no force.
    let verifier = &section[check..];
    let add = verifier
        .find("`git worktree add --detach MAIN/.claude/worktrees/verify-issue-12-a6cf COMMIT`")
        .unwrap();
    let enter = verifier
        .find("Call `EnterWorktree` with that path")
        .unwrap();
    let back = verifier.find("Go back to where you came").unwrap();
    let remove = verifier.find("`git worktree remove PATH`").unwrap();
    assert!(add < enter && enter < back && back < remove, "{verifier}");
    assert!(verifier.contains("Do not force."), "{verifier}");
    assert!(!verifier.contains("`ExitWorktree` with action `remove`"));
}

#[test]
fn connect_says_when_it_removed_the_old_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_claude(tmp.path(), 0);
    let out = connect(&bin, tmp.path(), tmp.path()).success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(stdout.starts_with("Removed the old riff MCP server entry.\n"));
}

#[test]
fn connect_fails_when_claude_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let out = connect(Path::new("false"), tmp.path(), tmp.path()).failure();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr);
    assert!(stderr.contains("false plugin marketplace add"), "{stderr}");
}

/// The real `claude` command, with its own config directory.
#[test]
fn claude_installs_the_plugin_and_drops_the_old_entry() {
    if Command::new("claude").arg("--version").output().is_err() {
        eprintln!("skip: the claude command is not installed");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config");
    let claude = |args: &[&str]| {
        let out = Command::new("claude")
            .args(args)
            .env("CLAUDE_CONFIG_DIR", &config)
            .output()
            .unwrap();
        assert!(out.status.success(), "claude {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    claude(&["mcp", "add", "--scope", "user", "riff", "--", "riff", "mcp"]);
    for _ in 0..2 {
        assert_cmd::Command::cargo_bin("riff")
            .unwrap()
            .args(["connect", "claude"])
            .env("XDG_DATA_HOME", tmp.path())
            .env("CLAUDE_CONFIG_DIR", &config)
            .current_dir(tmp.path())
            .assert()
            .success();
    }
    assert!(claude(&["plugin", "list"]).contains("riff@riff"));
    let settings = std::fs::read_to_string(config.join(".claude.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
    assert_eq!(settings["mcpServers"], serde_json::json!({}));
}
