//! `just lint-macos`: the clippy of `just test-macos` for a Mac with
//! Apple silicon, on each host, with no link, and `just check` runs it
//! (01M77NFYCGDD3GKCE4RXDPKRNE). The tests read the justfile, and run
//! `deploy/lint-macos.sh` with a fake `rustc` and a fake `cargo` first on
//! the `PATH`. One test runs it with the real ones on a small crate.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The Rust target of a Mac with Apple silicon.
const TARGET: &str = "aarch64-apple-darwin";

/// The line of the script when the target is missing.
const MISSING: &str = "lint-macos: the Rust target aarch64-apple-darwin is missing. \
                       Add it: rustup target add aarch64-apple-darwin\n";

fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The lines of the recipe `name` of the justfile, with no indent.
fn body(name: &str) -> Vec<String> {
    let justfile = std::fs::read_to_string(top().join("justfile")).unwrap();
    let start = format!("\n{name}:\n");
    justfile
        .split_once(&start)
        .unwrap_or_else(|| panic!("the justfile has no recipe {name}"))
        .1
        .lines()
        .take_while(|line| line.starts_with("    "))
        .map(|line| line.trim().to_owned())
        .collect()
}

/// A fake `rustc` and a fake `cargo` in `dir/bin`. The `rustc` prints
/// `dir/sysroot/lib/rustlib/TARGET/lib` for `--print target-libdir
/// --target TARGET`. The `cargo` writes each argument on a line of
/// `dir/cargo.log`, and exits with the number in `dir/cargo.exit`, else
/// with 0.
fn fakes(dir: &Path) {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    isolated::write_program(
        bin.join("rustc"),
        format!(
            "#!/bin/sh\n[ \"$*\" = \"--print target-libdir --target {TARGET}\" ] || exit 9\n\
             echo '{}/sysroot/lib/rustlib/{TARGET}/lib'\n",
            dir.display()
        ),
    );
    isolated::write_program(
        bin.join("cargo"),
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}/cargo.log'\n\
             exit \"$(cat '{0}/cargo.exit' 2>/dev/null || echo 0)\"\n",
            dir.display()
        ),
    );
}

/// The folder of the Rust target of a Mac in the sysroot of the fake
/// `rustc` of `dir`.
fn install_target(dir: &Path) {
    std::fs::create_dir_all(dir.join(format!("sysroot/lib/rustlib/{TARGET}/lib"))).unwrap();
}

/// Runs `deploy/lint-macos.sh ARGS` in `dir`, with the fakes of `dir`
/// first on the `PATH`.
fn lint(dir: &Path, args: &[&str]) -> Output {
    let path = format!(
        "{}:{}",
        dir.join("bin").display(),
        std::env::var("PATH").unwrap()
    );
    Command::new(top().join("deploy/lint-macos.sh"))
        .args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env_remove("RUSTC")
        .output()
        .unwrap()
}

/// The arguments that the fake `cargo` of `dir` got, else `None`.
fn cargo_args(dir: &Path) -> Option<Vec<String>> {
    let log = std::fs::read_to_string(dir.join("cargo.log")).ok()?;
    Some(log.lines().map(str::to_owned).collect())
}

/// The two `--config` arguments of the links override of the
/// `aws-lc-sys` at `version`.
fn links_override(version: &str) -> Vec<String> {
    let links = format!("target.{TARGET}.aws_lc_{}", version.replace('.', "_"));
    vec![
        "--config".into(),
        format!("{links}.rustc-cfg=[\"universal_prefixed\"]"),
        "--config".into(),
        format!("{links}.include=\"\""),
    ]
}

#[test]
fn check_lints_for_a_mac_with_the_clippy_of_test_macos() {
    let check = body("check");
    assert!(
        check.contains(
            &"{{just_executable()}} fmt-check lint lint-macos doc book reqs wrap versions".into()
        ),
        "{check:?}"
    );
    let clippy = body("test-macos")[0]
        .strip_prefix("cargo clippy ")
        .expect("test-macos lints first")
        .to_owned();
    assert_eq!(
        body("lint-macos"),
        [format!("deploy/lint-macos.sh {clippy}")]
    );
    // The command of the missing target is a line of just init.
    let add = MISSING.rsplit_once(": ").unwrap().1.trim_end();
    assert!(body("init").contains(&add.into()), "{:?}", body("init"));
}

#[test]
fn lint_macos_lints_for_a_mac_with_a_links_override_of_each_aws_lc_sys() {
    let dir = tempfile::tempdir().unwrap();
    fakes(dir.path());
    install_target(dir.path());
    std::fs::write(
        dir.path().join("Cargo.lock"),
        "version = 4\n\n\
         [[package]]\nname = \"aws-lc-rs\"\nversion = \"1.18.1\"\n\n\
         [[package]]\nname = \"aws-lc-sys\"\nversion = \"0.45.0\"\n\n\
         [[package]]\nname = \"aws-lc-sys\"\nversion = \"0.44.10\"\n\n\
         [[package]]\nname = \"riff\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    let out = lint(
        dir.path(),
        &["-p", "riff", "--all-targets", "--", "-D", "warnings"],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let mut want: Vec<String> = ["clippy", "--target", TARGET].map(str::to_owned).into();
    want.extend(links_override("0.45.0"));
    want.extend(links_override("0.44.10"));
    want.extend(["-p", "riff", "--all-targets", "--", "-D", "warnings"].map(str::to_owned));
    assert_eq!(cargo_args(dir.path()), Some(want));
}

#[test]
fn lint_macos_fails_when_clippy_fails() {
    let dir = tempfile::tempdir().unwrap();
    fakes(dir.path());
    install_target(dir.path());
    std::fs::write(dir.path().join("cargo.exit"), "101\n").unwrap();
    let out = lint(dir.path(), &["--", "-D", "warnings"]);
    assert_eq!(out.status.code(), Some(101), "{}", text(&out.stderr));
    // With no aws-lc-sys, there is no links override.
    assert_eq!(
        cargo_args(dir.path()),
        Some(
            ["clippy", "--target", TARGET, "--", "-D", "warnings"]
                .map(str::to_owned)
                .into()
        )
    );
}

#[test]
fn lint_macos_with_no_target_of_a_mac_names_the_command_that_adds_it() {
    let dir = tempfile::tempdir().unwrap();
    fakes(dir.path());
    let out = lint(dir.path(), &["-p", "riff"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stderr), MISSING);
    assert_eq!(cargo_args(dir.path()), None, "cargo ran");
}

#[test]
fn lint_macos_with_no_argument_shows_the_usage() {
    let dir = tempfile::tempdir().unwrap();
    fakes(dir.path());
    install_target(dir.path());
    let out = lint(dir.path(), &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).starts_with("usage: deploy/lint-macos.sh ARGS"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(cargo_args(dir.path()), None, "cargo ran");
}

/// The case of the issue: an import that only code under
/// `cfg(target_os = "linux")` uses. The lint of the host passes on Linux,
/// and the lint for a Mac fails. It runs the real `rustc` and `cargo`, on
/// the channel of `rust-toolchain.toml`, so it needs the Rust target of a
/// Mac: `just init` adds it.
#[test]
fn lint_macos_fails_on_an_import_that_only_linux_uses() {
    let dir = tempfile::tempdir().unwrap();
    let toolchain = std::fs::read_to_string(top().join("rust-toolchain.toml")).unwrap();
    let channel = toolchain
        .lines()
        .find(|line| line.starts_with("channel = "))
        .expect("rust-toolchain.toml has a channel");
    std::fs::write(
        dir.path().join("rust-toolchain.toml"),
        format!("[toolchain]\n{channel}\n"),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"only-linux\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
         [workspace]\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("src/lib.rs"),
        "//! A crate with code that only Linux uses.\n\n\
         use std::num::NonZeroU8;\n\n\
         /// One.\n\
         #[cfg(target_os = \"linux\")]\n\
         #[must_use]\n\
         pub fn one() -> u8 {\n    NonZeroU8::MIN.get()\n}\n",
    )
    .unwrap();
    let run = |program: &Path, args: &[&str]| {
        Command::new(program)
            .args(args)
            .current_dir(dir.path())
            .env("CARGO_TARGET_DIR", dir.path().join("target"))
            .env_remove("RUSTC")
            .output()
            .unwrap()
    };
    let flags = ["--all-targets", "--", "-D", "warnings"];
    let out = run(&top().join("deploy/lint-macos.sh"), &flags);
    let stderr = text(&out.stderr);
    assert_ne!(
        stderr, MISSING,
        "the test needs the Rust target of a Mac: just init adds it"
    );
    assert_eq!(out.status.code(), Some(101), "{stderr}");
    assert!(
        stderr.contains("error: unused import: `std::num::NonZeroU8`"),
        "{stderr}"
    );
    if cfg!(target_os = "linux") {
        let mut args = vec!["clippy"];
        args.extend(flags);
        let out = run(Path::new("cargo"), &args);
        assert!(out.status.success(), "{}", text(&out.stderr));
    }
}
