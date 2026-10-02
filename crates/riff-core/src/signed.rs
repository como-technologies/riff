//! Signed messages (R195-R201).
//!
//! # Design
//!
//! Each message carries a signature from the device key of its sender:
//! the key that its token is bound to (see [`crate::dpop`]). A reader
//! checks the signature. So it finds a message that changed in storage,
//! or a message with a false sender, and it does not need to trust the
//! storage of the server.
//!
//! The signature is a JWS with a detached payload (RFC 7515, appendix
//! F). The message carries the payload next to the signature:
//!
//! ```text
//!  sig     = <header>..<signature>
//!            header  {"typ":"riff-message","alg":"ES256","jwk":{public key}}
//!  payload = base64url of the JSON of Content
//! ```
//!
//! [`Content`] is what the signature covers: the who and the lead mark
//! of the sender, the thread of the post, the `to` selectors, the body,
//! the kind and the time (R196).
//!
//! The sender makes the payload one time. The server and each reader
//! keep its bytes unchanged, and never encode them again. A check runs
//! over the kept bytes ([`check`]), then decodes them ([`Signed`]). A
//! build skips a field of the payload that it does not know, so a new
//! field never stops a check (01M3T411N0HM699VJXW6RTVWKB).
//!
//! # Rules
//!
//! - A post to a thread signs that thread. A direct message has no
//!   thread when it is sent, so it signs no thread. Its one selector
//!   names the other session.
//! - `riff-server` accepts a signed post only from the key of the token
//!   of its caller, and only with a time near its own time (R197).
//! - A reader counts a message as verified only when the signature is
//!   valid, covers the message as the reader got it, and comes from a
//!   key of the user of the sender (R199). See
//!   [`crate::wire::Message::verified`].
//! - ECDSA can give two valid signatures for the same bytes. So a copy
//!   check compares [`payload_hash`], not the signature.
//!
//! # Example
//!
//! ```
//! use riff_core::dpop::Key;
//! use riff_core::name::Who;
//! use riff_core::signed::Content;
//! use riff_core::wire::Kind;
//!
//! let key = Key::generate();
//! let mike = Who::new("mike", Some("a6cf"))?;
//! let thread = "como-technologies/riff".parse()?;
//! let content = Content {
//!     from: &mike,
//!     thread: Some(&thread),
//!     to: &[],
//!     body: "ready",
//!     lead: false,
//!     kind: Kind::Message,
//!     at_ms: 1_000,
//! };
//! let sig = content.sign(&key);
//! assert_eq!(content.verify(&sig).unwrap(), key.thumbprint());
//!
//! // A changed body does not verify.
//! let changed = Content { body: "not ready", ..content };
//! assert!(changed.verify(&sig).is_err());
//!
//! // The check of the kept payload gives the key and the fields.
//! let (jkt, signed) = riff_core::signed::check(&content.payload(), &sig).unwrap();
//! assert_eq!(jkt, key.thumbprint());
//! assert!(signed.covers(&content));
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use p256::ecdsa::Signature;
use p256::ecdsa::signature::Verifier;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dpop::{ALG, Jwk, Key, json_b64};
use crate::name::{ThreadName, Who};
use crate::selector::Selector;
use crate::wire::Kind;

/// The `typ` of a message signature. It is not `dpop+jwt`, so a proof
/// never counts as a message signature, and a signature never counts as
/// a proof.
pub const TYP: &str = "riff-message";

/// What the signature of a message covers (R196).
#[derive(Clone, Copy, Debug, Serialize, schemars::JsonSchema)]
pub struct Content<'a> {
    /// The user and the session ID of the sender.
    pub from: &'a Who,
    /// True when the sender posts as the lead (R198). The place and the
    /// claims of the sender are not signed.
    pub lead: bool,
    /// The thread of the post. `None` for a direct message.
    pub thread: Option<&'a ThreadName>,
    pub to: &'a [Selector],
    pub body: &'a str,
    pub kind: Kind,
    /// The time of the message, in milliseconds since the Unix epoch.
    pub at_ms: u64,
}

impl Content<'_> {
    /// The payload of the signature: the base64url of the JSON of the
    /// content. The sender makes it one time. Each other side keeps its
    /// bytes.
    pub fn payload(&self) -> String {
        json_b64(self)
    }

    /// The signature of the content by the device key `key`.
    pub fn sign(&self, key: &Key) -> String {
        sign_payload(&self.payload(), key)
    }

    /// Checks that `sig` signs this content. Gives the thumbprint of the
    /// key that signed it. The caller checks that the key belongs to the
    /// sender.
    pub fn verify(&self, sig: &str) -> Result<String, SignError> {
        verify(&self.payload(), sig)
    }
}

/// The fields of a signed payload, as this build knows them. It skips
/// each field that it does not know.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Signed {
    pub from: Who,
    pub lead: bool,
    pub thread: Option<ThreadName>,
    #[serde(default)]
    pub to: Vec<Selector>,
    pub body: String,
    #[serde(default)]
    pub kind: Kind,
    pub at_ms: u64,
}

impl Signed {
    /// True when the payload holds the fields of `content`.
    pub fn covers(&self, content: &Content<'_>) -> bool {
        self.from == *content.from
            && self.lead == content.lead
            && self.thread.as_ref() == content.thread
            && self.to == content.to
            && self.body == content.body
            && self.kind == content.kind
            && self.at_ms == content.at_ms
    }
}

/// Checks that `sig` signs the kept bytes of `payload`, then decodes
/// them. Gives the thumbprint of the key and the fields. It never
/// encodes the payload again.
///
/// ```
/// use base64::Engine;
/// use riff_core::dpop::Key;
/// use riff_core::signed::{check, sign_payload};
///
/// let key = Key::generate();
/// // A payload with a field that this build does not know.
/// let json = r#"{"from":{"user":"ann","session":"s1"},"lead":false,"thread":"design",
///     "to":[],"body":"hi","kind":"message","at_ms":5,"reply_to":7}"#;
/// let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json);
/// let sig = sign_payload(&payload, &key);
/// let (jkt, signed) = check(&payload, &sig).unwrap();
/// assert_eq!(jkt, key.thumbprint());
/// assert_eq!(signed.body, "hi");
/// ```
pub fn check(payload: &str, sig: &str) -> Result<(String, Signed), SignError> {
    let jkt = verify(payload, sig)?;
    let bytes = B64
        .decode(payload)
        .map_err(|_| SignError::new("the payload is not base64url"))?;
    let signed = serde_json::from_slice(&bytes)
        .map_err(|e| SignError(format!("the payload does not decode: {e}")))?;
    Ok((jkt, signed))
}

/// The signature of the kept bytes `payload` by the device key `key`.
pub fn sign_payload(payload: &str, key: &Key) -> String {
    let header = json_b64(&Header {
        typ: TYP.into(),
        alg: ALG.into(),
        jwk: key.jwk(),
    });
    let input = format!("{header}.{payload}");
    format!("{header}..{}", key.sign(input.as_bytes()))
}

/// The hash of a payload, in hex. Two copies of one message have the
/// same hash.
///
/// ```
/// use riff_core::signed::payload_hash;
///
/// assert_eq!(payload_hash("abc"), payload_hash("abc"));
/// assert_ne!(payload_hash("abc"), payload_hash("abd"));
/// assert_eq!(payload_hash("abc").len(), 64);
/// ```
pub fn payload_hash(payload: &str) -> String {
    Sha256::digest(payload.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Checks that `sig` signs `payload` as it is. Gives the thumbprint of
/// the key.
fn verify(payload: &str, sig: &str) -> Result<String, SignError> {
    let Some((header, signature)) = sig.split_once("..") else {
        return Err(SignError::new("the signature is not a detached JWS"));
    };
    let header_json: serde_json::Value = decode(header)?;
    if header_json.pointer("/jwk/d").is_some() {
        return Err(SignError::new("the signature jwk holds a private key"));
    }
    let header_json: Header = serde_json::from_value(header_json)
        .map_err(|_| SignError::new("the signature header is not valid"))?;
    if header_json.typ != TYP {
        return Err(SignError::new("the signature typ is not riff-message"));
    }
    if header_json.alg != ALG {
        return Err(SignError::new("the signature alg is not ES256"));
    }
    let key = header_json
        .jwk
        .verifying_key()
        .map_err(|e| SignError(e.to_string()))?;
    let signature = B64
        .decode(signature)
        .ok()
        .and_then(|bytes| Signature::from_slice(&bytes).ok())
        .ok_or_else(|| SignError::new("the signature is malformed"))?;
    let input = format!("{header}.{payload}");
    key.verify(input.as_bytes(), &signature)
        .map_err(|_| SignError::new("the signature is not valid for this message"))?;
    Ok(header_json.jwk.thumbprint())
}

/// Why a signature is not valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignError(String);

impl SignError {
    fn new(message: &str) -> Self {
        SignError(message.to_owned())
    }
}

impl fmt::Display for SignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SignError {}

#[derive(Serialize, Deserialize)]
struct Header {
    typ: String,
    alg: String,
    jwk: Jwk,
}

fn decode<T: for<'de> Deserialize<'de>>(part: &str) -> Result<T, SignError> {
    let bytes = B64
        .decode(part)
        .map_err(|_| SignError::new("the signature header is not base64url"))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| SignError::new("the signature header is not valid JSON"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mike() -> Who {
        Who::new("mike", Some("a6cf")).unwrap()
    }

    fn thread() -> ThreadName {
        "como-technologies/riff".parse().unwrap()
    }

    fn to() -> Vec<Selector> {
        vec!["claim=issue-6".parse().unwrap()]
    }

    #[test]
    fn each_part_of_the_content_counts() {
        let key = Key::generate();
        let (mike, thread, to) = (mike(), thread(), to());
        let content = Content {
            from: &mike,
            thread: Some(&thread),
            to: &to,
            body: "ready",
            lead: false,
            kind: Kind::Message,
            at_ms: 1_000,
        };
        let sig = content.sign(&key);
        assert_eq!(content.verify(&sig).unwrap(), key.thumbprint());

        let brett = Who::new("brett", Some("a6cf")).unwrap();
        let other_session = Who::new("mike", Some("b7d0")).unwrap();
        let other_thread: ThreadName = "design".parse().unwrap();
        let changed = [
            Content {
                from: &brett,
                ..content
            },
            Content {
                from: &other_session,
                ..content
            },
            Content {
                thread: Some(&other_thread),
                ..content
            },
            Content {
                thread: None,
                ..content
            },
            Content { to: &[], ..content },
            Content {
                body: "ready!",
                ..content
            },
            Content {
                lead: true,
                ..content
            },
            Content {
                kind: Kind::Status,
                ..content
            },
            Content {
                at_ms: 1_001,
                ..content
            },
        ];
        for c in changed {
            assert_eq!(
                c.verify(&sig).unwrap_err().to_string(),
                "the signature is not valid for this message",
                "{c:?}"
            );
        }
    }

    /// A signed message of a later build: this build reads its kind as
    /// `other`, and keeps each field of its selector
    /// (01M3XSF90E9JYYTC13D9THY4WE). The check still covers the
    /// selector as it came.
    #[test]
    fn a_payload_with_a_kind_and_a_selector_field_of_a_later_build_checks() {
        let key = Key::generate();
        let json = r#"{"from":{"user":"mike","session":"a6cf"},"lead":true,"thread":"design",
            "to":[{"user":"brett","wave":"17"}],"body":"go","kind":"poll","at_ms":5}"#;
        let payload = B64.encode(json);
        let sig = sign_payload(&payload, &key);
        let (jkt, signed) = check(&payload, &sig).unwrap();
        assert_eq!(jkt, key.thumbprint());
        assert_eq!(signed.kind, Kind::Other);
        assert!(signed.to[0].is_other());

        let (mike, thread) = (mike(), "design".parse().unwrap());
        let read = |selector: &str| -> Vec<Selector> { serde_json::from_str(selector).unwrap() };
        let to = read(r#"[{"user":"brett","wave":"17"}]"#);
        let content = Content {
            from: &mike,
            lead: true,
            thread: Some(&thread),
            to: &to,
            body: "go",
            kind: Kind::Other,
            at_ms: 5,
        };
        assert!(signed.covers(&content));
        // A change of the field that the build does not know, or a
        // selector with only the fields that it knows, is not covered.
        for changed in [r#"[{"user":"brett","wave":"18"}]"#, r#"[{"user":"brett"}]"#] {
            let to = read(changed);
            assert!(!signed.covers(&Content { to: &to, ..content }), "{changed}");
        }
        // A kind that the build knows is not the kind of the payload.
        let known = Content {
            kind: Kind::Message,
            ..content
        };
        assert!(!signed.covers(&known));
    }

    #[test]
    fn the_signature_names_its_key() {
        let mike = mike();
        let content = Content {
            from: &mike,
            thread: None,
            to: &[],
            body: "hi",
            lead: false,
            kind: Kind::Message,
            at_ms: 5,
        };
        let (a, b) = (Key::generate(), Key::generate());
        assert_eq!(content.verify(&content.sign(&a)).unwrap(), a.thumbprint());
        assert_eq!(content.verify(&content.sign(&b)).unwrap(), b.thumbprint());
    }

    #[test]
    fn a_signature_from_another_key_under_this_header_is_refused() {
        let mike = mike();
        let content = Content {
            from: &mike,
            thread: None,
            to: &[],
            body: "hi",
            lead: false,
            kind: Kind::Message,
            at_ms: 5,
        };
        let sig = content.sign(&Key::generate());
        let other = content.sign(&Key::generate());
        let (header, _) = sig.split_once("..").unwrap();
        let (_, signature) = other.split_once("..").unwrap();
        let forged = format!("{header}..{signature}");
        assert!(content.verify(&forged).is_err());
    }

    #[test]
    fn a_dpop_proof_is_not_a_signature() {
        let key = Key::generate();
        let proof = key.proof("POST", "https://riff.example.com/v1/post", None, 5);
        let mut parts = proof.split('.');
        let (header, _, signature) = (parts.next(), parts.next(), parts.next());
        let as_sig = format!("{}..{}", header.unwrap(), signature.unwrap());
        let mike = mike();
        let content = Content {
            from: &mike,
            thread: None,
            to: &[],
            body: "hi",
            lead: false,
            kind: Kind::Message,
            at_ms: 5,
        };
        assert_eq!(
            content.verify(&as_sig).unwrap_err().to_string(),
            "the signature typ is not riff-message"
        );
    }

    #[test]
    fn malformed_signatures_are_refused() {
        let mike = mike();
        let content = Content {
            from: &mike,
            thread: None,
            to: &[],
            body: "hi",
            lead: false,
            kind: Kind::Message,
            at_ms: 5,
        };
        for bad in ["", "a.b.c", "a..b", "!!..!!"] {
            assert!(content.verify(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_private_jwk_and_other_algorithms_are_refused() {
        let key = Key::generate();
        let mike = mike();
        let content = Content {
            from: &mike,
            thread: None,
            to: &[],
            body: "hi",
            lead: false,
            kind: Kind::Message,
            at_ms: 5,
        };
        let with = |header: serde_json::Value| {
            let header = json_b64(&header);
            let input = format!("{header}.{}", json_b64(&content));
            format!("{header}..{}", key.sign(input.as_bytes()))
        };
        let good = serde_json::json!({"typ": TYP, "alg": ALG, "jwk": key.jwk()});
        assert!(content.verify(&with(good.clone())).is_ok());

        let mut private = good.clone();
        private["jwk"]["d"] = "secret".into();
        assert_eq!(
            content.verify(&with(private)).unwrap_err().to_string(),
            "the signature jwk holds a private key"
        );
        let mut none = good;
        none["alg"] = "none".into();
        assert_eq!(
            content.verify(&with(none)).unwrap_err().to_string(),
            "the signature alg is not ES256"
        );
    }
}
