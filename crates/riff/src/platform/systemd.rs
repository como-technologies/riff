//! systemd: the service manager of the daemon on Linux
//! (01M4Z60B7GTV4HHF9R3TMZRS7J, 01M6YQDWT387EV7Y7XMT2G6YVM). The steps
//! of the unit are in [`crate::service`].

use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use anyhow::{Context, Result, bail};

use crate::service::{DESKTOP_TARGET, MARK, ServiceManager, UNIT, Unit};
use crate::text;

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
    /// use riff::platform::systemd::Systemd;
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

/// Runs `systemctl --user ARGS`, and gives its output.
fn output(args: &[&str]) -> Result<Output> {
    std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| text::no_program("systemctl"))
}

/// The error of `systemctl --user ARGS` that gave `out`: the command,
/// and its stderr, or its exit when stderr has no text.
fn failed(args: &[&str], out: &Output) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let why = match stderr.trim() {
        "" => out.status.to_string(),
        text => text.to_owned(),
    };
    anyhow::anyhow!("systemctl --user {}: {why}", args.join(" "))
}

/// Runs `systemctl --user ARGS`, and gives its stdout. With `check`, an
/// exit that is not 0 is an error that names the command.
fn systemctl(args: &[&str], check: bool) -> Result<String> {
    let out = output(args)?;
    if check && !out.status.success() {
        return Err(failed(args, &out));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The result of `systemctl --user ARGS`, a new start of the unit, from
/// its output `out` (01M53NEE2AVNHA8RD8N40FPH03). The new start stops
/// each process of the unit. When the daemon asks, that is also its
/// `systemctl` child, which can end by a signal before it exits. With
/// no text on stderr, systemd has the job, and the new start runs. An
/// exit code that is not 0, or a signal with text on stderr, is a
/// failure. The exit code 0 is no failure, also with a warning on
/// stderr.
///
/// ```
/// use std::os::unix::process::ExitStatusExt;
/// use std::process::{ExitStatus, Output};
/// use riff::platform::systemd::restarted;
///
/// let out = |status: i32, stderr: &str| Output {
///     status: ExitStatus::from_raw(status),
///     stdout: Vec::new(),
///     stderr: stderr.into(),
/// };
/// let args = ["--no-block", "try-restart", "riff.service"];
/// assert!(restarted(&args, &out(0, "")).is_ok());
/// // The stop of the unit ends the child with a SIGTERM (15).
/// assert!(restarted(&args, &out(15, "")).is_ok());
/// // The exit code 1 (the wait status 1 << 8), with its error.
/// let e = restarted(&args, &out(1 << 8, "Access denied")).unwrap_err();
/// assert_eq!(e.to_string(), "systemctl --user --no-block try-restart riff.service: Access denied");
/// let e = restarted(&args, &out(15, "Failed to connect to bus")).unwrap_err();
/// assert!(e.to_string().ends_with(": Failed to connect to bus"));
/// ```
pub fn restarted(args: &[&str], out: &Output) -> Result<()> {
    use std::os::unix::process::ExitStatusExt;
    let stopped = out.status.signal().is_some() && out.stderr.trim_ascii().is_empty();
    if out.status.success() || stopped {
        return Ok(());
    }
    Err(failed(args, out))
}

/// The arguments of `systemctl --user` that give the unit `unit` the CPU
/// weight `weight`. With `--runtime`, systemd keeps the value in the
/// runtime folder of the user, not in a file of the person
/// (01M778VSP6Z29FCPGX5S76ME1T). A slice needs no unit file for it.
///
/// ```
/// assert_eq!(
///     riff::platform::systemd::weight_args("riff.slice", 1),
///     ["set-property", "--runtime", "riff.slice", "CPUWeight=1"]
/// );
/// ```
pub fn weight_args(unit: &str, weight: u16) -> [String; 4] {
    [
        "set-property".into(),
        "--runtime".into(),
        unit.into(),
        format!("CPUWeight={weight}"),
    ]
}

/// Gives the unit `unit` the CPU weight `weight`: `systemctl --user`
/// with [`weight_args`]. An exit that is not 0 is an error that names
/// the command.
pub async fn set_cpu_weight(unit: &str, weight: u16) -> Result<()> {
    let args = weight_args(unit, weight);
    let out = tokio::process::Command::new("systemctl")
        .arg("--user")
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .await
        .with_context(|| text::no_program("systemctl"))?;
    if !out.status.success() {
        return Err(failed(&args.each_ref().map(String::as_str), &out));
    }
    Ok(())
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
        let path = self.rewrite(unit)?;
        // It also moves the link of an older unit to the target of this
        // one (01M77AB12JTJAEEAND02DNK9NG).
        systemctl(&["reenable", UNIT], true)?;
        systemctl(&["restart", UNIT], true)?;
        Ok(path)
    }

    fn installed(&self) -> Result<Option<Unit>> {
        let Some(text) = self.installed_text()? else {
            return Ok(None);
        };
        Unit::parse(&text)
            .map(Some)
            .with_context(|| text::unit_unreadable(&self.unit_path()))
    }

    fn installed_text(&self) -> Result<Option<String>> {
        let path = self.unit_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        if text.lines().next() != Some(MARK) {
            bail!("{}", text::unit_not_of_riff(&path));
        }
        Ok(Some(text))
    }

    fn rewrite(&self, unit: &Unit) -> Result<PathBuf> {
        let path = self.unit_path();
        write_file(&path, &unit.text()?)?;
        systemctl(&["daemon-reload"], true)?;
        Ok(path)
    }

    fn try_restart(&self) -> Result<()> {
        let args = ["--no-block", "try-restart", UNIT];
        restarted(&args, &output(&args)?)
    }

    fn main_pid(&self) -> Result<Option<u32>> {
        let pid = systemctl(&["show", "--property=MainPID", "--value", UNIT], true)?;
        Ok(pid.parse().ok().filter(|pid| *pid != 0))
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

    /// True when the user manager has the target of a desktop active now:
    /// `systemctl --user is-active graphical-session.target`. A desktop
    /// such as GNOME or KDE Plasma starts it at each login. On a server, on
    /// WSL2, and on a host with no desktop open now, it is inactive.
    fn desktop(&self) -> bool {
        systemctl(&["is-active", DESKTOP_TARGET], false).is_ok_and(|state| state == "active")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// 01M53NEE2AVNHA8RD8N40FPH03: a child that a signal ends at the new
    /// start is no failure. An exit code that is not 0, or text on
    /// stderr, is a failure with its error.
    #[test]
    fn a_child_that_a_signal_ends_at_the_new_start_is_no_failure() {
        let run = |script: &str| {
            std::process::Command::new("sh")
                .args(["-c", script])
                .stdin(Stdio::null())
                .output()
                .unwrap()
        };
        let args = ["--no-block", "try-restart", UNIT];
        for signal in ["TERM", "KILL"] {
            let out = run(&format!("kill -{signal} $$"));
            assert!(!out.status.success());
            restarted(&args, &out).unwrap();
        }
        let failed = |script: &str| restarted(&args, &run(script)).unwrap_err().to_string();
        assert_eq!(
            failed("echo 'Failed to try-restart riff.service: Access denied' >&2; exit 1"),
            "systemctl --user --no-block try-restart riff.service: Failed to try-restart \
             riff.service: Access denied"
        );
        assert_eq!(
            failed("exit 5"),
            "systemctl --user --no-block try-restart riff.service: exit status: 5"
        );
        assert_eq!(
            failed("echo 'Failed to connect to bus' >&2; kill -TERM $$"),
            "systemctl --user --no-block try-restart riff.service: Failed to connect to bus"
        );
        restarted(&args, &run("echo 'Warning: the unit changed on disk' >&2")).unwrap();
    }

    /// The unit that riff wrote comes back from its file, with no
    /// systemctl (01M517BTDRKDXD3X84P6AEAF7P).
    #[test]
    fn the_installed_unit_is_the_unit_that_riff_wrote() {
        let root = tempfile::tempdir().unwrap();
        let systemd = Systemd {
            dir: root.path().join("systemd/user"),
        };
        assert_eq!(systemd.installed().unwrap(), None);

        let unit = Unit {
            exe: "/home/mike/.cache/riff/check/target/static/release/riff".into(),
            env: vec![("PATH".into(), "/home/mike/.cargo/bin:/usr/bin".into())],
            desktop: false,
        };
        write_file(&systemd.unit_path(), &unit.text().unwrap()).unwrap();
        assert_eq!(systemd.installed().unwrap(), Some(unit));

        write_file(&systemd.unit_path(), "[Unit]\nDescription=mine\n").unwrap();
        let e = systemd.installed().unwrap_err().to_string();
        assert!(e.starts_with("riff did not write the unit "), "{e}");

        write_file(&systemd.unit_path(), &format!("{MARK}\n[Service]\n")).unwrap();
        let e = systemd.installed().unwrap_err().to_string();
        assert!(e.starts_with("riff cannot read the unit "), "{e}");
    }
}
