//! `hygiene pr N` checks pull request N with `gh`. `hygiene commit [REV]`
//! checks the message of a commit with `git`. `hygiene book [DIR]` builds
//! the book in DIR with `mdbook` and checks it. `hygiene wrap [DIR]`
//! checks the wrap of each Markdown file in DIR. `hygiene ci [BASE]`
//! prints the recipe that `just ci` runs. See the library docs.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use hygiene::{Issue, PullRequest};
use serde::de::DeserializeOwned;

const USAGE: &str = "usage: hygiene pr NUMBER | hygiene commit [REV] | hygiene book [DIR] \
                     | hygiene wrap [DIR] | hygiene ci [BASE]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["pr", number] => pr(number),
        ["commit"] => commit("HEAD"),
        ["commit", rev] => commit(rev),
        ["book"] => book("docs"),
        ["book", dir] => book(dir),
        ["wrap"] => wrap("docs/src"),
        ["wrap", dir] => wrap(dir),
        ["ci"] => ci("origin/main"),
        ["ci", base] => ci(base),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Checks pull request `number` and its issue.
fn pr(number: &str) -> ExitCode {
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        eprintln!("error: {number:?} is not the number of a pull request\n{USAGE}");
        return ExitCode::from(2);
    }
    let pr: PullRequest = match gh(&["pr", "view", number, "--json", "title,body,milestone"]) {
        Ok(pr) => pr,
        Err(e) => return tool_error(&e),
    };
    let issue: Option<Issue> = match hygiene::issue_number(&pr) {
        Some(n) => {
            let n = n.to_string();
            match gh(&["issue", "view", &n, "--json", "number,state,milestone"]) {
                Ok(issue) => Some(issue),
                Err(e) => return tool_error(&e),
            }
        }
        None => None,
    };
    report(
        &format!("pull request #{number}"),
        &hygiene::check_pr(&pr, issue.as_ref()),
        HYGIENE,
    )
}

/// Checks the message of commit `rev`.
fn commit(rev: &str) -> ExitCode {
    let out = match Command::new("git")
        .args(["log", "-1", "--format=%B", rev])
        .output()
    {
        Ok(out) => out,
        Err(e) => return tool_error(&format!("git: {e}")),
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return tool_error(&format!("git log {rev}: {}", stderr.trim()));
    }
    let message = String::from_utf8_lossy(&out.stdout);
    report(
        &format!("commit {rev}"),
        &hygiene::check_commit(&message),
        HYGIENE,
    )
}

/// Builds the book in `dir` with `mdbook build`, and checks its log and
/// each page in `dir/book`, except `print.html`: it repeats each page.
/// It installs the theme first when it is missing, and fails when the
/// install or the build changed a tracked file.
fn book(dir: &str) -> ExitCode {
    let before = tracked(dir);
    if let Err(e) = theme(dir) {
        return tool_error(&e);
    }
    let out = match Command::new("mdbook")
        .args(["build", dir])
        .env("NO_COLOR", "1")
        .output()
    {
        Ok(out) => out,
        Err(e) => return tool_error(&format!("mdbook build {dir}: {e}")),
    };
    let log = String::from_utf8_lossy(&out.stderr);
    eprint!("{log}");
    if !out.status.success() {
        return tool_error(&format!("mdbook build {dir}: {}", out.status));
    }
    let mut errors = hygiene::book::check_log(&log);
    let pages = Path::new(dir).join("book");
    let mut names = match std::fs::read_dir(&pages) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "html"))
            .filter(|p| p.file_name().is_some_and(|n| n != "print.html"))
            .collect::<Vec<_>>(),
        Err(e) => return tool_error(&format!("{}: {e}", pages.display())),
    };
    names.sort();
    for page in names {
        match std::fs::read_to_string(&page) {
            Ok(html) => {
                errors.extend(hygiene::book::check_page(
                    &page.display().to_string(),
                    &html,
                ));
            }
            Err(e) => return tool_error(&format!("{}: {e}", page.display())),
        }
    }
    errors.extend(hygiene::book::check_tracked(&before, &tracked(dir)));
    report(&format!("the book in {dir}"), &errors, BOOK)
}

/// Checks the wrap of each `.md` file in `dir`.
fn wrap(dir: &str) -> ExitCode {
    let mut pages = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect::<Vec<_>>(),
        Err(e) => return tool_error(&format!("{dir}: {e}")),
    };
    pages.sort();
    let mut errors = Vec::new();
    for page in pages {
        match std::fs::read_to_string(&page) {
            Ok(text) => errors.extend(hygiene::wrap::check(&page.display().to_string(), &text)),
            Err(e) => return tool_error(&format!("{}: {e}", page.display())),
        }
    }
    report(&format!("the wrap of the pages in {dir}"), &errors, WRAP)
}

/// Prints the recipe that `just ci` runs for the diff from the merge
/// base with `base` to stdout, and the set and the reason to stderr.
fn ci(base: &str) -> ExitCode {
    let changed = changed(base);
    let choice = hygiene::ci::choose(base, changed.as_deref());
    eprintln!("{choice}");
    println!("{}", choice.checks.recipe());
    ExitCode::SUCCESS
}

/// The files of this repository that differ from the merge base of
/// `HEAD` and `base`: committed, not committed and not tracked. Each
/// path is from the top of the repository. A rename gives its two
/// paths. `None` when git cannot compare.
fn changed(base: &str) -> Option<Vec<String>> {
    let git = |at: &Path, args: &[&str]| {
        let out = Command::new("git").arg("-C").arg(at).args(args).output();
        out.ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let top = git(Path::new("."), &["rev-parse", "--show-toplevel"])?;
    let top = PathBuf::from(top.trim_end_matches('\n'));
    let fork = git(&top, &["merge-base", "HEAD", base])?;
    let diff = git(
        &top,
        &["diff", "--name-only", "--no-renames", "-z", fork.trim()],
    )?;
    let new = git(&top, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut names: Vec<String> = diff
        .split('\0')
        .chain(new.split('\0'))
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    names.sort();
    names.dedup();
    Some(names)
}

/// Installs the theme in `dir/gruvbox` with `mdbook-gruvbox install`,
/// when `dir/book.toml` names the theme and the directory is missing.
/// The install can write `book.toml`, so this puts its bytes back
/// (01M3W5YW0172EVF2JA8T7WW392). mdbook reports a missing `book.toml`.
fn theme(dir: &str) -> Result<(), String> {
    let toml = Path::new(dir).join("book.toml");
    let Ok(saved) = std::fs::read(&toml) else {
        return Ok(());
    };
    if !hygiene::book::uses_theme(&String::from_utf8_lossy(&saved))
        || Path::new(dir).join("gruvbox").is_dir()
    {
        return Ok(());
    }
    let shown = format!("mdbook-gruvbox install {dir}");
    let out = Command::new("mdbook-gruvbox")
        .args(["install", dir])
        .output()
        .map_err(|e| format!("{shown}: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{shown}: {}", stderr.trim()));
    }
    if std::fs::read(&toml).ok().as_ref() != Some(&saved) {
        std::fs::write(&toml, &saved).map_err(|e| format!("{}: {e}", toml.display()))?;
    }
    Ok(())
}

/// The tracked files of the repository of `dir` that differ from `HEAD`,
/// each with a mark of its content. Empty when `dir` is in no git
/// repository, or when the repository has no commit.
fn tracked(dir: &str) -> hygiene::book::Tracked {
    let git = |at: &Path, args: &[&str]| {
        let out = Command::new("git").arg("-C").arg(at).args(args).output();
        out.ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let Some(top) = git(Path::new(dir), &["rev-parse", "--show-toplevel"]) else {
        return hygiene::book::Tracked::new();
    };
    let top = PathBuf::from(top.trim_end_matches('\n'));
    git(&top, &["diff", "HEAD", "--name-only", "-z"])
        .unwrap_or_default()
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(|name| (name.to_owned(), mark(&top.join(name))))
        .collect()
}

/// A mark of the content of the file `path`, or `gone` when it cannot be
/// read, for example a deleted file.
fn mark(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let mut hasher = DefaultHasher::new();
            bytes.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        }
        Err(_) => "gone".to_owned(),
    }
}

/// Runs `gh` and reads its JSON output.
fn gh<T: DeserializeOwned>(args: &[&str]) -> Result<T, String> {
    let shown = format!("gh {}", args.join(" "));
    let out = Command::new("gh")
        .args(args)
        .output()
        .map_err(|e| format!("{shown}: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{shown}: {}", stderr.trim()));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("{shown}: {e}"))
}

/// A tool failed: exit status 2.
fn tool_error(text: &str) -> ExitCode {
    eprintln!("error: {text}");
    ExitCode::from(2)
}

/// The end of the report of a broken rule of issue hygiene.
const HYGIENE: &str =
    "rule(s) of issue hygiene. See \"Check a pull request on GitHub\" in the book.";

/// The end of the report of a broken rule of the book check.
const BOOK: &str = "rule(s) of the book check. See \"Check the book\" in the book.";

/// The end of the report of a broken rule of the wrap check.
const WRAP: &str = "rule(s) of the wrap check. See \"Check the wrap of the book\" in the book.";

/// Prints each error, and then how many rules `what` breaks and where to
/// read about them (`rules`). Exit status 1 when there is one.
fn report(what: &str, errors: &[hygiene::Error], rules: &str) -> ExitCode {
    if errors.is_empty() {
        println!("ok: {what}");
        return ExitCode::SUCCESS;
    }
    for e in errors {
        eprintln!("error: {e}");
    }
    eprintln!("{what} breaks {} {rules}", errors.len());
    ExitCode::FAILURE
}
