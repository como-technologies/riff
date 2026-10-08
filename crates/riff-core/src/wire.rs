//! Requests, replies and events between `riff` and `riff-server`.
//!
//! # Protocol
//!
//! Each call is a `POST` with a JSON body. A reply is JSON with status
//! 200. An error is plain text with status 400, 403, 404 or 409.
//!
//! Each call is one type that implements [`Call`]: the trait gives the
//! path and the type of the reply (01M3WRD8TBDPA4JNEZY6J4N2EX). The
//! client and the server use the same ones. A command asks for a
//! change of the state that the log gives. A signal changes only what
//! the server keeps in memory. A query reads.
//!
//! | Path | Request | Reply | Sort |
//! |---|---|---|---|
//! | `/v1/register` | [`Register`] | `null` | command |
//! | `/v1/start` | [`Start`] | [`Started`] | command |
//! | `/v1/end` | [`End`] | `null` | command |
//! | `/v1/join` | [`Join`] | `null` | command |
//! | `/v1/leave` | [`Leave`] | `null` | command |
//! | `/v1/post` | [`Post`] | [`Posted`] | command |
//! | `/v1/claim` | [`Claim`] | [`ClaimReply`] | command |
//! | `/v1/release` | [`Release`] | `null` | command |
//! | `/v1/release/for` | [`ReleaseFor`] | `null` | command |
//! | `/v1/lead` | [`Lead`] | [`LeadReply`] | command |
//! | `/v1/pause` | [`Pause`] | [`RiffReply`] | command |
//! | `/v1/resume` | [`Resume`] | [`RiffReply`] | command |
//! | `/v1/idle/set` | [`SetIdle`] | [`Idle`] | command |
//! | `/v1/plan/hold` | [`Hold`] | [`HoldReply`] | command |
//! | `/v1/plan/free` | [`Free`] | [`FreeReply`] | command |
//! | `/v1/plan` | [`SetPlan`] | [`PlanReply`] | command |
//! | `/v1/plan/off` | [`PlanOff`] | [`PlanOffReply`] | command |
//! | `/v1/forge/allow` | [`ForgeAllow`] | [`ForgeAccounts`] | command |
//! | `/v1/status` | [`SetStatus`] | `null` | signal |
//! | `/v1/blocked` | [`SetBlocked`] | `null` | signal |
//! | `/v1/step` | [`SetStep`] | `null` | signal |
//! | `/v1/blocked/look` | [`BlockedLook`] | [`BlockedLookReply`] | signal |
//! | `/v1/items` | [`ItemFacts`] | `null` | signal |
//! | `/v1/alive` | [`Alive`] | [`AliveReply`] | signal |
//! | `/v1/plan/seen` | [`PlanSeen`] | [`PlanReply`] | signal |
//! | `/v1/who` | [`WhoRequest`] | [`WhoReply`] | query |
//! | `/v1/threads` | [`Threads`] | [`ThreadsReply`] | query |
//! | `/v1/read` | [`Read`] | [`ReadReply`] | query |
//! | `/v1/riff` | [`RiffQuery`] | [`RiffReply`] | query |
//! | `/v1/idle` | [`IdleQuery`] | [`Idle`] | query |
//! | `/v1/plan/show` | [`PlanShow`] | [`PlanReply`] | query |
//!
//! `GET /v1/me` gives [`MeReply`]. `GET /v1/server` gives
//! [`ServerFacts`], also while the server replies 503 to each other
//! call.
//!
//! `POST /v1/token` is an OAuth 2.1 token endpoint. Its request is a
//! form, [`TokenRequest`]. Its reply is [`TokenReply`], or
//! [`TokenError`] with status 400. `GET /v1/sign-in` gives
//! [`SignInConfig`], or status 404 when the server has no sign-in
//! provider.
//!
//! `POST /v1/revoke` ends each sign-in of one person (R20). It needs
//! an access token and a DPoP proof. Its request is [`Revoke`] and
//! its reply is [`Revoked`]. A missing or bad token gets status 401. A
//! person who is not an admin and names another user gets status 403.
//!
//! Five calls change or show who may join the riff
//! (01M3JN3AHMK532XMRDASD4XD5D). Each needs an access token and a DPoP
//! proof, like `revoke`:
//!
//! | Path | Request | Reply | Who |
//! |---|---|---|---|
//! | `POST /v1/invite` | [`Invite`] | [`Invited`] | an admin |
//! | `POST /v1/remove` | [`Remove`] | [`Removed`] | an admin |
//! | `POST /v1/members` | [`Members`] | [`MembersReply`] | each person |
//! | `POST /v1/log` | [`LogQuery`] | [`LogReply`] | an admin |
//! | `POST /v1/admin` | [`SetAdmin`] | [`AdminSet`] | the owner |
//! | `POST /v1/owner` | [`PassOwner`] | [`OwnerPassed`] | the owner |
//!
//! A person who is not an admin gets status 403 from `invite`,
//! `remove` and `log`. A person who is not the owner gets status 403 from
//! `admin` and `owner`. `owner` to a person who is not a member or an
//! admin gets status 400. `remove` of the owner or of an admin gets status 400.
//!
//! Two metadata documents follow the MCP authorization spec (R22):
//!
//! | Path | Reply |
//! |---|---|
//! | `GET /.well-known/oauth-protected-resource` | [`ResourceMetadata`] (RFC 9728) |
//! | `GET /.well-known/oauth-authorization-server` | [`ServerMetadata`] (RFC 8414) |
//!
//! Two streams use server-sent events. Each event is one `data:` line
//! that holds JSON:
//!
//! | Stream | Query | Event |
//! |---|---|---|
//! | `GET /v1/watch` | `uri=<session URI>` | [`Wake`] |
//! | `GET /v1/tail` | `thread=<thread name>` | [`Tailed`] |
//!
//! `GET /v1/me` with the query `uri=<session URI>` gives [`MeReply`]:
//! only the session of the caller (01M3T5GFVS8NMA992KHZN4VE17). The
//! status line calls it, not `who`.
//!
//! Each stream starts with the comment line `: ready`, so that it sends
//! its first bytes when it opens. A reader skips each line that is not
//! a `data:` line.
//!
//! A session is live while its watch stream is open. Each request
//! carries the session URI of its sender as `me`. The server finds the
//! session by the *who* part of that URI. When the server needs
//! sign-in, the *who* part must match the token, or the reply is 403
//! (R104). A session keeps the user of its first call: a known session
//! ID with another user gets 409 (R159).
//! Only `register` sets the place of a known session; each other call
//! uses the URI only to make a session that the server does not know.
//!
//! # Example
//!
//! A post to the holder of a claim:
//!
//! ```
//! use riff_core::wire::Post;
//!
//! let post: Post = serde_json::from_str(r#"{
//!     "me": "riff://mike@pangolin/como-technologies/riff?session=a6cf",
//!     "thread": "como-technologies/riff",
//!     "to": [{ "claim": "issue-6" }],
//!     "body": "The API is ready."
//! }"#).unwrap();
//! assert_eq!(post.to[0].claim.as_deref(), Some("issue-6"));
//! ```

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::dpop::Key;
use crate::name::{SessionUri, ThreadName};
use crate::record::{By, Line, Plan, PlanSet, Record};
use crate::selector::Selector;
use crate::signed::Content;

/// The HTTP header of a refused command: the code of the refusal, for
/// example `held` (01M3WRD9JBQMNN96TXJH8EAJ3W). The text of the reply
/// is the reason, for a person.
pub const REFUSED_HEADER: &str = "riff-refused";

/// The HTTP header of the call ID of a command
/// (01M48VFX22S4811DYBBD7QDW24). Each try of one call sends the same ID.
/// `riff-server` runs a command with a call ID one time only, and gives
/// a second try the reply of the first.
pub const CALL_HEADER: &str = "riff-call";

/// The HTTP header of the reply to a repeated call: `1`. The reply is
/// the reply of the kept call, on the state of now
/// (01M48VFX22S4811DYBBD7QDW24).
pub const REPEAT_HEADER: &str = "riff-repeat";

// ANCHOR: call
/// A call: one wire type with its path and the type of its reply
/// (01M3WRD8TBDPA4JNEZY6J4N2EX). The client sends each call with one
/// generic function, and the server makes one route for each.
///
/// ```
/// use riff_core::wire::{Call, Claim};
///
/// /// The path of a call and the JSON of its body.
/// fn request<C: Call>(call: &C) -> (&'static str, String) {
///     (C::PATH, serde_json::to_string(call).unwrap())
/// }
///
/// let claim = Claim {
///     me: "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?,
///     thread: "como-technologies/riff".parse()?,
///     item: "issue-12".into(),
/// };
/// let (path, body) = request(&claim);
/// assert_eq!(path, "/v1/claim");
/// assert!(body.contains("issue-12"));
/// let reply: <Claim as Call>::Reply = serde_json::from_str(
///     r#"{"holder":"riff://mike@pangolin/como-technologies/riff?session=a6cf&claim=issue-12"}"#,
/// ).unwrap();
/// assert_eq!(reply.holder.claims(), ["issue-12"]);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub trait Call: Serialize + DeserializeOwned {
    /// The HTTP path of the call, for example `/v1/claim`.
    const PATH: &'static str;
    /// The reply to the call.
    type Reply: Serialize + DeserializeOwned;
}
// ANCHOR_END: call

/// Gives each call its path and its reply.
macro_rules! calls {
    ($($call:ty => $path:literal, $reply:ty;)*) => {
        $(impl Call for $call {
            const PATH: &'static str = $path;
            type Reply = $reply;
        })*
    };
}

calls! {
    Register => "/v1/register", ();
    Start => "/v1/start", Started;
    End => "/v1/end", ();
    Join => "/v1/join", ();
    Leave => "/v1/leave", ();
    Post => "/v1/post", Posted;
    Claim => "/v1/claim", ClaimReply;
    Release => "/v1/release", ReleaseReply;
    ReleaseFor => "/v1/release/for", ();
    Lead => "/v1/lead", LeadReply;
    Pause => "/v1/pause", RiffReply;
    Resume => "/v1/resume", RiffReply;
    SetIdle => "/v1/idle/set", Idle;
    Hold => "/v1/plan/hold", HoldReply;
    Free => "/v1/plan/free", FreeReply;
    SetPlan => "/v1/plan", PlanReply;
    PlanOff => "/v1/plan/off", PlanOffReply;
    PlanSeen => "/v1/plan/seen", PlanReply;
    PlanShow => "/v1/plan/show", PlanReply;
    SetStatus => "/v1/status", ();
    SetBlocked => "/v1/blocked", ();
    SetStep => "/v1/step", ();
    BlockedLook => "/v1/blocked/look", BlockedLookReply;
    ItemFacts => "/v1/items", ();
    Alive => "/v1/alive", AliveReply;
    WhoRequest => "/v1/who", WhoReply;
    Threads => "/v1/threads", ThreadsReply;
    Read => "/v1/read", ReadReply;
    RiffQuery => "/v1/riff", RiffReply;
    IdleQuery => "/v1/idle", Idle;
    Revoke => "/v1/revoke", Revoked;
    Invite => "/v1/invite", Invited;
    Remove => "/v1/remove", Removed;
    Members => "/v1/members", MembersReply;
    LogQuery => "/v1/log", LogReply;
    SetAdmin => "/v1/admin", AdminSet;
    PassOwner => "/v1/owner", OwnerPassed;
    TakeOwner => "/v1/owner/take", OwnerAsked;
    DenyOwner => "/v1/owner/deny", OwnerDenied;
    ForgeToken => "/v1/forge/token", ForgeTokenReply;
    ForgeCheck => "/v1/forge/check", ForgeCheckReply;
    ForgeAllow => "/v1/forge/allow", ForgeAccounts;
}

/// `POST /v1/register`: a session says that it exists and where it
/// works. A session registers when it starts and when it moves. It
/// joins the thread of its repository. A worker says that it is a
/// worker, so `who` shows it the same on each machine
/// (01M3NT4M159EHN5W8JRTQ417N4).
///
/// ```
/// use riff_core::wire::Register;
///
/// let old: Register = serde_json::from_str(r#"{"me":"riff://mike@pangolin/o/r?session=a1"}"#).unwrap();
/// assert!(!old.worker);
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Register {
    pub me: SessionUri,
    /// True when the session is a worker: `riff workers run` started it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub worker: bool,
}

/// `riff mcp` sends a keep-alive this often, also while no turn runs
/// (R204). A session with no sign of life for 3 minutes is gone.
pub const ALIVE_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// `POST /v1/alive`: a keep-alive. It shows that the session still runs,
/// but it is not a call: the idle time in `who` stays (R204).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Alive {
    pub me: SessionUri,
    /// The newest fact of the hooks of the session on its machine, when
    /// it has one (01M41FZNTPXQNCZ1S99HE42PYQ).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<Activity>,
    /// The seconds since the last prompt of the person in the session,
    /// when the prompt hook saw one (01M48VDWPDYRPEAXHR1MYDN1M7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_secs: Option<u64>,
}

/// The most characters in the text of a tool of an [`Activity`].
pub const ACTIVITY_CHARS: usize = 80;

/// What the hooks of a session saw last (01M41FZNTPXQNCZ1S99HE42PYQ):
/// a tool that runs, a turn that runs between two tools, or a turn
/// that ended. A hook writes it on the machine at each tool call and at
/// the end of each turn, and makes no call. `riff mcp` sends the newest
/// one in its keep-alive. A riff tool and `riff watch` give no fact:
/// they are no work.
///
/// ```
/// use riff_core::wire::Activity;
///
/// let tool = Activity { tool: Some("Bash: run just ci".into()), turn: true, secs: 30 };
/// assert!(tool.works());
/// let between = Activity { tool: None, turn: true, secs: 2 };
/// assert!(between.works());
/// let ended = Activity { tool: None, turn: false, secs: 5 };
/// assert!(!ended.works());
/// let json = serde_json::to_string(&ended).unwrap();
/// assert_eq!(json, r#"{"secs":5}"#);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Activity {
    /// The tool that runs, with its short text: `Bash: run just ci`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// True while a turn runs. False when the turn ended.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub turn: bool,
    /// The seconds since the fact.
    pub secs: u64,
}

impl Activity {
    /// True while a turn runs: a sign of work.
    pub fn works(&self) -> bool {
        self.turn
    }
}

/// The reply to [`Alive`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AliveReply {
    /// True when the server asks this idle worker to stop
    /// (01M3Q5A0NKY1FCS0YH6N6YD3GN). A call of the session since the ask,
    /// or the end of its watch at a wake, takes it back.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stop: bool,
    /// True when the session is a worker that must clear its context
    /// before its next claim (01M3X9XB37TQCXWPNFZRMRGJB4). The reply to its last release
    /// said so too.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear: bool,
}

/// A worker session sends a keep-alive this often, so that it stops soon
/// after the server asks (01M3Q5A0NKY1FCS0YH6N6YD3GN).
pub const WORKER_ALIVE_EVERY: std::time::Duration = std::time::Duration::from_secs(10);

/// `POST /v1/end`: the session ended (R205). It leaves `who`, and its
/// claims are free at once.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct End {
    pub me: SessionUri,
}

/// `POST /v1/who`: lists the known sessions. The call counts as a call
/// of `me`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WhoRequest {
    pub me: SessionUri,
    /// True lists gone sessions too.
    #[serde(default)]
    pub all: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WhoReply {
    pub sessions: Vec<SessionInfo>,
    /// The owner of the riff (01M3Q63NK0AHM25MB258B0K8XP).
    #[serde(default)]
    pub owner: RiffOwner,
    /// Each member of the riff with a USER, also when away
    /// (01M3NT4M3A4E3K5S2NM7MS6PQD). A riff with no sign-in has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<Person>,
}

/// The reply to `GET /v1/me`: only the session of the caller, and the
/// build of the server (01M3T5GFVS8NMA992KHZN4VE17). `session` is None
/// when the server does not know the session.
///
/// ```
/// use riff_core::wire::MeReply;
///
/// let reply: MeReply =
///     serde_json::from_str(r#"{"session":null,"build":"0.8.0 e3cfe5a2919c 2026-09-29T21:12:48Z"}"#)?;
/// assert!(reply.session.is_none());
/// assert_eq!(reply.build, "0.8.0 e3cfe5a2919c 2026-09-29T21:12:48Z");
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MeReply {
    /// The session of the caller, as `who` shows it.
    pub session: Option<SessionInfo>,
    /// The build of the server, as the `riff-build` header gives it.
    pub build: String,
}

/// `POST /v1/forge/token`: the forge token of the session `me`
/// (#628). The server picks the role and the repository from its own
/// facts of `me` ([`crate::forge::role_of`]). A `me` with no session
/// is the lead of the person at its place: the wrapper of the lead asks
/// before its session starts. A riff with no sign-in gives no token.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ForgeToken {
    pub me: SessionUri,
}

/// The reply to [`ForgeToken`]: a token for one repository, with the
/// rights of one role. Its `Debug` hides the token.
///
/// ```
/// use riff_core::forge::TokenRole;
/// use riff_core::wire::ForgeTokenReply;
///
/// let reply = ForgeTokenReply {
///     role: TokenRole::Worker,
///     repo: "o/r".into(),
///     token: "ghs_secret".into(),
///     ends_ms: 1,
///     permissions: Default::default(),
/// };
/// assert!(!format!("{reply:?}").contains("ghs_secret"));
/// ```
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ForgeTokenReply {
    /// The role that the server picked.
    pub role: crate::forge::TokenRole,
    /// The repository of the token, `OWNER/NAME`.
    pub repo: String,
    /// The token. Never print it.
    pub token: String,
    /// When the token ends, in milliseconds since the Unix epoch.
    pub ends_ms: u64,
    /// The permissions that GitHub gave.
    pub permissions: BTreeMap<String, crate::forge::Access>,
}

impl std::fmt::Debug for ForgeTokenReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForgeTokenReply")
            .field("role", &self.role)
            .field("repo", &self.repo)
            .field("ends_ms", &self.ends_ms)
            .field("permissions", &self.permissions)
            .finish_non_exhaustive()
    }
}

/// `POST /v1/forge/check`: `riff forge check`. The server makes a token
/// of each role for the repository of `me`, checks its rights, and
/// revokes it at once. The reply holds no token.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ForgeCheck {
    pub me: SessionUri,
}

/// The reply to [`ForgeCheck`]: one line for each role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ForgeCheckReply {
    /// The repository of the check, `OWNER/NAME`.
    pub repo: String,
    /// The ID of the GitHub App.
    pub app: u64,
    pub roles: Vec<RoleCheck>,
}

/// `POST /v1/forge/allow`: `riff forge allow`. The server makes forge
/// tokens only for the repositories of the GitHub accounts that the
/// owner or an admin allowed (01M4CHQR1E5HFV6KSTSM72H0QV). `owner` is an
/// organization or a personal account. With `allowed` true, the server
/// allows it; with false, it allows it no more. With no `owner`, the
/// command changes nothing and gives the list. Only the owner or an admin
/// can send it, as a person.
///
/// ```
/// use riff_core::wire::ForgeAllow;
///
/// let allow: ForgeAllow = serde_json::from_str(
///     r#"{"me":"riff://mike@pangolin/acme/app","owner":"acme","allowed":true}"#,
/// ).unwrap();
/// assert_eq!(allow.owner.as_deref(), Some("acme"));
/// let list: ForgeAllow = serde_json::from_str(r#"{"me":"riff://mike@pangolin"}"#).unwrap();
/// assert!(list.owner.is_none() && !list.allowed);
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ForgeAllow {
    pub me: SessionUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default)]
    pub allowed: bool,
}

/// The reply to [`ForgeAllow`]: the GitHub accounts that the server
/// makes forge tokens for, in lower case and in order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ForgeAccounts {
    #[serde(default)]
    pub accounts: Vec<String>,
}

/// The check of the token of one role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleCheck {
    pub role: crate::forge::TokenRole,
    /// The permissions that GitHub gave, when it gave a token.
    #[serde(default)]
    pub permissions: BTreeMap<String, crate::forge::Access>,
    /// Why the role has no good token, if it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The reply to `GET /v1/server`: the facts of one `riff-server`, for
/// `riff server` (01M3TJWJ12WEDCXW3W0529KRP2). Each field has a default,
/// so a `riff` of another build reads the reply. Each time is in
/// milliseconds since the Unix epoch, on the clock of the server.
///
/// ```
/// use riff_core::wire::ServerFacts;
///
/// let facts: ServerFacts = serde_json::from_str(r#"{"position":7,"later":true}"#)?;
/// assert_eq!(facts.position, 7);
/// assert!(facts.not_serving.is_none() && facts.checkpoint.is_none());
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ServerFacts {
    /// Why the server replies 503 to each other call. `None`: it serves.
    pub not_serving: Option<String>,
    /// The last error of the server since its start.
    pub last_error: Option<FactError>,
    /// The position of the last written record of the log.
    pub position: u64,
    /// The time of the last chunk write.
    pub chunk_written_at_ms: Option<u64>,
    /// How long the last chunk write took, in milliseconds.
    pub chunk_write_ms: Option<u64>,
    /// The number of failed tries of a chunk write since the start.
    pub write_errors: u64,
    /// The number of records that this build skipped since the start.
    pub skipped_records: u64,
    /// The newest checkpoint.
    pub checkpoint: Option<CheckpointFacts>,
    /// Why this build writes no checkpoint. `None`: it writes them.
    pub no_checkpoint: Option<String>,
    /// The number of chunks in the store.
    pub chunks: u64,
    pub sessions: u64,
    /// The number of read cursors.
    pub cursors: u64,
    pub threads: u64,
    /// The number of live token chains: one for each sign-in.
    pub sign_ins: u64,
    /// The memory that the server uses, in bytes. `None` when the
    /// system does not tell.
    pub memory_bytes: Option<u64>,
    /// The start time of the instance.
    pub started_at_ms: u64,
    /// How long the load and the replay took, in milliseconds.
    pub replay_ms: u64,
    /// The time of this reply.
    pub now_ms: u64,
}

/// An error of the server in [`ServerFacts`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct FactError {
    pub message: String,
    pub at_ms: u64,
}

/// The newest checkpoint in [`ServerFacts`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CheckpointFacts {
    pub position: u64,
    pub written_at_ms: u64,
    /// The version of the build that wrote it.
    pub build: String,
}

/// A member of the riff in the reply to `who`: a person, not a session
/// (01M3NT4M3A4E3K5S2NM7MS6PQD).
///
/// ```
/// use riff_core::wire::{Person, PersonRole};
///
/// let ada: Person =
///     serde_json::from_str(r#"{"user":"ada","role":"owner","live":false,"seen_secs":3600}"#).unwrap();
/// assert_eq!(ada.role, PersonRole::Owner);
/// assert_eq!(ada.role.tag(), Some("owner"));
/// assert_eq!(PersonRole::Member.tag(), None);
/// assert_eq!(ada.seen_secs, Some(3600));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Person {
    /// The USER of the person.
    pub user: String,
    pub role: PersonRole,
    /// True while a session of the person is live.
    pub live: bool,
    /// The seconds since the last call of a session of the person.
    /// `None` when the server knows no session of the person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_secs: Option<u64>,
}

/// The role of a person in the riff.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PersonRole {
    Owner,
    Admin,
    Member,
}

impl PersonRole {
    /// The tag on the row of the person: `owner`, `admin`, or none for
    /// a member.
    pub fn tag(self) -> Option<&'static str> {
        match self {
            PersonRole::Owner => Some("owner"),
            PersonRole::Admin => Some("admin"),
            PersonRole::Member => None,
        }
    }
}

/// The owner of the riff, in the reply to `who`
/// (01M3Q63NK0AHM25MB258B0K8XP). A reply with no owner field reads as
/// [`RiffOwner::NoSignIn`].
///
/// ```
/// use riff_core::wire::{RiffOwner, WhoReply};
///
/// let reply: WhoReply = serde_json::from_str(
///     r#"{"sessions":[],"owner":{"kind":"owner","user":"ada","email":"ada@gmail.com"}}"#,
/// ).unwrap();
/// assert_eq!(reply.owner, RiffOwner::Owner { user: "ada".into(), email: "ada@gmail.com".into() });
/// assert!(reply.owner.is("ada") && !reply.owner.is("bob"));
///
/// let old: WhoReply = serde_json::from_str(r#"{"sessions":[]}"#).unwrap();
/// assert_eq!(old.owner, RiffOwner::NoSignIn);
/// assert!(!RiffOwner::Nobody.is("ada"));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RiffOwner {
    /// A riff with no sign-in: it shows no owner.
    #[default]
    NoSignIn,
    /// A riff with sign-in and no owner yet.
    Nobody,
    /// The owner: the USER and the email, in lower case.
    Owner { user: String, email: String },
}

impl RiffOwner {
    /// True when `user` is the owner.
    pub fn is(&self, user: &str) -> bool {
        matches!(self, RiffOwner::Owner { user: owner, .. } if owner == user)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionInfo {
    /// The URI now: the place and the claims are current.
    pub uri: SessionUri,
    /// True while the session has an open watch stream. A lead is live
    /// while it is not gone, also with no watch
    /// (01M48VDGQ5KETKPM4G6TKTC2MB).
    pub live: bool,
    /// The seconds since the last call of the session. 0 while it is
    /// live.
    #[serde(default)]
    pub idle_secs: u64,
    /// The last status that the session set, if it set one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusInfo>,
    /// True when the session registered as a worker
    /// (01M3NT4M159EHN5W8JRTQ417N4).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub worker: bool,
    /// True when the server asked this idle worker to stop
    /// (01M3Q5A0NKY1FCS0YH6N6YD3GN).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stopping: bool,
    /// The seconds since the claims of the session last changed: a
    /// claim, a release, a new start, or the start of `riff-server`. A
    /// worker with no claim is idle for this time
    /// (01M3Q551WCMPQRCNJ8FXQEBFY4).
    #[serde(default)]
    pub claims_secs: u64,
    /// True when the session is a worker that must clear its context
    /// before its next claim (01M3X9XAK1KPZZVM1AJR2H8DSS).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub must_clear: bool,
    /// The seconds since the last fresh start of the session: a new
    /// agent process or a clear of its context. `None` when the log has
    /// no such start (01M3X9XC99KY4RQY36A7CYWY11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fresh_secs: Option<u64>,
    /// The state of the session, that the server derives
    /// (01M3QB6CJ1XCQG5B1BVR8AF3B4). An older server sends none: see
    /// [`SessionInfo::fill_state`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<SessionState>,
    /// The newest fact of the hooks of the session, with its age now
    /// (01M41FZNTPXQNCZ1S99HE42PYQ).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<Activity>,
    /// What each claim of the session waits for, when each one waits
    /// (01M41FZP9A50CH4A2VX344DW49).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waits: Option<Waits>,
    /// The block of the session, while it holds
    /// (01M41FZPGEK4TNPSM2051W4VMS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<BlockedInfo>,
    /// The long step of the session, until it is done
    /// (01M48VDGTD40P8RBZMS0XB5M9N).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepInfo>,
}

impl SessionInfo {
    /// Sets the state from the other facts of the session when the
    /// server sent none, as an older server does. `riff` is the state of
    /// the riff. A state from the server stays.
    ///
    /// ```
    /// use riff_core::wire::{RiffState, SessionInfo, SessionState};
    ///
    /// // A who reply of an older server: no state.
    /// let json = r#"{"uri":"riff://mike@thelio/o/r?session=w1&claim=issue-12","live":true}"#;
    /// let mut s: SessionInfo = serde_json::from_str(json).unwrap();
    /// assert_eq!(s.state, None);
    /// s.fill_state(RiffState::Running);
    /// assert_eq!(s.state, Some(SessionState::Busy));
    /// s.fill_state(RiffState::Paused);
    /// assert_eq!(s.state, Some(SessionState::Busy), "a state stays");
    /// ```
    pub fn fill_state(&mut self, riff: RiffState) {
        if self.state.is_some() {
            return;
        }
        self.state = Some(SessionState::of(&Facts {
            live: self.live,
            paused: riff == RiffState::Paused,
            blocked: self.blocked.is_some(),
            must_clear: self.must_clear,
            waiting: self.waits.is_some(),
            claims: !self.uri.claims().is_empty(),
            lead: self.uri.lead(),
            turn: self.work.as_ref().is_some_and(Activity::works),
        }));
    }
}

/// The state of a session. The server derives it from facts; no
/// session reports it (01M3QB6CJ1XCQG5B1BVR8AF3B4,
/// 01M41FZQVEF8S2W9RCM4V87C3D). The first state that matches wins, in
/// this order:
///
/// 1. `offline`: the session has no open watch stream. A lead is not
///    offline while it is not gone, also with no watch
///    (01M48VDGQ5KETKPM4G6TKTC2MB).
/// 2. `paused`: the riff is paused, or the repository of the session is
///    paused (01M3XAHZBGSSJB3YX23K88W01K).
/// 3. `blocked`: the session said that it cannot go on with no
///    decision, and the block holds (01M41FZPGEK4TNPSM2051W4VMS). A
///    lead with a block is `waiting` for its person
///    (01M48VDS8RKJS9HG3KSEYGBFGV).
/// 4. `must_clear`: it is a worker that must clear its context before
///    its next claim (01M3X9XAK1KPZZVM1AJR2H8DSS).
/// 5. `waiting`: each claim of the session waits for the machinery: a
///    verify, a merge, or an item of its `Needs:` line
///    (01M41FZP9A50CH4A2VX344DW49). Or a lead waits for its person.
/// 6. `busy`: it holds a claim. Or it is the lead, and a turn runs.
/// 7. `idle`: each other session.
///
/// ```mermaid
/// stateDiagram-v2
///     [*] --> idle
///     idle --> busy: claim
///     busy --> waiting: a verify is asked, a merge waits, a need is open
///     waiting --> busy: the fact ends
///     busy --> blocked: blocked REASON (wakes the lead)
///     blocked --> busy: an answer, then a sign of work
///     busy --> must_clear: the last release of a worker
///     must_clear --> idle: a clear
/// ```
///
/// ```
/// use riff_core::wire::{Facts, SessionState};
///
/// let all = Facts {
///     live: true, paused: true, blocked: true, must_clear: true,
///     waiting: true, claims: true, lead: false, turn: false,
/// };
/// let of = |f: Facts| SessionState::of(&f);
/// assert_eq!(of(Facts { live: false, ..all }), SessionState::Offline);
/// assert_eq!(of(all), SessionState::Paused);
/// let running = Facts { paused: false, ..all };
/// assert_eq!(of(running), SessionState::Blocked);
/// let free = Facts { blocked: false, ..running };
/// assert_eq!(of(free), SessionState::MustClear);
/// let cleared = Facts { must_clear: false, ..free };
/// assert_eq!(of(cleared), SessionState::Waiting);
/// assert_eq!(of(Facts { waiting: false, ..cleared }), SessionState::Busy);
/// let none = Facts { waiting: false, claims: false, ..cleared };
/// assert_eq!(of(none), SessionState::Idle);
///
/// // A lead: busy in a turn, waiting for its person, idle else.
/// let lead = Facts { lead: true, ..none };
/// assert_eq!(of(lead), SessionState::Idle);
/// assert_eq!(of(Facts { turn: true, ..lead }), SessionState::Busy);
/// assert_eq!(of(Facts { blocked: true, turn: true, ..lead }), SessionState::Waiting);
///
/// assert_eq!(serde_json::to_string(&SessionState::MustClear).unwrap(), r#""must_clear""#);
/// assert_eq!(SessionState::MustClear.word(), "must clear");
/// assert_eq!(serde_json::to_string(&SessionState::Waiting).unwrap(), r#""waiting""#);
/// assert_eq!(SessionState::Blocked.word(), "blocked");
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    #[default]
    Offline,
    Paused,
    Blocked,
    MustClear,
    Waiting,
    Busy,
    Idle,
}

/// The facts that make the state of a session ([`SessionState::of`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// An open watch stream, or a lead that is not gone.
    pub live: bool,
    /// The riff or the repository of the session is paused.
    pub paused: bool,
    /// A block that holds.
    pub blocked: bool,
    /// A worker that must clear its context.
    pub must_clear: bool,
    /// Each claim waits.
    pub waiting: bool,
    /// The session holds a claim.
    pub claims: bool,
    /// The session is the lead of its user.
    pub lead: bool,
    /// The hooks saw a turn that runs.
    pub turn: bool,
}

impl SessionState {
    /// The state of a session from its facts. A lead has no claims, so
    /// its state comes from its turn and its block
    /// (01M48VDS8RKJS9HG3KSEYGBFGV).
    pub fn of(f: &Facts) -> Self {
        if !f.live {
            SessionState::Offline
        } else if f.paused {
            SessionState::Paused
        } else if f.blocked && !f.lead {
            SessionState::Blocked
        } else if f.must_clear {
            SessionState::MustClear
        } else if (f.waiting && f.claims) || (f.blocked && f.lead) {
            SessionState::Waiting
        } else if f.claims || (f.lead && f.turn) {
            SessionState::Busy
        } else {
            SessionState::Idle
        }
    }

    /// The word of the state, the same in each command.
    pub fn word(self) -> &'static str {
        match self {
            SessionState::Offline => "offline",
            SessionState::Paused => "paused",
            SessionState::Blocked => "blocked",
            SessionState::MustClear => "must clear",
            SessionState::Waiting => "waiting",
            SessionState::Busy => "busy",
            SessionState::Idle => "idle",
        }
    }
}

/// What the claims of a session wait for (01M41FZP9A50CH4A2VX344DW49).
/// The server makes it from the facts of the items ([`ItemFact`]).
/// Nobody must decide, so it wakes nobody.
///
/// ```
/// use riff_core::wire::Waits;
///
/// assert_eq!(Waits::Verify { pull: 418 }.to_string(), "waits for a verify of PR #418");
/// assert_eq!(Waits::Merge { pull: 418 }.to_string(), "waits for the merge of PR #418");
/// assert_eq!(Waits::Needs { issues: vec![12, 15] }.to_string(), "waits for #12, #15");
/// let json = serde_json::to_string(&Waits::Merge { pull: 418 }).unwrap();
/// assert_eq!(json, r#"{"for":"merge","pull":418}"#);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "for", rename_all = "snake_case")]
pub enum Waits {
    /// The pull request of the item waits for a verify.
    Verify { pull: u64 },
    /// The verify passed: the pull request waits for the merge.
    Merge { pull: u64 },
    /// Items of the `Needs:` line of the item are open.
    Needs { issues: Vec<u64> },
}

impl std::fmt::Display for Waits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Waits::Verify { pull } => write!(f, "waits for a verify of PR #{pull}"),
            Waits::Merge { pull } => write!(f, "waits for the merge of PR #{pull}"),
            Waits::Needs { issues } => {
                let issues: Vec<String> = issues.iter().map(|n| format!("#{n}")).collect();
                write!(f, "waits for {}", issues.join(", "))
            }
        }
    }
}

/// A block in the reply to `who` (01M41FZPGEK4TNPSM2051W4VMS).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlockedInfo {
    /// Why the session cannot go on.
    pub reason: String,
    /// The seconds since the session said it.
    pub secs: u64,
    /// True when a message woke the session after the block. The block
    /// ends at the next sign of work (01M41FZPT31ATXP75QW965P3JB).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub answered: bool,
    /// True when riff woke the lead a second time
    /// (01M41FZQ545HQ9Q75CSKX8HF8H).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub woken_again: bool,
    /// True when the lead gave no answer after the second wake
    /// (01M41FZQCHWY1YVGAZ60ZHJK21).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unanswered: bool,
}

/// The most characters in the step of a [`Status`], and in the reason
/// of a [`SetBlocked`].
pub const STATUS_CHARS: usize = 400;

/// What a session does now, in its own words: its current step. The
/// words help a person. They make no state: a block is [`SetBlocked`]
/// (01M41FZPGEK4TNPSM2051W4VMS).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Status {
    pub step: String,
}

impl Status {
    /// Refuses a status that `who` cannot show on one line: an empty
    /// step, a line break, or more than [`STATUS_CHARS`] characters.
    ///
    /// ```
    /// use riff_core::wire::Status;
    ///
    /// let status = |step: &str| Status { step: step.into() };
    /// assert!(status("write the tests").check().is_ok());
    /// assert!(status(" ").check().is_err());
    /// assert!(status("two\nlines").check().is_err());
    /// assert!(status(&"x".repeat(401)).check().is_err());
    /// ```
    pub fn check(&self) -> Result<(), String> {
        one_line("step", &self.step)
    }
}

/// Refuses a text that `who` cannot show on one line: empty, with a
/// line break, or with more than [`STATUS_CHARS`] characters. `what`
/// names the text in the error.
fn one_line(what: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err(format!("the {what} is empty"));
    }
    if text.chars().any(char::is_control) {
        return Err(format!("the {what} must be one line"));
    }
    if text.chars().count() > STATUS_CHARS {
        return Err(format!(
            "the {what} has more than {STATUS_CHARS} characters"
        ));
    }
    Ok(())
}

/// A [`Status`] in the reply to `who`, with its age.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StatusInfo {
    #[serde(flatten)]
    pub status: Status,
    /// The seconds since the session set the status.
    pub age_secs: u64,
    /// True when the session set the status before the last change of
    /// its state: a claim or a release, a pause or a resume of the
    /// riff, or a start of `riff-server` (01M3Q551YHYZBFV2NDS1QCYXCD).
    /// A stale status is not the current state of the session.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
}

/// `POST /v1/status`: sets the status of `me`. It replaces the old
/// status.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetStatus {
    pub me: SessionUri,
    pub status: Status,
}

/// `POST /v1/blocked`: `me` cannot go on with no decision
/// (01M41FZPGEK4TNPSM2051W4VMS). It is a signal. The `blocked` tool and
/// `riff blocked` send it with the message that wakes the lead, in one
/// command: a session does neither alone.
///
/// ```
/// use riff_core::wire::SetBlocked;
///
/// let me: riff_core::name::SessionUri = "riff://mike@pangolin/o/r?session=a1".parse()?;
/// let set = |reason: &str| SetBlocked { me: me.clone(), reason: reason.into() };
/// assert!(set("which of the two designs?").check().is_ok());
/// assert_eq!(set(" ").check().unwrap_err(), "the reason is empty");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetBlocked {
    pub me: SessionUri,
    pub reason: String,
}

impl SetBlocked {
    /// Refuses a reason that is not one line of at most
    /// [`STATUS_CHARS`] characters.
    pub fn check(&self) -> Result<(), String> {
        one_line("reason", &self.reason)
    }
}

/// A change of the long step of a session (01M48VDGTD40P8RBZMS0XB5M9N):
/// a step that runs longer than a tool call, for example a live window
/// of two hours.
///
/// ```
/// use riff_core::wire::StepChange;
///
/// let start = StepChange::Start { name: "live window".into() };
/// assert_eq!(serde_json::to_string(&start).unwrap(), r#"{"start":{"name":"live window"}}"#);
/// assert_eq!(serde_json::to_string(&StepChange::Done).unwrap(), r#""done""#);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepChange {
    /// A new step starts. It replaces the old step.
    Start { name: String },
    /// The step ended well. No step shows.
    Done,
    /// The step failed, for `reason`. It shows until the next change.
    Fail { reason: String },
}

/// `POST /v1/step`: a change of the long step of `me`
/// (01M48VDGTD40P8RBZMS0XB5M9N). It is a signal, and a sign of life.
/// `riff step fail` also wakes the lead (01M48VDS663X064YS5ZGCCZSTB).
///
/// ```
/// use riff_core::wire::{SetStep, StepChange};
///
/// let me: riff_core::name::SessionUri = "riff://mike@pangolin/o/r?session=a1".parse()?;
/// let set = |change| SetStep { me: me.clone(), change };
/// assert!(set(StepChange::Start { name: "live window".into() }).check().is_ok());
/// assert!(set(StepChange::Done).check().is_ok());
/// assert_eq!(
///     set(StepChange::Fail { reason: " ".into() }).check().unwrap_err(),
///     "the reason is empty"
/// );
/// assert_eq!(
///     set(StepChange::Start { name: "a\nb".into() }).check().unwrap_err(),
///     "the name must be one line"
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetStep {
    pub me: SessionUri,
    pub change: StepChange,
}

impl SetStep {
    /// Refuses a name or a reason that is not one line of at most
    /// [`STATUS_CHARS`] characters.
    pub fn check(&self) -> Result<(), String> {
        match &self.change {
            StepChange::Start { name } => one_line("name", name),
            StepChange::Done => Ok(()),
            StepChange::Fail { reason } => one_line("reason", reason),
        }
    }
}

/// A long step in the reply to `who` (01M48VDGTD40P8RBZMS0XB5M9N).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StepInfo {
    /// The name of the step.
    pub name: String,
    /// The seconds since the step started, or since it failed.
    pub secs: u64,
    /// The reason, when the step failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}

/// `POST /v1/blocked/look`: the lead `me` looks at the blocks of the
/// sessions of its user in its repository. A block with no answer for
/// `after_secs` wakes the lead again (01M41FZQ545HQ9Q75CSKX8HF8H). A
/// block with no answer for `after_secs` after that is unanswered
/// (01M41FZQCHWY1YVGAZ60ZHJK21). The server refuses a caller that is not
/// the lead.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BlockedLook {
    pub me: SessionUri,
    pub after_secs: u64,
}

/// The reply to [`BlockedLook`]: each block that this look made
/// unanswered. The `riff mcp` of the lead shows each one in a desktop
/// notification (01M41FZQKZKW131Z8822G31T5G).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlockedLookReply {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unanswered: Vec<Unanswered>,
}

/// A block with no answer of the lead: the session with its claims, and
/// the reason. It holds no text of a message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Unanswered {
    pub session: SessionUri,
    pub reason: String,
}

/// `POST /v1/items`: what a client saw of the items of its repository
/// on the forge (01M41FZP2C4Z4J6WKRXZ5B31EH). The server has no
/// credential of the forge: `riff pr open`, `riff verify`, `riff pr
/// wait` and the look of the lead send the facts. It is a signal. With
/// `all`, the facts replace each fact of the repository: an item with
/// no fact in the list has none now. Else each fact replaces only the
/// fact of its item.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ItemFacts {
    pub me: SessionUri,
    pub items: Vec<ItemFact>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all: bool,
}

/// The facts of one item, for example `issue-12`.
///
/// ```
/// use riff_core::wire::{ItemFact, PullFact, PullState, Waits};
///
/// let fact = |state| ItemFact {
///     item: "issue-12".into(),
///     pull: Some(PullFact { number: 40, state }),
///     needs: vec![],
/// };
/// assert_eq!(fact(PullState::Asked).waits("issue-12"), Some(Waits::Verify { pull: 40 }));
/// assert_eq!(fact(PullState::Passed).waits("issue-12"), Some(Waits::Merge { pull: 40 }));
/// assert_eq!(fact(PullState::Passed).waits("verify-issue-12"), Some(Waits::Merge { pull: 40 }));
/// assert_eq!(fact(PullState::Asked).waits("verify-issue-12"), None, "the verifier works");
/// assert_eq!(fact(PullState::Failed).waits("issue-12"), None);
/// assert_eq!(fact(PullState::Merged).waits("issue-12"), None);
/// let needs = ItemFact { item: "issue-12".into(), pull: None, needs: vec![9] };
/// assert_eq!(needs.waits("issue-12"), Some(Waits::Needs { issues: vec![9] }));
/// assert_eq!(needs.waits("verify-issue-12"), None);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ItemFact {
    pub item: String,
    /// The newest pull request of the item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull: Option<PullFact>,
    /// The open items of its `Needs:` line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<u64>,
}

impl ItemFact {
    /// What a session that holds `claim` waits for, by this fact. The
    /// author waits for the verify and for the merge. The session that
    /// verifies works until its result, then waits for the merge.
    pub fn waits(&self, claim: &str) -> Option<Waits> {
        let verify = claim.starts_with("verify-");
        match self.pull.as_ref().map(|p| (p.number, p.state)) {
            Some((pull, PullState::Asked)) if !verify => Some(Waits::Verify { pull }),
            Some((pull, PullState::Passed)) => Some(Waits::Merge { pull }),
            _ if !verify && !self.needs.is_empty() => Some(Waits::Needs {
                issues: self.needs.clone(),
            }),
            _ => None,
        }
    }
}

/// A pull request of an item, and its state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PullFact {
    pub number: u64,
    pub state: PullState,
}

/// The state of a pull request: from the status `riff/verify` of its
/// head, and from its merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PullState {
    /// Open, with no verify result: it waits for a verify.
    Asked,
    /// The verify passed: it waits for the merge.
    Passed,
    /// The verify failed: the item is free with its work.
    Failed,
    /// Merged.
    Merged,
}

/// `POST /v1/threads`: lists the threads of `me`, with unread counts.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Threads {
    pub me: SessionUri,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ThreadsReply {
    pub threads: Vec<ThreadInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ThreadInfo {
    pub thread: ThreadName,
    pub members: Vec<SessionUri>,
    pub unread: usize,
}

/// `POST /v1/join`: adds `me` to a thread. It makes the thread if it
/// is new.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Join {
    pub me: SessionUri,
    pub thread: ThreadName,
}

/// `POST /v1/leave`: removes `me` from a thread. It is no longer the
/// lead there.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Leave {
    pub me: SessionUri,
    pub thread: ThreadName,
}

/// `POST /v1/post`: adds a message to a thread and wakes each session
/// that `to` selects. The sender and each woken session join the
/// thread.
///
/// With no thread, the post is a direct message: `to` must be one
/// selector with a `session`, and the thread is the direct thread of
/// the two sessions.
///
/// A signed-in sender signs the post with [`Post::sign`] (R195).
///
/// ```
/// use riff_core::dpop::Key;
/// use riff_core::wire::Post;
///
/// let me = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let mut post = Post::new(&me, Some("design".parse()?), vec![], "look");
/// let key = Key::generate();
/// post.sign(&key, 1_000);
/// let content = post.content().unwrap();
/// assert_eq!(content.verify(post.sig.as_ref().unwrap()).unwrap(), key.thumbprint());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Post {
    pub me: SessionUri,
    #[serde(default)]
    pub thread: Option<ThreadName>,
    #[serde(default)]
    pub to: Vec<Selector>,
    pub body: String,
    #[serde(default, skip_serializing_if = "Kind::is_message")]
    pub kind: Kind,
    /// The signed time, in milliseconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<u64>,
    /// The signature of the sender (see [`crate::signed`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
    /// The bytes that `sig` signs: the base64url of the JSON of
    /// [`Content`]. The server and each reader keep them unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
}

impl Post {
    /// A post of kind [`Kind::Message`], with no signature.
    pub fn new(me: &SessionUri, thread: Option<ThreadName>, to: Vec<Selector>, body: &str) -> Post {
        Post {
            me: me.clone(),
            thread,
            to,
            body: body.to_owned(),
            kind: Kind::Message,
            at_ms: None,
            sig: None,
            payload: None,
        }
    }

    /// Signs the post with the device key `key`, at the time `at_ms`
    /// (R195, R196). The post carries the payload and the signature.
    pub fn sign(&mut self, key: &Key, at_ms: u64) {
        self.at_ms = Some(at_ms);
        self.payload = self.content().map(|content| content.payload());
        self.sig = self
            .payload
            .as_deref()
            .map(|payload| crate::signed::sign_payload(payload, key));
    }

    /// What the signature covers. `None` when the post has no signed
    /// time.
    pub fn content(&self) -> Option<Content<'_>> {
        // A new field of a post must say here whether the signature
        // covers it.
        let Post {
            me,
            thread,
            to,
            body,
            kind,
            at_ms,
            sig: _,
            payload: _,
        } = self;
        Some(Content {
            from: me.who(),
            lead: me.lead(),
            thread: thread.as_ref(),
            to,
            body,
            kind: *kind,
            at_ms: (*at_ms)?,
        })
    }
}

/// The kind of a post. A `post` call sends `message`, `status` or
/// `note`.
///
/// ```
/// use riff_core::wire::Kind;
///
/// assert_eq!(serde_json::to_string(&Kind::Status).unwrap(), r#""status""#);
/// assert_eq!("status".parse::<Kind>(), Ok(Kind::Status));
/// assert_eq!(serde_json::to_string(&Kind::Note).unwrap(), r#""note""#);
/// assert_eq!("note".parse::<Kind>(), Ok(Kind::Note));
/// assert!("other".parse::<Kind>().is_err());
/// assert!(Kind::Note.needs_body() && !Kind::Status.needs_body());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A message to read.
    #[default]
    Message,
    /// A status request. Each session that it wakes sets its status
    /// with `status`. It does not post a reply.
    Status,
    /// A note: it informs and wakes no session. A session sees it at its
    /// next `read` (01M3JPMQE6S7YM4HPEVGXWK7ET).
    Note,
    /// A kind that this build does not know. A reader shows the post as
    /// a message.
    #[schemars(skip)]
    Other,
}

/// The set of kinds can grow. A build reads each value that it does not
/// know as [`Kind::Other`]: a text, and each other form of JSON
/// (01M3XSF90E9JYYTC13D9THY4WE).
impl<'de> Deserialize<'de> for Kind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(value
            .as_str()
            .and_then(|text| text.parse().ok())
            .unwrap_or(Kind::Other))
    }
}

impl Kind {
    pub fn is_message(&self) -> bool {
        *self == Kind::Message
    }

    /// True when a post of this kind needs a body. Only a status request
    /// needs none.
    pub fn needs_body(&self) -> bool {
        *self != Kind::Status
    }

    /// True for a kind that a `post` call can have. A kind of a later
    /// build reads as [`Kind::Other`], and no `post` call can have it
    /// (01M3XSF90E9JYYTC13D9THY4WE).
    ///
    /// ```
    /// use riff_core::wire::Kind;
    ///
    /// let read = |json: &str| serde_json::from_str::<Kind>(json).unwrap();
    /// assert_eq!(read(r#""note""#), Kind::Note);
    /// // A kind of a later build can have each form of JSON.
    /// for later in [r#""poll""#, r#"{"poll":"wave"}"#, "7", "null"] {
    ///     assert_eq!(read(later), Kind::Other, "{later}");
    /// }
    /// assert!(Kind::Note.is_post() && !Kind::Other.is_post());
    /// assert_eq!(serde_json::to_string(&Kind::Other).unwrap(), r#""other""#);
    ///
    /// // The schema of a call has only the kinds of a `post` call.
    /// let schema = serde_json::to_value(schemars::schema_for!(Kind)).unwrap();
    /// let kinds: Vec<&str> = schema["oneOf"]
    ///     .as_array()
    ///     .unwrap()
    ///     .iter()
    ///     .map(|kind| kind["const"].as_str().unwrap())
    ///     .collect();
    /// assert_eq!(kinds, ["message", "status", "note"]);
    /// ```
    pub fn is_post(&self) -> bool {
        *self != Kind::Other
    }
}

impl std::str::FromStr for Kind {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "message" => Ok(Kind::Message),
            "status" => Ok(Kind::Status),
            "note" => Ok(Kind::Note),
            _ => Err(format!("no kind {text}: use message, status or note")),
        }
    }
}

/// The reply to `post`. It tells the sender who woke.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Posted {
    pub thread: ThreadName,
    pub seq: u64,
    /// Each session that the message woke.
    #[serde(default)]
    pub woken: Vec<SessionUri>,
    /// Each selector that matched no session.
    #[serde(default)]
    pub unmatched: Vec<Selector>,
}

/// `POST /v1/read`: returns the messages that `me` has not read, or
/// all of them when `all` is true. It marks them as read.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Read {
    pub me: SessionUri,
    pub thread: ThreadName,
    #[serde(default)]
    pub all: bool,
    /// With `all`: the page after this seq, from [`ReadReply::next`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadReply {
    /// At most one page of messages (01M3TBZBX140GJWCV5GZ73Q5Z5).
    pub messages: Vec<Message>,
    /// The seq of the last message of the page, when more messages
    /// follow. Read again for the next page: with `all`, set
    /// [`Read::after`] to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<u64>,
    /// The keys of the user of each sender, to verify the messages.
    #[serde(default, skip_serializing_if = "Keys::is_empty")]
    pub keys: Keys,
    /// True from a riff with no sign-in: it trusts each caller (R211).
    #[serde(default)]
    pub trusted: bool,
}

/// The thumbprints of the device keys of each user, by user: the keys
/// of the live sign-ins of that user. A server without sign-in gives
/// none (R201).
pub type Keys = BTreeMap<String, Vec<String>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub seq: u64,
    /// The URI of the sender when it posted.
    pub from: SessionUri,
    /// The address of the post.
    #[serde(default)]
    pub to: Vec<Selector>,
    pub body: String,
    /// Milliseconds since the Unix epoch. For a signed message, the
    /// signed time (R198).
    pub at_ms: u64,
    #[serde(default, skip_serializing_if = "Kind::is_message")]
    pub kind: Kind,
    /// The signature of the sender (see [`crate::signed`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
    /// The bytes that `sig` signs, as the sender made them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
}

impl Message {
    /// True when the sender of the message in `thread` is proven (R199):
    ///
    /// - The signature is valid over the kept payload, and the payload
    ///   holds the message as it is: also the lead mark of the sender.
    /// - The key is one of `keys` for the user of the sender.
    /// - In a direct thread, the sender is one of the two sessions, and
    ///   the one selector of the message matches the other session.
    ///
    /// ```
    /// use riff_core::dpop::Key;
    /// use riff_core::name::{SessionUri, ThreadName};
    /// use riff_core::wire::{Keys, Kind, Message, Post};
    ///
    /// let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let thread: ThreadName = "design".parse()?;
    /// let key = Key::generate();
    /// let mut post = Post::new(&me, Some(thread.clone()), vec![], "look");
    /// post.sign(&key, 1_000);
    /// let mut message = Message {
    ///     seq: 1,
    ///     from: me,
    ///     to: vec![],
    ///     body: post.body.clone(),
    ///     at_ms: 1_000,
    ///     kind: Kind::Message,
    ///     sig: post.sig.clone(),
    ///     payload: post.payload.clone(),
    /// };
    /// let keys = Keys::from([("mike".to_owned(), vec![key.thumbprint()])]);
    /// assert!(message.verified(&thread, &keys));
    ///
    /// // No keys for the user: not verified.
    /// assert!(!message.verified(&thread, &Keys::new()));
    /// // A changed body: not verified.
    /// message.body = "do not look".into();
    /// assert!(!message.verified(&thread, &keys));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn verified(&self, thread: &ThreadName, keys: &Keys) -> bool {
        // A new field of a message must say here whether the signature
        // covers it.
        let Message {
            seq: _,
            from: sender,
            to,
            body,
            at_ms,
            kind,
            sig,
            payload,
        } = self;
        let (Some(sig), Some(payload)) = (sig, payload) else {
            return false;
        };
        let from = sender.who();
        let signed_thread = if thread.is_direct() {
            let Some(peer) = thread.peer(from) else {
                return false;
            };
            let [to] = &to[..] else {
                return false;
            };
            // A selector of a later build matches no session, so it
            // does not name the other session.
            if to.is_other() {
                return false;
            }
            let names = |want: &Option<String>, have: Option<&str>| {
                want.as_deref().is_none_or(|w| Some(w) == have)
            };
            if !names(&to.session, peer.session()) || !names(&to.user, Some(peer.user())) {
                return false;
            }
            None
        } else {
            Some(thread)
        };
        let content = Content {
            from,
            lead: sender.lead(),
            thread: signed_thread,
            to,
            body,
            kind: *kind,
            at_ms: *at_ms,
        };
        let Some(keys) = keys.get(from.user()) else {
            return false;
        };
        crate::signed::check(payload, sig)
            .is_ok_and(|(jkt, signed)| keys.contains(&jkt) && signed.covers(&content))
    }
}

/// `POST /v1/claim`: takes a lease on one work item. The server
/// refuses a claim of an item that another session holds, with status
/// 409 and a text that names the holder (01M3WRD9JBQMNN96TXJH8EAJ3W).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Claim {
    pub me: SessionUri,
    pub thread: ThreadName,
    pub item: String,
}

/// The reply to a claim that the server took.
///
/// ```
/// use riff_core::wire::ClaimReply;
///
/// // The reply of a server of 1.0.0 has no warning.
/// let old: ClaimReply = serde_json::from_str(r#"{"holder":"riff://ann@heron/acme/app?session=a1"}"#).unwrap();
/// assert_eq!(old.warning, None);
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClaimReply {
    /// The URI of `me` now, with the item in its claims.
    pub holder: SessionUri,
    /// Why a worker would not get this claim, for example a hold of the
    /// item (01M43GSGPJ69TPWPA4935WR8RW). The server grants it, because
    /// the caller is not a worker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// `POST /v1/plan/hold`: a lead holds an item of its repository thread
/// with a reason, so that no worker can claim it
/// (01M43GSGB9ZFHSG0Q83Y50FEGW). A hold of a held item replaces its
/// reason. Only a lead of the thread, the owner or an admin can
/// (01M43GSGGY0QMB5D5EH92M6ZFP).
///
/// ```
/// use riff_core::wire::{Call, Hold};
///
/// let hold = Hold {
///     me: "riff://mike@pangolin/como-technologies/riff?session=a1".parse()?,
///     thread: "como-technologies/riff".parse()?,
///     item: "issue-366".into(),
///     reason: "waits for the word of Mike".into(),
/// };
/// assert_eq!(Hold::PATH, "/v1/plan/hold");
/// assert!(serde_json::to_string(&hold).unwrap().contains(r#""reason":"waits for the word of Mike""#));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Hold {
    pub me: SessionUri,
    /// The repository thread of the item.
    pub thread: ThreadName,
    pub item: String,
    /// Why the item is held: 1 to 200 characters.
    pub reason: String,
}

/// The reply to a hold.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HoldReply {
    /// True when the call made the hold or changed its reason.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub changed: bool,
    /// The session that holds a claim of the item now. A hold does not
    /// end a claim: it stops only the next claim of a worker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<SessionUri>,
}

/// `POST /v1/plan/free`: a lead ends the hold of an item
/// (01M43GSGB9ZFHSG0Q83Y50FEGW). A free of an item with no hold changes
/// nothing.
///
/// ```
/// use riff_core::wire::{Call, Free, FreeReply};
///
/// assert_eq!(Free::PATH, "/v1/plan/free");
/// assert_eq!(serde_json::to_string(&FreeReply::default()).unwrap(), "{}");
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Free {
    pub me: SessionUri,
    /// The repository thread of the item.
    pub thread: ThreadName,
    pub item: String,
}

/// The reply to a free.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FreeReply {
    /// True when the item was held, and the call ended the hold.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub freed: bool,
}

/// The hold of one item: its reason, and who held it and when: the `by`
/// and the time of its `item_held` record (01M43GSGB9ZFHSG0Q83Y50FEGW).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HoldInfo {
    pub reason: String,
    /// The caller of the hold. `None` when the record has no cause.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<By>,
    /// The time of the record, in milliseconds since the Unix epoch.
    pub at_ms: u64,
}

/// `POST /v1/plan`: a session sends the full plan of its repository
/// thread (01M4A4YTNSJR0R1T9JNXPBSKHC). `base` is the position of the
/// `plan_set` record of the plan that the client compared with, or
/// `None` when the server had no plan. A `base` that is not the
/// position on the server gets the code `stale_base`
/// (01M4A4YTR2NKVBPE6BT9EC3X75).
///
/// ```
/// use riff_core::record::{Plan, PlanItem, PlanSet};
/// use riff_core::wire::{Call, SetPlan};
///
/// let plan = SetPlan {
///     me: "riff://mike@pangolin/como-technologies/riff?session=a1".parse()?,
///     base: Some(3001),
///     plan: PlanSet {
///         thread: "como-technologies/riff".parse()?,
///         plan: Plan {
///             wave: None,
///             items: vec![PlanItem { item: "issue-366".into(), needs: vec![] }],
///             done: vec![],
///         },
///     },
/// };
/// assert_eq!(SetPlan::PATH, "/v1/plan");
/// assert_eq!(
///     serde_json::to_string(&plan).unwrap(),
///     r#"{"me":"riff://mike@pangolin/como-technologies/riff?session=a1","base":3001,"plan":{"thread":"como-technologies/riff","items":[{"item":"issue-366","needs":[]}],"done":[]}}"#
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetPlan {
    pub me: SessionUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<u64>,
    pub plan: PlanSet,
}

/// `POST /v1/plan/off`: the server forgets the plan of the repository
/// thread. The holds stay (01M4A4YTNSJR0R1T9JNXPBSKHC).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PlanOff {
    pub me: SessionUri,
    pub thread: ThreadName,
}

/// The reply to a `plan_off`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanOffReply {
    /// True when the thread had a plan, and the call ended it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ended: bool,
}

/// `POST /v1/plan/seen`: a look saw that the plan of the forge is the
/// plan of the server at `position` (01M4A4YTYVHFGK0CJVACJQ8DQ3). It is
/// a signal: it makes no record.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PlanSeen {
    pub me: SessionUri,
    pub thread: ThreadName,
    pub position: u64,
}

/// `POST /v1/plan/show`: the plan of a repository thread, and its holds
/// (01M4A4Z1NKPDBXV2PRZCG86G6A).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PlanShow {
    pub me: SessionUri,
    pub thread: ThreadName,
}

/// The plan of the server for one repository thread
/// (01M4A4Z1NKPDBXV2PRZCG86G6A): the reply of `plan`, `plan_seen` and
/// the query `plan`.
///
/// ```
/// use riff_core::wire::PlanReply;
///
/// // A thread with no plan and no hold.
/// assert_eq!(serde_json::to_string(&PlanReply::default()).unwrap(), "{}");
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanReply {
    /// The plan. `None` when the plan of the thread is off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanShown>,
    /// Each hold of the thread, by its item: also of an item that is not
    /// in the plan.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub holds: BTreeMap<String, HoldInfo>,
}

/// The plan of one repository thread on the server, with its age.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanShown {
    #[serde(flatten)]
    pub plan: Plan,
    /// The position of the `plan_set` record of the plan: the `base` of
    /// the next `plan`.
    pub position: u64,
    /// The time of the `plan_set` record, in milliseconds since the Unix
    /// epoch.
    pub set_ms: u64,
    /// The time of the last `plan` or `plan_seen`, in milliseconds since
    /// the Unix epoch. `None` when no look came since the start of the
    /// server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_ms: Option<u64>,
    /// True when no look came for `PLAN_TTL` (01M4A4Z1QTHYXZDMCP9DZ39WVT).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
    /// The session that holds a claim of each item of the plan with a
    /// claim.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub holders: BTreeMap<String, SessionUri>,
}

/// `POST /v1/release`: frees a claim. Only its holder can.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Release {
    pub me: SessionUri,
    pub thread: ThreadName,
    pub item: String,
}

/// The reply to a release.
///
/// ```
/// use riff_core::wire::ReleaseReply;
///
/// assert_eq!(serde_json::to_string(&ReleaseReply::default()).unwrap(), "{}");
/// let last: ReleaseReply = serde_json::from_str(r#"{"must_clear":true}"#).unwrap();
/// assert!(last.must_clear);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseReply {
    /// True when the release was the last claim of a worker: the worker
    /// must clear its context before its next claim (01M3X9XB37TQCXWPNFZRMRGJB4).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub must_clear: bool,
}

/// `POST /v1/release/for`: the lead of a user frees the claim of
/// another session of that user (01M3WG243BW7P6E1ME0DFNQF8C). The
/// server posts a note to the thread of the claim.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseFor {
    pub me: SessionUri,
    pub thread: ThreadName,
    pub item: String,
    /// The session that holds the item: its session ID, or the start of
    /// it.
    pub session: String,
}

/// `POST /v1/lead`: makes `me` the lead of its user in its repository.
/// It replaces the old lead.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Lead {
    pub me: SessionUri,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LeadReply {
    /// The URI of `me` now, with `lead=true`.
    pub lead: SessionUri,
    /// The old lead, when another session was the lead.
    #[serde(default)]
    pub replaced: Option<SessionUri>,
}

/// `POST /v1/start`: a new start of the session: a new agent process,
/// a resume or a `/clear` (01M3JEE1QQCFS5TMZW5N2DAD2D). Its claims are free
/// at once. It keeps its ID, its threads and its lead. The call says
/// why the session starts, and if it is a worker (01M3X9X9M079WGFPJZHNXH9VEP).
///
/// ```
/// use riff_core::wire::{Start, StartReason};
///
/// let start: Start = serde_json::from_str(
///     r#"{"me":"riff://mike@pangolin/o/r?session=a1","reason":"clear","worker":true}"#,
/// ).unwrap();
/// assert_eq!(start.reason, StartReason::Clear);
/// assert!(start.worker);
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Start {
    pub me: SessionUri,
    /// `process`, `resume` or `clear`.
    pub reason: StartReason,
    /// True when the session is a worker: `riff workers run` started it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub worker: bool,
}

/// Why a `session_started` record is there (01M3X9X9M079WGFPJZHNXH9VEP). A `start` call
/// sends `process`, `resume` or `clear`.
///
/// | Reason | Meaning | A fresh context |
/// |---|---|---|
/// | `process` | A new agent process. | Yes |
/// | `resume` | The agent process resumed an old conversation. | No |
/// | `clear` | The agent cleared its context. | Yes |
/// | `join` | The session came with no new start: a `register`. | No |
/// | `other` | A reason that this build does not know. | No |
///
/// ```
/// use riff_core::wire::StartReason;
///
/// let read = |json: &str| serde_json::from_str::<StartReason>(json).unwrap();
/// assert_eq!(read(r#""process""#), StartReason::Process);
/// assert_eq!(read(r#""wake""#), StartReason::Other);
/// // A reason of a later build can have each form of JSON.
/// for later in [r#"{"wake":"timer"}"#, "7", "null", r#"["clear"]"#] {
///     assert_eq!(read(later), StartReason::Other, "{later}");
/// }
/// assert!(StartReason::Clear.is_fresh() && StartReason::Process.is_fresh());
/// assert!(!StartReason::Resume.is_fresh() && !StartReason::Join.is_fresh());
/// assert!(StartReason::Resume.is_start() && !StartReason::Join.is_start());
/// assert_eq!(StartReason::Clear.word(), "clear");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StartReason {
    Process,
    Resume,
    Clear,
    Join,
    /// A reason that this build does not know.
    Other,
}

/// The set of reasons can grow. A build reads each value that it does
/// not know as [`StartReason::Other`]: a text, and each other form of
/// JSON (01M3XM2C18TT8VSKGD77YPZG53).
impl<'de> Deserialize<'de> for StartReason {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let known = [
            StartReason::Process,
            StartReason::Resume,
            StartReason::Clear,
            StartReason::Join,
        ];
        Ok(known
            .into_iter()
            .find(|reason| value.as_str() == Some(reason.word()))
            .unwrap_or(StartReason::Other))
    }
}

impl StartReason {
    /// True for a start with a fresh context: it ends MustClear.
    pub fn is_fresh(self) -> bool {
        matches!(self, StartReason::Process | StartReason::Clear)
    }

    /// True for a reason that a `start` call can have.
    pub fn is_start(self) -> bool {
        matches!(
            self,
            StartReason::Process | StartReason::Resume | StartReason::Clear
        )
    }

    /// The word of the reason, as the JSON has it.
    pub fn word(self) -> &'static str {
        match self {
            StartReason::Process => "process",
            StartReason::Resume => "resume",
            StartReason::Clear => "clear",
            StartReason::Join => "join",
            StartReason::Other => "other",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Started {
    /// Each claim that the start freed.
    #[serde(default)]
    pub freed: Vec<Freed>,
}

/// A claim that a new start freed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Freed {
    pub thread: ThreadName,
    pub item: String,
}

/// `POST /v1/riff`: reads the pauses of the riff, for the place of
/// `me`. It changes nothing (01M3WRD9BSBKS9TN66H29TGTBV).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RiffQuery {
    pub me: SessionUri,
}

/// `POST /v1/pause`: pauses the repository of `me`, or with `riff` the
/// whole riff (01M3XAHZBGSSJB3YX23K88W01K). See
/// 01M3XAHZDSQR263QZVB41CK0MX for who can.
///
/// ```
/// use riff_core::wire::Pause;
///
/// let me = "riff://mike@pangolin/como-technologies/riff".parse()?;
/// let pause = Pause::here(me);
/// assert_eq!(
///     serde_json::to_string(&pause).unwrap(),
///     r#"{"me":"riff://mike@pangolin/como-technologies/riff"}"#
/// );
/// let json = serde_json::to_string(&Pause::whole("riff://mike@pangolin".parse()?)).unwrap();
/// assert_eq!(json, r#"{"me":"riff://mike@pangolin","riff":true}"#);
/// let whole: Pause = serde_json::from_str(r#"{"me":"riff://mike@pangolin","riff":true}"#).unwrap();
/// assert!(whole.riff);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Pause {
    pub me: SessionUri,
    /// True pauses the whole riff, not the repository of `me`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub riff: bool,
    /// Pauses this repository, not the repository of `me`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<ThreadName>,
}

impl Pause {
    /// The pause of the repository of `me`.
    pub fn here(me: SessionUri) -> Pause {
        Pause {
            me,
            riff: false,
            repository: None,
        }
    }

    /// The pause of the whole riff.
    pub fn whole(me: SessionUri) -> Pause {
        Pause {
            riff: true,
            ..Pause::here(me)
        }
    }
}

/// `POST /v1/resume`: resumes the repository of `me`, or with `riff`
/// the whole riff (01M3XAHZBGSSJB3YX23K88W01K). See
/// 01M3XAHZDSQR263QZVB41CK0MX for who can.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Resume {
    pub me: SessionUri,
    /// True resumes the whole riff, not the repository of `me`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub riff: bool,
    /// Resumes this repository, not the repository of `me`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<ThreadName>,
}

impl Resume {
    /// The resume of the repository of `me`.
    pub fn here(me: SessionUri) -> Resume {
        Resume {
            me,
            riff: false,
            repository: None,
        }
    }

    /// The resume of the whole riff.
    pub fn whole(me: SessionUri) -> Resume {
        Resume {
            riff: true,
            ..Resume::here(me)
        }
    }
}

/// The pauses of the riff, as the caller sees them
/// (01M3XAHZJAF6YVDJ7WX74X8RBX): the reply to `/v1/riff`, `/v1/pause`
/// and `/v1/resume`.
///
/// ```
/// use riff_core::wire::{RiffReply, RiffState};
///
/// // The reply of a server from before the pause of a repository.
/// let old: RiffReply = serde_json::from_str(r#"{"state":"paused"}"#).unwrap();
/// assert_eq!(old.state, RiffState::Paused);
/// assert!(old.riff.is_none() && old.repositories.is_empty());
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RiffReply {
    /// The state for the place of the caller: paused when the whole
    /// riff is paused, or when the repository of the caller is paused.
    pub state: RiffState,
    /// True when the call changed a pause.
    #[serde(default)]
    pub changed: bool,
    /// The pause of the whole riff. `None` when the riff runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub riff: Option<PauseInfo>,
    /// Each repository that is paused, in the order of the names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repositories: Vec<RepositoryPause>,
}

/// The pauses of a riff with no pause of a repository: `paused` is a
/// pause of the whole riff that no caller set.
///
/// ```
/// use riff_core::wire::{RiffReply, RiffState};
///
/// let paused = RiffReply::from(RiffState::Paused);
/// assert!(paused.riff.is_some());
/// assert!(RiffReply::from(RiffState::Running).riff.is_none());
/// ```
impl From<RiffState> for RiffReply {
    fn from(state: RiffState) -> RiffReply {
        RiffReply {
            state,
            changed: false,
            riff: (state == RiffState::Paused).then(PauseInfo::default),
            repositories: Vec::new(),
        }
    }
}

impl RiffReply {
    /// The pause of the repository `thread`, when it is paused.
    pub fn repository(&self, thread: &ThreadName) -> Option<&PauseInfo> {
        self.repositories
            .iter()
            .find(|r| &r.repository == thread)
            .map(|r| &r.pause)
    }
}

/// A pause that holds: who set it, and when
/// (01M3XAHZBGSSJB3YX23K88W01K).
///
/// ```
/// use riff_core::record::By;
/// use riff_core::wire::PauseInfo;
///
/// let pause = PauseInfo { by: Some(By::Person("mike".into())), at_ms: 7 };
/// let json = r#"{"by":{"person":"mike"},"at_ms":7}"#;
/// assert_eq!(serde_json::to_string(&pause).unwrap(), json);
/// assert_eq!(serde_json::from_str::<PauseInfo>(json).unwrap(), pause);
/// // The pause of a new riff: no caller set it.
/// assert_eq!(serde_json::to_string(&PauseInfo::default()).unwrap(), r#"{"at_ms":0}"#);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PauseInfo {
    /// The caller that set the pause. `None` when it is not known: the
    /// pause of a new riff, or of a record with no cause.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<By>,
    /// The time of the pause, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub at_ms: u64,
}

/// A repository that is paused, with its pause.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RepositoryPause {
    pub repository: ThreadName,
    #[serde(flatten)]
    pub pause: PauseInfo,
}

/// `POST /v1/idle`: reads the settings of idle workers
/// (01M3Q5A0TF9K49V8Z1ZY9NDF74). It changes nothing
/// (01M3WRD9BSBKS9TN66H29TGTBV).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct IdleQuery {
    pub me: SessionUri,
}

/// `POST /v1/idle/set`: sets each given setting of idle workers, and
/// gives the settings. Only the owner or an admin can; in a riff with
/// no sign-in, each caller can (01M3Q5A0TF9K49V8Z1ZY9NDF74).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetIdle {
    pub me: SessionUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_host: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_secs: Option<u64>,
}

/// The settings of idle workers (01M3Q5A0TF9K49V8Z1ZY9NDF74): the server
/// keeps at most `per_host` idle workers on each host, and stops each
/// other worker that is idle for `after_secs` seconds.
///
/// ```
/// use riff_core::wire::Idle;
///
/// let idle = Idle::default();
/// assert_eq!((idle.per_host, idle.after_secs), (1, 60));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Idle {
    pub per_host: u16,
    pub after_secs: u64,
}

impl Default for Idle {
    fn default() -> Self {
        Idle {
            per_host: 1,
            after_secs: 60,
        }
    }
}

/// The state of a riff. A new riff is paused.
///
/// ```
/// use riff_core::wire::RiffState;
///
/// assert_eq!(RiffState::default(), RiffState::Paused);
/// assert_eq!(serde_json::to_string(&RiffState::Running).unwrap(), r#""running""#);
/// assert_eq!(RiffState::Running.to_string(), "running");
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RiffState {
    /// The sessions stop at their next step and wait. Nobody claims.
    #[default]
    Paused,
    /// The sessions pick and do work.
    Running,
}

impl std::fmt::Display for RiffState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RiffState::Paused => "paused",
            RiffState::Running => "running",
        })
    }
}

/// An event on `GET /v1/watch?uri=…`: a message addressed to the
/// session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Wake {
    pub thread: ThreadName,
    pub seq: u64,
    pub from: SessionUri,
    #[serde(default, skip_serializing_if = "Kind::is_message")]
    pub kind: Kind,
}

/// An event on `GET /v1/tail?thread=…`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Tailed {
    pub thread: ThreadName,
    pub message: Message,
    /// The keys of the user of the sender, to verify the message.
    #[serde(default, skip_serializing_if = "Keys::is_empty")]
    pub keys: Keys,
    /// True from a riff with no sign-in (R211). See
    /// [`ReadReply::trusted`].
    #[serde(default)]
    pub trusted: bool,
}

/// The grant type that swaps an ID token of the sign-in provider for
/// a first pair of riff tokens (RFC 8693 token exchange).
pub const TOKEN_EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";

/// The subject token type of a [`TOKEN_EXCHANGE`] request.
pub const ID_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:id_token";

/// The subject token type of a [`TOKEN_EXCHANGE`] request that swaps a
/// person access token for a session access token (R19).
pub const ACCESS_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:access_token";

/// `POST /v1/token`, as `application/x-www-form-urlencoded`.
///
/// | `grant_type` | Fields |
/// |---|---|
/// | `refresh_token` | `refresh_token` |
/// | [`TOKEN_EXCHANGE`] | `subject_token` (an ID token), `subject_token_type` = [`ID_TOKEN_TYPE`] |
/// | [`TOKEN_EXCHANGE`] | `subject_token` (a person access token), `subject_token_type` = [`ACCESS_TOKEN_TYPE`], `session` |
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct TokenRequest {
    pub grant_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_token_type: Option<String>,
    /// The session ID of a session token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The server that the token is for (RFC 8707). When it is set, it
    /// must be the public URL of the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
}

/// A new pair of riff tokens. Send `access_token` with the `DPoP`
/// scheme and a proof from the device key. Use `refresh_token` once, to
/// get the next pair.
///
/// The reply to a swap for a session token has an empty
/// `refresh_token`: a session token has none
/// (01M3WFVAB44T8EP4QZD4KS7DRF).
///
/// ```
/// use riff_core::wire::TokenReply;
///
/// let json = r#"{"access_token":"a","token_type":"DPoP","expires_in":600,
///     "refresh_token":"","user":"mike"}"#;
/// let session: TokenReply = serde_json::from_str(json).unwrap();
/// assert!(session.refresh_token.is_empty());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TokenReply {
    pub access_token: String,
    /// Always `DPoP`.
    pub token_type: String,
    /// Seconds until the access token expires.
    pub expires_in: u64,
    /// Empty for a session token.
    pub refresh_token: String,
    /// The user part of the session URI, from the sign-in (R36).
    pub user: String,
}

/// `GET /v1/sign-in`: the OpenID Connect provider that `riff login`
/// signs in with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SignInConfig {
    /// The issuer. Its discovery document is at
    /// `<issuer>/.well-known/openid-configuration`.
    pub issuer: String,
    pub client_id: String,
    /// Google asks for the secret of a desktop client too. It is not a
    /// secret: it ships to each person who signs in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// The ID of this riff (01M3JNVBPMZ1K9WX7Q7DP6Y0DH). `riff` keeps it
    /// with its sign-in. Another ID means that the riff is new.
    pub riff_id: String,
}

/// The fields that riff uses from the discovery document of an OpenID
/// Connect provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
}

impl Discovery {
    /// The URL of the discovery document of `issuer`.
    ///
    /// ```
    /// use riff_core::wire::Discovery;
    ///
    /// assert_eq!(
    ///     Discovery::url("https://accounts.google.com/"),
    ///     "https://accounts.google.com/.well-known/openid-configuration"
    /// );
    /// ```
    pub fn url(issuer: &str) -> String {
        format!(
            "{}/.well-known/openid-configuration",
            issuer.trim_end_matches('/')
        )
    }
}

/// `POST /v1/revoke`: ends each sign-in of a person.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Revoke {
    /// The person. Leave it out for the caller. Only an admin names
    /// another person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

/// The reply to [`Revoke`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Revoked {
    pub user: String,
    /// The number of sign-ins that ended.
    pub sign_ins: usize,
}

/// `POST /v1/invite`: adds a member, by verified email.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Invite {
    pub email: String,
}

/// The reply to [`Invite`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Invited {
    /// The email of the member, in lower case.
    pub email: String,
    /// The public address of the riff: the `--public-url` of the
    /// server. The member puts it in `RIFF_SERVER`.
    pub address: String,
}

/// `POST /v1/remove`: removes a member and ends each sign-in of that
/// person.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Remove {
    pub email: String,
}

/// The reply to [`Remove`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Removed {
    /// The email, in lower case.
    pub email: String,
    /// The number of sign-ins that ended.
    pub sign_ins: usize,
}

/// `POST /v1/admin`: the owner makes a person an admin, or an admin a
/// member again.
///
/// ```
/// use riff_core::wire::SetAdmin;
///
/// let add = SetAdmin { email: "bob@gmail.com".into(), admin: true };
/// assert_eq!(
///     serde_json::to_string(&add).unwrap(),
///     r#"{"email":"bob@gmail.com","admin":true}"#
/// );
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SetAdmin {
    pub email: String,
    /// True makes the person an admin. False makes an admin a member
    /// again.
    pub admin: bool,
}

/// The reply to [`SetAdmin`]: the change of its record, with the email
/// in lower case.
pub use crate::record::AdminSet;

/// `POST /v1/owner`: the owner passes the owner role to a member or an
/// admin. The old owner stays an admin.
///
/// ```
/// use riff_core::wire::PassOwner;
///
/// let pass = PassOwner { email: "bob@gmail.com".into() };
/// assert_eq!(serde_json::to_string(&pass).unwrap(), r#"{"email":"bob@gmail.com"}"#);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PassOwner {
    pub email: String,
}

/// The reply to [`PassOwner`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OwnerPassed {
    /// The email of the new owner, in lower case.
    pub owner: String,
    /// The email of the old owner, now an admin.
    pub admin: String,
}

/// `POST /v1/owner/take`: an admin asks for the owner role
/// (01M3N7K3ZAZFGABN7032AYJWEM).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TakeOwner {}

/// The reply to [`TakeOwner`].
///
/// ```
/// use riff_core::wire::OwnerAsked;
///
/// let asked: OwnerAsked = serde_json::from_str(
///     r#"{"admin":"bob@gmail.com","owner":"ada@gmail.com","answer_secs":600}"#,
/// ).unwrap();
/// assert_eq!(asked.owner.as_deref(), Some("ada@gmail.com"));
/// assert!(!asked.already());
///
/// // The owner asked: nothing changed (01M3WRJAFS6W3J2ZRJ6XSW3SB5).
/// let same: OwnerAsked = serde_json::from_str(
///     r#"{"admin":"ada@gmail.com","owner":"ada@gmail.com","answer_secs":0}"#,
/// ).unwrap();
/// assert!(same.already());
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OwnerAsked {
    /// The email of the admin that asked, in lower case.
    pub admin: String,
    /// The email of the owner that answers. `None` when the riff had no
    /// owner: the admin is the owner now. The email of `admin` when the
    /// admin that asked is the owner already: nothing changed.
    pub owner: Option<String>,
    /// The time that the owner has to answer, in seconds. With no
    /// answer, the admin is the owner.
    pub answer_secs: u64,
}

impl OwnerAsked {
    /// True when the admin that asked is the owner already
    /// (01M3WRJAFS6W3J2ZRJ6XSW3SB5).
    pub fn already(&self) -> bool {
        self.owner.as_deref() == Some(self.admin.as_str())
    }
}

/// `POST /v1/owner/deny`: the owner keeps the owner role that an admin
/// asks for (01M3N7K41N03P26BEFFNX5617K).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DenyOwner {}

/// The reply to [`DenyOwner`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OwnerDenied {
    /// The email of the owner, who stays the owner.
    pub owner: String,
    /// The email of the admin that asked.
    pub admin: String,
}

/// `POST /v1/log`: the records of the log of one repository, for an
/// audit (01M3ZWRC11R5M9V1KTF05P240W). Only the owner and the admins
/// can read them.
///
/// ```
/// use riff_core::wire::{Call, LogQuery};
///
/// let query = LogQuery { repo: "acme/app".parse()? };
/// assert_eq!(serde_json::to_string(&query).unwrap(), r#"{"repo":"acme/app"}"#);
/// assert_eq!(LogQuery::PATH, "/v1/log");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LogQuery {
    /// The repository thread, for example `acme/app`.
    pub repo: ThreadName,
}

/// The reply to [`LogQuery`]: each record of the repository
/// ([`Record::of_repository`]), in log order. A post has only its mark
/// ([`Record::for_audit`], 01M3ZWRC3XBFN8FJDGE8XWZ5EA).
///
/// A reader skips a record of a kind that its build does not know, as
/// [`Line::parse`] does (01M43GSMZKCMET3DG07K538EDD). So `riff audit`
/// of one build reads the log of a later server.
///
/// ```
/// use riff_core::wire::LogReply;
///
/// let reply: LogReply = serde_json::from_str(r#"{"records":[
///     {"position":1,"written_at_ms":1,"change":{"reacted":{"emoji":"+1"}}},
///     {"position":2,"written_at_ms":1,"change":{"member_invited":{"email":"ann@acme.io"}}}
/// ]}"#).unwrap();
/// assert_eq!(reply.records.len(), 1);
/// assert_eq!(reply.records[0].envelope.position, 2);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogReply {
    #[serde(deserialize_with = "known_records")]
    pub records: Vec<Record>,
}

/// The records of a [`LogReply`] with a kind that this build knows.
/// A record that does not read is an error.
fn known_records<'de, D>(deserializer: D) -> Result<Vec<Record>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
    let mut records = Vec::with_capacity(values.len());
    for value in values {
        match Line::parse(&value.to_string()).map_err(serde::de::Error::custom)? {
            Line::Record(record) => records.push(*record),
            Line::Unknown { .. } => {}
        }
    }
    Ok(records)
}

/// `POST /v1/members`: shows who may join the riff.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Members {}

/// The reply to [`Members`].
///
/// ```
/// use riff_core::wire::MembersReply;
///
/// let reply: MembersReply = serde_json::from_str(
///     r#"{"owner":"ada@gmail.com","admins":[],"members":["bob@gmail.com"],"allowed_domains":["x.io"]}"#,
/// ).unwrap();
/// assert_eq!(reply.owner.as_deref(), Some("ada@gmail.com"));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MembersReply {
    /// The email of the owner. `None` when the riff has no owner: before
    /// the first sign-in, or after the owner was gone
    /// (01M3Q63NNC6SC03BFCG80M7B4D).
    pub owner: Option<String>,
    /// The email of each admin, sorted: the admins that the owner made
    /// and the admins of the settings (R210). The owner is not in it.
    pub admins: Vec<String>,
    /// The email of each member that is not the owner and not an admin,
    /// sorted (01M3MN157X8N9QKER1AJEPEJVX).
    pub members: Vec<String>,
    /// The allowed domains (R15).
    pub allowed_domains: Vec<String>,
}

/// An OAuth error reply, for example `{"error":"invalid_grant"}`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TokenError {
    pub error: String,
    /// What a person must read, for a refusal that only they can fix,
    /// for example a USER that another account holds (R209).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
}

/// The protected resource metadata of `riff-server` (RFC 9728).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResourceMetadata {
    /// The public URL of the server. Tokens are only for it.
    pub resource: String,
    /// The issuer of each token. It is the server itself.
    pub authorization_servers: Vec<String>,
    /// Always `["header"]`: a token goes only in the Authorization header.
    pub bearer_methods_supported: Vec<String>,
    /// Always `["ES256"]` (RFC 9449).
    pub dpop_signing_alg_values_supported: Vec<String>,
    /// Always true: each token is bound to a device key (R18).
    pub dpop_bound_access_tokens_required: bool,
}

/// The authorization server metadata of `riff-server` (RFC 8414).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServerMetadata {
    pub issuer: String,
    pub token_endpoint: String,
    pub grant_types_supported: Vec<String>,
    /// Empty: the server has no authorization endpoint (R83).
    pub response_types_supported: Vec<String>,
    pub code_challenge_methods_supported: Vec<String>,
    /// Always `["none"]`: `riff` is a public client.
    pub token_endpoint_auth_methods_supported: Vec<String>,
    /// Always `["ES256"]` (RFC 9449).
    pub dpop_signing_alg_values_supported: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::name::Who;

    const MIKE: &str = "riff://mike@pangolin/como-technologies/riff?session=a6cf";
    const BRETT: &str = "riff://brett@heron/como-technologies/riff?session=77e0";

    fn uri(text: &str) -> SessionUri {
        text.parse().unwrap()
    }

    /// The message that the server stores for a signed post.
    fn stored(post: &Post, from: SessionUri) -> Message {
        Message {
            seq: 1,
            from,
            to: post.to.clone(),
            body: post.body.clone(),
            at_ms: post.at_ms.unwrap(),
            kind: post.kind,
            sig: post.sig.clone(),
            payload: post.payload.clone(),
        }
    }

    fn keys(user: &str, key: &Key) -> Keys {
        Keys::from([(user.to_owned(), vec![key.thumbprint()])])
    }

    #[test]
    fn a_direct_message_verifies_only_in_its_own_thread() {
        let key = Key::generate();
        let (mike, brett) = (uri(MIKE), uri(BRETT));
        let to = vec![Selector::session("77e0")];
        let mut post = Post::new(&mike, None, to, "hi");
        post.sign(&key, 5);
        let message = stored(&post, mike.clone());
        let keys = keys("mike", &key);

        let theirs = ThreadName::direct(mike.who(), brett.who());
        assert!(message.verified(&theirs, &keys));

        // The same message in the direct thread of mike and another session.
        let other = Who::new("ada", Some("c3")).unwrap();
        assert!(!message.verified(&ThreadName::direct(mike.who(), &other), &keys));
        // In a direct thread without mike.
        assert!(!message.verified(&ThreadName::direct(brett.who(), &other), &keys));
        // In a named thread.
        assert!(!message.verified(&"design".parse().unwrap(), &keys));
    }

    #[test]
    fn a_direct_message_to_the_lead_must_reach_a_session_of_that_user() {
        let key = Key::generate();
        let mike = uri(MIKE);
        let lead = Who::new("mike", Some("1ead")).unwrap();
        let to = vec![Selector::lead("mike", "como-technologies/riff")];
        let mut post = Post::new(&mike, None, to, "may I?");
        post.sign(&key, 5);
        let message = stored(&post, mike.clone());
        let keys = keys("mike", &key);
        assert!(message.verified(&ThreadName::direct(mike.who(), &lead), &keys));
        let brett = Who::new("brett", Some("77e0")).unwrap();
        assert!(!message.verified(&ThreadName::direct(mike.who(), &brett), &keys));
    }

    #[test]
    fn a_false_sender_is_not_verified() {
        let key = Key::generate();
        let thread: ThreadName = "design".parse().unwrap();
        let mut post = Post::new(&uri(MIKE), Some(thread.clone()), vec![], "go");
        post.sign(&key, 5);
        let keys = keys("mike", &key);
        assert!(stored(&post, uri(MIKE)).verified(&thread, &keys));

        // The stored sender names the lead: another session of mike.
        let lead = uri("riff://mike@pangolin/como-technologies/riff?session=1ead&lead=true");
        assert!(!stored(&post, lead).verified(&thread, &keys));
        // The key of mike does not count for brett.
        assert!(!stored(&post, uri(BRETT)).verified(&thread, &keys));
    }

    #[test]
    fn the_place_and_the_claims_are_not_signed() {
        let key = Key::generate();
        let thread: ThreadName = "design".parse().unwrap();
        let mut post = Post::new(&uri(MIKE), Some(thread.clone()), vec![], "go");
        post.sign(&key, 5);
        let now = "riff://mike@pangolin/other/repo?session=a6cf&claim=issue-6#api";
        assert!(stored(&post, uri(now)).verified(&thread, &keys("mike", &key)));
    }

    #[test]
    fn the_lead_mark_is_signed() {
        let key = Key::generate();
        let keys = keys("mike", &key);
        let thread: ThreadName = "design".parse().unwrap();
        let lead = uri(&format!("{MIKE}&lead=true"));

        // A lead mark added to the sender after the post.
        let mut post = Post::new(&uri(MIKE), Some(thread.clone()), vec![], "merge now");
        post.sign(&key, 5);
        assert!(stored(&post, uri(MIKE)).verified(&thread, &keys));
        assert!(!stored(&post, lead.clone()).verified(&thread, &keys));

        // A post of the lead: its lead mark is signed.
        let mut post = Post::new(&lead, Some(thread.clone()), vec![], "merge now");
        post.sign(&key, 5);
        assert!(stored(&post, lead).verified(&thread, &keys));
        assert!(!stored(&post, uri(MIKE)).verified(&thread, &keys));
    }

    #[test]
    fn a_message_without_a_signature_is_not_verified() {
        let key = Key::generate();
        let thread: ThreadName = "design".parse().unwrap();
        let message = Message {
            seq: 1,
            from: uri(MIKE),
            to: vec![],
            body: "go".into(),
            at_ms: 5,
            kind: Kind::Message,
            sig: None,
            payload: None,
        };
        assert!(!message.verified(&thread, &keys("mike", &key)));
    }

    #[test]
    fn a_signed_post_carries_the_payload_that_it_signs() {
        let key = Key::generate();
        let thread: ThreadName = "design".parse().unwrap();
        let mut post = Post::new(&uri(MIKE), Some(thread.clone()), vec![], "go");
        post.sign(&key, 5);
        let payload = post.payload.clone().unwrap();
        assert_eq!(payload, post.content().unwrap().payload());
        let (jkt, signed) = crate::signed::check(&payload, post.sig.as_ref().unwrap()).unwrap();
        assert_eq!(jkt, key.thumbprint());
        assert!(signed.covers(&post.content().unwrap()));
    }

    #[test]
    fn a_message_needs_its_kept_payload() {
        let key = Key::generate();
        let thread: ThreadName = "design".parse().unwrap();
        let keys = keys("mike", &key);
        let mut post = Post::new(&uri(MIKE), Some(thread.clone()), vec![], "go");
        post.sign(&key, 5);

        let mut message = stored(&post, uri(MIKE));
        message.payload = None;
        assert!(!message.verified(&thread, &keys), "no payload");

        // The payload of another message, with its own signature.
        let mut other = Post::new(&uri(MIKE), Some(thread.clone()), vec![], "stop");
        other.sign(&key, 5);
        let mut message = stored(&post, uri(MIKE));
        message.payload = other.payload.clone();
        message.sig = other.sig.clone();
        assert!(
            !message.verified(&thread, &keys),
            "the payload of another body"
        );
    }

    #[test]
    fn a_new_field_in_the_payload_does_not_stop_the_check() {
        use base64::Engine;
        let key = Key::generate();
        let thread: ThreadName = "design".parse().unwrap();
        let mut post = Post::new(&uri(MIKE), Some(thread.clone()), vec![], "go");
        post.sign(&key, 5);
        // A later build adds `reply_to` to the payload, and signs it.
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let mut json: serde_json::Value =
            serde_json::from_slice(&b64.decode(post.payload.as_ref().unwrap()).unwrap()).unwrap();
        json["reply_to"] = 7.into();
        let payload = b64.encode(serde_json::to_vec(&json).unwrap());
        let mut message = stored(&post, uri(MIKE));
        message.sig = Some(crate::signed::sign_payload(&payload, &key));
        message.payload = Some(payload);
        assert!(message.verified(&thread, &keys("mike", &key)));
    }
}
