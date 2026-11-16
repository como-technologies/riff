//! When the TUI reads `gh` for each tab (#2209).
//!
//! # Design
//!
//! Four tabs read `gh` on a timer ([`Read`]): the Sessions tab reads the
//! titles of the issues, the Chat tab the open issues of its list of `#`,
//! the Waves tab the forge of its boards, and the Decisions tab the open
//! issues of its projects. Each one also reads while another tab shows,
//! so a person who opens a tab sees its facts at once
//! (01M7GFY7EBGFJJY27VZ6544B7W):
//!
//! - At the start of the TUI, each read starts, whichever tab shows.
//! - The tab that shows reads again each [`Read::every`]: 60 seconds.
//! - A tab that does not show reads again each [`AWAY_EVERY`]: 5
//!   minutes. So in an hour on another tab, the forge gets at most 12
//!   reads, not 60.
//! - A wake of GitHub, `r` and the end of an action ask for a read at
//!   once ([`Timer::again`]). The ask waits until the tab shows: a tab
//!   that does not show never reads sooner than [`AWAY_EVERY`].
//! - When the person opens a tab whose last read is [`Read::every`] old
//!   or older, its read starts at once. Until it ends, the tab shows the
//!   facts that it has. The Waves tab and the Decisions tab show their
//!   age in the note line ([`super::waves::note`],
//!   [`super::decisions::note`]).
//!
//! Each read is the conditional read of its tab ([`super::forge`]): a
//! `304` costs no point, and a read in the background adds no GraphQL
//! call. The status of the riff daemon of this machine and the holds of
//! the boards cost no point of GitHub: the TUI reads them at their own
//! rate, whichever tab shows ([`super::DAEMON_EVERY`], [`super::look`]).
//!
//! The time of a read is the time of its start, in milliseconds since
//! the epoch. So a test steps a fake clock ([`Timer::starts`]).
//!
//! ```mermaid
//! flowchart TD
//!     T["a turn of the loop"] --> R{"a read of this kind runs?"}
//!     R -- yes --> W["no read starts"]
//!     R -- no --> N{"a read before?"}
//!     N -- no --> S["the read starts"]
//!     N -- yes --> V{"its tab shows?"}
//!     V -- yes --> A{"an ask, or 60 s<br/>since the last start?"}
//!     V -- no --> B{"5 minutes<br/>since the last start?"}
//!     A -- yes --> S
//!     B -- yes --> S
//!     A -- no --> W
//!     B -- no --> W
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use super::{State, Tab};

/// The time between two reads of `gh` of a tab that does not show.
pub const AWAY_EVERY: Duration = Duration::from_secs(300);

/// A read of `gh` of a tab, on a timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Read {
    /// The titles of the issues of the Sessions tab
    /// ([`super::forge::titles`]).
    Titles,
    /// The open issues of the list of `#` of the Chat tab
    /// ([`super::forge::open_issues`]).
    OpenIssues,
    /// The forge of the boards of the Waves tab ([`super::forge::read`]).
    Forge,
    /// The open issues of the projects of the Decisions tab
    /// ([`super::forge::decisions`]).
    Decisions,
}

impl Read {
    /// Each read.
    pub const ALL: [Read; 4] = [Read::Titles, Read::OpenIssues, Read::Forge, Read::Decisions];

    /// The tab of the read.
    ///
    /// ```
    /// use riff::tui::Tab;
    /// use riff::tui::reads::Read;
    ///
    /// assert_eq!(Read::Titles.tab(), Tab::Sessions);
    /// assert_eq!(Read::OpenIssues.tab(), Tab::Chat);
    /// assert_eq!(Read::Forge.tab(), Tab::Waves);
    /// assert_eq!(Read::Decisions.tab(), Tab::Decisions);
    /// ```
    pub fn tab(self) -> Tab {
        match self {
            Read::Titles => Tab::Sessions,
            Read::OpenIssues => Tab::Chat,
            Read::Forge => Tab::Waves,
            Read::Decisions => Tab::Decisions,
        }
    }

    /// The time between two reads while the tab of the read shows: 60
    /// seconds ([`crate::top::ISSUES_TTL`], [`super::forge::READ_EVERY`]).
    ///
    /// ```
    /// use std::time::Duration;
    /// use riff::tui::reads::Read;
    ///
    /// for read in Read::ALL {
    ///     assert_eq!(read.every(), Duration::from_secs(60));
    /// }
    /// ```
    pub fn every(self) -> Duration {
        match self {
            Read::Titles => crate::top::ISSUES_TTL,
            Read::OpenIssues | Read::Forge | Read::Decisions => super::forge::READ_EVERY,
        }
    }
}

/// When each read of `gh` of the TUI starts: the start of the last read
/// of each kind, and the asks for a read at once.
#[derive(Debug, Clone, Default)]
pub struct Timer {
    /// The start of the last read of each kind, in milliseconds since the
    /// epoch.
    started: BTreeMap<Read, i64>,
    /// The reads that a wake of GitHub, `r` or the end of an action asked
    /// for since their last start.
    asked: BTreeSet<Read>,
}

impl Timer {
    /// True when the read `read` is due at `now_ms` while the tab `shown`
    /// shows (01M7GFY7EBGFJJY27VZ6544B7W): it never ran, or its tab shows
    /// and an ask waits, or its last start is [`Read::every`] old while
    /// its tab shows, else [`AWAY_EVERY`] old.
    ///
    /// ```
    /// use riff::tui::Tab;
    /// use riff::tui::reads::{Read, Timer};
    ///
    /// let mut timer = Timer::default();
    /// // At the start, each read is due, whichever tab shows.
    /// assert!(timer.due(Read::Forge, Tab::Sessions, 0));
    /// timer.starts(Read::Forge, Tab::Sessions, 0);
    /// // The tab that shows reads each minute, another one each 5 minutes.
    /// assert!(!timer.due(Read::Forge, Tab::Waves, 59_999));
    /// assert!(timer.due(Read::Forge, Tab::Waves, 60_000));
    /// assert!(!timer.due(Read::Forge, Tab::Sessions, 299_999));
    /// assert!(timer.due(Read::Forge, Tab::Sessions, 300_000));
    /// // An ask waits until the tab shows.
    /// timer.again(Read::Forge);
    /// assert!(!timer.due(Read::Forge, Tab::Sessions, 1_000));
    /// assert!(timer.due(Read::Forge, Tab::Waves, 1_000));
    /// ```
    pub fn due(&self, read: Read, shown: Tab, now_ms: i64) -> bool {
        let Some(&at) = self.started.get(&read) else {
            return true;
        };
        let shows = read.tab() == shown;
        if shows && self.asked.contains(&read) {
            return true;
        }
        let every = if shows { read.every() } else { AWAY_EVERY };
        let every = i64::try_from(every.as_millis()).unwrap_or(i64::MAX);
        now_ms.saturating_sub(at) >= every
    }

    /// True when the read `read` starts at `now_ms` while the tab `shown`
    /// shows: it is due ([`Timer::due`]). Then it takes the start, and
    /// the ask of the read. The loop asks only while no read of that kind
    /// runs, so an ask while a read runs gives one more read after it.
    ///
    /// ```
    /// use riff::tui::Tab;
    /// use riff::tui::reads::{Read, Timer};
    ///
    /// let mut timer = Timer::default();
    /// assert!(timer.starts(Read::Titles, Tab::Sessions, 0));
    /// assert!(!timer.starts(Read::Titles, Tab::Sessions, 0));
    /// timer.again(Read::Titles);
    /// assert!(timer.starts(Read::Titles, Tab::Sessions, 1_000));
    /// assert!(!timer.starts(Read::Titles, Tab::Sessions, 1_000));
    /// ```
    pub fn starts(&mut self, read: Read, shown: Tab, now_ms: i64) -> bool {
        let due = self.due(read, shown, now_ms);
        if due {
            self.started.insert(read, now_ms);
            self.asked.remove(&read);
        }
        due
    }

    /// Asks for the read `read` at once, while its tab shows or when it
    /// opens: a wake of GitHub, `r`, or the end of an action.
    pub fn again(&mut self, read: Read) {
        self.asked.insert(read);
    }
}

impl State {
    /// True when the read `read` starts at `now_ms` with the tab that
    /// shows now ([`Timer::starts`], 01M7GFY7EBGFJJY27VZ6544B7W).
    ///
    /// ```
    /// use riff::tui::reads::Read;
    /// use riff::tui::{Key, State, Tab};
    /// use riff_core::build::Build;
    ///
    /// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
    /// assert!(state.starts(Read::Forge, 0));
    /// // `r` in the Sessions tab reads the forge at its own time.
    /// state.on_key(Key::Again);
    /// assert!(!state.starts(Read::Forge, 1_000));
    /// // `r` in the Waves tab reads it at once.
    /// state.on_key(Key::Open(Tab::Waves));
    /// assert!(!state.starts(Read::Forge, 1_000));
    /// state.on_key(Key::Again);
    /// assert!(state.starts(Read::Forge, 1_000));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn starts(&mut self, read: Read, now_ms: i64) -> bool {
        self.timer.starts(read, self.tab, now_ms)
    }

    /// Asks for the read `read` at once, while its tab shows or when it
    /// opens ([`Timer::again`]).
    pub fn read_again(&mut self, read: Read) {
        self.timer.again(read);
    }
}

#[cfg(test)]
mod tests;
