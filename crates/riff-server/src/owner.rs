//! The owner role over time: a request that waits for the owner, and an
//! owner who is gone.
//!
//! # Model
//!
//! ```mermaid
//! stateDiagram-v2
//!     Owner --> Asked: an admin runs riff owner --take
//!     Asked --> Owner: riff owner EMAIL, riff owner --deny, or no answer in N minutes
//!     Owner --> NoOwner: the owner is gone
//!     Asked --> Owner: the owner is gone, the admin that asked is the owner
//!     NoOwner --> Owner: an admin runs riff owner --take
//! ```
//!
//! [`crate::token::Tokens`] keeps the owner, the request that waits and
//! whether the riff has no owner. This module holds the times
//! ([`Timing`], 01M3Q5460YESBSQHTV3M15PE53), the check of the owner
//! ([`Checks`], 01M3Q546335NBTKG5BHQ27QC93), and the text of each note
//! (01M3N7K4DVHSF7AQ402F14J26Z).
//!
//! # Rules
//!
//! - The server checks the owner each [`Timing::check_every`], while the
//!   riff has an owner and an admin who is not the owner. A check misses
//!   when the owner shows no sign of life: no session of the owner is
//!   live, and the owner made no call since the last check, also as a
//!   person (`riff chat`, `riff top`, `riff who`). See
//!   [`crate::state::State::present`]. A check wakes no session: a live
//!   session has an open watch or a keep-alive.
//! - After [`Timing::misses`] misses in a row, the server warns the
//!   owner ([`warn_news`]): a note to each session of the owner, and one
//!   line in the chat. When the next check misses too, the owner is
//!   gone.
//!
//! ```mermaid
//! stateDiagram-v2
//!     Seen --> Missed: a check with no sign of the owner
//!     Missed --> Missed: fewer than P misses in a row
//!     Missed --> Warned: P misses in a row
//!     Warned --> Gone: the next check misses too
//!     Missed --> Seen: a sign of the owner
//!     Warned --> Seen: a sign of the owner
//! ```
//! - The server posts each note as [`server_uri`]: the USER
//!   [`crate::token::SERVER_USER`], which no person can sign in as
//!   (01M3N7K4BC1RPZKQ1XNDTBRPGF). A note has no signature.
//! - Each change posts one note to the thread of each repository of the
//!   riff. It wakes no session. A change that needs a person also sends
//!   the same text to each live lead of that person, as a direct
//!   message: the owner for a request, the admin for a deny, each admin
//!   when the owner is gone.
//!
//! The module does no I/O and reads no clock. The caller passes `now`.

use std::time::{Duration, Instant};

use riff_core::name::SessionUri;

use crate::token::{OwnerChange, SERVER_USER};

/// The times of the owner role (01M3Q5460YESBSQHTV3M15PE53).
///
/// ```
/// use std::time::Duration;
/// use riff_server::owner::Timing;
///
/// let timing = Timing::default();
/// assert_eq!(timing.answer, Duration::from_secs(10 * 60));
/// assert_eq!(timing.check_every, Duration::from_secs(10 * 60));
/// assert_eq!(timing.misses, 3);
/// assert_eq!(timing.tick(), Duration::from_secs(1));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// The time that the owner has to answer a request (N). With no
    /// answer, the admin that asked is the owner.
    pub answer: Duration,
    /// The time between two checks of the owner (M).
    pub check_every: Duration,
    /// The misses in a row after which the server warns the owner (P).
    /// One more miss after the warning, and the owner is gone.
    pub misses: u32,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            answer: Duration::from_secs(10 * 60),
            check_every: Duration::from_secs(10 * 60),
            misses: 3,
        }
    }
}

impl Timing {
    /// The times from the settings of `riff-server`, in minutes.
    pub fn from_minutes(answer: u64, check_every: u64, misses: u32) -> Self {
        Timing {
            answer: Duration::from_secs(answer * 60),
            check_every: Duration::from_secs(check_every * 60),
            misses,
        }
    }

    /// The time between two looks of the server at the owner role: one
    /// second, or less when a time of the role is shorter.
    pub fn tick(&self) -> Duration {
        Duration::from_secs(1)
            .min(self.answer)
            .min(self.check_every)
            .max(Duration::from_millis(1))
    }
}

/// The result of one check of the owner (01M3Q546335NBTKG5BHQ27QC93).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// The owner showed a sign of life. The misses start again.
    Seen,
    /// A miss, with fewer than [`Timing::misses`] in a row.
    Missed,
    /// [`Timing::misses`] misses in a row: the server warns the owner.
    Warn,
    /// A miss after the warning: the owner is gone. The misses start
    /// again.
    Gone,
}

/// The checks of the owner: when the next one is due, and the misses in
/// a row (01M3Q546335NBTKG5BHQ27QC93).
///
/// ```
/// use std::time::{Duration, Instant};
/// use riff_server::owner::{Check, Checks, Timing};
///
/// let timing = Timing { check_every: Duration::from_secs(60), misses: 2, ..Timing::default() };
/// let start = Instant::now();
/// let mut checks = Checks::new(start, &timing);
/// assert!(!checks.due(start));
///
/// let first = start + Duration::from_secs(60);
/// assert!(checks.due(first));
/// assert_eq!(checks.record(first, false, &timing), Check::Missed);
/// assert!(!checks.due(first));
/// let second = first + Duration::from_secs(60);
/// assert_eq!(checks.record(second, true, &timing), Check::Seen, "a sign of life");
/// let third = second + Duration::from_secs(60);
/// assert_eq!(checks.record(third, false, &timing), Check::Missed);
/// let fourth = third + Duration::from_secs(60);
/// assert_eq!(checks.record(fourth, false, &timing), Check::Warn);
/// let fifth = fourth + Duration::from_secs(60);
/// assert_eq!(checks.record(fifth, false, &timing), Check::Gone);
/// assert_eq!(checks.record(fifth + Duration::from_secs(60), false, &timing), Check::Missed);
/// ```
#[derive(Clone, Debug)]
pub struct Checks {
    next: Instant,
    misses: u32,
}

impl Checks {
    /// No misses. The first check is one [`Timing::check_every`] after
    /// `now`.
    pub fn new(now: Instant, timing: &Timing) -> Self {
        Checks {
            next: now + timing.check_every,
            misses: 0,
        }
    }

    /// True when a check is due at `now`.
    pub fn due(&self, now: Instant) -> bool {
        self.next <= now
    }

    /// The time of the last check: one [`Timing::check_every`] before
    /// the next one. A call of the owner at this time or later is a sign
    /// of life.
    pub fn since(&self, timing: &Timing) -> Instant {
        self.next
            .checked_sub(timing.check_every)
            .unwrap_or(self.next)
    }

    /// Records a check at `now`: `seen` is true when the owner showed a
    /// sign of life.
    pub fn record(&mut self, now: Instant, seen: bool, timing: &Timing) -> Check {
        self.next = now + timing.check_every;
        if seen {
            self.misses = 0;
            return Check::Seen;
        }
        self.misses += 1;
        let warn_at = timing.misses.max(1);
        if self.misses > warn_at {
            self.misses = 0;
            Check::Gone
        } else if self.misses == warn_at {
            Check::Warn
        } else {
            Check::Missed
        }
    }

    /// Starts again with no misses, for example while the riff has no
    /// other admin.
    pub fn reset(&mut self, now: Instant, timing: &Timing) {
        *self = Checks::new(now, timing);
    }
}

/// The URI that the server posts its notes as
/// (01M3N7K4BC1RPZKQ1XNDTBRPGF).
///
/// ```
/// assert_eq!(riff_server::owner::server_uri().to_string(), "riff://riff@server");
/// ```
pub fn server_uri() -> SessionUri {
    format!("riff://{SERVER_USER}@server")
        .parse()
        .expect("the URI of the server is valid")
}

/// A time in whole minutes for people: `10 minutes`, `1 minute`, or
/// `less than a minute`.
///
/// ```
/// use std::time::Duration;
/// use riff_server::owner::minutes;
///
/// assert_eq!(minutes(Duration::from_secs(600)), "10 minutes");
/// assert_eq!(minutes(Duration::from_secs(60)), "1 minute");
/// assert_eq!(minutes(Duration::from_secs(5)), "less than a minute");
/// ```
pub fn minutes(time: Duration) -> String {
    match time.as_secs() / 60 {
        0 => "less than a minute".into(),
        1 => "1 minute".into(),
        n => format!("{n} minutes"),
    }
}

/// The note of a request that waits for the owner.
///
/// ```
/// use std::time::Duration;
///
/// assert_eq!(
///     riff_server::owner::asked_news("bob", "bob@gmail.com", "ada@gmail.com", Duration::from_secs(600)),
///     "members: bob asks for the owner role. The owner ada@gmail.com has 10 minutes \
///      to answer. To pass the role, the owner runs: riff owner bob@gmail.com. To keep \
///      it: riff owner --deny. With no answer, bob@gmail.com is the owner."
/// );
/// ```
pub fn asked_news(user: &str, admin: &str, owner: &str, answer: Duration) -> String {
    format!(
        "members: {user} asks for the owner role. The owner {owner} has {} to answer. \
         To pass the role, the owner runs: riff owner {admin}. To keep it: riff owner \
         --deny. With no answer, {admin} is the owner.",
        minutes(answer)
    )
}

/// The note of a request on a riff with no owner: the admin is the
/// owner at once.
///
/// ```
/// assert_eq!(
///     riff_server::owner::took_news("bob", "bob@gmail.com"),
///     "members: bob took the owner role. The riff had no owner. bob@gmail.com is the owner now."
/// );
/// ```
pub fn took_news(user: &str, admin: &str) -> String {
    format!("members: {user} took the owner role. The riff had no owner. {admin} is the owner now.")
}

/// The note of a deny.
///
/// ```
/// assert_eq!(
///     riff_server::owner::denied_news("ada", "ada@gmail.com", "bob@gmail.com"),
///     "members: ada kept the owner role. bob@gmail.com asked for it. ada@gmail.com stays the owner."
/// );
/// ```
pub fn denied_news(user: &str, owner: &str, admin: &str) -> String {
    format!("members: {user} kept the owner role. {admin} asked for it. {owner} stays the owner.")
}

/// The warning to the owner, one check before the owner is gone
/// (01M3Q546335NBTKG5BHQ27QC93).
///
/// ```
/// use riff_server::owner::{Timing, warn_news};
///
/// assert_eq!(
///     warn_news("ada@gmail.com", &Timing::default()),
///     "members: the owner ada@gmail.com was not seen at 3 checks in a row, 10 minutes \
///      apart. At the next check, in 10 minutes, the riff has no owner. To stay the \
///      owner, run a riff command, for example: riff who."
/// );
/// ```
pub fn warn_news(owner: &str, timing: &Timing) -> String {
    let every = minutes(timing.check_every);
    format!(
        "members: the owner {owner} was not seen at {} checks in a row, {every} apart. At \
         the next check, in {every}, the riff has no owner. To stay the owner, run a riff \
         command, for example: riff who.",
        timing.misses
    )
}

/// The note of a change that no person made.
///
/// ```
/// use std::time::Duration;
/// use riff_server::owner::{Timing, change_news};
/// use riff_server::token::OwnerChange;
///
/// let timing = Timing::default();
/// let granted = OwnerChange::Granted { owner: "bob@gmail.com".into(), old: "ada@gmail.com".into() };
/// assert_eq!(
///     change_news(&granted, &timing),
///     "members: the owner ada@gmail.com did not answer in 10 minutes. bob@gmail.com \
///      is the owner now. ada@gmail.com stays an admin."
/// );
/// let gone = OwnerChange::Gone { old: "ada@gmail.com".into(), owner: None };
/// assert_eq!(
///     change_news(&gone, &timing),
///     "members: the owner ada@gmail.com is gone: not seen at 4 checks in a row, \
///      10 minutes apart. The riff has no owner now. ada@gmail.com stays an admin. The \
///      riff needs a volunteer: the first admin that runs riff owner --take is the owner."
/// );
/// ```
pub fn change_news(change: &OwnerChange, timing: &Timing) -> String {
    match change {
        OwnerChange::Granted { owner, old } => format!(
            "members: the owner {old} did not answer in {}. {owner} is the owner now. \
             {old} stays an admin.",
            minutes(timing.answer)
        ),
        OwnerChange::Gone { old, owner } => {
            let gone = format!(
                "members: the owner {old} is gone: not seen at {} checks in a row, {} apart.",
                timing.misses.max(1) + 1,
                minutes(timing.check_every)
            );
            match owner {
                Some(owner) => format!(
                    "{gone} {owner} asked for the owner role first, and is the owner now. \
                     {old} stays an admin."
                ),
                None => format!(
                    "{gone} The riff has no owner now. {old} stays an admin. The riff needs a \
                     volunteer: the first admin that runs riff owner --take is the owner."
                ),
            }
        }
    }
}

/// The text of a direct message to a lead: the note, and what the lead
/// does with it.
///
/// ```
/// assert_eq!(
///     riff_server::owner::to_lead("members: x."),
///     "members: x. Show this to your user."
/// );
/// ```
pub fn to_lead(news: &str) -> String {
    format!("{news} Show this to your user.")
}
