//! Each log line of `riff-server` is JSON with a `severity`
//! (01M3TJWJ3VK671T9NM95F3ES82). The tests run the real binary.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use isolated::Isolated;
use riff_server::log::chunk_name;
use riff_server::trace::{BODY_TIME, DENIED_INTERVAL, DENIED_MAX};
use serde_json::Value;
use tempfile::NamedTempFile;

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

/// A server that listens on `listen` and writes its lines to `log`.
fn server(listen: &str, log: &NamedTempFile) -> Child {
    let child = Isolated::shared()
        .riff_server()
        .env("RIFF_LISTEN", listen)
        .stdin(Stdio::null())
        .stdout(log.reopen().unwrap())
        .stderr(log.reopen().unwrap())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while TcpStream::connect(listen).is_err() {
        assert!(start.elapsed() < Duration::from_secs(20), "no open port");
        std::thread::sleep(Duration::from_millis(20));
    }
    child
}

/// Stops `server` with SIGTERM, and gives each line of the token layer
/// in `log`: the lines `denied` and `dropped`.
fn stop(mut server: Child, log: &NamedTempFile) -> Vec<Value> {
    let pid = server.id().to_string();
    let kill = Command::new("kill").args(["-TERM", &pid]).status().unwrap();
    assert!(kill.success());
    assert!(server.wait().unwrap().success());
    let log = fs::read_to_string(log.path()).unwrap();
    let mut lines = lines(&log);
    lines.retain(|line| line["result"] == "denied" || line["result"] == "dropped");
    lines
}

/// Sends `head` and `body` of a call with no build, which the server
/// refuses, and reads the reply until the server closes the connection.
fn refused_call(listen: &str, head: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(listen).unwrap();
    let wait = BODY_TIME + Duration::from_secs(30);
    stream.set_read_timeout(Some(wait)).unwrap();
    let call = format!(
        "POST /v1/claim HTTP/1.1\r\nhost: riff\r\ncontent-type: application/json\r\n{head}\r\n\r\n{body}"
    );
    stream.write_all(call.as_bytes()).unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    reply
}

/// The read of the body of a refused call has a time limit
/// (01M3Z67B9RMVKY7TCXCG8HEZT4): a client sends a part of the body and
/// then nothing. The reply comes at the limit, and the server closes
/// the call. The line has no `named`.
#[test]
fn a_refused_call_with_a_slow_body_ends_at_the_time_limit() {
    let listen = free_address();
    let log = NamedTempFile::new().unwrap();
    let server = server(&listen, &log);

    let start = Instant::now();
    let part = r#"{"me":"riff://mike@pangolin/como-technologies/riff?session=a"#;
    let reply = refused_call(&listen, "content-length: 1000", part);
    let took = start.elapsed();
    assert!(reply.starts_with("HTTP/1.1 409"), "{reply}");
    assert!(
        reply.to_lowercase().contains("\r\nconnection: close\r\n"),
        "{reply}"
    );
    assert!(took >= BODY_TIME, "{took:?}");
    assert!(took < BODY_TIME + Duration::from_secs(20), "{took:?}");

    let lines = stop(server, &log);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["result"], "denied");
    assert_eq!(lines[0]["code"], "old_build");
    assert_eq!(lines[0]["path"], "/v1/claim");
    assert!(lines[0].get("named").is_none(), "{}", lines[0]);
}

/// The `denied` lines have a limit of rate (01M3Z67DZX9BC3TYF3PWGFGZJ7):
/// 1000 refused calls give no more lines than the limit, and one line
/// with the count. Each call gets the same reply. A stop of the server
/// writes the count of the window that it ends.
#[test]
fn a_thousand_refused_calls_give_the_lines_of_the_limit_and_one_count() {
    let listen = free_address();
    let log = NamedTempFile::new().unwrap();
    let server = server(&listen, &log);

    let body = r#"{"me":"riff://mike@pangolin/como-technologies/riff?session=a"}"#;
    let head = format!("connection: close\r\ncontent-length: {}", body.len());
    let text = |reply: &str| reply.split_once("\r\n\r\n").unwrap().1.to_owned();
    let start = Instant::now();
    let first = refused_call(&listen, &head, body);
    assert!(first.starts_with("HTTP/1.1 409"), "{first}");
    for _ in 1..1000 {
        let reply = refused_call(&listen, &head, body);
        assert!(reply.starts_with("HTTP/1.1 409"), "{reply}");
        assert_eq!(text(&reply), text(&first));
    }
    // A slow machine can need more than one window for the calls.
    let windows = start.elapsed().as_secs() / DENIED_INTERVAL.as_secs() + 1;

    let lines = stop(server, &log);
    let of = |result: &str| -> Vec<&Value> {
        let of_result = lines.iter().filter(|line| line["result"] == result);
        of_result.collect()
    };
    let denied = of("denied");
    let dropped = of("dropped");
    assert_eq!(lines.len(), denied.len() + dropped.len(), "{lines:?}");
    let written = denied.len() as u64;
    assert!(written <= DENIED_MAX * windows, "{written} in {windows}");
    let counts = dropped.iter().map(|line| line["count"].as_u64().unwrap());
    assert_eq!(written + counts.sum::<u64>(), 1000);
    assert!(!dropped.is_empty() && dropped.len() as u64 <= windows);
    if windows == 1 {
        assert_eq!(written, DENIED_MAX);
        assert_eq!(dropped[0]["count"], 1000 - DENIED_MAX);
    }
    for line in denied {
        assert_eq!(line["code"], "old_build");
        assert_eq!(line["named"], serde_json::json!({"session": "mike/a"}));
    }
    for line in dropped {
        assert_eq!(line["severity"], "WARNING");
    }
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
