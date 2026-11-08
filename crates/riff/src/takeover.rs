//! `riff takeover NAME`: a person takes over a worker of the riff daemon
//! (#720, 01M4Z60B83ZH10HYKW7X1C0SSS, 01M501PA6W0NCG9JEK017F9304,
//! 01M6VD1NSHTJQGR31VZ4689S8J).
//!
//! # Design
//!
//! A worker runs `claude -p` in stream-json, with no terminal. A takeover
//! stops its turn at once, and opens `claude --resume ID --model MODEL`
//! with the terminal of the person in the same container
//! (01M51KJ1R9K7A7RW8T8XH8VW43). MODEL is the model of the session: the
//! daemon keeps it in its record, and says it with the session ID
//! (01M6VD1NSHTJQGR31VZ4689S8J). With `--at-end`, it waits for the end
//! of the turn. When that `claude` ends, riff writes the reset of the
//! modes of the terminal ([`crate::runtime::reset_terminal`],
//! 01M6XVBRQMBMK68K8GCDRZ5GJ5), and the worker goes on with its item.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as riff takeover NAME
//!     participant D as riff daemon
//!     participant C as container NAME
//!     P->>D: takeover NAME
//!     D->>C: hold: claude -p stops its turn, or ends after it with --at-end
//!     D-->>P: held: the session ID and the model
//!     P->>C: podman exec -it NAME riff relay --terminal -- claude --resume ID --model MODEL
//!     Note over P,C: the person works, then types /exit
//!     Note over P: the reset of the modes of the terminal
//!     P->>D: go on
//!     D->>C: go on: claude -p --resume ID, with one new input
//!     D-->>P: done
//! ```
//!
//! The daemon keeps the takeover (see "A takeover" in
//! [`crate::daemon`]). When `riff takeover` ends before the terminal,
//! for example at Ctrl-C while the turn stops, the daemon gives the
//! session back when `claude` ends. When it ends during the
//! terminal, the daemon gives the session back at once: the person runs
//! `riff takeover NAME` again only when the session still holds.

use std::io::Write;
use std::path::Path;

use anyhow::{Result, bail};

use crate::daemon::{self, Ask, RELAY_BIN, Say};
use crate::runtime::Runtime;
use crate::text;

/// The command of the terminal of a takeover of the session `session`:
/// the relay adds the riff tools, and runs `claude --resume` with the
/// terminal, on the model of the session (01M6VD1NSHTJQGR31VZ4689S8J).
///
/// ```
/// let command = riff::takeover::command("s1", Some("opus"));
/// assert_eq!(
///     command,
///     ["/opt/riff/riff", "relay", "--terminal", "--", "claude", "--resume", "s1",
///      "--model", "opus", "--permission-mode", "auto"]
/// );
/// // A session with no --model.
/// assert!(!riff::takeover::command("s1", None).contains(&"--model".to_owned()));
/// ```
pub fn command(session: &str, model: Option<&str>) -> Vec<String> {
    let mut command: Vec<String> = [
        RELAY_BIN,
        "relay",
        "--terminal",
        "--",
        "claude",
        "--resume",
        session,
    ]
    .map(String::from)
    .to_vec();
    if let Some(model) = model {
        command.extend(["--model".to_owned(), model.to_owned()]);
    }
    command.extend(["--permission-mode".to_owned(), "auto".to_owned()]);
    command
}

/// Takes over the session in the container `name` of the daemon in the
/// folder `dir`, with the terminal of `runtime`. It interrupts the turn
/// of the session, or with `at_end` waits for its end. `out` is the
/// terminal of this process: it gets each line for the person. After
/// the terminal of the takeover, also a failed one, it gets the reset of
/// the modes of the terminal ([`crate::runtime::reset_terminal`],
/// 01M6XVBRQMBMK68K8GCDRZ5GJ5). It gives the exit code: 0 when the
/// session goes on after the takeover, 1 when it ended before.
pub async fn takeover(
    dir: &Path,
    runtime: &impl Runtime,
    name: &str,
    at_end: bool,
    out: &mut (impl Write + Send),
) -> Result<i32> {
    let mut client = daemon::Client::connect(dir).await?;
    client
        .ask(Ask::Takeover {
            name: name.into(),
            at_end,
        })
        .await?;
    let mut back = false;
    loop {
        match client.next().await? {
            Some(Say::Line { text }) => writeln!(out, "{text}")?,
            Some(Say::Held { session, model }) if !back => {
                writeln!(out, "{}", text::takeover_held(name))?;
                out.flush()?;
                let exit = runtime
                    .exec(name, &command(&session, model.as_deref()))
                    .await;
                let reset = crate::runtime::reset_terminal(out);
                back = true;
                client.go_on().await?;
                exit?;
                reset?;
            }
            Some(Say::Done) => {
                writeln!(out, "{}", text::takeover_back(name))?;
                return Ok(0);
            }
            Some(Say::Ended(_)) => {
                writeln!(out, "{}", text::takeover_session_ended(name))?;
                return Ok(1);
            }
            Some(Say::Failed { error }) => bail!("{error}"),
            Some(_) => {}
            None => bail!("{}", text::DAEMON_GONE),
        }
    }
}

/// `riff takeover [--at-end] NAME` on this machine: the daemon of
/// [`daemon::dir`] and rootless podman.
pub async fn here(name: &str, at_end: bool) -> Result<i32> {
    #[cfg(target_os = "linux")]
    {
        takeover(
            &daemon::dir()?,
            &crate::podman::Podman,
            name,
            at_end,
            &mut std::io::stdout(),
        )
        .await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (name, at_end);
        bail!("{}", text::RUN_LINUX_ONLY)
    }
}
