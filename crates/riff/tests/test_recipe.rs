//! `just test` runs the tests with a `TMPDIR` outside each git
//! repository (01M43B491Z25KT0XBC7CANFS5G). The test copies the `test`
//! recipe into a justfile of its own, with a fake `cargo` that writes
//! its `TMPDIR` to a file.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `test` recipe of the justfile: its line and its body.
fn recipe() -> String {
    let justfile = std::fs::read_to_string(repo().join("justfile")).unwrap();
    let start = justfile.find("\ntest *ARGS:\n").expect("a test recipe") + 1;
    let end = justfile[start..]
        .find("\n\n")
        .map_or(justfile.len(), |n| start + n);
    justfile[start..end].to_owned()
}

/// The `TMPDIR` that the fake `cargo` of `just test` gets, when `just`
/// runs with `tmp` as its `TMPDIR`.
fn tmpdir_of_cargo(tmp: &Path) -> PathBuf {
    let tree = isolated::outside_git();
    std::fs::write(tree.path().join("justfile"), recipe()).unwrap();
    let bin = tree.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let seen = tree.path().join("seen");
    let cargo = bin.join("cargo");
    std::fs::write(
        &cargo,
        format!(
            "#!/bin/sh\nprintf '%s' \"$TMPDIR\" > '{}'\n",
            seen.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let out = Command::new("just")
        .arg("test")
        .current_dir(tree.path())
        .env("PATH", path)
        .env("TMPDIR", tmp)
        .output()
        .expect("just runs");
    assert!(out.status.success(), "{out:?}");
    PathBuf::from(std::fs::read_to_string(seen).unwrap())
}

#[test]
fn a_tmpdir_in_a_repository_moves_out_of_it() {
    let home = isolated::outside_git();
    std::fs::create_dir_all(home.path().join(".git")).unwrap();
    let tmp = home.path().join(".cache/riff/tmp/w1");
    std::fs::create_dir_all(&tmp).unwrap();
    let seen = tmpdir_of_cargo(&tmp);
    assert!(!isolated::in_git(&seen), "{}", seen.display());
    assert!(seen.is_dir(), "{}", seen.display());
    let name = seen.file_name().unwrap().to_str().unwrap();
    assert!(name.starts_with("riff-test-"), "{name}");
}

#[test]
fn a_tmpdir_outside_each_repository_stays() {
    let tmp = isolated::outside_git();
    assert_eq!(tmpdir_of_cargo(tmp.path()), tmp.path());
}
