//! The runs that give the artifacts of a tree
//! (01M75TT13241HC22M0KBJ6HW96, 01M75Y0JGMPRZPNJA1EFM7EM7S,
//! 01M75YKD063DE6ZFA0H3K8BNZB):
//! `deploy/artifact-run.sh` takes an artifact only from a run of `ci.yml`
//! whose job that made the artifact passed, and that is the run that asks
//! or a run that it trusts. A push to `main`, a merge group and a tag
//! trust only a merge group and a push to `main` whose commit has the
//! tree of the artifact. A pull request and a call outside CI trust also
//! a run whose head commit has the workflows, `deploy` and the `justfile`
//! of `main`. The tests run the script with a fake `gh`. The fake prints
//! what the `--jq` filter of the real `gh` prints: the filters are
//! checked by a read of the live API, not here.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The arms of a `case "$1 $2"` of a fake `gh` that answer the questions
/// of `deploy/artifact-run.sh` about a run: each run is a push to `main`
/// of the workflow `ci.yml` at a commit of the tree `t0`, and each of its
/// jobs passed. The fakes of the plan, of `fetch.sh` and of `ci-test` hold
/// only such runs, and ask as a pull request or from outside CI: then the
/// script does not compare the tree.
pub const RUNS_OF_MAIN: &str = "'api repos/o/r/actions/runs/'*/jobs*)\n\
     printf 'Build=success\\nGate=success\\nmacOS riff=success\\nLinux riff of a Mac=success\\n'; \
     exit 0 ;;\n\
     'api repos/o/r/actions/runs/'*)\n\
     echo 'push main c0ffee t0 .github/workflows/ci.yml'; exit 0 ;;\n";

/// The files of `main` that decide what the jobs run, as the filter of
/// the tree prints them.
const MAIN: &str = ".github/workflows=w1 deploy=d1 justfile=j1";

/// The event and the ref of the run that asks: `GITHUB_EVENT_NAME` and
/// `GITHUB_REF`.
type Event = (&'static str, &'static str);

/// The run of a pull request.
const PULL_REQUEST: Event = ("pull_request", "refs/pull/6/merge");

/// A push to `main`.
const PUSH_TO_MAIN: Event = ("push", "refs/heads/main");

/// The runs of CI that are not of a pull request: a push to `main`, a
/// tag, a merge group and a run by hand.
const MERGED_ONLY: [Event; 4] = [
    PUSH_TO_MAIN,
    ("push", "refs/tags/v1.2.3"),
    ("merge_group", "refs/heads/gh-readonly-queue/main/pr-5-abc"),
    ("workflow_dispatch", "refs/heads/main"),
];

/// Sets the run `this` and its event and ref `event` in the environment
/// of `command`, and removes each of them that is not given.
fn in_run(command: &mut Command, event: Option<Event>, this: Option<&str>) {
    for name in ["GITHUB_RUN_ID", "GITHUB_EVENT_NAME", "GITHUB_REF"] {
        command.env_remove(name);
    }
    if let Some(run) = this {
        command.env("GITHUB_RUN_ID", run);
    }
    if let Some((name, git_ref)) = event {
        command.env("GITHUB_EVENT_NAME", name);
        command.env("GITHUB_REF", git_ref);
    }
}

/// The plan of `deploy/build-plan.sh` for the tree `T1` in the run `run`
/// of the event `event`.
fn plan(forge: &Forge, event: Event, run: &str) -> String {
    let mut command = Command::new("bash");
    command
        .arg(top().join("deploy/build-plan.sh"))
        .arg("T1")
        .env("PATH", forge.search_path())
        .env("GH_REPO", "o/r")
        .env_remove("GITHUB_OUTPUT")
        .env_remove("PROFILE")
        .env_remove("TESTS");
    in_run(&mut command, Some(event), Some(run));
    let out = command.output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

/// The plan of a run that builds and tests the tree `T1`: the run 900.
const BUILD_AND_TEST: &str = "tree=T1\nrun_tests=true\npassed_run=\nbuild_tests=true\n\
     tests_run=900\nbuild_static=true\nstatic_profile=dev\nstatic_run=900\n\
     build_macos_riff=false\nmacos_riff_run=\n\
     build_mac_session_riff=false\nmac_session_riff_run=\n";

/// The plan of a run that takes the pass and the binaries of `T1` from
/// the run `run`, and builds nothing.
fn nothing_from(run: &str) -> String {
    format!(
        "tree=T1\nrun_tests=false\npassed_run={run}\nbuild_tests=false\ntests_run=\n\
         build_static=false\nstatic_profile=dev\nstatic_run={run}\n\
         build_macos_riff=false\nmacos_riff_run=\n\
         build_mac_session_riff=false\nmac_session_riff_run=\n"
    )
}

/// A fake forge in a folder. Its `gh` logs each call to `calls` with
/// `printf`, so a backslash of a filter stays. It fails each call when
/// the file `fail` is there, and answers from these files:
///
/// - `art-NAME`: the runs that hold the artifact NAME, the newest first;
/// - `run-RUN`: the event, the branch, the head commit, the tree of that
///   commit and the workflow of the run RUN;
/// - `jobs-RUN`: a line `NAME=CONCLUSION` for each job of the run;
/// - `tree-REF`: the files of the commit or branch REF.
///
/// A missing `run-`, `jobs-` or `tree-` file is a call that fails.
struct Forge {
    dir: tempfile::TempDir,
}

impl Forge {
    fn new() -> Forge {
        let forge = Forge {
            dir: tempfile::tempdir().unwrap(),
        };
        isolated::write_program(
            forge.path("gh"),
            "#!/bin/sh\ndir=$(dirname \"$0\")\nprintf '%s\\n' \"$*\" >> \"$dir/calls\"\n\
             if [ -f \"$dir/fail\" ]; then echo 'gh: boom' >&2; exit 1; fi\n\
             answer() { if [ -f \"$dir/$1\" ]; then cat \"$dir/$1\"; exit 0; fi; \
             echo \"gh: no $1\" >&2; exit 1; }\n\
             case \"$1 $2\" in\n\
             'api repos/o/r/actions/artifacts?name='*)\n\
               name=${2#*name=}; name=${name%%&*}\n\
               [ -f \"$dir/art-$name\" ] && cat \"$dir/art-$name\"\n\
               exit 0 ;;\n\
             'api repos/o/r/actions/runs/'*/jobs*)\n\
               run=${2#repos/o/r/actions/runs/}; answer \"jobs-${run%%/*}\" ;;\n\
             'api repos/o/r/actions/runs/'*)\n\
               answer \"run-${2#repos/o/r/actions/runs/}\" ;;\n\
             'api repos/o/r/git/trees/'*)\n\
               ref=${2#repos/o/r/git/trees/}; answer \"tree-${ref%%\\?*}\" ;;\n\
             esac\n\
             echo \"no such call: $*\" >&2; exit 1\n",
        );
        forge.write("tree-main", MAIN);
        forge
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.path(name), format!("{text}\n")).unwrap();
    }

    /// The runs that hold the artifact `name`, the newest first.
    fn artifact(&self, name: &str, runs: &[&str]) {
        self.write(&format!("art-{name}"), &runs.join("\n"));
    }

    /// The run `run`: its event, branch, head commit, the tree of that
    /// commit and the workflow file.
    fn run(&self, run: &str, event: &str, branch: &str, sha: &str, tree: &str, path: &str) {
        self.write(
            &format!("run-{run}"),
            &format!("{event} {branch} {sha} {tree} {path}"),
        );
    }

    /// The jobs of the run `run`, with the conclusion of each.
    fn jobs(&self, run: &str, jobs: &[(&str, &str)]) {
        let lines: Vec<String> = jobs.iter().map(|(n, c)| format!("{n}={c}")).collect();
        self.write(&format!("jobs-{run}"), &lines.join("\n"));
    }

    /// The run `run` of a pull request with `ci.yml`: its head commit is
    /// `head-RUN` of the tree `T1` with the files `files`, and its jobs
    /// passed.
    fn pull_request(&self, run: &str, files: &str) {
        let sha = format!("head-{run}");
        self.run(
            run,
            "pull_request",
            "b",
            &sha,
            "T1",
            ".github/workflows/ci.yml",
        );
        self.write(&format!("tree-{sha}"), files);
        self.jobs(
            run,
            &[
                ("Plan", "success"),
                ("Build", "success"),
                ("Gate", "success"),
            ],
        );
    }

    /// Runs `deploy/artifact-run.sh NAME` outside CI, or in the run
    /// `this` of a pull request when it is given.
    fn ask(&self, name: &str, this: Option<&str>) -> Output {
        self.ask_in(this.map(|_| PULL_REQUEST), name, this)
    }

    /// Runs `deploy/artifact-run.sh NAME` in the run `this` of the event
    /// and the ref `event`, or outside CI with neither.
    fn ask_in(&self, event: Option<Event>, name: &str, this: Option<&str>) -> Output {
        let mut command = Command::new("bash");
        command
            .arg(top().join("deploy/artifact-run.sh"))
            .arg(name)
            .env("PATH", self.search_path())
            .env("GH_REPO", "o/r");
        in_run(&mut command, event, this);
        command.output().unwrap()
    }

    fn search_path(&self) -> String {
        format!(
            "{}:{}",
            self.dir.path().display(),
            std::env::var("PATH").unwrap_or_default()
        )
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.path("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The script exits with 0 and prints `run`, or nothing.
fn takes(out: &Output, run: &str) {
    assert!(out.status.success(), "{}", stderr(out));
    let want = if run.is_empty() {
        String::new()
    } else {
        format!("{run}\n")
    };
    assert_eq!(stdout(out), want, "{}", stderr(out));
}

/// The done-when of #1969: a run whose workflow file is the file of
/// `main` is taken, and a run whose workflow file differs from it is not.
#[test]
fn a_run_with_the_workflow_file_of_main_is_taken_and_another_is_not() {
    let forge = Forge::new();
    forge.artifact("tests-T1", &["77"]);
    forge.pull_request("77", MAIN);
    takes(&forge.ask("tests-T1", None), "77");
    let calls = forge.calls();
    for tree in ["main", "head-77"] {
        assert!(
            calls
                .iter()
                .any(|c| c.starts_with(&format!("api repos/o/r/git/trees/{tree}?recursive=1 "))),
            "{calls:?}"
        );
    }

    forge.pull_request("77", ".github/workflows=w2 deploy=d1 justfile=j1");
    let out = forge.ask("tests-T1", None);
    takes(&out, "");
    assert!(
        stderr(&out).contains(
            "artifact-run: tests-T1 of the run 77 does not count: its head commit head-77 does \
             not have the .github/workflows, the deploy and the justfile of main.\n"
        ),
        "{}",
        stderr(&out)
    );
}

/// A run of a pull request whose head commit has another script of
/// `deploy` or `justfile` than `main` gives no artifact to another pull
/// request or outside CI. Its own run takes its artifacts, for example
/// when it runs again.
#[test]
fn a_run_with_other_scripts_than_main_gives_no_artifact_to_another_run() {
    let forge = Forge::new();
    forge.artifact("static-dev-T1", &["77"]);
    for files in [
        ".github/workflows=w1 deploy=d2 justfile=j1",
        ".github/workflows=w1 deploy=d1 justfile=j2",
        ".github/workflows=w1 deploy=d1",
    ] {
        forge.pull_request("77", files);
        takes(&forge.ask("static-dev-T1", None), "");
        takes(&forge.ask("static-dev-T1", Some("900")), "");
        takes(&forge.ask("static-dev-T1", Some("77")), "77");
    }
}

/// The plan of another run builds and tests the tree of such a pull
/// request, and the plan of its own run takes what it made.
#[test]
fn the_plan_builds_and_tests_the_tree_of_a_pull_request_that_changes_a_script() {
    let forge = Forge::new();
    for name in ["gate-T1", "tests-T1", "static-dev-T1"] {
        forge.artifact(name, &["77"]);
    }
    forge.pull_request("77", ".github/workflows=w1 deploy=d2 justfile=j1");
    assert_eq!(plan(&forge, PULL_REQUEST, "900"), BUILD_AND_TEST);
    assert_eq!(plan(&forge, PULL_REQUEST, "77"), nothing_from("77"));
}

/// For a pull request and outside CI, a push to `main` and a merge group
/// count with no look at their files. A push of a tag counts only with
/// the files of `main`.
#[test]
fn a_push_to_main_and_a_merge_group_count_and_a_tag_needs_the_files_of_main() {
    let forge = Forge::new();
    forge.artifact("static-release-T1", &["77"]);
    forge.jobs("77", &[("Build", "success")]);
    for (event, branch) in [
        ("push", "main"),
        ("merge_group", "gh-readonly-queue/main/pr-5-abc"),
    ] {
        forge.run("77", event, branch, "s77", "T1", ".github/workflows/ci.yml");
        takes(&forge.ask("static-release-T1", None), "77");
    }
    let calls = forge.calls();
    assert!(!calls.iter().any(|c| c.contains("git/trees")), "{calls:?}");

    forge.run(
        "77",
        "push",
        "v1.2.3",
        "s77",
        "T1",
        ".github/workflows/ci.yml",
    );
    forge.write("tree-s77", ".github/workflows=w2 deploy=d1 justfile=j1");
    takes(&forge.ask("static-release-T1", None), "");
    forge.write("tree-s77", MAIN);
    takes(&forge.ask("static-release-T1", None), "77");
}

/// The done-when of #1980: a push to `main`, a tag, a merge group and a
/// run by hand take no artifact of a run of a pull request, also when its
/// head commit has the files of `main`. They do not read those files. A
/// pull request and a call outside CI take the artifact.
#[test]
fn a_run_that_is_not_of_a_pull_request_refuses_an_artifact_of_a_pull_request() {
    let forge = Forge::new();
    forge.artifact("tests-T1", &["77"]);
    forge.pull_request("77", MAIN);
    for event in MERGED_ONLY {
        let out = forge.ask_in(Some(event), "tests-T1", Some("900"));
        takes(&out, "");
        assert!(
            stderr(&out).contains(&format!(
                "artifact-run: tests-T1 of the run 77 does not count: it is a run of \
                 pull_request on b, and a run of {} takes only a merge group or a push to \
                 main.\n",
                event.0
            )),
            "{event:?}: {}",
            stderr(&out)
        );
    }
    let calls = forge.calls();
    assert!(!calls.iter().any(|c| c.contains("git/trees")), "{calls:?}");

    takes(&forge.ask("tests-T1", Some("900")), "77");
    takes(&forge.ask("tests-T1", None), "77");
}

/// The done-when of #1980: for a run that is not of a pull request, a
/// merge group or a push to `main` counts only when the tree of its
/// commit is the tree in the name of the artifact. A pull request and a
/// call outside CI do not compare the trees.
#[test]
fn a_run_that_is_not_of_a_pull_request_refuses_an_artifact_whose_tree_is_not_the_tree_of_its_run() {
    let forge = Forge::new();
    forge.artifact("static-release-T1", &["88"]);
    forge.jobs("88", &[("Build", "success")]);
    for (event, branch) in [
        ("push", "main"),
        ("merge_group", "gh-readonly-queue/main/pr-5-abc"),
    ] {
        forge.run("88", event, branch, "s88", "T2", ".github/workflows/ci.yml");
        for asker in MERGED_ONLY {
            let out = forge.ask_in(Some(asker), "static-release-T1", Some("900"));
            takes(&out, "");
            assert!(
                stderr(&out).contains(
                    "artifact-run: static-release-T1 of the run 88 does not count: the tree of \
                     its commit s88 is T2, not T1.\n"
                ),
                "{asker:?}: {}",
                stderr(&out)
            );
        }
        takes(&forge.ask("static-release-T1", Some("900")), "88");
        takes(&forge.ask("static-release-T1", None), "88");

        forge.run("88", event, branch, "s88", "T1", ".github/workflows/ci.yml");
        for asker in MERGED_ONLY {
            takes(
                &forge.ask_in(Some(asker), "static-release-T1", Some("900")),
                "88",
            );
        }
    }
}

/// A run takes its own artifacts, for example when it runs again: also a
/// tag, whose run is no push to `main`. Another tag does not take them.
#[test]
fn a_tag_takes_its_own_artifacts_and_not_those_of_another_tag() {
    let forge = Forge::new();
    forge.artifact("static-release-T1", &["99"]);
    forge.run(
        "99",
        "push",
        "v1.2.3",
        "s99",
        "T1",
        ".github/workflows/ci.yml",
    );
    forge.jobs("99", &[("Build", "success")]);
    let tag = Some(("push", "refs/tags/v1.2.3"));
    takes(&forge.ask_in(tag, "static-release-T1", Some("99")), "99");

    let other = Some(("push", "refs/tags/v1.2.4"));
    let out = forge.ask_in(other, "static-release-T1", Some("900"));
    takes(&out, "");
    assert!(
        stderr(&out).contains(
            "static-release-T1 of the run 99 does not count: it is a run of push on v1.2.3, \
             and a run of push takes only a merge group or a push to main.\n"
        ),
        "{}",
        stderr(&out)
    );
}

/// With no merge queue, the plan of a push to `main` builds and tests a
/// tree that only a pull request built. It takes the pass and the
/// binaries that a merge group made for the tree.
#[test]
fn the_plan_of_a_push_to_main_builds_and_tests_a_tree_that_only_a_pull_request_built() {
    let forge = Forge::new();
    let names = ["gate-T1", "tests-T1", "static-dev-T1"];
    for name in names {
        forge.artifact(name, &["77"]);
    }
    forge.pull_request("77", MAIN);
    assert_eq!(plan(&forge, PUSH_TO_MAIN, "900"), BUILD_AND_TEST);
    assert_eq!(plan(&forge, PULL_REQUEST, "900"), nothing_from("77"));

    for name in names {
        forge.artifact(name, &["77", "88"]);
    }
    forge.run(
        "88",
        "merge_group",
        "gh-readonly-queue/main/pr-5-abc",
        "s88",
        "T1",
        ".github/workflows/ci.yml",
    );
    forge.jobs("88", &[("Build", "success"), ("Gate", "success")]);
    assert_eq!(plan(&forge, PUSH_TO_MAIN, "900"), nothing_from("88"));
}

/// A run of another workflow gives no artifact, also on `main`.
#[test]
fn a_run_of_another_workflow_does_not_count() {
    let forge = Forge::new();
    forge.artifact("tests-T1", &["77"]);
    forge.run(
        "77",
        "push",
        "main",
        "s77",
        "T1",
        ".github/workflows/hygiene.yml",
    );
    forge.jobs("77", &[("Build", "success")]);
    let out = forge.ask("tests-T1", None);
    takes(&out, "");
    assert!(
        stderr(&out).contains(
            "tests-T1 of the run 77 does not count: it ran the workflow \
             .github/workflows/hygiene.yml, not .github/workflows/ci.yml.\n"
        ),
        "{}",
        stderr(&out)
    );
}

/// The job that made the artifact passed: `Gate` for `gate-TREE`,
/// `Build` for `tests-TREE` and `static-PROFILE-TREE`. A job that runs
/// has no conclusion yet. A job that failed and then passed in a new
/// attempt counts.
#[test]
fn only_a_run_whose_job_that_made_the_artifact_passed_counts() {
    let forge = Forge::new();
    forge.artifact("tests-T1", &["77"]);
    forge.artifact("gate-T1", &["77"]);
    forge.run(
        "77",
        "push",
        "main",
        "s77",
        "T1",
        ".github/workflows/ci.yml",
    );

    forge.jobs("77", &[("Build", "failure"), ("Gate", "success")]);
    let out = forge.ask("tests-T1", None);
    takes(&out, "");
    assert!(
        stderr(&out)
            .contains("tests-T1 of the run 77 does not count: its job Build did not pass.\n"),
        "{}",
        stderr(&out)
    );

    for gate in ["failure", "null", "cancelled"] {
        forge.jobs("77", &[("Build", "success"), ("Gate", gate)]);
        let out = forge.ask("gate-T1", None);
        takes(&out, "");
        assert!(
            stderr(&out).contains("its job Gate did not pass."),
            "{}",
            stderr(&out)
        );
    }
    forge.jobs("77", &[("Build", "success"), ("Gate", "success")]);
    takes(&forge.ask("gate-T1", None), "77");
    takes(&forge.ask("tests-T1", None), "77");

    forge.jobs("77", &[("Build", "failure"), ("Build", "success")]);
    takes(&forge.ask("tests-T1", None), "77");
}

/// The files of a Mac count by the job that made them: `macOS riff` for
/// `macos-riff-TREE`, `Linux riff of a Mac` for `mac-session-riff-TREE`.
/// A passed `Build` does not make them count. A push to `main`, a tag, a
/// merge group and a run by hand take them from the push to `main` of
/// their tree.
#[test]
fn the_files_of_a_mac_count_by_the_job_that_made_them() {
    let forge = Forge::new();
    forge.artifact("macos-riff-T1", &["77"]);
    forge.artifact("mac-session-riff-T1", &["77"]);
    forge.run(
        "77",
        "push",
        "main",
        "s77",
        "T1",
        ".github/workflows/ci.yml",
    );

    forge.jobs(
        "77",
        &[
            ("Build", "success"),
            ("macOS riff", "failure"),
            ("Linux riff of a Mac", "success"),
        ],
    );
    let out = forge.ask("macos-riff-T1", None);
    takes(&out, "");
    assert!(
        stderr(&out).contains(
            "macos-riff-T1 of the run 77 does not count: its job macOS riff did not pass.\n"
        ),
        "{}",
        stderr(&out)
    );
    takes(&forge.ask("mac-session-riff-T1", None), "77");

    forge.jobs(
        "77",
        &[
            ("Build", "success"),
            ("macOS riff", "success"),
            ("Linux riff of a Mac", "cancelled"),
        ],
    );
    takes(&forge.ask("macos-riff-T1", None), "77");
    for event in MERGED_ONLY {
        takes(&forge.ask_in(Some(event), "macos-riff-T1", None), "77");
    }
    let out = forge.ask("mac-session-riff-T1", None);
    takes(&out, "");
    assert!(
        stderr(&out).contains("its job Linux riff of a Mac did not pass."),
        "{}",
        stderr(&out)
    );
}

/// Of the runs that hold the artifact, the newest that counts wins. The
/// script asks for each run once, and for no run after the one it takes.
#[test]
fn the_newest_run_that_counts_wins() {
    let forge = Forge::new();
    forge.artifact("tests-T1", &["99", "99", "88", "77"]);
    forge.pull_request("99", ".github/workflows=w2 deploy=d1 justfile=j1");
    forge.pull_request("88", MAIN);
    forge.pull_request("77", MAIN);
    let out = forge.ask("tests-T1", None);
    takes(&out, "88");
    let calls = forge.calls();
    let runs: Vec<&String> = calls
        .iter()
        .filter(|c| c.starts_with("api repos/o/r/actions/runs/"))
        .collect();
    assert_eq!(runs.len(), 3, "{calls:?}");
    assert!(
        runs[0].starts_with("api repos/o/r/actions/runs/99 "),
        "{runs:?}"
    );
    assert!(
        runs[1].starts_with("api repos/o/r/actions/runs/88 "),
        "{runs:?}"
    );
    assert!(
        runs[2].starts_with("api repos/o/r/actions/runs/88/jobs?"),
        "{runs:?}"
    );
    let mains = calls
        .iter()
        .filter(|c| c.starts_with("api repos/o/r/git/trees/main?"))
        .count();
    assert_eq!(mains, 1, "{calls:?}");
}

/// A run, a tree or the jobs that gh cannot read give no artifact. With
/// no list of the artifacts, the script fails.
#[test]
fn what_gh_cannot_read_gives_no_artifact() {
    let forge = Forge::new();
    forge.artifact("tests-T1", &["77"]);
    let out = forge.ask("tests-T1", None);
    takes(&out, "");
    assert!(
        stderr(&out)
            .contains("tests-T1 of the run 77 does not count: gh could not read the run.\n"),
        "{}",
        stderr(&out)
    );

    forge.pull_request("77", MAIN);
    std::fs::remove_file(forge.path("tree-main")).unwrap();
    takes(&forge.ask("tests-T1", None), "");
    forge.write("tree-main", MAIN);
    std::fs::remove_file(forge.path("tree-head-77")).unwrap();
    takes(&forge.ask("tests-T1", None), "");
    forge.write("tree-head-77", "");
    forge.write("tree-main", "");
    takes(&forge.ask("tests-T1", None), "");

    forge.write("tree-head-77", MAIN);
    forge.write("tree-main", MAIN);
    std::fs::remove_file(forge.path("jobs-77")).unwrap();
    takes(&forge.ask("tests-T1", None), "");

    std::fs::write(forge.path("fail"), "").unwrap();
    let out = forge.ask("tests-T1", None);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(stdout(&out), "");
}

/// The script asks for each run, its jobs and its files with the filters
/// that read the live API.
#[test]
fn the_script_reads_the_run_its_jobs_and_its_files() {
    let forge = Forge::new();
    forge.artifact("tests-T1", &["77"]);
    forge.pull_request("77", MAIN);
    takes(&forge.ask("tests-T1", None), "77");
    let calls = forge.calls();
    let call = |start: &str| {
        calls
            .iter()
            .find(|c| c.starts_with(start))
            .unwrap_or_else(|| panic!("no call {start}: {calls:?}"))
            .clone()
    };
    let run = call("api repos/o/r/actions/runs/77 ");
    assert!(
        run.ends_with(
            r#" --jq "\(.event) \(.head_branch) \(.head_sha) \(.head_commit.tree_id) \(.path)""#
        ),
        "{run}"
    );
    let jobs = call("api repos/o/r/actions/runs/77/jobs?filter=all&per_page=100 ");
    assert!(
        jobs.ends_with(r#" --jq .jobs[] | "\(.name)=\(.conclusion)""#),
        "{jobs}"
    );
    let tree = call("api repos/o/r/git/trees/head-77?recursive=1 ");
    for part in [
        "if .truncated then empty else",
        r#".path == ".github/workflows" or .path == "deploy" or .path == "justfile""#,
        r#""\(.path)=\(.sha)""#,
    ] {
        assert!(tree.contains(part), "{part} is not in {tree}");
    }
}

#[test]
fn a_wrong_name_and_no_repository_are_refused() {
    let forge = Forge::new();
    for name in [
        "",
        "tree-T1",
        "gate-",
        "static-fast-T1",
        "macos-riff-",
        "mac-session-riff-",
    ] {
        let out = forge.ask(name, None);
        assert_eq!(out.status.code(), Some(2), "{name}: {}", stderr(&out));
    }
    let out = forge.ask("static-fast-T1", None);
    assert!(
        stderr(&out).contains(
            "artifact-run: static-fast-T1 is no artifact of a tree. Give gate-TREE, tests-TREE, \
             static-PROFILE-TREE, macos-riff-TREE or mac-session-riff-TREE.\n"
        ),
        "{}",
        stderr(&out)
    );
    let out = Command::new("bash")
        .arg(top().join("deploy/artifact-run.sh"))
        .arg("tests-T1")
        .env("PATH", forge.search_path())
        .env_remove("GH_REPO")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("give the repository in GH_REPO"));
    assert!(forge.calls().is_empty(), "{:?}", forge.calls());
}
