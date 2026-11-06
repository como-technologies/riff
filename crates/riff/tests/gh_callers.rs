//! Each `gh` call of riff outside a test is counted under a caller name
//! that the tally knows (#1597). A `Gh::default()` counts nothing, so
//! `riff daemon status` would show its calls as the calls of "other
//! callers" (#1582). The code is the source of each crate: the check reads
//! the files, as `no_skill.rs` does.

use std::fs;
use std::path::{Path, PathBuf};

use riff::ghcalls::{CALLERS, DAEMON};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The crates whose code ships in the binaries of riff. `hygiene`,
/// `real-session`, `reqs` and `isolated` are tools of this repository.
const CRATES: [&str; 5] = [
    "riff",
    "riff-client",
    "riff-core",
    "riff-server",
    "riff-state",
];

/// The ways to get a `gh` that counts nothing.
const UNCOUNTED: [&str; 3] = ["Gh::default()", "Gh::at(", "Command::new(\"gh\")"];

/// Each file of `dir` with the extension `ext`, and of its folders.
fn files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files(&path, ext, out);
        } else if path.extension().is_some_and(|x| x == ext) {
            out.push(path);
        }
    }
}

/// The source files of the crates that ship.
fn sources() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for name in CRATES {
        files(
            &repo().join("crates").join(name).join("src"),
            "rs",
            &mut paths,
        );
    }
    assert!(paths.len() > 50, "{} files", paths.len());
    paths
}

/// The lines of the code of `source` outside its tests, with their
/// numbers: not a comment, and not an item behind `#[cfg(test)]` (a
/// module, a function, a use). The item ends at the `}` that closes it
/// at the indent of the attribute, or at the `;` of its first line.
fn outside_tests(source: &str) -> Vec<(usize, &str)> {
    let indent = |line: &str| line.len() - line.trim_start().len();
    let mut out = Vec::new();
    let mut lines = source.lines().enumerate();
    while let Some((n, line)) = lines.next() {
        let code = line.trim_start();
        if code == "#[cfg(test)]" {
            let at = indent(line);
            let mut item = lines
                .by_ref()
                .map(|(_, l)| l)
                .skip_while(|l| l.trim_start().starts_with("#["));
            if let Some(first) = item.next()
                && !first.trim_end().ends_with(';')
            {
                for l in item {
                    if indent(l) == at && l.trim_start().starts_with('}') {
                        break;
                    }
                }
            }
        } else if !code.starts_with("//") {
            out.push((n + 1, line));
        }
    }
    out
}

/// Each line of `source` outside its tests that makes a `gh` that counts
/// nothing: the number of the line, and the text.
fn uncounted(source: &str) -> Vec<(usize, String)> {
    outside_tests(source)
        .into_iter()
        .filter(|(_, line)| UNCOUNTED.iter().any(|way| line.contains(way)))
        .map(|(n, line)| (n, line.trim().to_owned()))
        .collect()
}

/// Each caller name that the code of `source` outside its tests gives to a
/// `gh`: `Gh::here("NAME")`, `Gh::in_daemon("NAME")` and `.by("NAME")`.
fn names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (_, line) in outside_tests(source) {
        for start in ["Gh::here(\"", "Gh::in_daemon(\"", ".by(\""] {
            for (at, _) in line.match_indices(start) {
                let rest = &line[at + start.len()..];
                out.extend(rest.split_once('"').map(|(name, _)| name.to_owned()));
            }
        }
    }
    out
}

#[test]
fn the_check_reads_the_code_and_not_the_tests() {
    let source = "\
use crate::pr::Gh;
// let gh = Gh::default();
/// let gh = Gh::default();
fn a() { let gh = Gh::default(); }
#[cfg(test)]
use crate::pr::Gh as Test;
fn b() { let gh = Gh::here(\"top\"); }
#[cfg(test)]
mod tests {
    fn c() { let gh = Gh::default(); }
}
impl X {
    #[cfg(test)]
    #[allow(dead_code)]
    fn d() {
        Gh::at(\"gh\");
    }
    fn e() { Command::new(\"gh\"); }
}
";
    assert_eq!(
        uncounted(source),
        [
            (4, "fn a() { let gh = Gh::default(); }".to_owned()),
            (18, "fn e() { Command::new(\"gh\"); }".to_owned()),
        ]
    );
    assert_eq!(names(source), ["top"]);
}

#[test]
fn the_check_reads_the_names_of_the_callers() {
    let source = "let a = Gh::here(\"pr wait\"); let b = Gh::in_daemon(\"clone\").by(\"look\");\n\
                  // Gh::here(\"nothing\")\n";
    assert_eq!(names(source), ["pr wait", "clone", "look"]);
}

#[test]
fn no_gh_outside_a_test_counts_nothing() {
    let mut found = Vec::new();
    for path in sources() {
        let source = fs::read_to_string(&path).unwrap();
        for (n, line) in uncounted(&source) {
            found.push(format!("{}:{n}: {line}", path.display()));
        }
    }
    assert!(
        found.is_empty(),
        "give each of these a counted gh with a caller name: Gh::here(NAME) in a command, \
         Gh::in_daemon(NAME) or Gh::for_repo(REPO).by(NAME) in the daemon:\n{}",
        found.join("\n")
    );
}

#[test]
fn each_caller_name_is_one_that_the_tally_knows() {
    let mut given = Vec::new();
    for path in sources() {
        let source = fs::read_to_string(&path).unwrap();
        for name in names(&source) {
            let known = CALLERS.contains(&name.as_str()) || DAEMON.contains(&name.as_str());
            assert!(
                known,
                "{}: the tally knows no caller \"{name}\"",
                path.display()
            );
            given.push(name);
        }
    }
    // The check sees the code: each caller that has a name is there.
    for name in CALLERS
        .iter()
        .chain(DAEMON)
        .filter(|name| **name != "session")
    {
        assert!(given.iter().any(|g| g == name), "no code gives \"{name}\"");
    }
}

/// The section "See how the daemon uses the GitHub limit" of the
/// development guide.
fn limit_section() -> String {
    let text = fs::read_to_string(repo().join("docs/src/development.md")).unwrap();
    let start = text
        .find("#### See how the daemon uses the GitHub limit")
        .expect("the section is in the guide");
    let rest = &text[start..];
    let end = rest[5..].find("\n#").map_or(rest.len(), |at| at + 5);
    rest[..end].to_owned()
}

#[test]
fn the_guide_names_each_caller_of_the_tally() {
    let section = limit_section();
    for name in CALLERS.iter().chain(DAEMON) {
        assert!(
            section.contains(&format!("`{name}`")),
            "the section \"See how the daemon uses the GitHub limit\" names no caller `{name}`"
        );
    }
}
