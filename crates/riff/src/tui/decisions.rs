//! The Decisions tab of the TUI (#2188): each open question and each
//! design issue, in a wave, in the Backlog or in neither, so that a
//! person works through each decision in one place.
//!
//! # Design
//!
//! The tab lists each open issue with the label `question`
//! ([`riff_client::waves::QUESTION`]) or the label [`DESIGN`] of each
//! project of the person, the projects of the boards of the Waves tab
//! ([`super::waves::Waves::repos`], 01M77F7R7RERVQH7XS39GWJJKF). A row
//! shows the number, the wave or `Backlog`, the title, the labels, and
//! each person of the riff that a heading `Questions for PERSON` of the
//! body names ([`names`], [`Decision::persons`],
//! 01M6QBJA0J7PQ0J29HWM1KDVHJ, 01M7G7SCW5AR1P73R1CMA2QWF7). The rows
//! whose questions name the person of the TUI come first ([`rows`],
//! 01M7FYVHB1APFXPXA71PMD18EH):
//!
//! ```text
//!  1 Sessions  2 Chat•2  3 Waves  4 Machines  5 Devices  6 Reports  [7 Decisions]
//!  riff running · build 10df8a4 · link ok 2s
//!  QUESTIONS FOR MIKE · 2
//!        ITEM   WAVE      TITLE                       LABELS            FOR
//!  > ?   #2173  Wave 60   The verify of a pull requ…  design question   mike
//!    ?   #1489  Backlog   Design: one place where a…  question          mike
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
//! - **The persons**: a name of a heading counts when it is the USER of
//!   a person of the riff, or of the person of the TUI, in any case. The
//!   row shows the USER, or `-` for none. So `Questions for the
//!   discussion` names nobody.
//! - **The order**: first the part of the person, then the other
//!   decisions. In each part, the rows go in the order of the waves,
//!   then the Backlog, then the issues in neither (`-`), and then by the
//!   repository and the number ([`Place`], [`IssueRef`]).
//! - **The mark**: `?` for an issue with the label `question`, and `·`
//!   for a design issue with no open question.
//! - **More than one project**: the item names its repository, as
//!   `riff#2173`, and its owner too when two projects have the same name
//!   ([`IssueRef::item`]).
//! - **The narrow layout** shows the mark, the item, the wave and the
//!   title.
//! - **The empty body** ([`rows`], 01M7GBGD8EH2B1DXZ8NADZ1A85): no line
//!   while the first read runs, [`NONE_OPEN`] when each project read
//!   well and has no decision, and the line [`text::decisions_unread`]
//!   for the projects that no read gave yet.
//!
//! # The reads
//!
//! While the tab shows, the TUI reads the open issues of each project
//! each [`super::forge::READ_EVERY`], at once when the tab opens, on `r`
//! and at the end of an action, and soon after a wake of the GitHub App
//! of the repository of the folder ([`super::forge::decisions`],
//! [`State::decisions_due`], 01M7FYVHB1APFXPXA71PMD18EH). It is the
//! conditional REST read of the Waves tab, with no other call: a read
//! that changed nothing costs no point. An issue whose labels `question`
//! and `design` go leaves the tab at the next read. A read that fails
//! keeps the last decisions of its repository, and the note line names
//! its fault once ([`super::forge::Reads`]). With no other note, the note
//! line shows the age of the last read ([`note`],
//! 01M7G7SHRDEA6BCNSYQA8RGZ23).
//!
//! # The keys
//!
//! | Key | Does |
//! |---|---|
//! | `↑` `↓`, `j` `k`, `PgUp` `PgDn`, `g` `G` | Select a decision ([`Key::moves`]). |
//! | `Enter`, a click on the selected row, a double click | Opens the issue in the browser, and puts its URL in the note line. |
//! | `o` | Puts the URL of the issue in the note line, as in the Waves tab. |
//! | `a` | Marks the questions of the issue as answered, after `y`: `riff questions --answered` ([`super::Effect::Answered`]). |
//! | `r` | Reads the decisions again now. |
//!
//! ```mermaid
//! flowchart LR
//!     G["gh: the open issues<br/>of each project"] -- "the label question or design" --> D["the decisions"]
//!     D -- "Questions for PERSON<br/>names the person" --> M["the part of the person, first"]
//!     D --> O["the other decisions"]
//!     M -- "Enter" --> B["the issue in the browser"]
//!     O -- "Enter" --> B
//! ```

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use riff_client::waves::{QUESTION, wave_number};
use riff_core::name::{ThreadName, allowed};

use super::forge::{BACKLOG, Entry, Reads};
use super::view::{DIM, Layout, WARNING, width, window};
use super::waves::{BOLD, MOVE_KEYS, SELECTED, age, cut, issue_url, padded, short_repo};
use super::{Action, Effect, Key, OnRows, State, Tab};
use crate::text;

/// The label of a design issue: a decision of a person that is not
/// asked yet, or a design in work.
pub const DESIGN: &str = "design";

/// The line of the body when the person has no open decision.
pub const NONE_OPEN: &str = "No question and no design issue is open.";

/// An issue of a project: the ID of a decision, and the row that the
/// person selected. It names the row, so it stays when a read moves the
/// rows.
///
/// ```
/// use riff::tui::decisions::IssueRef;
///
/// let issue = IssueRef { repo: "como-technologies/riff".parse()?, number: 2173 };
/// assert_eq!(issue.url(), "https://github.com/como-technologies/riff/issues/2173");
/// let projects = |names: &[&str]| names.iter().map(|n| n.parse().unwrap()).collect::<Vec<_>>();
/// assert_eq!(issue.item(&projects(&["como-technologies/riff"])), "#2173");
/// let two = projects(&["como-technologies/riff", "como-technologies/strata"]);
/// assert_eq!(issue.item(&two), "riff#2173");
/// let same_name = projects(&["como-technologies/riff", "n8behavior/riff"]);
/// assert_eq!(issue.item(&same_name), "como-technologies/riff#2173");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IssueRef {
    /// The repository, `OWNER/REPO`.
    pub repo: ThreadName,
    /// The number of the issue.
    pub number: u64,
}

impl IssueRef {
    /// The URL of the issue ([`issue_url`]).
    pub fn url(&self) -> String {
        issue_url(&self.repo.to_string(), self.number)
    }

    /// The text of the item of a row among the projects `repos`
    /// (01M7FYVHB1APFXPXA71PMD18EH): `#N` with one project. With more, the
    /// name of its repository first, as `riff#2173`, and the owner too
    /// when another project has the same name.
    pub fn item(&self, repos: &[ThreadName]) -> String {
        let repo = self.repo.to_string();
        let name = short_repo(&repo);
        let same = repos
            .iter()
            .filter(|other| short_repo(&other.to_string()) == name)
            .count();
        match (repos.len() > 1, same > 1) {
            (false, _) => format!("#{}", self.number),
            (true, false) => format!("{}#{}", text::safe(name), self.number),
            (true, true) => format!("{}#{}", text::safe(&repo), self.number),
        }
    }
}

/// The place of an issue in the order of the rows: the order of the
/// variants is the order of the rows.
///
/// ```
/// use riff::tui::decisions::Place;
///
/// let mut places = [None, Some("Backlog"), Some("Later"), Some("Wave 60: Plans"), Some("Wave 9")]
///     .map(Place::of);
/// places.sort();
/// assert_eq!(places.map(|place| place.name()), ["Wave 9", "Wave 60", "Backlog", "-", "-"]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    /// A milestone `Wave N`, or `Wave N: NAME`: its N.
    Wave(u64),
    /// The milestone [`BACKLOG`].
    Backlog,
    /// In no wave and not in the Backlog: with no milestone, or in a
    /// milestone that is out of the waves (R223).
    Neither,
}

impl Place {
    /// The place of an issue in the milestone with the title `milestone`.
    pub fn of(milestone: Option<&str>) -> Place {
        match milestone.map(|title| (wave_number(title), title == BACKLOG)) {
            Some((Some(n), _)) => Place::Wave(n),
            Some((None, true)) => Place::Backlog,
            Some((None, false)) | None => Place::Neither,
        }
    }

    /// The text of the column `WAVE`: `Wave N`, `Backlog`, or `-` in
    /// neither.
    pub fn name(&self) -> String {
        match self {
            Place::Wave(n) => format!("Wave {n}"),
            Place::Backlog => BACKLOG.to_owned(),
            Place::Neither => "-".to_owned(),
        }
    }
}

/// An open issue with the label `question` or [`DESIGN`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub issue: IssueRef,
    pub title: String,
    /// Its wave, the Backlog, or neither.
    pub place: Place,
    /// Its labels, in the order of GitHub.
    pub labels: Vec<String>,
    /// The names of the headings `Questions for PERSON` of its body
    /// ([`names`]).
    pub named: Vec<String>,
}

impl Decision {
    /// The decision of the open entry `entry` of `repo`: `None` for a
    /// pull request, and for an issue with neither label.
    ///
    /// ```
    /// use riff::tui::decisions::{Decision, Place};
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
    /// let repo = "o/r".parse()?;
    /// let found: Vec<Decision> = entries.into_iter().filter_map(|e| Decision::of(&repo, e)).collect();
    /// assert_eq!(found.len(), 2);
    /// assert_eq!((found[0].issue.number, &found[0].place), (12, &Place::Backlog));
    /// assert_eq!(found[0].named, ["mike"]);
    /// assert!(found[0].asked());
    /// assert_eq!((found[1].issue.number, &found[1].place), (13, &Place::Wave(3)));
    /// assert!(!found[1].asked());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn of(repo: &ThreadName, entry: Entry) -> Option<Decision> {
        let Entry {
            number,
            title,
            labels,
            milestone,
            pull_request,
            body,
            state_reason: _,
            updated_at: _,
            created_at: _,
            comments: _,
            said: _,
            merge: _,
        } = entry;
        let labels: Vec<String> = labels.into_iter().map(|label| label.name).collect();
        let decides = labels.iter().any(|l| l == QUESTION || l == DESIGN);
        if pull_request.is_some() || !decides {
            return None;
        }
        Some(Decision {
            issue: IssueRef {
                repo: repo.clone(),
                number,
            },
            title,
            place: Place::of(milestone.as_ref().map(|m| m.title.as_str())),
            labels,
            named: names(body.as_deref().unwrap_or_default()),
        })
    }

    /// True when it has the label `question`: a person must answer.
    pub fn asked(&self) -> bool {
        self.labels.iter().any(|l| l == QUESTION)
    }

    /// The persons of the riff that its questions name: each USER of
    /// `people` that [`Decision::named`] names, in any case, as the riff
    /// writes it, in the order of the names (01M7G7SCW5AR1P73R1CMA2QWF7).
    ///
    /// ```
    /// use riff::tui::decisions::{Decision, IssueRef, Place, names};
    ///
    /// let named = |body: &str| Decision {
    ///     issue: IssueRef { repo: "o/r".parse().unwrap(), number: 1 },
    ///     title: "t".into(),
    ///     place: Place::Backlog,
    ///     labels: vec!["question".into()],
    ///     named: names(body),
    /// };
    /// let people = ["mike", "nate", "ada", "ada.lovelace"];
    /// assert_eq!(named("## Questions for Nate and Mike").persons(&people), ["nate", "mike"]);
    /// assert!(named("## Questions for the discussion").persons(&people).is_empty());
    /// assert!(named("## Questions for this design").persons(&people).is_empty());
    /// assert_eq!(named("## Questions for mike about the board").persons(&people), ["mike"]);
    /// assert_eq!(named("## Questions for Ada.Lovelace").persons(&people), ["ada.lovelace"]);
    /// ```
    pub fn persons(&self, people: &[&str]) -> Vec<String> {
        self.named
            .iter()
            .filter_map(|name| people.iter().find(|p| p.eq_ignore_ascii_case(name)))
            .map(|p| (*p).to_owned())
            .collect()
    }
}

/// The names of the headings `Questions for PERSON` of the body `body`,
/// in their order, each one once (01M6QBJA0J7PQ0J29HWM1KDVHJ,
/// 01M7G7SCW5AR1P73R1CMA2QWF7). A heading is a line that starts with `#`
/// or `**`. A name has the characters of a USER of riff
/// ([`riff_core::name::allowed`]), after an `@` or not. More names come
/// after `and` or a comma: `Questions for mike and nate`. The first other
/// word, a `.` at the end of a word, or a mark other than a comma, ends
/// the names. [`Decision::persons`] keeps only the names of the persons
/// of the riff.
///
/// ```
/// use riff::tui::decisions::names;
///
/// assert_eq!(names("## Facts\n\n## Questions for mike\n\nQ1. Which?"), ["mike"]);
/// assert_eq!(names("### Questions for Mike:\n## Questions for mike (answered)"), ["Mike"]);
/// assert_eq!(names("**Questions for @nate and ann**"), ["nate", "ann"]);
/// assert_eq!(names("## Questions for mike, nate, and ann"), ["mike", "nate", "ann"]);
/// assert_eq!(names("## Questions for mike,nate"), ["mike", "nate"]);
/// assert_eq!(names("## Questions for ada.lovelace and ann."), ["ada.lovelace", "ann"]);
/// assert_eq!(names("## Questions for mike about the board"), ["mike"]);
/// assert_eq!(names("## Questions for the discussion"), ["the"]);
/// assert_eq!(names("Questions for mike come later."), Vec::<String>::new());
/// assert_eq!(names("We have questions for mike."), Vec::<String>::new());
/// assert_eq!(names(""), Vec::<String>::new());
/// ```
pub fn names(body: &str) -> Vec<String> {
    const HEAD: &str = "questions for ";
    let mut found: Vec<String> = Vec::new();
    for line in body.lines() {
        let line = line.trim_start();
        if !(line.starts_with('#') || line.starts_with("**")) {
            continue;
        }
        let line = line.trim_start_matches(|c: char| c == '#' || c == '*' || c.is_whitespace());
        let Some(rest) = line
            .get(..HEAD.len())
            .filter(|head| head.eq_ignore_ascii_case(HEAD))
            .and_then(|_| line.get(HEAD.len()..))
        else {
            continue;
        };
        for name in list(rest) {
            if !found.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                found.push(name.to_owned());
            }
        }
    }
    found
}

/// The names at the start of `text`: `mike`, `@mike`, `mike and nate`,
/// `mike, nate and ann`, or `mike,nate` ([`names`]).
fn list(text: &str) -> Vec<&str> {
    let mut names = Vec::new();
    // True when a name comes next: at the start, and after `and` or a
    // comma.
    let mut next = true;
    // Each word ends at a comma too: `mike,nate` is `mike,` and `nate`.
    let words = text
        .split_whitespace()
        .flat_map(|word| word.split_inclusive(','));
    for word in words {
        let (word, comma) = word
            .strip_suffix(',')
            .map_or((word, false), |word| (word, true));
        if (word.is_empty() || word.eq_ignore_ascii_case("and")) && !names.is_empty() {
            next = true;
            continue;
        }
        if !next {
            break;
        }
        let word = word.trim_start_matches('@');
        let end = word.find(|c| !allowed(c)).unwrap_or(word.len());
        // A `.` at the end of a word ends a sentence.
        let name = word[..end].trim_end_matches('.');
        if name.is_empty() {
            break;
        }
        names.push(name);
        if name.len() < word.len() {
            break;
        }
        next = comma;
    }
    names
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
    Item(Item),
    /// An empty line.
    Blank,
    /// A line of text.
    Text(String),
}

impl Row {
    /// The pick of this row, when the person can select it: its issue.
    pub fn pick(&self) -> Option<IssueRef> {
        match self {
            Row::Item(item) => Some(item.decision.issue.clone()),
            _ => None,
        }
    }
}

/// A decision as its row shows it: the text of its item
/// ([`IssueRef::item`]), and the persons of the riff that its questions
/// name ([`Decision::persons`]). Only [`Item::new`] makes one, so the
/// texts agree with the decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    decision: Decision,
    item: String,
    persons: Vec<String>,
}

impl Item {
    /// The row of `decision` among the projects `repos`, with the USERs
    /// `people` of the riff.
    ///
    /// ```
    /// use riff::tui::decisions::{Decision, IssueRef, Item, Place};
    ///
    /// let repo: riff_core::name::ThreadName = "o/r".parse()?;
    /// let decision = Decision {
    ///     issue: IssueRef { repo: repo.clone(), number: 12 },
    ///     title: "Which name?".into(),
    ///     place: Place::Backlog,
    ///     labels: vec!["question".into()],
    ///     named: vec!["Mike".into(), "the".into()],
    /// };
    /// let item = Item::new(decision, &[repo], &["mike", "nate"]);
    /// assert_eq!((item.item(), item.persons()), ("#12", &["mike".to_owned()][..]));
    /// assert_eq!(item.decision().issue.number, 12);
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn new(decision: Decision, repos: &[ThreadName], people: &[&str]) -> Item {
        Item {
            item: decision.issue.item(repos),
            persons: decision.persons(people),
            decision,
        }
    }

    /// Its decision.
    pub fn decision(&self) -> &Decision {
        &self.decision
    }

    /// The text of its item, as `#2173` or `riff#2173`.
    pub fn item(&self) -> &str {
        &self.item
    }

    /// The persons of the riff that its questions name.
    pub fn persons(&self) -> &[String] {
        &self.persons
    }
}

/// What the Decisions tab knows, and the choices of the person.
#[derive(Debug, Clone, Default)]
pub struct Decisions {
    /// The last good read of the decisions of each repository.
    reads: Reads<ThreadName, Vec<Decision>>,
    /// True when the tab opened, or `r`, a wake of GitHub or the end of
    /// an action asked for a read, since the last start of a read.
    again: bool,
    /// The selected row.
    pick: Option<IssueRef>,
}

/// What a key of the Decisions tab gives the frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// A line for the note line.
    Note(String),
    /// A question of `y` or `n` for the note line.
    Ask(super::Question),
    /// Open the issue in the browser.
    Browse(IssueRef),
}

impl Decisions {
    /// Asks for a read at once: the next [`Decisions::starts`] is true.
    pub fn again(&mut self) {
        self.again = true;
    }

    /// True when a read starts at `now_ms`: none yet, the last one is
    /// [`super::forge::READ_EVERY`] old, or [`Decisions::again`] asked for
    /// one. It takes the ask, so an ask while the read runs asks for one
    /// more.
    ///
    /// ```
    /// use riff::tui::decisions::Decisions;
    ///
    /// let mut decisions = Decisions::default();
    /// assert!(decisions.starts(0));
    /// decisions.read(Vec::new(), 1_000);
    /// assert!(!decisions.starts(60_999));
    /// assert!(decisions.starts(61_000));
    /// decisions.again();
    /// assert!(decisions.starts(2_000));
    /// assert!(!decisions.starts(2_000));
    /// ```
    pub fn starts(&mut self, now_ms: i64) -> bool {
        std::mem::take(&mut self.again) || self.reads.due(now_ms)
    }

    /// Takes the read `read` of each repository at `at_ms`
    /// ([`Reads::take`]): a good read gives the decisions of its
    /// repository now, so an issue whose labels went leaves the tab. It
    /// gives the line of a new fault, for the note line.
    pub fn read(
        &mut self,
        read: Vec<(ThreadName, Result<Vec<Decision>, String>)>,
        at_ms: i64,
    ) -> Option<String> {
        self.reads.take(read, at_ms)
    }

    /// The index of the selected row in `rows`: the pick, else the first
    /// decision.
    pub fn selected(&self, rows: &[Row]) -> Option<usize> {
        let picks = |pick: &IssueRef| rows.iter().position(|r| r.pick().as_ref() == Some(pick));
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
    /// use riff::tui::decisions::{Answer, Decision, Decisions, IssueRef, Item, Place, Row};
    ///
    /// let issue = |number| IssueRef { repo: "o/r".parse().unwrap(), number };
    /// let row = |number| {
    ///     let decision = Decision {
    ///         issue: issue(number),
    ///         title: "Which name?".into(),
    ///         place: Place::Neither,
    ///         labels: vec!["design".into()],
    ///         named: vec![],
    ///     };
    ///     Row::Item(Item::new(decision, &[issue(number).repo], &[]))
    /// };
    /// let rows = [row(12), row(13)];
    /// let mut decisions = Decisions::default();
    /// assert_eq!(decisions.selected(&rows), Some(0));
    /// assert_eq!(decisions.on_key(Key::Down, &rows, 20), None);
    /// assert_eq!(decisions.selected(&rows), Some(1));
    /// assert_eq!(decisions.on_key(Key::Enter, &rows, 20), Some(Answer::Browse(issue(13))));
    /// assert_eq!(
    ///     decisions.on_key(Key::Char('o'), &rows, 20),
    ///     Some(Answer::Note("https://github.com/o/r/issues/13".into()))
    /// );
    /// // A click selects a row, and a click on the selected row opens it.
    /// assert_eq!(decisions.on_key(Key::Row(0), &rows, 20), None);
    /// assert_eq!(decisions.on_key(Key::Row(0), &rows, 20), Some(Answer::Browse(issue(12))));
    /// // A design with no label question has no question to answer.
    /// assert_eq!(
    ///     decisions.on_key(Key::Char('a'), &rows, 20),
    ///     Some(Answer::Note(riff::text::questions_not_asked("o/r", 12)))
    /// );
    /// ```
    pub fn on_key(&mut self, key: Key, rows: &[Row], page: usize) -> Option<Answer> {
        match key.on_rows(rows, self.selected(rows), page, |row| row.pick().is_some()) {
            OnRows::Select(row) => {
                self.pick = row.pick();
                None
            }
            OnRows::Act(key, row) => act(key, row),
            OnRows::Nothing => None,
        }
    }
}

/// Takes the key `key` on the selected row `row`.
fn act(key: Key, row: &Row) -> Option<Answer> {
    let Row::Item(item) = row else {
        return None;
    };
    let decision = item.decision();
    let issue = &decision.issue;
    match key {
        Key::Enter => Some(Answer::Browse(issue.clone())),
        Key::Char('o') => Some(Answer::Note(issue.url())),
        Key::Char('a') => {
            let IssueRef { repo, number } = issue;
            Some(
                match Effect::answered(&repo.to_string(), *number, decision.asked()) {
                    Ok(question) => Answer::Ask(question),
                    Err(note) => Answer::Note(note),
                },
            )
        }
        _ => None,
    }
}

/// The rows of the Decisions tab of `decisions` for the projects `repos`
/// of the person `user`, with the USERs `people` of the persons of the
/// riff (01M7FYVHB1APFXPXA71PMD18EH): the part of the decisions whose
/// questions name `user`, then the part of the others. In each part, the
/// rows go in the order of [`Place`], then of [`IssueRef`]. With more
/// than one project, an item names its repository. No row before the
/// first read, and a last line for the projects that no read gave yet
/// (01M7GBGD8EH2B1DXZ8NADZ1A85).
///
/// ```
/// use riff::tui::decisions::{Decision, Decisions, IssueRef, Place, Row, rows};
///
/// let repo: riff_core::name::ThreadName = "o/r".parse()?;
/// let decision = |number, milestone: Option<&str>, label: &str, person: &str| Decision {
///     issue: IssueRef { repo: repo.clone(), number },
///     title: "t".into(),
///     place: Place::of(milestone),
///     labels: vec![label.into()],
///     named: vec![person.into()],
/// };
/// let repos = [repo.clone()];
/// let people = ["mike", "nate"];
/// let mut decisions = Decisions::default();
/// assert!(rows(&decisions, &repos, "mike", &people).is_empty());
/// decisions.read(vec![(repo.clone(), Ok(vec![
///     decision(20, None, "question", "nate"),
///     decision(21, Some("Backlog"), "question", "Mike"),
///     decision(22, Some("Wave 60"), "design", "nate"),
///     decision(23, Some("Wave 60"), "question", "mike"),
///     decision(24, Some("Wave 59"), "question", "the"),
/// ]))], 0);
/// let shown: Vec<String> = rows(&decisions, &repos, "mike", &people)
///     .iter()
///     .map(|row| match row {
///         Row::Part { person: Some(p), count } => format!("for {p}: {count}"),
///         Row::Part { person: None, count } => format!("others: {count}"),
///         Row::Item(item) => format!("{} {}", item.item(), item.persons().join(" ")),
///         row => format!("{row:?}"),
///     })
///     .collect();
/// assert_eq!(
///     shown,
///     [
///         "for mike: 2", "Columns", "#23 mike", "#21 mike", "Blank",
///         "others: 3", "Columns", "#24 ", "#22 nate", "#20 nate",
///     ]
/// );
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn rows(decisions: &Decisions, repos: &[ThreadName], user: &str, people: &[&str]) -> Vec<Row> {
    if repos.is_empty() {
        return super::waves::NO_BOARD
            .map(|line| Row::Text(line.into()))
            .to_vec();
    }
    if decisions.reads.at().is_none() {
        return Vec::new();
    }
    let (read, unread): (Vec<&ThreadName>, Vec<&ThreadName>) = repos
        .iter()
        .partition(|repo| decisions.reads.get(*repo).is_some());
    let mut all: Vec<&Decision> = read
        .into_iter()
        .filter_map(|repo| decisions.reads.get(repo))
        .flatten()
        .collect();
    all.sort_by(|a, b| a.place.cmp(&b.place).then_with(|| a.issue.cmp(&b.issue)));
    let known: Vec<&str> = people.iter().copied().chain([user]).collect();
    let (mine, others): (Vec<Item>, Vec<Item>) = all
        .into_iter()
        .map(|decision| Item::new(decision.clone(), repos, &known))
        .partition(|item| item.persons.iter().any(|p| p.eq_ignore_ascii_case(user)));
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
        rows.extend(part.into_iter().map(Row::Item));
    }
    if unread.is_empty() && rows.is_empty() {
        rows.push(Row::Text(NONE_OPEN.to_owned()));
    }
    if !unread.is_empty() {
        if !rows.is_empty() {
            rows.push(Row::Blank);
        }
        let unread: Vec<String> = unread.iter().map(|repo| repo.to_string()).collect();
        rows.push(Row::Text(text::decisions_unread(&unread)));
    }
    rows
}

/// `text` cut to `columns - 2` and padded to `columns`: a column with at
/// least two spaces after it.
fn column(text: &str, columns: usize) -> String {
    padded(text, columns, 2)
}

/// The columns of the wave.
const WAVE: usize = 10;

/// The columns of the labels.
const LABELS: usize = 18;

/// The columns of the persons, the last column.
const FOR: usize = 10;

/// The columns of the mark and the selection before the item.
const LEAD: usize = 6;

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
/// lines that fit show, around the selected row ([`window`]).
pub fn body(
    decisions: &Decisions,
    all: &[Row],
    layout: Layout,
    columns: u16,
    rows: u16,
) -> Vec<(Line<'static>, usize)> {
    let wide = layout == Layout::Wide;
    let selected = decisions.selected(all);
    let items = all
        .iter()
        .filter_map(|row| match row {
            Row::Item(item) => Some(usize::from(width(item.item())) + 2),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        .max(7);
    let shape = Shape {
        wide,
        items,
        columns: usize::from(columns),
    };
    let lines: Vec<(Line<'static>, usize)> = all
        .iter()
        .enumerate()
        .filter(|(_, row)| wide || **row != Row::Columns)
        .map(|(i, row)| (line(row, selected == Some(i), &shape), i))
        .collect();
    window(lines, selected, rows)
}

/// The shape of the lines of a body: the layout, the columns of the
/// item, and the columns of a line.
struct Shape {
    wide: bool,
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
        Row::Item(item) => item_line(item, selected, shape),
        Row::Blank => Line::raw(""),
        Row::Text(text) => Line::raw(cut(text, columns)),
    };
    if selected {
        line.patch_style(SELECTED)
    } else {
        line
    }
}

/// The line of the decision of `item`.
fn item_line(item: &Item, selected: bool, shape: &Shape) -> Line<'static> {
    let Item {
        decision,
        item,
        persons,
    } = item;
    let sel = if selected { ">" } else { " " };
    let (mark, style): (&str, Style) = match decision.asked() {
        true => ("?", WARNING),
        false => ("·", DIM),
    };
    let title = text::safe(&decision.title);
    let wave = text::safe(&decision.place.name());
    if !shape.wide {
        // `> ? #2173  Wave 60  The verify of a pull…`
        let used = 4 + shape.items + WAVE;
        return Line::from(vec![
            Span::raw(format!("{sel} ")),
            Span::styled(mark, style),
            Span::raw(format!(
                " {}{}",
                column(item, shape.items),
                column(&wave, WAVE)
            )),
            Span::raw(cut(&title, shape.columns.saturating_sub(used))),
        ]);
    }
    let labels = text::safe(&decision.labels.join(" "));
    let persons = match persons.is_empty() {
        true => "-".to_owned(),
        false => text::safe(&persons.join(" ")),
    };
    Line::from(vec![
        Span::raw(format!("{sel} ")),
        Span::styled(mark, style),
        Span::raw("   "),
        Span::raw(column(item, shape.items)),
        Span::raw(column(&wave, WAVE)),
        Span::raw(column(&title, title_columns(shape.columns, shape.items))),
        Span::raw(column(&labels, LABELS)),
        Span::raw(cut(&persons, FOR)),
    ])
}

/// The note of the Decisions tab when the note line has no other
/// (01M7G7SHRDEA6BCNSYQA8RGZ23): the age of the last read, or that the
/// first read runs.
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
    match age(decisions.reads.at(), now_ms) {
        Some(age) => format!("The decisions come from gh, {age} ago."),
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
    MOVE_KEYS
        .into_iter()
        .chain([
            ("Enter", "open the issue in the browser"),
            ("o", "show the URL"),
            ("a", "mark as answered"),
            ("y n", "answer the question"),
        ])
        .collect()
}

impl State {
    /// The Decisions tab.
    pub fn decisions(&self) -> &Decisions {
        &self.decisions
    }

    /// The projects of the Decisions tab: the repositories of the boards
    /// of the Waves tab ([`super::waves::Waves::repos`]).
    pub fn decision_repos(&self) -> Vec<ThreadName> {
        let Some((look, _)) = self.last_look() else {
            return Vec::new();
        };
        // Each one parses: riff-server names each one as a thread, and
        // the folder is a clone of a repository.
        self.waves
            .repos(look)
            .iter()
            .filter_map(|repo| repo.parse().ok())
            .collect()
    }

    /// The rows of the Decisions tab now, with the persons of the riff of
    /// the last look.
    pub fn decision_rows(&self) -> Vec<Row> {
        let people: Vec<&str> = self
            .last_look()
            .map(|(look, _)| look.who.people.iter().map(|p| p.user.as_str()).collect())
            .unwrap_or_default();
        rows(
            &self.decisions,
            &self.decision_repos(),
            self.me.who().user(),
            &people,
        )
    }

    /// Takes the read `read` of the decisions at `at_ms`
    /// ([`Decisions::read`]): a new fault of `gh` goes to the note line.
    pub fn read_decisions(
        &mut self,
        read: Vec<(ThreadName, Result<Vec<Decision>, String>)>,
        at_ms: i64,
    ) {
        if let Some(fault) = self.decisions.read(read, at_ms) {
            self.note(fault);
        }
    }

    /// A wake of GitHub, or the end of an action: the Decisions tab reads
    /// again at once while it shows, or when it opens.
    pub fn decisions_again(&mut self) {
        self.decisions.again();
    }

    /// The projects of a read of the Decisions tab that starts at
    /// `now_ms` (01M7FYVHB1APFXPXA71PMD18EH): while the tab shows, the
    /// first read, a read each [`super::forge::READ_EVERY`], and a read at
    /// once after the tab opened, after `r`, and after
    /// [`State::decisions_again`] ([`Decisions::starts`]). `None` when no
    /// read starts.
    pub fn decisions_due(&mut self, now_ms: i64) -> Option<Vec<ThreadName>> {
        (self.tab == Tab::Decisions && self.decisions.starts(now_ms)).then(|| self.decision_repos())
    }

    /// Takes a key of the Decisions tab that the frame does not take.
    pub(super) fn on_decisions_key(&mut self, key: Key) -> Option<Action> {
        let rows = self.decision_rows();
        match self.decisions.on_key(key, &rows, self.page)? {
            Answer::Note(note) => self.note(note),
            Answer::Ask(question) => self.ask(question),
            Answer::Browse(issue) => {
                self.note(text::questions_web(&issue.url()));
                return Some(Action::Browse(issue));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
