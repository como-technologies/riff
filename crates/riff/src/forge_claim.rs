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
//! and [`reference`] takes only the letters, the digits and `-`, `_`
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

/// Runs `git ARGS` in the clone `main`, with no input.
fn git(main: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(main)
        .stdin(Stdio::null())
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

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::daemon_key::Key;

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
        run(&clone, &["config", "user.email", &format!("{name}@example.com")]);
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
            ["Riff-Item", "Riff-User", "Riff-Session", "Riff-Key", "Riff-Time"]
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
        assert_eq!(found[0].claim.as_ref().map(|c| c.user.as_str()), Some("mike"));
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
