//! The rules of the proof of presence (01M75WKS7H066EXQPGF003B4SC,
//! 01M75WKSADNAW4RNFAXD48D7DF, 01M75WKSD5A66RHQMMM5CMKWJB,
//! 01M75WKSFX84BHHGFX8VJ37DC6, 01M75WKSJTB28ZQV8ADPBQ86WE,
//! 01M75WKSRP6P4GGJPWTVRMDW85, 01M75WKSVEXMNGZQ4RYVGXH0FX,
//! 01M75WKS1TS9FME704FQNBZQN1, 01M75WKS4RF17WF85MWA4GFK0C): one test for
//! each rule, in the form given, when, then.

use riff_core::proof::{Action, Policy};
use riff_core::record::{KeyAdded, PolicySet, Proved};
use riff_core::selector::Selector;
use riff_core::wire::{Approve, Invite, Post, RemoveDevice, SetPolicy};

use super::*;
use crate::state::Prove;

fn policy_set(proof: Policy) -> Change {
    Change::PolicySet(PolicySet { proof })
}

fn required() -> Change {
    policy_set(Policy::Required)
}

/// ann, a person and an admin.
fn admin() -> Caller {
    Caller::of(&person()).with_role(Role::Admin)
}

fn set(proof: Policy) -> Whole<SetPolicy> {
    Whole(SetPolicy { proof })
}

fn key_added(name: &str) -> Change {
    Change::KeyAdded(KeyAdded {
        user: "ann".into(),
        name: name.into(),
        credential: format!("c-{name}"),
        public_key: "pQ".into(),
    })
}

fn prove(name: &str, counter: u64) -> Whole<Prove> {
    Whole(Prove {
        name: name.into(),
        action: Action::PauseRiff,
        counter,
    })
}

fn proved(name: &str, counter: u64) -> Change {
    Change::Proved(Proved {
        user: "ann".into(),
        name: name.into(),
        action: "pause the whole riff".into(),
        counter,
    })
}

/// An admin with a proof sets the policy, with each policy. With no
/// proof, or with a proof of another action, the change is refused, also
/// from `off`. The same policy again makes no record.
#[test]
fn an_admin_with_a_proof_sets_the_policy() {
    let on = Action::Policy {
        proof: Policy::Required,
    };
    given(&[])
        .when_as(&admin().with_proof(on.clone()), set(Policy::Required))
        .then(&[required()]);
    given(&[])
        .when_as(&admin(), set(Policy::Required))
        .then_refused_as(Code::NeedsProof, "needs a proof of presence");
    given(&[])
        .when_as(&admin().with_proof(Action::Test), set(Policy::Required))
        .then_refused_as(
            Code::BadProof,
            "is for the action \"test a proof of presence\"",
        );
    given(&[required()])
        .when_as(&admin().with_proof(on), set(Policy::Required))
        .then(&[]);
    let off = Action::Policy { proof: Policy::Off };
    given(&[required()])
        .when_as(&admin().with_proof(off), set(Policy::Off))
        .then(&[policy_set(Policy::Off)]);
}

/// A member cannot set the policy, also with a proof, and a session
/// cannot, also as an admin.
#[test]
fn a_member_or_a_session_cannot_set_the_policy() {
    let on = Action::Policy {
        proof: Policy::Required,
    };
    given(&[])
        .when_as(
            &Caller::of(&person()).with_proof(on.clone()),
            set(Policy::Required),
        )
        .then_refused_as(Code::NotAllowed, "ann is not an admin");
    let session = Caller::of(&ann()).with_role(Role::Admin).with_proof(on);
    given(&[])
        .when_as(&session, set(Policy::Required))
        .then_refused_as(
            Code::NotAllowed,
            "a session cannot send the command set_policy",
        );
}

/// With `required`, a pause of the whole riff needs a proof; a pause of
/// a repository does not, and nor does the pause of the server.
#[test]
fn with_required_a_pause_of_the_whole_riff_needs_a_proof() {
    let running = [required(), riff_set(RiffState::Running)];
    given(&running)
        .when_as(&admin(), PauseRiff)
        .then_refused_as(Code::NeedsProof, "pause the whole riff needs a proof");
    given(&running)
        .when_as(&admin().with_proof(Action::PauseRiff), PauseRiff)
        .then(&[riff_set(RiffState::Paused)]);
    given(&running)
        .when_as(&Caller::server(), PauseRiff)
        .then(&[riff_set(RiffState::Paused)]);
    given(&[required(), riff_set(RiffState::Paused)])
        .when_as(&admin(), ResumeRiff)
        .then_refused_as(Code::NeedsProof, "resume the whole riff needs a proof");
    given(&running)
        .when_as(&admin(), PauseNamed(repo()))
        .then(&[repo_set(RiffState::Paused)]);
    // With off, no proof is asked.
    given(&[riff_set(RiffState::Running)])
        .when_as(&admin(), PauseRiff)
        .then(&[riff_set(RiffState::Paused)]);
}

/// With `required`, a proof of one action does not do another: an
/// invite of another person is refused.
#[test]
fn a_proof_of_one_action_does_not_do_another() {
    let invite = Whole(Invite {
        email: "bob@acme.io".into(),
    });
    given(&[required()])
        .when_as(&admin().with_proof(Action::invite("cy@acme.io")), invite)
        .then_refused_as(Code::BadProof, "not for \"invite bob@acme.io\"");
}

/// An approval pauses the whole riff, needs an admin, and with
/// `required` the proof of its tag.
#[test]
fn an_approval_pauses_the_whole_riff_with_the_proof_of_its_tag() {
    let approve = |tag: &str| {
        Whole(Approve {
            me: person(),
            repo: repo(),
            tag: tag.into(),
        })
    };
    let of = |tag: &str| Action::Approve {
        repo: repo(),
        tag: tag.into(),
    };
    let running = [required(), riff_set(RiffState::Running)];
    given(&running)
        .when_as(&admin(), approve("v1.2.0"))
        .then_refused_as(Code::NeedsProof, "approve the deploy of v1.2.0 of acme/app");
    given(&running)
        .when_as(&admin().with_proof(of("v1.1.0")), approve("v1.2.0"))
        .then_refused_as(Code::BadProof, "v1.1.0");
    given(&running)
        .when_as(&admin().with_proof(of("v1.2.0")), approve("v1.2.0"))
        .then(&[riff_set(RiffState::Paused)]);
    given(&[riff_set(RiffState::Running)])
        .when_as(&Caller::of(&person()), approve("v1.2.0"))
        .then_refused_as(Code::NotAllowed, "approves the deploy of a release");
    given(&[riff_set(RiffState::Running)])
        .when_as(&admin(), approve(" "))
        .then_refused_as(Code::BadRequest, "names the tag");
}

/// A proof writes the record `proved` for a key of the person. A key
/// that the person does not have is refused, and so is a counter that
/// did not grow. A key that counts nothing gives 0 each time.
#[test]
fn a_proof_needs_a_key_of_the_person_and_a_counter_that_grows() {
    let ann = Caller::of(&person());
    given(&[key_added("yubikey")])
        .when_as(&ann, prove("yubikey", 1))
        .then(&[proved("yubikey", 1)]);
    given(&[key_added("yubikey")])
        .when_as(&ann, prove("phone", 1))
        .then_refused_as(Code::BadProof, "you have no key with the name phone");
    given(&[key_added("yubikey"), proved("yubikey", 5)])
        .when_as(&ann, prove("yubikey", 5))
        .then_refused_as(Code::BadProof, "did not grow");
    given(&[key_added("yubikey"), proved("yubikey", 5)])
        .when_as(&ann, prove("yubikey", 0))
        .then_refused_as(Code::BadProof, "did not grow");
    given(&[key_added("phone")])
        .when_as(&ann, prove("phone", 0))
        .then(&[proved("phone", 0)]);
    let state = given(&[key_added("yubikey")]).apply(&[proved("yubikey", 7)]);
    let key = state.written.people().device_of("ann", "yubikey").unwrap();
    assert_eq!(key.counter, 7);
}

/// With `required`, the removal of a device needs a proof.
#[test]
fn with_required_the_removal_of_a_device_needs_a_proof() {
    let remove = || {
        Whole(RemoveDevice {
            name: "yubikey".into(),
        })
    };
    let of = Action::RemoveDevice {
        name: "yubikey".into(),
    };
    let given_key = [required(), key_added("yubikey")];
    given(&given_key)
        .when_as(&Caller::of(&person()), remove())
        .then_refused_as(Code::NeedsProof, "remove the device yubikey");
    given(&given_key)
        .when_as(&Caller::of(&person()).with_proof(of), remove())
        .then(&[Change::DeviceRemoved(riff_core::record::DeviceRemoved {
            user: "ann".into(),
            name: "yubikey".into(),
        })]);
}

/// With `required`, a post that asks the daemon of a host for a step
/// that can lose work needs a proof of that step on that host, in a
/// direct thread and in the thread of the repository. A step that
/// cannot lose work needs none.
#[test]
fn with_required_a_step_on_another_host_needs_a_proof() {
    let daemon: SessionUri = "riff://ann@kite/acme/app?session=d1".parse().unwrap();
    let post = |thread: Option<ThreadName>, body: &str| {
        Whole(Post::new(
            &person(),
            thread,
            vec![Selector::session("d1")],
            body,
        ))
    };
    let end = Action::End {
        host: "kite".into(),
        name: "riff-issue-7-1a2b3c4d".into(),
    };
    for thread in [None, Some(repo())] {
        given(&[required()])
            .live(std::slice::from_ref(&daemon))
            .when_as(
                &Caller::of(&person()),
                post(thread.clone(), "daemon end riff-issue-7-1a2b3c4d"),
            )
            .then_refused_as(
                Code::NeedsProof,
                "end the session riff-issue-7-1a2b3c4d on the host kite needs a proof",
            );
        let elsewhere = Action::End {
            host: "heron".into(),
            name: "riff-issue-7-1a2b3c4d".into(),
        };
        given(&[required()])
            .live(std::slice::from_ref(&daemon))
            .when_as(
                &Caller::of(&person()).with_proof(elsewhere),
                post(thread.clone(), "daemon end riff-issue-7-1a2b3c4d"),
            )
            .then_refused_as(Code::BadProof, "on the host heron");
        let changes = given(&[required()])
            .live(std::slice::from_ref(&daemon))
            .when_as(
                &Caller::of(&person()).with_proof(end.clone()),
                post(thread.clone(), "daemon end riff-issue-7-1a2b3c4d"),
            );
        assert!(changes.0.is_ok(), "{:?}", changes.0);
        let status = given(&[required()])
            .live(std::slice::from_ref(&daemon))
            .when_as(&Caller::of(&person()), post(thread, "daemon status"));
        assert!(status.0.is_ok(), "{:?}", status.0);
    }
}
