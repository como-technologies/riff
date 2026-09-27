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
//! - The old IDs, `R` and a number, stay as they are. They get no ULID and no
//!   new number.
//! - `just ci` runs [`check`] on `docs/src/requirements.md` and on each
//!   text file that can cite a requirement:
//!
//! | Check | Level | Finds |
//! |---|---|---|
//! | `duplicate-id` | error | Two requirements with the same ID. |
//! | `unknown-id` | error | A cited ID that no requirement has. |
//! | `id-format` | warning | A requirement ID that is not an old ID and not a valid ULID. |
//!
//! A requirement is a line of the form `- **ID** text`. A citation is a
//! word that has the form of an old ID or of a ULID.
//!
//! ```
//! let id = reqs::new_id();
//! assert!(reqs::is_ulid(&id));
//! assert_ne!(id, reqs::new_id());
//!
//! let requirements = format!("- **R1** One.\n- **{id}** Two.\n");
//! let cited = format!("See R1 and {id}.");
//! let report = reqs::check(&requirements, &[("src/lib.rs", cited.as_str())]);
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

/// True for an old requirement ID: `R` and a number.
///
/// ```
/// assert!(reqs::is_legacy("R233"));
/// assert!(!reqs::is_legacy("R"));
/// assert!(!reqs::is_legacy("R12a"));
/// ```
pub fn is_legacy(word: &str) -> bool {
    word.strip_prefix('R')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// What [`check`] found. Each line names the file and the line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// `duplicate-id` and `unknown-id`. Each fails `just ci`.
    pub errors: Vec<String>,
    /// `id-format`.
    pub warnings: Vec<String>,
}

/// The path of the requirements in the repository.
pub const REQUIREMENTS: &str = "docs/src/requirements.md";

/// Checks the requirement IDs of `requirements` (the text of
/// [`REQUIREMENTS`]), and each ID that `sources` cite. Each source is a
/// path and its text.
///
/// ```
/// let requirements = "- **R1** One.\n- **R1** Again.\n- **X-1** Bad.\n";
/// let report = reqs::check(requirements, &[("a.rs", "// R1, R7")]);
/// assert_eq!(report.errors, [
///     "docs/src/requirements.md:2: duplicate-id: R1 is also on line 1",
///     "a.rs:1: unknown-id: R7 is not in docs/src/requirements.md",
/// ]);
/// assert_eq!(report.warnings, [
///     "docs/src/requirements.md:3: id-format: X-1 is not a ULID (run just rid)",
/// ]);
/// ```
pub fn check(requirements: &str, sources: &[(&str, &str)]) -> Report {
    let mut report = Report::default();
    let mut ids: BTreeMap<&str, usize> = BTreeMap::new();
    for (n, line) in requirements.lines().enumerate() {
        let Some(id) = defined(line) else {
            continue;
        };
        let n = n + 1;
        if let Some(first) = ids.get(id) {
            report.errors.push(format!(
                "{REQUIREMENTS}:{n}: duplicate-id: {id} is also on line {first}"
            ));
            continue;
        }
        ids.insert(id, n);
        if !is_legacy(id) && !is_ulid(id) {
            report.warnings.push(format!(
                "{REQUIREMENTS}:{n}: id-format: {id} is not a ULID (run just rid)"
            ));
        }
    }
    let mut cite = |path: &str, text: &str, skip_defined: bool| {
        for (n, line) in text.lines().enumerate() {
            let own = if skip_defined { defined(line) } else { None };
            for word in words(line) {
                if Some(word) == own || !(is_legacy(word) || is_ulid(word)) {
                    continue;
                }
                if !ids.contains_key(word) {
                    report.errors.push(format!(
                        "{path}:{}: unknown-id: {word} is not in {REQUIREMENTS}",
                        n + 1
                    ));
                }
            }
        }
    };
    cite(REQUIREMENTS, requirements, true);
    for (path, text) in sources {
        cite(path, text, false);
    }
    report
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
        assert_eq!(check(&merged, &[]), Report::default());
    }

    #[test]
    fn a_cited_ulid_must_exist() {
        let id = new_id();
        let other = new_id();
        let requirements = format!("- **{id}** One.\n");
        let source = format!("// {id} and {other}\n");
        let report = check(&requirements, &[("x.rs", &source)]);
        assert_eq!(
            report.errors,
            [format!(
                "x.rs:1: unknown-id: {other} is not in {REQUIREMENTS}"
            )]
        );
    }

    #[test]
    fn a_requirement_can_cite_another_one() {
        let requirements = "- **R1** One.\n- **R2** Like R1, but not R3.\n";
        let report = check(requirements, &[]);
        assert_eq!(
            report.errors,
            ["docs/src/requirements.md:2: unknown-id: R3 is not in docs/src/requirements.md"]
        );
    }

    #[test]
    fn other_words_are_not_ids() {
        let requirements = "- **R1** One.\n";
        let text = "RIFF_USER R2D2 Rust ABCDEFGHIJKLMNOPQRSTUVWXYZ r12 R1x";
        assert!(check(requirements, &[("a.md", text)]).errors.is_empty());
    }
}
