//! The temp folder of each worker, on disk.
//!
//! # Design
//!
//! On a machine where `/tmp` is a tmpfs, each file in it uses memory.
//! Claude Code puts the scratch directory of each session under
//! `CLAUDE_CODE_TMPDIR` (else `/tmp`), and cargo tests put their temp
//! files under `TMPDIR`. A worker builds riff binaries there for its
//! live checks, up to some GB for one session. So each worker gets a
//! temp folder of its own on disk, and riff deletes it when the worker
//! ends (01M41VAGJC69S9R2TD1B1EQ4W4, 01M41VAGQ2VA2Q0VSFJNG4H08W).
//!
//! | Folder | Path |
//! |---|---|
//! | The root | `workers.tmp`, else `$RIFF_HOME/tmp`, else `$XDG_CACHE_HOME/riff/tmp`, else `~/.cache/riff/tmp` ([`root`]) |
//! | The folder of the worker `ID` | `ROOT/ID` ([`of`]) |
//!
//! `riff workers run` sets `TMPDIR` and `CLAUDE_CODE_TMPDIR` to the
//! folder of its worker. It also puts both in the `env` of the flag
//! settings of `claude` ([`with_env`]): the `env` of the user settings
//! of Claude Code replaces a variable of the process, and the flag
//! settings win over the user settings.
//!
//! ```mermaid
//! flowchart TD
//!     R["riff workers run"] --> M["make ROOT/ID, give claude TMPDIR and CLAUDE_CODE_TMPDIR"]
//!     M --> C["claude works"]
//!     C --> X{"how does the context end?"}
//!     X -- "the clear" --> P["delete each part of ROOT/ID<br/>that no process holds, keep tasks"]
//!     P --> C
//!     X -- "claude ends, riff workers stop" --> D["delete ROOT/ID<br/>when no process uses it"]
//!     X -- "the pane dies" --> T["the next tidy deletes ROOT/ID<br/>when no process uses it"]
//! ```
//!
//! - A process uses a folder when its working directory or one of its
//!   open files is in the folder, or when its `TMPDIR` or
//!   `CLAUDE_CODE_TMPDIR` is in it ([`User::uses`]). riff never deletes a
//!   folder that a live process uses (01M41VAGMQPBDPV8XPGEKYRXZZ).
//! - The end of a worker ([`end_session`]): the wrapper deletes the
//!   folder when `claude` ended, and `riff workers stop` after it
//!   stopped each process of the worker (01M41VAGQ2VA2Q0VSFJNG4H08W).
//!   A child of `claude` that lives on keeps the folder. The next tidy
//!   deletes it.
//! - The clear ([`end_context`]): `claude` lives on, so its variables
//!   do not count. riff deletes each file and folder that no process
//!   holds open or works in. It keeps each folder `tasks`: Claude Code
//!   writes the output of the background tasks of the whole process
//!   there, also of the watch after the clear
//!   (01M41VAGSCNESHTZ6216P2E133).
//! - Each tidy ([`sweep`]) deletes each folder of the root that no
//!   process uses and that is older than [`YOUNG`]: the folder of a
//!   worker whose pane died with its wrapper
//!   (01M41VAGVR2PPVAYDN0SWK2F02).
//! - `riff workers` and `riff workers tmp` show the disk use of the root
//!   on this machine ([`size`], 01M41VAGY396K07BTPSW9TNBX5).
//!
//! ```
//! use riff::temp::{User, end_session};
//!
//! let root = tempfile::tempdir()?;
//! let (ended, live) = (root.path().join("w1"), root.path().join("w2"));
//! std::fs::create_dir_all(ended.join("claude-1000/p/s1/scratchpad"))?;
//! std::fs::create_dir_all(&live)?;
//! // The claude of w2 runs with TMPDIR in its folder.
//! let users = [User { pid: 7, cwd: None, files: vec![], tmp: vec![live.clone()] }];
//! assert!(end_session(&ended, &users));
//! assert!(!end_session(&live, &users));
//! assert!(!ended.exists() && live.exists());
//! # Ok::<(), std::io::Error>(())
//! ```

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Result;

/// The variables of the temp folder that a worker gets.
pub const VARS: [&str; 2] = ["TMPDIR", "CLAUDE_CODE_TMPDIR"];

/// The folder that the clear keeps: Claude Code writes the output of
/// background tasks there (01M41VAGSCNESHTZ6216P2E133).
pub const KEEP: &str = "tasks";

/// A tidy keeps a folder that changed in this time: the wrapper made it
/// just now, and `claude` does not run yet (01M41VAGVR2PPVAYDN0SWK2F02).
pub const YOUNG: Duration = Duration::from_secs(60);

/// The root of the temp folders with no setting: `$RIFF_HOME/tmp`, else
/// `$XDG_CACHE_HOME/riff/tmp`, else `$HOME/.cache/riff/tmp`. An empty
/// value counts as unset. `None` with none of them.
///
/// ```
/// use riff::temp::root_from;
/// use std::path::PathBuf;
///
/// let some = |s: &str| Some(s.into());
/// assert_eq!(root_from(some("/r"), some("/c"), some("/h")), Some(PathBuf::from("/r/tmp")));
/// assert_eq!(root_from(None, some("/c"), some("/h")), Some(PathBuf::from("/c/riff/tmp")));
/// assert_eq!(root_from(some(""), None, some("/h")), Some(PathBuf::from("/h/.cache/riff/tmp")));
/// assert_eq!(root_from(None, None, None), None);
/// ```
pub fn root_from(
    home: Option<std::ffi::OsString>,
    cache: Option<std::ffi::OsString>,
    user: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let set = |v: Option<std::ffi::OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(home) = set(home) {
        return Some(home.join("tmp"));
    }
    set(cache)
        .or_else(|| set(user).map(|h| h.join(".cache")))
        .map(|c| c.join("riff").join("tmp"))
}

/// The root of the temp folders of this machine: `workers.tmp` in the
/// settings `path`, else [`root_from`] of this process
/// (01M41VAGJC69S9R2TD1B1EQ4W4).
pub fn root(path: &Path) -> Result<PathBuf> {
    if let Some(dir) = crate::settings::workers_tmp(path)? {
        return Ok(dir);
    }
    root_from(
        std::env::var_os(crate::home::VAR),
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
    )
    .ok_or_else(|| anyhow::anyhow!("cannot find the temp folder: set XDG_CACHE_HOME or HOME"))
}

/// The temp folder of the worker `session` in `root`, or `None` for a
/// session ID that is not a plain name.
///
/// ```
/// use riff::temp::of;
/// assert_eq!(of("/t".as_ref(), "1a2b-3c"), Some("/t/1a2b-3c".into()));
/// assert_eq!(of("/t".as_ref(), "../x"), None);
/// assert_eq!(of("/t".as_ref(), ""), None);
/// ```
pub fn of(root: &Path, session: &str) -> Option<PathBuf> {
    let plain = !session.is_empty()
        && session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    plain.then(|| root.join(session))
}

/// The temp folder of the worker `session` of this machine, from the
/// settings of this process.
pub fn here(session: &str) -> Option<PathBuf> {
    let settings = crate::settings::path().ok()?;
    of(&root(&settings).ok()?, session)
}

/// `args` of `claude` with the variables of `dir` in the `env` of its
/// flag settings (01M41VAGJC69S9R2TD1B1EQ4W4). It changes the first
/// `--settings` that holds a JSON object, and else puts a new
/// `--settings` first.
///
/// ```
/// use riff::temp::with_env;
///
/// let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
/// assert_eq!(
///     with_env(&args(&["--settings", r#"{"a":false}"#, "Join the riff."]), "/t/w1".as_ref()),
///     args(&["--settings", r#"{"a":false,"env":{"TMPDIR":"/t/w1","CLAUDE_CODE_TMPDIR":"/t/w1"}}"#, "Join the riff."]),
/// );
/// assert_eq!(
///     with_env(&args(&["Join the riff."]), "/t/w1".as_ref()),
///     args(&["--settings", r#"{"env":{"TMPDIR":"/t/w1","CLAUDE_CODE_TMPDIR":"/t/w1"}}"#, "Join the riff."]),
/// );
/// ```
pub fn with_env(args: &[String], dir: &Path) -> Vec<String> {
    let dir = dir.to_string_lossy();
    let add = |settings: &mut serde_json::Map<String, serde_json::Value>| {
        let env = settings
            .entry("env")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(env) = env.as_object_mut() {
            for var in VARS {
                env.insert(var.into(), dir.as_ref().into());
            }
        }
    };
    let mut out = args.to_vec();
    let at = out.windows(2).position(|pair| {
        pair[0] == "--settings"
            && serde_json::from_str::<serde_json::Value>(&pair[1]).is_ok_and(|v| v.is_object())
    });
    if let Some(at) = at {
        let mut value: serde_json::Value =
            serde_json::from_str(&out[at + 1]).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(settings) = value.as_object_mut() {
            add(settings);
        }
        out[at + 1] = value.to_string();
        return out;
    }
    let mut settings = serde_json::Map::new();
    add(&mut settings);
    let mut first = vec![
        "--settings".to_owned(),
        serde_json::Value::Object(settings).to_string(),
    ];
    first.append(&mut out);
    first
}

/// What a process of this user holds in the file system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub pid: u32,
    /// The working directory.
    pub cwd: Option<PathBuf>,
    /// The paths of the open files.
    pub files: Vec<PathBuf>,
    /// The values of [`VARS`].
    pub tmp: Vec<PathBuf>,
}

impl User {
    /// True when the working directory or an open file is in `path`.
    ///
    /// ```
    /// use riff::temp::User;
    /// let u = User { pid: 1, cwd: Some("/t/w1/a".into()), files: vec!["/t/w2/f".into()], tmp: vec!["/t/w3".into()] };
    /// assert!(u.holds("/t/w1".as_ref()) && u.holds("/t/w2".as_ref()));
    /// assert!(!u.holds("/t/w3".as_ref()) && !u.holds("/t/w".as_ref()));
    /// assert!(u.uses("/t/w3".as_ref()));
    /// ```
    pub fn holds(&self, path: &Path) -> bool {
        self.cwd.iter().chain(&self.files).any(|p| p.starts_with(path))
    }

    /// True when [`holds`](Self::holds), or when a variable of [`VARS`]
    /// is in `path` (01M41VAGMQPBDPV8XPGEKYRXZZ).
    pub fn uses(&self, path: &Path) -> bool {
        self.holds(path) || self.tmp.iter().any(|p| p.starts_with(path))
    }
}

/// The values of [`VARS`] in an environment of `/proc/PID/environ`.
///
/// ```
/// let env = b"HOME=/h\0TMPDIR=/t/w1\0CLAUDE_CODE_TMPDIR=/c\0";
/// assert_eq!(riff::temp::parse_environ(env), vec![std::path::PathBuf::from("/t/w1"), "/c".into()]);
/// ```
pub fn parse_environ(env: &[u8]) -> Vec<PathBuf> {
    env.split(|b| *b == 0)
        .filter_map(|var| {
            let var = String::from_utf8_lossy(var);
            let (name, value) = var.split_once('=')?;
            (VARS.contains(&name) && !value.is_empty()).then(|| PathBuf::from(value))
        })
        .collect()
}

/// Each process of this user that riff can read, but this process.
pub fn users() -> Vec<User> {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let me = std::process::id();
    dir.filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|&pid| pid != me)
        .filter_map(|pid| {
            let dir = PathBuf::from(format!("/proc/{pid}"));
            let tmp = parse_environ(&std::fs::read(dir.join("environ")).ok()?);
            let files = std::fs::read_dir(dir.join("fd"))
                .map(|fds| {
                    fds.filter_map(|fd| std::fs::read_link(fd.ok()?.path()).ok())
                        .collect()
                })
                .unwrap_or_default();
            Some(User {
                pid,
                cwd: std::fs::read_link(dir.join("cwd")).ok(),
                files,
                tmp,
            })
        })
        .collect()
}

/// `dir` and its real path, when they differ: `/proc` shows real paths.
fn both(dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![dir.to_owned()];
    if let Ok(real) = std::fs::canonicalize(dir)
        && real != dir
    {
        paths.push(real);
    }
    paths
}

/// True when a process of `users` uses `dir` ([`User::uses`]).
fn used(dir: &Path, users: &[User]) -> bool {
    both(dir)
        .iter()
        .any(|d| users.iter().any(|u| u.uses(d)))
}

/// True when a process of `users` holds `path` ([`User::holds`]).
fn held(path: &Path, users: &[User]) -> bool {
    both(path)
        .iter()
        .any(|p| users.iter().any(|u| u.holds(p)))
}

/// Deletes the temp folder `dir` of a worker that ended, when no
/// process of `users` uses it (01M41VAGQ2VA2Q0VSFJNG4H08W). True when
/// it deleted the folder.
pub fn end_session(dir: &Path, users: &[User]) -> bool {
    dir.is_dir() && !used(dir, users) && std::fs::remove_dir_all(dir).is_ok()
}

/// Deletes each file and folder in the temp folder `dir` of a worker
/// whose context ends, when no process of `users` holds it open or
/// works in it (01M41VAGSCNESHTZ6216P2E133). It keeps each folder
/// [`KEEP`]. In a held folder, and in a folder that holds a folder
/// [`KEEP`], it looks at each part again. Returns the deleted paths.
///
/// ```
/// use riff::temp::{User, end_context};
///
/// let dir = tempfile::tempdir()?;
/// let s1 = dir.path().join("claude-1000/p/s1");
/// std::fs::create_dir_all(s1.join("scratchpad/target"))?;
/// std::fs::create_dir_all(s1.join("tasks"))?;
/// std::fs::write(s1.join("tasks/watch.output"), "")?;
/// let held = dir.path().join("claude-1000/p/s0/scratchpad");
/// std::fs::create_dir_all(&held)?;
/// std::fs::write(dir.path().join("rustc.tmp"), "")?;
/// // claude has TMPDIR here, and works in s0: only s0 is held.
/// let users = [User { pid: 7, cwd: Some(held.clone()), files: vec![], tmp: vec![dir.path().into()] }];
/// let gone = end_context(dir.path(), &users);
/// assert_eq!(gone.len(), 2, "{gone:?}");
/// assert!(!s1.join("scratchpad").exists() && !dir.path().join("rustc.tmp").exists());
/// assert!(s1.join("tasks/watch.output").exists() && held.exists());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn end_context(dir: &Path, users: &[User]) -> Vec<PathBuf> {
    let mut gone = Vec::new();
    prune(dir, users, &mut gone);
    gone
}

fn prune(dir: &Path, users: &[User], gone: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        if is_dir && entry.file_name() == KEEP {
            continue;
        }
        if held(&path, users) || (is_dir && has_keep(&path, KEEP_DEPTH)) {
            if is_dir {
                prune(&path, users, gone);
            }
            continue;
        }
        let removed = if is_dir {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if removed.is_ok() {
            gone.push(path);
        }
    }
}

/// How deep [`end_context`] looks for a folder [`KEEP`]: Claude Code
/// has it in `claude-UID/PROJECT/SESSION/tasks`.
const KEEP_DEPTH: usize = 4;

/// True when `dir` holds a folder [`KEEP`] at most `depth` levels down.
fn has_keep(dir: &Path, depth: usize) -> bool {
    if depth == 0 {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .any(|e| e.file_name() == KEEP || has_keep(&e.path(), depth - 1))
}

/// Deletes each folder of `root` that no process of `users` uses and
/// that did not change after `now` less [`YOUNG`]
/// (01M41VAGVR2PPVAYDN0SWK2F02). Returns the deleted folders.
///
/// ```
/// use riff::temp::{User, sweep};
/// use std::time::{Duration, SystemTime};
///
/// let root = tempfile::tempdir()?;
/// for w in ["w1", "w2"] {
///     std::fs::create_dir(root.path().join(w))?;
/// }
/// let users = [User { pid: 7, cwd: None, files: vec![], tmp: vec![root.path().join("w2")] }];
/// assert!(sweep(root.path(), &users, SystemTime::now()).is_empty(), "w1 is young");
/// let later = SystemTime::now() + Duration::from_secs(120);
/// assert_eq!(sweep(root.path(), &users, later), [root.path().join("w1")]);
/// assert!(root.path().join("w2").exists());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn sweep(root: &Path, users: &[User], now: SystemTime) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let old = |path: &Path| {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .is_ok_and(|at| now.duration_since(at).is_ok_and(|age| age >= YOUNG))
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs.into_iter()
        .filter(|dir| old(dir) && end_session(dir, users))
        .collect()
}

/// The disk use of the files under `path` in bytes. A missing path
/// counts 0.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// std::fs::write(dir.path().join("f"), vec![1u8; 10_000])?;
/// assert!(riff::temp::size(dir.path()) >= 10_000);
/// assert_eq!(riff::temp::size(&dir.path().join("none")), 0);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn size(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    let own = meta.blocks() * 512;
    if !meta.is_dir() {
        return own;
    }
    let inner: u64 = std::fs::read_dir(path)
        .map(|entries| entries.flatten().map(|e| size(&e.path())).sum())
        .unwrap_or(0);
    own + inner
}

/// The folder of a worker for its wrapper: it makes the folder, and the
/// drop deletes it when no process uses it any more
/// ([`end_session`]).
#[derive(Debug)]
pub struct Folder(PathBuf);

impl Folder {
    /// Makes the temp folder of the worker `session` of this machine.
    /// `None` when riff cannot make it: then the worker uses the temp
    /// folder of the machine.
    pub fn make(session: &str) -> Option<Folder> {
        let dir = here(session)?;
        std::fs::create_dir_all(&dir)
            .inspect_err(|e| eprintln!("riff: cannot make the temp folder {}: {e}", dir.display()))
            .ok()?;
        Some(Folder(dir))
    }

    /// The path of the folder.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        end_session(&self.0, &users());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This process holds an open file in a folder: the folder stays.
    #[test]
    fn users_sees_an_open_file_of_another_process() {
        let dir = tempfile::tempdir().unwrap();
        let w1 = dir.path().join("w1");
        std::fs::create_dir(&w1).unwrap();
        std::fs::write(w1.join("held"), "").unwrap();
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("exec sleep 30 < \"$0\"")
            .arg(w1.join("held"))
            .stdin(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut child_held = false;
        for _ in 0..100 {
            if users()
                .iter()
                .any(|u| u.pid == child.id() && u.holds(&w1))
            {
                child_held = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let ended = end_session(&w1, &users());
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(child_held, "the child holds the file");
        assert!(!ended && w1.exists());
        assert!(end_session(&w1, &users()));
    }

    #[test]
    fn a_settings_value_that_is_not_an_object_gets_a_new_flag() {
        let args = vec!["--settings".to_owned(), "x.json".to_owned()];
        let out = with_env(&args, Path::new("/t"));
        assert_eq!(out[0], "--settings");
        assert!(out[1].contains("\"TMPDIR\":\"/t\""));
        assert_eq!(out[2..], args[..]);
    }
}
