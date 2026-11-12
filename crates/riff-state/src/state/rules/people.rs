//! The rules of the people (01M3XA875QZ584JBGA37853PWX): one test for
//! each rule of [`people`](crate::state::people), in the form given,
//! when, then. A person is `USER@gmail.com`. The role of a caller comes
//! from the people of the state, as in the engine.

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

/// An old record of the owner role (01M7767T393P46VTNNE5SY203K).
fn owner_set(email: Option<&str>) -> Change {
    Change::OwnerSet(OwnerSet {
        email: email.map(str::to_owned),
    })
}

/// An old request for the owner role (01M7767T393P46VTNNE5SY203K).
fn owner_asked(email: &str, due_ms: u64) -> Change {
    Change::OwnerAsked(record::OwnerAsked {
        email: email.into(),
        due_ms,
    })
}

fn signins_ended(user: &str) -> Change {
    Change::SigninsEnded(SigninsEnded { user: user.into() })
}

/// Ada and Bob are admins. Carol is a member. Each of them signed in.
fn riff_of_ada() -> Vec<Change> {
    vec![
        person_joined("ada", ADA),
        admin_set(ADA, true),
        member_invited(BOB),
        admin_set(BOB, true),
        person_joined("bob", BOB),
        member_invited(CAROL),
        person_joined("carol", CAROL),
    ]
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

fn name_admins(emails: &[&str]) -> NameAdmins {
    NameAdmins {
        emails: emails.iter().map(|e| (*e).to_owned()).collect(),
    }
}

fn revoke(user: Option<&str>) -> wire::Revoke {
    wire::Revoke {
        user: user.map(str::to_owned),
    }
}

fn server() -> SessionUri {
    riff_core::name::SessionUri::server()
}

impl Given {
    /// The person `user` sends `ask`, with the role that the people
    /// give to `user`.
    fn when_person<A: Ask>(self, user: &str, ask: A) -> When {
        let me: SessionUri = format!("riff://{user}@heron").parse().unwrap();
        let caller = Caller::of(&me).with_role(self.state.role(user));
        self.when_as(&caller, ask)
    }

    /// The provider verified `email`: the sign-in sends `admit`.
    fn when_signs_in(self, email: &str, allowed_domain: bool) -> When {
        let user = crate::state::people::user_of(&crate::state::people::email(email)).unwrap();
        let caller = Caller::sign_in(&Who::new(&user, None).unwrap(), email);
        self.when_as(&caller, admit(email, allowed_domain))
    }
}

impl When {
    /// The command makes `changes`, and posts `lines` in the chat, in
    /// this order (01M76S6128EZRZS2D5CE2QCC05). Each line is from the
    /// server, has the kind `riff`, has no `to`, and wakes no session
    /// (01M77BSQQGGNE83ECS27YGVEP1, 01M6Y4ZCTSTVM971BDC1F7GJPK).
    fn then_chat(self, changes: &[Change], lines: &[&str]) {
        let Ok(made) = self.0 else {
            panic!("refused");
        };
        let (chat, rest): (Vec<Change>, Vec<Change>) = made.into_iter().partition(|change| {
            matches!(change, Change::Posted(posted) if posted.thread == ThreadName::chat())
        });
        assert_eq!(rest, changes);
        let posted: Vec<&Posted> = chat
            .iter()
            .filter_map(|change| match change {
                Change::Posted(posted) => Some(&**posted),
                _ => None,
            })
            .collect();
        let bodies: Vec<&str> = posted.iter().map(|p| p.message.body.as_str()).collect();
        assert_eq!(bodies, lines);
        for line in posted {
            assert_eq!(line.message.from, server());
            assert_eq!(line.message.kind, Kind::Riff);
            assert!(line.message.to.is_empty());
            assert!(line.woken.is_empty());
        }
    }
}

/// The first person who signs in to a riff with no admin is an admin
/// and a member (01M7767SRX4K1N1BCDHT7M85DN).
#[test]
fn the_first_person_that_signs_in_is_an_admin() {
    given(&[]).when_signs_in(ADA, false).then(&[
        person_joined("ada", ADA),
        member_invited(ADA),
        admin_set(ADA, true),
    ]);
    // A member of a riff with no admin, for example from an invite of
    // the settings, is an admin at the sign-in too.
    given(&[member_invited(ADA)])
        .when_signs_in(ADA, false)
        .then(&[person_joined("ada", ADA), admin_set(ADA, true)]);
}

/// A riff with an admin, from a sign-in or from `--admin` at a start
/// (01M7767SVH8AKK7144QTJGGGRY), is not new: the next person is no admin.
#[test]
fn on_a_riff_with_an_admin_the_next_person_is_no_admin() {
    let named = [member_invited(BOB), admin_set(BOB, true)];
    // A person of an allowed domain signs in, and is a member.
    given(&named)
        .when_signs_in(ADA, true)
        .then(&[person_joined("ada", ADA)]);
    // A person of no allowed domain is refused.
    given(&named)
        .when_signs_in(ADA, false)
        .then_refused_as(Code::NotMember, "riff invite ada@gmail.com");
    // The admin of the setting signs in.
    given(&named)
        .when_signs_in(" Bob@Gmail.com ", false)
        .then(&[person_joined("bob", BOB)]);
}

#[test]
fn a_person_who_is_no_member_signs_in_only_with_an_invite_or_an_allowed_domain() {
    given(&riff_of_ada())
        .when_signs_in(DAN, false)
        .then_refused_as(
            Code::NotMember,
            "dan@gmail.com is not a member of this riff; ask an admin to run: \
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
        .then_chat(
            &[member_invited(DAN)],
            &["members: bob invited dan@gmail.com. dan@gmail.com is a member now."],
        );
    given(&riff_of_ada())
        .when_person("carol", invite(DAN))
        .then_refused_as(
            Code::NotAllowed,
            "carol is not an admin; only an admin can invite a person",
        );
    given(&riff_of_ada())
        .when_person("bob", remove(CAROL))
        .then_chat(
            &[member_removed(CAROL)],
            &["members: bob removed carol@gmail.com. carol@gmail.com is not a member now."],
        );
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
fn an_admin_cannot_be_removed() {
    given(&riff_of_ada())
        .when_person("bob", remove(ADA))
        .then_refused_as(
            Code::BadRequest,
            "run riff admin remove ada@gmail.com first",
        );
    given(&riff_of_ada())
        .when_person("ada", remove(BOB))
        .then_refused_as(
            Code::BadRequest,
            "run riff admin remove bob@gmail.com first",
        );
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

/// Each admin adds and removes an admin (01M77671TQ92487GSM24RJG82Z). A member
/// cannot.
#[test]
fn an_admin_adds_and_removes_an_admin() {
    // A person who is no member is a member too.
    given(&riff_of_ada())
        .when_person("ada", set_admin("Dan@gmail.com", true))
        .then_chat(
            &[member_invited(DAN), admin_set(DAN, true)],
            &["members: ada made dan@gmail.com an admin."],
        );
    given(&riff_of_ada())
        .when_person("bob", set_admin(CAROL, true))
        .then_chat(
            &[admin_set(CAROL, true)],
            &["members: bob made carol@gmail.com an admin."],
        );
    given(&riff_of_ada())
        .when_person("ada", set_admin(BOB, true))
        .then(&[]);
    given(&riff_of_ada())
        .when_person("ada", set_admin(BOB, false))
        .then_chat(
            &[admin_set(BOB, false)],
            &["members: ada made bob@gmail.com a member again, not an admin."],
        );
    given(&riff_of_ada())
        .when_person("bob", set_admin(ADA, false))
        .then_chat(
            &[admin_set(ADA, false)],
            &["members: bob made ada@gmail.com a member again, not an admin."],
        );
    // An admin removes the own role while another admin stays.
    given(&riff_of_ada())
        .when_person("ada", set_admin(ADA, false))
        .then_chat(
            &[admin_set(ADA, false)],
            &["members: ada made ada@gmail.com a member again, not an admin."],
        );
    given(&riff_of_ada())
        .when_person("carol", set_admin(DAN, true))
        .then_refused_as(
            Code::NotAllowed,
            "carol is not an admin; only an admin adds or removes an admin",
        );
    given(&riff_of_ada())
        .when_person("ada", set_admin(CAROL, false))
        .then_refused_as(Code::BadRequest, "carol@gmail.com is not an admin");
}

/// The last admin stays (01M7767SY5PW1AMB0PBMW2GXDX): a riff with an admin
/// keeps one.
#[test]
fn the_last_admin_stays() {
    let mut alone = riff_of_ada();
    alone.push(admin_set(BOB, false));
    given(&alone)
        .when_person("ada", set_admin(" Ada@Gmail.com", false))
        .then_refused_as(Code::BadRequest, &crate::state::people::last_admin(ADA));
    // Bob is a member again, and cannot remove an admin.
    given(&alone)
        .when_person("bob", set_admin(ADA, false))
        .then_refused_as(Code::NotAllowed, "bob is not an admin");
    // With a second admin, the first can go.
    alone.push(admin_set(CAROL, true));
    given(&alone)
        .when_person("carol", set_admin(ADA, false))
        .then_chat(
            &[admin_set(ADA, false)],
            &["members: carol made ada@gmail.com a member again, not an admin."],
        );
}

/// The log reads an old `owner_set` as an `admin_set` of its email
/// (01M7767T393P46VTNNE5SY203K). Its other records of the owner role change
/// nothing.
#[test]
fn an_old_log_with_the_owner_role_loads_with_the_owner_as_an_admin() {
    let old = [
        person_joined("ada", ADA),
        owner_set(Some(ADA)),
        member_invited(BOB),
        person_joined("bob", BOB),
        owner_asked(BOB, 9),
        Change::OwnerDenied(by_email(BOB)),
        owner_set(Some(BOB)),
        owner_set(None),
    ];
    let state = given(&old).apply(&[]);
    assert_eq!(state.role("ada"), Role::Admin);
    assert_eq!(state.role("bob"), Role::Admin);
    let members = state.people().members();
    assert_eq!(members.admins, [ADA, BOB]);
    assert!(members.members.is_empty());
    // The old owner is an admin of today: ada adds an admin.
    given(&old)
        .when_person("ada", set_admin(CAROL, true))
        .then_chat(
            &[member_invited(CAROL), admin_set(CAROL, true)],
            &["members: ada made carol@gmail.com an admin."],
        );
}

/// The setting `--admin EMAIL` names each admin at each start
/// (01M7767SVH8AKK7144QTJGGGRY): an email that is not an admin becomes an admin
/// and a member. A name that is not an email names nobody.
#[test]
fn the_setting_names_each_admin_at_each_start() {
    given(&[])
        .when(
            &server(),
            name_admins(&[" Ada@Gmail.com", "not-an-email", ADA]),
        )
        .then(&[member_invited(ADA), admin_set(ADA, true)]);
    given(&riff_of_ada())
        .when(&server(), name_admins(&[ADA, BOB]))
        .then(&[]);
    // An admin that riff admin remove made a member is an admin again.
    let mut removed = riff_of_ada();
    removed.push(admin_set(BOB, false));
    given(&removed)
        .when(&server(), name_admins(&[BOB]))
        .then(&[admin_set(BOB, true)]);
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
    given(&team())
        .when(&person(), name_admins(&[ADA]))
        .then_refused_as(
            Code::NotAllowed,
            "a person cannot send the command name_admins",
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
    assert_eq!(check.result.unwrap().0[0], member_invited(DAN));
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
    // An admin makes bob a member again. The record waits in the queue.
    state.queue(&cause, &[admin_set(BOB, false)], now);
    assert_eq!(state.role("bob"), Role::Member, "the pending copy");
    // A query still reads the written copy.
    assert_eq!(state.people().role_of("bob"), Role::Admin);
}

/// A start from a checkpoint gives the people of a full replay, for a
/// checkpoint at each position. An old record of the owner role is in
/// the log (01M7767T393P46VTNNE5SY203K).
#[test]
fn a_start_from_a_checkpoint_gives_the_people_of_a_full_replay() {
    let mut changes = vec![riff_made("r1"), riff_set(RiffState::Paused)];
    changes.extend(riff_of_ada());
    changes.extend([
        owner_asked(BOB, 9),
        member_removed(CAROL),
        signins_ended("bob"),
        admin_set(CAROL, true),
        owner_set(Some(DAN)),
    ]);
    let records: Vec<Record> = changes
        .iter()
        .enumerate()
        .map(|(n, change)| Record {
            envelope: Envelope {
                position: u64::try_from(n).unwrap() + 1,
                written_at_ms: 0,
                by: None,
                command: None,
                call: None,
            },
            change: change.clone(),
        })
        .collect();
    let now = Instant::now();
    let full = State::replay(records.clone(), now, 0);
    assert_eq!(full.riff_id().as_deref(), Some("r1"));
    assert_eq!(full.people().members().admins, [ADA, BOB, CAROL, DAN]);
    for at in 0..=records.len() {
        let snapshot = State::replay(records[..at].to_vec(), now, 0).snapshot(now, 0);
        // The checkpoint is JSON in the store.
        let json = serde_json::to_vec(&snapshot).unwrap();
        let snapshot: Snapshot = serde_json::from_slice(&json).unwrap();
        let loaded = State::load(Some(snapshot), records[at..].to_vec(), now, 0);
        assert!(loaded.same_log_state(&full), "a checkpoint at {at}");
        assert_eq!(loaded.signins_ended(), full.signins_ended(), "at {at}");
    }
    // A riff whose old owner was gone keeps the old owner as an admin.
    let gone = given(&[owner_set(Some(ADA)), owner_set(None)]).apply(&[]);
    let loaded = State::load(Some(gone.snapshot(now, 0)), [], now, 0);
    assert!(loaded.has_admin());
    assert_eq!(loaded.people().members().admins, [ADA]);
}

/// The people of a riff with the repository acme/app: ann and bob are
/// admins, and the sessions of both are live.
fn ann_and_bob(tail: &[Change]) -> Given {
    let mut riff = team();
    riff.extend([
        person_joined("ann", "ann@acme.io"),
        admin_set("ann@acme.io", true),
        member_invited("bob@acme.io"),
        admin_set("bob@acme.io", true),
        person_joined("bob", "bob@acme.io"),
    ]);
    riff.extend_from_slice(tail);
    given(&riff).live(&[ann(), bob()])
}

/// The records of a command that is not refused.
fn made(when: When) -> Vec<Change> {
    match when {
        When(Ok(changes)) => changes,
        When(Err(refused)) => panic!("refused: {refused}"),
    }
}

/// The one post of a chunk: a line in the chat (01M76S6128EZRZS2D5CE2QCC05).
/// The chunk posts no note to the thread of a repository, and no direct
/// message. The line is from the server, has the kind `riff`, has no
/// `to`, and wakes no session (01M77BSQQGGNE83ECS27YGVEP1).
fn line_of(changes: &[Change]) -> String {
    let posts: Vec<&Posted> = changes
        .iter()
        .filter_map(|change| match change {
            Change::Posted(posted) => Some(&**posted),
            _ => None,
        })
        .collect();
    let [line] = &posts[..] else {
        panic!("not one post: {changes:?}");
    };
    assert_eq!(line.thread, ThreadName::chat());
    assert_eq!(
        (&line.message.from, line.message.kind),
        (&server(), Kind::Riff)
    );
    assert!(line.message.to.is_empty() && line.woken.is_empty());
    line.message.body.clone()
}

/// An invite, a removal, a new admin and an admin who is a member again
/// each post one line in the chat, and no note in the thread of a
/// repository: the riff has the repository acme/app
/// (01M76S6128EZRZS2D5CE2QCC05). A change of nothing posts no line.
#[test]
fn each_change_of_the_members_posts_one_line_in_the_chat() {
    assert_eq!(ann_and_bob(&[]).state.repositories(), [repo()]);
    let invited = made(ann_and_bob(&[]).when_person("ann", invite("cy@acme.io")));
    assert_eq!(
        line_of(&invited),
        "members: ann invited cy@acme.io. cy@acme.io is a member now."
    );
    let cy = [member_invited("cy@acme.io")];
    let removed = made(ann_and_bob(&cy).when_person("ann", remove("cy@acme.io")));
    assert_eq!(
        line_of(&removed),
        "members: ann removed cy@acme.io. cy@acme.io is not a member now."
    );
    let added = made(ann_and_bob(&cy).when_person("bob", set_admin("cy@acme.io", true)));
    assert_eq!(line_of(&added), "members: bob made cy@acme.io an admin.");
    let dropped = made(ann_and_bob(&[]).when_person("ann", set_admin("bob@acme.io", false)));
    assert_eq!(
        line_of(&dropped),
        "members: ann made bob@acme.io a member again, not an admin."
    );
    let again = made(ann_and_bob(&[]).when_person("ann", invite("bob@acme.io")));
    assert!(again.is_empty(), "{again:?}");
}
