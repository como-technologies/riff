//! `riff-server` saves and exits on SIGTERM (R129).

use isolated::Isolated;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn sigterm_stops_the_server_with_success() {
    let mut cmd = Isolated::shared().riff_server();
    let mut server = cmd
        .args(["--listen", "127.0.0.1:0"])
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(server.stdout.take().unwrap()).lines();
    while !lines.next().unwrap().unwrap().contains("listens on") {}

    let pid = server.id().to_string();
    let kill = Command::new("kill").args(["-TERM", &pid]).status().unwrap();
    assert!(kill.success());
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = server.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "riff-server did not stop");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success(), "{status}");
    let rest: Vec<String> = lines.map_while(Result::ok).collect();
    assert!(
        rest.iter().any(|l| l.contains("saving the state")),
        "{rest:?}"
    );
}
