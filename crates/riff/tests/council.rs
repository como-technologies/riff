//! `riff council list` lists the members of the council of a folder: the
//! files of `.riff/council/` over the default set that riff builds in
//! (01M7CFBZ7839WHMFARR4T6W13W, 01M7CFBZMCE36ZRXD0GBCM4MFX,
//! 01M7CN1H1XGQVBGFH30MRE1BP4). The book has the how-to.

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Output;

use isolated::Isolated;

use crate::book;

/// The default set, in the order of 01M7CFBZHQZ8FDT3Q0NGYKBZWB, with
/// the start of the column JOINS of each member.
const DEFAULT_SET: [(&str, &str); 9] = [
    ("design", "each council"),
    ("testing", "each council"),
    ("docs", "each council"),
    ("rust", "files **/*.rs"),
    ("security", "files crates/riff-server/**"),
    ("attacker", "files crates/riff-server/**"),
    ("requirements", "files **/requirements*.md"),
    ("newcomer", "files docs/**"),
    ("operations", "files **/*daemon*"),
];

/// The member of the how-to "Add a member to the council".
const PERFORMANCE: &str = "+++
files = [\"crates/riff-server/**\"]
labels = [\"performance\"]
+++

# Performance

You review the time and the memory of riff-server.

## A read of the log reads each line one time

A second pass over the log is a finding.
";

/// A new git repository.
fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let out = Isolated::shared()
        .command("git")
        .args(["init", "-q"])
        .arg(root.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    root
}

/// The member file `name` of the repository `root`, with the text `text`.
fn member(root: &Path, name: &str, text: &str) {
    let folder = root.join(".riff/council");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(name), text).unwrap();
}

/// `riff council list` in the folder `dir`.
fn list(dir: &Path) -> Output {
    Isolated::shared()
        .riff()
        .args(["council", "list"])
        .current_dir(dir)
        .output()
        .unwrap()
}

/// The words of each row of the table `out`, with no header.
fn rows(out: &str) -> Vec<Vec<String>> {
    let mut lines = out.lines();
    let head: Vec<&str> = lines.next().unwrap().split_whitespace().collect();
    assert_eq!(head, ["MEMBER", "SOURCE", "RULES", "JOINS"], "{out}");
    lines
        .map(|line| line.split_whitespace().map(str::to_owned).collect())
        .collect()
}

/// 01M7CFBZMCE36ZRXD0GBCM4MFX, 01M7CN1H1XGQVBGFH30MRE1BP4: in a clone
/// with the file `.riff/council/performance.md`, also from a folder
/// under the top, the list names the 9 default members with the source
/// riff, then `performance` with the path of its file, each with when it
/// joins and the count of its rules.
#[test]
fn riff_council_list_names_the_default_set_and_a_member_of_the_folder() {
    let root = repository();
    member(root.path(), "performance.md", PERFORMANCE);
    let sub = root.path().join("crates/a");
    fs::create_dir_all(&sub).unwrap();
    for dir in [root.path(), &sub] {
        let out = list(dir);
        assert!(out.status.success(), "{out:?}");
        let stdout = String::from_utf8(out.stdout).unwrap();
        let rows = rows(&stdout);
        assert_eq!(rows.len(), 10, "{stdout}");
        for ((name, joins), row) in DEFAULT_SET.iter().zip(&rows) {
            assert_eq!(
                (row[0].as_str(), row[1].as_str()),
                (*name, "riff"),
                "{stdout}"
            );
            assert!(row[2].parse::<usize>().unwrap() > 0, "{stdout}");
            assert!(row[3..].join(" ").starts_with(joins), "{name}: {stdout}");
        }
        assert_eq!(
            rows[9],
            [
                "performance",
                ".riff/council/performance.md",
                "1",
                "files",
                "crates/riff-server/**;",
                "labels",
                "performance"
            ],
            "{stdout}"
        );
    }
}

/// 01M7CFBZ7839WHMFARR4T6W13W: a file with the name of a default member
/// takes its place in the list, with its file as its source.
#[test]
fn riff_council_list_shows_a_file_in_the_place_of_a_default_member() {
    let root = repository();
    member(
        root.path(),
        "rust.md",
        "+++\nfiles = [\"src/**\"]\n+++\n# Our Rust\n\n## A\n\n## B\n",
    );
    let out = list(root.path());
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let rows = rows(&stdout);
    assert_eq!(rows.len(), 9, "{stdout}");
    assert_eq!(
        rows[3],
        ["rust", ".riff/council/rust.md", "2", "files", "src/**"],
        "{stdout}"
    );
}

/// 01M7CN1H1XGQVBGFH30MRE1BP4: on a member file that riff cannot read,
/// `riff council list` exits with 1, prints no table, and names the
/// file and the line.
#[test]
fn riff_council_list_exits_with_1_on_a_bad_file_and_names_it() {
    let root = repository();
    member(root.path(), "performance.md", PERFORMANCE);
    for (file, text, named) in [
        (
            "perf.md",
            "# Perf\n",
            ".riff/council/perf.md:1: the file has no table.",
        ),
        (
            "perf.md",
            "+++\nfile = [\"a\"]\n+++\n# Perf\n",
            ".riff/council/perf.md:2: riff does not know the key file.",
        ),
        (
            "Perf.md",
            "+++\n+++\n# Perf\n",
            ".riff/council/Perf.md: \"Perf\" is not the name of a member.",
        ),
    ] {
        for old in ["perf.md", "Perf.md"] {
            let _ = fs::remove_file(root.path().join(".riff/council").join(old));
        }
        member(root.path(), file, text);
        let out = list(root.path());
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        assert!(out.stdout.is_empty(), "{out:?}");
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(stderr.contains(named), "{stderr}");
    }
}

/// The text of the first `sh` block of `text`.
fn first_sh_block(text: &str) -> String {
    let start = text.find("```sh\n").expect("an sh block") + "```sh\n".len();
    let end = text[start..].find("\n```\n").expect("the end of the block");
    text[start..start + end].to_owned()
}

/// The text under the line `heading` of the book page `name`, up to the
/// next heading.
fn under(name: &str, heading: &str) -> String {
    let page = book::page(name);
    let start = page
        .lines()
        .position(|line| line == heading)
        .unwrap_or_else(|| panic!("{heading} is in {name}"));
    let rest: Vec<&str> = page.lines().skip(start + 1).collect();
    let end = rest.iter().position(|line| line.starts_with('#'));
    rest[..end.unwrap_or(rest.len())].join("\n")
}

/// The how-to "Add a member to the council" ends its copyable `sh` block
/// with `riff council list`. The block runs in a new clone, and its last
/// command lists the new member. Each row of the example in "List the
/// members of the council" is a row of that output.
#[test]
fn the_book_adds_a_member_and_lists_it() {
    let part = book::part("development.md", "Add a member to the council");
    let block = first_sh_block(&part);
    assert_eq!(block.lines().last(), Some("riff council list"), "{block}");
    assert!(block.contains(PERFORMANCE), "{block}");

    let root = repository();
    let env = Isolated::shared();
    let bin = env.riff_path().parent().unwrap().to_owned();
    let mut path = OsString::from(bin);
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    let out = env
        .command("bash")
        .args(["-e", "-c", &block])
        .current_dir(root.path())
        .env("PATH", path)
        .env("GIT_AUTHOR_NAME", "Ada")
        .env("GIT_AUTHOR_EMAIL", "ada@example.com")
        .env("GIT_COMMITTER_NAME", "Ada")
        .env("GIT_COMMITTER_EMAIL", "ada@example.com")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let last = stdout.lines().last().unwrap_or_default();
    assert!(
        last.starts_with("performance ") && last.contains(".riff/council/performance.md"),
        "{stdout}"
    );

    let how_to = under("development.md", "### List the members of the council");
    assert_eq!(book::commands_in(&how_to), ["riff council list"]);
    let example = how_to
        .split("```text\n")
        .nth(1)
        .and_then(|rest| rest.split("\n```").next())
        .expect("an example of the output");
    let table = String::from_utf8(list(root.path()).stdout).unwrap();
    for line in example.lines().filter(|line| *line != "...") {
        assert!(
            table.lines().any(|row| row == line),
            "{line:?} is not in:\n{table}"
        );
    }
}

/// "The members of the council" in the book says in the present tense
/// that riff builds in the default set, and points to the how-to of
/// `riff council list`.
#[test]
fn the_book_says_that_riff_builds_in_the_default_set() {
    let text = under("how-it-works.md", "#### The members of the council");
    let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(words.contains("and riff builds it in."), "{words}");
    assert!(!words.contains("will build"), "{words}");
    assert!(
        words.contains("(development.md#list-the-members-of-the-council)"),
        "{words}"
    );
}
