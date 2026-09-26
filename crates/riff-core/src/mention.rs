//! Finds the mentions in a message body (R51).
//!
//! # Rules
//!
//! - A mention is `@` followed by a name. The `@` starts the text or
//!   follows a character that cannot be part of a name, so the `@`
//!   inside `mike@pangolin` does not start a mention.
//! - A name is a run of letters, digits and `_ - . # : / @ ~`. A `.` or
//!   `:` at the end is punctuation, not part of the name.
//! - Markdown around a name does not matter: `**@name**`, `(@name)`,
//!   `"@name"` and `@name's` are all mentions.
//! - Text in code is not a mention: an inline code span in backticks,
//!   or a line in a fenced code block. So a message can quote a name
//!   without waking it.
//!
//! The server matches each mention against the known session names.
//!
//! # Example
//!
//! ```
//! use riff_core::mention::mentions;
//!
//! let body = "**@brett@heron:riff#tests**, see (@mike@pangolin:riff#api). \
//!             Not `@quoted@host:riff`, and not mail@example.com.";
//! assert_eq!(
//!     mentions(body),
//!     ["brett@heron:riff#tests", "mike@pangolin:riff#api"]
//! );
//! ```

/// The text after each `@` that starts a mention, outside code.
pub fn mentions(body: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut fenced = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        // Even segments are outside backticks; odd segments are code.
        for text in line.split('`').step_by(2) {
            scan(text, &mut found);
        }
    }
    found
}

fn scan<'a>(text: &'a str, found: &mut Vec<&'a str>) {
    let mut prev: Option<char> = None;
    for (i, c) in text.char_indices() {
        if c == '@' && !prev.is_some_and(is_name_char) {
            let rest = &text[i + 1..];
            let end = rest.find(|c| !is_name_char(c)).unwrap_or(rest.len());
            let name = rest[..end].trim_end_matches(['.', ':']);
            if !name.is_empty() {
                found.push(name);
            }
        }
        prev = Some(c);
    }
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || "_-.#:/@~".contains(c)
}

#[cfg(test)]
mod tests {
    use super::mentions;

    #[test]
    fn markdown_around_a_name_still_mentions() {
        for body in [
            "@a@b:r",
            "**@a@b:r**",
            "(@a@b:r)",
            "\"@a@b:r\"",
            "@a@b:r's work",
            "@a@b:r, hi",
            "@a@b:r.",
            "@a@b:r: hi",
        ] {
            assert_eq!(mentions(body), ["a@b:r"], "{body}");
        }
    }

    #[test]
    fn code_is_not_a_mention() {
        assert!(mentions("see `@a@b:r`").is_empty());
        assert!(mentions("```\n@a@b:r\n```").is_empty());
        assert_eq!(mentions("```\n@x@y:r\n```\n@a@b:r"), ["a@b:r"]);
    }

    #[test]
    fn an_at_inside_a_word_is_not_a_mention() {
        assert!(mentions("mail@example.com").is_empty());
        assert!(mentions("meet @ 5pm").is_empty());
    }

    #[test]
    fn a_full_name_is_one_mention() {
        assert_eq!(mentions("@riff://a@b/o/r#w now"), ["riff://a@b/o/r#w"]);
    }
}
