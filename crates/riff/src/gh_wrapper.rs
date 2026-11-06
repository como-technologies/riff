//! The `gh` of a container session counts its calls (#1582).
//!
//! # Design
//!
//! A session runs `gh` on its own: `gh pr view`, `gh run watch`, a script
//! of the project. The call uses the GraphQL points of the token of the
//! person, and the daemon cannot see it. So the `gh` of a container is a
//! wrapper: the daemon binds its own binary of riff at [`BIND`], and a
//! name `gh` in `argv[0]` makes `riff` act as the wrapper
//! ([`invoked_as_gh`], [`run`]).
//!
//! The wrapper adds the call to the tally of the daemon
//! ([`crate::ghcalls::Tally`]) and then replaces itself with the real
//! `gh` at [`REAL`]. It does not stay in the way of `gh`:
//!
//! - It does not read the output of `gh`, so the terminal, the pipes, the
//!   exit code and the signals are those of `gh`. So the points come from
//!   the table of the arguments ([`crate::ghcalls::points_of`]), and not
//!   from the reply.
//! - It counts a call before the call runs. A call that fails costs
//!   points too.
//! - It counts nothing that `gh` does without GitHub: `gh auth`, `gh
//!   --version` ([`crate::ghcalls::Resource::of`]).
//! - It never fails the call. When the daemon does not answer, it drops
//!   the count and runs `gh`.
//!
//! A command of `riff` that runs in a container, for example `riff pr
//! wait`, counts its calls itself and with its own name. It sets
//! [`COUNTED_VAR`] for its `gh`, so the wrapper counts that call no
//! more ([`crate::pr::Gh::here`]).
//!
//! ```mermaid
//! flowchart LR
//!     A["claude, a script: gh pr view 40"] --> W["/usr/local/bin/gh:<br/>the binary of riff"]
//!     W -- "SessionAsk::GhCalls" --> D["riff daemon:<br/>the tally"]
//!     W -- "exec" --> G["/usr/bin/gh"]
//!     R["riff pr wait in the container"] -- "RIFF_GH_COUNTED=1" --> W
//!     R -- "SessionAsk::GhCalls, as pr wait" --> D
//! ```
//!
//! # An image with its own `gh` at the bind path
//!
//! A bind hides the file below it. An image that has its `gh` at [`BIND`]
//! (from the tarball of a release, or `install gh /usr/local/bin`) would
//! lose it: the wrapper cannot run a `gh` that it hides, and each `gh`
//! call of the session would fail. So the daemon binds the wrapper only
//! in a session of an image that has no `gh` at [`BIND`]. It asks once
//! for each image, with `/usr/local/bin/gh --version` in a container with
//! no network ([`crate::runtime::Runtime::output`]), and keeps the answer
//! until the daemon stops. In a session of an image with its own
//! `gh` there, `gh` is that file, and riff counts none of its calls.
//! `riff daemon status` lists those images
//! ([`Shown::own_gh`](crate::ghcalls::Shown::own_gh)).
//!
//! An image with its `gh` at any other path, `/usr/bin/gh` or a folder of
//! `PATH`, gets the wrapper, which runs that `gh` ([`real_in`]).
//!
//! ```mermaid
//! flowchart TD
//!     S["a session starts"] --> Q{"/usr/local/bin/gh --version<br/>works in the image?"}
//!     Q -- "no" --> B["bind the wrapper at /usr/local/bin/gh:<br/>the calls are counted"]
//!     Q -- "yes: the image has its own gh" --> K["no bind: its gh runs as it is,<br/>riff daemon status names the image"]
//! ```

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::ghcalls::{self, Resource, SESSION, Tally};

/// Where the daemon binds the wrapper in a container: the place of the
/// `gh` that the shell finds first on a `PATH` of a Debian image. It binds
/// nothing there when the image has its own `gh` there.
pub const BIND: &str = "/usr/local/bin/gh";

/// The real `gh` in the image of a session (01M5SNT57ANM99NQS5SNB8XPWV).
pub const REAL: &str = "/usr/bin/gh";

/// The variable that gives another path of the real `gh`. Only a test
/// sets it.
///
/// ```
/// assert_eq!(riff::gh_wrapper::REAL_VAR, "RIFF_GH_REAL");
/// ```
pub const REAL_VAR: &str = "RIFF_GH_REAL";

/// The variable that a command of `riff` sets for its `gh` when it counted
/// the call itself.
pub const COUNTED_VAR: &str = "RIFF_GH_COUNTED";

/// True when this process runs under the name `gh`: the shell of the
/// container found the binary of riff at [`BIND`].
///
/// ```
/// use std::ffi::OsStr;
/// use riff::gh_wrapper::named_gh;
///
/// assert!(named_gh(OsStr::new("gh")));
/// assert!(named_gh(OsStr::new("/usr/local/bin/gh")));
/// assert!(!named_gh(OsStr::new("riff")));
/// assert!(!named_gh(OsStr::new("/opt/riff/riff")));
/// assert!(!named_gh(OsStr::new("")));
/// ```
pub fn named_gh(argv0: &OsStr) -> bool {
    Path::new(argv0).file_name() == Some(OsStr::new("gh"))
}

/// True when this process runs under the name `gh`.
pub fn invoked_as_gh() -> bool {
    std::env::args_os().next().is_some_and(|a| named_gh(&a))
}

/// What the wrapper counts for `gh ARGS`: the kind and the points, and
/// `None` for a call that uses no limit.
///
/// ```
/// use riff::gh_wrapper::counted;
///
/// assert_eq!(counted(&["pr", "view", "40"]), Some(("pr view".to_owned(), 1)));
/// assert_eq!(counted(&["api", "repos/o/r"]), Some(("api repo".to_owned(), 0)));
/// assert_eq!(counted(&["auth", "status"]), None);
/// assert_eq!(counted(&["--version"]), None);
/// assert_eq!(counted(&[]), None);
/// ```
pub fn counted(args: &[&str]) -> Option<(String, u32)> {
    Resource::of(args)?;
    Some((ghcalls::kind_of(args), ghcalls::points_of(args, "")))
}

/// The wrapper: counts the call `gh ARGS` with `tally` when
/// [`COUNTED_VAR`] is not set, and runs the real `gh` in the place of
/// this process. It returns only when the real `gh` does not start.
pub fn run(args: Vec<OsString>, tally: Option<&Tally>) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    let words: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    if std::env::var_os(COUNTED_VAR).is_none()
        && let (Some(tally), Some((kind, points))) = (tally, counted(&words))
    {
        tally.record(SESSION, &kind, points);
    }
    std::process::Command::new(real()).args(args).exec()
}

/// The path of the real `gh` of this process ([`real_in`]).
fn real() -> PathBuf {
    let me = std::env::current_exe().unwrap_or_default();
    real_in(
        std::env::var_os(REAL_VAR),
        Path::new(REAL),
        std::env::var_os("PATH").as_deref(),
        &me,
    )
}

/// The path of the real `gh`: `var` (the value of [`REAL_VAR`]) when it is
/// set, else `default` ([`REAL`]) when the image has it there. An image
/// that has `gh` at another place gets the first `gh` on `path` that is
/// not the file `me`, which is the wrapper itself. The daemon binds the
/// wrapper at [`BIND`] only when the image has no `gh` there, so the bind
/// never hides the only `gh` of an image (#1595).
///
/// ```
/// use std::ffi::OsString;
/// use std::os::unix::fs::PermissionsExt;
/// use std::path::Path;
/// use riff::gh_wrapper::real_in;
///
/// let dir = tempfile::tempdir()?;
/// let write = |name: &str| -> std::io::Result<std::path::PathBuf> {
///     let folder = dir.path().join(name);
///     std::fs::create_dir_all(&folder)?;
///     let gh = folder.join("gh");
///     std::fs::write(&gh, "#!/bin/sh\n")?;
///     std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755))?;
///     Ok(gh)
/// };
/// let (wrapper, other) = (write("local")?, write("opt")?);
/// let path = std::env::join_paths([dir.path().join("local"), dir.path().join("opt")])?;
/// let nowhere = Path::new("/nowhere/gh");
///
/// // The default is there: it wins.
/// assert_eq!(real_in(None, &other, Some(&path), &wrapper), other);
/// // The default is not there: the first `gh` on `path` that is not the
/// // wrapper. The wrapper may have another name for the same file.
/// assert_eq!(real_in(None, nowhere, Some(&path), &wrapper), other);
/// let bound = dir.path().join("bound");
/// std::fs::hard_link(&wrapper, &bound)?;
/// assert_eq!(real_in(None, nowhere, Some(&path), &bound), other);
/// // No other `gh`: the default, which then fails with its own error.
/// let only: OsString = dir.path().join("local").into();
/// assert_eq!(real_in(None, nowhere, Some(&only), &wrapper), nowhere);
/// // The variable of a test wins over all.
/// assert_eq!(real_in(Some("/t/gh".into()), &other, Some(&path), &wrapper), Path::new("/t/gh"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn real_in(var: Option<OsString>, default: &Path, path: Option<&OsStr>, me: &Path) -> PathBuf {
    if let Some(var) = var.filter(|var| !var.is_empty()) {
        return var.into();
    }
    if default.is_file() {
        return default.to_owned();
    }
    std::env::split_paths(path.unwrap_or_default())
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join("gh"))
        .find(|gh| gh.is_file() && !same_inode(gh, me))
        .unwrap_or_else(|| default.to_owned())
}

/// True when `a` and `b` are the same file, also under two names.
fn same_inode(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => (a.dev(), a.ino()) == (b.dev(), b.ino()),
        _ => false,
    }
}

/// The `riff` that runs as `gh`: counts and runs the real `gh`
/// ([`run`]). It never returns.
pub fn main() -> ! {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let error = run(args, Tally::here().as_ref());
    eprintln!("gh: cannot run {}: {error}", real().display());
    std::process::exit(127)
}
