//! The service manager of riff (01M4Z60B7GTV4HHF9R3TMZRS7J): it
//! installs and removes the user unit of the daemon, and shows its
//! state. On Linux, it is systemd
//! ([`Systemd`](crate::platform::systemd::Systemd)), through
//! [`crate::platform::service_manager`] (01M6YQDWT387EV7Y7XMT2G6YVM). On
//! macOS, it is launchd ([`Launchd`](crate::platform::launchd::Launchd),
//! 01M6Z49H6CHSPV0FJZWY2MYHEM): the steps of its agent are there.
//!
//! # Design
//!
//! On Linux, `riff daemon install` writes the unit [`UNIT`] in the folder of the
//! user units of systemd: `XDG_CONFIG_HOME/systemd/user`, else
//! `HOME/.config/systemd/user` (01M4ZDTVK74596M0ZA46N51NKV). Then it
//! runs these steps:
//!
//! | Step | Command |
//! |---|---|
//! | Read the unit again | `systemctl --user daemon-reload` |
//! | Start it at each login, and move the link of an older unit | `systemctl --user reenable riff.service` |
//! | Start it, or start it again with this riff | `systemctl --user restart riff.service` |
//!
//! # The start at a login
//!
//! On a host with a desktop, the daemon starts after the desktop of each
//! login (01M77AB12JTJAEEAND02DNK9NG). Before the desktop, the keyring
//! can show no unlock prompt, and a login with a security key leaves the
//! keyring locked. `riff daemon install` asks the service manager
//! whether the host has a desktop ([`ServiceManager::desktop`]): on
//! Linux, whether the user manager of systemd has [`DESKTOP_TARGET`]
//! active. The answer goes in [`Unit::desktop`]:
//!
//! | The host | In `[Unit]` | In `[Install]` |
//! |---|---|---|
//! | With a desktop | `After=graphical-session.target` | `WantedBy=graphical-session.target` |
//! | With no desktop, for example a server or WSL2 | | `WantedBy=default.target` |
//!
//! ```mermaid
//! flowchart LR
//!     L["a login"] --> U["the user manager starts:<br/>default.target"]
//!     U -- "a host with no desktop" --> D["riff daemon"]
//!     L --> G["the desktop starts:<br/>graphical-session.target"]
//!     G -- "a host with a desktop" --> D
//! ```
//!
//! The `reenable` moves the link of an older unit from
//! `default.target.wants` to the new target. A write of the unit after an
//! update keeps its target ([`Unit::parse`]).
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
//! and `riff` use one folder ([`crate::daemon::dir`]). It also gets
//! `CLAUDE_CONFIG_DIR` when it is set, so the daemon uses the same Claude
//! folder as the person (01M51M3ZTV61C9BMFT3J1M06NF).
//!
//! After an update of riff, the daemon runs the new riff
//! ([`crate::daemon::after_update`], [`crate::daemon::Daemon::restart_when_old`]).
//! These steps read the unit file again ([`Unit::parse`]):
//!
//! | Step | Command |
//! |---|---|
//! | Write the unit with another riff, and read it again | `systemctl --user daemon-reload` |
//! | Start it again when it runs, and do not wait | `systemctl --user --no-block try-restart riff.service` |
//! | The process of the daemon | `systemctl --user show --property=MainPID --value riff.service` |
//!
//! The new start stops each process of the unit, also the `systemctl`
//! child of the daemon that asked for it. So that child can end by a
//! signal before it exits. With no text on stderr, the new start runs:
//! it is no failure
//! ([`restarted`](crate::platform::systemd::restarted),
//! 01M53NEE2AVNHA8RD8N40FPH03).
//!
//! ```
//! use riff::service::Unit;
//!
//! let unit = Unit {
//!     exe: "/home/mike/.cargo/bin/riff".into(),
//!     env: vec![("PATH".into(), "/home/mike/.cargo/bin:/usr/bin".into())],
//!     desktop: false,
//! };
//! let text = unit.text()?;
//! assert!(text.contains("ExecStart=\"/home/mike/.cargo/bin/riff\" daemon\n"));
//! assert!(text.contains("Environment=\"PATH=/home/mike/.cargo/bin:/usr/bin\"\n"));
//! assert!(text.contains("WantedBy=default.target\n"));
//! let desktop = Unit { desktop: true, ..unit }.text()?;
//! assert!(desktop.contains("After=graphical-session.target\n"));
//! assert!(desktop.contains("WantedBy=graphical-session.target\n"));
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::text;

/// The user unit of the daemon.
pub const UNIT: &str = "riff.service";

/// The first line of each unit file that riff writes.
pub const MARK: &str = "# The unit of the riff daemon. riff daemon install writes this file.";

/// The target of the user manager of systemd that the desktop of a login
/// starts, for example GNOME or KDE Plasma (01M77AB12JTJAEEAND02DNK9NG).
pub const DESKTOP_TARGET: &str = "graphical-session.target";

/// The target of the user manager of systemd at its start, before a
/// desktop: the target of the unit on a host with no desktop.
pub const DEFAULT_TARGET: &str = "default.target";

/// The user unit of the daemon as data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// The absolute path of `riff`.
    pub exe: PathBuf,
    /// The variables of the daemon.
    pub env: Vec<(String, String)>,
    /// True on a host with a desktop: the daemon starts after the
    /// desktop of each login, with [`DESKTOP_TARGET`]. Else it starts
    /// with the user manager, with [`DEFAULT_TARGET`]
    /// (01M77AB12JTJAEEAND02DNK9NG).
    pub desktop: bool,
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

/// The text in double quotes at the start of `text`, with the escapes
/// of [`quoted`] undone, and the rest of `text`. `None` when `text` has
/// no such text.
fn unquoted(text: &str, dollar: bool) -> Option<(String, &str)> {
    let rest = text.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => return Some((out, &rest[at + 1..])),
            '\\' => out.push(chars.next()?.1),
            // A doubled sign: `%%`, and `$$` in an `ExecStart`.
            '%' | '$' if c == '%' || dollar => {
                if chars.next()?.1 != c {
                    return None;
                }
                out.push(c);
            }
            c => out.push(c),
        }
    }
    None
}

impl Unit {
    /// The unit in `text`, the text of a unit file that [`Unit::text`]
    /// wrote: its `ExecStart`, its `Environment` lines and its target.
    /// `None` when it has no `ExecStart` of `riff daemon`.
    ///
    /// ```
    /// use riff::service::Unit;
    ///
    /// let unit = Unit {
    ///     exe: "/opt/my $riff/riff".into(),
    ///     env: vec![("PATH".into(), "/a b:/c\"d\\e%f$g".into())],
    ///     desktop: false,
    /// };
    /// assert_eq!(Unit::parse(&unit.text()?), Some(unit.clone()));
    /// let desktop = Unit { desktop: true, ..unit };
    /// assert_eq!(Unit::parse(&desktop.text()?), Some(desktop));
    /// assert_eq!(Unit::parse("[Service]\nExecStart=/opt/riff daemon\n"), None);
    /// assert_eq!(Unit::parse("[Service]\nExecStart=\"/opt/riff\" relay\n"), None);
    /// assert_eq!(Unit::parse("[Unit]\nDescription=mine\n"), None);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn parse(text: &str) -> Option<Unit> {
        let (mut exe, mut env, mut desktop) = (None, Vec::new(), false);
        for line in text.lines() {
            if line.strip_prefix("WantedBy=") == Some(DESKTOP_TARGET) {
                desktop = true;
            } else if let Some(value) = line.strip_prefix("ExecStart=") {
                let (path, rest) = unquoted(value, true)?;
                if rest.trim() != "daemon" {
                    return None;
                }
                exe = Some(PathBuf::from(path));
            } else if let Some(value) = line.strip_prefix("Environment=") {
                let (pair, _) = unquoted(value, false)?;
                let (name, value) = pair.split_once('=')?;
                env.push((name.to_owned(), value.to_owned()));
            }
        }
        Some(Unit {
            exe: exe?,
            env,
            desktop,
        })
    }

    /// The unit of this riff: the path of this binary on disk
    /// ([`crate::binary::this_on_disk`]), its `PATH`, and its
    /// `XDG_DATA_HOME` and `CLAUDE_CONFIG_DIR` when they are set
    /// (01M51M3ZTV61C9BMFT3J1M06NF). It has no desktop: the install asks
    /// the service manager for it ([`crate::daemon::install`]).
    pub fn of_this() -> Result<Unit> {
        let exe = crate::binary::this_on_disk().context("cannot find the path of this riff")?;
        let env = ["PATH", "XDG_DATA_HOME", "CLAUDE_CONFIG_DIR"]
            .into_iter()
            .filter_map(|name| {
                std::env::var(name)
                    .ok()
                    .filter(|v| !v.is_empty())
                    .map(|v| (name.to_owned(), v))
            })
            .collect();
        Ok(Unit {
            exe,
            env,
            desktop: false,
        })
    }

    /// The text of the unit file.
    ///
    /// ```
    /// use riff::service::Unit;
    ///
    /// let unit = Unit { exe: "/opt/my riff/riff".into(), env: vec![("PATH".into(), "/a%b$c".into())], desktop: false };
    /// let text = unit.text()?;
    /// assert!(text.contains("ExecStart=\"/opt/my riff/riff\" daemon\n"));
    /// assert!(text.contains("Environment=\"PATH=/a%%b$c\"\n"));
    /// let bad = Unit { exe: "/opt/riff\n".into(), env: vec![], desktop: false };
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
             Description=riff daemon: the sessions of riff on this machine\n",
        );
        if self.desktop {
            out.push_str(&format!("After={DESKTOP_TARGET}\n"));
        }
        out.push_str("\n[Service]\nType=simple\n");
        out.push_str(&format!("ExecStart={} daemon\n", quoted(exe, true)?));
        for (name, value) in &self.env {
            out.push_str(&format!(
                "Environment={}\n",
                quoted(&format!("{name}={value}"), false)?
            ));
        }
        let target = match self.desktop {
            true => DESKTOP_TARGET,
            false => DEFAULT_TARGET,
        };
        out.push_str(&format!(
            "Restart=on-failure\n\
             RestartSec=2\n\n\
             [Install]\n\
             WantedBy={target}\n",
        ));
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

    /// True when this host has a desktop now: the unit then starts after
    /// the desktop of each login ([`Unit::desktop`],
    /// 01M77AB12JTJAEEAND02DNK9NG). The default has none: a manager with
    /// no target of a desktop, for example launchd.
    fn desktop(&self) -> bool {
        false
    }

    /// The unit in the unit file, or `None` when no unit file is there.
    /// An error for a unit file that riff did not write.
    fn installed(&self) -> Result<Option<Unit>>;

    /// The text of the unit file, or `None` when no unit file is there.
    /// An error for a unit file that riff did not write. A manager that
    /// keeps no text gives the text of [`ServiceManager::installed`].
    fn installed_text(&self) -> Result<Option<String>> {
        self.installed()?.map(|unit| unit.text()).transpose()
    }

    /// Writes `unit`, and reloads the manager. It does not start the
    /// unit again (01M517BTDRKDXD3X84P6AEAF7P). Gives the path of the
    /// unit file.
    fn rewrite(&self, unit: &Unit) -> Result<PathBuf>;

    /// Asks the manager to start the unit again when it runs. It does
    /// not wait for the new start (01M517BTJ9EV4ZGPYRB6HXJJC9). A new
    /// start that stops the process that asks is no failure
    /// ([`restarted`](crate::platform::systemd::restarted),
    /// 01M53NEE2AVNHA8RD8N40FPH03).
    fn try_restart(&self) -> Result<()>;

    /// The process ID of the daemon that the unit runs, or `None` when
    /// it runs none.
    fn main_pid(&self) -> Result<Option<u32>>;

    /// The text of the unit file that the manager writes for `unit`. The
    /// default is the unit of systemd, [`Unit::text`].
    fn text(&self, unit: &Unit) -> Result<String> {
        unit.text()
    }

    /// The unit in `text`, the text of a unit file that
    /// [`ServiceManager::text`] wrote, or `None`. The default is the unit
    /// of systemd, [`Unit::parse`].
    fn unit_in(&self, text: &str) -> Option<Unit> {
        Unit::parse(text)
    }

    /// The name of the unit for a person. The default is the unit of
    /// systemd, [`UNIT`].
    fn unit_name(&self) -> String {
        UNIT.to_owned()
    }

    /// The command that shows the log of the daemon. The default is the
    /// journal of systemd.
    fn log_command(&self) -> String {
        "journalctl --user -u riff".to_owned()
    }

    /// True when the daemon keeps a log file of its own, with a size cap
    /// ([`crate::daemon::log`], #1858): the journal of systemd can rotate
    /// in a few hours on a busy host. The default is the journal of
    /// systemd.
    fn keeps_log(&self) -> bool {
        true
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
            desktop: false,
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
            desktop: false,
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
            desktop: false,
        };
        assert!(bad.text().is_err());
    }

    /// 01M77AB12JTJAEEAND02DNK9NG: the unit of a host with a desktop
    /// starts after the desktop of each login. The unit of a host with no
    /// desktop starts with the user manager. A unit keeps its target
    /// through a read of its file.
    #[test]
    fn the_unit_of_a_host_with_a_desktop_starts_after_the_desktop() {
        let unit = |desktop| Unit {
            exe: "/home/mike/bin/riff".into(),
            env: vec![("PATH".into(), "/usr/bin".into())],
            desktop,
        };
        assert_eq!(
            unit(true).text().unwrap(),
            "# The unit of the riff daemon. riff daemon install writes this file.\n\
             [Unit]\n\
             Description=riff daemon: the sessions of riff on this machine\n\
             After=graphical-session.target\n\n\
             [Service]\n\
             Type=simple\n\
             ExecStart=\"/home/mike/bin/riff\" daemon\n\
             Environment=\"PATH=/usr/bin\"\n\
             Restart=on-failure\n\
             RestartSec=2\n\n\
             [Install]\n\
             WantedBy=graphical-session.target\n"
        );
        let server = unit(false).text().unwrap();
        assert!(!server.contains("graphical-session"), "{server}");
        assert!(
            server.ends_with("[Install]\nWantedBy=default.target\n"),
            "{server}"
        );
        for desktop in [true, false] {
            let text = unit(desktop).text().unwrap();
            assert_eq!(Unit::parse(&text), Some(unit(desktop)), "{text}");
        }
    }
}
