//! The clone of a session, in its container (#2139).
//!
//! # Design
//!
//! Each session clones and pushes in its container, with its own token
//! on GitHub. The host makes no clone of a session, and binds no git
//! tree with write into a container: the host runs no git in a folder
//! that a session wrote ([`crate::host_git`]).
//!
//! The spec of a session has an empty folder at the path of its clone
//! ([`crate::runtime::Source::Empty`]): on podman, a volume of the
//! container, on the disk and not in the tmpfs of the home. `podman run
//! --rm` removes it with the container. The container sees the git dir
//! of the main clone, `MAIN/.git`, at the same path with no write: its
//! objects, its refs and its config. Only the daemon or the person
//! writes the main clone, never a session. The spec gives the plan of
//! the clone to the relay in the variable [`VAR`], as plain data
//! ([`Plan`], [`crate::run::spec`]). Before it starts `claude`, the relay
//! makes the clone there ([`Plan::make`]), with no object from GitHub
//! that the main clone has:
//!
//! | Step | git in the container |
//! |---|---|
//! | The clone | `git init`, and `MAIN/.git/objects` in the alternates of the clone: the clone reads each object of the main clone, and copies none |
//! | `origin` | `git remote add origin https://github.com/OWNER/REPO.git` ([`origin`]) |
//! | The view of the main clone | `git fetch MAIN 'refs/remotes/origin/*:refs/remotes/origin/*'`: the branches of `origin` as the main clone has them |
//! | With a token | `git fetch --prune origin`: the branches of GitHub, with only the objects that the main clone does not have. git takes the token of the session from `gh` |
//! | The default branch | `git remote set-head origin BASE`: the default branch of [`Plan::base`], from the main clone on the host |
//! | The lead | `git switch -c BASE --track origin/BASE` ([`Checkout::Default`]) |
//! | A build, a rebase, a fix of the checks, with the branch on `origin` | `git switch -c BRANCH --track origin/BRANCH`: the session goes on from the work that it pushed ([`Checkout::Branch`]) |
//! | The same, with no branch on `origin` | `git switch -c BRANCH --no-track origin/BASE` |
//! | A verify | with a token, `git fetch origin refs/pull/N/head`; then `git checkout --detach COMMIT` ([`Checkout::Pull`]): the head commit of the pull request, also from a fork. A push to the pull request after the look does not change what the verify reads |
//!
//! Each start point is explicit, so no config of the person, for example
//! `checkout.guess=false`, changes the branch. The host looks for the
//! branch of the item on `origin` one time ([`crate::run::earlier`]), and
//! the plan holds the answer ([`Checkout::Branch`]): the start text and
//! the clone agree.
//!
//! A session has a token when the folder of its `gh` has the file
//! [`crate::daemon::gh_token::HOSTS`] ([`token_here`]). With no token, a
//! lead with no token of an App or a session of `riff run --no-github`,
//! the clone fetches nothing from GitHub: it starts from the view of the
//! main clone. So such a session also starts on a private repository.
//!
//! A clone that is there stays: a takeover runs the relay again in the
//! same container. The folder of a new clone is empty, and after a fault
//! it is empty again. Each git step has the time limit [`LIMIT`] and no
//! prompt. When a step fails, the relay starts no `claude`, and the
//! error names the step.
//!
//! The work of a session is what it pushed: its clone goes with its
//! container. The next session of the item clones again, and goes on
//! from the branch on `origin`.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as riff daemon (host)
//!     participant R as riff relay (container)
//!     participant M as MAIN/.git (no write)
//!     participant G as GitHub
//!     D->>R: podman run: an empty volume at CLONE, MAIN/.git, RIFF_CLONE
//!     R->>M: git init, the alternates, git fetch MAIN: no object copied
//!     R->>G: with a token: git fetch origin, only the new objects
//!     R->>R: git switch -c BRANCH --track origin/BRANCH, or a new branch
//!     R->>R: start claude in CLONE
//!     R->>G: claude pushes BRANCH
//!     Note over R: the container ends: its volume goes
//!     D->>R: the next session: a new volume, a new clone
//!     R->>G: git fetch origin, git switch: the pushed work
//! ```
//!
//! # A session with no GitHub
//!
//! A session of `riff run --no-github` pushes nothing. So when `claude`
//! ends, the relay writes the commits of the branch that `origin/BASE`
//! does not have in a git bundle, in the folder `out/` of the folder of
//! the session ([`Plan::bundle`], [`crate::run::OUT`]). The session can
//! write that folder, so the host trusts no file there. `riff run` opens
//! the file with no link, copies it into the folder of the session,
//! which no session sees, and checks the first line of the copy: a git
//! bundle. Then it runs `git bundle verify` on the copy, in the main
//! clone ([`crate::run::take_bundle`]). The person fetches from the
//! copy. A file in the place of the bundle that is no bundle, for
//! example a `gitdir:` file, gets no git.
//!
//! ```mermaid
//! flowchart LR
//!     C["claude ends"] --> B{"commits that origin/BASE does not have?"}
//!     B -- yes --> W["the relay: git bundle create out/BRANCH.bundle"]
//!     B -- no --> N["no bundle"]
//!     W --> T["riff run: a copy with no link, the first line, git bundle verify"]
//!     T --> P["the person: git fetch COPY BRANCH, git push"]
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// The variable of the container that holds the [`Plan`] of its clone,
/// as JSON. The relay reads it, and `claude` does not get it.
pub const VAR: &str = "RIFF_CLONE";

/// The time limit of each git step of the relay.
pub const LIMIT: Duration = Duration::from_secs(600);

/// The URL of the repository `repo` (`OWNER/REPO`) on GitHub: the
/// `origin` of each clone of a session. git asks `gh` for the token of
/// the session for it ([`crate::run::GIT_HELPER`]).
///
/// ```
/// assert_eq!(riff::session_clone::origin("o/r"), "https://github.com/o/r.git");
/// ```
pub fn origin(repo: &str) -> String {
    format!("https://github.com/{repo}.git")
}

/// True when this session has a token on GitHub: the folder of its `gh`
/// ([`crate::daemon::gh_token::CONFIG_VAR`]) has the file
/// [`crate::daemon::gh_token::HOSTS`]. The daemon writes none for a lead
/// with no token of an App and for a session of `riff run --no-github`
/// ([`crate::daemon::gh_token::write_none`]).
pub fn token_here() -> bool {
    use crate::daemon::gh_token::{CONFIG_VAR, HOSTS};
    std::env::var_os(CONFIG_VAR)
        .filter(|dir| !dir.is_empty())
        .is_some_and(|dir| Path::new(&dir).join(HOSTS).is_file())
}

/// What the clone of a session checks out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Checkout {
    /// The default branch of `origin`: the lead.
    Default,
    /// The branch of the item: a build, a rebase, a fix of the checks.
    Branch {
        /// The name of the branch.
        name: String,
        /// `origin` has the branch, as the host saw it
        /// ([`crate::run::earlier`]): the session goes on from it. Else
        /// the branch is new, from the default branch.
        earlier: bool,
        /// For a session with no GitHub: the file of the bundle of the
        /// branch, which the relay writes when `claude` ends
        /// ([`Plan::bundle`]).
        bundle: Option<PathBuf>,
    },
    /// The head commit `commit` of the pull request `pull`, detached: a
    /// verify.
    Pull {
        /// The number of the pull request.
        pull: u64,
        /// The head commit of the look.
        commit: String,
    },
}

/// The plan of the clone of a session: plain data, in [`VAR`].
///
/// ```
/// use riff::session_clone::{Checkout, Plan};
///
/// let plan = Plan {
///     path: "/d/riff/sessions/issue-12-1a2b3c4d/clone".into(),
///     origin: riff::session_clone::origin("o/r"),
///     main: "/src/r/.git".into(),
///     base: "main".into(),
///     checkout: Checkout::Branch { name: "worktree-issue-12".into(), earlier: false, bundle: None },
/// };
/// assert_eq!(Plan::of_var(&plan.to_var())?, plan);
/// assert!(Plan::of_var("{").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// The folder of the clone in the container: an empty folder on its
    /// disk.
    pub path: PathBuf,
    /// The `origin` of the clone ([`origin`]).
    pub origin: String,
    /// The git dir of the main clone, `MAIN/.git`: the container sees it
    /// at the same path, with no write.
    pub main: PathBuf,
    /// The default branch of `origin`, from `origin/HEAD` of the main
    /// clone on the host ([`crate::permissions::Project::of`]).
    pub base: String,
    /// What the clone checks out.
    pub checkout: Checkout,
}

/// What [`Plan::make`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Made {
    /// The folder had a clone already: it stays as it is.
    There,
    /// A new clone.
    New,
}

impl Plan {
    /// The plan as the value of [`VAR`]. A plan of paths and text always
    /// serializes, so the error of `serde_json` cannot come.
    pub fn to_var(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// The plan in the value `value` of [`VAR`].
    pub fn of_var(value: &str) -> Result<Plan> {
        serde_json::from_str(value).with_context(|| format!("{VAR} holds no plan of a clone"))
    }

    /// The plan of this process: [`VAR`], or `None` when it is not set.
    pub fn here() -> Result<Option<Plan>> {
        match std::env::var(VAR) {
            Ok(value) if !value.is_empty() => Plan::of_var(&value).map(Some),
            _ => Ok(None),
        }
    }

    /// Makes the clone at [`Plan::path`], with the git of this process:
    /// the steps of the module docs. With `token`, the session has a
    /// token on GitHub ([`token_here`]): the clone fetches `origin`. A
    /// folder with a `.git` keeps its clone. A folder with other files
    /// gets no clone. After a fault, the folder is empty again.
    pub fn make(&self, token: bool) -> Result<Made> {
        if self.path.join(".git").symlink_metadata().is_ok() {
            return Ok(Made::There);
        }
        std::fs::create_dir_all(&self.path)
            .with_context(|| format!("cannot make {}", self.path.display()))?;
        let mut entries = std::fs::read_dir(&self.path)
            .with_context(|| format!("cannot read {}", self.path.display()))?;
        if entries.next().is_some() {
            bail!(
                "{} has files and no clone: riff clones only in an empty folder",
                self.path.display()
            );
        }
        let made = self.steps(token);
        if made.is_err() {
            empty(&self.path);
        }
        made.map(|()| Made::New)
    }

    /// The git steps of [`Plan::make`] in the empty folder of the clone.
    fn steps(&self, token: bool) -> Result<()> {
        let dir = &self.path;
        git(dir, &["init", "-q"])?;
        let alternates = dir.join(".git/objects/info/alternates");
        std::fs::write(
            &alternates,
            format!("{}\n", self.main.join("objects").display()),
        )
        .with_context(|| format!("cannot write {}", alternates.display()))?;
        git(dir, &["remote", "add", "origin", &self.origin])?;
        let main = self.main.to_string_lossy();
        git(
            dir,
            &[
                "fetch",
                "-q",
                "--no-tags",
                &main,
                "+refs/remotes/origin/*:refs/remotes/origin/*",
                "^refs/remotes/origin/HEAD",
            ],
        )?;
        if token {
            git(dir, &["fetch", "-q", "--prune", "origin"])?;
        }
        git(dir, &["remote", "set-head", "origin", &self.base])?;
        let base = format!("origin/{}", self.base);
        match &self.checkout {
            Checkout::Default => {
                git(dir, &["switch", "-q", "-c", &self.base, "--track", &base])?;
            }
            Checkout::Branch {
                name,
                earlier: true,
                bundle: _,
            } => {
                let from = format!("origin/{name}");
                git(dir, &["switch", "-q", "-c", name, "--track", &from])?;
            }
            Checkout::Branch {
                name,
                earlier: false,
                bundle: _,
            } => {
                git(dir, &["switch", "-q", "-c", name, "--no-track", &base])?;
            }
            Checkout::Pull { pull, commit } => {
                if token {
                    let head = format!("refs/pull/{pull}/head");
                    git(dir, &["fetch", "-q", "origin", &head])?;
                }
                git(
                    dir,
                    &[
                        "-c",
                        "advice.detachedHead=false",
                        "checkout",
                        "-q",
                        "--detach",
                        commit,
                    ],
                )?;
            }
        }
        Ok(())
    }

    /// [`Plan::make`] with the token of this process ([`token_here`]),
    /// and the line for the log of the session
    /// ([`crate::text::relay_cloned`]): none when the clone was there.
    pub fn make_line(&self) -> Result<Option<String>> {
        let token = token_here();
        Ok(match self.make(token)? {
            Made::There => None,
            Made::New => Some(crate::text::relay_cloned(self, token)),
        })
    }

    /// The file of the bundle of a session with no GitHub: the file
    /// `bundle` of [`Checkout::Branch`]. `None` for each other session.
    ///
    /// ```
    /// use riff::session_clone::{Checkout, Plan};
    ///
    /// let plan = |checkout| Plan {
    ///     path: "/s/clone".into(),
    ///     origin: "https://github.com/o/r.git".into(),
    ///     main: "/src/r/.git".into(),
    ///     base: "main".into(),
    ///     checkout,
    /// };
    /// let file = std::path::Path::new("/s/out/worktree-issue-12.bundle");
    /// let offline = Checkout::Branch { name: "worktree-issue-12".into(), earlier: false, bundle: Some(file.into()) };
    /// assert_eq!(plan(offline).bundle_file(), Some(file));
    /// assert_eq!(plan(Checkout::Default).bundle_file(), None);
    /// ```
    pub fn bundle_file(&self) -> Option<&Path> {
        match &self.checkout {
            Checkout::Branch {
                name: _,
                earlier: _,
                bundle,
            } => bundle.as_deref(),
            Checkout::Default | Checkout::Pull { pull: _, commit: _ } => None,
        }
    }

    /// The branch of the item: the name of [`Checkout::Branch`].
    fn branch(&self) -> Option<&str> {
        match &self.checkout {
            Checkout::Branch {
                name,
                earlier: _,
                bundle: _,
            } => Some(name),
            Checkout::Default | Checkout::Pull { pull: _, commit: _ } => None,
        }
    }

    /// Writes the bundle of a session with no GitHub ([`Plan::bundle_file`]):
    /// the commits of the branch that `origin/BASE` does not have. An
    /// older bundle goes first. It gives the file, or `None` with no
    /// bundle to write: no file in the plan, no branch in the clone, or no
    /// such commit.
    pub fn bundle(&self) -> Result<Option<PathBuf>> {
        let (Some(file), Some(name)) = (self.bundle_file(), self.branch()) else {
            return Ok(None);
        };
        match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(e).with_context(|| format!("cannot remove {}", file.display()));
            }
            _ => {}
        }
        let head = format!("refs/heads/{name}");
        if git(&self.path, &["rev-parse", "-q", "--verify", &head]).is_err() {
            return Ok(None);
        }
        let range = format!("refs/remotes/origin/{}..{head}", self.base);
        if git(&self.path, &["rev-list", "--count", &range])? == "0" {
            return Ok(None);
        }
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)
                .with_context(|| format!("cannot make {}", folder.display()))?;
        }
        git(
            &self.path,
            &["bundle", "create", "-q", &file.to_string_lossy(), &range],
        )?;
        Ok(Some(file.to_owned()))
    }
}

/// Removes each file and folder in `dir`: a clone that failed. `dir`
/// was empty before the clone, so nothing else goes.
fn empty(dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let _ = match entry.file_type() {
            Ok(kind) if kind.is_dir() => std::fs::remove_dir_all(&path),
            _ => std::fs::remove_file(&path),
        };
    }
}

/// git `args` in `dir`, with no prompt and the limit [`LIMIT`]
/// ([`crate::host_git::within`]).
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    crate::host_git::within(dir, args, LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs git in `dir` with an identity and no signing.
    fn git_in(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// The count of the objects in the own object store of the clone at
    /// `clone`: loose and in packs. The objects of its alternates do not
    /// count.
    fn own_objects(clone: &Path) -> u64 {
        git_in(clone, &["count-objects", "-v"])
            .lines()
            .filter_map(|l| l.split_once(": "))
            .filter(|(key, _)| *key == "count" || *key == "in-pack")
            .map(|(_, n)| n.parse::<u64>().unwrap())
            .sum()
    }

    /// A bare `origin` that a `file://` URL names, as a URL of GitHub
    /// does, with one commit on `main`, and a main clone of it.
    struct Origin {
        dir: tempfile::TempDir,
        url: String,
        main: PathBuf,
    }

    impl Origin {
        fn new() -> Origin {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let seed = root.join("seed");
            std::fs::create_dir(&seed).unwrap();
            git_in(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
            git_in(&seed, &["init", "-q", "-b", "main"]);
            std::fs::write(seed.join("a"), "one").unwrap();
            git_in(&seed, &["add", "a"]);
            git_in(&seed, &["commit", "-q", "-m", "one"]);
            let url = format!("file://{}", root.join("origin.git").display());
            git_in(&seed, &["push", "-q", &url, "main"]);
            git_in(root, &["clone", "-q", "--", &url, "main"]);
            let main = root.join("main");
            Origin { dir, url, main }
        }

        /// The plan of a clone at `name` with `checkout`.
        fn plan(&self, name: &str, checkout: Checkout) -> Plan {
            Plan {
                path: self.dir.path().join(name).join("clone"),
                origin: self.url.clone(),
                main: self.main.join(".git"),
                base: "main".into(),
                checkout,
            }
        }

        /// A commit of the file `a` with `text` in `clone`.
        fn commit(clone: &Path, text: &str) -> String {
            std::fs::write(clone.join("a"), text).unwrap();
            git_in(clone, &["commit", "-q", "-a", "-m", text]);
            git_in(clone, &["rev-parse", "HEAD"])
        }
    }

    /// The branch `name` of the item, with no bundle.
    fn branch(name: &str, earlier: bool) -> Checkout {
        Checkout::Branch {
            name: name.into(),
            earlier,
            bundle: None,
        }
    }

    /// 01M7EY3P0Q8TQF5QRTRDWCYCTQ: the clone reads the objects of the main
    /// clone through its alternates, and copies none of them. The branch
    /// of the item is new, from the default branch, with no upstream.
    #[test]
    fn a_clone_links_to_the_main_clone_and_makes_the_branch() {
        let origin = Origin::new();
        let plan = origin.plan("one", branch("worktree-issue-12", false));
        assert_eq!(plan.make(true).unwrap(), Made::New);
        let alternates =
            std::fs::read_to_string(plan.path.join(".git/objects/info/alternates")).unwrap();
        assert_eq!(
            Path::new(alternates.trim()),
            origin.main.join(".git/objects")
        );
        assert_eq!(own_objects(&plan.path), 0, "the clone copied objects");
        assert_eq!(
            git_in(&plan.path, &["branch", "--show-current"]),
            "worktree-issue-12"
        );
        assert_eq!(
            git_in(&plan.path, &["remote", "get-url", "origin"]),
            origin.url
        );
        assert_eq!(
            git_in(&plan.path, &["symbolic-ref", "refs/remotes/origin/HEAD"]),
            "refs/remotes/origin/main"
        );
        let upstream = [
            "for-each-ref",
            "--format=%(upstream)",
            "refs/heads/worktree-issue-12",
        ];
        assert_eq!(git_in(&plan.path, &upstream), "");
        // A clone that is there stays: the relay of a takeover.
        assert_eq!(plan.make(true).unwrap(), Made::There);
    }

    /// 01M7EY3P0Q8TQF5QRTRDWCYCTQ: with a token, the clone fetches the new
    /// work of `origin`, and gets only the objects that the main clone
    /// does not have. A branch that `origin` lost goes from the view of
    /// the main clone.
    #[test]
    fn a_clone_with_a_token_fetches_only_what_the_main_clone_does_not_have() {
        let origin = Origin::new();
        git_in(
            &origin.main,
            &["push", "-q", "origin", "HEAD:refs/heads/gone"],
        );
        let bare = origin.dir.path().join("origin.git");
        git_in(&bare, &["update-ref", "-d", "refs/heads/gone"]);
        let other = origin.plan("other", Checkout::Default);
        other.make(true).unwrap();
        let new = Origin::commit(&other.path, "two");
        git_in(&other.path, &["push", "-q", "origin", "main"]);

        let plan = origin.plan("lead", Checkout::Default);
        plan.make(true).unwrap();
        assert_eq!(git_in(&plan.path, &["rev-parse", "HEAD"]), new);
        // The commit, its tree and its file: no object of the main clone.
        assert_eq!(own_objects(&plan.path), 3);
        let remotes = git_in(&plan.path, &["branch", "-r"]);
        assert!(!remotes.contains("gone"), "{remotes}");
    }

    /// 01M7EY3P0Q8TQF5QRTRDWCYCTQ: with no token, the clone fetches nothing
    /// from GitHub: it starts from the view of the main clone. So a lead
    /// with no token and a session with no GitHub also start on a private
    /// repository, here an `origin` that is not there.
    #[test]
    fn a_clone_with_no_token_starts_from_the_main_clone() {
        let origin = Origin::new();
        let mut plan = origin.plan("offline", branch("worktree-issue-12", false));
        plan.origin = "https://github.invalid/o/private.git".into();
        assert_eq!(plan.make(false).unwrap(), Made::New);
        assert_eq!(
            git_in(&plan.path, &["rev-parse", "HEAD"]),
            git_in(&origin.main, &["rev-parse", "origin/main"])
        );
        assert_eq!(own_objects(&plan.path), 0);
        // With a token, the fetch of that `origin` fails, the error names
        // the step, and the folder is empty again.
        let mut fails = origin.plan("fails", Checkout::Default);
        fails.origin = format!("file://{}", origin.dir.path().join("none.git").display());
        let error = fails.make(true).unwrap_err();
        assert!(
            error.to_string().contains("fetch -q --prune origin"),
            "{error}"
        );
        assert_eq!(std::fs::read_dir(&fails.path).unwrap().count(), 0);
        // A folder with files and no clone gets no clone, and its files
        // stay.
        std::fs::write(fails.path.join("mine"), "").unwrap();
        let error = fails.make(false).unwrap_err();
        assert!(
            error.to_string().contains("has files and no clone"),
            "{error}"
        );
        assert!(fails.path.join("mine").exists());
    }

    /// 01M7EY3P0Q8TQF5QRTRDWCYCTQ: the session pushes its work, and the
    /// next session of the item goes on from the branch of `origin`, with
    /// it as the upstream. A branch that git refuses fails, and the error
    /// names the step. The integration test `session_clone` runs it with
    /// `checkout.guess=false` in the git config of the person.
    #[test]
    fn the_next_session_goes_on_from_the_pushed_branch() {
        let origin = Origin::new();
        let first = origin.plan("first", branch("worktree-issue-12", false));
        first.make(true).unwrap();
        let work = Origin::commit(&first.path, "WIP: two");
        git_in(
            &first.path,
            &["push", "-q", "-u", "origin", "worktree-issue-12"],
        );
        let next = origin.plan("next", branch("worktree-issue-12", true));
        assert_eq!(next.make(true).unwrap(), Made::New);
        assert_eq!(git_in(&next.path, &["rev-parse", "HEAD"]), work);
        assert_eq!(
            git_in(&next.path, &["rev-parse", "--abbrev-ref", "@{upstream}"]),
            "origin/worktree-issue-12"
        );
        let bad = origin.plan("bad", branch("bad..name", false));
        let error = bad.make(true).unwrap_err();
        assert!(
            error.to_string().contains("switch -q -c bad..name"),
            "{error}"
        );
    }

    /// The lead gets the default branch of `origin`, with its upstream.
    #[test]
    fn the_clone_of_the_lead_is_on_the_default_branch() {
        let origin = Origin::new();
        let plan = origin.plan("lead", Checkout::Default);
        plan.make(true).unwrap();
        assert_eq!(git_in(&plan.path, &["branch", "--show-current"]), "main");
        assert_eq!(
            git_in(&plan.path, &["rev-parse", "--abbrev-ref", "@{upstream}"]),
            "origin/main"
        );
    }

    /// 01M5QVARWYFHTQV5THNRB09SZA: the head commit of a pull request from
    /// a fork is on no branch of `origin`. The clone of the verify
    /// fetches `refs/pull/N/head`, and checks out the commit of the look,
    /// detached: a push to the pull request after the look does not
    /// change it.
    #[test]
    fn the_clone_of_a_verify_has_the_head_commit_of_the_look() {
        let origin = Origin::new();
        let fork = origin.plan("fork", Checkout::Default);
        fork.make(true).unwrap();
        let head = Origin::commit(&fork.path, "two");
        git_in(
            &fork.path,
            &["push", "-q", "origin", "HEAD:refs/pull/7/head"],
        );
        Origin::commit(&fork.path, "three");
        git_in(
            &fork.path,
            &["push", "-q", "origin", "HEAD:refs/pull/7/head"],
        );
        let pull = |pull| Checkout::Pull {
            pull,
            commit: head.clone(),
        };
        let verify = origin.plan("verify", pull(7));
        verify.make(true).unwrap();
        assert_eq!(git_in(&verify.path, &["rev-parse", "HEAD"]), head);
        assert_eq!(git_in(&verify.path, &["branch", "--show-current"]), "");
        assert_eq!(
            std::fs::read_to_string(verify.path.join("a")).unwrap(),
            "two"
        );
        // A pull request that `origin` does not have fails, and the error
        // names the ref.
        let error = origin.plan("none", pull(8)).make(true).unwrap_err();
        assert!(error.to_string().contains("refs/pull/8/head"), "{error}");
    }

    /// 01M7EY3WHQQP9330CZ736HK0FJ: a session with no GitHub. The bundle
    /// holds the commits of the branch, and a clone of the person fetches
    /// them from it. With no commit, there is no bundle. Each bundle takes
    /// the place of the older one. With no branch in the clone, the older
    /// bundle goes, and there is no new one.
    #[test]
    fn a_session_with_no_github_gives_its_commits_in_a_bundle() {
        let origin = Origin::new();
        let file = origin.dir.path().join("out/worktree-issue-12.bundle");
        let plan = origin.plan(
            "offline",
            Checkout::Branch {
                name: "worktree-issue-12".into(),
                earlier: false,
                bundle: Some(file.clone()),
            },
        );
        plan.make(false).unwrap();
        assert_eq!(plan.bundle().unwrap(), None);
        Origin::commit(&plan.path, "two");
        assert_eq!(plan.bundle().unwrap(), Some(file.clone()));
        let work = Origin::commit(&plan.path, "three");
        assert_eq!(plan.bundle().unwrap(), Some(file.clone()));
        let at = file.to_string_lossy();
        git_in(&origin.main, &["fetch", "-q", &at, "worktree-issue-12"]);
        assert_eq!(git_in(&origin.main, &["rev-parse", "FETCH_HEAD"]), work);
        // The session removed its branch: no bundle.
        git_in(&plan.path, &["switch", "-q", "--detach"]);
        git_in(&plan.path, &["branch", "-q", "-D", "worktree-issue-12"]);
        assert_eq!(plan.bundle().unwrap(), None);
        assert!(!file.exists(), "the older bundle stays");
        // No file in the plan, no bundle.
        let none = origin.plan("none", branch("worktree-issue-12", false));
        assert_eq!(none.bundle().unwrap(), None);
    }
}
