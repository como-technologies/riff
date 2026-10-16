//! `hygiene test-part I/N` with a fake `cargo`, as `just test-part` runs
//! it in each job of the tests of the Gate (01M513W2NSE303E6XAM18BW7T6).

use std::path::Path;
use std::process::{Command, Output, Stdio};

use hygiene::part::Part;

use crate::SPAWN;

/// The list of the fake cargo: tests of two binaries, doc tests, a name
/// in two binaries, and a benchmark.
const LIST: &str = "a::one: test\n\
                    a::two: test\n\
                    b::three: test\n\
                    b::four: test\n\
                    shared::name: test\n\
                    crates/x/src/lib.rs - five (line 12): test\n\
                    crates/x/src/lib.rs - X::six (line 40): test\n\
                    shared::name: test\n\
                    seven::fast: benchmark\n";

/// A directory with a fake `cargo`. For `--list` it prints `list` to
/// stdout and a build line to stderr, and exits with `list-exit`. Else
/// it writes each argument on a line to `run.log`, and exits with
/// `run-exit`.
fn fake_cargo(list: &str, list_exit: i32, run_exit: i32) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("list"), list).unwrap();
    let script = format!(
        "#!/bin/sh\nd='{}'\ncase \" $* \" in\n\
         *' --list '*) cat \"$d/list\"; echo '   Compiling x' >&2; exit {list_exit} ;;\n\
         *) for a in \"$@\"; do printf '%s\\n' \"$a\"; done >> \"$d/run.log\"; exit {run_exit} ;;\n\
         esac\n",
        d.display()
    );
    let cargo = d.join("cargo");
    let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
    std::fs::write(&cargo, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

/// Runs `hygiene test-part PART` with the fake cargo of `dir` in `CARGO`.
fn test_part(dir: &Path, part: &str) -> Output {
    let child = {
        let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        Command::new(env!("CARGO_BIN_EXE_hygiene"))
            .args(["test-part", part])
            .env("CARGO", dir.join("cargo"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The runs of the fake cargo in `dir`: the arguments of each run that
/// is not a list.
fn runs(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("run.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_parts_together_run_each_test_one_time() {
    let names = hygiene::part::names(LIST);
    assert_eq!(names.len(), 7, "{names:?}");
    let count = 3;
    let mut ran = Vec::new();
    for index in 1..=count {
        let dir = fake_cargo(LIST, 0, 0);
        let part = format!("{index}/{count}");
        let out = test_part(dir.path(), &part);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        let mine = Part { index, count }.select(&names);
        let stderr = text(&out.stderr);
        assert!(stderr.contains("   Compiling x\n"), "{stderr}");
        assert!(
            stderr.contains(&format!(
                "just test-part {part} runs {} of 7 tests\n",
                mine.len()
            )),
            "{stderr}"
        );
        let args = runs(dir.path());
        if mine.is_empty() {
            assert!(args.is_empty(), "{args:?}");
            continue;
        }
        assert_eq!(args[..4], ["test", "--workspace", "--", "--exact"]);
        assert_eq!(args[4..], mine);
        ran.extend(args[4..].iter().cloned());
    }
    ran.sort();
    assert_eq!(ran, names);
}

#[test]
fn a_part_with_no_test_runs_nothing() {
    let names = hygiene::part::names(LIST);
    let count = 40;
    let empty = (1..=count)
        .map(|index| Part { index, count })
        .find(|part| part.select(&names).is_empty())
        .unwrap();
    let dir = fake_cargo(LIST, 0, 0);
    let out = test_part(dir.path(), &empty.to_string());
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains(&format!("just test-part {empty} runs 0 of 7 tests")));
    assert!(runs(dir.path()).is_empty());
}

#[test]
fn a_failed_test_fails_the_part() {
    let dir = fake_cargo(LIST, 0, 101);
    let out = test_part(dir.path(), "1/1");
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert_eq!(runs(dir.path()).len(), 4 + 7);
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
