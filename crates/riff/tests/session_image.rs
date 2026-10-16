//! The session image of this repository: `riff.toml` pins it by digest
//! (01M4Z7SP3KSKJYKNRP1MQ70C4Q), the workflow "Session image" builds it
//! from `deploy/session/` (01M4Z7SP7Y19BQ8QTV1T6AE8SR), its Dockerfile
//! has the toolchain of `rust-toolchain.toml`
//! (01M4Z7SP5TGJ3DMWFGE5SN68N1), `riff image` prints it
//! (01M4Z7SPC20RWG218EWPEGYZTF), the tests of `crates/riff` run in it
//! with `deploy/session-test.sh` (01M51117NDKF11FNZ915TTQED4), and the
//! jobs "Tests I/N" of the Gate run each test of the workspace in it
//! (01M515Z3Y7GSFNTJGXJD727MQK).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use isolated::Isolated;
use riff::session_image::{FILE, Image};

use crate::release::{ci_job, fake_engine};

/// The root of this repository.
fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The text of the file `path` of this repository.
fn read(path: &str) -> String {
    fs::read_to_string(top().join(path)).unwrap()
}

#[test]
fn this_repository_pins_its_session_image_by_digest() {
    let image = Image::of_repository(&top()).unwrap();
    assert_eq!(image.name(), "ghcr.io/como-technologies/riff-session");
    assert_ne!(
        image.digest(),
        format!("sha256:{}", "0".repeat(64)),
        "{FILE} pins the placeholder, not the digest of a build"
    );
}

#[test]
fn the_workflow_builds_the_image_that_riff_toml_names() {
    let workflow = read(".github/workflows/session-image.yml");
    let image = Image::of_repository(&top()).unwrap();
    assert!(
        workflow.contains(&format!("IMAGE: {}\n", image.name())),
        "{workflow}"
    );
    for path in [
        "deploy/session/**",
        "riff.toml",
        ".github/workflows/session-image.yml",
    ] {
        assert_eq!(
            workflow.matches(&format!("- {path}\n")).count(),
            2,
            "{path}"
        );
    }
    assert!(
        workflow.contains("git rev-parse HEAD:deploy/session"),
        "{workflow}"
    );
}

#[test]
fn the_session_image_has_the_toolchain_of_the_repository() {
    let toolchain: toml_edit::DocumentMut = read("rust-toolchain.toml").parse().unwrap();
    let channel = toolchain["toolchain"]["channel"].as_str().unwrap();
    let components: Vec<&str> = toolchain["toolchain"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    let dockerfile = read("deploy/session/Dockerfile");
    let arg = format!("ARG RUST_CHANNEL={channel}");
    assert!(
        dockerfile.lines().any(|line| line == arg),
        "deploy/session/Dockerfile has no line {arg:?}: change it with rust-toolchain.toml"
    );
    let installed: Vec<&str> = dockerfile
        .split("--component ")
        .skip(1)
        .map(|rest| rest.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(
        installed, components,
        "deploy/session/Dockerfile installs other components than rust-toolchain.toml"
    );
}

/// A new git repository with the riff config `config`, if any.
fn repository(config: Option<&str>) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let status = Command::new("git")
        .args(["init", "-q"])
        .arg(root.path())
        .status()
        .unwrap();
    assert!(status.success());
    if let Some(config) = config {
        fs::write(root.path().join(FILE), config).unwrap();
    }
    root
}

/// `riff image` in the folder `dir`.
fn riff_image(dir: &Path) -> Output {
    Isolated::shared()
        .riff()
        .arg("image")
        .current_dir(dir)
        .output()
        .unwrap()
}

#[test]
fn riff_image_prints_the_pinned_image() {
    let text = format!("ghcr.io/o/i@sha256:{}", "ab".repeat(32));
    let root = repository(Some(&format!("[session]\nimage = \"{text}\"\n")));
    let sub = root.path().join("a/b");
    fs::create_dir_all(&sub).unwrap();
    for dir in [root.path(), &sub] {
        let out = riff_image(dir);
        assert!(out.status.success(), "{out:?}");
        assert_eq!(String::from_utf8(out.stdout).unwrap(), format!("{text}\n"));
    }
}

#[test]
fn riff_image_refuses_a_tag() {
    let root = repository(Some("[session]\nimage = \"ghcr.io/o/i:latest\"\n"));
    let out = riff_image(root.path());
    assert!(!out.status.success(), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    let error = String::from_utf8(out.stderr).unwrap();
    assert!(error.contains("names a tag"), "{error}");

    let tagged = format!("ghcr.io/o/i:v1@sha256:{}", "ab".repeat(32));
    for (image, why) in [
        (tagged.as_str(), "names a tag"),
        ("ghcr.io/o/i", "has no digest"),
    ] {
        fs::write(
            root.path().join(FILE),
            format!("[session]\nimage = \"{image}\"\n"),
        )
        .unwrap();
        let out = riff_image(root.path());
        assert!(!out.status.success(), "{out:?}");
        let error = String::from_utf8(out.stderr).unwrap();
        assert!(error.contains(why), "{image}: {error}");
    }
}

#[test]
fn riff_image_names_a_missing_config() {
    let root = repository(None);
    let out = riff_image(root.path());
    assert!(!out.status.success(), "{out:?}");
    let error = String::from_utf8(out.stderr).unwrap();
    assert!(error.contains("cannot read the riff config"), "{error}");
    assert!(error.contains(FILE), "{error}");
}

/// Runs `deploy/session-test.sh` with `args`, the fake engines of `bin`
/// first in `PATH`, and the engine `engine` when it is set.
fn session_test_output(args: &[&str], bin: &Path, engine: Option<&str>) -> Output {
    let env = Isolated::new();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let mut cmd = env.command(top().join("deploy/session-test.sh"));
    cmd.args(args).env("PATH", path).env_remove("RIFF_ENGINE");
    if let Some(engine) = engine {
        cmd.env("RIFF_ENGINE", engine);
    }
    cmd.output().unwrap()
}

/// [`session_test_output`] that passes. The engine logs its arguments
/// to `bin/podman.log`, one on each line.
fn session_test(args: &[&str], bin: &Path, engine: Option<&str>) -> Vec<String> {
    let out = session_test_output(args, bin, engine);
    assert!(out.status.success(), "{out:?}");
    let log = fs::read_to_string(bin.join("podman.log")).unwrap();
    log.lines().map(str::to_owned).collect()
}

/// The command that the container runs: the arguments after `bash`.
fn command_of(args: &[String]) -> Vec<String> {
    let at = args.iter().position(|a| a == "bash").unwrap();
    assert_eq!(args[at + 1], "-c", "{args:?}");
    let shell = args[at + 3..].iter().position(|a| a == "bash").unwrap();
    args[at + 3 + shell + 1..].to_vec()
}

/// 01M51117NDKF11FNZ915TTQED4, 01M51117K3WXK35CE67KPRFMA8: the tests of
/// `crates/riff` run in the image that `riff.toml` pins, with `--init`,
/// as the user of the host, with a home of their own, with the limits of
/// a session, and with the values of `just cargo-test`. The arguments go
/// to `cargo test`.
#[test]
fn the_session_tests_run_in_the_pinned_image_with_an_init() {
    let bin = tempfile::tempdir().unwrap();
    fake_engine(bin.path());
    let args = session_test(&["run::riff_lead_start"], bin.path(), None);
    let has = |pair: [&str; 2]| args.windows(2).any(|w| w == pair);
    let root = top().canonicalize().unwrap().display().to_string();
    assert_eq!(
        args[..4],
        ["run", "--rm", "--init", "--userns=keep-id"],
        "{args:?}"
    );
    assert!(has(["-v", &format!("{root}:{root}")]), "{args:?}");
    assert!(has(["-w", &root]), "{args:?}");
    assert!(has(["--cpus", "4"]), "{args:?}");
    assert!(has(["--memory", "8g"]), "{args:?}");
    assert!(has(["-e", "HOME=/tmp/home"]), "{args:?}");
    let id = Command::new("id").arg("-un").output().unwrap();
    let user = String::from_utf8(id.stdout).unwrap().trim().to_owned();
    assert!(has(["-e", &format!("USER={user}")]), "{args:?}");
    assert!(has(["-e", &format!("LOGNAME={user}")]), "{args:?}");
    let target = format!("CARGO_TARGET_DIR={root}/target/session");
    assert!(has(["-e", &target]), "{args:?}");
    assert!(has(["-e", "CARGO_INCREMENTAL"]), "{args:?}");
    let image = Image::of_repository(&top()).unwrap().to_string();
    assert!(args.contains(&image), "{args:?}");
    let script = "mkdir -p \"$HOME\" && exec \"$@\"";
    assert!(has(["-c", script]), "{args:?}");
    assert_eq!(
        command_of(&args),
        ["just", "cargo-test", "-p", "riff", "run::riff_lead_start"],
        "{args:?}"
    );
}

/// 01M515Z3Y7GSFNTJGXJD727MQK: with `--part I/N`, the container runs
/// part I of N of the tests of the workspace, in place of the tests of
/// `crates/riff`. A part needs one value.
#[test]
fn the_session_tests_run_one_part_of_the_tests_of_the_workspace() {
    let bin = tempfile::tempdir().unwrap();
    fake_engine(bin.path());
    let args = session_test(&["--part", "2/4"], bin.path(), None);
    assert_eq!(args[..3], ["run", "--rm", "--init"], "{args:?}");
    assert_eq!(command_of(&args), ["just", "test-part", "2/4"], "{args:?}");

    for bad in [&["--part"][..], &["--part", "1/4", "2/4"]] {
        let bin = tempfile::tempdir().unwrap();
        fake_engine(bin.path());
        let out = session_test_output(bad, bin.path(), None);
        assert_eq!(out.status.code(), Some(2), "{bad:?}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("give one part, for example --part 2/4"),
            "{out:?}"
        );
        assert!(!bin.path().join("podman.log").exists(), "{bad:?}");
    }
}

/// 01M51117NDKF11FNZ915TTQED4: docker has no `--userns=keep-id`, so the
/// tests run there as the user and the group of the host.
#[test]
fn the_session_tests_run_as_the_user_of_the_host_in_docker() {
    let bin = tempfile::tempdir().unwrap();
    fake_engine(bin.path());
    fs::copy(bin.path().join("podman"), bin.path().join("docker")).unwrap();
    let args = session_test(&[], bin.path(), Some("docker"));
    let id = |flag: &str| {
        let out = Command::new("id").arg(flag).output().unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };
    let user = format!("{}:{}", id("-u"), id("-g"));
    assert_eq!(
        args[..5],
        ["run", "--rm", "--init", "--user", &user],
        "{args:?}"
    );
    assert!(!args.iter().any(|a| a.starts_with("--userns")), "{args:?}");
    assert_eq!(command_of(&args), ["just", "cargo-test", "-p", "riff"]);
}

/// 01M515Z3Y7GSFNTJGXJD727MQK: the jobs "Tests I/N" of the Gate run each
/// part of the tests in the session image, with docker and a cache of
/// `target/session`. No other job runs the session tests.
#[test]
fn the_gate_runs_the_tests_in_the_session_image() {
    let job = ci_job("test");
    for part in [
        "RIFF_ENGINE: docker\n",
        "CARGO_INCREMENTAL: 0\n",
        "uses: actions/cache@",
        "path: target/session\n",
        "key: session-${{ hashFiles('riff.toml', 'rust-toolchain.toml', 'Cargo.lock') }}\n",
        "run: deploy/session-test.sh --part ${{ matrix.part }}\n",
    ] {
        assert!(job.contains(part), "{part} is not in the job test:\n{job}");
    }
    let workflow = read(".github/workflows/ci.yml");
    assert_eq!(workflow.matches("deploy/session-test.sh").count(), 1);
    assert!(!workflow.contains("\n  session_tests:\n"), "{workflow}");
    let justfile = read("justfile");
    assert!(
        justfile.contains("\nsession-test *ARGS:\n    deploy/session-test.sh {{ARGS}}\n"),
        "{justfile}"
    );
}
