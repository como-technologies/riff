//! The check of the tables of shared surfaces: the threat model of the
//! sandbox in the rustdoc of `riff::confine` and in the book.
//!
//! # Design
//!
//! A shared surface is a thing that a session writes or asks, and that
//! a process outside each sandbox reads or runs
//! (01M4DDZ8B3QBZATBPNHVFDH7BR). Each one is a row of a table, in the
//! rustdoc for agents and in the book for people
//! (01M4DDZ8DFY1SP1AVVRSD0DRV4). An HTML comment marks each table, so
//! the check finds it in the two texts:
//!
//! | Marker | Table | Its last column |
//! |---|---|---|
//! | `<!-- surfaces -->` | each surface with a control | the tests of the control, each in backticks |
//! | `<!-- open-surfaces -->` | each surface with no control yet | the decision: an issue `#N`, or `Accept` |
//!
//! The table ends at `<!-- /surfaces -->` or `<!-- /open-surfaces -->`.
//! [`check`] finds these faults:
//!
//! | Rule | Finds |
//! |---|---|
//! | `surfaces` | A text with no table, or two texts with a different row. |
//! | `surface-test` | A row with a control that names no test, or a test that no source file has. |
//! | `surface-decision` | A row with no control and no issue or accept. |
//!
//! ```
//! let doc = "\
//! <!-- surfaces -->
//! | Surface | Writes | Outside | Control | Test |
//! |---|---|---|---|---|
//! | The target | its target | the broker | from the broker | `a_test_run_writes_no_folder` |
//! <!-- /surfaces -->
//! <!-- open-surfaces -->
//! | Surface | Writes | Outside | Risk | Decision |
//! |---|---|---|---|---|
//! | The state | a file | the wrapper | a stop | #630 |
//! <!-- /open-surfaces -->
//! ";
//! let tests = hygiene::surfaces::test_names("fn a_test_run_writes_no_folder() {}");
//! assert!(hygiene::surfaces::check(doc, doc, &tests).is_empty());
//!
//! let none = hygiene::surfaces::test_names("");
//! let errors = hygiene::surfaces::check(doc, doc, &none);
//! assert_eq!(errors[0].rule, "surface-test");
//! ```

use std::collections::BTreeSet;

use crate::Error;

/// The marker of the table of the surfaces with a control.
pub const SURFACES: &str = "surfaces";

/// The marker of the table of the surfaces with no control yet.
pub const OPEN: &str = "open-surfaces";

/// The text of the rustdoc of a module: each `//!` line with no
/// `//! ` before it.
///
/// ```
/// let source = "//! # Design\n//!\n//! | a |\nuse std::fs;\n";
/// assert_eq!(hygiene::surfaces::doc_of(source), "# Design\n\n| a |\n");
/// ```
pub fn doc_of(source: &str) -> String {
    source
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("//!"))
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .fold(String::new(), |text, line| text + line + "\n")
}

/// The rows of the table between `<!-- MARKER -->` and
/// `<!-- /MARKER -->` in `text`, with no head and no rule line. Each row
/// is its cells, each with no space at its ends. `None` when `text` has
/// no such table.
///
/// ```
/// let text = "<!-- t -->\n| a | b |\n|---|---|\n| 1 |  2 |\n<!-- /t -->\n";
/// assert_eq!(hygiene::surfaces::rows(text, "t"), Some(vec![vec!["1".into(), "2".into()]]));
/// assert_eq!(hygiene::surfaces::rows(text, "u"), None);
/// ```
pub fn rows(text: &str, marker: &str) -> Option<Vec<Vec<String>>> {
    let start = text.find(&format!("<!-- {marker} -->"))?;
    let rest = &text[start..];
    let end = rest.find(&format!("<!-- /{marker} -->"))?;
    let rows = rest[..end]
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('|'))
        .skip(2)
        .map(cells)
        .collect();
    Some(rows)
}

fn cells(line: &str) -> Vec<String> {
    let inner = line.trim_matches('|');
    inner.split(" | ").map(|c| c.trim().to_owned()).collect()
}

/// The name of each function in `sources`: the word after each `fn `.
///
/// ```
/// let names = hygiene::surfaces::test_names("#[test]\nfn a_b() {}\npub fn c<T>(t: T) {}");
/// assert!(names.contains("a_b") && names.contains("c"));
/// ```
pub fn test_names(sources: &str) -> BTreeSet<String> {
    sources
        .split("fn ")
        .skip(1)
        .filter_map(|rest| {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

/// The tests that `cell` names: each code span, by its last `::` part.
///
/// ```
/// let cell = "`confine::a_b`, `c_d` and the Gate";
/// assert_eq!(hygiene::surfaces::tests_of(cell), ["a_b", "c_d"]);
/// ```
pub fn tests_of(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .filter_map(|span| span.rsplit("::").next())
        .map(str::to_owned)
        .collect()
}

/// Whether `cell` is a decision: it names an issue `#N`, or it starts
/// with `Accept`.
///
/// ```
/// use hygiene::surfaces::is_decision;
///
/// assert!(is_decision("#630"));
/// assert!(is_decision("Accept: a stop of a worker only"));
/// assert!(!is_decision("later"));
/// assert!(!is_decision("# 1"));
/// ```
pub fn is_decision(cell: &str) -> bool {
    let issue = cell
        .split('#')
        .skip(1)
        .any(|rest| rest.starts_with(|c: char| c.is_ascii_digit()));
    issue || cell.starts_with("Accept")
}

/// The faults of the tables of shared surfaces in `doc` (the rustdoc of
/// `riff::confine`) and `book` (the book page), with `tests` the name of
/// each function of the sources. Empty when each rule holds.
pub fn check(doc: &str, book: &str, tests: &BTreeSet<String>) -> Vec<Error> {
    let mut errors = Vec::new();
    for marker in [SURFACES, OPEN] {
        let (Some(of_doc), Some(of_book)) = (rows(doc, marker), rows(book, marker)) else {
            errors.push(Error::new(
                "surfaces",
                format!("the rustdoc and the book each need the table <!-- {marker} -->"),
            ));
            continue;
        };
        if of_doc.is_empty() && marker == SURFACES {
            errors.push(Error::new("surfaces", "the table of surfaces has no row"));
        }
        for row in of_doc.iter().filter(|r| !of_book.contains(r)) {
            errors.push(Error::new(
                "surfaces",
                format!("the book has no row \"{}\" of the rustdoc", row[0]),
            ));
        }
        for row in of_book.iter().filter(|r| !of_doc.contains(r)) {
            errors.push(Error::new(
                "surfaces",
                format!("the rustdoc has no row \"{}\" of the book", row[0]),
            ));
        }
        for row in &of_doc {
            let last = row.last().map(String::as_str).unwrap_or_default();
            if marker == OPEN {
                if !is_decision(last) {
                    errors.push(Error::new(
                        "surface-decision",
                        format!("\"{}\" needs an issue #N or an accept", row[0]),
                    ));
                }
                continue;
            }
            let named = tests_of(last);
            if named.is_empty() {
                errors.push(Error::new(
                    "surface-test",
                    format!("\"{}\" names no test of its control", row[0]),
                ));
            }
            for test in named.iter().filter(|t| !tests.contains(*t)) {
                errors.push(Error::new(
                    "surface-test",
                    format!("\"{}\" names the test {test}, and no source has it", row[0]),
                ));
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(marker: &str, rows: &[&str]) -> String {
        let mut text = format!("<!-- {marker} -->\n| a | b |\n|---|---|\n");
        for row in rows {
            text += &format!("{row}\n");
        }
        text + &format!("<!-- /{marker} -->\n")
    }

    fn rules(doc: &str, book: &str, tests: &[&str]) -> Vec<&'static str> {
        let tests = tests.iter().map(|t| (*t).to_owned()).collect();
        check(doc, book, &tests).iter().map(|e| e.rule).collect()
    }

    fn texts(surfaces: &[&str], open: &[&str]) -> String {
        table(SURFACES, surfaces) + &table(OPEN, open)
    }

    #[test]
    fn two_equal_tables_with_real_tests_pass() {
        let text = texts(&["| Git | `x::a_b` |"], &["| State | #630 |"]);
        assert!(rules(&text, &text, &["a_b"]).is_empty());
    }

    #[test]
    fn a_text_with_no_table_fails() {
        let text = texts(&["| Git | `a_b` |"], &[]);
        assert_eq!(
            rules(&text, &table(SURFACES, &["| Git | `a_b` |"]), &["a_b"]),
            ["surfaces"]
        );
        assert_eq!(rules("", "", &[]), ["surfaces", "surfaces"]);
    }

    #[test]
    fn a_row_in_one_text_only_fails() {
        let doc = texts(&["| Git | `a_b` |", "| Tmux | `a_b` |"], &[]);
        let book = texts(&["| Git | `a_b` |"], &["| State | #1 |"]);
        assert_eq!(rules(&doc, &book, &["a_b"]), ["surfaces", "surfaces"]);
    }

    #[test]
    fn a_control_with_no_test_or_a_test_that_is_not_there_fails() {
        let text = texts(&["| Git | the Gate |", "| Tmux | `gone` |"], &[]);
        assert_eq!(
            rules(&text, &text, &["a_b"]),
            ["surface-test", "surface-test"]
        );
    }

    #[test]
    fn an_open_surface_with_no_decision_fails() {
        let text = texts(&["| Git | `a_b` |"], &["| State | later |"]);
        assert_eq!(rules(&text, &text, &["a_b"]), ["surface-decision"]);
    }
}
