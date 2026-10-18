//! Finds the crates of the terminal in the dependencies of a crate
//! (01M54216PBE319RASGNMY9F0MR). `riff-client` holds no terminal code:
//! the test `riff_client_has_no_terminal_crate` runs [`found`] on its
//! `Cargo.toml` and on the `Cargo.lock` of the workspace.
//!
//! The roots are the normal dependencies of the manifest, also those of
//! a target and the build dependencies: not its dev dependencies. Then
//! [`found`] follows each root through the lock file, so a crate of the
//! terminal that comes through another crate counts too.
//!
//! ```
//! let manifest = "[dependencies]\nratatui.workspace = true\n";
//! assert_eq!(hygiene::terminal::found(manifest, ""), ["ratatui"]);
//!
//! let manifest = "[dependencies]\nui = \"1\"\n\n[dev-dependencies]\nclap = \"4\"\n";
//! let lock = "[[package]]\nname = \"ui\"\ndependencies = [\n \"crossterm 0.29.0\",\n]\n";
//! assert_eq!(hygiene::terminal::found(manifest, lock), ["crossterm"]);
//! ```

use std::collections::{BTreeMap, BTreeSet};

/// The crates of the terminal.
pub const TERMINAL: [&str; 4] = ["ratatui", "crossterm", "clap", "rustyline"];

/// The crates of [`TERMINAL`] that the crate of `manifest` (its
/// `Cargo.toml`) depends on, through the packages of `lock` (a
/// `Cargo.lock`). Sorted, each one time.
pub fn found(manifest: &str, lock: &str) -> Vec<String> {
    let graph = packages(lock);
    let mut seen = BTreeSet::new();
    let mut next: Vec<String> = dependencies(manifest);
    while let Some(name) = next.pop() {
        if seen.insert(name.clone())
            && let Some(more) = graph.get(&name)
        {
            next.extend(more.iter().cloned());
        }
    }
    seen.into_iter()
        .filter(|name| TERMINAL.contains(&name.as_str()))
        .collect()
}

/// The names of the normal and the build dependencies of `manifest`,
/// also those of a target. Not its dev dependencies.
///
/// ```
/// let manifest = "[package]\nname = \"a\"\n\n[dependencies]\nb.workspace = true\n\
///     c = { path = \"../c\" }\n\n[dev-dependencies]\nd = \"1\"\n\n\
///     [target.'cfg(unix)'.dependencies]\ne = \"1\"\n\n[dependencies.f]\nversion = \"1\"\n";
/// assert_eq!(hygiene::terminal::dependencies(manifest), ["b", "c", "e", "f"]);
/// ```
pub fn dependencies(manifest: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_table = false;
    for line in manifest.lines().map(str::trim) {
        if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_end_matches(']');
            let (table, crate_name) = match header.rsplit_once("dependencies.") {
                Some((table, name)) => (format!("{table}dependencies"), Some(name)),
                None => (header.to_owned(), None),
            };
            let normal = (table == "dependencies" || table.ends_with(".dependencies"))
                || table == "build-dependencies"
                || table.ends_with(".build-dependencies");
            match crate_name {
                Some(name) if normal => {
                    names.push(name.trim_matches('"').to_owned());
                    in_table = false;
                }
                _ => in_table = normal,
            }
            continue;
        }
        if !in_table || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = line.split_once('=') {
            let name = key.trim().split('.').next().unwrap_or_default();
            names.push(name.trim_matches('"').to_owned());
        }
    }
    names
}

/// Each package of `lock` with the names of its dependencies.
fn packages(lock: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut name = String::new();
    let mut in_list = false;
    for line in lock.lines().map(str::trim) {
        if line == "[[package]]" {
            name.clear();
            in_list = false;
        } else if let Some(value) = line.strip_prefix("name = ") {
            name = value.trim_matches('"').to_owned();
            graph.entry(name.clone()).or_default();
        } else if line.starts_with("dependencies = [") {
            in_list = true;
        } else if in_list && line == "]" {
            in_list = false;
        } else if in_list {
            let dependency = line.trim_matches(|c| c == '"' || c == ',');
            let dependency = dependency.split(' ').next().unwrap_or_default();
            graph
                .entry(name.clone())
                .or_default()
                .insert(dependency.to_owned());
        }
    }
    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crate_of_the_terminal_through_two_crates_counts() {
        let manifest = "[dependencies]\na = \"1\"\n";
        let lock = concat!(
            "[[package]]\nname = \"a\"\nversion = \"1.0.0\"\ndependencies = [\n \"b\",\n]\n\n",
            "[[package]]\nname = \"b\"\nversion = \"1.0.0\"\ndependencies = [\n",
            " \"rustyline 18.0.1\",\n \"serde\",\n]\n\n",
            "[[package]]\nname = \"rustyline\"\nversion = \"18.0.1\"\n",
        );
        assert_eq!(found(manifest, lock), ["rustyline"]);
    }

    #[test]
    fn a_dev_dependency_does_not_count() {
        let manifest = "[dependencies]\na = \"1\"\n\n[dev-dependencies]\nratatui = \"0.30\"\n";
        assert!(found(manifest, "").is_empty());
    }

    #[test]
    fn a_build_dependency_and_a_target_dependency_count() {
        let manifest = "[build-dependencies]\nclap = \"4\"\n\n\
                        [target.'cfg(unix)'.dependencies]\ncrossterm = \"0.29\"\n";
        assert_eq!(found(manifest, ""), ["clap", "crossterm"]);
    }
}
