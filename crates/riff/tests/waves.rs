//! Waves (R213-R225): the skill, the requirements and the book page
//! "Waves" agree. Only their parts "on GitHub" name the objects of the
//! forge that hold a wave (R222). `riff/CLAUDE.md` has no wave rule:
//! the skill teaches it.

use isolated::Isolated;
use std::fs;
use std::path::{Path, PathBuf};

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
    // The skill, the requirements, the page "Waves", the page
    // "Development" (check a pull request), the page "Start a Riff"
    // (the rules of gh) and the design of the plan each have a forge
    // part.
    assert_eq!(forge, 6);
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
        "An item is merged when it is closed, or when it has",
        "comment `Merged in #PR (COMMIT)`",
        "closed when it is merged and each check after the release passed.",
        "A wave is done when each of its items is closed.",
        "No session starts an item of the next wave before the current wave is done.",
        "`Merged in #PR (COMMIT)`",
        "A person or a session can add a work item at any time, with no wave.",
        "Each item is in a later wave than each of its needs.",
        "No item blocks or breaks the other work of its wave",
        "the last number plus one.",
        "has the leads of more than one person, the people agree on one lead to plan",
        "wave is a milestone named `Wave N`. A name can follow, for example `Wave 5: Cloud`.",
        "A work item is an issue in the milestone.",
        "merge each item, make a release, deploy the release to the shared server, stop the workers, update each machine, start the sessions again, run the checks after the release, close each item.",
        "A riff with no shared server of its own code skips the release and the deploy.",
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

/// A check that only the shared riff can test waits for the release
/// of the wave (R194, 01M3MRDEXSMFT0STXF2HAR2QZ6). A live check of new
/// code runs in a dev session. No text says "after the merge" for a
/// check.
#[test]
fn a_check_of_the_shared_riff_waits_for_the_release() {
    let skill = flat(&skill());
    let requirements = flat(&read("docs/src/requirements.md"));
    let verify = flat(&read("docs/src/how-it-works.md"));
    let development = flat(&read("docs/src/development.md"));
    let page = flat(&read("docs/src/waves.md"));
    for (name, doc) in [
        ("SKILL.md", &skill),
        ("requirements.md", &requirements),
        ("how-it-works.md", &verify),
        ("development.md", &development),
    ] {
        assert!(
            doc.contains("only the shared riff can test is a check after the release"),
            "{name}"
        );
    }
    for (name, doc) in [
        ("SKILL.md", &skill),
        ("requirements.md", &requirements),
        ("how-it-works.md", &verify),
        ("development.md", &development),
        ("waves.md", &page),
    ] {
        assert!(doc.contains("check after the release"), "{name}");
        assert!(!doc.contains("check after the merge"), "{name}");
        assert!(!doc.contains("checks after the merge"), "{name}");
    }
    for (name, doc) in [("SKILL.md", &skill), ("requirements.md", &requirements)] {
        assert!(doc.contains("runs in a dev session"), "{name}");
    }
}

#[test]
fn a_session_takes_its_work_from_the_current_wave() {
    let requirements = flat(&read("docs/src/requirements.md"));
    let r166 = &requirements[requirements.find("**R166**").unwrap()..];
    let r166 = &r166[..r166.find(" - **R").unwrap()];
    for text in [
        "an open work item of the current wave (R214)",
        "It takes work only from the current wave (R216).",
        "It never picks an item whose needs are open (R215).",
        "the session verifies, runs its checks after the release, or waits.",
    ] {
        assert!(r166.contains(text), "R166 does not say {text:?}: {r166}");
    }
    assert!(!r166.contains("next wave"), "{r166}");
    let how = flat(&read("docs/src/how-it-works.md"));
    assert!(how.contains("picks the free item of the current wave"));
}

#[test]
fn a_milestone_closes_only_with_no_open_issue() {
    let requirements = flat(&read("docs/src/requirements.md"));
    let r224 = &requirements[requirements.find("**R224**").unwrap()..];
    let r224 = &r224[..r224.find(" - **R").unwrap()];
    for doc in [r224, &flat(&read("docs/src/waves.md"))] {
        assert!(
            doc.contains("only when the milestone has no open issue"),
            "{doc}"
        );
        assert!(!doc.contains("stay in"), "{doc}");
    }
    assert!(flat(&skill()).contains("Close it only when it has no open issue."));
}

#[test]
fn no_text_says_a_wave_ends_when_its_items_are_merged() {
    let mut texts = vec![("SKILL.md".to_owned(), skill())];
    texts.extend(book());
    for (name, text) in &texts {
        let text = flat(text);
        for old in [
            "A wave ends when each of its items is merged",
            "when its wave has ended",
            "take an item of the next wave",
        ] {
            assert!(!text.contains(old), "{name} says {old:?}");
        }
    }
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
    assert_eq!(
        riff,
        [
            r#"riff tell lead "New item: issue-70""#,
            "riff wave list",
            r#"riff wave make "Wave 6: Cloud""#,
            r#"riff wave rename 6 "Cloud and sign-in""#,
            "riff wave end 6",
        ]
    );
    for command in riff {
        let words: Vec<&str> = command.split_whitespace().collect();
        Isolated::shared()
            .assert_riff()
            .args(&words[1..3])
            .arg("--help")
            .assert()
            .success();
    }
}

/// A fake `gh` in `bin`: it logs each call to `bin/gh.log`, answers the
/// list of the open milestones with two waves and the backlog, and
/// answers a POST, a PATCH of a title and another PATCH with a
/// milestone.
fn fake_gh(bin: &Path) {
    let open = r#"[{"number":10,"title":"Backlog","open_issues":61,"closed_issues":4},{"number":33,"title":"Wave 28","open_issues":2,"closed_issues":0},{"number":32,"title":"Wave 27: Cloud","open_issues":0,"closed_issues":9}]"#;
    let made = r#"{"number":34,"title":"Wave 29","open_issues":0,"closed_issues":0}"#;
    let renamed = r#"{"number":33,"title":"Wave 28: 1.x, fixes that slow the work","open_issues":2,"closed_issues":0}"#;
    let ended = r#"{"number":32,"title":"Wave 27: Cloud","open_issues":0,"closed_issues":9}"#;
    let path = bin.join("gh");
    isolated::write_program(
        &path,
        format!(
            "#!/bin/sh\necho \"gh $*\" >> \"$(dirname \"$0\")/gh.log\"\ncase \"$*\" in\n\
             'api repos/{{owner}}/{{repo}}/milestones?state=open&per_page=100') printf '%s' '{open}' ;;\n\
             'api -X POST '*) printf '%s' '{made}' ;;\n\
             'api -X PATCH '*'-f title='*) printf '%s' '{renamed}' ;;\n\
             'api -X PATCH '*) printf '%s' '{ended}' ;;\n\
             *) exit 1 ;;\nesac\n"
        ),
    );
}

/// `riff wave ARGS` with the fake `gh` of `bin` first in `PATH`.
fn riff_wave(env: &Isolated, bin: &Path, args: &[&str]) -> std::process::Output {
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    env.riff()
        .arg("wave")
        .args(args)
        .current_dir(bin)
        .env("PATH", path)
        .output()
        .unwrap()
}

/// 01M509TJRBE2J7HYKKHRJ1ZFR0, 01M509TJTV7TC32EKXW0TEQ5NZ,
/// 01M509TJXE04CTG5D1K751WXJY: the steps of the waves of the lead.
#[test]
fn riff_wave_lists_makes_and_ends_the_waves_with_gh() {
    let env = Isolated::new();
    let bin = tempfile::tempdir().unwrap();
    fake_gh(bin.path());
    let text = |out: &std::process::Output| {
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        stdout + &String::from_utf8_lossy(&out.stderr)
    };

    let list = riff_wave(&env, bin.path(), &["list"]);
    assert!(list.status.success(), "{}", text(&list));
    // The folder is in no git repository: no items and no hosts, and
    // one line on stderr says why (01M58ATZ6FA5PJMD3D848CEWBJ).
    assert_eq!(
        text(&list),
        "Wave 27: Cloud  current  0 open, 9 closed\nWave 28  2 open, 0 closed\nBacklog  61 \
         open, 4 closed\nriff: the list has no items and no hosts: this folder is in no git \
         repository\n"
    );

    let made = riff_wave(&env, bin.path(), &["make", "Wave", "29"]);
    assert!(made.status.success(), "{}", text(&made));
    assert_eq!(text(&made), "Made the wave Wave 29.\n");

    let open = riff_wave(&env, bin.path(), &["end", "28"]);
    assert_eq!(open.status.code(), Some(1), "{}", text(&open));
    assert!(text(&open).contains("The wave Wave 28 has 2 open items. riff ended nothing."));

    let ended = riff_wave(&env, bin.path(), &["end", "27"]);
    assert!(ended.status.success(), "{}", text(&ended));
    assert_eq!(
        text(&ended),
        "Ended the wave Wave 27: Cloud. Its 9 items are closed.\n"
    );

    let log = fs::read_to_string(bin.path().join("gh.log")).unwrap();
    let writes: Vec<&str> = log.lines().filter(|l| l.contains(" -X ")).collect();
    assert_eq!(
        writes,
        [
            "gh api -X POST repos/{owner}/{repo}/milestones -f title=Wave 29",
            "gh api -X PATCH repos/{owner}/{repo}/milestones/32 -f state=closed",
        ]
    );
}

/// 01M518A0BEQBX9RH9H0CEFYF9Q: `riff wave rename N NAME` gives the open
/// wave N a new name in one step, and keeps its number.
#[test]
fn riff_wave_rename_gives_the_wave_a_new_name_and_keeps_its_number() {
    let env = Isolated::new();
    let bin = tempfile::tempdir().unwrap();
    fake_gh(bin.path());
    let text = |out: &std::process::Output| {
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        stdout + &String::from_utf8_lossy(&out.stderr)
    };

    let renamed = riff_wave(
        &env,
        bin.path(),
        &[
            "rename", "28", "1.x,", "fixes", "that", "slow", "the", "work",
        ],
    );
    assert!(renamed.status.success(), "{}", text(&renamed));
    assert_eq!(
        text(&renamed),
        "Renamed the wave Wave 28 to Wave 28: 1.x, fixes that slow the work. Each pull request \
         names it `Milestone: Wave 28`, so no pull request changes.\n"
    );

    let none = riff_wave(&env, bin.path(), &["rename", "26", "Old"]);
    assert_eq!(none.status.code(), Some(1), "{}", text(&none));
    assert!(text(&none).contains("No open wave has the number 26."));

    let blank = riff_wave(&env, bin.path(), &["rename", "28", " "]);
    assert_eq!(blank.status.code(), Some(1), "{}", text(&blank));
    assert!(text(&blank).contains("riff wave rename needs a name"));

    let log = fs::read_to_string(bin.path().join("gh.log")).unwrap();
    let writes: Vec<&str> = log.lines().filter(|l| l.contains(" -X ")).collect();
    assert_eq!(
        writes,
        [
            "gh api -X PATCH repos/{owner}/{repo}/milestones/33 -f title=Wave 28: 1.x, fixes that \
          slow the work"
        ]
    );
}
