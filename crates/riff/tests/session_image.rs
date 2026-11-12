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
        "deploy/base/**",
        "deploy/rust/**",
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
    // 01M5SNTDZ95BRDCPQEKDTYD2PY: the three images, each after the one before.
    for (job, needs, folder) in [
        ("base", None, "deploy/base"),
        ("rust", Some("base"), "deploy/rust"),
        ("session", Some("rust"), "deploy/session"),
    ] {
        let text = workflow_job(&workflow, job);
        assert!(
            text.contains(&format!("folder: {folder}\n")),
            "{job}:\n{text}"
        );
        match needs {
            Some(needs) => assert!(
                text.contains(&format!("needs: [{needs}]\n")),
                "{job}:\n{text}"
            ),
            None => assert!(!text.contains("needs:"), "{job}:\n{text}"),
        }
    }
    // The tag is the git tree of the folder (01M4Z7SP7Y19BQ8QTV1T6AE8SR).
    let action = read(".github/actions/image/action.yml");
    assert!(
        action.contains("git rev-parse \"HEAD:$FOLDER\""),
        "{action}"
    );
}

/// The text of the job `name` of the workflow `workflow`.
fn workflow_job(workflow: &str, name: &str) -> String {
    let jobs = workflow.split_once("\njobs:").unwrap().1;
    let job = jobs.split_once(&format!("\n  {name}:\n")).unwrap().1;
    let end = job
        .match_indices("\n  ")
        .find(|(at, _)| job[at + 3..].starts_with(|c: char| c.is_ascii_alphabetic()))
        .map_or(job.len(), |(at, _)| at);
    job[..end].to_owned()
}

/// The components that the `--component` flags of the Dockerfile at
/// `path` install.
fn installed(path: &str) -> Vec<String> {
    let text = read(path);
    text.split("--component ")
        .skip(1)
        .chain(text.split("rustup component add ").skip(1))
        .map(|rest| rest.split_whitespace().next().unwrap().to_owned())
        .collect()
}

/// 01M5SNTDW9WVEXZW5D0TKGBA8Z: the channel of `deploy/rust/` is the
/// channel of `rust-toolchain.toml`, and the components of `deploy/rust/`
/// and `deploy/session/` together are its components.
#[test]
fn the_images_have_the_toolchain_of_the_repository() {
    let toolchain: toml_edit::DocumentMut = read("rust-toolchain.toml").parse().unwrap();
    let channel = toolchain["toolchain"]["channel"].as_str().unwrap();
    let mut components: Vec<String> = toolchain["toolchain"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    components.sort();
    let arg = format!("ARG RUST_CHANNEL={channel}");
    assert!(
        read("deploy/rust/Dockerfile")
            .lines()
            .any(|line| line == arg),
        "deploy/rust/Dockerfile has no line {arg:?}: change it with rust-toolchain.toml"
    );
    let mut both = installed("deploy/rust/Dockerfile");
    both.extend(installed("deploy/session/Dockerfile"));
    both.sort();
    both.dedup();
    assert_eq!(
        both, components,
        "deploy/rust/ and deploy/session/ install other components than rust-toolchain.toml"
    );
}

/// 01M5SNT57ANM99NQS5SNB8XPWV: `riff-rust` builds `FROM` a digest of
/// `riff-base`, and it is the base that `riff project init` pins.
#[test]
fn the_rust_image_builds_from_a_digest_of_the_base_image() {
    let base = riff::session_image::base();
    assert_eq!(base.name(), "ghcr.io/como-technologies/riff-base");
    assert_ne!(
        base.digest(),
        format!("sha256:{}", "0".repeat(64)),
        "deploy/rust/Dockerfile pins the placeholder, not the digest of a build"
    );
    let from = read("deploy/rust/Dockerfile")
        .lines()
        .filter(|line| line.starts_with("FROM "))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(from, [format!("FROM {base}")]);
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

/// 01M6W7VC9REZKE87T6YT5Z47A2: `riff image` refuses `ANTHROPIC_MODEL`
/// in `[session.env]`, and the error names the key and the file.
#[test]
fn riff_image_refuses_anthropic_model_in_the_env() {
    let image = format!("ghcr.io/o/i@sha256:{}", "ab".repeat(32));
    let root = repository(Some(&format!(
        "[session]\nimage = \"{image}\"\n\n[session.env]\nANTHROPIC_MODEL = \"sonnet\"\n"
    )));
    let out = riff_image(root.path());
    assert!(!out.status.success(), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    let error = String::from_utf8(out.stderr).unwrap();
    assert!(
        error.contains(&format!(
            "{FILE}: [session.env] ANTHROPIC_MODEL: riff sets the model"
        )),
        "{error}"
    );
    assert!(
        error.contains("Put model = \"sonnet\" in [session]"),
        "{error}"
    );

    fs::write(
        root.path().join(FILE),
        format!("[session]\nimage = \"{image}\"\nmodel = \"sonnet\"\n"),
    )
    .unwrap();
    let out = riff_image(root.path());
    assert!(out.status.success(), "{out:?}");
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

/// `riff image --check` in the folder `dir`.
async fn riff_image_check(dir: &Path) -> Output {
    let mut cmd = Isolated::shared().riff();
    cmd.args(["image", "--check"]).current_dir(dir);
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

/// 01M5SMSB0NFBD1XHEW3BSE4BDP: `riff image --check` asks the registry of
/// the image with no login. A package that a host can pull gives one
/// line and the exit 0.
#[tokio::test]
async fn riff_image_check_says_that_a_host_can_pull_a_public_image() {
    let (host, seen) = crate::common::registry(200, 200).await;
    let image = format!("{host}/o/i@sha256:{}", "ab".repeat(32));
    let root = repository(Some(&format!("[session]\nimage = \"{image}\"\n")));
    let out = riff_image_check(root.path()).await;
    assert!(out.status.success(), "{out:?}");
    let parsed: Image = image.parse().unwrap();
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("{}\n", riff::text::image_open(&parsed))
    );
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 3);

    // Without --check, riff asks nothing.
    let out = riff_image(root.path());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), format!("{image}\n"));
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 3);
}

/// A package that is private, or not there: the exit 1, and the fix.
#[tokio::test]
async fn riff_image_check_gives_the_fix_for_an_image_that_a_host_cannot_pull() {
    for status in [401_u16, 403, 404] {
        let (host, _) = crate::common::registry(status, 200).await;
        let image = format!("{host}/o/i@sha256:{}", "ab".repeat(32));
        let root = repository(Some(&format!("[session]\nimage = \"{image}\"\n")));
        let out = riff_image_check(root.path()).await;
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        assert!(out.stdout.is_empty(), "{out:?}");
        let parsed: Image = image.parse().unwrap();
        let why = format!(
            "the registry answered {}",
            reqwest::StatusCode::from_u16(status).unwrap()
        );
        assert_eq!(
            String::from_utf8(out.stderr).unwrap(),
            format!("{}\n", riff::text::image_closed(&parsed, &why))
        );
    }
}

/// A registry that nobody listens at gives no answer: the exit 1 and a
/// line that says so.
#[tokio::test]
async fn riff_image_check_says_when_it_cannot_ask_the_registry() {
    let image = format!("127.0.0.1:9/o/i@sha256:{}", "ab".repeat(32));
    let root = repository(Some(&format!("[session]\nimage = \"{image}\"\n")));
    let out = riff_image_check(root.path()).await;
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let error = String::from_utf8(out.stderr).unwrap();
    assert!(
        error.starts_with("riff cannot ask 127.0.0.1:9 about "),
        "{error}"
    );
    assert!(error.trim_end().ends_with("Try again."), "{error}");
}

/// Runs `deploy/session-test.sh` with `args`, the fake engines of `bin`
/// first in `PATH`, and the engine `engine` when it is set.
fn session_test_output(args: &[&str], bin: &Path, engine: Option<&str>) -> Output {
    session_test_in(&Isolated::new(), args, bin, engine)
}

/// [`session_test_output`] in the environment `env`.
fn session_test_in(env: &Isolated, args: &[&str], bin: &Path, engine: Option<&str>) -> Output {
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
    logged_after_the_pull(bin)
}

/// The arguments that the fake engine of `bin` logged in `podman.log`,
/// one on each line, after the pull of the image: the pull comes first
/// (01M6QPKG1X8VZTDF8RTTKFW67C).
fn logged_after_the_pull(bin: &Path) -> Vec<String> {
    let log = fs::read_to_string(bin.join("podman.log")).unwrap();
    let lines: Vec<String> = log.lines().map(str::to_owned).collect();
    let image = Image::of_repository(&top()).unwrap().to_string();
    assert_eq!(lines[..2], ["pull", image.as_str()], "{lines:?}");
    lines[2..].to_vec()
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
    // The checkout is at the same path on each machine
    // (01M75NDR15DNBFHDBNKA3CA1BE).
    let at = "/var/tmp/riff-ci/riff";
    assert!(has(["-v", &format!("{root}:{at}")]), "{args:?}");
    assert!(has(["-w", at]), "{args:?}");
    assert!(has(["--cpus", "4"]), "{args:?}");
    assert!(has(["--memory", "8g"]), "{args:?}");
    assert!(has(["-e", "HOME=/tmp/home"]), "{args:?}");
    let id = Command::new("id").arg("-un").output().unwrap();
    let user = String::from_utf8(id.stdout).unwrap().trim().to_owned();
    assert!(has(["-e", &format!("USER={user}")]), "{args:?}");
    assert!(has(["-e", &format!("LOGNAME={user}")]), "{args:?}");
    let target = format!("CARGO_TARGET_DIR={at}/target/session");
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

/// A fake engine `name` in `bin` whose first `fails` pulls fail with the
/// exit 125 of the engine, and a fake `sleep`. Both log one line for each
/// call to `bin/engine.log`: its arguments, and `sleep 10` for a wait.
fn flaky_engine(bin: &Path, name: &str, fails: u32) {
    let log = bin.join("engine.log");
    let count = bin.join("pulls");
    isolated::write_program(
        bin.join(name),
        format!(
            "#!/bin/sh\necho \"$*\" >> {log}\n\
             case \"$1\" in\n\
             pull) n=$(cat {count} 2>/dev/null || echo 0); echo $((n + 1)) > {count}\n\
             if [ $((n + 1)) -le {fails} ]; then\n\
             echo 'failed to copy: httpReadSeeker: failed open' >&2; exit 125; fi ;;\n\
             esac\n",
            log = log.display(),
            count = count.display(),
        ),
    );
    isolated::write_program(
        bin.join("sleep"),
        format!("#!/bin/sh\necho \"sleep $*\" >> {}\n", log.display()),
    );
}

/// The lines that [`flaky_engine`] logged to `bin/engine.log`.
fn engine_calls(bin: &Path) -> Vec<String> {
    let log = fs::read_to_string(bin.join("engine.log")).unwrap();
    log.lines().map(str::to_owned).collect()
}

/// 01M6QPKG1X8VZTDF8RTTKFW67C (#1634): a pull of the image that fails
/// once fails no test. The script pulls the image first, waits 10
/// seconds after a failed pull, and pulls again. The tests run after the
/// pull that works.
#[test]
fn the_session_tests_pull_the_image_again_after_a_pull_that_failed() {
    let image = Image::of_repository(&top()).unwrap().to_string();
    let pull = format!("pull {image}");
    for (name, runs) in [("podman", "run --rm --init"), ("docker", "run --rm")] {
        let bin = tempfile::tempdir().unwrap();
        flaky_engine(bin.path(), name, 1);
        let env = Isolated::new();
        let out = session_test_in(&env, &["--part", "2/4"], bin.path(), Some(name));
        assert!(out.status.success(), "{name}: {out:?}");
        let calls = engine_calls(bin.path());
        assert_eq!(
            calls[..3],
            [pull.as_str(), "sleep 10", pull.as_str()],
            "{calls:?}"
        );
        assert!(calls[3].starts_with(&format!("{runs} ")), "{calls:?}");
        assert_eq!(calls.iter().filter(|c| **c == pull).count(), 2, "{calls:?}");
        assert!(
            calls.last().unwrap().ends_with("just test-part 2/4"),
            "{calls:?}"
        );
        let error = String::from_utf8(out.stderr).unwrap();
        assert!(
            error.contains(&format!("the pull of the image {image} failed")),
            "{error}"
        );
    }
}

/// 01M6QPKG1X8VZTDF8RTTKFW67C: a pull that works the first time waits
/// for nothing.
#[test]
fn the_session_tests_wait_for_no_pull_that_works() {
    let bin = tempfile::tempdir().unwrap();
    flaky_engine(bin.path(), "podman", 0);
    let out = session_test_output(&["--part", "1/4"], bin.path(), Some("podman"));
    assert!(out.status.success(), "{out:?}");
    let calls = engine_calls(bin.path());
    assert!(calls[0].starts_with("pull "), "{calls:?}");
    assert!(calls.iter().all(|c| !c.starts_with("sleep")), "{calls:?}");
    assert_eq!(calls.len(), 2, "{calls:?}");
}

/// 01M6QPKG1X8VZTDF8RTTKFW67C: after 3 failed pulls, the script says that
/// the pull of the image failed, names the image, exits with 1, and runs
/// no test. It waits between the pulls only: twice.
#[test]
fn the_session_tests_stop_after_3_pulls_that_failed() {
    let image = Image::of_repository(&top()).unwrap().to_string();
    let pull = format!("pull {image}");
    let bin = tempfile::tempdir().unwrap();
    flaky_engine(bin.path(), "podman", 3);
    let out = session_test_output(&[], bin.path(), Some("podman"));
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let error = String::from_utf8(out.stderr).unwrap();
    assert!(
        error.contains(&format!("the pull of the image {image} failed 3 times.")),
        "{error}"
    );
    let calls = engine_calls(bin.path());
    let pull = pull.as_str();
    assert_eq!(calls, [pull, "sleep 10", pull, "sleep 10", pull]);
}

/// 01M6QPKG1X8VZTDF8RTTKFW67C, 01M6QPKG4MSK9AF70V6SZHSBKZ: "How the Gate
/// runs" says that the tests pull the session image up to 3 times, and
/// has the `sh` block that runs the failed jobs of a run again.
#[test]
fn the_book_says_how_the_gate_pulls_the_image_and_runs_failed_jobs_again() {
    let book = read("docs/src/development.md");
    let gate = book.split_once("\n## How the Gate runs\n").unwrap().1;
    let gate = gate.split_once("\n## ").unwrap().0;
    assert!(gate.contains("pulls again, up to 3 times"), "{gate}");
    assert!(
        gate.contains("```sh\ngh run rerun RUN --failed\n```\n"),
        "{gate}"
    );
}

/// 01M6QQ1NXY0CXMZEEEP413W4PK, 01M6QQ1P5YCT0JX90X5FCKM4W1: with `--build`,
/// the container runs `just ci-build`, the checks and the test build of
/// the job Build. With `--archive I/N`, it runs part I of N from the
/// archive of that build. Each has the container of the tests, and each
/// takes its own values.
#[test]
fn the_session_tests_build_once_and_run_a_part_from_the_archive() {
    let bin = tempfile::tempdir().unwrap();
    fake_engine(bin.path());
    let args = session_test(&["--build"], bin.path(), None);
    assert_eq!(args[..3], ["run", "--rm", "--init"], "{args:?}");
    assert_eq!(command_of(&args), ["just", "ci-build"], "{args:?}");
    assert!(args.windows(2).any(|w| w == ["-e", "CARGO_INCREMENTAL"]));

    let bin = tempfile::tempdir().unwrap();
    fake_engine(bin.path());
    let args = session_test(&["--archive", "3/4"], bin.path(), None);
    assert_eq!(args[..3], ["run", "--rm", "--init"], "{args:?}");
    assert_eq!(
        command_of(&args),
        ["just", "test-archived", "3/4"],
        "{args:?}"
    );

    for (bad, said) in [
        (
            &["--archive"][..],
            "give one part, for example --archive 2/4",
        ),
        (
            &["--archive", "1/4", "2/4"],
            "give one part, for example --archive 2/4",
        ),
        (&["--build", "1/4"], "--build takes no argument"),
    ] {
        let bin = tempfile::tempdir().unwrap();
        fake_engine(bin.path());
        let out = session_test_output(bad, bin.path(), None);
        assert_eq!(out.status.code(), Some(2), "{bad:?}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(said),
            "{out:?}"
        );
        assert!(!bin.path().join("podman.log").exists(), "{bad:?}");
    }
}

/// 01M51117NDKF11FNZ915TTQED4: docker has no `--userns=keep-id`, so the
/// tests run there as the user and the group of the host. The container
/// gets the passwd and the group file of the image, in a new folder of
/// `TMPDIR`, with an entry for the user and the group, as keep-id of
/// podman gives one.
#[test]
fn the_session_tests_run_as_the_user_of_the_host_in_docker() {
    let (uid, gid) = (host_id("-u"), host_id("-g"));
    let image = Image::of_repository(&top()).unwrap().to_string();
    let bin = tempfile::tempdir().unwrap();
    fake_engine(bin.path());
    isolated::copy_program(bin.path().join("podman"), bin.path().join("docker"));
    let env = Isolated::new();
    let args = in_docker(&env, bin.path());
    let copy = |file: &str| ["run", "--rm", image.as_str(), "cat", file].map(String::from);
    assert_eq!(args[..5], copy("/etc/passwd"), "{args:?}");
    assert_eq!(args[5..10], copy("/etc/group"), "{args:?}");
    let run = &args[10..];
    let user = format!("{uid}:{gid}");
    assert_eq!(
        run[..5],
        ["run", "--rm", "--init", "--user", &user],
        "{args:?}"
    );
    assert!(!run.iter().any(|a| a.starts_with("--userns")), "{args:?}");
    assert_eq!(command_of(run), ["just", "cargo-test", "-p", "riff"]);
    // The fake image has no entry for the user: the run adds one.
    for file in ["passwd", "group"] {
        let source = bind_source(run, file);
        assert!(source.starts_with(env.path().join("tmp")), "{source:?}");
    }
    let passwd = format!("{}:x:{uid}:{gid}::/tmp/home:/bin/bash\n", host_id("-un"));
    assert_eq!(bound(run, "passwd"), passwd);
    assert_eq!(
        bound(run, "group"),
        format!("{}:x:{gid}:\n", host_id("-gn"))
    );
}

/// 01M51117NDKF11FNZ915TTQED4: with docker, an image with an entry for
/// the UID and the GID of the user gets no second one.
#[test]
fn the_session_tests_in_docker_keep_the_entry_of_the_image() {
    let (uid, gid) = (host_id("-u"), host_id("-g"));
    let passwd = format!("me:x:{uid}:{gid}::/home/me:/bin/sh");
    let group = format!("us:x:{gid}:");
    let bin = tempfile::tempdir().unwrap();
    isolated::write_program(
        bin.path().join("docker"),
        format!(
            "#!/bin/sh\ncase \"$*\" in\n\
             *' cat /etc/passwd') echo '{passwd}' ;;\n\
             *' cat /etc/group') echo '{group}' ;;\n\
             *) for a in \"$@\"; do printf '%s\\n' \"$a\"; done >> {log} ;;\n\
             esac\n",
            log = bin.path().join("podman.log").display(),
        ),
    );
    let env = Isolated::new();
    let run = in_docker(&env, bin.path());
    assert_eq!(run[..3], ["run", "--rm", "--init"], "{run:?}");
    assert_eq!(bound(&run, "passwd"), format!("{passwd}\n"));
    assert_eq!(bound(&run, "group"), format!("{group}\n"));
}

/// The output of `id FLAG` on the host.
fn host_id(flag: &str) -> String {
    let out = Command::new("id").arg(flag).output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// Runs `deploy/session-test.sh` with the fake docker of `bin` in `env`,
/// and gives the arguments that the fake docker logged, one by one.
fn in_docker(env: &Isolated, bin: &Path) -> Vec<String> {
    let out = session_test_in(env, &[], bin, Some("docker"));
    assert!(out.status.success(), "{out:?}");
    logged_after_the_pull(bin)
}

/// The file that the arguments `run` bind with no write over
/// `/etc/FILE`.
fn bind_source(run: &[String], file: &str) -> PathBuf {
    let target = format!(":/etc/{file}:ro");
    let at = run
        .windows(2)
        .position(|w| w[0] == "-v" && w[1].ends_with(&target))
        .unwrap_or_else(|| panic!("no bind of /etc/{file}: {run:?}"));
    PathBuf::from(run[at + 1].trim_end_matches(&target))
}

/// The text of the file that the arguments `run` bind over `/etc/FILE`.
fn bound(run: &[String], file: &str) -> String {
    fs::read_to_string(bind_source(run, file)).unwrap()
}

/// 01M515Z3Y7GSFNTJGXJD727MQK, 01M6QQ1NXY0CXMZEEEP413W4PK,
/// 01M6QQ1P5YCT0JX90X5FCKM4W1: the job Build builds the tests in the
/// session image, with docker and a cache of `target/session`, and the
/// jobs "Tests I/N" of the Gate run each part from its archive in the
/// same image. No other job runs the session tests. The key of the cache
/// has the hash of `deploy/session-test.sh` (01M75NDR415ES5GK69MX80PTBS).
#[test]
fn the_gate_runs_the_tests_in_the_session_image() {
    let build = ci_job("build");
    for part in [
        "RIFF_ENGINE: docker\n",
        "CARGO_INCREMENTAL: 0\n",
        "uses: actions/cache@",
        "path: target/session\n",
        "key: session-${{ hashFiles('riff.toml', 'rust-toolchain.toml', 'Cargo.lock', 'deploy/session-test.sh') }}\n",
        "run: deploy/session-test.sh --build\n",
        "path: target/test-archive/\n",
    ] {
        assert!(
            build.contains(part),
            "{part} is not in the job build:\n{build}"
        );
    }
    let job = ci_job("test");
    for part in [
        "RIFF_ENGINE: docker\n",
        "path: target/test-archive\n",
        "run: deploy/session-test.sh --archive ${{ matrix.part }}\n",
    ] {
        assert!(job.contains(part), "{part} is not in the job test:\n{job}");
    }
    // The parts compile nothing: no cache of a build, no cargo.
    assert!(!job.contains("actions/cache"), "{job}");
    assert!(!job.contains("cargo"), "{job}");
    let workflow = read(".github/workflows/ci.yml");
    assert_eq!(workflow.matches("run: deploy/session-test.sh").count(), 2);
    assert!(!workflow.contains("\n  session_tests:\n"), "{workflow}");
    let justfile = read("justfile");
    assert!(
        justfile.contains("\nsession-test *ARGS:\n    deploy/session-test.sh {{ARGS}}\n"),
        "{justfile}"
    );
}

/// 01M5SNTDW9WVEXZW5D0TKGBA8Z, 01M77NG25CVVSD5MCN753C4BDW: the session
/// image of this repository builds `FROM` a digest of `riff-rust`, and
/// adds only what riff needs: it installs no `claude`, no `gh` and no
/// Rust, and it adds the Rust target of `just lint-macos`.
#[test]
fn the_session_image_builds_from_a_digest_of_the_rust_image_and_adds_only_what_riff_needs() {
    let dockerfile = read("deploy/session/Dockerfile");
    let from: Vec<&str> = dockerfile
        .lines()
        .filter(|line| line.starts_with("FROM "))
        .collect();
    assert_eq!(from.len(), 1, "{from:?}");
    let rust: Image = from[0].trim_start_matches("FROM ").parse().unwrap();
    assert_eq!(rust.name(), "ghcr.io/como-technologies/riff-rust");
    assert_ne!(
        rust.digest(),
        format!("sha256:{}", "0".repeat(64)),
        "deploy/session/Dockerfile pins the placeholder, not the digest of a build"
    );
    for part in [
        "claude.ai/install.sh",
        "gh_${GH_VERSION}",
        "sh.rustup.rs",
        "ARG CLAUDE_VERSION",
        "ARG GH_VERSION",
        "ARG RUST_CHANNEL",
    ] {
        assert!(
            !dockerfile.contains(part),
            "deploy/session/Dockerfile has {part:?}: riff-base and riff-rust have it"
        );
    }
    assert!(
        dockerfile.contains("\n && rustup target add aarch64-apple-darwin \\\n"),
        "{dockerfile}"
    );
    assert!(
        read("deploy/lint-macos.sh").contains("\ntarget=aarch64-apple-darwin\n"),
        "deploy/lint-macos.sh lints for another target"
    );
}
