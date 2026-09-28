//! Device keys and DPoP proofs (RFC 9449, R18).
//!
//! # Design
//!
//! Each device has one P-256 key. The private part stays on the device,
//! in the OS keyring. Each request carries a *proof*: a short JWT that
//! the key signs (ES256). The proof names the method and the URL of the
//! request, and the hash of the access token. So a copied token or a
//! copied proof is of no use without the key.
//!
//! ```text
//!  Authorization: DPoP <access token>
//!  DPoP: <header>.<payload>.<signature>
//!        header  {"typ":"dpop+jwt","alg":"ES256","jwk":{public key}}
//!        payload {"jti","htm","htu","iat","ath"}
//! ```
//!
//! The server binds each sign-in to the thumbprint of the key (RFC
//! 7638). It accepts a token only with a proof from that key.
//!
//! # Rules for a valid proof
//!
//! - `typ` is `dpop+jwt`, `alg` is `ES256`, and `jwk` is a public P-256
//!   key with no private part.
//! - The signature is good for that key.
//! - `htm` is the method and `htu` is the URL of the request, with no
//!   query and no fragment.
//! - `iat` is at most [`MAX_AGE`] seconds old, and at most
//!   [`MAX_SKEW`] seconds in the future (R86).
//! - `jti` is not empty. The server refuses a `jti` that it saw before.
//! - With an access token, `ath` is its hash.
//!
//! # Example
//!
//! ```
//! use riff_core::dpop::{Key, verify};
//!
//! let key = Key::generate();
//! let url = "https://riff.example.com/v1/who";
//! let proof = key.proof("POST", url, Some("t-1"), 1_000);
//! let checked = verify(&proof, "POST", url, Some("t-1"), 1_000).unwrap();
//! assert_eq!(checked.jkt, key.thumbprint());
//!
//! // The proof does not work for another token or another URL.
//! assert!(verify(&proof, "POST", url, Some("t-2"), 1_000).is_err());
//! assert!(verify(&proof, "POST", "https://riff.example.com/v1/post", Some("t-1"), 1_000).is_err());
//! ```

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use p256::ecdsa::signature::{Signer, Verifier};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A proof may be this many seconds old (R86).
pub const MAX_AGE: u64 = 300;

/// A proof may be this many seconds in the future, for clock skew (R86).
pub const MAX_SKEW: u64 = 10;

/// The `typ` of a proof.
const TYP: &str = "dpop+jwt";

/// The only signing algorithm.
pub const ALG: &str = "ES256";

/// The private key of a device.
#[derive(Clone)]
pub struct Key(SigningKey);

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never print the private key.
        write!(f, "Key({})", self.thumbprint())
    }
}

impl Key {
    /// Makes a new key from OS random bytes.
    pub fn generate() -> Self {
        loop {
            let mut bytes = [0u8; 32];
            // The OS random source fails only when the OS is broken.
            getrandom::fill(&mut bytes).expect("the OS gives random bytes");
            // Nearly each 32 bytes make a valid scalar; try again if not.
            if let Ok(key) = SigningKey::from_slice(&bytes) {
                return Key(key);
            }
        }
    }

    /// Reads a key from [`Key::to_secret`].
    pub fn from_secret(secret: &str) -> Result<Self, DpopError> {
        let bytes = B64
            .decode(secret)
            .map_err(|_| DpopError::new("the key is not base64url"))?;
        SigningKey::from_slice(&bytes)
            .map(Key)
            .map_err(|_| DpopError::new("the key is not a P-256 key"))
    }

    /// The private key as base64url. Keep it only in the OS keyring.
    pub fn to_secret(&self) -> String {
        B64.encode(self.0.to_bytes())
    }

    /// The public key as a JWK.
    pub fn jwk(&self) -> Jwk {
        let point = self.0.verifying_key().to_sec1_point(false);
        let coordinate = |c: Option<&_>| B64.encode(c.map_or(&[][..], |c: &p256::FieldBytes| c));
        Jwk {
            kty: "EC".into(),
            crv: "P-256".into(),
            x: coordinate(point.x()),
            y: coordinate(point.y()),
        }
    }

    /// The JWK thumbprint of the public key (RFC 7638).
    pub fn thumbprint(&self) -> String {
        self.jwk().thumbprint()
    }

    /// A proof for one request. `url` has no query. `now` is in seconds
    /// since the Unix epoch.
    pub fn proof(&self, method: &str, url: &str, access_token: Option<&str>, now: u64) -> String {
        let header = Header {
            typ: TYP.into(),
            alg: ALG.into(),
            jwk: self.jwk(),
        };
        let mut jti = [0u8; 16];
        getrandom::fill(&mut jti).expect("the OS gives random bytes");
        let claims = Claims {
            jti: B64.encode(jti),
            htm: method.into(),
            htu: url.into(),
            iat: now,
            ath: access_token.map(token_hash),
        };
        let signing_input = format!("{}.{}", json_b64(&header), json_b64(&claims));
        format!("{signing_input}.{}", self.sign(signing_input.as_bytes()))
    }

    /// The ES256 signature of `input`, as base64url.
    pub(crate) fn sign(&self, input: &[u8]) -> String {
        let signature: Signature = self.0.sign(input);
        B64.encode(signature.to_bytes())
    }
}

/// A public P-256 key as a JWK.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Jwk {
    pub kty: String,
    pub crv: String,
    pub x: String,
    pub y: String,
}

impl Jwk {
    /// The JWK thumbprint (RFC 7638): base64url of the SHA-256 hash of
    /// the required members, in order, with no spaces.
    pub fn thumbprint(&self) -> String {
        let canonical = format!(
            r#"{{"crv":"{}","kty":"{}","x":"{}","y":"{}"}}"#,
            self.crv, self.kty, self.x, self.y
        );
        B64.encode(Sha256::digest(canonical.as_bytes()))
    }

    pub(crate) fn verifying_key(&self) -> Result<VerifyingKey, DpopError> {
        if self.kty != "EC" || self.crv != "P-256" {
            return Err(DpopError::new("the jwk is not a P-256 key"));
        }
        let x = B64.decode(&self.x).ok();
        let y = B64.decode(&self.y).ok();
        let (Some(x), Some(y)) = (x, y) else {
            return Err(DpopError::new("the jwk coordinates are not base64url"));
        };
        if x.len() != 32 || y.len() != 32 {
            return Err(DpopError::new("the jwk coordinates are not 32 bytes"));
        }
        let mut sec1 = vec![4u8];
        sec1.extend(x);
        sec1.extend(y);
        VerifyingKey::from_sec1_bytes(&sec1)
            .map_err(|_| DpopError::new("the jwk is not a point on P-256"))
    }
}

/// What a valid proof tells the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    /// The thumbprint of the key that signed the proof.
    pub jkt: String,
    /// The unique ID of the proof.
    pub jti: String,
    /// When the proof was made, in seconds since the Unix epoch.
    pub iat: u64,
}

/// Why a proof is not valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DpopError(String);

impl DpopError {
    fn new(message: &str) -> Self {
        DpopError(message.to_owned())
    }
}

impl fmt::Display for DpopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DpopError {}

/// Checks a proof for one request. See the module docs for the rules.
/// The caller checks that the `jti` is new.
pub fn verify(
    proof: &str,
    method: &str,
    url: &str,
    access_token: Option<&str>,
    now: u64,
) -> Result<Proof, DpopError> {
    let mut parts = proof.split('.');
    let (Some(header), Some(claims), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(DpopError::new("the proof is not a compact JWT"));
    };
    let header: serde_json::Value = json_part(header)?;
    // RFC 9449 section 4.3 (R113).
    if header.pointer("/jwk/d").is_some() {
        return Err(DpopError::new("the proof jwk holds a private key"));
    }
    let header: Header = serde_json::from_value(header)
        .map_err(|_| DpopError::new("a proof part is not valid JSON"))?;
    if header.typ != TYP {
        return Err(DpopError::new("the proof typ is not dpop+jwt"));
    }
    if header.alg != ALG {
        return Err(DpopError::new("the proof alg is not ES256"));
    }
    let key = header.jwk.verifying_key()?;
    let signature = B64
        .decode(signature)
        .ok()
        .and_then(|bytes| Signature::from_slice(&bytes).ok())
        .ok_or_else(|| DpopError::new("the proof signature is malformed"))?;
    let signing_input = &proof[..proof.rfind('.').unwrap_or(0)];
    key.verify(signing_input.as_bytes(), &signature)
        .map_err(|_| DpopError::new("the proof signature is not valid"))?;

    let claims: Claims = json_part(claims)?;
    if claims.htm != method {
        return Err(DpopError::new("the proof is for another method"));
    }
    if claims.htu != url {
        return Err(DpopError::new("the proof is for another URL"));
    }
    if now.saturating_sub(claims.iat) > MAX_AGE {
        return Err(DpopError::new("the proof is too old"));
    }
    if claims.iat > now + MAX_SKEW {
        return Err(DpopError::new("the proof is from the future"));
    }
    if claims.jti.is_empty() || claims.jti.len() > 64 {
        return Err(DpopError::new("the proof jti is empty or too long"));
    }
    if let Some(token) = access_token
        && claims.ath.as_deref() != Some(&token_hash(token))
    {
        return Err(DpopError::new("the proof is for another access token"));
    }
    Ok(Proof {
        jkt: header.jwk.thumbprint(),
        jti: claims.jti,
        iat: claims.iat,
    })
}

/// The `ath` of an access token: base64url of its SHA-256 hash.
pub fn token_hash(token: &str) -> String {
    B64.encode(Sha256::digest(token.as_bytes()))
}

#[derive(Serialize, Deserialize)]
struct Header {
    typ: String,
    alg: String,
    jwk: Jwk,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    jti: String,
    htm: String,
    htu: String,
    iat: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ath: Option<String>,
}

pub(crate) fn json_b64<T: Serialize>(value: &T) -> String {
    B64.encode(serde_json::to_vec(value).expect("the proof parts serialize"))
}

fn json_part<T: for<'de> Deserialize<'de>>(part: &str) -> Result<T, DpopError> {
    let bytes = B64
        .decode(part)
        .map_err(|_| DpopError::new("a proof part is not base64url"))?;
    serde_json::from_slice(&bytes).map_err(|_| DpopError::new("a proof part is not valid JSON"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://riff.example.com/v1/who";

    #[test]
    fn a_key_survives_the_keyring_format() {
        let key = Key::generate();
        let again = Key::from_secret(&key.to_secret()).unwrap();
        assert_eq!(again.thumbprint(), key.thumbprint());
        assert_ne!(Key::generate().thumbprint(), key.thumbprint());
        assert!(Key::from_secret("nope!").is_err());
    }

    #[test]
    fn debug_hides_the_private_key() {
        let key = Key::generate();
        assert!(!format!("{key:?}").contains(&key.to_secret()));
    }

    #[test]
    fn thumbprint_matches_rfc_7638_members() {
        let jwk = Jwk {
            kty: "EC".into(),
            crv: "P-256".into(),
            x: "a".into(),
            y: "b".into(),
        };
        let expected = B64.encode(Sha256::digest(
            br#"{"crv":"P-256","kty":"EC","x":"a","y":"b"}"#,
        ));
        assert_eq!(jwk.thumbprint(), expected);
    }

    #[test]
    fn a_good_proof_names_its_key() {
        let key = Key::generate();
        let proof = key.proof("POST", URL, None, 500);
        let checked = verify(&proof, "POST", URL, None, 500).unwrap();
        assert_eq!(checked.jkt, key.thumbprint());
        assert_eq!(checked.iat, 500);
        assert!(!checked.jti.is_empty());
        let other = verify(&key.proof("POST", URL, None, 500), "POST", URL, None, 500).unwrap();
        assert_ne!(other.jti, checked.jti);
    }

    #[test]
    fn the_method_must_match() {
        let proof = Key::generate().proof("POST", URL, None, 500);
        assert!(verify(&proof, "GET", URL, None, 500).is_err());
    }

    #[test]
    fn old_and_future_proofs_are_refused() {
        let proof = Key::generate().proof("POST", URL, None, 1_000);
        assert!(verify(&proof, "POST", URL, None, 1_000 + MAX_AGE).is_ok());
        assert!(verify(&proof, "POST", URL, None, 1_000 + MAX_AGE + 1).is_err());
        assert!(verify(&proof, "POST", URL, None, 1_000 - MAX_SKEW).is_ok());
        assert!(verify(&proof, "POST", URL, None, 1_000 - MAX_SKEW - 1).is_err());
    }

    #[test]
    fn a_token_needs_its_hash_in_the_proof() {
        let key = Key::generate();
        let without = key.proof("POST", URL, None, 500);
        assert!(verify(&without, "POST", URL, Some("t"), 500).is_err());
        let with = key.proof("POST", URL, Some("t"), 500);
        assert!(verify(&with, "POST", URL, Some("t"), 500).is_ok());
    }

    #[test]
    fn a_changed_proof_is_refused() {
        let key = Key::generate();
        let proof = key.proof("POST", URL, None, 500);
        let (input, _) = proof.rsplit_once('.').unwrap();
        // The signature of another key over the same input.
        let forged: Signature = Key::generate().0.sign(input.as_bytes());
        let forged = format!("{input}.{}", B64.encode(forged.to_bytes()));
        assert_eq!(
            verify(&forged, "POST", URL, None, 500)
                .unwrap_err()
                .to_string(),
            "the proof signature is not valid"
        );
        // Claims from another proof under the first signature.
        let other = key.proof("GET", URL, None, 500);
        let mut parts: Vec<&str> = proof.split('.').collect();
        parts[1] = other.split('.').nth(1).unwrap();
        assert!(verify(&parts.join("."), "GET", URL, None, 500).is_err());
    }

    #[test]
    fn malformed_proofs_are_refused() {
        for bad in ["", "a.b", "a.b.c.d", "!!.!!.!!"] {
            assert!(verify(bad, "POST", URL, None, 500).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_private_jwk_is_refused() {
        let key = Key::generate();
        let mut header = serde_json::to_value(Header {
            typ: TYP.into(),
            alg: ALG.into(),
            jwk: key.jwk(),
        })
        .unwrap();
        header["jwk"]["d"] = "private".into();
        let claims = key.proof("POST", URL, None, 500);
        let claims = claims.split('.').nth(1).unwrap();
        let input = format!("{}.{claims}", json_b64(&header));
        let signature: Signature = key.0.sign(input.as_bytes());
        let proof = format!("{input}.{}", B64.encode(signature.to_bytes()));
        assert_eq!(
            verify(&proof, "POST", URL, None, 500)
                .unwrap_err()
                .to_string(),
            "the proof jwk holds a private key"
        );
    }

    #[test]
    fn a_huge_iat_is_refused_without_a_panic() {
        let key = Key::generate();
        let proof = key.proof("POST", URL, None, u64::MAX);
        assert_eq!(
            verify(&proof, "POST", URL, None, 500)
                .unwrap_err()
                .to_string(),
            "the proof is from the future"
        );
    }

    #[test]
    fn other_algorithms_are_refused() {
        let key = Key::generate();
        let header = Header {
            typ: TYP.into(),
            alg: "none".into(),
            jwk: key.jwk(),
        };
        let proof = key.proof("POST", URL, None, 500);
        let rest = proof.split_once('.').unwrap().1;
        let none = format!("{}.{rest}", json_b64(&header));
        assert_eq!(
            verify(&none, "POST", URL, None, 500)
                .unwrap_err()
                .to_string(),
            "the proof alg is not ES256"
        );
    }
}
