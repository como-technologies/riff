//! Requirement IDs of this repository.
//!
//! # Design
//!
//! Workers in parallel add requirements at the same time. A number that
//! each worker picks as "the next free one" clashes. So a new
//! requirement gets a [ULID](https://github.com/ulid/spec): 48 bits of
//! time and 80 random bits, in 26 characters of Crockford base32. Two
//! workers never pick the same ID, and they need no message about it.
//!
//! - `just rid` prints a new ID ([`new_id`]). Nobody writes an ID by hand
//!   or gives it a meaning.
//! - The old IDs `R1` to `R232` stay as they are. They get no ULID and no
//!   new number. `R233` and up are not valid IDs.
//! - `just ci` runs [`check`] on `docs/src/requirements.md`, on
//!   `docs/src/requirements-history.md` and on each text file that can
//!   cite a requirement:
//!
//! | Check | Level | Finds |
//! |---|---|---|
//! | `duplicate-id` | error | Two requirements with the same ID, also one on each page. |
//! | `unknown-id` | error | A cited ID that no requirement has, on either page. |
//! | `id-format` | warning | A requirement ID that is not an old ID (`R1` to `R232`) and not a valid ULID. |
//! | `block-end` | error | A requirement block whose last line does not end with a full stop. A new requirement in the middle of another one causes it. |
//! | `superseded-in-full` | error | A requirement in `requirements.md` that says "Superseded by" or "Replaced by" and does not say "in part". |
//!
//! A requirement is a line of the form `- **ID** text`. Its block is
//! that line and the indented lines under it. A citation is a word that
//! has the form of an old ID or of a ULID.
//!
//! `requirements.md` holds the rules of today (01M5RGXD7WV4H3VR0YA7JW9GXR,
//! 01M5RGXDB28TBJJ9X1N9YQATZ6). A requirement that a new one supersedes
//! in full moves to `requirements-history.md` ([`HISTORY`]) in the same
//! change, in the same section, with its ID and its text unchanged. A
//! requirement that a new one supersedes in part stays, and its text
//! keeps only the part that holds. A citation of an ID finds it on
//! either page.
//!
//! `just req SECTION "text"` adds a requirement ([`add`]): it gets an ID
//! like `just rid`, wraps the text at 72 characters, and writes it after
//! the last requirement of the section, never inside one. It writes to
//! `requirements.md` only. Nobody edits `requirements.md` by hand to add
//! a requirement.
//!
//! ```
//! let id = reqs::new_id();
//! assert!(reqs::is_ulid(&id));
//! assert_ne!(id, reqs::new_id());
//!
//! let requirements = format!("- **R1** One.\n- **{id}** Two.\n");
//! let history = format!("- **R2** Old. Superseded by {id}.\n");
//! let cited = format!("See R1, R2 and {id}.");
//! let report = reqs::check(&requirements, &history, &[("src/lib.rs", cited.as_str())]);
//! assert!(report.errors.is_empty() && report.warnings.is_empty());
//! ```

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// The 32 characters of Crockford base32, in value order.
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A new ULID from the clock and the random source of the OS.
pub fn new_id() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let mut random = [0u8; 10];
    getrandom::fill(&mut random).expect("the OS gives random bytes");
    ulid(ms, random)
}

/// The ULID of a time in milliseconds since the Unix epoch and 80
/// random bits.
///
/// ```
/// assert_eq!(reqs::ulid(0, [0; 10]), "00000000000000000000000000");
/// assert_eq!(reqs::ulid(1_469_918_176_385, [0xff; 10]), "01ARYZ6S41ZZZZZZZZZZZZZZZZ");
/// ```
pub fn ulid(ms: u64, random: [u8; 10]) -> String {
    let mut value = u128::from(ms & 0xFFFF_FFFF_FFFF) << 80;
    for (i, byte) in random.iter().enumerate() {
        value |= u128::from(*byte) << (72 - 8 * i);
    }
    (0..26)
        .rev()
        .map(|i| char::from(CROCKFORD[((value >> (5 * i)) & 31) as usize]))
        .collect()
}

/// True for a valid ULID: 26 characters of upper-case Crockford base32,
/// and a first character of at most `7`, so it fits in 128 bits.
///
/// ```
/// assert!(reqs::is_ulid("01ARYZ6S41ZZZZZZZZZZZZZZZZ"));
/// assert!(!reqs::is_ulid("01ARYZ6S41ZZZZZZZZZZZZZZZ"), "25 characters");
/// assert!(!reqs::is_ulid("01ARYZ6S41ZZZZZZZZZZZZZZZI"), "I is not Crockford");
/// assert!(!reqs::is_ulid("81ARYZ6S41ZZZZZZZZZZZZZZZZ"), "more than 128 bits");
/// ```
pub fn is_ulid(word: &str) -> bool {
    word.len() == 26 && word.bytes().all(|b| CROCKFORD.contains(&b)) && word.as_bytes()[0] <= b'7'
}

/// The last old requirement ID. A new requirement never gets an old
/// ID: `R233` and up are not valid.
pub const LAST_LEGACY: u32 = 232;

/// True for an old requirement ID: `R1` to `R232` ([`LAST_LEGACY`]).
///
/// ```
/// assert!(reqs::is_legacy("R1"));
/// assert!(reqs::is_legacy("R232"));
/// assert!(!reqs::is_legacy("R233"), "a new ID gets a ULID");
/// assert!(!reqs::is_legacy("R0"));
/// assert!(!reqs::is_legacy("R12a"));
/// ```
pub fn is_legacy(word: &str) -> bool {
    r_number(word).is_some_and(|n| (1..=LAST_LEGACY).contains(&n))
}

/// The number of a word of the form `R` and a number.
fn r_number(word: &str) -> Option<u32> {
    let n = word.strip_prefix('R')?;
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(n.parse().unwrap_or(u32::MAX))
}

/// What [`check`] found. Each line names the file and the line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// `duplicate-id`, `unknown-id`, `block-end` and `superseded-in-full`.
    /// Each fails `just ci`.
    pub errors: Vec<String>,
    /// `id-format`.
    pub warnings: Vec<String>,
}

/// The path of the requirements of today in the repository.
pub const REQUIREMENTS: &str = "docs/src/requirements.md";

/// The path of the history page: the requirements that a new one
/// superseded in full.
pub const HISTORY: &str = "docs/src/requirements-history.md";

/// Checks the requirement IDs of `requirements` (the text of
/// [`REQUIREMENTS`]) and of `history` (the text of [`HISTORY`]), and each
/// ID that `sources` cite. Each source is a path and its text. An ID is
/// unique across both pages, and a citation finds its ID on either one.
///
/// A requirement of `requirements` whose block says "Superseded by" or
/// "Replaced by" and does not say "in part" is superseded in full. It
/// belongs on the history page.
///
/// ```
/// let requirements = "- **R1** One.\n- **R1** Again.\n- **X-1** Bad.\n- **R233** New.\n";
/// let history = "- **R3** Old. Superseded by R1.\n- **R1** Twice.\n";
/// let report = reqs::check(requirements, history, &[("a.rs", "// R1, R3, R7")]);
/// assert_eq!(report.errors, [
///     "docs/src/requirements.md:2: duplicate-id: R1 is also at docs/src/requirements.md:1",
///     "docs/src/requirements-history.md:2: duplicate-id: R1 is also at docs/src/requirements.md:1",
///     "a.rs:1: unknown-id: R7 is not in docs/src/requirements.md or docs/src/requirements-history.md",
/// ]);
/// assert_eq!(report.warnings, [
///     "docs/src/requirements.md:3: id-format: X-1 is not a ULID (run just rid)",
///     "docs/src/requirements.md:4: id-format: R233 is not a ULID (run just rid)",
/// ]);
///
/// let report = reqs::check("- **R1** Old. Superseded by R2.\n- **R2** New.\n", "", &[]);
/// assert_eq!(report.errors, [
///     "docs/src/requirements.md:1: superseded-in-full: R1 says \"Superseded by\" and not \"in part\". \
///      Move it to docs/src/requirements-history.md, in the same section, with its ID and text unchanged",
/// ]);
///
/// let report = reqs::check("- **R1** Replaced by R2.\n- **R2** New.\n", "", &[]);
/// assert!(report.errors[0].contains("R1 says \"Replaced by\" and not \"in part\""));
/// ```
pub fn check(requirements: &str, history: &str, sources: &[(&str, &str)]) -> Report {
    let mut report = Report::default();
    let mut ids: BTreeMap<&str, (&str, usize)> = BTreeMap::new();
    for (path, text) in [(REQUIREMENTS, requirements), (HISTORY, history)] {
        let lines: Vec<&str> = text.lines().collect();
        for (n, line) in lines.iter().enumerate() {
            let Some(id) = defined(line) else {
                continue;
            };
            let n = n + 1;
            if let Some((first_path, first)) = ids.get(id) {
                report.errors.push(format!(
                    "{path}:{n}: duplicate-id: {id} is also at {first_path}:{first}"
                ));
                continue;
            }
            ids.insert(id, (path, n));
            if !is_legacy(id) && !is_ulid(id) {
                report.warnings.push(format!(
                    "{path}:{n}: id-format: {id} is not a ULID (run just rid)"
                ));
            }
        }
        for (n, line) in lines.iter().enumerate() {
            let Some(id) = defined(line) else {
                continue;
            };
            let end = block_end(&lines, n);
            if !lines[end].trim_end().ends_with('.') {
                report.errors.push(format!(
                    "{path}:{}: block-end: the requirement on line {} does not end with a full stop (a new requirement inside it?)",
                    end + 1,
                    n + 1
                ));
            }
            if path == REQUIREMENTS
                && let Some(marker) = superseded_in_full(&lines[n..=end])
            {
                report.errors.push(format!(
                    "{path}:{}: superseded-in-full: {id} says \"{marker}\" and not \"in part\". \
                     Move it to {HISTORY}, in the same section, with its ID and text unchanged",
                    n + 1
                ));
            }
        }
    }
    let mut cite = |path: &str, text: &str, skip_defined: bool| {
        for (n, line) in text.lines().enumerate() {
            let own = if skip_defined { defined(line) } else { None };
            for word in words(line) {
                if Some(word) == own || !(r_number(word).is_some() || is_ulid(word)) {
                    continue;
                }
                if !ids.contains_key(word) {
                    report.errors.push(format!(
                        "{path}:{}: unknown-id: {word} is not in {REQUIREMENTS} or {HISTORY}",
                        n + 1
                    ));
                }
            }
        }
    };
    cite(REQUIREMENTS, requirements, true);
    cite(HISTORY, history, true);
    for (path, text) in sources {
        cite(path, text, false);
    }
    report
}

/// The words that mark a requirement as superseded in full, when the block
/// `block` has one of them and does not say "in part". A line break may
/// split any of the phrases.
fn superseded_in_full(block: &[&str]) -> Option<&'static str> {
    let text = block
        .iter()
        .flat_map(|l| l.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ");
    if text.contains("in part") {
        return None;
    }
    ["Superseded by", "Replaced by"]
        .into_iter()
        .find(|marker| text.contains(marker))
}

/// The last line (0-based) of the requirement block that starts on line
/// `start`: the indented lines under it, up to a blank or other line.
fn block_end(lines: &[&str], start: usize) -> usize {
    let mut end = start;
    while lines
        .get(end + 1)
        .is_some_and(|l| l.starts_with(' ') && !l.trim().is_empty())
    {
        end += 1;
    }
    end
}

/// The text of `requirements` with a new requirement `text`, with the
/// ID `id`, after the last requirement of the section `section` (the
/// text of a heading, with no `#`). The lines of the new requirement
/// are at most 72 characters wide. Other lines stay as they are.
///
/// ```
/// let book = "# T\n\n## A\n\n- **R1** One,\n  two.\n\n## B\n\n- **R2** Two.\n";
/// let id = reqs::new_id();
/// let out = reqs::add(book, "A", &id, "New.").unwrap();
/// assert_eq!(out, format!("# T\n\n## A\n\n- **R1** One,\n  two.\n- **{id}** New.\n\n## B\n\n- **R2** Two.\n"));
/// assert!(reqs::add(book, "C", &id, "New.").is_err());
/// ```
pub fn add(requirements: &str, section: &str, id: &str, text: &str) -> Result<String, String> {
    if !text.trim_end().ends_with('.') {
        return Err("the text of a requirement must end with a full stop".to_string());
    }
    let lines: Vec<&str> = requirements.lines().collect();
    let heading = lines
        .iter()
        .position(|l| l.starts_with('#') && l.trim_start_matches('#').trim() == section)
        .ok_or_else(|| format!("no section {section:?} in {REQUIREMENTS}"))?;
    let mut last = None;
    for (n, line) in lines.iter().enumerate().skip(heading + 1) {
        if line.starts_with('#') {
            break;
        }
        if defined(line).is_some() {
            last = Some(block_end(&lines, n));
        }
    }
    let last = last.ok_or_else(|| format!("section {section:?} has no requirement"))?;
    let mut block = Vec::new();
    let mut line = format!("- **{id}**");
    let mut words_on_line = 0;
    for word in text.split_whitespace() {
        if words_on_line > 0 && line.len() + 1 + word.len() > 72 {
            block.push(line);
            line = String::from(" ");
            words_on_line = 0;
        }
        line.push(' ');
        line.push_str(word);
        words_on_line += 1;
    }
    block.push(line);
    let mut out: Vec<String> = lines.iter().map(|l| (*l).to_string()).collect();
    out.splice(last + 1..last + 1, block);
    let mut out = out.join("\n");
    if requirements.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// The ID of a requirement line `- **ID** text`.
fn defined(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("- **")?;
    let (id, _) = rest.split_once("**")?;
    (!id.is_empty() && !id.contains(char::is_whitespace)).then_some(id)
}

/// The words of a line: runs of ASCII letters and digits.
fn words(line: &str) -> impl Iterator<Item = &str> {
    line.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_sort_by_time() {
        let ids: Vec<String> = (0..1000).map(|_| new_id()).collect();
        let unique: std::collections::BTreeSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        assert!(ulid(1, [0; 10]) < ulid(2, [0; 10]));
        assert!(ids.iter().all(|id| is_ulid(id)));
    }

    #[test]
    fn two_workers_add_requirements_with_no_clash() {
        let base = "- **R1** Old.\n";
        let a = format!("- **{}** From worker a.\n", new_id());
        let b = format!("- **{}** From worker b.\n", new_id());
        let merged = format!("{base}{a}{b}");
        assert_eq!(check(&merged, "", &[]), Report::default());
    }

    #[test]
    fn a_cited_ulid_must_exist() {
        let id = new_id();
        let other = new_id();
        let requirements = format!("- **{id}** One.\n");
        let source = format!("// {id} and {other}\n");
        let report = check(&requirements, "", &[("x.rs", &source)]);
        assert_eq!(
            report.errors,
            [format!(
                "x.rs:1: unknown-id: {other} is not in {REQUIREMENTS} or {HISTORY}"
            )]
        );
    }

    #[test]
    fn a_requirement_can_cite_another_one() {
        let requirements = "- **R1** One.\n- **R2** Like R1, but not R3.\n";
        let report = check(requirements, "", &[]);
        assert_eq!(
            report.errors,
            [
                "docs/src/requirements.md:2: unknown-id: R3 is not in docs/src/requirements.md or docs/src/requirements-history.md"
            ]
        );
    }

    #[test]
    fn a_requirement_split_by_a_new_one_is_an_error() {
        let id = new_id();
        let requirements = format!("- **R1** One,\n- **{id}** New.\n  and two.\n");
        let report = check(&requirements, "", &[]);
        assert_eq!(
            report.errors,
            [format!(
                "{REQUIREMENTS}:1: block-end: the requirement on line 1 does not end with a full stop (a new requirement inside it?)"
            )]
        );
    }

    #[test]
    fn add_wraps_and_keeps_the_other_lines() {
        let book = "## A\n\n- **R1** One.\n\n## B\n\n- **R2** Two.\n";
        let id = new_id();
        let text = format!("{}end.", "word ".repeat(40));
        let out = add(book, "A", &id, &text).unwrap();
        assert!(add(book, "A", &id, "No stop").is_err());
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[..3], ["## A", "", "- **R1** One."]);
        assert_eq!(lines.last(), Some(&"- **R2** Two."));
        assert!(lines[3].starts_with(&format!("- **{id}** word")));
        assert!(lines[3..lines.len() - 5].iter().all(|l| l.len() <= 72));
        assert!(lines[4].starts_with("  word"));
        assert_eq!(
            check(&out, "", &[]),
            Report {
                errors: vec![],
                warnings: vec![]
            }
        );
    }

    #[test]
    fn other_words_are_not_ids() {
        let requirements = "- **R1** One.\n";
        let text = "RIFF_USER R2D2 Rust ABCDEFGHIJKLMNOPQRSTUVWXYZ r12 R1x";
        assert!(check(requirements, "", &[("a.md", text)]).errors.is_empty());
    }

    #[test]
    fn a_citation_of_an_id_on_the_history_page_resolves() {
        let (new, old) = (new_id(), new_id());
        let requirements = format!("- **{new}** New, like {old}.\n");
        let history = format!("- **{old}** Old. Superseded by {new}.\n");
        let source = format!("// {old} and {new}\n");
        assert_eq!(
            check(&requirements, &history, &[("x.rs", &source)]),
            Report::default()
        );
        let report = check(&requirements, "", &[("x.rs", &source)]);
        assert_eq!(report.errors.len(), 2, "{report:?}");
        assert!(report.errors.iter().all(|e| e.contains("unknown-id")));
    }

    #[test]
    fn a_history_requirement_may_cite_a_requirement_of_today() {
        let requirements = "- **R1** One.\n";
        let history = "- **R2** Old. Superseded by R1.\n";
        assert_eq!(check(requirements, history, &[]), Report::default());
        let report = check("", "- **R2** Old. Superseded by R1.\n", &[]);
        assert_eq!(
            report.errors,
            [format!(
                "{HISTORY}:1: unknown-id: R1 is not in {REQUIREMENTS} or {HISTORY}"
            )]
        );
    }

    #[test]
    fn an_id_on_both_pages_is_a_duplicate() {
        let id = new_id();
        let requirements = format!("- **{id}** One.\n");
        let history = format!("\n- **{id}** Old.\n");
        let report = check(&requirements, &history, &[]);
        assert_eq!(
            report.errors,
            [format!(
                "{HISTORY}:2: duplicate-id: {id} is also at {REQUIREMENTS}:1"
            )]
        );
    }

    #[test]
    fn a_superseded_requirement_left_in_the_rules_of_today_is_an_error() {
        let id = new_id();
        let requirements =
            format!("- **R1** Old,\n  and more. Superseded by\n  {id}.\n- **{id}** New.\n");
        let report = check(&requirements, "", &[]);
        assert_eq!(report.errors.len(), 1, "{report:?}");
        assert!(
            report.errors[0].starts_with(&format!("{REQUIREMENTS}:1: superseded-in-full: R1 says"))
        );
        assert!(report.errors[0].contains(HISTORY), "{report:?}");
        // The same text on the history page is where it belongs.
        assert_eq!(check("", &requirements, &[]), Report::default());
    }

    #[test]
    fn a_replaced_requirement_left_in_the_rules_of_today_is_an_error() {
        let id = new_id();
        for text in [
            format!("- **R1** Replaced by {id}.\n"),
            format!("- **R1** Replaced by\n  {id}.\n"),
        ] {
            let requirements = format!("{text}- **{id}** New.\n");
            let report = check(&requirements, "", &[]);
            assert_eq!(report.errors.len(), 1, "{text}: {report:?}");
            assert!(
                report.errors[0].starts_with(&format!(
                    "{REQUIREMENTS}:1: superseded-in-full: R1 says \"Replaced by\""
                )),
                "{report:?}"
            );
            assert_eq!(check("", &requirements, &[]), Report::default(), "{text}");
        }
    }

    #[test]
    fn a_requirement_superseded_in_part_stays() {
        let id = new_id();
        for text in [
            format!("- **R1** One. Superseded in part by {id}.\n"),
            format!("- **R1** One. Superseded by {id} in\n  part.\n"),
            format!("- **R1** One. Replaced in part by {id}.\n"),
            "- **R1** One. The old one is superseded by another.\n".to_string(),
        ] {
            let requirements = format!("{text}- **{id}** New.\n");
            assert_eq!(check(&requirements, "", &[]), Report::default(), "{text}");
        }
    }

    #[test]
    fn a_split_requirement_on_the_history_page_is_an_error() {
        let history = "- **R1** One,\n- **R2** Two.\n";
        let report = check("", history, &[]);
        assert_eq!(report.errors.len(), 1, "{report:?}");
        assert!(report.errors[0].starts_with(HISTORY), "{report:?}");
        assert!(report.errors[0].contains("block-end"), "{report:?}");
    }
}
