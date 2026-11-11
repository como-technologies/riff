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

/// "Update riff" of "Start a Riff" is one command, and the start of the
/// TUI again after it (01M748V7Y8DQPCQJ9D2Q7HKB76).
#[test]
fn a_person_updates_riff_with_riff_update() {
    let update = commands_of_part(PAGE, "Update riff");
    assert_eq!(update, ["riff update", "riff"]);
    each_is_real(&update);
}

/// "Update riff" of "Start a Riff" says that a TUI that runs keeps its
/// build, with the words of the row of the TUI, and how to start it
/// again (01M748V7Y8DQPCQJ9D2Q7HKB76).
#[test]
fn update_riff_says_to_start_the_tui_again() {
    let part = part(PAGE, "Update riff");
    let how = &part[part
        .find("\n### Start the TUI again after an update\n")
        .expect("the how-to")..];
    let text = how.split_whitespace().collect::<Vec<_>>().join(" ");
    let build = riff_core::build::Build {
        version: "VERSION".into(),
        id: "ID".into(),
        time: String::new(),
    };
    for words in [
        "A TUI that runs keeps its build until it starts again.".to_owned(),
        format!("`{}`", riff::text::installed_riff(&build)),
        "Press `q` in the TUI, then start it again: ```sh riff ```".to_owned(),
    ] {
        assert!(text.contains(&words), "{words}: {text}");
    }
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
            "riff github create --owner OWNER",
            "riff cloud create shared",
            "riff cloud status shared",
            "riff github check",
            "riff github install OWNER",
            "riff invite EMAIL",
            "riff members",
            "riff tail",
            "riff owner --take",
            "riff owner EMAIL",
            "riff owner --deny",
            "riff chat",
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

/// 01M6WHG7A736K9N01TXGAQS0MZ: "Start a Team Riff" has the how-to "Make
/// the GitHub Apps of riff", and "Add another repository to the riff"
/// has the step that installs them on another account, each with a
/// copyable `sh` block of a real command. The how-to makes the secret of
/// the keys before the deploy of a release with the Apps
/// (01M6YC3SYATQW7GZZV3APFG85B).
#[test]
fn the_book_has_the_how_tos_of_the_github_apps() {
    let make = part("start-a-team-riff.md", "Make the GitHub Apps of riff");
    let commands = book::commands_in(&make);
    assert_eq!(
        commands,
        [
            "riff github create --owner OWNER",
            "git pull",
            "riff cloud create shared",
            "riff cloud status shared",
            "riff github check",
            "riff github install OWNER"
        ]
    );
    for heading in [
        "\n### Make the secret before the deploy\n",
        "\n### Check the tokens of each role\n",
        "\n### Accept the new rights of the lead App\n",
    ] {
        assert!(make.contains(heading), "{heading}: {make}");
    }
    // 01M6ZEJ4NEZGQFV3Q4BER5AW3S, 01M6ZHWVPD7MN6GH6GS0GVETHA: the table
    // has the rights of the lead on the alerts. The deploy needs no step
    // first.
    for row in [
        "| `security_events` | none | none | write |",
        "| `secret_scanning_alerts` | none | none | write |",
        "| `vulnerability_alerts` | none | none | read |",
    ] {
        assert!(make.contains(row), "{row}: {make}");
    }
    // 01M74FVDMYDR8685BA1TFAS70R: the how-to of the new rights is one
    // `sh` block of `riff github install OWNER`, and no list of steps.
    let accept = make
        .split("\n### Accept the new rights of the lead App\n")
        .nth(1)
        .unwrap();
    assert_eq!(book::commands_in(accept), ["riff github install OWNER"]);
    assert_eq!(accept.matches("```sh").count(), 1, "{accept}");
    assert!(
        !accept.lines().any(|line| line.starts_with("1. ")),
        "{accept}"
    );
    let text = make.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "The deploy needs no step of yours first:",
        "riff-server gives the lead a token with its other rights, and logs one warning.",
        "`riff github check` names each missing right with the fix.",
        "GitHub has no API call that sets them.",
        "your browser opens the page of its rights, and riff prints each right to set.",
        "your browser opens its page **Configure**.",
        "With nothing to do, it changes nothing and says so in one line.",
    ] {
        assert!(text.contains(words), "{words}: {make}");
    }
    assert!(
        !text.contains("before the deploy of a riff-server"),
        "{make}"
    );
    assert!(
        make.contains("(how-it-works.md#when-riff-server-cannot-read-the-keys-of-the-apps)"),
        "{make}"
    );
    let riff: Vec<String> = commands
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    each_is_real(&riff);
    let add = part(PAGE, HOW_TO);
    let install = add
        .split("\n### Install the GitHub Apps of riff on the account\n")
        .nth(1)
        .expect("the step is in the how-to");
    let install = install.split("\n### ").next().unwrap();
    let commands = book::commands_in(install);
    assert_eq!(commands, ["riff github install OWNER"]);
    each_is_real(&commands);
}

/// 01M74FVDMYDR8685BA1TFAS70R: no page of the book lists the steps on
/// GitHub that set the rights of an App or accept them: `riff github
/// install OWNER` opens each page.
#[test]
fn no_page_of_the_book_lists_the_steps_on_github_for_the_rights_of_an_app() {
    for entry in fs::read_dir(book::dir()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".md") || name.starts_with("requirements") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for words in [
            "**Developer settings**",
            "**Permissions & events**",
            "**Review request**",
            "**Accept new permissions**",
        ] {
            assert!(!text.contains(words), "{name} has {words}");
        }
    }
}

/// 01M74MB9C716796KFJMKRZKJ4D: "Accept the new rights of the lead App"
/// says that each lead and each session gets the new rights at once after
/// the accept, and that `riff github check` renews them for a lead App
/// with no webhook. "New rights of a GitHub App" of "How It Works" shows
/// how, with a diagram, and how to see each renew of the daemon.
#[test]
fn the_book_says_that_each_lead_and_session_gets_the_new_rights_at_once() {
    let make = part("start-a-team-riff.md", "Make the GitHub Apps of riff");
    let accept = make
        .split("\n### Accept the new rights of the lead App\n")
        .nth(1)
        .expect("the how-to is in the part");
    let accept = accept.split("\n### ").next().unwrap();
    let words = accept.split_whitespace().collect::<Vec<_>>().join(" ");
    for line in [
        "Each lead and each session gets the new rights at once after the accept.",
        "A lead App that riff made before it had a webhook sends riff-server no event. Then \
         the check at the end of `riff github install` finds the new rights",
        "(how-it-works.md#new-rights-of-a-github-app)",
    ] {
        assert!(words.contains(line), "{line}: {accept}");
    }
    let how = page("how-it-works.md");
    let renew = how
        .split("\n#### New rights of a GitHub App\n")
        .nth(1)
        .expect("the part is in How It Works");
    let renew = renew.split("\n#### ").next().unwrap();
    assert!(renew.contains("```mermaid"), "{renew}");
    assert_eq!(
        book::commands_in(renew),
        ["journalctl --user -u riff | grep 'has new rights'"]
    );
    assert!(renew.contains("riff daemon: The install 2020 of a GitHub App has new rights."));
}

/// 01M6Z7R4R56ANXBQN0XJZXNFXW: "Add another repository to the riff" has
/// the step that adds the repository to the installs of the Apps, with a
/// copyable `sh` block of each real command, and the words of GitHub.
#[test]
fn the_book_adds_a_repository_to_the_installs_of_the_apps() {
    let add = part(PAGE, HOW_TO);
    let step = add
        .split("\n### Add the repository to the installs of the Apps\n")
        .nth(1)
        .expect("the step is in the how-to");
    let step = step.split("\n### ").next().unwrap();
    let commands = book::commands_in(step);
    assert_eq!(
        commands,
        [
            "riff github install OWNER",
            "riff github install OWNER --repo OWNER/NAME"
        ]
    );
    each_is_real(&commands);
    let words = step.split_whitespace().collect::<Vec<_>>().join(" ");
    for name in [
        "**Only select repositories**",
        "**Configure**",
        "**Repository access**",
        "**Save**",
    ] {
        assert!(words.contains(name), "{name}: {step}");
    }
}

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
        "### A repository with no CI\n",
        "#### Use the checks that you have\n",
        "#### Allow auto-merge and squash merge\n",
        "#### Make the ruleset of the default branch\n",
        "#### Make the Backlog and the first wave\n",
        "### Write a CLAUDE.md, when you want one\n",
        "### Build the image of a project\n",
        "### Build the image with a workflow\n",
        "### Pin the image of the project\n",
        "### Build the image again for a new riff-base\n",
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
        "printf '[session]\\nimage = \"%s@%s\"\\n' \"$IMAGE\" \"$DIGEST\" > riff.toml",
        "riff init --yes --image \"$IMAGE@$DIGEST\"",
        "sed -i \"s|^FROM .*|FROM $BASE|\" deploy/session/Dockerfile",
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

/// 01M5SMSB0NFBD1XHEW3BSE4BDP, 01M5SMRRAG1WPBJMQKKTPSRHRP: the how-to has
/// the step "Check that each host can pull the image". It says that each
/// host of the wave must pull the session image, and has a copyable `sh`
/// block for the check and for the fix. Its `text` block is the line that
/// `riff image --check` prints, and the pin step links to it.
#[test]
fn the_how_to_says_that_each_host_must_pull_the_image_and_how_to_check_it() {
    let how_to = part(PAGE, HOW_TO);
    let step = how_to
        .split("### Check that each host can pull the image\n")
        .nth(1)
        .expect("the step");
    let step = step.split("\n### ").next().unwrap();
    assert_eq!(
        book::commands_in(step),
        [
            "riff image --check",
            "podman login ghcr.io",
            "riff daemon status"
        ]
    );
    let words = step.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(words.contains("The session image must be pullable by each host of the wave."));
    assert!(words.contains("riff manages no registry login"));
    let image: riff::session_image::Image =
        format!("ghcr.io/acme/session@sha256:{}", "ab".repeat(32))
            .parse()
            .unwrap();
    let line = riff::text::image_closed(&image, "the registry answered 403 Forbidden")
        .replace(image.digest(), "sha256:…");
    assert!(
        step.contains(&format!("```text\n{line}\n```")),
        "{line}\n{step}"
    );
    assert!(how_to.contains("(#check-that-each-host-can-pull-the-image)"));
    assert!(step.contains("(how-it-works.md#report-a-verify)"));
}

/// 01M5RN89Z749VYS1CRYJQTKAWY: the how-to has the step "Write a CLAUDE.md,
/// when you want one" with a copyable `sh` block. The block writes a
/// `CLAUDE.md` with one part for each row of the table of the step. The
/// how-to says that the file is the choice of the repository, and that the
/// CI is the gate of a change.
#[test]
fn the_how_to_leaves_claude_md_to_the_repository_and_writes_one_on_request() {
    let how_to = part(PAGE, HOW_TO);
    let step = how_to
        .split("### Write a CLAUDE.md, when you want one\n")
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
    assert!(words.contains("A `CLAUDE.md` or an `AGENTS.md` is the choice of the repository."));
    assert!(
        words.contains("A session reads the rules of the repository for agents when it has them.")
    );
    assert!(how_to.contains("(#write-a-claudemd-when-you-want-one)"));
    // The part before the steps says that riff checks no such file, and that the CI is the gate.
    let intro = how_to.split("### Read the output").next().unwrap();
    let intro = intro.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(intro.contains(
        "`riff init` checks neither: it prints no line for them, and a repository with none passes."
    ));
    assert!(intro.contains("The full check of a pull request, its CI, is the gate of a change"));
    assert!(
        !how_to.contains("| `CLAUDE.md` |"),
        "the table of steps has no row for CLAUDE.md"
    );
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
    assert_eq!(rows.len(), 6, "{rows:?}");
    // The order of the table is the order of the steps in the code.
    let order = [
        "allows auto-merge and squash merge",
        "riff.toml",
        "makes the `Backlog`",
        "makes `Wave 1`",
        "ruleset `riff`",
        "ruleset `releases`",
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
        riff::repo_setup::Step::Releases,
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
/// ruleset of the default branch, but the verify. An empty list means that
/// the repository has no CI (01M5SMNKDWBD1J6Y18ENQG87CK).
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
    assert!(step.contains(
        "An empty list\nmeans that the repository has no CI: `riff/verify` is the only check"
    ));
    assert!(riff::repo_setup::full_check_of(&[]).is_empty());
}

/// 01M5SMNKDWBD1J6Y18ENQG87CK: the how-to says what riff does with no CI,
/// and it names no job `Gate` for another repository: neither does a text
/// of `riff init`.
#[test]
fn the_how_to_says_what_riff_does_with_no_ci_and_names_no_job_gate() {
    let how_to = part(PAGE, HOW_TO);
    let step = how_to
        .split("### A repository with no CI\n")
        .nth(1)
        .expect("the step");
    let step = step.split("\n### ").next().unwrap();
    let note = riff::text::init_ci_no("acme/app", riff::pr::VERIFY_CONTEXT);
    assert!(step.contains(&note), "{note}");
    for word in ["riff needs no CI", "`riff/verify`", "at once"] {
        assert!(step.contains(word), "{word}");
    }
    assert!(!how_to.contains("Gate"), "the how-to names a job Gate");
    let texts = [
        riff::text::init_fix_ruleset("main", riff::pr::VERIFY_CONTEXT),
        riff::text::init_fix_ci("main"),
        riff::text::init_ci_none("acme/app"),
        riff::text::INIT_FIX_CI_NONE.to_owned(),
        note,
    ];
    for text in texts {
        assert!(!text.contains("Gate"), "{text}");
    }
}

/// The ruleset of the how-to is the ruleset that the step of `riff init`
/// makes (01M5QWA0E9A4ZJ8X49EM2MJM5Q), and it requires the checks that
/// riff reads: the verify alone, for a repository with no CI.
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
    assert_eq!(checks, [riff::pr::VERIFY_CONTEXT]);
}

/// 01M6WZ40YED6THKVJ2JQRXFK4N, 01M6WZ40PZNBHW2BZHMQX80WJ8: the how-to
/// "Let the lead push the release tag" makes the ruleset `releases` that
/// the step of `riff init` makes, with the App ID of the lead App from
/// `gh api apps/SLUG`. The fix of the check names the how-to.
#[test]
fn the_ruleset_releases_of_the_how_to_is_the_ruleset_of_the_step() {
    let how_to = part(PAGE, HOW_TO);
    let step = how_to
        .split("\n#### Let the lead push the release tag\n")
        .nth(1)
        .expect("the how-to");
    let step = step.split("\n#### ").next().unwrap();
    assert_eq!(
        book::commands_in(step)[..2],
        [
            "LEAD_ID=$(gh api apps/riff-acme-lead --jq .id)",
            "gh api -X POST repos/{owner}/{repo}/rulesets --input - <<JSON",
        ]
    );
    let start = step.find("<<JSON\n").expect("the ruleset") + "<<JSON\n".len();
    let json = &step[start..start + step[start..].find("\nJSON\n").expect("its end")];
    let ruleset: serde_json::Value =
        serde_json::from_str(&json.replace("$LEAD_ID", "7")).expect("the ruleset is JSON");
    assert_eq!(ruleset, riff::repo_setup::releases(7));
    assert!(
        riff::text::init_fix_releases("riff-acme-lead")
            .contains("See \"Let the lead push the release tag\" in the book")
    );
}

/// The commands of the how-to that write `riff.toml` write a file that
/// riff reads (01M4Z7SP3KSKJYKNRP1MQ70C4Q), its `sed` finds the digest of
/// `riff-base` and of `riff-rust` in the Dockerfiles of this repository,
/// and each Dockerfile of the how-to builds `FROM` one of them
/// (01M5SNT57ANM99NQS5SNB8XPWV).
/// The how-to runs on the Linux host of the riff: its `sed -i` is the
/// `sed` of GNU.
#[cfg(target_os = "linux")]
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
    let base = riff::session_image::base().to_string();
    // The `sed` of a command that reads a Dockerfile of riff from GitHub,
    // run on the Dockerfile `path` of this repository.
    let from_of = |var: &str, path: &str| {
        let read = commands
            .iter()
            .find(|c| {
                c.starts_with(&format!(
                    "{var}=$(gh api repos/como-technologies/riff/contents/{path} "
                ))
            })
            .unwrap_or_else(|| panic!("the read of {var}"));
        // `gh api ... | base64 -d | sed ... | head -n 1`: the file replaces the first two.
        let tail: Vec<&str> = read.split(" | ").skip(2).collect();
        let (sed, head) = (tail[0], tail[1].trim_end_matches(')'));
        sh(&format!("{sed} {path} | {head}"), &root, &[])
            .trim_end()
            .to_owned()
    };

    // "Add riff.toml": riff-base goes in the new file.
    let from_rust = from_of("IMAGE", "deploy/rust/Dockerfile");
    assert_eq!(from_rust, base);
    let dir = tempfile::tempdir().unwrap();
    let write = "printf '[session]\\nimage = \"%s\"\\n' \"$IMAGE\" > riff.toml";
    assert!(commands.contains(&write.to_owned()));
    sh(write, dir.path(), &[("IMAGE", &base)]);
    let image = riff::session_image::Image::of_repository(dir.path()).unwrap();
    assert_eq!(image.to_string(), base);

    // "Build the image of a project": BASE is riff-base, RUST is riff-rust,
    // and each Dockerfile of the how-to starts with the FROM of one.
    assert_eq!(from_of("BASE", "deploy/rust/Dockerfile"), base);
    let rust = from_of("RUST", "deploy/session/Dockerfile");
    assert!(rust.parse::<riff::session_image::Image>().is_ok(), "{rust}");
    assert!(rust.contains("/riff-rust@sha256:"), "{rust}");
    let block = |at: &str| {
        let after = how_to.split(at).nth(1).unwrap_or_else(|| panic!("{at}"));
        let block = after.split("```sh\n").nth(1).unwrap();
        block.split("\n```").next().unwrap().to_owned()
    };
    for (after, from) in [
        ("This one is for a project that needs Node:", &base),
        (
            "build `FROM riff-rust` and add what the project needs:",
            &rust,
        ),
        ("build `FROM riff-base` and install that\nversion.", &base),
    ] {
        let work = tempfile::tempdir().unwrap();
        sh(
            &block(after),
            work.path(),
            &[("BASE", &base), ("RUST", &rust)],
        );
        let dockerfile =
            std::fs::read_to_string(work.path().join("deploy/session/Dockerfile")).unwrap();
        assert_eq!(
            dockerfile.lines().next(),
            Some(format!("FROM {from}").as_str()),
            "{after}"
        );
        assert!(dockerfile.lines().all(|l| !l.is_empty()), "{dockerfile}");
    }

    // "Pin the image of the project": the pin riff reads, and the pin that
    // `riff init --image` takes.
    let digest = format!("sha256:{}", "ab".repeat(32));
    let pin_new = "printf '[session]\\nimage = \"%s@%s\"\\n' \"$IMAGE\" \"$DIGEST\" > riff.toml";
    assert!(commands.contains(&pin_new.to_owned()));
    let env = [
        ("IMAGE", "ghcr.io/acme/session"),
        ("DIGEST", digest.as_str()),
    ];
    sh(pin_new, dir.path(), &env);
    let image = riff::session_image::Image::of_repository(dir.path()).unwrap();
    assert_eq!(image.to_string(), format!("ghcr.io/acme/session@{digest}"));

    // "Build the image again for a new riff-base": the `sed` moves the FROM.
    let bump = "sed -i \"s|^FROM .*|FROM $BASE|\" deploy/session/Dockerfile";
    assert!(commands.contains(&bump.to_owned()));
    let work = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(work.path().join("deploy/session")).unwrap();
    let old = format!(
        "ghcr.io/como-technologies/riff-base@sha256:{}",
        "00".repeat(32)
    );
    std::fs::write(
        work.path().join("deploy/session/Dockerfile"),
        format!("FROM {old}\nRUN true\n"),
    )
    .unwrap();
    sh(bump, work.path(), &[("BASE", &base)]);
    let moved = std::fs::read_to_string(work.path().join("deploy/session/Dockerfile")).unwrap();
    assert_eq!(moved, format!("FROM {base}\nRUN true\n"));
}
