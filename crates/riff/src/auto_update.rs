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
//! Each process that talks to the riff can start the update: a session,
//! `riff workers host` in the main clone, or the daemon. The update
//! tells the lead in the repository of that process. The daemon has no
//! place: its working folder can be the work tree of another
//! repository, for example a home folder that is a dotfiles repository.
//! So the daemon calls [`no_place`], and starts the update with no
//! place. That update tells the lead in each repository whose wave the
//! daemon runs ([`lead_places`], 01M58AW5SBG3ZP7149ZY66VTYW). The update of the daemon runs in the
//! unit of the daemon, and a new start of the unit stops it. So the
//! update tells the lead first, and then asks the daemon to start again
//! (01M58AVFM0D62R48HKK8CFY4VW). The daemon does not start again while
//! the update holds the lock ([`crate::daemon::update_waits`],
//! 01M58AW9YR84SKSYBYBJ6V7TNJ).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as riff process
//!     participant S as riff-server
//!     participant U as riff update in the background
//!     participant C as cargo
//!     participant L as lead
//!     participant D as riff daemon
//!     P->>S: call
//!     S-->>P: reply, riff-build: 0.4.0 (newer)
//!     P->>U: start: riff update --tag v0.4.0
//!     U->>U: take the update lock, record v0.4.0
//!     U->>C: install --tag v0.4.0 riff riff-server
//!     U->>L: tell: the host, v0.3.0, v0.4.0, or the error
//!     U->>D: start again with the new riff
//! ```
//!
//! A machine that comes from an older riff can have a build of the
//! release of the server that is not static: the update of the older
//! riff built it with cargo. The daemon starts no session with it
//! (01M510NW8YEVMQKD6RWJPW38N3). So when the `riff` of the bin folder of
//! cargo gets a reply from a server of its own build, and that `riff` or
//! the `riff-server` beside it is not static, [`begin`] also starts
//! `riff update --tag vX.Y.Z` of that release, and `riff update` puts
//! the static files of the release in place (01M510NWBNC20GXFRMHPW12TW9,
//! [`crate::release::needs_static`]). This fix gets one try for each
//! release of its own ([`mark`]). A riff that runs from another folder,
//! for example the build of a clone, starts no fix.
//!
//! After the install, the new `riff` is on disk. `riff watch`,
//! `riff tail`, `riff top`, `riff chat` and `riff mcp` run it (see
//! [`binary`](crate::binary)). The update tells the lead of the
//! user, as the person, once: the host, the old release and the new
//! release, or the error (01M3N7JJKBME6VSNTHD8VPN3K9). `cargo install`
//! replaces the binaries only after a good build, so a failed update
//! keeps the old ones. The update stops no session, and changes no
//! sign-in and no device key (01M3N7JJNPPJNTXFDTEY4MSDVJ).
//!
//! A start of `riff-server` can refuse the message to the lead for a
//! short time. So the update tries again after an error, each
//! [`TELL_EVERY`], for [`TELL_FOR`] ([`tell_until`],
//! 01M56P1T4KK03MF7EK5CC0YZNB).

use std::future::Future;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use riff_core::build::Build;
use riff_core::name::Place;

use crate::wave::Wave;
use crate::{identity, lifecycle, local, release, settings, text};

/// True in the update in the background ([`in_background`]).
static OFF: AtomicBool = AtomicBool::new(false);

/// True in the update in the background ([`run`]). It starts no other
/// update. It prints no note that `riff-server` runs another build: it
/// installs that build, and its last line and its message to the lead
/// say how it went (01M53NEE7W0ZC8B7EV9ET6QHBB).
pub fn in_background() -> bool {
    OFF.load(Ordering::Relaxed)
}

/// True in the daemon ([`no_place`]).
static NO_PLACE: AtomicBool = AtomicBool::new(false);

/// Marks this process as one with no place: the daemon. Its update in
/// the background gets no place, also when the working folder of the
/// daemon is in the work tree of a repository
/// (01M58AW5SBG3ZP7149ZY66VTYW).
pub fn no_place() {
    NO_PLACE.store(true, Ordering::Relaxed);
}

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

/// The place of this process: `None` after [`no_place`]. Else the one
/// that [`remember`] keeps, else the place of the working directory.
/// `None` when neither is known.
fn here() -> Option<Place> {
    if NO_PLACE.load(Ordering::Relaxed) {
        return None;
    }
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

/// The record of the try of `tag` ([`local::tried`]): the tag, and
/// for the release of this riff the tag and ` static`. An update of
/// the release of this riff only puts its static files in place, so
/// that fix gets one try of its own, also after an update of the
/// release with cargo (01M510NWBNC20GXFRMHPW12TW9).
///
/// ```
/// use riff::auto_update::mark;
///
/// let this = format!("v{}", env!("CARGO_PKG_VERSION"));
/// assert_eq!(mark(&this), format!("{this} static"));
/// assert_eq!(mark("v0.0.1"), "v0.0.1");
/// ```
pub fn mark(tag: &str) -> String {
    if tag == lifecycle::release_tag(env!("CARGO_PKG_VERSION")) {
        format!("{tag} static")
    } else {
        tag.to_owned()
    }
}

/// True when this riff is the `riff` of the bin folder of cargo, and
/// that `riff` or the `riff-server` beside it is not static
/// ([`release::needs_static`]).
fn needs_static() -> bool {
    let Some(bin) = release::bin_dir(|name| std::env::var_os(name)) else {
        return false;
    };
    crate::binary::this_on_disk().is_ok_and(|me| release::needs_static(&me, &bin))
}

/// Starts the update in the background when the riff at `url` runs
/// `server`, a newer release than this riff, and this machine has
/// `update.auto = true` (01M3N7JJEKZMN1E5NJQRK2QYVB). When `server` is
/// the build of this riff, it starts the update of that release only
/// when this riff must be static and is not
/// ([`release::needs_static`], 01M510NWBNC20GXFRMHPW12TW9). It does nothing when an update runs,
/// when the release was tried ([`mark`]), or when this process started
/// it already. The check of each reply calls it.
pub fn begin(url: &str, server: &Build) {
    let this = Build::this();
    let (tag, fix) = match wanted(&this, server) {
        Some(tag) => (tag, false),
        None if this.matches(server) && release::TARGET.is_some() => {
            (lifecycle::release_tag(&this.version), true)
        }
        None => return,
    };
    if in_background() {
        return;
    }
    let mark = mark(&tag);
    let Ok(mut started) = STARTED.lock() else {
        return;
    };
    if started.as_deref() == Some(&mark) {
        return;
    }
    if fix {
        // The server of this build answers each call: a process looks
        // once.
        *started = Some(mark.clone());
    }
    if !settings::path().is_ok_and(|p| on(&p)) {
        return;
    }
    let Some(dir) = local::dir() else {
        return;
    };
    if local::updating(&dir) || local::tried(&dir).as_deref() == Some(&mark) {
        *started = Some(mark);
        return;
    }
    if fix && !needs_static() {
        return;
    }
    match start(&dir, &tag, url) {
        Ok(()) if fix => eprintln!("riff: {}", text::auto_update_static_started(&tag)),
        Ok(()) => eprintln!("riff: {}", text::auto_update_started(&tag)),
        Err(e) => eprintln!("riff: cannot start the update of riff: {e:#}"),
    }
    *started = Some(mark);
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
    local::make_dir(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let path = local::update_log(dir);
    let log = local::options()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    local::private(&log).with_context(|| format!("cannot set the mode of {}", path.display()))?;
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
/// the update lock, records the try of `tag` ([`mark`]), runs
/// [`lifecycle::update`], tells the lead of the user in the repository
/// of `place`, or with no `place` in each repository that the daemon
/// serves ([`lead_places`]), the result (01M3N7JJKBME6VSNTHD8VPN3K9),
/// and then runs [`lifecycle::daemon_after_update`]. It
/// stops at once when another update holds the lock, or when `tag` was
/// tried. When its working directory is missing, it stops before it
/// records the try (01M3NT2PYFHPB0C19Q2QB2AE6W). `riff` passes
/// [`DEFAULT_SERVER`](riff_client::api::DEFAULT_SERVER) as `local`.
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
    let mark = mark(tag);
    if local::tried(&dir).as_deref() == Some(mark.as_str()) {
        println!("riff: this machine tried {tag} already.");
        return Ok(());
    }
    let old = lifecycle::release_tag(env!("CARGO_PKG_VERSION"));
    let host = crate::identity::this_host();
    let done = match identity::working_dir() {
        Ok(_) => {
            local::set_tried(&dir, &mark)?;
            lifecycle::update(cargo, claude, Some(tag), server, local).await
        }
        Err(e) => Err(e),
    };
    let body = match &done {
        Ok(words) => {
            println!("{words}");
            if tag == old {
                text::auto_updated_static(&host, tag)
            } else {
                text::auto_updated(&host, &old, tag)
            }
        }
        Err(e) => {
            let tried = local::tried(&dir).as_deref() == Some(mark.as_str());
            text::auto_update_failed(&host, &old, tag, &format!("{e:#}"), tried)
        }
    };
    println!("riff: {body}");
    // Never the working directory: it is the local files of riff
    // (01M58AW5SBG3ZP7149ZY66VTYW).
    let leads = lead_places(place, &served(), server);
    if leads.is_empty() {
        println!(
            "riff: cannot tell the lead: {}",
            text::no_lead_place(server)
        );
    }
    for lead in &leads {
        let tell = || crate::worker::tell_lead(Some(lead), server, &body);
        if let Err(e) = tell_until(tell, TELL_EVERY, TELL_FOR).await {
            println!("riff: cannot tell the lead in {}: {e:#}", lead.repo_text());
        }
    }
    // The start of the daemon again stops each process of its unit: so
    // after the message (01M58AVFM0D62R48HKK8CFY4VW).
    if done.is_ok() {
        for line in lifecycle::daemon_after_update().await {
            println!("{line}");
        }
    }
    done.map(|_| ())
}

/// The repositories that the daemon of this machine serves. None when
/// it serves none, or its file cannot be read.
fn served() -> Vec<Wave> {
    let served = crate::daemon::dir().and_then(|dir| crate::daemon::remote::read_served(&dir));
    served
        .map(|served| served.into_values().map(|s| s.wave).collect())
        .unwrap_or_default()
}

/// The places whose leads the update in the background tells
/// (01M58AW5SBG3ZP7149ZY66VTYW): `given`, the place of the process that
/// started the update, when it is in a repository. Else the place of
/// each repository that the daemon of this machine serves on `server`,
/// from `served`, once: the daemon has no place. A `/` at the end of a
/// URL does not count. Empty when there is none.
///
/// ```
/// use riff::auto_update::lead_places;
/// use riff::identity::place_from_text;
/// use riff::wave::Wave;
///
/// let wave = |repo: &str, server: &str| Wave {
///     repo: repo.into(),
///     main: "/src/riff".into(),
///     server: server.into(),
///     host: "thelio".into(),
/// };
/// let served = [
///     wave("acme/beta", "http://other"),
///     wave("acme/alpha", "http://riff"),
///     wave("acme/gamma", "http://riff/"),
///     wave("acme/alpha", "http://riff"),
/// ];
/// // A session or a workers host: its own repository.
/// let session = place_from_text("thelio/acme/gamma#issue-7")?;
/// assert_eq!(lead_places(Some(&session), &served, "http://riff"), [session]);
/// // The daemon: each repository that it serves on the same riff.
/// let both = [
///     place_from_text("thelio/acme/alpha")?,
///     place_from_text("thelio/acme/gamma")?,
/// ];
/// assert_eq!(lead_places(None, &served, "http://riff"), both);
/// let outside = place_from_text("thelio/-")?;
/// assert_eq!(lead_places(Some(&outside), &served, "http://riff"), both);
/// assert!(lead_places(None, &served[..1], "http://riff").is_empty());
/// assert!(lead_places(None, &[], "http://riff").is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn lead_places(given: Option<&Place>, served: &[Wave], server: &str) -> Vec<Place> {
    if let Some(place) = given.filter(|p| p.default_thread().is_some()) {
        return vec![place.clone()];
    }
    let mut places: Vec<Place> = Vec::new();
    let server = riff_client::link::base_of(server);
    let same = |wave: &&Wave| riff_client::link::base_of(&wave.server) == server;
    for wave in served.iter().filter(same) {
        if let Ok(place) = wave.place(None)
            && !places.contains(&place)
        {
            places.push(place);
        }
    }
    places
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

/// The time between two tries of the message to the lead.
pub const TELL_EVERY: Duration = Duration::from_secs(5);

/// The time for which the update tries the message to the lead.
pub const TELL_FOR: Duration = Duration::from_secs(2 * 60);

/// Calls `tell` until it succeeds, each `every`, for at most `max`
/// (01M56P1T4KK03MF7EK5CC0YZNB). Gives the last error when no try
/// succeeds. So a start of `riff-server` does not lose the message.
///
/// ```
/// use std::cell::Cell;
/// use std::time::Duration;
/// use riff::auto_update::tell_until;
///
/// # tokio::runtime::Builder::new_current_thread().enable_time().build()?.block_on(async {
/// let tries = Cell::new(0);
/// let tell = || {
///     tries.set(tries.get() + 1);
///     let n = tries.get();
///     async move { if n < 3 { anyhow::bail!("refused") } else { Ok(()) } }
/// };
/// tell_until(tell, Duration::ZERO, Duration::from_secs(1)).await?;
/// assert_eq!(tries.get(), 3);
/// # anyhow::Ok(())
/// # })?;
/// # anyhow::Ok(())
/// ```
pub async fn tell_until<F, T>(mut tell: F, every: Duration, max: Duration) -> Result<()>
where
    F: FnMut() -> T,
    T: Future<Output = Result<()>>,
{
    let end = tokio::time::Instant::now() + max;
    loop {
        match tell().await {
            Ok(()) => return Ok(()),
            Err(e) if tokio::time::Instant::now() + every > end => return Err(e),
            Err(e) => {
                println!("riff: cannot tell the lead yet, it tries again: {e:#}");
                tokio::time::sleep(every).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    /// A `tell` that fails `fails` times, then succeeds. It counts the
    /// tries and the messages that arrived.
    fn flaky<'a>(
        fails: usize,
        tries: &'a Cell<usize>,
        told: &'a Cell<usize>,
    ) -> impl FnMut() -> std::future::Ready<Result<()>> + 'a {
        move || {
            tries.set(tries.get() + 1);
            std::future::ready(if tries.get() <= fails {
                Err(anyhow::anyhow!("no other session is the lead"))
            } else {
                told.set(told.get() + 1);
                Ok(())
            })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_tell_tries_again_and_the_lead_gets_the_message_once() {
        let (tries, told) = (Cell::new(0), Cell::new(0));
        tell_until(flaky(1, &tries, &told), TELL_EVERY, TELL_FOR)
            .await
            .unwrap();
        assert_eq!((tries.get(), told.get()), (2, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn the_tries_stop_after_the_bound() {
        let (tries, told) = (Cell::new(0), Cell::new(0));
        let start = tokio::time::Instant::now();
        let error = tell_until(flaky(usize::MAX, &tries, &told), TELL_EVERY, TELL_FOR)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("lead"), "{error}");
        assert!(start.elapsed() <= TELL_FOR);
        assert_eq!(tries.get(), 25);
        assert_eq!(told.get(), 0);
    }
}
