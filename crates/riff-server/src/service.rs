//! Run `riff-server` as a systemd user service.
//!
//! # Design
//!
//! `riff-server` with no subcommand runs in the foreground (R118).
//! `riff-server install` and `riff-server uninstall` manage a systemd
//! user service with the `systemctl` command.
//!
//! ```mermaid
//! flowchart LR
//!     I[riff-server install] --> C{systemctl --user works?}
//!     C -- no --> F[fail, change nothing]
//!     C -- yes --> W[write unit and settings]
//!     W --> R[daemon-reload]
//!     R --> E[enable]
//!     E --> S[restart]
//! ```
//!
//! - [`install()`] writes two files to [`dir()`]: the unit
//!   [`UNIT`] ([`unit_text()`]) and the settings [`ENV`]
//!   ([`env_text()`]), with mode 0600 (R119). The unit runs the binary
//!   that ran `install`, with the settings as environment variables.
//! - [`install()`] then runs `daemon-reload`, `enable` and `restart`. So
//!   a second `install` replaces the files and restarts the service
//!   (R120).
//! - [`uninstall()`] runs `disable --now` when the unit is there. Then it
//!   removes both files and runs `daemon-reload` (R121).
//! - Both first run `systemctl --user show-environment`. When it fails,
//!   they fail and change nothing (R122).
//!
//! ```
//! use std::path::Path;
//!
//! let unit = riff_server::service::unit_text(
//!     Path::new("/usr/bin/riff-server"),
//!     Path::new("/home/mike/.config/systemd/user/riff-server.env"),
//! );
//! assert!(unit.contains("ExecStart=/usr/bin/riff-server\n"));
//! assert!(unit.contains("Restart=on-failure\n"));
//! ```

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The file name of the unit.
pub const UNIT: &str = "riff-server.service";

/// The file name of the settings. It is next to the unit.
pub const ENV: &str = "riff-server.env";

/// The name of the service for `systemctl`.
const NAME: &str = "riff-server";

/// The text of the unit. It runs `exe` with the settings in `env`.
pub fn unit_text(exe: &Path, env: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=riff-server: the central service that riff sessions connect to\n\
         \n\
         [Service]\n\
         ExecStart={}\n\
         EnvironmentFile={}\n\
         Restart=on-failure\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exe.display(),
        env.display()
    )
}

/// The text of the settings file: one `NAME="value"` line for each
/// setting. It escapes `\` and `"`. A value with a line break is an
/// error.
///
/// ```
/// let text = riff_server::service::env_text(&[
///     ("RIFF_LISTEN", "127.0.0.1:7878".into()),
///     ("RIFF_ADMINS", "mike,ann".into()),
/// ])?;
/// assert_eq!(text, "RIFF_LISTEN=\"127.0.0.1:7878\"\nRIFF_ADMINS=\"mike,ann\"\n");
/// assert!(riff_server::service::env_text(&[("A", "x\ny".into())]).is_err());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn env_text(settings: &[(&str, String)]) -> io::Result<String> {
    let mut text = String::new();
    for (name, value) in settings {
        if value.contains(['\n', '\r']) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{name} has a line break"),
            ));
        }
        let value = value.replace('\\', "\\\\").replace('"', "\\\"");
        text.push_str(&format!("{name}=\"{value}\"\n"));
    }
    Ok(text)
}

/// The directory for the unit: `$XDG_CONFIG_HOME/systemd/user`, or
/// `$HOME/.config/systemd/user`.
///
/// ```
/// use std::path::Path;
///
/// let dir = riff_server::service::dir_from(None, Some("/home/mike".into()))?;
/// assert_eq!(dir, Path::new("/home/mike/.config/systemd/user"));
/// let dir = riff_server::service::dir_from(Some("/cfg".into()), None)?;
/// assert_eq!(dir, Path::new("/cfg/systemd/user"));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn dir_from(
    config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> io::Result<PathBuf> {
    let config = match (config_home.filter(|c| !c.is_empty()), home) {
        (Some(config), _) => PathBuf::from(config),
        (None, Some(home)) => Path::new(&home).join(".config"),
        (None, None) => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "set HOME or XDG_CONFIG_HOME",
            ));
        }
    };
    Ok(config.join("systemd/user"))
}

/// [`dir_from`] with the values from the environment.
pub fn dir() -> io::Result<PathBuf> {
    dir_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

/// Writes the unit and the settings to `dir`, then enables and
/// restarts the service with `systemctl` (R119, R120).
pub fn install(
    systemctl: &Path,
    dir: &Path,
    exe: &Path,
    settings: &[(&str, String)],
) -> io::Result<()> {
    check(systemctl)?;
    let env = dir.join(ENV);
    let env_body = env_text(settings)?;
    std::fs::create_dir_all(dir)?;
    write_private(&env, &env_body)?;
    std::fs::write(dir.join(UNIT), unit_text(exe, &env))?;
    run(systemctl, &["daemon-reload"])?;
    run(systemctl, &["enable", NAME])?;
    run(systemctl, &["restart", NAME])
}

/// Stops and disables the service, and removes the unit and the
/// settings from `dir` (R121). It returns false when there was nothing
/// to remove.
pub fn uninstall(systemctl: &Path, dir: &Path) -> io::Result<bool> {
    check(systemctl)?;
    let unit = dir.join(UNIT);
    let env = dir.join(ENV);
    let had_unit = unit.exists();
    if had_unit {
        run(systemctl, &["disable", "--now", NAME])?;
    }
    let mut removed = had_unit;
    for path in [&unit, &env] {
        match std::fs::remove_file(path) {
            Ok(()) => removed = true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    if had_unit {
        run(systemctl, &["daemon-reload"])?;
    }
    Ok(removed)
}

/// Fails when `systemctl --user` does not work here (R122).
fn check(systemctl: &Path) -> io::Result<()> {
    run(systemctl, &["show-environment"]).map_err(|e| {
        io::Error::other(format!(
            "this host has no systemd user manager ({e}). Run riff-server in a terminal."
        ))
    })
}

/// Runs `systemctl --user` with `args`. It fails with the output of the
/// command when the command fails.
fn run(systemctl: &Path, args: &[&str]) -> io::Result<()> {
    let line = format!("{} --user {}", systemctl.display(), args.join(" "));
    let out = Command::new(systemctl)
        .arg("--user")
        .args(args)
        .output()
        .map_err(|e| io::Error::new(e.kind(), format!("run {line}: {e}")))?;
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "{line} failed: {}{}",
            String::from_utf8_lossy(&out.stdout).trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

/// Writes `text` to `path` with mode 0600.
fn write_private(path: &Path, text: &str) -> io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_names_the_settings_and_starts_at_login() {
        let unit = unit_text(
            Path::new("/bin/riff-server"),
            Path::new("/c/riff-server.env"),
        );
        assert!(unit.contains("EnvironmentFile=/c/riff-server.env\n"));
        assert!(unit.contains("WantedBy=default.target\n"));
    }

    #[test]
    fn env_text_escapes_quotes_and_backslashes() {
        let text = env_text(&[("A", r#"x"y\z"#.into())]).unwrap();
        assert_eq!(text, "A=\"x\\\"y\\\\z\"\n");
    }

    #[test]
    fn empty_config_home_uses_home() {
        let dir = dir_from(Some("".into()), Some("/h".into())).unwrap();
        assert_eq!(dir, Path::new("/h/.config/systemd/user"));
        assert!(dir_from(None, None).is_err());
    }

    #[test]
    fn install_fails_and_writes_nothing_without_systemctl() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("user");
        let err = install(Path::new("false"), &dir, Path::new("/x"), &[]).unwrap_err();
        assert!(err.to_string().contains("no systemd user manager"), "{err}");
        assert!(!dir.exists());
    }

    #[test]
    fn settings_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(ENV);
        std::fs::write(&path, "old").unwrap();
        write_private(&path, "new").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }
}
