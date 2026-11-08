//! The Waves tab of the TUI (#1638): the boards, the items, the pull
//! requests, hold and free, the pause, the next waves and the checks of
//! `main`.
//!
//! # Design
//!
//! The tab shows a board for each repository of a live session, and for
//! the repository of the working folder ([`board_repos`],
//! 01M6VFG882T794B1CTHK5AKPDG). A board is the model of `riff top`
//! ([`Board`]): the items of the current wave, from the facts of the
//! forge ([`super::forge`]), the claims of `who` and the holds of
//! riff-server. The look of the TUI reads the holds of each board while
//! the tab shows ([`super::look`], 01M6VFGK79JGRF919BM8SJ6E08).
//!
//! ```text
//!  1 Sessions  2 Chat•2  [3 Waves]  4 Machines  5 Devices  6 Reports
//!  riff running · owner mike · build 10df8a4 · link ok 2s
//!  ▼ como-technologies/riff · Wave 48        1 stuck 2 free 1 held 3 work 2 verify
//!        STATE   ITEM   TITLE                           WHO / PULL REQUEST   AGE
//!    !   stuck   #1601  Fix the help                    PR #1604 conflict    2h
//!  > ○   free    #1620  Add the flag --format           -                    -
//!    ○   free    #1622  The book names the flag         -                    -
//!    ◌   held    #1631  Design: the web UI              waits for mike       3d
//!    !   work    #994   Design: the TUI of riff         3a3f8d5d blocked     32m
//!    ●   work    #1590  An idle look asks GitHub first  5b1e2a90 busy        40m
//!    ●   work    #1594  riff counts the gh calls        9c0d1e2f busy        1h
//!    ◐   verify  #1599  The wave takes an issue         PR #1614 verify      5m
//!    ◐   verify  #1600  The book tells how to detach    PR #1613 merge       9m
//!        done 4 · closed 1 (Enter shows them)
//!        main: the full check of 3f2a91c runs · 4 of 9 jobs done
//!  ▶ como-technologies/strata · Wave 5                               1 work 3 done
//!  ▶ como-technologies/kadomony · no wave
//!
//!  NEXT WAVES · como-technologies/riff
//!    Wave 49: 8 items    Wave 50: 3 items    Backlog: 41 items
//!
//!
//!  The boards come from gh, 41s ago.
//!  ↑↓ select  Enter open  h hold  f free  p pause  o URL  r read again  ? help
//! ```
//!
//! - **The order** ([`rows`]): the repositories with a stuck, held or
//!   free item come first, and their boards are open. Then the other
//!   repositories with a wave, then the ones with no wave, each by name,
//!   each one folded line with its counts. The person folds a board and
//!   opens it again with `Enter`, `←`, `→` or `Space`
//!   (01M6VFG882T794B1CTHK5AKPDG).
//! - **The rows** ([`ItemRow`], 01M6VFGJWVB3WEEYBCNSZ2FJT0): stuck, free,
//!   held, asked, work, verify and tracker. A work row shows the session
//!   that holds the claim with its state and mark
//!   ([`super::Mark`]), a verify or stuck row its pull request. A verify
//!   or a rebase that waits for its turn is a verify row that names the
//!   pull request before it, as `PR #42 after #41`
//!   (01M6YPYTW6CVCDBZ9631CFXQBJ). The
//!   closed items of the wave are one line, and `Enter` shows them.
//! - **The questions of each wave** (01M6Y0ZQWDYVC23WA5CVN0QSH8): the
//!   asked rows have the issues with the label `question` of the later
//!   waves too ([`later_questions`]), in number order, as `riff
//!   questions` lists them. Such a row names its wave before its title:
//!   `Wave 56 · Design: …`.
//! - **The lines under a board** (01M6VFGJZF7XGNGVWTECKAH429): the full
//!   check of `main` when it needs a look, and each release whose checks
//!   after the deploy have not passed.
//! - **The next waves** (01M6VFGK22ZC1D7WF6PP853TP9): the later waves and
//!   the Backlog of the repository of the selected row.
//!
//! The rows are a list ([`Row`]). The selection names a row ([`Pick`]),
//! so it stays on its item when a look moves the rows. The view builds
//! only the lines that fit the body, around the selection.
//!
//! # The actions
//!
//! Each action that changes something asks first in the note line
//! ([`super::Question`], 01M6VFGK9XXWCA8X7CE8EZNSJV), and calls the code
//! of its CLI command ([`super::Effect`]). `h` asks for the reason first
//! ([`Reason`], [`typed`]):
//!
//! | Key | Does | The code of |
//! |---|---|---|
//! | `h` | Asks for a reason, then holds the item. | `riff plan hold` |
//! | `f` | Frees the held item. | `riff plan free` |
//! | `a` | Marks the questions of an item with the label `question` as answered: the label goes, and the lead wakes. | `riff questions --answered` |
//! | `p` | Pauses or resumes the repository of the row. | `riff pause --repo`, `riff resume --repo` |
//! | `P` | Pauses or resumes the whole riff. | `riff pause --riff`, `riff resume --riff` |
//! | `o` | Puts the URL of the pull request, the item or the repository in the note line. | |
//! | `Enter` | Folds a board, shows the closed items, or puts the whole row of an item in the note line. | |
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as the person
//!     participant T as the Waves tab
//!     participant S as riff-server
//!     P->>T: h on #1620
//!     T-->>P: Hold #1620 of o/r. The reason: █
//!     P->>T: waits for the design, Enter
//!     T-->>P: Hold #1620 of o/r: waits for the design? y/N
//!     P->>T: y
//!     T->>S: riff plan hold issue-1620 (the code of the CLI)
//!     S-->>T: the line of the CLI, in the note line
//!     T->>S: a look at once: the new holds
//! ```

use std::collections::{BTreeMap, BTreeSet};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use riff_client::sessions::{Sessions, Show};
use riff_client::waves::{Board, Issues, board_repos, wave_number};
use riff_core::name::ThreadName;
use riff_core::wire::{HoldInfo, RiffState, SessionInfo};

use super::forge::{Closed, Forge, Main};
use super::view::Layout;
use super::{Action, Effect, Key, Look, Mark, NOTHING_CHANGED, Question, State};
use crate::text;

/// The body of the tab when no repository has a board: two lines that
/// fit the narrow layout.
pub const NO_BOARD: [&str; 2] = [
    "No repository has a live session,",
    "and this folder is no clone of one.",
];

/// The style of a stuck item and of a broken `main`.
const ERROR: Style = Style::new().fg(Color::Red);

/// The style of a word that needs a look.
const WARNING: Style = Style::new().fg(Color::Yellow);

/// The style of a verify and of a done item.
const CYAN: Style = Style::new().fg(Color::Cyan);

/// The style of a quiet mark and of the names of the columns.
const DIM: Style = Style::new().add_modifier(Modifier::DIM);

/// The style of a title.
const BOLD: Style = Style::new().add_modifier(Modifier::BOLD);

/// The style of the selected row.
const SELECTED: Style = Style::new().add_modifier(Modifier::REVERSED);

/// The state of an item on a board (01M6VFGJWVB3WEEYBCNSZ2FJT0), in the
/// order of the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemState {
    /// Its pull request cannot merge by itself.
    Stuck,
    /// No claim, no pull request in verify, no hold.
    Free,
    /// The lead holds it.
    Held,
    /// It has the label `question`: it waits for the answer of a person.
    Asked,
    /// A session holds a claim of it.
    Work,
    /// Its pull request waits for a verify or for the merge, or its rebase
    /// waits for its turn (#1801).
    Verify,
    /// A tracker issue: no session does it.
    Tracker,
    /// It closed as completed.
    Done,
    /// It closed as not planned or as a duplicate.
    Closed,
}

impl ItemState {
    /// The word of the state, in the row and in the counts.
    ///
    /// ```
    /// use riff::tui::waves::ItemState;
    ///
    /// assert_eq!(ItemState::Stuck.word(), "stuck");
    /// assert_eq!(ItemState::Asked.word(), "asked");
    /// assert_eq!(ItemState::Done.word(), "done");
    /// ```
    pub fn word(self) -> &'static str {
        match self {
            ItemState::Stuck => "stuck",
            ItemState::Free => "free",
            ItemState::Held => "held",
            ItemState::Asked => "asked",
            ItemState::Work => "work",
            ItemState::Verify => "verify",
            ItemState::Tracker => "tracker",
            ItemState::Done => "done",
            ItemState::Closed => "closed",
        }
    }

    /// The mark of an item in this state, and its style. A work item has
    /// the mark of its session ([`ItemRow::mark`]).
    pub fn mark(self) -> (&'static str, Style) {
        match self {
            ItemState::Stuck => ("!", ERROR),
            ItemState::Free => ("○", DIM),
            ItemState::Held => ("◌", DIM),
            ItemState::Asked => ("?", WARNING),
            ItemState::Work => ("●", Style::new().fg(Color::Green)),
            ItemState::Verify => ("◐", CYAN),
            ItemState::Tracker => ("·", DIM),
            ItemState::Done => ("✓", CYAN),
            ItemState::Closed => ("×", DIM),
        }
    }
}

/// The row of an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRow {
    pub number: u64,
    pub state: ItemState,
    pub title: String,
    /// The session that holds the claim, the pull request, or the reason
    /// of the hold. `-` for none.
    pub who: String,
    /// When its age starts, in milliseconds since the epoch: the claim or
    /// the block of the session, the last update of the pull request, or
    /// the hold.
    pub since_ms: Option<i64>,
    /// The mark: its symbol and its style.
    pub mark: (&'static str, Style),
    /// The number of its open pull request.
    pub pull: Option<u64>,
    /// The wave of a question of a later wave, as `Wave N`. `None` for an
    /// item of the current wave.
    pub wave: Option<String>,
}

/// The wave of the line of a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wave {
    /// The current wave, as `Wave N`.
    Named(String),
    /// The repository has no open issue in a wave.
    None,
    /// The forge of the repository is not read yet.
    Unread,
}

/// The line of a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRow {
    /// `OWNER/REPO`.
    pub repo: String,
    /// True when its board shows.
    pub open: bool,
    pub wave: Wave,
    /// The count of each state that is not empty, in the order of the
    /// rows. A folded board counts the closed items too.
    pub counts: Vec<(usize, ItemState)>,
    /// True when the repository or the whole riff is paused.
    pub paused: bool,
}

/// A row of the body of the Waves tab, before the draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// The line of a repository: its board is open or folded.
    Repo(RepoRow),
    /// The names of the columns, in the wide layout only.
    Columns,
    /// An item of the board of `repo`.
    Item { repo: String, item: ItemRow },
    /// The counts of the items that closed in the wave of `repo`.
    Closed {
        repo: String,
        done: usize,
        closed: usize,
        shown: bool,
    },
    /// The full check of `main`.
    Main(Main),
    /// An open release issue: its number and title.
    Release(u64, String),
    /// An empty line.
    Blank,
    /// The title of the next waves of `repo`.
    NextTitle(String),
    /// The next waves: the name of each, and its open items.
    Next(Vec<(String, usize)>),
    /// A line of text.
    Text(String),
}

/// The row that the person selected. It names the row, so it stays when
/// a look moves the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// The line of a repository.
    Repo(String),
    /// An item of a repository.
    Item(String, u64),
    /// The line of the closed items of a repository.
    Closed(String),
}

impl Row {
    /// The pick of this row, when the person can select it.
    pub fn pick(&self) -> Option<Pick> {
        match self {
            Row::Repo(row) => Some(Pick::Repo(row.repo.clone())),
            Row::Item { repo, item } => Some(Pick::Item(repo.clone(), item.number)),
            Row::Closed { repo, .. } => Some(Pick::Closed(repo.clone())),
            _ => None,
        }
    }

    /// The repository of this row, if it has one.
    fn repo(&self) -> Option<&str> {
        match self {
            Row::Repo(row) => Some(&row.repo),
            Row::Item { repo, .. } | Row::Closed { repo, .. } | Row::NextTitle(repo) => Some(repo),
            _ => None,
        }
    }
}

/// What the Waves tab knows, and the choices of the person.
#[derive(Debug, Clone, Default)]
pub struct Waves {
    /// The repository of the working folder, `OWNER/REPO`, if any.
    here: Option<String>,
    /// The last good read of the forge of each repository.
    forge: BTreeMap<String, Forge>,
    /// The time of the last read of the forge, in milliseconds since the
    /// epoch.
    read_ms: Option<i64>,
    /// The fault of the last read that failed, while it holds.
    fault: Option<String>,
    /// The selected row.
    pick: Option<Pick>,
    /// The folds that the person chose: `true` for an open board.
    folds: BTreeMap<String, bool>,
    /// The repositories whose closed items show.
    shown: BTreeSet<String>,
}

/// What a key of the Waves tab gives the frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// A line for the note line.
    Note(String),
    /// A question of `y` or `n` for the note line.
    Ask(Question),
    /// The reason of a hold, for the person to type in the note line.
    Reason(Reason),
}

/// The most characters of the reason of a hold: the limit of
/// riff-server.
pub const REASON_MAX: usize = 200;

/// The reason of a hold of `item` of `repo`, as the person types it in
/// the note line (01M6VFGK9XXWCA8X7CE8EZNSJV).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    /// The repository, `OWNER/REPO`.
    pub repo: String,
    /// The number of the item.
    pub item: u64,
    /// The text so far.
    pub text: String,
}

impl Reason {
    /// The note line while the person types: the question, the text and
    /// a cursor.
    ///
    /// ```
    /// use riff::tui::waves::Reason;
    ///
    /// let reason = Reason { repo: "o/r".into(), item: 12, text: "a des".into() };
    /// assert_eq!(reason.line(), "Hold #12 of o/r. The reason: a des█");
    /// ```
    pub fn line(&self) -> String {
        format!(
            "Hold #{} of {}. The reason: {}█",
            self.item,
            text::safe(&self.repo),
            text::safe(&self.text)
        )
    }
}

/// Takes the key `key` while the person types the reason `reason` of a
/// hold (01M6VFGK9XXWCA8X7CE8EZNSJV, 01M6VFGKCJ02G33TXR01GY6M0G): each
/// character types into it, also `q` and the digits. `Backspace` deletes
/// the last one, `Enter` asks `y` or `n`, `Esc` holds nothing, and
/// `Ctrl-C` ends the TUI.
///
/// ```
/// use riff::tui::waves::Reason;
/// use riff::tui::{Action, Effect, Key, NOTHING_CHANGED, State};
/// use riff_core::build::Build;
///
/// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
/// let reason = || Reason { repo: "o/r".into(), item: 12, text: String::new() };
/// state.ask_reason(reason());
/// for key in "q 3x".chars() {
///     assert_eq!(state.on_key(Key::Char(key)), None);
/// }
/// state.on_key(Key::Backspace);
/// assert_eq!(state.asked().as_deref(), Some("Hold #12 of o/r. The reason: q 3█"));
/// state.on_key(Key::Enter);
/// assert_eq!(state.asked().as_deref(), Some("Hold #12 of o/r: q 3? y/N"));
/// let hold = Effect::Hold { repo: "o/r".into(), item: 12, reason: "q 3".into() };
/// assert_eq!(state.on_key(Key::Char('y')), Some(Action::Do(hold.clone())));
/// assert_eq!(state.asked(), None);
/// // Each other key cancels the question.
/// state.ask(hold.question());
/// assert_eq!(state.on_key(Key::Char('n')), None);
/// assert_eq!(state.shown_note(), Some(NOTHING_CHANGED));
/// // Esc cancels the text, and Ctrl-C ends the TUI.
/// state.ask_reason(reason());
/// assert_eq!(state.on_key(Key::Esc), None);
/// assert_eq!(state.asked(), None);
/// state.ask_reason(reason());
/// assert_eq!(state.on_key(Key::Quit), Some(Action::Quit));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn typed(state: &mut State, mut reason: Reason, key: Key) -> Option<Action> {
    match key {
        Key::Quit => return Some(Action::Quit),
        Key::Char(c) if reason.text.chars().count() < REASON_MAX => reason.text.push(c),
        Key::Backspace => {
            reason.text.pop();
        }
        Key::Enter if !reason.text.trim().is_empty() => {
            let hold = Effect::Hold {
                reason: reason.text.trim().to_owned(),
                repo: reason.repo,
                item: reason.item,
            };
            state.ask(hold.question());
            return None;
        }
        Key::Esc => {
            state.note(NOTHING_CHANGED);
            return None;
        }
        _ => {}
    }
    state.reason = Some(reason);
    None
}

impl Waves {
    /// The tab of a TUI that runs in the clone of `here`, `OWNER/REPO`.
    pub fn new(here: Option<String>) -> Waves {
        Waves {
            here,
            ..Waves::default()
        }
    }

    /// The repository of the working folder, `OWNER/REPO`.
    pub fn here(&self) -> Option<&str> {
        self.here.as_deref()
    }

    /// The repositories that get a board, from the look `look`
    /// (01M6VFG882T794B1CTHK5AKPDG).
    pub fn repos(&self, look: &Look) -> Vec<String> {
        let sessions = Sessions::new(look.who.sessions.clone(), Vec::new());
        board_repos(&sessions, &Show::default(), self.here.as_deref())
            .into_iter()
            .collect()
    }

    /// True when a read of the forge is due at `now_ms`: none yet, or the
    /// last one is [`super::forge::READ_EVERY`] old.
    ///
    /// ```
    /// use riff::tui::waves::Waves;
    ///
    /// let mut waves = Waves::new(None);
    /// assert!(waves.due(0));
    /// waves.read(Vec::new(), 1_000);
    /// assert!(!waves.due(60_999));
    /// assert!(waves.due(61_000));
    /// ```
    pub fn due(&self, now_ms: i64) -> bool {
        let every = i64::try_from(super::forge::READ_EVERY.as_millis()).unwrap_or(i64::MAX);
        self.read_ms.is_none_or(|at| now_ms - at >= every)
    }

    /// Takes the read `read` of the forge of each repository at `at_ms`.
    /// A read that failed keeps the last facts of its repository
    /// (01M6VFGK4P2FWMFQHBZKR3JXSZ). A repository that `read` does not
    /// name has no board any more, and goes. It gives the line of the
    /// first fault when it is new, for the note line, and whether an item
    /// turned stuck, for the news of the tab.
    ///
    /// ```
    /// use riff::tui::forge::Forge;
    /// use riff::tui::waves::Waves;
    ///
    /// let mut waves = Waves::new(None);
    /// let (fault, _) = waves.read(vec![("o/r".into(), Ok(Forge::default()))], 0);
    /// assert_eq!(fault, None);
    /// let failed = || vec![("o/r".to_owned(), Err("gh: HTTP 502".to_owned()))];
    /// let (fault, _) = waves.read(failed(), 60_000);
    /// assert_eq!(fault.as_deref(), Some("riff: gh: HTTP 502"));
    /// // The same fault again: the note line named it once.
    /// assert_eq!(waves.read(failed(), 120_000).0, None);
    /// assert!(waves.forge("o/r").is_some(), "the last facts stay");
    /// ```
    pub fn read(
        &mut self,
        read: Vec<(String, Result<Forge, String>)>,
        at_ms: i64,
    ) -> (Option<String>, bool) {
        let stuck_before = self.stuck();
        let mut last = std::mem::take(&mut self.forge);
        let mut fault = None;
        for (repo, read) in read {
            match read {
                Ok(forge) => {
                    self.forge.insert(repo, forge);
                }
                Err(e) => {
                    fault.get_or_insert(text::top_gh_fault(&e));
                    if let Some(forge) = last.remove(&repo) {
                        self.forge.insert(repo, forge);
                    }
                }
            }
        }
        self.read_ms = Some(at_ms);
        let new = fault.clone().filter(|f| self.fault.as_ref() != Some(f));
        self.fault = fault;
        let turned = !self.stuck().is_subset(&stuck_before);
        (new, turned)
    }

    /// The stuck items of each repository.
    fn stuck(&self) -> BTreeSet<(String, u64)> {
        self.forge
            .iter()
            .flat_map(|(repo, forge)| forge.issues.stuck.keys().map(|n| (repo.clone(), *n)))
            .collect()
    }

    /// The last facts of the forge of `repo`.
    pub fn forge(&self, repo: &str) -> Option<&Forge> {
        self.forge.get(repo)
    }

    /// The time of the last read of the forge.
    pub fn read_at(&self) -> Option<i64> {
        self.read_ms
    }

    /// The selected row.
    pub fn picked(&self) -> Option<&Pick> {
        self.pick.as_ref()
    }

    /// The index of the selected row in `rows`: the pick, else the first
    /// item, else the first row that the person can select.
    pub fn selected(&self, rows: &[Row]) -> Option<usize> {
        let picks = |pick: &Pick| rows.iter().position(|r| r.pick().as_ref() == Some(pick));
        self.pick
            .as_ref()
            .and_then(picks)
            .or_else(|| rows.iter().position(|r| matches!(r, Row::Item { .. })))
            .or_else(|| rows.iter().position(|r| r.pick().is_some()))
    }

    /// Takes the key `key` of the person on the rows `rows` of the look
    /// `look` at the time `now_ms`, with `page` rows in a page
    /// (01M6VFGKCJ02G33TXR01GY6M0G, 01M6VFGK9XXWCA8X7CE8EZNSJV).
    pub fn on_key(
        &mut self,
        key: Key,
        rows: &[Row],
        (look, now_ms): (&Look, i64),
        page: usize,
    ) -> Option<Answer> {
        let picks: Vec<usize> = (0..rows.len())
            .filter(|&i| rows[i].pick().is_some())
            .collect();
        let at = self
            .selected(rows)
            .and_then(|i| picks.iter().position(|&p| p == i));
        let mut go = |to: usize| {
            if let Some(&row) = picks.get(to.min(picks.len().saturating_sub(1))) {
                self.pick = rows[row].pick();
            }
            None
        };
        let at_or_0 = at.unwrap_or(0);
        match key {
            Key::Up => go(at_or_0.saturating_sub(1)),
            Key::Down => go(at.map_or(0, |at| at + 1)),
            Key::PageUp => go(at_or_0.saturating_sub(page.max(1))),
            Key::PageDown => go(at_or_0 + page.max(1)),
            Key::First => go(0),
            Key::Last => go(picks.len().saturating_sub(1)),
            Key::Row(row) => {
                let pick = rows.get(row).and_then(Row::pick)?;
                if self.selected(rows) == Some(row) {
                    return self.enter(rows.get(row)?, now_ms);
                }
                self.pick = Some(pick);
                None
            }
            key => {
                let row = rows.get(self.selected(rows)?)?;
                self.act(key, row, look, now_ms)
            }
        }
    }

    /// Takes the key `key` on the selected row `row`.
    fn act(&mut self, key: Key, row: &Row, look: &Look, now_ms: i64) -> Option<Answer> {
        match (key, row) {
            (Key::Enter, row) => self.enter(row, now_ms),
            (Key::Left | Key::Right | Key::Char(' '), Row::Repo(repo)) => {
                let open = match key {
                    Key::Left => false,
                    Key::Right => true,
                    _ => !repo.open,
                };
                self.folds.insert(repo.repo.clone(), open);
                None
            }
            (Key::Char('h'), Row::Item { repo, item }) => Some(Answer::Reason(Reason {
                repo: repo.clone(),
                item: item.number,
                text: String::new(),
            })),
            (Key::Char('h'), _) => Some(Answer::Note("Select an item to hold it.".to_owned())),
            (Key::Char('f'), Row::Item { repo, item }) if item.state == ItemState::Held => {
                let free = Effect::Free {
                    repo: repo.clone(),
                    item: item.number,
                };
                Some(Answer::Ask(free.question()))
            }
            (Key::Char('f'), Row::Item { repo, item }) => Some(Answer::Note(format!(
                "#{} of {} has no hold.",
                item.number,
                text::safe(repo)
            ))),
            (Key::Char('f'), _) => Some(Answer::Note("Select a held item to free it.".to_owned())),
            // The label from the last read of the forge: an item in work
            // can have it too.
            (Key::Char('a'), Row::Item { repo, item }) => {
                let asked = self
                    .forge
                    .get(repo)
                    .is_some_and(|forge| forge.issues.questions.contains(&item.number));
                if !asked {
                    return Some(Answer::Note(text::questions_not_asked(repo, item.number)));
                }
                let answered = Effect::Answered {
                    repo: repo.clone(),
                    item: item.number,
                };
                Some(Answer::Ask(answered.question()))
            }
            (Key::Char('a'), _) => Some(Answer::Note(
                "Select an item with the label question to mark it as answered.".to_owned(),
            )),
            (Key::Char('p'), row) => {
                let repo = row.repo()?.to_owned();
                // The pause of the repository itself: the whole riff has `P`.
                let paused = repo
                    .parse::<ThreadName>()
                    .is_ok_and(|thread| look.pauses.repository(&thread).is_some());
                let pause = Effect::Pause {
                    repo: Some(repo),
                    state: if paused {
                        RiffState::Running
                    } else {
                        RiffState::Paused
                    },
                };
                Some(Answer::Ask(pause.question()))
            }
            (Key::Char('P'), _) => {
                let pause = Effect::Pause {
                    repo: None,
                    state: match look.pauses.riff {
                        Some(_) => RiffState::Running,
                        None => RiffState::Paused,
                    },
                };
                Some(Answer::Ask(pause.question()))
            }
            (Key::Char('o'), row) => Some(Answer::Note(url(row)?)),
            _ => None,
        }
    }

    /// `Enter` on `row`: it folds a board, shows the closed items, or puts
    /// the whole row of an item in the note line.
    fn enter(&mut self, row: &Row, now_ms: i64) -> Option<Answer> {
        match row {
            Row::Repo(repo) => {
                self.folds.insert(repo.repo.clone(), !repo.open);
                None
            }
            Row::Closed { repo, shown, .. } => {
                if *shown {
                    self.shown.remove(repo);
                } else {
                    self.shown.insert(repo.clone());
                }
                None
            }
            Row::Item { item, .. } => Some(Answer::Note(whole(item, now_ms))),
            _ => None,
        }
    }
}

/// True when the repository `repo` or the whole riff is paused in `look`.
fn paused(look: &Look, repo: &str) -> bool {
    look.pauses.riff.is_some()
        || repo
            .parse::<ThreadName>()
            .is_ok_and(|thread| look.pauses.repository(&thread).is_some())
}

/// The URL of the selected row: its pull request, its item, or its
/// repository (01M6VFGK9XXWCA8X7CE8EZNSJV).
///
/// ```
/// use riff::tui::waves::{Row, url};
///
/// assert_eq!(url(&Row::Text("x".into())), None);
/// ```
pub fn url(row: &Row) -> Option<String> {
    let base = |repo: &str| format!("https://github.com/{}", text::safe(repo));
    match row {
        Row::Item { repo, item } => Some(match item.pull {
            Some(pull) => format!("{}/pull/{pull}", base(repo)),
            None => format!("{}/issues/{}", base(repo), item.number),
        }),
        Row::Repo(row) => Some(base(&row.repo)),
        Row::Closed { repo, .. } => Some(format!("{}/issues?q=is%3Aclosed", base(repo))),
        _ => None,
    }
}

/// The whole row of `item` at `now_ms`, for the note line.
fn whole(item: &ItemRow, now_ms: i64) -> String {
    let mut parts = vec![
        format!("#{} {}", item.number, text::safe(&item.title)),
        item.state.word().to_owned(),
    ];
    if let Some(wave) = &item.wave {
        parts.push(text::safe(wave));
    }
    if item.who != "-" {
        parts.push(text::safe(&item.who));
    }
    if let Some(age) = age(item.since_ms, now_ms) {
        parts.push(age);
    }
    parts.join(" · ")
}

/// The age of a time `since_ms` at `now_ms`, as `2h`.
fn age(since_ms: Option<i64>, now_ms: i64) -> Option<String> {
    let since = since_ms?;
    Some(text::ago(
        u64::try_from((now_ms - since) / 1000).unwrap_or(0),
    ))
}

/// The short ID of a session: the first 8 characters.
fn short(s: &SessionInfo) -> String {
    s.uri
        .who()
        .session()
        .unwrap_or_default()
        .chars()
        .take(8)
        .collect()
}

/// The rows of the item of the board `board` of `repo`, with the facts
/// `forge`, the sessions `sessions`, the holds `holds` and the time of
/// the look `look_ms` (01M6VFGJWVB3WEEYBCNSZ2FJT0). The asked rows have
/// the questions of the later waves too ([`later_questions`]).
fn items(
    repo: &str,
    board: &Board,
    forge: &Forge,
    sessions: &Sessions,
    holds: &BTreeMap<String, HoldInfo>,
    look_ms: i64,
) -> Vec<ItemRow> {
    let title = |n: u64| forge.issues.titles.get(&n).cloned().unwrap_or_default();
    let pull = |n: u64| forge.pulls.get(&n);
    let pulled = |n: u64| pull(n).and_then(|p| p.updated).map(|s| s * 1000);
    let row = |number: u64, state: ItemState, who: String, since_ms: Option<i64>| ItemRow {
        number,
        state,
        title: title(number),
        who,
        since_ms,
        mark: state.mark(),
        pull: pull(number).map(|p| p.number),
        wave: None,
    };
    // The session of the claim `claim` of the item in `repo`.
    let holder = |n: u64, verify: bool| {
        sessions.on_server().find(|s| {
            s.uri.place().repo_text() == repo
                && s.uri.claims().iter().any(|c| {
                    riff_client::waves::issue_of(c) == Some(n) && c.starts_with("verify-") == verify
                })
        })
    };
    let mut rows = Vec::new();
    for (n, why) in &board.stuck {
        let who = match pull(*n) {
            Some(p) => format!("PR #{} {why}", p.number),
            None => why.clone(),
        };
        rows.push(row(*n, ItemState::Stuck, who, pulled(*n)));
    }
    for n in &board.free {
        rows.push(row(*n, ItemState::Free, "-".into(), None));
    }
    for (n, reason) in &board.held {
        let at = holds
            .get(&format!("issue-{n}"))
            .and_then(|h| i64::try_from(h.at_ms).ok());
        rows.push(row(*n, ItemState::Held, reason.clone(), at));
    }
    // The questions of the current wave and of each later wave, in
    // number order.
    let mut asked: Vec<(u64, Option<String>)> = board.question.iter().map(|n| (*n, None)).collect();
    asked.extend(
        later_questions(&forge.issues)
            .into_iter()
            .map(|(n, wave)| (n, Some(wave))),
    );
    asked.sort_unstable();
    for (n, wave) in asked {
        let mut item = row(n, ItemState::Asked, "waits for a person".into(), None);
        item.wave = wave;
        rows.push(item);
    }
    for n in &board.claimed {
        let mut item = row(*n, ItemState::Work, "-".into(), None);
        if let Some(s) = holder(*n, false) {
            let word = s.state.map(|state| state.word()).unwrap_or_default();
            item.who = format!("{} {word}", short(s)).trim_end().to_owned();
            let secs = s.blocked.as_ref().map_or(s.claims_secs, |b| b.secs);
            item.since_ms = Some(look_ms - i64::try_from(secs).unwrap_or(0) * 1000);
            item.mark = match Mark::of(s.state, None) {
                Some(mark) => (mark.symbol(), mark.style()),
                None => (" ", Style::new()),
            };
        }
        rows.push(item);
    }
    // A verify or a rebase that waits for its turn names the pull request
    // before it (#1801).
    let before = |n: u64| {
        let mut waits = board.waits.iter().chain(&board.rebases);
        waits.find(|(w, _)| *w == n).map(|(_, first)| *first)
    };
    let mut verify: Vec<u64> = board.verify.clone();
    verify.extend(board.rebases.iter().map(|(n, _)| *n));
    verify.sort_unstable();
    for n in verify {
        let who = match (pull(n), holder(n, true), before(n)) {
            (Some(p), _, Some(first)) => format!("PR #{} after #{first}", p.number),
            (Some(p), _, None) => format!("PR #{} {}", p.number, p.word),
            (None, Some(s), _) => format!("{} verifies", short(s)),
            (None, None, Some(first)) => format!("after #{first}"),
            (None, None, None) => "-".into(),
        };
        rows.push(row(n, ItemState::Verify, who, pulled(n)));
    }
    for n in &board.tracker {
        rows.push(row(*n, ItemState::Tracker, "-".into(), None));
    }
    rows
}

/// The row of a closed item.
fn ended(closed: &Closed) -> ItemRow {
    let state = if closed.done {
        ItemState::Done
    } else {
        ItemState::Closed
    };
    ItemRow {
        number: closed.number,
        state,
        title: closed.title.clone(),
        who: "-".into(),
        since_ms: None,
        mark: state.mark(),
        pull: None,
        wave: None,
    }
}

/// The count of each state of `items`, in the order of the states.
fn counts(items: &[ItemRow]) -> Vec<(usize, ItemState)> {
    let mut counts: BTreeMap<ItemState, usize> = BTreeMap::new();
    for item in items {
        *counts.entry(item.state).or_default() += 1;
    }
    counts.into_iter().map(|(state, n)| (n, state)).collect()
}

/// The rows of the Waves tab of `waves` with the look `look` at `look_ms`
/// (01M6VFG882T794B1CTHK5AKPDG, 01M6VFGJWVB3WEEYBCNSZ2FJT0,
/// 01M6VFGJZF7XGNGVWTECKAH429, 01M6VFGK22ZC1D7WF6PP853TP9).
pub fn rows(waves: &Waves, look: &Look, look_ms: i64) -> Vec<Row> {
    let sessions = Sessions::new(look.who.sessions.clone(), Vec::new());
    let repos = board_repos(&sessions, &Show::default(), waves.here.as_deref());
    if repos.is_empty() {
        return NO_BOARD.map(|line| Row::Text(line.into())).to_vec();
    }
    let none = BTreeMap::new();
    let daemons = riff_client::daemons::hosts(&look.who.sessions);
    // Each repository: the order key, its line, and the rows of its board.
    let mut boards: Vec<(u8, RepoRow, Vec<Row>)> = Vec::new();
    for repo in repos {
        let paused = paused(look, &repo);
        let folded = |wave| RepoRow {
            repo: repo.clone(),
            open: false,
            wave,
            counts: Vec::new(),
            paused,
        };
        let Some(forge) = waves.forge.get(&repo) else {
            boards.push((2, folded(Wave::Unread), Vec::new()));
            continue;
        };
        let holds = look.holds.get(&repo).unwrap_or(&none);
        // The verifies and the rebases that wait for their turn, from the
        // facts of the daemons (#1801).
        let waits = riff_client::daemons::waits(&daemons, &repo);
        let issues = forge.issues.clone().with_waits(waits);
        let Some(board) = Board::of(&repo, &issues, &sessions, holds) else {
            boards.push((2, folded(Wave::None), Vec::new()));
            continue;
        };
        let items = items(&repo, &board, forge, &sessions, holds, look_ms);
        let looks = items.iter().any(|i| {
            matches!(
                i.state,
                ItemState::Stuck | ItemState::Held | ItemState::Free
            )
        });
        let open = waves.folds.get(&repo).copied().unwrap_or(looks);
        let closed = forge.closed.as_deref().unwrap_or_default();
        let mut counted = counts(&items);
        if !open {
            counted.extend(counts(&closed.iter().map(ended).collect::<Vec<_>>()));
        }
        let line = RepoRow {
            repo: repo.clone(),
            open,
            wave: Wave::Named(wave_name(&board.wave)),
            counts: counted,
            paused,
        };
        let mut under = Vec::new();
        if open {
            under.push(Row::Columns);
            under.extend(items.into_iter().map(|item| Row::Item {
                repo: repo.clone(),
                item,
            }));
            let done = closed.iter().filter(|c| c.done).count();
            if !closed.is_empty() {
                let shown = waves.shown.contains(&repo);
                under.push(Row::Closed {
                    repo: repo.clone(),
                    done,
                    closed: closed.len() - done,
                    shown,
                });
                if shown {
                    under.extend(closed.iter().map(|c| Row::Item {
                        repo: repo.clone(),
                        item: ended(c),
                    }));
                }
            }
            under.extend(forge.main.clone().map(Row::Main));
            under.extend(
                forge
                    .releases
                    .iter()
                    .map(|(n, title)| Row::Release(*n, title.clone())),
            );
        }
        boards.push((u8::from(!looks), line, under));
    }
    boards.sort_by(|a, b| (a.0, &a.1.repo).cmp(&(b.0, &b.1.repo)));
    let mut rows = Vec::new();
    for (_, line, under) in boards {
        rows.push(Row::Repo(line));
        rows.extend(under);
    }
    // The next waves of the repository of the selected row, else of the
    // first board.
    let repo = waves
        .selected(&rows)
        .and_then(|i| rows[i].repo())
        .map(str::to_owned);
    let next = repo
        .into_iter()
        .chain(waves.forge.keys().cloned())
        .find_map(|repo| Some((next_waves(waves.forge.get(&repo)?)?, repo)));
    if let Some((next, repo)) = next {
        rows.extend([Row::Blank, Row::NextTitle(repo), Row::Next(next)]);
    }
    rows
}

/// The next waves of `forge`: each later wave and the Backlog, with
/// their open items. `None` when it has none.
fn next_waves(forge: &Forge) -> Option<Vec<(String, usize)>> {
    let mut next: Vec<(String, usize)> = forge
        .issues
        .waves
        .iter()
        .skip(1)
        .map(|(title, items)| (wave_name(title), items.len()))
        .collect();
    if forge.backlog > 0 {
        next.push((super::forge::BACKLOG.to_owned(), forge.backlog));
    }
    (!next.is_empty()).then_some(next)
}

/// The short name of the wave with the milestone `title`: `Wave N`, or
/// the title when it names no wave.
///
/// ```
/// use riff::tui::waves::wave_name;
///
/// assert_eq!(wave_name("Wave 56: the projects and the roles"), "Wave 56");
/// assert_eq!(wave_name("Wave 49"), "Wave 49");
/// assert_eq!(wave_name("Backlog"), "Backlog");
/// ```
pub fn wave_name(title: &str) -> String {
    wave_number(title).map_or_else(|| title.to_owned(), |n| format!("Wave {n}"))
}

/// The questions of the later waves of `issues`, as `riff questions`
/// finds them: each open issue of a wave after the current one with the
/// label [`riff_client::waves::QUESTION`], with the name of its wave
/// ([`wave_name`]), in the order of the waves
/// (01M6Y0ZQWDYVC23WA5CVN0QSH8). The board of the current wave shows them
/// as asked rows.
///
/// ```
/// use riff::tui::waves::later_questions;
/// use riff_client::waves::{Issue, Issues};
///
/// let issue = |number, wave: &str, question| Issue {
///     number,
///     title: "t".into(),
///     milestone: Some(wave.into()),
///     tracker: false,
///     question,
/// };
/// let issues = Issues::of(vec![
///     issue(1700, "Wave 52", true),
///     issue(1668, "Wave 56: the projects", true),
///     issue(1669, "Wave 56: the projects", false),
///     issue(1500, "Backlog", true),
/// ]);
/// assert_eq!(later_questions(&issues), [(1668, "Wave 56".to_owned())]);
/// ```
pub fn later_questions(issues: &Issues) -> Vec<(u64, String)> {
    issues
        .waves
        .iter()
        .skip(1)
        .flat_map(|(title, items)| {
            items
                .iter()
                .filter(|n| issues.questions.contains(n))
                .map(|n| (*n, wave_name(title)))
        })
        .collect()
}

/// `n items`, or `1 item`.
fn items_of(n: usize) -> String {
    match n {
        1 => "1 item".to_owned(),
        n => format!("{n} items"),
    }
}

/// The width of `text` in columns.
fn width(text: &str) -> usize {
    Line::raw(text).width()
}

/// `text` cut to `columns` with `…` at its end when it is wider.
///
/// ```
/// use riff::tui::waves::cut;
///
/// assert_eq!(cut("Fix the help", 20), "Fix the help");
/// assert_eq!(cut("The book names the flag", 21), "The book names the f…");
/// assert_eq!(cut("abc", 0), "");
/// ```
pub fn cut(text: &str, columns: usize) -> String {
    if width(text) <= columns {
        return text.to_owned();
    }
    let mut out = String::new();
    for c in text.chars() {
        if width(&out) + width(&c.to_string()) + 1 > columns {
            break;
        }
        out.push(c);
    }
    if columns > 0 {
        out.push('…');
    }
    out
}

/// `text` cut to `columns - 1` and padded to `columns`: a column with
/// one space after it.
fn column(text: &str, columns: usize) -> String {
    let text = cut(text, columns.saturating_sub(1));
    let pad = columns.saturating_sub(width(&text));
    format!("{text}{}", " ".repeat(pad))
}

/// The lines of the body of the Waves tab of `waves` with the look
/// `look`, in `columns` and `rows` of the layout `layout` at the time
/// `now_ms`, and for each line the index of its row in `all`, the rows
/// of [`rows`]. Only the lines that fit show, around the selected row.
pub fn body(
    waves: &Waves,
    all: &[Row],
    layout: Layout,
    columns: u16,
    rows: u16,
    now_ms: i64,
) -> Vec<(Line<'static>, usize)> {
    let wide = layout == Layout::Wide;
    let selected = waves.selected(all);
    let lines: Vec<(Line<'static>, usize)> = all
        .iter()
        .enumerate()
        .filter(|(_, row)| wide || **row != Row::Columns)
        .map(|(i, row)| {
            let line = line(row, selected == Some(i), wide, usize::from(columns), now_ms);
            (line, i)
        })
        .collect();
    let height = usize::from(rows);
    let at = selected
        .and_then(|s| lines.iter().position(|(_, i)| *i == s))
        .unwrap_or(0);
    let start = (at + 1).saturating_sub(height);
    lines.into_iter().skip(start).take(height).collect()
}

/// The line of `row`, `selected` or not, in the wide layout or not, in
/// `columns` at `now_ms`.
fn line(row: &Row, selected: bool, wide: bool, columns: usize, now_ms: i64) -> Line<'static> {
    let sel = if selected { ">" } else { " " };
    let line = match row {
        Row::Repo(repo) => repo_line(repo, wide, columns),
        Row::Columns => Line::styled(
            format!(
                "      {}{}{}{}AGE",
                column("STATE", 8),
                column("ITEM", 7),
                column("TITLE", title_columns(columns)),
                column("WHO / PULL REQUEST", 21),
            ),
            DIM,
        ),
        Row::Item { item, .. } => item_line(item, sel, wide, columns, now_ms),
        Row::Closed {
            done,
            closed,
            shown,
            ..
        } => {
            let mut text = format!("done {done} · closed {closed}");
            if wide {
                text.push_str(if *shown {
                    " (Enter hides them)"
                } else {
                    " (Enter shows them)"
                });
            }
            let pad = if wide { "     " } else { " " };
            Line::raw(cut(&format!("{sel}{pad}{text}"), columns))
        }
        Row::Main(main) => {
            let style = if main.broken() { ERROR } else { WARNING };
            let pad = if wide { "      " } else { "  " };
            Line::styled(cut(&format!("{pad}{}", main.line(!wide)), columns), style)
        }
        Row::Release(number, title) => {
            let pad = if wide { "      " } else { "  " };
            let text = format!(
                "{pad}{} (#{number}): its checks after the deploy have not passed",
                text::safe(title)
            );
            Line::styled(cut(&text, columns), WARNING)
        }
        Row::Blank => Line::raw(""),
        Row::NextTitle(repo) => {
            let name = if wide {
                repo.as_str()
            } else {
                short_repo(repo)
            };
            Line::styled(
                cut(&format!("NEXT WAVES · {}", text::safe(name)), columns),
                BOLD,
            )
        }
        Row::Next(next) => {
            let parts: Vec<String> = next
                .iter()
                .map(|(name, n)| format!("{}: {}", text::safe(name), items_of(*n)))
                .collect();
            let gap = if wide { "    " } else { " · " };
            Line::raw(cut(&format!("  {}", parts.join(gap)), columns))
        }
        Row::Text(text) => Line::raw(cut(text, columns)),
    };
    if selected {
        line.patch_style(SELECTED)
    } else {
        line
    }
}

/// The columns of the title in a wide line of `columns`: the rest of
/// the line after the other columns.
fn title_columns(columns: usize) -> usize {
    columns.saturating_sub(47).max(10)
}

/// The name of `repo` with no owner, for the narrow layout
/// (01M6VFGKF574TCZ7C3AW3MWX4S) and the marks of the Machines tab
/// ([`super::machines::marks`]).
pub(super) fn short_repo(repo: &str) -> &str {
    repo.rsplit_once('/').map_or(repo, |(_, name)| name)
}

/// The line of a repository.
fn repo_line(repo: &RepoRow, wide: bool, columns: usize) -> Line<'static> {
    let fold = if repo.open { "▼" } else { "▶" };
    let name = if wide {
        repo.repo.as_str()
    } else {
        short_repo(&repo.repo)
    };
    let wave = match &repo.wave {
        Wave::Named(wave) => text::safe(wave),
        Wave::None => "no wave".to_owned(),
        Wave::Unread => "not read yet".to_owned(),
    };
    let mut left = vec![Span::raw(format!("{fold} {} · {wave}", text::safe(name)))];
    if repo.paused {
        left.extend([Span::raw(" · "), Span::styled("paused", WARNING)]);
    }
    let counts: Vec<String> = repo
        .counts
        .iter()
        .map(|(n, state)| format!("{n} {}", state.word()))
        .collect();
    let counts = counts.join(" ");
    let used: usize = left.iter().map(|s| width(&s.content)).sum();
    let mut line = Line::from(left);
    if counts.is_empty() || (!wide && repo.open) {
        return line;
    }
    if wide && used + 2 + width(&counts) <= columns {
        let pad = columns - used - width(&counts);
        line.spans.push(Span::raw(" ".repeat(pad)));
        line.spans.push(Span::raw(counts));
    } else {
        let rest = columns.saturating_sub(used + 3);
        line.spans.push(Span::raw(" · "));
        line.spans.push(Span::raw(cut(&counts, rest)));
    }
    line
}

/// The line of an item.
fn item_line(item: &ItemRow, sel: &str, wide: bool, columns: usize, now_ms: i64) -> Line<'static> {
    let (mark, style) = item.mark;
    let number = format!("#{}", item.number);
    // A question of a later wave names its wave first
    // (01M6Y0ZQWDYVC23WA5CVN0QSH8).
    let title = match &item.wave {
        Some(wave) => format!("{} · {}", text::safe(wave), text::safe(&item.title)),
        None => text::safe(&item.title),
    };
    if !wide {
        // `> ○ free    #1620  Add the flag --format`: the title starts at
        // the column 19 (01M6VFGKF574TCZ7C3AW3MWX4S).
        return Line::from(vec![
            Span::raw(format!("{sel} ")),
            Span::styled(mark, style),
            Span::raw(format!(" {:<7} {number:<6} ", item.state.word())),
            Span::raw(cut(&title, columns.saturating_sub(19))),
        ]);
    }
    let age = age(item.since_ms, now_ms).unwrap_or_else(|| "-".to_owned());
    Line::from(vec![
        Span::raw(format!("{sel} ")),
        Span::styled(mark, style),
        Span::raw("   "),
        Span::raw(column(item.state.word(), 8)),
        Span::raw(column(&number, 7)),
        Span::raw(column(&title, title_columns(columns))),
        Span::raw(column(&text::safe(&item.who), 21)),
        Span::raw(age),
    ])
}

/// The note of the Waves tab when the note line has no other
/// (01M6VFGYW0XVJ5JSX6R1NCR6W7): the age of the last read of the forge,
/// or that the first read runs.
///
/// ```
/// use riff::tui::waves::{Waves, note};
///
/// let mut waves = Waves::new(None);
/// assert_eq!(note(&waves, 0), "The boards come from gh: the first read runs.");
/// waves.read(Vec::new(), 1_000);
/// assert_eq!(note(&waves, 42_000), "The boards come from gh, 41s ago.");
/// ```
pub fn note(waves: &Waves, now_ms: i64) -> String {
    match age(waves.read_ms, now_ms) {
        Some(age) => format!("The boards come from gh, {age} ago."),
        None => "The boards come from gh: the first read runs.".to_owned(),
    }
}

/// The keys of the key line of the Waves tab in `layout`: the text of
/// the key, its word, and the key that a click on it presses.
pub fn keys_of(layout: Layout) -> Vec<(&'static str, &'static str, Option<Key>)> {
    match layout {
        Layout::Wide => vec![
            ("↑↓", "select", None),
            ("Enter", "open", Some(Key::Enter)),
            ("h", "hold", Some(Key::Char('h'))),
            ("f", "free", Some(Key::Char('f'))),
            ("p", "pause", Some(Key::Char('p'))),
            ("o", "URL", Some(Key::Char('o'))),
            ("r", "read again", Some(Key::Again)),
            ("?", "help", Some(Key::Help)),
        ],
        _ => vec![
            ("↑↓", "select", None),
            ("h", "hold", Some(Key::Char('h'))),
            ("?", "help", Some(Key::Help)),
            ("q", "quit", Some(Key::Quit)),
        ],
    }
}

/// The keys of the Waves tab in the help popup, before the keys of the
/// frame.
pub fn help_keys() -> Vec<(&'static str, &'static str)> {
    vec![
        ("↑ ↓, j k", "select a row"),
        ("PgUp PgDn", "move a page"),
        ("g G", "the first or last row"),
        ("Enter", "open the row"),
        ("← → Space", "fold a board"),
        ("h", "hold the item"),
        ("f", "free the item"),
        ("a", "mark as answered"),
        ("p", "pause the repository"),
        ("P", "pause the whole riff"),
        ("o", "show the URL"),
        ("y n", "answer the question"),
    ]
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use riff_client::waves::{Issue, Issues};
    use riff_core::build::Build;
    use riff_core::wire::RiffOwner;

    use super::*;
    use crate::tui::forge::ItemPull;
    use crate::tui::view::{compute_view, render};
    use crate::tui::{Action, Effect, News, State, Tab};

    /// The time of the screen tests, in milliseconds since the epoch.
    const NOW: i64 = 1_794_000_000_000;

    const RIFF: &str = "como-technologies/riff";

    fn build(id: &str) -> Build {
        format!("0.1.0 {id} 2026-11-06T12:00:00Z").parse().unwrap()
    }

    /// A live session of `user` in `repo` with the claim `claim`, the
    /// state `state` and more facts in `more`.
    fn session(
        id: &str,
        repo: &str,
        claim: &str,
        state: &str,
        more: serde_json::Value,
    ) -> SessionInfo {
        let mut json = serde_json::json!({
            "uri": format!("riff://mike@thelio/{repo}?session={id}&claim={claim}"),
            "live": true,
            "state": state,
        });
        json.as_object_mut()
            .unwrap()
            .extend(more.as_object().unwrap().clone());
        serde_json::from_value(json).unwrap()
    }

    fn issue(number: u64, title: &str, wave: &str) -> Issue {
        Issue {
            number,
            title: title.into(),
            milestone: Some(wave.into()),
            tracker: false,
            question: false,
        }
    }

    /// The facts of the forge of riff in the mock-up of #994: Wave 48 with
    /// a stuck item, two free, one held, three in work and two in verify;
    /// 4 done and 1 closed; the next waves; `main` runs.
    fn riff_forge() -> Forge {
        let wave = "Wave 48: what 4.9.0 left, and the design of the TUI";
        let mut list = vec![
            issue(1601, "Fix the help", wave),
            issue(1620, "Add the flag --format", wave),
            issue(1622, "The book names the flag", wave),
            issue(1631, "Design: the web UI", wave),
            issue(994, "Design: the TUI of riff", wave),
            issue(1590, "An idle look asks GitHub first", wave),
            issue(1594, "riff counts the gh calls", wave),
            issue(1599, "The wave takes an issue", wave),
            issue(1600, "The book tells how to detach", wave),
        ];
        list.extend((1..=8).map(|n| issue(1700 + n, "later", "Wave 49")));
        list.extend((1..=3).map(|n| issue(1800 + n, "later", "Wave 50")));
        let ago = |secs: i64| Some(NOW / 1000 - secs);
        let pull = |number: u64, word: &str, secs: i64| ItemPull {
            number,
            word: word.into(),
            updated: ago(secs),
        };
        Forge {
            issues: Issues::of(list)
                .with_verify([1599, 1600, 1601])
                .with_stuck([(1601, "conflict".to_owned())]),
            milestones: BTreeMap::from([(wave.to_owned(), 54)]),
            backlog: 41,
            pulls: BTreeMap::from([
                (1601, pull(1604, "conflict", 7200)),
                (1599, pull(1614, "verify", 300)),
                (1600, pull(1613, "merge", 540)),
            ]),
            closed: Some(
                (1..=5)
                    .map(|n| Closed {
                        number: 1500 + n,
                        title: format!("closed {n}"),
                        done: n != 5,
                    })
                    .collect(),
            ),
            main: Some(Main::Runs {
                commit: "3f2a91c0ffee".into(),
                done: 4,
                jobs: 9,
            }),
            releases: Vec::new(),
        }
    }

    /// The state of the mock-up of the Waves tab of #994: three
    /// repositories, the board of riff open, strata with one item in work
    /// and kadomony with no wave, 2 seconds after a good look and 41
    /// seconds after the read of the forge, with #1620 selected.
    fn mock_up() -> State {
        let mut state = State::new(build("10df8a4c0ffe"), "riff://mike@thelio".parse().unwrap());
        let mut look = Look::default();
        look.who.owner = RiffOwner::Owner {
            user: "mike".into(),
            email: "m@x.io".into(),
        };
        look.server = Some(build("10df8a4c0ffe"));
        let blocked = serde_json::json!({"blocked": {"reason": "which design?", "secs": 1920}});
        look.who.sessions = vec![
            session("3a3f8d5d00", RIFF, "issue-994", "blocked", blocked),
            session(
                "5b1e2a9000",
                RIFF,
                "issue-1590",
                "busy",
                serde_json::json!({"claims_secs": 2400}),
            ),
            session(
                "9c0d1e2f00",
                RIFF,
                "issue-1594",
                "busy",
                serde_json::json!({"claims_secs": 3600}),
            ),
            session(
                "7d2c1b0a00",
                "como-technologies/strata",
                "issue-31",
                "busy",
                serde_json::json!({}),
            ),
            session(
                "4e54d4e500",
                "como-technologies/kadomony",
                "",
                "idle",
                serde_json::json!({}),
            ),
        ];
        let hold = HoldInfo {
            reason: "waits for mike".into(),
            by: None,
            at_ms: u64::try_from(NOW - 3 * 86_400_000).unwrap(),
        };
        look.holds = BTreeMap::from([(
            RIFF.to_owned(),
            BTreeMap::from([("issue-1631".to_owned(), hold)]),
        )]);
        state.looked(look, NOW - 2_000);
        state.news(Tab::Chat, News::Lines(2));
        state.on_key(Key::Open(Tab::Waves));
        let strata = Forge {
            issues: Issues::of(vec![issue(31, "Ship it", "Wave 5")]),
            milestones: BTreeMap::from([("Wave 5".to_owned(), 5)]),
            closed: Some(
                (1..=3)
                    .map(|n| Closed {
                        number: 20 + n,
                        title: "done".into(),
                        done: true,
                    })
                    .collect(),
            ),
            ..Forge::default()
        };
        state.read_forge(
            vec![
                (RIFF.to_owned(), Ok(riff_forge())),
                ("como-technologies/strata".to_owned(), Ok(strata)),
                (
                    "como-technologies/kadomony".to_owned(),
                    Ok(Forge::default()),
                ),
            ],
            NOW - 41_000,
        );
        state.drew(20, NOW);
        state.on_key(Key::Down);
        state
    }

    /// The rows of `state` drawn into a `Buffer` of `width` x `height`,
    /// with no spaces at their ends.
    fn drawn(state: &State, width: u16, height: u16) -> Vec<String> {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        render(&compute_view(state, area, NOW), &mut buffer);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    /// The screen test of the mock-up of the Waves tab of #994 at 80 x 24
    /// (01M6VFG882T794B1CTHK5AKPDG, 01M6VFGJWVB3WEEYBCNSZ2FJT0,
    /// 01M6VFGJZF7XGNGVWTECKAH429, 01M6VFGK22ZC1D7WF6PP853TP9).
    #[test]
    fn the_waves_tab_at_80_x_24_is_the_mock_up() {
        let want = [
            " 1 Sessions  2 Chat•2  [3 Waves]  4 Machines  5 Devices  6 Reports",
            " riff running · owner mike · build 10df8a4 · link ok 2s",
            " ▼ como-technologies/riff · Wave 48        1 stuck 2 free 1 held 3 work 2 verify",
            "       STATE   ITEM   TITLE                           WHO / PULL REQUEST   AGE",
            "   !   stuck   #1601  Fix the help                    PR #1604 conflict    2h",
            " > ○   free    #1620  Add the flag --format           -                    -",
            "   ○   free    #1622  The book names the flag         -                    -",
            "   ◌   held    #1631  Design: the web UI              waits for mike       3d",
            "   !   work    #994   Design: the TUI of riff         3a3f8d5d blocked     32m",
            "   ●   work    #1590  An idle look asks GitHub first  5b1e2a90 busy        40m",
            "   ●   work    #1594  riff counts the gh calls        9c0d1e2f busy        1h",
            "   ◐   verify  #1599  The wave takes an issue         PR #1614 verify      5m",
            "   ◐   verify  #1600  The book tells how to detach    PR #1613 merge       9m",
            "       done 4 · closed 1 (Enter shows them)",
            "       main: the full check of 3f2a91c runs · 4 of 9 jobs done",
            " ▶ como-technologies/strata · Wave 5                               1 work 3 done",
            " ▶ como-technologies/kadomony · no wave",
            "",
            " NEXT WAVES · como-technologies/riff",
            "   Wave 49: 8 items    Wave 50: 3 items    Backlog: 41 items",
            "",
            "",
            " The boards come from gh, 41s ago.",
            " ↑↓ select  Enter open  h hold  f free  p pause  o URL  r read again  ? help",
        ];
        assert_eq!(drawn(&mock_up(), 80, 24), want);
    }

    /// The number of the items of each state in the rows `rows` of a
    /// mock-up: from the counts of the line of its open board, and from
    /// its rows of items.
    fn counts_and_rows(rows: &[String]) -> (BTreeMap<String, usize>, BTreeMap<String, usize>) {
        let board = rows.iter().find(|row| row.starts_with(" ▼ ")).unwrap();
        let words: Vec<&str> = board
            .split("Wave 48")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .collect();
        let counts = words
            .chunks(2)
            .map(|pair| (pair[1].to_owned(), pair[0].parse().unwrap()))
            .collect();
        let mut items = BTreeMap::new();
        for row in rows {
            let words: Vec<&str> = row.split_whitespace().skip_while(|w| *w == ">").collect();
            if let [_, state, item, ..] = words[..]
                && item.starts_with('#')
            {
                *items.entry(state.to_owned()).or_insert(0) += 1;
            }
        }
        (counts, items)
    }

    /// The mock-up in the rustdoc of this module and in "See the open
    /// items of a wave" of the book is the screen of the 80 x 24 test, and
    /// the counts of its board match its rows (#1707).
    #[test]
    fn the_rustdoc_and_the_book_show_the_screen_and_its_counts_match_its_rows() {
        use crate::tui::text_block;

        let screen = drawn(&mock_up(), 80, 24);
        let rustdoc = text_block(include_str!("waves.rs"), "//! # Design", "//! ");
        assert_eq!(rustdoc, screen);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/waves.md"
        ))
        .unwrap();
        let book = text_block(&book, "### See the open items of a wave", "");
        assert_eq!(book, screen);
        let (counts, rows) = counts_and_rows(&book);
        assert_eq!(counts, rows);
        assert_eq!(rows["work"], 3, "{book:#?}");
        // A count that does not match its rows fails.
        let mut wrong = book.clone();
        wrong.retain(|row| !row.contains("#1594"));
        let (counts, rows) = counts_and_rows(&wrong);
        assert_ne!(counts, rows);
    }

    /// The screen test of the narrow layout of the Waves tab at 40 x 24
    /// (01M6VAZKK4KHCDCX4RKG9B95D7, 01M6VFGKF574TCZ7C3AW3MWX4S).
    #[test]
    fn the_waves_tab_at_40_x_24_is_the_narrow_mock_up() {
        let want = [
            "‹ 3/6 Waves ›",
            "riff running · link ok 2s",
            "▼ riff · Wave 48",
            "  ! stuck   #1601  Fix the help",
            "> ○ free    #1620  Add the flag --format",
            "  ○ free    #1622  The book names the f…",
            "  ◌ held    #1631  Design: the web UI",
            "  ! work    #994   Design: the TUI of r…",
            "  ● work    #1590  An idle look asks Gi…",
            "  ● work    #1594  riff counts the gh c…",
            "  ◐ verify  #1599  The wave takes an is…",
            "  ◐ verify  #1600  The book tells how t…",
            "  done 4 · closed 1",
            "  main: 3f2a91c runs · 4 of 9 jobs done",
            "▶ strata · Wave 5 · 1 work 3 done",
            "▶ kadomony · no wave",
            "",
            "NEXT WAVES · riff",
            "  Wave 49: 8 items · Wave 50: 3 items ·…",
            "",
            "",
            "",
            "The boards come from gh, 41s ago.",
            "↑↓ select  h hold  ? help  q quit",
        ];
        assert_eq!(drawn(&mock_up(), 40, 24), want);
    }

    /// The selected row of the drawn rows: the one that starts with `>`.
    fn selected(rows: &[String]) -> &str {
        rows.iter()
            .find(|r| r.starts_with(" >"))
            .map_or("", |r| r.as_str())
    }

    /// 01M6VFGKCJ02G33TXR01GY6M0G: the keys select the row above or
    /// below, a page, the first or the last row. The selection stays on
    /// its item when a read moves the rows.
    #[test]
    fn the_keys_select_a_row_and_the_selection_stays_on_its_item() {
        let mut state = mock_up();
        state.on_key(Key::Char('j'));
        assert!(selected(&drawn(&state, 80, 24)).contains("#1622"));
        state.on_key(Key::Up);
        state.on_key(Key::Char('k'));
        assert!(selected(&drawn(&state, 80, 24)).contains("#1601"));
        // The line of a repository has no `>`: it is drawn reversed.
        state.on_key(Key::Up);
        assert_eq!(state.waves().picked(), Some(&Pick::Repo(RIFF.into())));
        state.on_key(Key::Char('G'));
        assert_eq!(
            state.waves().picked(),
            Some(&Pick::Repo("como-technologies/kadomony".into()))
        );
        state.on_key(Key::Char('g'));
        assert_eq!(state.waves().picked(), Some(&Pick::Repo(RIFF.into())));
        state.drew(5, NOW);
        state.on_key(Key::PageDown);
        assert_eq!(state.waves().picked(), Some(&Pick::Item(RIFF.into(), 994)));
        state.on_key(Key::PageUp);
        state.on_key(Key::PageUp);
        assert_eq!(state.waves().picked(), Some(&Pick::Repo(RIFF.into())));
        // A read where #1620 is gone and #994 moved: the pick stays.
        state.on_key(Key::PageDown);
        let mut forge = riff_forge();
        forge.issues.titles.remove(&1620);
        for (_, items) in &mut forge.issues.waves {
            items.retain(|n| *n != 1620);
        }
        state.read_forge(vec![(RIFF.to_owned(), Ok(forge))], NOW);
        assert!(selected(&drawn(&state, 80, 24)).contains("#994 "));
    }

    /// 01M6VFG882T794B1CTHK5AKPDG: `Enter`, `←`, `→` and `Space` on the
    /// line of a repository fold or unfold its board.
    #[test]
    fn the_keys_fold_and_unfold_a_board() {
        let mut state = mock_up();
        state.on_key(Key::Char('G'));
        state.on_key(Key::Up);
        assert_eq!(
            state.waves().picked(),
            Some(&Pick::Repo("como-technologies/strata".into()))
        );
        state.on_key(Key::Enter);
        let rows = drawn(&state, 80, 24);
        let at = rows
            .iter()
            .position(|r| r.contains("como-technologies/strata"))
            .unwrap();
        assert!(
            rows[at].starts_with(" ▼ como-technologies/strata · Wave 5"),
            "{rows:#?}"
        );
        assert!(
            !rows[at].contains("3 done"),
            "an open board counts no closed item"
        );
        assert!(rows[at + 2].contains("#31") && rows[at + 2].contains("7d2c1b0a busy"));
        assert_eq!(rows[at + 3], "       done 3 · closed 0 (Enter shows them)");
        state.on_key(Key::Left);
        assert!(drawn(&state, 80, 24)[at].starts_with(" ▶ como-technologies/strata"));
        state.on_key(Key::Right);
        assert!(drawn(&state, 80, 24)[at].starts_with(" ▼ como-technologies/strata"));
        state.on_key(Key::Char(' '));
        assert!(drawn(&state, 80, 24)[at].starts_with(" ▶ como-technologies/strata"));
        // The board of riff folds too, and its line counts the closed items.
        state.on_key(Key::Char('g'));
        state.on_key(Key::Enter);
        let rows = drawn(&state, 80, 24);
        assert!(
            rows[2].starts_with(" ▶ como-technologies/riff · Wave 48"),
            "{rows:#?}"
        );
        // The counts do not fit on the right: they follow, cut.
        assert_eq!(
            rows[2],
            " ▶ como-technologies/riff · Wave 48 · 1 stuck 2 free 1 held 3 work 2 verify 4 d…"
        );
    }

    /// 01M6VFGJWVB3WEEYBCNSZ2FJT0: `Enter` on the line of the closed items
    /// shows them, and again hides them.
    #[test]
    fn enter_shows_the_closed_items_of_the_wave() {
        let mut state = mock_up();
        while state.waves().picked() != Some(&Pick::Closed(RIFF.into())) {
            state.on_key(Key::Down);
        }
        state.on_key(Key::Enter);
        let rows = drawn(&state, 80, 30);
        let at = rows
            .iter()
            .position(|r| r.contains("done 4 · closed 1"))
            .unwrap();
        assert!(rows[at].ends_with("(Enter hides them)"), "{}", rows[at]);
        assert!(
            rows[at + 1].starts_with("   ✓   done    #1501  closed 1"),
            "{rows:#?}"
        );
        assert!(
            rows[at + 5].starts_with("   ×   closed  #1505  closed 5"),
            "{rows:#?}"
        );
        state.on_key(Key::Enter);
        let rows = drawn(&state, 80, 30);
        assert!(!rows.iter().any(|r| r.contains("#1501")));
    }

    /// 01M6VFGK9XXWCA8X7CE8EZNSJV: `h` asks for a reason, then for `y`,
    /// and holds the item with the code of `riff plan hold`.
    #[test]
    fn h_asks_for_a_reason_and_a_yes_and_holds_the_item() {
        let mut state = mock_up();
        assert_eq!(state.on_key(Key::Char('h')), None);
        for c in "after #1622".chars() {
            state.on_key(Key::Char(c));
        }
        let rows = drawn(&state, 80, 24);
        assert_eq!(
            rows[22],
            " Hold #1620 of como-technologies/riff. The reason: after #1622█"
        );
        // The tab and the frame take no key while the person types.
        assert_eq!(state.tab(), Tab::Waves);
        state.on_key(Key::Enter);
        assert_eq!(
            drawn(&state, 80, 24)[22],
            " Hold #1620 of como-technologies/riff: after #1622? y/N"
        );
        let hold = Effect::Hold {
            repo: RIFF.into(),
            item: 1620,
            reason: "after #1622".into(),
        };
        assert_eq!(state.on_key(Key::Char('y')), Some(Action::Do(hold)));
        // An empty reason holds nothing.
        state.on_key(Key::Char('h'));
        state.on_key(Key::Char(' '));
        state.on_key(Key::Enter);
        assert_eq!(
            state.asked().as_deref(),
            Some("Hold #1620 of como-technologies/riff. The reason:  █")
        );
    }

    /// 01M6VFGK9XXWCA8X7CE8EZNSJV: `f` frees a held item after `y`. An
    /// item with no hold gets a line, and no question.
    #[test]
    fn f_frees_a_held_item_after_a_yes() {
        let mut state = mock_up();
        state.on_key(Key::Char('f'));
        assert_eq!(state.asked(), None);
        assert_eq!(
            state.shown_note(),
            Some("#1620 of como-technologies/riff has no hold.")
        );
        state.on_key(Key::Down);
        state.on_key(Key::Down);
        state.on_key(Key::Char('f'));
        let free = Effect::Free {
            repo: RIFF.into(),
            item: 1631,
        };
        assert_eq!(
            state.asked().as_deref(),
            Some("Free #1631 of como-technologies/riff? y/N")
        );
        assert_eq!(
            drawn(&state, 80, 24)[22],
            " Free #1631 of como-technologies/riff? y/N"
        );
        assert_eq!(state.on_key(Key::Char('y')), Some(Action::Do(free)));
    }

    /// 01M6XZ2TCBVN69GTJYV9X8GBW5: `a` on an item with the label
    /// `question` asks `y` or `n`, and `y` gives the effect of `riff
    /// questions --answered`. The row of the item has the mark `?` and the
    /// state `asked`. An item in work can have the label too. An item with
    /// no label gets a line, and no question.
    #[test]
    fn a_marks_the_questions_of_an_item_as_answered_after_a_yes() {
        let mut state = mock_up();
        state.on_key(Key::Char('a'));
        assert_eq!(state.asked(), None);
        assert_eq!(
            state.shown_note(),
            Some("#1620 of como-technologies/riff has no label question. Nothing changed.")
        );
        let mut forge = riff_forge();
        forge
            .issues
            .titles
            .insert(1640, "Which key marks it?".into());
        forge.issues.waves[0].1.push(1640);
        forge.issues.questions.extend([1640, 1590]);
        state.read_forge(vec![(RIFF.to_owned(), Ok(forge))], NOW);
        for _ in 0..3 {
            state.on_key(Key::Down);
        }
        let rows = drawn(&state, 80, 24);
        assert_eq!(
            rows[8],
            " > ?   asked   #1640  Which key marks it?             waits for a person   -"
        );
        state.on_key(Key::Char('a'));
        let answered = Effect::Answered {
            repo: RIFF.into(),
            item: 1640,
        };
        assert_eq!(
            drawn(&state, 80, 24)[22],
            " Mark the questions of #1640 of como-technologies/riff as answered? y/N"
        );
        assert_eq!(state.on_key(Key::Char('y')), Some(Action::Do(answered)));
        // `n` changes nothing.
        state.on_key(Key::Char('a'));
        assert_eq!(state.on_key(Key::Char('n')), None);
        assert_eq!(state.shown_note(), Some(crate::tui::NOTHING_CHANGED));
        // #1590 is in work, with the label.
        state.on_key(Key::Down);
        state.on_key(Key::Down);
        assert_eq!(state.waves().picked(), Some(&Pick::Item(RIFF.into(), 1590)));
        state.on_key(Key::Char('a'));
        assert_eq!(
            state.asked().as_deref(),
            Some("Mark the questions of #1590 of como-technologies/riff as answered? y/N")
        );
        state.on_key(Key::Esc);
        // The line of a repository has no item.
        state.on_key(Key::Char('g'));
        state.on_key(Key::Char('a'));
        assert_eq!(
            state.shown_note(),
            Some("Select an item with the label question to mark it as answered.")
        );
    }

    /// 01M6VFGK9XXWCA8X7CE8EZNSJV: `p` pauses the repository of the row, or
    /// resumes it when it is paused, and `P` the whole riff. `n` changes
    /// nothing.
    #[test]
    fn p_pauses_the_repository_and_capital_p_the_whole_riff() {
        let mut state = mock_up();
        state.on_key(Key::Char('p'));
        let pause = Effect::Pause {
            repo: Some(RIFF.into()),
            state: RiffState::Paused,
        };
        let yes = state.question.as_ref().map(|question| &question.yes);
        assert_eq!(yes, Some(&Action::Do(pause)));
        assert_eq!(
            drawn(&state, 80, 24)[22],
            " Pause como-technologies/riff? y/N"
        );
        assert_eq!(state.on_key(Key::Char('n')), None);
        assert_eq!(state.shown_note(), Some(crate::tui::NOTHING_CHANGED));
        state.on_key(Key::Char('P'));
        assert_eq!(drawn(&state, 80, 24)[22], " Pause the whole riff? y/N");
        state.on_key(Key::Esc);

        // A paused repository: its line says so, and `p` resumes it.
        let (look, at) = state.last_look().unwrap();
        let mut look = look.clone();
        look.pauses = serde_json::from_value(serde_json::json!({
            "state": "running",
            "repositories": [{"repository": RIFF, "pause": {"at_ms": 1}}],
        }))
        .unwrap();
        state.looked(look, at);
        let rows = drawn(&state, 80, 24);
        assert!(
            rows[2].starts_with(" ▼ como-technologies/riff · Wave 48 · paused"),
            "{}",
            rows[2]
        );
        state.on_key(Key::Char('p'));
        assert_eq!(
            drawn(&state, 80, 24)[22],
            " Resume como-technologies/riff? y/N"
        );
        state.on_key(Key::Esc);

        // The whole riff is paused, and the repository not: `p` pauses the
        // repository, and `P` resumes the riff.
        let (look, at) = state.last_look().unwrap();
        let mut look = look.clone();
        look.pauses = serde_json::from_value(serde_json::json!({
            "state": "paused",
            "riff": {"at_ms": 1},
        }))
        .unwrap();
        state.looked(look, at);
        assert!(drawn(&state, 80, 24)[2].contains(" · paused"));
        state.on_key(Key::Char('p'));
        assert_eq!(
            drawn(&state, 80, 24)[22],
            " Pause como-technologies/riff? y/N"
        );
        state.on_key(Key::Esc);
        state.on_key(Key::Char('P'));
        assert_eq!(drawn(&state, 80, 24)[22], " Resume the whole riff? y/N");
    }

    /// 01M6VFGK9XXWCA8X7CE8EZNSJV: `o` puts the URL of the pull request, of
    /// the item or of the repository in the note line, and `Enter` on an
    /// item its whole row.
    #[test]
    fn o_shows_the_url_and_enter_the_whole_row() {
        let mut state = mock_up();
        state.on_key(Key::Char('o'));
        assert_eq!(
            state.shown_note(),
            Some("https://github.com/como-technologies/riff/issues/1620")
        );
        state.on_key(Key::Up);
        state.on_key(Key::Char('o'));
        assert_eq!(
            state.shown_note(),
            Some("https://github.com/como-technologies/riff/pull/1604")
        );
        state.on_key(Key::Enter);
        assert_eq!(
            state.shown_note(),
            Some("#1601 Fix the help · stuck · PR #1604 conflict · 2h")
        );
        state.on_key(Key::Up);
        state.on_key(Key::Char('o'));
        assert_eq!(
            state.shown_note(),
            Some("https://github.com/como-technologies/riff")
        );
    }

    /// 01M6VAZKG8FS4HEA9VSA4Z4YF1, 01M6VFGKCJ02G33TXR01GY6M0G: a click on a
    /// row selects it, and a click on the selected row acts as `Enter`.
    /// A click on a key of the key line presses it.
    #[test]
    fn a_click_selects_a_row_and_a_second_click_opens_it() {
        let mut state = mock_up();
        let area = Rect::new(0, 0, 80, 24);
        let view = compute_view(&state, area, NOW);
        // The row of #1631 is the row 7 of the screen.
        assert_eq!(state.on_click(&view, 40, 7, false), None);
        assert_eq!(state.waves().picked(), Some(&Pick::Item(RIFF.into(), 1631)));
        let view = compute_view(&state, area, NOW);
        state.on_click(&view, 40, 7, false);
        assert_eq!(
            state.shown_note(),
            Some("#1631 Design: the web UI · held · waits for mike · 3d")
        );
        // A click on the line of strata, then on it again, opens its board.
        let view = compute_view(&state, area, NOW);
        state.on_click(&view, 10, 15, false);
        let view = compute_view(&state, area, NOW);
        state.on_click(&view, 10, 15, false);
        assert!(drawn(&state, 80, 24)[15].starts_with(" ▼ como-technologies/strata"));
        // The key line: `h hold`.
        let keys = " ↑↓ select  Enter open  h hold  f free  p pause  o URL  r read again  ? help";
        let x = u16::try_from(keys.chars().position(|c| c == 'h').unwrap()).unwrap();
        let view = compute_view(&state, area, NOW);
        assert_eq!(view.hit(x, 23), Some(Key::Char('h')));
        assert_eq!(view.hit(1, 23), None);
        // No click in the empty rows.
        assert_eq!(view.hit(10, 21), None);
    }

    /// The tab gets news for an item that turned stuck while the person
    /// looks at another tab (01M6VAZK1R932VQMJBC32MTTJB).
    #[test]
    fn an_item_that_turns_stuck_gives_the_tab_news() {
        let mut state = mock_up();
        state.on_key(Key::Open(Tab::Sessions));
        state.read_forge(vec![(RIFF.to_owned(), Ok(riff_forge()))], NOW);
        assert_eq!(state.news_of(Tab::Waves), None, "the same stuck item");
        let mut forge = riff_forge();
        forge.issues = forge.issues.with_stuck([(1599, "checks".to_owned())]);
        state.read_forge(vec![(RIFF.to_owned(), Ok(forge))], NOW);
        assert_eq!(state.news_of(Tab::Waves), Some(News::Dot));
        assert!(drawn(&state, 80, 24)[0].contains("3 Waves•"));
    }

    /// 01M6VMACJGF4A0ZXC6YK13KNCH, 01M6VFGK4P2FWMFQHBZKR3JXSZ: a read of
    /// `gh` that fails names the error in the note line once, keeps the
    /// board, and a question shows over the fault of the link.
    #[test]
    fn a_read_that_fails_names_its_error_and_keeps_the_board() {
        let mut state = mock_up();
        let failed = || vec![(RIFF.to_owned(), Err("gh: HTTP 502".to_owned()))];
        state.read_forge(failed(), NOW);
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[22], " riff: gh: HTTP 502");
        assert!(rows[4].contains("#1601"), "the board stays");
        state.failed("riff: no good look since 21:35:07: cannot reach riff-server");
        state.on_key(Key::Char('P'));
        assert_eq!(drawn(&state, 80, 24)[22], " Pause the whole riff? y/N");
        state.on_key(Key::Esc);
        assert!(drawn(&state, 80, 24)[22].contains("cannot reach riff-server"));
    }

    /// 01M6VFGJZF7XGNGVWTECKAH429: a broken `main` names the jobs that
    /// failed, and an open release issue gets a line.
    #[test]
    fn a_broken_main_and_an_open_release_get_their_lines() {
        let mut state = mock_up();
        let mut forge = riff_forge();
        forge.main = Some(Main::Broken {
            commit: "3f2a91c".into(),
            failed: vec!["Clippy".into()],
        });
        forge.releases = vec![(1686, "Release v4.11.0".into())];
        state.read_forge(vec![(RIFF.to_owned(), Ok(forge))], NOW);
        let rows = drawn(&state, 80, 26);
        assert_eq!(
            rows[14],
            "       main is broken: the full check of 3f2a91c failed: Clippy"
        );
        assert_eq!(
            rows[15],
            "       Release v4.11.0 (#1686): its checks after the deploy have not passed"
        );
        // No look at main: no line.
        let mut forge = riff_forge();
        forge.main = None;
        state.read_forge(vec![(RIFF.to_owned(), Ok(forge))], NOW);
        assert!(!drawn(&state, 80, 24).iter().any(|r| r.contains("main")));
    }

    /// A tab with no repository says so, and a repository with no read of
    /// the forge yet says that too.
    #[test]
    fn no_board_and_a_board_not_read_yet_say_so() {
        let mut state = State::new(build("10df8a4c0ffe"), "riff://mike@thelio".parse().unwrap());
        state.looked(Look::default(), NOW);
        state.on_key(Key::Open(Tab::Waves));
        let rows = drawn(&state, 80, 24);
        assert_eq!(
            rows[2..4],
            NO_BOARD.map(|line| format!(" {line}")),
            "{rows:#?}"
        );
        assert_eq!(rows[22], " The boards come from gh: the first read runs.");
        assert_eq!(state.on_key(Key::Char('h')), None);
        assert_eq!(state.on_key(Key::Down), None);
        // The clone of o/r gets a board, before its first read.
        let mut state = State::new(build("10df8a4c0ffe"), "riff://mike@thelio".parse().unwrap())
            .in_clone(Some("o/r".into()));
        state.looked(Look::default(), NOW);
        state.on_key(Key::Open(Tab::Waves));
        assert_eq!(state.boards(), ["o/r"]);
        assert_eq!(drawn(&state, 80, 24)[2], " ▶ o/r · not read yet");
        // Another tab reads no board.
        state.on_key(Key::Open(Tab::Sessions));
        assert!(state.boards().is_empty());
    }

    /// A short terminal shows the lines around the selection.
    #[test]
    fn a_short_terminal_keeps_the_selection_in_view() {
        let mut state = mock_up();
        state.on_key(Key::Char('G'));
        let rows = drawn(&state, 80, 12);
        assert!(rows[9].contains("como-technologies/kadomony"), "{rows:#?}");
        assert!(!rows.iter().any(|r| r.contains("#1601")), "{rows:#?}");
        state.on_key(Key::Char('g'));
        let rows = drawn(&state, 80, 12);
        assert!(rows[2].contains("como-technologies/riff"), "{rows:#?}");
    }

    /// The help popup of the Waves tab names its keys
    /// (01M6VAZKDEQ6951KGPGAMF1SNJ).
    #[test]
    fn the_help_names_the_keys_of_the_tab() {
        let mut state = mock_up();
        state.on_key(Key::Char('?'));
        let text = drawn(&state, 80, 24).join("\n");
        for (key, does) in help_keys() {
            assert!(text.contains(key) && text.contains(does), "{key}: {text}");
        }
    }

    /// A wide terminal gives the title more room.
    #[test]
    fn a_wide_terminal_gives_the_title_more_room() {
        let mut state = mock_up();
        let mut forge = riff_forge();
        let long = "A title that is longer than the column of 80 columns of the tab";
        forge.issues.titles.insert(1620, long.into());
        state.read_forge(vec![(RIFF.to_owned(), Ok(forge))], NOW);
        assert!(drawn(&state, 80, 24)[5].contains("A title that is longer than th…"));
        assert!(drawn(&state, 120, 24)[5].contains(long));
    }

    /// The facts of riff of the mock-up with three questions: #1622 of the
    /// current wave, #1610 of Wave 49 and #1801 of Wave 50.
    fn questions_forge() -> Forge {
        let mut forge = riff_forge();
        forge.issues.waves[1].1.push(1610);
        forge
            .issues
            .titles
            .insert(1610, "Design: the projects of the riff".into());
        forge.issues.questions = [1610, 1622, 1801].into();
        forge
    }

    /// 01M6Y0ZQWDYVC23WA5CVN0QSH8: an issue with the label `question` in a
    /// wave that is not current shows on the board with "waits for a
    /// person" and its wave, in number order with the questions of the
    /// current wave. An item of a later wave with no label does not show.
    #[test]
    fn a_question_of_a_wave_that_is_not_current_shows_with_waits_for_a_person() {
        let mut state = mock_up();
        state.read_forge(vec![(RIFF.to_owned(), Ok(questions_forge()))], NOW);
        let rows = drawn(&state, 80, 30);
        assert_eq!(
            rows[2],
            " ▼ como-technologies/riff · Wave 48 · 1 stuck 1 free 1 held 3 asked 3 work 2 ve…"
        );
        assert_eq!(
            rows[6..10],
            [
                "   ◌   held    #1631  Design: the web UI              waits for mike       3d",
                "   ?   asked   #1610  Wave 49 · Design: the projects… waits for a person   -",
                "   ?   asked   #1622  The book names the flag         waits for a person   -",
                "   ?   asked   #1801  Wave 50 · later                 waits for a person   -",
            ],
            "{rows:#?}"
        );
        // "See the questions of each wave in the TUI" of the book shows
        // these rows.
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/how-it-works.md"
        ))
        .unwrap();
        let book =
            crate::tui::text_block(&book, "### See the questions of each wave in the TUI", "");
        assert_eq!(book, rows[6..10]);
        assert!(!rows.iter().any(|r| r.contains("#1702")), "{rows:#?}");
        assert!(
            rows.iter()
                .any(|r| r == "   Wave 49: 9 items    Wave 50: 3 items    Backlog: 41 items")
        );
        // The narrow layout names the wave too.
        assert_eq!(
            drawn(&state, 40, 30)[6],
            "  ? asked   #1610  Wave 49 · Design: th…"
        );
        // The row is an item: the person selects it under #1620 and #1631,
        // and `Enter` names its wave.
        state.on_key(Key::Down);
        state.on_key(Key::Down);
        assert_eq!(state.waves().picked(), Some(&Pick::Item(RIFF.into(), 1610)));
        state.on_key(Key::Enter);
        assert_eq!(
            state.shown_note(),
            Some("#1610 Design: the projects of the riff · asked · Wave 49 · waits for a person")
        );
        // `a` marks its questions as answered after `y`
        // (01M6XZ2TCBVN69GTJYV9X8GBW5).
        state.on_key(Key::Char('a'));
        assert_eq!(
            state.asked().as_deref(),
            Some("Mark the questions of #1610 of como-technologies/riff as answered? y/N")
        );
        let answered = Effect::Answered {
            repo: RIFF.into(),
            item: 1610,
        };
        assert_eq!(state.on_key(Key::Char('y')), Some(Action::Do(answered)));
        // A folded board counts the questions of the later waves.
        state.on_key(Key::Char('g'));
        state.on_key(Key::Enter);
        assert!(
            drawn(&state, 80, 30)[2].contains("3 asked"),
            "{:#?}",
            drawn(&state, 80, 30)
        );
    }

    /// #1801: the daemon of the wave tells that the verify of #1599 and the
    /// rebase of #1601 wait for #1613. Each is a verify row that names the
    /// pull request before it, as "See the open items of a wave" of the book
    /// shows.
    #[test]
    fn a_verify_and_a_rebase_that_wait_for_their_turn_name_the_pull_request_before_them() {
        let mut state = mock_up();
        let (look, at) = state.last_look().unwrap();
        let mut look = look.clone();
        let facts = riff_client::daemons::Facts {
            waves: vec![riff_client::daemons::Wave {
                repo: RIFF.into(),
                word: "no free work now".into(),
                image: None,
                waits: BTreeMap::from([(1599, 1613), (1601, 1613)]),
            }],
            ..riff_client::daemons::Facts::default()
        };
        let mut daemon = session("d1d1d1d100", RIFF, "", "idle", serde_json::json!({}));
        daemon.machine = Some(riff_core::wire::MachineInfo {
            numbers: facts.line(),
            age_secs: 5,
        });
        look.who.sessions.push(daemon);
        state.looked(look, at);
        let rows = drawn(&state, 80, 24);
        assert!(
            rows[2].ends_with("2 free 1 held 3 work 3 verify"),
            "no stuck item: {rows:#?}"
        );
        let verify: Vec<String> = rows
            .iter()
            .filter(|r| r.contains("verify  #"))
            .cloned()
            .collect();
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/waves.md"
        ))
        .unwrap();
        let book = crate::tui::text_block(&book, "names the pull request before it", "");
        assert_eq!(verify, book);
        assert_eq!(
            verify,
            [
                "   ◐   verify  #1599  The wave takes an issue         PR #1614 after #1613 5m",
                "   ◐   verify  #1600  The book tells how to detach    PR #1613 merge       9m",
                "   ◐   verify  #1601  Fix the help                    PR #1604 after #1613 2h",
            ]
        );
    }

    /// 01M6Y0ZQWDYVC23WA5CVN0QSH8: a repository with no free, held or stuck
    /// item and a question of a later wave has a folded board that counts
    /// it.
    #[test]
    fn a_folded_board_counts_the_question_of_a_later_wave() {
        let mut state = mock_up();
        let strata = Forge {
            issues: Issues::of(vec![
                issue(31, "Ship it", "Wave 5"),
                Issue {
                    question: true,
                    ..issue(40, "Which name?", "Wave 7")
                },
            ]),
            ..Forge::default()
        };
        state.read_forge(
            vec![
                (RIFF.to_owned(), Ok(riff_forge())),
                ("como-technologies/strata".to_owned(), Ok(strata)),
            ],
            NOW,
        );
        let rows = drawn(&state, 80, 24);
        let strata = rows
            .iter()
            .find(|r| r.contains("como-technologies/strata"))
            .unwrap();
        assert!(strata.ends_with("1 asked 1 work"), "{strata}");
    }
}
