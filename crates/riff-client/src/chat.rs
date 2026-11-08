//! The names of a chat line: whom a line of the thread `chat` wakes
//! (01M6Y1A5PTCVZ6GRJ0FXMART2T). `riff chat`, the Chat tab of the TUI
//! and each chat line of riff itself share them.
//!
//! # Design
//!
//! A line wakes only the names that it holds: `@brett` wakes the person
//! brett, and `@riff` the lead of the sender in the project riff. The
//! UI reads the names of the line with [`names`], and posts the line
//! with one selector `name=NAME` for each name ([`to`]). It checks no
//! name, and refuses no line for a name: riff-server resolves each name
//! when the line is posted (`riff_state::state::names`). A line with no
//! name wakes no one.
//!
//! ```mermaid
//! flowchart LR
//!     L["the line"] --> N["names:<br/>the word after each @"]
//!     N --> T["to:<br/>name=NAME for each name"]
//!     T --> P["the post"]
//!     P --> S["riff-server resolves each name"]
//! ```

use riff_core::selector::Selector;

/// Each name of a chat line, in its order: the word after each `@` at
/// the start of a word. A name has letters, digits, `-`, `_`, `.` and
/// `/` (`OWNER/REPO`). The first other character ends it, and
/// punctuation at its end does not count. [`to`] reads the names so, and
/// the highlight of a line in `riff chat` too.
///
/// ```
/// use riff_client::chat::names;
///
/// let named: Vec<_> = names("@brett: can @riff, or (@ann) ask mike@thelio?").collect();
/// assert_eq!(named, ["brett", "riff"]);
/// let named: Vec<_> = names("@como-technologies/riff. @sens-o-matic! @lead:strata @").collect();
/// assert_eq!(named, ["como-technologies/riff", "sens-o-matic", "lead"]);
/// ```
pub fn names(line: &str) -> impl Iterator<Item = &str> {
    line.split_whitespace().filter_map(|word| {
        let after = word.strip_prefix('@')?;
        let end = after
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/')))
            .unwrap_or(after.len());
        let name = after[..end].trim_end_matches(|c: char| !c.is_alphanumeric());
        (!name.is_empty()).then_some(name)
    })
}

/// The selectors of the post of the chat line `line`: one `name=NAME`
/// for each name of [`names`], once, with no regard to case. A line
/// with no name has none, so it wakes no one. The UI adds no selector
/// of its own.
///
/// ```
/// use riff_client::chat::to;
/// use riff_core::selector::Selector;
///
/// assert_eq!(to("@riff is #12 done?"), [Selector::name("riff")]);
/// assert_eq!(
///     to("@brett @riff @Brett, ok?"),
///     [Selector::name("brett"), Selector::name("riff")]
/// );
/// assert_eq!(to("/me asks @riff to look"), [Selector::name("riff")]);
/// // A name that no one has goes too: riff-server resolves it.
/// assert_eq!(to("@nobody hi"), [Selector::name("nobody")]);
/// assert!(to("is #12 done? mail mike@thelio").is_empty());
/// ```
pub fn to(line: &str) -> Vec<Selector> {
    let mut named: Vec<&str> = Vec::new();
    for name in names(line) {
        if !named.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            named.push(name);
        }
    }
    named.into_iter().map(Selector::name).collect()
}
