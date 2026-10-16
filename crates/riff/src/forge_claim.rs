//! The claims of riff 3.0 on the forge (01M4Z60B8C3W5TMPVWNFXD1SYN,
//! 01M4Z60B8DYAVEQF6F8N9QP7MS, 01M519Z4CT0PRXPRM22037Z5RY).
//!
//! # Design
//!
//! A claim is the ref `refs/riff/claims/ITEM` on the forge, for example
//! `refs/riff/claims/issue-12`. It is not a branch. The daemon makes each
//! claim and each release in the main clone of the repository, with the
//! git sign-in of the person: each step is one `git` call there.
//!
//! | Step | git |
//! |---|---|
//! | The commit of a claim | `git mktree` gives the empty tree. `git commit-tree --no-gpg-sign` makes a commit with no parent, with the message `riff claim ITEM` and the trailers [`Claim::message`]. |
//! | A claim | `git push --porcelain --force-with-lease=REF: origin COMMIT:REF`. The empty value of the lease means: the ref does not exist yet. |
//! | A release | `git push --porcelain --force-with-lease=REF:COMMIT origin :REF`: a delete with the lease of the claim commit. |
//! | The claims of the forge | `git ls-remote origin 'refs/riff/claims/*'` ([`list`], 01M519Z4F7Z87BXQYT7F8EABCN) |
//! | The facts of a claim | the trailers of its commit ([`Claim::read`]). When the clone lacks the commit, riff fetches the ref with no ref and no `FETCH_HEAD` ([`facts`]). |
//!
//! The forge compares the lease on the server, so only one session gets
//! an item. A lease that fails in the client (`stale info`) or on the
//! server (`cannot lock ref`) means: another session holds the item
//! (01M4Z60B8EB7A44A41CWWP7G8K). A claim has no time limit
//! (01M4Z60B8FQ3GMWQH12CXE0JS9). In a public repository, each claim ref
//! and each claim commit is public, also after the release: a claim
//! commit holds no secret (01M4Z60B8G1VWGNKBH8ZJGJ7QJ).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant A as the daemon of Mike
//!     participant G as the forge
//!     participant B as the daemon of Brett
//!     A->>G: push refs/riff/claims/issue-12, lease: no ref
//!     B->>G: push refs/riff/claims/issue-12, lease: no ref
//!     G-->>A: a new ref: Took::Won(COMMIT)
//!     G-->>B: refused: Took::Held
//!     A->>G: delete refs/riff/claims/issue-12, lease: COMMIT
//!     G-->>A: Freed::Deleted
//! ```
//!
//! riff builds no shell string: the item is one part of one argument,
//! and [`reference()`] takes only the letters, the digits and `-`, `_`
//! and `.` (01M4Z60B8KDXD190RWC7D184Q3).
//!
//! # Example
//!
//! The message of a claim commit, and its facts again:
//!
//! ```
//! use riff::daemon_key::Key;
//! use riff::forge_claim::Claim;
//!
//! let key = Key::from_bytes(&[1; 32]).public();
//! let claim = Claim {
//!     item: "issue-12".into(),
//!     user: "mike".into(),
//!     session: "1a2b3c4d".into(),
//!     key,
//!     time: "2026-10-16T12:00:00Z".into(),
//! };
//! let message = claim.message();
//! assert!(message.starts_with("riff claim issue-12\n\nRiff-Item: issue-12\n"));
//! assert_eq!(Claim::read(&message), Some(claim));
//! assert_eq!(Claim::read("fix the tests"), None);
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail};

use crate::daemon_key::Public;
use crate::text;

/// The start of the ref of each claim.
pub const REFS: &str = "refs/riff/claims/";

/// The subject of each claim commit, before the item.
const SUBJECT: &str = "riff claim ";

/// The facts of a claim: the trailers of its commit
/// (01M519Z4CT0PRXPRM22037Z5RY).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// The work item, for example `issue-12`.
    pub item: String,
    /// The user of the daemon that claims.
    pub user: String,
    /// The session ID of the session that holds the item.
    pub session: String,
    /// The public key of the daemon that claims.
    pub key: Public,
    /// The time of the claim: UTC, RFC 3339.
    pub time: String,
}

impl Claim {
    /// The claim of `item` by the session `session` of the daemon of
    /// `key` and of the user `user`, now.
    pub fn now(item: &str, user: &str, session: &str, key: Public) -> Claim {
        Claim {
            item: item.to_owned(),
            user: user.to_owned(),
            session: session.to_owned(),
            key,
            time: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        }
    }

    /// The message of the claim commit: `riff claim ITEM`, a blank line,
    /// and the trailers `Riff-Item`, `Riff-User`, `Riff-Session`,
    /// `Riff-Key` and `Riff-Time`.
    pub fn message(&self) -> String {
        format!(
            "{SUBJECT}{item}\n\nRiff-Item: {item}\nRiff-User: {user}\nRiff-Session: {session}\nRiff-Key: {key}\nRiff-Time: {time}\n",
            item = self.item,
            user = self.user,
            session = self.session,
            key = self.key,
            time = self.time,
        )
    }

    /// The facts of the commit message `message`. `None` when it is not
    /// the message of a claim commit: no subject `riff claim ITEM`, a
    /// missing trailer, a `Riff-Item` of another item, or a `Riff-Key`
    /// that is no key.
    pub fn read(message: &str) -> Option<Claim> {
        let (subject, rest) = message.split_once('\n').unwrap_or((message, ""));
        let item = subject.trim().strip_prefix(SUBJECT)?;
        let mut trailers: BTreeMap<&str, &str> = BTreeMap::new();
        for line in rest.lines() {
            if let Some((name, value)) = line.split_once(": ") {
                trailers.entry(name.trim()).or_insert(value.trim());
            }
        }
        let trailer = |name: &str| trailers.get(name).map(|v| (*v).to_owned());
        let claim = Claim {
            item: trailer("Riff-Item")?,
            user: trailer("Riff-User")?,
            session: trailer("Riff-Session")?,
            key: Public::parse(trailers.get("Riff-Key")?)?,
            time: trailer("Riff-Time")?,
        };
        (claim.item == item).then_some(claim)
    }
}

/// What a claim gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Took {
    /// The forge has the new ref: the session holds the item. The value
    /// is the claim commit: the lease of its release.
    Won(String),
    /// The lease failed: another session holds the item.
    Held,
}

/// What a release gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freed {
    /// The forge deleted the ref.
    Deleted,
    /// The lease failed: the ref is gone, or it holds another claim.
    /// The release changed nothing.
    NotAt,
}

/// The ref of the claim of `item`. It fails for an item that is not one
/// part of a ref: riff takes only the letters, the digits and `-`, `_`
/// and `.`, with no `.` at the start, no `..` and no `.lock` at the end.
///
/// ```
/// use riff::forge_claim::reference;
///
/// assert_eq!(reference("verify-issue-12")?, "refs/riff/claims/verify-issue-12");
/// assert!(reference("issue 12").is_err());
/// assert!(reference("../main").is_err());
/// assert!(reference("-x").is_err());
/// assert!(reference("x.lock").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn reference(item: &str) -> Result<String> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || "-_.".contains(c);
    let good = !item.is_empty()
        && item.chars().all(allowed)
        && !item.starts_with(['.', '-'])
        && !item.contains("..")
        && !item.ends_with(".lock");
    if !good {
        bail!("{}", text::forge_claim_bad_item(item));
    }
    Ok(format!("{REFS}{item}"))
}

/// Runs `git ARGS` in the clone `main`, with no input. git asks for no
/// sign-in on a terminal (`GIT_TERMINAL_PROMPT=0`): a push or a fetch
/// with no sign-in fails at once (01M527NXZ6KRFAA641ZKPVGC4V).
fn git(main: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(main)
        .stdin(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .with_context(|| text::no_program("git"))
}

/// The stdout of `git ARGS` in `main`, trimmed. It fails when git fails,
/// with the error of git.
fn git_text(main: &Path, args: &[&str]) -> Result<String> {
    let out = git(main, args)?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Makes the claim commit of `claim` in the clone `main`: the empty
/// tree, no parent, no signature, and [`Claim::message`]. It gives the
/// ID of the commit. git takes the author and the committer from the
/// config of the person.
pub fn commit(main: &Path, claim: &Claim) -> Result<String> {
    reference(&claim.item)?;
    let tree = git_text(main, &["mktree"])?;
    let message = claim.message();
    git_text(
        main,
        &["commit-tree", "--no-gpg-sign", "-m", &message, &tree],
    )
}

/// What `git push --porcelain` says of the ref `reference`: `None` when
/// the push changed it, or the reason when the forge refused it.
fn refused(stdout: &str, reference: &str) -> Option<Option<String>> {
    let to = format!(":{reference}\t");
    stdout.lines().find(|l| l.contains(&to)).map(|line| {
        line.starts_with('!')
            .then(|| line.rsplit('\t').next().unwrap_or_default().to_owned())
    })
}

/// True when the reason `why` of a refused push is a failed lease: in
/// the client (`stale info`) or on the server (`cannot lock ref`, or a
/// ref that is there already). riff reads it as: another session holds
/// the item (01M4Z60B8EB7A44A41CWWP7G8K).
///
/// ```
/// use riff::forge_claim::lease_failed;
///
/// assert!(lease_failed("[rejected] (stale info)"));
/// assert!(lease_failed("[remote rejected] (cannot lock ref 'refs/riff/claims/issue-12': reference already exists)"));
/// assert!(!lease_failed("[remote rejected] (permission denied)"));
/// ```
pub fn lease_failed(why: &str) -> bool {
    [
        "stale info",
        "cannot lock ref",
        "already exists",
        "failed to update ref",
    ]
    .iter()
    .any(|part| why.contains(part))
}

/// Pushes with `lease` the change `spec` of the ref `reference` of the
/// clone `main` to `origin`. It gives `true` when the forge changed the
/// ref, and `false` when the lease failed. Each other fault of the push
/// fails, with the error of git.
fn push(main: &Path, reference: &str, lease: &str, spec: &str) -> Result<bool> {
    let lease = format!("--force-with-lease={reference}:{lease}");
    let args = ["push", "--porcelain", "--no-verify", &lease, "origin", spec];
    let out = git(main, &args)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    match refused(&stdout, reference) {
        Some(None) if out.status.success() => Ok(true),
        Some(Some(why)) if lease_failed(&why) => Ok(false),
        _ => bail!(
            "git {}: {}{}",
            args.join(" "),
            stdout.trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        ),
    }
}

/// Claims the item of `claim` on the forge of the clone `main`: the
/// claim commit ([`commit`]), and a push of its ref with the lease of no
/// ref (01M4Z60B8DYAVEQF6F8N9QP7MS).
pub fn take(main: &Path, claim: &Claim) -> Result<Took> {
    let reference = reference(&claim.item)?;
    let commit = commit(main, claim)?;
    let spec = format!("{commit}:{reference}");
    Ok(if push(main, &reference, "", &spec)? {
        Took::Won(commit)
    } else {
        Took::Held
    })
}

/// Releases the claim of `item` with the lease of its claim commit
/// `commit`: a delete of its ref on the forge of the clone `main`.
pub fn release(main: &Path, item: &str, commit: &str) -> Result<Freed> {
    let reference = reference(item)?;
    if !is_commit_id(commit) {
        bail!("{}", text::forge_claim_bad_commit(commit));
    }
    let spec = format!(":{reference}");
    Ok(if push(main, &reference, commit, &spec)? {
        Freed::Deleted
    } else {
        Freed::NotAt
    })
}

/// True when `id` is a full commit ID: 40 or 64 hex digits.
fn is_commit_id(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.chars().all(|c| c.is_ascii_hexdigit())
}

/// Each claim on the forge of the clone `main`: its item, and its claim
/// commit. riff asks the forge, so the list is true now
/// (01M519Z4F7Z87BXQYT7F8EABCN).
pub fn list(main: &Path) -> Result<BTreeMap<String, String>> {
    let pattern = format!("{REFS}*");
    let out = git_text(main, &["ls-remote", "origin", &pattern])?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let (commit, name) = line.split_once('\t')?;
            let item = name.trim().strip_prefix(REFS)?;
            (is_commit_id(commit.trim()) && reference(item).is_ok())
                .then(|| (item.to_owned(), commit.trim().to_owned()))
        })
        .collect())
}

/// The facts of the claim commit `commit` of `item`, in the clone
/// `main`. When the clone lacks the commit, riff fetches the ref of the
/// claim with no ref and no `FETCH_HEAD`. `None` when the commit is not
/// a claim commit of `item` ([`Claim::read`]).
pub fn facts(main: &Path, item: &str, commit: &str) -> Result<Option<Claim>> {
    let reference = reference(item)?;
    if !is_commit_id(commit) {
        bail!("{}", text::forge_claim_bad_commit(commit));
    }
    let object = format!("{commit}^{{commit}}");
    if !git(main, &["cat-file", "-e", &object])?.status.success() {
        git_text(
            main,
            &[
                "fetch",
                "-q",
                "--no-tags",
                "--no-write-fetch-head",
                "origin",
                &reference,
            ],
        )?;
    }
    let raw = git_text(main, &["cat-file", "commit", commit])?;
    let message = raw.split_once("\n\n").map(|(_, m)| m).unwrap_or_default();
    Ok(Claim::read(message).filter(|claim| claim.item == item))
}

/// One claim on the forge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The work item.
    pub item: String,
    /// The claim commit: the lease of its release.
    pub commit: String,
    /// Its facts. `None` when the commit is not a claim commit of riff.
    pub claim: Option<Claim>,
}

/// Each claim on the forge of the clone `main`, with its facts
/// ([`list`], [`facts`]).
pub fn found(main: &Path) -> Result<Vec<Found>> {
    list(main)?
        .into_iter()
        .map(|(item, commit)| {
            let claim = facts(main, &item, &commit)?;
            Ok(Found {
                item,
                commit,
                claim,
            })
        })
        .collect()
}

/// The time with no link to the daemon of a claim after which the daemon
/// of the lead of its user deletes the claim (01M519Z4M0F2RY0VDQNB2QFAPJ):
/// 15 minutes.
pub const OFFLINE_MS: i64 = 15 * 60 * 1000;

/// What the daemon of a lead knows of the link to the daemon of a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    /// True while a link to that daemon runs.
    pub online: bool,
    /// The last time that a link to that daemon ran, when one ran since
    /// the start of this daemon.
    pub seen_ms: Option<i64>,
}

/// Why the daemon of the lead deletes a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stale {
    /// The daemon of the session wrote the end of the session.
    Ended,
    /// This daemon had no link to the daemon of the session for this
    /// many minutes.
    Offline(i64),
}

/// Whether the daemon of the lead of `user` deletes `claim`
/// (01M519Z4M0F2RY0VDQNB2QFAPJ, 01M4Z60B8FFRMT3DVG8H1CCZ13). `me` is the
/// key of this daemon, `state` its riff of peers, `link` the link to the
/// daemon of the claim, `since_ms` the start of this daemon and `now_ms`
/// the time now.
///
/// | The claim | Its session | The link to its daemon | The daemon of the lead |
/// |---|---|---|---|
/// | of another user | each | each | keeps it: it holds |
/// | of the user | ended: its daemon wrote the end | each | deletes it |
/// | of the user, of this daemon | runs, or no record | — | keeps it: this daemon releases it ([`crate::daemon`]) |
/// | of the user, of another daemon | runs, or no record | up, or down for less than 15 minutes | keeps it |
/// | of the user, of another daemon | runs, or no record | down for 15 minutes | deletes it |
///
/// With no link since the start of this daemon, the time counts from
/// that start.
///
/// ```
/// use riff::daemon_key::Key;
/// use riff::forge_claim::{Claim, Link, OFFLINE_MS, Stale, stale};
/// use riff::peer::state::State;
///
/// let (me, other) = (Key::from_bytes(&[1; 32]).public(), Key::from_bytes(&[2; 32]).public());
/// let state = State::new(me);
/// let claim = Claim::now("issue-12", "mike", "s1", other);
/// let down = Link { online: false, seen_ms: Some(1_000) };
/// assert_eq!(stale(&claim, "mike", &me, &state, down, 0, 1_000 + OFFLINE_MS), Some(Stale::Offline(15)));
/// assert_eq!(stale(&claim, "mike", &me, &state, down, 0, OFFLINE_MS), None);
/// assert_eq!(stale(&claim, "brett", &me, &state, down, 0, 10 * OFFLINE_MS), None);
/// let up = Link { online: true, seen_ms: None };
/// assert_eq!(stale(&claim, "mike", &me, &state, up, 0, 10 * OFFLINE_MS), None);
/// ```
pub fn stale(
    claim: &Claim,
    user: &str,
    me: &Public,
    state: &crate::peer::state::State,
    link: Link,
    since_ms: i64,
    now_ms: i64,
) -> Option<Stale> {
    if claim.user != user {
        return None;
    }
    if state
        .run(&claim.key, &claim.session)
        .is_some_and(|run| run.body.ended)
    {
        return Some(Stale::Ended);
    }
    if claim.key == *me || link.online {
        return None;
    }
    let down = now_ms - link.seen_ms.unwrap_or(since_ms);
    (down >= OFFLINE_MS).then_some(Stale::Offline(down / 60_000))
}

/// Whether the daemon of `me`, with its riff of peers `state`, may
/// delete the claim of the facts `claim` for `riff release ITEM --forge`
/// (01M519Z4PESWFV8Y03SKTSGV4Q): the owner deletes each claim, and a
/// member deletes a claim of its own user. It gives the reason of a
/// refusal. A commit that is not a claim commit of riff has no user:
/// only the owner deletes it.
///
/// ```
/// use riff::daemon_key::Key;
/// use riff::forge_claim::{Claim, may_free};
/// use riff::peer::state::State;
///
/// let owner = Key::from_bytes(&[1; 32]).public();
/// let state = State::new(owner);
/// let claim = Claim::now("issue-12", "brett", "s1", owner);
/// assert!(may_free(&state, &owner, Some(&claim)).is_ok());
/// assert!(may_free(&state, &owner, None).is_ok());
/// ```
pub fn may_free(
    state: &crate::peer::state::State,
    me: &Public,
    claim: Option<&Claim>,
) -> std::result::Result<(), String> {
    if state.riff() == *me {
        return Ok(());
    }
    let mine = state.user_of(me);
    match claim {
        Some(claim) if mine == Some(claim.user.as_str()) => Ok(()),
        Some(claim) => Err(text::forge_release_not_yours(
            &claim.item,
            &claim.user,
            mine,
        )),
        None => Err(text::FORGE_RELEASE_NOT_RIFF.to_owned()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::daemon_key::Key;
    use crate::peer::body::{Body, Member, Riff, Session};
    use crate::peer::record::{Id, Record, check};
    use crate::peer::state::State;

    /// The state of the riff of the owner `owner` (mike) with the member
    /// `brett`: the records `riff` and `member`, and more records of the
    /// owner from `more`.
    fn riff_with(owner: &Key, brett: Public, more: &[Body]) -> State {
        let mut state = State::new(owner.public());
        let mut bodies = vec![
            Body::Riff(Riff {
                user: "mike".into(),
                host: None,
            }),
            Body::Member(Member {
                key: brett,
                user: "brett".into(),
                host: "kadomony".into(),
            }),
        ];
        bodies.extend(more.iter().cloned());
        let mut before = Id::ZERO;
        for (seq, body) in (1..).zip(bodies) {
            let record = Record::sign(owner, owner.public(), seq, before, seq, &body).unwrap();
            before = record.id();
            state.keep(&check(record.bytes().to_vec(), &state).unwrap());
        }
        state
    }

    /// 01M519Z4PESWFV8Y03SKTSGV4Q: a member deletes a claim of its own
    /// user, and not one of another user. The owner deletes each claim.
    #[test]
    fn a_member_frees_a_claim_of_its_user_and_the_owner_frees_each_claim() {
        let owner = Key::from_bytes(&[1; 32]);
        let brett = Key::from_bytes(&[2; 32]).public();
        let state = riff_with(&owner, brett, &[]);
        let of_mike = Claim::now("issue-12", "mike", "s1", owner.public());
        let of_brett = Claim::now("issue-13", "brett", "s2", brett);
        assert_eq!(may_free(&state, &brett, Some(&of_brett)), Ok(()));
        let refused = may_free(&state, &brett, Some(&of_mike)).unwrap_err();
        assert!(refused.contains("mike"), "{refused}");
        assert!(may_free(&state, &brett, None).is_err());
        for claim in [Some(&of_mike), Some(&of_brett), None] {
            assert_eq!(may_free(&state, &owner.public(), claim), Ok(()));
        }
        // A key that is not in the riff frees nothing.
        let stranger = Key::from_bytes(&[3; 32]).public();
        assert!(may_free(&state, &stranger, Some(&of_brett)).is_err());
    }

    /// 01M519Z4M0F2RY0VDQNB2QFAPJ: the daemon of the lead deletes a claim
    /// of its user after the end record of its session, and after 15
    /// minutes with no link to its daemon, on a test clock. A claim of
    /// another user holds.
    #[test]
    fn the_lead_deletes_a_claim_of_its_user_after_the_end_or_15_minutes_offline() {
        let owner = Key::from_bytes(&[1; 32]);
        let brett = Key::from_bytes(&[2; 32]).public();
        let session = |id: &str, ended: bool| {
            Body::Session(Box::new(Session {
                session: format!("riff://mike@thelio/o/r?session={id}#issue-12")
                    .parse()
                    .unwrap(),
                worker: true,
                lead: None,
                claims: vec!["issue-12".into()],
                status: None,
                ended,
            }))
        };
        // The records of the owner: the session e1 ended, r1 runs.
        let state = riff_with(
            &owner,
            brett,
            &[
                session("e1", false),
                session("e1", true),
                session("r1", false),
            ],
        );
        // `me` is the daemon of the lead. The claims are of the daemon
        // of the owner.
        let me = Key::from_bytes(&[4; 32]).public();
        let of = |id: &str, user: &str, key: Public| Claim::now("issue-12", user, id, key);
        let up = Link {
            online: true,
            seen_ms: None,
        };
        let down_at = |ms| Link {
            online: false,
            seen_ms: Some(ms),
        };
        let never = Link {
            online: false,
            seen_ms: None,
        };
        let start = 1_000_000;
        // The end record counts at once, also while the link is up.
        let ended = of("e1", "mike", owner.public());
        assert_eq!(
            stale(&ended, "mike", &me, &state, up, start, start),
            Some(Stale::Ended)
        );
        // A session that runs, of a daemon with a link: it holds.
        let runs = of("r1", "mike", owner.public());
        assert_eq!(
            stale(
                &runs,
                "mike",
                &me,
                &state,
                up,
                start,
                start + 10 * OFFLINE_MS
            ),
            None
        );
        // The link ended 14 minutes ago: it holds. At 15 minutes: it goes.
        let ended_at = start + 60_000;
        let at = |minutes: i64| ended_at + minutes * 60_000;
        assert_eq!(
            stale(&runs, "mike", &me, &state, down_at(ended_at), start, at(14)),
            None
        );
        assert_eq!(
            stale(&runs, "mike", &me, &state, down_at(ended_at), start, at(15)),
            Some(Stale::Offline(15))
        );
        // No link since the start of this daemon: the time counts from
        // the start.
        assert_eq!(
            stale(
                &runs,
                "mike",
                &me,
                &state,
                never,
                start,
                start + OFFLINE_MS - 1
            ),
            None
        );
        assert_eq!(
            stale(&runs, "mike", &me, &state, never, start, start + OFFLINE_MS),
            Some(Stale::Offline(15))
        );
        // A session with no record yet counts as one that runs.
        let unknown = of("u1", "mike", owner.public());
        assert_eq!(
            stale(
                &unknown,
                "mike",
                &me,
                &state,
                never,
                start,
                start + OFFLINE_MS
            ),
            Some(Stale::Offline(15))
        );
        // A claim of another user holds, also offline and ended.
        let of_brett = of("e1", "brett", owner.public());
        assert_eq!(
            stale(
                &of_brett,
                "mike",
                &me,
                &state,
                never,
                start,
                start + 99 * OFFLINE_MS
            ),
            None
        );
        // A claim of this daemon that runs, or that has no record, holds:
        // this daemon releases it at the end of its session.
        let mine = of("x1", "mike", me);
        assert_eq!(
            stale(
                &mine,
                "mike",
                &me,
                &state,
                never,
                start,
                start + 99 * OFFLINE_MS
            ),
            None
        );
    }

    /// Runs `git ARGS` in `dir` with no config of the machine, and gives
    /// its stdout.
    pub(crate) fn run(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// A bare forge `forge.git` in `root`, and the clone `name` of it,
    /// with the git identity `name`.
    pub(crate) fn clone_of(root: &Path, name: &str) -> PathBuf {
        if !root.join("forge.git").exists() {
            run(root, &["init", "-q", "--bare", "-b", "main", "forge.git"]);
        }
        run(root, &["clone", "-q", "forge.git", name]);
        let clone = root.join(name);
        run(&clone, &["config", "user.name", name]);
        run(
            &clone,
            &["config", "user.email", &format!("{name}@example.com")],
        );
        clone
    }

    fn claim(item: &str, user: &str, byte: u8) -> Claim {
        Claim::now(item, user, "s1", Key::from_bytes(&[byte; 32]).public())
    }

    /// 01M4Z60B8DYAVEQF6F8N9QP7MS, 01M519Z4CT0PRXPRM22037Z5RY: the claim
    /// commit has the empty tree, no parent, no signature, the message
    /// `riff claim ITEM` and the five trailers.
    #[test]
    fn a_claim_commit_has_the_empty_tree_no_parent_no_signature_and_the_trailers() {
        let root = tempfile::tempdir().unwrap();
        let main = clone_of(root.path(), "mike");
        let made = claim("issue-12", "mike", 1);
        let commit = commit(&main, &made).unwrap();
        let empty = run(&main, &["hash-object", "-t", "tree", "/dev/null"]);
        assert_eq!(run(&main, &["show", "-s", "--format=%T", &commit]), empty);
        assert_eq!(run(&main, &["show", "-s", "--format=%P", &commit]), "");
        let raw = run(&main, &["cat-file", "commit", &commit]);
        assert!(!raw.contains("gpgsig"), "{raw}");
        assert_eq!(run(&main, &["show", "-s", "--format=%an", &commit]), "mike");
        let message = run(&main, &["show", "-s", "--format=%B", &commit]);
        assert_eq!(message.lines().next(), Some("riff claim issue-12"));
        let trailers = run(
            &main,
            &["show", "-s", "--format=%(trailers:only,unfold)", &commit],
        );
        let names: Vec<&str> = trailers
            .lines()
            .filter_map(|l| l.split_once(": ").map(|(name, _)| name))
            .collect();
        assert_eq!(
            names,
            [
                "Riff-Item",
                "Riff-User",
                "Riff-Session",
                "Riff-Key",
                "Riff-Time"
            ]
        );
        assert!(trailers.contains(&format!("Riff-Key: {}", made.key)));
        assert!(chrono::DateTime::parse_from_rfc3339(&made.time).is_ok());
        assert!(made.time.ends_with('Z'), "{}", made.time);
        assert_eq!(Claim::read(&message), Some(made));
    }

    /// 01M4Z60B8EB7A44A41CWWP7G8K: a second claim of the item reads as
    /// held, a release with a wrong lease fails, and the release with
    /// the lease of the claim commit deletes the ref.
    #[test]
    fn a_claim_holds_until_its_release_with_its_lease() {
        let root = tempfile::tempdir().unwrap();
        let (mike, brett) = (
            clone_of(root.path(), "mike"),
            clone_of(root.path(), "brett"),
        );
        let Took::Won(won) = take(&mike, &claim("issue-12", "mike", 1)).unwrap() else {
            panic!("the first claim wins");
        };
        assert_eq!(
            take(&brett, &claim("issue-12", "brett", 2)).unwrap(),
            Took::Held
        );
        assert_eq!(
            list(&brett).unwrap(),
            BTreeMap::from([("issue-12".to_owned(), won.clone())])
        );
        // Brett lacks the commit of Mike: riff fetches it.
        let found = found(&brett).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].claim.as_ref().map(|c| c.user.as_str()),
            Some("mike")
        );
        let other = commit(&brett, &claim("issue-12", "brett", 2)).unwrap();
        assert_eq!(release(&brett, "issue-12", &other).unwrap(), Freed::NotAt);
        assert_eq!(list(&mike).unwrap().len(), 1);
        assert_eq!(release(&mike, "issue-12", &won).unwrap(), Freed::Deleted);
        assert!(list(&mike).unwrap().is_empty());
        assert_eq!(release(&mike, "issue-12", &won).unwrap(), Freed::NotAt);
        // The item is free again.
        assert!(matches!(
            take(&brett, &claim("issue-12", "brett", 2)).unwrap(),
            Took::Won(_)
        ));
    }

    /// A commit of the ref that is not a claim commit of riff has no
    /// facts, and a fault of the forge is an error, not a held item.
    #[test]
    fn a_foreign_ref_has_no_facts_and_a_lost_forge_is_an_error() {
        let root = tempfile::tempdir().unwrap();
        let main = clone_of(root.path(), "mike");
        let tree = run(&main, &["mktree"]);
        let other = run(&main, &["commit-tree", "-m", "hello", &tree]);
        let to = format!("{other}:refs/riff/claims/issue-7");
        run(&main, &["push", "-q", "origin", &to]);
        let found = found(&main).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].claim, None);
        std::fs::remove_dir_all(root.path().join("forge.git")).unwrap();
        assert!(take(&main, &claim("issue-12", "mike", 1)).is_err());
        assert!(list(&main).is_err());
    }
}
