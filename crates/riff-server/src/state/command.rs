//! A command: a call that asks for a change of the riff.
//!
//! Each command is a type that implements [`Command`]
//! (01M3WNQRCBP0PHSA0H3THDH5NJ). The type is in the file of its group:
//!
//! | Group | File | Commands |
//! |---|---|---|
//! | sessions | [`super::sessions`] | [`Register`](super::Register), [`Start`](super::Start), [`End`](super::End) |
//! | threads | [`super::threads`] | [`Join`](super::Join), [`Leave`](super::Leave), [`Post`](riff_core::wire::Post), [`Announce`](super::Announce) |
//! | work | [`super::work`] | [`Claim`](super::Claim), [`Release`](super::Release), [`ReleaseFor`](super::ReleaseFor), [`Lead`](super::Lead) |
//! | the riff | [`super::the_riff`] | [`SetRiff`](super::SetRiff), [`SetIdle`](super::SetIdle), [`Forget`](super::Forget) |

use std::time::Instant;

use riff_core::name::SessionUri;
use riff_core::record::Change;

use super::view::View;

/// The time of a call: an instant, and the same time in milliseconds
/// since the Unix epoch, from the clock of the state.
#[derive(Clone, Copy, Debug)]
pub struct Now {
    pub at: Instant,
    pub ms: u64,
}

/// A call that asks for a change. [`Command::handle`] holds its rule.
///
/// ```
/// use std::time::Instant;
/// use riff_core::name::SessionUri;
/// use riff_server::state::{Claim, Command, State};
///
/// /// The number of changes that a command makes now.
/// fn changes<C: Command>(state: &State, me: &SessionUri, command: &C) -> usize {
///     state.handle(me, command, Instant::now()).map_or(0, |changes| changes.len())
/// }
///
/// let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let mut state = State::default();
/// state.register(&mike, Instant::now());
/// let thread = mike.default_thread().unwrap();
/// // A new riff is paused, so a claim is refused.
/// assert_eq!(changes(&state, &mike, &Claim { thread, item: "issue-12".into() }), 0);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub trait Command {
    /// What `handle` keeps for the reply, for example the selectors of
    /// a post that matched no session.
    type Note;

    /// Checks the command of `me` against `view`. Gives the changes and
    /// the note, or why the command is refused. It does no I/O and
    /// changes nothing.
    fn handle(
        &self,
        me: &SessionUri,
        view: &View<'_>,
        now: Now,
    ) -> Result<(Vec<Change>, Self::Note), String>;
}
