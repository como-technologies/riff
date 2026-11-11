//! A proof of presence on riff-server (PROOF_ASK): the page of the
//! proof in the browser, the check of its assertion, and the use of a
//! proof by the call of an action.
//!
//! # Design
//!
//! A proof is a fresh signature of a key of the person, for one action.
//! It runs on a page of riff-server in the browser, as the add of a key
//! does ([`crate::devices`]), so each kind of authenticator works, also
//! a phone over a QR code:
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff
//!     participant S as riff-server
//!     participant B as browser
//!     participant A as authenticator
//!     R->>S: the action, with no proof
//!     S-->>R: 403 needs_proof (the policy is required)
//!     R->>S: POST /v1/proof: the action (a person token)
//!     S-->>R: the URL of the page, with the ticket in its fragment
//!     R->>B: open the page
//!     B->>S: POST /proof/options, with the ticket
//!     S-->>B: a challenge that names the action and the person, and the keys of the person
//!     B->>A: navigator.credentials.get: a touch
//!     A-->>B: the signature
//!     B->>S: POST /proof/reply, with the ticket
//!     S->>S: the check of the signature, then the command prove: proved
//!     R->>S: POST /v1/proof/wait, each second, until the proof ends
//!     R->>S: the action again, with the header riff-proof: TICKET
//!     S->>S: the proof is for this action and this person: the action, and the proof is used
//! ```
//!
//! - [`Proofs`] keeps each proof in memory: the person, the action, the
//!   challenge and the proof of the token of the start. A proof is good
//!   for [`LIFE`] from its start, for one reply of the page, and for one
//!   use. A restart of riff-server loses a proof: riff asks for a new
//!   one.
//! - The challenge is the hash of 32 random bytes, the USER and the
//!   action. So it names the action and the person, and no two proofs
//!   have the same one.
//! - The ticket is in the fragment of the URL of the page, so no request
//!   line holds it. The page and its two calls need no riff token: the
//!   ticket is their proof.
//! - Only a person token starts a proof and waits for it: riff-server
//!   refuses a session token with 403 (PROOF_ASK). A session never gives
//!   a proof.
//! - [`crate::webauthn::assert`] checks the assertion. Then
//!   [`crate::engine::Engine::prove`] sends the command `prove` with the
//!   proof of the token of the start: its `handle` checks the counter,
//!   and its record `proved` names the key and the action (PROOF_LOG).
//! - The call of an action carries the ticket in the header
//!   [`PROOF_HEADER`]. The engine takes the proof under the lock of the
//!   state, and uses it only when the command passes its check
//!   (PROOF_REFUSE). `POST /v1/proof/use` does the same for a step that
//!   riff-server does not do itself (PROOF_GATE).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::{Extension, Json, extract::State as AxumState};
use riff_core::proof::{Action, LIFE_SECS, PROOF_HEADER};
use riff_core::wire::{
    CALL_HEADER, ProofOptions, ProofReplied, ProofReply, ProofStart, ProofStarted, ProofState,
    ProofTicket, ProofUsed, ProofWait, UseProof,
};
use riff_state::state::{Code, Prove, Refused};
use sha2::{Digest, Sha256};

use crate::auth::SignedIn;
use crate::engine::Failed;
use crate::webauthn::{self, Party};

/// The path of the page of a proof.
pub const PAGE_PATH: &str = "/proof";

/// How long a proof is good, from its start (PROOF_ASK).
pub const LIFE: Duration = Duration::from_secs(LIFE_SECS);

/// The refusal of a call of a session token (PROOF_ASK).
pub const PERSON_ONLY: &str = "a session never gives a proof of presence: an authenticator \
    is a device of your person. Your person runs the command in a terminal.";

/// The refusal of a person with no key.
pub const NO_KEY: &str = "you have no authenticator, so you cannot give a proof of presence. \
    Add one first: riff device add --key NAME";

/// One proof that riff-server keeps.
#[derive(Clone, Debug)]
struct Proof {
    /// The USER of the person.
    user: String,
    action: Action,
    challenge: [u8; 32],
    /// The proof of the token of the start: the command `prove` acts
    /// with it.
    start: SignedIn,
    made: Instant,
    step: Step,
}

/// Where a proof is.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    /// The page has not sent an assertion.
    Waiting,
    /// riff-server checks the assertion of the page.
    Checking,
    /// The key `name` signed: the proof waits for its use.
    Proved { name: String },
    /// A call used the proof. The call ID of that call, when it had one:
    /// a second try of the same call gets the proof again.
    Used { call: Option<String> },
    /// riff-server refused the assertion.
    Failed { reason: String },
}

/// The proofs that riff-server keeps in memory, by their ticket
/// (PROOF_ASK). Each is good for its life, for one reply of the page and
/// for one use.
///
/// ```
/// use std::time::{Duration, Instant};
/// use riff_core::name::Who;
/// use riff_core::proof::Action;
/// use riff_core::wire::ProofState;
/// use riff_server::auth::SignedIn;
/// use riff_server::proofs::{LIFE, Proofs};
///
/// let start = SignedIn { who: Who::new("mike", None)?, jkt: "k".into(), started: 0 };
/// let now = Instant::now();
/// let mut proofs = Proofs::default();
/// let ticket = proofs.start(&start, Action::PauseRiff, now);
/// assert_eq!(proofs.state(&ticket, "mike", now), Some(ProofState::Waiting));
/// // Another person learns nothing of the proof.
/// assert_eq!(proofs.state(&ticket, "bob", now), None);
/// // No proof is good before the page sent its assertion.
/// assert!(proofs.resolve(&ticket, "mike", None, now).is_err());
///
/// let taken = proofs.take(&ticket, now)?;
/// assert_eq!(taken.action, Action::PauseRiff);
/// assert!(proofs.take(&ticket, now).is_err());
/// proofs.proved(&ticket, "yubikey");
/// assert_eq!(proofs.resolve(&ticket, "mike", None, now)?, Action::PauseRiff);
/// assert!(proofs.resolve(&ticket, "bob", None, now).is_err());
///
/// // One use.
/// proofs.spend(&ticket, Some("c1"));
/// assert!(proofs.resolve(&ticket, "mike", None, now).is_err());
/// // A second try of the same call gets it again.
/// assert_eq!(proofs.resolve(&ticket, "mike", Some("c1"), now)?, Action::PauseRiff);
///
/// // A proof is old after its life.
/// let old = proofs.start(&start, Action::ResumeRiff, now);
/// proofs.take(&old, now)?;
/// proofs.proved(&old, "yubikey");
/// let later = now + LIFE + Duration::from_secs(1);
/// assert!(proofs.resolve(&old, "mike", None, later).unwrap_err().contains("old"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct Proofs {
    life: Duration,
    proofs: HashMap<String, Proof>,
}

impl Default for Proofs {
    fn default() -> Self {
        Proofs::new(LIFE)
    }
}

/// A proof that the assertion of the page took: what the check and the
/// command need.
#[derive(Clone, Debug)]
pub struct Taken {
    pub user: String,
    pub action: Action,
    pub challenge: [u8; 32],
    pub start: SignedIn,
}

impl Proofs {
    /// The proofs, each good for `life`.
    pub fn new(life: Duration) -> Proofs {
        Proofs {
            life,
            proofs: HashMap::new(),
        }
    }

    /// A new proof of `action` for the person of `start`: gives its
    /// ticket.
    pub fn start(&mut self, start: &SignedIn, action: Action, now: Instant) -> String {
        self.forget(now);
        let ticket = crate::token::random_token();
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).expect("the OS gives random bytes");
        let user = start.who.user().to_owned();
        let mut hash = Sha256::new();
        hash.update(seed);
        hash.update(user.as_bytes());
        hash.update([0]);
        hash.update(serde_json::to_vec(&action).expect("an action is JSON"));
        let proof = Proof {
            user,
            action,
            challenge: hash.finalize().into(),
            start: start.clone(),
            made: now,
            step: Step::Waiting,
        };
        self.proofs.insert(ticket.clone(), proof);
        ticket
    }

    fn old(&self, proof: &Proof, now: Instant) -> bool {
        now.duration_since(proof.made) > self.life
    }

    /// The proof of `ticket` while it waits for its assertion.
    fn waiting(&self, ticket: &str, now: Instant) -> Result<&Proof, String> {
        let Some(proof) = self.proofs.get(ticket) else {
            return Err(UNKNOWN.into());
        };
        if proof.step != Step::Waiting {
            return Err("this proof has its signature already: run the command again".into());
        }
        if self.old(proof, now) {
            return Err(OLD.into());
        }
        Ok(proof)
    }

    /// The USER, the action and the challenge of the proof of `ticket`,
    /// while it waits for its assertion.
    pub fn options(
        &self,
        ticket: &str,
        now: Instant,
    ) -> Result<(String, Action, [u8; 32]), String> {
        let proof = self.waiting(ticket, now)?;
        Ok((proof.user.clone(), proof.action.clone(), proof.challenge))
    }

    /// Takes the proof of `ticket` for its one assertion.
    pub fn take(&mut self, ticket: &str, now: Instant) -> Result<Taken, String> {
        let proof = self.waiting(ticket, now)?;
        let taken = Taken {
            user: proof.user.clone(),
            action: proof.action.clone(),
            challenge: proof.challenge,
            start: proof.start.clone(),
        };
        self.set(ticket, Step::Checking);
        Ok(taken)
    }

    /// The key `name` signed the proof of `ticket`.
    pub fn proved(&mut self, ticket: &str, name: &str) {
        self.set(
            ticket,
            Step::Proved {
                name: name.to_owned(),
            },
        );
    }

    /// riff-server refused the assertion of the proof of `ticket`.
    pub fn failed(&mut self, ticket: &str, reason: &str) {
        self.set(
            ticket,
            Step::Failed {
                reason: reason.to_owned(),
            },
        );
    }

    fn set(&mut self, ticket: &str, step: Step) {
        if let Some(proof) = self.proofs.get_mut(ticket) {
            proof.step = step;
        }
    }

    /// Where the proof of `ticket` of `user` is. `None` when riff-server
    /// knows no such proof of `user`.
    pub fn state(&self, ticket: &str, user: &str, now: Instant) -> Option<ProofState> {
        let proof = self.proofs.get(ticket).filter(|proof| proof.user == user)?;
        Some(match &proof.step {
            Step::Waiting if self.old(proof, now) => ProofState::Failed { reason: OLD.into() },
            Step::Waiting | Step::Checking => ProofState::Waiting,
            Step::Proved { name } => ProofState::Proved { name: name.clone() },
            Step::Used { .. } => ProofState::Failed {
                reason: USED.into(),
            },
            Step::Failed { reason } => ProofState::Failed {
                reason: reason.clone(),
            },
        })
    }

    /// The action of the proof of `ticket` for a call of `user` with the
    /// call ID `call` (PROOF_REFUSE): a proof that the key of the person
    /// signed, in its life, and not used. A second try of the call that
    /// used it gets it again. Else why riff-server does not take it.
    pub fn resolve(
        &self,
        ticket: &str,
        user: &str,
        call: Option<&str>,
        now: Instant,
    ) -> Result<Action, String> {
        let Some(proof) = self.proofs.get(ticket).filter(|proof| proof.user == user) else {
            return Err(UNKNOWN.into());
        };
        match &proof.step {
            Step::Used { call: used } if used.is_some() && used.as_deref() == call => {}
            Step::Used { .. } => return Err(USED.into()),
            Step::Waiting | Step::Checking => {
                return Err("this proof has no good signature yet: give it in the page".into());
            }
            Step::Failed { reason } => {
                return Err(format!("riff-server refused this proof: {reason}"));
            }
            Step::Proved { .. } if self.old(proof, now) => return Err(OLD.into()),
            Step::Proved { .. } => {}
        }
        Ok(proof.action.clone())
    }

    /// The call with the call ID `call` used the proof of `ticket`.
    pub fn spend(&mut self, ticket: &str, call: Option<&str>) {
        self.set(
            ticket,
            Step::Used {
                call: call.map(str::to_owned),
            },
        );
    }

    /// Forgets each proof older than two lives.
    fn forget(&mut self, now: Instant) {
        let most = 2 * self.life;
        self.proofs
            .retain(|_, proof| now.duration_since(proof.made) <= most);
    }
}

/// The refusal of a ticket that riff-server does not know.
const UNKNOWN: &str = "riff-server knows no such proof of yours: run the command again";

/// The refusal of a proof whose time ended.
const OLD: &str = "the proof is old: its time ended. Run the command again for a new one";

/// The refusal of a proof that a call used.
const USED: &str = "a call used this proof already: each proof is good for one action";

/// The ticket of the header [`PROOF_HEADER`] of a request, when it has
/// one.
pub fn ticket_of(headers: &HeaderMap) -> Result<Option<String>, Failed> {
    let Some(value) = headers.get(PROOF_HEADER) else {
        return Ok(None);
    };
    match value.to_str() {
        Ok(ticket) if !ticket.is_empty() && ticket.len() <= 128 => Ok(Some(ticket.to_owned())),
        _ => Err(Refused::new(
            Code::BadProof,
            format!("the header {PROOF_HEADER} needs the ticket of a proof"),
        )
        .into()),
    }
}

/// The call ID of the header `riff-call`, for a second try of a call.
fn call_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get(CALL_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// Refuses a session token (PROOF_ASK).
fn person_only(signed_in: &SignedIn) -> Result<(), (StatusCode, String)> {
    match signed_in.who.session() {
        Some(_) => Err((StatusCode::FORBIDDEN, PERSON_ONLY.to_owned())),
        None => Ok(()),
    }
}

/// The relying party of `s`.
fn party(s: &crate::Server) -> Party {
    Party::of(&s.config.public_url)
}

/// `POST /v1/proof`: a new proof of the action, and the URL of its page
/// (PROOF_ASK). A person with no key gets the way to add one.
pub(crate) async fn start(
    AxumState(s): AxumState<crate::Shared>,
    Extension(signed_in): Extension<SignedIn>,
    Json(ProofStart { action }): Json<ProofStart>,
) -> crate::Reply<ProofStarted> {
    person_only(&signed_in)?;
    let user = signed_in.who.user().to_owned();
    if s.engine.read(|state| state.credentials(&user)).is_empty() {
        return Err((StatusCode::FORBIDDEN, NO_KEY.to_owned()));
    }
    let text = action.text();
    let ticket = s
        .engine
        .proofs(|proofs| proofs.start(&signed_in, action, Instant::now()));
    tracing::info!(%user, action = %text, "a proof of presence: a page for the proof");
    Ok(Json(ProofStarted {
        url: format!("{}{PAGE_PATH}#{ticket}", party(&s).origin),
        ticket,
    }))
}

/// `POST /v1/proof/wait`: where the proof of the ticket is. Only the
/// person of the proof can ask.
pub(crate) async fn wait(
    AxumState(s): AxumState<crate::Shared>,
    Extension(signed_in): Extension<SignedIn>,
    Json(ProofWait { ticket }): Json<ProofWait>,
) -> crate::Reply<ProofState> {
    person_only(&signed_in)?;
    let user = signed_in.who.user();
    s.engine
        .proofs(|proofs| proofs.state(&ticket, user, Instant::now()))
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, UNKNOWN.to_owned()))
}

/// `POST /v1/proof/use`: the check of riff-server before a step that it
/// does not do itself (PROOF_GATE): the policy, and the proof of the
/// header, which it uses.
pub(crate) async fn use_proof(
    AxumState(s): AxumState<crate::Shared>,
    signed_in: Option<Extension<SignedIn>>,
    headers: HeaderMap,
    Json(UseProof { action }): Json<UseProof>,
) -> Result<Json<ProofUsed>, Failed> {
    let ticket = ticket_of(&headers)?;
    let call = call_of(&headers);
    let signed_in = signed_in.map(|Extension(signed_in)| signed_in);
    let proved = s.engine.use_step(
        signed_in.as_ref(),
        ticket.as_deref(),
        call.as_deref(),
        &action,
    )?;
    let user = signed_in
        .as_ref()
        .map_or("nobody", |signed_in| signed_in.who.user());
    tracing::info!(%user, action = %action.text(), proved, "a step that riff-server does not do itself");
    Ok(Json(ProofUsed { proved }))
}

/// `GET /proof`: the page of the proof. It reads the ticket from its
/// fragment.
pub(crate) async fn page() -> Response {
    (
        StatusCode::OK,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Html(PAGE),
    )
        .into_response()
}

/// `POST /proof/options`: the options of the assertion of the proof of
/// the ticket (PROOF_ASK).
pub(crate) async fn options(
    AxumState(s): AxumState<crate::Shared>,
    Json(ProofTicket { ticket }): Json<ProofTicket>,
) -> crate::Reply<ProofOptions> {
    let (user, action, challenge) = s
        .engine
        .proofs(|proofs| proofs.options(&ticket, Instant::now()))
        .map_err(|why| (StatusCode::BAD_REQUEST, why))?;
    let credentials = s.engine.read(|state| state.credentials(&user));
    Ok(Json(webauthn::proof_options(
        &party(&s),
        &user,
        &challenge,
        &credentials,
        &action.text(),
    )))
}

/// `POST /proof/reply`: the check of the assertion, and the command
/// `prove` (PROOF_CHECK, PROOF_LOG). Each assertion ends the wait of
/// the proof: proved, or refused with the reason.
pub(crate) async fn reply(
    AxumState(s): AxumState<crate::Shared>,
    Json(reply): Json<ProofReply>,
) -> crate::Reply<ProofReplied> {
    let ticket = reply.ticket.clone();
    let taken = s
        .engine
        .proofs(|proofs| proofs.take(&ticket, Instant::now()))
        .map_err(|why| (StatusCode::BAD_REQUEST, why))?;
    let user = taken.user.clone();
    let text = taken.action.text();
    let failed = |status: StatusCode, reason: String| {
        tracing::warn!(%user, action = %text, "a proof of presence: riff refused the proof: {reason}");
        s.engine.proofs(|proofs| proofs.failed(&ticket, &reason));
        (status, reason)
    };
    let keys = s.engine.read(|state| state.keys(&user));
    let asserted = match webauthn::assert(&party(&s), &taken.challenge, &reply, &keys) {
        Ok(asserted) => asserted,
        Err(refused) => return Err(failed(StatusCode::BAD_REQUEST, refused.0)),
    };
    let prove = Prove {
        name: asserted.name.clone(),
        action: taken.action.clone(),
        counter: asserted.counter,
    };
    match s.engine.prove(&taken.start, prove).await {
        Ok(()) => {
            tracing::info!(%user, action = %text, name = %asserted.name, "a proof of presence: riff checked the proof");
            s.engine
                .proofs(|proofs| proofs.proved(&ticket, &asserted.name));
            Ok(Json(ProofReplied {
                text: format!(
                    "riff checked your proof with the key {}. Go back to your terminal.",
                    asserted.name
                ),
            }))
        }
        Err(e) => Err(failed(e.status(), e.text())),
    }
}

/// The page of a proof. It reads the ticket from its fragment, asks
/// riff-server for the options, shows the action, and gives the options
/// to `navigator.credentials.get` when the person presses the button: a
/// browser asks for a gesture first. Then it sends the assertion.
const PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>riff: a proof of presence</title></head>
<body>
<h1>A proof of presence for riff</h1>
<p id="what">riff reads the proof.</p>
<p><button id="prove" hidden>Give the proof</button></p>
<p id="said"></p>
<script>
"use strict";
const ticket = location.hash.slice(1);
history.replaceState(null, "", location.pathname);
const what = document.getElementById("what");
const button = document.getElementById("prove");
const said = document.getElementById("said");
const bytes = (text) => Uint8Array.from(atob(text.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0));
const text = (buffer) => btoa(String.fromCharCode(...new Uint8Array(buffer))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
async function call(path, body) {
  const reply = await fetch(path, {method: "POST", headers: {"content-type": "application/json"}, body: JSON.stringify(body)});
  if (!reply.ok) throw new Error(await reply.text());
  return reply.json();
}
async function prove(options) {
  button.disabled = true;
  said.textContent = "Touch your authenticator, or scan the QR code with your phone.";
  let credential;
  try {
    credential = await navigator.credentials.get({publicKey: {
      challenge: bytes(options.challenge),
      timeout: options.timeout,
      rpId: options.rpId,
      allowCredentials: options.allowCredentials.map((c) => ({...c, id: bytes(c.id)})),
      userVerification: options.userVerification,
    }});
  } catch (e) {
    said.textContent = "The browser gave no signature: " + e.message + " Press the button to try again.";
    button.disabled = false;
    return;
  }
  try {
    const done = await call("/proof/reply", {
      ticket,
      raw_id: text(credential.rawId),
      client_data: text(credential.response.clientDataJSON),
      authenticator_data: text(credential.response.authenticatorData),
      signature: text(credential.response.signature),
    });
    said.textContent = done.text;
  } catch (e) {
    said.textContent = "riff did not take the proof: " + e.message;
  }
}
(async () => {
  if (!window.PublicKeyCredential) {
    what.textContent = "This browser cannot give a proof here. It needs WebAuthn, on https or on localhost.";
    return;
  }
  let options;
  try {
    options = await call("/proof/options", {ticket});
  } catch (e) {
    what.textContent = e.message;
    return;
  }
  what.textContent = options.user + " at the riff of " + options.rpId + ": " + options.action + ". Press the button only if you asked for this.";
  button.hidden = false;
  button.onclick = () => prove(options);
})();
</script>
</body></html>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use riff_core::name::Who;

    fn start(user: &str) -> SignedIn {
        SignedIn {
            who: Who::new(user, None).unwrap(),
            jkt: "k".into(),
            started: 0,
        }
    }

    #[test]
    fn a_session_token_is_refused() {
        assert!(person_only(&start("mike")).is_ok());
        let session = SignedIn {
            who: Who::new("mike", Some("a1")).unwrap(),
            ..start("mike")
        };
        let (status, text) = person_only(&session).unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(text.contains("a session never gives"), "{text}");
    }

    #[test]
    fn each_proof_has_its_own_ticket_and_challenge() {
        let now = Instant::now();
        let mut proofs = Proofs::default();
        let one = proofs.start(&start("mike"), Action::PauseRiff, now);
        let two = proofs.start(&start("mike"), Action::PauseRiff, now);
        assert_ne!(one, two);
        let (_, _, first) = proofs.options(&one, now).unwrap();
        let (_, _, second) = proofs.options(&two, now).unwrap();
        assert_ne!(first, second);
        assert_eq!(proofs.options("made-up", now).unwrap_err(), UNKNOWN);
    }

    #[test]
    fn a_refused_proof_is_good_for_nothing() {
        let now = Instant::now();
        let mut proofs = Proofs::default();
        let ticket = proofs.start(&start("mike"), Action::Test, now);
        proofs.take(&ticket, now).unwrap();
        proofs.failed(&ticket, "a bad signature");
        let why = proofs.resolve(&ticket, "mike", None, now).unwrap_err();
        assert!(why.contains("a bad signature"), "{why}");
        assert_eq!(
            proofs.state(&ticket, "mike", now),
            Some(ProofState::Failed {
                reason: "a bad signature".into()
            })
        );
    }

    #[test]
    fn a_used_proof_says_so_and_an_old_one_is_forgotten() {
        let now = Instant::now();
        let mut proofs = Proofs::default();
        let ticket = proofs.start(&start("mike"), Action::Test, now);
        proofs.take(&ticket, now).unwrap();
        proofs.proved(&ticket, "yubikey");
        proofs.spend(&ticket, None);
        assert_eq!(
            proofs.resolve(&ticket, "mike", None, now).unwrap_err(),
            USED
        );
        // With no call ID, no second try gets it.
        assert!(proofs.resolve(&ticket, "mike", Some("c1"), now).is_err());
        let much_later = now + 2 * LIFE + Duration::from_secs(1);
        proofs.start(&start("mike"), Action::Test, much_later);
        assert_eq!(proofs.state(&ticket, "mike", much_later), None);
    }

    #[test]
    fn the_challenge_names_the_person_and_the_action() {
        // Two proofs never share a challenge, and the page shows the
        // action and the person.
        assert!(PAGE.contains("options.action"));
        assert!(PAGE.contains("options.user"));
        assert!(PAGE.contains("navigator.credentials.get"));
        assert!(PAGE.contains("location.hash"));
        assert!(PAGE.contains("history.replaceState"));
        for path in [
            <ProofTicket as riff_core::wire::Call>::PATH,
            <ProofReply as riff_core::wire::Call>::PATH,
        ] {
            assert!(PAGE.contains(&format!("\"{path}\"")), "{path}");
        }
    }

    #[test]
    fn a_header_with_no_ticket_is_refused() {
        let mut headers = HeaderMap::new();
        assert_eq!(ticket_of(&headers).unwrap(), None);
        headers.insert(PROOF_HEADER, "T1".parse().unwrap());
        assert_eq!(ticket_of(&headers).unwrap().as_deref(), Some("T1"));
        headers.insert(PROOF_HEADER, "".parse().unwrap());
        assert!(ticket_of(&headers).is_err());
    }
}
