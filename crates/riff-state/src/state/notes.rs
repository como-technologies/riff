//! The group "notes": the command [`SetNotes`], and the notes of the
//! lead of each user in each repository thread
//! (01M59258K3TYR09HC4AWHWZPEX). The storage is a part of
//! [`Plans`](super::plan::Plans): the notes are in the part of the
//! repository thread, and in its part of the checkpoint.
//!
//! | Who | Writes | Reads (01M59258NTXPMWV18TKRKWNTYN) |
//! |---|---|---|
//! | the person of the user, and a session of the user that is no worker | the notes of the user | the notes of the user |
//! | the owner | no | the notes of each user |
//! | each other member | no | no |
//!
//! [`permits`](super::permits) refuses a worker. `handle` refuses a text
//! over the limit and a text that looks like a token
//! ([`riff_core::notes::check`]). The same text makes no record. Each
//! text stays: the history is the old texts of the part.
//!
//! ```mermaid
//! flowchart LR
//!     W[notes set TEXT] --> C{check: size, token}
//!     C -->|refused| X[bad_request]
//!     C -->|good| S{same text?}
//!     S -->|yes| N[no record]
//!     S -->|no| R[(notes_set)]
//!     R --> H[the old text goes to the history]
//! ```

use riff_core::notes;
use riff_core::record::{Change, NotesSet};
use riff_core::wire::{NotesReply, SetNotes, SetNotesReply};

use super::command::{Caller, Code, Command, CommandKind, Done, Now, Refused, Role};
use super::plan::repository;
use super::view::View;

/// Writes the notes of the lead of the caller in a repository thread
/// (01M59258K3TYR09HC4AWHWZPEX). The user of the notes is the user of
/// the caller. The same text as the text now makes no record.
impl Command for SetNotes {
    const KIND: CommandKind = CommandKind::Notes;
    type Reply = SetNotesReply;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        _: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let SetNotes {
            me: _,
            thread,
            text,
        } = self;
        repository(thread)?;
        notes::check(text).map_err(|reason| Refused::new(Code::BadRequest, reason))?;
        let user = caller.who().user();
        let now = view
            .riff
            .plans()
            .notes(thread, user)
            .map_or("", |version| version.text.as_str());
        let mut changes = Vec::new();
        if now != text {
            changes.push(Change::NotesSet(NotesSet {
                thread: thread.clone(),
                user: user.to_owned(),
                text: text.clone(),
            }));
        }
        Ok((changes, ()))
    }

    fn reply(&self, _: &Caller, _: &View<'_>, done: &Done, (): (), _: Now) -> SetNotesReply {
        SetNotesReply {
            changed: !done.made.is_empty(),
        }
    }
}

/// Says who reads the notes of `user`: the user of the caller, and the
/// owner (01M59258NTXPMWV18TKRKWNTYN).
pub fn may_read(caller: &Caller, user: &str) -> Result<(), Refused> {
    if caller.who().user() == user || caller.role() >= Role::Owner {
        Ok(())
    } else {
        Err(Refused::new(
            Code::NotAllowed,
            format!("only {user} and the owner can read the notes of the lead of {user}"),
        ))
    }
}

/// The reply of the query `notes`: the text now and with `history` the
/// old texts. A lead with no notes has an empty text.
pub fn reply(
    plans: &super::plan::Plans,
    thread: &riff_core::name::ThreadName,
    user: &str,
    history: bool,
) -> NotesReply {
    let now = plans.notes(thread, user);
    NotesReply {
        user: user.to_owned(),
        text: now.map(|v| v.text.clone()).unwrap_or_default(),
        by: now.and_then(|v| v.by.clone()),
        at_ms: now.map(|v| v.at_ms),
        history: if history {
            plans.notes_before(thread, user).to_vec()
        } else {
            Vec::new()
        },
    }
}
