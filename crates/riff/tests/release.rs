//! The static riff of a release: `deploy/static/build.sh` builds it in
//! the Rust image of the channel of the repository
//! (01M4ZPJ04D9MJHNPAV0WN1KCES), `deploy/static/check.sh` refuses a
//! binary with an interpreter, the CI job "Static riff" runs both for
//! each pull request (01M4ZPJ06QYVA23N9SQ45T8WQC), and the job "Static
//! riff release" adds the files that `riff update` downloads to the
//! release of a tag (01M4ZPJ091W3NYV1V0FJS9K6MS). The scripts run a fake
//! container engine that logs its arguments.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;

use isolated::Isolated;
use riff::release::{BINARIES, SUMS, asset};

/// The root of this repository.
fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// The text of the file `path` of this repository.
fn read(path: &str) -> String {
    fs::read_to_string(top().join(path)).unwrap()
}

/// The job `name` of the CI workflow, up to the next job.
fn ci_job(name: &str) -> String {
    let text = read(".github/workflows/ci.yml");
    let jobs = text.split_once("\njobs:").unwrap().1;
    let job = jobs.split_once(&format!("\n  {name}:\n")).unwrap().1;
    let end = job
        .match_indices("\n  ")
        .find(|(at, _)| job[at + 3..].starts_with(|c: char| c.is_ascii_alphabetic()))
        .map_or(job.len(), |(at, _)| at);
    job[..end].to_owned()
}

/// A fake container engine `podman` in `bin`: it logs each argument on
/// a line to `bin/podman.log`, and `RIFF_COMMIT` to `bin/podman.env`.
fn fake_engine(bin: &Path) {
    let path = bin.join("podman");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done >> {log}\n\
             echo \"RIFF_COMMIT=$RIFF_COMMIT\" > {env}\n",
            log = bin.join("podman.log").display(),
            env = bin.join("podman.env").display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// Runs the script `name` of `deploy/static` with `args`, the fake
/// engine of `bin` first in `PATH`, and `vars`.
fn script(name: &str, args: &[&str], bin: &Path, vars: &[(&str, &Path)]) -> Output {
    let env = Isolated::new();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let mut cmd = env.command(top().join("deploy/static").join(name));
    cmd.args(args).env("PATH", path).env_remove("RIFF_ENGINE");
    for (var, value) in vars {
        cmd.env(var, value);
    }
    cmd.output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// 01M4ZPJ04D9MJHNPAV0WN1KCES: the build runs in
/// `rust:CHANNEL-alpine`, with the channel of `rust-toolchain.toml`, the
/// tree at `/src`, the target in `target/static`, and the commit of
/// `deploy/build-id.sh`, since the container has no git.
#[test]
fn the_static_build_runs_in_the_rust_image_of_the_channel_of_the_repository() {
    let toolchain: toml_edit::DocumentMut = read("rust-toolchain.toml").parse().unwrap();
    let channel = toolchain["toolchain"]["channel"].as_str().unwrap();
    let bin = tempfile::tempdir().unwrap();
    fake_engine(bin.path());

    let out = script("build.sh", &["release"], bin.path(), &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let log = fs::read_to_string(bin.path().join("podman.log")).unwrap();
    let args: Vec<&str> = log.lines().collect();
    let has = |pair: [&str; 2]| args.windows(2).any(|w| w == pair);
    let tree = format!("{}:/src", top().display());
    assert_eq!(args[..2], ["run", "--rm"], "{args:?}");
    assert!(has(["-v", &tree]), "{args:?}");
    assert!(has(["-e", "CARGO_TARGET_DIR=/src/target/static"]), "{args:?}");
    assert!(has(["-e", "RIFF_COMMIT"]), "{args:?}");
    assert!(has(["-e", "RIFF_COMMIT_TIME"]), "{args:?}");
    let image = format!("docker.io/library/rust:{channel}-alpine");
    assert!(args.contains(&image.as_str()), "{args:?}");
    let run = args.iter().find(|a| a.contains("cargo build")).unwrap();
    assert!(
        run.contains("cargo build --locked --profile \"$1\" -p riff -p riff-server"),
        "{run}"
    );
    assert!(run.contains("apk add -q --no-cache musl-dev linux-headers"), "{run}");
    assert_eq!(args.last(), Some(&"release"), "{args:?}");

    let id = std::process::Command::new(top().join("deploy/build-id.sh"))
        .output()
        .unwrap();
    let commit = text(&id.stdout).lines().next().unwrap().to_owned();
    let env = fs::read_to_string(bin.path().join("podman.env")).unwrap();
    assert_eq!(env.trim(), commit);
}

#[test]
fn the_static_build_and_check_refuse_another_profile() {
    let bin = tempfile::tempdir().unwrap();
    fake_engine(bin.path());
    for name in ["build.sh", "check.sh"] {
        let out = script(name, &["fast"], bin.path(), &[]);
        assert_eq!(out.status.code(), Some(2), "{name}");
        assert!(
            text(&out.stderr).contains("\"fast\" is not a profile. Give release or dev."),
            "{name}: {}",
            text(&out.stderr)
        );
    }
    assert!(!bin.path().join("podman.log").exists());
}

/// 01M4ZPJ06QYVA23N9SQ45T8WQC: the check refuses a binary that needs
/// shared libraries, as the riff of a plain `cargo build` does, and a
/// file that is not an ELF. It runs no container then.
#[cfg(target_env = "gnu")]
#[test]
fn the_static_check_refuses_a_binary_with_an_interpreter() {
    let bin = tempfile::tempdir().unwrap();
    fake_engine(bin.path());
    let dir = tempfile::tempdir().unwrap();
    let riff = Isolated::shared().riff_path();
    for name in ["riff", "riff-server"] {
        fs::copy(&riff, dir.path().join(name)).unwrap();
    }
    let out = script(
        "check.sh",
        &["release"],
        bin.path(),
        &[("RIFF_STATIC_DIR", dir.path())],
    );
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(&format!(
            "{}/riff needs the shared libraries of /lib64/ld-linux-x86-64.so.2: it is not static.",
            dir.path().display()
        )),
        "{stderr}"
    );

    fs::write(dir.path().join("riff"), "#!/bin/sh\n").unwrap();
    let out = script(
        "check.sh",
        &["release"],
        bin.path(),
        &[("RIFF_STATIC_DIR", dir.path())],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("/riff is not an ELF file."),
        "{}",
        text(&out.stderr)
    );
    assert!(!bin.path().join("podman.log").exists());
}

/// 01M4ZPJ06QYVA23N9SQ45T8WQC: CI builds and checks the static riff of
/// each pull request and each push, with the profile of a release.
#[test]
fn ci_builds_and_checks_the_static_riff_of_each_pull_request() {
    let job = ci_job("static");
    for part in [
        "name: Static riff\n",
        "if: github.event_name == 'push' || github.event_name == 'pull_request'\n",
        "fetch-depth: 0\n",
        "run: deploy/static/build.sh release\n",
        "run: deploy/static/check.sh release\n",
    ] {
        assert!(job.contains(part), "{part} is not in the job static:\n{job}");
    }
    assert!(!job.contains("contents: write"), "{job}");
}

/// 01M4ZPJ091W3NYV1V0FJS9K6MS: for a tag, the job static names the
/// files as `riff update` asks for them, and the job static_release adds
/// them to the release of the tag. Only that job writes the repository.
#[test]
fn a_release_tag_gets_the_files_that_riff_update_downloads() {
    let target = riff::release::TARGET.unwrap_or("x86_64-unknown-linux-musl");
    let job = ci_job("static");
    let mut binaries = BINARIES;
    binaries.sort();
    let each = format!("for name in {}; do", binaries.join(" "));
    let copy = format!("cp \"target/static/release/$name\" \"dist/$name-{target}\"");
    let assets: Vec<String> = binaries.iter().map(|b| asset(b, target)).collect();
    let sums = format!("sha256sum -- {} > {SUMS}\n", assets.join(" "));
    for part in [each, copy, sums] {
        assert!(job.contains(&part), "{part} is not in the job static:\n{job}");
    }
    assert!(job.contains("if: github.ref_type == 'tag'"), "{job}");

    let publish = ci_job("static_release");
    for part in [
        "name: Static riff release\n",
        "needs: [static]\n",
        "if: github.ref_type == 'tag'\n",
        "contents: write\n",
        "gh release view \"$TAG\"",
        "for _ in $(seq 120); do",
        "sleep 30",
        "run: gh release upload \"$TAG\" dist/* --clobber",
    ] {
        assert!(
            publish.contains(part),
            "{part} is not in the job static_release:\n{publish}"
        );
    }
}
