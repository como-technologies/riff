//! The terminal of the lead and its workers.
//!
//! # Design
//!
//! A person runs the lead in a terminal. When that terminal is tmux,
//! riff lays out two windows (01M3JD390F49HZSKEJ3VACX0ZA,
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
//!     P->>T: tmux new -s riff
//!     P->>L: claude --remote-control
//!     L->>M: start
//!     M->>S: register, who
//!     S-->>M: this session is the lead
//!     M->>T: list-panes: a pane with @riff=tail?
//!     M->>T: no: split-window "riff tail", mark it
//!     L->>T: riff workers start 3
//!     T->>T: window riff-workers, 3 panes: claude "Join the riff."
//! ```
//!
//! No message starts a process. Only a local command starts workers.
//!
//! A worker starts with no Remote Control, so the Claude app lists only
//! the lead (01M3JD394YFA3TQRE3E72ZER4Z). Each pane gets the riff-server
//! URL of the session that makes it, so all of them talk to the same
//! riff (01M3JD39BASN1GNJTZXXKBCNZ9).
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
/// The mark of the window of the workers.
pub const WORKERS: &str = "workers";
/// The name of the window of the workers.
pub const WORKERS_WINDOW: &str = "riff-workers";
/// The first prompt of a worker.
pub const JOIN: &str = "Join the riff.";

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

/// A worker pane of this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// A worker: `claude "Join the riff."` in the main worktree, with
    /// `RIFF_WORKER=1`, its riff session ID in `RIFF_SESSION`, and no
    /// Remote Control.
    ///
    /// ```
    /// use riff::terminal::Program;
    /// let worker = Program::worker("claude".as_ref(), "/src/riff".as_ref(), "http://h:7878", "w1");
    /// assert_eq!(worker.command, "'claude' 'Join the riff.'");
    /// assert!(worker.env.contains(&("RIFF_WORKER".into(), "1".into())));
    /// assert!(worker.env.contains(&("RIFF_SESSION".into(), "w1".into())));
    /// assert_eq!(worker.session.as_deref(), Some("w1"));
    /// assert!(!worker.command.contains("remote-control"));
    /// ```
    pub fn worker(claude: &Path, main: &Path, server: &str, session: &str) -> Self {
        Program {
            dir: main.to_owned(),
            env: vec![
                ("RIFF_SERVER".into(), server.into()),
                ("RIFF_WORKER".into(), "1".into()),
                ("RIFF_SESSION".into(), session.into()),
            ],
            command: format!("{} {}", quote(&claude.to_string_lossy()), quote(JOIN)),
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
    /// with the session of its program. Returns the name of the window.
    fn workers(&self, programs: &[Program]) -> Result<String>;

    /// The worker panes of this machine: each pane with a
    /// [`SESSION_MARK`], in each session of the terminal.
    fn worker_panes(&self) -> Result<Vec<WorkerPane>>;

    /// Ends the program in `pane` and closes the pane.
    fn kill(&self, pane: &str) -> Result<()>;
}

/// The tmux backend.
#[derive(Debug, Clone)]
pub struct Tmux {
    bin: PathBuf,
    pane: String,
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
        }
    }

    /// The tmux of this machine, with `tmux` on `PATH`. It needs no
    /// pane: it lists and stops the workers from any terminal.
    pub fn machine() -> Self {
        Self::new("tmux", "")
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        self.try_run(args)?.map_err(|stderr| {
            anyhow::anyhow!("tmux {} failed: {stderr}", args.first().unwrap_or(&""))
        })
    }

    /// Runs tmux. The inner error is the stderr of a failed command.
    fn try_run(&self, args: &[&str]) -> Result<std::result::Result<String, String>> {
        let out = Command::new(&self.bin)
            .args(args)
            .output()
            .with_context(|| format!("cannot run {}", self.bin.display()))?;
        if !out.status.success() {
            return Ok(Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()));
        }
        Ok(Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned()))
    }

    /// Marks `pane` with the session of `program`, if it has one.
    fn mark(&self, pane: &str, program: &Program) -> Result<()> {
        if let Some(session) = &program.session {
            self.run(&["set-option", "-p", "-t", pane, SESSION_MARK, session])?;
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

    fn workers(&self, programs: &[Program]) -> Result<String> {
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
                }
                Some(id) => {
                    let pane = self.open(&["split-window", "-t", id], "#{pane_id}", program)?;
                    self.mark(&pane, program)?;
                    self.run(&["select-layout", "-t", id, "tiled"])?;
                }
            }
        }
        Ok(WORKERS_WINDOW.to_owned())
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
