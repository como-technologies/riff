//! Each page of the book and the requirements wraps at 72 characters
//! (01M3MNT28NXA9VRST119QR8AQK).

mod book;

/// The wrap width of the book.
const WIDTH: usize = 72;

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
fn long_lines(text: &str) -> Vec<(usize, &str)> {
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
fn each_page_of_the_book_wraps_at_72() {
    let mut long = Vec::new();
    for entry in std::fs::read_dir(book::dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "md") {
            let text = std::fs::read_to_string(&path).unwrap();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            for (n, line) in long_lines(&text) {
                long.push(format!("{name}:{n}: {line}"));
            }
        }
    }
    assert!(long.is_empty(), "lines over {WIDTH}:\n{}", long.join("\n"));
}
