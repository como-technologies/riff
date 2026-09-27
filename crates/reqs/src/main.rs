//! `reqs rid` prints a new requirement ID. `reqs check [ROOT]` checks
//! the requirement IDs of a repository. See the library docs.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The files and directories that can cite a requirement.
const ROOTS: &[&str] = &["CLAUDE.md", "crates", "docs/src", "deploy", "justfile"];

/// Directories that hold no citations of their own: build output, and
/// this crate, whose docs and tests cite IDs that do not exist.
const SKIP: &[&str] = &["target", "book", "gruvbox", "reqs"];

/// The text files to read.
const EXTENSIONS: &[&str] = &["rs", "md", "toml", "json", "sh", "just", "yml", "yaml"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["rid"] => {
            println!("{}", reqs::new_id());
            ExitCode::SUCCESS
        }
        ["check"] => run_check(Path::new(".")),
        ["check", root] => run_check(Path::new(root)),
        _ => {
            eprintln!("usage: reqs rid | reqs check [ROOT]");
            ExitCode::from(2)
        }
    }
}

fn run_check(root: &Path) -> ExitCode {
    let requirements = match std::fs::read_to_string(root.join(reqs::REQUIREMENTS)) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("error: {}: {e}", reqs::REQUIREMENTS);
            return ExitCode::FAILURE;
        }
    };
    let mut files = Vec::new();
    for start in ROOTS {
        collect(&root.join(start), &mut files);
    }
    let texts: Vec<(String, String)> = files
        .iter()
        .filter(|p| !p.ends_with(reqs::REQUIREMENTS))
        .filter_map(|p| {
            let text = std::fs::read_to_string(p).ok()?;
            let shown = p.strip_prefix(root).unwrap_or(p);
            Some((shown.display().to_string(), text))
        })
        .collect();
    let sources: Vec<(&str, &str)> = texts
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let report = reqs::check(&requirements, &sources);
    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    for e in &report.errors {
        eprintln!("error: {e}");
    }
    if report.errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Adds each text file under `path` to `files`, in name order.
fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        let text = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXTENSIONS.contains(&e))
            || path.file_name().is_some_and(|n| n == "justfile");
        if text {
            files.push(path.to_path_buf());
        }
        return;
    }
    let skip = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| SKIP.contains(&n));
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    if skip {
        return;
    }
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| Some(e.ok()?.path())).collect();
    entries.sort();
    for entry in entries {
        collect(&entry, files);
    }
}
