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
        }
    }

    /// A worker: `claude "Join the riff."` in the main worktree, with
    /// `RIFF_WORKER=1` and no Remote Control.
    ///
    /// ```
    /// use riff::terminal::Program;
    /// let worker = Program::worker("claude".as_ref(), "/src/riff".as_ref(), "http://h:7878");
    /// assert_eq!(worker.command, "'claude' 'Join the riff.'");
    /// assert!(worker.env.contains(&("RIFF_WORKER".into(), "1".into())));
    /// assert!(!worker.command.contains("remote-control"));
    /// ```
    pub fn worker(claude: &Path, main: &Path, server: &str) -> Self {
        Program {
            dir: main.to_owned(),
            env: vec![
                ("RIFF_SERVER".into(), server.into()),
                ("RIFF_WORKER".into(), "1".into()),
            ],
            command: format!("{} {}", quote(&claude.to_string_lossy()), quote(JOIN)),
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
    /// opens the window first when it is missing. Returns the name of
    /// the window.
    fn workers(&self, programs: &[Program]) -> Result<String>;
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

    fn run(&self, args: &[&str]) -> Result<String> {
        let out = Command::new(&self.bin)
            .args(args)
            .output()
            .with_context(|| format!("cannot run {}", self.bin.display()))?;
        if !out.status.success() {
            bail!(
                "tmux {} failed: {}",
                args.first().unwrap_or(&""),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
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
                    let id = self.open(
                        &["new-window", "-a", "-t", &here, "-n", WORKERS_WINDOW],
                        "#{window_id}",
                        program,
                    )?;
                    self.run(&["set-option", "-w", "-t", &id, "@riff", WORKERS])?;
                    window = Some(id);
                }
                Some(id) => {
                    self.open(&["split-window", "-t", id], "#{pane_id}", program)?;
                    self.run(&["select-layout", "-t", id, "tiled"])?;
                }
            }
        }
        Ok(WORKERS_WINDOW.to_owned())
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
