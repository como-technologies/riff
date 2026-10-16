//! The calls of the riff tools that talk: the tools of a worker, a
//! verifier and the lead in a container ([`crate::mcp::LEAD_TOOLS`]).
//!
//! # Design
//!
//! Each tool that talks calls one [`Talk`]. Each call gives the same
//! wire type, so each tool and its text stay in one place
//! ([`crate::mcp`]):
//!
//! | Talk | For | Acts on |
//! |---|---|---|
//! | [`Api`] | a session on the host, and the default | riff-server 1.x |
//! | [`crate::peer::talk::Session`] | a session of the riff daemon | the riff of peers of the daemon, with no riff-server (01M519Z3FTAZ20DQKDTW09M6NS) |
//!
//! ```mermaid
//! flowchart LR
//!     T["the riff tools: post, read, tell, status, ..."] --> K{"Talk"}
//!     K -->|"Api"| S["riff-server 1.x"]
//!     K -->|"peer::talk::Session"| D["the riff daemon: its records"]
//! ```

use anyhow::Result;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{
    FreeReply, HoldReply, Kind, LeadReply, Posted, RiffReply, RiffState, SessionInfo, Status,
    ThreadInfo, WhoReply,
};

use crate::api::{Api, Inbox, PauseScope, Told};
use crate::wave::BoxFuture;

/// The calls of the tools that talk. See the design of the module. Each
/// call has the meaning of the call of [`Api`] with the same name.
pub trait Talk: Send + Sync {
    /// The sessions: [`Api::who`].
    fn who<'a>(&'a self, me: &'a SessionUri, all: bool) -> BoxFuture<'a, Result<Vec<SessionInfo>>>;

    /// The sessions and the owner: [`Api::roster`].
    fn roster<'a>(&'a self, me: &'a SessionUri, all: bool) -> BoxFuture<'a, Result<WhoReply>>;

    /// The pauses: [`Api::pauses`].
    fn pauses<'a>(&'a self, me: &'a SessionUri) -> BoxFuture<'a, Result<RiffReply>>;

    /// The threads of `me`: [`Api::threads`].
    fn threads<'a>(&'a self, me: &'a SessionUri) -> BoxFuture<'a, Result<Vec<ThreadInfo>>>;

    /// A post: [`Api::post`].
    fn post<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: Option<&'a ThreadName>,
        to: &'a [Selector],
        body: &'a str,
        kind: Kind,
    ) -> BoxFuture<'a, Result<Posted>>;

    /// The status of `me`: [`Api::status`].
    fn status<'a>(&'a self, me: &'a SessionUri, status: &'a Status) -> BoxFuture<'a, Result<()>>;

    /// A block of `me`: [`Api::blocked`].
    fn blocked<'a>(&'a self, me: &'a SessionUri, reason: &'a str) -> BoxFuture<'a, Result<Told>>;

    /// A direct message: [`Api::tell`].
    fn tell<'a>(
        &'a self,
        me: &'a SessionUri,
        session: &'a str,
        body: &'a str,
    ) -> BoxFuture<'a, Result<Posted>>;

    /// A page of messages: [`Api::inbox_page`].
    fn inbox_page<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: Option<&'a ThreadName>,
        all: bool,
        after: Option<u64>,
    ) -> BoxFuture<'a, Result<Vec<Inbox>>>;

    /// A hold of an item: [`Api::hold`].
    fn hold<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: &'a ThreadName,
        item: &'a str,
        reason: &'a str,
    ) -> BoxFuture<'a, Result<HoldReply>>;

    /// A free of an item: [`Api::free`].
    fn free<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: &'a ThreadName,
        item: &'a str,
    ) -> BoxFuture<'a, Result<FreeReply>>;

    /// `me` takes the lead: [`Api::lead`].
    fn lead<'a>(&'a self, me: &'a SessionUri) -> BoxFuture<'a, Result<LeadReply>>;

    /// A pause or a resume: [`Api::set_pause`].
    fn set_pause<'a>(
        &'a self,
        me: &'a SessionUri,
        scope: &'a PauseScope,
        state: RiffState,
    ) -> BoxFuture<'a, Result<(RiffReply, Vec<Posted>)>>;
}

impl Talk for Api {
    fn who<'a>(&'a self, me: &'a SessionUri, all: bool) -> BoxFuture<'a, Result<Vec<SessionInfo>>> {
        Box::pin(Api::who(self, me, all))
    }

    fn roster<'a>(&'a self, me: &'a SessionUri, all: bool) -> BoxFuture<'a, Result<WhoReply>> {
        Box::pin(Api::roster(self, me, all))
    }

    fn pauses<'a>(&'a self, me: &'a SessionUri) -> BoxFuture<'a, Result<RiffReply>> {
        Box::pin(Api::pauses(self, me))
    }

    fn threads<'a>(&'a self, me: &'a SessionUri) -> BoxFuture<'a, Result<Vec<ThreadInfo>>> {
        Box::pin(Api::threads(self, me))
    }

    fn post<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: Option<&'a ThreadName>,
        to: &'a [Selector],
        body: &'a str,
        kind: Kind,
    ) -> BoxFuture<'a, Result<Posted>> {
        Box::pin(Api::post(self, me, thread, to, body, kind))
    }

    fn status<'a>(&'a self, me: &'a SessionUri, status: &'a Status) -> BoxFuture<'a, Result<()>> {
        Box::pin(Api::status(self, me, status))
    }

    fn blocked<'a>(&'a self, me: &'a SessionUri, reason: &'a str) -> BoxFuture<'a, Result<Told>> {
        Box::pin(Api::blocked(self, me, reason))
    }

    fn tell<'a>(
        &'a self,
        me: &'a SessionUri,
        session: &'a str,
        body: &'a str,
    ) -> BoxFuture<'a, Result<Posted>> {
        Box::pin(Api::tell(self, me, session, body))
    }

    fn inbox_page<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: Option<&'a ThreadName>,
        all: bool,
        after: Option<u64>,
    ) -> BoxFuture<'a, Result<Vec<Inbox>>> {
        Box::pin(Api::inbox_page(self, me, thread, all, after))
    }

    fn hold<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: &'a ThreadName,
        item: &'a str,
        reason: &'a str,
    ) -> BoxFuture<'a, Result<HoldReply>> {
        Box::pin(Api::hold(self, me, thread, item, reason))
    }

    fn free<'a>(
        &'a self,
        me: &'a SessionUri,
        thread: &'a ThreadName,
        item: &'a str,
    ) -> BoxFuture<'a, Result<FreeReply>> {
        Box::pin(Api::free(self, me, thread, item))
    }

    fn lead<'a>(&'a self, me: &'a SessionUri) -> BoxFuture<'a, Result<LeadReply>> {
        Box::pin(Api::lead(self, me))
    }

    fn set_pause<'a>(
        &'a self,
        me: &'a SessionUri,
        scope: &'a PauseScope,
        state: RiffState,
    ) -> BoxFuture<'a, Result<(RiffReply, Vec<Posted>)>> {
        Box::pin(Api::set_pause(self, me, scope, state))
    }
}
