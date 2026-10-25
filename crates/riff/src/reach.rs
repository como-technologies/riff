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
//! --move-open-to-backlog`, `riff release check` and the chat and pause
//! steps of `riff release cut`, `publish` and `approve` go
//! the same way: the daemon makes the call on the riff-server of the
//! session, with the sign-in of the person on the host
//! ([`crate::daemon::ServerCall`]). So do `riff members`, `riff audit`,
//! `riff session why`, `riff who` and `riff whoami` (#1338,
//! 01M5QVX8D3JE51FAYRWXMAYFAY, 01M5QW31TFA1W2GWPGRS2Y65GP), and `riff run`
//! of the lead (01M5QVX8FFG2FPWZEW61A985CF): the daemon runs the item as a
//! job of the wave, because the container has no control socket of the
//! daemon.
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
//! the sessions, the holds of a thread, the members, the state of `riff who`
//! and `riff whoami`, and the records of the thread `ends`. Only the lead
//! can resume the whole riff, read the log for an audit and run an item:
//! the daemon knows by the socket which session asks.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{
    HoldInfo, Kind, LogReply, MembersReply, Person, RiffOwner, RiffReply, RiffState, SessionInfo,
    WhoReply,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::daemon::ServerCall;
use crate::lifecycle::Source;
use crate::watches::{Op, Registered, Watches};
use crate::{identity, session_relay, text};
use riff_client::api::{Api, PauseScope};

/// What `riff release cut`, `publish` and `approve` read of the riff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Roster {
    /// The USER of this session.
    pub user: String,
    /// The owner of the riff.
    pub owner: RiffOwner,
    /// Each member of the riff.
    pub people: Vec<Person>,
}

/// What `riff whoami` reads of the riff: the state of the pauses, or why
/// riff cannot read it, and the URI that the server holds for the
/// session (#1338, 01M5QW31TFA1W2GWPGRS2Y65GP).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhoamiFacts {
    /// The state of the riff for the place of the caller.
    pub state: std::result::Result<RiffReply, String>,
    /// The URI of the session as the server holds it (its claims and
    /// its lead), or `None` when the server does not know the session.
    pub uri: Option<SessionUri>,
}

impl WhoamiFacts {
    /// Reads the facts as `me` with `api`. A fault of one read is in the
    /// facts, and fails no other.
    pub async fn read(api: &Api, me: &SessionUri) -> WhoamiFacts {
        WhoamiFacts {
            state: api.pauses(me).await.map_err(|e| format!("{e:#}")),
            uri: api.me(me).await.ok().and_then(|r| r.session).map(|s| s.uri),
        }
    }
}

/// What `riff who` reads of the riff: the state of the pauses, the owner
/// and the sessions (#1338, 01M5QW31TFA1W2GWPGRS2Y65GP).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhoFacts {
    /// The state of the riff for the place of the caller.
    pub pauses: RiffReply,
    /// The owner and the sessions.
    pub who: WhoReply,
}

impl WhoFacts {
    /// Reads the facts as `me` with `api`. `all` adds the gone sessions.
    pub async fn read(api: &Api, me: &SessionUri, all: bool) -> Result<WhoFacts> {
        Ok(WhoFacts {
            pauses: api.pauses(me).await?,
            who: api.roster(me, all).await?,
        })
    }
}

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

    /// The user of this session, and the owner and the people of the
    /// riff.
    pub async fn roster(&self) -> Result<Roster> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                let reply = api.roster(&me, false).await?;
                Ok(Roster {
                    user: me.who().user().to_owned(),
                    owner: reply.owner,
                    people: reply.people,
                })
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::Roster).await,
        }
    }

    /// Posts `body` in riff chat. It wakes the leads that it names.
    pub async fn chat_post(&self, body: &str) -> Result<()> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                let to = crate::chat::wakes(body, me.who().user());
                let thread = crate::chat::thread();
                api.post(&me, Some(&thread), &to, body, Kind::Message)
                    .await?;
                Ok(())
            }
            Reach::Daemon(socket) => {
                let call = ServerCall::ChatPost {
                    body: body.to_owned(),
                };
                let _: serde_json::Value = ask(socket, &call).await?;
                Ok(())
            }
        }
    }

    /// Each kept message of riff chat.
    pub async fn chat_read(&self) -> Result<Vec<crate::cycle::ChatLine>> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                let read = api.read(&me, &crate::chat::thread(), true).await?;
                Ok(crate::cycle::chat_lines(read))
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::ChatRead).await,
        }
    }

    /// What `riff whoami` reads, through the socket of the session. On a
    /// host, `riff whoami` reads straight with [`WhoamiFacts::read`].
    pub async fn whoami(&self) -> Result<WhoamiFacts> {
        let Reach::Daemon(socket) = self else {
            anyhow::bail!("this riff has no socket of a session");
        };
        ask(socket, &ServerCall::Whoami).await
    }

    /// What `riff who` reads, through the socket of the session. `all`
    /// adds the gone sessions. On a host, `riff who` reads straight with
    /// [`WhoFacts::read`].
    pub async fn who_facts(&self, all: bool) -> Result<WhoFacts> {
        let Reach::Daemon(socket) = self else {
            anyhow::bail!("this riff has no socket of a session");
        };
        ask(socket, &ServerCall::WhoFacts { all }).await
    }

    /// Who may join the riff, for `riff members`. Straight, it reads
    /// with the sign-in of the person on this machine (#1338).
    pub async fn members(&self) -> Result<MembersReply> {
        match self {
            Reach::Direct(server) => Api::new(server).signed_in(None)?.members().await,
            Reach::Daemon(socket) => ask(socket, &ServerCall::Members).await,
        }
    }

    /// The records of the repository thread `repo`, for `riff audit`.
    /// Only the owner and the admins can read them; through the socket,
    /// only the lead can ask (#1338).
    pub async fn log(&self, repo: &ThreadName) -> Result<LogReply> {
        match self {
            Reach::Direct(server) => Api::new(server).signed_in(None)?.log(repo).await,
            Reach::Daemon(socket) => {
                let call = ServerCall::Log { repo: repo.clone() };
                ask(socket, &call).await
            }
        }
    }

    /// The records of the thread `ends`, oldest first, for `riff session
    /// why` (#1338).
    pub async fn ends(&self) -> Result<Vec<crate::ends::Record>> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                crate::ends::read(&api, &me).await
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::Ends).await,
        }
    }

    /// Asks the daemon to run the item `item` as a job of the wave of the
    /// repository, and gives the line for the person (#1338). Only the
    /// lead may. On the host, `riff run` talks to the daemon on its own
    /// control socket.
    pub async fn run(&self, item: &str) -> Result<String> {
        let Reach::Daemon(socket) = self else {
            anyhow::bail!("this riff has no socket of a session");
        };
        let call = ServerCall::Run {
            item: item.to_owned(),
        };
        ask(socket, &call).await
    }

    /// Registers the watch of `op` in `repo`: the daemon wakes the lead at
    /// the end of the operation ([`crate::watches`],
    /// 01M5RGCW5NCKQYA61RM63914TX). On the host, riff writes the file of
    /// the watches of the daemon itself, so a daemon that is not up reads
    /// it at its start. Through the socket, the daemon writes it.
    pub async fn watch(&self, repo: &str, op: Op) -> Result<Registered> {
        match self {
            Reach::Direct(_) => {
                let dir = crate::daemon::dir()?;
                Watches::new(&dir).register(repo, op, crate::daemon::now_ms())
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::Watch { op }).await,
        }
    }

    /// Pauses the whole riff, and gives the line for the person.
    pub async fn pause_riff(&self) -> Result<String> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                let state = RiffState::Paused;
                let (reply, posted) = api.set_pause(&me, &PauseScope::Riff, state).await?;
                Ok(text::riff_set(None, state, &reply, &posted))
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::PauseRiff).await,
        }
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
