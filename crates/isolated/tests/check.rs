//! Each integration test runs `riff` and `riff-server` only through the
//! helper (01M3MY2KWKBJCQ0BCNC6533RBW).

use std::path::Path;

use isolated::{Isolated, offenders};

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

/// A test gets a `TMPDIR` outside each git repository: `/var/tmp` from
/// `.cargo/config.toml`, or the `/tmp` of a test run
/// (01M4C5WFP1CGG4PYPEP2NVRGKQ).
#[test]
fn a_test_of_this_repository_has_its_tmpdir_outside_each_repository() {
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
