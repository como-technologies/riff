//! The Markdown that riff reads in a text of a person or of a session: a
//! heading, a code block and a list mark. The "Done when" part of an
//! issue ([`crate::rollout`]), a member file of the council
//! ([`crate::council`]) and the part `## The council` of the result of a
//! verify ([`crate::council::result`]) read their lines with these rules
//! (01M7D11KGEJ8ACFJK6Y10WENJZ).
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
    let mut fenced = false;
    lines.into_iter().map(move |line| {
        let found = heading(line).filter(|_| !fenced);
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        (line, found)
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
    let unnumbered = line.trim_start_matches(|c: char| c.is_ascii_digit());
    let rest = match line.strip_prefix(['-', '*', '+']) {
        Some(rest) => rest,
        None if unnumbered.len() < line.len() => {
            unnumbered.strip_prefix(['.', ')']).unwrap_or(line)
        }
        None => line,
    };
    rest.trim_start()
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
