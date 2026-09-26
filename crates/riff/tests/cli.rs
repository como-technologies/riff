use assert_cmd::Command;

#[test]
fn version_names_the_binary() {
    Command::cargo_bin("riff")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(concat!("riff ", env!("CARGO_PKG_VERSION"), "\n"));
}

#[test]
fn session_start_hook_adds_the_watch_context() {
    let out = Command::cargo_bin("riff")
        .unwrap()
        .args(["hook", "session-start"])
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .write_stdin(r#"{"session_id":"a6cf","source":"startup","hook_event_name":"SessionStart"}"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let context = out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("riff://mike@pangolin/"), "{context}");
    assert!(context.contains("session=a6cf"));
    assert!(context.contains("`riff watch` with the Monitor tool"));
}

#[test]
fn session_start_hook_never_fails() {
    let out = Command::cargo_bin("riff")
        .unwrap()
        .args(["hook", "session-start"])
        .env_remove("USER")
        .env_remove("RIFF_USER")
        .write_stdin("not json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8(out).unwrap().contains("riff watch"));
}
