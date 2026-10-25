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
//!   floor, the live sessions with their item, phase, last step and the
//!   digest of the image of their container, the starts that wait, and
//!   the state of each wave ([`of_status`]);
//! - from the host: the load, the free memory, the disk of the session
//!   folders, the version of podman, and the merged pin of each wave
//!   with whether podman has it ([`look`], [`pins`],
//!   01M5TKPG5WZ1T58FNAMZNP6CJ5);
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
///          "image": "i", "digest": "0123456789ab", "since_ms": 0, "phase": "works",
///          "last": "> Bash: just check"},
///         {"name": "riff-lead-o-r", "session": "l1", "item": "lead",
///          "image": "i", "since_ms": 0, "phase": "terminal"},
///     ],
///     "limit": {"sessions": 4, "floor_gb": 8, "cpus": 8, "memory_gb": 8},
///     "waiting": ["riff-issue-13-2b3c4d5e"],
///     "waves": [{"repo": "o/r", "word": "started issue-12"}],
///     "test": {"tag": "v4.1.1", "commit": "90e36bfd", "secs": 185},
///     "claude_token": "set",
/// }))?;
/// let facts = of_status(&status);
/// assert_eq!((facts.build.as_str(), facts.limit, facts.floor_gb), ("3.1.0", 4, 8));
/// assert_eq!(facts.sessions.len(), 1);
/// assert_eq!(facts.sessions[0].last, "> Bash: just check");
/// assert_eq!(facts.sessions[0].digest, "0123456789ab");
/// assert_eq!(facts.waiting, ["riff-issue-13-2b3c4d5e"]);
/// assert_eq!(facts.waves[0].word, "started issue-12");
/// assert_eq!(facts.test.as_ref().map(|t| t.secs), Some(185));
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
            })
            .collect(),
        test: status.test.clone(),
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
    let mut waves = std::mem::take(&mut facts.waves);
    let host = tokio::task::spawn_blocking(move || {
        pins(&dir, &mut waves, podman_has);
        (
            crate::machine::here(),
            crate::disk::here(&dir),
            podman(),
            waves,
            gh(&main),
            claude(crate::run::now_ms()),
            riff_client::login::stored(&server),
        )
    })
    .await?;
    let (machine, disk, podman, waves, gh, claude, stored) = host;
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

/// The version of podman on `PATH`, or `None` with no podman.
fn podman() -> Option<String> {
    let out = crate::run::output("podman", Path::new("/"), &["--version"]).ok()?;
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
    crate::run::output("podman", Path::new("/"), &["image", "exists", image]).is_ok()
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
/// let wave = |repo: &str| Wave { repo: repo.into(), word: "waits".into(), image: None };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(repo: &str) -> daemons::Wave {
        daemons::Wave {
            repo: repo.into(),
            word: "waits".into(),
            image: None,
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
