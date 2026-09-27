//! The build of `riff` and `riff-server`, and the check that they match.
//!
//! # Design
//!
//! A message is valid only between builds that match. For now, two
//! builds match when they have the same crate version and the same
//! commit (01M3JEE7KZR5VVJGZQD82AA6NH). The crate version stays
//! `0.1.0`, so the commit tells the builds apart.
//!
//! The commit is the last commit that changed the code: `crates`,
//! `Cargo.toml` or `Cargo.lock`. A commit that changes only the book
//! keeps the match. `build.rs` asks git. The image build of
//! `riff-server` has no git: it gives `RIFF_COMMIT` and
//! `RIFF_COMMIT_TIME`, from `deploy/build-id.sh`, which runs the same
//! git command.
//!
//! Each call of `riff` carries its build in the header [`HEADER`], and
//! each reply of `riff-server` carries the build of the server
//! (01M3JEE7P46GWXR1BD4Q1TTSGN). Each side refuses a build that does not
//! match, or no build:
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff
//!     participant S as riff-server
//!     R->>S: call, riff-build: 0.1.0 929605821e54 2026-09-27T22:03:01Z
//!     alt the builds match
//!         S-->>R: reply, riff-build: the same build
//!     else they do not match, or the call has no build
//!         S-->>R: 409, riff-build: the build of the server, the mismatch
//!     end
//!     Note over R: a reply with no build, or another build,<br/>is a Mismatch too (an old riff-server)
//! ```

use std::fmt;

/// The HTTP header that carries a [`Build`].
pub const HEADER: &str = "riff-build";

/// The build of this binary as text, for `--version`
/// (01M3JEE7WT04BKX377VW5GDSPY).
pub const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " ",
    env!("RIFF_BUILD_COMMIT"),
    " ",
    env!("RIFF_BUILD_TIME")
);

/// The part of the book that tells how to update each side.
pub const UPDATE_URL: &str =
    "https://como-technologies.github.io/riff/how-it-works.html#when-the-builds-do-not-match";

/// One build of riff: the crate version, the commit and its UTC time.
///
/// ```
/// use riff_core::build::Build;
///
/// let b: Build = "0.1.0 929605821e54 2026-09-27T22:03:01Z".parse().unwrap();
/// assert_eq!(b.commit, "929605821e54");
/// assert_eq!(b.to_string(), "0.1.0 929605821e54 2026-09-27T22:03:01Z");
/// assert!("0.1.0 929605821e54".parse::<Build>().is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    /// The crate version.
    pub version: String,
    /// The last commit that changed the code.
    pub commit: String,
    /// The time of that commit, in UTC.
    pub time: String,
}

impl Build {
    /// The build of this binary.
    ///
    /// ```
    /// let me = riff_core::build::Build::this();
    /// assert_eq!(me.version, env!("CARGO_PKG_VERSION"));
    /// assert!(!me.commit.is_empty());
    /// ```
    pub fn this() -> Build {
        Build {
            version: env!("CARGO_PKG_VERSION").into(),
            commit: env!("RIFF_BUILD_COMMIT").into(),
            time: env!("RIFF_BUILD_TIME").into(),
        }
    }

    /// The build that a reply or a request names in [`HEADER`]. `None`
    /// when it names none, or names it in another form.
    pub fn from_header(value: Option<&[u8]>) -> Option<Build> {
        std::str::from_utf8(value?).ok()?.parse().ok()
    }

    /// True when a message is valid between the two builds: the same
    /// version and the same commit.
    ///
    /// ```
    /// use riff_core::build::Build;
    ///
    /// let a: Build = "0.1.0 aaaa 2026-09-27T10:00:00Z".parse().unwrap();
    /// let b: Build = "0.1.0 bbbb 2026-09-27T11:00:00Z".parse().unwrap();
    /// assert!(a.matches(&a.clone()));
    /// assert!(!a.matches(&b));
    /// ```
    pub fn matches(&self, other: &Build) -> bool {
        self.version == other.version && self.commit == other.commit
    }
}

impl fmt::Display for Build {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.version, self.commit, self.time)
    }
}

impl std::str::FromStr for Build {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        match parts[..] {
            [version, commit, time] => Ok(Build {
                version: version.into(),
                commit: commit.into(),
                time: time.into(),
            }),
            _ => Err(format!("{s:?} is not a riff build")),
        }
    }
}

/// `riff` and its `riff-server` have builds that do not match
/// (01M3JEE7RDTDD3KQMKH41E8D57). `None` is a side that sent no build: a
/// build from before this check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    /// The build of `riff`.
    pub riff: Option<Build>,
    /// The build of `riff-server`.
    pub server: Option<Build>,
}

impl Mismatch {
    /// The side to update: the older build. A side with no build is
    /// older. With the same time, both.
    ///
    /// ```
    /// use riff_core::build::{Build, Mismatch, Side};
    ///
    /// let old: Build = "0.1.0 aaaa 2026-09-27T10:00:00Z".parse().unwrap();
    /// let new: Build = "0.1.0 bbbb 2026-09-27T11:00:00Z".parse().unwrap();
    /// let m = |riff: Option<&Build>, server: Option<&Build>| Mismatch {
    ///     riff: riff.cloned(),
    ///     server: server.cloned(),
    /// }.older();
    /// assert_eq!(m(Some(&old), Some(&new)), Side::Riff);
    /// assert_eq!(m(Some(&new), Some(&old)), Side::Server);
    /// assert_eq!(m(Some(&new), None), Side::Server);
    /// assert_eq!(m(None, Some(&new)), Side::Riff);
    /// assert_eq!(m(Some(&new), Some(&new)), Side::Both);
    /// ```
    pub fn older(&self) -> Side {
        match (&self.riff, &self.server) {
            (Some(r), Some(s)) if r.time < s.time => Side::Riff,
            (Some(r), Some(s)) if r.time > s.time => Side::Server,
            (Some(_), None) => Side::Server,
            (None, Some(_)) => Side::Riff,
            _ => Side::Both,
        }
    }
}

/// The side of a [`Mismatch`] to update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Update `riff` on the machine of the session.
    Riff,
    /// Update the `riff-server` of the riff.
    Server,
    /// Update both to the same commit.
    Both,
}

impl fmt::Display for Mismatch {
    /// ```
    /// use riff_core::build::Mismatch;
    ///
    /// let m = Mismatch {
    ///     riff: Some("0.1.0 bbbb 2026-09-27T11:00:00Z".parse().unwrap()),
    ///     server: None,
    /// };
    /// let text = m.to_string();
    /// assert!(text.starts_with("this riff (0.1.0 bbbb 2026-09-27T11:00:00Z) and its riff-server \
    ///     (a build from before the check) do not match."), "{text}");
    /// assert!(text.contains("Update riff-server"));
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = |b: &Option<Build>| match b {
            Some(b) => b.to_string(),
            None => "a build from before the check".into(),
        };
        write!(
            f,
            "this riff ({}) and its riff-server ({}) do not match. Messages are valid only \
             between the same builds. ",
            name(&self.riff),
            name(&self.server)
        )?;
        let step = match self.older() {
            Side::Riff => "Update riff on this machine, then start your sessions again.",
            Side::Server => "Update riff-server, on the machine of the riff.",
            Side::Both => "Update riff and riff-server from the same commit.",
        };
        write!(f, "{step} See {UPDATE_URL}")
    }
}

impl std::error::Error for Mismatch {}
