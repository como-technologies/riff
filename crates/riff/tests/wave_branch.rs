//! The wave keeps the default branch of the repository in no file
//! (01M5R56GN3DC09GAPAF69GENEZ, #1404): `waves.json` has the form of an
//! older build, and the look of the wave reads the branch from the forge.
//! A branch that does not read is the error of the look.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use riff::pr::Gh;
use riff::wave::{self, Wave};

const REPO: &str = "o/r";

/// A fake `gh` that logs each call to `gh.log`. The default branch is the
/// text of the file `branch`, and it does not read while the file
/// `branch-fails` is there.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/gh.log"
case "$*" in
  'api repos/o/r --jq .default_branch')
    if [ -f "$dir/branch-fails" ]; then echo 'gh: HTTP 502' >&2; exit 1; fi
    cat "$dir/branch" ;;
  'api repos/o/r') echo '{"default_branch":"master"}' ;;
  'api repos/o/r/milestones'*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
  'api repos/o/r/rules/branches/master'*) echo '[]' ;;
  'api repos/o/r/commits/'*) echo '{"check_runs":[]}' ;;
  'pr list'*) echo '[]' ;;
  'issue list'*) echo '[]' ;;
  *) exit 1 ;;
esac
"#;

/// The fake `gh` in `dir`, with the default branch `master`.
fn forge(dir: &Path) -> Gh {
    let path = dir.join("gh");
    isolated::write_program(&path, FAKE_GH);
    std::fs::write(dir.join("branch"), "master\n").unwrap();
    Gh::at(path)
}

/// The calls of `gh` so far.
fn calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("gh.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_look_of_a_wave_reads_the_default_branch_from_the_forge_at_each_look() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());
    let look = || wave::look_on(&gh, REPO, BTreeSet::new(), BTreeSet::new()).unwrap();
    assert_eq!(look().branch, "master");

    // The repository renamed its default branch: the next look has it.
    std::fs::write(dir.path().join("branch"), "trunk\n").unwrap();
    assert_eq!(look().branch, "trunk");
    let reads = calls(dir.path())
        .iter()
        .filter(|call| call.ends_with("--jq .default_branch"))
        .count();
    assert_eq!(reads, 2, "{:?}", calls(dir.path()));
}

#[test]
fn a_default_branch_that_does_not_read_is_the_error_of_the_look_and_the_next_look_reads_again() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());
    let look = || wave::look_on(&gh, REPO, BTreeSet::new(), BTreeSet::new());
    std::fs::write(dir.path().join("branch-fails"), "").unwrap();
    let error = format!("{:#}", look().unwrap_err());
    assert!(error.contains("repos/o/r"), "{error}");
    // The look read nothing else.
    assert_eq!(
        calls(dir.path()),
        ["api repos/o/r --jq .default_branch"],
        "{error}"
    );

    std::fs::remove_file(dir.path().join("branch-fails")).unwrap();
    assert_eq!(look().unwrap().branch, "master");
}

/// A wave of an older build: `waves.json` had four fields. The daemon of a
/// later build reads it, and the wave it writes has the same four.
#[test]
fn a_waves_file_of_an_older_build_is_valid_and_the_daemon_writes_no_branch() {
    let dir = tempfile::tempdir().unwrap();
    let old = serde_json::json!({REPO: {
        "repo": REPO, "main": "/src/r", "server": "http://127.0.0.1:1", "host": "thelio",
    }});
    std::fs::write(dir.path().join(wave::WAVES_FILE), old.to_string()).unwrap();

    let waves = wave::read_waves(dir.path()).unwrap();
    assert_eq!(waves[REPO].repo, REPO);
    assert_eq!(waves[REPO].host, "thelio");

    let other = Wave {
        repo: "o/other".into(),
        main: "/src/other".into(),
        server: "http://127.0.0.1:1".into(),
        host: "thelio".into(),
    };
    let waves: BTreeMap<String, Wave> = waves
        .into_iter()
        .chain([(other.repo.clone(), other)])
        .collect();
    wave::write_waves(dir.path(), &waves).unwrap();
    let kept = std::fs::read_to_string(dir.path().join(wave::WAVES_FILE)).unwrap();
    assert!(!kept.contains("branch"), "{kept}");
    assert_eq!(wave::read_waves(dir.path()).unwrap(), waves);
}
