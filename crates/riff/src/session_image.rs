//! The session image of a repository, pinned by digest in the riff
//! config of the repository (01M4Z60B7JF80TMP6FHQ04MHCZ).
//!
//! # Design
//!
//! riff runs each session in a container from the image of its
//! repository. The repository names that image in its riff config: the
//! file [`FILE`] at its root, with the key `image` in the table
//! `[session]` (01M4Z7SP3KSKJYKNRP1MQ70C4Q):
//!
//! ```toml
//! [session]
//! image = "ghcr.io/como-technologies/riff-session@sha256:…"
//! sources = ["crates.io", "index.crates.io", "static.crates.io"]
//! ```
//!
//! The key `sources` is a list of hosts: the package sources that the
//! auto mode of each session trusts ([`sources_of_config`],
//! 01M5SMPA9WN40DMQFNE3V1H4SK). riff adds none by itself, so a config
//! with no `sources` trusts none. riff reads the list from the merged
//! config, as it reads the image ([`merged_sources`]).
//!
//! The tables `[session.env]` and `[session.volumes]` give the env and
//! the volumes of a session (01M5SMVVC4GQS310CVK3H9CMQ7). See
//! [`crate::session_setup`]. They follow the rule of the image: only the
//! merged file counts ([`merged_config`]).
//!
//! An [`Image`] has a registry host, a path and a digest. It has no tag.
//! A tag can move to another image, but a digest names one image. So
//! each session of a repository runs the same image, and only a change of
//! [`FILE`] in a pull request gives the sessions a new image. riff
//! refuses a reference with a tag, also beside a digest. It also refuses
//! a reference with no registry host: for a short name, podman asks
//! which registry to use.
//!
//! `riff image` prints the image of the repository of the current
//! folder ([`of_folder`], 01M4Z7SPC20RWG218EWPEGYZTF). `podman pull`
//! takes the text as it is. `riff image --check` also asks the registry
//! whether a host with no login can pull it
//! ([`crate::registry`], 01M5SMSB0NFBD1XHEW3BSE4BDP).
//!
//! The pin that counts is the merged pin ([`merged`],
//! 01M51STPHXSSZMHF9T7Z85097R): [`FILE`] on the default branch of
//! `origin`, after a fetch of that branch in the main clone. riff asks
//! `origin` for that branch first, and sets `origin/HEAD` of the main
//! clone to it ([`follow_origin`], 01M7G81XGBPP23H9T7V0WX87QZ). The wave of
//! the daemon, `riff host join`, `riff run`, `riff lead start` and the
//! image step of an update use only it. So an edit of [`FILE`] that is
//! not merged, in the main clone or on a branch that is checked out
//! there, never chooses the image that riff pulls and runs. When the
//! working tree of the main clone pins another image, riff says so in
//! one line (01M51STPMEM62Z8B26YM2CH6QH). `riff image`, `riff init` and
//! the image of this folder in `riff update --check` read the working
//! tree ([`of_folder`], 01M51SVXQK037E20R7WAY66532), so a person can try
//! an edit before its merge.
//!
//! ```mermaid
//! flowchart TD
//!     G["origin on GitHub"] -- "git ls-remote --symref origin HEAD" --> M
//!     M["the main clone"] -- "git fetch, git remote set-head origin BRANCH" --> O["origin/HEAD, for example origin/main"]
//!     O -- "git show origin/main:riff.toml" --> P["the merged pin"]
//!     M -- "riff.toml on disk" --> L["the pin of the working tree"]
//!     P --> U["the wave, riff host join, riff run,<br/>riff lead start, the update step"]
//!     L -- "another image: one line" --> U
//!     L --> F["riff image, riff init"]
//! ```
//!
//! The image of this repository comes from the folder `deploy/session/`
//! (01M4Z7SP5TGJ3DMWFGE5SN68N1). It builds `FROM` the `riff-rust` of
//! `deploy/rust/`, and that builds `FROM` the `riff-base` of
//! `deploy/base/` (01M5SNTDW9WVEXZW5D0TKGBA8Z). Each `FROM` pins the
//! digest of the image before it. The workflow "Session image" builds the
//! three in order on each change of a folder, and pushes each to its
//! package on ghcr.io with the tag `tree-ID`. ID is the git tree of the
//! folder. When that tag is there, the workflow builds nothing
//! (01M4Z7SP7Y19BQ8QTV1T6AE8SR). It fails at the first `FROM` that pins
//! another digest, and when [`FILE`] does not pin the digest of the last
//! image (01M5SNTDZ95BRDCPQEKDTYD2PY, 01M4Z7SP9YZ5JCHA8YD8R5A4Y9). So a
//! change of a Dockerfile and the change of its pin go in one pull
//! request.
//!
//! ```mermaid
//! flowchart LR
//!     B["deploy/base/Dockerfile"] -- "each change" --> W["workflow Session image"]
//!     R["deploy/rust/Dockerfile<br/>FROM riff-base@sha256:…"] --> W
//!     D["deploy/session/Dockerfile<br/>FROM riff-rust@sha256:…"] --> W
//!     W -- "push tree-ID" --> G[("ghcr.io/como-technologies/<br/>riff-base, riff-rust, riff-session")]
//!     G -- "the digest of riff-session" --> T["riff.toml: [session] image"]
//!     T -- "the merged pin" --> U["riff run, riff lead start, the waves"]
//!     U -- "podman pull NAME@sha256:…" --> G
//! ```
//!
//! A new project pins `riff-base`, not the image of this repository:
//! [`base`] reads it from the `FROM` of `deploy/rust/Dockerfile`, and `riff
//! init` writes it in `riff.toml` (01M5SNTPEP41NMSP3SEKR7KVB0). The
//! project adds its toolchain in an image of its own.
//!
//! ```
//! use riff::session_image::Image;
//!
//! let digest = format!("sha256:{}", "ab".repeat(32));
//! let text = format!("ghcr.io/como-technologies/riff-session@{digest}");
//! let image: Image = text.parse()?;
//! assert_eq!(image.name(), "ghcr.io/como-technologies/riff-session");
//! assert_eq!(image.digest(), digest);
//! assert_eq!(image.to_string(), text);
//!
//! // A tag can move: riff refuses it, also beside a digest.
//! assert!("ghcr.io/como-technologies/riff-session:latest".parse::<Image>().is_err());
//! assert!(format!("ghcr.io/o/i:1.0@{digest}").parse::<Image>().is_err());
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};

use crate::session_clone::Branch;
use crate::session_setup::Setup;
use crate::text;

/// The riff config of a repository, at the root of the repository.
pub const FILE: &str = "riff.toml";

/// A session image, pinned by its digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    name: String,
    digest: String,
}

impl Image {
    /// The registry host and the path, for example
    /// `ghcr.io/como-technologies/riff-session`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The digest: `sha256:` and 64 hex digits.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// The registry host, with its port when it has one, for example
    /// `ghcr.io`.
    ///
    /// ```
    /// let digest = format!("sha256:{}", "ab".repeat(32));
    /// let image: riff::session_image::Image =
    ///     format!("ghcr.io/como-technologies/riff-session@{digest}").parse()?;
    /// assert_eq!(image.host(), "ghcr.io");
    /// assert_eq!(image.path(), "como-technologies/riff-session");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn host(&self) -> &str {
        self.name
            .split_once('/')
            .map_or(&self.name, |(host, _)| host)
    }

    /// The path in the registry, for example
    /// `como-technologies/riff-session`.
    pub fn path(&self) -> &str {
        self.name.split_once('/').map_or("", |(_, path)| path)
    }

    /// The image that the riff config of the repository at `root` pins.
    ///
    /// ```
    /// use riff::session_image::{FILE, Image};
    ///
    /// let root = tempfile::tempdir()?;
    /// assert!(Image::of_repository(root.path()).is_err());
    ///
    /// let text = format!("ghcr.io/o/i@sha256:{}", "0".repeat(64));
    /// std::fs::write(root.path().join(FILE), format!("[session]\nimage = \"{text}\"\n"))?;
    /// assert_eq!(Image::of_repository(root.path())?.to_string(), text);
    ///
    /// std::fs::write(root.path().join(FILE), "[session]\nimage = \"ghcr.io/o/i:latest\"\n")?;
    /// let error = format!("{:#}", Image::of_repository(root.path()).unwrap_err());
    /// assert!(error.contains("names a tag"), "{error}");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn of_repository(root: &Path) -> Result<Image> {
        let path = root.join(FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read the riff config {}", path.display()))?;
        Image::of_config(&text, &path.display().to_string())
    }

    /// The image that the riff config `text` pins. `place` names the
    /// config in an error. A bad table `[session.env]` or
    /// `[session.volumes]` is an error too ([`Setup::of_config`]): so each
    /// reader of the file, also `riff image`, finds it.
    pub(crate) fn of_config(text: &str, place: &str) -> Result<Image> {
        let config: toml_edit::DocumentMut = text
            .parse()
            .with_context(|| format!("{place} is not valid TOML"))?;
        let image = config
            .get("session")
            .and_then(|session| session.get("image"))
            .and_then(toml_edit::Item::as_str)
            .with_context(|| format!("{place} has no image in [session]"))?;
        let image = image
            .parse()
            .with_context(|| format!("the session image of {place}"))?;
        Setup::of_config(text, place)?;
        Ok(image)
    }
}

/// The merged pin of the main clone `main` (01M51STPHXSSZMHF9T7Z85097R):
/// the image that [`FILE`] pins on the default branch of `origin`, the
/// branch of `origin/HEAD`. First it fetches that branch. When the
/// fetch fails, it reads the branch that the clone has, and gives `say`
/// one line (01M51STPPWYFKQ1ESCNA6V3WV2). When [`FILE`] in the working
/// tree pins another image, or no good image, it gives `say` one line
/// (01M51STPMEM62Z8B26YM2CH6QH). The working tree never changes the
/// image. An error when the clone has no `origin/HEAD`, when that branch
/// has no [`FILE`] (the error names the file, the branch and the fix,
/// 01M5QY976MTPRX5J4N1MKXAGDK), or when the merged [`FILE`] pins no good
/// image.
///
/// ```
/// use std::path::Path;
/// use std::process::Command;
/// use riff::session_image::{FILE, merged};
///
/// let git = |dir: &Path, args: &[&str]| {
///     let status = Command::new("git")
///         .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
///         .arg("-C")
///         .arg(dir)
///         .args(args)
///         .env("GIT_CONFIG_GLOBAL", "/dev/null")
///         .status()
///         .unwrap();
///     assert!(status.success(), "git {args:?}");
/// };
/// let config = |n: &str| format!("[session]\nimage = \"ghcr.io/o/i@sha256:{}\"\n", n.repeat(64));
/// let dir = tempfile::tempdir()?;
/// let (origin, main) = (dir.path().join("origin.git"), dir.path().join("main"));
/// git(dir.path(), &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
/// git(dir.path(), &["init", "-q", "-b", "main", main.to_str().unwrap()]);
/// std::fs::write(main.join(FILE), config("0"))?;
/// git(&main, &["add", FILE]);
/// git(&main, &["commit", "-q", "-m", "pin"]);
/// git(&main, &["remote", "add", "origin", origin.to_str().unwrap()]);
/// git(&main, &["push", "-q", "origin", "main"]);
/// git(&main, &["remote", "set-head", "origin", "main"]);
///
/// // The working tree pins the merged image: no line.
/// let mut lines = Vec::new();
/// let image = merged(&main, |line| lines.push(line.to_owned()))?;
/// assert_eq!(image.to_string(), format!("ghcr.io/o/i@sha256:{}", "0".repeat(64)));
/// assert!(lines.is_empty(), "{lines:?}");
///
/// // An edit that is not merged: riff says so, and uses the merged pin.
/// std::fs::write(main.join(FILE), config("1"))?;
/// assert_eq!(merged(&main, |line| lines.push(line.to_owned()))?, image);
/// assert_eq!(lines.len(), 1);
/// assert!(lines[0].contains("riff uses the merged pin"), "{}", lines[0]);
///
/// // A branch with no riff.toml: the error names the file, the branch and the fix.
/// git(&main, &["rm", "-q", "-f", FILE]);
/// git(&main, &["commit", "-q", "-m", "no pin"]);
/// git(&main, &["push", "-q", "origin", "main"]);
/// let error = merged(&main, |_| {}).unwrap_err().to_string();
/// assert!(error.starts_with("riff.toml is not on origin/main"), "{error}");
/// assert!(error.contains("Fix: add riff.toml"), "{error}");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn merged(main: &Path, say: impl FnMut(&str)) -> Result<Image> {
    Ok(merged_config(main, say)?.image)
}

/// What the merged [`FILE`] gives a session (01M5SMVVC4GQS310CVK3H9CMQ7):
/// the pinned image, and the env and the volumes of the tables
/// `[session.env]` and `[session.volumes]` ([`Setup`]), with the default
/// branch that holds the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    /// The pinned image.
    pub image: Image,
    /// The env and the volumes.
    pub setup: Setup,
    /// The default branch of `origin` ([`follow_origin`]): the start text
    /// and the plan of the clone of `riff run` and of the lead name it
    /// (01M7G81XGBPP23H9T7V0WX87QZ).
    pub base: Branch,
}

/// [`merged`] with the env and the volumes of the merged [`FILE`] too. A
/// bad table of them is an error, as a bad image is.
///
/// ```
/// use riff::session_image::{FILE, merged_config};
///
/// let dir = tempfile::tempdir()?;
/// let git = |dir: &std::path::Path, args: &[&str]| {
///     let status = std::process::Command::new("git")
///         .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
///         .arg("-C")
///         .arg(dir)
///         .args(args)
///         .env("GIT_CONFIG_GLOBAL", "/dev/null")
///         .status()
///         .unwrap();
///     assert!(status.success(), "git {args:?}");
/// };
/// let (origin, main) = (dir.path().join("origin.git"), dir.path().join("main"));
/// git(dir.path(), &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
/// git(dir.path(), &["init", "-q", "-b", "main", main.to_str().unwrap()]);
/// let image = format!("ghcr.io/o/i@sha256:{}", "0".repeat(64));
/// std::fs::write(
///     main.join(FILE),
///     format!("[session]\nimage = \"{image}\"\n\n[session.env]\nA = \"$HOME\"\n"),
/// )?;
/// git(&main, &["add", FILE]);
/// git(&main, &["commit", "-q", "-m", "pin"]);
/// git(&main, &["remote", "add", "origin", origin.to_str().unwrap()]);
/// git(&main, &["push", "-q", "origin", "main"]);
/// git(&main, &["remote", "set-head", "origin", "main"]);
///
/// let merged = merged_config(&main, |_| {})?;
/// assert_eq!(merged.image.to_string(), image);
/// assert_eq!(merged.setup.env, [("A".to_owned(), "$HOME".to_owned())]);
/// assert_eq!(merged.base.as_str(), "main");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn merged_config(main: &Path, mut say: impl FnMut(&str)) -> Result<Merged> {
    let base = follow_origin(main, &mut say)?;
    merged_config_on(main, base, say)
}

/// [`merged_config`] on the default branch `base` that
/// [`follow_origin`] gave: so a caller can say the line of `origin` and
/// not the lines about the pin, as `riff run --image` does. `say` gets
/// each line about the pin.
pub fn merged_config_on(main: &Path, base: Branch, mut say: impl FnMut(&str)) -> Result<Merged> {
    let head = format!("origin/{base}");
    let rev = base.of_origin();
    let config = match git(main, &["show", &format!("{rev}:{FILE}")]) {
        Ok(config) => config,
        Err(_) if lacks_config(main, &rev) => bail!("{}", text::image_no_config(&head)),
        Err(e) => return Err(e),
    };
    let place = format!("{FILE} of {head}");
    let image = Image::of_config(&config, &place)?;
    let setup = Setup::of_config(&config, &place)?;
    match Image::of_repository(main) {
        Ok(local) if local == image => {}
        Ok(local) => say(&text::image_local_differs(
            &main.join(FILE),
            &local.to_string(),
            &image.to_string(),
            &head,
        )),
        Err(e) => say(&text::image_local_bad(
            &format!("{e:#}"),
            &image.to_string(),
            &head,
        )),
    }
    Ok(Merged { image, setup, base })
}

/// The default branch of `origin` of the main clone `main`, in one place
/// (01M7G81XGBPP23H9T7V0WX87QZ): riff asks `origin` for the branch of
/// its `HEAD` with one `git ls-remote --symref`, fetches that branch into
/// `refs/remotes/origin` of the main clone, and sets `origin/HEAD` there
/// to it. So a rename of the default branch on GitHub needs no step of
/// the person, and each later read of `origin/HEAD` ([`merged_sources`],
/// [`merged_here`]) names the same branch. When the look or the fetch
/// fails, it says so with `say` ([`text::image_no_fetch`]), and gives the
/// branch of `origin/HEAD` as the clone has it
/// (01M51STPPWYFKQ1ESCNA6V3WV2). A clone with no `origin/HEAD` then has
/// no default branch: the error gives the fix.
///
/// ```
/// use riff::session_image::follow_origin;
///
/// let dir = tempfile::tempdir()?;
/// let git = |dir: &std::path::Path, args: &[&str]| {
///     let out = std::process::Command::new("git")
///         .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
///         .arg("-C")
///         .arg(dir)
///         .args(args)
///         .env("GIT_CONFIG_GLOBAL", "/dev/null")
///         .output()
///         .unwrap();
///     assert!(out.status.success(), "git {args:?}");
///     String::from_utf8_lossy(&out.stdout).trim().to_owned()
/// };
/// let (origin, main) = (dir.path().join("origin.git"), dir.path().join("main"));
/// git(dir.path(), &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
/// git(dir.path(), &["clone", "-q", origin.to_str().unwrap(), main.to_str().unwrap()]);
/// git(&main, &["commit", "-q", "--allow-empty", "-m", "one"]);
/// git(&main, &["push", "-q", "origin", "HEAD:main"]);
/// git(&main, &["remote", "set-head", "origin", "main"]);
///
/// // The default branch moves to trunk on origin, and the clone fetches
/// // nothing: its origin/HEAD still names main.
/// git(&origin, &["branch", "-m", "main", "trunk"]);
/// let mut said = Vec::new();
/// let base = follow_origin(&main, |line| said.push(line.to_owned()))?;
/// assert_eq!(base.as_str(), "trunk");
/// assert!(said.is_empty(), "{said:?}");
/// let head = git(&main, &["symbolic-ref", "refs/remotes/origin/HEAD"]);
/// assert_eq!(head, "refs/remotes/origin/trunk");
/// assert_eq!(git(&main, &["rev-parse", "origin/trunk"]), git(&origin, &["rev-parse", "trunk"]));
///
/// // With no origin, it says why, and the clone keeps its origin/HEAD.
/// std::fs::remove_dir_all(&origin)?;
/// let base = follow_origin(&main, |line| said.push(line.to_owned()))?;
/// assert_eq!(base.as_str(), "trunk");
/// assert_eq!(said.len(), 1, "{said:?}");
/// assert!(said[0].starts_with("riff cannot fetch origin/trunk: "), "{said:?}");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn follow_origin(main: &Path, mut say: impl FnMut(&str)) -> Result<Branch> {
    let followed = (|| {
        let look = ["ls-remote", "--symref", "origin", "HEAD"];
        crate::git_credential::keyring_open(main, "origin", &look)?;
        let base = origin_head(&git(main, &look)?)?;
        fetch_origin(main, &base)?;
        git(main, &["remote", "set-head", "origin", base.as_str()])?;
        anyhow::Ok(base)
    })();
    followed.or_else(|e| {
        let base = head_here(main)?;
        say(&text::image_no_fetch(
            &format!("origin/{base}"),
            &format!("{e:#}"),
        ));
        Ok(base)
    })
}

/// The refspec of riff that fetches the branch `name` of `origin` into
/// `refs/remotes/origin` of a clone, with force: the one place of it.
/// [`fetch_origin`] fetches with it, and so do the merges of the order
/// ([`crate::order::merges`]) and the council of a pull request
/// ([`crate::council::pick::of_pull`]).
///
/// ```
/// assert_eq!(
///     riff::session_image::refspec("main"),
///     "+refs/heads/main:refs/remotes/origin/main"
/// );
/// ```
pub fn refspec(name: &str) -> String {
    format!("+refs/heads/{name}:refs/remotes/origin/{name}")
}

/// Fetches the branch `branch` from `origin` into `refs/remotes/origin`
/// of the main clone `main`, with no tag ([`refspec`]). Before the
/// `git`, it checks the keyring
/// ([`crate::git_credential::keyring_open`]). The default branch comes
/// this way ([`follow_origin`]), and the branch of the item of `riff run
/// --no-github` ([`crate::run::fetch_for_no_github`]).
///
/// ```
/// use riff::session_image::fetch_origin;
///
/// let dir = tempfile::tempdir()?;
/// let git = |dir: &std::path::Path, args: &[&str]| {
///     let out = std::process::Command::new("git")
///         .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
///         .arg("-C")
///         .arg(dir)
///         .args(args)
///         .env("GIT_CONFIG_GLOBAL", "/dev/null")
///         .output()
///         .unwrap();
///     assert!(out.status.success(), "git {args:?}");
///     String::from_utf8_lossy(&out.stdout).trim().to_owned()
/// };
/// let (origin, main) = (dir.path().join("origin.git"), dir.path().join("main"));
/// git(dir.path(), &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
/// git(dir.path(), &["clone", "-q", origin.to_str().unwrap(), main.to_str().unwrap()]);
/// git(&main, &["commit", "-q", "--allow-empty", "-m", "one"]);
/// git(&main, &["tag", "v1"]);
/// git(&main, &["push", "-q", "origin", "HEAD:main", "HEAD:work", "v1"]);
/// git(&main, &["tag", "-d", "v1"]);
///
/// fetch_origin(&main, &"work".parse()?)?;
/// assert_eq!(git(&main, &["rev-parse", "origin/work"]), git(&origin, &["rev-parse", "work"]));
/// assert_eq!(git(&main, &["tag"]), "");
/// // A branch that origin does not have: the error names the fetch.
/// let error = fetch_origin(&main, &"gone".parse()?).unwrap_err();
/// assert!(format!("{error:#}").starts_with("git fetch "), "{error:#}");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn fetch_origin(main: &Path, branch: &Branch) -> Result<()> {
    let spec = refspec(branch.as_str());
    let fetch = ["fetch", "--quiet", "--no-tags", "origin", &spec];
    crate::git_credential::keyring_open(main, "origin", &fetch)?;
    git(main, &fetch).map(drop)
}

/// The branch of the `HEAD` of `origin` in `listed`, the output of `git
/// ls-remote --symref URL HEAD` ([`follow_origin`]). A `HEAD` that names
/// no branch, as on an empty repository, is the error
/// [`text::ORIGIN_NO_DEFAULT`].
///
/// ```
/// use riff::session_image::origin_head;
///
/// let listed = "ref: refs/heads/trunk\tHEAD\n1a2b\tHEAD\n";
/// assert_eq!(origin_head(listed)?.as_str(), "trunk");
/// assert!(origin_head("1a2b\tHEAD\n").is_err());
/// assert!(origin_head("").is_err());
/// // A name that is no name of a git branch is no default branch.
/// assert!(origin_head("ref: refs/heads/a..b\tHEAD\n").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn origin_head(listed: &str) -> Result<Branch> {
    listed
        .lines()
        .find_map(|line| {
            let (target, name) = line.split_once('\t')?;
            target
                .strip_prefix("ref: refs/heads/")
                .filter(|_| name == "HEAD")
        })
        .ok_or_else(|| anyhow!("{}", text::ORIGIN_NO_DEFAULT))?
        .parse()
}

/// The branch of `origin/HEAD` of the main clone `main`, as the clone has
/// it. With no `origin/HEAD`, the error gives the fix
/// ([`text::image_no_origin_head`]).
fn head_here(main: &Path) -> Result<Branch> {
    let no_head = || anyhow!("{}", text::image_no_origin_head(main));
    let head = git(main, &["symbolic-ref", "refs/remotes/origin/HEAD"]).map_err(|_| no_head())?;
    head.strip_prefix("refs/remotes/origin/")
        .ok_or_else(no_head)?
        .parse()
}

/// The package sources that the riff config `text` names
/// (01M5SMPA9WN40DMQFNE3V1H4SK): the key `sources` of the table
/// `[session]`, a list of hosts. A config with no `sources` names none:
/// riff adds no source by itself. A host that comes twice counts once.
/// `place` names the config in an error. An error when `sources` is not
/// a list of hosts. A host has lowercase letters, digits, `-` and `.`
/// only, so it is no URL and no path.
///
/// ```
/// use riff::session_image::sources_of_config;
///
/// let config = r#"
/// [session]
/// sources = ["pypi.org", "files.pythonhosted.org", "pypi.org"]
/// "#;
/// assert_eq!(sources_of_config(config, "riff.toml")?, ["pypi.org", "files.pythonhosted.org"]);
/// assert!(sources_of_config("[session]\nimage = \"x\"\n", "riff.toml")?.is_empty());
///
/// let error = sources_of_config("[session]\nsources = [\"https://pypi.org\"]\n", "riff.toml");
/// assert!(error.unwrap_err().to_string().contains("not a host"));
/// assert!(sources_of_config("[session]\nsources = \"pypi.org\"\n", "riff.toml").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn sources_of_config(text: &str, place: &str) -> Result<Vec<String>> {
    let config: toml_edit::DocumentMut = text
        .parse()
        .with_context(|| format!("{place} is not valid TOML"))?;
    let Some(item) = config
        .get("session")
        .and_then(|session| session.get("sources"))
    else {
        return Ok(Vec::new());
    };
    let list = item
        .as_array()
        .with_context(|| format!("`sources` in [session] of {place} is not a list of hosts"))?;
    let mut sources: Vec<String> = Vec::new();
    for value in list {
        let host = value.as_str().filter(|h| is_host(h)).with_context(|| {
            format!(
                "`sources` in [session] of {place} has {}, which is not a host, \
                 for example pypi.org",
                value.to_string().trim()
            )
        })?;
        if !sources.iter().any(|s| s == host) {
            sources.push(host.to_owned());
        }
    }
    Ok(sources)
}

/// True when `text` is a host name: labels of lowercase letters, digits
/// and `-`, joined by `.`. A label does not start or end with `-`.
fn is_host(text: &str) -> bool {
    text.split('.').all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'))
    })
}

/// The merged package sources of the main clone `main`
/// (01M5SMPA9WN40DMQFNE3V1H4SK): the `sources` that [`FILE`] names on
/// the default branch of `origin`, as [`merged`] reads the image. The
/// working tree never changes them, so an edit in a clone trusts no
/// source. A clone with no `origin/HEAD`, and a branch with no [`FILE`],
/// have none. An error when the merged [`FILE`] has a `sources` that is
/// not a list of hosts.
///
/// ```
/// use std::process::Command;
/// use riff::session_image::{FILE, merged_sources};
///
/// let dir = tempfile::tempdir()?;
/// let (origin, main) = (dir.path().join("origin.git"), dir.path().join("main"));
/// let git = |dir: &std::path::Path, args: &[&str]| {
///     let status = Command::new("git")
///         .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
///         .arg("-C")
///         .arg(dir)
///         .args(args)
///         .env("GIT_CONFIG_GLOBAL", "/dev/null")
///         .status()
///         .unwrap();
///     assert!(status.success(), "git {args:?}");
/// };
/// git(dir.path(), &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
/// git(dir.path(), &["init", "-q", "-b", "main", main.to_str().unwrap()]);
/// assert!(merged_sources(&main)?.is_empty());
/// std::fs::write(main.join(FILE), "[session]\nsources = [\"pypi.org\"]\n")?;
/// git(&main, &["add", FILE]);
/// git(&main, &["commit", "-q", "-m", "sources"]);
/// git(&main, &["remote", "add", "origin", origin.to_str().unwrap()]);
/// git(&main, &["push", "-q", "origin", "main"]);
/// git(&main, &["remote", "set-head", "origin", "main"]);
/// assert_eq!(merged_sources(&main)?, ["pypi.org"]);
///
/// // An edit that is not merged trusts no source.
/// std::fs::write(main.join(FILE), "[session]\nsources = [\"pypi.org\", \"npmjs.org\"]\n")?;
/// assert_eq!(merged_sources(&main)?, ["pypi.org"]);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn merged_sources(main: &Path) -> Result<Vec<String>> {
    let Ok(base) = head_here(main) else {
        return Ok(Vec::new());
    };
    let show = format!("{}:{FILE}", base.of_origin());
    let Ok(config) = git(main, &["show", &show]) else {
        return Ok(Vec::new());
    };
    sources_of_config(&config, &format!("{FILE} of origin/{base}"))
}

/// The merged pin of the main clone `main` as the clone has it now
/// (01M5TKPG5WZ1T58FNAMZNP6CJ5): the image that [`FILE`] pins on the
/// default branch of `origin`. Unlike [`merged`], it fetches nothing and
/// says nothing, so the facts of the daemon can read it at each look.
/// The latest start of a session fetched that branch. An error when the
/// clone has no `origin/HEAD`, when that branch has no [`FILE`], or when
/// it pins no good image.
///
/// ```
/// use std::process::Command;
/// use riff::session_image::{FILE, merged_here};
///
/// let dir = tempfile::tempdir()?;
/// let (origin, main) = (dir.path().join("origin.git"), dir.path().join("main"));
/// let git = |dir: &std::path::Path, args: &[&str]| {
///     let status = Command::new("git")
///         .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
///         .arg("-C")
///         .arg(dir)
///         .args(args)
///         .env("GIT_CONFIG_GLOBAL", "/dev/null")
///         .status()
///         .unwrap();
///     assert!(status.success(), "git {args:?}");
/// };
/// let config = |n: &str| format!("[session]\nimage = \"ghcr.io/o/i@sha256:{}\"\n", n.repeat(64));
/// git(dir.path(), &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
/// git(dir.path(), &["init", "-q", "-b", "main", main.to_str().unwrap()]);
/// assert!(merged_here(&main).is_err());
/// std::fs::write(main.join(FILE), config("0"))?;
/// git(&main, &["add", FILE]);
/// git(&main, &["commit", "-q", "-m", "pin"]);
/// git(&main, &["remote", "add", "origin", origin.to_str().unwrap()]);
/// git(&main, &["push", "-q", "origin", "main"]);
/// git(&main, &["remote", "set-head", "origin", "main"]);
/// assert_eq!(merged_here(&main)?.digest(), format!("sha256:{}", "0".repeat(64)));
///
/// // An edit that is not merged changes nothing.
/// std::fs::write(main.join(FILE), config("1"))?;
/// assert_eq!(merged_here(&main)?.digest(), format!("sha256:{}", "0".repeat(64)));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn merged_here(main: &Path) -> Result<Image> {
    let base = head_here(main)?;
    let config = git(main, &["show", &format!("{}:{FILE}", base.of_origin())])?;
    Image::of_config(&config, &format!("{FILE} of origin/{base}"))
}

/// True when the branch `head` is there, and has no [`FILE`].
fn lacks_config(main: &Path, head: &str) -> bool {
    let has = |spec: String| git(main, &["rev-parse", "--verify", "--quiet", &spec]).is_ok();
    has(format!("{head}^{{commit}}")) && !has(format!("{head}:{FILE}"))
}

/// Runs `git ARGS` in `dir` with no prompt, and gives its stdout. An
/// error names the command and its stderr, in one line.
pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = crate::host_git::git(dir)?
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .context("cannot run git")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let lines: Vec<&str> = stderr
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        bail!("git {}: {}", args.join(" "), lines.join(" "));
    }
    let out = String::from_utf8(out.stdout).context("the output of git is not UTF-8")?;
    Ok(out.trim_end().to_owned())
}

/// The Dockerfile of `riff-rust` at the commit of the build. Its `FROM`
/// pins the `riff-base` of the release.
const RUST_DOCKERFILE: &str = include_str!("../../../deploy/rust/Dockerfile");

/// The `riff-base` that the release of riff that runs pins
/// (01M5SNTPEP41NMSP3SEKR7KVB0): the image in the first `FROM` of
/// `deploy/rust/Dockerfile` at the commit of the build. `riff project init` pins
/// it in a repository that has no [`FILE`], and the daemon names it to the
/// lead of a repository whose image has an old tool
/// ([`crate::image_tools`]).
///
/// ```
/// let image = riff::session_image::base();
/// assert!(image.name().ends_with("/riff-base"), "{image}");
/// assert!(image.digest().starts_with("sha256:"), "{image}");
/// ```
pub fn base() -> Image {
    from_of(RUST_DOCKERFILE)
        .and_then(|from| from.parse().ok())
        .expect("deploy/rust/Dockerfile builds FROM a riff-base pinned by digest")
}

/// The image in the first `FROM` line of the Dockerfile `dockerfile`.
fn from_of(dockerfile: &str) -> Option<&str> {
    dockerfile
        .lines()
        .find_map(|line| line.strip_prefix("FROM "))
        .map(str::trim)
}

/// The text of a [`FILE`] that pins `image`: the file that the how-to
/// "Add another repository to the riff" writes.
///
/// ```
/// let image = riff::session_image::base();
/// let text = riff::session_image::config_of(&image);
/// assert_eq!(text, format!("[session]\nimage = \"{image}\"\n"));
/// ```
pub fn config_of(image: &Image) -> String {
    format!("[session]\nimage = \"{image}\"\n")
}

/// The image that the riff config of the repository of the folder `dir`
/// pins: the file [`FILE`] at the top of its git work tree.
///
/// ```
/// use riff::session_image::{FILE, of_folder};
///
/// let root = tempfile::tempdir()?;
/// let status = std::process::Command::new("git")
///     .args(["init", "-q"])
///     .arg(root.path())
///     .status()?;
/// assert!(status.success());
/// let text = format!("ghcr.io/o/i@sha256:{}", "0".repeat(64));
/// std::fs::write(root.path().join(FILE), format!("[session]\nimage = \"{text}\"\n"))?;
/// std::fs::create_dir(root.path().join("src"))?;
/// assert_eq!(of_folder(&root.path().join("src"))?.to_string(), text);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn of_folder(dir: &Path) -> Result<Image> {
    Image::of_repository(&top(dir)?)
}

/// The top of the git work tree of the folder `dir`.
pub(crate) fn top(dir: &Path) -> Result<PathBuf> {
    let out = crate::host_git::git(dir)?
        .args(["rev-parse", "--show-toplevel"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("cannot run git")?;
    if !out.status.success() {
        bail!(
            "{} is not in a git repository: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let top = String::from_utf8(out.stdout).context("the path of the repository is not UTF-8")?;
    Ok(top.trim_end_matches('\n').into())
}

impl FromStr for Image {
    type Err = anyhow::Error;

    /// Reads `HOST/PATH@sha256:DIGEST`. It refuses a reference with no
    /// registry host, a tag, and a reference with no digest.
    fn from_str(text: &str) -> Result<Image> {
        let (name, digest) = text
            .split_once('@')
            .map_or((text, None), |(n, d)| (n, Some(d)));
        let Some((host, path)) = name.split_once('/') else {
            bail!("{text} has no registry host, for example ghcr.io/");
        };
        if !(host.contains('.') || host.contains(':') || host == "localhost") {
            bail!("{text} has no registry host, for example ghcr.io/");
        }
        if path.contains(':') {
            bail!("{text} names a tag: a tag can move. Pin the image as HOST/PATH@sha256:DIGEST");
        }
        let Some(digest) = digest else {
            bail!("{text} has no digest: pin the image as HOST/PATH@sha256:DIGEST");
        };
        let Some(hex) = digest.strip_prefix("sha256:") else {
            bail!("the digest of {text} does not start with sha256:");
        };
        if hex.len() != 64 || !hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            bail!("the digest of {text} is not 64 lowercase hex digits");
        }
        let allowed = |b: u8| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-');
        if !host.bytes().all(|b| allowed(b) || b == b':')
            || path
                .split('/')
                .any(|part| part.is_empty() || !part.bytes().all(allowed))
        {
            bail!("{text} is not a valid image name: use lowercase letters, digits, . _ - and /");
        }
        Ok(Image {
            name: name.to_owned(),
            digest: digest.to_owned(),
        })
    }
}

impl fmt::Display for Image {
    /// `HOST/PATH@sha256:DIGEST`, the reference that podman pulls.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.digest)
    }
}

/// A main clone for a test, with `config` as [`FILE`] on `origin/main`:
/// the folder of the test, the main clone and the bare origin.
#[cfg(test)]
pub(crate) fn test_clone(
    config: &str,
) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let (origin, main) = (dir.path().join("origin.git"), dir.path().join("main"));
    test_git(
        dir.path(),
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            origin.to_str().unwrap(),
        ],
    );
    test_git(
        dir.path(),
        &["init", "-q", "-b", "main", main.to_str().unwrap()],
    );
    std::fs::write(main.join(FILE), config).unwrap();
    test_git(&main, &["add", FILE]);
    test_git(&main, &["commit", "-q", "-m", "pin"]);
    test_git(
        &main,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    test_git(&main, &["push", "-q", "origin", "main"]);
    test_git(&main, &["remote", "set-head", "origin", "main"]);
    (dir, main, origin)
}

/// Runs `git ARGS` in `dir` for a test, with no config of the person.
#[cfg(test)]
pub(crate) fn test_git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest() -> String {
        format!("sha256:{}", "0123456789abcdef".repeat(4))
    }

    /// A riff config that pins the image of the digest digit `n`.
    fn config(n: &str) -> String {
        format!("[session]\nimage = \"{}\"\n", pin(n))
    }

    fn pin(n: &str) -> String {
        format!("ghcr.io/o/i@sha256:{}", n.repeat(64))
    }

    fn merged_lines(main: &Path) -> (Result<Image>, Vec<String>) {
        let mut lines = Vec::new();
        let image = merged(main, |line| lines.push(line.to_owned()));
        (image, lines)
    }

    /// 01M51STPHXSSZMHF9T7Z85097R: a pin that is merged after the last
    /// fetch of the clone counts: riff fetches the default branch first.
    #[test]
    fn the_merged_pin_is_the_one_on_origin_after_a_fetch() {
        let (dir, main, origin) = test_clone(&config("0"));
        let other = dir.path().join("other");
        test_git(
            dir.path(),
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        );
        std::fs::write(other.join(FILE), config("1")).unwrap();
        test_git(&other, &["commit", "-q", "-am", "a new pin"]);
        test_git(&other, &["push", "-q", "origin", "main"]);

        let (image, lines) = merged_lines(&main);
        assert_eq!(image.unwrap().to_string(), pin("1"));
        // The working tree still has the old pin: one line says so.
        assert_eq!(
            lines,
            [text::image_local_differs(
                &main.join(FILE),
                &pin("0"),
                &pin("1"),
                "origin/main"
            )]
        );
    }

    /// 01M51STPPWYFKQ1ESCNA6V3WV2: with no origin to fetch from, riff
    /// says so and reads the branch that the clone has.
    #[test]
    fn a_failed_fetch_reads_the_branch_that_the_clone_has() {
        let (_dir, main, origin) = test_clone(&config("0"));
        std::fs::remove_dir_all(&origin).unwrap();
        let (image, lines) = merged_lines(&main);
        assert_eq!(image.unwrap().to_string(), pin("0"));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with("riff cannot fetch origin/main: "),
            "{}",
            lines[0]
        );
        assert!(
            lines[0].ends_with("riff uses riff.toml of origin/main as the clone has it."),
            "{}",
            lines[0]
        );
    }

    /// 01M51STPPWYFKQ1ESCNA6V3WV2: after a failed fetch, a clone with no
    /// `origin/HEAD` has no merged pin. The error gives the fix.
    #[test]
    fn a_clone_with_no_origin_head_has_no_merged_pin() {
        let (_dir, main, origin) = test_clone(&config("0"));
        test_git(&main, &["remote", "set-head", "origin", "--delete"]);
        std::fs::remove_dir_all(&origin).unwrap();
        let error = format!("{:#}", merged_lines(&main).0.unwrap_err());
        assert_eq!(error, text::image_no_origin_head(&main));
        assert!(error.ends_with("remote set-head origin --auto"), "{error}");
    }

    /// 01M7G81XGBPP23H9T7V0WX87QZ: the step of the image sets
    /// `origin/HEAD` of the main clone from `origin`. A clone with no
    /// `origin/HEAD` gets it, and a rename on `origin` moves it: the pin,
    /// the sources and the pin of the facts ([`merged_here`]) come from
    /// the new default branch, and the merged config names it.
    #[test]
    fn the_step_of_the_image_sets_origin_head_from_origin() {
        let (dir, main, origin) = test_clone(&config("0"));
        test_git(&main, &["remote", "set-head", "origin", "--delete"]);
        let (image, lines) = merged_lines(&main);
        assert_eq!(image.unwrap().to_string(), pin("0"));
        assert!(lines.is_empty(), "{lines:?}");

        // Another clone puts a new pin and a source on trunk, and the
        // default branch moves there. The main clone fetches nothing.
        let other = dir.path().join("other");
        let url = origin.to_str().unwrap();
        test_git(dir.path(), &["clone", "-q", url, other.to_str().unwrap()]);
        let trunk = format!("{}sources = [\"pypi.org\"]\n", config("1"));
        std::fs::write(other.join(FILE), &trunk).unwrap();
        test_git(&other, &["commit", "-q", "-am", "a new pin"]);
        test_git(&other, &["push", "-q", "origin", "HEAD:trunk"]);
        test_git(&origin, &["symbolic-ref", "HEAD", "refs/heads/trunk"]);
        std::fs::write(main.join(FILE), &trunk).unwrap();

        let found = merged_config(&main, |_| {}).unwrap();
        assert_eq!(found.base.as_str(), "trunk");
        assert_eq!(found.image.to_string(), pin("1"));
        assert_eq!(merged_here(&main).unwrap().to_string(), pin("1"));
        assert_eq!(merged_sources(&main).unwrap(), ["pypi.org"]);
    }

    /// 01M5QY976MTPRX5J4N1MKXAGDK: an `origin` with no `riff.toml` on its
    /// default branch has no merged pin. The error names the file, the
    /// branch and the fix, not the error of git. A `riff.toml` in the
    /// working tree that is not merged does not help.
    #[test]
    fn an_origin_with_no_riff_toml_gives_the_error_with_the_fix() {
        let (_dir, main, _origin) = test_clone(&config("0"));
        test_git(&main, &["rm", "-q", FILE]);
        test_git(&main, &["commit", "-q", "-m", "no pin"]);
        test_git(&main, &["push", "-q", "origin", "main"]);
        std::fs::write(main.join(FILE), config("1")).unwrap();

        let (image, lines) = merged_lines(&main);
        let error = format!("{:#}", image.unwrap_err());
        assert_eq!(error, text::image_no_config("origin/main"));
        assert!(
            error.starts_with("riff.toml is not on origin/main"),
            "{error}"
        );
        assert!(error.contains("[session]"), "{error}");
        assert!(
            error.contains("Add another repository to the riff"),
            "{error}"
        );
        assert!(!error.contains("git show"), "{error}");
        assert!(!error.contains("fatal"), "{error}");
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// 01M5QY976MTPRX5J4N1MKXAGDK: a branch that git cannot find is no
    /// branch with no `riff.toml`: the error stays the error of git. With
    /// no origin, the clone keeps its `origin/HEAD`.
    #[test]
    fn a_branch_that_is_not_there_keeps_the_error_of_git() {
        let (_dir, main, origin) = test_clone(&config("0"));
        test_git(
            &main,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/nope",
            ],
        );
        std::fs::remove_dir_all(&origin).unwrap();
        let (image, _) = merged_lines(&main);
        let error = format!("{:#}", image.unwrap_err());
        assert!(
            error.starts_with("git show refs/remotes/origin/nope:riff.toml: "),
            "{error}"
        );
    }

    /// 01M51STPMEM62Z8B26YM2CH6QH: a working tree with no riff config or
    /// with a tag gives one line with why, and the merged pin.
    #[test]
    fn a_working_tree_with_no_good_pin_gives_one_line_with_why() {
        let (_dir, main, _origin) = test_clone(&config("0"));
        std::fs::remove_file(main.join(FILE)).unwrap();
        let (image, lines) = merged_lines(&main);
        assert_eq!(image.unwrap().to_string(), pin("0"));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with("cannot read the riff config "),
            "{}",
            lines[0]
        );
        let tail = format!("riff uses the merged pin {} of origin/main.", pin("0"));
        assert!(lines[0].ends_with(&tail), "{}", lines[0]);

        std::fs::write(
            main.join(FILE),
            "[session]\nimage = \"ghcr.io/o/i:latest\"\n",
        )
        .unwrap();
        let (image, lines) = merged_lines(&main);
        assert_eq!(image.unwrap().to_string(), pin("0"));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("names a tag"), "{}", lines[0]);
        assert!(lines[0].ends_with(&tail), "{}", lines[0]);
    }

    /// The merged config pins a tag: riff refuses it, and names the
    /// branch. The working tree does not help.
    #[test]
    fn a_merged_pin_with_a_tag_is_an_error() {
        let (_dir, main, _origin) = test_clone("[session]\nimage = \"ghcr.io/o/i:latest\"\n");
        std::fs::write(main.join(FILE), config("0")).unwrap();
        let error = format!("{:#}", merged_lines(&main).0.unwrap_err());
        assert!(
            error.starts_with("the session image of riff.toml of origin/main: "),
            "{error}"
        );
        assert!(error.contains("names a tag"), "{error}");
    }

    /// 01M5SMVVC4GQS310CVK3H9CMQ7: the env and the volumes come from the
    /// merged file, not from the working tree.
    #[test]
    fn the_env_and_the_volumes_are_the_ones_of_the_merged_file() {
        let merged = format!(
            "{}\n[session.env]\nA = \"$HOME/a\"\n\n[session.volumes]\nv = \"/v\"\n",
            config("0")
        );
        let (_dir, main, _origin) = test_clone(&merged);
        std::fs::write(main.join(FILE), config("0")).unwrap();
        let found = merged_config(&main, |_| {}).unwrap();
        assert_eq!(found.image.to_string(), pin("0"));
        assert_eq!(found.setup.env, [("A".to_owned(), "$HOME/a".to_owned())]);
        assert_eq!(found.setup.volumes, [("v".to_owned(), "/v".to_owned())]);
    }

    /// 01M5SMW2B4GMJ3V6BCX1KGC1ZY: a bad table in the merged file stops
    /// riff, and the error names the file and the branch.
    #[test]
    fn a_bad_env_in_the_merged_file_is_an_error_that_names_the_branch() {
        let bad = format!("{}\n[session.env]\nGH_TOKEN = \"x\"\n", config("0"));
        let (_dir, main, _origin) = test_clone(&bad);
        let error = format!("{:#}", merged_config(&main, |_| {}).unwrap_err());
        assert!(error.starts_with("riff.toml of origin/main: "), "{error}");
        assert!(
            error.contains("[session.env] GH_TOKEN: no session gets a token of GitHub"),
            "{error}"
        );
        // `merged` has the same rule.
        assert!(merged(&main, |_| {}).is_err());
    }

    /// 01M5SMW2B4GMJ3V6BCX1KGC1ZY: a bad table is a bad config for each
    /// reader of the file, also for the working tree (`riff image`).
    #[test]
    fn a_bad_env_makes_the_config_of_the_working_tree_bad() {
        let root = tempfile::tempdir().unwrap();
        let bad = format!("{}\n[session.volumes]\n\"/etc\" = \"/etc\"\n", config("0"));
        std::fs::write(root.path().join(FILE), bad).unwrap();
        let error = format!("{:#}", Image::of_repository(root.path()).unwrap_err());
        assert!(
            error.contains("[session.volumes] /etc is not the name of a volume"),
            "{error}"
        );
        std::fs::write(root.path().join(FILE), config("0")).unwrap();
        assert!(Image::of_repository(root.path()).is_ok());
    }

    fn refused(text: &str, why: &str) {
        let error = text.parse::<Image>().unwrap_err().to_string();
        assert!(error.contains(why), "{text}: {error}");
    }

    #[test]
    fn an_image_with_a_host_a_path_and_a_digest_is_good() {
        let d = digest();
        for name in [
            "ghcr.io/como-technologies/riff-session",
            "localhost/riff-session",
            "localhost:5000/a/b_c.d-e",
            "docker.io/library/debian",
        ] {
            let image: Image = format!("{name}@{d}").parse().unwrap();
            assert_eq!(image.name(), name);
            assert_eq!(image.digest(), d);
            assert_eq!(image.to_string(), format!("{name}@{d}"));
        }
    }

    #[test]
    fn a_tag_is_refused() {
        let d = digest();
        refused("ghcr.io/o/i:latest", "names a tag");
        refused("localhost:5000/i:v1", "names a tag");
        refused("ghcr.io/o/i", "has no digest");
        refused(&format!("ghcr.io/o/i:1.0@{d}"), "names a tag");
        refused(&format!("localhost:5000/i:v1@{d}"), "names a tag");
    }

    #[test]
    fn a_bad_digest_is_refused() {
        let hex = "0".repeat(64);
        refused(
            &format!("ghcr.io/o/i@sha512:{hex}"),
            "does not start with sha256:",
        );
        refused("ghcr.io/o/i@sha256:0123", "64 lowercase hex");
        refused(
            &format!("ghcr.io/o/i@sha256:{}", "A".repeat(64)),
            "64 lowercase hex",
        );
        refused(
            &format!("ghcr.io/o/i@sha256:{hex}@sha256:{hex}"),
            "64 lowercase hex",
        );
    }

    #[test]
    fn a_short_or_bad_name_is_refused() {
        let d = digest();
        refused(&format!("debian@{d}"), "no registry host");
        refused(&format!("library/debian@{d}"), "no registry host");
        refused(&format!("ghcr.io/O/i@{d}"), "not a valid image name");
        refused(&format!("ghcr.io//i@{d}"), "not a valid image name");
        refused(&format!("ghcr.io/o i@{d}"), "not a valid image name");
    }

    /// 01M5SNTPEP41NMSP3SEKR7KVB0: the base of the release is the image in
    /// the first `FROM` of the Dockerfile of `riff-rust`, pinned by digest.
    #[test]
    fn the_base_of_the_release_is_the_from_of_the_rust_image() {
        let dockerfile = "# A comment\nFROM ghcr.io/o/riff-base@sha256:0\nFROM other\n";
        assert_eq!(from_of(dockerfile), Some("ghcr.io/o/riff-base@sha256:0"));
        assert_eq!(from_of("ARG X=1\n"), None);
        let image = base();
        assert_eq!(image.name(), "ghcr.io/como-technologies/riff-base");
        assert_eq!(from_of(RUST_DOCKERFILE), Some(image.to_string().as_str()));
    }

    #[test]
    fn a_config_with_no_image_names_the_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(FILE), "[session]\n").unwrap();
        let error = format!("{:#}", Image::of_repository(root.path()).unwrap_err());
        assert!(
            error.contains("riff.toml has no image in [session]"),
            "{error}"
        );
        std::fs::write(root.path().join(FILE), "[session\n").unwrap();
        let error = format!("{:#}", Image::of_repository(root.path()).unwrap_err());
        assert!(error.contains("is not valid TOML"), "{error}");
    }
}
