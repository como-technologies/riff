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
        "### Name the lead",
        "```sh\nriff lead --session 77e0a1b2\n```",
        "### When you have no lead",
        "not the lead any more",
        "claude --remote-control",
    ] {
        assert!(lead.contains(part), "\"The lead\" has no {part:?}");
    }
}

/// 01M3JD5QCVCB3VEK4KP955JSEA: the skill and the book say it.
#[test]
fn the_lead_takes_no_claims_not_even_a_verify() {
    let lead = section("how-it-works.md", "The lead");
    assert!(lead.contains("It takes no claims: no work item and no verify."));
}

#[test]
fn start_a_riff_links_the_lead() {
    let page = read("start-a-riff.md");
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
/// the how-to for the release by the lead. The skill tells the lead.
#[test]
fn the_book_says_how_the_lead_frees_a_claim() {
    let page = read("how-it-works.md");
    let thread = "como-technologies/riff".parse().unwrap();
    for part in [
        "### Free the claim of another session",
        "```sh\nriff release issue-12 --session 068a2cc2\n```",
        &riff::text::released_for(&thread, "issue-12", "068a2cc2"),
    ] {
        assert!(page.contains(part), "how-it-works.md has no {part:?}");
    }
}

/// 01M3W8AYDFPZNZ898WAJS7JEZA: the skill and the book name the
/// automatic step of the lead.
#[test]
fn the_book_names_the_automatic_step_of_the_lead() {
    let book = section("how-it-works.md", "A status");
    for part in [
        "### See what the lead does",
        "```sh\nriff who\n```",
        "told 075ff6a7",
        "the step of a `tell` shows\nonly the session",
        "A step does not end a block of the\nlead",
        "posted a note: Waves: new item #314",
        "asked for status",
        "paused the riff",
        "resumed the riff",
    ] {
        assert!(book.contains(part), "\"A status\" has no {part:?}");
    }
    // The book shows the words that the tools give.
    use riff_core::wire::{Kind, RiffState};
    for step in [
        riff::text::told_step("075ff6a7-aaaa"),
        riff::text::posted_step(Kind::Note, "Waves: new item #314"),
        riff::text::posted_step(Kind::Status, ""),
        riff::text::riff_step(true, RiffState::Paused).to_owned(),
        riff::text::riff_step(false, RiffState::Paused).to_owned(),
        riff::text::riff_step(false, RiffState::Running).to_owned(),
        riff::text::riff_step(true, RiffState::Running).to_owned(),
    ] {
        assert!(book.contains(&step), "\"A status\" has no {step:?}");
    }
    // 01M3WKCYM623M66ATHCH3QGMKP: no text of a direct message.
    assert!(
        !book.contains("request: claim issue-302"),
        "\"A status\" shows the text of a tell in a step"
    );
}

/// 01M51PD7Q24N252V66NFEHY49N: "Start the lead" in `development.md`
/// shows the lines that riff prints at the end of the lead.
#[test]
fn start_the_lead_shows_the_lines_of_the_end_of_the_lead() {
    let page = read("development.md");
    let start = page
        .find("\n### Start the lead\n")
        .expect("development.md has \"### Start the lead\"");
    let rest = &page[start + 1..];
    let part = rest[4..]
        .find("\n### ")
        .map_or(rest, |end| &rest[..end + 4]);
    for line in [
        riff::text::lead_ended("riff-lead-OWNER-REPO"),
        riff::text::LEAD_NO_RESUME.to_owned(),
    ] {
        assert!(
            part.contains(&format!("\n{line}\n")),
            "\"Start the lead\" has no line {line:?}"
        );
    }
}
