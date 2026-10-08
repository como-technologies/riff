//! The requests to run one command outside the profile of a session
//! (#614).
//!
//! # Design
//!
//! The sandbox of a session is on, with no switch to turn it off. One
//! named command can run outside the profile, one time, after the
//! owner or an admin approves it.
//!
//! - **The ask** (01M4DA9PFR6V3K3FE1568277H3). `riff outside ask` in a
//!   session sends the command, its folder and a reason. The server
//!   keeps the request in its memory, with a new ID. A restart drops
//!   each open request: the session asks again.
//! - **The decision** (01M4DA9PJ0MJPBQRTA79CVXEA2). Only the owner or an
//!   admin approves or denies, and only with a token of a person. A
//!   session in its sandbox has only the grant of its own session, so
//!   no session approves a request, also not the session that asked.
//! - **The run** (01M4DA9PM89KP332T6BR7V0CDT). The broker of the session,
//!   outside the sandbox, takes the approved request: the command and
//!   the folder come from the server. The server gives it only once,
//!   and only to the session that asked.
//! - **The log** (01M4DA9PPFBVPHP57JZQDF4R7R). The server posts each step
//!   to the thread of the repository of the session: [`news`].
//!
//! ```mermaid
//! sequenceDiagram
//!     participant S as riff outside ask (in the sandbox)
//!     participant B as broker (outside)
//!     participant R as riff-server
//!     participant A as an admin (riff outside approve)
//!     S->>R: ask: command, folder, reason
//!     R-->>S: the ID
//!     R->>R: post: the ask wakes the lead
//!     S->>B: outside ID
//!     A->>R: approve ID (a token of a person)
//!     B->>R: take ID (each 2 seconds)
//!     R-->>B: the request, taken one time
//!     B->>B: check the folder, run the command
//!     B-->>S: the exit code
//! ```
//!
//! # Example
//!
//! ```
//! use std::time::Instant;
//! use riff_core::name::{SessionUri, Who};
//! use riff_core::wire::OutsideState;
//! use riff_server::outside::Outside;
//!
//! let me: SessionUri = "riff://mike@pangolin/acme/app?session=a6cf".parse()?;
//! let now = Instant::now();
//! let mut outside = Outside::default();
//! let asked = outside.ask(&me, vec!["sudo".into(), "true".into()], "/w", "a reason", now)?;
//! assert_eq!(asked.state, OutsideState::Asked);
//!
//! // The session itself cannot approve; a person can.
//! assert!(outside.decide(&asked.id, me.who(), true, now).is_err());
//! let mike = Who::new("mike", None)?;
//! assert_eq!(outside.decide(&asked.id, &mike, true, now)?.state, OutsideState::Approved);
//!
//! // The broker of the session takes it one time.
//! let taken = outside.take(&asked.id, me.who(), now)?;
//! assert!(taken.taken);
//! assert_eq!(taken.state, OutsideState::Ran);
//! assert!(!outside.take(&asked.id, me.who(), now)?.taken);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use riff_core::name::{SessionUri, Who};
use riff_core::wire::{OutsideRequest, OutsideState};

/// How long the server keeps a request after its ask.
pub const LIFE: Duration = Duration::from_secs(60 * 60);

/// The longest command, reason or folder, in bytes.
const MAX: usize = 4096;

/// The requests of this server, in its memory.
#[derive(Debug, Default)]
pub struct Outside {
    requests: BTreeMap<String, (OutsideRequest, Instant)>,
}

impl Outside {
    /// A new request of the session `by`. Gives the request, or why the
    /// server refuses it.
    pub fn ask(
        &mut self,
        by: &SessionUri,
        command: Vec<String>,
        cwd: &str,
        reason: &str,
        now: Instant,
    ) -> Result<OutsideRequest, String> {
        if by.who().session().is_none() {
            return Err("only a session asks to run a command outside its profile".into());
        }
        if command.first().is_none_or(|program| program.is_empty()) {
            return Err("the request names no command".into());
        }
        if reason.trim().is_empty() {
            return Err("the request needs a reason: give --reason".into());
        }
        if !cwd.starts_with('/') {
            return Err(format!("the folder {cwd:?} is not an absolute path"));
        }
        if command.iter().map(String::len).sum::<usize>() + cwd.len() + reason.len() > MAX {
            return Err(format!("the request is longer than {MAX} bytes"));
        }
        self.sweep(now);
        let request = OutsideRequest {
            id: new_id(),
            by: by.clone(),
            command,
            cwd: cwd.to_owned(),
            reason: reason.trim().to_owned(),
            state: OutsideState::Asked,
            decided_by: None,
            taken: false,
        };
        self.requests
            .insert(request.id.clone(), (request.clone(), now));
        Ok(request)
    }

    /// `who` approves or denies the request `id`. The caller checks that
    /// `who` is the owner or an admin. A token of a session never
    /// decides (01M4DA9PJ0MJPBQRTA79CVXEA2).
    pub fn decide(
        &mut self,
        id: &str,
        who: &Who,
        approve: bool,
        now: Instant,
    ) -> Result<OutsideRequest, String> {
        if who.session().is_some() {
            return Err(
                "a session cannot approve or deny a request: an admin runs riff outside in a \
                 terminal"
                    .into(),
            );
        }
        self.sweep(now);
        let request = self.get(id)?;
        if request.state != OutsideState::Asked {
            return Err(format!("the request {id} is {}, not asked", name(request.state)));
        }
        request.state = if approve {
            OutsideState::Approved
        } else {
            OutsideState::Denied
        };
        request.decided_by = Some(who.user().to_owned());
        Ok(request.clone())
    }

    /// The broker of the session `who` takes the request `id`. An
    /// approved request is marked as run, and the reply has `taken`
    /// true: only this one time (01M4DA9PM89KP332T6BR7V0CDT).
    pub fn take(&mut self, id: &str, who: &Who, now: Instant) -> Result<OutsideRequest, String> {
        self.sweep(now);
        let request = self.get(id)?;
        if request.by.who() != who {
            return Err(format!("the request {id} is not of this session"));
        }
        let mut reply = request.clone();
        if request.state == OutsideState::Approved {
            request.state = OutsideState::Ran;
            reply.state = OutsideState::Ran;
            reply.taken = true;
        }
        Ok(reply)
    }

    /// The requests that the server keeps, the oldest first.
    pub fn list(&mut self, now: Instant) -> Vec<OutsideRequest> {
        self.sweep(now);
        let mut all: Vec<_> = self.requests.values().cloned().collect();
        all.sort_by_key(|(_, at)| *at);
        all.into_iter().map(|(request, _)| request).collect()
    }

    fn get(&mut self, id: &str) -> Result<&mut OutsideRequest, String> {
        self.requests
            .get_mut(id)
            .map(|(request, _)| request)
            .ok_or_else(|| format!("the server has no request {id}: it is old, or the server started again"))
    }

    /// Forgets each request older than [`LIFE`].
    fn sweep(&mut self, now: Instant) {
        self.requests
            .retain(|_, (_, at)| now.saturating_duration_since(*at) < LIFE);
    }
}

/// A new ID: 8 random hex digits.
fn new_id() -> String {
    let mut bytes = [0u8; 4];
    getrandom::fill(&mut bytes).expect("the OS gives random bytes");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The word of a state.
fn name(state: OutsideState) -> &'static str {
    match state {
        OutsideState::Asked => "asked",
        OutsideState::Approved => "approved",
        OutsideState::Denied => "denied",
        OutsideState::Ran => "run",
    }
}

/// The text of the post of the server about `request`, after its last
/// step (01M4DA9PPFBVPHP57JZQDF4R7R).
///
/// ```
/// use riff_core::wire::{OutsideRequest, OutsideState};
/// use riff_server::outside::news;
///
/// let mut request = OutsideRequest {
///     id: "7f3a9c21".into(),
///     by: "riff://mike@pangolin/acme/app?session=a6cf".parse().unwrap(),
///     command: vec!["sudo".into(), "true".into()],
///     cwd: "/w".into(),
///     reason: "a test".into(),
///     state: OutsideState::Asked,
///     decided_by: None,
///     taken: false,
/// };
/// let ask = news(&request);
/// assert!(ask.contains("outside 7f3a9c21: mike/a6cf asks to run `sudo true` in /w"), "{ask}");
/// assert!(ask.contains("reason: a test"), "{ask}");
/// assert!(ask.contains("riff outside approve 7f3a9c21"), "{ask}");
/// request.state = OutsideState::Ran;
/// request.decided_by = Some("dan".into());
/// let ran = news(&request);
/// assert!(ran.contains("the broker runs it one time"), "{ran}");
/// assert!(ran.contains("approved by dan"), "{ran}");
/// ```
pub fn news(request: &OutsideRequest) -> String {
    let head = format!(
        "outside {}: {} asks to run `{}` in {} (reason: {})",
        request.id,
        request.by.who(),
        request.command.join(" "),
        request.cwd,
        request.reason
    );
    let by = request.decided_by.as_deref().unwrap_or("?");
    match request.state {
        OutsideState::Asked => format!(
            "{head}. Only the owner or an admin decides, in a terminal: riff outside approve {id}, \
             or riff outside deny {id}.",
            id = request.id
        ),
        OutsideState::Approved => format!("{head}. It is approved by {by}."),
        OutsideState::Denied => format!("{head}. It is denied by {by}. It does not run."),
        OutsideState::Ran => {
            format!("{head}. It is approved by {by}, and the broker runs it one time now.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str) -> SessionUri {
        format!("riff://mike@pangolin/acme/app?session={id}")
            .parse()
            .unwrap()
    }

    fn ask(outside: &mut Outside, by: &SessionUri, now: Instant) -> OutsideRequest {
        outside
            .ask(by, vec!["true".into()], "/w", "a test", now)
            .unwrap()
    }

    #[test]
    fn a_session_cannot_approve_its_own_request_or_another() {
        let (me, other) = (session("a6cf"), session("b7d0"));
        let now = Instant::now();
        let mut outside = Outside::default();
        let asked = ask(&mut outside, &me, now);
        for who in [me.who(), other.who()] {
            let refused = outside.decide(&asked.id, who, true, now).unwrap_err();
            assert!(refused.contains("a session cannot approve"), "{refused}");
        }
        assert_eq!(outside.list(now)[0].state, OutsideState::Asked);
        assert!(!outside.take(&asked.id, me.who(), now).unwrap().taken);
    }

    #[test]
    fn only_the_session_that_asked_takes_it_and_only_once() {
        let (me, other) = (session("a6cf"), session("b7d0"));
        let now = Instant::now();
        let mut outside = Outside::default();
        let asked = ask(&mut outside, &me, now);
        let dan = Who::new("dan", None).unwrap();
        outside.decide(&asked.id, &dan, true, now).unwrap();
        assert!(outside.take(&asked.id, other.who(), now).is_err());
        assert!(outside.take(&asked.id, me.who(), now).unwrap().taken);
        assert!(!outside.take(&asked.id, me.who(), now).unwrap().taken);
        // A request that ran gets no second decision.
        assert!(outside.decide(&asked.id, &dan, true, now).is_err());
    }

    #[test]
    fn a_denied_request_never_runs() {
        let me = session("a6cf");
        let now = Instant::now();
        let mut outside = Outside::default();
        let asked = ask(&mut outside, &me, now);
        let dan = Who::new("dan", None).unwrap();
        let denied = outside.decide(&asked.id, &dan, false, now).unwrap();
        assert_eq!(denied.state, OutsideState::Denied);
        assert_eq!(denied.decided_by.as_deref(), Some("dan"));
        let took = outside.take(&asked.id, me.who(), now).unwrap();
        assert!(!took.taken);
        assert_eq!(took.state, OutsideState::Denied);
    }

    #[test]
    fn a_request_needs_a_session_a_command_a_reason_and_a_full_path() {
        let now = Instant::now();
        let mut outside = Outside::default();
        let person: SessionUri = "riff://mike@pangolin/acme/app".parse().unwrap();
        let me = session("a6cf");
        let cmd = || vec!["true".to_owned()];
        assert!(outside.ask(&person, cmd(), "/w", "r", now).is_err());
        assert!(outside.ask(&me, vec![], "/w", "r", now).is_err());
        assert!(outside.ask(&me, cmd(), "/w", " ", now).is_err());
        assert!(outside.ask(&me, cmd(), "w", "r", now).is_err());
        assert!(outside.ask(&me, cmd(), "/w", &"r".repeat(MAX), now).is_err());
        assert!(outside.list(now).is_empty());
    }

    #[test]
    fn the_server_forgets_a_request_after_its_life() {
        let me = session("a6cf");
        let now = Instant::now();
        let mut outside = Outside::default();
        let asked = ask(&mut outside, &me, now);
        let later = now + LIFE;
        assert!(outside.list(later).is_empty());
        let gone = outside.take(&asked.id, me.who(), later).unwrap_err();
        assert!(gone.contains("no request"), "{gone}");
    }
}
