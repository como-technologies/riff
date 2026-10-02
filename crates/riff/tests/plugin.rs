//! The written plugin passes the Claude Code validator, and `riff connect
//! claude` installs it.

use isolated::Isolated;
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

/// A riff that does not answer, so `riff connect claude` checks no
/// real riff and opens no browser. It warns.
const NO_SERVER: &str = "http://127.0.0.1:1";

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

/// `riff connect claude` with the fake `bin`. The Claude Code settings
/// are in `data/home/.claude`, never in the real home.
fn connect(bin: &Path, data: &Path, cwd: &Path) -> assert_cmd::assert::Assert {
    Isolated::shared()
        .assert_riff()
        .args(["connect", "claude", "--claude"])
        .arg(bin)
        .env("RIFF_SERVER", NO_SERVER)
        .env("XDG_DATA_HOME", data)
        .env("HOME", data.join("home"))
        .env_remove("CLAUDE_CONFIG_DIR")
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
            "Added the riff plugin from {} to Claude Code.\n\
             Added the riff status line to {}.\n\
             riff is installed but off. To turn it on in a repository: cd REPO && riff enable\n",
            market.display(),
            tmp.path().join("home/.claude/settings.json").display()
        ));
    assert!(market.join("riff/.mcp.json").is_file());
    // It installs the plugin in no scope: `riff enable` turns it on.
    let log = std::fs::read_to_string(tmp.path().join("log")).unwrap();
    assert_eq!(
        log,
        format!(
            "mcp remove --scope user riff\n\
             plugin marketplace add {}\n",
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
    let verify = pos("another session to verify the work");
    let merge = pos("On a pass, the forge merges the pull request");
    let release = pos("done, then call `release`.");
    assert!(start < verify && verify < merge && merge < release);

    // The author and the verifier each have their steps.
    let section = &skill[pos("## Verify finished work")..pos("## Remove a stale worktree")];
    let ask = section.find("### Ask for a verify").unwrap();
    let check = section
        .find("### Verify the work of another session")
        .unwrap();
    assert!(ask < check);
    let flat = section.split_whitespace().collect::<Vec<_>>().join(" ");
    for text in [
        "A session never verifies its own work.",
        "`[{\"user\": \"USER\", \"repo\": \"OWNER/REPO\", \"lead\": true}]`",
        "No session merges and no session pushes to the default branch.",
        "Open a pull request for the branch with one command:",
        "turns on auto-merge with a squash at once, before any other push.",
        "sets the verify status of that commit: success on a pass, failure on a fail.",
        "`verify-issue-12`",
        "Do not change the code.",
        "`[{\"claim\": \"issue-12\"}]`",
    ] {
        assert!(flat.contains(text), "no {text:?} in {section}");
    }

    // The verify worktree has a name of its own
    // (01M3K0FZ7X1NCPHXFN6WA4T3ES): the tools make it and remove it,
    // with no `git worktree add` by hand and no `cd`.
    let verifier = &section[check..];
    let flat = verifier.split_whitespace().collect::<Vec<_>>().join(" ");
    let at = |text: &str| {
        flat.find(text)
            .unwrap_or_else(|| panic!("no {text:?} in {verifier}"))
    };
    let enter = at("Call `EnterWorktree` with a name");
    let name = at("`verify-issue-12-a6cf`");
    let checkout = at("`git checkout --detach COMMIT` there. Do not `cd`.");
    let release = at("Call `release` with `verify-issue-12`.");
    let remove = at("call `ExitWorktree` with action `remove` and `discard_changes` set to true.");
    assert!(enter < name && name < checkout && checkout < release && release < remove);
    assert!(!verifier.contains("worktree add"), "{verifier}");
    assert!(!verifier.contains("`keep`"), "{verifier}");
}

/// The skill that `riff connect` writes keeps a live security fault out
/// of the public text on the forge and out of a post
/// (01M3W62QG36F9RD4SZ1X508T3A). The fault goes to the lead with `tell`.
#[test]
fn connect_writes_the_skill_with_no_live_security_fault_in_public_text() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_claude(tmp.path(), 1);
    connect(&bin, tmp.path(), tmp.path()).success();
    let skill = tmp
        .path()
        .join("riff/claude-plugin/riff/skills/riff/SKILL.md");
    let skill = std::fs::read_to_string(skill).unwrap();
    let section = |from: &str, to: &str| {
        let start = skill.find(from).unwrap_or_else(|| panic!("no {from:?}"));
        let end = skill[start..]
            .find(to)
            .unwrap_or_else(|| panic!("no {to:?}"));
        skill[start..start + end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let ask = section(
        "### Ask for a verify",
        "### Verify the work of another session",
    );
    assert!(
        ask.contains(
            "Your public text on the forge and your posts to a thread hold no live security fault"
        ),
        "{ask}"
    );
    let list = &ask[ask.find("The public text is").unwrap()..];
    for text in [
        "the body of a pull request",
        "a comment on a pull request",
        "an issue",
        "a comment on an issue",
        "a commit message",
    ] {
        assert!(list.contains(text), "no {text:?} in {list}");
    }
    let check = section(
        "### Verify the work of another session",
        "### Pull requests on GitHub",
    );
    let only = check.find("The result holds only the check against the `Done when:` line.");
    let report = check.find("Report it with one command");
    assert!(only.is_some() && only < report, "{check}");
    assert!(check.contains("It holds no live security fault"), "{check}");
    for part in [&ask, &check] {
        assert!(
            part.contains("`tell` the lead the fault. The lead decides on a private advisory."),
            "{part}"
        );
    }
}

/// The skill names one `riff` command for each step of a pull request
/// (01M3NB6G132QG4TAEJ5QPRJNAE), and no `gh` recipe for those steps.
#[test]
fn connect_writes_the_skill_with_the_pull_request_commands() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_claude(tmp.path(), 1);
    connect(&bin, tmp.path(), tmp.path()).success();
    let skill = tmp
        .path()
        .join("riff/claude-plugin/riff/skills/riff/SKILL.md");
    let skill = std::fs::read_to_string(skill).unwrap();
    let section = |from: &str, to: &str| {
        let start = skill.find(from).unwrap_or_else(|| panic!("no {from:?}"));
        let end = skill[start..]
            .find(to)
            .unwrap_or_else(|| panic!("no {to:?}"));
        skill[start..start + end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let ask = section(
        "### Ask for a verify",
        "### Verify the work of another session",
    );
    let open = ask.find("`riff pr open --title \"TITLE\" --file summary.md`");
    let request = ask.find("Post a verify request");
    let wait = ask.find("`riff pr wait 40`, with `run_in_background` true.");
    assert!(open.is_some() && open < request && request < wait, "{ask}");

    let check = section(
        "### Verify the work of another session",
        "### Pull requests on GitHub",
    );
    for text in [
        "`riff verify pass 40 --file result.md`",
        "`riff verify fail 40 --file result.md`",
    ] {
        assert!(check.contains(text), "no {text:?} in {check}");
    }

    let github = section("### Pull requests on GitHub", "## Remove a stale worktree");
    for text in [
        "| `riff pr open --title \"TITLE\" --file summary.md` |",
        "| `riff pr wait 40` |",
        "| `riff verify pass 40 --file result.md` |",
        "| `riff verify fail 40 --file result.md` |",
        "Do not write a shell loop around `gh`.",
    ] {
        assert!(github.contains(text), "no {text:?} in {github}");
    }
    for recipe in [
        "gh pr create",
        "gh pr merge 40",
        "gh pr comment",
        "/statuses/",
    ] {
        assert!(!skill.contains(recipe), "the skill still has {recipe:?}");
    }
    // Each command in the skill is a real command.
    for args in [&["pr", "open"][..], &["pr", "wait"], &["verify"]] {
        Isolated::shared()
            .assert_riff()
            .args(args)
            .arg("--help")
            .assert()
            .success();
    }
}

#[test]
fn connect_writes_the_skill_with_the_waves() {
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

    // Start step 2 takes an item of the current wave, before the claim.
    let step = pos("2. Find a free work item: an open issue of the current wave");
    let claim = pos("3. Call `claim` with the item");
    assert!(step < claim);

    // The concept comes first, then the work of the lead, then the forge.
    let waves = pos("## Waves\n");
    let lead = pos("### Plan the waves");
    let forge = pos("### Waves on GitHub");
    let next = pos("## Write acceptance criteria");
    assert!(claim < waves && waves < lead && lead < forge && forge < next);
    for (i, text) in [
        (
            waves,
            "The current wave is the open wave with the lowest number.",
        ),
        (lead, "Do these steps only when you are the lead."),
        (forge, "A wave is a milestone named `Wave N`."),
    ] {
        assert!(skill[i..].contains(text), "no {text:?} in {skill}");
    }
}

/// A fresh base, a rebase before each push, and a clean-up after the
/// merge, each with copyable commands (01M3MNP39172Y463WGQAW125KW,
/// 01M3MNP3B8YJ699432D4PSFWDB).
#[test]
fn connect_writes_the_skill_with_git_hygiene() {
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

    // The start routine and the verify steps point at the section.
    let step = pos("5. Make the worktree from a fresh base.");
    let enter = pos("Call the `EnterWorktree` tool with the item as the name");
    assert!(step < enter);
    pos("Rebase it on a fresh default branch");
    pos("11. After the merge, remove your worktree and its branch.");

    let section = pos("## Keep good git hygiene");
    let fresh = pos("### Start from a fresh base");
    let rebase = pos("### Rebase before each push");
    let clean = pos("### Clean up after a merge");
    let threads = pos("## Threads");
    assert!(section < fresh && fresh < rebase && rebase < clean && clean < threads);
    for (from, to, blocks) in [
        (
            fresh,
            rebase,
            &[
                "```sh\ngit fetch -q --prune origin\n```",
                "```sh\ngit reset -q --hard origin/main\n```",
            ][..],
        ),
        (
            rebase,
            clean,
            &[
                "```sh\ngit fetch -q origin\ngit rebase origin/main\ngit diff --stat origin/main...HEAD\n```",
            ][..],
        ),
        (
            clean,
            threads,
            &[
                "```sh\ngit -C MAIN fetch -q --prune origin\ngit -C MAIN worktree prune\n\
                 git -C MAIN worktree list | grep issue-12\ngit -C MAIN branch --list '*issue-12*'\n```",
                "```sh\ngit worktree list\ngit branch --list 'worktree-*'\n```",
            ][..],
        ),
    ] {
        for block in blocks {
            assert!(
                skill[from..to].contains(block),
                "no {block:?} in {}",
                &skill[from..to]
            );
        }
    }
}

#[test]
fn connect_says_when_it_removed_the_old_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_claude(tmp.path(), 0);
    let out = connect(&bin, tmp.path(), tmp.path()).success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(stdout.starts_with("Removed the old riff MCP server entry.\n"));
}

/// 01M3JFFJEW8BSRBZ9JQPKT0S8Z
#[test]
fn connect_adds_the_statusline_when_none_is_set() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_claude(tmp.path(), 1);
    let settings = tmp.path().join("home/.claude/settings.json");
    let out = connect(&bin, tmp.path(), tmp.path()).success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(
        stdout.contains(&format!(
            "Added the riff status line to {}.",
            settings.display()
        )),
        "{stdout}"
    );
    let text = std::fs::read_to_string(&settings).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["statusLine"]["command"], "riff statusline");
    assert_eq!(value["statusLine"]["type"], "command");

    // A second run changes nothing and says nothing of it.
    let out = connect(&bin, tmp.path(), tmp.path()).success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(!stdout.contains("status line"), "{stdout}");
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), text);
}

#[test]
fn connect_keeps_the_other_keys_in_their_order() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_claude(tmp.path(), 1);
    let settings = tmp.path().join("home/.claude/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let old = "{\n  \"permissions\": { \"allow\": [\"Bash(ls)\"] },\n  \"model\": \"opus\"\n}\n";
    std::fs::write(&settings, old).unwrap();
    connect(&bin, tmp.path(), tmp.path()).success();
    let text = std::fs::read_to_string(&settings).unwrap();
    assert!(
        text.starts_with("{\n  \"permissions\": { \"allow\": [\"Bash(ls)\"] },\n  \"model\": \"opus\",\n  \"statusLine\""),
        "{text}"
    );
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["model"], "opus");
    assert_eq!(value["statusLine"]["command"], "riff statusline");
}

#[test]
fn connect_leaves_another_statusline() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_claude(tmp.path(), 1);
    let settings = tmp.path().join("home/.claude/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let old = "{\"statusLine\": {\"type\": \"command\", \"command\": \"mine\"}}";
    std::fs::write(&settings, old).unwrap();
    let out = connect(&bin, tmp.path(), tmp.path()).success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(stdout.contains("has another status line"), "{stdout}");
    // It prints the command to add riff to that status line.
    assert!(stdout.contains("calls `riff statusline`"), "{stdout}");
    assert!(
        stdout.contains("\"Find the pane of a session\""),
        "{stdout}"
    );
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), old);
}

#[test]
fn connect_leaves_settings_that_are_not_json() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_claude(tmp.path(), 1);
    let settings = tmp.path().join("home/.claude/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, "{ not json").unwrap();
    let out = connect(&bin, tmp.path(), tmp.path()).success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(
        stdout.contains("riff did not set the status line"),
        "{stdout}"
    );
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{ not json");
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
        Isolated::shared()
            .assert_riff()
            .args(["connect", "claude"])
            .env("RIFF_SERVER", NO_SERVER)
            .env("XDG_DATA_HOME", tmp.path())
            .env("CLAUDE_CONFIG_DIR", &config)
            .current_dir(tmp.path())
            .assert()
            .success();
    }
    assert!(claude(&["plugin", "marketplace", "list"]).contains("riff"));
    // riff is on nowhere: the user settings have no entry of the plugin.
    let user = std::fs::read_to_string(config.join("settings.json")).unwrap();
    assert_eq!(riff::enable::entry(&user), None, "{user}");
    let settings = std::fs::read_to_string(config.join(".claude.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
    assert_eq!(settings["mcpServers"], serde_json::json!({}));
}
