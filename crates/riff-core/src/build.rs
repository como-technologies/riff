//! The build of `riff` and `riff-server`, and the check that they can
//! talk.
//!
//! # Design
//!
//! A build is the crate version, the last commit that changed the code
//! (`crates`, `Cargo.toml` or `Cargo.lock`), and the UTC time of that
//! commit. `build.rs` finds the commit. The image build of
//! `riff-server` has no repository: it gives `RIFF_COMMIT` and
//! `RIFF_COMMIT_TIME`, from `deploy/build-id.sh`, which runs the same
//! command.
//!
//! The crate version is a semantic version, and it says what a build
//! can talk to (01M3MX1DYY6AVDW946NR0B9T2C). The line of a version is
//! its major, and its minor while the major is 0: `0.4.1` is on the
//! line `0.4`, `1.2.0` on the line `1`. A change to a wire type, to the
//! API or to the header starts a new line (01M3MX1E3R5WESVHA8RZXFQR1J).
//! Most merges change no wire type, so the builds differ and the line
//! matches: `riff` goes on, and tells the person once to update
//! ([`other_build`]).
//!
//! `riff-server` also takes a `riff` of the line before its own
//! (01M3MX1E1EY1M7JGNCN6FCEVQK), so a person has one release to update
//! in. [`compatible`] holds the rule.
//!
//! Each call of `riff` carries its build in the header [`HEADER`], and
//! each reply of `riff-server` carries the build of the server
//! (01M3JEE7P46GWXR1BD4Q1TTSGN). Each side refuses a version that it
//! cannot talk to, or no build (01M3MX1E65XGWDZ062PQ9YXQ5T):
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff
//!     participant S as riff-server
//!     R->>S: call, riff-build: 0.3.2 929605821e54 2026-09-27T22:03:01Z
//!     alt the line of the server, or the line before
//!         S-->>R: reply, riff-build: 0.4.0 7213825ab1c2 2026-09-28T20:10:44Z
//!         Note over R: another build: one note to update, then go on
//!     else another line, or no build
//!         S-->>R: 409, riff-build: the build of the server, the mismatch
//!     end
//!     Note over R: a reply with no build, or of a line that riff<br/>cannot talk to, is a Mismatch too
//! ```

use std::fmt;

/// The HTTP header that carries a [`Build`].
pub const HEADER: &str = "riff-build";

/// The build of this binary as text, for `--version` and [`HEADER`]
/// (01M3JEE7WT04BKX377VW5GDSPY).
///
/// ```
/// use riff_core::build::{Build, VERSION};
///
/// assert_eq!(VERSION, Build::this().to_string());
/// assert!(VERSION.starts_with(env!("CARGO_PKG_VERSION")));
/// ```
pub const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " ",
    env!("RIFF_BUILD_COMMIT"),
    " ",
    env!("RIFF_BUILD_TIME"),
);

/// The part of the book that tells how to update each side.
pub const UPDATE_URL: &str =
    "https://como-technologies.github.io/riff/how-it-works.html#when-the-versions-do-not-match";

/// A semantic version: major, minor and patch. A pre-release or build
/// part after `-` or `+` is not part of the order.
///
/// ```
/// use riff_core::build::Semver;
///
/// let v: Semver = "0.4.1".parse().unwrap();
/// assert_eq!(v, Semver { major: 0, minor: 4, patch: 1 });
/// assert_eq!(v.line(), "0.4");
/// assert_eq!("1.2.0-rc1".parse::<Semver>().unwrap().line(), "1");
/// assert!("0.4".parse::<Semver>().is_err());
/// assert!("v0.4.1".parse::<Semver>().is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Semver {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Semver {
    /// The line of the version: the major, and the minor while the
    /// major is 0. Builds of one line talk.
    fn key(self) -> (u64, u64) {
        if self.major == 0 {
            (0, self.minor)
        } else {
            (self.major, 0)
        }
    }

    /// The line as text: `0.4` or `1`.
    pub fn line(self) -> String {
        match self.key() {
            (0, minor) => format!("0.{minor}"),
            (major, _) => major.to_string(),
        }
    }

    /// The first version of the line before this one. `None` for the
    /// line `0.0`, and for the line `1`: the last line of 0.x is not
    /// known.
    ///
    /// ```
    /// use riff_core::build::Semver;
    ///
    /// let v = |s: &str| s.parse::<Semver>().unwrap();
    /// assert_eq!(v("0.4.2").line_before(), Some(v("0.3.0")));
    /// assert_eq!(v("2.1.0").line_before(), Some(v("1.0.0")));
    /// assert_eq!(v("1.0.0").line_before(), None);
    /// assert_eq!(v("0.0.9").line_before(), None);
    /// ```
    pub fn line_before(self) -> Option<Semver> {
        match self.key() {
            (0, 0) | (1, _) => None,
            (0, minor) => Some(Semver {
                major: 0,
                minor: minor - 1,
                patch: 0,
            }),
            (major, _) => Some(Semver {
                major: major - 1,
                minor: 0,
                patch: 0,
            }),
        }
    }

    /// The first version of the line after this one.
    ///
    /// ```
    /// use riff_core::build::Semver;
    ///
    /// let v = |s: &str| s.parse::<Semver>().unwrap();
    /// assert_eq!(v("0.4.2").line_after(), v("0.5.0"));
    /// assert_eq!(v("1.3.2").line_after(), v("2.0.0"));
    /// ```
    pub fn line_after(self) -> Semver {
        match self.key() {
            (0, minor) => Semver {
                major: 0,
                minor: minor + 1,
                patch: 0,
            },
            (major, _) => Semver {
                major: major + 1,
                minor: 0,
                patch: 0,
            },
        }
    }

    /// True when the two versions are on the same line.
    ///
    /// ```
    /// use riff_core::build::Semver;
    ///
    /// let v = |s: &str| s.parse::<Semver>().unwrap();
    /// assert!(v("0.4.0").same_line(v("0.4.3")));
    /// assert!(!v("0.4.0").same_line(v("0.5.0")));
    /// assert!(v("1.2.0").same_line(v("1.9.4")));
    /// ```
    pub fn same_line(self, other: Semver) -> bool {
        self.key() == other.key()
    }
}

impl fmt::Display for Semver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl std::str::FromStr for Semver {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let core = s.split(['-', '+']).next().unwrap_or_default();
        let parts: Vec<u64> = core
            .split('.')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| format!("{s:?} is not a semantic version"))?;
        match parts[..] {
            [major, minor, patch] => Ok(Semver {
                major,
                minor,
                patch,
            }),
            _ => Err(format!("{s:?} is not a semantic version")),
        }
    }
}

/// One build of riff: the crate version, the commit and its UTC time.
///
/// ```
/// use riff_core::build::Build;
///
/// let b: Build = "0.4.1 929605821e54 2026-09-27T22:03:01Z".parse().unwrap();
/// assert_eq!(b.commit, "929605821e54");
/// assert_eq!(b.semver().unwrap().minor, 4);
/// assert_eq!(b.to_string(), "0.4.1 929605821e54 2026-09-27T22:03:01Z");
/// // A build of an older form, or with no semantic version, is not a build.
/// assert!("0.2.0 929605821e54 2026-09-27T22:03:01Z wire 1".parse::<Build>().is_err());
/// assert!("next 929605821e54 2026-09-27T22:03:01Z".parse::<Build>().is_err());
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

    /// The semantic version of the build. `None` when the crate version
    /// is not one.
    pub fn semver(&self) -> Option<Semver> {
        self.version.parse().ok()
    }

    /// True when the two are the same build: the same version and
    /// commit.
    ///
    /// ```
    /// use riff_core::build::Build;
    ///
    /// let a: Build = "0.4.0 aaaa 2026-09-27T10:00:00Z".parse().unwrap();
    /// let b: Build = "0.4.0 bbbb 2026-09-27T11:00:00Z".parse().unwrap();
    /// assert!(a.matches(&a.clone()));
    /// assert!(!a.matches(&b));
    /// ```
    pub fn matches(&self, other: &Build) -> bool {
        self.version == other.version && self.commit == other.commit
    }
}

/// True when `riff` and `riff-server` can talk: the two versions are on
/// the same line (01M3MX1DYY6AVDW946NR0B9T2C), or `riff` is on the line
/// before the line of the server (01M3MX1E1EY1M7JGNCN6FCEVQK).
///
/// ```
/// use riff_core::build::{Build, compatible};
///
/// let b = |v: &str| Build { version: v.into(), ..Build::this() };
/// assert!(compatible(&b("0.4.0"), &b("0.4.3")));
/// assert!(compatible(&b("0.4.3"), &b("0.4.0")));
/// assert!(compatible(&b("0.3.2"), &b("0.4.0")));
/// assert!(!compatible(&b("0.4.0"), &b("0.3.2")));
/// assert!(!compatible(&b("0.2.0"), &b("0.4.0")));
/// assert!(compatible(&b("1.0.0"), &b("1.3.0")));
/// assert!(!compatible(&b("next"), &b("0.4.0")));
/// ```
pub fn compatible(riff: &Build, server: &Build) -> bool {
    let (Some(r), Some(s)) = (riff.semver(), server.semver()) else {
        return false;
    };
    r.same_line(s) || s.line_before().is_some_and(|before| r.same_line(before))
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
            [version, commit, time] => {
                version.parse::<Semver>()?;
                Ok(Build {
                    version: version.into(),
                    commit: commit.into(),
                    time: time.into(),
                })
            }
            _ => Err(format!("{s:?} is not a riff build")),
        }
    }
}

/// `riff` and its `riff-server` cannot talk
/// (01M3MX1E65XGWDZ062PQ9YXQ5T). `None` is a side that sent no build,
/// or a build of an older form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    /// The build of `riff`.
    pub riff: Option<Build>,
    /// The build of `riff-server`.
    pub server: Option<Build>,
}

impl Mismatch {
    /// The side to update: the older version. A side with no build is
    /// older.
    ///
    /// ```
    /// use riff_core::build::{Build, Mismatch, Side};
    ///
    /// let b = |v: &str| Some(Build { version: v.into(), ..Build::this() });
    /// let m = |riff, server| Mismatch { riff, server }.older();
    /// assert_eq!(m(b("0.2.0"), b("0.4.0")), Side::Riff);
    /// assert_eq!(m(b("0.4.0"), b("0.3.2")), Side::Server);
    /// assert_eq!(m(b("0.4.0"), None), Side::Server);
    /// assert_eq!(m(None, b("0.4.0")), Side::Riff);
    /// ```
    pub fn older(&self) -> Side {
        let version = |b: &Option<Build>| b.as_ref().and_then(Build::semver);
        match (version(&self.riff), version(&self.server)) {
            (Some(r), Some(s)) if r > s => Side::Server,
            (Some(_), None) => Side::Server,
            _ => Side::Riff,
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
}

impl fmt::Display for Mismatch {
    /// ```
    /// use riff_core::build::Mismatch;
    ///
    /// let m = Mismatch {
    ///     riff: Some("0.2.0 bbbb 2026-09-27T11:00:00Z".parse().unwrap()),
    ///     server: Some("0.4.0 aaaa 2026-09-28T11:00:00Z".parse().unwrap()),
    /// };
    /// assert_eq!(
    ///     m.to_string(),
    ///     "this riff (0.2.0 bbbb 2026-09-27T11:00:00Z) and its riff-server (0.4.0 aaaa \
    ///      2026-09-28T11:00:00Z) do not match. riff-server 0.4 talks only with riff 0.4 and \
    ///      0.3. Update riff on this machine, then start your sessions again. See \
    ///      https://como-technologies.github.io/riff/how-it-works.html#when-the-versions-do-not-match"
    /// );
    /// let old = Mismatch { riff: m.riff.clone(), server: None };
    /// assert!(old.to_string().contains("riff-server (an older build)"), "{old}");
    /// assert!(old.to_string().contains("Update riff-server"), "{old}");
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = |b: &Option<Build>| match b {
            Some(b) => b.to_string(),
            None => "an older build".into(),
        };
        write!(
            f,
            "this riff ({}) and its riff-server ({}) do not match. ",
            name(&self.riff),
            name(&self.server)
        )?;
        if let Some(s) = self.server.as_ref().and_then(Build::semver) {
            let line = s.line();
            match s.line_before() {
                Some(before) => write!(
                    f,
                    "riff-server {line} talks only with riff {line} and {}. ",
                    before.line()
                )?,
                None => write!(f, "riff-server {line} talks only with riff {line}. ")?,
            }
        }
        let step = match self.older() {
            Side::Riff => "Update riff on this machine, then start your sessions again.",
            Side::Server => "Update riff-server, on the machine of the riff.",
        };
        write!(f, "{step} See {UPDATE_URL}")
    }
}

impl std::error::Error for Mismatch {}

/// The note when the builds differ and they can talk
/// (01M3MX1E8M9TKBN90P4DYKH3H8). `riff` prints it once for each process,
/// and goes on. When `riff` is on the line before the server, the note
/// says that the next line of the server refuses it.
///
/// ```
/// use riff_core::build::{Build, other_build};
///
/// let server: Build = "0.4.3 aaaa 2026-09-27T10:00:00Z".parse().unwrap();
/// let riff: Build = "0.4.0 bbbb 2026-09-27T11:00:00Z".parse().unwrap();
/// assert_eq!(
///     other_build(&riff, &server),
///     "riff-server runs build 0.4.3 aaaa 2026-09-27T10:00:00Z; this riff runs build \
///      0.4.0 bbbb 2026-09-27T11:00:00Z. Run riff update when you can."
/// );
/// let riff: Build = "0.3.2 cccc 2026-09-20T11:00:00Z".parse().unwrap();
/// assert_eq!(
///     other_build(&riff, &server),
///     "riff-server runs build 0.4.3 aaaa 2026-09-27T10:00:00Z; this riff runs build \
///      0.3.2 cccc 2026-09-20T11:00:00Z. riff-server 0.5 will refuse riff 0.3. Run riff \
///      update soon."
/// );
/// ```
pub fn other_build(riff: &Build, server: &Build) -> String {
    let head = format!("riff-server runs build {server}; this riff runs build {riff}.");
    match (riff.semver(), server.semver()) {
        (Some(r), Some(s)) if !r.same_line(s) => format!(
            "{head} riff-server {} will refuse riff {}. Run riff update soon.",
            s.line_after().line(),
            r.line()
        ),
        _ => format!("{head} Run riff update when you can."),
    }
}
