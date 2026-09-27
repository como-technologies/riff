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
