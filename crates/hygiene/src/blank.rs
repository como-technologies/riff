//! The blank check: `hygiene blank PATH...` fails when a Markdown file
//! has two blank lines in a row.
//!
//! # Design
//!
//! One blank line ends a paragraph. A second blank line adds nothing,
//! and a diff that adds it is noise. So no Markdown file of the book or
//! of the plugin has two blank lines in a row
//! (01M51K1VMWTNZGD6PT1BYDGWM3). `just wrap` runs `hygiene blank
//! docs/src crates/riff/claude-plugin`: each `.md` file in each folder
//! and its subfolders. The skill is not in the wrap check: its front
//! matter is one long line.
//!
//! The rule `blank` finds each blank line after a blank line. It skips
//! a code block: the text of a block is an example, as it is. A line
//! with only spaces is blank.
//!
//! ```
//! let errors = hygiene::blank::check("a.md", "# A\n\n\nText.\n");
//! assert_eq!(errors.len(), 1);
//! assert_eq!(errors[0].rule, "blank");
//! assert_eq!(errors[0].text, "a.md:3: a second blank line. Remove it.");
//!
//! let block = "# A\n\n```text\none\n\n\ntwo\n```\n";
//! assert!(hygiene::blank::check("a.md", block).is_empty());
//! ```

use crate::Error;

/// The number of each blank line of `text` that follows a blank line,
/// outside a code block. Each number counts from 1.
pub fn second_blank_lines(text: &str) -> Vec<usize> {
    let mut code = false;
    let mut blank = false;
    let mut found = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            code = !code;
        }
        let is_blank = !code && trimmed.is_empty();
        if is_blank && blank {
            found.push(n + 1);
        }
        blank = is_blank;
    }
    found
}

/// The errors of the Markdown file `name` with the content `text`: one
/// for each line of [`second_blank_lines`].
pub fn check(name: &str, text: &str) -> Vec<Error> {
    second_blank_lines(text)
        .into_iter()
        .map(|n| {
            Error::new(
                "blank",
                format!("{name}:{n}: a second blank line. Remove it."),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_blank_line_after_a_blank_line_counts() {
        assert_eq!(second_blank_lines("a\n\nb\n"), Vec::<usize>::new());
        assert_eq!(second_blank_lines("a\n\n\nb\n"), [3]);
        assert_eq!(second_blank_lines("a\n\n\n\nb\n"), [3, 4]);
        assert_eq!(second_blank_lines("a\n\n  \nb\n"), [3]);
        assert_eq!(second_blank_lines("\n\na\n"), [2]);
    }

    #[test]
    fn a_code_block_keeps_its_blank_lines() {
        let text = "a\n\n```sh\nx\n\n\ny\n```\n\nb\n";
        assert_eq!(second_blank_lines(text), Vec::<usize>::new());
        let indented = "1. a\n\n   ```sh\n   x\n\n\n   ```\n";
        assert_eq!(second_blank_lines(indented), Vec::<usize>::new());
    }

    #[test]
    fn a_blank_line_after_a_code_block_and_a_blank_line_counts() {
        assert_eq!(second_blank_lines("```\nx\n```\n\n\n# B\n"), [5]);
    }
}
