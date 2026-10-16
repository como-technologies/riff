//! `riff lead start`: the lead of a repository in a container with a
//! terminal (#720, 01M4Z60B81RTZT71PN5HV0GZHX).
//!
//! # Design
//!
//! The lead is a session of the riff daemon, as a worker is, but with a
//! terminal: `podman run -d -i -t`. conmon holds the terminal, so the
//! lead runs on when the person detaches with Ctrl-P Ctrl-Q, and when
//! the daemon stops. `riff lead start` and `riff view` attach to it
//! ([`crate::runtime::Runtime::attach`]). riff 3.0 uses no tmux.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as riff lead start (the person)
//!     participant S as riff-server
//!     participant D as riff daemon
//!     participant C as container riff-lead-OWNER-REPO
//!     P->>D: status: does the lead run?
//!     P->>S: start the session of the lead
//!     P->>D: start SPEC with a terminal
//!     D->>C: podman run -d -i -t: riff relay --terminal -- claude
//!     D-->>P: done: the container runs
//!     P->>C: podman attach (Ctrl-P Ctrl-Q detaches)
//!     D->>S: keep the session of the lead alive
//! ```
//!
//! | Part | Value | Requirement |
//! |---|---|---|
//! | The container | `riff-lead-OWNER-REPO` ([`name_of`]): one lead for each repository | 01M4Z60B81RTZT71PN5HV0GZHX, 01M501P9MXNAP5QXNVBVF5JJFN |
//! | The folder | `leads/OWNER-REPO` in the data folder of riff ([`folder_of`]): `clone/` and `claude/` | 01M501P9Q51AYN9GBPWKVHA229 |
//! | The clone | `git clone --reference MAIN`, on the default branch, at the first start; a fetch and a fast-forward at each later start, when it has no local change and git can fast-forward it ([`fresh_clone`]) | 01M501P9Q51AYN9GBPWKVHA229, 01M50RRQ9DT0JPSB358EAZRP8Z, 01M51JYDHAQDJ3GH2J8DT3AZQS |
//! | The Claude folder | the first-run answers of `claude`, as for each session ([`run::claude_folder`]) | 01M501P9SDGVWG3MTVCW5BDRQQ, 01M51JACH6TQP318ZFG6NQ9GRM |
//! | The command | `claude --session-id ID --permission-mode auto START` ([`spec`]); the daemon runs it as `riff relay --terminal -- ...` | 01M501P9VN47QAKZ6B8P39ZSW4 |
//! | The riff tools | [`crate::mcp::LEAD_TOOLS`], on riff-server as the session of the lead | 01M501P9XWXWDBSXP4QRTHTZ4P |
//! | The wakes | `/opt/riff/riff relay watch` in a background task | 01M501PA04NP79RJQEDP4D7YTM |
//!
//! The clone and the Claude folder stay between the starts of the lead:
//! the lead keeps the history of its conversations, and a dialog of
//! `claude` that the person answered once. Each start is a new session
//! of riff-server and of `claude`. The other spec parts are those of a
//! worker ([`crate::run::spec`]).

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use riff_core::name::{Place, SessionUri};
use riff_core::wire::StartReason;

use crate::api::Api;
use crate::daemon::{self, Ask, Say, Status};
use crate::run::{self, Facts};
use crate::runtime::{Runtime, SessionSpec};
use crate::session_image::Image;
use crate::{identity, terminal, text};

/// The work item of the lead in the daemon.
pub const ITEM: &str = "lead";

/// The name of the container of the lead of the repository `repo`
/// (`OWNER/REPO`). Each character that podman does not take in a name
/// becomes `-`.
///
/// ```
/// assert_eq!(riff::lead::name_of("como-technologies/riff"), "riff-lead-como-technologies-riff");
/// assert_eq!(riff::lead::name_of("o/a b"), "riff-lead-o-a-b");
/// ```
pub fn name_of(repo: &str) -> String {
    format!("riff-lead-{}", plain(repo))
}

/// `repo` with `-` in place of `/` and of each character that is not a
/// letter, a digit, `_` or `.`.
fn plain(repo: &str) -> String {
    repo.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// The folder of the lead of `repo` in the data folder `data` of riff
/// (01M501P9Q51AYN9GBPWKVHA229).
///
/// ```
/// use std::path::Path;
///
/// let folder = riff::lead::folder_of(Path::new("/d/riff"), "o/r");
/// assert_eq!(folder, Path::new("/d/riff/leads/o-r"));
/// ```
pub fn folder_of(data: &Path, repo: &str) -> PathBuf {
    data.join("leads").join(plain(repo))
}

/// The [`SessionSpec`] of the lead of `repo`: the spec of a worker
/// ([`run::spec`]) with the name of the lead, and `claude` with a
/// terminal (01M501P9VN47QAKZ6B8P39ZSW4). Its first prompt is
/// [`text::lead_start`].
///
/// ```
/// use riff::run::Facts;
///
/// let facts = Facts {
///     session: "1a2b3c4d-0000-4000-8000-000000000000".into(),
///     item: "lead".into(),
///     image: "img".into(),
///     home: "/home/mike".into(),
///     user: "mike".into(),
///     clone: "/d/riff/leads/o-r/clone".into(),
///     objects: "/src/r/.git/objects".into(),
///     claude: "/d/riff/leads/o-r/claude".into(),
///     credentials: "/home/mike/.claude/.credentials.json".into(),
///     gitconfig: None,
///     gh_config: None,
/// };
/// let spec = riff::lead::spec(&facts, "o/r");
/// assert_eq!(spec.name, "riff-lead-o-r");
/// assert_eq!(spec.workdir, facts.clone);
/// assert_eq!(
///     spec.command[..5],
///     ["claude", "--session-id", facts.session.as_str(), "--permission-mode", "auto"]
/// );
/// assert!(!spec.command.iter().any(|a| a == "-p"));
/// ```
pub fn spec(facts: &Facts, repo: &str) -> SessionSpec {
    let mut spec = run::spec(facts);
    spec.name = name_of(repo);
    spec.command = vec![
        "claude".into(),
        "--session-id".into(),
        facts.session.clone(),
        "--permission-mode".into(),
        "auto".into(),
        text::lead_start(repo),
    ];
    spec
}

/// True when the daemon of `status` runs the session `name`.
///
/// ```
/// use riff::daemon::{Live, Phase, Status};
///
/// let live = Live {
///     name: "riff-lead-o-r".into(),
///     session: "s".into(),
///     item: "lead".into(),
///     image: "i".into(),
///     since_ms: 0,
///     phase: Phase::Terminal,
///     last: String::new(),
/// };
/// let status = Status {
///     pid: 1, build: "b".into(), since_ms: 0, dir: "/d".into(), waits: None, key: None,
///     riff: None, owner: None, position: 0, sessions: vec![live], limit: None,
///     waiting: vec![], waves: vec![], jobs: 0,
/// };
/// assert!(riff::lead::runs(&status, "riff-lead-o-r"));
/// assert!(!riff::lead::runs(&status, "riff-lead-o-other"));
/// ```
pub fn runs(status: &Status, name: &str) -> bool {
    status.sessions.iter().any(|s| s.name == name)
}

/// What [`fresh_clone`] did with the clone of the lead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fresh {
    /// It made the clone, on the default branch of `origin`.
    Made,
    /// It fetched `origin`, and fast-forwarded the branch to its
    /// upstream.
    Forward,
    /// The clone has a local change, or the fetch or the fast-forward
    /// failed: it stays as it is. The text says why.
    Kept(String),
}

/// Brings the clone of the lead at `clone` up to date at each start of
/// the lead (01M501P9Q51AYN9GBPWKVHA229, 01M50RRQ9DT0JPSB358EAZRP8Z).
/// When it is not there, it makes it from the main clone `main`, on the
/// default branch of `origin`. Else it fetches `origin`, and
/// fast-forwards the branch to its upstream, when the clone has no
/// local change. When git refuses the fast-forward, for example because
/// of an untracked file with the path of a new file of `origin`, the
/// clone stays as it is (01M51JYDHAQDJ3GH2J8DT3AZQS).
pub fn fresh_clone(main: &Path, clone: &Path) -> Result<Fresh> {
    if !clone.join(".git").exists() {
        let remote = run::output("git", main, &["remote", "get-url", "origin"])?;
        let top = main.to_string_lossy();
        let path = clone.to_string_lossy();
        run::output(
            "git",
            main,
            &["clone", "-q", "--reference", &top, "--", &remote, &path],
        )?;
        return Ok(Fresh::Made);
    }
    let git = |args: &[&str]| run::output("git", clone, args);
    if !git(&["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        return Ok(Fresh::Kept(text::LEAD_CLONE_CHANGED.into()));
    }
    if git(&["rev-parse", "-q", "--verify", "@{upstream}"]).is_err() {
        return Ok(Fresh::Kept(text::LEAD_CLONE_NO_UPSTREAM.into()));
    }
    if let Err(e) = git(&["fetch", "-q", "origin"]) {
        return Ok(Fresh::Kept(format!("{e:#}")));
    }
    if git(&["rev-list", "--count", "@{upstream}..HEAD"])? != "0" {
        return Ok(Fresh::Kept(text::LEAD_CLONE_AHEAD.into()));
    }
    if let Err(e) = git(&["merge", "-q", "--ff-only", "@{upstream}"]) {
        return Ok(Fresh::Kept(text::lead_clone_no_forward(&format!("{e:#}"))));
    }
    Ok(Fresh::Forward)
}

/// Attaches the terminal of this process to the lead `name` with
/// `runtime`, until a detach or the end of the lead.
pub async fn attach(runtime: &impl Runtime, name: &str) -> Result<()> {
    println!("{}", text::LEAD_DETACH);
    runtime.attach(name).await?;
    println!("{}", text::lead_detached(name));
    Ok(())
}

/// `riff lead start` in the repository of `here`: it starts the lead
/// with the daemon of this machine when it does not run, and attaches to
/// it. `api` is the sign-in of riff for the riff-server `server`. On a
/// host that is not Linux, it starts nothing.
pub async fn start(api: Api, here: &Place, server: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        start_with(&daemon::dir()?, &crate::podman::Podman, api, here, server).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (api, here, server);
        bail!("{}", text::RUN_LINUX_ONLY)
    }
}

/// [`start`] with the daemon in the folder `dir` and `runtime`.
pub async fn start_with(
    dir: &Path,
    runtime: &impl Runtime,
    api: Api,
    here: &Place,
    server: &str,
) -> Result<()> {
    let repo = here.repo_text();
    let name = name_of(&repo);
    if runs(&daemon::status(dir).await?, &name) {
        println!("{}", text::lead_runs(&repo, &name));
        return attach(runtime, &name).await;
    }
    let work = identity::working_dir()?;
    let (main, common) = run::main_clone(&work)?;
    let image = Image::of_repository(&main)?.to_string();
    daemon::image(dir, &image, |line| println!("{line}")).await?;
    let host = run::host_facts(&main)?;
    let folder = folder_of(&run::data_dir()?, &repo);
    let clone = folder.join("clone");
    let claude = folder.join("claude");
    if let Fresh::Kept(why) = fresh_clone(&main, &clone)? {
        println!("{}", text::lead_clone_kept(&clone, &why));
    }
    run::claude_folder(
        &claude,
        &clone,
        &host.home.join(".claude.json"),
        &crate::permissions::Trust::of(here.repo(), server),
    )?;

    let session = terminal::new_session_id();
    let place = Place::new(here.host(), here.repo().clone(), None)?;
    let me: SessionUri = identity::agent(&place, &session, server)?;
    let api = api.signed_in(Some(&session))?;
    api.start(&me, StartReason::Process, false).await?;
    let existing = |p: PathBuf| p.exists().then_some(p);
    let facts = Facts {
        session: session.clone(),
        item: ITEM.into(),
        image: image.clone(),
        home: host.home.clone(),
        user: host.user.clone(),
        clone,
        objects: common.join("objects"),
        claude,
        credentials: host.credentials.clone(),
        gitconfig: existing(host.home.join(".gitconfig")),
        gh_config: existing(host.home.join(".config/gh")),
    };
    let start = daemon::Start {
        session: session.clone(),
        item: ITEM.into(),
        folder,
        spec: spec(&facts, &repo),
        start: text::lead_start(&repo),
        values: vec![("GH_TOKEN".to_owned(), host.gh_token.clone())],
        secrets: std::iter::once(host.gh_token.clone())
            .chain(host.sign_in.secrets.iter().cloned())
            .collect(),
        server: server.to_owned(),
        me: me.clone(),
        terminal: true,
    };
    println!("{}", text::lead_starts(&repo, &name, &session));
    if let Err(e) = ask_start(dir, start).await {
        let _ = api.end(&me).await;
        return Err(e);
    }
    attach(runtime, &name).await
}

/// Asks the daemon in the folder `dir` to start the session of `start`
/// with a terminal, and waits until its container runs.
async fn ask_start(dir: &Path, start: daemon::Start) -> Result<()> {
    let mut client = daemon::Client::connect(dir).await?;
    client.ask(Ask::Start(Box::new(start))).await?;
    loop {
        match client.next().await? {
            Some(Say::Done) => return Ok(()),
            Some(Say::Failed { error }) => bail!("{error}"),
            Some(Say::Line { text }) => println!("{text}"),
            Some(Say::Ended(_)) | None => bail!("{}", text::DAEMON_GONE),
            Some(_) => {}
        }
    }
}
