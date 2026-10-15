//! The service manager of riff 3.0 (01M4Z60B7GTV4HHF9R3TMZRS7J): it
//! installs and removes the user unit of the daemon, and shows its
//! state. On Linux, it is systemd ([`Systemd`]).
//!
//! # Design
//!
//! `riff daemon install` writes the unit [`UNIT`] in the folder of the
//! user units of systemd: `XDG_CONFIG_HOME/systemd/user`, else
//! `HOME/.config/systemd/user` (01M4ZDTVK74596M0ZA46N51NKV). Then it
//! runs these steps:
//!
//! | Step | Command |
//! |---|---|
//! | Read the unit again | `systemctl --user daemon-reload` |
//! | Start it at each login | `systemctl --user enable riff.service` |
//! | Start it, or start it again with this riff | `systemctl --user restart riff.service` |
//!
//! `riff daemon uninstall` undoes it (01M501M0T0JBWGMMHCT7HT95T9). It
//! deletes only a unit file that starts with the line [`MARK`]:
//!
//! | Step | Command |
//! |---|---|
//! | Stop it, and start it at no login | `systemctl --user disable --now riff.service` |
//! | Delete the unit file | |
//! | Forget the unit | `systemctl --user daemon-reload` |
//!
//! The unit runs `riff daemon` with the absolute path of this riff. It
//! gets the `PATH` of the shell that installs it, so the daemon finds
//! the same `podman`, and `XDG_DATA_HOME` when it is set, so the daemon
//! and `riff` use one folder ([`crate::daemon::dir`]).
//!
//! ```
//! use riff::service::Unit;
//!
//! let unit = Unit {
//!     exe: "/home/mike/.cargo/bin/riff".into(),
//!     env: vec![("PATH".into(), "/home/mike/.cargo/bin:/usr/bin".into())],
//! };
//! let text = unit.text()?;
//! assert!(text.contains("ExecStart=\"/home/mike/.cargo/bin/riff\" daemon\n"));
//! assert!(text.contains("Environment=\"PATH=/home/mike/.cargo/bin:/usr/bin\"\n"));
//! assert!(text.contains("WantedBy=default.target\n"));
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, bail};

use crate::text;

/// The user unit of the daemon.
pub const UNIT: &str = "riff.service";

/// The first line of each unit file that riff writes.
pub const MARK: &str = "# The unit of the riff daemon. riff daemon install writes this file.";

/// The user unit of the daemon as data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// The absolute path of `riff`.
    pub exe: PathBuf,
    /// The variables of the daemon.
    pub env: Vec<(String, String)>,
}

/// `text` in double quotes for a line of a unit. `dollar` doubles each
/// `$`: an `ExecStart` expands variables, an `Environment` does not.
/// riff refuses a text with a control character: no line of a unit can
/// hold it.
fn quoted(text: &str, dollar: bool) -> Result<String> {
    if text.chars().any(char::is_control) {
        bail!("{}", text::unit_bad_text(text));
    }
    let mut out = String::from('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '%' => out.push_str("%%"),
            '$' if dollar => out.push_str("$$"),
            c => out.push(c),
        }
    }
    out.push('"');
    Ok(out)
}

impl Unit {
    /// The unit of this riff: the path of this binary, its `PATH`, and
    /// its `XDG_DATA_HOME` when it is set.
    pub fn of_this() -> Result<Unit> {
        let exe = std::env::current_exe().context("cannot find the path of this riff")?;
        let env = ["PATH", "XDG_DATA_HOME"]
            .into_iter()
            .filter_map(|name| {
                std::env::var(name)
                    .ok()
                    .filter(|v| !v.is_empty())
                    .map(|v| (name.to_owned(), v))
            })
            .collect();
        Ok(Unit { exe, env })
    }

    /// The text of the unit file.
    ///
    /// ```
    /// use riff::service::Unit;
    ///
    /// let unit = Unit { exe: "/opt/my riff/riff".into(), env: vec![("PATH".into(), "/a%b$c".into())] };
    /// let text = unit.text()?;
    /// assert!(text.contains("ExecStart=\"/opt/my riff/riff\" daemon\n"));
    /// assert!(text.contains("Environment=\"PATH=/a%%b$c\"\n"));
    /// let bad = Unit { exe: "/opt/riff\n".into(), env: vec![] };
    /// assert!(bad.text().is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn text(&self) -> Result<String> {
        let exe = self
            .exe
            .to_str()
            .with_context(|| format!("the path of riff {} is not UTF-8", self.exe.display()))?;
        let mut out = format!(
            "{MARK}\n\
             [Unit]\n\
             Description=riff daemon: the sessions of riff on this machine\n\n\
             [Service]\n\
             Type=simple\n",
        );
        out.push_str(&format!("ExecStart={} daemon\n", quoted(exe, true)?));
        for (name, value) in &self.env {
            out.push_str(&format!(
                "Environment={}\n",
                quoted(&format!("{name}={value}"), false)?
            ));
        }
        out.push_str(
            "Restart=on-failure\n\
             RestartSec=2\n\n\
             [Install]\n\
             WantedBy=default.target\n",
        );
        Ok(out)
    }
}

/// The service manager of this machine (01M4Z60B7GTV4HHF9R3TMZRS7J).
pub trait ServiceManager {
    /// Writes `unit`, and starts it again, so that a new riff takes
    /// effect. Gives the path of the unit file.
    fn install(&self, unit: &Unit) -> Result<PathBuf>;

    /// Stops and disables the unit, deletes its file, and reloads the
    /// manager. Gives the path of the deleted file, or `None` when no
    /// unit is installed. It refuses a unit file that riff did not write.
    fn uninstall(&self) -> Result<Option<PathBuf>>;

    /// The state of the unit, for example `active` or `inactive`.
    fn state(&self) -> Result<String>;
}

/// systemd, for the user units of the person.
#[derive(Debug, Clone)]
pub struct Systemd {
    /// The folder of the user units.
    pub dir: PathBuf,
}

impl Systemd {
    /// The folder of the user units from `XDG_CONFIG_HOME` and `HOME`.
    ///
    /// ```
    /// use riff::service::Systemd;
    /// use std::path::Path;
    ///
    /// let some = |s: &str| Some(s.into());
    /// assert_eq!(Systemd::from(some("/c"), some("/h"))?.dir, Path::new("/c/systemd/user"));
    /// assert_eq!(Systemd::from(some(""), some("/h"))?.dir, Path::new("/h/.config/systemd/user"));
    /// assert!(Systemd::from(None, None).is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn from(
        config_home: Option<std::ffi::OsString>,
        home: Option<std::ffi::OsString>,
    ) -> Result<Systemd> {
        let set = |v: Option<std::ffi::OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
        let dir = match (set(config_home), set(home)) {
            (Some(config), _) => config.join("systemd/user"),
            (None, Some(home)) => home.join(".config/systemd/user"),
            (None, None) => bail!("set HOME or XDG_CONFIG_HOME"),
        };
        Ok(Systemd { dir })
    }

    /// [`Systemd::from`] with the values of this process.
    pub fn here() -> Result<Systemd> {
        Systemd::from(
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::var_os("HOME"),
        )
    }

    /// The path of the unit file.
    pub fn unit_path(&self) -> PathBuf {
        self.dir.join(UNIT)
    }
}

/// Runs `systemctl --user ARGS`, and gives its stdout. With `check`, an
/// exit that is not 0 is an error that names the command.
fn systemctl(args: &[&str], check: bool) -> Result<String> {
    let out = std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| text::no_program("systemctl"))?;
    if check && !out.status.success() {
        bail!(
            "systemctl --user {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Writes `text` to `path`, through a new file and a rename.
fn write_file(path: &Path, text: &str) -> Result<()> {
    let dir = path.parent().context("a unit file has a folder")?;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let new = path.with_extension("service.new");
    std::fs::write(&new, text).with_context(|| format!("cannot write {}", new.display()))?;
    std::fs::rename(&new, path).with_context(|| format!("cannot write {}", path.display()))
}

impl ServiceManager for Systemd {
    fn install(&self, unit: &Unit) -> Result<PathBuf> {
        let path = self.unit_path();
        write_file(&path, &unit.text()?)?;
        systemctl(&["daemon-reload"], true)?;
        systemctl(&["enable", UNIT], true)?;
        systemctl(&["restart", UNIT], true)?;
        Ok(path)
    }

    fn uninstall(&self) -> Result<Option<PathBuf>> {
        let path = self.unit_path();
        let old = match std::fs::read_to_string(&path) {
            Ok(old) => old,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        if old.lines().next() != Some(MARK) {
            bail!("{}", text::unit_not_of_riff(&path));
        }
        systemctl(&["disable", "--now", UNIT], true)?;
        std::fs::remove_file(&path).with_context(|| format!("cannot delete {}", path.display()))?;
        systemctl(&["daemon-reload"], true)?;
        Ok(Some(path))
    }

    fn state(&self) -> Result<String> {
        let state = systemctl(&["is-active", UNIT], false)?;
        Ok(if state.is_empty() {
            "unknown".into()
        } else {
            state
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unit_quotes_each_value_for_systemd() {
        let unit = Unit {
            exe: "/home/mike/bin/riff".into(),
            env: vec![
                ("PATH".into(), "/a b:/c\"d\\e".into()),
                ("XDG_DATA_HOME".into(), "/home/mike/.local/share".into()),
            ],
        };
        let text = unit.text().unwrap();
        assert_eq!(
            text,
            "# The unit of the riff daemon. riff daemon install writes this file.\n\
             [Unit]\n\
             Description=riff daemon: the sessions of riff on this machine\n\n\
             [Service]\n\
             Type=simple\n\
             ExecStart=\"/home/mike/bin/riff\" daemon\n\
             Environment=\"PATH=/a b:/c\\\"d\\\\e\"\n\
             Environment=\"XDG_DATA_HOME=/home/mike/.local/share\"\n\
             Restart=on-failure\n\
             RestartSec=2\n\n\
             [Install]\n\
             WantedBy=default.target\n"
        );
        // An ExecStart expands $, so riff doubles it.
        let dollar = Unit {
            exe: "/opt/$riff".into(),
            env: vec![],
        };
        assert!(
            dollar
                .text()
                .unwrap()
                .contains("ExecStart=\"/opt/$$riff\" daemon")
        );
        let bad = Unit {
            exe: "/opt/riff".into(),
            env: vec![("PATH".into(), "/a\n[Service]".into())],
        };
        assert!(bad.text().is_err());
    }

    #[test]
    fn uninstall_with_no_unit_says_none_and_keeps_a_unit_of_another() {
        let root = tempfile::tempdir().unwrap();
        let systemd = Systemd {
            dir: root.path().join("systemd/user"),
        };
        assert_eq!(systemd.uninstall().unwrap(), None);

        // A unit that riff did not write stays, and riff runs no systemctl.
        write_file(&systemd.unit_path(), "[Unit]\nDescription=mine\n").unwrap();
        let e = systemd.uninstall().unwrap_err().to_string();
        assert!(e.starts_with("riff did not write the unit "), "{e}");
        assert!(systemd.unit_path().exists());
    }
}
