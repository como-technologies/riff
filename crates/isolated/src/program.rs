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
/// program in the test process, as `PATH:LINE`. The helper crate itself
/// does not count.
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
    crate::test_lines(root, writes_a_program)
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
