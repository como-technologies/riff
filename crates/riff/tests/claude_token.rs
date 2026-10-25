//! `riff claude-token` keeps the token of `claude setup-token` only from
//! a person in a terminal (01M58B7GZT7WFSBBG1XWJHR4MH).

use std::io::Write;
use std::process::Stdio;

use isolated::Isolated;

/// With stdin from a pipe, as in an agent session, `riff claude-token`
/// refuses, keeps no token, and shows no value.
#[test]
fn riff_claude_token_refuses_a_token_that_does_not_come_from_a_terminal() {
    let env = Isolated::new();
    let token = "sk-ant-oat01-secret1";
    let mut child = env
        .riff()
        .arg("claude-token")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{token}\n").as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success(), "{out:?}");
    let shown = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(shown.contains("in a terminal only"), "{shown}");
    assert!(!shown.contains(token), "{shown}");
    assert!(!written(&env.riff_home(), token), "a file holds the token");
}

/// True when a file under `dir` holds `text`.
fn written(dir: &std::path::Path, text: &str) -> bool {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            let path = entry.path();
            if path.is_dir() {
                written(&path, text)
            } else {
                std::fs::read_to_string(&path).is_ok_and(|content| content.contains(text))
            }
        })
}
