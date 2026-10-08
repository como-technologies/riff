//! The sandbox of a worker (01M4BTB72DY0C5Y0EJVR0ZH6FZ): `riff workers
//! run` starts a fake `claude` under the worker profile, and the fake
//! tries each thing that the profile allows or refuses.

use isolated::Isolated;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

/// A machine of a person: a fake home with its secrets and its Claude
/// folder, and a git worktree outside the home.
struct Machine {
    env: Isolated,
    /// The folder of the main clone.
    root: PathBuf,
    /// The temp folder that holds `root`, when it is not in `env`.
    _own: Option<tempfile::TempDir>,
}

impl Machine {
    fn new() -> Self {
        let env = Isolated::new();
        let root = env.path().join("main");
        Self::with(env, root, None)
    }

    /// A machine whose clone is in `target/tmp` of this build, not in
    /// `/tmp` or `/var/tmp`: a test run hides both behind a new empty
    /// folder.
    fn outside_tmp() -> Self {
        let exe = std::env::current_exe().unwrap();
        let tmp = exe.ancestors().nth(3).unwrap().join("tmp");
        std::fs::create_dir_all(&tmp).unwrap();
        let own = tempfile::tempdir_in(tmp).unwrap();
        let root = own.path().join("main");
        Self::with(Isolated::new(), root, Some(own))
    }

    fn with(env: Isolated, root: PathBuf, own: Option<tempfile::TempDir>) -> Self {
        let m = Machine {
            env,
            root,
            _own: own,
        };
        let home = m.env.home();
        for (file, text) in [
            (".bashrc", "# the bashrc of the person\n"),
            (".ssh/id_ed25519", "a key\n"),
            (".claude/settings.json", "{}\n"),
            (".local/share/riff/rules/w1.json", "{}\n"),
        ] {
            let path = home.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        git(&m.clone(), &["init", "-q"]);
        m
    }

    fn home(&self) -> PathBuf {
        self.env.home()
    }

    /// The main clone, where a worker starts.
    fn clone(&self) -> PathBuf {
        std::fs::create_dir_all(&self.root).unwrap();
        self.root.canonicalize().unwrap()
    }

    /// The folder of the worktrees of the clone: the worktree of the
    /// profile of a worker with no item (01M4BT341H1M1N1MT947HXNXDR).
    /// It holds the git worktree `issue-1`.
    fn worktrees(&self) -> PathBuf {
        let dir = self.clone().join(".claude/worktrees");
        if !dir.join("issue-1").exists() {
            let clone = self.clone();
            git(
                &clone,
                &[
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    "x",
                ],
            );
            git(
                &clone,
                &["worktree", "add", "-q", ".claude/worktrees/issue-1"],
            );
        }
        dir
    }

    /// A fake `claude` in the folder of the worktrees that runs `body`
    /// with bash.
    fn claude(&self, body: &str) -> PathBuf {
        let path = self.worktrees().join("claude");
        std::fs::write(&path, format!("#!/bin/bash\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// `riff workers run CLAUDE` in `dir`, as the worker session `w1`.
    fn run(&self, dir: &Path, claude: &Path, vars: &[(&str, &std::ffi::OsStr)]) -> Output {
        let mut cmd = self.env.riff();
        cmd.args(["workers", "run"])
            .arg(claude)
            .current_dir(dir)
            .env("RIFF_SESSION", "w1")
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("TMUX_PANE", "%5");
        for (name, value) in vars {
            cmd.env(name, value);
        }
        cmd.output().unwrap()
    }
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A process outside the sandbox. Its drop ends it.
struct Outside(Child);

impl Outside {
    fn start() -> Self {
        Outside(
            Command::new("sleep")
                .arg("300")
                .stdin(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }
}

impl Drop for Outside {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A TCP port on loopback with a listener, out of the local port range
/// of the kernel, so no rule of the sandbox allows it.
fn port_out_of_range() -> TcpListener {
    let local = riff::confine::local_ports();
    (20000..u16::MAX)
        .filter(|p| !local.contains(p))
        .find_map(|p| TcpListener::bind(("127.0.0.1", p)).ok())
        .expect("a free port out of the local range")
}

/// The Landlock ABI in the line of the sandbox, or 0.
fn abi(err: &str) -> i32 {
    err.split("Landlock ABI ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// 01M4BTB72DY0C5Y0EJVR0ZH6FZ, 01M4BTB79FZ5RAFXMZRRJZNCH9,
/// 01M4BTB7BPA38ARM789WBV507D, 01M4BTB7DY1Y74PP3JWKVX58JQ: a worker
/// writes its worktree, its own Claude folder and the riff state. It
/// cannot write or read the home of the person, its secrets, the Claude
/// folder of the person or its own permission rules. It connects to the
/// local port range and not to another port. It cannot signal a
/// process outside the sandbox (ABI 6), or read its environment.
#[test]
fn a_worker_does_only_what_its_profile_allows() {
    let m = Machine::new();
    let outside = Outside::start();
    let closed = port_out_of_range();
    let open = TcpListener::bind("127.0.0.1:0").unwrap();
    let home = m.home();
    let result = m.worktrees().join("result");
    let tries = [
        (
            "worktree",
            format!("echo x > '{}/issue-1/new'", m.worktrees().display()),
        ),
        (
            "git-objects",
            format!("echo x > '{}/.git/objects/probe'", m.clone().display()),
        ),
        (
            "git-config",
            format!("echo x >> '{}/.git/config'", m.clone().display()),
        ),
        (
            "git-hook",
            format!("echo x > '{}/.git/hooks/post-merge'", m.clone().display()),
        ),
        (
            "git-root",
            format!("echo x > '{}/.git/probe'", m.clone().display()),
        ),
        ("clone", format!("echo x > '{}/new'", m.clone().display())),
        ("home", format!("echo x > '{}/new'", home.display())),
        ("bashrc", format!("cat '{}/.bashrc'", home.display())),
        ("ssh", format!("cat '{}/.ssh/id_ed25519'", home.display())),
        (
            "person-claude",
            format!("echo x > '{}/.claude/settings.json'", home.display()),
        ),
        (
            "rules",
            format!(
                "echo x > '{}/.local/share/riff/rules/w1.json'",
                home.display()
            ),
        ),
        (
            "plugin",
            format!(
                "echo x > '{}/.local/share/riff/claude-plugin/probe'",
                home.display()
            ),
        ),
        (
            "gitconfig",
            format!("echo x >> '{}/.gitconfig'", home.display()),
        ),
        (
            "own-claude",
            "echo x > \"$CLAUDE_CONFIG_DIR/settings.json\"".into(),
        ),
        (
            "state",
            format!("echo x > '{}/state/probe'", m.env.riff_home().display()),
        ),
        (
            "tmux-conf",
            format!(
                "echo x >> '{}/state/tmux.conf'",
                m.env.riff_home().display()
            ),
        ),
        (
            "other-session",
            format!(
                "echo x > '{}/state/sessions/w2/probe'",
                m.env.riff_home().display()
            ),
        ),
        ("own-state", "echo x > \"$RIFF_STATE/probe\"".into()),
        (
            "open-port",
            format!(
                "exec 3<>/dev/tcp/127.0.0.1/{}",
                open.local_addr().unwrap().port()
            ),
        ),
        (
            "closed-port",
            format!(
                "exec 3<>/dev/tcp/127.0.0.1/{}",
                closed.local_addr().unwrap().port()
            ),
        ),
        ("signal", format!("kill -0 {}", outside.0.id())),
        ("environ", format!("cat /proc/{}/environ", outside.0.id())),
    ];
    let mut body = format!(": > '{}'\n", result.display());
    for (name, command) in &tries {
        body.push_str(&format!(
            "if ( {command} ) >/dev/null 2>&1; then echo '{name} yes'; else echo '{name} no'; fi >> '{}'\n",
            result.display()
        ));
    }
    body.push_str(&format!(
        "echo \"claude-dir $CLAUDE_CONFIG_DIR\" >> '{}'\n",
        result.display()
    ));
    let claude = m.claude(&body);
    let state = m.env.riff_home().join("state");
    std::fs::create_dir_all(state.join("sessions/w2")).unwrap();
    std::fs::write(state.join("tmux.conf"), "# riff\n").unwrap();
    // The plugin and the git config of the person are there, so that a
    // "no" comes from the sandbox, not from a missing folder.
    std::fs::create_dir_all(home.join(".local/share/riff/claude-plugin")).unwrap();
    let gitconfig = home.join(".gitconfig");
    let before = std::fs::read(&gitconfig).unwrap_or_default();
    std::fs::write(&gitconfig, &before).unwrap();

    let out = m.run(&m.clone(), &claude, &[]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert!(
        err.contains("riff: the sandbox is on: Landlock ABI"),
        "{err}"
    );
    let got = std::fs::read_to_string(&result).unwrap();
    let mut want = vec![
        "worktree yes",
        "git-objects yes",
        "git-config no",
        "git-hook no",
        "git-root no",
        "clone no",
        "home no",
        "bashrc no",
        "ssh no",
        "person-claude no",
        "rules no",
        "plugin no",
        "gitconfig no",
        "own-claude yes",
        "state no",
        "tmux-conf no",
        "other-session no",
        "own-state yes",
        "open-port yes",
        "closed-port no",
    ];
    // The scope of signals needs ABI 6 (Linux 6.12).
    let signal = match abi(&err) >= 6 {
        true => "signal no",
        false => {
            println!("skip: the scope of signals needs Landlock ABI 6");
            got.lines().find(|l| l.starts_with("signal ")).unwrap()
        }
    };
    want.extend([signal, "environ no"]);
    let lines: Vec<&str> = got.lines().collect();
    assert_eq!(&lines[..want.len()], &want[..], "{got}");
    let claude_dir = home.join(".local/share/riff/claude/w1");
    assert_eq!(
        lines[want.len()],
        format!("claude-dir {}", claude_dir.display())
    );
    assert!(!home.join("new").exists());
    assert!(!home.join(".local/share/riff/claude-plugin/probe").exists());
    assert_eq!(std::fs::read(&gitconfig).unwrap(), before);
    assert_eq!(
        std::fs::read_to_string(home.join(".claude/settings.json")).unwrap(),
        "{}\n"
    );
}

/// 01M4CN0W3V733V6R2SG1YYZRCN: a worker commits in its worktree, and
/// fetches and pushes it, with no write of the config and the hooks of
/// the clone. The config and the hooks stay as they were.
#[test]
fn a_worker_commits_and_pushes_with_no_write_of_the_git_config() {
    let m = Machine::new();
    let worktrees = m.worktrees();
    // A remote that the sandbox writes: in the folder of the worktrees.
    let remote = worktrees.join("remote.git");
    git(&worktrees, &["init", "-q", "--bare", "remote.git"]);
    git(
        &m.clone(),
        &["remote", "add", "origin", &remote.display().to_string()],
    );
    git(&m.clone(), &["push", "-q", "origin", "HEAD:main"]);
    git(&m.clone(), &["fetch", "-q", "origin"]);
    git(&m.clone(), &["pack-refs", "--all"]);
    let config = std::fs::read_to_string(m.clone().join(".git/config")).unwrap();
    let hooks = m.clone().join(".git/hooks");
    let hooks_before = std::fs::read_dir(&hooks).map_or(0, |d| d.count());
    let result = worktrees.join("result");
    let claude = m.claude(&format!(
        "cd \"$(dirname \"$0\")/issue-1\"\n\
         r() {{ \"$@\" >/dev/null 2>&1; echo \"$1 $2 $?\" >> '{0}'; }}\n\
         : > '{0}'\n\
         echo x > f\n\
         r git add f\n\
         r git -c user.name=t -c user.email=t@t commit -q -m one\n\
         r git push -q --force-with-lease --force-if-includes origin HEAD\n\
         r git fetch -q origin\n\
         r git rebase -q origin/main\n\
         r git config branch.x.remote origin",
        result.display()
    ));
    let out = m.run(&m.clone(), &claude, &[]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_eq!(
        std::fs::read_to_string(&result).unwrap(),
        "git add 0\ngit -c 0\ngit push 0\ngit fetch 0\ngit rebase 0\ngit config 255\n"
    );
    let heads = Command::new("git")
        .args([
            "--git-dir",
            &remote.display().to_string(),
            "branch",
            "--list",
        ])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&heads.stdout).contains("issue-1"),
        "the push did not reach the remote: {}",
        String::from_utf8_lossy(&heads.stdout)
    );
    assert_eq!(
        std::fs::read_to_string(m.clone().join(".git/config")).unwrap(),
        config
    );
    assert_eq!(
        std::fs::read_dir(&hooks).map_or(0, |d| d.count()),
        hooks_before
    );
}

/// 01M4C5AQQX543ZFJ7J005HC5E9, 01M4C5AQV8AT8F5WKNF1C9CE5F,
/// 01M4C5AQYT6V1MJ37JXJ3PNYQ2: a worker reaches no tmux server of the
/// person, and gets no `TMUX`. A pair of sockets still works.
#[test]
fn a_worker_reaches_no_tmux_server() {
    use std::os::unix::fs::MetadataExt;
    let m = Machine::new();
    let tmp = m.env.path().join("tmux-tmp");
    let uid = std::fs::metadata("/proc/self").unwrap().uid();
    let dir = tmp.join(format!("tmux-{uid}"));
    std::fs::create_dir_all(&dir).unwrap();
    let socket = dir.join("riff");
    let _server = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    // Outside the sandbox, the connect works.
    std::os::unix::net::UnixStream::connect(&socket).unwrap();
    let result = m.worktrees().join("result");
    let python = |code: &str| format!("python3 -c \"import socket; {code}\"");
    let tries = [
        (
            "tmux-socket",
            python(&format!(
                "socket.socket(socket.AF_UNIX).connect('{}')",
                socket.display()
            )),
        ),
        ("socketpair", python("socket.socketpair()")),
    ];
    let mut body = format!(": > '{}'\n", result.display());
    for (name, command) in &tries {
        body.push_str(&format!(
            "if ( {command} ) >/dev/null 2>&1; then echo '{name} yes'; else echo '{name} no'; fi >> '{}'\n",
            result.display()
        ));
    }
    body.push_str(&format!(
        "echo \"tmux-vars [$TMUX] [$TMUX_PANE]\" >> '{0}'\n\
         grep core /proc/self/limits | tr -s ' ' >> '{0}'\n",
        result.display()
    ));
    let claude = m.claude(&body);
    let tmux = format!("{},1,0", socket.display());
    let out = m.run(
        &m.clone(),
        &claude,
        &[("TMUX_TMPDIR", tmp.as_os_str()), ("TMUX", tmux.as_ref())],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_eq!(
        std::fs::read_to_string(&result).unwrap(),
        // No core dump and no crash helper (01M4C6HE4D4ADJVBCC6EW8FP0X).
        "tmux-socket no\nsocketpair yes\ntmux-vars [] []\nMax core file size 1 1 bytes \n"
    );
}

/// 01M4C5AQGCA3TFZDW23HYKS83S: `riff test-run` in a worker cannot make
/// its namespaces in the sandbox, so the broker runs it outside. The
/// program runs in the test run, and its output and its exit code come
/// back to the worker.
#[test]
fn a_test_run_in_a_worker_runs_through_the_broker() {
    let m = Machine::outside_tmp();
    let result = m.worktrees().join("result");
    let bin = Isolated::shared().riff_path();
    let claude = m.claude(&format!(
        "cd \"$(dirname \"$0\")/issue-1\"\n\
         '{1}' test-run -- sh -c 'echo \"run [$RIFF_TEST_RUN] [$RIFF_BROKER]\"; grep core /proc/self/limits | tr -s \" \"; exit 3' > '{0}' 2>&1\n\
         echo \"code $?\" >> '{0}'\n\
         echo \"broker [${{RIFF_BROKER:+set}}]\" >> '{0}'",
        result.display(),
        bin.display()
    ));
    let out = m.run(&m.clone(), &claude, &[]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    let got = std::fs::read_to_string(&result).unwrap();
    if got.contains("bubblewrap") || got.contains("apparmor") {
        println!("skip: this machine has no bubblewrap for a test run: {got}");
        assert!(got.ends_with("code 1\nbroker [set]\n"), "{got}");
        return;
    }
    assert_eq!(
        got,
        "run [1] []\nMax core file size 1 1 bytes \ncode 3\nbroker [set]\n"
    );
}

/// 01M4CN0W1F0Y7955C6Q1XB601G, 01M4CN0RRJF5EWGE0VDSX3442B: a request to
/// the broker cannot name the folder that a test run writes. A
/// `CARGO_TARGET_DIR` of the request is not the target of the run, and a
/// target that is a link to a folder outside the worktree gives no run.
/// The folder outside gets no file.
#[test]
fn a_test_run_writes_no_folder_that_the_request_names() {
    let m = Machine::outside_tmp();
    let result = m.worktrees().join("result");
    // A folder of the person outside the worktree, and outside /tmp, so
    // a bind of it would show in the run.
    let outside = m.clone().parent().unwrap().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let bin = Isolated::shared().riff_path();
    let claude = m.claude(&format!(
        "cd \"$(dirname \"$0\")/issue-1\"\n\
         CARGO_TARGET_DIR='{2}' '{1}' test-run -- sh -c 'w() {{ if echo x > \"$2\"; then echo \"$1 yes\"; else echo \"$1 no\"; fi; }}; w outside \"{2}/evil\"; w target target/probe' > '{0}' 2>/dev/null\n\
         echo \"code $?\" >> '{0}'\n\
         rm -rf target && ln -s '{2}' target\n\
         '{1}' test-run -- sh -c 'echo x > target/link' >> '{0}' 2>&1\n\
         echo \"code $?\" >> '{0}'",
        result.display(),
        bin.display(),
        outside.display()
    ));
    let out = m.run(&m.clone(), &claude, &[("CARGO_TARGET_DIR", "".as_ref())]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    let got = std::fs::read_to_string(&result).unwrap();
    assert_eq!(
        std::fs::read_dir(&outside).unwrap().count(),
        0,
        "a test run wrote outside the worktree: {got}"
    );
    if got.contains("bubblewrap") || got.contains("apparmor") {
        println!("skip: this machine has no bubblewrap for a test run: {got}");
        return;
    }
    // The run writes the target of the worktree, not the folder of the
    // request.
    assert!(got.starts_with("outside no\ntarget yes\ncode 0\n"), "{got}");
    assert!(got.contains("is not in"), "{got}");
    assert!(got.ends_with("code 1\n"), "{got}");
}

/// 01M4D7TB7FZAMASMQG9K7M3Q0D: a worker plants a `.git` file that names
/// another repository of the person in a folder of its worktree, and
/// asks for a test run there. The test run takes the clone of the
/// session from the broker: it reads no git dir of the other
/// repository.
#[test]
fn a_planted_git_file_gives_a_test_run_no_other_git_dir() {
    let m = Machine::outside_tmp();
    let result = m.worktrees().join("result");
    // Another repository of the person, outside the clone and outside
    // /tmp, so a bind of it would show in the run.
    let other = m.clone().parent().unwrap().join("other");
    std::fs::create_dir_all(&other).unwrap();
    git(&other, &["init", "-q"]);
    std::fs::write(other.join(".git/secret"), "a token\n").unwrap();
    let bin = Isolated::shared().riff_path();
    let claude = m.claude(&format!(
        "mkdir -p \"$(dirname \"$0\")/issue-1/n\" && cd \"$(dirname \"$0\")/issue-1/n\"\n\
         echo 'gitdir: {2}/.git' > .git\n\
         '{1}' test-run -- sh -c 'if cat \"{2}/.git/secret\"; then echo read yes; else echo read no; fi' > '{0}' 2>&1\n\
         echo \"code $?\" >> '{0}'",
        result.display(),
        bin.display(),
        other.display()
    ));
    let out = m.run(&m.clone(), &claude, &[]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    let got = std::fs::read_to_string(&result).unwrap();
    if got.contains("bubblewrap") || got.contains("apparmor") {
        println!("skip: this machine has no bubblewrap for a test run: {got}");
        return;
    }
    assert!(!got.contains("a token"), "{got}");
    assert!(got.ends_with("read no\ncode 0\n"), "{got}");
}

/// 01M4BR61PPQV7JJE5Y2G9Q90AF, 01M4BTB7757XF8RZ6MRXKTM8SB: a worktree
/// path that is a link to the home of the person gives no profile, so
/// `claude` does not start and the home gets no write. The same for a
/// target dir that is a link to the home.
#[test]
fn a_link_to_the_home_gives_no_sandbox_and_no_write() {
    let m = Machine::new();
    git(&m.home(), &["init", "-q"]);
    let link = m.env.path().join("link");
    std::os::unix::fs::symlink(m.home(), &link).unwrap();
    let mark = format!("echo x > '{}/new'", m.home().display());
    let claude = m.claude(&mark);

    let out = m.run(&link, &claude, &[]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_ne!(out.status.code(), Some(0), "{err}");
    assert!(err.contains("gives the home of the person"), "{err}");
    assert!(!m.home().join("new").exists(), "claude ran: {err}");

    let target = m.env.path().join("target-link");
    std::os::unix::fs::symlink(m.home(), &target).unwrap();
    let out = m.run(
        &m.clone(),
        &claude,
        &[("CARGO_TARGET_DIR", target.as_os_str())],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_ne!(out.status.code(), Some(0), "{err}");
    assert!(err.contains("gives the home of the person"), "{err}");
    assert!(!m.home().join("new").exists(), "claude ran: {err}");
}

/// 01M4DWJ0KT7G2RX05X00YGVN21: a session writes its worktree, so it can
/// put a link there. A target in the worktree that is a link to a
/// folder outside it makes riff refuse the sandbox: `claude` does not
/// start, and the folder gets no write.
#[test]
fn a_link_out_of_the_worktree_gives_no_sandbox_and_no_write() {
    let m = Machine::new();
    let tree = m.worktrees().join("issue-1");
    let away = m.home().join(".config");
    std::fs::create_dir_all(&away).unwrap();
    std::os::unix::fs::symlink(&away, tree.join("target")).unwrap();
    let claude = m.claude(&format!("echo x > '{}/new'", away.display()));

    let out = m.run(&tree, &claude, &[]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_ne!(out.status.code(), Some(0), "{err}");
    assert!(err.contains("is a link out of its root"), "{err}");
    assert!(!away.join("new").exists(), "claude ran: {err}");
}

/// `riff workers sandbox` in `start`, from a shell that went to `start`
/// (`PWD`), refuses with `refusal`. It shows no profile, and it runs no
/// program, so the repository `other` gets no file `new`.
fn refuses_at(m: &Machine, start: &Path, other: &Path, refusal: &str) {
    let sandbox = |more: &[&str]| {
        m.env
            .riff()
            .args(["workers", "sandbox", "--role", "worker"])
            .args(more)
            .current_dir(start)
            .env("PWD", start)
            .env("RIFF_SESSION", "w1")
            .output()
            .unwrap()
    };
    let out = sandbox(&["--show"]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!out.status.success(), "{out:?}");
    assert!(err.contains(refusal), "{err}");
    assert!(out.stdout.is_empty(), "{out:?}");

    let new = other.join("new");
    let out = sandbox(&["--", "touch", new.to_str().unwrap()]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!out.status.success(), "{out:?}");
    assert!(err.contains(refusal), "{err}");
    assert!(!new.exists(), "the program ran: {err}");
}

/// Another repository of the person, outside the clone.
fn other_repository(m: &Machine) -> PathBuf {
    let other = m.clone().parent().unwrap().join("other");
    std::fs::create_dir_all(&other).unwrap();
    git(&other, &["init", "-q"]);
    other
}

/// 01M4EPNXVSA592BFRKG4ZB9AWB: a session writes the worktree folder, so
/// it can put a link there to another repository of the person. A
/// sandbox that starts at the link gets no profile: the other
/// repository and its git dir get no write.
#[test]
fn a_link_in_the_worktree_folder_to_another_repository_gives_no_sandbox() {
    let m = Machine::new();
    let other = other_repository(&m);
    // A link in a worktree: the folder has a `.git`, so it looks like a
    // repository of the session.
    let link = m.worktrees().join("issue-1/x");
    std::os::unix::fs::symlink(&other, &link).unwrap();
    refuses_at(&m, &link, &other, "has a link in the worktree folder");
    // A link in the place of a worktree.
    let link = m.worktrees().join("x");
    std::os::unix::fs::symlink(&other, &link).unwrap();
    refuses_at(&m, &link, &other, "is a link or not a folder");
}

/// 01M4EPNY387PPG93H7HYNZ07N5: a `.git` file in a worktree that names
/// the git dir of another repository gives no profile.
#[test]
fn a_git_file_that_names_another_clone_gives_no_sandbox() {
    let m = Machine::new();
    let other = other_repository(&m);
    let tree = m.worktrees().join("issue-1/y");
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(
        tree.join(".git"),
        format!("gitdir: {}\n", other.join(".git").display()),
    )
    .unwrap();
    refuses_at(&m, &tree, &other, "the git dir is of the clone");
}

/// 01M4EPNYARVEJXA5419QGQMHD5: a worktree of the clone in a folder below
/// a worktree is not one folder of the worktree folder: no profile. The
/// clone of the person gets no write.
#[test]
fn a_worktree_below_a_worktree_gives_no_sandbox() {
    let m = Machine::new();
    let deep = m.worktrees().join("issue-1/sub");
    std::fs::create_dir_all(&deep).unwrap();
    let clone = m.clone();
    std::fs::write(
        deep.join(".git"),
        format!(
            "gitdir: {}\n",
            clone.join(".git/worktrees/issue-1").display()
        ),
    )
    .unwrap();
    refuses_at(&m, &deep, &clone, "is not the clone");
}

/// 01M4DWJ0AQX8N7J9T02VJ0XHF1: the sandbox makes the own state folder
/// of the session, and the wrapper deletes it when the session ends.
#[test]
fn the_own_state_folder_lives_as_long_as_its_session() {
    let m = Machine::new();
    let result = m.worktrees().join("result");
    let claude = m.claude(&format!(
        "echo \"$RIFF_STATE\" > '{0}'; test -d \"$RIFF_STATE\" && echo there >> '{0}'",
        result.display()
    ));
    let out = m.run(&m.clone(), &claude, &[]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    let own = m.env.riff_home().join("state/sessions/w1");
    assert_eq!(
        std::fs::read_to_string(&result).unwrap(),
        format!(
            "{}\nthere\n",
            own.canonicalize().unwrap_or(own.clone()).display()
        )
    );
    assert!(!own.exists(), "the own folder stays after the end");
}

/// 01M4BTB7Q1ZT1WD2NMF6BAVWPB: `riff workers sandbox --show` prints the
/// sandbox of the role here, and what the kernel applies, and runs
/// nothing.
#[test]
fn show_prints_the_sandbox_here() {
    let m = Machine::new();
    let out = m
        .env
        .riff()
        .args(["workers", "sandbox", "--show"])
        .current_dir(m.clone())
        .env("RIFF_SESSION", "w1")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{out:?}");
    assert!(
        text.starts_with("The sandbox of the worker here:\nWrite:\n"),
        "{text}"
    );
    assert!(
        text.contains(&format!("  {}\n", m.worktrees().display())),
        "{text}"
    );
    assert!(
        text.contains("Connect to the TCP ports: 9, "),
        "the dead server of the test is on port 9: {text}"
    );
    assert!(text.contains("the sandbox is on: Landlock ABI"), "{text}");
    assert!(
        !text.contains(&format!("  {}\n", m.home().display())),
        "{text}"
    );
}

/// 01M4D4BZ41AH29KSA9B0VZB8DQ: a worker and a test run read the
/// programs of the cargo home, but not its registry tokens: a planted
/// `credentials.toml` and `credentials` stay unreadable in both.
#[test]
fn no_session_and_no_test_run_reads_the_cargo_registry_tokens() {
    let m = Machine::outside_tmp();
    let plant = |cargo: &Path| {
        for (file, text) in [
            ("credentials.toml", "[registry]\ntoken = \"cio_planted\"\n"),
            ("credentials", "[registry]\ntoken = \"cio_planted\"\n"),
            ("bin/tool", "a tool\n"),
        ] {
            let path = cargo.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
    };
    let tries = |cargo: &Path| {
        format!(
            "r() {{ if cat \"$2\" >/dev/null 2>&1; then echo \"$1 yes\"; else echo \"$1 no\"; fi; }}; \
             r toml {0}/credentials.toml; r old {0}/credentials; r bin {0}/bin/tool",
            cargo.display()
        )
    };

    // A session has the cargo home ~/.cargo.
    let home_cargo = m.home().join(".cargo");
    plant(&home_cargo);
    let result = m.worktrees().join("result");
    let claude = m.claude(&format!(
        "sh -c '{}' > '{}'",
        tries(&home_cargo),
        result.display()
    ));
    let out = m.run(&m.clone(), &claude, &[]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    let got = std::fs::read_to_string(&result).unwrap();
    assert_eq!(got, "toml no\nold no\nbin yes\n");

    // A test run, with a cargo home outside /tmp and /var/tmp: the run
    // has its own of both.
    if std::env::var_os("RIFF_TEST_RUN").is_some() {
        println!("skip: a test run cannot start a test run");
        return;
    }
    let cargo = m.clone().parent().unwrap().join("cargo-home");
    plant(&cargo);
    let out = m
        .env
        .riff()
        .args(["test-run", "--", "sh", "-c", &tries(&cargo)])
        .current_dir(m.worktrees().join("issue-1"))
        .env("CARGO_HOME", &cargo)
        .output()
        .unwrap();
    let got = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if got.contains("bubblewrap") || got.contains("apparmor") {
        println!("skip: this machine has no bubblewrap for a test run: {got}");
        return;
    }
    assert_eq!(got, "toml no\nold no\nbin yes\n");
}

/// A fake `tmux` for the broker of a lead: it writes each call to
/// `log`, and keeps the worker panes, their clone marks and the windows
/// in files next to it.
const FAKE_TMUX: &str = r#"#!/bin/sh
[ "$1" = -L ] && shift 2
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/log"
n=$(grep -c -e '^split-window' -e '^new-window' "$dir/log")
case "$1" in
  list-panes)
    if [ "$2" = "-a" ]; then cat "$dir/workers" 2>/dev/null; else cat "$dir/panes" 2>/dev/null; fi ;;
  list-windows) cat "$dir/windows" 2>/dev/null ;;
  display-message)
    case "$5" in
      '#{@riff-clone}') grep "^$4 " "$dir/clones" 2>/dev/null | cut -d' ' -f2- ;;
      '#{window_id}') echo "@0" ;;
    esac ;;
  new-window) echo "@7 %$((n + 10))" ;;
  split-window) echo "%$((n + 10))" ;;
  set-option)
    case "$2 $5" in
      "-p @riff-session") echo "$4 $6" >> "$dir/workers" ;;
      "-p @riff-clone") echo "$4 $6" >> "$dir/clones" ;;
      "-p @riff") echo "$6" >> "$dir/panes" ;;
      "-w @riff") echo "$4 $6" >> "$dir/windows" ;;
    esac ;;
  kill-pane)
    grep -v "^$3 " "$dir/workers" > "$dir/workers.new"
    mv "$dir/workers.new" "$dir/workers"
    [ -n "$FAKE_TMUX_DONE" ] && : > "$FAKE_TMUX_DONE" ;;
  send-keys)
    case "$*" in
      *"-l Join the riff.") [ -n "$FAKE_TMUX_DONE" ] && : > "$FAKE_TMUX_DONE" ;;
    esac ;;
esac
exit 0
"#;

/// A client of the broker in python: it sends the operation `$1` with
/// the arguments after it, with a pipe as stdout, and prints the reply
/// and the JSON on the pipe. The serde form of an `OsString` is
/// `{"Unix": [bytes]}`.
const ASK: &str = r#"
import array, json, os, socket, sys
broker = socket.socket(fileno=int(os.environ["RIFF_BROKER"]))
mine, theirs = socket.socketpair(socket.AF_UNIX, socket.SOCK_SEQPACKET)
r, w = os.pipe()
args = [{"Unix": list(a.encode())} for a in sys.argv[2:]]
req = json.dumps({"op": sys.argv[1], "args": args, "cwd": os.getcwd(), "env": []})
fds = array.array("i", [theirs.fileno(), 0, w, 2])
broker.sendmsg([req.encode()], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, fds)])
theirs.close()
os.close(w)
out = b""
while True:
    chunk = os.read(r, 65536)
    if not chunk:
        break
    out += chunk
print(mine.recv(1 << 20).decode())
print(out.decode())
"#;

/// 01M4DDWP9XSA14E0YF211XZYKR, 01M4DDWPC693RNWHY7P7XBZ9TB,
/// 01M4DDWPEGBAXKTS0X3THFB8VZ, 01M4DDWPGRM1P4A76GFHPQHMQF,
/// 01M4DDWPN8FADA663TTZSVD698: `riff workers lead` runs a fake
/// `claude` in the sandbox of the lead. It starts a worker and stops it
/// through the broker; the broker refuses a stop of a worker of
/// another clone. A direct connect to the tmux socket, a signal to a
/// process outside, and a write of the MCP config, the rules, the git
/// config or a file of the clone fail.
#[test]
fn a_lead_in_its_sandbox_starts_and_stops_a_worker_through_the_broker() {
    use std::os::unix::fs::MetadataExt;
    let m = Machine::new();
    let home = m.home();
    let rules = home.join(".local/share/riff/rules/lead-t.json");
    std::fs::write(&rules, "{}\n").unwrap();
    // The limit of workers of the machine.
    let out = m
        .env
        .riff()
        .args(["workers", "limit", "2"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    // The fake tmux, with the worker of another clone in the pane %50.
    let fake = m.env.path().join("tmux-bin");
    std::fs::create_dir_all(&fake).unwrap();
    std::fs::write(fake.join("tmux"), FAKE_TMUX).unwrap();
    std::fs::set_permissions(fake.join("tmux"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(fake.join("workers"), "%50 s-other-0000\n").unwrap();
    std::fs::write(fake.join("clones"), "%50 /nowhere/other\n").unwrap();
    // The tmux socket of the person.
    let uid = std::fs::metadata("/proc/self").unwrap().uid();
    let tmp = m.env.path().join("tmux-tmp");
    let socket = tmp.join(format!("tmux-{uid}/riff"));
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    let _server = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let outside = Outside::start();

    let trees = m.worktrees();
    let result = trees.join("result");
    let ask = trees.join("ask.py");
    std::fs::write(&ask, ASK).unwrap();
    let bin = Isolated::shared().riff_path();
    let mcp = home.join(".local/share/riff/given/workers-mcp.json");
    let tries = [
        (
            "tmux-socket",
            format!(
                "python3 -c \"import socket; socket.socket(socket.AF_UNIX).connect('{}')\"",
                socket.display()
            ),
        ),
        ("signal", format!("kill -0 {}", outside.0.id())),
        ("mcp", format!("echo x >> '{}'", mcp.display())),
        ("rules", format!("echo x >> '{}'", rules.display())),
        (
            "git-config",
            format!("echo x >> '{}/.git/config'", m.clone().display()),
        ),
        ("clone", format!("echo x >> '{}/new'", m.clone().display())),
    ];
    let mut body = format!(
        ": > '{r}'\n\
         python3 '{ask}' workers-start 1 > '{r}.start' 2>&1\n\
         pane=$(python3 -c \"import json,sys; print(json.loads(open(sys.argv[1]).read().splitlines()[1])['Ok']['panes'][0]['pane'])\" '{r}.start')\n\
         echo \"started [$pane]\" >> '{r}'\n\
         '{bin}' workers stop %50 >> '{r}.other' 2>&1; echo \"other $?\" >> '{r}'\n\
         '{bin}' workers stop \"$pane\" >> '{r}.stop' 2>&1; echo \"stop $?\" >> '{r}'\n\
         python3 '{ask}' pane-type hello > '{r}.type' 2>&1\n",
        r = result.display(),
        ask = ask.display(),
        bin = bin.display(),
    );
    for (name, command) in &tries {
        body.push_str(&format!(
            "if ( {command} ) >/dev/null 2>&1; then echo '{name} yes'; else echo '{name} no'; fi >> '{}'\n",
            result.display()
        ));
    }
    let claude = m.claude(&body);
    let path = format!("{}:{}", fake.display(), std::env::var("PATH").unwrap());
    let tmux = format!("{},1,0", socket.display());
    let out = m
        .env
        .riff()
        .args(["workers", "lead", "--name", "lead-t"])
        .arg(&claude)
        .current_dir(m.clone())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("PATH", path)
        .env("TMUX_TMPDIR", &tmp)
        .env("TMUX", &tmux)
        .env("TMUX_PANE", "%0")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    let read = |suffix: &str| {
        std::fs::read_to_string(format!("{}{suffix}", result.display())).unwrap_or_default()
    };
    let log = std::fs::read_to_string(fake.join("log")).unwrap_or_default();
    let got = read("");
    let pane = got
        .lines()
        .find_map(|l| l.strip_prefix("started ["))
        .and_then(|l| l.strip_suffix(']'))
        .unwrap_or_default()
        .to_owned();
    assert!(
        pane.starts_with('%'),
        "{got}\nstart: {}\nlog: {log}\n{err}",
        read(".start")
    );
    assert_eq!(
        got,
        format!(
            "started [{pane}]\nother 1\nstop 0\ntmux-socket no\nsignal no\nmcp no\nrules no\ngit-config no\nclone no\n"
        ),
        "start: {}\nother: {}\nstop: {}\nlog: {log}",
        read(".start"),
        read(".other"),
        read(".stop"),
    );
    // The broker started the worker in the tmux of the lead, with the
    // clone mark, and killed only its pane.
    let mark = format!(
        "set-option -p -t {pane} @riff-clone {}",
        m.clone().display()
    );
    assert!(log.contains(&mark), "{log}");
    assert!(log.contains(&format!("kill-pane -t {pane}")), "{log}");
    assert!(!log.contains("kill-pane -t %50"), "{log}");
    // The broker types only into the pane of the lead.
    assert!(log.contains("send-keys -t %0 -l hello"), "{log}");
    let other = read(".other");
    assert!(other.contains("no worker runs in the pane %50"), "{other}");
    let stop = read(".stop");
    assert!(stop.contains("Stopped 1"), "{stop}");
    assert_eq!(
        std::fs::read_to_string(fake.join("workers")).unwrap(),
        "%50 s-other-0000\n"
    );
}

/// A riff-server on loopback, in a runtime of its own, with the lead
/// `l1` and a running riff.
struct Server {
    base: String,
    rt: tokio::runtime::Runtime,
    _service: riff_server::Service,
}

impl Server {
    fn start(m: &Machine) -> Self {
        use riff_server::auth::Config;
        use riff_server::store::Memory;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let (service, base) = rt.block_on(async {
            let mut config = Config::default();
            config.lease.wait = std::time::Duration::from_millis(10);
            let store = std::sync::Arc::new(Memory::default());
            let service = riff_server::Service::load(config, store).await.unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let router = service.router();
            tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            (service, format!("http://{addr}"))
        });
        let server = Server {
            base,
            rt,
            _service: service,
        };
        let lead = server.uri(m, "l1");
        let api = server.api();
        server.rt.block_on(async {
            api.register(&lead).await.unwrap();
            api.set_riff(&lead, riff_core::wire::RiffState::Running)
                .await
                .unwrap();
        });
        server
    }

    fn api(&self) -> riff::api::Api {
        riff::api::Api::new(&self.base)
    }

    /// The URI of the session `id` of mike on pangolin in the main clone.
    fn uri(&self, m: &Machine, id: &str) -> riff_core::name::SessionUri {
        let place = riff::identity::place_in(&m.clone(), "pangolin").unwrap();
        riff_core::name::SessionUri::new(
            riff_core::name::Who::new("mike", Some(id)).unwrap(),
            place,
        )
    }

    /// The worker `id` starts and claims `issue-1`.
    fn worker_claims(&self, m: &Machine, id: &str) {
        let w = self.uri(m, id);
        let api = self.api();
        self.rt.block_on(async {
            api.start(&w, riff_core::wire::StartReason::Process, true)
                .await
                .unwrap();
            let thread = w.default_thread().unwrap();
            api.claim(&w, &thread, "issue-1").await.unwrap();
        });
    }

    /// The live sessions, as the lead sees them.
    fn who(&self, m: &Machine) -> String {
        let lead = self.uri(m, "l1");
        let api = self.api();
        let sessions = self.rt.block_on(api.who(&lead, false)).unwrap();
        format!("{sessions:?}")
    }

    /// The history of the repository thread, as the lead reads it.
    fn history(&self, m: &Machine) -> String {
        let out = m
            .env
            .riff()
            .args(["read", "--all"])
            .current_dir(m.clone())
            .env("RIFF_SERVER", &self.base)
            .env("RIFF_SESSION", "l1")
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

/// The worker `id` in the pane `%5` runs a fake `claude` in its sandbox
/// with `riff workers sandbox`, with the limit of workers `limit` and
/// the worker panes `workers` in a fake tmux. The fake releases
/// `issue-1`, runs the Stop hook, and waits until the fake tmux got the
/// start prompt or closed a pane. Gives the log of the fake tmux, and
/// the output of the fake.
fn worker_ends_its_item(
    m: &Machine,
    server: &Server,
    id: &str,
    limit: u16,
    workers: &str,
) -> (String, String) {
    let home = m.home();
    std::fs::write(
        home.join(format!(".local/share/riff/rules/{id}.json")),
        "{}\n",
    )
    .unwrap();
    let out = m
        .env
        .riff()
        .args(["workers", "limit", &limit.to_string()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let fake = m.env.path().join("tmux-bin");
    std::fs::create_dir_all(&fake).unwrap();
    std::fs::write(fake.join("tmux"), FAKE_TMUX).unwrap();
    std::fs::set_permissions(fake.join("tmux"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(fake.join("workers"), workers).unwrap();
    server.worker_claims(m, id);

    let trees = m.worktrees();
    let result = trees.join("result");
    let done = trees.join("done");
    let bin = Isolated::shared().riff_path();
    let claude = m.claude(&format!(
        "echo \"tmux [$TMUX$TMUX_PANE] broker [${{RIFF_BROKER:+yes}}]\" > '{r}'\n\
         '{bin}' release issue-1 >> '{r}' 2>&1\n\
         printf '{{\"session_id\":\"{id}\",\"hook_event_name\":\"Stop\"}}' | '{bin}' hook stop >> '{r}' 2>&1\n\
         echo \"hook $?\" >> '{r}'\n\
         for i in $(seq 300); do [ -e '{done}' ] && break; sleep 0.1; done\n\
         [ -e '{done}' ] && echo done >> '{r}'\n\
         exit 0",
        bin = bin.display(),
        r = result.display(),
        done = done.display(),
    ));
    let path = format!("{}:{}", fake.display(), std::env::var("PATH").unwrap());
    let out = m
        .env
        .riff()
        .args(["workers", "sandbox", "--role", "worker"])
        .arg(&claude)
        .current_dir(m.clone())
        .env("RIFF_SERVER", &server.base)
        .env("RIFF_SESSION", id)
        .env("RIFF_WORKER", "1")
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("PATH", path)
        .env("TMUX", "/nowhere/tmux-1000/default,1,0")
        .env("TMUX_PANE", "%5")
        .env("FAKE_TMUX_DONE", &done)
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    let got = std::fs::read_to_string(&result).unwrap_or_default();
    let log = std::fs::read_to_string(fake.join("log")).unwrap_or_default();
    assert!(got.ends_with("done\n"), "{got}\nlog: {log}\n{err}");
    // The fake reaches no tmux: it has only its broker.
    assert!(got.starts_with("tmux [] broker [yes]\n"), "{got}");
    (log, got)
}

/// Each `-t` target in the log of the fake tmux.
fn targets(log: &str) -> Vec<String> {
    log.lines()
        .filter_map(|l| l.split(" -t ").nth(1))
        .map(|rest| rest.split(' ').next().unwrap().to_owned())
        .collect()
}

/// 01M4DVW26Q5MX025XBW7JCK44S, 01M4DVW24ESG6XCBNMFV7T9Z4E,
/// 01M4DVW2B8W108N696GAYN3V1B: a worker in its sandbox releases its last
/// claim and its turn ends. It reaches no tmux, but riff types `/clear`
/// and the start prompt into its own pane through its broker, and into
/// no other pane.
#[test]
fn a_worker_in_its_sandbox_gets_clear_in_its_own_pane_through_the_broker() {
    let m = Machine::new();
    let server = Server::start(&m);
    let id = "clear-653";
    let (log, got) = worker_ends_its_item(&m, &server, id, 2, &format!("%5 {id}\n%6 other\n"));
    assert!(got.contains("hook 0"), "{got}");
    assert!(log.contains("send-keys -t %5 -l /clear\n"), "{log}");
    assert!(log.contains("send-keys -t %5 -l Join the riff.\n"), "{log}");
    assert!(!log.contains("kill-pane"), "{log}");
    let targets = targets(&log);
    assert!(targets.iter().all(|t| t == "%5"), "{log}");
}

/// 01M4DVW290H2AQF6EQFHTY1EE1, 01M4DVW2DGX8N9NJZHSAGK5EH4: with limit 1
/// and two workers, a worker in its sandbox that ends its item ends in
/// place of the clear. Its broker closes only its pane, posts the note
/// to the lead, and ends its session after the pane closed.
#[test]
fn a_worker_in_its_sandbox_over_the_limit_ends_through_the_broker() {
    let m = Machine::new();
    let server = Server::start(&m);
    let id = "limit-653";
    let (log, _) = worker_ends_its_item(&m, &server, id, 1, &format!("%5 {id}\n%6 other\n"));
    assert!(log.contains("kill-pane -t %5\n"), "{log}");
    assert!(!log.contains("kill-pane -t %6"), "{log}");
    assert!(!log.contains("send-keys"), "{log}");
    let history = server.history(&m);
    assert!(
        history.contains("limit 1, runs 2 on pangolin: the worker in the pane %5 ends"),
        "{history}"
    );
    let span = isolated::Span::start();
    while server.who(&m).contains(id) {
        assert!(
            span.within(std::time::Duration::from_secs(20)),
            "the session did not end: {}",
            server.who(&m)
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
