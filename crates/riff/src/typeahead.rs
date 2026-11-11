//! The type-ahead of `@` and `#` in `riff chat` and in the Chat tab of
//! the TUI (#1771, #1880, 01M6Y1EKGKNW2MH970PDF6PJK1,
//! 01M6Y1EKKCCZ5215V49Z5T3HKH, 01M74CX2V9YAF0KFPJDRM4DA83,
//! 01M74CX9H7WT1T4EN70AVD2TGZ, 01M74CX9KVGQG4Y7ATT6268NMY).
//!
//! # Design
//!
//! `@` gives a list of the names of the chat, and `#` a list of the open
//! issues. riff-server gives the names
//! ([`riff_client::api::Api::names`], 01M6XZ27XV8RAC8B72DCM937ST): each
//! person and each project, with its kind and whether its role is
//! online. A UI keeps the names of its last read, and reads them again
//! each [`READ_EVERY`]. `gh` gives the open issues ([`OpenIssues`]): the
//! conditional read of the Waves tab
//! ([`crate::tui::forge::open_issues`]), of each repository of a live
//! session and of the repository of the working folder. A UI reads them
//! again each [`crate::tui::forge::READ_EVERY`], and a `304` costs no
//! point. After each key, a UI asks [`Ahead::list`] for the list of the
//! line at the cursor, and shows it under the prompt of `riff chat`, or
//! over the prompt of the Chat tab:
//!
//! ```text
//! [riff] > is #12 done? @r
//!   riff  project  online
//!   rita  person   offline
//!   Tab or Enter picks riff. Esc closes the list.
//!
//! [riff] > is #186
//!   #1862  riff  Projects A: riff-server keeps the projects of the riff
//!   #1860  riff  Pick an issue with #: the type-ahead of the open issues
//!   Tab or Enter picks #1862. Esc closes the list.
//! ```
//!
//! - **The word** is an `@` or a `#` at the start of the line or after a
//!   space, and the letters after it up to the cursor. The cursor is at
//!   the end of the word: at the end of the line, or before a space.
//! - **The list of `@`** holds each name that starts with the letters,
//!   or whose part after the slash does, with no regard to case, in the
//!   order of riff-server.
//! - **The list of `#`** holds the issues that fit the letters or
//!   digits after the `#`, the newest first. Digits fit each issue whose
//!   number starts with them. Letters fit each issue with a word of its
//!   title that starts with them, with no regard to case. A row has the
//!   number, the repository after the slash, and the title, cut to the
//!   width ([`List::rows`]).
//! - **A list** shows at most [`SHOWN`] entries, and the number of the
//!   others. With no entry, or when the word is the one entry of the list
//!   already, no list shows.
//! - **Tab or Enter** picks the first entry of the list ([`List::pick`]):
//!   `@NAME`, or `#N` for an issue of the repository of the working
//!   folder and `OWNER/REPO#N` for an issue of another repository, takes
//!   the place of the word, and a space comes after it at the end of the
//!   line.
//! - **Esc** closes the list ([`Ahead::close`]). It stays closed until
//!   the cursor leaves the word ([`Ahead::edited`]).
//! - **The list never blocks the line.** A key types as before, and a
//!   line with a name or an issue that is not in the list posts as it
//!   is: riff-server resolves each name at the post
//!   (01M6XZ280AMTAMPD6R4212YC2T).
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> NoList
//!     NoList --> List: type @ or # and letters that fit
//!     List --> List: a letter that fits
//!     List --> NoList: Tab or Enter picks the first entry
//!     List --> Closed: Esc
//!     Closed --> NoList: the cursor leaves the word
//!     List --> NoList: a letter that fits nothing
//! ```
//!
//! ```
//! use riff::typeahead::{Ahead, OpenIssue, OpenIssues};
//! use riff_core::wire::{ChatName, NameKind};
//!
//! let name = |name: &str, kind, online| ChatName { name: name.into(), kind, online };
//! let names = [
//!     name("brett", NameKind::Person, true),
//!     name("riff", NameKind::Project, true),
//!     name("rita", NameKind::Person, false),
//! ];
//! let mut ahead = Ahead::default();
//! let none = OpenIssues::default();
//! let line = "is #12 done? @r";
//! let list = ahead.list(&names, &none, line, line.len()).unwrap();
//! assert_eq!(list.rows(80), ["riff  project  online", "rita  person   offline"]);
//! assert_eq!(list.note(), "Tab or Enter picks riff. Esc closes the list.");
//! assert_eq!(list.pick(line), ("is #12 done? @riff ".to_owned(), 19));
//! // Esc closes the list until the cursor leaves the word.
//! ahead.close(line, line.len());
//! assert!(ahead.list(&names, &none, line, line.len()).is_none());
//! ahead.edited("is #12 done? @ri", 16);
//! assert!(ahead.list(&names, &none, "is #12 done? @ri", 16).is_none());
//! ahead.edited("is #12 done? ", 13);
//! assert!(ahead.list(&names, &none, "is #12 done? @r", 15).is_some());
//!
//! // `#` lists the open issues, the newest first.
//! let issue = |number, title: &str, opened| OpenIssue {
//!     repo: "como-technologies/riff".into(),
//!     number,
//!     title: title.into(),
//!     opened: Some(opened),
//! };
//! let mut issues = OpenIssues::new(Some("como-technologies/riff".into()));
//! issues.read(vec![("como-technologies/riff".into(), Ok(vec![
//!     issue(1860, "Pick an issue with #", 100),
//!     issue(1862, "Projects A: riff-server keeps the projects", 200),
//! ]))]);
//! let line = "is #186";
//! let list = ahead.list(&names, &issues, line, line.len()).unwrap();
//! assert_eq!(list.rows(80), [
//!     "#1862  riff  Projects A: riff-server keeps the projects",
//!     "#1860  riff  Pick an issue with #",
//! ]);
//! assert_eq!(list.note(), "Tab or Enter picks #1862. Esc closes the list.");
//! assert_eq!(list.pick(line), ("is #1862 ".to_owned(), 9));
//! ```

use std::collections::BTreeMap;
use std::time::Duration;

use riff_core::wire::{ChatName, NameKind};

use crate::text::safe;

/// The most entries that a list shows.
pub const SHOWN: usize = 5;

/// The time between two reads of the names: the online state of a name
/// changes.
pub const READ_EVERY: Duration = Duration::from_secs(30);

/// The type-ahead of a prompt: the word of a list that the person closed
/// with `Esc`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ahead {
    /// The byte of the `@` or the `#` of the word whose list the person
    /// closed.
    closed: Option<usize>,
}

impl Ahead {
    /// The list for `line` with the cursor at the byte `pos`: of `names`,
    /// the names of the last read in the order of riff-server, for an
    /// `@` word, and of `issues` for a `#` word. `None` when the cursor
    /// is at the end of no `@` or `#` word, when nothing fits the word,
    /// when the word is the one entry that fits, and while the list of
    /// the word is closed.
    ///
    /// ```
    /// use riff::typeahead::{Ahead, OpenIssues};
    /// use riff_core::wire::{ChatName, NameKind};
    ///
    /// let project = |name: &str| ChatName { name: name.into(), kind: NameKind::Project, online: false };
    /// let names = ["acme/riff", "brett", "como-technologies/riff", "mike"].map(project);
    /// let shown = |line: &str, pos: usize| {
    ///     let list = Ahead::default().list(&names, &OpenIssues::default(), line, pos);
    ///     list.map(|l| l.entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>())
    /// };
    /// // The part after the slash fits too, with no regard to case.
    /// assert_eq!(shown("@RI", 3).unwrap(), ["acme/riff", "como-technologies/riff"]);
    /// assert_eq!(shown("hi @", 4).unwrap().len(), 4);
    /// // The cursor in the word, an @ in a word, no fit, the one name.
    /// assert_eq!(shown("@br hi", 2), None);
    /// assert_eq!(shown("@br hi", 3).unwrap(), ["brett"]);
    /// assert_eq!(shown("mail a@b", 8), None);
    /// assert_eq!(shown("@zed", 4), None);
    /// assert_eq!(shown("@Brett", 6), None);
    /// ```
    pub fn list(
        &self,
        names: &[ChatName],
        issues: &OpenIssues,
        line: &str,
        pos: usize,
    ) -> Option<List> {
        let (at, mark, letters) = word(line, pos)?;
        if self.closed == Some(at) {
            return None;
        }
        let (more, entries) = match mark {
            '@' => fit_names(names, letters)?,
            _ => fit_issues(issues, letters)?,
        };
        Some(List {
            at,
            end: pos,
            mark,
            entries,
            more,
        })
    }

    /// The person changed `line`, or moved the cursor to the byte `pos`:
    /// a closed list opens again when the cursor left its word.
    pub fn edited(&mut self, line: &str, pos: usize) {
        if word(line, pos).map(|(at, ..)| at) != self.closed {
            self.closed = None;
        }
    }

    /// `Esc`: closes the list of the word at the byte `pos` of `line`.
    pub fn close(&mut self, line: &str, pos: usize) {
        self.closed = word(line, pos).map(|(at, ..)| at);
    }
}

/// The `@` or `#` word that ends at the byte `pos` of `line`: the byte
/// of its mark, the mark, and the letters after the mark. `None` when
/// `pos` is in a word, or the word at the cursor does not start with `@`
/// or `#`.
fn word(line: &str, pos: usize) -> Option<(usize, char, &str)> {
    let (before, after) = (line.get(..pos)?, line.get(pos..)?);
    if after.starts_with(|c: char| !c.is_whitespace()) {
        return None;
    }
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let word = &before[start..];
    let mark = word.chars().next().filter(|c| matches!(c, '@' | '#'))?;
    Some((start, mark, &word[1..]))
}

/// The number of the names that fit `letters` and do not show, and the
/// entries of the names that show. `None` with no fit, or when
/// `letters` is the one name that fits.
fn fit_names(names: &[ChatName], letters: &str) -> Option<(usize, Vec<Entry>)> {
    let letters = letters.to_lowercase();
    let fits: Vec<&ChatName> = names
        .iter()
        .filter(|n| {
            let name = n.name.to_lowercase();
            let after = name.rsplit('/').next().unwrap_or(&name);
            name.starts_with(&letters) || after.starts_with(&letters)
        })
        .collect();
    match fits.as_slice() {
        [] => return None,
        [one] if one.name.to_lowercase() == letters => return None,
        _ => {}
    }
    let more = fits.len().saturating_sub(SHOWN);
    let entries = fits.into_iter().take(SHOWN).map(Entry::of_name).collect();
    Some((more, entries))
}

/// [`fit_names`] for the open issues `issues` and the letters or digits
/// `letters` after a `#`: digits fit the start of the number, letters
/// the start of a word of the title. `None` with no fit, when `letters`
/// has a character that is no letter and no digit, or when the digits
/// are the number of the one issue that fits.
fn fit_issues(issues: &OpenIssues, letters: &str) -> Option<(usize, Vec<Entry>)> {
    if !letters.chars().all(char::is_alphanumeric) {
        return None;
    }
    let digits = letters.chars().all(|c| c.is_ascii_digit());
    let lower = letters.to_lowercase();
    let fits: Vec<&OpenIssue> = issues
        .newest()
        .iter()
        .filter(|issue| match digits {
            true => issue.number.to_string().starts_with(letters),
            false => words(&issue.title).any(|w| w.to_lowercase().starts_with(&lower)),
        })
        .collect();
    match fits.as_slice() {
        [] => return None,
        [one] if digits && one.number.to_string() == letters => return None,
        _ => {}
    }
    let more = fits.len().saturating_sub(SHOWN);
    let entries = fits
        .into_iter()
        .take(SHOWN)
        .map(|issue| Entry::of_issue(issue, issues.here()))
        .collect();
    Some((more, entries))
}

/// The words of `title`: each run of letters and digits.
fn words(title: &str) -> impl Iterator<Item = &str> {
    title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
}

/// An open issue for the list of `#` ([`crate::tui::forge::open_issues`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenIssue {
    /// Its repository, `OWNER/REPO`.
    pub repo: String,
    pub number: u64,
    pub title: String,
    /// The time it opened, in seconds since the epoch.
    pub opened: Option<i64>,
}

/// The open issues for the list of `#`: the last read of each
/// repository, and the repository of the working folder
/// (01M74CX9KVGQG4Y7ATT6268NMY).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenIssues {
    /// The repository of the working folder, `OWNER/REPO`.
    here: Option<String>,
    /// The last good read of each repository.
    read: BTreeMap<String, Vec<OpenIssue>>,
    /// The issues of each read, the newest first.
    newest: Vec<OpenIssue>,
}

impl OpenIssues {
    /// No issues yet, in the working folder of the repository `here`,
    /// `OWNER/REPO`, if any: a pick of an issue of `here` puts `#N`.
    pub fn new(here: Option<String>) -> OpenIssues {
        OpenIssues {
            here,
            ..OpenIssues::default()
        }
    }

    /// The repository of the working folder.
    pub fn here(&self) -> Option<&str> {
        self.here.as_deref()
    }

    /// Takes the read `read` of the open issues of each repository
    /// ([`crate::tui::forge::open_issues_each`]). A read that failed
    /// keeps the issues of the last read of its repository. A repository
    /// that `read` does not name goes.
    ///
    /// ```
    /// use riff::typeahead::{OpenIssue, OpenIssues};
    ///
    /// let issue = |repo: &str, number, opened| OpenIssue {
    ///     repo: repo.into(),
    ///     number,
    ///     title: "t".into(),
    ///     opened: Some(opened),
    /// };
    /// let numbers = |issues: &OpenIssues| {
    ///     issues.newest().iter().map(|i| (i.repo.clone(), i.number)).collect::<Vec<_>>()
    /// };
    /// let mut issues = OpenIssues::new(None);
    /// issues.read(vec![
    ///     ("o/a".into(), Ok(vec![issue("o/a", 7, 100), issue("o/a", 9, 300)])),
    ///     ("o/b".into(), Ok(vec![issue("o/b", 2, 200)])),
    /// ]);
    /// // The newest first, of each repository.
    /// assert_eq!(numbers(&issues), [("o/a".into(), 9), ("o/b".into(), 2), ("o/a".into(), 7)]);
    /// // A read that fails keeps the last issues, and a repository that
    /// // the read does not name goes.
    /// issues.read(vec![("o/a".into(), Err("gh: HTTP 502".into()))]);
    /// assert_eq!(numbers(&issues), [("o/a".into(), 9), ("o/a".into(), 7)]);
    /// ```
    pub fn read(&mut self, read: Vec<(String, Result<Vec<OpenIssue>, String>)>) {
        let mut last = std::mem::take(&mut self.read);
        for (repo, issues) in read {
            let issues = match issues {
                Ok(issues) => issues,
                Err(_) => last.remove(&repo).unwrap_or_default(),
            };
            self.read.insert(repo, issues);
        }
        let mut newest: Vec<OpenIssue> = self.read.values().flatten().cloned().collect();
        newest.sort_by(|a, b| {
            (b.opened, b.number)
                .cmp(&(a.opened, a.number))
                .then_with(|| a.repo.cmp(&b.repo))
        });
        self.newest = newest;
    }

    /// Each open issue, the newest first: by the time it opened, then by
    /// its number.
    pub fn newest(&self) -> &[OpenIssue] {
        &self.newest
    }
}

/// An entry of a list: a name of the chat, or an open issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The text that a pick puts in the place of the word: `@NAME`,
    /// `#N` or `OWNER/REPO#N`.
    pub text: String,
    /// What the note says that a pick picks: the name, or the text of an
    /// issue.
    pub name: String,
    /// The columns of its row.
    pub columns: Vec<String>,
}

impl Entry {
    /// The entry of the name `n`: the name, its kind, and `online` or
    /// `offline`.
    fn of_name(n: &ChatName) -> Entry {
        let name = safe(&n.name);
        let kind = match n.kind {
            NameKind::Person => "person ",
            NameKind::Project => "project",
        };
        let online = if n.online { "online" } else { "offline" };
        Entry {
            text: format!("@{name}"),
            columns: vec![name.clone(), kind.to_owned(), online.to_owned()],
            name,
        }
    }

    /// The entry of `issue`, in the working folder of the repository
    /// `here`: `#N` for an issue of `here`, else `OWNER/REPO#N`. Its
    /// columns are `#N`, the repository after the slash, and the title.
    fn of_issue(issue: &OpenIssue, here: Option<&str>) -> Entry {
        let number = format!("#{}", issue.number);
        let repo = safe(&issue.repo);
        let text = match here == Some(issue.repo.as_str()) {
            true => number.clone(),
            false => format!("{repo}{number}"),
        };
        let after = repo.rsplit('/').next().unwrap_or(&repo).to_owned();
        Entry {
            name: text.clone(),
            text,
            columns: vec![number, after, safe(&issue.title)],
        }
    }
}

/// The list of the entries that fit an `@` or a `#` word
/// ([`Ahead::list`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    /// The byte of the mark of the word in the line.
    pub at: usize,
    /// The byte of the end of the word: the cursor.
    pub end: usize,
    /// The mark of the word: `@` or `#`.
    pub mark: char,
    /// The entries that show, at most [`SHOWN`]: the first one is the
    /// pick of `Tab` and `Enter`.
    pub entries: Vec<Entry>,
    /// The number of the other entries that fit the word.
    pub more: usize,
}

impl List {
    /// The entry that `Tab` and `Enter` pick.
    pub fn first(&self) -> &Entry {
        &self.entries[0]
    }

    /// The text that takes the place of the word in `line`: the text of
    /// the first entry, and a space at the end of the line.
    ///
    /// ```
    /// use riff::typeahead::{Ahead, OpenIssues};
    /// use riff_core::wire::{ChatName, NameKind};
    ///
    /// let names = [ChatName { name: "riff".into(), kind: NameKind::Project, online: true }];
    /// let none = OpenIssues::default();
    /// let list = |line: &str| Ahead::default().list(&names, &none, line, 2).unwrap();
    /// assert_eq!(list("@r").text("@r"), "@riff ");
    /// assert_eq!(list("@r hi").text("@r hi"), "@riff");
    /// ```
    pub fn text(&self, line: &str) -> String {
        let space = if self.end == line.len() { " " } else { "" };
        format!("{}{space}", self.first().text)
    }

    /// `line` after the pick of the first entry, and the byte of the
    /// cursor after it.
    pub fn pick(&self, line: &str) -> (String, usize) {
        let text = self.text(line);
        let picked = format!("{}{text}{}", &line[..self.at], &line[self.end..]);
        (picked, self.at + text.len())
    }

    /// A row for each entry that shows, as the person reads it, cut to
    /// `width` columns with `…`: its columns, two spaces apart. A row of
    /// a name has the name, its kind, and `online` or `offline`. A row of
    /// an issue has its number, its repository after the slash, and its
    /// title. A last row counts the other entries.
    ///
    /// ```
    /// use riff::typeahead::{Ahead, OpenIssue, OpenIssues, SHOWN};
    /// use riff_core::wire::{ChatName, NameKind};
    ///
    /// let person = |n: usize| ChatName { name: format!("ann{n}"), kind: NameKind::Person, online: n == 0 };
    /// let names: Vec<ChatName> = (0..SHOWN + 2).map(person).collect();
    /// let none = OpenIssues::default();
    /// let rows = Ahead::default().list(&names, &none, "@a", 2).unwrap().rows(80);
    /// assert_eq!(rows.len(), SHOWN + 1);
    /// assert_eq!(rows[0], "ann0  person   online");
    /// assert_eq!(rows[SHOWN], "and 2 more: type more letters");
    ///
    /// // A long title is cut to the width.
    /// let issue = OpenIssue {
    ///     repo: "o/riff".into(),
    ///     number: 1864,
    ///     title: "Projects C: riff questions, the TUI, riff top and riff chat".into(),
    ///     opened: None,
    /// };
    /// let mut issues = OpenIssues::new(Some("o/riff".into()));
    /// issues.read(vec![("o/riff".into(), Ok(vec![issue]))]);
    /// let rows = Ahead::default().list(&[], &issues, "#P", 2).unwrap().rows(40);
    /// assert_eq!(rows, ["#1864  riff  Projects C: riff questions…"]);
    /// ```
    pub fn rows(&self, width: usize) -> Vec<String> {
        let mut widths: Vec<usize> = Vec::new();
        for entry in &self.entries {
            for (i, column) in entry.columns.iter().enumerate() {
                let w = textwrap::core::display_width(column);
                match widths.get_mut(i) {
                    Some(most) => *most = (*most).max(w),
                    None => widths.push(w),
                }
            }
        }
        let mut rows: Vec<String> = self
            .entries
            .iter()
            .map(|entry| {
                let last = entry.columns.len().saturating_sub(1);
                let mut row = String::new();
                for (i, column) in entry.columns.iter().enumerate() {
                    if i > 0 {
                        row.push_str("  ");
                    }
                    row.push_str(column);
                    if i < last {
                        let pad = widths[i] - textwrap::core::display_width(column);
                        row.push_str(&" ".repeat(pad));
                    }
                }
                crate::tui::waves::cut(&row, width)
            })
            .collect();
        if self.more > 0 {
            let what = match self.mark {
                '#' => "letters or digits",
                _ => "letters",
            };
            let row = format!("and {} more: type more {what}", self.more);
            rows.push(crate::tui::waves::cut(&row, width));
        }
        rows
    }

    /// The note of the list: the keys, and the entry that they pick.
    pub fn note(&self) -> String {
        format!(
            "Tab or Enter picks {}. Esc closes the list.",
            self.first().name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(name: &str) -> ChatName {
        ChatName {
            name: name.into(),
            kind: NameKind::Person,
            online: true,
        }
    }

    fn issue(repo: &str, number: u64, title: &str) -> OpenIssue {
        OpenIssue {
            repo: repo.into(),
            number,
            title: title.into(),
            opened: i64::try_from(number).ok(),
        }
    }

    /// The open issues of riff, the repository of the working folder, and
    /// of strata.
    fn issues() -> OpenIssues {
        let mut issues = OpenIssues::new(Some("como-technologies/riff".into()));
        let riff = "como-technologies/riff";
        let strata = "como-technologies/strata";
        issues.read(vec![
            (
                riff.into(),
                Ok(vec![
                    issue(
                        riff,
                        1862,
                        "Projects A: riff-server keeps the projects of the riff",
                    ),
                    issue(riff, 1864, "Projects C: riff questions, the TUI, riff top"),
                    issue(riff, 186, "Show the board"),
                    issue(riff, 1880, "Pick an issue with #"),
                ]),
            ),
            (
                strata.into(),
                Ok(vec![issue(strata, 12, "The issues of strata")]),
            ),
        ]);
        issues
    }

    /// The numbers of the issues of the list of `line` with the cursor at
    /// its end.
    fn listed(issues: &OpenIssues, line: &str) -> Option<Vec<String>> {
        let list = Ahead::default().list(&[], issues, line, line.len())?;
        Some(list.entries.into_iter().map(|e| e.text).collect())
    }

    #[test]
    fn the_word_is_a_mark_at_the_start_or_after_a_space_up_to_the_cursor() {
        assert_eq!(word("@", 1), Some((0, '@', "")));
        assert_eq!(word("hi @ri", 6), Some((3, '@', "ri")));
        assert_eq!(word("hi\t@ri there", 6), Some((3, '@', "ri")));
        assert_eq!(word("is #186", 7), Some((3, '#', "186")));
        assert_eq!(word("#", 1), Some((0, '#', "")));
        assert_eq!(word("hi @ri", 5), None);
        assert_eq!(word("hi ri", 5), None);
        assert_eq!(word("a@b", 3), None);
        assert_eq!(word("o/r#12", 6), None);
        // A wide space before the word, and a byte in a character.
        assert_eq!(word("hi\u{3000}@r", 7), Some((5, '@', "r")));
        assert_eq!(word("é@", 1), None);
        assert_eq!(word("", 0), None);
    }

    /// 01M74CX2V9YAF0KFPJDRM4DA83: digits fit the start of the number,
    /// the newest issue first.
    #[test]
    fn digits_fit_the_start_of_the_number_the_newest_first() {
        let issues = issues();
        assert_eq!(
            listed(&issues, "is #186").unwrap(),
            ["#1864", "#1862", "#186"]
        );
        assert_eq!(listed(&issues, "#18").unwrap()[0], "#1880");
        // `#` alone lists each issue, the newest first.
        assert_eq!(
            listed(&issues, "#").unwrap(),
            [
                "#1880",
                "#1864",
                "#1862",
                "#186",
                "como-technologies/strata#12"
            ]
        );
        assert_eq!(listed(&issues, "#9"), None);
        // The number of the one issue that fits: no list.
        assert_eq!(listed(&issues, "#1880"), None);
        assert_eq!(listed(&issues, "#12"), None);
        // A number that more issues start with: the list stays.
        assert_eq!(listed(&issues, "#186").unwrap().len(), 3);
    }

    /// 01M74CX2V9YAF0KFPJDRM4DA83: letters fit the start of a word of
    /// the title, with no regard to case. A character that is no letter
    /// and no digit gives no list.
    #[test]
    fn letters_fit_the_start_of_a_word_of_the_title() {
        let issues = issues();
        assert_eq!(listed(&issues, "#proj").unwrap(), ["#1864", "#1862"]);
        assert_eq!(listed(&issues, "#SERV").unwrap(), ["#1862"]);
        assert_eq!(
            listed(&issues, "#issue").unwrap(),
            ["#1880", "como-technologies/strata#12"]
        );
        // `ojects` is in a word, not at its start.
        assert_eq!(listed(&issues, "#ojects"), None);
        assert_eq!(listed(&issues, "#riff-s"), None);
        assert_eq!(listed(&issues, "#12,"), None);
    }

    /// 01M74CX2V9YAF0KFPJDRM4DA83: the list shows the five newest issues
    /// that fit, and counts the others.
    #[test]
    fn a_list_of_issues_shows_five_and_counts_the_others() {
        let mut issues = OpenIssues::new(Some("o/r".into()));
        let many = (1..=8).map(|n| issue("o/r", n, "Fix it")).collect();
        issues.read(vec![("o/r".into(), Ok(many))]);
        let list = Ahead::default().list(&[], &issues, "#fix", 4).unwrap();
        assert_eq!(list.entries.len(), SHOWN);
        assert_eq!(list.first().text, "#8");
        assert_eq!(list.more, 3);
        assert_eq!(
            list.rows(80).last().unwrap(),
            "and 3 more: type more letters or digits"
        );
    }

    /// 01M74CX9H7WT1T4EN70AVD2TGZ: a row has the number, the repository
    /// after the slash and the title, in columns, cut to the width.
    #[test]
    fn a_row_of_an_issue_has_the_number_the_repository_and_the_title() {
        let issues = issues();
        let list = Ahead::default().list(&[], &issues, "#is", 3).unwrap();
        assert_eq!(
            list.rows(80),
            [
                "#1880  riff    Pick an issue with #",
                "#12    strata  The issues of strata",
            ]
        );
        let list = Ahead::default().list(&[], &issues, "#proj", 5).unwrap();
        assert_eq!(
            list.rows(40),
            [
                "#1864  riff  Projects C: riff questions…",
                "#1862  riff  Projects A: riff-server ke…",
            ]
        );
        assert_eq!(list.rows(0), ["", ""]);
    }

    /// 01M74CX9H7WT1T4EN70AVD2TGZ: the pick of an issue of the repository
    /// of the working folder puts `#N`, of another repository
    /// `OWNER/REPO#N`, and a space at the end of the line.
    #[test]
    fn a_pick_puts_the_number_or_the_repository_and_the_number() {
        let issues = issues();
        let ahead = Ahead::default();
        let line = "is #186";
        let list = ahead.list(&[], &issues, line, line.len()).unwrap();
        assert_eq!(
            list.note(),
            "Tab or Enter picks #1864. Esc closes the list."
        );
        assert_eq!(list.pick(line), ("is #1864 ".to_owned(), 9));
        let line = "see #strata";
        let list = ahead.list(&[], &issues, line, line.len()).unwrap();
        assert_eq!(
            list.note(),
            "Tab or Enter picks como-technologies/strata#12. Esc closes the list."
        );
        assert_eq!(
            list.pick(line),
            ("see como-technologies/strata#12 ".to_owned(), 32)
        );
        // In the middle of the line: no space.
        let line = "ask #stra now";
        let list = ahead.list(&[], &issues, line, 9).unwrap();
        assert_eq!(
            list.pick(line),
            ("ask como-technologies/strata#12 now".to_owned(), 31)
        );
    }

    /// 01M74CX9H7WT1T4EN70AVD2TGZ: `Esc` closes the list of issues until
    /// the cursor leaves the word, as for a name.
    #[test]
    fn esc_closes_the_list_of_issues_until_the_cursor_leaves_the_word() {
        let issues = issues();
        let mut ahead = Ahead::default();
        ahead.close("is #186", 7);
        assert!(ahead.list(&[], &issues, "is #186", 7).is_none());
        ahead.edited("is #1862", 8);
        assert!(ahead.list(&[], &issues, "is #186", 7).is_none());
        ahead.edited("is ", 3);
        assert!(ahead.list(&[], &issues, "is #186", 7).is_some());
    }

    /// An issue with no time sorts after the issues with one, and two
    /// repositories with one time sort by number, then by name.
    #[test]
    fn the_order_is_the_time_then_the_number_then_the_repository() {
        let at = |repo: &str, number, opened| OpenIssue {
            repo: repo.into(),
            number,
            title: "t".into(),
            opened,
        };
        let mut issues = OpenIssues::new(None);
        issues.read(vec![
            (
                "o/a".into(),
                Ok(vec![at("o/a", 3, None), at("o/a", 1, Some(5))]),
            ),
            (
                "o/b".into(),
                Ok(vec![at("o/b", 1, Some(5)), at("o/b", 2, Some(5))]),
            ),
        ]);
        let order: Vec<(&str, u64)> = issues
            .newest()
            .iter()
            .map(|i| (i.repo.as_str(), i.number))
            .collect();
        assert_eq!(order, [("o/b", 2), ("o/a", 1), ("o/b", 1), ("o/a", 3)]);
    }

    #[test]
    fn a_pick_in_the_middle_of_the_line_puts_no_space() {
        let names = [person("brett")];
        let ahead = Ahead::default();
        let none = OpenIssues::default();
        let line = "ask @b about #12";
        let list = ahead.list(&names, &none, line, 6).unwrap();
        assert_eq!(list.pick(line), ("ask @brett about #12".to_owned(), 10));
        let line = "@b";
        let list = ahead.list(&names, &none, line, 2).unwrap();
        assert_eq!(list.pick(line), ("@brett ".into(), 7));
    }

    #[test]
    fn a_close_holds_for_its_word_only() {
        let names = [person("brett"), person("bob")];
        let none = OpenIssues::default();
        let mut ahead = Ahead::default();
        ahead.close("@b @b", 5);
        assert!(ahead.list(&names, &none, "@b @b", 5).is_none());
        assert!(ahead.list(&names, &none, "@b @b", 2).is_some());
        // A key in the word keeps the list closed.
        ahead.edited("@b @br", 6);
        assert!(ahead.list(&names, &none, "@b @br", 6).is_none());
        // The cursor goes to the other word: the close goes.
        ahead.edited("@b @br", 2);
        assert!(ahead.list(&names, &none, "@b @br", 6).is_some());
        // Esc with no word closes nothing.
        ahead.close("hi", 2);
        assert!(ahead.list(&names, &none, "@b", 2).is_some());
    }

    #[test]
    fn a_name_or_a_title_with_an_escape_shows_with_no_escape() {
        let names = [person("x\x1b[2Jy")];
        let none = OpenIssues::default();
        let list = Ahead::default().list(&names, &none, "@x", 2).unwrap();
        assert_eq!(list.rows(80), ["xy  person   online"]);
        assert_eq!(list.note(), "Tab or Enter picks xy. Esc closes the list.");
        let mut issues = OpenIssues::new(Some("o/r".into()));
        let bad = issue("o/r", 7, "Fix\x1b]0;owned\x07 it");
        issues.read(vec![("o/r".into(), Ok(vec![bad]))]);
        let list = Ahead::default().list(&[], &issues, "#fix", 4).unwrap();
        assert_eq!(list.rows(80), ["#7  r  Fix it"]);
    }
}
