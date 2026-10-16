//! Each integration test runs `riff` and `riff-server` only through the
//! helper (01M3MY2KWKBJCQ0BCNC6533RBW).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use isolated::{Isolated, offenders, program_writers};

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

#[test]
fn no_test_file_runs_a_binary_of_riff_without_the_helper() {
    let found = offenders(repo());
    assert!(
        found.is_empty(),
        "these lines name a binary of riff; use isolated::Isolated: {found:#?}"
    );
}

#[test]
fn the_check_fails_a_test_file_that_runs_the_binary_without_the_helper() {
    let root = tempfile::tempdir().unwrap();
    let tests = root.path().join("crates/riff-server/tests");
    std::fs::create_dir_all(tests.join("common")).unwrap();
    std::fs::write(
        tests.join("common/mod.rs"),
        "pub fn server() { Command::new(env!(\"CARGO_BIN_EXE_riff-server\")); }\n",
    )
    .unwrap();
    std::fs::write(
        tests.join("good.rs"),
        "let env = Isolated::new();\nenv.riff_server().arg(\"--help\");\n",
    )
    .unwrap();
    assert_eq!(
        offenders(root.path()),
        ["crates/riff-server/tests/common/mod.rs:1"]
    );
}

/// 01M517P9J77TZX28QYBR9SH6WW.
#[test]
fn no_test_file_writes_a_program_in_the_test_process() {
    let found = program_writers(repo());
    assert!(
        found.is_empty(),
        "these lines write a program in the test process; use isolated::write_program: \
         {found:#?}"
    );
}

/// The check fails a write over a program that the helper made, and a
/// write of a script, also when the write sets no mode
/// (01M517P9J77TZX28QYBR9SH6WW).
#[test]
fn the_check_fails_a_new_text_of_a_program_and_a_script_with_no_mode() {
    let root = tempfile::tempdir().unwrap();
    let tests = root.path().join("crates/riff/tests");
    std::fs::create_dir_all(&tests).unwrap();
    std::fs::write(
        tests.join("usage.rs"),
        "impl Machine {\n\
         \x20   fn new() -> Machine {\n\
         \x20       let gh = bin.path().join(\"gh\");\n\
         \x20       isolated::write_program(&gh, FAKE_GH);\n\
         \x20   }\n\
         }\n\
         fn knows_pr_40(machine: &Machine) {\n\
         \x20   let gh = machine.bin.path().join(\"gh\");\n\
         \x20   std::fs::write(&gh, with_pr).unwrap();\n\
         }\n",
    )
    .unwrap();
    std::fs::write(
        tests.join("lifecycle.rs"),
        "let cargo = bin.path().join(\"cargo\");\n\
         std::fs::write(\n\
         \x20   &cargo,\n\
         \x20   format!(\"#!/bin/sh\\necho \\\"$*\\\" >> {}/cargo.log\\n\", bin.path().display()),\n\
         )\n\
         .unwrap();\n",
    )
    .unwrap();
    std::fs::write(
        tests.join("good.rs"),
        "let gh = isolated::write_program(bin.join(\"gh\"), FAKE_GH);\n\
         isolated::write_program(&gh, with_pr);\n\
         std::fs::write(bin.join(\"gh.log\"), \"\").unwrap();\n",
    )
    .unwrap();
    assert_eq!(
        program_writers(root.path()),
        [
            "crates/riff/tests/lifecycle.rs:2",
            "crates/riff/tests/usage.rs:9"
        ]
    );
}

/// A program of the helper runs at once each time, while other threads
/// of the process start children all the time (01M517P9J77TZX28QYBR9SH6WW).
#[test]
fn a_program_of_the_helper_runs_while_other_threads_start_children() {
    /// Stops the other threads also when an assert fails.
    struct Stop<'a>(&'a AtomicBool);
    impl Drop for Stop<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let stop = AtomicBool::new(false);
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    std::process::Command::new("true").status().unwrap();
                }
            });
        }
        let _stop = Stop(&stop);
        for n in 0..100 {
            let text = format!("#!/bin/sh\necho {n}\n");
            let program = isolated::write_program(dir.path().join(format!("p{n}")), text);
            let out = std::process::Command::new(&program).output();
            let out = out.unwrap_or_else(|e| panic!("run {n}: {e}"));
            assert_eq!(out.stdout, format!("{n}\n").as_bytes());
        }
    });
}

#[test]
fn the_helper_sets_each_dir_in_its_temp_dir() {
    let env = Isolated::new();
    let vars = env.vars();
    let value = |name: &str| {
        vars.iter()
            .rev()
            .find(|(n, _)| n == name)
            .and_then(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("{name} is set"))
    };
    for name in [
        "RIFF_HOME",
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_RUNTIME_DIR",
        "TMPDIR",
    ] {
        let dir = std::path::PathBuf::from(value(name));
        assert!(dir.starts_with(env.path()), "{name}: {}", dir.display());
        assert!(dir.is_dir(), "{name}: {}", dir.display());
    }
    assert_eq!(value("GIT_CONFIG_GLOBAL"), "/dev/null");
    assert_eq!(value("GIT_ALLOW_PROTOCOL"), "file");
    let bus = value("DBUS_SESSION_BUS_ADDRESS");
    assert!(
        bus.to_string_lossy()
            .contains(&env.path().display().to_string())
    );
}

/// `.cargo/config.toml` gives each test a `TMPDIR` outside each git
/// repository, also a plain `cargo test` in a worker whose home is a
/// repository (01M49NP2907J4SH4S6MAY09VXE).
#[test]
fn a_test_of_this_repository_has_its_tmpdir_outside_each_repository() {
    assert_eq!(std::env::var_os("TMPDIR"), Some("/var/tmp".into()));
    assert!(!isolated::in_git(&std::env::temp_dir()));
}

/// 01M49NP2JW8JFWYY56K7AK3H05.
#[test]
fn the_helper_is_outside_each_repository() {
    assert!(!isolated::in_git(Isolated::new().path()));
}

#[test]
fn the_temp_dir_goes_when_the_environment_drops() {
    let env = Isolated::new();
    let path = env.path().to_owned();
    drop(env);
    assert!(!path.exists());
}
