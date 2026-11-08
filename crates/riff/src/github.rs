//! `riff github`: make, install and check the two GitHub Apps of the
//! riff (01M6WG52J7WTP079W1AYEQ4W08).
//!
//! # Design
//!
//! | Command | Does |
//! |---|---|
//! | `riff github create --owner OWNER` | Opens the start page of riff-server, which makes the session App `riff-OWNER` and the lead App `riff-OWNER-lead` on OWNER. Then the steps of `install` ([`create`]) |
//! | `riff github install OWNER` | Opens the install page of each App for OWNER, waits for the two installs, and runs `check` ([`install`]) |
//! | `riff github check` | Asks riff-server for a token of each role for the repository of this folder, and prints the bot and the rights of each role ([`check`]) |
//!
//! riff-server holds the keys of the Apps, and makes each token
//! (`riff_server::github`). This side reads the account with the `gh`
//! of the person (`gh api users/OWNER`), opens the browser, and waits.
//! GitHub has no API that installs an App: the owner of the account
//! clicks "Install" one time for each App.
//!
//! ```mermaid
//! flowchart LR
//!     C["riff github create"] --> B["browser: two clicks to make,<br/>two clicks to install"]
//!     I["riff github install"] --> B
//!     B --> W["wait for the two installs,<br/>at most 10 minutes"]
//!     W --> K["riff github check"]
//! ```
//!
//! ## Who runs it
//!
//! Each command refuses in an agent session and in a container, before
//! each call ([`refusal`], 01M6WG537B67VWVENY3821RV33). riff-server
//! refuses `create` and `install` to a member who is not the owner or an
//! admin of the riff.

use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use riff_client::api::Api;
use riff_core::wire::{GitHubApp, GitHubAppFacts, GitHubCreate};
use serde::Deserialize;

use crate::text;

/// The site of GitHub: the install pages of the Apps.
pub const WEB: &str = "https://github.com";

/// How often riff asks riff-server for the installs, and for how long
/// at most (01M6WG5322WRWQA79RCSCY4MSS).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wait {
    pub every: Duration,
    pub most: Duration,
}

impl Default for Wait {
    fn default() -> Self {
        Wait {
            every: Duration::from_secs(3),
            most: Duration::from_secs(10 * 60),
        }
    }
}

/// An account on GitHub, as `gh api users/OWNER` gives it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Account {
    pub login: String,
    pub id: u64,
    /// `Organization` or `User`.
    #[serde(rename = "type")]
    pub kind: String,
}

impl Account {
    /// True for an organization.
    pub fn organization(&self) -> bool {
        self.kind == "Organization"
    }
}

/// The account `owner`, with the `gh` of the person.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let script = "#!/bin/sh\necho '{\"login\":\"acme\",\"id\":7,\"type\":\"Organization\"}'\n";
/// let gh = riff::pr::Gh::at(isolated::write_program(dir.path().join("gh"), script));
/// let account = riff::github::account(&gh, "acme")?;
/// assert_eq!((account.id, account.organization()), (7, true));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn account(gh: &crate::pr::Gh, owner: &str) -> Result<Account> {
    gh.json(&["api", &format!("users/{owner}")])
        .with_context(|| format!("riff cannot read the account {owner} on GitHub"))
}

/// Why this process may not run `riff github COMMAND`, or `None`
/// (01M6WG537B67VWVENY3821RV33): an agent session, or a container.
///
/// ```
/// let refused = riff::github::refusal_for("check", true, false).unwrap();
/// assert!(refused.contains("a session never runs riff github check"), "{refused}");
/// assert!(riff::github::refusal_for("check", false, false).is_none());
/// ```
pub fn refusal_for(command: &str, agent: bool, container: bool) -> Option<String> {
    (agent || container).then(|| text::github_refused(command, container))
}

/// [`refusal_for`] this process.
pub fn refusal(command: &str) -> Option<String> {
    refusal_for(
        command,
        crate::cycle::in_agent(),
        crate::cycle::in_container(),
    )
}

/// `riff github create --owner OWNER`: the start page of riff-server in
/// the browser, then the steps of [`install`]. `repo` is the repository
/// of this folder, when it has one. It gives false when a check failed.
pub async fn create(
    api: &Api,
    account: &Account,
    open: impl Fn(&str),
    wait: Wait,
    repo: Option<&str>,
    out: &mut impl Write,
) -> Result<bool> {
    // The Apps of the riff before the run: a run that makes an App again
    // waits for its new ID, not for the install of the old one.
    let before = api.github_apps(None).await?;
    let started = api
        .github_create(&GitHubCreate {
            owner: account.login.clone(),
            organization: account.organization(),
            owner_id: Some(account.id),
        })
        .await?;
    let making: Making = started
        .makes
        .iter()
        .map(|app| {
            let old = before.apps.iter().find(|facts| facts.app == *app);
            (*app, old.map(|facts| facts.id))
        })
        .collect();
    writeln!(
        out,
        "{}",
        text::github_create_opens(&account.login, &started.url)
    )?;
    open(&started.url);
    wait_for(api, &account.login, wait, &making, out).await?;
    after_installs(api, &account.login, repo, out).await
}

/// `riff github install OWNER`: the install page of each App for OWNER,
/// then the wait for the two installs and [`check`].
pub async fn install(
    api: &Api,
    account: &Account,
    web: &str,
    open: impl Fn(&str),
    wait: Wait,
    repo: Option<&str>,
    out: &mut impl Write,
) -> Result<bool> {
    let apps = api.github_apps(Some(&account.login)).await?;
    if apps.apps.len() < GitHubApp::ALL.len() {
        anyhow::bail!(text::github_no_apps(&account.login));
    }
    for app in apps.apps.iter().filter(|app| app.installed != Some(true)) {
        let url = format!(
            "{web}/apps/{}/installations/new/permissions?target_id={}",
            app.slug, account.id
        );
        writeln!(out, "{}", text::github_install_opens(&app.slug, &url))?;
        open(&url);
    }
    wait_for(api, &account.login, wait, &Making::new(), out).await?;
    after_installs(api, &account.login, repo, out).await
}

/// The Apps that a run of `riff github create` makes, each with the ID
/// that the riff had for it before the run, if any. A made App has
/// another ID.
type Making = Vec<(GitHubApp, Option<u64>)>;

/// Waits until riff-server has both Apps, each installed on `owner`, and
/// each App of `making` with a new ID. It prints one line for each new
/// fact.
async fn wait_for(
    api: &Api,
    owner: &str,
    wait: Wait,
    making: &Making,
    out: &mut impl Write,
) -> Result<()> {
    let start = Instant::now();
    let mut told: Vec<(GitHubApp, bool)> = Vec::new();
    loop {
        let apps = api.github_apps(Some(owner)).await?;
        let made = |facts: &GitHubAppFacts| {
            making
                .iter()
                .find(|(app, _)| *app == facts.app)
                .is_none_or(|(_, old)| *old != Some(facts.id))
        };
        for facts in apps.apps.iter().filter(|facts| made(facts)) {
            let making = making.iter().any(|(app, _)| *app == facts.app);
            for line in new_lines(facts, owner, making, &mut told) {
                writeln!(out, "{line}")?;
            }
        }
        let ready = |app: GitHubApp| {
            apps.apps
                .iter()
                .any(|facts| facts.app == app && made(facts) && facts.installed == Some(true))
        };
        if GitHubApp::ALL.into_iter().all(ready) {
            return Ok(());
        }
        if start.elapsed() >= wait.most {
            anyhow::bail!(text::github_wait_ended(owner));
        }
        tokio::time::sleep(wait.every).await;
    }
}

/// The lines of the facts of one App that riff did not print yet: made,
/// then installed.
fn new_lines(
    facts: &GitHubAppFacts,
    owner: &str,
    making: bool,
    told: &mut Vec<(GitHubApp, bool)>,
) -> Vec<String> {
    let mut lines = Vec::new();
    if making && !told.contains(&(facts.app, false)) {
        told.push((facts.app, false));
        lines.push(text::github_app_made(&facts.slug, &facts.bot));
    }
    if facts.installed == Some(true) && !told.contains(&(facts.app, true)) {
        told.push((facts.app, true));
        lines.push(text::github_app_installed(&facts.slug, owner));
    }
    lines
}

/// After the installs: [`check`] for the repository of this folder when
/// it is a repository of `owner`, else the line that says where to run
/// it.
async fn after_installs(
    api: &Api,
    owner: &str,
    repo: Option<&str>,
    out: &mut impl Write,
) -> Result<bool> {
    match repo.filter(|repo| {
        repo.split_once('/')
            .is_some_and(|(of, _)| of.eq_ignore_ascii_case(owner))
    }) {
        Some(repo) => check(api, repo, out).await,
        None => {
            writeln!(out, "{}", text::github_check_elsewhere(owner))?;
            Ok(true)
        }
    }
}

/// `riff github check` for the repository `repo`: one line for each
/// role (01M6WG534PPE2JW8JQFRJDEK86). It gives false when a role failed.
pub async fn check(api: &Api, repo: &str, out: &mut impl Write) -> Result<bool> {
    let checked = api.github_check(repo).await?;
    write!(out, "{}", text::github_checked(repo, &checked))?;
    Ok(checked.roles.iter().all(|role| role.passed()))
}
