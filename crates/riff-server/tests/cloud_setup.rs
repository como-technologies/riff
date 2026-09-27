//! `deploy/cloud-setup.sh` makes the bucket and sets its lifecycle rule
//! (R46, R136). The tests run the script with a fake `gcloud` that
//! writes each call to a log.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn deploy() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy")
}

/// Runs the script with a fake `gcloud`. The fake finds the bucket
/// only when `bucket` is true. Returns the calls, one on each line.
fn run(bucket: bool) -> String {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("calls");
    let fake = dir.path().join("gcloud");
    fs::write(
        &fake,
        format!(
            r#"#!/bin/sh
echo "$*" >> {log}
case "$*" in
    "billing projects describe"*) echo True ;;
    "storage buckets describe"*) exit {missing} ;;
    "secrets versions list"*) echo 1 ;;
esac
"#,
            log = log.display(),
            missing = if bucket { 0 } else { 1 },
        ),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        dir.path().display(),
        std::env::var("PATH").unwrap()
    );
    let out = Command::new(deploy().join("cloud-setup.sh"))
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::read_to_string(log).unwrap()
}

#[test]
fn setup_makes_the_bucket_and_sets_the_rule() {
    let calls = run(false);
    let make = calls
        .lines()
        .find(|l| l.starts_with("storage buckets create gs://como-riff-state "))
        .expect("the script makes the bucket");
    assert!(make.contains("--public-access-prevention"), "{make}");
    assert!(make.contains("--uniform-bucket-level-access"), "{make}");
    assert!(
        calls.contains(
            "storage buckets update gs://como-riff-state --lifecycle-file lifecycle.json"
        )
    );
}

#[test]
fn setup_again_keeps_the_bucket_and_sets_the_rule() {
    let calls = run(true);
    assert!(!calls.contains("storage buckets create"), "{calls}");
    assert!(calls.contains("storage buckets update gs://como-riff-state --lifecycle-file"));
}

#[test]
fn the_rule_deletes_only_thread_objects_after_30_days() {
    let text = fs::read_to_string(deploy().join("lifecycle.json")).unwrap();
    let rules: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        rules,
        serde_json::json!({"rule": [{
            "action": {"type": "Delete"},
            "condition": {"age": 30, "matchesPrefix": ["threads/"]},
        }]})
    );
}
