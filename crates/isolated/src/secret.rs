//! A fake secret of a test (01M75NAP2HNM11ZH7RC5SP8KCJ).
//!
//! # Design
//!
//! The code scanning of GitHub runs CodeQL, with the rule
//! `rust/hard-coded-cryptographic-value`. The rule follows each
//! constant, for example a string literal, to the key of a
//! cryptographic function, for example the HMAC of the signature of a
//! webhook. It reads the tests too. So a fixed fake secret of a test
//! opens an alert, and the alert stops the cut of a release until a
//! person reviews it.
//!
//! So no test has a string literal for a webhook secret or a key.
//! [`fake_secret`] gives a new secret at each call, from random bytes of
//! the OS: CodeQL stops each flow at `getrandom::fill`. A test that
//! needs one secret in two places keeps it in a variable, or in a
//! `static` with a `LazyLock`. A key that a test parses, for example the
//! private key of a GitHub App, is the test key of a file of `testdata`.
//!
//! ```mermaid
//! flowchart LR
//!     L["a string literal"] -- "a flow: an alert" --> H["Hmac::new_from_slice"]
//!     R["getrandom::fill"] -- "no flow" --> F["fake_secret()"]
//!     F --> H
//! ```
//!
//! The one fixed secret is [`GITHUB_EXAMPLE`]: the published example of
//! the docs of GitHub, not a fake. Only a doc test uses it, and CodeQL
//! reads no doc test.
//!
//! [`hard_coded_keys`] finds each line of a test with a string literal
//! for a webhook secret or a key: in each file under `crates/*/tests`,
//! and in the test code of each file under `crates/*/src` (see
//! [`in_src`](crate::in_src)). A test of this crate fails on each one.
//! The names of the check have no word `secret`: CodeQL takes a value
//! with such a name as a secret, and the message of the failed test as
//! a log of it.

use std::path::Path;

/// The example delivery of the docs of GitHub, "Validating webhook
/// deliveries": a secret, a body and the header `X-Hub-Signature-256`
/// of the body with the secret.
#[derive(Clone, Copy, Debug)]
pub struct Example {
    pub secret: &'static str,
    pub body: &'static [u8],
    pub header: &'static str,
}

/// The example of the docs of GitHub. GitHub publishes it, so it is no
/// fake secret. Only a doc test checks a signature with it.
pub const GITHUB_EXAMPLE: Example = Example {
    secret: "It's a Secret to Everybody",
    body: b"Hello, World!",
    header: "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17",
};

/// A new fake secret for a test: a webhook secret, or a key that the
/// test does not parse. Each call gives another one: `fake-secret-`,
/// then 32 random bytes of the OS in hex.
///
/// ```
/// let secret = isolated::fake_secret();
/// assert!(secret.starts_with("fake-secret-"), "{secret}");
/// assert_eq!(secret.len(), "fake-secret-".len() + 64);
/// assert_ne!(secret, isolated::fake_secret());
/// ```
///
/// # Panics
///
/// When the OS gives no random bytes.
pub fn fake_secret() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS gives random bytes");
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("fake-secret-{hex}")
}

/// Each line of a test under `root/crates/*` with a string literal for a
/// webhook secret or a key, as `PATH:LINE` (see [`hard_coded_lines`]): a
/// line of a file under `tests`, or of the test code of a file under
/// `src` (see [`unit_tests`](crate::unit_tests)). The helper crate
/// itself does not count.
///
/// ```
/// let root = tempfile::tempdir()?;
/// let tests = root.path().join("crates/riff-server/tests");
/// std::fs::create_dir_all(&tests)?;
/// std::fs::write(tests.join("good.rs"), "let secret = isolated::fake_secret();\n")?;
/// std::fs::write(tests.join("bad.rs"), "\nlet secret = \"whsec-of-the-test\";\n")?;
/// let src = root.path().join("crates/riff/src");
/// std::fs::create_dir_all(&src)?;
/// std::fs::write(
///     src.join("lib.rs"),
///     "pub fn hook(secret: &str) {}\n\
///      #[cfg(test)]\n\
///      mod tests {\n\
///      \x20   fn a() { super::hook(\"s\"); }\n\
///      }\n",
/// )?;
/// assert_eq!(
///     isolated::hard_coded_keys(root.path()),
///     ["crates/riff-server/tests/bad.rs:2"]
/// );
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// A function of the program does not count: only a function of the same
/// test, and the functions of [`TAKERS`].
pub fn hard_coded_keys(root: &Path) -> Vec<String> {
    crate::each_test_lines(root, hard_coded_lines)
}

/// The functions of riff that take a webhook secret or a key of a GitHub
/// App, by their path, with the place of each such argument, from 0. A
/// call counts with each path before it, for example
/// `riff_server::hooks::signature`.
pub const TAKERS: [(&[&str], &[usize]); 3] = [
    (&["signature"], &[0]),
    (&["signed"], &[0]),
    (&["AppKeys", "new"], &[3, 4]),
];

/// The lines of the test file `text` with a string literal for a webhook
/// secret or a key, from 1. The literal is `"…"`, `b"…"` or `r"…"`,
/// also after `&`, in `Some(…)` or in `String::from(…)`. Each of these
/// is one:
///
/// - the value of a `let`, a `const`, a `static`, a field or a JSON key
///   with a [secret name](is_secret_name);
/// - the value of a variable with a secret name in a pair, for example
///   `.env("RIFF_OIDC_CLIENT_SECRET", …)`, and of a flag with a secret
///   name, for example `"--client-secret", …`;
/// - the argument for a secret or a key of a function of [`TAKERS`];
/// - the argument for a parameter with a secret name of a function of
///   `text`.
///
/// ```
/// let text = r##"
/// pub const WEBHOOK_SECRET: &str = "whsec-of-the-test";
/// let app = AppKeys::new(7, "riff-acme", "acme", "THE KEY", Some("THE SECRET"));
/// async fn deliver(url: &str, secret: Option<&str>) {}
/// deliver(&url, Some("another secret")).await;
/// json!({ "webhook_secret": "w", "client_secret": "c", "pem": KEY });
/// let header = riff_server::hooks::signature(&secret, &body);
/// let good = signature("s", b"{}");
/// cmd.env("RIFF_OIDC_CLIENT_SECRET", "my-secret").args(["--client-secret", "wrong"]);
/// let key = "pull-40-conflict";
/// let token = "gho_Zx9";
/// const SECRET_OPEN: &str = r#"{"number":2,"state":"open"}"#;
/// cloud.set("shared.env", "CLOUD_GITHUB_SECRET", "");
/// let names = ["CLOUD_SECRET", "CLOUD_BUCKET", "--client-secret", "--issuer"];
/// "##;
/// assert_eq!(isolated::hard_coded_lines(text), [2, 3, 5, 6, 8, 9]);
/// ```
pub fn hard_coded_lines(text: &str) -> Vec<usize> {
    let tokens = tokens(text);
    let mut takers: Vec<(Vec<String>, Vec<usize>)> = TAKERS
        .iter()
        .map(|(path, places)| {
            let path = path.iter().map(|part| (*part).to_owned()).collect();
            (path, places.to_vec())
        })
        .collect();
    takers.extend(local_takers(&tokens));
    let mut lines = Vec::new();
    for at in 0..tokens.len() {
        lines.extend(named_literal(&tokens, at));
        for (path, places) in &takers {
            let Some(args) = call_args(&tokens, at, path) else {
                continue;
            };
            let found = places.iter().filter_map(|place| args.get(*place));
            lines.extend(found.filter_map(|arg| literal_at(&tokens, *arg)));
        }
    }
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// True when `name` is a secret name: its last word, after a `_` or a
/// `-`, is `secret`, in any case.
///
/// ```
/// use isolated::is_secret_name;
/// assert!(is_secret_name("LEAD_WEBHOOK_SECRET"));
/// assert!(is_secret_name("client_secret"));
/// assert!(is_secret_name("--client-secret"));
/// assert!(is_secret_name("secret"));
/// assert!(!is_secret_name("secrets"));
/// assert!(!is_secret_name("SECRET_OPEN"));
/// ```
pub fn is_secret_name(name: &str) -> bool {
    name.rsplit(['_', '-'])
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("secret"))
}

/// True when `name` is the name of a variable, for example
/// `RIFF_OIDC_CLIENT_SECRET`.
fn is_variable(name: &str) -> bool {
    name.chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// A token of Rust code.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    /// A name or a keyword.
    Name(String),
    /// A string literal, with its text.
    Text(String),
    /// A mark: one char, or one of [`MARKS`].
    Mark(String),
    /// A number, a char or a lifetime.
    Other,
}

/// The marks of two chars.
const MARKS: [&str; 14] = [
    "::", "=>", "==", "!=", "<=", ">=", "->", "+=", "-=", "*=", "/=", "%=", "^=", "|=",
];

/// The tokens of `text`, each with its line from 1. A comment gives no
/// token.
fn tokens(text: &str) -> Vec<(Token, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut line = 1;
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        let from = at;
        let next = chars.get(at + 1).copied();
        let token = if c.is_whitespace() {
            at += 1;
            None
        } else if c == '/' && next == Some('/') {
            at = (at..chars.len())
                .find(|n| chars[*n] == '\n')
                .unwrap_or(chars.len());
            None
        } else if c == '/' && next == Some('*') {
            at = block_comment_end(&chars, at);
            None
        } else if c == '"' {
            let (text, end) = string(&chars, at + 1, None);
            at = end;
            Some(Token::Text(text))
        } else if c == '\'' {
            at = quote_end(&chars, at);
            Some(Token::Other)
        } else if c.is_alphabetic() || c == '_' {
            let (token, end) = word(&chars, at);
            at = end;
            Some(token)
        } else if c.is_ascii_digit() {
            at = name_end(&chars, at);
            Some(Token::Other)
        } else {
            let two: String = chars[at..chars.len().min(at + 2)].iter().collect();
            let mark = if MARKS.contains(&two.as_str()) {
                two
            } else {
                c.to_string()
            };
            at += mark.chars().count();
            Some(Token::Mark(mark))
        };
        if let Some(token) = token {
            out.push((token, line));
        }
        line += chars[from..at].iter().filter(|c| **c == '\n').count();
    }
    out
}

/// The end of the letters, digits and `_` from `at`.
fn name_end(chars: &[char], at: usize) -> usize {
    (at..chars.len())
        .find(|n| !(chars[*n].is_alphanumeric() || chars[*n] == '_'))
        .unwrap_or(chars.len())
}

/// The end of the comment `/* … */` at `at`, with each comment in it.
fn block_comment_end(chars: &[char], mut at: usize) -> usize {
    let mut depth = 0;
    while at < chars.len() {
        match (chars[at], chars.get(at + 1)) {
            ('/', Some('*')) => {
                depth += 1;
                at += 2;
            }
            ('*', Some('/')) => {
                depth -= 1;
                at += 2;
                if depth == 0 {
                    return at;
                }
            }
            _ => at += 1,
        }
    }
    chars.len()
}

/// The text of the string literal from `at`, after its `"`, and its end.
/// `raw` is the count of `#` of a raw literal: it has no escapes.
fn string(chars: &[char], mut at: usize, raw: Option<usize>) -> (String, usize) {
    let mut text = String::new();
    while let Some(&c) = chars.get(at) {
        match (c, raw) {
            ('\\', None) => {
                text.extend(chars.get(at + 1));
                at += 2;
            }
            ('"', None) => return (text, at + 1),
            ('"', Some(hashes)) if (1..=hashes).all(|n| chars.get(at + n) == Some(&'#')) => {
                return (text, at + 1 + hashes);
            }
            _ => {
                text.push(c);
                at += 1;
            }
        }
    }
    (text, chars.len())
}

/// The end of the char literal or the lifetime at the `'` at `at`.
fn quote_end(chars: &[char], at: usize) -> usize {
    match (chars.get(at + 1), chars.get(at + 2)) {
        (Some('\\'), _) => (at + 3..chars.len())
            .find(|n| chars[*n] == '\'')
            .map_or(chars.len(), |n| n + 1),
        (Some(_), Some('\'')) => at + 3,
        _ => name_end(chars, at + 1),
    }
}

/// The name at `at`, or the string literal with the prefix `b`, `c`,
/// `r`, `br` or `cr` there, and its end.
fn word(chars: &[char], at: usize) -> (Token, usize) {
    let end = name_end(chars, at);
    let name: String = chars[at..end].iter().collect();
    let hashes = (end..chars.len()).take_while(|n| chars[*n] == '#').count();
    let quote = chars.get(end + hashes) == Some(&'"');
    match name.as_str() {
        "b" | "c" if hashes == 0 && quote => {
            let (text, end) = string(chars, end + 1, None);
            (Token::Text(text), end)
        }
        "r" | "br" | "cr" if quote => {
            let (text, end) = string(chars, end + hashes + 1, Some(hashes));
            (Token::Text(text), end)
        }
        // A raw name, for example `r#type`.
        "r" if hashes == 1 => {
            let after = name_end(chars, end + 1);
            (Token::Name(chars[end + 1..after].iter().collect()), after)
        }
        _ => (Token::Name(name), end),
    }
}

fn is_mark(tokens: &[(Token, usize)], at: usize, mark: &str) -> bool {
    matches!(tokens.get(at), Some((Token::Mark(m), _)) if m == mark)
}

fn is_name(tokens: &[(Token, usize)], at: usize, name: &str) -> bool {
    name_at(tokens, at) == Some(name)
}

fn name_at(tokens: &[(Token, usize)], at: usize) -> Option<&str> {
    match tokens.get(at) {
        Some((Token::Name(name), _)) => Some(name),
        _ => None,
    }
}

/// The line of the string literal at `at`: also after `&`, in `Some(…)`
/// or in `String::from(…)`.
fn literal_at(tokens: &[(Token, usize)], mut at: usize) -> Option<usize> {
    loop {
        match tokens.get(at)? {
            (Token::Text(_), line) => return Some(*line),
            (Token::Mark(mark), _) if mark == "&" => at += 1,
            (Token::Name(name), _) if name == "Some" && is_mark(tokens, at + 1, "(") => at += 2,
            (Token::Name(name), _)
                if name == "String"
                    && is_mark(tokens, at + 1, "::")
                    && is_name(tokens, at + 2, "from")
                    && is_mark(tokens, at + 3, "(") =>
            {
                at += 4;
            }
            _ => return None,
        }
    }
}

/// The line of the string literal of a secret name at `at`: `let NAME =
/// LITERAL`, also with `mut` or a type, `const` and `static`, `NAME:
/// LITERAL` of a field or of a JSON key, and `"NAME", LITERAL` of a
/// variable or a flag.
fn named_literal(tokens: &[(Token, usize)], at: usize) -> Option<usize> {
    if matches!(name_at(tokens, at), Some("let" | "const" | "static")) {
        let name = at + 1 + usize::from(is_name(tokens, at + 1, "mut"));
        if !name_at(tokens, name).is_some_and(is_secret_name) {
            return None;
        }
        let end =
            (name..tokens.len()).find(|n| is_mark(tokens, *n, "=") || is_mark(tokens, *n, ";"))?;
        return if is_mark(tokens, end, "=") {
            literal_at(tokens, end + 1)
        } else {
            None
        };
    }
    match tokens.get(at) {
        Some((Token::Name(name) | Token::Text(name), _))
            if is_secret_name(name) && is_mark(tokens, at + 1, ":") =>
        {
            literal_at(tokens, at + 2)
        }
        Some((Token::Text(name), _)) if is_secret_name(name) && is_mark(tokens, at + 1, ",") => {
            let value = literal_at(tokens, at + 2)?;
            let flag = name.starts_with("--")
                && !matches!(&tokens[at + 2].0, Token::Text(text) if text.starts_with('-'));
            let pair = is_variable(name)
                && at > 0
                && is_mark(tokens, at - 1, "(")
                && args(tokens, at - 1).len() == 2;
            (flag || pair).then_some(value)
        }
        _ => None,
    }
}

/// The start of each argument of the list that opens with the `(` at
/// `open`, up to its `)`.
fn args(tokens: &[(Token, usize)], open: usize) -> Vec<usize> {
    let mut starts = vec![open + 1];
    let mut depth = 0;
    for (at, (token, _)) in tokens.iter().enumerate().skip(open + 1) {
        let Token::Mark(mark) = token else {
            continue;
        };
        match mark.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" if depth == 0 => {
                // A list with no argument, or with a `,` at its end.
                if starts.last() == Some(&at) {
                    starts.pop();
                }
                break;
            }
            ")" | "]" | "}" => depth -= 1,
            "," if depth == 0 => starts.push(at + 1),
            _ => {}
        }
    }
    starts
}

/// The start of each argument of the call of `path` at `at`, when a call
/// of `path` is there: not the `fn` of it.
fn call_args(tokens: &[(Token, usize)], at: usize, path: &[String]) -> Option<Vec<usize>> {
    if at > 0 && is_name(tokens, at - 1, "fn") {
        return None;
    }
    let mut next = at;
    for (n, part) in path.iter().enumerate() {
        if n > 0 {
            if !is_mark(tokens, next, "::") {
                return None;
            }
            next += 1;
        }
        if !is_name(tokens, next, part) {
            return None;
        }
        next += 1;
    }
    is_mark(tokens, next, "(").then(|| args(tokens, next))
}

/// Each function of the tokens with a parameter whose name has the word
/// `secret`, with the place of each such parameter, from 0. A `self`
/// has no place.
fn local_takers(tokens: &[(Token, usize)]) -> Vec<(Vec<String>, Vec<usize>)> {
    let mut found = Vec::new();
    for at in 0..tokens.len() {
        if !is_name(tokens, at, "fn") {
            continue;
        }
        let Some(name) = name_at(tokens, at + 1) else {
            continue;
        };
        let mut open = at + 2;
        if is_mark(tokens, open, "<") {
            let mut depth = 0;
            while open < tokens.len() {
                if is_mark(tokens, open, "<") {
                    depth += 1;
                } else if is_mark(tokens, open, ">") {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                open += 1;
            }
            open += 1;
        }
        if !is_mark(tokens, open, "(") {
            continue;
        }
        let mut places = Vec::new();
        let mut place = 0;
        for param in args(tokens, open) {
            let pattern: Vec<&Token> = tokens[param..]
                .iter()
                .map(|(token, _)| token)
                .take_while(
                    |token| !matches!(token, Token::Mark(m) if m == ":" || m == "," || m == ")"),
                )
                .collect();
            if pattern.contains(&&Token::Name("self".to_owned())) {
                continue;
            }
            let last = pattern.iter().rev().find_map(|token| match token {
                Token::Name(name) => Some(name),
                _ => None,
            });
            if last.is_some_and(|name| is_secret_name(name)) {
                places.push(place);
            }
            place += 1;
        }
        if !places.is_empty() {
            found.push((vec![name.to_owned()], places));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_in_a_comment_or_after_a_char_does_not_count() {
        let text = "// let secret = \"a\";\n\
                    /* let secret = \"b\"; /* nested */ let secret = \"c\"; */\n\
                    let quote = '\"'; let escaped = '\\''; let s: &'static str = \"d\";\n\
                    let secret = \"e\";\n";
        assert_eq!(hard_coded_lines(text), [4]);
    }

    #[test]
    fn a_raw_string_and_a_string_over_lines_keep_the_lines() {
        let text = "let other = r#\"a \"quoted\" text\n\
                    and a line\"#;\n\
                    let more = \"one\\\n\
                    two\";\n\
                    let webhook_secret = br\"raw\";\n\
                    let r#secret = b\"bytes\";\n";
        assert_eq!(hard_coded_lines(text), [5, 6]);
    }

    #[test]
    fn a_parameter_counts_after_generics_and_without_self() {
        let text = "fn sign<T: Into<String>>(body: T, secret: &str) {}\n\
                    impl App { fn hook(&self, url: &str, lead_secret: &str) {} }\n\
                    sign(\"body\", \"s\");\n\
                    app.hook(\"url\", String::from(\"s\"));\n\
                    sign(\"body\", &secret);\n";
        assert_eq!(hard_coded_lines(text), [3, 4]);
    }

    #[test]
    fn the_secret_of_a_taker_counts_and_its_other_arguments_do_not() {
        let text = "fn signed(url: &str) {}\n\
                    let ok = hooks::signed(&secret, b\"{}\", \"sha256=00\");\n\
                    let app = riff_server::keys::AppKeys::new(\n\
                    \x20   7,\n\
                    \x20   \"riff-acme\",\n\
                    \x20   \"acme\",\n\
                    \x20   KEY,\n\
                    \x20   None,\n\
                    );\n\
                    let app = AppKeys::new(7, \"riff-acme\", \"acme\", KEY, Some(\"s\"),);\n";
        assert_eq!(hard_coded_lines(text), [10]);
    }

    #[test]
    fn each_fake_secret_is_new() {
        let secrets: std::collections::HashSet<String> = (0..100).map(|_| fake_secret()).collect();
        assert_eq!(secrets.len(), 100);
    }
}
