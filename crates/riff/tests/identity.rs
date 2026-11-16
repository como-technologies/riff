//! The place rules against real git repositories and worktrees.

use std::path::Path;
use std::process::Command;

use riff::identity::place_in;
use riff_core::name::{SessionUri, Who};

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn repo(dir: &Path, remote: Option<&str>) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
    if let Some(url) = remote {
        git(dir, &["remote", "add", "origin", url]);
    }
}

fn name(dir: &Path) -> String {
    let who = Who::new("mike", Some("a6cf")).unwrap();
    SessionUri::new(who, place_in(dir, "pangolin").unwrap()).to_string()
}

#[test]
fn the_main_worktree_has_no_worktree_part() {
    let tmp = tempfile::tempdir().unwrap();
    repo(
        tmp.path(),
        Some("git@github.com:como-technologies/riff.git"),
    );
    assert_eq!(
        name(tmp.path()),
        "riff://mike@pangolin/como-technologies/riff?session=a6cf"
    );
}

#[test]
fn a_linked_worktree_adds_its_directory_name() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("riff");
    std::fs::create_dir(&main).unwrap();
    repo(&main, Some("https://github.com/como-technologies/riff.git"));
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            ".claude/worktrees/pr-23",
            "-b",
            "pr-23",
        ],
    );
    let worktree = main.join(".claude/worktrees/pr-23");
    assert_eq!(
        name(&worktree),
        "riff://mike@pangolin/como-technologies/riff?session=a6cf#pr-23"
    );
    // A subdirectory of the worktree gives the same name.
    let sub = worktree.join("src");
    std::fs::create_dir(&sub).unwrap();
    assert_eq!(name(&sub), name(&worktree));
}

#[test]
fn a_repository_without_a_remote_is_local() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("scratch");
    std::fs::create_dir(&main).unwrap();
    repo(&main, None);
    assert_eq!(
        name(&main),
        "riff://mike@pangolin/local/scratch?session=a6cf"
    );
}

#[test]
fn a_directory_outside_git_uses_a_dash() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("notes");
    std::fs::create_dir(&dir).unwrap();
    assert_eq!(name(&dir), "riff://mike@pangolin/-?session=a6cf#notes");
}

/// R100, 01M7GMV5ZYYF94ZKQ5ECQYTJXH: the origin of a Claude Code cloud
/// session, `http://local_proxy@127.0.0.1:PORT/PATH`, gives the place of
/// the session, as before. An origin with a token, also near that form,
/// gives no place: the error names `origin` and no token.
#[test]
fn a_cloud_session_keeps_its_place_and_an_origin_with_a_token_has_none() {
    let tmp = tempfile::tempdir().unwrap();
    repo(
        tmp.path(),
        Some("http://local_proxy@127.0.0.1:8080/git/como-technologies/riff"),
    );
    assert_eq!(
        name(tmp.path()),
        "riff://mike@pangolin/como-technologies/riff?session=a6cf"
    );
    for url in isolated::ORIGINS_WITH_TOKEN {
        git(tmp.path(), &["remote", "set-url", "origin", url]);
        let error = format!("{:#}", place_in(tmp.path(), "pangolin").unwrap_err());
        assert_eq!(error, riff::text::origin_with_user(tmp.path()), "{url}");
    }
}

/// 01M7GMV5ZYYF94ZKQ5ECQYTJXH: the fix that the refusal and the book give
/// works for each setup that riff refuses. In the top folder of the
/// clone, the test runs each command of [`riff::text::ORIGIN_FIX`], then
/// [`riff::text::ORIGIN_REMOVE`] for each line of
/// [`riff::text::ORIGIN_LIST`] with the token. Then riff takes `origin`.
/// The refusal and an `sh` block of the book hold each command.
#[test]
fn the_fix_of_the_refusal_gives_origin_a_url_that_riff_takes() {
    use riff::text::{ORIGIN_FIX, ORIGIN_LIST, ORIGIN_REMOVE};
    let book = crate::book::commands_of_part("development.md", "Run one item in a container");
    let refusal = riff::text::origin_with_user(Path::new("DIR"));
    for command in ORIGIN_FIX.iter().chain([&ORIGIN_LIST, &ORIGIN_REMOVE]) {
        assert!(book.iter().any(|line| line == command), "{command}");
        assert!(refusal.contains(&format!("`{command}`")), "{command}");
    }
    let sh = |dir: &Path, command: &str| {
        let out = Command::new("sh")
            .args(["-c", command])
            .current_dir(dir)
            .output()
            .unwrap();
        // A command with nothing to remove exits with 5.
        assert!(
            matches!(out.status.code(), Some(0 | 5)),
            "{command}: {out:?}"
        );
        String::from_utf8(out.stdout).unwrap()
    };
    let urls: Vec<[&str; 4]> = isolated::ORIGINS_WITH_TOKEN
        .iter()
        .map(|url| ["remote", "set-url", "origin", *url])
        .collect();
    let urls: Vec<[&[&str]; 1]> = urls.iter().map(|args| [&args[..]]).collect();
    let setups = isolated::ORIGIN_SETUPS_WITH_TOKEN
        .into_iter()
        .chain(urls.iter().map(|setup| &setup[..]));
    // The names of the lines that the fix removes: two rules, and one URL
    // in an include file.
    let mut removed = Vec::new();

    for setup in setups {
        let tmp = tempfile::tempdir().unwrap();
        repo(tmp.path(), Some(isolated::CLEAN_ORIGIN));
        for args in setup {
            git(tmp.path(), args);
        }
        let error = format!("{:#}", place_in(tmp.path(), "pangolin").unwrap_err());
        assert_eq!(error, riff::text::origin_with_user(tmp.path()), "{setup:?}");
        for command in ORIGIN_FIX {
            sh(tmp.path(), command);
        }
        for line in sh(tmp.path(), ORIGIN_LIST).lines() {
            if !line.contains("T0KEN") {
                continue;
            }
            let (file, rest) = line
                .strip_prefix("file:")
                .unwrap()
                .split_once('\t')
                .unwrap();
            let name = rest.split(' ').next().unwrap();
            let remove = ORIGIN_REMOVE
                .replace("FILE", &format!("'{file}'"))
                .replace("NAME", &format!("'{name}'"));
            sh(tmp.path(), &remove);
            removed.push(name.to_owned());
        }
        let taken = riff::identity::origin_urls(tmp.path()).unwrap().unwrap();
        let fixed = ["https://github.com/OWNER/REPO.git"];
        assert_eq!(taken.of(false), fixed, "{setup:?}");
        assert_eq!(taken.of(true), fixed, "{setup:?}");
        assert_eq!(
            name(tmp.path()),
            "riff://mike@pangolin/OWNER/REPO?session=a6cf"
        );
    }
    assert_eq!(
        removed,
        [
            "url.https://T0KEN@github.com/.insteadof",
            "url.https://T0KEN@github.com/.pushinsteadof",
            "remote.origin.url",
        ]
    );
}
