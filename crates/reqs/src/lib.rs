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
//! The cut of a requirement superseded in part is a check of a change,
//! not of one text: [`uncut`] compares `requirements.md` before and
//! after the change. It finds each pair that the change adds ([`in_part`])
//! where the text of the old requirement stays as it was. The job
//! `Hygiene` of a pull request runs it (`hygiene pr`, rule `in-part`,
//! 01M6WBVMJY79HJRQJZ3ZVD5CCC).
//!
//! ```mermaid
//! flowchart LR
//!     O[requirements.md before] --> P[pairs: B supersedes A in part]
//!     N[requirements.md after] --> P
//!     P -- "a pair the change adds" --> T{text of A changed?}
//!     T -- no --> E[in-part: names A and B]
//!     T -- yes --> K[ok]
//! ```
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

/// A requirement `by` that supersedes the requirement `id` in part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InPart {
    /// The ID of the old requirement: its text keeps only the part that
    /// holds.
    pub id: String,
    /// The ID of the new requirement.
    pub by: String,
}

impl InPart {
    fn new(id: &str, by: &str) -> Self {
        Self {
            id: id.to_owned(),
            by: by.to_owned(),
        }
    }
}

/// Each pair of `requirements` (the text of [`REQUIREMENTS`]) where one
/// requirement supersedes another in part, in the order of the page and
/// each pair once. A block says it in one of two forms:
///
/// - The new one: "Supersedes A in part", "Supersedes A and B in part",
///   "Supersedes in part A", "Supersedes A in full, and B in part", also
///   with "supersedes" or "replaces". Each ID before "in part" counts,
///   up to the last "in full" or the start of the clause. After
///   "Supersedes in part", each ID of the clause counts.
/// - The old one: "Superseded in part by B", "Superseded by B in part",
///   "Replaced in part by B".
///
/// A clause ends at `.`, `:` or `;`. A line break may split it.
///
/// ```
/// use reqs::InPart;
///
/// let page = "- **R3** New. Supersedes R1 in full, and R2 in part.\n\
///             - **R4** Newer. Supersedes in part R3\n  and R2: the rest stays.\n\
///             - **R5** Old. Superseded by R4 in part.\n\
///             - **R6** Supersedes R5.\n";
/// let pairs = reqs::in_part(page);
/// assert_eq!(pairs, [
///     InPart { id: "R2".into(), by: "R3".into() },
///     InPart { id: "R3".into(), by: "R4".into() },
///     InPart { id: "R2".into(), by: "R4".into() },
///     InPart { id: "R5".into(), by: "R4".into() },
/// ]);
/// ```
pub fn in_part(requirements: &str) -> Vec<InPart> {
    let mut pairs: Vec<InPart> = Vec::new();
    for (id, text) in blocks(requirements) {
        for pair in block_in_part(id, &text) {
            if !pairs.contains(&pair) {
                pairs.push(pair);
            }
        }
    }
    pairs
}

/// Each requirement that a change from `old` to `new` (two texts of
/// [`REQUIREMENTS`]) supersedes in part, and whose text the change does
/// not cut. The change supersedes a requirement A in part when `new`
/// has a pair of [`in_part`] that `old` does not have, and A is in
/// `old`. It cuts A when the text of A in `new` differs from the text
/// in `old`. The compare skips the wrap, and each sentence that marks A
/// as superseded in part: such a mark alone is no cut. A that `new` no
/// longer has moved to the history page, superseded in full: no error.
///
/// ```
/// use reqs::InPart;
///
/// let old = "- **R1** One. Two.\n";
/// let new = "- **R1** One. Two.\n- **R2** Three. Supersedes R1 in part.\n";
/// assert_eq!(reqs::uncut(old, new), [InPart { id: "R1".into(), by: "R2".into() }]);
///
/// let cut = "- **R1** One.\n- **R2** Three. Supersedes R1 in part.\n";
/// assert!(reqs::uncut(old, cut).is_empty());
///
/// let marked = "- **R1** One. Two. Superseded in part\n  by R2.\n- **R2** Three.\n";
/// assert_eq!(reqs::uncut(old, marked), [InPart { id: "R1".into(), by: "R2".into() }]);
/// ```
pub fn uncut(old: &str, new: &str) -> Vec<InPart> {
    let before: BTreeMap<&str, String> = blocks(old).into_iter().collect();
    let after: BTreeMap<&str, String> = blocks(new).into_iter().collect();
    let known = in_part(old);
    in_part(new)
        .into_iter()
        .filter(|pair| !known.contains(pair))
        .filter(|pair| {
            let (Some(was), Some(is)) = (before.get(pair.id.as_str()), after.get(pair.id.as_str()))
            else {
                return false;
            };
            unmarked(&pair.id, was) == unmarked(&pair.id, is)
        })
        .collect()
}

/// The ID and the text of each requirement of `requirements`, in the
/// order of the page. The text is the words of the block, with one
/// space between two words.
fn blocks(requirements: &str) -> Vec<(&str, String)> {
    let lines: Vec<&str> = requirements.lines().collect();
    let mut out = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        if let Some(id) = defined(line) {
            let text = lines[n..=block_end(&lines, n)]
                .iter()
                .flat_map(|l| l.split_whitespace())
                .collect::<Vec<_>>()
                .join(" ");
            out.push((id, text));
        }
    }
    out
}

/// The pairs of [`in_part`] that the block `text` of requirement `id`
/// says.
fn block_in_part(id: &str, text: &str) -> Vec<InPart> {
    let mut pairs = Vec::new();
    for clause in clauses(text, &["supersedes ", "replaces "]) {
        let words: Vec<&str> = words(clause).collect();
        let lead = words.starts_with(&["in", "part"]);
        let mut pending = Vec::new();
        let mut k = 0;
        while k < words.len() {
            match (words[k], words.get(k + 1)) {
                ("in", Some(&"part")) => {
                    pairs.extend(pending.drain(..).map(|old| InPart::new(old, id)));
                    k += 2;
                }
                ("in", Some(&"full")) => {
                    pending.clear();
                    k += 2;
                }
                (word, _) => {
                    if is_citation(word) && word != id {
                        pending.push(word);
                    }
                    k += 1;
                }
            }
        }
        if lead {
            pairs.extend(pending.into_iter().map(|old| InPart::new(old, id)));
        }
    }
    for clause in clauses(text, &["superseded ", "replaced "]) {
        let words: Vec<&str> = words(clause).collect();
        if words.windows(2).any(|w| w == ["in", "part"]) {
            pairs.extend(
                words
                    .into_iter()
                    .filter(|w| is_citation(w) && *w != id)
                    .map(|by| InPart::new(id, by)),
            );
        }
    }
    pairs
}

/// The clauses of `text` after each of the words `starts`, in upper or
/// lower case: up to the next `.`, `:` or `;`, or the end.
fn clauses<'a>(text: &'a str, starts: &[&str]) -> Vec<&'a str> {
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    for start in starts {
        for (at, _) in lower.match_indices(start) {
            let rest = &text[at + start.len()..];
            let end = rest.find(['.', ':', ';']).unwrap_or(rest.len());
            out.push(&rest[..end]);
        }
    }
    out
}

/// The text of requirement `id` with no sentence that marks it as
/// superseded in part: such a sentence names a pair of [`in_part`] with
/// `id` as the old requirement.
fn unmarked(id: &str, text: &str) -> String {
    text.split_inclusive(". ")
        .filter(|sentence| !block_in_part(id, sentence).iter().any(|pair| pair.id == id))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// True for a word that has the form of a citation: an old ID or a
/// ULID.
fn is_citation(word: &str) -> bool {
    r_number(word).is_some() || is_ulid(word)
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

    /// The rules of today before a change: R1 with two sentences.
    const BEFORE: &str = "## A\n\n- **R1** The old rule. It has\n  two parts.\n";

    #[test]
    fn a_change_that_supersedes_in_part_and_leaves_the_text_fails() {
        let id = new_id();
        let after = format!("{BEFORE}- **{id}** The new rule. Supersedes R1 in part.\n");
        assert_eq!(uncut(BEFORE, &after), [InPart::new("R1", &id)]);
    }

    #[test]
    fn a_change_that_supersedes_in_part_and_cuts_the_text_passes() {
        let id = new_id();
        let after = format!(
            "## A\n\n- **R1** The old rule.\n- **{id}** The new rule. Supersedes R1 in part.\n"
        );
        assert_eq!(uncut(BEFORE, &after), []);
    }

    #[test]
    fn a_new_wrap_is_no_cut() {
        let id = new_id();
        let after = format!(
            "## A\n\n- **R1** The old\n  rule. It has two parts.\n- **{id}** Supersedes in part\n  R1.\n"
        );
        assert_eq!(uncut(BEFORE, &after), [InPart::new("R1", &id)]);
    }

    #[test]
    fn a_mark_on_the_old_requirement_alone_is_no_cut() {
        let id = new_id();
        for old in [
            format!("- **R1** The old rule. It has\n  two parts. Superseded in part by {id}.\n"),
            format!(
                "- **R1** The old rule. Replaced in part by {id}: the second part. It has two parts.\n"
            ),
        ] {
            let after = format!("## A\n\n{old}- **{id}** The new rule.\n");
            assert_eq!(uncut(BEFORE, &after), [InPart::new("R1", &id)], "{old}");
        }
        let after = format!(
            "## A\n\n- **R1** The old rule. Superseded in part by {id}.\n- **{id}** The new rule.\n"
        );
        assert_eq!(uncut(BEFORE, &after), []);
    }

    #[test]
    fn each_old_requirement_of_a_new_one_needs_its_cut() {
        let id = new_id();
        let before = format!("{BEFORE}- **R2** Two.\n- **R3** Three.\n");
        let after = format!(
            "{BEFORE}- **R2** Two, cut.\n- **R3** Three.\n- **{id}** Supersedes R1, R2 and R3 in part.\n"
        );
        assert_eq!(
            uncut(&before, &after),
            [InPart::new("R1", &id), InPart::new("R3", &id)]
        );
    }

    #[test]
    fn a_pair_of_an_older_change_is_not_checked_again() {
        let id = new_id();
        let before = format!("{BEFORE}- **{id}** New. Supersedes R1 in part.\n");
        let after = format!("{before}- **R2** Other.\n");
        assert_eq!(uncut(&before, &after), []);
    }

    #[test]
    fn an_old_requirement_that_is_new_or_gone_needs_no_cut() {
        let (new, newer) = (new_id(), new_id());
        let after =
            format!("{BEFORE}- **{new}** New.\n- **{newer}** Newer. Supersedes {new} in part.\n");
        assert_eq!(uncut(BEFORE, &after), [], "the old one is new too");
        let moved = format!("## A\n\n- **{new}** New. Supersedes R1 in part.\n");
        assert_eq!(uncut(BEFORE, &moved), [], "R1 moved to the history page");
    }

    #[test]
    fn supersedes_in_full_is_not_in_part() {
        let id = new_id();
        for text in [
            format!("- **{id}** Supersedes R1.\n"),
            format!("- **{id}** Supersedes R1 in full.\n"),
            format!("- **{id}** Superseded by R1.\n"),
            format!("- **{id}** Supersedes R1. A part of the screen stays in part.\n"),
        ] {
            assert_eq!(in_part(&text), [], "{text}");
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
