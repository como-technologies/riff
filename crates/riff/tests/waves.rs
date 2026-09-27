//! Waves (R213-R225): the skill, the requirements and the book page
//! "Waves" agree. Only their parts "on GitHub" name the objects of the
//! forge that hold a wave (R222). `riff/CLAUDE.md` has no wave rule:
//! the skill teaches it.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> String {
    fs::read_to_string(repo().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn skill() -> String {
    read("crates/riff/claude-plugin/riff/skills/riff/SKILL.md")
}

/// The level of a Markdown heading line, for example 2 for `## Waves`.
fn level(line: &str) -> Option<usize> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    (hashes > 0 && line[hashes..].starts_with(' ')).then_some(hashes)
}

/// `text` without its forge parts: each section whose heading ends with
/// "on GitHub", with its subsections.
fn without_forge_parts(text: &str) -> String {
    let mut out = String::new();
    let mut skip: Option<usize> = None;
    let mut fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
        if let Some(n) = level(line).filter(|_| !fence) {
            if skip.is_some_and(|s| n <= s) {
                skip = None;
            }
            if skip.is_none() && line.ends_with(" on GitHub") {
                skip = Some(n);
            }
        }
        if skip.is_none() {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// `text` with each run of white space as one space.
fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The book pages, with their names.
fn book() -> Vec<(String, String)> {
    let mut pages: Vec<_> = fs::read_dir(repo().join("docs/src"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .map(|p| (p.display().to_string(), fs::read_to_string(&p).unwrap()))
        .collect();
    pages.sort();
    pages
}

#[test]
fn a_forge_part_ends_at_the_next_heading_of_its_level() {
    let text = "# A\nx\n## B on GitHub\nmilestone\n```sh\n# a comment\n```\n\
                ### C\nmilestone\n## D\ny\n";
    assert_eq!(without_forge_parts(text), "# A\nx\n## D\ny\n");
}

#[test]
fn the_book_has_the_page_waves_with_a_diagram() {
    let summary = read("docs/src/SUMMARY.md");
    assert!(summary.contains("- [Waves](waves.md)"), "{summary}");
    let page = read("docs/src/waves.md");
    assert!(page.starts_with("# Waves\n"));
    assert!(page.contains("## The life of a wave\n"));
    let life = &page[page.find("## The life of a wave").unwrap()..];
    assert!(life.contains("```mermaid\n"), "{life}");
    assert!(page.contains("## Waves on GitHub\n"));
}

#[test]
fn only_the_forge_parts_name_milestones() {
    let mut texts = vec![("SKILL.md".to_owned(), skill())];
    texts.extend(book());
    let mut forge = 0;
    for (name, text) in &texts {
        let rest = without_forge_parts(text);
        let words = rest.to_lowercase();
        assert!(!words.contains("milestone"), "{name} names milestones");
        forge += usize::from(rest.len() < text.len());
    }
    // The skill, the requirements and the page "Waves" each have a forge part.
    assert_eq!(forge, 3);
}

#[test]
fn the_skill_the_requirements_and_the_page_agree() {
    let skill = flat(&skill());
    let requirements = flat(&read("docs/src/requirements.md"));
    let page = flat(&read("docs/src/waves.md"));
    for text in [
        "Wave 1, Wave 2, and so on. The waves run in number order.",
        "The current wave is the open wave with the lowest number.",
        "The next wave is the open wave after it.",
        "has no waves, each open item is in the current wave.",
        "`Needs:` line",
        "An item is merged when it is closed, when its wave has ended, or when it has",
        "`Merged in COMMIT`",
        "A person or a session can add a work item at any time, with no wave.",
        "Each item is in a later wave than each of its needs.",
        "No item blocks or breaks the other work of its wave",
        "the last number plus one.",
        "has the leads of more than one person, the people agree on one lead to plan",
        "wave is a milestone named `Wave N`. A name can follow, for example `Wave 5: Cloud`.",
        "A work item is an issue in the milestone.",
    ] {
        for (name, doc) in [
            ("SKILL.md", &skill),
            ("requirements.md", &requirements),
            ("waves.md", &page),
        ] {
            assert!(doc.contains(text), "{name} does not say {text:?}");
        }
    }
}

#[test]
fn a_session_takes_its_work_from_the_current_wave() {
    let requirements = flat(&read("docs/src/requirements.md"));
    let r166 = &requirements[requirements.find("**R166**").unwrap()..];
    let r166 = &r166[..r166.find(" - **R").unwrap()];
    for text in [
        "an open work item of the current wave (R214)",
        "an item of the next wave whose needs are merged (R215)",
        "It never picks an item whose needs are open.",
    ] {
        assert!(r166.contains(text), "R166 does not say {text:?}: {r166}");
    }
    let how = flat(&read("docs/src/how-it-works.md"));
    assert!(how.contains("picks the free item of the current wave"));
}

#[test]
fn the_repository_notes_have_no_wave_rule() {
    let notes = read("CLAUDE.md").to_lowercase();
    assert!(!notes.contains("wave"), "{notes}");
    assert!(!notes.contains("milestone"), "{notes}");
}

#[test]
fn each_riff_command_of_the_page_is_real() {
    let page = read("docs/src/waves.md");
    let mut riff = Vec::new();
    let mut in_sh = false;
    for line in page.lines() {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && line.starts_with("riff ") {
            riff.push(line.to_owned());
        }
    }
    assert_eq!(riff, [r#"riff tell lead "New item: issue-70""#]);
    for command in riff {
        let words: Vec<&str> = command.split_whitespace().collect();
        Command::cargo_bin("riff")
            .unwrap()
            .args(&words[1..2])
            .arg("--help")
            .assert()
            .success();
    }
}
