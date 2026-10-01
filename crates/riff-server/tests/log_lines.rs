//! Each log line of `riff-server` is JSON with a `severity`
//! (01M3TJWJ3VK671T9NM95F3ES82). The tests run the real binary.

use std::fs;
use std::process::Stdio;
use std::time::{Duration, Instant};

use isolated::Isolated;
use riff_server::log::chunk_name;
use serde_json::Value;

/// A free port on the loopback address.
fn free_address() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}

/// Each line of `log` as JSON. A line that is not JSON fails the test.
fn lines(log: &str) -> Vec<Value> {
    log.lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}")))
        .collect()
}

#[test]
fn each_line_of_the_log_of_a_server_is_json_with_a_severity() {
    let listen = free_address();
    let log = tempfile::NamedTempFile::new().unwrap();
    let mut child = Isolated::shared()
        .riff_server()
        .env("RIFF_LISTEN", &listen)
        .stdin(Stdio::null())
        .stdout(log.reopen().unwrap())
        .stderr(log.reopen().unwrap())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while std::net::TcpStream::connect(&listen).is_err() {
        assert!(start.elapsed() < Duration::from_secs(20), "no open port");
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();

    let log = fs::read_to_string(log.path()).unwrap();
    let lines = lines(&log);
    assert!(lines.len() >= 3, "{log}");
    for line in &lines {
        let severity = line["severity"].as_str().expect("a severity");
        assert!(
            ["DEBUG", "INFO", "WARNING", "ERROR"].contains(&severity),
            "{line}"
        );
        assert!(line["message"].is_string() && line["time"].is_string());
    }
    let find = |text: &str| {
        lines
            .iter()
            .find(|line| line["message"].as_str().unwrap().contains(text))
            .unwrap_or_else(|| panic!("no line with {text} in {log}"))
    };
    assert_eq!(find("riff-server listens on")["severity"], "INFO");
    // A server with no store warns that its log is in memory only.
    assert_eq!(find("the log is in memory only")["severity"], "WARNING");
}

/// The alert of the cloud project fires on `severity>=ERROR`
/// (01M3TJWJ6J3M6JRXJTAETZ5M6F): a start that fails logs such a line.
#[test]
fn a_start_that_fails_logs_one_line_with_the_severity_error() {
    let dir = tempfile::tempdir().unwrap();
    let chunk = dir.path().join(chunk_name(999));
    fs::create_dir_all(chunk.parent().unwrap()).unwrap();
    fs::write(&chunk, "{\"format\":2,\"first\":999}\n").unwrap();
    let out = Isolated::shared()
        .riff_server()
        .arg("--dir")
        .arg(dir.path())
        .env("RIFF_LISTEN", free_address())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let log = String::from_utf8(out.stdout).unwrap();
    let errors: Vec<Value> = lines(&log)
        .into_iter()
        .filter(|line| line["severity"] == "ERROR")
        .collect();
    assert_eq!(errors.len(), 1, "{log}");
    let message = errors[0]["message"].as_str().unwrap();
    assert!(
        message.starts_with("riff-server stops: cannot read the saved object"),
        "{message}"
    );
}
