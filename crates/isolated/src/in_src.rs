//! The tests in a file of `crates/*/src`: its `#[cfg(test)]` items and
//! its doc tests.
//!
//! # Design
//!
//! A check of the tests reads each test, and no code of a program. A
//! file under `crates/*/tests` is all test. A file of `src` holds code
//! of the program too, and the code of a program can write a program,
//! for example `riff update`. So [`test_code`] keeps only the tests of
//! a file, each in a text of its own:
//!
//! - the `#[cfg(test)]` items, in one text: each line from the
//!   attribute to the end of the item. An attribute `cfg` with the word
//!   `test` counts, but one with `not(`;
//! - each doc test: each line of a `///` or `//!` comment in one Rust
//!   code block, without the `///` or `//!`. A doc test is a program of
//!   its own, so a check that finds a name in one doc test does not
//!   look for it in another.
//!
//! In each text, each line that is not of the test is empty, so a line
//! number of a test is the line number of the file. A module
//! `#[cfg(test)] mod NAME;` is in a file of its own: [`test_modules`]
//! gives its names, and each line of that file is a test.
//!
//! ```mermaid
//! flowchart LR
//!     F[file of src] --> A{"in a #[cfg(test)] item?"}
//!     A -- yes --> K[the line, in the text of the items]
//!     A -- no --> D{"in a Rust block of a doc comment?"}
//!     D -- yes --> C[the code after ///, in the text of that doc test]
//!     D -- no --> E[an empty line in each text]
//! ```

use std::path::{Path, PathBuf};

/// The tests of each of `files`, each a file of `src` with its text: all
/// the text of a file of a module `#[cfg(test)] mod NAME;`, and of each
/// file under it, else each text of [`test_code`]. A file can come more
/// than one time.
///
/// ```
/// use std::path::PathBuf;
/// let files = vec![
///     (PathBuf::from("src/tui.rs"), "#[cfg(test)]\nmod tests;\n".to_owned()),
///     (PathBuf::from("src/tui/tests.rs"), "fn a() {}\n".to_owned()),
///     (PathBuf::from("src/lib.rs"), "#[cfg(test)]\nmod rules;\nfn b() {}\n".to_owned()),
///     (PathBuf::from("src/rules/mod.rs"), "fn c() {}\n".to_owned()),
///     (PathBuf::from("src/other.rs"), "/// ```\n/// d();\n/// ```\nfn d() {}\n".to_owned()),
/// ];
/// let tests = isolated::unit_tests(files);
/// let texts: Vec<(&str, &str)> = tests
///     .iter()
///     .map(|(file, text)| (file.to_str().unwrap(), text.as_str()))
///     .collect();
/// assert_eq!(
///     texts,
///     [
///         ("src/tui.rs", "#[cfg(test)]\nmod tests;"),
///         ("src/tui/tests.rs", "fn a() {}\n"),
///         ("src/lib.rs", "#[cfg(test)]\nmod rules;\n"),
///         ("src/rules/mod.rs", "fn c() {}\n"),
///         ("src/other.rs", "\n\n\n"),
///         ("src/other.rs", "\n d();\n\n"),
///     ]
/// );
/// ```
pub fn unit_tests(files: Vec<(PathBuf, String)>) -> Vec<(PathBuf, String)> {
    let modules: Vec<PathBuf> = files
        .iter()
        .flat_map(|(file, text)| {
            let roots = ["mod.rs", "lib.rs", "main.rs"];
            let dir = match file.file_name() {
                Some(name) if roots.iter().any(|root| name == *root) => {
                    file.parent().map(Path::to_owned).unwrap_or_default()
                }
                _ => file.with_extension(""),
            };
            test_modules(text)
                .into_iter()
                .map(move |name| dir.join(name))
        })
        .collect();
    files
        .into_iter()
        .flat_map(|(file, text)| {
            let module = modules
                .iter()
                .any(|m| file.with_extension("") == *m || file.starts_with(m));
            let codes = if module { vec![text] } else { test_code(&text) };
            codes.into_iter().map(move |code| (file.clone(), code))
        })
        .collect()
}

/// The tests of the text of a file of `src`, each with the lines of the
/// file: first the `#[cfg(test)]` items, then each doc test. In each
/// text, each line that is not of its test is empty (see [the
/// design](self)).
///
/// ```
/// let text = "/// A doc.\n\
///             ///\n\
///             /// ```\n\
///             /// let a = 1;\n\
///             /// ```\n\
///             /// ```text\n\
///             /// not code\n\
///             /// ```\n\
///             pub fn a() {}\n\
///             #[cfg(test)]\n\
///             mod tests {\n\
///             \x20   fn b() { let s = \"}\"; let c = '}'; }\n\
///             }\n\
///             fn c() {}\n\
///             #[cfg(not(test))]\n\
///             fn d() {}\n\
///             //! ```no_run\n\
///             //! let e = 2;\n";
/// let codes = isolated::test_code(text);
/// let codes: Vec<Vec<&str>> = codes.iter().map(|code| code.split('\n').collect()).collect();
/// assert_eq!(codes.len(), 3);
/// let lines = text.lines().count();
/// assert!(codes.iter().all(|code| code.len() == lines));
///
/// let items = &codes[0];
/// assert_eq!(&items[9..13], ["#[cfg(test)]", "mod tests {", text.lines().nth(11).unwrap(), "}"]);
/// assert_eq!(items.iter().filter(|line| !line.is_empty()).count(), 4);
///
/// assert_eq!(codes[1][3], " let a = 1;");
/// assert_eq!(codes[1].iter().filter(|line| !line.is_empty()).count(), 1);
/// assert_eq!(codes[2][17], " let e = 2;");
/// assert_eq!(codes[2].iter().filter(|line| !line.is_empty()).count(), 1);
/// ```
pub fn test_code(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut items = vec![""; lines.len()];
    for (first, _, last) in test_items(text) {
        items[first..=last].copy_from_slice(&lines[first..=last]);
    }
    let mut codes = vec![items];
    // Outside a code block, `None`. In a block, the code of its doc
    // test, or `None` for a block of another language.
    let mut block: Option<Option<Vec<&str>>> = None;
    for (n, line) in lines.iter().enumerate() {
        let Some(doc) = doc_text(line) else {
            codes.extend(block.take().flatten());
            continue;
        };
        if let Some(info) = doc.trim_start().strip_prefix("```") {
            match block.take() {
                Some(code) => codes.extend(code),
                None => block = Some(rust_block(info).then(|| vec![""; lines.len()])),
            }
        } else if let Some(Some(code)) = &mut block {
            code[n] = doc;
        }
    }
    codes.extend(block.flatten());
    codes.into_iter().map(|code| code.join("\n")).collect()
}

/// The name of each module `#[cfg(test)] mod NAME;` of `text`: a module
/// in a file of its own.
///
/// ```
/// let text = "#[cfg(test)]\nmod tests;\n#[cfg(test)]\npub(crate) mod rules;\nmod other;\n";
/// assert_eq!(isolated::test_modules(text), ["tests", "rules"]);
/// ```
pub fn test_modules(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    test_items(text)
        .into_iter()
        .filter_map(|(_, item, _)| {
            let words: Vec<&str> = lines[item].split_whitespace().collect();
            let at = words.iter().position(|w| *w == "mod")?;
            words.get(at + 1)?.strip_suffix(';')
        })
        .collect()
}

/// Each item of `text` with a `#[cfg(test)]` attribute, as line
/// numbers from 0: the attribute, the line of the item, and its last
/// line.
fn test_items(text: &str) -> Vec<(usize, usize, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut starts = Vec::with_capacity(lines.len());
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        starts.push(at);
        at += line.len();
    }
    let mut items = Vec::new();
    let mut n = 0;
    while n < lines.len() {
        if !test_cfg(lines[n]) {
            n += 1;
            continue;
        }
        let mut item = n + 1;
        while item < lines.len() && {
            let line = lines[item].trim_start();
            line.is_empty() || line.starts_with("#[") || line.starts_with("//")
        } {
            item += 1;
        }
        if item == lines.len() {
            break;
        }
        let end = item_end(text, starts[item]);
        let last = text[..end].matches('\n').count().min(lines.len() - 1);
        items.push((n, item, last));
        n = last + 1;
    }
    items
}

/// True when `line` is a `cfg` attribute with the word `test`, and
/// with no `not(`.
fn test_cfg(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("#[cfg(")
        && !line.contains("not(")
        && line
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|word| word == "test")
}

/// The text of a `///` or `//!` comment line, after the `///` or `//!`.
fn doc_text(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let doc = line
        .strip_prefix("///")
        .or_else(|| line.strip_prefix("//!"))?;
    (!doc.starts_with('/')).then_some(doc)
}

/// True when the info of a code block of a doc comment names Rust: no
/// info, or only `rust` and the attributes of a doc test.
fn rust_block(info: &str) -> bool {
    const WORDS: [&str; 7] = [
        "rust",
        "no_run",
        "ignore",
        "should_panic",
        "compile_fail",
        "standalone_crate",
        "test_harness",
    ];
    info.split([',', ' ', '\t'])
        .filter(|word| !word.is_empty())
        .all(|word| {
            WORDS.contains(&word) || word.starts_with("edition") || word.starts_with("ignore-")
        })
}

/// The byte of `text` that ends the item at `from`: its `;`, or the `}`
/// of its block. A brace in a string, a char or a comment does not
/// count.
fn item_end(text: &str, from: usize) -> usize {
    let bytes = text.as_bytes();
    let (mut braces, mut parens) = (0usize, 0usize);
    let mut at = from;
    while at < bytes.len() {
        match bytes[at] {
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at = text[at..].find('\n').map_or(bytes.len(), |n| at + n);
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => at = comment_end(text, at),
            b'"' => at = string_end(text, at),
            b'\'' => at = char_end(text, at),
            b'(' | b'[' => parens += 1,
            b')' | b']' => parens = parens.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => {
                braces = braces.saturating_sub(1);
                if braces == 0 && parens == 0 {
                    return at;
                }
            }
            b';' if braces == 0 && parens == 0 => return at,
            _ => {}
        }
        at += 1;
    }
    bytes.len()
}

/// The last byte of the block comment at `at`, which can hold others.
fn comment_end(text: &str, mut at: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    while at + 1 < bytes.len() {
        match (bytes[at], bytes[at + 1]) {
            (b'/', b'*') => {
                depth += 1;
                at += 1;
            }
            (b'*', b'/') => {
                depth -= 1;
                at += 1;
                if depth == 0 {
                    return at;
                }
            }
            _ => {}
        }
        at += 1;
    }
    bytes.len()
}

/// The last byte of the string whose `"` is at `at`: a plain string, or
/// a raw string `r#"…"#` with each count of `#`.
fn string_end(text: &str, at: usize) -> usize {
    let bytes = text.as_bytes();
    let hashes = bytes[..at].iter().rev().take_while(|b| **b == b'#').count();
    if at > hashes && bytes[at - hashes - 1] == b'r' {
        let end = format!("\"{}", "#".repeat(hashes));
        return text[at + 1..]
            .find(&end)
            .map_or(bytes.len(), |n| at + n + end.len());
    }
    let mut next = at + 1;
    while next < bytes.len() {
        match bytes[next] {
            b'\\' => next += 1,
            b'"' => return next,
            _ => {}
        }
        next += 1;
    }
    bytes.len()
}

/// The last byte of the char whose `'` is at `at`, or `at` for a
/// lifetime or a label.
fn char_end(text: &str, at: usize) -> usize {
    let rest = &text[at + 1..];
    if rest.starts_with('\\') {
        return rest[2.min(rest.len())..]
            .find('\'')
            .map_or(text.len(), |n| at + 3 + n);
    }
    match rest.chars().next() {
        Some(c) if rest[c.len_utf8()..].starts_with('\'') => at + c.len_utf8() + 1,
        _ => at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_ends_at_its_semicolon_or_at_the_brace_of_its_block() {
        let text = "mod tests;\nfn a() {}\nconst A: [&str; 2] = [\"{\", \"}\"];\n";
        assert_eq!(item_end(text, 0), 9);
        assert_eq!(&text[item_end(text, 11)..][..1], "}");
        assert_eq!(&text[item_end(text, 21)..], ";\n");
    }

    #[test]
    fn a_brace_in_a_string_a_char_or_a_comment_does_not_count() {
        let text = "mod tests {\n\
                    \x20   // }\n\
                    \x20   /* } /* } */ } */\n\
                    \x20   fn a<'a>(s: &'a str) { let _ = ('}', '\\'', '\\u{7d}', 'é'); }\n\
                    \x20   const S: &str = r#\"}\"#;\n\
                    \x20   const T: &str = \"\\\"}\";\n\
                    \x20   const U: &[u8] = br\"}\";\n\
                    }\n\
                    fn after() {}\n";
        let end = item_end(text, 0);
        assert_eq!(text[..end].matches('\n').count(), 7);
    }

    #[test]
    fn a_test_item_starts_at_its_attribute_and_skips_the_other_attributes() {
        let text = "fn a() {}\n\
                    #[cfg(any(test, feature = \"test-support\"))]\n\
                    // A helper.\n\
                    #[allow(dead_code)]\n\
                    pub(crate) fn helper(\n\
                    \x20   a: u8,\n\
                    ) -> u8 {\n\
                    \x20   a\n\
                    }\n\
                    #[cfg(test)]\n\
                    mod tests;\n";
        assert_eq!(test_items(text), [(1, 4, 8), (9, 10, 10)]);
    }

    #[test]
    fn a_block_of_another_language_is_no_doc_test() {
        assert!(rust_block(""));
        assert!(rust_block("rust"));
        assert!(rust_block("no_run"));
        assert!(rust_block("rust,ignore"));
        assert!(rust_block("should_panic, edition2021"));
        assert!(!rust_block("text"));
        assert!(!rust_block("sh"));
        assert!(!rust_block("mermaid"));
    }

    #[test]
    fn a_comment_of_four_slashes_is_no_doc() {
        assert_eq!(doc_text("    /// let a = 1;"), Some(" let a = 1;"));
        assert_eq!(doc_text("//! text"), Some(" text"));
        assert_eq!(doc_text("//// not a doc"), None);
        assert_eq!(doc_text("// a comment"), None);
    }
}
