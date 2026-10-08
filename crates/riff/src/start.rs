//! `riff` with no command: the one action that starts the riff
//! (01M4BSSWWEBVHZGXCVYMJ7D7PQ).
//!
//! # Design
//!
//! riff runs its own tmux server, with its own socket ([`SOCKET`]) and
//! its own config ([`CONFIG`]), so no tmux config or dotfile of the
//! person changes a riff pane (01M4BSSWYVJ1RTEM1PTH94S5DH). Each
//! repository has one tmux session in that server, named after the
//! repository ([`session_name`]). Its first window holds the lead.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as person
//!     participant R as riff
//!     participant S as riff-server
//!     participant T as tmux -L riff
//!     participant L as lead (claude)
//!     P->>R: riff
//!     R->>S: pauses, who
//!     R-->>P: the picker: each clone with its state
//!     P->>R: a number, or the path of a new clone
//!     R->>T: has-session -t =OWNER/REPO
//!     alt no session
//!         R->>T: new-session -d -s OWNER/REPO -c CLONE
//!         T->>L: claude --remote-control
//!         L->>T: riff mcp adds the riff tail pane
//!     end
//!     R->>T: attach-session (or switch-client inside riff)
//! ```
//!
//! - **The picker.** It lists the clones that riff knows on this host
//!   ([`known`]), and the clone of the directory where `riff` runs. For
//!   each, it shows the state of its repository: running or paused, and
//!   the live sessions (01M4BSSX1BN322T63HTW0KVSA5). The person picks a
//!   number, or names the path of a new clone. riff keeps each picked
//!   clone in the file [`CLONES`].
//! - **One lead for each repository.** When the tmux session of the
//!   repository runs, `riff` attaches to it and starts no second lead
//!   (01M4BSSX3RSK79ZSJZZB1S0NYF).
//! - **No credential of the person.** riff starts the tmux server with
//!   an empty environment and only the kept variables ([`server_env`]),
//!   and tmux copies no variable of a client at an attach. So the server
//!   and each pane hold no credential of the person
//!   (01M4C4WW15HGA1VEDFRBEMZAW7).
//! - **The lead.** It is `claude` with the flags of the lead
//!   ([`lead_args`]) in the main clone. `riff workers lead` starts it,
//!   with a temp folder of its own and the forge token of the lead,
//!   through [`crate::forge::ForgeEnv`] ([`lead_command`],
//!   01M4C4WQVZR49FDGPJMFW22GTM). Its flag settings hold the
//!   permission rules of the profile of the lead
//!   ([`write_lead_settings`]). The `riff mcp` of the lead adds
//!   the `riff tail` pane beside it, as before (see
//!   [`crate::terminal`]). The workers start in the same tmux server,
//!   because each pane has the `TMUX` of the riff server.
//!
//! ```
//! use riff::start::{session_name, SOCKET};
//! assert_eq!(SOCKET, "riff");
//! assert_eq!(session_name("como-technologies/riff.rs"), "como-technologies/riff_rs");
//! ```

use std::ffi::OsStr;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use riff_core::name::Repo;
use riff_core::wire::{RiffReply, RiffState, SessionInfo};

use crate::terminal::quote;

/// The socket name of the tmux server of riff: `tmux -L riff`.
pub const SOCKET: &str = "riff";

/// The file of the tmux config of riff, in the local files of riff.
pub const CONFIG_FILE: &str = "tmux.conf";

/// The file of the clones that riff knows, one path on each line, in
/// the local files of riff.
pub const CLONES: &str = "clones";

/// The tmux config of riff. tmux reads it in place of the config of
/// the person (01M4BSSWYVJ1RTEM1PTH94S5DH).
pub const CONFIG: &str = "\
# The tmux config of riff. riff writes this file at each start.
set -g default-terminal tmux-256color
set -g history-limit 50000
set -g mouse on
set -g base-index 1
set -g pane-base-index 1
set -g renumber-windows on
set -g status-left '[riff #S] '
set -g status-left-length 60
set -g set-titles on
set -g set-titles-string 'riff #S'
set -g update-environment ''
";

/// A clone that the picker shows: its main worktree and its repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clone {
    /// The main worktree.
    pub path: PathBuf,
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
}

/// A line of the picker: a clone and the state of its repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The clone.
    pub clone: Clone,
    /// Running or paused. `None` when riff-server did not answer.
    pub state: Option<RiffState>,
    /// The live sessions of the repository.
    pub live: usize,
}

/// What the person picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// The row with this index.
    Row(usize),
    /// The path of a clone that is not in the list.
    Path(PathBuf),
}

/// The name of the tmux session of `repo`. tmux does not allow `.` and
/// `:` in a session name, so they become `_`.
pub fn session_name(repo: &str) -> String {
    repo.replace(['.', ':'], "_")
}

/// The flags of `claude` for the lead: Remote Control, so that the
/// person can answer the lead from the Claude app, the plugin and the
/// MCP config of riff ([`crate::launch`], 01M4BYH7Y3P1JMQR51TWFGVZ39),
/// and the file of its flag settings ([`write_lead_settings`]).
///
/// ```
/// use riff::launch::Given;
///
/// let given = Given { plugin: "/d/riff".into(), mcp: "/s/workers-mcp.json".into() };
/// assert_eq!(
///     riff::start::lead_args(&given, "/s/lead.json".as_ref()),
///     [
///         "--remote-control", "--plugin-dir", "/d/riff", "--strict-mcp-config",
///         "--mcp-config", "/s/workers-mcp.json", "--settings", "/s/lead.json",
///     ],
/// );
/// ```
pub fn lead_args(given: &crate::launch::Given, settings: &Path) -> Vec<String> {
    let mut args = vec!["--remote-control".to_owned()];
    args.extend(crate::launch::args(given));
    args.push("--settings".to_owned());
    args.push(settings.to_string_lossy().into_owned());
    args
}

/// The file of the flag settings of the lead of the tmux session
/// `name` in the folder `dir`.
///
/// ```
/// assert_eq!(
///     riff::start::lead_settings_file("/s".as_ref(), "como/riff"),
///     std::path::Path::new("/s/lead/como/riff.json"),
/// );
/// ```
pub fn lead_settings_file(dir: &Path, name: &str) -> PathBuf {
    dir.join("lead").join(format!("{name}.json"))
}

/// Writes the flag settings of the lead to `file`: the status line and
/// `rules` ([`crate::launch::settings`], 01M4BT33R71HXAVQGHFD4ZFGR5,
/// 01M4BW2SW96JS62ZYQNW6804TV). A file keeps the command of the tmux
/// session short: the rules can be many.
pub fn write_lead_settings(file: &Path, rules: &crate::permissions::Rules) -> Result<()> {
    let json = serde_json::Value::Object(crate::launch::settings(rules)).to_string();
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    }
    std::fs::write(file, json).with_context(|| format!("cannot write {}", file.display()))
}

/// The name of the temp folder of the lead of `repo`.
///
/// ```
/// assert_eq!(riff::start::lead_name("como/riff.x"), "lead-como-riff.x");
/// ```
pub fn lead_name(repo: &str) -> String {
    format!("lead-{}", repo.replace(['/', ':'], "-"))
}

/// The shell command of the lead `name`: `riff workers lead`, which runs
/// `claude` with [`lead_args`] through [`crate::forge::ForgeEnv`]
/// (01M4C4WQVZR49FDGPJMFW22GTM).
///
/// ```
/// use riff::launch::Given;
///
/// let given = Given { plugin: "/d/riff".into(), mcp: "/s/m.json".into() };
/// assert_eq!(
///     riff::start::lead_command(
///         "/bin/riff".as_ref(),
///         "lead-como-riff",
///         "claude".as_ref(),
///         &given,
///         "/s/l.json".as_ref(),
///     ),
///     "'/bin/riff' 'workers' 'lead' '--name' 'lead-como-riff' 'claude' '--remote-control' \
///      '--plugin-dir' '/d/riff' '--strict-mcp-config' '--mcp-config' '/s/m.json' \
///      '--settings' '/s/l.json'",
/// );
/// ```
pub fn lead_command(
    riff: &Path,
    name: &str,
    claude: &Path,
    given: &crate::launch::Given,
    settings: &Path,
) -> String {
    let wrapper = [riff.to_string_lossy().into_owned()]
        .into_iter()
        .chain(["workers", "lead", "--name", name].map(str::to_owned));
    wrapper
        .chain(std::iter::once(claude.to_string_lossy().into_owned()))
        .chain(lead_args(given, settings))
        .map(|a| quote(&a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The paths in the text of [`CLONES`], in order, with no copy and no
/// empty line.
///
/// ```
/// use std::path::PathBuf;
/// assert_eq!(
///     riff::start::known("/a\n\n/b\n/a\n"),
///     [PathBuf::from("/a"), PathBuf::from("/b")],
/// );
/// ```
pub fn known(text: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let path = PathBuf::from(line);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

/// Adds `clone` to the file [`CLONES`] in `dir`, when it is not there.
pub fn remember(dir: &Path, clone: &Path) -> Result<()> {
    let file = dir.join(CLONES);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    if known(&text).iter().any(|p| p == clone) {
        return Ok(());
    }
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .with_context(|| format!("cannot write {}", file.display()))?;
    writeln!(f, "{}", clone.display())?;
    Ok(())
}

/// The clone of `dir`: its main worktree and the repository of its
/// `origin`. `None` outside git, or with no `origin`.
pub fn clone_of(dir: &Path) -> Option<Clone> {
    let main = crate::identity::main_worktree(dir)?;
    let place = crate::identity::place(&main).ok()?;
    match place.repo() {
        Repo::Git { .. } => Some(Clone {
            path: main,
            repo: place.repo_text(),
        }),
        Repo::None => None,
    }
}

/// The clones of `paths`, with no copy of a repository: the first path
/// of a repository wins. A path that is no clone is left out.
pub fn clones(paths: &[PathBuf]) -> Vec<Clone> {
    let mut out: Vec<Clone> = Vec::new();
    for clone in paths.iter().filter_map(|p| clone_of(p)) {
        if !out.iter().any(|c| c.repo == clone.repo) {
            out.push(clone);
        }
    }
    out
}

/// The rows of the picker: each clone with the state of its repository
/// from `pauses` and `who`. With no answer of riff-server, the state is
/// unknown and no session is live.
///
/// ```
/// use riff::start::{rows, Clone};
/// use riff_core::wire::{RiffReply, RiffState};
///
/// let clone = Clone { path: "/src/riff".into(), repo: "como/riff".into() };
/// let running = RiffReply::from(RiffState::Running);
/// let row = &rows(&[clone.clone()], Some((&running, &[])))[0];
/// assert_eq!((row.state, row.live), (Some(RiffState::Running), 0));
/// let row = &rows(&[clone], None)[0];
/// assert_eq!((row.state, row.live), (None, 0));
/// ```
pub fn rows(clones: &[Clone], riff: Option<(&RiffReply, &[SessionInfo])>) -> Vec<Row> {
    clones
        .iter()
        .map(|clone| {
            let (state, live) = match riff {
                None => (None, 0),
                Some((pauses, who)) => {
                    let paused = pauses.riff.is_some()
                        || pauses
                            .repositories
                            .iter()
                            .any(|r| r.repository.to_string() == clone.repo);
                    let state = if paused {
                        RiffState::Paused
                    } else {
                        RiffState::Running
                    };
                    let live = who
                        .iter()
                        .filter(|s| s.live && s.uri.place().repo_text() == clone.repo)
                        .count();
                    (Some(state), live)
                }
            };
            Row {
                clone: clone.clone(),
                state,
                live,
            }
        })
        .collect()
}

/// The text of the picker.
///
/// ```
/// use riff::start::{menu, Clone, Row};
/// use riff_core::wire::RiffState;
///
/// let row = Row {
///     clone: Clone { path: "/src/riff".into(), repo: "como/riff".into() },
///     state: Some(RiffState::Paused),
///     live: 2,
/// };
/// assert_eq!(
///     menu(&[row]),
///     "The riff repositories on this machine:\n  \
///      1  como/riff  paused, 2 live sessions  /src/riff\n\
///      Type a number, or the path of a new clone: ",
/// );
/// assert_eq!(
///     menu(&[]),
///     "riff knows no clone on this machine.\nType the path of a clone: ",
/// );
/// ```
pub fn menu(rows: &[Row]) -> String {
    if rows.is_empty() {
        return "riff knows no clone on this machine.\nType the path of a clone: ".into();
    }
    let width = rows.iter().map(|r| r.clone.repo.len()).max().unwrap_or(0);
    let mut text = String::from("The riff repositories on this machine:\n");
    for (i, row) in rows.iter().enumerate() {
        let state = match row.state {
            Some(RiffState::Running) => "running",
            Some(RiffState::Paused) => "paused",
            None => "no answer from riff-server",
        };
        let live = match row.live {
            1 => "1 live session".to_owned(),
            n => format!("{n} live sessions"),
        };
        text.push_str(&format!(
            "  {}  {:width$}  {state}, {live}  {}\n",
            i + 1,
            row.clone.repo,
            row.clone.path.display(),
        ));
    }
    text.push_str("Type a number, or the path of a new clone: ");
    text
}

/// What `answer` picks from `count` rows. A path is relative to `here`.
///
/// ```
/// use riff::start::{pick, Pick};
/// let here = std::path::Path::new("/home/ada");
/// assert_eq!(pick("2\n", 3, here), Some(Pick::Row(1)));
/// assert_eq!(pick("4", 3, here), None);
/// assert_eq!(pick("0", 3, here), None);
/// assert_eq!(pick("", 3, here), None);
/// assert_eq!(pick("src/riff", 3, here), Some(Pick::Path("/home/ada/src/riff".into())));
/// assert_eq!(pick("/src/x", 0, here), Some(Pick::Path("/src/x".into())));
/// ```
pub fn pick(answer: &str, count: usize, here: &Path) -> Option<Pick> {
    let answer = answer.trim();
    if answer.is_empty() {
        return None;
    }
    if let Ok(n) = answer.parse::<usize>() {
        return (1..=count).contains(&n).then(|| Pick::Row(n - 1));
    }
    Some(Pick::Path(here.join(answer)))
}

/// The tmux server of riff.
#[derive(Debug, Clone)]
pub struct RiffTmux {
    bin: PathBuf,
    config: PathBuf,
}

impl RiffTmux {
    /// The tmux at `bin`, with the config file `config`.
    pub fn new(bin: impl Into<PathBuf>, config: impl Into<PathBuf>) -> Self {
        RiffTmux {
            bin: bin.into(),
            config: config.into(),
        }
    }

    /// `tmux -L riff -f CONFIG`. tmux reads the config only when the
    /// server starts. It gets only the [`server_env`] of this process,
    /// so the server and each of its panes hold no credential of the
    /// person (01M4C4WW15HGA1VEDFRBEMZAW7).
    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.bin);
        cmd.args(["-L", SOCKET, "-f"]).arg(&self.config);
        cmd.env_clear().envs(server_env(std::env::vars_os()));
        cmd
    }

    /// True when the server runs a session with the name `name`.
    pub fn has_session(&self, name: &str) -> Result<bool> {
        let out = self
            .command()
            .args(["has-session", "-t", &format!("={name}")])
            .output()
            .with_context(|| format!("cannot run {}", self.bin.display()))?;
        Ok(out.status.success())
    }

    /// Starts the session `name` in `dir`, with `command` in its first
    /// pane and each of `env` in its environment. It does not attach.
    pub fn new_session(
        &self,
        name: &str,
        dir: &Path,
        env: &[(String, String)],
        command: &str,
    ) -> Result<()> {
        let mut cmd = self.command();
        cmd.args(["new-session", "-d", "-s", name, "-c"]).arg(dir);
        for (k, v) in env {
            cmd.arg("-e").arg(format!("{k}={v}"));
        }
        cmd.arg(command);
        let out = cmd
            .output()
            .with_context(|| format!("cannot run {}", self.bin.display()))?;
        if !out.status.success() {
            bail!(
                "tmux new-session failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }

    /// The command that shows the session `name` to the person:
    /// `switch-client` in a pane of the riff server, else
    /// `attach-session`.
    pub fn attach(&self, name: &str, inside: bool) -> Command {
        let mut cmd = self.command();
        let verb = if inside {
            "switch-client"
        } else {
            "attach-session"
        };
        cmd.args([verb, "-t", &format!("={name}")]);
        cmd
    }
}

/// The variables of `parent` that the tmux server of riff gets: the kept
/// variables of a session ([`crate::profile::kept`]), with no `TMUX` and
/// no `TMUX_PANE`, so a pane of another tmux server does not reach that
/// server (01M4C4WW15HGA1VEDFRBEMZAW7). It also gets the
/// [`crate::profile::WRAPPER_VARS`]: the wrapper of each session runs
/// in a pane, outside the sandbox, and reads the keyring
/// (RID_NO_KEYRING).
///
/// ```
/// use std::ffi::OsString;
/// let parent = [
///     ("PATH", "/bin"),
///     ("GH_TOKEN", "ghp_x"),
///     ("TMUX", "/tmp/t,1,0"),
///     ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
/// ]
/// .map(|(k, v)| (OsString::from(k), OsString::from(v)));
/// let names: Vec<OsString> = riff::start::server_env(parent).into_iter().map(|(k, _)| k).collect();
/// assert_eq!(names, ["PATH", "DBUS_SESSION_BUS_ADDRESS"]);
/// ```
pub fn server_env(
    parent: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    parent
        .into_iter()
        .filter(|(name, _)| {
            name.to_str().is_some_and(|name| {
                (crate::profile::kept(name) || crate::profile::WRAPPER_VARS.contains(&name))
                    && !["TMUX", "TMUX_PANE"].contains(&name)
            })
        })
        .collect()
}

/// True when `tmux`, the value of `TMUX`, is a pane of the riff server:
/// its socket is named [`SOCKET`].
///
/// ```
/// use riff::start::inside_riff;
/// assert!(inside_riff(Some("/tmp/tmux-1000/riff,42,0".as_ref())));
/// assert!(!inside_riff(Some("/tmp/tmux-1000/default,42,0".as_ref())));
/// assert!(!inside_riff(None));
/// ```
pub fn inside_riff(tmux: Option<&OsStr>) -> bool {
    let Some(tmux) = tmux.and_then(OsStr::to_str) else {
        return false;
    };
    let socket = tmux.split(',').next().unwrap_or_default();
    Path::new(socket).file_name() == Some(OsStr::new(SOCKET))
}

/// What [`choose`] gives: the clone to start.
pub fn choose(
    rows: &[Row],
    here: &Path,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<Clone> {
    write!(output, "{}", menu(rows))?;
    output.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    match pick(&answer, rows.len(), here) {
        Some(Pick::Row(i)) => Ok(rows[i].clone.clone()),
        Some(Pick::Path(path)) => clone_of(&path).with_context(|| {
            format!(
                "{} is not a clone of a repository with an origin",
                path.display()
            )
        }),
        None => bail!("you picked no repository: riff started nothing"),
    }
}
