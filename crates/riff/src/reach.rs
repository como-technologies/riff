//! How a riff command reaches riff-server: straight, or through the riff
//! daemon (#1228, 01M5GVPNAKAPA596K9361HSAAV).
//!
//! # Design
//!
//! A session in a container has `RIFF_SOCKET`, the socket of its session
//! on the riff daemon, but no `RIFF_SERVER` and no sign-in: the keyring
//! of the host is not there. So `http://127.0.0.1:7878`, the default, does
//! not answer in the container. The riff tools already go through the
//! socket. [`Reach`] makes `riff wave list`, `riff wave move`, `riff wave end
//! --move-open-to-backlog` and `riff release check` go
//! the same way: the daemon makes the call on the riff-server of the
//! session, with the sign-in of the person on the host
//! ([`crate::daemon::ServerCall`]).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant C as riff wave list (in the container)
//!     participant D as riff daemon (on the host)
//!     participant S as riff-server
//!     C->>D: server: who (on RIFF_SOCKET)
//!     D->>S: who, as the session, with the sign-in of the host
//!     S-->>D: the sessions
//!     D-->>C: served: the sessions
//! ```
//!
//! | `--server` or `RIFF_SERVER` | `RIFF_SOCKET` | The way |
//! |---|---|---|
//! | yes | any | [`Reach::Direct`]: straight to that server |
//! | no | no | [`Reach::Direct`]: the riff of this machine |
//! | no | yes | [`Reach::Daemon`]: through the socket |
//!
//! Through the socket, each session can read the build of riff-server,
//! the sessions and the holds of a thread. Only the lead can resume the
//! whole riff: the daemon knows by the socket which session asks.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{HoldInfo, RiffState, SessionInfo};
use serde::de::DeserializeOwned;

use crate::daemon::ServerCall;
use crate::lifecycle::Source;
use crate::{identity, session_relay, text};
use riff_client::api::{Api, PauseScope};

/// The way of a riff command to riff-server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// Straight to the riff-server at this URL, with the sign-in of this
    /// machine.
    Direct(String),
    /// Through the riff daemon, on the socket of the session at this
    /// path.
    Daemon(PathBuf),
}

impl Reach {
    /// The way to `server`, which comes from `source`, when `socket` is
    /// the socket of the session (`RIFF_SOCKET`), if any.
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use riff::lifecycle::Source;
    /// use riff::reach::Reach;
    ///
    /// let socket = Some(PathBuf::from("/run/riff/riff.sock"));
    /// let local = "http://127.0.0.1:7878";
    /// assert_eq!(
    ///     Reach::of(local, Source::Default, socket.clone()),
    ///     Reach::Daemon("/run/riff/riff.sock".into())
    /// );
    /// assert_eq!(Reach::of(local, Source::Default, None), Reach::Direct(local.into()));
    /// let named = "https://riff.example.com";
    /// assert_eq!(Reach::of(named, Source::Env, socket.clone()), Reach::Direct(named.into()));
    /// assert_eq!(Reach::of(named, Source::Flag, socket), Reach::Direct(named.into()));
    /// ```
    pub fn of(server: &str, source: Source, socket: Option<PathBuf>) -> Reach {
        match (source, socket) {
            (Source::Default, Some(socket)) => Reach::Daemon(socket),
            _ => Reach::Direct(server.to_owned()),
        }
    }

    /// The version of the build of riff-server, or `None` when it names
    /// none.
    pub async fn build(&self) -> Result<Option<String>> {
        match self {
            Reach::Direct(server) => Ok(Api::new(server)
                .probe(crate::lifecycle::PROBE_WAIT)
                .await?
                .build
                .map(|b| b.version.to_string())),
            Reach::Daemon(socket) => ask(socket, &ServerCall::Build).await,
        }
    }

    /// The sessions of the riff, as `riff who` reads them. `all` adds the
    /// gone sessions.
    pub async fn who(&self, all: bool) -> Result<Vec<SessionInfo>> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                api.who(&me, all).await
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::Who { all }).await,
        }
    }

    /// The holds of the thread `thread`, by their item.
    pub async fn holds(&self, thread: &ThreadName) -> Result<BTreeMap<String, HoldInfo>> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                Ok(api.plan_show(&me, thread).await?.holds)
            }
            Reach::Daemon(socket) => {
                let call = ServerCall::Holds {
                    thread: thread.clone(),
                };
                ask(socket, &call).await
            }
        }
    }

    /// The facts of the shared riff for `riff release check`: the build
    /// of the server, and the build of the daemon of each host.
    pub async fn served(&self) -> crate::cycle::Served {
        let server = self
            .build()
            .await
            .map_err(|e| format!("{e:#}"))
            .and_then(|b| b.ok_or_else(|| "the server names no build".to_owned()));
        let hosts = self
            .who(true)
            .await
            .map(|sessions| {
                riff_client::daemons::hosts(&sessions)
                    .into_iter()
                    .map(|h| (format!("{}@{}", h.user, h.host), h.facts.build))
                    .collect()
            })
            .map_err(|e| format!("{e:#}"));
        crate::cycle::Served { server, hosts }
    }

    /// The state of the riff daemon on the socket of the session, for
    /// `riff daemon status` in a container (#1239).
    pub async fn daemon_status(socket: &std::path::Path) -> Result<crate::daemon::Status> {
        ask(socket, &ServerCall::Status).await
    }

    /// The reply of the daemon of `host` to `request`, through the
    /// daemon on the socket (#1239). Only the lead may.
    pub async fn host(
        &self,
        host: &str,
        request: &crate::daemon::remote::Request,
    ) -> Result<String> {
        let Reach::Daemon(socket) = self else {
            anyhow::bail!("this riff has no socket of a session");
        };
        let call = ServerCall::Host {
            host: host.to_owned(),
            request: request.to_string(),
        };
        ask(socket, &call).await
    }

    /// Resumes the whole riff, and gives the line for the person.
    pub async fn resume_riff(&self) -> Result<String> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                let state = RiffState::Running;
                let (reply, posted) = api.set_pause(&me, &PauseScope::Riff, state).await?;
                Ok(text::riff_set(None, state, &reply, &posted))
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::ResumeRiff).await,
        }
    }
}

/// The Cloud Run log of an instance of riff, read by the riff daemon of
/// the host with its `gcloud` (#1247). The container has no `gcloud` and
/// no Google sign-in. Only the lead may.
pub struct DaemonLog {
    /// The socket of the session.
    pub socket: PathBuf,
    /// The settings of the instance, for the service and the project.
    pub settings: crate::cloud::Settings,
}

impl crate::cycle::CloudLog for DaemonLog {
    /// Asks the daemon. It needs a multi-thread runtime.
    fn lines(&self, filter: &str) -> Result<Vec<crate::cycle::LogLine>> {
        let call = ServerCall::CloudLog {
            service: self.settings.service.clone(),
            project: self.settings.project.clone(),
            filter: filter.to_owned(),
        };
        let handle = tokio::runtime::Handle::current();
        let out: String = tokio::task::block_in_place(|| handle.block_on(ask(&self.socket, &call)))
            .map_err(|e| {
                if e.to_string() == text::DAEMON_NO_GCLOUD {
                    anyhow!(crate::cycle::NoGcloud)
                } else {
                    e
                }
            })?;
        crate::cycle::log_lines(&out)
    }
}

/// The riff of `server` with the sign-in of this machine, and the sender
/// of this process: the agent session with its session ID, else the
/// person on this machine.
fn signed(server: &str) -> Result<(Api, SessionUri)> {
    let place = identity::place(&identity::working_dir()?)?;
    let id = identity::session_id();
    let base = Api::new(server);
    let me = match id.as_deref() {
        Some(id) => identity::agent(&place, id, base.base())?,
        None => identity::person(&place, base.base())?,
    };
    Ok((base.signed_in(id.as_deref())?, me))
}

/// Makes `call` through the daemon on `socket`, and reads its answer.
async fn ask<T: DeserializeOwned>(socket: &std::path::Path, call: &ServerCall) -> Result<T> {
    let reply = session_relay::ask_server(socket, call).await?;
    serde_json::from_value(reply).map_err(|e| anyhow!("the riff daemon gave another answer: {e}"))
}
