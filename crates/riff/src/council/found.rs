//! The part `## New findings` of the result of a verify of a fix head
//! (01M7GDQWDA1Z08V4EDMP255ZF1, 01M7GDQWK92WWTTE361JGWDVGY): each finding
//! that the verifier shows and that does not fail the verify.
//!
//! # Design
//!
//! The verify of a fix head fails only on a criterion of "Done when", a
//! security fault, a finding of the last verify that stays, and a fault
//! in the diff since the head of the last verify. The verifier puts each
//! other finding that it shows in the part [`HEADING`] of its result,
//! one list item each ([`crate::text::wave_verify_start`]). The report
//! of the verify ([`crate::pr::report`]) files each such finding as an
//! issue of its own ([`file()`]), and writes the number of its issue before
//! it: `- #77: src/a.rs:12: ...`. A finding that starts so is filed
//! already, so a second report of the same result files nothing twice.
//!
//! ```mermaid
//! flowchart TD
//!     R["the result of a verify of a fix head"] --> P["the part ## New findings:<br/>up to the next # or ## heading"]
//!     P --> L{"a list item with no #N: ?"}
//!     L -- "yes" --> F["file it: an issue in the Backlog,<br/>Found by: #PR"]
//!     F --> W["the line gets #N: before the finding"]
//!     L -- "no" --> K["the line stays"]
//! ```
//!
//! The part runs from the heading [`HEADING`], of that level and case, to
//! the next heading of one or two `#`, and each such part counts, as the
//! part of the council does ([`super::result`]). A line in a code block
//! is no finding.
//!
//! ```
//! use riff::council::found::{file, unfiled};
//!
//! let mut result = "FAIL\n\n## New findings\n\n- src/a.rs:12: the rustdoc names an old flag.\n\
//!                   - #70: src/b.rs:3: a test gap.\n".to_owned();
//! assert_eq!(unfiled(&result), ["src/a.rs:12: the rustdoc names an old flag."]);
//! file(&mut result, |_| Ok(77))?;
//! assert!(result.ends_with("- #77: src/a.rs:12: the rustdoc names an old flag.\n- #70: src/b.rs:3: a test gap.\n"));
//! assert!(unfiled(&result).is_empty());
//! # Ok::<(), anyhow::Error>(())
//! ```

use anyhow::Result;

use crate::markdown;

/// The heading of the part of the new findings in a result, as the start
/// text of a verify of a fix head gives it
/// ([`crate::text::wave_verify_start`]).
pub const HEADING: &str = "## New findings";

/// Each finding of the parts [`HEADING`] of `result` that riff has not
/// filed yet, in the order of the result.
pub fn unfiled(result: &str) -> Vec<&str> {
    lines(result)
        .into_iter()
        .filter_map(|(_, found)| found)
        .collect()
}

/// Files each finding of [`unfiled`] with `issue`, which gives the number
/// of its issue, and writes `#N: ` before the finding in its line. It
/// writes each number in `result` as it goes: at the first fault of
/// `issue`, it stops and gives the fault, and each line before it keeps
/// its number.
///
/// ```
/// use riff::council::found::file;
///
/// let mut result = "## New findings\n\n- one.\n- two.\n- three.\n".to_owned();
/// let mut next = 76;
/// let fault = file(&mut result, |finding| {
///     next += 1;
///     match finding {
///         "two." => anyhow::bail!("HTTP 502"),
///         _ => Ok(next),
///     }
/// });
/// assert_eq!(fault.unwrap_err().to_string(), "HTTP 502");
/// assert_eq!(result, "## New findings\n\n- #77: one.\n- two.\n- three.\n");
/// ```
pub fn file(result: &mut String, mut issue: impl FnMut(&str) -> Result<u64>) -> Result<()> {
    let mut out = String::with_capacity(result.len());
    let mut fault = None;
    for (line, found) in lines(result) {
        match found.filter(|_| fault.is_none()).map(|f| (f, issue(f))) {
            Some((finding, Ok(number))) => {
                let at = line.trim_end().len() - finding.len();
                out.push_str(&format!("{}#{number}: {}", &line[..at], &line[at..]));
            }
            Some((_, Err(e))) => {
                fault = Some(e);
                out.push_str(line);
            }
            None => out.push_str(line),
        }
    }
    *result = out;
    fault.map_or(Ok(()), Err)
}

/// Each line of `result`, with its end, and the finding in it: the text of
/// a list item of a part [`HEADING`], out of a code block, that has no
/// number of an issue before it ([`filed`]). `None` for each other line.
fn lines(result: &str) -> Vec<(&str, Option<&str>)> {
    let Some(start) = markdown::heading(HEADING) else {
        return Vec::new();
    };
    let (mut inside, mut fenced) = (false, false);
    markdown::headings(result.split_inclusive('\n'))
        .map(|(line, heading)| {
            let fence = line.trim_start().starts_with("```");
            let found = match heading {
                Some(found) if found == start => {
                    inside = true;
                    None
                }
                Some((level, _)) if level <= start.0 => {
                    inside = false;
                    None
                }
                _ if inside && !fenced && !fence => finding(line),
                _ => None,
            };
            if fence {
                fenced = !fenced;
            }
            (line, found)
        })
        .collect()
}

/// The finding of the line `line` of the part: the text of a list item
/// ([`markdown::item`]) with no number of an issue before it. `None` for
/// a line with no list mark, an empty item, and a filed item.
///
/// ```
/// use riff::council::found::finding;
///
/// assert_eq!(finding("- src/a.rs:12: a gap.\n"), Some("src/a.rs:12: a gap."));
/// assert_eq!(finding("  1. a gap."), Some("a gap."));
/// assert_eq!(finding("- #77: a gap."), None);
/// assert_eq!(finding("- #rust: a gap."), Some("#rust: a gap."));
/// assert_eq!(finding("A line of text."), None);
/// assert_eq!(finding("- "), None);
/// ```
pub fn finding(line: &str) -> Option<&str> {
    let bare = line.trim();
    let text = markdown::item(bare);
    let marked = text.len() < bare.len();
    (marked && !text.is_empty() && !filed(text)).then_some(text)
}

/// True when the finding `text` starts with the number of its issue:
/// `#77: `.
fn filed(text: &str) -> bool {
    text.strip_prefix('#')
        .and_then(|rest| rest.split_once(": "))
        .is_some_and(|(number, _)| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 01M7GDQWK92WWTTE361JGWDVGY: only the list items of the part count.
    /// A line of text, a line in a code block and a line after the next
    /// heading of one or two `#` are no findings. A line under a heading
    /// of three `#` counts, and each part `## New findings` counts.
    #[test]
    fn only_a_list_item_of_the_part_is_a_finding() {
        let result = "FAIL\n\n- 1. A criterion fails.\n\n## The council\n\n- rust: no finding.\n\n\
                      ## New findings\n\nThe old code has these:\n\n- one.\n### More\n* two.\n\
                      ```text\n- no.\n```\n\n## Notes\n\n- no.\n\n## New findings\n\n3. three.\n";
        assert_eq!(unfiled(result), ["one.", "two.", "three."]);
        for other in ["## New Findings", "### New findings", "# New findings"] {
            let result = format!("{other}\n\n- one.\n");
            assert!(unfiled(&result).is_empty(), "{other}");
        }
        assert!(unfiled("PASS\n\n- It works.\n").is_empty());
    }

    /// 01M7GDQWK92WWTTE361JGWDVGY: [`file()`] keeps the list mark, the
    /// spaces and the end of each line, and each line that is no
    /// finding. A result with no part stays as it was, and files nothing.
    #[test]
    fn a_filed_line_keeps_its_mark_and_its_end() {
        let mut result = "## New findings\r\n\r\n  * one.\r\n10. two.".to_owned();
        let mut numbers = [12, 13].into_iter();
        file(&mut result, |_| Ok(numbers.next().unwrap())).unwrap();
        assert_eq!(
            result,
            "## New findings\r\n\r\n  * #12: one.\r\n10. #13: two."
        );

        let mut plain = "PASS\n\n- It works.\n".to_owned();
        file(&mut plain, |_| panic!("no finding")).unwrap();
        assert_eq!(plain, "PASS\n\n- It works.\n");
    }

    /// 01M7GDQWK92WWTTE361JGWDVGY: a second report of a result that a
    /// fault stopped files only the findings with no number.
    #[test]
    fn a_second_report_files_only_the_rest() {
        let mut result = "## New findings\n\n- one.\n- two.\n".to_owned();
        let fault = file(&mut result, |finding| match finding {
            "one." => Ok(77),
            _ => anyhow::bail!("the limit is out"),
        });
        assert!(fault.is_err());
        let mut asked = Vec::new();
        file(&mut result, |finding| {
            asked.push(finding.to_owned());
            Ok(78)
        })
        .unwrap();
        assert_eq!(asked, ["two."]);
        assert_eq!(result, "## New findings\n\n- #77: one.\n- #78: two.\n");
    }
}
