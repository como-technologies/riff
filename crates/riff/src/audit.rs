//! `riff audit`: proves from the log and from the forge that a wave
//! followed the rules (01M3ZWRC6H0P7EF6CMECCYZ2RC).
//!
//! # Design
//!
//! The command reads two sources, and changes nothing:
//!
//! - The records of the repository, from `riff-server` (`POST /v1/log`).
//!   Only the owner and the admins can read them
//!   (01M3ZWRC11R5M9V1KTF05P240W). A post has only its mark: `request`,
//!   `verify request`, `verify result` or none
//!   (01M3ZWRC3XBFN8FJDGE8XWZ5EA).
//! - The waves, the issues and the pull requests, from `gh`
//!   ([`Forge::read`], 01M3ZWRC9F7TGSHB966TPVVS9Q).
//!
//! [`audit`] replays the records in log order, and checks each rule at
//! each record. The span of a wave starts when the wave became the
//! current wave: at its start, or at the close of the last wave before
//! it. It ends at the close of the wave, or now. Rule 1 looks at each
//! item of the wave in the whole log. Rules 2 to 7 look at each record
//! in the span.
//!
//! ```mermaid
//! flowchart LR
//!     S[riff-server: POST /v1/log] -->|records of the repository| A[audit]
//!     G[gh: milestones, issues, pull requests, statuses] -->|Forge| A
//!     A --> R[Report: each rule with pass, fail or not checked]
//! ```
//!
//! | Rule | It checks | From |
//! |---|---|---|
//! | 1 | Each item has a claim, a pull request, a verify by another session, a merge and a release. No release before the merge or a verify result, except the release of a worker after its verify request (01M3ZWRCC46EHYKQ4Q2NZS9TEB). | each item of the wave |
//! | 2 | The verifier held no other claim, and is not the author. | each `verify-issue-N` claim |
//! | 3 | A worker had a clear (a `session_started` with the reason `clear` or `process`) between its last release and its next claim. | each claim |
//! | 4 | No claim while the riff or the repository was paused. | each claim |
//! | 5 | No claim of an item of a later wave, and no claim of an item with an open need. | each `issue-N` claim |
//! | 6 | The lead held no claim of a work item. | each claim of `issue-N` or `verify-issue-N` |
//! | 7 | Each request came from the lead of the user of its sender, to a session of that user. | each post with the mark `request` |
//!
//! A rule with nothing to check in the span is "not checked", with the
//! reason. So is a record that the facts cannot prove, for example a
//! claim of an item that is in no wave.
//!
//! # Example
//!
//! ```
//! use riff::audit::{Forge, Issue, Pull, Verdict, Wave, audit};
//! use riff_core::name::Who;
//! use riff_core::record::{By, Change, Claimed, Record, Released};
//!
//! let ann = "riff://ann@heron/acme/app?session=s1".parse()?;
//! let bob = "riff://bob@heron/acme/app?session=s2".parse()?;
//! let record = |position, session: &riff_core::name::SessionUri, change| Record {
//!     position,
//!     written_at_ms: position * 1000,
//!     by: Some(By::Session(session.who().clone())),
//!     command: None,
//!     change,
//! };
//! let claim = |session: &riff_core::name::SessionUri, item: &str| Claimed {
//!     session: session.clone(),
//!     thread: "acme/app".parse().unwrap(),
//!     item: item.into(),
//! };
//! let records = [
//!     record(1, &ann, Change::Claimed(claim(&ann, "issue-7"))),
//!     record(2, &bob, Change::Claimed(claim(&bob, "verify-issue-7"))),
//!     record(3, &bob, Change::Released(Released::of(claim(&bob, "verify-issue-7")))),
//!     record(4, &ann, Change::Released(Released::of(claim(&ann, "issue-7")))),
//! ];
//! let forge = Forge {
//!     waves: vec![Wave { title: "Wave 1".into(), number: 1, created_ms: 0, closed_ms: None }],
//!     issues: [(7, Issue { wave: Some("Wave 1".into()), needs: vec![], merged_ms: Some(3500) })].into(),
//!     pulls: vec![Pull { number: 40, issue: 7, merged_ms: Some(3500), verified: true }],
//! };
//! let report = audit("Wave 1", &records, &forge, 10_000)?;
//! assert_eq!(report.rules[0].verdict(), Verdict::Pass);
//! assert_eq!(report.rules[1].verdict(), Verdict::Pass);
//! // No session posted a request in the span.
//! assert_eq!(report.rules[6].verdict(), Verdict::NotChecked);
//! assert!(!report.failed());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use anyhow::{Result, bail};
use riff_core::name::{Place, ThreadName, Who};
use riff_core::record::{By, Change, Posted, Record, Scope};
use riff_core::wire::{RiffState, StartReason};
use serde::Deserialize;

use crate::pr::Gh;
use crate::rollout;
use crate::top::wave_number;

/// A wave of the forge: a milestone `Wave N`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wave {
    pub title: String,
    pub number: u64,
    /// When the wave was made, in milliseconds since the Unix epoch.
    pub created_ms: u64,
    /// When the wave was closed. `None` while it is open.
    pub closed_ms: Option<u64>,
}

/// An issue of the forge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Issue {
    /// The title of its milestone.
    pub wave: Option<String>,
    /// The issues of its `Needs:` line.
    pub needs: Vec<u64>,
    /// When it was merged: its close, or its first comment
    /// `Merged in #PR (COMMIT)` (R215), the earlier one.
    pub merged_ms: Option<u64>,
}

/// A pull request of an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pull {
    pub number: u64,
    /// The issue of its `Issue:` trailer.
    pub issue: u64,
    /// When the forge merged it. `None` while it is not merged.
    pub merged_ms: Option<u64>,
    /// True when the last status `riff/verify` of its head is `success`.
    pub verified: bool,
}

/// The facts of the forge for an audit.
#[derive(Clone, Debug, Default)]
pub struct Forge {
    /// Each wave, open and closed.
    pub waves: Vec<Wave>,
    /// Each issue, by its number.
    pub issues: BTreeMap<u64, Issue>,
    /// Each pull request of the wave.
    pub pulls: Vec<Pull>,
}

/// The result of a rule, or of one check of a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    NotChecked,
}

impl Verdict {
    /// The word for people.
    pub fn word(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Fail => "fail",
            Verdict::NotChecked => "not checked",
        }
    }
}

/// One check of a rule: its verdict, and the records that show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub verdict: Verdict,
    pub text: String,
}

/// A rule, with each of its checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// 1 to 7.
    pub number: usize,
    pub findings: Vec<Finding>,
    /// Why the rule is not checked when it has no check.
    pub none: &'static str,
}

/// The text of each rule, in order.
pub const RULES: [&str; 7] = [
    "Each item has a claim, a pull request, a verify by another session, a merge and a release. \
     No release comes before the merge or a verify result.",
    "Each verifier held no other claim, and is not the author.",
    "Each worker had a clear between its last release and its next claim.",
    "No claim while the riff or the repository was paused.",
    "No claim of an item of a later wave, and no claim of an item with an open need.",
    "The lead held no claim of a work item.",
    "Each request came from the lead of the user of its sender.",
];

impl Rule {
    fn new(number: usize, none: &'static str) -> Rule {
        Rule {
            number,
            findings: Vec::new(),
            none,
        }
    }

    fn add(&mut self, verdict: Verdict, text: String) {
        self.findings.push(Finding { verdict, text });
    }

    /// The text of the rule.
    pub fn text(&self) -> &'static str {
        RULES[self.number - 1]
    }

    /// `Fail` when a check fails. Else `Pass` when a check passes. Else
    /// `NotChecked`.
    pub fn verdict(&self) -> Verdict {
        let has = |verdict| self.findings.iter().any(|f| f.verdict == verdict);
        if has(Verdict::Fail) {
            Verdict::Fail
        } else if has(Verdict::Pass) {
            Verdict::Pass
        } else {
            Verdict::NotChecked
        }
    }
}

/// The result of an audit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// The title of the wave.
    pub wave: String,
    /// The start of the span, in milliseconds since the Unix epoch.
    pub from_ms: u64,
    /// The end of the span: the close of the wave. `None` while the wave
    /// is open: the span ends now.
    pub to_ms: Option<u64>,
    /// The rules 1 to 7, in order.
    pub rules: Vec<Rule>,
}

impl Report {
    /// True when a rule fails.
    pub fn failed(&self) -> bool {
        self.rules.iter().any(|r| r.verdict() == Verdict::Fail)
    }
}

/// The name of a session for people: the user and the first 8
/// characters of the session ID.
fn name(who: &Who) -> String {
    match who.session() {
        Some(id) => format!("{}/{}", who.user(), id.chars().take(8).collect::<String>()),
        None => who.user().to_owned(),
    }
}

/// The issue of a claim `issue-N`. `None` for each other claim, also for
/// `verify-issue-N`.
fn work_item(item: &str) -> Option<u64> {
    item.strip_prefix("issue-")?.parse().ok()
}

/// The issue of a claim `verify-issue-N`.
fn verify_item(item: &str) -> Option<u64> {
    work_item(item.strip_prefix("verify-")?)
}

/// The span of the wave `title`: from the time when it became the
/// current wave to its close (01M3ZWRC9F7TGSHB966TPVVS9Q).
fn span<'a>(title: &str, waves: &'a [Wave]) -> Result<(&'a Wave, u64)> {
    let Some(wave) = waves.iter().find(|w| w.title == title) else {
        bail!("the repository has no wave {title}");
    };
    let mut from = wave.created_ms;
    for earlier in waves.iter().filter(|w| w.number < wave.number) {
        match earlier.closed_ms {
            Some(closed) => from = from.max(closed),
            None => bail!(
                "{title} has not started: {} is open. The audit checks a wave from the time when it \
                 is the current wave.",
                earlier.title
            ),
        }
    }
    Ok((wave, from))
}

/// The number of the current wave at `at_ms`: the open wave with the
/// lowest number at that time.
fn current_wave(waves: &[Wave], at_ms: u64) -> Option<u64> {
    waves
        .iter()
        .filter(|w| w.created_ms <= at_ms && w.closed_ms.is_none_or(|closed| closed > at_ms))
        .map(|w| w.number)
        .min()
}

/// What the replay knows at a record.
#[derive(Default)]
struct Replay {
    /// The holder of each claim.
    holders: BTreeMap<String, Who>,
    /// The lead of each user in the repository.
    leads: BTreeMap<String, Who>,
    /// The worker mark of the last start of each session.
    workers: BTreeMap<Who, bool>,
    /// The position of the last release of a worker that must clear,
    /// until its next clear.
    must_clear: BTreeMap<Who, u64>,
    /// True for a session that posted a verify request since its last
    /// claim.
    requested: BTreeSet<Who>,
    /// The issues with a verify result.
    results: BTreeSet<u64>,
    /// The pause of the riff and of the repository.
    riff_paused: bool,
    repo_paused: bool,
}

/// What rule 1 finds for an item in the whole log.
#[derive(Default)]
struct Item {
    /// The position and the holder of each claim `issue-N`.
    claims: Vec<(u64, Who)>,
    /// The position and the holder of each claim `verify-issue-N`.
    verifies: Vec<(u64, Who)>,
    /// The position of each release of `issue-N` by its holder.
    releases: Vec<u64>,
    /// Each release before the merge, with no verify result and no
    /// verify request of a worker before it.
    early: Vec<u64>,
}

/// Checks each rule for the wave `title` (01M3ZWRC6H0P7EF6CMECCYZ2RC).
/// `records` are the records of the repository in log order. `now_ms`
/// ends the span of an open wave. It fails when the forge has no such
/// wave, or when an earlier wave is open.
pub fn audit(title: &str, records: &[Record], forge: &Forge, now_ms: u64) -> Result<Report> {
    let (wave, from_ms) = span(title, &forge.waves)?;
    let end_ms = wave.closed_ms.unwrap_or(now_ms);
    let in_span = |record: &Record| (from_ms..=end_ms).contains(&record.written_at_ms);

    let mut rules: Vec<Rule> = [
        "the wave has no item",
        "no session claimed a verify in the span",
        "no session claimed in the span",
        "no session claimed in the span",
        "no session claimed an issue in the span",
        "no session claimed a work item in the span",
        "no session posted a request in the span",
    ]
    .into_iter()
    .enumerate()
    .map(|(i, none)| Rule::new(i + 1, none))
    .collect();

    let wave_items: BTreeSet<u64> = forge
        .issues
        .iter()
        .filter(|(_, issue)| issue.wave.as_deref() == Some(title))
        .map(|(n, _)| *n)
        .collect();
    let merged_of = |n: u64| {
        forge
            .pulls
            .iter()
            .filter(|p| p.issue == n)
            .filter_map(|p| p.merged_ms)
            .min()
    };
    // The authors of each issue, in the whole log: a verifier that
    // claims the item later is an author too.
    let mut authors: BTreeMap<u64, BTreeSet<Who>> = BTreeMap::new();
    for record in records {
        if let Change::Claimed(c) = &record.change
            && let Some(n) = work_item(&c.item)
        {
            authors
                .entry(n)
                .or_default()
                .insert(c.session.who().clone());
        }
    }

    let mut items: BTreeMap<u64, Item> = BTreeMap::new();
    let mut state = Replay::default();
    for record in records {
        let at = record.written_at_ms;
        let checked = in_span(record);
        match &record.change {
            Change::Claimed(c) => {
                let who = c.session.who();
                if checked {
                    check_claim(&mut rules, &state, forge, record, who, &c.item, &authors);
                }
                if let Some(n) = work_item(&c.item) {
                    items
                        .entry(n)
                        .or_default()
                        .claims
                        .push((record.position, who.clone()));
                }
                if let Some(n) = verify_item(&c.item) {
                    items
                        .entry(n)
                        .or_default()
                        .verifies
                        .push((record.position, who.clone()));
                }
                state.holders.insert(c.item.clone(), who.clone());
                state.requested.remove(who);
            }
            Change::Released(r) => {
                let who = r.session.who();
                let by_holder = match &record.by {
                    Some(By::Session(by)) => by == who,
                    None => true,
                    Some(_) => false,
                };
                if let Some(n) = work_item(&r.item)
                    && by_holder
                {
                    let item = items.entry(n).or_default();
                    item.releases.push(record.position);
                    let merged = merged_of(n).is_some_and(|merged| merged <= at);
                    let worker_asked = state.workers.get(who).copied().unwrap_or(false)
                        && state.requested.contains(who);
                    if !(merged || state.results.contains(&n) || worker_asked) {
                        item.early.push(record.position);
                    }
                }
                if state.holders.get(&r.item) == Some(who) {
                    state.holders.remove(&r.item);
                }
                if r.must_clear {
                    state.must_clear.insert(who.clone(), record.position);
                }
            }
            Change::SessionStarted(s) => {
                let who = s.session.who();
                state.workers.insert(who.clone(), s.worker);
                if matches!(s.reason, StartReason::Clear | StartReason::Process) {
                    state.must_clear.remove(who);
                }
            }
            Change::SessionForgotten(f) => {
                let who = f.session.who();
                state.holders.retain(|_, holder| holder != who);
                state.leads.retain(|_, lead| lead != who);
            }
            Change::LeadSet(m) => {
                let who = m.session.who();
                state.leads.insert(who.user().to_owned(), who.clone());
            }
            Change::LeftThread(m) => {
                let who = m.session.who();
                state.leads.retain(|_, lead| lead != who);
            }
            Change::PauseSet(set) => {
                let paused = set.state == RiffState::Paused;
                match set.scope {
                    Scope::Riff => state.riff_paused = paused,
                    Scope::Repository(_) => state.repo_paused = paused,
                    Scope::Other => {}
                }
            }
            Change::Posted(posted) => {
                let from = posted.message.from.who();
                match posted.message.body.as_str() {
                    "verify request" => {
                        state.requested.insert(from.clone());
                    }
                    "verify result" => {
                        for selector in &posted.message.to {
                            if let Some(n) = selector.claim.as_deref().and_then(work_item) {
                                state.results.insert(n);
                            }
                        }
                    }
                    "request" if checked => check_request(&mut rules[6], &state, record, posted),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    for n in &wave_items {
        let item = items.remove(n).unwrap_or_default();
        let (verdict, text) = item_flow(*n, &item, forge);
        rules[0].add(verdict, text);
    }

    Ok(Report {
        wave: title.to_owned(),
        from_ms,
        to_ms: wave.closed_ms,
        rules,
    })
}

/// Rule 1 for the item `n`: its verdict and its text.
fn item_flow(n: u64, item: &Item, forge: &Forge) -> (Verdict, String) {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    match item.claims.first() {
        Some((position, who)) => found.push(format!("claim at record {position} by {}", name(who))),
        None => missing.push("no claim".to_owned()),
    }
    let pulls: Vec<&Pull> = forge.pulls.iter().filter(|p| p.issue == n).collect();
    let pull = pulls
        .iter()
        .find(|p| p.merged_ms.is_some())
        .or(pulls.first());
    match pull {
        Some(pull) => found.push(format!("PR #{}", pull.number)),
        None => missing.push("no pull request".to_owned()),
    }
    let authors: BTreeSet<&Who> = item.claims.iter().map(|(_, who)| who).collect();
    let verifier = item.verifies.iter().find(|(_, who)| !authors.contains(who));
    match (pull, verifier) {
        (Some(pull), Some((position, who))) if pull.verified => found.push(format!(
            "verify claim at record {position} by {}",
            name(who)
        )),
        (Some(pull), _) if !pull.verified => {
            missing.push(format!(
                "no riff/verify success on the head of PR #{}",
                pull.number
            ));
        }
        (Some(_), _) => missing.push("no verify claim by a session that is not the author".into()),
        (None, _) => {}
    }
    match pull.and_then(|p| p.merged_ms) {
        Some(_) => found.push("merged".to_owned()),
        None if pull.is_some() => missing.push("not merged".to_owned()),
        None => {}
    }
    match item.releases.last() {
        Some(position) => found.push(format!("release at record {position}")),
        None => missing.push("no release".to_owned()),
    }
    for position in &item.early {
        missing.push(format!(
            "release at record {position} before the merge and before a verify result"
        ));
    }
    if missing.is_empty() {
        (Verdict::Pass, format!("issue-{n}: {}.", found.join(", ")))
    } else {
        (Verdict::Fail, format!("issue-{n}: {}.", missing.join("; ")))
    }
}

/// The rules 2 to 6 at a claim in the span.
fn check_claim(
    rules: &mut [Rule],
    state: &Replay,
    forge: &Forge,
    record: &Record,
    who: &Who,
    item: &str,
    authors: &BTreeMap<u64, BTreeSet<Who>>,
) {
    let at = record.written_at_ms;
    let position = record.position;
    let claim = format!("{item} at record {position} by {}", name(who));

    // Rule 2: the verifier.
    if let Some(n) = verify_item(item) {
        let held: Vec<&str> = state
            .holders
            .iter()
            .filter(|(other, holder)| *holder == who && other.as_str() != item)
            .map(|(other, _)| other.as_str())
            .collect();
        let author = authors.get(&n).is_some_and(|a| a.contains(who));
        let mut why = Vec::new();
        if !held.is_empty() {
            why.push(format!("it held {}", held.join(", ")));
        }
        if author {
            why.push(format!("it is an author of issue-{n}"));
        }
        if why.is_empty() {
            rules[1].add(Verdict::Pass, claim.clone());
        } else {
            rules[1].add(Verdict::Fail, format!("{claim}: {}.", why.join("; ")));
        }
    }

    // Rule 3: a clear between the last release of a worker and its
    // next claim.
    match state.must_clear.get(who) {
        Some(release) => rules[2].add(
            Verdict::Fail,
            format!("{claim}: no clear after its last release at record {release}."),
        ),
        None => rules[2].add(Verdict::Pass, claim.clone()),
    }

    // Rule 4: no claim while paused.
    if state.riff_paused || state.repo_paused {
        let what = if state.riff_paused {
            "the riff"
        } else {
            "the repository"
        };
        rules[3].add(Verdict::Fail, format!("{claim}: {what} was paused."));
    } else {
        rules[3].add(Verdict::Pass, claim.clone());
    }

    // Rule 5: the wave and the needs of an issue.
    if let Some(n) = work_item(item) {
        let (verdict, text) = wave_and_needs(n, at, forge);
        let text = match text {
            Some(text) => format!("{claim}: {text}."),
            None => claim.clone(),
        };
        rules[4].add(verdict, text);
    }

    // Rule 6: the lead holds no work item.
    if work_item(item).is_some() || verify_item(item).is_some() {
        if state.leads.get(who.user()) == Some(who) {
            rules[5].add(
                Verdict::Fail,
                format!("{claim}: it was the lead of {}.", who.user()),
            );
        } else {
            rules[5].add(Verdict::Pass, claim);
        }
    }
}

/// Rule 5 for a claim of the issue `n` at `at`: its verdict, and why
/// when it does not pass.
fn wave_and_needs(n: u64, at: u64, forge: &Forge) -> (Verdict, Option<String>) {
    let Some(issue) = forge.issues.get(&n) else {
        return (
            Verdict::NotChecked,
            Some(format!("the forge has no issue #{n}")),
        );
    };
    let Some(number) = issue.wave.as_deref().and_then(wave_number) else {
        return (
            Verdict::NotChecked,
            Some(format!("issue #{n} is in no wave")),
        );
    };
    let mut why = Vec::new();
    match current_wave(&forge.waves, at) {
        Some(current) if number > current => {
            why.push(format!(
                "it is in Wave {number}, and Wave {current} was open"
            ));
        }
        Some(_) => {}
        None => {
            return (
                Verdict::NotChecked,
                Some("no wave was open at the claim".into()),
            );
        }
    }
    for need in &issue.needs {
        match forge.issues.get(need) {
            Some(needed) if needed.merged_ms.is_some_and(|merged| merged <= at) => {}
            Some(_) => why.push(format!("its need #{need} was open")),
            None => why.push(format!("its need #{need} is not an issue of the forge")),
        }
    }
    if why.is_empty() {
        (Verdict::Pass, None)
    } else {
        (Verdict::Fail, Some(why.join("; ")))
    }
}

/// Rule 7 at a post with the mark `request` in the span.
fn check_request(rule: &mut Rule, state: &Replay, record: &Record, posted: &Posted) {
    let from = posted.message.from.who();
    let request = format!("request at record {} from {}", record.position, name(from));
    let mut why = Vec::new();
    match state.leads.get(from.user()) {
        Some(lead) if lead == from => {}
        Some(lead) => why.push(format!("the lead of {} was {}", from.user(), name(lead))),
        None => why.push(format!("{} had no lead", from.user())),
    }
    if let Some(to) = posted.thread.peer(from)
        && to.user() != from.user()
    {
        why.push(format!(
            "it went to {}, a session of another user",
            name(&to)
        ));
    }
    if why.is_empty() {
        rule.add(Verdict::Pass, request);
    } else {
        rule.add(Verdict::Fail, format!("{request}: {}.", why.join("; ")));
    }
}

/// A time for people: `2026-10-02 19:37 UTC`.
fn time(ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(i64::try_from(ms).unwrap_or(i64::MAX)).map_or_else(
        || ms.to_string(),
        |t| t.format("%Y-%m-%d %H:%M UTC").to_string(),
    )
}

/// The report for people. Rule 1 has a line for each item. Each other
/// rule has the number of its checks that pass, and a line for each
/// check that fails or is not checked.
///
/// ```
/// use riff::audit::{Report, Rule, render};
///
/// let report = Report { wave: "Wave 1".into(), from_ms: 0, to_ms: None, rules: vec![] };
/// assert_eq!(
///     render(&report, "acme/app"),
///     "Audit of Wave 1 in acme/app, from 1970-01-01 00:00 UTC to now.\n\nResult: each rule passes.\n"
/// );
/// ```
pub fn render(report: &Report, repo: &str) -> String {
    let to = report.to_ms.map_or_else(|| "now".to_owned(), time);
    let mut out = format!(
        "Audit of {} in {repo}, from {} to {to}.\n",
        report.wave,
        time(report.from_ms)
    );
    for rule in &report.rules {
        let verdict = rule.verdict();
        let _ = writeln!(out, "\nRule {}: {}", rule.number, verdict.word());
        let _ = writeln!(out, "  {}", rule.text());
        if rule.findings.is_empty() {
            let _ = writeln!(out, "  Not checked: {}.", rule.none);
            continue;
        }
        let passed = rule
            .findings
            .iter()
            .filter(|f| f.verdict == Verdict::Pass)
            .count();
        if rule.number > 1 && passed > 0 {
            let _ = writeln!(out, "  {passed} checks pass.");
        }
        for finding in &rule.findings {
            if rule.number > 1 && finding.verdict == Verdict::Pass {
                continue;
            }
            let _ = writeln!(out, "  {}: {}", finding.verdict.word(), finding.text);
        }
    }
    let failed: Vec<String> = report
        .rules
        .iter()
        .filter(|r| r.verdict() == Verdict::Fail)
        .map(|r| r.number.to_string())
        .collect();
    let not_checked = report
        .rules
        .iter()
        .filter(|r| r.verdict() == Verdict::NotChecked)
        .count();
    let result = match (failed.as_slice(), not_checked) {
        ([], 0) => "each rule passes.".to_owned(),
        ([], n) => format!("no rule fails. {n} rules are not checked."),
        ([one], _) => format!("rule {one} fails."),
        (many, _) => format!("the rules {} fail.", many.join(", ")),
    };
    let _ = writeln!(out, "\nResult: {result}");
    out
}

/// A milestone in the reply of `gh api .../milestones`.
#[derive(Deserialize)]
struct GhMilestone {
    title: String,
    created_at: String,
    closed_at: Option<String>,
}

/// An issue in the reply of `gh issue list --json`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhIssue {
    number: u64,
    milestone: Option<GhTitle>,
    #[serde(default)]
    body: String,
    closed_at: Option<String>,
    #[serde(default)]
    comments: Vec<GhComment>,
}

#[derive(Deserialize)]
struct GhTitle {
    title: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhComment {
    #[serde(default)]
    body: String,
    created_at: String,
}

/// A pull request in the reply of `gh pr list --json`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPull {
    number: u64,
    #[serde(default)]
    body: String,
    head_ref_oid: String,
    merged_at: Option<String>,
}

/// A status in the reply of `gh api .../statuses`.
#[derive(Deserialize)]
struct GhStatus {
    context: String,
    state: String,
}

/// The milliseconds of a time of GitHub, for example
/// `2026-10-02T19:37:02Z`.
///
/// ```
/// assert_eq!(riff::audit::millis("1970-01-01T00:00:02Z").unwrap(), 2000);
/// assert!(riff::audit::millis("yesterday").is_err());
/// ```
pub fn millis(time: &str) -> Result<u64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(time)
        .map_err(|e| anyhow::anyhow!("the time {time:?} of gh does not read: {e}"))?;
    Ok(u64::try_from(parsed.timestamp_millis()).unwrap_or_default())
}

impl Forge {
    /// Reads the facts of the wave `title` of `repo` (`OWNER/REPO`)
    /// with `gh` (01M3ZWRC9F7TGSHB966TPVVS9Q): each milestone `Wave N`,
    /// each issue with its milestone, its `Needs:` line, its close and
    /// its comments `Merged in`, each pull request of the milestone, and
    /// the statuses of the head of each of them.
    pub fn read(gh: &Gh, repo: &str, title: &str) -> Result<Forge> {
        let milestones: Vec<GhMilestone> = gh.json(&[
            "api",
            &format!("repos/{repo}/milestones?state=all&per_page=100"),
        ])?;
        let mut waves = Vec::new();
        for m in milestones {
            if let Some(number) = wave_number(&m.title) {
                waves.push(Wave {
                    number,
                    created_ms: millis(&m.created_at)?,
                    closed_ms: m.closed_at.as_deref().map(millis).transpose()?,
                    title: m.title,
                });
            }
        }
        let list: Vec<GhIssue> = gh.json(&[
            "issue",
            "list",
            "--repo",
            repo,
            "--state",
            "all",
            "--limit",
            "5000",
            "--json",
            "number,milestone,body,closedAt,comments",
        ])?;
        let mut issues = BTreeMap::new();
        for issue in list {
            let mut merged = issue.closed_at.as_deref().map(millis).transpose()?;
            for comment in &issue.comments {
                if comment.body.trim_start().starts_with("Merged in #") {
                    let at = millis(&comment.created_at)?;
                    merged = Some(merged.map_or(at, |m| m.min(at)));
                }
            }
            issues.insert(
                issue.number,
                Issue {
                    wave: issue.milestone.map(|m| m.title),
                    needs: rollout::needs(&issue.body),
                    merged_ms: merged,
                },
            );
        }
        let search = format!("milestone:\"{title}\"");
        let list: Vec<GhPull> = gh.json(&[
            "pr",
            "list",
            "--repo",
            repo,
            "--state",
            "all",
            "--limit",
            "1000",
            "--search",
            &search,
            "--json",
            "number,body,headRefOid,mergedAt",
        ])?;
        let mut pulls = Vec::new();
        for pull in list {
            let Some(issue) = hygiene::trailers(&pull.body)
                .into_iter()
                .find(|(key, _)| key == "Issue")
                .and_then(|(_, value)| value.trim_start_matches('#').parse().ok())
            else {
                continue;
            };
            let statuses: Vec<GhStatus> = gh.json(&[
                "api",
                &format!(
                    "repos/{repo}/commits/{}/statuses?per_page=100",
                    pull.head_ref_oid
                ),
            ])?;
            // GitHub lists the newest status first.
            let verified = statuses
                .iter()
                .find(|s| s.context == crate::pr::VERIFY_CONTEXT)
                .is_some_and(|s| s.state == "success");
            pulls.push(Pull {
                number: pull.number,
                issue,
                merged_ms: pull.merged_at.as_deref().map(millis).transpose()?,
                verified,
            });
        }
        Ok(Forge {
            waves,
            issues,
            pulls,
        })
    }
}

/// The repository thread of `place`, for the query of the log.
///
/// ```
/// use riff_core::name::{Place, Repo};
///
/// let repo = Repo::Git { owner: "acme".into(), name: "app".into() };
/// let thread = riff::audit::repo_of(&Place::new("heron", repo, None)?)?;
/// assert_eq!(thread.to_string(), "acme/app");
/// assert!(riff::audit::repo_of(&Place::host_only("heron")?).is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn repo_of(place: &Place) -> Result<ThreadName> {
    match place.default_thread() {
        Some(thread) => Ok(thread),
        None => bail!("run riff audit in a clone of the repository of the wave"),
    }
}

/// The time now, in milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(now.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use riff_core::record::{Claimed, Member, PauseSet, Released, SessionStarted};
    use riff_core::wire::Message;

    const WAVE: &str = "Wave 2";

    fn uri(session: &str) -> riff_core::name::SessionUri {
        format!("riff://ann@heron/acme/app?session={session}")
            .parse()
            .unwrap()
    }

    /// A record at `position`, at `position` seconds, by `session`.
    fn record(position: u64, session: &str, change: Change) -> Record {
        Record {
            position,
            written_at_ms: position * 1000,
            by: Some(By::Session(uri(session).who().clone())),
            command: None,
            call: None,
            change,
        }
    }

    fn claimed(session: &str, item: &str) -> Claimed {
        Claimed {
            session: uri(session),
            thread: "acme/app".parse().unwrap(),
            item: item.into(),
        }
    }

    fn claim(position: u64, session: &str, item: &str) -> Record {
        record(position, session, Change::Claimed(claimed(session, item)))
    }

    fn release(position: u64, session: &str, item: &str, must_clear: bool) -> Record {
        let released = Released {
            must_clear,
            ..Released::of(claimed(session, item))
        };
        record(position, session, Change::Released(released))
    }

    fn started(position: u64, session: &str, reason: StartReason) -> Record {
        let started = SessionStarted {
            session: uri(session),
            reason,
            worker: true,
        };
        record(position, session, Change::SessionStarted(started))
    }

    fn lead(position: u64, session: &str) -> Record {
        let member = Member {
            session: uri(session),
            thread: "acme/app".parse().unwrap(),
        };
        record(position, session, Change::LeadSet(member))
    }

    fn post(position: u64, session: &str, to: &str, mark: &str) -> Record {
        let from = uri(session);
        let thread = ThreadName::direct(from.who(), uri(to).who());
        let posted = Posted {
            thread,
            message: Message {
                seq: position,
                from,
                to: Vec::new(),
                body: mark.into(),
                at_ms: position * 1000,
                kind: Default::default(),
                sig: None,
                payload: None,
            },
            woken: BTreeSet::new(),
        };
        record(position, session, Change::Posted(Box::new(posted)))
    }

    fn pause(position: u64, state: RiffState) -> Record {
        let set = PauseSet {
            scope: Scope::Riff,
            state,
        };
        record(position, "lead", Change::PauseSet(set))
    }

    /// Wave 1 closed at 5 s; Wave 2 is open, with issues 7 and 8. Issue
    /// 8 needs 7. Issue 9 is in Wave 3.
    fn forge() -> Forge {
        let issue = |wave: &str, needs: Vec<u64>, merged_ms| Issue {
            wave: Some(wave.into()),
            needs,
            merged_ms,
        };
        Forge {
            waves: vec![
                Wave {
                    title: "Wave 1".into(),
                    number: 1,
                    created_ms: 0,
                    closed_ms: Some(5_000),
                },
                Wave {
                    title: WAVE.into(),
                    number: 2,
                    created_ms: 1_000,
                    closed_ms: None,
                },
                Wave {
                    title: "Wave 3".into(),
                    number: 3,
                    created_ms: 1_000,
                    closed_ms: None,
                },
            ],
            issues: [
                (7, issue(WAVE, vec![], Some(30_000))),
                (8, issue(WAVE, vec![7], Some(60_000))),
                (9, issue("Wave 3", vec![], None)),
            ]
            .into(),
            pulls: vec![
                Pull {
                    number: 40,
                    issue: 7,
                    merged_ms: Some(30_000),
                    verified: true,
                },
                Pull {
                    number: 41,
                    issue: 8,
                    merged_ms: Some(60_000),
                    verified: true,
                },
            ],
        }
    }

    /// A good wave: the lead l1 asks w1 for issue 7; w1 asks for a
    /// verify and releases; w2 verifies; w1 clears and takes issue 8
    /// after the merge of 7; w2 verifies it too.
    fn good() -> Vec<Record> {
        vec![
            lead(10, "l1"),
            started(11, "w1", StartReason::Process),
            started(12, "w2", StartReason::Process),
            post(13, "l1", "w1", "request"),
            claim(14, "w1", "issue-7"),
            post(15, "w1", "l1", "verify request"),
            release(16, "w1", "issue-7", true),
            claim(17, "w2", "verify-issue-7"),
            release(18, "w2", "verify-issue-7", true),
            started(31, "w1", StartReason::Clear),
            claim(32, "w1", "issue-8"),
            post(33, "w1", "l1", "verify request"),
            release(34, "w1", "issue-8", true),
            started(35, "w2", StartReason::Clear),
            claim(36, "w2", "verify-issue-8"),
            release(37, "w2", "verify-issue-8", true),
        ]
    }

    fn verdicts(records: &[Record]) -> Vec<Verdict> {
        audit(WAVE, records, &forge(), 100_000)
            .unwrap()
            .rules
            .iter()
            .map(Rule::verdict)
            .collect()
    }

    fn fails(records: &[Record], rule: usize) -> Vec<String> {
        audit(WAVE, records, &forge(), 100_000).unwrap().rules[rule - 1]
            .findings
            .iter()
            .filter(|f| f.verdict == Verdict::Fail)
            .map(|f| f.text.clone())
            .collect()
    }

    #[test]
    fn a_good_wave_passes_each_rule() {
        assert_eq!(verdicts(&good()), [Verdict::Pass; 7]);
    }

    #[test]
    fn the_span_starts_when_the_wave_is_current() {
        let report = audit(WAVE, &good(), &forge(), 100_000).unwrap();
        assert_eq!((report.from_ms, report.to_ms), (5_000, None));
        let mut open = forge();
        open.waves[0].closed_ms = None;
        let error = audit(WAVE, &good(), &open, 100_000).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("Wave 2 has not started: Wave 1 is open.")
        );
        let error = audit("Wave 9", &good(), &forge(), 100_000).unwrap_err();
        assert_eq!(error.to_string(), "the repository has no wave Wave 9");
    }

    #[test]
    fn rule_1_fails_a_release_before_the_merge() {
        let mut records = good();
        // w1 is no worker now, and asks for no verify.
        records.retain(|r| r.position != 11 && r.position != 15);
        assert_eq!(
            fails(&records, 1),
            ["issue-7: release at record 16 before the merge and before a verify result."]
        );
    }

    #[test]
    fn rule_1_fails_an_item_with_no_verify_by_another_session() {
        let mut records = good();
        records.retain(|r| r.position != 17 && r.position != 18);
        assert_eq!(
            fails(&records, 1),
            ["issue-7: no verify claim by a session that is not the author."]
        );
        let mut forge = forge();
        forge.pulls[0].verified = false;
        forge.pulls[0].merged_ms = None;
        let report = audit(WAVE, &good(), &forge, 100_000).unwrap();
        assert_eq!(
            report.rules[0].findings[0].text,
            "issue-7: no riff/verify success on the head of PR #40; not merged."
        );
    }

    #[test]
    fn rule_2_fails_a_verifier_with_a_claim() {
        let mut records = good();
        records.insert(7, claim(17, "w2", "issue-9"));
        assert_eq!(
            fails(&records, 2)[0],
            "verify-issue-7 at record 17 by ann/w2: it held issue-9."
        );
    }

    #[test]
    fn rule_2_fails_a_verifier_that_is_the_author() {
        let mut records = good();
        records.push(claim(38, "w1", "verify-issue-8"));
        assert_eq!(
            fails(&records, 2),
            ["verify-issue-8 at record 38 by ann/w1: it is an author of issue-8."]
        );
    }

    #[test]
    fn rule_3_fails_a_claim_with_no_clear() {
        let mut records = good();
        records.retain(|r| r.position != 31);
        assert_eq!(
            fails(&records, 3),
            ["issue-8 at record 32 by ann/w1: no clear after its last release at record 16."]
        );
    }

    #[test]
    fn rule_4_fails_a_claim_while_paused() {
        let mut records = good();
        records.insert(4, pause(13, RiffState::Paused));
        records.insert(6, pause(14, RiffState::Running));
        // The pause comes before the claim of issue-7, and the resume
        // after it.
        let records: Vec<Record> = records
            .into_iter()
            .enumerate()
            .map(|(i, mut r)| {
                r.position = i as u64 + 100;
                r
            })
            .collect();
        assert_eq!(
            fails(&records, 4),
            ["issue-7 at record 105 by ann/w1: the riff was paused."]
        );
    }

    #[test]
    fn rule_5_fails_a_later_wave_and_an_open_need() {
        let mut records = good();
        records.push(claim(38, "w3", "issue-9"));
        // issue-8 before the merge of its need 7 at 30 s.
        records.push(claim(20, "w4", "issue-8"));
        records.sort_by_key(|r| r.position);
        assert_eq!(
            fails(&records, 5),
            [
                "issue-8 at record 20 by ann/w4: its need #7 was open.",
                "issue-9 at record 38 by ann/w3: it is in Wave 3, and Wave 2 was open.",
            ]
        );
    }

    #[test]
    fn rule_6_fails_a_claim_of_the_lead() {
        let mut records = good();
        records.push(claim(38, "l1", "issue-9"));
        assert_eq!(
            fails(&records, 6),
            ["issue-9 at record 38 by ann/l1: it was the lead of ann."]
        );
    }

    #[test]
    fn rule_7_fails_a_request_that_is_not_from_the_lead() {
        let mut records = good();
        records.push(post(38, "w1", "w2", "request"));
        assert_eq!(
            fails(&records, 7),
            ["request at record 38 from ann/w1: the lead of ann was ann/l1."]
        );
    }

    #[test]
    fn a_rule_with_nothing_to_check_is_not_checked() {
        let records: Vec<Record> = good()
            .into_iter()
            .filter(|r| !matches!(&r.change, Change::Posted(p) if p.message.body == "request"))
            .collect();
        let report = audit(WAVE, &records, &forge(), 100_000).unwrap();
        assert_eq!(report.rules[6].verdict(), Verdict::NotChecked);
        assert!(!report.failed());
        let text = render(&report, "acme/app");
        assert!(text.contains("Rule 7: not checked\n"), "{text}");
        assert!(text.contains("  Not checked: no session posted a request in the span.\n"));
        assert!(
            text.ends_with("Result: no rule fails. 1 rules are not checked.\n"),
            "{text}"
        );
    }

    #[test]
    fn the_report_names_each_item_and_each_failure() {
        let mut records = good();
        records.push(claim(38, "l1", "issue-9"));
        let report = audit(WAVE, &records, &forge(), 100_000).unwrap();
        let text = render(&report, "acme/app");
        assert!(
            text.contains(
                "  pass: issue-7: claim at record 14 by ann/w1, PR #40, verify claim at record 17 \
                 by ann/w2, merged, release at record 16.\n"
            ),
            "{text}"
        );
        assert!(text.contains("Rule 6: fail\n"), "{text}");
        assert!(text.contains("  4 checks pass.\n  fail: issue-9 at record 38 by ann/l1"));
        assert!(text.ends_with("Result: the rules 5, 6 fail.\n"), "{text}");
    }
}
