//! `hygiene pr N` checks pull request N with `gh`. `hygiene commit [REV]`
//! checks the message of a commit with `git`. `hygiene book [DIR]` builds
//! the book in DIR with `mdbook` and checks it. See the library docs.

use std::path::Path;
use std::process::{Command, ExitCode};

use hygiene::{Issue, PullRequest};
use serde::de::DeserializeOwned;

const USAGE: &str = "usage: hygiene pr NUMBER | hygiene commit [REV] | hygiene book [DIR]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["pr", number] => pr(number),
        ["commit"] => commit("HEAD"),
        ["commit", rev] => commit(rev),
        ["book"] => book("docs"),
        ["book", dir] => book(dir),
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
fn book(dir: &str) -> ExitCode {
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
    report(&format!("the book in {dir}"), &errors, BOOK)
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
