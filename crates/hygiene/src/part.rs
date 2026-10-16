//! The parts of the tests of the Gate.
//!
//! # Design
//!
//! The Gate on GitHub runs its checks as jobs at the same time
//! (01M513W2K3HNRWPQTQNBQPT8MZ). The tests take the most time, so they
//! run in N parts, each part in a job of its own. `just test-part I/N`
//! runs `hygiene test-part I/N` (01M513W2NSE303E6XAM18BW7T6):
//!
//! 1. `cargo test --workspace -- --list --format terse` builds each test
//!    binary and lists the name of each test, also of each doc test.
//! 2. [`names`] reads the names, and [`Part::plan`] keeps each name of a
//!    test that is not a doc test whose [`fnv1a`] hash modulo N is I − 1.
//! 3. `cargo test --workspace --tests --no-fail-fast -- --exact NAME...`
//!    runs these tests.
//! 4. Part 1 also runs each doc test: `cargo test --workspace --doc
//!    --no-fail-fast`.
//!
//! With `--no-fail-fast`, a failed test binary does not stop the others,
//! so the log of a part shows each failed test.
//!
//! A doc test has a name with spaces ([`is_doc`]), for example
//! `crates/x/src/lib.rs - ulid (line 57)`. rustdoc splits each filter at
//! its spaces, so no filter names one doc test. The doc tests take
//! seconds, so they run as one group.
//!
//! A name is in one part only, so the N parts together run each test. A
//! name that two test binaries share runs in each of them, as in a full
//! run. The hash of a name does not change with the other names, so a
//! new test moves no other test to another part.
//!
//! ```mermaid
//! flowchart LR
//!     L["cargo test -- --list:<br/>each name"] --> D{"a doc test?"}
//!     D -- yes --> P1["part 1: cargo test --doc"]
//!     D -- no --> H{"fnv1a(name) mod N"}
//!     H -- "I − 1" --> R["cargo test --tests -- --exact<br/>the names of part I"]
//!     H -- "another value" --> S["the job of another part"]
//! ```
//!
//! ```
//! use hygiene::part::{names, Part};
//!
//! let list = "cli::help: test\n\
//!             cli::version: test\n\
//!             src/lib.rs - ulid (line 57): test\n\
//!             bench::fast: benchmark\n";
//! let all = names(list);
//! assert_eq!(all, ["cli::help", "cli::version", "src/lib.rs - ulid (line 57)"]);
//!
//! let plans: Vec<_> = ["1/2", "2/2"]
//!     .iter()
//!     .map(|p| p.parse::<Part>().unwrap().plan(&all))
//!     .collect();
//! let mut each: Vec<&str> = plans.iter().flat_map(|p| p.tests.clone()).collect();
//! each.sort();
//! assert_eq!(each, ["cli::help", "cli::version"]);
//! assert_eq!((plans[0].doc, plans[1].doc), (1, 0));
//! assert!("3/2".parse::<Part>().is_err());
//! ```

use std::fmt;
use std::str::FromStr;

/// Part `index` of `count` parts of the tests, as `I/N` gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    /// I: from 1 to `count`.
    pub index: u64,
    /// N: 1 or more.
    pub count: u64,
}

impl FromStr for Part {
    type Err = String;

    /// Reads `I/N`, with 1 ≤ I ≤ N.
    ///
    /// ```
    /// use hygiene::part::Part;
    ///
    /// assert_eq!("2/4".parse(), Ok(Part { index: 2, count: 4 }));
    /// for bad in ["0/4", "5/4", "1/0", "2", "a/4", "1/4/2", ""] {
    ///     assert_eq!(
    ///         bad.parse::<Part>(),
    ///         Err(format!("\"{bad}\" is not a part. Give I/N with 1 ≤ I ≤ N, for example 2/4."))
    ///     );
    /// }
    /// ```
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = || format!("\"{text}\" is not a part. Give I/N with 1 ≤ I ≤ N, for example 2/4.");
        let (index, count) = text.split_once('/').ok_or_else(bad)?;
        let number = |n: &str| -> Result<u64, String> {
            if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            n.parse().map_err(|_| bad())
        };
        let (index, count) = (number(index)?, number(count)?);
        if index == 0 || index > count {
            return Err(bad());
        }
        Ok(Self { index, count })
    }
}

impl fmt::Display for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.index, self.count)
    }
}

/// What one part runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan<'a> {
    /// The names of the tests of the part that are not doc tests, in
    /// the order of the list.
    pub tests: Vec<&'a str>,
    /// The count of the doc tests that the part runs: each doc test in
    /// part 1, else none.
    pub doc: usize,
}

impl Part {
    /// True when the test `name`, which is not a doc test, is in this
    /// part.
    ///
    /// ```
    /// use hygiene::part::Part;
    ///
    /// let one = Part { index: 1, count: 1 };
    /// assert!(one.has("any::test"));
    /// ```
    pub fn has(&self, name: &str) -> bool {
        fnv1a(name) % self.count == self.index - 1
    }

    /// What this part runs of the tests `names`.
    pub fn plan<'a>(&self, names: &'a [String]) -> Plan<'a> {
        let (doc, tests): (Vec<&str>, Vec<&str>) =
            names.iter().map(String::as_str).partition(|n| is_doc(n));
        Plan {
            tests: tests.into_iter().filter(|name| self.has(name)).collect(),
            doc: if self.index == 1 { doc.len() } else { 0 },
        }
    }
}

/// True when `name` is the name of a doc test: it has a space, as in
/// `crates/x/src/lib.rs - ulid (line 57)`. The name of each other test
/// is a Rust path.
///
/// ```
/// use hygiene::part::is_doc;
///
/// assert!(is_doc("crates/x/src/lib.rs - ulid (line 57)"));
/// assert!(!is_doc("cli::help"));
/// ```
pub fn is_doc(name: &str) -> bool {
    name.contains(' ')
}

/// The 64-bit FNV-1a hash of `text`. It is the same on each machine and
/// with each Rust, so each job of the Gate gives a name the same part.
///
/// ```
/// use hygiene::part::fnv1a;
///
/// assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
/// assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
/// ```
pub fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The test names of the output of `cargo test -- --list --format
/// terse`: each line `NAME: test`, sorted, each name one time. A
/// benchmark is not a test.
pub fn names(list: &str) -> Vec<String> {
    let mut names: Vec<String> = list
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The line that `just test-part` prints before it runs the tests of
/// `plan`, of the tests `names`.
///
/// ```
/// use hygiene::part::{summary, Plan, Part};
///
/// let names = ["a".to_owned(), "b".to_owned(), "x.rs - f (line 1)".to_owned()];
/// let part = Part { index: 1, count: 2 };
/// let plan = Plan { tests: vec!["a"], doc: 1 };
/// assert_eq!(
///     summary(part, &plan, &names),
///     "just test-part 1/2 runs 1 of 2 tests, and the 1 doc tests"
/// );
/// let part = Part { index: 2, count: 2 };
/// let plan = Plan { tests: vec!["b"], doc: 0 };
/// assert_eq!(summary(part, &plan, &names), "just test-part 2/2 runs 1 of 2 tests");
/// ```
pub fn summary(part: Part, plan: &Plan<'_>, names: &[String]) -> String {
    let all = names.iter().filter(|n| !is_doc(n)).count();
    let line = format!(
        "just test-part {part} runs {} of {all} tests",
        plan.tests.len()
    );
    if plan.doc == 0 {
        line
    } else {
        format!("{line}, and the {} doc tests", plan.doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(count: u64) -> Vec<Part> {
        (1..=count).map(|index| Part { index, count }).collect()
    }

    #[test]
    fn each_name_is_in_exactly_one_part() {
        let names: Vec<String> = (0..500).map(|i| format!("module::test_{i}")).collect();
        for count in 1..=6 {
            let parts = all(count);
            for name in &names {
                let holders = parts.iter().filter(|p| p.has(name)).count();
                assert_eq!(holders, 1, "{name} in {count} parts");
            }
        }
    }

    #[test]
    fn the_parts_are_near_the_same_size() {
        let names: Vec<String> = (0..2000).map(|i| format!("module::test_{i}")).collect();
        for part in all(4) {
            let size = part.plan(&names).tests.len();
            assert!((350..=650).contains(&size), "{part}: {size} of 2000");
        }
    }

    #[test]
    fn a_new_name_moves_no_other_name() {
        let names: Vec<String> = (0..50).map(|i| format!("t{i}")).collect();
        let mut more = names.clone();
        more.push("a_new_test".to_owned());
        for part in all(3) {
            let after: Vec<&str> = part
                .plan(&more)
                .tests
                .into_iter()
                .filter(|n| *n != "a_new_test")
                .collect();
            assert_eq!(part.plan(&names).tests, after, "{part}");
        }
    }

    #[test]
    fn only_part_one_runs_the_doc_tests() {
        let names: Vec<String> = ["a::b", "x.rs - f (line 1)", "x.rs - g (line 9)"]
            .map(str::to_owned)
            .to_vec();
        for part in all(3) {
            let plan = part.plan(&names);
            assert!(plan.tests.iter().all(|n| !is_doc(n)), "{plan:?}");
            assert_eq!(plan.doc, if part.index == 1 { 2 } else { 0 });
        }
    }

    #[test]
    fn names_reads_only_the_lines_of_tests() {
        let list = "\n2 tests, 0 benchmarks\nb::two: test\na::one: test\nb::two: test\n\
                    : test\nx: benchmark\nRunning a line of cargo\n";
        assert_eq!(names(list), ["a::one", "b::two"]);
    }
}
