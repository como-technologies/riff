//! The static riff of a release: `deploy/static/build.sh` builds it in
//! the Rust image of the channel of the repository
//! (01M4ZPJ04D9MJHNPAV0WN1KCES), `deploy/static/check.sh` refuses a
//! binary with an interpreter, the CI job "Build" runs the build for each
//! pull request and each push (01M6QQ1NXY0CXMZEEEP413W4PK), the job
//! "Static riff" runs the check (01M4ZPJ06QYVA23N9SQ45T8WQC), and the job
//! "Static riff release" adds the files that `riff update` downloads to
//! the release of a tag (01M4ZPJ091W3NYV1V0FJS9K6MS). The jobs of a Mac
//! add the files of a Mac, and change no job of Linux
//! (01M6Z8F7B3HVVNGQMX8GP1Z7Q1, 01M6Z8FKZRJZ4NY60PQAH9SMHV). The scripts
//! run a fake container engine that logs its arguments.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use isolated::Isolated;
use riff::release::{BINARIES, MAC, MAC_SESSION, SUMS, asset, sums};

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
pub(crate) fn ci_job(name: &str) -> String {
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
/// a line to `bin/podman.log`, and `RIFF_BUILD` to `bin/podman.env`.
pub(crate) fn fake_engine(bin: &Path) {
    let path = bin.join("podman");
    isolated::write_program(
        &path,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done >> {log}\n\
             echo \"RIFF_BUILD=$RIFF_BUILD\" > {env}\n",
            log = bin.join("podman.log").display(),
            env = bin.join("podman.env").display(),
        ),
    );
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
/// tree at `/src`, the target in `target/static`, and the build of
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
    assert!(
        has(["-e", "CARGO_TARGET_DIR=/src/target/static"]),
        "{args:?}"
    );
    assert!(has(["-e", "RIFF_BUILD"]), "{args:?}");
    assert!(has(["-e", "RIFF_BUILD_TIME"]), "{args:?}");
    let image = format!("docker.io/library/rust:{channel}-alpine");
    assert!(args.contains(&image.as_str()), "{args:?}");
    // The script of the container has lines of its own.
    for part in [
        "apk add -q --no-cache musl-dev linux-headers\n",
        "cargo build --locked --profile \"$1\" -p riff -p riff-server\n",
    ] {
        assert!(log.contains(part), "{part} is not in:\n{log}");
    }
    assert_eq!(args.last(), Some(&"release"), "{args:?}");

    let id = std::process::Command::new(top().join("deploy/build-id.sh"))
        .output()
        .unwrap();
    let build = text(&id.stdout).lines().next().unwrap().to_owned();
    let env = fs::read_to_string(bin.path().join("podman.env")).unwrap();
    assert_eq!(env.trim(), build);
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
        isolated::copy_program(&riff, dir.path().join(name));
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

    isolated::write_program(dir.path().join("riff"), "#!/bin/sh\n");
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

/// 01M4ZPJ06QYVA23N9SQ45T8WQC, 01M513W2V4G94R7F4D4Q3M2RA1,
/// 01M6QQ1P8H32E2TQ0CYA87R02V: the job Build builds the static riff of
/// each pull request and each push, and the job Static riff checks it. A
/// tag and a pull request with the label release build the profile
/// release, each other run the profile dev. The check compiles nothing.
#[test]
fn ci_builds_and_checks_the_static_riff_of_each_pull_request() {
    let plan = ci_job("plan");
    for part in [
        "name: Plan\n",
        "PROFILE: ${{ (github.ref_type == 'tag' || (github.event_name == 'pull_request' && \
         contains(github.event.pull_request.labels.*.name, 'release'))) && 'release' || 'dev' }}\n",
        "TESTS: ${{ github.ref_type != 'tag' }}\n",
        "run: deploy/build-plan.sh\n",
    ] {
        assert!(
            plan.contains(part),
            "{part} is not in the job plan:\n{plan}"
        );
    }
    let build = ci_job("build");
    for part in [
        "name: Build\n",
        "needs: [plan]\n",
        "fetch-depth: 0\n",
        "run: deploy/static/build.sh \"$STATIC_PROFILE\"\n",
    ] {
        assert!(
            build.contains(part),
            "{part} is not in the job build:\n{build}"
        );
    }
    assert!(!build.contains("contents: write"), "{build}");

    let job = ci_job("static");
    for part in [
        "name: Static riff\n",
        "needs: [plan, build]\n",
        "if: github.event_name == 'push' || github.event_name == 'pull_request'\n",
        "run: deploy/static/check.sh \"$PROFILE\"\n",
        "actions/download-artifact@",
    ] {
        assert!(
            job.contains(part),
            "{part} is not in the job static:\n{job}"
        );
    }
    assert!(!job.contains("contents: write"), "{job}");
    assert!(!job.contains("build.sh"), "the check builds:\n{job}");
}

/// 01M513W2V4G94R7F4D4Q3M2RA1, 01M6QQ1P8H32E2TQ0CYA87R02V: the job Build
/// keeps a cache of the static build and of the downloaded crates, keyed
/// by the toolchain and `Cargo.lock`. A build of the profile release uses
/// no cache, so each release binary comes from a clean build.
#[test]
fn the_static_build_of_ci_has_a_cache_and_a_release_build_uses_none() {
    let job = ci_job("build");
    let cache = job
        .split_once("name: Cache the static build\n")
        .unwrap_or_else(|| panic!("the job build has no cache of the static build:\n{job}"));
    let step = cache.1.split_once("\n\n").unwrap().0;
    assert!(
        step.contains(
            "if: needs.plan.outputs.build_static == 'true' && \
             needs.plan.outputs.static_profile != 'release'\n"
        ),
        "the cache step runs for a release build:\n{step}"
    );
    for part in [
        "uses: actions/cache@",
        "target/static\n",
        "target/static-cargo\n",
        "key: static-dev-${{ hashFiles('rust-toolchain.toml', 'Cargo.lock') }}\n",
        "restore-keys: static-dev-",
    ] {
        assert!(step.contains(part), "{part} is not in:\n{step}");
    }
    // The other cache is the cache of the test build.
    assert_eq!(job.matches("actions/cache").count(), 2, "{job}");
    assert!(
        job.contains("RIFF_STATIC_CARGO: ${{ github.workspace }}/target/static-cargo\n"),
        "{job}"
    );
    // The container writes as root; the cache saves as the runner user.
    let chown = "run: sudo chown -R \"$(id -u):$(id -g)\" target/static target/static-cargo\n";
    let at = job
        .find(chown)
        .unwrap_or_else(|| panic!("no chown:\n{job}"));
    assert!(
        job[..at]
            .rsplit_once("\n      - ")
            .unwrap()
            .1
            .contains("needs.plan.outputs.static_profile != 'release'\n"),
        "{job}"
    );
}

/// 01M513W2V4G94R7F4D4Q3M2RA1: `build.sh` keeps the downloaded crates in
/// the volume `riff-static-cargo`, or in the volume or the folder of
/// `RIFF_STATIC_CARGO`.
#[test]
fn the_static_build_keeps_the_crates_where_riff_static_cargo_says() {
    let mount = |vars: &[(&str, &Path)]| {
        let bin = tempfile::tempdir().unwrap();
        fake_engine(bin.path());
        let out = script("build.sh", &["dev"], bin.path(), vars);
        assert!(out.status.success(), "{}", text(&out.stderr));
        let log = fs::read_to_string(bin.path().join("podman.log")).unwrap();
        let args: Vec<String> = log.lines().map(str::to_owned).collect();
        let at = args
            .iter()
            .position(|a| a.ends_with(":/usr/local/cargo/registry"))
            .unwrap_or_else(|| panic!("no registry in {args:?}"));
        assert_eq!(args[at - 1], "-v", "{args:?}");
        assert!(args.windows(2).any(|w| w == ["-e", "CARGO_INCREMENTAL"]));
        args[at].clone()
    };
    assert_eq!(mount(&[]), "riff-static-cargo:/usr/local/cargo/registry");
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        mount(&[("RIFF_STATIC_CARGO", dir.path())]),
        format!("{}:/usr/local/cargo/registry", dir.path().display())
    );
}

/// 01M4ZPJ091W3NYV1V0FJS9K6MS: for a tag, the job static names the
/// files as `riff update` asks for them, and the job static_release adds
/// them to the release of the tag. Only that job writes the repository.
#[test]
fn a_release_tag_gets_the_files_that_riff_update_downloads() {
    let target = riff::release::LINUX;
    let job = ci_job("static");
    let mut binaries = BINARIES;
    binaries.sort();
    let each = format!("for name in {}; do", binaries.join(" "));
    let copy = format!("cp \"target/static/release/$name\" \"dist/$name-{target}\"");
    let assets: Vec<String> = binaries.iter().map(|b| asset(b, target)).collect();
    let sums = format!("sha256sum -- {} > {SUMS}\n", assets.join(" "));
    for part in [each, copy, sums] {
        assert!(
            job.contains(&part),
            "{part} is not in the job static:\n{job}"
        );
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

/// 01M6Z8F7B3HVVNGQMX8GP1Z7Q1, 01M6Z8FKZRJZ4NY60PQAH9SMHV: for a tag, the
/// job macOS riff builds `riff` and `riff-server` on a Mac with Apple
/// silicon, the job Linux riff of a Mac builds the static Linux riff on
/// arm64, and the job macOS riff release adds the files that `riff
/// update` on a Mac downloads to the release, with their own file of
/// sums, the sums last. A build of a release uses no cache.
#[test]
fn a_release_tag_gets_the_files_that_riff_update_on_a_mac_downloads() {
    let mac = ci_job("macos_riff");
    let copy = format!("cp \"target/release/$name\" \"dist/$name-{MAC}\"");
    for part in [
        "name: macOS riff\n",
        "if: github.ref_type == 'tag'\n",
        "runs-on: macos-latest\n",
        "continue-on-error: true\n",
        "run: test \"$(uname -sm)\" = \"Darwin arm64\"\n",
        "run: cargo build --locked --profile release -p riff -p riff-server\n",
        "target/release/riff --version\n",
        "for name in riff riff-server; do",
        &copy,
        "name: macos-riff\n",
    ] {
        assert!(
            mac.contains(part),
            "{part} is not in the job macos_riff:\n{mac}"
        );
    }
    for cache in ["actions/cache", "rust-cache"] {
        assert!(!mac.contains(cache), "{mac}");
    }

    let linux = ci_job("mac_session_riff");
    let copy = format!(
        "cp target/static/release/riff dist/{}\n",
        asset("riff", MAC_SESSION)
    );
    for part in [
        "name: Linux riff of a Mac\n",
        "if: github.ref_type == 'tag'\n",
        "runs-on: ubuntu-24.04-arm\n",
        "continue-on-error: true\n",
        "run: test \"$(uname -sm)\" = \"Linux aarch64\"\n",
        "run: deploy/static/build.sh release\n",
        "if readelf -lW \"$file\" | grep -q 'Requesting program interpreter'; then",
        "\"$file\" --version\n",
        &copy,
        "name: mac-session-riff\n",
    ] {
        assert!(
            linux.contains(part),
            "{part} is not in the job mac_session_riff:\n{linux}"
        );
    }
    for cache in ["actions/cache", "rust-cache", "RIFF_STATIC_CARGO"] {
        assert!(!linux.contains(cache), "{linux}");
    }

    let publish = ci_job("macos_release");
    let mut assets: Vec<String> = riff::release::files(MAC)
        .into_iter()
        .map(|file| file.asset)
        .collect();
    assets.sort();
    let line = format!("sha256sum -- {} > {}\n", assets.join(" "), sums(MAC));
    for part in [
        "name: macOS riff release\n",
        "needs: [macos_riff, mac_session_riff]\n",
        "if: github.ref_type == 'tag'\n",
        "continue-on-error: true\n",
        "contents: write\n",
        "name: macos-riff\n",
        "name: mac-session-riff\n",
        &line,
        "gh release view \"$TAG\"",
        "for _ in $(seq 120); do",
        "sleep 30",
    ] {
        assert!(
            publish.contains(part),
            "{part} is not in the job macos_release:\n{publish}"
        );
    }
    // Each file of a Mac is a riff-* file, and its sums go after it.
    assert!(assets.iter().all(|a| a.starts_with("riff-")), "{assets:?}");
    let files = publish
        .find("gh release upload \"$TAG\" dist/riff-* --clobber")
        .unwrap_or_else(|| panic!("no upload of the files:\n{publish}"));
    let sums_up = format!("gh release upload \"$TAG\" dist/{} --clobber", sums(MAC));
    let sums_at = publish
        .find(&sums_up)
        .unwrap_or_else(|| panic!("no upload of the sums:\n{publish}"));
    assert!(files < sums_at, "{publish}");
}

/// 01M6Z8F7B3HVVNGQMX8GP1Z7Q1, 01M6Z8FKZRJZ4NY60PQAH9SMHV: Linux stays
/// the same. The jobs of Linux name no file and no job of a Mac. The
/// files of Linux and `SHA256SUMS` keep their names. Only macOS riff
/// release needs a job of a Mac, and each job of a Mac has
/// `continue-on-error`. So a failed build for the Mac stops no file, no
/// run and no deploy of Linux.
#[test]
fn the_release_of_a_mac_changes_no_job_of_linux() {
    let mac_jobs = ["macos_riff", "mac_session_riff", "macos_release"];
    for name in [
        "build",
        "static",
        "static_release",
        "release",
        "deploy",
        "gate",
    ] {
        let job = ci_job(name);
        for mac in [MAC, MAC_SESSION, "SHA256SUMS-"].iter().chain(&mac_jobs) {
            assert!(!job.contains(mac), "{mac} is in the job {name}:\n{job}");
        }
    }
    let job = ci_job("static");
    for part in [
        "cp \"target/static/release/$name\" \"dist/$name-x86_64-unknown-linux-musl\"\n",
        "sha256sum -- riff-x86_64-unknown-linux-musl riff-server-x86_64-unknown-linux-musl \
         > SHA256SUMS\n",
    ] {
        assert!(
            job.contains(part),
            "{part} is not in the job static:\n{job}"
        );
    }
    assert!(ci_job("static_release").contains("needs: [static]\n"));
    let deploy = ci_job("deploy");
    for part in [
        "needs: [gate, release, static_release]\n",
        "--pattern '*-x86_64-unknown-linux-musl' --pattern SHA256SUMS\n",
    ] {
        assert!(
            deploy.contains(part),
            "{part} is not in the job deploy:\n{deploy}"
        );
    }
    let workflow = read(".github/workflows/ci.yml");
    let needs: Vec<&str> = workflow
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("needs:"))
        .filter(|line| mac_jobs.iter().any(|mac| line.contains(mac)))
        .collect();
    assert_eq!(needs, ["needs: [macos_riff, mac_session_riff]"]);
    for name in mac_jobs {
        let job = ci_job(name);
        assert!(job.contains("\n    continue-on-error: true\n"), "{job}");
    }
}
