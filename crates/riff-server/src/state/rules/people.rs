//! The rules of the people (01M3XA875QZ584JBGA37853PWX): one test for
//! each rule of [`people`](crate::state::people), in the form given,
//! when, then. A person is `USER@gmail.com`. The role of a caller comes
//! from the people of the state, as in the engine.

use std::collections::BTreeSet;

use riff_core::record::{self, Email, OwnerSet, PersonJoined, SigninsEnded};

use super::*;

const ADA: &str = "ada@gmail.com";
const BOB: &str = "bob@gmail.com";
const CAROL: &str = "carol@gmail.com";
const DAN: &str = "dan@gmail.com";

fn person_joined(user: &str, email: &str) -> Change {
    Change::PersonJoined(PersonJoined {
        user: user.into(),
        email: email.into(),
    })
}

fn by_email(email: &str) -> Email {
    Email {
        email: email.into(),
    }
}

fn member_invited(email: &str) -> Change {
    Change::MemberInvited(by_email(email))
}

fn member_removed(email: &str) -> Change {
    Change::MemberRemoved(by_email(email))
}

fn admin_set(email: &str, admin: bool) -> Change {
    Change::AdminSet(record::AdminSet {
        email: email.into(),
        admin,
    })
}

fn owner_set(email: Option<&str>) -> Change {
    Change::OwnerSet(OwnerSet {
        email: email.map(str::to_owned),
    })
}

fn owner_asked(email: &str, due_ms: u64) -> Change {
    Change::OwnerAsked(record::OwnerAsked {
        email: email.into(),
        due_ms,
    })
}

fn owner_denied(email: &str) -> Change {
    Change::OwnerDenied(by_email(email))
}

fn signins_ended(user: &str) -> Change {
    Change::SigninsEnded(SigninsEnded { user: user.into() })
}

/// Ada is the owner. Bob is an admin that the owner made. Carol is a
/// member. Each of them signed in.
fn riff_of_ada() -> Vec<Change> {
    vec![
        person_joined("ada", ADA),
        owner_set(Some(ADA)),
        member_invited(BOB),
        admin_set(BOB, true),
        person_joined("bob", BOB),
        member_invited(CAROL),
        person_joined("carol", CAROL),
    ]
}

/// The records that keep ada, the old owner, an admin and a member.
fn ada_stays() -> [Change; 2] {
    [member_invited(ADA), admin_set(ADA, true)]
}

/// The time that the owner has to answer a request.
fn answer() -> Duration {
    crate::owner::Timing::default().answer
}

fn answer_ms() -> u64 {
    u64::try_from(answer().as_millis()).unwrap()
}

fn admit(email: &str, allowed_domain: bool) -> Admit {
    Admit {
        email: email.into(),
        allowed_domain,
    }
}

fn invite(email: &str) -> wire::Invite {
    wire::Invite {
        email: email.into(),
    }
}

fn remove(email: &str) -> wire::Remove {
    wire::Remove {
        email: email.into(),
    }
}

fn set_admin(email: &str, admin: bool) -> wire::SetAdmin {
    wire::SetAdmin {
        email: email.into(),
        admin,
    }
}

fn pass(email: &str) -> wire::PassOwner {
    wire::PassOwner {
        email: email.into(),
    }
}

fn revoke(user: Option<&str>) -> wire::Revoke {
    wire::Revoke {
        user: user.map(str::to_owned),
    }
}

fn server() -> SessionUri {
    crate::owner::server_uri()
}

impl Given {
    /// The same state with these admin emails of the settings (R210).
    fn with_admins(self, admins: &[&str]) -> Given {
        let admins: Vec<String> = admins.iter().map(|a| (*a).to_owned()).collect();
        let timing = crate::owner::Timing::default();
        let settings = Settings::new(&admins, "https://riff.test", timing);
        Given {
            state: self.state.with_settings(settings),
            now: self.now,
        }
    }

    /// The person `user` sends `ask`, with the role that the people
    /// give to `user`.
    fn when_person<A: Ask>(self, user: &str, ask: A) -> When {
        let me: SessionUri = format!("riff://{user}@heron").parse().unwrap();
        let caller = Caller::of(&me).with_role(self.state.role(user));
        self.when_as(&caller, ask)
    }

    /// The provider verified `email`: the sign-in sends `admit`.
    fn when_signs_in(self, email: &str, allowed_domain: bool) -> When {
        let user = crate::oidc::user_of(&crate::state::people::email(email)).unwrap();
        let caller = Caller::sign_in(&Who::new(&user, None).unwrap(), email);
        self.when_as(&caller, admit(email, allowed_domain))
    }
}

#[test]
fn the_first_person_that_signs_in_is_the_owner() {
    given(&[])
        .when_signs_in(ADA, false)
        .then(&[person_joined("ada", ADA), owner_set(Some(ADA))]);
}

#[test]
fn on_a_riff_with_admins_of_the_settings_only_an_admin_becomes_the_owner() {
    // A person of an allowed domain signs in, and is not the owner.
    given(&[])
        .with_admins(&[BOB])
        .when_signs_in(ADA, true)
        .then(&[person_joined("ada", ADA)]);
    // A person of no allowed domain is refused: the riff is not new.
    given(&[])
        .with_admins(&[BOB])
        .when_signs_in(ADA, false)
        .then_refused_as(Code::NotMember, "riff invite ada@gmail.com");
    // The admin of the settings is the owner.
    given(&[person_joined("ada", ADA)])
        .with_admins(&[" Bob@Gmail.com "])
        .when_signs_in(BOB, false)
        .then(&[person_joined("bob", BOB), owner_set(Some(BOB))]);
}

#[test]
fn a_person_who_is_no_member_signs_in_only_with_an_invite_or_an_allowed_domain() {
    given(&riff_of_ada())
        .when_signs_in(DAN, false)
        .then_refused_as(
            Code::NotMember,
            "dan@gmail.com is not a member of this riff; ask its owner to run: \
             riff invite dan@gmail.com",
        );
    given(&riff_of_ada())
        .when_signs_in(DAN, true)
        .then(&[person_joined("dan", DAN)]);
    let mut invited = riff_of_ada();
    invited.push(member_invited(DAN));
    given(&invited)
        .when_signs_in(DAN, false)
        .then(&[person_joined("dan", DAN)]);
}

/// The first email that signs in with a USER holds it (R209).
#[test]
fn another_email_does_not_get_the_user_of_the_first() {
    given(&riff_of_ada())
        .when_signs_in("ada@other.io", true)
        .then_refused_as(
            Code::NotAllowed,
            "the user ada belongs to another account; ask an admin",
        );
    // A person who may not join gets "not a member", and learns nothing
    // of the USER names (01M3XGP011KGXDP9D1FNMT374F).
    given(&riff_of_ada())
        .when_signs_in("ada@other.io", false)
        .then_refused_as(Code::NotMember, "riff invite ada@other.io");
    // `apply` keeps the first email too.
    let state = given(&riff_of_ada()).apply(&[person_joined("ada", "ada@other.io")]);
    assert_eq!(state.written.people().email_of("ada"), Some(ADA));
}

/// A second try of a sign-in makes no second record
/// (01M3XA877YZQ649SWB5TN60V5P).
#[test]
fn a_sign_in_of_a_known_person_makes_no_record() {
    given(&riff_of_ada()).when_signs_in(ADA, false).then(&[]);
    given(&riff_of_ada())
        .when_signs_in(" Carol@Gmail.com", false)
        .then(&[]);
}

#[test]
fn nobody_signs_in_as_the_riff_server() {
    given(&[])
        .when_signs_in("Riff@gmail.com", false)
        .then_refused_as(Code::BadRequest, "the riff server");
}

/// The sign-in keeps the position of the pending copy at its check
/// (01M3XA87A9GGFA89RQXWSKY0V6).
#[test]
fn a_sign_in_gets_the_position_of_the_log_at_its_check() {
    let Given { mut state, now } = given(&riff_of_ada());
    let caller = Caller::sign_in(&Who::new("carol", None).unwrap(), CAROL);
    let (made, admitted) = state.run(&caller, &admit(CAROL, false), now).unwrap();
    assert!(made.is_empty());
    assert_eq!(admitted.user, "carol");
    assert_eq!(admitted.position, 7);
    // A removal after the check has a later position.
    let state = Given { state, now }.apply(&[member_removed(CAROL)]);
    assert_eq!(state.signins_ended()["carol"], 8);
}

#[test]
fn only_an_admin_invites_and_removes() {
    given(&riff_of_ada())
        .when_person("bob", invite(" Dan@Gmail.com"))
        .then(&[member_invited(DAN)]);
    given(&riff_of_ada())
        .when_person("carol", invite(DAN))
        .then_refused_as(
            Code::NotAllowed,
            "carol is not an admin; only an admin can invite a person",
        );
    given(&riff_of_ada())
        .when_person("bob", remove(CAROL))
        .then(&[member_removed(CAROL)]);
    given(&riff_of_ada())
        .when_person("carol", remove(CAROL))
        .then_refused_as(
            Code::NotAllowed,
            "carol is not an admin; only an admin can remove a person",
        );
}

#[test]
fn an_invite_of_a_member_and_of_a_bad_email_makes_no_record() {
    given(&riff_of_ada())
        .when_person("ada", invite(CAROL))
        .then(&[]);
    given(&riff_of_ada())
        .when_person("ada", invite("not-an-email"))
        .then_refused_as(Code::BadRequest, "not an email");
}

#[test]
fn the_owner_and_an_admin_cannot_be_removed() {
    given(&riff_of_ada())
        .when_person("bob", remove(ADA))
        .then_refused_as(Code::BadRequest, "the owner stays");
    given(&riff_of_ada())
        .when_person("ada", remove(BOB))
        .then_refused_as(Code::BadRequest, "riff admin remove bob@gmail.com first");
    // A person that the riff does not know: no record.
    given(&riff_of_ada())
        .when_person("ada", remove("nobody@gmail.com"))
        .then(&[]);
}

/// A removal ends each sign-in of the person from before its record
/// (R20, 01M3XA87A9GGFA89RQXWSKY0V6): `apply` keeps the position.
#[test]
fn a_removal_and_a_revoke_keep_their_position_for_the_user() {
    let state = given(&riff_of_ada()).apply(&[member_removed(CAROL), signins_ended("bob")]);
    assert_eq!(state.signins_ended()["carol"], 8);
    assert_eq!(state.signins_ended()["bob"], 9);
    assert!(!state.signins_ended().contains_key("ada"));
    // The removed person is no member, and keeps the USER.
    assert!(state.people().members().members.is_empty());
    assert_eq!(state.written.people().email_of("carol"), Some(CAROL));
}

#[test]
fn only_the_owner_adds_and_removes_an_admin() {
    // A person who is no member is a member too, as before the people
    // moved to the log.
    given(&riff_of_ada())
        .when_person("ada", set_admin("Dan@gmail.com", true))
        .then(&[member_invited(DAN), admin_set(DAN, true)]);
    given(&riff_of_ada())
        .when_person("ada", set_admin(CAROL, true))
        .then(&[admin_set(CAROL, true)]);
    given(&riff_of_ada())
        .when_person("ada", set_admin(BOB, true))
        .then(&[]);
    given(&riff_of_ada())
        .when_person("ada", set_admin(BOB, false))
        .then(&[admin_set(BOB, false)]);
    given(&riff_of_ada())
        .when_person("bob", set_admin(CAROL, true))
        .then_refused_as(
            Code::NotAllowed,
            "bob is not the owner; only the owner adds or removes an admin",
        );
    given(&riff_of_ada())
        .when_person("ada", set_admin(ADA, false))
        .then_refused_as(Code::BadRequest, "the owner stays an admin");
    given(&riff_of_ada())
        .when_person("ada", set_admin(CAROL, false))
        .then_refused_as(Code::BadRequest, "is not an admin that the owner made");
}

/// The admins of the settings add to the admins that the owner made
/// (R210). They are not in the log.
#[test]
fn an_admin_of_the_settings_has_the_role_of_an_admin() {
    let given = given(&riff_of_ada()).with_admins(&["Carol@gmail.com"]);
    assert_eq!(given.state.role("ada"), Role::Owner);
    assert_eq!(given.state.role("bob"), Role::Admin);
    assert_eq!(given.state.role("carol"), Role::Admin);
    assert_eq!(given.state.role("dan"), Role::Member);
    let (owner, admins, members) = given.state.people().roles();
    assert_eq!(owner.as_deref(), Some(ADA));
    assert_eq!(admins, [BOB, CAROL]);
    assert!(members.is_empty());
    given
        .when_person("carol", invite(DAN))
        .then(&[member_invited(DAN)]);
}

/// The old owner stays an admin and a member: `handle` decides it, and
/// gives the records before `owner_set`.
#[test]
fn the_owner_passes_the_role_and_stays_an_admin() {
    let [invited, admin] = ada_stays();
    given(&riff_of_ada())
        .when_person("ada", pass(" Carol@gmail.com"))
        .then(&[invited.clone(), admin.clone(), owner_set(Some(CAROL))]);
    given(&riff_of_ada())
        .when_person("bob", pass(BOB))
        .then_refused_as(
            Code::NotAllowed,
            "bob is not the owner; only the owner passes the owner role",
        );
    given(&riff_of_ada())
        .when_person("ada", pass(DAN))
        .then_refused_as(
            Code::NotMember,
            "dan@gmail.com is not a member of this riff; run riff invite dan@gmail.com first",
        );
    given(&riff_of_ada())
        .when_person("ada", pass(ADA))
        .then_refused_as(Code::BadRequest, "is the owner of this riff already");
    // The role goes back: ada is a member and an admin already.
    let mut passed = riff_of_ada();
    passed.extend([invited, admin, owner_set(Some(CAROL))]);
    let state = given(&passed).apply(&[]);
    assert_eq!(state.role("carol"), Role::Owner);
    assert_eq!(state.role("ada"), Role::Admin);
    given(&passed)
        .when_person("carol", pass(ADA))
        .then(&[admin_set(CAROL, true), owner_set(Some(ADA))]);
}

#[test]
fn an_admin_asks_for_the_owner_role_and_waits() {
    given(&riff_of_ada())
        .when_person("bob", wire::TakeOwner {})
        .then(&[owner_asked(BOB, answer_ms())]);
    given(&riff_of_ada())
        .when_person("carol", wire::TakeOwner {})
        .then_refused_as(
            Code::NotAllowed,
            "carol is not an admin; only an admin can take the owner role",
        );
    // The owner is the owner already: no record.
    given(&riff_of_ada())
        .when_person("ada", wire::TakeOwner {})
        .then(&[]);
}

#[test]
fn a_second_request_waits_for_the_first() {
    let mut asked = riff_of_ada();
    asked.extend([admin_set(CAROL, true), owner_asked(BOB, 9)]);
    given(&asked)
        .when_person("carol", wire::TakeOwner {})
        .then_refused_as(
            Code::BadRequest,
            "bob@gmail.com asked for the owner role first; wait for the answer of the owner",
        );
    // The owner still gets "already", and the request still waits.
    given(&asked)
        .when_person("ada", wire::TakeOwner {})
        .then(&[]);
}

#[test]
fn the_owner_denies_only_a_request_that_waits() {
    let mut asked = riff_of_ada();
    asked.push(owner_asked(BOB, 9));
    given(&asked)
        .when_person("ada", wire::DenyOwner {})
        .then(&[owner_denied(BOB)]);
    given(&asked)
        .when_person("bob", wire::DenyOwner {})
        .then_refused_as(
            Code::NotAllowed,
            "bob is not the owner; only the owner denies the owner role",
        );
    given(&riff_of_ada())
        .when_person("ada", wire::DenyOwner {})
        .then_refused_as(Code::BadRequest, "no admin asks for the owner role now");
    let state = given(&asked).apply(&[owner_denied(BOB)]);
    assert_eq!(state.asks(), None);
}

/// A pass ends the request that waits: the `owner_set` record ends it.
#[test]
fn a_pass_ends_the_request_that_waits() {
    let mut asked = riff_of_ada();
    asked.push(owner_asked(BOB, 9));
    let state = given(&asked).apply(&[owner_set(Some(CAROL))]);
    assert_eq!(state.asks(), None);
}

#[test]
fn with_no_answer_in_time_the_admin_that_asked_is_the_owner() {
    let mut asked = riff_of_ada();
    asked.push(owner_asked(BOB, answer_ms()));
    let [invited, admin] = ada_stays();
    // Before the time ends, the server changes nothing.
    given(&asked).when(&server(), GrantOwner).then(&[]);
    given(&riff_of_ada())
        .after(Duration::from_secs(3600))
        .when(&server(), GrantOwner)
        .then(&[]);
    given(&asked)
        .after(answer())
        .when(&server(), GrantOwner)
        .then(&[invited, admin, owner_set(Some(BOB))]);
}

#[test]
fn an_owner_who_is_gone_leaves_the_role_to_the_admin_that_asked_or_to_nobody() {
    let mut asked = riff_of_ada();
    asked.push(owner_asked(BOB, 9));
    let [invited, admin] = ada_stays();
    given(&asked).when(&server(), EndOwner).then(&[
        invited.clone(),
        admin.clone(),
        owner_set(Some(BOB)),
    ]);
    given(&riff_of_ada())
        .when(&server(), EndOwner)
        .then(&[invited, admin, owner_set(None)]);
    // A riff with no owner: nothing to end.
    given(&[]).when(&server(), EndOwner).then(&[]);
}

/// A riff whose owner is gone has no owner until an admin takes the
/// role (01M3Q63NNC6SC03BFCG80M7B4D).
#[test]
fn a_riff_with_no_owner_gets_one_only_from_an_admin_that_asks() {
    let mut gone = riff_of_ada();
    gone.extend(ada_stays());
    gone.push(owner_set(None));
    let no_owner = "the riff has no owner; an admin takes the owner role with: riff owner --take";
    // A sign-in and the setting make no owner.
    given(&gone)
        .when_signs_in(DAN, true)
        .then(&[person_joined("dan", DAN)]);
    let name = NameOwner { email: ADA.into() };
    given(&gone).when(&server(), name).then(&[]);
    // Each action of the owner names `riff owner --take`.
    given(&gone)
        .when_person("ada", set_admin(CAROL, true))
        .then_refused_as(Code::NotAllowed, no_owner);
    given(&gone)
        .when_person("ada", pass(BOB))
        .then_refused_as(Code::NotAllowed, no_owner);
    given(&gone)
        .when_person("ada", wire::DenyOwner {})
        .then_refused_as(Code::NotAllowed, no_owner);
    // The first admin that asks is the owner at once.
    given(&gone)
        .when_person("bob", wire::TakeOwner {})
        .then(&[owner_set(Some(BOB))]);
    given(&gone)
        .when_person("carol", wire::TakeOwner {})
        .then_refused_as(Code::NotAllowed, "carol is not an admin");
}

/// The setting `--owner` names the owner only of a riff that has none
/// and had none (01M3JN3ASSV9SA0QZKXXJ0RTEV).
#[test]
fn the_setting_names_the_owner_of_a_new_riff() {
    let name = || NameOwner {
        email: " Ada@Gmail.com".into(),
    };
    given(&[])
        .when(&server(), name())
        .then(&[owner_set(Some(ADA))]);
    given(&[owner_set(Some(BOB))])
        .when(&server(), name())
        .then(&[]);
    // The first other person is then not the owner.
    given(&[owner_set(Some(ADA))])
        .when_signs_in(BOB, false)
        .then_refused_as(Code::NotMember, "riff invite bob@gmail.com");
    given(&[owner_set(Some(ADA))])
        .when_signs_in(ADA, false)
        .then(&[person_joined("ada", ADA)]);
}

/// A person ends the own sign-ins. Only an admin ends the sign-ins of
/// another person (R20).
#[test]
fn a_person_revokes_the_own_sign_ins_and_an_admin_those_of_another_person() {
    given(&riff_of_ada())
        .when_person("carol", revoke(None))
        .then(&[signins_ended("carol")]);
    given(&riff_of_ada())
        .when_person("carol", revoke(Some(" Carol ")))
        .then(&[signins_ended("carol")]);
    given(&riff_of_ada())
        .when_person("carol", revoke(Some("bob")))
        .then_refused_as(
            Code::NotAllowed,
            "carol is not an admin; only an admin revokes another person",
        );
    given(&riff_of_ada())
        .when_person("bob", revoke(Some("Carol")))
        .then(&[signins_ended("carol")]);
}

/// Only a person sends a command of the people, only the sign-in sends
/// `admit`, and only the server sends its commands
/// (01M3WRD959DYNZHDKP5ZT9Q1C7).
#[test]
fn a_session_cannot_send_a_command_of_the_people() {
    given(&team())
        .when(&ann(), invite(BOB))
        .then_refused_as(Code::NotAllowed, "a session cannot send the command invite");
    given(&team())
        .when(&person(), admit(ADA, true))
        .then_refused_as(Code::NotAllowed, "a person cannot send the command admit");
    given(&team()).when(&person(), EndOwner).then_refused_as(
        Code::NotAllowed,
        "a person cannot send the command end_owner",
    );
}

/// A command of the people names no `me`, so it changes no presence:
/// the person of the token gets no entry and no place, and `who` shows
/// no row for it.
#[test]
fn a_command_of_the_people_changes_no_presence() {
    let Given { mut state, now } = given(&riff_of_ada());
    let me: SessionUri = "riff://ada@server".parse().unwrap();
    let caller = Caller::of(&me).with_role(state.role("ada"));
    let check = state.check(&caller, &invite(DAN), now);
    assert!(check.registered.is_none());
    assert_eq!(check.result.unwrap().0, [member_invited(DAN)]);
    assert!(!state.knows(me.who()));
    assert!(state.who(now, 0, true).is_empty());
}

/// The role of a caller comes from the pending copy
/// (01M3XA87F70CD3WH4STADSCW6S): a command that comes after a change of
/// a role in the queue gets the new role, before the write.
#[test]
fn a_change_of_a_role_counts_for_the_next_command_in_the_queue() {
    let now = Instant::now();
    let mut state = State::with_writer(now, 0);
    let cause = Cause::of(&Caller::server(), CommandKind::Forget);
    let records = state.queue(&cause, &riff_of_ada(), now);
    state.written(&records);
    assert_eq!(state.role("bob"), Role::Admin);
    // The owner makes bob a member again. The record waits in the queue.
    state.queue(&cause, &[admin_set(BOB, false)], now);
    assert_eq!(state.role("bob"), Role::Member, "the pending copy");
    // A query still reads the written copy.
    assert_eq!(state.people().role_of("bob"), Role::Admin);
}

/// The note of a change of the owner role goes in the chunk of its
/// command (01M3N7K4DVHSF7AQ402F14J26Z): one note in the thread of each
/// repository, which wakes no session, and a direct message to each
/// live lead of the person who must act.
#[test]
fn the_note_of_a_request_is_in_the_chunk_of_the_command() {
    let mut riff = team();
    riff.extend([
        person_joined("ann", "ann@acme.io"),
        owner_set(Some("ann@acme.io")),
        member_invited("bob@acme.io"),
        admin_set("bob@acme.io", true),
        person_joined("bob", "bob@acme.io"),
    ]);
    let given = given(&riff).live(&[ann(), bob()]);
    let bob_person: SessionUri = "riff://bob@kite".parse().unwrap();
    let caller = Caller::of(&bob_person).with_role(given.state.role("bob"));
    let When(Ok(changes)) = given.when_as(&caller, wire::TakeOwner {}) else {
        panic!("refused");
    };
    assert_eq!(changes[0], owner_asked("bob@acme.io", answer_ms()));
    let posts: Vec<&Posted> = changes
        .iter()
        .filter_map(|change| match change {
            Change::Posted(posted) => Some(&**posted),
            _ => None,
        })
        .collect();
    let [note, told] = &posts[..] else {
        panic!("{changes:?}");
    };
    assert_eq!(note.thread, repo());
    assert_eq!(note.message.kind, Kind::Note);
    assert!(note.woken.is_empty());
    assert_eq!(note.message.from, server());
    let body = &note.message.body;
    assert!(
        body.starts_with("members: bob asks for the owner role."),
        "{body}"
    );
    // The owner must answer: the lead of ann gets a direct message.
    assert!(told.thread.is_direct());
    assert_eq!(told.message.kind, Kind::Message);
    assert_eq!(told.woken, BTreeSet::from([ann().who().clone()]));
    assert!(told.message.body.ends_with("Show this to your user."));
}

/// A start from a checkpoint gives the people of a full replay, for a
/// checkpoint at each position.
#[test]
fn a_start_from_a_checkpoint_gives_the_people_of_a_full_replay() {
    let mut changes = vec![riff_made("r1"), riff_set(RiffState::Paused)];
    changes.extend(riff_of_ada());
    changes.extend([
        owner_asked(BOB, 9),
        member_removed(CAROL),
        signins_ended("bob"),
        admin_set(CAROL, true),
    ]);
    let records: Vec<Record> = changes
        .iter()
        .enumerate()
        .map(|(n, change)| Record {
            position: u64::try_from(n).unwrap() + 1,
            written_at_ms: 0,
            by: None,
            command: None,
            call: None,
            change: change.clone(),
        })
        .collect();
    let now = Instant::now();
    let full = State::replay(records.clone(), now, 0);
    assert_eq!(full.riff_id().as_deref(), Some("r1"));
    assert_eq!(full.asks().as_deref(), Some(BOB));
    for at in 0..=records.len() {
        let snapshot = State::replay(records[..at].to_vec(), now, 0).snapshot(now, 0);
        // The checkpoint is JSON in the store.
        let json = serde_json::to_vec(&snapshot).unwrap();
        let snapshot: Snapshot = serde_json::from_slice(&json).unwrap();
        let loaded = State::load(Some(snapshot), records[at..].to_vec(), now, 0);
        assert!(loaded.same_log_state(&full), "a checkpoint at {at}");
        assert_eq!(loaded.signins_ended(), full.signins_ended(), "at {at}");
    }
    // A riff whose owner is gone keeps that fact in the checkpoint.
    let gone = given(&[owner_set(Some(ADA)), owner_set(None)]).apply(&[]);
    let loaded = State::load(Some(gone.snapshot(now, 0)), [], now, 0);
    assert!(loaded.owned());
    assert_eq!(loaded.people().members().owner, None);
}
