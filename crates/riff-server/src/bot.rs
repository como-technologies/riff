//! The lines of riff-bot that riff writes for a person: the line of
//! `riff release cut`, the ask while a deploy waits, and the lines of the
//! daemon about a lead
//! (01M74BPHQZHXFYYNXAJ1WFASQQ). A question for a person gets no line
//! (01M75QTMB42NME9Z2Y0XC0AN97): the Waves tab and `riff questions` show it.
//!
//! # Design
//!
//! The riff of the person sends only the fact, in the call `bot`
//! ([`BotPost`](riff_core::wire::BotPost)). riff-server writes the text
//! of the line itself ([`line()`]), from the fact and from what it holds
//! ([`Facts`]): the owner of the riff, the place of the caller, and the
//! holds of the version in the thread `release` ([`riff_core::hold`]).
//! It posts the line in the chat with the kind `riff`, so the chat shows
//! it as `<riff-bot>`. No client posts that kind, and no field of the
//! call is a text of the line: a client puts no words of its own in a
//! line of riff-bot. The tag of a release must be a version `vX.Y.Z` and
//! nothing more, and the line holds the version that riff-server parsed.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as riff daemon of mike
//!     participant S as riff-server
//!     participant C as riff chat of mike
//!     D->>S: bot: release_waits v3.3.0, run 9
//!     S->>S: the owner, the repository, the holds of v3.3.0
//!     S-->>C: <riff-bot> @mike riff release v3.3.0 waits for your approval: ...
//! ```
//!
//! | Fact | Who sends it | The line names |
//! |---|---|---|
//! | `release_published` | `riff release cut` | the owner |
//! | `release_waits` | the watch of the deploy | the owner |
//! | `lead_started`, `lead_waits`, `lead_no_main`, `lead_failed`, `lead_stale`, `lead_no_network` | the daemon | no one |
//!
//! A line that names a person with `@USER` wakes the terminals of that
//! person by the name (01M6Y1A5PTCVZ6GRJ0FXMART2T): an ask of riff. The
//! line of the daemon about a lead names the person with no `@`, and
//! wakes no session (01M5NJ87YEHHGJBANM7CRYV56Z).
//!
//! Only a person, and the lead of the person in its repository, sends a
//! fact: `riff release cut` also runs in the container of the lead, and
//! the daemon sends each other fact as the person.

use riff_core::hold::Hold;
use riff_core::name::{SessionUri, lead_container};
use riff_core::selector::Selector;
use riff_core::wire::BotFact;

/// The refusal of a fact from a session that is not the lead.
pub const ONLY_PERSON_OR_LEAD: &str =
    "only a person, and the lead of the person, tell riff-server a fact for a line of riff-bot";

/// A line of riff-bot: its text, and the names that it wakes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The text.
    pub body: String,
    /// One selector `name=USER` for the person that the line names.
    pub to: Vec<Selector>,
}

/// What riff-server holds for a line of riff-bot.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// The caller: its user, and the repository and the host of the fact.
    pub me: &'a SessionUri,
    /// The USER of the owner of the riff, or `None` when it has none.
    pub owner: Option<&'a str>,
    /// The holds that stand on the version of the fact.
    pub holds: &'a [Hold],
}

/// The line of riff-bot for `fact`. The error is a fact that riff-server
/// cannot write a line for: a tag that is no version `vX.Y.Z`, also with
/// a text after its version, or a caller in no repository.
///
/// ```
/// use riff_core::hold::Hold;
/// use riff_core::wire::BotFact;
/// use riff_server::bot::{Facts, line};
///
/// let me = "riff://mike@thelio/como-technologies/riff".parse()?;
/// let held = [Hold { user: "nate".into(), repo: "o/s".into(), reason: "A demo runs.".into() }];
/// let facts = Facts { me: &me, owner: Some("mike"), holds: &held };
/// let waits = BotFact::ReleaseWaits { tag: "v3.3.0".into(), run: 9, reminder: false };
/// let asked = line(&waits, &facts).unwrap();
/// assert_eq!(
///     asked.body,
///     "@mike riff release v3.3.0 waits for your approval: \
///      https://github.com/como-technologies/riff/actions/runs/9. In a terminal, run: riff \
///      release approve v3.3.0 A hold stands: the lead of o/s asks to wait: A demo runs."
/// );
/// assert_eq!(asked.to[0].to_string(), "name=mike");
///
/// let started = line(&BotFact::LeadStarted, &facts).unwrap();
/// assert!(started.body.starts_with(
///     "riff started the lead of mike in como-technologies/riff on thelio again"
/// ));
/// assert!(started.to.is_empty());
///
/// for tag in ["@everyone hi", "v3.3.0-approve now", "v3.3.0+@everyone"] {
///     let bad = BotFact::ReleasePublished { tag: tag.into(), run: None };
///     assert!(line(&bad, &facts).is_err());
/// }
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn line(fact: &BotFact, facts: &Facts<'_>) -> Result<Line, String> {
    let me = facts.me;
    if me.default_thread().is_none() {
        return Err("the caller is in no repository: a line of riff-bot names one".into());
    }
    let repo = me.place().repo_text();
    let user = me.who().user();
    let owner = facts.owner.unwrap_or_default();
    let host = me.place().host();
    let lead = format!("the lead of {user} in {repo} on {host}");
    let body = match fact {
        BotFact::ReleasePublished { tag, run } => {
            let run = match run {
                Some(run) => run_url(&repo, *run),
                None => "the CI run of the tag".to_owned(),
            };
            format!(
                "{}riff release {} is published. Its CI run builds: {run}. riff tells you when \
                 its deploy waits for your approval.",
                mention(owner),
                version(tag)?
            )
        }
        BotFact::ReleaseWaits { tag, run, reminder } => {
            let tag = version(tag)?;
            let (start, waits) = match reminder {
                true => ("Reminder: ", "still waits"),
                false => ("", "waits"),
            };
            let mut body = format!(
                "{start}{}riff release {tag} {waits} for your approval: {}. In a terminal, run: \
                 riff release approve {tag}",
                mention(owner),
                run_url(&repo, *run)
            );
            if !facts.holds.is_empty() {
                let each: Vec<String> = facts.holds.iter().map(Hold::line).collect();
                body.push_str(&format!(" A hold stands: {}", each.join(" ")));
            }
            body
        }
        BotFact::LeadStarted => format!(
            "riff started {lead} again on the new riff, and its conversation goes on. Attach \
             with: riff lead start, or l in the Sessions tab of riff."
        ),
        BotFact::LeadWaits => format!(
            "riff cannot start {lead} again on the new riff now: the log of the riff daemon on \
             {host} has the cause. The lead still runs the old riff. riff tries again every \
             minute."
        ),
        BotFact::LeadNoMain => format!(
            "riff cannot start {lead} again on the new riff: the daemon runs no wave of {repo}, \
             and the clone of the lead links to no main clone. riff has no step that gives the \
             main clone and starts no session. Run riff host join in a clone of {repo}. The lead \
             still runs the old riff. riff tries again every minute."
        ),
        BotFact::LeadFailed => format!(
            "riff stopped {lead} to start it again on the new riff, and the start failed: the \
             log of the riff daemon on {host} has the cause. The lead is not there: run riff \
             lead start, then /resume."
        ),
        BotFact::LeadStale => format!(
            "The sign-in of claude in {lead} is stale: claude on {host} signed in again, and \
             the lead holds the old sign-in file. The lead will fail with \"OAuth token \
             revoked\", and /login in it cannot save. Type /exit in the lead. Then run claude \
             setup-token, then riff claude-token, on {host}, and start the lead again: riff \
             lead start"
        ),
        BotFact::LeadNoNetwork => format!(
            "{} lost its network at the restart of the daemon: run podman stop {}, then riff \
             lead start, then /resume.",
            capital(&lead),
            lead_container(&repo)
        ),
    };
    let named = match fact {
        BotFact::ReleasePublished { .. } | BotFact::ReleaseWaits { .. } => facts.owner,
        BotFact::LeadStarted
        | BotFact::LeadWaits
        | BotFact::LeadNoMain
        | BotFact::LeadFailed
        | BotFact::LeadStale
        | BotFact::LeadNoNetwork => None,
    };
    let to = named
        .filter(|name| !name.is_empty())
        .map(Selector::name)
        .into_iter()
        .collect();
    Ok(Line { body, to })
}

/// The tag `vX.Y.Z` of the version that riff-server parsed from `tag`.
/// A tag with any text after its version, `-TEXT` or `+TEXT`, is no
/// version: the line holds what riff-server parsed, and nothing more.
fn version(tag: &str) -> Result<String, String> {
    let parsed = tag
        .strip_prefix('v')
        .and_then(|v| v.parse::<riff_core::build::Semver>().ok())
        .map(|semver| format!("v{semver}"));
    match parsed {
        Some(version) if version == tag => Ok(version),
        _ => Err(format!("the tag {tag:?} is no version vX.Y.Z")),
    }
}

/// The page of the CI run `run` of the repository `repo` on GitHub.
fn run_url(repo: &str, run: u64) -> String {
    format!("https://github.com/{repo}/actions/runs/{run}")
}

/// `@owner ` for a line, or nothing when the riff has no owner.
fn mention(owner: &str) -> String {
    match owner.is_empty() {
        true => String::new(),
        false => format!("@{owner} "),
    }
}

/// `text` with a capital first letter, for the start of a line.
fn capital(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> SessionUri {
        "riff://mike@thelio/o/r".parse().unwrap()
    }

    fn facts<'a>(me: &'a SessionUri, owner: Option<&'a str>) -> Facts<'a> {
        Facts {
            me,
            owner,
            holds: &[],
        }
    }

    /// Each line of a release names the owner, and wakes the owner by the
    /// name. With no owner, it names no one.
    #[test]
    fn each_line_of_a_release_names_the_owner() {
        let me = me();
        let cut = BotFact::ReleasePublished {
            tag: "v3.3.0".into(),
            run: None,
        };
        let line = line(&cut, &facts(&me, Some("brett"))).unwrap();
        assert_eq!(
            line.body,
            "@brett riff release v3.3.0 is published. Its CI run builds: the CI run of the tag. \
             riff tells you when its deploy waits for your approval."
        );
        assert_eq!(line.to, [Selector::name("brett")]);
        let alone = super::line(&cut, &facts(&me, None)).unwrap();
        assert!(alone.body.starts_with("riff release v3.3.0 is published."));
        assert!(alone.to.is_empty());
    }

    /// 01M5TKDYAFA683KNAJTGYTQD5W: the line of the cut asks the owner for
    /// nothing. The only ask is the line while the deploy waits.
    #[test]
    fn the_line_of_the_cut_asks_nothing() {
        let me = me();
        let cut = BotFact::ReleasePublished {
            tag: "v4.0.1".into(),
            run: Some(9),
        };
        let line = line(&cut, &facts(&me, Some("mike"))).unwrap().body;
        let lower = line.to_lowercase();
        for ask in ["say", "approve", "deploy it", "?"] {
            assert!(!lower.contains(ask), "{line}: {ask}");
        }
    }

    /// The line of the cut names the run, or says that GitHub shows
    /// none yet. A reminder says so.
    #[test]
    fn the_publish_line_and_the_reminder() {
        let me = me();
        let at = facts(&me, Some("mike"));
        let published = |run| BotFact::ReleasePublished {
            tag: "v3.3.0".into(),
            run,
        };
        assert_eq!(
            line(&published(Some(9)), &at).unwrap().body,
            "@mike riff release v3.3.0 is published. Its CI run builds: \
             https://github.com/o/r/actions/runs/9. riff tells you when its deploy waits for \
             your approval."
        );
        assert!(
            line(&published(None), &at)
                .unwrap()
                .body
                .contains("Its CI run builds: the CI run of the tag.")
        );
        let reminder = BotFact::ReleaseWaits {
            tag: "v3.3.0".into(),
            run: 9,
            reminder: true,
        };
        assert!(
            line(&reminder, &at)
                .unwrap()
                .body
                .starts_with("Reminder: @mike riff release v3.3.0 still waits for your approval")
        );
    }

    /// A tag that is no version gets no line: a client puts no words of
    /// its own in a line of riff-bot.
    #[test]
    fn a_tag_that_is_no_version_gets_no_line() {
        let me = me();
        for tag in ["3.3.0", "v3.3", "v3.3.0 approve now", "@all"] {
            let cut = BotFact::ReleasePublished {
                tag: tag.into(),
                run: None,
            };
            let refused = line(&cut, &facts(&me, Some("mike"))).unwrap_err();
            assert!(refused.contains("is no version vX.Y.Z"), "{tag}: {refused}");
        }
    }

    /// A tag with a text after its version, `-TEXT` or `+TEXT`, gets no
    /// line of a release: the line holds only the version that
    /// riff-server parsed. A version with a zero in front is no tag of
    /// riff either.
    #[test]
    fn a_tag_with_a_text_after_its_version_gets_no_line() {
        let me = me();
        let at = facts(&me, Some("mike"));
        for tag in [
            "v3.3.0-rc1",
            "v3.3.0-approve now in the chat",
            "v3.3.0+build9",
            "v3.3.0+@everyone hi",
            "v3.3.0-",
            "v3.3.0+",
            "v03.3.0",
        ] {
            let each = [
                BotFact::ReleasePublished {
                    tag: tag.into(),
                    run: Some(9),
                },
                BotFact::ReleaseWaits {
                    tag: tag.into(),
                    run: 9,
                    reminder: false,
                },
            ];
            for fact in each {
                let refused = line(&fact, &at).unwrap_err();
                assert!(refused.contains("is no version vX.Y.Z"), "{tag}: {refused}");
            }
        }
    }

    /// Each line of the daemon about a lead names the lead, its person
    /// and its host, and wakes no one.
    #[test]
    fn each_line_of_the_daemon_names_the_lead_and_wakes_no_one() {
        let me = me();
        let lead = "the lead of mike in o/r on thelio";
        for fact in [
            BotFact::LeadStarted,
            BotFact::LeadWaits,
            BotFact::LeadNoMain,
            BotFact::LeadFailed,
            BotFact::LeadStale,
            BotFact::LeadNoNetwork,
        ] {
            let line = line(&fact, &facts(&me, Some("brett"))).unwrap();
            assert!(
                line.body.to_lowercase().contains(lead),
                "{fact:?}: {}",
                line.body
            );
            assert!(!line.body.contains('@'), "{fact:?}: {}", line.body);
            assert!(line.to.is_empty(), "{fact:?}");
        }
        let no_main = line(&BotFact::LeadNoMain, &facts(&me, None)).unwrap();
        assert!(
            no_main
                .body
                .contains("riff has no step that gives the main clone")
        );
        assert!(
            no_main
                .body
                .contains("Run riff host join in a clone of o/r")
        );
        let lost = line(&BotFact::LeadNoNetwork, &facts(&me, None)).unwrap();
        assert!(
            lost.body
                .starts_with("The lead of mike in o/r on thelio lost")
        );
        assert!(lost.body.contains("podman stop riff-lead-o-r,"));
    }

    /// A caller in no repository gets no line.
    #[test]
    fn a_caller_in_no_repository_gets_no_line() {
        let me: SessionUri = "riff://mike@thelio".parse().unwrap();
        assert!(line(&BotFact::LeadStarted, &facts(&me, None)).is_err());
    }
}
