//! `hygiene test-build DIR` and `hygiene test-part I/N --archive DIR`
//! with a fake `cargo` and fake test binaries: the Gate builds the test
//! binaries one time, and each part runs from the archive with no cargo
//! (01M6QQ1NXY0CXMZEEEP413W4PK, 01M6QQ1P5YCT0JX90X5FCKM4W1).

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use hygiene::archive::Manifest;
use hygiene::part::{Part, names};

/// The names that the test binary `all` of crate `a` lists.
const ALL: &str = "one::a: test\none::b: test\nshared::name: test\nslow::c: test\n";

/// The names that the test binary `unit` of crate `b` lists: `shared::name`
/// is in both binaries.
const UNIT: &str = "two::a: test\ntwo::b: test\nshared::name: test\n";

/// The test binary `name` in `dir`: `--list` prints `list`, and a run
/// writes its folder, `TMPDIR`, `CARGO_MANIFEST_DIR` and each argument to
/// `run.log` of the checkout `top`, and exits with the number in the file
/// `top/exit-NAME` when there is one.
fn test_binary(top: &Path, dir: &str, name: &str, list: &str) {
    let path = top.join(dir).join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let script = format!(
        "#!/bin/sh\nlist='{list}'\ncase \" $* \" in\n\
         *' --list '*) printf '%b' \"$list\" ;;\n\
         *) {{ echo \"--run-- {name}\"; pwd; echo \"TMPDIR=$TMPDIR\"; \
         echo \"MANIFEST=$CARGO_MANIFEST_DIR\"; for a in \"$@\"; do printf '%s\\n' \"$a\"; done; }} \
         >> '{log}'\n\
         if [ -f '{top}/exit-{name}' ]; then exit \"$(cat '{top}/exit-{name}')\"; fi ;;\n\
         esac\n",
        list = list.replace('\n', "\\n").replace('\'', ""),
        log = top.join("run.log").display(),
        top = top.display(),
    );
    isolated::write_program(path, script);
}

/// A checkout with two crates, their test binaries and a binary, and a
/// fake `cargo` that prints the JSON messages of the build of them. A run
/// of the fake cargo for the doc tests writes `doc` to `run.log`.
fn checkout() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let top = dir.path().canonicalize().unwrap();
    for name in ["a", "b"] {
        std::fs::create_dir_all(top.join("crates").join(name)).unwrap();
    }
    let deps = "target/session/debug/deps";
    test_binary(&top, deps, "all-1f", ALL);
    test_binary(&top, deps, "unit-2e", UNIT);
    isolated::write_program(top.join("target/session/debug/tool"), "#!/bin/sh\n");
    let message = |kind: &str, test: bool, krate: &str, exe: &str| {
        format!(
            "{{\"reason\":\"compiler-artifact\",\"manifest_path\":\"{top}/crates/{krate}/Cargo.toml\",\
             \"target\":{{\"kind\":[\"{kind}\"],\"name\":\"n\"}},\"profile\":{{\"test\":{test}}},\
             \"executable\":\"{top}/target/session/debug/{exe}\"}}",
            top = top.display()
        )
    };
    let build = message("bin", false, "a", "tool");
    let tests = [
        message("test", true, "a", "deps/all-1f"),
        message("test", true, "b", "deps/unit-2e"),
        message("bin", false, "a", "tool"),
    ]
    .join("\n");
    let cargo = format!(
        "#!/bin/sh\necho \"cargo $*\" >> '{top}/cargo.log'\n\
         case \" $* \" in\n\
         *' --doc '*) echo doc >> '{top}/run.log'; exit \"$(cat '{top}/exit-doc' 2>/dev/null || echo 0)\" ;;\n\
         *' build '*) echo '   Compiling x' >&2; cat <<'EOF'\n{build}\nEOF\n;;\n\
         *' test '*) cat <<'EOF'\n{tests}\nEOF\n;;\n\
         esac\n",
        top = top.display()
    );
    isolated::write_program(top.join("fake-cargo"), cargo);
    (dir, top)
}

fn hygiene(top: &Path, args: &[&str], cargo: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hygiene"))
        .args(args)
        .current_dir(top)
        .env("CARGO", cargo)
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

fn build(top: &Path) -> Output {
    hygiene(top, &["test-build", "archive"], &top.join("fake-cargo"))
}

/// The runs of the test binaries in `run.log`: the lines of each run.
fn runs(top: &Path) -> Vec<Vec<String>> {
    let log = std::fs::read_to_string(top.join("run.log")).unwrap_or_default();
    log.split("--run-- ")
        .filter(|run| !run.is_empty())
        .map(|run| run.lines().map(str::to_owned).collect())
        .collect()
}

/// Unpacks the tar of the archive in `top`, as the job of a part does,
/// after it removed the build.
fn unpack(top: &Path) {
    std::fs::remove_dir_all(top.join("target")).unwrap();
    let out = Command::new("tar")
        .args(["-xf", "archive/tests.tar", "-C"])
        .arg(top)
        .current_dir(top)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
}

#[test]
fn the_test_build_packs_each_test_binary_and_each_binary() {
    let (_dir, top) = checkout();
    let out = build(&top);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));

    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(top.join("archive/manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.root, top.to_string_lossy());
    let tests: Vec<(&str, &str)> = manifest
        .tests
        .iter()
        .map(|t| (t.dir.as_str(), t.exe.as_str()))
        .collect();
    assert_eq!(
        tests,
        [
            ("crates/a", "target/session/debug/deps/all-1f"),
            ("crates/b", "target/session/debug/deps/unit-2e"),
        ]
    );
    assert_eq!(manifest.bins, ["target/session/debug/tool"]);

    let list = Command::new("tar")
        .args(["-tf", "archive/tests.tar"])
        .current_dir(&top)
        .output()
        .unwrap();
    let mut files: Vec<String> = text(&list.stdout).lines().map(str::to_owned).collect();
    files.sort();
    assert_eq!(
        files,
        [
            "target/session/debug/deps/all-1f",
            "target/session/debug/deps/unit-2e",
            "target/session/debug/tool",
        ]
    );

    // The build, the test build and the doc tests: the doc tests run
    // once, in the build, and in no part.
    let cargo = std::fs::read_to_string(top.join("cargo.log")).unwrap();
    let lines: Vec<&str> = cargo.lines().collect();
    assert_eq!(
        lines,
        [
            "cargo build --workspace --bins --message-format=json-render-diagnostics",
            "cargo test --workspace --no-run --message-format=json-render-diagnostics",
            "cargo test --workspace --doc --no-fail-fast",
        ]
    );
    assert_eq!(runs(&top), [["doc"]]);
    // Cargo shows its own build lines.
    assert!(text(&out.stderr).contains("   Compiling x\n"));
}

#[test]
fn a_failed_doc_test_fails_the_build_and_writes_no_archive() {
    let (_dir, top) = checkout();
    std::fs::write(top.join("exit-doc"), "101").unwrap();
    let out = build(&top);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert!(!top.join("archive").exists());
}

#[test]
fn the_parts_run_each_test_one_time_with_no_cargo() {
    let (_dir, top) = checkout();
    assert_eq!(build(&top).status.code(), Some(0));
    unpack(&top);
    std::fs::remove_file(top.join("run.log")).unwrap();
    // A cargo that does not exist: a part must start none.
    let no_cargo = top.join("no-cargo-here");

    let count = 3;
    let mut ran: Vec<String> = Vec::new();
    for index in 1..=count {
        let part = Part { index, count };
        let out = hygiene(
            &top,
            &["test-part", &part.to_string(), "--archive", "archive"],
            &no_cargo,
        );
        assert_eq!(out.status.code(), Some(0), "{part}: {}", text(&out.stderr));
        let stderr = text(&out.stderr);
        let run: usize = [ALL, UNIT]
            .iter()
            .map(|list| part.plan(&names(list)).tests.len())
            .sum();
        assert!(
            stderr.contains(&format!(
                "just test-part {part} runs {run} of 7 tests of the archive\n"
            )),
            "{stderr}"
        );
        for run in runs(&top) {
            // The first line is the name of the binary.
            assert_eq!(
                run[1],
                top.join(if run[0] == "all-1f" {
                    "crates/a"
                } else {
                    "crates/b"
                })
                .to_string_lossy()
            );
            assert_eq!(run[2], "TMPDIR=/var/tmp");
            assert_eq!(run[3], format!("MANIFEST={}", run[1]));
            assert_eq!(run[4], "--exact");
            ran.extend(run[5..].iter().map(|name| format!("{}:{name}", run[0])));
        }
        std::fs::remove_file(top.join("run.log")).unwrap();
    }
    ran.sort();
    // Each test one time: the name that two binaries share runs in each.
    assert_eq!(
        ran,
        [
            "all-1f:one::a",
            "all-1f:one::b",
            "all-1f:shared::name",
            "all-1f:slow::c",
            "unit-2e:shared::name",
            "unit-2e:two::a",
            "unit-2e:two::b",
        ]
    );
}

#[test]
fn a_failed_test_binary_fails_the_part_and_the_other_binaries_still_run() {
    let (_dir, top) = checkout();
    assert_eq!(build(&top).status.code(), Some(0));
    unpack(&top);
    std::fs::remove_file(top.join("run.log")).unwrap();
    std::fs::write(top.join("exit-all-1f"), "101").unwrap();
    let out = hygiene(
        &top,
        &["test-part", "1/1", "--archive", "archive"],
        &top.join("no-cargo-here"),
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    let names: Vec<String> = runs(&top).iter().map(|r| r[0].clone()).collect();
    assert_eq!(names, ["all-1f", "unit-2e"]);
}

#[test]
fn a_checkout_at_another_path_fails_before_it_runs_a_test() {
    let (_dir, top) = checkout();
    assert_eq!(build(&top).status.code(), Some(0));
    let other = tempfile::tempdir().unwrap();
    let other = other.path().canonicalize().unwrap();
    let copy = Command::new("cp")
        .args(["-a"])
        .arg(top.join("archive"))
        .arg(top.join("target"))
        .arg(&other)
        .output()
        .unwrap();
    assert!(copy.status.success());
    let out = hygiene(
        &other,
        &["test-part", "1/2", "--archive", "archive"],
        &other.join("no-cargo-here"),
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(&format!(
            "the archive was built in {}, and this folder is {}.",
            top.display(),
            other.display()
        )),
        "{stderr}"
    );
    assert!(!other.join("run.log").exists());
}

#[test]
fn a_part_with_no_unpacked_files_or_no_archive_fails() {
    let (_dir, top) = checkout();
    assert_eq!(build(&top).status.code(), Some(0));
    std::fs::remove_file(top.join("target/session/debug/tool")).unwrap();
    let out = hygiene(
        &top,
        &["test-part", "1/2", "--archive", "archive"],
        &top.join("no-cargo-here"),
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("target/session/debug/tool is missing in"),
        "{}",
        text(&out.stderr)
    );

    let out = hygiene(
        &top,
        &["test-part", "1/2", "--archive", "nowhere"],
        &top.join("no-cargo-here"),
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("nowhere/manifest.json"));

    let out = hygiene(
        &top,
        &["test-part", "3/2", "--archive", "archive"],
        &top.join("no-cargo-here"),
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("is not a part"));
}

/// The runner of the archive gives each test the `TMPDIR` that
/// `.cargo/config.toml` forces for a run with cargo
/// (01M49NP2907J4SH4S6MAY09VXE).
#[test]
fn the_archive_runs_with_the_tmpdir_that_cargo_forces() {
    let config = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cargo/config.toml"),
    )
    .unwrap();
    assert!(
        config.contains("TMPDIR = { value = \"/var/tmp\", force = true }"),
        "{config}"
    );
}

/// 01M75NDQY6BHZBV9PF735E3FGY: `hygiene test-name NAME --archive` runs
/// the tests whose name has NAME, in each test binary that has one, with
/// no cargo. A binary with no such test does not run.
#[test]
fn a_name_runs_the_tests_with_the_name_from_the_archive_with_no_cargo() {
    let (_dir, top) = checkout();
    assert_eq!(build(&top).status.code(), Some(0));
    unpack(&top);
    std::fs::remove_file(top.join("run.log")).unwrap();
    let no_cargo = top.join("no-cargo-here");

    let out = hygiene(
        &top,
        &["test-name", "one::", "--archive", "archive"],
        &no_cargo,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("just ci-test one:: runs 2 of 7 tests of the archive\n"),
        "{}",
        text(&out.stderr)
    );
    let ran = runs(&top);
    assert_eq!(ran.len(), 1, "{ran:?}");
    assert_eq!(ran[0][0], "all-1f");
    assert_eq!(ran[0][1], top.join("crates/a").to_string_lossy());
    assert_eq!(ran[0][2], "TMPDIR=/var/tmp");
    assert_eq!(ran[0][4..], ["--exact", "one::a", "one::b"]);
    std::fs::remove_file(top.join("run.log")).unwrap();

    // A name that two binaries share runs in each of them.
    let out = hygiene(
        &top,
        &["test-name", "shared::name", "--archive", "archive"],
        &no_cargo,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let ran: Vec<(String, Vec<String>)> = runs(&top)
        .into_iter()
        .map(|run| (run[0].clone(), run[5..].to_vec()))
        .collect();
    assert_eq!(
        ran,
        [
            ("all-1f".to_owned(), vec!["shared::name".to_owned()]),
            ("unit-2e".to_owned(), vec!["shared::name".to_owned()]),
        ]
    );
}

/// 01M75NDQY6BHZBV9PF735E3FGY: a name that no test of the archive has,
/// and an empty name, fail with 2 and run no test binary. A failed test
/// fails the run with 1.
#[test]
fn a_name_with_no_test_fails_and_a_failed_test_fails_the_run() {
    let (_dir, top) = checkout();
    assert_eq!(build(&top).status.code(), Some(0));
    unpack(&top);
    std::fs::remove_file(top.join("run.log")).unwrap();
    let no_cargo = top.join("no-cargo-here");

    let out = hygiene(
        &top,
        &["test-name", "three", "--archive", "archive"],
        &no_cargo,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains(&hygiene::archive::none_named("three")),
        "{}",
        text(&out.stderr)
    );
    let out = hygiene(&top, &["test-name", "", "--archive", "archive"], &no_cargo);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("give the name of a test"));
    assert!(runs(&top).is_empty(), "{:?}", runs(&top));

    std::fs::write(top.join("exit-unit-2e"), "101").unwrap();
    let out = hygiene(
        &top,
        &["test-name", "two::a", "--archive", "archive"],
        &no_cargo,
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
}
