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
//!
//! # A pull request that stops
//!
//! A pull request can stop on its way to the merge with no sign: a
//! conflict with the default branch, or a verify request that no session
//! takes. The same look sees the open pull requests, and the claims of
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
//! ```
//!
//! Each message comes one time for each pull request, head commit and
//! state (01M49Q31FASDM7CG3JEGPYCZB9). The `riff mcp` of the lead keeps
//! what it told in memory. So after a new start of it, a state can come
//! one more time, and the wait of 30 minutes starts again.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use riff_core::name::{SessionUri, Who};
use riff_core::selector::Selector;
use riff_core::wire::{ItemFact, Kind, PullFact, PullState, Unanswered};

use crate::api::Api;
use crate::pr::Gh;
use crate::rollout::{Issue, Pull, Verify, branch_issue, claims, needs};
use crate::text;

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

/// Reads the open issues and the open pull requests of `repo` with `gh`.
pub fn read_forge(gh: &Gh, repo: &str) -> Result<(Vec<Issue>, Vec<Pull>)> {
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
    /// claims `claims` of the repository:
    ///
    /// - A pull request of an item with auto-merge on and a conflict
    ///   ([`Pull::conflict`]) goes to the session that holds the item, or
    ///   to the lead when no session holds it (01M49Q30XMVRFX42YTM1PHX0RZ).
    /// - A pull request that waits for a verify ([`Verify::Asked`]) with
    ///   no `verify-` claim for the wait of this watch goes to the lead
    ///   (01M49Q316RXNATJP587DWGDNCD). A claim starts the wait again.
    ///
    /// Each state comes one time, until the head of the pull request
    /// changes.
    ///
    /// ```
    /// use std::collections::HashSet;
    /// use std::time::{Duration, Instant};
    /// use riff::look::PullWatch;
    /// use riff::rollout::Pull;
    ///
    /// let mut watch = PullWatch::new(Duration::from_secs(30 * 60));
    /// let pull = Pull {
    ///     number: 40,
    ///     branch: "worktree-issue-12".into(),
    ///     head: "1a2b3c4d".into(),
    ///     mergeable: Some("CONFLICTING".into()),
    ///     auto_merge: true,
    ///     ..Pull::default()
    /// };
    /// let held: HashSet<String> = ["issue-12".to_owned()].into();
    /// let start = Instant::now();
    ///
    /// // The conflict goes to the holder of the item, one time.
    /// let news = watch.news(&[pull.clone()], &held, start);
    /// assert_eq!(news.len(), 1);
    /// assert_eq!(news[0].to.as_deref(), Some("issue-12"));
    /// assert!(watch.news(&[pull.clone()], &held, start).is_empty());
    ///
    /// // 30 minutes with no verify claim: one message to the lead.
    /// let later = start + Duration::from_secs(30 * 60);
    /// let news = watch.news(&[pull.clone()], &held, later);
    /// assert_eq!(news.len(), 1);
    /// assert_eq!(news[0].to, None);
    /// assert!(news[0].body.contains("no session claims verify-issue-12"));
    /// assert!(watch.news(&[pull.clone()], &held, later).is_empty());
    ///
    /// // A new head is a new state: the conflict comes again, to the
    /// // lead now that no session holds the item.
    /// let pushed = Pull { head: "5e6f7a8b".into(), ..pull };
    /// let news = watch.news(&[pushed], &HashSet::new(), later);
    /// assert_eq!(news.len(), 1);
    /// assert_eq!(news[0].to, None);
    /// assert!(news[0].body.contains("No session holds issue-12"));
    /// ```
    pub fn news(&mut self, pulls: &[Pull], claims: &HashSet<String>, now: Instant) -> Vec<PullNews> {
        let heads: HashSet<(u64, &str)> = pulls.iter().map(|p| (p.number, p.head.as_str())).collect();
        self.told
            .retain(|(n, head, _)| heads.contains(&(*n, head.as_str())));
        self.since
            .retain(|(n, head), _| heads.contains(&(*n, head.as_str())));
        let mut news = Vec::new();
        for pull in pulls {
            let Some(issue) = branch_issue(&pull.branch) else {
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
            let waits = pull.verify() == Some(Verify::Asked)
                && !claims.contains(&format!("verify-{item}"));
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

/// Posts `news` as a message, as the person of the lead `me`: to the
/// session that holds its item, or to the lead. The lead does not see its
/// own posts, so the person posts it.
async fn tell_news(api: &Api, me: &SessionUri, news: &PullNews) -> Result<()> {
    let person = SessionUri::new(Who::new(me.who().user(), None)?, me.place().clone());
    let to: Selector = match (&news.to, me.who().session()) {
        (Some(item), _) => format!("claim={item},repo={}", me.place().repo_text()).parse()?,
        (None, Some(session)) => format!("session={session}").parse()?,
        (None, None) => anyhow::bail!("the lead has no session"),
    };
    api.post(&person, me.default_thread().as_ref(), &[to], &news.body, Kind::Message)
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
/// stop ([`PullWatch::news`]). It gives the blocks that are unanswered
/// now.
pub async fn once(
    api: &Api,
    me: &SessionUri,
    gh: &Arc<Gh>,
    wake: Duration,
    notify: Option<&Notifier>,
    watch: &mut PullWatch,
) -> Result<Vec<Unanswered>> {
    let (gh, repo) = (gh.clone(), me.place().repo_text());
    let forge = tokio::task::spawn_blocking(move || read_forge(&gh, &repo)).await?;
    // A forge that does not answer stops no look at the blocks.
    let pulls = match forge {
        Ok((issues, pulls)) => {
            api.item_facts(me, item_facts(&issues, &pulls), true).await?;
            Some(pulls)
        }
        Err(_) => None,
    };
    let unanswered = api.look_blocks(me, wake.as_secs()).await?;
    if let Some(notifier) = notify {
        for block in &unanswered {
            notifier.show(block);
        }
    }
    if let Some(pulls) = pulls {
        let claims = claims(&api.who(me, false).await?);
        for news in watch.news(&pulls, &claims, Instant::now()) {
            if let Err(e) = tell_news(api, me, &news).await {
                watch.untell(&news);
                return Err(e);
            }
        }
    }
    Ok(unanswered)
}

/// Runs the look while the tools run. It acts only while `me` is the
/// lead. It prints an error once, not again until the error changes.
pub async fn run(api: Api, me: impl Fn() -> SessionUri, settings: Option<PathBuf>) {
    let (gh, notifier) = (Arc::new(Gh::default()), Notifier::default());
    let mut watch = PullWatch::default();
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
            once(&api, &me, &gh, wake, notify, &mut watch).await?;
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
