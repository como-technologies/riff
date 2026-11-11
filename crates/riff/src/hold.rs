//! What the other leads hear of a release, and the hold of a lead
//! (#1453, 01M5SNV75DRFHR26BT2M8243VV, 01M5SNV78EBAYY4SZ891DM2MQG).
//!
//! # Design
//!
//! A release changes the riff for each lead: the cut, the deploy, the
//! pause and the restart of a daemon. Only the lead that runs the
//! release used to hear of it. Now each step posts one note to each
//! other lead of the riff, in the thread [`THREAD`]. The note names the
//! version and the step, and says what comes next for that lead
//! ([`Step`], [`crate::text::release_note`]).
//!
//! | Step | Who posts the note |
//! |---|---|
//! | cut, publish, approve, check, rollback | the command, as its session ([`tell_leads`]) |
//! | the deploy waits, the deploy passed, the deploy failed | the watch of the daemon, as the person ([`crate::watches`]) |
//! | the daemon starts again on a new riff | the daemon, to the leads on its host ([`crate::daemon::Daemon::tell_leads_restart`]) |
//!
//! The lead of the user and the repository that runs the step gets no
//! note: it knows the step, and the watches wake it.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as lead of como-technologies/riff
//!     participant S as riff-server
//!     participant B as lead of n8behavior/sens-o-matic
//!     A->>S: riff release cut 4.4.0
//!     S-->>B: wake: the lead of como-technologies/riff cut v4.4.0. next for you: ...
//!     B->>S: riff release hold v4.4.0 "I migrate a database"
//!     S-->>A: wake: the lead of n8behavior/sens-o-matic asks to wait for v4.4.0
//!     A->>S: riff release status
//!     S-->>A: hold: the lead of n8behavior/sens-o-matic asks to wait: I migrate a database
//!     Note over A,B: riff release approve v4.4.0 refuses until B ends its hold, or a person passes --over-holds
//! ```
//!
//! # The hold
//!
//! A lead holds a version with `riff release hold vX.Y.Z REASON`. The
//! hold is a message in the thread [`THREAD`] ([`hold_body`]). The lead
//! ends it with `riff release hold vX.Y.Z --end` ([`end_body`]). riff
//! reads the holds of a version from the thread ([`holds`], in
//! [`riff_core::hold`]): the last message of each lead decides. A message
//! counts only when the reader proved the sender and the sender was the
//! lead when it posted ([`Line::lead`]). So the log is the one record of
//! the holds, and no new state is on the server. A command reads the
//! holds as the person ([`read_as_person`]): a read of the lead itself
//! would mark as read the notes that wake it. riff-server reads them
//! from its own messages for the line of riff-bot while the deploy waits.
//!
//! `riff release status` shows each hold. `riff release approve` refuses
//! while one stands, unless a person passes `--over-holds`
//! (01M5SNV7BC5J58WRZRJ5HS4NSJ, 01M5SNV7E775DRY9QY60E8JPJR).

use std::collections::BTreeMap;

use anyhow::Result;
use riff_client::api::{Api, Refusal};
use riff_core::name::{Place, SessionUri};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, SessionInfo};
use serde::{Deserialize, Serialize};

pub use riff_core::hold::{Hold, Line, THREAD, end_body, hold_body, holds, thread};

/// A step of a release that the other leads hear of
/// (01M5SNV75DRFHR26BT2M8243VV).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// `riff release cut`: the release pull request.
    Cut {
        /// The number of the pull request.
        number: u64,
    },
    /// `riff release publish`: the tag and the GitHub release.
    Published,
    /// The job `Deploy` waits for the approval of a person.
    Waits,
    /// `riff release approve`: the riff is paused, the deploy runs.
    Approved,
    /// The job `Deploy` passed.
    Deployed,
    /// The job `Deploy` failed.
    DeployFailed,
    /// `riff release check` passed.
    Checked,
    /// `riff release check` found a failed check.
    CheckFailed,
    /// `riff release check` found no failed check, and one or more checks
    /// could not run.
    CheckUnknown,
    /// `riff release check` found that the deploy had failed.
    NotDeployed,
    /// `riff release rollback`: the deploy of an older release.
    RolledBack,
}

/// The selectors of the leads of the riff that a note goes to: one for
/// each lead in `sessions`. With `except`, the lead of the same user in
/// the same repository gets none.
///
/// ```
/// use riff::hold::leads;
/// use riff_core::name::SessionUri;
/// use riff_core::wire::SessionInfo;
///
/// let session = |uri: &str, lead: bool| -> SessionInfo {
///     let uri: SessionUri = uri.parse().unwrap();
///     serde_json::from_value(serde_json::json!({"uri": uri.with_lead(lead), "live": true})).unwrap()
/// };
/// let sessions = [
///     session("riff://mike@thelio/como-technologies/riff?session=l1", true),
///     session("riff://nate@pangolin/n8behavior/sens-o-matic?session=l2", true),
///     session("riff://nate@pangolin/n8behavior/sens-o-matic?session=w1#issue-3", false),
/// ];
/// let mike: SessionUri = "riff://mike@thelio/como-technologies/riff?session=l1".parse().unwrap();
/// assert_eq!(leads(&sessions, None).len(), 2);
/// let others = leads(&sessions, Some(&mike));
/// assert_eq!(others.len(), 1);
/// assert_eq!(others[0].to_string(), "user=nate,repo=n8behavior/sens-o-matic,lead=true");
/// ```
pub fn leads(sessions: &[SessionInfo], except: Option<&SessionUri>) -> Vec<Selector> {
    let mut to: Vec<Selector> = sessions
        .iter()
        .filter(|s| s.uri.lead())
        .filter(|s| {
            except.is_none_or(|me| {
                s.uri.who().user() != me.who().user()
                    || s.uri.place().repo_text() != me.place().repo_text()
            })
        })
        .map(|s| Selector::lead(s.uri.who().user(), &s.uri.place().repo_text()))
        .collect();
    to.sort_by_key(ToString::to_string);
    to.dedup();
    to
}

/// Posts `body` as a message in the thread [`THREAD`] to each lead of
/// the riff. With `skip_own`, the lead of the user and the repository of
/// `me` is not woken: it runs the step. It gives the sessions that the
/// post woke; with no other lead, it posts nothing
/// (01M5SNV75DRFHR26BT2M8243VV).
pub async fn tell_leads(
    api: &Api,
    me: &SessionUri,
    skip_own: bool,
    body: &str,
) -> Result<Vec<SessionUri>> {
    let sessions = api.who(me, false).await?;
    let to = leads(&sessions, skip_own.then_some(me));
    if to.is_empty() {
        return Ok(Vec::new());
    }
    let posted = api
        .post(me, Some(&thread()), &to, body, Kind::Message)
        .await?;
    Ok(posted.woken)
}

/// Posts `body` as a message in the thread [`THREAD`] to the one session
/// `session`: the note of the daemon to a lead on its host
/// (01M5SNV7H3739PXW73H14XM4B8). It gives the sessions that the post
/// woke.
pub async fn tell_session(
    api: &Api,
    me: &SessionUri,
    session: &str,
    body: &str,
) -> Result<Vec<SessionUri>> {
    let to = [Selector::session(session)];
    let posted = api
        .post(me, Some(&thread()), &to, body, Kind::Message)
        .await?;
    Ok(posted.woken)
}

/// The lines of the thread [`THREAD`], oldest first, as the person of
/// `place` reads them, on the riff-server `server`. A read of a session
/// would move its cursor over the notes that wake it, so the holds are
/// read as the person. A thread that the server does not know has none.
pub async fn read_as_person(server: &str, place: &Place) -> Result<Vec<Line>> {
    let person = crate::identity::person(place, server)?;
    let api = Api::new(server).signed_in(None)?;
    read(&api, &person).await
}

/// The lines of the thread [`THREAD`], oldest first, as `me` reads
/// them, and marks them read. A thread that the server does not know has
/// none.
pub async fn read(api: &Api, me: &SessionUri) -> Result<Vec<Line>> {
    let messages = match api.read(me, &thread(), true).await {
        Ok(messages) => messages,
        Err(e)
            if e.downcast_ref::<Refusal>()
                .is_some_and(|r| r.status == reqwest::StatusCode::NOT_FOUND) =>
        {
            Vec::new()
        }
        Err(e) => return Err(e),
    };
    Ok(messages
        .iter()
        .map(|c| Line::of(&c.message, c.verified))
        .collect())
}

/// What `riff release hold` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Held {
    /// The lead holds the version now. `told` leads got the message.
    Set {
        /// The number of leads told.
        told: usize,
    },
    /// The lead ended its hold. `told` leads got the message.
    Ended {
        /// The number of leads told.
        told: usize,
    },
    /// `--end`, and the lead had no hold on the version.
    NoHold,
}

/// Holds `tag` for `me`, the lead of its repository
/// (01M5SNV78EBAYY4SZ891DM2MQG). A second hold replaces the reason.
pub async fn hold(api: &Api, me: &SessionUri, tag: &str, reason: &str) -> Result<Held> {
    let body = hold_body(&me.place().repo_text(), tag, reason);
    let told = tell_leads(api, me, false, &body).await?.len();
    Ok(Held::Set { told })
}

/// Ends the hold of `me` on `tag`. With no hold, it posts nothing.
pub async fn end(api: &Api, me: &SessionUri, tag: &str) -> Result<Held> {
    let lines = read_as_person(api.base(), me.place()).await?;
    let mine = |h: &Hold| h.user == me.who().user() && h.repo == me.place().repo_text();
    if !holds(&lines, tag).iter().any(mine) {
        return Ok(Held::NoHold);
    }
    let body = end_body(&me.place().repo_text(), tag);
    let told = tell_leads(api, me, false, &body).await?.len();
    Ok(Held::Ended { told })
}

/// The lead of `repo` for a note: the words that name who runs a step.
///
/// ```
/// assert_eq!(riff::hold::by("como-technologies/riff"), "the lead of como-technologies/riff");
/// ```
pub fn by(repo: &str) -> String {
    format!("the lead of {repo}")
}

/// The holds of `lines` on each tag of `tags`, for `riff release status`.
pub fn holds_of(lines: &[Line], tags: &[String]) -> BTreeMap<String, Vec<Hold>> {
    tags.iter()
        .map(|tag| (tag.clone(), holds(lines, tag)))
        .filter(|(_, held)| !held.is_empty())
        .collect()
}
