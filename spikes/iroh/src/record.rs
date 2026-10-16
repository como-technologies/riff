//! A record that one daemon sends to another, signed by its author.
//!
//! Decision 23 of #699: iroh checks the node key, each record carries
//! the signature of its author, and the daemon checks the size and the
//! type of each message before it applies it. In this spike the author
//! key is the node key of the endpoint, so a record is good only on a
//! connection from its own author.
//!
//! The bytes of a record, with each number big-endian:
//!
//! | Bytes      | Part                                   |
//! |------------|----------------------------------------|
//! | 1          | the type: 1 a message, 2 a status      |
//! | 32         | the public key of the author (ed25519) |
//! | 8          | the sequence number of the author      |
//! | 2          | the length of the body                 |
//! | the length | the body, UTF-8                        |
//! | 64         | the signature                          |
//!
//! The signature covers [`DOMAIN`] and each byte before the signature.
//! A record is at most [`MAX`] bytes.
//!
//! ```
//! use iroh::SecretKey;
//! use iroh_spike::record::{self, Kind, Refused};
//!
//! let author = SecretKey::generate();
//! let bytes = record::sign(&author, Kind::Message, 7, "hello").unwrap();
//! let got = record::check(&bytes, author.public()).unwrap();
//! assert_eq!((got.kind, got.seq, got.body.as_str()), (Kind::Message, 7, "hello"));
//!
//! let other = SecretKey::generate().public();
//! assert_eq!(record::check(&bytes, other), Err(Refused::NotThePeer));
//! ```

use std::fmt;

use iroh::{PublicKey, SecretKey, Signature};

/// The largest record, in bytes. A reader asks the stream for no more.
pub const MAX: usize = 4096;

/// The bytes before the body: the type, the author, the sequence
/// number and the length.
const HEAD: usize = 1 + 32 + 8 + 2;

/// The bytes of the signature.
const SIG: usize = Signature::LENGTH;

/// The largest body that fits in [`MAX`].
pub const MAX_BODY: usize = MAX - HEAD - SIG;

/// The signature covers this tag first, so a signature of a record
/// is good for nothing else that the same key signs.
pub const DOMAIN: &[u8] = b"riff spike record v1\0";

/// The type of a record. Each type has one writer in riff 3.0
/// (decision 20); the spike knows two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Message,
    Status,
}

impl Kind {
    fn byte(self) -> u8 {
        match self {
            Kind::Message => 1,
            Kind::Status => 2,
        }
    }

    fn of(byte: u8) -> Option<Kind> {
        match byte {
            1 => Some(Kind::Message),
            2 => Some(Kind::Status),
            _ => None,
        }
    }
}

/// A record that passed each check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub kind: Kind,
    pub author: PublicKey,
    pub seq: u64,
    pub body: String,
}

/// Why a reader refuses a record. The checks run in this order: the
/// size, the type, the length, the author, the signature, the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The record has fewer bytes than a head and a signature.
    TooSmall(usize),
    /// The record has more than [`MAX`] bytes.
    TooBig(usize),
    /// The type byte names no type.
    UnknownType(u8),
    /// The length of the body does not agree with the size.
    BadLength,
    /// The author is not the peer of the connection.
    NotThePeer,
    /// The signature is not good for the author.
    BadSignature,
    /// The body is not UTF-8.
    NotText,
    /// A body for [`sign`] is longer than [`MAX_BODY`].
    BodyTooBig(usize),
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refused::TooSmall(n) => write!(f, "too small: {n} bytes"),
            Refused::TooBig(n) => write!(f, "too big: {n} bytes, the limit is {MAX}"),
            Refused::UnknownType(b) => write!(f, "unknown type {b}"),
            Refused::BadLength => write!(f, "the length of the body does not agree"),
            Refused::NotThePeer => write!(f, "the author is not the peer"),
            Refused::BadSignature => write!(f, "the signature is not good"),
            Refused::NotText => write!(f, "the body is not UTF-8"),
            Refused::BodyTooBig(n) => write!(f, "the body has {n} bytes, the limit is {MAX_BODY}"),
        }
    }
}

impl std::error::Error for Refused {}

/// Makes the bytes of a record with this author, type, sequence
/// number and body.
pub fn sign(author: &SecretKey, kind: Kind, seq: u64, body: &str) -> Result<Vec<u8>, Refused> {
    if body.len() > MAX_BODY {
        return Err(Refused::BodyTooBig(body.len()));
    }
    let mut bytes = Vec::with_capacity(HEAD + body.len() + SIG);
    bytes.push(kind.byte());
    bytes.extend_from_slice(author.public().as_bytes());
    bytes.extend_from_slice(&seq.to_be_bytes());
    // MAX_BODY is less than u16::MAX, so the length fits.
    bytes.extend_from_slice(&(body.len() as u16).to_be_bytes());
    bytes.extend_from_slice(body.as_bytes());
    let signature = author.sign(&signed(&bytes));
    bytes.extend_from_slice(&signature.to_bytes());
    Ok(bytes)
}

/// Checks the bytes of a record from the peer `peer`, the endpoint ID
/// that iroh checked at the handshake.
pub fn check(bytes: &[u8], peer: PublicKey) -> Result<Record, Refused> {
    if bytes.len() > MAX {
        return Err(Refused::TooBig(bytes.len()));
    }
    if bytes.len() < HEAD + SIG {
        return Err(Refused::TooSmall(bytes.len()));
    }
    let kind = Kind::of(bytes[0]).ok_or(Refused::UnknownType(bytes[0]))?;
    let length = u16::from_be_bytes([bytes[41], bytes[42]]) as usize;
    if HEAD + length + SIG != bytes.len() {
        return Err(Refused::BadLength);
    }
    let author: [u8; 32] = bytes[1..33].try_into().expect("32 bytes");
    if author != *peer.as_bytes() {
        return Err(Refused::NotThePeer);
    }
    let (content, signature) = bytes.split_at(HEAD + length);
    let signature = Signature::from_bytes(signature.try_into().expect("64 bytes"));
    peer.verify(&signed(content), &signature)
        .map_err(|_| Refused::BadSignature)?;
    let seq = u64::from_be_bytes(bytes[33..41].try_into().expect("8 bytes"));
    let body = std::str::from_utf8(&content[HEAD..]).map_err(|_| Refused::NotText)?;
    Ok(Record {
        kind,
        author: peer,
        seq,
        body: body.to_owned(),
    })
}

fn signed(content: &[u8]) -> Vec<u8> {
    [DOMAIN, content].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> SecretKey {
        SecretKey::from_bytes(&[7; 32])
    }

    #[test]
    fn a_signed_record_passes_each_check() {
        let bytes = sign(&key(), Kind::Status, 42, "ready").unwrap();
        assert_eq!(bytes.len(), HEAD + 5 + SIG);
        let got = check(&bytes, key().public()).unwrap();
        assert_eq!(got.kind, Kind::Status);
        assert_eq!(got.seq, 42);
        assert_eq!(got.body, "ready");
        assert_eq!(got.author, key().public());
    }

    #[test]
    fn the_largest_body_fits_and_one_more_byte_does_not() {
        let body = "x".repeat(MAX_BODY);
        let bytes = sign(&key(), Kind::Message, 1, &body).unwrap();
        assert_eq!(bytes.len(), MAX);
        assert!(check(&bytes, key().public()).is_ok());
        let body = "x".repeat(MAX_BODY + 1);
        assert_eq!(
            sign(&key(), Kind::Message, 1, &body),
            Err(Refused::BodyTooBig(MAX_BODY + 1))
        );
    }

    #[test]
    fn a_record_over_the_limit_is_refused_before_any_other_check() {
        let mut bytes = vec![9; MAX + 1];
        assert_eq!(check(&bytes, key().public()), Err(Refused::TooBig(MAX + 1)));
        bytes.truncate(HEAD + SIG - 1);
        assert_eq!(
            check(&bytes, key().public()),
            Err(Refused::TooSmall(HEAD + SIG - 1))
        );
    }

    #[test]
    fn an_unknown_type_is_refused() {
        let mut bytes = sign(&key(), Kind::Message, 1, "hi").unwrap();
        bytes[0] = 3;
        assert_eq!(check(&bytes, key().public()), Err(Refused::UnknownType(3)));
    }

    #[test]
    fn a_length_that_does_not_agree_is_refused() {
        let mut bytes = sign(&key(), Kind::Message, 1, "hi").unwrap();
        bytes[42] = 3;
        assert_eq!(check(&bytes, key().public()), Err(Refused::BadLength));
        let mut bytes = sign(&key(), Kind::Message, 1, "hi").unwrap();
        bytes.push(0);
        assert_eq!(check(&bytes, key().public()), Err(Refused::BadLength));
    }

    #[test]
    fn a_record_of_another_author_is_refused() {
        let other = SecretKey::from_bytes(&[8; 32]);
        let bytes = sign(&other, Kind::Message, 1, "hi").unwrap();
        assert_eq!(check(&bytes, key().public()), Err(Refused::NotThePeer));
    }

    #[test]
    fn a_change_of_one_byte_breaks_the_signature() {
        let good = sign(&key(), Kind::Message, 1, "hi").unwrap();
        // The sequence number, the body and the signature itself.
        for at in [40, HEAD, HEAD + 2] {
            let mut bytes = good.clone();
            bytes[at] ^= 1;
            assert_eq!(
                check(&bytes, key().public()),
                Err(Refused::BadSignature),
                "byte {at}"
            );
        }
        // A changed type is still a known type, and the signature fails.
        let mut bytes = good.clone();
        bytes[0] = Kind::Status.byte();
        assert_eq!(check(&bytes, key().public()), Err(Refused::BadSignature));
    }

    #[test]
    fn a_signature_of_the_content_with_no_domain_is_refused() {
        let mut bytes = sign(&key(), Kind::Message, 1, "hi").unwrap();
        let content = bytes.len() - SIG;
        let bare = key().sign(&bytes[..content]);
        bytes[content..].copy_from_slice(&bare.to_bytes());
        assert_eq!(check(&bytes, key().public()), Err(Refused::BadSignature));
    }

    #[test]
    fn a_body_that_is_not_utf8_is_refused() {
        // A signed body with a byte that is not UTF-8.
        let mut content = vec![Kind::Message.byte()];
        content.extend_from_slice(key().public().as_bytes());
        content.extend_from_slice(&1u64.to_be_bytes());
        content.extend_from_slice(&1u16.to_be_bytes());
        content.push(0xff);
        let signature = key().sign(&signed(&content));
        content.extend_from_slice(&signature.to_bytes());
        assert_eq!(check(&content, key().public()), Err(Refused::NotText));
    }
}
