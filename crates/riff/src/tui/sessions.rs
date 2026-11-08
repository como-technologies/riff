//! The Sessions tab of the TUI (#1637, 01M5421633BKPPMANYRVZ76BVP): the
//! one view of the sessions. It does what the live table of `riff top`
//! and the view of the riff daemon of this machine did, and the command
//! `riff view` is gone (01M6VGHPP3ZT5TTMSGAXV3ZK7W).
//!
//! # Design
//!
//! ```text
//! [1 Sessions]  2 Chat•2  3 Waves•  4 Machines  5 Devices  6 Reports
//!  riff running · owner mike · build 10df8a4 · link ok 2s
//!  6 sessions: 1 blocked · 1 done · 3 busy · 1 idle · 1 to update         by repo
//!  ▼ riff › mike  owner  online                                        4 sessions
//!  > ! 3a3f8d5d  #994   worker     thelio   blocked 32m  which of two designs?
//!    ✓ 8f1c2d3e  #1599  container  thelio   done         waits for a verify of PR
//!    ● 5b1e2a90  #1590  container  thelio   busy 12m     Bash: just check
//!    ○ 4e54d4e5  ↑      lead       thelio   idle 2h      monitoring work
//!  ▶ strata · brett, mike · kadomony, pangolin                2 sessions · 2 busy
//! ──3a3f8d5d · #994 Design: the TUI of riff · thelio ─────────────────────────────
//!  which of two designs? (32m ago)
//!  the lead gave no answer
//!  working on #994 Design: the TUI of riff
//!  claim #994   pull request none   riff build current
//! ```
//!
//! The tab draws the sessions model of `riff-client`
//! ([`Sessions`](riff_client::sessions::Sessions)): the
//! sessions of riff-server from `who`, and the container sessions of the
//! riff daemon of this machine from its status, each second. A container
//! session that is on riff-server too is one row, with the phase of its
//! container (01M56Y8KD05JH2J7AMK636WGT7, 01M6VGGW73KKG6Q0TZNBGYN7E0).
//! The words of a session are the words of `riff top`: its state, its
//! detail, its tags and its mark `↑` (01M6VGHPGYF1G3BJ7J1W8ZZM17).
//!
//! - **The summary line** counts the sessions by their [`Mark`], then
//!   the words (`offline`, `paused`, `must clear`), the sessions that
//!   run an old riff, and the leads that wait for a person. On the
//!   right: the place in a long tree, and the top level. With no room,
//!   the top level goes first, then the place. While a filter is on, the
//!   line starts with it (01M6WKCKVJTJZSXVTZDZ9FZPFY):
//!   `filter host:pangolin · 1 of 6 sessions: 1 busy · Esc clears`, and
//!   the key line has `Esc clear the filter`. Enter on a host in the
//!   Machines tab sets the filter, and the note line says so
//!   (01M6WKCKYBMXQHB25V9ZW49DBK).
//! - **The tree** (01M6VGGW73KKG6Q0TZNBGYN7E0,
//!   01M6WKCC98EGKB3MSYKN3X2JMH) has three levels: repository, person,
//!   session ([`By::tab_levels`]). `b` puts the person first. The host
//!   is a column of each session row, after its tag, and no level: the
//!   tree of `riff top` keeps the host as a level. A row with one row
//!   under it takes that row after `›`, as in `riff top`. A group row
//!   ends with the counts of its sessions, or the count of its sessions
//!   with no room for each.
//! - **The order** (01M6VAZKRTVZTY73PQV5GE2WY6,
//!   01M6VGGWAAKQ3HPXZ7JX5FE7B0): in a group, the worst mark first, then
//!   the session ID. The groups by their worst row: `!`, a session that
//!   runs an old riff (`↑`), `✓`, `◐`, `●`, `○`, a word, then `offline`,
//!   then a person with no session, and then by their name.
//! - **The fold** (01M6VGGWAAKQ3HPXZ7JX5FE7B0): a group is open when it
//!   holds a `!`, a `✓` or a `↑` row, or the selected row, or when a
//!   filter is set ([`matches()`], 01M6VGH4JTM81PZH3F4R51VWG5). Each other
//!   group is one folded line that names what it holds, then its counts
//!   (01M6WKCKRN073FHR719KJNG3CQ). The person folds and opens a group:
//!   the choice lives while the TUI runs.
//! - **The marks** (01M6VGGWE75Y1XEXG0TM60P6S5): a `✓` stays until the
//!   person selects its row. A session that turns `!` or `✓` while the
//!   person is in another tab gives the tab the news `•`.
//! - **The detail pane** (01M6VGH4QSE8BP51R3TZ9SJDVY) shows the selected
//!   row under a line with its name, when the terminal has 24 rows or
//!   more and 80 columns or more. A session shows the detail of
//!   `riff top` ([`crate::state::detail`]), its claims, its pull request
//!   and its riff, and on this machine its container, its phase and its
//!   last step. Its line names its host. The Machines tab shows the
//!   facts of a host.
//! - **The narrow layout** (01M6VGH4V01H3CC6GE45FHGF5J) has one column
//!   of rows, fewer words, and no pane. From 50 columns, a session row
//!   has its host, cut to 8 columns (01M6WKCKNSCT73R2DSR01CGN5J). Enter
//!   on a session opens its detail on the whole screen, and Esc goes
//!   back.
//! - **The window:** the tab builds only the rows that fit the body
//!   ([`window`]), so a long tree costs no more to draw than a short one.
//!
//! ```mermaid
//! flowchart TD
//!     M["the model:<br/>each session, each container here"] --> F["the filter: each word matches"]
//!     F --> T["the tree: repository › person › session,<br/>the host in each session row"]
//!     T --> O["the order: the worst row first"]
//!     O --> D["the fold: open with !, ✓, ↑,<br/>the selection, or a choice"]
//!     D --> W["the window: only the rows that fit"]
//! ```
//!
//! # The keys
//!
//! | Key | Does |
//! |---|---|
//! | `↑` `↓` `j` `k`, `PgUp` `PgDn`, `g` `G`, `Home` `End` | Select a row. |
//! | `←` `→`, `Space` | Fold or open a group. `←` on a session selects its group. |
//! | `Enter` | A group: fold or open. A lead of this machine: attach. A worker in a container of a daemon of yours: its Session log in the Reports tab, and with no detail pane, its detail on the whole screen first. Each other session: its detail on the whole screen. |
//! | `l` | Attach to the lead of the repository of the row, else to the first lead of this machine. |
//! | `t` | Take over the selected session, after `y` (`riff takeover NAME`). |
//! | `x` | End the selected session of the daemon, after `y` (`riff daemon end NAME`). |
//! | `p`, `P` | Pause or resume the repository of the row, or the whole riff, after `y`. |
//! | `e` | Open each group, or fold the groups that need no look. |
//! | `b` | Put the person at the top of the tree, or the repository again. |
//! | `/` | Filter. Enter keeps the filter, Esc clears it. |
//! | `Esc` | Leave the detail on the whole screen, or clear the filter and the note line. |
//!
//! Each action calls the code of its CLI command, and asks `y` or `n`
//! first, but the attach (01M6VGH4Y0G25VYBBD5ZV4GG2H). `l`, `t` and `x`
//! work only on a container session of the daemon of this machine. On a
//! session of another host they put a note in the note line
//! (01M6VGHC981887SCEKGBNRPHJV). A click selects a row, a click on `▶`
//! or `▼` folds its group, and a double click presses Enter
//! (01M6VGHCJ8WNT6H9VJYE2HP8SP).

use std::collections::{BTreeMap, BTreeSet};

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use riff_client::sessions::{By, Entry, Level, Live, PersonRow, Phase, out_of_date};
use riff_core::build::Build;
use riff_core::name::{Repo, ThreadName};
use riff_core::wire::{PersonRole, RiffState, SessionInfo, SessionState, Waits};

use super::reports::Source;
use super::view::{Hit, Layout, Placed, style_of, width};
use super::{Action, Effect, Key, Mark, Question, State};
use crate::text::{self, SessionKind, safe};

/// The rows of the detail pane: its line and the lines under it.
pub const PANE: u16 = 10;

/// The fewest rows of a terminal with the detail pane.
pub const PANE_ROWS: u16 = 24;

/// The style of the counts of a group and of a word of less weight.
fn muted() -> Style {
    style_of(crate::style::MUTED)
}

/// The style of the selected row.
const SELECTED: Style = Style::new().add_modifier(Modifier::REVERSED);

/// A row of the tree that the person selects.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Selected {
    /// A group row, by the keys of its last group from the top.
    Group(Vec<String>),
    /// A session row, by its session ID.
    Session(String),
}

/// The choices of the person in the Sessions tab. They live while the
/// TUI runs.
#[derive(Debug, Clone, Default)]
pub struct Choices {
    /// The selected row, and its place at the last key.
    selected: Option<Selected>,
    at: usize,
    /// The folds that the person set: true for open. The key is the top
    /// level (true for the repository) and the keys of the group.
    folds: BTreeMap<(bool, Vec<String>), bool>,
    /// The filter, and true while the person types it.
    filter: String,
    typing: bool,
    /// True when `b` put the person at the top of the tree. The
    /// repository is the top level at the start.
    person_first: bool,
    /// The sessions with a `✓` that the person selected: their `✓` goes.
    pub(super) seen: BTreeSet<String>,
    /// The first row of the tree in the window, and the rows that fit, at
    /// the last draw.
    pub(super) top: usize,
    pub(super) page: usize,
    /// True while the detail of the selected row shows on the whole
    /// screen.
    full: bool,
    /// True when the tab showed its detail pane at the last draw.
    pub(super) pane: bool,
}

impl Choices {
    /// The filter.
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Sets the filter, for example `host:thelio` from the Machines tab
    /// (01M6VFD0M3RHT488D0RQ84RVMC). The tab shows the tree, not the
    /// detail on the whole screen.
    pub(super) fn set_filter(&mut self, filter: String) {
        self.filter = filter;
        self.typing = false;
        self.full = false;
    }

    /// True while the person types the filter.
    pub fn typing(&self) -> bool {
        self.typing
    }

    /// True while the detail shows on the whole screen.
    pub fn full(&self) -> bool {
        self.full
    }

    /// True while a filter is on: it has a word.
    pub fn filtered(&self) -> bool {
        !self.filter.trim().is_empty()
    }

    /// The top level of the tree: the repository, or after `b` the
    /// person.
    pub fn by(&self) -> By {
        if self.person_first {
            By::Person
        } else {
            By::Repo
        }
    }

    /// The fold key of the group with the keys `path`.
    fn fold_key(&self, path: &[String]) -> (bool, Vec<String>) {
        (self.by() == By::Repo, path.to_vec())
    }
}

/// A session of the tab: its entry of the sessions model, and its user,
/// its host and its repository (`OWNER/REPO`, or `-`).
#[derive(Debug, Clone)]
pub struct Item<'a> {
    entry: &'a Entry,
    place: [String; 3],
}

impl<'a> Item<'a> {
    /// The item of `entry`: a session of riff-server at its place, or a
    /// container session of this machine at the place of `me`, with no
    /// repository. A session with no session ID, a person on the command
    /// line, has no row.
    fn of(entry: &'a Entry, me: &riff_core::name::SessionUri) -> Option<Item<'a>> {
        let place = match &entry.server {
            Some(s) => {
                s.uri.who().session()?;
                let place = s.uri.place();
                [
                    s.uri.who().user().to_owned(),
                    place.host().to_owned(),
                    place.repo_text(),
                ]
            }
            None => [
                me.who().user().to_owned(),
                me.place().host().to_owned(),
                "-".to_owned(),
            ],
        };
        Some(Item { entry, place })
    }

    /// The session on riff-server.
    fn info(&self) -> Option<&'a SessionInfo> {
        self.entry.server.as_ref()
    }

    /// The container session of the daemon of this machine.
    fn live(&self) -> Option<&'a Live> {
        self.entry.container.as_ref()
    }

    /// The session ID.
    fn id(&self) -> &'a str {
        self.entry.id().unwrap_or_default()
    }

    /// The first `n` characters of the session ID.
    fn short(&self, n: usize) -> String {
        safe(&self.id().chars().take(n).collect::<String>())
    }

    /// The key of the session at `level`.
    fn key(&self, level: Level) -> &str {
        match level {
            Level::Person => &self.place[0],
            Level::Host => &self.place[1],
            Level::Repo => &self.place[2],
        }
    }

    /// The state that riff-server derives.
    fn state(&self) -> Option<SessionState> {
        self.info().map(crate::state::of)
    }

    /// The mark of the session. A `✓` that the person saw is the mark of
    /// the state.
    fn mark(&self, seen: &BTreeSet<String>) -> Option<Mark> {
        let phase = self
            .entry
            .phase()
            .filter(|phase| *phase != Phase::Ends || !seen.contains(self.id()));
        Mark::of(self.state(), phase)
    }

    /// True when the session runs an old riff (01M514C142K5BM2746F1D9Y448).
    fn old(&self, server: Option<&Build>) -> bool {
        self.info().is_some_and(|s| out_of_date(s, server))
    }

    /// True for the lead of a repository.
    fn lead(&self) -> bool {
        self.info().is_some_and(|s| s.uri.lead())
            || self
                .live()
                .is_some_and(|c| c.lead || c.phase == Phase::Terminal)
    }

    /// The word of the session with no mark: `offline`, `paused`, `must
    /// clear`, or the phase of a container with no session on riff-server.
    fn word(&self) -> &'static str {
        match (self.state(), self.live()) {
            (Some(state), _) => state.word(),
            (None, Some(c)) => text::phase_word(c.phase),
            (None, None) => SessionState::Offline.word(),
        }
    }

    /// The tag of the session: `lead`, `container`, `worker`, `host` or
    /// `daemon` ([`SessionKind`]).
    fn tag(&self) -> &'static str {
        match (self.info(), self.live()) {
            (Some(s), _) => SessionKind::of(s).map_or("", SessionKind::word),
            (None, Some(_)) if self.lead() => SessionKind::Lead.word(),
            (None, Some(_)) => SessionKind::Container.word(),
            (None, None) => "",
        }
    }

    /// The claims of the session: of riff-server, else the item of its
    /// container.
    fn claims(&self) -> Vec<String> {
        match (self.info(), self.live()) {
            (Some(s), _) => s.uri.claims().to_vec(),
            (None, Some(c)) if !self.lead() => vec![c.item.clone()],
            _ => Vec::new(),
        }
    }

    /// The item of the session as a person reads it: `#994` for the
    /// claim `issue-994`, `#9` for `verify-issue-9`, else the claim.
    fn item(&self) -> Option<String> {
        self.claims().first().map(|c| item_of(c))
    }

    /// The time of the state in seconds at `now_ms`: the block, the time
    /// since the session was seen, or the time since its claims changed.
    /// A container with no session on riff-server: the time since its
    /// start. A `✓` has none.
    fn age(&self, mark: Option<Mark>, now_ms: i64) -> Option<u64> {
        if mark == Some(Mark::Done) {
            return None;
        }
        match (self.info(), self.live()) {
            (Some(s), _) => Some(match (crate::state::of(s), &s.blocked) {
                (SessionState::Blocked, Some(block)) => block.secs,
                (SessionState::Offline, _) => s.idle_secs,
                _ => s.claims_secs,
            }),
            (None, Some(c)) => u64::try_from((now_ms - c.since_ms) / 1000).ok(),
            (None, None) => None,
        }
    }

    /// The last words of the row: the reason of a block, the person that
    /// a lead waits for, what the claims wait for, the last step of the
    /// container, the step of the session, its work, or what an idle
    /// session does.
    fn last(&self) -> String {
        let info = self.info();
        let state = self.state();
        if let Some(s) = info
            && let Some(block) = &s.blocked
        {
            if state == Some(SessionState::Blocked) {
                return safe(&block.reason);
            }
            if s.uri.lead() && state == Some(SessionState::Waiting) {
                let user = safe(s.uri.who().user());
                return format!("waiting for {user}: {}", safe(&block.reason));
            }
        }
        if let Some(waits) = info.and_then(|s| s.waits.as_ref()) {
            return waits.to_string();
        }
        if let Some(c) = self.live().filter(|c| !c.last.is_empty()) {
            return safe(c.last.trim_start_matches("> "));
        }
        if let Some(s) = info {
            if let Some(status) = s.status.as_ref().filter(|i| !i.stale) {
                return safe(&status.status.step);
            }
            if let Some(work) = &s.work {
                return crate::state::work(work);
            }
            if state == Some(SessionState::Idle) {
                return crate::state::idle(s).to_owned();
            }
        }
        String::new()
    }

    /// The pull request of the session, from what its claims wait for.
    fn pull(&self) -> Option<u64> {
        match self.info()?.waits.as_ref()? {
            Waits::Verify { pull } | Waits::Merge { pull } => Some(*pull),
            Waits::Needs { .. } => None,
        }
    }

    /// The words that a word of the filter can match, each with its key.
    fn words(&self, seen: &BTreeSet<String>) -> Vec<(&'static str, String)> {
        let mut words = vec![
            ("user", self.place[0].clone()),
            ("host", self.place[1].clone()),
            ("repo", self.place[2].clone()),
            ("id", self.id().to_owned()),
            ("state", self.word().to_owned()),
            ("tag", self.tag().to_owned()),
        ];
        if let Some(mark) = self.mark(seen) {
            words.push(("state", mark.word().to_owned()));
        }
        for claim in self.claims() {
            words.push(("item", item_of(&claim)));
            words.push(("item", claim));
        }
        if let Some(c) = self.live() {
            words.push(("name", c.name.clone()));
        }
        words
    }
}

/// A claim as a person reads it: `#994` for `issue-994`, `verify #9`
/// for `verify-issue-9`, else the claim.
///
/// ```
/// use riff::tui::sessions::item_of;
///
/// assert_eq!(item_of("issue-994"), "#994");
/// assert_eq!(item_of("verify-issue-9"), "verify #9");
/// assert_eq!(item_of("release-v1"), "release-v1");
/// ```
pub fn item_of(claim: &str) -> String {
    match (
        riff_client::waves::issue_of(claim),
        claim.starts_with("verify-"),
    ) {
        (Some(n), false) => format!("#{n}"),
        (Some(n), true) => format!("verify #{n}"),
        (None, _) => safe(claim),
    }
}

/// True when the word `word` of the filter matches one of `words`: a
/// word with no `KEY:` matches each word, `host:thelio` only a host. The
/// match is a part of the word, with no case.
///
/// ```
/// use riff::tui::sessions::matches;
///
/// let words = [("host", "thelio".to_owned()), ("item", "#994".to_owned())];
/// assert!(matches("thel", &words));
/// assert!(matches("host:thelio", &words));
/// assert!(!matches("user:thelio", &words));
/// assert!(matches("#994", &words));
/// assert!(matches("THELIO", &words));
/// assert!(!matches("pangolin", &words));
/// ```
pub fn matches(word: &str, words: &[(&str, String)]) -> bool {
    let word = word.to_lowercase();
    let (key, value) = match word.split_once(':') {
        Some((key, value)) if words.iter().any(|(k, _)| *k == key) => (Some(key), value),
        _ => (None, word.as_str()),
    };
    words
        .iter()
        .filter(|(k, _)| key.is_none_or(|key| key == *k))
        .any(|(_, w)| w.to_lowercase().contains(value))
}

/// The place of a row in its group: the worst mark first
/// (01M6VAZKRTVZTY73PQV5GE2WY6), then a word, then `offline`.
fn rank(mark: Option<Mark>, offline: bool) -> u8 {
    match mark {
        Some(Mark::Blocked) => 0,
        Some(Mark::Done) => 2,
        Some(Mark::Waiting) => 3,
        Some(Mark::Busy) => 4,
        Some(Mark::Idle) => 5,
        None if offline => 7,
        None => 6,
    }
}

/// The rank of a group with no session: after each other group.
const NO_SESSION: u8 = 8;

/// A group row of the tree: a person, a host or a repository, each
/// session under it, and the groups of the next level.
#[derive(Debug, Clone)]
pub struct Group<'a> {
    level: Level,
    key: String,
    /// The keys of the groups from the top, and this key.
    path: Vec<String>,
    items: Vec<Item<'a>>,
    kids: Vec<Group<'a>>,
    /// The rank of its worst row: a session that runs an old riff comes
    /// after `!`.
    rank: u8,
}

/// What the tab knows of the riff to make its tree.
struct Facts<'s> {
    seen: &'s BTreeSet<String>,
    server: Option<&'s Build>,
}

impl Facts<'_> {
    /// The rank of `item` in its group.
    fn rank(&self, item: &Item) -> u8 {
        rank(
            item.mark(self.seen),
            item.state() == Some(SessionState::Offline),
        )
    }

    /// The rank of a group with `item`.
    fn group_rank(&self, item: &Item) -> u8 {
        let rank = self.rank(item);
        if item.old(self.server) {
            rank.min(1)
        } else {
            rank
        }
    }

    /// True when `item` needs a look: a `!`, a `✓` or a `↑`.
    fn needs_look(&self, item: &Item) -> bool {
        matches!(item.mark(self.seen), Some(Mark::Blocked | Mark::Done)) || item.old(self.server)
    }
}

/// The groups of `items` at the first of `levels`, each with the groups
/// of the next levels, under the keys `path`.
fn groups<'a>(
    items: Vec<Item<'a>>,
    levels: &[Level],
    path: &[String],
    facts: &Facts,
) -> Vec<Group<'a>> {
    let Some((&level, rest)) = levels.split_first() else {
        return Vec::new();
    };
    let mut by: BTreeMap<String, Vec<Item<'a>>> = BTreeMap::new();
    for item in items {
        by.entry(item.key(level).to_owned()).or_default().push(item);
    }
    let mut groups: Vec<Group<'a>> = by
        .into_iter()
        .map(|(key, mut items)| {
            items.sort_by_key(|item| (facts.rank(item), item.id()));
            let mut path = path.to_vec();
            path.push(key.clone());
            let rank = items
                .iter()
                .map(|item| facts.group_rank(item))
                .min()
                .unwrap_or(NO_SESSION);
            Group {
                level,
                kids: groups(items.clone(), rest, &path, facts),
                key,
                path,
                items,
                rank,
            }
        })
        .collect();
    groups.sort_by(|a, b| (a.rank, &a.key).cmp(&(b.rank, &b.key)));
    groups
}

/// One row of the tree.
#[derive(Debug, Clone)]
pub enum Row<'g, 'a> {
    /// A group, with each group that it holds alone on its line after
    /// `›`.
    Group {
        chain: Vec<&'g Group<'a>>,
        depth: usize,
        open: bool,
    },
    /// A session.
    Session { item: &'g Item<'a>, depth: usize },
}

impl Row<'_, '_> {
    /// The depth of the row: 0 for the top level.
    fn depth(&self) -> usize {
        match self {
            Row::Group { depth, .. } | Row::Session { depth, .. } => *depth,
        }
    }

    /// The key of the row.
    fn selected(&self) -> Selected {
        match self {
            Row::Group { chain, .. } => Selected::Group(last(chain).path.clone()),
            Row::Session { item, .. } => Selected::Session(item.id().to_owned()),
        }
    }

    /// The items of the row: each session under a group, or the session.
    fn items(&self) -> Vec<&Item<'_>> {
        match self {
            Row::Group { chain, .. } => last(chain).items.iter().collect(),
            Row::Session { item, .. } => vec![item],
        }
    }

    /// The key of the row at `level`, when the row has one: a group of
    /// the chain, or each session under it at that level.
    fn key_at(&self, level: Level) -> Option<String> {
        let items = self.items();
        let first = items.first()?.key(level).to_owned();
        items
            .iter()
            .all(|item| item.key(level) == first)
            .then_some(first)
    }
}

/// The last group of a chain.
fn last<'g, 'a>(chain: &[&'g Group<'a>]) -> &'g Group<'a> {
    chain[chain.len() - 1]
}

/// The tree of the tab, and its rows.
pub struct Tree<'a> {
    groups: Vec<Group<'a>>,
}

impl State {
    /// The items of the tab: each session of the model that the filter
    /// shows.
    fn items(&self) -> Vec<Item<'_>> {
        let words: Vec<&str> = self.sessions.filter.split_whitespace().collect();
        self.model
            .entries
            .iter()
            .filter_map(|entry| Item::of(entry, &self.me))
            .filter(|item| {
                let shown = item.words(&self.sessions.seen);
                words.iter().all(|word| matches(word, &shown))
            })
            .collect()
    }

    /// Each person of the riff, from `who` and the sessions.
    fn people(&self) -> BTreeMap<String, PersonRow> {
        match self.last_look() {
            Some((look, _)) => self.model.people(&look.who.people, &look.who.owner),
            None => BTreeMap::new(),
        }
    }

    /// The build of riff-server.
    fn server(&self) -> Option<&Build> {
        self.last_look().and_then(|(look, _)| look.server.as_ref())
    }

    /// The tree of the tab: the groups of the sessions that the filter
    /// shows ([`By::tab_levels`]), and with the person at the top, each
    /// person with no session that the filter shows.
    pub fn tree(&self) -> Tree<'_> {
        let facts = Facts {
            seen: &self.sessions.seen,
            server: self.server(),
        };
        let by = self.sessions.by();
        let mut groups = groups(self.items(), &by.tab_levels(), &[], &facts);
        if by == By::Person {
            let words: Vec<&str> = self.sessions.filter.split_whitespace().collect();
            for user in self.people().into_keys() {
                let shown = [("user", user.clone())];
                if groups.iter().any(|g| g.key == user)
                    || !words.iter().all(|word| matches(word, &shown))
                {
                    continue;
                }
                groups.push(Group {
                    level: Level::Person,
                    path: vec![user.clone()],
                    key: user,
                    items: Vec::new(),
                    kids: Vec::new(),
                    rank: NO_SESSION,
                });
            }
        }
        Tree { groups }
    }

    /// The rows of `tree` that show: the rows of each open group.
    pub fn rows<'g, 'a>(&self, tree: &'g Tree<'a>) -> Vec<Row<'g, 'a>> {
        let facts = Facts {
            seen: &self.sessions.seen,
            server: self.server(),
        };
        let mut rows = Vec::new();
        self.flatten(&tree.groups, 0, &facts, &mut rows);
        rows
    }

    fn flatten<'g, 'a>(
        &self,
        groups: &'g [Group<'a>],
        depth: usize,
        facts: &Facts,
        rows: &mut Vec<Row<'g, 'a>>,
    ) {
        for group in groups {
            let mut chain = vec![group];
            while let [only] = last(&chain).kids.as_slice() {
                chain.push(only);
            }
            let end = last(&chain);
            let open = self.is_open(end, facts);
            rows.push(Row::Group {
                chain: chain.clone(),
                depth,
                open,
            });
            if !open {
                continue;
            }
            if end.kids.is_empty() {
                rows.extend(end.items.iter().map(|item| Row::Session {
                    item,
                    depth: depth + 1,
                }));
            } else {
                self.flatten(&end.kids, depth + 1, facts, rows);
            }
        }
    }

    /// True when the group `group` is open: the choice of the person,
    /// else with a filter, a row that needs a look, or the selected
    /// session. A group with no session is folded.
    fn is_open(&self, group: &Group, facts: &Facts) -> bool {
        if group.items.is_empty() {
            return false;
        }
        let choices = &self.sessions;
        if let Some(open) = choices.folds.get(&choices.fold_key(&group.path)) {
            return *open;
        }
        choices.filtered()
            || group.items.iter().any(|item| {
                facts.needs_look(item)
                    || choices.selected == Some(Selected::Session(item.id().to_owned()))
            })
    }

    /// The place of the selected row in `rows`: the selected row, else
    /// the row at its old place, else the first session.
    fn at(&self, rows: &[Row]) -> Option<usize> {
        if rows.is_empty() {
            return None;
        }
        let choices = &self.sessions;
        match &choices.selected {
            Some(selected) => rows
                .iter()
                .position(|row| &row.selected() == selected)
                .or(Some(choices.at.min(rows.len() - 1))),
            None => rows
                .iter()
                .position(|row| matches!(row, Row::Session { .. }))
                .or(Some(0)),
        }
    }

    /// The facts of the selected row that an action needs.
    fn picked(&self) -> Option<Picked> {
        let tree = self.tree();
        let rows = self.rows(&tree);
        let row = rows.get(self.at(&rows)?)?;
        let repo = row.key_at(Level::Repo).filter(|repo| repo != "-");
        let session = match row {
            Row::Session { item, .. } => Some(Picked1 {
                name: item.live().map(|c| c.name.clone()),
                lead: item.lead(),
                host: item.key(Level::Host).to_owned(),
                log: self.log_of(item),
            }),
            Row::Group { .. } => None,
        };
        Some(Picked { repo, session })
    }

    /// The Session log of the worker `item` (01M6VT3JS3HRKXEJKDV0C97VZS):
    /// its container on this machine, else the container of the session
    /// in the facts that the daemon of its host sent, when the host is of
    /// the person of the TUI. A container of the daemon is named
    /// `riff-ITEM-ID8`, with the first 8 characters of the session ID. A
    /// lead has none: Enter attaches to it.
    fn log_of(&self, item: &Item) -> Option<Source> {
        if item.lead() {
            return None;
        }
        if let Some(c) = item.live() {
            return Some(Source {
                name: c.name.clone(),
                host: None,
            });
        }
        let [user, host, _] = &item.place;
        let id: String = item.id().chars().take(8).collect();
        if user != self.me.who().user() || id.is_empty() {
            return None;
        }
        let end = format!("-{id}");
        let hosts = riff_client::daemons::hosts(self.model.on_server());
        let name = hosts
            .iter()
            .find(|h| &h.host == host && &h.user == user)?
            .facts
            .sessions
            .iter()
            .find(|s| s.name.ends_with(&end))?
            .name
            .clone();
        let here = host == self.me.place().host();
        Some(Source {
            name,
            host: (!here).then(|| host.clone()),
        })
    }

    /// Selects the row that `to` gives from the place of the selected row
    /// and the count of the rows. A `✓` goes when the person selects its
    /// row.
    fn move_to(&mut self, to: impl Fn(usize, usize) -> usize) {
        let tree = self.tree();
        let rows = self.rows(&tree);
        let Some(at) = self.at(&rows) else {
            return;
        };
        let to = to(at, rows.len()).min(rows.len() - 1);
        let selected = rows[to].selected();
        let ends = match &rows[to] {
            Row::Session { item, .. } => item.entry.phase() == Some(Phase::Ends),
            Row::Group { .. } => false,
        };
        drop(rows);
        drop(tree);
        if let (Selected::Session(id), true) = (&selected, ends) {
            self.sessions.seen.insert(id.clone());
        }
        self.sessions.selected = Some(selected);
        self.sessions.at = to;
    }

    /// Folds or opens the group row at `at`, or with `open`, sets it.
    fn fold(&mut self, at: Option<usize>, open: Option<bool>) {
        let tree = self.tree();
        let rows = self.rows(&tree);
        let Some(at) = at.or_else(|| self.at(&rows)) else {
            return;
        };
        let Some(Row::Group {
            chain, open: now, ..
        }) = rows.get(at)
        else {
            return;
        };
        if last(chain).items.is_empty() {
            return;
        }
        let key = self.sessions.fold_key(&last(chain).path);
        let open = open.unwrap_or(!now);
        let selected = rows[at].selected();
        drop(rows);
        drop(tree);
        self.sessions.folds.insert(key, open);
        self.sessions.selected = Some(selected);
        self.sessions.at = at;
    }

    /// `e`: opens each group, or when each group is open, folds the
    /// groups that need no look.
    fn open_each(&mut self) {
        let tree = self.tree();
        let rows = self.rows(&tree);
        let folded = rows.iter().any(|row| {
            matches!(row, Row::Group { chain, open: false, .. } if !last(chain).items.is_empty())
        });
        let mut keys = Vec::new();
        fn walk(groups: &[Group], keys: &mut Vec<Vec<String>>) {
            for group in groups {
                keys.push(group.path.clone());
                walk(&group.kids, keys);
            }
        }
        walk(&tree.groups, &mut keys);
        drop(rows);
        drop(tree);
        let by = self.sessions.by() == By::Repo;
        self.sessions.folds.retain(|(repo, _), _| *repo != by);
        if folded {
            for key in keys {
                self.sessions.folds.insert((by, key), true);
            }
        }
    }

    /// The key `key` of the person in the Sessions tab.
    pub(super) fn on_sessions_key(&mut self, key: Key) -> Option<Action> {
        if self.sessions.typing {
            match key {
                Key::Char(c) => self.sessions.filter.push(c),
                Key::Backspace => {
                    self.sessions.filter.pop();
                }
                Key::Enter => self.sessions.typing = false,
                Key::Esc => self.clear_filter(),
                _ => {}
            }
            return None;
        }
        let page = self.sessions.page.max(1);
        match key {
            Key::Up => self.move_to(|at, _| at.saturating_sub(1)),
            Key::Down => self.move_to(|at, _| at + 1),
            Key::PageUp => self.move_to(|at, _| at.saturating_sub(page)),
            Key::PageDown => self.move_to(|at, _| at + page),
            Key::First => self.move_to(|_, _| 0),
            Key::Last => self.move_to(|_, len| len.saturating_sub(1)),
            Key::Left => return self.left(),
            Key::Right => self.right(),
            Key::Char(' ') => self.fold(None, None),
            Key::Enter => return self.enter(),
            Key::Char('l') => return self.lead(),
            Key::Char('t') => self.takeover(),
            Key::Char('x') => self.end(),
            Key::Char('p') => self.pause(false),
            Key::Char('P') => self.pause(true),
            Key::Char('e') => self.open_each(),
            Key::Char('b') => self.sessions.person_first = !self.sessions.person_first,
            Key::Char('/') => self.sessions.typing = true,
            Key::Esc if self.sessions.full => self.sessions.full = false,
            Key::Esc => self.clear_filter(),
            Key::Row(at) => self.move_to(|_, _| at),
            Key::Fold(at) => self.fold(Some(at), None),
            _ => {}
        }
        None
    }

    /// Esc: clears the filter. With a filter, the note goes too: it can
    /// name the filter, as after Enter in the Machines tab.
    fn clear_filter(&mut self) {
        if self.sessions.filtered() {
            self.note = None;
        }
        self.sessions.filter.clear();
        self.sessions.typing = false;
    }

    /// `←`: folds an open group, else selects the group of the row.
    fn left(&mut self) -> Option<Action> {
        let tree = self.tree();
        let rows = self.rows(&tree);
        let at = self.at(&rows)?;
        let open_group = matches!(&rows[at], Row::Group { chain, open: true, .. } if !last(chain).items.is_empty());
        let depth = rows[at].depth();
        let parent = rows[..at].iter().rposition(|row| row.depth() < depth);
        drop(rows);
        drop(tree);
        if open_group {
            self.fold(Some(at), Some(false));
        } else if let Some(parent) = parent {
            self.move_to(|_, _| parent);
        }
        None
    }

    /// `→`: opens a folded group, else selects the first row in it.
    fn right(&mut self) {
        let tree = self.tree();
        let rows = self.rows(&tree);
        let Some(at) = self.at(&rows) else {
            return;
        };
        let folded = matches!(&rows[at], Row::Group { chain, open: false, .. } if !last(chain).items.is_empty());
        let group = matches!(&rows[at], Row::Group { .. });
        drop(rows);
        drop(tree);
        if folded {
            self.fold(Some(at), Some(true));
        } else if group {
            self.move_to(|at, _| at + 1);
        }
    }

    /// Enter: a group folds or opens, and a lead of this machine
    /// attaches. A worker in a container opens its Session log in the
    /// Reports tab (01M6VT3JS3HRKXEJKDV0C97VZS): with no detail pane, it
    /// first shows its detail on the whole screen. Each other session
    /// shows its detail on the whole screen.
    fn enter(&mut self) -> Option<Action> {
        let picked = self.picked()?;
        match picked.session {
            None => self.fold(None, None),
            Some(Picked1 {
                lead: true,
                name: Some(name),
                ..
            }) => return Some(Action::Attach(name)),
            Some(Picked1 { log: Some(log), .. }) if self.sessions.full || self.sessions.pane => {
                self.open_log(log);
            }
            Some(_) if self.sessions.full => self.note(super::reports::NO_LOG_OF_SESSION),
            Some(_) => self.sessions.full = true,
        }
        None
    }

    /// `l`: the attach to the lead of the repository of the row, else to
    /// the first lead of this machine (01M51PD7M04HZR2PVVJA4WCBK5).
    fn lead(&mut self) -> Option<Action> {
        let picked = self.picked();
        if let Some(Picked {
            session:
                Some(Picked1 {
                    lead: true,
                    name: None,
                    host,
                    ..
                }),
            ..
        }) = &picked
            && host != self.me.place().host()
        {
            self.note(text::on_host_only(host));
            return None;
        }
        let repo = picked.as_ref().and_then(|p| p.repo.clone());
        let leads: Vec<(Option<String>, String)> = self
            .model
            .containers()
            .filter(|c| c.lead || c.phase == Phase::Terminal)
            .map(|c| {
                let repo = self
                    .model
                    .entries
                    .iter()
                    .find(|e| e.container.as_ref() == Some(c))
                    .and_then(|e| e.server.as_ref())
                    .map(|s| s.uri.place().repo_text());
                (repo, c.name.clone())
            })
            .collect();
        let of_repo = repo.as_ref().and_then(|repo| {
            leads
                .iter()
                .find(|(of, name)| of.as_ref() == Some(repo) || *name == crate::lead::name_of(repo))
                .map(|(_, name)| name.clone())
        });
        match of_repo.or_else(|| leads.first().map(|(_, name)| name.clone())) {
            Some(name) => Some(Action::Attach(name)),
            None => {
                self.note(text::NO_LEAD_HERE);
                None
            }
        }
    }

    /// The container session of this machine of the selected row, for `t`
    /// and `x`, or the note that says why there is none.
    fn container(&mut self) -> Option<(String, bool)> {
        match self.picked().and_then(|p| p.session) {
            None => {
                self.note(text::PICK_A_SESSION);
                None
            }
            Some(Picked1 {
                name: Some(name),
                lead,
                ..
            }) => Some((name, lead)),
            Some(Picked1 { host, .. }) if host != self.me.place().host() => {
                self.note(text::on_host_only(&host));
                None
            }
            Some(_) => {
                self.note(text::NO_CONTAINER_HERE);
                None
            }
        }
    }

    /// `t`: asks, then takes over the selected session
    /// (01M51KJ1R9K7A7RW8T8XH8VW43).
    fn takeover(&mut self) {
        match self.container() {
            Some((name, true)) => self.note(text::no_takeover_of_lead(&name)),
            Some((name, false)) => {
                self.ask(Question::new(
                    text::ask_takeover(&name),
                    Action::Takeover(name),
                ));
            }
            None => {}
        }
    }

    /// `x`: asks, then ends the selected session of the daemon
    /// (01M579Z9VRD12FRK6DSYVKDHQB).
    fn end(&mut self) {
        if let Some((name, _)) = self.container() {
            self.ask(Question::new(text::ask_end(&name), Action::End(name)));
        }
    }

    /// `p` and `P`: asks, then pauses or resumes the repository of the
    /// row, or with `riff`, the whole riff (01M3XAHZDSQR263QZVB41CK0MX).
    /// It is the [`Effect::Pause`] of `p` and `P` in the Waves tab.
    fn pause(&mut self, riff: bool) {
        let Some((look, _)) = self.last_look() else {
            return;
        };
        let pauses = &look.pauses;
        let (repo, paused) = if riff {
            (None, pauses.riff.is_some())
        } else {
            let Some(repo) = self
                .picked()
                .and_then(|p| p.repo)
                .and_then(|r| r.parse::<ThreadName>().ok())
            else {
                self.note(text::PICK_A_REPOSITORY);
                return;
            };
            let paused = pauses.repository(&repo).is_some();
            (Some(repo.to_string()), paused)
        };
        let state = if paused {
            RiffState::Running
        } else {
            RiffState::Paused
        };
        self.ask(Effect::Pause { repo, state }.question());
    }
}

/// The facts of the selected row that an action needs.
struct Picked {
    /// The repository of the row, `OWNER/REPO`.
    repo: Option<String>,
    /// The session of a session row.
    session: Option<Picked1>,
}

/// The facts of a selected session.
struct Picked1 {
    /// The name of its container on this machine.
    name: Option<String>,
    lead: bool,
    host: String,
    /// Its Session log, for a worker in a container.
    log: Option<Source>,
}

/// The counts of a list of sessions: by mark, by word, the sessions that
/// run an old riff, and the leads that wait for a person.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counts {
    pub sessions: usize,
    pub marks: BTreeMap<Mark, usize>,
    pub words: BTreeMap<&'static str, usize>,
    pub old: usize,
    pub waiting_for_a_person: usize,
}

impl Counts {
    fn of(items: &[&Item], seen: &BTreeSet<String>, server: Option<&Build>) -> Counts {
        let mut counts = Counts {
            sessions: items.len(),
            ..Counts::default()
        };
        for item in items {
            match item.mark(seen) {
                Some(mark) => *counts.marks.entry(mark).or_default() += 1,
                None => *counts.words.entry(item.word()).or_default() += 1,
            }
            counts.old += usize::from(item.old(server));
            counts.waiting_for_a_person += usize::from(item.info().is_some_and(|s| {
                s.uri.lead() && crate::state::of(s) == SessionState::Waiting && s.blocked.is_some()
            }));
        }
        counts
    }

    /// `N sessions`, or `1 session`.
    fn total(&self) -> String {
        many(self.sessions, "session")
    }

    /// Each count of a mark and of a word that is more than 0, the worst
    /// mark first.
    fn states(&self) -> Vec<String> {
        self.marks
            .iter()
            .map(|(mark, n)| format!("{n} {}", mark.word()))
            .chain(self.words.iter().map(|(word, n)| format!("{n} {word}")))
            .collect()
    }

    /// The counts of a group row: `5 sessions · 1 blocked · 2 busy`.
    fn line(&self) -> String {
        std::iter::once(self.total())
            .chain(self.states())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// `n word`, with `s` after the word when `n` is not 1.
fn many(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// The rows of the body of the Sessions tab in `area`, at the time
/// `now_ms`, and the places that a click hits. `margin` is the column
/// before each row of the wide layout. It also gives the first row of
/// the tree in the window, and the rows of the tree that fit.
pub fn body(
    state: &State,
    area: Rect,
    layout: Layout,
    margin: u16,
    now_ms: i64,
) -> (Vec<Placed>, Vec<Hit>, usize, usize) {
    let mut placed = Vec::new();
    let mut hits = Vec::new();
    if area.height == 0 {
        return (placed, hits, 0, 0);
    }
    let row = |y: u16| Rect::new(area.x + margin, area.y + y, area.width - margin, 1);
    let tree = state.tree();
    let rows = state.rows(&tree);
    let at = state.at(&rows);
    let full = state.sessions.full;
    let pane = pane(state, area, layout);
    let fit = if pane {
        usize::from(area.height.saturating_sub(1 + PANE))
    } else {
        usize::from(area.height - 1)
    };
    let top = window(state.sessions.top, at.unwrap_or(0), fit, rows.len());

    if full {
        if let Some(shown) = at.and_then(|at| rows.get(at)) {
            let (title, lines) = detail(state, shown, now_ms);
            placed.push(Placed {
                area: Rect::new(area.x, area.y, area.width, 1),
                line: rule(&title, area.width),
            });
            for (line, y) in lines.into_iter().zip(1..area.height) {
                placed.push(Placed { area: row(y), line });
            }
        }
        return (placed, hits, top, fit);
    }

    let place = (rows.len() > fit).then(|| {
        let end = (top + fit).min(rows.len());
        format!("{}-{end} of {}", top + 1, rows.len())
    });
    placed.push(Placed {
        area: row(0),
        line: summary(state, layout, place, area.width - margin),
    });
    let width = area.width - margin;
    let hosts = host_cols(&tree, layout, area.width);
    for ((i, shown), y) in rows.iter().enumerate().skip(top).zip(1..).take(fit) {
        let selected = Some(i) == at;
        let line = match shown {
            Row::Group { chain, depth, open } => {
                group_line(state, chain, *depth, *open, selected, layout, width)
            }
            Row::Session { item, depth } => {
                session_line(state, item, *depth, hosts, selected, layout, now_ms)
            }
        };
        let area = row(y);
        if let Row::Group { depth, .. } = shown {
            let x = area.x + u16::try_from(depth * 2).unwrap_or(u16::MAX);
            hits.push(Hit {
                area: Rect::new(x, area.y, 1, 1).intersection(area),
                key: Key::Fold(i),
            });
        }
        hits.push(Hit {
            area,
            key: Key::Row(i),
        });
        placed.push(Placed { area, line });
    }
    if pane && let Some(shown) = at.and_then(|at| rows.get(at)) {
        let first = area.height - PANE;
        let (title, lines) = detail(state, shown, now_ms);
        placed.push(Placed {
            area: Rect::new(area.x, area.y + first, area.width, 1),
            line: rule(&title, area.width),
        });
        for (line, y) in lines.into_iter().zip(first + 1..area.height) {
            placed.push(Placed { area: row(y), line });
        }
    }
    (placed, hits, top, fit)
}

/// True when the tab shows the detail pane in the body `area`: in the
/// wide layout, with 24 rows or more, and no detail on the whole screen
/// (01M6VGH4QSE8BP51R3TZ9SJDVY).
pub fn pane(state: &State, area: Rect, layout: Layout) -> bool {
    layout == Layout::Wide && !state.sessions.full && area.height + 4 >= PANE_ROWS
}

/// The first row of a window of `fit` rows of `len` rows, after a window
/// that started at `top`, so that the row `at` shows.
///
/// ```
/// use riff::tui::sessions::window;
///
/// assert_eq!(window(0, 3, 10, 40), 0);
/// assert_eq!(window(0, 12, 10, 40), 3);
/// assert_eq!(window(5, 2, 10, 40), 2);
/// // No empty rows at the end of a window.
/// assert_eq!(window(35, 39, 10, 40), 30);
/// assert_eq!(window(3, 0, 10, 4), 0);
/// ```
pub fn window(top: usize, at: usize, fit: usize, len: usize) -> usize {
    let top = top.min(len.saturating_sub(fit));
    if at < top {
        at
    } else if fit > 0 && at >= top + fit {
        at + 1 - fit
    } else {
        top
    }
}

/// The line `──TITLE ───` of `width` columns over the detail.
fn rule(title: &str, width: u16) -> Line<'static> {
    let head = format!("──{title} ");
    let rest = usize::from(width).saturating_sub(Line::raw(head.as_str()).width());
    Line::styled(format!("{head}{}", "─".repeat(rest)), muted())
}

/// The summary line: the counts of the sessions that show, then on the
/// right the place in a long tree and the top level. While a filter is
/// on, the line starts with it, and the count is of all the sessions:
/// `filter host:pangolin · 1 of 6 sessions: 1 busy · Esc clears`. The
/// narrow layout has `filter host:pangolin · 1 of 6 sessions`
/// (01M6WKCKVJTJZSXVTZDZ9FZPFY).
fn summary(state: &State, layout: Layout, place: Option<String>, width: u16) -> Line<'static> {
    let items = state.items();
    let items: Vec<&Item> = items.iter().collect();
    let counts = Counts::of(&items, &state.sessions.seen, state.server());
    let states = counts.states();
    let filtered = state.sessions.filtered();
    let wide = layout == Layout::Wide;
    let total = if filtered {
        let all = state
            .model
            .entries
            .iter()
            .filter_map(|entry| Item::of(entry, &state.me))
            .count();
        format!("{} of {}", counts.sessions, many(all, "session"))
    } else {
        counts.total()
    };
    let mut left = if wide {
        let mut parts = states;
        if counts.old > 0 {
            parts.push(format!("{} to update", counts.old));
        }
        if counts.waiting_for_a_person > 0 {
            parts.push(format!(
                "{} waiting for a person",
                counts.waiting_for_a_person
            ));
        }
        if parts.is_empty() {
            total
        } else {
            format!("{total}: {}", parts.join(" · "))
        }
    } else if filtered || states.is_empty() {
        total
    } else {
        states.join(" · ")
    };
    if filtered {
        left = format!("filter {} · {left}", safe(state.sessions.filter.trim()));
        if wide {
            left.push_str(" · Esc clears");
        }
    }
    let mut right = Vec::new();
    if wide {
        right.extend(place);
        right.push(
            match state.sessions.by() {
                By::Person => "by person",
                By::Repo => "by repo",
            }
            .to_owned(),
        );
    }
    // With no room, the top level goes first, then the place.
    let used = Line::raw(left.as_str()).width();
    while right.len() > 1 && used + 2 + Line::raw(right.join(" · ")).width() >= usize::from(width)
    {
        right.pop();
    }
    aligned(
        vec![Span::raw(left)],
        vec![Span::styled(right.join(" · "), muted())],
        width,
    )
}

/// `left`, then `right` at the right end of `width` columns, one column
/// before the end. With no room, two spaces between them.
fn aligned(left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: u16) -> Line<'static> {
    let used: usize = left.iter().map(Span::width).sum();
    let wide: usize = right.iter().map(Span::width).sum();
    let room = usize::from(width).saturating_sub(1);
    let gap = if wide == 0 {
        0
    } else {
        room.saturating_sub(used + wide).max(2)
    };
    let mut spans = left;
    if wide > 0 {
        spans.push(Span::raw(" ".repeat(gap)));
        spans.extend(right);
    }
    Line::from(spans)
}

/// The parts of the label of the group `group`: a person with its role
/// and its state, a host, or a repository by its short name when the
/// sessions have one owner.
fn label(state: &State, group: &Group, layout: Layout) -> Vec<Span<'static>> {
    match group.level {
        Level::Person => {
            let user = Span::styled(safe(&group.key), style_of(crate::style::person(&group.key)));
            if layout != Layout::Wide {
                return vec![user];
            }
            let people = state.people();
            let person = people.get(&group.key).copied().unwrap_or(PersonRow {
                role: PersonRole::Member,
                live: false,
                seen_secs: None,
            });
            let mut parts = vec![user];
            if let Some(tag) = person.role.tag() {
                parts.push(Span::styled(tag, muted()));
            }
            parts.extend(
                crate::state::person(person.live, person.seen_secs)
                    .into_iter()
                    .map(|(text, style)| Span::styled(text, style_of(style))),
            );
            parts
        }
        Level::Host | Level::Repo => vec![Span::raw(name(state, group))],
    }
}

/// The name of the group `group`: a user, a host, or a repository by
/// [`repo_name`].
fn name(state: &State, group: &Group) -> String {
    match group.level {
        Level::Repo => repo_name(state, &group.key),
        Level::Person | Level::Host => safe(&group.key),
    }
}

/// What the folded group `group` holds, after its label: the names of
/// the groups under it, then the hosts of its sessions, the worst row
/// first: ` · mike, brett · thelio, pangolin` (01M6WKCKRN073FHR719KJNG3CQ).
fn held(state: &State, group: &Group) -> String {
    let mut hosts: Vec<String> = Vec::new();
    for item in &group.items {
        let host = safe(item.key(Level::Host));
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    }
    let kids: Vec<String> = group.kids.iter().map(|kid| name(state, kid)).collect();
    [kids, hosts]
        .iter()
        .filter(|names| !names.is_empty())
        .map(|names| format!(" · {}", names.join(", ")))
        .collect()
}

/// The name of the repository `repo`: its short name when the
/// repositories of the sessions have one owner, else `OWNER/REPO`
/// (01M3WNHCD659FH3Z5VYYH69WWR).
fn repo_name(state: &State, repo: &str) -> String {
    let mut owners = state
        .model
        .on_server()
        .filter_map(|s| match s.uri.place().repo() {
            Repo::Git { owner, .. } => Some(owner),
            Repo::None => None,
        });
    let first = owners.next();
    let one = owners.all(|owner| Some(owner) == first);
    match repo.split_once('/') {
        Some((_, name)) if one => safe(name),
        _ => safe(repo),
    }
}

/// The line of a group row: its fold mark, its label with each group of
/// its chain after `›`, and its counts on the right.
fn group_line(
    state: &State,
    chain: &[&Group],
    depth: usize,
    open: bool,
    selected: bool,
    layout: Layout,
    width: u16,
) -> Line<'static> {
    let end = last(chain);
    let mut left = vec![Span::raw(indent(depth, selected))];
    left.push(Span::raw(if open { "▼ " } else { "▶ " }));
    let wide = layout == Layout::Wide;
    let mut before = 0;
    for (i, group) in chain.iter().enumerate() {
        let parts = label(state, group, layout);
        if i > 0 {
            left.push(Span::raw(if before > 1 { "  › " } else { " › " }));
        }
        before = parts.len();
        for (j, part) in parts.into_iter().enumerate() {
            if j > 0 {
                left.push(Span::raw("  "));
            }
            left.push(part);
        }
    }
    let items: Vec<&Item> = end.items.iter().collect();
    let right = if items.is_empty() {
        if wide {
            String::new()
        } else {
            let person = state.people().get(&end.key).copied();
            crate::state::person(person.is_some_and(|p| p.live), None)
                .into_iter()
                .map(|(text, _)| text)
                .next()
                .unwrap_or_default()
        }
    } else {
        // A folded row names what it holds, then each count. With no
        // room, it has the count of its sessions, and with no room for
        // the names too, no names.
        let counts = Counts::of(&items, &state.sessions.seen, state.server());
        let used: usize = left.iter().map(Span::width).sum();
        let held = if open {
            String::new()
        } else {
            held(state, end)
        };
        let fits =
            |used: usize, text: &str| used + 2 + Line::raw(text).width() < usize::from(width);
        let with = used + Line::raw(held.as_str()).width();
        let line = counts.line();
        let total = counts.total();
        let (held, right) = if wide && fits(with, &line) {
            (held, line)
        } else if fits(with, &total) {
            (held, total)
        } else if wide && fits(used, &line) {
            (String::new(), line)
        } else {
            (String::new(), total)
        };
        if !held.is_empty() {
            left.push(Span::raw(held));
        }
        right
    };
    let line = aligned(left, vec![Span::styled(right, muted())], width);
    if selected {
        line.patch_style(SELECTED)
    } else {
        line
    }
}

/// The columns before a row at `depth`: `>` for the selected row when
/// there is room.
fn indent(depth: usize, selected: bool) -> String {
    let pad = " ".repeat(depth * 2);
    if selected && depth > 0 {
        format!(">{}", &pad[1..])
    } else {
        pad
    }
}

/// The most columns of the host of a session row: in the wide layout,
/// and in the narrow layout.
const HOST_COLS: u16 = 12;
const NARROW_HOST_COLS: u16 = 8;

/// The fewest columns of a terminal with the host in each session row.
pub const HOST_FROM: u16 = 50;

/// The columns of the host of each session row of `tree` in `columns`:
/// the widest host, up to [`HOST_COLS`], or up to [`NARROW_HOST_COLS`]
/// in the narrow layout. Under [`HOST_FROM`] columns, none
/// (01M6WKCC98EGKB3MSYKN3X2JMH, 01M6WKCKNSCT73R2DSR01CGN5J).
fn host_cols(tree: &Tree, layout: Layout, columns: u16) -> u16 {
    if columns < HOST_FROM {
        return 0;
    }
    let most = if layout == Layout::Wide {
        HOST_COLS
    } else {
        NARROW_HOST_COLS
    };
    tree.groups
        .iter()
        .flat_map(|group| &group.items)
        .map(|item| width(&safe(item.key(Level::Host))))
        .max()
        .unwrap_or(0)
        .min(most)
}

/// The line of a session row, with the host in `hosts` columns (none
/// for 0). The wide layout:
/// `MARK ID8  ITEM   TAG        HOST      STATE AGE    LAST`. The narrow
/// layout: `MARK  ID4  ITEM   HOST      [lead] STATE AGE`, with each
/// session after two columns.
fn session_line(
    state: &State,
    item: &Item,
    depth: usize,
    hosts: u16,
    selected: bool,
    layout: Layout,
    now_ms: i64,
) -> Line<'static> {
    let wide = layout == Layout::Wide;
    let depth = if wide { depth } else { 1 };
    let mark = item.mark(&state.sessions.seen);
    let (symbol, mark_style) = match mark {
        Some(mark) => (mark.symbol(), mark.style()),
        None => (" ", Style::new()),
    };
    let mut item_text = item.item().unwrap_or_default();
    if item.old(state.server()) {
        item_text.push_str(crate::top::OUT_OF_DATE);
    }
    let (word, word_style) = match mark {
        Some(mark) => (mark.word(), mark.style()),
        None => (
            item.word(),
            item.state()
                .map_or_else(Style::new, |s| style_of(crate::state::style(s))),
        ),
    };
    let state_text = match item.age(mark, now_ms) {
        Some(secs) => format!("{word} {}", text::ago(secs)),
        None => word.to_owned(),
    };
    let id_style = item
        .info()
        .map_or_else(Style::new, |s| style_of(crate::style::session(&s.uri)));
    let host = (hosts > 0).then(|| {
        let host = super::waves::cut(&safe(item.key(Level::Host)), usize::from(hosts));
        Span::raw(format!("{} ", pad(&host, hosts)))
    });
    let mut spans = vec![
        Span::raw(indent(depth, selected)),
        Span::styled(symbol, mark_style),
    ];
    if wide {
        spans.extend([
            Span::raw(" "),
            Span::styled(item.short(8), id_style),
            Span::raw("  "),
            Span::raw(pad(&item_text, 6)),
            Span::raw(" "),
            Span::styled(pad(item.tag(), 10), muted()),
            Span::raw(" "),
        ]);
        spans.extend(host);
        spans.extend([
            Span::styled(pad(&state_text, 12), word_style),
            Span::raw(" "),
            Span::raw(item.last()),
        ]);
    } else {
        spans.extend([
            Span::raw("  "),
            Span::styled(item.short(4), id_style),
            Span::raw("  "),
            Span::raw(pad(&item_text, 6)),
            Span::raw(" "),
        ]);
        spans.extend(host);
        if item.lead() {
            spans.push(Span::styled("lead ", muted()));
        }
        spans.push(Span::styled(state_text, word_style));
    }
    let line = Line::from(spans);
    if selected {
        line.patch_style(SELECTED)
    } else {
        line
    }
}

/// `text` with spaces after it to `cols` columns.
fn pad(text: &str, cols: u16) -> String {
    let used = width(text);
    format!(
        "{text}{}",
        " ".repeat(usize::from(cols.saturating_sub(used)))
    )
}

/// The title and the lines of the detail of the row `row` at `now_ms`.
fn detail(state: &State, row: &Row, now_ms: i64) -> (String, Vec<Line<'static>>) {
    match row {
        Row::Session { item, .. } => session_detail(state, item, now_ms),
        Row::Group { chain, .. } => group_detail(state, chain),
    }
}

/// The detail of a session: the detail of `riff top`, its claims, its
/// pull request and its riff, then its container on this machine.
fn session_detail(state: &State, item: &Item, now_ms: i64) -> (String, Vec<Line<'static>>) {
    let issues = state.titles.get(item.key(Level::Repo));
    let title = |n: u64| issues.and_then(|i| i.titles.get(&n).cloned());
    let mut head = vec![item.short(8)];
    if let Some(claim) = item.claims().first() {
        let text = match riff_client::waves::issue_of(claim).and_then(title) {
            Some(t) => format!("{} {}", item_of(claim), safe(&t)),
            None => item_of(claim),
        };
        head.push(text);
    }
    head.push(safe(item.key(Level::Host)));
    if let Some(c) = item.live() {
        let secs = u64::try_from((now_ms - c.since_ms) / 1000).unwrap_or(0);
        head.push(text::ago(secs));
    }
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(s) = item.info() {
        lines.extend(
            crate::state::detail(s, &title)
                .into_iter()
                .map(|(text, style)| Line::styled(text, style_of(style))),
        );
        let claims = item.claims();
        let claims = match claims.as_slice() {
            [] => "no claim".to_owned(),
            [one] => format!("claim {}", item_of(one)),
            more => format!(
                "claims {}",
                more.iter()
                    .map(|c| item_of(c))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        };
        let pull = item.pull().map_or_else(
            || "pull request none".to_owned(),
            |n| format!("pull request #{n}"),
        );
        let build = if item.old(state.server()) {
            "riff build old: riff update"
        } else {
            "riff build current"
        };
        lines.push(Line::raw(format!("{claims}   {pull}   {build}")));
    }
    if let Some(c) = item.live() {
        let secs = u64::try_from((now_ms - c.since_ms) / 1000).unwrap_or(0);
        lines.push(Line::raw(format!(
            "container {} · {} for {}",
            safe(&c.name),
            text::phase_word(c.phase),
            text::ago(secs)
        )));
        if !c.last.is_empty() {
            lines.push(Line::raw(format!("last step: {}", safe(&c.last))));
        }
    }
    (head.join(" · "), lines)
}

/// The detail of a group: its name, its counts, and for a repository its
/// pause. The facts of a host are in the Machines tab.
fn group_detail(state: &State, chain: &[&Group]) -> (String, Vec<Line<'static>>) {
    let end = last(chain);
    let title: Vec<String> = chain
        .iter()
        .map(|group| {
            label(state, group, Layout::Wide)
                .iter()
                .map(|span| span.content.to_string())
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .collect();
    let items: Vec<&Item> = end.items.iter().collect();
    let mut lines = Vec::new();
    if !items.is_empty() {
        let counts = Counts::of(&items, &state.sessions.seen, state.server());
        lines.push(Line::raw(counts.line()));
    }
    for group in chain {
        if group.level == Level::Repo
            && let Some((look, _)) = state.last_look()
            && let Ok(thread) = group.key.parse::<ThreadName>()
            && (look.pauses.riff.is_some() || look.pauses.repository(&thread).is_some())
        {
            lines.push(Line::styled(
                format!("{} is paused", safe(&group.key)),
                style_of(crate::style::WARNING),
            ));
        }
    }
    (title.join(" › "), lines)
}

/// The keys of the key line of the Sessions tab: the text of the key,
/// its word, and the key that a click on it presses. While the person
/// types the filter, the line has the keys of the filter
/// ([`super::machines::filter_keys`]). While a filter is on, the line
/// has `Esc clear the filter`, in place of keys of less use
/// (01M6WKCKVJTJZSXVTZDZ9FZPFY).
pub fn keys(state: &State, layout: Layout) -> Vec<(&'static str, &'static str, Option<Key>)> {
    if state.sessions.full {
        let log = state
            .picked()
            .and_then(|p| p.session)
            .is_some_and(|s| s.log.is_some());
        let mut keys = vec![("Esc", "back", Some(Key::Esc))];
        if log {
            keys.push(("Enter", "log", Some(Key::Enter)));
        }
        keys.push(("l", "lead", Some(Key::Char('l'))));
        if !log || layout == Layout::Wide {
            keys.push(("t", "take over", Some(Key::Char('t'))));
        }
        keys.push(("?", "help", Some(Key::Help)));
        return keys;
    }
    let clear = ("Esc", "clear the filter", Some(Key::Esc));
    match layout {
        Layout::Wide if state.sessions.filtered() => vec![
            ("↑↓", "select", None),
            ("Enter", "open", Some(Key::Enter)),
            ("l", "lead", Some(Key::Char('l'))),
            ("t", "take over", Some(Key::Char('t'))),
            clear,
            ("?", "help", Some(Key::Help)),
        ],
        _ if state.sessions.filtered() => vec![
            ("Enter", "open", Some(Key::Enter)),
            clear,
            ("?", "help", Some(Key::Help)),
        ],
        Layout::Wide => vec![
            ("↑↓", "select", None),
            ("←→", "fold", None),
            ("Enter", "open", Some(Key::Enter)),
            ("l", "lead", Some(Key::Char('l'))),
            ("t", "take over", Some(Key::Char('t'))),
            ("p", "pause", Some(Key::Char('p'))),
            ("/", "filter", Some(Key::Char('/'))),
            ("?", "help", Some(Key::Help)),
        ],
        _ => vec![
            ("↑↓", "select", None),
            ("Enter", "open", Some(Key::Enter)),
            ("?", "help", Some(Key::Help)),
            ("q", "quit", Some(Key::Quit)),
        ],
    }
}

/// The keys of the Sessions tab in the help popup.
pub fn help_keys() -> Vec<(&'static str, &'static str)> {
    vec![
        ("↑ ↓ j k", "select a row"),
        ("PgUp PgDn g G", "move a page, or to the first or last row"),
        ("← → Space", "fold or open a group"),
        (
            "Enter",
            "fold, attach to a lead, or open a session or its log",
        ),
        ("l", "attach to the lead"),
        ("t", "take over the session, after y"),
        ("x", "end the session of the daemon, after y"),
        (
            "p, P",
            "pause or resume the repository, or the riff, after y",
        ),
        ("e", "open each group, or fold the groups that need no look"),
        ("b", "put the person or the repository at the top"),
        ("/", "filter: Enter keeps it, Esc clears it"),
    ]
}

#[cfg(test)]
mod tests;
