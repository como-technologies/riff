//! The files in the local folders of the platform
//! ([`Platform::local_dir`](crate::platform::Platform::local_dir) and
//! [`Platform::marks`](crate::platform::Platform::marks)). Each folder
//! is private: mode 0700, and each file in it mode 0600
//! (01M56RB1DQ396DQ75Q23G87C28). A writer of the client and of `riff`
//! uses [`write()`], [`make_dir`] or [`options`]: the refresh lock of
//! the sign-in ([`crate::login`]), the leave marks ([`crate::marks`]),
//! and each file of `riff::local`.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// Makes the folder `dir` with mode 0700, also each folder above it
/// that it makes. A `dir` that exists gets mode 0700
/// (01M56RB1DQ396DQ75Q23G87C28).
///
/// ```
/// use std::os::unix::fs::PermissionsExt;
///
/// let run = tempfile::tempdir()?;
/// let dir = run.path().join("riff");
/// riff_client::files::make_dir(&dir)?;
/// assert_eq!(std::fs::metadata(&dir)?.permissions().mode() & 0o777, 0o700);
/// std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))?;
/// riff_client::files::make_dir(&dir)?;
/// assert_eq!(std::fs::metadata(&dir)?.permissions().mode() & 0o777, 0o700);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn make_dir(dir: &Path) -> io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    set_mode(dir, 0o700)
}

/// The options to open a file in a local folder: mode 0600 for a new
/// file. [`private`] gives the mode to a file that exists.
pub fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.mode(0o600);
    options
}

/// Gives the open `file` mode 0600, when its mode differs.
pub fn private(file: &File) -> io::Result<()> {
    if file.metadata()?.permissions().mode() & 0o777 != 0o600 {
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Writes `contents` to the file `path` with mode 0600. It makes the
/// folder of the file with [`make_dir`].
///
/// ```
/// use std::os::unix::fs::PermissionsExt;
///
/// let run = tempfile::tempdir()?;
/// let path = run.path().join("riff/statusline-a6cf");
/// riff_client::files::write(&path, "x")?;
/// assert_eq!(std::fs::read_to_string(&path)?, "x");
/// assert_eq!(std::fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        make_dir(dir)?;
    }
    let mut file = options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    private(&file)?;
    file.write_all(contents.as_ref())
}

/// Gives `dir` and each folder in it mode 0700, and each other file in
/// it mode 0600. A link stays as it is.
///
/// ```
/// use std::os::unix::fs::PermissionsExt;
///
/// let run = tempfile::tempdir()?;
/// let (top, pool) = (run.path(), run.path().join("jobserver"));
/// let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
/// let set = |p: &std::path::Path, m| std::fs::set_permissions(p, std::fs::Permissions::from_mode(m));
/// std::fs::create_dir_all(&pool)?;
/// std::fs::write(top.join("activity-a6cf"), "{}")?;
/// std::fs::write(pool.join("size"), "4")?;
/// set(top, 0o775)?;
/// set(&pool, 0o775)?;
/// set(&top.join("activity-a6cf"), 0o664)?;
/// set(&pool.join("size"), 0o644)?;
/// riff_client::files::secure(top)?;
/// assert_eq!(mode(top), 0o700);
/// assert_eq!(mode(&pool), 0o700);
/// assert_eq!(mode(&top.join("activity-a6cf")), 0o600);
/// assert_eq!(mode(&pool.join("size")), 0o600);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn secure(dir: &Path) -> io::Result<()> {
    set_mode(dir, 0o700)?;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let done = if kind.is_dir() {
            secure(&entry.path())
        } else if kind.is_symlink() {
            Ok(())
        } else {
            set_mode(&entry.path(), 0o600)
        };
        match done {
            // Another process removed it.
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            other => other?,
        }
    }
    Ok(())
}

/// Gives `path` the mode `mode`, when its mode differs.
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    if std::fs::metadata(path)?.permissions().mode() & 0o777 != mode {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}
