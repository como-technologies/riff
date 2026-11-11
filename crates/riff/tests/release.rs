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
/// 01M75TMWKT8E0YYEENWVX4B67A: the job Build builds the static riff of
/// each pull request and each push, and the job Static riff checks it. A
/// push, to main or of a tag, builds the profile release, each other run
/// the profile dev. The check compiles nothing.
#[test]
fn ci_builds_and_checks_the_static_riff_of_each_pull_request() {
    let plan = ci_job("plan");
    for part in [
        "name: Plan\n",
        "PROFILE: ${{ github.event_name == 'push' && 'release' || 'dev' }}\n",
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

/// 01M6Z8F7B3HVVNGQMX8GP1Z7Q1, 01M6Z8FKZRJZ4NY60PQAH9SMHV,
/// 01M75TMWKT8E0YYEENWVX4B67A: when the plan finds no files of a Mac for
/// the tree, the job macOS riff builds `riff` and `riff-server` on a Mac
/// with Apple silicon, and the job Linux riff of a Mac builds the static
/// Linux riff on arm64. For a tag, the job macOS riff release adds the
/// files that `riff update` on a Mac downloads to the release, with their
/// own file of sums, the sums last. A build of a release uses no cache.
#[test]
fn a_release_tag_gets_the_files_that_riff_update_on_a_mac_downloads() {
    let mac = ci_job("macos_riff");
    let copy = format!("cp \"target/release/$name\" \"dist/$name-{MAC}\"");
    for part in [
        "name: macOS riff\n",
        "needs: [plan]\n",
        "if: needs.plan.outputs.build_macos_riff == 'true'\n",
        "runs-on: macos-latest\n",
        "continue-on-error: true\n",
        "run: test \"$(uname -sm)\" = \"Darwin arm64\"\n",
        "run: cargo build --locked --profile release -p riff -p riff-server\n",
        "target/release/riff --version\n",
        "for name in riff riff-server; do",
        &copy,
        "name: macos-riff-${{ needs.plan.outputs.tree }}\n",
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
        "needs: [plan]\n",
        "if: needs.plan.outputs.build_mac_session_riff == 'true'\n",
        "runs-on: ubuntu-24.04-arm\n",
        "continue-on-error: true\n",
        "run: test \"$(uname -sm)\" = \"Linux aarch64\"\n",
        "run: deploy/static/build.sh release\n",
        "if readelf -lW \"$file\" | grep -q 'Requesting program interpreter'; then",
        "\"$file\" --version\n",
        &copy,
        "name: mac-session-riff-${{ needs.plan.outputs.tree }}\n",
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
        "needs: [plan, macos_riff, mac_session_riff]\n",
        "if: ${{ !cancelled() && needs.plan.result == 'success' && github.ref_type == 'tag' }}\n",
        "continue-on-error: true\n",
        "contents: write\n",
        "name: macos-riff-${{ needs.plan.outputs.tree }}\n",
        "run-id: ${{ needs.plan.outputs.macos_riff_run }}\n",
        "name: mac-session-riff-${{ needs.plan.outputs.tree }}\n",
        "run-id: ${{ needs.plan.outputs.mac_session_riff_run }}\n",
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
    assert_eq!(needs, ["needs: [plan, macos_riff, mac_session_riff]"]);
    for name in mac_jobs {
        let job = ci_job(name);
        assert!(job.contains("\n    continue-on-error: true\n"), "{job}");
    }
}

// ── deploy/static/smoke.sh: the release test of a tree
// (01M75TN3HCK6VBB2GRCJG1Y5B2).

/// Runs the release test on the binaries in `dir`, with `wait` seconds
/// for each wait.
fn smoke(dir: &Path, wait: &str) -> Output {
    let env = Isolated::new();
    env.command(top().join("deploy/static/smoke.sh"))
        .arg("dev")
        .env("RIFF_STATIC_DIR", dir)
        .env("RIFF_SMOKE_WAIT", wait)
        .output()
        .unwrap()
}

/// A folder with the `riff` under test, and the program `server` as
/// `riff-server`.
fn riff_and(server: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(Isolated::shared().riff_path(), dir.path().join("riff")).unwrap();
    isolated::write_program(dir.path().join("riff-server"), server);
    dir
}

/// The build that `riff --version` of the `riff` under test names.
fn build_of_riff() -> String {
    let out = Isolated::new().riff().arg("--version").output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    text(&out.stdout)
        .trim()
        .strip_prefix("riff ")
        .unwrap()
        .to_owned()
}

/// The smoke test runs the binaries of the build, with no cargo test:
/// each step passes, in order.
#[test]
fn the_release_test_runs_riff_and_riff_server_and_passes() {
    let env = Isolated::shared();
    let dir = env.riff_path().parent().unwrap().to_owned();
    assert_eq!(env.riff_server_path().parent(), Some(dir.as_path()));
    let out = smoke(&dir, "30");
    assert!(
        out.status.success(),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
    let stdout = text(&out.stdout);
    let steps = [
        format!(
            "riff and riff-server start, and have the build {}.",
            build_of_riff()
        ),
        "riff-server serves at http://127.0.0.1:".into(),
        "riff server finds the same build at http://127.0.0.1:".into(),
        "riff post put a message in the thread smoke, and riff read shows it.".into(),
        "riff-server stops on SIGTERM with the exit code 0.".into(),
        format!("The release test of {} passed.", dir.display()),
    ];
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), steps.len(), "{stdout}");
    for (line, step) in lines.iter().zip(&steps) {
        assert!(line.starts_with(step.as_str()), "{step} is not {line}");
    }
}

/// The smoke test fails, and names the binary, when a binary does not
/// start: here a riff with an interpreter that is not there, as a riff
/// that is not static has on a machine with another libc.
#[test]
fn the_release_test_fails_on_a_release_binary_that_does_not_start() {
    let dir = tempfile::tempdir().unwrap();
    isolated::write_program(dir.path().join("riff"), "#!/nonexistent/ld-linux\n");
    std::os::unix::fs::symlink(
        Isolated::shared().riff_server_path(),
        dir.path().join("riff-server"),
    )
    .unwrap();
    let out = smoke(dir.path(), "30");
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stdout));
    assert!(
        text(&out.stderr).contains("smoke.sh: riff does not start: riff --version failed:"),
        "{}",
        text(&out.stderr)
    );
    assert!(text(&out.stdout).is_empty(), "{}", text(&out.stdout));

    // A riff-server that ends at its start: the error has its log.
    let build = build_of_riff();
    let dir = riff_and(&format!(
        "#!/bin/sh\n[ \"$1\" = --version ] && echo 'riff-server {build}' && exit 0\n\
         echo 'riff-server: the store is broken' >&2\nexit 101\n"
    ));
    let out = smoke(dir.path(), "30");
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("smoke.sh: riff-server does not start: it ended with the exit code 101."),
        "{stderr}"
    );
    assert!(
        stderr.contains("riff-server: the store is broken"),
        "{stderr}"
    );
}

/// A riff-server that never serves fails after the wait, and a riff and
/// a riff-server of two builds fail at once.
#[test]
fn the_release_test_fails_on_a_server_that_does_not_serve_and_on_two_builds() {
    let build = build_of_riff();
    let dir = riff_and(&format!(
        "#!/bin/sh\n[ \"$1\" = --version ] && echo 'riff-server {build}' && exit 0\n\
         exec sleep 30\n"
    ));
    let out = smoke(dir.path(), "1");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("smoke.sh: riff-server does not serve after 1 seconds."),
        "{}",
        text(&out.stderr)
    );

    let dir = riff_and("#!/bin/sh\necho 'riff-server of another build'\n");
    let out = smoke(dir.path(), "30");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains(&format!(
            "smoke.sh: riff --version printed \"riff {build}\", and riff-server --version \
             \"riff-server of another build\": not one build."
        )),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn the_release_test_refuses_another_profile_and_a_missing_binary() {
    let env = Isolated::new();
    let out = env
        .command(top().join("deploy/static/smoke.sh"))
        .arg("fast")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("\"fast\" is not a profile. Give release or dev."),
        "{}",
        text(&out.stderr)
    );

    let dir = tempfile::tempdir().unwrap();
    let out = smoke(dir.path(), "30");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains(&format!(
            "smoke.sh: {}/riff is missing, or it cannot run. Build it: deploy/static/build.sh dev",
            dir.path().canonicalize().unwrap().display()
        )),
        "{}",
        text(&out.stderr)
    );
}

/// 01M75TN3HCK6VBB2GRCJG1Y5B2, 01M75TN3M9JEVZQXEA2E8FTBKS: the job Static
/// riff runs the smoke test after the check, on the binaries of the
/// profile of the run. For a push, it sets the status of the release
/// test from the outcome of the smoke test, also when a step before it
/// failed, but only for the binaries of the profile release.
#[test]
fn ci_runs_the_release_test_and_sets_its_status_for_a_push() {
    let job = ci_job("static");
    let check = job
        .find("run: deploy/static/check.sh \"$PROFILE\"\n")
        .unwrap_or_else(|| panic!("no check:\n{job}"));
    let smoke = job
        .find("      - name: Smoke test\n        id: smoke\n        run: deploy/static/smoke.sh \"$PROFILE\"\n")
        .unwrap_or_else(|| panic!("no smoke test:\n{job}"));
    let status = job
        .find("      - name: Set the status of the release test\n")
        .unwrap_or_else(|| panic!("no status:\n{job}"));
    assert!(check < smoke && smoke < status, "{job}");
    let step = job[status..].split_once("\n\n").unwrap().0;
    for part in [
        "if: ${{ !cancelled() && github.event_name == 'push' && \
         needs.plan.outputs.static_profile == 'release' }}\n",
        "GH_TOKEN: ${{ github.token }}\n",
        "GH_REPO: ${{ github.repository }}\n",
        "OUTCOME: ${{ steps.smoke.outcome }}\n",
        "RUN_URL: ${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}\n",
        "run: deploy/release-test-status.sh \"$OUTCOME\" \"$GITHUB_SHA\" \"$RUN_URL\"",
    ] {
        assert!(step.contains(part), "{part} is not in:\n{step}");
    }
    assert!(
        job.contains(
            "\n    permissions:\n      contents: read\n      actions: read\n      statuses: write\n"
        ),
        "{job}"
    );
    // The release test runs the binaries: no cargo test, no build.
    for build in ["cargo", "build.sh"] {
        assert!(!job.contains(build), "{build} is in:\n{job}");
    }
}

/// Runs `deploy/release-test-status.sh` with `args` and a fake `gh` that
/// logs its arguments, one on a line, to `gh.log` in `bin`.
fn status(bin: &Path, args: &[&str]) -> Output {
    isolated::write_program(
        bin.join("gh"),
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done >> {}\n",
            bin.join("gh.log").display()
        ),
    );
    let env = Isolated::new();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    env.command(top().join("deploy/release-test-status.sh"))
        .args(args)
        .env("PATH", path)
        .env("GH_REPO", "o/r")
        .output()
        .unwrap()
}

/// 01M75TN3M9JEVZQXEA2E8FTBKS: the status `riff/release-test` of the
/// commit is success when the smoke test passed, and failure for each
/// other outcome. Its target is the run.
#[test]
fn the_status_of_the_release_test_follows_the_outcome_of_the_smoke_test() {
    let run = "https://github.com/o/r/actions/runs/9";
    for (outcome, state, description) in [
        (
            "success",
            "success",
            "The check and the smoke test of the release binaries passed.",
        ),
        (
            "failure",
            "failure",
            "The check or the smoke test of the release binaries did not pass: failure.",
        ),
        (
            "skipped",
            "failure",
            "The check or the smoke test of the release binaries did not pass: skipped.",
        ),
    ] {
        let bin = tempfile::tempdir().unwrap();
        let out = status(bin.path(), &[outcome, "c0ffee", run]);
        assert!(out.status.success(), "{}", text(&out.stderr));
        assert_eq!(
            text(&out.stdout),
            format!("The status riff/release-test of c0ffee is {state}: {description}\n")
        );
        let log = fs::read_to_string(bin.path().join("gh.log")).unwrap();
        let state = format!("state={state}");
        let description = format!("description={description}");
        let target = format!("target_url={run}");
        let want = [
            "api",
            "-X",
            "POST",
            "repos/o/r/statuses/c0ffee",
            "-f",
            &state,
            "-f",
            "context=riff/release-test",
            "-f",
            &description,
            "-f",
            &target,
        ];
        assert_eq!(log.lines().collect::<Vec<_>>(), want, "{outcome}");
    }

    let bin = tempfile::tempdir().unwrap();
    let out = status(bin.path(), &["maybe", "c0ffee", run]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("\"maybe\" is not an outcome of a step."),
        "{}",
        text(&out.stderr)
    );
    let out = status(bin.path(), &["success", "c0ffee"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!bin.path().join("gh.log").exists());
}
