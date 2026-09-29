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
//! After the install, the new `riff` is on disk. `riff watch` and
//! `riff tail` run it, and `riff mcp` tells its session to reconnect
//! (see [`binary`](crate::binary)). The update tells the lead of the
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

use crate::{lifecycle, local, settings, text};

/// True in the update in the background: it starts no other update.
static OFF: AtomicBool = AtomicBool::new(false);

/// The release that this process started an update for.
static STARTED: Mutex<Option<String>> = Mutex::new(None);

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
/// process or a Ctrl-C does not stop it. Its output goes to the log in
/// `dir`.
fn start(dir: &Path, tag: &str, url: &str) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let path = local::update_log(dir);
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    let exe = std::env::current_exe().context("cannot find the riff binary")?;
    let mut child = Command::new(exe)
        .args(["update", "--background", "--tag", tag, "--server", url])
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
/// the lead of the user the result (01M3N7JJKBME6VSNTHD8VPN3K9). It
/// stops at once when another update holds the lock, or when `tag` was
/// tried. `riff` passes [`DEFAULT_SERVER`](crate::api::DEFAULT_SERVER)
/// as `local`.
pub async fn run(cargo: &Path, claude: &Path, tag: &str, server: &str, local: &str) -> Result<()> {
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
    local::set_tried(&dir, tag)?;
    let old = lifecycle::release_tag(env!("CARGO_PKG_VERSION"));
    let host = crate::identity::this_host();
    let done = lifecycle::update(cargo, claude, Some(tag), server, local).await;
    let body = match &done {
        Ok(words) => {
            println!("{words}");
            text::auto_updated(&host, &old, tag)
        }
        Err(e) => text::auto_update_failed(&host, &old, tag, &format!("{e:#}")),
    };
    println!("riff: {body}");
    if let Err(e) = crate::worker::tell_lead(server, &body).await {
        println!("riff: cannot tell the lead: {e:#}");
    }
    done.map(|_| ())
}
