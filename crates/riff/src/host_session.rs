//! `riff host-session` (#2157): a session on the host that only a person
//! starts, for the lead of the repository. It is the way out when the
//! sessions of riff in containers need a step on the host: the journal of
//! the daemon, `podman`, the keyring, or a riff command of a person.
//!
//! # Design
//!
//! `riff host-session` runs `claude` in the terminal of the person, in the
//! clone of the project, as the person. It adds only the riff tools and
//! the wakes. The session ID is new, and riff-server shows the session
//! with the role `host session`.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as the person
//!     participant H as riff host-session
//!     participant S as riff-server
//!     participant C as claude on the host
//!     participant L as the lead in its container
//!     P->>H: riff host-session, in the clone
//!     H->>S: register the session, keep it alive as a host session
//!     H->>L: tell: the session ID started
//!     H->>C: claude --session-id ID PROMPT --mcp-config (riff relay mcp)
//!     C->>S: riff relay watch --follow, in the Monitor tool
//!     L->>S: tell ID: a step on the host
//!     S-->>C: the wake
//!     C->>L: tell lead: the result
//!     P->>C: /exit
//!     H->>L: tell: the session ID ended
//!     H->>S: end the session
//! ```
//!
//! | Part | Value | Requirement |
//! |---|---|---|
//! | The caller | only a person in a terminal on the host: [`crate::cycle::Caller::person_step`] refuses each agent session, a container session and the lead, before each call. riff asks no role: the session acts with the rights of the person | 01M7GTEYVRA3549J3PCA0DG22R |
//! | The session | a new session ID ([`identity::new_session_id`]) of the person, on this host, in the repository of the folder. `riff host-session` sends its keep-alive each [`ALIVE_EVERY`] ([`Api::alive_in_host_session`]): riff-server shows it live, with the role `host session` in `riff who` | 01M7GTF5GZYET0F5R0KTGT59C1 |
//! | The command | `claude --session-id ID PROMPT --mcp-config CONFIG` ([`command`]). riff gives no model, no effort, no permission mode and no settings: the Claude sign-in, the settings, the model, the effort and the permission mode of the person hold | 01M7GTEYVRA3549J3PCA0DG22R |
//! | The riff tools | the MCP server `riff` runs `riff relay mcp` with [`VAR`] ([`mcp_config`]): it serves the tools that only talk ([`crate::tools::CONTAINER_TOOLS`]) as the session, with the sign-in of the person on this host ([`mcp`]). The arguments hold no token | 01M7GTF5EE7KN7PZX0YYP537C8 |
//! | The wakes | `riff relay watch --follow` in the Monitor tool, with [`VAR`] ([`follow`]): the first prompt says so ([`text::host_session_start`]) | 01M7GTF5EE7KN7PZX0YYP537C8 |
//! | The riff commands | `RIFF_SESSION` is the session ID, and `RIFF_SERVER` the riff-server of `riff host-session`, so each `riff` command of the session acts as it there. The session has no `RIFF_SOCKET`: a command acts on the host, as the person, and a step of a person refuses, as in each agent session | 01M7GTF5EE7KN7PZX0YYP537C8 |
//! | The lead | riff tells the lead of the repository the session ID at the start and at the end ([`text::host_session_started`], [`text::host_session_ended`]) | 01M7GTEYVRA3549J3PCA0DG22R |
//! | The end | at the end of `claude`, riff ends the session on riff-server: it is gone at once. riff exits with the exit code of `claude` | 01M7GTEYVRA3549J3PCA0DG22R |

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use riff_client::api::{self, Api, LEAD};
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::ALIVE_EVERY;
use serde_json::{Value, json};

use crate::{identity, text};

/// The variable that marks the processes of a session of `riff
/// host-session`: its session ID. `riff relay mcp` and `riff relay
/// watch` serve that session with no `RIFF_SOCKET`.
pub const VAR: &str = "RIFF_HOST_SESSION";

/// The program of the session.
pub const CLAUDE: &str = "claude";

/// The wait before a new watch of riff-server, after a fault.
const WAKE_RETRY: Duration = Duration::from_secs(1);

/// The session ID of the session of `riff host-session` of this process:
/// [`VAR`], when it is set.
pub fn session_here() -> Option<String> {
    std::env::var(VAR).ok().filter(|id| !id.is_empty())
}

/// The MCP config of the session `session`: the server
/// [`crate::session_relay::MCP_SERVER`] runs `bin relay mcp`, with
/// [`VAR`]. It holds no token: the server signs in as the person on
/// this host.
///
/// ```
/// let config = riff::host_session::mcp_config("/usr/local/bin/riff".as_ref(), "s1");
/// let riff = &config["mcpServers"]["riff"];
/// assert_eq!(riff["command"], "/usr/local/bin/riff");
/// assert_eq!(riff["args"], serde_json::json!(["relay", "mcp"]));
/// assert_eq!(riff["env"][riff::host_session::VAR], "s1");
/// ```
pub fn mcp_config(bin: &Path, session: &str) -> Value {
    json!({
        "mcpServers": {
            crate::session_relay::MCP_SERVER: {
                "type": "stdio",
                "command": bin.to_string_lossy(),
                "args": ["relay", "mcp"],
                "env": { VAR: session },
            }
        }
    })
}

/// The command of the session `session`, with the first prompt
/// `prompt`: `claude --session-id ID PROMPT --mcp-config CONFIG`. The
/// option `--mcp-config` goes last, because it takes more than one
/// value. No model, no permission mode and no settings: those of the
/// person hold.
///
/// ```
/// let command = riff::host_session::command("/usr/local/bin/riff".as_ref(), "s1", "Start.");
/// assert_eq!(command[..5], ["claude", "--session-id", "s1", "Start.", "--mcp-config"]);
/// assert!(command[5].contains(r#""args":["relay","mcp"]"#));
/// assert_eq!(command.len(), 6);
/// ```
pub fn command(bin: &Path, session: &str, prompt: &str) -> Vec<String> {
    vec![
        CLAUDE.into(),
        "--session-id".into(),
        session.into(),
        prompt.into(),
        "--mcp-config".into(),
        mcp_config(bin, session).to_string(),
    ]
}

/// The URI of the session `id` of `riff host-session` in the folder of
/// this process, at `server`.
fn me_here(id: &str, server: &str) -> Result<SessionUri> {
    let place = identity::place(&identity::working_dir()?)?;
    identity::agent(&place, id, server)
}

/// `riff relay mcp` in the session `id` of `riff host-session`: the riff
/// tools that only talk, on stdin and stdout, as the session, with the
/// sign-in of the person on this host. It ends when `claude` closes
/// stdin.
pub async fn mcp(server: &str, id: &str) -> Result<()> {
    use rmcp::ServiceExt;
    let me = me_here(id, server)?;
    let api = Api::new(server).signed_in(Some(id))?;
    let tools = crate::tools::Tools::new(api, me).in_host_session();
    let service = tools.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// `riff relay watch` in the session `id` of `riff host-session`: gives
/// the line of each wake of the session on riff-server to `emit`, as the
/// riff daemon does for a session in a container
/// ([`crate::session_relay::follow`]). With no `follow`, it ends after
/// the first wake. Each wake comes once.
pub async fn follow(
    server: &str,
    id: &str,
    follow: bool,
    mut emit: impl FnMut(&str),
) -> Result<()> {
    use futures::StreamExt;
    let me = me_here(id, server)?;
    let api = Api::new(server).signed_in(Some(id))?;
    let wakes = api::follow(|| api.watch(&me), WAKE_RETRY);
    tokio::pin!(wakes);
    let mut seen = BTreeSet::new();
    while let Some(wake) = wakes.next().await {
        match wake {
            Ok(wake) if seen.insert((wake.thread.to_string(), wake.seq)) => {
                emit(&text::wake_line(&wake));
                if !follow {
                    return Ok(());
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("riff relay watch: {e:#}"),
        }
    }
    bail!("riff relay watch: the watch of riff-server ended")
}

/// The lead of the repository thread `repo` in `sessions`, when one is
/// live.
///
/// ```
/// use riff_core::wire::SessionInfo;
///
/// let info = |uri: &str| -> SessionInfo {
///     serde_json::from_value(serde_json::json!({ "uri": uri, "live": true })).unwrap()
/// };
/// let list = [
///     info("riff://mike@thelio/acme/other?session=o1&lead=true"),
///     info("riff://mike@thelio/acme/app?session=w1"),
///     info("riff://mike@thelio/acme/app?session=l1&lead=true"),
/// ];
/// let repo = "acme/app".parse().unwrap();
/// let lead = riff::host_session::lead_in(&list, &repo).unwrap();
/// assert_eq!(lead.who().session(), Some("l1"));
/// assert!(riff::host_session::lead_in(&list[..2], &repo).is_none());
/// ```
pub fn lead_in(sessions: &[riff_core::wire::SessionInfo], repo: &ThreadName) -> Option<SessionUri> {
    sessions
        .iter()
        .find(|s| s.live && s.uri.lead() && s.uri.default_thread().as_ref() == Some(repo))
        .map(|s| s.uri.clone())
}

/// Tells the lead of the repository of `me` the text `body`, as `me`. A
/// fault is one line: the session goes on.
async fn tell_lead(api: &Api, me: &SessionUri, body: &str) {
    if let Err(e) = api.tell(me, LEAD, body).await {
        eprintln!("{}", text::host_session_untold(&format!("{e:#}")));
    }
}

/// `riff host-session` in the folder `dir`, at `server`: the session on
/// the host for the lead (01M7GTEYVRA3549J3PCA0DG22R). It gives the exit code of
/// `claude`. The caller refused each agent session before.
pub async fn run(server: &str, dir: &Path) -> Result<i32> {
    let place = identity::place(dir)?;
    let Some(repo) = place.default_thread() else {
        bail!(text::HOST_SESSION_NO_REPO);
    };
    let id = identity::new_session_id();
    let me = identity::agent(&place, &id, server)?;
    let api = Api::new(server).signed_in(Some(&id))?;
    api.register(&me)
        .await
        .context("riff host-session cannot register the session at riff-server")?;
    api.alive_in_host_session(&me).await?;
    let lead = match api.who(&me, false).await {
        Ok(sessions) => lead_in(&sessions, &repo),
        Err(_) => None,
    };
    let bin = std::env::current_exe().context("cannot find the path of riff")?;
    let command = command(&bin, &id, &text::host_session_start(&me, lead.as_ref()));
    tell_lead(&api, &me, &text::host_session_started(&me)).await;
    println!("{}", text::host_session_line(&me, lead.as_ref()));
    let code = run_claude(&api, &me, &command, dir).await;
    tell_lead(&api, &me, &text::host_session_ended(&me)).await;
    if let Err(e) = api.end(&me).await {
        eprintln!("riff host-session: cannot end the session at riff-server: {e:#}");
    }
    code
}

/// Runs `command` in the terminal, in the folder `dir`, as the session
/// `me` at the riff-server of `api`, and sends its keep-alive until the
/// end. The riff commands and the riff tools of the session reach the
/// same riff-server. A Ctrl-C goes to
/// `claude` only: `claude` takes it itself. It gives the exit code of
/// `claude`, or 1 after a signal.
async fn run_claude(api: &Api, me: &SessionUri, command: &[String], dir: &Path) -> Result<i32> {
    let id = me.who().session().unwrap_or_default();
    let (program, args) = command.split_first().context(text::RELAY_NO_COMMAND)?;
    // The handler comes first: a Ctrl-C after it never ends riff.
    let mut int = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .current_dir(dir)
        .env(identity::SESSION_VARS[0], id)
        .env(VAR, id)
        .env("RIFF_SERVER", api.base())
        .env_remove(crate::daemon::SOCKET_VAR)
        .spawn()
        .with_context(|| text::no_program(program))?;
    let mut alive = tokio::time::interval(ALIVE_EVERY);
    alive.tick().await;
    let status = loop {
        tokio::select! {
            status = child.wait() => break status?,
            _ = alive.tick() => {
                if let Err(e) = api.alive_in_host_session(me).await {
                    eprintln!("riff host-session: the keep-alive failed: {e:#}");
                }
            }
            _ = int.recv() => {}
        }
    };
    Ok(status.code().unwrap_or(1))
}
