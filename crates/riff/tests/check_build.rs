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
echo "$PWD $*" >> "$(dirname "$0")/just.log"
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
    assert_eq!(log, format!("{} static\n", check.display()));

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
