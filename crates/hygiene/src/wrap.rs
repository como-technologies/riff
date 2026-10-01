//! The wrap check: `hygiene wrap [DIR]` fails when a prose line of a
//! Markdown file in DIR is longer than 72 characters.
//!
//! # Design
//!
//! Each page of the book and the requirements wraps at [`WIDTH`]
//! (01M3MNT28NXA9VRST119QR8AQK). `just wrap` runs `hygiene wrap
//! docs/src`, and `just ci` runs it for each diff, also for a diff of
//! only text (01M3WNN836EFG7GJQKZRTSK5FT). So the check is not a test
//! of a crate: a change of the book needs no build of riff.
//!
//! The rule `wrap` finds only a line that a wrap can make shorter. It
//! skips:
//!
//! - a line in a code block, a row of a table, and a heading;
//! - a line with one word after its list marker. A link and a code
//!   span are each one word, also with the text that follows them with
//!   no space.
//!
//! ```
//! let prose = "word ".repeat(15);
//! let errors = hygiene::wrap::check("a.md", &prose);
//! assert_eq!(errors.len(), 1);
//! assert_eq!(errors[0].rule, "wrap");
//! assert!(errors[0].text.starts_with("a.md:1: the line has 75 characters"));
//!
//! let link = format!("- [{}](page.md#part).", "a b ".repeat(20));
//! assert!(hygiene::wrap::check("a.md", &link).is_empty());
//! ```

use crate::Error;

/// The wrap width of the book.
pub const WIDTH: usize = 72;

/// The words of `line` that a wrap can put on a new line. A link and a
/// code span are each one word, also with the text that follows them
/// with no space.
fn words(line: &str) -> usize {
    let mut count = 0;
    let mut rest = line.trim();
    while !rest.is_empty() {
        let end = if rest.starts_with('[') {
            rest.find("](")
                .and_then(|i| rest[i..].find(')').map(|j| i + j + 1))
        } else if let Some(code) = rest.strip_prefix('`') {
            code.find('`').map(|i| i + 2)
        } else {
            None
        }
        .unwrap_or(0);
        let end = end + rest[end..].find(' ').unwrap_or(rest.len() - end);
        count += 1;
        rest = rest[end..].trim_start();
    }
    count
}

/// The text of `line` after its list marker, `- ` or `1. `.
fn item_text(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix("- ") {
        return rest;
    }
    match line.split_once(". ") {
        Some((n, rest)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => rest,
        _ => line,
    }
}

/// The lines of `text` that are over [`WIDTH`] and that a wrap can
/// make shorter: prose outside a code block, a table and a heading.
/// Each line comes with its number, from 1.
pub fn long_lines(text: &str) -> Vec<(usize, &str)> {
    let mut code = false;
    let mut long = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            code = !code;
            continue;
        }
        if code || trimmed.starts_with('|') || trimmed.starts_with('#') {
            continue;
        }
        if line.chars().count() > WIDTH && words(item_text(trimmed)) > 1 {
            long.push((n + 1, line));
        }
    }
    long
}

/// The errors of the Markdown file `name` with the content `text`: one
/// for each line of [`long_lines`].
pub fn check(name: &str, text: &str) -> Vec<Error> {
    long_lines(text)
        .into_iter()
        .map(|(n, line)| {
            Error::new(
                "wrap",
                format!(
                    "{name}:{n}: the line has {} characters. Wrap it at {WIDTH}: {line}",
                    line.chars().count()
                ),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_and_a_code_span_are_one_word() {
        assert_eq!(words("[Start a Riff](start-a-riff.md#you-need)."), 1);
        assert_eq!(words("`riff://USER@HOST/-?session=ID`."), 1);
        assert_eq!(words("see [Join a Riff](join-a-riff.md) now"), 3);
    }

    #[test]
    fn only_prose_that_a_wrap_can_break_is_long() {
        let over = "word ".repeat(15);
        let link = format!("[{}](page.md)", "a b ".repeat(20));
        let text = format!("{over}\n```sh\n{over}\n```\n| {over} |\n# {over}\n{link}\n1. {link}\n");
        assert_eq!(long_lines(&text), vec![(1, over.as_str())]);
    }

    #[test]
    fn a_line_of_72_characters_passes_and_73_fails() {
        let at = format!("{} b", "a".repeat(70));
        assert_eq!(at.chars().count(), WIDTH);
        assert!(check("a.md", &at).is_empty());
        let over = format!("{} b", "a".repeat(71));
        let errors = check("a.md", &format!("ok\n{over}\n"));
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].to_string(),
            format!("wrap: a.md:2: the line has 73 characters. Wrap it at 72: {over}")
        );
    }

    #[test]
    fn the_width_counts_characters_not_bytes() {
        let line = format!("{} é", "é".repeat(70));
        assert!(line.len() > WIDTH);
        assert!(check("a.md", &line).is_empty());
    }
}
