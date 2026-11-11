//! The test archive of the Gate: the test binaries of the workspace,
//! built one time, and run in parts by other jobs.
//!
//! # Design
//!
//! CI compiles the code of a tree two times: one test build and one
//! release build, both in the job `Build` (01M6QQ1NXY0CXMZEEEP413W4PK).
//! `hygiene test-build DIR` is the test build. It does three things:
//!
//! 1. `cargo build --workspace --bins` and `cargo test --workspace
//!    --no-run` build each binary and each test binary of the workspace.
//!    [`artifacts`] reads the JSON messages of cargo, and [`Manifest`]
//!    keeps the path of each test binary, the folder of its crate, and
//!    the path of each binary.
//! 2. `cargo test --workspace --doc --no-fail-fast` runs the doc tests.
//!    rustdoc compiles each doc test when it runs, so a part of the
//!    tests cannot run them from an archive.
//! 3. `tar` packs the files into `DIR/tests.tar`, and `DIR/manifest.json`
//!    holds the [`Manifest`].
//!
//! The job `Build` keeps `DIR` as the artifact `tests-TREE`
//! (01M6QQ1P0GH9AGCDA5N8EMEFGF). Each job `Tests I/N` unpacks
//! `tests.tar` in its checkout, and runs `hygiene test-part I/N
//! --archive DIR` (01M6QQ1P5YCT0JX90X5FCKM4W1). That run starts no
//! cargo, and compiles nothing:
//!
//! ```mermaid
//! flowchart LR
//!     B["Build: cargo test --no-run<br/>and the doc tests"] --> A["tests-TREE:<br/>tests.tar, manifest.json"]
//!     A --> P1["Tests 1/4"]
//!     A --> P2["Tests 2/4"]
//!     A --> P3["Tests 3/4"]
//!     A --> P4["Tests 4/4"]
//!     P1 --> L["each test binary:<br/>--list, then --exact the names of the part"]
//! ```
//!
//! A test binary finds its files with paths that the build bakes in: the
//! checkout (`CARGO_MANIFEST_DIR`) and the folder of its own file
//! (`riff-server` beside `riff`). So the archive holds the paths from the
//! top of the checkout, and the manifest names that top. A run in a
//! checkout at another path fails ([`Manifest::check_root`]), and does
//! not run tests that read the wrong files.
//!
//! The part of a name is the same as in the run with cargo
//! ([`crate::part::Part::has`]). A name that two test binaries share
//! runs in each of them.
//!
//! CI builds at the same path on each runner: `deploy/session-test.sh`
//! binds the checkout at `/var/tmp/riff-ci/riff` in the container
//! (01M75NDR15DNBFHDBNKA3CA1BE). A verifier runs a test of the commit
//! from the archive of CI with `just ci-test NAME`
//! (01M75NDQY6BHZBV9PF735E3FGY): `deploy/ci-test.sh` takes the archive
//! of the tree that the newest CI run at `HEAD` built, for a pull request
//! the merge with `main` (01M75T9RGR89MAC6S78K42QN7Q). It puts a linked
//! worktree of `HEAD` at that path, unpacks the archive in it, and runs
//! `hygiene test-name NAME --archive DIR` there. It runs the tests whose
//! name has NAME ([`named`]), as `cargo test NAME` chooses them, and
//! compiles nothing:
//!
//! ```mermaid
//! flowchart LR
//!     C["clone of the verifier,<br/>HEAD at the commit"] --> D["gh run download<br/>tests-TREE"]
//!     D --> W["worktree of HEAD at<br/>/var/tmp/riff-ci/riff"]
//!     W --> U["tar -xf tests.tar"]
//!     U --> N["hygiene test-name NAME:<br/>--exact the names with NAME"]
//! ```
//!
//! ```
//! use hygiene::archive::{artifacts, Kind, Manifest};
//! use std::path::Path;
//!
//! let top = Path::new("/src/riff");
//! let messages = r#"
//! {"reason":"compiler-artifact","manifest_path":"/src/riff/crates/riff/Cargo.toml","target":{"kind":["test"],"name":"all"},"profile":{"test":true},"executable":"/src/riff/target/session/debug/deps/all-1f"}
//! {"reason":"compiler-artifact","manifest_path":"/src/riff/crates/riff/Cargo.toml","target":{"kind":["bin"],"name":"riff"},"profile":{"test":false},"executable":"/src/riff/target/session/debug/riff"}
//! {"reason":"compiler-artifact","manifest_path":"/root/.cargo/registry/src/serde/Cargo.toml","target":{"kind":["lib"],"name":"serde"},"profile":{"test":false},"executable":null}
//! {"reason":"build-finished","success":true}
//! "#;
//! let found = artifacts(messages, top);
//! assert_eq!(found.len(), 2);
//! assert_eq!(found[0].kind, Kind::Test);
//! assert_eq!(found[0].dir, "crates/riff");
//! assert_eq!(found[1].kind, Kind::Bin);
//!
//! let manifest = Manifest::new(top, found);
//! assert_eq!(manifest.tests[0].exe, "target/session/debug/deps/all-1f");
//! assert_eq!(manifest.bins, ["target/session/debug/riff"]);
//! assert!(manifest.check_root(Path::new("/src/riff")).is_ok());
//! assert!(manifest.check_root(Path::new("/work/riff")).is_err());
//! ```

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// What an artifact of cargo is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A test binary: a unit test or an integration test.
    Test,
    /// A binary of the workspace, for example `riff-server`.
    Bin,
}

/// A binary that cargo built in the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    /// A test binary, or a binary.
    pub kind: Kind,
    /// The name of its target, for example `all`.
    pub name: String,
    /// The folder of its crate, from the top of the checkout.
    pub dir: String,
    /// The path of its file, from the top of the checkout. It is the
    /// absolute path when the file is outside the checkout.
    pub exe: String,
}

/// The binaries of the workspace in the messages of
/// `cargo ... --message-format=json`: one JSON object on each line.
/// A line that is not JSON, and an artifact of a crate outside `top`,
/// are not binaries of the workspace. Each file is in the list one
/// time.
pub fn artifacts(messages: &str, top: &Path) -> Vec<Artifact> {
    #[derive(Deserialize)]
    struct Message {
        reason: String,
        #[serde(default)]
        manifest_path: String,
        #[serde(default)]
        target: Target,
        #[serde(default)]
        profile: Profile,
        #[serde(default)]
        executable: Option<String>,
    }
    #[derive(Deserialize, Default)]
    struct Target {
        #[serde(default)]
        kind: Vec<String>,
        #[serde(default)]
        name: String,
    }
    #[derive(Deserialize, Default)]
    struct Profile {
        #[serde(default)]
        test: bool,
    }
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    for line in messages.lines() {
        let Ok(message) = serde_json::from_str::<Message>(line) else {
            continue;
        };
        let Some(exe) = message.executable else {
            continue;
        };
        if message.reason != "compiler-artifact" {
            continue;
        }
        let kind = if message.profile.test {
            Kind::Test
        } else if message.target.kind.iter().any(|k| k == "bin") {
            Kind::Bin
        } else {
            continue;
        };
        let Some(dir) = Path::new(&message.manifest_path)
            .parent()
            .and_then(|dir| dir.strip_prefix(top).ok())
        else {
            continue;
        };
        let exe = relative(&exe, top);
        if !seen.insert(exe.clone()) {
            continue;
        }
        found.push(Artifact {
            kind,
            name: message.target.name,
            dir: dir.to_string_lossy().into_owned(),
            exe,
        });
    }
    found
}

/// The path `file` from `top`, or `file` when it is not under `top`.
fn relative(file: &str, top: &Path) -> String {
    match Path::new(file).strip_prefix(top) {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(_) => file.to_owned(),
    }
}

/// A test binary of the archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Test {
    /// The name of its target, for example `all`.
    pub name: String,
    /// The folder of its crate: the test runs there, as cargo runs it.
    pub dir: String,
    /// The path of its file, from the top of the checkout.
    pub exe: String,
}

/// What the archive holds. `manifest.json` is this value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The top of the checkout that built the archive.
    pub root: String,
    /// The test binaries.
    pub tests: Vec<Test>,
    /// The binaries of the workspace, from the top of the checkout. A
    /// test of one crate runs the binary of another crate.
    pub bins: Vec<String>,
}

impl Manifest {
    /// The manifest of `artifacts`, built in the checkout `top`. The
    /// lists are sorted, so the same tree gives the same manifest.
    pub fn new(top: &Path, artifacts: Vec<Artifact>) -> Manifest {
        let mut tests = Vec::new();
        let mut bins = Vec::new();
        for artifact in artifacts {
            match artifact.kind {
                Kind::Test => tests.push(Test {
                    name: artifact.name,
                    dir: artifact.dir,
                    exe: artifact.exe,
                }),
                Kind::Bin => bins.push(artifact.exe),
            }
        }
        tests.sort_by(|a, b| (&a.dir, &a.name, &a.exe).cmp(&(&b.dir, &b.name, &b.exe)));
        bins.sort();
        Manifest {
            root: top.to_string_lossy().into_owned(),
            tests,
            bins,
        }
    }

    /// Every file of the archive, from the top of the checkout.
    pub fn files(&self) -> Vec<&str> {
        self.tests
            .iter()
            .map(|t| t.exe.as_str())
            .chain(self.bins.iter().map(String::as_str))
            .collect()
    }

    /// Fails when `here` is not the top of the checkout that built the
    /// archive.
    pub fn check_root(&self, here: &Path) -> Result<(), String> {
        if Path::new(&self.root) == here {
            return Ok(());
        }
        Err(format!(
            "the archive was built in {}, and this folder is {}. A test binary reads the files of \
             the path that built it: unpack and run the archive in a checkout at that path",
            self.root,
            here.display()
        ))
    }

    /// Fails when a file of the archive is missing in the checkout
    /// `top`: the archive is not unpacked.
    pub fn check_files(&self, top: &Path) -> Result<(), String> {
        for file in self.files() {
            if !top.join(file).is_file() {
                return Err(format!(
                    "{file} is missing in {}. Unpack tests.tar there first",
                    top.display()
                ));
            }
        }
        Ok(())
    }
}

/// The line that `hygiene test-part --archive` prints before it runs
/// the tests: how many of the tests of the binaries it runs.
///
/// ```
/// use hygiene::archive::summary;
/// use hygiene::part::Part;
///
/// let part = Part { index: 2, count: 4 };
/// assert_eq!(
///     summary(part, 571, 2280),
///     "just test-part 2/4 runs 571 of 2280 tests of the archive"
/// );
/// ```
pub fn summary(part: crate::part::Part, run: usize, all: usize) -> String {
    format!("just test-part {part} runs {run} of {all} tests of the archive")
}

/// The names of `names` that have `filter` in them: the tests of a test
/// binary that `hygiene test-name FILTER --archive` runs, as `cargo test
/// FILTER` chooses them.
///
/// ```
/// use hygiene::archive::named;
///
/// let names = ["one::a".to_owned(), "one::b".to_owned(), "two::a".to_owned()];
/// assert_eq!(named(&names, "one::"), ["one::a", "one::b"]);
/// assert_eq!(named(&names, "two::a"), ["two::a"]);
/// assert!(named(&names, "three").is_empty());
/// ```
pub fn named<'a>(names: &'a [String], filter: &str) -> Vec<&'a str> {
    names
        .iter()
        .map(String::as_str)
        .filter(|name| name.contains(filter))
        .collect()
}

/// The line that `hygiene test-name` prints before it runs the tests:
/// how many of the tests of the binaries have `filter` in their name.
///
/// ```
/// use hygiene::archive::named_summary;
///
/// assert_eq!(
///     named_summary("one::", 2, 2280),
///     "just ci-test one:: runs 2 of 2280 tests of the archive"
/// );
/// ```
pub fn named_summary(filter: &str, run: usize, all: usize) -> String {
    format!("just ci-test {filter} runs {run} of {all} tests of the archive")
}

/// The error of `hygiene test-name` when no test of the archive has
/// `filter` in its name.
///
/// ```
/// use hygiene::archive::none_named;
///
/// assert_eq!(
///     none_named("three"),
///     "no test of the archive has \"three\" in its name. The archive holds no doc test: \
///      the job Build ran them"
/// );
/// ```
pub fn none_named(filter: &str) -> String {
    format!(
        "no test of the archive has {filter:?} in its name. The archive holds no doc test: the \
         job Build ran them"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(kind: &str, test: bool, manifest: &str, exe: &str) -> String {
        format!(
            "{{\"reason\":\"compiler-artifact\",\"manifest_path\":\"{manifest}\",\
             \"target\":{{\"kind\":[\"{kind}\"],\"name\":\"n\"}},\
             \"profile\":{{\"test\":{test}}},\"executable\":{exe}}}"
        )
    }

    #[test]
    fn a_build_script_a_lib_and_a_crate_outside_the_checkout_are_not_binaries() {
        let top = Path::new("/s");
        let text = [
            message(
                "custom-build",
                false,
                "/s/crates/a/Cargo.toml",
                "\"/s/t/b\"",
            ),
            message("lib", false, "/s/crates/a/Cargo.toml", "null"),
            message("test", true, "/r/x/Cargo.toml", "\"/s/t/x\""),
            "not json".to_owned(),
            "{\"reason\":\"build-finished\"}".to_owned(),
        ]
        .join("\n");
        assert_eq!(artifacts(&text, top), []);
    }

    #[test]
    fn a_file_is_in_the_list_one_time() {
        let top = Path::new("/s");
        let one = message("bin", false, "/s/crates/a/Cargo.toml", "\"/s/t/a\"");
        let found = artifacts(&format!("{one}\n{one}\n"), top);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn the_manifest_is_the_same_for_each_order_of_the_artifacts() {
        let top = Path::new("/s");
        let a = message("test", true, "/s/crates/b/Cargo.toml", "\"/s/t/b-1\"");
        let b = message("test", true, "/s/crates/a/Cargo.toml", "\"/s/t/a-2\"");
        let c = message("bin", false, "/s/crates/a/Cargo.toml", "\"/s/t/z\"");
        let d = message("bin", false, "/s/crates/a/Cargo.toml", "\"/s/t/y\"");
        let one = Manifest::new(
            top,
            artifacts(&[&a, &b, &c, &d].map(String::as_str).join("\n"), top),
        );
        let two = Manifest::new(
            top,
            artifacts(&[&d, &c, &b, &a].map(String::as_str).join("\n"), top),
        );
        assert_eq!(one, two);
        assert_eq!(one.bins, ["t/y", "t/z"]);
        assert_eq!(one.files(), ["t/a-2", "t/b-1", "t/y", "t/z"]);
    }

    #[test]
    fn the_manifest_round_trips_through_json() {
        let manifest = Manifest {
            root: "/s".into(),
            tests: vec![Test {
                name: "all".into(),
                dir: "crates/a".into(),
                exe: "t/all-1".into(),
            }],
            bins: vec!["t/a".into()],
        };
        let json = serde_json::to_string(&manifest).unwrap();
        assert_eq!(serde_json::from_str::<Manifest>(&json).unwrap(), manifest);
    }

    #[test]
    fn a_missing_file_fails_the_check() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest {
            root: dir.path().to_string_lossy().into_owned(),
            tests: vec![],
            bins: vec!["t/a".into()],
        };
        let e = manifest.check_files(dir.path()).unwrap_err();
        assert!(e.contains("t/a is missing"), "{e}");
        std::fs::create_dir_all(dir.path().join("t")).unwrap();
        std::fs::write(dir.path().join("t/a"), "").unwrap();
        assert!(manifest.check_files(dir.path()).is_ok());
    }
}
