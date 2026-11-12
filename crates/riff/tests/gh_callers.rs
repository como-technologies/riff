//! Each `gh` call of riff outside a test is counted under a caller name
//! that the tally knows (#1597). A `Gh::default()` counts nothing, so
//! `riff daemon status` would show its calls as the calls of "other
//! callers" (#1582). A `Gh::for_repo(..)` with no `.by(..)` counts as
//! "other" too (#1629). A `gh` that riff starts with no `Gh` counts
//! nothing, and in the daemon it can ask the keyring for an unlock prompt
//! (01M77AB7XZV8KG8RRZ8C6R5A8X). A `git` that talks to a remote can start
//! `gh` for a credential, so its function looks at the keyring first
//! (01M78TSH2VNHA75RR1HXC5Z27G). The code is the source of each crate: the
//! check reads the files, as `no_skill.rs` does.

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
const UNCOUNTED: [&str; 5] = [
    "Gh::default()",
    "Gh::at(",
    "Command::new(\"gh\")",
    "output(\"gh\"",
    "output_within(\"gh\"",
];

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

/// The start of a `Gh` for a repository, which counts as "other" until a
/// `.by(NAME)` names the caller.
const FOR_REPO: &str = "Gh::for_repo(";

/// The text of `rest` after the `)` that closes the `(` that `rest` starts
/// with.
fn after_parens(rest: &str) -> &str {
    let mut depth = 0;
    for (at, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[at + 1..];
                }
            }
            _ => {}
        }
    }
    ""
}

/// The names of the methods of the chain that starts `rest`: `.a(..)?.b`
/// gives `a` and `b`. The chain may span lines.
fn chain(mut rest: &str) -> Vec<&str> {
    let mut methods = Vec::new();
    loop {
        rest = rest.trim_start().trim_start_matches('?').trim_start();
        let Some(after) = rest.strip_prefix('.') else {
            return methods;
        };
        let end = after
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        methods.push(&after[..end]);
        rest = &after[end..];
        if rest.starts_with('(') {
            rest = after_parens(rest);
        }
    }
}

/// Each `Gh::for_repo(..)` in the code of `source` outside its tests whose
/// chain of methods has no `.by(..)`: the number of the line, and the text
/// of the line.
fn nameless(source: &str) -> Vec<(usize, String)> {
    let mut code = String::new();
    let mut starts = Vec::new();
    for (n, line) in outside_tests(source) {
        starts.push((code.len(), n, line));
        code.push_str(line);
        code.push('\n');
    }
    let mut out = Vec::new();
    for (at, _) in code.match_indices(FOR_REPO) {
        let rest = &code[at + FOR_REPO.len() - 1..];
        if !chain(after_parens(rest)).contains(&"by") {
            let (_, n, line) = starts[starts.partition_point(|(start, ..)| *start <= at) - 1];
            out.push((n, line.trim().to_owned()));
        }
    }
    out
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
    fn f() { run::output_within(\"gh\", dir, &[\"auth\", \"token\"], WAIT); }
}
";
    assert_eq!(
        uncounted(source),
        [
            (4, "fn a() { let gh = Gh::default(); }".to_owned()),
            (18, "fn e() { Command::new(\"gh\"); }".to_owned()),
            (
                19,
                "fn f() { run::output_within(\"gh\", dir, &[\"auth\", \"token\"], WAIT); }"
                    .to_owned()
            ),
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
fn the_check_reads_the_chain_of_a_for_repo() {
    let source = "\
fn a() { Gh::for_repo(repo).by(\"sweep\") }
fn b() { Gh::for_repo(repo) }
fn c() {
    let gh = pr::Gh::for_repo(&format!(\"{}/{}\", owner, name))
        .with_limit(LIMIT)
        .by(\"wave\");
}
fn d() {
    let gh = Gh::for_repo(repo)
        .with_limit(LIMIT);
    gh.by(\"look\");
}
// Gh::for_repo(repo)
#[cfg(test)]
fn e() {
    Gh::for_repo(repo)
}
fn f() { Gh::for_repo(repo).with_limit(by(1)) }
fn g() { Gh::for_repo(repo)?.by(\"look\") }
";
    assert_eq!(
        nameless(source),
        [
            (2, "fn b() { Gh::for_repo(repo) }".to_owned()),
            (9, "let gh = Gh::for_repo(repo)".to_owned()),
            (
                18,
                "fn f() { Gh::for_repo(repo).with_limit(by(1)) }".to_owned()
            ),
        ]
    );
}

#[test]
fn each_for_repo_outside_a_test_has_a_caller_name() {
    let mut found = Vec::new();
    for path in sources() {
        let source = fs::read_to_string(&path).unwrap();
        for (n, line) in nameless(&source) {
            found.push(format!("{}:{n}: {line}", path.display()));
        }
    }
    assert!(
        found.is_empty(),
        "give each of these a caller name: Gh::for_repo(REPO).by(NAME), with NAME one of \
         DAEMON:\n{}",
        found.join("\n")
    );
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

#[test]
fn the_guide_names_no_call_that_counts_as_other() {
    let other = riff::ghcalls::OTHER;
    assert!(!CALLERS.contains(&other) && !DAEMON.contains(&other));
    assert!(
        !limit_section().contains(&format!("`{other}`")),
        "the section \"See how the daemon uses the GitHub limit\" names a caller `{other}`: \
         each call of riff has a name"
    );
}

/// The starts of the arguments of a `git` that talks to a remote. Such a
/// `git` can start `gh` for a credential (01M78TSH2VNHA75RR1HXC5Z27G).
const REMOTE_GIT: [&str; 6] = [
    "[\"fetch\"",
    "[\"push\"",
    "[\"clone\"",
    "[\"ls-remote\"",
    "[\"pull\"",
    "\"git\", \"fetch\"",
];

/// The calls that look at the keyring before a `git` that talks to a
/// remote: `git_credential::run`, `git_credential::keyring_open`, and the
/// `remote_git` of a module that calls `keyring_open`.
const GUARDS: [&str; 2] = ["git_credential::", "remote_git("];

/// The places of a remote `git` that need no look at the keyring: a file,
/// and a text of the line or of its function. Each runs only in a command
/// that a person types, or starts no `git`.
const NO_LOOK: [(&str, &str); 6] = [
    // `riff claim` and `riff run`: the earlier work of an item.
    ("dropped.rs", "pub async fn fetch("),
    // `riff cloud create`.
    ("cloud.rs", "fn fetch("),
    // `riff project init`, which an agent session cannot run.
    ("project.rs", "pub fn pull("),
    // `podman pull`.
    ("podman.rs", "[\"pull\", image]"),
    // The subcommands of `gh`.
    ("ghcalls.rs", "(\"label\", &[\"clone\""),
    // `git ls-remote --get-url` reads the config, and talks to no remote.
    ("git_credential.rs", "pub fn urls("),
];

/// True when the line `line` starts a function.
fn starts_fn(line: &str) -> bool {
    let mut code = line.trim_start();
    for word in [
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "async ",
        "const ",
        "unsafe ",
    ] {
        code = code.strip_prefix(word).unwrap_or(code);
    }
    code.starts_with("fn ")
}

/// The lines of the function of `source` around the line `at`, from 0:
/// its first line, and its lines up to the `}` that closes it.
fn function_of(source: &str, at: usize) -> Vec<&str> {
    let lines: Vec<&str> = source.lines().collect();
    let Some(start) = (0..=at).rev().find(|&n| starts_fn(lines[n])) else {
        return vec![lines[at]];
    };
    let indent = |line: &str| line.len() - line.trim_start().len();
    let end = (start + 1..lines.len())
        .find(|&n| {
            indent(lines[n]) == indent(lines[start]) && lines[n].trim_start().starts_with('}')
        })
        .unwrap_or(lines.len() - 1);
    lines[start..=end.max(at)].to_vec()
}

/// Each line of `source` outside its tests that starts a `git` that
/// talks to a remote in a function with no look at the keyring, and is
/// not on the list `no_look`: the number of the line, and the text.
fn unguarded(source: &str, no_look: &[&str]) -> Vec<(usize, String)> {
    outside_tests(source)
        .into_iter()
        .filter(|(_, line)| REMOTE_GIT.iter().any(|start| line.contains(start)))
        .filter(|(n, line)| {
            let function = function_of(source, n - 1);
            let guarded = function
                .iter()
                .any(|l| GUARDS.iter().any(|guard| l.contains(guard)));
            let allowed = no_look
                .iter()
                .any(|text| line.contains(text) || function[0].contains(text));
            !guarded && !allowed
        })
        .map(|(n, line)| (n, line.trim().to_owned()))
        .collect()
}

#[test]
fn the_check_finds_a_remote_git_with_no_look_at_the_keyring() {
    let source = "\
fn a(dir: &Path) {
    run::output(\"git\", dir, &[\"fetch\", \"-q\", \"origin\"])?;
}
fn b(dir: &Path) {
    git_credential::run(dir, \"origin\", &[\"fetch\", \"-q\", \"origin\"])?;
}
fn c(dir: &Path) {
    let push = [\"push\", \"origin\"];
    crate::git_credential::keyring_open(dir, \"origin\", &push)?;
    git(&push)?;
}
fn d(&self) {
    self.remote_git(&self.dir, &[\"ls-remote\", \"origin\"])?;
}
pub fn person(dir: &Path) {
    git(dir, &[\"pull\", \"origin\"])?;
}
// git(dir, &[\"clone\", url])
#[cfg(test)]
mod tests {
    fn e() { git(&[\"push\", \"origin\"]); }
}
fn f() {
    let args = [\"GIT_TERMINAL_PROMPT=0\", \"git\", \"fetch\", \"-q\"];
}
";
    assert_eq!(
        unguarded(source, &["pub fn person("]),
        [
            (
                2,
                "run::output(\"git\", dir, &[\"fetch\", \"-q\", \"origin\"])?;".to_owned()
            ),
            (
                24,
                "let args = [\"GIT_TERMINAL_PROMPT=0\", \"git\", \"fetch\", \"-q\"];".to_owned()
            ),
        ]
    );
}

#[test]
fn each_git_that_talks_to_a_remote_looks_at_the_keyring_first() {
    let mut found = Vec::new();
    for path in sources() {
        let source = fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        let no_look: Vec<&str> = NO_LOOK
            .iter()
            .filter(|(file, _)| *file == name)
            .map(|(_, text)| *text)
            .collect();
        for (n, line) in unguarded(&source, &no_look) {
            found.push(format!("{}:{n}: {line}", path.display()));
        }
    }
    assert!(
        found.is_empty(),
        "each git that talks to a remote can start gh for a credential: in the daemon, run it \
         with git_credential::run, or call git_credential::keyring_open first \
         (01M78TSH2VNHA75RR1HXC5Z27G):\n{}",
        found.join("\n")
    );
}
