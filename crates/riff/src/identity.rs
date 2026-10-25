//! Works out the session URI from the agent tool, the user, the machine
//! and git.
//!
//! # Rules
//!
//! | Part | Source, in order |
//! |---|---|
//! | user | `RIFF_USER`, then the sign-in at the server (see [`crate::login`]), then `USER`. A keyring error stops the command (R157, R158). |
//! | session | `RIFF_SESSION`, then the ID of the live `riff mcp` of the agent session (see [`crate::local`]), then `CLAUDE_CODE_SESSION_ID`. A person has none. |
//! | host | `RIFF_HOST`, then `cloud` in a cloud session, then the machine name without its domain. |
//! | owner/repo | The `origin` remote. Without a remote: `local/<main worktree directory>`. |
//! | worktree | The directory name of a linked worktree. The main worktree has none. |
//!
//! A fault of `git` is no source: the command fails and names the
//! reason of git (see [`place_in`]).
//!
//! After an update, a long run gives its place to its new binary in
//! [`PLACE_ARG`]. The new binary takes host, owner/repo and worktree
//! from there, not from the rules above (see [`here`]).
//!
//! Outside git, the repository part is `-` and the worktree part is the
//! directory name. Each part goes through
//! [`riff_core::name::sanitize`].
//!
//! Claude Code gives the session ID to each process that it starts for
//! a session: `riff mcp` and a `riff watch` in a background task get it
//! in the environment, and the hooks get it on stdin (see
//! [`agent_session`]). So they all find the same session, in any
//! directory (R57).
//!
//! `/clear` gives the session a new ID, but `riff mcp` keeps its process
//! and the old ID. So the ID that `riff mcp` recorded comes before the
//! ID from Claude Code. The session keeps one ID for its life (R58,
//! R167).
//!
//! A person on the command line has no session ID. The URI of a person
//! is `riff://USER@HOST` (R65). The directory still gives the default
//! thread.

use std::ffi::OsString;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use riff_core::name::{Place, Repo, SessionUri, Who, sanitize};

use crate::local;
use riff_client::login;

/// The environment variables that hold the session ID, in order.
pub const SESSION_VARS: [&str; 2] = ["RIFF_SESSION", "CLAUDE_CODE_SESSION_ID"];

/// The session ID of this process, if it has one.
pub fn session_id() -> Option<String> {
    agent_session(std::env::var(SESSION_VARS[1]).ok())
}

/// The session ID of an agent session whose agent tool gives the ID
/// `given`: `RIFF_SESSION`, then the ID that the live `riff mcp` of the
/// session recorded, then `given` (R167). A hook passes the ID from its
/// stdin.
pub fn agent_session(given: Option<String>) -> Option<String> {
    pick_session(std::env::var(SESSION_VARS[0]).ok(), given, || {
        local::dir().and_then(|dir| local::recorded_above(&dir))
    })
}

/// The session ID from `RIFF_SESSION`, the ID that `riff mcp` recorded,
/// and the ID of the agent tool, in that order. Only an agent session
/// has a record, so `recorded` runs only when the agent tool gives an
/// ID. An empty ID counts as none.
///
/// ```
/// use riff::identity::pick_session;
///
/// let s = |id: &str| Some(id.to_owned());
/// assert_eq!(pick_session(s("mine"), s("new"), || s("old")), s("mine"));
/// assert_eq!(pick_session(None, s("new"), || s("old")), s("old"));
/// assert_eq!(pick_session(None, s("new"), || None), s("new"));
/// assert_eq!(pick_session(s(""), s("new"), || None), s("new"));
/// assert_eq!(pick_session(None, None, || s("old")), None);
/// ```
pub fn pick_session(
    riff_session: Option<String>,
    given: Option<String>,
    recorded: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let set = |id: Option<String>| id.filter(|id| !id.is_empty());
    set(riff_session).or_else(|| {
        let given = set(given)?;
        Some(set(recorded()).unwrap_or(given))
    })
}

/// The URI of the caller: an agent session when there is a session ID,
/// otherwise a person. `place` is where the caller works. `server` is
/// the riff-server, for the user of its sign-in.
pub fn me(place: &Place, server: &str) -> Result<SessionUri> {
    match session_id() {
        Some(id) => agent(place, &id, server),
        None => Ok(SessionUri::new(
            Who::new(&user(server)?, None)?,
            Place::host_only(place.host())?,
        )),
    }
}

/// The URI of the agent session `id` at `place`. A hook uses it, with
/// the ID from [`agent_session`].
pub fn agent(place: &Place, id: &str, server: &str) -> Result<SessionUri> {
    Ok(SessionUri::new(
        Who::new(&user(server)?, Some(&sanitize(id)))?,
        place.clone(),
    ))
}

/// The URI of the person at `place`, with no session, also inside an
/// agent session. The wrapper of a worker uses it
/// (01M493YZVZGA7TSRJH6F67VN0H).
pub fn person(place: &Place, server: &str) -> Result<SessionUri> {
    Ok(SessionUri::new(
        Who::new(&user(server)?, None)?,
        place.clone(),
    ))
}

/// The user for `server`: from `RIFF_USER`, the sign-in, or `USER`.
/// Only a missing `RIFF_USER` makes it read the keyring. A keyring error
/// stops it, also when riff cannot open the keyring: `USER` stands in
/// only when there is no sign-in (R157, R158).
fn user(server: &str) -> Result<String> {
    let riff_user = std::env::var("RIFF_USER").ok();
    let signed_in = match riff_user {
        Some(_) => None,
        None => login::user(server)
            .context("riff cannot find your user. Unlock the keyring, or set RIFF_USER")?,
    };
    pick_user(
        riff_user.as_deref(),
        signed_in.as_deref(),
        std::env::var("USER").ok().as_deref(),
    )
}

/// The user from `RIFF_USER`, the user of the sign-in, and `USER`, in
/// that order (R36). Without a sign-in, `USER` stands in until the
/// server checks tokens. The caller passes no sign-in only when there
/// is none, never when the keyring failed (R157).
///
/// ```
/// use riff::identity::pick_user;
///
/// assert_eq!(pick_user(None, Some("mike"), Some("lovelace")).unwrap(), "mike");
/// assert_eq!(pick_user(Some("brett"), Some("mike"), None).unwrap(), "brett");
/// assert_eq!(pick_user(None, None, Some("Lovelace")).unwrap(), "lovelace");
/// assert!(pick_user(None, None, None).is_err());
/// ```
pub fn pick_user(
    riff_user: Option<&str>,
    signed_in: Option<&str>,
    os_user: Option<&str>,
) -> Result<String> {
    let user = riff_user
        .or(signed_in)
        .or(os_user)
        .context("set RIFF_USER. At a riff with sign-in, you can run riff login instead")?;
    Ok(sanitize(&user.to_lowercase()))
}

/// The URI of an agent session. It fails when there is no session ID.
pub fn session(place: &Place, server: &str) -> Result<SessionUri> {
    let me = me(place, server)?;
    if me.who().session().is_none() {
        bail!(
            "no session ID: run this inside Claude Code, or set {}",
            SESSION_VARS[0]
        );
    }
    Ok(me)
}

/// The hidden argument that gives a new binary the place of the process
/// that ran it after an update (01M3NJGD45GF7Y4CZWQ7GRDHZN). Only riff
/// gives it. A process that the new binary starts does not inherit it.
pub const PLACE_ARG: &str = "--place";

/// The place of this process: `given` in [`PLACE_ARG`], else the place
/// of the working directory. So a long run keeps its repository and
/// worktree over an update, also when its directory is gone.
pub fn here(given: Option<&Place>) -> Result<Place> {
    match given {
        Some(place) => Ok(place.clone()),
        None => place(&working_dir()?),
    }
}

/// `place` as the text of [`PLACE_ARG`]: `HOST/OWNER/REPO#WORKTREE`, or
/// `HOST/-` outside git. [`place_from_text`] reads it back.
///
/// ```
/// use riff::identity::{place_from_text, place_text};
/// use riff_core::name::{Place, Repo};
///
/// let repo = Repo::Git { owner: "acme".into(), name: "alpha".into() };
/// for place in [
///     Place::new("thelio", repo.clone(), Some("issue-12"))?,
///     Place::new("thelio", repo, None)?,
///     Place::new("thelio", Repo::None, Some("scratch"))?,
///     Place::host_only("thelio")?,
/// ] {
///     let text = place_text(&place);
///     assert_eq!(place_from_text(&text)?, place, "{text}");
/// }
/// assert_eq!(place_text(&place_from_text("thelio/acme/alpha#issue-12")?), "thelio/acme/alpha#issue-12");
/// assert!(place_from_text("thelio").is_err());
/// assert!(place_from_text("thelio/acme").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn place_text(place: &Place) -> String {
    let worktree = place
        .worktree()
        .map(|w| format!("#{w}"))
        .unwrap_or_default();
    format!("{}/{}{worktree}", place.host(), place.repo_text())
}

/// The place in the text of [`place_text`].
pub fn place_from_text(text: &str) -> Result<Place> {
    let (rest, worktree) = match text.split_once('#') {
        Some((rest, worktree)) => (rest, Some(worktree)),
        None => (text, None),
    };
    let Some((host, repo)) = rest.split_once('/') else {
        bail!("no repository part");
    };
    let repo = match repo {
        "-" => Repo::None,
        repo => {
            let Some((owner, name)) = repo.split_once('/') else {
                bail!("the repository is not OWNER/REPO");
            };
            Repo::Git {
                owner: owner.into(),
                name: name.into(),
            }
        }
    };
    Ok(Place::new(host, repo, worktree)?)
}

/// The working directory of this process. When riff cannot read it, for
/// example because another process removed it, the error names it: see
/// [`working_dir_error`] (01M3NJGD6H8DNVHHHG80F9YFCE).
pub fn working_dir() -> Result<PathBuf> {
    std::env::current_dir().map_err(|e| working_dir_error(e, std::env::var_os("PWD")))
}

/// The error of [`working_dir`] for `error`, with the directory from
/// `PWD`.
///
/// ```
/// use std::io::Error;
/// use riff::identity::working_dir_error;
///
/// // ENOENT, as getcwd gives it for a removed directory.
/// let gone = || Error::from_raw_os_error(2);
/// let e = working_dir_error(gone(), Some("/src/riff/.claude/worktrees/w".into()));
/// let text = format!("{e:#}");
/// assert!(text.starts_with("riff cannot read its working directory \
///     /src/riff/.claude/worktrees/w. Change to a directory that exists"), "{text}");
/// assert!(text.contains("No such file or directory"), "{text}");
/// let e = working_dir_error(gone(), None);
/// assert!(format!("{e:#}").starts_with("riff cannot read its working directory. Change"));
/// ```
pub fn working_dir_error(error: std::io::Error, pwd: Option<OsString>) -> anyhow::Error {
    let dir = pwd
        .filter(|p| !p.is_empty())
        .map(|p| format!(" {}", p.to_string_lossy()))
        .unwrap_or_default();
    anyhow::Error::new(error).context(format!(
        "riff cannot read its working directory{dir}. Change to a directory that exists"
    ))
}

/// The place for `dir`, with the host from the environment.
pub fn place(dir: &Path) -> Result<Place> {
    place_in(dir, &this_host())
}

/// The host of this machine, from the environment: see [`host`].
pub fn this_host() -> String {
    host(
        std::env::var("RIFF_HOST").ok().as_deref(),
        std::env::var(REMOTE_VAR).ok().as_deref(),
        &gethostname::gethostname().to_string_lossy(),
    )
}

/// Claude Code sets this variable to `true` in a cloud session (R100).
pub const REMOTE_VAR: &str = "CLAUDE_CODE_REMOTE";

/// The host from `RIFF_HOST`, the value of [`REMOTE_VAR`] and the machine
/// name. A cloud session has a random container name, so its host is
/// `cloud` (R42).
///
/// ```
/// use riff::identity::host;
///
/// assert_eq!(host(None, None, "Pangolin.local"), "pangolin");
/// assert_eq!(host(None, Some("true"), "a1b2c3d4e5"), "cloud");
/// assert_eq!(host(Some("brett"), Some("true"), "a1b2c3d4e5"), "brett");
/// ```
pub fn host(riff_host: Option<&str>, remote: Option<&str>, machine: &str) -> String {
    match (riff_host, remote) {
        (Some(host), _) => host.to_owned(),
        (None, Some("true")) => "cloud".to_owned(),
        _ => short_host(machine),
    }
}

/// True when `dir` is the top of a git worktree, not a dir in a
/// repository above it (01M49JW9Y8SNT3J242SF646DF4).
///
/// ```
/// let dir = isolated::outside_git();
/// assert!(!riff::identity::is_top(dir.path()));
/// std::process::Command::new("git").arg("init").arg("-q").arg(dir.path()).status()?;
/// assert!(riff::identity::is_top(dir.path()));
/// std::fs::create_dir(dir.path().join("sub"))?;
/// assert!(!riff::identity::is_top(&dir.path().join("sub")));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn is_top(dir: &Path) -> bool {
    let Some(top) = git(dir, &["rev-parse", "--show-toplevel"]) else {
        return false;
    };
    match (std::fs::canonicalize(top), std::fs::canonicalize(dir)) {
        (Ok(top), Ok(dir)) => top == dir,
        _ => false,
    }
}

/// The main worktree of the repository of `dir`: the first worktree
/// that `git worktree list` names. `None` outside git.
pub fn main_worktree(dir: &Path) -> Option<std::path::PathBuf> {
    let list = git(dir, &["worktree", "list", "--porcelain"])?;
    let first = list.lines().next()?.strip_prefix("worktree ")?;
    Some(first.into())
}

/// The common git directory of the repository of `dir`, as git gives
/// it: the `.git` of the main clone, also in a linked worktree. `None`
/// when git knows no repository there.
pub fn common_dir(dir: &Path) -> Option<std::path::PathBuf> {
    let args = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
    git(dir, &args).map(Into::into)
}

/// The place for `dir` on a known host.
///
/// riff never guesses a place (01M5R9S69W7QEMNKV3JRNV0RNB). It asks git, and git answers
/// in one of three ways: a repository, no repository, or a fault. Only
/// an answer of git makes a place. A fault of git, after [`GIT_TRIES`]
/// tries, is an error of the command, with the reason of git
/// ([`reading`]). So a session never registers at a place that a failed
/// `git` call made up, and `riff lead --session ID` finds it (#1401).
pub fn place_in(dir: &Path, host: &str) -> Result<Place> {
    let (repo, worktree) = match asked(dir, &["rev-parse", "--show-toplevel"], &NOT_GIT)? {
        Some(top) => (
            repo_of(dir, &top)?,
            linked_worktree(dir)?.then(|| base_name(&top)),
        ),
        None => (Repo::None, Some(base_name(&dir.to_string_lossy()))),
    };
    Ok(Place::new(
        &sanitize(host),
        repo,
        worktree.as_deref().map(sanitize).as_deref(),
    )?)
}

/// `pangolin.local` becomes `pangolin`.
fn short_host(host: &str) -> String {
    host.split('.').next().unwrap_or(host).to_lowercase()
}

/// OWNER/REPO from the `origin` remote. Without a remote, the owner is
/// `local` and the repo is the directory name of the main worktree.
fn repo_of(dir: &Path, top: &str) -> Result<Repo> {
    let remote = asked(dir, &["remote", "get-url", "origin"], &NO_REMOTE)?;
    if let Some((owner, name)) = remote.and_then(|url| parse_remote(&url)) {
        return Ok(Repo::Git {
            owner: sanitize(&owner),
            name: sanitize(&name),
        });
    }
    let common = asked(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        &[],
    )?;
    let main = common
        .as_deref()
        .and_then(|common| Path::new(common).parent())
        .map_or_else(|| top.to_owned(), |p| p.to_string_lossy().into_owned());
    Ok(Repo::Git {
        owner: "local".into(),
        name: sanitize(&base_name(&main)),
    })
}

/// True when `dir` is in a linked worktree, not the main one.
fn linked_worktree(dir: &Path) -> Result<bool> {
    let git_dir = asked(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-dir"],
        &[],
    )?;
    let common = asked(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        &[],
    )?;
    Ok(matches!((git_dir, common), (Some(a), Some(b)) if a != b))
}

/// What git says, with `LC_ALL=C`, when `dir` is outside git: no
/// repository, no work tree, or no such directory.
const NOT_GIT: [&str; 3] = [
    "not a git repository",
    "this operation must be run in a work tree",
    "cannot change to",
];

/// What git says when the repository has no remote `origin`.
const NO_REMOTE: [&str; 1] = ["No such remote"];

/// The tries of one `git` call that decides the place. A fault of git
/// that passes, for example a failed fork on a busy machine, passes
/// within them.
pub const GIT_TRIES: u32 = 3;

/// The wait between two tries of [`GIT_TRIES`].
const GIT_WAIT: Duration = Duration::from_millis(100);

/// `git -C dir ARGS` for the place: the text, or `None` when git says
/// that the thing is not there (`absent`). A fault of git is tried
/// [`GIT_TRIES`] times, then it is an error that names the call and the
/// reason ([`reading`]).
fn asked(dir: &Path, args: &[&str], absent: &[&str]) -> Result<Option<String>> {
    let mut tried = 0;
    loop {
        tried += 1;
        let run = Command::new("git")
            .env("LC_ALL", "C")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output();
        match reading(run, absent) {
            Ok(text) => return Ok(text),
            Err(_) if tried < GIT_TRIES => std::thread::sleep(GIT_WAIT),
            Err(why) => bail!(
                "riff cannot read git in {}: `git {}` {why}. riff does not guess where \
                 you work. Run the command again.",
                dir.display(),
                args.join(" ")
            ),
        }
    }
}

/// What one `git` call that decides the place gave: the text of its
/// output, `None` when git is not there or says that the thing is not
/// there, or the reason of a fault.
///
/// - Git gave output with exit code 0: the text, trimmed. An empty
///   output is `None`.
/// - Git failed, and its message has a phrase of `absent`: `None`. So
///   a directory outside git has no place in a repository, and a
///   repository with no `origin` is local.
/// - No `git` program: `None`. riff works outside git.
/// - Each other failure is a fault, with a reason: git could not start,
///   a signal stopped it, or it failed with another message.
///
/// ```
/// use std::os::unix::process::ExitStatusExt;
/// use std::process::{ExitStatus, Output};
/// use riff::identity::reading;
///
/// let ran = |code: i32, out: &str, err: &str| -> std::io::Result<Output> {
///     Ok(Output {
///         status: ExitStatus::from_raw(code << 8),
///         stdout: out.as_bytes().to_vec(),
///         stderr: err.as_bytes().to_vec(),
///     })
/// };
/// let outside = ["not a git repository"];
/// assert_eq!(reading(ran(0, "/src/riff\n", ""), &outside), Ok(Some("/src/riff".into())));
/// assert_eq!(reading(ran(0, "", ""), &outside), Ok(None));
/// let fatal = "fatal: not a git repository (or any of the parent directories): .git";
/// assert_eq!(reading(ran(128, "", fatal), &outside), Ok(None));
/// // A fault is no answer: riff does not make up a place from it.
/// let why = reading(ran(128, "", "fatal: unable to read config"), &outside).unwrap_err();
/// assert_eq!(why, "failed with exit code 128: fatal: unable to read config");
/// let killed = Ok(Output {
///     status: ExitStatus::from_raw(9),
///     stdout: Vec::new(),
///     stderr: Vec::new(),
/// });
/// assert_eq!(reading(killed, &outside).unwrap_err(), "was stopped by signal 9");
/// let busy = Err(std::io::Error::from_raw_os_error(11));
/// assert!(reading(busy, &outside).unwrap_err().starts_with("did not start:"));
/// let no_git = Err(std::io::Error::from(std::io::ErrorKind::NotFound));
/// assert_eq!(reading(no_git, &outside), Ok(None));
/// ```
pub fn reading(
    run: std::io::Result<Output>,
    absent: &[&str],
) -> std::result::Result<Option<String>, String> {
    let out = match run {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("did not start: {e}")),
    };
    if out.status.success() {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        return Ok((!text.is_empty()).then_some(text));
    }
    let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    if absent.iter().any(|phrase| said.contains(phrase)) {
        return Ok(None);
    }
    Err(match (out.status.code(), out.status.signal()) {
        (Some(code), _) => format!("failed with exit code {code}: {said}"),
        (None, Some(signal)) => format!("was stopped by signal {signal}"),
        (None, None) => "failed".to_owned(),
    })
}

/// OWNER and REPO from a remote URL, for example
/// `https://github.com/como-technologies/riff.git` or
/// `git@github.com:como-technologies/riff.git`.
///
/// ```
/// use riff::identity::parse_remote;
///
/// let expected = Some(("como-technologies".to_owned(), "riff".to_owned()));
/// assert_eq!(parse_remote("https://github.com/como-technologies/riff.git"), expected);
/// assert_eq!(parse_remote("git@github.com:como-technologies/riff.git"), expected);
/// assert_eq!(parse_remote("riff"), None);
/// ```
pub fn parse_remote(url: &str) -> Option<(String, String)> {
    let path = url.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = path.rsplit(['/', ':']);
    let name = parts.next().filter(|s| !s.is_empty())?;
    let owner = parts.next().filter(|s| !s.is_empty())?;
    Some((owner.to_owned(), name.to_owned()))
}

/// OWNER and REPO from a remote URL of a repository on GitHub, in each
/// form that git takes: `https://github.com/O/R.git`,
/// `git@github.com:O/R.git` or `ssh://git@github.com/O/R`. A URL of
/// another host gives `None`, so does a path of a folder.
///
/// ```
/// use riff::identity::parse_github_remote;
///
/// let expected = Some(("como-technologies".to_owned(), "riff".to_owned()));
/// assert_eq!(parse_github_remote("https://github.com/como-technologies/riff.git"), expected);
/// assert_eq!(parse_github_remote("git@github.com:como-technologies/riff.git"), expected);
/// assert_eq!(parse_github_remote("ssh://git@github.com/como-technologies/riff"), expected);
/// assert_eq!(parse_github_remote("https://gitlab.com/como-technologies/riff.git"), None);
/// assert_eq!(parse_github_remote("/home/mike/github.com/riff"), None);
/// assert_eq!(parse_github_remote("../riff"), None);
/// ```
pub fn parse_github_remote(url: &str) -> Option<(String, String)> {
    let host = "github.com";
    let start = url.find(host)?;
    let (before, after) = (&url[..start], &url[start + host.len()..]);
    let named = before.ends_with("//") || before.ends_with('@');
    if named && after.starts_with(['/', ':']) {
        parse_remote(url)
    } else {
        None
    }
}

/// `OWNER/REPO` of the remote `origin` of `dir`, when it is a repository
/// on GitHub ([`parse_github_remote`]). `None` outside git, with no
/// remote `origin`, and with a remote on another host.
pub fn github_origin(dir: &Path) -> Option<String> {
    let url = git(dir, &["remote", "get-url", "origin"])?;
    let (owner, name) = parse_github_remote(&url)?;
    Some(format!("{owner}/{name}"))
}

fn base_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| "root".into(), |n| n.to_string_lossy().into_owned())
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// The variable that marks a worker session.
pub const WORKER: &str = "RIFF_WORKER";

/// True when this process runs in a worker session: `RIFF_WORKER=1`.
///
/// ```
/// assert!(riff::identity::is_worker_value(Some("1")));
/// assert!(!riff::identity::is_worker_value(Some("0")));
/// assert!(!riff::identity::is_worker_value(None));
/// ```
pub fn is_worker() -> bool {
    is_worker_value(std::env::var(WORKER).ok().as_deref())
}

/// [`is_worker`] for the value of `RIFF_WORKER`.
pub fn is_worker_value(value: Option<&str>) -> bool {
    value == Some("1")
}

/// A new riff session ID for a worker: a random UUID (version 4).
///
/// ```
/// let id = riff::identity::new_session_id();
/// assert_eq!(id.len(), 36);
/// assert_eq!(id.as_bytes()[14], b'4');
/// assert_ne!(id, riff::identity::new_session_id());
/// ```
pub fn new_session_id() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("the OS gives random bytes");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_working_dir_error_keeps_its_cause_and_skips_an_empty_pwd() {
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let e = working_dir_error(denied, Some("".into()));
        assert_eq!(
            e.to_string(),
            "riff cannot read its working directory. Change to a directory that exists"
        );
        let cause = e.downcast_ref::<std::io::Error>().unwrap();
        assert_eq!(cause.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn remotes_parse_in_both_forms() {
        for url in [
            "https://github.com/como-technologies/riff.git",
            "https://github.com/como-technologies/riff",
            "git@github.com:como-technologies/riff.git",
            "ssh://git@github.com/como-technologies/riff/",
        ] {
            assert_eq!(
                parse_remote(url),
                Some(("como-technologies".into(), "riff".into())),
                "{url}"
            );
        }
        assert_eq!(parse_remote("riff"), None);
    }

    #[test]
    fn host_names_lose_their_domain() {
        assert_eq!(short_host("Pangolin.local"), "pangolin");
    }

    #[test]
    fn only_a_true_remote_flag_gives_the_cloud_host() {
        assert_eq!(host(None, Some("false"), "pangolin"), "pangolin");
        assert_eq!(host(None, Some(""), "pangolin"), "pangolin");
        assert_eq!(host(None, Some("true"), "pangolin"), "cloud");
    }
}
