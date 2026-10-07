//! The rules of the holds (01M43GSGB9ZFHSG0Q83Y50FEGW) and of the plan
//! (01M4A4YTNSJR0R1T9JNXPBSKHC): one test for each rule of
//! [`plan`](crate::state::plan), in the form given, when, then.

use riff_core::record::{ItemFreed, ItemHeld, Plan, PlanEnded, PlanItem, PlanSet, Wave};

use super::*;
use crate::state::plan::PLAN_TTL;

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
    assert_eq!(made[0].envelope.by, Some(By::Session(ann().who().clone())));

    // The worker ann2 is refused.
    let worker = Caller::of(&ann2());
    let claim_12 = claim("issue-12").of(&ann2());
    let refused = state.check(&worker, &claim_12, now).result.unwrap_err();
    assert_eq!(refused.code, Code::OnHold);
    let since = crate::tools::utc(made[0].envelope.written_at_ms);
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

/// The plan of `items`, each with no need, in the repository thread.
struct SendPlan {
    base: Option<u64>,
    items: &'static [&'static str],
}

impl Ask for SendPlan {
    type Command = wire::SetPlan;
    fn of(self, me: &SessionUri) -> wire::SetPlan {
        wire::SetPlan {
            me: me.clone(),
            base: self.base,
            plan: PlanSet {
                thread: repo(),
                plan: plan_of(self.items),
            },
        }
    }
}

struct Off;

impl Ask for Off {
    type Command = wire::PlanOff;
    fn of(self, me: &SessionUri) -> wire::PlanOff {
        wire::PlanOff {
            me: me.clone(),
            thread: repo(),
        }
    }
}

fn plan_of(items: &[&str]) -> Plan {
    Plan {
        wave: Some(Wave {
            number: 21,
            title: "Wave 21".into(),
        }),
        items: items
            .iter()
            .map(|item| PlanItem {
                item: (*item).into(),
                needs: vec![],
            })
            .collect(),
        done: vec![],
    }
}

fn plan_set(items: &[&str]) -> Change {
    Change::PlanSet(PlanSet {
        thread: repo(),
        plan: plan_of(items),
    })
}

/// The records of the team and a plan of `items`, and the position of
/// the `plan_set` in the records of [`given`].
fn planned(items: &[&str]) -> (Vec<Change>, u64) {
    let records = [team(), vec![plan_set(items)]].concat();
    let position = u64::try_from(records.len()).unwrap();
    (records, position)
}

/// A session of another repository.
fn cy() -> SessionUri {
    "riff://cy@wren/acme/lib?session=c1".parse().unwrap()
}

/// The case of the issue (01M4A4YTR2NKVBPE6BT9EC3X75): two `plan`
/// commands with the same `base` cross. The first is taken. The second
/// gets `stale_base`, and its old plan does not replace the new one.
#[test]
fn two_plans_that_cross_give_the_second_stale_base() {
    let Given { mut state, now } = given(&team()).live(&[ann(), bob()]);
    let first = SendPlan {
        base: None,
        items: &["issue-12", "issue-13"],
    }
    .of(&ann());
    let second = SendPlan {
        base: None,
        items: &["issue-12"],
    }
    .of(&bob());
    let (made, ()) = state.run(&Caller::of(&ann()), &first, now).unwrap();
    let refused = state.run(&Caller::of(&bob()), &second, now).unwrap_err();
    assert_eq!(refused.code, Code::StaleBase);
    let position = made[0].envelope.position;
    let part = format!("at position {position}, and the base is no plan");
    assert!(refused.reason.contains(&part), "{refused}");
    state.written(&made);
    let kept = state.plans().plan(&repo()).unwrap();
    assert_eq!(kept.plan, plan_of(&["issue-12", "issue-13"]));
    assert_eq!(kept.position, position);

    // With the base of the server, the plan of bob is taken.
    let again = SendPlan {
        base: Some(position),
        items: &["issue-12"],
    }
    .of(&bob());
    let (made, ()) = state.run(&Caller::of(&bob()), &again, now).unwrap();
    assert_eq!(made[0].change, plan_set(&["issue-12"]));
}

/// A `plan` equal to the plan of the server makes no record. With no
/// plan on the server, a base is stale; with a plan, no base is stale.
#[test]
fn the_same_plan_makes_no_record() {
    let (records, position) = planned(&["issue-12"]);
    let member = Caller::of(&ann());
    let same = || SendPlan {
        base: Some(position),
        items: &["issue-12"],
    };
    given(&records).when_as(&member, same()).then(&[]);
    given(&team())
        .when_as(&member, same())
        .then_refused_as(Code::StaleBase, "has no plan on the server");
    let no_base = SendPlan {
        base: None,
        items: &["issue-12"],
    };
    given(&records)
        .when_as(&member, no_base)
        .then_refused_as(Code::StaleBase, "and the base is no plan");
}

/// `plan` checks the form (01M4A4YTTB24XNB4G49675QMHT).
#[test]
fn a_plan_needs_items_of_the_form_issue_n_once_each() {
    let member = Caller::of(&ann());
    let send = |thread: ThreadName, plan: Plan| wire::SetPlan {
        me: ann(),
        base: None,
        plan: PlanSet { thread, plan },
    };
    let twice = plan_of(&["issue-12", "issue-12"]);
    let mut bad_need = plan_of(&["issue-12"]);
    bad_need.items[0].needs = vec!["verify-issue-3".into()];
    let mut bad_done = plan_of(&["issue-12"]);
    bad_done.done = vec!["#3".into()];
    for (thread, plan, part) in [
        (repo(), twice, "issue-12 is twice"),
        (repo(), plan_of(&["wave5-live"]), "wave5-live is not a name"),
        (repo(), bad_need, "verify-issue-3 is not a name"),
        (repo(), bad_done, "#3 is not a name"),
        (design(), plan_of(&["issue-12"]), "not a repository thread"),
    ] {
        let Given { mut state, now } = given(&team()).live(&[ann()]);
        let command = send(thread, plan);
        let refused = state.check(&member, &command, now).result.unwrap_err();
        assert_eq!(refused.code, Code::BadRequest, "{refused}");
        assert!(refused.reason.contains(part), "{refused}");
    }
}

/// The case of the issue (01M4A4YTWK68JDA0DKXX2HV4FA): a worker gets
/// `not_allowed` for `plan` and `plan_off`, also with the role of the
/// owner. A session out of the thread and a person who is a member
/// get it too. The owner and an admin can.
#[test]
fn a_worker_cannot_send_a_plan() {
    let (records, _) = planned(&["issue-12"]);
    let records = [records, worker_records()].concat();
    for role in Role::ALL {
        let worker = Caller::of(&ann2()).with_role(role);
        let send = SendPlan {
            base: None,
            items: &["issue-12"],
        };
        given(&worker_records())
            .when_as(&worker, send)
            .then_refused_as(Code::NotAllowed, "a worker cannot send plan");
        given(&records)
            .when_as(&worker, Off)
            .then_refused_as(Code::NotAllowed, "a worker cannot send plan_off");
    }
    given(&records)
        .when_as(&Caller::of(&cy()), Off)
        .then_refused_as(Code::NotAllowed, "only a session in acme/app");
    given(&records)
        .when_as(&Caller::of(&person()), Off)
        .then_refused_as(Code::NotAllowed, "the owner or an admin");
    let ended = Change::PlanEnded(PlanEnded { thread: repo() });
    given(&records)
        .when_as(&Caller::of(&person()).with_role(Role::Owner), Off)
        .then(std::slice::from_ref(&ended));
    given(&records)
        .when_as(&Caller::of(&cy()).with_role(Role::Admin), Off)
        .then(&[ended]);
}

/// The case of the issue (01M4A4YTNSJR0R1T9JNXPBSKHC): `plan_off`
/// removes the plan and keeps the holds. A `plan_off` with no plan
/// makes no record.
#[test]
fn plan_off_removes_the_plan_and_keeps_the_holds() {
    let (records, _) = planned(&["issue-12"]);
    let records = [records, vec![item_held("issue-13", REASON)]].concat();
    let Given { mut state, now } = given(&records).live(&[ann()]);
    let member = Caller::of(&ann());
    let (made, ()) = state.run(&member, &Off.of(&ann()), now).unwrap();
    assert_eq!(made.len(), 1);
    state.written(&made);
    assert!(state.plans().plan(&repo()).is_none());
    let held = state.plans().hold(&repo(), "issue-13").unwrap();
    assert_eq!(held.reason, REASON);
    let (made, ()) = state.run(&member, &Off.of(&ann()), now).unwrap();
    assert!(made.is_empty());

    let reply = state.plan(&repo(), now, 0);
    assert_eq!(reply.plan, None);
    assert_eq!(reply.holds.len(), 1);
}

/// The case of the issue (01M4A4Z1QTHYXZDMCP9DZ39WVT), with a fake
/// clock: the plan is stale `PLAN_TTL` after the last `plan` or
/// `plan_seen`, and not stale after a new `plan_seen`.
#[test]
fn a_plan_is_stale_plan_ttl_after_the_last_look() {
    let Given { mut state, now } = given(&team()).live(&[ann(), bob()]);
    let send = SendPlan {
        base: None,
        items: &["issue-12"],
    }
    .of(&ann());
    let (made, ()) = state.run(&Caller::of(&ann()), &send, now).unwrap();
    state.written(&made);
    let position = made[0].envelope.position;
    let stale = |state: &State, at: Instant| state.plan(&repo(), at, 0).plan.unwrap().stale;
    let tick = Duration::from_secs(1);
    assert!(!stale(&state, now + PLAN_TTL - tick));
    assert!(stale(&state, now + PLAN_TTL));

    // A plan_seen of bob at the position of the plan is a new look.
    let seen = now + PLAN_TTL;
    assert!(state.sees_plan(bob().who(), &repo(), position));
    state.signal(bob().who(), Signal::PlanSeen { thread: repo() }, seen);
    assert!(!stale(&state, seen + PLAN_TTL - tick));
    assert!(stale(&state, seen + PLAN_TTL));

    // A plan that is the same as the plan of the server is a look too,
    // with no record.
    let same = SendPlan {
        base: Some(position),
        items: &["issue-12"],
    }
    .of(&ann());
    let later = seen + PLAN_TTL;
    let (made, ()) = state.run(&Caller::of(&ann()), &same, later).unwrap();
    assert!(made.is_empty());
    assert!(!stale(&state, later + PLAN_TTL - tick));
    assert!(stale(&state, later + PLAN_TTL));
}

/// `plan_seen` counts only from a session in the thread that is not a
/// worker, and only for the position of the plan of the server
/// (01M4A4YTYVHFGK0CJVACJQ8DQ3).
#[test]
fn plan_seen_counts_only_for_the_plan_of_the_server() {
    let (records, position) = planned(&["issue-12"]);
    let records = [records, worker_records()].concat();
    let Given { state, now: _ } = given(&records);
    assert!(state.sees_plan(ann().who(), &repo(), position));
    assert!(!state.sees_plan(ann().who(), &repo(), position - 1));
    assert!(!state.sees_plan(ann2().who(), &repo(), position));
    assert!(!state.sees_plan(person().who(), &repo(), position));
    assert!(!state.sees_plan(cy().who(), &repo(), position));
}

/// The query (01M4A4Z1NKPDBXV2PRZCG86G6A): the plan, the position and
/// the time of its record, the time of the last look, the holder of
/// each item of the plan with a claim, and each hold of the thread.
#[test]
fn the_query_gives_the_plan_its_holders_and_the_holds() {
    let (records, position) = planned(&["issue-12", "issue-13"]);
    let more = vec![
        claimed(&bob(), "issue-13"),
        claimed(&bob(), "issue-99"),
        item_held("issue-14", REASON),
    ];
    let records = [records, more].concat();
    let Given { mut state, now } = given(&records).live(&[bob()]);
    let reply = state.plan(&repo(), now, 5_000);
    let shown = reply.plan.unwrap();
    assert_eq!(shown.plan, plan_of(&["issue-12", "issue-13"]));
    assert_eq!(shown.position, position);
    assert_eq!(shown.set_ms, 0);
    // No look since the start: the count starts at the start.
    assert_eq!(shown.seen_ms, None);
    assert!(!shown.stale);
    let holders: Vec<&str> = shown.holders.keys().map(String::as_str).collect();
    assert_eq!(holders, ["issue-13"]);
    assert_eq!(shown.holders["issue-13"].who(), bob().who());
    assert_eq!(reply.holds["issue-14"].reason, REASON);

    let look = now + Duration::from_secs(2);
    state.signal(ann().who(), Signal::PlanSeen { thread: repo() }, look);
    let later = look + Duration::from_secs(3);
    let shown = state.plan(&repo(), later, 9_000).plan.unwrap();
    assert_eq!(shown.seen_ms, Some(6_000));
    // A thread with no plan and no hold gives an empty reply.
    let lib = "acme/lib".parse().unwrap();
    assert_eq!(state.plan(&lib, now, 0), wire::PlanReply::default());
}
