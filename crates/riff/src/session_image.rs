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
use std::process::Command;
use std::str::FromStr;

use anyhow::{Context, Result, bail};

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
        let config: toml_edit::DocumentMut = text
            .parse()
            .with_context(|| format!("{} is not valid TOML", path.display()))?;
        let image = config
            .get("session")
            .and_then(|session| session.get("image"))
            .and_then(toml_edit::Item::as_str)
            .with_context(|| format!("{} has no image in [session]", path.display()))?;
        image
            .parse()
            .with_context(|| format!("the session image of {}", path.display()))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn digest() -> String {
        format!("sha256:{}", "0123456789abcdef".repeat(4))
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
