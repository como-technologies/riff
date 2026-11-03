//! No text of riff for a session, no requirement and no page of the
//! book sends a reader to a skill: riff installs none
//! (01M6GXAG10W6EJ89AQA5B4HHPJ). The steps that the texts name are in the
//! instructions of the riff tools (01M6GXACQVG2EZBAGYXG3D5ED7,
//! 01M6GXAFYJYG9JC76W651KF2KT).

use std::fs;
use std::path::{Path, PathBuf};

use riff::text::{MCP_CONTAINER, MCP_LEAD, MCP_TOOLS};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The words that send a reader to the skill that riff no longer installs.
const PHRASES: [&str; 3] = ["the skill", "riff skill", "lead skill"];

/// The phrase of `text` that names the skill, if there is one. A text
/// wraps in the middle of a phrase: the check reads the white space of
/// the whole text as one space.
fn names_the_skill(text: &str) -> Option<&'static str> {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = flat.to_lowercase();
    PHRASES.into_iter().find(|phrase| lower.contains(phrase))
}

/// Each file of `dir` with the extension `ext`, and of its folders.
fn files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files(&path, ext, out);
        } else if path.extension().is_some_and(|x| x == ext) {
            out.push(path);
        }
    }
}

/// The code of a Rust file: its lines that are not a comment. A text
/// that riff sends is in the code, in a string. A string that wraps with
/// `\` at the end of a line is one line.
fn code(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("\\\n", " ")
}

#[test]
fn the_check_reads_a_string_that_wraps_in_the_code() {
    let source = "let a = \"see in the riff \\\n             skill.\";\n// the skill\n";
    assert_eq!(names_the_skill(&code(source)), Some("riff skill"));
    assert_eq!(
        names_the_skill(&code("// the skill\n/// the skill\n")),
        None
    );
}

#[test]
fn the_check_finds_the_skill_in_a_wrapped_text() {
    assert_eq!(
        names_the_skill("see \"Pause\" in the riff\n    skill."),
        Some("riff skill")
    );
    assert_eq!(names_the_skill("The skill says so."), Some("the skill"));
    assert_eq!(names_the_skill("the lead skill"), Some("lead skill"));
    assert_eq!(
        names_the_skill("see \"Pause\" in the instructions of the riff tools."),
        None
    );
    // A setting of Claude Code and a test text about secrets are no skill of riff.
    assert_eq!(
        names_the_skill("`skillOverrides` with `auto-mode-setup`"),
        None
    );
    assert_eq!(
        names_the_skill("the sk-learn skill and the ghp_ prefix"),
        None
    );
}

#[test]
fn the_code_of_each_crate_sends_no_text_to_the_skill() {
    let mut paths = Vec::new();
    let mut dirs: Vec<_> = fs::read_dir(repo().join("crates"))
        .unwrap()
        .map(|e| e.unwrap().path().join("src"))
        .filter(|dir| dir.is_dir())
        .collect();
    dirs.sort();
    for dir in &dirs {
        files(dir, "rs", &mut paths);
    }
    assert!(paths.len() > 50, "{} files", paths.len());
    for path in paths {
        let code = code(&fs::read_to_string(&path).unwrap());
        assert_eq!(
            names_the_skill(&code),
            None,
            "{}: a text names the skill",
            path.display()
        );
    }
}

#[test]
fn the_requirements_and_the_book_name_no_skill() {
    let mut pages = Vec::new();
    files(&repo().join("docs/src"), "md", &mut pages);
    pages.retain(|page| !page.ends_with("requirements-history.md"));
    assert!(pages.iter().any(|p| p.ends_with("requirements.md")));
    assert!(pages.iter().any(|p| p.ends_with("how-it-works.md")));
    for page in pages {
        let text = fs::read_to_string(&page).unwrap();
        assert_eq!(
            names_the_skill(&text),
            None,
            "{} names the skill",
            page.display()
        );
    }
}

/// The part `name` of the instructions `text`: it starts a paragraph or
/// follows a sentence, and has the name and a full stop.
fn has_part(text: &str, name: &str) -> bool {
    text.starts_with(&format!("{name}. "))
        || text.contains(&format!("\n\n{name}. "))
        || text.contains(&format!(". {name}. "))
}

#[test]
fn the_instructions_hold_the_parts_that_the_texts_name() {
    for (kind, text) in [
        ("host", MCP_TOOLS),
        ("container", MCP_CONTAINER),
        ("lead", MCP_LEAD),
    ] {
        assert!(has_part(text, "Pause"), "{kind}: no Pause");
        assert_eq!(names_the_skill(text), None, "{kind}");
    }
    assert!(has_part(MCP_TOOLS, "Start routine"));
    assert!(has_part(MCP_TOOLS, "Pick up dropped work"));
    assert!(!has_part(MCP_CONTAINER, "Pick up dropped work"));
}

/// Each text that names a part of the instructions of the riff tools:
/// the part is there, and the text names no skill.
#[test]
fn each_text_names_a_part_that_the_instructions_hold() {
    use riff_core::wire::RiffState;
    use riff_state::state::{Dropped, dropped};

    let strata = "como-technologies/strata".parse().unwrap();
    let me = "riff://mike@pangolin/como-technologies/riff?session=a6cf"
        .parse()
        .unwrap();
    let freed = riff_core::wire::Freed {
        thread: "como-technologies/riff".parse().unwrap(),
        item: "issue-12".into(),
    };
    let earlier = riff::dropped::Earlier {
        item: "issue-12".into(),
        pushed: vec![],
        kept: vec![riff::dropped::Kept {
            path: "/w/issue-12".into(),
            changed: 1,
            not_pushed: 0,
        }],
    };
    let texts = [
        riff_client::text::riff_news(None, RiffState::Paused),
        riff_client::text::riff_news(Some(&strata), RiffState::Paused),
        riff::text::joined(&me),
        dropped(
            &Dropped::Start(riff_core::wire::StartReason::Clear),
            std::slice::from_ref(&freed),
        ),
        earlier.claim_line(),
        riff::dropped::start_lines(std::slice::from_ref(&earlier)).unwrap(),
    ];
    let mut named = 0;
    for text in &texts {
        assert_eq!(names_the_skill(text), None, "{text}");
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for part in flat.split("\"").skip(1).step_by(2) {
            if flat.contains(&format!("\"{part}\" in the instructions of the riff tools")) {
                named += 1;
                assert!(has_part(MCP_TOOLS, part), "{part}: {text}");
            }
        }
    }
    assert!(named >= 4, "{named} parts");
}
