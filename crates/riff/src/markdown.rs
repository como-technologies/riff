//! The Markdown that riff reads in a text of a person or of a session: a
//! heading, a code block and a list mark. The "Done when" part of an
//! issue ([`crate::rollout`]), a member file of the council
//! ([`crate::council`]), and the parts `## The council` and `## New
//! findings` of the result of a verify ([`crate::council::result`],
//! [`crate::council::found`]) read their lines with these rules
//! (01M7D11KGEJ8ACFJK6Y10WENJZ, 01M7GDQWK92WWTTE361JGWDVGY).
//!
//! ```
//! use riff::markdown::{heading, headings, item};
//!
//! assert_eq!(heading("## The council"), Some((2, "The council")));
//! assert_eq!(item("1. docs: no finding."), "docs: no finding.");
//! let text = "# A\n\n```sh\n# run it\n```\n## B\n";
//! let found: Vec<_> = headings(text.lines()).filter_map(|(_, h)| h).collect();
//! assert_eq!(found, [(1, "A"), (2, "B")]);
//! ```

/// The level and the text of the heading `line`: after the spaces, one to
/// six `#` and a space. `(2, "The council")` for `## The council`. `None`
/// for a line that is no heading.
///
/// ```
/// use riff::markdown::heading;
///
/// assert_eq!(heading("### Done when: "), Some((3, "Done when:")));
/// assert_eq!(heading("  # A"), Some((1, "A")));
/// assert_eq!(heading("## "), Some((2, "")));
/// assert_eq!(heading("##The council"), None);
/// assert_eq!(heading("####### Seven"), None);
/// assert_eq!(heading("- design"), None);
/// ```
pub fn heading(line: &str) -> Option<(usize, &str)> {
    let line = line.trim_start();
    let text = line.trim_start_matches('#');
    let marks = line.len() - text.len();
    match text.strip_prefix(' ') {
        Some(text) if (1..=6).contains(&marks) => Some((marks, text.trim())),
        _ => None,
    }
}

/// Each line of `lines`, with its heading ([`heading`]). A line in a code
/// block, from a line that starts with ```` ``` ```` to the next one, is no
/// heading.
///
/// ```
/// use riff::markdown::headings;
///
/// let text = "## A\n```\n## B\n```\ntext";
/// let found: Vec<_> = headings(text.lines()).collect();
/// assert_eq!(
///     found,
///     [("## A", Some((2, "A"))), ("```", None), ("## B", None), ("```", None), ("text", None)]
/// );
/// ```
pub fn headings<'a>(
    lines: impl IntoIterator<Item = &'a str>,
) -> impl Iterator<Item = (&'a str, Option<(usize, &'a str)>)> {
    blocks(lines).map(|(line, found, _)| (line, found))
}

/// Each line of `lines`, with its heading ([`heading`]), and `true` for a
/// line of a code block: a line that starts with ```` ``` ````, the next
/// such line, and each line between them. A line of a code block is no
/// heading.
fn blocks<'a>(
    lines: impl IntoIterator<Item = &'a str>,
) -> impl Iterator<Item = (&'a str, Option<(usize, &'a str)>, bool)> {
    let mut fenced = false;
    lines.into_iter().map(move |line| {
        let fence = line.trim_start().starts_with("```");
        let code = fenced || fence;
        if fence {
            fenced = !fenced;
        }
        (line, heading(line).filter(|_| !code), code)
    })
}

/// Where a line of a text is, as [`parts`] gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Out of each part, or the heading of a part.
    Out,
    /// In a part, out of a code block.
    Text,
    /// In a part, in a code block.
    Code,
}

/// Each line of `lines`, with its [`Place`] in the parts `start` of the
/// text, for example `## The council`. A part runs from the line after a
/// heading `start`, of that level and text, to the next heading of that
/// level or fewer `#`. A line in a code block is no heading
/// ([`headings`]), and each such part counts. A `start` that is no
/// heading has no part.
///
/// ```
/// use riff::markdown::{Place, parts};
///
/// let text = "## B\n- one\n```\n## C\n```\n### D\n## E\n- two\n## B\n- three";
/// let places: Vec<_> = parts(text.lines(), "## B").collect();
/// assert_eq!(
///     places,
///     [
///         ("## B", Place::Out), ("- one", Place::Text), ("```", Place::Code),
///         ("## C", Place::Code), ("```", Place::Code), ("### D", Place::Text),
///         ("## E", Place::Out), ("- two", Place::Out), ("## B", Place::Out),
///         ("- three", Place::Text),
///     ]
/// );
/// assert!(parts(text.lines(), "B").all(|(_, place)| place == Place::Out));
/// ```
pub fn parts<'a>(
    lines: impl IntoIterator<Item = &'a str>,
    start: &str,
) -> impl Iterator<Item = (&'a str, Place)> {
    let start = heading(start);
    let mut inside = false;
    blocks(lines).map(move |(line, found, code)| {
        let place = match (found, start) {
            (Some(found), Some(start)) if found == start => {
                inside = true;
                Place::Out
            }
            (Some((level, _)), Some((top, _))) if level <= top => {
                inside = false;
                Place::Out
            }
            _ if !inside => Place::Out,
            _ if code => Place::Code,
            _ => Place::Text,
        };
        (line, place)
    })
}

/// The text of the line `line` after its list mark: after the spaces, a
/// `-`, `*` or `+`, or a number with `.` or `)`, and the spaces after the
/// mark. A line with no list mark loses only its spaces.
///
/// ```
/// use riff::markdown::item;
///
/// assert_eq!(item("  - docs"), "docs");
/// assert_eq!(item("* **docs**"), "**docs**");
/// assert_eq!(item("12) docs"), "docs");
/// assert_eq!(item("3. docs"), "docs");
/// assert_eq!(item("2fa"), "2fa");
/// assert_eq!(item(" docs"), "docs");
/// ```
pub fn item(line: &str) -> &str {
    let line = line.trim_start();
    after_mark(line).unwrap_or(line).trim_start()
}

/// The text of the list item `line`, as [`item`] gives it, when a space,
/// a tab or the end of the line comes after its list mark. `None` for
/// each other line: a line with no list mark, and a line such as
/// `**Bold.**` or `---`.
///
/// ```
/// use riff::markdown::list_item;
///
/// assert_eq!(list_item("  - docs\n"), Some("docs\n"));
/// assert_eq!(list_item("10)\tdocs"), Some("docs"));
/// assert_eq!(list_item("-"), Some(""));
/// assert_eq!(list_item("**None.**"), None);
/// assert_eq!(list_item("---"), None);
/// assert_eq!(list_item("1.5 docs"), None);
/// assert_eq!(list_item("docs"), None);
/// ```
pub fn list_item(line: &str) -> Option<&str> {
    let rest = after_mark(line.trim_start())?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim_start())
}

/// The text of `line` right after its list mark, with the spaces after
/// the mark. `line` has no spaces before the mark. `None` for a line with
/// no list mark.
fn after_mark(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix(['-', '*', '+']) {
        return Some(rest);
    }
    let unnumbered = line.trim_start_matches(|c: char| c.is_ascii_digit());
    match unnumbered.len() < line.len() {
        true => unnumbered.strip_prefix(['.', ')']),
        false => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A heading is one to six `#` after the spaces, then a space. The
    /// text has no spaces around it.
    #[test]
    fn a_heading_has_one_to_six_marks_and_a_space() {
        assert_eq!(heading("## The council  "), Some((2, "The council")));
        assert_eq!(heading("###### Six"), Some((6, "Six")));
        assert_eq!(heading("\t## Tab"), Some((2, "Tab")));
        assert_eq!(heading("#"), None);
        assert_eq!(heading("#\tTab"), None);
        assert_eq!(heading("text # not"), None);
    }

    /// A code block ends at the next fence, also when the first fence
    /// names a language. A fence after a list mark is no fence.
    #[test]
    fn a_heading_in_a_code_block_is_no_heading() {
        let text = "# A\n  ```rust\n# no\n  ```\n# B\n- ```\n# no\n- ```\n";
        let found: Vec<_> = headings(text.lines()).filter_map(|(_, h)| h).collect();
        assert_eq!(found, [(1, "A"), (1, "B"), (1, "no")]);
    }

    /// A list mark is `-`, `*`, `+`, or a number with `.` or `)`. A
    /// number with no such mark stays.
    #[test]
    fn a_list_mark_is_a_sign_or_a_number_with_a_dot_or_a_parenthesis() {
        for line in [
            "- a b",
            "* a b",
            "+ a b",
            "1. a b",
            "10) a b",
            "   7.   a b",
        ] {
            assert_eq!(item(line), "a b", "{line}");
        }
        assert_eq!(item("2026 a"), "2026 a");
        assert_eq!(item("1,5 a"), "1,5 a");
        assert_eq!(item(""), "");
    }
}
