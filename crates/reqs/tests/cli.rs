//! `reqs rid` and `reqs check` as `just rid` and `just ci` run them.

use std::path::Path;
use std::process::{Command, Output};

fn reqs(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_reqs"))
        .args(args)
        .output()
        .unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A repository with `requirements` and one source file.
fn repo(requirements: &str, source: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let write = |path: &str, text: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("docs/src/requirements.md", requirements);
    write("crates/a/src/lib.rs", source);
    write("crates/a/target/debug/x.rs", "// R404 is build output");
    dir
}

fn check(dir: &Path) -> Output {
    reqs(&["check", dir.to_str().unwrap()])
}

#[test]
fn rid_prints_a_new_ulid_each_time() {
    let one = String::from_utf8(reqs(&["rid"]).stdout).unwrap();
    let two = String::from_utf8(reqs(&["rid"]).stdout).unwrap();
    assert!(reqs::is_ulid(one.trim()), "{one}");
    assert!(reqs::is_ulid(two.trim()), "{two}");
    assert_ne!(one, two);
}

#[test]
fn a_clean_repository_passes() {
    let id = reqs::new_id();
    let dir = repo(
        &format!("- **R1** One.\n- **{id}** Two.\n"),
        &format!("// R1, {id}\n"),
    );
    let out = check(dir.path());
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stderr(&out), "");
}

#[test]
fn a_duplicate_id_fails_and_names_the_id() {
    let id = reqs::new_id();
    let dir = repo(&format!("- **{id}** One.\n- **{id}** Two.\n"), "");
    let out = check(dir.path());
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("duplicate-id"), "{err}");
    assert!(err.contains(&id), "{err}");
}

#[test]
fn a_malformed_id_warns_but_passes() {
    let dir = repo("- **REQ-7** One.\n", "");
    let out = check(dir.path());
    assert!(out.status.success());
    assert!(stderr(&out).starts_with("warning: "), "{}", stderr(&out));
    assert!(stderr(&out).contains("id-format: REQ-7"));
}

#[test]
fn the_next_r_number_warns() {
    let dir = repo("- **R232** Old.\n- **R233** New, by habit.\n", "");
    let out = check(dir.path());
    assert!(out.status.success());
    assert!(stderr(&out).contains("id-format: R233"), "{}", stderr(&out));
    assert!(!stderr(&out).contains("R232"), "{}", stderr(&out));
}

#[test]
fn a_cited_id_that_does_not_exist_fails() {
    let dir = repo("- **R1** One.\n", "/// See R9.\n");
    let out = check(dir.path());
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains("crates/a/src/lib.rs:1: unknown-id: R9"),
        "{err}"
    );
    assert!(!err.contains("R404"), "build output is not read: {err}");
}

#[test]
fn this_repository_passes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = check(&root);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stderr(&out), "", "no warnings");
}
