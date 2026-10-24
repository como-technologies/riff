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
//! - [`Issues`]: the title of each open issue of a repository, its
//!   waves, the current wave first, and the items in verify.
//! - [`Board`]: the board of a wave of a repository, from its
//!   [`Issues`], the claims of the sessions of riff-server and the holds
//!   of the lead: `free`, `held`, `claimed`, `verify`, `stuck` and
//!   `tracker` (01M3Z9N6X92KT051P10CKKV7EK, 01M58ATZ1F42DWNNB24C2HJPEY,
//!   01M5ND16V2SVN8A9K7W90VC6X9). `stuck` holds the items whose pull
//!   request cannot merge by itself, with the reason (#279).
//!   `riff top` shows the board of the current wave, `riff wave list`
//!   the board of each wave (01M58ATZ408RBCX0PM0D0ZCY4B).
//! - [`is_tracker`]: a tracker issue has no code of its own. No session
//!   does it, so it is never free (01M5ND16V2SVN8A9K7W90VC6X9).
//! - [`Milestone`]: an open milestone, a line of `riff wave list`
//!   (01M509TJRBE2J7HYKKHRJ1ZFR0).
//!
//! ```mermaid
//! flowchart LR
//!     G["the forge, through the UI:<br/>the open issues and pull requests"] --> I["Issues"]
//!     S["riff-server: who"] -- "the claims" --> B["Board"]
//!     H["riff-server: plan show"] -- "the holds" --> B
//!     I --> B
//!     B --> T["riff top: the board of the current wave"]
//!     B --> W
//!     M["the forge: the open milestones"] --> L["Milestone"] --> W["riff wave list"]
//! ```
//!
//! ```
//! use std::collections::BTreeMap;
//! use riff_client::sessions::Sessions;
//! use riff_client::waves::{Board, Issue, Issues};
//! use riff_core::wire::{HoldInfo, SessionInfo};
//!
//! let issue = |number, title: &str| Issue { number, title: title.into(), milestone: Some("Wave 3".into()), tracker: false };
//! let issues = Issues::of(vec![issue(12, "Claimed"), issue(13, "Free"), issue(14, "Asked"), issue(15, "Held")])
//!     .with_verify([14]);
//! let claim: SessionInfo = serde_json::from_value(serde_json::json!({
//!     "uri": "riff://mike@thelio/o/r?session=a1&claim=issue-12", "live": true,
//! }))?;
//! let holds = BTreeMap::from([(
//!     "issue-15".to_owned(),
//!     HoldInfo { reason: "a reading".into(), by: None, at_ms: 0 },
//! )]);
//! let board = Board::of("o/r", &issues, &Sessions::new(vec![claim], vec![]), &holds).unwrap();
//! assert_eq!(board.wave, "Wave 3");
//! assert_eq!((board.free, board.claimed, board.verify), (vec![13], vec![12], vec![14]));
//! assert_eq!(board.held, [(15, "a reading".to_owned())]);
//! # Ok::<(), serde_json::Error>(())
//! ```

use std::collections::{BTreeMap, BTreeSet};

use riff_core::name::Repo;
use riff_core::wire::HoldInfo;
use serde::{Deserialize, Serialize};

use crate::sessions::{Sessions, Show};

/// An open issue of the forge, as the UI reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    /// The title of its milestone.
    pub milestone: Option<String>,
    /// True for a tracker issue ([`is_tracker`]).
    #[serde(default)]
    pub tracker: bool,
}

/// The open issues of a repository: the title of each, the current
/// wave, and the items in verify.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issues {
    /// The title of each open issue, by number.
    pub titles: BTreeMap<u64, String>,
    /// Each wave with an open issue, in number order: the title of its
    /// milestone and its open issues. The first is the current wave
    /// ([`Issues::wave`]).
    pub waves: Vec<(String, Vec<u64>)>,
    /// The issues whose pull request waits for a verify or for the
    /// merge (01M3Z9N6X92KT051P10CKKV7EK).
    pub verify: BTreeSet<u64>,
    /// The issues whose pull request cannot merge by itself, each with
    /// the reason in a few words, for example `checks` or `conflict`
    /// (#279). They are in `verify` too.
    #[serde(default)]
    pub stuck: BTreeMap<u64, String>,
    /// The tracker issues ([`is_tracker`]).
    #[serde(default)]
    pub trackers: BTreeSet<u64>,
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
    ///     tracker: riff_client::waves::is_tracker(title, []),
    /// };
    /// let issues = Issues::of(vec![
    ///     issue(12, "Show the wave", Some("Wave 10")),
    ///     issue(13, "Later", Some("Wave 11")),
    ///     issue(9, "Old", Some("Wave 9: Cloud")),
    ///     issue(4, "Kept", Some("Backlog")),
    ///     issue(5, "New", None),
    ///     issue(14, "Tracker: the cloud", Some("Wave 10")),
    /// ]);
    /// assert_eq!(issues.titles[&12], "Show the wave");
    /// assert_eq!(issues.trackers, [14].into());
    /// assert_eq!(issues.wave(), Some(&("Wave 9: Cloud".into(), vec![9])));
    /// let titles: Vec<&str> = issues.waves.iter().map(|(t, _)| t.as_str()).collect();
    /// assert_eq!(titles, ["Wave 9: Cloud", "Wave 10", "Wave 11"]);
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
        let waves = waves
            .into_values()
            .map(|(title, mut items)| {
                items.sort_unstable();
                (title, items)
            })
            .collect();
        Issues {
            trackers: list
                .iter()
                .filter(|i| i.tracker)
                .map(|i| i.number)
                .collect(),
            titles: list.into_iter().map(|i| (i.number, i.title)).collect(),
            waves,
            verify: BTreeSet::new(),
            stuck: BTreeMap::new(),
        }
    }

    /// The current wave: the `Wave N` milestone with the lowest N and an
    /// open issue, and its open issues.
    pub fn wave(&self) -> Option<&(String, Vec<u64>)> {
        self.waves.first()
    }

    /// These issues with `verify`, the issues whose pull request waits
    /// for a verify or for the merge.
    #[must_use]
    pub fn with_verify(mut self, verify: impl IntoIterator<Item = u64>) -> Issues {
        self.verify = verify.into_iter().collect();
        self
    }

    /// These issues with `stuck`, the issues whose pull request cannot
    /// merge by itself, each with the reason (#279).
    #[must_use]
    pub fn with_stuck(mut self, stuck: impl IntoIterator<Item = (u64, String)>) -> Issues {
        self.stuck = stuck.into_iter().collect();
        self
    }

    /// The issues after a new read of the forge: `new`, or `last` when
    /// the read failed (01M3ZC09FA9DZPTHK31XECZ566). So the titles and
    /// the board stay while the network is away.
    ///
    /// ```
    /// use riff_client::waves::{Issue, Issues};
    ///
    /// let wave = Issue { number: 7, title: "Fix".into(), milestone: Some("Wave 3".into()), tracker: false };
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
    ///     tracker: false,
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

/// The label of a tracker issue (01M5ND16V2SVN8A9K7W90VC6X9).
pub const TRACKER: &str = "tracker";

/// The start of the title of a tracker issue.
pub const TRACKER_TITLE: &str = "Tracker:";

/// True for a tracker issue: its `labels` have [`TRACKER`], or its
/// `title` starts with [`TRACKER_TITLE`]. A tracker has no code of its
/// own: its parts are separate items. No session does it
/// (01M5ND16V2SVN8A9K7W90VC6X9).
///
/// ```
/// use riff_client::waves::is_tracker;
///
/// assert!(is_tracker("Wave 37 parts", ["bug", "tracker"]));
/// assert!(is_tracker("Tracker: the cloud", []));
/// assert!(!is_tracker("Fix the tracker", ["bug"]));
/// ```
pub fn is_tracker<'a>(title: &str, labels: impl IntoIterator<Item = &'a str>) -> bool {
    title.trim_start().starts_with(TRACKER_TITLE) || labels.into_iter().any(|l| l == TRACKER)
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

/// The board of a wave of a repository: each open item of the wave in
/// one group (01M3Z9N6X92KT051P10CKKV7EK, 01M58ATZ1F42DWNNB24C2HJPEY).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    /// `OWNER/REPO`.
    pub repo: String,
    /// The title of the wave.
    pub wave: String,
    /// The items with no claim, no pull request in verify and no hold.
    pub free: Vec<u64>,
    /// The items with no claim and no pull request in verify that the
    /// lead holds, each with the reason of its hold.
    pub held: Vec<(u64, String)>,
    /// The items with a claim that is not a verify claim.
    pub claimed: Vec<u64>,
    /// The items with a verify claim, and the items with no claim whose
    /// pull request waits for a verify or for the merge: they are no
    /// work for a build.
    pub verify: Vec<u64>,
    /// The items with no claim whose pull request cannot merge by itself,
    /// each with the reason: `checks`, `conflict` or `auto-merge off`
    /// (#279). They wait for a session or for the lead, not for the
    /// merge.
    #[serde(default)]
    pub stuck: Vec<(u64, String)>,
    /// The tracker issues with no claim and no pull request in verify
    /// ([`is_tracker`]). They are never free: no session does them.
    #[serde(default)]
    pub tracker: Vec<u64>,
}

impl Board {
    /// The board of the current wave of `repo` ([`Issues::wave`]), with
    /// its `issues`, the claims of the sessions on riff-server and the
    /// `holds` of its repository thread. `None` when the repository has
    /// no current wave. See [`Board::each`].
    pub fn of(
        repo: &str,
        issues: &Issues,
        sessions: &Sessions,
        holds: &BTreeMap<String, HoldInfo>,
    ) -> Option<Board> {
        let (wave, items) = issues.wave()?;
        Some(Board::wave(repo, wave, items, issues, sessions, holds))
    }

    /// The board of each wave of `repo` with an open issue, in number
    /// order. Only a claim in `repo` counts: an issue of another
    /// repository can have the same number. A claim wins over a hold: a
    /// session can claim a held item. A hold names its item, `issue-N`.
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use riff_client::sessions::Sessions;
    /// use riff_client::waves::{Board, Issue, Issues};
    /// use riff_core::wire::{HoldInfo, SessionInfo};
    ///
    /// let issue = |number, wave: &str| Issue { number, title: "t".into(), milestone: Some(wave.into()), tracker: false };
    /// let issues = Issues::of(vec![
    ///     issue(12, "Wave 3"), issue(13, "Wave 3"), issue(14, "Wave 3"), issue(20, "Wave 4"),
    /// ]);
    /// let claim: SessionInfo = serde_json::from_value(serde_json::json!({
    ///     "uri": "riff://mike@thelio/o/r?session=a1&claim=issue-12", "live": true,
    /// }))?;
    /// let hold = |reason: &str| HoldInfo { reason: reason.into(), by: None, at_ms: 0 };
    /// let holds = BTreeMap::from([
    ///     ("issue-12".to_owned(), hold("later")),
    ///     ("issue-13".to_owned(), hold("a reading at 19:40 UTC")),
    /// ]);
    /// let boards = Board::each("o/r", &issues, &Sessions::new(vec![claim], vec![]), &holds);
    /// assert_eq!(boards.len(), 2);
    /// let (now, next) = (&boards[0], &boards[1]);
    /// assert_eq!(now.wave, "Wave 3");
    /// assert_eq!(now.claimed, [12], "a claim wins over a hold");
    /// assert_eq!(now.held, [(13, "a reading at 19:40 UTC".to_owned())]);
    /// assert_eq!(now.free, [14], "a held item is never free");
    /// assert_eq!((next.wave.as_str(), next.free.as_slice()), ("Wave 4", &[20][..]));
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn each(
        repo: &str,
        issues: &Issues,
        sessions: &Sessions,
        holds: &BTreeMap<String, HoldInfo>,
    ) -> Vec<Board> {
        issues
            .waves
            .iter()
            .map(|(wave, items)| Board::wave(repo, wave, items, issues, sessions, holds))
            .collect()
    }

    /// The board of the wave `wave` with its open `items`.
    fn wave(
        repo: &str,
        wave: &str,
        items: &[u64],
        issues: &Issues,
        sessions: &Sessions,
        holds: &BTreeMap<String, HoldInfo>,
    ) -> Board {
        let mut board = Board {
            repo: repo.to_owned(),
            wave: wave.to_owned(),
            free: Vec::new(),
            held: Vec::new(),
            claimed: Vec::new(),
            verify: Vec::new(),
            stuck: Vec::new(),
            tracker: Vec::new(),
        };
        for &n in items {
            let claims: Vec<&String> = sessions
                .on_server()
                .filter(|s| s.uri.place().repo_text() == repo)
                .flat_map(|s| s.uri.claims().iter())
                .filter(|c| issue_of(c) == Some(n))
                .collect();
            if claims.iter().any(|c| c.starts_with("verify-")) {
                board.verify.push(n);
            } else if let Some(why) = issues.stuck.get(&n).filter(|_| claims.is_empty()) {
                board.stuck.push((n, why.clone()));
            } else if claims.is_empty() && issues.verify.contains(&n) {
                board.verify.push(n);
            } else if !claims.is_empty() {
                board.claimed.push(n);
            } else if issues.trackers.contains(&n) {
                board.tracker.push(n);
            } else if let Some(hold) = holds.get(&format!("issue-{n}")) {
                board.held.push((n, hold.reason.clone()));
            } else {
                board.free.push(n);
            }
        }
        board
    }

    /// One line for each group of the board that is not empty, in the
    /// order `free`, `held`, `claimed`, `verify`, `stuck`, `tracker`. With
    /// `paused`, no group `free`: nobody can claim its items
    /// (01M588G5K0B58QXD35AA5MTZ6E). The reason of each hold follows its
    /// items (01M58ATZ1F42DWNNB24C2HJPEY), and so does the reason of each
    /// stuck pull request (#279). The reason of a hold is text of the
    /// lead: the UI makes it safe.
    ///
    /// ```
    /// use riff_client::waves::Board;
    ///
    /// let board = Board {
    ///     repo: "o/r".into(),
    ///     wave: "Wave 3".into(),
    ///     free: vec![14, 15],
    ///     held: vec![(12, "a reading".into()), (13, "a reading".into()), (16, "later".into())],
    ///     claimed: vec![],
    ///     verify: vec![17],
    ///     stuck: vec![(19, "checks".into()), (20, "auto-merge off".into()), (21, "checks".into())],
    ///     tracker: vec![18],
    /// };
    /// assert_eq!(board.groups(false), [
    ///     "free: #14 #15",
    ///     "held: #12 #13 (a reading), #16 (later)",
    ///     "verify: #17",
    ///     "stuck: #19 #21 (checks), #20 (auto-merge off)",
    ///     "tracker: #18",
    /// ]);
    /// assert_eq!(
    ///     board.groups(true),
    ///     [
    ///         "held: #12 #13 (a reading), #16 (later)",
    ///         "verify: #17",
    ///         "stuck: #19 #21 (checks), #20 (auto-merge off)",
    ///         "tracker: #18",
    ///     ]
    /// );
    /// ```
    pub fn groups(&self, paused: bool) -> Vec<String> {
        let list = |items: &[u64]| {
            let items: Vec<String> = items.iter().map(|n| format!("#{n}")).collect();
            items.join(" ")
        };
        // The items with the same reason, in the order of the first.
        let by_reason = |pairs: &[(u64, String)]| {
            let mut reasons: Vec<(&str, Vec<u64>)> = Vec::new();
            for (n, reason) in pairs {
                match reasons.iter_mut().find(|(r, _)| r == reason) {
                    Some((_, items)) => items.push(*n),
                    None => reasons.push((reason, vec![*n])),
                }
            }
            let groups: Vec<String> = reasons
                .iter()
                .map(|(reason, items)| format!("{} ({reason})", list(items)))
                .collect();
            groups.join(", ")
        };
        [
            (
                "free",
                if paused {
                    String::new()
                } else {
                    list(&self.free)
                },
            ),
            ("held", by_reason(&self.held)),
            ("claimed", list(&self.claimed)),
            ("verify", list(&self.verify)),
            ("stuck", by_reason(&self.stuck)),
            ("tracker", list(&self.tracker)),
        ]
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(name, items)| format!("{name}: {items}"))
        .collect()
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
            tracker: false,
        };
        Issues::of((12..=16).map(issue).collect()).with_verify([15])
    }

    #[test]
    fn the_board_comes_from_a_reply_of_who() {
        let who: Vec<SessionInfo> = serde_json::from_str(WHO).unwrap();
        let sessions = Sessions::new(who, vec![]);
        let none = BTreeMap::new();
        let board = Board::of("o/r", &issues(), &sessions, &none).unwrap();
        assert_eq!(
            board.free,
            [14, 16],
            "a claim of o/s is not on the board of o/r"
        );
        assert_eq!(board.claimed, [12]);
        assert_eq!(board.verify, [13, 15]);
        assert!(board.held.is_empty());
        assert_eq!(Board::of("o/r", &Issues::default(), &sessions, &none), None);
    }

    /// A held item with no claim and no pull request in verify is in
    /// `held` with its reason, never in `free`. A claim and a verify win
    /// over a hold. A hold of another name holds nothing
    /// (01M58ATZ1F42DWNNB24C2HJPEY).
    #[test]
    fn a_held_item_is_in_held_and_never_free() {
        let who: Vec<SessionInfo> = serde_json::from_str(WHO).unwrap();
        let sessions = Sessions::new(who, vec![]);
        let hold = |reason: &str| HoldInfo {
            reason: reason.into(),
            by: None,
            at_ms: 1,
        };
        let holds: BTreeMap<String, HoldInfo> = [
            ("issue-12", "claimed anyway"),
            ("issue-14", "VmLck reading at 19:40 UTC"),
            ("issue-15", "in verify"),
            ("verify-issue-16", "not this item"),
        ]
        .into_iter()
        .map(|(item, reason)| (item.to_owned(), hold(reason)))
        .collect();
        let board = Board::of("o/r", &issues(), &sessions, &holds).unwrap();
        assert_eq!(board.free, [16]);
        assert_eq!(board.held, [(14, "VmLck reading at 19:40 UTC".to_owned())]);
        assert_eq!(board.claimed, [12]);
        assert_eq!(board.verify, [13, 15]);
        assert_eq!(
            board.groups(false),
            [
                "free: #16",
                "held: #14 (VmLck reading at 19:40 UTC)",
                "claimed: #12",
                "verify: #13 #15"
            ]
        );
    }

    /// A tracker issue with no claim is in `tracker`, never in `free`,
    /// also with a hold. A claim wins: a session that claims a tracker
    /// shows (01M5ND16V2SVN8A9K7W90VC6X9).
    #[test]
    fn a_tracker_is_in_tracker_and_never_free() {
        let who: Vec<SessionInfo> = serde_json::from_str(WHO).unwrap();
        let sessions = Sessions::new(who, vec![]);
        let issue = |number, title: &str, labels: &[&str]| Issue {
            number,
            title: title.into(),
            milestone: Some("Wave 3".into()),
            tracker: is_tracker(title, labels.iter().copied()),
        };
        let issues = Issues::of(vec![
            issue(12, "Tracker: claimed anyway", &[]),
            issue(14, "Tracker: the cloud", &[]),
            issue(16, "The parts", &["tracker"]),
            issue(17, "Held parts", &["tracker"]),
            issue(18, "An item", &["bug"]),
        ]);
        let holds = BTreeMap::from([(
            "issue-17".to_owned(),
            HoldInfo {
                reason: "later".into(),
                by: None,
                at_ms: 1,
            },
        )]);
        let board = Board::of("o/r", &issues, &sessions, &holds).unwrap();
        assert_eq!(board.free, [18]);
        assert_eq!(board.tracker, [14, 16, 17]);
        assert_eq!(board.claimed, [12]);
        assert!(board.held.is_empty());
        assert_eq!(
            board.groups(true),
            ["claimed: #12", "tracker: #14 #16 #17"],
            "a pause hides no tracker"
        );
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
