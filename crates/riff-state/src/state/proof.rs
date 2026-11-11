//! The rule of a proof of presence (01M75WKSFX84BHHGFX8VJ37DC6): one function for
//! each action of the list, in a command and in a step that riff-server
//! does not do itself.
//!
//! - [`Command::proof`](super::Command::proof) gives the action of a
//!   command. [`State::check`](super::State::check) runs [`check`] after
//!   `permits` and before `handle`, under the lock.
//! - The engine of riff-server marks the caller with the action of the
//!   proof of its call ([`Caller::with_proof`](super::Caller::with_proof)),
//!   and uses the proof only when the check passed it.
//!
//! ```mermaid
//! flowchart TD
//!     A[an action of the list] --> P{a proof of the call?}
//!     P -->|for this action| G[good: the proof is used]
//!     P -->|for another action| B[bad_proof]
//!     P -->|none| R{the policy is required,<br/>or the action is the policy or a test?}
//!     R -->|yes| N[needs_proof]
//!     R -->|no| O[good, with no proof]
//! ```

use riff_core::proof::{Action, Policy};

use super::command::{Code, Refused};

/// What a person reads when an action needs a proof that the call does
/// not have.
///
/// ```
/// use riff_core::proof::Action;
///
/// let text = riff_state::state::proof::needs_text(&Action::PauseRiff);
/// assert!(text.starts_with("pause the whole riff needs a proof of presence"), "{text}");
/// assert!(text.contains("riff device add --key NAME"), "{text}");
/// ```
pub fn needs_text(action: &Action) -> String {
    format!(
        "{} needs a proof of presence: a touch of your authenticator. riff asks for it in your \
         browser. A session never gives one: your person runs the command in a terminal. With no \
         authenticator, add one first: riff device add --key NAME",
        action.text()
    )
}

/// The rule of the proof of `action` with the policy `policy`, for a
/// call whose proof is for `proved`. Gives true when the call uses its
/// proof, false when the action goes on with no proof, or the refusal.
///
/// - A proof for the action is good, with each policy: the log marks
///   the action with it.
/// - A proof for another action is refused with `bad_proof`.
/// - With no proof, the policy `required` refuses with `needs_proof`.
///   The change of the policy and the test of a proof need a proof with
///   each policy.
///
/// ```
/// use riff_core::proof::{Action, Policy};
/// use riff_state::state::Code;
/// use riff_state::state::proof::check;
///
/// let pause = Action::PauseRiff;
/// assert_eq!(check(Policy::Off, &pause, None), Ok(false));
/// assert_eq!(check(Policy::Off, &pause, Some(&pause)), Ok(true));
/// assert_eq!(check(Policy::Required, &pause, Some(&pause)), Ok(true));
/// assert_eq!(check(Policy::Required, &pause, None).unwrap_err().code, Code::NeedsProof);
/// let other = check(Policy::Off, &pause, Some(&Action::ResumeRiff)).unwrap_err();
/// assert_eq!(other.code, Code::BadProof);
/// let policy = Action::Policy { proof: Policy::Required };
/// assert_eq!(check(Policy::Off, &policy, None).unwrap_err().code, Code::NeedsProof);
/// ```
pub fn check(policy: Policy, action: &Action, proved: Option<&Action>) -> Result<bool, Refused> {
    match proved {
        Some(proved) if proved == action => Ok(true),
        Some(proved) => Err(Refused::new(
            Code::BadProof,
            format!(
                "the proof of the call is for the action \"{}\", not for \"{}\": give a proof \
                 for this action",
                proved.text(),
                action.text()
            ),
        )),
        None if policy == Policy::Required || always(action) => {
            Err(Refused::new(Code::NeedsProof, needs_text(action)))
        }
        None => Ok(false),
    }
}

/// True for an action that needs a proof with each policy: the change
/// of the policy, and the test of a proof.
fn always(action: &Action) -> bool {
    matches!(action, Action::Policy { .. } | Action::Test)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_and_the_policy_need_a_proof_with_each_policy() {
        for action in [Action::Test, Action::Policy { proof: Policy::Off }] {
            for policy in [Policy::Off, Policy::Required] {
                let refused = check(policy, &action, None).unwrap_err();
                assert_eq!(refused.code, Code::NeedsProof, "{action:?} {policy:?}");
                assert_eq!(check(policy, &action, Some(&action)), Ok(true));
            }
        }
    }

    #[test]
    fn a_proof_for_another_action_names_both() {
        let refused = check(
            Policy::Required,
            &Action::invite("bob@acme.io"),
            Some(&Action::invite("cy@acme.io")),
        )
        .unwrap_err();
        assert_eq!(refused.code, Code::BadProof);
        assert!(
            refused.reason.contains("\"invite cy@acme.io\""),
            "{refused}"
        );
        assert!(
            refused.reason.contains("\"invite bob@acme.io\""),
            "{refused}"
        );
    }
}
