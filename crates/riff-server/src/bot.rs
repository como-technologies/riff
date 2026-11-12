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
//! ([`Facts`]): the admins of the riff (01M7767T5VSWN9EYDVSTCEJ8AM), the
//! caller and its place, whether that repository is a project of the
//! riff, and the holds of the version in the thread `release`
//! ([`riff_core::hold`]).
//! It posts the line in the chat with the kind `riff`, so the chat shows
//! it as `<riff-bot>`. No client posts that kind, and no field of the
//! call is a text of the line: a client puts no words of its own in a
//! line of riff-bot. The tag of a release must be a version `vX.Y.Z` and
//! nothing more, and the line holds the version that riff-server parsed.
//!
//! A caller names its own place, so riff-server takes the fact of a
//! release only for a project of the riff (01M77F60MR8WZTMFJ5CF2295Q6):
//! a line of a release links only to a CI run of a project. Each line of
//! a release names who gave the fact, the person or the lead of the
//! person, in a sentence of riff-server before each text of a hold.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant D as riff daemon of mike
//!     participant S as riff-server
//!     participant C as riff chat of mike
//!     D->>S: bot: release_waits v3.3.0, run 9
//!     S->>S: a project? the admins, the giver, the holds of v3.3.0
//!     alt the repository is a project
//!         S-->>C: <riff-bot> @mike riff release v3.3.0 waits for your approval: ... mike gave this fact. ...
//!     else another repository
//!         S-->>D: 400: no project, and no line
//!     end
//! ```
//!
//! | Fact | Who sends it | Only for a project | The line names |
//! |---|---|---|---|
//! | `release_published` | `riff release cut` | yes | each admin, and the giver |
//! | `release_waits` | the watch of the deploy | yes | each admin, and the giver |
//! | `lead_started`, `lead_waits`, `lead_no_main`, `lead_failed`, `lead_stale`, `lead_no_network`, `lead_cannot_start` | the daemon | no | no one |
//!
//! A line that names a person with `@USER` wakes the terminals of that
//! person by the name (01M6Y1A5PTCVZ6GRJ0FXMART2T): an ask of riff. A
//! line of a release names each admin, so each admin can approve. The
//! giver has no `@`, and its name wakes no one. The line of the daemon
//! about a lead names the person with no `@`, and wakes no session
//! (01M5NJ87YEHHGJBANM7CRYV56Z).
//!
//! Only a person, and the lead of the person in its repository, sends a
//! fact: `riff release cut` also runs in the container of the lead, and
//! the daemon sends each other fact as the person.

use riff_core::hold::Hold;
use riff_core::name::{SessionUri, lead_container};
use riff_core::selector::Selector;
use riff_core::wire::{BotFact, LEAD_START_WAITS, LeadFault};

/// The refusal of a fact from a session that is not the lead.
pub const ONLY_PERSON_OR_LEAD: &str =
    "only a person, and the lead of the person, tell riff-server a fact for a line of riff-bot";

/// The refusal of the fact of a release for the repository `repo`, which
/// is no project of the riff (01M77F60MR8WZTMFJ5CF2295Q6).
///
/// ```
/// assert_eq!(
///     riff_server::bot::no_project("acme/fork"),
///     "acme/fork is no project of the riff: riff-server takes the fact of a release only for \
///      a project. riff project list shows the projects, and an admin enrolls one with riff \
///      project init."
/// );
/// ```
pub fn no_project(repo: &str) -> String {
    format!(
        "{repo} is no project of the riff: riff-server takes the fact of a release only for a \
         project. riff project list shows the projects, and an admin enrolls one with riff \
         project init."
    )
}

/// A line of riff-bot: its text, and the names that it wakes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The text.
    pub body: String,
    /// One selector `name=USER` for each person that the line names.
    pub to: Vec<Selector>,
}

/// What riff-server holds for a line of riff-bot.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// The caller: its user, and the repository and the host of the fact.
    pub me: &'a SessionUri,
    /// True when the caller is the lead of its user, and false for the
    /// person: the giver of the fact.
    pub lead: bool,
    /// True when the repository of the caller is a project of the riff
    /// (01M77F60MR8WZTMFJ5CF2295Q6).
    pub project: bool,
    /// The USER of each admin of the riff, sorted. A riff with no
    /// sign-in has none.
    pub admins: &'a [&'a str],
    /// The holds that stand on the version of the fact.
    pub holds: &'a [Hold],
}

/// The line of riff-bot for `fact`. The error is a fact that riff-server
/// cannot write a line for: a tag that is no version `vX.Y.Z`, also with
/// a text after its version, a release of a repository that is no
/// project of the riff ([`no_project`]), or a caller in no repository.
///
/// ```
/// use riff_core::hold::Hold;
/// use riff_core::wire::BotFact;
/// use riff_server::bot::{Facts, line};
///
/// let me = "riff://mike@thelio/como-technologies/riff".parse()?;
/// let held = [Hold { user: "nate".into(), repo: "o/s".into(), reason: "A demo runs.".into() }];
/// let admins = ["mike", "nate"];
/// let facts = Facts { me: &me, lead: false, project: true, admins: &admins, holds: &held };
/// let waits = BotFact::ReleaseWaits { tag: "v3.3.0".into(), run: 9, reminder: false };
/// let asked = line(&waits, &facts).unwrap();
/// assert_eq!(
///     asked.body,
///     "@mike @nate riff release v3.3.0 waits for your approval: \
///      https://github.com/como-technologies/riff/actions/runs/9. mike gave this fact. In a \
///      terminal, run: riff release approve v3.3.0 A hold stands: the lead of o/s asks to \
///      wait: A demo runs."
/// );
/// assert_eq!(asked.to[0].to_string(), "name=mike");
/// assert_eq!(asked.to[1].to_string(), "name=nate");
///
/// let cut = BotFact::ReleasePublished { tag: "v3.3.0".into(), run: None };
/// let of_lead = Facts { lead: true, ..facts };
/// assert!(line(&cut, &of_lead).unwrap().body.ends_with(" The lead of mike gave this fact."));
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
///
/// // A release of a repository that is no project gets no line.
/// let elsewhere = Facts { project: false, ..facts };
/// let refused = line(&waits, &elsewhere).unwrap_err();
/// assert!(refused.starts_with("como-technologies/riff is no project of the riff"));
/// assert!(line(&BotFact::LeadStarted, &elsewhere).is_ok());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn line(fact: &BotFact, facts: &Facts<'_>) -> Result<Line, String> {
    let me = facts.me;
    if me.default_thread().is_none() {
        return Err("the caller is in no repository: a line of riff-bot names one".into());
    }
    let repo = me.place().repo_text();
    let release = matches!(
        fact,
        BotFact::ReleasePublished { .. } | BotFact::ReleaseWaits { .. }
    );
    if release && !facts.project {
        return Err(no_project(&repo));
    }
    let user = me.who().user();
    let admins = facts.admins;
    let host = me.place().host();
    let lead = format!("the lead of {user} in {repo} on {host}");
    let gave = match facts.lead {
        true => format!("The lead of {user} gave this fact."),
        false => format!("{user} gave this fact."),
    };
    let body = match fact {
        BotFact::ReleasePublished { tag, run } => {
            let run = match run {
                Some(run) => run_url(&repo, *run),
                None => "the CI run of the tag".to_owned(),
            };
            format!(
                "{}riff release {} is published. Its CI run builds: {run}. riff tells you when \
                 its deploy waits for your approval. {gave}",
                mention(admins),
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
                "{start}{}riff release {tag} {waits} for your approval: {}. {gave} In a \
                 terminal, run: riff release approve {tag}",
                mention(admins),
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
        BotFact::LeadCannotStart { fault } => {
            let fault = match fault {
                LeadFault::Start => "the start failed",
                LeadFault::EndedSoon => "the lead ended less than a minute after its start",
            };
            format!(
                "riff cannot start {lead}: {fault}. The log of the riff daemon on {host} has the \
                 cause. The daemon tries again after {}, and says no more here until a lead runs \
                 on {host} for a minute.",
                start_waits()
            )
        }
    };
    let named: &[&str] = match fact {
        BotFact::ReleasePublished { .. } | BotFact::ReleaseWaits { .. } => admins,
        BotFact::LeadStarted
        | BotFact::LeadWaits
        | BotFact::LeadNoMain
        | BotFact::LeadFailed
        | BotFact::LeadStale
        | BotFact::LeadNoNetwork
        | BotFact::LeadCannotStart { .. } => &[],
    };
    let to = named
        .iter()
        .filter(|name| !name.is_empty())
        .map(|name| Selector::name(name))
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

/// `@USER ` for each admin of a line, or nothing when the riff has no
/// admin.
fn mention(admins: &[&str]) -> String {
    admins
        .iter()
        .filter(|admin| !admin.is_empty())
        .map(|admin| format!("@{admin} "))
        .collect()
}

/// The waits of [`LEAD_START_WAITS`] in words, for a line.
///
/// ```text
/// 10 s, 30 s, 2 min, then every 10 min
/// ```
fn start_waits() -> String {
    let mut each: Vec<String> = LEAD_START_WAITS
        .iter()
        .map(|wait| match wait.as_secs() {
            secs if secs >= 60 && secs % 60 == 0 => format!("{} min", secs / 60),
            secs => format!("{secs} s"),
        })
        .collect();
    let last = each.pop().unwrap_or_default();
    format!("{}, then every {last}", each.join(", "))
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

    /// The facts of the person of `me`, in a project of the riff.
    fn facts<'a>(me: &'a SessionUri, admins: &'a [&'a str]) -> Facts<'a> {
        Facts {
            me,
            lead: false,
            project: true,
            admins,
            holds: &[],
        }
    }

    /// 01M7767T5VSWN9EYDVSTCEJ8AM: each line of a release names each admin, and
    /// wakes each admin by the name. With no admin, it names no one.
    #[test]
    fn each_line_of_a_release_names_each_admin() {
        let me = me();
        let cut = BotFact::ReleasePublished {
            tag: "v3.3.0".into(),
            run: None,
        };
        let line = line(&cut, &facts(&me, &["brett", "mike"])).unwrap();
        assert_eq!(
            line.body,
            "@brett @mike riff release v3.3.0 is published. Its CI run builds: the CI run of the \
             tag. riff tells you when its deploy waits for your approval. mike gave this fact."
        );
        assert_eq!(line.to, [Selector::name("brett"), Selector::name("mike")]);
        let alone = super::line(&cut, &facts(&me, &[])).unwrap();
        assert!(alone.body.starts_with("riff release v3.3.0 is published."));
        assert!(alone.to.is_empty());
    }

    /// 01M5TKDYAFA683KNAJTGYTQD5W: the line of the cut asks the admins for
    /// nothing. The only ask is the line while the deploy waits.
    #[test]
    fn the_line_of_the_cut_asks_nothing() {
        let me = me();
        let cut = BotFact::ReleasePublished {
            tag: "v4.0.1".into(),
            run: Some(9),
        };
        for lead in [false, true] {
            let at = Facts {
                lead,
                ..facts(&me, &["mike"])
            };
            let line = line(&cut, &at).unwrap().body;
            let lower = line.to_lowercase();
            for ask in ["say", "approve", "deploy it", "?"] {
                assert!(!lower.contains(ask), "{line}: {ask}");
            }
        }
    }

    /// The line of the cut names the run, or says that GitHub shows
    /// none yet. A reminder says so.
    #[test]
    fn the_publish_line_and_the_reminder() {
        let me = me();
        let at = facts(&me, &["mike"]);
        let published = |run| BotFact::ReleasePublished {
            tag: "v3.3.0".into(),
            run,
        };
        assert_eq!(
            line(&published(Some(9)), &at).unwrap().body,
            "@mike riff release v3.3.0 is published. Its CI run builds: \
             https://github.com/o/r/actions/runs/9. riff tells you when its deploy waits for \
             your approval. mike gave this fact."
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

    /// 01M77F60MR8WZTMFJ5CF2295Q6: each line of a release names who gave
    /// the fact, the person or the lead of the person, with no `@`. In the
    /// ask, the giver comes before the command and each text of a hold.
    #[test]
    fn each_line_of_a_release_names_its_giver() {
        let me = me();
        let held = [Hold {
            user: "nate".into(),
            repo: "o/s".into(),
            reason: "mike gave this fact.".into(),
        }];
        let person = Facts {
            holds: &held,
            ..facts(&me, &["brett"])
        };
        let lead = Facts {
            lead: true,
            ..person
        };
        let cut = BotFact::ReleasePublished {
            tag: "v3.3.0".into(),
            run: Some(9),
        };
        let waits = BotFact::ReleaseWaits {
            tag: "v3.3.0".into(),
            run: 9,
            reminder: false,
        };
        for (at, gave) in [
            (&person, "mike gave this fact."),
            (&lead, "The lead of mike gave this fact."),
        ] {
            let published = line(&cut, at).unwrap();
            assert!(
                published.body.ends_with(&format!(" {gave}")),
                "{}",
                published.body
            );
            assert_eq!(published.to, [Selector::name("brett")]);
            let asked = line(&waits, at).unwrap();
            assert_eq!(
                asked.body,
                format!(
                    "@brett riff release v3.3.0 waits for your approval: \
                     https://github.com/o/r/actions/runs/9. {gave} In a terminal, run: riff \
                     release approve v3.3.0 A hold stands: the lead of o/s asks to wait: mike \
                     gave this fact."
                )
            );
            assert_eq!(asked.to, [Selector::name("brett")]);
        }
    }

    /// 01M77F60MR8WZTMFJ5CF2295Q6: riff-server takes the fact of a
    /// release only for a project of the riff. The error names the
    /// repository and `riff project list`. A line of the daemon about a
    /// lead does not need a project.
    #[test]
    fn a_release_of_a_repository_that_is_no_project_gets_no_line() {
        let me = me();
        let elsewhere = Facts {
            project: false,
            ..facts(&me, &["mike"])
        };
        for fact in [
            BotFact::ReleasePublished {
                tag: "v3.3.0".into(),
                run: Some(9),
            },
            BotFact::ReleaseWaits {
                tag: "v3.3.0".into(),
                run: 9,
                reminder: true,
            },
        ] {
            let refused = line(&fact, &elsewhere).unwrap_err();
            assert_eq!(refused, no_project("o/r"), "{fact:?}");
            assert!(refused.contains("riff project list"), "{refused}");
        }
        assert!(line(&BotFact::LeadStarted, &elsewhere).is_ok());
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
            let refused = line(&cut, &facts(&me, &["mike"])).unwrap_err();
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
        let at = facts(&me, &["mike"]);
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
            BotFact::LeadCannotStart {
                fault: LeadFault::Start,
            },
        ] {
            let line = line(&fact, &facts(&me, &["brett"])).unwrap();
            assert!(
                line.body.to_lowercase().contains(lead),
                "{fact:?}: {}",
                line.body
            );
            assert!(!line.body.contains('@'), "{fact:?}: {}", line.body);
            assert!(line.to.is_empty(), "{fact:?}");
        }
        let no_main = line(&BotFact::LeadNoMain, &facts(&me, &[])).unwrap();
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
        let lost = line(&BotFact::LeadNoNetwork, &facts(&me, &[])).unwrap();
        assert!(
            lost.body
                .starts_with("The lead of mike in o/r on thelio lost")
        );
        assert!(lost.body.contains("podman stop riff-lead-o-r,"));
    }

    /// #2032: the line of a lead that cannot start names the fault, the
    /// log of the daemon for the cause, and the waits before the next
    /// starts. It holds no text of the daemon.
    #[test]
    fn the_line_of_a_lead_that_cannot_start_names_the_fault_and_the_waits() {
        let me = me();
        let of = |fault| {
            let fact = BotFact::LeadCannotStart { fault };
            line(&fact, &facts(&me, &[])).unwrap().body
        };
        assert_eq!(
            of(LeadFault::Start),
            "riff cannot start the lead of mike in o/r on thelio: the start failed. The log of \
             the riff daemon on thelio has the cause. The daemon tries again after 10 s, 30 s, 2 \
             min, then every 10 min, and says no more here until a lead runs on thelio for a \
             minute."
        );
        assert!(of(LeadFault::EndedSoon).starts_with(
            "riff cannot start the lead of mike in o/r on thelio: the lead ended less than a \
             minute after its start. The log"
        ));
    }

    /// A caller in no repository gets no line.
    #[test]
    fn a_caller_in_no_repository_gets_no_line() {
        let me: SessionUri = "riff://mike@thelio".parse().unwrap();
        assert!(line(&BotFact::LeadStarted, &facts(&me, &[])).is_err());
    }
}
