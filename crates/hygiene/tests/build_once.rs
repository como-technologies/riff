//! Build once for each tree (01M6QQ1NXY0CXMZEEEP413W4PK,
//! 01M6QQ1P0GH9AGCDA5N8EMEFGF, 01M6QQ1P37XW7N465MQGPHCXZM): the plan of
//! the job Build finds the artifacts that a run made for the same tree,
//! and the jobs after it take them. The tests run `deploy/build-plan.sh`
//! with a fake `gh`, and read the workflow. The fake prints what the
//! `--jq` filter of the real `gh` prints: the filter is checked by a read
//! of the live API, not here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fake `gh` in `dir`. For `api repos/o/r/actions/artifacts?name=NAME
/// ...` it prints the file `NAME` of `dir` when it is there, and nothing
/// else. It fails when the file `fail` is there. It logs each call.
fn gh(dir: &Path) {
    isolated::write_program(
        dir.join("gh"),
        "#!/bin/sh\ndir=$(dirname \"$0\")\necho \"$*\" >> \"$dir/calls\"\n\
         if [ -f \"$dir/fail\" ]; then echo 'gh: boom' >&2; exit 1; fi\n\
         case \"$1 $2\" in\n\
         'api repos/o/r/actions/artifacts?name='*)\n\
           name=${2#*name=}; name=${name%%&*}\n\
           [ -f \"$dir/$name\" ] && cat \"$dir/$name\"\n\
           exit 0 ;;\n\
         esac\n\
         echo \"no such call: $*\" >&2; exit 1\n",
    );
}

/// The artifacts that the fake forge holds: name and the run that made it.
fn forge(artifacts: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    gh(dir.path());
    for (name, run) in artifacts {
        std::fs::write(dir.path().join(name), format!("{run}\n")).unwrap();
    }
    dir
}

/// Runs the plan for the tree `T1` in this run, 900, with the profile and
/// the tests that it wants. It gives the plan, and the output.
fn run_plan(dir: &Path, profile: &str, tests: &str) -> (BTreeMap<String, String>, Output) {
    let outputs = dir.join("outputs");
    let _ = std::fs::remove_file(&outputs);
    let path = format!(
        "{}:{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("bash")
        .arg(top().join("deploy/build-plan.sh"))
        .arg("T1")
        .env("PATH", path)
        .env("GH_REPO", "o/r")
        .env("GITHUB_RUN_ID", "900")
        .env("GITHUB_OUTPUT", &outputs)
        .env("PROFILE", profile)
        .env("TESTS", tests)
        .output()
        .unwrap();
    let text = std::fs::read_to_string(&outputs).unwrap_or_default();
    let plan = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    (plan, out)
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn expect(plan: &BTreeMap<String, String>, want: &[(&str, &str)]) {
    let got: Vec<(&str, &str)> = plan.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut want = want.to_vec();
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn a_tree_with_no_artifact_is_built_in_this_run() {
    let dir = forge(&[]);
    let (plan, out) = run_plan(dir.path(), "dev", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    expect(
        &plan,
        &[
            ("tree", "T1"),
            ("build_tests", "true"),
            ("tests_run", "900"),
            ("build_static", "true"),
            ("static_profile", "dev"),
            ("static_run", "900"),
        ],
    );
}

/// The done-when of the issue: a push to main whose tree has the
/// artifacts of a passed pull request head builds nothing, and takes
/// both artifacts from the run of the pull request.
#[test]
fn a_push_to_main_with_the_tree_of_a_passed_pull_request_builds_nothing() {
    let dir = forge(&[("tests-T1", "77"), ("static-dev-T1", "77")]);
    let (plan, out) = run_plan(dir.path(), "dev", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    expect(
        &plan,
        &[
            ("tree", "T1"),
            ("build_tests", "false"),
            ("tests_run", "77"),
            ("build_static", "false"),
            ("static_profile", "dev"),
            ("static_run", "77"),
        ],
    );
    assert!(stderr(&out).contains("no test build"), "{}", stderr(&out));
    assert!(stderr(&out).contains("no static build"), "{}", stderr(&out));
}

#[test]
fn a_tree_with_another_tree_artifacts_is_built() {
    let dir = forge(&[("tests-T2", "77"), ("static-dev-T2", "77")]);
    let (plan, _) = run_plan(dir.path(), "dev", "true");
    assert_eq!(plan["build_tests"], "true");
    assert_eq!(plan["build_static"], "true");
}

#[test]
fn the_release_binaries_of_a_tree_beat_its_dev_binaries() {
    let dir = forge(&[
        ("tests-T1", "77"),
        ("static-dev-T1", "77"),
        ("static-release-T1", "78"),
    ]);
    let (plan, _) = run_plan(dir.path(), "dev", "true");
    assert_eq!(plan["build_static"], "false");
    assert_eq!(plan["static_profile"], "release");
    assert_eq!(plan["static_run"], "78");
}

#[test]
fn a_release_run_takes_no_dev_binaries() {
    let dir = forge(&[("tests-T1", "77"), ("static-dev-T1", "77")]);
    let (plan, _) = run_plan(dir.path(), "release", "true");
    assert_eq!(plan["build_tests"], "false");
    assert_eq!(plan["build_static"], "true");
    assert_eq!(plan["static_profile"], "release");
    assert_eq!(plan["static_run"], "900");
}

/// A tag needs the release binaries and no test binary. It takes those
/// of the pull request with the same tree, else it builds them.
#[test]
fn a_tag_takes_the_release_binaries_of_its_tree_or_builds_them() {
    let dir = forge(&[("static-release-T1", "78")]);
    let (plan, _) = run_plan(dir.path(), "release", "false");
    expect(
        &plan,
        &[
            ("tree", "T1"),
            ("build_tests", "false"),
            ("tests_run", ""),
            ("build_static", "false"),
            ("static_profile", "release"),
            ("static_run", "78"),
        ],
    );

    let dir = forge(&[("static-dev-T1", "77")]);
    let (plan, _) = run_plan(dir.path(), "release", "false");
    assert_eq!(plan["build_static"], "true");
    assert_eq!(plan["static_run"], "900");
    let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
    assert!(!calls.contains("name=tests-"), "{calls}");
}

#[test]
fn a_failed_gh_builds_and_says_so() {
    let dir = forge(&[("tests-T1", "77")]);
    std::fs::write(dir.path().join("fail"), "").unwrap();
    let (plan, out) = run_plan(dir.path(), "dev", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(plan["build_tests"], "true");
    assert_eq!(plan["build_static"], "true");
    assert!(
        stderr(&out).contains("gh could not list the artifacts named tests-T1. The job builds it."),
        "{}",
        stderr(&out)
    );
}

#[test]
fn the_plan_asks_only_for_a_live_artifact_of_this_repository() {
    let dir = forge(&[]);
    run_plan(dir.path(), "dev", "true");
    let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
    let call = calls.lines().next().unwrap();
    assert!(
        call.contains("repos/o/r/actions/artifacts?name=tests-T1&per_page=100"),
        "{call}"
    );
    // A fork cannot give an artifact to a tag, and an expired one is gone.
    for part in [
        ".expired == false",
        ".workflow_run.repository_id == .workflow_run.head_repository_id",
        "sort_by(.created_at) | last",
    ] {
        assert!(call.contains(part), "{part} is not in {call}");
    }
}

#[test]
fn the_plan_refuses_a_wrong_profile_and_no_repository() {
    let dir = forge(&[]);
    let (_, out) = run_plan(dir.path(), "fast", "true");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("\"fast\" is not a profile. Give release or dev."),
        "{}",
        stderr(&out)
    );
    let (_, out) = run_plan(dir.path(), "dev", "maybe");
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("TESTS is \"maybe\"."));

    let out = Command::new("bash")
        .arg(top().join("deploy/build-plan.sh"))
        .arg("T1")
        .env_remove("GH_REPO")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("give the repository in GH_REPO"));
}

#[test]
fn the_tree_comes_from_git_when_no_argument_gives_it() {
    let dir = forge(&[]);
    let repo = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@e.test"])
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    git(&["init", "-q"]);
    std::fs::write(repo.path().join("a"), "a").unwrap();
    git(&["add", "a"]);
    git(&["commit", "-q", "-m", "a"]);
    let tree = git(&["rev-parse", "HEAD^{tree}"]);
    let path = format!(
        "{}:{}",
        dir.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("bash")
        .arg(top().join("deploy/build-plan.sh"))
        .env("PATH", path)
        .env("GH_REPO", "o/r")
        .env("GITHUB_RUN_ID", "900")
        .env_remove("GITHUB_OUTPUT")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with(&format!("tree={tree}\n")), "{stdout}");
}

// ── deploy/static/fetch.sh: the real-session test takes the static riff
// of the commit (01M6QQ1PDNMHQ47FG8TD85P7CN).

/// A repository with one commit and the scripts of `deploy/`, with stubs
/// for the build and the check that log to `calls`, and a fake `gh`. The
/// fake `gh` answers an artifact lookup from the files `art-NAME` (the
/// run), `run download` and `release download` by writing the files of a
/// build, and fails each download when the file `fail-download` is there.
/// The folder of the fake `gh` is `bin`.
struct Checkout {
    top: tempfile::TempDir,
    bin: tempfile::TempDir,
}

impl Checkout {
    fn new() -> Checkout {
        let top = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        let root = top.path();
        for file in ["deploy/static/fetch.sh", "deploy/artifact-run.sh"] {
            std::fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
            isolated::copy_program(self::top().join(file), root.join(file));
        }
        for script in ["build.sh", "check.sh"] {
            isolated::write_program(
                root.join("deploy/static").join(script),
                format!(
                    "#!/bin/sh\necho \"{script} $*\" >> '{}/calls'\n",
                    bin.path().display()
                ),
            );
        }
        isolated::write_program(
            bin.path().join("gh"),
            "#!/bin/sh\ndir=$(dirname \"$0\")\necho \"$*\" >> \"$dir/calls\"\n\
             make() { printf '%s' \"$1\" > \"$2\"; }\n\
             case \"$1 $2\" in\n\
             'api repos/o/r/actions/artifacts?name='*)\n\
               name=${2#*name=}; name=${name%%&*}\n\
               [ -f \"$dir/art-$name\" ] && cat \"$dir/art-$name\"\n\
               exit 0 ;;\n\
             'run download')\n\
               [ -f \"$dir/fail-download\" ] && exit 1\n\
               while [ $# -gt 0 ]; do [ \"$1\" = --dir ] && d=$2; shift; done\n\
               make riff-bytes \"$d/riff\"; make server-bytes \"$d/riff-server\"; exit 0 ;;\n\
             'release download')\n\
               [ -f \"$dir/fail-download\" ] && exit 1\n\
               while [ $# -gt 0 ]; do [ \"$1\" = --dir ] && d=$2; shift; done\n\
               make riff-bytes \"$d/riff-x86_64-unknown-linux-musl\"\n\
               make server-bytes \"$d/riff-server-x86_64-unknown-linux-musl\"\n\
               if [ -f \"$dir/bad-sums\" ]; then echo '0000 riff-x86_64-unknown-linux-musl' > \"$d/SHA256SUMS\"; \
               else (cd \"$d\" && sha256sum riff-x86_64-unknown-linux-musl riff-server-x86_64-unknown-linux-musl > SHA256SUMS); fi\n\
               exit 0 ;;\n\
             esac\n\
             echo \"no such call: $*\" >&2; exit 1\n",
        );
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@e.test"])
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
        };
        git(&["init", "-q"]);
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "a"]);
        Checkout { top, bin }
    }

    fn tree(&self) -> String {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD^{tree}"])
            .current_dir(self.top.path())
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn tag(&self, tag: &str) {
        let out = Command::new("git")
            .args(["tag", tag])
            .current_dir(self.top.path())
            .output()
            .unwrap();
        assert!(out.status.success());
    }

    /// The fake forge holds the artifact `name`, made by the run `run`.
    fn artifact(&self, name: &str, run: &str) {
        std::fs::write(
            self.bin.path().join(format!("art-{name}")),
            format!("{run}\n"),
        )
        .unwrap();
    }

    fn fetch(&self) -> Output {
        let path = format!(
            "{}:{}",
            self.bin.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new("bash")
            .arg(self.top.path().join("deploy/static/fetch.sh"))
            .env("PATH", path)
            .env("GH_REPO", "o/r")
            .output()
            .unwrap()
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.bin.path().join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod fetch {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn the_real_session_test_takes_the_artifact_of_the_tree_and_builds_nothing() {
        let c = Checkout::new();
        c.artifact(&format!("static-dev-{}", c.tree()), "77");
        let out = c.fetch();
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), "target/static/fetched\n");
        let dir = c.top.path().join("target/static/fetched");
        assert_eq!(
            std::fs::read_to_string(dir.join("riff")).unwrap(),
            "riff-bytes"
        );
        for name in ["riff", "riff-server"] {
            let mode = std::fs::metadata(dir.join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "{name} does not run");
        }
        let calls = c.calls();
        assert!(
            calls.iter().any(|call| call
                == &format!(
                    "run download 77 --name static-dev-{} --dir target/static/fetched",
                    c.tree()
                )),
            "{calls:?}"
        );
        assert!(
            !calls.iter().any(|call| call.starts_with("build.sh")),
            "{calls:?}"
        );
    }

    #[test]
    fn the_release_binaries_of_the_tree_beat_the_dev_binaries() {
        let c = Checkout::new();
        c.artifact(&format!("static-dev-{}", c.tree()), "77");
        c.artifact(&format!("static-release-{}", c.tree()), "78");
        let out = c.fetch();
        assert!(out.status.success(), "{}", stderr(&out));
        assert!(
            c.calls()
                .iter()
                .any(|call| call.starts_with("run download 78 --name static-release-")),
            "{:?}",
            c.calls()
        );
    }

    #[test]
    fn a_tag_on_head_gives_the_files_of_its_release() {
        let c = Checkout::new();
        c.tag("v1.2.3");
        let out = c.fetch();
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), "target/static/fetched\n");
        let dir = c.top.path().join("target/static/fetched");
        assert_eq!(
            std::fs::read_to_string(dir.join("riff-server")).unwrap(),
            "server-bytes"
        );
        assert!(!dir.join("SHA256SUMS").exists());
        assert!(
            c.calls()
                .iter()
                .any(|call| call.starts_with("release download v1.2.3 --dir")),
            "{:?}",
            c.calls()
        );
    }

    #[test]
    fn a_release_with_a_wrong_sum_is_not_taken_and_the_test_builds() {
        let c = Checkout::new();
        c.tag("v1.2.3");
        std::fs::write(c.bin.path().join("bad-sums"), "").unwrap();
        let out = c.fetch();
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), "target/static/debug\n");
        assert!(stderr(&out).contains("the release v1.2.3 has no good static files."));
    }

    #[test]
    fn a_tree_with_no_artifact_and_no_tag_is_built_with_the_profile_dev() {
        let c = Checkout::new();
        let out = c.fetch();
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), "target/static/debug\n");
        let builds: Vec<String> = c
            .calls()
            .into_iter()
            .filter(|call| call.ends_with(" dev") && call.contains(".sh"))
            .collect();
        assert_eq!(builds, ["build.sh dev", "check.sh dev"]);
        assert!(stderr(&out).contains("no CI run has the static binaries of the tree"));
    }

    #[test]
    fn a_failed_download_builds() {
        let c = Checkout::new();
        c.artifact(&format!("static-dev-{}", c.tree()), "77");
        std::fs::write(c.bin.path().join("fail-download"), "").unwrap();
        let out = c.fetch();
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), "target/static/debug\n");
        assert!(stderr(&out).contains("gh could not download static-dev-"));
    }
}

// ── .github/workflows/ci.yml: the shape of the jobs.

fn workflow() -> String {
    std::fs::read_to_string(top().join(".github/workflows/ci.yml")).unwrap()
}

/// The names of the jobs of the CI workflow.
fn job_names() -> Vec<String> {
    let text = workflow();
    let jobs = text.split_once("\njobs:\n").unwrap().1;
    jobs.lines()
        .filter_map(|line| line.strip_prefix("  "))
        .filter_map(|line| line.strip_suffix(':'))
        .filter(|name| name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .map(str::to_owned)
        .collect()
}

/// The job `name` of the CI workflow, up to the next job.
fn job(name: &str) -> String {
    let text = workflow();
    let jobs = text.split_once("\njobs:").unwrap().1;
    let job = jobs
        .split_once(&format!("\n  {name}:\n"))
        .unwrap_or_else(|| panic!("ci.yml has no job {name}"))
        .1;
    let end = job
        .match_indices("\n  ")
        .find(|(at, _)| job[at + 3..].starts_with(|c: char| c.is_ascii_alphabetic()))
        .map_or(job.len(), |(at, _)| at);
    job[..end].to_owned()
}

/// The steps of `job`: the text of each `- ...` at the step level. A
/// comment before the first step is no step.
fn steps(job: &str) -> Vec<String> {
    let steps = job.split_once("\n    steps:\n").unwrap().1;
    steps
        .split("\n      - ")
        .map(|step| step.trim_start_matches("      - ").to_owned())
        .filter(|step| step.lines().any(|line| !line.trim_start().starts_with('#')))
        .collect()
}

/// 01M6QQ1NXY0CXMZEEEP413W4PK: the job Build is the only job of CI that
/// compiles the code for Linux on x86-64. A job of a deploy, a check or a
/// release runs no cargo, no rustup, no cache of a build and no build of
/// an image from the source. The jobs macOS and macOS riff compile for
/// macOS (01M6YPB5DZ9M3VEJ7V7P7EF797), and the job Linux riff of a Mac
/// compiles the Linux riff of the sessions of a Mac
/// (01M6Z8FKZRJZ4NY60PQAH9SMHV).
#[test]
fn only_the_job_build_compiles_for_linux() {
    let names = job_names();
    let compile = ["build", "macos", "macos_riff", "mac_session_riff"];
    for name in compile {
        assert!(names.contains(&name.to_owned()), "{name}: {names:?}");
    }
    for name in names {
        // The audit reads Cargo.lock with cargo-audit, and compiles nothing.
        if compile.contains(&name.as_str()) || name == "audit" {
            continue;
        }
        let job = job(&name);
        for compile in [
            "cargo",
            "rustup",
            "rust-cache",
            "just ",
            "build-push-action",
            "static/build.sh",
            "session-test.sh --build",
        ] {
            assert!(!job.contains(compile), "{compile} is in the job {name}");
        }
    }
}

/// 01M6QQ1P37XW7N465MQGPHCXZM: each step of the job Build after the plan
/// runs only when the plan says that its artifact is missing. So a run
/// whose tree has both artifacts builds nothing.
#[test]
fn each_build_step_runs_only_for_a_missing_artifact() {
    let build_job = job("build");
    let all = steps(&build_job);
    assert!(all[0].starts_with("uses: actions/checkout@"), "{}", all[0]);
    assert!(all[1].contains("id: plan\n"), "{}", all[1]);
    assert!(all[1].contains("run: deploy/build-plan.sh"), "{}", all[1]);
    for step in &all[2..] {
        assert!(
            step.contains("if: steps.plan.outputs.build_tests == 'true'\n")
                || step.contains("if: steps.plan.outputs.build_static == 'true'"),
            "a step runs with no plan:\n{step}"
        );
    }
    // The test build and its upload need the test plan; the static build,
    // its cache and its upload the static plan.
    let tests = all
        .iter()
        .filter(|s| s.contains("build_tests == 'true'"))
        .count();
    let static_ = all
        .iter()
        .filter(|s| s.contains("build_static == 'true'"))
        .count();
    assert_eq!((tests, static_), (3, 4), "{build_job}");
}

/// 01M6QQ1P0GH9AGCDA5N8EMEFGF: the artifacts have the git tree in their
/// name, and Build keeps them only after each step passed.
#[test]
fn the_artifacts_have_the_tree_in_their_name_and_come_after_the_builds() {
    let build_job = job("build");
    let tests = build_job
        .find("name: tests-${{ steps.plan.outputs.tree }}\n")
        .unwrap_or_else(|| panic!("no tests artifact:\n{build_job}"));
    let static_ = build_job
        .find("name: static-${{ steps.plan.outputs.static_profile }}-${{ steps.plan.outputs.tree }}\n")
        .unwrap_or_else(|| panic!("no static artifact:\n{build_job}"));
    for build in [
        "run: deploy/session-test.sh --build\n",
        "run: deploy/static/build.sh \"$STATIC_PROFILE\"\n",
    ] {
        let at = build_job.find(build).unwrap();
        assert!(at < tests && at < static_, "{build} is after an upload");
    }
    assert!(
        build_job.contains("path: target/test-archive/\n"),
        "{build_job}"
    );
    assert!(
        build_job.contains("if-no-files-found: error\n"),
        "an upload may be empty:\n{build_job}"
    );
    // The names of the jobs that take them have the same form.
    for (name, artifact) in [
        ("test", "tests-${{ needs.build.outputs.tree }}"),
        (
            "static",
            "static-${{ needs.build.outputs.static_profile }}-${{ needs.build.outputs.tree }}",
        ),
        (
            "stage",
            "static-${{ needs.build.outputs.static_profile }}-${{ needs.build.outputs.tree }}",
        ),
    ] {
        let taker = job(name);
        assert!(
            taker.contains(&format!("name: {artifact}\n")),
            "{name} takes no {artifact}:\n{taker}"
        );
        assert!(taker.contains("actions/download-artifact@"), "{name}");
        assert!(
            taker.contains("github-token: ${{ github.token }}\n"),
            "{name}"
        );
    }
    assert!(
        workflow().contains("\npermissions:\n  contents: read\n  actions: read\n"),
        "a job needs actions: read to take an artifact of another run"
    );
}

/// 01M6QQ1P8H32E2TQ0CYA87R02V: a tag and a pull request with the label
/// release want the profile release, and a tag wants no test binary. A
/// tag job names the files from the profile release, and the job Static
/// riff release adds them to the release.
#[test]
fn a_tag_and_a_release_pull_request_build_the_release_profile() {
    let build_job = job("build");
    let profile = build_job
        .lines()
        .find_map(|line| line.trim().strip_prefix("PROFILE: "))
        .unwrap();
    assert!(profile.contains("github.ref_type == 'tag'"), "{profile}");
    assert!(
        profile.contains("contains(github.event.pull_request.labels.*.name, 'release')"),
        "{profile}"
    );
    assert!(profile.ends_with("&& 'release' || 'dev' }}"), "{profile}");
    assert!(build_job.contains("TESTS: ${{ github.ref_type != 'tag' }}\n"));
    let static_job = job("static");
    assert!(
        static_job.contains(
            "cp \"target/static/release/$name\" \"dist/$name-x86_64-unknown-linux-musl\""
        ),
        "{static_job}"
    );
    assert!(job("static_release").contains("needs: [static]\n"));
}
