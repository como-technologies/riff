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
//! --rm` removes it with the container. Of the git dir of the main clone
//! `MAIN/.git`, the container sees only its objects, at the same path
//! with no write ([`OBJECTS`]): no config, no ref, no hook, no log and no
//! worktree. The main clone of `riff run` is the clone of the person, and
//! its config can hold a token. Only the daemon or the person writes the
//! main clone, never a session.
//!
//! Before the start, the host writes the branches of `origin` of the main
//! clone in the file [`REFS`] of the folder of the session, with one `git
//! for-each-ref` ([`write_refs`]). No container can write that folder:
//! the spec binds only folders in it with write. The container sees the
//! file at the same path, with no write. So the list is whole at one
//! time: a `git pack-refs` in the main clone after the start changes
//! nothing that the container sees.
//!
//! The spec gives the plan of the clone to the relay in the variable
//! [`VAR`], as plain data ([`Plan`], [`crate::run::spec`]). Before it
//! starts `claude`, the relay makes the clone there ([`Plan::make`]),
//! with no object from GitHub that the main clone has:
//!
//! | Step | git in the container |
//! |---|---|
//! | The clone | `git init`, and `MAIN/.git/objects` in the alternates of the clone: the clone reads each object of the main clone, and copies none |
//! | `origin` | `git remote add origin https://github.com/OWNER/REPO.git` ([`Origin`]) |
//! | The view of the main clone | the relay makes the git dir `.git/riff-main` in the clone ([`VIEW`]): its own `HEAD`, an empty `refs/`, a link to `MAIN/.git/objects`, and a link to the file [`REFS`] as its `packed-refs`. Then `git fetch VIEW 'refs/remotes/origin/*:refs/remotes/origin/*'`: the branches of `origin` as the main clone had them. Then the view goes |
//! | With a token | `git fetch --prune origin`: the branches of GitHub, with only the objects that the main clone does not have. git takes the token of the session from `gh` |
//! | The default branch | `git remote set-head origin BASE`: the default branch that the host gives ([`Plan::base`]) |
//! | The lead | `git switch -c BASE --track origin/BASE` ([`Checkout::Default`]) |
//! | A build, a rebase, a fix of the checks, with the branch on `origin` | `git switch -c BRANCH --track origin/BRANCH`: the session goes on from the work that it pushed ([`Checkout::Branch`]) |
//! | The same, with no branch on `origin` | `git switch -c BRANCH --no-track origin/BASE` |
//! | A verify | with a token, `git fetch origin refs/pull/N/head`; then `git checkout --detach COMMIT` ([`Checkout::Pull`]): the head commit of the pull request, also from a fork. A push to the pull request after the look does not change what the verify reads |
//!
//! Each start point is explicit, so no config of the person, for example
//! `checkout.guess=false`, changes the branch. The host looks for the
//! branch of the item on `origin` one time ([`crate::run::earlier`]), and
//! the plan holds the answer ([`Checkout::Branch`]): the start text and
//! the clone agree. Each name in the plan has a type that parses:
//! [`Branch`], [`Commit`], [`PullNumber`] and [`Origin`].
//!
//! A session has a token when the folder of its `gh` has the file
//! [`crate::daemon::gh_token::HOSTS`] ([`token_here`]). With no token, a
//! lead with no token of an App or a session of `riff run --no-github`,
//! the clone fetches nothing from GitHub: it starts from the view of the
//! main clone. So such a session also starts on a private repository.
//! The plan is the one place that says that a session has no GitHub
//! ([`Plan::no_github`]): the daemon reads it there, and asks for no
//! token ([`crate::daemon::Start::no_github`]). Before its start, `riff
//! run` fetches the default branch, and the branch of the item when
//! `origin` has it, into the main clone
//! ([`crate::run::fetch_for_no_github`]): so its clone goes on from the
//! branch on `origin` too.
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
//!     participant M as MAIN/.git/objects and origin-refs (no write)
//!     participant G as GitHub
//!     D->>D: git for-each-ref in MAIN: the new file origin-refs
//!     D->>R: podman run: an empty volume at CLONE, the objects of MAIN, origin-refs, RIFF_CLONE
//!     R->>M: git init, the alternates, git fetch of the view: no object copied
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
//! the session ([`Plan::bundle`], [`crate::run::bundle_out`]). The
//! session can write that folder, so the host trusts no file there:
//! `riff run` takes the bundle with the checks of
//! [`crate::run::take_bundle`], and the person fetches from its copy.
//!
//! ```mermaid
//! flowchart LR
//!     C["claude ends"] --> B{"commits that origin/BASE does not have?"}
//!     B -- yes --> W["the relay: git bundle create out/BRANCH.bundle"]
//!     B -- no --> N["no bundle"]
//!     W --> T["riff run: take_bundle, a checked copy"]
//!     T --> P["the person: git fetch COPY BRANCH, git push"]
//! ```

use std::fmt;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::runtime::{Env, SessionSpec};

/// The variable of the container that holds the [`Plan`] of its clone,
/// as JSON. The relay reads it, and `claude` does not get it.
pub const VAR: &str = "RIFF_CLONE";

/// The time limit of each git step of the relay.
pub const LIMIT: Duration = Duration::from_secs(600);

/// The impls of a name of the plan that parses ([`FromStr`]): `as_str`,
/// [`fmt::Display`], and serde as a string through the parse.
macro_rules! parsed_name {
    ($name:ident) => {
        impl $name {
            /// The name as text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = anyhow::Error;

            fn try_from(text: String) -> Result<Self> {
                text.parse()
            }
        }

        impl From<$name> for String {
            fn from(name: $name) -> String {
                name.0
            }
        }
    };
}

/// The name of a git branch, as `git check-ref-format --branch` takes
/// it: no empty part between the `/`, no part that starts with `.` or
/// ends with `.lock`, no `..`, no `@{`, no space, no control character,
/// none of `~^:?*[\`, no `-` at the start, no `.` at the end, and not
/// `@`. So no branch is an option of git.
///
/// ```
/// use riff::session_clone::Branch;
///
/// assert_eq!("worktree-issue-12".parse::<Branch>()?.as_str(), "worktree-issue-12");
/// assert_eq!("feature/a.b".parse::<Branch>()?.to_string(), "feature/a.b");
/// let bad = [
///     "", "-x", "a..b", "a b", "a~1", "a^", "a:b", "a?", "a*", "a[", "a\\b", "/a", "a/",
///     "a//b", ".a", "a/.b", "a.", "a.lock", "a@{1}", "@", "a\u{7f}",
/// ];
/// for name in bad {
///     assert!(name.parse::<Branch>().is_err(), "{name:?}");
/// }
/// assert!(serde_json::from_str::<Branch>(r#""a..b""#).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Branch(String);

parsed_name!(Branch);

impl FromStr for Branch {
    type Err = anyhow::Error;

    fn from_str(name: &str) -> Result<Branch> {
        let bad_char = |c: char| c.is_ascii_control() || " ~^:?*[\\".contains(c);
        let bad_part =
            |part: &str| part.is_empty() || part.starts_with('.') || part.ends_with(".lock");
        if name == "@"
            || name.starts_with('-')
            || name.ends_with('.')
            || name.contains("..")
            || name.contains("@{")
            || name.chars().any(bad_char)
            || name.split('/').any(bad_part)
        {
            bail!("{name:?} is no name of a git branch");
        }
        Ok(Branch(name.to_owned()))
    }
}

/// The hash of a commit, as git takes it: 4 to 64 hex digits.
///
/// ```
/// use riff::session_clone::Commit;
///
/// assert_eq!("0123abcd".parse::<Commit>()?.as_str(), "0123abcd");
/// let long = "a".repeat(65);
/// for hash in ["", "abc", "xyz1", "-1234", "0123 abcd", long.as_str()] {
///     assert!(hash.parse::<Commit>().is_err(), "{hash:?}");
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Commit(String);

parsed_name!(Commit);

impl FromStr for Commit {
    type Err = anyhow::Error;

    fn from_str(hash: &str) -> Result<Commit> {
        if !(4..=64).contains(&hash.len()) || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("{hash:?} is no hash of a commit");
        }
        Ok(Commit(hash.to_owned()))
    }
}

/// The URL of `origin` of the clone of a session: the repository on
/// GitHub ([`Origin::github`]), or a `file://` URL in a test. It has no
/// space and no control character, and it is no option of git. git asks
/// `gh` for the token of the session for it ([`crate::run::GIT_HELPER`]).
///
/// ```
/// use riff::session_clone::Origin;
///
/// assert_eq!(Origin::github("o/r")?.as_str(), "https://github.com/o/r.git");
/// for repo in ["o", "o/r/x", "/r", "o/", "o/..", "-o/r", "o r/x", "o/r?x"] {
///     assert!(Origin::github(repo).is_err(), "{repo:?}");
/// }
/// assert!("file:///tmp/origin.git".parse::<Origin>().is_ok());
/// for url in ["", "-u", "ssh://h/r", "https://a b", "/tmp/origin.git"] {
///     assert!(url.parse::<Origin>().is_err(), "{url:?}");
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Origin(String);

parsed_name!(Origin);

impl Origin {
    /// The URL of the repository `repo` (`OWNER/REPO`) on GitHub: the
    /// `origin` of each clone of a session.
    pub fn github(repo: &str) -> Result<Origin> {
        let part = |p: &str| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && !p.starts_with('-')
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        };
        match repo.split_once('/') {
            Some((owner, name)) if part(owner) && part(name) => {
                Ok(Origin(format!("https://github.com/{repo}.git")))
            }
            _ => bail!("{repo:?} is no repository OWNER/REPO"),
        }
    }
}

impl FromStr for Origin {
    type Err = anyhow::Error;

    fn from_str(url: &str) -> Result<Origin> {
        let scheme = ["https://", "file://"].iter().any(|s| url.starts_with(s));
        if !scheme || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
            bail!("{url:?} is no URL of origin");
        }
        Ok(Origin(url.to_owned()))
    }
}

/// The number of a pull request: not 0.
///
/// ```
/// use riff::session_clone::PullNumber;
///
/// assert_eq!(PullNumber::try_from(40)?.to_string(), "40");
/// assert!(PullNumber::try_from(0).is_err());
/// assert!(serde_json::from_str::<PullNumber>("0").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PullNumber(NonZeroU64);

impl TryFrom<u64> for PullNumber {
    type Error = anyhow::Error;

    fn try_from(number: u64) -> Result<PullNumber> {
        NonZeroU64::new(number)
            .map(PullNumber)
            .context("0 is no number of a pull request")
    }
}

impl fmt::Display for PullNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The one part of the git dir of the main clone that the container of a
/// session sees, at the same path, with no write: its objects.
pub const OBJECTS: &str = "objects";

/// The file of the branches of `origin` of the main clone, in the folder
/// of a session or of the lead ([`write_refs`],
/// [`crate::run::refs_in`]).
pub const REFS: &str = "origin-refs";

/// Writes the branches of `origin` of the git dir `main` of the main
/// clone in the file [`REFS`] of the folder `folder` of a session or of
/// the lead ([`crate::run::refs_in`]), in the form of `packed-refs`: a
/// line `HASH REF` for each ref under `refs/remotes/origin/` that is no
/// symbolic ref, from one `git for-each-ref`. The host writes it before
/// each start. No container can write `folder`: the spec binds only
/// folders in it with write. The write goes through
/// [`crate::host_file::write`]: a new file `origin-refs.riff` with
/// `create_new`, renamed to `origin-refs`. So the file of an earlier
/// start goes, and a container of that start keeps the file that it
/// sees. A link or a folder at the path is an error, and the host writes
/// nothing through the link.
pub fn write_refs(main: &Path, folder: &Path) -> Result<()> {
    let listed = crate::host_git::output(
        main,
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname) %(symref)",
            "refs/remotes/origin",
        ],
    )?;
    let lines: String = listed
        .lines()
        .filter_map(
            |line| match line.split_whitespace().collect::<Vec<_>>()[..] {
                [hash, name] => Some(format!("{hash} {name}\n")),
                _ => None,
            },
        )
        .collect();
    crate::host_file::write(folder, Path::new(REFS), lines.as_bytes())
}

/// The git dir in the `.git` of a new clone where the relay reads the
/// branches of `origin` of the main clone ([`Plan::make`]): its own
/// `HEAD`, an empty `refs/`, a link to [`OBJECTS`] of the main clone, and
/// a link to the file [`REFS`] as its `packed-refs`. It goes after the
/// fetch.
pub const VIEW: &str = "riff-main";

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
        name: Branch,
        /// `origin` has the branch, as the host saw it
        /// ([`crate::run::earlier`]): the session goes on from it. Else
        /// the branch is new, from the default branch.
        earlier: bool,
        /// A session of `riff run --no-github`: the clone fetches
        /// nothing from GitHub, the daemon asks for no token
        /// ([`Plan::no_github`]), and the relay writes the bundle of the
        /// branch when `claude` ends ([`Plan::bundle_file`]).
        no_github: bool,
    },
    /// The head commit `commit` of the pull request `pull`, detached: a
    /// verify.
    Pull {
        /// The number of the pull request.
        pull: PullNumber,
        /// The head commit of the look.
        commit: Commit,
    },
}

impl Checkout {
    /// True when the clone goes on from the branch of the item on
    /// `origin`: the start text of a build says so.
    ///
    /// ```
    /// use riff::session_clone::Checkout;
    ///
    /// let branch = |earlier| Checkout::Branch { name: "worktree-issue-12".parse().unwrap(), earlier, no_github: false };
    /// assert!(branch(true).earlier());
    /// assert!(!branch(false).earlier());
    /// assert!(!Checkout::Default.earlier());
    /// ```
    pub fn earlier(&self) -> bool {
        match self {
            Checkout::Branch {
                name: _,
                earlier,
                no_github: _,
            } => *earlier,
            Checkout::Default | Checkout::Pull { pull: _, commit: _ } => false,
        }
    }
}

/// The plan of the clone of a session: plain data, in [`VAR`].
///
/// ```
/// use riff::session_clone::{Checkout, Origin, Plan};
///
/// let plan = Plan {
///     path: "/d/riff/sessions/issue-12-1a2b3c4d/clone".into(),
///     origin: Origin::github("o/r")?,
///     main: "/src/r/.git".into(),
///     refs: "/d/riff/sessions/issue-12-1a2b3c4d/origin-refs".into(),
///     base: "main".parse()?,
///     checkout: Checkout::Branch { name: "worktree-issue-12".parse()?, earlier: false, no_github: false },
/// };
/// assert_eq!(Plan::of_var(&plan.to_var())?, plan);
/// assert!(Plan::of_var("{").is_err());
/// // A name that does not parse is no plan.
/// assert!(Plan::of_var(&plan.to_var().replace("worktree-issue-12", "-x")).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// The folder of the clone in the container: an empty folder on its
    /// disk.
    pub path: PathBuf,
    /// The `origin` of the clone.
    pub origin: Origin,
    /// The git dir of the main clone, `MAIN/.git`: the container sees its
    /// [`OBJECTS`] at the same path, with no write.
    pub main: PathBuf,
    /// The file [`REFS`] of the branches of `origin` of the main clone,
    /// which the host wrote ([`write_refs`]): the container sees it at
    /// the same path, with no write.
    pub refs: PathBuf,
    /// The default branch of `origin`, as the host gives it: `riff run`
    /// and the lead read it from `origin/HEAD` of the main clone
    /// ([`crate::permissions::Project::of`]), a wave from GitHub
    /// ([`crate::pr::default_branch`]).
    pub base: Branch,
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
    /// The plan as the value of [`VAR`]. A plan of paths and of names
    /// that parsed always serializes, so the error of `serde_json` cannot
    /// come.
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

    /// The plan in the spec `spec` of a session ([`VAR`]), or `None` when
    /// the spec has none that parses.
    pub fn of_spec(spec: &SessionSpec) -> Option<Plan> {
        spec.env.iter().find_map(|e| match e {
            Env::Value(name, value) if name == VAR => Plan::of_var(value).ok(),
            Env::Value(..) | Env::Host(_) => None,
        })
    }

    /// True for the plan of a session of `riff run --no-github`
    /// (01M6Z87JY83V3KR72E2AG8V8KG): the one place that says so.
    ///
    /// ```
    /// use riff::session_clone::{Checkout, Origin, Plan};
    ///
    /// let plan = |no_github| Plan {
    ///     path: "/s/clone".into(),
    ///     origin: Origin::github("o/r").unwrap(),
    ///     main: "/src/r/.git".into(),
    ///     refs: "/s/origin-refs".into(),
    ///     base: "main".parse().unwrap(),
    ///     checkout: Checkout::Branch { name: "worktree-issue-12".parse().unwrap(), earlier: false, no_github },
    /// };
    /// assert!(plan(true).no_github());
    /// assert!(!plan(false).no_github());
    /// assert!(!Plan { checkout: Checkout::Default, ..plan(true) }.no_github());
    /// ```
    pub fn no_github(&self) -> bool {
        match self.checkout {
            Checkout::Branch {
                name: _,
                earlier: _,
                no_github,
            } => no_github,
            Checkout::Default | Checkout::Pull { pull: _, commit: _ } => false,
        }
    }

    /// Makes the clone at [`Plan::path`], with the git of this process:
    /// the steps of the module docs. With `token`, the session has a
    /// token on GitHub ([`token_here`]): the clone fetches `origin`, but
    /// not for a session with no GitHub ([`Plan::fetches`]). A folder
    /// with a `.git` keeps its clone. A folder with other files gets no
    /// clone. After a fault, the folder is empty again.
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
        let made = self.steps(self.fetches(token));
        if made.is_err() {
            empty(&self.path);
        }
        made.map(|()| Made::New)
    }

    /// True when the clone fetches `origin`: the session has a token
    /// (`token`), and it is no session of `riff run --no-github`
    /// ([`Plan::no_github`]).
    pub fn fetches(&self, token: bool) -> bool {
        token && !self.no_github()
    }

    /// The git steps of [`Plan::make`] in the empty folder of the clone.
    /// With `fetch`, the clone fetches `origin` ([`Plan::fetches`]).
    fn steps(&self, fetch: bool) -> Result<()> {
        let dir = &self.path;
        git(dir, &["init", "-q"])?;
        let alternates = dir.join(".git/objects/info/alternates");
        std::fs::write(
            &alternates,
            format!("{}\n", self.main.join(OBJECTS).display()),
        )
        .with_context(|| format!("cannot write {}", alternates.display()))?;
        git(dir, &["remote", "add", "origin", self.origin.as_str()])?;
        let view = self.view()?;
        let fetched = git(
            dir,
            &[
                "fetch",
                "-q",
                "--no-tags",
                &view.to_string_lossy(),
                "+refs/remotes/origin/*:refs/remotes/origin/*",
                "^refs/remotes/origin/HEAD",
            ],
        );
        let removed = std::fs::remove_dir_all(&view)
            .with_context(|| format!("cannot remove {}", view.display()));
        fetched?;
        removed?;
        if fetch {
            git(dir, &["fetch", "-q", "--prune", "origin"])?;
        }
        let base = self.base.as_str();
        git(dir, &["remote", "set-head", "origin", base])?;
        let from_base = format!("origin/{base}");
        match &self.checkout {
            Checkout::Default => {
                git(dir, &["switch", "-q", "-c", base, "--track", &from_base])?;
            }
            Checkout::Branch {
                name,
                earlier: true,
                no_github: _,
            } => {
                let from = format!("origin/{name}");
                git(
                    dir,
                    &["switch", "-q", "-c", name.as_str(), "--track", &from],
                )?;
            }
            Checkout::Branch {
                name,
                earlier: false,
                no_github: _,
            } => {
                let name = name.as_str();
                git(dir, &["switch", "-q", "-c", name, "--no-track", &from_base])?;
            }
            Checkout::Pull { pull, commit } => {
                if fetch {
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
                        commit.as_str(),
                    ],
                )?;
            }
        }
        Ok(())
    }

    /// Makes the git dir [`VIEW`] in the `.git` of the new clone: its own
    /// `HEAD`, an empty `refs/`, a link to [`OBJECTS`] of the main clone,
    /// and a link to the file [`Plan::refs`] as its `packed-refs`. git
    /// reads no config, no hook and no log of the main clone there.
    fn view(&self) -> Result<PathBuf> {
        let view = self.path.join(".git").join(VIEW);
        let refs = view.join("refs");
        std::fs::create_dir_all(&refs)
            .with_context(|| format!("cannot make {}", refs.display()))?;
        let head = view.join("HEAD");
        std::fs::write(&head, format!("ref: refs/heads/{}\n", self.base))
            .with_context(|| format!("cannot write {}", head.display()))?;
        let links = [
            (self.main.join(OBJECTS), view.join(OBJECTS)),
            (self.refs.clone(), view.join("packed-refs")),
        ];
        for (part, link) in links {
            std::os::unix::fs::symlink(&part, &link)
                .with_context(|| format!("cannot link {} to {}", link.display(), part.display()))?;
        }
        Ok(view)
    }

    /// [`Plan::make`] with the token of this process ([`token_here`]),
    /// and the line for the log of the session
    /// ([`crate::text::relay_cloned`]): none when the clone was there.
    pub fn make_line(&self) -> Result<Option<String>> {
        let token = token_here();
        Ok(match self.make(token)? {
            Made::There => None,
            Made::New => Some(crate::text::relay_cloned(self, self.fetches(token))),
        })
    }

    /// The file of the bundle of a session with no GitHub, which the
    /// relay writes when `claude` ends: `out/BRANCH.bundle` in the folder
    /// of the clone ([`crate::run::bundle_out`]). `None` for each other
    /// session.
    ///
    /// ```
    /// use riff::session_clone::{Checkout, Origin, Plan};
    ///
    /// let plan = |checkout| Plan {
    ///     path: "/s/clone".into(),
    ///     origin: Origin::github("o/r").unwrap(),
    ///     main: "/src/r/.git".into(),
    ///     refs: "/s/origin-refs".into(),
    ///     base: "main".parse().unwrap(),
    ///     checkout,
    /// };
    /// let branch = |no_github| Checkout::Branch { name: "worktree-issue-12".parse().unwrap(), earlier: false, no_github };
    /// let file = std::path::PathBuf::from("/s/out/worktree-issue-12.bundle");
    /// assert_eq!(plan(branch(true)).bundle_file(), Some(file));
    /// assert_eq!(plan(branch(false)).bundle_file(), None);
    /// assert_eq!(plan(Checkout::Default).bundle_file(), None);
    /// ```
    pub fn bundle_file(&self) -> Option<PathBuf> {
        match &self.checkout {
            Checkout::Branch {
                name,
                earlier: _,
                no_github: true,
            } => Some(crate::run::bundle_out(self.path.parent()?, name.as_str())),
            Checkout::Branch {
                name: _,
                earlier: _,
                no_github: false,
            }
            | Checkout::Default
            | Checkout::Pull { pull: _, commit: _ } => None,
        }
    }

    /// The branch of the item: the name of [`Checkout::Branch`].
    fn branch(&self) -> Option<&Branch> {
        match &self.checkout {
            Checkout::Branch {
                name,
                earlier: _,
                no_github: _,
            } => Some(name),
            Checkout::Default | Checkout::Pull { pull: _, commit: _ } => None,
        }
    }

    /// Writes the bundle of a session with no GitHub ([`Plan::bundle_file`]):
    /// the commits of the branch that `origin/BASE` does not have. An
    /// older bundle goes first. It gives the file, or `None` with no
    /// bundle to write: a session with GitHub, no branch in the clone, or
    /// no such commit. A fault of git is an error, not "no branch".
    pub fn bundle(&self) -> Result<Option<PathBuf>> {
        let (Some(file), Some(name)) = (self.bundle_file(), self.branch()) else {
            return Ok(None);
        };
        match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(e).with_context(|| format!("cannot remove {}", file.display()));
            }
            _ => {}
        }
        // `for-each-ref` names the branch when the clone has it, and
        // fails only on a fault.
        let head = format!("refs/heads/{name}");
        let listed = git(&self.path, &["for-each-ref", "--format=%(refname)", &head])?;
        if !listed.lines().any(|line| line == head) {
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
        Ok(Some(file))
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

    /// The name `name` of a branch.
    fn name(name: &str) -> Branch {
        name.parse().unwrap()
    }

    /// A bare `origin` that a `file://` URL names, as a URL of GitHub
    /// does, with one commit on `main`, and a main clone of it. The plans
    /// read the main clone as the container sees it: `seen/.git`, with a
    /// link to its [`OBJECTS`] only, and the file [`REFS`] that the host
    /// wrote for it.
    struct Remote {
        dir: tempfile::TempDir,
        url: Origin,
        main: PathBuf,
        seen: PathBuf,
        refs: PathBuf,
    }

    impl Remote {
        fn new() -> Remote {
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
            let seen = root.join("seen/.git");
            std::fs::create_dir_all(&seen).unwrap();
            let objects = main.join(".git").join(OBJECTS);
            std::os::unix::fs::symlink(objects, seen.join(OBJECTS)).unwrap();
            write_refs(&main.join(".git"), root).unwrap();
            let refs = root.join(REFS);
            Remote {
                dir,
                url: url.parse().unwrap(),
                main,
                seen,
                refs,
            }
        }

        /// The plan of a clone at `name` with `checkout`.
        fn plan(&self, folder: &str, checkout: Checkout) -> Plan {
            Plan {
                path: self.dir.path().join(folder).join("clone"),
                origin: self.url.clone(),
                main: self.seen.clone(),
                refs: self.refs.clone(),
                base: name("main"),
                checkout,
            }
        }

        /// The URL of a bare repository `name` in the folder of the test,
        /// which is not there.
        fn none(&self, name: &str) -> Origin {
            format!("file://{}", self.dir.path().join(name).display())
                .parse()
                .unwrap()
        }

        /// A commit of the file `a` with `text` in `clone`.
        fn commit(clone: &Path, text: &str) -> String {
            std::fs::write(clone.join("a"), text).unwrap();
            git_in(clone, &["commit", "-q", "-a", "-m", text]);
            git_in(clone, &["rev-parse", "HEAD"])
        }
    }

    /// The branch `name` of the item of a session with GitHub.
    fn branch(branch: &str, earlier: bool) -> Checkout {
        Checkout::Branch {
            name: name(branch),
            earlier,
            no_github: false,
        }
    }

    /// 01M7EY3P0Q8TQF5QRTRDWCYCTQ: the clone reads the objects of the main
    /// clone through its alternates, and copies none of them. The branch
    /// of the item is new, from the default branch, with no upstream. The
    /// view of the main clone goes after the fetch.
    #[test]
    fn a_clone_links_to_the_main_clone_and_makes_the_branch() {
        let origin = Remote::new();
        let plan = origin.plan("one", branch("worktree-issue-12", false));
        assert_eq!(plan.make(true).unwrap(), Made::New);
        let alternates =
            std::fs::read_to_string(plan.path.join(".git/objects/info/alternates")).unwrap();
        assert_eq!(Path::new(alternates.trim()), origin.seen.join(OBJECTS));
        assert_eq!(own_objects(&plan.path), 0, "the clone copied objects");
        assert!(!plan.path.join(".git").join(VIEW).exists());
        assert_eq!(
            git_in(&plan.path, &["branch", "--show-current"]),
            "worktree-issue-12"
        );
        assert_eq!(
            git_in(&plan.path, &["remote", "get-url", "origin"]),
            origin.url.as_str()
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
        let origin = Remote::new();
        git_in(
            &origin.main,
            &["push", "-q", "origin", "HEAD:refs/heads/gone"],
        );
        git_in(&origin.main, &["fetch", "-q", "origin"]);
        write_refs(&origin.main.join(".git"), origin.dir.path()).unwrap();
        assert!(
            std::fs::read_to_string(&origin.refs)
                .unwrap()
                .contains("refs/remotes/origin/gone")
        );
        let bare = origin.dir.path().join("origin.git");
        git_in(&bare, &["update-ref", "-d", "refs/heads/gone"]);
        let other = origin.plan("other", Checkout::Default);
        other.make(true).unwrap();
        let new = Remote::commit(&other.path, "two");
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
        let origin = Remote::new();
        let mut plan = origin.plan("offline", branch("worktree-issue-12", false));
        plan.origin = "https://github.invalid/o/private.git".parse().unwrap();
        assert_eq!(plan.make(false).unwrap(), Made::New);
        assert_eq!(
            git_in(&plan.path, &["rev-parse", "HEAD"]),
            git_in(&origin.main, &["rev-parse", "origin/main"])
        );
        assert_eq!(own_objects(&plan.path), 0);
        // With a token, the fetch of that `origin` fails, the error names
        // the step, and the folder is empty again.
        let mut fails = origin.plan("fails", Checkout::Default);
        fails.origin = origin.none("none.git");
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
    /// it as the upstream. The integration test `session_clone` runs it
    /// with `checkout.guess=false` in the git config of the person. A
    /// branch that git refuses cannot be in a plan ([`Branch`]).
    #[test]
    fn the_next_session_goes_on_from_the_pushed_branch() {
        let origin = Remote::new();
        let first = origin.plan("first", branch("worktree-issue-12", false));
        first.make(true).unwrap();
        let work = Remote::commit(&first.path, "WIP: two");
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
    }

    /// The lead gets the default branch of `origin`, with its upstream.
    #[test]
    fn the_clone_of_the_lead_is_on_the_default_branch() {
        let origin = Remote::new();
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
        let origin = Remote::new();
        let fork = origin.plan("fork", Checkout::Default);
        fork.make(true).unwrap();
        let head = Remote::commit(&fork.path, "two");
        git_in(
            &fork.path,
            &["push", "-q", "origin", "HEAD:refs/pull/7/head"],
        );
        Remote::commit(&fork.path, "three");
        git_in(
            &fork.path,
            &["push", "-q", "origin", "HEAD:refs/pull/7/head"],
        );
        let pull = |pull: u64| Checkout::Pull {
            pull: PullNumber::try_from(pull).unwrap(),
            commit: head.parse().unwrap(),
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

    /// 01M7EY3WHQQP9330CZ736HK0FJ: a session with no GitHub. Its clone
    /// fetches nothing from GitHub, also with a token: here an `origin`
    /// that is not there. The bundle `out/BRANCH.bundle` beside the clone
    /// holds the commits of the branch, and a clone of the person fetches
    /// them from it. With no commit, there is no bundle. Each bundle takes
    /// the place of the older one. With no branch in the clone, the older
    /// bundle goes, and there is no new one. A fault of git is an error,
    /// not "no branch".
    #[test]
    fn a_session_with_no_github_gives_its_commits_in_a_bundle() {
        let origin = Remote::new();
        let file = origin
            .dir
            .path()
            .join("offline/out/worktree-issue-12.bundle");
        let mut plan = origin.plan(
            "offline",
            Checkout::Branch {
                name: name("worktree-issue-12"),
                earlier: false,
                no_github: true,
            },
        );
        plan.origin = origin.none("none.git");
        assert_eq!(plan.bundle_file(), Some(file.clone()));
        assert!(!plan.fetches(true));
        assert_eq!(plan.make(true).unwrap(), Made::New);
        assert_eq!(plan.bundle().unwrap(), None);
        Remote::commit(&plan.path, "two");
        assert_eq!(plan.bundle().unwrap(), Some(file.clone()));
        let work = Remote::commit(&plan.path, "three");
        assert_eq!(plan.bundle().unwrap(), Some(file.clone()));
        let at = file.to_string_lossy();
        git_in(&origin.main, &["fetch", "-q", &at, "worktree-issue-12"]);
        assert_eq!(git_in(&origin.main, &["rev-parse", "FETCH_HEAD"]), work);
        // A branch whose name starts with that of the branch is not the
        // branch.
        git_in(&plan.path, &["switch", "-q", "--detach"]);
        git_in(&plan.path, &["branch", "-q", "-D", "worktree-issue-12"]);
        git_in(&plan.path, &["branch", "-q", "worktree-issue-12-b"]);
        assert_eq!(plan.bundle().unwrap(), None);
        assert!(!file.exists(), "the older bundle stays");
        // A clone that git cannot read: an error that names git.
        std::fs::write(plan.path.join(".git/HEAD"), "no ref\n").unwrap();
        let error = plan.bundle().unwrap_err();
        assert!(error.to_string().contains("for-each-ref"), "{error}");
        // A session with GitHub writes no bundle.
        let none = origin.plan("none", branch("worktree-issue-12", false));
        assert_eq!(none.bundle().unwrap(), None);
    }

    /// 01M7FMJJB0TZM8875T7GQZAJEA: the host lists the branches of `origin`
    /// of the main clone one time, and the clone starts from that list.
    /// After the list, the main clone packs its refs and loses its branch:
    /// the clone still starts from the list. The list has no symbolic ref,
    /// no branch of the person and no tag. A main clone with each ref
    /// loose gives its branches too.
    #[test]
    fn the_clone_starts_from_the_list_of_refs_that_the_host_wrote() {
        let origin = Remote::new();
        let root = origin.dir.path();
        let loose = root.join("loose");
        std::fs::create_dir(&loose).unwrap();
        git_in(&loose, &["init", "-q", "-b", "main"]);
        git_in(&loose, &["remote", "add", "origin", origin.url.as_str()]);
        git_in(&loose, &["fetch", "-q", "origin"]);
        git_in(
            &loose,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        git_in(&loose, &["update-ref", "refs/heads/mine", "origin/main"]);
        git_in(&loose, &["tag", "v1", "origin/main"]);
        let git_dir = loose.join(".git");
        assert!(!git_dir.join("packed-refs").exists());
        let list = root.join("list");
        std::fs::create_dir(&list).unwrap();
        write_refs(&git_dir, &list).unwrap();
        let refs = list.join(REFS);
        let commit = git_in(&loose, &["rev-parse", "origin/main"]);
        assert_eq!(
            std::fs::read_to_string(&refs).unwrap(),
            format!("{commit} refs/remotes/origin/main\n")
        );
        git_in(&loose, &["pack-refs", "--all", "--prune"]);
        git_in(&loose, &["update-ref", "-d", "refs/remotes/origin/main"]);
        let mut plan = origin.plan("from-loose", Checkout::Default);
        plan.main = git_dir;
        plan.refs = refs;
        assert_eq!(plan.make(false).unwrap(), Made::New);
        assert_eq!(git_in(&plan.path, &["rev-parse", "HEAD"]), commit);
        assert_eq!(own_objects(&plan.path), 0);
    }

    /// 01M7FMJJB0TZM8875T7GQZAJEA, the condition of the lead: the host
    /// writes the list of refs as a new file, and follows no link. A link
    /// or a folder at its path is an error, and the host writes nothing
    /// through the link. A plain file of an earlier start goes, and the
    /// container of that start keeps the list that it sees: here a hard
    /// link, which keeps the file as a bind does.
    #[test]
    fn the_host_writes_the_list_of_refs_through_no_link() {
        let origin = Remote::new();
        let root = origin.dir.path();
        let main = origin.main.join(".git");
        let folder = |name: &str| {
            let folder = root.join(name);
            std::fs::create_dir(&folder).unwrap();
            folder
        };
        let read = |path: &Path| std::fs::read_to_string(path).unwrap();
        let target = root.join("target");
        let linked = folder("linked");
        std::os::unix::fs::symlink(&target, linked.join(REFS)).unwrap();
        let error = write_refs(&main, &linked).unwrap_err();
        let link = crate::text::host_file_link(&linked.join(REFS));
        assert_eq!(error.to_string(), link);
        assert!(!target.exists(), "riff wrote through the link");
        std::fs::write(&target, "mine\n").unwrap();
        assert!(write_refs(&main, &linked).is_err());
        assert_eq!(read(&target), "mine\n");
        let taken = folder("taken");
        std::fs::create_dir(taken.join(REFS)).unwrap();
        assert!(write_refs(&main, &taken).is_err());
        let earlier = folder("earlier");
        std::fs::write(earlier.join(REFS), "old\n").unwrap();
        let seen = root.join("seen-refs");
        std::fs::hard_link(earlier.join(REFS), &seen).unwrap();
        write_refs(&main, &earlier).unwrap();
        let text = read(&earlier.join(REFS));
        assert!(text.ends_with(" refs/remotes/origin/main\n"), "{text}");
        assert_eq!(read(&seen), "old\n");
    }
}
