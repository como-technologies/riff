//! The compile cache of a machine: one `sccache` for all its workers.
//!
//! # Design
//!
//! Each item gets a fresh worktree, so each worker builds each
//! dependency of `Cargo.lock` again, and writes it to the `target` of
//! its worktree. The dependencies change seldom. So the workers of a
//! machine share one compile cache: riff starts each worker with
//! `RUSTC_WRAPPER` set to `sccache` (01M492379BGA3AERT1AM12C650). A
//! second worktree then reads each dependency from the cache. A shared
//! `CARGO_TARGET_DIR` is not the way: cargo locks it, so the builds of
//! the workers would wait for each other.
//!
//! | Variable | Value |
//! |---|---|
//! | `RUSTC_WRAPPER` | the `sccache` of the machine ([`find`]) |
//! | `SCCACHE_DIR` | `$RIFF_HOME/sccache`, else `$XDG_CACHE_HOME/riff/sccache`, else `~/.cache/riff/sccache` ([`dir_from`], 01M49237BM12PVBERD6JXDSX5V) |
//! | `SCCACHE_CACHE_SIZE` | `workers.cache`, default [`SIZE`] |
//! | `SCCACHE_SERVER_PORT` | a port from the folder ([`port`], 01M49237DWTBSEM6CVFH2BYC4V) |
//! | `SCCACHE_IGNORE_SERVER_IO_ERROR` | `1`: when the server ends, a compile goes on with no cache |
//!
//! The server of the cache is of the machine, not of a worker
//! (01M49AB2QYGJ73Y19KGAY1WDW7, 01M49AB2TBMHGNXM3GE4NDFYYG).
//! `riff workers run` starts it before `claude`, with no variable of a
//! worker and in its own process group ([`Cache::start_server`]). So
//! the clear, the reap and the stop of the worker that built first do
//! not see it. A server that a build starts again, for example after
//! its idle time, has [`SERVER_MARK`]`=1` in its environment, and
//! [`crate::workload::is_cache_server`] never counts it as a process
//! of a worker. Another program with the mark stays of its worker.
//!
//! The cache is on the machine only: no cloud store, no network. The
//! port comes from the folder, so the workers of a machine share one
//! `sccache` server, and that server never mixes with an `sccache` of
//! the person or of a test. `sccache` takes its jobs from the pool of
//! cargo (see [`crate::jobserver`]).
//!
//! riff installs `sccache` itself (01M4923963S666V9YWTZ46ZZ50): the
//! start of `riff workers host`, and `riff update` on a machine with a
//! limit of workers, run `cargo install --locked sccache --version`
//! [`PINNED`] when `sccache` is missing or older ([`ensure`]). When the
//! install fails, the workers build with no cache, as before.
//!
//! ```mermaid
//! flowchart TD
//!     S["riff workers host, riff update"] --> F{"sccache PINNED or newer?"}
//!     F -- yes --> W
//!     F -- no --> I["cargo install --locked sccache --version PINNED"]
//!     I -- ok --> W["riff workers run: RUSTC_WRAPPER, SCCACHE_DIR,<br/>SCCACHE_CACHE_SIZE, SCCACHE_SERVER_PORT"]
//!     I -- fails --> N["one note to the lead:<br/>the workers build with no cache"]
//!     W --> C["each build of each worker<br/>reads and writes one cache"]
//! ```
//!
//! `riff workers` shows the size of the cache and its hit rate
//! ([`Stats`], 01M492398HA0AXX0J8BZCKNGTG). `riff workers cache` shows
//! and sets the size, and `riff workers cache --clear` empties it
//! (01M49239AWKEPKEPMZZRRTVRAT).
//!
//! ```
//! use riff::sccache::{env, Cache};
//!
//! let cache = Cache { bin: "/c/bin/sccache".into(), dir: "/h/sccache".into(), size: "40G".into() };
//! let vars = env(Some(&cache));
//! assert_eq!(vars[0], ("RUSTC_WRAPPER".to_owned(), Some("/c/bin/sccache".to_owned())));
//! assert_eq!(vars[1], ("SCCACHE_DIR".to_owned(), Some("/h/sccache".to_owned())));
//! assert_eq!(vars[2], ("SCCACHE_CACHE_SIZE".to_owned(), Some("40G".to_owned())));
//! assert_eq!(vars[4], ("SCCACHE_IGNORE_SERVER_IO_ERROR".to_owned(), Some("1".to_owned())));
//! assert_eq!(env(None).iter().filter(|(_, value)| value.is_some()).count(), 0);
//! ```

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The version of `sccache` that riff installs.
pub const PINNED: &str = "0.18.0";

/// The default most size of the cache: `workers.cache`.
pub const SIZE: &str = "40G";

/// The variables that riff gives a worker for the cache.
pub const VARS: [&str; 5] = [
    "RUSTC_WRAPPER",
    "SCCACHE_DIR",
    "SCCACHE_CACHE_SIZE",
    "SCCACHE_SERVER_PORT",
    "SCCACHE_IGNORE_SERVER_IO_ERROR",
];

/// The variable that `sccache` gives the server that it starts
/// (01M49AB2TBMHGNXM3GE4NDFYYG).
pub const SERVER_MARK: &str = "SCCACHE_START_SERVER";

/// The cache of this machine: the `sccache` binary, its folder and its
/// most size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cache {
    /// The `sccache` binary.
    pub bin: PathBuf,
    /// The folder of the cache: `SCCACHE_DIR`.
    pub dir: PathBuf,
    /// The most size: `SCCACHE_CACHE_SIZE`, for example `40G`.
    pub size: String,
}

impl Cache {
    /// The cache of this machine with the settings `path`, or `None`
    /// when the machine has no `sccache`.
    pub fn here(path: &Path) -> Result<Option<Self>> {
        let Some(bin) = find() else {
            return Ok(None);
        };
        Ok(Some(Cache {
            bin,
            dir: dir()?,
            size: crate::settings::workers_cache(path)?,
        }))
    }

    /// `sccache ARGS` with the variables of the cache, so that it talks
    /// to the server of the workers.
    pub fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.bin);
        cmd.args(args);
        for (var, value) in env(Some(self)) {
            if let Some(value) = value {
                cmd.env(var, value);
            }
        }
        cmd
    }

    /// Starts the server of the cache when it does not run, with no
    /// variable of a worker in `worker_vars`, and in its own process
    /// group (01M49AB2QYGJ73Y19KGAY1WDW7). A server that runs makes the
    /// start fail: no matter.
    pub fn start_server(&self, worker_vars: &[&str]) {
        use std::os::unix::process::CommandExt;
        let mut cmd = self.command(&["--start-server"]);
        for var in worker_vars {
            cmd.env_remove(var);
        }
        let _ = cmd
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    /// The numbers of the server of the cache, or `None` when `sccache`
    /// gives none. It starts the server when it does not run.
    pub fn stats(&self) -> Option<Stats> {
        let out = self
            .command(&["--show-stats", "--stats-format", "json"])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| Stats::parse(&String::from_utf8_lossy(&out.stdout)))
            .flatten()
    }

    /// Stops the server of the cache and deletes the folder of the
    /// cache (01M49239AWKEPKEPMZZRRTVRAT). The next build starts a new
    /// server with an empty cache.
    pub fn clear(&self) -> Result<()> {
        // A server that does not run makes the stop fail: no matter.
        let _ = self
            .command(&["--stop-server"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(e).with_context(|| format!("cannot delete {}", self.dir.display()))
            }
            _ => Ok(()),
        }
    }
}

/// The variables of a worker for `cache`: the value to set, or `None`
/// to unset. With no cache, riff unsets each of [`VARS`]
/// (01M492379BGA3AERT1AM12C650).
pub fn env(cache: Option<&Cache>) -> Vec<(String, Option<String>)> {
    let values = cache.map(|cache| {
        [
            cache.bin.display().to_string(),
            cache.dir.display().to_string(),
            cache.size.clone(),
            port(&cache.dir).to_string(),
            "1".to_owned(),
        ]
    });
    VARS.iter()
        .enumerate()
        .map(|(i, var)| ((*var).to_owned(), values.as_ref().map(|v| v[i].clone())))
        .collect()
}

/// The folder of the cache from `RIFF_HOME`, `XDG_CACHE_HOME` and
/// `HOME` (01M49237BM12PVBERD6JXDSX5V).
///
/// ```
/// use riff::sccache::dir_from;
/// assert_eq!(dir_from(Some("/r".into()), Some("/c".into()), None), Some("/r/sccache".into()));
/// assert_eq!(dir_from(None, Some("/c".into()), Some("/h".into())), Some("/c/riff/sccache".into()));
/// assert_eq!(dir_from(None, Some("".into()), Some("/h".into())), Some("/h/.cache/riff/sccache".into()));
/// assert_eq!(dir_from(None, None, None), None);
/// ```
pub fn dir_from(
    home: Option<std::ffi::OsString>,
    cache: Option<std::ffi::OsString>,
    user: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let set = |v: Option<std::ffi::OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(home) = set(home) {
        return Some(home.join("sccache"));
    }
    set(cache)
        .or_else(|| set(user).map(|h| h.join(".cache")))
        .map(|c| c.join("riff").join("sccache"))
}

/// The folder of the cache of this process: [`dir_from`].
pub fn dir() -> Result<PathBuf> {
    dir_from(
        std::env::var_os(crate::home::VAR),
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
    )
    .context("cannot find the folder of the compile cache: set XDG_CACHE_HOME or HOME")
}

/// The port of the `sccache` server of the folder `dir`: 20000 to
/// 29999, the same for the same folder (01M49237DWTBSEM6CVFH2BYC4V).
///
/// ```
/// use riff::sccache::port;
/// let one = port("/h/.cache/riff/sccache".as_ref());
/// assert_eq!(one, port("/h/.cache/riff/sccache".as_ref()));
/// assert_ne!(one, port("/tmp/test/sccache".as_ref()));
/// assert!((20000..30000).contains(&one), "{one}");
/// ```
pub fn port(dir: &Path) -> u16 {
    use std::os::unix::ffi::OsStrExt;
    // FNV-1a: the same number in each build of riff.
    let hash = dir
        .as_os_str()
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
        });
    20000 + u16::try_from(hash % 10000).unwrap_or(0)
}

/// The `sccache` of this machine: the first on `PATH`, else in
/// `~/.cargo/bin`, where `cargo install` puts it.
pub fn find() -> Option<PathBuf> {
    let cargo = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(|h| PathBuf::from(h).join(".cargo").join("bin"));
    find_in(std::env::var_os("PATH").as_deref(), cargo.as_deref())
}

/// The first file `sccache` that can run in the folders of `path`, else
/// in `extra`.
///
/// ```
/// use std::os::unix::fs::PermissionsExt;
///
/// let (a, b) = (tempfile::tempdir()?, tempfile::tempdir()?);
/// let bin = b.path().join("sccache");
/// std::fs::write(&bin, "#!/bin/sh\n")?;
/// let path = std::env::join_paths([a.path(), b.path()])?;
/// assert_eq!(riff::sccache::find_in(Some(&path), None), None, "it cannot run");
/// std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))?;
/// assert_eq!(riff::sccache::find_in(Some(&path), None), Some(bin.clone()));
/// assert_eq!(riff::sccache::find_in(None, Some(b.path())), Some(bin));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn find_in(path: Option<&OsStr>, extra: Option<&Path>) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let dirs = path.map(std::env::split_paths).into_iter().flatten();
    dirs.chain(extra.map(Path::to_owned))
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join("sccache"))
        .find(|bin| {
            std::fs::metadata(bin).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

/// The version in the output of `sccache --version`.
///
/// ```
/// use riff::sccache::version_of;
/// assert_eq!(version_of("sccache 0.18.0\n"), Some((0, 18, 0)));
/// assert_eq!(version_of("sccache 0.9.1-dev"), Some((0, 9, 1)));
/// assert_eq!(version_of("0.18.0"), Some((0, 18, 0)));
/// assert_eq!(version_of("command not found"), None);
/// ```
pub fn version_of(out: &str) -> Option<(u32, u32, u32)> {
    let word = out
        .split_whitespace()
        .find(|w| w.starts_with(|c: char| c.is_ascii_digit()))?;
    let mut parts = word.split(['.', '-', '+']).map(str::parse::<u32>);
    Some((
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ))
}

/// The version of the `sccache` binary `bin`, or `None` when it gives
/// none.
pub fn version(bin: &Path) -> Option<(u32, u32, u32)> {
    let out = Command::new(bin).arg("--version").output().ok()?;
    version_of(&String::from_utf8_lossy(&out.stdout))
}

/// The arguments of `cargo` that install the pinned `sccache`.
///
/// ```
/// assert_eq!(riff::sccache::install_args(), ["install", "--locked", "sccache", "--version", "0.18.0"]);
/// ```
pub fn install_args() -> [&'static str; 5] {
    ["install", "--locked", "sccache", "--version", PINNED]
}

/// What [`ensure`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Install {
    /// The machine has [`PINNED`] or newer: riff ran nothing.
    Present,
    /// riff installed [`PINNED`].
    Installed,
}

/// Installs [`PINNED`] with `cargo` when this machine has no `sccache`,
/// or an older one (01M4923963S666V9YWTZ46ZZ50). An error says why the
/// install failed.
pub fn ensure(cargo: &Path) -> Result<Install> {
    let pinned = version_of(PINNED).context("the pinned version of sccache")?;
    if find()
        .and_then(|bin| version(&bin))
        .is_some_and(|have| have >= pinned)
    {
        return Ok(Install::Present);
    }
    println!("{}", crate::text::sccache_installs(PINNED));
    let status = Command::new(cargo)
        .args(install_args())
        .current_dir(crate::lifecycle::run_dir())
        .stdin(std::process::Stdio::null())
        .status()
        .with_context(|| format!("cannot run {}", cargo.display()))?;
    if !status.success() {
        bail!("cargo install {status}");
    }
    if find().is_none() {
        bail!("cargo installed sccache, but it is not on PATH or in ~/.cargo/bin");
    }
    Ok(Install::Installed)
}

/// The numbers of the cache from `sccache --show-stats --stats-format
/// json` (01M492398HA0AXX0J8BZCKNGTG).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// The bytes in the cache.
    pub bytes: u64,
    /// The most bytes of the cache, when `sccache` tells it.
    pub max: Option<u64>,
    /// The compiles that the cache gave.
    pub hits: u64,
    /// The compiles that the cache did not have.
    pub misses: u64,
}

impl Stats {
    /// Reads the JSON of `sccache --show-stats --stats-format json`.
    ///
    /// ```
    /// use riff::sccache::Stats;
    ///
    /// let json = r#"{"stats": {"cache_hits": {"counts": {"Rust": 85, "C/C++": 5}},
    ///     "cache_misses": {"counts": {"Rust": 10}}},
    ///     "cache_size": 3221225472, "max_cache_size": 42949672960}"#;
    /// let stats = Stats::parse(json).unwrap();
    /// assert_eq!(stats, Stats { bytes: 3 << 30, max: Some(40 << 30), hits: 90, misses: 10 });
    /// assert_eq!(stats.rate(), Some(90));
    /// assert_eq!(Stats::parse("{}").unwrap().rate(), None);
    /// assert_eq!(Stats::parse("no json"), None);
    /// ```
    pub fn parse(json: &str) -> Option<Self> {
        let info: serde_json::Value = serde_json::from_str(json).ok()?;
        let count = |key: &str| {
            info.pointer(&format!("/stats/{key}/counts"))
                .and_then(serde_json::Value::as_object)
                .map_or(0, |counts| {
                    counts.values().filter_map(serde_json::Value::as_u64).sum()
                })
        };
        Some(Stats {
            bytes: info
                .get("cache_size")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            max: info
                .get("max_cache_size")
                .and_then(serde_json::Value::as_u64),
            hits: count("cache_hits"),
            misses: count("cache_misses"),
        })
    }

    /// The hits in percent of the hits and the misses, or `None` with
    /// no compile yet.
    pub fn rate(&self) -> Option<u64> {
        let all = self.hits + self.misses;
        (all > 0).then(|| self.hits * 100 / all)
    }
}

/// True when `size` is a size that `sccache` reads: a number with an
/// optional `K`, `M`, `G` or `T`.
///
/// ```
/// use riff::sccache::valid_size;
/// assert!(valid_size("40G") && valid_size("500M") && valid_size("1T") && valid_size("1000"));
/// assert!(!valid_size("") && !valid_size("G") && !valid_size("40 G") && !valid_size("40GB"));
/// ```
pub fn valid_size(size: &str) -> bool {
    let digits = size.strip_suffix(['K', 'M', 'G', 'T']).unwrap_or(size);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}
