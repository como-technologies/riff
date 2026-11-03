//! One test environment for each run of `riff` and `riff-server` in a
//! test (01M3MY2KWKBJCQ0BCNC6533RBW).
//!
//! # Design
//!
//! A test never touches the riff of the machine: its sign-in, its
//! keyring entries, its device key, its settings, its local files, or
//! the shared server. So each integration test runs the binaries only
//! through an [`Isolated`] environment:
//!
//! | Variable | Value |
//! |---|---|
//! | Each `RIFF_…` and `CLAUDE_…` variable | removed |
//! | `GH_TOKEN`, `GITHUB_TOKEN` | removed: the `gh` of the machine has no sign-in in a test, so no test reaches the forge |
//! | `RIFF_SERVER` | [`DEAD_SERVER`]: nothing listens there |
//! | `RIFF_RELEASES` | an empty dir of its own: no release is Latest and no release has static binaries, so `riff update` reaches no GitHub, and runs the fake `cargo` of a test (01M4ZPJ0DJ9FTYNPTBBVJCTB37, 01M510MPR1S0EQNRK2195XTXG5) |
//! | `RIFF_MACHINE` | [`MACHINE`]: the numbers of a machine with free memory and no load, so no test depends on the machine that runs it (01M3WFZ08D8VT9KD6HXY09NHSE) |
//! | `RIFF_ON` | `1`: riff is on in each directory, so a test of a hook needs no repository with `riff enable`. A test of the state removes it (01M3XY2SWEK0N8MC3MY4TMYTD3) |
//! | `RIFF_DYNAMIC` | `1`: `riff init` and the daemon take the riff under test, which is not static, because a fake `podman` runs `riff relay` on the host. A test of the check removes it (01M510NWEFSN0NWQH8KQ6FA7DC) |
//! | `RIFF_HOME` | a dir of its own: riff keeps its settings, local files and secrets there, and never opens the OS keyring |
//! | `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_RUNTIME_DIR`, `TMPDIR` | dirs of its own |
//! | `DBUS_SESSION_BUS_ADDRESS` | a bus that does not exist, so each call to the OS keyring fails |
//! | `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM` | `/dev/null` and `1`: git reads no config of the machine |
//! | `GIT_ALLOW_PROTOCOL` | `file`: git reaches no remote on the network (01M3WG82ZMQYG1TGHE4ET0BDDW) |
//!
//! Each dir is in one temp dir, which goes when the [`Isolated`] value
//! drops. A test sets its own variables after the helper, so its
//! values win.
//!
//! The `TMPDIR` of a worker is under the home of the person, and a home
//! can be a git repository. A plain temp dir then is in that repository,
//! and a test of "no repository" finds it, or writes to it, for example
//! `riff enable` in a dir of the test. Three guards keep each test out
//! of it:
//!
//! - `.cargo/config.toml` sets `TMPDIR` to `/var/tmp` for each cargo
//!   run in the repository, also a plain `cargo test`
//!   (01M49NP2907J4SH4S6MAY09VXE).
//! - The temp dir of an [`Isolated`] is outside each git repository,
//!   also when cargo runs from another dir (01M49NP2JW8JFWYY56K7AK3H05).
//! - A test that needs a place outside each git repository gets its
//!   temp dir from [`outside_git`] (01M43B48Z8R0SXAWBQP75CPR55).
//!
//! A container of riff sets `commit.gpgsign=false` in `GIT_CONFIG_COUNT`,
//! `GIT_CONFIG_KEY_0` and `GIT_CONFIG_VALUE_0`, and code under test that
//! runs git in the test process takes them. A test of such code starts
//! with [`rerun_without_git_config`], and a doc test with
//! [`clear_git_config_env`] (01M6FYHS06W0Y98GQTQY6Q37F0).
//!
//! A check of a time in a test uses a [`Span`]: it fails only when the
//! logic is slow, not when the machine is busy. See [`time`].
//!
//! A test writes a fake program with [`write_program`]: a child process
//! writes it, so its run cannot fail with `ETXTBSY`. See [`program`].
//!
//! [`offenders`] finds each test file that names a binary of riff
//! without the helper, and [`program_writers`] each test file that
//! writes a program in the test process. A test of this crate fails on
//! each one.
//!
//! # Example
//!
//! ```no_run
//! use isolated::Isolated;
//!
//! let env = Isolated::new();
//! let out = env.riff().arg("--help").output().unwrap();
//! assert!(out.status.success());
//! ```

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use tempfile::TempDir;

pub mod program;
pub mod time;

pub use program::{
    copy_program, file_name, program_writers, program_writes, write_program, writes_a_program,
};
pub use time::{
    HANG, Late, PRESSURE_DIR, PRESSURE_LIMIT, Pressure, RESOURCES, Span, in_time, stall,
};

/// The prefixes of the variables that the helper removes: riff and the
/// agent tool read them.
pub const REMOVED_PREFIXES: [&str; 2] = ["RIFF_", "CLAUDE_"];

/// The server of each test that names no server of its own: nothing
/// listens there (the discard port). So a test never reaches the riff of
/// the machine on 127.0.0.1:7878, or the shared riff.
pub const DEAD_SERVER: &str = "http://127.0.0.1:9";

/// The numbers of the machine of each test that gives no numbers of its
/// own: 8 cores, 32 GB of memory, all of it available, and no load.
pub const MACHINE: &str = "cpu 8x3000MHz, mem 32GB, 32GB available, load 0.00";

/// The other variables that the helper removes: the tokens of `gh`. With `HOME` in a temp dir, `gh` then has no
/// sign-in, so a test never reaches the forge.
pub const REMOVED: [&str; 2] = ["GH_TOKEN", "GITHUB_TOKEN"];

/// A test environment in a temp dir of its own.
pub struct Isolated {
    dir: TempDir,
}

impl Default for Isolated {
    fn default() -> Self {
        Self::new()
    }
}

impl Isolated {
    /// Makes a new temp dir with each dir of the environment, outside
    /// each git repository (01M49NP2JW8JFWYY56K7AK3H05).
    pub fn new() -> Isolated {
        Isolated::in_roots(&temp_roots())
    }

    /// Makes the environment in the first of `roots` that is a dir
    /// outside each git repository, as [`outside_git_from`] does.
    ///
    /// ```
    /// let home = tempfile::tempdir()?;
    /// std::fs::create_dir_all(home.path().join(".git"))?;
    /// let clean = isolated::outside_git();
    ///
    /// let env = isolated::Isolated::in_roots(&[home.path().into(), clean.path().into()]);
    /// assert!(env.path().starts_with(clean.path()));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn in_roots(roots: &[PathBuf]) -> Isolated {
        Isolated::in_dir(outside_git_from(roots))
    }

    /// One environment for the whole test process, for a helper that
    /// returns a command. Its dir is in `target/tmp`, and stays after
    /// the process ends.
    pub fn shared() -> &'static Isolated {
        static SHARED: LazyLock<Isolated> = LazyLock::new(|| {
            let exe = std::env::current_exe().expect("the path of the test");
            // target/debug/deps/TEST: target/tmp holds the dir.
            let target = exe.ancestors().nth(3).expect("the target dir");
            let tmp = target.join("tmp");
            std::fs::create_dir_all(&tmp).expect("target/tmp");
            // A static never drops, so the dir stays.
            Isolated::in_dir(
                tempfile::Builder::new()
                    .prefix("isolated-")
                    .tempdir_in(tmp)
                    .expect("a temp dir"),
            )
        });
        &SHARED
    }

    fn in_dir(dir: TempDir) -> Isolated {
        let env = Isolated { dir };
        for dir in [
            env.home().join(".config"),
            env.home().join(".local/share"),
            env.home().join(".local/state"),
            env.path().join("run"),
            env.path().join("tmp"),
            env.releases(),
            env.riff_home(),
        ] {
            std::fs::create_dir_all(dir).expect("a dir of the test environment");
        }
        env
    }

    /// The temp dir that holds each dir of the environment.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// `HOME` of the environment.
    pub fn home(&self) -> PathBuf {
        self.path().join("home")
    }

    /// `RIFF_HOME` of the environment.
    pub fn riff_home(&self) -> PathBuf {
        self.path().join("riff")
    }

    /// `RIFF_RELEASES` of the environment: the release files of
    /// `riff update`, at `TAG/NAME`. It is empty.
    pub fn releases(&self) -> PathBuf {
        self.path().join("releases")
    }

    /// Each variable of the environment: `Some` sets it, `None` removes
    /// it.
    pub fn vars(&self) -> Vec<(OsString, Option<OsString>)> {
        let mut vars: Vec<(OsString, Option<OsString>)> = std::env::vars_os()
            .map(|(name, _)| name)
            .filter(|name| removed(name))
            .map(|name| (name, None))
            .collect();
        let path = |p: PathBuf| Some(p.into_os_string());
        let home = self.home();
        vars.extend([
            ("RIFF_SERVER".into(), Some(DEAD_SERVER.into())),
            ("RIFF_RELEASES".into(), path(self.releases())),
            ("RIFF_MACHINE".into(), Some(MACHINE.into())),
            ("RIFF_ON".into(), Some("1".into())),
            ("RIFF_DYNAMIC".into(), Some("1".into())),
            ("RIFF_HOME".into(), path(self.riff_home())),
            ("HOME".into(), path(home.clone())),
            ("XDG_CONFIG_HOME".into(), path(home.join(".config"))),
            ("XDG_DATA_HOME".into(), path(home.join(".local/share"))),
            ("XDG_STATE_HOME".into(), path(home.join(".local/state"))),
            ("XDG_RUNTIME_DIR".into(), path(self.path().join("run"))),
            ("TMPDIR".into(), path(self.path().join("tmp"))),
            (
                "DBUS_SESSION_BUS_ADDRESS".into(),
                Some(format!("unix:path={}", self.path().join("no-bus").display()).into()),
            ),
            ("GIT_CONFIG_GLOBAL".into(), Some("/dev/null".into())),
            ("GIT_CONFIG_NOSYSTEM".into(), Some("1".into())),
            ("GIT_ALLOW_PROTOCOL".into(), Some("file".into())),
        ]);
        vars
    }

    /// A command that runs `program` in the environment. Use it for a
    /// program that runs riff, for example a script or `just`.
    pub fn command(&self, program: impl AsRef<OsStr>) -> std::process::Command {
        let mut cmd = std::process::Command::new(program);
        cmd.isolate(self);
        cmd
    }

    /// The path of the `riff` binary under test. Run it only in the
    /// environment, for example with [`Isolated::command`].
    pub fn riff_path(&self) -> PathBuf {
        assert_cmd::cargo::cargo_bin("riff")
    }

    /// The path of the `riff-server` binary under test. Run it only in
    /// the environment.
    pub fn riff_server_path(&self) -> PathBuf {
        assert_cmd::cargo::cargo_bin("riff-server")
    }

    /// `riff` in the environment.
    pub fn riff(&self) -> std::process::Command {
        self.command(self.riff_path())
    }

    /// `riff-server` in the environment.
    pub fn riff_server(&self) -> std::process::Command {
        self.command(self.riff_server_path())
    }

    /// `riff` in the environment, for `assert_cmd`.
    pub fn assert_riff(&self) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::new(self.riff_path());
        cmd.isolate(self);
        cmd
    }

    /// `riff-server` in the environment, for `assert_cmd`.
    pub fn assert_riff_server(&self) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::new(self.riff_server_path());
        cmd.isolate(self);
        cmd
    }

    /// `riff` in the environment, for tokio.
    pub fn tokio_riff(&self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(self.riff_path());
        cmd.isolate(self);
        cmd
    }
}

/// True when the helper removes the variable `name`.
///
/// ```
/// assert!(isolated::removed("RIFF_SERVER".as_ref()));
/// assert!(isolated::removed("CLAUDE_CODE_SESSION_ID".as_ref()));
/// assert!(isolated::removed("GH_TOKEN".as_ref()));
/// assert!(isolated::removed("GITHUB_TOKEN".as_ref()));
/// assert!(!isolated::removed("PATH".as_ref()));
/// ```
pub fn removed(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    REMOVED_PREFIXES.iter().any(|p| name.starts_with(p)) || REMOVED.contains(&name.as_ref())
}

/// True when git reads config from the variable `name`:
/// `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_N` or `GIT_CONFIG_VALUE_N`.
///
/// ```
/// assert!(isolated::git_config_var("GIT_CONFIG_COUNT".as_ref()));
/// assert!(isolated::git_config_var("GIT_CONFIG_KEY_0".as_ref()));
/// assert!(isolated::git_config_var("GIT_CONFIG_VALUE_12".as_ref()));
/// assert!(!isolated::git_config_var("GIT_CONFIG_KEY_".as_ref()));
/// assert!(!isolated::git_config_var("GIT_CONFIG_GLOBAL".as_ref()));
/// assert!(!isolated::git_config_var("GIT_CONFIG_NOSYSTEM".as_ref()));
/// ```
pub fn git_config_var(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    let number = |rest: &str| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit());
    name == "GIT_CONFIG_COUNT"
        || ["GIT_CONFIG_KEY_", "GIT_CONFIG_VALUE_"]
            .iter()
            .any(|prefix| name.strip_prefix(prefix).is_some_and(number))
}

/// Removes each [`git_config_var`] from the environment of this process,
/// and gives their names (01M6FYHS06W0Y98GQTQY6Q37F0).
///
/// A container of riff sets `commit.gpgsign=false` in them, and code under
/// test that runs git takes them from the process. Call it first in a doc
/// test: each doc test is a process of its own. Never call it in a test
/// binary with other tests: they run at the same time, and some of them
/// need the variables. Such a test calls [`rerun_without_git_config`].
///
/// ```
/// // SAFETY: a doc test is one process, and it has one thread here.
/// unsafe {
///     std::env::set_var("GIT_CONFIG_COUNT", "1");
///     std::env::set_var("GIT_CONFIG_KEY_0", "commit.gpgsign");
///     std::env::set_var("GIT_CONFIG_VALUE_0", "false");
/// }
/// let mut cleared = isolated::clear_git_config_env();
/// cleared.sort();
/// assert_eq!(cleared, ["GIT_CONFIG_COUNT", "GIT_CONFIG_KEY_0", "GIT_CONFIG_VALUE_0"]);
/// assert!(std::env::vars_os().all(|(name, _)| !isolated::git_config_var(&name)));
/// // The next call finds nothing.
/// assert!(isolated::clear_git_config_env().is_empty());
/// ```
pub fn clear_git_config_env() -> Vec<OsString> {
    let names: Vec<OsString> = std::env::vars_os()
        .map(|(name, _)| name)
        .filter(|name| git_config_var(name))
        .collect();
    for name in &names {
        // SAFETY: std holds one lock for the environment, and it holds the
        // lock when it starts a child. A doc test calls this before its first
        // git call, and no other code of a doc test reads these variables.
        unsafe { std::env::remove_var(name) };
    }
    names
}

/// The variable that tells a test that [`rerun_without_git_config`] runs it
/// for the second time.
const RERUN: &str = "ISOLATED_RERUN";

/// The name that the test harness gives to the test `test` of the module
/// `module` (from `module_path!()`): the module with no crate name, then
/// the test.
///
/// ```
/// assert_eq!(
///     isolated::test_name("riff::daemon::sweep::tests", "the_sweep"),
///     "daemon::sweep::tests::the_sweep"
/// );
/// assert_eq!(isolated::test_name("all::save", "a_clone"), "save::a_clone");
/// assert_eq!(isolated::test_name("git_config", "a_test"), "a_test");
/// ```
pub fn test_name(module: &str, test: &str) -> String {
    match module.split_once("::") {
        Some((_, path)) => format!("{path}::{test}"),
        None => test.to_owned(),
    }
}

/// The command that runs the one test `name` of the test binary `exe`
/// again, with each [`git_config_var`] of `vars` removed and the marker of
/// the second run set.
///
/// ```
/// let cmd = isolated::rerun_command(
///     "/target/deps/all".as_ref(),
///     "save::a_clone",
///     ["PATH", "GIT_CONFIG_COUNT", "GIT_CONFIG_KEY_0", "GIT_CONFIG_NOSYSTEM"].map(Into::into),
/// );
/// let removed: Vec<_> = cmd.get_envs().filter(|(_, v)| v.is_none()).map(|(k, _)| k).collect();
/// assert_eq!(removed, ["GIT_CONFIG_COUNT", "GIT_CONFIG_KEY_0"]);
/// let args: Vec<_> = cmd.get_args().collect();
/// assert_eq!(args, ["save::a_clone", "--exact", "--test-threads=1"]);
/// ```
pub fn rerun_command(
    exe: &Path,
    name: &str,
    vars: impl IntoIterator<Item = OsString>,
) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    cmd.args([name, "--exact", "--test-threads=1"])
        .env(RERUN, "1");
    for var in vars.into_iter().filter(|var| git_config_var(var)) {
        cmd.env_remove(var);
    }
    cmd
}

/// For a test whose code under test runs git in the test process
/// (01M6FYHS06W0Y98GQTQY6Q37F0): in a test binary with other tests, the
/// variables of git in the process stay, because other tests need them.
/// So the test runs again, alone, in a child process of its own with no
/// [`git_config_var`] in its environment. The call in the first run gives
/// `false` after the child passed, and the test returns. In the child it
/// gives `true`, and the test goes on. It fails when the child fails, or
/// when it ran no test (a wrong name).
///
/// `module` is `module_path!()`, and `test` the name of the test function.
///
/// ```no_run
/// fn a_host_whose_git_signs_each_commit_gets_the_wip_commit() {
///     if !isolated::rerun_without_git_config(module_path!(), "a_host_whose_git_signs_each_commit_gets_the_wip_commit") {
///         return;
///     }
///     // From here, the process has no `GIT_CONFIG_COUNT`.
/// }
/// ```
pub fn rerun_without_git_config(module: &str, test: &str) -> bool {
    if std::env::var_os(RERUN).is_some() {
        return true;
    }
    let name = test_name(module, test);
    let exe = std::env::current_exe().expect("the path of the test binary");
    let vars = std::env::vars_os().map(|(var, _)| var);
    let out = rerun_command(&exe, &name, vars)
        .output()
        .expect("the test binary runs again");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success() && text.contains("1 passed"),
        "the test {name} failed when it ran again with no git config:\n{text}"
    );
    false
}

/// A command that can run in an [`Isolated`] environment.
pub trait Isolate {
    /// Sets and removes each variable of `env` (see [`Isolated::vars`]).
    fn isolate(&mut self, env: &Isolated) -> &mut Self;
}

macro_rules! isolate {
    ($t:ty) => {
        impl Isolate for $t {
            fn isolate(&mut self, env: &Isolated) -> &mut Self {
                for (name, value) in env.vars() {
                    match value {
                        Some(value) => self.env(name, value),
                        None => self.env_remove(name),
                    };
                }
                self
            }
        }
    };
}

isolate!(std::process::Command);
isolate!(assert_cmd::Command);
isolate!(tokio::process::Command);

/// Each line of a test file under `root/crates/*/tests` that names a
/// binary of riff without the helper, as `PATH:LINE`. The helper crate
/// itself does not count.
///
/// ```
/// let root = tempfile::tempdir()?;
/// let tests = root.path().join("crates/riff/tests");
/// std::fs::create_dir_all(&tests)?;
/// std::fs::write(tests.join("good.rs"), "let env = Isolated::new();\nenv.riff();\n")?;
/// std::fs::write(tests.join("bad.rs"), "\nCommand::cargo_bin(\"riff\")\n")?;
/// assert_eq!(isolated::offenders(root.path()), ["crates/riff/tests/bad.rs:2"]);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn offenders(root: &Path) -> Vec<String> {
    test_lines(root, |text| {
        let lines = text.lines().enumerate();
        lines
            .filter(|(_, line)| names_a_binary(line))
            .map(|(n, _)| n + 1)
            .collect()
    })
}

/// Each line of a test file under `root/crates/*/tests` that `lines`
/// gives for the text of the file, as `PATH:LINE`. `lines` gives the
/// numbers of the lines from 1. The helper crate itself does not count.
fn test_lines(root: &Path, lines: impl Fn(&str) -> Vec<usize>) -> Vec<String> {
    let mut files = Vec::new();
    let Ok(crates) = std::fs::read_dir(root.join("crates")) else {
        return Vec::new();
    };
    for krate in crates.flatten() {
        if krate.file_name() != env!("CARGO_PKG_NAME") {
            rust_files(&krate.path().join("tests"), &mut files);
        }
    }
    files.sort();
    let mut found = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let name = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .display()
            .to_string();
        found.extend(lines(&text).into_iter().map(|n| format!("{name}:{n}")));
    }
    found
}

/// The roots that [`outside_git`] tries, in order: the temp dir of the
/// process, then the temp dirs of the system, on disk first. `just test`
/// tries the same roots.
pub fn temp_roots() -> Vec<PathBuf> {
    let mut roots = vec![std::env::temp_dir()];
    roots.extend(["/var/tmp", "/tmp", "/dev/shm"].map(PathBuf::from));
    roots
}

/// A new temp dir with no git repository at it or above it
/// (01M43B48Z8R0SXAWBQP75CPR55). It is in the first of [`temp_roots`]
/// that is outside each repository.
///
/// # Panics
///
/// When each root is in a git repository.
pub fn outside_git() -> TempDir {
    outside_git_from(&temp_roots())
}

/// A new temp dir in the first of `roots` that is a dir outside each git
/// repository.
///
/// ```
/// let home = tempfile::tempdir()?;
/// std::fs::create_dir_all(home.path().join(".git"))?;
/// let tmp = home.path().join("tmp");
/// std::fs::create_dir_all(&tmp)?;
/// let clean = isolated::outside_git();
///
/// let dir = isolated::outside_git_from(&[tmp, clean.path().into()]);
/// assert!(dir.path().starts_with(clean.path()));
/// assert!(!isolated::in_git(dir.path()));
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// # Panics
///
/// When each root is in a git repository, or is not a dir.
pub fn outside_git_from(roots: &[PathBuf]) -> TempDir {
    let root = roots
        .iter()
        .find(|root| root.is_dir() && !in_git(root))
        .unwrap_or_else(|| panic!("each temp root is in a git repository: {roots:?}"));
    tempfile::tempdir_in(root).expect("a temp dir")
}

/// True when `dir`, or a dir above it, holds `.git`: a git repository
/// or a linked worktree. The path counts as the OS resolves it.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let sub = dir.path().join("a/b");
/// std::fs::create_dir_all(&sub)?;
/// std::fs::create_dir_all(dir.path().join(".git"))?;
/// assert!(isolated::in_git(&sub));
/// assert!(!isolated::in_git(isolated::outside_git().path()));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn in_git(dir: &Path) -> bool {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_owned());
    dir.ancestors().any(|d| d.join(".git").exists())
}

/// True when a line of a test names a binary of riff directly.
///
/// ```
/// use isolated::names_a_binary;
/// assert!(names_a_binary(r#"Command::new(env!("CARGO_BIN_EXE_riff"))"#));
/// assert!(names_a_binary(r#"Command::cargo_bin("riff-server")"#));
/// assert!(names_a_binary(r#"assert_cmd::cargo::cargo_bin("riff")"#));
/// assert!(!names_a_binary(r#"env.riff().arg("who")"#));
/// assert!(!names_a_binary(r#"Command::cargo_bin("reqs")"#));
/// ```
pub fn names_a_binary(line: &str) -> bool {
    line.contains("CARGO_BIN_EXE_riff") || (line.contains("cargo_bin") && line.contains("\"riff"))
}

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
}
