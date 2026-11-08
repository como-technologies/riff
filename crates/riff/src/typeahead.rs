//! The type-ahead of `@` in `riff chat` and in the Chat tab of the TUI
//! (#1771, 01M6Y1EKGKNW2MH970PDF6PJK1, 01M6Y1EKKCCZ5215V49Z5T3HKH).
//!
//! # Design
//!
//! riff-server gives the names of the chat ([`riff_client::api::Api::names`],
//! 01M6XZ27XV8RAC8B72DCM937ST): each person and each project, with its
//! kind and whether its role is online. A UI keeps the names of its last
//! read, and reads them again each [`READ_EVERY`]. After each key, it
//! asks [`Ahead::list`] for the list of the line at the cursor, and
//! shows it under the prompt of `riff chat`, or over the prompt of the
//! Chat tab:
//!
//! ```text
//! [riff] > is #12 done? @r
//!   riff  project  online
//!   rita  person   offline
//!   Tab or Enter picks riff. Esc closes the list.
//! ```
//!
//! - **The word** is an `@` at the start of the line or after a space,
//!   and the letters after it up to the cursor. The cursor is at the end
//!   of the word: at the end of the line, or before a space.
//! - **The list** holds each name that starts with the letters, or whose
//!   part after the slash does, with no regard to case, in the order of
//!   riff-server. It shows at most [`SHOWN`] names, and the number of
//!   the others. With no name, or when the word is the one name of the
//!   list already, no list shows.
//! - **Tab or Enter** picks the first name of the list ([`List::pick`]):
//!   the name takes the place of the letters, and a space comes after it
//!   at the end of the line.
//! - **Esc** closes the list ([`Ahead::close`]). It stays closed until
//!   the cursor leaves the word ([`Ahead::edited`]).
//! - **The list never blocks the line.** A key types as before, and a
//!   line with a name that is not in the list posts as it is: riff-server
//!   resolves each name at the post (01M6XZ280AMTAMPD6R4212YC2T).
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> NoList
//!     NoList --> List: type @ and letters that fit a name
//!     List --> List: a letter that fits a name
//!     List --> NoList: Tab or Enter picks the first name
//!     List --> Closed: Esc
//!     Closed --> NoList: the cursor leaves the word
//!     List --> NoList: a letter that fits no name
//! ```
//!
//! ```
//! use riff::typeahead::Ahead;
//! use riff_core::wire::{ChatName, NameKind};
//!
//! let name = |name: &str, kind, online| ChatName { name: name.into(), kind, online };
//! let names = [
//!     name("brett", NameKind::Person, true),
//!     name("riff", NameKind::Project, true),
//!     name("rita", NameKind::Person, false),
//! ];
//! let mut ahead = Ahead::default();
//! let line = "is #12 done? @r";
//! let list = ahead.list(&names, line, line.len()).unwrap();
//! assert_eq!(list.rows(), ["riff  project  online", "rita  person   offline"]);
//! assert_eq!(list.note(), "Tab or Enter picks riff. Esc closes the list.");
//! assert_eq!(list.pick(line), ("is #12 done? @riff ".to_owned(), 19));
//! // Esc closes the list until the cursor leaves the word.
//! ahead.close(line, line.len());
//! assert!(ahead.list(&names, line, line.len()).is_none());
//! ahead.edited("is #12 done? @ri", 16);
//! assert!(ahead.list(&names, "is #12 done? @ri", 16).is_none());
//! ahead.edited("is #12 done? ", 13);
//! assert!(ahead.list(&names, "is #12 done? @r", 15).is_some());
//! ```

use std::time::Duration;

use riff_core::wire::{ChatName, NameKind};

use crate::text::safe;

/// The most names that a list shows.
pub const SHOWN: usize = 5;

/// The time between two reads of the names: the online state of a name
/// changes.
pub const READ_EVERY: Duration = Duration::from_secs(30);

/// The type-ahead of a prompt: the word of a list that the person closed
/// with `Esc`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ahead {
    /// The byte of the `@` of the word whose list the person closed.
    closed: Option<usize>,
}

impl Ahead {
    /// The list of `names`, the names of the last read in the order of
    /// riff-server, for `line` with the cursor at the byte `pos`. `None`
    /// when the cursor is at the end of no `@` word, when no name fits
    /// the word, when the word is the one name that fits, and while the
    /// list of the word is closed.
    ///
    /// ```
    /// use riff::typeahead::Ahead;
    /// use riff_core::wire::{ChatName, NameKind};
    ///
    /// let project = |name: &str| ChatName { name: name.into(), kind: NameKind::Project, online: false };
    /// let names = ["acme/riff", "brett", "como-technologies/riff", "mike"].map(project);
    /// let shown = |line: &str, pos: usize| {
    ///     let list = Ahead::default().list(&names, line, pos);
    ///     list.map(|l| l.names.iter().map(|n| n.name.clone()).collect::<Vec<_>>())
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
    pub fn list(&self, names: &[ChatName], line: &str, pos: usize) -> Option<List> {
        let (at, letters) = word(line, pos)?;
        if self.closed == Some(at) {
            return None;
        }
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
        Some(List {
            at,
            end: pos,
            more: fits.len().saturating_sub(SHOWN),
            names: fits.into_iter().take(SHOWN).cloned().collect(),
        })
    }

    /// The person changed `line`, or moved the cursor to the byte `pos`:
    /// a closed list opens again when the cursor left its word.
    pub fn edited(&mut self, line: &str, pos: usize) {
        if word(line, pos).map(|(at, _)| at) != self.closed {
            self.closed = None;
        }
    }

    /// `Esc`: closes the list of the word at the byte `pos` of `line`.
    pub fn close(&mut self, line: &str, pos: usize) {
        self.closed = word(line, pos).map(|(at, _)| at);
    }
}

/// The `@` word that ends at the byte `pos` of `line`: the byte of its
/// `@`, and the letters after the `@`. `None` when `pos` is in a word,
/// or the word at the cursor does not start with `@`.
fn word(line: &str, pos: usize) -> Option<(usize, &str)> {
    let (before, after) = (line.get(..pos)?, line.get(pos..)?);
    if after.starts_with(|c: char| !c.is_whitespace()) {
        return None;
    }
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let letters = before[start..].strip_prefix('@')?;
    Some((start, letters))
}

/// The list of the names that fit an `@` word ([`Ahead::list`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    /// The byte of the `@` in the line.
    pub at: usize,
    /// The byte of the end of the word: the cursor.
    pub end: usize,
    /// The names that show, at most [`SHOWN`]: the first one is the pick
    /// of `Tab` and `Enter`.
    pub names: Vec<ChatName>,
    /// The number of the other names that fit the word.
    pub more: usize,
}

impl List {
    /// The name that `Tab` and `Enter` pick.
    pub fn first(&self) -> &ChatName {
        &self.names[0]
    }

    /// The text that takes the place of the word in `line`: `@`, the
    /// first name, and a space at the end of the line.
    ///
    /// ```
    /// use riff::typeahead::Ahead;
    /// use riff_core::wire::{ChatName, NameKind};
    ///
    /// let names = [ChatName { name: "riff".into(), kind: NameKind::Project, online: true }];
    /// let list = |line: &str| Ahead::default().list(&names, line, 2).unwrap();
    /// assert_eq!(list("@r").text("@r"), "@riff ");
    /// assert_eq!(list("@r hi").text("@r hi"), "@riff");
    /// ```
    pub fn text(&self, line: &str) -> String {
        let space = if self.end == line.len() { " " } else { "" };
        format!("@{}{space}", safe(&self.first().name))
    }

    /// `line` after the pick of the first name, and the byte of the
    /// cursor after it.
    pub fn pick(&self, line: &str) -> (String, usize) {
        let text = self.text(line);
        let picked = format!("{}{text}{}", &line[..self.at], &line[self.end..]);
        (picked, self.at + text.len())
    }

    /// A row for each name that shows, as the person reads it: the name,
    /// its kind, and `online` or `offline`, in columns. A last row counts
    /// the other names.
    ///
    /// ```
    /// use riff::typeahead::{Ahead, SHOWN};
    /// use riff_core::wire::{ChatName, NameKind};
    ///
    /// let person = |n: usize| ChatName { name: format!("ann{n}"), kind: NameKind::Person, online: n == 0 };
    /// let names: Vec<ChatName> = (0..SHOWN + 2).map(person).collect();
    /// let rows = Ahead::default().list(&names, "@a", 2).unwrap().rows();
    /// assert_eq!(rows.len(), SHOWN + 1);
    /// assert_eq!(rows[0], "ann0  person   online");
    /// assert_eq!(rows[SHOWN], "and 2 more: type more letters");
    /// ```
    pub fn rows(&self) -> Vec<String> {
        let shown: Vec<String> = self.names.iter().map(|n| safe(&n.name)).collect();
        let width = shown
            .iter()
            .map(|n| textwrap::core::display_width(n))
            .max()
            .unwrap_or(0);
        let mut rows: Vec<String> = shown
            .iter()
            .zip(&self.names)
            .map(|(name, n)| {
                let pad = " ".repeat(width - textwrap::core::display_width(name));
                let kind = match n.kind {
                    NameKind::Person => "person ",
                    NameKind::Project => "project",
                };
                let online = if n.online { "online" } else { "offline" };
                format!("{name}{pad}  {kind}  {online}")
            })
            .collect();
        if self.more > 0 {
            rows.push(format!("and {} more: type more letters", self.more));
        }
        rows
    }

    /// The note of the list: the keys, and the name that they pick.
    pub fn note(&self) -> String {
        format!(
            "Tab or Enter picks {}. Esc closes the list.",
            safe(&self.first().name)
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

    #[test]
    fn the_word_is_an_at_at_the_start_or_after_a_space_up_to_the_cursor() {
        assert_eq!(word("@", 1), Some((0, "")));
        assert_eq!(word("hi @ri", 6), Some((3, "ri")));
        assert_eq!(word("hi\t@ri there", 6), Some((3, "ri")));
        assert_eq!(word("hi @ri", 5), None);
        assert_eq!(word("hi ri", 5), None);
        assert_eq!(word("a@b", 3), None);
        // A wide space before the word, and a byte in a character.
        assert_eq!(word("hi\u{3000}@r", 7), Some((5, "r")));
        assert_eq!(word("é@", 1), None);
        assert_eq!(word("", 0), None);
    }

    #[test]
    fn a_pick_in_the_middle_of_the_line_puts_no_space() {
        let names = [person("brett")];
        let ahead = Ahead::default();
        let line = "ask @b about #12";
        let list = ahead.list(&names, line, 6).unwrap();
        assert_eq!(list.pick(line), ("ask @brett about #12".to_owned(), 10));
        let line = "@b";
        let list = ahead.list(&names, line, 2).unwrap();
        assert_eq!(list.pick(line), ("@brett ".into(), 7));
    }

    #[test]
    fn a_close_holds_for_its_word_only() {
        let names = [person("brett"), person("bob")];
        let mut ahead = Ahead::default();
        ahead.close("@b @b", 5);
        assert!(ahead.list(&names, "@b @b", 5).is_none());
        assert!(ahead.list(&names, "@b @b", 2).is_some());
        // A key in the word keeps the list closed.
        ahead.edited("@b @br", 6);
        assert!(ahead.list(&names, "@b @br", 6).is_none());
        // The cursor goes to the other word: the close goes.
        ahead.edited("@b @br", 2);
        assert!(ahead.list(&names, "@b @br", 6).is_some());
        // Esc with no word closes nothing.
        ahead.close("hi", 2);
        assert!(ahead.list(&names, "@b", 2).is_some());
    }

    #[test]
    fn a_name_with_an_escape_shows_with_no_escape() {
        let names = [person("x\x1b[2Jy")];
        let list = Ahead::default().list(&names, "@x", 2).unwrap();
        assert_eq!(list.rows(), ["xy  person   online"]);
        assert_eq!(list.note(), "Tab or Enter picks xy. Esc closes the list.");
    }
}
