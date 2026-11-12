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
//! --move-open-to-backlog`, `riff release check` and the chat steps of
//! `riff release cut` go the same way: the daemon makes
//! the call on the riff-server of the
//! session, with the sign-in of the person on the host
//! ([`crate::daemon::ServerCall`]). So do `riff members`, `riff audit`,
//! `riff session why`, `riff who` and `riff whoami` (#1338,
//! 01M5QVX8D3JE51FAYRWXMAYFAY, 01M5QW31TFA1W2GWPGRS2Y65GP), `riff plan hold`
//! and `riff plan free` (#1619, 01M6QPQZ0FNEZFQ1F4Q9C7FEEQ), and `riff run`
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
//! and `riff whoami`, the records of the thread `ends`, and the person on
//! GitHub for `riff pr open` ([`Reach::person`]). Only the lead
//! can resume the whole riff, read the log for an audit, run an item and
//! follow the events of a session with `riff daemon events`: the daemon
//! knows by the socket which session asks. `riff daemon events NAME` asks
//! the daemon on the socket, and `riff daemon events NAME --host HOST`
//! asks the daemon of HOST through it ([`Reach::follow_events`],
//! #1441).
//!
//! # Each command in a container
//!
//! Each riff command that asks riff-server takes the socket in a
//! container (01M74A84SA5KCBVGVW7M9ETJ3A): also `riff top`, `riff read`,
//! `riff post`, `riff status`, `riff blocked`, `riff step`, `riff tell`,
//! `riff denials`, `riff pause`, `riff resume`, `riff pr open`, `riff pr
//! wait`, `riff verify`, `riff project list` and the TUI. Each such method of
//! [`Reach`] takes the client and the URI of the session for the way
//! straight, and gives the line for the person: [`Reach::post`],
//! [`Reach::read`], [`Reach::look`] and the others. The table [`COMMANDS`]
//! names the way of each subcommand ([`Way`]). A command of
//! [`Way::Host`], for example `riff claim` or `riff login`, refuses in a
//! container before each call (01M74A8CXVRD352VRJ9FH54HJ1).
//!
//! | Through the socket | Who may |
//! |---|---|
//! | post, tell, read, status, blocked, step | each session, as with its riff tools |
//! | pause and resume, the fact of a pull request, the thread denials | the lead (01M74A8D1GV6TCPXV2XEPVA53A) |
//! | the projects of the riff, for `riff project list` | each session (01M776HMKPTH3S6MYJZ8RJGWXV) |
//!
//! ```mermaid
//! flowchart LR
//!     C["riff COMMAND in a container"] --> W{"its way"}
//!     W -- Socket --> D["the riff daemon of the host:<br/>the call as the session"]
//!     D --> S["riff-server"]
//!     W -- Host --> R["refuses at once:<br/>run it on the host"]
//!     W -- Local --> L["asks no riff-server"]
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use riff_core::build::Build;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{
    BotFact, HoldInfo, Kind, LogReply, MembersReply, Person, PullState, RiffOwner, RiffReply,
    RiffState, SessionInfo, Status, StepChange, WhoReply,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::daemon::ServerCall;
use crate::hold::{Held, Line};
use crate::lifecycle::Source;
use crate::watches::{Op, Registered, Watches};
use crate::{identity, session_relay, text};
use riff_client::api::{Api, PauseScope};

/// What `riff release cut` reads of the riff.
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
    /// How the session acts on GitHub, from the daemon of a container
    /// (01M6WPMWYSW98MQYQCCJ54ZX04). `None` on a host, and from an older
    /// daemon.
    #[serde(default)]
    pub github: Option<crate::daemon::gh_token::ActsAs>,
}

impl WhoamiFacts {
    /// Reads the facts as `me` with `api`. A fault of one read is in the
    /// facts, and fails no other.
    pub async fn read(api: &Api, me: &SessionUri) -> WhoamiFacts {
        WhoamiFacts {
            state: api.pauses(me).await.map_err(|e| format!("{e:#}")),
            uri: api.me(me).await.ok().and_then(|r| r.session).map(|s| s.uri),
            github: None,
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

    /// What riff-server says to `who`: the sessions of the riff, and the
    /// repositories of the leads of the caller (#1666). `all` adds the
    /// gone sessions.
    pub async fn who_reply(&self, all: bool) -> Result<WhoReply> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                api.roster(&me, all).await
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::Who { all }).await,
        }
    }

    /// The sessions of the riff, as `riff who` reads them. `all` adds the
    /// gone sessions.
    pub async fn who(&self, all: bool) -> Result<Vec<SessionInfo>> {
        Ok(self.who_reply(all).await?.sessions)
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

    /// Why riff-server cannot read the keys of the GitHub Apps, from its
    /// facts (`GET /v1/server`, 01M6YC3T3B8JKT5BSQJTYMXBKY), or `None` when it reads
    /// them. The person signs in to read them; a lead asks the daemon.
    pub async fn keys_fault(&self) -> Result<Option<String>> {
        match self {
            Reach::Direct(server) => {
                Ok(Api::new(server).signed_in(None)?.facts().await?.keys_fault)
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::KeysFault).await,
        }
    }

    /// The facts of the shared riff for `riff release check`: the build
    /// of the server, the build of the daemon of each host, and the read
    /// of the keys of the GitHub Apps.
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
        let keys = self.keys_fault().await.map_err(|e| format!("{e:#}"));
        crate::cycle::Served {
            server,
            hosts,
            keys,
        }
    }

    /// The person of this session on GitHub, for the line
    /// `Co-authored-by:` of `riff pr open` (01M6WZ2CG9DFB45E3BQCGQQYNY):
    /// read with the `gh` here, or by the daemon with the `gh` of the
    /// person on the host. A container with a token of an App has no
    /// account of the person.
    pub async fn person(&self) -> Result<crate::pr::GitHubUser> {
        match self {
            Reach::Direct(_) => {
                crate::pr::on_thread(|| crate::pr::read_user(&crate::pr::Gh::here("pr open")))
                    .await?
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::Person).await,
        }
    }

    /// The state of the riff daemon on the socket of the session, for
    /// `riff daemon status` in a container (#1239).
    pub async fn daemon_status(socket: &std::path::Path) -> Result<crate::daemon::Status> {
        ask(socket, &ServerCall::Status).await
    }

    /// The riff processes of the host of the daemon on the socket, for
    /// `riff ps` in a container (#1610). Only the lead may.
    pub async fn daemon_ps(socket: &std::path::Path) -> Result<crate::ps::Ps> {
        ask(socket, &ServerCall::Ps).await
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

    /// Follows the events of the session `name` up to its end, through
    /// the socket of the session, as `riff daemon events NAME` does on a
    /// host (#1441). With no `host`, the daemon on the socket answers.
    /// With a `host`, it asks the daemon of that host through
    /// riff-server, as `riff daemon events NAME --host HOST` does. Only
    /// the lead may. It gives each line to `show`, and gives true when
    /// the session failed.
    pub async fn follow_events(
        &self,
        host: Option<&str>,
        name: &str,
        show: impl FnMut(&str),
    ) -> Result<bool> {
        use crate::daemon::remote::{Request, follow_pages, page_of};
        let Reach::Daemon(socket) = self else {
            anyhow::bail!("this riff has no socket of a session");
        };
        let get = |after, notes| async move {
            let name = name.to_owned();
            match host {
                Some(host) => {
                    let request = Request::Events { name, after, notes };
                    page_of(&self.host(host, &request).await?, host)
                }
                None => {
                    let call = ServerCall::Events { name, after, notes };
                    let text: String = ask(socket, &call).await?;
                    text.parse()
                        .map_err(|()| anyhow!("{}", text::DAEMON_NO_PAGE_HERE))
                }
            }
        };
        follow_pages(get, show).await
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

    /// Tells riff-server the fact `fact`: riff-server posts the line of
    /// riff-bot for it in riff chat ([`riff_core::wire::BotPost`]).
    pub async fn bot(&self, fact: BotFact) -> Result<()> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                api.bot(&me, fact).await
            }
            Reach::Daemon(socket) => {
                let call = ServerCall::Bot { fact };
                let _: serde_json::Value = ask(socket, &call).await?;
                Ok(())
            }
        }
    }

    /// The pauses of the riff for the place of the caller, with each
    /// repository with no live lead (01M776RSYKJK9NETFAD8G0CCVQ): straight,
    /// or through the socket of the session, as `riff whoami` reads them.
    pub async fn pauses(&self) -> Result<RiffReply> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                api.pauses(&me).await
            }
            Reach::Daemon(_) => self.whoami().await?.state.map_err(anyhow::Error::msg),
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

    /// Posts `body` as the note of a step of a release to each other lead
    /// of the riff, and gives the number of leads told
    /// ([`crate::hold::tell_leads`], 01M5SNV75DRFHR26BT2M8243VV).
    pub async fn release_news(&self, body: &str) -> Result<usize> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                Ok(crate::hold::tell_leads(&api, &me, true, body).await?.len())
            }
            Reach::Daemon(socket) => {
                let call = ServerCall::ReleaseNews {
                    body: body.to_owned(),
                };
                ask(socket, &call).await
            }
        }
    }

    /// Holds the version `tag` as the lead of this session, or ends its
    /// hold when `reason` is `None` (01M5SNV78EBAYY4SZ891DM2MQG).
    pub async fn release_hold(&self, tag: &str, reason: Option<&str>) -> Result<Held> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                match reason {
                    Some(reason) => crate::hold::hold(&api, &me, tag, reason).await,
                    None => crate::hold::end(&api, &me, tag).await,
                }
            }
            Reach::Daemon(socket) => {
                let call = ServerCall::ReleaseHold {
                    tag: tag.to_owned(),
                    reason: reason.map(str::to_owned),
                };
                ask(socket, &call).await
            }
        }
    }

    /// Each message of the thread `release`, for the holds.
    pub async fn release_lines(&self) -> Result<Vec<Line>> {
        match self {
            Reach::Direct(server) => {
                let place = identity::place(&identity::working_dir()?)?;
                crate::hold::read_as_person(server, &place).await
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::ReleaseLines).await,
        }
    }

    /// Holds `item` of the repository thread `thread` with `reason` as
    /// this session, and gives the line for the person (#1619,
    /// 01M6QPQZ0FNEZFQ1F4Q9C7FEEQ). Only a lead of the thread, the owner
    /// or an admin can: riff-server decides, also through the socket.
    pub async fn plan_hold(&self, thread: &ThreadName, item: &str, reason: &str) -> Result<String> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                let reply = api.hold(&me, thread, item, reason).await?;
                Ok(text::held(&reply, thread, item))
            }
            Reach::Daemon(socket) => {
                let call = ServerCall::PlanHold {
                    thread: thread.clone(),
                    item: item.to_owned(),
                    reason: reason.to_owned(),
                };
                ask(socket, &call).await
            }
        }
    }

    /// Frees the hold of `item` of the repository thread `thread` as this
    /// session, and gives the line for the person (#1619).
    pub async fn plan_free(&self, thread: &ThreadName, item: &str) -> Result<String> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                let reply = api.free(&me, thread, item).await?;
                Ok(text::freed(reply, thread, item))
            }
            Reach::Daemon(socket) => {
                let call = ServerCall::PlanFree {
                    thread: thread.clone(),
                    item: item.to_owned(),
                };
                ask(socket, &call).await
            }
        }
    }

    /// The step of riff-server of the approval of the deploy of `tag` of
    /// `repo` (01M75WKSRP6P4GGJPWTVRMDW85): it pauses the whole riff, and gives the
    /// line for the person. With the policy `required`, riff asks for the
    /// proof of the approval first. Only `riff release approve` of a
    /// person in a terminal pauses so: a session in a container gets the
    /// refusal of a step of a person (01M6WZ40SFVZ6CRQ1H4Z27CWP4).
    pub async fn approve(&self, repo: &ThreadName, tag: &str) -> Result<String> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                let action = riff_core::proof::Action::Approve {
                    repo: repo.clone(),
                    tag: tag.to_owned(),
                };
                let asker = crate::proof::Asker::terminal();
                let (reply, posted) = crate::proof::act(&api, &action, &asker, |api| {
                    let me = me.clone();
                    async move { api.approve(&me, repo, tag).await }
                })
                .await?;
                let state = RiffState::Paused;
                Ok(text::riff_set(None, state, &reply, &posted))
            }
            Reach::Daemon(_) => anyhow::bail!(text::release_refused(
                "approve",
                crate::cycle::Refusal::Agent
            )),
        }
    }

    /// Resumes the whole riff, and gives the line for the person. With
    /// the policy `required`, a person gives a proof first, and a session
    /// gets the refusal (01M75WKSD5A66RHQMMM5CMKWJB).
    pub async fn resume_riff(&self) -> Result<String> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                let state = RiffState::Running;
                let action = riff_core::proof::Action::ResumeRiff;
                let asker = crate::proof::Asker::terminal();
                let (reply, posted) = crate::proof::act(&api, &action, &asker, |api| {
                    let me = me.clone();
                    async move { api.set_pause(&me, &PauseScope::Riff, state).await }
                })
                .await?;
                Ok(text::riff_set(None, state, &reply, &posted))
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::ResumeRiff).await,
        }
    }

    /// True when `riff top` and the TUI go on after `error` of a look,
    /// with the last good look (01M3Z8FXE2DY34ZP75WJE1S8HR). Straight, a
    /// new try must be able to repair it ([`riff_client::api::passes`]).
    /// Through the socket, each error passes: the riff daemon of the
    /// host starts again after an update (01M74A8D8AMEM1A64XWXPC65J5).
    ///
    /// ```
    /// use riff::reach::Reach;
    ///
    /// let refused = anyhow::anyhow!("the daemon went away");
    /// assert!(Reach::Daemon("/run/riff/riff.sock".into()).passes(&refused));
    /// assert!(!Reach::Direct("http://127.0.0.1:7878".into()).passes(&refused));
    /// ```
    pub fn passes(&self, error: &anyhow::Error) -> bool {
        match self {
            Reach::Direct(_) => riff_client::api::passes(error),
            Reach::Daemon(_) => true,
        }
    }

    /// One look of `riff top` and of the TUI: the state of the riff and
    /// the reply to `who`, with the state of each session that the server
    /// sent none for, and the time of the call `who`
    /// (01M74A8D8AMEM1A64XWXPC65J5). Straight, it
    /// asks with `api` as `me` ([`crate::top::look`]). Through the socket,
    /// one call reads both.
    pub async fn look(
        &self,
        api: &Api,
        me: &SessionUri,
    ) -> Result<(RiffReply, WhoReply, std::time::Duration)> {
        match self {
            Reach::Direct(_) => crate::top::look(api, me).await,
            Reach::Daemon(_) => {
                let start = std::time::Instant::now();
                let WhoFacts { pauses, mut who } = self.who_facts(false).await?;
                let call = start.elapsed();
                crate::state::fill(&mut who.sessions, pauses.state);
                Ok((pauses, who, call))
            }
        }
    }

    /// The holds of the thread `thread`, by their item, for `riff top` and
    /// the TUI. Straight, it asks with `api` as `me`.
    pub async fn holds_as(
        &self,
        api: &Api,
        me: &SessionUri,
        thread: &ThreadName,
    ) -> Result<BTreeMap<String, HoldInfo>> {
        match self {
            Reach::Direct(_) => Ok(api.plan_show(me, thread).await?.holds),
            Reach::Daemon(_) => self.holds(thread).await,
        }
    }

    /// The live sessions of the riff, for `riff verify` and `riff pr
    /// open`. Straight, it asks with `api` as `me`.
    pub async fn sessions(&self, api: &Api, me: &SessionUri) -> Result<Vec<SessionInfo>> {
        match self {
            Reach::Direct(_) => api.who(me, false).await,
            Reach::Daemon(_) => self.who(false).await,
        }
    }

    /// `riff post`: posts `body` of the kind `kind` to `thread`, and wakes
    /// the sessions that `to` selects. It gives the line for the person.
    pub async fn post(
        &self,
        api: &Api,
        me: &SessionUri,
        thread: &ThreadName,
        to: &[riff_core::selector::Selector],
        body: &str,
        kind: Kind,
    ) -> Result<String> {
        match self {
            Reach::Direct(_) => Ok(text::posted(
                &api.post(me, Some(thread), to, body, kind).await?,
            )),
            Reach::Daemon(socket) => {
                let call = ServerCall::Post {
                    thread: thread.clone(),
                    to: to.to_vec(),
                    body: body.to_owned(),
                    kind,
                };
                ask(socket, &call).await
            }
        }
    }

    /// `riff status`: sets the status of the session to `step`, and gives
    /// the line for the person.
    pub async fn set_status(&self, api: &Api, me: &SessionUri, step: &str) -> Result<String> {
        match self {
            Reach::Direct(_) => {
                let status = Status {
                    step: step.to_owned(),
                };
                api.status(me, &status).await?;
                Ok(text::status_set(&status))
            }
            Reach::Daemon(socket) => {
                let call = ServerCall::SetStatus {
                    step: step.to_owned(),
                };
                ask(socket, &call).await
            }
        }
    }

    /// `riff blocked`: says that the session cannot go on, and gives the
    /// line for the person.
    pub async fn blocked(&self, api: &Api, me: &SessionUri, reason: &str) -> Result<String> {
        match self {
            Reach::Direct(_) => Ok(text::blocked_set(reason, api.blocked(me, reason).await?)),
            Reach::Daemon(socket) => {
                let call = ServerCall::Blocked {
                    reason: reason.to_owned(),
                };
                ask(socket, &call).await
            }
        }
    }

    /// `riff step`: starts, ends or fails a long step, and gives the line
    /// for the person.
    pub async fn step(&self, api: &Api, me: &SessionUri, change: StepChange) -> Result<String> {
        match self {
            Reach::Direct(_) => {
                let told = api.step(me, change.clone()).await?;
                Ok(text::step_set(&change, told))
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::Step { change }).await,
        }
    }

    /// `riff tell`: sends `body` to the session `session`, and gives the
    /// line for the person.
    pub async fn tell(
        &self,
        api: &Api,
        me: &SessionUri,
        session: &str,
        body: &str,
    ) -> Result<String> {
        match self {
            Reach::Direct(_) => Ok(text::posted(&api.tell(me, session, body).await?)),
            Reach::Daemon(socket) => {
                let call = ServerCall::Tell {
                    session: session.to_owned(),
                    body: body.to_owned(),
                };
                ask(socket, &call).await
            }
        }
    }

    /// `riff read`: the text of the unread messages ([`read`]), after a
    /// join of the thread `repo`. `thread` reads only that thread, `all`
    /// the full history. Through the socket, the session joins the thread
    /// of its own repository.
    pub async fn read(
        &self,
        api: &Api,
        me: &SessionUri,
        repo: Option<&ThreadName>,
        thread: Option<&str>,
        all: bool,
    ) -> Result<String> {
        match self {
            Reach::Direct(_) => read(api, me, repo, thread, all).await,
            Reach::Daemon(socket) => {
                let call = ServerCall::Read {
                    thread: thread.map(str::to_owned),
                    all,
                };
                ask(socket, &call).await
            }
        }
    }

    /// Tells the fact of the pull request `number` of the issue `issue`,
    /// with no error ([`crate::look::tell_fact`]): `riff pr open`, `riff
    /// verify` and `riff pr wait` tell it after their work, and their work
    /// is done (01M41FZP2C4Z4J6WKRXZ5B31EH).
    pub async fn fact(
        &self,
        api: &Api,
        me: &SessionUri,
        issue: u64,
        number: u64,
        state: PullState,
    ) {
        match self {
            Reach::Direct(_) => crate::look::tell_fact(api, me, issue, number, state).await,
            Reach::Daemon(socket) => {
                let call = ServerCall::Fact {
                    issue,
                    number,
                    state,
                };
                let _: Result<serde_json::Value> = ask(socket, &call).await;
            }
        }
    }

    /// `riff pause` and `riff resume`: pauses or resumes the repository of
    /// `here`, the repository `repo`, or with `riff` the whole riff, and
    /// gives the line for the person ([`pause`]). With the policy
    /// `required`, the pause and the resume of the whole riff need a proof:
    /// `asker` asks for it (01M75WKSD5A66RHQMMM5CMKWJB).
    #[allow(clippy::too_many_arguments)]
    pub async fn pause_as(
        &self,
        api: &Api,
        me: &SessionUri,
        here: &riff_core::name::Place,
        riff: bool,
        repo: Option<ThreadName>,
        state: RiffState,
        asker: &crate::proof::Asker,
    ) -> Result<String> {
        match self {
            Reach::Direct(_) if riff => {
                let action = match state {
                    RiffState::Paused => riff_core::proof::Action::PauseRiff,
                    RiffState::Running => riff_core::proof::Action::ResumeRiff,
                };
                crate::proof::act(api, &action, asker, |api| async move {
                    pause(&api, me, here, PauseScope::Riff, state).await
                })
                .await
            }
            Reach::Direct(_) => pause(api, me, here, PauseScope::of(riff, repo), state).await,
            Reach::Daemon(socket) => ask(socket, &ServerCall::Pause { riff, repo, state }).await,
        }
    }

    /// Each project of the riff, for `riff project list`
    /// (01M76SHB4SAWFW3S5Y7X9KC6E4): straight, or in a container through
    /// the socket of the session (01M776HMKPTH3S6MYJZ8RJGWXV). Each person
    /// and each session may read them.
    pub async fn projects(&self) -> Result<riff_core::wire::ProjectsReply> {
        match self {
            Reach::Direct(server) => {
                let (api, me) = signed(server)?;
                api.projects(&me).await
            }
            Reach::Daemon(socket) => ask(socket, &ServerCall::Projects).await,
        }
    }

    /// The records of the thread `denials` at or after `from_ms`, oldest
    /// first, for `riff denials` ([`crate::denied::read`]).
    pub async fn denials(
        &self,
        api: &Api,
        me: &SessionUri,
        from_ms: u64,
    ) -> Result<Vec<crate::denied::Record>> {
        match self {
            Reach::Direct(_) => crate::denied::read(api, me, from_ms).await,
            Reach::Daemon(socket) => {
                let bodies: Vec<(u64, String)> = ask(socket, &ServerCall::Denials).await?;
                Ok(crate::denied::records(
                    bodies.iter().map(|(at_ms, body)| (*at_ms, body.as_str())),
                    from_ms,
                ))
            }
        }
    }
}

/// The way of a riff command to riff-server in a container session:
/// with `RIFF_SOCKET`, and with no `--server` and no `RIFF_SERVER`
/// (#1733).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    /// It asks riff-server through the socket of the session: the riff
    /// daemon of the host makes each call for the session.
    Socket,
    /// It needs the host: the sign-in of a person, a claim, or a program
    /// of the host. In a container it refuses at once, and asks nothing.
    Host,
    /// It asks no riff-server.
    Local,
}

/// One riff command in a container session ([`COMMANDS`]).
#[derive(Debug, Clone, Copy)]
pub struct InContainer {
    /// The words of the command after `riff`, with an example of each
    /// argument.
    pub args: &'static [&'static str],
    /// Its way to riff-server in a container session.
    pub way: Way,
}

const fn socket(args: &'static [&'static str]) -> InContainer {
    InContainer {
        args,
        way: Way::Socket,
    }
}

const fn host(args: &'static [&'static str]) -> InContainer {
    InContainer {
        args,
        way: Way::Host,
    }
}

const fn local(args: &'static [&'static str]) -> InContainer {
    InContainer {
        args,
        way: Way::Local,
    }
}

/// Each riff command, with its way to riff-server in a container
/// session (01M74A84SA5KCBVGVW7M9ETJ3A). No command there asks
/// `http://127.0.0.1:7878`, the riff-server of the machine: the container
/// has none. The tests of `riff` check that each subcommand of the CLI
/// has a row, and run each row with [`Way::Socket`] and [`Way::Host`] in
/// a container session. A subcommand with a flag `--host` has a row for
/// each way.
///
/// ```
/// use riff::reach::{COMMANDS, Way};
///
/// let way = |args: &[&str]| COMMANDS.iter().find(|c| c.args == args).map(|c| c.way);
/// assert_eq!(way(&["top", "--once"]), Some(Way::Socket));
/// assert_eq!(way(&["read"]), Some(Way::Socket));
/// assert_eq!(way(&["claim", "issue-12"]), Some(Way::Host));
/// assert_eq!(way(&["usage"]), Some(Way::Local));
/// ```
pub const COMMANDS: &[InContainer] = &[
    // The commands of a session.
    socket(&["whoami"]),
    socket(&["who"]),
    socket(&["post", "hello"]),
    socket(&["status", "writing the tests"]),
    socket(&["blocked", "which design?"]),
    socket(&["step", "start", "a live window"]),
    socket(&["step", "done"]),
    socket(&["step", "fail", "the window closed"]),
    socket(&["tell", "lead", "done"]),
    socket(&["read"]),
    socket(&["top", "--once"]),
    socket(&["denials"]),
    socket(&["session", "why", "issue-12"]),
    socket(&["members"]),
    socket(&["questions"]),
    socket(&["pr", "open", "--title", "Show the wave", "--issue", "12"]),
    socket(&["pr", "wait", "40"]),
    socket(&[
        "verify",
        "fail",
        "40",
        "--file",
        "result.md",
        "--commit",
        "1a2b3c4d",
    ]),
    socket(&["ps"]),
    socket(&["project", "list"]),
    // The commands of the lead.
    socket(&["pause"]),
    socket(&["resume"]),
    socket(&["plan", "hold", "issue-12", "a reading at 19:40"]),
    socket(&["plan", "free", "issue-12"]),
    socket(&["run", "issue-12"]),
    socket(&["audit", "--wave", "3"]),
    socket(&["wave", "list"]),
    socket(&["wave", "move", "14", "--to", "3"]),
    socket(&["wave", "end", "3", "--move-open-to-backlog"]),
    socket(&["host", "join"]),
    socket(&["host", "join", "--host", "thelio"]),
    socket(&["host", "leave", "--host", "thelio"]),
    socket(&["lead", "stop", "--host", "thelio"]),
    socket(&["build", "--host", "thelio"]),
    socket(&["release", "cut", "3.4.0"]),
    socket(&["release", "status"]),
    socket(&["release", "approve", "v3.4.0"]),
    socket(&["release", "hold", "v3.4.0", "a reading at 19:40"]),
    socket(&["release", "check", "v3.4.0"]),
    socket(&["release", "retry", "v3.4.0"]),
    socket(&["release", "test", "v3.4.0", "--host", "thelio"]),
    socket(&["release", "rollback", "v3.4.0"]),
    socket(&["daemon", "status"]),
    socket(&["daemon", "status", "--host", "thelio"]),
    socket(&["daemon", "events", "riff-issue-12-1a2b3c4d"]),
    socket(&[
        "daemon",
        "events",
        "riff-issue-12-1a2b3c4d",
        "--host",
        "thelio",
    ]),
    socket(&["daemon", "limit", "--sessions", "4", "--host", "thelio"]),
    socket(&["daemon", "cache", "--host", "thelio"]),
    socket(&["daemon", "sweep", "--host", "thelio"]),
    socket(&[
        "daemon",
        "end",
        "riff-issue-12-1a2b3c4d",
        "--host",
        "thelio",
    ]),
    socket(&["daemon", "restart", "--host", "thelio"]),
    // The relay of the session.
    socket(&["relay", "--", "true"]),
    socket(&["relay", "mcp"]),
    socket(&["relay", "watch"]),
    socket(&["relay", "denied"]),
    socket(&["relay", "prompt"]),
    socket(&["relay", "idle"]),
    // The steps of a person, and the claims.
    host(&["login"]),
    host(&["logout"]),
    host(&["invite", "ada@example.com"]),
    host(&["remove", "ada@example.com"]),
    host(&["admin", "add", "ada@example.com"]),
    host(&["admin", "remove", "ada@example.com"]),
    host(&["admin", "proof", "required"]),
    host(&["owner", "--take"]),
    host(&["chat"]),
    host(&["tail"]),
    host(&["claim", "issue-12"]),
    host(&["release", "issue-12"]),
    host(&["lead", "--session", "c1d2"]),
    host(&["lead", "start"]),
    host(&["lead", "stop"]),
    host(&["lead", "move", "thelio"]),
    host(&["lead", "notes"]),
    host(&["lead", "notes", "edit"]),
    host(&["host", "leave"]),
    host(&["questions", "--answered", "12"]),
    host(&["server"]),
    host(&["update"]),
    host(&["cloud", "create", "riff"]),
    host(&["cloud", "signin", "riff"]),
    host(&["cloud", "deploy", "riff"]),
    host(&["cloud", "smoke", "riff"]),
    host(&["cloud", "list"]),
    host(&["cloud", "status", "riff"]),
    host(&["cloud", "check", "riff"]),
    host(&["cloud", "log", "riff"]),
    host(&["cloud", "delete", "riff"]),
    host(&["github", "create", "--owner", "acme"]),
    host(&["github", "install", "acme"]),
    host(&["github", "check"]),
    host(&["github", "events"]),
    host(&["device", "add", "--key", "yubikey"]),
    host(&["device", "list"]),
    host(&["device", "remove", "yubikey"]),
    host(&["device", "test"]),
    host(&["project", "init", "--host", "thelio"]),
    host(&["project", "remove", "acme/app"]),
    // No riff-server.
    local(&["usage"]),
    local(&["init", "--check"]),
    local(&["setup", "--check"]),
    local(&["image"]),
    local(&["build"]),
    local(&["wave", "make", "Wave 4"]),
    local(&["wave", "rename", "3", "the new wave"]),
    local(&["wave", "end", "3"]),
    local(&["takeover", "riff-issue-12-1a2b3c4d"]),
    local(&["claude-token"]),
    local(&["daemon"]),
    local(&["daemon", "install"]),
    local(&["daemon", "uninstall"]),
    local(&["daemon", "limit", "--sessions", "4"]),
    local(&["daemon", "cache"]),
    local(&["daemon", "sweep"]),
    local(&["daemon", "end", "riff-issue-12-1a2b3c4d"]),
    local(&["daemon", "restart"]),
    local(&["daemon", "mcp"]),
    local(&["daemon", "mcp", "add", "docs"]),
    local(&["daemon", "mcp", "remove", "docs"]),
];

/// `riff read` as `me` at `api`: `me` joins the thread `repo` first, the
/// thread of the repository of the folder, and gets the text of its
/// unread messages. `thread` reads only that thread, `all` the full
/// history. The riff daemon reads so for a session in a container, with
/// the thread of the repository of the session ([`ServerCall::Read`],
/// #1733).
pub async fn read(
    api: &Api,
    me: &SessionUri,
    repo: Option<&ThreadName>,
    thread: Option<&str>,
    all: bool,
) -> Result<String> {
    if let Some(repo) = repo {
        api.join(me, repo).await?;
    }
    let thread = match thread {
        Some(t) => Some(api.thread_to_read(me, t).await?),
        None => None,
    };
    let inbox = api.inbox(me, thread.as_ref(), all).await?;
    Ok(text::inbox(&inbox, me).trim_end().to_owned())
}

/// `riff pause` and `riff resume`: pauses or resumes `scope` as `me` at
/// `api`, wakes each session that the change stops or starts, and gives
/// the line for the person (01M3XAHZDSQR263QZVB41CK0MX,
/// 01M3JCG3YD7C2Y3V0QJPF082YH). The Waves tab of the TUI calls it too.
///
/// The URI of a person names only the host. For the pause of the
/// repository of the directory `here`, the person calls from `here`, so
/// the server knows the repository.
pub async fn pause(
    api: &Api,
    me: &SessionUri,
    here: &riff_core::name::Place,
    scope: PauseScope,
    state: RiffState,
) -> Result<String> {
    let at_here;
    let me = if matches!(scope, PauseScope::Here) && me.who().session().is_none() {
        at_here = SessionUri::new(me.who().clone(), here.clone());
        &at_here
    } else {
        me
    };
    let (reply, posted) = api.set_pause(me, &scope, state).await?;
    let repository = scope.repository(me);
    Ok(text::riff_set(repository.as_ref(), state, &reply, &posted))
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

/// Makes `call` through the daemon on `socket`, and reads its answer. An
/// answer that this riff cannot read gives the error of
/// [`text::daemon_other_answer`], with the build of the daemon
/// ([`daemon_build`], #1693, 01M6WBX2FYPH0K8RAKXMRKDGX7).
async fn ask<T: DeserializeOwned>(socket: &std::path::Path, call: &ServerCall) -> Result<T> {
    let reply = session_relay::ask_server(socket, call).await?;
    match serde_json::from_value(reply) {
        Ok(answer) => Ok(answer),
        Err(e) => {
            let theirs = daemon_build(socket).await;
            let fault = e.to_string();
            Err(anyhow!(text::daemon_other_answer(
                &fault,
                &Build::this(),
                theirs.as_deref()
            )))
        }
    }
}

/// The build of the riff daemon on `socket`, as its status names it. It
/// reads only the field `build`, so a status of another shape gives it
/// too. `None` when the daemon does not answer, or names no build.
pub async fn daemon_build(socket: &std::path::Path) -> Option<String> {
    let status = session_relay::ask_server(socket, &ServerCall::Status)
        .await
        .ok()?;
    status.get("build")?.as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    /// A riff daemon of another build on a socket in `dir`: it answers
    /// `who` in a shape that this riff cannot read, and its status with
    /// `build`, or with no build.
    fn other_daemon(dir: &std::path::Path, build: Option<&'static str>) -> PathBuf {
        let path = dir.join("riff.sock");
        let listener = UnixListener::bind(&path).unwrap();
        tokio::spawn(async move {
            while let Ok((conn, _)) = listener.accept().await {
                let (read, mut write) = conn.into_split();
                let mut lines = BufReader::new(read).lines();
                let Ok(Some(first)) = lines.next_line().await else {
                    continue;
                };
                let ask: serde_json::Value = serde_json::from_str(&first).unwrap();
                let reply = match ask["call"]["call"].as_str() {
                    Some("status") => serde_json::json!({ "build": build, "pid": 7 }),
                    _ => serde_json::json!({ "sessions": { "s1": {} } }),
                };
                let say = serde_json::json!({ "say": "served", "reply": reply });
                write
                    .write_all(format!("{say}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        path
    }

    /// #1693, 01M6WBX2FYPH0K8RAKXMRKDGX7: an answer of a newer daemon
    /// that this riff cannot read names the two builds and the step, in
    /// place of the bare fault of serde.
    #[tokio::test]
    async fn an_answer_of_a_newer_daemon_names_the_two_builds_and_the_step() {
        let dir = tempfile::tempdir().unwrap();
        let socket = other_daemon(dir.path(), Some("999.0.0 bbbb 2099-01-01T00:00:00Z"));
        let error = Reach::Daemon(socket).who(false).await.unwrap_err();
        let error = format!("{error:#}");
        assert!(
            error.starts_with("the riff daemon runs a newer riff: wait for the restart"),
            "{error}"
        );
        assert!(
            error.contains("The riff daemon runs the build 999.0.0 bbbb"),
            "{error}"
        );
        assert!(
            error.contains(&format!("this riff is the build {}", Build::this())),
            "{error}"
        );
        assert!(error.contains("next: end your turn"), "{error}");
    }

    /// A daemon whose status names no build still gives the build of this
    /// riff and the fault.
    #[tokio::test]
    async fn an_answer_of_a_daemon_with_no_build_names_the_build_of_this_riff() {
        let dir = tempfile::tempdir().unwrap();
        let socket = other_daemon(dir.path(), None);
        assert_eq!(daemon_build(&socket).await, None);
        let error = Reach::Daemon(socket).who(false).await.unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains("names no build"), "{error}");
        assert!(
            error.contains(&format!("This riff is the build {}", Build::this())),
            "{error}"
        );
        assert!(error.contains("invalid type: map"), "{error}");
    }
}
