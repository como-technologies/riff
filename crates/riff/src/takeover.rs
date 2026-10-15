//! `riff takeover NAME`: a person takes over a worker of the riff daemon
//! (#720, 01M4Z60B83ZH10HYKW7X1C0SSS, 01M501PA6W0NCG9JEK017F9304).
//!
//! # Design
//!
//! A worker runs `claude -p` in stream-json, with no terminal. A takeover
//! stops it at the end of its turn, and opens `claude --resume ID` with
//! the terminal of the person in the same container. When that `claude`
//! ends, the worker goes on with its item.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as riff takeover NAME
//!     participant D as riff daemon
//!     participant C as container NAME
//!     P->>D: takeover NAME
//!     D->>C: hold: claude -p ends after its turn
//!     D-->>P: held: the session ID
//!     P->>C: podman exec -it NAME riff relay --terminal -- claude --resume ID
//!     Note over P,C: the person works, then types /exit
//!     P->>D: go on
//!     D->>C: go on: claude -p --resume ID, with one new input
//!     D-->>P: done
//! ```
//!
//! The daemon keeps the takeover (see "A takeover" in
//! [`crate::daemon`]). When `riff takeover` ends before the terminal,
//! for example at Ctrl-C while the turn runs, the daemon gives the
//! session back at the end of the turn. When it ends during the
//! terminal, the daemon gives the session back at once: the person runs
//! `riff takeover NAME` again only when the session still holds.

use std::path::Path;

use anyhow::{Result, bail};

use crate::daemon::{self, Ask, RELAY_BIN, Say};
use crate::runtime::Runtime;
use crate::text;

/// The command of the terminal of a takeover of the session `session`:
/// the relay adds the riff tools, and runs `claude --resume` with the
/// terminal.
///
/// ```
/// let command = riff::takeover::command("s1");
/// assert_eq!(
///     command,
///     ["/opt/riff/riff", "relay", "--terminal", "--", "claude", "--resume", "s1",
///      "--permission-mode", "auto"]
/// );
/// ```
pub fn command(session: &str) -> Vec<String> {
    [
        RELAY_BIN,
        "relay",
        "--terminal",
        "--",
        "claude",
        "--resume",
        session,
        "--permission-mode",
        "auto",
    ]
    .map(String::from)
    .to_vec()
}

/// Takes over the session in the container `name` of the daemon in the
/// folder `dir`, with the terminal of `runtime`. It gives `show` each
/// line for the person. It gives the exit code: 0 when the session goes
/// on after the takeover, 1 when it ended before.
pub async fn takeover(
    dir: &Path,
    runtime: &impl Runtime,
    name: &str,
    mut show: impl FnMut(&str),
) -> Result<i32> {
    let mut client = daemon::Client::connect(dir).await?;
    client.ask(Ask::Takeover { name: name.into() }).await?;
    let mut back = false;
    loop {
        match client.next().await? {
            Some(Say::Line { text }) => show(&text),
            Some(Say::Held { session }) if !back => {
                show(&text::takeover_held(name));
                let exit = runtime.exec(name, &command(&session)).await;
                back = true;
                client.go_on().await?;
                exit?;
            }
            Some(Say::Done) => {
                show(&text::takeover_back(name));
                return Ok(0);
            }
            Some(Say::Ended(_)) => {
                show(&text::takeover_session_ended(name));
                return Ok(1);
            }
            Some(Say::Failed { error }) => bail!("{error}"),
            Some(_) => {}
            None => bail!("{}", text::DAEMON_GONE),
        }
    }
}

/// `riff takeover NAME` on this machine: the daemon of [`daemon::dir`]
/// and rootless podman.
pub async fn here(name: &str) -> Result<i32> {
    #[cfg(target_os = "linux")]
    {
        takeover(&daemon::dir()?, &crate::podman::Podman, name, |line| {
            println!("{line}");
        })
        .await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        bail!("{}", text::RUN_LINUX_ONLY)
    }
}
