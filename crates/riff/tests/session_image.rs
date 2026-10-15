//! The session image of this repository: `riff.toml` pins it by digest
//! (01M4Z7SP3KSKJYKNRP1MQ70C4Q), the workflow "Session image" builds it
//! from `deploy/session/` (01M4Z7SP7Y19BQ8QTV1T6AE8SR), its Dockerfile
//! has the toolchain of `rust-toolchain.toml`
//! (01M4Z7SP5TGJ3DMWFGE5SN68N1), and `riff image` prints it
//! (01M4Z7SPC20RWG218EWPEGYZTF).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use isolated::Isolated;
use riff::session_image::{FILE, Image};

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
