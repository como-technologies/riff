//! The rules of the notes of the lead (01M59258K3TYR09HC4AWHWZPEX,
//! 01M77XMZBJ6DRGJTGEEE0EDECP, 01M59258RJ3CG498Z8AVQ5TV5Q): one test for
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

/// The team, and ann is the lead of the project: the session ann holds
/// the claim `lead`. bob is the lead of its user, and not the lead of the
/// project.
fn led() -> Vec<Change> {
    [team(), vec![claimed(&ann(), LEAD_CLAIM)]].concat()
}

/// The person `user` on the command line.
fn person_of(user: &str) -> SessionUri {
    format!("riff://{user}@kite").parse().unwrap()
}

/// The lead of the project writes the notes of the project. The record
/// names the user of the writer. The same text as the text now makes no
/// record. A new text is a new record.
#[test]
fn the_lead_of_the_project_writes_the_notes_of_the_project() {
    let lead = Caller::of(&ann());
    given(&led())
        .live(&[ann()])
        .when_as(&lead, notes("Wave 36 runs."))
        .then(&[notes_set("ann", "Wave 36 runs.")]);
    // bob wrote the text now: the same text from the lead makes no record.
    let written = [led(), vec![notes_set("bob", "Wave 36 runs.")]].concat();
    given(&written)
        .live(&[ann()])
        .when_as(&lead, notes("Wave 36 runs."))
        .then(&[]);
    given(&written)
        .live(&[ann()])
        .when_as(&lead, notes("Wave 37 runs."))
        .then(&[notes_set("ann", "Wave 37 runs.")]);
    // No notes and an empty text: nothing to write.
    given(&led())
        .live(&[ann()])
        .when_as(&lead, notes(""))
        .then(&[]);
}

/// The lead of another user is not the lead of the project: a member
/// that is not the lead cannot write the notes.
#[test]
fn a_member_that_is_not_the_lead_cannot_write_the_notes() {
    given(&led())
        .live(&[ann(), bob()])
        .when_as(&Caller::of(&bob()), notes("Prefer kite."))
        .then_refused_as(
            Code::NotAllowed,
            "only the lead of acme/app and an admin write the notes of the project",
        );
    given(&led())
        .when_as(&Caller::of(&person_of("bob")), notes("Prefer kite."))
        .then_refused_as(Code::NotAllowed, "only the lead of acme/app and an admin");
}

/// An admin writes the notes of the project, as a person or as a session
/// that is not the lead.
#[test]
fn an_admin_writes_the_notes_of_the_project() {
    let admin = Caller::of(&person_of("bob")).with_role(Role::Admin);
    given(&led())
        .when_as(&admin, notes("Prefer thelio."))
        .then(&[notes_set("bob", "Prefer thelio.")]);
    let session = Caller::of(&bob()).with_role(Role::Admin);
    given(&led())
        .live(&[ann(), bob()])
        .when_as(&session, notes("Prefer kite."))
        .then(&[notes_set("bob", "Prefer kite.")]);
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
    given(&led())
        .live(&[ann()])
        .when_as(&lead, Notes("a".repeat(NOTES_MAX + 1)))
        .then_refused_as(Code::BadRequest, "the limit is 65536 bytes (64 KiB)");
    let at = "a".repeat(NOTES_MAX);
    given(&led())
        .live(&[ann()])
        .when_as(&lead, Notes(at.clone()))
        .then(&[notes_set("ann", &at)]);
}

/// A text that looks like a token is refused.
#[test]
fn a_text_that_looks_like_a_token_is_refused() {
    given(&led())
        .live(&[ann()])
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
    let Given { mut state, now } = given(&led()).live(&[ann()]);
    let refused = state.run(&Caller::of(&ann()), &ask, now).unwrap_err();
    assert_eq!(refused.code, Code::BadRequest);
}

/// The texts of the notes of each user in `records`, each with the time
/// `at_ms` of its record.
fn timed(records: &[(Change, u64)]) -> State {
    let records = records
        .iter()
        .enumerate()
        .map(|(n, (change, at_ms))| Record {
            envelope: Envelope {
                position: u64::try_from(n).unwrap() + 1,
                written_at_ms: *at_ms,
                by: None,
                command: None,
                call: None,
            },
            change: change.clone(),
        });
    State::replay(records, Instant::now(), 0)
}

/// The notes of a project are one text: the newest text, of any user.
/// The history has each other text, the oldest first.
#[test]
fn the_notes_are_the_newest_text_of_the_project() {
    let state = timed(&[
        (notes_set("bob", "one"), 1_000),
        (notes_set("ann", "two"), 2_000),
        (notes_set("bob", "three"), 3_000),
    ]);
    let now = state.notes(&repo(), true);
    assert_eq!(now.text, "three");
    assert_eq!(now.at_ms, Some(3_000));
    let before: Vec<_> = now.history.iter().map(|v| v.text.as_str()).collect();
    assert_eq!(before, ["one", "two"]);
    assert!(state.notes(&repo(), false).history.is_empty());
    // No notes: an empty text.
    let none = state.notes(&design(), true);
    assert_eq!(none.text, "");
    assert!(none.by.is_none() && none.at_ms.is_none());
}

/// The notes of a project with two texts of today, of two users, start
/// from the newest, after a replay of the log and after a load of the
/// checkpoint. The older text is in the history.
#[test]
fn the_notes_of_a_project_with_two_texts_of_today_start_from_the_newest() {
    for (ann_at, bob_at, newest, older) in [
        (2_000, 1_000, "ann's", "bob's"),
        (1_000, 2_000, "bob's", "ann's"),
    ] {
        let mut today = [
            (notes_set("ann", "ann's"), ann_at),
            (notes_set("bob", "bob's"), bob_at),
        ];
        today.sort_by_key(|(_, at_ms)| *at_ms);
        let replayed = timed(&today);
        let snapshot = replayed.snapshot(Instant::now(), 0);
        let loaded = State::load(Some(snapshot), [], Instant::now(), 0);
        for state in [&replayed, &loaded] {
            let notes = state.notes(&repo(), true);
            assert_eq!(notes.text, newest);
            let history: Vec<_> = notes.history.iter().map(|v| v.text.as_str()).collect();
            assert_eq!(history, [older]);
        }
    }
}

/// Each member reads the notes of the project: a session of another
/// user, a worker and a person who is no admin. The query takes no
/// caller.
#[test]
fn each_member_reads_the_notes_of_the_project() {
    let records = [
        worker_records(),
        vec![
            claimed(&ann(), LEAD_CLAIM),
            notes_set("ann", "Wave 36 runs."),
        ],
    ]
    .concat();
    let state = given(&records).live(&[ann(), ann2(), bob()]).state;
    assert_eq!(state.notes(&repo(), false).text, "Wave 36 runs.");
}
