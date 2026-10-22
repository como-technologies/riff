//! The riff tools of a session in a container, over MCP.
//!
//! The daemon serves these tools to the session of `riff run` and to the
//! lead through `riff relay mcp` ([`crate::daemon`],
//! [`crate::session_relay`]). The tools keep the URI of their session.
//! Each tool sends the URI with its request.
//!
//! # A session in a container
//!
//! The daemon runs on the host, outside the container. So a session gets
//! only the tools of [`CONTAINER_TOOLS`]: they only talk, and act on no
//! folder of the host and no claim (01M4ZV3B6T2KB09WJ59F07HXET). They act
//! on the riff-server of the session. `riff run` does the claim, the
//! release and the verify request. The tools do not list each other
//! tool, and refuse a call of it with [`text::not_in_a_container`]. The
//! instructions of the server are [`text::MCP_CONTAINER`].
//!
//! The lead in a container ([`crate::lead`]) also gets the tools of a
//! lead that only talk: [`LEAD_TOOLS`]. Its instructions are
//! [`text::MCP_LEAD`]: they say how the lead gets its wakes, the steps
//! of the waves, and the rules of the lead.
//!
//! ```
//! assert!(riff::tools::LEAD_TOOLS.contains(&"notes"));
//! assert!(!riff::tools::CONTAINER_TOOLS.contains(&"notes"));
//! ```
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
//! # The tokens of a claim
//!
//! A granted `claim` records the start time of the claim on this
//! machine. A `release` of the claim of the session, and a `leave`, end
//! it, sum its tokens, and put the sum on the issue of the claim
//! (01M3Y1YP1ZA5TBRA01MKWM3VC6). See [`crate::usage`].

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, RiffState, Status};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::usage::{self, Meter};
use crate::{dropped, identity, leave, text};
use riff_client::api::{Api, PauseScope};

#[derive(Clone)]
pub struct Tools {
    api: Api,
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
pub const LEAD_TOOLS: [&str; 14] = [
    "blocked", "free", "hold", "lead", "notes", "pause", "post", "read", "resume", "status",
    "tell", "threads", "who", "whoami",
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
    /// The thread. Leave it out to read the unread messages of all your
    /// threads. A direct thread of yours is `dm:` and the session ID of
    /// the other side or its start, or the user of a person, as
    /// `threads` shows it.
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
pub struct NotesArgs {
    /// The repository thread. Leave it out to use your repository
    /// thread.
    thread: Option<String>,
    /// The whole new text of the notes. It replaces the old text. Leave
    /// it out to read the notes. 64 KiB at most. Never put a secret in
    /// it: riff refuses a text that looks like a token.
    text: Option<String>,
    /// True reads also the old texts. It has no effect with a text.
    history: Option<bool>,
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
            me: Arc::new(Mutex::new(me)),
            dir: Arc::new(Mutex::new(std::env::current_dir().unwrap_or_default())),
            earlier: false,
            worker: false,
            meter: None,
            reach: Reach::Host,
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

    /// The description of the tool `name`, or `None` when no tool has
    /// that name.
    ///
    /// ```
    /// let blocked = riff::tools::Tools::description("blocked").unwrap();
    /// assert!(blocked.contains("The next prompt of the person ends the block of the lead."));
    /// assert!(riff::tools::Tools::description("nothing").is_none());
    /// ```
    pub fn description(name: &str) -> Option<String> {
        let tool = Self::tool_router().get(name)?.clone();
        Some(tool.description?.into_owned())
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
            .api
            .who(&me, true)
            .await
            .ok()
            .and_then(|list| list.into_iter().find(|s| same(&s.uri, &me)))
            .map_or(me.clone(), |s| s.uri);
        let state = match self.api.pauses(&me).await {
            Ok(pauses) => format!(
                "{}\n{}",
                text::riff_state(&pauses, now.default_thread().as_ref()),
                text::build_line(riff_client::link::server_build().as_ref())
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
        let pauses = self.api.pauses(&me).await.map_err(err)?;
        let mut who = self.api.roster(&me, all).await.map_err(err)?;
        crate::state::fill(&mut who.sessions, pauses.state);
        let owner = text::owner_line(&who.owner)
            .map(|line| format!("{line}\n"))
            .unwrap_or_default();
        Ok(format!(
            "{}\n{owner}{}\n{}",
            text::riff_state(&pauses, me.default_thread().as_ref()),
            text::build_line(riff_client::link::server_build().as_ref()),
            text::who(&who.sessions, &who.owner, &me)
        ))
    }

    #[tool(description = "List your threads with their unread counts.")]
    async fn threads(&self) -> ToolResult {
        let me = self.here()?;
        let list = self.api.threads(&me).await.map_err(err)?;
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
            .api
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
        self.api.status(&self.here()?, &status).await.map_err(err)?;
        Ok(text::status_set(&status))
    }

    #[tool(
        description = "Say that you cannot go on with no decision of a person. One call does both: \
riff shows you as blocked, and the message `blocked: REASON` wakes the lead of your user. The block \
of a worker ends at its next work after an answer. Do not use it to wait for a verify, a merge or a \
need: riff shows that wait by itself. When you are the lead, it sends no message: riff shows you as \
waiting for your user. The next prompt of the person ends the block of the lead."
    )]
    async fn blocked(&self, Parameters(a): Parameters<BlockedArgs>) -> ToolResult {
        let told = self
            .api
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
        let posted = self.api.tell(&me, &a.session, &a.body).await.map_err(err)?;
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
        let thread = match a.thread {
            Some(t) => Some(self.api.thread_to_read(&me, &t).await.map_err(err)?),
            None => None,
        };
        let inbox = self
            .api
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
            .api
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
        let reply = self.api.free(&me, &thread, &a.item).await.map_err(err)?;
        self.lead_step(&me, format!("freed {}", a.item)).await;
        Ok(text::freed(reply, &thread, &a.item))
    }

    #[tool(
        description = "Only the lead: read or write the notes of the lead. The notes are text \
that riff keeps for your user and your repository: the running state, what waits for the person, \
and the choices of the person and of the host. Read them at each start, resume and compaction. \
With `text`, replace the whole text and keep the old text in the history. Without `text`, read \
the notes; `history` true reads also the old texts. 64 KiB at most. Never put a secret in the \
notes."
    )]
    async fn notes(&self, Parameters(a): Parameters<NotesArgs>) -> ToolResult {
        let thread = self.thread(a.thread)?;
        let me = self.here()?;
        if let Some(body) = a.text {
            let reply = self.api.notes_set(&me, &thread, &body).await.map_err(err)?;
            self.lead_step(&me, "wrote the notes".into()).await;
            return Ok(text::notes_saved(reply, &thread));
        }
        let reply = self
            .api
            .notes_show(&me, &thread, None, a.history.unwrap_or(false))
            .await
            .map_err(err)?;
        Ok(text::notes_shown(&reply, &thread))
    }

    #[tool(
        description = "Make this session the lead of your user in your repository. The other \
sessions of your user send their questions to the lead. It replaces the old lead. Call it only when \
your user says so."
    )]
    async fn lead(&self) -> ToolResult {
        let me = self.here()?;
        let reply = self.api.lead(&me).await.map_err(err)?;
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

impl Tools {
    /// Pauses or resumes the repository of the session, or with
    /// `whole` the whole riff (01M3XAHZBGSSJB3YX23K88W01K).
    async fn set_riff(&self, whole: bool, state: RiffState) -> ToolResult {
        let me = self.here()?;
        let scope = PauseScope::of(whole, None);
        let (reply, posted) = self.api.set_pause(&me, &scope, state).await.map_err(err)?;
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
        let Ok(list) = self.api.who(me, false).await else {
            return;
        };
        let lead = list.into_iter().find(|s| same(&s.uri, me) && s.uri.lead());
        if lead.is_some() {
            let _ = self.api.status(me, &Status { step }).await;
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
/// session ID.
fn same(a: &SessionUri, b: &SessionUri) -> bool {
    a.who() == b.who() || (a.who().session().is_some() && a.who().session() == b.who().session())
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

impl Tools {}
