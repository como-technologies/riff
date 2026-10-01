//! `hygiene book` in a git repository, as `just book` runs it in a
//! worktree: the install of the theme keeps `book.toml`, and a build
//! that changes a tracked file fails (01M3W5YW0172EVF2JA8T7WW392). The
//! theme tool is a fake on the PATH. `mdbook` is the real one.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;

/// Held while a test writes a fake tool, and while it starts a process.
/// A process that starts while another thread writes the script gets a
/// copy of its open file, and then the exec of the script fails with
/// "Text file busy".
static SPAWN: Mutex<()> = Mutex::new(());

/// Runs `cmd` to its end. It starts under [`SPAWN`].
fn run(cmd: &mut Command) -> Output {
    let child = {
        let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The `book.toml` of the test book. It names one file of the theme.
const BOOK_TOML: &str = "[book]\ntitle = \"t\"\nsrc = \"src\"\n\n\
                         [output.html]\nadditional-css = [\"gruvbox/x.css\"]\n";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = run(Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir));
    assert!(out.status.success(), "git {args:?}: {}", text(&out.stderr));
    text(&out.stdout)
}

/// A git repository with one commit: the book in `docs`, the file
/// `notes.txt`, and a `.gitignore` for the theme and the built book. The
/// directory `bin` is for the fake tools. It is not in the repository.
struct Repo {
    dir: tempfile::TempDir,
}

impl Repo {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = Self { dir };
        let src = repo.top().join("docs/src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir(repo.bin()).unwrap();
        std::fs::write(repo.top().join("docs/book.toml"), BOOK_TOML).unwrap();
        std::fs::write(src.join("SUMMARY.md"), "# Summary\n\n- [A](a.md)\n").unwrap();
        std::fs::write(src.join("a.md"), "# A\n\nText.\n").unwrap();
        std::fs::write(repo.top().join("notes.txt"), "notes\n").unwrap();
        std::fs::write(
            repo.top().join(".gitignore"),
            "/docs/book\n/docs/gruvbox\n/bin\n",
        )
        .unwrap();
        git(repo.top(), &["init", "-q"]);
        git(repo.top(), &["add", "-A"]);
        git(repo.top(), &["commit", "-q", "-m", "A book"]);
        repo
    }

    fn top(&self) -> &Path {
        self.dir.path()
    }

    fn bin(&self) -> PathBuf {
        self.top().join("bin")
    }

    /// Writes the fake tool `name` with the shell `script`.
    fn tool(&self, name: &str, script: &str) {
        let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.bin().join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A fake `mdbook-gruvbox`. As version 1.2.0 of the real tool, its
    /// install writes the theme and also changes `book.toml`. It counts
    /// its calls in `bin/calls`.
    fn theme_tool(&self) {
        self.tool(
            "mdbook-gruvbox",
            "echo call >> \"$(dirname \"$0\")/calls\"\n\
             mkdir -p \"$2/gruvbox\" && echo 'a{}' > \"$2/gruvbox/x.css\"\n\
             echo '# changed by the install' >> \"$2/book.toml\"",
        );
    }

    /// How many times the fake `mdbook-gruvbox` ran.
    fn installs(&self) -> usize {
        std::fs::read_to_string(self.bin().join("calls")).map_or(0, |calls| calls.lines().count())
    }

    /// Runs `hygiene book docs` at the top, with the fake tools first on
    /// the PATH.
    fn check(&self) -> (Output, String) {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let path =
            std::env::join_paths(std::iter::once(self.bin()).chain(std::env::split_paths(&path)))
                .unwrap();
        let out = run(Command::new(env!("CARGO_BIN_EXE_hygiene"))
            .args(["book", "docs"])
            .env("PATH", path)
            .current_dir(self.top()));
        let stderr = text(&out.stderr);
        assert_ne!(
            out.status.code(),
            Some(2),
            "hygiene book could not run mdbook; install it with just init:\n{stderr}"
        );
        (out, stderr)
    }

    fn status(&self) -> String {
        git(self.top(), &["status", "--porcelain"])
    }
}

/// The real `mdbook` on the PATH.
fn mdbook() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("mdbook"))
        .find(|path| path.is_file())
        .expect("mdbook is on the PATH; install it with just init")
}

#[test]
fn the_install_of_the_theme_keeps_book_toml() {
    let repo = Repo::new();
    repo.theme_tool();
    let (out, stderr) = repo.check();
    assert!(out.status.success(), "{stderr}");
    assert_eq!(repo.installs(), 1);
    assert!(repo.top().join("docs/gruvbox/x.css").is_file());
    assert_eq!(
        std::fs::read_to_string(repo.top().join("docs/book.toml")).unwrap(),
        BOOK_TOML
    );
    assert_eq!(repo.status(), "");
}

#[test]
fn a_theme_that_is_there_is_not_installed_again() {
    let repo = Repo::new();
    repo.theme_tool();
    let (out, stderr) = repo.check();
    assert!(out.status.success(), "{stderr}");
    let (out, stderr) = repo.check();
    assert!(out.status.success(), "{stderr}");
    assert_eq!(repo.installs(), 1);
    assert_eq!(repo.status(), "");
}

#[test]
fn a_theme_tool_that_fails_is_a_tool_error() {
    let repo = Repo::new();
    repo.tool("mdbook-gruvbox", "echo 'no theme' >&2\nexit 1");
    let path = std::env::join_paths([repo.bin(), mdbook().parent().unwrap().to_owned()]).unwrap();
    let out = run(Command::new(env!("CARGO_BIN_EXE_hygiene"))
        .args(["book", "docs"])
        .env("PATH", path)
        .current_dir(repo.top()));
    assert_eq!(out.status.code(), Some(2));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("error: mdbook-gruvbox install docs: no theme"),
        "{stderr}"
    );
}

#[test]
fn a_build_that_changes_a_tracked_file_fails() {
    let repo = Repo::new();
    repo.theme_tool();
    repo.tool(
        "mdbook",
        &format!(
            "echo more >> notes.txt\nexec '{}' \"$@\"",
            mdbook().display()
        ),
    );
    let (out, stderr) = repo.check();
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("error: tracked: notes.txt: the build changed this tracked file"),
        "{stderr}"
    );
    assert!(!stderr.contains("docs/book.toml"), "{stderr}");
    assert!(stderr.contains("rule(s) of the book check"), "{stderr}");
}

#[test]
fn a_file_that_a_person_changed_before_the_build_passes() {
    let repo = Repo::new();
    repo.theme_tool();
    std::fs::write(repo.top().join("notes.txt"), "my change\n").unwrap();
    std::fs::write(
        repo.top().join("docs/book.toml"),
        format!("{BOOK_TOML}# my change\n"),
    )
    .unwrap();
    let (out, stderr) = repo.check();
    assert!(out.status.success(), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(repo.top().join("docs/book.toml")).unwrap(),
        format!("{BOOK_TOML}# my change\n")
    );
    assert_eq!(repo.status(), " M docs/book.toml\n M notes.txt\n");
}
