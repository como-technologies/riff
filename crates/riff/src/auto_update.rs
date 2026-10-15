//! A machine that updates riff by itself.
//!
//! # Design
//!
//! The person turns it on for the machine with `riff update --auto on`:
//! the key `update.auto` in [`settings`]. It is off by default
//! (01M3N7JJC5WQBJ7SJZSZNBAVVR).
//!
//! Each reply of `riff-server` names its build. When a `riff` process
//! gets a reply from a server of a newer version than its own, and the
//! machine has `update.auto = true`, [`begin`] starts
//! `riff update --tag vX.Y.Z` of the release of that server in the
//! background (01M3N7JJEKZMN1E5NJQRK2QYVB). It never installs an older
//! release: [`wanted`] holds the rule. With `update.auto = false`, only
//! the note to update stays.
//!
//! Many processes of a machine see the new server at the same time. So
//! the update in the background ([`run`]) takes the update lock of the
//! machine, and records the release that it tries
//! (01M3N7JJH0SXXQYYBAHWPCNQGX). An update that finds the lock held, or
//! the release tried, stops at once. So one update runs for each
//! release, and a failed release waits for the next release. The files
//! are in [`local`].
//!
//! The update in the background runs in the local files of riff, a
//! directory that exists, also when the process that saw the new server
//! runs in a removed worktree. It gets the place of that process, which
//! [`remember`] keeps, so that the message to the lead finds the
//! repository (01M3NT2Q0RNM9PVHT42V459624). When its own working
//! directory is missing, it stops before `cargo` runs, and the release
//! does not count as tried (01M3NT2PYFHPB0C19Q2QB2AE6W). Each other
//! failure counts, so one release gets at most one install.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as riff process
//!     participant S as riff-server
//!     participant U as riff update in the background
//!     participant C as cargo
//!     participant L as lead
//!     P->>S: call
//!     S-->>P: reply, riff-build: 0.4.0 (newer)
//!     P->>U: start: riff update --tag v0.4.0
//!     U->>U: take the update lock, record v0.4.0
//!     U->>C: install --tag v0.4.0 riff riff-server
//!     U->>L: tell: the host, v0.3.0, v0.4.0, or the error
//! ```
//!
//! After the install, the new `riff` is on disk. `riff watch`,
//! `riff tail`, `riff top`, `riff chat` and `riff mcp` run it (see
//! [`binary`](crate::binary)). The update tells the lead of the
//! user, as the person, once: the host, the old release and the new
//! release, or the error (01M3N7JJKBME6VSNTHD8VPN3K9). `cargo install`
//! replaces the binaries only after a good build, so a failed update
//! keeps the old ones. The update stops no session, and changes no
//! sign-in and no device key (01M3N7JJNPPJNTXFDTEY4MSDVJ).

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use riff_core::build::Build;
use riff_core::name::Place;

use crate::{identity, lifecycle, local, settings, text};

/// True in the update in the background: it starts no other update.
static OFF: AtomicBool = AtomicBool::new(false);

/// The release that this process started an update for.
static STARTED: Mutex<Option<String>> = Mutex::new(None);

/// The place of this process, while its directory still existed.
static HERE: Mutex<Option<Place>> = Mutex::new(None);

/// Keeps `place`, the place of this process, for the update in the
/// background (01M3NT2Q0RNM9PVHT42V459624). `riff` calls it once it
/// knows the place, before its first call to the riff.
pub fn remember(place: &Place) {
    if let Ok(mut here) = HERE.lock() {
        *here = Some(place.clone());
    }
}

/// The place of this process: the one that [`remember`] keeps, else the
/// place of the working directory. `None` when neither is known.
fn here() -> Option<Place> {
    let kept = HERE.lock().ok().and_then(|here| here.clone());
    kept.or_else(|| identity::place(&identity::working_dir().ok()?).ok())
}

/// The release tag to install when `this` riff gets a reply from
/// `server`: the release of the server, when its version is newer.
/// `None` for the same version, an older one, or a build with no
/// version.
///
/// ```
/// use riff::auto_update::wanted;
/// use riff_core::build::Build;
///
/// let b = |v: &str| Build { version: v.into(), ..Build::this() };
/// assert_eq!(wanted(&b("0.3.0"), &b("0.4.0")).as_deref(), Some("v0.4.0"));
/// assert_eq!(wanted(&b("0.3.0"), &b("0.3.1")).as_deref(), Some("v0.3.1"));
/// assert_eq!(wanted(&b("0.2.0"), &b("0.4.0")).as_deref(), Some("v0.4.0"));
/// assert_eq!(wanted(&b("0.4.0"), &b("0.4.0")), None);
/// assert_eq!(wanted(&b("0.4.0"), &b("0.3.2")), None);
/// assert_eq!(wanted(&b("0.4.0"), &b("next")), None);
/// ```
pub fn wanted(this: &Build, server: &Build) -> Option<String> {
    let (Some(this), Some(new)) = (this.semver(), server.semver()) else {
        return None;
    };
    (new > this).then(|| lifecycle::release_tag(&server.version))
}

/// Starts the update in the background when the riff at `url` runs
/// `server`, a newer release than this riff, and this machine has
/// `update.auto = true` (01M3N7JJEKZMN1E5NJQRK2QYVB). It does nothing
/// when an update runs, when the release was tried, or when this
/// process started it already. The check of each reply calls it.
pub fn begin(url: &str, server: &Build) {
    let Some(tag) = wanted(&Build::this(), server) else {
        return;
    };
    if OFF.load(Ordering::Relaxed) {
        return;
    }
    let Ok(mut started) = STARTED.lock() else {
        return;
    };
    if started.as_deref() == Some(&tag) || !settings::path().is_ok_and(|p| on(&p)) {
        return;
    }
    let Some(dir) = local::dir() else {
        return;
    };
    if local::updating(&dir) || local::tried(&dir).as_deref() == Some(&tag) {
        *started = Some(tag);
        return;
    }
    match start(&dir, &tag, url) {
        Ok(()) => eprintln!("riff: {}", text::auto_update_started(&tag)),
        Err(e) => eprintln!("riff: cannot start the update of riff: {e:#}"),
    }
    *started = Some(tag);
}

/// True when the settings at `path` turn the update by itself on. A
/// bad file counts as off.
fn on(path: &Path) -> bool {
    settings::update_auto(path).unwrap_or(false)
}

/// Starts `riff update --background --tag TAG --server URL` with this
/// binary, in a process group of its own, so that the end of this
/// process or a Ctrl-C does not stop it. It runs in `dir`, with the
/// place of this process (01M3NT2Q0RNM9PVHT42V459624). Its output goes
/// to the log in `dir`.
fn start(dir: &Path, tag: &str, url: &str) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let path = local::update_log(dir);
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    let exe = crate::binary::this_on_disk().context("cannot find the riff binary")?;
    let mut command = Command::new(exe);
    if let Some(place) = here() {
        command.args([identity::PLACE_ARG, &identity::place_text(&place)]);
    }
    let mut child = command
        .args(["update", "--background", "--tag", tag, "--server", url])
        .current_dir(dir)
        .env("PWD", dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .process_group(0)
        .spawn()
        .context("cannot run riff update")?;
    // A long run, for example riff watch, collects the exit.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// The update in the background (01M3N7JJH0SXXQYYBAHWPCNQGX): takes
/// the update lock, records `tag`, runs [`lifecycle::update`], and tells
/// the lead of the user in the repository of `place` the result
/// (01M3N7JJKBME6VSNTHD8VPN3K9). It stops at once when another update
/// holds the lock, or when `tag` was tried. When its working directory
/// is missing, it stops before it records `tag`
/// (01M3NT2PYFHPB0C19Q2QB2AE6W). `riff` passes
/// [`DEFAULT_SERVER`](crate::api::DEFAULT_SERVER) as `local`.
pub async fn run(
    cargo: &Path,
    claude: &Path,
    tag: &str,
    server: &str,
    local: &str,
    place: Option<&Place>,
) -> Result<()> {
    OFF.store(true, Ordering::Relaxed);
    let dir = local::dir().context("cannot find the local files of riff: set HOME")?;
    let Some(_lock) = local::update(&dir)? else {
        println!("riff: another update of riff runs on this machine.");
        return Ok(());
    };
    if local::tried(&dir).as_deref() == Some(tag) {
        println!("riff: this machine tried {tag} already.");
        return Ok(());
    }
    let old = lifecycle::release_tag(env!("CARGO_PKG_VERSION"));
    let host = crate::identity::this_host();
    let done = match identity::working_dir() {
        Ok(_) => {
            local::set_tried(&dir, tag)?;
            lifecycle::update(cargo, claude, Some(tag), server, local).await
        }
        Err(e) => Err(e),
    };
    let body = match &done {
        Ok(words) => {
            println!("{words}");
            text::auto_updated(&host, &old, tag)
        }
        Err(e) => {
            let tried = local::tried(&dir).as_deref() == Some(tag);
            text::auto_update_failed(&host, &old, tag, &format!("{e:#}"), tried)
        }
    };
    println!("riff: {body}");
    if let Err(e) = crate::worker::tell_lead(place, server, &body).await {
        println!("riff: cannot tell the lead: {e:#}");
    }
    done.map(|_| ())
}

/// What the status line says about a newer release of `riff-server`
/// (01M3NT6X22A4GNFTNKRYV8Z4N1). Each holds the release tag of the
/// server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tag {
    /// The session runs an older release. Run `riff update`.
    Available(String),
    /// The update by itself installs the release now.
    Updating(String),
    /// The release is installed. The session still runs the old one
    /// until its `riff mcp` runs the new one.
    Installed(String),
}

/// The machine as the status line sees it, for [`tag`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Machine<'a> {
    /// `update.auto` is on.
    pub auto: bool,
    /// An update by itself holds the update lock.
    pub updating: bool,
    /// The release that the update by itself tried last.
    pub tried: Option<&'a str>,
}

/// The tag of the status line for a session that runs `session`, on a
/// machine with `installed` on disk and `machine`, in a riff of
/// `server`. `session` is `None` for a riff too old to record its build
/// (01M3NJCRVZW5BFQYZ9N185K2D2). Only the release counts, not the
/// commit. `None` when the session runs the release of the server or a
/// newer one (01M3NT6X22A4GNFTNKRYV8Z4N1).
///
/// ```
/// use riff::auto_update::{tag, Machine, Tag};
/// use riff_core::build::Build;
///
/// let b = |v: &str| Build { version: v.into(), ..Build::this() };
/// let off = Machine::default();
/// let on = Machine { auto: true, ..off };
/// // The same release, or another commit of it: no tag.
/// let dev = Build { commit: "dev".into(), ..b("0.5.0") };
/// assert_eq!(tag(Some(&b("0.5.0")), &b("0.5.0"), &dev, off), None);
/// assert_eq!(tag(Some(&b("0.6.0")), &b("0.6.0"), &b("0.5.0"), off), None);
/// assert_eq!(tag(None, &b("0.5.0"), &b("next"), off), None);
/// // A newer release: update it.
/// assert_eq!(tag(Some(&b("0.5.0")), &b("0.5.0"), &b("0.6.0"), off), Some(Tag::Available("v0.6.0".into())));
/// // With update.auto on, the update by itself installs it.
/// assert_eq!(tag(Some(&b("0.5.0")), &b("0.5.0"), &b("0.6.0"), on), Some(Tag::Updating("v0.6.0".into())));
/// let running = Machine { updating: true, tried: Some("v0.6.0"), ..on };
/// assert_eq!(tag(Some(&b("0.5.0")), &b("0.5.0"), &b("0.6.0"), running), Some(Tag::Updating("v0.6.0".into())));
/// // A failed update by itself waits for the next release.
/// let failed = Machine { tried: Some("v0.6.0"), ..on };
/// assert_eq!(tag(Some(&b("0.5.0")), &b("0.5.0"), &b("0.6.0"), failed), Some(Tag::Available("v0.6.0".into())));
/// // Installed on disk, but the session runs the old release.
/// assert_eq!(tag(Some(&b("0.5.0")), &b("0.6.0"), &b("0.6.0"), failed), Some(Tag::Installed("v0.6.0".into())));
/// // A session of a riff too old to record its build.
/// assert_eq!(tag(None, &b("0.6.0"), &b("0.6.0"), off), Some(Tag::Installed("v0.6.0".into())));
/// ```
pub fn tag(
    session: Option<&Build>,
    installed: &Build,
    server: &Build,
    machine: Machine,
) -> Option<Tag> {
    server.semver()?;
    if session.is_some_and(|s| wanted(s, server).is_none()) {
        return None;
    }
    let release = lifecycle::release_tag(&server.version);
    if wanted(installed, server).is_none() {
        return Some(Tag::Installed(release));
    }
    let starts = machine.auto && (machine.updating || machine.tried != Some(&release));
    Some(if starts {
        Tag::Updating(release)
    } else {
        Tag::Available(release)
    })
}
