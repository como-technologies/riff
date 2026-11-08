//! The helper gives a test no git config and no `gh` sign-in of a session
//! container with the token of an App (01M6XWAC88HABQRKPG9MM1SCY0). This
//! test binary changes the environment in one test only.

use std::path::Path;
use std::process::{Command, Output};

/// The text of the output of `cmd`, in `dir`.
fn stdout(cmd: &mut Command, dir: &Path) -> String {
    let out = cmd.current_dir(dir).output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// `gh auth token` of `cmd`, or `None` when the machine has no `gh`.
fn gh_token(cmd: &mut Command) -> Option<Output> {
    match cmd.args(["auth", "token"]).output() {
        Ok(out) => Some(out),
        Err(e) => {
            eprintln!("skip: no gh to read the sign-in: {e}");
            None
        }
    }
}

#[test]
fn the_helper_gives_no_git_config_and_no_gh_sign_in_of_a_session_container() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let session: Vec<_> = isolated::test_env(root)
        .into_iter()
        .filter(|(name, _)| isolated::git_config_var(name.as_ref()) || name == "GH_CONFIG_DIR")
        .collect();
    isolated::clear_git_config_env();
    // SAFETY: this test binary changes the environment only here.
    unsafe {
        for (name, value) in &session {
            std::env::set_var(name, value);
        }
    }
    let outside = isolated::outside_git();
    let git = |cmd: &mut Command| {
        stdout(
            cmd.args(["config", "--get", "user.name"])
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1"),
            outside.path(),
        )
    };
    assert_eq!(
        git(&mut Command::new("git")),
        "riff worker 1a2b3c4d",
        "the setup is real: a child of the test gets the git config"
    );
    let env = isolated::Isolated::new();
    assert_eq!(
        git(&mut env.command("git")),
        "",
        "a child of the helper gets none"
    );
    assert_eq!(
        stdout(env.command("git").args(["config", "--list"]), env.path()),
        "",
        "git of the helper has no config at all"
    );

    let Some(signed) = gh_token(&mut Command::new("gh")) else {
        return;
    };
    assert!(
        signed.status.success() && !signed.stdout.trim_ascii().is_empty(),
        "the setup is real: the gh of the test has a sign-in: {signed:?}"
    );
    let off = gh_token(&mut env.command("gh")).unwrap();
    assert!(
        !off.status.success() && off.stdout.trim_ascii().is_empty(),
        "the gh of the helper has no sign-in: {off:?}"
    );
}
