//! The waves model: the board of `riff top` and the waves of
//! `riff wave list` (01M56Y8KAPHJMSK89ZW1PA8B0E).
//!
//! # Design
//!
//! On the forge a wave is a milestone named `Wave N`, or `Wave N: NAME`
//! ([`wave_number`]). The UI reads the forge: `riff` asks `gh`, and gives
//! the open issues ([`Issue`]), the issues whose pull request waits for a
//! verify, and the open milestones ([`Milestone`]) to this model. The
//! model is plain data with serde:
//!
//! - [`Issues`]: the title of each open issue of a repository, the
//!   current wave, and the items in verify.
//! - [`Board`]: the board of the current wave of a repository, from its
//!   [`Issues`] and the claims of the sessions of riff-server: `free`,
//!   `claimed` and `verify` (01M3Z9N6X92KT051P10CKKV7EK).
//! - [`Milestone`]: an open milestone, a line of `riff wave list`
//!   (01M509TJRBE2J7HYKKHRJ1ZFR0).
//!
//! ```mermaid
//! flowchart LR
//!     G["the forge, through the UI:<br/>the open issues and pull requests"] --> I["Issues"]
//!     S["riff-server: who"] -- "the claims" --> B["Board"]
//!     I --> B
//!     B --> T["riff top: the board"]
//!     M["the forge: the open milestones"] --> L["Milestone"] --> W["riff wave list"]
//! ```
//!
//! ```
//! use riff_client::sessions::Sessions;
//! use riff_client::waves::{Board, Issue, Issues};
//! use riff_core::wire::SessionInfo;
//!
//! let issue = |number, title: &str| Issue { number, title: title.into(), milestone: Some("Wave 3".into()) };
//! let issues = Issues::of(vec![issue(12, "Claimed"), issue(13, "Free"), issue(14, "Asked")])
//!     .with_verify([14]);
//! let claim: SessionInfo = serde_json::from_value(serde_json::json!({
//!     "uri": "riff://mike@thelio/o/r?session=a1&claim=issue-12", "live": true,
//! }))?;
//! let board = Board::of("o/r", &issues, &Sessions::new(vec![claim], vec![])).unwrap();
//! assert_eq!(board.wave, "Wave 3");
//! assert_eq!((board.free, board.claimed, board.verify), (vec![13], vec![12], vec![14]));
//! # Ok::<(), serde_json::Error>(())
//! ```

use std::collections::{BTreeMap, BTreeSet};

use riff_core::name::Repo;
use serde::{Deserialize, Serialize};

use crate::sessions::{Sessions, Show};

/// An open issue of the forge, as the UI reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    /// The title of its milestone.
    pub milestone: Option<String>,
}

/// The open issues of a repository: the title of each, the current
/// wave, and the items in verify.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issues {
    /// The title of each open issue, by number.
    pub titles: BTreeMap<u64, String>,
    /// The current wave: the open `Wave N` milestone with the lowest N,
    /// and its open issues.
    pub wave: Option<(String, Vec<u64>)>,
    /// The issues whose pull request waits for a verify or for the
    /// merge (01M3Z9N6X92KT051P10CKKV7EK).
    pub verify: BTreeSet<u64>,
}

impl Issues {
    /// The model of the open issues `list`.
    ///
    /// ```
    /// use riff_client::waves::{Issue, Issues};
    ///
    /// let issue = |number, title: &str, milestone: Option<&str>| Issue {
    ///     number,
    ///     title: title.into(),
    ///     milestone: milestone.map(Into::into),
    /// };
    /// let issues = Issues::of(vec![
    ///     issue(12, "Show the wave", Some("Wave 10")),
    ///     issue(13, "Later", Some("Wave 11")),
    ///     issue(9, "Old", Some("Wave 9: Cloud")),
    ///     issue(4, "Kept", Some("Backlog")),
    ///     issue(5, "New", None),
    /// ]);
    /// assert_eq!(issues.titles[&12], "Show the wave");
    /// assert_eq!(issues.wave, Some(("Wave 9: Cloud".into(), vec![9])));
    /// ```
    pub fn of(list: Vec<Issue>) -> Issues {
        let mut waves: BTreeMap<u64, (String, Vec<u64>)> = BTreeMap::new();
        for issue in &list {
            let Some(milestone) = &issue.milestone else {
                continue;
            };
            if let Some(n) = wave_number(milestone) {
                let wave = waves
                    .entry(n)
                    .or_insert_with(|| (milestone.clone(), Vec::new()));
                wave.1.push(issue.number);
            }
        }
        let wave = waves.into_values().next().map(|(title, mut items)| {
            items.sort_unstable();
            (title, items)
        });
        Issues {
            titles: list.into_iter().map(|i| (i.number, i.title)).collect(),
            wave,
            verify: BTreeSet::new(),
        }
    }

    /// These issues with `verify`, the issues whose pull request waits
    /// for a verify or for the merge.
    #[must_use]
    pub fn with_verify(mut self, verify: impl IntoIterator<Item = u64>) -> Issues {
        self.verify = verify.into_iter().collect();
        self
    }

    /// The issues after a new read of the forge: `new`, or `last` when
    /// the read failed (01M3ZC09FA9DZPTHK31XECZ566). So the titles and
    /// the board stay while the network is away.
    ///
    /// ```
    /// use riff_client::waves::{Issue, Issues};
    ///
    /// let wave = Issue { number: 7, title: "Fix".into(), milestone: Some("Wave 3".into()) };
    /// let last = Some(Issues::of(vec![wave]));
    /// // The read failed: the last issues stay.
    /// assert_eq!(Issues::newest(last.clone(), None), last);
    /// // The forge gave no open issue: the board is empty.
    /// let new = Some(Issues::of(vec![]));
    /// assert_eq!(Issues::newest(last, new.clone()), new);
    /// assert_eq!(Issues::newest(None, None), None);
    /// ```
    pub fn newest(last: Option<Issues>, new: Option<Issues>) -> Option<Issues> {
        new.or(last)
    }

    /// The issues of each repository after a new read of the forge for
    /// each repository in `new` (01M3ZC09FA9DZPTHK31XECZ566): a read that
    /// failed keeps the last issues of its repository. A repository
    /// that `new` does not name has no live session any more, and goes.
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use riff_client::waves::{Issue, Issues};
    ///
    /// let wave = |n: u64| Issues::of(vec![Issue {
    ///     number: n,
    ///     title: "a".into(),
    ///     milestone: Some(format!("Wave {n}")),
    /// }]);
    /// let last = BTreeMap::from([("o/r".to_owned(), wave(3)), ("o/old".to_owned(), wave(4))]);
    /// let each = Issues::newest_each(last, vec![("o/r".into(), None), ("o/s".into(), Some(wave(5)))]);
    /// assert_eq!(each.keys().collect::<Vec<_>>(), ["o/r", "o/s"]);
    /// assert_eq!(each["o/r"], wave(3));
    /// ```
    pub fn newest_each(
        mut last: BTreeMap<String, Issues>,
        new: Vec<(String, Option<Issues>)>,
    ) -> BTreeMap<String, Issues> {
        new.into_iter()
            .filter_map(|(repo, read)| {
                let issues = Issues::newest(last.remove(&repo), read)?;
                Some((repo, issues))
            })
            .collect()
    }
}

/// The N of a milestone `Wave N`, or `Wave N: NAME`.
///
/// ```
/// use riff_client::waves::wave_number;
///
/// assert_eq!(wave_number("Wave 10"), Some(10));
/// assert_eq!(wave_number("Wave 5: Cloud"), Some(5));
/// assert_eq!(wave_number("Backlog"), None);
/// assert_eq!(wave_number("Wavelength"), None);
/// ```
pub fn wave_number(title: &str) -> Option<u64> {
    let rest = title.strip_prefix("Wave ")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let after = &rest[digits.len()..];
    if !(after.is_empty() || after.starts_with(':')) {
        return None;
    }
    digits.parse().ok()
}

/// The issue number of a claim `issue-N` or `verify-issue-N`.
///
/// ```
/// use riff_client::waves::issue_of;
///
/// assert_eq!(issue_of("issue-202"), Some(202));
/// assert_eq!(issue_of("verify-issue-196"), Some(196));
/// assert_eq!(issue_of("docs"), None);
/// ```
pub fn issue_of(claim: &str) -> Option<u64> {
    claim
        .strip_prefix("verify-")
        .unwrap_or(claim)
        .strip_prefix("issue-")?
        .parse()
        .ok()
}

/// The repositories that get a board: each repository, `OWNER/REPO`,
/// of a live session on riff-server that `show` shows, and `here`, the
/// repository of the working directory, when `show` has no `--user` and
/// no `--host` and its `--repo` matches (01M42KHN80V49HDDZF953HXDT0).
/// The UI reads the issues of each one from the forge.
///
/// ```
/// use riff_client::sessions::{Sessions, Show};
/// use riff_client::waves::board_repos;
/// use riff_core::wire::SessionInfo;
///
/// let s: SessionInfo = serde_json::from_value(serde_json::json!({
///     "uri": "riff://mike@thelio/o/r?session=a1", "live": true,
/// }))?;
/// let sessions = Sessions::new(vec![s], vec![]);
/// let all = Show::default();
/// assert_eq!(board_repos(&sessions, &all, Some("o/s")), ["o/r".to_owned(), "o/s".to_owned()].into());
/// let ann = Show { user: Some("ann".into()), ..Show::default() };
/// assert!(board_repos(&sessions, &ann, Some("o/s")).is_empty());
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn board_repos(sessions: &Sessions, show: &Show, here: Option<&str>) -> BTreeSet<String> {
    let mut repos: BTreeSet<String> = sessions
        .on_server()
        .filter(|s| s.live && show.shows(s))
        .filter(|s| matches!(s.uri.place().repo(), Repo::Git { .. }))
        .map(|s| s.uri.place().repo_text())
        .collect();
    if let Some(here) = here
        && show.user.is_none()
        && show.host.is_none()
        && show.repo.as_ref().is_none_or(|r| r == here)
    {
        repos.insert(here.to_owned());
    }
    repos
}

/// The board of the current wave of a repository: each open item of
/// the wave in one group (01M3Z9N6X92KT051P10CKKV7EK).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    /// `OWNER/REPO`.
    pub repo: String,
    /// The title of the wave.
    pub wave: String,
    /// The items with no claim, and no pull request in verify.
    pub free: Vec<u64>,
    /// The items with a claim that is not a verify claim.
    pub claimed: Vec<u64>,
    /// The items with a verify claim, and the items with no claim whose
    /// pull request waits for a verify or for the merge: they are no
    /// work for a build.
    pub verify: Vec<u64>,
}

impl Board {
    /// The board of `repo` with its `issues` and the claims of the
    /// sessions on riff-server. Only a claim in `repo` counts: an issue
    /// of another repository can have the same number. `None` when the
    /// repository has no current wave.
    pub fn of(repo: &str, issues: &Issues, sessions: &Sessions) -> Option<Board> {
        let (wave, items) = issues.wave.as_ref()?;
        let mut board = Board {
            repo: repo.to_owned(),
            wave: wave.clone(),
            free: Vec::new(),
            claimed: Vec::new(),
            verify: Vec::new(),
        };
        for &n in items {
            let claims: Vec<&String> = sessions
                .on_server()
                .filter(|s| s.uri.place().repo_text() == repo)
                .flat_map(|s| s.uri.claims().iter())
                .filter(|c| issue_of(c) == Some(n))
                .collect();
            let group = if claims.iter().any(|c| c.starts_with("verify-"))
                || (claims.is_empty() && issues.verify.contains(&n))
            {
                &mut board.verify
            } else if claims.is_empty() {
                &mut board.free
            } else {
                &mut board.claimed
            };
            group.push(n);
        }
        Some(board)
    }
}

/// An open milestone of the forge: a line of `riff wave list`
/// (01M509TJRBE2J7HYKKHRJ1ZFR0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Milestone {
    /// The number of the milestone on the forge, not of the wave.
    pub number: u64,
    pub title: String,
    /// Its open issues and pull requests.
    pub open_issues: u64,
    /// Its closed issues and pull requests.
    pub closed_issues: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use riff_core::wire::SessionInfo;

    /// A sample reply of `who` of riff-server: a claim and a verify
    /// claim in `o/r`, and a claim of the same number in `o/s`.
    const WHO: &str = r#"[
        {"uri": "riff://mike@thelio/o/r?session=a1&claim=issue-12", "live": true},
        {"uri": "riff://mike@thelio/o/r?session=b2&claim=verify-issue-13", "live": true},
        {"uri": "riff://mike@thelio/o/s?session=c3&claim=issue-14", "live": true}
    ]"#;

    fn issues() -> Issues {
        let issue = |number| Issue {
            number,
            title: format!("item {number}"),
            milestone: Some("Wave 3".into()),
        };
        Issues::of((12..=16).map(issue).collect()).with_verify([15])
    }

    #[test]
    fn the_board_comes_from_a_reply_of_who() {
        let who: Vec<SessionInfo> = serde_json::from_str(WHO).unwrap();
        let sessions = Sessions::new(who, vec![]);
        let board = Board::of("o/r", &issues(), &sessions).unwrap();
        assert_eq!(
            board.free,
            [14, 16],
            "a claim of o/s is not on the board of o/r"
        );
        assert_eq!(board.claimed, [12]);
        assert_eq!(board.verify, [13, 15]);
        assert_eq!(Board::of("o/r", &Issues::default(), &sessions), None);
    }

    #[test]
    fn the_board_repos_come_from_a_reply_of_who() {
        let who: Vec<SessionInfo> = serde_json::from_str(WHO).unwrap();
        let sessions = Sessions::new(who, vec![]);
        let repos = board_repos(&sessions, &Show::default(), None);
        assert_eq!(repos, ["o/r".to_owned(), "o/s".to_owned()].into());
    }

    #[test]
    fn the_model_is_plain_data_with_serde() {
        let issues = issues();
        let json = serde_json::to_string(&issues).unwrap();
        assert_eq!(serde_json::from_str::<Issues>(&json).unwrap(), issues);
        let milestone = Milestone {
            number: 40,
            title: "Wave 32".into(),
            open_issues: 3,
            closed_issues: 20,
        };
        let json = serde_json::to_string(&milestone).unwrap();
        assert_eq!(serde_json::from_str::<Milestone>(&json).unwrap(), milestone);
    }
}
