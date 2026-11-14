//! The lead of a project in a container with a terminal (#720,
//! 01M4Z60B81RTZT71PN5HV0GZHX), and `riff lead start` and `riff lead
//! restart` (#1793).
//!
//! # Design
//!
//! The lead is a session of the riff daemon, as a worker is, but with a
//! terminal: `podman run -d -i -t`. conmon holds the terminal, so the
//! lead runs on when the person detaches with Ctrl-P Ctrl-Q, and when
//! the daemon stops. The daemon of a host that joined the project starts
//! the lead ([`crate::lead_host`], [`claim_start`]). `riff lead start`
//! and the Sessions tab of the TUI attach to it
//! ([`crate::runtime::Runtime::attach`]). `riff lead start` starts no
//! lead: on a host that runs no lead of the project, it names the host of
//! the lead and the step there ([`start`]).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as riff daemon of a host that joined
//!     participant S as riff-server
//!     participant C as container riff-lead-OWNER-REPO
//!     participant P as riff lead start (the person)
//!     D->>S: the claim lead is free: claim it
//!     D->>S: start the session of the lead
//!     D->>C: podman run -d -i -t: riff relay --terminal -- claude
//!     D->>S: keep the session of the lead alive
//!     P->>D: status: does the lead run here?
//!     P->>C: podman exec stty: another size, so claude draws again
//!     P->>C: podman attach (Ctrl-P Ctrl-Q detaches)
//! ```
//!
//! | Part | Value | Requirement |
//! |---|---|---|
//! | The container | `riff-lead-OWNER-REPO` ([`name_of`]): one lead for each repository | 01M4Z60B81RTZT71PN5HV0GZHX, 01M501P9MXNAP5QXNVBVF5JJFN |
//! | The folder | `leads/OWNER-REPO` in the data folder of riff ([`folder_of`]): `clone/` and `claude/` | 01M501P9Q51AYN9GBPWKVHA229 |
//! | The start | one start of the lead at a time on the host: each start holds the lock `start.lock` in the folder of the lead ([`start_lock`]), and under the lock a start that finds the lead running starts nothing ([`one_start`]) | 01M77Y2BG12SEVF4ZEZ673QBFZ |
//! | The cause | each start gives the daemon its cause ([`LeadCause`], #2099): the lead loop ([`claim_start`]), `riff lead move` and who ran it ([`take`]), and in the daemon the start of the daemon, a fault and an update (`Daemon::lead_back`, `Daemon::leads_again`). The daemon writes the cause in the line of the start in its log ([`text::lead_starts`]), and keeps it in the record of the lead for `riff daemon status` | 01M7BNP4ZYJCCEK7ZHCJ3EX3SJ |
//! | The clone | `git clone --reference MAIN`, on the default branch, at the first start; a fetch and a fast-forward at each later start, when it has no local change and git can fast-forward it ([`fresh_clone`]) | 01M501P9Q51AYN9GBPWKVHA229, 01M50RRQ9DT0JPSB358EAZRP8Z, 01M51JYDHAQDJ3GH2J8DT3AZQS |
//! | The Claude folder | the first-run answers of `claude`, as for each session ([`run::claude_folder`]), and `"tui": "default"`, so `claude` turns on no mouse reports ([`run::TUI`]) | 01M501P9SDGVWG3MTVCW5BDRQQ, 01M51JACH6TQP318ZFG6NQ9GRM, 01M6XVBRT2T838TX53SSXR6HRH |
//! | The command | `claude --model MODEL --session-id ID --dangerously-skip-permissions START` ([`spec`]), with the model of the merged `riff.toml` ([`crate::session_setup::Setup::claude`]); the daemon runs it as `riff relay --terminal -- ...` | 01M501P9VN47QAKZ6B8P39ZSW4, 01M5GG2X5F1HXCQJ2JJWV6305K, 01M6VAQWECJ6N6VJF66KCMNYD1 |
//! | The riff tools | [`crate::tools::LEAD_TOOLS`], on riff-server as the session of the lead | 01M501P9XWXWDBSXP4QRTHTZ4P |
//! | The riff commands | `RIFF_SOCKET` and no `RIFF_SERVER`: each command that asks riff-server reaches it through the daemon, for example `riff top`, `riff read`, `riff verify`, `riff wave list` and `riff plan hold`; a step of a person or a claim refuses ([`crate::reach::COMMANDS`]) | 01M5GVPNAKAPA596K9361HSAAV, 01M6QPQZ0FNEZFQ1F4Q9C7FEEQ, 01M74A84SA5KCBVGVW7M9ETJ3A, 01M74A8CXVRD352VRJ9FH54HJ1 |
//! | The riff command | `riff` is the riff of the container: the relay puts `/opt/riff` first on the `PATH` of `claude` ([`crate::session_relay::riff_first`]), so `! riff who` works in the terminal of the lead. The spec sets no `PATH` of its own, and `/opt/riff/riff` stays for the permission rules | 01M5QVCVXWBBG1BMGRVBQS25W9 |
//! | The wakes | `/opt/riff/riff relay watch --follow` in the Monitor tool, started once | 01M501PA04NP79RJQEDP4D7YTM, 01M59235BBXEKPZBRH23C39574 |
//! | The attach | [`crate::runtime::Runtime::redraw`], then [`crate::runtime::Runtime::attach`] ([`attach`]): `claude` draws its screen at each attach | 01M51PD7M04HZR2PVVJA4WCBK5 |
//! | After the attach | [`Left`]: a detach, or the end of the lead when its container does not run; the line says which. First riff writes the reset of the modes of the terminal ([`crate::runtime::reset_terminal`]) | 01M51PD7Q24N252V66NFEHY49N, 01M6XVBRQMBMK68K8GCDRZ5GJ5 |
//! | The sign-in of `claude` | the token of `riff claude-token` only ([`needs_token`]); a lead from an older riff with the sign-in file of the host gets a word when the host replaced that file ([`stale`], [`crate::daemon::Daemon::stale_leads`]) | 01M77Y6ZQ76DTX470STN8ATTA9, 01M5NJ87YEHHGJBANM7CRYV56Z |
//! | A new riff | after `riff update`, the daemon stops an old lead (the bind of its riff has no link: [`old`]) when it is idle ([`idle`]: the `Stop` hook ended its turn, and its transcript got no write after the tail of the turn, [`between_turns`]; or its transcript got no write for 5 minutes, [`quiet`]) and the daemon itself runs the riff on disk, and starts it again with `claude --resume ID` ([`spec_again`], [`again`]): the same session, clone and Claude folder, no new session on riff-server; the main clone is that of the wave, and with no wave the one that the clone of the lead links to ([`linked_main`]); the person gets one line in riff chat ([`crate::daemon::Daemon::leads_again`]), and an attached terminal one line, then the attach to the new container ([`attach`], [`Again`]) | 01M5TC24PB3YC2CC8BA275WSEN, 01M6XTS7V47RFX1G37FRGRBWCY, 01M6WBX2FQEHW92Y1A5HM4H8BM, 01M6WBX2FT7Y9FV9VGGPDBKAHM, 01M5TC2919TB9XPF42YFNJBP79, 01M5TC29A4Q94NJP7HFC75XCF3, 01M6QB89JJEM9Z1V3RVZYHKYT0 |
//! | The project | `riff lead move` and the daemon start a lead only for a project of the riff ([`crate::project::lead_refused`], #1863): else riff names the fix, `riff project init`, and starts no session and no container | 01M77FHTT8BZ2YN1SV7H1X3NG8, 01M77FHTXBCWA4FG793TEB5XCY |
//! | The start | `riff lead start` starts no lead (#1793): it attaches to the lead of the project when its container runs on this host. Else it names the host of the lead and the step there, or says that the daemon of a host that joined the project starts the lead ([`start`], [`text::lead_not_here`]) | 01M77Y6ZBV7C8Z332SAFKQCZ5A |
//! | The repository | the daemon serves it while the lead runs, so `riff who` and the commands with `--host` reach the host (`Daemon::serve_lead`) | 01M6QVSWQZE41BMQ6YN2963C4V |
//! | The restart | `riff lead restart`: riff finds the lead of the project ([`to_restart`]), the lead writes its notes ([`restart_notes`]), then the daemon of its host ends its container ([`crate::daemon::Daemon::stop_lead`]); the host loses the part lead, an update starts the old lead no more, and a host that joined the project starts a new lead ([`crate::lead_host`]) | 01M77Y6ZEQ4A6C6E0XZ56WRE89, 01M6QVSWWM3EA4ZS47XCKV38D0 |
//!
//! The clone and the Claude folder stay between the starts of the lead:
//! the lead keeps the history of its conversations, and a dialog of
//! `claude` that the person answered once. Each start is a new session
//! of riff-server and of `claude`, but a start again after `riff update`
//! keeps the session and resumes the conversation. The other spec parts
//! are those of a worker ([`crate::run::spec`]).

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use riff_core::name::{Place, SessionUri};
use riff_core::wire::{SessionInfo, StartReason};

use crate::daemon::{self, Ask, Say, Status};
use crate::run::{self, Facts};
use crate::runtime::{Runtime, SessionSpec};
use crate::{git_credential, identity, session_image, text};
use riff_client::api::Api;
use riff_client::sessions::LeadCause;

/// The work item of the lead in the daemon.
pub const ITEM: &str = "lead";

/// The name of the container of the lead of the repository `repo`
/// (`OWNER/REPO`): [`riff_core::name::lead_container`].
///
/// ```
/// assert_eq!(riff::lead::name_of("como-technologies/riff"), "riff-lead-como-technologies-riff");
/// ```
pub fn name_of(repo: &str) -> String {
    riff_core::name::lead_container(repo)
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
    folder_of_name(data, &name_of(repo))
}

/// The folder of the lead in the container `name` ([`name_of`]), in the
/// data folder `data` of riff: the same as [`folder_of`] of its
/// repository.
///
/// ```
/// use std::path::Path;
/// use riff::lead::{folder_of, folder_of_name, name_of};
///
/// let data = Path::new("/d/riff");
/// let repo = "como-technologies/riff";
/// assert_eq!(folder_of_name(data, &name_of(repo)), folder_of(data, repo));
/// ```
pub fn folder_of_name(data: &Path, name: &str) -> PathBuf {
    data.join("leads")
        .join(name.strip_prefix("riff-lead-").unwrap_or(name))
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
///     setup: Default::default(),
/// };
/// let spec = riff::lead::spec(&facts, "o/r", &riff::text::lead_start("o/r"));
/// assert_eq!(spec.name, "riff-lead-o-r");
/// assert_eq!(spec.workdir, facts.clone);
/// assert_eq!(
///     spec.command[..6],
///     ["claude", "--model", "opus", "--session-id", facts.session.as_str(), "--dangerously-skip-permissions"]
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
    spec_with(facts, repo, "--session-id", start)
}

/// [`spec`] for a lead that goes on with its conversation: `claude
/// --resume ID` in place of `claude --session-id ID`
/// (01M5TC2919TB9XPF42YFNJBP79). The daemon starts the lead again
/// with it on a new riff ([`again`]). `start` is its first prompt,
/// [`text::lead_again`].
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
///     sign_in: riff::run::ClaudeSignIn::Token,
///     gitconfig: None,
///     setup: Default::default(),
/// };
/// let spec = riff::lead::spec_again(&facts, "o/r", &riff::text::lead_again("o/r"));
/// assert_eq!(spec.name, "riff-lead-o-r");
/// assert_eq!(
///     spec.command[..6],
///     ["claude", "--model", "opus", "--resume", facts.session.as_str(), "--dangerously-skip-permissions"]
/// );
/// assert!(!spec.command.iter().any(|a| a == "--session-id"));
/// // The same clone and the same Claude folder: `claude` finds the conversation.
/// assert_eq!(spec.workdir, facts.clone);
/// ```
pub fn spec_again(facts: &Facts, repo: &str, start: &str) -> SessionSpec {
    spec_with(facts, repo, "--resume", start)
}

/// Adds [`text::lead_no_github`] with `why` to the end of the first
/// prompt of the lead of `spec`, the last word of its command: a lead
/// that starts with no token on GitHub tells its person the fix and the
/// prompt for `claude` on the host first (01M6YC3T0TXANJH6YMTJZR1HCR,
/// 01M6Z87JTXW0Y99X08VHA2SSSR).
///
/// ```
/// use riff::runtime::SessionSpec;
///
/// let mut spec = SessionSpec {
///     name: "riff-lead-o-r".into(),
///     image: "img".into(),
///     home: "/home/mike".into(),
///     workdir: "/w".into(),
///     binds: vec![],
///     env: vec![],
///     memory: String::new(),
///     cpus: 0,
///     command: vec!["claude".into(), "Start.".into()],
/// };
/// riff::lead::tell_no_github(&mut spec, "the fix");
/// assert_eq!(spec.command[1], format!("Start. {}", riff::text::lead_no_github("the fix")));
/// ```
pub fn tell_no_github(spec: &mut SessionSpec, why: &str) {
    if let Some(start) = spec.command.last_mut() {
        start.push(' ');
        start.push_str(&text::lead_no_github(why));
    }
}

/// [`spec`], with the flag `session_flag` for the session ID of `claude`.
fn spec_with(facts: &Facts, repo: &str, session_flag: &str, start: &str) -> SessionSpec {
    let mut spec = run::spec(facts);
    spec.name = name_of(repo);
    // The folder `~/.cache` of the container can belong to root, when a
    // volume of `riff.toml` binds under it: `riff build` needs another
    // one.
    spec.env.push(crate::runtime::Env::Value(
        crate::check::VAR.into(),
        spec.home.join(".riff-check").to_string_lossy().into_owned(),
    ));
    spec.command = facts.setup.claude();
    spec.command.extend([
        session_flag.to_owned(),
        facts.session.clone(),
        "--dangerously-skip-permissions".into(),
        start.to_owned(),
    ]);
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
///     digest: String::new(),
///     image: "i".into(),
///     since_ms: 0,
///     phase: Phase::Terminal,
///     last: String::new(),
///     lead: true,
///     cause: None,
/// };
/// let status = Status {
///     pid: 1, build: "b".into(), since_ms: 0, dir: "/d".into(), sessions: vec![live],
///     limit: None, waiting: vec![], waves: vec![], parts: vec![], leaving: vec![], jobs: 0, test: None, mcp: vec![],
///     claude_token: Default::default(), watches: vec![], complete: vec![], releases: vec![], lost: vec![], github: Default::default(), no_starts: vec![], no_tokens: vec![], writes: vec![], unserved: vec![], sends: vec![], events: vec![],
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
        git_credential::run(
            main,
            &remote,
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
    if let Err(e) = git_credential::run(clone, "origin", &["fetch", "-q", "origin"]) {
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
/// (01M77Y6ZQ76DTX470STN8ATTA9): the lead runs for hours, and the next
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

/// True when the lead `name` runs an old riff (01M5TC24PB3YC2CC8BA275WSEN):
/// the bind of its container at [`daemon::RELAY_BIN`] holds a file that
/// `riff update` replaced, so in the container the file has no link. The
/// path on the host holds the new riff. When the runtime cannot say, the
/// lead is not old.
pub async fn old(runtime: &impl Runtime, name: &str) -> bool {
    matches!(
        runtime.links(name, Path::new(daemon::RELAY_BIN)).await,
        Ok(0)
    )
}

/// True when no turn of the lead of `record` runs, as far as riff can
/// see: no write to its transcript in `idle`, and its container started
/// `idle` ago (01M5TC24PB3YC2CC8BA275WSEN). `claude` writes the transcript
/// at each message and each tool call, so a turn leaves it quiet for a
/// short time only. A tool call that runs very long, or a lead that thinks
/// long, counts as idle after `idle`: the wait is a guess, and a wrong
/// guess costs one turn.
///
/// ```
/// use std::time::Duration;
/// use riff::daemon::Record;
/// use riff_client::sessions::{Live, Phase};
///
/// let dir = tempfile::tempdir()?;
/// let project = dir.path().join("claude/projects/-d-clone");
/// std::fs::create_dir_all(&project)?;
/// let transcript = std::fs::File::create(project.join("s1.jsonl"))?;
/// let record = Record {
///     live: Live {
///         name: "riff-lead-o-r".into(),
///         session: "s1".into(),
///         item: "lead".into(),
///         image: "i".into(),
///         digest: String::new(),
///         since_ms: 0,
///         phase: Phase::Terminal,
///         last: String::new(),
///         lead: true,
///         cause: None,
///     },
///     folder: dir.path().to_owned(),
///     server: "http://127.0.0.1:1".into(),
///     me: "riff://mike@thelio/o/r?session=s1".parse().unwrap(),
///     inputs: vec![],
///     closed: false,
///     failed: false,
///     job: None,
///     terminal: true,
///     takeover: false,
///     interrupt: false,
///     model: Some("opus".into()),
///     github: None,
/// };
/// let idle = Duration::from_secs(300);
/// // A write now: a turn runs.
/// assert!(!riff::lead::quiet(&record, idle));
/// // The last write was 10 minutes ago: no turn runs.
/// transcript.set_modified(std::time::SystemTime::now() - Duration::from_secs(600))?;
/// assert!(riff::lead::quiet(&record, idle));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn quiet(record: &daemon::Record, idle: std::time::Duration) -> bool {
    let started = started(record);
    let last = written(record).map_or(started, |written| written.max(started));
    std::time::SystemTime::now()
        .duration_since(last)
        .is_ok_and(|age| age >= idle)
}

/// The start of the container of the lead of `record`.
fn started(record: &daemon::Record) -> std::time::SystemTime {
    let since = u64::try_from(record.live.since_ms).unwrap_or(0);
    std::time::UNIX_EPOCH + std::time::Duration::from_millis(since)
}

/// The last write to the transcript of the lead of `record`, if it has
/// one.
fn written(record: &daemon::Record) -> Option<std::time::SystemTime> {
    crate::usage::transcripts_in(&record.folder.join("claude"), &record.live.session)
        .iter()
        .filter_map(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok())
        .max()
}

/// The file of the end of the last turn of a lead, in the folder of the
/// lead: the daemon writes it at each `Stop` hook ([`end_turn`],
/// 01M6WBX2FQEHW92Y1A5HM4H8BM).
pub const TURN_ENDED: &str = "turn-ended";

/// How long after the end of a turn `claude` may still write the
/// transcript of that turn, for example the summary of the hooks of the
/// end. A later write is a new turn (01M6WBX2FQEHW92Y1A5HM4H8BM).
pub const TURN_TAIL: std::time::Duration = std::time::Duration::from_secs(5);

/// Keeps the end of a turn of the lead whose folder is `folder`: the
/// file [`TURN_ENDED`], written now, with the time in milliseconds since
/// the epoch (01M6WBX2FQEHW92Y1A5HM4H8BM).
pub fn end_turn(folder: &Path) -> Result<()> {
    let path = folder.join(TURN_ENDED);
    std::fs::write(&path, daemon::now_ms().to_string())
        .with_context(|| format!("cannot write {}", path.display()))
}

/// True when the lead of `record` is between two turns
/// (01M6WBX2FQEHW92Y1A5HM4H8BM): a turn of its container ended
/// ([`end_turn`]), and its transcript got no write later than
/// [`TURN_TAIL`] after that end. A lead from a riff with no `Stop` hook
/// never is: [`quiet`] holds for it.
pub fn between_turns(record: &daemon::Record) -> bool {
    let Ok(ended) = std::fs::metadata(record.folder.join(TURN_ENDED)).and_then(|m| m.modified())
    else {
        return false;
    };
    ended >= started(record) && written(record).is_none_or(|written| written <= ended + TURN_TAIL)
}

/// True when no turn of the lead of `record` runs: it is between two
/// turns ([`between_turns`]), or its transcript got no write in `quiet`
/// ([`quiet`], 01M6WBX2FQEHW92Y1A5HM4H8BM).
pub fn idle(record: &daemon::Record, quiet: std::time::Duration) -> bool {
    between_turns(record) || self::quiet(record, quiet)
}

/// Attaches the terminal of this process to the lead `name` with
/// `runtime`, until a detach or the end of the lead. `folder` is the
/// folder of the lead ([`folder_of`]). `out` is that terminal: riff
/// writes its lines there. A lead with a stale sign-in gets the line of
/// [`text::lead_sign_in_stale`] first ([`stale`]). Then it asks the
/// runtime to make the lead draw its terminal again
/// (01M51PD7M04HZR2PVVJA4WCBK5). When that fails, it prints why, and
/// attaches. After each attach, also a failed one, it writes the reset of
/// the modes of the terminal ([`crate::runtime::reset_terminal`],
/// 01M6XVBRQMBMK68K8GCDRZ5GJ5).
///
/// When the daemon starts the lead again on a new riff while the person is
/// attached ([`Again`], 01M6XTS7V47RFX1G37FRGRBWCY), the stop of the old
/// container ends the attach. Then it waits for the new container, says
/// [`text::lead_again_attached`], and attaches to it. After the attach, a
/// lead whose container does not run ended (01M51PD7Q24N252V66NFEHY49N). When
/// the runtime cannot say, it counts as a detach.
///
/// ```mermaid
/// sequenceDiagram
///     participant P as the terminal of the person
///     participant D as riff daemon
///     participant C as container of the lead
///     P->>C: attach
///     D->>D: write started-again: the riff, the time
///     D->>C: stop: the attach ends
///     P->>P: the reset of the modes of the terminal
///     D->>C: a new container: claude --resume
///     D->>D: started-again: started
///     P->>P: riff started this lead again on riff vX.Y.Z; its conversation goes on
///     P->>C: attach to the new container
/// ```
pub async fn attach(
    runtime: &impl Runtime,
    name: &str,
    folder: &Path,
    out: &mut (impl Write + Send),
) -> Result<Left> {
    if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty())
        && stale(runtime, name, Path::new(&home)).await
    {
        writeln!(out, "{}", text::lead_sign_in_stale(name))?;
    }
    writeln!(out, "{}", text::LEAD_DETACH)?;
    loop {
        if let Err(e) = runtime.redraw(name).await {
            writeln!(out, "{}", text::lead_no_redraw(&format!("{e:#}")))?;
        }
        out.flush()?;
        let since_ms = daemon::now_ms();
        let attached = runtime.attach(name).await;
        crate::runtime::reset_terminal(out)?;
        attached?;
        match started_again(runtime, name, folder, since_ms).await {
            Some(riff) => writeln!(out, "{}", text::lead_again_attached(&riff))?,
            None => break,
        }
    }
    Ok(match runtime.running(name).await {
        Ok(false) => Left::Ended,
        Ok(true) | Err(_) => Left::Detached,
    })
}

/// The file in the folder of a lead of a start again on a new riff
/// (01M6XTS7V47RFX1G37FRGRBWCY): the daemon writes it before it stops the old
/// container ([`Again`]).
pub const AGAIN: &str = "started-again";

/// How long an attach waits for the new container of a lead that starts again
/// (01M6XTS7V47RFX1G37FRGRBWCY). The daemon waits up to 30 s for the end of
/// the old container, then tries the start up to three times.
pub const AGAIN_WAIT: std::time::Duration = std::time::Duration::from_secs(120);

/// The time between two looks of an attach at a lead that starts again.
const AGAIN_EVERY: std::time::Duration = std::time::Duration::from_millis(50);

/// A start again of a lead on a new riff, in the file [`AGAIN`] of the folder
/// of the lead (01M6XTS7V47RFX1G37FRGRBWCY). The daemon writes it before it
/// stops the old container, and again with `started` when the new container
/// runs. A start that fails removes it ([`Again::clear`]).
///
/// ```
/// use riff::lead::Again;
///
/// let folder = tempfile::tempdir()?;
/// assert_eq!(Again::read(folder.path()), None);
/// let again = Again { riff: "v3.3.0".into(), at_ms: 7, started: false };
/// again.write(folder.path())?;
/// assert_eq!(Again::read(folder.path()), Some(again));
/// Again::clear(folder.path());
/// assert_eq!(Again::read(folder.path()), None);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Again {
    /// The riff of the new container, for example `v3.3.0`.
    pub riff: String,
    /// The time of the stop of the old container, in milliseconds since
    /// the epoch.
    pub at_ms: i64,
    /// True when the new container runs.
    pub started: bool,
}

impl Again {
    /// Writes the start again in the folder `folder` of the lead.
    pub fn write(&self, folder: &Path) -> Result<()> {
        let path = folder.join(AGAIN);
        std::fs::write(&path, serde_json::to_vec(self)?)
            .with_context(|| format!("cannot write {}", path.display()))
    }

    /// The start again in the folder `folder` of the lead, or `None`.
    pub fn read(folder: &Path) -> Option<Again> {
        let text = std::fs::read(folder.join(AGAIN)).ok()?;
        serde_json::from_slice(&text).ok()
    }

    /// Removes the start again from the folder `folder` of the lead: the
    /// start failed.
    pub fn clear(folder: &Path) {
        let _ = std::fs::remove_file(folder.join(AGAIN));
    }
}

/// The riff of the new container when the daemon started the lead `name`
/// again after `since_ms` ([`Again`], 01M6XTS7V47RFX1G37FRGRBWCY). It waits
/// up to [`AGAIN_WAIT`] for the new container. `None` when no start again
/// came after `since_ms`, when the start failed, and when the new container
/// does not run in time.
async fn started_again(
    runtime: &impl Runtime,
    name: &str,
    folder: &Path,
    since_ms: i64,
) -> Option<String> {
    let start = std::time::Instant::now();
    loop {
        let again = Again::read(folder).filter(|again| again.at_ms >= since_ms)?;
        if again.started && runtime.running(name).await.unwrap_or(false) {
            return Some(again.riff);
        }
        if start.elapsed() >= AGAIN_WAIT {
            return None;
        }
        tokio::time::sleep(AGAIN_EVERY).await;
    }
}

/// [`attach`] with the terminal of this process, then the line of
/// [`Left::line`]. After the end of the lead, it also says that the
/// `claude --resume` line of `claude` does not work on the host.
async fn attach_here(runtime: &impl Runtime, name: &str, folder: &Path) -> Result<()> {
    let left = attach(runtime, name, folder, &mut std::io::stdout()).await?;
    println!("{}", left.line(name));
    if left == Left::Ended {
        println!("{}", text::LEAD_NO_RESUME);
    }
    Ok(())
}

/// `riff lead start` in the repository of `here` (#1793,
/// 01M77Y6ZBV7C8Z332SAFKQCZ5A): it attaches to the lead of the project when
/// the daemon of this machine runs its container. It starts no lead: the
/// daemon of a host that joined the project does ([`crate::lead_host`]).
/// Else the error says where the lead is ([`text::lead_not_here`]): it
/// reads the holder of the claim lead on riff-server, with the sign-in
/// `api` of the person at the riff-server `server`. On a host that is not
/// Linux, it attaches to nothing.
///
/// ```mermaid
/// flowchart TD
///     S["riff lead start"] --> R{"the container of the lead<br/>runs on this host?"}
///     R -- yes --> A["attach"]
///     R -- no --> C{"the claim lead"}
///     C -- "a session on another host" --> H["exit 1: ssh HOST, then riff lead start,<br/>or riff lead move with this host"]
///     C -- "a session on this host" --> D["exit 1: the daemon runs<br/>no container of it now"]
///     C -- free --> N["exit 1: a host that joined<br/>starts the lead: riff host join"]
/// ```
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
    let Some(thread) = here.default_thread() else {
        bail!("{}", text::RUN_NO_REPO);
    };
    let repo = here.repo_text();
    let name = name_of(&repo);
    // With no daemon here, no lead runs here: riff says where it runs.
    let status = daemon::status(dir).await;
    if status.is_ok_and(|status| runs(&status, &name)) {
        println!("{}", text::lead_runs(&repo, &name));
        let folder = folder_of(&run::data_dir()?, &repo);
        return attach_here(runtime, &name, &folder).await;
    }
    let me = identity::person(here, server)?;
    let plan = api.signed_in(None)?.plan_show(&me, &thread).await?;
    bail!(
        "{}",
        text::lead_not_here(&repo, here.host(), plan.lead.as_ref())
    )
}

/// The start of a new lead by the daemon in the folder `dir` for the
/// project of `wave` (#1791): the claim lead was free. It starts the lead
/// in its container from the main clone of the wave, and asks for the
/// claim lead only when it is free. False when another session
/// got the claim first: the new session ends, and no container starts.
/// False too when another start on this host made the lead first
/// ([`one_start`]): the next look claims for that lead.
pub async fn claim_start(dir: &Path, wave: &crate::wave::Wave) -> Result<bool> {
    let here = wave.place(None)?;
    let main = wave.main.clone();
    let (main, common) = tokio::task::spawn_blocking(move || {
        let (main, common) = run::main_clone(&main)?;
        session_image::merged(&main, |_| {})?;
        anyhow::Ok((main, common))
    })
    .await??;
    let api = Api::new(&wave.server);
    let start = text::lead_start(&wave.repo);
    let new = New {
        main: (&main, &common),
        start: &start,
        if_free: true,
        cause: LeadCause::LeadLoop,
    };
    match launch(dir, api, &here, &wave.server, new).await {
        Ok(Launched::New(_)) => Ok(true),
        Ok(Launched::Runs(_)) => Ok(false),
        Err(e) if e.downcast_ref::<Held>().is_some() => Ok(false),
        Err(e) => Err(e),
    }
}

/// Another live session holds the claim lead of the project: the start of
/// a lead with `if_free` starts no second lead (#1791). The text is the
/// refusal of riff-server, which names the holder and its host.
///
/// ```
/// let error = anyhow::Error::new(riff::lead::Held("the lead of o/r runs on thelio".into()));
/// assert!(error.downcast_ref::<riff::lead::Held>().is_some());
/// assert_eq!(error.to_string(), "the lead of o/r runs on thelio");
/// ```
#[derive(Debug)]
pub struct Held(pub String);

impl std::fmt::Display for Held {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Held {}

/// The file of the lock of the starts of the lead, in the folder of the
/// lead ([`start_lock`], #2032).
pub const START_LOCK: &str = "start.lock";

/// Takes the lock of the starts of the lead in the folder `folder` of the
/// lead ([`START_LOCK`]): one start of the lead of a repository at a time
/// on the host. When another start holds the lock, it calls `waits`, then
/// waits for the end of that start. The lock ends when the file drops, or
/// when the process ends.
///
/// ```
/// # tokio::runtime::Runtime::new()?.block_on(async {
/// let folder = tempfile::tempdir()?;
/// let lock = riff::lead::start_lock(folder.path(), || panic!("no other start")).await?;
/// let file = std::fs::File::open(folder.path().join(riff::lead::START_LOCK))?;
/// assert!(file.try_lock().is_err(), "a second start waits");
/// drop(lock);
/// assert!(file.try_lock().is_ok());
/// # anyhow::Ok(())
/// # })?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub async fn start_lock(folder: &Path, waits: impl FnOnce()) -> Result<std::fs::File> {
    let path = folder.join(START_LOCK);
    std::fs::create_dir_all(folder).with_context(|| format!("cannot make {}", folder.display()))?;
    let file = riff_client::files::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => return Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => waits(),
        Err(std::fs::TryLockError::Error(e)) => {
            return Err(e).with_context(|| format!("cannot lock {}", path.display()));
        }
    }
    tokio::task::spawn_blocking(move || file.lock().map(|()| file))
        .await?
        .with_context(|| format!("cannot lock {}", path.display()))
}

/// What a start of the lead did ([`one_start`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launched {
    /// It started a new lead with this session ID.
    New(String),
    /// A lead of the repository ran already on the host, with this
    /// session ID: it started nothing.
    Runs(String),
}

/// One start of the lead of `repo` at a time on this host (#2032): `start`
/// runs under the lock of the starts of the lead in the folder `folder` of
/// the lead ([`start_lock`]), and only when the daemon in the folder `dir`
/// runs no lead of `repo` then. `riff lead move` and the start of the
/// daemon ([`claim_start`]) go through it, and the daemon holds the lock
/// to start a lead again (`Daemon::lead_back`). So two starts at once make
/// one container.
pub async fn one_start(
    dir: &Path,
    folder: &Path,
    repo: &str,
    start: impl Future<Output = Result<String>>,
) -> Result<Launched> {
    let _lock = start_lock(folder, || println!("{}", text::lead_start_waits_lock(repo))).await?;
    let name = name_of(repo);
    let status = daemon::status(dir).await?;
    if let Some(live) = status.sessions.iter().find(|s| s.name == name) {
        return Ok(Launched::Runs(live.session.clone()));
    }
    Ok(Launched::New(start.await?))
}

/// A new lead for [`launch`].
struct New<'a> {
    /// The main clone and its git dir.
    main: (&'a Path, &'a Path),
    /// The first prompt.
    start: &'a str,
    /// True to claim the lead only when it is free.
    if_free: bool,
    /// What starts the lead (#2099).
    cause: LeadCause,
}

/// Starts the lead of the repository of `here` on the daemon in the
/// folder `dir`, from the main clone of `new` with its git dir, with the
/// first prompt of `new`, one start at a time ([`one_start`]). It waits
/// until the container runs, and makes the new session the lead as the
/// person, with no session ID (01M5NJAYC7MXS13TA3VJB4N6RP). The new lead
/// gets the claim lead of the project (#1791): with `if_free`, only when
/// no live session holds it, else the error is [`Held`]; with no
/// `if_free`, it takes the claim from its holder. The daemon writes the
/// cause of `new` in its log, and keeps it for `riff daemon status`
/// (#2099). A failed start ends the new session.
async fn launch(
    dir: &Path,
    api: Api,
    here: &Place,
    server: &str,
    new: New<'_>,
) -> Result<Launched> {
    let repo = here.repo_text();
    let folder = folder_of(&run::data_dir()?, &repo);
    let new = launch_new(dir, api, here, server, new);
    one_start(dir, &folder, &repo, new).await
}

/// [`launch`] itself, under the lock of the starts of the lead. It gives
/// the session ID of the new lead.
async fn launch_new(
    dir: &Path,
    api: Api,
    here: &Place,
    server: &str,
    new: New<'_>,
) -> Result<String> {
    let (main, common) = new.main;
    let repo = here.repo_text();
    // A lead only for a project (#1863).
    let projects = crate::project::projects_of(here, server).await?;
    if let Some(refused) = crate::project::lead_refused(&projects, &repo) {
        return Err(refused.into());
    }
    // With no token, a sign-in file that ends soon is no token either.
    let host = run::host_facts().map_err(|fault| match fault {
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
        crate::permissions::Role::Lead,
    )?;

    let session = identity::new_session_id();
    let place = Place::new(here.host(), here.repo().clone(), None)?;
    let me: SessionUri = identity::agent(&place, &session, server)?;
    let person = api.clone().signed_in(None)?;
    let api = api.signed_in(Some(&session))?;
    api.start(&me, StartReason::Process, false).await?;
    let named = identity::person(&place, server)?;
    let led = match new.if_free {
        true => person
            .claim_lead(&named, &session)
            .await
            .and_then(|led| led.map_err(|held| Held(held).into())),
        false => person.lead(&named, &session).await,
    };
    let led = or_end(&api, &me, led).await?;
    println!("{}", text::lead_made(&repo, led.replaced.as_ref()));
    let parts = Parts {
        repo: &repo,
        host: &host,
        merged: &merged,
        folder: &folder,
        common,
        server,
        session: &session,
        me: &me,
    };
    let mut start = parts.start(spec, new.start);
    start.cause = Some(new.cause);
    or_end(&api, &me, ask_start(dir, start).await).await?;
    Ok(session)
}

/// The result `step` of a step of [`launch_new`] after the start of the
/// session `me` of the new lead. When the step failed, the session ends
/// ([`crate::wave::end_session`]), and the error stays.
async fn or_end<T>(api: &Api, me: &SessionUri, step: Result<T>) -> Result<T> {
    if step.is_err() {
        crate::wave::end_session(api, me).await;
    }
    step
}

/// What a start of the lead takes from the host and from the repository:
/// the same for a new lead ([`launch`]) and for a lead that starts again
/// ([`again`]).
struct Parts<'a> {
    /// The repository, as `OWNER/REPO`.
    repo: &'a str,
    /// The facts of the host.
    host: &'a run::Host,
    /// The image and the setup of the merged `riff.toml`.
    merged: &'a session_image::Merged,
    /// The folder of the lead: `clone/` and `claude/`.
    folder: &'a Path,
    /// The git dir of the main clone.
    common: &'a Path,
    /// The riff-server of the lead.
    server: &'a str,
    /// The session ID of the lead.
    session: &'a str,
    /// The URI of the session of the lead.
    me: &'a SessionUri,
}

impl Parts<'_> {
    /// The [`daemon::Start`] of the lead, with the spec that `spec`
    /// makes ([`spec`], or [`spec_again`]) and the first prompt `prompt`.
    fn start(&self, spec: fn(&Facts, &str, &str) -> SessionSpec, prompt: &str) -> daemon::Start {
        let host = self.host;
        let existing = |p: PathBuf| p.exists().then_some(p);
        let facts = Facts {
            session: self.session.to_owned(),
            item: ITEM.into(),
            image: self.merged.image.to_string(),
            home: host.home.clone(),
            user: host.user.clone(),
            clone: self.folder.join("clone"),
            objects: self.common.join("objects"),
            claude: self.folder.join("claude"),
            sign_in: host.claude.sign_in(),
            gitconfig: existing(host.home.join(".gitconfig")),
            setup: self.merged.setup.clone(),
        };
        daemon::Start {
            session: self.session.to_owned(),
            item: ITEM.into(),
            folder: self.folder.to_owned(),
            spec: spec(&facts, self.repo, prompt),
            start: prompt.to_owned(),
            values: host.values(),
            secrets: host.secrets(),
            server: self.server.to_owned(),
            me: self.me.clone(),
            terminal: true,
            no_github: false,
            cause: None,
        }
    }
}

/// The main clone that the clone of the lead at `clone` links to
/// (01M6QB89JJEM9Z1V3RVZYHKYT0): `git clone --reference MAIN`
/// ([`fresh_clone`]) writes `MAIN/.git/objects` in the file
/// `.git/objects/info/alternates` of the clone, and the container of the
/// lead binds that folder at the same path ([`run::spec`]).
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let clone = dir.path().join("clone");
/// let info = clone.join(".git/objects/info");
/// std::fs::create_dir_all(&info)?;
/// std::fs::write(info.join("alternates"), "/src/riff/.git/objects\n")?;
/// assert_eq!(riff::lead::linked_main(&clone)?, std::path::Path::new("/src/riff"));
/// // A clone with no link names no main clone.
/// assert!(riff::lead::linked_main(dir.path()).is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn linked_main(clone: &Path) -> Result<PathBuf> {
    let objects = clone.join(".git/objects");
    let file = objects.join("info/alternates");
    let text = std::fs::read_to_string(&file)
        .with_context(|| format!("cannot read {}", file.display()))?;
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .ok_or_else(|| anyhow::anyhow!("{} names no folder", file.display()))?;
    // A relative path is relative to the objects folder of the clone.
    let linked = objects.join(line);
    linked
        .parent()
        .and_then(Path::parent)
        .map(Path::to_owned)
        .ok_or_else(|| anyhow::anyhow!("{} has no main clone", linked.display()))
}

/// The daemon runs no wave of the repository of the lead, and the clone of
/// the lead links to no main clone: no step gives the main clone
/// (01M6QB89JJEM9Z1V3RVZYHKYT0). The text names the clone and the step
/// there is. The daemon tells riff-server the fact
/// [`riff_core::wire::BotFact::LeadNoMain`] for it.
///
/// ```
/// let error = anyhow::Error::new(riff::lead::NoMain("no main clone".into()));
/// assert!(error.downcast_ref::<riff::lead::NoMain>().is_some());
/// assert_eq!(error.to_string(), "no main clone");
/// ```
#[derive(Debug)]
pub struct NoMain(pub String);

impl std::fmt::Display for NoMain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NoMain {}

/// The [`daemon::Start`] of the lead of `record` for a start again on the
/// riff of this daemon (01M5TC2919TB9XPF42YFNJBP79): the same session, the
/// same clone and the same Claude folder, and `claude --resume ID`
/// ([`spec_again`]) with the first prompt [`text::lead_again`]. The
/// host facts, the image and the setup are those of now, as for a new
/// lead. It makes no clone and no new session on riff-server. It reads
/// the host, so call it off the async threads.
///
/// `wave` is the wave of the repository of the lead, when the daemon runs
/// one: its main clone is the main clone of the start. With no wave, the
/// main clone is the one that the clone of the lead links to
/// ([`linked_main`]). So a host that must not run the wave of a
/// repository starts its lead again all the same (#1606, 01M6QB89JJEM9Z1V3RVZYHKYT0).
/// With neither, the error is [`NoMain`].
pub fn again(wave: Option<&crate::wave::Wave>, record: &daemon::Record) -> Result<daemon::Start> {
    let here = record.me.place();
    let repo = here.repo_text();
    let folder = record.folder.clone();
    let main = match wave {
        Some(wave) => wave.main.clone(),
        None => {
            let clone = folder.join("clone");
            linked_main(&clone)
                .map_err(|_| anyhow::Error::new(NoMain(text::lead_again_no_main(&repo, &clone))))?
        }
    };
    let (main, common) = run::main_clone(&main)?;
    let host = run::host_facts().map_err(|fault| match fault {
        run::HostFault::SignInEnds(_) => anyhow::anyhow!("{}", text::LEAD_NEEDS_TOKEN),
        fault => fault.into(),
    })?;
    needs_token(&host.claude.sign_in())?;
    let merged = session_image::merged_config(&main, |_| {})?;
    let sources = session_image::merged_sources(&main)?;
    run::claude_folder(
        &folder.join("claude"),
        &folder.join("clone"),
        &host.home.join(".claude.json"),
        &crate::permissions::Trust::of(here.repo(), &record.server, &sources),
        crate::permissions::Role::Lead,
    )?;
    let parts = Parts {
        repo: &repo,
        host: &host,
        merged: &merged,
        folder: &folder,
        common: &common,
        server: &record.server,
        session: &record.live.session,
        me: &record.me,
    };
    Ok(parts.start(spec_again, &text::lead_again(&repo)))
}

/// What the daemon in the folder `dir` does for `riff lead move`
/// (#1163): it starts the lead of the repository of `wave` when none
/// runs, with the first prompt [`text::lead_moved`], and makes it the
/// lead as the person, with no session ID (01M5NJAYC7MXS13TA3VJB4N6RP).
/// `by` asked for it: the person or the lead of the person. The cause of
/// the start names the command and `by` (#2099). It gives the reply for
/// the caller, with the session ID ([`session_of`]).
pub async fn take(dir: &Path, wave: &crate::wave::Wave, by: &SessionUri) -> Result<String> {
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
            let new = New {
                main: (&main, &common),
                start: &start,
                if_free: false,
                cause: LeadCause::Command {
                    command: "riff lead move".into(),
                    by: Box::new(by.clone()),
                },
            };
            let launched = launch(dir, api, &here, &wave.server, new).await?;
            match launched {
                Launched::New(session) | Launched::Runs(session) => session,
            }
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
        let ask = text::lead_handover_ask(host);
        let no_notes = (wait, text::lead_move_no_handover as fn(_) -> _);
        await_notes(api, me, &thread, old, &ask, no_notes).await?;
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

/// Asks the live lead `lead` to write its notes with `ask`, and waits up
/// to `wait` for a change of the notes of the thread `thread`. A lead that
/// writes nothing gets the warning that `no_notes` makes, and the caller
/// goes on with the notes as they are.
async fn await_notes(
    api: &Api,
    me: &SessionUri,
    thread: &riff_core::name::ThreadName,
    lead: &str,
    ask: &str,
    (wait, no_notes): (std::time::Duration, fn(std::time::Duration) -> String),
) -> Result<()> {
    let before = api.notes_show(me, thread, false).await?;
    api.tell(me, lead, ask).await?;
    let start = tokio::time::Instant::now();
    loop {
        let now = api.notes_show(me, thread, false).await?;
        if now.at_ms != before.at_ms || now.text != before.text {
            return Ok(());
        }
        if start.elapsed() >= wait {
            eprintln!("{}", no_notes(wait));
            return Ok(());
        }
        tokio::time::sleep(HANDOVER_EVERY).await;
    }
}

/// The first step of `riff lead restart` as `me` (#1793,
/// 01M77Y6ZEQ4A6C6E0XZ56WRE89): the person, or the lead itself. It gives
/// the lead of the project: the holder of the claim lead of the repository
/// thread of `me` on riff-server ([`riff_core::wire::PlanReply::lead`]).
/// The daemon of its host ends it. It refuses when no session holds the
/// claim ([`text::lead_restart_none`]), when the lead is a session of
/// another user ([`text::lead_restart_other_user`]): only its person and
/// the lead itself act on the daemon of its host
/// (01M579Z9PZZCVWQKG7V8WZ5KRB), and when `me` is a session that is not
/// the lead ([`text::LEAD_RESTART_NOT_LEAD`]).
pub async fn to_restart(api: &Api, me: &SessionUri) -> Result<SessionUri> {
    let Some(thread) = me.default_thread() else {
        bail!("{}", text::RUN_NO_REPO);
    };
    let repo = me.place().repo_text();
    let Some(lead) = api.plan_show(me, &thread).await?.lead else {
        bail!("{}", text::lead_restart_none(&repo));
    };
    if lead.who().user() != me.who().user() {
        bail!("{}", text::lead_restart_other_user(&repo, &lead));
    }
    if let Some(own) = me.who().session()
        && lead.who().session() != Some(own)
    {
        bail!("{}", text::LEAD_RESTART_NOT_LEAD);
    }
    Ok(lead)
}

/// The notes step of `riff lead restart` as `me` (#1793,
/// 01M77Y6ZEQ4A6C6E0XZ56WRE89): it asks the lead `lead` ([`to_restart`]) to
/// write its notes, and waits up to `wait` for them, as [`move_to`] does
/// for the handover. Then the caller asks the daemon of the host of the
/// lead to end its container ([`stop_here`], or
/// [`daemon::remote::Request::LeadRestart`]), and a host that joined the
/// project starts a new lead ([`crate::lead_host`]).
///
/// ```mermaid
/// sequenceDiagram
///     participant R as riff lead restart
///     participant L as the lead
///     participant D as daemon of the host of the lead
///     participant H as daemon of a host that joined
///     R->>L: write your notes
///     L->>R: the notes change (at most `wait`)
///     R->>D: lead restart
///     D->>D: stop the container, take the part lead
///     D-->>R: done
///     H->>H: the claim lead is free: start a new lead
/// ```
///
/// A lead that writes no notes in `wait` gets a warning, and the restart
/// goes on with the notes as they are. When the caller is the lead
/// itself, it asks nobody: it cannot answer its own message, so it writes
/// its notes first.
pub async fn restart_notes(
    api: &Api,
    me: &SessionUri,
    lead: &SessionUri,
    wait: std::time::Duration,
) -> Result<()> {
    let Some(thread) = me.default_thread() else {
        bail!("{}", text::RUN_NO_REPO);
    };
    let Some(id) = lead
        .who()
        .session()
        .filter(|id| Some(*id) != me.who().session())
    else {
        return Ok(());
    };
    let ask = text::lead_restart_ask();
    let no_notes = (wait, text::lead_restart_no_notes as fn(_) -> _);
    await_notes(api, me, &thread, id, &ask, no_notes).await
}

/// The last step of `riff lead restart` on this host (#1793): the daemon in
/// the folder `dir` ends the container of the lead of `repo`, and the host
/// loses the part lead ([`daemon::Daemon::stop_lead`]). It prints each
/// line of the daemon.
pub async fn stop_here(dir: &Path, repo: &str) -> Result<()> {
    let ask = Ask::StopLead {
        repo: repo.to_owned(),
    };
    daemon::ask_done(dir, ask, |line| println!("{line}")).await
}

/// Asks the daemon in the folder `dir` to start the session of `start`
/// with a terminal, and waits until its container runs.
pub(crate) async fn ask_start(dir: &Path, start: daemon::Start) -> Result<()> {
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

/// `riff lead notes`: shows the notes of the project of `thread`, and
/// with `edit` changes them (01M77XMZBJ6DRGJTGEEE0EDECP,
/// 01M59258K3TYR09HC4AWHWZPEX).
///
/// `edit` is `None` to show. `Some(None)` opens the editor, and
/// `Some(Some(path))` takes the new text from the file `path` (`-` is the
/// standard input). A new text equal to the old text changes nothing.
pub async fn notes(
    api: &Api,
    me: &SessionUri,
    thread: &riff_core::name::ThreadName,
    history: bool,
    edit: Option<Option<&Path>>,
) -> Result<()> {
    let Some(file) = edit else {
        let reply = api.notes_show(me, thread, history).await?;
        print!("{}", text::notes_shown(&reply, thread));
        return Ok(());
    };
    let old = api.notes_show(me, thread, false).await?.text;
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

/// Sends `body` to the lead of the project of `place`, else of this
/// directory, as the person (01M78A9FSHE9XEPGC3SJAB7PMX).
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
    use crate::runtime::{End, Exit, Fake, TERMINAL_RESET};

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
        let left = attach(
            &fake,
            NAME,
            tempfile::tempdir().unwrap().path(),
            &mut fake.terminal(),
        )
        .await
        .unwrap();
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

    /// 01M78VWTRY5XX9JPQHE3CMH6T2, #1791: another session holds the claim
    /// lead of the project, so the start of a new lead fails after the
    /// start of its session. riff-server refuses the first end call of
    /// that session: the session ends with one more try ([`or_end`]), and
    /// the error stays [`Held`].
    #[tokio::test]
    async fn a_failed_lead_start_ends_its_session_also_after_a_failed_end_call() {
        use std::sync::atomic::Ordering;

        use riff_core::name::Who;

        let conflict = axum::http::StatusCode::CONFLICT;
        let (server, ends) = crate::wave::server_that_fails_ends(conflict, 1).await;
        let api = Api::new(&server);
        let place = identity::place_from_text("thelio/o/r").unwrap();
        let person = SessionUri::new(Who::new("mike", None).unwrap(), place.clone());
        let lead = |id| SessionUri::new(Who::new("mike", Some(id)).unwrap(), place.clone());
        let (first, second) = (lead("lead1"), lead("lead2"));
        for me in [&first, &second] {
            api.start(me, StartReason::Process, false).await.unwrap();
        }
        assert!(api.claim_lead(&person, "lead1").await.unwrap().is_ok());

        let held = api.claim_lead(&person, "lead2").await.unwrap();
        let held = held.map_err(|held| Held(held).into());
        let error = or_end(&api, &second, held).await.unwrap_err();
        assert!(error.downcast_ref::<Held>().is_some(), "{error:#}");
        assert_eq!(ends.load(Ordering::SeqCst), 2);
        let who = api.who(&first, false).await.unwrap();
        assert!(!who.iter().any(|s| s.uri.who() == second.who()), "{who:?}");
        // A step that worked ends no session.
        assert_eq!(or_end(&api, &first, anyhow::Ok(7)).await.unwrap(), 7);
        assert_eq!(ends.load(Ordering::SeqCst), 2);
    }

    /// 01M51PD7Q24N252V66NFEHY49N
    #[tokio::test]
    async fn the_end_of_the_lead_in_the_attach_says_that_the_lead_ended() {
        let fake = Fake::default().with_image("img").with_exit_in_attach();
        let end = lead(&fake).await;
        let left = attach(
            &fake,
            NAME,
            tempfile::tempdir().unwrap().path(),
            &mut fake.terminal(),
        )
        .await
        .unwrap();
        assert_eq!(left, Left::Ended);
        assert_eq!(left.line(NAME), text::lead_ended(NAME));
        assert_ne!(text::lead_ended(NAME), text::lead_detached(NAME));
        assert_eq!(end.wait().await.unwrap(), Exit::SUCCESS);
    }

    #[tokio::test]
    async fn a_lead_that_the_runtime_cannot_see_after_the_attach_counts_as_a_detach() {
        let fake = Fake::default().with_image("img").with_no_answer();
        let _end = lead(&fake).await;
        assert_eq!(
            attach(
                &fake,
                NAME,
                tempfile::tempdir().unwrap().path(),
                &mut fake.terminal()
            )
            .await
            .unwrap(),
            Left::Detached
        );
    }

    /// Waits until the fake has `n` attaches of `NAME`.
    async fn attaches(fake: &Fake, n: usize) {
        let attach = format!("attach {NAME}");
        let span = std::time::Instant::now();
        while fake.calls().iter().filter(|c| **c == attach).count() < n {
            assert!(span.elapsed().as_secs() < 30, "{:?}", fake.calls());
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }

    /// 01M6XTS7V47RFX1G37FRGRBWCY: the daemon starts the lead again on a new
    /// riff while the person is attached. The terminal of the person gets the
    /// line, and the attach goes on in the new container. The end of the lead
    /// after it is an end: the start again is older than that attach.
    #[tokio::test]
    async fn an_attached_terminal_gets_the_line_when_the_daemon_starts_the_lead_again() {
        let fake = Fake::default().with_image("img").with_attach_until_end();
        let folder = tempfile::tempdir().unwrap();
        let _old = lead(&fake).await;
        let daemon = {
            let (fake, folder) = (fake.clone(), folder.path().to_owned());
            tokio::spawn(async move {
                attaches(&fake, 1).await;
                // The daemon of the new riff: the mark, then the stop.
                let mut again = Again {
                    riff: "v3.3.0".into(),
                    at_ms: daemon::now_ms(),
                    started: false,
                };
                again.write(&folder).unwrap();
                fake.stop(NAME).await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                let new = lead(&fake).await;
                again.started = true;
                again.write(&folder).unwrap();
                // The person ends the lead in the new container.
                attaches(&fake, 2).await;
                fake.stop(NAME).await.unwrap();
                new
            })
        };
        let left = attach(&fake, NAME, folder.path(), &mut fake.terminal())
            .await
            .unwrap();
        let _new = daemon.await.unwrap();
        assert_eq!(left, Left::Ended);
        // 01M6XVBRQMBMK68K8GCDRZ5GJ5: the reset of the modes after each
        // attach, before the line of the start again.
        assert_eq!(
            String::from_utf8(fake.screen()).unwrap(),
            format!(
                "{}\n{TERMINAL_RESET}{}\n{TERMINAL_RESET}",
                text::LEAD_DETACH,
                text::lead_again_attached("v3.3.0")
            )
        );
        assert_eq!(
            fake.calls(),
            [
                format!("terminal {NAME}"),
                format!("redraw {NAME}"),
                format!("attach {NAME}"),
                format!("stop {NAME}"),
                format!("terminal {NAME}"),
                format!("redraw {NAME}"),
                format!("attach {NAME}"),
                format!("stop {NAME}"),
            ]
        );
    }

    /// 01M6XTS7V47RFX1G37FRGRBWCY: a start again that fails removes the mark:
    /// the attach ends at once with the end of the lead, and waits no longer.
    #[tokio::test]
    async fn an_attach_waits_no_more_when_the_start_again_fails() {
        let fake = Fake::default().with_image("img").with_attach_until_end();
        let folder = tempfile::tempdir().unwrap();
        let _old = lead(&fake).await;
        let daemon = {
            let (fake, folder) = (fake.clone(), folder.path().to_owned());
            tokio::spawn(async move {
                attaches(&fake, 1).await;
                let again = Again {
                    riff: "v3.3.0".into(),
                    at_ms: daemon::now_ms(),
                    started: false,
                };
                again.write(&folder).unwrap();
                fake.stop(NAME).await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                Again::clear(&folder);
            })
        };
        let span = std::time::Instant::now();
        let left = attach(&fake, NAME, folder.path(), &mut fake.terminal())
            .await
            .unwrap();
        daemon.await.unwrap();
        assert_eq!(left, Left::Ended);
        assert_eq!(
            String::from_utf8(fake.screen()).unwrap(),
            format!("{}\n{TERMINAL_RESET}", text::LEAD_DETACH)
        );
        assert!(span.elapsed() < AGAIN_WAIT / 4, "{:?}", span.elapsed());
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
            setup: Default::default(),
        };
        let spec = spec(&facts, "como-technologies/riff", "start");
        (fake.start_terminal(&spec, &[]).await.unwrap(), file)
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

    /// The fake runs the lead `NAME` with a bind of the file `riff`, as
    /// the daemon binds its own riff at [`daemon::RELAY_BIN`].
    async fn lead_with_riff(fake: &Fake, riff: &Path) -> End {
        let mut spec = SessionSpec {
            name: NAME.into(),
            image: "img".into(),
            home: "/home/mike".into(),
            workdir: "/w".into(),
            binds: vec![crate::runtime::Bind {
                source: crate::runtime::Source::Path(riff.to_owned()),
                target: daemon::RELAY_BIN.into(),
                write: false,
            }],
            env: vec![],
            memory: "8g".into(),
            cpus: 4,
            command: vec![],
        };
        spec.command = vec!["claude".into()];
        fake.start_terminal(&spec, &[]).await.unwrap()
    }

    /// 01M5TC24PB3YC2CC8BA275WSEN
    #[tokio::test]
    async fn a_lead_is_old_when_riff_update_replaced_the_riff_that_its_container_binds() {
        let dir = tempfile::tempdir().unwrap();
        let riff = dir.path().join("riff");
        std::fs::write(&riff, "4.4.0").unwrap();
        let fake = Fake::default().with_image("img");
        let _end = lead_with_riff(&fake, &riff).await;
        assert!(!old(&fake, NAME).await);
        // `riff update` writes a new file and renames it over the old one.
        let new = dir.path().join("riff.new");
        std::fs::write(&new, "4.5.0").unwrap();
        std::fs::rename(&new, &riff).unwrap();
        assert!(old(&fake, NAME).await);
        // A new container binds the new file.
        fake.stop(NAME).await.unwrap();
        let _again = lead_with_riff(&fake, &riff).await;
        assert!(!old(&fake, NAME).await);
    }

    /// 01M5TC24PB3YC2CC8BA275WSEN
    #[tokio::test]
    async fn a_lead_that_does_not_run_or_that_the_runtime_cannot_see_is_not_old() {
        let fake = Fake::default().with_image("img");
        assert!(!old(&fake, NAME).await);
        let dir = tempfile::tempdir().unwrap();
        let riff = dir.path().join("riff");
        std::fs::write(&riff, "4.4.0").unwrap();
        // A lead that binds no riff: the runtime has no such file.
        let _end = lead(&fake).await;
        assert!(!old(&fake, NAME).await);
    }

    /// 01M5TC24PB3YC2CC8BA275WSEN
    #[test]
    fn a_lead_is_quiet_after_its_transcript_got_no_write_for_the_idle_time() {
        use std::time::{Duration, SystemTime};
        let dir = tempfile::tempdir().unwrap();
        let mut record = record_in(dir.path());
        let idle = Duration::from_secs(300);
        let ago = |secs: u64| SystemTime::now() - Duration::from_secs(secs);
        // No transcript yet: the start of the lead counts.
        record.live.since_ms = i64::try_from(
            ago(10)
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        assert!(!quiet(&record, idle));
        record.live.since_ms = 0;
        assert!(quiet(&record, idle));
        // A transcript of the lead, and one of another session.
        let project = dir.path().join("claude/projects/-w-clone");
        std::fs::create_dir_all(&project).unwrap();
        let own = std::fs::File::create(project.join("s1.jsonl")).unwrap();
        let other = std::fs::File::create(project.join("s2.jsonl")).unwrap();
        other.set_modified(ago(0)).unwrap();
        own.set_modified(ago(600)).unwrap();
        assert!(
            quiet(&record, idle),
            "a write to another session is no turn of this one"
        );
        own.set_modified(ago(30)).unwrap();
        assert!(!quiet(&record, idle));
        // The start of a lead after its transcript counts as the last write.
        own.set_modified(ago(600)).unwrap();
        record.live.since_ms = i64::try_from(
            ago(20)
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        assert!(!quiet(&record, idle));
    }

    /// 01M5TC2919TB9XPF42YFNJBP79, 01M6VAQWECJ6N6VJF66KCMNYD1: both
    /// start with the model of the `riff.toml`.
    #[test]
    fn the_spec_of_a_lead_that_starts_again_resumes_the_conversation_in_the_same_place() {
        let facts = Facts {
            session: "1a2b3c4d-0000-4000-8000-000000000000".into(),
            item: ITEM.into(),
            image: "img".into(),
            home: "/home/mike".into(),
            user: "mike".into(),
            clone: "/d/riff/leads/o-r/clone".into(),
            objects: "/src/r/.git/objects".into(),
            claude: "/d/riff/leads/o-r/claude".into(),
            sign_in: run::ClaudeSignIn::Token,
            gitconfig: None,
            setup: crate::session_setup::Setup::of_config(
                "[session]\nmodel = \"sonnet\"\n",
                "riff.toml",
            )
            .unwrap(),
        };
        let first = spec(&facts, "o/r", "start");
        let again = spec_again(&facts, "o/r", "go on");
        // Only the session flag and the prompt differ.
        assert_eq!(again.binds, first.binds);
        assert_eq!(again.env, first.env);
        assert_eq!(again.workdir, first.workdir);
        assert_eq!(again.name, first.name);
        assert_eq!(
            first.command[..4],
            ["claude", "--model", "sonnet", "--session-id"]
        );
        assert_eq!(
            again.command,
            [
                "claude",
                "--model",
                "sonnet",
                "--resume",
                &facts.session,
                "--dangerously-skip-permissions",
                "go on"
            ]
        );
    }

    /// A record of the lead `NAME` with its folder in `dir`.
    fn record_in(dir: &Path) -> daemon::Record {
        daemon::Record {
            live: riff_client::sessions::Live {
                digest: String::new(),
                name: NAME.into(),
                session: "s1".into(),
                item: ITEM.into(),
                image: "img".into(),
                since_ms: 0,
                phase: riff_client::sessions::Phase::Terminal,
                last: String::new(),
                lead: true,
                cause: None,
            },
            folder: dir.to_owned(),
            server: "http://127.0.0.1:1".into(),
            me: "riff://mike@thelio/como-technologies/riff?session=s1"
                .parse()
                .unwrap(),
            inputs: vec![],
            closed: false,
            failed: false,
            job: None,
            terminal: true,
            takeover: false,
            interrupt: false,
            model: None,
            github: None,
        }
    }

    /// 01M77Y6ZQ76DTX470STN8ATTA9
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

    /// 01M6XVBRQMBMK68K8GCDRZ5GJ5: a failed attach also ends with the
    /// reset.
    #[tokio::test]
    async fn an_attach_to_a_lead_that_does_not_run_fails() {
        let fake = Fake::default().with_image("img");
        assert!(
            attach(
                &fake,
                NAME,
                tempfile::tempdir().unwrap().path(),
                &mut fake.terminal()
            )
            .await
            .is_err()
        );
        assert!(fake.screen().ends_with(TERMINAL_RESET.as_bytes()));
    }

    /// 01M74BVSFDAE67FA1PRYK7E2PN, #1855: `riff lead start` reads a status
    /// line in which item 724 of a wave waits for the pull request #728,
    /// and attaches to the lead that runs.
    #[tokio::test]
    async fn a_lead_start_reads_a_status_with_a_wait_and_attaches_to_the_lead() {
        let fake = Fake::default().with_image("img");
        let _end = lead(&fake).await;
        let root = tempfile::tempdir().unwrap();
        let dir = daemon::dir_in(root.path());
        let said = daemon::says_once(&dir, daemon::A_WAIT).await;
        let repo = riff_core::name::Repo::Git {
            owner: "como-technologies".into(),
            name: "riff".into(),
        };
        let here = Place::new("thelio", repo, None).unwrap();
        let server = "http://127.0.0.1:9";
        start_with(&dir, &fake, Api::new(server), &here, server)
            .await
            .unwrap();
        said.await.unwrap();
        assert!(
            fake.calls().contains(&format!("attach {NAME}")),
            "{:?}",
            fake.calls()
        );
    }

    /// The bytes of a `claude` with its fullscreen renderer: the
    /// alternate screen, and the mouse reports of each click, drag and
    /// move, in the SGR form.
    const MOUSE_ON: &[u8] = b"\x1b[?1049h\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h";

    /// 01M6XVBRQMBMK68K8GCDRZ5GJ5
    #[tokio::test]
    async fn after_a_detach_the_output_ends_with_the_reset_of_the_modes() {
        let fake = Fake::default()
            .with_image("img")
            .with_terminal_bytes(MOUSE_ON);
        let _end = lead(&fake).await;
        let left = attach(
            &fake,
            NAME,
            tempfile::tempdir().unwrap().path(),
            &mut fake.terminal(),
        )
        .await
        .unwrap();
        assert_eq!(left, Left::Detached);
        let mut want = format!("{}\n", text::LEAD_DETACH).into_bytes();
        want.extend_from_slice(MOUSE_ON);
        want.extend_from_slice(TERMINAL_RESET.as_bytes());
        assert_eq!(fake.screen(), want);
    }

    /// 01M6XVBRQMBMK68K8GCDRZ5GJ5
    #[tokio::test]
    async fn after_the_end_of_the_lead_the_output_ends_with_the_reset_of_the_modes() {
        let fake = Fake::default()
            .with_image("img")
            .with_exit_in_attach()
            .with_terminal_bytes(MOUSE_ON);
        let _end = lead(&fake).await;
        let left = attach(
            &fake,
            NAME,
            tempfile::tempdir().unwrap().path(),
            &mut fake.terminal(),
        )
        .await
        .unwrap();
        assert_eq!(left, Left::Ended);
        let screen = fake.screen();
        assert!(screen.ends_with(TERMINAL_RESET.as_bytes()), "{screen:?}");
        assert!(screen.windows(MOUSE_ON.len()).any(|w| w == MOUSE_ON));
    }

    /// The transcript of the lead of `record`, with its last write `ago`
    /// before now.
    fn written_ago(record: &daemon::Record, ago: std::time::Duration) {
        let project = record.folder.join("claude/projects/-d-clone");
        std::fs::create_dir_all(&project).unwrap();
        let file = std::fs::File::create(project.join("s1.jsonl")).unwrap();
        file.set_modified(std::time::SystemTime::now() - ago)
            .unwrap();
    }

    /// The end of the last turn of the lead of `record`, `ago` before now.
    fn ended_ago(record: &daemon::Record, ago: std::time::Duration) {
        end_turn(&record.folder).unwrap();
        let file = std::fs::File::options()
            .write(true)
            .open(record.folder.join(TURN_ENDED))
            .unwrap();
        file.set_modified(std::time::SystemTime::now() - ago)
            .unwrap();
    }

    /// 01M6WBX2FQEHW92Y1A5HM4H8BM: a lead is idle at the end of its turn,
    /// also when its transcript got a write a moment ago. A write after
    /// the tail of the turn is a new turn. A lead with no end of a turn
    /// keeps the rule of the quiet transcript.
    #[test]
    fn a_lead_is_idle_at_the_end_of_its_turn() {
        use std::time::Duration;
        let dir = tempfile::tempdir().unwrap();
        let record = record_in(dir.path());
        let quiet = Duration::from_secs(300);

        // No `Stop` hook yet: a write 10 s ago is a turn.
        written_ago(&record, Duration::from_secs(10));
        assert!(!between_turns(&record));
        assert!(!idle(&record, quiet));

        // The turn ended 8 s ago, and `claude` wrote the end of the turn
        // a moment after the hook: the lead is between two turns.
        ended_ago(&record, Duration::from_secs(8));
        written_ago(&record, Duration::from_secs(7));
        assert!(between_turns(&record));
        assert!(idle(&record, quiet));
        let kept: i64 = std::fs::read_to_string(dir.path().join(TURN_ENDED))
            .unwrap()
            .parse()
            .unwrap();
        assert!(kept > 0);

        // A wake started a new turn: a write after the tail.
        written_ago(&record, Duration::from_secs(1));
        assert!(!between_turns(&record));
        assert!(!idle(&record, quiet));

        // The end of a turn of an older container does not count.
        let mut again = record.clone();
        again.live.since_ms = daemon::now_ms();
        ended_ago(&again, Duration::from_secs(60));
        std::fs::remove_dir_all(dir.path().join("claude")).unwrap();
        assert!(!between_turns(&again));

        // With no end of a turn, the quiet rule still holds.
        std::fs::remove_file(dir.path().join(TURN_ENDED)).unwrap();
        written_ago(&record, Duration::from_secs(600));
        assert!(!between_turns(&record));
        assert!(idle(&record, quiet));
    }
}
