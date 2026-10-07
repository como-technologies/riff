//! The written plugin passes the Claude Code validator, and holds the
//! skill.

use isolated::Isolated;
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
    for path in [dir.path().join(riff::plugin::NAME)] {
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

#[test]
fn the_plugin_has_the_skill_with_the_criteria_check() {
    let tmp = tempfile::tempdir().unwrap();
    riff::plugin::write(&tmp.path().join("riff/claude-plugin")).unwrap();
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
fn the_plugin_has_the_skill_with_the_verify_flow() {
    let tmp = tempfile::tempdir().unwrap();
    riff::plugin::write(&tmp.path().join("riff/claude-plugin")).unwrap();
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

    // The work of a worker ends at the verify request
    // (01M3Z9N6AK6W9KCA1MN72X78B6), and the verifier does the steps
    // after the merge (01M3Z9N6GQK0NYCMGQ66FW406V). A session that is
    // not a worker keeps its claim (01M3Z9N6NFNMW62JZ796RV4643).
    let author = section[ask..check]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let in_author = |text: &str| {
        author
            .find(text)
            .unwrap_or_else(|| panic!("no {text:?} in {author}"))
    };
    let request = in_author("Post a verify request to your repository thread.");
    let ends = in_author("In a worker (`RIFF_WORKER=1`), your work on the item ends here.");
    let state = in_author(
        "Write the state on the issue as a comment: the pull request, the commit, what is left \
         after the merge (for example a check after the release), and what a session must know \
         when the verify fails.",
    );
    let free = in_author("Call `release` with the item.");
    let clear = in_author("End your turn. riff clears your context");
    let person = in_author("A session that is not a worker keeps its claim and waits");
    assert!(request < ends && ends < state && state < free && free < clear && clear < person);
    for text in [
        "One context holds one item.",
        "do not start a second item in this context",
        "On a fail, the item is free with its branch",
    ] {
        in_author(text);
    }
    assert!(!author.contains("Set your status to blocked"), "{author}");
    let pass = at("When no session holds it, the author was a worker and released the item.");
    let wait =
        at("`riff pr wait 40`, with `run_in_background` true. Keep your claim while you wait.");
    let done = at("Post a note that the item is done");
    let item = at("remove the worktree and the branch of the item, when your machine has them");
    assert!(checkout < pass && pass < wait && wait < done && done < release && remove < item);
    for text in [
        "When no session holds the item, the result also wakes your lead.",
        "On a fail with no holder, the item `issue-12` is free with its branch.",
        "sees the failed verify in the result of `claim`",
    ] {
        at(text);
    }
    assert!(!verifier.contains("worktree add"), "{verifier}");
    assert!(!verifier.contains("`keep`"), "{verifier}");
}

/// The skill that riff writes keeps a live security fault out
/// of the public text on the forge and out of a post
/// (01M3W62QG36F9RD4SZ1X508T3A). The fault goes to the lead with `tell`.
#[test]
fn the_plugin_has_the_skill_with_no_live_security_fault_in_public_text() {
    let tmp = tempfile::tempdir().unwrap();
    riff::plugin::write(&tmp.path().join("riff/claude-plugin")).unwrap();
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
fn the_plugin_has_the_skill_with_the_pull_request_commands() {
    let tmp = tempfile::tempdir().unwrap();
    riff::plugin::write(&tmp.path().join("riff/claude-plugin")).unwrap();
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
fn the_plugin_has_the_skill_with_the_waves() {
    let tmp = tempfile::tempdir().unwrap();
    riff::plugin::write(&tmp.path().join("riff/claude-plugin")).unwrap();
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
fn the_plugin_has_the_skill_with_git_hygiene() {
    let tmp = tempfile::tempdir().unwrap();
    riff::plugin::write(&tmp.path().join("riff/claude-plugin")).unwrap();
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
