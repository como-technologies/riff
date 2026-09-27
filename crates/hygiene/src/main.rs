//! `hygiene pr N` checks pull request N with `gh`. `hygiene commit [REV]`
//! checks the message of a commit with `git`. See the library docs.

use std::process::{Command, ExitCode};

use hygiene::{Issue, PullRequest};
use serde::de::DeserializeOwned;

const USAGE: &str = "usage: hygiene pr NUMBER | hygiene commit [REV]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["pr", number] => pr(number),
        ["commit"] => commit("HEAD"),
        ["commit", rev] => commit(rev),
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
    report(&format!("commit {rev}"), &hygiene::check_commit(&message))
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

/// Prints each error. Exit status 1 when there is one.
fn report(what: &str, errors: &[hygiene::Error]) -> ExitCode {
    if errors.is_empty() {
        println!("ok: {what}");
        return ExitCode::SUCCESS;
    }
    for e in errors {
        eprintln!("error: {e}");
    }
    eprintln!(
        "{what} breaks {} rule(s) of issue hygiene. See \"Check a pull request on GitHub\" in the book.",
        errors.len()
    );
    ExitCode::FAILURE
}
