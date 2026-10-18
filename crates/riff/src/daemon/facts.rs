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
//!   floor, the live sessions with their item, phase and last step, the
//!   starts that wait, and the state of each wave ([`of_status`]);
//! - from the host: the load, the free memory, the disk of the session
//!   folders, the version of podman, and the session image that the
//!   repository pins ([`look`]);
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
//! ```mermaid
//! sequenceDiagram
//!     participant D as riff daemon on thelio
//!     participant S as riff-server
//!     participant L as lead on pangolin
//!     loop each 30 s
//!         D->>D: the status, the host, the sign-ins
//!         D->>S: machine: "daemon facts: {...}"
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

use super::Status;
use crate::session_image;
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

/// The time between two looks: [`daemons::EVERY`], or the milliseconds
/// of [`EVERY_VAR`].
pub fn every() -> Duration {
    std::env::var(EVERY_VAR)
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(daemons::EVERY, Duration::from_millis)
}

/// The facts of the status of the daemon: the build, the limit, the
/// sessions, the starts that wait and the waves. The lead counts in no
/// limit: it is not a session of the facts.
///
/// ```
/// use riff::daemon::facts::of_status;
///
/// let status: riff::daemon::Status = serde_json::from_value(serde_json::json!({
///     "pid": 7, "build": "3.1.0", "since_ms": 0, "dir": "/d",
///     "sessions": [
///         {"name": "riff-issue-12-1a2b3c4d", "session": "1a2b3c4d", "item": "issue-12",
///          "image": "i", "since_ms": 0, "phase": "works", "last": "> Bash: just check"},
///         {"name": "riff-lead-o-r", "session": "l1", "item": "lead",
///          "image": "i", "since_ms": 0, "phase": "terminal"},
///     ],
///     "limit": {"sessions": 4, "floor_gb": 8, "cpus": 8, "memory_gb": 8},
///     "waiting": ["riff-issue-13-2b3c4d5e"],
///     "waves": [{"repo": "o/r", "word": "started issue-12"}],
///     "claude_token": "set",
/// }))?;
/// let facts = of_status(&status);
/// assert_eq!((facts.build.as_str(), facts.limit, facts.floor_gb), ("3.1.0", 4, 8));
/// assert_eq!(facts.sessions.len(), 1);
/// assert_eq!(facts.sessions[0].last, "> Bash: just check");
/// assert_eq!(facts.waiting, ["riff-issue-13-2b3c4d5e"]);
/// assert_eq!(facts.waves[0].word, "started issue-12");
/// assert_eq!(facts.claude_token, riff_client::daemons::Token::Set);
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn of_status(status: &Status) -> Facts {
    let limit = status.limit.unwrap_or_default();
    Facts {
        build: status.build.clone(),
        limit: limit.sessions,
        floor_gb: limit.floor_gb,
        sessions: status
            .sessions
            .iter()
            .filter(|live| live.phase != riff_client::sessions::Phase::Terminal)
            .map(|live| daemons::Session {
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
            })
            .collect(),
        claude_token: status.claude_token,
        ..Facts::default()
    }
}

/// The facts of the daemon in the folder `dir` and of its host, for the
/// repository of `wave`. Each step that can block runs on a thread for
/// blocking work. A fact that riff cannot read stays empty.
pub async fn look(dir: &Path, wave: &Wave, api: &Api) -> anyhow::Result<Facts> {
    let mut facts = of_status(&super::status(dir).await?);
    let (dir, main) = (dir.to_owned(), wave.main.clone());
    let server = wave.server.clone();
    let host = tokio::task::spawn_blocking(move || {
        (
            crate::machine::here(),
            crate::disk::here(&dir),
            podman(),
            image(&main),
            gh(&main),
            claude(crate::run::now_ms()),
            riff_client::login::stored(&server),
        )
    })
    .await?;
    let (machine, disk, podman, image, gh, claude, stored) = host;
    facts.machine = Some(machine);
    facts.disk = disk;
    facts.podman = podman;
    facts.image = image;
    facts.gh = gh;
    facts.claude = claude;
    facts.riff = match api.has_sign_in().await {
        Ok(false) => Sign::On,
        Ok(true) => riff_of(stored),
        Err(_) => Sign::Unknown,
    };
    Ok(facts)
}

/// The version of podman on `PATH`, or `None` with no podman.
fn podman() -> Option<String> {
    let out = crate::run::output("podman", Path::new("/"), &["--version"]).ok()?;
    Some(out.rsplit(' ').next().unwrap_or(&out).to_owned())
}

/// The session image that the main clone `main` pins, and whether
/// podman has it. `None` when the clone pins none.
fn image(main: &Path) -> Option<Image> {
    let image = session_image::Image::of_repository(main).ok()?;
    let here = crate::run::output(
        "podman",
        Path::new("/"),
        &["image", "exists", &image.to_string()],
    )
    .is_ok();
    Some(Image {
        name: image.name().to_owned(),
        here,
    })
}

/// The sign-in of `gh` in the main clone `main`: it gives a token, or
/// not.
fn gh(main: &Path) -> Sign {
    match crate::run::output("gh", main, &["auth", "token"]) {
        Ok(token) if !token.is_empty() => Sign::On,
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
