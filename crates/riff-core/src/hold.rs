//! The hold of a lead on a release, as a message of the thread
//! [`THREAD`] (01M5SNV78EBAYY4SZ891DM2MQG).
//!
//! A lead holds a version with a message ([`hold_body`]), and ends the
//! hold with another one ([`end_body`]). [`holds`] reads the holds that
//! stand from the lines of the thread: the last message of each lead
//! decides. `riff release status` and `riff release approve` read them
//! as the person. riff-server reads them from its own messages, for the
//! line of riff-bot while the deploy waits ([`crate::wire::BotFact`]).
//!
//! ```
//! use riff_core::hold::{Line, end_body, hold_body, holds};
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

use serde::{Deserialize, Serialize};

use crate::name::ThreadName;
use crate::wire::Message;

/// The thread of the notes of a release and of the holds.
pub const THREAD: &str = "release";

/// The thread [`THREAD`].
///
/// ```
/// assert_eq!(riff_core::hold::thread().to_string(), "release");
/// ```
pub fn thread() -> ThreadName {
    THREAD.parse().expect("release is a thread name")
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
    /// The line of `message`. `proven` is true when the reader proved the
    /// sender: a reader checks the signature, and riff-server holds the
    /// message itself.
    ///
    /// ```
    /// use riff_core::hold::Line;
    /// use riff_core::wire::{Kind, Message};
    ///
    /// let message = Message {
    ///     seq: 1,
    ///     from: "riff://nate@pangolin/o/s?session=l2&lead=true".parse()?,
    ///     to: vec![],
    ///     body: "hi".into(),
    ///     at_ms: 0,
    ///     kind: Kind::Message,
    ///     sig: None,
    ///     payload: None,
    /// };
    /// let line = Line::of(&message, true);
    /// assert_eq!((line.user.as_str(), line.repo.as_str(), line.lead), ("nate", "o/s", true));
    /// assert!(!Line::of(&message, false).lead);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn of(message: &Message, proven: bool) -> Line {
        let from = &message.from;
        Line {
            user: from.who().user().to_owned(),
            repo: from.place().repo_text(),
            lead: from.lead() && proven,
            body: message.body.clone(),
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
    /// use riff_core::hold::Hold;
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
///     riff_core::hold::hold_body("o/r", "v4.4.0", "I migrate\na database"),
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
///     riff_core::hold::end_body("o/r", "v4.4.0"),
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
