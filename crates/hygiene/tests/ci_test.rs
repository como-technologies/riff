//! `just ci-test NAME`: a verifier runs a test of the commit from the
//! test binaries that CI built, and compiles nothing
//! (01M75NDQY6BHZBV9PF735E3FGY). The tests run the recipe in a clone of
//! one commit with the real `justfile`, `deploy/ci-test.sh` and
//! `deploy/artifact-run.sh`. A fake `gh` holds a fake CI: the artifact
//! `tests-TREE` with a manifest, and a tar of the `hygiene` of this build
//! and a fake test binary. A fake `cargo` logs each call and fails.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use hygiene::archive::{Manifest, Test};

fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The names that the fake test binary lists.
const LIST: &str = "one::named_test: test\\none::other: test\\ntwo::x: test\\n";

/// A clone of one commit, the fake CI and its fake tools in one folder:
///
/// - `clone`: the clone, with the scripts and the `justfile`;
/// - `bin`: the fake `gh` and `cargo`, which log to `bin/calls`;
/// - `art`: the files of the artifact, `manifest.json` and `tests.tar`;
/// - `ci/riff`: the path that the archive names.
///
/// The fake test binary writes each run to `run.log`: its folder, the
/// values of the environment that the jobs Tests I/N set, and its
/// arguments. It exits with the number in the file `exit`, when it is
/// there.
struct Fake {
    dir: tempfile::TempDir,
}

impl Fake {
    fn new() -> Fake {
        Fake::with_root(None)
    }

    /// A fake CI whose archive names `root`, else `ci/riff` of the folder.
    fn with_root(root: Option<&Path>) -> Fake {
        let fake = Fake {
            dir: tempfile::tempdir().unwrap(),
        };
        let root = root.map_or_else(|| fake.root(), Path::to_owned);
        let clone = fake.clone_dir();
        for file in ["deploy/ci-test.sh", "deploy/artifact-run.sh"] {
            std::fs::create_dir_all(clone.join(file).parent().unwrap()).unwrap();
            isolated::copy_program(top().join(file), clone.join(file));
        }
        let justfile = std::fs::read_to_string(top().join("justfile")).unwrap();
        std::fs::write(clone.join("justfile"), justfile).unwrap();
        std::fs::write(clone.join(".gitignore"), "/target\n").unwrap();
        std::fs::create_dir_all(clone.join("crates/a")).unwrap();
        std::fs::write(clone.join("crates/a/Cargo.toml"), "").unwrap();
        fake.git(&["init", "-q"]);
        fake.git(&["add", "-A"]);
        fake.git(&["commit", "-q", "-m", "a"]);

        let bin = fake.path("bin");
        std::fs::create_dir_all(&bin).unwrap();
        isolated::write_program(
            bin.join("gh"),
            format!(
                "#!/bin/sh\ndir=$(dirname \"$0\")\necho \"gh $*\" >> \"$dir/calls\"\n\
                 case \"$1 $2\" in\n\
                 'api repos/o/r/actions/artifacts?name='*)\n\
                   name=${{2#*name=}}; name=${{name%%&*}}\n\
                   [ -f \"$dir/art-$name\" ] && cat \"$dir/art-$name\"\n\
                   exit 0 ;;\n\
                 'run download')\n\
                   while [ $# -gt 0 ]; do [ \"$1\" = --dir ] && d=$2; shift; done\n\
                   mkdir -p \"$d\" && cp '{art}/manifest.json' '{art}/tests.tar' \"$d/\"; exit 0 ;;\n\
                 esac\n\
                 echo \"no such call: $*\" >&2; exit 1\n",
                art = fake.path("art").display(),
            ),
        );
        isolated::write_program(
            bin.join("cargo"),
            "#!/bin/sh\necho \"cargo $*\" >> \"$(dirname \"$0\")/calls\"\nexit 1\n",
        );

        // The files of the archive, from the top of the checkout of CI.
        let stage = fake.path("stage");
        let deps = stage.join("target/session/debug/deps");
        std::fs::create_dir_all(&deps).unwrap();
        isolated::copy_program(
            env!("CARGO_BIN_EXE_hygiene"),
            stage.join("target/session/debug/hygiene"),
        );
        isolated::write_program(
            deps.join("all-1f"),
            format!(
                "#!/bin/sh\ncase \" $* \" in\n\
                 *' --list '*) printf '{LIST}' ;;\n\
                 *) {{ echo --run--; pwd; for v in HOME CARGO_HOME CARGO_TARGET_DIR TMPDIR \
                 CARGO_MANIFEST_DIR RIFF_SERVER LEAK; do eval \"echo $v=\\$$v\"; done; \
                 for a in \"$@\"; do printf '%s\\n' \"$a\"; done; }} >> '{log}'\n\
                 if [ -f '{exit}' ]; then exit \"$(cat '{exit}')\"; fi ;;\n\
                 esac\n",
                log = fake.path("run.log").display(),
                exit = fake.path("exit").display(),
            ),
        );
        let art = fake.path("art");
        std::fs::create_dir_all(&art).unwrap();
        let manifest = Manifest {
            root: root.to_string_lossy().into_owned(),
            tests: vec![Test {
                name: "all".into(),
                dir: "crates/a".into(),
                exe: "target/session/debug/deps/all-1f".into(),
            }],
            bins: vec!["target/session/debug/hygiene".into()],
        };
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        std::fs::write(art.join("manifest.json"), json + "\n").unwrap();
        let tar = Command::new("tar")
            .arg("-cf")
            .arg(art.join("tests.tar"))
            .arg("-C")
            .arg(&stage)
            .args(manifest.files())
            .output()
            .unwrap();
        assert!(tar.status.success(), "{tar:?}");
        fake
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().canonicalize().unwrap().join(name)
    }

    fn clone_dir(&self) -> PathBuf {
        self.path("clone")
    }

    fn root(&self) -> PathBuf {
        self.path("ci/riff")
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@e.test"])
            .args(args)
            .current_dir(self.clone_dir())
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn tree(&self) -> String {
        self.git(&["rev-parse", "HEAD^{tree}"])
    }

    /// The fake CI holds the artifact of the tree of HEAD, from run 77.
    fn keep_artifact(&self) {
        let name = format!("art-tests-{}", self.tree());
        std::fs::write(self.path("bin").join(name), "77\n").unwrap();
    }

    /// Runs `just ci-test NAME` in the clone, with a value `LEAK` that
    /// no test may see.
    fn ci_test(&self, name: &str) -> Output {
        let path = format!(
            "{}:{}",
            self.path("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new("just")
            .args(["ci-test", name])
            .current_dir(self.clone_dir())
            .env("PATH", path)
            .env("GH_REPO", "o/r")
            .env("LEAK", "1")
            .output()
            .unwrap()
    }

    fn calls(&self) -> Vec<String> {
        lines(&self.path("bin").join("calls"))
    }

    /// The runs of the fake test binary: the lines of each run.
    fn runs(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(self.path("run.log"))
            .unwrap_or_default()
            .split("--run--\n")
            .filter(|run| !run.is_empty())
            .map(|run| run.lines().map(str::to_owned).collect())
            .collect()
    }
}

fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux {
    use super::*;

    /// The done-when of #1947: the recipe runs one test of the artifact
    /// `tests-TREE` of a fake CI by its name, at the path of the archive,
    /// in the environment of the jobs Tests I/N, and runs no cargo.
    #[test]
    fn the_recipe_runs_one_test_of_the_artifact_of_ci_and_builds_nothing() {
        let fake = Fake::new();
        fake.keep_artifact();
        let out = fake.ci_test("named_test");
        assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
        assert!(
            stderr(&out).contains("just ci-test named_test runs 1 of 3 tests of the archive\n"),
            "{}",
            stderr(&out)
        );

        let root = fake.root();
        let krate = root.join("crates/a").display().to_string();
        let target = root.join("target/session").display().to_string();
        let home = fake.path("ci/ci-test-home").display().to_string();
        assert_eq!(
            fake.runs(),
            [[
                krate.clone(),
                format!("HOME={home}"),
                format!("CARGO_HOME={target}/cargo"),
                format!("CARGO_TARGET_DIR={target}"),
                "TMPDIR=/var/tmp".to_owned(),
                format!("CARGO_MANIFEST_DIR={krate}"),
                "RIFF_SERVER=http://127.0.0.1:9".to_owned(),
                "LEAK=".to_owned(),
                "--exact".to_owned(),
                "one::named_test".to_owned(),
            ]]
        );
        let calls = fake.calls();
        assert!(!calls.iter().any(|c| c.starts_with("cargo")), "{calls:?}");
        let download = format!(
            "gh run download 77 --name tests-{tree} --dir {}/target/ci-test/{tree}.part",
            fake.clone_dir().display(),
            tree = fake.tree()
        );
        assert!(calls.contains(&download), "{calls:?}");
        // The checkout at the path of the archive is the commit of HEAD.
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&root)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&head.stdout).trim(),
            fake.git(&["rev-parse", "HEAD"])
        );
    }

    /// A second run of the same tree takes the download and the checkout
    /// of the first. A failed test fails the run with 1.
    #[test]
    fn a_second_run_takes_the_download_and_the_checkout_of_the_first() {
        let fake = Fake::new();
        fake.keep_artifact();
        assert_eq!(fake.ci_test("named_test").status.code(), Some(0));
        std::fs::write(fake.path("exit"), "101").unwrap();
        let out = fake.ci_test("other");
        assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
        assert!(
            stderr(&out).contains("takes the checkout of an earlier run"),
            "{}",
            stderr(&out)
        );
        let downloads = fake
            .calls()
            .into_iter()
            .filter(|c| c.starts_with("gh run download"))
            .count();
        assert_eq!(downloads, 1);
        assert_eq!(fake.runs()[1].last().unwrap(), "one::other");
    }

    /// With no artifact of the tree, the recipe says so, exits with 3,
    /// and builds nothing: the verifier says so in its result, and only
    /// then builds the test.
    #[test]
    fn with_no_artifact_of_the_tree_the_recipe_says_so_and_builds_nothing() {
        let fake = Fake::new();
        let out = fake.ci_test("named_test");
        assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
        assert!(
            stderr(&out).contains(&format!(
                "ci-test: CI kept no test build of the tree {} of HEAD: no run has tests-{}. Say \
                 so in your result, and build the test: just test named_test\n",
                fake.tree(),
                fake.tree()
            )),
            "{}",
            stderr(&out)
        );
        let calls = fake.calls();
        assert!(!calls.iter().any(|c| c.starts_with("cargo")), "{calls:?}");
        assert!(
            !calls.iter().any(|c| c.starts_with("gh run download")),
            "{calls:?}"
        );
        assert!(!fake.root().exists());
        assert!(fake.runs().is_empty());
    }

    /// An archive whose path cannot be made here, for example the path of
    /// a runner of GitHub, cannot run: exit 3.
    #[test]
    fn an_archive_at_a_path_that_cannot_be_made_exits_with_3() {
        let fake = Fake::with_root(Some(Path::new("/proc/riff-ci-test/riff")));
        fake.keep_artifact();
        let out = fake.ci_test("named_test");
        assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
        assert!(
            stderr(&out).contains(
                "the archive runs only at /proc/riff-ci-test/riff, and ci-test cannot make that \
                 folder"
            ),
            "{}",
            stderr(&out)
        );
        assert!(fake.runs().is_empty());
    }

    /// A folder at the path of the archive that ci-test did not make
    /// stays as it is, and no test runs. A name that no test has fails
    /// with 2.
    #[test]
    fn a_folder_that_ci_test_did_not_make_stays_and_a_wrong_name_fails() {
        let fake = Fake::new();
        fake.keep_artifact();
        std::fs::create_dir_all(fake.root()).unwrap();
        std::fs::write(fake.root().join("mine"), "x").unwrap();
        let out = fake.ci_test("named_test");
        assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
        assert!(
            stderr(&out).contains("and ci-test did not make it. Move it away, then run again."),
            "{}",
            stderr(&out)
        );
        assert!(fake.root().join("mine").exists());
        assert!(fake.runs().is_empty());

        std::fs::remove_dir_all(fake.root()).unwrap();
        let out = fake.ci_test("nothing");
        assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
        assert!(
            stderr(&out).contains(&hygiene::archive::none_named("nothing")),
            "{}",
            stderr(&out)
        );
        assert!(fake.runs().is_empty());
    }
}

/// 01M75NDQY6BHZBV9PF735E3FGY: the recipe `ci-test` runs the script, and
/// the private recipe `test-named` runs `hygiene test-name` with the
/// values of the tests, as `test-archived` runs a part.
#[test]
fn the_justfile_runs_the_script_and_the_runner_of_the_archive() {
    let justfile = std::fs::read_to_string(top().join("justfile")).unwrap();
    assert!(
        justfile.contains("\nci-test NAME:\n    deploy/ci-test.sh \"$1\"\n"),
        "{justfile}"
    );
    assert!(
        justfile.contains(
            "\ntest-named NAME ARCHIVE:\n    {{test_env}} \"${CARGO_TARGET_DIR:-target}/debug/hygiene\" \
             test-name \"$1\" --archive \"$2\"\n"
        ),
        "{justfile}"
    );
}
