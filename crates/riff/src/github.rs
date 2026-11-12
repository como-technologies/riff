//! `riff github`: make, install and check the two GitHub Apps of the
//! riff (01M6WG52J7WTP079W1AYEQ4W08).
//!
//! # Design
//!
//! | Command | Does |
//! |---|---|
//! | `riff github create --owner OWNER` | Opens the start page of riff-server, which makes the session App `riff-OWNER` and the lead App `riff-OWNER-lead` on OWNER. Then the steps of `install` ([`create`]) |
//! | `riff github install OWNER [--repo OWNER/NAME]` | Opens the install page of each App for OWNER, the page of the rights of each App without the rights of riff, and the page Configure of each install without the repository or the rights. It waits for the two installs, each with the repository and the rights, and runs `check` ([`install`]) |
//! | `riff github check` | Asks riff-server for a token of each role for the repository of this folder, and prints the bot and the rights of each role ([`check`]) |
//! | `riff github events` | Asks riff-server for the last wakes of the GitHub App for the repository of this folder, and prints them with the counts of the deliveries ([`events`], #1655) |
//!
//! riff-server holds the keys of the Apps, and makes each token
//! (`riff_server::github`). This side reads the account with the `gh`
//! of the person (`gh api users/OWNER`), opens the browser, and waits.
//! GitHub has no API that installs an App, or that adds a repository to
//! an install: the owner of the account clicks "Install" one time for
//! each App, and adds a repository on the page Configure of each install
//! ([`configure_page`]).
//!
//! ```mermaid
//! flowchart LR
//!     C["riff github create"] --> B["browser: two clicks to make,<br/>two clicks to install"]
//!     I["riff github install"] --> N{"nothing to do?"}
//!     N -- yes --> K["riff github check"]
//!     N -- no --> B
//!     B --> W["wait for the two installs,<br/>at most 10 minutes"]
//!     W -- "an App without<br/>the rights of riff" --> R["browser: the page of its rights,<br/>set each right"]
//!     R --> W
//!     W -- "an install that did not<br/>accept the rights" --> A["browser: the page Configure,<br/>accept the new rights"]
//!     A --> W
//!     W -- "an install<br/>without the repository" --> P["browser: the page Configure,<br/>add the repository"]
//!     P --> W
//!     W --> K
//! ```
//!
//! ## The rights
//!
//! riff-server compares the rights of each App, and of its install on
//! OWNER, with the one list of riff: the rights of the manifest
//! (`riff_server::github::app_rights`, 01M74FVDCJT8P0670P46MWR1MJ).
//! GitHub has no API call that sets the rights of an App, or that
//! accepts them for an install. So for an App whose rights are not the
//! list, riff opens the page of its rights ([`rights_page`]) and prints
//! one line for each right to set. When the App has the list, riff opens
//! the page Configure of each install that has not accepted it. riff
//! opens each page one time, and waits (01M74FVDGM0342M8TZQCCK5H4H). A
//! run of `install` with nothing to do opens no page, says so in one
//! line, and runs `check`.
//!
//! ## The repository
//!
//! The repository is the repository of the folder, or the one of
//! `--repo` ([`named_repo`]), when it is a repository of OWNER
//! (01M6Z7DMYAW52X62TN1DEX9NCG). The wait asks riff-server whether each
//! install has it. For an App that is installed without it, riff prints
//! the page Configure of the install and opens it, one time for each
//! App. With no such repository, the wait ends at the two installs, and
//! riff names where to run `riff github check`.
//!
//! ## Who runs it
//!
//! Each command refuses in an agent session and in a container, before
//! each call ([`refusal`], 01M6WG537B67VWVENY3821RV33). riff-server
//! refuses `create` and `install` to a member who is not an admin of the
//! riff.

use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use riff_client::api::Api;
use riff_core::wire::{GitHubApp, GitHubAppFacts, GitHubAppsReply, GitHubCreate, GitHubOwner};
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

/// The page Configure of the install `installation` on `account`, on the
/// site `web`: there the owner of the account adds a repository under
/// "Repository access" (01M6Z7DMYAW52X62TN1DEX9NCG).
///
/// ```
/// use riff::github::{Account, configure_page};
///
/// let org = Account { login: "acme".into(), id: 7, kind: "Organization".into() };
/// assert_eq!(
///     configure_page("https://github.com", &org, 1010),
///     "https://github.com/organizations/acme/settings/installations/1010"
/// );
/// let user = Account { login: "ada".into(), id: 8, kind: "User".into() };
/// assert_eq!(
///     configure_page("https://github.com", &user, 1010),
///     "https://github.com/settings/installations/1010"
/// );
/// ```
pub fn configure_page(web: &str, account: &Account, installation: u64) -> String {
    if account.organization() {
        format!(
            "{web}/organizations/{}/settings/installations/{installation}",
            account.login
        )
    } else {
        format!("{web}/settings/installations/{installation}")
    }
}

/// The page of the rights of the App `slug` of the account `owner`, on
/// the site `web`: there a manager of the App sets each right, and
/// clicks "Save changes" (01M74FVDGM0342M8TZQCCK5H4H).
///
/// ```
/// use riff::github::rights_page;
/// use riff_core::wire::GitHubOwner;
///
/// let org = GitHubOwner { login: "acme".into(), organization: true };
/// assert_eq!(
///     rights_page("https://github.com", &org, "riff-acme-lead"),
///     "https://github.com/organizations/acme/settings/apps/riff-acme-lead/permissions"
/// );
/// let user = GitHubOwner { login: "ada".into(), organization: false };
/// assert_eq!(
///     rights_page("https://github.com", &user, "riff-ada"),
///     "https://github.com/settings/apps/riff-ada/permissions"
/// );
/// ```
pub fn rights_page(web: &str, owner: &GitHubOwner, slug: &str) -> String {
    if owner.organization {
        format!(
            "{web}/organizations/{}/settings/apps/{slug}/permissions",
            owner.login
        )
    } else {
        format!("{web}/settings/apps/{slug}/permissions")
    }
}

/// True when `repo`, `OWNER/NAME`, is a repository of `owner`.
fn of_owner(repo: &str, owner: &str) -> bool {
    repo.split_once('/')
        .is_some_and(|(of, name)| of.eq_ignore_ascii_case(owner) && !name.is_empty())
}

/// The repository of `riff github install OWNER --repo REPO`: `repo`
/// when it is a repository `OWNER/NAME` of `owner`, else the refusal
/// (01M6Z7DMYAW52X62TN1DEX9NCG).
///
/// ```
/// assert_eq!(riff::github::named_repo("acme", "acme/app")?, "acme/app");
/// let refused = riff::github::named_repo("acme", "other/app").unwrap_err().to_string();
/// assert!(refused.contains("other/app is no repository of acme"), "{refused}");
/// assert!(riff::github::named_repo("acme", "acme").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn named_repo<'a>(owner: &str, repo: &'a str) -> Result<&'a str> {
    if !of_owner(repo, owner) {
        anyhow::bail!(text::github_repo_elsewhere(owner, repo));
    }
    Ok(repo)
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
    web: &str,
    open: impl Fn(&str),
    wait: Wait,
    repo: Option<&str>,
    out: &mut impl Write,
) -> Result<bool> {
    // The Apps of the riff before the run: a run that makes an App again
    // waits for its new ID, not for the install of the old one.
    let before = api.github_apps(None, None).await?;
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
    let goal = Goal::new(account, web, repo, making);
    wait_for(api, &goal, wait, &open, out).await?;
    after_installs(api, &goal, out).await
}

/// `riff github install OWNER`: the install page of each App for OWNER
/// that is not installed there, then the wait for the two installs, each
/// with the repository `repo` of OWNER and the rights of riff, and
/// [`check`]. `repo` is the repository of `--repo`, else of this folder,
/// when it has one (01M6Z7DMYAW52X62TN1DEX9NCG). With nothing to do, it
/// opens no page and says so in one line (01M74FVDGM0342M8TZQCCK5H4H).
pub async fn install(
    api: &Api,
    account: &Account,
    web: &str,
    open: impl Fn(&str),
    wait: Wait,
    repo: Option<&str>,
    out: &mut impl Write,
) -> Result<bool> {
    let goal = Goal::new(account, web, repo, Making::new());
    let apps = api.github_apps(Some(&account.login), goal.repo).await?;
    if apps.apps.len() < GitHubApp::ALL.len() {
        anyhow::bail!(text::github_no_apps(&account.login));
    }
    if goal.all_reached(&apps) {
        writeln!(
            out,
            "{}",
            text::github_install_nothing(&account.login, goal.repo)
        )?;
        return after_installs(api, &goal, out).await;
    }
    for app in apps.apps.iter().filter(|app| app.installed != Some(true)) {
        let url = format!(
            "{web}/apps/{}/installations/new/permissions?target_id={}",
            app.slug, account.id
        );
        writeln!(out, "{}", text::github_install_opens(&app.slug, &url))?;
        open(&url);
    }
    wait_for(api, &goal, wait, &open, out).await?;
    after_installs(api, &goal, out).await
}

/// The Apps that a run of `riff github create` makes, each with the ID
/// that the riff had for it before the run, if any. A made App has
/// another ID.
type Making = Vec<(GitHubApp, Option<u64>)>;

/// What the wait of `create` and `install` waits for.
struct Goal<'a> {
    /// The account of the two installs.
    account: &'a Account,
    /// The site of GitHub, for the page Configure of an install.
    web: &'a str,
    /// The repository that each install must have: a repository of the
    /// account, or `None`.
    repo: Option<&'a str>,
    /// The Apps that the run makes.
    making: Making,
}

impl<'a> Goal<'a> {
    /// The goal for `repo` when it is a repository of the account.
    fn new(account: &'a Account, web: &'a str, repo: Option<&'a str>, making: Making) -> Self {
        let repo = repo.filter(|repo| of_owner(repo, &account.login));
        Goal {
            account,
            web,
            repo,
            making,
        }
    }

    /// True when `facts` is of an App that the run does not make, or of
    /// a made App: one with a new ID.
    fn made(&self, facts: &GitHubAppFacts) -> bool {
        self.making
            .iter()
            .find(|(app, _)| *app == facts.app)
            .is_none_or(|(_, old)| *old != Some(facts.id))
    }

    /// True when `facts` is of a made App with the rights of riff,
    /// installed on the account and on the repository of the goal, and
    /// its install accepted the rights.
    fn reached(&self, facts: &GitHubAppFacts) -> bool {
        self.made(facts)
            && facts.installed == Some(true)
            && facts.has_repo != Some(false)
            && facts.to_set.is_empty()
            && facts.to_accept.is_empty()
    }

    /// True when `apps` has both Apps, each [`Goal::reached`].
    fn all_reached(&self, apps: &GitHubAppsReply) -> bool {
        GitHubApp::ALL.into_iter().all(|app| {
            apps.apps
                .iter()
                .any(|facts| facts.app == app && self.reached(facts))
        })
    }

    /// The account of the App of `facts`, for the page of its rights:
    /// the one that riff-server read, else the account of the goal.
    fn owner_of(&self, facts: &GitHubAppFacts) -> GitHubOwner {
        facts.owner.clone().unwrap_or_else(|| GitHubOwner {
            login: self.account.login.clone(),
            organization: self.account.organization(),
        })
    }
}

/// A fact of one App that the wait prints one time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fact {
    Made,
    /// The App does not have the rights of riff.
    ToSet,
    /// The App has the rights of riff, after [`Fact::ToSet`].
    Set,
    Installed,
    /// The install has not accepted the rights of riff.
    ToAccept,
    /// The install accepted the rights of riff, after [`Fact::ToAccept`].
    Accepted,
    /// The install does not have the repository of the goal.
    Without,
    /// The install has the repository of the goal.
    With,
}

/// Waits until riff-server has both Apps of the [`Goal`]
/// ([`Goal::reached`]). It prints one line for each new fact. It opens
/// the page of the rights of each App without the rights of riff, and
/// the page Configure of each install without the rights or the
/// repository, one time each.
async fn wait_for(
    api: &Api,
    goal: &Goal<'_>,
    wait: Wait,
    open: &impl Fn(&str),
    out: &mut impl Write,
) -> Result<()> {
    let start = Instant::now();
    let owner = &goal.account.login;
    let mut told: Vec<(GitHubApp, Fact)> = Vec::new();
    loop {
        let apps = api.github_apps(Some(owner), goal.repo).await?;
        for facts in apps.apps.iter().filter(|facts| goal.made(facts)) {
            for (line, page) in new_lines(facts, goal, &mut told) {
                writeln!(out, "{line}")?;
                if let Some(page) = page {
                    open(&page);
                }
            }
        }
        if goal.all_reached(&apps) {
            return Ok(());
        }
        if start.elapsed() >= wait.most {
            anyhow::bail!(text::github_wait_ended(owner, goal.repo));
        }
        tokio::time::sleep(wait.every).await;
    }
}

/// The lines of the facts of one App that riff did not print yet: made,
/// without or with the rights of riff, installed, the rights that the
/// install did not accept or accepted, then without or with the
/// repository of the goal. The line of an App without the rights comes
/// with the page of its rights, to open. The line of an install without
/// the rights or the repository comes with its page Configure, to open.
fn new_lines(
    facts: &GitHubAppFacts,
    goal: &Goal<'_>,
    told: &mut Vec<(GitHubApp, Fact)>,
) -> Vec<(String, Option<String>)> {
    let had = |told: &[(GitHubApp, Fact)], fact: Fact| told.contains(&(facts.app, fact));
    let (had_to_set, had_to_accept) = (had(told, Fact::ToSet), had(told, Fact::ToAccept));
    let mut first = |fact: Fact| {
        let new = !told.contains(&(facts.app, fact));
        if new {
            told.push((facts.app, fact));
        }
        new
    };
    let mut lines = Vec::new();
    let making = goal.making.iter().any(|(app, _)| *app == facts.app);
    if making && first(Fact::Made) {
        lines.push((text::github_app_made(&facts.slug, &facts.bot), None));
    }
    let has_rights = facts.to_set.is_empty();
    if !has_rights && first(Fact::ToSet) {
        let page = rights_page(goal.web, &goal.owner_of(facts), &facts.slug);
        let line = text::github_rights_to_set(&facts.slug, &page, &facts.to_set);
        lines.push((line, Some(page)));
    }
    if has_rights && had_to_set && first(Fact::Set) {
        lines.push((text::github_rights_set(&facts.slug), None));
    }
    if facts.installed != Some(true) {
        return lines;
    }
    let owner = &goal.account.login;
    if first(Fact::Installed) {
        lines.push((text::github_app_installed(&facts.slug, owner), None));
    }
    let accepted = facts.to_accept.is_empty();
    if let (true, false, Some(installation)) = (has_rights, accepted, facts.installation)
        && first(Fact::ToAccept)
    {
        let page = configure_page(goal.web, goal.account, installation);
        let line = text::github_rights_to_accept(&facts.slug, owner, &page, &facts.to_accept);
        lines.push((line, Some(page)));
    }
    if accepted && had_to_accept && first(Fact::Accepted) {
        lines.push((text::github_rights_accepted(&facts.slug, owner), None));
    }
    let Some(repo) = goal.repo else {
        return lines;
    };
    match (facts.has_repo, facts.installation) {
        (Some(false), Some(installation)) if first(Fact::Without) => {
            let page = configure_page(goal.web, goal.account, installation);
            let line = text::github_repo_missing(&facts.slug, repo, &page);
            lines.push((line, Some(page)));
        }
        (Some(true), _) if first(Fact::With) => {
            lines.push((text::github_repo_added(&facts.slug, repo), None));
        }
        _ => {}
    }
    lines
}

/// After the installs: [`check`] for the repository of the goal, else
/// the line that says where to run it.
async fn after_installs(api: &Api, goal: &Goal<'_>, out: &mut impl Write) -> Result<bool> {
    match goal.repo {
        Some(repo) => check(api, repo, out).await,
        None => {
            writeln!(out, "{}", text::github_check_elsewhere(&goal.account.login))?;
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

/// `riff github events` for the repository `repo`: the last wakes of the
/// GitHub App, and the counts of the deliveries (#1655).
pub async fn events(api: &Api, repo: &str, out: &mut impl Write) -> Result<()> {
    let reply = api.github_events(repo).await?;
    write!(out, "{}", text::github_events(repo, &reply))?;
    Ok(())
}
