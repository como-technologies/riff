//! The look of the lead: the facts of the forge, and the blocks with no
//! answer.
//!
//! # Design
//!
//! The server has no credential of the forge (01M41FZP2C4Z4J6WKRXZ5B31EH).
//! So the clients tell it what they see: `riff pr open`, `riff verify`
//! and `riff pr wait` send the fact of their item, and the `riff mcp` of
//! the lead sends the facts of each open item once each [`LOOK_EVERY`].
//! The server makes `waiting` from them (01M41FZP9A50CH4A2VX344DW49).
//!
//! The same look checks the blocks of the sessions of the user. The
//! time is the setting `lead.wake` of the machine of the lead
//! ([`crate::settings::lead_wake`]):
//!
//! ```mermaid
//! sequenceDiagram
//!     participant W as blocked session
//!     participant S as riff-server
//!     participant L as riff mcp of the lead
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

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use riff_core::name::SessionUri;
use riff_core::wire::{ItemFact, PullFact, PullState, Unanswered};

use crate::api::Api;
use crate::pr::Gh;
use crate::rollout::{Issue, Pull, Verify, branch_issue, needs};

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
        let (Some(n), Some(verify)) = (branch_issue(&pull.branch), pull.verify()) else {
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

/// Reads the open issues and the open pull requests of `repo` with `gh`,
/// and makes their facts.
pub fn forge_facts(gh: &Gh, repo: &str) -> Result<Vec<ItemFact>> {
    let pulls = crate::rollout::pulls(gh, repo)?;
    let issues: Vec<Issue> = gh.json(&[
        "issue",
        "list",
        "--repo",
        repo,
        "--state",
        "open",
        "--limit",
        "1000",
        "--json",
        "number,body,comments",
    ])?;
    Ok(item_facts(&issues, &pulls))
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
/// one. It gives the blocks that are unanswered now.
pub async fn once(
    api: &Api,
    me: &SessionUri,
    gh: &Arc<Gh>,
    wake: Duration,
    notify: Option<&Notifier>,
) -> Result<Vec<Unanswered>> {
    let (gh, repo) = (gh.clone(), me.place().repo_text());
    let facts = tokio::task::spawn_blocking(move || forge_facts(&gh, &repo)).await?;
    // A forge that does not answer stops no look at the blocks.
    if let Ok(facts) = facts {
        api.item_facts(me, facts, true).await?;
    }
    let unanswered = api.look_blocks(me, wake.as_secs()).await?;
    if let Some(notifier) = notify {
        for block in &unanswered {
            notifier.show(block);
        }
    }
    Ok(unanswered)
}

/// Runs the look while the tools run. It acts only while `me` is the
/// lead. It prints an error once, not again until the error changes.
pub async fn run(api: Api, me: impl Fn() -> SessionUri, settings: Option<PathBuf>) {
    let (gh, notifier) = (Arc::new(Gh::default()), Notifier::default());
    let mut last_error = None;
    loop {
        tokio::time::sleep(LOOK_EVERY).await;
        if api.left() {
            continue;
        }
        let me = me();
        let step = async {
            if !is_lead(&api, &me).await? {
                return anyhow::Ok(());
            }
            let (wake, notify) = read_settings(settings.as_deref());
            // A machine with no desktop gets no notification.
            let notify = (notify && notifier.here()).then_some(&notifier);
            once(&api, &me, &gh, wake, notify).await?;
            Ok(())
        };
        match step.await {
            Ok(()) => last_error = None,
            Err(e) => {
                let e = format!("{e:#}");
                if last_error.as_ref() != Some(&e) {
                    eprintln!("riff: the look of the lead: {e}");
                }
                last_error = Some(e);
            }
        }
    }
}

/// True when `me` is the lead now.
async fn is_lead(api: &Api, me: &SessionUri) -> Result<bool> {
    let sessions = api.who(me, false).await?;
    Ok(sessions
        .iter()
        .any(|s| s.uri.who() == me.who() && s.uri.lead()))
}

/// The wake time and the notify switch of the settings at `path`, or of
/// this machine. A setting that does not read gives its default.
fn read_settings(path: Option<&Path>) -> (Duration, bool) {
    let path = path
        .map(Path::to_path_buf)
        .or_else(|| crate::settings::path().ok());
    let minutes = path
        .as_deref()
        .and_then(|p| crate::settings::lead_wake(p).ok())
        .unwrap_or(crate::settings::LEAD_WAKE);
    let notify = path
        .as_deref()
        .and_then(|p| crate::settings::lead_notify(p).ok())
        .unwrap_or(true);
    (Duration::from_secs(minutes * 60), notify)
}
