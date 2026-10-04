//! The rules of the holds (01M43GSGB9ZFHSG0Q83Y50FEGW): one test for
//! each rule of [`plan`](crate::state::plan), in the form given, when,
//! then.

use riff_core::record::{ItemFreed, ItemHeld};

use super::*;

struct Hold {
    item: &'static str,
    reason: String,
}

impl Ask for Hold {
    type Command = wire::Hold;
    fn of(self, me: &SessionUri) -> wire::Hold {
        wire::Hold {
            me: me.clone(),
            thread: repo(),
            item: self.item.into(),
            reason: self.reason,
        }
    }
}

struct Free(&'static str);

impl Ask for Free {
    type Command = wire::Free;
    fn of(self, me: &SessionUri) -> wire::Free {
        wire::Free {
            me: me.clone(),
            thread: repo(),
            item: self.0.into(),
        }
    }
}

const REASON: &str = "waits for the word of Mike";

fn hold(item: &'static str) -> Hold {
    Hold {
        item,
        reason: REASON.into(),
    }
}

fn item_held(item: &str, reason: &str) -> Change {
    Change::ItemHeld(ItemHeld {
        thread: repo(),
        item: item.into(),
        reason: reason.into(),
    })
}

fn item_freed(item: &str) -> Change {
    Change::ItemFreed(ItemFreed {
        thread: repo(),
        item: item.into(),
    })
}

/// carol: a member in the repository thread, not the lead of her user.
fn carol() -> SessionUri {
    "riff://carol@wren/acme/app?session=c1".parse().unwrap()
}

/// A lead holds an item with a reason. A second hold replaces the
/// reason. The same reason again changes nothing. A free ends the hold,
/// and a free of an item with no hold changes nothing.
#[test]
fn a_lead_holds_an_item_and_frees_it() {
    let member = Caller::of(&ann());
    given(&team())
        .when_as(&member, hold("issue-12"))
        .then(&[item_held("issue-12", REASON)]);
    let held = [team(), vec![item_held("issue-12", REASON)]].concat();
    given(&held).when_as(&member, hold("issue-12")).then(&[]);
    let other = Hold {
        item: "issue-12",
        reason: "the design is not done".into(),
    };
    given(&held)
        .when_as(&member, other)
        .then(&[item_held("issue-12", "the design is not done")]);
    given(&held)
        .when_as(&member, Free("issue-12"))
        .then(&[item_freed("issue-12")]);
    given(&team()).when_as(&member, Free("issue-12")).then(&[]);
}

/// A hold needs a reason of 1 to 200 characters, and an item with a
/// valid name.
#[test]
fn a_hold_needs_a_reason_of_1_to_200_characters() {
    let member = Caller::of(&ann());
    let with = |reason: String| Hold {
        item: "issue-12",
        reason,
    };
    given(&team())
        .when_as(&member, with("  ".into()))
        .then_refused_as(Code::BadRequest, "1 to 200 characters, not 0");
    given(&team())
        .when_as(&member, with("é".repeat(201)))
        .then_refused_as(Code::BadRequest, "not 201");
    given(&team())
        .when_as(&member, with("é".repeat(200)))
        .then(&[item_held("issue-12", &"é".repeat(200))]);
    let bad = Hold {
        item: "issue 12",
        reason: REASON.into(),
    };
    given(&team())
        .when_as(&member, bad)
        .then_refused_as(Code::BadRequest, "not allowed");
}

/// A worker, and a session that is not a lead, get `not_allowed` for
/// `hold` and `free` (01M43GSGGY0QMB5D5EH92M6ZFP). The owner and an
/// admin can, also as a person.
#[test]
fn a_worker_and_a_session_that_is_not_a_lead_cannot_hold_or_free() {
    let held = [worker_records(), vec![item_held("issue-12", REASON)]].concat();
    // ann2 is a worker, also with the role of an admin.
    for role in Role::ALL {
        let worker = Caller::of(&ann2()).with_role(role);
        given(&worker_records())
            .when_as(&worker, hold("issue-12"))
            .then_refused_as(Code::NotAllowed, "a worker cannot hold an item");
        given(&held)
            .when_as(&worker, Free("issue-12"))
            .then_refused_as(Code::NotAllowed, "a worker cannot free an item");
    }
    // carol is in the thread, and not a lead.
    let records = [held.clone(), vec![joined(&carol(), &repo())]].concat();
    let carol = Caller::of(&carol());
    given(&records)
        .when_as(&carol, hold("issue-7"))
        .then_refused_as(Code::NotAllowed, "only a lead of acme/app");
    given(&records)
        .when_as(&carol, Free("issue-12"))
        .then_refused_as(Code::NotAllowed, "only a lead of acme/app");
    // A person who is a member is no lead.
    let person = Caller::of(&person());
    given(&held)
        .when_as(&person, Free("issue-12"))
        .then_refused_as(Code::NotAllowed, "the owner or an admin");
    // An admin can, as a session and as a person.
    given(&records)
        .when_as(&carol.with_role(Role::Admin), Free("issue-12"))
        .then(&[item_freed("issue-12")]);
    given(&held)
        .when_as(&person.with_role(Role::Owner), Free("issue-12"))
        .then(&[item_freed("issue-12")]);
}

/// A hold names one item by its exact name, and needs a repository
/// thread.
#[test]
fn a_hold_names_one_item_of_a_repository_thread() {
    let member = Caller::of(&ann());
    let records = [team(), vec![item_held("issue-12", REASON)]].concat();
    let Given { state, now: _ } = given(&records);
    assert!(state.plans().hold(&repo(), "issue-12").is_some());
    assert!(state.plans().hold(&repo(), "verify-issue-12").is_none());
    assert!(state.plans().hold(&design(), "issue-12").is_none());

    let in_design = wire::Hold {
        me: ann(),
        thread: design(),
        item: "issue-12".into(),
        reason: REASON.into(),
    };
    let Given { mut state, now } = given(&team()).live(&[ann()]);
    let refused = state.check(&member, &in_design, now).result.unwrap_err();
    assert_eq!(refused.code, Code::BadRequest);
    assert!(
        refused.reason.contains("not a repository thread"),
        "{refused}"
    );
}

/// The case of the issue (01M43GSGPJ69TPWPA4935WR8RW): a lead holds
/// `issue-12`. A claim of a worker gets `on_hold` with the lead, the
/// time and the reason. A claim of a session that is not a worker is
/// granted, with the warning. After `free`, the claim of the worker is
/// granted.
#[test]
fn a_held_item_refuses_a_worker_and_warns_each_other_session() {
    let Given { mut state, now } = worker_team().live(&[ann(), ann2(), bob()]);
    let lead = Caller::of(&ann());
    let hold = hold("issue-12").of(&ann());
    let (made, ()) = state.run(&lead, &hold, now).unwrap();
    assert_eq!(made[0].by, Some(By::Session(ann().who().clone())));

    // The worker ann2 is refused.
    let worker = Caller::of(&ann2());
    let claim_12 = claim("issue-12").of(&ann2());
    let refused = state.check(&worker, &claim_12, now).result.unwrap_err();
    assert_eq!(refused.code, Code::OnHold);
    let since = crate::tools::utc(made[0].written_at_ms);
    let expected = format!(
        "issue-12 is held by the lead (the session ann/a1) since {since}: {REASON}. Pick another \
         item."
    );
    assert_eq!(refused.reason, expected);
    // A verify of the item is not held.
    let verify = claim("verify-issue-12").of(&ann2());
    assert!(state.check(&worker, &verify, now).result.is_ok());

    // bob is no worker: the claim is granted, with the warning.
    let claim_bob = claim("issue-12").of(&bob());
    let (changes, warning) = state
        .check(&Caller::of(&bob()), &claim_bob, now)
        .result
        .unwrap();
    assert_eq!(changes, [claimed(&bob(), "issue-12")]);
    let warning = warning.unwrap();
    assert!(
        warning.starts_with("issue-12 is held by the lead (the session ann/a1)"),
        "{warning}"
    );
    assert!(warning.ends_with(REASON), "{warning}");

    // A hold does not end a claim: the claim of bob stays.
    let (made, _) = state.run(&Caller::of(&bob()), &claim_bob, now).unwrap();
    let again = wire::Hold {
        reason: "the design is not done".into(),
        ..hold.clone()
    };
    let (held, ()) = state.run(&lead, &again, now).unwrap();
    state.written(&[made, held].concat());
    let held_by = state.written_riff().work().holder(&repo(), "issue-12");
    assert_eq!(held_by, Some(bob().who()));
    let reason = &state.plans().hold(&repo(), "issue-12").unwrap().reason;
    assert_eq!(reason, "the design is not done");

    // After the free, and the release of bob, the worker gets the item.
    let free = Free("issue-12").of(&ann());
    state.run(&lead, &free, now).unwrap();
    let release = release("issue-12").of(&bob());
    state.run(&Caller::of(&bob()), &release, now).unwrap();
    let (changes, warning) = state.check(&worker, &claim_12, now).result.unwrap();
    assert_eq!(changes, [claimed(&ann2(), "issue-12")]);
    assert_eq!(warning, None);
}
