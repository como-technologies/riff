//! The base image of riff, riff-base (01M5SNT57ANM99NQS5SNB8XPWV): the
//! Dockerfile of `deploy/base/` has each tool of a session and no
//! language, `deploy/contract.sh` checks that contract in a container
//! (01M5SNTE280ABA4RYEPK445P3A), and the workflow "Release images" tags
//! the images of a release (01M5SNTE54DH3N568GZXRQ86DC).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use isolated::Isolated;

use crate::release::fake_engine;

/// The root of this repository.
fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The text of the file `path` of this repository.
fn read(path: &str) -> String {
    fs::read_to_string(top().join(path)).unwrap()
}

/// The instructions of a Dockerfile, one on each item: with no comment
/// and no empty line, and a line that ends with a backslash joined to
/// the next line.
fn instructions(dockerfile: &str) -> Vec<String> {
    let mut steps = Vec::new();
    let mut open = String::new();
    for line in dockerfile.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.strip_suffix('\\') {
            Some(part) => {
                open.push_str(part.trim_end());
                open.push(' ');
            }
            None => {
                open.push_str(line);
                steps.push(std::mem::take(&mut open));
            }
        }
    }
    steps
}

/// The value of `ARG NAME=VALUE` in `steps`.
fn arg(steps: &[String], name: &str) -> String {
    let prefix = format!("ARG {name}=");
    steps
        .iter()
        .find_map(|s| s.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("no {prefix}"))
        .to_owned()
}

/// True when `version` is `X.Y.Z` with numbers.
fn is_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// 01M5SNT57ANM99NQS5SNB8XPWV: riff-base has Debian slim, bash, CA
/// certificates, git, curl, procps, gh at `/usr/bin/gh` and claude at
/// fixed versions.
#[test]
fn the_base_image_has_each_tool_of_a_session() {
    let steps = instructions(&read("deploy/base/Dockerfile"));
    assert!(
        steps[0].starts_with("FROM docker.io/library/debian:") && steps[0].ends_with("-slim"),
        "{steps:?}"
    );
    let apt = steps
        .iter()
        .find(|s| s.contains("apt-get install"))
        .expect("an apt-get install");
    for package in ["bash", "ca-certificates", "curl", "git", "procps"] {
        assert!(
            apt.split_whitespace().any(|word| word == package),
            "no {package}: {apt}"
        );
    }
    // gh: the package of its release, so the binary is /usr/bin/gh.
    let gh = arg(&steps, "GH_VERSION");
    assert!(is_version(&gh), "GH_VERSION {gh}");
    let step = steps
        .iter()
        .find(|s| s.contains("dpkg -i"))
        .expect("the gh package");
    assert!(step.contains("/gh_${GH_VERSION}_linux_amd64.deb"), "{step}");
    // claude: the installer of Anthropic, at the fixed version.
    let claude = arg(&steps, "CLAUDE_VERSION");
    assert!(is_version(&claude), "CLAUDE_VERSION {claude}");
    let step = steps
        .iter()
        .find(|s| s.contains("claude.ai/install.sh"))
        .expect("claude");
    assert!(step.contains("bash -s \"$CLAUDE_VERSION\""), "{step}");
    assert!(step.contains("/usr/local/bin/claude"), "{step}");
    assert!(
        steps
            .iter()
            .any(|s| s.starts_with("ENV DISABLE_AUTOUPDATER=1")),
        "{steps:?}"
    );
}

/// 01M5SNT57ANM99NQS5SNB8XPWV: riff-base has no language, no build tool,
/// no D-Bus, no keyring, no riff and no content of a project.
#[test]
fn the_base_image_has_no_language_and_no_build_tool() {
    let steps = instructions(&read("deploy/base/Dockerfile"));
    let words: Vec<&str> = steps
        .iter()
        .flat_map(|s| s.split(|c: char| !(c.is_alphanumeric() || "-_.+".contains(c))))
        .collect();
    for banned in [
        "rustup",
        "cargo",
        "rustc",
        "gcc",
        "g++",
        "cc",
        "make",
        "cmake",
        "pkg-config",
        "libc6-dev",
        "node",
        "nodejs",
        "npm",
        "python",
        "python3",
        "pip",
        "go",
        "golang",
        "java",
        "default-jdk",
        "ruby",
        "php",
        "dotnet",
        "dbus",
        "dbus-bin",
        "dbus-daemon",
        "gnome-keyring",
        "libsecret-1-0",
        "just",
        "mdbook",
        "sccache",
    ] {
        assert!(!words.contains(&banned), "riff-base installs {banned}");
    }
    for step in &steps {
        for instruction in ["COPY", "ADD", "USER", "ENTRYPOINT"] {
            assert!(
                !step.starts_with(&format!("{instruction} ")),
                "riff-base has {step:?}: it holds no project content and no riff"
            );
        }
    }
}

/// `deploy/contract.sh` with `args`, the fake engine of `bin` first in
/// `PATH`.
fn contract(args: &[&str], bin: &Path) -> Output {
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    Isolated::new()
        .command(top().join("deploy/contract.sh"))
        .args(args)
        .env("PATH", path)
        .env_remove("RIFF_ENGINE")
        .output()
        .unwrap()
}

/// 01M5SNTE280ABA4RYEPK445P3A: the script runs the checks in a container
/// that has no network and a user that is not root, with the versions
/// of `deploy/base/Dockerfile` and the channel of `rust-toolchain.toml`.
#[test]
fn the_contract_script_checks_the_image_in_a_container() {
    let steps = instructions(&read("deploy/base/Dockerfile"));
    let toolchain: toml_edit::DocumentMut = read("rust-toolchain.toml").parse().unwrap();
    let channel = toolchain["toolchain"]["channel"].as_str().unwrap();
    for kind in ["base", "rust"] {
        let bin = tempfile::tempdir().unwrap();
        fake_engine(bin.path());
        let out = contract(&[kind, "ghcr.io/o/i@sha256:0"], bin.path());
        assert!(out.status.success(), "{out:?}");
        let log = fs::read_to_string(bin.path().join("podman.log")).unwrap();
        let args: Vec<&str> = log.lines().collect();
        assert_eq!(
            args[..8],
            [
                "run",
                "--rm",
                "--network=none",
                "--user",
                "12345:12345",
                "-e",
                "HOME=/tmp",
                "--entrypoint"
            ],
            "{args:?}"
        );
        assert_eq!(args[8..10], ["bash", "ghcr.io/o/i@sha256:0"], "{args:?}");
        assert_eq!(args[10], "-c", "{args:?}");
        // The script has many lines, and the fake engine logs each line.
        let (script, tail) = args[11..].split_at(args.len() - 11 - 5);
        assert_eq!(
            tail,
            [
                "contract",
                &arg(&steps, "CLAUDE_VERSION"),
                &arg(&steps, "GH_VERSION"),
                channel,
                kind
            ],
            "{args:?}"
        );
        // The script names each tool of the contract.
        for tool in [
            "gh",
            "claude",
            "git",
            "curl",
            "ps",
            "bash",
            "rustfmt",
            "clippy-driver",
        ] {
            assert!(script.join("\n").contains(tool), "no check of {tool}");
        }
    }
}

/// A kind that is not `base` or `rust`, and a call with no image, give
/// the usage and the exit 2. The engine does not run.
#[test]
fn the_contract_script_refuses_a_bad_call() {
    for args in [&["session", "i"][..], &["base"], &[]] {
        let bin = tempfile::tempdir().unwrap();
        fake_engine(bin.path());
        let out = contract(args, bin.path());
        assert_eq!(out.status.code(), Some(2), "{args:?}: {out:?}");
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            error.contains("usage: deploy/contract.sh base|rust IMAGE"),
            "{error}"
        );
        assert!(!bin.path().join("podman.log").exists(), "{args:?}");
    }
}

/// 01M5SNTE54DH3N568GZXRQ86DC: on a release tag, the workflow checks
/// that the packages are public, tags the images that the release pins
/// with the version and `latest`, and adds their digests to the notes of
/// the release.
#[test]
fn the_release_workflow_tags_and_names_the_images_of_the_release() {
    let workflow = read(".github/workflows/release-images.yml");
    assert!(workflow.contains("tags: [\"v*\"]\n"), "{workflow}");
    assert!(
        workflow.contains("sed -n 's/^FROM //p' deploy/rust/Dockerfile"),
        "{workflow}"
    );
    assert!(
        workflow.contains("sed -n 's/^FROM //p' deploy/session/Dockerfile"),
        "{workflow}"
    );
    for part in [
        "https://ghcr.io/token?service=ghcr.io&scope=repository:$name:pull",
        "docker buildx imagetools create",
        "--tag \"${ref%@*}:$version\" --tag \"${ref%@*}:latest\" \"$ref\"",
        "gh release edit \"$TAG\"",
        "## Images",
    ] {
        assert!(workflow.contains(part), "{part} is not in the workflow");
    }
    // The check of the packages comes before the tags.
    let public = workflow.find("Check that the packages are public").unwrap();
    let tags = workflow
        .find("Tag the images with the version and latest")
        .unwrap();
    assert!(public < tags);
}
