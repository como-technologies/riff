//! An environment ends each process of it when it drops: also a child
//! whose parent ended, and a process that ignores a SIGTERM
//! (01M77HHYZWY1WWK1T6W6KX2SHV). A process of another environment stays.
//! The run of a guard finds each process that a test left, and ends it
//! (01M77HHZ2FP07ES4ZYG5KYVZ7E).

use std::path::Path;
use std::time::Duration;

use isolated::{Isolated, MARK, Span};

/// A daemon as a fake `systemctl` starts it: in the background, so its
/// parent ends at once. It ignores a SIGTERM, and it starts a child.
const DAEMON: &str = "sh -c \"trap '' TERM; sleep 600 & sleep 600\" > /dev/null 2>&1 &";

/// Waits until the environment of the dir `dir` has `count` processes,
/// and gives their commands.
fn wait_for_processes(dir: &Path, count: usize) -> Vec<String> {
    let span = Span::start();
    loop {
        let found = isolated::marked(MARK, dir);
        // The copy of the first sh, before it runs the second one, has the
        // redirect of its command.
        if found.len() == count && found.iter().all(|p| !p.command.contains("/dev/null")) {
            return found.into_iter().map(|p| p.command).collect();
        }
        assert!(
            span.within(Duration::from_secs(10)),
            "not {count} processes: {found:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn each_command_has_the_mark_with_the_dir_of_its_environment() {
    let env = Isolated::new();
    let out = env
        .command("sh")
        .args(["-c", "printf %s \"$ISOLATED_DIR\""])
        .output()
        .unwrap();
    assert_eq!(
        Path::new(&String::from_utf8(out.stdout).unwrap()),
        env.path()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn the_drop_ends_each_process_of_the_environment_and_no_other() {
    let env = Isolated::new();
    let other = Isolated::new();
    for env in [&env, &other] {
        let status = env.command("sh").args(["-c", DAEMON]).status().unwrap();
        assert!(status.success());
    }
    let (dir, other_dir) = (env.path().to_owned(), other.path().to_owned());
    let mut commands = wait_for_processes(&dir, 3);
    commands.sort();
    assert_eq!(
        commands,
        [
            "sh -c trap '' TERM; sleep 600 & sleep 600",
            "sleep 600",
            "sleep 600"
        ]
    );
    wait_for_processes(&other_dir, 3);

    drop(env);
    assert_eq!(isolated::marked(MARK, &dir), []);
    assert_eq!(isolated::marked(MARK, &other_dir).len(), 3);
    drop(other);
    assert_eq!(isolated::marked(MARK, &other_dir), []);
}

/// In a run of the guard, it leaves a `sleep` that no environment
/// started. In each other run, it does nothing.
#[test]
fn a_test_that_leaves_a_sleep_in_a_run_of_the_guard() {
    if std::env::var_os(isolated::RUN).is_none() {
        return;
    }
    let status = std::process::Command::new("sh")
        .args(["-c", "sleep 600 > /dev/null 2>&1 &"])
        .status()
        .unwrap();
    assert!(status.success());
}

/// It starts a daemon in an environment, and ends with the drop of the
/// environment.
#[test]
fn a_test_whose_environment_ends_its_daemon() {
    let env = Isolated::new();
    let status = env.command("sh").args(["-c", DAEMON]).status().unwrap();
    assert!(status.success());
}

#[cfg(target_os = "linux")]
#[test]
fn the_guard_finds_the_process_that_a_test_left_and_ends_it() {
    let exe = std::env::current_exe().unwrap();
    let left = isolated::left_after(
        &exe,
        &[
            "a_test_that_leaves_a_sleep_in_a_run_of_the_guard",
            "a_test_whose_environment_ends_its_daemon",
        ],
    );
    assert_eq!(left.len(), 1, "{left:?}");
    // The sleep ended: it has no environment, or it is gone.
    let environ = std::fs::read(format!("/proc/{}/environ", left[0].pid)).unwrap_or_default();
    assert!(environ.is_empty());

    let none = isolated::left_after(&exe, &["a_test_whose_environment_ends_its_daemon"]);
    assert_eq!(none, []);
}

#[test]
#[should_panic(expected = "did not pass")]
fn the_guard_fails_on_a_test_that_did_not_run() {
    let exe = std::env::current_exe().unwrap();
    isolated::left_after(&exe, &["a_test_with_another_name_now"]);
}
