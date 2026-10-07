//! A test run never touches the riff of the machine
//! (01M3MY2KWKBJCQ0BCNC6533RBW), and `RIFF_HOME` gives riff a home of
//! its own (01M3MY2KSV73WS8D902YCH2PRX).

use std::net::TcpListener;
use std::path::Path;
use std::process::Output;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use isolated::Isolated;
use riff::login::{SignIn, secret_name};
use riff::secrets;

/// The variable that makes this test binary run [`child_runs_riff`].
const CHILD: &str = "ISOLATION_CHILD";

/// The sign-in of the tests, as the store keeps it.
fn sign_in() -> String {
    serde_json::to_string(&SignIn {
        user: "mike".into(),
        access_token: "a-1".into(),
        refresh_token: "r-1".into(),
        expires_at: 0,
        riff_id: None,
    })
    .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Each file under `dir`, with its content.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(snapshot(&path));
        } else {
            files.push((path.display().to_string(), std::fs::read(&path).unwrap()));
        }
    }
    files.sort();
    files
}

/// The variable that makes [`child_runs_riff`] also call the riff of the
/// machine on purpose.
const CALL_MACHINE: &str = "ISOLATION_CALL_MACHINE";

/// The variable that makes this test binary run [`noise_calls_the_machine`].
const NOISE: &str = "ISOLATION_NOISE";

/// The riff of the machine.
const MACHINE: &str = "127.0.0.1:7878";

/// Runs in a child of the tests below only: riff runs commands that go
/// to the server.
#[test]
fn child_runs_riff() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    // `riff server` and `riff update` also ask the riff of the machine
    // on purpose, so they are not here.
    let env = Isolated::new();
    for args in [&["who"][..], &["whoami"], &["read"], &["claim", "issue-1"]] {
        env.riff().args(args).output().unwrap();
    }
    if std::env::var_os(CALL_MACHINE).is_some() {
        env.riff()
            .env("RIFF_SERVER", format!("http://{MACHINE}"))
            .arg("who")
            .output()
            .unwrap();
    }
}

/// Runs in a child of [`a_call_of_another_process_does_not_count`] only:
/// it calls the riff of the machine in a loop, as a session or a test of
/// another worktree can.
#[test]
fn noise_calls_the_machine() {
    use std::io::{Read, Write};
    if std::env::var_os(NOISE).is_none() {
        return;
    }
    let end = Instant::now() + Duration::from_secs(120);
    while Instant::now() < end {
        if let Ok(mut stream) = std::net::TcpStream::connect(MACHINE) {
            let _ = stream.write_all(b"GET /who HTTP/1.1\r\nhost: 127.0.0.1\r\n\r\n");
            let _ = stream.read(&mut [0; 16]);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A live server on `server`: it counts each call, and ends it at once.
fn counting(server: TcpListener) -> Arc<AtomicUsize> {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    std::thread::spawn(move || {
        for stream in server.incoming() {
            counted.fetch_add(1, Ordering::SeqCst);
            drop(stream);
        }
    });
    calls
}

/// Runs [`child_runs_riff`] in a child with `RIFF_SERVER` of the shell
/// set to `server`, or unset. With `call_machine`, the child also calls
/// the riff of the machine. The child and each riff under it carry the
/// value of [`CHILD`] that this returns.
fn run_child(server: Option<&str>, call_machine: bool) -> String {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let marker = format!(
        "{}-{}",
        std::process::id(),
        RUNS.fetch_add(1, Ordering::SeqCst)
    );
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "child_runs_riff", "--nocapture"])
        .env(CHILD, &marker);
    match server {
        Some(server) => cmd.env("RIFF_SERVER", server),
        None => cmd.env_remove("RIFF_SERVER"),
    };
    if call_machine {
        cmd.env(CALL_MACHINE, "1");
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("1 passed"), "{out:?}");
    marker
}

#[test]
fn the_helper_keeps_the_riff_server_of_the_shell_out() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", server.local_addr().unwrap());
    let calls = counting(server);
    run_child(Some(&url), false);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a riff under the helper called RIFF_SERVER of the shell"
    );
}

/// The riff of the machine, as the tests see it: a live server on
/// [`MACHINE`] that counts each call by the value of [`CHILD`] of the
/// process that calls. A caller with no value, for example a riff of a
/// session or a test of another worktree, counts under "". So a test
/// counts only the calls of its own child.
#[cfg(target_os = "linux")]
mod machine {
    use std::collections::HashMap;
    use std::net::{TcpListener, TcpStream};
    use std::sync::{LazyLock, Mutex};

    use super::{CHILD, MACHINE};

    type Calls = Mutex<HashMap<String, usize>>;

    /// `None` when a real riff of this machine holds the port: the
    /// helper still sends no call there, but the tests cannot count
    /// them.
    static CALLS: LazyLock<Option<&'static Calls>> = LazyLock::new(|| {
        let server = TcpListener::bind(MACHINE).ok()?;
        let calls: &'static Calls = Box::leak(Box::default());
        std::thread::spawn(move || {
            for stream in server.incoming().flatten() {
                std::thread::spawn(move || count(calls, stream));
            }
        });
        Some(calls)
    });

    /// Counts the call on `stream`. The caller waits for an answer, so
    /// it still holds its end while this finds it.
    fn count(calls: &Calls, stream: TcpStream) {
        let port = stream.peer_addr().map(|a| a.port()).unwrap_or(0);
        let marker = caller_marker(port).unwrap_or_default();
        *calls.lock().unwrap().entry(marker).or_default() += 1;
    }

    /// The calls of the callers with the value `marker` of [`CHILD`], or
    /// `None` when a real riff of this machine holds the port.
    pub fn calls(marker: &str) -> Option<usize> {
        let calls = (*CALLS)?;
        Some(calls.lock().unwrap().get(marker).copied().unwrap_or(0))
    }

    /// The value of [`CHILD`] of the process that holds the local end
    /// 127.0.0.1:`port` of a call to [`MACHINE`].
    fn caller_marker(port: u16) -> Option<String> {
        let link = format!("socket:[{}]", socket_inode(port)?);
        for process in std::fs::read_dir("/proc").ok()?.flatten() {
            let Ok(fds) = std::fs::read_dir(process.path().join("fd")) else {
                continue;
            };
            let holds = fds.flatten().any(|fd| {
                std::fs::read_link(fd.path()).is_ok_and(|to| to.as_os_str() == link.as_str())
            });
            if holds {
                let environ = std::fs::read(process.path().join("environ")).ok()?;
                let prefix = format!("{CHILD}=");
                return environ
                    .split(|b| *b == 0)
                    .find_map(|var| var.strip_prefix(prefix.as_bytes()))
                    .map(|value| String::from_utf8_lossy(value).into_owned());
            }
        }
        None
    }

    /// The inode of the socket from 127.0.0.1:`port` to [`MACHINE`], as
    /// `/proc/net/tcp` shows it.
    fn socket_inode(port: u16) -> Option<u64> {
        let local = format!("0100007F:{port:04X}");
        let tcp = std::fs::read_to_string("/proc/net/tcp").ok()?;
        tcp.lines().skip(1).find_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let ours =
                fields.get(1) == Some(&local.as_str()) && fields.get(2) == Some(&"0100007F:1EC6");
            ours.then(|| fields.get(9)?.parse().ok()).flatten()
        })
    }
}

#[cfg(target_os = "linux")]
#[test]
fn the_helper_keeps_the_riff_of_the_machine_out() {
    assert_eq!(isolated::DEAD_SERVER, "http://127.0.0.1:9");
    if machine::calls("").is_none() {
        eprintln!("skip: {MACHINE} is in use");
        return;
    }
    let marker = run_child(None, false);
    assert_eq!(
        machine::calls(&marker),
        Some(0),
        "a riff under the helper called the riff of the machine"
    );
}

/// A child process that ends when it drops.
struct Kill(std::process::Child);

impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_call_of_another_process_does_not_count() {
    let Some(before) = machine::calls("") else {
        eprintln!("skip: {MACHINE} is in use");
        return;
    };
    let _noise = Kill(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "noise_calls_the_machine", "--nocapture"])
            .env(NOISE, "1")
            .env_remove(CHILD)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let span = isolated::Span::start();
    while machine::calls("") == Some(before) {
        assert!(span.within(Duration::from_secs(30)), "no noise");
        std::thread::sleep(Duration::from_millis(20));
    }
    let during = machine::calls("").unwrap();
    let marker = run_child(None, false);
    assert!(machine::calls("").unwrap() > during, "the noise stopped");
    assert_eq!(machine::calls(&marker), Some(0));
}

#[cfg(target_os = "linux")]
#[test]
fn a_call_of_the_child_counts() {
    if machine::calls("").is_none() {
        eprintln!("skip: {MACHINE} is in use");
        return;
    }
    let marker = run_child(None, true);
    assert!(
        machine::calls(&marker).unwrap() > 0,
        "the call of the child"
    );
}

#[test]
fn riff_home_keeps_the_settings_out_of_the_config_of_the_person() {
    let env = Isolated::new();
    let config = env.home().join(".config/riff");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("config.toml"), "[workers]\nlimit = 1\n").unwrap();
    let before = snapshot(&config);

    let out = env.riff().args(["workers", "limit", "3"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let out = env.riff().args(["workers", "limit"]).output().unwrap();
    assert!(stdout(&out).contains('3'), "{out:?}");

    assert_eq!(snapshot(&config), before);
    let kept = std::fs::read_to_string(env.riff_home().join("config.toml")).unwrap();
    assert!(kept.contains("limit = 3"), "{kept}");
}

#[test]
fn riff_home_keeps_the_sign_in_in_its_files_not_in_the_keyring() {
    let env = Isolated::new();
    let server = "http://127.0.0.1:9";
    let files = env.riff_home().join("secrets");
    secrets::file_set(&files, &secret_name(server), &sign_in()).unwrap();

    let out = env
        .riff()
        .args(["--server", server, "logout"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("You signed out"), "{out:?}");
    assert_eq!(
        secrets::file_get(&files, &secret_name(server)).unwrap(),
        None
    );
}

#[test]
fn riff_without_riff_home_finds_no_keyring_under_the_helper() {
    // The helper gives riff a D-Bus that does not exist, so each call
    // to the OS keyring fails. With RIFF_HOME, riff makes none.
    let env = Isolated::new();
    let out = env
        .riff()
        .env_remove("RIFF_HOME")
        .args(["--server", "http://127.0.0.1:9", "logout"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("keyring"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn only_the_owner_reads_a_secret_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    secrets::file_set(dir.path(), "k", "v").unwrap();
    let mode = std::fs::metadata(secrets::file_of(dir.path(), "k"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "{mode:o}");
}
