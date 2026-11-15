//! Each session clones and pushes in its container (#2139). The host binds
//! no git tree with write into a container: the clone of a session is an
//! empty volume of its container, and the relay clones there from the git
//! dir of the main clone, with no write, before `claude` starts. With a
//! token, it fetches `origin` too. The work of a session is what it
//! pushed, and the next session of the item goes on from the branch on
//! `origin`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use riff::run::{ClaudeSignIn, Facts};
use riff::session_clone::{Checkout, Origin, Plan, VAR};
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
        origin: Origin::github("o/r").unwrap(),
        base: "main".parse().unwrap(),
        checkout: work.checkout(|_| Ok(false)).unwrap(),
        main: main.join(".git"),
        refs: riff::run::refs_in(&folder),
        claude: folder.join("claude"),
        sign_in: ClaudeSignIn::Token,
        gitconfig: None,
        setup: Default::default(),
    }
}

/// #2139, 01M7EY3P0Q8TQF5QRTRDWCYCTQ: the arguments of `podman` for a
/// work session and a verify bind no git tree with write. The clone is a
/// new volume of the container, and the plan of the clone names what the
/// relay checks out: the branch of the item, or the head commit of the
/// pull request. 01M7FMJJB0TZM8875T7GQZAJEA: of the git dir of the main
/// clone, they bind only its objects, with no write: no `config`, `refs`,
/// `packed-refs`, `logs`, `hooks` or `worktrees`. They bind the list of
/// the branches of `origin` with no write, and no folder above it with
/// write: no container can write the folder of that file.
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
        let binds: Vec<&str> = args
            .windows(2)
            .filter(|w| w[0] == "-v")
            .map(|w| w[1].as_str())
            .collect();
        let git_dir = facts.main.display().to_string();
        let refs = facts.refs.display().to_string();
        for ro in [format!("{git_dir}/objects"), refs.clone()] {
            let bind = format!("{ro}:{ro}:ro");
            assert!(binds.contains(&bind.as_str()), "{bind}: {args:?}");
        }
        let of_main: Vec<&&str> = binds.iter().filter(|b| b.starts_with(&git_dir)).collect();
        assert_eq!(of_main.len(), 1, "{of_main:?}");
        for bind in &binds {
            if bind.ends_with(":ro") {
                continue;
            }
            let source = Path::new(bind.split(':').next().unwrap());
            let git_tree = source.join(".git").exists()
                || source.ancestors().any(|a| a.ends_with(".git"))
                || source.starts_with(&facts.clone);
            assert!(!git_tree, "a git tree with write: {bind}");
            assert!(
                !facts.refs.starts_with(source),
                "the folder of the list of refs has write: {bind}"
            );
        }
        let plan = args
            .iter()
            .find_map(|a| a.strip_prefix(&format!("{VAR}=")))
            .map(|plan| Plan::of_var(plan).unwrap())
            .unwrap();
        assert_eq!(plan.path, facts.clone);
        assert_eq!(plan.origin.as_str(), "https://github.com/o/r.git");
        assert_eq!(plan.main, main.join(".git"));
        assert_eq!(plan.refs, facts.refs);
        assert_eq!(plan.checkout, work.checkout(|_| Ok(false)).unwrap());
    }
}

/// A session of the test: the folder of its `gh`, with a token when
/// `token` is true, and the git config of the person in its home.
struct Session {
    gh: PathBuf,
    gitconfig: PathBuf,
}

impl Session {
    /// The session in `root`. The git config of the person has
    /// `checkout.guess=false`: the start points of the clone are explicit.
    fn new(root: &Path, token: bool) -> Session {
        let gh = root.join(if token { "gh-token" } else { "gh-none" });
        std::fs::create_dir_all(&gh).unwrap();
        if token {
            std::fs::write(gh.join(riff::daemon::gh_token::HOSTS), "github.com:\n").unwrap();
        }
        let gitconfig = root.join("gitconfig");
        std::fs::write(&gitconfig, "[checkout]\n\tguess = false\n").unwrap();
        Session { gh, gitconfig }
    }

    /// Runs `riff relay --terminal -- sh -c SCRIPT` in the folder of the
    /// clone of `plan`, as the container runs it: with `RIFF_SOCKET`, the
    /// folder is there and empty, as a new volume is, and the plan is in
    /// [`VAR`].
    fn relay(&self, env: &isolated::Isolated, plan: &Plan, script: &str) -> Output {
        std::fs::create_dir_all(&plan.path).unwrap();
        env.riff()
            .args(["relay", "--terminal", "--", "sh", "-c", script])
            .env(VAR, plan.to_var())
            .env("RIFF_SOCKET", env.path().join("riff.sock"))
            .env(riff::daemon::gh_token::CONFIG_VAR, &self.gh)
            .env("GIT_CONFIG_GLOBAL", &self.gitconfig)
            .current_dir(&plan.path)
            .output()
            .unwrap()
    }
}

/// The git dir of the main clone `main` as the container sees it
/// (01M7FMJJB0TZM8875T7GQZAJEA): `seen/.git` beside it, with a link to
/// its objects only, and no config, ref, hook or log.
fn seen(main: &Path) -> PathBuf {
    let seen = main.with_file_name("seen").join(".git");
    if !seen.exists() {
        std::fs::create_dir_all(&seen).unwrap();
        std::os::unix::fs::symlink(main.join(".git/objects"), seen.join("objects")).unwrap();
    }
    seen
}

/// The plan of a clone of `main` at `path`, with `origin` at `url`. The
/// relay reads the main clone as the container sees it ([`seen`]), and
/// the list of its refs that the host writes beside the clone now
/// ([`riff::session_clone::write_refs`]).
fn plan_at(path: PathBuf, url: &str, main: &Path, checkout: Checkout) -> Plan {
    let refs = path.with_file_name(riff::session_clone::REFS);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    riff::session_clone::write_refs(&main.join(".git"), path.parent().unwrap()).unwrap();
    Plan {
        path,
        origin: url.parse().unwrap(),
        main: seen(main),
        refs,
        base: "main".parse().unwrap(),
        checkout,
    }
}

/// The branch of issue 12, with no bundle.
fn branch(earlier: bool) -> Checkout {
    Checkout::Branch {
        name: "worktree-issue-12".parse().unwrap(),
        earlier,
        no_github: false,
    }
}

/// #2139, 01M7EY3P0Q8TQF5QRTRDWCYCTQ: the relay clones before `claude`
/// starts, in the folder of the sessions, from the git dir of the main
/// clone with no object copied, and with the token from `origin`, on a new
/// branch of the item. The session commits and pushes in its container,
/// and `claude` does not get the plan. The next session of the item
/// clones again, and goes on from the pushed branch, also with
/// `checkout.guess=false` in the git config of the person. The relay of a
/// takeover keeps the clone that is there.
#[test]
fn a_session_clones_and_pushes_in_its_container_and_the_next_goes_on_from_the_branch() {
    let env = isolated::Isolated::new();
    let root = env.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let (url, main) = origin_and_main(&root);
    let session = Session::new(&root, true);
    let sessions = env.riff_home().join("sessions");
    let plan = |name: &str, earlier| {
        plan_at(
            sessions.join(name).join("clone"),
            &url,
            &main,
            branch(earlier),
        )
    };
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();

    let first = plan("issue-12-1a2b3c4d", false);
    let out = session.relay(
        &env,
        &first,
        "printf '%s' \"$RIFF_CLONE\" > ../env && echo two > a && \
         git -c user.name=w -c user.email=w@w commit -q -am 'WIP: two' && \
         git push -q -u origin worktree-issue-12",
    );
    assert!(out.status.success(), "{out:?}");
    assert!(
        text(&out.stderr).contains(&riff::text::relay_cloned(&first, true)),
        "{out:?}"
    );
    assert_eq!(
        std::fs::read_to_string(first.path.parent().unwrap().join("env")).unwrap(),
        ""
    );
    let alternates =
        std::fs::read_to_string(first.path.join(".git/objects/info/alternates")).unwrap();
    assert_eq!(Path::new(alternates.trim()), seen(&main).join("objects"));
    let pushed = git(
        &main,
        &["ls-remote", "origin", "refs/heads/worktree-issue-12"],
    );
    let work = git(&first.path, &["rev-parse", "HEAD"]);
    assert!(pushed.starts_with(&work), "{pushed}");

    // The next session: a new clone, on the pushed branch.
    let next = plan("issue-12-2b3c4d5e", true);
    let out = session.relay(&env, &next, "git log -1 --format=%s > ../seen");
    assert!(out.status.success(), "{out:?}");
    assert!(
        text(&out.stderr).contains(&riff::text::relay_cloned(&next, true)),
        "{out:?}"
    );
    assert_eq!(
        std::fs::read_to_string(next.path.parent().unwrap().join("seen")).unwrap(),
        "WIP: two\n"
    );
    assert_eq!(git(&next.path, &["rev-parse", "HEAD"]), work);

    // A takeover runs the relay again in the same container: the clone
    // stays as it is.
    std::fs::write(next.path.join("mine"), "").unwrap();
    let out = session.relay(&env, &next, "true");
    assert!(out.status.success(), "{out:?}");
    assert!(!text(&out.stderr).contains("cloned"), "{out:?}");
    assert!(next.path.join("mine").exists());
}

/// #2139, 01M7EY3P0Q8TQF5QRTRDWCYCTQ: a session with no token, a lead with
/// no token of an App or a session of `riff run --no-github`, fetches
/// nothing from GitHub. So it starts on a private repository too: here an
/// `origin` that asks for a sign-in that the session does not have. Its
/// clone starts from the main clone.
#[test]
fn a_session_with_no_token_starts_from_the_main_clone_on_a_private_repository() {
    let env = isolated::Isolated::new();
    let root = env.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let (_, main) = origin_and_main(&root);
    let private = "https://github.com/como-technologies/no-such-private-repo.git";
    let lead = plan_at(
        env.riff_home().join("leads/o-r/clone"),
        private,
        &main,
        Checkout::Default,
    );
    let out = Session::new(&root, false).relay(&env, &lead, "git log -1 --format=%s > ../seen");
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(&riff::text::relay_cloned(&lead, false)),
        "{out:?}"
    );
    assert_eq!(
        std::fs::read_to_string(env.riff_home().join("leads/o-r/seen")).unwrap(),
        "one\n"
    );
}

/// #2139: a clone that fails starts no command, the error names the step,
/// and the folder of the clone is empty again.
#[test]
fn a_clone_that_fails_starts_no_command() {
    let env = isolated::Isolated::new();
    let root = env.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let (_, main) = origin_and_main(&root);
    let none = format!("file://{}", root.join("none.git").display());
    let plan = plan_at(root.join("s/clone"), &none, &main, Checkout::Default);
    let out = Session::new(&root, true).relay(&env, &plan, "touch ../ran");
    assert!(!out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("fetch -q --prune origin"),
        "{out:?}"
    );
    assert!(!root.join("s/ran").exists());
    assert_eq!(std::fs::read_dir(&plan.path).unwrap().count(), 0);
}
