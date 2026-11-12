//! The group "notes": the command [`SetNotes`], and the notes of the
//! lead: one text for each project (01M77XMZBJ6DRGJTGEEE0EDECP). The
//! storage is a part of [`Plans`](super::plan::Plans): the notes are in
//! the part of the repository thread, and in its part of the checkpoint.
//!
//! | Who | Writes | Reads |
//! |---|---|---|
//! | the lead of the project | yes | yes |
//! | an admin, as a person or as a session that is no worker | yes | yes |
//! | a worker | no | yes |
//! | each other member | no | yes |
//!
//! [`permits`](super::permits) refuses a worker, and `handle` each other
//! caller that is not the lead or an admin. `handle` refuses a text over
//! the limit and a text that looks like a token
//! ([`riff_core::notes::check`]). The same text as the text now makes no
//! record. Each text stays: the history is the old texts of the project.
//!
//! The part keeps the texts by the user of the writer, as the checkpoint
//! did before one text for each project. The text now is the newest text
//! of all users. So the notes of a project with the texts of two users
//! start from the newest, and the older text is in the history.
//!
//! ```mermaid
//! flowchart LR
//!     W[notes set TEXT] --> L{the lead or an admin?}
//!     L -->|no| A[not_allowed]
//!     L -->|yes| C{check: size, token}
//!     C -->|refused| X[bad_request]
//!     C -->|good| S{same text as now?}
//!     S -->|yes| N[no record]
//!     S -->|no| R[(notes_set)]
//!     R --> H[the old text goes to the history]
//! ```

use riff_core::notes;
use riff_core::record::{Change, NotesSet};
use riff_core::wire::{NotesReply, SetNotes, SetNotesReply};

use super::command::{Caller, Class, Code, Command, CommandKind, Done, Now, Refused, Role};
use super::plan::repository;
use super::view::View;

/// Writes the notes of the project of a repository thread
/// (01M77XMZBJ6DRGJTGEEE0EDECP). Only the lead of the project and an
/// admin write them. The record names the user of the caller. The same
/// text as the text now makes no record.
impl Command for SetNotes {
    const KIND: CommandKind = CommandKind::Notes;
    type Reply = SetNotesReply;
    type Note = ();

    fn handle(
        &self,
        caller: &Caller,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, ()), Refused> {
        let SetNotes {
            me: _,
            thread,
            text,
        } = self;
        repository(thread)?;
        let who = caller.who();
        let lead = caller.class() == Class::Session
            && view.lead_of(&(who.user().to_owned(), thread.clone()), now.at) == Some(who);
        if !lead && caller.role() < Role::Admin {
            return Err(Refused::new(
                Code::NotAllowed,
                format!(
                    "only the lead of {thread} and an admin write the notes of the project. \
                     Tell the lead."
                ),
            ));
        }
        notes::check(text).map_err(|reason| Refused::new(Code::BadRequest, reason))?;
        let then = view
            .riff
            .plans()
            .notes(thread)
            .map_or("", |version| version.text.as_str());
        let mut changes = Vec::new();
        if then != text {
            changes.push(Change::NotesSet(NotesSet {
                thread: thread.clone(),
                user: who.user().to_owned(),
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

/// The reply of the query `notes`: the text now of the project of
/// `thread`, and with `history` the old texts. Each member reads them
/// (01M77XMZBJ6DRGJTGEEE0EDECP). A project with no notes has an empty
/// text.
pub fn reply(
    plans: &super::plan::Plans,
    thread: &riff_core::name::ThreadName,
    history: bool,
) -> NotesReply {
    let now = plans.notes(thread);
    NotesReply {
        text: now.map(|v| v.text.clone()).unwrap_or_default(),
        by: now.and_then(|v| v.by.clone()),
        at_ms: now.map(|v| v.at_ms),
        history: if history {
            plans.notes_before(thread)
        } else {
            Vec::new()
        },
    }
}
