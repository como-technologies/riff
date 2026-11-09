//! launchd: the service manager of the daemon on macOS
//! (01M6Z49H6CHSPV0FJZWY2MYHEM). The agent [`LABEL`] of the person takes
//! the place of the user unit of systemd ([`crate::service`]). riff
//! builds this module on each OS, so that its tests also run on Linux.
//! On macOS, it is [`crate::platform::Manager`].
//!
//! # Design
//!
//! `riff daemon install` writes the plist of the agent
//! ([`Launchd::plist`]) in `~/Library/LaunchAgents`
//! (01M6Z4A4X2V5H4EE3A9RJ5RA6W). Then it runs these steps in the domain
//! `gui/UID` of the person:
//!
//! | Step | Command |
//! |---|---|
//! | Unload the agent, when it is loaded | `launchctl bootout gui/UID/LABEL` |
//! | Let it load | `launchctl enable gui/UID/LABEL` |
//! | Load it: `RunAtLoad` starts the daemon | `launchctl bootstrap gui/UID PLIST`, up to [`TRIES`] times, [`PAUSE`] apart |
//!
//! `riff daemon uninstall` unloads the agent, and deletes only a plist
//! whose second line is [`MARK`] (01M6Z4A4ZHF2ZBBZ1RC1ZHVSEH). The state
//! and the process of the daemon come from `launchctl print
//! gui/UID/LABEL` ([`state_of`], 01M6Z4A521PMRBK1R1E7NNADCQ).
//!
//! launchd reads a plist only when it loads it. So a new start of the
//! daemon unloads and loads the agent ([`reload_script`],
//! 01M6Z4A54HKQQHNNXDABMMSFH8). A process group of its own runs the two
//! steps: when launchd stops the daemon, it ends each process of the
//! group of the daemon, and that group no more holds them.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as riff daemon
//!     participant S as sh, in a process group of its own
//!     participant L as launchd
//!     D->>S: start it, and do not wait
//!     S->>L: launchctl bootout gui/UID/LABEL
//!     L->>D: SIGTERM: the daemon stops
//!     S->>L: launchctl bootstrap gui/UID PLIST
//!     L->>L: RunAtLoad: a new riff daemon
//! ```
//!
//! ```
//! use riff::platform::launchd::Launchd;
//! use riff::service::Unit;
//!
//! let launchd = Launchd::from(Some("/Users/mike".into()), 501)?;
//! let unit = Unit {
//!     exe: "/Users/mike/.cargo/bin/riff".into(),
//!     env: vec![("PATH".into(), "/opt/homebrew/bin:/usr/bin".into())],
//! };
//! let text = launchd.plist(&unit)?;
//! assert!(text.contains("\t\t<string>/Users/mike/.cargo/bin/riff</string>\n\t\t<string>daemon</string>\n"));
//! assert!(text.contains("\t<key>RunAtLoad</key>\n\t<true/>\n"));
//! assert_eq!(Launchd::parse(&text), Some(unit));
//! assert_eq!(launchd.target(), "gui/501/io.github.como-technologies.riff");
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::service::{ServiceManager, Unit};
use crate::text;

/// The label of the agent of the daemon.
pub const LABEL: &str = "io.github.como-technologies.riff";

/// The second line of each plist that riff writes: a plist starts with
/// the line of XML.
pub const MARK: &str =
    "<!-- The agent of the riff daemon. riff daemon install writes this file. -->";

/// The most tries of a `launchctl bootstrap`. launchd can refuse it for
/// a short time after a `launchctl bootout`.
pub const TRIES: u32 = 10;

/// The pause between two tries of a `launchctl bootstrap`.
pub const PAUSE: Duration = Duration::from_millis(500);

/// launchd, for the agents of the person.
#[derive(Debug, Clone)]
pub struct Launchd {
    /// The folder of the agents: `~/Library/LaunchAgents`.
    pub dir: PathBuf,
    /// The log of the daemon: `~/Library/Logs/riff/daemon.log`.
    pub log: PathBuf,
    /// The domain of the agent: `gui/UID`.
    pub domain: String,
    /// The program `launchctl`. A test gives a fake one.
    pub launchctl: PathBuf,
}

impl Launchd {
    /// launchd for the person with the home `home` and the user ID `uid`
    /// (01M6Z49RAPM0SRYHY7AE3S6XY8).
    ///
    /// ```
    /// use riff::platform::launchd::Launchd;
    /// use std::path::Path;
    ///
    /// let launchd = Launchd::from(Some("/Users/mike".into()), 501)?;
    /// assert_eq!(launchd.dir, Path::new("/Users/mike/Library/LaunchAgents"));
    /// assert_eq!(launchd.log, Path::new("/Users/mike/Library/Logs/riff/daemon.log"));
    /// assert_eq!(launchd.domain, "gui/501");
    /// assert_eq!(
    ///     launchd.unit_path(),
    ///     Path::new("/Users/mike/Library/LaunchAgents/io.github.como-technologies.riff.plist")
    /// );
    /// assert!(Launchd::from(Some("".into()), 501).is_err());
    /// assert!(Launchd::from(None, 501).is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn from(home: Option<OsString>, uid: u32) -> Result<Launchd> {
        let Some(home) = home.filter(|h| !h.is_empty()).map(PathBuf::from) else {
            bail!("set HOME");
        };
        Ok(Launchd {
            dir: home.join("Library/LaunchAgents"),
            log: home.join("Library/Logs/riff/daemon.log"),
            domain: format!("gui/{uid}"),
            launchctl: "launchctl".into(),
        })
    }

    /// [`Launchd::from`] with the values of this process.
    pub fn here() -> Result<Launchd> {
        Launchd::from(std::env::var_os("HOME"), nix::unistd::getuid().as_raw())
    }

    /// The path of the plist.
    pub fn unit_path(&self) -> PathBuf {
        self.dir.join(format!("{LABEL}.plist"))
    }

    /// The service of the agent in its domain: `gui/UID/LABEL`.
    pub fn target(&self) -> String {
        format!("{}/{LABEL}", self.domain)
    }

    /// The text of the plist of `unit` (01M6Z4A4X2V5H4EE3A9RJ5RA6W). riff
    /// refuses a text with a control character.
    ///
    /// ```
    /// use riff::platform::launchd::Launchd;
    /// use riff::service::Unit;
    ///
    /// let launchd = Launchd::from(Some("/Users/mike".into()), 501)?;
    /// let unit = Unit { exe: "/opt/a&b/riff".into(), env: vec![("A".into(), "<x>".into())] };
    /// let text = launchd.plist(&unit)?;
    /// assert!(text.contains("<string>/opt/a&amp;b/riff</string>"));
    /// assert!(text.contains("\t\t<key>A</key>\n\t\t<string>&lt;x&gt;</string>\n"));
    /// let bad = Unit { exe: "/opt/riff\n".into(), env: vec![] };
    /// assert!(launchd.plist(&bad).is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn plist(&self, unit: &Unit) -> Result<String> {
        let exe = unit
            .exe
            .to_str()
            .with_context(|| format!("the path of riff {} is not UTF-8", unit.exe.display()))?;
        let log = self
            .log
            .to_str()
            .with_context(|| format!("the path {} is not UTF-8", self.log.display()))?;
        let mut out = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             {MARK}\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
             \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n\
             <dict>\n\
             \t<key>Label</key>\n\
             \t<string>{LABEL}</string>\n\
             \t<key>ProgramArguments</key>\n\
             \t<array>\n\
             \t\t{}\n\
             \t\t<string>daemon</string>\n\
             \t</array>\n",
            string(exe)?
        );
        if !unit.env.is_empty() {
            out.push_str("\t<key>EnvironmentVariables</key>\n\t<dict>\n");
            for (name, value) in &unit.env {
                out.push_str(&format!(
                    "\t\t<key>{}</key>\n\t\t{}\n",
                    escaped(name)?,
                    string(value)?
                ));
            }
            out.push_str("\t</dict>\n");
        }
        let log = string(log)?;
        out.push_str(&format!(
            "\t<key>RunAtLoad</key>\n\
             \t<true/>\n\
             \t<key>KeepAlive</key>\n\
             \t<dict>\n\
             \t\t<key>SuccessfulExit</key>\n\
             \t\t<false/>\n\
             \t</dict>\n\
             \t<key>StandardOutPath</key>\n\
             \t{log}\n\
             \t<key>StandardErrorPath</key>\n\
             \t{log}\n\
             </dict>\n\
             </plist>\n"
        ));
        Ok(out)
    }

    /// The unit in `text`, the text of a plist that [`Launchd::plist`]
    /// wrote: its `ProgramArguments` and its `EnvironmentVariables`.
    /// `None` when it has no `ProgramArguments` of `riff daemon`.
    ///
    /// ```
    /// use riff::platform::launchd::Launchd;
    /// use riff::service::Unit;
    ///
    /// let launchd = Launchd::from(Some("/Users/mike".into()), 501)?;
    /// let unit = Unit { exe: "/opt/my <riff>/riff".into(), env: vec![("PATH".into(), "/a b:/c&d".into())] };
    /// assert_eq!(Launchd::parse(&launchd.plist(&unit)?), Some(unit));
    /// let relay = "\t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>/opt/riff</string>\n\t\t<string>relay</string>\n\t</array>\n";
    /// assert_eq!(Launchd::parse(relay), None);
    /// assert_eq!(Launchd::parse("<plist version=\"1.0\">\n</plist>\n"), None);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn parse(text: &str) -> Option<Unit> {
        let mut lines = text.lines().map(str::trim);
        let (mut exe, mut env) = (None, Vec::new());
        while let Some(line) = lines.next() {
            match line {
                "<key>ProgramArguments</key>" => {
                    if lines.next()? != "<array>" {
                        return None;
                    }
                    let path = string_in(lines.next()?)?;
                    if string_in(lines.next()?)? != "daemon" || lines.next()? != "</array>" {
                        return None;
                    }
                    exe = Some(PathBuf::from(path));
                }
                "<key>EnvironmentVariables</key>" => {
                    if lines.next()? != "<dict>" {
                        return None;
                    }
                    loop {
                        let line = lines.next()?;
                        if line == "</dict>" {
                            break;
                        }
                        let name = line.strip_prefix("<key>")?.strip_suffix("</key>")?;
                        env.push((unescaped(name), string_in(lines.next()?)?));
                    }
                }
                _ => {}
            }
        }
        Some(Unit { exe: exe?, env })
    }

    /// Runs `launchctl ARGS`, and gives its output.
    fn output(&self, args: &[&str]) -> Result<Output> {
        Command::new(&self.launchctl)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .with_context(|| text::no_program("launchctl"))
    }

    /// Runs `launchctl ARGS`. An exit that is not 0 is an error that
    /// names the command.
    fn run(&self, args: &[&str]) -> Result<()> {
        let out = self.output(args)?;
        if !out.status.success() {
            return Err(failed(args, &out));
        }
        Ok(())
    }

    /// The output of `launchctl print gui/UID/LABEL`, or `None` when the
    /// agent is not loaded.
    fn print(&self) -> Result<Option<String>> {
        let out = self.output(&["print", &self.target()])?;
        Ok(out
            .status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned()))
    }

    /// Unloads the agent when it is loaded.
    fn bootout(&self) -> Result<()> {
        if self.print()?.is_some() {
            self.run(&["bootout", &self.target()])?;
        }
        Ok(())
    }

    /// Loads the plist, up to [`TRIES`] times, [`PAUSE`] apart.
    fn bootstrap(&self) -> Result<()> {
        let path = self.unit_path();
        let path = path.to_string_lossy();
        let args = ["bootstrap", self.domain.as_str(), &path];
        let mut tries = 1;
        loop {
            let out = self.output(&args)?;
            if out.status.success() {
                return Ok(());
            }
            if tries >= TRIES {
                return Err(failed(&args, &out));
            }
            tries += 1;
            std::thread::sleep(PAUSE);
        }
    }
}

/// `text` for a value in a plist: `&`, `<` and `>` as entities. riff
/// refuses a text with a control character: no line of a plist that riff
/// writes can hold it.
fn escaped(text: &str) -> Result<String> {
    if text.chars().any(char::is_control) {
        bail!("{}", text::unit_bad_text(text));
    }
    Ok(text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;"))
}

/// `text` in a `<string>` of a plist.
fn string(text: &str) -> Result<String> {
    Ok(format!("<string>{}</string>", escaped(text)?))
}

/// `text` with the entities of XML undone.
fn unescaped(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The value of the `<string>` in `line`, or `None`.
fn string_in(line: &str) -> Option<String> {
    let value = line.strip_prefix("<string>")?.strip_suffix("</string>")?;
    Some(unescaped(value))
}

/// The error of `launchctl ARGS` that gave `out`: the command, and its
/// stderr, or its exit when stderr has no text.
fn failed(args: &[&str], out: &Output) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let why = match stderr.trim() {
        "" => out.status.to_string(),
        text => text.to_owned(),
    };
    anyhow::anyhow!("launchctl {}: {why}", args.join(" "))
}

/// The value of the field `name` of the service in `print`, the output
/// of `launchctl print`: a line with one tab, `NAME = VALUE`. A line of a
/// part of the service has more tabs.
fn field<'a>(print: &'a str, name: &str) -> Option<&'a str> {
    print.lines().find_map(|line| {
        let (key, value) = line.strip_prefix('\t')?.split_once(" = ")?;
        (key == name).then_some(value.trim())
    })
}

/// The state of the agent in the words of systemd, from `print`, the
/// output of `launchctl print`, or `None` for an agent that is not
/// loaded (01M6Z4A521PMRBK1R1E7NNADCQ). So [`crate::daemon::starts`]
/// reads it as it reads a state of systemd.
///
/// ```
/// use riff::platform::launchd::state_of;
///
/// let print = |state: &str, exit: &str| {
///     format!("gui/501/io.github.como-technologies.riff = {{\n\tstate = {state}\n\tlast exit code = {exit}\n\tendpoints = {{\n\t\tstate = x\n\t}}\n}}\n")
/// };
/// assert_eq!(state_of(Some(&print("running", "(never exited)"))), "active");
/// assert_eq!(state_of(Some(&print("spawn scheduled", "1"))), "activating");
/// assert_eq!(state_of(Some(&print("not running", "78: EX_CONFIG"))), "failed");
/// assert_eq!(state_of(Some(&print("not running", "0"))), "inactive");
/// assert_eq!(state_of(Some(&print("not running", "(never exited)"))), "inactive");
/// assert_eq!(state_of(None), "inactive");
/// ```
pub fn state_of(print: Option<&str>) -> &'static str {
    let Some(print) = print else {
        return "inactive";
    };
    match field(print, "state") {
        Some("running") => "active",
        Some("spawn scheduled" | "xpcproxy") => "activating",
        _ => {
            let code = field(print, "last exit code")
                .and_then(|code| code.split(':').next()?.trim().parse::<i64>().ok());
            match code {
                Some(code) if code != 0 => "failed",
                _ => "inactive",
            }
        }
    }
}

/// The script of a new start of the daemon (01M6Z4A54HKQQHNNXDABMMSFH8):
/// `$1` is `launchctl`, `$2` the service, `$3` the domain and `$4` the
/// plist. It unloads the agent, then loads it, up to [`TRIES`] times,
/// [`PAUSE`] apart.
///
/// ```
/// let script = riff::platform::launchd::reload_script();
/// assert!(script.starts_with("\"$1\" bootout \"$2\";"));
/// assert!(script.contains("until \"$1\" bootstrap \"$3\" \"$4\"; do [ \"$n\" -ge 10 ]"));
/// assert!(script.contains("sleep 0.5"));
/// ```
pub fn reload_script() -> String {
    format!(
        "\"$1\" bootout \"$2\"; n=1; until \"$1\" bootstrap \"$3\" \"$4\"; do \
         [ \"$n\" -ge {TRIES} ] && exit 1; n=$((n + 1)); sleep {}; done",
        PAUSE.as_secs_f32()
    )
}

/// Writes `text` to `path`, through a new file and a rename.
fn write_file(path: &Path, text: &str) -> Result<()> {
    let dir = path.parent().context("a plist has a folder")?;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let new = path.with_extension("plist.new");
    std::fs::write(&new, text).with_context(|| format!("cannot write {}", new.display()))?;
    std::fs::rename(&new, path).with_context(|| format!("cannot write {}", path.display()))
}

impl ServiceManager for Launchd {
    fn install(&self, unit: &Unit) -> Result<PathBuf> {
        let path = self.rewrite(unit)?;
        self.bootout()?;
        self.run(&["enable", &self.target()])?;
        self.bootstrap()?;
        Ok(path)
    }

    fn uninstall(&self) -> Result<Option<PathBuf>> {
        if self.installed_text()?.is_none() {
            return Ok(None);
        }
        self.bootout()?;
        let path = self.unit_path();
        std::fs::remove_file(&path).with_context(|| format!("cannot delete {}", path.display()))?;
        Ok(Some(path))
    }

    fn state(&self) -> Result<String> {
        Ok(state_of(self.print()?.as_deref()).to_owned())
    }

    fn installed(&self) -> Result<Option<Unit>> {
        let Some(text) = self.installed_text()? else {
            return Ok(None);
        };
        Launchd::parse(&text)
            .map(Some)
            .with_context(|| text::agent_unreadable(&self.unit_path()))
    }

    fn installed_text(&self) -> Result<Option<String>> {
        let path = self.unit_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        if text.lines().nth(1) != Some(MARK) {
            bail!("{}", text::agent_not_of_riff(&path, &self.target()));
        }
        Ok(Some(text))
    }

    /// Writes the plist, and makes the folder of the log: launchd makes
    /// no folder. launchd reads the plist at the next load.
    fn rewrite(&self, unit: &Unit) -> Result<PathBuf> {
        let path = self.unit_path();
        write_file(&path, &self.plist(unit)?)?;
        if let Some(dir) = self.log.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot make {}", dir.display()))?;
        }
        Ok(path)
    }

    /// Starts [`reload_script`] in a process group of its own, when the
    /// agent is loaded, and does not wait for it. A thread waits for its
    /// end, so it leaves no zombie.
    fn try_restart(&self) -> Result<()> {
        use std::os::unix::process::CommandExt;
        if self.print()?.is_none() {
            return Ok(());
        }
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(reload_script())
            .arg("sh")
            .arg(&self.launchctl)
            .arg(self.target())
            .arg(&self.domain)
            .arg(self.unit_path())
            .process_group(0)
            .stdin(Stdio::null())
            .spawn()
            .with_context(|| text::no_program("sh"))?;
        std::thread::spawn(move || child.wait());
        Ok(())
    }

    fn main_pid(&self) -> Result<Option<u32>> {
        Ok(self
            .print()?
            .and_then(|print| field(&print, "pid")?.parse().ok())
            .filter(|pid| *pid != 0))
    }

    fn text(&self, unit: &Unit) -> Result<String> {
        self.plist(unit)
    }

    fn unit_in(&self, text: &str) -> Option<Unit> {
        Launchd::parse(text)
    }

    fn unit_name(&self) -> String {
        LABEL.to_owned()
    }

    fn log_command(&self) -> String {
        format!("tail -f {}", self.log.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake `launchctl` in `bin`: it logs each call in `launchctl.log`.
    /// The agent is loaded while the file `loaded` is there: `bootstrap`
    /// makes it, and `bootout` deletes it. `print` shows the state and the
    /// pid 4242 of a loaded agent. With the file `refuse`, a `bootstrap`
    /// fails, and takes one line of the file away.
    fn fake(bin: &Path) -> PathBuf {
        let body = r#"#!/bin/sh
dir=$(dirname "$0")
echo "launchctl $*" >> "$dir/launchctl.log"
case "$1" in
  print)
    [ -f "$dir/loaded" ] || { echo "Could not find service" >&2; exit 113; }
    printf '%s = {\n\tstate = running\n\tpid = 4242\n\tlast exit code = (never exited)\n}\n' "$2" ;;
  bootout)
    [ -f "$dir/loaded" ] || { echo "Boot-out failed: 3: No such process" >&2; exit 3; }
    rm "$dir/loaded" ;;
  bootstrap)
    if [ -s "$dir/refuse" ]; then
      sed -i.old 1d "$dir/refuse"
      echo "Bootstrap failed: 5: Input/output error" >&2
      exit 5
    fi
    touch "$dir/loaded" ;;
esac
exit 0
"#;
        isolated::write_program(bin.join("launchctl"), body)
    }

    /// launchd in a temp home, with the fake `launchctl` of [`fake`].
    struct Here {
        _root: tempfile::TempDir,
        bin: PathBuf,
        launchd: Launchd,
    }

    impl Here {
        fn new() -> Here {
            let root = tempfile::tempdir().unwrap();
            let bin = root.path().join("bin");
            std::fs::create_dir(&bin).unwrap();
            let mut launchd = Launchd::from(Some(root.path().join("home").into()), 501).unwrap();
            launchd.launchctl = fake(&bin);
            Here {
                _root: root,
                bin,
                launchd,
            }
        }

        fn log(&self) -> String {
            std::fs::read_to_string(self.bin.join("launchctl.log")).unwrap_or_default()
        }

        fn loaded(&self) -> bool {
            self.bin.join("loaded").exists()
        }
    }

    fn unit() -> Unit {
        Unit {
            exe: "/Users/mike/.cargo/bin/riff".into(),
            env: vec![
                ("PATH".into(), "/opt/homebrew/bin:/usr/bin".into()),
                ("XDG_DATA_HOME".into(), "/Users/mike/.local/share".into()),
            ],
        }
    }

    /// 01M6Z4A4X2V5H4EE3A9RJ5RA6W: the plist of the daemon, line by line.
    #[test]
    fn the_plist_runs_riff_daemon_with_its_variables_and_its_log() {
        let launchd = Launchd::from(Some("/Users/mike".into()), 501).unwrap();
        assert_eq!(
            launchd.plist(&unit()).unwrap(),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!-- The agent of the riff daemon. riff daemon install writes this file. -->\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
             \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n\
             <dict>\n\
             \t<key>Label</key>\n\
             \t<string>io.github.como-technologies.riff</string>\n\
             \t<key>ProgramArguments</key>\n\
             \t<array>\n\
             \t\t<string>/Users/mike/.cargo/bin/riff</string>\n\
             \t\t<string>daemon</string>\n\
             \t</array>\n\
             \t<key>EnvironmentVariables</key>\n\
             \t<dict>\n\
             \t\t<key>PATH</key>\n\
             \t\t<string>/opt/homebrew/bin:/usr/bin</string>\n\
             \t\t<key>XDG_DATA_HOME</key>\n\
             \t\t<string>/Users/mike/.local/share</string>\n\
             \t</dict>\n\
             \t<key>RunAtLoad</key>\n\
             \t<true/>\n\
             \t<key>KeepAlive</key>\n\
             \t<dict>\n\
             \t\t<key>SuccessfulExit</key>\n\
             \t\t<false/>\n\
             \t</dict>\n\
             \t<key>StandardOutPath</key>\n\
             \t<string>/Users/mike/Library/Logs/riff/daemon.log</string>\n\
             \t<key>StandardErrorPath</key>\n\
             \t<string>/Users/mike/Library/Logs/riff/daemon.log</string>\n\
             </dict>\n\
             </plist>\n"
        );
        // A unit with no variables has no EnvironmentVariables.
        let bare = Unit {
            exe: "/opt/riff".into(),
            env: vec![],
        };
        let text = launchd.plist(&bare).unwrap();
        assert!(!text.contains("EnvironmentVariables"), "{text}");
        assert_eq!(Launchd::parse(&text), Some(bare));
        let bad = Unit {
            exe: "/opt/riff".into(),
            env: vec![("PATH".into(), "/a\n</string>".into())],
        };
        assert!(launchd.plist(&bad).is_err());
    }

    /// 01M6Z4A4X2V5H4EE3A9RJ5RA6W: the install writes the plist and reads
    /// it back, makes the folder of the log, and loads the agent. A second
    /// install unloads the agent first.
    #[test]
    fn the_install_writes_and_reads_the_plist_and_loads_the_agent() {
        let here = Here::new();
        let launchd = &here.launchd;
        assert_eq!(launchd.installed().unwrap(), None);
        assert_eq!(launchd.state().unwrap(), "inactive");
        assert_eq!(launchd.main_pid().unwrap(), None);
        std::fs::remove_file(here.bin.join("launchctl.log")).unwrap();

        let path = launchd.install(&unit()).unwrap();
        assert_eq!(path, launchd.unit_path());
        assert_eq!(launchd.installed().unwrap(), Some(unit()));
        let text = launchd.installed_text().unwrap().unwrap();
        assert_eq!(text, launchd.plist(&unit()).unwrap());
        assert_eq!(launchd.unit_in(&text), Some(unit()));
        assert!(launchd.log.parent().unwrap().is_dir());
        assert!(here.loaded());
        let plist = path.display();
        assert_eq!(
            here.log(),
            format!(
                "launchctl print gui/501/{LABEL}\n\
                 launchctl enable gui/501/{LABEL}\n\
                 launchctl bootstrap gui/501 {plist}\n"
            )
        );
        assert_eq!(launchd.state().unwrap(), "active");
        assert_eq!(launchd.main_pid().unwrap(), Some(4242));

        std::fs::remove_file(here.bin.join("launchctl.log")).unwrap();
        launchd.install(&unit()).unwrap();
        assert_eq!(
            here.log(),
            format!(
                "launchctl print gui/501/{LABEL}\n\
                 launchctl bootout gui/501/{LABEL}\n\
                 launchctl enable gui/501/{LABEL}\n\
                 launchctl bootstrap gui/501 {plist}\n"
            )
        );
    }

    /// 01M6Z4A4X2V5H4EE3A9RJ5RA6W: a bootstrap that launchd refuses for a
    /// short time comes in a later try. After [`TRIES`] refusals, the
    /// install fails with the error of launchctl.
    #[test]
    fn the_install_tries_the_bootstrap_again() {
        let here = Here::new();
        std::fs::write(here.bin.join("refuse"), "1\n2\n").unwrap();
        here.launchd.install(&unit()).unwrap();
        assert!(here.loaded());
        assert_eq!(here.log().matches(" bootstrap ").count(), 3);

        let here = Here::new();
        std::fs::write(here.bin.join("refuse"), "x\n".repeat(TRIES as usize)).unwrap();
        let e = here.launchd.install(&unit()).unwrap_err().to_string();
        assert!(
            e.starts_with("launchctl bootstrap gui/501 ")
                && e.ends_with(": Bootstrap failed: 5: Input/output error"),
            "{e}"
        );
        assert_eq!(here.log().matches(" bootstrap ").count(), TRIES as usize);
    }

    /// 01M6Z4A4ZHF2ZBBZ1RC1ZHVSEH: the uninstall unloads the agent and
    /// deletes the plist. With no plist, it says none, and runs no
    /// launchctl. A plist of another stays.
    #[test]
    fn the_uninstall_unloads_the_agent_and_keeps_a_plist_of_another() {
        let here = Here::new();
        let launchd = &here.launchd;
        assert_eq!(launchd.uninstall().unwrap(), None);
        assert_eq!(here.log(), "");

        launchd.install(&unit()).unwrap();
        std::fs::remove_file(here.bin.join("launchctl.log")).unwrap();
        assert_eq!(launchd.uninstall().unwrap(), Some(launchd.unit_path()));
        assert!(!launchd.unit_path().exists());
        assert!(!here.loaded());
        assert_eq!(
            here.log(),
            format!("launchctl print gui/501/{LABEL}\nlaunchctl bootout gui/501/{LABEL}\n")
        );
        // The folder of the log stays.
        assert!(launchd.log.parent().unwrap().is_dir());

        // An agent that is not loaded: riff only deletes the plist.
        launchd.rewrite(&unit()).unwrap();
        std::fs::remove_file(here.bin.join("launchctl.log")).unwrap();
        assert_eq!(launchd.uninstall().unwrap(), Some(launchd.unit_path()));
        assert_eq!(here.log(), format!("launchctl print gui/501/{LABEL}\n"));

        write_file(&launchd.unit_path(), "<?xml version=\"1.0\"?>\n<plist/>\n").unwrap();
        let e = launchd.uninstall().unwrap_err().to_string();
        assert!(e.starts_with("riff did not write the agent "), "{e}");
        assert!(launchd.unit_path().exists());
        let e = launchd.installed().unwrap_err().to_string();
        assert!(e.starts_with("riff did not write the agent "), "{e}");

        write_file(
            &launchd.unit_path(),
            &format!("<?xml?>\n{MARK}\n<plist/>\n"),
        )
        .unwrap();
        let e = launchd.installed().unwrap_err().to_string();
        assert!(e.starts_with("riff cannot read the agent "), "{e}");
    }

    /// 01M6Z4A54HKQQHNNXDABMMSFH8: a rewrite only writes the plist. A new
    /// start unloads and loads the agent, and does nothing for an agent
    /// that is not loaded.
    #[test]
    fn a_new_start_unloads_and_loads_the_agent() {
        let here = Here::new();
        let launchd = &here.launchd;
        launchd.rewrite(&unit()).unwrap();
        assert_eq!(launchd.installed().unwrap(), Some(unit()));
        assert_eq!(here.log(), "");
        launchd.try_restart().unwrap();
        assert_eq!(here.log(), format!("launchctl print gui/501/{LABEL}\n"));

        launchd.install(&unit()).unwrap();
        std::fs::remove_file(here.bin.join("launchctl.log")).unwrap();
        launchd.try_restart().unwrap();
        let want = format!(
            "launchctl print gui/501/{LABEL}\n\
             launchctl bootout gui/501/{LABEL}\n\
             launchctl bootstrap gui/501 {}\n",
            launchd.unit_path().display()
        );
        // The fake logs the bootstrap before it loads the agent: wait for
        // both.
        let span = isolated::Span::start();
        while here.log() != want || !here.loaded() {
            assert!(
                span.within(Duration::from_secs(10)),
                "{}\nis not\n{want}, or the agent is not loaded",
                here.log()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn the_unit_has_the_name_of_the_agent_and_its_log() {
        let launchd = Launchd::from(Some("/Users/mike".into()), 501).unwrap();
        assert_eq!(launchd.unit_name(), LABEL);
        assert_eq!(
            launchd.log_command(),
            "tail -f /Users/mike/Library/Logs/riff/daemon.log"
        );
    }
}
