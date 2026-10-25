//! `riff lead start`: the lead of a repository in a container with a
//! terminal (#720, 01M4Z60B81RTZT71PN5HV0GZHX).
//!
//! # Design
//!
//! The lead is a session of the riff daemon, as a worker is, but with a
//! terminal: `podman run -d -i -t`. conmon holds the terminal, so the
//! lead runs on when the person detaches with Ctrl-P Ctrl-Q, and when
//! the daemon stops. `riff lead start` and `riff view` attach to it
//! ([`crate::runtime::Runtime::attach`]).//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as riff lead start (the person)
//!     participant S as riff-server
//!     participant D as riff daemon
//!     participant C as container riff-lead-OWNER-REPO
//!     P->>D: status: does the lead run? Does the wave run?
//!     opt no wave of the repository runs
//!         P->>D: run the wave of the repository
//!     end
//!     P->>S: start the session of the lead
//!     P->>D: start SPEC with a terminal
//!     D->>C: podman run -d -i -t: riff relay --terminal -- claude
//!     D-->>P: done: the container runs
//!     P->>C: podman exec stty: another size, so claude draws again
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
//! | The command | `claude --session-id ID --dangerously-skip-permissions START` ([`spec`]); the daemon runs it as `riff relay --terminal -- ...` | 01M501P9VN47QAKZ6B8P39ZSW4, 01M5GG2X5F1HXCQJ2JJWV6305K |
//! | The riff tools | [`crate::tools::LEAD_TOOLS`], on riff-server as the session of the lead | 01M501P9XWXWDBSXP4QRTHTZ4P |
//! | The riff commands | `RIFF_SOCKET` and no `RIFF_SERVER`: `riff wave list` and `riff release check` reach riff-server through the daemon ([`crate::reach`]) | 01M5GVPNAKAPA596K9361HSAAV |
//! | The riff command | `riff` is the riff of the container: the relay puts `/opt/riff` first on the `PATH` of `claude` ([`crate::session_relay::riff_first`]), so `! riff who` works in the terminal of the lead. The spec sets no `PATH` of its own, and `/opt/riff/riff` stays for the permission rules | 01M5QVCVXWBBG1BMGRVBQS25W9 |
//! | The wakes | `/opt/riff/riff relay watch --follow` in the Monitor tool, started once | 01M501PA04NP79RJQEDP4D7YTM, 01M59235BBXEKPZBRH23C39574 |
//! | The attach | [`crate::runtime::Runtime::redraw`], then [`crate::runtime::Runtime::attach`] ([`attach`]): `claude` draws its screen at each attach | 01M51PD7M04HZR2PVVJA4WCBK5 |
//! | After the attach | [`Left`]: a detach, or the end of the lead when its container does not run; the line says which | 01M51PD7Q24N252V66NFEHY49N |
//! | The sign-in of `claude` | the token of `riff claude-token` only ([`needs_token`]); a lead from an older riff with the sign-in file of the host gets a word when the host replaced that file ([`stale`], [`crate::daemon::Daemon::stale_leads`]) | 01M5NJ87NXF6077Y9W32840BR8, 01M5NJ87YEHHGJBANM7CRYV56Z |
//! | The wave | the wave of the repository on the daemon, when none runs: the lead in a container has no rollout ([`crate::wave::ask`]); a failed ask stops no lead | 01M53NGVE839TRH7XB7CDPQNZE, 01M5515CJ7QDJ639VGR7SJTE08 |
//!
//! The clone and the Claude folder stay between the starts of the lead:
//! the lead keeps the history of its conversations, and a dialog of
//! `claude` that the person answered once. Each start is a new session
//! of riff-server and of `claude`. The other spec parts are those of a
//! worker ([`crate::run::spec`]).

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use riff_core::name::{Place, SessionUri};
use riff_core::wire::{SessionInfo, StartReason};

use crate::daemon::{self, Ask, Say, Status};
use crate::run::{self, Facts};
use crate::runtime::{Runtime, SessionSpec};
use crate::{identity, session_image, text};
use riff_client::api::Api;

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
/// terminal (01M501P9VN47QAKZ6B8P39ZSW4). The lead runs with
/// `--dangerously-skip-permissions`, and a worker keeps
/// `--permission-mode auto` (01M5GG2X5F1HXCQJ2JJWV6305K). Its first
/// prompt is [`text::lead_start`].
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
///     sign_in: riff::run::ClaudeSignIn::File("/home/mike/.claude/.credentials.json".into()),
///     gitconfig: None,
///     gh_config: None,
///     setup: Default::default(),
/// };
/// let spec = riff::lead::spec(&facts, "o/r", &riff::text::lead_start("o/r"));
/// assert_eq!(spec.name, "riff-lead-o-r");
/// assert_eq!(spec.workdir, facts.clone);
/// assert_eq!(
///     spec.command[..4],
///     ["claude", "--session-id", facts.session.as_str(), "--dangerously-skip-permissions"]
/// );
/// assert!(!spec.command.iter().any(|a| a == "--permission-mode"));
/// assert!(!spec.command.iter().any(|a| a == "-p"));
/// assert!(spec.env.iter().any(|e| matches!(e, riff::runtime::Env::Value(n, v) if n == "RIFF_CHECK" && v == "/home/mike/.riff-check")));
/// // 01M5QVCVXWBBG1BMGRVBQS25W9: the spec leaves the `PATH` of the image
/// // as it is. The relay puts `/opt/riff` first on it, so `! riff` in the
/// // lead runs the riff of the container.
/// assert!(!spec.env.iter().any(|e| matches!(e, riff::runtime::Env::Value(n, _) if n == "PATH")));
/// ```
pub fn spec(facts: &Facts, repo: &str, start: &str) -> SessionSpec {
    let mut spec = run::spec(facts);
    spec.name = name_of(repo);
    // The folder `~/.cache` of the container can belong to root, when a
    // volume of `riff.toml` binds under it: `riff build` needs another
    // one.
    spec.env.push(crate::runtime::Env::Value(
        crate::check::VAR.into(),
        spec.home.join(".riff-check").to_string_lossy().into_owned(),
    ));
    spec.command = vec![
        "claude".into(),
        "--session-id".into(),
        facts.session.clone(),
        "--dangerously-skip-permissions".into(),
        start.to_owned(),
    ];
    spec
}

/// True when the daemon of `status` runs the session `name`.
///
/// ```
/// use riff::daemon::Status;
/// use riff_client::sessions::{Live, Phase};
///
/// let live = Live {
///     name: "riff-lead-o-r".into(),
///     session: "s".into(),
///     item: "lead".into(),
///     image: "i".into(),
///     since_ms: 0,
///     phase: Phase::Terminal,
///     last: String::new(),
///     lead: true,
/// };
/// let status = Status {
///     pid: 1, build: "b".into(), since_ms: 0, dir: "/d".into(), sessions: vec![live],
///     limit: None, waiting: vec![], waves: vec![], jobs: 0, test: None, mcp: vec![],
///     claude_token: Default::default(), watches: vec![], lost: vec![],
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

/// How an attach to the lead ended (01M51PD7Q24N252V66NFEHY49N).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    /// The person detached with Ctrl-P Ctrl-Q: the lead runs on.
    Detached,
    /// The lead ended, for example after `/exit`: its container does
    /// not run.
    Ended,
}

impl Left {
    /// The line for the person after the attach to the lead `name`.
    ///
    /// ```
    /// use riff::lead::Left;
    ///
    /// assert_eq!(Left::Detached.line("n"), riff::text::lead_detached("n"));
    /// assert_eq!(Left::Ended.line("n"), riff::text::lead_ended("n"));
    /// ```
    pub fn line(self, name: &str) -> String {
        match self {
            Left::Detached => text::lead_detached(name),
            Left::Ended => text::lead_ended(name),
        }
    }
}

/// Refuses a lead with the sign-in file of `claude` of the host
/// (01M5NJ87NXF6077Y9W32840BR8): the lead runs for hours, and the next
/// sign-in of `claude` on the host replaces that file. Only the token of
/// `riff claude-token` lives as long as the lead.
///
/// ```
/// use riff::run::ClaudeSignIn;
///
/// assert!(riff::lead::needs_token(&ClaudeSignIn::Token).is_ok());
/// let file = ClaudeSignIn::File("/home/mike/.claude/.credentials.json".into());
/// let refused = riff::lead::needs_token(&file).unwrap_err().to_string();
/// assert_eq!(refused, riff::text::LEAD_NEEDS_TOKEN);
/// ```
pub fn needs_token(sign_in: &run::ClaudeSignIn) -> Result<()> {
    match sign_in {
        run::ClaudeSignIn::Token => Ok(()),
        run::ClaudeSignIn::File(_) => bail!("{}", text::LEAD_NEEDS_TOKEN),
    }
}

/// The sign-in file of `claude` in a session with the home `home`: the
/// target of the bind of [`run::ClaudeSignIn::File`].
///
/// ```
/// let path = riff::lead::credentials_of(std::path::Path::new("/home/mike"));
/// assert_eq!(path, std::path::Path::new("/home/mike/.claude/.credentials.json"));
/// ```
pub fn credentials_of(home: &Path) -> PathBuf {
    home.join(".claude").join(".credentials.json")
}

/// True when the lead `name` holds a sign-in file of `claude` that the
/// host replaced (01M5NJ87YEHHGJBANM7CRYV56Z): in the container, the
/// file of the bind has no link. `home` is the home of the person, and
/// of the container. A lead with the token has no such bind: the file
/// is in its Claude folder, with a link, or is not there.
pub async fn stale(runtime: &impl Runtime, name: &str, home: &Path) -> bool {
    matches!(runtime.links(name, &credentials_of(home)).await, Ok(0))
}

/// Attaches the terminal of this process to the lead `name` with
/// `runtime`, until a detach or the end of the lead. A lead with a
/// stale sign-in gets the line of [`text::lead_sign_in_stale`] first
/// ([`stale`]). Then it asks the
/// runtime to make the lead draw its terminal again
/// (01M51PD7M04HZR2PVVJA4WCBK5). When that fails, it prints why, and
/// attaches. After the attach, a lead whose container does not run
/// ended (01M51PD7Q24N252V66NFEHY49N). When the runtime cannot say, it
/// counts as a detach.
pub async fn attach(runtime: &impl Runtime, name: &str) -> Result<Left> {
    if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty())
        && stale(runtime, name, Path::new(&home)).await
    {
        println!("{}", text::lead_sign_in_stale(name));
    }
    println!("{}", text::LEAD_DETACH);
    if let Err(e) = runtime.redraw(name).await {
        println!("{}", text::lead_no_redraw(&format!("{e:#}")));
    }
    runtime.attach(name).await?;
    Ok(match runtime.running(name).await {
        Ok(false) => Left::Ended,
        Ok(true) | Err(_) => Left::Detached,
    })
}

/// [`attach`], then the line of [`Left::line`]. After the end of the
/// lead, it also says that the `claude --resume` line of `claude` does
/// not work on the host.
async fn attach_here(runtime: &impl Runtime, name: &str) -> Result<()> {
    let left = attach(runtime, name).await?;
    println!("{}", left.line(name));
    if left == Left::Ended {
        println!("{}", text::LEAD_NO_RESUME);
    }
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
    let status = daemon::status(dir).await?;
    // A lead with no merged pin does not start (01M5QY976MTPRX5J4N1MKXAGDK):
    // the wave says nothing of it, and no line says that the lead starts.
    let clone = if runs(&status, &name) {
        None
    } else {
        let (main, common) = run::main_clone(&identity::working_dir()?)?;
        session_image::merged(&main, |_| {})?;
        Some((main, common))
    };
    wave(dir, &status, here, server).await;
    let Some((main, common)) = clone else {
        println!("{}", text::lead_runs(&repo, &name));
        return attach_here(runtime, &name).await;
    };
    let start = text::lead_start(&repo);
    launch(dir, api, here, server, (&main, &common), &start).await?;
    attach_here(runtime, &name).await
}

/// Starts the lead of the repository of `here` on the daemon in the
/// folder `dir`, from the main clone `main` with the git dir `common`,
/// with the first prompt `start`. It waits until the container runs,
/// and makes the new session the lead as the person, with no session
/// ID (01M5NJAYC7MXS13TA3VJB4N6RP). It gives the session ID of the
/// lead. A failed start ends the new session.
async fn launch(
    dir: &Path,
    api: Api,
    here: &Place,
    server: &str,
    (main, common): (&Path, &Path),
    start: &str,
) -> Result<String> {
    let repo = here.repo_text();
    // With no token, a sign-in file that ends soon is no token either.
    let host = run::host_facts(main).map_err(|fault| match fault {
        run::HostFault::SignInEnds(_) => anyhow::anyhow!("{}", text::LEAD_NEEDS_TOKEN),
        fault => fault.into(),
    })?;
    needs_token(&host.claude.sign_in())?;
    let merged = session_image::merged_config(main, |line| println!("{line}"))?;
    let image = merged.image.to_string();
    daemon::image(dir, &image, |line| println!("{line}")).await?;
    let folder = folder_of(&run::data_dir()?, &repo);
    let clone = folder.join("clone");
    let claude = folder.join("claude");
    if let Fresh::Kept(why) = fresh_clone(main, &clone)? {
        println!("{}", text::lead_clone_kept(&clone, &why));
    }
    let sources = session_image::merged_sources(main)?;
    run::claude_folder(
        &claude,
        &clone,
        &host.home.join(".claude.json"),
        &crate::permissions::Trust::of(here.repo(), server, &sources),
    )?;

    let session = identity::new_session_id();
    let place = Place::new(here.host(), here.repo().clone(), None)?;
    let me: SessionUri = identity::agent(&place, &session, server)?;
    let person = api.clone().signed_in(None)?;
    let api = api.signed_in(Some(&session))?;
    api.start(&me, StartReason::Process, false).await?;
    let named = identity::person(&place, server)?;
    let led = match person.lead(&named, &session).await {
        Ok(reply) => reply,
        Err(e) => {
            let _ = api.end(&me).await;
            return Err(e);
        }
    };
    println!("{}", text::lead_made(&repo, led.replaced.as_ref()));
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
        sign_in: host.claude.sign_in(),
        gitconfig: existing(host.home.join(".gitconfig")),
        gh_config: existing(host.home.join(".config/gh")),
        setup: merged.setup,
    };
    let start = daemon::Start {
        session: session.clone(),
        item: ITEM.into(),
        folder,
        spec: spec(&facts, &repo, start),
        start: start.to_owned(),
        values: host.values(),
        secrets: host.secrets(),
        server: server.to_owned(),
        me: me.clone(),
        terminal: true,
    };
    println!("{}", text::lead_starts(&repo, &name_of(&repo), &session));
    if let Err(e) = ask_start(dir, start).await {
        let _ = api.end(&me).await;
        return Err(e);
    }
    Ok(session)
}

/// What the daemon in the folder `dir` does for `riff lead move`
/// (#1163): it starts the lead of the repository of `wave` when none
/// runs, with the first prompt [`text::lead_moved`], and makes it the
/// lead as the person, with no session ID (01M5NJAYC7MXS13TA3VJB4N6RP).
/// It gives the reply for the caller, with the session ID
/// ([`session_of`]).
pub async fn take(dir: &Path, wave: &crate::wave::Wave) -> Result<String> {
    let here = wave.place(None)?;
    let repo = here.repo_text();
    let name = name_of(&repo);
    let status = daemon::status(dir).await?;
    let session = match status.sessions.iter().find(|s| s.name == name) {
        Some(live) => live.session.clone(),
        None => {
            let (main, common) = run::main_clone(&wave.main)?;
            let api = Api::new(&wave.server);
            let start = text::lead_moved(&repo);
            launch(dir, api, &here, &wave.server, (&main, &common), &start).await?
        }
    };
    let place = Place::new(here.host(), here.repo().clone(), None)?;
    let me = identity::person(&place, &wave.server)?;
    let api = Api::new(&wave.server).signed_in(None)?;
    api.lead(&me, &session).await?;
    Ok(text::lead_taken(&repo, here.host(), &name, &session))
}

/// The session ID of the new lead in the reply of the daemon
/// ([`take`]).
///
/// ```
/// let reply = riff::text::lead_taken("o/r", "thelio", "riff-lead-o-r", "1a2b");
/// assert_eq!(riff::lead::session_of(&reply), Some("1a2b"));
/// assert_eq!(riff::lead::session_of("done"), None);
/// ```
pub fn session_of(reply: &str) -> Option<&str> {
    reply.lines().find_map(|l| l.strip_prefix("session "))
}

/// The longest wait of `riff lead move` for the handover of the old
/// lead (01M596DH64SQ5AJ4Y6BETGTPSK).
pub const HANDOVER_WAIT: std::time::Duration = std::time::Duration::from_secs(300);

/// How often `riff lead move` reads the notes for the handover.
const HANDOVER_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

/// `riff lead move HOST` as `me` (#1163, 01M596DH64SQ5AJ4Y6BETGTPSK): the
/// person, or the lead of the person. It gives the session ID of the new
/// lead.
///
/// ```mermaid
/// sequenceDiagram
///     participant M as riff lead move thelio
///     participant O as old lead
///     participant D as daemon of thelio
///     M->>O: write the handover to the notes
///     O->>M: the notes change (at most `wait`)
///     M->>D: lead move
///     D->>D: start the lead, make it the lead
///     D-->>M: done, session ID
///     M->>O: you are not the lead any more
/// ```
///
/// A session that is not the lead refuses. A lead that writes no
/// handover in `wait` gets a warning, and the move goes on with the
/// notes as they are. A HOST with no daemon fails before the old lead
/// loses the lead. When the caller is the old lead, it asks nobody for
/// a handover: it cannot answer its own message, so it writes the
/// handover first.
pub async fn move_to(
    api: &Api,
    me: &SessionUri,
    host: &str,
    wait: std::time::Duration,
) -> Result<String> {
    let Some(thread) = me.default_thread() else {
        bail!("{}", text::RUN_NO_REPO);
    };
    let repo = me.place().repo_text();
    let user = me.who().user();
    let sessions = api.who(me, false).await?;
    let leads: Vec<&SessionInfo> = sessions
        .iter()
        .filter(|s| s.live && s.uri.lead())
        .filter(|s| s.uri.who().user() == user && s.uri.place().repo_text() == repo)
        .collect();
    let own = me.who().session();
    if own.is_some() && !leads.iter().any(|s| s.uri.who().session() == own) {
        bail!("{}", text::LEAD_MOVE_NOT_LEAD);
    }
    if leads.iter().any(|s| s.uri.place().host() == host) {
        bail!("{}", text::lead_move_same_host(host));
    }
    if daemon::remote::find(&sessions, user, &repo, host).is_none() {
        bail!("{}", text::no_daemon_host(host, &repo));
    }
    let old = leads.first().and_then(|s| s.uri.who().session());
    let asked = old.filter(|id| Some(*id) != own);
    if let Some(old) = asked {
        let before = api.notes_show(me, &thread, None, false).await?;
        api.tell(me, old, &text::lead_handover_ask(host)).await?;
        let start = tokio::time::Instant::now();
        loop {
            let now = api.notes_show(me, &thread, None, false).await?;
            if now.at_ms != before.at_ms || now.text != before.text {
                break;
            }
            if start.elapsed() >= wait {
                eprintln!("{}", text::lead_move_no_handover(wait));
                break;
            }
            tokio::time::sleep(HANDOVER_EVERY).await;
        }
    }
    let wait = daemon::remote::REPLY_WAIT;
    let request = daemon::remote::Request::LeadMove;
    let reply = daemon::remote::ask(api, me, host, &request, wait).await?;
    let Some(session) = session_of(&reply) else {
        bail!("riff: the daemon of {host} gave no session for the new lead: {reply}");
    };
    if let Some(old) = asked {
        api.tell(me, old, &text::lead_moved_away(host)).await?;
    } else if old.is_some() {
        println!("{}", text::lead_moved_away(host));
    }
    Ok(session.to_owned())
}

/// Starts the wave of the repository of `here` on the daemon in the
/// folder `dir`, when the daemon of `status` runs none
/// (01M53NGVE839TRH7XB7CDPQNZE): the lead in a container has no rollout
/// of its own. A failed ask stops no lead: it prints why
/// (01M5515CJ7QDJ639VGR7SJTE08).
async fn wave(dir: &Path, status: &Status, here: &Place, server: &str) {
    let repo = here.repo_text();
    if crate::wave::runs(status, &repo) {
        println!("{}", text::wave_runs(&repo));
        return;
    }
    match crate::wave::ask(dir, here, server, |line| println!("{line}")).await {
        Ok(repo) => println!("{}", text::wave_asked(&repo)),
        Err(e) => println!("{}", text::lead_no_wave(&repo, &format!("{e:#}"))),
    }
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

/// `riff lead notes`: shows the notes of the lead, and with `edit`
/// changes them (01M59258NTXPMWV18TKRKWNTYN, 01M59258K3TYR09HC4AWHWZPEX).
///
/// `edit` is `None` to show. `Some(None)` opens the editor, and
/// `Some(Some(path))` takes the new text from the file `path` (`-` is the
/// standard input). A new text equal to the old text changes nothing.
pub async fn notes(
    api: &Api,
    me: &SessionUri,
    thread: &riff_core::name::ThreadName,
    user: Option<&str>,
    history: bool,
    edit: Option<Option<&Path>>,
) -> Result<()> {
    let Some(file) = edit else {
        let reply = api.notes_show(me, thread, user, history).await?;
        print!("{}", text::notes_shown(&reply, thread));
        return Ok(());
    };
    let old = api.notes_show(me, thread, None, false).await?.text;
    let new = match file {
        Some(path) if path == Path::new("-") => {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)?;
            text
        }
        Some(path) => std::fs::read_to_string(path)?,
        None => in_editor(&old)?,
    };
    let reply = api.notes_set(me, thread, new.trim_end()).await?;
    println!("{}", text::notes_saved(reply, thread));
    Ok(())
}

/// Opens `$VISUAL`, `$EDITOR` or `vi` on `old`, and gives the text after
/// it.
fn in_editor(old: &str) -> Result<String> {
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .find_map(|var| std::env::var(var).ok().filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| "vi".to_owned());
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("riff-lead-notes.md");
    std::fs::write(&path, old)?;
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("riff-editor")
        .arg(&path)
        .status()?;
    if !status.success() {
        bail!("the editor {editor} failed: riff changed no notes");
    }
    Ok(std::fs::read_to_string(&path)?)
}

/// Sends `body` to the lead of the person in the repository of
/// `place`, else of this directory, as the person.
pub(crate) async fn tell_lead(place: Option<&Place>, server: &str, body: &str) -> Result<()> {
    let place = identity::here(place)?;
    let me = identity::person(&place, server)?;
    let api = Api::new(server).signed_in(None)?;
    api.tell(&me, riff_client::api::LEAD, body).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{End, Exit, Fake};

    const NAME: &str = "riff-lead-como-technologies-riff";

    /// The fake runs the lead `NAME` with a terminal.
    async fn lead(fake: &Fake) -> End {
        let spec = SessionSpec {
            name: NAME.into(),
            image: "img".into(),
            home: "/home/mike".into(),
            workdir: "/d/clone".into(),
            binds: vec![],
            env: vec![],
            memory: "8g".into(),
            cpus: 4,
            command: vec!["claude".into()],
        };
        fake.start_terminal(&spec, &[]).await.unwrap()
    }

    /// 01M51PD7M04HZR2PVVJA4WCBK5, 01M51PD7Q24N252V66NFEHY49N
    #[tokio::test]
    async fn an_attach_to_a_lead_that_runs_redraws_it_and_a_detach_says_that_it_runs_on() {
        let fake = Fake::default().with_image("img");
        let _end = lead(&fake).await;
        let left = attach(&fake, NAME).await.unwrap();
        assert_eq!(left, Left::Detached);
        assert_eq!(left.line(NAME), text::lead_detached(NAME));
        // The redraw comes before the attach, so the attach changes the
        // size of the terminal.
        assert_eq!(
            fake.calls(),
            [
                format!("terminal {NAME}"),
                format!("redraw {NAME}"),
                format!("attach {NAME}"),
            ]
        );
        assert!(fake.running(NAME).await.unwrap());
    }

    /// 01M51PD7Q24N252V66NFEHY49N
    #[tokio::test]
    async fn the_end_of_the_lead_in_the_attach_says_that_the_lead_ended() {
        let fake = Fake::default().with_image("img").with_exit_in_attach();
        let end = lead(&fake).await;
        let left = attach(&fake, NAME).await.unwrap();
        assert_eq!(left, Left::Ended);
        assert_eq!(left.line(NAME), text::lead_ended(NAME));
        assert_ne!(text::lead_ended(NAME), text::lead_detached(NAME));
        assert_eq!(end.wait().await.unwrap(), Exit::SUCCESS);
    }

    #[tokio::test]
    async fn a_lead_that_the_runtime_cannot_see_after_the_attach_counts_as_a_detach() {
        let fake = Fake::default().with_image("img").with_no_answer();
        let _end = lead(&fake).await;
        assert_eq!(attach(&fake, NAME).await.unwrap(), Left::Detached);
    }

    /// The fake runs the lead `NAME` with a bind of the sign-in file of
    /// the host in `home`, as a lead with no token of an older riff.
    async fn lead_with_file(fake: &Fake, home: &Path) -> (End, PathBuf) {
        let host = home.join(".claude");
        std::fs::create_dir_all(&host).unwrap();
        let file = host.join(".credentials.json");
        std::fs::write(&file, "old").unwrap();
        let facts = Facts {
            session: "1a2b3c4d-0000-4000-8000-000000000000".into(),
            item: ITEM.into(),
            image: "img".into(),
            home: home.to_owned(),
            user: "mike".into(),
            clone: home.join("clone"),
            objects: home.join("objects"),
            claude: home.join("lead-claude"),
            sign_in: run::ClaudeSignIn::File(file.clone()),
            gitconfig: None,
            gh_config: None,
            setup: Default::default(),
        };
        let spec = spec(&facts, "como-technologies/riff", "start");
        let values = [("GH_TOKEN".to_owned(), "gho_x".to_owned())];
        (fake.start_terminal(&spec, &values).await.unwrap(), file)
    }

    /// 01M5NJ87YEHHGJBANM7CRYV56Z
    #[tokio::test]
    async fn a_lead_is_stale_when_the_host_replaced_its_sign_in_file() {
        let home = tempfile::tempdir().unwrap();
        let fake = Fake::default().with_image("img");
        let (_end, file) = lead_with_file(&fake, home.path()).await;
        assert!(!stale(&fake, NAME, home.path()).await);
        // A write in place keeps the file of the bind.
        std::fs::write(&file, "refreshed in place").unwrap();
        assert!(!stale(&fake, NAME, home.path()).await);
        // claude on the host writes a new file in its place.
        let new = file.with_extension("new");
        std::fs::write(&new, "new").unwrap();
        std::fs::rename(&new, &file).unwrap();
        assert!(stale(&fake, NAME, home.path()).await);
    }

    /// 01M5NJ87YEHHGJBANM7CRYV56Z
    #[tokio::test]
    async fn a_lead_with_the_token_or_that_does_not_run_is_not_stale() {
        let home = tempfile::tempdir().unwrap();
        let fake = Fake::default().with_image("img");
        assert!(!stale(&fake, NAME, home.path()).await);
        let _end = lead(&fake).await;
        assert!(!stale(&fake, NAME, home.path()).await);
    }

    /// 01M5NJ87NXF6077Y9W32840BR8
    #[test]
    fn a_lead_needs_the_token_of_the_host() {
        assert!(needs_token(&run::ClaudeSignIn::Token).is_ok());
        let file = run::ClaudeSignIn::File("/home/mike/.claude/.credentials.json".into());
        let refused = needs_token(&file).unwrap_err().to_string();
        assert!(refused.contains("claude token: not set"), "{refused}");
        assert!(
            refused.contains("claude setup-token, then riff claude-token"),
            "{refused}"
        );
    }

    #[tokio::test]
    async fn an_attach_to_a_lead_that_does_not_run_fails() {
        let fake = Fake::default().with_image("img");
        assert!(attach(&fake, NAME).await.is_err());
    }
}
