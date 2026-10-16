//! `hygiene test-part I/N` with a fake `cargo`, as `just test-part` runs
//! it in each job of the tests of the Gate (01M513W2NSE303E6XAM18BW7T6).

use std::path::Path;
use std::process::{Command, Output, Stdio};

use hygiene::part::{Part, names, summary};

/// The list of the fake cargo: tests of two binaries, a name in two
/// binaries, doc tests, and a benchmark.
const LIST: &str = "a::one: test\n\
                    a::two: test\n\
                    b::three: test\n\
                    b::four: test\n\
                    shared::name: test\n\
                    crates/x/src/lib.rs - five (line 12): test\n\
                    crates/x/src/lib.rs - X::six (line 40): test\n\
                    shared::name: test\n\
                    seven::fast: benchmark\n";

/// The tests of [`LIST`] that are not doc tests.
const TESTS: [&str; 5] = ["a::one", "a::two", "b::four", "b::three", "shared::name"];

/// The run of the doc tests.
const DOC: [&str; 4] = ["test", "--workspace", "--doc", "--no-fail-fast"];

/// The start of the run of the tests that are not doc tests.
const TESTS_RUN: [&str; 6] = [
    "test",
    "--workspace",
    "--tests",
    "--no-fail-fast",
    "--",
    "--exact",
];

/// A directory with a fake `cargo`. For `--list` it prints `list` to
/// stdout and a build line to stderr, and exits with `list-exit`. Else
/// it writes a line `--run--` and each argument on a line to `run.log`,
/// and exits with `run-exit`.
fn fake_cargo(list: &str, list_exit: i32, run_exit: i32) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("list"), list).unwrap();
    let script = format!(
        "#!/bin/sh\nd='{}'\ncase \" $* \" in\n\
         *' --list '*) cat \"$d/list\"; echo '   Compiling x' >&2; exit {list_exit} ;;\n\
         *) {{ echo --run--; for a in \"$@\"; do printf '%s\\n' \"$a\"; done; }} >> \"$d/run.log\"\n\
         exit {run_exit} ;;\n\
         esac\n",
        d.display()
    );
    isolated::write_program(d.join("cargo"), script);
    dir
}

/// Runs `hygiene test-part PART` with the fake cargo of `dir` in `CARGO`.
fn test_part(dir: &Path, part: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hygiene"))
        .args(["test-part", part])
        .env("CARGO", dir.join("cargo"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The runs of the fake cargo in `dir` that are not a list: the
/// arguments of each.
fn runs(dir: &Path) -> Vec<Vec<String>> {
    let log = std::fs::read_to_string(dir.join("run.log")).unwrap_or_default();
    log.split("--run--\n")
        .filter(|run| !run.is_empty())
        .map(|run| run.lines().map(str::to_owned).collect())
        .collect()
}

#[test]
fn the_parts_together_run_each_test_one_time() {
    let all = names(LIST);
    assert_eq!(all.len(), 7, "{all:?}");
    let count = 3;
    let mut ran = Vec::new();
    let mut doc = 0;
    for index in 1..=count {
        let dir = fake_cargo(LIST, 0, 0);
        let part = Part { index, count };
        let out = test_part(dir.path(), &part.to_string());
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        let plan = part.plan(&all);
        let stderr = text(&out.stderr);
        assert!(stderr.contains("   Compiling x\n"), "{stderr}");
        let line = summary(part, &plan, &all);
        assert!(stderr.contains(&format!("{line}\n")), "{stderr}");
        let mut each = runs(dir.path());
        if index == 1 {
            assert_eq!(each.pop().unwrap(), DOC);
            doc += 1;
        }
        if plan.tests.is_empty() {
            assert!(each.is_empty(), "{each:?}");
            continue;
        }
        assert_eq!(each.len(), 1, "{each:?}");
        let args = &each[0];
        assert_eq!(args[..6], TESTS_RUN);
        assert_eq!(args[6..], plan.tests);
        ran.extend(args[6..].iter().cloned());
    }
    ran.sort();
    assert_eq!(ran, TESTS);
    assert_eq!(doc, 1);
}

#[test]
fn a_part_with_no_test_runs_nothing() {
    let all = names(LIST);
    let count = 40;
    let empty = (2..=count)
        .map(|index| Part { index, count })
        .find(|part| part.plan(&all).tests.is_empty())
        .unwrap();
    let dir = fake_cargo(LIST, 0, 0);
    let out = test_part(dir.path(), &empty.to_string());
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains(&format!("just test-part {empty} runs 0 of 5 tests\n")),
        "{}",
        text(&out.stderr)
    );
    assert!(runs(dir.path()).is_empty());
}

#[test]
fn a_failed_test_fails_the_part_and_the_doc_tests_still_run() {
    let dir = fake_cargo(LIST, 0, 101);
    let out = test_part(dir.path(), "1/1");
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("just test-part 1/1 runs 5 of 5 tests, and the 2 doc tests\n"),
        "{}",
        text(&out.stderr)
    );
    let each = runs(dir.path());
    assert_eq!(each.len(), 2, "{each:?}");
    assert_eq!(each[0][..6], TESTS_RUN);
    assert_eq!(each[0][6..], TESTS);
    assert_eq!(each[1], DOC);
}

#[test]
fn a_failed_build_fails_the_part_and_runs_no_test() {
    let dir = fake_cargo(LIST, 101, 0);
    let out = test_part(dir.path(), "1/2");
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("error: cargo test --workspace -- --list --format terse:"),
        "{}",
        text(&out.stderr)
    );
    assert!(runs(dir.path()).is_empty());
}

#[test]
fn a_list_with_no_test_is_an_error() {
    let dir = fake_cargo("2 tests, 0 benchmarks\n", 0, 0);
    let out = test_part(dir.path(), "1/2");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("listed no test"),
        "{}",
        text(&out.stderr)
    );
    assert!(runs(dir.path()).is_empty());
}

#[test]
fn a_bad_part_is_an_error() {
    let dir = fake_cargo(LIST, 0, 0);
    for bad in ["0/2", "3/2", "two", "1/0"] {
        let out = test_part(dir.path(), bad);
        assert_eq!(out.status.code(), Some(2), "{bad}");
        let stderr = text(&out.stderr);
        assert!(
            stderr.contains(&format!("error: \"{bad}\" is not a part.")),
            "{stderr}"
        );
        assert!(stderr.contains("hygiene test-part I/N"), "{stderr}");
    }
    assert!(runs(dir.path()).is_empty());
}
