//! The lead (R175-R180, R200, R229-R231, 01M77XMS645VZNTP22DTCB7RXE) is
//! in one place in the book: "The lead" in `how-it-works.md`. The other
//! pages link to it.

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
fn the_lead_has_a_graph_of_two_people_and_one_lead_on_two_machines() {
    let lead = section("how-it-works.md", "The lead");
    let graph = lead
        .split("```mermaid\nflowchart")
        .nth(1)
        .expect("a mermaid flowchart");
    let graph = &graph[..graph.find("```").unwrap()];
    for part in [
        "subgraph pangolin",
        "subgraph thelio",
        "the lead of the project",
        "M((mike)) <-->|questions, answers| L",
        "B((brett)) <-->|questions, answers| L",
        "BA -->|questions, reports| L",
    ] {
        assert!(graph.contains(part), "the graph has no {part:?}");
    }
}

#[test]
fn the_lead_names_each_part() {
    let lead = section("how-it-works.md", "The lead");
    for part in [
        "one lead for each project",
        "(#when-the-people-disagree)",
        "### When the people disagree",
        "```sh\nriff questions\n```",
        "(#ask-the-lead)",
        "(#the-lead-conducts-your-sessions)",
        "(waves.md)",
        "### When a message of the lead counts",
        "### Name the lead",
        "```sh\nriff lead restart\n```",
        "### When you have no lead",
        "not the lead any more",
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
        "one lead for each project",
        "becomes the lead",
        "takes an answer of the lead",
        "takes an answer of your lead",
        "reader shows its sender without",
    ];
    for entry in fs::read_dir(book()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".md") || name == "requirements.md" || name == "requirements-history.md"
        {
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

/// The text of the part `### heading`, up to the next `### ` heading.
fn part(page: &str, heading: &str) -> String {
    let text = read(page);
    let start = text
        .find(&format!("\n### {heading}\n"))
        .unwrap_or_else(|| panic!("{page} has no \"### {heading}\""));
    let rest = &text[start + 1..];
    let end = rest[4..].find("\n### ").map_or(rest.len(), |i| i + 4);
    rest[..end].to_owned()
}

/// The names of the book pages, `requirements.md` and its history apart:
/// they keep the superseded decisions.
fn pages() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(book())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.ends_with(".md") && name != "requirements.md" && name != "requirements-history.md"
        })
        .collect();
    names.sort();
    names
}

/// 01M5QV9STQF1P9GN65P35E0FJH: the lead has no Remote Control. "Start
/// the lead" says `riff lead start`, and no page names the flag or the
/// command of Remote Control.
#[test]
fn the_lead_has_no_remote_control() {
    let part = part("how-it-works.md", "Start the lead");
    for words in [
        "```sh\nriff lead start\n```",
        "(development.md#start-the-lead)",
    ] {
        assert!(part.contains(words), "\"Start the lead\" has no {words:?}");
    }
    for name in pages() {
        let text = read(&name);
        for words in ["--remote-control", "/rc"] {
            assert!(!text.contains(words), "{name} names {words:?}");
        }
    }
}

/// 01M51PD7Q24N252V66NFEHY49N: "Start the lead" in `development.md`
/// shows the lines that riff prints at the end of the lead.
#[test]
fn start_the_lead_shows_the_lines_of_the_end_of_the_lead() {
    let part = part("development.md", "Start the lead");
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

/// 01M6XVBRQMBMK68K8GCDRZ5GJ5, 01M6XVBRT2T838TX53SSXR6HRH: "Let the lead
/// start the work" and "Take over a session" say how to select text in
/// the terminal of an attach, and that the terminal is whole again
/// after it.
#[test]
fn the_book_says_how_to_select_text_and_that_the_terminal_comes_back_whole() {
    for (page, heading) in [
        ("start-a-riff.md", "Let the lead start the work"),
        ("development.md", "Take over a session"),
    ] {
        // The words of the part on one line, as the wrap breaks the lines.
        let part = part(page, heading)
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for words in [
            "select text with the mouse",
            "hold shift",
            "the mouse reports",
            "whole again",
        ] {
            assert!(part.contains(words), "\"{heading}\" has no {words:?}");
        }
    }
}

/// #1793, 01M77Y6ZBV7C8Z332SAFKQCZ5A, 01M77Y6ZEQ4A6C6E0XZ56WRE89,
/// 01M77Y6ZHM0V6YQ3XJEVJ8J5PC: "Start the lead" in `development.md` says
/// that the daemon of a host that joined starts the lead, and shows what
/// `riff lead start` says on another host. "Stop the lead" in
/// `start-a-riff.md` has `riff lead restart` and `riff host join
/// --no-lead`, each in a block of its own.
#[test]
fn the_book_starts_and_stops_the_lead_with_the_commands_of_today() {
    let start = part("development.md", "Start the lead");
    let lead = "riff://mike@thelio/como-technologies/riff?session=4e54d4e5&lead=true"
        .parse()
        .unwrap();
    let there = riff::text::lead_not_here("como-technologies/riff", "pangolin", Some(&lead));
    for words in [
        "```sh\nriff host join\n```",
        "```sh\nriff lead start\n```",
        &format!("\n{there}\n"),
        "(start-a-riff.md#stop-the-lead)",
    ] {
        assert!(start.contains(words), "\"Start the lead\" has no {words:?}");
    }
    let stop = part("start-a-riff.md", "Stop the lead");
    for words in [
        "```sh\nriff lead restart\n```",
        "```sh\nriff host join --no-lead\n```",
        "```sh\nriff host join --no-lead --host thelio\n```",
        "workers, no lead",
    ] {
        assert!(stop.contains(words), "\"Stop the lead\" has no {words:?}");
    }
    for name in pages() {
        let text = read(&name);
        for words in ["riff lead stop", "riff lead --session"] {
            assert!(!text.contains(words), "{name} names {words:?}");
        }
    }
}
