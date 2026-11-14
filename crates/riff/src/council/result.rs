//! The part `## The council` of the result of a verify: a pass names each
//! member of the council (01M7CFBZSQEEZNZBR9H1S54GS1,
//! 01M7CX5BTPFX77KS5J87C7GBY2, 01M7D11KGEJ8ACFJK6Y10WENJZ).
//!
//! # Design
//!
//! A pass of a pull request reads its council as `riff council show` does
//! ([`super::pick::of_pull`]), after the checks of the head commit and
//! before the squash message ([`crate::pr::report`]). [`check`] then finds
//! the line of each member in the part `## The council` of the result. A
//! member with no line is a [`NoLine`]: `riff verify pass` reports nothing
//! and prints its line, and the daemon reads such a PASS of the wave as a
//! result with no verdict ([`crate::wave`]). A fail needs no such part.
//!
//! ```mermaid
//! flowchart TD
//!     R["the result of a pass"] --> P["the part ## The council:<br/>up to the next heading of its level"]
//!     C["the council of the pull request,<br/>as riff council show picks it"] --> K{"a line for each member?"}
//!     P --> K
//!     K -- yes --> G["the comment, the status, the post"]
//!     K -- "no" --> N["NoLine: it names each member with no line,<br/>and riff reports nothing"]
//! ```
//!
//! A line of a member is a line of the part whose first word is the name
//! of the member, in any case ([`first_word`]). A list mark can come
//! before it, and `:`, `*`, `_` or `` ` `` around it. The part runs from
//! its heading to the next heading with as many `#` or fewer, and each
//! such part counts. A line in a code block is no heading.
//!
//! ```
//! use riff::council::default_set;
//! use riff::council::pick::{Change, pick};
//! use riff::council::result::check;
//!
//! let change = Change { paths: vec!["src/a.rs".into()], labels: vec![] };
//! let council = pick(default_set()?, &change);
//! let result = "PASS\n\n- It works.\n\n## The council\n\n\
//!               - design: no finding.\n\
//!               - testing: no finding.\n\
//!               - **docs**: not shown: docs/a.md:3: .riff/council/docs.md \"No stale text\": an old flag.\n\
//!               - rust: no finding.\n";
//! assert!(check(&council, result).is_ok());
//! let fault = check(&council, &result.replace("- rust: no finding.\n", "")).unwrap_err();
//! assert_eq!(fault.members.iter().map(|m| m.as_str()).collect::<Vec<_>>(), ["rust"]);
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::fmt;

use super::Name;
use super::pick::Council;
use crate::text;

/// The heading of the part, as the start text of a verify gives it.
pub const HEADING: &str = "## The council";

/// The text of [`HEADING`]: a heading of another level or case with this
/// text starts the part too.
const TITLE: &str = "The council";

/// The members of a council with no line in the part `## The council` of
/// a result (01M7CFBZSQEEZNZBR9H1S54GS1). Its text is the line of `riff
/// verify pass` ([`text::verify_no_council_line`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoLine {
    /// Each member with no line, in the order of the council.
    pub members: Vec<Name>,
}

impl fmt::Display for NoLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&text::verify_no_council_line(&self.members))
    }
}

impl std::error::Error for NoLine {}

/// Checks that the part `## The council` of `result` has a line for each
/// member of `council` (01M7CFBZSQEEZNZBR9H1S54GS1,
/// 01M7D11KGEJ8ACFJK6Y10WENJZ). A result with no such part has a line for
/// no member.
///
/// ```
/// use riff::council::default_set;
/// use riff::council::pick::{Change, pick};
/// use riff::council::result::check;
///
/// let council = pick(default_set()?, &Change { paths: vec!["justfile".into()], labels: vec![] });
/// let fault = check(&council, "PASS\n\n- It works.\n").unwrap_err();
/// assert_eq!(fault.members.len(), 3);
/// assert!(fault.to_string().contains("for design, testing and docs."), "{fault}");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn check(council: &Council, result: &str) -> Result<(), NoLine> {
    let words: Vec<&str> = part(result).into_iter().filter_map(first_word).collect();
    let members: Vec<Name> = council
        .seats
        .iter()
        .map(|seat| &seat.member.name)
        .filter(|name| {
            !words
                .iter()
                .any(|word| word.eq_ignore_ascii_case(name.as_str()))
        })
        .cloned()
        .collect();
    match members.is_empty() {
        true => Ok(()),
        false => Err(NoLine { members }),
    }
}

/// The lines of each part `## The council` of `result`, each from the
/// line after its heading up to the next heading with as many `#` or
/// fewer. A line in a code block is no heading. No line when the result
/// has no such part.
fn part(result: &str) -> Vec<&str> {
    let mut fenced = false;
    // The level of the heading of the part that the line is in.
    let mut level = None;
    let mut lines = Vec::new();
    for line in result.lines() {
        let heading = match fenced {
            true => None,
            false => heading(line),
        };
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        match heading {
            Some((n, text)) if text.eq_ignore_ascii_case(TITLE) => level = Some(n),
            Some((n, _)) if level.is_some_and(|at| n <= at) => level = None,
            _ if level.is_some() => lines.push(line),
            _ => {}
        }
    }
    lines
}

/// The level and the text of the heading `line`: `(2, "The council")` for
/// `## The council`. `None` for a line that is no heading.
fn heading(line: &str) -> Option<(usize, &str)> {
    let marks = line.chars().take_while(|&c| c == '#').count();
    let text = line[marks..].strip_prefix(' ')?;
    (1..=6)
        .contains(&marks)
        .then(|| (marks, text.trim().trim_end_matches('#').trim_end()))
}

/// The first word of a line of the part, with no list mark before it and
/// no `:`, `*`, `_` or `` ` `` around it: `docs` for `- docs: no
/// finding.`, `* **docs**: no finding.` and `` `docs`: no finding.``.
/// `None` for a line with no word.
///
/// ```
/// use riff::council::result::first_word;
///
/// assert_eq!(first_word("- docs: no finding."), Some("docs"));
/// assert_eq!(first_word("  * **docs**: no finding."), Some("docs"));
/// assert_eq!(first_word("+ `docs`: no finding."), Some("docs"));
/// assert_eq!(first_word("- **docs:** no finding."), Some("docs"));
/// assert_eq!(first_word("docs no finding"), Some("docs"));
/// assert_eq!(first_word("- Each member reviewed it."), Some("Each"));
/// assert_eq!(first_word("- "), None);
/// ```
pub fn first_word(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let line = ["- ", "* ", "+ "]
        .iter()
        .find_map(|mark| line.strip_prefix(mark))
        .unwrap_or(line);
    line.split(|c: char| c.is_whitespace() || c == ':')
        .map(|word| word.trim_matches(|c: char| "*_`".contains(c)))
        .find(|word| !word.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::council::default_set;
    use crate::council::pick::{Change, pick};

    /// The council of a change of a `.rs` file: design, testing, docs and
    /// rust.
    fn council() -> Council {
        let change = Change {
            paths: vec!["crates/riff/src/council.rs".into()],
            labels: vec![],
        };
        pick(default_set().unwrap(), &change)
    }

    /// A PASS with the part `## The council`, with the line of each name
    /// of `names`.
    fn result(names: &[&str]) -> String {
        let lines: String = names
            .iter()
            .map(|name| format!("- {name}: no finding.\n"))
            .collect();
        format!(
            "PASS\n\n- The check passes: the test the_check_passes passed.\n\n{HEADING}\n\n{lines}"
        )
    }

    /// The names of the members with no line in `result`.
    fn no_line(result: &str) -> Vec<String> {
        match check(&council(), result) {
            Ok(()) => Vec::new(),
            Err(fault) => fault.members.iter().map(ToString::to_string).collect(),
        }
    }

    /// 01M7CFBZSQEEZNZBR9H1S54GS1: a PASS result with a line for each
    /// member of the council passes the check. One with no line for
    /// `docs` fails it, and the error names `docs`.
    #[test]
    fn a_pass_with_no_line_for_docs_fails_the_check_and_names_docs() {
        let council = council();
        assert_eq!(
            check(&council, &result(&["design", "testing", "docs", "rust"])),
            Ok(())
        );
        let fault = check(&council, &result(&["design", "testing", "rust"])).unwrap_err();
        assert_eq!(fault.members, ["docs".parse::<Name>().unwrap()]);
        let line = fault.to_string();
        assert!(
            line.contains("has no line in the part `## The council` for docs."),
            "{line}"
        );
        assert!(line.ends_with("Nothing is reported."), "{line}");
        assert_eq!(line.lines().count(), 1, "{line}");
    }

    /// With no part `## The council`, each member has no line, in the
    /// order of the council. A line of a member out of the part does not
    /// count.
    #[test]
    fn a_result_with_no_part_names_each_member() {
        let result = "PASS\n\n- It works.\n- docs: no finding.\n";
        assert_eq!(no_line(result), ["design", "testing", "docs", "rust"]);
        let fault = check(&council(), result).unwrap_err();
        assert!(
            fault
                .to_string()
                .contains("for design, testing, docs and rust."),
            "{fault}"
        );
    }

    /// 01M7D11KGEJ8ACFJK6Y10WENJZ: the part ends at the next heading with
    /// as many `#` or fewer. A line under a heading with more `#`, a
    /// heading of another case, marks around a name and a line after a
    /// code block count. A `#` line in a code block ends no part, and the
    /// name must be the whole first word. Each part `## The council`
    /// counts.
    #[test]
    fn the_part_ends_at_the_next_heading_of_its_level() {
        let after = "PASS\n\n## The Council\n\n- design: no finding.\n- testing: no finding.\n\n\
                     ## Notes\n\n- docs: no finding.\n- rust: no finding.\n";
        assert_eq!(no_line(after), ["docs", "rust"]);
        let deeper = "# Result\n\nPASS\n\n## The council\n\n- **Design**: no finding.\n\
                      ### More\n\n* `testing`: no finding.\n\n```sh\n# run it\n```\n\
                      - docs: not shown: a.md:1: .riff/council/docs.md \"A\": b.\n\
                      - rusty: no finding.\n\n# Next\n\n- rust: no finding.\n";
        assert_eq!(no_line(deeper), ["rust"]);
        let twice = "## The council\n\n- design: no finding.\n\n## Notes\n\n\
                     - testing: no finding.\n\n## The council\n\n- docs: no finding.\n\
                     - rust: no finding.\n";
        assert_eq!(no_line(twice), ["testing"]);
    }

    /// A heading is one to six `#`, then a space.
    #[test]
    fn a_heading_has_marks_and_a_space() {
        assert_eq!(heading("## The council"), Some((2, "The council")));
        assert_eq!(heading("### The council ##"), Some((3, "The council")));
        assert_eq!(heading("##The council"), None);
        assert_eq!(heading("####### Seven"), None);
        assert_eq!(heading("- design"), None);
    }
}
