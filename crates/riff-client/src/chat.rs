//! The wakes of the chat: the sessions that a line of the thread `chat`
//! wakes (01M3NB5N0D99JB5CE6RB4VEYPF, 01M5QVH7KDK3R2S4NDFB2V1XY9,
//! 01M6VG1PJDWE523TR9X9WJ5VXG).
//! `riff chat` and the Chat tab of the TUI share them, so each UI has
//! one rule for `@lead:REPO`.
//!
//! # Design
//!
//! A line wakes each lead of the sender, unless it names a lead:
//! `@lead` wakes each lead of the sender, and `@USER` wakes each lead
//! of USER. `@lead:REPO` and `@USER:REPO` wake one lead: the lead in the
//! repository REPO. REPO is `OWNER/REPO`, or the name after the slash.
//!
//! [`wakes`] reads the line, and [`resolve`] picks the lead from the
//! leads that riff-server lists. When no lead or more than one lead
//! fits, the answer is the text for the person, and the UI sends
//! nothing. [`receivers`] does both, with a call `who` when the line
//! names a repository.
//!
//! ```mermaid
//! flowchart LR
//!     L["the line"] --> W["wakes:<br/>a selector for each name"]
//!     W -- "a name with a repository" --> R["resolve:<br/>the leads of who"]
//!     W -- "no repository" --> P["the post"]
//!     R -- "one lead fits" --> P
//!     R -- "no lead, or two leads" --> T["the text for the person:<br/>nothing is sent"]
//! ```
//!
//! The texts of [`resolve`] hold the names as the line and riff-server
//! gave them: the UI makes them safe.

use std::collections::BTreeSet;

use anyhow::Result;
use riff_core::name::SessionUri;
use riff_core::selector::Selector;

use crate::api::Api;

/// The sessions that a chat line of `sender` wakes: the leads of each
/// user that the line names with `@USER`, and the leads of the sender for
/// `@lead`. `@USER:REPO` and `@lead:REPO` name the one lead in the
/// repository REPO: the selector keeps REPO as typed (`OWNER/REPO` or
/// the name after the slash), and [`resolve`] makes it `OWNER/REPO`.
/// Punctuation after the name does not count. A line that names no one
/// wakes the leads of the sender, so a question never waits unseen.
///
/// ```
/// use riff_client::chat::wakes;
///
/// let lead = |user: &str| format!("user={user},lead=true").parse().unwrap();
/// let in_repo = |user: &str, repo: &str| {
///     format!("user={user},repo={repo},lead=true").parse().unwrap()
/// };
/// assert_eq!(wakes("@lead: is #12 done?", "mike"), [lead("mike")]);
/// assert_eq!(wakes("ask @brett, and @lead", "mike"), [lead("brett"), lead("mike")]);
/// assert_eq!(wakes("@mike @lead", "mike"), [lead("mike")]);
/// assert_eq!(wakes("mail mike@thelio about it", "brett"), [lead("brett")]);
/// assert_eq!(wakes("no names here", "brett"), [lead("brett")]);
///
/// // A name with a repository wakes one lead.
/// assert_eq!(wakes("@lead:strata is #12 done?", "mike"), [in_repo("mike", "strata")]);
/// assert_eq!(
///     wakes("@brett:como-technologies/strata, can I take #14?", "mike"),
///     [in_repo("brett", "como-technologies/strata")]
/// );
/// assert_eq!(
///     wakes("@lead:strata @lead:riff @lead", "mike"),
///     [in_repo("mike", "strata"), in_repo("mike", "riff"), lead("mike")]
/// );
/// // Text after a colon that is no repository is no repository.
/// assert_eq!(wakes("@brett: can I take #14?", "mike"), [lead("brett")]);
/// ```
pub fn wakes(line: &str, sender: &str) -> Vec<Selector> {
    let mut named: Vec<(&str, Option<&str>)> = Vec::new();
    for (user, repo) in names(line) {
        let user = if user == "lead" { sender } else { user };
        if !user.is_empty() && !named.contains(&(user, repo)) {
            named.push((user, repo));
        }
    }
    if named.is_empty() {
        named.push((sender, None));
    }
    named
        .into_iter()
        .map(|(user, repo)| match repo {
            Some(repo) => Selector::lead(user, repo),
            None => Selector {
                user: Some(user.to_owned()),
                lead: Some(true),
                ..Selector::default()
            },
        })
        .collect()
}

/// Each name of a chat line, in its order: the word after each `@`, and
/// the repository after a colon (`@USER:REPO`). `lead` stays `lead`.
/// Punctuation after the name does not count. [`wakes`] reads the names
/// so, and the highlight of a line in `riff chat` too.
///
/// ```
/// use riff_client::chat::names;
///
/// let named: Vec<_> = names("@brett: can @lead:strata, or (@ann) ask mike@thelio?").collect();
/// assert_eq!(named, [("brett", None), ("lead", Some("strata"))]);
/// ```
pub fn names(line: &str) -> impl Iterator<Item = (&str, Option<&str>)> {
    line.split_whitespace().filter_map(|word| {
        let name = word
            .strip_prefix('@')?
            .trim_end_matches(|c: char| !c.is_alphanumeric());
        Some(match name.split_once(':') {
            Some((user, repo)) if is_repo(repo) => (user, Some(repo)),
            _ => (name, None),
        })
    })
}

/// True when `text` is `REPO` or `OWNER/REPO`, with letters, digits, `-`,
/// `_` and `.` in each part.
fn is_repo(text: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    text.split_once('/')
        .map_or_else(|| part(text), |(owner, name)| part(owner) && part(name))
}

/// Makes each selector of [`wakes`] that names a repository exact: the
/// lead of its user in `OWNER/REPO`, from the sessions that riff-server
/// lists (`leads` may hold other sessions: only a lead counts). A
/// repository that is `OWNER/REPO` must be the repository of a lead of
/// the user. A repository that is a name must be the name after the slash
/// of the repository of exactly one lead of the user. A selector with no
/// repository stays as it is.
///
/// When no lead fits, or more than one lead fits, the answer is the text
/// for the person. It names the leads that the chat found. The chat sends
/// nothing then.
///
/// ```
/// use riff_client::chat::{resolve, wakes};
/// use riff_core::name::SessionUri;
///
/// let leads: Vec<SessionUri> = [
///     "riff://brett@heron/como-technologies/strata?session=b1&lead=true",
///     "riff://brett@heron/como-technologies/riff?session=b2&lead=true",
///     "riff://brett@heron/acme/riff?session=b3&lead=true",
///     "riff://brett@heron/como-technologies/riff?session=b4",
///     "riff://mike@thelio/como-technologies/riff?session=m1&lead=true",
/// ].into_iter().map(|u| u.parse().unwrap()).collect();
/// let one = |line: &str| resolve(wakes(line, "mike"), &leads);
/// let selector = |s: &str| vec![s.parse().unwrap()];
///
/// // The name after the slash fits one lead of the user.
/// assert_eq!(
///     one("@brett:strata hi"),
///     Ok(selector("user=brett,repo=como-technologies/strata,lead=true"))
/// );
/// assert_eq!(
///     one("@lead:riff hi"),
///     Ok(selector("user=mike,repo=como-technologies/riff,lead=true"))
/// );
/// // OWNER/REPO fits one lead, also when the name fits two.
/// assert_eq!(
///     one("@brett:acme/riff hi"),
///     Ok(selector("user=brett,repo=acme/riff,lead=true"))
/// );
/// // A line with no repository stays as it is.
/// assert_eq!(one("@brett hi"), Ok(selector("user=brett,lead=true")));
///
/// // Two leads fit, or no lead fits: no line, and the leads that it found.
/// assert_eq!(
///     one("@brett:riff hi"),
///     Err("brett has 2 leads in a repository with the name riff: \
///          acme/riff, como-technologies/riff. Use OWNER/REPO.".to_owned())
/// );
/// assert_eq!(
///     one("@brett:other hi"),
///     Err("brett has no lead in other. The leads of brett: acme/riff, \
///          como-technologies/riff, como-technologies/strata.".to_owned())
/// );
/// assert_eq!(
///     one("@lead:other hi"),
///     Err("mike has no lead in other. The leads of mike: \
///          como-technologies/riff.".to_owned())
/// );
/// assert_eq!(
///     resolve(wakes("@ann:riff", "mike"), &leads),
///     Err("ann has no lead.".to_owned())
/// );
/// ```
pub fn resolve(to: Vec<Selector>, leads: &[SessionUri]) -> Result<Vec<Selector>, String> {
    to.into_iter()
        .map(|selector| match (&selector.user, &selector.repo) {
            (Some(user), Some(asked)) => {
                pick(user, asked, leads).map(|repo| Selector::lead(user, &repo))
            }
            _ => Ok(selector),
        })
        .collect()
}

/// The `OWNER/REPO` of the one lead of `user` that `asked` names.
fn pick(user: &str, asked: &str, leads: &[SessionUri]) -> Result<String, String> {
    let repos: BTreeSet<String> = leads
        .iter()
        .filter(|l| l.lead() && l.who().user() == user)
        .map(|l| l.place().repo_text())
        .collect();
    let fits = |repo: &&String| {
        repo.as_str() == asked || (!asked.contains('/') && repo.rsplit('/').next() == Some(asked))
    };
    let fit: Vec<&String> = repos.iter().filter(fits).collect();
    let list = |repos: &mut dyn Iterator<Item = &String>| {
        repos.map(String::as_str).collect::<Vec<_>>().join(", ")
    };
    match fit[..] {
        [one] => Ok(one.clone()),
        [] if repos.is_empty() => Err(format!("{user} has no lead.")),
        [] => Err(format!(
            "{user} has no lead in {asked}. The leads of {user}: {}.",
            list(&mut repos.iter())
        )),
        _ => Err(format!(
            "{user} has {} leads in a repository with the name {asked}: {}. Use OWNER/REPO.",
            fit.len(),
            list(&mut fit.into_iter())
        )),
    }
}

/// The selectors of the chat line `body` of `me`: [`wakes`], and
/// [`resolve`] with the leads that riff-server lists when the line names
/// a repository. The error names the leads that it found, or is the
/// error of the call to riff-server.
pub async fn receivers(api: &Api, me: &SessionUri, body: &str) -> Result<Vec<Selector>> {
    let to = wakes(body, me.who().user());
    if to.iter().all(|s| s.repo.is_none()) {
        return Ok(to);
    }
    let leads: Vec<SessionUri> = api
        .who(me, false)
        .await?
        .into_iter()
        .map(|s| s.uri)
        .collect();
    resolve(to, &leads).map_err(anyhow::Error::msg)
}
