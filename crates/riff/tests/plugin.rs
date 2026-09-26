//! The written plugin passes the Claude Code validator.

use std::process::Command;

#[test]
fn claude_accepts_the_written_plugin() {
    let dir = tempfile::tempdir().unwrap();
    riff::plugin::write(dir.path()).unwrap();
    let Ok(out) = Command::new("claude").arg("--version").output() else {
        eprintln!("skip: the claude command is not installed");
        return;
    };
    assert!(out.status.success());
    for path in [dir.path().to_owned(), dir.path().join(riff::plugin::NAME)] {
        let out = Command::new("claude")
            .args(["plugin", "validate", "--strict"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}: {}{}",
            path.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
