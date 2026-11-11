//! The token of each session on GitHub (01M6WPMNXN1BSBEH74TG5DDRTF,
//! 01M6WPMP035A93SM6SAJGS6MTC, 01M6Z87A04EXTT1FFJSJ1EM6TB).
//!
//! # Design
//!
//! Each session acts on GitHub with a token of a GitHub App of the riff:
//! the session App for a worker and a verifier, the lead App for the lead
//! (01M6WG52J7WTP079W1AYEQ4W08). Before the start of each container, the
//! daemon asks riff-server for the token of the session, signed in as the
//! person ([`crate::wave::World::github_token`],
//! 01M6WPMEEWX09DHJZF18BKBPYW). riff-server takes the role and the
//! repository from its own facts. The daemon asks only for a repository
//! where the `gh` of the person can push ([`can_push`]).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as riff daemon (host)
//!     participant S as riff-server
//!     participant G as GitHub
//!     participant C as the container
//!     D->>G: gh api repos/OWNER/REPO: can the person push?
//!     D->>S: POST /v1/github/token: the session (signed in as the person)
//!     S->>G: a token: one repository, the rights of the role
//!     S-->>D: the token, its bot, its role, its rights, its end
//!     D->>D: FOLDER/gh/hosts.yml, mode 0600
//!     D->>C: start: GH_CONFIG_DIR=FOLDER/gh, the folder with no write, no GH_TOKEN
//!     Note over D,C: 10 minutes before the end: a new token in the same file
//!     D->>G: at the end of the session: DELETE /installation/token
//! ```
//!
//! A token ends after one hour, and the environment of a container
//! cannot change after its start. So the token is in a file, not in a
//! variable: `gh` reads [`HOSTS`] at each call ([`write()`]). git gets the
//! token from `gh auth git-credential`: [`crate::run::spec`] sets the
//! credential helper of `https://github.com` with `GIT_CONFIG_*`. The
//! daemon binds the folder with no write, and sets [`CONFIG_VAR`] to it
//! ([`give`]). The container gets no `GH_TOKEN`, and no bind of
//! `~/.config/gh` of the host.
//!
//! | Path | Holds |
//! |---|---|
//! | `FOLDER/gh/hosts.yml` | The token and the bot, mode `0600`. |
//! | `FOLDER/gh/config.yml` | `version: "1"`: `gh` changes nothing in the folder. |
//!
//! [`renew_in`] gives the wait until [`RENEW_BEFORE`] before the end of
//! the token: then the daemon asks again, and writes the new token in the
//! same file. At the end of the session, the daemon revokes the token and
//! removes the folder ([`remove`]). After a restart, the daemon reads the
//! token of a session that runs from its file ([`read_token`]), and its
//! end from the record of the session ([`ActsAs::Bot`]).
//!
//! With the token, the daemon also gives each commit of the session its
//! author: the login of the bot and its noreply address ([`author`],
//! [`give_author`], 01M6Y6B8VBM3TKC3YHZKSA4PFZ).
//!
//! # New rights of an install
//!
//! A token keeps the rights that its install had when GitHub made it. So
//! when the owner of an account accepts new rights of an App, riff-server
//! sends the renew of the install on `GET /v1/github/renew`
//! (`riff_server::renew`, #1906). The daemon follows that stream on each
//! riff-server of its sessions and of its waves, as the person
//! ([`crate::wave::World::github_renews`], 01M74MB99RT6B9MKSBMSVD4JRG). The reply of
//! riff-server names the install of each token ([`ActsAs::Bot`]). For a
//! renew, the daemon asks at once for a new token of each session of the
//! install, the lead too, and writes it in the same file. It forgets its
//! token of the writes of the install ([`Writes::forget`]): the next write
//! asks for a new one. riff-server revokes the old tokens a minute later.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant S as riff-server
//!     participant D as riff daemon
//!     participant C as the container of the lead
//!     D->>S: GET /v1/github/renew (as the person)
//!     S-->>D: renew: the install 2020
//!     D->>S: POST /v1/github/token: the lead (a token of the install 2020)
//!     S-->>D: a token with the new rights
//!     D->>C: FOLDER/gh/hosts.yml: the new token
//!     Note over D: a worker of the install 1010 keeps its token
//! ```
//!
//! # No token of the person
//!
//! No session gets the token of the person (01M6Z87A04EXTT1FFJSJ1EM6TB).
//! riff keeps no `gh auth token` for a session, and the daemon
//! refuses to start a container whose env names one of [`TOKEN_VARS`]
//! ([`check_env`]): that check is the last step before each start. When
//! riff-server gives no token of an App ([`ActsAs::NoToken`]), because
//! the riff has no App of the role or no sign-in, the App is not on the
//! repository, the `gh` of the person cannot push there, or riff-server
//! is older:
//!
//! - no container of a worker or of a verify starts, and the wave says
//!   why ([`crate::wave`]);
//! - the lead starts with no token on GitHub: the folder of its token
//!   has no [`HOSTS`] ([`write_none`]), and its riff tools work, so it
//!   tells the person the fix.
//!
//! The reason names the fix, for example `riff github install OWNER`,
//! and a prompt for `claude` on the host ([`crate::text::gh_repair`],
//! 01M6Z87JTXW0Y99X08VHA2SSSR). A session of `riff run --no-github`
//! needs no GitHub ([`ActsAs::NoGitHub`], 01M6Z87JY83V3KR72E2AG8V8KG):
//! the daemon asks riff-server for nothing.
//!
//! ```mermaid
//! flowchart LR
//!     A["the answer of riff-server"] --> T{"a token of an App?"}
//!     T -- yes --> B["FOLDER/gh/hosts.yml: the session acts as the bot"]
//!     T -- no --> R{"the lead?"}
//!     R -- yes --> L["the lead starts with no token on GitHub<br/>its first prompt ends with the fix and the prompt"]
//!     R -- "no: a worker or a verify" --> N["no container starts<br/>the wave tells the lead why"]
//! ```
//!
//! # The writes of the daemon
//!
//! The daemon itself writes on GitHub with a token of the lead App
//! (01M6XZJFE2ZTR5RE3AWM30R704): the comment of the tokens of a claim,
//! the comment `Merged in`, the result of a verify, the issue of a broken
//! main, and the delete of an old branch `riff-kept`. So GitHub shows the
//! bot of the lead App, not the person. [`Writes`] keeps one token for
//! each repository, from riff-server ([`crate::wave::World::daemon_token`],
//! 01M6XZJFGNVG6B7DF5VQ3H1DNX), and gives it to the `gh` of each such
//! step ([`writer`]). That `gh` makes each write with the token of the
//! App, and each read with the token of the person
//! ([`crate::pr::Gh::writing_as`]).
//!
//! ```mermaid
//! flowchart LR
//!     W["a write step of the daemon"] --> K{"a token of the lead App<br/>for the repository,<br/>10 minutes or more left?"}
//!     K -- yes --> A["the write as riff-OWNER-lead[bot]"]
//!     K -- no --> S["POST /v1/github/daemon<br/>(signed in as the person)"]
//!     S -- "a token" --> A
//!     S -- "none, and why" --> P["the write with the token of the person<br/>riff daemon status says why"]
//! ```
//!
//! The daemon asks only for a repository where the `gh` of the person can
//! push, as for a session. After an answer with no token, it asks again
//! after [`NONE_AGAIN`], and after a fault after [`RETRY`]. `riff daemon
//! status` names each repository with the bot of its writes, or with the
//! token of the person and why ([`Writes::shown`]).

use std::collections::BTreeMap;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use riff_core::wire::{GitHubRole, Rights, SessionToken};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::runtime::{Bind, Env, SessionSpec, Source};
use crate::wave::World;

/// The folder of the token in the folder of a session.
pub const FOLDER: &str = "gh";

/// The file of the token in [`FOLDER`].
pub const HOSTS: &str = "hosts.yml";

/// The config of `gh` in [`FOLDER`].
pub const CONFIG: &str = "config.yml";

/// The variable that names the folder of the config of `gh`.
pub const CONFIG_VAR: &str = "GH_CONFIG_DIR";

/// The variables where `gh` reads a token. No container gets one
/// (01M6Z87A04EXTT1FFJSJ1EM6TB).
pub const TOKEN_VARS: [&str; 2] = ["GH_TOKEN", "GITHUB_TOKEN"];

/// The daemon asks for a new token this long before the end of the old
/// one (01M6WPMNXN1BSBEH74TG5DDRTF).
pub const RENEW_BEFORE: Duration = Duration::from_secs(10 * 60);

/// The wait before the next ask, after an ask that failed, or for a
/// token whose end does not read.
pub const RETRY: Duration = Duration::from_secs(60);

/// The wait before the daemon asks again after an answer with no token
/// of an App: for its writes on a repository (01M6XZJFE2ZTR5RE3AWM30R704),
/// and for a lead with no token (01M6YC3T0TXANJH6YMTJZR1HCR). The wave
/// starts no item of a repository for this long after a worker got no
/// token (01M6Z87A04EXTT1FFJSJ1EM6TB).
pub const NONE_AGAIN: Duration = Duration::from_secs(10 * 60);

/// How a session acts on GitHub. It holds no token, so the record of the
/// session keeps it, and `riff whoami` shows it
/// (01M6WPMWYSW98MQYQCCJ54ZX04). No session acts with the token of the
/// person (01M6Z87A04EXTT1FFJSJ1EM6TB).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "as", rename_all = "snake_case")]
pub enum ActsAs {
    /// With a token of a GitHub App of the riff.
    Bot {
        /// The login of the bot, for example `riff-acme[bot]`.
        bot: String,
        /// The role of the session.
        role: GitHubRole,
        /// The rights of the token.
        rights: Rights,
        /// The end of the token, as GitHub gives it:
        /// `2026-11-08T13:00:00Z`.
        ends: String,
        /// The ID of the install of the App that gave the token: a renew
        /// of that install asks for a new token at once (01M74MB99RT6B9MKSBMSVD4JRG).
        /// `None` from an older riff-server.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        installation: Option<u64>,
    },
    /// With no token on GitHub at all: riff-server gave no token of an
    /// App (01M6Z87A04EXTT1FFJSJ1EM6TB), or cannot read the keys of the
    /// GitHub Apps (01M6YC3T0TXANJH6YMTJZR1HCR). Only a lead starts so.
    NoToken {
        /// Why, with the fix and the prompt for `claude` on the host
        /// (01M6Z87JTXW0Y99X08VHA2SSSR).
        why: String,
    },
    /// With no GitHub: a session of `riff run --no-github`, which needs
    /// none (01M6Z87JY83V3KR72E2AG8V8KG). The daemon asks for no token.
    NoGitHub,
}

impl ActsAs {
    /// True when the token is of the install `installation`: a renew of
    /// that install asks for a new token (01M74MB99RT6B9MKSBMSVD4JRG).
    ///
    /// ```
    /// use riff::daemon::gh_token::ActsAs;
    /// use riff_core::wire::GitHubRole;
    ///
    /// let bot = |installation| ActsAs::Bot {
    ///     bot: "riff-acme-lead[bot]".into(),
    ///     role: GitHubRole::Lead,
    ///     rights: Default::default(),
    ///     ends: "2026-11-08T13:00:00Z".into(),
    ///     installation,
    /// };
    /// assert!(bot(Some(2020)).of_install(2020));
    /// assert!(!bot(Some(1010)).of_install(2020));
    /// assert!(!bot(None).of_install(2020));
    /// assert!(!ActsAs::NoGitHub.of_install(2020));
    /// ```
    pub fn of_install(&self, installation: u64) -> bool {
        matches!(self, ActsAs::Bot { installation: Some(id), .. } if *id == installation)
    }
}

/// Why a session acts with no token of an App: the why of
/// [`ActsAs::NoToken`], [`crate::text::GH_RUN_NO_GITHUB`] for
/// [`ActsAs::NoGitHub`], else [`crate::text::GH_TOKEN_NONE`].
///
/// ```
/// use riff::daemon::gh_token::{ActsAs, why_none};
///
/// assert_eq!(why_none(&ActsAs::NoToken { why: "no keys".into() }), "no keys");
/// assert_eq!(why_none(&ActsAs::NoGitHub), riff::text::GH_RUN_NO_GITHUB);
/// ```
pub fn why_none(acts: &ActsAs) -> String {
    match acts {
        ActsAs::NoToken { why } => why.clone(),
        ActsAs::NoGitHub => crate::text::GH_RUN_NO_GITHUB.to_owned(),
        ActsAs::Bot { .. } => crate::text::GH_TOKEN_NONE.to_owned(),
    }
}

/// A token of a session, or no token and why. It has no `Debug`: no log
/// shows the token.
#[derive(Clone)]
pub struct Grant {
    /// The token of the App. `None` with no token.
    pub token: Option<Zeroizing<String>>,
    /// How the session acts with it.
    pub acts: ActsAs,
    /// The user ID of the bot, when riff-server has it: the email of a
    /// commit ([`author`]).
    pub bot_id: Option<u64>,
}

impl Grant {
    /// No token on GitHub at all, with why (01M6Z87A04EXTT1FFJSJ1EM6TB).
    pub fn none(why: &str) -> Grant {
        Grant {
            token: None,
            acts: ActsAs::NoToken {
                why: why.to_owned(),
            },
            bot_id: None,
        }
    }

    /// The grant of a session of `riff run --no-github`: no GitHub
    /// (01M6Z87JY83V3KR72E2AG8V8KG).
    pub fn no_github() -> Grant {
        Grant {
            token: None,
            acts: ActsAs::NoGitHub,
            bot_id: None,
        }
    }

    /// The grant of the reply `reply` of riff-server: its token, or no
    /// token with the problem of the reply (01M6Z87A04EXTT1FFJSJ1EM6TB).
    /// With `no_github`, riff-server cannot read the keys of the Apps
    /// (01M6YC3T0TXANJH6YMTJZR1HCR): its problem has the fix and the
    /// prompt already.
    ///
    /// ```
    /// use riff::daemon::gh_token::{ActsAs, Grant};
    /// use riff_core::wire::{GitHubRole, SessionToken};
    ///
    /// let reply = SessionToken {
    ///     role: GitHubRole::Verifier,
    ///     repo: "acme/app".into(),
    ///     bot: Some("riff-acme[bot]".into()),
    ///     bot_id: Some(1101),
    ///     token: Some("ghs_abcd".into()),
    ///     rights: [("contents".to_owned(), "read".to_owned())].into(),
    ///     ends: Some("2026-11-08T13:00:00Z".into()),
    ///     installation: Some(1010),
    ///     problem: None,
    ///     no_github: false,
    /// };
    /// let grant = Grant::of(reply.clone());
    /// assert_eq!(grant.token.as_deref().map(String::as_str), Some("ghs_abcd"));
    /// assert!(matches!(grant.acts, ActsAs::Bot { role: GitHubRole::Verifier, installation: Some(1010), .. }));
    /// assert_eq!(grant.bot_id, Some(1101));
    ///
    /// let why = "riff-acme is not installed on acme/app: riff github install acme";
    /// let missing = SessionToken { token: None, problem: Some(why.into()), ..reply.clone() };
    /// let grant = Grant::of(missing);
    /// assert!(grant.token.is_none());
    /// assert_eq!(grant.acts, ActsAs::NoToken { why: why.into() });
    ///
    /// let why = "riff-server cannot read the keys of the GitHub Apps";
    /// let paused = SessionToken { token: None, problem: Some(why.into()), no_github: true, ..reply };
    /// let grant = Grant::of(paused);
    /// assert!(grant.token.is_none());
    /// assert_eq!(grant.acts, ActsAs::NoToken { why: why.into() });
    /// ```
    pub fn of(reply: SessionToken) -> Grant {
        match (reply.token, reply.problem) {
            (Some(token), _) => Grant {
                token: Some(Zeroizing::new(token)),
                acts: ActsAs::Bot {
                    bot: reply.bot.unwrap_or_default(),
                    role: reply.role,
                    rights: reply.rights,
                    ends: reply.ends.unwrap_or_default(),
                    installation: reply.installation,
                },
                bot_id: reply.bot_id,
            },
            (None, Some(problem)) => Grant::none(&problem),
            (None, None) if reply.no_github => Grant::none(crate::text::GH_NONE),
            (None, None) => Grant::none(crate::text::GH_TOKEN_NONE),
        }
    }

    /// The same grant, whose why with no token ends with the prompt for
    /// `claude` on the host for the repository `repo`
    /// ([`crate::text::gh_repair`], 01M6Z87JTXW0Y99X08VHA2SSSR). A token
    /// stays as it is.
    ///
    /// ```
    /// use riff::daemon::gh_token::{ActsAs, Grant};
    ///
    /// let why = "riff-acme is not installed on acme/app: riff github install acme";
    /// let grant = Grant::none(why).repaired("acme/app");
    /// assert_eq!(grant.acts, ActsAs::NoToken { why: riff::text::gh_repair("acme/app", why) });
    /// ```
    #[must_use]
    pub fn repaired(self, repo: &str) -> Grant {
        match self.acts {
            ActsAs::NoToken { why } => Grant {
                acts: ActsAs::NoToken {
                    why: crate::text::gh_repair(repo, &why),
                },
                ..self
            },
            _ => self,
        }
    }
}

/// The `hosts.yml` of `gh` with the token `token` of the bot `bot`: the
/// form of `gh` with one account.
///
/// ```
/// let text = riff::daemon::gh_token::hosts_yml("riff-acme[bot]", "ghs_abcd");
/// assert!(text.starts_with("github.com:\n"));
/// assert!(text.contains("    oauth_token: ghs_abcd\n"));
/// assert!(text.contains("    user: \"riff-acme[bot]\"\n"));
/// assert!(text.contains("        \"riff-acme[bot]\":\n            oauth_token: ghs_abcd\n"));
/// ```
pub fn hosts_yml(bot: &str, token: &str) -> String {
    let bot = bot.replace(['"', '\\', '\n'], "");
    format!(
        "github.com:\n    git_protocol: https\n    users:\n        \"{bot}\":\n            \
         oauth_token: {token}\n    oauth_token: {token}\n    user: \"{bot}\"\n"
    )
}

/// The `config.yml` of `gh`: its version, so that `gh` changes no file of
/// the folder, and HTTPS for git.
pub const CONFIG_YML: &str = "version: \"1\"\ngit_protocol: https\n";

/// The folder of the token of the session whose folder is `folder`.
pub fn dir(folder: &Path) -> PathBuf {
    folder.join(FOLDER)
}

/// Writes the token `token` of the bot `bot` in the folder of the token of
/// the session `folder`: the folder with mode `0700`, each file with mode
/// `0600`, through a new file and a rename. So a `gh` in the container
/// reads the old file or the new one, never a part. It gives the folder.
///
/// ```
/// use std::os::unix::fs::PermissionsExt;
/// use riff::daemon::gh_token::{read_token, remove, write};
///
/// let folder = tempfile::tempdir()?;
/// let dir = write(folder.path(), "riff-acme[bot]", "ghs_one")?;
/// let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
/// assert_eq!(mode(&dir), 0o700);
/// assert_eq!(mode(&dir.join("hosts.yml")), 0o600);
/// assert_eq!(mode(&dir.join("config.yml")), 0o600);
/// write(folder.path(), "riff-acme[bot]", "ghs_two")?;
/// assert_eq!(read_token(folder.path()).as_deref().map(String::as_str), Some("ghs_two"));
/// remove(folder.path());
/// assert!(!dir.exists());
/// assert!(read_token(folder.path()).is_none());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn write(folder: &Path, bot: &str, token: &str) -> Result<PathBuf> {
    write_hosts(folder, Some(&Zeroizing::new(hosts_yml(bot, token))))
}

/// Writes the folder of the token of the session `folder` with no token
/// (01M6YC3T0TXANJH6YMTJZR1HCR, 01M6Z87A04EXTT1FFJSJ1EM6TB): [`CONFIG`]
/// and no [`HOSTS`]. `gh` there signs in to nothing, and git gets no
/// password. The folder stays, so a bind of it shows a later token of
/// [`write()`]. It gives the folder.
///
/// ```
/// use riff::daemon::gh_token::{read_token, write, write_none};
///
/// let folder = tempfile::tempdir()?;
/// write(folder.path(), "riff-acme-lead[bot]", "ghs_one")?;
/// let dir = write_none(folder.path())?;
/// assert!(dir.is_dir());
/// assert!(dir.join("config.yml").is_file());
/// assert!(!dir.join("hosts.yml").exists());
/// assert!(read_token(folder.path()).is_none());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn write_none(folder: &Path) -> Result<PathBuf> {
    write_hosts(folder, None)
}

/// Writes [`CONFIG`], and [`HOSTS`] with the text `hosts`, in the folder
/// of the token of the session `folder`. See [`write()`]. With no
/// `hosts`, it removes [`HOSTS`].
fn write_hosts(folder: &Path, hosts: Option<&str>) -> Result<PathBuf> {
    let dir = dir(folder);
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot make {}", dir.display()))?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("cannot set the mode of {}", dir.display()))?;
    let mut files = vec![(CONFIG, CONFIG_YML)];
    match hosts {
        Some(hosts) => files.push((HOSTS, hosts)),
        None => match std::fs::remove_file(dir.join(HOSTS)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                bail!("cannot remove {}: {e}", dir.join(HOSTS).display())
            }
            _ => {}
        },
    }
    for (name, text) in files {
        let path = dir.join(name);
        let new = dir.join(format!("{name}.new"));
        let _ = std::fs::remove_file(&new);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&new)
            .with_context(|| format!("cannot write {}", new.display()))?;
        std::io::Write::write_all(&mut file, text.as_bytes())
            .with_context(|| format!("cannot write {}", new.display()))?;
        std::fs::rename(&new, &path).with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(dir)
}

/// The token in the folder of the token of the session `folder`, when it
/// has one.
pub fn read_token(folder: &Path) -> Option<Zeroizing<String>> {
    let text = Zeroizing::new(std::fs::read_to_string(dir(folder).join(HOSTS)).ok()?);
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("oauth_token:"))
        .map(str::trim)
        .find(|token| !token.is_empty())
        .map(|token| Zeroizing::new(token.to_owned()))
}

/// Removes the folder of the token of the session `folder`.
pub fn remove(folder: &Path) {
    let _ = std::fs::remove_dir_all(dir(folder));
}

/// Gives the container of `spec` the folder of the token `dir`: the bind
/// with no write at the same path, and [`CONFIG_VAR`] in place of each
/// one of `spec` and `values`.
///
/// ```
/// use riff::daemon::gh_token::give;
/// use riff::runtime::{Bind, Env, SessionSpec, Source};
///
/// let mut spec = SessionSpec {
///     name: "riff-issue-12-1a2b3c4d".into(),
///     image: "img".into(),
///     home: "/home/mike".into(),
///     workdir: "/w".into(),
///     binds: vec![],
///     env: vec![Env::Host("GH_CONFIG_DIR".into()), Env::Value("USER".into(), "mike".into())],
///     memory: String::new(),
///     cpus: 0,
///     command: vec![],
/// };
/// let mut values = vec![("GH_CONFIG_DIR".to_owned(), "/home/mike/.config/gh".to_owned())];
/// give(&mut spec, &mut values, std::path::Path::new("/s/issue-12/gh"));
/// assert!(values.is_empty());
/// assert!(!spec.env.contains(&Env::Host("GH_CONFIG_DIR".into())));
/// assert!(spec.env.contains(&Env::Value("GH_CONFIG_DIR".into(), "/s/issue-12/gh".into())));
/// assert!(spec.binds.contains(&Bind {
///     source: Source::Path("/s/issue-12/gh".into()),
///     target: "/s/issue-12/gh".into(),
///     write: false,
/// }));
/// assert!(spec.check_values(&values).is_ok());
/// ```
pub fn give(spec: &mut SessionSpec, values: &mut Vec<(String, String)>, dir: &Path) {
    spec.env
        .retain(|e| !matches!(e, Env::Host(name) | Env::Value(name, _) if name == CONFIG_VAR));
    values.retain(|(name, _)| name != CONFIG_VAR);
    spec.env.push(Env::Value(
        CONFIG_VAR.into(),
        dir.to_string_lossy().into_owned(),
    ));
    spec.binds.push(Bind {
        source: Source::Path(dir.to_owned()),
        target: dir.to_owned(),
        write: false,
    });
}

/// Refuses the spec `spec` of a container whose env names one of
/// [`TOKEN_VARS`]: no container gets the token of the person
/// (01M6Z87A04EXTT1FFJSJ1EM6TB). The daemon checks each spec with it as
/// the last step before the start of its container.
///
/// ```
/// use riff::daemon::gh_token::check_env;
/// use riff::runtime::{Env, SessionSpec};
///
/// let spec = |env: Vec<Env>| SessionSpec {
///     name: "riff-issue-12-1a2b3c4d".into(),
///     image: "img".into(),
///     home: "/home/mike".into(),
///     workdir: "/w".into(),
///     binds: vec![],
///     env,
///     memory: String::new(),
///     cpus: 0,
///     command: vec![],
/// };
/// assert!(check_env(&spec(vec![Env::Value("GH_CONFIG_DIR".into(), "/s/gh".into())])).is_ok());
/// let error = check_env(&spec(vec![Env::Host("GH_TOKEN".into())])).unwrap_err();
/// assert_eq!(error.to_string(), riff::text::daemon_person_token("riff-issue-12-1a2b3c4d", "GH_TOKEN"));
/// let value = Env::Value("GITHUB_TOKEN".into(), "gho_x".into());
/// assert!(check_env(&spec(vec![value])).is_err());
/// ```
pub fn check_env(spec: &SessionSpec) -> Result<()> {
    for env in &spec.env {
        let (Env::Host(name) | Env::Value(name, _)) = env;
        if TOKEN_VARS.contains(&name.as_str()) {
            bail!("{}", crate::text::daemon_person_token(&spec.name, name));
        }
    }
    Ok(())
}

/// The author and the committer of each commit of a session with the
/// token of the bot `bot`: the login of the bot as the name
/// (01M6Y6B8VBM3TKC3YHZKSA4PFZ), and its noreply address with its user
/// ID `bot_id` ([`crate::pr::noreply`], 01M6WZ2C8JXEA888XYMRXTS17V).
/// GitHub links the commit to the App. The trailer `Riff-Session:` of
/// each commit names the session. The squash message of `riff verify
/// pass` keeps the person as a co-author ([`crate::pr::squash`]).
///
/// ```
/// use riff::daemon::gh_token::author;
///
/// assert_eq!(
///     author("riff-como-technologies[bot]", Some(1101)),
///     (
///         "riff-como-technologies[bot]".to_owned(),
///         "1101+riff-como-technologies[bot]@users.noreply.github.com".to_owned()
///     )
/// );
/// let (name, email) = author("riff-acme-lead[bot]", None);
/// assert_eq!(name, "riff-acme-lead[bot]");
/// assert_eq!(email, "riff-acme-lead[bot]@users.noreply.github.com");
/// ```
pub fn author(bot: &str, bot_id: Option<u64>) -> (String, String) {
    (bot.to_owned(), crate::pr::noreply(bot_id, bot))
}

/// Gives each commit of the container of `spec` the author `name` and
/// `email` of [`author`]: `user.name` and `user.email` with
/// `GIT_CONFIG_*` ([`crate::run::git_config`]). They win over
/// `~/.gitconfig`.
///
/// ```
/// use riff::daemon::gh_token::give_author;
/// use riff::runtime::{Env, SessionSpec};
///
/// let mut spec = SessionSpec {
///     name: "riff-issue-12-1a2b3c4d".into(),
///     image: "img".into(),
///     home: "/home/mike".into(),
///     workdir: "/w".into(),
///     binds: vec![],
///     env: vec![],
///     memory: String::new(),
///     cpus: 0,
///     command: vec![],
/// };
/// riff::run::git_config(&mut spec.env, "commit.gpgsign", "false");
/// give_author(&mut spec, "riff-acme[bot]", "1101+riff-acme[bot]@users.noreply.github.com");
/// let value = |n: &str, v: &str| Env::Value(n.into(), v.into());
/// assert!(spec.env.contains(&value("GIT_CONFIG_COUNT", "3")));
/// assert!(spec.env.contains(&value("GIT_CONFIG_KEY_1", "user.name")));
/// assert!(spec.env.contains(&value("GIT_CONFIG_VALUE_1", "riff-acme[bot]")));
/// assert!(spec.env.contains(&value("GIT_CONFIG_KEY_2", "user.email")));
/// ```
pub fn give_author(spec: &mut SessionSpec, name: &str, email: &str) {
    crate::run::git_config(&mut spec.env, "user.name", name);
    crate::run::git_config(&mut spec.env, "user.email", email);
}

/// The wait from `now_secs` until [`RENEW_BEFORE`] before the end `ends`
/// of a token: none when that time is past, and [`RETRY`] when the end
/// does not read.
///
/// ```
/// use std::time::Duration;
/// use riff::daemon::gh_token::{RETRY, renew_in};
///
/// let now = riff::daemon::sweep::date_seconds("2026-11-08T12:00:00Z").unwrap();
/// assert_eq!(renew_in("2026-11-08T13:00:00Z", now), Duration::from_secs(50 * 60));
/// assert_eq!(renew_in("2026-11-08T12:05:00Z", now), Duration::ZERO);
/// assert_eq!(renew_in("later", now), RETRY);
/// ```
pub fn renew_in(ends: &str, now_secs: i64) -> Duration {
    let Some(end) = crate::daemon::sweep::date_seconds(ends) else {
        return RETRY;
    };
    let before = i64::try_from(RENEW_BEFORE.as_secs()).unwrap_or(i64::MAX);
    let wait = end.saturating_sub(before).saturating_sub(now_secs);
    Duration::from_secs(u64::try_from(wait).unwrap_or(0))
}

/// The seconds since 1970 now.
pub fn now_secs() -> i64 {
    crate::daemon::now_ms() / 1000
}

/// True when the `gh` of the person can push to `repo`
/// (`gh api repos/OWNER/REPO --jq .permissions.push`).
pub fn can_push(gh: &crate::pr::Gh, repo: &str) -> Result<bool> {
    let path = format!("repos/{repo}");
    let out = gh.run_as(
        &format!("the read of the rights of the person on {repo}"),
        &["api", &path, "--jq", ".permissions.push"],
    )?;
    Ok(out.trim() == "true")
}

/// Revokes the token `token` on GitHub with `gh`
/// (`DELETE /installation/token`), with the token in the environment of
/// `gh` only.
pub fn revoke(gh: &crate::pr::Gh, token: &str) -> Result<()> {
    gh.clone().with_given_token(token).run_as(
        "the revoke of the token of a session",
        &["api", "--method", "DELETE", "installation/token"],
    )?;
    Ok(())
}

/// How the daemon writes on GitHub in each repository
/// (01M6XZJFE2ZTR5RE3AWM30R704): the token of the lead App that
/// riff-server gave, or the token of the person and why. It holds each
/// token in memory only, and keeps no file.
#[derive(Default)]
pub struct Writes {
    /// The grant of each repository, by `OWNER/NAME`.
    kept: Mutex<BTreeMap<String, Kept>>,
    /// One ask to riff-server at a time: two writes that need a token ask
    /// one time.
    asking: tokio::sync::Mutex<()>,
}

/// The grant of the writes on one repository.
struct Kept {
    grant: Grant,
    /// The riff-server that gave it. Empty when the daemon knew none.
    server: String,
    /// The second since 1970 from which the daemon asks again.
    again: i64,
}

impl Writes {
    /// The `gh` `gh` for a step of the daemon on `repo`: its writes go
    /// with the token of the lead App of the riff-server `server`, and its
    /// reads keep their token ([`crate::pr::Gh::writing_as`]). The daemon
    /// asks `world` for a token when it has none with [`RENEW_BEFORE`] or
    /// more left, and after [`NONE_AGAIN`] or [`RETRY`] when it had
    /// none. With no `server`, it asks the riff-server of the last ask for
    /// `repo`. With no token, `gh` stays as it is: the token of the person.
    pub async fn gh(
        &self,
        world: &dyn World,
        server: Option<&str>,
        repo: &str,
        gh: crate::pr::Gh,
    ) -> crate::pr::Gh {
        let grant = self.grant(world, server, repo).await;
        match (&grant.token, &grant.acts) {
            (Some(token), ActsAs::Bot { bot, .. }) => gh.writing_as(bot, token),
            _ => gh,
        }
    }

    /// The grant for the writes on `repo`: the one it keeps, or a new one
    /// of `world`.
    async fn grant(&self, world: &dyn World, server: Option<&str>, repo: &str) -> Grant {
        if let Some(grant) = self.fresh(server, repo) {
            return grant;
        }
        let _one = self.asking.lock().await;
        if let Some(grant) = self.fresh(server, repo) {
            return grant;
        }
        let server = server
            .map(str::to_owned)
            .or_else(|| self.lock().get(repo).map(|kept| kept.server.clone()))
            .filter(|server| !server.is_empty());
        let now = now_secs();
        let (grant, again) = match &server {
            None => (Grant::none(crate::text::GH_NO_SERVER), now),
            Some(server) => match world.daemon_token(server, repo).await {
                Ok(grant) => {
                    let wait = match &grant.acts {
                        ActsAs::Bot { ends, .. } => renew_in(ends, now),
                        ActsAs::NoToken { .. } | ActsAs::NoGitHub => NONE_AGAIN,
                    };
                    (grant, now.saturating_add(secs(wait)))
                }
                Err(e) => {
                    let why = crate::text::gh_daemon_token_failed(&format!("{e:#}"));
                    (Grant::none(&why), now.saturating_add(secs(RETRY)))
                }
            },
        };
        let kept = Kept {
            grant: grant.clone(),
            server: server.unwrap_or_default(),
            again,
        };
        let old = self.lock().insert(repo.to_owned(), kept);
        if old.is_none_or(|old| !same_writer(&old.grant.acts, &grant.acts)) {
            println!(
                "riff daemon: {}",
                crate::text::daemon_writes(repo, &grant.acts)
            );
        }
        grant
    }

    /// The grant of `repo` that it keeps, while the daemon need not ask
    /// again, and when it is of `server`.
    fn fresh(&self, server: Option<&str>, repo: &str) -> Option<Grant> {
        let kept = self.lock();
        let kept = kept.get(repo)?;
        let of_server = server.is_none_or(|server| server == kept.server);
        (of_server && kept.again > now_secs()).then(|| kept.grant.clone())
    }

    /// How the daemon wrote last on each repository: `riff daemon status`
    /// shows it.
    pub fn shown(&self) -> Vec<crate::daemon::WritesShown> {
        self.lock()
            .iter()
            .map(|(repo, kept)| crate::daemon::WritesShown {
                repo: repo.clone(),
                acts: kept.grant.acts.clone(),
            })
            .collect()
    }

    /// Forgets the token of each repository that the riff-server `server`
    /// gave from the install `installation`, and gives each such
    /// repository: the next write there asks for a new token
    /// (01M74MB99RT6B9MKSBMSVD4JRG). `riff daemon status` still shows how the daemon wrote
    /// last.
    ///
    /// ```
    /// use riff::daemon::gh_token::Writes;
    ///
    /// let writes = Writes::default();
    /// assert!(writes.forget("https://riff.example", 2020).is_empty());
    /// ```
    pub fn forget(&self, server: &str, installation: u64) -> Vec<String> {
        let mut kept = self.lock();
        let mut forgot = Vec::new();
        for (repo, kept) in kept.iter_mut() {
            if kept.server == server && kept.grant.acts.of_install(installation) {
                kept.again = 0;
                forgot.push(repo.clone());
            }
        }
        forgot
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Kept>> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// True when `a` and `b` write as the same bot, or both with the token of
/// the person for the same reason: the end of a token does not count.
fn same_writer(a: &ActsAs, b: &ActsAs) -> bool {
    match (a, b) {
        (ActsAs::Bot { bot: a, .. }, ActsAs::Bot { bot: b, .. }) => a == b,
        (a, b) => a == b,
    }
}

/// The seconds of `wait`.
fn secs(wait: Duration) -> i64 {
    i64::try_from(wait.as_secs()).unwrap_or(i64::MAX)
}

/// The `gh` `gh` whose writes on `repo` go as the daemon of `world`
/// ([`Writes::gh`]): as it is when `world` has no [`Writes`].
pub async fn writer(
    world: &dyn World,
    server: Option<&str>,
    repo: &str,
    gh: crate::pr::Gh,
) -> crate::pr::Gh {
    match world.writes() {
        Some(writes) => writes.gh(world, server, repo, gh).await,
        None => gh,
    }
}
