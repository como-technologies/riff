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
/// next heading. A line that starts with `#` in a code block is no
/// heading.
fn under(name: &str, heading: &str) -> String {
    let page = book::page(name);
    let start = page
        .lines()
        .position(|line| line == heading)
        .unwrap_or_else(|| panic!("{heading} is in {name}"));
    let rest: Vec<&str> = page.lines().skip(start + 1).collect();
    let mut fenced = false;
    let end = rest.iter().position(|line| {
        if line.starts_with("```") {
            fenced = !fenced;
        }
        !fenced && line.starts_with('#')
    });
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

/// A member that joins for each path.
const ALL: &str = "+++
files = [\"**\"]
+++

# Performance

You review the time and the memory of each change.

## A read of the log reads each line one time

A second pass over the log is a finding.
";

/// `git ARGS` in `dir`, as Ada, with no signature.
fn git(dir: &Path, args: &[&str]) {
    let out = Isolated::shared()
        .command("git")
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A bare `origin.git` in `root`, and the clone `work` of it, with one
/// commit on `main` that has no member file. Gives the path of `work`.
fn clone_with_origin(root: &Path) -> std::path::PathBuf {
    git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    git(root, &["init", "-q", "-b", "main", "work"]);
    let work = root.join("work");
    git(&work, &["commit", "-q", "--allow-empty", "-m", "start"]);
    let origin = root.join("origin.git");
    git(
        &work,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(&work, &["push", "-q", "origin", "main"]);
    work
}

/// A pull request of the base branch `main` for the fake `gh`: its
/// number, the paths of its files and its labels.
type Pull<'a> = (u64, &'a [&'a str], &'a [&'a str]);

/// A fake `gh` in a new folder. It answers the reads of
/// `riff council show` for the pull requests `pulls`. The issue of each
/// one is issue 12, with no label. Each other call fails.
fn fake_gh(pulls: &[Pull]) -> tempfile::TempDir {
    let bin = tempfile::tempdir().unwrap();
    let mut cases = String::new();
    for (number, paths, labels) in pulls {
        let labels: Vec<String> = labels
            .iter()
            .map(|label| format!("{{\"name\":\"{label}\"}}"))
            .collect();
        cases.push_str(&format!(
            "  'pr view {number} --json baseRefName,body,labels') printf '%s' '{{\"baseRefName\":\"main\",\"body\":\"Closes #12\\n\\nIssue: #12\\n\",\"labels\":[{}]}}' ;;\n",
            labels.join(",")
        ));
        let paths: Vec<String> = paths.iter().map(|path| format!("'{path}'")).collect();
        cases.push_str(&format!(
            "  'api repos/{{owner}}/{{repo}}/pulls/{number}/files?per_page=100 --paginate --jq '*) printf '%s\\n' {} ;;\n",
            paths.join(" ")
        ));
    }
    cases.push_str("  'issue view 12 --json labels') printf '%s' '{\"labels\":[]}' ;;\n");
    isolated::write_program(
        bin.path().join("gh"),
        format!(
            "#!/bin/sh\ncase \"$*\" in\n{cases}  *) echo \"gh: no such call: $*\" >&2; exit 1 ;;\nesac\n"
        ),
    );
    bin
}

/// `riff council show NUMBER` in the folder `dir`, with the fake `gh` of
/// `bin` first in `PATH`.
fn show(dir: &Path, bin: &Path, number: u64) -> Output {
    let mut path = OsString::from(bin);
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    Isolated::shared()
        .riff()
        .args(["council", "show", &number.to_string()])
        .current_dir(dir)
        .env("PATH", path)
        .output()
        .unwrap()
}

/// The stdout of `out`, after a check that the command worked.
fn stdout_of(out: Output) -> String {
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// The text after `== ` of each line of `out` that starts with it.
fn headings(out: &str) -> Vec<&str> {
    out.lines()
        .filter_map(|line| line.strip_prefix("== "))
        .collect()
}

/// 01M7CFBZ7839WHMFARR4T6W13W, 01M7CRX66K9Q750SB8M2MPG6V7: a pull
/// request that adds `.riff/council/performance.md` with `files =
/// ["**"]` does not get `performance`, because the base branch has no
/// such file. The folder has it, and riff does not read it. Another clone
/// merges the file into the base branch. Then a second pull request gets
/// `performance`, from a folder with no such file: riff fetches the base
/// branch and reads it. When the fetch fails, riff says so, and reads the
/// base branch as the clone has it.
#[test]
fn riff_council_show_reads_the_member_files_of_the_base_branch() {
    let root = tempfile::tempdir().unwrap();
    let work = clone_with_origin(root.path());
    git(&work, &["switch", "-q", "-c", "performance"]);
    member(&work, "performance.md", ALL);
    git(&work, &["add", "."]);
    git(
        &work,
        &["commit", "-q", "-m", "The council gets performance"],
    );
    let bin = fake_gh(&[
        (40, &[".riff/council/performance.md"], &[]),
        (41, &["src/a.rs"], &[]),
    ]);
    let stdout = stdout_of(show(&work, bin.path(), 40));
    assert!(
        stdout.starts_with("The council of pull request #40 has 3 members. riff read the member files of origin/main.\n"),
        "{stdout}"
    );
    assert_eq!(
        headings(&stdout),
        [
            "design (riff): each council",
            "testing (riff): each council",
            "docs (riff): each council"
        ],
        "{stdout}"
    );
    assert!(!stdout.contains("performance"), "{stdout}");

    git(
        root.path(),
        &[
            "clone",
            "-q",
            root.path().join("origin.git").to_str().unwrap(),
            "other",
        ],
    );
    let other = root.path().join("other");
    member(&other, "performance.md", ALL);
    git(&other, &["add", "."]);
    git(
        &other,
        &["commit", "-q", "-m", "The council gets performance"],
    );
    git(&other, &["push", "-q", "origin", "main"]);
    git(&work, &["switch", "-q", "main"]);
    assert!(!work.join(".riff/council/performance.md").exists());
    let stdout = stdout_of(show(&work, bin.path(), 41));
    let joined = [
        "design (riff): each council",
        "testing (riff): each council",
        "docs (riff): each council",
        "rust (riff): the path src/a.rs matches **/*.rs",
        "performance (.riff/council/performance.md): the path src/a.rs matches **",
    ];
    assert_eq!(headings(&stdout), joined, "{stdout}");
    // The text of the file after the table.
    let text = &ALL[ALL.find("# Performance").unwrap()..];
    assert!(stdout.ends_with(&format!("\n\n{text}")), "{stdout}");

    git(
        &work,
        &["remote", "set-url", "origin", "/no/such/origin.git"],
    );
    let out = show(&work, bin.path(), 41);
    let stderr = String::from_utf8(out.stderr.clone()).unwrap();
    assert!(
        stderr.contains("riff cannot fetch origin/main: ")
            && stderr.contains("riff reads the member files of origin/main as the clone has it."),
        "{stderr}"
    );
    assert_eq!(headings(&stdout_of(out)), joined);
}

/// `riff council show` exits with 1 when `gh` cannot read the pull
/// request, and names the call.
#[test]
fn riff_council_show_exits_with_1_when_gh_cannot_read_the_pull_request() {
    let root = tempfile::tempdir().unwrap();
    let work = clone_with_origin(root.path());
    let bin = fake_gh(&[]);
    let out = show(&work, bin.path(), 42);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("gh pr view 42"), "{stderr}");
}

/// "See the council of a pull request" in the book has
/// `riff council show 40` in a copyable `sh` block. Each line of its
/// example comes, in that order, in the output for a pull request that
/// changes a `.rs` file and a page of the book, with the label
/// `security`.
#[test]
fn the_book_shows_the_council_of_a_pull_request() {
    let how_to = under("how-it-works.md", "#### See the council of a pull request");
    assert_eq!(book::commands_in(&how_to), ["riff council show 40"]);
    let example = how_to
        .split("```text\n")
        .nth(1)
        .and_then(|rest| rest.split("\n```").next())
        .expect("an example of the output");

    let root = tempfile::tempdir().unwrap();
    let work = clone_with_origin(root.path());
    let bin = fake_gh(&[(
        40,
        &["crates/riff/src/council.rs", "docs/src/how-it-works.md"],
        &["security"],
    )]);
    let stdout = stdout_of(show(&work, bin.path(), 40));
    let mut rest = stdout.lines();
    for line in example.lines().filter(|line| *line != "...") {
        assert!(
            rest.any(|out| out == line),
            "{line:?} is not in this order in:\n{stdout}"
        );
    }
    assert!(
        example.contains("== Left out by the limit of 6"),
        "{example}"
    );
}
