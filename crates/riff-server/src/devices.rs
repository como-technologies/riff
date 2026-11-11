//! The devices of a person on riff-server: the add of a key in the
//! browser, the list and the removal (01M75NGT9Z0NSZTG97TBAMP7RS).
//!
//! # Design
//!
//! A key is a FIDO2 / WebAuthn authenticator of the person. The add runs
//! on a page of riff-server in the browser, as `riff login` does, so each
//! kind of authenticator works, also a phone over a QR code
//! (01M75NH2RSEJ3K618M17PCH3ZZ):
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff device add --key NAME
//!     participant S as riff-server
//!     participant B as browser
//!     participant A as authenticator
//!     R->>S: POST /v1/device/key (a person token)
//!     S-->>R: the URL of the page, with the ticket in its fragment
//!     R->>B: open the page
//!     B->>S: GET /device/key
//!     B->>S: POST /device/key/options, with the ticket
//!     S-->>B: the options: a challenge, the algorithms, the keys of the person
//!     B->>A: navigator.credentials.create: a touch
//!     A-->>B: the new credential
//!     B->>S: POST /device/key/reply, with the ticket
//!     S->>S: the check of the reply, then the command add_key: key_added
//!     S-->>B: riff added the key NAME
//!     R->>S: POST /v1/device/key/wait, each second, until the add ends
//! ```
//!
//! - [`Adds`] keeps each add in memory: the person, the name, the
//!   challenge and the proof of the token of the start. An add is good
//!   for [`ADD_LIFE`] and for one reply. A restart of riff-server loses
//!   an add that waits: the person runs `riff device add` again.
//! - The ticket is in the fragment of the URL of the page, so no
//!   request line holds it. The page reads it, and sends it in the body
//!   of each of its two calls. The page and its calls need no riff
//!   token: the ticket is their proof.
//! - Only a person token starts an add, lists the devices and removes
//!   one: riff-server refuses a session token with 403
//!   (01M75NH30PNATYN33MXZQMVBW8). The routes are in the group of the
//!   people, so each needs a token, also in a riff that does not
//!   require a sign-in.
//! - The start checks the name as `add_key` does
//!   (01M75NH33CVGBVASSACBPXRR2G), so a bad name fails before the
//!   browser opens.
//! - [`crate::webauthn::check`] checks the reply. Then
//!   [`crate::engine::Engine::add_key`] sends the command `add_key` with
//!   the proof of the token of the start: a sign-in that ended since
//!   the start sends no command. Its `handle` checks the name and the
//!   credential again on the pending copy.
//! - The user handle of the options is the same for each key of a
//!   person: a hash of the ID of the riff and the USER.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::{Extension, Json, extract::State as AxumState};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use riff_core::proof::Action;
use riff_core::wire::{
    Devices, DevicesReply, KeyOptions, KeyReplied, KeyReply, KeyStart, KeyStarted, KeyState,
    KeyTicket, KeyWait,
};
use riff_state::state::AddKey;
use sha2::{Digest, Sha256};

use crate::auth::SignedIn;
use crate::engine::Failed;
use crate::webauthn::{self, Party};

/// The path of the page of the add of a key.
pub const PAGE_PATH: &str = "/device/key";

/// How long an add is good (01M75NH2RSEJ3K618M17PCH3ZZ).
pub const ADD_LIFE: Duration = Duration::from_secs(5 * 60);

/// The refusal of a call of a session token (01M75NH30PNATYN33MXZQMVBW8).
pub const PERSON_ONLY: &str = "a session never adds, lists or removes a device: an \
    authenticator is a device of your person. Your person runs riff device in a terminal.";

/// One add of a key that riff-server keeps.
#[derive(Clone, Debug)]
struct Add {
    /// The USER of the person.
    user: String,
    name: String,
    challenge: [u8; 32],
    /// The proof of the token of the start: the command `add_key` acts
    /// with it.
    proof: SignedIn,
    made: Instant,
    state: Step,
}

/// Where an add is.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    /// The page has not sent a reply.
    Waiting,
    /// riff-server checks the reply of the page.
    Checking,
    /// The add ended: added or failed.
    Ended(KeyState),
}

/// The adds of keys that riff-server keeps in memory, by their ticket
/// (01M75NH2RSEJ3K618M17PCH3ZZ). Each is good for its life and for one
/// reply.
///
/// ```
/// use std::time::{Duration, Instant};
/// use riff_core::name::Who;
/// use riff_core::wire::KeyState;
/// use riff_server::auth::SignedIn;
/// use riff_server::devices::{ADD_LIFE, Adds};
///
/// let proof = SignedIn { who: Who::new("mike", None)?, jkt: "k".into(), started: 0 };
/// let now = Instant::now();
/// let mut adds = Adds::default();
/// let ticket = adds.start(&proof, "yubikey", now);
/// assert_eq!(adds.state(&ticket, "mike", now), Some(KeyState::Waiting));
/// // Another person learns nothing of the add.
/// assert_eq!(adds.state(&ticket, "bob", now), None);
/// let (user, name, _) = adds.options(&ticket, now)?;
/// assert_eq!((user.as_str(), name.as_str()), ("mike", "yubikey"));
///
/// // One reply: a second one is refused.
/// let taken = adds.take(&ticket, now)?;
/// assert_eq!(taken.name, "yubikey");
/// assert!(adds.take(&ticket, now).is_err());
/// adds.end(&ticket, KeyState::Added { name: "yubikey".into() });
/// assert_eq!(adds.state(&ticket, "mike", now), Some(KeyState::Added { name: "yubikey".into() }));
///
/// // An add that waits past its life fails.
/// let old = adds.start(&proof, "phone", now);
/// let later = now + ADD_LIFE + Duration::from_secs(1);
/// assert!(matches!(adds.state(&old, "mike", later), Some(KeyState::Failed { .. })));
/// assert!(adds.options(&old, later).is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct Adds {
    life: Duration,
    adds: HashMap<String, Add>,
}

impl Default for Adds {
    fn default() -> Self {
        Adds::new(ADD_LIFE)
    }
}

/// An add that the reply of the page took: what the check and the
/// command need.
#[derive(Clone, Debug)]
pub struct Taken {
    pub name: String,
    pub challenge: [u8; 32],
    pub proof: SignedIn,
}

impl Adds {
    /// The adds, each good for `life`.
    pub fn new(life: Duration) -> Adds {
        Adds {
            life,
            adds: HashMap::new(),
        }
    }

    /// A new add of the key `name` for the person of `proof`: gives its
    /// ticket.
    pub fn start(&mut self, proof: &SignedIn, name: &str, now: Instant) -> String {
        self.forget(now);
        let ticket = crate::token::random_token();
        let mut challenge = [0u8; 32];
        getrandom::fill(&mut challenge).expect("the OS gives random bytes");
        let add = Add {
            user: proof.who.user().to_owned(),
            name: name.to_owned(),
            challenge,
            proof: proof.clone(),
            made: now,
            state: Step::Waiting,
        };
        self.adds.insert(ticket.clone(), add);
        ticket
    }

    /// The add of `ticket` while it waits for its reply.
    fn waiting(&self, ticket: &str, now: Instant) -> Result<&Add, String> {
        let Some(add) = self.adds.get(ticket) else {
            return Err(UNKNOWN.into());
        };
        if add.state != Step::Waiting {
            return Err("this add has its reply already: run riff device add again".into());
        }
        if now.duration_since(add.made) > self.life {
            return Err(OLD.into());
        }
        Ok(add)
    }

    /// The USER, the name and the challenge of the add of `ticket`,
    /// while it waits for its reply.
    pub fn options(
        &self,
        ticket: &str,
        now: Instant,
    ) -> Result<(String, String, [u8; 32]), String> {
        let add = self.waiting(ticket, now)?;
        Ok((add.user.clone(), add.name.clone(), add.challenge))
    }

    /// Takes the add of `ticket` for its one reply.
    pub fn take(&mut self, ticket: &str, now: Instant) -> Result<Taken, String> {
        let add = self.waiting(ticket, now)?;
        let taken = Taken {
            name: add.name.clone(),
            challenge: add.challenge,
            proof: add.proof.clone(),
        };
        if let Some(add) = self.adds.get_mut(ticket) {
            add.state = Step::Checking;
        }
        Ok(taken)
    }

    /// The add of `ticket` ended as `state`.
    pub fn end(&mut self, ticket: &str, state: KeyState) {
        if let Some(add) = self.adds.get_mut(ticket) {
            add.state = Step::Ended(state);
        }
    }

    /// Where the add of `ticket` of `user` is. `None` when riff-server
    /// knows no such add of `user`.
    pub fn state(&self, ticket: &str, user: &str, now: Instant) -> Option<KeyState> {
        let add = self.adds.get(ticket).filter(|add| add.user == user)?;
        Some(match &add.state {
            Step::Waiting if now.duration_since(add.made) > self.life => {
                KeyState::Failed { reason: OLD.into() }
            }
            Step::Waiting | Step::Checking => KeyState::Waiting,
            Step::Ended(state) => state.clone(),
        })
    }

    /// Forgets each add older than two lives.
    fn forget(&mut self, now: Instant) {
        let most = 2 * self.life;
        self.adds
            .retain(|_, add| now.duration_since(add.made) <= most);
    }
}

/// The refusal of a ticket that riff-server does not know.
const UNKNOWN: &str = "riff-server knows no such add: run riff device add --key NAME again";

/// The refusal of an add whose time ended.
const OLD: &str = "the time of the add ended: run riff device add --key NAME again";

/// Refuses a session token (01M75NH30PNATYN33MXZQMVBW8).
fn person_only(signed_in: &SignedIn) -> Result<(), (StatusCode, String)> {
    match signed_in.who.session() {
        Some(_) => Err((StatusCode::FORBIDDEN, PERSON_ONLY.to_owned())),
        None => Ok(()),
    }
}

/// The relying party of `s` (01M75NH2Y3H1CR10CGRWNGT8RG).
fn party(s: &crate::Server) -> Party {
    Party::of(&s.config.public_url)
}

/// The adds of `s`.
fn adds(s: &crate::Server) -> std::sync::MutexGuard<'_, Adds> {
    s.devices
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

/// `POST /v1/devices`: the devices of the person of the token
/// (01M75NH3625QATGDT6DK652ZK6).
pub(crate) async fn list(
    AxumState(s): AxumState<crate::Shared>,
    Extension(signed_in): Extension<SignedIn>,
    Json(Devices {}): Json<Devices>,
) -> crate::Reply<DevicesReply> {
    person_only(&signed_in)?;
    let user = signed_in.who.user();
    Ok(Json(s.engine.read(|state| state.devices(user))))
}

/// `POST /v1/device/key`: a new add, and the URL of its page
/// (01M75NH2RSEJ3K618M17PCH3ZZ).
pub(crate) async fn start(
    AxumState(s): AxumState<crate::Shared>,
    Extension(signed_in): Extension<SignedIn>,
    headers: HeaderMap,
    Json(KeyStart { name }): Json<KeyStart>,
) -> Result<Json<KeyStarted>, Response> {
    person_only(&signed_in).map_err(IntoResponse::into_response)?;
    let user = signed_in.who.user();
    s.engine
        .read(|state| state.refuse_key_name(user, &name))
        .map_err(|refused| Failed::Refused(refused).into_response())?;
    // With the policy required, the add of a second key needs a proof of
    // a key of the person (PROOF_KEYS). The first key rests on the
    // trust of the sign-in.
    if s.engine.read(|state| state.has_key(user)) {
        let ticket = crate::proofs::ticket_of(&headers).map_err(IntoResponse::into_response)?;
        let call = headers
            .get(riff_core::wire::CALL_HEADER)
            .and_then(|value| value.to_str().ok());
        let action = Action::AddKey { name: name.clone() };
        s.engine
            .use_step(Some(&signed_in), ticket.as_deref(), call, &action)
            .map_err(IntoResponse::into_response)?;
    }
    let ticket = adds(&s).start(&signed_in, &name, Instant::now());
    tracing::info!(%user, %name, "riff device add: a page for a new key");
    Ok(Json(KeyStarted {
        url: format!("{}{PAGE_PATH}#{ticket}", party(&s).origin),
        ticket,
    }))
}

/// `POST /v1/device/key/wait`: where the add of the ticket is. Only the
/// person of the add can ask.
pub(crate) async fn wait(
    AxumState(s): AxumState<crate::Shared>,
    Extension(signed_in): Extension<SignedIn>,
    Json(KeyWait { ticket }): Json<KeyWait>,
) -> crate::Reply<KeyState> {
    person_only(&signed_in)?;
    adds(&s)
        .state(&ticket, signed_in.who.user(), Instant::now())
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, UNKNOWN.to_owned()))
}

/// `GET /device/key`: the page of the add. It reads the ticket from its
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

/// `POST /device/key/options`: the options of the new credential of the
/// add of the ticket (01M75NH2VF6HGZVY04GB358JG0).
pub(crate) async fn options(
    AxumState(s): AxumState<crate::Shared>,
    Json(KeyTicket { ticket }): Json<KeyTicket>,
) -> crate::Reply<KeyOptions> {
    let (user, name, challenge) = adds(&s)
        .options(&ticket, Instant::now())
        .map_err(|why| (StatusCode::BAD_REQUEST, why))?;
    let (riff_id, credentials) = s
        .engine
        .read(|state| (state.riff_id(), state.credentials(&user)));
    let handle = Sha256::digest(format!("{}/{user}", riff_id.unwrap_or_default()));
    Ok(Json(webauthn::options(
        &party(&s),
        &user,
        &handle[..16],
        &challenge,
        &credentials,
        &name,
    )))
}

/// `POST /device/key/reply`: the check of the reply of the authenticator,
/// and the command `add_key` (01M75NH2VF6HGZVY04GB358JG0). Each reply
/// ends the add: added, or refused with the reason.
pub(crate) async fn reply(
    AxumState(s): AxumState<crate::Shared>,
    Json(reply): Json<KeyReply>,
) -> crate::Reply<KeyReplied> {
    let ticket = reply.ticket.clone();
    let taken = adds(&s)
        .take(&ticket, Instant::now())
        .map_err(|why| (StatusCode::BAD_REQUEST, why))?;
    let user = taken.proof.who.user().to_owned();
    let failed = |status: StatusCode, reason: String| {
        tracing::warn!(%user, name = %taken.name, "riff device add: riff refused the key: {reason}");
        adds(&s).end(
            &ticket,
            KeyState::Failed {
                reason: reason.clone(),
            },
        );
        (status, reason)
    };
    let checked = match webauthn::check(&party(&s), &taken.challenge, &reply) {
        Ok(checked) => checked,
        Err(refused) => return Err(failed(StatusCode::BAD_REQUEST, refused.0)),
    };
    let add = AddKey {
        name: taken.name.clone(),
        credential: B64.encode(&checked.credential),
        public_key: B64.encode(&checked.public_key),
    };
    match s.engine.add_key(&taken.proof, add).await {
        Ok(device) => {
            tracing::info!(%user, name = %device.name, alg = checked.alg, "riff device add: riff added a key");
            adds(&s).end(
                &ticket,
                KeyState::Added {
                    name: device.name.clone(),
                },
            );
            Ok(Json(KeyReplied {
                text: format!(
                    "riff added the key {}. Go back to your terminal.",
                    device.name
                ),
            }))
        }
        Err(e) => Err(failed(e.status(), e.text())),
    }
}

/// The page of the add of a key. It reads the ticket from its fragment,
/// asks riff-server for the options, and gives them to
/// `navigator.credentials.create` when the person presses the button: a
/// browser asks for a gesture first. Then it sends the reply.
const PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>riff: add a key</title></head>
<body>
<h1>Add a key to riff</h1>
<p id="what">riff reads the add.</p>
<p><button id="add" hidden>Add the key</button></p>
<p id="said"></p>
<script>
"use strict";
const ticket = location.hash.slice(1);
history.replaceState(null, "", location.pathname);
const what = document.getElementById("what");
const button = document.getElementById("add");
const said = document.getElementById("said");
const bytes = (text) => Uint8Array.from(atob(text.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0));
const text = (buffer) => btoa(String.fromCharCode(...new Uint8Array(buffer))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
async function call(path, body) {
  const reply = await fetch(path, {method: "POST", headers: {"content-type": "application/json"}, body: JSON.stringify(body)});
  if (!reply.ok) throw new Error(await reply.text());
  return reply.json();
}
async function add(options) {
  button.disabled = true;
  said.textContent = "Touch your authenticator, or scan the QR code with your phone.";
  let credential;
  try {
    credential = await navigator.credentials.create({publicKey: {
      rp: options.rp,
      user: {...options.user, id: bytes(options.user.id)},
      challenge: bytes(options.challenge),
      pubKeyCredParams: options.pubKeyCredParams,
      timeout: options.timeout,
      excludeCredentials: options.excludeCredentials.map((c) => ({...c, id: bytes(c.id)})),
      authenticatorSelection: options.authenticatorSelection,
      attestation: options.attestation,
    }});
  } catch (e) {
    said.textContent = e.name === "InvalidStateError"
      ? "This authenticator has a key of riff already: riff device list shows it. Use another authenticator."
      : "The browser made no key: " + e.message + " Press the button to try again.";
    button.disabled = false;
    return;
  }
  try {
    const done = await call("/device/key/reply", {
      ticket,
      raw_id: text(credential.rawId),
      client_data: text(credential.response.clientDataJSON),
      attestation: text(credential.response.attestationObject),
    });
    said.textContent = done.text;
  } catch (e) {
    said.textContent = "riff did not add the key: " + e.message;
  }
}
(async () => {
  if (!window.PublicKeyCredential) {
    what.textContent = "This browser cannot make a key here. It needs WebAuthn, on https or on localhost.";
    return;
  }
  let options;
  try {
    options = await call("/device/key/options", {ticket});
  } catch (e) {
    what.textContent = e.message;
    return;
  }
  what.textContent = "Add the key " + options.name + " of " + options.user.name + " to the riff at " + options.rp.id + ". Press the button.";
  button.hidden = false;
  button.onclick = () => add(options);
})();
</script>
</body></html>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use riff_core::name::Who;

    fn proof(user: &str, session: Option<&str>) -> SignedIn {
        SignedIn {
            who: Who::new(user, session).unwrap(),
            jkt: "k".into(),
            started: 0,
        }
    }

    #[test]
    fn a_session_token_is_refused() {
        assert!(person_only(&proof("mike", None)).is_ok());
        let (status, text) = person_only(&proof("mike", Some("a1"))).unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(text.contains("a session never adds"), "{text}");
    }

    #[test]
    fn each_add_has_its_own_ticket_and_challenge() {
        let now = Instant::now();
        let mut adds = Adds::default();
        let one = adds.start(&proof("mike", None), "a", now);
        let two = adds.start(&proof("mike", None), "b", now);
        assert_ne!(one, two);
        let (_, _, first) = adds.options(&one, now).unwrap();
        let (_, _, second) = adds.options(&two, now).unwrap();
        assert_ne!(first, second);
        assert_eq!(adds.options("made-up", now).unwrap_err(), UNKNOWN);
    }

    #[test]
    fn an_add_that_is_checked_still_waits_for_the_person() {
        let now = Instant::now();
        let mut adds = Adds::default();
        let ticket = adds.start(&proof("mike", None), "a", now);
        adds.take(&ticket, now).unwrap();
        assert_eq!(adds.state(&ticket, "mike", now), Some(KeyState::Waiting));
        // The time of a checked add does not end it.
        let later = now + ADD_LIFE + Duration::from_secs(1);
        assert_eq!(adds.state(&ticket, "mike", later), Some(KeyState::Waiting));
    }

    #[test]
    fn an_old_add_is_forgotten_after_two_lives() {
        let now = Instant::now();
        let mut adds = Adds::default();
        let old = adds.start(&proof("mike", None), "a", now);
        let much_later = now + 2 * ADD_LIFE + Duration::from_secs(1);
        adds.start(&proof("mike", None), "b", much_later);
        assert_eq!(adds.state(&old, "mike", much_later), None);
    }

    #[test]
    fn the_page_reads_the_ticket_from_its_fragment_and_calls_both_paths() {
        assert!(PAGE.contains("location.hash"));
        assert!(PAGE.contains("history.replaceState"));
        for path in [
            <KeyTicket as riff_core::wire::Call>::PATH,
            <KeyReply as riff_core::wire::Call>::PATH,
        ] {
            assert!(PAGE.contains(&format!("\"{path}\"")), "{path}");
        }
        assert!(PAGE.contains("navigator.credentials.create"));
    }
}
