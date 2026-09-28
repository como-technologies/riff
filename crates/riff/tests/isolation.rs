//! A test run never touches the riff of the machine
//! (01M3MY2KWKBJCQ0BCNC6533RBW), and `RIFF_HOME` gives riff a home of
//! its own (01M3MY2KSV73WS8D902YCH2PRX).

use std::net::TcpListener;
use std::path::Path;
use std::process::Output;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Runs in a child of [`the_helper_keeps_the_riff_server_of_the_shell_out`]
/// only: riff runs commands that go to the server.
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
/// set to `server`, or unset.
fn run_child(server: Option<&str>) {
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "child_runs_riff", "--nocapture"])
        .env(CHILD, "1");
    match server {
        Some(server) => cmd.env("RIFF_SERVER", server),
        None => cmd.env_remove("RIFF_SERVER"),
    };
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("1 passed"), "{out:?}");
}

#[test]
fn the_helper_keeps_the_riff_server_of_the_shell_out() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", server.local_addr().unwrap());
    let calls = counting(server);
    run_child(Some(&url));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a riff under the helper called RIFF_SERVER of the shell"
    );
}

#[test]
fn the_helper_keeps_the_riff_of_the_machine_out() {
    assert_eq!(isolated::DEAD_SERVER, "http://127.0.0.1:9");
    // A real riff of this machine holds the port: the helper still sends
    // no call there, but this test cannot count them.
    let Ok(server) = TcpListener::bind("127.0.0.1:7878") else {
        eprintln!("skip: 127.0.0.1:7878 is in use");
        return;
    };
    let calls = counting(server);
    run_child(None);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a riff under the helper called the riff of the machine"
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
