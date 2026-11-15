//! The Decisions tab of the TUI (#2188): each open question and each
//! design issue, in a wave, in the Backlog or with no milestone, so that
//! a person works through each decision in one place.
//!
//! # Design
//!
//! The tab lists each open issue with the label `question`
//! ([`riff_client::waves::QUESTION`]) or the label [`DESIGN`] of each
//! project of the person, the projects of the boards of the Waves tab
//! ([`super::waves::Waves::repos`], 01M77F7R7RERVQH7XS39GWJJKF). A row
//! shows the number, the title, the wave or `Backlog`, the labels, and
//! each person that a heading `Questions for PERSON` of the body names
//! ([`persons`], 01M6QBJA0J7PQ0J29HWM1KDVHJ). The rows whose questions
//! name the person of the TUI come first ([`rows`],
//! 01M7FYVHB1APFXPXA71PMD18EH):
//!
//! ```text
//!  1 Sessions  2 Chat•2  3 Waves  4 Machines  5 Devices  6 Reports  [7 Decisions]
//!  riff running · build 10df8a4 · link ok 2s
//!  QUESTIONS FOR MIKE · 2
//!        ITEM   WAVE      TITLE                       LABELS            FOR
//!  > ?   #2173  Wave 60   The verify of a pull requ…  design question   mike
//!    ?   #1489  Backlog   Design: one place where a…  question          Mike
//!
//!  OTHER DECISIONS · 3
//!        ITEM   WAVE      TITLE                       LABELS            FOR
//!    ?   #2189  Wave 60   Design: a Plan tab in the…  design question   nate
//!    ·   #1739  Backlog   Design: take over or atta…  design            -
//!    ?   #1532  -         A wave item whose result …  question          -
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!  The decisions come from gh, 41s ago.
//!  ↑↓ select  Enter open  o URL  a answered  r read again  ? help  q quit
//! ```
//!
//! - **The order**: first the part of the person, then the other
//!   decisions. In each part, the rows go in the order of the waves,
//!   then the Backlog, then each other milestone, then the issues with no
//!   milestone, each in number order ([`Decision::place`]).
//! - **The mark**: `?` for an issue with the label `question`, and `·`
//!   for a design issue with no open question.
//! - **More than one project**: the item names its repository, as
//!   `riff#2173`.
//!
//! # The reads
//!
//! While the tab shows, the TUI reads the open issues of each project
//! each [`super::forge::READ_EVERY`], at once when the tab opens and on
//! `r`, and soon after a wake of GitHub ([`super::forge::decisions`],
//! 01M7FYVHB1APFXPXA71PMD18EH). It is the conditional REST read of the
//! Waves tab, with no other call: a read that changed nothing costs no
//! point. An issue whose labels `question` and `design` go leaves the
//! tab at the next read.
//!
//! # The keys
//!
//! | Key | Does |
//! |---|---|
//! | `Enter`, a click on the selected row | Opens the issue in the browser, and puts its URL in the note line. |
//! | `o` | Puts the URL of the issue in the note line, as in the Waves tab. |
//! | `a` | Marks the questions of the issue as answered, after `y`: `riff questions --answered` ([`super::Effect::Answered`]). |
//!
//! ```mermaid
//! flowchart LR
//!     G["gh: the open issues<br/>of each project"] -- "the label question or design" --> D["the decisions"]
//!     D -- "Questions for PERSON<br/>names the person" --> M["the part of the person, first"]
//!     D --> O["the other decisions"]
//!     M -- "Enter" --> B["the issue in the browser"]
//!     O -- "Enter" --> B
//! ```

use std::collections::BTreeMap;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use riff_client::waves::{QUESTION, wave_number};

use super::forge::{BACKLOG, Entry};
use super::view::Layout;
use super::waves::{cut, wave_name};
use super::{Action, Effect, Key, State};
use crate::text;

/// The label of a design issue: a decision of a person that is not
/// asked yet, or a design in work.
pub const DESIGN: &str = "design";

/// The style of the mark of a question.
const WARNING: Style = Style::new().fg(Color::Yellow);

/// The style of the mark of a design and of the names of the columns.
const DIM: Style = Style::new().add_modifier(Modifier::DIM);

/// The style of the title of a part.
const BOLD: Style = Style::new().add_modifier(Modifier::BOLD);

/// The style of the selected row.
const SELECTED: Style = Style::new().add_modifier(Modifier::REVERSED);

/// The line of the body when the person has no open decision.
pub const NONE_OPEN: &str = "No question and no design issue is open.";

/// An open issue with the label `question` or [`DESIGN`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// `OWNER/REPO`.
    pub repo: String,
    pub number: u64,
    pub title: String,
    /// The title of its milestone: a wave, the Backlog or another one.
    /// `None` with no milestone.
    pub milestone: Option<String>,
    /// Its labels, in the order of GitHub.
    pub labels: Vec<String>,
    /// The persons that the headings `Questions for PERSON` of its body
    /// name ([`persons`]).
    pub persons: Vec<String>,
}

impl Decision {
    /// The decision of the open entry `entry` of `repo`: `None` for a
    /// pull request, and for an issue with neither label.
    ///
    /// ```
    /// use riff::tui::decisions::Decision;
    /// use riff::tui::forge::Entry;
    ///
    /// let entries: Vec<Entry> = serde_json::from_str(r###"[
    ///   {"number": 12, "title": "Which name?", "labels": [{"name": "question"}],
    ///    "milestone": {"number": 1, "title": "Backlog"}, "body": "## Questions for mike\n\nQ1. Which?"},
    ///   {"number": 13, "title": "Design: the board", "labels": [{"name": "design"}],
    ///    "milestone": {"number": 5, "title": "Wave 3: Boards"}},
    ///   {"number": 14, "title": "Fix it", "labels": [{"name": "bug"}]},
    ///   {"number": 40, "title": "Which name?", "labels": [{"name": "question"}], "pull_request": {}}
    /// ]"###)?;
    /// let found: Vec<Decision> = entries.iter().filter_map(|e| Decision::of("o/r", e)).collect();
    /// assert_eq!(found.len(), 2);
    /// assert_eq!((found[0].number, found[0].wave().as_str()), (12, "Backlog"));
    /// assert_eq!(found[0].persons, ["mike"]);
    /// assert!(found[0].asked());
    /// assert_eq!((found[1].number, found[1].wave().as_str()), (13, "Wave 3"));
    /// assert!(!found[1].asked());
    /// assert_eq!(found[1].url(), "https://github.com/o/r/issues/13");
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn of(repo: &str, entry: &Entry) -> Option<Decision> {
        let labels: Vec<String> = entry.labels.iter().map(|l| l.name.clone()).collect();
        let decides = labels.iter().any(|l| l == QUESTION || l == DESIGN);
        if entry.pull_request.is_some() || !decides {
            return None;
        }
        Some(Decision {
            repo: repo.to_owned(),
            number: entry.number,
            title: entry.title.clone(),
            milestone: entry.milestone.as_ref().map(|m| m.title.clone()),
            labels,
            persons: persons(entry.body.as_deref().unwrap_or_default()),
        })
    }

    /// True when it has the label `question`: a person must answer.
    pub fn asked(&self) -> bool {
        self.labels.iter().any(|l| l == QUESTION)
    }

    /// The wave of the row: `Wave N`, `Backlog`, the title of another
    /// milestone, or `-` with none.
    pub fn wave(&self) -> String {
        self.milestone
            .as_deref()
            .map_or_else(|| "-".to_owned(), wave_name)
    }

    /// The URL of the issue.
    pub fn url(&self) -> String {
        format!(
            "https://github.com/{}/issues/{}",
            text::safe(&self.repo),
            self.number
        )
    }

    /// True when its questions name `user`, in any case.
    pub fn names(&self, user: &str) -> bool {
        self.persons.iter().any(|p| p.eq_ignore_ascii_case(user))
    }

    /// The place of its milestone in the order of the rows: each wave in
    /// the order of its number, then the Backlog, then each other
    /// milestone by its title, then no milestone.
    ///
    /// ```
    /// use riff::tui::decisions::Decision;
    ///
    /// let at = |milestone: Option<&str>| Decision {
    ///     repo: "o/r".into(),
    ///     number: 1,
    ///     title: "t".into(),
    ///     milestone: milestone.map(str::to_owned),
    ///     labels: vec!["question".into()],
    ///     persons: vec![],
    /// };
    /// let mut places = [at(None), at(Some("Backlog")), at(Some("Later")), at(Some("Wave 60")), at(Some("Wave 9"))]
    ///     .map(|d| (d.place(), d.wave()));
    /// places.sort();
    /// let waves: Vec<&str> = places.iter().map(|(_, wave)| wave.as_str()).collect();
    /// assert_eq!(waves, ["Wave 9", "Wave 60", "Backlog", "Later", "-"]);
    /// ```
    pub fn place(&self) -> (u8, u64, String) {
        match self.milestone.as_deref() {
            Some(title) => match wave_number(title) {
                Some(n) => (0, n, String::new()),
                None if title == BACKLOG => (1, 0, String::new()),
                None => (2, 0, title.to_owned()),
            },
            None => (3, 0, String::new()),
        }
    }
}

/// The persons that the headings `Questions for PERSON` of the body
/// `body` name, in their order, each one once (01M6QBJA0J7PQ0J29HWM1KDVHJ).
/// The heading may start with `#` and `*`, and may name more persons:
/// `Questions for mike and nate`. The words after the names do not
/// count.
///
/// ```
/// use riff::tui::decisions::persons;
///
/// assert_eq!(persons("## Facts\n\n## Questions for mike\n\nQ1. Which?"), ["mike"]);
/// assert_eq!(persons("### Questions for Mike:\n## Questions for mike (answered)"), ["Mike"]);
/// assert_eq!(persons("**Questions for @nate and ann**"), ["nate", "ann"]);
/// assert_eq!(persons("We have questions for mike."), Vec::<String>::new());
/// assert_eq!(persons(""), Vec::<String>::new());
/// ```
pub fn persons(body: &str) -> Vec<String> {
    const HEAD: &str = "questions for ";
    let mut found: Vec<String> = Vec::new();
    for line in body.lines() {
        let line = line.trim_start_matches(|c: char| c == '#' || c == '*' || c.is_whitespace());
        let Some(rest) = line
            .get(..HEAD.len())
            .filter(|head| head.eq_ignore_ascii_case(HEAD))
            .and_then(|_| line.get(HEAD.len()..))
        else {
            continue;
        };
        let names = rest
            .split(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '@' | ',' | ' ')))
            .next()
            .unwrap_or_default();
        let names = names
            .split([',', ' '])
            .map(|name| name.trim_start_matches('@'))
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("and"));
        for name in names {
            if !found.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                found.push(name.to_owned());
            }
        }
    }
    found
}

/// A row of the body of the Decisions tab, before the draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// The title of a part: the questions for `person`, or with `None`
    /// the other decisions, and the count of its rows.
    Part {
        person: Option<String>,
        count: usize,
    },
    /// The names of the columns, in the wide layout only.
    Columns,
    /// A decision.
    Item(Decision),
    /// An empty line.
    Blank,
    /// A line of text.
    Text(String),
}

impl Row {
    /// The pick of this row, when the person can select it: the
    /// repository and the number of its issue.
    pub fn pick(&self) -> Option<(String, u64)> {
        match self {
            Row::Item(decision) => Some((decision.repo.clone(), decision.number)),
            _ => None,
        }
    }
}

/// What the Decisions tab knows, and the choices of the person.
#[derive(Debug, Clone, Default)]
pub struct Decisions {
    /// The last good read of the decisions of each repository.
    read: BTreeMap<String, Vec<Decision>>,
    /// The time of the last read, in milliseconds since the epoch.
    read_ms: Option<i64>,
    /// The fault of the last read that failed, while it holds.
    fault: Option<String>,
    /// The selected row: its repository and its number.
    pick: Option<(String, u64)>,
}

/// What a key of the Decisions tab gives the frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// A line for the note line.
    Note(String),
    /// A question of `y` or `n` for the note line.
    Ask(super::Question),
    /// Open the issue at this URL in the browser.
    Browse(String),
}

impl Decisions {
    /// True when a read is due at `now_ms`: none yet, or the last one is
    /// [`super::forge::READ_EVERY`] old.
    ///
    /// ```
    /// use riff::tui::decisions::Decisions;
    ///
    /// let mut decisions = Decisions::default();
    /// assert!(decisions.due(0));
    /// decisions.read(Vec::new(), 1_000);
    /// assert!(!decisions.due(60_999));
    /// assert!(decisions.due(61_000));
    /// ```
    pub fn due(&self, now_ms: i64) -> bool {
        let every = i64::try_from(super::forge::READ_EVERY.as_millis()).unwrap_or(i64::MAX);
        self.read_ms.is_none_or(|at| now_ms - at >= every)
    }

    /// Takes the read `read` of each repository at `at_ms`. A good read
    /// gives the decisions of its repository now, so an issue whose
    /// labels went leaves the tab. A read that failed keeps the last
    /// decisions of its repository. A repository that `read` does not
    /// name goes. It gives the line of the first fault when it is new,
    /// for the note line.
    ///
    /// ```
    /// use riff::tui::decisions::Decisions;
    ///
    /// let mut decisions = Decisions::default();
    /// assert_eq!(decisions.read(vec![("o/r".into(), Ok(vec![]))], 0), None);
    /// let failed = || vec![("o/r".to_owned(), Err("gh: HTTP 502".to_owned()))];
    /// assert_eq!(decisions.read(failed(), 60_000).as_deref(), Some("riff: gh: HTTP 502"));
    /// // The same fault again: the note line named it once.
    /// assert_eq!(decisions.read(failed(), 120_000), None);
    /// assert_eq!(decisions.read_at(), Some(120_000));
    /// ```
    pub fn read(
        &mut self,
        read: Vec<(String, Result<Vec<Decision>, String>)>,
        at_ms: i64,
    ) -> Option<String> {
        let mut last = std::mem::take(&mut self.read);
        let mut fault = None;
        for (repo, read) in read {
            match read {
                Ok(found) => {
                    self.read.insert(repo, found);
                }
                Err(e) => {
                    fault.get_or_insert(text::top_gh_fault(&e));
                    if let Some(found) = last.remove(&repo) {
                        self.read.insert(repo, found);
                    }
                }
            }
        }
        self.read_ms = Some(at_ms);
        let new = fault.clone().filter(|f| self.fault.as_ref() != Some(f));
        self.fault = fault;
        new
    }

    /// The time of the last read.
    pub fn read_at(&self) -> Option<i64> {
        self.read_ms
    }

    /// The index of the selected row in `rows`: the pick, else the first
    /// decision.
    pub fn selected(&self, rows: &[Row]) -> Option<usize> {
        let picks =
            |pick: &(String, u64)| rows.iter().position(|r| r.pick().as_ref() == Some(pick));
        self.pick
            .as_ref()
            .and_then(picks)
            .or_else(|| rows.iter().position(|r| r.pick().is_some()))
    }

    /// Takes the key `key` of the person on the rows `rows`, with `page`
    /// rows in a page. A click on the selected row acts as `Enter`.
    ///
    /// ```
    /// use riff::tui::Key;
    /// use riff::tui::decisions::{Answer, Decision, Decisions, Row};
    ///
    /// let decision = |number| Decision {
    ///     repo: "o/r".into(),
    ///     number,
    ///     title: "Which name?".into(),
    ///     milestone: None,
    ///     labels: vec!["design".into()],
    ///     persons: vec![],
    /// };
    /// let rows = [Row::Item(decision(12)), Row::Item(decision(13))];
    /// let mut decisions = Decisions::default();
    /// assert_eq!(decisions.selected(&rows), Some(0));
    /// assert_eq!(decisions.on_key(Key::Down, &rows, 20), None);
    /// assert_eq!(decisions.selected(&rows), Some(1));
    /// let url = "https://github.com/o/r/issues/13".to_owned();
    /// assert_eq!(decisions.on_key(Key::Enter, &rows, 20), Some(Answer::Browse(url.clone())));
    /// assert_eq!(decisions.on_key(Key::Char('o'), &rows, 20), Some(Answer::Note(url)));
    /// // A click selects a row, and a click on the selected row opens it.
    /// assert_eq!(decisions.on_key(Key::Row(0), &rows, 20), None);
    /// assert!(matches!(decisions.on_key(Key::Row(0), &rows, 20), Some(Answer::Browse(_))));
    /// // A design with no label question has no question to answer.
    /// assert_eq!(
    ///     decisions.on_key(Key::Char('a'), &rows, 20),
    ///     Some(Answer::Note(riff::text::questions_not_asked("o/r", 12)))
    /// );
    /// ```
    pub fn on_key(&mut self, key: Key, rows: &[Row], page: usize) -> Option<Answer> {
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
                    return act(Key::Enter, rows.get(row)?);
                }
                self.pick = Some(pick);
                None
            }
            key => act(key, rows.get(self.selected(rows)?)?),
        }
    }
}

/// Takes the key `key` on the selected row `row`.
fn act(key: Key, row: &Row) -> Option<Answer> {
    let Row::Item(decision) = row else {
        return None;
    };
    match key {
        Key::Enter => Some(Answer::Browse(decision.url())),
        Key::Char('o') => Some(Answer::Note(decision.url())),
        Key::Char('a') if decision.asked() => {
            let answered = Effect::Answered {
                repo: decision.repo.clone(),
                item: decision.number,
            };
            Some(Answer::Ask(answered.question()))
        }
        Key::Char('a') => Some(Answer::Note(text::questions_not_asked(
            &decision.repo,
            decision.number,
        ))),
        _ => None,
    }
}

/// The rows of the Decisions tab of `decisions` for the projects `repos`
/// of the person `user` (01M7FYVHB1APFXPXA71PMD18EH): the part of the
/// decisions whose questions name `user`, then the part of the others.
/// In each part, the rows go in the order of [`Decision::place`], then
/// of the repository and of the number. No row before the first read.
///
/// ```
/// use riff::tui::decisions::{Decision, Decisions, Row, rows};
///
/// let decision = |number, milestone: Option<&str>, label: &str, person: &str| Decision {
///     repo: "o/r".into(),
///     number,
///     title: "t".into(),
///     milestone: milestone.map(str::to_owned),
///     labels: vec![label.into()],
///     persons: vec![person.into()],
/// };
/// let repos = ["o/r".to_owned()];
/// let mut decisions = Decisions::default();
/// assert!(rows(&decisions, &repos, "mike").is_empty());
/// decisions.read(vec![("o/r".into(), Ok(vec![
///     decision(20, None, "question", "nate"),
///     decision(21, Some("Backlog"), "question", "Mike"),
///     decision(22, Some("Wave 60"), "design", "nate"),
///     decision(23, Some("Wave 60"), "question", "mike"),
/// ]))], 0);
/// let shown: Vec<String> = rows(&decisions, &repos, "mike")
///     .iter()
///     .map(|row| match row {
///         Row::Part { person: Some(p), count } => format!("for {p}: {count}"),
///         Row::Part { person: None, count } => format!("others: {count}"),
///         Row::Item(d) => format!("#{}", d.number),
///         row => format!("{row:?}"),
///     })
///     .collect();
/// assert_eq!(
///     shown,
///     ["for mike: 2", "Columns", "#23", "#21", "Blank", "others: 2", "Columns", "#22", "#20"]
/// );
/// ```
pub fn rows(decisions: &Decisions, repos: &[String], user: &str) -> Vec<Row> {
    if repos.is_empty() {
        return super::waves::NO_BOARD
            .map(|line| Row::Text(line.into()))
            .to_vec();
    }
    if decisions.read_ms.is_none() {
        return Vec::new();
    }
    let mut all: Vec<&Decision> = repos
        .iter()
        .filter_map(|repo| decisions.read.get(repo))
        .flatten()
        .collect();
    if all.is_empty() {
        return vec![Row::Text(NONE_OPEN.to_owned())];
    }
    all.sort_by_key(|d| (d.place(), d.repo.clone(), d.number));
    let (mine, others): (Vec<&Decision>, Vec<&Decision>) =
        all.into_iter().partition(|d| d.names(user));
    let mut rows = Vec::new();
    for (person, part) in [(Some(user.to_owned()), mine), (None, others)] {
        if part.is_empty() {
            continue;
        }
        if !rows.is_empty() {
            rows.push(Row::Blank);
        }
        rows.push(Row::Part {
            person,
            count: part.len(),
        });
        rows.push(Row::Columns);
        rows.extend(part.into_iter().cloned().map(Row::Item));
    }
    rows
}

/// The width of `text` in columns.
fn width(text: &str) -> usize {
    Line::raw(text).width()
}

/// `text` cut to `columns - 2` and padded to `columns`: a column with at
/// least two spaces after it.
fn column(text: &str, columns: usize) -> String {
    let text = cut(text, columns.saturating_sub(2));
    let pad = columns.saturating_sub(width(&text));
    format!("{text}{}", " ".repeat(pad))
}

/// The columns of the wave.
const WAVE: usize = 10;

/// The columns of the labels.
const LABELS: usize = 18;

/// The columns of the persons, the last column.
const FOR: usize = 10;

/// The columns of the mark and the selection before the item.
const LEAD: usize = 6;

/// The text of the item of `decision`: `#N`, or with `many`
/// repositories, the name of its repository first, as `riff#2173`.
fn item(decision: &Decision, many: bool) -> String {
    match many {
        true => format!(
            "{}#{}",
            text::safe(super::waves::short_repo(&decision.repo)),
            decision.number
        ),
        false => format!("#{}", decision.number),
    }
}

/// The columns of the title in a wide line of `columns`, with an item
/// column of `items`: the rest of the line after the other columns.
fn title_columns(columns: usize, items: usize) -> usize {
    columns
        .saturating_sub(LEAD + items + WAVE + LABELS + FOR)
        .max(10)
}

/// The lines of the body of the Decisions tab of `decisions` with the
/// rows `all` of [`rows`], in `columns` and `rows` of the layout
/// `layout`, and for each line the index of its row in `all`. Only the
/// lines that fit show, around the selected row.
pub fn body(
    decisions: &Decisions,
    all: &[Row],
    layout: Layout,
    columns: u16,
    rows: u16,
) -> Vec<(Line<'static>, usize)> {
    let wide = layout == Layout::Wide;
    let selected = decisions.selected(all);
    let items: Vec<&Decision> = all
        .iter()
        .filter_map(|row| match row {
            Row::Item(decision) => Some(decision),
            _ => None,
        })
        .collect();
    let many = items.windows(2).any(|pair| pair[0].repo != pair[1].repo);
    let item_columns = items
        .iter()
        .map(|d| width(&item(d, many)) + 2)
        .max()
        .unwrap_or(0)
        .max(7);
    let shape = Shape {
        wide,
        many,
        items: item_columns,
        columns: usize::from(columns),
    };
    let lines: Vec<(Line<'static>, usize)> = all
        .iter()
        .enumerate()
        .filter(|(_, row)| wide || **row != Row::Columns)
        .map(|(i, row)| (line(row, selected == Some(i), &shape), i))
        .collect();
    let height = usize::from(rows);
    let at = selected
        .and_then(|s| lines.iter().position(|(_, i)| *i == s))
        .unwrap_or(0);
    let start = (at + 1).saturating_sub(height);
    lines.into_iter().skip(start).take(height).collect()
}

/// The shape of the lines of a body: the layout, whether the items name
/// their repository, the columns of the item, and the columns of a line.
struct Shape {
    wide: bool,
    many: bool,
    items: usize,
    columns: usize,
}

/// The line of `row`, `selected` or not, in the shape `shape`.
fn line(row: &Row, selected: bool, shape: &Shape) -> Line<'static> {
    let columns = shape.columns;
    let line = match row {
        Row::Part { person, count } => {
            let title = match person {
                Some(person) => format!("QUESTIONS FOR {}", text::safe(person).to_uppercase()),
                None => "OTHER DECISIONS".to_owned(),
            };
            Line::styled(cut(&format!("{title} · {count}"), columns), BOLD)
        }
        Row::Columns => Line::styled(
            cut(
                &format!(
                    "      {}{}{}{}FOR",
                    column("ITEM", shape.items),
                    column("WAVE", WAVE),
                    column("TITLE", title_columns(columns, shape.items)),
                    column("LABELS", LABELS),
                ),
                columns,
            ),
            DIM,
        ),
        Row::Item(decision) => item_line(decision, selected, shape),
        Row::Blank => Line::raw(""),
        Row::Text(text) => Line::raw(cut(text, columns)),
    };
    if selected {
        line.patch_style(SELECTED)
    } else {
        line
    }
}

/// The line of a decision.
fn item_line(decision: &Decision, selected: bool, shape: &Shape) -> Line<'static> {
    let sel = if selected { ">" } else { " " };
    let (mark, style) = match decision.asked() {
        true => ("?", WARNING),
        false => ("·", DIM),
    };
    let number = item(decision, shape.many);
    let title = text::safe(&decision.title);
    let wave = text::safe(&decision.wave());
    if !shape.wide {
        // `> ? #2173  Wave 60  The verify of a pull…`
        let used = 4 + shape.items + WAVE;
        return Line::from(vec![
            Span::raw(format!("{sel} ")),
            Span::styled(mark, style),
            Span::raw(format!(
                " {}{}",
                column(&number, shape.items),
                column(&wave, WAVE)
            )),
            Span::raw(cut(&title, shape.columns.saturating_sub(used))),
        ]);
    }
    let labels = text::safe(&decision.labels.join(" "));
    let persons = match decision.persons.is_empty() {
        true => "-".to_owned(),
        false => text::safe(&decision.persons.join(" ")),
    };
    Line::from(vec![
        Span::raw(format!("{sel} ")),
        Span::styled(mark, style),
        Span::raw("   "),
        Span::raw(column(&number, shape.items)),
        Span::raw(column(&wave, WAVE)),
        Span::raw(column(&title, title_columns(shape.columns, shape.items))),
        Span::raw(column(&labels, LABELS)),
        Span::raw(cut(&persons, FOR)),
    ])
}

/// The note of the Decisions tab when the note line has no other: the
/// age of the last read, or that the first read runs.
///
/// ```
/// use riff::tui::decisions::{Decisions, note};
///
/// let mut decisions = Decisions::default();
/// assert_eq!(note(&decisions, 0), "The decisions come from gh: the first read runs.");
/// decisions.read(Vec::new(), 1_000);
/// assert_eq!(note(&decisions, 42_000), "The decisions come from gh, 41s ago.");
/// ```
pub fn note(decisions: &Decisions, now_ms: i64) -> String {
    match decisions.read_ms {
        Some(at) => format!(
            "The decisions come from gh, {} ago.",
            text::ago(u64::try_from((now_ms - at) / 1000).unwrap_or(0))
        ),
        None => "The decisions come from gh: the first read runs.".to_owned(),
    }
}

/// The keys of the key line of the Decisions tab in `layout`: the text
/// of the key, its word, and the key that a click on it presses.
pub fn keys_of(layout: Layout) -> Vec<(&'static str, &'static str, Option<Key>)> {
    match layout {
        Layout::Wide => vec![
            ("↑↓", "select", None),
            ("Enter", "open", Some(Key::Enter)),
            ("o", "URL", Some(Key::Char('o'))),
            ("a", "answered", Some(Key::Char('a'))),
            ("r", "read again", Some(Key::Again)),
            ("?", "help", Some(Key::Help)),
            ("q", "quit", Some(Key::Quit)),
        ],
        _ => vec![
            ("↑↓", "select", None),
            ("Enter", "open", Some(Key::Enter)),
            ("?", "help", Some(Key::Help)),
            ("q", "quit", Some(Key::Quit)),
        ],
    }
}

/// The keys of the Decisions tab in the help popup, before the keys of
/// the frame.
pub fn help_keys() -> Vec<(&'static str, &'static str)> {
    vec![
        ("↑ ↓, j k", "select a row"),
        ("PgUp PgDn", "move a page"),
        ("g G", "the first or last row"),
        ("Enter", "open the issue in the browser"),
        ("o", "show the URL"),
        ("a", "mark as answered"),
        ("y n", "answer the question"),
    ]
}

impl State {
    /// The Decisions tab.
    pub fn decisions(&self) -> &Decisions {
        &self.decisions
    }

    /// The projects of the Decisions tab while it shows: the
    /// repositories of the boards of the Waves tab
    /// ([`super::waves::Waves::repos`]).
    pub fn decision_repos(&self) -> Vec<String> {
        match (self.tab, self.last_look()) {
            (super::Tab::Decisions, Some((look, _))) => self.waves.repos(look),
            _ => Vec::new(),
        }
    }

    /// The rows of the Decisions tab now.
    pub fn decision_rows(&self) -> Vec<Row> {
        let repos = match self.last_look() {
            Some((look, _)) => self.waves.repos(look),
            None => Vec::new(),
        };
        rows(&self.decisions, &repos, self.me.who().user())
    }

    /// Takes the read `read` of the decisions at `at_ms`
    /// ([`Decisions::read`]): a new fault of `gh` goes to the note line.
    pub fn read_decisions(
        &mut self,
        read: Vec<(String, Result<Vec<Decision>, String>)>,
        at_ms: i64,
    ) {
        if let Some(fault) = self.decisions.read(read, at_ms) {
            self.note(fault);
        }
    }

    /// Takes a key of the Decisions tab that the frame does not take.
    pub(super) fn on_decisions_key(&mut self, key: Key) -> Option<Action> {
        let rows = self.decision_rows();
        match self.decisions.on_key(key, &rows, self.page)? {
            Answer::Note(note) => self.note(note),
            Answer::Ask(question) => self.ask(question),
            Answer::Browse(url) => {
                self.note(text::tui_opens(&url));
                return Some(Action::Browse(url));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use riff_core::build::Build;
    use riff_core::name::ThreadName;

    use super::*;
    use crate::tui::view::{compute_view, render};
    use crate::tui::{Look, News, Tab};

    /// The time of the screen tests, in milliseconds since the epoch.
    const NOW: i64 = 1_794_000_000_000;

    const RIFF: &str = "como-technologies/riff";

    fn build(id: &str) -> Build {
        format!("0.1.0 {id} 2026-11-06T12:00:00Z").parse().unwrap()
    }

    /// A decision of `repo` with the labels `labels`, in `milestone`,
    /// whose questions name `persons`.
    fn decision(
        repo: &str,
        number: u64,
        title: &str,
        milestone: Option<&str>,
        labels: &[&str],
        persons: &[&str],
    ) -> Decision {
        Decision {
            repo: repo.into(),
            number,
            title: title.into(),
            milestone: milestone.map(str::to_owned),
            labels: labels.iter().map(|l| (*l).to_owned()).collect(),
            persons: persons.iter().map(|p| (*p).to_owned()).collect(),
        }
    }

    /// The decisions of riff in the mock-up of the module: two for mike,
    /// one for nate, a design with no question, and a question with no
    /// milestone.
    fn riff_decisions() -> Vec<Decision> {
        let wave = Some("Wave 60: the lead does the host steps");
        vec![
            decision(
                RIFF,
                2189,
                "Design: a Plan tab in the TUI for the planning of the waves",
                wave,
                &["design", "question"],
                &["nate"],
            ),
            decision(
                RIFF,
                2173,
                "The verify of a pull request does not converge",
                wave,
                &["design", "question"],
                &["mike"],
            ),
            decision(
                RIFF,
                1489,
                "Design: one place where a person sees each decision",
                Some("Backlog"),
                &["question"],
                &["Mike"],
            ),
            decision(
                RIFF,
                1739,
                "Design: take over or attach a session of another host",
                Some("Backlog"),
                &["design"],
                &[],
            ),
            decision(
                RIFF,
                1532,
                "A wave item whose result is a report in its issue",
                None,
                &["question"],
                &[],
            ),
        ]
    }

    /// The state of the mock-up of the Decisions tab: mike, in a clone of
    /// riff, a project, 2 seconds after a good look and 41 seconds after
    /// the read of the decisions.
    fn mock_up() -> State {
        let me = "riff://mike@thelio".parse().unwrap();
        let mut state = State::new(build("10df8a4c0ffe"), me).in_clone(Some(RIFF.to_owned()));
        let mut look = Look {
            server: Some(build("10df8a4c0ffe")),
            ..Look::default()
        };
        let riff: ThreadName = RIFF.parse().unwrap();
        look.who.repos = vec![riff.clone()];
        look.who.projects = vec![riff];
        state.looked(look, NOW - 2_000);
        state.news(Tab::Chat, News::Lines(2));
        state.on_key(Key::Open(Tab::Decisions));
        state.read_decisions(vec![(RIFF.to_owned(), Ok(riff_decisions()))], NOW - 41_000);
        state.drew(20, NOW);
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

    /// 01M7FYVHB1APFXPXA71PMD18EH, 01M7FYVHDJJ288RRV8EBRKSY49: the screen
    /// test of the Decisions tab at 80 x 24 is the mock-up of the
    /// rustdoc of the module, and its body is the mock-up of the book.
    /// The questions for mike come first, also one that names `Mike`.
    /// Each row has the number, the wave or `Backlog`, the title, the
    /// labels and the persons.
    #[test]
    fn the_decisions_tab_at_80_x_24_is_the_mock_up() {
        let screen = drawn(&mock_up(), 80, 24);
        let doc = include_str!("decisions.rs");
        let want = crate::tui::text_block(doc, "# Design", "//! ");
        assert_eq!(screen, want);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/how-it-works.md"
        ))
        .unwrap();
        let book = crate::tui::text_block(&book, "### Work through the decisions in the TUI", "");
        assert_eq!(book, screen[2..12]);
    }

    /// 01M7FYVHDJJ288RRV8EBRKSY49: the screen test of the narrow layout
    /// of the Decisions tab at 40 x 24 (01M6VAZKK4KHCDCX4RKG9B95D7).
    #[test]
    fn the_decisions_tab_at_40_x_24_is_the_narrow_mock_up() {
        let mut want = vec![
            "‹ 7/7 Decisions ›",
            "riff running · link ok 2s",
            "QUESTIONS FOR MIKE · 2",
            "> ? #2173  Wave 60   The verify of a pu…",
            "  ? #1489  Backlog   Design: one place …",
            "",
            "OTHER DECISIONS · 3",
            "  ? #2189  Wave 60   Design: a Plan tab…",
            "  · #1739  Backlog   Design: take over …",
            "  ? #1532  -         A wave item whose …",
        ];
        want.resize(22, "");
        want.extend([
            "The decisions come from gh, 41s ago.",
            "↑↓ select  Enter open  ? help  q quit",
        ]);
        assert_eq!(drawn(&mock_up(), 40, 24), want);
    }

    /// 01M7FYVHB1APFXPXA71PMD18EH: a question in the Backlog and a design
    /// issue in a wave are both in the tab, read from the REST list of
    /// the open issues. After the label `question` of the question goes,
    /// the next read takes it out of the tab. The design stays.
    #[test]
    fn a_question_in_the_backlog_and_a_design_in_a_wave_show_and_a_question_with_no_label_goes() {
        let entries = |labels: &str| -> Vec<Entry> {
            serde_json::from_str(&format!(
                r###"[
                  {{"number": 12, "title": "Which name?", "labels": [{labels}],
                    "milestone": {{"number": 1, "title": "Backlog"}},
                    "body": "## Questions for mike\n\nQ1. Which name?"}},
                  {{"number": 13, "title": "Design: the board", "labels": [{{"name": "design"}}],
                    "milestone": {{"number": 5, "title": "Wave 3"}}}},
                  {{"number": 14, "title": "Fix it", "milestone": {{"number": 5, "title": "Wave 3"}}}}
                ]"###
            ))
            .unwrap()
        };
        let found = |labels: &str| -> Vec<Decision> {
            entries(labels)
                .iter()
                .filter_map(|e| Decision::of(RIFF, e))
                .collect()
        };
        let mut state = mock_up();
        let numbers = |state: &State| -> Vec<u64> {
            state
                .decision_rows()
                .iter()
                .filter_map(|row| row.pick().map(|(_, n)| n))
                .collect()
        };
        state.read_decisions(
            vec![(RIFF.to_owned(), Ok(found(r#"{"name": "question"}"#)))],
            NOW,
        );
        assert_eq!(numbers(&state), [12, 13]);
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[2], " QUESTIONS FOR MIKE · 1", "{rows:#?}");
        assert!(
            rows[4].starts_with(" > ?   #12    Backlog   Which name?"),
            "{rows:#?}"
        );
        assert!(rows[4].ends_with("question          mike"), "{rows:#?}");
        assert!(
            rows[8].starts_with("   ·   #13    Wave 3    Design: the board"),
            "{rows:#?}"
        );
        // The label goes: the question leaves the tab at the next read.
        state.read_decisions(vec![(RIFF.to_owned(), Ok(found("")))], NOW + 60_000);
        assert_eq!(numbers(&state), [13]);
        // With no decision open, the body says so.
        state.read_decisions(vec![(RIFF.to_owned(), Ok(Vec::new()))], NOW + 120_000);
        assert_eq!(state.decision_rows(), [Row::Text(NONE_OPEN.to_owned())]);
    }

    /// 01M7FYVHDJJ288RRV8EBRKSY49: Enter opens the issue of the selected
    /// row in the browser, and puts its URL in the note line. `o` puts
    /// only the URL there. `a` asks `y` or `n`, and `y` marks the
    /// questions as answered with the code of `riff questions
    /// --answered`.
    #[test]
    fn enter_opens_the_issue_o_shows_its_url_and_a_marks_it_as_answered() {
        let mut state = mock_up();
        let url = "https://github.com/como-technologies/riff/issues/2173";
        assert_eq!(
            state.on_key(Key::Enter),
            Some(Action::Browse(url.to_owned()))
        );
        assert_eq!(state.shown_note(), Some(text::tui_opens(url).as_str()));
        state.on_key(Key::Char('j'));
        assert_eq!(state.on_key(Key::Char('o')), None);
        assert_eq!(
            state.shown_note(),
            Some("https://github.com/como-technologies/riff/issues/1489")
        );
        assert_eq!(state.on_key(Key::Char('a')), None);
        assert_eq!(
            state.asked().as_deref(),
            Some("Mark the questions of #1489 of como-technologies/riff as answered? y/N")
        );
        let answered = Effect::Answered {
            repo: RIFF.to_owned(),
            item: 1489,
        };
        assert_eq!(state.on_key(Key::Char('y')), Some(Action::Do(answered)));
        // A click on a row selects it, and a double click opens it once.
        let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
        assert_eq!(state.on_click(&view, 10, 9, false), None);
        assert_eq!(
            state.on_click(&view, 10, 9, true),
            Some(Action::Browse(
                "https://github.com/como-technologies/riff/issues/2189".to_owned()
            ))
        );
    }

    /// With more than one project, an item names its repository, and the
    /// rows of each project are in one list.
    #[test]
    fn with_two_projects_an_item_names_its_repository() {
        let mut state = mock_up();
        let mut look = state.last_look().unwrap().0.clone();
        let strata: ThreadName = "como-technologies/strata".parse().unwrap();
        look.who.repos.push(strata.clone());
        look.who.projects.push(strata);
        state.looked(look, NOW);
        state.read_decisions(
            vec![
                (RIFF.to_owned(), Ok(riff_decisions())),
                (
                    "como-technologies/strata".to_owned(),
                    Ok(vec![decision(
                        "como-technologies/strata",
                        7,
                        "Which store?",
                        Some("Wave 2"),
                        &["question"],
                        &["mike"],
                    )]),
                ),
            ],
            NOW,
        );
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[2], " QUESTIONS FOR MIKE · 3", "{rows:#?}");
        assert!(
            rows[4].starts_with(" > ?   strata#7   Wave 2    Which store?"),
            "{rows:#?}"
        );
        assert!(
            rows[5].starts_with("   ?   riff#2173  Wave 60"),
            "{rows:#?}"
        );
    }
}
