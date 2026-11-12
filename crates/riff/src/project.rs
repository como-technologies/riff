//! `riff project init`, `riff project list` and `riff project remove`:
//! the projects of the riff (01M76SG2E9RPV4B7CYSZPGGZ3P).
//!
//! # Design
//!
//! A project is a repository `OWNER/REPO` on GitHub that a person
//! enrolled in the riff. riff-server keeps the projects in its log:
//! who enrolled each one and when, and the hosts that may run its wave.
//! `riff project init` sets up the repository of a clone for riff, then
//! enrolls it (01M76SHAX0KR0DM9GPXAPZJRDZ):
//!
//! ```mermaid
//! flowchart LR
//!     R{"a clone with origin<br/>on GitHub?"} -- "no" --> N["stop, and say so"]
//!     R -- "yes" --> C["the checks of the repository<br/>with gh, and the Apps<br/>from riff-server"]
//!     C -- "a step to do" --> Q["one question:<br/>y, or --yes"]
//!     Q --> S["the steps, with gh,<br/>then git pull and<br/>the checks again"]
//!     S --> E{"each check<br/>passed?"}
//!     C -- "all done" --> E
//!     E -- "no" --> F["the fix of each check,<br/>no project, exit 1"]
//!     E -- "yes" --> P["POST /v1/project/init<br/>with the token of gh"]
//!     P --> G["riff-server reads the<br/>repository on GitHub, and<br/>enrolls it or sets its hosts"]
//! ```
//!
//! 1. The checks ([`checks`]): the facts of the repository on GitHub
//!    ([`crate::repo_setup`]), and the GitHub Apps of riff on the
//!    repository ([`apps_check`], 01M6Z87JTXW0Y99X08VHA2SSSR). riff asks
//!    riff-server for the bot of the session App, for the check of the
//!    guard, and for the lead App, for the check of the ruleset
//!    `releases`. A line with `step` names a step that riff can do now
//!    ([`crate::first_run::Check::planned_line`],
//!    01M5RGG9KJXCKH8C94145WS8SM).
//! 2. A note names the open issues with no "Done when" line
//!    ([`text::init_no_done_when`], 01M5SMMP7ZW2PZNGK63WWZAC8P). It fails
//!    no check.
//! 3. The steps ([`run`]): for a person who is an admin of the
//!    repository, after one question ([`Question::Setup`]), or with no
//!    question with `--yes` (01M5R1672NF56P15VD0JEX51K4). `--check` does
//!    none. After the step of `riff.toml`, riff runs `git pull
//!    --ff-only` in the clone when it is safe ([`pull`],
//!    01M5RGGD9AYNC12Z2CG5CDZWNA), and checks again
//!    (01M5RGGD6YR36EXZEG61XCJVRY).
//! 4. When a check still fails, riff enrolls nothing, names the fix of
//!    each failed check, and exits with 1. `--check` stops after the
//!    checks, and enrolls nothing.
//! 5. The enrollment ([`init`]): the call `POST /v1/project/init` with the
//!    hosts of `--host` and the token of `gh` of the person ([`gh_token`],
//!    01M76SHB27S4QDV9HMRR4B6APJ). riff-server reads the repository on
//!    GitHub with the token one time, and needs an admin of the riff who
//!    is an admin of the repository (01M76SG8GBXFYXKWMQHPCNEXBN). A second
//!    run sets the hosts again and changes nothing else.
//!
//! `riff project remove OWNER/REPO` asks riff-server to forget the
//! project with the same check, and keeps the repository on GitHub as it
//! is. `riff project init` and `riff project remove` refuse in an agent
//! session and in a container, before each call ([`refusal`]). `riff
//! project list` prints each project ([`text::project_list`],
//! 01M76SHB4SAWFW3S5Y7X9KC6E4): in a container, through the socket of the
//! session ([`crate::reach::Reach::projects`]).
//!
//! # A wave and a lead only for a project
//!
//! The daemon of a host runs the wave of a repository only for a project
//! that allows the host ([`wave_refused`], #1863). `riff host join` asks
//! riff-server for the projects before each other step, and refuses with
//! the fix ([`wave_allowed`]). The loop of the wave asks at each look, so
//! a host that the project no longer allows starts no new session, and
//! `riff daemon status` gives the reason ([`Refused::word`]). A lead
//! starts only for a project ([`lead_refused`]). So a session in a
//! repository that is no project has the chat and the messages only.
//!
//! ```mermaid
//! flowchart LR
//!     J["riff host join,<br/>a look of the wave"] --> P{"a project?"}
//!     P -- "no" --> N["no wave, no lead:<br/>riff project init"]
//!     P -- "yes" --> H{"no hosts,<br/>or this host?"}
//!     H -- "no" --> M["no new session here:<br/>riff project init --host HOST"]
//!     H -- "yes" --> W["the daemon runs the wave"]
//! ```
//!
//! ```
//! use riff::project::{Facts, checks};
//!
//! let unread = Facts {
//!     github: Err("gh: not logged in".into()),
//!     no_done_when: None,
//!     folder: "/home/mike/app".into(),
//!     apps: None,
//! };
//! let lines: Vec<String> = checks(&unread).iter().map(|c| c.line()).collect();
//! assert_eq!(lines.len(), 1);
//! assert!(lines[0].starts_with("fail  the repository on GitHub: "), "{}", lines[0]);
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use riff_client::api::Api;
use riff_core::name::{Place, ThreadName};
use riff_core::wire::ProjectFacts;

use crate::first_run::{Check, Mode, Question};
use crate::repo_setup::{self, Forge, Github};
use crate::{identity, session_image, text};

/// The facts of the repository of a clone that `riff project init`
/// checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// The facts of the repository on GitHub that riff reads
    /// ([`crate::repo_setup`]), or why it cannot.
    pub github: Result<Github, String>,
    /// The numbers of the open issues of the repository that have no
    /// "Done when" line ([`crate::repo_setup::no_done_when`],
    /// 01M5SMMP7ZW2PZNGK63WWZAC8P), or why riff cannot list the issues.
    /// `None` when riff did not read the repository.
    pub no_done_when: Option<Result<Vec<u64>, String>>,
    /// The folder of the clone, where `git pull` runs after the step of
    /// `riff.toml` (01M5RGGD9AYNC12Z2CG5CDZWNA).
    pub folder: PathBuf,
    /// What riff-server says of the GitHub Apps of riff on the
    /// repository ([`apps_of`], 01M6Z87JTXW0Y99X08VHA2SSSR). `None` when
    /// riff did not ask: no sign-in, or no answer.
    pub apps: Option<Apps>,
}

/// What riff-server says of the GitHub Apps of riff on one repository,
/// from the check of the token of each role (01M6WG534PPE2JW8JQFRJDEK86).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Apps {
    /// The repository, `OWNER/NAME`.
    pub repo: String,
    /// Each App that is not on the repository, as riff-server names it,
    /// with its fix: `riff-acme is not installed on acme/app: riff github
    /// install acme`.
    pub missing: Vec<String>,
}

/// The check of the GitHub Apps of riff on a repository: no worker and
/// no verify starts there while an App is not on it, and the check names
/// the same fix (01M6Z87A04EXTT1FFJSJ1EM6TB, 01M6Z87JTXW0Y99X08VHA2SSSR).
///
/// ```
/// use riff::project::{Apps, apps_check};
///
/// let on = Apps { repo: "acme/app".into(), missing: vec![] };
/// assert_eq!(apps_check(&on).line(), "pass  the GitHub Apps of riff: on acme/app");
/// let missing = Apps {
///     repo: "acme/app".into(),
///     missing: vec!["riff-acme is not installed on acme/app: riff github install acme".into()],
/// };
/// assert_eq!(
///     apps_check(&missing).line(),
///     "fail  the GitHub Apps of riff: riff-acme is not installed on acme/app. Fix: riff github install acme"
/// );
/// ```
pub fn apps_check(apps: &Apps) -> Check {
    match apps.missing.first() {
        None => Check::pass(text::INIT_APPS, text::init_apps_on(&apps.repo)),
        Some(missing) => {
            let (found, fix) = missing
                .rsplit_once(": ")
                .unwrap_or((missing.as_str(), "riff github check"));
            Check::fail(text::INIT_APPS, found.to_owned(), fix.to_owned())
        }
    }
}

/// How long `riff project init` waits for riff-server to check the
/// GitHub Apps.
const APPS_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// What riff-server says of the GitHub Apps of riff on `repo`
/// ([`Apps`]), asked with `api`: each role with an App that is not on the
/// repository. A riff with no App gives no check: its sessions keep the
/// token of the person by design. `None` when riff cannot ask.
pub async fn apps_of(api: &Api, repo: &str) -> Option<Apps> {
    if !api.sends_token() {
        return None;
    }
    let checked = tokio::time::timeout(APPS_WAIT, api.github_check(repo))
        .await
        .ok()?
        .ok()?;
    let missing = checked
        .roles
        .into_iter()
        .filter_map(|role| role.problem)
        .filter(|problem| problem.contains(": riff github install "))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    Some(Apps {
        repo: repo.to_owned(),
        missing,
    })
}

/// Each check of `riff project init` on `facts`, in order: the facts of
/// the repository on GitHub, then the GitHub Apps of riff. An `Err` of
/// the repository is one failed check.
pub fn checks(facts: &Facts) -> Vec<Check> {
    let mut checks = repo_setup::checks_of(&facts.github);
    checks.extend(facts.apps.iter().map(apps_check));
    checks
}

/// The end of the checks and the steps of [`run`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A check failed: riff enrolls nothing, and exits with 1.
    Failed,
    /// Each check passed with `--check`: riff enrolls nothing.
    Checked,
    /// Each check passed: riff enrolls the repository.
    Ready,
}

/// The checks and the steps of `riff project init` on `facts`, in the
/// forge `forge`. It prints each check, the note of the issues with no
/// "Done when" line, and the steps. When a check of the repository
/// failed and has a step, and the person is an admin, it names the steps
/// and does them: after the question [`Question::Setup`] to `ask` in
/// [`Mode::Ask`], and with no question in [`Mode::Yes`]
/// (01M5R1672NF56P15VD0JEX51K4). It prints the list of the checks again
/// after the steps, and the end of a failed run names each failed check
/// with its fix. With [`Mode::Check`], it does no step.
pub fn run(
    facts: &Facts,
    mode: Mode,
    mut ask: impl FnMut(Question) -> Result<bool>,
    forge: &Forge,
) -> Result<Outcome> {
    println!("{}", text::PROJECT_CHECKS);
    let mut list = checks(facts);
    // A step fixes a failed check only when riff can do the step now
    // (01M5RGG9KJXCKH8C94145WS8SM).
    let doable = match &facts.github {
        Ok(github) if mode != Mode::Check && github.admin => Some(github),
        _ => None,
    };
    for check in &list {
        println!(
            "  {}",
            doable.map_or_else(|| check.line(), |github| check.planned_line(github))
        );
    }
    if let (Ok(github), Some(issues)) = (&facts.github, &facts.no_done_when) {
        match issues {
            Ok(numbers) if !numbers.is_empty() => {
                println!("{}", text::init_no_done_when(&github.repo, numbers));
            }
            Ok(_) => {}
            Err(why) => println!("{}", text::init_issues_unread(&github.repo, why)),
        }
    }
    let after = match mode {
        Mode::Check => None,
        Mode::Ask | Mode::Yes => set_up(facts, &list, mode, &mut ask, forge)?,
    };
    if let Some(facts) = &after {
        // The second list: each step is done, so a check that fails has no
        // step to wait for (01M5RGGD6YR36EXZEG61XCJVRY).
        list = checks(facts);
        println!("{}", text::PROJECT_CHECKS_AGAIN);
        for check in &list {
            println!("  {}", check.line());
        }
    }
    let failed: Vec<(&str, &str)> = list
        .iter()
        .filter_map(|c| Some((c.name.as_str(), c.fix.as_deref()?)))
        .collect();
    if !failed.is_empty() {
        println!("{}", text::project_failed(&failed));
        return Ok(Outcome::Failed);
    }
    if mode == Mode::Check {
        println!("{}", text::PROJECT_CHECKED);
        return Ok(Outcome::Checked);
    }
    Ok(Outcome::Ready)
}

/// The steps of the failed checks `list` that set up the repository on
/// GitHub (01M5R1672NF56P15VD0JEX51K4). It names them, and does them
/// in `forge` when the person is an admin, and says yes: always in
/// [`Mode::Yes`], else after the question [`Question::Setup`]. It
/// returns the facts after the steps, or `None` when it did no step.
/// When the step made `riff.toml` and the clone has none, it gives the
/// clone the file with `git pull` ([`pull`]) when it is safe.
fn set_up(
    facts: &Facts,
    list: &[Check],
    mode: Mode,
    ask: &mut impl FnMut(Question) -> Result<bool>,
    forge: &Forge,
) -> Result<Option<Facts>> {
    let Ok(github) = &facts.github else {
        return Ok(None);
    };
    let steps = repo_setup::steps_of(list);
    if steps.is_empty() {
        return Ok(None);
    }
    if !github.admin {
        println!("{}", text::init_not_admin(&github.repo));
        return Ok(None);
    }
    let titles: Vec<String> = steps.iter().map(|s| s.title(github)).collect();
    println!("{}", text::init_setup(&github.repo, &titles));
    if mode != Mode::Yes && !ask(Question::Setup)? {
        println!("{}", text::INIT_SETUP_KEPT);
        return Ok(None);
    }
    let mut after = github.clone();
    for (step, done) in repo_setup::setup(forge, &steps, &mut after) {
        let title = step.title(github);
        println!(
            "  {}",
            match &done {
                Ok(what) => text::init_step_done(&title, what),
                Err(why) => text::init_step_failed(&title, why),
            }
        );
        // The base has no language: the project adds its toolchain.
        if step == repo_setup::Step::Config && done.is_ok() && forge.image.is_none() {
            println!("  {}", text::init_toolchain(&forge.pin()));
        }
    }
    let made = !github.has(session_image::FILE) && after.has(session_image::FILE);
    if made && session_image::of_folder(&facts.folder).is_err() {
        // The new riff.toml is on the default branch, not yet in this clone.
        let branch = &github.default_branch;
        match pull(&facts.folder, branch) {
            Ok(()) => println!("{}", text::init_pulled(branch)),
            Err(why) => println!("{}", text::init_not_pulled(branch, &why)),
        }
    }
    Ok(Some(Facts {
        github: Ok(after),
        ..facts.clone()
    }))
}

/// Gives the clone in `clone` the commits of `origin` with `git pull
/// --ff-only origin BRANCH` (01M5RGGD9AYNC12Z2CG5CDZWNA), when it is safe:
/// the clone is on the branch `branch`, and has no change. A clone with
/// commits that `origin` lacks does not fast-forward, so git stops and
/// changes nothing. It gives why riff ran no pull, or why git failed, in
/// one line.
///
/// ```
/// use riff::project::pull;
///
/// let dir = tempfile::tempdir()?;
/// let git = |dir: &std::path::Path, args: &[&str]| {
///     let status = std::process::Command::new("git")
///         .args(["-c", "user.name=a", "-c", "user.email=a@b.c", "-C"])
///         .arg(dir)
///         .args(args)
///         .status()
///         .unwrap();
///     assert!(status.success(), "git {args:?}");
/// };
/// let (origin, clone) = (dir.path().join("origin"), dir.path().join("clone"));
/// git(dir.path(), &["init", "-q", "-b", "main", "origin"]);
/// git(&origin, &["commit", "-q", "--allow-empty", "-m", "first"]);
/// git(dir.path(), &["clone", "-q", "origin", "clone"]);
/// git(&origin, &["commit", "-q", "--allow-empty", "-m", "second"]);
///
/// assert_eq!(pull(&clone, "dev"), Err("the clone is on main, not on dev".to_owned()));
/// std::fs::write(clone.join("note"), "")?;
/// assert_eq!(pull(&clone, "main"), Err("the clone has a change".to_owned()));
/// std::fs::remove_file(clone.join("note"))?;
/// assert_eq!(pull(&clone, "main"), Ok(()));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn pull(clone: &Path, branch: &str) -> Result<(), String> {
    let git = |args: &[&str]| session_image::git(clone, args).map_err(|e| format!("{e:#}"));
    let on = git(&["symbolic-ref", "--quiet", "--short", "HEAD"]).unwrap_or_default();
    if on != branch {
        return Err(text::init_clone_branch(&on, branch));
    }
    if !git(&["status", "--porcelain"])?.is_empty() {
        return Err(text::INIT_CLONE_CHANGED.to_owned());
    }
    git(&["pull", "--ff-only", "origin", branch]).map(drop)
}

/// Why this process may not run `riff project COMMAND`, or `None`
/// (01M76SHB27S4QDV9HMRR4B6APJ): an agent session, or a container.
///
/// ```
/// let refused = riff::project::refusal_for("init", true, false).unwrap();
/// assert!(refused.contains("a session never runs riff project init"), "{refused}");
/// let refused = riff::project::refusal_for("remove", false, true).unwrap();
/// assert!(refused.contains("a container never runs riff project remove"), "{refused}");
/// assert!(riff::project::refusal_for("init", false, false).is_none());
/// ```
pub fn refusal_for(command: &str, agent: bool, container: bool) -> Option<String> {
    (agent || container).then(|| text::project_refused(command, container))
}

/// [`refusal_for`] this process.
pub fn refusal(command: &str) -> Option<String> {
    refusal_for(
        command,
        crate::cycle::in_agent(),
        crate::cycle::in_container(),
    )
}

/// The token of `gh` of the person (01M76SHB27S4QDV9HMRR4B6APJ): `gh_var`
/// (`GH_TOKEN`), else `github_var` (`GITHUB_TOKEN`), else what
/// `from_gh` reads with `gh auth token`. An empty value counts as none.
///
/// ```
/// use riff::project::token_from;
///
/// let gh = || Ok("gho_of_gh".to_owned());
/// assert_eq!(*token_from(Some("gho_a".into()), Some("gho_b".into()), gh)?, "gho_a");
/// assert_eq!(*token_from(Some(String::new()), Some("gho_b".into()), gh)?, "gho_b");
/// assert_eq!(*token_from(None, None, gh)?, "gho_of_gh");
/// let none = token_from(None, None, || anyhow::bail!("no oauth token")).unwrap_err();
/// assert!(format!("{none:#}").contains("gh auth login"), "{none:#}");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn token_from(
    gh_var: Option<String>,
    github_var: Option<String>,
    from_gh: impl FnOnce() -> Result<String>,
) -> Result<zeroize::Zeroizing<String>> {
    let set = |value: Option<String>| value.filter(|v| !v.trim().is_empty());
    let token = match set(gh_var).or_else(|| set(github_var)) {
        Some(token) => token,
        None => from_gh().context(text::PROJECT_NO_TOKEN)?,
    };
    Ok(zeroize::Zeroizing::new(token.trim().to_owned()))
}

/// [`token_from`] this process, with the `gh` of the machine in the
/// folder `dir`. riff gives the token only to riff-server.
pub fn gh_token(dir: &Path) -> Result<zeroize::Zeroizing<String>> {
    token_from(
        std::env::var("GH_TOKEN").ok(),
        std::env::var("GITHUB_TOKEN").ok(),
        || crate::run::output("gh", dir, &["auth", "token"]),
    )
}

/// The person on this host, with no repository: the caller of a call of
/// the projects.
fn person(api: &Api, dir: &Path) -> Result<riff_core::name::SessionUri> {
    let here = identity::place(dir)?;
    identity::person(&Place::host_only(here.host())?, api.base())
}

/// `riff project init`: the checks and the steps of the repository of
/// the current folder, then its enrollment with the hosts `hosts` on the
/// riff-server `server` (01M76SHAX0KR0DM9GPXAPZJRDZ). `image` is the
/// session image that a new `riff.toml` pins in place of the base of the
/// release (`--image`, 01M5SNTPEP41NMSP3SEKR7KVB0). It asks its question
/// on the terminal, and gives the exit code.
pub async fn init(
    mode: Mode,
    image: Option<session_image::Image>,
    hosts: &[String],
    server: &str,
) -> Result<i32> {
    let here = identity::working_dir()?;
    let Some(origin) = identity::github_origin(&here) else {
        anyhow::bail!("{}", text::PROJECT_NO_ORIGIN);
    };
    let repo: ThreadName = origin.parse()?;
    let api = Api::new(server).signed_in(None);
    let (bot, lead, apps) = match &api {
        Ok(api) => (
            crate::guard::session_bot(api).await,
            repo_setup::lead_app(api).await,
            apps_of(api, &origin).await,
        ),
        Err(e) => {
            let why = format!("{e:#}");
            (
                Err(text::guard_unasked(server, &why)),
                Err(text::lead_unasked(server, &why)),
                None,
            )
        }
    };
    let forge = Forge {
        image,
        ..Forge::of(&origin)
    };
    let github = repo_setup::read(&forge.gh, bot, lead);
    let no_done_when = github
        .is_ok()
        .then(|| repo_setup::no_done_when(&forge.gh).map_err(|e| format!("{e:#}")));
    let facts = Facts {
        github,
        no_done_when,
        folder: here.clone(),
        apps,
    };
    match run(&facts, mode, crate::first_run::ask_here, &forge)? {
        Outcome::Failed => return Ok(1),
        Outcome::Checked => return Ok(0),
        Outcome::Ready => {}
    }
    let api = api?;
    let me = person(&api, &here)?;
    let token = gh_token(&here)?;
    let reply = api.project_init(&me, &repo, hosts, &token).await?;
    println!("{}", text::project_enrolled(&reply));
    Ok(0)
}

/// `riff project remove REPO`: riff-server forgets the project `repo`,
/// with the token of `gh` of the person (01M76SHB27S4QDV9HMRR4B6APJ). It
/// gives the line for the person.
pub async fn remove(server: &str, repo: &str) -> Result<String> {
    let repo: ThreadName = repo.parse().with_context(|| text::project_bad_repo(repo))?;
    let here = identity::working_dir()?;
    let api = Api::new(server).signed_in(None)?;
    let me = person(&api, &here)?;
    let token = gh_token(&here)?;
    let gone = api.project_remove(&me, &repo, &token).await?;
    Ok(text::project_removed(&gone.repo))
}

/// Why a host runs no wave of a repository, or a repository gets no lead
/// (#1863): the repository is no project of the riff, or the project does
/// not allow the host. Its text names the fix: `riff project init` with
/// the hosts, by an admin. Its [`Refused::word`] is the word of the wave
/// in `riff daemon status`.
///
/// ```
/// use riff::project::Refused;
///
/// let none = Refused::NoProject { repo: "o/r".into() };
/// assert!(none.to_string().contains("runs riff project init in a clone of o/r"), "{none}");
/// assert!(none.word().contains("no project of the riff"), "{}", none.word());
/// let host = Refused::Host { repo: "o/r".into(), host: "thelio".into(), hosts: vec!["b".into()] };
/// assert!(host.to_string().contains("riff project init --host b --host thelio"), "{host}");
/// assert!(host.word().contains("does not allow the host thelio"), "{}", host.word());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The repository `repo` is no project of the riff.
    NoProject {
        /// The repository, as `OWNER/REPO`.
        repo: String,
    },
    /// The project `repo` allows only `hosts`, and not `host`.
    Host {
        /// The repository, as `OWNER/REPO`.
        repo: String,
        /// The host that the project does not allow.
        host: String,
        /// The hosts of the project.
        hosts: Vec<String>,
    },
}

impl Refused {
    /// The word of the wave in `riff daemon status`.
    pub fn word(&self) -> String {
        match self {
            Refused::NoProject { .. } => text::WAVE_NO_PROJECT.into(),
            Refused::Host { host, .. } => text::wave_host_not_allowed(host),
        }
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            Refused::NoProject { repo } => text::no_project(repo),
            Refused::Host { repo, host, hosts } => text::host_not_allowed(repo, host, hosts),
        })
    }
}

impl std::error::Error for Refused {}

/// Why the host `host` runs no wave of `repo` (`OWNER/REPO`), with the
/// projects `projects` of the riff (#1863): `None` when `repo` is a
/// project that allows `host`. A project with no hosts allows each host.
///
/// ```
/// use riff::project::{Refused, wave_refused};
/// use riff_core::wire::ProjectFacts;
///
/// let project = |hosts: &[&str]| ProjectFacts {
///     repo: "o/r".parse().unwrap(),
///     by: None,
///     at_ms: 5,
///     hosts: hosts.iter().map(|h| h.to_string()).collect(),
///     leads: vec![],
/// };
/// assert_eq!(wave_refused(&[project(&[])], "o/r", "thelio"), None);
/// assert_eq!(wave_refused(&[project(&["thelio"])], "o/r", "thelio"), None);
/// assert!(matches!(
///     wave_refused(&[project(&["pangolin"])], "o/r", "thelio"),
///     Some(Refused::Host { hosts, .. }) if hosts == ["pangolin"]
/// ));
/// assert!(matches!(wave_refused(&[], "o/r", "thelio"), Some(Refused::NoProject { .. })));
/// assert!(matches!(
///     wave_refused(&[project(&[])], "o/other", "thelio"),
///     Some(Refused::NoProject { .. })
/// ));
/// ```
pub fn wave_refused(projects: &[ProjectFacts], repo: &str, host: &str) -> Option<Refused> {
    let project = projects.iter().find(|p| p.repo.to_string() == repo);
    let Some(project) = project else {
        return lead_refused(projects, repo);
    };
    let allowed = project.hosts.is_empty() || project.hosts.iter().any(|h| h == host);
    (!allowed).then(|| Refused::Host {
        repo: repo.to_owned(),
        host: host.to_owned(),
        hosts: project.hosts.clone(),
    })
}

/// Why `repo` (`OWNER/REPO`) gets no lead, with the projects `projects`
/// of the riff (#1863): `None` when it is a project. The hosts of a
/// project are for its wave: a lead runs on each host.
///
/// ```
/// use riff::project::{Refused, lead_refused};
/// use riff_core::wire::ProjectFacts;
///
/// let project = ProjectFacts {
///     repo: "o/r".parse().unwrap(),
///     by: None,
///     at_ms: 5,
///     hosts: vec!["pangolin".into()],
///     leads: vec![],
/// };
/// assert_eq!(lead_refused(&[project.clone()], "o/r"), None);
/// assert_eq!(
///     lead_refused(&[project], "o/other"),
///     Some(Refused::NoProject { repo: "o/other".into() })
/// );
/// ```
pub fn lead_refused(projects: &[ProjectFacts], repo: &str) -> Option<Refused> {
    let known = projects.iter().any(|p| p.repo.to_string() == repo);
    (!known).then(|| Refused::NoProject {
        repo: repo.to_owned(),
    })
}

/// Each project of the riff on `server`, as the person of the host of
/// `here` (01M76SG8JVHTKY6N62FQG7XAE6).
pub async fn projects_of(here: &Place, server: &str) -> Result<Vec<ProjectFacts>> {
    let me = identity::person(&Place::host_only(here.host())?, server)?;
    let api = Api::new(server).signed_in(None)?;
    Ok(api.projects(&me).await?.projects)
}

/// Fails with the [`Refused`] of [`wave_refused`] when the host of `here`
/// may run no wave of the repository of `here` on `server` (#1863).
pub async fn wave_allowed(here: &Place, server: &str) -> Result<()> {
    let projects = projects_of(here, server).await?;
    match wave_refused(&projects, &here.repo_text(), here.host()) {
        Some(refused) => Err(refused.into()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The question of a run that must ask nothing.
    fn no_question(question: Question) -> Result<bool> {
        panic!("riff asked: {}", question.text())
    }

    fn failed(facts: &Facts) -> Vec<Check> {
        checks(facts).into_iter().filter(|c| !c.passed()).collect()
    }

    /// A repository on GitHub with each part.
    fn github() -> Github {
        Github {
            repo: "acme/app".into(),
            default_branch: "main".into(),
            auto_merge: true,
            squash_merge: true,
            required: vec!["Gate".into(), "riff/verify".into()],
            milestones: vec!["Backlog".into(), "Wave 1".into()],
            admin: true,
            files: vec!["riff.toml".into()],
            ci: vec!["Gate".into()],
            guard: None,
            bot: Ok(None),
            lead: Ok(None),
            releases: None,
            queue: None,
        }
    }

    fn facts_of(github: Result<Github, String>) -> Facts {
        Facts {
            github,
            no_done_when: None,
            folder: "/nonexistent/clone".into(),
            apps: None,
        }
    }

    /// 01M5QVF5535YRN1EF3DZAB4W8G: the checks of the repository, in order,
    /// and one failed check when `gh` cannot read it. No check of the
    /// machine.
    #[test]
    fn the_repository_on_github_has_seven_checks() {
        let names = |facts: &Facts| {
            checks(facts)
                .into_iter()
                .map(|c| c.name)
                .collect::<Vec<_>>()
        };
        let with = facts_of(Ok(github()));
        assert!(checks(&with).iter().all(Check::passed));
        assert_eq!(
            names(&with),
            [
                "riff.toml on main",
                "auto-merge and squash merge",
                "the ruleset of main",
                "the milestone Backlog",
                "the first wave",
                "the checks of a pull request",
                "the guard"
            ]
        );
        let none = facts_of(Ok(Github {
            auto_merge: false,
            required: vec![],
            milestones: vec![],
            ..github()
        }));
        assert_eq!(failed(&none).len(), 4);
        let unread = facts_of(Err("gh: not logged in".into()));
        let failed = failed(&unread);
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(failed[0].name, "the repository on GitHub");
        // The Apps come last.
        let apps = Facts {
            apps: Some(Apps {
                repo: "acme/app".into(),
                missing: vec![],
            }),
            ..with
        };
        assert_eq!(names(&apps).last().unwrap(), "the GitHub Apps of riff");
    }

    /// The fake forge of the tests of `riff project init`
    /// (`tests/fake/forge`), in a folder of its own.
    struct Forged {
        dir: tempfile::TempDir,
        /// The bot of the session App that riff-server gives.
        bot: Result<Option<String>, String>,
    }

    impl Forged {
        /// A bare repository: no file, a pull request that ran two checks,
        /// no auto-merge, no ruleset, no milestone.
        fn bare() -> Forged {
            let dir = tempfile::tempdir().unwrap();
            let files = [
                ("files", ""),
                ("milestones", ""),
                (
                    "prior_pulls",
                    r#"[{"number":3,"html_url":"https://github.com/acme/app/pull/3","head":{"ref":"feature","sha":"abc1234"},"base":{"ref":"main"}}]"#,
                ),
                (
                    "checks_abc1234",
                    r#"{"check_runs":[{"name":"checks / lint"},{"name":"checks / test"}]}"#,
                ),
            ];
            for (name, text) in files {
                std::fs::write(dir.path().join(name), text).unwrap();
            }
            isolated::write_program(dir.path().join("gh"), include_str!("../tests/fake/forge"));
            Forged { dir, bot: Ok(None) }
        }

        /// The same forge, in a riff whose session App has the bot `bot`.
        fn with_bot(self, bot: &str) -> Forged {
            Forged {
                bot: Ok(Some(bot.into())),
                ..self
            }
        }

        fn forge(&self) -> Forge {
            Forge {
                gh: crate::pr::Gh::at(self.dir.path().join("gh")),
                waits: repo_setup::Waits::NONE,
                image: None,
            }
        }

        /// The facts that riff reads from the forge now, for a clone in
        /// `folder`.
        fn facts_in(&self, folder: &Path) -> Facts {
            Facts {
                folder: folder.to_owned(),
                ..facts_of(repo_setup::read(
                    &self.forge().gh,
                    self.bot.clone(),
                    Ok(None),
                ))
            }
        }

        /// The facts that riff reads from the forge now.
        fn facts(&self) -> Facts {
            self.facts_in("/nonexistent/clone".as_ref())
        }

        /// The calls that change the forge.
        fn changes(&self) -> Vec<String> {
            std::fs::read_to_string(self.dir.path().join("calls"))
                .unwrap_or_default()
                .lines()
                .filter(|c| c.contains(" -X ") || c.starts_with("label "))
                .map(Into::into)
                .collect()
        }

        fn has(&self, name: &str) -> bool {
            self.dir.path().join(name).exists()
        }
    }

    /// `run` on `facts` in `forged`, with a question that answers `yes`:
    /// the outcome and the number of questions.
    fn run_counted(forged: &Forged, facts: &Facts, mode: Mode, yes: bool) -> (Outcome, usize) {
        let asked = std::cell::Cell::new(0);
        let question = |q: Question| {
            assert_eq!(q, Question::Setup, "riff asked: {}", q.text());
            asked.set(asked.get() + 1);
            Ok(yes)
        };
        let outcome = run(facts, mode, question, &forged.forge()).unwrap();
        (outcome, asked.get())
    }

    /// 01M5R1672NF56P15VD0JEX51K4: `riff project init --yes` sets up a
    /// bare repository with no question; the second run changes nothing
    /// and asks nothing.
    #[test]
    fn yes_sets_up_a_bare_repository_and_a_second_run_changes_nothing() {
        let forged = Forged::bare();
        let outcome = run(&forged.facts(), Mode::Yes, no_question, &forged.forge()).unwrap();
        assert_eq!(outcome, Outcome::Ready);
        let changes = forged.changes().len();
        assert!(changes >= 8, "{:?}", forged.changes());
        for name in ["auto_merge", "ruleset", "config.json", "merge.json"] {
            assert!(forged.has(name), "{name}");
        }
        let milestones = std::fs::read_to_string(forged.dir.path().join("milestones")).unwrap();
        assert_eq!(milestones, "Backlog\nWave 1\n");

        let outcome = run(&forged.facts(), Mode::Yes, no_question, &forged.forge()).unwrap();
        assert_eq!(outcome, Outcome::Ready);
        assert_eq!(
            forged.changes().len(),
            changes,
            "a second run changes the forge"
        );
    }

    /// 01M5R1672NF56P15VD0JEX51K4: in `Mode::Ask`, the steps wait for one
    /// question. A no does none, and a yes does each.
    #[test]
    fn the_steps_wait_for_one_question_and_a_no_changes_nothing() {
        let forged = Forged::bare();
        let facts = forged.facts();
        assert_eq!(
            run_counted(&forged, &facts, Mode::Ask, false),
            (Outcome::Failed, 1)
        );
        assert!(forged.changes().is_empty(), "{:?}", forged.changes());

        assert_eq!(
            run_counted(&forged, &facts, Mode::Ask, true),
            (Outcome::Ready, 1)
        );
        assert!(forged.has("ruleset") && forged.has("config.json"));

        // Set up: no question at all.
        let facts = forged.facts();
        assert!(failed(&facts).is_empty());
        assert_eq!(
            run_counted(&forged, &facts, Mode::Ask, true),
            (Outcome::Ready, 0)
        );
    }

    /// 01M5R1672NF56P15VD0JEX51K4, 01M76SHAX0KR0DM9GPXAPZJRDZ: `--check`
    /// makes no call that changes the forge, asks nothing, and the
    /// outcome enrolls nothing.
    #[test]
    fn check_names_the_steps_and_changes_nothing() {
        let forged = Forged::bare();
        let facts = forged.facts();
        assert_eq!(
            run_counted(&forged, &facts, Mode::Check, true),
            (Outcome::Failed, 0)
        );
        assert!(forged.changes().is_empty(), "{:?}", forged.changes());
        assert!(!forged.has("ruleset"));
        let set_up = facts_of(Ok(github()));
        assert_eq!(
            run(&set_up, Mode::Check, no_question, &forged.forge()).unwrap(),
            Outcome::Checked
        );
    }

    /// 01M6WPV86SW4AAVA2NCBQNYXK3: in a riff with a session App, `riff
    /// project init --check` names the missing guard with its step and
    /// changes nothing. `--yes` puts the guard of the bot in the pull
    /// request of `riff.toml`, and the check of the guard passes after it.
    #[test]
    fn the_check_names_a_missing_guard_and_yes_puts_it_with_riff_toml() {
        let forged = Forged::bare().with_bot("riff-acme[bot]");
        let facts = forged.facts();
        let guard = checks(&facts).pop().unwrap();
        assert_eq!(guard.name, "the guard");
        assert_eq!(guard.step, Some(repo_setup::Step::Config));
        assert!(
            guard
                .line()
                .contains("has no .github/workflows/riff-guard.yml"),
            "{}",
            guard.line()
        );
        assert_eq!(
            run_counted(&forged, &facts, Mode::Check, true),
            (Outcome::Failed, 0)
        );
        assert!(forged.changes().is_empty(), "{:?}", forged.changes());

        let (outcome, _) = run_counted(&forged, &facts, Mode::Yes, true);
        assert_eq!(outcome, Outcome::Ready);
        let written = std::fs::read_to_string(forged.dir.path().join("guard")).unwrap();
        assert_eq!(written, crate::guard::workflow("riff-acme[bot]"));
        let guard = checks(&forged.facts()).pop().unwrap();
        assert!(guard.passed(), "{}", guard.line());
    }

    /// 01M5R1672NF56P15VD0JEX51K4: a person who is not an admin gets the
    /// steps named. riff asks nothing and changes nothing.
    #[test]
    fn a_person_who_is_not_an_admin_gets_no_question_and_no_change() {
        let forged = Forged::bare();
        std::fs::write(forged.dir.path().join("not_admin"), "").unwrap();
        let facts = forged.facts();
        assert!(!facts.github.as_ref().unwrap().admin);
        for mode in [Mode::Ask, Mode::Yes] {
            assert_eq!(
                run_counted(&forged, &facts, mode, true),
                (Outcome::Failed, 0),
                "{mode:?}"
            );
        }
        assert!(forged.changes().is_empty(), "{:?}", forged.changes());
    }

    /// A step that fails fails the run, and the next steps run.
    #[test]
    fn a_step_that_fails_fails_the_run_and_the_next_steps_run() {
        let forged = Forged::bare();
        std::fs::write(forged.dir.path().join("fail"), "rulesets").unwrap();
        let (outcome, _) = run_counted(&forged, &forged.facts(), Mode::Yes, true);
        assert_eq!(outcome, Outcome::Failed);
        assert!(forged.has("auto_merge") && forged.has("config.json"));
        assert!(!forged.has("ruleset"));
        let milestones = std::fs::read_to_string(forged.dir.path().join("milestones")).unwrap();
        assert_eq!(milestones, "Backlog\nWave 1\n");
    }

    /// Runs `git ARGS` in `dir`, and gives its stdout.
    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=riff",
                "-c",
                "user.email=riff@example.com",
                "-C",
            ])
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// A clone of a repository on the branch `main` that lacks the last
    /// commit of its origin: the commit that adds `riff.toml`, as the
    /// merge of the step does. It gives the folders of the origin and of
    /// the clone.
    fn clone_behind(root: &Path) -> (PathBuf, PathBuf) {
        let (origin, clone) = (root.join("origin"), root.join("clone"));
        git(root, &["init", "-q", "-b", "main", "origin"]);
        std::fs::write(origin.join("CLAUDE.md"), "rules\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "-q", "-m", "first"]);
        git(root, &["clone", "-q", "origin", "clone"]);
        std::fs::write(
            origin.join("riff.toml"),
            session_image::config_of(&session_image::base()),
        )
        .unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "-q", "-m", "Set up riff"]);
        (origin, clone)
    }

    /// 01M5RGGD9AYNC12Z2CG5CDZWNA: the clone is on the default branch, has
    /// no change, and lacks only the commits of `origin`: after the step of
    /// `riff.toml`, riff runs `git pull --ff-only`, and the clone has the
    /// file.
    #[test]
    fn a_clone_that_lacks_only_commits_of_origin_gets_riff_toml_by_git_pull() {
        let forged = Forged::bare();
        let root = tempfile::tempdir().unwrap();
        let (origin, clone) = clone_behind(root.path());
        assert!(!clone.join("riff.toml").exists());

        let facts = forged.facts_in(&clone);
        run(&facts, Mode::Yes, no_question, &forged.forge()).unwrap();
        assert!(forged.has("config.json"), "the step ran");
        assert!(clone.join("riff.toml").exists());
        assert_eq!(
            git(&clone, &["rev-parse", "HEAD"]),
            git(&origin, &["rev-parse", "HEAD"])
        );
    }

    /// 01M5RGGD9AYNC12Z2CG5CDZWNA: a clone on another branch, with a change,
    /// or with a commit that `origin` lacks gets no `git pull`: riff changes
    /// nothing in it.
    #[test]
    fn a_clone_that_is_not_safe_to_pull_gets_no_git_pull() {
        for case in ["another branch", "a change", "a local commit"] {
            let forged = Forged::bare();
            let root = tempfile::tempdir().unwrap();
            let (_, clone) = clone_behind(root.path());
            match case {
                "another branch" => drop(git(&clone, &["checkout", "-q", "-b", "dev"])),
                "a change" => std::fs::write(clone.join("CLAUDE.md"), "mine\n").unwrap(),
                _ => drop(git(
                    &clone,
                    &["commit", "-q", "--allow-empty", "-m", "mine"],
                )),
            }
            let head = git(&clone, &["rev-parse", "HEAD"]);
            let facts = forged.facts_in(&clone);
            run(&facts, Mode::Yes, no_question, &forged.forge()).unwrap();
            assert!(forged.has("config.json"), "{case}: the step ran");
            assert!(!clone.join("riff.toml").exists(), "{case}");
            assert_eq!(git(&clone, &["rev-parse", "HEAD"]), head, "{case}");
        }
    }

    #[test]
    fn pull_says_why_it_ran_no_git_pull() {
        let root = tempfile::tempdir().unwrap();
        let (_, clone) = clone_behind(root.path());
        assert_eq!(
            pull(&clone, "trunk"),
            Err("the clone is on main, not on trunk".to_owned())
        );
        git(&clone, &["checkout", "-q", "--detach"]);
        assert_eq!(
            pull(&clone, "main"),
            Err("the clone is on no branch, not on main".to_owned())
        );
        git(&clone, &["checkout", "-q", "main"]);
        std::fs::write(clone.join("new"), "").unwrap();
        assert_eq!(
            pull(&clone, "main"),
            Err("the clone has a change".to_owned())
        );
        std::fs::remove_file(clone.join("new")).unwrap();
        git(&clone, &["commit", "-q", "--allow-empty", "-m", "mine"]);
        let why = pull(&clone, "main").unwrap_err();
        assert!(why.starts_with("git pull --ff-only origin main: "), "{why}");
        assert!(!why.contains('\n'), "{why}");
        // Not a repository: git says so.
        let none = tempfile::tempdir().unwrap();
        assert!(pull(none.path(), "main").is_err());
    }
}
