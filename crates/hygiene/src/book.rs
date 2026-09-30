//! The book check: `hygiene book [DIR]` builds the book and fails when
//! it has a missing include or anchor.
//!
//! # Design
//!
//! mdbook does not fail on a bad include. For a missing file it prints
//! an `ERROR` line, leaves the include line in the text, and exits 0.
//! For a missing anchor it prints nothing and renders an empty code
//! block. So `hygiene book` runs `mdbook build`, then checks the log and
//! each page (01M3T5MG7VVYG4A8YBM75FGSMB):
//!
//! | Rule | Finds |
//! |---|---|
//! | `mdbook` | An `ERROR` line in the log of mdbook. |
//! | `include` | An include line, `{{#...}}`, that mdbook left in the text of a page, outside a code block. |
//! | `empty-code` | A code block with no text. A missing anchor gives one. |
//!
//! An example of an include in a code block of the book is escaped, as
//! `\{{#include file.rs:name}}`. mdbook shows it as text in the code
//! block. The `include` rule skips each code block, so it skips the
//! example. mdbook also leaves a missing file of a code block as text in
//! the block, so only the `ERROR` line shows that one.
//!
//! ```
//! let page = "<p>See {{#include gone.rs}}.</p>\n<pre><code></code></pre>";
//! let rules: Vec<_> = hygiene::book::check_page("a.html", page)
//!     .into_iter()
//!     .map(|e| e.rule)
//!     .collect();
//! assert_eq!(rules, ["include", "empty-code"]);
//!
//! let example = "<pre><code class=\"language-text\">{{#include a.rs:x}}\n</code></pre>";
//! assert!(hygiene::book::check_page("a.html", example).is_empty());
//! ```

use crate::Error;

/// Regions of a page that the `include` rule skips: code, and the
/// scripts and styles of the theme.
const SKIPS: [(&str, &str); 4] = [
    ("<pre", "</pre>"),
    ("<code", "</code>"),
    ("<script", "</script>"),
    ("<style", "</style>"),
];

/// An `mdbook` error for each `ERROR` line of the log of mdbook.
///
/// ```
/// let log = " INFO Book building has started\n\
///            ERROR Error updating \"{{#include gone.rs}}\", Could not read file\n\
///             WARN Caused By: No such file or directory (os error 2)\n";
/// let errors = hygiene::book::check_log(log);
/// assert_eq!(errors.len(), 1);
/// assert_eq!(errors[0].rule, "mdbook");
/// assert!(errors[0].text.starts_with("ERROR Error updating"));
/// ```
pub fn check_log(log: &str) -> Vec<Error> {
    log.lines()
        .map(|line| strip_ansi(line.trim()))
        .filter(|line| line.starts_with("ERROR"))
        .map(|line| Error::new("mdbook", line))
        .collect()
}

/// The `include` and `empty-code` errors of one page of the book.
/// `name` names the page in each error, with the line.
pub fn check_page(name: &str, html: &str) -> Vec<Error> {
    let mut errors = Vec::new();
    let mut at = 0;
    loop {
        let rest = &html[at..];
        let include = rest.find("{{#");
        let skip = SKIPS
            .iter()
            .filter_map(|&(open, close)| tag_start(rest, open).map(|i| (i, open, close)))
            .min_by_key(|&(i, ..)| i);
        match (include, skip) {
            (Some(i), skip) if skip.is_none_or(|(s, ..)| i < s) => {
                let text = rest[i..].lines().next().unwrap_or_default();
                errors.push(Error::new(
                    "include",
                    format!(
                        "{name}:{}: mdbook left an include line in the text: {}",
                        line_of(html, at + i),
                        text.trim()
                    ),
                ));
                at += i + 3;
            }
            (_, Some((s, open, close))) => {
                let end = rest[s..].find(close).map_or(rest.len(), |e| s + e);
                if open == "<pre" && is_empty(&rest[s..end]) {
                    errors.push(Error::new(
                        "empty-code",
                        format!(
                            "{name}:{}: a code block has no text. Check the anchor of its include.",
                            line_of(html, at + s)
                        ),
                    ));
                }
                at += (end + close.len()).min(rest.len());
            }
            _ => return errors,
        }
    }
}

/// The start of the first tag `open` in `text`: `open` and then `>` or
/// a space, so that `<pre` does not match `<preview`.
fn tag_start(text: &str, open: &str) -> Option<usize> {
    text.match_indices(open)
        .find(|&(i, _)| {
            text[i + open.len()..]
                .chars()
                .next()
                .is_some_and(|c| c == '>' || c.is_whitespace())
        })
        .map(|(i, _)| i)
}

/// True when a `<pre>` block, from `<pre` to before `</pre>`, has no
/// text. The hidden lines of a Rust block (`<span class="boring">`) and
/// the tags do not count.
fn is_empty(block: &str) -> bool {
    let Some(open_end) = block.find('>') else {
        return true;
    };
    let mut body = block[open_end + 1..].to_owned();
    while let Some(s) = body.find("<span class=\"boring\">") {
        let e = body[s..].find("</span>").map_or(body.len(), |e| s + e + 7);
        body.replace_range(s..e, "");
    }
    let mut in_tag = false;
    !body.chars().any(|c| match c {
        '<' => {
            in_tag = true;
            false
        }
        '>' => {
            in_tag = false;
            false
        }
        c => !in_tag && !c.is_whitespace(),
    })
}

/// The line number of byte `at` in `text`, from 1.
fn line_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

/// `line` with no ANSI color codes.
fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(html: &str) -> Vec<&'static str> {
        check_page("a.html", html)
            .into_iter()
            .map(|e| e.rule)
            .collect()
    }

    #[test]
    fn an_include_in_the_text_fails_with_its_line() {
        let errors = check_page(
            "a.html",
            "<p>one</p>\n<p>Text {{#include gone.rs}} here.</p>",
        );
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].rule, "include");
        assert!(
            errors[0].text.starts_with("a.html:2: "),
            "{}",
            errors[0].text
        );
        assert!(errors[0].text.contains("{{#include gone.rs}}"));
    }

    #[test]
    fn each_include_in_the_text_fails() {
        assert_eq!(
            rules("<p>{{#include a.rs}}</p><p>{{#rustdoc_include b.rs}}</p>"),
            ["include", "include"]
        );
    }

    #[test]
    fn an_include_in_code_passes() {
        assert!(
            rules("<pre><code class=\"language-text\">{{#include a.rs:x}}\n</code></pre>")
                .is_empty()
        );
        assert!(rules("<p>Use <code>{{#include a.rs}}</code>.</p>").is_empty());
    }

    #[test]
    fn an_include_after_code_fails() {
        assert_eq!(
            rules("<pre><code>x</code></pre><p>{{#include a.rs}}</p>"),
            ["include"]
        );
    }

    #[test]
    fn a_script_or_a_style_is_skipped() {
        assert!(rules("<script>let a = \"{{#x}}\";</script><style>a{}</style>").is_empty());
    }

    #[test]
    fn an_empty_rust_block_fails() {
        let html = "<pre class=\"playground\"><code class=\"language-rust\"><span class=\"boring\">#![allow(unused)]\n\
                    </span><span class=\"boring\">fn main() {\n</span>\n<span class=\"boring\">}</span></code></pre>";
        assert_eq!(rules(html), ["empty-code"]);
    }

    #[test]
    fn an_empty_text_block_fails() {
        assert_eq!(
            rules("<pre><code class=\"language-text\">\n</code></pre>"),
            ["empty-code"]
        );
        assert_eq!(rules("<pre class=\"mermaid\"></pre>"), ["empty-code"]);
    }

    #[test]
    fn a_block_with_text_passes() {
        let html = "<pre class=\"playground\"><code class=\"language-rust\"><span class=\"boring\">fn main() {\n</span>let a = 1;\n<span class=\"boring\">}</span></code></pre>";
        assert!(rules(html).is_empty());
        assert!(rules("<pre class=\"mermaid\">flowchart LR\n a --> b</pre>").is_empty());
    }

    #[test]
    fn a_tag_that_starts_like_pre_is_not_code() {
        assert_eq!(rules("<preview>{{#include a.rs}}</preview>"), ["include"]);
    }

    #[test]
    fn an_open_block_with_no_end_stops_the_scan() {
        assert!(rules("<pre><code>x").is_empty());
    }

    #[test]
    fn the_log_check_reads_colored_lines() {
        let log = "\u{1b}[31mERROR\u{1b}[0m Error updating \"{{#include a.rs}}\"\n INFO done\n";
        let errors = check_log(log);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].text, "ERROR Error updating \"{{#include a.rs}}\"");
        assert!(check_log(" INFO Book building has started\n WARN Caused By: x\n").is_empty());
    }
}
