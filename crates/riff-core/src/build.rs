//! The build of `riff` and `riff-server`, their wire version, and the
//! check that they can talk.
//!
//! # Design
//!
//! A build is the crate version, the last commit that changed the code
//! (`crates`, `Cargo.toml` or `Cargo.lock`), the UTC time of that
//! commit, and the wire version [`WIRE`]. `build.rs` finds the commit.
//! The image build of `riff-server` has no repository: it gives
//! `RIFF_COMMIT` and `RIFF_COMMIT_TIME`, from `deploy/build-id.sh`,
//! which runs the same command.
//!
//! A message is valid only between the same wire version
//! (01M3MNVT7G701SDP1Z1THMRDQ2). A change to a wire type, to the API or
//! to the header bumps [`WIRE`]. The test `tests/wire.rs` keeps the JSON
//! schema of the wire types in `wire.json`, and fails when the schema
//! changes and [`WIRE`] stays the same. Most merges change no wire type,
//! so the builds differ and the wire matches: `riff` goes on, and tells
//! the person once to update ([`other_build`]).
//!
//! Each call of `riff` carries its build in the header [`HEADER`], and
//! each reply of `riff-server` carries the build of the server
//! (01M3JEE7P46GWXR1BD4Q1TTSGN). Each side refuses another wire version,
//! or no build:
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff
//!     participant S as riff-server
//!     R->>S: call, riff-build: 0.1.0 929605821e54 2026-09-27T22:03:01Z wire 1
//!     alt the same wire version
//!         S-->>R: reply, riff-build: the build of the server
//!         Note over R: another build: one note to update, then go on
//!     else another wire version, or no build
//!         S-->>R: 409, riff-build: the build of the server, the mismatch
//!     end
//!     Note over R: a reply with no build, or another wire version,<br/>is a Mismatch too (an old riff-server)
//! ```

use std::fmt;

/// The HTTP header that carries a [`Build`].
pub const HEADER: &str = "riff-build";

/// The wire version as a literal, so that [`VERSION`] can hold it.
macro_rules! wire {
    () => {
        1
    };
}

/// The wire version of this build (01M3MNVT7G701SDP1Z1THMRDQ2). Bump it
/// with each change to a message, the API or the header.
pub const WIRE: u32 = wire!();

/// The build of this binary as text, for `--version` and [`HEADER`]
/// (01M3JEE7WT04BKX377VW5GDSPY).
///
/// ```
/// use riff_core::build::{Build, VERSION, WIRE};
///
/// assert_eq!(VERSION, Build::this().to_string());
/// assert!(VERSION.ends_with(&format!(" wire {WIRE}")));
/// ```
pub const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " ",
    env!("RIFF_BUILD_COMMIT"),
    " ",
    env!("RIFF_BUILD_TIME"),
    " wire ",
    wire!()
);

/// The part of the book that tells how to update each side.
pub const UPDATE_URL: &str =
    "https://como-technologies.github.io/riff/how-it-works.html#when-the-wire-does-not-match";

/// One build of riff: the crate version, the commit, its UTC time and
/// the wire version.
///
/// ```
/// use riff_core::build::Build;
///
/// let b: Build = "0.1.0 929605821e54 2026-09-27T22:03:01Z wire 3".parse().unwrap();
/// assert_eq!(b.commit, "929605821e54");
/// assert_eq!(b.wire, 3);
/// assert_eq!(b.to_string(), "0.1.0 929605821e54 2026-09-27T22:03:01Z wire 3");
/// // A build from before the wire version is not a build.
/// assert!("0.1.0 929605821e54 2026-09-27T22:03:01Z".parse::<Build>().is_err());
/// assert!("0.1.0 929605821e54 2026-09-27T22:03:01Z wire x".parse::<Build>().is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    /// The crate version.
    pub version: String,
    /// The last commit that changed the code.
    pub commit: String,
    /// The time of that commit, in UTC.
    pub time: String,
    /// The wire version.
    pub wire: u32,
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
            wire: WIRE,
        }
    }

    /// The build that a reply or a request names in [`HEADER`]. `None`
    /// when it names none, or names it in another form.
    pub fn from_header(value: Option<&[u8]>) -> Option<Build> {
        std::str::from_utf8(value?).ok()?.parse().ok()
    }

    /// True when the two are the same build: the same version, commit
    /// and wire version.
    ///
    /// ```
    /// use riff_core::build::Build;
    ///
    /// let a: Build = "0.1.0 aaaa 2026-09-27T10:00:00Z wire 1".parse().unwrap();
    /// let b: Build = "0.1.0 bbbb 2026-09-27T11:00:00Z wire 1".parse().unwrap();
    /// assert!(a.matches(&a.clone()));
    /// assert!(!a.matches(&b));
    /// ```
    pub fn matches(&self, other: &Build) -> bool {
        self.version == other.version && self.commit == other.commit && self.wire == other.wire
    }

    /// True when a message is valid between the two builds: the same
    /// wire version (01M3MNVT7G701SDP1Z1THMRDQ2).
    ///
    /// ```
    /// use riff_core::build::Build;
    ///
    /// let a: Build = "0.1.0 aaaa 2026-09-27T10:00:00Z wire 1".parse().unwrap();
    /// let b: Build = "0.1.0 bbbb 2026-09-27T11:00:00Z wire 1".parse().unwrap();
    /// let c: Build = "0.1.0 cccc 2026-09-27T12:00:00Z wire 2".parse().unwrap();
    /// assert!(a.talks_with(&b));
    /// assert!(!a.talks_with(&c));
    /// ```
    pub fn talks_with(&self, other: &Build) -> bool {
        self.wire == other.wire
    }
}

impl fmt::Display for Build {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} wire {}",
            self.version, self.commit, self.time, self.wire
        )
    }
}

impl std::str::FromStr for Build {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        match parts[..] {
            [version, commit, time, "wire", wire] => Ok(Build {
                version: version.into(),
                commit: commit.into(),
                time: time.into(),
                wire: wire
                    .parse()
                    .map_err(|_| format!("{s:?} has no wire version"))?,
            }),
            _ => Err(format!("{s:?} is not a riff build")),
        }
    }
}

/// `riff` and its `riff-server` have other wire versions
/// (01M3JEE7RDTDD3KQMKH41E8D57). `None` is a side that sent no build: a
/// build from before the wire version.
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
    /// let old: Build = "0.1.0 aaaa 2026-09-27T10:00:00Z wire 1".parse().unwrap();
    /// let new: Build = "0.1.0 bbbb 2026-09-27T11:00:00Z wire 2".parse().unwrap();
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
    ///     riff: Some("0.1.0 bbbb 2026-09-27T11:00:00Z wire 2".parse().unwrap()),
    ///     server: None,
    /// };
    /// let text = m.to_string();
    /// assert!(text.starts_with("this riff (0.1.0 bbbb 2026-09-27T11:00:00Z wire 2) and its \
    ///     riff-server (a build from before the wire version) do not match."), "{text}");
    /// assert!(text.contains("Update riff-server"));
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = |b: &Option<Build>| match b {
            Some(b) => b.to_string(),
            None => "a build from before the wire version".into(),
        };
        write!(
            f,
            "this riff ({}) and its riff-server ({}) do not match. Messages are valid only \
             between the same wire version. ",
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

/// The note when the builds differ and the wire matches
/// (01M3MNVT9TYNXZ8V845BHKQADV). `riff` prints it once for each process,
/// and goes on.
///
/// ```
/// use riff_core::build::{Build, other_build};
///
/// let server: Build = "0.1.0 aaaa 2026-09-27T10:00:00Z wire 1".parse().unwrap();
/// let riff: Build = "0.1.0 bbbb 2026-09-27T11:00:00Z wire 1".parse().unwrap();
/// assert_eq!(
///     other_build(&riff, &server),
///     "riff-server runs build 0.1.0 aaaa 2026-09-27T10:00:00Z wire 1; this riff runs build \
///      0.1.0 bbbb 2026-09-27T11:00:00Z wire 1. Run riff update when you can."
/// );
/// ```
pub fn other_build(riff: &Build, server: &Build) -> String {
    format!(
        "riff-server runs build {server}; this riff runs build {riff}. Run riff update when \
         you can."
    )
}
