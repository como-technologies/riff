//! Each start of a new lead gives the lead a new clone, and the host
//! runs no git in the old clone (01M7E7A8QM8SRPQN3G1BXVJW1R, #2140).

use std::path::Path;
use std::process::Command;

use riff::lead::{Fresh, fresh_clone};

/// Runs `git ARGS` in `dir`, and gives its output.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Commits a change of the file `a` in `dir`.
fn commit(dir: &Path, words: &str) -> String {
    std::fs::write(dir.join("a"), words).unwrap();
    git(dir, &["commit", "-q", "-a", "-m", words]);
    git(dir, &["rev-parse", "HEAD"])
}

/// An `origin` with one commit, and a main clone of it.
fn origin_and_main(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let origin = dir.join("origin");
    std::fs::create_dir(&origin).unwrap();
    git(&origin, &["init", "-q"]);
    std::fs::write(origin.join("a"), "one").unwrap();
    git(&origin, &["add", "a"]);
    git(&origin, &["commit", "-q", "-m", "one"]);
    let main = dir.join("main");
    git(
        dir,
        &[
            "clone",
            "-q",
            "--",
            &origin.to_string_lossy(),
            &main.to_string_lossy(),
        ],
    );
    (origin, main)
}

/// The clone of the lead of `o/r` in the data folder `data`.
fn lead_clone(data: &Path) -> std::path::PathBuf {
    data.join("leads/o-r/clone")
}

/// Writes in the git config of the clone `clone` a program for
/// `core.fsmonitor`, `credential.helper` and `core.sshCommand`, as a lead
/// can leave it (#2140). The program writes the file `mark`.
fn hostile_config(clone: &Path, mark: &Path) {
    let program = isolated::write_program(
        mark.with_extension("sh"),
        format!("#!/bin/sh\necho \"$0 $*\" >> '{}'\n", mark.display()),
    );
    let program = program.display();
    let config = clone.join(".git/config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&format!(
        "[core]\n\tfsmonitor = {program}\n\tsshCommand = {program}\n\
         [credential]\n\thelper = !{program}\n"
    ));
    std::fs::write(&config, text).unwrap();
}

/// 01M7E7A8QM8SRPQN3G1BXVJW1R: the first start makes the clone in
/// `new-clones/lead-OWNER-REPO`, then moves it in.
#[test]
fn the_first_start_makes_the_clone_of_the_lead_in_a_folder_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let (origin, main) = origin_and_main(dir.path());
    let clone = lead_clone(dir.path());
    assert_eq!(fresh_clone(&main, &clone).unwrap(), Fresh::Made);
    assert_eq!(
        git(&clone, &["rev-parse", "HEAD"]),
        git(&origin, &["rev-parse", "HEAD"])
    );
    assert_eq!(git(&clone, &["branch", "--show-current"]), "main");
    let new = riff::lead::new_clone_of(&clone).unwrap();
    assert_eq!(new, dir.path().join("new-clones/lead-o-r"));
    assert!(!new.exists(), "{}", new.display());
}

/// 01M7E7A8QM8SRPQN3G1BXVJW1R: each later start gives the lead a new
/// clone at the new commit of origin. The old clone goes, with each
/// change in it: a change that is not committed, a commit, a branch and
/// an untracked file.
#[test]
fn a_second_start_gives_the_lead_a_new_clone_at_the_new_commit_of_origin() {
    let dir = tempfile::tempdir().unwrap();
    let (origin, main) = origin_and_main(dir.path());
    let clone = lead_clone(dir.path());
    fresh_clone(&main, &clone).unwrap();
    commit(&clone, "mine");
    git(&clone, &["switch", "-q", "-c", "alone"]);
    std::fs::write(clone.join("a"), "not committed").unwrap();
    std::fs::write(clone.join("b"), "untracked").unwrap();

    let new = commit(&origin, "two");
    assert_eq!(fresh_clone(&main, &clone).unwrap(), Fresh::New);
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), new);
    assert_eq!(git(&clone, &["branch", "--show-current"]), "main");
    assert_eq!(git(&clone, &["status", "--porcelain"]), "");
    assert_eq!(std::fs::read_to_string(clone.join("a")).unwrap(), "two");
    assert!(!clone.join("b").exists());
    assert!(!clone.with_file_name(riff::lead::OLD_CLONE).exists());
    assert!(!riff::lead::new_clone_of(&clone).unwrap().exists());
}

/// #2140, 01M7E7A8QM8SRPQN3G1BXVJW1R: the old clone has a git config
/// that names a program for `core.fsmonitor`, `credential.helper` and
/// `core.sshCommand`. The start gives the lead a new clone, and the
/// program does not run: the host runs no git in the old clone.
#[test]
fn a_start_runs_no_program_of_the_git_config_of_the_old_clone() {
    let dir = tempfile::tempdir().unwrap();
    let (origin, main) = origin_and_main(dir.path());
    let clone = lead_clone(dir.path());
    fresh_clone(&main, &clone).unwrap();
    let mark = dir.path().join("mark");
    hostile_config(&clone, &mark);

    let new = commit(&origin, "two");
    assert_eq!(fresh_clone(&main, &clone).unwrap(), Fresh::New);
    assert!(
        !mark.exists(),
        "the program of the old clone ran: {}",
        std::fs::read_to_string(&mark).unwrap_or_default()
    );
    let config = std::fs::read_to_string(clone.join(".git/config")).unwrap();
    assert!(!config.contains("fsmonitor"), "{config}");
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), new);
}

/// 01M7E7A8QM8SRPQN3G1BXVJW1R: when the new clone fails, the old clone
/// stays as it is, and the text says why. With no old clone, the start
/// fails.
#[test]
fn when_the_new_clone_fails_the_old_clone_stays_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let (origin, main) = origin_and_main(dir.path());
    let clone = lead_clone(dir.path());
    fresh_clone(&main, &clone).unwrap();
    let old = git(&clone, &["rev-parse", "HEAD"]);
    std::fs::write(clone.join("b"), "mine").unwrap();

    // origin is gone.
    std::fs::rename(&origin, dir.path().join("gone")).unwrap();
    let Fresh::Kept(why) = fresh_clone(&main, &clone).unwrap() else {
        panic!("the old clone did not stay");
    };
    assert!(why.contains("git clone"), "{why}");
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), old);
    assert_eq!(std::fs::read_to_string(clone.join("b")).unwrap(), "mine");
    assert!(!riff::lead::new_clone_of(&clone).unwrap().exists());
    let note = riff::text::lead_clone_kept(&clone, &why);
    assert!(note.contains(&*clone.to_string_lossy()), "{note}");

    let none = dir.path().join("leads/o-other/clone");
    assert!(fresh_clone(&main, &none).is_err());
    assert!(!none.exists());
}

/// 01M7GMV5ZYYF94ZKQ5ECQYTJXH: the origin of the main clone has a token
/// in its URL. The new clone of the lead refuses it before each git with
/// the URL: the old clone stays, and its note names `origin` and no
/// token. With no old clone, the start fails with the same error.
#[test]
fn the_clone_of_the_lead_refuses_an_origin_with_a_token() {
    let dir = tempfile::tempdir().unwrap();
    let (_, main) = origin_and_main(dir.path());
    let clone = lead_clone(dir.path());
    fresh_clone(&main, &clone).unwrap();
    let old = git(&clone, &["rev-parse", "HEAD"]);
    let refused = riff::text::origin_with_user(&main);

    for url in isolated::ORIGINS_WITH_TOKEN {
        git(&main, &["remote", "set-url", "origin", url]);
        let Fresh::Kept(why) = fresh_clone(&main, &clone).unwrap() else {
            panic!("the old clone did not stay");
        };
        assert_eq!(why, refused);
        assert!(!riff::text::lead_clone_kept(&clone, &why).contains("T0KEN"));
        assert_eq!(git(&clone, &["rev-parse", "HEAD"]), old);
        let none = dir.path().join("leads/o-other/clone");
        let error = format!("{:#}", fresh_clone(&main, &none).unwrap_err());
        assert_eq!(error, refused);
        assert!(!none.exists());
    }
}

/// 01M6QB89JJEM9Z1V3RVZYHKYT0: a host that runs no wave of the repository
/// still finds the main clone of the lead: the clone links to it.
#[test]
fn the_clone_of_a_lead_links_to_the_main_clone_that_made_it() {
    let dir = tempfile::tempdir().unwrap();
    let (_, main) = origin_and_main(dir.path());
    let clone = dir.path().join("lead/clone");
    fresh_clone(&main, &clone).unwrap();

    let linked = riff::lead::linked_main(&clone).unwrap();
    assert_eq!(linked, std::fs::canonicalize(&main).unwrap());
    // It is the main clone that a start takes: its top, and its git dir.
    let (top, common) = riff::run::main_clone(&linked).unwrap();
    assert_eq!(top, linked);
    assert_eq!(common, linked.join(".git"));
}

/// 01M6QB89JJEM9Z1V3RVZYHKYT0: a clone with no link names no main clone.
/// A clone that `git clone --dissociate` made is that case.
#[test]
fn a_clone_with_no_link_to_a_main_clone_names_none() {
    let dir = tempfile::tempdir().unwrap();
    let (origin, main) = origin_and_main(dir.path());
    let clone = dir.path().join("lead/clone");
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--reference",
            &main.to_string_lossy(),
            "--dissociate",
            "--",
            &origin.to_string_lossy(),
            &clone.to_string_lossy(),
        ],
    );
    let why = riff::lead::linked_main(&clone).unwrap_err();
    assert!(format!("{why:#}").contains("alternates"), "{why:#}");
}
