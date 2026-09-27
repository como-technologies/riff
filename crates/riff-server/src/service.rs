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
//!   a second `install` writes the files again and restarts the service
//!   (R120).
//! - A second `install` keeps each old setting that it does not get
//!   again ([`installed()`], then [`merge()`]). A setting that it gets,
//!   as an option or as a `RIFF_*` variable, replaces the old one. So a
//!   plain `riff-server install` keeps `--listen` and `--insecure`
//!   (01M3JCE5477135XSD740DG7KFT).
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
pub fn env_text<N: AsRef<str>>(settings: &[(N, String)]) -> io::Result<String> {
    let mut text = String::new();
    for (name, value) in settings {
        let name = name.as_ref();
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

/// The settings in the text of a settings file, in order. It reads the
/// lines of [`env_text()`], and also `NAME=value` with no quotes. It
/// skips empty lines and comments.
///
/// ```
/// let text = "RIFF_LISTEN=\"0.0.0.0:7878\"\n# note\nA=\"x\\\"y\"\nB=z\n";
/// assert_eq!(
///     riff_server::service::parse_env(text),
///     [
///         ("RIFF_LISTEN".to_owned(), "0.0.0.0:7878".to_owned()),
///         ("A".to_owned(), "x\"y".to_owned()),
///         ("B".to_owned(), "z".to_owned()),
///     ]
/// );
/// ```
pub fn parse_env(text: &str) -> Vec<(String, String)> {
    let mut settings = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let value = match value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            Some(quoted) => {
                let mut out = String::new();
                let mut chars = quoted.chars();
                while let Some(c) = chars.next() {
                    out.push(if c == '\\' {
                        chars.next().unwrap_or(c)
                    } else {
                        c
                    });
                }
                out
            }
            None => value.to_owned(),
        };
        settings.push((name.trim().to_owned(), value));
    }
    settings
}

/// The settings of a new install: the `old` settings, with each
/// setting in `given` in place of the old one. A setting in `defaults`
/// goes in only when `old` does not have it
/// (01M3JCE5477135XSD740DG7KFT).
///
/// ```
/// use riff_server::service::merge;
///
/// let old = vec![
///     ("RIFF_LISTEN".to_owned(), "0.0.0.0:7878".to_owned()),
///     ("RIFF_INSECURE".to_owned(), "true".to_owned()),
/// ];
/// // A plain install keeps the old address.
/// let plain = merge(old.clone(), &[], &[("RIFF_LISTEN", "127.0.0.1:7878".into())]);
/// assert_eq!(plain, old);
/// // A new --listen replaces it, and keeps --insecure.
/// let moved = merge(old, &[("RIFF_LISTEN", "127.0.0.1:7878".into())], &[]);
/// assert_eq!(moved[0].1, "127.0.0.1:7878");
/// assert_eq!(moved[1].0, "RIFF_INSECURE");
/// ```
pub fn merge(
    old: Vec<(String, String)>,
    given: &[(&str, String)],
    defaults: &[(&str, String)],
) -> Vec<(String, String)> {
    let mut settings = old;
    for (name, value) in given {
        match settings.iter_mut().find(|(n, _)| n == name) {
            Some(setting) => setting.1 = value.clone(),
            None => settings.push(((*name).to_owned(), value.clone())),
        }
    }
    for (name, value) in defaults {
        if !settings.iter().any(|(n, _)| n == name) {
            settings.push(((*name).to_owned(), value.clone()));
        }
    }
    settings
}

/// The settings of the installed service in `dir`. It is empty when
/// there is no settings file.
pub fn installed(dir: &Path) -> io::Result<Vec<(String, String)>> {
    match std::fs::read_to_string(dir.join(ENV)) {
        Ok(text) => Ok(parse_env(&text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
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
pub fn install<N: AsRef<str>>(
    systemctl: &Path,
    dir: &Path,
    exe: &Path,
    settings: &[(N, String)],
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
    fn parse_env_reads_what_env_text_writes() {
        let settings = vec![
            ("A".to_owned(), r#"x"y\z"#.to_owned()),
            ("B".to_owned(), String::new()),
        ];
        let text = env_text(&settings).unwrap();
        assert_eq!(parse_env(&text), settings);
    }

    #[test]
    fn merge_adds_a_new_setting_after_the_old_ones() {
        let old = vec![("A".to_owned(), "1".to_owned())];
        let merged = merge(old, &[("B", "2".into())], &[("A", "0".into())]);
        assert_eq!(
            merged,
            [
                ("A".to_owned(), "1".to_owned()),
                ("B".to_owned(), "2".to_owned())
            ]
        );
    }

    #[test]
    fn installed_is_empty_without_a_settings_file() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(installed(tmp.path()).unwrap().is_empty());
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
        let none: &[(&str, String)] = &[];
        let err = install(Path::new("false"), &dir, Path::new("/x"), none).unwrap_err();
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
