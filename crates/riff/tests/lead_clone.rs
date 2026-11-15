//! The lead clones in its container, and a clone that an older riff made
//! on the host goes at the start of a new lead, with no git (#2139,
//! #2140).

use std::path::Path;
use std::process::Command;

use riff::runtime::Source;
use riff::session_clone::{Checkout, Plan, VAR};

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

/// The facts of the lead of `o/r` in the data folder `data`.
fn facts(data: &Path) -> riff::run::Facts {
    riff::run::Facts {
        session: "1a2b3c4d-0000-4000-8000-000000000000".into(),
        item: riff::lead::ITEM.into(),
        image: "img".into(),
        home: "/home/mike".into(),
        user: "mike".into(),
        clone: data.join("leads/o-r/clone"),
        repo: "o/r".into(),
        base: "main".into(),
        checkout: Checkout::Default,
        main: "/src/r/.git".into(),
        claude: data.join("leads/o-r/claude"),
        sign_in: riff::run::ClaudeSignIn::Token,
        gitconfig: None,
        setup: Default::default(),
    }
}

/// #2139: the spec of the lead has its clone as an empty volume of the
/// container, and the plan of the clone on the default branch. Each bind
/// with write of a path of the host is a folder with no git tree: the
/// Claude folder. The start again of the lead has the same plan.
#[test]
fn the_lead_clones_in_its_container_on_the_default_branch() {
    let dir = tempfile::tempdir().unwrap();
    let facts = facts(dir.path());
    for spec in [
        riff::lead::spec(&facts, "o/r", "Start."),
        riff::lead::spec_again(&facts, "o/r", "Again."),
    ] {
        let clone = spec.binds.iter().find(|b| b.target == facts.clone).unwrap();
        assert_eq!((&clone.source, clone.write), (&Source::Empty, true));
        let writes: Vec<_> = spec
            .binds
            .iter()
            .filter_map(|b| match &b.source {
                Source::Path(path) if b.write => Some(path.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(writes, std::slice::from_ref(&facts.claude));
        let plan = spec
            .env
            .iter()
            .find_map(|e| match e {
                riff::runtime::Env::Value(name, plan) if name == VAR => Plan::of_var(plan).ok(),
                _ => None,
            })
            .unwrap();
        assert_eq!(plan.path, facts.clone);
        assert_eq!(plan.origin, "https://github.com/o/r.git");
        assert_eq!(plan.main, Path::new("/src/r/.git"));
        assert_eq!(plan.base, "main");
        assert_eq!(plan.checkout, Checkout::Default);
        let args: Vec<String> = riff::podman::terminal_args(&spec)
            .unwrap()
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        let volume = format!("type=volume,destination={},U=true", facts.clone.display());
        assert!(args.contains(&volume), "{args:?}");
    }
}

/// #2140, 01M7EBBVE6WYKT5JDVASFCYPQQ: an older riff left the clone of the
/// lead on the host, with a change and a git config that names a program
/// for `core.fsmonitor`, `credential.helper` and `core.sshCommand`. The
/// start of a new lead removes it, the Claude folder stays, and the
/// program does not run: the host runs no git in that clone.
#[test]
fn the_clone_of_an_older_riff_goes_and_no_program_of_its_config_runs() {
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin");
    std::fs::create_dir(&origin).unwrap();
    git(&origin, &["init", "-q"]);
    std::fs::write(origin.join("a"), "one").unwrap();
    git(&origin, &["add", "a"]);
    git(&origin, &["commit", "-q", "-m", "one"]);
    let folder = dir.path().join("leads/o-r");
    std::fs::create_dir_all(folder.join("claude")).unwrap();
    let clone = folder.join("clone");
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--",
            &origin.to_string_lossy(),
            &clone.to_string_lossy(),
        ],
    );
    std::fs::write(clone.join("a"), "not committed").unwrap();
    let mark = dir.path().join("mark");
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

    let stays = riff::lead::remove_host_clone(&folder, riff::compile_cache::remove).unwrap();
    assert!(stays.is_empty(), "{stays:?}");
    assert!(!clone.exists());
    assert!(riff::lead::old_clones(&folder).is_empty());
    assert!(folder.join("claude").exists(), "the Claude folder stays");
    assert!(
        !mark.exists(),
        "the program of the old clone ran: {}",
        std::fs::read_to_string(&mark).unwrap_or_default()
    );
}
