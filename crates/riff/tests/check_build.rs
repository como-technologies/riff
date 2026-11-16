//! `riff build` builds riff of the default branch in the check clone
//! (01M5084K79M5XJT3MP8FFVQKDX, 01M56P380A5PXGMVJVCSD8KBRG). Each test
//! runs the real `riff` binary with a real git, a bare `origin` and a
//! fake `just`.

use std::path::Path;
use std::process::{Command, Output};

use isolated::Isolated;

/// `just static` writes its folder and its words to `just.log` beside
/// it, and makes the riff of the build.
const FAKE_JUST: &str = r#"#!/bin/sh
echo "$(pwd -P) $*" >> "$(dirname "$0")/just.log"
mkdir -p target/static/release
git rev-parse HEAD > target/static/release/riff
"#;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A bare `origin`, and a clone of it in `here` with one commit on
/// `main`. The `riff.toml` of the commit has the table `[release]`.
fn repo(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    repo_with(root, "[release]\n")
}

/// Like [`repo`], with `config` as the text of `riff.toml`.
fn repo_with(root: &Path, config: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let origin = root.join("origin.git");
    let here = root.join("here");
    git(root, &["init", "-q", "--bare", "origin.git"]);
    git(root, &["clone", "-q", "origin.git", "here"]);
    std::fs::write(here.join("riff.toml"), config).unwrap();
    git(&here, &["add", "riff.toml"]);
    commit(&here, "one");
    (origin, here)
}

/// Commits a new file `name` in `clone`, and pushes it to `main`.
fn commit(clone: &Path, name: &str) -> String {
    std::fs::write(clone.join(name), name).unwrap();
    git(clone, &["add", name]);
    git(clone, &["commit", "-q", "-m", name]);
    git(clone, &["push", "-q", "origin", "HEAD:main"]);
    git(clone, &["rev-parse", "HEAD"])
}

fn build(env: &Isolated, here: &Path, just: &Path) -> Output {
    env.riff()
        .arg("build")
        .arg("--just")
        .arg(just)
        .current_dir(here)
        .output()
        .unwrap()
}

#[test]
fn riff_build_clones_fetches_and_builds_the_default_branch() {
    let env = Isolated::new();
    let root = isolated::outside_git();
    let (_origin, here) = repo(root.path());
    let fake = isolated::outside_git();
    let just = isolated::write_program(fake.path().join("just"), FAKE_JUST);
    let check = env.home().join(".cache/riff/check");
    let riff = check.join("target/static/release/riff");

    let out = build(&env, &here, &just);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        riff.display().to_string()
    );
    let first = git(&here, &["rev-parse", "HEAD"]);
    assert_eq!(git(&check, &["rev-parse", "HEAD"]), first);
    assert_eq!(std::fs::read_to_string(&riff).unwrap().trim(), first);
    let log = std::fs::read_to_string(fake.path().join("just.log")).unwrap();
    // The real path: on macOS, `/var` is a link to `/private/var`.
    let real = std::fs::canonicalize(&check).unwrap();
    assert_eq!(log, format!("{} static\n", real.display()));

    // A new commit on main: the next build fetches it, and builds it.
    let second = commit(&here, "two");
    let out = build(&env, &here, &just);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(git(&check, &["rev-parse", "HEAD"]), second);
    assert_eq!(std::fs::read_to_string(&riff).unwrap().trim(), second);
}

#[test]
fn riff_build_fails_when_just_static_fails() {
    let env = Isolated::new();
    let root = isolated::outside_git();
    let (_origin, here) = repo(root.path());
    let fake = isolated::outside_git();
    let just = isolated::write_program(fake.path().join("just"), "#!/bin/sh\nexit 3\n");

    let out = build(&env, &here, &just);
    assert!(!out.status.success(), "{out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("just static in"), "{err}");
    assert!(out.stdout.is_empty(), "{out:?}");
}

/// `riff build` in `here` as [`build`], with the programs of `bin` first
/// in `PATH`.
fn build_with(env: &Isolated, here: &Path, just: &Path, bin: &Path) -> Output {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let path =
        std::env::join_paths(std::iter::once(bin.to_owned()).chain(std::env::split_paths(&path)));
    env.riff()
        .arg("build")
        .arg("--just")
        .arg(just)
        .current_dir(here)
        .env("PATH", path.unwrap())
        .output()
        .unwrap()
}

/// The calls of the log of [`isolated::logged_git`] that talk to a
/// remote: a clone, a fetch, an `ls-remote`, a push or a pull.
fn remote_calls(log: &Path) -> Vec<String> {
    let calls = std::fs::read_to_string(log).unwrap_or_default();
    calls
        .lines()
        .filter(|call| {
            call.split(' ')
                .any(|word| ["clone", "fetch", "ls-remote", "push", "pull"].contains(&word))
        })
        .map(str::to_owned)
        .collect()
}

/// 01M7GMV5ZYYF94ZKQ5ECQYTJXH: `riff build` refuses an origin with a token
/// in its URL, in each form of [`isolated::ORIGINS_WITH_TOKEN`]. It exits
/// with 1, starts no git that talks to the origin, and makes no check
/// clone. Its error names `origin`, and no token.
#[test]
fn riff_build_refuses_an_origin_with_a_token() {
    let env = Isolated::new();
    let root = isolated::outside_git();
    let (_origin, here) = repo(root.path());
    let fake = isolated::outside_git();
    let just = isolated::write_program(fake.path().join("just"), FAKE_JUST);
    let log = isolated::logged_git(fake.path());
    let refused = riff::text::origin_with_user(Path::new("DIR"));
    let (head, tail) = refused.split_once("DIR").unwrap();

    for url in isolated::ORIGINS_WITH_TOKEN {
        git(&here, &["remote", "set-url", "origin", url]);
        let out = build_with(&env, &here, &just, fake.path());
        assert_eq!(out.status.code(), Some(1), "{url}: {out:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.starts_with(&format!("Error: {head}")), "{err}");
        assert!(err.trim_end().ends_with(tail), "{err}");
        assert!(!err.contains("T0KEN"), "{err}");
        assert!(!String::from_utf8_lossy(&out.stdout).contains("T0KEN"));
    }
    assert!(
        std::fs::read_to_string(&log)
            .unwrap()
            .contains("config -z --list")
    );
    assert!(!std::fs::read_to_string(&log).unwrap().contains("T0KEN"));
    assert_eq!(remote_calls(&log), Vec::<String>::new());
    assert!(!env.home().join(".cache/riff/check").exists());
    assert!(!fake.path().join("just.log").exists());
}

/// 01M7GMV5ZYYF94ZKQ5ECQYTJXH: `riff build` refuses an origin with a token
/// in a URL past its first URL, in each setup of
/// [`isolated::ORIGIN_SETUPS_WITH_TOKEN`]: a line break in the URL, an
/// empty URL with a push URL that holds a token, a second URL, a rule
/// `insteadOf` of git, and a file that the git config includes. It exits
/// with 1, and starts no git that talks to the origin. No output and no
/// word of a git holds the token.
#[test]
fn riff_build_refuses_a_token_in_each_url_of_origin() {
    let env = Isolated::new();
    let fake = isolated::outside_git();
    let just = isolated::write_program(fake.path().join("just"), FAKE_JUST);
    let log = isolated::logged_git(fake.path());
    let refused = riff::text::origin_with_user(Path::new("DIR"));
    let (head, tail) = refused.split_once("DIR").unwrap();

    for setup in isolated::ORIGIN_SETUPS_WITH_TOKEN {
        let root = isolated::outside_git();
        let (_origin, here) = repo(root.path());
        git(
            &here,
            &["remote", "set-url", "origin", isolated::CLEAN_ORIGIN],
        );
        for args in setup {
            git(&here, args);
        }
        let out = build_with(&env, &here, &just, fake.path());
        assert_eq!(out.status.code(), Some(1), "{setup:?}: {out:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.starts_with(&format!("Error: {head}")),
            "{setup:?}: {err}"
        );
        assert!(err.trim_end().ends_with(tail), "{setup:?}: {err}");
        assert!(!err.contains("T0KEN"), "{err}");
        assert!(!String::from_utf8_lossy(&out.stdout).contains("T0KEN"));
    }
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(calls.contains("config -z --list"), "{calls}");
    assert!(!calls.contains("T0KEN"), "{calls}");
    assert_eq!(remote_calls(&log), Vec::<String>::new());
    assert!(!env.home().join(".cache/riff/check").exists());
    assert!(!fake.path().join("just.log").exists());
}

/// 01M7GMV5ZYYF94ZKQ5ECQYTJXH: when git cannot read the URLs of `origin`,
/// the error of `riff build` gives the output of git with no user info. A
/// rule `insteadOf` with no value has a token in its name, and git names
/// it when it fails. `riff build` exits with 1, and starts no git that
/// talks to the origin.
#[test]
fn riff_build_shows_no_user_info_of_a_rule_that_git_cannot_read() {
    let env = Isolated::new();
    let root = isolated::outside_git();
    let (_origin, here) = repo(root.path());
    git(
        &here,
        &["remote", "set-url", "origin", isolated::CLEAN_ORIGIN],
    );
    let config = here.join(".git/config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str("[url \"https://U:T0KEN@127.0.0.1:1/\"]\n\tinsteadOf\n");
    std::fs::write(&config, text).unwrap();
    let fake = isolated::outside_git();
    let just = isolated::write_program(fake.path().join("just"), FAKE_JUST);
    let log = isolated::logged_git(fake.path());

    let out = build_with(&env, &here, &just, fake.path());
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("`git remote get-url --all origin` failed"),
        "{err}"
    );
    assert!(
        err.contains("missing value for 'url.https://127.0.0.1:1/.insteadof'"),
        "{err}"
    );
    assert!(!err.contains("T0KEN"), "{err}");
    assert!(!String::from_utf8_lossy(&out.stdout).contains("T0KEN"));
    assert_eq!(remote_calls(&log), Vec::<String>::new());
    assert!(!env.home().join(".cache/riff/check").exists());
}

/// 01M7GMV5ZYYF94ZKQ5ECQYTJXH: an origin with no user, with the user
/// `git` of SSH, or with the user `local_proxy` of a Claude Code cloud
/// session works as before: `riff build` clones it, and builds. The `git`
/// of the test fetches the bare origin in place of that URL, and gives
/// each other call to git as it is.
#[test]
fn riff_build_takes_an_origin_with_no_secret() {
    let env = Isolated::new();
    let root = isolated::outside_git();
    let (origin, here) = repo(root.path());
    let fake = isolated::outside_git();
    let just = isolated::write_program(fake.path().join("just"), FAKE_JUST);
    let urls = [
        "https://github.com/o/r.git",
        "git@github.com:o/r.git",
        "ssh://git@github.com/o/r.git",
        "http://local_proxy@127.0.0.1:8080/git/o/r",
    ];
    let real = isolated::git_of_path(fake.path());
    let real = real.display();
    let instead: String = urls
        .iter()
        .map(|url| format!(" -c 'url.{}.insteadOf={url}'", origin.display()))
        .collect();
    isolated::write_program(
        fake.path().join("git"),
        format!(
            "#!/bin/sh\ncase \" $* \" in *\" clone \"*|*\" fetch \"*) exec '{real}'{instead} \"$@\" ;; esac\n\
             exec '{real}' \"$@\"\n"
        ),
    );
    let check = env.home().join(".cache/riff/check");

    for url in urls {
        git(&here, &["remote", "set-url", "origin", url]);
        let _ = std::fs::remove_dir_all(&check);
        let out = build_with(&env, &here, &just, fake.path());
        assert!(out.status.success(), "{url}: {out:?}");
        assert_eq!(git(&check, &["remote", "get-url", "origin"]), url);
        assert_eq!(
            git(&check, &["rev-parse", "HEAD"]),
            git(&here, &["rev-parse", "HEAD"])
        );
    }
}

/// 01M5SMPX8NM75P8E38DGQ17W7B: a repository with no `[release]` in its
/// `riff.toml` has no `riff build`. The command refuses with one line, and
/// it makes no check clone and runs no `just`.
#[test]
fn riff_build_refuses_a_repository_with_no_release_table() {
    for config in ["", "[session]\nimage = \"x\"\n"] {
        let env = Isolated::new();
        let root = isolated::outside_git();
        let (_origin, here) = repo_with(root.path(), config);
        let fake = isolated::outside_git();
        let just = isolated::write_program(fake.path().join("just"), FAKE_JUST);

        let out = build(&env, &here, &just);
        assert!(!out.status.success(), "{out:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains(riff::text::NO_RELEASE), "{err}");
        assert_eq!(err.trim().lines().count(), 1, "{err}");
        assert!(out.stdout.is_empty(), "{out:?}");
        assert!(!env.home().join(".cache/riff/check").exists());
        assert!(!fake.path().join("just.log").exists());
    }
}
