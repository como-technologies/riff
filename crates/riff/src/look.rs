//! The look: the pull requests that stop, the full check of `main`, and (with
//! no runner now) the facts of the forge and the blocks with no answer.
//!
//! # Who runs the look
//!
//! The riff daemon runs the look of the repository of each wave that it
//! runs, once each [`LOOK_EVERY`] (each 5 minutes while the wave has no
//! session and the look told nothing, #1549), until `riff leave`
//! (`crate::wave::watch`, 01M5QVH3MAEPPJN9PPVDX6N2EY). It reads the
//! forge with the `gh` of the person, and posts as the person: to the
//! lead, or to the session that holds an item.
//!
//! ```mermaid
//! flowchart LR
//!     W["riff join"] --> D["riff daemon"]
//!     D --> A["the loop of the wave:<br/>the work"]
//!     D --> B["the look, each minute"]
//!     B --> G["gh: the pull requests,<br/>the full check of main"]
//!     B --> C["riff-server: the claims"]
//!     B --> P["post as the person"]
//!     P --> L["the lead"]
//!     P --> H["the session that holds an item"]
//! ```
//!
//! The daemon sends the messages for a pull request that stops (see "A
//! pull request that stops"), and does the steps for a broken `main`
//! (see "When main breaks"). It keeps what it told in memory
//! ([`Watch`]). A message about `main` can come one more time after a
//! new start of the daemon. A message about a pull request cannot (see
//! "A pull request that stops"). A look that fails shows its fault on
//! stderr one time until the fault changes, and the wave goes on.
//!
//! The rest of this module has no runner now, because the `riff mcp` of a
//! session on the host is gone (#1200): [`once`] sends the facts of the
//! items, and looks at the blocks, for a lead session, and nothing calls
//! it but its tests (#1384). The server makes `waiting` from the facts of
//! the clients (01M41FZP2C4Z4J6WKRXZ5B31EH, 01M41FZP9A50CH4A2VX344DW49).
//!
//! # The blocks
//!
//! [`once`] checks the blocks of the sessions of the user. The time is
//! the setting `lead.wake` of the machine of the lead
//! ([`crate::settings::lead_wake`]):
//!
//! ```mermaid
//! sequenceDiagram
//!     participant W as blocked session
//!     participant S as riff-server
//!     participant L as the look of a lead session
//!     participant P as the person
//!     W->>S: blocked REASON, and "blocked: REASON" to the lead (wake 1)
//!     loop each look
//!         L->>S: the facts of the forge
//!         L->>S: look at the blocks, lead.wake
//!     end
//!     Note over S: lead.wake with no answer
//!     S->>L: "blocked: ... has no answer after N minutes" (wake 2)
//!     Note over S: lead.wake more with no answer
//!     S-->>L: unanswered
//!     L->>P: a desktop notification (lead.notify)
//!     Note over P: riff top: "the lead gave no answer"
//! ```
//!
//! A message that wakes the blocked session is its answer. Then the
//! next sign of work ends the block (01M41FZPT31ATXP75QW965P3JB).
//!
//! The notification holds the session, its claims and the reason, and
//! no text of a message (01M41FZQKZKW131Z8822G31T5G). It needs a
//! desktop: a display (`DISPLAY` or `WAYLAND_DISPLAY`) and
//! `notify-send`, as on GNOME. A machine with no desktop gets none, and
//! nothing fails.
//!
//! # A pull request that stops
//!
//! A pull request can stop on its way to the merge with no sign: a
//! conflict with the default branch, or a verify request that no session
//! takes. The look of the daemon sees the open pull requests, and the claims of
//! `who` ([`PullWatch`]):
//!
//! ```mermaid
//! flowchart TD
//!     L["each look"] --> C{"auto-merge on, and CONFLICTING?"}
//!     C -- yes --> H{"a session holds the item?"}
//!     H -- yes --> M1["message to that session: rebase and push"]
//!     H -- no --> M2["message to the lead"]
//!     L --> V{"waits for a verify, with no verify- claim?"}
//!     V -- "yes, for 30 minutes" --> M3["message to the lead"]
//!     V -- "no: a claim or a result" --> R["the wait starts again"]
//!     V -- "a conflict, a held verify,<br/>or a verify that waits for another merge" --> R
//! ```
//!
//! A verify that waits for the merge of another pull request is in the
//! order of the wave ([`crate::order`]): the look takes these waits from
//! the loop of the wave ([`Watch::waits`]), and the holds from riff-server
//! ([`World::holds`], 01M6WCGNN0X2DGYYM97656RQK1).
//!
//! Each message comes one time for each pull request, head commit and
//! state (01M49Q31FASDM7CG3JEGPYCZB9). The daemon keeps what it told in
//! memory ([`Watch`]), and the wait of 30 minutes starts again after a
//! new start of it.
//!
//! A wave can run on more hosts, and each host has a daemon with a watch
//! of its own. So each post has a key ([`PullNews::key`]): the pull
//! request, its head commit and the state. `riff-server` drops a post
//! with the key of a message that it keeps from the same user in the
//! thread (01M5TKG2K1AYK1KE7C2AA2W9DS, 01M5TKG2P26WXHJZ10Y9J69VFJ). So
//! the lead gets each message one time, also after a new start of a
//! daemon, while the thread keeps the message.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as the daemon of pangolin
//!     participant B as the daemon of thelio
//!     participant S as riff-server
//!     participant L as the lead
//!     A->>S: post, key pull-40-1a2b3c4d-conflict
//!     S->>L: the message
//!     B->>S: post, key pull-40-1a2b3c4d-conflict
//!     Note over S: it keeps this key: it drops the post
//! ```
//!
//! # A question for a person
//!
//! A lead that waits for the answer of a person adds the label `question`
//! to an issue ([`riff_client::waves::QUESTION`],
//! 01M6QBJA0J7PQ0J29HWM1KDVHJ). The look tells the person of the lead one
//! time in riff chat, with the title and the link of the issue
//! ([`AskWatch`], [`crate::text::wave_question`], 01M6QBJHV7PVV5893Q6RCPENJQ).
//! The line names the person first (`@mike`, [`crate::wave::asked`]), so
//! it wakes the terminals of the person, and no agent session.
//! The daemon keeps the issues that it told in memory. An issue that loses
//! the label goes out of that set, so the next time that the label goes on,
//! the person gets a new line.
//!
//! The post has a key ([`question_key`]): the issue, and the time of the
//! last event "labeled" with the label, which the look reads one time
//! for each new issue ([`question_at`]). The look of each host and a new
//! start of the daemon make the same key for the same label, and
//! `riff-server` drops the second post. A label that goes on again has a new
//! time, so it has a new key.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as the daemon of pangolin
//!     participant B as the daemon of thelio
//!     participant S as riff-server
//!     participant P as the person (riff chat)
//!     A->>S: post "@mike A lead waits ..." to name=mike, key question-12-2026-11-05T20:11:03Z
//!     S->>P: the line
//!     B->>S: post, the same key
//!     Note over S: it keeps this key: it drops the post
//! ```
//!
//! # When main breaks
//!
//! A pull request merges when the full check of its own head passed, also
//! when `main` moved after that. So two pull requests that each pass
//! their full check can break `main` together. The look of the daemon
//! reads the full check of the head of `main` ([`MainWatch`],
//! 01M51QEYKTD7ZEB0YA0C3MQTPN, 01M5R13DGMTNWPY88A3EG49WWS): each check
//! that the ruleset of the default branch requires, but `riff/verify`
//! ([`crate::repo_setup::full_check`]). A repository with no CI has none,
//! and its look gives nothing (01M5SMNKDWBD1J6Y18ENQG87CK). The look reads
//! the rules at each look, and the check runs of each check on `main`. A
//! check that ends `skipped` or `neutral` passed ([`crate::pr::is_success`]).
//!
//! ```mermaid
//! flowchart TD
//!     L["each look"] --> G{"the full check of the head of main"}
//!     G -- "runs, or no run" --> N["nothing"]
//!     G -- failed --> T{"the lead got this commit?"}
//!     T -- yes --> N
//!     T -- "no: main passed before" --> F["the fix item: file it,<br/>or comment on the open one"]
//!     F --> M["message to the lead: commit, run,<br/>failed jobs, fix item"]
//!     T -- "no: main was broken at an older commit" --> C["comment on the fix item,<br/>note to the lead"]
//!     G -- passed --> X["close each fix item that no session claims<br/>and no open pull request names"]
//!     X --> P["note to the lead"]
//! ```
//!
//! - **The fix item** ([`file_fix_item`], 01M51QEYPGD6W345MZ4F85C38F).
//!   An issue with the label [`BROKEN_MAIN`] in the current wave. riff
//!   files one only when no issue with the label is open. The rollout
//!   gives it to a worker before each other item
//!   ([`crate::rollout::free_work_of`], 01M51QEYS538XA58KDRY844T52).
//! - **When main passes again** (01M51QEYVQN74Q1VDTVHB0445Q). riff closes
//!   each free fix item: the fix came in another pull request, or a run
//!   of the failed jobs again passed. A fix item that a session claims,
//!   or that an open pull request names, stays open.
//!
//! The daemon keeps the last broken commit that it told in memory
//! ([`Watch`]), as for the pull requests. When `main` is still broken at a
//! new start of it, the daemon posts the message again, and the open fix
//! item gets a comment.
//!
//! A wave can run on more hosts, and each daemon has a watch of its own,
//! so each post about `main` has a key too ([`MainNews::key`],
//! 01M6FYH7BFFSZQ2TAJSX8A5QQV, 01M6GX8CN4ZTV0X40GD5AWPXH9): the commit,
//! the run of the check (its run number and its job number) and the
//! state, `broken` or `passes`. `riff-server` drops a post with the key
//! of a message that it keeps from the same user in the thread. So the
//! lead gets each message about `main` one time, also after a new start
//! of a daemon. A rerun makes a new job, so a commit that breaks, passes
//! on a rerun and breaks again gives the lead three messages. A message
//! that `main` passes names the check with the highest job among the
//! required checks, so a rerun of any one check gives a new key, also when
//! the first check does not run again.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as the daemon of pangolin
//!     participant B as the daemon of thelio
//!     participant S as riff-server
//!     participant L as the lead
//!     A->>S: post, key main-c60887e-38861280871-7-broken
//!     S->>L: the message
//!     B->>S: post, key main-c60887e-38861280871-7-broken
//!     Note over S: it keeps this key: it drops the post
//!     A->>S: post, key main-c60887e-38861280871-8-passes (a rerun)
//!     S->>L: the note
//!     A->>S: post, key main-c60887e-38861280871-9-broken (a second rerun)
//!     S->>L: the message
//! ```
//!
//! # The release cycle
//!
//! The look has no part in the release cycle. Each riff command that
//! starts an async operation of the cycle registers a watch, and the
//! daemon follows it ([`crate::watches`], 01M5RGCW5NCKQYA61RM63914TX).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use riff_core::name::{SessionUri, Who};
use riff_core::selector::Selector;
use riff_core::wire::{ItemFact, Kind, PullFact, PullState, Unanswered};
use serde::Deserialize;

use crate::pr::{
    CheckRun, GATE, Gate, Gh, check_runs, full_state, gate_state, is_success, newest_runs,
};
use crate::repo_setup;
use crate::rollout::{BROKEN_MAIN, Issue, Milestone, Pull, Verify, claims, current_wave, needs};
use crate::text;
use crate::wave::{Wave, World};
use riff_client::api::Api;

/// The time between two looks.
pub const LOOK_EVERY: Duration = Duration::from_secs(60);

/// The facts of the open items: the state of the pull request of each
/// item, and the open items of its `Needs:` line
/// (01M41FZP2C4Z4J6WKRXZ5B31EH). A need is met when its issue is closed,
/// or when the issue has a comment `Merged in #` (01M49HAW3NXNXNX02ETDZD3YCN).
///
/// ```
/// use riff::look::item_facts;
/// use riff::rollout::{Check, Comment, Issue, Pull};
/// use riff_core::wire::{PullFact, PullState};
///
/// let issue = |number, body: &str, comments: &[&str]| Issue {
///     number,
///     body: body.into(),
///     comments: comments.iter().map(|c| Comment { body: c.to_string() }).collect(),
///     milestone: None,
///     labels: vec![],
///     ..Issue::default()
/// };
/// let pull = |number, branch: &str, checks| Pull { number, branch: branch.into(), checks, ..Pull::default() };
/// let facts = item_facts(
///     &[
///         issue(12, "", &[]),
///         issue(13, "Needs: #12, #9, #11", &[]),
///         issue(14, "", &[]),
///         issue(11, "", &["Merged in #50 (abc)"]),
///     ],
///     &[
///         pull(40, "worktree-issue-12", vec![]),
///         pull(41, "worktree-issue-14", vec![Check::verify("SUCCESS")]),
///         pull(42, "by-hand", vec![]),
///     ],
/// );
/// assert_eq!(facts.len(), 3);
/// assert_eq!(facts[0].item, "issue-12");
/// assert_eq!(facts[0].pull, Some(PullFact { number: 40, state: PullState::Asked }));
/// // #9 is closed, and #11 is merged: only #12 is open.
/// assert_eq!((facts[1].item.as_str(), facts[1].needs.as_slice()), ("issue-13", &[12][..]));
/// assert_eq!(facts[2].pull, Some(PullFact { number: 41, state: PullState::Passed }));
/// ```
pub fn item_facts(issues: &[Issue], pulls: &[Pull]) -> Vec<ItemFact> {
    let open: HashSet<u64> = issues
        .iter()
        .filter(|i| !i.merged())
        .map(|i| i.number)
        .collect();
    let mut facts: BTreeMap<u64, ItemFact> = BTreeMap::new();
    let fact = |n: u64| ItemFact {
        item: format!("issue-{n}"),
        pull: None,
        needs: Vec::new(),
    };
    for pull in pulls {
        let (Some(n), Some(verify)) = (pull.issue(), pull.verify()) else {
            continue;
        };
        let state = match verify {
            Verify::Asked => PullState::Asked,
            Verify::Passed => PullState::Passed,
            Verify::Failed => PullState::Failed,
        };
        let number = pull.number;
        facts.entry(n).or_insert_with(|| fact(n)).pull = Some(PullFact { number, state });
    }
    for issue in issues {
        let open_needs: Vec<u64> = needs(&issue.body)
            .into_iter()
            .filter(|n| open.contains(n))
            .collect();
        if !open_needs.is_empty() {
            let n = issue.number;
            facts.entry(n).or_insert_with(|| fact(n)).needs = open_needs;
        }
    }
    facts.into_values().collect()
}

/// Reads the open issues, with their labels, and the open pull requests
/// of `repo` with `gh`.
pub fn read_forge(gh: &Gh, repo: &str) -> Result<(Vec<Issue>, Vec<Pull>)> {
    let pulls = crate::rollout::pulls(gh, repo)?;
    let issues = crate::rollout::issues(gh, repo)?;
    Ok((issues, pulls))
}

/// Sends the fact of one item, with no error: `riff pr open`, `riff
/// verify` and `riff pr wait` send it after their work, and their work
/// is done (01M41FZP2C4Z4J6WKRXZ5B31EH).
pub async fn tell_fact(api: &Api, me: &SessionUri, issue: u64, number: u64, state: PullState) {
    let fact = ItemFact {
        item: format!("issue-{issue}"),
        pull: Some(PullFact { number, state }),
        needs: Vec::new(),
    };
    let _ = api.item_facts(me, vec![fact], false).await;
}

/// The longest wait of a pull request for a verify claim before the lead
/// gets a message (01M49Q316RXNATJP587DWGDNCD).
pub const VERIFY_WAIT: Duration = Duration::from_secs(30 * 60);

/// A message of the look about a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullNews {
    /// The item whose holder gets the message, or `None` for the lead.
    pub to: Option<String>,
    pub body: String,
    /// What the message tells: the pull request, its head, and the
    /// state.
    pub told: (u64, String, Stop),
}

impl PullNews {
    /// The key of the post of the news (01M5TKG2K1AYK1KE7C2AA2W9DS): the
    /// pull request, its head commit and the state. The look of each host
    /// makes the same key for the same news, and riff-server drops the
    /// second post.
    ///
    /// ```
    /// use riff::look::{PullNews, Stop};
    ///
    /// let news = PullNews {
    ///     to: None,
    ///     body: String::new(),
    ///     told: (40, "1a2b3c4d".into(), Stop::Conflict),
    /// };
    /// assert_eq!(news.key(), "pull-40-1a2b3c4d-conflict");
    /// let waits = PullNews { told: (40, "1a2b3c4d".into(), Stop::NoVerify), ..news };
    /// assert_eq!(waits.key(), "pull-40-1a2b3c4d-no-verify");
    /// ```
    pub fn key(&self) -> String {
        let (number, head, stop) = &self.told;
        let stop = match stop {
            Stop::Conflict => "conflict",
            Stop::NoVerify => "no-verify",
        };
        format!("pull-{number}-{head}-{stop}")
    }
}

/// Why a pull request stops on its way to the merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stop {
    /// Auto-merge is on, and it has a conflict with the default branch.
    Conflict,
    /// It waits for a verify, and no session claims the verify.
    NoVerify,
}

/// The stops of the pull requests that the look told, so that each
/// message comes one time for each pull request and state
/// (01M49Q31FASDM7CG3JEGPYCZB9). The `riff mcp` of the lead keeps it in
/// memory. A new head of a pull request is a new state.
#[derive(Debug)]
pub struct PullWatch {
    wait: Duration,
    told: HashSet<(u64, String, Stop)>,
    /// Since when each pull request, at its head, waits with no verify
    /// claim.
    since: HashMap<(u64, String), Instant>,
}

impl Default for PullWatch {
    fn default() -> Self {
        Self::new(VERIFY_WAIT)
    }
}

impl PullWatch {
    /// A watch that tells the lead of a verify that no session claims
    /// after `wait`.
    pub fn new(wait: Duration) -> Self {
        Self {
            wait,
            told: HashSet::new(),
            since: HashMap::new(),
        }
    }

    /// The messages for the open pull requests `pulls` at `now`, with the
    /// claims `claims` and the holds `holds` of the repository, and the
    /// verifies `waits` that wait for another pull request, by issue
    /// ([`crate::wave::order`]):
    ///
    /// - A pull request of an item with auto-merge on and a conflict
    ///   ([`Pull::conflict`]) goes to the session that holds the item, or
    ///   to the lead when no session holds it (01M49Q30XMVRFX42YTM1PHX0RZ).
    /// - A pull request that waits for a verify ([`Verify::Asked`]) with
    ///   no `verify-` claim for the wait of this watch goes to the lead
    ///   (01M49Q316RXNATJP587DWGDNCD). A claim starts the wait again. So
    ///   does a conflict, a hold of its `verify-` item, and a verify that
    ///   waits for another pull request: nobody has to take that verify
    ///   (01M6WCGNN0X2DGYYM97656RQK1).
    ///
    /// Each state comes one time, until the head of the pull request
    /// changes.
    ///
    /// ```
    /// use std::collections::{BTreeMap, HashSet};
    /// use std::time::{Duration, Instant};
    /// use riff::look::PullWatch;
    /// use riff::rollout::Pull;
    ///
    /// let mut watch = PullWatch::new(Duration::from_secs(30 * 60));
    /// let pull = |number, issue: u64, mergeable: &str| Pull {
    ///     number,
    ///     branch: format!("worktree-issue-{issue}"),
    ///     head: "1a2b3c4d".into(),
    ///     mergeable: Some(mergeable.into()),
    ///     auto_merge: true,
    ///     ..Pull::default()
    /// };
    /// let (conflict, asked) = (pull(40, 12, "CONFLICTING"), pull(41, 13, "MERGEABLE"));
    /// let pulls = [conflict.clone(), asked];
    /// let held: HashSet<String> = ["issue-12".to_owned()].into();
    /// let (none, no_wait) = (HashSet::new(), BTreeMap::new());
    /// let start = Instant::now();
    ///
    /// // The conflict goes to the holder of the item, one time.
    /// let news = watch.news(&pulls, &held, &none, &no_wait, start);
    /// assert_eq!(news.len(), 1);
    /// assert_eq!(news[0].to.as_deref(), Some("issue-12"));
    /// assert!(watch.news(&pulls, &held, &none, &no_wait, start).is_empty());
    ///
    /// // 30 minutes with no verify claim: one message to the lead, for the
    /// // pull request with no conflict.
    /// let later = start + Duration::from_secs(30 * 60);
    /// let news = watch.news(&pulls, &held, &none, &no_wait, later);
    /// assert_eq!(news.len(), 1);
    /// assert_eq!(news[0].to, None);
    /// assert!(news[0].body.contains("no session claims verify-issue-13"));
    /// assert!(watch.news(&pulls, &held, &none, &no_wait, later).is_empty());
    ///
    /// // A new head is a new state: the conflict comes again, to the
    /// // lead now that no session holds the item.
    /// let pushed = Pull { head: "5e6f7a8b".into(), ..conflict };
    /// let news = watch.news(&[pushed], &none, &none, &no_wait, later);
    /// assert_eq!(news.len(), 1);
    /// assert_eq!(news[0].to, None);
    /// assert!(news[0].body.contains("No session holds issue-12"));
    /// ```
    pub fn news(
        &mut self,
        pulls: &[Pull],
        claims: &HashSet<String>,
        holds: &HashSet<String>,
        waits: &BTreeMap<u64, u64>,
        now: Instant,
    ) -> Vec<PullNews> {
        let heads: HashSet<(u64, &str)> =
            pulls.iter().map(|p| (p.number, p.head.as_str())).collect();
        self.told
            .retain(|(n, head, _)| heads.contains(&(*n, head.as_str())));
        self.since
            .retain(|(n, head), _| heads.contains(&(*n, head.as_str())));
        let mut news = Vec::new();
        for pull in pulls {
            let Some(issue) = pull.issue() else {
                continue;
            };
            let item = format!("issue-{issue}");
            let (number, head) = (pull.number, pull.head.clone());
            if pull.conflict() && self.told.insert((number, head.clone(), Stop::Conflict)) {
                let held = claims.contains(&item);
                news.push(PullNews {
                    to: held.then(|| item.clone()),
                    body: text::pull_conflict(number, &item, &head, held),
                    told: (number, head.clone(), Stop::Conflict),
                });
            }
            // A conflict gets its rebase first, a held verify waits for the
            // lead, and an ordered verify waits for another merge (#1710).
            let verify = format!("verify-{item}");
            let waits = pull.verify() == Some(Verify::Asked)
                && !claims.contains(&verify)
                && !holds.contains(&verify)
                && pull.mergeable.as_deref() != Some("CONFLICTING")
                && !waits.contains_key(&issue);
            if !waits {
                self.since.remove(&(number, head));
                continue;
            }
            let since = *self.since.entry((number, head.clone())).or_insert(now);
            if now.saturating_duration_since(since) >= self.wait
                && self.told.insert((number, head.clone(), Stop::NoVerify))
            {
                news.push(PullNews {
                    to: None,
                    body: text::pull_no_verify(number, &item, &head, self.wait),
                    told: (number, head, Stop::NoVerify),
                });
            }
        }
        news
    }

    /// Forgets that `news` was told, for a message that did not go out:
    /// the next look tells it again.
    pub fn untell(&mut self, news: &PullNews) {
        self.told.remove(&news.told);
    }
}

/// The full check of the head commit of the default branch
/// (01M51QEYKTD7ZEB0YA0C3MQTPN, 01M5R13DGMTNWPY88A3EG49WWS,
/// 01M5QYBWPAJ219BE8QKVKG1TWW).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainGate {
    /// The head commit of the default branch.
    pub commit: String,
    /// The state of its full check ([`full_state`]).
    pub gate: Gate,
    /// The page of the run of the check that did not pass, or of the check
    /// with the newest job when all passed, for example
    /// `https://github.com/o/r/actions/runs/38861280871`.
    pub run: Option<String>,
    /// The number of the job of that check in the run, for example `7`. A
    /// rerun of a run keeps the run and makes new jobs ([`job_of`]).
    pub job: Option<String>,
}

impl MainGate {
    /// The full check of a commit from the check runs of each check
    /// (`checks`, one list for each check of [`repo_setup::full_check`]):
    /// `None` when the commit has no run of any check. The commit is the
    /// commit of the first run, and a run of another commit does not
    /// count: `main` moved between two calls. Of the runs of one check,
    /// only the newest count ([`newest_runs`], 01M5RGM8HBVQMY25H5PYESK17J).
    /// The run is the first newest run that did not pass. When all passed,
    /// it is the newest run with the highest job number, so a rerun of any
    /// one check changes it (01M6GX8CN4ZTV0X40GD5AWPXH9).
    ///
    /// ```
    /// use riff::look::MainGate;
    /// use riff::pr::{CheckRun, Gate};
    ///
    /// let run = |conclusion: &str, id: u64| CheckRun {
    ///     status: "completed".into(),
    ///     conclusion: Some(conclusion.into()),
    ///     head_sha: "5103311abc".into(),
    ///     html_url: Some(format!("https://github.com/o/r/actions/runs/{id}/job/7")),
    ///     ..CheckRun::default()
    /// };
    /// let main = MainGate::of(&[vec![run("success", 1)], vec![run("failure", 2)]]).unwrap();
    /// assert_eq!(main.commit, "5103311abc");
    /// assert_eq!(main.gate, Gate::Ended("failure".into()));
    /// assert_eq!(main.run.as_deref(), Some("https://github.com/o/r/actions/runs/2"));
    /// assert_eq!(main.job.as_deref(), Some("7"));
    /// assert!(main.broken());
    /// // A check with no run yet: the full check has not started.
    /// let waits = MainGate::of(&[vec![run("success", 1)], vec![]]).unwrap();
    /// assert_eq!(waits.gate, Gate::Missing);
    /// assert_eq!(MainGate::of(&[vec![], vec![]]), None);
    /// // A repository with no CI has no check to read (01M5SMNKDWBD1J6Y18ENQG87CK).
    /// assert_eq!(MainGate::of(&[]), None);
    /// // A required check that was skipped passed.
    /// assert_eq!(MainGate::of(&[vec![run("skipped", 3)]]).unwrap().gate, Gate::Passed);
    /// // An old failed run of a check does not count when a newer run passed.
    /// let at = |started: &str, run: CheckRun| CheckRun { started_at: Some(started.into()), ..run };
    /// let old = at("2026-10-24T17:04:00Z", run("failure", 4));
    /// let new = at("2026-10-24T17:26:00Z", run("success", 5));
    /// let main = MainGate::of(&[vec![old.clone(), new.clone()]]).unwrap();
    /// assert_eq!(main.gate, Gate::Passed);
    /// assert_eq!(main.run.as_deref(), Some("https://github.com/o/r/actions/runs/5"));
    /// assert_eq!(MainGate::of(&[vec![new, old]]).unwrap().gate, Gate::Passed);
    /// // Two checks that passed: the run is the one with the highest job, so
    /// // a rerun of the second check shows, and not only a rerun of the first.
    /// let at_job = |id: u64, job: u64| CheckRun {
    ///     html_url: Some(format!("https://github.com/o/r/actions/runs/{id}/job/{job}")),
    ///     ..run("success", id)
    /// };
    /// let main = MainGate::of(&[vec![at_job(6, 20)], vec![at_job(7, 30)]]).unwrap();
    /// assert_eq!(main.run.as_deref(), Some("https://github.com/o/r/actions/runs/7"));
    /// assert_eq!(main.job.as_deref(), Some("30"));
    /// ```
    pub fn of(checks: &[Vec<CheckRun>]) -> Option<Self> {
        let first = checks.iter().flatten().next()?;
        let commit = first.head_sha.as_str();
        let at_head: Vec<Vec<CheckRun>> = checks
            .iter()
            .map(|runs| {
                runs.iter()
                    .filter(|r| r.head_sha == commit)
                    .cloned()
                    .collect()
            })
            .collect();
        let gate = full_state(at_head.iter().map(|runs| gate_state(runs)));
        let newest: Vec<&CheckRun> = at_head.iter().flat_map(|runs| newest_runs(runs)).collect();
        let job_number = |r: &CheckRun| {
            r.html_url
                .as_deref()
                .and_then(job_of)
                .and_then(|job| job.parse::<u64>().ok())
        };
        let shown = newest
            .iter()
            .copied()
            .find(|r| r.conclusion.as_deref().is_some_and(|c| !is_success(c)))
            // A tie keeps the first check.
            .or_else(|| {
                newest.iter().copied().reduce(|best, r| {
                    if job_number(r) > job_number(best) {
                        r
                    } else {
                        best
                    }
                })
            })
            .unwrap_or(first);
        let page = shown.html_url.as_deref();
        Some(Self {
            commit: commit.to_owned(),
            gate,
            run: page.and_then(run_of).map(str::to_owned),
            job: page.and_then(job_of).map(str::to_owned),
        })
    }

    /// True when the full check failed: a check ended with `failure` or
    /// `timed_out`. A cancelled check says nothing about the code.
    pub fn broken(&self) -> bool {
        matches!(&self.gate, Gate::Ended(c) if c == "failure" || c == "timed_out")
    }
}

/// The page of the run of the page of a job.
///
/// ```
/// use riff::look::run_of;
///
/// assert_eq!(
///     run_of("https://github.com/o/r/actions/runs/38861280871/job/1"),
///     Some("https://github.com/o/r/actions/runs/38861280871")
/// );
/// assert_eq!(run_of("https://github.com/o/r/runs/2"), None);
/// ```
pub fn run_of(job: &str) -> Option<&str> {
    let (run, _) = job.split_once("/job/")?;
    run.contains("/actions/runs/").then_some(run)
}

/// The number of the job in the page of a job of a run. A rerun of a run
/// keeps the number of the run and makes new jobs, so the job tells one
/// attempt from the next.
///
/// ```
/// use riff::look::job_of;
///
/// assert_eq!(job_of("https://github.com/o/r/actions/runs/38861280871/job/7"), Some("7"));
/// assert_eq!(job_of("https://github.com/o/r/actions/runs/38861280871/job/7/steps"), Some("7"));
/// assert_eq!(job_of("https://github.com/o/r/runs/2"), None);
/// ```
pub fn job_of(job: &str) -> Option<&str> {
    run_of(job)?;
    let (_, tail) = job.split_once("/job/")?;
    let end = tail
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(tail.len());
    (end > 0).then(|| &tail[..end])
}

/// The number at the end of the page of a run.
fn number_of(run: &str) -> Option<&str> {
    run.rsplit('/')
        .next()
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

/// Reads the full check of the head of `branch`, the default branch, in
/// `repo` (`OWNER/REPO`) with `gh`: the checks that the ruleset of the
/// default branch requires ([`repo_setup::full_check`]), then the check
/// runs of each of them on `branch`. `None` when the head has no run of
/// any check yet. It fails when `gh` cannot read the required checks.
pub fn read_main(gh: &Gh, repo: &str, branch: &str) -> Result<Option<MainGate>> {
    let checks = repo_setup::full_check(gh, repo)?;
    let branch = crate::pr::percent(branch);
    let mut runs = Vec::new();
    for check in &checks {
        runs.push(check_runs(gh, repo, &branch, check)?);
    }
    Ok(MainGate::of(&runs))
}

/// A job of a run, as `gh api repos/R/actions/runs/N/jobs` gives it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Job {
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub name: String,
    /// `success`, `failure`, `cancelled` and so on, when it is completed.
    #[serde(default, deserialize_with = "crate::text::forge_de_opt")]
    pub conclusion: Option<String>,
}

/// The names of the jobs that did not pass, with no Gate: the Gate only
/// sums the other jobs.
///
/// ```
/// use riff::look::{Job, failed_jobs};
///
/// let job = |name: &str, conclusion: &str| Job { name: name.into(), conclusion: Some(conclusion.into()) };
/// let jobs = [
///     job("Clippy", "failure"),
///     job("Tests 1/4", "timed_out"),
///     job("Checks", "success"),
///     job("Static riff", "skipped"),
///     job("Gate", "failure"),
/// ];
/// assert_eq!(failed_jobs(&jobs), ["Clippy", "Tests 1/4"]);
/// ```
pub fn failed_jobs(jobs: &[Job]) -> Vec<String> {
    jobs.iter()
        .filter(|j| j.name != GATE)
        .filter(|j| {
            matches!(
                j.conclusion.as_deref(),
                Some("failure" | "timed_out" | "cancelled")
            )
        })
        .map(|j| j.name.clone())
        .collect()
}

/// Reads the failed jobs of the run `run` in `repo` with `gh`
/// ([`failed_jobs`]).
pub fn read_failed_jobs(gh: &Gh, repo: &str, run: &str) -> Result<Vec<String>> {
    Ok(failed_jobs(&read_jobs(gh, repo, run)?))
}

/// Reads the jobs of the run `run` (its page) in `repo` with `gh`.
pub fn read_jobs(gh: &Gh, repo: &str, run: &str) -> Result<Vec<Job>> {
    let id = number_of(run).ok_or_else(|| anyhow!("no run number in {run}"))?;
    let path = format!("repos/{repo}/actions/runs/{id}/jobs?per_page=100");
    gh.json(&["api", &path, "--jq", ".jobs"])
}

/// What the full check of `main` tells at one look ([`MainWatch::news`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainNews {
    /// The full check of `main` failed at a commit that the lead did not get.
    /// `first` is true when `main` passed before: the lead gets a
    /// message. Else `main` was broken at an older commit: a note.
    Broken { main: MainGate, first: bool },
    /// The full check of `main` passed. `again` is true when the lead got a
    /// broken `main` before.
    Passed { main: MainGate, again: bool },
}

impl MainNews {
    /// The key of the post of the news (01M5TKG2K1AYK1KE7C2AA2W9DS,
    /// 01M6GX8CN4ZTV0X40GD5AWPXH9): the commit of `main`, the run of the
    /// check that the message names (the number of the run and of its job)
    /// and the state, `broken` or `passes`. A broken note names the check
    /// that failed. A note that `main` passes names the check with the
    /// highest job among the required checks ([`MainGate::of`]). The look
    /// of each host makes the same key for the same news, and riff-server
    /// drops the second post. A rerun of the same commit has a new job,
    /// and a job is higher than the jobs before it, so a rerun of any one
    /// check gives a new key. A check with no page of a run in Actions
    /// gives a key with no run.
    ///
    /// ```
    /// use riff::look::{MainGate, MainNews};
    /// use riff::pr::Gate;
    ///
    /// let main = |gate, job: &str| MainGate {
    ///     commit: "c60887e1".into(),
    ///     gate,
    ///     run: Some("https://github.com/o/r/actions/runs/38861280871".into()),
    ///     job: Some(job.into()),
    /// };
    /// let failure = Gate::Ended("failure".into());
    /// let broken = MainNews::Broken { main: main(failure.clone(), "7"), first: true };
    /// assert_eq!(broken.key(), "main-c60887e1-38861280871-7-broken");
    /// let passes = MainNews::Passed { main: main(Gate::Passed, "8"), again: true };
    /// assert_eq!(passes.key(), "main-c60887e1-38861280871-8-passes");
    /// // It breaks again on a second rerun: a new job, a new key.
    /// let again = MainNews::Broken { main: main(failure.clone(), "9"), first: true };
    /// assert_eq!(again.key(), "main-c60887e1-38861280871-9-broken");
    ///
    /// let outside = MainGate { commit: "c60887e1".into(), gate: failure, run: None, job: None };
    /// let broken = MainNews::Broken { main: outside, first: true };
    /// assert_eq!(broken.key(), "main-c60887e1-broken");
    /// ```
    pub fn key(&self) -> String {
        let (main, state) = match self {
            MainNews::Broken { main, .. } => (main, "broken"),
            MainNews::Passed { main, .. } => (main, "passes"),
        };
        let ran = main
            .run
            .as_deref()
            .and_then(number_of)
            .zip(main.job.as_deref());
        match ran {
            Some((run, job)) => format!("main-{}-{run}-{job}-{state}", main.commit),
            None => format!("main-{}-{state}", main.commit),
        }
    }
}

/// The last broken commit of `main` that the lead got
/// (01M51QEYKTD7ZEB0YA0C3MQTPN). The `riff mcp` of the lead keeps it in
/// memory.
#[derive(Debug, Clone, Default)]
pub struct MainWatch {
    told: Option<String>,
}

impl MainWatch {
    /// The news of `main` at one look. A full check that runs, a head with no
    /// run and a cancelled check give none. A failed full check gives news one
    /// time for each commit. A passed full check gives news at each look: the
    /// look closes the free fix items then.
    ///
    /// ```
    /// use riff::look::{MainGate, MainNews, MainWatch};
    /// use riff::pr::Gate;
    ///
    /// let main = |commit: &str, gate: Gate| MainGate { commit: commit.into(), gate, run: None, job: None };
    /// let failed = |commit: &str| main(commit, Gate::Ended("failure".into()));
    /// let mut watch = MainWatch::default();
    ///
    /// assert_eq!(
    ///     watch.news(&failed("a")),
    ///     Some(MainNews::Broken { main: failed("a"), first: true })
    /// );
    /// assert_eq!(watch.news(&failed("a")), None);
    /// assert_eq!(watch.news(&main("b", Gate::Running)), None);
    /// assert_eq!(watch.news(&main("b", Gate::Ended("cancelled".into()))), None);
    /// // Still broken at a newer commit: a note.
    /// assert_eq!(
    ///     watch.news(&failed("b")),
    ///     Some(MainNews::Broken { main: failed("b"), first: false })
    /// );
    /// let passed = main("c", Gate::Passed);
    /// assert_eq!(watch.news(&passed), Some(MainNews::Passed { main: passed.clone(), again: true }));
    /// assert_eq!(watch.news(&passed), Some(MainNews::Passed { main: passed.clone(), again: false }));
    /// ```
    pub fn news(&mut self, main: &MainGate) -> Option<MainNews> {
        if main.broken() {
            if self.told.as_deref() == Some(main.commit.as_str()) {
                return None;
            }
            let first = self.told.replace(main.commit.clone()).is_none();
            return Some(MainNews::Broken {
                main: main.clone(),
                first,
            });
        }
        (main.gate == Gate::Passed).then(|| MainNews::Passed {
            main: main.clone(),
            again: self.told.take().is_some(),
        })
    }
}

/// The open fix items of `issues`: each issue with the label
/// [`BROKEN_MAIN`].
fn fix_items_of(issues: &[Issue]) -> Vec<u64> {
    issues
        .iter()
        .filter(|i| i.broken_main())
        .map(|i| i.number)
        .collect()
}

/// The open fix item of `repo`, read now with `gh`: the lead of another
/// person can file one after the look read the issues.
fn open_fix_item(gh: &Gh, repo: &str) -> Result<Option<u64>> {
    let open: Vec<Issue> = gh.json(&[
        "issue",
        "list",
        "--repo",
        repo,
        "--label",
        BROKEN_MAIN,
        "--state",
        "open",
        "--json",
        "number,labels",
    ])?;
    Ok(fix_items_of(&open).first().copied())
}

/// Puts the commit, the run and the failed jobs of a failed full check of
/// `main` on the open fix item `number` (01M51QEYPGD6W345MZ4F85C38F).
fn comment_fix_item(
    gh: &Gh,
    repo: &str,
    number: u64,
    main: &MainGate,
    jobs: &[String],
) -> Result<u64> {
    let comment = text::fix_item_comment(&main.commit, main.run.as_deref(), jobs);
    let number_text = number.to_string();
    gh.run(
        &[
            "issue",
            "comment",
            &number_text,
            "--repo",
            repo,
            "--body",
            &comment,
        ],
        None,
    )?;
    Ok(number)
}

/// Files the fix item of the failed full check of `main` in `repo` with `gh`
/// (01M51QEYPGD6W345MZ4F85C38F): it makes the label [`BROKEN_MAIN`] when
/// it is missing, and puts the issue in the current wave. It gives the
/// number of the new issue.
pub fn file_fix_item(gh: &Gh, repo: &str, main: &MainGate, jobs: &[String]) -> Result<u64> {
    // A label that is there needs no change: `issue create` fails only
    // when the label is missing.
    let _ = gh.run(
        &[
            "label",
            "create",
            BROKEN_MAIN,
            "--repo",
            repo,
            "--color",
            "B60205",
            "--description",
            text::BROKEN_MAIN_LABEL,
            "--force",
        ],
        None,
    );
    let open: Vec<Milestone> = gh.json(&[
        "api",
        &format!("repos/{repo}/milestones?state=open&per_page=100"),
    ])?;
    let title = text::fix_item_title(&main.commit);
    // The body goes in a file: an error of gh names its arguments, and a
    // short line keeps the reason in the post to the lead.
    let mut body = tempfile::NamedTempFile::new()?;
    body.write_all(text::fix_item_body(&main.commit, main.run.as_deref(), jobs).as_bytes())?;
    let body_path = body.path().to_string_lossy().into_owned();
    let mut args = vec![
        "issue",
        "create",
        "--repo",
        repo,
        "--title",
        &title,
        "--body-file",
        &body_path,
        "--label",
        BROKEN_MAIN,
    ];
    if let Some(wave) = current_wave(&open) {
        args.extend(["--milestone", wave]);
    }
    let url = text::forge(gh.run(&args, None)?.trim());
    url.rsplit('/')
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or_else(|| anyhow!("gh issue create printed no issue URL: {url}"))
}

/// Does the steps of the forge for `news` in `repo` with `gh`, and gives
/// the post to the lead, with true for a message that wakes it
/// (01M51QEYKTD7ZEB0YA0C3MQTPN to 01M51QEYVQN74Q1VDTVHB0445Q):
///
/// - A broken `main`: the failed jobs, and the fix item. An open fix
///   item gets a comment. Else riff reads the label again, and files a
///   fix item only when it finds none. A step that fails goes into the
///   post: the lead must know that `main` is broken.
/// - A passed full check: riff closes each fix item that no session claims
///   (`claims`) and that no open pull request names (`pulls`). The post
///   comes when `main` was broken, or when riff closed an item.
pub fn main_steps(
    gh: &Gh,
    repo: &str,
    issues: &[Issue],
    pulls: &[Pull],
    claims: &HashSet<String>,
    news: &MainNews,
) -> Result<Option<(String, bool)>> {
    match news {
        MainNews::Broken { main, first } => {
            let jobs = match &main.run {
                Some(run) => read_failed_jobs(gh, repo, run).unwrap_or_default(),
                None => Vec::new(),
            };
            let fix = match fix_items_of(issues).first() {
                Some(&n) => comment_fix_item(gh, repo, n, main, &jobs),
                None => open_fix_item(gh, repo).and_then(|open| match open {
                    Some(n) => comment_fix_item(gh, repo, n, main, &jobs),
                    None => file_fix_item(gh, repo, main, &jobs),
                }),
            };
            let fix = fix.map_err(|e| text::forge(&format!("{e:#}")));
            let body = text::main_broken(&main.commit, main.run.as_deref(), &jobs, &fix, *first);
            Ok(Some((body, *first)))
        }
        MainNews::Passed { main, again } => {
            let named: HashSet<u64> = pulls.iter().filter_map(Pull::issue).collect();
            let (free, kept): (Vec<u64>, Vec<u64>) = fix_items_of(issues)
                .into_iter()
                .partition(|n| !claims.contains(&format!("issue-{n}")) && !named.contains(n));
            let comment = text::fix_item_closed(&main.commit, main.run.as_deref());
            for n in &free {
                gh.run(
                    &[
                        "issue",
                        "close",
                        &n.to_string(),
                        "--repo",
                        repo,
                        "--comment",
                        &comment,
                    ],
                    None,
                )?;
            }
            if !*again && free.is_empty() {
                return Ok(None);
            }
            let body = text::main_passes(&main.commit, main.run.as_deref(), &free, &kept, *again);
            Ok(Some((body, false)))
        }
    }
}

/// One look at the full check of the default branch `branch` of `repo`,
/// with the open `issues` and `pulls` and the `claims` of this look: it
/// reads the full check, and does the steps of the forge
/// ([`main_steps`]). It gives the post to the lead, its kind (a
/// message that wakes the lead, or a note) and its key
/// ([`MainNews::key`]). `watch` now holds the news:
/// when the post does not go out, the caller sets it back to what it
/// was, and the next look does it again.
///
/// ```
/// use std::sync::Arc;
/// use riff::look::{MainWatch, main_post};
/// use riff::pr::Gh;
/// use riff_core::wire::Kind;
///
/// # tokio::runtime::Builder::new_current_thread().build()?.block_on(async {
/// let dir = tempfile::tempdir()?;
/// let gh = isolated::write_program(
///     dir.path().join("gh"),
///     "#!/bin/sh\n\
///      case \"$1 $2\" in\n\
///        api*) case \"$2\" in\n\
///          repos/o/r) echo '{\"default_branch\":\"master\"}' ;;\n\
///          */rules/branches/master*) echo '[{\"type\":\"required_status_checks\",\"parameters\":{\"required_status_checks\":[{\"context\":\"Gate\"},{\"context\":\"riff/verify\"}]}}]' ;;\n\
///          */commits/master/check-runs?check_name=Gate) echo '{\"check_runs\":[{\"status\":\"completed\",\"conclusion\":\"failure\",\"head_sha\":\"5103311abc\"}]}' ;;\n\
///          *) echo '[]' ;;\n\
///        esac ;;\n\
///        \"issue list\") echo '[]' ;;\n\
///        \"issue create\") echo https://github.com/o/r/issues/901 ;;\n\
///      esac\n",
/// );
/// let gh = Arc::new(Gh::at(gh));
/// let mut watch = MainWatch::default();
///
/// // The default branch of the repository is `master`. The full check failed at a
/// // new commit: a message for the lead, with the fix item.
/// let (body, kind, key) = main_post(&gh, "o/r", "master", (vec![], vec![]), Default::default(), &mut watch)
///     .await?
///     .unwrap();
/// assert_eq!(kind, Kind::Message);
/// assert_eq!(key, "main-5103311abc-broken");
/// assert!(body.starts_with("main is broken: the full check failed at commit 5103311."), "{body}");
/// assert!(body.contains("The fix item is #901"), "{body}");
/// // The same commit at the next look: nothing.
/// assert_eq!(main_post(&gh, "o/r", "master", (vec![], vec![]), Default::default(), &mut watch).await?, None);
/// # Ok::<(), anyhow::Error>(())
/// # })?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub async fn main_post(
    gh: &Arc<Gh>,
    repo: &str,
    branch: &str,
    forge: (Vec<Issue>, Vec<Pull>),
    claims: HashSet<String>,
    watch: &mut MainWatch,
) -> Result<Option<(String, Kind, String)>> {
    let main = {
        let (gh, repo, branch) = (gh.clone(), repo.to_owned(), branch.to_owned());
        tokio::task::spawn_blocking(move || read_main(&gh, &repo, &branch)).await??
    };
    let Some(main) = main else {
        return Ok(None);
    };
    let Some(news) = watch.news(&main) else {
        return Ok(None);
    };
    let key = news.key();
    let (gh, repo) = (gh.clone(), repo.to_owned());
    let steps = tokio::task::spawn_blocking(move || {
        let (issues, pulls) = forge;
        main_steps(&gh, &repo, &issues, &pulls, &claims, &news)
    })
    .await??;
    Ok(steps.map(|(body, wake)| (body, if wake { Kind::Message } else { Kind::Note }, key)))
}

/// One look at the full check of the default branch `branch` of the lead
/// `me` ([`main_post`]): it posts to the lead. When the post does not go
/// out, `watch` forgets the news, and the next look does it again.
pub async fn look_at_main(
    api: &Api,
    me: &SessionUri,
    gh: &Arc<Gh>,
    branch: &str,
    forge: (Vec<Issue>, Vec<Pull>),
    claims: HashSet<String>,
    watch: &mut MainWatch,
) -> Result<()> {
    let before = watch.clone();
    let posted = match main_post(gh, &me.place().repo_text(), branch, forge, claims, watch).await {
        Ok(Some((body, kind, key))) => post_as_person(api, me, None, &body, kind, Some(&key)).await,
        Ok(None) => Ok(()),
        Err(e) => Err(e),
    };
    if posted.is_err() {
        *watch = before;
    }
    posted
}

/// The issues with the label `question` that the look told the person of
/// the lead of (#1605). The daemon keeps them in memory. An issue that
/// loses the label, or closes, goes out of the set: when the label goes on
/// again, the person gets a new notice.
#[derive(Debug, Default, Clone)]
pub struct AskWatch {
    told: BTreeSet<u64>,
}

impl AskWatch {
    /// The open `issues` that have the label `question` and that this
    /// watch did not tell yet, in the order of `issues`. The watch keeps
    /// them as told: [`AskWatch::untell`] takes one back after a post that
    /// failed.
    ///
    /// ```
    /// use riff::look::AskWatch;
    /// use riff::rollout::{Issue, Label};
    ///
    /// let issue = |number, label: &str| Issue {
    ///     number,
    ///     labels: vec![Label { name: label.into() }],
    ///     ..Issue::default()
    /// };
    /// let mut watch = AskWatch::default();
    /// let open = [issue(12, "question"), issue(13, "bug"), issue(14, "question")];
    /// let numbers = |news: Vec<&Issue>| news.iter().map(|i| i.number).collect::<Vec<_>>();
    /// assert_eq!(numbers(watch.news(&open)), [12, 14]);
    /// // The same look again: nothing new.
    /// assert!(watch.news(&open).is_empty());
    /// // 12 got its answer, and 14 stays. The label goes on 12 again: a new notice.
    /// let answered = [issue(12, "bug"), issue(14, "question")];
    /// assert!(watch.news(&answered).is_empty());
    /// assert_eq!(numbers(watch.news(&open)), [12]);
    /// ```
    pub fn news<'a>(&mut self, issues: &'a [Issue]) -> Vec<&'a Issue> {
        let asking: BTreeSet<u64> = issues
            .iter()
            .filter(|i| i.question())
            .map(|i| i.number)
            .collect();
        self.told.retain(|n| asking.contains(n));
        issues
            .iter()
            .filter(|i| i.question())
            .filter(|i| self.told.insert(i.number))
            .collect()
    }

    /// Takes `issue` back as not told: the post of its notice failed, so
    /// the next look tells it again.
    pub fn untell(&mut self, issue: u64) {
        self.told.remove(&issue);
    }
}

/// The key of the post of the notice for the label `question` on `issue`
/// (#1605). `at` is the time that the label went on, from the events of
/// the issue ([`question_at`]). The look of each host makes the same key
/// for the same label, and riff-server drops the second post. A new time
/// is a new key, so the person hears each time that the label goes on.
///
/// ```
/// use riff::look::question_key;
///
/// assert_eq!(question_key(12, Some("2026-11-05T20:11:03Z")), "question-12-2026-11-05T20:11:03Z");
/// assert_eq!(question_key(12, None), "question-12");
/// ```
pub fn question_key(issue: u64, at: Option<&str>) -> String {
    match at {
        Some(at) => format!("question-{issue}-{at}"),
        None => format!("question-{issue}"),
    }
}

/// The time that the label `question` went on `issue` of `repo` the last
/// time, from the events of the issue. `None` when the read fails or
/// finds no event: the notice then has no time in its key.
pub fn question_at(gh: &Gh, repo: &str, issue: u64) -> Option<String> {
    let path = format!("repos/{repo}/issues/{issue}/events");
    let filter = format!(
        r#".[] | select(.event == "labeled" and .label.name == "{}") | .created_at"#,
        riff_client::waves::QUESTION
    );
    let found = gh
        .run(&["api", &path, "--paginate", "--jq", &filter], None)
        .ok()?;
    found
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(str::to_owned)
}

/// What the look of the daemon keeps between two looks of one wave:
/// what it told of the pull requests and of `main`, and of the label
/// `question` (01M5QVH3MAEPPJN9PPVDX6N2EY, #1605). It is in memory only.
#[derive(Debug, Default)]
pub struct Watch {
    pulls: PullWatch,
    main: MainWatch,
    asks: AskWatch,
    /// How old a reply of an earlier read of the forge can be, for the
    /// next look ([`look_wave`]). The loop of the wave reads the same
    /// issues and pull requests, so the look takes its reply and calls no
    /// `gh` for them (#1549). Zero: the look reads the forge itself.
    pub accepts: Duration,
    /// The verifies of the wave that wait for another pull request, from
    /// the last look of its loop ([`crate::wave::order`], #1710): no
    /// message "waits for a verify" names them.
    pub waits: BTreeMap<u64, u64>,
}

/// One look of the daemon at the repository of `wave`
/// (01M5QVH3MAEPPJN9PPVDX6N2EY): it reads the open issues and pull
/// requests and the claims, sends the messages for the pull requests
/// that stop ([`PullWatch::news`]), and looks at the full check of the
/// default branch ([`main_post`]). Each post goes through [`World::post`].
/// When a post does not go out, `watch` forgets what it did not tell, and
/// the next look does it again. A forge or a riff-server that does not
/// answer is the error. So is a default branch that does not read
/// ([`crate::pr::default_branch`]): the wave reads it at each look, and
/// keeps it in no file (01M5R56GN3DC09GAPAF69GENEZ).
///
/// The look takes the reply of an earlier read of the issues and the pull
/// requests, when it is not older than [`Watch::accepts`]: the loop of the
/// wave reads them too. It gives `true` when it posted something, and
/// `false` for a quiet look.
pub async fn look_wave(world: &dyn World, wave: &Wave, watch: &mut Watch) -> Result<bool> {
    let gh = Arc::new(Gh::clone(&world.gh(&wave.repo)).accepting(watch.accepts));
    let (issues, pulls) = {
        let (gh, repo) = (gh.clone(), wave.repo.clone());
        tokio::task::spawn_blocking(move || read_forge(&gh, &repo)).await??
    };
    let claims = world.claims(wave).await?;
    let holds = world.holds(wave).await?;
    let news = watch
        .pulls
        .news(&pulls, &claims, &holds, &watch.waits, Instant::now());
    let mut told = !news.is_empty();
    for (at, one) in news.iter().enumerate() {
        let item = one.to.as_deref();
        let key = one.key();
        if let Err(e) = world
            .post(wave, item, &one.body, Kind::Message, Some(&key))
            .await
        {
            news[at..].iter().for_each(|rest| watch.pulls.untell(rest));
            return Err(e);
        }
    }
    // The person hears once that the label `question` went on an issue
    // (#1605). A post that fails stays for the next look.
    let asking: Vec<(u64, String)> = watch
        .asks
        .news(&issues)
        .iter()
        .map(|i| (i.number, i.title.clone()))
        .collect();
    for (at, (number, title)) in asking.iter().enumerate() {
        let (gh, repo, number) = (gh.clone(), wave.repo.clone(), *number);
        let labeled = tokio::task::spawn_blocking(move || question_at(&gh, &repo, number)).await?;
        let key = question_key(number, labeled.as_deref());
        let body = text::wave_question(&wave.repo, number, title);
        told = true;
        if let Err(e) = world.ask_person(wave, &body, &key).await {
            asking[at..]
                .iter()
                .for_each(|(rest, _)| watch.asks.untell(*rest));
            return Err(e);
        }
    }
    // Like a forge that does not answer, a default branch that does not read is
    // the error of the look. The news about the pull requests went out first.
    let branch = {
        let (gh, repo) = (gh.clone(), wave.repo.clone());
        tokio::task::spawn_blocking(move || crate::pr::default_branch(&gh, &repo)).await??
    };
    let before = watch.main.clone();
    let repo = &wave.repo;
    // The issue of a broken main, its label and its comments go as the
    // daemon (01M6XZJFE2ZTR5RE3AWM30R704).
    let gh = Arc::new(
        crate::daemon::gh_token::writer(world, Some(&wave.server), repo, Gh::clone(&gh)).await,
    );
    let posted = match main_post(&gh, repo, &branch, (issues, pulls), claims, &mut watch.main).await
    {
        Ok(Some((body, kind, key))) => {
            told = true;
            world.post(wave, None, &body, kind, Some(&key)).await
        }
        Ok(None) => Ok(()),
        Err(e) => Err(e),
    };
    if posted.is_err() {
        watch.main = before;
    }
    posted.map(|()| told)
}

/// Posts `news` as a message, as the person of the lead `me`: to the
/// session that holds its item, or to the lead.
async fn tell_news(api: &Api, me: &SessionUri, news: &PullNews) -> Result<()> {
    let key = news.key();
    post_as_person(
        api,
        me,
        news.to.as_deref(),
        &news.body,
        Kind::Message,
        Some(&key),
    )
    .await
}

/// Posts `body` of `kind`, as the person of the lead `me`: to the
/// session that holds the item `to`, or to the lead when `to` is `None`.
/// The lead does not see its own posts, so the person posts it. The post has
/// `key`, when there is one (01M5TKG2K1AYK1KE7C2AA2W9DS).
async fn post_as_person(
    api: &Api,
    me: &SessionUri,
    to: Option<&str>,
    body: &str,
    kind: Kind,
    key: Option<&str>,
) -> Result<()> {
    let person = SessionUri::new(Who::new(me.who().user(), None)?, me.place().clone());
    let to: Selector = match (to, me.who().session()) {
        (Some(item), _) => format!("claim={item},repo={}", me.place().repo_text()).parse()?,
        (None, Some(session)) => format!("session={session}").parse()?,
        (None, None) => anyhow::bail!("the lead has no session"),
    };
    api.post_keyed(
        &person,
        me.default_thread().as_ref(),
        &[to],
        body,
        kind,
        key,
    )
    .await?;
    Ok(())
}

/// The desktop notification of a block with no answer
/// (01M41FZQKZKW131Z8822G31T5G): `notify-send`, when this machine has a
/// display.
#[derive(Clone, Debug)]
pub struct Notifier {
    pub program: PathBuf,
}

impl Default for Notifier {
    fn default() -> Self {
        Self {
            program: "notify-send".into(),
        }
    }
}

impl Notifier {
    /// True when this machine has a desktop: a display, and the program.
    pub fn here(&self) -> bool {
        let set = |name| std::env::var_os(name).is_some_and(|v| !v.is_empty());
        (set("DISPLAY") || set("WAYLAND_DISPLAY")) && self.found()
    }

    fn found(&self) -> bool {
        if self.program.components().count() > 1 {
            return self.program.is_file();
        }
        std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|dir| dir.join(&self.program).is_file())
        })
    }

    /// Shows `block`. It never fails: a failed notification changes
    /// nothing.
    pub fn show(&self, block: &Unanswered) {
        let (title, body) = notification(block);
        let _ = std::process::Command::new(&self.program)
            .args(["--app-name=riff", &title, &body])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// The title and the body of the notification of `block`: the session,
/// its claims and the reason. No text of a message.
///
/// ```
/// use riff::look::notification;
/// use riff_core::wire::Unanswered;
///
/// let block = Unanswered {
///     session: "riff://mike@pangolin/o/r?session=1a2b3c4d5e&claim=issue-12".parse()?,
///     reason: "which design?".into(),
/// };
/// let (title, body) = notification(&block);
/// assert_eq!(title, "riff: the lead gave no answer");
/// assert_eq!(body, "1a2b3c4d (issue-12) is blocked: which design?");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn notification(block: &Unanswered) -> (String, String) {
    let id = block.session.who().session().unwrap_or_default();
    let short: String = id.chars().take(8).collect();
    let claims = match block.session.claims() {
        [] => String::new(),
        claims => format!(" ({})", claims.join(", ")),
    };
    let reason = crate::text::safe(&block.reason);
    (
        "riff: the lead gave no answer".into(),
        format!("{short}{claims} is blocked: {reason}"),
    )
}

/// One look of the lead `me`: it sends the facts of the forge, when it
/// can read them, and looks at the blocks with the wake time `wake`. It
/// shows each block that is unanswered now with `notify`, when it has
/// one. Then it sends the messages of `watch` for the pull requests that
/// stop ([`PullWatch::news`]), and looks at the full check of the default
/// branch ([`look_at_main`], with the watch `main`). It reads that
/// branch from the forge ([`crate::pr::default_branch`]): a branch that
/// does not read gives no look at the full check
/// (01M5R0J131464KZX180BW2AK98). It gives the blocks that are unanswered
/// now.
pub async fn once(
    api: &Api,
    me: &SessionUri,
    gh: &Arc<Gh>,
    wake: Duration,
    notify: Option<&Notifier>,
    watch: &mut PullWatch,
    main: &mut MainWatch,
) -> Result<Vec<Unanswered>> {
    let forge = {
        let (gh, repo) = (gh.clone(), me.place().repo_text());
        tokio::task::spawn_blocking(move || read_forge(&gh, &repo)).await?
    };
    // A forge that does not answer stops no look at the blocks.
    let forge = match forge {
        Ok((issues, pulls)) => {
            api.item_facts(me, item_facts(&issues, &pulls), true)
                .await?;
            Some((issues, pulls))
        }
        Err(_) => None,
    };
    let unanswered = api.look_blocks(me, wake.as_secs()).await?;
    if let Some(notifier) = notify {
        for block in &unanswered {
            notifier.show(block);
        }
    }
    if let Some((issues, pulls)) = forge {
        let claims = claims(&api.who(me, false).await?);
        let (holds, waits) = (HashSet::new(), BTreeMap::new());
        for news in watch.news(&pulls, &claims, &holds, &waits, Instant::now()) {
            if let Err(e) = tell_news(api, me, &news).await {
                watch.untell(&news);
                return Err(e);
            }
        }
        // Like a forge that does not answer, a default branch that does not
        // read stops no look at the blocks, which came first.
        let branch = {
            let (gh, repo) = (gh.clone(), me.place().repo_text());
            tokio::task::spawn_blocking(move || crate::pr::default_branch(&gh, &repo)).await?
        };
        if let Ok(branch) = branch {
            look_at_main(api, me, gh, &branch, (issues, pulls), claims, main).await?;
        }
    }
    Ok(unanswered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rollout::Label;

    /// #1710: in Wave 50 the lead heard "PR #1703 waits for a verify" while
    /// the head had a conflict and the lead held its verify. A pull request
    /// with a conflict, a held verify, or a verify that waits for the merge
    /// of another pull request gets no message after 30 minutes. When it
    /// waits for a verify again, the wait starts again.
    #[test]
    fn no_verify_message_names_a_conflict_a_held_verify_or_a_verify_that_waits_for_a_merge() {
        let wait = VERIFY_WAIT;
        let mut watch = PullWatch::new(wait);
        let pull = |number, issue: u64, mergeable: &str| Pull {
            number,
            branch: format!("worktree-issue-{issue}"),
            head: format!("{issue}a2b3c4d"),
            mergeable: Some(mergeable.into()),
            auto_merge: true,
            ..Pull::default()
        };
        let pulls = [
            pull(40, 12, "CONFLICTING"),
            pull(41, 13, "MERGEABLE"),
            pull(42, 14, "MERGEABLE"),
        ];
        let holds: HashSet<String> = ["verify-issue-13".to_owned()].into();
        let waits = BTreeMap::from([(14, 43)]);
        let (none, no_wait) = (HashSet::new(), BTreeMap::new());
        let start = Instant::now();
        let stops = |news: Vec<PullNews>| -> Vec<(u64, Stop)> {
            news.into_iter().map(|n| (n.told.0, n.told.2)).collect()
        };
        // The conflict goes to the lead: no session holds issue-12.
        let first = watch.news(&pulls, &none, &holds, &waits, start);
        assert_eq!(stops(first), [(40, Stop::Conflict)]);
        let later = start + wait;
        assert!(watch.news(&pulls, &none, &holds, &waits, later).is_empty());
        // The lead frees the verify of 13, and 14 gets its turn: the wait
        // starts now.
        assert!(watch.news(&pulls, &none, &none, &no_wait, later).is_empty());
        let news = watch.news(&pulls, &none, &none, &no_wait, later + wait);
        assert_eq!(stops(news), [(41, Stop::NoVerify), (42, Stop::NoVerify)]);
    }

    /// A fake `gh` in a new folder: it logs each call to `gh.log` there
    /// on one line, then runs `script`.
    fn fake(script: &str) -> (tempfile::TempDir, Gh) {
        let dir = tempfile::tempdir().unwrap();
        let text = format!(
            "#!/bin/sh\nlog=\"$(dirname \"$0\")/gh.log\"\nprintf '%s' \"$*\" | tr '\\n' ' ' >> \"$log\"\n\
             printf '\\n' >> \"$log\"\n{script}\n"
        );
        let gh = Gh::at(isolated::write_program(dir.path().join("gh"), text));
        (dir, gh)
    }

    /// The calls of the fake `gh` in `dir` that start with `call`.
    fn calls(dir: &tempfile::TempDir, call: &str) -> Vec<String> {
        std::fs::read_to_string(dir.path().join("gh.log"))
            .unwrap_or_default()
            .lines()
            .filter(|l| l.starts_with(call))
            .map(str::to_owned)
            .collect()
    }

    fn fix(number: u64) -> Issue {
        Issue {
            number,
            labels: vec![Label {
                name: BROKEN_MAIN.into(),
            }],
            ..Issue::default()
        }
    }

    fn main(gate: Gate) -> MainGate {
        MainGate {
            commit: "5103311abc".into(),
            gate,
            run: None,
            job: None,
        }
    }

    /// 01M5RGM8HBVQMY25H5PYESK17J: an old failed run of a check on `main`
    /// does not make main broken when a newer run of it passed.
    #[test]
    fn the_look_at_main_reads_the_newest_run_of_each_check() {
        let run = |started: &str, conclusion: &str, id: u64| CheckRun {
            status: "completed".into(),
            conclusion: Some(conclusion.into()),
            head_sha: "5103311abc".into(),
            html_url: Some(format!("https://github.com/o/r/actions/runs/{id}/job/7")),
            started_at: Some(started.into()),
            ..CheckRun::default()
        };
        let old = run("2026-10-24T17:01:00Z", "failure", 1);
        let new = run("2026-10-24T17:22:00Z", "success", 2);
        let hygiene = run("2026-10-24T17:05:00Z", "success", 3);
        for runs in [
            vec![old.clone(), new.clone()],
            vec![new.clone(), old.clone()],
        ] {
            let main = MainGate::of(&[runs, vec![hygiene.clone()]]).unwrap();
            assert_eq!(main.gate, Gate::Passed);
            assert!(!main.broken());
        }
        let late = run("2026-10-24T17:40:00Z", "failure", 4);
        let main = MainGate::of(&[vec![late, new, old], vec![hygiene]]).unwrap();
        assert!(main.broken());
        assert_eq!(
            main.run.as_deref(),
            Some("https://github.com/o/r/actions/runs/4")
        );
    }

    /// 01M6GX8CN4ZTV0X40GD5AWPXH9: one commit of `main` that breaks, passes
    /// on a rerun and breaks again on a second rerun gives three keys. A
    /// rerun of a run keeps the run and makes a new job. A second look at
    /// the same run gives the same key.
    #[test]
    fn a_rerun_of_the_same_commit_of_main_gives_a_new_key() {
        let attempt = |conclusion: &str, run: u64, job: u64, started: &str| CheckRun {
            status: "completed".into(),
            conclusion: Some(conclusion.into()),
            head_sha: "5103311abc".into(),
            html_url: Some(format!(
                "https://github.com/o/r/actions/runs/{run}/job/{job}"
            )),
            started_at: Some(started.into()),
            ..CheckRun::default()
        };
        let keys_of = |attempts: &[CheckRun]| {
            let mut watch = MainWatch::default();
            let main = MainGate::of(&[attempts.to_vec()]).unwrap();
            watch.news(&main).map(|news| news.key())
        };
        let first = attempt("failure", 50, 1, "2026-10-24T17:00:00Z");
        let second = attempt("success", 50, 2, "2026-10-24T17:20:00Z");
        let third = attempt("failure", 50, 3, "2026-10-24T17:40:00Z");
        let fourth = attempt("failure", 51, 4, "2026-10-24T18:00:00Z");
        let keys: Vec<Option<String>> = [
            vec![first.clone()],
            vec![first.clone(), second.clone()],
            vec![first.clone(), second.clone(), third.clone()],
            vec![first.clone(), second.clone(), third.clone(), fourth],
        ]
        .iter()
        .map(|attempts| keys_of(attempts))
        .collect();
        let [a, b, c, d] = <[Option<String>; 4]>::try_from(keys).unwrap();
        assert_eq!(a.as_deref(), Some("main-5103311abc-50-1-broken"));
        assert_eq!(b.as_deref(), Some("main-5103311abc-50-2-passes"));
        assert_eq!(c.as_deref(), Some("main-5103311abc-50-3-broken"));
        assert_eq!(d.as_deref(), Some("main-5103311abc-51-4-broken"));
        // The same attempts again: the same key.
        assert_eq!(keys_of(&[first]), a);
    }

    /// 01M6GX8CN4ZTV0X40GD5AWPXH9: `main` requires two checks, and only the
    /// second one runs again. The key of a note that `main` passes holds
    /// the job of the newest run of any check, so the second pass of one
    /// commit gets a key of its own. A second look at the same runs gives
    /// the same key.
    #[test]
    fn a_rerun_of_the_second_check_of_main_gives_a_new_key_for_a_pass() {
        let attempt = |conclusion: &str, run: u64, job: u64, started: &str| CheckRun {
            status: "completed".into(),
            conclusion: Some(conclusion.into()),
            head_sha: "5103311abc".into(),
            html_url: Some(format!(
                "https://github.com/o/r/actions/runs/{run}/job/{job}"
            )),
            started_at: Some(started.into()),
            ..CheckRun::default()
        };
        let gate = attempt("success", 50, 1, "2026-10-24T17:00:00Z");
        let hygiene = [
            attempt("failure", 51, 2, "2026-10-24T17:01:00Z"),
            attempt("success", 51, 3, "2026-10-24T17:20:00Z"),
            attempt("failure", 51, 4, "2026-10-24T17:40:00Z"),
            attempt("success", 51, 5, "2026-10-24T18:00:00Z"),
        ];
        let mut watch = MainWatch::default();
        let mut keys = Vec::new();
        for count in 1..=hygiene.len() {
            let main = MainGate::of(&[vec![gate.clone()], hygiene[..count].to_vec()]).unwrap();
            keys.push(watch.news(&main).map(|news| news.key()));
        }
        assert_eq!(
            keys,
            [
                Some("main-5103311abc-51-2-broken".to_owned()),
                Some("main-5103311abc-51-3-passes".to_owned()),
                Some("main-5103311abc-51-4-broken".to_owned()),
                Some("main-5103311abc-51-5-passes".to_owned()),
            ]
        );
        // The same runs again: the same key.
        let main = MainGate::of(&[vec![gate], hygiene.to_vec()]).unwrap();
        let again = MainWatch::default().news(&main).map(|news| news.key());
        assert_eq!(again.as_deref(), Some("main-5103311abc-51-5-passes"));
    }

    /// riff cannot file the fix item: the lead still gets the message
    /// that main is broken, with the reason (01M51QEYKTD7ZEB0YA0C3MQTPN).
    #[test]
    fn a_fix_item_that_riff_cannot_file_still_gives_the_message() {
        let (dir, gh) = fake(
            "case \"$1 $2\" in\n\
             \"label create\") echo 'no right' >&2; exit 1 ;;\n\
             \"issue create\") echo 'HTTP 403' >&2; exit 1 ;;\n\
             *) echo '[]' ;;\n\
             esac",
        );
        let news = MainNews::Broken {
            main: main(Gate::Ended("failure".into())),
            first: true,
        };
        let (body, wake) = main_steps(&gh, "o/r", &[], &[], &HashSet::new(), &news)
            .unwrap()
            .unwrap();
        assert!(wake);
        assert!(
            body.starts_with("main is broken: the full check failed at commit 5103311.")
                && body.contains("riff could not file the fix item: ")
                && body.contains("HTTP 403")
                && body.ends_with("File an issue with the label broken-main."),
            "{body}"
        );
        // A label that riff cannot make does not stop the try.
        assert_eq!(calls(&dir, "issue create").len(), 1);
    }

    /// The lead of another person filed a fix item after the look read
    /// the issues: riff reads the label again, and puts a comment on that
    /// item. It files no second one (01M51QEYPGD6W345MZ4F85C38F).
    #[test]
    fn a_fix_item_of_another_lead_gets_a_comment_and_no_second_item() {
        let (dir, gh) = fake(
            "case \"$*\" in\n\
             *'--label broken-main'*) echo '[{\"number\":902,\"labels\":[{\"name\":\"broken-main\"}]}]' ;;\n\
             esac",
        );
        let news = MainNews::Broken {
            main: main(Gate::Ended("failure".into())),
            first: true,
        };
        let (body, _) = main_steps(&gh, "o/r", &[], &[], &HashSet::new(), &news)
            .unwrap()
            .unwrap();
        assert!(body.contains("The fix item is #902"), "{body}");
        assert_eq!(calls(&dir, "issue comment 902").len(), 1);
        assert!(calls(&dir, "issue create").is_empty());
    }

    /// When main passes, riff closes only the free fix items: a fix item
    /// that a session claims, or that an open pull request names, stays
    /// open (01M51QEYVQN74Q1VDTVHB0445Q). With no broken main before and
    /// nothing to close, the lead gets no post.
    #[test]
    fn a_pass_closes_only_the_free_fix_items() {
        let (dir, gh) = fake("exit 0");
        let issues = [fix(7), fix(8), fix(9)];
        let pulls = [Pull {
            number: 40,
            branch: "worktree-issue-8".into(),
            ..Pull::default()
        }];
        let claims: HashSet<String> = ["issue-9".to_owned()].into();
        let news = MainNews::Passed {
            main: main(Gate::Passed),
            again: true,
        };
        let (body, wake) = main_steps(&gh, "o/r", &issues, &pulls, &claims, &news)
            .unwrap()
            .unwrap();
        assert!(!wake);
        assert_eq!(
            body,
            "main passes again: the full check passed at commit 5103311. riff closed the fix item #7. \
             The fix items #8, #9 stay open: a session claims each, or a pull request names it."
        );
        let closes = calls(&dir, "issue close");
        assert_eq!(closes.len(), 1, "{closes:?}");
        assert!(
            closes[0].starts_with("issue close 7 --repo o/r --comment "),
            "{closes:?}"
        );

        let (dir, gh) = fake("exit 0");
        let news = MainNews::Passed {
            main: main(Gate::Passed),
            again: false,
        };
        let steps = main_steps(&gh, "o/r", &[], &[], &HashSet::new(), &news).unwrap();
        assert_eq!(steps, None);
        assert!(calls(&dir, "").is_empty());
    }
}
