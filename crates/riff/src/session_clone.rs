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
//! --rm` removes it with the container. The spec gives the plan of the
//! clone to the relay in the variable [`VAR`], as plain data ([`Plan`],
//! [`crate::run::spec`]). Before it starts `claude`, the relay makes the
//! clone there ([`Plan::make`]):
//!
//! | Session | git in the container |
//! |---|---|
//! | Each | `git clone --reference MAIN -- https://github.com/OWNER/REPO.git CLONE` ([`origin`]): the container sees the objects of the main clone with no write, so the clone fetches only what the main clone does not have. git takes the token of the session from `gh` |
//! | A build, a rebase, a fix of the checks | the branch of the item ([`Checkout::Branch`]): `git switch BRANCH` when `origin` has it, so the session goes on from the work that it pushed, else `git switch -c BRANCH` |
//! | A verify | `git fetch origin refs/pull/N/head`, then `git checkout --detach COMMIT` ([`Checkout::Pull`]): the head commit of the pull request, also from a fork. A push to the pull request after the look does not change what the verify reads |
//! | The lead | the default branch of `origin` ([`Checkout::Default`]) |
//!
//! A clone that is there stays: a takeover runs the relay again in the
//! same container. Each git step has the time limit [`LIMIT`] and no
//! prompt. When a step fails, the relay starts no `claude`, and the error
//! names the step.
//!
//! The work of a session is what it pushed: its clone goes with its
//! container. The next session of the item clones again, and goes on
//! from the branch on `origin`.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as riff daemon (host)
//!     participant R as riff relay (container)
//!     participant G as GitHub
//!     D->>R: podman run: an empty volume at CLONE, RIFF_CLONE
//!     R->>G: git clone --reference MAIN, with the token of the session
//!     R->>R: git switch BRANCH: the branch of origin, or a new one
//!     R->>R: start claude in CLONE
//!     R->>G: claude pushes BRANCH
//!     Note over R: the container ends: its volume goes
//!     D->>R: the next session: a new volume, a new clone
//!     R->>G: git clone, git switch BRANCH: the pushed work
//! ```
//!
//! # A session with no GitHub
//!
//! A session of `riff run --no-github` pushes nothing. So when `claude`
//! ends, the relay writes the commits of the branch that `origin/HEAD`
//! does not have in a git bundle, in the folder `out/` of the folder of
//! the session ([`Plan::bundle`], [`crate::run::OUT`]). A bundle is a
//! file of data, not a git tree: a `git fetch` of it in a clone of the
//! person reads no config of the session, and runs no program of it.
//!
//! ```mermaid
//! flowchart LR
//!     C["claude ends"] --> B{"commits that origin/HEAD does not have?"}
//!     B -- yes --> W["git bundle create out/BRANCH.bundle"]
//!     B -- no --> N["no bundle"]
//!     W --> P["the person: git fetch BUNDLE BRANCH, git push"]
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The variable of the container that holds the [`Plan`] of its clone,
/// as JSON. The relay reads it, and `claude` does not get it.
pub const VAR: &str = "RIFF_CLONE";

/// The time limit of each git step of the relay.
pub const LIMIT: Duration = Duration::from_secs(600);

/// The URL of the repository `repo` (`OWNER/REPO`) on GitHub, where each
/// session clones from. git asks `gh` for the token of the session for
/// it ([`crate::run::GIT_HELPER`]).
///
/// ```
/// assert_eq!(riff::session_clone::origin("o/r"), "https://github.com/o/r.git");
/// ```
pub fn origin(repo: &str) -> String {
    format!("https://github.com/{repo}.git")
}

/// What the clone of a session checks out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Checkout {
    /// The default branch of `origin`: the lead.
    Default,
    /// The branch of the item: the branch of `origin` when it has one,
    /// else a new branch.
    Branch(String),
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
///     reference: "/src/r/.git".into(),
///     checkout: Checkout::Branch("worktree-issue-12".into()),
///     bundle: None,
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
    /// The repository to clone ([`origin`]).
    pub origin: String,
    /// The git dir of the main clone: `--reference`. The container sees
    /// its objects, with no write.
    pub reference: PathBuf,
    /// What the clone checks out.
    pub checkout: Checkout,
    /// For a session with no GitHub: the file of the bundle of the
    /// branch, which the relay writes when `claude` ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<PathBuf>,
}

/// What [`Plan::make`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Made {
    /// The folder had a clone already: it stays as it is.
    There,
    /// A new clone. `earlier` is true when it goes on from the branch of
    /// the item on `origin`.
    New {
        /// The branch of `origin` was there.
        earlier: bool,
    },
}

impl Plan {
    /// The plan as the value of [`VAR`].
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
    /// the steps of the module docs. A folder with a `.git` keeps its
    /// clone.
    pub fn make(&self) -> Result<Made> {
        if self.path.join(".git").symlink_metadata().is_ok() {
            return Ok(Made::There);
        }
        std::fs::create_dir_all(&self.path)
            .with_context(|| format!("cannot make {}", self.path.display()))?;
        let (path, reference) = (
            self.path.to_string_lossy(),
            self.reference.to_string_lossy(),
        );
        git(
            &self.path,
            &[
                "clone",
                "-q",
                "--reference",
                &reference,
                "--",
                &self.origin,
                &path,
            ],
        )?;
        let earlier = match &self.checkout {
            Checkout::Default => false,
            Checkout::Branch(branch) => {
                let remote = format!("refs/remotes/origin/{branch}");
                let earlier = git(&self.path, &["rev-parse", "-q", "--verify", &remote]).is_ok();
                let switch: &[&str] = if earlier {
                    &["switch", "-q", branch]
                } else {
                    &["switch", "-q", "-c", branch]
                };
                git(&self.path, switch)?;
                earlier
            }
            Checkout::Pull { pull, commit } => {
                let head = format!("refs/pull/{pull}/head");
                git(&self.path, &["fetch", "-q", "origin", &head])?;
                git(
                    &self.path,
                    &[
                        "-c",
                        "advice.detachedHead=false",
                        "checkout",
                        "-q",
                        "--detach",
                        commit,
                    ],
                )?;
                false
            }
        };
        Ok(Made::New { earlier })
    }

    /// [`Plan::make`], and the line for the log of the session
    /// ([`crate::text::relay_cloned`]): none when the clone was there.
    pub fn make_line(&self) -> Result<Option<String>> {
        Ok(match self.make()? {
            Made::There => None,
            Made::New { earlier } => Some(crate::text::relay_cloned(self, earlier)),
        })
    }

    /// Writes the bundle of a session with no GitHub
    /// ([`Plan::bundle`]): the commits of the branch that `origin/HEAD`
    /// does not have. It gives the file, or `None` with no bundle to
    /// write: no file in the plan, no branch, or no such commit. An
    /// older bundle goes first.
    pub fn bundle(&self) -> Result<Option<PathBuf>> {
        let (Some(file), Checkout::Branch(branch)) = (&self.bundle, &self.checkout) else {
            return Ok(None);
        };
        match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(e).with_context(|| format!("cannot remove {}", file.display()));
            }
            _ => {}
        }
        let range = format!("refs/remotes/origin/HEAD..refs/heads/{branch}");
        if git(&self.path, &["rev-list", "--count", &range])? == "0" {
            return Ok(None);
        }
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)
                .with_context(|| format!("cannot make {}", folder.display()))?;
        }
        git(
            &self.path,
            &["bundle", "create", &file.to_string_lossy(), &range],
        )?;
        Ok(Some(file.clone()))
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

    /// Runs git in `dir` with no config of the person.
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
                reference: self.main.join(".git"),
                checkout,
                bundle: None,
            }
        }

        /// A commit of the file `a` with `text` in `clone`.
        fn commit(clone: &Path, text: &str) -> String {
            std::fs::write(clone.join("a"), text).unwrap();
            git_in(clone, &["commit", "-q", "-a", "-m", text]);
            git_in(clone, &["rev-parse", "HEAD"])
        }
    }

    /// The clone reads the objects of the main clone through its
    /// alternates, and the branch of the item is new.
    #[test]
    fn a_clone_links_to_the_main_clone_and_makes_the_branch() {
        let origin = Origin::new();
        let plan = origin.plan("one", Checkout::Branch("worktree-issue-12".into()));
        assert_eq!(plan.make().unwrap(), Made::New { earlier: false });
        let alternates =
            std::fs::read_to_string(plan.path.join(".git/objects/info/alternates")).unwrap();
        assert_eq!(
            alternates.trim(),
            origin.main.join(".git/objects").display().to_string()
        );
        assert_eq!(
            git_in(&plan.path, &["branch", "--show-current"]),
            "worktree-issue-12"
        );
        // A clone that is there stays: the relay of a takeover.
        assert_eq!(plan.make().unwrap(), Made::There);
    }

    /// The session pushes its work: the next session of the item goes on
    /// from the branch of `origin`.
    #[test]
    fn the_next_session_goes_on_from_the_pushed_branch() {
        let origin = Origin::new();
        let branch = Checkout::Branch("worktree-issue-12".into());
        let first = origin.plan("first", branch.clone());
        first.make().unwrap();
        let work = Origin::commit(&first.path, "WIP: two");
        git_in(
            &first.path,
            &["push", "-q", "-u", "origin", "worktree-issue-12"],
        );
        let next = origin.plan("next", branch);
        assert_eq!(next.make().unwrap(), Made::New { earlier: true });
        assert_eq!(git_in(&next.path, &["rev-parse", "HEAD"]), work);
        assert_eq!(
            git_in(&next.path, &["rev-parse", "--abbrev-ref", "@{upstream}"]),
            "origin/worktree-issue-12"
        );
    }

    /// The lead gets the default branch of `origin`.
    #[test]
    fn the_clone_of_the_lead_is_on_the_default_branch() {
        let origin = Origin::new();
        let plan = origin.plan("lead", Checkout::Default);
        plan.make().unwrap();
        assert_eq!(git_in(&plan.path, &["branch", "--show-current"]), "main");
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
        fork.make().unwrap();
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
        verify.make().unwrap();
        assert_eq!(git_in(&verify.path, &["rev-parse", "HEAD"]), head);
        assert_eq!(git_in(&verify.path, &["branch", "--show-current"]), "");
        assert_eq!(
            std::fs::read_to_string(verify.path.join("a")).unwrap(),
            "two"
        );
        // A pull request that `origin` does not have fails, and the error
        // names the ref.
        let error = origin.plan("none", pull(8)).make().unwrap_err();
        assert!(error.to_string().contains("refs/pull/8/head"), "{error}");
    }

    /// A session with no GitHub: the bundle holds the commits of the
    /// branch, and a clone of the person fetches them from it. With no
    /// commit, there is no bundle.
    #[test]
    fn a_session_with_no_github_gives_its_commits_in_a_bundle() {
        let origin = Origin::new();
        let file = origin.dir.path().join("out/worktree-issue-12.bundle");
        let plan = Plan {
            bundle: Some(file.clone()),
            ..origin.plan("offline", Checkout::Branch("worktree-issue-12".into()))
        };
        plan.make().unwrap();
        assert_eq!(plan.bundle().unwrap(), None);
        let work = Origin::commit(&plan.path, "two");
        assert_eq!(plan.bundle().unwrap(), Some(file.clone()));
        let at = file.to_string_lossy();
        git_in(&origin.main, &["fetch", "-q", &at, "worktree-issue-12"]);
        assert_eq!(git_in(&origin.main, &["rev-parse", "FETCH_HEAD"]), work);
        // No file in the plan, no bundle.
        let none = Plan {
            bundle: None,
            ..plan.clone()
        };
        assert_eq!(none.bundle().unwrap(), None);
    }
}
