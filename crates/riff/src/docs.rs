//! The docs check of each item: the book stays current
//! (01M4C4WQ9K6ZC85K24QFXJAZ2W).
//!
//! # Design
//!
//! The `Done when:` line of each issue has one criterion with the fixed
//! label [`CRITERION`]. A fixed label lets riff find it with no model.
//! Two commands use it:
//!
//! - `riff verify pass N` reads the issue of pull request N. It refuses
//!   when the issue has no [`CRITERION`] ([`criterion`]), or when the
//!   result has no line [`CHECKED`] with what the verifier checked
//!   ([`checked`]). Then it makes no comment, no status and no post
//!   (01M4C4WQHF7PRFHZJ9CNS847KX). `riff verify fail` takes a result
//!   with no such line: a fail can stop early.
//! - `riff plan check` lists each open item of an open wave with no
//!   [`CRITERION`] ([`missing`], 01M4C4WQW5X7ZRES1KXH7KXJSY). The lead
//!   runs it when it places an item.
//!
//! ```mermaid
//! flowchart LR
//!     I[issue: Done when with - Docs:] --> P{riff verify pass}
//!     R[result file with Docs:] --> P
//!     P -->|both| G[the check of the Gate, the comment, the status]
//!     P -->|one is missing| X[refused: nothing is reported]
//!     W[open waves] --> C[riff plan check] --> L[each item with no - Docs:]
//! ```

use anyhow::Result;
use serde::Deserialize;

use crate::pr::Gh;
use crate::top::wave_number;

/// The label of the docs criterion in the `Done when:` line of an issue.
pub const CRITERION: &str = "- Docs:";

/// The label of the docs line of the result of a verify.
pub const CHECKED: &str = "Docs:";

/// True when `body` has a `Done when:` line, and after it a criterion
/// that starts with [`CRITERION`].
///
/// ```
/// use riff::docs::criterion;
///
/// assert!(criterion("Text.\n\nDone when:\n\n- A test.\n- Docs: the book has a how-to.\n"));
/// assert!(criterion("## Done when:\r\n  - Docs: the book.\r\n"));
/// assert!(!criterion("Done when:\n\n- A test.\n"));
/// // The label counts only in the Done when part.
/// assert!(!criterion("- Docs: the book.\n\nDone when:\n- A test.\n"));
/// assert!(!criterion("Done when:\n- The docs: the book.\n"));
/// ```
pub fn criterion(body: &str) -> bool {
    body.lines()
        .skip_while(|line| !line.contains("Done when:"))
        .skip(1)
        .any(|line| line.trim_start().starts_with(CRITERION))
}

/// True when the result of a verify has a line that starts with
/// [`CHECKED`], after the marks of a list, a heading or bold text.
///
/// ```
/// use riff::docs::checked;
///
/// assert!(checked("1. A test: pass.\nDocs: the how-to of riff plan check.\n"));
/// assert!(checked("- **Docs:** pass, the how-to has an sh block.\n"));
/// assert!(checked("## Docs: pass\n"));
/// assert!(!checked("1. A test: pass.\n"));
/// assert!(!checked("The Docs: line is missing.\n"));
/// ```
pub fn checked(result: &str) -> bool {
    result.lines().any(|line| {
        line.trim_start_matches(|c: char| c.is_whitespace() || "-*#>".contains(c))
            .replace("**", "")
            .starts_with(CHECKED)
    })
}

/// An open issue, as `gh issue list --json number,title,body,milestone`
/// gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    pub number: u64,
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub milestone: Option<Milestone>,
}

/// The milestone of an issue.
#[derive(Debug, Clone, Deserialize)]
pub struct Milestone {
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub title: String,
}

/// The items of `open` that are in a wave and have no [`criterion`], in
/// the order of the wave, then of the number. `open` holds only open
/// issues, and an issue is in an open wave when it is open: GitHub
/// closes a milestone only with no open issue.
///
/// ```
/// use riff::docs::{Item, Milestone, missing};
///
/// let item = |number, wave: Option<&str>, body: &str| Item {
///     number,
///     title: format!("Item {number}"),
///     body: body.into(),
///     milestone: wave.map(|t| Milestone { title: t.into() }),
/// };
/// let docs = "Done when:\n- A test.\n- Docs: the book.\n";
/// let open = [
///     item(12, Some("Wave 10: Sandbox"), "Done when:\n- A test.\n"),
///     item(7, Some("Wave 9"), "No criteria."),
///     item(8, Some("Wave 9"), docs),
///     item(3, Some("Backlog"), "Out of the waves."),
///     item(4, None, "No wave."),
/// ];
/// let numbers: Vec<u64> = missing(&open).iter().map(|i| i.number).collect();
/// assert_eq!(numbers, [7, 12]);
/// ```
pub fn missing(open: &[Item]) -> Vec<&Item> {
    let mut items: Vec<(u64, &Item)> = open
        .iter()
        .filter(|item| !criterion(&item.body))
        .filter_map(|item| Some((wave_number(&item.milestone.as_ref()?.title)?, item)))
        .collect();
    items.sort_by_key(|(wave, item)| (*wave, item.number));
    items.into_iter().map(|(_, item)| item).collect()
}

/// The open issues of `repo` (`OWNER/REPO`), with `gh`.
pub fn open_items(gh: &Gh, repo: &str) -> Result<Vec<Item>> {
    gh.json(&[
        "issue",
        "list",
        "--repo",
        repo,
        "--state",
        "open",
        "--limit",
        "1000",
        "--json",
        "number,title,body,milestone",
    ])
}

/// The body of issue `number` of `repo`, with `gh`.
pub fn issue_body(gh: &Gh, repo: &str, number: u64) -> Result<String> {
    #[derive(Deserialize)]
    struct Body {
        body: String,
    }
    let issue: Body = gh.json(&[
        "issue",
        "view",
        &number.to_string(),
        "--repo",
        repo,
        "--json",
        "body",
    ])?;
    Ok(issue.body)
}
