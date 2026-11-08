//! The helper that clears the config of git from the environment of a test
//! (01M6FYHS06W0Y98GQTQY6Q37F0). This test binary changes the environment
//! in one test only.

use std::process::Command;

/// What git gives for `commit.gpgsign` in a process of this test: the
/// config of the machine is off, so only `GIT_CONFIG_*` can set it.
fn gpgsign() -> String {
    let out = Command::new("git")
        .args(["config", "--get", "commit.gpgsign"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[test]
fn the_helper_clears_the_config_of_git_from_the_environment_of_the_test() {
    // The keys of a session container go first, so only the keys of the
    // test stay (01M6XWACAZH79NK727KMC3AQ0D).
    isolated::clear_git_config_env();
    // SAFETY: this test binary changes the environment only here.
    unsafe {
        std::env::set_var("GIT_CONFIG_COUNT", "2");
        std::env::set_var("GIT_CONFIG_KEY_0", "commit.gpgsign");
        std::env::set_var("GIT_CONFIG_VALUE_0", "false");
        std::env::set_var("GIT_CONFIG_KEY_1", "user.name");
        std::env::set_var("GIT_CONFIG_VALUE_1", "riff");
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    assert_eq!(
        gpgsign(),
        "false",
        "the setup is real: a child of the test gets the variables"
    );

    let mut cleared = isolated::clear_git_config_env();
    cleared.sort();

    let names: Vec<_> = cleared.iter().map(|n| n.to_string_lossy()).collect();
    assert_eq!(
        names,
        [
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_KEY_0",
            "GIT_CONFIG_KEY_1",
            "GIT_CONFIG_VALUE_0",
            "GIT_CONFIG_VALUE_1",
        ]
    );
    assert_eq!(
        gpgsign(),
        "",
        "a child of the test gets no config from the environment"
    );
    assert!(
        std::env::vars_os().all(|(name, _)| !isolated::git_config_var(&name)),
        "the process has none"
    );
    assert_eq!(
        std::env::var("GIT_CONFIG_NOSYSTEM").as_deref(),
        Ok("1"),
        "the other variables of git stay"
    );
}
