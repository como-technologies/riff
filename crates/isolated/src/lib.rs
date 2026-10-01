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
//! | Each `RIFF_…` and `CLAUDE_…` variable, `TMUX`, `TMUX_PANE` | removed |
//! | `RIFF_SERVER` | [`DEAD_SERVER`]: nothing listens there |
//! | `RIFF_MACHINE` | [`MACHINE`]: the numbers of a machine with free memory and no load, so no test depends on the machine that runs it (01M3WFZ08D8VT9KD6HXY09NHSE) |
//! | `RIFF_HOME` | a dir of its own: riff keeps its settings, local files and secrets there, and never opens the OS keyring |
//! | `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_RUNTIME_DIR`, `TMPDIR` | dirs of its own |
//! | `DBUS_SESSION_BUS_ADDRESS` | a bus that does not exist, so each call to the OS keyring fails |
//! | `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM` | `/dev/null` and `1`: git reads no config of the machine |
//!
//! Each dir is in one temp dir, which goes when the [`Isolated`] value
//! drops. A test sets its own variables after the helper, so its
//! values win.
//!
//! [`offenders`] finds each test file that names a binary of riff
//! without the helper. A test of this crate fails on each one.
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

/// The other variables that the helper removes: the tmux of the person.
pub const REMOVED: [&str; 2] = ["TMUX", "TMUX_PANE"];

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
    /// Makes a new temp dir with each dir of the environment.
    pub fn new() -> Isolated {
        Isolated::in_dir(tempfile::tempdir().expect("a temp dir"))
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
            ("RIFF_MACHINE".into(), Some(MACHINE.into())),
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
/// assert!(isolated::removed("TMUX".as_ref()));
/// assert!(!isolated::removed("PATH".as_ref()));
/// ```
pub fn removed(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    REMOVED_PREFIXES.iter().any(|p| name.starts_with(p)) || REMOVED.contains(&name.as_ref())
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
        for (n, line) in text.lines().enumerate() {
            if names_a_binary(line) {
                found.push(format!("{name}:{}", n + 1));
            }
        }
    }
    found
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
