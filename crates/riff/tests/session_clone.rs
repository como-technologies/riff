//! Each session clones and pushes in its container (#2139). The host binds
//! no git tree with write into a container: the clone of a session is an
//! empty volume of its container, and the relay clones there from the git
//! dir of the main clone, with no write, before `claude` starts. With a
//! token, it fetches `origin` too. The work of a session is what it
//! pushed, and the next session of the item goes on from the branch on
//! `origin`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use riff::daemon::{Say, SessionAsk};
use riff::run::{ClaudeSignIn, Facts};
use riff::session_clone::{Checkout, Origin, Plan, VAR};
use riff::session_relay::Up;
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
        plan: Plan {
            path: folder.join("clone"),
            origin: Origin::github("o/r").unwrap(),
            main: main.join(".git"),
            refs: riff::run::refs_in(&folder),
            base: "main".parse().unwrap(),
            checkout: work.checkout(|_| Ok(false)).unwrap(),
        },
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
        let spec = riff::run::spec(&facts).unwrap();
        let args: Vec<String> = riff::podman::run_args(&spec)
            .unwrap()
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        let clone = facts.plan.path.display().to_string();
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
        let git_dir = facts.plan.main.display().to_string();
        let refs = facts.plan.refs.display().to_string();
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
                || source.starts_with(&facts.plan.path);
            assert!(!git_tree, "a git tree with write: {bind}");
            assert!(
                !facts.plan.refs.starts_with(source),
                "the folder of the list of refs has write: {bind}"
            );
        }
        let plan = args
            .iter()
            .find_map(|a| a.strip_prefix(&format!("{VAR}=")))
            .map(|plan| Plan::of_var(plan).unwrap())
            .unwrap();
        assert_eq!(plan, facts.plan);
    }
}

/// A session of the test: the folder of its `gh`, with a token when
/// `token` is true, the git config of the person in its home, and the
/// fake daemon on its socket.
struct Session {
    gh: PathBuf,
    gitconfig: PathBuf,
    socket: PathBuf,
    daemon: FakeDaemon,
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
        let socket = root.join("riff.sock");
        let daemon = FakeDaemon::start(&socket);
        Session {
            gh,
            gitconfig,
            socket,
            daemon,
        }
    }

    /// Runs `riff relay --terminal -- sh -c SCRIPT` in the folder of the
    /// clone of `plan`, as the container runs it: with `RIFF_SOCKET`, the
    /// folder is there and empty, as a new volume is, and the plan is in
    /// [`VAR`].
    fn relay(&self, env: &isolated::Isolated, plan: &Plan, script: &str) -> Output {
        self.relay_with(env, plan, true, script)
    }

    /// [`Session::relay`], with no terminal when `terminal` is false:
    /// `riff relay -- sh -c SCRIPT`.
    fn relay_with(
        &self,
        env: &isolated::Isolated,
        plan: &Plan,
        terminal: bool,
        script: &str,
    ) -> Output {
        self.relay_command(env, plan, terminal, &["sh", "-c", script])
    }

    /// [`Session::relay_with`] with the command `command`.
    fn relay_command(
        &self,
        env: &isolated::Isolated,
        plan: &Plan,
        terminal: bool,
        command: &[&str],
    ) -> Output {
        self.relay_process(env, plan, terminal, command)
            .output()
            .unwrap()
    }

    /// The process of [`Session::relay_command`], not yet started.
    fn relay_process(
        &self,
        env: &isolated::Isolated,
        plan: &Plan,
        terminal: bool,
        command: &[&str],
    ) -> Command {
        std::fs::create_dir_all(&plan.path).unwrap();
        let mut args = vec!["relay"];
        args.extend(terminal.then_some("--terminal"));
        args.push("--");
        args.extend(command);
        let mut relay = env.riff();
        relay
            .args(args)
            .env(VAR, plan.to_var().unwrap())
            .env("RIFF_SOCKET", &self.socket)
            .env(riff::daemon::gh_token::CONFIG_VAR, &self.gh)
            .env("GIT_CONFIG_GLOBAL", &self.gitconfig)
            .current_dir(&plan.path);
        relay
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

/// #2139, 01M7G824SYKB3BN2MN9A9JKAG0: a clone that fails starts no
/// command, and the folder of the clone is empty again. The relay says
/// one note that names the step on one line, and no other text of the
/// fault: it says the note to the daemon, and with a terminal it prints
/// the same note on stderr too. It exits with 1.
#[test]
fn a_clone_that_fails_starts_no_command() {
    let env = isolated::Isolated::new();
    let root = env.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let (_, main) = origin_and_main(&root);
    let none = format!("file://{}", root.join("none.git").display());
    let plan = plan_at(root.join("s/clone"), &none, &main, Checkout::Default);
    let session = Session::new(&root, true);
    let out = session.relay(&env, &plan, "touch ../ran");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let note = riff::text::relay_clone_failed("");
    let line = stderr.lines().find(|l| l.starts_with(note.trim_end()));
    assert!(
        line.is_some_and(|l| l.contains("fetch -q --prune origin")),
        "{out:?}"
    );
    let asks = session.daemon.take();
    let [
        SessionAsk::NoteKey { .. },
        SessionAsk::Note { line: said, .. },
    ] = &asks[..]
    else {
        panic!("{asks:?}");
    };
    assert_eq!(Some(said.as_str()), line, "{out:?}");
    assert_eq!(
        stderr.matches("claude did not start").count(),
        1,
        "{stderr}"
    );
    assert!(!stderr.contains("Error"), "{stderr}");
    assert!(!root.join("s/ran").exists());
    assert_eq!(std::fs::read_dir(&plan.path).unwrap().count(), 0);
}

/// 01M7G81XGBPP23H9T7V0WX87QZ: the default branch moved from `main` to
/// `trunk` on `origin` after the main clone, so its `origin/HEAD` still
/// names `main`. The step of the image asks `origin`
/// ([`riff::session_image::follow_origin`]): the default branch is
/// `trunk`, and `origin/HEAD` of the main clone names it now. The clone
/// with the token prunes `origin/main`, and the new branch of the item
/// starts from `origin/trunk`.
#[test]
fn a_session_starts_on_the_default_branch_of_origin_after_a_rename() {
    let env = isolated::Isolated::new();
    let root = env.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let (url, main) = origin_and_main(&root);
    git(&root.join("origin.git"), &["branch", "-m", "main", "trunk"]);
    assert_eq!(
        git(&main, &["symbolic-ref", "refs/remotes/origin/HEAD"]),
        "refs/remotes/origin/main"
    );
    let base = riff::session_image::follow_origin(&main, |line| panic!("{line}")).unwrap();
    assert_eq!(base.as_str(), "trunk");
    assert_eq!(
        git(&main, &["symbolic-ref", "refs/remotes/origin/HEAD"]),
        "refs/remotes/origin/trunk"
    );
    let item = "worktree-issue-12".parse().unwrap();
    let earlier = riff::run::earlier(&main, &item).unwrap();
    assert!(!earlier);

    let path = env.riff_home().join("sessions/issue-12-1a2b3c4d/clone");
    let plan = Plan {
        base,
        ..plan_at(path, &url, &main, branch(earlier))
    };
    let out = Session::new(&root, true).relay(&env, &plan, "true");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        git(&plan.path, &["branch", "--show-current"]),
        "worktree-issue-12"
    );
    assert_eq!(
        git(&plan.path, &["symbolic-ref", "refs/remotes/origin/HEAD"]),
        "refs/remotes/origin/trunk"
    );
    assert_eq!(
        git(&plan.path, &["rev-parse", "HEAD"]),
        git(&main, &["rev-parse", "origin/trunk"])
    );
    let gone = git(&plan.path, &["branch", "-r", "--list", "origin/main"]);
    assert_eq!(gone, "");
}

/// A fake riff daemon on the socket `socket` of a session. It answers
/// each key of the notes with done, after it closed the connection of
/// the first `unanswered` keys with no answer, and each note with
/// `answer`. It keeps each key and each note in order. It relays a
/// session: the close of the input, and an ack for each event and for
/// the exit.
struct FakeDaemon {
    asks: Arc<Mutex<Vec<SessionAsk>>>,
    answer: Arc<Mutex<Say>>,
    unanswered: Arc<Mutex<usize>>,
}

impl FakeDaemon {
    fn start(socket: &Path) -> FakeDaemon {
        let listener = UnixListener::bind(socket).unwrap();
        let asks = Arc::new(Mutex::new(Vec::new()));
        let answer = Arc::new(Mutex::new(Say::Done));
        let unanswered = Arc::new(Mutex::new(0));
        let (kept, said, left) = (asks.clone(), answer.clone(), unanswered.clone());
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let mut conn = conn.unwrap();
                let mut lines = BufReader::new(conn.try_clone().unwrap()).lines();
                let Some(Ok(first)) = lines.next() else {
                    continue;
                };
                let mut say = |say: &Say| {
                    let line = serde_json::to_string(say).unwrap();
                    let _ = writeln!(conn, "{line}");
                };
                match serde_json::from_str::<SessionAsk>(&first).unwrap() {
                    SessionAsk::Relay { .. } => {
                        say(&Say::Resume { events: 0 });
                        say(&Say::Close);
                        for line in lines.map_while(Result::ok) {
                            match serde_json::from_str::<Up>(&line).unwrap() {
                                Up::Event { seq, .. } => say(&Say::Ack { seq }),
                                Up::Exit { seq, .. } => {
                                    say(&Say::Ack { seq });
                                    break;
                                }
                                Up::Held => {}
                            }
                        }
                    }
                    ask @ SessionAsk::Note { .. } => {
                        kept.lock().unwrap().push(ask);
                        say(&said.lock().unwrap().clone());
                    }
                    ask @ SessionAsk::NoteKey { .. } => {
                        kept.lock().unwrap().push(ask);
                        let mut left = left.lock().unwrap();
                        if *left > 0 {
                            *left -= 1;
                            continue;
                        }
                        say(&Say::Done);
                    }
                    ask => {
                        kept.lock().unwrap().push(ask);
                        say(&Say::Done);
                    }
                }
            }
        });
        FakeDaemon {
            asks,
            answer,
            unanswered,
        }
    }

    /// Each key and each note that the relay gave, in order, from now on.
    fn take(&self) -> Vec<SessionAsk> {
        std::mem::take(&mut self.asks.lock().unwrap())
    }
}

/// #2139, 01M7G824SYKB3BN2MN9A9JKAG0, 01M7GE3KYR7JZHDV5S7PKAZX57: the
/// relay gives a new key to the daemon on the socket of the session
/// first, then says the note of the clone with it. With no terminal, it
/// does not print a note that the daemon took: the daemon reads that
/// stderr. With a terminal, it prints it too. When the daemon keeps no
/// note, the relay prints it. The command does not get the key, and
/// cannot read the memory of the relay: the relay is not dumpable, so
/// its files in `/proc` are not of the person.
#[test]
fn the_relay_gives_the_daemon_its_key_then_its_notes_and_prints_a_note_only_when_it_must() {
    let env = isolated::Isolated::new();
    let root = env.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let (url, main) = origin_and_main(&root);
    let session = Session::new(&root, true);
    let sessions = env.riff_home().join("sessions");
    let plan = |name: &str| {
        plan_at(
            sessions.join(name).join("clone"),
            &url,
            &main,
            branch(false),
        )
    };
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let daemon = &session.daemon;
    // The key and the note of one relay.
    let key_and_note = |asks: &[SessionAsk]| -> (String, String) {
        let [
            SessionAsk::NoteKey { key, .. },
            SessionAsk::Note {
                line, key: with, ..
            },
        ] = asks
        else {
            panic!("{asks:?}");
        };
        assert_eq!(with, key);
        let key = serde_json::to_value(key).unwrap();
        (key.as_str().unwrap().to_owned(), line.clone())
    };

    let first = plan("issue-12-1a2b3c4d");
    let script = "env > ../env; stat -c %u /proc/$PPID/environ > ../owner; \
                  cat /proc/$PPID/environ > /dev/null 2>&1 && touch ../read; true";
    let out = session.relay_with(&env, &first, false, script);
    assert!(out.status.success(), "{out:?}");
    let (key, line) = key_and_note(&daemon.take());
    let cloned = riff::text::relay_cloned(&first, true);
    assert_eq!(line, cloned);
    assert!(!text(&out.stderr).contains(&cloned), "{out:?}");
    let folder = first.path.parent().unwrap();
    let seen = std::fs::read_to_string(folder.join("env")).unwrap();
    assert_eq!(key.len(), 64);
    assert!(!seen.contains(&key), "{seen}");
    let me = nix::unistd::getuid();
    if !me.is_root() {
        let owner = std::fs::read_to_string(folder.join("owner")).unwrap();
        assert_ne!(owner.trim(), me.to_string());
        assert!(!folder.join("read").exists());
    }

    // With a terminal: the note goes to the daemon and to stderr. Each
    // relay has a new key.
    let second = plan("issue-12-2b3c4d5e");
    let out = session.relay(&env, &second, "true");
    assert!(out.status.success(), "{out:?}");
    let (other, line) = key_and_note(&daemon.take());
    assert_ne!(other, key);
    let cloned = riff::text::relay_cloned(&second, true);
    assert_eq!(line, cloned);
    assert_eq!(text(&out.stderr).matches(&cloned).count(), 1, "{out:?}");

    // The daemon keeps no note: the relay prints it.
    *daemon.answer.lock().unwrap() = Say::Failed {
        error: riff::text::DAEMON_NOTES_FULL.into(),
    };
    let third = plan("issue-12-3c4d5e6f");
    let out = session.relay_with(&env, &third, false, "true");
    assert!(out.status.success(), "{out:?}");
    let (_, line) = key_and_note(&daemon.take());
    let cloned = riff::text::relay_cloned(&third, true);
    assert_eq!(line, cloned);
    assert_eq!(text(&out.stderr).matches(&cloned).count(), 1, "{out:?}");
}

/// 01M7GJBESDBCHWB7SAT60JETEJ: the daemon closes the connection of the
/// first two keys of the relay with no answer. The relay gives the same
/// key again, and makes the clone, says its note and starts the command
/// only after the daemon took the key. So no other process of the
/// container can give a key first. With a terminal, as the lead, and
/// with none, as a worker.
#[test]
fn the_relay_starts_nothing_before_the_daemon_takes_its_key() {
    let env = isolated::Isolated::new();
    let root = env.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let (url, main) = origin_and_main(&root);
    let session = Session::new(&root, true);
    let sessions = env.riff_home().join("sessions");
    for (name, terminal) in [("issue-12-1a2b3c4d", true), ("issue-12-2b3c4d5e", false)] {
        *session.daemon.unanswered.lock().unwrap() = 2;
        let plan = plan_at(
            sessions.join(name).join("clone"),
            &url,
            &main,
            branch(false),
        );
        let out = session.relay_with(&env, &plan, terminal, "touch ../ran");
        assert!(out.status.success(), "{out:?}");
        let asks = session.daemon.take();
        let [
            SessionAsk::NoteKey { key: one, .. },
            SessionAsk::NoteKey { key: two, .. },
            SessionAsk::NoteKey { key: three, .. },
            SessionAsk::Note {
                line, key: with, ..
            },
        ] = &asks[..]
        else {
            panic!("{terminal}: {asks:?}");
        };
        assert!(one == two && two == three && three == with, "{terminal}");
        assert_eq!(*line, riff::text::relay_cloned(&plan, true), "{terminal}");
        assert!(plan.path.with_file_name("ran").exists(), "{terminal}");
    }
}

/// 01M7GXY2SAK89TEQVP72764DR4: a SIGTERM while the relay waits for the
/// answer of the daemon to its key ends the relay with 1 at once. It
/// makes no clone, and starts no command. Here the daemon never answers
/// the key.
#[test]
fn a_sigterm_before_claude_ends_the_relay_with_1_at_once() {
    let env = isolated::Isolated::new();
    let root = env.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let (url, main) = origin_and_main(&root);
    let session = Session::new(&root, true);
    *session.daemon.unanswered.lock().unwrap() = usize::MAX;
    let plan = plan_at(
        env.riff_home().join("sessions/issue-12-1a2b3c4d/clone"),
        &url,
        &main,
        branch(false),
    );
    let mut relay = session
        .relay_process(&env, &plan, false, &["sh", "-c", "touch ../ran"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // The relay gave its key, so its handlers of the signals are there.
    let span = isolated::Span::start();
    let mut gave = !session.daemon.take().is_empty();
    while !gave && span.within(isolated::HANG) {
        std::thread::sleep(Duration::from_millis(10));
        gave = !session.daemon.take().is_empty();
    }
    if gave {
        let pid = nix::unistd::Pid::from_raw(i32::try_from(relay.id()).unwrap());
        nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).unwrap();
    }
    let span = isolated::Span::start();
    let mut ended = relay.try_wait().unwrap();
    while gave && ended.is_none() && span.within(Duration::from_secs(10)) {
        std::thread::sleep(Duration::from_millis(10));
        ended = relay.try_wait().unwrap();
    }
    if ended.is_none() {
        let _ = relay.kill();
        let _ = relay.wait();
    }
    assert!(gave, "the relay gave no key");
    assert_eq!(
        ended.map(|status| status.code()),
        Some(Some(1)),
        "the relay did not end with 1 after the signal"
    );
    assert!(!plan.path.with_file_name("ran").exists());
    assert_eq!(std::fs::read_dir(&plan.path).unwrap().count(), 0);
}

/// 01M7GJBFAJDCYCMRDSWSX3VNWX: each line that `riff relay` itself prints
/// on its stderr starts with `riff relay: `, also its error: a command
/// that does not start, with a terminal and with none, and no socket.
/// The exit is 1.
#[test]
fn each_line_of_the_relay_itself_on_its_stderr_starts_with_riff_relay() {
    let env = isolated::Isolated::new();
    let root = env.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let (url, main) = origin_and_main(&root);
    let session = Session::new(&root, true);
    let sessions = env.riff_home().join("sessions");
    let lines = |out: &Output| -> Vec<String> {
        String::from_utf8_lossy(&out.stderr)
            .lines()
            .map(str::to_owned)
            .collect()
    };
    let program = "/nonexistent/claude";
    for (name, terminal) in [("issue-12-1a2b3c4d", true), ("issue-12-2b3c4d5e", false)] {
        let plan = plan_at(
            sessions.join(name).join("clone"),
            &url,
            &main,
            branch(false),
        );
        let out = session.relay_command(&env, &plan, terminal, &[program]);
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        let lines = lines(&out);
        assert!(!lines.is_empty(), "{out:?}");
        for line in &lines {
            assert!(line.starts_with(riff::text::RELAY), "{terminal}: {line}");
        }
        let error = riff::text::relay(&riff::text::no_program(program));
        assert!(lines.last().unwrap().starts_with(&error), "{lines:?}");
    }
    let out = env
        .riff()
        .args(["relay", "--", "true"])
        .env_remove("RIFF_SOCKET")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(
        lines(&out),
        [riff::text::relay(riff::text::RELAY_NO_SOCKET)]
    );
}
