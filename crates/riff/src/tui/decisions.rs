//! The Decisions tab of the TUI (#2188): each open question and each
//! design issue, in a wave, in the Backlog or in neither, so that a
//! person works through each decision in one place.
//!
//! # Design
//!
//! The tab lists each open issue with the label `question`
//! ([`riff_client::waves::QUESTION`]) or the label [`hygiene::DESIGN`] of each
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
//! - **The width** (01M7GFGRJMAYZVG68EP3B916T8): a row fits the line.
//!   The title gets the rest of the line, and at least 10 columns. In
//!   the wide layout, the labels give their columns first. An item wider
//!   than its column loses its start, as `…ies/riff#2173`.
//! - **The empty body** ([`rows`], 01M7GBGD8EH2B1DXZ8NADZ1A85,
//!   01M7GFH1D93M7F7S82E788EFHG): [`super::waves::NO_BOARD`] with no
//!   project, no line while the first read runs, [`NONE_OPEN`] when each
//!   project read well and has no decision, and the line
//!   [`text::decisions_unread`] for the projects that no read gave yet.
//!
//! # The reads
//!
//! The TUI reads the open issues of each project at its start, each
//! [`super::forge::READ_EVERY`] while the tab shows, and each
//! [`super::reads::AWAY_EVERY`] while another tab shows. While the tab
//! shows, or when it opens, it also reads at once on `r`, at the end of
//! an action, and soon after a wake of the GitHub App of the repository
//! of the folder. A tab that opens with a read 60 seconds old reads at
//! once ([`super::forge::decisions`], [`State::decisions_due`],
//! [`super::reads`], 01M7GFY7EBGFJJY27VZ6544B7W). It is the
//! conditional REST read of the Waves tab, with no other call: a read
//! that changed nothing costs no point. An issue whose labels `question`
//! and `design` go leaves the tab at the next read. A read that fails
//! keeps the last decisions of its repository, and the note line names
//! its fault once ([`super::forge::Reads`]). With no other note, the note
//! line shows the age of the last read, or that the first read runs
//! ([`note`], 01M7G7SHRDEA6BCNSYQA8RGZ23, 01M7GFH1D93M7F7S82E788EFHG).
//!
//! # The keys
//!
//! | Key | Does |
//! |---|---|
//! | `↑` `↓`, `j` `k`, `PgUp` `PgDn`, `g` `G` | Select a decision ([`Key::moves`]). |
//! | `Enter`, a click on the selected row, a double click | Opens the issue in the browser, and puts its URL in the note line. |
//! | `o` | Puts the URL of the issue in the note line, as in the Waves tab. |
//! | `a` | Marks the questions of the issue as answered, after `y`: `riff questions --answered` ([`super::Effect::Answered`]). With no label `question`, it asks nothing (01M7GFH204647G2B9NTX72JKFR). |
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

use hygiene::DESIGN;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use riff_client::waves::{QUESTION, wave_number};
use riff_core::name::{ThreadName, allowed};

use super::forge::{BACKLOG, Entry, Reads};
use super::reads::Read;
use super::view::{DIM, Layout, WARNING, width, window};
use super::waves::{
    BOLD, ISSUE_KEYS, MOVE_KEYS, SELECTED, age, cut, padded, repo_url, short_repo, short_wave,
};
use super::{Action, Effect, Key, OnRows, State};
use crate::text;

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
    /// The URL of the issue: the URL of a row of the Decisions tab, and
    /// of an item of the Waves tab with no pull request.
    pub fn url(&self) -> String {
        format!(
            "{}/issues/{}",
            repo_url(&self.repo.to_string()),
            self.number
        )
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

    /// The text of the column `WAVE`: `Wave N` ([`short_wave`]),
    /// `Backlog`, or `-` in neither.
    pub fn name(&self) -> String {
        match self {
            Place::Wave(n) => short_wave(*n),
            Place::Backlog => BACKLOG.to_owned(),
            Place::Neither => "-".to_owned(),
        }
    }
}

/// An open issue with the label `question` or [`DESIGN`]. Only
/// [`Decision::new`] makes one, so it has one of the two labels, and its
/// names come from its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    issue: IssueRef,
    title: String,
    /// Its wave, the Backlog, or neither.
    place: Place,
    /// Its labels, in the order of GitHub.
    labels: Vec<String>,
    /// The names of the headings `Questions for PERSON` of its body
    /// ([`names`]).
    named: Vec<String>,
}

impl Decision {
    /// The decision of the issue `issue` with the title `title`, in the
    /// milestone `milestone`, with the labels `labels` and the body
    /// `body`: `None` with neither the label `question` nor [`DESIGN`].
    ///
    /// ```
    /// use riff::tui::decisions::{Decision, IssueRef, Place};
    ///
    /// let issue = IssueRef { repo: "o/r".parse()?, number: 12 };
    /// let labels = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect();
    /// let body = "## Questions for mike\n\nQ1. Which?";
    /// let backlog = Some("Backlog");
    /// let decision = Decision::new(issue.clone(), "Which?".into(), backlog, labels(&["question"]), body);
    /// let decision = decision.unwrap();
    /// assert_eq!((decision.issue(), decision.place()), (&issue, Place::Backlog));
    /// assert_eq!(decision.named(), ["mike"]);
    /// assert!(Decision::new(issue, "Fix it".into(), None, labels(&["bug"]), "").is_none());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn new(
        issue: IssueRef,
        title: String,
        milestone: Option<&str>,
        labels: Vec<String>,
        body: &str,
    ) -> Option<Decision> {
        let decides = labels.iter().any(|l| l == QUESTION || l == DESIGN);
        decides.then(|| Decision {
            issue,
            title,
            place: Place::of(milestone),
            labels,
            named: names(body),
        })
    }

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
    /// assert_eq!((found[0].issue().number, found[0].place()), (12, Place::Backlog));
    /// assert_eq!(found[0].named(), ["mike"]);
    /// assert!(found[0].asked());
    /// assert_eq!((found[1].issue().number, found[1].place()), (13, Place::Wave(3)));
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
        if pull_request.is_some() {
            return None;
        }
        let issue = IssueRef {
            repo: repo.clone(),
            number,
        };
        Decision::new(
            issue,
            title,
            milestone.as_ref().map(|m| m.title.as_str()),
            labels.into_iter().map(|label| label.name).collect(),
            body.as_deref().unwrap_or_default(),
        )
    }

    /// Its issue.
    pub fn issue(&self) -> &IssueRef {
        &self.issue
    }

    /// Its wave, the Backlog, or neither.
    pub fn place(&self) -> Place {
        self.place
    }

    /// The names of the headings `Questions for PERSON` of its body
    /// ([`names`]).
    pub fn named(&self) -> &[String] {
        &self.named
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
    /// use riff::tui::decisions::{Decision, IssueRef};
    ///
    /// let named = |body: &str| {
    ///     let issue = IssueRef { repo: "o/r".parse().unwrap(), number: 1 };
    ///     Decision::new(issue, "t".into(), None, vec!["question".into()], body).unwrap()
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
/// or `**`, and then `Questions for`, with a capital `Q`. A name has the
/// characters of a USER of riff ([`riff_core::name::allowed`]), after an
/// `@` or not. More names come after `and` or a comma: `Questions for
/// mike and nate`. After a name, each other word, a `.` at the end of a
/// word, or a mark other than a comma, ends the names.
/// [`Decision::persons`] keeps only the names of the persons of the riff.
///
/// ```
/// use riff::tui::decisions::names;
///
/// let none = Vec::<String>::new();
/// assert_eq!(names("## Facts\n\n## Questions for mike\n\nQ1. Which?"), ["mike"]);
/// assert_eq!(names("### Questions for Mike:\n## Questions for mike (answered)"), ["Mike"]);
/// assert_eq!(names("**Questions for @nate and ann**"), ["nate", "ann"]);
/// assert_eq!(names("## Questions for mike, nate, and ann"), ["mike", "nate", "ann"]);
/// assert_eq!(names("## Questions for mike,nate"), ["mike", "nate"]);
/// assert_eq!(names("## Questions for mike , nate"), ["mike", "nate"]);
/// assert_eq!(names("## Questions for mike,,nate"), ["mike", "nate"]);
/// assert_eq!(names("## Questions for mike And nate"), ["mike", "nate"]);
/// assert_eq!(names("## Questions for and mike"), ["and"]);
/// assert_eq!(names("## Questions for ,mike"), none);
/// assert_eq!(names("## Questions for ada.lovelace and ann."), ["ada.lovelace", "ann"]);
/// assert_eq!(names("## Questions for mike. And nate, later"), ["mike"]);
/// assert_eq!(names("## Questions for mike about the board"), ["mike"]);
/// assert_eq!(names("## Questions for the discussion"), ["the"]);
/// assert_eq!(names("## Questions for the lead and mike"), ["the"]);
/// assert_eq!(names("## Answers from ann, nate"), none);
/// assert_eq!(names("## questions for mike\n  ## Questions for nate"), none);
/// assert_eq!(names("* Questions for mike: see below"), none);
/// assert_eq!(names("Questions for mike come later."), none);
/// assert_eq!(names("We have questions for mike."), none);
/// assert_eq!(names(""), none);
/// ```
pub fn names(body: &str) -> Vec<String> {
    const HEAD: &str = "Questions for ";
    let mut found: Vec<String> = Vec::new();
    for line in body.lines() {
        if !(line.starts_with('#') || line.starts_with("**")) {
            continue;
        }
        let heading = line.trim_start_matches(|c: char| c == '#' || c == '*' || c.is_whitespace());
        let Some(rest) = heading.strip_prefix(HEAD) else {
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
    /// use riff::tui::decisions::{Decision, IssueRef, Item};
    ///
    /// let repo: riff_core::name::ThreadName = "o/r".parse()?;
    /// let issue = IssueRef { repo: repo.clone(), number: 12 };
    /// let body = "## Questions for Mike and the lead";
    /// let decision = Decision::new(issue, "Which?".into(), None, vec!["question".into()], body);
    /// let item = Item::new(decision.unwrap(), &[repo], &["mike", "nate"]);
    /// assert_eq!((item.item(), item.persons()), ("#12", &["mike".to_owned()][..]));
    /// assert_eq!(item.decision().issue().number, 12);
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
    /// use riff::tui::decisions::{Answer, Decision, Decisions, IssueRef, Item, Row};
    ///
    /// let issue = |number| IssueRef { repo: "o/r".parse().unwrap(), number };
    /// let row = |number| {
    ///     let decision = Decision::new(issue(number), "Which?".into(), None, vec!["design".into()], "");
    ///     Row::Item(Item::new(decision.unwrap(), &[issue(number).repo], &[]))
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
    let issue = decision.issue();
    match key {
        Key::Enter => Some(Answer::Browse(issue.clone())),
        Key::Char('o') => Some(Answer::Note(issue.url())),
        // 01M7GFH204647G2B9NTX72JKFR: with no label `question`, a note.
        Key::Char('a') => Some(match Effect::answered(issue, decision.asked()) {
            Ok(question) => Answer::Ask(question),
            Err(note) => Answer::Note(note),
        }),
        _ => None,
    }
}

/// The rows of the Decisions tab of `decisions` for the projects `repos`
/// of the person `user`, with the USERs `people` of the persons of the
/// riff (01M7FYVHB1APFXPXA71PMD18EH): the part of the decisions whose
/// questions name `user`, then the part of the others. In each part, the
/// rows go in the order of [`Place`], then of [`IssueRef`]. With more
/// than one project, an item names its repository. The lines of
/// [`super::waves::NO_BOARD`] with no project
/// (01M7GFH1D93M7F7S82E788EFHG), no row before the first read, and a
/// last line for the projects that no read gave yet
/// (01M7GBGD8EH2B1DXZ8NADZ1A85).
///
/// ```
/// use riff::tui::decisions::{Decision, Decisions, IssueRef, Row, rows};
///
/// let repo: riff_core::name::ThreadName = "o/r".parse()?;
/// let decision = |number, milestone: Option<&str>, label: &str, person: &str| {
///     let (issue, body) = (IssueRef { repo: repo.clone(), number }, format!("# Questions for {person}"));
///     Decision::new(issue, "t".into(), milestone, vec![label.into()], &body).unwrap()
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
        rows.push(Row::Text(text::decisions_unread(&unread)));
    }
    rows
}

/// The least spaces after a column.
const GAP: usize = 2;

/// `text` cut to `columns - GAP` and padded to `columns`: a column with
/// at least [`GAP`] spaces after it.
fn column(text: &str, columns: usize) -> String {
    padded(text, columns, GAP)
}

/// The item `item` in a column of `columns`, as [`column`], but an item
/// that is too wide loses its start, so that its number shows
/// (01M7GFGRJMAYZVG68EP3B916T8): `…ies/riff#2173`.
fn item_column(item: &str, columns: usize) -> String {
    let reversed: String = item.chars().rev().collect();
    let end: String = cut(&reversed, columns.saturating_sub(GAP))
        .chars()
        .rev()
        .collect();
    column(&end, columns)
}

/// The least columns of the item, before its [`GAP`]: `#2173`.
const ITEM: usize = 5;

/// The columns of the wave.
const WAVE: usize = 10;

/// The columns of the labels, when the line has room for them.
const LABELS: usize = 18;

/// The columns of the persons, the last column.
const FOR: usize = 10;

/// The columns of the selection and the mark before the item: `> ?   `
/// in the wide layout.
const LEAD: usize = 6;

/// The columns of the selection and the mark before the item in the
/// narrow layout: `> ? `.
const NARROW_LEAD: usize = 4;

/// The least columns of the title (01M7GFGRJMAYZVG68EP3B916T8).
const TITLE: usize = 10;

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
            Row::Item(item) => Some(usize::from(width(item.item()))),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        .max(ITEM)
        + GAP;
    let shape = Shape::new(wide, items, usize::from(columns));
    let lines: Vec<(Line<'static>, usize)> = all
        .iter()
        .enumerate()
        .filter(|(_, row)| wide || **row != Row::Columns)
        .map(|(i, row)| (line(row, selected == Some(i), &shape), i))
        .collect();
    window(lines, selected, rows)
}

/// The shape of the lines of a body: the layout, the columns of a line,
/// and the columns of the item, the title and the labels in it.
struct Shape {
    wide: bool,
    columns: usize,
    item: usize,
    title: usize,
    labels: usize,
}

impl Shape {
    /// The shape of a line of `columns` in the wide layout (`wide`) or
    /// the narrow one, for items of `items` columns: the line fits
    /// (01M7GFGRJMAYZVG68EP3B916T8). The title gets the rest of the line,
    /// and at least [`TITLE`] columns. In the wide layout, the labels give
    /// their columns first. The item gets no more columns than the others
    /// leave.
    fn new(wide: bool, items: usize, columns: usize) -> Shape {
        let others = match wide {
            true => LEAD + WAVE + FOR,
            false => NARROW_LEAD + WAVE,
        };
        let item = items.min(columns.saturating_sub(others + TITLE));
        let rest = columns.saturating_sub(others + item);
        let labels = match wide {
            true => LABELS.min(rest.saturating_sub(TITLE)),
            false => 0,
        };
        Shape {
            wide,
            columns,
            item,
            title: rest - labels,
            labels,
        }
    }
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
                    "{}{}{}{}{}FOR",
                    " ".repeat(LEAD),
                    column("ITEM", shape.item),
                    column("WAVE", WAVE),
                    column("TITLE", shape.title),
                    column("LABELS", shape.labels),
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
    let title = text::safe(&decision.title);
    let wave = text::safe(&decision.place.name());
    if !shape.wide {
        // `> ? #2173  Wave 60  The verify of a pull…`
        let mut spans = lead(selected, decision.asked(), NARROW_LEAD);
        spans.extend([
            Span::raw(item_column(item, shape.item)),
            Span::raw(column(&wave, WAVE)),
            Span::raw(cut(&title, shape.title)),
        ]);
        return Line::from(spans);
    }
    let labels = text::safe(&decision.labels.join(" "));
    let persons = match persons.is_empty() {
        true => "-".to_owned(),
        false => text::safe(&persons.join(" ")),
    };
    let mut spans = lead(selected, decision.asked(), LEAD);
    spans.extend([
        Span::raw(item_column(item, shape.item)),
        Span::raw(column(&wave, WAVE)),
        Span::raw(column(&title, shape.title)),
        Span::raw(column(&labels, shape.labels)),
        Span::raw(cut(&persons, FOR)),
    ]);
    Line::from(spans)
}

/// The spans of the lead of an item, in `columns`: `>` when the row is
/// `selected`, the mark `?` of an issue that asks questions (`asked`) or
/// `·`, and spaces.
fn lead(selected: bool, asked: bool, columns: usize) -> Vec<Span<'static>> {
    let (mark, style): (&str, Style) = match asked {
        true => ("?", WARNING),
        false => ("·", DIM),
    };
    vec![
        Span::raw(if selected { "> " } else { "  " }),
        Span::styled(mark, style),
        Span::raw(" ".repeat(columns.saturating_sub(3))),
    ]
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
        .chain([("Enter", "open the issue in the browser")])
        .chain(ISSUE_KEYS)
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

    /// The projects of a read of the Decisions tab that starts at
    /// `now_ms`, as each read of `gh` of a tab ([`Read::Decisions`],
    /// [`State::starts`], 01M7GFY7EBGFJJY27VZ6544B7W). `None` when no
    /// read starts.
    pub fn decisions_due(&mut self, now_ms: i64) -> Option<Vec<ThreadName>> {
        self.starts(Read::Decisions, now_ms)
            .then(|| self.decision_repos())
    }

    /// Takes a key of the Decisions tab that the frame does not take.
    pub(super) fn on_decisions_key(&mut self, key: Key) -> Option<Action> {
        let rows = self.decision_rows();
        match self.decisions.on_key(key, &rows, self.page)? {
            Answer::Note(note) => self.note(note),
            Answer::Ask(question) => self.ask(question),
            // Only the URL, so that a narrow note line shows all of it
            // that fits.
            Answer::Browse(issue) => {
                self.note(issue.url());
                return Some(Action::Browse(issue));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
