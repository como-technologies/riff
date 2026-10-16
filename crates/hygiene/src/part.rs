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
//! 2. [`names`] reads the names, and [`Part::select`] keeps each name
//!    whose [`fnv1a`] hash modulo N is I − 1.
//! 3. `cargo test --workspace -- --exact NAME...` runs these tests.
//!
//! A name is in one part only, so the N parts together run each test. A
//! name that two test binaries share runs in each of them, as in a full
//! run. The hash of a name does not change with the other names, so a
//! new test moves no other test to another part.
//!
//! ```mermaid
//! flowchart LR
//!     L["cargo test -- --list:<br/>each name"] --> H{"fnv1a(name) mod N"}
//!     H -- "I − 1" --> R["cargo test -- --exact<br/>the names of part I"]
//!     H -- "another part" --> S["the job of that part"]
//! ```
//!
//! ```
//! use hygiene::part::{names, Part};
//!
//! let list = "cli::help: test\n\
//!             src/lib.rs - ulid (line 57): test\n\
//!             bench::fast: benchmark\n";
//! let all = names(list);
//! assert_eq!(all, ["cli::help", "src/lib.rs - ulid (line 57)"]);
//!
//! let parts: Vec<Part> = ["1/2", "2/2"].iter().map(|p| p.parse().unwrap()).collect();
//! let mut each: Vec<&str> = parts.iter().flat_map(|p| p.select(&all)).collect();
//! each.sort();
//! assert_eq!(each, all);
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

impl Part {
    /// True when the test `name` is in this part.
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

    /// The names of `names` that are in this part, in their order.
    pub fn select<'a>(&self, names: &'a [String]) -> Vec<&'a str> {
        names
            .iter()
            .map(String::as_str)
            .filter(|name| self.has(name))
            .collect()
    }
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

/// The line that `just test-part` prints before it runs the tests.
///
/// ```
/// use hygiene::part::{summary, Part};
///
/// assert_eq!(
///     summary(Part { index: 2, count: 4 }, 731, 2903),
///     "just test-part 2/4 runs 731 of 2903 tests"
/// );
/// ```
pub fn summary(part: Part, mine: usize, all: usize) -> String {
    format!("just test-part {part} runs {mine} of {all} tests")
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
            let size = part.select(&names).len();
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
                .select(&more)
                .into_iter()
                .filter(|n| *n != "a_new_test")
                .collect();
            assert_eq!(part.select(&names), after, "{part}");
        }
    }

    #[test]
    fn names_reads_only_the_lines_of_tests() {
        let list = "\n2 tests, 0 benchmarks\nb::two: test\na::one: test\nb::two: test\n\
                    : test\nx: benchmark\nRunning a line of cargo\n";
        assert_eq!(names(list), ["a::one", "b::two"]);
    }
}
