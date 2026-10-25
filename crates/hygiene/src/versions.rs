//! The versions check: `hygiene versions [ROOT]` fails when a text names
//! a version of riff.
//!
//! # Design
//!
//! A text, a rustdoc line and a page of the book describe the riff of
//! today. A version name in them ages: it was right on the day of the
//! release, and wrong on the next major release. So the code, its
//! tests and the book name no version of riff
//! (01M5R2XVDN99TE9NEJYY1GJE91, 01M5R2XVGMTB2HC9PRMS85ZWTY).
//! `just check` runs `hygiene versions`.
//!
//! The rule `versions` finds each name of the form `riff N` or
//! `riff N.M`, also `riff-server N`, in each file of these folders
//! under ROOT (the default is `.`), but in
//! `docs/src/requirements-history.md`: a requirement that a new one
//! superseded in full moved there with its text unchanged, and its
//! history names versions. `docs/src/requirements.md` holds the rules of
//! today, so the check reads it.
//! It finds a name that a line break splits too: `riff` at the end of
//! a line, and the number at the start of the next one.
//!
//! - `crates/*/src`
//! - `crates/*/tests`
//! - `docs/src`
//!
//! A use that must name a version stays. It is a line in the allow
//! list `crates/hygiene/versions.allow`:
//!
//! ```text
//! PATH | TEXT | REASON
//! ```
//!
//! - PATH is the path of the file from ROOT.
//! - TEXT is a part of a line of that file. It holds the version name.
//!   The line is allowed for each name in TEXT.
//! - REASON says why the name stays. The one allowed kind: a rule between
//!   versions, with its real text.
//!
//! A line of the list that allows nothing is an error: the list holds
//! no old entry. A line that starts with `#` is a comment.
//!
//! ```mermaid
//! flowchart TD
//!     F["each file of crates/*/src, crates/*/tests, docs/src"] --> L{"a line with<br/>riff N or riff N.M?"}
//!     L -- no --> OK["pass"]
//!     L -- yes --> A{"an allow line of the file<br/>holds that name?"}
//!     A -- yes --> OK
//!     A -- no --> E["error: rewrite the line,<br/>or allow it"]
//! ```
//!
//! ```
//! use hygiene::versions::{check, parse_allow};
//!
//! // The sample is built, so that this file names no version.
//! let line = format!("riff {} runs on Linux only.", "3.0");
//! let file = ("crates/a/src/text.rs".to_owned(), format!("{line}\n"));
//! let errors = check(&[file.clone()], &[]);
//! assert_eq!(errors.len(), 1);
//! assert_eq!(errors[0].rule, "versions");
//! assert!(errors[0].text.starts_with("crates/a/src/text.rs:1: "));
//!
//! let (allow, problems) = parse_allow(&format!(
//!     "crates/a/src/text.rs | riff {} runs | the rule of a release\n",
//!     "3.0"
//! ));
//! assert!(problems.is_empty());
//! assert!(check(&[file], &allow).is_empty());
//! ```

use std::io;
use std::ops::Range;
use std::path::Path;

use crate::Error;

/// The file of the allow list, from the root.
pub const ALLOW_FILE: &str = "crates/hygiene/versions.allow";

/// The page that the check skips: the history of the requirements
/// names versions.
pub const SKIPPED: &str = "docs/src/requirements-history.md";

/// A line of the allow list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowed {
    /// The line of the list, from 1.
    pub at: usize,
    /// The path of the file from the root.
    pub path: String,
    /// A part of a line of the file, with the version name.
    pub text: String,
    /// Why the name stays.
    pub reason: String,
}

fn word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The version names in `line`, each as the range of its bytes: `riff`
/// or `riff-server`, a space, and a number `N` or `N.M` with any
/// number of parts. A sentence dot after the number is not part of it.
///
/// ```
/// use hygiene::versions::names;
///
/// // The sample is built, so that this file names no version.
/// let line = format!("riff {} and riff-server {}.", 3, "4.1.1");
/// let found: Vec<&str> = names(&line).into_iter().map(|r| &line[r]).collect();
/// assert_eq!(found, [format!("riff {}", 3), format!("riff-server {}", "4.1.1")]);
/// assert!(names("riff 3x, riff more, a riff 2nd").is_empty());
/// ```
pub fn names(line: &str) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = line[from..].find("riff") {
        let start = from + offset;
        from = start + 4;
        if start > 0 && (word_byte(bytes[start - 1]) || bytes[start - 1] == b'-') {
            continue;
        }
        let mut at = start + 4;
        if line[at..].starts_with("-server") {
            at += 7;
        }
        if bytes.get(at) != Some(&b' ') {
            continue;
        }
        at += 1;
        let digits = |from: usize| {
            bytes[from..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count()
        };
        let mut n = digits(at);
        if n == 0 {
            continue;
        }
        at += n;
        while bytes.get(at) == Some(&b'.') {
            n = digits(at + 1);
            if n == 0 {
                break;
            }
            at += 1 + n;
        }
        if bytes.get(at).is_some_and(|b| word_byte(*b)) {
            continue;
        }
        found.push(start..at);
        from = at;
    }
    found
}

/// The lines of the allow list `source`, and the errors in it: a line
/// that is not `PATH | TEXT | REASON`, or that has an empty part.
pub fn parse_allow(source: &str) -> (Vec<Allowed>, Vec<Error>) {
    let mut allow = Vec::new();
    let mut errors = Vec::new();
    for (n, line) in source.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, " | ").map(str::trim).collect();
        match parts[..] {
            [path, text, reason] if ![path, text, reason].contains(&"") => allow.push(Allowed {
                at: n + 1,
                path: path.to_owned(),
                text: text.to_owned(),
                reason: reason.to_owned(),
            }),
            _ => errors.push(Error::new(
                "versions",
                format!(
                    "{ALLOW_FILE}:{}: the line is not `PATH | TEXT | REASON`",
                    n + 1
                ),
            )),
        }
    }
    (allow, errors)
}

/// True when `allow` has a line for `path` whose text holds the range
/// `name` of `line`.
fn allowed(
    allow: &[Allowed],
    used: &mut [bool],
    path: &str,
    line: &str,
    name: &Range<usize>,
) -> bool {
    let mut covered = false;
    for (entry, used) in allow.iter().zip(used) {
        if entry.path != path {
            continue;
        }
        let holds = line
            .match_indices(&entry.text)
            .any(|(at, text)| at <= name.start && name.end <= at + text.len());
        *used |= holds;
        covered |= holds;
    }
    covered
}

/// A name that a line break splits: `line` ends with `riff` or
/// `riff-server`, and `next` starts with the number, after the mark of a
/// comment. It gives the two lines as one, and the range of the name in
/// it. A line that ends with two spaces and a backslash, in a Rust
/// string, keeps the two spaces: the name is not split.
fn split_name(line: &str, next: &str) -> Option<(String, Range<usize>)> {
    let body = line.trim_end();
    let body = body.strip_suffix('\\').unwrap_or(body);
    if body.ends_with("  ") {
        return None;
    }
    let head = body.trim_end();
    let next = ["///", "//!", "//", ">"]
        .iter()
        .find_map(|mark| next.trim_start().strip_prefix(mark))
        .unwrap_or(next)
        .trim_start();
    let joined = format!("{head} {next}");
    names(&joined)
        .into_iter()
        .find(|name| name.start < head.len() && name.end > head.len())
        .map(|name| (joined, name))
}

/// The errors of the files `files` (each with its path from the root
/// and its text) against the list `allow`: one for each line with a
/// version name that no line of the list allows, and one for each line
/// of the list that allows no name.
pub fn check(files: &[(String, String)], allow: &[Allowed]) -> Vec<Error> {
    let mut used = vec![false; allow.len()];
    let mut errors = Vec::new();
    for (path, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        for (n, &line) in lines.iter().enumerate() {
            let mut open: Vec<String> = names(line)
                .into_iter()
                .filter(|name| !allowed(allow, &mut used, path, line, name))
                .map(|name| line[name].to_owned())
                .collect();
            let split = lines.get(n + 1).and_then(|next| split_name(line, next));
            if let Some((joined, name)) = split
                && !allowed(allow, &mut used, path, &joined, &name)
            {
                open.push(joined[name].to_owned());
            }
            if !open.is_empty() {
                errors.push(Error::new(
                    "versions",
                    format!(
                        "{path}:{}: the line names {}. Say what riff does, with no version. \
                         A name that must stay is a line of {ALLOW_FILE}: {}",
                        n + 1,
                        open.iter()
                            .map(|name| format!("\"{name}\""))
                            .collect::<Vec<_>>()
                            .join(", "),
                        line.trim()
                    ),
                ));
            }
        }
    }
    for (entry, used) in allow.iter().zip(used) {
        if !used {
            errors.push(Error::new(
                "versions",
                format!(
                    "{ALLOW_FILE}:{}: no line of {} has a version name in \"{}\". Remove the line.",
                    entry.at, entry.path, entry.text
                ),
            ));
        }
    }
    errors
}

/// The files that the check reads under `root`, each with its path
/// from `root`, in path order: each file in `crates/*/src`,
/// `crates/*/tests` and `docs/src`, but [`SKIPPED`] and a file that is
/// not UTF-8.
pub fn scan(root: &Path) -> io::Result<Vec<(String, String)>> {
    let mut dirs = vec![root.join("docs/src")];
    if let Ok(crates) = std::fs::read_dir(root.join("crates")) {
        for krate in crates {
            let krate = krate?.path();
            dirs.push(krate.join("src"));
            dirs.push(krate.join("tests"));
        }
    }
    let mut paths = Vec::new();
    for dir in dirs {
        files(&dir, &mut paths)?;
    }
    let mut found = Vec::new();
    for path in paths {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let name = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if name == SKIPPED {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            found.push((name, text));
        }
    }
    found.sort();
    Ok(found)
}

/// Adds each file in `dir` and its subfolders to `paths`. A folder
/// that is not there has no file.
fn files(dir: &Path, paths: &mut Vec<std::path::PathBuf>) -> io::Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            files(&path, paths)?;
        } else {
            paths.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name of the form that the check refuses, built so that this
    /// file names no version.
    fn riff(version: &str) -> String {
        format!("riff {version}")
    }

    fn found(line: &str) -> Vec<String> {
        names(line)
            .into_iter()
            .map(|r| line[r].to_owned())
            .collect()
    }

    #[test]
    fn a_name_has_riff_or_riff_server_a_space_and_a_number() {
        for version in ["1", "3.0", "4.1.1", "10.20"] {
            let name = riff(version);
            assert_eq!(found(&format!("The {name} runs.")), [name.as_str()]);
            assert_eq!(found(&format!("{name}.")), [name]);
            let server = format!("riff-server {version}");
            assert_eq!(found(&format!("({server})")), [server]);
        }
    }

    #[test]
    fn a_number_with_a_letter_after_it_is_not_a_version() {
        assert_eq!(found(&format!("{}.x line", riff("1"))), [riff("1")]);
        assert!(found(&format!("{}x", riff("1"))).is_empty());
        assert!(found(&format!("{}_a", riff("1"))).is_empty());
        assert!(found("riff 1a, riff  2, riff, riff-core 3, riff-3.md").is_empty());
        let (a, b) = (riff("3"), riff("3"));
        assert!(found(&format!("my{a}, the-{b}")).is_empty());
    }

    #[test]
    fn a_sentence_dot_is_not_part_of_the_number() {
        assert_eq!(found(&format!("{}. Next", riff("3"))), [riff("3")]);
        assert_eq!(found(&format!("{}..", riff("3.0"))), [riff("3.0")]);
    }

    #[test]
    fn each_name_of_a_line_counts() {
        let server = format!("riff-server {}", 1);
        let line = format!("{} and {} and {server}", riff("1.5"), riff("3"));
        assert_eq!(found(&line), [riff("1.5"), riff("3"), server]);
    }

    fn file(text: &str) -> Vec<(String, String)> {
        vec![("crates/a/src/a.rs".to_owned(), text.to_owned())]
    }

    #[test]
    fn a_name_that_a_line_break_splits_fails() {
        for (first, second) in [
            ("/// A post of riff", "/// 1.8.1 is not there."),
            ("//! The plugin of riff", "//!    3 is left."),
            ("the riff", "   3.0 runs here."),
            ("> the riff", "> 3.0 runs here."),
            ("\"runs on riff \\", "     3.0 only\""),
            ("// riff-server", "// 4.1 is the server."),
        ] {
            let text = format!("{first}\n{second}\n");
            let errors = check(&file(&text), &[]);
            assert_eq!(errors.len(), 1, "{text:?}: {errors:?}");
            assert!(
                errors[0].text.starts_with("crates/a/src/a.rs:1: "),
                "{errors:?}"
            );
        }
        let text = "the riff\nis here.\nriff\nand 3 more.\n";
        assert!(check(&file(text), &[]).is_empty());
        let text = "let a = \"riff  \\\n      4 sessions\";\n";
        assert!(
            check(&file(text), &[]).is_empty(),
            "two spaces are not a split"
        );
    }

    #[test]
    fn a_text_of_the_allow_list_allows_a_name_that_a_line_break_splits() {
        let text = format!("The plugin of riff\n{} is left.\n", "3");
        let (allow, _) = parse_allow(&format!(
            "crates/a/src/a.rs | plugin of {} | the old plugin\n",
            riff("3")
        ));
        assert!(check(&file(&text), &allow).is_empty());
        let (other, _) = parse_allow(&format!(
            "crates/a/src/a.rs | file of {} | the old file\n",
            riff("3")
        ));
        assert_eq!(check(&file(&text), &other).len(), 2);
    }

    #[test]
    fn a_line_with_a_name_fails_and_names_the_file_and_the_line() {
        let text = format!("fine\n/// {} runs on Linux only.\n", riff("3.0"));
        let errors = check(&file(&text), &[]);
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].to_string(),
            format!(
                "versions: crates/a/src/a.rs:2: the line names \"{}\". Say what riff does, with \
                 no version. A name that must stay is a line of {ALLOW_FILE}: /// {} runs on \
                 Linux only.",
                riff("3.0"),
                riff("3.0")
            )
        );
    }

    #[test]
    fn an_allowed_text_passes_in_its_file_only() {
        let line = format!("The plugin of {} stays.", riff("3"));
        let (allow, problems) = parse_allow(&format!(
            "crates/a/src/a.rs | plugin of {} | the old plugin\n",
            riff("3")
        ));
        assert!(problems.is_empty());
        assert!(check(&file(&line), &allow).is_empty());
        let other = vec![("crates/a/src/b.rs".to_owned(), line)];
        let rules: Vec<_> = check(&other, &allow)
            .iter()
            .map(|e| e.text.clone())
            .collect();
        assert_eq!(
            rules.len(),
            2,
            "the name fails, and the allow line is unused: {rules:?}"
        );
    }

    #[test]
    fn an_allowed_text_does_not_allow_another_name_on_its_line() {
        let line = format!("{} and the plugin of {}.", riff("3.0"), riff("3"));
        let (allow, _) = parse_allow(&format!(
            "crates/a/src/a.rs | plugin of {} | the old plugin\n",
            riff("3")
        ));
        let errors = check(&file(&line), &allow);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].text.contains(&format!("\"{}\"", riff("3.0"))),
            "{errors:?}"
        );
    }

    #[test]
    fn a_text_allows_each_line_that_holds_it() {
        let line = format!("The plugin of {}.", riff("3"));
        let (allow, _) = parse_allow(&format!(
            "crates/a/src/a.rs | plugin of {} | the old plugin\n",
            riff("3")
        ));
        assert!(check(&file(&format!("{line}\n{line}\n")), &allow).is_empty());
    }

    #[test]
    fn a_line_of_the_list_that_allows_nothing_fails() {
        let (allow, _) = parse_allow("crates/a/src/a.rs | a name | a reason\n");
        let errors = check(&file("no name here\n"), &allow);
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].to_string(),
            format!(
                "versions: {ALLOW_FILE}:1: no line of crates/a/src/a.rs has a version name in \
                 \"a name\". Remove the line."
            )
        );
    }

    #[test]
    fn the_list_has_three_parts_in_each_line() {
        let source = "# a comment\n\ncrates/a | text | reason\nonly two | parts\n\
                      crates/a |  | no text\ncrates/a | text | reason | with | pipes\n";
        let (allow, problems) = parse_allow(source);
        assert_eq!(allow.len(), 2);
        assert_eq!(allow[0].at, 3);
        assert_eq!(allow[1].reason, "reason | with | pipes");
        let lines: Vec<_> = problems.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            lines,
            [
                format!("{ALLOW_FILE}:4: the line is not `PATH | TEXT | REASON`"),
                format!("{ALLOW_FILE}:5: the line is not `PATH | TEXT | REASON`"),
            ]
        );
    }

    #[test]
    fn scan_reads_the_folders_of_the_check_and_skips_the_requirements_history() {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in [
            ("crates/a/src/lib.rs", "a"),
            ("crates/a/tests/t/b.rs", "b"),
            ("crates/a/other/c.rs", "not read"),
            ("docs/src/page.md", "p"),
            ("docs/src/requirements.md", "r"),
            ("docs/src/requirements-history.md", "not read"),
            ("docs/book.toml", "not read"),
        ] {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        std::fs::write(root.path().join("docs/src/bytes.bin"), [0xff, 0xfe]).unwrap();
        let paths: Vec<_> = scan(root.path())
            .unwrap()
            .into_iter()
            .map(|f| f.0)
            .collect();
        assert_eq!(
            paths,
            [
                "crates/a/src/lib.rs",
                "crates/a/tests/t/b.rs",
                "docs/src/page.md",
                "docs/src/requirements.md"
            ]
        );
    }
}
