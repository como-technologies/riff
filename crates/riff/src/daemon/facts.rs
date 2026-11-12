//! The facts of the host that the daemon sends to the lead (#1065,
//! 01M57D3F50347RD8ED79KEA9V7).
//!
//! # Design
//!
//! Each [`every`], the session of the daemon for each repository that it
//! serves ([`super::remote`]) sends the [`Facts`] of its host to
//! riff-server, as the numbers of its machine. `riff who` and `riff top`
//! of each machine show them for each host
//! ([`riff_client::daemons`]). The facts are:
//!
//! - from the status of the daemon: the riff build, the limit and the
//!   floor, the caps of each new container, the live sessions with their
//!   item, phase, last step and the digest of the image of their
//!   container, the starts that wait, the state of each wave, the parts
//!   of the host, each repository where the host takes no new claim
//!   and still runs sessions ([`of_status`], [`left`],
//!   01M6WJYQJ58MHQF1XPXXZW2ET6), and the release of each watch of a
//!   deploy or of a check, at its step ([`crate::watches::Watch::release`],
//!   01M6XTS7RGXB30MFM6DZ7PVZ75), the current wave of each watch of a
//!   wave whose release waits for its cut
//!   ([`crate::watches::Seen::complete`], 01M75N6YNHQQHGS0VGKTKSNJTR), and
//!   the release of each watch of a wave from the cut to the check, at its
//!   step ([`crate::watches::Seen::release`], 01M75R6FPVZA3JA5C48PQ26A85);
//! - from the host: the load, the free memory, the disk of the session
//!   folders, the version of podman, the merged pin of each wave
//!   with whether podman has it ([`look`], [`pins`],
//!   01M5TKPG5WZ1T58FNAMZNP6CJ5), and `update.auto`: the watch of a
//!   deploy on another host waits for this host when it is on
//!   ([`crate::watches::Riffs`], 01M6WFNDS76NESPXHMQPGHHQSS);
//! - the sign-ins of riff, `gh` and `claude` (01M57D3FGV57W96A7SWTEFRCGW),
//!   and whether the token of `riff claude-token` is set
//!   (01M58B7HA8CRCW0MJ17QP3R5QG).
//!
//! At each look, the session also tells the lead of its repository, with
//! one note each time:
//!
//! - when no token of `riff claude-token` is set
//!   (01M58B7HCXKZF39DCF6KV84JRV), and the sign-in of `claude` has less than
//!   [`SHORT_MINUTES`](riff_client::daemons::SHORT_MINUTES) left, the step
//!   for the person, and when it comes back ([`Short`],
//!   01M57D3FC5PB5EQ6Y38Y5R2DBB);
//! - when a sweep keeps a folder whose work is over, because its clone
//!   holds work that is not on `origin` ([`News`],
//!   01M57D3FEJPHD962GW0H82N8E6).
//!
//! Each step of a look has a time limit (#1858,
//! 01M749XKWD72JYB3QD8RYNHKTV): the status of the daemon [`DAEMON_WAIT`],
//! each `podman` and `gh` [`STEP_WAIT`], the whole look [`LOOK_WAIT`], and
//! each call to riff-server its budget. A step that hangs, for example a
//! `podman` that waits for a lock, gives no fact or a failed send: it
//! holds up no next look. [`Sends`] keeps the outcome of each send
//! (01M749XKZ7N13R6EDJEREZZ0F4). The first failure logs one warning line
//! with its cause, the next failures log none, and the first send that
//! goes out again logs one line. `riff daemon status` shows the cause
//! while the sends fail.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as riff daemon on thelio
//!     participant S as riff-server
//!     participant L as lead on pangolin
//!     loop each 30 s
//!         D->>D: the status, the host, the sign-ins, each in its time limit
//!         D->>S: machine: "daemon facts: {...}"
//!         opt the first send that fails, or the first one that goes out again
//!             D->>D: one line in the log
//!         end
//!         opt the sign-in of claude goes under 2 hours, or comes back
//!             D->>S: one note to the lead
//!         end
//!         opt a sweep kept a folder with work that is not on origin
//!             D->>S: one note to the lead
//!         end
//!     end
//!     L->>S: riff who, riff top
//!     S-->>L: the facts of each host
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use riff_client::api::Api;
use riff_client::daemons::{self, Facts, Image, Sign};
use serde::{Deserialize, Serialize};

use super::Status;
use crate::session_image;
use crate::text;
use crate::wave::Wave;

/// The variable that gives the time between two looks in milliseconds.
/// Only a test sets it.
///
/// ```
/// assert_eq!(riff::daemon::facts::EVERY_VAR, "RIFF_FACTS_EVERY_MS");
/// ```
pub const EVERY_VAR: &str = "RIFF_FACTS_EVERY_MS";

/// The most notes for one repository that wait for the session of the
/// daemon.
pub const NEWS_MOST: usize = 50;

/// The longest wait of a look, and of the status of the session of the
/// daemon, for the status of the daemon (#1858). The status asks podman
/// for the digest of each new container, up to 5 s each.
pub const DAEMON_WAIT: Duration = Duration::from_secs(20);

/// The longest time of one `podman` or `gh` of a look (#1858). A step
/// that takes longer gives no fact: riff stops it.
pub const STEP_WAIT: Duration = Duration::from_secs(10);

/// The longest time of a whole look (#1858). A look that takes longer
/// is a failed send.
pub const LOOK_WAIT: Duration = Duration::from_secs(60);

/// The time between two looks: [`daemons::EVERY`], or the milliseconds
/// of [`EVERY_VAR`].
pub fn every() -> Duration {
    std::env::var(EVERY_VAR)
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(daemons::EVERY, Duration::from_millis)
}

/// The facts of the status of the daemon: the build, the limit, the
/// caps of each new container, the sessions, the starts that wait and
/// the waves. The lead counts in no limit: it is not a session of the
/// facts.
///
/// ```
/// use riff::daemon::facts::of_status;
/// use riff_client::daemons::Caps;
///
/// let status: riff::daemon::Status = serde_json::from_value(serde_json::json!({
///     "pid": 7, "build": "3.1.0", "since_ms": 0, "dir": "/d",
///     "sessions": [
///         {"name": "riff-issue-12-1a2b3c4d", "session": "1a2b3c4d", "item": "issue-12",
///          "image": "i", "digest": "0123456789ab", "since_ms": 0, "phase": "works",
///          "last": "> Bash: just check"},
///         {"name": "riff-lead-o-r", "session": "l1", "item": "lead",
///          "image": "i", "since_ms": 0, "phase": "terminal"},
///     ],
///     "limit": {"sessions": 4, "floor_gb": 8, "cpus": 8, "memory_gb": 8},
///     "waiting": ["riff-issue-13-2b3c4d5e"],
///     "waves": [{"repo": "o/r", "word": "started issue-12", "waits": {"13": 41}}],
///     "parts": [{"repo": "o/r", "workers": true, "lead": true}],
///     "leaving": [{"repo": "o/s", "sessions": ["riff-issue-7-3c4d5e6f"]}],
///     "test": {"tag": "v4.1.1", "commit": "90e36bfd", "secs": 185},
///     "claude_token": "set",
///     "watches": [
///         {"repo": "o/r", "what": "the deploy of v4.1.1", "since_ms": 0,
///          "release": {"repo": "o/r", "tag": "v4.1.1", "step": "waits"}},
///         {"repo": "o/r", "what": "the pull request #9 of the release", "since_ms": 0},
///     ],
///     "complete": [{"repo": "o/q", "wave": 41, "waits": "main_failed"}],
///     "releases": [{"repo": "o/s", "tag": "v2.0.0", "step": "test_runs"}],
/// }))?;
/// let facts = of_status(&status);
/// assert_eq!((facts.build.as_str(), facts.limit, facts.floor_gb), ("3.1.0", 4, 8));
/// assert_eq!(facts.caps, Some(Caps { cpus: 8, memory_gb: 8 }));
/// assert_eq!(facts.sessions.len(), 1);
/// assert_eq!(facts.sessions[0].last, "> Bash: just check");
/// assert_eq!(facts.sessions[0].digest, "0123456789ab");
/// assert_eq!(facts.waiting, ["riff-issue-13-2b3c4d5e"]);
/// assert_eq!(facts.waves[0].word, "started issue-12");
/// // The verify of 13 waits for the merge of #41 (#1710).
/// assert_eq!(facts.waves[0].waits[&13], 41);
/// assert_eq!(facts.parts[0].parts.word(), "workers and lead");
/// // The host left o/s, and one session of it goes on
/// // (01M6WJYQJ58MHQF1XPXXZW2ET6).
/// assert_eq!(facts.leaving[0].sessions, ["riff-issue-7-3c4d5e6f"]);
/// assert_eq!(facts.test.as_ref().map(|t| t.secs), Some(185));
/// assert_eq!(facts.claude_token, riff_client::daemons::Token::Set);
/// // The release of each watch of a deploy or of a check
/// // (01M6XTS7RGXB30MFM6DZ7PVZ75), then of each watch of a wave
/// // (01M75R6FPVZA3JA5C48PQ26A85).
/// assert_eq!(facts.releases.len(), 2);
/// assert_eq!(facts.releases[0].step, riff_client::daemons::ReleaseStep::Waits);
/// assert_eq!(facts.releases[1].step, riff_client::daemons::ReleaseStep::TestRuns);
/// // The complete wave of each watch of a wave (01M75N6YNHQQHGS0VGKTKSNJTR).
/// assert_eq!(facts.complete[0].wave, 41);
/// assert_eq!(facts.complete[0].waits, Some(riff_client::daemons::CutWaits::MainFailed));
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn of_status(status: &Status) -> Facts {
    let limit = status.limit.unwrap_or_default();
    Facts {
        build: status.build.clone(),
        limit: limit.sessions,
        floor_gb: limit.floor_gb,
        caps: Some(daemons::Caps {
            cpus: limit.cpus,
            memory_gb: limit.memory_gb,
        }),
        sessions: status
            .sessions
            .iter()
            .filter(|live| live.phase != riff_client::sessions::Phase::Terminal)
            .map(|live| daemons::Session {
                digest: live.digest.clone(),
                name: live.name.clone(),
                item: live.item.clone(),
                phase: live.phase,
                last: live.last.clone(),
            })
            .collect(),
        waiting: status.waiting.clone(),
        waves: status
            .waves
            .iter()
            .map(|w| daemons::Wave {
                repo: w.repo.clone(),
                word: w.word.clone(),
                image: None,
                waits: w.waits.clone(),
            })
            .collect(),
        parts: status.parts.clone(),
        leaving: status.leaving.clone(),
        test: status.test.clone(),
        claude_token: status.claude_token,
        releases: status
            .watches
            .iter()
            .filter_map(|w| w.release.clone())
            .chain(status.releases.iter().cloned())
            .collect(),
        complete: status.complete.clone(),
        ..Facts::default()
    }
}

/// The facts of the daemon in the folder `dir` and of its host, for the
/// repository of `wave`. Each step that can block runs on a thread for
/// blocking work. A fact that riff cannot read stays empty.
///
/// When the daemon serves the repository of `wave` no more, the facts
/// name it as a repository that the host left ([`left`],
/// 01M6WJYQJ58MHQF1XPXXZW2ET6).
pub async fn look(dir: &Path, wave: &Wave, api: &Api) -> anyhow::Result<Facts> {
    let mut facts = of_status(&super::status_within(dir, DAEMON_WAIT).await?);
    let dir = dir.to_owned();
    let (server, repo) = (wave.server.clone(), wave.repo.clone());
    let mut waves = std::mem::take(&mut facts.waves);
    let host = tokio::task::spawn_blocking(move || {
        pins(&dir, &mut waves, podman_has);
        // A file that cannot be read counts as served.
        let served = super::remote::read_served(&dir).map_or(true, |all| all.contains_key(&repo));
        (
            crate::machine::here(),
            crate::disk::here(&dir),
            podman(),
            waves,
            gh_sign(&host_gh()),
            claude(crate::run::now_ms()),
            riff_client::login::stored(&server),
            crate::settings::path()
                .and_then(|path| crate::settings::update_auto(&path))
                .ok(),
            served,
        )
    })
    .await?;
    let (machine, disk, podman, waves, gh, claude, stored, auto, served) = host;
    if !served {
        left(&mut facts, &wave.repo);
    }
    facts.auto = auto;
    facts.machine = Some(machine);
    facts.disk = disk;
    facts.podman = podman;
    facts.waves = waves;
    facts.gh = gh;
    facts.claude = claude;
    facts.riff = match api.has_sign_in().await {
        Ok(false) => Sign::On,
        Ok(true) => riff_of(stored),
        Err(_) => Sign::Unknown,
    };
    Ok(facts)
}

/// Names `repo` in `facts` as a repository that the host left, with no
/// session (01M6WJYQJ58MHQF1XPXXZW2ET6), when the facts give the host no
/// part in it and do not name it yet. [`look`] calls it when the daemon
/// serves `repo` no more: the look after the last session, before the
/// session of the daemon for `repo` ends ([`super::remote::serve`]).
///
/// ```
/// use riff::daemon::facts::left;
/// use riff_client::daemons::{Facts, Leaving, Parts, RepoParts};
///
/// let mut facts = Facts::default();
/// left(&mut facts, "o/r");
/// assert_eq!(facts.leaving, [Leaving { repo: "o/r".into(), sessions: vec![] }]);
/// // One time only.
/// left(&mut facts, "o/r");
/// assert_eq!(facts.leaving.len(), 1);
/// // A host with a part in the repository did not leave it.
/// let lead = Parts { workers: false, lead: true };
/// let mut facts = Facts { parts: vec![RepoParts { repo: "o/r".into(), parts: lead }], ..Facts::default() };
/// left(&mut facts, "o/r");
/// assert!(facts.leaving.is_empty());
/// ```
pub fn left(facts: &mut Facts, repo: &str) {
    let named =
        facts.parts.iter().any(|p| p.repo == repo) || facts.leaving.iter().any(|l| l.repo == repo);
    if !named {
        facts.leaving.push(daemons::Leaving {
            repo: repo.to_owned(),
            sessions: Vec::new(),
        });
    }
}

/// The version of podman on `PATH`, or `None` with no podman.
fn podman() -> Option<String> {
    let out =
        crate::run::output_within("podman", Path::new("/"), &["--version"], STEP_WAIT).ok()?;
    Some(out.rsplit(' ').next().unwrap_or(&out).to_owned())
}

/// The merged pin of the main clone `main` (01M5TKPG5WZ1T58FNAMZNP6CJ5):
/// the short digest of the image, and whether `has` says that the host
/// has the image. `None` when the clone has no good merged pin.
fn image(main: &Path, has: impl Fn(&str) -> bool) -> Option<Image> {
    let image = session_image::merged_here(main).ok()?;
    Some(Image {
        digest: daemons::short(image.digest()),
        here: has(&image.to_string()),
    })
}

/// True when podman has the image `image` on this host.
fn podman_has(image: &str) -> bool {
    let args = ["image", "exists", image];
    crate::run::output_within("podman", Path::new("/"), &args, STEP_WAIT).is_ok()
}

/// Gives each wave of `waves` its merged pin: the pin of the main clone
/// that the daemon keeps for the repository of the wave in
/// [`crate::wave::WAVES_FILE`] of the folder `dir`. `has` says whether
/// the host has an image. A wave that the file does not know has none.
///
/// Two repositories of one host pin two images, and the host has only the
/// first one:
///
/// ```
/// use std::collections::BTreeMap;
/// use std::process::Command;
/// use riff::daemon::facts::pins;
/// use riff::session_image::FILE;
/// use riff::wave::{Wave as Known, write_waves};
/// use riff_client::daemons::{Image, Wave};
///
/// let root = tempfile::tempdir()?;
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
/// let pin = |n: &str| format!("ghcr.io/o/i@sha256:{}", n.repeat(64));
/// // A main clone whose origin has the pin `n`.
/// let clone = |name: &str, n: &str| -> std::io::Result<std::path::PathBuf> {
///     let (origin, main) = (root.path().join(format!("{name}.git")), root.path().join(name));
///     git(root.path(), &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
///     git(root.path(), &["init", "-q", "-b", "main", main.to_str().unwrap()]);
///     std::fs::write(main.join(FILE), format!("[session]\nimage = \"{}\"\n", pin(n)))?;
///     git(&main, &["add", FILE]);
///     git(&main, &["commit", "-q", "-m", "pin"]);
///     git(&main, &["remote", "add", "origin", origin.to_str().unwrap()]);
///     git(&main, &["push", "-q", "origin", "main"]);
///     git(&main, &["remote", "set-head", "origin", "main"]);
///     Ok(main)
/// };
/// let known = |repo: &str, main| Known {
///     repo: repo.into(),
///     main,
///     server: "http://127.0.0.1:1".into(),
///     host: "thelio".into(),
/// };
/// let (riff, sens) = (clone("riff", "a")?, clone("sens", "b")?);
/// let folder = root.path().join("daemon");
/// std::fs::create_dir(&folder)?;
/// write_waves(&folder, &BTreeMap::from([
///     ("o/riff".to_owned(), known("o/riff", riff)),
///     ("o/sens".to_owned(), known("o/sens", sens)),
/// ]))?;
///
/// let wave = |repo: &str| Wave { repo: repo.into(), word: "waits".into(), image: None, waits: BTreeMap::new() };
/// let mut waves = vec![wave("o/riff"), wave("o/sens"), wave("o/other")];
/// pins(&folder, &mut waves, |image| image == pin("a"));
/// let image = |digit: &str, here| Some(Image { digest: digit.repeat(12), here });
/// assert_eq!(waves[0].image, image("a", true));
/// assert_eq!(waves[1].image, image("b", false));
/// assert_eq!(waves[2].image, None, "the daemon keeps no main clone for it");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn pins(dir: &Path, waves: &mut [daemons::Wave], has: impl Fn(&str) -> bool + Copy) {
    let known = crate::wave::read_waves(dir).unwrap_or_default();
    for wave in waves {
        wave.image = known
            .get(&wave.repo)
            .and_then(|known| image(&known.main, has));
    }
}

/// The `gh` of a look: the `gh` of the facts of the host
/// ([`host_gh`](crate::run::host_gh)) with the time limit [`STEP_WAIT`].
/// It is a `gh` of the daemon, so it runs no `gh` that would ask the
/// keyring for an unlock prompt (01M77AB7XZV8KG8RRZ8C6R5A8X).
///
/// ```
/// use riff::daemon::facts::{STEP_WAIT, host_gh};
///
/// assert_eq!(host_gh().limit(), Some(STEP_WAIT));
/// assert_eq!(host_gh().caller(), riff::run::host_gh().caller());
/// ```
pub fn host_gh() -> crate::pr::Gh {
    crate::run::host_gh().with_limit(STEP_WAIT)
}

/// The sign-in of `gh` with `gh auth token`: it gives a token, or not.
/// riff keeps no token of it. While the token of `gh` is locked in the
/// keyring, the daemon runs no `gh` ([`host_gh`]), and cannot say.
///
/// ```
/// use std::time::Duration;
/// use riff::daemon::facts::gh_sign;
/// use riff::pr::Gh;
/// use riff_client::daemons::Sign;
///
/// let dir = tempfile::tempdir()?;
/// let gh = |script: &str| Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// assert_eq!(gh_sign(&gh("#!/bin/sh\necho gho_not_real\n")), Sign::On);
/// assert_eq!(gh_sign(&gh("#!/bin/sh\n")), Sign::Off);
/// assert_eq!(gh_sign(&gh("#!/bin/sh\necho 'not logged in' >&2\nexit 1\n")), Sign::Off);
/// let hangs = gh("#!/bin/sh\nsleep 600\n").with_limit(Duration::from_millis(500));
/// assert_eq!(gh_sign(&hangs), Sign::Off);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn gh_sign(gh: &crate::pr::Gh) -> Sign {
    match gh
        .run(&["auth", "token"], None)
        .map(zeroize::Zeroizing::new)
    {
        Ok(token) if !token.trim().is_empty() => Sign::On,
        Err(e) if riff_client::secrets::is_locked(&e) => Sign::Unknown,
        _ => Sign::Off,
    }
}

/// The sign-in of `claude` of the person at the time `now_ms`: the
/// minutes left, or [`Sign::Off`] with no sign-in file.
fn claude(now_ms: i64) -> Sign {
    let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) else {
        return Sign::Unknown;
    };
    let path = crate::run::credentials_in(Path::new(&home));
    let Ok(file) = std::fs::read_to_string(path) else {
        return Sign::Off;
    };
    match crate::run::sign_in(&file, now_ms) {
        Ok(sign_in) => Sign::Left {
            minutes: sign_in.minutes,
        },
        Err(_) => Sign::Unknown,
    }
}

/// The sign-in of riff from the keyring: on, ended or none, or unknown
/// when the keyring does not answer.
///
/// ```
/// use riff::daemon::facts::riff_of;
/// use riff_client::daemons::Sign;
/// use riff_client::login::SignIn;
///
/// let sign_in = SignIn {
///     user: "mike".into(),
///     access_token: "a".into(),
///     refresh_token: "r".into(),
///     expires_at: 0,
///     riff_id: None,
/// };
/// assert_eq!(riff_of(Ok(Some(sign_in.clone()))), Sign::On);
/// assert_eq!(riff_of(Ok(Some(sign_in.end()))), Sign::Off);
/// assert_eq!(riff_of(Ok(None)), Sign::Off);
/// assert_eq!(riff_of(Err(anyhow::anyhow!("locked"))), Sign::Unknown);
/// ```
pub fn riff_of(stored: anyhow::Result<Option<riff_client::login::SignIn>>) -> Sign {
    match stored {
        Ok(Some(sign_in)) if !sign_in.ended() => Sign::On,
        Ok(_) => Sign::Off,
        Err(_) => Sign::Unknown,
    }
}

/// What [`Short`] says at a look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    /// The sign-in of `claude` went short, with the minutes left.
    Short(i64),
    /// It came back.
    Back,
}

/// The gate of the notes of the sign-in of `claude`
/// (01M57D3FC5PB5EQ6Y38Y5R2DBB): one note when it goes short, not one at
/// each look, and one note when it comes back. A look that cannot say
/// changes nothing.
///
/// ```
/// use riff::daemon::facts::{Said, Short};
/// use riff_client::daemons::Sign;
///
/// let mut gate = Short::default();
/// assert_eq!(gate.look(&Sign::Left { minutes: 300 }), None);
/// assert_eq!(gate.look(&Sign::Left { minutes: 100 }), Some(Said::Short(100)));
/// assert_eq!(gate.look(&Sign::Left { minutes: 90 }), None, "one note, not one at each look");
/// assert_eq!(gate.look(&Sign::Unknown), None);
/// assert_eq!(gate.look(&Sign::Left { minutes: 480 }), Some(Said::Back));
/// assert_eq!(gate.look(&Sign::Left { minutes: 470 }), None);
/// assert_eq!(gate.look(&Sign::Off), Some(Said::Short(0)));
/// ```
#[derive(Debug, Default)]
pub struct Short {
    short: bool,
}

impl Short {
    /// The note to send for the sign-in `sign`, or `None`.
    pub fn look(&mut self, sign: &Sign) -> Option<Said> {
        if *sign == Sign::Unknown {
            return None;
        }
        let short = sign.short();
        if short == self.short {
            return None;
        }
        self.short = short;
        Some(match sign {
            Sign::Left { minutes } if short => Said::Short((*minutes).max(0)),
            _ if short => Said::Short(0),
            _ => Said::Back,
        })
    }
}

/// The notes of the daemon for the lead of each repository, until the
/// session of the daemon for that repository sends them
/// (01M57D3FEJPHD962GW0H82N8E6). It keeps at most [`NEWS_MOST`] for each
/// repository: the oldest go first.
///
/// ```
/// use riff::daemon::facts::News;
///
/// let news = News::default();
/// news.push("o/r", "kept the folder".into());
/// assert_eq!(news.take("o/s"), Vec::<String>::new());
/// assert_eq!(news.take("o/r"), ["kept the folder"]);
/// assert!(news.take("o/r").is_empty());
/// for i in 0..60 {
///     news.push("o/r", i.to_string());
/// }
/// let kept = news.take("o/r");
/// assert_eq!((kept.len(), kept[0].as_str()), (50, "10"));
/// ```
#[derive(Debug, Clone, Default)]
pub struct News(Arc<Mutex<BTreeMap<String, Vec<String>>>>);

impl News {
    /// Keeps `note` for the lead of `repo`.
    pub fn push(&self, repo: &str, note: String) {
        let mut all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let notes = all.entry(repo.to_owned()).or_default();
        notes.push(note);
        let over = notes.len().saturating_sub(NEWS_MOST);
        notes.drain(..over);
    }

    /// Takes each note for the lead of `repo`.
    pub fn take(&self, repo: &str) -> Vec<String> {
        let mut all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        all.remove(repo).unwrap_or_default()
    }
}

/// The sends of the facts for one repository (#1858): the last one that
/// went out, and the sends that failed after it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Sent {
    /// The repository, `OWNER/NAME`.
    pub repo: String,
    /// The last send that went out, in milliseconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok_ms: Option<i64>,
    /// The sends that failed after it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<Failed>,
}

/// The sends of the facts that failed in a row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failed {
    /// The first failure, in milliseconds since the epoch.
    pub since_ms: i64,
    /// The cause of the last failure, in one line.
    pub cause: String,
    /// The failures.
    pub count: u64,
}

/// The sends of the facts of each repository that the daemon serves
/// (#1858, 01M749XKZ7N13R6EDJEREZZ0F4). The session of the daemon for a
/// repository keeps the outcome of each send ([`Sends::record`]), and
/// forgets it at its end. `riff daemon status` shows them
/// ([`super::Status::sends`]).
///
/// A send that fails logs one warning line with its cause. The next
/// failures log no line, and keep the newest cause. The first send that
/// goes out again logs one line:
///
/// ```
/// use riff::daemon::facts::Sends;
///
/// let sends = Sends::default();
/// assert_eq!(sends.record("o/r", 1_000, Ok(())), None);
/// let warning = sends.record("o/r", 31_000, Err("machine failed (500): down\nnow".into()));
/// assert_eq!(
///     warning.as_deref(),
///     Some("warning: o/r: cannot send the facts of this host: machine failed (500): down now. \
///           riff tries again at each look.")
/// );
/// assert_eq!(sends.record("o/r", 61_000, Err("cannot reach riff-server".into())), None);
/// let shown = sends.shown();
/// let failed = shown[0].failed.as_ref().unwrap();
/// assert_eq!((shown[0].ok_ms, failed.since_ms, failed.count), (Some(1_000), 31_000, 2));
/// assert_eq!(failed.cause, "cannot reach riff-server");
///
/// let again = sends.record("o/r", 91_000, Ok(()));
/// assert_eq!(
///     again.as_deref(),
///     Some("o/r: the facts of this host go out again, after 2 failed sends in 1m.")
/// );
/// assert_eq!(sends.shown()[0].failed, None);
/// sends.forget("o/r");
/// assert!(sends.shown().is_empty());
/// ```
#[derive(Debug, Clone, Default)]
pub struct Sends(Arc<Mutex<BTreeMap<String, Sent>>>);

impl Sends {
    /// Keeps the outcome `sent` of a send of the facts for `repo` at the
    /// time `now_ms`, and gives the line for the log, when there is one.
    pub fn record(&self, repo: &str, now_ms: i64, sent: Result<(), String>) -> Option<String> {
        let mut all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = all.entry(repo.to_owned()).or_insert_with(|| Sent {
            repo: repo.to_owned(),
            ..Sent::default()
        });
        match sent {
            Ok(()) => {
                entry.ok_ms = Some(now_ms);
                let failed = entry.failed.take()?;
                let secs = u64::try_from((now_ms - failed.since_ms) / 1000).unwrap_or(0);
                Some(text::daemon_facts_again(repo, failed.count, secs))
            }
            Err(cause) => {
                let cause = cause.split_whitespace().collect::<Vec<_>>().join(" ");
                if let Some(failed) = &mut entry.failed {
                    failed.cause = cause;
                    failed.count += 1;
                    return None;
                }
                let line = text::daemon_facts_failed(repo, &cause);
                entry.failed = Some(Failed {
                    since_ms: now_ms,
                    cause,
                    count: 1,
                });
                Some(line)
            }
        }
    }

    /// Forgets the sends for `repo`: the daemon serves it no more.
    pub fn forget(&self, repo: &str) {
        let mut all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        all.remove(repo);
    }

    /// The sends of each repository, by repository.
    pub fn shown(&self) -> Vec<Sent> {
        let all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        all.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(repo: &str) -> daemons::Wave {
        daemons::Wave {
            repo: repo.into(),
            word: "waits".into(),
            image: None,
            waits: BTreeMap::new(),
        }
    }

    fn known(repo: &str, main: &Path) -> Wave {
        Wave {
            repo: repo.into(),
            main: main.to_owned(),
            server: "http://127.0.0.1:1".into(),
            host: "thelio".into(),
        }
    }

    /// 01M5TKPG5WZ1T58FNAMZNP6CJ5: each wave gets the pin of its own main
    /// clone, and `has` is asked with the full reference of that pin.
    #[test]
    fn each_wave_gets_the_pin_of_its_own_main_clone() {
        let pin = |n: &str| format!("ghcr.io/o/i@sha256:{}", n.repeat(64));
        let config = |n: &str| format!("[session]\nimage = \"{}\"\n", pin(n));
        let (_one, one, _) = session_image::test_clone(&config("a"));
        let (_two, two, _) = session_image::test_clone(&config("b"));
        let folder = tempfile::tempdir().unwrap();
        crate::wave::write_waves(
            folder.path(),
            &BTreeMap::from([
                ("o/one".to_owned(), known("o/one", &one)),
                ("o/two".to_owned(), known("o/two", &two)),
            ]),
        )
        .unwrap();

        let asked = Mutex::new(Vec::new());
        let has = |image: &str| {
            asked.lock().unwrap().push(image.to_owned());
            image == pin("b")
        };
        let mut waves = vec![wave("o/one"), wave("o/two")];
        pins(folder.path(), &mut waves, has);
        let digest = |n: &str| daemons::short(&format!("sha256:{}", n.repeat(64)));
        assert_eq!(
            waves[0].image,
            Some(Image {
                digest: digest("a"),
                here: false
            })
        );
        assert_eq!(
            waves[1].image,
            Some(Image {
                digest: digest("b"),
                here: true
            })
        );
        assert_eq!(*asked.lock().unwrap(), [pin("a"), pin("b")]);
    }

    /// A main clone that has no merged pin gives no image: the host line
    /// says `no session image` for that wave.
    #[test]
    fn a_clone_with_no_merged_pin_gives_no_image() {
        let (_dir, main, _) = session_image::test_clone("[release]\n");
        let folder = tempfile::tempdir().unwrap();
        crate::wave::write_waves(
            folder.path(),
            &BTreeMap::from([("o/r".to_owned(), known("o/r", &main))]),
        )
        .unwrap();
        let mut waves = vec![wave("o/r")];
        pins(folder.path(), &mut waves, |_: &str| true);
        assert_eq!(waves[0].image, None);
        // A folder with no waves.json knows no wave.
        let empty = tempfile::tempdir().unwrap();
        pins(empty.path(), &mut waves, |_: &str| true);
        assert_eq!(waves[0].image, None);
    }
}
