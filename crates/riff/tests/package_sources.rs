//! The package sources that the auto mode of a session trusts
//! (01M5SMPA9WN40DMQFNE3V1H4SK): they come from `sources` in the table
//! `[session]` of `riff.toml` on the default branch of `origin`. riff adds
//! none by itself.

use std::path::{Path, PathBuf};
use std::process::Command;

use riff::permissions::Trust;
use riff::session_image::{FILE, merged_sources, sources_of_config};
use serde_json::Value;

/// The root of this repository.
fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A main clone with `config` as `riff.toml` on `origin/main`: the folder
/// of the test, and the main clone.
fn main_clone(config: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let (origin, main) = (dir.path().join("origin.git"), dir.path().join("main"));
    git(
        dir.path(),
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            origin.to_str().unwrap(),
        ],
    );
    git(
        dir.path(),
        &["init", "-q", "-b", "main", main.to_str().unwrap()],
    );
    std::fs::write(main.join(FILE), config).unwrap();
    git(&main, &["add", FILE]);
    git(&main, &["commit", "-q", "-m", "config"]);
    git(
        &main,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(&main, &["push", "-q", "origin", "main"]);
    git(&main, &["remote", "set-head", "origin", "main"]);
    (dir, main)
}

/// The entries of `autoMode.environment` in the `settings.json` of the
/// Claude folder that riff makes for a session of `main`, and that trust
/// a package source.
fn trusted(main: &Path) -> Vec<String> {
    let sources = merged_sources(main).unwrap();
    let trust = Trust {
        owner: Some("acme"),
        server: Some("http://h:7878"),
        sources: &sources,
    };
    let folder = tempfile::tempdir().unwrap();
    let claude = folder.path().join("claude");
    riff::run::claude_folder(
        &claude,
        &folder.path().join("clone"),
        &folder.path().join("none.json"),
        &trust,
    )
    .unwrap();
    let settings: Value =
        serde_json::from_slice(&std::fs::read(claude.join("settings.json")).unwrap()).unwrap();
    settings["autoMode"]["environment"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_owned())
        // ghcr.io is the registry of the session image, not a package source.
        .filter(|e| e.starts_with("Trusted internal domains: ") && !e.contains("ghcr.io"))
        .collect()
}

#[test]
fn with_no_sources_no_package_source_is_trusted() {
    let (_dir, main) = main_clone("[session]\nimage = \"ghcr.io/o/i@sha256:0\"\n");
    assert_eq!(merged_sources(&main).unwrap(), Vec::<String>::new());
    assert_eq!(trusted(&main), Vec::<String>::new());
}

#[test]
fn with_sources_each_host_is_trusted() {
    let (_dir, main) =
        main_clone("[session]\nsources = [\"pypi.org\", \"files.pythonhosted.org\"]\n");
    let entries = trusted(&main);
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert!(entries[0].starts_with("Trusted internal domains: pypi.org."));
    assert!(entries[1].starts_with("Trusted internal domains: files.pythonhosted.org."));
}

#[test]
fn only_the_merged_sources_count() {
    let (_dir, main) = main_clone("[session]\nsources = [\"pypi.org\"]\n");
    // An edit in the working tree, and a commit that is not on origin.
    std::fs::write(main.join(FILE), "[session]\nsources = [\"evil.example\"]\n").unwrap();
    git(&main, &["commit", "-q", "-a", "-m", "not merged"]);
    let entries = trusted(&main);
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].contains("pypi.org"), "{entries:?}");
}

#[test]
fn a_clone_with_no_config_on_origin_trusts_no_source() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    assert!(merged_sources(dir.path()).unwrap().is_empty());
}

#[test]
fn a_sources_key_that_is_no_list_of_hosts_is_an_error() {
    for bad in [
        "sources = \"pypi.org\"",
        "sources = [\"https://pypi.org\"]",
        "sources = [\"pypi.org/simple\"]",
        "sources = [\"Pypi.org\"]",
        "sources = [\"\"]",
        "sources = [\"a b\"]",
        "sources = [1]",
    ] {
        let config = format!("[session]\n{bad}\n");
        let error = sources_of_config(&config, "riff.toml")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("`sources` in [session] of riff.toml"),
            "{bad}: {error}"
        );
        let (_dir, main) = main_clone(&config);
        assert!(merged_sources(&main).is_err(), "{bad}");
    }
}

/// The riff.toml of this repository names the sources of cargo, so the
/// sessions of riff keep the trust that they had.
#[test]
fn this_repository_names_the_sources_of_cargo() {
    let text = std::fs::read_to_string(top().join(FILE)).unwrap();
    assert_eq!(
        sources_of_config(&text, FILE).unwrap(),
        ["crates.io", "index.crates.io", "static.crates.io"]
    );
}
