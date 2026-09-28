use assert_cmd::Command;
use isolated::Isolated;

/// The start hook, away from the local files and the riff-server of
/// this machine (R167). No server listens on port 9.
fn hook(run: &tempfile::TempDir) -> Command {
    let mut cmd = Isolated::shared().assert_riff();
    cmd.args(["hook", "session-start"])
        .env("RIFF_HOME", run.path())
        .env("RIFF_SERVER", "http://127.0.0.1:9")
        .env_remove("RIFF_SESSION");
    cmd
}

#[test]
fn version_names_the_binary() {
    Isolated::shared()
        .assert_riff()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("riff {}\n", riff_core::build::VERSION));
}

#[test]
fn session_start_hook_adds_the_watch_context() {
    let run = tempfile::tempdir().unwrap();
    let out = hook(&run)
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
    assert!(context.contains("`riff watch --once` with the Bash tool"));
}

#[test]
fn a_cloud_session_has_the_host_cloud() {
    let run = tempfile::tempdir().unwrap();
    let out = hook(&run)
        .env("RIFF_USER", "mike")
        .env_remove("RIFF_HOST")
        .env("CLAUDE_CODE_REMOTE", "true")
        .write_stdin(r#"{"session_id":"a6cf","source":"startup","hook_event_name":"SessionStart"}"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("riff://mike@cloud/"), "{out}");
}

#[test]
fn session_start_hook_never_fails() {
    let run = tempfile::tempdir().unwrap();
    let out = hook(&run)
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

#[test]
fn login_says_when_it_cannot_reach_the_server() {
    let stderr = Isolated::shared()
        .assert_riff()
        .args(["login", "--server", "http://127.0.0.1:9"])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(stderr).unwrap();
    assert!(stderr.contains("cannot reach riff-server"), "{stderr}");
}

#[test]
fn the_default_server_is_the_local_server() {
    let out = Isolated::shared()
        .assert_riff()
        .args(["login", "--help"])
        .env_remove("RIFF_SERVER")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(out).unwrap();
    assert!(help.contains("[default: http://127.0.0.1:7878]"), "{help}");
}
