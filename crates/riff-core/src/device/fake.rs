//! A fake FIDO2 / WebAuthn authenticator, for the tests of each crate.
//!
//! [`Authenticator`] has a P-256 key and a credential ID. It makes the
//! reply that the page of the add sends to riff-server
//! ([`KeyReply`]), as a browser and a real authenticator make it for
//! `navigator.credentials.create`:
//!
//! - `clientDataJSON`: the type `webauthn.create`, the challenge of the
//!   options, and the origin.
//! - `attestationObject`: CBOR with the form, the statement and the
//!   authenticator data. The data has the hash of the relying party ID,
//!   the flags, the counter, and the credential with its COSE key.
//!
//! A [`Shape`] other than [`Shape::Packed`] and [`Shape::None`] makes a
//! reply that riff-server must refuse.
//!
//! It also signs the assertion of a proof of presence ([`ProofReply`]),
//! as `navigator.credentials.get` gives it: the authenticator data with
//! the hash of the relying party ID, the flags and the counter, and the
//! signature over the data and the hash of `clientDataJSON`. A
//! [`Proof`] other than [`Proof::Good`] makes an assertion that
//! riff-server must refuse.
//!
//! ```
//! use riff_core::device::fake::{Authenticator, Shape, options};
//!
//! let key = Authenticator::new();
//! let reply = key.reply("T", &options("riff.example.com", "c2VlZA"), "https://riff.example.com");
//! assert_eq!(reply.ticket, "T");
//! assert_eq!(reply.raw_id, key.credential());
//! let other = key.reply_as("T", &options("riff.example.com", "c2VlZA"), "https://riff.example.com", Shape::OtherChallenge);
//! assert_ne!(other.client_data, reply.client_data);
//! ```

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use sha2::{Digest, Sha256};

use crate::wire::{
    KeyAlgorithm, KeyCredential, KeyOptions, KeyParty, KeyReply, KeySelection, KeyUser,
    ProofOptions, ProofReply,
};

/// The COSE number of ES256.
const ES256: i64 = -7;

/// The flag of user presence.
pub const PRESENT: u8 = 0x01;
/// The flag of user verification.
pub const VERIFIED: u8 = 0x04;
/// The flag of attested credential data.
pub const ATTESTED: u8 = 0x40;

/// The shape of a reply of [`Authenticator::reply_as`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A good reply with the attestation `packed` and no certificate:
    /// the new key signs it.
    Packed,
    /// A good reply with the attestation `none`.
    None,
    /// The attestation `packed`, with a signature of another key.
    BadSignature,
    /// A challenge that riff-server did not give.
    OtherChallenge,
    /// An origin that is not riff-server.
    OtherOrigin,
    /// The hash of another relying party ID.
    OtherParty,
    /// No flag of user presence.
    NoPresence,
    /// The type `webauthn.get`, of a sign-in, not of a new credential.
    OtherType,
    /// A `rawId` that is not the credential ID of the authenticator data.
    OtherRawId,
    /// An attestation of another form, for example `tpm`.
    Form(&'static str),
}

/// The shape of an assertion of [`Authenticator::assert_as`]: the
/// signature of a proof of presence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proof {
    /// A good assertion: the key signs it, and the counter grows.
    Good,
    /// A signature of another key.
    BadSignature,
    /// A challenge that riff-server did not give.
    OtherChallenge,
    /// An origin that is not riff-server.
    OtherOrigin,
    /// The hash of another relying party ID.
    OtherParty,
    /// No flag of user presence.
    NoPresence,
    /// The type `webauthn.create`, of a new credential.
    OtherType,
    /// The counter of the last assertion again: a copy of the key.
    SameCounter,
}

/// A fake authenticator with one credential: a P-256 key and an ID.
pub struct Authenticator {
    key: SigningKey,
    credential: Vec<u8>,
    /// The counter of the last assertion. `None` for an authenticator
    /// that counts nothing: each assertion has the counter 0.
    counter: Option<std::sync::atomic::AtomicU32>,
}

impl Default for Authenticator {
    fn default() -> Self {
        Authenticator::new()
    }
}

impl Authenticator {
    /// An authenticator with a new key and a new credential ID of 16
    /// bytes.
    pub fn new() -> Authenticator {
        let mut credential = vec![0u8; 16];
        getrandom::fill(&mut credential).expect("the OS gives random bytes");
        Authenticator {
            key: new_key(),
            credential,
            counter: Some(std::sync::atomic::AtomicU32::new(0)),
        }
    }

    /// An authenticator that counts nothing, as a passkey of a phone:
    /// each assertion has the counter 0.
    pub fn not_counting() -> Authenticator {
        Authenticator {
            counter: None,
            ..Authenticator::new()
        }
    }

    /// A good assertion of the proof of `ticket` with `options` on
    /// `origin`, as the page of the proof sends it.
    pub fn assert(&self, ticket: &str, options: &ProofOptions, origin: &str) -> ProofReply {
        self.assert_as(ticket, options, origin, Proof::Good)
    }

    /// An assertion of the shape `shape`.
    ///
    /// ```
    /// use riff_core::device::fake::{Authenticator, Proof, proof_options};
    ///
    /// let key = Authenticator::new();
    /// let options = proof_options("riff.example.com", "c2VlZA", &key);
    /// let one = key.assert("T", &options, "https://riff.example.com");
    /// let two = key.assert("T", &options, "https://riff.example.com");
    /// // The counter grows, so the data differ.
    /// assert_ne!(one.authenticator_data, two.authenticator_data);
    /// let again = key.assert_as("T", &options, "https://riff.example.com", Proof::SameCounter);
    /// assert_eq!(again.authenticator_data, two.authenticator_data);
    /// ```
    pub fn assert_as(
        &self,
        ticket: &str,
        options: &ProofOptions,
        origin: &str,
        shape: Proof,
    ) -> ProofReply {
        use std::sync::atomic::Ordering;
        let challenge = match shape {
            Proof::OtherChallenge => B64.encode(b"a challenge of another proof"),
            _ => options.challenge.clone(),
        };
        let origin = match shape {
            Proof::OtherOrigin => "https://evil.example.com",
            _ => origin,
        };
        let kind = match shape {
            Proof::OtherType => "webauthn.create",
            _ => "webauthn.get",
        };
        let client_data = serde_json::json!({
            "type": kind,
            "challenge": challenge,
            "origin": origin,
            "crossOrigin": false,
        })
        .to_string();
        let party = match shape {
            Proof::OtherParty => "evil.example.com",
            _ => options.rp_id.as_str(),
        };
        let flags = match shape {
            Proof::NoPresence => VERIFIED,
            _ => PRESENT | VERIFIED,
        };
        let counter = match (&self.counter, shape) {
            (None, _) => 0,
            (Some(counter), Proof::SameCounter) => counter.load(Ordering::SeqCst),
            (Some(counter), _) => counter.fetch_add(1, Ordering::SeqCst) + 1,
        };
        let mut data = Sha256::digest(party.as_bytes()).to_vec();
        data.push(flags);
        data.extend_from_slice(&counter.to_be_bytes());
        let mut signed = data.clone();
        signed.extend_from_slice(&Sha256::digest(client_data.as_bytes()));
        let signer = match shape {
            Proof::BadSignature => new_key(),
            _ => self.key.clone(),
        };
        let signature: Signature = signer.sign(&signed);
        ProofReply {
            ticket: ticket.to_owned(),
            raw_id: self.credential(),
            client_data: B64.encode(client_data.as_bytes()),
            authenticator_data: B64.encode(&data),
            signature: B64.encode(signature.to_der().as_bytes()),
        }
    }

    /// The credential ID, in base64url with no padding.
    pub fn credential(&self) -> String {
        B64.encode(&self.credential)
    }

    /// The public key as a COSE key in CBOR, in base64url with no
    /// padding: the `public_key` of the record `key_added`.
    pub fn public_key(&self) -> String {
        B64.encode(self.cose_key())
    }

    /// A good reply to `options` for the page of the add of `ticket` on
    /// `origin`, with the attestation `packed`.
    pub fn reply(&self, ticket: &str, options: &KeyOptions, origin: &str) -> KeyReply {
        self.reply_as(ticket, options, origin, Shape::Packed)
    }

    /// A reply of the shape `shape`.
    pub fn reply_as(
        &self,
        ticket: &str,
        options: &KeyOptions,
        origin: &str,
        shape: Shape,
    ) -> KeyReply {
        let challenge = match shape {
            Shape::OtherChallenge => B64.encode(b"a challenge of another add"),
            _ => options.challenge.clone(),
        };
        let origin = match shape {
            Shape::OtherOrigin => "https://evil.example.com",
            _ => origin,
        };
        let kind = match shape {
            Shape::OtherType => "webauthn.get",
            _ => "webauthn.create",
        };
        let client_data = serde_json::json!({
            "type": kind,
            "challenge": challenge,
            "origin": origin,
            "crossOrigin": false,
        })
        .to_string();
        let party = match shape {
            Shape::OtherParty => "evil.example.com",
            _ => options.rp.id.as_str(),
        };
        let flags = match shape {
            Shape::NoPresence => ATTESTED,
            _ => PRESENT | VERIFIED | ATTESTED,
        };
        let data = self.data(party, flags);
        let mut signed = data.clone();
        signed.extend_from_slice(&Sha256::digest(client_data.as_bytes()));
        let signer = match shape {
            Shape::BadSignature => new_key(),
            _ => self.key.clone(),
        };
        let signature: Signature = signer.sign(&signed);
        let packed = Cbor::Map(vec![
            (Cbor::text("alg"), Cbor::Int(ES256)),
            (
                Cbor::text("sig"),
                Cbor::Bytes(signature.to_der().as_bytes().to_vec()),
            ),
        ]);
        let (form, statement) = match shape {
            Shape::None => ("none", Cbor::Map(Vec::new())),
            Shape::Form(form) => (form, packed),
            _ => ("packed", packed),
        };
        let attestation = Cbor::Map(vec![
            (Cbor::text("fmt"), Cbor::text(form)),
            (Cbor::text("attStmt"), statement),
            (Cbor::text("authData"), Cbor::Bytes(data)),
        ]);
        let raw_id = match shape {
            Shape::OtherRawId => B64.encode(b"another credential"),
            _ => self.credential(),
        };
        KeyReply {
            ticket: ticket.to_owned(),
            raw_id,
            client_data: B64.encode(client_data.as_bytes()),
            attestation: B64.encode(attestation.encode()),
        }
    }

    /// The authenticator data: the hash of the relying party ID, the
    /// flags, a counter of 0, an AAGUID of zeros, and the credential.
    fn data(&self, party: &str, flags: u8) -> Vec<u8> {
        let mut data = Sha256::digest(party.as_bytes()).to_vec();
        data.push(flags);
        data.extend_from_slice(&[0; 4]);
        data.extend_from_slice(&[0; 16]);
        let length = u16::try_from(self.credential.len()).expect("a short credential ID");
        data.extend_from_slice(&length.to_be_bytes());
        data.extend_from_slice(&self.credential);
        data.extend_from_slice(&self.cose_key());
        data
    }

    /// The public key as a COSE key: EC2, ES256, P-256, x and y.
    fn cose_key(&self) -> Vec<u8> {
        let point = self.key.verifying_key().to_sec1_point(false);
        let coordinate = |c: Option<&p256::FieldBytes>| c.map_or_else(Vec::new, |c| c.to_vec());
        Cbor::Map(vec![
            (Cbor::Int(1), Cbor::Int(2)),
            (Cbor::Int(3), Cbor::Int(ES256)),
            (Cbor::Int(-1), Cbor::Int(1)),
            (Cbor::Int(-2), Cbor::Bytes(coordinate(point.x()))),
            (Cbor::Int(-3), Cbor::Bytes(coordinate(point.y()))),
        ])
        .encode()
    }
}

/// The options of an add on the relying party `party` with the
/// challenge `challenge`, as riff-server gives them: for a test that
/// needs no riff-server.
pub fn options(party: &str, challenge: &str) -> KeyOptions {
    KeyOptions {
        rp: KeyParty {
            id: party.to_owned(),
            name: "riff".into(),
        },
        user: KeyUser {
            id: B64.encode(b"user"),
            name: "mike".into(),
            display_name: "mike".into(),
        },
        challenge: challenge.to_owned(),
        pub_key_cred_params: vec![KeyAlgorithm {
            kind: "public-key".into(),
            alg: ES256,
        }],
        timeout: 300_000,
        exclude_credentials: Vec::<KeyCredential>::new(),
        authenticator_selection: KeySelection {
            resident_key: "discouraged".into(),
            user_verification: "preferred".into(),
        },
        attestation: "none".into(),
        name: "yubikey".into(),
    }
}

/// The options of a proof on the relying party `party` with the
/// challenge `challenge` for the key of `key`, as riff-server gives them:
/// for a test that needs no riff-server.
pub fn proof_options(party: &str, challenge: &str, key: &Authenticator) -> ProofOptions {
    ProofOptions {
        challenge: challenge.to_owned(),
        timeout: 300_000,
        rp_id: party.to_owned(),
        allow_credentials: vec![KeyCredential {
            kind: "public-key".into(),
            id: key.credential(),
        }],
        user_verification: "preferred".into(),
        action: "test a proof of presence".into(),
        user: "mike".into(),
    }
}

fn new_key() -> SigningKey {
    loop {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("the OS gives random bytes");
        if let Ok(key) = SigningKey::from_slice(&bytes) {
            return key;
        }
    }
}

/// The few CBOR values that a reply holds, in the canonical form of
/// CTAP2: each length is definite, and the shortest head.
pub enum Cbor {
    Int(i64),
    Bytes(Vec<u8>),
    Text(String),
    Map(Vec<(Cbor, Cbor)>),
}

impl Cbor {
    /// A text value.
    pub fn text(text: &str) -> Cbor {
        Cbor::Text(text.to_owned())
    }

    /// The bytes of the value.
    ///
    /// ```
    /// use riff_core::device::fake::Cbor;
    ///
    /// assert_eq!(Cbor::Int(-7).encode(), [0x26]);
    /// assert_eq!(Cbor::Int(500).encode(), [0x19, 0x01, 0xf4]);
    /// assert_eq!(Cbor::text("fmt").encode(), b"\x63fmt");
    /// assert_eq!(Cbor::Map(vec![]).encode(), [0xa0]);
    /// ```
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut Vec<u8>) {
        match self {
            Cbor::Int(n) if *n >= 0 => head(0, n.unsigned_abs(), out),
            Cbor::Int(n) => head(1, (-1 - n).unsigned_abs(), out),
            Cbor::Bytes(bytes) => {
                head(2, bytes.len() as u64, out);
                out.extend_from_slice(bytes);
            }
            Cbor::Text(text) => {
                head(3, text.len() as u64, out);
                out.extend_from_slice(text.as_bytes());
            }
            Cbor::Map(pairs) => {
                head(5, pairs.len() as u64, out);
                for (key, value) in pairs {
                    key.write(out);
                    value.write(out);
                }
            }
        }
    }
}

/// The head of a CBOR item: the major type and the shortest form of `n`.
fn head(major: u8, n: u64, out: &mut Vec<u8>) {
    let major = major << 5;
    match n {
        0..24 => out.push(major | n as u8),
        24..0x100 => out.extend_from_slice(&[major | 24, n as u8]),
        0x100..0x1_0000 => {
            out.push(major | 25);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        0x1_0000..0x1_0000_0000 => {
            out.push(major | 26);
            out.extend_from_slice(&(n as u32).to_be_bytes());
        }
        _ => {
            out.push(major | 27);
            out.extend_from_slice(&n.to_be_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(text: &str) -> Vec<u8> {
        B64.decode(text).unwrap()
    }

    #[test]
    fn the_client_data_names_the_type_the_challenge_and_the_origin() {
        let key = Authenticator::new();
        let options = options("riff.example.com", "c2VlZA");
        let reply = key.reply("T", &options, "https://riff.example.com");
        let data: serde_json::Value = serde_json::from_slice(&decoded(&reply.client_data)).unwrap();
        assert_eq!(data["type"], "webauthn.create");
        assert_eq!(data["challenge"], "c2VlZA");
        assert_eq!(data["origin"], "https://riff.example.com");
    }

    #[test]
    fn the_authenticator_data_starts_with_the_hash_of_the_party_and_the_flags() {
        let key = Authenticator::new();
        let data = key.data("riff.example.com", PRESENT | ATTESTED);
        assert_eq!(&data[..32], &Sha256::digest(b"riff.example.com")[..]);
        assert_eq!(data[32], PRESENT | ATTESTED);
        assert_eq!(&data[55..71], &key.credential[..]);
        assert_eq!(&data[71..], &key.cose_key()[..]);
    }

    #[test]
    fn each_shape_changes_the_reply() {
        let key = Authenticator::new();
        let options = options("riff.example.com", "c2VlZA");
        let good = key.reply("T", &options, "https://riff.example.com");
        for shape in [
            Shape::None,
            Shape::BadSignature,
            Shape::OtherChallenge,
            Shape::OtherOrigin,
            Shape::OtherParty,
            Shape::NoPresence,
            Shape::OtherType,
            Shape::OtherRawId,
            Shape::Form("tpm"),
        ] {
            let other = key.reply_as("T", &options, "https://riff.example.com", shape);
            assert_ne!(other, good, "{shape:?}");
        }
    }

    #[test]
    fn each_length_has_its_shortest_head() {
        let mut out = Vec::new();
        for n in [0, 23, 24, 255, 256, 65_535, 65_536, u64::from(u32::MAX) + 1] {
            out.clear();
            head(0, n, &mut out);
            let expected = match n {
                0..24 => 1,
                24..0x100 => 2,
                0x100..0x1_0000 => 3,
                0x1_0000..0x1_0000_0000 => 5,
                _ => 9,
            };
            assert_eq!(out.len(), expected, "{n}");
        }
    }
}
