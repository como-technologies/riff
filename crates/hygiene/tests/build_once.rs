//! Build once for each tree (01M6QQ1NXY0CXMZEEEP413W4PK,
//! 01M6QQ1P0GH9AGCDA5N8EMEFGF, 01M6QQ1P37XW7N465MQGPHCXZM), and test it
//! once (01M75NAF4JQWQG3ZVGJM74XRBG, 01M75NAF7VV5S7343P9Q7H7F60,
//! 01M75NAFBH6M49GMSV8VG857EP): the job Plan finds the artifacts and the
//! pass of the Gate that a run made for the same tree, and the jobs after
//! it take them. A commit of `main` gets one release build, of Linux and
//! of a Mac, and its tag takes it (01M75TMWKT8E0YYEENWVX4B67A,
//! 01M75TN3EC2T25FH0D09JESHZF). The tests run `deploy/build-plan.sh`
//! with a fake `gh`, and read the workflow. Each run of the fake forge is
//! a push to `main`, and the plan asks with no event, so each artifact
//! counts: the tests of `artifact_run` say which runs count for each
//! event. The fake prints what the `--jq` filter of the real `gh` prints:
//! the filter is checked by a read of the live API, not here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fake `gh` in `dir`. For `api repos/o/r/actions/artifacts?name=NAME
/// ...` it prints the file `NAME` of `dir` when it is there, and nothing
/// else. Each run is a push to `main` that counts
/// ([`RUNS_OF_MAIN`](crate::artifact_run::RUNS_OF_MAIN)). It fails when
/// the file `fail` is there. It logs each call.
fn gh(dir: &Path) {
    isolated::write_program(
        dir.join("gh"),
        [
            "#!/bin/sh\ndir=$(dirname \"$0\")\necho \"$*\" >> \"$dir/calls\"\n\
             if [ -f \"$dir/fail\" ]; then echo 'gh: boom' >&2; exit 1; fi\n\
             case \"$1 $2\" in\n\
             'api repos/o/r/actions/artifacts?name='*)\n\
               name=${2#*name=}; name=${name%%&*}\n\
               [ -f \"$dir/$name\" ] && cat \"$dir/$name\"\n\
               exit 0 ;;\n",
            crate::artifact_run::RUNS_OF_MAIN,
            "esac\necho \"no such call: $*\" >&2; exit 1\n",
        ]
        .concat(),
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
        .env_remove("GITHUB_EVENT_NAME")
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

/// The plan of a tree that no run built: this run, 900, builds and
/// tests it. The profile dev wants no binary of a Mac.
const BUILD_AND_TEST: [(&str, &str); 12] = [
    ("tree", "T1"),
    ("run_tests", "true"),
    ("passed_run", ""),
    ("build_tests", "true"),
    ("tests_run", "900"),
    ("build_static", "true"),
    ("static_profile", "dev"),
    ("static_run", "900"),
    ("build_macos_riff", "false"),
    ("macos_riff_run", ""),
    ("build_mac_session_riff", "false"),
    ("mac_session_riff_run", ""),
];

/// The plan of a tree whose Gate passed in the run `run`: no build and
/// no test.
fn nothing_from(run: &str) -> [(&'static str, String); 12] {
    [
        ("tree", "T1".into()),
        ("run_tests", "false".into()),
        ("passed_run", run.into()),
        ("build_tests", "false".into()),
        ("tests_run", String::new()),
        ("build_static", "false".into()),
        ("static_profile", "dev".into()),
        ("static_run", run.into()),
        ("build_macos_riff", "false".into()),
        ("macos_riff_run", String::new()),
        ("build_mac_session_riff", "false".into()),
        ("mac_session_riff_run", String::new()),
    ]
}

fn expect_owned(plan: &BTreeMap<String, String>, want: &[(&str, String)]) {
    let want: Vec<(&str, &str)> = want.iter().map(|(k, v)| (*k, v.as_str())).collect();
    expect(plan, &want);
}

/// The artifacts that a run `run` keeps for the tree `T1` after its Gate
/// passed: the test binaries, the static binaries and the pass.
fn passed(run: &'static str) -> [(&'static str, &'static str); 3] {
    [("gate-T1", run), ("tests-T1", run), ("static-dev-T1", run)]
}

#[test]
fn a_tree_with_no_artifact_is_built_in_this_run() {
    let dir = forge(&[]);
    let (plan, out) = run_plan(dir.path(), "dev", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    expect(&plan, &BUILD_AND_TEST);
}

/// 01M75NAF4JQWQG3ZVGJM74XRBG: a run whose tree passed its Gate in a run
/// that counts takes what that run made: it builds nothing, and the tests
/// and macOS do not run.
#[test]
fn a_tree_that_passed_its_gate_builds_nothing_and_tests_nothing() {
    let dir = forge(&passed("77"));
    let (plan, out) = run_plan(dir.path(), "dev", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    expect_owned(&plan, &nothing_from("77"));
    assert!(
        stderr(&out).contains(
            "gate-T1 is in run 77: the tree passed its Gate, so no test build and no test."
        ),
        "{}",
        stderr(&out)
    );
    assert!(stderr(&out).contains("no static build"), "{}", stderr(&out));
    // No test runs, so the plan does not look for the test binaries.
    let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
    assert!(!calls.contains("name=tests-"), "{calls}");
}

/// 01M75NAF7VV5S7343P9Q7H7F60, 01M75NAFBH6M49GMSV8VG857EP: a merge group
/// with a new tree, after `main` moved, builds it and tests it in its own
/// run. That run keeps its artifacts and its pass, so the push to `main`
/// of the same tree builds nothing and tests nothing: the tree is built
/// and tested once.
#[test]
fn a_merge_group_with_a_new_tree_builds_and_tests_it_once() {
    // The pull request had another tree, before main moved.
    let dir = forge(&[
        ("gate-T0", "77"),
        ("tests-T0", "77"),
        ("static-dev-T0", "77"),
    ]);
    let (plan, out) = run_plan(dir.path(), "dev", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    expect(&plan, &BUILD_AND_TEST);
    assert!(
        stderr(&out).contains("no gate-T1: the tests and the job macOS run."),
        "{}",
        stderr(&out)
    );

    // The merge group run, 900, passed its Gate and kept its artifacts.
    for (name, run) in passed("900") {
        std::fs::write(dir.path().join(name), format!("{run}\n")).unwrap();
    }
    let (plan, out) = run_plan(dir.path(), "dev", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    expect_owned(&plan, &nothing_from("900"));
}

/// 01M75NAFBH6M49GMSV8VG857EP, #1181: a push to `main` with a tree that a
/// run tested runs no test part and no job macOS, and builds nothing.
#[test]
fn a_push_to_main_with_a_tested_tree_runs_no_test() {
    let dir = forge(&passed("78"));
    let (plan, out) = run_plan(dir.path(), "dev", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(plan["run_tests"], "false");
    assert_eq!(plan["passed_run"], "78");
    assert_eq!(plan["build_tests"], "false");
    assert_eq!(plan["build_static"], "false");
}

/// 01M75NAF4JQWQG3ZVGJM74XRBG: a tree with the binaries of a run whose
/// Gate did not pass has no `gate-TREE`: the run takes the binaries, and
/// runs the tests again.
#[test]
fn a_tree_whose_gate_did_not_pass_runs_its_tests_again_and_builds_nothing() {
    let dir = forge(&[("tests-T1", "77"), ("static-dev-T1", "77")]);
    let (plan, out) = run_plan(dir.path(), "dev", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    expect(
        &plan,
        &[
            ("tree", "T1"),
            ("run_tests", "true"),
            ("passed_run", ""),
            ("build_tests", "false"),
            ("tests_run", "77"),
            ("build_static", "false"),
            ("static_profile", "dev"),
            ("static_run", "77"),
            ("build_macos_riff", "false"),
            ("macos_riff_run", ""),
            ("build_mac_session_riff", "false"),
            ("mac_session_riff_run", ""),
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

/// The release binaries of the tree `T1` that the run `run` made: of
/// Linux, and of a Mac.
fn released(run: &'static str) -> [(&'static str, &'static str); 3] {
    [
        ("static-release-T1", run),
        ("macos-riff-T1", run),
        ("mac-session-riff-T1", run),
    ]
}

/// The plan of the profile release for a tree whose release binaries
/// the run `run` made: it builds nothing, and takes them.
fn released_plan(plan: &BTreeMap<String, String>, run: &str) {
    for (name, value) in [
        ("build_static", "false"),
        ("static_profile", "release"),
        ("static_run", run),
        ("build_macos_riff", "false"),
        ("macos_riff_run", run),
        ("build_mac_session_riff", "false"),
        ("mac_session_riff_run", run),
    ] {
        assert_eq!(plan[name], value, "{name}: {plan:?}");
    }
}

/// 01M75TN3EC2T25FH0D09JESHZF: a tag needs the release binaries and no
/// test binary. It takes those of Linux and of a Mac that a merge group
/// or a push to `main` made for its tree, and builds nothing. With no such
/// artifact, it builds them.
#[test]
fn a_tag_takes_the_release_binaries_of_its_tree_or_builds_them() {
    let dir = forge(&released("78"));
    let (plan, _) = run_plan(dir.path(), "release", "false");
    expect(
        &plan,
        &[
            ("tree", "T1"),
            ("run_tests", "false"),
            ("passed_run", ""),
            ("build_tests", "false"),
            ("tests_run", ""),
            ("build_static", "false"),
            ("static_profile", "release"),
            ("static_run", "78"),
            ("build_macos_riff", "false"),
            ("macos_riff_run", "78"),
            ("build_mac_session_riff", "false"),
            ("mac_session_riff_run", "78"),
        ],
    );

    let dir = forge(&[("static-dev-T1", "77")]);
    let (plan, out) = run_plan(dir.path(), "release", "false");
    assert_eq!(plan["build_static"], "true");
    assert_eq!(plan["static_run"], "900");
    assert_eq!(plan["build_macos_riff"], "true");
    assert_eq!(plan["macos_riff_run"], "900");
    assert_eq!(plan["build_mac_session_riff"], "true");
    assert_eq!(plan["mac_session_riff_run"], "900");
    for line in [
        "no macos-riff-T1: the job macos_riff builds it.",
        "no mac-session-riff-T1: the job mac_session_riff builds it.",
    ] {
        assert!(stderr(&out).contains(line), "{}", stderr(&out));
    }
    let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
    assert!(!calls.contains("name=tests-"), "{calls}");
    assert!(!calls.contains("name=gate-"), "{calls}");
}

/// 01M75TMWKT8E0YYEENWVX4B67A, 01M75TN3EC2T25FH0D09JESHZF: the first push
/// to `main` of a tree that its merge group built and tested builds the
/// release binaries of Linux and of a Mac once, and runs no test. A
/// second run of the same tree builds nothing, and a tag of that commit
/// builds nothing.
#[test]
fn a_commit_of_main_builds_its_release_once_and_its_tag_builds_nothing() {
    // The merge group, run 77, built the profile dev and passed its Gate.
    let dir = forge(&passed("77"));
    let (plan, out) = run_plan(dir.path(), "release", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    expect(
        &plan,
        &[
            ("tree", "T1"),
            ("run_tests", "false"),
            ("passed_run", "77"),
            ("build_tests", "false"),
            ("tests_run", ""),
            ("build_static", "true"),
            ("static_profile", "release"),
            ("static_run", "900"),
            ("build_macos_riff", "true"),
            ("macos_riff_run", "900"),
            ("build_mac_session_riff", "true"),
            ("mac_session_riff_run", "900"),
        ],
    );
    assert!(
        stderr(&out).contains(
            "no static-release-T1: the job builds the static binaries of the profile release."
        ),
        "{}",
        stderr(&out)
    );

    // The push, run 900, kept the release binaries of the tree.
    for (name, run) in released("900") {
        std::fs::write(dir.path().join(name), format!("{run}\n")).unwrap();
    }
    let (plan, out) = run_plan(dir.path(), "release", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(plan["run_tests"], "false");
    assert_eq!(plan["build_tests"], "false");
    released_plan(&plan, "900");
    for line in [
        "static-release-T1 is in run 900: no static build.",
        "macos-riff-T1 is in run 900: no build of macos_riff.",
        "mac-session-riff-T1 is in run 900: no build of mac_session_riff.",
    ] {
        assert!(stderr(&out).contains(line), "{}", stderr(&out));
    }

    // The tag of the commit.
    let (plan, out) = run_plan(dir.path(), "release", "false");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(plan["build_tests"], "false");
    released_plan(&plan, "900");
}

/// 01M75TMWKT8E0YYEENWVX4B67A: a run of the profile dev, a pull request
/// or a merge group, asks for no binary of a Mac and builds none. When
/// the tree has the release binaries of Linux, it takes them.
#[test]
fn a_pull_request_builds_no_binary_of_a_mac() {
    let dir = forge(&released("78"));
    let (plan, out) = run_plan(dir.path(), "dev", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(plan["static_profile"], "release");
    assert_eq!(plan["build_static"], "false");
    for (name, value) in [
        ("build_macos_riff", "false"),
        ("macos_riff_run", ""),
        ("build_mac_session_riff", "false"),
        ("mac_session_riff_run", ""),
    ] {
        assert_eq!(plan[name], value, "{name}");
    }
    let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
    assert!(!calls.contains("name=macos-riff-"), "{calls}");
    assert!(!calls.contains("name=mac-session-riff-"), "{calls}");
}

/// 01M75TMWKT8E0YYEENWVX4B67A: a job of a Mac that failed kept no
/// artifact. The next run of the profile release builds only that one.
#[test]
fn a_failed_build_of_a_mac_is_the_only_build_of_the_next_run() {
    let dir = forge(&[("static-release-T1", "78"), ("macos-riff-T1", "78")]);
    let (plan, _) = run_plan(dir.path(), "release", "false");
    assert_eq!(plan["build_static"], "false");
    assert_eq!(plan["build_macos_riff"], "false");
    assert_eq!(plan["macos_riff_run"], "78");
    assert_eq!(plan["build_mac_session_riff"], "true");
    assert_eq!(plan["mac_session_riff_run"], "900");
}

/// A `gh` that fails gives no artifact: the run makes each of them, and
/// runs the tests.
#[test]
fn a_failed_gh_builds_and_tests_and_says_so() {
    let dir = forge(&passed("77"));
    std::fs::write(dir.path().join("fail"), "").unwrap();
    let (plan, out) = run_plan(dir.path(), "dev", "true");
    assert!(out.status.success(), "{}", stderr(&out));
    expect(&plan, &BUILD_AND_TEST);
    for name in ["gate-T1", "tests-T1", "static-dev-T1"] {
        assert!(
            stderr(&out).contains(&format!(
                "gh could not list the artifacts named {name}. The run makes it."
            )),
            "{}",
            stderr(&out)
        );
    }
}

/// The names of the artifacts that the calls of the fake `gh` in `dir`
/// asked for, in order.
fn asked(dir: &Path) -> Vec<String> {
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    calls
        .lines()
        .map(|call| {
            call.split("name=")
                .nth(1)
                .unwrap()
                .split('&')
                .next()
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[test]
fn the_plan_asks_only_for_a_live_artifact_of_this_repository() {
    let dir = forge(&[]);
    run_plan(dir.path(), "release", "true");
    assert_eq!(
        asked(dir.path()),
        [
            "gate-T1",
            "tests-T1",
            "static-release-T1",
            "macos-riff-T1",
            "mac-session-riff-T1"
        ]
    );
    let dir = forge(&[]);
    run_plan(dir.path(), "dev", "true");
    assert_eq!(
        asked(dir.path()),
        ["gate-T1", "tests-T1", "static-release-T1", "static-dev-T1"]
    );
    let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
    for call in calls.lines() {
        assert!(
            call.starts_with("api repos/o/r/actions/artifacts?name="),
            "{call}"
        );
        assert!(call.contains("&per_page=100 "), "{call}");
        // A fork cannot give an artifact to a tag, and an expired one is gone.
        for part in [
            ".expired == false",
            ".workflow_run.repository_id == .workflow_run.head_repository_id",
            "sort_by(.created_at) | reverse",
        ] {
            assert!(call.contains(part), "{part} is not in {call}");
        }
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
        .env_remove("GITHUB_EVENT_NAME")
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
            [
                "#!/bin/sh\ndir=$(dirname \"$0\")\necho \"$*\" >> \"$dir/calls\"\n\
             make() { printf '%s' \"$1\" > \"$2\"; }\n\
             case \"$1 $2\" in\n\
             'api repos/o/r/actions/artifacts?name='*)\n\
               name=${2#*name=}; name=${name%%&*}\n\
               [ -f \"$dir/art-$name\" ] && cat \"$dir/art-$name\"\n\
               exit 0 ;;\n",
                crate::artifact_run::RUNS_OF_MAIN,
                "'run download')\n\
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
            ]
            .concat(),
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
            .env_remove("GITHUB_EVENT_NAME")
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

/// The value of the line `key: VALUE` of `job`, at the first level of
/// the job.
fn field(job: &str, key: &str) -> String {
    let start = format!("\n    {key}: ");
    let rest = job
        .split_once(&start)
        .unwrap_or_else(|| panic!("no {key}:\n{job}"))
        .1;
    rest.lines().next().unwrap().to_owned()
}

/// 01M75NAF4JQWQG3ZVGJM74XRBG: the job Plan runs the plan before each
/// other job, and gives each line of the plan as an output. It runs on
/// each event of Build, also the merge group, and builds nothing.
#[test]
fn the_job_plan_runs_the_plan_and_gives_each_line_as_an_output() {
    let plan = job("plan");
    assert_eq!(field(&plan, "name"), "Plan");
    assert_eq!(field(&plan, "if"), "github.event_name != 'schedule'");
    assert!(!plan.contains("\n    needs:"), "{plan}");
    let all = steps(&plan);
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(all[0].starts_with("uses: actions/checkout@"), "{}", all[0]);
    assert!(all[1].contains("id: plan\n"), "{}", all[1]);
    assert!(all[1].contains("run: deploy/build-plan.sh"), "{}", all[1]);
    // Each name that the plan writes is an output of the job.
    let dir = forge(&[]);
    let (lines, _) = run_plan(dir.path(), "dev", "true");
    assert_eq!(lines.len(), 12, "{lines:?}");
    for name in lines.keys() {
        assert!(
            plan.contains(&format!(
                "\n      {name}: ${{{{ steps.plan.outputs.{name} }}}}\n"
            )),
            "{name} is no output of the job plan:\n{plan}"
        );
    }
    assert!(plan.contains("GH_TOKEN: ${{ github.token }}\n"), "{plan}");
    // Build, the tests, macOS, the Gate, the check of the static riff, the
    // stage and the jobs of the build of a Mac take the plan.
    for name in [
        "build",
        "test",
        "macos",
        "gate",
        "static",
        "stage",
        "macos_riff",
        "mac_session_riff",
        "macos_release",
    ] {
        let needs = field(&job(name), "needs");
        assert!(needs.contains("plan"), "{name}: {needs}");
    }
}

/// 01M75NAF4JQWQG3ZVGJM74XRBG: the tests and macOS run only when the plan
/// found no pass of the Gate on the tree. macOS needs only the plan, so it
/// starts beside Build.
#[test]
fn the_tests_and_macos_run_only_for_a_tree_with_no_pass() {
    let when = "github.event_name != 'schedule' && github.ref_type != 'tag' && \
                needs.plan.outputs.run_tests == 'true'";
    assert_eq!(field(&job("test"), "if"), when);
    assert_eq!(field(&job("macos"), "if"), when);
    assert_eq!(field(&job("macos"), "needs"), "[plan]");
    assert_eq!(field(&job("test"), "needs"), "[plan, build]");
}

/// 01M75NAF4JQWQG3ZVGJM74XRBG, 01M75NAF7VV5S7343P9Q7H7F60: the Gate keeps
/// its pass as `gate-TREE` when the tests ran in its run, after the step
/// that checks each part. No event is left out, so a merge group keeps
/// its pass for the push to `main`.
#[test]
fn the_gate_keeps_its_pass_as_gate_tree_when_the_tests_ran() {
    let gate = job("gate");
    let all = steps(&gate);
    assert_eq!(all.len(), 3, "{all:?}");
    assert!(all[0].starts_with("name: Each part passed\n"), "{}", all[0]);
    let run = "if: needs.plan.outputs.run_tests == 'true'\n";
    assert!(all[1].contains(run), "{}", all[1]);
    assert!(all[1].contains("> target/gate/passed\n"), "{}", all[1]);
    assert!(
        all[2].starts_with("uses: actions/upload-artifact@"),
        "{}",
        all[2]
    );
    assert!(all[2].contains(run), "{}", all[2]);
    assert!(
        all[2].contains("name: gate-${{ needs.plan.outputs.tree }}\n"),
        "{}",
        all[2]
    );
    assert!(all[2].contains("path: target/gate/\n"), "{}", all[2]);
    assert!(all[2].contains("if-no-files-found: error"), "{}", all[2]);
    assert!(!gate.contains("event_name =="), "{gate}");
    assert!(!job("build").contains("event_name =="), "{}", job("build"));
}

/// 01M6QQ1P37XW7N465MQGPHCXZM: each step of the job Build after the
/// checkout runs only when the plan says that its artifact is missing. So
/// a run whose tree has both artifacts builds nothing.
#[test]
fn each_build_step_runs_only_for_a_missing_artifact() {
    let build_job = job("build");
    assert_eq!(field(&build_job, "needs"), "[plan]");
    let all = steps(&build_job);
    assert!(all[0].starts_with("uses: actions/checkout@"), "{}", all[0]);
    for step in &all[1..] {
        assert!(
            step.contains("if: needs.plan.outputs.build_tests == 'true'\n")
                || step.contains("if: needs.plan.outputs.build_static == 'true'"),
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
        .find("name: tests-${{ needs.plan.outputs.tree }}\n")
        .unwrap_or_else(|| panic!("no tests artifact:\n{build_job}"));
    let static_ = build_job
        .find("name: static-${{ needs.plan.outputs.static_profile }}-${{ needs.plan.outputs.tree }}\n")
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
        ("test", "tests-${{ needs.plan.outputs.tree }}"),
        (
            "static",
            "static-${{ needs.plan.outputs.static_profile }}-${{ needs.plan.outputs.tree }}",
        ),
        (
            "stage",
            "static-${{ needs.plan.outputs.static_profile }}-${{ needs.plan.outputs.tree }}",
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

/// 01M6QQ1P8H32E2TQ0CYA87R02V, 01M75TMWKT8E0YYEENWVX4B67A: a push, to
/// `main` or of a tag, wants the profile release, and each other event,
/// also a pull request with the label release, the profile dev. A tag
/// wants no test binary. A tag job names the files from the profile
/// release, and the job Static riff release adds them to the release.
#[test]
fn a_push_builds_the_release_profile_and_a_pull_request_the_profile_dev() {
    let plan_job = job("plan");
    assert!(
        plan_job.contains(
            "\n      PROFILE: ${{ github.event_name == 'push' && 'release' || 'dev' }}\n"
        ),
        "{plan_job}"
    );
    assert!(!plan_job.contains("labels"), "{plan_job}");
    assert!(plan_job.contains("TESTS: ${{ github.ref_type != 'tag' }}\n"));
    let static_job = job("static");
    assert!(
        static_job.contains(
            "cp \"target/static/release/$name\" \"dist/$name-x86_64-unknown-linux-musl\""
        ),
        "{static_job}"
    );
    assert!(job("static_release").contains("needs: [static]\n"));
}

/// 01M75TMWKT8E0YYEENWVX4B67A, 01M75TN3EC2T25FH0D09JESHZF: each job of
/// the build of a Mac runs only when the plan finds its artifact
/// missing, and keeps the artifact by the tree. The job of the release
/// of a Mac takes both from the runs of the plan, also when the plan
/// skipped a job of a Mac.
#[test]
fn the_jobs_of_a_mac_build_once_for_each_tree_and_the_tag_takes_their_files() {
    for (name, artifact) in [
        ("macos_riff", "macos-riff"),
        ("mac_session_riff", "mac-session-riff"),
    ] {
        let mac = job(name);
        assert_eq!(field(&mac, "needs"), "[plan]", "{name}");
        assert_eq!(
            field(&mac, "if"),
            format!("needs.plan.outputs.build_{name} == 'true'"),
            "{name}"
        );
        let upload = steps(&mac)
            .into_iter()
            .find(|step| step.starts_with("uses: actions/upload-artifact@"))
            .unwrap_or_else(|| panic!("{name} keeps no artifact:\n{mac}"));
        assert!(
            upload.contains(&format!(
                "name: {artifact}-${{{{ needs.plan.outputs.tree }}}}\n"
            )),
            "{upload}"
        );
        assert!(upload.contains("if-no-files-found: error"), "{upload}");
    }

    let release = job("macos_release");
    assert_eq!(
        field(&release, "needs"),
        "[plan, macos_riff, mac_session_riff]"
    );
    assert_eq!(
        field(&release, "if"),
        "${{ !cancelled() && needs.plan.result == 'success' && github.ref_type == 'tag' }}"
    );
    assert!(release.contains("\n      actions: read\n"), "{release}");
    for (artifact, run) in [
        ("macos-riff", "macos_riff_run"),
        ("mac-session-riff", "mac_session_riff_run"),
    ] {
        let download = format!(
            "name: {artifact}-${{{{ needs.plan.outputs.tree }}}}\n          \
             run-id: ${{{{ needs.plan.outputs.{run} }}}}\n          \
             github-token: ${{{{ github.token }}}}\n"
        );
        assert!(release.contains(&download), "{download}\n{release}");
    }
    // The release of a Mac compiles nothing.
    for compile in ["cargo", "rustup", "static/build.sh"] {
        assert!(!release.contains(compile), "{compile}:\n{release}");
    }
}
