//! The lead (R175-R180, R200, R228-R232) is in one place in the book:
//! "The lead" in `how-it-works.md`. The other pages link to it.

use std::fs;
use std::path::{Path, PathBuf};

fn book() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src")
}

fn read(page: &str) -> String {
    fs::read_to_string(book().join(page)).unwrap_or_else(|e| panic!("{page}: {e}"))
}

/// The text of the section `## heading`, up to the next `## ` heading.
fn section(page: &str, heading: &str) -> String {
    let text = read(page);
    let start = text
        .find(&format!("\n## {heading}\n"))
        .unwrap_or_else(|| panic!("{page} has no \"## {heading}\""));
    let rest = &text[start + 1..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |i| i + 3);
    rest[..end].to_owned()
}

#[test]
fn the_lead_has_a_graph_of_two_people_on_two_machines() {
    let lead = section("how-it-works.md", "The lead");
    let graph = lead
        .split("```mermaid\nflowchart")
        .nth(1)
        .expect("a mermaid flowchart");
    let graph = &graph[..graph.find("```").unwrap()];
    for part in [
        "subgraph pangolin",
        "subgraph thelio",
        "mike: lead",
        "brett: lead",
    ] {
        assert!(graph.contains(part), "the graph has no {part:?}");
    }
}

#[test]
fn the_lead_names_each_part() {
    let lead = section("how-it-works.md", "The lead");
    for part in [
        "at most one lead in each repository",
        "(#ask-the-lead)",
        "(#the-lead-conducts-your-sessions)",
        "(waves.md)",
        "### When a message of the lead counts",
        "### Make a session the lead",
        "riff lead",
        "not the lead any more",
        "claude --remote-control",
    ] {
        assert!(lead.contains(part), "\"The lead\" has no {part:?}");
    }
}

/// 01M3JD5QCVCB3VEK4KP955JSEA: the skill and the book say it.
#[test]
fn the_lead_takes_no_claims_not_even_a_verify() {
    let skill = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("claude-plugin/riff/skills/riff/SKILL.md"),
    )
    .unwrap();
    assert!(skill.contains("As the lead, take no claims: no work item and no verify."));
    let lead = section("how-it-works.md", "The lead");
    assert!(lead.contains("It takes no claims: no work item and no verify."));
}

#[test]
fn start_a_riff_says_which_session_is_your_lead() {
    let page = read("start-a-riff.md");
    assert!(page.contains("The first session that you start in a project is your lead."));
    assert!(page.contains("(how-it-works.md#the-lead)"));
}

#[test]
fn no_other_page_repeats_the_rules_of_the_lead() {
    let rules = [
        "at most one lead",
        "becomes the lead",
        "takes an answer of the lead",
        "takes an answer of your lead",
        "reader shows its sender without",
    ];
    for entry in fs::read_dir(book()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".md") || name == "requirements.md" {
            continue;
        }
        let mut text = read(&name);
        if name == "how-it-works.md" {
            let lead = section(&name, "The lead");
            text = text.replace(&lead, "");
        }
        for rule in rules {
            assert!(!text.contains(rule), "{name} repeats {rule:?}");
        }
    }
}

/// 01M3WG243BW7P6E1ME0DFNQF8C, 01M3WG2460P4GF7GEVBY92Q33W: the book has
/// the how-to for the release by the lead, and says what riff does for
/// a worker that dies. The skill tells the lead. The book shows the
/// words that riff gives.
#[test]
fn the_book_and_the_skill_say_how_the_lead_frees_a_claim() {
    use riff::terminal::WorkerPane;
    let page = read("how-it-works.md");
    let thread = "como-technologies/riff".parse().unwrap();
    for part in [
        "### Free the claim of another session",
        "```sh\nriff release issue-12 --session 068a2cc2\n```",
        &riff::text::released_for(&thread, "issue-12", "068a2cc2"),
        "### A worker that dies",
        "```sh\njournalctl -u systemd-oomd --since \"-10min\"\n```",
    ] {
        assert!(page.contains(part), "how-it-works.md has no {part:?}");
    }
    let pane = WorkerPane {
        pane: "%5".into(),
        session: "6072f384-d57d-463c-a837-6df28bc9bc8a".into(),
    };
    let cause = "systemd-oomd killed the pane: memory pressure for \
                 /user.slice/user-1000.slice/user@1000.service being 66.21% > 50.00% for > 20s \
                 with reclaim activity";
    let note = riff::text::worker_gone("pangolin", &pane, &["issue-12".into()], Some(cause));
    let flat = page.replace('\n', " ");
    assert!(flat.contains(&note), "how-it-works.md has no {note:?}");

    let skill = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("claude-plugin/riff/skills/riff/SKILL.md"),
    )
    .unwrap();
    for part in [
        "`riff release ITEM --session ID`",
        "The pane ended with no end call",
        "Only the lead frees the\n  claim of another session",
    ] {
        assert!(skill.contains(part), "the skill has no {part:?}");
    }
}

/// 01M3W8AYDFPZNZ898WAJS7JEZA: the skill and the book name the
/// automatic step of the lead.
#[test]
fn the_skill_and_the_book_name_the_automatic_step_of_the_lead() {
    let skill = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("claude-plugin/riff/skills/riff/SKILL.md"),
    )
    .unwrap();
    let start = skill
        .find("\n## Status\n")
        .expect("the skill has \"## Status\"");
    let status = &skill[start..];
    let status = &status[..status[3..].find("\n## ").map_or(status.len(), |i| i + 3)];
    for part in [
        "riff sets your step by itself",
        "`tell`, `post`, `pause`, `resume` and `lead`",
        "told 075ff6a7",
        "posted a note: Waves: new item #314",
        "The\nstep shows no text of a direct message",
        "it keeps your `blocked`\nreason",
        "Set your status for\nwork that riff cannot see",
    ] {
        assert!(
            status.contains(part),
            "\"Status\" of the skill has no {part:?}"
        );
    }
    let book = section("how-it-works.md", "A status");
    for part in [
        "### See what the lead does",
        "```sh\nriff who\n```",
        "told 075ff6a7",
        "the step of a `tell` shows\nonly the session",
        "A `blocked` reason of the lead stays",
        "posted a note: Waves: new item #314",
        "asked for status",
        "paused the riff",
        "resumed the riff",
        "became the lead",
    ] {
        assert!(book.contains(part), "\"A status\" has no {part:?}");
    }
    // The book shows the words that the tools give.
    use riff_core::wire::{Kind, RiffState};
    for step in [
        riff::text::told_step("075ff6a7-aaaa"),
        riff::text::posted_step(Kind::Note, "Waves: new item #314"),
        riff::text::posted_step(Kind::Status, ""),
        riff::text::riff_step(RiffState::Paused).to_owned(),
        riff::text::riff_step(RiffState::Running).to_owned(),
        riff::text::LEAD_STEP.to_owned(),
    ] {
        assert!(book.contains(&step), "\"A status\" has no {step:?}");
    }
    // 01M3WKCYM623M66ATHCH3QGMKP: no text of a direct message.
    for (name, text) in [("the skill", status), ("\"A status\"", book.as_str())] {
        assert!(
            !text.contains("request: claim issue-302"),
            "{name} shows the text of a tell in a step"
        );
    }
}
