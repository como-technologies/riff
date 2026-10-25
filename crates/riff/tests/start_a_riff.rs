//! "Start a Riff", the first page of the book for a person. It asks one
//! question first (01M3MN2R92DA7QPP80G1AENX4M). Its path "Just this
//! machine" has at most three commands (R4), each one real, and no
//! sign-in. No book page names the Cloud Run URL (R5). The
//! `riff-server` command of the path runs in
//! `crates/riff-server/tests/start_a_riff.rs`. "Join a Riff" is checked
//! in `join_a_riff.rs`.

use crate::book;

use std::fs;

use book::{commands_of, commands_of_part, each_is_real, page, part};

const PAGE: &str = "start-a-riff.md";

/// The commands of "Just this machine".
fn commands() -> Vec<String> {
    commands_of_part(PAGE, "Just this machine")
}

/// The text of the page before its first `##` part.
fn introduction() -> String {
    let page = page(PAGE);
    page[..page.find("\n## ").unwrap()].to_owned()
}

#[test]
fn the_introduction_says_what_a_riff_is_and_that_riff_runs_on_linux_only() {
    let introduction = introduction().replace('\n', " ");
    assert!(
        introduction.contains(
            "A riff is the place where your sessions and the sessions of your team meet."
        ),
        "{introduction}"
    );
    assert!(introduction.contains("Linux only"), "{introduction}");
}

/// The question comes before the paths. Each answer links its page.
#[test]
fn the_page_asks_one_question_first() {
    let introduction = introduction();
    let question = introduction
        .find("Did a person give you a riff address?")
        .expect("the question");
    let answers = &introduction[question..];
    for link in [
        "(join-a-riff.md)",
        "(#just-this-machine)",
        "(start-a-team-riff.md)",
    ] {
        assert!(answers.contains(link), "{link}: {answers}");
    }
}

#[test]
fn the_page_names_no_insecure_no_systemd_and_no_install_subcommand() {
    let text = page(PAGE);
    for word in ["--insecure", "systemd", "riff-server install"] {
        assert!(!text.contains(word), "{PAGE} names {word}");
    }
}

#[test]
fn a_person_starts_a_riff_with_at_most_three_commands() {
    let commands = commands();
    assert!(!commands.is_empty());
    assert!(commands.len() <= 3, "{commands:?}");
}

#[test]
fn the_install_command_installs_riff_and_its_server_from_the_repository() {
    let install = &commands()[0];
    let words: Vec<&str> = install.split_whitespace().collect();
    assert_eq!(words[..3], ["cargo", "install", "--locked"], "{install}");
    let git = words.iter().position(|w| *w == "--git").unwrap();
    assert_eq!(words[git + 1], env!("CARGO_PKG_REPOSITORY"));
    assert_eq!(words[git + 2..], [env!("CARGO_PKG_NAME"), "riff-server"]);
}

/// "Update riff" of "Start a Riff" is one command, and `riff init`
/// removes the plugin of riff 3 (01M5JFF7TAVA87TMJXK25HDJ19).
#[test]
fn a_person_updates_riff_with_riff_update() {
    let update = commands_of_part(PAGE, "Update riff");
    assert_eq!(update, ["riff update", "riff init"]);
    each_is_real(&update);
}

/// "Update riff" of "Start a Riff" says which release `riff update`
/// installs (01M3MRMAVVKJ5WS8GWCJHWH0R4): the release that GitHub marks
/// Latest only for the riff of this machine
/// (01M510MPR1S0EQNRK2195XTXG5), and the release of the shared server
/// for a shared riff.
#[test]
fn update_riff_says_that_it_installs_the_release_of_the_riff() {
    let text = part(PAGE, "Update riff")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for words in [
        "It installs the release that your riff runs.",
        "That is not always the newest release (see [Releases](how-it-works.md#releases))",
        "The riff of this machine: it installs the release that GitHub marks Latest.",
        "A shared riff: it installs the release of the shared server.",
        "After a new release, update when the shared riff runs it.",
    ] {
        assert!(text.contains(words), "{words}");
    }
    assert!(
        !text.contains("installs the newest release of riff"),
        "{text}"
    );
    assert!(!text.contains("same build"), "{text}");
}

/// "Start a Team Riff" (01M3MEFG6F102T1H8DFJ38EJ4A): the owner uses the
/// riff, signs in first, then invites each person, and sees each change
/// of the members with `riff tail` (01M3MN14ZCTRVD3T455P6TFK1B). Each
/// `riff` command is real. The server part of the page runs in
/// `crates/riff-server/tests/start_a_team_riff.rs`.
#[test]
fn the_owner_of_a_team_riff_signs_in_then_invites() {
    let commands = commands_of("start-a-team-riff.md");
    assert!(
        commands.contains(&"echo 'export RIFF_SERVER=URL' >> ~/.bashrc".to_owned()),
        "{commands:?}"
    );
    let riff: Vec<String> = commands
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    assert_eq!(
        riff,
        [
            "riff cloud create NAME --project PROJECT --region REGION",
            "riff cloud signin NAME",
            "riff cloud deploy NAME",
            "riff cloud list",
            "riff cloud status NAME",
            "riff cloud log NAME --errors",
            "riff cloud delete NAME",
            "riff cloud list",
            "riff login",
            "riff invite EMAIL",
            "riff members",
            "riff tail",
            "riff owner --take",
            "riff owner EMAIL",
            "riff owner --deny",
            "riff who",
        ]
    );
    each_is_real(&riff);
    // The example note of the page is the note that riff posts.
    let invited = riff_core::wire::Invited {
        email: "bob@gmail.com".into(),
        address: "URL".into(),
    };
    let note = riff_client::text::invited_news("ada", &invited);
    assert!(page("start-a-team-riff.md").contains(&note), "{note}");
}

#[test]
fn no_book_page_names_a_cloud_run_url() {
    for entry in fs::read_dir(book::dir()).unwrap() {
        let path = entry.unwrap().path();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains(".run.app"), "{}", path.display());
    }
}

#[test]
fn the_page_has_no_command_of_the_plugin_and_signs_in_nowhere() {
    let text = page(PAGE);
    for word in ["riff connect", "riff enable", "riff disable", "riff watch"] {
        assert!(!text.contains(word), "{PAGE} names {word}");
    }
    let riff: Vec<String> = commands()
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    each_is_real(&riff);
}

/// The how-to "Add another repository to the riff" of "Start a Riff".
const HOW_TO: &str = "Add another repository to the riff";

/// 01M5QVF5C3CYJPMQ9CXNZJ8E93: "Start a Riff" has the how-to "Add another
/// repository to the riff", with a copyable `sh` block for each step.
#[test]
fn the_page_has_the_how_to_add_another_repository_to_the_riff() {
    let how_to = part(PAGE, HOW_TO);
    let commands = book::commands_in(&how_to);
    assert!(
        commands.contains(&"riff init --check".to_owned()),
        "{commands:?}"
    );
    for step in [
        "### Add riff.toml\n",
        "#### Add the job Gate\n",
        "#### Use the checks that you have\n",
        "#### Allow auto-merge and squash merge\n",
        "#### Make the ruleset of the default branch\n",
        "#### Make the Backlog and the first wave\n",
        "### Write the CLAUDE.md of the repository\n",
        "### Pin an image with another toolchain\n",
    ] {
        assert!(how_to.contains(step), "{step}");
    }
    for command in [
        "printf '[session]\\nimage = \"%s\"\\n' \"$IMAGE\" > riff.toml",
        "git push",
        "gh api -X PATCH repos/{owner}/{repo} -F allow_auto_merge=true -F allow_squash_merge=true",
        "gh api -X POST repos/{owner}/{repo}/rulesets --input - <<'JSON'",
        "riff wave make \"Wave 1\"",
        "gh api -X POST repos/{owner}/{repo}/milestones -f title=Backlog",
        "podman push --digestfile /tmp/digest \"$IMAGE:node\"",
        "printf '[session]\\nimage = \"%s@%s\"\\n' \"$IMAGE\" \"$(cat /tmp/digest)\" > riff.toml",
    ] {
        assert!(
            commands.contains(&command.to_owned()),
            "{command}\nnot in {commands:?}"
        );
    }
    let riff: Vec<String> = commands
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    each_is_real(&riff);
}

/// 01M5R580VCRCHJM8FY7NT485R1: the how-to has the step "Write the
/// CLAUDE.md of the repository" with a copyable `sh` block. The block
/// writes a `CLAUDE.md` with one part for each row of the table of the
/// step: the rules that the start texts of riff leave to the repository.
#[test]
fn the_how_to_writes_a_claude_md_with_the_rules_that_the_start_texts_leave_out() {
    let how_to = part(PAGE, HOW_TO);
    let step = how_to
        .split("### Write the CLAUDE.md of the repository\n")
        .nth(1)
        .expect("the step");
    let step = step.split("\n### ").next().unwrap();
    let block = step.split("```sh\n").nth(1).expect("its block");
    let block = block.split("\n```").next().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = std::process::Command::new("sh")
        .args(["-c", block])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{block}: {out:?}");
    let file = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    for row in ["Work", "Issues", "Verify"] {
        assert!(
            step.contains(&format!("| {row} |")),
            "{row} is not in the table"
        );
        assert!(file.contains(&format!("\n## {row}\n")), "{row}\n{file}");
    }
    assert!(file.contains("\"Done when:\""), "{file}");
    // The step has no riff command to check.
    assert!(
        book::commands_in(step)
            .iter()
            .all(|c| !c.starts_with("riff "))
    );
    // The step says where a session gets the rules, and links it from the table.
    let words = step.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(words.contains("The rules of your repository come from your `CLAUDE.md`."));
    assert!(how_to.contains("(#write-the-claudemd-of-the-repository)"));
}

/// 01M5R1672NF56P15VD0JEX51K4: the how-to starts with `riff init` in a
/// copyable `sh` block, lists what it sets up in the order of the steps
/// (`riff::repo_setup::Step`), and names `--yes` and `--check`.
#[test]
fn the_how_to_starts_with_riff_init_and_lists_what_it_sets_up() {
    let how_to = part(PAGE, HOW_TO);
    let commands = book::commands_in(&how_to);
    assert_eq!(commands[0], "riff init");
    // Nothing but prose comes before the first block.
    let before = how_to.split("```").next().unwrap();
    assert!(!before.contains("###"), "{before}");
    let table = how_to
        .split("| Step | What `riff init` does |")
        .nth(1)
        .expect("the table");
    let table = table.split("\n\n").next().unwrap();
    let rows: Vec<&str> = table.lines().skip(2).collect();
    assert_eq!(rows.len(), 5, "{rows:?}");
    // The order of the table is the order of the steps in the code.
    let order = [
        "allows auto-merge and squash merge",
        "riff.toml",
        "makes the `Backlog`",
        "makes `Wave 1`",
        "ruleset `riff`",
    ];
    for (row, word) in rows.iter().zip(order) {
        assert!(
            row.to_lowercase().contains(&word.to_lowercase()),
            "{word}: {row}"
        );
    }
    let steps = [
        riff::repo_setup::Step::Merge,
        riff::repo_setup::Step::Config,
        riff::repo_setup::Step::Backlog,
        riff::repo_setup::Step::Wave,
        riff::repo_setup::Step::Ruleset,
    ];
    assert!(steps.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(
        table.contains(&format!("`{}`", riff::repo_setup::SETUP_BRANCH)),
        "{table}"
    );
    assert!(
        table.contains(&format!("\"{}\"", riff::repo_setup::SETUP_TITLE)),
        "{table}"
    );
    for word in [
        "`riff init --yes`",
        "`riff init --check`",
        "`CLAUDE.md`",
        "`git pull`",
    ] {
        assert!(how_to.contains(word), "{word}");
    }
}

/// 01M5QY976MTPRX5J4N1MKXAGDK: the step "Add riff.toml" says the error of
/// `riff lead start` with no `riff.toml`, and the error names the how-to.
#[test]
fn the_step_add_riff_toml_says_the_error_of_a_repository_with_no_riff_toml() {
    let how_to = part(PAGE, HOW_TO);
    let error = riff::text::image_no_config("origin/main");
    let first = error.split(',').next().unwrap();
    assert!(how_to.contains(&format!("`{first}`")), "{first}");
    assert!(error.contains(&format!("\"{HOW_TO}\"")), "{error}");
}

/// 01M5QZ01Y14Z76RC220RT7YTX7: the step "Use the checks that you have" has
/// a copyable `sh` block that lists the checks that riff waits for, as
/// `riff::repo_setup::full_check_of` does: the required checks of the
/// ruleset of the default branch, but the verify. An empty list is `Gate`.
#[test]
fn the_how_to_lists_the_checks_that_riff_waits_for() {
    let how_to = part(PAGE, HOW_TO);
    let step = how_to
        .split("#### Use the checks that you have\n")
        .nth(1)
        .expect("the step");
    let step = step.split("\n#### ").next().unwrap();
    let commands = book::commands_in(step);
    assert_eq!(commands.len(), 2, "{commands:?}");
    assert_eq!(
        commands[0],
        "BRANCH=$(gh api repos/{owner}/{repo} --jq .default_branch)"
    );
    assert!(commands[1].starts_with(r#"gh api "repos/{owner}/{repo}/rules/branches/$BRANCH""#));
    assert!(commands[1].contains(r#"select(.type == "required_status_checks")"#));
    assert!(commands[1].ends_with(&format!(r#"- ["{}"]'"#, riff::pr::VERIFY_CONTEXT)));
    assert!(step.contains("An empty list\nmeans that riff waits for a check run named `Gate`"));
    assert_eq!(riff::repo_setup::full_check_of(&[]), [riff::pr::GATE]);
}

/// The ruleset of the how-to is the ruleset that the step of `riff init`
/// makes (01M5QWA0E9A4ZJ8X49EM2MJM5Q), and it requires the checks that
/// riff reads: the verify and the `Gate` that the wave waits for.
#[test]
fn the_ruleset_of_the_how_to_is_the_ruleset_of_the_step() {
    let how_to = part(PAGE, HOW_TO);
    let start = how_to.find("<<'JSON'\n").expect("the ruleset") + "<<'JSON'\n".len();
    let json = &how_to[start..start + how_to[start..].find("\nJSON\n").expect("its end")];
    let ruleset: serde_json::Value = serde_json::from_str(json).expect("the ruleset is JSON");
    let step: serde_json::Value = serde_json::from_str(riff::repo_setup::RULESET).unwrap();
    assert_eq!(ruleset, step);
    let checks: Vec<&str> = ruleset["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["type"] == "required_status_checks")
        .flat_map(|r| {
            r["parameters"]["required_status_checks"]
                .as_array()
                .unwrap()
        })
        .map(|c| c["context"].as_str().unwrap())
        .collect();
    assert_eq!(checks, [riff::pr::GATE, riff::pr::VERIFY_CONTEXT]);
}

/// The commands of the how-to that write `riff.toml` write a file that
/// riff reads (01M4Z7SP3KSKJYKNRP1MQ70C4Q), and its `sed` finds the pin of
/// the `riff.toml` of this repository.
#[test]
fn the_riff_toml_of_the_how_to_is_a_file_that_riff_reads() {
    let how_to = part(PAGE, HOW_TO);
    let commands = book::commands_in(&how_to);
    let sh = |line: &str, dir: &std::path::Path, env: &[(&str, &str)]| {
        let out = std::process::Command::new("sh")
            .args(["-c", line])
            .current_dir(dir)
            .envs(env.iter().copied())
            .output()
            .unwrap();
        assert!(out.status.success(), "{line}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pin = riff::session_image::Image::of_repository(&root)
        .unwrap()
        .to_string();

    // "Add riff.toml": the pin of riff goes in the new file.
    let read = commands
        .iter()
        .find(|c| c.starts_with("IMAGE=$(gh api repos/como-technologies/riff/contents/riff.toml"))
        .expect("the read of the image of riff");
    let sed = read.split(" | ").last().unwrap().trim_end_matches(')');
    assert_eq!(sh(&format!("{sed} riff.toml"), &root, &[]).trim_end(), pin);
    let dir = tempfile::tempdir().unwrap();
    let write = "printf '[session]\\nimage = \"%s\"\\n' \"$IMAGE\" > riff.toml";
    assert!(commands.contains(&write.to_owned()));
    sh(write, dir.path(), &[("IMAGE", &pin)]);
    let image = riff::session_image::Image::of_repository(dir.path()).unwrap();
    assert_eq!(image.to_string(), pin);

    // "Pin an image with another toolchain": the base is that pin, and the
    // digest of `podman push` makes a pin that riff reads.
    let base = commands
        .iter()
        .find(|c| c.starts_with("BASE=$(sed "))
        .expect("the base");
    assert_eq!(
        sh(&format!("{base}; printf %s \"$BASE\""), dir.path(), &[]),
        pin
    );
    assert!(commands.contains(&"FROM $BASE".to_owned()) || how_to.contains("\nFROM $BASE\n"));
    let digest = format!("sha256:{}", "ab".repeat(32));
    std::fs::write(dir.path().join("digest"), &digest).unwrap();
    let pin_new =
        "printf '[session]\\nimage = \"%s@%s\"\\n' \"$IMAGE\" \"$(cat /tmp/digest)\" > riff.toml"
            .replace(
                "/tmp/digest",
                &dir.path().join("digest").display().to_string(),
            );
    sh(&pin_new, dir.path(), &[("IMAGE", "ghcr.io/acme/session")]);
    let image = riff::session_image::Image::of_repository(dir.path()).unwrap();
    assert_eq!(image.to_string(), format!("ghcr.io/acme/session@{digest}"));
}
