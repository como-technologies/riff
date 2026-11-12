//! A proof of presence: the policy of the riff, and the actions that a
//! proof covers.
//!
//! A proof is a fresh signature of an authenticator of a person: a touch
//! of a security key, a fingerprint or a PIN. riff-server gives a
//! challenge that names one [`Action`] and the person. The person signs
//! it in the browser, and riff-server checks the signature with a key of
//! the person. Then the call of the action carries the ticket of the
//! proof in the header [`PROOF_HEADER`]. A proof is good for one action
//! of one person, one time, for 5 minutes.
//!
//! ```mermaid
//! flowchart TD
//!     C[a call of an action] --> H{the header riff-proof?}
//!     H -->|yes| T{a proof of the person, for this action,<br/>not used, not old?}
//!     T -->|no| X[refused: bad_proof]
//!     T -->|yes| A[the action, with a proof]
//!     H -->|no| P{the policy}
//!     P -->|off| N[the action, with no proof]
//!     P -->|required| R[refused: needs_proof,<br/>and how to give a proof]
//! ```
//!
//! The [`Policy`] of a riff is [`Policy::Off`] or [`Policy::Required`].
//! A new riff is `off`. The change of the policy needs a proof also when
//! the policy is `off`.
//!
//! The client and the server make the action of a call with the same
//! constructor, so a proof for one action never matches another:
//!
//! ```
//! use riff_core::proof::{Action, Policy};
//!
//! let approve = Action::Approve { repo: "acme/app".parse()?, tag: "v1.2.0".into() };
//! assert_eq!(approve.text(), "approve the deploy of v1.2.0 of acme/app");
//! assert_ne!(approve, Action::Approve { repo: "acme/app".parse()?, tag: "v1.3.0".into() });
//!
//! // An email is in lower case, as riff-server keeps it.
//! assert_eq!(Action::invite("Bob@Acme.io"), Action::invite("bob@acme.io"));
//! assert_eq!(Policy::default(), Policy::Off);
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::name::ThreadName;

/// The HTTP header of the ticket of a proof on the call of an action.
pub const PROOF_HEADER: &str = "riff-proof";

/// The code of the refusal of an action with no proof while the policy
/// is `required`, or of a change of the policy with no proof.
pub const NEEDS_PROOF: &str = "needs_proof";

/// The code of the refusal of a proof that riff-server does not take:
/// one for another action or person, one that is used or old, or one
/// whose signature did not check.
pub const BAD_PROOF: &str = "bad_proof";

/// How long a proof is good, from its start, in seconds.
pub const LIFE_SECS: u64 = 5 * 60;

/// The policy of the proof of presence of a riff.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    /// No action asks for a proof. An action that has one is marked in
    /// the log.
    #[default]
    Off,
    /// Each action of the list needs a proof: riff-server refuses it
    /// with no proof.
    Required,
}

impl Policy {
    /// The word of the policy: `off` or `required`.
    ///
    /// ```
    /// use riff_core::proof::Policy;
    ///
    /// assert_eq!(Policy::Required.word(), "required");
    /// assert_eq!("off".parse::<Policy>(), Ok(Policy::Off));
    /// assert!("on".parse::<Policy>().is_err());
    /// ```
    pub fn word(self) -> &'static str {
        match self {
            Policy::Off => "off",
            Policy::Required => "required",
        }
    }

    /// True for [`Policy::Off`], for a field that a reply leaves out.
    pub fn is_off(&self) -> bool {
        *self == Policy::Off
    }
}

impl std::fmt::Display for Policy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

impl std::str::FromStr for Policy {
    type Err = String;

    fn from_str(text: &str) -> Result<Policy, String> {
        match text {
            "off" => Ok(Policy::Off),
            "required" => Ok(Policy::Required),
            other => Err(format!("the policy {other:?} is not off or required")),
        }
    }
}

/// An action that a proof covers. Its [`Action::text`] is on the page
/// of the proof, and in the record `proved`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// The test of an authenticator: `riff device test`.
    Test,
    /// The change of the policy of the riff.
    Policy { proof: Policy },
    /// The approval of the deploy of the release `tag` of `repo`: `riff
    /// release approve`.
    Approve { repo: ThreadName, tag: String },
    /// The answer to the questions of the issue `number` of `repo`: the
    /// decision that waits for the person.
    Answer { repo: ThreadName, number: u64 },
    /// The pause of the whole riff.
    PauseRiff,
    /// The resume of the whole riff.
    ResumeRiff,
    /// The invite of the person `email`.
    Invite { email: String },
    /// The removal of the member `email`.
    Remove { email: String },
    /// `admin` true makes `email` an admin, false a member again.
    Admin { email: String, admin: bool },
    /// The end of the session `name` on the host `host`.
    End { host: String, name: String },
    /// The end of each session of `repo` on `host` now: `riff host leave
    /// --now`.
    Leave { host: String, repo: String },
    /// The end of the lead of `repo` on `host`, for a new lead: `riff lead
    /// restart`.
    LeadRestart { host: String, repo: String },
    /// The removal of the folder `name` on `host`, with the work in its
    /// clone: `riff daemon sweep --remove NAME --force`.
    Sweep { host: String, name: String },
    /// The take-over of the session `name` on `host`.
    TakeOver { host: String, name: String },
    /// The add of the key `name` by a person who has a key already.
    AddKey { name: String },
    /// The removal of the device `name` of the person.
    RemoveDevice { name: String },
}

impl Action {
    /// The invite of `email`, in lower case.
    pub fn invite(email: &str) -> Action {
        Action::Invite {
            email: email.trim().to_lowercase(),
        }
    }

    /// The removal of `email`, in lower case.
    pub fn remove(email: &str) -> Action {
        Action::Remove {
            email: email.trim().to_lowercase(),
        }
    }

    /// The change of the role of `email`, in lower case.
    pub fn admin(email: &str, admin: bool) -> Action {
        Action::Admin {
            email: email.trim().to_lowercase(),
            admin,
        }
    }

    /// The action as a person reads it, on the page of the proof and in
    /// the log.
    ///
    /// ```
    /// use riff_core::proof::{Action, Policy};
    ///
    /// let end = Action::End { host: "pangolin".into(), name: "riff-issue-12-1a2b3c4d".into() };
    /// assert_eq!(end.text(), "end the session riff-issue-12-1a2b3c4d on the host pangolin");
    /// assert_eq!(Action::Policy { proof: Policy::Required }.text(), "set the proof of presence of the riff to required");
    /// ```
    pub fn text(&self) -> String {
        match self {
            Action::Test => "test a proof of presence".into(),
            Action::Policy { proof } => {
                format!("set the proof of presence of the riff to {proof}")
            }
            Action::Approve { repo, tag } => format!("approve the deploy of {tag} of {repo}"),
            Action::Answer { repo, number } => {
                format!("answer the questions of the issue #{number} of {repo}")
            }
            Action::PauseRiff => "pause the whole riff".into(),
            Action::ResumeRiff => "resume the whole riff".into(),
            Action::Invite { email } => format!("invite {email}"),
            Action::Remove { email } => format!("remove {email} from the riff"),
            Action::Admin { email, admin: true } => format!("make {email} an admin"),
            Action::Admin {
                email,
                admin: false,
            } => format!("make {email} a member again"),
            Action::End { host, name } => format!("end the session {name} on the host {host}"),
            Action::Leave { host, repo } => {
                format!("end the sessions of {repo} on the host {host} now")
            }
            Action::LeadRestart { host, repo } => {
                format!("end the lead of {repo} on the host {host}, for a new lead")
            }
            Action::Sweep { host, name } => {
                format!("remove the folder {name} on the host {host}, with its work")
            }
            Action::TakeOver { host, name } => {
                format!("take over the session {name} on the host {host}")
            }
            Action::AddKey { name } => format!("add the key {name}"),
            Action::RemoveDevice { name } => format!("remove the device {name}"),
        }
    }
}

/// The action of a request to the riff daemon of `host`, when the
/// request is a step that can lose work. The body is the text of the
/// request, as `riff ... --host HOST` posts it to the daemon: `daemon
/// end NAME`, `leave REPO --now`, `lead restart`, or `daemon sweep --remove
/// NAME --force`. `repo` is the repository of the daemon, as
/// `OWNER/REPO`: `lead restart` ends its lead. The daemon reads the words
/// of a request, so this reads them the same way. Each other body gives
/// `None`.
///
/// ```
/// use riff_core::proof::{Action, of_step};
///
/// let end = of_step("daemon end riff-issue-12-1a2b3c4d", "pangolin", "acme/app");
/// assert_eq!(end, Some(Action::End { host: "pangolin".into(), name: "riff-issue-12-1a2b3c4d".into() }));
/// assert_eq!(of_step("daemon status", "pangolin", "acme/app"), None);
/// ```
pub fn of_step(body: &str, host: &str, repo: &str) -> Option<Action> {
    let host = host.to_owned();
    match body.split_whitespace().collect::<Vec<_>>()[..] {
        ["daemon", "end", name] => Some(Action::End {
            host,
            name: name.to_owned(),
        }),
        ["daemon", "sweep", "--remove", name, "--force"] => Some(Action::Sweep {
            host,
            name: name.to_owned(),
        }),
        ["leave", repo, "--now"] => Some(Action::Leave {
            host,
            repo: repo.to_owned(),
        }),
        ["lead", "restart"] => Some(Action::LeadRestart {
            host,
            repo: repo.to_owned(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_reads_as_its_json() {
        let end = Action::End {
            host: "pangolin".into(),
            name: "riff-issue-12-1a2b3c4d".into(),
        };
        let json = serde_json::to_string(&end).unwrap();
        assert_eq!(
            json,
            r#"{"action":"end","host":"pangolin","name":"riff-issue-12-1a2b3c4d"}"#
        );
        assert_eq!(serde_json::from_str::<Action>(&json).unwrap(), end);
        let test: Action = serde_json::from_str(r#"{"action":"test"}"#).unwrap();
        assert_eq!(test, Action::Test);
    }

    #[test]
    fn each_action_has_a_text_of_its_own() {
        let repo: ThreadName = "acme/app".parse().unwrap();
        let host = || "pangolin".to_owned();
        let name = || "riff-issue-12-1a2b3c4d".to_owned();
        let all = [
            Action::Test,
            Action::Policy { proof: Policy::Off },
            Action::Policy {
                proof: Policy::Required,
            },
            Action::Approve {
                repo: repo.clone(),
                tag: "v1.0.0".into(),
            },
            Action::Answer {
                repo: repo.clone(),
                number: 7,
            },
            Action::PauseRiff,
            Action::ResumeRiff,
            Action::invite("bob@acme.io"),
            Action::remove("bob@acme.io"),
            Action::admin("bob@acme.io", true),
            Action::admin("bob@acme.io", false),
            Action::End {
                host: host(),
                name: name(),
            },
            Action::Leave {
                host: host(),
                repo: "acme/app".into(),
            },
            Action::LeadRestart {
                host: host(),
                repo: "acme/app".into(),
            },
            Action::Sweep {
                host: host(),
                name: name(),
            },
            Action::TakeOver {
                host: host(),
                name: name(),
            },
            Action::AddKey { name: name() },
            Action::RemoveDevice { name: name() },
        ];
        let texts: std::collections::BTreeSet<String> = all.iter().map(Action::text).collect();
        assert_eq!(texts.len(), all.len());
    }

    #[test]
    fn only_a_step_that_can_lose_work_is_an_action() {
        let step = |body: &str| of_step(body, "pangolin", "acme/app");
        assert_eq!(
            step("daemon end riff-issue-12-1a2b3c4d"),
            Some(Action::End {
                host: "pangolin".into(),
                name: "riff-issue-12-1a2b3c4d".into()
            })
        );
        // The daemon reads the words of a request, so the check does too.
        assert_eq!(
            step("  daemon   end\triff-issue-12-1a2b3c4d "),
            step("daemon end riff-issue-12-1a2b3c4d")
        );
        assert_eq!(
            step("daemon sweep --remove issue-12-1a2b3c4d --force"),
            Some(Action::Sweep {
                host: "pangolin".into(),
                name: "issue-12-1a2b3c4d".into()
            })
        );
        assert_eq!(
            step("leave acme/other --now"),
            Some(Action::Leave {
                host: "pangolin".into(),
                repo: "acme/other".into()
            })
        );
        assert_eq!(
            step("lead restart"),
            Some(Action::LeadRestart {
                host: "pangolin".into(),
                repo: "acme/app".into()
            })
        );
        for safe in [
            "daemon status",
            "daemon sweep",
            "daemon sweep --remove issue-12-1a2b3c4d",
            "leave acme/app",
            "daemon end",
            "daemon end a b",
            "join acme/app",
            "join acme/app --no-lead",
            "lead move",
            "lead stop",
            "hello",
            "",
        ] {
            assert_eq!(step(safe), None, "{safe:?}");
        }
    }
}
