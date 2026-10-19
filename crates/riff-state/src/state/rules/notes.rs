//! The rules of the notes of the lead (01M59258K3TYR09HC4AWHWZPEX,
//! 01M59258NTXPMWV18TKRKWNTYN, 01M59258RJ3CG498Z8AVQ5TV5Q): one test for
//! each rule of [`notes`](crate::state::notes), in the form given, when,
//! then.

use riff_core::notes::NOTES_MAX;
use riff_core::record::NotesSet;

use super::*;

struct Notes(String);

impl Ask for Notes {
    type Command = wire::SetNotes;
    fn of(self, me: &SessionUri) -> wire::SetNotes {
        wire::SetNotes {
            me: me.clone(),
            thread: repo(),
            text: self.0,
        }
    }
}

fn notes(text: &str) -> Notes {
    Notes(text.into())
}

fn notes_set(user: &str, text: &str) -> Change {
    Change::NotesSet(NotesSet {
        thread: repo(),
        user: user.into(),
        text: text.into(),
    })
}

/// A session of the user writes the notes of the user. The same text
/// makes no record. A new text is a new record.
#[test]
fn the_lead_writes_the_notes_of_its_user() {
    let lead = Caller::of(&ann());
    given(&team())
        .when_as(&lead, notes("Wave 36 runs."))
        .then(&[notes_set("ann", "Wave 36 runs.")]);
    let written = [team(), vec![notes_set("ann", "Wave 36 runs.")]].concat();
    given(&written)
        .when_as(&lead, notes("Wave 36 runs."))
        .then(&[]);
    given(&written)
        .when_as(&lead, notes("Wave 37 runs."))
        .then(&[notes_set("ann", "Wave 37 runs.")]);
    // No notes and an empty text: nothing to write.
    given(&team()).when_as(&lead, notes("")).then(&[]);
}

/// The person of the user writes the same notes.
#[test]
fn the_person_of_the_user_writes_the_notes_of_the_user() {
    given(&team())
        .when_as(&Caller::of(&person()), notes("Prefer thelio."))
        .then(&[notes_set("ann", "Prefer thelio.")]);
}

/// A worker cannot write the notes.
#[test]
fn a_worker_cannot_write_the_notes() {
    for role in Role::ALL {
        let worker = Caller::of(&ann2()).with_role(role);
        given(&worker_records())
            .when_as(&worker, notes("x"))
            .then_refused_as(
                Code::NotAllowed,
                "a worker cannot write the notes of the lead",
            );
    }
}

/// A text over the limit is refused, and says the limit. A text at the
/// limit is taken.
#[test]
fn a_text_over_the_limit_is_refused() {
    let lead = Caller::of(&ann());
    given(&team())
        .when_as(&lead, Notes("a".repeat(NOTES_MAX + 1)))
        .then_refused_as(Code::BadRequest, "the limit is 65536 bytes (64 KiB)");
    let at = "a".repeat(NOTES_MAX);
    given(&team())
        .when_as(&lead, Notes(at.clone()))
        .then(&[notes_set("ann", &at)]);
}

/// A text that looks like a token is refused.
#[test]
fn a_text_that_looks_like_a_token_is_refused() {
    given(&team())
        .when_as(
            &Caller::of(&ann()),
            notes("claude token: sk-ant-oat01-abcdefghijklmnop"),
        )
        .then_refused_as(Code::BadRequest, "looks like it has a secret");
}

/// The notes need a repository thread.
#[test]
fn the_notes_need_a_repository_thread() {
    let ask = wire::SetNotes {
        me: ann(),
        thread: design(),
        text: "x".into(),
    };
    let Given { mut state, now } = given(&team()).live(&[ann()]);
    let refused = state.run(&Caller::of(&ann()), &ask, now).unwrap_err();
    assert_eq!(refused.code, Code::BadRequest);
}

/// The user reads the notes, the owner reads the notes of each user,
/// and another member reads no notes of another user. The history has
/// each old text, the oldest first.
#[test]
fn the_user_and_the_owner_read_the_notes_and_the_history() {
    let records = [
        team(),
        vec![
            notes_set("ann", "one"),
            notes_set("ann", "two"),
            notes_set("bob", "bob's"),
        ],
    ]
    .concat();
    let state = given(&records).live(&[ann(), bob()]).state;
    let ann = Caller::of(&ann());
    let now = state.notes(&ann, &repo(), None, false).unwrap();
    assert_eq!((now.user.as_str(), now.text.as_str()), ("ann", "two"));
    assert!(now.history.is_empty());
    let all = state.notes(&ann, &repo(), Some("ann"), true).unwrap();
    let before: Vec<_> = all.history.iter().map(|v| v.text.as_str()).collect();
    assert_eq!(before, ["one"]);
    // Another member.
    let refused = state.notes(&ann, &repo(), Some("bob"), false).unwrap_err();
    assert_eq!(refused.code, Code::NotAllowed);
    // The owner.
    let owner = Caller::of(&person()).with_role(Role::Owner);
    let bobs = state.notes(&owner, &repo(), Some("bob"), false).unwrap();
    assert_eq!(bobs.text, "bob's");
    // An admin is no owner.
    let admin = Caller::of(&person()).with_role(Role::Admin);
    assert!(state.notes(&admin, &repo(), Some("bob"), false).is_err());
    // No notes: an empty text.
    let none = state.notes(&ann, &design(), None, true).unwrap();
    assert_eq!(none.text, "");
    assert!(none.by.is_none() && none.at_ms.is_none());
}
