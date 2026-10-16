//! A fake program of a test, written by a child process
//! (01M517P9J77TZX28QYBR9SH6WW).
//!
//! # Design
//!
//! The tests of a crate run as threads of one process. A child that a
//! thread starts gets a copy of each open file of the process, until it
//! runs its program. So when the test process writes a fake program,
//! a child of another thread can hold a copy of the write handle. The
//! run of the program then fails with `ETXTBSY` ("Text file busy"). A
//! rename after the write does not help: the file stays the same.
//!
//! So a test never opens a program to write. [`write_program`] gives
//! the text to a child `sh` on a pipe, and [`copy_program`] runs `cp`.
//! The child writes the file and ends, so no process holds a write
//! handle when the test runs the program.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant T as test thread
//!     participant S as sh (child)
//!     participant O as child of another thread
//!     T->>S: the text on a pipe
//!     S->>S: write the file, chmod 755, end
//!     Note over O: holds copies of the files of the test process only
//!     T->>T: run the program: no write handle is open
//! ```
//!
//! [`program_writers`] finds each test file that writes a program in
//! the test process. A test of this crate fails on each one.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Writes the program `path` with `text`, with the mode 755, by a child
/// `sh`. Returns `path`.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let hello = isolated::write_program(dir.path().join("hello"), "#!/bin/sh\necho hi\n");
/// let out = std::process::Command::new(&hello).output()?;
/// assert_eq!(out.stdout, b"hi\n");
///
/// use std::os::unix::fs::PermissionsExt;
/// assert_eq!(std::fs::metadata(&hello)?.permissions().mode() & 0o777, 0o755);
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// # Panics
///
/// When the child cannot write the file.
pub fn write_program(path: impl AsRef<Path>, text: impl AsRef<[u8]>) -> PathBuf {
    let path = path.as_ref();
    let mut sh = Command::new("sh")
        .args(["-c", "cat > \"$0\" && chmod 755 \"$0\""])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .expect("sh writes the program");
    let mut stdin = sh.stdin.take().expect("the stdin of sh");
    stdin
        .write_all(text.as_ref())
        .unwrap_or_else(|e| panic!("cannot give the text of {} to sh: {e}", path.display()));
    drop(stdin);
    let status = sh.wait().expect("the end of sh");
    assert!(status.success(), "sh cannot write {}", path.display());
    path.to_owned()
}

/// Copies the program `from` to `to` by a child `cp`, with its mode.
/// Returns `to`.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let one = isolated::write_program(dir.path().join("one"), "#!/bin/sh\necho one\n");
/// let two = isolated::copy_program(&one, dir.path().join("two"));
/// assert_eq!(std::process::Command::new(&two).output()?.stdout, b"one\n");
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// # Panics
///
/// When `cp` fails.
pub fn copy_program(from: impl AsRef<Path>, to: impl AsRef<Path>) -> PathBuf {
    let (from, to) = (from.as_ref(), to.as_ref());
    let out = Command::new("cp")
        .arg(from)
        .arg(to)
        .output()
        .expect("cp copies the program");
    assert!(
        out.status.success(),
        "cp {} {}: {}",
        from.display(),
        to.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    to.to_owned()
}

/// Each line of a test file under `root/crates/*/tests` that writes a
/// program in the test process, as `PATH:LINE` (see
/// [`program_writes`]). The helper crate itself does not count.
///
/// ```
/// let root = tempfile::tempdir()?;
/// let tests = root.path().join("crates/riff/tests");
/// std::fs::create_dir_all(&tests)?;
/// std::fs::write(tests.join("good.rs"), "isolated::write_program(&gh, FAKE_GH);\n")?;
/// std::fs::write(
///     tests.join("bad.rs"),
///     "std::fs::write(&gh, FAKE_GH).unwrap();\n\
///      std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();\n",
/// )?;
/// assert_eq!(isolated::program_writers(root.path()), ["crates/riff/tests/bad.rs:2"]);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn program_writers(root: &Path) -> Vec<String> {
    crate::test_lines(root, program_writes)
}

/// The lines of the test file `text` that write a program in the test
/// process, from 1. Each of these is one:
///
/// - a line that sets the mode 755 ([`writes_a_program`]);
/// - a call of `fs::write` with a text that starts with `#!`;
/// - a call of `fs::write` to a file with the name of a program that
///   the file makes with [`write_program`] or [`copy_program`]. The
///   program has the mode 755 already, so the write needs no
///   `set_permissions`. The name is the last `join("NAME")` of the
///   path, or of the `let` of a path that is a variable (see
///   [`file_name`]).
///
/// ```
/// use isolated::program_writes;
/// let good = "let gh = bin.join(\"gh\");\n\
///             isolated::write_program(&gh, FAKE_GH);\n\
///             isolated::write_program(&gh, with_pr);\n\
///             std::fs::write(bin.join(\"gh.log\"), \"\");\n\
///             fn other(path: &Path) { std::fs::write(path, \"{}\").unwrap(); }\n";
/// assert_eq!(program_writes(good), [0; 0]);
///
/// let again = "fn new(bin: &Path) {\n\
///                  let gh = bin.join(\"gh\");\n\
///                  isolated::write_program(&gh, FAKE_GH);\n\
///              }\n\
///              fn knows_pr(bin: &Path) {\n\
///                  let gh = bin.join(\"gh\");\n\
///                  std::fs::write(&gh, with_pr).unwrap();\n\
///              }\n";
/// assert_eq!(program_writes(again), [7]);
///
/// let copy = "isolated::copy_program(&riff, dir.join(name));\n\
///             isolated::copy_program(&riff, dir.join(\"riff-server\"));\n\
///             std::fs::write(\n    dir.join(\"riff-server\"),\n    text,\n);\n";
/// assert_eq!(program_writes(copy), [3]);
///
/// let script = "std::fs::write(\n    &cargo,\n    format!(\n        \"#!/bin/sh\\necho {}\\n\",\n\
///               log.display()\n    ),\n)\n.unwrap();\n\
///               fs::write(dir.join(\"riff\"), r#\"#!/bin/sh\"#).unwrap();\n";
/// assert_eq!(program_writes(script), [1, 9]);
///
/// let mode = "fs::write(&p, text)?;\n\
///             fs::set_permissions(&p, fs::Permissions::from_mode(0o755))?;\n";
/// assert_eq!(program_writes(mode), [2]);
/// ```
pub fn program_writes(text: &str) -> Vec<usize> {
    let made = |call, index| {
        calls(text, call).filter_map(move |(at, rest)| file_name(text, at, arg(rest, index)?))
    };
    let programs: Vec<&str> = made("write_program(", 0)
        .chain(made("copy_program(", 1))
        .collect();
    let mut lines: Vec<usize> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| writes_a_program(line))
        .map(|(n, _)| n + 1)
        .collect();
    for (at, rest) in calls(text, "fs::write(") {
        let statement = rest.split(';').next().unwrap_or(rest);
        let script = statement.contains("\"#!");
        let program = arg(rest, 0)
            .and_then(|path| file_name(text, at, path))
            .is_some_and(|file| programs.contains(&file));
        if script || program {
            lines.push(text[..at].matches('\n').count() + 1);
        }
    }
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// Each place in `text` where `call` starts, with the text after it.
fn calls<'a>(text: &'a str, call: &'a str) -> impl Iterator<Item = (usize, &'a str)> {
    text.match_indices(call)
        .map(move |(at, _)| (at, &text[at + call.len()..]))
}

/// The argument `index` of a call, from `rest`: the text after the open
/// parenthesis. A comma or a parenthesis in a string does not count.
fn arg(rest: &str, index: usize) -> Option<&str> {
    let (mut depth, mut start, mut found, mut in_str) = (0usize, 0, 0, false);
    let mut chars = rest.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '\\' if in_str => {
                chars.next();
            }
            '"' => in_str = !in_str,
            _ if in_str => {}
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth > 0 => depth -= 1,
            ',' | ')' if depth == 0 => {
                if found == index {
                    return Some(rest[start..at].trim());
                }
                if c == ')' {
                    return None;
                }
                (found, start) = (found + 1, at + 1);
            }
            _ => {}
        }
    }
    None
}

/// The file name of the path argument `path` of a call at `at` in
/// `text`: the last `join("NAME")` of the path. When the path is a
/// variable, for example `&gh`, it is the last `join("NAME")` of the
/// last `let` of that variable before the call. Else none.
///
/// ```
/// use isolated::file_name;
/// let text = "let gh = bin.path().join(\"gh\");\nwrite(&gh);\nwrite(path);\n";
/// let at = text.find("write(").unwrap();
/// assert_eq!(file_name(text, at, "&gh"), Some("gh"));
/// assert_eq!(file_name(text, at, "d.join(\"cargo\")"), Some("cargo"));
/// assert_eq!(file_name(text, at, "path"), None);
/// assert_eq!(file_name(text, 0, "&gh"), None);
/// ```
pub fn file_name<'a>(text: &'a str, at: usize, path: &'a str) -> Option<&'a str> {
    if let Some(file) = joined(path) {
        return Some(file);
    }
    let name = path.trim_start_matches('&').trim();
    let plain = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    if !plain {
        return None;
    }
    let before = &text[..at];
    let start = [
        format!("let {name} "),
        format!("let {name}:"),
        format!("let mut {name} "),
    ]
    .iter()
    .filter_map(|lets| before.rfind(lets.as_str()))
    .max()?;
    joined(before[start..].split(';').next()?)
}

/// The last `join("NAME")` of `expr`.
fn joined(expr: &str) -> Option<&str> {
    let start = expr.rfind("join(\"")? + "join(\"".len();
    expr[start..].split('"').next()
}

/// True when a line of a test makes a file that it wrote a program.
///
/// ```
/// use isolated::writes_a_program;
/// assert!(writes_a_program(
///     "std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();"
/// ));
/// assert!(writes_a_program("fs::set_permissions(&p, fs::Permissions::from_mode(0o755))?;"));
/// assert!(!writes_a_program("isolated::write_program(&gh, FAKE_GH);"));
/// assert!(!writes_a_program(
///     "std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();"
/// ));
/// assert!(!writes_a_program("assert_eq!(mode & 0o777, 0o755);"));
/// ```
pub fn writes_a_program(line: &str) -> bool {
    line.contains("set_permissions") && line.contains("0o755")
}
