//! The session image of a repository, pinned by digest in the riff
//! config of the repository (01M4Z60B7JF80TMP6FHQ04MHCZ).
//!
//! # Design
//!
//! riff 3.0 runs each session in a container from the image of its
//! repository. The repository names that image in its riff config: the
//! file [`FILE`] at its root, with the key `image` in the table
//! `[session]` (01M4Z7SP3KSKJYKNRP1MQ70C4Q):
//!
//! ```toml
//! [session]
//! image = "ghcr.io/como-technologies/riff-session@sha256:…"
//! ```
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
//! takes the text as it is.
//!
//! The pin that counts is the merged pin ([`merged`],
//! 01M51STPHXSSZMHF9T7Z85097R): [`FILE`] on the default branch of
//! `origin`, after a fetch of that branch in the main clone. The wave of
//! the daemon, `riff wave run`, `riff run`, `riff lead start` and the
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
//!     M["the main clone"] -- "git fetch origin BRANCH" --> O["origin/HEAD, for example origin/main"]
//!     O -- "git show origin/main:riff.toml" --> P["the merged pin"]
//!     M -- "riff.toml on disk" --> L["the pin of the working tree"]
//!     P --> U["the wave, riff wave run, riff run,<br/>riff lead start, the update step"]
//!     L -- "another image: one line" --> U
//!     L --> F["riff image, riff init"]
//! ```
//!
//! The image of this repository comes from the folder `deploy/session/`
//! (01M4Z7SP5TGJ3DMWFGE5SN68N1). The workflow "Session image" builds it
//! on each change of that folder, and pushes it to
//! `ghcr.io/como-technologies/riff-session` with the tag `tree-ID`. ID is
//! the git tree of the folder. When that tag is there, the workflow
//! builds nothing (01M4Z7SP7Y19BQ8QTV1T6AE8SR). It fails when [`FILE`]
//! does not pin the digest of that tag (01M4Z7SP9YZ5JCHA8YD8R5A4Y9). So a
//! change of the Dockerfile and the change of its pin go in one pull
//! request.
//!
//! ```mermaid
//! flowchart LR
//!     D["deploy/session/Dockerfile"] -- "each change" --> W["workflow Session image"]
//!     W -- "push tree-ID" --> G[("ghcr.io/como-technologies/riff-session")]
//!     G -- "its digest" --> T["riff.toml: [session] image"]
//!     T --> R["riff image, riff run"]
//!     R -- "podman pull NAME@sha256:…" --> G
//! ```
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
use std::path::Path;
use std::process::{Command, Stdio};
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};

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
    /// config in an error.
    fn of_config(text: &str, place: &str) -> Result<Image> {
        let config: toml_edit::DocumentMut = text
            .parse()
            .with_context(|| format!("{place} is not valid TOML"))?;
        let image = config
            .get("session")
            .and_then(|session| session.get("image"))
            .and_then(toml_edit::Item::as_str)
            .with_context(|| format!("{place} has no image in [session]"))?;
        image
            .parse()
            .with_context(|| format!("the session image of {place}"))
    }
}

/// The merged pin of the main clone `main` (01M51STPHXSSZMHF9T7Z85097R):
/// the image that [`FILE`] pins on the default branch of `origin`, the
/// branch of `origin/HEAD`. First it fetches that branch. When the
/// fetch fails, it reads the branch that the clone has, and gives `say`
/// one line (01M51STPPWYFKQ1ESCNA6V3WV2). When [`FILE`] in the working
/// tree pins another image, or no good image, it gives `say` one line
/// (01M51STPMEM62Z8B26YM2CH6QH). The working tree never changes the
/// image. An error when the clone has no `origin/HEAD`, or when the
/// merged [`FILE`] pins no good image.
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
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn merged(main: &Path, mut say: impl FnMut(&str)) -> Result<Image> {
    let head = git(
        main,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .map_err(|_| anyhow!("{}", text::image_no_origin_head(main)))?;
    let branch = head.strip_prefix("origin/").unwrap_or(&head);
    if let Err(e) = git(main, &["fetch", "--quiet", "origin", branch]) {
        say(&text::image_no_fetch(&head, &format!("{e:#}")));
    }
    let config = git(main, &["show", &format!("{head}:{FILE}")])?;
    let image = Image::of_config(&config, &format!("{FILE} of {head}"))?;
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
    Ok(image)
}

/// Runs `git ARGS` in `dir` with no prompt, and gives its stdout. An
/// error names the command and its stderr, in one line.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
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
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
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
    Image::of_repository(Path::new(top.trim_end_matches('\n')))
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
    let status = Command::new("git")
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

    /// 01M51STPPWYFKQ1ESCNA6V3WV2: a clone with no `origin/HEAD` has no
    /// merged pin. The error gives the fix.
    #[test]
    fn a_clone_with_no_origin_head_has_no_merged_pin() {
        let (_dir, main, _origin) = test_clone(&config("0"));
        test_git(&main, &["remote", "set-head", "origin", "--delete"]);
        let error = format!("{:#}", merged_lines(&main).0.unwrap_err());
        assert_eq!(error, text::image_no_origin_head(&main));
        assert!(error.ends_with("remote set-head origin --auto"), "{error}");
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
