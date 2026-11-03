//! A test of code that runs git in the test process runs again, alone, with
//! no config of git in its environment (01M6FYHS06W0Y98GQTQY6Q37F0).

use std::process::Command;

/// The probe: after the helper, the process has no `GIT_CONFIG_COUNT`,
/// `GIT_CONFIG_KEY_N` or `GIT_CONFIG_VALUE_N`, and a child of the test
/// gets no `commit.gpgsign` from the environment.
#[test]
fn a_test_that_runs_again_has_no_git_config() {
    if !isolated::rerun_without_git_config(
        module_path!(),
        "a_test_that_runs_again_has_no_git_config",
    ) {
        return;
    }
    let left: Vec<_> = std::env::vars_os()
        .map(|(name, _)| name)
        .filter(|name| isolated::git_config_var(name))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    let out = Command::new("git")
        .args(["config", "--get", "commit.gpgsign"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "");
}

/// The process of the probe has the variables of a container of riff, as
/// a worker has them. The probe passes only when its second run has none.
#[test]
fn a_probe_in_a_container_of_riff_passes_after_the_second_run() {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "a_test_that_runs_again_has_no_git_config",
            "--exact",
            "--test-threads=1",
        ])
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "commit.gpgsign")
        .env("GIT_CONFIG_VALUE_0", "false")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success() && text.contains("1 passed"), "{out:?}");
}

/// A wrong name runs no test, and a run of no test is no pass.
#[test]
#[should_panic(expected = "failed when it ran again with no git config")]
fn a_second_run_that_runs_no_test_fails() {
    isolated::rerun_without_git_config(module_path!(), "no_such_test");
}
