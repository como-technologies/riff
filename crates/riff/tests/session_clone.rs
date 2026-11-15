//! Each session clones and pushes in its container (#2139). The host binds
//! no git tree with write into a container: the clone of a session is an
//! empty volume of its container, and the relay clones there before
//! `claude` starts. The work of a session is what it pushed, and the next
//! session of the item goes on from the branch on `origin`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use riff::run::{ClaudeSignIn, Facts};
use riff::session_clone::{Checkout, Plan, VAR};
use riff::wave::Work;

/// Runs `git ARGS` in `dir` with an identity and no signing, and gives
/// its output.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A bare `origin` that a `file://` URL names, as a URL of GitHub does,
/// with one commit on `main`, and a main clone of it, in `root`. It gives
/// the URL and the main clone.
fn origin_and_main(root: &Path) -> (String, PathBuf) {
    let seed = root.join("seed");
    std::fs::create_dir(&seed).unwrap();
    git(root, &["init", "-q", "--bare", "origin.git"]);
    git(&seed, &["init", "-q"]);
    std::fs::write(seed.join("a"), "one").unwrap();
    git(&seed, &["add", "a"]);
    git(&seed, &["commit", "-q", "-m", "one"]);
    let url = format!("file://{}", root.join("origin.git").display());
    git(&seed, &["push", "-q", &url, "main"]);
    git(root, &["clone", "-q", "--", &url, "main"]);
    (url, root.join("main"))
}

/// The facts of the session of `work` with the main clone `main`, its
/// folder in `root`.
fn facts(root: &Path, main: &Path, work: &Work) -> Facts {
    let folder = root.join(format!("sessions/{}-1a2b3c4d", work.item()));
    Facts {
        session: "1a2b3c4d-0000-4000-8000-000000000000".into(),
        item: work.item(),
        image: "localhost/riff-session:latest".into(),
        home: "/home/mike".into(),
        user: "mike".into(),
        clone: folder.join("clone"),
        repo: "o/r".into(),
        checkout: work.checkout(),
        bundle: None,
        objects: main.join(".git/objects"),
        claude: folder.join("claude"),
        sign_in: ClaudeSignIn::Token,
        gitconfig: None,
        setup: Default::default(),
    }
}

/// #2139: the arguments of `podman` for a work session and a verify bind
/// no git tree with write. The clone is a new volume of the container,
/// the objects of the main clone have no write, and the plan of the clone
/// names what the relay checks out: the branch of the item, or the head
/// commit of the pull request.
#[test]
fn the_podman_arguments_of_a_work_session_and_a_verify_have_no_writable_bind_of_a_git_tree() {
    let root = tempfile::tempdir().unwrap();
    let (_, main) = origin_and_main(root.path());
    let works = [
        Work::Build { issue: 12 },
        Work::Verify {
            issue: 12,
            pull: 40,
            commit: "1a2b3c4d".into(),
        },
    ];
    for work in works {
        let facts = facts(root.path(), &main, &work);
        let spec = riff::run::spec(&facts);
        let args: Vec<String> = riff::podman::run_args(&spec)
            .unwrap()
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        let clone = facts.clone.display().to_string();
        let volume = format!("type=volume,destination={clone},U=true");
        assert!(
            args.windows(2).any(|w| w[0] == "--mount" && w[1] == volume),
            "{args:?}"
        );
        let objects = facts.objects.display().to_string();
        let ro = format!("{objects}:{objects}:ro");
        assert!(
            args.windows(2).any(|w| w[0] == "-v" && w[1] == ro),
            "{args:?}"
        );
        for bind in args.windows(2).filter(|w| w[0] == "-v").map(|w| &w[1]) {
            if bind.ends_with(":ro") {
                continue;
            }
            let source = Path::new(bind.split(':').next().unwrap());
            let git_tree = source.join(".git").exists()
                || source.ancestors().any(|a| a.ends_with(".git"))
                || source.starts_with(&facts.clone);
            assert!(!git_tree, "a git tree with write: {bind}");
        }
        let plan = args
            .iter()
            .find_map(|a| a.strip_prefix(&format!("{VAR}=")))
            .map(|plan| Plan::of_var(plan).unwrap())
            .unwrap();
        assert_eq!(plan.path, facts.clone);
        assert_eq!(plan.origin, "https://github.com/o/r.git");
        assert_eq!(plan.reference, main.join(".git"));
        assert_eq!(plan.checkout, work.checkout());
    }
}

/// Runs `riff relay --terminal -- sh -c SCRIPT` in the folder of the
/// clone of `plan`, as the container runs it: the folder is there and
/// empty, as a new volume is, and the plan is in [`VAR`].
fn relay(env: &isolated::Isolated, plan: &Plan, script: &str) -> Output {
    std::fs::create_dir_all(&plan.path).unwrap();
    env.riff()
        .args(["relay", "--terminal", "--", "sh", "-c", script])
        .env(VAR, plan.to_var())
        .current_dir(&plan.path)
        .output()
        .unwrap()
}

/// #2139: the relay clones before `claude` starts, from `origin` with the
/// objects of the main clone, on a new branch of the item. The session
/// commits and pushes in its container, and `claude` does not get the
/// plan. The next session of the item clones again, and goes on from the
/// pushed branch. The relay of a takeover keeps the clone that is there.
#[test]
fn a_session_clones_and_pushes_in_its_container_and_the_next_goes_on_from_the_branch() {
    let env = isolated::Isolated::new();
    let root = env.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let (url, main) = origin_and_main(&root);
    let plan = |name: &str| Plan {
        path: root.join(name).join("clone"),
        origin: url.clone(),
        reference: main.join(".git"),
        checkout: Checkout::Branch("worktree-issue-12".into()),
        bundle: None,
    };
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();

    let first = plan("first");
    let out = relay(
        &env,
        &first,
        "printf '%s' \"$RIFF_CLONE\" > ../env && echo two > a && \
         git -c user.name=w -c user.email=w@w commit -q -am 'WIP: two' && \
         git push -q -u origin worktree-issue-12",
    );
    assert!(out.status.success(), "{out:?}");
    assert!(
        text(&out.stderr).contains(&riff::text::relay_cloned(&first, false)),
        "{out:?}"
    );
    assert_eq!(std::fs::read_to_string(root.join("first/env")).unwrap(), "");
    let alternates =
        std::fs::read_to_string(first.path.join(".git/objects/info/alternates")).unwrap();
    // git writes the real path of the main clone.
    assert_eq!(
        std::fs::canonicalize(alternates.trim()).unwrap(),
        std::fs::canonicalize(main.join(".git/objects")).unwrap()
    );
    let pushed = git(
        &main,
        &["ls-remote", "origin", "refs/heads/worktree-issue-12"],
    );
    let work = git(&first.path, &["rev-parse", "HEAD"]);
    assert!(pushed.starts_with(&work), "{pushed}");

    // The next session: a new clone, on the pushed branch.
    let next = plan("next");
    let out = relay(&env, &next, "git log -1 --format=%s > ../seen");
    assert!(out.status.success(), "{out:?}");
    assert!(
        text(&out.stderr).contains(&riff::text::relay_cloned(&next, true)),
        "{out:?}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("next/seen")).unwrap(),
        "WIP: two\n"
    );
    assert_eq!(git(&next.path, &["rev-parse", "HEAD"]), work);

    // A takeover runs the relay again in the same container: the clone
    // stays as it is.
    std::fs::write(next.path.join("mine"), "").unwrap();
    let out = relay(&env, &next, "true");
    assert!(out.status.success(), "{out:?}");
    assert!(!text(&out.stderr).contains("cloned"), "{out:?}");
    assert!(next.path.join("mine").exists());
}

/// #2139: a clone that fails starts no command, and the error names the
/// step.
#[test]
fn a_clone_that_fails_starts_no_command() {
    let env = isolated::Isolated::new();
    let root = env.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let (_, main) = origin_and_main(&root);
    let plan = Plan {
        path: root.join("s/clone"),
        origin: format!("file://{}", root.join("none.git").display()),
        reference: main.join(".git"),
        checkout: Checkout::Default,
        bundle: None,
    };
    let out = relay(&env, &plan, "touch ../ran");
    assert!(!out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("clone -q --reference"),
        "{out:?}"
    );
    assert!(!root.join("s/ran").exists());
}
