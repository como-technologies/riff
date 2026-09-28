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
//! F). The reader makes the payload again from the message:
//!
//! ```text
//!  sig = <header>..<signature>
//!        header  {"typ":"riff-message","alg":"ES256","jwk":{public key}}
//!        payload the JSON of Content, not in the sig
//! ```
//!
//! [`Content`] is what the signature covers: the who and the lead mark
//! of the sender, the thread of the post, the `to` selectors, the body,
//! the kind and the time (R196).
//! Its JSON has one form only, so the sender and each reader make the
//! same bytes.
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
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use p256::ecdsa::Signature;
use p256::ecdsa::signature::Verifier;
use serde::{Deserialize, Serialize};

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
    /// The signature of the content by the device key `key`.
    pub fn sign(&self, key: &Key) -> String {
        let header = json_b64(&Header {
            typ: TYP.into(),
            alg: ALG.into(),
            jwk: key.jwk(),
        });
        let input = format!("{header}.{}", json_b64(self));
        format!("{header}..{}", key.sign(input.as_bytes()))
    }

    /// Checks that `sig` signs this content. Gives the thumbprint of the
    /// key that signed it. The caller checks that the key belongs to the
    /// sender.
    pub fn verify(&self, sig: &str) -> Result<String, SignError> {
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
        let input = format!("{header}.{}", json_b64(self));
        key.verify(input.as_bytes(), &signature)
            .map_err(|_| SignError::new("the signature is not valid for this message"))?;
        Ok(header_json.jwk.thumbprint())
    }
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
