//! `riff mcp`: the tools that a session uses, over stdio.
//!
//! The tools keep the URI of their session. `move` changes its place
//! (R64). Each other tool sends the URI with its request.
//!
//! # Life
//!
//! `riff mcp` runs as long as its agent process. So it tells the server
//! that the session runs, and when it ends:
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as agent tool
//!     participant M as riff mcp
//!     participant S as riff-server
//!     M->>S: register
//!     S-->>M: done
//!     Note over M: in tmux, the lead gets its tail pane
//!     loop each ALIVE_EVERY, also while no turn runs
//!         M->>S: alive
//!     end
//!     A-->>M: stdin closes, or SIGTERM, SIGINT or SIGHUP
//!     M->>S: end
//! ```
//!
//! `riff mcp` looks for the lead mark of its session only after its
//! register ends (01M3XM68N5M5DKB86W5079X2G9). A query makes the server
//! know a session, and makes no lead. So a session in `who` with no
//! lead mark can be a session whose register is not done. The tail
//! pane, the rollout and the reap all start after the register.
//!
//! The `riff mcp` of the lead also starts workers by itself when the
//! wave has free work (see [`crate::rollout`]). It ends the session of a
//! worker of its machine whose pane is gone (see [`crate::reap`]). Each
//! 10 minutes it tidies the worktrees and the disk of its machine (see
//! [`crate::tidy`]).
//!
//! A keep-alive is not a call: `who` still shows the time since the
//! last call (R204). After the end call, the session leaves `who`, and
//! its claims are free at once (R205). A session that stops with no end
//! call is gone after 3 minutes with no keep-alive.
//!
//! A session that left the riff makes no call (see [`crate::leave`]).
//! Each tool except `join` refuses, and the keep-alive, the rollout and
//! the reap wait. The client of the tools refuses each request too
//! ([`crate::api`], 01M3XQVJXWBC3DKAVWBPXPSGZS).
//!
//! After `riff update`, `riff mcp` runs the new binary in place at the
//! first moment with no request in flight, with no end call, so that
//! the claims of the session stay. Claude Code keeps its connection: the
//! new process answers the next request with no new handshake
//! (01M3NT6WZTKAFKGDWGCFKC8TB5). See [`crate::relay`]. Claude Code
//! sends no initialize request: it starts with `server/discover` (MCP
//! `2026-07-28`). The old process then gives the new one
//! `--client modern` ([`Client`], 01M53QQR7J7DG86SMAERKMYSAF).
//!
//! ```mermaid
//! flowchart LR
//!     H["the handshake ends"] --> P{"peer info?"}
//!     P -- "yes: initialize" --> J["--client JSON"]
//!     P -- "no: server/discover" --> M["--client modern"]
//!     J --> N["the new riff mcp: serve_directly with the peer info"]
//!     M --> O["the new riff mcp: serve_directly with no peer info"]
//! ```
//!
//! A `riff mcp` that runs a replaced riff for longer than
//! [`crate::binary::STUCK_AFTER`] cannot start again by itself, for
//! example a `riff mcp` of v1.7.0 or older with Claude Code. The hook of
//! the session then tells the session the fix (see
//! [`crate::binary::stuck_mcp`], 01M53QQRAJMB24B0804YHPMN18).
//!
//! # The tokens of a claim
//!
//! A granted `claim` records the start time of the claim on this
//! machine. A `release` of the claim of the session, and a `leave`, end
//! it, sum its tokens, and put the sum on the issue of the claim
//! (01M3Y1YP1ZA5TBRA01MKWM3VC6). The result of the tool has one line
//! with the sum. The sum never fails the tool. See [`crate::usage`].
//!
//! # The step of the lead
//!
//! A person sees in `riff who` what the lead does, with no `status`
//! call of the lead (01M3W8AYDFPZNZ898WAJS7JEZA). After each `tell`,
//! `post`, `pause`, `resume` or `lead` call that the server accepts,
//! the tools set the step of the lead from that call:
//!
//! ```mermaid
//! flowchart LR
//!     C["tell, post, pause, resume or lead"] --> O{"the call is OK?"}
//!     O -- yes --> L{"this session is the lead?"}
//!     L -- yes --> S["status: the step of the call"]
//!     O -- no --> N["no change"]
//!     L -- no --> N
//! ```
//!
//! | Call | Step |
//! |---|---|
//! | `tell` | `told 075ff6a7` |
//! | `post` | `posted a note: Waves: new item #314`, `posted a message: …` or `asked for status` |
//! | `pause`, `resume` | `paused the repository`, `resumed the repository`; with `riff` true, `paused the riff`, `resumed the riff` |
//! | `lead` | `became the lead` |
//!
//! Each member of the riff reads the step, and a direct thread is
//! private to its two sessions. So the step of a `tell` has no text of
//! the message (01M3WKCYM623M66ATHCH3QGMKP). A `post` cannot go to a
//! direct thread: the server refuses it. The text of a post is one line
//! with no control character.
//!
//! The step is a status like each other one: a later `status` call of
//! the lead replaces it, and the next of these calls replaces that. The
//! words come from [`text::told_step`], [`text::posted_step`],
//! [`text::riff_step`] and [`text::LEAD_STEP`].
//!
//! # A session in a container
//!
//! The daemon serves the tools of a session of `riff run`
//! ([`crate::daemon`]). The daemon runs on the host, outside the
//! container. So that session gets only the tools of
//! [`CONTAINER_TOOLS`]: they only talk, and act on no folder of the host
//! and no claim (01M4ZV3B6T2KB09WJ59F07HXET). They act on the
//! riff-server of the session. A session with no riff-server talks on
//! the riff of peers of the daemon ([`crate::peer::talk`],
//! 01M51VZG27DXSRHBT8M14S7B9A): [`Tools::with_talk`]. `riff run` does the
//! claim, the release and the verify request. The tools do not list each
//! other tool, and refuse a call of it with [`text::not_in_a_container`].
//! The instructions of the server are [`text::MCP_CONTAINER`].
//!
//! The lead in a container ([`crate::lead`]) also gets the tools of a
//! lead that only talk: [`LEAD_TOOLS`]. Its instructions
//! are [`text::MCP_LEAD`]: they say how the lead gets its wakes, and
//! the steps of the waves.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{ALIVE_EVERY, Kind, RiffState, Status, WORKER_ALIVE_EVERY};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::api::{Api, PauseScope};
use crate::binary::{Follow, with_last, with_place};
use crate::talk::Talk;
use crate::usage::{self, Meter};
use crate::{dropped, identity, leave, relay, text};

/// The hidden option that gives a new `riff mcp` the client of its
/// connection after an update (01M3NT6WZTKAFKGDWGCFKC8TB5): the
/// initialize request as JSON, or [`MODERN`] (see [`Client`]).
pub const CLIENT: &str = "--client";

/// The value of [`CLIENT`] for a client that sent no initialize request
/// (01M53QQR7J7DG86SMAERKMYSAF).
pub const MODERN: &str = "modern";

#[derive(Clone)]
pub struct Tools {
    api: Api,
    /// The calls of the tools that talk ([`crate::talk`]): the riff of
    /// peers of the daemon for a session of the daemon. `None` talks on
    /// riff-server with [`Tools::api`].
    talk: Option<Arc<dyn Talk>>,
    me: Arc<Mutex<SessionUri>>,
    /// Where the session works: the directory of the WIP push of `leave`,
    /// and of the look for earlier work at a claim.
    dir: Arc<Mutex<PathBuf>>,
    /// True when a granted claim looks for the earlier work on its item
    /// (01M3WFYER9QWA698KY2E1HNTCW).
    earlier: bool,
    /// True in a worker session: each register says so
    /// (01M3NT4M159EHN5W8JRTQ417N4).
    worker: bool,
    /// The process ID of the `riff workers run` wrapper of a worker. The
    /// tools stop it when the server asks (01M3Q5A0QZTSTXHHNYCE8HFJSB).
    wrapper: Option<u32>,
    /// What counts the tokens of each claim of the session
    /// ([`crate::usage`]). `None` counts nothing.
    meter: Option<Arc<Meter>>,
    /// Where the session runs: on the host, it gets each tool. In a
    /// container, it gets only the tools of [`CONTAINER_TOOLS`]
    /// (01M4ZV3B6T2KB09WJ59F07HXET), or of [`LEAD_TOOLS`] for the lead.
    reach: Reach,
}

/// Where a session runs, for its tools.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// On the host: each tool.
    Host,
    /// A worker or a verifier in a container: [`CONTAINER_TOOLS`].
    Container,
    /// The lead in a container: [`LEAD_TOOLS`].
    Lead,
}

/// The tools of a session in a container (01M4ZV3B6T2KB09WJ59F07HXET).
/// Each only talks, and acts on no folder of the host and no claim.
pub const CONTAINER_TOOLS: [&str; 8] = [
    "blocked", "post", "read", "status", "tell", "threads", "who", "whoami",
];

/// The tools of the lead in a container: the tools of
/// [`CONTAINER_TOOLS`], and the tools of a lead that only talk.
pub const LEAD_TOOLS: [&str; 13] = [
    "blocked", "free", "hold", "lead", "pause", "post", "read", "resume", "status", "tell",
    "threads", "who", "whoami",
];

#[derive(Deserialize, JsonSchema)]
pub struct ThreadArg {
    /// The thread. Leave it out to use your repository thread.
    thread: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PostArgs {
    /// The thread. Leave it out to use your repository thread.
    thread: Option<String>,
    /// The sessions to wake. A session wakes when it matches one or more
    /// selectors. Leave it out to wake nobody. Text in the body never wakes.
    to: Option<Vec<Selector>>,
    /// The message.
    body: String,
    /// `message` (the default), `status` for a status request, or `note`.
    /// Each session that a status request wakes sets its status with the
    /// `status` tool. A note wakes no session: the sessions that `to`
    /// selects see it at their next `read`. Use it for a board, a
    /// "started" or a "done".
    kind: Option<Kind>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PauseArgs {
    /// True names the whole riff: each repository. Only a lead whose
    /// user is the owner or an admin can. Leave it out to name only
    /// your repository.
    riff: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct StatusArgs {
    /// Your current step, in one short line.
    step: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct BlockedArgs {
    /// Why you cannot go on, in one short line.
    reason: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct WhoArgs {
    /// True lists gone sessions too.
    all: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ReadArgs {
    /// The thread. Leave it out to read the unread messages of all your threads.
    thread: Option<String>,
    /// True returns the full history, not only the unread messages.
    all: Option<bool>,
    /// With all: the page after this message number. A page that has
    /// more messages after it names the number.
    after: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct TellArgs {
    /// The session: its session ID or the start of it, as `read` shows
    /// it, its full riff:// URI from `who`, or `lead` for the lead of
    /// your user in your repository.
    session: String,
    /// The message.
    body: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ClaimArgs {
    /// The thread. Leave it out to use your repository thread.
    thread: Option<String>,
    /// The work item, for example issue-12.
    item: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct HoldArgs {
    /// The repository thread. Leave it out to use your repository
    /// thread.
    thread: Option<String>,
    /// The work item, for example issue-12.
    item: String,
    /// Why the item is held, in 1 to 200 characters. A worker that
    /// claims the item gets it.
    reason: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct FreeArgs {
    /// The repository thread. Leave it out to use your repository
    /// thread.
    thread: Option<String>,
    /// The work item, for example issue-12.
    item: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ReleaseArgs {
    /// The thread. Leave it out to use your repository thread.
    thread: Option<String>,
    /// The work item, for example issue-12.
    item: String,
    /// Only the lead: the session that holds the item, by its session ID
    /// or the start of it. Leave it out to release your own claim.
    session: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct MoveArgs {
    /// The absolute path of the directory where you work now, for example a new worktree.
    path: String,
}

type ToolResult = Result<String, String>;

#[tool_router]
impl Tools {
    /// The tools of the session `me`, which works in the current
    /// directory. The client `api` holds the mark of a leave
    /// ([`Api::for_session`]). With no mark, the `leave` tool refuses.
    pub fn new(api: Api, me: SessionUri) -> Self {
        Self {
            api,
            talk: None,
            me: Arc::new(Mutex::new(me)),
            dir: Arc::new(Mutex::new(std::env::current_dir().unwrap_or_default())),
            earlier: false,
            worker: false,
            wrapper: None,
            meter: None,
            reach: Reach::Host,
        }
    }

    /// The tools that talk call `talk`, not riff-server: the tools of a
    /// session of the riff daemon (01M519Z3FTAZ20DQKDTW09M6NS).
    pub fn with_talk(mut self, talk: Arc<dyn Talk>) -> Self {
        self.talk = Some(talk);
        self
    }

    /// The calls of the tools that talk.
    fn talk(&self) -> &dyn Talk {
        match &self.talk {
            Some(talk) => talk.as_ref(),
            None => &self.api,
        }
    }

    /// Gives the session only the tools of [`CONTAINER_TOOLS`]: it runs
    /// in a container, and the tools run on the host
    /// (01M4ZV3B6T2KB09WJ59F07HXET).
    pub fn in_container(mut self) -> Self {
        self.reach = Reach::Container;
        self
    }

    /// Gives the session only the tools of [`LEAD_TOOLS`]: the lead in a
    /// container (01M4Z60B81RTZT71PN5HV0GZHX).
    pub fn in_lead_container(mut self) -> Self {
        self.reach = Reach::Lead;
        self
    }

    /// True when the session gets the tool `name`.
    fn serves(&self, name: &str) -> bool {
        match self.reach {
            Reach::Host => true,
            Reach::Container => CONTAINER_TOOLS.contains(&name),
            Reach::Lead => LEAD_TOOLS.contains(&name),
        }
    }

    /// The tools of the session: each tool, or in a container only the
    /// tools of [`CONTAINER_TOOLS`].
    fn router(&self) -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        let mut router = Self::tool_router();
        for tool in router.list_all() {
            if !self.serves(&tool.name) {
                router.remove_route(&tool.name);
            }
        }
        router
    }

    /// Counts the tokens of each claim of the session with `meter`
    /// (01M3Y1YP1ZA5TBRA01MKWM3VC6). `riff mcp` turns it on with the
    /// meter of the machine, and a test only with a directory and a `gh`
    /// of its own.
    pub fn with_meter(mut self, meter: Option<Meter>) -> Self {
        self.meter = meter.map(Arc::new);
        self
    }

    /// Stops the process `wrapper`, the `riff workers run` of this
    /// worker, when the server asks this idle worker to stop
    /// (01M3Q5A0QZTSTXHHNYCE8HFJSB).
    pub fn in_wrapper(mut self, wrapper: Option<u32>) -> Self {
        self.wrapper = wrapper;
        self
    }

    /// Registers the session as a worker or not
    /// (01M3NT4M159EHN5W8JRTQ417N4).
    pub fn as_worker(mut self, worker: bool) -> Self {
        self.worker = worker;
        self
    }

    /// Keeps the mark of a leave of the session in `local`
    /// ([`Api::for_session`]). When the mark is there, the session left
    /// before, for example before a resume. `riff mcp` gets a client
    /// with the mark from [`Api::signed_in`], so only a test calls it.
    pub fn in_local(mut self, local: &std::path::Path) -> Self {
        if let Some(id) = self.me().who().session() {
            self.api = self.api.for_session(local, id);
        }
        self
    }

    /// Sets the directory where the session works.
    pub fn in_dir(self, dir: PathBuf) -> Self {
        *self.dir.lock().unwrap_or_else(|p| p.into_inner()) = dir;
        self
    }

    /// Makes a granted claim name the earlier work on its item in the
    /// clone where the session works (01M3WFYER9QWA698KY2E1HNTCW). It
    /// fetches from `origin`, so `riff mcp` turns it on, and a test
    /// only for a clone of its own.
    pub fn with_earlier_work(mut self) -> Self {
        self.earlier = true;
        self
    }

    /// True while the session is out of the riff: its mark is there
    /// ([`Api::left`]).
    pub fn left(&self) -> bool {
        self.api.left()
    }

    #[tool(
        description = "Show the URI of this session: who you are, where you work, what you hold. \
Show the state of the riff: running, or which pause stops you (the pause of the whole riff or of \
your repository) and who set it."
    )]
    async fn whoami(&self) -> ToolResult {
        let me = self.here()?;
        let now = self
            .talk()
            .who(&me, true)
            .await
            .ok()
            .and_then(|list| list.into_iter().find(|s| same(&s.uri, &me)))
            .map_or(me.clone(), |s| s.uri);
        let state = match self.talk().pauses(&me).await {
            Ok(pauses) => format!(
                "{}\n{}",
                text::riff_state(&pauses, now.default_thread().as_ref()),
                text::build_line(crate::link::server_build().as_ref())
            ),
            Err(e) => format!("riff cannot read the state of the riff: {e:#}"),
        };
        Ok(format!("{}\n{now}\n{state}", text::name(&now)))
    }

    #[tool(
        description = "List the sessions in the riff with their URIs. Show which are live, how long each other session is idle, and the status of each session with its age. A session that ended, or stopped for 3 minutes, is gone and not listed."
    )]
    async fn who(&self, Parameters(a): Parameters<WhoArgs>) -> ToolResult {
        let me = self.here()?;
        let all = a.all.unwrap_or(false);
        let pauses = self.talk().pauses(&me).await.map_err(err)?;
        let mut who = self.talk().roster(&me, all).await.map_err(err)?;
        crate::state::fill(&mut who.sessions, pauses.state);
        let owner = text::owner_line(&who.owner)
            .map(|line| format!("{line}\n"))
            .unwrap_or_default();
        Ok(format!(
            "{}\n{owner}{}\n{}",
            text::riff_state(&pauses, me.default_thread().as_ref()),
            text::build_line(crate::link::server_build().as_ref()),
            text::who(&who.sessions, &who.owner, &me)
        ))
    }

    #[tool(description = "List your threads with their unread counts.")]
    async fn threads(&self) -> ToolResult {
        let me = self.here()?;
        let list = self.talk().threads(&me).await.map_err(err)?;
        Ok(text::threads(&list, &me))
    }

    #[tool(description = "Join a thread.")]
    async fn join_thread(&self, Parameters(a): Parameters<ThreadArg>) -> ToolResult {
        let me = self.here()?;
        let thread = self.thread(a.thread)?;
        self.api.join(&me, &thread).await.map_err(err)?;
        Ok(format!("You joined {thread}."))
    }

    #[tool(description = "Leave a thread.")]
    async fn leave_thread(&self, Parameters(a): Parameters<ThreadArg>) -> ToolResult {
        let me = self.here()?;
        let thread = self.thread(a.thread)?;
        self.api.leave(&me, &thread).await.map_err(err)?;
        Ok(format!("You left {thread}."))
    }

    #[tool(
        description = "Leave the riff: this session only. Call it when your user runs /riff:leave \
or says \"leave the riff\". When you hold a claim, it commits each change of your worktree as a WIP \
commit and pushes the branch. Then it frees your claims, and you leave `who`. Each riff tool except \
`join` then refuses."
    )]
    async fn leave(&self) -> ToolResult {
        let me = self.here()?;
        let claims: Vec<String> = self
            .api
            .who(&me, false)
            .await
            .map_err(err)?
            .into_iter()
            .find(|s| s.uri.who() == me.who())
            .map(|s| s.uri.claims().to_vec())
            .unwrap_or_default();
        let wip = if claims.is_empty() {
            None
        } else {
            let dir = self.dir.lock().unwrap_or_else(|p| p.into_inner()).clone();
            Some(leave::wip(&dir).map_err(|e| format!("{}{e:#}", text::LEAVE_REFUSED))?)
        };
        self.api
            .leave_riff(&me)
            .await
            .map_err(|e| format!("{}{e:#}", text::LEAVE_FAILED))?;
        let mut out = text::left(wip.as_deref(), &claims);
        let counted = self
            .count(&me, |meter, id| meter.release_all(id, usage::now_ms()))
            .await;
        for line in counted.unwrap_or_default() {
            out.push('\n');
            out.push_str(&line);
        }
        Ok(out)
    }

    #[tool(
        description = "Join the riff again after a leave. Call it when your user runs /riff:join \
or says \"join the riff\". Then start the watch and follow the start routine of the riff skill."
    )]
    async fn join(&self) -> ToolResult {
        let me = self.me();
        self.api.join_riff(&me, self.worker).await.map_err(err)?;
        Ok(text::joined(&me))
    }

    #[tool(
        description = "Post a message to a thread. Only the sessions that `to` selects wake. A selector \
names fields (user, session, host, repo, worktree, claim, lead); a session matches when each named field \
matches."
    )]
    async fn post(&self, Parameters(a): Parameters<PostArgs>) -> ToolResult {
        let thread = self.thread(a.thread)?;
        let to = a.to.unwrap_or_default();
        let kind = a.kind.unwrap_or_default();
        let me = self.here()?;
        let posted = self
            .talk()
            .post(&me, Some(&thread), &to, &a.body, kind)
            .await
            .map_err(err)?;
        self.lead_step(&me, text::posted_step(kind, &a.body)).await;
        Ok(text::posted(&posted))
    }

    #[tool(
        description = "Set your status: your current step, in your own words. They help a person. \
riff makes your state from facts by itself: your claims, your work, a verify or a merge that you \
wait for, a pause. `who` shows the state, then your words with their age. When a status request \
wakes you, answer with this tool. Do not post a reply. When you cannot go on, use `blocked`."
    )]
    async fn status(&self, Parameters(a): Parameters<StatusArgs>) -> ToolResult {
        let status = Status { step: a.step };
        self.talk()
            .status(&self.here()?, &status)
            .await
            .map_err(err)?;
        Ok(text::status_set(&status))
    }

    #[tool(
        description = "Say that you cannot go on with no decision of a person. One call does both: \
riff shows you as blocked, and the message `blocked: REASON` wakes the lead of your user. The block \
ends at your next work after an answer. Do not use it to wait for a verify, a merge or a need: riff \
shows that wait by itself. When you are the lead, it sends no message: riff shows you as waiting for \
your user, and the next prompt of your user ends it."
    )]
    async fn blocked(&self, Parameters(a): Parameters<BlockedArgs>) -> ToolResult {
        let told = self
            .talk()
            .blocked(&self.here()?, &a.reason)
            .await
            .map_err(err)?;
        Ok(text::blocked_set(&a.reason, told))
    }

    #[tool(
        description = "Send a direct message to one session. It wakes that session. Use the session \
`lead` to ask the lead of your user in your repository."
    )]
    async fn tell(&self, Parameters(a): Parameters<TellArgs>) -> ToolResult {
        let me = self.here()?;
        let posted = self
            .talk()
            .tell(&me, &a.session, &a.body)
            .await
            .map_err(err)?;
        let to = posted
            .woken
            .first()
            .and_then(|s| s.who().session())
            .unwrap_or(&a.session);
        self.lead_step(&me, text::told_step(to)).await;
        Ok(text::posted(&posted))
    }

    #[tool(description = "Read unread messages. Leave out the thread to read all your threads.")]
    async fn read(&self, Parameters(a): Parameters<ReadArgs>) -> ToolResult {
        let me = self.here()?;
        let thread = a.thread.map(|t| self.thread(Some(t))).transpose()?;
        let inbox = self
            .talk()
            .inbox_page(&me, thread.as_ref(), a.all.unwrap_or(false), a.after)
            .await
            .map_err(err)?;
        Ok(text::inbox(&inbox, &me))
    }

    #[tool(
        description = "Claim a work item so that no other session does the same work. The result \
names the pushed branch and the worktree of an earlier session on the item, when there is one, \
and the pull request of the item with the state of its verify."
    )]
    async fn claim(&self, Parameters(a): Parameters<ClaimArgs>) -> ToolResult {
        let thread = self.thread(a.thread)?;
        let me = self.here()?;
        let reply = self.api.claim(&me, &thread, &a.item).await.map_err(err)?;
        let mut out = text::claimed(&reply, &thread, &a.item);
        if reply.granted {
            let (thread, item) = (thread.to_string(), a.item.clone());
            self.count(&me, move |meter, id| meter.started(id, &thread, &item))
                .await;
        }
        if reply.granted && self.earlier {
            let dir = self.dir.lock().unwrap_or_else(|p| p.into_inner()).clone();
            if let Some(line) = dropped::at_claim(&dir, &a.item).await {
                out.push('\n');
                out.push_str(&line);
            }
            let repo = me.place().repo_text();
            if let Some(line) = crate::rollout::at_claim(&dir, &repo, &a.item).await {
                out.push('\n');
                out.push_str(&line);
            }
        }
        Ok(out)
    }

    #[tool(
        description = "Release a work item that you claimed. Only the lead: with `session`, free \
the claim of another session of your user, for example one that is gone or that does not answer."
    )]
    async fn release(&self, Parameters(a): Parameters<ReleaseArgs>) -> ToolResult {
        let thread = self.thread(a.thread)?;
        let me = self.here()?;
        match a.session {
            Some(holder) => {
                self.api
                    .release_for(&me, &thread, &a.item, &holder)
                    .await
                    .map_err(err)?;
                Ok(text::released_for(&thread, &a.item, &holder))
            }
            None => {
                let reply = self.api.release(&me, &thread, &a.item).await.map_err(err)?;
                let mut out = text::released(&thread, &a.item, reply);
                let (thread, item) = (thread.to_string(), a.item);
                let counted = self
                    .count(&me, move |meter, id| meter.release(id, &thread, &item))
                    .await;
                if let Some(line) = counted.flatten() {
                    out.push('\n');
                    out.push_str(&line);
                }
                Ok(out)
            }
        }
    }

    #[tool(
        description = "Only the lead: hold a work item with a reason, so that no worker claims \
it. Use it in place of a claim to keep an item from the workers. A hold is not a claim, and it \
does not end a claim. Each other session can still claim the item, with a warning."
    )]
    async fn hold(&self, Parameters(a): Parameters<HoldArgs>) -> ToolResult {
        let thread = self.thread(a.thread)?;
        let me = self.here()?;
        let reply = self
            .talk()
            .hold(&me, &thread, &a.item, &a.reason)
            .await
            .map_err(err)?;
        self.lead_step(&me, format!("held {}", a.item)).await;
        Ok(text::held(&reply, &thread, &a.item))
    }

    #[tool(
        description = "Only the lead: free a held work item, so that a worker can claim it again."
    )]
    async fn free(&self, Parameters(a): Parameters<FreeArgs>) -> ToolResult {
        let thread = self.thread(a.thread)?;
        let me = self.here()?;
        let reply = self.talk().free(&me, &thread, &a.item).await.map_err(err)?;
        self.lead_step(&me, format!("freed {}", a.item)).await;
        Ok(text::freed(reply, &thread, &a.item))
    }

    #[tool(
        description = "Make this session the lead of your user in your repository. The other \
sessions of your user send their questions to the lead. It replaces the old lead. Call it only when \
your user says so."
    )]
    async fn lead(&self) -> ToolResult {
        let me = self.here()?;
        let reply = self.talk().lead(&me).await.map_err(err)?;
        self.lead_step(&me, text::LEAD_STEP.into()).await;
        Ok(text::led(&reply))
    }

    #[tool(
        description = "Pause your repository. Each session of it stops at its next step and \
waits. The other repositories go on. With `riff` true, pause the whole riff. Only the lead can. \
Call it only when your user says so."
    )]
    async fn pause(&self, Parameters(a): Parameters<PauseArgs>) -> ToolResult {
        self.set_riff(a.riff.unwrap_or(false), RiffState::Paused)
            .await
    }

    #[tool(
        description = "Resume your repository. Each session of it goes on from where it \
stopped. With `riff` true, resume the whole riff. Only the lead can. Call it only when your \
user says so."
    )]
    async fn resume(&self, Parameters(a): Parameters<PauseArgs>) -> ToolResult {
        self.set_riff(a.riff.unwrap_or(false), RiffState::Running)
            .await
    }

    #[tool(
        name = "move",
        description = "Tell riff that you work in a new directory, for example a new worktree. \
Your session ID and your claims stay. Call it each time you change worktree."
    )]
    async fn move_to(&self, Parameters(a): Parameters<MoveArgs>) -> ToolResult {
        let me = self.here()?;
        let path = PathBuf::from(&a.path);
        if !path.is_dir() {
            return Err(format!("{} is not a directory", a.path));
        }
        let place = identity::place(&path).map_err(err)?;
        let moved = me.moved(place);
        self.api
            .register_as(&moved, self.worker)
            .await
            .map_err(err)?;
        *self.me.lock().unwrap_or_else(|p| p.into_inner()) = moved.clone();
        *self.dir.lock().unwrap_or_else(|p| p.into_inner()) = path;
        // The URI of the server holds the claims and the lead
        // (01M517N7ANC06S4GTEEZHAB255).
        let shown = match self.api.me(&moved).await {
            Ok(reply) => reply.session.map_or(moved, |s| s.uri),
            Err(_) => moved,
        };
        Ok(format!("You moved. Your URI is now {shown}"))
    }
}

#[tool_handler(router = self.router())]
impl ServerHandler for Tools {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        let instructions = match self.reach {
            Reach::Host => text::MCP_TOOLS,
            Reach::Container => text::MCP_CONTAINER,
            Reach::Lead => text::MCP_LEAD,
        };
        rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_server_info(rmcp::model::Implementation::from_build_env())
        .with_instructions(instructions.to_owned())
    }

    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, rmcp::ErrorData> {
        if !self.serves(&request.name) {
            let refusal = match self.reach {
                Reach::Lead => text::not_in_the_lead_container(&request.name),
                _ => text::not_in_a_container(&request.name),
            };
            return Ok(
                rmcp::model::CallToolResult::error(vec![rmcp::model::ContentBlock::text(refusal)])
                    .into(),
            );
        }
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.router().call(call).await
    }
}

/// The server of `riff mcp` with no tool. Its instructions say why: in
/// a directory where riff is off, they name `riff enable`
/// (01M3XY2ST8R67SKTXJECAYJZRX).
struct Off(String);

impl ServerHandler for Off {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        let mut info = rmcp::model::ServerConfig::default();
        info.instructions = Some(self.0.clone());
        info
    }
}

/// `riff mcp` in a directory where riff is off. It serves a server with
/// no tool, whose instructions are [`text::MCP_OFF`], over stdio until
/// the agent tool closes the stream. It makes no call to
/// `riff-server`, and keeps no file of the session.
pub async fn serve_off() -> Result<()> {
    serve_no_tools(text::MCP_OFF.into()).await
}

/// `riff mcp` of an agent that runs inside the riff session `outer`
/// (01M517N75C0Z5BAFQAWRN3YTYB). As [`serve_off`], with the
/// instructions [`crate::hook::nested_line`]. It registers no session,
/// and sends no keep-alive and no end call.
pub async fn serve_nested(outer: &str) -> Result<()> {
    serve_no_tools(crate::hook::nested_line(outer)).await
}

async fn serve_no_tools(instructions: String) -> Result<()> {
    let service = Off(instructions).serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

impl Tools {
    /// The arguments of the new binary after an update: the arguments of
    /// this process, with the place of the session and `client`
    /// (01M3NT6WZTKAFKGDWGCFKC8TB5).
    fn args(&self, client: &str) -> Vec<std::ffi::OsString> {
        let args = with_place(std::env::args_os().skip(1), self.me().place());
        with_last(args, CLIENT, client)
    }

    /// Pauses or resumes the repository of the session, or with
    /// `whole` the whole riff (01M3XAHZBGSSJB3YX23K88W01K).
    async fn set_riff(&self, whole: bool, state: RiffState) -> ToolResult {
        let me = self.here()?;
        let scope = PauseScope::of(whole, None);
        let (reply, posted) = self
            .talk()
            .set_pause(&me, &scope, state)
            .await
            .map_err(err)?;
        self.lead_step(&me, text::riff_step(whole, state).into())
            .await;
        let repository = scope.repository(&me);
        Ok(text::riff_set(repository.as_ref(), state, &reply, &posted))
    }

    /// Sets the step of `me` to `step`, when `me` is the lead: the
    /// automatic step of the call that the lead made
    /// (01M3W8AYDFPZNZ898WAJS7JEZA). The step of each other session
    /// stays. A failure is not reported: the call of the lead is done.
    async fn lead_step(&self, me: &SessionUri, step: String) {
        let Ok(list) = self.talk().who(me, false).await else {
            return;
        };
        let lead = list.into_iter().find(|s| same(&s.uri, me) && s.uri.lead());
        if lead.is_some() {
            let _ = self.talk().status(me, &Status { step }).await;
        }
    }

    /// Runs `count` with the meter and the session ID of `me`, off the
    /// runtime: it reads the transcripts, and it can run `gh`. `None`
    /// with no meter.
    async fn count<T: Send + 'static>(
        &self,
        me: &SessionUri,
        count: impl FnOnce(&Meter, &str) -> T + Send + 'static,
    ) -> Option<T> {
        let meter = self.meter.clone()?;
        let id = me.who().session()?.to_owned();
        tokio::task::spawn_blocking(move || count(&meter, &id))
            .await
            .ok()
    }

    fn me(&self) -> SessionUri {
        self.me.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// The session, or the refusal when it left the riff
    /// (01M3MEEFETT9A0DRWBKQTG77Z2).
    fn here(&self) -> Result<SessionUri, String> {
        if self.left() {
            return Err(text::LEFT.into());
        }
        Ok(self.me())
    }

    fn thread(&self, given: Option<String>) -> Result<ThreadName, String> {
        match given {
            Some(t) => t.parse().map_err(err),
            None => self
                .me()
                .default_thread()
                .ok_or_else(|| "name a thread: this session is not in a git repository".into()),
        }
    }
}

/// True when `a` and `b` are one session: the same who, or the same
/// session ID. In the riff of peers, a session of the daemon has the
/// user of the key of the daemon ([`crate::peer::talk`]).
fn same(a: &SessionUri, b: &SessionUri) -> bool {
    a.who() == b.who() || (a.who().session().is_some() && a.who().session() == b.who().session())
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

impl Tools {
    /// Sends a keep-alive each [`ALIVE_EVERY`] for as long as the tools
    /// run (R204), in a worker each [`WORKER_ALIVE_EVERY`]. A failed
    /// keep-alive is not reported: the next one tries again. When the
    /// reply asks this idle worker to stop, it stops its wrapper
    /// (01M3Q5A0QZTSTXHHNYCE8HFJSB).
    pub fn keep_alive(&self) -> tokio::task::JoinHandle<()> {
        self.keep_alive_every(if self.worker {
            WORKER_ALIVE_EVERY
        } else {
            ALIVE_EVERY
        })
    }

    /// [`Tools::keep_alive`] with another period, for tests.
    pub fn keep_alive_every(&self, every: std::time::Duration) -> tokio::task::JoinHandle<()> {
        let tools = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            tick.tick().await;
            loop {
                tick.tick().await;
                if tools.left() {
                    continue;
                }
                // A hung request must not stop the next keep-alive.
                let me = tools.me();
                let (activity, prompt_secs) = me
                    .who()
                    .session()
                    .zip(crate::local::dir())
                    .map(|(id, dir)| {
                        let now_ms = crate::activity::now_ms();
                        (
                            crate::activity::read(&dir, id, now_ms),
                            crate::activity::prompt_secs(&dir, id, now_ms),
                        )
                    })
                    .unwrap_or_default();
                let alive = tools.api.alive_with(&me, activity, prompt_secs);
                let reply = tokio::time::timeout(every, alive).await;
                if let Ok(Ok(reply)) = reply
                    && reply.stop
                {
                    tools.stop_wrapper();
                }
            }
        })
    }

    /// Sends SIGTERM to the wrapper of this worker. The wrapper stops
    /// `claude`; then the input of the tools closes, and they end the
    /// session (01M3Q5A0QZTSTXHHNYCE8HFJSB).
    fn stop_wrapper(&self) {
        let Some(pid) = self.wrapper.filter(|_| self.worker) else {
            eprintln!("riff: {}", crate::text::IDLE_STOP_NO_WRAPPER);
            return;
        };
        eprintln!("riff: {}", crate::text::IDLE_STOP);
        crate::worker::stop_wrapper(pid);
    }

    /// Runs the rollout of workers for as long as the tools run
    /// (01M3Q5QE01DB0FJQJWFKR450KQ). It acts only while this session is
    /// the lead. See [`crate::rollout`].
    pub fn rollout(&self) -> tokio::task::JoinHandle<()> {
        let tools = self.clone();
        let env = crate::rollout::Live {
            api: self.api.clone(),
            me: move || tools.me(),
            tmux: crate::terminal::Tmux::from_env(),
            claude: "claude".into(),
            gh: Arc::new(crate::pr::Gh::default()),
            off_told: false.into(),
        };
        tokio::spawn(crate::rollout::run(env))
    }

    /// Runs the look of the lead for as long as the tools run: the facts
    /// of the forge, and the blocks with no answer. It acts only while
    /// this session is the lead. See [`crate::look`].
    pub fn look(&self) -> tokio::task::JoinHandle<()> {
        let tools = self.clone();
        tokio::spawn(crate::look::run(self.api.clone(), move || tools.me(), None))
    }

    /// Looks at the worker panes of this machine each
    /// [`crate::reap::EVERY`], for as long as the tools run. While this
    /// session is the lead, it ends the session of a worker whose pane
    /// is gone, and posts a note to the lead
    /// (01M3WG2460P4GF7GEVBY92Q33W). So the claims of a killed worker
    /// are free at once, and the rollout starts a worker for the item.
    /// A session outside tmux looks at nothing. See [`crate::reap`].
    pub fn reap(&self) -> tokio::task::JoinHandle<()> {
        use crate::reap::{self, Reaper};
        let tools = self.clone();
        let tmux = crate::terminal::Tmux::from_env();
        tokio::spawn(async move {
            let Some(tmux) = tmux else { return };
            let mut reaper = Reaper::default();
            let mut tick = tokio::time::interval(reap::EVERY);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                // tmux and /proc: keep them off the runtime.
                let looked = {
                    let tmux = tmux.clone();
                    tokio::task::spawn_blocking(move || {
                        let lost = reap::lost(&mut reaper, &tmux);
                        (reaper, lost)
                    })
                    .await
                };
                let Ok((kept, lost)) = looked else { return };
                reaper = kept;
                if lost.is_empty() || tools.left() {
                    continue;
                }
                let me = tools.me();
                let sessions = match tools.api.who(&me, false).await {
                    Ok(sessions) => sessions,
                    Err(e) => {
                        eprintln!("riff: cannot end the session of a lost worker: {e:#}");
                        continue;
                    }
                };
                let lead = sessions
                    .iter()
                    .any(|s| s.uri.who() == me.who() && s.uri.lead());
                if !lead {
                    continue;
                }
                let reaped = reap::reap(
                    &tools.api,
                    &me,
                    &sessions,
                    &lost,
                    reap::journal,
                    crate::deaths::record_here,
                )
                .await;
                for note in &reaped.notes {
                    eprintln!("riff: {note}");
                    if let Err(e) = crate::rollout::note_lead(tools.api.base(), &me, note).await {
                        eprintln!("riff: cannot post the note of a lost worker: {e:#}");
                    }
                }
                if let Some(alarm) = &reaped.alarm {
                    eprintln!("riff: {alarm}");
                    if let Err(e) = crate::rollout::message_lead(tools.api.base(), &me, alarm).await
                    {
                        eprintln!("riff: cannot tell the lead: {e:#}");
                    }
                }
            }
        })
    }

    /// Tidies the clone of this machine each [`crate::tidy::every`], for
    /// as long as the tools run, while this session is the lead
    /// (01M41A118QPQKFAAHGQFFX4F3B): the worktrees of merged pull
    /// requests and the disk. See [`crate::tidy`].
    pub fn tidy(&self) -> tokio::task::JoinHandle<()> {
        let tools = self.clone();
        tokio::spawn(async move {
            let mut timer = crate::tidy::timer();
            let mut guard = crate::tidy::Guard::default();
            loop {
                timer.tick().await;
                if tools.left() {
                    continue;
                }
                let me = tools.me();
                let lead = match tools.api.who(&me, false).await {
                    Ok(sessions) => sessions
                        .iter()
                        .any(|s| s.uri.who() == me.who() && s.uri.lead()),
                    Err(_) => false,
                };
                if !lead {
                    continue;
                }
                let Ok(dir) = crate::identity::working_dir() else {
                    continue;
                };
                if let Err(e) =
                    crate::tidy::tidy_as_person(&dir, tools.api.base(), &mut guard).await
                {
                    eprintln!("riff: cannot tidy the worktrees: {e:#}");
                }
            }
        })
    }

    /// Runs the monitor of this machine for as long as the tools run,
    /// while this session is the lead (01M421QPKWPX00X24F8V6DT8Z3). See
    /// [`crate::monitor`]. After each look, also with the monitor off,
    /// it sends the numbers of this machine to the server when its limit
    /// of workers is more than 0 ([`HostStatus::of_this_machine`],
    /// 01M515KSWVGCBS76AWZV74TJQ3). So `riff top` on each machine shows
    /// them, also with no `riff workers host` here.
    ///
    /// [`HostStatus::of_this_machine`]: crate::host::HostStatus::of_this_machine
    pub fn monitor(&self) -> tokio::task::JoinHandle<()> {
        let tools = self.clone();
        tokio::spawn(async move {
            let mut monitor = crate::monitor::Monitor::new();
            loop {
                tokio::time::sleep(crate::monitor::Monitor::every()).await;
                let Ok(path) = crate::settings::path() else {
                    continue;
                };
                let on = crate::settings::monitor(&path).is_ok_and(|m| m.on);
                let runs = crate::settings::workers_limit(&path).is_ok_and(|limit| limit > 0);
                if !(on || runs) || tools.left() {
                    continue;
                }
                let me = tools.me();
                let lead = match tools.api.who(&me, false).await {
                    Ok(sessions) => sessions
                        .iter()
                        .any(|s| s.uri.who() == me.who() && s.uri.lead()),
                    Err(_) => false,
                };
                if !lead {
                    continue;
                }
                if on && let Err(e) = monitor.look(&tools.api, &me).await {
                    eprintln!("riff: the monitor cannot look: {e:#}");
                }
                if runs {
                    tools.send_numbers(&me).await;
                }
            }
        })
    }

    /// Sends the numbers of this machine as `me`
    /// (01M515KSWVGCBS76AWZV74TJQ3). It reads them off the runtime: a
    /// read of tmux and of the disk can block.
    async fn send_numbers(&self, me: &SessionUri) {
        let dir = self.dir.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let read = move || crate::host::HostStatus::of_this_machine(Some(&dir));
        let Ok(Some(numbers)) = tokio::task::spawn_blocking(read).await else {
            return;
        };
        if let Err(e) = self.api.machine(me, &numbers.line()).await {
            eprintln!("riff: cannot send the numbers of this machine: {e:#}");
        }
    }

    /// Tells the server that the session ended (R205). It waits at most
    /// [`END_WAIT`].
    pub async fn end(&self) {
        if self.left() {
            return;
        }
        let me = self.me();
        match tokio::time::timeout(END_WAIT, self.api.end(&me)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => eprintln!("riff: {e:#}"),
            Err(_) => eprintln!("riff: the end call took too long"),
        }
    }
}

/// The hidden flag of the check of a new binary
/// (01M43F5F9AQ9S39E1JZF8EBJEH).
pub const CHECK: &str = "--check";

/// The longest wait for the check of a new binary.
pub const CHECK_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

/// The client of the connection of `riff mcp`, that an old process gives
/// the new one in [`CLIENT`] after an update.
///
/// A client of the MCP protocol `2025-11-25` or older starts with an
/// initialize request. A client of `2026-07-28`, as Claude Code is,
/// starts with `server/discover`, and sends its version, its name and
/// its capabilities with each request. rmcp then knows no peer info. The
/// new process serves such a connection with no peer info too
/// (01M53QQR7J7DG86SMAERKMYSAF).
#[derive(Debug, Clone)]
pub enum Client {
    /// The initialize request of the client.
    Initialized(Box<rmcp::model::InitializeRequestParams>),
    /// A client that sent no initialize request: [`MODERN`].
    Modern,
}

impl Client {
    /// The client of a connection whose peer info is `peer`: the
    /// initialize request, or [`Client::Modern`] with none.
    pub fn of(peer: Option<&rmcp::model::InitializeRequestParams>) -> Client {
        match peer {
            Some(peer) => Client::Initialized(Box::new(peer.clone())),
            None => Client::Modern,
        }
    }

    /// The value of [`CLIENT`] for this client.
    ///
    /// ```
    /// use riff::mcp::{Client, given_client};
    ///
    /// assert_eq!(Client::of(None).arg()?, "modern");
    /// let json = r#"{"protocolVersion":"2025-06-18","capabilities":{},
    ///     "clientInfo":{"name":"claude-code","version":"2"}}"#;
    /// let client = given_client(Some(json))?.expect("a client");
    /// let again = given_client(Some(&client.arg()?))?.expect("a client");
    /// assert_eq!(again.peer().unwrap().client_info.name, "claude-code");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn arg(&self) -> Result<String> {
        Ok(match self {
            Client::Initialized(peer) => serde_json::to_string(peer)?,
            Client::Modern => MODERN.to_owned(),
        })
    }

    /// The peer info of the new process: the initialize request, or
    /// none for [`Client::Modern`].
    pub fn peer(self) -> Option<rmcp::model::InitializeRequestParams> {
        match self {
            Client::Initialized(peer) => Some(*peer),
            Client::Modern => None,
        }
    }
}

/// The client that an old `riff mcp` gave in [`CLIENT`]. `None` with no
/// `arg`. The check of a new binary (01M43F5F9AQ9S39E1JZF8EBJEH) reads
/// it too, so a binary that cannot read it fails the check.
///
/// ```
/// use riff::mcp::{Client, given_client};
///
/// assert!(given_client(None)?.is_none());
/// let json = r#"{"protocolVersion":"2025-06-18","capabilities":{},
///     "clientInfo":{"name":"claude-code","version":"2"}}"#;
/// let client = given_client(Some(json))?.expect("a client");
/// assert_eq!(client.peer().unwrap().client_info.name, "claude-code");
/// assert!(matches!(given_client(Some("modern"))?, Some(Client::Modern)));
/// assert!(given_client(Some("{}")).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn given_client(arg: Option<&str>) -> Result<Option<Client>> {
    Ok(match arg {
        None => None,
        Some(MODERN) => Some(Client::Modern),
        Some(json) => Some(Client::Initialized(serde_json::from_str(json)?)),
    })
}

/// Runs the new binary at `binary` with `args` and [`CHECK`], in `dir`,
/// for at most [`CHECK_WAIT`] (01M43F5F9AQ9S39E1JZF8EBJEH). It serves
/// nothing: stdin is empty. The error is the last line of its stderr.
async fn check(
    binary: std::path::PathBuf,
    mut args: Vec<std::ffi::OsString>,
    dir: PathBuf,
) -> std::result::Result<(), String> {
    args.push(CHECK.into());
    let mut cmd = tokio::process::Command::new(binary);
    cmd.args(args)
        .current_dir(&dir)
        .env("PWD", &dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let out = tokio::time::timeout(CHECK_WAIT, cmd.output())
        .await
        .map_err(|_| text::check_too_long(CHECK_WAIT))?
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(err
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map_or_else(|| out.status.to_string(), str::to_owned))
}

/// The longest wait for the end call. The agent tool stops a slow
/// process.
pub const END_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Serves the tools on stdin and stdout until the session ends. It
/// sends keep-alives while it runs, and the end call when its stdin
/// closes or a signal stops it (R204, R205). On a new binary that
/// passes its check (01M43F5F9AQ9S39E1JZF8EBJEH), it runs it in place,
/// with no end call (01M3NT6WZTKAFKGDWGCFKC8TB5), also for a client
/// with no initialize request (01M53QQR7J7DG86SMAERKMYSAF). With
/// `client`, the [`Client`] of the old process, it skips the
/// handshake. It sends `registered` when its register ends, also when
/// the register fails (01M3XM68N5M5DKB86W5079X2G9). A session that left
/// the riff does not register: it drops `registered` with no value.
pub async fn serve(
    api: Api,
    me: SessionUri,
    client: Option<&str>,
    registered: tokio::sync::oneshot::Sender<()>,
) -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let worker = crate::worker::is_worker();
    let tools = Tools::new(api.clone(), me.clone())
        .with_earlier_work()
        .with_meter(Meter::here())
        .as_worker(worker)
        .in_wrapper(crate::worker::wrapper());
    // Start even if the server is down: each tool call reports the error.
    if tools.left() {
        drop(registered);
    } else {
        if let Err(e) = api.register_as(&me, worker).await {
            eprintln!("riff: {e:#}");
        }
        // The receiver can be gone: nothing waits for the register.
        let _ = registered.send(());
    }
    let alive = tools.keep_alive();
    // A worker is never the lead.
    let monitor = (!worker).then(|| tools.monitor());
    let rollout = (!worker).then(|| tools.rollout());
    let reap = (!worker).then(|| tools.reap());
    let tidy = (!worker).then(|| tools.tidy());
    let look = (!worker).then(|| tools.look());
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let mut hup = signal(SignalKind::hangup())?;
    let follow = Follow::this();
    // An update waits for the end of the handshake: before it, the new
    // process cannot take the connection. A client with no initialize
    // request ends it too (01M53QQR7J7DG86SMAERKMYSAF).
    let ready = tokio::sync::Notify::new();
    let known = Mutex::new(None::<String>);
    let update = async {
        ready.notified().await;
        let Some(client) = known.lock().unwrap_or_else(|p| p.into_inner()).clone() else {
            return std::future::pending().await;
        };
        follow
            .new_one_that(|binary| {
                let args = tools.args(&client);
                let dir = tools.dir.lock().unwrap_or_else(|p| p.into_inner()).clone();
                check(binary, args, dir)
            })
            .await;
    };
    let (outside, inside) = tokio::io::duplex(relay::PIPE);
    let relay = relay::run(outside, update);
    tokio::pin!(relay);
    let given = given_client(client)?;
    let service = async {
        match given {
            Some(client) => Ok(rmcp::service::serve_directly(
                tools.clone(),
                inside,
                client.peer(),
            )),
            None => tools.clone().serve(inside).await,
        }
    };
    // The relay runs while the tools make the handshake. Each way out
    // but an update ends with the end call.
    let mut result = Ok(());
    let service = tokio::select! {
        service = service => service.map_err(|e| result = Err(e.into())).ok(),
        ended = &mut relay => {
            result = ended.map(|_| ()).map_err(Into::into);
            None
        }
        _ = term.recv() => None,
        _ = int.recv() => None,
        _ = hup.recv() => None,
    };
    if let Some(service) = &service {
        let client = Client::of(service.peer().peer_info().as_deref());
        *known.lock().unwrap_or_else(|p| p.into_inner()) = Some(client.arg()?);
    }
    ready.notify_one();
    let ended = match &service {
        Some(_) => tokio::select! {
            ended = &mut relay => ended.map_err(|e| result = Err(e.into())).ok(),
            _ = term.recv() => None,
            _ = int.recv() => None,
            _ = hup.recv() => None,
        },
        None => None,
    };
    let client = known.lock().unwrap_or_else(|p| p.into_inner()).take();
    if ended == Some(relay::Ended::Update)
        && let Some(client) = client
    {
        alive.abort();
        if let Some(rollout) = &rollout {
            rollout.abort();
        }
        if let Some(reap) = &reap {
            reap.abort();
        }
        if let Some(monitor) = &monitor {
            monitor.abort();
        }
        if let Some(tidy) = &tidy {
            tidy.abort();
        }
        if let Some(look) = &look {
            look.abort();
        }
        let dir = tools.dir.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let _ = std::env::set_current_dir(dir);
        follow.run(tools.args(&client));
        anyhow::bail!("riff mcp cannot run the new riff");
    }
    alive.abort();
    if let Some(rollout) = rollout {
        rollout.abort();
    }
    if let Some(reap) = reap {
        reap.abort();
    }
    if let Some(monitor) = monitor {
        monitor.abort();
    }
    if let Some(tidy) = tidy {
        tidy.abort();
    }
    if let Some(look) = look {
        look.abort();
    }
    tools.end().await;
    result
}
