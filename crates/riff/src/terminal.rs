//! The terminal of the lead and its workers.
//!
//! # Design
//!
//! `riff` starts the lead in the tmux server of riff (see
//! [`crate::start`]). In tmux, riff lays out two windows (01M3JD390F49HZSKEJ3VACX0ZA,
//! 01M3JD392Q5ANX0FPZ51W7B0E3):
//!
//! - **The lead window.** `riff mcp` of the lead adds a pane with
//!   `riff tail` beside the lead. It marks the pane with the tmux
//!   option `@riff=tail`. When the window has a marked pane, it adds
//!   none. So a restart, a `/clear` or a resume of the lead does not
//!   add a pane.
//! - **The workers window.** `riff workers start N` opens the window
//!   `riff-workers`, marked `@riff=workers`, with one pane for each
//!   worker. A second start adds panes to the same window. Each pane
//!   runs `claude "Join the riff."` in the main worktree. The start hook
//!   then gives the session the start routine.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as person
//!     participant L as lead (claude --remote-control)
//!     participant M as riff mcp of the lead
//!     participant T as tmux
//!     participant S as riff-server
//!     P->>T: riff: tmux -L riff new-session
//!     T->>L: claude --remote-control
//!     L->>M: start
//!     M->>S: register, who
//!     S-->>M: this session is the lead
//!     M->>T: list-panes: a pane with @riff=tail?
//!     M->>T: no: split-window "riff tail", mark it
//!     L->>T: riff workers start 3
//!     T->>T: window riff-workers, 3 panes: claude "Join the riff."
//! ```
//!
//! No message starts a process, except a request of the lead to a
//! workers host on another machine of its user ([`crate::host`]).
//!
//! A worker starts with no Remote Control, so the Claude app lists only
//! the lead (01M3JD394YFA3TQRE3E72ZER4Z). The flag settings
//! [`worker_settings`] outrank the user settings, so a worker has no
//! Remote Control also when the user settings turn on
//! `remoteControlAtStartup` (01M3JV0ZNGKDFMRR9ACT0480V9). They also turn
//! off the recap of Claude Code, because no person reads a worker pane
//! (01M3MN0D429T4Q80DYBE9S9XR7), and each plugin with a language server
//! ([`crate::worker_lsp`], 01M3ZJ1FAF7EJXP9CSET8ZY1K3). The user settings
//! file does not change. The clear of a
//! worker ([`crate::next`]) keeps the same process, so each next item has the same settings.
//!
//! Each pane gets the riff-server URL of the session that makes it, so
//! all of them talk to the same riff (01M3JD39BASN1GNJTZXXKBCNZ9).
//!
//! # The workers of a machine
//!
//! Each worker gets a new riff session ID in `RIFF_SESSION`, and its
//! pane gets the mark `@riff-session` with that ID
//! (01M3JPQT9BA7JVMZPV68FY4MQ6). So riff finds each worker of the
//! machine in each tmux session, and knows its riff session:
//!
//! ```mermaid
//! flowchart LR
//!     P["tmux list-panes -a<br/>panes with @riff-session"] --> L["riff workers<br/>pane, session, claim, status"]
//!     S["riff who"] --> L
//!     P --> K["riff workers stop<br/>kill-pane, then the end call"]
//! ```
//!
//! - `riff workers start N` starts at most the limit of the machine
//!   ([`crate::settings`]) minus the workers that run
//!   (01M3JPQT57PJCRBQYJNDVESS04). See [`room`].
//! - It refuses in a worker, and in an agent session that is not the
//!   lead (01M3JPQT79FE47518Z8DFFQYYG).
//! - `riff workers stop` kills each pane, then sends the end call of
//!   its session. The session leaves `riff who` and frees its claims at
//!   once (01M3JPQTDFW3C7QBSZZ2M831MH).
//!
//! tmux is one backend of [`Terminal`]. A later backend, for example
//! zellij, implements the same trait (01M3JD399ABBWE3DJT5BVXAFH5).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use riff_core::name::SessionUri;

use crate::api::Api;

/// The mark of the pane with `riff tail`.
pub const TAIL: &str = "tail";
/// The pane option that holds the riff session ID of a worker.
pub const SESSION_MARK: &str = "@riff-session";
/// The pane option that holds the main clone of a worker: the broker
/// of a lead acts only on the workers of its own clone
/// ([`crate::door`]).
pub const CLONE_MARK: &str = "@riff-clone";
/// The mark of the window of the workers.
pub const WORKERS: &str = "workers";
/// The name of the window of the workers.
pub const WORKERS_WINDOW: &str = "riff-workers";
/// The first prompt of a worker.
pub const JOIN: &str = "Join the riff.";
/// The flag settings of a worker: no Remote Control, also when the
/// user settings turn it on (01M3JV0ZNGKDFMRR9ACT0480V9), no recap
/// (01M3MN0D429T4Q80DYBE9S9XR7), no suggested prompt
/// (01M4CGFMX16JY8W2P9JJDDTSFC), the riff status line
/// (01M4BYH7Y3P1JMQR51TWFGVZ39), the plugin of an older riff off
/// (01M4CMN13D97R2JKHYGFSAM313), and each plugin of `lsp` off: the
/// plugins with a language server (01M3ZJ1FAF7EJXP9CSET8ZY1K3). A rule
/// denies `riff cloud` (01M4262DY8NN30SC4REYX2G9DV).
///
/// ```
/// use riff::terminal::worker_settings;
/// assert_eq!(
///     worker_settings(&[]),
///     r#"{"remoteControlAtStartup":false,"awaySummaryEnabled":false,"promptSuggestionEnabled":false,"statusLine":{"type":"command","command":"riff statusline"},"permissions":{"deny":["Bash(riff cloud)","Bash(riff cloud *)"]},"enabledPlugins":{"riff@riff":false}}"#,
/// );
/// assert_eq!(
///     worker_settings(&["rust-analyzer-lsp@claude-plugins-official".into()]),
///     r#"{"remoteControlAtStartup":false,"awaySummaryEnabled":false,"promptSuggestionEnabled":false,"statusLine":{"type":"command","command":"riff statusline"},"permissions":{"deny":["Bash(riff cloud)","Bash(riff cloud *)"]},"enabledPlugins":{"riff@riff":false,"rust-analyzer-lsp@claude-plugins-official":false}}"#,
/// );
/// ```
pub fn worker_settings(lsp: &[String]) -> String {
    let off: serde_json::Map<String, serde_json::Value> =
        std::iter::once(crate::old_config::PLUGIN)
            .chain(lsp.iter().map(String::as_str))
            .map(|p| (p.to_owned(), false.into()))
            .collect();
    serde_json::json!({
        "remoteControlAtStartup": false,
        "awaySummaryEnabled": false,
        "promptSuggestionEnabled": false,
        "statusLine": crate::launch::statusline(),
        "permissions": {"deny": ["Bash(riff cloud)", "Bash(riff cloud *)"]},
        "enabledPlugins": off,
    })
    .to_string()
}

/// `args` of `claude` with its flag settings changed by `edit`. It
/// changes the first `--settings` that holds a JSON object, and else
/// adds a new `--settings` at the end.
///
/// ```
/// use riff::terminal::with_settings;
///
/// let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
/// let on = |s: &mut serde_json::Map<String, serde_json::Value>| {
///     s.insert("b".into(), true.into());
/// };
/// assert_eq!(
///     with_settings(&args(&["--settings", "x.json", "--settings", r#"{"a":1}"#]), on),
///     args(&["--settings", "x.json", "--settings", r#"{"a":1,"b":true}"#]),
/// );
/// assert_eq!(
///     with_settings(&args(&["Join the riff."]), on),
///     args(&["Join the riff.", "--settings", r#"{"b":true}"#]),
/// );
/// ```
pub fn with_settings(
    args: &[String],
    edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) -> Vec<String> {
    let mut out = args.to_vec();
    let at = out.windows(2).position(|pair| {
        pair[0] == "--settings"
            && serde_json::from_str::<serde_json::Value>(&pair[1]).is_ok_and(|v| v.is_object())
    });
    match at {
        Some(at) => {
            let mut value: serde_json::Value =
                serde_json::from_str(&out[at + 1]).unwrap_or_else(|_| serde_json::json!({}));
            if let Some(settings) = value.as_object_mut() {
                edit(settings);
            }
            out[at + 1] = value.to_string();
        }
        None => {
            let mut settings = serde_json::Map::new();
            edit(&mut settings);
            out.push("--settings".to_owned());
            out.push(serde_json::Value::Object(settings).to_string());
        }
    }
    out
}

/// A program to run in a new pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// The directory where it starts.
    pub dir: PathBuf,
    /// The variables that it gets.
    pub env: Vec<(String, String)>,
    /// The shell command.
    pub command: String,
    /// The riff session ID of a worker. Its pane gets it as the mark
    /// [`SESSION_MARK`].
    pub session: Option<String>,
}

/// The `claude` of a worker and its flags.
#[derive(Debug, Clone, Copy)]
pub struct Claude<'a> {
    /// The `claude` program.
    pub bin: &'a Path,
    /// The root of the plugin (see [`crate::plugin::root`]).
    pub plugin: &'a Path,
    /// The file with the only MCP servers of the worker (see
    /// [`crate::worker_mcp`]).
    pub mcp: &'a Path,
    /// The flag settings (see [`worker_settings`]).
    pub settings: &'a str,
}

/// A worker pane of this machine.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkerPane {
    /// The tmux pane, for example `%3`.
    pub pane: String,
    /// The riff session ID of the worker.
    pub session: String,
}

/// A new riff session ID for a worker: a random UUID (version 4).
///
/// ```
/// let id = riff::terminal::new_session_id();
/// assert_eq!(id.len(), 36);
/// assert_eq!(id.as_bytes()[14], b'4');
/// assert_ne!(id, riff::terminal::new_session_id());
/// ```
pub fn new_session_id() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("the OS gives random bytes");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// How many workers `riff workers start` starts: at most the `limit`
/// minus the workers that `run` (01M3JPQT57PJCRBQYJNDVESS04).
///
/// ```
/// use riff::terminal::room;
/// assert_eq!(room(2, 0, 0), 0);
/// assert_eq!(room(3, 2, 0), 2);
/// assert_eq!(room(3, 2, 1), 1);
/// assert_eq!(room(1, 2, 5), 0);
/// ```
pub fn room(asked: u16, limit: u16, run: usize) -> u16 {
    let free = usize::from(limit).saturating_sub(run);
    asked.min(u16::try_from(free).unwrap_or(u16::MAX))
}

impl Program {
    /// `riff tail` of the repository thread of `dir`.
    ///
    /// ```
    /// use riff::terminal::Program;
    /// let tail = Program::tail("/bin/riff".as_ref(), "/src/riff".as_ref(), "http://h:7878");
    /// assert_eq!(tail.command, "'/bin/riff' tail");
    /// assert_eq!(tail.env, [("RIFF_SERVER".into(), "http://h:7878".into())]);
    /// ```
    pub fn tail(riff: &Path, dir: &Path, server: &str) -> Self {
        Program {
            dir: dir.to_owned(),
            env: vec![("RIFF_SERVER".into(), server.into())],
            command: format!("{} tail", quote(&riff.to_string_lossy())),
            session: None,
        }
    }

    /// A worker: `claude "Join the riff."` through `riff workers run`
    /// (see [`crate::worker`]) in the main worktree, with
    /// `RIFF_WORKER=1`, `RIFF_ON=1`, its riff session ID in
    /// `RIFF_SESSION`, and the flags of `claude` (see [`Claude`] and
    /// [`crate::launch`]).
    /// With `slice`, the wrapper runs `claude` in a scope of that slice
    /// (see [`crate::limits`]).
    /// `--mcp-config` takes more than one value, so `--settings` comes
    /// after it.
    ///
    /// ```
    /// use riff::terminal::{Claude, Program};
    /// let claude = Claude {
    ///     bin: "claude".as_ref(),
    ///     plugin: "/d/riff".as_ref(),
    ///     mcp: "/run/riff/workers-mcp.json".as_ref(),
    ///     settings: r#"{"awaySummaryEnabled":false}"#,
    /// };
    /// let worker = Program::worker(
    ///     "/bin/riff".as_ref(), &claude, "/src/riff".as_ref(), "http://h:7878", "w1",
    ///     Some("riff-workers.slice"),
    /// );
    /// assert_eq!(
    ///     worker.command,
    ///     r#"'/bin/riff' workers run 'claude' '--plugin-dir' '/d/riff' '--strict-mcp-config' '--mcp-config' '/run/riff/workers-mcp.json' '--settings' '{"awaySummaryEnabled":false}' 'Join the riff.'"#,
    /// );
    /// assert!(worker.env.contains(&("RIFF_WORKER".into(), "1".into())));
    /// assert!(worker.env.contains(&("RIFF_ON".into(), "1".into())));
    /// assert!(worker.env.contains(&("RIFF_SESSION".into(), "w1".into())));
    /// assert!(worker.env.contains(&("RIFF_WORKER_SLICE".into(), "riff-workers.slice".into())));
    /// assert_eq!(worker.session.as_deref(), Some("w1"));
    /// assert!(!worker.command.contains("remote-control"));
    /// ```
    pub fn worker(
        riff: &Path,
        claude: &Claude,
        main: &Path,
        server: &str,
        session: &str,
        slice: Option<&str>,
    ) -> Self {
        let mut env = vec![
            ("RIFF_SERVER".into(), server.into()),
            ("RIFF_WORKER".into(), "1".into()),
            (crate::launch::ON.0.into(), crate::launch::ON.1.into()),
            ("RIFF_SESSION".into(), session.into()),
        ];
        if let Some(slice) = slice {
            env.push((crate::limits::SLICE_VAR.into(), slice.into()));
        }
        Program {
            dir: main.to_owned(),
            env,
            command: format!(
                "{} workers run {} {} {} {} {}",
                quote(&riff.to_string_lossy()),
                quote(&claude.bin.to_string_lossy()),
                crate::launch::args(&crate::launch::Given {
                    plugin: claude.plugin.to_owned(),
                    mcp: claude.mcp.to_owned(),
                })
                .iter()
                .map(|a| quote(a))
                .collect::<Vec<_>>()
                .join(" "),
                quote("--settings"),
                quote(claude.settings),
                quote(JOIN)
            ),
            session: Some(session.into()),
        }
    }
}

/// A terminal that can lay out panes.
pub trait Terminal {
    /// Adds a pane with `program` beside the pane of the session, and
    /// marks it with `mark`. It adds none when the window has a pane
    /// with that mark. True when it added one.
    fn beside(&self, mark: &str, program: &Program) -> Result<bool>;

    /// Adds one pane for each program to the window of the workers. It
    /// opens the window first when it is missing. It marks each pane
    /// with the session of its program. Returns the name of the window,
    /// and each new pane with the session of its program.
    fn workers(&self, programs: &[Program]) -> Result<(String, Vec<WorkerPane>)>;

    /// The worker panes of this machine: each pane with a
    /// [`SESSION_MARK`], in each session of the terminal.
    fn worker_panes(&self) -> Result<Vec<WorkerPane>>;

    /// Ends the program in `pane` and closes the pane.
    fn kill(&self, pane: &str) -> Result<()>;

    /// The text that `pane` shows now, with no colors.
    fn screen(&self, pane: &str) -> Result<String>;

    /// Types `text` into `pane`, then Enter.
    fn type_line(&self, pane: &str, text: &str) -> Result<()>;
}

/// The tmux backend.
#[derive(Debug, Clone)]
pub struct Tmux {
    bin: PathBuf,
    pane: String,
    /// The socket name of the server (`-L`), or `None` for the server
    /// of `TMUX`.
    socket: Option<String>,
}

impl Tmux {
    /// The tmux of this process: `None` outside tmux. tmux sets `TMUX`
    /// and `TMUX_PANE` in each pane.
    pub fn from_env() -> Option<Self> {
        Self::from_vars(std::env::var_os("TMUX"), std::env::var("TMUX_PANE").ok())
    }

    /// The tmux of these variables, with `tmux` on `PATH`.
    ///
    /// ```
    /// use riff::terminal::Tmux;
    /// assert!(Tmux::from_vars(Some("/tmp/tmux-1000/default,1,0".into()), Some("%3".into())).is_some());
    /// assert!(Tmux::from_vars(None, Some("%3".into())).is_none());
    /// assert!(Tmux::from_vars(Some("".into()), Some("%3".into())).is_none());
    /// assert!(Tmux::from_vars(Some("/tmp/tmux-1000/default,1,0".into()), None).is_none());
    /// ```
    pub fn from_vars(tmux: Option<OsString>, pane: Option<String>) -> Option<Self> {
        let pane = pane.filter(|p| !p.is_empty())?;
        tmux.filter(|t| !t.is_empty())?;
        Some(Self::new("tmux", &pane))
    }

    /// The tmux at `bin`, for the session in `pane`.
    pub fn new(bin: impl Into<PathBuf>, pane: &str) -> Self {
        Tmux {
            bin: bin.into(),
            pane: pane.to_owned(),
            socket: None,
        }
    }

    /// The tmux of this machine, with `tmux` on `PATH`. It needs no
    /// pane: it lists and stops the workers from any terminal. Outside
    /// tmux, it is the server of riff (01M4BSSX66A2NNVQK48KQH8BEZ, see
    /// [`crate::start`]).
    pub fn machine() -> Self {
        Self::machine_with("tmux", std::env::var_os("TMUX"))
    }

    /// [`Tmux::machine`] at `bin`, with `tmux`, the value of `TMUX`.
    ///
    /// ```
    /// use riff::terminal::Tmux;
    /// assert_eq!(Tmux::machine_with("tmux", None).socket(), Some("riff"));
    /// assert_eq!(Tmux::machine_with("tmux", Some("".into())).socket(), Some("riff"));
    /// let pane = Some("/tmp/tmux-1000/default,1,0".into());
    /// assert_eq!(Tmux::machine_with("tmux", pane).socket(), None);
    /// ```
    pub fn machine_with(bin: impl Into<PathBuf>, tmux: Option<OsString>) -> Self {
        let mut machine = Self::new(bin, "");
        if tmux.is_none_or(|t| t.is_empty()) {
            machine.socket = Some(crate::start::SOCKET.into());
        }
        machine
    }

    /// The pane of the session, or an empty text for the tmux of the
    /// machine.
    pub fn pane(&self) -> &str {
        &self.pane
    }

    /// The socket name of the server, when it is not the server of
    /// `TMUX`.
    pub fn socket(&self) -> Option<&str> {
        self.socket.as_deref()
    }

    /// The process ID of the program of `pane`, or `None` when tmux
    /// does not give one. [`crate::reap`] finds the systemd scope of a
    /// worker pane from it.
    pub fn pane_pid(&self, pane: &str) -> Option<u32> {
        self.try_run(&["display-message", "-p", "-t", pane, "#{pane_pid}"])
            .ok()?
            .ok()?
            .parse()
            .ok()
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        self.try_run(args)?.map_err(|stderr| {
            anyhow::anyhow!("tmux {} failed: {stderr}", args.first().unwrap_or(&""))
        })
    }

    /// Runs tmux. The inner error is the stderr of a failed command.
    fn try_run(&self, args: &[&str]) -> Result<std::result::Result<String, String>> {
        let mut cmd = Command::new(&self.bin);
        if let Some(socket) = &self.socket {
            cmd.args(["-L", socket]);
        }
        let out = cmd
            .args(args)
            .output()
            .with_context(|| format!("cannot run {}", self.bin.display()))?;
        if !out.status.success() {
            return Ok(Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()));
        }
        Ok(Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned()))
    }

    /// The main clone of the worker in `pane` ([`CLONE_MARK`]), or
    /// `None` when the pane has no such mark.
    pub fn pane_clone(&self, pane: &str) -> Option<PathBuf> {
        let format = format!("#{{{CLONE_MARK}}}");
        let clone = self
            .try_run(&["display-message", "-p", "-t", pane, &format])
            .ok()?
            .ok()?;
        (!clone.is_empty()).then(|| PathBuf::from(clone))
    }

    /// Marks `pane` with the session of `program` and its folder, the
    /// main clone, if it has a session.
    fn mark(&self, pane: &str, program: &Program) -> Result<()> {
        if let Some(session) = &program.session {
            self.run(&["set-option", "-p", "-t", pane, SESSION_MARK, session])?;
            let clone = program.dir.to_string_lossy();
            self.run(&["set-option", "-p", "-t", pane, CLONE_MARK, &clone])?;
        }
        Ok(())
    }

    /// Runs `command` (`split-window` or `new-window`) with the place,
    /// the variables and the program. Returns what `-F` prints.
    fn open(&self, command: &[&str], format: &str, program: &Program) -> Result<String> {
        let dir = program.dir.to_string_lossy();
        let env: Vec<String> = program
            .env
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        let mut args: Vec<&str> = command.to_vec();
        args.extend(["-d", "-c", &dir, "-P", "-F", format]);
        for e in &env {
            args.extend(["-e", e]);
        }
        args.push(&program.command);
        self.run(&args)
    }
}

impl Terminal for Tmux {
    fn beside(&self, mark: &str, program: &Program) -> Result<bool> {
        let marks = self.run(&["list-panes", "-t", &self.pane, "-F", "#{@riff}"])?;
        if marks.lines().any(|m| m == mark) {
            return Ok(false);
        }
        let pane = self.open(
            &["split-window", "-h", "-t", &self.pane],
            "#{pane_id}",
            program,
        )?;
        self.run(&["set-option", "-p", "-t", &pane, "@riff", mark])?;
        Ok(true)
    }

    fn workers(&self, programs: &[Program]) -> Result<(String, Vec<WorkerPane>)> {
        let mut opened = Vec::new();
        let windows = self.run(&[
            "list-windows",
            "-t",
            &self.pane,
            "-F",
            "#{window_id} #{@riff}",
        ])?;
        let mut window = windows
            .lines()
            .filter_map(|l| l.split_once(' '))
            .find(|(_, mark)| *mark == WORKERS)
            .map(|(id, _)| id.to_owned());
        for program in programs {
            match &window {
                None => {
                    // `new-window -t` takes a window, not a pane.
                    let here =
                        self.run(&["display-message", "-p", "-t", &self.pane, "#{window_id}"])?;
                    let ids = self.open(
                        &["new-window", "-a", "-t", &here, "-n", WORKERS_WINDOW],
                        "#{window_id} #{pane_id}",
                        program,
                    )?;
                    let (id, pane) = ids.split_once(' ').unwrap_or((&ids, ""));
                    self.run(&["set-option", "-w", "-t", id, "@riff", WORKERS])?;
                    self.mark(pane, program)?;
                    window = Some(id.to_owned());
                    opened.push((pane.to_owned(), program));
                }
                Some(id) => {
                    let pane = self.open(&["split-window", "-t", id], "#{pane_id}", program)?;
                    self.mark(&pane, program)?;
                    self.run(&["select-layout", "-t", id, "tiled"])?;
                    opened.push((pane, program));
                }
            }
        }
        let panes = opened
            .into_iter()
            .filter_map(|(pane, program)| {
                let session = program.session.clone()?;
                Some(WorkerPane { pane, session })
            })
            .collect();
        Ok((WORKERS_WINDOW.to_owned(), panes))
    }

    fn worker_panes(&self) -> Result<Vec<WorkerPane>> {
        let format = format!("#{{pane_id}} #{{{SESSION_MARK}}}");
        let list = match self.try_run(&["list-panes", "-a", "-F", &format])? {
            Ok(list) => list,
            // No tmux server runs: no worker runs.
            Err(e) if e.contains("no server running") || e.contains("error connecting") => {
                String::new()
            }
            Err(e) => bail!("tmux list-panes failed: {e}"),
        };
        Ok(list
            .lines()
            .filter_map(|l| l.split_once(' '))
            .filter(|(_, session)| !session.trim().is_empty())
            .map(|(pane, session)| WorkerPane {
                pane: pane.to_owned(),
                session: session.trim().to_owned(),
            })
            .collect())
    }

    fn kill(&self, pane: &str) -> Result<()> {
        self.run(&["kill-pane", "-t", pane]).map(|_| ())
    }

    fn screen(&self, pane: &str) -> Result<String> {
        self.run(&["capture-pane", "-p", "-t", pane])
    }

    fn type_line(&self, pane: &str, text: &str) -> Result<()> {
        self.run(&["send-keys", "-t", pane, "-l", text])?;
        self.run(&["send-keys", "-t", pane, "Enter"]).map(|_| ())
    }
}

/// Adds the `riff tail` pane beside the lead (01M3JD390F49HZSKEJ3VACX0ZA).
/// It does nothing when `me` is not the lead. True when it added a pane.
pub async fn tail_beside_lead(
    api: &Api,
    me: &SessionUri,
    terminal: &impl Terminal,
    program: &Program,
) -> Result<bool> {
    let id = me.who().session();
    let lead = api
        .who(me, false)
        .await?
        .iter()
        .any(|s| s.uri.who().session() == id && s.uri.lead());
    if !lead {
        return Ok(false);
    }
    terminal.beside(TAIL, program)
}

/// `text` in single quotes for `sh`.
///
/// ```
/// use riff::terminal::quote;
/// assert_eq!(quote("a b"), "'a b'");
/// assert_eq!(quote("it's"), r"'it'\''s'");
/// ```
pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}
