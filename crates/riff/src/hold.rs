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
//! reads the holds of a version from the thread ([`holds`]): the last
//! message of each lead decides. A message counts only when the reader
//! proved the sender and the sender was the lead when it posted
//! ([`Line::lead`]). So the log is the one record of the holds, and no
//! new state is on the server. A command reads the holds as the person
//! ([`read_as_person`]): a read of the lead itself would mark as read the
//! notes that wake it.
//!
//! `riff release status` shows each hold. `riff release approve` refuses
//! while one stands, unless a person passes `--over-holds`
//! (01M5SNV7BC5J58WRZRJ5HS4NSJ, 01M5SNV7E775DRY9QY60E8JPJR).
//!
//! ```
//! use riff::hold::{Line, end_body, hold_body, holds};
//!
//! let line = |user: &str, repo: &str, body: String| Line {
//!     user: user.into(),
//!     repo: repo.into(),
//!     lead: true,
//!     body,
//! };
//! let lines = vec![
//!     line("nate", "n8behavior/sens-o-matic", hold_body("n8behavior/sens-o-matic", "v4.4.0", "I migrate a database")),
//!     line("mike", "como-technologies/strata", hold_body("como-technologies/strata", "v4.4.0", "A demo runs")),
//!     line("mike", "como-technologies/strata", end_body("como-technologies/strata", "v4.4.0")),
//! ];
//! let held = holds(&lines, "v4.4.0");
//! assert_eq!(held.len(), 1);
//! assert_eq!(
//!     held[0].line(),
//!     "the lead of n8behavior/sens-o-matic asks to wait: I migrate a database"
//! );
//! assert!(holds(&lines, "v4.4.1").is_empty());
//! ```

use std::collections::BTreeMap;

use anyhow::Result;
use riff_client::api::{Api, Checked, Refusal};
use riff_core::name::{Place, SessionUri, ThreadName};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, SessionInfo};
use serde::{Deserialize, Serialize};

/// The thread of the notes of a release and of the holds.
pub const THREAD: &str = "release";

/// The thread [`THREAD`].
///
/// ```
/// assert_eq!(riff::hold::thread().to_string(), "release");
/// ```
pub fn thread() -> ThreadName {
    THREAD.parse().expect("release is a thread name")
}

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
    /// `riff release check` found that the deploy had failed.
    NotDeployed,
    /// `riff release rollback`: the deploy of an older release.
    RolledBack,
}

/// A message of the thread [`THREAD`], as [`holds`] reads it. The daemon
/// gives the lines to a session in a container as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    /// The USER of the sender.
    pub user: String,
    /// The repository of the sender, as `OWNER/REPO`.
    pub repo: String,
    /// True when the sender was the lead when it posted, and the reader
    /// proved the sender.
    pub lead: bool,
    /// The text.
    pub body: String,
}

impl Line {
    /// The line of a message that `me` read.
    pub fn of(checked: &Checked) -> Line {
        let from = &checked.message.from;
        Line {
            user: from.who().user().to_owned(),
            repo: from.place().repo_text(),
            lead: from.lead() && checked.verified,
            body: checked.message.body.clone(),
        }
    }
}

/// The hold of one lead on one version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hold {
    /// The USER of the lead.
    pub user: String,
    /// The repository of the lead, as `OWNER/REPO`.
    pub repo: String,
    /// Why the lead asks to wait.
    pub reason: String,
}

impl Hold {
    /// The words that `riff release status`, `riff release approve` and
    /// the line to the owner use (01M5SNV7BC5J58WRZRJ5HS4NSJ).
    ///
    /// ```
    /// use riff::hold::Hold;
    ///
    /// let hold = Hold { user: "nate".into(), repo: "n8behavior/sens-o-matic".into(), reason: "A demo runs.".into() };
    /// assert_eq!(hold.line(), "the lead of n8behavior/sens-o-matic asks to wait: A demo runs.");
    /// ```
    pub fn line(&self) -> String {
        format!("the lead of {} asks to wait: {}", self.repo, self.reason)
    }
}

/// The body of the message that holds `tag`. The reason is one line.
///
/// ```
/// assert_eq!(
///     riff::hold::hold_body("o/r", "v4.4.0", "I migrate\na database"),
///     "The lead of o/r asks to wait for v4.4.0: I migrate a database\nnext: riff release status"
/// );
/// ```
pub fn hold_body(repo: &str, tag: &str, reason: &str) -> String {
    let reason = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("The lead of {repo} asks to wait for {tag}: {reason}\nnext: riff release status")
}

/// The body of the message that ends the hold of `tag`.
///
/// ```
/// assert_eq!(
///     riff::hold::end_body("o/r", "v4.4.0"),
///     "The lead of o/r ends its wait for v4.4.0.\nnext: riff release status"
/// );
/// ```
pub fn end_body(repo: &str, tag: &str) -> String {
    format!("The lead of {repo} ends its wait for {tag}.\nnext: riff release status")
}

/// The first line of a hold: the version and the reason.
fn parsed_hold(body: &str) -> Option<(&str, &str)> {
    let first = body.lines().next()?;
    let (_, rest) = first
        .strip_prefix("The lead of ")?
        .split_once(" asks to wait for ")?;
    rest.split_once(": ")
}

/// The version of the end of a hold.
fn parsed_end(body: &str) -> Option<&str> {
    let first = body.lines().next()?;
    let (_, rest) = first
        .strip_prefix("The lead of ")?
        .split_once(" ends its wait for ")?;
    Some(rest.trim_end_matches('.'))
}

/// The holds on `tag` that stand in `lines`, oldest first: the last
/// message of each lead decides (01M5SNV78EBAYY4SZ891DM2MQG). A message
/// of a session that was no proven lead counts for nothing.
pub fn holds(lines: &[Line], tag: &str) -> Vec<Hold> {
    let mut standing: Vec<Hold> = Vec::new();
    for line in lines.iter().filter(|l| l.lead) {
        let same = |h: &Hold| h.user == line.user && h.repo == line.repo;
        if let Some((held, reason)) = parsed_hold(&line.body)
            && held == tag
        {
            standing.retain(|h| !same(h));
            standing.push(Hold {
                user: line.user.clone(),
                repo: line.repo.clone(),
                reason: reason.to_owned(),
            });
        } else if parsed_end(&line.body) == Some(tag) {
            standing.retain(|h| !same(h));
        }
    }
    standing
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
    Ok(messages.iter().map(Line::of).collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn line(user: &str, repo: &str, lead: bool, body: String) -> Line {
        Line {
            user: user.into(),
            repo: repo.into(),
            lead,
            body,
        }
    }

    /// 01M5SNV78EBAYY4SZ891DM2MQG: the last message of a lead decides,
    /// and a second hold replaces the reason.
    #[test]
    fn the_last_message_of_each_lead_decides() {
        let r = "o/r";
        let lines = [
            line("nate", r, true, hold_body(r, "v1.0.0", "first")),
            line("nate", r, true, hold_body(r, "v1.0.0", "second")),
            line("mike", "o/s", true, hold_body("o/s", "v1.0.0", "other")),
            line("nate", r, true, end_body(r, "v1.0.0")),
            line("nate", r, true, hold_body(r, "v1.0.0", "third")),
        ];
        let held = holds(&lines, "v1.0.0");
        assert_eq!(
            held.iter().map(|h| h.reason.as_str()).collect::<Vec<_>>(),
            ["other", "third"]
        );
    }

    /// 01M5SNV78EBAYY4SZ891DM2MQG: a message of a session that was no
    /// proven lead holds nothing and ends nothing.
    #[test]
    fn a_message_of_no_lead_counts_for_nothing() {
        let r = "o/r";
        let lines = [
            line("nate", r, true, hold_body(r, "v1.0.0", "real")),
            line("mallory", r, false, hold_body(r, "v1.0.0", "forged")),
            line("mallory", r, false, end_body(r, "v1.0.0")),
        ];
        let held = holds(&lines, "v1.0.0");
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].user, "nate");
    }

    /// A hold on one version leaves another version alone. A text that
    /// only looks like a hold holds nothing.
    #[test]
    fn a_hold_is_for_one_version_and_a_note_holds_nothing() {
        let r = "o/r";
        let lines = [
            line("nate", r, true, hold_body(r, "v1.0.0", "x")),
            line(
                "nate",
                r,
                true,
                "The lead of o/r cut the release v1.0.0.".into(),
            ),
            line("nate", r, true, "asks to wait for v1.0.1: x".into()),
        ];
        assert_eq!(holds(&lines, "v1.0.0").len(), 1);
        assert!(holds(&lines, "v1.0.1").is_empty());
    }

    /// The hold of a reason with a colon keeps all of the reason.
    #[test]
    fn a_reason_keeps_its_colons() {
        let r = "o/r";
        let lines = [line(
            "nate",
            r,
            true,
            hold_body(r, "v1.0.0", "step 2: the data"),
        )];
        assert_eq!(holds(&lines, "v1.0.0")[0].reason, "step 2: the data");
    }
}
